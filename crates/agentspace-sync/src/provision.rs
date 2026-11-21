//! `operator provision -Host <project>` (docs/operator.md, provision; docs/state.md, A host's
//! folder; D46, D48, D52, D54, D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The steps MUST run in operator.md's order**: the record found (and the project list read
//!   only for a project with no record), the log opened, the run lock taken without waiting, the
//!   start recorded, then deploy, start and wait, `sync`, `bootstrap`, the record made `ready`,
//!   `place_pending`, and the sign-in line. The first failure among steps 4 to 7 ends the run.
//! - **A run MUST NOT take a `ready` record out of `ready`** (D48; P3): on such a record step 3
//!   writes `last_attempt_at` alone, a failure writes nothing, and step 8 writes only an
//!   `instance_id` that differs. `creating` and `failed` are for a host that was never `ready`.
//! - **Every change to the registry is made under `hosts.lock`**, never held across an AWS call
//!   or an ssh.
//! - **Nothing is written for a project that has no record and is not listed**: no log, no host's
//!   folder, no record.
//! - **The log's lines are the run's own**, one per step, each landing whole at the end of the
//!   file; a line that cannot be written after 5 tries 50 ms apart is dropped and changes nothing.
//!   A reason MUST NOT hold a secret, which no failure message holds.
//! - **A run MUST NOT read its stdin**, and nothing it writes to its stdout or stderr can fail it.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use agentspace_ca::CaEnv;
use agentspace_driver_ec2::PendingPlaced;
use agentspace_hosts::{BOX_PROJECT, Host, HostState, RunLock, Target};
use agentspace_operator_api::{Aws, Context, Failure, Remote, Say};

use crate::deploy::{self, full_home, read_hosts};
use crate::{Seams, step_failed};

/// How many times a log line is tried, and the pause between tries (keeper.md, Log lines).
const LOG_TRIES: u32 = 5;
const LOG_PAUSE: Duration = Duration::from_millis(50);

/// A variable of the run's environment, as the process's environment gives it: `AGENTSPACE_STATE_DIR`,
/// `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND`, which step 9's driver context is built from.
pub(crate) type Env<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// The run's log, `hosts\<project>\provision.log`, open for append, sharing read, write and delete.
struct Log {
    file: File,
}

impl Log {
    /// Writes `<time> <step> <outcome>[ reason=<reason>]` and one LF, in one write, retried.
    fn line(&self, step: &str, outcome: &str, reason: Option<&str>) {
        self.line_with(step, outcome, None, reason);
    }

    /// [`Log::line`], with `field` (one `name=value` with no space) after the outcome.
    fn line_with(&self, step: &str, outcome: &str, field: Option<&str>, reason: Option<&str>) {
        let time = agentspace_state::format_time(agentspace_state::now());
        let mut text = format!("{time} {step} {outcome}");
        if let Some(field) = field {
            text.push(' ');
            text.push_str(field);
        }
        if let Some(reason) = reason {
            text.push_str(" reason=");
            text.push_str(&log_reason(reason));
        }
        text.push('\n');
        for attempt in 0..LOG_TRIES {
            if (&self.file).write_all(text.as_bytes()).is_ok() {
                return;
            }
            if attempt + 1 < LOG_TRIES {
                std::thread::sleep(LOG_PAUSE);
            }
        }
    }

    fn failed(&self, step: &str, failure: &Failure) {
        self.line(step, "failed", Some(&failure.message));
    }
}

/// A failure message as a log line's reason: without its `agentspace: ` prefix, each control
/// character written `?`.
pub(crate) fn log_reason(message: &str) -> String {
    message
        .strip_prefix("agentspace: ")
        .unwrap_or(message)
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// A value as a log line's field: each control character and space written `?`, so the field
/// stays one word.
fn log_field(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() || c.is_whitespace() {
                '?'
            } else {
                c
            }
        })
        .collect()
}

/// Opens the log for append, creating the host's folder and the file when missing.
fn open_log(path: &Path) -> Result<Log, Failure> {
    let open = || -> std::io::Result<File> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        // The standard library opens a file sharing read, write and delete, as the log's rules
        // ask; append mode lands each write whole at the end of the file.
        OpenOptions::new().append(true).create(true).open(path)
    };
    open().map(|file| Log { file }).map_err(|e| {
        step_failed(&format!(
            "agentspace: Cannot open the provisioning log {}: {}.",
            path.display(),
            error_text(&e)
        ))
    })
}

/// The system's error text, with no final period and no line break.
fn error_text(e: &dyn std::fmt::Display) -> String {
    let text = e.to_string();
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    text.trim().trim_end_matches('.').to_string()
}

/// Step 2's lock, taken without waiting; a held lock and any other failure are each logged as
/// `run failed`.
fn take_run_lock(home: &Path, project: &str, log: &Log) -> Result<RunLock, Failure> {
    let failure = match agentspace_hosts::try_run_lock(home, project) {
        Ok(Some(lock)) => return Ok(lock),
        Ok(None) => step_failed(&format!(
            "agentspace: A provisioning run for '{project}' is already running."
        )),
        Err(e) => step_failed(&format!(
            "agentspace: Cannot take {}: {}.",
            agentspace_hosts::host_run_lock_path(home, project).display(),
            error_text(&e)
        )),
    };
    log.failed("run", &failure);
    Err(failure)
}

/// Step 3: the run's start, under `hosts.lock`, re-reading the registry. Returns whether the
/// record was `ready` as the run began.
fn record_start(home: &Path, project: &str, now: &str) -> Result<bool, Failure> {
    let held = crate::registry::lock(home)?;
    let mut hosts = read_hosts(home)?;
    let was_ready = match hosts.hosts.iter_mut().find(|h| h.project == project) {
        None => {
            hosts.hosts.push(Host {
                project: project.to_string(),
                stack: agentspace_hosts::stack_name(project),
                state: HostState::Creating,
                instance_id: None,
                last_attempt_at: Some(now.to_string()),
            });
            false
        }
        Some(record) => {
            record.last_attempt_at = Some(now.to_string());
            if record.state == HostState::Ready {
                true
            } else {
                record.state = HostState::Creating;
                false
            }
        }
    };
    deploy::write_hosts(home, &hosts, &held)?;
    Ok(was_ready)
}

/// The failure of step 4, 5, 6 or 7: logged, and recorded `failed` for a host that was never
/// `ready`, with its `instance_id` kept. A `ready` record is not written at all.
fn record_failure(home: &Path, project: &str, was_ready: bool, log: &Log) {
    if was_ready {
        return;
    }
    let result = deploy::change_record(home, project, |record| {
        // A record that another writer made `ready` meanwhile is never taken out of it.
        if record.state == HostState::Ready || record.state == HostState::Failed {
            return false;
        }
        record.state = HostState::Failed;
        true
    });
    if let Err(f) = result {
        log.failed("record", &f);
    }
}

/// Step 8: the host recorded `ready`, with the instance ID step 4 read.
fn record_ready(home: &Path, project: &str, instance_id: &str) -> Result<(), Failure> {
    deploy::change_record(home, project, |record| {
        let mut changed = false;
        if record.state != HostState::Ready {
            record.state = HostState::Ready;
            changed = true;
        }
        if record.instance_id.as_deref() != Some(instance_id) {
            record.instance_id = Some(instance_id.to_string());
            changed = true;
        }
        changed
    })
}

/// The EC2 driver's `Context` for step 9, built from the run's environment as the lease CLI
/// builds its own (operator.md, Environment variables; P12): `USERPROFILE` is the operator's
/// home, and the state folder, the ssh folder and the ProxyCommand line come from
/// `AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND` when set and not
/// empty, each folder made full against the working folder.
pub(crate) fn driver_context(
    ctx: &Context,
    env: Env<'_>,
) -> Result<agentspace_driver_api::Context, Failure> {
    let home = full_home(ctx);
    let home_value = home.clone().into_os_string();
    let var = |name: &str| -> Option<OsString> {
        match name {
            "USERPROFILE" => Some(home_value.clone()),
            "AGENTSPACE_STATE_DIR" | "AGENTSPACE_SSH_DIR" => env(name),
            _ => None,
        }
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| home.clone());
    let paths = agentspace_state::resolve_paths(&var, &cwd)
        .map_err(|_| step_failed("agentspace: The state and ssh folders cannot be found."))?;
    let state_dir = paths.state_dir.unwrap_or_else(|| home.join(".agentspace"));
    let ssh_dir = paths.ssh_dir.unwrap_or_else(|| home.join(".ssh"));
    // Each is the variable made full when the run had it set and not empty, else `None`
    // (driver.md, The detached start, C7), as the lease CLI fills its own.
    let is_set = |name: &str| env(name).is_some_and(|v| !v.is_empty());
    let state_dir_env = is_set("AGENTSPACE_STATE_DIR").then(|| state_dir.clone());
    let ssh_dir_env = is_set("AGENTSPACE_SSH_DIR").then(|| ssh_dir.clone());
    let proxy_override =
        match env("AGENTSPACE_PROXY_COMMAND").filter(|v| !v.is_empty()) {
            None => None,
            Some(value) => Some(value.into_string().map_err(|_| {
                step_failed("agentspace: AGENTSPACE_PROXY_COMMAND is not valid text.")
            })?),
        };
    let system_root = Some(ctx.system_root.display().to_string()).filter(|s| !s.is_empty());
    let own_exe = std::env::current_exe().map_err(|e| {
        step_failed(&format!(
            "agentspace: The operator's own program cannot be found: {}.",
            error_text(&e)
        ))
    })?;
    Ok(agentspace_driver_api::Context {
        home,
        ssh_dir,
        proxy_override,
        system_root,
        own_exe,
        state_dir,
        // place_pending starts no provisioning run, so it needs no program for one.
        operator_program: None,
        state_dir_env,
        ssh_dir_env,
    })
}

/// The pinned name of the driver's step, the only one this crate calls. `Err` is a failure of the
/// whole call; one lease's failure is in the result's `failed`.
fn place_pending(
    driver_ctx: &agentspace_driver_api::Context,
    project: &str,
) -> Result<PendingPlaced, Failure> {
    agentspace_driver_ec2::place_pending(driver_ctx, project).map_err(|f| Failure {
        message: format!("agentspace: {}", f.message),
        caller: agentspace_operator_api::CallerFailure::Same,
    })
}

/// Step 9's lines for a call that returned: `place done placed=<n>`, then one
/// `place failed lease=<id> reason=<message>` per lease left `pending`, in the order it was tried.
// QUESTION: operator.md's line form has no field for the count or a lease's ID, and the T03 ruling
// says to log `placed` and one line per failed lease (its ID and message). Interim: the count as
// `placed=<n>` after `done`, and each lease as `lease=<id>` before its `reason`, the reason last.
fn log_placed(log: &Log, placed: &PendingPlaced) {
    log.line_with(
        "place",
        "done",
        Some(&format!("placed={}", placed.placed)),
        None,
    );
    for lease in &placed.failed {
        log.line_with(
            "place",
            "failed",
            Some(&format!("lease={}", log_field(&lease.lease_id))),
            Some(&lease.message),
        );
    }
}

/// The run, steps 1 to 10.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    ctx: &Context,
    ca_env: &CaEnv,
    project: &str,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
    env: Env<'_>,
) -> Result<(), Failure> {
    let home = full_home(ctx);
    // The operator gives a normalized project; anything else makes no path (state.md).
    if !agentspace_hosts::is_normalized_project(project) {
        return Err(step_failed(&format!(
            "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not '{}'.",
            log_reason(project)
        )));
    }

    // Step 1: the record, and the project list only for a project with no record.
    let hosts = read_hosts(&home)?;
    if agentspace_hosts::host_for(&hosts, project).is_none() {
        let list_path = agentspace_hosts::projects_path(&home);
        let projects = agentspace_hosts::read_projects(&home).map_err(|e| {
            step_failed(&format!(
                "agentspace: The project list {} cannot be read: {}.",
                list_path.display(),
                error_text(&e)
                    .trim_start_matches("The file is not valid: ")
                    .trim_start_matches("The file could not be read: ")
            ))
        })?;
        if !agentspace_hosts::is_listed(&projects, project) {
            return Err(step_failed(&format!(
                "agentspace: Project '{project}' has no host and is not in {}, so no host is made for it.",
                list_path.display()
            )));
        }
    }

    // Step 2: the log, then the lock.
    let log = open_log(&agentspace_hosts::host_log_path(&home, project))?;
    let _run_lock = take_run_lock(&home, project, &log)?;

    // Step 3.
    let now = agentspace_state::format_time(agentspace_state::now());
    let was_ready = record_start(&home, project, &now).inspect_err(|f| log.failed("run", f))?;
    log.line("run", "begun", None);
    say.progress(&format!("Provisioning the host of '{project}'..."), None);

    let is_box = project == BOX_PROJECT;
    let target = if is_box {
        Target::Box
    } else {
        Target::Host(project)
    };
    let fail = |step: &str, f: Failure| -> Failure {
        log.failed(step, &f);
        record_failure(&home, project, was_ready, &log);
        f
    };

    // Step 4: deploy's existence check and steps 1 to 5, never with -InstanceType or sync.
    let deployed = if is_box {
        deploy::box_stack(ctx, say, aws, seams)
    } else {
        deploy::host_stack(ctx, ca_env, project, None, say, aws, seams)
    };
    let instance_id = deployed.map_err(|f| fail("deploy", f))?;
    log.line("deploy", "done", None);

    // Step 5: start and wait, with no push.
    agentspace_box::start_and_wait_for(aws, say, seams.clock, &instance_id)
        .map_err(|f| fail("start", f))?;
    log.line("start", "done", None);

    // Step 6: sync, its forced push included.
    crate::sync_target_using(ctx, ca_env, target, say, aws, remote, seams)
        .map_err(|f| fail("sync", f))?;
    log.line("sync", "done", None);

    // Step 7: bootstrap.
    crate::bootstrap_using(ctx, ca_env, target, say, aws, remote, seams)
        .map_err(|f| fail("bootstrap", f))?;
    log.line("bootstrap", "done", None);

    // Step 8.
    record_ready(&home, project, &instance_id).inspect_err(|f| log.failed("record", f))?;
    log.line("record", "done", None);
    say.progress(&format!("Recorded the host of '{project}' as ready."), None);

    // Step 9: a failure leaves the host ready and the leases pending, and the run exits 0.
    match driver_context(ctx, env).and_then(|d| place_pending(&d, project)) {
        Ok(placed) => log_placed(&log, &placed),
        Err(f) => {
            log.failed("place", &f);
            say.warning(&format!(
                "agentspace: The pending leases of '{project}' could not be placed: {}; the next run places them.",
                log_reason(&f.message).trim_end_matches('.')
            ));
        }
    }

    // Step 10: the sign-in line, only for a host that was not ready, and never for the box.
    if !was_ready && !is_box {
        say.progress(
            &format!(
                "Sign in on this host once, by hand: run 'agentspace-operator status -Host {project}' first and again after an hour, then see \"Signing in on a new host\" in the agentspace README."
            ),
            None,
        );
    }
    log.line("run", "done", None);
    Ok(())
}

/// A path's text, for the tests.
#[cfg(test)]
fn shown(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Scratch;

    #[test]
    fn a_reason_loses_its_prefix_and_control_characters() {
        assert_eq!(
            log_reason("agentspace: Stack 'x' is A\nB."),
            "Stack 'x' is A?B."
        );
        assert_eq!(log_reason("No prefix\t."), "No prefix?.");
    }

    #[test]
    fn a_log_line_has_the_time_the_step_and_the_reason() {
        let dir = Scratch::new("provision-log");
        let path = dir.path().join("hosts").join("p").join("provision.log");
        let log = open_log(&path).unwrap();
        log.line("run", "begun", None);
        log.failed(
            "deploy",
            &step_failed("agentspace: Stack deploy failed; see the stack events in the console."),
        );
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines.len(), 2);
        assert!(text.ends_with('\n') && !text.contains('\r'));
        let (time, rest) = lines[0].split_once(' ').unwrap();
        assert!(agentspace_state::parse_time(time).is_some());
        assert_eq!(rest, "run begun");
        assert!(lines[1].ends_with(
            " deploy failed reason=Stack deploy failed; see the stack events in the console."
        ));
        // A second open appends.
        open_log(&path).unwrap().line("run", "done", None);
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 3);
    }

    #[test]
    fn a_folder_at_the_logs_path_fails_with_its_name() {
        let dir = Scratch::new("provision-log-folder");
        let path = dir.path().join("provision.log");
        std::fs::create_dir_all(&path).unwrap();
        let message = open_log(&path).err().unwrap().message;
        assert!(message.starts_with(&format!(
            "agentspace: Cannot open the provisioning log {}: ",
            shown(&path)
        )));
        assert!(message.ends_with('.') && !message.ends_with(".."));
    }

    #[test]
    fn the_driver_context_takes_the_runs_variables_and_the_operators_home() {
        let dir = Scratch::new("provision-driver-ctx");
        let ctx = crate::tests::ctx_in(dir.path());
        let none = |_: &str| None;
        let d = driver_context(&ctx, &none).unwrap();
        let home = full_home(&ctx);
        assert_eq!(d.home, home);
        assert_eq!(d.state_dir, home.join(".agentspace"));
        assert_eq!(d.ssh_dir, home.join(".ssh"));
        assert_eq!(d.proxy_override, None);
        assert_eq!(d.operator_program, None);
        assert_eq!(d.state_dir_env, None);
        assert_eq!(d.ssh_dir_env, None);
        // Set but empty is the same as unset.
        let empty = |name: &str| match name {
            "AGENTSPACE_STATE_DIR" | "AGENTSPACE_SSH_DIR" => Some(OsString::new()),
            _ => None,
        };
        let d = driver_context(&ctx, &empty).unwrap();
        assert_eq!((d.state_dir_env, d.ssh_dir_env), (None, None));
        assert_eq!(d.state_dir, home.join(".agentspace"));
        let state = dir.path().join("state");
        let ssh = dir.path().join("ssh");
        let (s, h) = (state.clone(), ssh.clone());
        let set = move |name: &str| match name {
            "AGENTSPACE_STATE_DIR" => Some(s.clone().into_os_string()),
            "AGENTSPACE_SSH_DIR" => Some(h.clone().into_os_string()),
            "AGENTSPACE_PROXY_COMMAND" => Some(OsString::from("proxy %h")),
            "USERPROFILE" => Some(OsString::from("elsewhere")),
            _ => None,
        };
        let d = driver_context(&ctx, &set).unwrap();
        assert_eq!(d.home, home);
        assert_eq!(d.state_dir, state);
        assert_eq!(d.ssh_dir, ssh);
        assert_eq!(d.proxy_override.as_deref(), Some("proxy %h"));
        // Each variable the run had is passed on, made full, so the driver's detached start passes
        // it on if and only if the run had it (driver.md, C7).
        assert_eq!(d.state_dir_env.as_deref(), Some(state.as_path()));
        assert_eq!(d.ssh_dir_env.as_deref(), Some(ssh.as_path()));
        // A relative value is made full against the working folder.
        let relative = |name: &str| match name {
            "AGENTSPACE_STATE_DIR" => Some(OsString::from("rel-state")),
            _ => None,
        };
        let d = driver_context(&ctx, &relative).unwrap();
        let made = d.state_dir_env.expect("the state folder variable was set");
        assert!(made.is_absolute() && made.ends_with("rel-state"));
        assert_eq!(made, d.state_dir);
        assert_eq!(d.ssh_dir_env, None);
    }

    #[test]
    fn place_pendings_result_is_logged_as_a_count_then_one_line_per_lease_it_left() {
        let dir = Scratch::new("provision-placed");
        let path = dir.path().join("provision.log");
        let log = open_log(&path).unwrap();
        log_placed(
            &log,
            &PendingPlaced {
                placed: 2,
                failed: vec![
                    agentspace_driver_ec2::PendingFailure {
                        lease_id: "8d41b6a2-0c3e-4f57-a1d9-2e6b7c8f9a01".to_string(),
                        message: "agentspace: The CA failed its check.\nThe lease stays pending."
                            .to_string(),
                    },
                    agentspace_driver_ec2::PendingFailure {
                        lease_id: "1f2e3d4c-5b6a-4978-8a1b-2c3d4e5f6a7b".to_string(),
                        message: "The alias file is not agentspace's.".to_string(),
                    },
                ],
            },
        );
        log_placed(&log, &PendingPlaced::default());
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        let rests: Vec<&str> = text.lines().map(|l| l.split_once(' ').unwrap().1).collect();
        assert_eq!(
            rests,
            [
                "place done placed=2",
                "place failed lease=8d41b6a2-0c3e-4f57-a1d9-2e6b7c8f9a01 reason=The CA failed its check.?The lease stays pending.",
                "place failed lease=1f2e3d4c-5b6a-4978-8a1b-2c3d4e5f6a7b reason=The alias file is not agentspace's.",
                "place done placed=0",
            ]
        );
        assert_eq!(log_field("a b\tc"), "a?b?c");
    }

    #[test]
    fn the_start_and_the_outcome_are_recorded_by_the_ready_host_rule() {
        let dir = Scratch::new("provision-records");
        let home = dir.path().to_path_buf();
        // No record: added as creating, with the start's time.
        assert!(!record_start(&home, "example", "2026-11-21T10:00:00Z").unwrap());
        let hosts = read_hosts(&home).unwrap();
        let r = agentspace_hosts::host_for(&hosts, "example").unwrap();
        assert_eq!(r.state, HostState::Creating);
        assert_eq!(r.stack, "agentspace-host-example");
        assert_eq!(r.last_attempt_at.as_deref(), Some("2026-11-21T10:00:00Z"));
        // A failure records failed, and a new start makes it creating again.
        let log = open_log(&home.join("log")).unwrap();
        record_failure(&home, "example", false, &log);
        let hosts = read_hosts(&home).unwrap();
        assert_eq!(
            agentspace_hosts::host_for(&hosts, "example").unwrap().state,
            HostState::Failed
        );
        assert!(!record_start(&home, "example", "2026-11-21T10:10:00Z").unwrap());
        record_ready(&home, "example", "i-1234567890abcdef0").unwrap();
        let hosts = read_hosts(&home).unwrap();
        let r = agentspace_hosts::host_for(&hosts, "example").unwrap();
        assert_eq!(r.state, HostState::Ready);
        assert_eq!(r.instance_id.as_deref(), Some("i-1234567890abcdef0"));
        assert_eq!(r.last_attempt_at.as_deref(), Some("2026-11-21T10:10:00Z"));
        // A ready record: the start writes last_attempt_at alone, a failure writes nothing.
        assert!(record_start(&home, "example", "2026-11-21T11:00:00Z").unwrap());
        record_failure(&home, "example", true, &log);
        let hosts = read_hosts(&home).unwrap();
        let r = agentspace_hosts::host_for(&hosts, "example").unwrap();
        assert_eq!(r.state, HostState::Ready);
        assert_eq!(r.last_attempt_at.as_deref(), Some("2026-11-21T11:00:00Z"));
        // The box's record stays ready throughout.
        assert!(record_start(&home, "swimcoach", "2026-11-21T12:00:00Z").unwrap());
    }

    #[test]
    fn a_held_run_lock_logs_one_line_and_fails() {
        let dir = Scratch::new("provision-lock");
        let home = dir.path().to_path_buf();
        let held = agentspace_hosts::try_run_lock(&home, "example")
            .unwrap()
            .unwrap();
        let log = open_log(&agentspace_hosts::host_log_path(&home, "example")).unwrap();
        let f = take_run_lock(&home, "example", &log).unwrap_err();
        assert_eq!(
            f.message,
            "agentspace: A provisioning run for 'example' is already running."
        );
        drop(held);
        let text =
            std::fs::read_to_string(agentspace_hosts::host_log_path(&home, "example")).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.ends_with(
            " run failed reason=A provisioning run for 'example' is already running.\n"
        ));
        assert!(!agentspace_hosts::hosts_path(&home).exists());
    }
}

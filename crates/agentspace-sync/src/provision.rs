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
//!   writes `last_attempt_at` alone, removing `last_success_at`, a failure writes nothing, and step
//!   8 writes only an `instance_id` that differs. `creating` and `failed` are for a host that was
//!   never `ready`.
//! - **Step 3's write MUST remove the record's `last_success_at`** (D77; pin P24), and **the run
//!   MUST write `last_success_at` only at step 9's end, only when it succeeded**: `place_pending`
//!   returned without error and failed or left no lease (pin P22). It is the time the run ended,
//!   written under `hosts.lock` alone; a write that fails is logged `success failed` and changes
//!   nothing else (pin P23).
//! - **The deploy step MUST find its type in step 4's order** (D68, D71; pin P26): the lease state
//!   lock, then `hosts.lock` for the removal of the record's `instance_type` (whether a type is
//!   then passed or none), `hosts.lock` released, the lease state file read under the lease state
//!   lock still held, the type decided, and that lock released before any AWS call. **It passes
//!   no type while a live `ready` lease of the project holds the host**, and never maps a size to
//!   a type itself: the EC2 driver's `resize_target` does (D67).
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
use std::path::{Path, PathBuf};
use std::time::Duration;

use agentspace_ca::CaEnv;
use agentspace_driver_ec2::{PendingPlaced, Unplaced};
use agentspace_hosts::{BOX_PROJECT, Host, HostState, RunLock, Target};
use agentspace_operator_api::{Aws, Context, Failure, Remote, Say};

use crate::deploy::{self, full_home, read_hosts};
use crate::patch_wait::PatchOutcome;
use crate::{Seams, step_failed};

/// Step 10's line, the run's last, exactly (operator.md, provision, step 10; pin P20).
pub(crate) const SIGN_IN_LINE: &str = "Sign in on this host once, by hand, under a lease: see \"Signing in on a new host\" in the agentspace README.";

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

/// The system's error text, with no final period and no line break. Rust's trailing
/// ` (os error <n>)` MUST be dropped: a message carries the system's own words alone (operator.md,
/// Exit codes).
pub(crate) fn error_text(e: &dyn std::fmt::Display) -> String {
    let text = e.to_string();
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    without_os_error(text.trim())
        .trim()
        .trim_end_matches('.')
        .to_string()
}

/// `text` without one trailing ` (os error <digits>)`, the suffix Rust's `io::Error` adds to an OS
/// error's message. Anything else, a near miss included, is returned as it is.
fn without_os_error(text: &str) -> &str {
    const OPEN: &str = " (os error ";
    if let Some(body) = text.strip_suffix(')')
        && let Some(open) = body.rfind(OPEN)
    {
        let code = &body[open + OPEN.len()..];
        let digits = code.strip_prefix('-').unwrap_or(code);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return &text[..open];
        }
    }
    text
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

/// A person's `deploy` takes its host's `provision.lock` the way the run's step 2 does, without
/// waiting, and holds it for its whole life (operator.md, deploy; D67's change note; pins P34 and
/// P36). A lock another program holds refuses the `deploy` with
/// `agentspace: A provisioning run or deploy for '<project>' is already running; the deploy did not run.`;
/// one that cannot be taken for any other reason fails it with `agentspace: Cannot take <path>:
/// <error>.`. Neither writes a line to `provision.log`, which is the run's alone.
pub(crate) fn take_person_lock(home: &Path, project: &str) -> Result<RunLock, Failure> {
    match agentspace_hosts::try_run_lock(home, project) {
        Ok(Some(lock)) => Ok(lock),
        Ok(None) => Err(step_failed(&format!(
            "agentspace: A provisioning run or deploy for '{project}' is already running; the deploy did not run."
        ))),
        Err(e) => Err(step_failed(&format!(
            "agentspace: Cannot take {}: {}.",
            agentspace_hosts::host_run_lock_path(home, project).display(),
            error_text(&e)
        ))),
    }
}

/// The state folder the run's leases are in: `AGENTSPACE_STATE_DIR` when the run has it set and
/// not empty, made full, else `%USERPROFILE%\.agentspace` (operator.md, Environment variables),
/// the folder whose leases step 9 places.
fn state_dir_for(ctx: &Context, env: Env<'_>) -> Result<PathBuf, Failure> {
    let home = full_home(ctx);
    let home_value = home.clone().into_os_string();
    let var = |name: &str| -> Option<OsString> {
        match name {
            "USERPROFILE" => Some(home_value.clone()),
            "AGENTSPACE_STATE_DIR" => env(name),
            _ => None,
        }
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| home.clone());
    let paths = agentspace_state::resolve_paths(&var, &cwd)
        .map_err(|_| step_failed("agentspace: The state and ssh folders cannot be found."))?;
    Ok(paths.state_dir.unwrap_or_else(|| home.join(".agentspace")))
}

/// What a text of a lock's or a file's failure shows: no line break, no final period.
fn why_text(why: &str) -> String {
    error_text(&why)
}

/// Step 4.1: the lease state lock, waited for up to its 30 seconds as a lease command that changes
/// leases waits (state.md, The lease state lock). A lock not taken is
/// `agentspace: The lease state lock <path> is held by another program; try again.`, and one that
/// cannot be opened for any other reason `agentspace: Cannot take <path>: <error>.` (pin P35).
fn take_lease_lock(state_dir: &Path) -> Result<agentspace_state::LockGuard, Failure> {
    let path = state_dir.join("leases.lock");
    agentspace_state::lock(state_dir).map_err(|e| match e {
        agentspace_state::StoreError::Busy => step_failed(&format!(
            "agentspace: The lease state lock {} is held by another program; try again.",
            path.display()
        )),
        agentspace_state::StoreError::CannotLock(why)
        | agentspace_state::StoreError::Unreadable(why)
        | agentspace_state::StoreError::NotValid(why)
        | agentspace_state::StoreError::Unwritable(why) => step_failed(&format!(
            "agentspace: Cannot take {}: {}.",
            path.display(),
            why_text(&why)
        )),
    })
}

/// Step 4.3's reading: the lease state file, whole and read-only, under the lease state lock the
/// caller holds. A missing file holds no leases. A file that cannot be read or is not valid fails
/// with `agentspace: The lease state file <path> cannot be read: <reason>.`, as the guard writes it
/// (pin P26).
fn read_leases(state_dir: &Path) -> Result<Vec<agentspace_state::Record>, Failure> {
    agentspace_state::read(state_dir)
        .map(|file| file.leases)
        .map_err(|e| {
            let why = match &e {
                agentspace_state::StoreError::Unreadable(why)
                | agentspace_state::StoreError::NotValid(why)
                | agentspace_state::StoreError::CannotLock(why)
                | agentspace_state::StoreError::Unwritable(why) => why.replace(['\r', '\n'], " "),
                agentspace_state::StoreError::Busy => "the file is in use".to_string(),
            };
            step_failed(&format!(
                "agentspace: The lease state file {} cannot be read: {why}.",
                state_dir.join("leases.json").display()
            ))
        })
}

/// What the run's deploy step decides from the leases at one reading's time (step 4.3): **no type**
/// when the project holds a live `ready` lease, an unexpired stored `ready` record of the project,
/// so the host keeps its size while a lease holds it (D66, D68's change note; pin P2(a)); otherwise
/// the EC2 driver's `resize_target`, which the run never restates (D67).
pub(crate) fn decide_type(
    leases: &[agentspace_state::Record],
    project: &str,
    now: i64,
) -> Option<&'static str> {
    let held = leases
        .iter()
        .any(|l| l.project == project && l.state == "ready" && agentspace_state::is_live(l, now));
    if held {
        None
    } else {
        agentspace_driver_ec2::resize_target(leases, project)
    }
}

/// Step 4's first three steps, in the order of the locks (docs/state.md, The order of the locks;
/// operator.md, provision, step 4; pin P26): takes the lease state lock, then `hosts.lock` for the
/// removal of the record's `instance_type`, releases `hosts.lock`, reads the lease state file under
/// the lease state lock still held, decides the type, and releases that lock before any call to
/// AWS. **The type is removed whether a type is then passed or none**, so a run whose deploy step
/// fails or is killed leaves none (pin P30). Each failure comes before any call to AWS; the type
/// found, or none, is `Ok`'s.
pub(crate) fn find_type(
    ctx: &Context,
    env: Env<'_>,
    home: &Path,
    project: &str,
) -> Result<Option<&'static str>, Failure> {
    let state_dir = state_dir_for(ctx, env)?;
    // 4.1: the lease state lock, then (inside the removal) hosts.lock.
    let lease_lock = take_lease_lock(&state_dir)?;
    // 4.2: removes the type and writes the registry, then releases hosts.lock.
    crate::registry::set_instance_type(home, project, None)?;
    // 4.3: the leases, one reading's time.
    let leases = read_leases(&state_dir)?;
    let found = decide_type(&leases, project, agentspace_state::now());
    // 4.4: released before any call to AWS.
    drop(lease_lock);
    Ok(found)
}

/// Step 3: the run's start, under `hosts.lock`, re-reading the registry. Returns whether the
/// record was `ready` as the run began.
fn record_start(home: &Path, project: &str, now: &str) -> Result<bool, Failure> {
    let held = crate::registry::lock(home)?;
    let mut hosts = read_hosts(home)?;
    let was_ready = match hosts.hosts.iter_mut().find(|h| h.project == project) {
        None => {
            hosts.hosts.push(Host {
                last_success_at: None,
                project: project.to_string(),
                stack: agentspace_hosts::stack_name(project),
                state: HostState::Creating,
                instance_id: None,
                instance_type: None,
                last_attempt_at: Some(now.to_string()),
            });
            false
        }
        Some(record) => {
            record.last_attempt_at = Some(now.to_string());
            // The same write removes `last_success_at`, so a run that has begun never reads as
            // one that succeeded (D77; pin P24).
            record.last_success_at = None;
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

/// Step 9's lines for a call that returned: `place done placed=<n>`, then, in the order the call
/// tried the leases, one `place failed lease=<id> reason=<message>` per lease whose placement
/// failed, and from v0.09 one `place refused lease=<id>` per lease it answered `unsatisfiable` and
/// one `place left lease=<id>` per lease it left for the next run, neither with a reason. The form
/// is operator.md's, provision, "Its log" (architect, 2026-11-21, for v0.07-T03; pin P37).
fn log_placed(log: &Log, placed: &PendingPlaced) {
    log.line_with(
        "place",
        "done",
        Some(&format!("placed={}", placed.placed)),
        None,
    );
    for lease in &placed.unplaced {
        let id = format!("lease={}", log_field(lease.lease_id()));
        match lease {
            Unplaced::Failed(f) => log.line_with("place", "failed", Some(&id), Some(&f.message)),
            Unplaced::Refused { .. } => log.line_with("place", "refused", Some(&id), None),
            Unplaced::Left { .. } => log.line_with("place", "left", Some(&id), None),
        }
    }
}

/// Step 9 and its end, called by the run with `provision.lock` held: places the project's pending
/// leases and logs the result; a failure of the call as a whole leaves the host ready and the
/// leases pending, warns, and the run still exits 0. Then, when the run succeeded, it records the
/// success (D77; pins P22, P23 and P24), and otherwise writes nothing.
fn place_and_record(
    ctx: &Context,
    env: Env<'_>,
    home: &Path,
    project: &str,
    say: &dyn Say,
    log: &Log,
) {
    let placed = driver_context(ctx, env).and_then(|d| place_pending(&d, project));
    match &placed {
        Ok(placed) => log_placed(log, placed),
        Err(f) => {
            log.failed("place", f);
            say.warning(&format!(
                "agentspace: The pending leases of '{project}' could not be placed: {}; the next run places them.",
                log_reason(&f.message).trim_end_matches('.')
            ));
        }
    }
    if succeeded(&placed) {
        record_success(home, project, log);
    }
}

/// Whether the run succeeded, by step 9's result (D77's change note, pin P22): the call returned
/// without error and no lease was failed or left. A lease the call placed or answered
/// `unsatisfiable` (`place refused`) is not a failure.
fn succeeded(placed: &Result<PendingPlaced, Failure>) -> bool {
    placed.as_ref().is_ok_and(|p| {
        p.unplaced
            .iter()
            .all(|u| matches!(u, Unplaced::Refused { .. }))
    })
}

/// Step 9's end for a run that succeeded (D77; pins P23 and P24): `last_success_at` set to the
/// time the run ended, in UTC in state.md's timestamp form, under `hosts.lock` alone, and nothing
/// else. A write that fails is logged as `success failed` with the registry's message, and changes
/// nothing else: no warning, no change to the run's exit.
fn record_success(home: &Path, project: &str, log: &Log) {
    let ended = agentspace_state::format_time(agentspace_state::now());
    if let Err(f) = crate::registry::set_last_success_at(home, project, &ended) {
        log.failed("success", &f);
    }
}

/// Step 6's line for a `sync` that succeeded: `sync done`, or, when its patch wait ended on
/// `failed`, `sync done; the patch run failed, see the host's journal`, exactly, with no other
/// field (operator.md, provision, Its log; pin P27).
fn log_sync_done(log: &Log, outcome: PatchOutcome) {
    match outcome {
        PatchOutcome::Patched => log.line("sync", "done", None),
        PatchOutcome::Failed => log.line(
            "sync",
            "done; the patch run failed, see the host's journal",
            None,
        ),
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

    // Step 4: its type found in steps 4.1 to 4.4, then deploy's existence check and steps 1 to 5
    // (never its `sync`) with `-InstanceType` when a type was found, and none otherwise (D68, D71;
    // pin P26). A step 4 that failed before it found a type logs no `type` field (pin P24).
    let found = find_type(ctx, env, &home, project).map_err(|f| fail("deploy", f))?;
    let type_field = found.map(|t| format!("type={}", log_field(t)));
    let deployed = if is_box {
        deploy::box_stack(ctx, found, say, aws, seams)
    } else {
        deploy::host_stack(ctx, ca_env, project, found, say, aws, seams)
    };
    let instance_id = deployed.inspect_err(|f| {
        log.line_with("deploy", "failed", type_field.as_deref(), Some(&f.message));
        record_failure(&home, project, was_ready, &log);
    })?;
    log.line_with("deploy", "done", type_field.as_deref(), None);

    // Step 5: start and wait, with no push. A host's deadline hint names its own alias, never the
    // box's (operator.md, From the command line, "What a command takes from its host").
    agentspace_box::start_and_wait_for(aws, say, seams.clock, &instance_id)
        .map_err(|f| {
            if is_box {
                f
            } else {
                crate::syncing::hint_for(&agentspace_hosts::host_alias(project), f)
            }
        })
        .map_err(|f| fail("start", f))?;
    log.line("start", "done", None);

    // Step 6: sync, its patch wait and its forced push included. A wait that ended on `failed` is
    // recorded in the step's line, since a detached run's warning reaches no one (pin P27).
    let outcome = crate::syncing::sync_target(ctx, ca_env, target, say, aws, remote, seams)
        .map_err(|f| fail("sync", f))?;
    log_sync_done(&log, outcome);

    // Step 7: bootstrap.
    crate::bootstrap_using(ctx, ca_env, target, say, aws, remote, seams)
        .map_err(|f| fail("bootstrap", f))?;
    log.line("bootstrap", "done", None);

    // Step 8.
    record_ready(&home, project, &instance_id).inspect_err(|f| log.failed("record", f))?;
    log.line("record", "done", None);
    say.progress(&format!("Recorded the host of '{project}' as ready."), None);

    // Step 9, and the run's success when it succeeded.
    place_and_record(ctx, env, &home, project, say, &log);

    // Step 10: the sign-in line, only for a host that was not ready, and never for the box. From
    // v0.08 a host runs only while its project holds a live lease (D56), so the sign-ins are made
    // under one (pin P20).
    if !was_ready && !is_box {
        say.progress(SIGN_IN_LINE, None);
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
    fn step_6s_line_records_a_patch_run_that_failed() {
        let dir = Scratch::new("provision-sync-line");
        let path = dir.path().join("provision.log");
        let log = open_log(&path).unwrap();
        log_sync_done(&log, PatchOutcome::Patched);
        log_sync_done(&log, PatchOutcome::Failed);
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines.len(), 2);
        let rests: Vec<&str> = lines
            .iter()
            .map(|l| {
                let (time, rest) = l.split_once(' ').unwrap();
                assert!(agentspace_state::parse_time(time).is_some());
                rest
            })
            .collect();
        assert_eq!(
            rests,
            [
                "sync done",
                "sync done; the patch run failed, see the host's journal"
            ]
        );
    }

    #[test]
    fn the_sign_in_line_sends_a_person_to_a_lease_first() {
        assert_eq!(
            SIGN_IN_LINE,
            "Sign in on this host once, by hand, under a lease: see \"Signing in on a new host\" in the agentspace README."
        );
        assert!(!SIGN_IN_LINE.contains("status -Host"));
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
        // The system's own words alone: Rust's ` (os error <n>)` is dropped.
        assert!(
            !message.contains("os error"),
            "The message MUST NOT carry Rust's os error suffix"
        );
        assert_eq!(
            error_text(&"Access is denied. (os error 5)"),
            "Access is denied"
        );
        assert_eq!(
            error_text(&"A near miss (os error 12a)"),
            "A near miss (os error 12a)"
        );
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
                unplaced: vec![
                    Unplaced::Failed(agentspace_driver_ec2::PendingFailure {
                        lease_id: "8d41b6a2-0c3e-4f57-a1d9-2e6b7c8f9a01".to_string(),
                        message: "agentspace: The CA failed its check.\nThe lease stays pending."
                            .to_string(),
                    }),
                    Unplaced::Refused {
                        lease_id: "5c6d7e8f-9a0b-4c1d-8e2f-3a4b5c6d7e8f".to_string(),
                    },
                    Unplaced::Failed(agentspace_driver_ec2::PendingFailure {
                        lease_id: "1f2e3d4c-5b6a-4978-8a1b-2c3d4e5f6a7b".to_string(),
                        message: "The alias file is not agentspace's.".to_string(),
                    }),
                    Unplaced::Left {
                        lease_id: "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d".to_string(),
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
                "place refused lease=5c6d7e8f-9a0b-4c1d-8e2f-3a4b5c6d7e8f",
                "place failed lease=1f2e3d4c-5b6a-4978-8a1b-2c3d4e5f6a7b reason=The alias file is not agentspace's.",
                "place left lease=0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
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
    fn a_run_has_succeeded_only_when_no_lease_was_failed_or_left() {
        let id = "8d41b6a2-0c3e-4f57-a1d9-2e6b7c8f9a01".to_string();
        let with = |unplaced: Vec<Unplaced>| -> Result<PendingPlaced, Failure> {
            Ok(PendingPlaced {
                placed: 1,
                unplaced,
            })
        };
        assert!(succeeded(&Ok(PendingPlaced::default())));
        assert!(succeeded(&with(vec![])));
        // A lease answered `unsatisfiable` is not a failure.
        assert!(succeeded(&with(vec![Unplaced::Refused {
            lease_id: id.clone()
        }])));
        assert!(!succeeded(&with(vec![Unplaced::Left {
            lease_id: id.clone()
        }])));
        assert!(!succeeded(&with(vec![
            Unplaced::Refused {
                lease_id: id.clone()
            },
            Unplaced::Failed(agentspace_driver_ec2::PendingFailure {
                lease_id: id.clone(),
                message: "The alias file is not agentspace's.".to_string(),
            }),
        ])));
        assert!(!succeeded(&Err(step_failed(
            "agentspace: The lease records could not be read."
        ))));
    }

    #[test]
    fn every_start_removes_the_success_and_nothing_else_writes_one() {
        let dir = Scratch::new("provision-success-removed");
        let home = dir.path().to_path_buf();
        for (project, ready) in [("swimcoach", true), ("example", false)] {
            if project == "example" {
                assert!(!record_start(&home, project, "2026-11-24T09:00:00Z").unwrap());
            }
            agentspace_hosts::set_last_success_at(&home, project, "2026-11-24T09:30:00Z").unwrap();
            assert_eq!(
                record_start(&home, project, "2026-11-24T10:00:00Z").unwrap(),
                ready
            );
            let hosts = read_hosts(&home).unwrap();
            let r = agentspace_hosts::host_for(&hosts, project).unwrap();
            assert_eq!(r.last_success_at, None, "{project}");
            assert_eq!(r.last_attempt_at.as_deref(), Some("2026-11-24T10:00:00Z"));
            // Neither a failure nor step 8 writes a success.
            let log = open_log(&home.join("log")).unwrap();
            record_failure(&home, project, ready, &log);
            record_ready(&home, project, "i-1234567890abcdef0").unwrap();
            let hosts = read_hosts(&home).unwrap();
            let r = agentspace_hosts::host_for(&hosts, project).unwrap();
            assert_eq!(r.last_success_at, None, "{project}");
        }
    }

    #[test]
    fn a_success_is_written_alone_and_a_write_that_fails_is_one_log_line() {
        let dir = Scratch::new("provision-success-write");
        let home = dir.path().to_path_buf();
        assert!(!record_start(&home, "example", "2026-11-24T10:00:00Z").unwrap());
        let before = read_hosts(&home).unwrap();
        let path = home.join("log");
        let log = open_log(&path).unwrap();
        let t0 = agentspace_state::now();
        record_success(&home, "example", &log);
        let t1 = agentspace_state::now();
        let after = read_hosts(&home).unwrap();
        let r = agentspace_hosts::host_for(&after, "example").unwrap();
        let at = agentspace_state::parse_time(r.last_success_at.as_deref().unwrap()).unwrap();
        assert!(t0 <= at && at <= t1);
        // Nothing else of any record changed.
        let mut expected = before.clone();
        expected.hosts[1].last_success_at = r.last_success_at.clone();
        assert_eq!(after, expected);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");

        // A registry that cannot be read: one line, the registry's message as its reason, and the
        // file as it was.
        std::fs::write(agentspace_hosts::hosts_path(&home), "not json").unwrap();
        record_success(&home, "example", &log);
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
        let (time, rest) = lines[0].split_once(' ').unwrap();
        assert!(agentspace_state::parse_time(time).is_some());
        assert!(
            rest.starts_with(&format!(
                "success failed reason=The host registry {} cannot be read: ",
                shown(&agentspace_hosts::hosts_path(&home))
            )),
            "{rest}"
        );
        assert_eq!(
            std::fs::read(agentspace_hosts::hosts_path(&home)).unwrap(),
            b"not json"
        );
    }

    /// Records the warnings step 9 gives; nothing else of its words matters here.
    #[derive(Default)]
    struct Warnings(std::cell::RefCell<Vec<String>>);

    impl Say for Warnings {
        fn mode(&self) -> agentspace_operator_api::Mode {
            agentspace_operator_api::Mode::Person
        }
        fn progress(&self, _: &str, _: Option<agentspace_operator_api::CallerLine>) {}
        fn warning(&self, person: &str) {
            self.0.borrow_mut().push(person.to_string());
        }
        fn detail(&self, _: &str) {}
    }

    fn no_env(_: &str) -> Option<OsString> {
        None
    }

    /// A `swimcoach` lease for step 9's lease state file, in the throwaway home's state folder.
    fn box_lease(n: u32, state: &str, size: &str) -> agentspace_state::Record {
        let ready = state == "ready";
        agentspace_state::Record {
            sized_for: None,
            plan: None,
            id: format!("8d41b6a2-0c3e-4f57-a1d9-2e6b7c8f90{n:02x}"),
            project: BOX_PROJECT.to_string(),
            state: state.to_string(),
            platform: "linux/arm64".to_string(),
            profile: "agent-linux".to_string(),
            size: size.to_string(),
            ttl_seconds: 3600,
            created_at: "2026-11-24T10:00:00Z".to_string(),
            expires_at: ready.then(|| "2099-01-01T00:00:00Z".to_string()),
            released_at: None,
            ssh: ready.then(|| agentspace_state::Ssh {
                host_alias: "agentspace-swimcoach-8d41b6a2".to_string(),
                user: "ubuntu".to_string(),
            }),
            reasons: None,
            driver: Some("ec2".to_string()),
        }
    }

    /// Step 9 and its end, run over the box's record in a throwaway home after a run's start, with
    /// `leases` in its lease state file and `instance_type` as the type step 4 recorded. Returns
    /// the record's `last_success_at`, step 9's log lines without their times, and the warnings.
    fn step_9(
        name: &str,
        instance_type: Option<&str>,
        leases: Vec<agentspace_state::Record>,
    ) -> (Option<String>, Vec<String>, Vec<String>) {
        let dir = Scratch::new(name);
        let ctx = crate::tests::ctx_in(dir.path());
        let home = full_home(&ctx);
        agentspace_hosts::set_last_success_at(&home, BOX_PROJECT, "2026-01-01T00:00:00Z").unwrap();
        assert!(
            record_start(
                &home,
                BOX_PROJECT,
                &agentspace_state::format_time(agentspace_state::now())
            )
            .unwrap()
        );
        agentspace_hosts::set_instance_type(&home, BOX_PROJECT, instance_type).unwrap();
        let state = home.join(".agentspace");
        let bytes = agentspace_state::serialize(&agentspace_state::StateFile { leases }).unwrap();
        std::fs::write(state.join("leases.json"), bytes).unwrap();
        let path = dir.path().join("provision.log");
        let log = open_log(&path).unwrap();
        let say = Warnings::default();
        place_and_record(&ctx, &no_env, &home, BOX_PROJECT, &say, &log);
        drop(log);
        let hosts = read_hosts(&home).unwrap();
        let record = agentspace_hosts::host_for(&hosts, BOX_PROJECT)
            .unwrap()
            .clone();
        if let Some(success) = &record.last_success_at {
            let success = agentspace_state::parse_time(success).unwrap();
            let attempt =
                agentspace_state::parse_time(record.last_attempt_at.as_deref().unwrap()).unwrap();
            assert!(success >= attempt, "A success is at or after its attempt");
        }
        let lines = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| l.split_once(' ').unwrap().1.to_string())
            .collect();
        (record.last_success_at, lines, say.0.into_inner())
    }

    #[test]
    fn step_9_records_a_success_when_every_pending_lease_was_placed_or_answered() {
        // Nothing to place.
        let (success, lines, warnings) = step_9("success-none", Some("m9g.xlarge"), vec![]);
        assert!(success.is_some(), "{lines:?}");
        assert_eq!(lines, ["place done placed=0"], "A success adds no line");
        assert!(warnings.is_empty());
        // A lease answered `unsatisfiable`, since a live `ready` lease holds the smaller host.
        let (success, lines, _) = step_9(
            "success-refused",
            Some("m9g.xlarge"),
            vec![box_lease(1, "ready", "m"), box_lease(2, "pending", "l")],
        );
        assert!(success.is_some(), "{lines:?}");
        assert!(lines[1].starts_with("place refused lease="), "{lines:?}");
    }

    #[test]
    fn step_9_records_no_success_when_a_lease_failed_or_was_left() {
        // A lease the host meets, with no CA to issue it: `place failed lease=<id>`.
        let (success, lines, _) = step_9(
            "failed-lease",
            Some("m9g.xlarge"),
            vec![box_lease(1, "pending", "m")],
        );
        assert!(
            lines.iter().any(|l| l.starts_with("place failed lease=")),
            "{lines:?}"
        );
        assert_eq!(success, None, "{lines:?}");
        // A host of no known type: the lease is left for the next run.
        let (success, lines, _) = step_9("left-lease", None, vec![box_lease(1, "pending", "m")]);
        assert!(
            lines.iter().any(|l| l.starts_with("place left lease=")),
            "{lines:?}"
        );
        assert_eq!(success, None, "{lines:?}");
    }

    #[test]
    fn step_9_records_no_success_when_the_call_failed_as_a_whole() {
        // A lease state file that is not valid fails the call itself: `place failed reason=...`,
        // one warning, and still no success.
        let dir = Scratch::new("failed-call");
        let ctx = crate::tests::ctx_in(dir.path());
        let home = full_home(&ctx);
        assert!(record_start(&home, BOX_PROJECT, "2026-11-24T10:00:00Z").unwrap());
        std::fs::write(home.join(".agentspace").join("leases.json"), "{").unwrap();
        let path = dir.path().join("provision.log");
        let log = open_log(&path).unwrap();
        let say = Warnings::default();
        place_and_record(&ctx, &no_env, &home, BOX_PROJECT, &say, &log);
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(" place failed reason="), "{text}");
        assert_eq!(say.0.borrow().len(), 1);
        let hosts = read_hosts(&home).unwrap();
        assert_eq!(
            agentspace_hosts::host_for(&hosts, BOX_PROJECT)
                .unwrap()
                .last_success_at,
            None
        );
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

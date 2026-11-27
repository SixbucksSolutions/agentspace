//! One run of the guard (docs/operator.md, guard, steps 1 to 8; D56, D57, D63).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The run takes `guard.lock`, reads the host registry and the lease state file without their
//! locks, finds each host's instance ID, asks EC2 about every host in one call, has
//! [`decide`] decide each host, carries out what it returns, and sends every running host its
//! horizon. It writes neither file it reads. The streams are `say`'s alone, and the log is
//! [`crate::log`]'s.
//!
//! **Where each line goes.** A progress line, the three of step 6 and the closing line of step 8,
//! goes through [`Say::progress`], which `-Quiet` suppresses. **A host's part that fails goes
//! through [`Say::warning`] as `agentspace: Host '<project>': <message>`, and so does the dropped
//! log line's `agentspace: Cannot write the guard's log <path>: <error>.`, said once at the first
//! line dropped**: `warning` is the guard's failure channel, and nothing that is not a failure
//! goes through it (architect, 2026-11-22, for v0.08-T03). `-Quiet` MUST NOT withhold a failure
//! (operator.md, guard, Its streams), so the [`Say`] the operator hands the guard MUST print
//! `warning` whatever `-Quiet` says, and suppress `progress` alone.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::Path;

use agentspace_box::{
    Described, describe_hosts, instance_id_from_stack, send_horizon_on, stop_host,
};
use agentspace_hosts::{
    HostKey, HostState, Hosts, HostsError, guard_lock_path, host_run_lock_path_keyed, hosts_path,
    run_is_running_keyed, try_guard_lock,
};
use agentspace_operator_api::{Aws, CallerFailure, Failure, HostPlatform, SIGN_IN_MESSAGE, Say};
use agentspace_state::{StoreError, format_time};

use crate::decide::{Action, HostOnPlatform, ProvisionLock, decide_on};
use crate::log::{GuardLog, bare};
use crate::{GRACE_SECONDS, Input, Report};

/// The message of every failure this run makes itself: a person's, with no caller's line.
fn failure(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::Same,
    }
}

/// Whether a failure is #0034's sign-in message, which ends the run. It is told by the message,
/// equal to [`SIGN_IN_MESSAGE`], and never by a [`CallerFailure`] kind: `send_horizon` also
/// returns [`CallerFailure::Same`] when it refuses a negative horizon.
fn is_sign_in(failure: &Failure) -> bool {
    failure.message == SIGN_IN_MESSAGE
}

/// What a [`HostsError`] says, as the one sentence fragment of a message: the file's own reason,
/// with no line break.
fn hosts_reason(error: &HostsError) -> String {
    let text = match error {
        HostsError::Unreadable(why) | HostsError::CannotLock(why) | HostsError::Unwritable(why) => {
            why.to_string()
        }
        HostsError::NotValid(why) => why.to_string(),
        HostsError::CannotLockRun(_) | HostsError::CannotLockGuard(_) => lock_error(error),
        HostsError::Busy => "the registry is in use by another command".to_string(),
        HostsError::BadProject => "a project is not in its normalized form".to_string(),
    };
    text.replace(['\r', '\n'], " ")
}

/// The `<error>` of `Cannot take <path>: <error>.` and `Cannot try <path>: <error>.`: the host
/// registry library's own text for the error, its [`std::fmt::Display`], as provision's step 2
/// prints it for its own lock (`error_text` in agentspace-sync's provision.rs), with each control
/// character a space and the one final period dropped, since the sentence supplies it. The guard
/// has no rendering of its own (operator.md, guard, steps 1 and 6; architect, 2026-11-22, for
/// v0.08-T03).
fn lock_error(error: &HostsError) -> String {
    let text: String = error
        .to_string()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    text.trim().trim_end_matches('.').to_string()
}

/// The lease state file's failure and its `<reason>` (operator.md, guard, step 3).
fn leases_failure(path: &Path, why: &str) -> Failure {
    failure(format!(
        "agentspace: The lease state file {} cannot be read: {why}.",
        path.display()
    ))
}

/// The time of a stop as a progress line prints it: `<h:mm><am|pm>` in the laptop's local time, or
/// `<h:mm><am|pm> UTC` when the system gives no local time for the instant (always off Windows),
/// from [`agentspace_zone::local_clock_time_or_utc`], the one formatter and the one fallback
/// (operator.md, guard, step 6; architect, 2026-11-22, for v0.08-T03). The guard keeps no
/// fallback of its own.
fn stop_time(unix: i64) -> String {
    agentspace_zone::local_clock_time_or_utc(unix)
}

/// The `<error>` of the failed-send line for a host whose record names a platform agentspace has no
/// seam for (operator.md, guard, step 7; architect, 2026-11-26, for v0.12-T03, pin P10): no
/// document or command exists for it, so nothing is sent. `platform` is the record's own platform.
fn no_seam(platform: &str) -> Failure {
    failure(format!(
        "agentspace: No Session Manager document or horizon command exists for the platform {platform}."
    ))
}

/// Sends one running host its horizon through its platform's rows (operator.md, guard, step 7; The
/// platform seam, the Session Manager document and horizon command rows): one
/// `agentspace_box::send_horizon_on`, on every platform, both Linux ones included, whose
/// `--parameters` is `commands=<command>` on `AWS-RunShellScript`, byte for byte as before v0.12,
/// and the JSON value `{"commands":["<command>"]}` on every other document. `platform` is the seam
/// of the host's platform, or `None` for a record whose platform, `record_platform`, agentspace has
/// no seam for: that send makes no call and fails with [`no_seam`] (pin P10).
fn send_horizon_for(
    aws: &dyn Aws,
    platform: Option<&dyn HostPlatform>,
    record_platform: &str,
    instance_id: &str,
    horizon: i64,
) -> Result<(), Failure> {
    match platform {
        Some(platform) => send_horizon_on(aws, instance_id, platform, horizon),
        None => Err(no_seam(record_platform)),
    }
}

/// How a run picks a host's rows of the seam from its record's platform: in a release build,
/// `agentspace_platforms::for_platform` and nothing else (D101). A test MAY pass a stand-in.
type Pick<'a> = &'a dyn Fn(&str) -> Option<&'static dyn HostPlatform>;

/// The run's state: its streams, its log, and the hosts whose part failed.
struct Run<'a> {
    say: &'a dyn Say,
    log: GuardLog,
    /// The registry's index of each host whose part failed, once each.
    failed: BTreeSet<usize>,
    /// The dropped-log message has been said: it is said once, at the first line dropped.
    drop_said: Cell<bool>,
}

impl Run<'_> {
    /// Says `agentspace: Cannot write the guard's log <path>: <error>.` through [`Say::warning`] at
    /// the first line the log dropped, and never again (operator.md, guard, Its log; architect,
    /// 2026-11-22, for v0.08-T03). Every write to the log MUST be followed by a call of this.
    fn note_drop(&self) {
        if self.drop_said.get() {
            return;
        }
        if let Some(error) = self.log.dropped() {
            self.drop_said.set(true);
            self.say.warning(&format!(
                "agentspace: Cannot write the guard's log {}: {error}.",
                self.log.path().display()
            ));
        }
    }

    /// A line about one host in the log, `<time> host=<host name> <text>`.
    fn log_host(&self, host: &str, text: &str) {
        self.log.host(host, text);
        self.note_drop();
    }

    /// A host's part fails (operator.md, guard, A host's part that fails): its message through
    /// [`Say::warning`] as `agentspace: Host '<host name>': <message>`, one `failed` line in the log,
    /// and the run goes on. `message` has no `agentspace: ` prefix.
    fn host_failed(&mut self, index: usize, host: &str, step: &str, message: &str) {
        let message = bare(message);
        self.say
            .warning(&format!("agentspace: Host '{host}': {message}"));
        self.log.host_failed(host, step, message);
        self.note_drop();
        self.failed.insert(index);
    }

    /// A call met #0034's sign-in message: it is logged without a `host=` field, as the step of the
    /// call that met it, and ends the run.
    fn sign_in(&self, step: &str, failure: Failure) -> Failure {
        self.ends(step, failure)
    }

    /// A failure that ends the run: logged without a `host=` field, and returned.
    fn ends(&self, step: &str, failure: Failure) -> Failure {
        self.log.run_failed(step, &failure.message);
        self.note_drop();
        failure
    }
}

/// One run of the guard (operator.md, guard, steps 1 to 8). See [`crate::run`].
pub(crate) fn execute(input: &Input, say: &dyn Say, aws: &dyn Aws) -> Result<Report, Failure> {
    execute_with(input, say, aws, &agentspace_platforms::for_platform)
}

/// [`execute`], with each host's rows of the seam picked by `pick`.
fn execute_with(
    input: &Input,
    say: &dyn Say,
    aws: &dyn Aws,
    pick: Pick<'_>,
) -> Result<Report, Failure> {
    let home = &input.ctx.home;
    let mut run = Run {
        say,
        log: GuardLog::new(home),
        failed: BTreeSet::new(),
        drop_said: Cell::new(false),
    };

    // Step 1: guard.lock, without waiting, held until this function returns.
    let _guard_lock = match try_guard_lock(home) {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            return Ok(Report {
                lock_held: true,
                ..Report::default()
            });
        }
        Err(e) => {
            let f = failure(format!(
                "agentspace: Cannot take {}: {}.",
                guard_lock_path(home).display(),
                lock_error(&e)
            ));
            return Err(run.ends("lock", f));
        }
    };

    // Step 2: the host registry, without its lock.
    let hosts: Hosts = match agentspace_hosts::read(home) {
        Ok(hosts) => hosts,
        Err(e) => {
            let f = failure(format!(
                "agentspace: The host registry {} cannot be read: {}.",
                hosts_path(home).display(),
                hosts_reason(&e)
            ));
            return Err(run.ends("hosts", f));
        }
    };

    // Step 3: the lease state file, without its lock. A missing file is not "no leases".
    let leases_path = input.state_dir.join("leases.json");
    // `agentspace_state::read` reads a missing file as no leases, so its absence is checked first:
    // agentspace never deletes the file, so one found here is still there for the read.
    if matches!(std::fs::metadata(&leases_path), Err(ref e) if e.kind() == std::io::ErrorKind::NotFound)
    {
        let f = leases_failure(&leases_path, "the file does not exist");
        return Err(run.ends("leases", f));
    }
    let leases = match agentspace_state::read(input.state_dir) {
        Ok(file) => file.leases,
        Err(e) => {
            let why = match &e {
                StoreError::Unreadable(why)
                | StoreError::NotValid(why)
                | StoreError::CannotLock(why)
                | StoreError::Unwritable(why) => why.replace(['\r', '\n'], " "),
                StoreError::Busy => "the file is in use".to_string(),
            };
            return Err(run.ends("leases", leases_failure(&leases_path, &why)));
        }
    };
    let now = input.now;

    // Step 4: each host's instance ID, in the registry's order.
    // Each host is named by its host name in every message and log line: its project on
    // `linux/arm64`, and `<project>.<os>-<arch>` on any other platform (operator.md, guard).
    let names: Vec<String> = hosts
        .hosts
        .iter()
        .map(|host| host.key().host_name())
        .collect();
    let mut to_decide: Vec<HostOnPlatform> = Vec::new();
    let mut decided: Vec<usize> = Vec::new();
    for (index, host) in hosts.hosts.iter().enumerate() {
        let id = match &host.instance_id {
            Some(id) => id.clone(),
            None if host.state == HostState::Ready => {
                match instance_id_from_stack(aws, &host.stack) {
                    Ok(id) => id,
                    Err(f) if is_sign_in(&f) => return Err(run.sign_in("instance", f)),
                    Err(f) => {
                        run.host_failed(index, &names[index], "instance", &f.message);
                        continue;
                    }
                }
            }
            // A `creating` or `failed` record with no ID: no instance of it is known.
            None => continue,
        };
        to_decide.push(HostOnPlatform {
            key: host.key(),
            instance_id: id,
        });
        decided.push(index);
    }

    // Step 5: every host's instance in one call, not made when no ID was found.
    let ids: Vec<&str> = to_decide.iter().map(|h| h.instance_id.as_str()).collect();
    let described: Vec<Described> = match describe_hosts(aws, &ids) {
        Ok(described) => described,
        Err(f) if is_sign_in(&f) => return Err(run.sign_in("describe", f)),
        Err(f) => return Err(run.ends("describe", f)),
    };

    // Step 6: decide each host, in the registry's order, and carry out what is decided.
    // A host's provisioning lock is its own folder's, `hosts\<host name>\provision.lock`.
    let provision_held = |key: &HostKey| match run_is_running_keyed(home, key) {
        Ok(false) => ProvisionLock::Free,
        Ok(true) => ProvisionLock::Held,
        Err(e) => ProvisionLock::NotTried {
            path: host_run_lock_path_keyed(home, key).display().to_string(),
            error: lock_error(&e),
        },
    };
    let actions = decide_on(&to_decide, &leases, &described, &provision_held, now);

    let (mut running, mut stopped, mut unexplained) = (0usize, 0usize, 0usize);
    // Each running host the run did not stop, for step 7: its registry index, ID, horizon.
    let mut sends: Vec<(usize, String, i64)> = Vec::new();
    for (&index, action) in decided.iter().zip(actions) {
        // The host's name in every message and log line of its part.
        let host = names[index].as_str();
        match action {
            Action::Nothing { .. } => {}
            Action::NotFound { instance_id, .. } => {
                let message = format!("Instance {instance_id} was not found.");
                run.host_failed(index, host, "instance", &message);
            }
            Action::Unexpected { member, .. } => {
                // `State.Name` is read for every host; `LaunchTime` for a running one alone.
                if member == "LaunchTime" {
                    running += 1;
                }
                let message = format!(
                    "Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].{member}."
                );
                run.host_failed(index, host, "instance", &message);
            }
            Action::Explained {
                instance_id,
                horizon,
                ..
            } => {
                running += 1;
                sends.push((index, instance_id, horizon));
            }
            Action::Unexplained {
                instance_id,
                stop_at,
                horizon,
                ..
            } => {
                running += 1;
                unexplained += 1;
                run.log_host(
                    host,
                    &format!(
                        "unexplained since={} stops={}",
                        format_time(stop_at - GRACE_SECONDS),
                        format_time(stop_at)
                    ),
                );
                say.progress(
                    &format!(
                        "The host of '{host}' ({instance_id}) has no live lease; it stops at {}.",
                        stop_time(stop_at)
                    ),
                    None,
                );
                sends.push((index, instance_id, horizon));
            }
            Action::Provisioning {
                instance_id,
                stop_at,
                horizon,
                ..
            } => {
                running += 1;
                unexplained += 1;
                run.log_host(
                    host,
                    &format!(
                        "unexplained since={} stops={} provisioning=running",
                        format_time(stop_at - GRACE_SECONDS),
                        format_time(stop_at)
                    ),
                );
                say.progress(
                    &format!(
                        "The host of '{host}' ({instance_id}) has no live lease, but a provisioning run is working on it; it is not stopped."
                    ),
                    None,
                );
                sends.push((index, instance_id, horizon));
            }
            Action::LockNotTried {
                instance_id,
                path,
                error,
                horizon,
                ..
            } => {
                running += 1;
                let message = format!("Cannot try {path}: {error}.");
                run.host_failed(index, host, "lock", &message);
                sends.push((index, instance_id, horizon));
            }
            Action::Stop {
                instance_id,
                stop_at,
                horizon,
                ..
            } => {
                running += 1;
                match stop_host(aws, &instance_id) {
                    Ok(()) => {
                        stopped += 1;
                        run.log_host(
                            host,
                            &format!("stopped since={}", format_time(stop_at - GRACE_SECONDS)),
                        );
                        say.progress(
                            &format!(
                                "Stopping the host of '{host}' ({instance_id}): no live lease explains it."
                            ),
                            None,
                        );
                    }
                    Err(f) if is_sign_in(&f) => return Err(run.sign_in("stop", f)),
                    Err(f) => {
                        let message = format!("Could not stop {instance_id}: {}", bare(&f.message));
                        run.host_failed(index, host, "stop", &message);
                        sends.push((index, instance_id, horizon));
                    }
                }
            }
        }
    }

    // Step 7: every running host the run did not stop is sent its horizon, in the registry's
    // order, through its own platform's rows. A horizon sent is not logged and prints nothing.
    for (index, instance_id, horizon) in sends {
        let record_platform = hosts.hosts[index].platform();
        match send_horizon_for(
            aws,
            pick(record_platform),
            record_platform,
            &instance_id,
            horizon,
        ) {
            Ok(()) => {}
            Err(f) if is_sign_in(&f) => return Err(run.sign_in("horizon", f)),
            Err(f) => {
                let message = format!(
                    "Could not send the lease horizon to {instance_id}: {}",
                    bare(&f.message)
                );
                run.host_failed(index, &names[index], "horizon", &message);
            }
        }
    }

    // Step 8: the closing line. A line the log dropped was said once, at the first drop, and the
    // report carries it: the operator exits 1 on it (D36; architect, 2026-11-22, for v0.08-T03).
    say.progress(
        &format!(
            "Hosts: {}; running {running}, stopped {stopped}, unexplained {unexplained}.",
            hosts.hosts.len()
        ),
        None,
    );
    Ok(Report {
        lock_held: false,
        hosts: hosts.hosts.len(),
        running,
        stopped,
        unexplained,
        failed: run.failed.len(),
        log_dropped: run.log.dropped().is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sign_in_message_is_told_by_its_text_and_not_by_its_kind() {
        let same = |message: &str| Failure {
            message: message.to_string(),
            caller: CallerFailure::Same,
        };
        assert!(is_sign_in(&Failure {
            message: SIGN_IN_MESSAGE.to_string(),
            caller: CallerFailure::CouldNotReach,
        }));
        assert!(!is_sign_in(&same(
            "agentspace: The lease horizon cannot be negative."
        )));
    }

    #[test]
    fn the_stop_time_is_the_zone_crates_text_and_never_empty() {
        // The one formatter is agentspace_zone's: the local time when the system gives one, else
        // the same instant in UTC with " UTC" after it, never a bare UTC time.
        let minute = 1_795_075_920;
        for unix in [minute, minute + 1, minute + 59, minute + 60, 1_780_000_020] {
            let shown = stop_time(unix);
            assert_eq!(shown, agentspace_zone::local_clock_time_or_utc(unix));
            let local = agentspace_zone::local_clock_time(unix);
            match local {
                Some(local) => assert_eq!(shown, local),
                None => assert_eq!(
                    shown,
                    format!("{} UTC", agentspace_zone::clock_time_at(unix, 0))
                ),
            }
            let bare = shown.strip_suffix(" UTC").unwrap_or(&shown);
            assert!(bare.ends_with("am") || bare.ends_with("pm"), "{shown}");
            assert!(shown.contains(':') && !shown.starts_with('0'), "{shown}");
        }
        // Seconds are rounded up to the next whole minute.
        assert_eq!(stop_time(minute + 1), stop_time(minute + 60));
    }

    #[test]
    fn the_box_is_the_registry_seed_project() {
        // The seed's one record is the box's, which the guard guards like any host.
        assert_eq!(
            Hosts::seed().hosts[0].project,
            agentspace_hosts::BOX_PROJECT
        );
    }

    #[test]
    fn a_reason_for_the_hosts_file_holds_no_line_break() {
        assert!(!hosts_reason(&HostsError::Busy).contains('\n'));
    }

    /// The guard's `<error>` is the host registry library's own `Display`, the text provision's
    /// step 2 prints for its own lock (its `error_text` of a `HostsError` is this `Display` with
    /// control characters flattened and the final period dropped), and not the `Failure`'s
    /// alone: the same text for the same error, with no second rendering.
    #[test]
    fn a_lock_error_is_the_hosts_librarys_display_without_its_final_period() {
        use agentspace_hosts::Failure;
        for (error, expected) in [
            (
                HostsError::CannotLockGuard(Failure::CannotOpenLock),
                "The guard's lock could not be taken: the lock file could not be opened",
            ),
            (
                HostsError::CannotLockGuard(Failure::FolderIsAFile),
                "The guard's lock could not be taken: a file is where a folder belongs",
            ),
            (
                HostsError::CannotLockRun(Failure::CannotOpenLock),
                "The provisioning run's lock could not be taken: the lock file could not be opened",
            ),
            (
                HostsError::CannotLockRun(Failure::FolderIsAFile),
                "The provisioning run's lock could not be taken: a file is where a folder belongs",
            ),
        ] {
            assert_eq!(lock_error(&error), expected);
            assert_eq!(format!("{}.", lock_error(&error)), error.to_string());
            // The reason the run gives for a host-registry file's error is the same text.
            assert_eq!(hosts_reason(&error), expected);
        }
    }
}

/// The run's flows, against a scripted AWS CLI and a throwaway home.
#[cfg(test)]
mod flow_tests {
    use super::*;
    use agentspace_operator_api::{AwsError, Call, CallerLine, Context, Mode};
    use serde_json::{Value, json};
    use std::cell::RefCell;
    use std::path::PathBuf;

    const ID: &str = "i-0123456789abcdef0";
    /// An arbitrary run time: 2026-11-19T08:12:34Z.
    const NOW: i64 = 1_795_075_954;

    type Answer = Box<dyn Fn(&str) -> Result<Value, AwsError>>;

    /// Records each call as `service operation args...` and answers from a function.
    struct FakeAws {
        calls: RefCell<Vec<String>>,
        answer: Answer,
    }

    impl Aws for FakeAws {
        fn call(&self, call: &Call) -> Result<Value, AwsError> {
            let mut line = vec![call.service, call.operation];
            line.extend(call.args);
            let line = line.join(" ");
            let answer = (self.answer)(&line);
            self.calls.borrow_mut().push(line);
            answer
        }
    }

    /// Records `progress` and `warning` lines apart.
    #[derive(Default)]
    struct FakeSay {
        progress: RefCell<Vec<String>>,
        warnings: RefCell<Vec<String>>,
    }

    impl Say for FakeSay {
        fn mode(&self) -> Mode {
            Mode::Person
        }
        fn progress(&self, person: &str, _caller: Option<CallerLine>) {
            self.progress.borrow_mut().push(person.to_string());
        }
        fn warning(&self, person: &str) {
            self.warnings.borrow_mut().push(person.to_string());
        }
        fn detail(&self, _text: &str) {}
    }

    struct World {
        home: tempfile::TempDir,
        ctx: Context,
    }

    impl World {
        /// A throwaway home whose lease state file holds no lease, with the registry missing: the
        /// box's record alone.
        fn new() -> World {
            let home = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(home.path().join(".agentspace")).unwrap();
            std::fs::write(
                home.path().join(".agentspace").join("leases.json"),
                "{\"version\":0,\"leases\":[]}\n",
            )
            .unwrap();
            let ctx = Context {
                home: home.path().to_path_buf(),
                local_app_data: None,
                system_root: PathBuf::from("C:\\Windows"),
                nested: false,
            };
            World { home, ctx }
        }

        fn state_dir(&self) -> PathBuf {
            self.home.path().join(".agentspace")
        }

        fn log(&self) -> Vec<String> {
            std::fs::read_to_string(self.state_dir().join("guard.log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }

        /// One run, with each host's rows from [`stand_in_pick`]: both Linux platforms' real rows,
        /// and a stand-in for `windows/amd64`, so a Windows host's send is shown whatever
        /// `agentspace-windows` builds.
        fn run(&self, say: &FakeSay, aws: &FakeAws) -> Result<Report, Failure> {
            self.run_picking(say, aws, &stand_in_pick)
        }

        fn run_picking(&self, say: &FakeSay, aws: &FakeAws, pick: Pick) -> Result<Report, Failure> {
            let state_dir = self.state_dir();
            execute_with(
                &Input {
                    ctx: &self.ctx,
                    state_dir: &state_dir,
                    now: NOW,
                },
                say,
                aws,
                pick,
            )
        }
    }

    fn stack_answer() -> Value {
        json!({"Stacks": [{"Outputs": [{"OutputKey": "InstanceId", "OutputValue": ID}]}]})
    }

    /// An AWS whose stack read gives `ID`, whose describe gives `instance`, and whose other calls
    /// are answered by `rest`.
    fn aws(instance: Value, rest: impl Fn(&str) -> Result<Value, AwsError> + 'static) -> FakeAws {
        FakeAws {
            calls: RefCell::default(),
            answer: Box::new(move |line| {
                if line.starts_with("cloudformation describe-stacks") {
                    Ok(stack_answer())
                } else if line.starts_with("ec2 describe-instances") {
                    Ok(json!({"Reservations": [{"Instances": [instance.clone()]}]}))
                } else {
                    rest(line)
                }
            }),
        }
    }

    fn running(launched: i64) -> Value {
        json!({"InstanceId": ID, "State": {"Name": "running"}, "LaunchTime": format_time(launched)})
    }

    fn ok(_: &str) -> Result<Value, AwsError> {
        Ok(Value::Null)
    }

    fn operations(aws: &FakeAws) -> Vec<String> {
        aws.calls
            .borrow()
            .iter()
            .map(|c| c.split(' ').take(2).collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn a_guard_lock_held_by_another_ends_the_run_with_nothing_read_called_logged_or_printed() {
        let w = World::new();
        let _held = agentspace_hosts::try_guard_lock(&w.ctx.home)
            .unwrap()
            .unwrap();
        let (say, aws) = (FakeSay::default(), aws(running(NOW), ok));
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(
            report,
            Report {
                lock_held: true,
                ..Report::default()
            }
        );
        assert!(aws.calls.borrow().is_empty());
        assert!(w.log().is_empty());
        assert!(say.progress.borrow().is_empty() && say.warnings.borrow().is_empty());
    }

    #[test]
    fn a_registry_that_is_not_valid_fails_the_run_before_any_call() {
        let w = World::new();
        std::fs::write(w.state_dir().join("hosts.json"), "not json").unwrap();
        let (say, aws) = (FakeSay::default(), aws(running(NOW), ok));
        let f = w.run(&say, &aws).unwrap_err();
        assert!(f.message.starts_with("agentspace: The host registry "));
        assert!(f.message.contains(" cannot be read: "));
        assert!(aws.calls.borrow().is_empty());
        let log = w.log();
        assert_eq!(log.len(), 1);
        assert!(log[0].contains("Z failed step=hosts reason=The host registry "));
        assert!(!log[0].contains(" host="));
    }

    #[test]
    fn a_missing_or_invalid_lease_state_file_fails_the_run_and_is_not_no_leases() {
        for content in [None, Some("not json")] {
            let w = World::new();
            let path = w.state_dir().join("leases.json");
            match content {
                None => std::fs::remove_file(&path).unwrap(),
                Some(text) => std::fs::write(&path, text).unwrap(),
            }
            let (say, aws) = (FakeSay::default(), aws(running(NOW - 3600), ok));
            let f = w.run(&say, &aws).unwrap_err();
            assert!(
                f.message.starts_with("agentspace: The lease state file ")
                    && f.message.contains(" cannot be read: ")
            );
            if content.is_none() {
                assert!(f.message.ends_with(": the file does not exist."));
            }
            assert!(aws.calls.borrow().is_empty(), "No call, so no stop");
            let log = w.log();
            assert_eq!(log.len(), 1);
            assert!(log[0].contains("Z failed step=leases reason=The lease state file "));
            assert!(!log[0].contains(" host="));
        }
    }

    #[test]
    fn a_due_unexplained_host_is_stopped_logged_and_sent_no_horizon() {
        let w = World::new();
        let (say, aws) = (FakeSay::default(), aws(running(NOW - 3600), ok));
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(
            report,
            Report {
                lock_held: false,
                hosts: 1,
                running: 1,
                stopped: 1,
                unexplained: 0,
                failed: 0,
                log_dropped: false
            }
        );
        assert_eq!(
            operations(&aws),
            [
                "cloudformation describe-stacks",
                "ec2 describe-instances",
                "ec2 stop-instances"
            ]
        );
        assert_eq!(
            *say.progress.borrow(),
            [
                format!("Stopping the host of 'swimcoach' ({ID}): no live lease explains it."),
                "Hosts: 1; running 1, stopped 1, unexplained 0.".to_string()
            ]
        );
        let log = w.log();
        assert_eq!(log.len(), 1);
        assert!(log[0].ends_with(&format!(
            "Z host=swimcoach stopped since={}",
            format_time(NOW - 3600)
        )));
    }

    #[test]
    fn an_unexplained_host_before_its_stop_time_is_logged_told_and_sent_its_horizon() {
        let w = World::new();
        let launched = NOW - 60;
        let (say, aws) = (FakeSay::default(), aws(running(launched), ok));
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(
            (report.running, report.stopped, report.unexplained),
            (1, 0, 1)
        );
        assert_eq!(
            operations(&aws),
            [
                "cloudformation describe-stacks",
                "ec2 describe-instances",
                "ssm send-command"
            ]
        );
        let send = aws.calls.borrow()[2].clone();
        assert!(
            send.split_whitespace().any(|w| w == launched.to_string()),
            "The horizon is the launch time: {send}"
        );
        let progress = say.progress.borrow();
        assert!(progress[0].starts_with(&format!(
            "The host of 'swimcoach' ({ID}) has no live lease; it stops at "
        )));
        assert!(progress[0].ends_with("m."));
        assert_eq!(
            progress[1],
            "Hosts: 1; running 1, stopped 0, unexplained 1."
        );
        let log = w.log();
        assert_eq!(log.len(), 1, "A horizon sent is not logged");
        assert!(log[0].ends_with(&format!(
            "Z host=swimcoach unexplained since={} stops={}",
            format_time(launched),
            format_time(launched + GRACE_SECONDS)
        )));
    }

    #[test]
    fn a_host_whose_provisioning_run_holds_its_lock_is_left_logged_and_sent_its_horizon() {
        let w = World::new();
        let _run = agentspace_hosts::try_run_lock(&w.ctx.home, "swimcoach")
            .unwrap()
            .unwrap();
        let (say, aws) = (FakeSay::default(), aws(running(NOW - 3600), ok));
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(
            (report.running, report.stopped, report.unexplained),
            (1, 0, 1)
        );
        assert_eq!(
            operations(&aws),
            [
                "cloudformation describe-stacks",
                "ec2 describe-instances",
                "ssm send-command"
            ]
        );
        assert_eq!(
            say.progress.borrow()[0],
            format!(
                "The host of 'swimcoach' ({ID}) has no live lease, but a provisioning run is working on it; it is not stopped."
            )
        );
        assert!(w.log()[0].ends_with(&format!(
            " host=swimcoach unexplained since={} stops={} provisioning=running",
            format_time(NOW - 3600),
            format_time(NOW - 3600 + GRACE_SECONDS)
        )));
    }

    #[test]
    fn a_stop_that_fails_fails_the_hosts_part_and_the_host_is_sent_its_horizon() {
        let w = World::new();
        let aws = aws(running(NOW - 3600), |line| {
            if line.starts_with("ec2 stop-instances") {
                Err(AwsError::Failed("Denied\nbadly".to_string()))
            } else {
                Ok(Value::Null)
            }
        });
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!((report.running, report.stopped, report.failed), (1, 0, 1));
        assert_eq!(operations(&aws).last().unwrap(), "ssm send-command");
        assert_eq!(
            *say.warnings.borrow(),
            [format!(
                "agentspace: Host 'swimcoach': Could not stop {ID}: Denied\nbadly"
            )]
        );
        let log = w.log();
        assert_eq!(log.len(), 1);
        assert!(log[0].ends_with(&format!(
            "Z host=swimcoach failed step=stop reason=Could not stop {ID}: Denied?badly"
        )));
    }

    #[test]
    fn a_horizon_that_fails_to_send_fails_the_hosts_part_and_is_logged_last() {
        let w = World::new();
        let aws = aws(running(NOW - 60), |_| {
            Err(AwsError::Failed("InvalidInstanceId".to_string()))
        });
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(report.failed, 1);
        let log = w.log();
        assert_eq!(log.len(), 2);
        assert!(log[0].contains(" unexplained since="));
        assert!(log[1].ends_with(&format!(
            "Z host=swimcoach failed step=horizon reason=Could not send the lease horizon to {ID}: InvalidInstanceId"
        )));
    }

    #[test]
    fn the_sign_in_message_from_any_call_ends_the_run_after_logging_it_with_no_host() {
        for (step, ends_at) in [
            ("instance", "cloudformation describe-stacks"),
            ("describe", "ec2 describe-instances"),
            ("stop", "ec2 stop-instances"),
            ("horizon", "ssm send-command"),
        ] {
            let w = World::new();
            // A stop is due for `stop`, and a horizon is due for `horizon`.
            let launched = if step == "horizon" {
                NOW - 60
            } else {
                NOW - 3600
            };
            let aws = FakeAws {
                calls: RefCell::default(),
                answer: Box::new(move |line| {
                    if line.starts_with(ends_at) {
                        Err(AwsError::SignIn)
                    } else if line.starts_with("cloudformation describe-stacks") {
                        Ok(stack_answer())
                    } else {
                        Ok(json!({"Reservations": [{"Instances": [running(launched)]}]}))
                    }
                }),
            };
            let say = FakeSay::default();
            let f = w.run(&say, &aws).unwrap_err();
            assert_eq!(f.message, SIGN_IN_MESSAGE, "{step}");
            assert_eq!(
                operations(&aws).last().unwrap(),
                ends_at,
                "No call after the one that met it ({step})"
            );
            let log = w.log();
            assert_eq!(log.len(), if step == "horizon" { 2 } else { 1 }, "{step}");
            let last = log.last().unwrap();
            assert!(
                last.ends_with(&format!(
                    "Z failed step={step} reason={}",
                    bare(SIGN_IN_MESSAGE)
                )),
                "{last}"
            );
            assert!(!last.contains(" host="));
            assert!(
                say.progress
                    .borrow()
                    .iter()
                    .all(|l| !l.starts_with("Hosts:"))
            );
        }
    }

    #[test]
    fn a_describe_with_no_reservations_fails_the_run_with_nothing_stopped_or_sent() {
        let w = World::new();
        let aws = FakeAws {
            calls: RefCell::default(),
            answer: Box::new(|line| {
                if line.starts_with("cloudformation") {
                    Ok(stack_answer())
                } else {
                    Ok(json!({}))
                }
            }),
        };
        let say = FakeSay::default();
        let f = w.run(&say, &aws).unwrap_err();
        assert_eq!(
            f.message,
            "agentspace: Unexpected answer from ec2 describe-instances: no Reservations."
        );
        assert_eq!(
            operations(&aws),
            ["cloudformation describe-stacks", "ec2 describe-instances"]
        );
        let log = w.log();
        assert_eq!(log.len(), 1);
        assert!(log[0].contains("Z failed step=describe reason=Unexpected answer"));
        assert!(!log[0].contains(" host="));
    }

    #[test]
    fn a_running_host_with_no_launch_time_fails_its_part_and_is_sent_nothing() {
        let w = World::new();
        let aws = aws(json!({"InstanceId": ID, "State": {"Name": "running"}}), ok);
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!((report.running, report.failed), (1, 1));
        assert_eq!(
            operations(&aws),
            ["cloudformation describe-stacks", "ec2 describe-instances"]
        );
        assert_eq!(
            *say.warnings.borrow(),
            [
                "agentspace: Host 'swimcoach': Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].LaunchTime."
            ]
        );
        assert!(w.log()[0].ends_with(
            "Z host=swimcoach failed step=instance reason=Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].LaunchTime."
        ));
    }

    #[test]
    fn a_host_that_is_not_running_gets_no_line_no_stop_and_no_horizon() {
        let w = World::new();
        let aws = aws(json!({"InstanceId": ID, "State": {"Name": "stopped"}}), ok);
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!((report.hosts, report.running, report.failed), (1, 0, 0));
        assert_eq!(
            operations(&aws),
            ["cloudformation describe-stacks", "ec2 describe-instances"]
        );
        assert!(w.log().is_empty());
        assert_eq!(
            *say.progress.borrow(),
            ["Hosts: 1; running 0, stopped 0, unexplained 0."]
        );
    }

    #[test]
    fn a_dropped_log_line_is_said_once_as_a_warning_and_reported_without_changing_what_it_did() {
        let w = World::new();
        std::fs::create_dir_all(w.state_dir().join("guard.log")).unwrap();
        let (say, aws) = (FakeSay::default(), aws(running(NOW - 3600), ok));
        let report = w.run(&say, &aws).unwrap();
        assert!(report.log_dropped);
        assert_eq!(report.failed, 0, "A drop is not a host's failure");
        let warnings = say.warnings.borrow();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("agentspace: Cannot write the guard's log "));
        assert!(warnings[0].ends_with('.'));
        assert_eq!(operations(&aws).last().unwrap(), "ec2 stop-instances");
        assert!(
            say.progress
                .borrow()
                .iter()
                .any(|l| l.starts_with("Hosts:"))
        );
    }

    // Two hosts of one project on two platforms (operator.md, guard, from v0.12; D91, D101).

    const BOX_ID: &str = "i-0123456789abcdefb";
    const ARM_ID: &str = "i-0123456789abcdefa";
    const WIN_ID: &str = "i-0123456789abcdef9";
    const AMD_ID: &str = "i-0123456789abcdef8";
    const ARM_HOST: &str = "agentspace";
    const WIN_HOST: &str = "agentspace.windows-amd64";
    const AMD_HOST: &str = "agentspace.linux-amd64";

    /// One registry record: the project, its platform (omitted for `linux/arm64`, as a writer
    /// writes it), its stack and its instance ID.
    fn record(project: &str, platform: Option<&str>, stack: &str, id: &str) -> Value {
        let mut record =
            json!({"project": project, "stack": stack, "state": "ready", "instance_id": id});
        if let Some(platform) = platform {
            record["platform"] = json!(platform);
        }
        record
    }

    /// The box, and agentspace's `linux/arm64`, `windows/amd64` and `linux/amd64` hosts, in that
    /// order.
    fn four_hosts() -> Vec<Value> {
        vec![
            record("swimcoach", None, "agentbox", BOX_ID),
            record("agentspace", None, "agentspace-host-agentspace", ARM_ID),
            record(
                "agentspace",
                Some("windows/amd64"),
                "agentspace-windows-amd64-host-agentspace",
                WIN_ID,
            ),
            record(
                "agentspace",
                Some("linux/amd64"),
                "agentspace-linux-amd64-host-agentspace",
                AMD_ID,
            ),
        ]
    }

    fn lease_on(platform: &str, state: &str, expires: Option<i64>) -> Value {
        static NUMBER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let n = NUMBER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut lease = json!({
            "id": format!("00000000-0000-4000-8000-{n:012x}"),
            "project": "agentspace",
            "state": state,
            "platform": platform,
            "profile": "agent-linux",
            "size": "m",
            "ttl_seconds": 28800,
            "created_at": "2026-11-20T00:00:00Z",
        });
        if let Some(expires) = expires {
            lease["expires_at"] = json!(format_time(expires));
        }
        if state == "ready" {
            lease["ssh"] =
                json!({"host_alias": format!("agentspace-agentspace-{n:08x}"), "user": "ubuntu"});
        }
        lease
    }

    impl World {
        fn seed(&self, hosts: &[Value], leases: &[Value]) {
            std::fs::write(
                self.state_dir().join("hosts.json"),
                json!({"version": 0, "hosts": hosts}).to_string(),
            )
            .unwrap();
            std::fs::write(
                self.state_dir().join("leases.json"),
                json!({"version": 0, "leases": leases}).to_string(),
            )
            .unwrap();
        }
    }

    /// An AWS whose `describe-instances` answers every `(id, state, launched)` and whose other
    /// calls succeed.
    fn aws_of(instances: &[(&str, &str, i64)]) -> FakeAws {
        let instances: Vec<Value> = instances
            .iter()
            .map(|(id, state, launched)| {
                json!({"InstanceId": id, "State": {"Name": state}, "LaunchTime": format_time(*launched)})
            })
            .collect();
        FakeAws {
            calls: RefCell::default(),
            answer: Box::new(move |line| {
                if line.starts_with("ec2 describe-instances") {
                    Ok(json!({"Reservations": [{"Instances": instances.clone()}]}))
                } else {
                    Ok(Value::Null)
                }
            }),
        }
    }

    /// The calls that name `id`, as `service operation`.
    fn calls_for(aws: &FakeAws, id: &str) -> Vec<String> {
        aws.calls
            .borrow()
            .iter()
            .filter(|c| c.split(' ').any(|word| word == id))
            .map(|c| c.split(' ').take(2).collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn a_running_host_whose_project_holds_only_leases_on_another_platform_is_unexplained() {
        let w = World::new();
        // A live lease of agentspace on `linux/arm64` alone: the windows and `linux/amd64` hosts of
        // the project are running, launched a minute ago, and explained by nothing.
        w.seed(
            &four_hosts(),
            &[lease_on("linux/arm64", "ready", Some(NOW + 1800))],
        );
        let launched = NOW - 60;
        let aws = aws_of(&[
            (BOX_ID, "stopped", launched),
            (ARM_ID, "running", NOW - 100_000),
            (WIN_ID, "running", launched),
            (AMD_ID, "running", launched),
        ]);
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(
            (
                report.hosts,
                report.running,
                report.stopped,
                report.unexplained
            ),
            (4, 3, 0, 2)
        );
        // Each is named by its host name, in the registry's order.
        let progress = say.progress.borrow();
        assert!(progress[0].starts_with(&format!(
            "The host of '{WIN_HOST}' ({WIN_ID}) has no live lease; it stops at "
        )));
        assert!(progress[1].starts_with(&format!(
            "The host of '{AMD_HOST}' ({AMD_ID}) has no live lease; it stops at "
        )));
        assert_eq!(
            progress[2],
            "Hosts: 4; running 3, stopped 0, unexplained 2."
        );
        let log = w.log();
        assert_eq!(log.len(), 2, "{log:?}");
        assert!(log[0].ends_with(&format!(
            "Z host={WIN_HOST} unexplained since={} stops={}",
            format_time(launched),
            format_time(launched + GRACE_SECONDS)
        )));
        assert!(log[1].contains(&format!(" host={AMD_HOST} unexplained since=")));
    }

    #[test]
    fn a_due_host_is_stopped_when_only_another_platforms_lease_is_live() {
        let w = World::new();
        w.seed(
            &four_hosts(),
            &[lease_on("linux/arm64", "ready", Some(NOW + 1800))],
        );
        let old = NOW - 100_000;
        let aws = aws_of(&[
            (BOX_ID, "stopped", old),
            (ARM_ID, "running", old),
            (WIN_ID, "running", old),
            (AMD_ID, "stopped", old),
        ]);
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!((report.running, report.stopped, report.failed), (2, 1, 0));
        assert_eq!(calls_for(&aws, WIN_ID), ["ec2 stop-instances"]);
        // The arm64 host is explained: never stopped, and sent its horizon.
        assert_eq!(calls_for(&aws, ARM_ID), ["ssm send-command"]);
        assert_eq!(
            say.progress.borrow()[0],
            format!("Stopping the host of '{WIN_HOST}' ({WIN_ID}): no live lease explains it.")
        );
        let log = w.log();
        assert_eq!(log.len(), 1);
        assert!(log[0].ends_with(&format!(
            "Z host={WIN_HOST} stopped since={}",
            format_time(old)
        )));
    }

    #[test]
    fn a_live_lease_on_each_platform_explains_each_host_and_logs_nothing() {
        let w = World::new();
        w.seed(
            &four_hosts(),
            &[
                lease_on("linux/arm64", "ready", Some(NOW + 1800)),
                lease_on("windows/amd64", "pending", None),
                lease_on("linux/amd64", "ready", Some(NOW + 900)),
            ],
        );
        let old = NOW - 100_000;
        let aws = aws_of(&[
            (BOX_ID, "stopped", old),
            (ARM_ID, "running", old),
            (WIN_ID, "running", old),
            (AMD_ID, "running", old),
        ]);
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(
            (
                report.running,
                report.stopped,
                report.unexplained,
                report.failed
            ),
            (3, 0, 0, 0)
        );
        assert!(w.log().is_empty());
        assert!(
            !aws.calls
                .borrow()
                .iter()
                .any(|c| c.starts_with("ec2 stop-instances"))
        );
    }

    #[test]
    fn each_platforms_host_is_sent_through_its_own_platforms_rows() {
        let w = World::new();
        w.seed(
            &four_hosts(),
            &[
                lease_on("linux/arm64", "ready", Some(NOW + 1800)),
                lease_on("windows/amd64", "ready", Some(NOW + 600)),
                lease_on("linux/amd64", "ready", Some(NOW + 900)),
            ],
        );
        let old = NOW - 100_000;
        let aws = aws_of(&[
            (BOX_ID, "stopped", old),
            (ARM_ID, "running", old),
            (WIN_ID, "running", old),
            (AMD_ID, "running", old),
        ]);
        let say = FakeSay::default();
        w.run(&say, &aws).unwrap();
        // Both Linux platforms go through today's shell document, with their own horizon.
        let send_to = |id: &str| -> String {
            aws.calls
                .borrow()
                .iter()
                .find(|c| c.starts_with("ssm send-command") && c.split(' ').any(|word| word == id))
                .cloned()
                .unwrap_or_default()
        };
        let arm = send_to(ARM_ID);
        assert!(arm.contains("AWS-RunShellScript"), "{arm}");
        assert!(
            arm.split_whitespace()
                .any(|word| word == (NOW + 1800).to_string())
        );
        let amd = send_to(AMD_ID);
        assert!(amd.contains("AWS-RunShellScript"), "{amd}");
        assert!(
            amd.split_whitespace()
                .any(|word| word == (NOW + 900).to_string())
        );
        // The Windows host goes through its own rows: the PowerShell document, its own horizon
        // command, and `--parameters` as the JSON value, never the shorthand.
        let win = send_to(WIN_ID);
        assert!(
            win.contains(" --document-name AWS-RunPowerShellScript "),
            "{win}"
        );
        let command = format!("{} {}", STAND_IN_HORIZON, NOW + 600);
        assert!(
            win.ends_with(&format!(" --parameters {{\"commands\":[\"{command}\"]}}")),
            "{win}"
        );
        assert!(!win.contains("commands="), "{win}");
    }

    #[test]
    fn a_hosts_provisioning_lock_is_its_own_platforms_folder() {
        let old = NOW - 100_000;
        let instances = [
            (BOX_ID, "stopped", old),
            (ARM_ID, "stopped", old),
            (WIN_ID, "running", old),
            (AMD_ID, "stopped", old),
        ];
        // A run on the `linux/arm64` host's lock does not hold the windows host.
        let w = World::new();
        w.seed(&four_hosts(), &[]);
        let _arm =
            agentspace_hosts::try_run_lock_keyed(&w.ctx.home, &HostKey::linux_arm64("agentspace"))
                .unwrap()
                .unwrap();
        let aws = aws_of(&instances);
        let report = w.run(&FakeSay::default(), &aws).unwrap();
        assert_eq!((report.stopped, report.unexplained), (1, 0));
        assert_eq!(calls_for(&aws, WIN_ID), ["ec2 stop-instances"]);
        // A run on the windows host's own lock holds it.
        let w = World::new();
        w.seed(&four_hosts(), &[]);
        let _win = agentspace_hosts::try_run_lock_keyed(
            &w.ctx.home,
            &HostKey::new("agentspace", "windows/amd64"),
        )
        .unwrap()
        .unwrap();
        let aws = aws_of(&instances);
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!((report.stopped, report.unexplained), (0, 1));
        // Not stopped, and sent its horizon through its own rows, as a provisioning host is.
        assert_eq!(calls_for(&aws, WIN_ID), ["ssm send-command"]);
        assert_eq!(
            say.progress.borrow()[0],
            format!(
                "The host of '{WIN_HOST}' ({WIN_ID}) has no live lease, but a provisioning run is working on it; it is not stopped."
            )
        );
        assert!(w.log()[0].ends_with(&format!(
            " host={WIN_HOST} unexplained since={} stops={} provisioning=running",
            format_time(old),
            format_time(old + GRACE_SECONDS)
        )));
    }

    #[test]
    fn a_failed_part_names_a_host_off_arm64_by_its_host_name() {
        let w = World::new();
        w.seed(&four_hosts(), &[]);
        let old = NOW - 100_000;
        // The windows host is missing from the answer.
        let aws = aws_of(&[
            (BOX_ID, "stopped", old),
            (ARM_ID, "stopped", old),
            (AMD_ID, "stopped", old),
        ]);
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(
            *say.warnings.borrow(),
            [format!(
                "agentspace: Host '{WIN_HOST}': Instance {WIN_ID} was not found."
            )]
        );
        let log = w.log();
        assert_eq!(log.len(), 1);
        assert!(log[0].ends_with(&format!(
            "Z host={WIN_HOST} failed step=instance reason=Instance {WIN_ID} was not found."
        )));
        // The same failure for the arm64 host keeps its project as its name.
        let w = World::new();
        w.seed(&four_hosts(), &[]);
        let aws = aws_of(&[
            (BOX_ID, "stopped", old),
            (WIN_ID, "stopped", old),
            (AMD_ID, "stopped", old),
        ]);
        let say = FakeSay::default();
        w.run(&say, &aws).unwrap();
        assert_eq!(
            *say.warnings.borrow(),
            [format!(
                "agentspace: Host '{ARM_HOST}': Instance {ARM_ID} was not found."
            )]
        );
    }

    #[test]
    fn a_host_that_fails_to_send_its_horizon_is_named_by_its_host_name() {
        let w = World::new();
        w.seed(
            &four_hosts(),
            &[lease_on("linux/amd64", "ready", Some(NOW + 900))],
        );
        let old = NOW - 100_000;
        let mut aws = aws_of(&[
            (BOX_ID, "stopped", old),
            (ARM_ID, "stopped", old),
            (WIN_ID, "stopped", old),
            (AMD_ID, "running", old),
        ]);
        let describe = aws.answer;
        aws.answer = Box::new(move |line| {
            if line.starts_with("ssm send-command") {
                Err(AwsError::Failed("InvalidInstanceId".to_string()))
            } else {
                describe(line)
            }
        });
        let say = FakeSay::default();
        let report = w.run(&say, &aws).unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(
            *say.warnings.borrow(),
            [format!(
                "agentspace: Host '{AMD_HOST}': Could not send the lease horizon to {AMD_ID}: InvalidInstanceId"
            )]
        );
        assert!(w.log()[0].ends_with(&format!(
            "Z host={AMD_HOST} failed step=horizon reason=Could not send the lease horizon to {AMD_ID}: InvalidInstanceId"
        )));
    }

    /// A stand-in platform whose only rows the horizon send reads are its Session Manager document
    /// and its horizon command, `<STAND_IN_HORIZON> <horizon>`.
    struct StandIn {
        platform: &'static str,
        document: &'static str,
    }

    /// The stand-in's horizon command, before the horizon's digits.
    const STAND_IN_HORIZON: &str = "stand-in horizon";

    /// The stand-in for `windows/amd64` in every flow test: the PowerShell document, and a command
    /// that holds a space, so the JSON value's form is the one that carries it whole.
    static STAND_IN_WINDOWS: StandIn = StandIn {
        platform: "windows/amd64",
        document: "AWS-RunPowerShellScript",
    };

    /// The flow tests' pick: the stand-in for `windows/amd64`, and `for_platform`'s real rows for
    /// every other platform, so both Linux platforms send today's text.
    fn stand_in_pick(platform: &str) -> Option<&'static dyn HostPlatform> {
        match platform {
            "windows/amd64" => Some(&STAND_IN_WINDOWS),
            other => agentspace_platforms::for_platform(other),
        }
    }

    impl HostPlatform for StandIn {
        fn platform(&self) -> &'static str {
            self.platform
        }
        fn image_parameter(&self) -> Option<&'static str> {
            None
        }
        fn new_stack_instance_type(&self) -> Option<&'static str> {
            None
        }
        fn new_stack_extra_parameters(&self) -> &'static [&'static str] {
            &[]
        }
        fn login_user(&self) -> &'static str {
            "agent"
        }
        fn ssm_document(&self) -> &'static str {
            self.document
        }
        fn remote_command(&self, script: &str) -> Result<String, Failure> {
            Ok(script.to_string())
        }
        fn bootstrap(&self) -> Result<agentspace_operator_api::Bootstrap, Failure> {
            Err(failure("not used".to_string()))
        }
        fn sync_plan(&self) -> Result<agentspace_operator_api::SyncPlan, Failure> {
            Err(failure("not used".to_string()))
        }
        fn patch_check_command(&self) -> Result<String, Failure> {
            Err(failure("not used".to_string()))
        }
        fn patch_messages(&self) -> agentspace_operator_api::PatchMessages {
            agentspace_operator_api::PatchMessages {
                phase: String::new(),
                log_command: String::new(),
            }
        }
        fn horizon_command(&self, horizon: u64) -> Result<String, Failure> {
            Ok(format!("{STAND_IN_HORIZON} {horizon}"))
        }
        fn grow_script(&self) -> Result<String, Failure> {
            Err(failure("not used".to_string()))
        }
        fn push_zone(&self, _windows_zone_id: &str, _iana: &str) -> Result<String, Failure> {
            Err(failure("not used".to_string()))
        }
        fn push_script(&self, _zone: &str) -> Result<String, Failure> {
            Err(failure("not used".to_string()))
        }
    }

    #[test]
    fn the_horizon_send_is_the_platforms_rows_and_a_platform_with_none_fails_with_no_call() {
        // Both Linux platforms' real rows: today's send, byte for byte.
        for platform in ["linux/arm64", "linux/amd64"] {
            let aws = aws(Value::Null, ok);
            send_horizon_for(
                &aws,
                agentspace_platforms::for_platform(platform),
                platform,
                ID,
                7,
            )
            .unwrap();
            let direct = aws_of(&[]);
            agentspace_box::send_horizon(&direct, ID, 7).unwrap();
            assert_eq!(*aws.calls.borrow(), *direct.calls.borrow(), "{platform}");
            assert_eq!(operations(&aws), ["ssm send-command"], "{platform}");
        }
        // Another document: its own command, as the JSON value.
        let aws = aws_of(&[]);
        send_horizon_for(&aws, Some(&STAND_IN_WINDOWS), "windows/amd64", ID, 7).unwrap();
        assert_eq!(
            *aws.calls.borrow(),
            [format!(
                "ssm send-command --instance-ids {ID} --document-name AWS-RunPowerShellScript --comment agentspace guard: lease horizon --parameters {{\"commands\":[\"{STAND_IN_HORIZON} 7\"]}}"
            )]
        );
        // A platform with no rows (pin P10): no call, and the failure names the platform.
        let aws = aws_of(&[]);
        let failed = send_horizon_for(&aws, None, "windows/arm64", ID, 7).unwrap_err();
        assert_eq!(
            failed.message,
            "agentspace: No Session Manager document or horizon command exists for the platform windows/arm64."
        );
        assert!(!is_sign_in(&failed));
        assert!(aws.calls.borrow().is_empty());
    }

    #[test]
    fn the_real_rows_of_each_supported_platform_pick_their_documents() {
        let document =
            |platform: &str| agentspace_platforms::for_platform(platform).map(|p| p.ssm_document());
        assert_eq!(document("linux/arm64"), Some("AWS-RunShellScript"));
        assert_eq!(document("linux/amd64"), Some("AWS-RunShellScript"));
        assert_eq!(document("windows/amd64"), Some("AWS-RunPowerShellScript"));
        assert_eq!(document("windows/arm64"), None);
    }

    /// Pin P10 (operator.md, guard, step 7): a record whose platform `for_platform` does not know
    /// is decided as any host is. Due and unexplained, it is stopped, so it never runs unbounded.
    /// Running and explained, its horizon send fails its part with the failed-send line, naming the
    /// host by its host name. The run is the release one, `execute`, whose rows are
    /// `for_platform`'s alone.
    #[test]
    fn a_record_whose_platform_has_no_seam_is_stopped_when_due_and_its_send_fails_its_part() {
        const ODD_ID: &str = "i-0123456789abcdef7";
        const ODD_HOST: &str = "agentspace.windows-arm64";
        let records = vec![
            record("swimcoach", None, "agentbox", BOX_ID),
            record(
                "agentspace",
                Some("windows/arm64"),
                "agentspace-windows-arm64-host-agentspace",
                ODD_ID,
            ),
        ];
        let old = NOW - 100_000;
        let release = |w: &World, say: &FakeSay, aws: &FakeAws| {
            let state_dir = w.state_dir();
            execute(
                &Input {
                    ctx: &w.ctx,
                    state_dir: &state_dir,
                    now: NOW,
                },
                say,
                aws,
            )
        };

        // No live lease, and due: stopped, and sent nothing.
        let w = World::new();
        w.seed(&records, &[]);
        let aws = aws_of(&[(BOX_ID, "stopped", old), (ODD_ID, "running", old)]);
        let say = FakeSay::default();
        let report = release(&w, &say, &aws).unwrap();
        assert_eq!((report.running, report.stopped, report.failed), (1, 1, 0));
        assert_eq!(calls_for(&aws, ODD_ID), ["ec2 stop-instances"]);
        assert_eq!(
            say.progress.borrow()[0],
            format!("Stopping the host of '{ODD_HOST}' ({ODD_ID}): no live lease explains it.")
        );

        // A live lease on its platform explains it: not stopped, and its send fails its part.
        let w = World::new();
        w.seed(
            &records,
            &[lease_on("windows/arm64", "ready", Some(NOW + 1800))],
        );
        let aws = aws_of(&[(BOX_ID, "stopped", old), (ODD_ID, "running", old)]);
        let say = FakeSay::default();
        let report = release(&w, &say, &aws).unwrap();
        assert_eq!((report.running, report.stopped, report.failed), (1, 0, 1));
        assert!(calls_for(&aws, ODD_ID).is_empty(), "No call names it");
        let line = format!(
            "Could not send the lease horizon to {ODD_ID}: No Session Manager document or horizon command exists for the platform windows/arm64."
        );
        assert_eq!(
            *say.warnings.borrow(),
            [format!("agentspace: Host '{ODD_HOST}': {line}")]
        );
        let log = w.log();
        assert_eq!(log.len(), 1, "{log:?}");
        assert!(log[0].ends_with(&format!(
            "Z host={ODD_HOST} failed step=horizon reason={line}"
        )));
    }
}

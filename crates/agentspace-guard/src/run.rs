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

use agentspace_box::{Described, describe_hosts, instance_id_from_stack, send_horizon, stop_host};
use agentspace_hosts::{
    HostState, Hosts, HostsError, guard_lock_path, host_run_lock_path, hosts_path, run_is_running,
    try_guard_lock,
};
use agentspace_operator_api::{Aws, CallerFailure, Failure, SIGN_IN_MESSAGE, Say};
use agentspace_state::{StoreError, format_time};

use crate::decide::{Action, HostToDecide, ProvisionLock, decide};
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
        HostsError::CannotLockRun(why) | HostsError::CannotLockGuard(why) => why.to_string(),
        HostsError::Busy => "the registry is in use by another command".to_string(),
        HostsError::BadProject => "a project is not in its normalized form".to_string(),
    };
    text.replace(['\r', '\n'], " ")
}

/// The lease state file's failure and its `<reason>` (operator.md, guard, step 3).
fn leases_failure(path: &Path, why: &str) -> Failure {
    failure(format!(
        "agentspace: The lease state file {} cannot be read: {why}.",
        path.display()
    ))
}

/// The time of a stop as a progress line prints it, `<h:mm><am|pm>`: the laptop's local time,
/// from [`agentspace_zone::local_clock_time`], the one formatter of the guard's stop time
/// (operator.md, guard, step 6; architect, 2026-11-22, for v0.08-T03). When the system gives no
/// offset for the instant (always off Windows), the time is printed in UTC, as
/// [`agentspace_zone::clock_time_at`] with offset 0, so a line is never printed with no time.
fn stop_time(unix: i64) -> String {
    agentspace_zone::local_clock_time(unix)
        .unwrap_or_else(|| agentspace_zone::clock_time_at(unix, 0))
}

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

    /// A line about one host in the log, `<time> host=<project> <text>`.
    fn log_host(&self, project: &str, text: &str) {
        self.log.host(project, text);
        self.note_drop();
    }

    /// A host's part fails (operator.md, guard, A host's part that fails): its message through
    /// [`Say::warning`] as `agentspace: Host '<project>': <message>`, one `failed` line in the log,
    /// and the run goes on. `message` has no `agentspace: ` prefix.
    fn host_failed(&mut self, index: usize, project: &str, step: &str, message: &str) {
        let message = bare(message);
        self.say
            .warning(&format!("agentspace: Host '{project}': {message}"));
        self.log.host_failed(project, step, message);
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
            let why = match &e {
                HostsError::CannotLockGuard(why) => why.to_string(),
                other => hosts_reason(other),
            };
            let f = failure(format!(
                "agentspace: Cannot take {}: {why}.",
                guard_lock_path(home).display()
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
    let leases = match agentspace_state::read(input.state_dir) {
        Ok(file) => {
            if file.leases.is_empty()
                && matches!(std::fs::metadata(&leases_path), Err(ref e) if e.kind() == std::io::ErrorKind::NotFound)
            {
                let f = leases_failure(&leases_path, "the file does not exist");
                return Err(run.ends("leases", f));
            }
            file.leases
        }
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
    let mut to_decide: Vec<HostToDecide> = Vec::new();
    let mut decided: Vec<usize> = Vec::new();
    for (index, host) in hosts.hosts.iter().enumerate() {
        let id = match &host.instance_id {
            Some(id) => id.clone(),
            None if host.state == HostState::Ready => {
                match instance_id_from_stack(aws, &host.stack) {
                    Ok(id) => id,
                    Err(f) if is_sign_in(&f) => return Err(run.sign_in("instance", f)),
                    Err(f) => {
                        run.host_failed(index, &host.project, "instance", &f.message);
                        continue;
                    }
                }
            }
            // A `creating` or `failed` record with no ID: no instance of it is known.
            None => continue,
        };
        to_decide.push(HostToDecide {
            project: host.project.clone(),
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
    let provision_held = |project: &str| match run_is_running(home, project) {
        Ok(false) => ProvisionLock::Free,
        Ok(true) => ProvisionLock::Held,
        Err(e) => ProvisionLock::NotTried {
            path: host_run_lock_path(home, project).display().to_string(),
            error: hosts_reason(&e),
        },
    };
    let actions = decide(&to_decide, &leases, &described, &provision_held, now);

    let (mut running, mut stopped, mut unexplained) = (0usize, 0usize, 0usize);
    // Each running host the run did not stop, for step 7: its registry index, project, ID, horizon.
    let mut sends: Vec<(usize, String, String, i64)> = Vec::new();
    for (&index, action) in decided.iter().zip(actions) {
        match action {
            Action::Nothing { .. } => {}
            Action::NotFound {
                project,
                instance_id,
            } => {
                let message = format!("Instance {instance_id} was not found.");
                run.host_failed(index, &project, "instance", &message);
            }
            Action::Unexpected {
                project, member, ..
            } => {
                // `State.Name` is read for every host; `LaunchTime` for a running one alone.
                if member == "LaunchTime" {
                    running += 1;
                }
                let message = format!(
                    "Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].{member}."
                );
                run.host_failed(index, &project, "instance", &message);
            }
            Action::Explained {
                project,
                instance_id,
                horizon,
            } => {
                running += 1;
                sends.push((index, project, instance_id, horizon));
            }
            Action::Unexplained {
                project,
                instance_id,
                stop_at,
                horizon,
            } => {
                running += 1;
                unexplained += 1;
                run.log_host(
                    &project,
                    &format!(
                        "unexplained since={} stops={}",
                        format_time(stop_at - GRACE_SECONDS),
                        format_time(stop_at)
                    ),
                );
                say.progress(
                    &format!(
                        "The host of '{project}' ({instance_id}) has no live lease; it stops at {}.",
                        stop_time(stop_at)
                    ),
                    None,
                );
                sends.push((index, project, instance_id, horizon));
            }
            Action::Provisioning {
                project,
                instance_id,
                stop_at,
                horizon,
            } => {
                running += 1;
                unexplained += 1;
                run.log_host(
                    &project,
                    &format!(
                        "unexplained since={} stops={} provisioning=running",
                        format_time(stop_at - GRACE_SECONDS),
                        format_time(stop_at)
                    ),
                );
                say.progress(
                    &format!(
                        "The host of '{project}' ({instance_id}) has no live lease, but a provisioning run is working on it; it is not stopped."
                    ),
                    None,
                );
                sends.push((index, project, instance_id, horizon));
            }
            Action::LockNotTried {
                project,
                instance_id,
                path,
                error,
                horizon,
            } => {
                running += 1;
                let message = format!("Cannot try {path}: {error}.");
                run.host_failed(index, &project, "lock", &message);
                sends.push((index, project, instance_id, horizon));
            }
            Action::Stop {
                project,
                instance_id,
                stop_at,
                horizon,
            } => {
                running += 1;
                match stop_host(aws, &instance_id) {
                    Ok(()) => {
                        stopped += 1;
                        run.log_host(
                            &project,
                            &format!("stopped since={}", format_time(stop_at - GRACE_SECONDS)),
                        );
                        say.progress(
                            &format!(
                                "Stopping the host of '{project}' ({instance_id}): no live lease explains it."
                            ),
                            None,
                        );
                    }
                    Err(f) if is_sign_in(&f) => return Err(run.sign_in("stop", f)),
                    Err(f) => {
                        let message = format!("Could not stop {instance_id}: {}", bare(&f.message));
                        run.host_failed(index, &project, "stop", &message);
                        sends.push((index, project, instance_id, horizon));
                    }
                }
            }
        }
    }

    // Step 7: every running host the run did not stop is sent its horizon, in the registry's
    // order. A horizon sent is not logged and prints nothing.
    for (index, project, instance_id, horizon) in sends {
        match send_horizon(aws, &instance_id, horizon) {
            Ok(()) => {}
            Err(f) if is_sign_in(&f) => return Err(run.sign_in("horizon", f)),
            Err(f) => {
                let message = format!(
                    "Could not send the lease horizon to {instance_id}: {}",
                    bare(&f.message)
                );
                run.host_failed(index, &project, "horizon", &message);
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
        // the same instant in UTC.
        let minute = 1_795_075_920;
        for unix in [minute, minute + 1, minute + 59, minute + 60, 1_780_000_020] {
            let shown = stop_time(unix);
            let expected = agentspace_zone::local_clock_time(unix)
                .unwrap_or_else(|| agentspace_zone::clock_time_at(unix, 0));
            assert_eq!(shown, expected);
            assert!(shown.ends_with("am") || shown.ends_with("pm"), "{shown}");
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

        fn run(&self, say: &FakeSay, aws: &FakeAws) -> Result<Report, Failure> {
            let state_dir = self.state_dir();
            execute(
                &Input {
                    ctx: &self.ctx,
                    state_dir: &state_dir,
                    now: NOW,
                },
                say,
                aws,
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
        assert!(
            f.message.starts_with("agentspace: The host registry "),
            "{f:?}"
        );
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
                    && f.message.contains(" cannot be read: "),
                "{f:?}"
            );
            if content.is_none() {
                assert!(f.message.ends_with(": the file does not exist."), "{f:?}");
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
}

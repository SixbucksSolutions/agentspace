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
//! through [`Say::warning`] as `agentspace: Host '<project>': <message>`**, because a library
//! prints only through [`Say`] and no other method of it prints a line a person reads. `-Quiet`
//! MUST NOT withhold a failure (operator.md, guard, Its streams), so the [`Say`] the operator
//! hands the guard MUST print `warning` whatever `-Quiet` says, and suppress `progress` alone.

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

/// The time of a stop as a progress line prints it, `<h:mm><am|pm>`: the laptop's local time, read
/// from the system now, with seconds rounded up to the next whole minute (operator.md, guard,
/// step 6).
fn local_time(unix: i64) -> String {
    let rounded = if unix.rem_euclid(60) == 0 {
        unix
    } else {
        unix + (60 - unix.rem_euclid(60))
    };
    let (hour, minute) = local::hour_and_minute(rounded);
    twelve_hour(hour, minute)
}

/// `h:mm` and `am` or `pm`: `12:34pm`, `5:19am`, `12:05am` after midnight.
fn twelve_hour(hour: u32, minute: u32) -> String {
    let h = match hour % 12 {
        0 => 12,
        h => h,
    };
    let suffix = if hour < 12 { "am" } else { "pm" };
    format!("{h}:{minute:02}{suffix}")
}

/// The laptop's local time of an instant, which only Windows can give (the guard runs on the
/// laptop): the system's zone of the moment, with the rule of that date applied. Anywhere else the
/// zone is UTC.
#[allow(unsafe_code)]
mod local {
    use agentspace_state::format_time;

    /// The hour (0 to 23) and minute of `unix` in the zone the system is set to now.
    pub(super) fn hour_and_minute(unix: i64) -> (u32, u32) {
        let utc = format_time(unix);
        let (hour, minute) = (field(&utc, 11), field(&utc, 14));
        #[cfg(windows)]
        if let Some(local) = to_local(&utc) {
            return local;
        }
        (hour, minute)
    }

    /// The two digits of `text` at `at`, which `format_time`'s form guarantees.
    fn field(text: &str, at: usize) -> u32 {
        text.get(at..at + 2)
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(0)
    }

    #[cfg(windows)]
    fn to_local(utc: &str) -> Option<(u32, u32)> {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTime;

        let universal = SYSTEMTIME {
            wYear: u16::try_from(field(utc, 0) * 100 + field(utc, 2)).ok()?,
            wMonth: u16::try_from(field(utc, 5)).ok()?,
            wDayOfWeek: 0,
            wDay: u16::try_from(field(utc, 8)).ok()?,
            wHour: u16::try_from(field(utc, 11)).ok()?,
            wMinute: u16::try_from(field(utc, 14)).ok()?,
            wSecond: u16::try_from(field(utc, 17)).ok()?,
            wMilliseconds: 0,
        };
        let mut local = SYSTEMTIME {
            wYear: 0,
            wMonth: 0,
            wDayOfWeek: 0,
            wDay: 0,
            wHour: 0,
            wMinute: 0,
            wSecond: 0,
            wMilliseconds: 0,
        };
        // SAFETY: A null zone means the zone the system is set to now. `universal` and `local` are
        // valid SYSTEMTIME values for the call to read and to fill.
        let done =
            unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &universal, &mut local) };
        (done != 0).then_some((u32::from(local.wHour), u32::from(local.wMinute)))
    }
}

/// The run's state: its streams, its log, and the hosts whose part failed.
struct Run<'a> {
    say: &'a dyn Say,
    log: GuardLog,
    /// The registry's index of each host whose part failed, once each.
    failed: BTreeSet<usize>,
}

impl Run<'_> {
    /// A host's part fails (operator.md, guard, A host's part that fails): its message to stderr
    /// as `agentspace: Host '<project>': <message>`, one `failed` line in the log, and the run goes
    /// on. `message` has no `agentspace: ` prefix.
    fn host_failed(&mut self, index: usize, project: &str, step: &str, message: &str) {
        let message = bare(message);
        self.say
            .warning(&format!("agentspace: Host '{project}': {message}"));
        self.log.host_failed(project, step, message);
        self.failed.insert(index);
    }

    /// A call met #0034's sign-in message: it is logged without a `host=` field, as the step of the
    /// call that met it, and ends the run.
    fn sign_in(&self, step: &str, failure: Failure) -> Failure {
        self.log.run_failed(step, &failure.message);
        failure
    }

    /// A failure that ends the run: logged without a `host=` field, and returned.
    fn ends(&self, step: &str, failure: Failure) -> Failure {
        self.log.run_failed(step, &failure.message);
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
                run.log.host(
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
                        local_time(stop_at)
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
                run.log.host(
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
                        run.log.host(
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

    // Step 8: the closing line. A line the log dropped is a run that has not succeeded (D36).
    say.progress(
        &format!(
            "Hosts: {}; running {running}, stopped {stopped}, unexplained {unexplained}.",
            hosts.hosts.len()
        ),
        None,
    );
    if let Some(error) = run.log.dropped() {
        return Err(failure(format!(
            "agentspace: Cannot write the guard's log {}: {error}.",
            run.log.path().display()
        )));
    }
    Ok(Report {
        lock_held: false,
        hosts: hosts.hosts.len(),
        running,
        stopped,
        unexplained,
        failed: run.failed.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_twelve_hour_clock_has_no_leading_zero_and_noon_and_midnight_are_twelve() {
        assert_eq!(twelve_hour(0, 5), "12:05am");
        assert_eq!(twelve_hour(5, 19), "5:19am");
        assert_eq!(twelve_hour(11, 59), "11:59am");
        assert_eq!(twelve_hour(12, 34), "12:34pm");
        assert_eq!(twelve_hour(13, 0), "1:00pm");
        assert_eq!(twelve_hour(23, 59), "11:59pm");
    }

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
    fn a_stop_time_with_seconds_is_rounded_up_to_the_next_minute() {
        // The zone is the system's, so the check is against the same conversion of the rounded
        // minute: a time one second past a minute reads as the next minute.
        let minute = 1_795_075_920;
        assert_eq!(local_time(minute), local_time(minute));
        assert_eq!(local_time(minute + 1), local_time(minute + 60));
        assert_eq!(local_time(minute + 59), local_time(minute + 60));
        assert_ne!(local_time(minute + 1), local_time(minute + 120));
    }

    #[test]
    fn the_local_time_is_a_real_zone_offset_of_the_instant() {
        // Every zone's offset is a multiple of 15 minutes, so the minute of the hour is the UTC
        // minute's modulo 15, and the hour and minute are in range.
        for unix in [1_795_075_920_i64, 1_795_075_920 + 7 * 60, 1_780_000_020] {
            let (hour, minute) = local::hour_and_minute(unix);
            assert!(hour < 24 && minute < 60);
            assert_eq!(minute % 15, unix.rem_euclid(3600) as u32 / 60 % 15);
        }
        let shown = local_time(1_795_075_921);
        assert!(shown.ends_with("am") || shown.ends_with("pm"), "{shown}");
        assert!(shown.contains(':') && !shown.starts_with('0'), "{shown}");
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
                failed: 0
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
    fn a_dropped_log_line_makes_the_run_fail_without_changing_what_it_did() {
        let w = World::new();
        std::fs::create_dir_all(w.state_dir().join("guard.log")).unwrap();
        let (say, aws) = (FakeSay::default(), aws(running(NOW - 3600), ok));
        let f = w.run(&say, &aws).unwrap_err();
        assert!(
            f.message
                .starts_with("agentspace: Cannot write the guard's log ")
        );
        assert_eq!(operations(&aws).last().unwrap(), "ec2 stop-instances");
        assert!(
            say.progress
                .borrow()
                .iter()
                .any(|l| l.starts_with("Hosts:"))
        );
    }
}

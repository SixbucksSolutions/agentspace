//! `guard`'s dispatch (docs/operator.md, guard; D56, D63): what the operator reads and prints around
//! the guard's run, which is `agentspace-guard`'s.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The operator resolves, once, what the run reads beside the context: the state folder and the
//! run's time, the UTC clock to the whole second. It calls the run with a [`Say`] built here
//! ([`GuardSay`]): its `progress` is `-Quiet`'s own (a quiet [`Say`] suppresses the closing
//! `Hosts:` line the run prints through it), and its `warning` is **the guard's failure channel**,
//! which prints whatever `-Quiet` says, since every failure always prints (operator.md, guard,
//! Its streams, exit codes and help; architect, 2026-11-22, for v0.08-T03). The run gets the
//! pinned [`Aws`] too. The operator prints nothing but the count line of a run in which hosts'
//! parts failed, and exits by the report: 0 when no part failed and no log line was dropped, a
//! lock another program holds included, and 1 otherwise. A failure that ends the run is printed
//! by the caller of [`guard`], as every command's failure is. A run that exits 1 with nothing more
//! to print, a log line dropped and no host failed, is [`Outcome::FailedAfterSaying`], never a
//! [`Failure`] with an empty message.
//!
//! **The seam:** [`Reach::guard`](crate::Reach::guard) is the run, and its default is [`agentspace_guard::run`]; a test's
//! [`Reach`](crate::Reach) overrides it with a stand-in and drives [`dispatch`](crate::dispatch) with a
//! `guard` request. [`guard_with`] is the same dispatch over any [`GuardRun`], for a test that
//! has no [`Reach`](crate::Reach).

use std::path::Path;

use agentspace_guard::{Input, Report};
use agentspace_operator_api::{Aws, CallerFailure, CallerLine, Context, Failure, Mode, Say};

use crate::status::unix_seconds;
use crate::{Outcome, Parts, Report as Printed};

/// A guard run: [`agentspace_guard::run`]'s signature, which a test's stand-in has too.
pub type GuardRun<'a> = dyn Fn(&Input, &dyn Say, &dyn Aws) -> Result<Report, Failure> + 'a;

/// The [`Say`] the guard's run is given. `progress` goes to the command's own [`Say`], so `-Quiet`
/// suppresses it. `warning`, which the run uses for a host's failure and for a dropped log line,
/// goes to the command's [`Printed`] channel, which `-Quiet` never suppresses (operator.md, guard,
/// Its streams, exit codes and help). The command's own [`Say`] is not changed, nor is
/// `agentspace-operator-api`'s.
pub struct GuardSay<'a> {
    say: &'a dyn Say,
    report: &'a dyn Printed,
}

impl<'a> GuardSay<'a> {
    /// The guard's [`Say`] over a command's `say` and its report channel.
    pub fn new(say: &'a dyn Say, report: &'a dyn Printed) -> GuardSay<'a> {
        GuardSay { say, report }
    }
}

impl Say for GuardSay<'_> {
    fn mode(&self) -> Mode {
        self.say.mode()
    }
    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        self.say.progress(person, caller);
    }
    fn warning(&self, person: &str) {
        self.report.report(person);
    }
    fn detail(&self, text: &str) {
        self.say.detail(text);
    }
}

/// `guard`, over `run`: one run at `now`, then the operator's closing (operator.md, guard, step 8).
///
/// - `Ok(report)` with no failed host and no dropped log line, `guard.lock` held by another program
///   included: `Ok(Outcome::Done)`, and nothing more is printed.
/// - `Ok(report)` with failed hosts, whether or not a log line was dropped: fails with
///   `agentspace: <n> of <m> hosts failed.`, `-All`'s line, `<m>` being the registry's records,
///   which the caller prints once and exits 1 for.
/// - `Ok(report)` with a dropped log line and no failed host: `Ok(Outcome::FailedAfterSaying)`,
///   since the run said why through `warning`: the caller prints nothing more and exits 1.
/// - `Err(failure)`: that failure, unchanged.
pub fn guard_with(
    run: &GuardRun<'_>,
    ctx: &Context,
    state_dir: &Path,
    now: i64,
    say: &dyn Say,
    aws: &dyn Aws,
) -> Result<Outcome, Failure> {
    let report = run(
        &Input {
            ctx,
            state_dir,
            now,
        },
        say,
        aws,
    )?;
    if report.failed > 0 {
        return Err(Failure {
            message: format!(
                "agentspace: {} of {} hosts failed.",
                report.failed, report.hosts
            ),
            caller: CallerFailure::Same,
        });
    }
    if report.log_dropped {
        return Ok(Outcome::FailedAfterSaying);
    }
    Ok(Outcome::Done)
}

/// `guard` as the dispatch runs it: [`guard_with`] over [`Reach::guard`](crate::Reach::guard) (the
/// real run unless a stand-in overrides it), the state folder
/// [`Reach::lease_state_dir`](crate::Reach::lease_state_dir) gives, the clock's reading read once,
/// and the [`GuardSay`] over the command's `say` and report channel.
pub fn guard(parts: &Parts) -> Result<Outcome, Failure> {
    let state_dir = parts.reach.lease_state_dir(parts.ctx);
    let now = unix_seconds(parts.clock.utc_now());
    let say = GuardSay::new(parts.say, parts.report);
    guard_with(
        &|input, say, aws| parts.reach.guard(input, say, aws),
        parts.ctx,
        &state_dir,
        now,
        &say,
        parts.aws,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_operator_api::{CallerLine, Mode};
    use std::cell::RefCell;
    use std::path::PathBuf;

    struct Mute;

    impl Say for Mute {
        fn mode(&self) -> Mode {
            Mode::Person
        }
        fn progress(&self, _: &str, _: Option<CallerLine>) {}
        fn warning(&self, _: &str) {}
        fn detail(&self, _: &str) {}
    }

    struct NoAws;

    impl Aws for NoAws {
        fn call(
            &self,
            _: &agentspace_operator_api::Call,
        ) -> Result<serde_json::Value, agentspace_operator_api::AwsError> {
            panic!("The dispatch makes no AWS call of its own");
        }
    }

    fn ctx() -> Context {
        Context {
            home: PathBuf::from("C:\\throwaway\\home"),
            local_app_data: None,
            system_root: PathBuf::from("C:\\throwaway\\root"),
            nested: false,
        }
    }

    #[test]
    fn the_run_gets_the_context_the_folder_and_the_time_and_a_clean_report_is_ok() {
        let seen = RefCell::new(None);
        let run = |input: &Input, _: &dyn Say, _: &dyn Aws| {
            *seen.borrow_mut() = Some((
                input.ctx.home.clone(),
                input.state_dir.to_path_buf(),
                input.now,
            ));
            Ok(Report {
                hosts: 3,
                ..Report::default()
            })
        };
        let context = ctx();
        let result = guard_with(
            &run,
            &context,
            Path::new("C:\\state"),
            1_700_000_000,
            &Mute,
            &NoAws,
        );
        assert_eq!(result, Ok(Outcome::Done));
        assert_eq!(
            seen.into_inner(),
            Some((
                PathBuf::from("C:\\throwaway\\home"),
                PathBuf::from("C:\\state"),
                1_700_000_000
            ))
        );
    }

    #[test]
    fn a_held_lock_is_exit_0_and_failed_hosts_are_the_count_line() {
        let context = ctx();
        let held = |_: &Input, _: &dyn Say, _: &dyn Aws| {
            Ok(Report {
                lock_held: true,
                ..Report::default()
            })
        };
        assert_eq!(
            guard_with(&held, &context, Path::new("s"), 0, &Mute, &NoAws),
            Ok(Outcome::Done)
        );
        let failed = |_: &Input, _: &dyn Say, _: &dyn Aws| {
            Ok(Report {
                hosts: 4,
                running: 2,
                failed: 1,
                ..Report::default()
            })
        };
        let failure = guard_with(&failed, &context, Path::new("s"), 0, &Mute, &NoAws).unwrap_err();
        assert_eq!(failure.message, "agentspace: 1 of 4 hosts failed.");
        assert_eq!(failure.caller, CallerFailure::Same);
    }

    #[test]
    fn a_dropped_log_line_is_exit_1_with_the_count_line_only_when_hosts_failed() {
        let context = ctx();
        let run = |report: Report| move |_: &Input, _: &dyn Say, _: &dyn Aws| Ok(report.clone());
        let go =
            |report: Report| guard_with(&run(report), &context, Path::new("s"), 0, &Mute, &NoAws);
        // Neither: exit 0. The lock held, with nothing else: exit 0.
        assert_eq!(go(Report::default()), Ok(Outcome::Done));
        let held = Report {
            lock_held: true,
            ..Report::default()
        };
        assert_eq!(go(held), Ok(Outcome::Done));
        // Failed only: the count line, once.
        let failed = Report {
            hosts: 5,
            failed: 2,
            ..Report::default()
        };
        assert_eq!(
            go(failed).unwrap_err().message,
            "agentspace: 2 of 5 hosts failed."
        );
        // A dropped line only: exit 1 with no line of the operator's own, so an outcome and not a
        // failure.
        let dropped = Report {
            hosts: 5,
            log_dropped: true,
            ..Report::default()
        };
        let outcome = go(dropped).expect("Not a failure: there is nothing to print");
        assert_eq!(outcome, Outcome::FailedAfterSaying);
        assert_eq!(outcome.exit_code(), 1);
        // Both: the count line, once, and exit 1.
        let both = Report {
            hosts: 5,
            failed: 1,
            log_dropped: true,
            ..Report::default()
        };
        let failure = go(both).unwrap_err();
        assert_eq!(failure.message, "agentspace: 1 of 5 hosts failed.");
    }

    #[test]
    fn the_runs_warning_prints_whatever_quiet_says_and_its_progress_does_not() {
        use crate::testing::FakeSay;
        for quiet in [true, false] {
            let fake = FakeSay::new(quiet);
            let say = GuardSay::new(&fake, &fake);
            say.progress("Hosts: 1; running 0, stopped 0, unexplained 0.", None);
            say.warning("agentspace: Host 'p': it failed.");
            let expected: &[&str] = if quiet {
                &["agentspace: Host 'p': it failed."]
            } else {
                &[
                    "Hosts: 1; running 0, stopped 0, unexplained 0.",
                    "agentspace: Host 'p': it failed.",
                ]
            };
            assert_eq!(fake.lines(), expected, "quiet {quiet}");
        }
    }

    #[test]
    fn a_failure_that_ends_the_run_is_passed_on_unchanged() {
        let context = ctx();
        let ended = |_: &Input, _: &dyn Say, _: &dyn Aws| {
            Err(Failure {
                message: "agentspace: Cannot take C:\\x\\guard.lock: access denied.".to_string(),
                caller: CallerFailure::CouldNotReach,
            })
        };
        let failure = guard_with(&ended, &context, Path::new("s"), 0, &Mute, &NoAws).unwrap_err();
        assert_eq!(
            failure.message,
            "agentspace: Cannot take C:\\x\\guard.lock: access denied."
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
    }

    #[test]
    fn the_outcomes_exit_codes_are_0_and_1() {
        assert_eq!(Outcome::Done.exit_code(), 0);
        assert_eq!(Outcome::FailedAfterSaying.exit_code(), 1);
    }
}

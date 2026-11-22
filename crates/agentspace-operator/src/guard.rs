//! `guard`'s dispatch (docs/operator.md, guard; D56, D63): what the operator reads and prints around
//! the guard's run, which is `agentspace-guard`'s.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The operator resolves, once, what the run reads beside the context: the state folder and the
//! run's time, the UTC clock to the whole second. It calls the run with the pinned [`Say`], which
//! is `-Quiet`'s own (a quiet [`Say`] suppresses the closing `Hosts:` line the run prints through
//! it), and the pinned [`Aws`]. It prints nothing but the count line of a run in which hosts'
//! parts failed, and exits by the report: 0 when no part failed, a lock another program holds
//! included, and 1 otherwise. A failure that ends the run is printed by the caller of
//! [`guard`], as every command's failure is.
//!
//! **The seam:** [`Reach::guard`](crate::Reach::guard) is the run, and its default is [`agentspace_guard::run`]; a test's
//! [`Reach`](crate::Reach) overrides it with a stand-in and drives [`dispatch`](crate::dispatch) with a
//! `guard` request. [`guard_with`] is the same dispatch over any [`GuardRun`], for a test that
//! has no [`Reach`](crate::Reach).

use std::path::Path;
use std::time::UNIX_EPOCH;

use agentspace_box::Clock;
use agentspace_guard::{Input, Report};
use agentspace_operator_api::{Aws, CallerFailure, Context, Failure, Say};

use crate::Parts;

/// A guard run: [`agentspace_guard::run`]'s signature, which a test's stand-in has too.
pub type GuardRun<'a> = dyn Fn(&Input, &dyn Say, &dyn Aws) -> Result<Report, Failure> + 'a;

/// The UTC clock's reading, in Unix seconds, to the whole second (operator.md, guard, step 3). A
/// clock set before 1970 reads 0.
pub fn utc_seconds(clock: &dyn Clock) -> i64 {
    clock
        .utc_now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// `guard`, over `run`: one run at `now`, then the operator's closing (operator.md, guard, step 8).
///
/// - `Ok(report)` with no failed host, `guard.lock` held by another program included: `Ok(())`,
///   and nothing more is printed.
/// - `Ok(report)` with failed hosts: fails with `agentspace: <n> of <m> hosts failed.`, `-All`'s
///   line, `<m>` being the registry's records, which the caller prints and exits 1 for.
/// - `Err(failure)`: that failure, unchanged.
pub fn guard_with(
    run: &GuardRun<'_>,
    ctx: &Context,
    state_dir: &Path,
    now: i64,
    say: &dyn Say,
    aws: &dyn Aws,
) -> Result<(), Failure> {
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
    Ok(())
}

/// `guard` as the dispatch runs it: [`guard_with`] over [`Reach::guard`](crate::Reach::guard) (the real run unless a
/// stand-in overrides it), the state folder [`Reach::lease_state_dir`](crate::Reach::lease_state_dir) gives, and the clock's
/// reading read once.
pub fn guard(parts: &Parts) -> Result<(), Failure> {
    let state_dir = parts.reach.lease_state_dir(parts.ctx);
    let now = utc_seconds(parts.clock);
    guard_with(
        &|input, say, aws| parts.reach.guard(input, say, aws),
        parts.ctx,
        &state_dir,
        now,
        parts.say,
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
        assert_eq!(result, Ok(()));
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
            Ok(())
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
    fn the_time_is_the_clock_to_the_whole_second() {
        struct Fixed;
        impl Clock for Fixed {
            fn now(&self) -> std::time::Duration {
                std::time::Duration::ZERO
            }
            fn utc_now(&self) -> std::time::SystemTime {
                UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 999_000_000)
            }
            fn sleep(&self, _: std::time::Duration) {}
        }
        assert_eq!(utc_seconds(&Fixed), 1_700_000_000);
    }
}

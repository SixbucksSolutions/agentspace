//! A keeper's loop: `-Watch` (docs/keeper.md, Watching; Fast failures and backoff; Release grace;
//! Locks, board-holder-keeper.lock; Log lines, A keeper's own lines).
//!
//! Every duration here is measured on `Clock::monotonic` and waited with `Clock::sleep`, never on
//! local time and never with the standard library's sleep (keeper.md, Times).
//!
//! **No error ends the loop** (keeper.md, Watching): the look, the pass and the wait each run on
//! their own, and a panic that escapes one is caught, logged as a keeper's `failed` line with the
//! matching reason of keeper.md's Reasons, and the loop goes on as for any other error there
//! (keeper.md, Fast failures and backoff; architect, 2026-11-19, for E0.06-T02-S05). Catching it
//! does not stop the process's panic hook from running, and the default hook writes the panic's
//! message to stderr. A library MUST NOT set the process's hook, so keeping that message off
//! stderr is the binary's (keeper.md, Streams and environment).

use crate::Deps;
use crate::args::WatchOptions;
use crate::log::{Logger, pass_line};
use crate::pass::{Action, PassContext, run_pass};
use agentspace_keeper_api::{HolderRecord, Query, Taken};
use std::any::Any;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::Duration;

/// What a keeper runs with, beyond the pass's own context.
pub(crate) struct Keeping<'a> {
    pub state_dir: &'a Path,
    pub profile: &'a Path,
    pub ssh_exe: &'a Path,
    pub own_exe: &'a Path,
    pub options: WatchOptions,
}

/// The holder a keeper tracks between passes: one it started, or one it adopted.
struct Tracked {
    /// Its PID, creation time, lease ID and alias, as the keeper holds them.
    record: HolderRecord,
    /// The monotonic time at which the keeper began to track it.
    since: Duration,
    /// Seen alive a look at least `-FastFailureSeconds` after `since`, or adopted.
    proven: bool,
}

/// The keeper's state, which lives only in its process (keeper.md, Watching).
struct State {
    tracked: Option<Tracked>,
    /// Fast failures in a row.
    failures: u32,
}

/// Which line a wait is logged with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Word {
    Grace,
    Backoff,
}

/// A wait, as the keeper's table gives it.
#[derive(Clone, Copy)]
struct Wait {
    word: Word,
    seconds: u64,
}

/// The `<error>` of a reason, for a panic whose payload is neither a `&str` nor a `String`.
const NO_MESSAGE: &str = "The panic carried no message.";

/// A panic's message: its payload when that is a `&str` or a `String`, else a fixed text.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        NO_MESSAGE.to_string()
    }
}

/// Runs `f`, and gives its value, or the message of a panic that escaped it.
fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| panic_message(&*payload))
}

/// The keeper's own line, for the current second.
fn log_own(deps: &Deps, state_dir: &Path, stdout: &mut dyn Write, text: &str) {
    let mut log = Logger::new(state_dir, deps.clock, stdout);
    // A keeper's own line that is dropped ends nothing (keeper.md, Log lines).
    let _ = log.log(text);
}

/// `<time> <text>`, with the time the current second, as a keeper's own lines have it.
fn own_line(deps: &Deps, text: &str) -> String {
    let secs = deps.clock.utc_now_ms().div_euclid(1000);
    format!("{} {text}", agentspace_state::format_time(secs))
}

/// A keeper's `failed lease=- alias=- pid=- reason=<reason>` line, with the pass line's grammar
/// and the current second as its time. It is dropped when it cannot be logged, a panic while it
/// is logged included, and a dropped `failed` line leaves the loop running (keeper.md, Log lines).
fn log_failed(deps: &Deps, state_dir: &Path, stdout: &mut dyn Write, reason: &str) {
    let _ = guarded(|| {
        let secs = deps.clock.utc_now_ms().div_euclid(1000);
        let line = pass_line(
            &agentspace_state::format_time(secs),
            "failed",
            "",
            "",
            "",
            Some(reason),
        );
        log_own(deps, state_dir, stdout, &line);
    });
}

/// The backoff after the `failures`-th fast failure in a row:
/// `min(BackoffMaxSeconds, BackoffBaseSeconds * 2^(n-1))` (keeper.md, Fast failures and backoff).
fn backoff_seconds(options: &WatchOptions, failures: u32) -> u64 {
    let max = u64::from(options.backoff_max_seconds);
    let base = u64::from(options.backoff_base_seconds);
    // 2^(n-1) passes any limit of 86400 long before 2^40.
    let exponent = failures.saturating_sub(1).min(40);
    base.saturating_mul(1u64 << exponent).min(max)
}

/// The wait after a stop, a tried stop, or a holder that ended and was never a fast failure: the
/// release grace, and at least `-IntervalSeconds`.
fn grace_wait(options: &WatchOptions) -> Wait {
    Wait {
        word: Word::Grace,
        seconds: u64::from(options.release_grace_seconds.max(options.interval_seconds)),
    }
}

/// The wait after a failed start: the backoff for the count, and at least `-IntervalSeconds`.
fn backoff_wait(options: &WatchOptions, failures: u32) -> Wait {
    Wait {
        word: Word::Backoff,
        seconds: backoff_seconds(options, failures).max(u64::from(options.interval_seconds)),
    }
}

/// The wait after a holder that ended: the longer of the grace and, for an unproven holder, the
/// backoff for the new count, never their sum, and at least `-IntervalSeconds`. The line says
/// `backoff` when the backoff is at least the grace, else `grace` (keeper.md, Release grace).
fn ended_wait(options: &WatchOptions, unproven: bool, failures: u32) -> Wait {
    let grace = u64::from(options.release_grace_seconds);
    let (word, longer) = if unproven {
        let backoff = backoff_seconds(options, failures);
        if backoff >= grace {
            (Word::Backoff, backoff)
        } else {
            (Word::Grace, grace)
        }
    } else {
        (Word::Grace, grace)
    };
    Wait {
        word,
        seconds: longer.max(u64::from(options.interval_seconds)),
    }
}

/// Logs a wait's line and waits it out. A line that cannot be logged changes nothing: the wait is
/// still its full time. A panic in the line or in the sleep is the wait's error (keeper.md,
/// Reasons): it is logged, and the keeper goes on.
fn wait(deps: &Deps, state_dir: &Path, stdout: &mut dyn Write, wait: Option<Wait>, interval: u32) {
    let seconds = wait.map_or(u64::from(interval), |w| w.seconds);
    if let Some(w) = wait {
        let word = match w.word {
            Word::Grace => "grace",
            Word::Backoff => "backoff",
        };
        let logged = guarded(|| {
            let line = own_line(deps, &format!("{word} seconds={}", w.seconds));
            log_own(deps, state_dir, stdout, &line);
        });
        if let Err(message) = logged {
            log_failed(
                deps,
                state_dir,
                stdout,
                &format!("The wait stopped on an error: {message}"),
            );
        }
    }
    // A wait whose `Clock::sleep` panics ends there, its error is logged, and the loop goes on to
    // its next pass (keeper.md, Fast failures and backoff; architect, 2026-11-19, for
    // E0.06-T02-S05). The pinned clock is the only way a keeper waits.
    if let Err(message) = guarded(|| deps.clock.sleep(Duration::from_secs(seconds))) {
        log_failed(
            deps,
            state_dir,
            stdout,
            &format!("The wait stopped on an error: {message}"),
        );
    }
}

/// The look: is the tracked holder alive, by PID, creation time and command line, against the
/// keeper's `-SshExe` and the alias it was started or adopted with? It reads the process only,
/// never the record. A process that cannot be opened to query it, or that cannot be checked, is
/// not found.
fn alive(deps: &Deps, ssh_exe: &Path, holder: &HolderRecord) -> bool {
    match deps.processes.query(holder.pid) {
        Query::Running(info) => {
            info.process_started_at == holder.process_started_at
                && info
                    .command_line
                    .as_deref()
                    .is_some_and(|cl| deps.matcher.is_holder(cl, ssh_exe, &holder.alias))
        }
        Query::Gone | Query::CannotOpen(_) => false,
    }
}

/// Runs a keeper: the keeper lock once, then passes until `-MaxPasses` have run, and the exit
/// code (keeper.md, Watching; Exit codes).
pub(crate) fn watch(keeping: &Keeping, deps: &Deps, stdout: &mut dyn Write) -> i32 {
    let state_dir = keeping.state_dir;
    let options = keeping.options;

    // The keeper lock, taken once, before the first pass, and held for the keeper's whole life:
    // the guard lives until this function returns (keeper.md, Locks).
    let _keeper_lock = match deps.locks.keeper(state_dir) {
        Taken::Held(guard) => guard,
        Taken::Busy(pid) => return another_keeper(deps, state_dir, stdout, pid),
        // `Skipped` is the pass lock's in the pin; from the keeper lock it is read as `Busy(None)`
        // (architect, 2026-11-19, for E0.06-T02-S05, ruling 1).
        Taken::Skipped => return another_keeper(deps, state_dir, stdout, None),
        Taken::Failed(error) => {
            log_failed(
                deps,
                state_dir,
                stdout,
                &format!("The keeper lock could not be taken: {error}"),
            );
            return 1;
        }
    };

    let mut state = State {
        tracked: None,
        failures: 0,
    };
    let mut passes: u32 = 0;
    loop {
        // 1. The look, before the pass, and the wait it may earn at once.
        look(keeping, deps, stdout, &mut state);

        // 2. The pass, with the pass time taken now. It counts whether or not it ran to the end.
        // The monotonic time is read in the same guard: a holder this pass started or adopted is
        // tracked from the end of the pass.
        let tracked = state.tracked.as_ref().map(|t| &t.record);
        let ran = guarded(|| {
            let ctx = PassContext {
                state_dir,
                profile: keeping.profile,
                ssh_exe: keeping.ssh_exe,
                own_exe: keeping.own_exe,
                now: None,
                tracked,
            };
            let report = run_pass(&ctx, deps, stdout);
            (report.action, deps.clock.monotonic())
        });
        passes = passes.saturating_add(1);

        // 3. What the pass did, for tracking, and the wait it earns. A pass that stopped on a
        // panic is a pass that failed before deciding: nothing changes.
        let after = match ran {
            Ok((action, ended)) => track(&mut state, action, ended, &options),
            Err(message) => {
                log_failed(
                    deps,
                    state_dir,
                    stdout,
                    &format!("The pass stopped on an error: {message}"),
                );
                None
            }
        };

        // 4. `-MaxPasses` passes have run.
        if options.max_passes.is_some_and(|max| passes >= max) {
            return 0;
        }

        // 5. The wait. Every iteration waits at least `-IntervalSeconds`, on every path.
        wait(deps, state_dir, stdout, after, options.interval_seconds);
    }
}

/// A second keeper's one line, and its exit 0, with no pass and no wait.
fn another_keeper(deps: &Deps, state_dir: &Path, stdout: &mut dyn Write, pid: Option<u32>) -> i32 {
    let pid = pid.map_or_else(|| "-".to_string(), |p| p.to_string());
    let _ = guarded(|| {
        let line = own_line(deps, &format!("another keeper is running pid={pid}"));
        log_own(deps, state_dir, stdout, &line);
    });
    0
}

/// What a look found.
enum Seen {
    /// Alive; `proven` when tracked for at least `-FastFailureSeconds`.
    Alive { proven: bool },
    /// Gone, no longer the same process, or the check itself failed.
    Ended,
}

/// Step 1 of an iteration: the look at the tracked holder, and the wait it may earn at once.
fn look(keeping: &Keeping, deps: &Deps, stdout: &mut dyn Write, state: &mut State) {
    let options = &keeping.options;
    let Some(tracked) = state.tracked.as_ref() else {
        return;
    };
    let proof = Duration::from_secs(u64::from(options.fast_failure_seconds));
    let seen = guarded(|| {
        if alive(deps, keeping.ssh_exe, &tracked.record) {
            let age = deps.clock.monotonic().saturating_sub(tracked.since);
            Seen::Alive {
                proven: age >= proof,
            }
        } else {
            Seen::Ended
        }
    })
    .unwrap_or_else(|message| {
        // A look that stopped on an error is a check that failed: the holder is no longer
        // tracked, as when it is gone (keeper.md, Watching, step 1).
        log_failed(
            deps,
            keeping.state_dir,
            stdout,
            &format!("The look at the holder stopped on an error: {message}"),
        );
        Seen::Ended
    });

    match seen {
        // Alive, and tracked for at least `-FastFailureSeconds`: proven, and the count goes to 0.
        // An adopted holder is under the same rule (architect, 2026-11-19, ruling 2).
        Seen::Alive { proven: true } => {
            if let Some(t) = state.tracked.as_mut() {
                t.proven = true;
            }
            state.failures = 0;
        }
        // Alive and younger: nothing changes.
        Seen::Alive { proven: false } => {}
        Seen::Ended => {
            // No longer tracked. Never proven is a fast failure; proven leaves the count as it is.
            let proven = state.tracked.take().is_some_and(|t| t.proven);
            if !proven {
                state.failures = state.failures.saturating_add(1);
            }
            // The keeper waits once, at once, before the pass.
            let w = ended_wait(options, !proven, state.failures);
            wait(
                deps,
                keeping.state_dir,
                stdout,
                Some(w),
                options.interval_seconds,
            );
        }
    }
}

/// Step 3 of an iteration: what the pass did, for tracking (keeper.md, Watching), and the wait
/// after it. `ended` is the monotonic time at the end of the pass. `None` is the plain wait of
/// `-IntervalSeconds`, with no line.
fn track(
    state: &mut State,
    action: Action,
    ended: Duration,
    options: &WatchOptions,
) -> Option<Wait> {
    match action {
        // Tracked, unproven, from the end of this pass.
        Action::Started(holder) => {
            state.tracked = Some(Tracked {
                record: holder,
                since: ended,
                proven: false,
            });
            None
        }
        Action::Kept(holder) => {
            let already = state.tracked.as_ref().is_some_and(|t| {
                t.record.pid == holder.pid
                    && t.record.process_started_at == holder.process_started_at
            });
            if !already {
                // Adopted: tracked from the end of this pass, proven, in place of any holder
                // tracked before. The count is unchanged, and no grace follows.
                state.tracked = Some(Tracked {
                    record: holder,
                    since: ended,
                    proven: true,
                });
            }
            None
        }
        // The pass found no trusted holder, and started nothing: the grace, or the longer
        // backoff if the holder was never proven (card #0068).
        Action::Lost(_) => {
            let proven = state.tracked.take().is_some_and(|t| t.proven);
            if !proven {
                state.failures = state.failures.saturating_add(1);
            }
            Some(ended_wait(options, !proven, state.failures))
        }
        // A start that did not give a live holder: a fast failure, and the backoff alone. A
        // holder stopped because its record could not be written gets no grace either.
        Action::StartFailed(_) => {
            state.tracked = None;
            state.failures = state.failures.saturating_add(1);
            Some(backoff_wait(options, state.failures))
        }
        // A holder stopped, or a stop tried: the count goes to 0, and the release grace follows.
        Action::Stopped(_) | Action::StopFailed(_) => {
            state.tracked = None;
            state.failures = 0;
            Some(grace_wait(options))
        }
        // Nothing changes.
        Action::Idle | Action::Skipped | Action::Failed => None,
    }
}

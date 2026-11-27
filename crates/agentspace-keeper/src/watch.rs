//! A keeper's loop: `-Watch` (docs/keeper.md, Watching; The keeper's wait; Fast failures and
//! backoff; Release grace; Locks, board-holder-keeper.lock; Log lines, A keeper's own lines).
//!
//! Every duration here is measured on `Clock::monotonic` and waited with `Clock::sleep`, never on
//! local time and never with the standard library's sleep (keeper.md, Times).
//!
//! **The keeper keeps a state for each host, and one host's state never changes another's**: the
//! holder it tracks there, whether that holder is proven, the host's count of fast failures, and
//! the host's hold-off (keeper.md, Watching). With only the box there is one host's state, and the
//! loop is v0.06's step for step.
//!
//! **A keeper starts the guard** (keeper.md, Starting the guard; D56, D63): at its first iteration
//! and then once `-GuardIntervalSeconds` have passed since its last attempt, at the first due point
//! of the loop: just before a pass, or once in a wait, which then runs on to the end it had. The
//! time of the last attempt lives in the keeper's process alone. The attempt is never inside a
//! pass, and changes no host's state.
//!
//! **No error ends the loop** (keeper.md, Watching): the look, the guard's start, the pass and the
//! wait each run on their own, and a panic that escapes one is caught, logged as a keeper's
//! `failed` line with the matching reason of keeper.md's Reasons, and the loop goes on as for any
//! other error there
//! (keeper.md, Fast failures and backoff; architect, 2026-11-19, for E0.06-T02-S05). Catching it
//! does not stop the process's panic hook from running, and the default hook writes the panic's
//! message to stderr. A library MUST NOT set the process's hook, so keeping that message off
//! stderr is the binary's (keeper.md, Streams and environment).

use crate::Deps;
use crate::args::WatchOptions;
use crate::guard::{self, GuardSetup, Outcome};
use crate::hosts::box_key;
use crate::log::{Logger, pass_line};
use crate::pass::{Action, PassContext, Tracking, run_pass_hosts};
use agentspace_hosts::HostKey;
use agentspace_keeper_api::{HolderRecord, Query, Taken};
use std::any::Any;
use std::collections::HashMap;
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
    /// The guard's program and environment (keeper.md, Starting the guard).
    pub guard: GuardSetup,
}

/// The holder a keeper tracks on a host between passes: one it started, or one it adopted.
struct Tracked {
    /// Its PID, creation time, lease ID and alias, as the keeper holds them.
    record: HolderRecord,
    /// The monotonic time at which the keeper began to track it.
    since: Duration,
    /// Seen alive a look at least `-FastFailureSeconds` after `since`, or adopted.
    proven: bool,
}

/// One host's state, which lives only in the keeper's process (keeper.md, Watching).
#[derive(Default)]
struct HostState {
    tracked: Option<Tracked>,
    /// Fast failures in a row, on this host alone.
    failures: u32,
    /// The monotonic time at which the host's hold-off ends; while it runs, no pass of this
    /// keeper starts a holder on the host.
    hold_until: Option<Duration>,
}

/// The keeper's state. A host is keyed by its project and its platform (D91), the box by
/// [`box_key`].
struct State {
    hosts: HashMap<HostKey, HostState>,
    /// The hosts of the keeper's last pass, or the box alone before its first pass (keeper.md, The
    /// keeper's wait).
    keeper_hosts: Vec<HostKey>,
    /// The monotonic time at the end of the keeper's last attempt of the guard's start, `None`
    /// before its first, so a new keeper's first iteration starts the guard (keeper.md, When the
    /// keeper starts it).
    guard_last: Option<Duration>,
}

impl State {
    fn host(&mut self, key: &HostKey) -> &mut HostState {
        self.hosts.entry(key.clone()).or_default()
    }

    /// Whether `key`'s hold-off is still running at `now`.
    fn held_off(&self, key: &HostKey, now: Duration) -> bool {
        self.hosts
            .get(key)
            .and_then(|h| h.hold_until)
            .is_some_and(|until| until > now)
    }

    /// When the first of the keeper's hosts that is in a hold-off is due, or `None` when one of
    /// them has no hold-off running at `now`, and so is due at once after a look, and an interval
    /// after a pass.
    fn all_held_until(&self, now: Duration) -> Option<Duration> {
        let mut first: Option<Duration> = None;
        for key in &self.keeper_hosts {
            match self.hosts.get(key).and_then(|h| h.hold_until) {
                Some(until) if until > now => {
                    first = Some(first.map_or(until, |f| f.min(until)));
                }
                _ => return None,
            }
        }
        first
    }

    /// Drops the state of a host that has nothing left in it: no tracked holder, no count and no
    /// hold-off running. Such a state is the same as none.
    fn prune(&mut self, now: Duration) {
        self.hosts.retain(|_, h| {
            h.tracked.is_some() || h.failures > 0 || h.hold_until.is_some_and(|u| u > now)
        });
    }
}

/// Which line a hold-off is logged with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Word {
    Grace,
    Backoff,
}

/// A hold-off, as the keeper's tables give it.
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

/// The hold-off after a stop, or a tried stop: the release grace, and at least
/// `-IntervalSeconds`.
fn grace_wait(options: &WatchOptions) -> Wait {
    Wait {
        word: Word::Grace,
        seconds: u64::from(options.release_grace_seconds.max(options.interval_seconds)),
    }
}

/// The hold-off after a failed start: the backoff for the host's count, and at least
/// `-IntervalSeconds`.
fn backoff_wait(options: &WatchOptions, failures: u32) -> Wait {
    Wait {
        word: Word::Backoff,
        seconds: backoff_seconds(options, failures).max(u64::from(options.interval_seconds)),
    }
}

/// The hold-off after a holder that ended: the longer of the grace and, for an unproven holder,
/// the backoff for the host's new count, never their sum, and at least `-IntervalSeconds`. The
/// line says `backoff` when the backoff is at least the grace, else `grace` (keeper.md, Release
/// grace).
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

/// Sets a host's hold-off and logs its line: `grace seconds=<n>` or `backoff seconds=<n>`, with
/// ` host=<host name>` for a host that is not the box (keeper.md, A keeper's own lines). The hold-off
/// replaces any running there, and is counted from when its line has been written, so the line's
/// `<n>` is the whole seconds it has left then. A line that cannot be logged changes nothing; a
/// panic while it is logged is the wait's error (keeper.md, Reasons), and the hold-off is set all
/// the same.
fn hold_off(
    deps: &Deps,
    state_dir: &Path,
    stdout: &mut dyn Write,
    state: &mut State,
    key: &HostKey,
    w: Wait,
) {
    let word = match w.word {
        Word::Grace => "grace",
        Word::Backoff => "backoff",
    };
    let host = if key.is_box() {
        String::new()
    } else {
        format!(" host={}", key.host_name())
    };
    let logged = guarded(|| {
        let line = own_line(deps, &format!("{word} seconds={}{host}", w.seconds));
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
    // Architect's to confirm: the hold-off counts from just after its line, not from the end of
    // the pass or the look itself, which keeps v0.06's wait of exactly `<n>` seconds after the
    // line even when the line's write was retried; the two differ by the line's write alone.
    let from = guarded(|| deps.clock.monotonic()).unwrap_or_default();
    state.host(key).hold_until = Some(from.saturating_add(Duration::from_secs(w.seconds)));
}

/// Waits `d` on the keeper's clock. A wait whose `Clock::sleep` panics ends there, its error is
/// logged, and the loop goes on to its next pass (keeper.md, Fast failures and backoff;
/// architect, 2026-11-19, for E0.06-T02-S05). The pinned clock is the only way a keeper waits.
fn sleep(deps: &Deps, state_dir: &Path, stdout: &mut dyn Write, d: Duration) {
    if let Err(message) = guarded(|| deps.clock.sleep(d)) {
        log_failed(
            deps,
            state_dir,
            stdout,
            &format!("The wait stopped on an error: {message}"),
        );
    }
}

/// The monotonic time now, or `None` when reading it panicked, which is logged as the wait's
/// error.
fn now(deps: &Deps, state_dir: &Path, stdout: &mut dyn Write) -> Option<Duration> {
    match guarded(|| deps.clock.monotonic()) {
        Ok(t) => Some(t),
        Err(message) => {
            log_failed(
                deps,
                state_dir,
                stdout,
                &format!("The wait stopped on an error: {message}"),
            );
            None
        }
    }
}

/// The look's check of one holder: is it alive, by PID, creation time and command line, against
/// the keeper's `-SshExe` and the alias it was started or adopted with? It reads the process only,
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
        hosts: HashMap::new(),
        keeper_hosts: vec![box_key()],
        guard_last: None,
    };
    let mut passes: u32 = 0;
    loop {
        // 1. The look, before the pass, at each host the keeper tracks a holder on, and the wait
        // it may earn at once.
        if look(keeping, deps, stdout, &mut state) {
            wait_after_look(keeping, deps, stdout, &mut state);
        }

        // 2. The guard's start, when it is due: just before the pass, after the look and any wait
        // after it, and before the pass time is taken.
        guard_before_pass(keeping, deps, stdout, &mut state);

        // 3. The pass, with the pass time taken now. It counts whether or not it ran to the end.
        // The monotonic time is read in the same guard: a holder this pass started or adopted is
        // tracked from the end of the pass.
        let tracked: HashMap<HostKey, HolderRecord> = state
            .hosts
            .iter()
            .filter_map(|(k, h)| h.tracked.as_ref().map(|t| (k.clone(), t.record.clone())))
            .collect();
        let ran = {
            let state = &state;
            let held_off = |key: &HostKey| state.held_off(key, deps.clock.monotonic());
            guarded(|| {
                let ctx = PassContext {
                    state_dir,
                    profile: keeping.profile,
                    ssh_exe: keeping.ssh_exe,
                    own_exe: keeping.own_exe,
                    now: None,
                    tracked: None,
                };
                let tracking = Tracking {
                    tracked: &tracked,
                    held_off: &held_off,
                };
                let passed = run_pass_hosts(&ctx, &tracking, deps, stdout);
                (passed, deps.clock.monotonic())
            })
        };
        passes = passes.saturating_add(1);

        // 4. What each host's turn did, for tracking there, and the hold-off it calls for. A pass
        // that stopped on a panic is a pass that failed before any turn: nothing changes.
        // Architect's to confirm: a panic is caught for the whole pass, as in v0.06, so a panic
        // in one host's turn ends the turns after it in that pass too; the code panics on no
        // input, so only a fake can raise one.
        let mut holds: Vec<(HostKey, Wait)> = Vec::new();
        match ran {
            Ok((passed, ended)) => {
                // Architect's to confirm: a pass that took no turn (`skipped`, or failed before
                // its turns) leaves the keeper's hosts those of the pass before it.
                if let Some(hosts) = passed.hosts {
                    state.keeper_hosts = hosts;
                }
                for turn in passed.turns {
                    let host = state.host(&turn.host);
                    if let Some(w) = track(host, turn.action, ended, &options) {
                        holds.push((turn.host, w));
                    }
                }
            }
            Err(message) => {
                log_failed(
                    deps,
                    state_dir,
                    stdout,
                    &format!("The pass stopped on an error: {message}"),
                );
            }
        }

        // 5. `-MaxPasses` passes have run.
        if options.max_passes.is_some_and(|max| passes >= max) {
            return 0;
        }

        // 6. The hold-offs the pass set, in the order of the turns.
        for (key, w) in holds {
            hold_off(deps, state_dir, stdout, &mut state, &key, w);
        }

        // 7. The wait. Every iteration waits at least `-IntervalSeconds`, on every path.
        wait_after_pass(keeping, deps, stdout, &mut state, options.interval_seconds);
    }
}

/// The wait after a look that set a hold-off (keeper.md, The keeper's wait): until the first of
/// the keeper's hosts is due, which is at once while any of them has no hold-off running. With
/// only the box, it waits out the box's hold-off, as v0.06's wait after the look did.
fn wait_after_look(keeping: &Keeping, deps: &Deps, stdout: &mut dyn Write, state: &mut State) {
    let Some(start) = now(deps, keeping.state_dir, stdout) else {
        return;
    };
    if let Some(until) = state.all_held_until(start) {
        wait(
            keeping,
            deps,
            stdout,
            state,
            start,
            until.saturating_sub(start),
        );
    }
}

/// The wait after a pass (keeper.md, The keeper's wait): until the first of the keeper's hosts is
/// due, a host with no hold-off running being due `-IntervalSeconds` after the wait began, and
/// never shorter than `-IntervalSeconds`. With only the box, it is v0.06's: the box's hold-off
/// when the pass set one, else `-IntervalSeconds`.
fn wait_after_pass(
    keeping: &Keeping,
    deps: &Deps,
    stdout: &mut dyn Write,
    state: &mut State,
    interval: u32,
) {
    let state_dir = keeping.state_dir;
    let interval = Duration::from_secs(u64::from(interval));
    let Some(start) = now(deps, state_dir, stdout) else {
        // A clock that cannot be read still gives the floor (D14).
        sleep(deps, state_dir, stdout, interval);
        return;
    };
    state.prune(start);
    let d = state
        .all_held_until(start)
        .map_or(interval, |until| until.saturating_sub(start).max(interval));
    wait(keeping, deps, stdout, state, start, d);
}

/// A wait of `d` from `start`, in which the guard's start is made once when it falls due before
/// the wait's end: at that moment, or at the wait's beginning when it is already due (keeper.md,
/// When the keeper starts it, point 2). The wait then runs on to the end it had, neither sooner
/// nor later for it; a start that finds `guard.lock` held is not tried again in the same wait.
fn wait(
    keeping: &Keeping,
    deps: &Deps,
    stdout: &mut dyn Write,
    state: &mut State,
    start: Duration,
    d: Duration,
) {
    let state_dir = keeping.state_dir;
    let end = start.saturating_add(d);
    let due_at = guard_due_at(keeping, state).max(start);
    if due_at >= end {
        sleep(deps, state_dir, stdout, d);
        return;
    }
    if due_at > start {
        sleep(deps, state_dir, stdout, due_at - start);
    }
    guard_attempt(keeping, deps, stdout, state, due_at);
    // The rest of the wait, to the end it had. A clock that cannot be read here leaves the part of
    // the wait that was planned after the start, so the wait keeps its floor (D14).
    let rest = match now(deps, state_dir, stdout) {
        Some(t) => end.saturating_sub(t),
        None => end.saturating_sub(due_at),
    };
    if !rest.is_zero() {
        sleep(deps, state_dir, stdout, rest);
    }
}

/// When the guard's start is next due, on the keeper's monotonic clock: at once (zero) before the
/// keeper's first attempt, then `-GuardIntervalSeconds` after the end of its last.
fn guard_due_at(keeping: &Keeping, state: &State) -> Duration {
    let interval = Duration::from_secs(u64::from(keeping.options.guard_interval_seconds));
    state
        .guard_last
        .map_or(Duration::ZERO, |last| last.saturating_add(interval))
}

/// The guard's start just before a pass, when it is due (keeper.md, When the keeper starts it,
/// point 1).
fn guard_before_pass(keeping: &Keeping, deps: &Deps, stdout: &mut dyn Write, state: &mut State) {
    let due_at = guard_due_at(keeping, state);
    match guarded(|| deps.clock.monotonic()) {
        Ok(t) if t >= due_at => guard_attempt(keeping, deps, stdout, state, t),
        Ok(_) => {}
        // A clock that cannot be read is an error that escaped the start: logged as the start's,
        // and it counts as an attempt (keeper.md, The keeper's wait). With no reading to mark its
        // end, the time it was due stands in for it, so the next one is due `-GuardIntervalSeconds`
        // after that and a counted attempt always moves `guard_last`.
        Err(message) => {
            log_failed(
                deps,
                keeping.state_dir,
                stdout,
                &guard::failed_reason(&guard::escaped(&message)),
            );
            state.guard_last = Some(due_at);
        }
    }
}

/// One attempt of the guard's start, or its try (keeper.md, How the keeper starts it): try
/// `guard.lock`, find the program, start it detached, and log one line. A lock found held logs
/// nothing and is not an attempt, so the start stays due. Otherwise the attempt counts, whatever
/// its line, and the next is due `-GuardIntervalSeconds` after its end, once its line has been
/// written or dropped. `began` is the monotonic time at its beginning, which stands in for its end
/// when the clock cannot be read after it. It changes no host's state.
fn guard_attempt(
    keeping: &Keeping,
    deps: &Deps,
    stdout: &mut dyn Write,
    state: &mut State,
    began: Duration,
) {
    let state_dir = keeping.state_dir;
    let outcome = guarded(|| guard::attempt(&keeping.guard, deps.guard))
        .unwrap_or_else(|message| Outcome::Failed(guard::escaped(&message)));
    match &outcome {
        Outcome::Held => return,
        Outcome::Started(_) => {
            let text = outcome.text().unwrap_or_default();
            // A line that cannot be logged is dropped, and the attempt stands; a panic while it is
            // logged is an error that escaped the start.
            let logged = guarded(|| {
                let line = own_line(deps, &text);
                log_own(deps, state_dir, stdout, &line);
            });
            if let Err(message) = logged {
                log_failed(
                    deps,
                    state_dir,
                    stdout,
                    &guard::failed_reason(&guard::escaped(&message)),
                );
            }
        }
        Outcome::Failed(why) => {
            log_failed(deps, state_dir, stdout, &guard::failed_reason(why));
        }
    }
    state.guard_last = Some(guarded(|| deps.clock.monotonic()).unwrap_or(began));
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

/// The hosts a look walks, each that the keeper tracks a holder on: the box first, then the others
/// in the order their turns took in the keeper's last pass, which is `hosts.json`'s file order
/// (keeper.md, Watching, step 1; architect, 2026-11-26, for v0.12-T01, pin P46). The look reads no
/// file for that order. A holder is tracked only through a turn, so every tracked host was in the
/// last pass; were one not, it would come last, in the ordinal order of its host name.
fn look_order(state: &State) -> Vec<HostKey> {
    let tracked = |k: &HostKey| state.hosts.get(k).is_some_and(|h| h.tracked.is_some());
    let mut keys: Vec<HostKey> = state
        .keeper_hosts
        .iter()
        .filter(|k| tracked(k))
        .cloned()
        .collect();
    let mut rest: Vec<HostKey> = state
        .hosts
        .iter()
        .filter(|(k, h)| h.tracked.is_some() && !keys.contains(k))
        .map(|(k, _)| k.clone())
        .collect();
    rest.sort_by_key(HostKey::host_name);
    keys.extend(rest);
    if let Some(i) = keys.iter().position(HostKey::is_box) {
        let boxed = keys.remove(i);
        keys.insert(0, boxed);
    }
    keys
}

/// Step 1 of an iteration: the look at each host's tracked holder, in [`look_order`]. A holder
/// found ended sets its host's hold-off, counted from the look, and logs its line. Says whether
/// the look set any hold-off.
fn look(keeping: &Keeping, deps: &Deps, stdout: &mut dyn Write, state: &mut State) -> bool {
    let options = &keeping.options;
    let proof = Duration::from_secs(u64::from(options.fast_failure_seconds));
    let keys = look_order(state);
    let mut set_any = false;
    for key in keys {
        let Some(tracked) = state.host(&key).tracked.as_ref() else {
            continue;
        };
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

        let host = state.host(&key);
        match seen {
            // Alive, and tracked for at least `-FastFailureSeconds`: proven, and the host's count
            // goes to 0. An adopted holder is under the same rule (architect, 2026-11-19, ruling
            // 2).
            Seen::Alive { proven: true } => {
                if let Some(t) = host.tracked.as_mut() {
                    t.proven = true;
                }
                host.failures = 0;
            }
            // Alive and younger: nothing changes.
            Seen::Alive { proven: false } => {}
            Seen::Ended => {
                // No longer tracked. Never proven is a fast failure on this host; proven leaves
                // the count as it is.
                let proven = host.tracked.take().is_some_and(|t| t.proven);
                if !proven {
                    host.failures = host.failures.saturating_add(1);
                }
                let w = ended_wait(options, !proven, host.failures);
                hold_off(deps, keeping.state_dir, stdout, state, &key, w);
                set_any = true;
            }
        }
    }
    set_any
}

/// Step 3 of an iteration, for one host: what its turn did, for tracking there (keeper.md,
/// Watching), and the hold-off it calls for. `ended` is the monotonic time at the end of the pass.
/// `None` sets no hold-off; one running there runs on.
fn track(
    host: &mut HostState,
    action: Action,
    ended: Duration,
    options: &WatchOptions,
) -> Option<Wait> {
    match action {
        // Tracked, unproven, from the end of this pass.
        Action::Started(holder) => {
            host.tracked = Some(Tracked {
                record: holder,
                since: ended,
                proven: false,
            });
            None
        }
        Action::Kept(holder) => {
            let already = host.tracked.as_ref().is_some_and(|t| {
                t.record.pid == holder.pid
                    && t.record.process_started_at == holder.process_started_at
            });
            if !already {
                // Adopted: tracked from the end of this pass, proven, in place of any holder
                // tracked there before. The count and the hold-off are unchanged, and no grace
                // follows.
                host.tracked = Some(Tracked {
                    record: holder,
                    since: ended,
                    proven: true,
                });
            }
            None
        }
        // The pass found no trusted holder there, and started nothing: the grace, or the longer
        // backoff if the holder was never proven (card #0068).
        Action::Lost(_) => {
            let proven = host.tracked.take().is_some_and(|t| t.proven);
            if !proven {
                host.failures = host.failures.saturating_add(1);
            }
            Some(ended_wait(options, !proven, host.failures))
        }
        // A start that did not give a live holder: a fast failure, and the backoff alone. A
        // holder stopped because its record could not be written gets no grace either.
        Action::StartFailed(_) => {
            host.tracked = None;
            host.failures = host.failures.saturating_add(1);
            Some(backoff_wait(options, host.failures))
        }
        // A holder stopped, or a stop tried: the count goes to 0, and the release grace follows.
        Action::Stopped(_) | Action::StopFailed(_) => {
            host.tracked = None;
            host.failures = 0;
            Some(grace_wait(options))
        }
        // Nothing changes.
        Action::Idle | Action::Skipped | Action::Failed => None,
    }
}

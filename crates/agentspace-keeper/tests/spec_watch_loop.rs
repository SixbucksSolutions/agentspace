//! The keeper's lock, `-MaxPasses`, the waits and the loop that never ends for an error
//! (keeper.md, Watching, Exit codes, Locks, Log lines and Times; E0.06-T02-S05).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. A keeper takes
//! `board-holder-keeper.lock` once, before its first pass, and holds it for its whole life. Once
//! its loop has started, only `-MaxPasses`, a usage error, the keeper lock or the process being
//! killed ends it. Every iteration waits at least `-IntervalSeconds`, on every path, and every
//! duration is measured on the pinned clock's monotonic time, never on UTC.

mod common;
mod watch;

use common::{ID_A, PassMode};
use watch::*;

/// The lines of a log, each without its time.
fn bodies(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            l.split_once(' ')
                .map(|(_, rest)| rest)
                .unwrap_or(l)
                .to_string()
        })
        .collect()
}

fn idle_world() -> std::rc::Rc<World> {
    let w = World::new();
    w.set_leases(vec![]);
    w
}

// ---------------------------------------------------------------------------------------------
// The keeper lock
// ---------------------------------------------------------------------------------------------

#[test]
fn the_keeper_lock_is_taken_once_before_the_first_pass_and_held_to_the_end() {
    let w = idle_world();
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0), idle(30), idle(60)]);
    let names = w.names();
    assert_eq!(
        w.count("lock:keeper"),
        1,
        "The keeper lock is taken once, not per pass"
    );
    assert_eq!(w.count("lock:pass"), 3);
    assert!(
        names.iter().position(|n| n == "lock:keeper") < names.iter().position(|n| n == "lock:pass"),
        "The keeper lock is taken before the first pass: {names:?}"
    );
    assert_eq!(
        names.last().map(String::as_str),
        Some("release:keeper"),
        "The keeper lock is held until the keeper ends: {names:?}"
    );
    assert_eq!(w.count("release:keeper"), 1);
}

#[test]
fn a_second_keeper_names_the_running_keepers_pid_and_exits_0_with_no_pass() {
    let w = World::new();
    *w.keeper_mode.borrow_mut() = KeeperMode::Busy(Some(4242));
    let ran = w.run(&[]);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        ran.stdout,
        "2026-11-19T12:00:00Z another keeper is running pid=4242\n"
    );
    assert_eq!(w.log(), ran.stdout);
    assert_eq!(ran.stderr, "");
    assert_eq!(w.count("lock:pass"), 0);
    assert_eq!(w.count_prefix("query:"), 0);
    assert_eq!(w.count_prefix("start:"), 0);
    assert!(w.sleeps().is_empty(), "A second keeper exits with no wait");
}

#[test]
fn a_second_keeper_whose_lock_file_gives_no_pid_logs_a_dash() {
    let w = World::new();
    *w.keeper_mode.borrow_mut() = KeeperMode::Busy(None);
    let ran = w.run(&[]);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        ran.stdout,
        "2026-11-19T12:00:00Z another keeper is running pid=-\n"
    );
    assert_eq!(w.count("lock:pass"), 0);
}

/// The time of a keeper's own line is the current second when it is written, truncated.
#[test]
fn a_keepers_own_line_carries_the_current_second_truncated() {
    let w = World::new();
    w.set_utc_ms(common::now_ms() + 999);
    *w.keeper_mode.borrow_mut() = KeeperMode::Busy(Some(7));
    let ran = w.run(&[]);
    assert_eq!(ran.stdout, another_keeper(0, Some(7)));
}

#[test]
fn a_keeper_lock_that_cannot_be_taken_for_another_reason_exits_1_and_never_loops() {
    let w = World::new();
    *w.keeper_mode.borrow_mut() = KeeperMode::Failed("Access is denied.".to_string());
    let ran = w.run(&["-MaxPasses", "5"]);
    assert_eq!(ran.exit, 1);
    assert_eq!(
        ran.stdout,
        failed(0, "The keeper lock could not be taken: Access is denied.")
    );
    assert_eq!(
        w.count("lock:pass"),
        0,
        "A keeper MUST NOT loop without the lock"
    );
    assert!(w.sleeps().is_empty());
}

// ---------------------------------------------------------------------------------------------
// -MaxPasses
// ---------------------------------------------------------------------------------------------

/// The exit comes after the pass and before the wait: the last pass logs no wait.
#[test]
fn max_passes_ends_the_keeper_before_a_wait() {
    let w = World::new();
    w.plan_fail(EXITED_AT_ONCE);
    let ran = w.run(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![failed_start(0, ID_A, EXITED_AT_ONCE)]);
    assert!(w.sleeps().is_empty());
    assert_eq!(w.count("lock:pass"), 1);
}

#[test]
fn a_pass_counts_toward_max_passes_whether_or_not_it_ran_to_the_end() {
    let w = idle_world();
    w.pass_modes.borrow_mut().push_back(PassMode::Skipped);
    w.pass_modes.borrow_mut().push_back(PassMode::Skipped);
    let ran = w.run(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![skipped(0), skipped(30)]);
    assert_eq!(w.sleeps_secs(), vec![30]);
    assert_eq!(w.count("lock:pass"), 2);
}

/// A skipped pass changes nothing: the tracked holder stays tracked, and the next pass keeps it.
#[test]
fn a_skipped_pass_leaves_the_tracked_holder_as_it_was() {
    let w = World::new();
    w.plan_holder(100, None);
    w.pass_modes.borrow_mut().push_back(PassMode::Held);
    w.pass_modes.borrow_mut().push_back(PassMode::Skipped);
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![started(0, ID_A, 100), skipped(30), kept(60, ID_A, 100)],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30]);
}

// ---------------------------------------------------------------------------------------------
// No error ends the loop, and every wait is at least the interval
// ---------------------------------------------------------------------------------------------

#[test]
fn a_pass_lock_error_fails_the_pass_and_the_keeper_goes_on() {
    let w = idle_world();
    w.pass_modes.borrow_mut().push_back(PassMode::Skipped);
    w.pass_modes
        .borrow_mut()
        .push_back(PassMode::Failed("Access is denied.".to_string()));
    let ran = w.run(&["-IntervalSeconds", "40", "-MaxPasses", "3"]);
    assert_eq!(
        ran.exit, 0,
        "A keeper never exits for an error once its loop has started"
    );
    assert_lines(
        &w,
        &ran,
        vec![
            skipped(0),
            failed(40, "The pass stopped on an error: Access is denied."),
            idle(80),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![40, 40]);
}

#[test]
fn an_unreadable_lease_state_fails_a_pass_and_the_keeper_goes_on_with_its_holder() {
    let w = World::new();
    w.adopt(100, ID_A, None);
    w.on("lock:pass", 2, |w| w.set_leases_raw(b"this is not json"));
    w.on("lock:pass", 3, |w| {
        w.set_leases(vec![common::ready(ID_A, FAR)])
    });
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    let lines: Vec<&str> = ran.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "Three passes, one line each: {lines:?}");
    assert_eq!(lines[0], kept(0, ID_A, 100).trim_end());
    assert!(
        lines[1].starts_with(&format!(
            "{} failed lease=- alias=- pid=- reason=Lease state could not be read: ",
            at(30)
        )),
        "The pass's own error line was {:?}",
        lines[1]
    );
    assert_eq!(lines[2], kept(60, ID_A, 100).trim_end());
    assert_eq!(w.sleeps_secs(), vec![30, 30]);
    assert!(w.stops().is_empty(), "The holder is left as it was");
}

#[test]
fn a_matcher_that_fails_its_self_check_fails_each_pass_and_the_keeper_goes_on() {
    let w = World::new();
    *w.self_check.borrow_mut() = Err("case one; case two".to_string());
    let ran = w.run(&["-IntervalSeconds", "40", "-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    let reason = "The pass stopped on an error: The holder command matcher failed its own checks: case one; case two";
    assert_lines(
        &w,
        &ran,
        vec![failed(0, reason), failed(40, reason), failed(80, reason)],
    );
    assert_eq!(w.sleeps_secs(), vec![40, 40]);
    assert_eq!(
        w.count_prefix("start:"),
        0,
        "A pass whose matcher fails trusts nothing"
    );
    assert_eq!(w.count_prefix("stop:"), 0);
}

#[test]
fn a_record_that_cannot_be_removed_fails_the_pass_and_the_keeper_goes_on() {
    let w = World::new();
    w.adopt(100, ID_A, Some(secs(0)));
    *w.remove_error.borrow_mut() = Some("boom".to_string());
    let ran = w.run(&["-IntervalSeconds", "40", "-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    let reason = "The pass stopped on an error: The record could not be removed: boom";
    assert_lines(&w, &ran, vec![failed(0, reason), failed(40, reason)]);
    assert_eq!(w.sleeps_secs(), vec![40]);
    assert_eq!(w.count_prefix("start:"), 0);
}

// ---------------------------------------------------------------------------------------------
// A line that cannot be logged
// ---------------------------------------------------------------------------------------------

/// Makes `board-holder.log` a folder, so no line can be written to it.
fn unwritable_log(w: &World) {
    std::fs::create_dir_all(w.log_path()).unwrap();
}

#[test]
fn a_keeper_whose_lines_cannot_be_logged_still_loops_and_waits_its_interval() {
    let w = idle_world();
    unwritable_log(&w);
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(w.count("lock:pass"), 3);
    assert_eq!(w.sleeps_secs(), vec![30, 30]);
}

/// A keeper tracks by what its pass did, never by the line, and a wait whose line cannot be
/// logged still waits its full time: the holder started is tracked, found gone unproven, and the
/// grace of 95 seconds is waited.
#[test]
fn a_dropped_line_changes_neither_the_tracking_nor_the_length_of_a_wait() {
    let w = World::new();
    unwritable_log(&w);
    w.plan_holder(100, Some(secs(10)));
    w.plan_holder(101, None);
    let ran = w.run(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(w.count("store:write"), 2);
    assert_eq!(w.sleeps_secs(), vec![30, 95]);
    assert_eq!(w.starts().len(), 2);
}

// ---------------------------------------------------------------------------------------------
// Durations are on the monotonic clock
// ---------------------------------------------------------------------------------------------

const PROOF: [&str; 10] = [
    "-IntervalSeconds",
    "5",
    "-ReleaseGraceSeconds",
    "10",
    "-BackoffBaseSeconds",
    "40",
    "-FastFailureSeconds",
    "10",
    "-MaxPasses",
    "0",
];

fn proof_flags(passes: &str) -> Vec<&str> {
    let mut v = PROOF.to_vec();
    *v.last_mut().unwrap() = passes;
    v
}

/// A step of the UTC clock forward by ten hours must not prove a holder that lived 7 seconds on
/// the monotonic clock: it ends before its proof of 10 seconds, so it is a fast failure and the
/// wait is the backoff (40), not the grace (10).
#[test]
fn a_step_of_the_utc_clock_forward_proves_no_holder() {
    let w = World::new();
    w.plan_holder(100, Some(secs(7)));
    w.plan_holder(200, None);
    w.on("sleep:5", 1, |w| w.step_utc(10 * 3600 * 1000));
    let ran = w.run(&proof_flags("3"));
    assert_eq!(ran.exit, 0);
    assert_eq!(
        bodies(&ran.stdout),
        vec![
            format!(
                "started lease={ID_A} alias={} pid=100 host=swimcoach",
                common::alias_of(ID_A)
            ),
            format!(
                "kept lease={ID_A} alias={} pid=100 host=swimcoach",
                common::alias_of(ID_A)
            ),
            "backoff seconds=40".to_string(),
            format!(
                "started lease={ID_A} alias={} pid=200 host=swimcoach",
                common::alias_of(ID_A)
            ),
        ]
    );
    assert_eq!(w.sleeps_secs(), vec![5, 5, 40]);
}

/// A step of the UTC clock back by an hour must not stop a holder that lived 20 seconds on the
/// monotonic clock from being proven: its end is the grace (10), not the backoff (40).
#[test]
fn a_step_of_the_utc_clock_back_does_not_keep_a_holder_from_being_proven() {
    let w = World::new();
    w.plan_holder(100, Some(secs(20)));
    w.plan_holder(200, None);
    w.on("sleep:5", 1, |w| w.step_utc(-3600 * 1000));
    let ran = w.run(&proof_flags("5"));
    assert_eq!(ran.exit, 0);
    let alias = common::alias_of(ID_A);
    assert_eq!(
        bodies(&ran.stdout),
        vec![
            format!("started lease={ID_A} alias={alias} pid=100 host=swimcoach"),
            format!("kept lease={ID_A} alias={alias} pid=100 host=swimcoach"),
            format!("kept lease={ID_A} alias={alias} pid=100 host=swimcoach"),
            format!("kept lease={ID_A} alias={alias} pid=100 host=swimcoach"),
            "grace seconds=10".to_string(),
            format!("started lease={ID_A} alias={alias} pid=200 host=swimcoach"),
        ]
    );
    assert_eq!(w.sleeps_secs(), vec![5, 5, 5, 5, 10]);
}

/// A wait is a call of the pinned clock's `sleep`, for the whole length: the same timeline on a
/// UTC clock that moves one hour per sleep gives the same sleeps.
#[test]
fn the_waits_do_not_depend_on_the_utc_clock() {
    let w = World::new();
    for _ in 0..4 {
        w.plan_fail(EXITED_AT_ONCE);
    }
    w.on("sleep:30", 1, |w| w.step_utc(3600 * 1000));
    w.on("sleep:60", 1, |w| w.step_utc(-7200 * 1000));
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(w.sleeps_secs(), vec![30, 60]);
}

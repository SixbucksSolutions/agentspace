//! Fast failures and backoff (keeper.md, Watching and Fast failures and backoff; E0.06-T02-S05).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. A fast failure is a start
//! that did not give a live holder, or a tracked holder found gone before it was proven. Only
//! proof of life (a look that finds the tracked holder alive 60 seconds after tracking began, or
//! `-FastFailureSeconds`) resets the count, and a stop, or a tried stop, also does. After the n-th
//! fast failure in a row the backoff is `min(BackoffMax, BackoffBase * 2^(n-1))`, and the wait is
//! that or the interval, whichever is longer. Every sleep is read off the fake clock, so each
//! wait is asserted exactly.

mod common;
mod watch;

use agentspace_keeper_api::{ProcessInfo, Query};
use common::{FakeMatcher, ID_A, ID_B, alias_of, ready, released};
use watch::*;

/// The expected lines and sleeps of `passes` passes that each fail to start, for a backoff that
/// is `wait(n)` seconds after the n-th failure. The last pass logs no wait (`-MaxPasses`).
fn failing_starts(passes: usize, wait: impl Fn(u64) -> u64) -> (Vec<String>, Vec<u64>) {
    let mut lines = Vec::new();
    let mut sleeps = Vec::new();
    let mut t = 0i64;
    for n in 1..=passes as u64 {
        lines.push(failed_start(t, ID_A, EXITED_AT_ONCE));
        if n < passes as u64 {
            let s = wait(n);
            lines.push(backoff(t, s));
            sleeps.push(s);
            t += s as i64;
        }
    }
    (lines, sleeps)
}

fn world_with_failing_starts(n: usize) -> std::rc::Rc<World> {
    let w = World::new();
    for _ in 0..n {
        w.plan_fail(EXITED_AT_ONCE);
    }
    w
}

#[test]
fn backoff_doubles_from_the_base_to_the_longest_and_stays_there() {
    let w = world_with_failing_starts(8);
    let ran = w.run(&[
        "-IntervalSeconds",
        "5",
        "-BackoffBaseSeconds",
        "10",
        "-BackoffMaxSeconds",
        "100",
        "-MaxPasses",
        "8",
    ]);
    assert_eq!(ran.exit, 0);
    let (lines, sleeps) = failing_starts(8, |n| (10u64 << (n - 1)).min(100));
    assert_eq!(sleeps, vec![10, 20, 40, 80, 100, 100, 100]);
    assert_lines(&w, &ran, lines);
    assert_eq!(w.sleeps_secs(), sleeps);
}

#[test]
fn the_interval_is_a_floor_under_the_backoff_and_the_logged_seconds_include_it() {
    let w = world_with_failing_starts(6);
    let ran = w.run(&["-IntervalSeconds", "100", "-MaxPasses", "6"]);
    assert_eq!(ran.exit, 0);
    let (lines, sleeps) = failing_starts(6, |n| (30u64 << (n - 1)).clamp(100, 300));
    assert_eq!(sleeps, vec![100, 100, 120, 240, 300]);
    assert_lines(&w, &ran, lines);
    assert_eq!(w.sleeps_secs(), sleeps);
}

#[test]
fn a_longest_backoff_equal_to_the_base_gives_the_same_wait_every_time() {
    let w = world_with_failing_starts(4);
    let ran = w.run(&[
        "-BackoffBaseSeconds",
        "50",
        "-BackoffMaxSeconds",
        "50",
        "-MaxPasses",
        "4",
    ]);
    assert_eq!(ran.exit, 0);
    let (lines, sleeps) = failing_starts(4, |_| 50);
    assert_lines(&w, &ran, lines);
    assert_eq!(w.sleeps_secs(), sleeps);
}

/// With the defaults, a started holder that keeps dying unproven waits 95 (`grace`), 95 (`grace`),
/// 120 (`backoff`), 240 (`backoff`), then 300 (`backoff`) seconds before each next start, and one
/// line says which: `backoff` when the backoff is at least the grace, else `grace`.
#[test]
fn a_holder_that_keeps_dying_unproven_waits_95_95_120_240_300() {
    let w = World::new();
    for pid in 100..107 {
        w.plan_holder(pid, Some(secs(10)));
    }
    let ran = w.run(&["-MaxPasses", "7"]);
    assert_eq!(ran.exit, 0);
    let waits = [95u64, 95, 120, 240, 300, 300];
    let mut lines = vec![started(0, ID_A, 100)];
    let mut sleeps = Vec::new();
    let mut t = 0i64;
    for (i, wait) in waits.iter().enumerate() {
        // The look comes one interval after the pass, and finds the holder gone.
        t += 30;
        sleeps.push(30);
        lines.push(if *wait == 95 {
            grace(t, *wait)
        } else {
            backoff(t, *wait)
        });
        sleeps.push(*wait);
        t += *wait as i64;
        lines.push(started(t, ID_A, 101 + i as u32));
    }
    assert_lines(&w, &ran, lines);
    assert_eq!(w.sleeps_secs(), sleeps);
    // No wait is longer than the greatest of the grace, the longest backoff and the interval.
    assert!(w.sleeps_secs().iter().all(|s| *s <= 300));
    // Each wait is logged, then waited in full.
    for s in w.sleeps().iter().filter(|s| s.secs > 30) {
        assert!(
            s.last_line.ends_with(&format!(" seconds={}", s.secs)),
            "The wait of {} seconds was not logged with its seconds: {}",
            s.secs,
            s.last_line
        );
    }
}

/// Only proof of life resets the count. Two failed starts make a count of 2; a holder then lives
/// to be proven (the count goes to 0) and dies; the next failed start waits the first backoff, 30,
/// and not the third, 120. A proven holder's end is the grace, never a backoff.
#[test]
fn a_proven_holder_resets_the_count() {
    let w = World::new();
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_holder(100, Some(secs(100)));
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_fail(EXITED_AT_ONCE);
    let ran = w.run(&["-MaxPasses", "8"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            failed_start(30, ID_A, EXITED_AT_ONCE),
            backoff(30, 60),
            started(90, ID_A, 100),
            // The look at +120 finds it alive, age 30. At +150 its age is 60: proven.
            kept(120, ID_A, 100),
            kept(150, ID_A, 100),
            kept(180, ID_A, 100),
            // It ended at +190 and the look at +210 finds it gone: proven, so the grace.
            grace(210, 95),
            failed_start(305, ID_A, EXITED_AT_ONCE),
            backoff(305, 30),
            failed_start(335, ID_A, EXITED_AT_ONCE),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 60, 30, 30, 30, 30, 95, 30]);
}

/// A holder started and found gone before the proof is a fast failure, and a holder proven at
/// exactly the threshold (`-FastFailureSeconds`) is proven: the look at an age of 10 with a
/// threshold of 10 proves it, so its end then is the grace (10), not the backoff (40).
#[test]
fn a_holder_alive_at_the_threshold_is_proven() {
    let w = World::new();
    w.plan_holder(100, Some(secs(12)));
    w.plan_holder(200, None);
    let ran = w.run(&[
        "-IntervalSeconds",
        "5",
        "-ReleaseGraceSeconds",
        "10",
        "-BackoffBaseSeconds",
        "40",
        "-FastFailureSeconds",
        "10",
        "-MaxPasses",
        "4",
    ]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            kept(5, ID_A, 100),
            kept(10, ID_A, 100),
            grace(15, 10),
            started(25, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![5, 5, 5, 10]);
}

/// The same holder with a threshold of 11: at an age of 10 it is not yet proven, so its end is a
/// fast failure, and the wait is the longer backoff, 40, not the grace, 10.
#[test]
fn a_holder_found_gone_a_second_short_of_the_threshold_is_a_fast_failure() {
    let w = World::new();
    w.plan_holder(100, Some(secs(12)));
    w.plan_holder(200, None);
    let ran = w.run(&[
        "-IntervalSeconds",
        "5",
        "-ReleaseGraceSeconds",
        "10",
        "-BackoffBaseSeconds",
        "40",
        "-FastFailureSeconds",
        "11",
        "-MaxPasses",
        "4",
    ]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            kept(5, ID_A, 100),
            kept(10, ID_A, 100),
            backoff(15, 40),
            started(55, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![5, 5, 5, 40]);
}

/// A stop, or a tried stop, resets the count: a failed start makes a count of 1, a holder is
/// started and then stopped (its lease released), and the next failed start waits 30, not 60.
#[test]
fn a_stop_resets_the_count() {
    let w = World::new();
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_holder(100, None);
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_fail(EXITED_AT_ONCE);
    w.on("lock:pass", 3, |w| {
        w.set_leases(vec![released(ID_A, FAR), ready(ID_B, FAR)]);
    });
    let ran = w.run(&["-MaxPasses", "5"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            started(30, ID_A, 100),
            stopped(60, ID_A, 100),
            grace(60, 95),
            failed_start(155, ID_B, EXITED_AT_ONCE),
            backoff(155, 30),
            failed_start(185, ID_B, EXITED_AT_ONCE),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 95, 30]);
}

/// A start that succeeds does not reset the count: two failed starts, then a holder that dies
/// unproven, make a count of 3, and the wait is the third backoff, 120.
#[test]
fn a_start_that_succeeds_does_not_reset_the_count() {
    let w = World::new();
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_holder(100, Some(secs(10)));
    w.plan_holder(101, None);
    let ran = w.run(&["-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            failed_start(30, ID_A, EXITED_AT_ONCE),
            backoff(30, 60),
            started(90, ID_A, 100),
            backoff(120, 120),
            started(240, ID_A, 101),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 60, 30, 120]);
}

/// An idle pass does not reset the count.
#[test]
fn an_idle_pass_does_not_reset_the_count() {
    let w = world_with_failing_starts(3);
    w.on("lock:pass", 2, |w| w.set_leases(vec![]));
    w.on("lock:pass", 3, |w| w.set_leases(vec![ready(ID_A, FAR)]));
    let ran = w.run(&["-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            idle(30),
            failed_start(60, ID_A, EXITED_AT_ONCE),
            backoff(60, 60),
            failed_start(120, ID_A, EXITED_AT_ONCE),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 60]);
}

/// A skipped pass, like an idle one, changes nothing: the count stays and no wait is logged for it.
#[test]
fn a_skipped_pass_does_not_reset_the_count() {
    let w = world_with_failing_starts(3);
    w.pass_modes.borrow_mut().push_back(common::PassMode::Held);
    w.pass_modes
        .borrow_mut()
        .push_back(common::PassMode::Skipped);
    let ran = w.run(&["-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            skipped(30),
            failed_start(60, ID_A, EXITED_AT_ONCE),
            backoff(60, 60),
            failed_start(120, ID_A, EXITED_AT_ONCE),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 60]);
}

/// With a fast failure the wait is the longer of the grace and the backoff, never their sum, and a
/// grace set above `-BackoffMaxSeconds` therefore waits the grace, logged `grace`.
#[test]
fn a_grace_above_the_longest_backoff_is_what_is_waited() {
    let w = World::new();
    for pid in 100..104 {
        w.plan_holder(pid, Some(secs(10)));
    }
    let ran = w.run(&["-ReleaseGraceSeconds", "400", "-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            grace(30, 400),
            started(430, ID_A, 101),
            grace(460, 400),
            started(860, ID_A, 102),
            grace(890, 400),
            started(1290, ID_A, 103),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 400, 30, 400, 30, 400]);
}

/// When the backoff equals the grace, the line says `backoff`: it is at least the grace.
#[test]
fn a_backoff_equal_to_the_grace_is_logged_as_the_backoff() {
    let w = World::new();
    w.plan_holder(100, Some(secs(10)));
    w.plan_holder(200, None);
    let ran = w.run(&[
        "-ReleaseGraceSeconds",
        "60",
        "-BackoffBaseSeconds",
        "60",
        "-MaxPasses",
        "2",
    ]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            backoff(30, 60),
            started(90, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 60]);
}

/// One iteration can wait twice, and the two waits add: a proven holder's end gives `grace
/// seconds=95`, then a failed start gives `backoff seconds=30` before the next pass.
#[test]
fn a_proven_holders_end_then_a_failed_start_wait_twice() {
    let w = World::new();
    w.adopt(100, ID_A, Some(secs(20)));
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_holder(200, None);
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            grace(30, 95),
            failed_start(125, ID_A, EXITED_AT_ONCE),
            backoff(125, 30),
            started(155, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 95, 30]);
}

/// The interval is a floor under the grace as well.
#[test]
fn the_interval_is_a_floor_under_the_grace() {
    let w = World::new();
    w.adopt(100, ID_A, Some(secs(50)));
    w.plan_holder(200, None);
    let ran = w.run(&["-IntervalSeconds", "100", "-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![kept(0, ID_A, 100), grace(100, 100), started(200, ID_A, 200)],
    );
    assert_eq!(w.sleeps_secs(), vec![100, 100]);
}

/// A holder found gone when a look cannot open it, finds another process under its PID, or
/// cannot read its command line, is no longer tracked: if it was never proven, that is a fast
/// failure (the wait is the backoff, 40, over the grace, 10).
fn unproven_holder_not_found_by_a_look(answer: impl Fn(&World) + 'static) {
    let w = World::new();
    w.plan_holder(100, None);
    w.plan_holder(200, None);
    w.on("sleep:5", 1, answer);
    let ran = w.run(&[
        "-IntervalSeconds",
        "5",
        "-ReleaseGraceSeconds",
        "10",
        "-BackoffBaseSeconds",
        "40",
        "-MaxPasses",
        "2",
    ]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            backoff(5, 40),
            started(45, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![5, 40]);
    assert!(
        w.stops().is_empty(),
        "A process the keeper cannot vouch for is left alone"
    );
}

#[test]
fn a_look_that_cannot_open_the_holder_finds_it_gone() {
    unproven_holder_not_found_by_a_look(|w| w.answer_query(100, Query::CannotOpen(5)));
}

#[test]
fn a_look_that_finds_another_process_under_the_pid_finds_it_gone() {
    unproven_holder_not_found_by_a_look(|w| {
        w.answer_query(
            100,
            Query::Running(ProcessInfo {
                process_started_at: "2026-11-19T11:00:00.000Z".to_string(),
                command_line: Some(FakeMatcher::command_for(&w.ssh, &alias_of(ID_A))),
            }),
        );
    });
}

#[test]
fn a_look_that_cannot_read_the_command_line_finds_it_gone() {
    unproven_holder_not_found_by_a_look(|w| {
        let rec = w.record().unwrap();
        w.answer_query(
            100,
            Query::Running(ProcessInfo {
                process_started_at: rec.process_started_at,
                command_line: None,
            }),
        );
    });
}

/// An adopted holder is proven from adoption with its count unchanged (architect, 2026-11-19, for
/// E0.06-T02-S05, ruling 2): two failed starts make a count of 2, a trusted holder the keeper did
/// not start is then adopted, and it ends before a look 60 seconds after tracking began. Its end
/// is the grace (proven), and the next failed start is the third in a row: 120, not 30.
#[test]
fn an_adoption_leaves_the_count_unchanged() {
    let w = World::new();
    for _ in 0..4 {
        w.plan_fail(EXITED_AT_ONCE);
    }
    // The third pass, at +90, finds a recorded holder that lives 20 seconds.
    w.on("lock:pass", 3, |w| {
        w.adopt(500, ID_A, Some(secs(20)));
    });
    let ran = w.run(&["-MaxPasses", "5"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            failed_start(30, ID_A, EXITED_AT_ONCE),
            backoff(30, 60),
            kept(90, ID_A, 500),
            grace(120, 95),
            failed_start(215, ID_A, EXITED_AT_ONCE),
            backoff(215, 120),
            failed_start(335, ID_A, EXITED_AT_ONCE),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 60, 30, 95, 120]);
}

/// A later look at least 60 seconds after an adopted holder's tracking began, that finds it alive,
/// resets the count, as for a holder the keeper started (ruling 2): after two failed starts and
/// an adopted holder seen alive at +150, the next failed start is the first in a row, 30.
#[test]
fn an_adopted_holder_seen_alive_after_the_proof_resets_the_count() {
    let w = World::new();
    for _ in 0..4 {
        w.plan_fail(EXITED_AT_ONCE);
    }
    w.on("lock:pass", 3, |w| {
        w.adopt(500, ID_A, Some(secs(100)));
    });
    let ran = w.run(&["-MaxPasses", "8"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            failed_start(30, ID_A, EXITED_AT_ONCE),
            backoff(30, 60),
            kept(90, ID_A, 500),
            kept(120, ID_A, 500),
            // The look at +150 finds it alive 60 seconds after its tracking began: the count is 0.
            kept(150, ID_A, 500),
            kept(180, ID_A, 500),
            // It ended at +190; the look at +210 finds it gone.
            grace(210, 95),
            failed_start(305, ID_A, EXITED_AT_ONCE),
            backoff(305, 30),
            failed_start(335, ID_A, EXITED_AT_ONCE),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 60, 30, 30, 30, 30, 95, 30]);
}

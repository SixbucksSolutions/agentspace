//! The keeper's timelines, one per row of keeper.md's "What a keeper does over time" (E0.06-T02-S05).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/keeper.md and the pinned `agentspace-keeper-api`, with the defaults of the arguments:
//! 30 seconds for the interval and the first backoff, 300 for the longest backoff, 95 for the
//! release grace and 60 for the proof of a holder. Each test fixes its own times: the first pass is
//! at `at(0)`, and every sleep moves the fake clock on.

mod common;
mod watch;

use common::{ID_A, ID_B, LATER, ready, released};
use watch::*;

/// Logon with a lease ready and no record: `started`, then `kept` every 30 seconds or so.
#[test]
fn logon_with_a_ready_lease_and_no_record_starts_then_keeps_every_interval() {
    let w = World::new();
    w.plan_holder(100, None);
    let ran = w.run(&["-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            kept(30, ID_A, 100),
            kept(60, ID_A, 100),
            kept(90, ID_A, 100),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 30]);
    assert_eq!(w.starts().len(), 1);
    assert!(w.stops().is_empty());
    assert_eq!(w.record().map(|r| r.pid), Some(100));
}

/// A keeper with no holder to track starts its first one at once: nothing is waited before it.
#[test]
fn a_keeper_with_no_holder_starts_its_first_one_at_once() {
    let w = World::new();
    w.plan_holder(100, None);
    let ran = w.run(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![started(0, ID_A, 100)]);
    assert!(w.sleeps().is_empty());
}

/// Logon or restart with a running, recorded holder: `kept` on the first pass, with no new holder
/// and no grace.
#[test]
fn a_restart_with_a_running_recorded_holder_keeps_it_on_the_first_pass() {
    let w = World::new();
    w.adopt(100, ID_A, None);
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![kept(0, ID_A, 100), kept(30, ID_A, 100), kept(60, ID_A, 100)],
    );
    assert_eq!(w.count_prefix("start:"), 0);
    assert_eq!(w.count_prefix("stop:"), 0);
    assert_eq!(w.sleeps_secs(), vec![30, 30]);
}

/// The look reads the process only, never the record, and a keeper with no holder to track has
/// nothing to look at before its first pass.
#[test]
fn the_first_pass_has_no_look_before_it() {
    let w = World::new();
    w.adopt(100, ID_A, None);
    w.run(&["-MaxPasses", "1"]);
    let names = w.names();
    let first_pass = names.iter().position(|n| n == "lock:pass").unwrap();
    assert!(
        names[..first_pass].iter().all(|n| !n.starts_with("query:")),
        "Nothing is looked at before the first pass: {names:?}"
    );
}

/// The look queries the tracked holder by PID before each pass, and never reads the record.
#[test]
fn the_look_queries_the_tracked_holder_before_each_pass() {
    let w = World::new();
    w.plan_holder(100, None);
    w.run(&["-MaxPasses", "3"]);
    let names = w.names();
    let spans = w.pass_spans();
    assert_eq!(spans.len(), 3);
    for pair in spans.windows(2) {
        let (_, release) = pair[0];
        let (next_lock, _) = pair[1];
        let gap = &names[release.unwrap() + 1..next_lock];
        assert!(
            gap.iter().any(|n| n == "query:100"),
            "The tracked holder was not looked at between two passes: {gap:?}"
        );
        assert!(
            gap.iter().all(|n| !n.starts_with("store:")),
            "The look read or wrote the record: {gap:?}"
        );
    }
}

/// The lease is released or expires: `stopped`, then `grace seconds=95`, then `idle`.
#[test]
fn an_expiring_lease_gives_stopped_then_grace_then_idle() {
    let w = World::new();
    w.set_leases(vec![ready(ID_A, "2026-11-19T12:01:00Z")]);
    w.adopt(100, ID_A, None);
    let ran = w.run(&["-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    // The lease expires at the pass at +60: a lease whose expires_at equals the pass time is
    // expired. The grace line carries the second it is written, +60, and 95 seconds are waited.
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            kept(30, ID_A, 100),
            stopped(60, ID_A, 100),
            grace(60, 95),
            idle(155),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 95]);
    assert_eq!(w.stops().len(), 1);
    assert_eq!(w.count_prefix("start:"), 0);
    assert!(w.record().is_none(), "The record is removed with the stop");
}

/// The lease expires and another is ready: `stopped`, `grace seconds=95`, then `started` for the
/// other lease. The holder through the first lease was kept while it was ready, though the other
/// expires later.
#[test]
fn an_expiring_lease_with_another_ready_gives_stopped_grace_then_started() {
    let w = World::new();
    w.set_leases(vec![
        ready(ID_A, "2026-11-19T12:01:00Z"),
        ready(ID_B, LATER),
    ]);
    w.adopt(100, ID_A, None);
    w.plan_holder(200, None);
    let ran = w.run(&["-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            kept(30, ID_A, 100),
            stopped(60, ID_A, 100),
            grace(60, 95),
            started(155, ID_B, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 95]);
    assert_eq!(w.starts()[0].0, ID_B);
}

/// A lease released while its holder runs is as an expiry: the stop, then the grace.
#[test]
fn a_released_lease_gives_stopped_then_grace() {
    let w = World::new();
    w.adopt(100, ID_A, None);
    w.on("lock:pass", 2, |w| {
        w.set_leases(vec![released(ID_A, FAR)]);
    });
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            stopped(30, ID_A, 100),
            grace(30, 95),
            idle(125),
        ],
    );
}

/// The holder is killed: within an interval the look finds it gone and the keeper waits the grace
/// (`grace seconds=95`); then `started`. The holder was adopted, so it counts as proven and the
/// wait is the grace.
#[test]
fn a_killed_holder_gives_grace_at_the_next_look_then_started() {
    let w = World::new();
    w.adopt(100, ID_A, Some(secs(40)));
    w.plan_holder(200, None);
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            kept(30, ID_A, 100),
            grace(60, 95),
            started(155, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 95]);
    // The grace is logged, then waited in full, and nothing is started before it has passed.
    let sleeps = w.sleeps();
    assert!(sleeps[2].last_line.ends_with(" grace seconds=95"));
    let start = w.mono_of(&format!("start:{ID_A}"), 1);
    assert!(
        start >= sleeps[2].at + secs(95),
        "The grace counts from the look, and nothing starts before it has passed"
    );
    // The pass that follows finds the record naming a process that is gone, and removes it.
    assert!(w.names().contains(&"store:remove".to_string()));
}

/// The holder ends between a look and its pass: `lost` for it, then `grace seconds=95`; then
/// `started` if a lease is ready (card #0068). No holder is started in the pass that logged
/// `lost`.
#[test]
fn a_holder_that_ends_between_the_look_and_the_pass_gives_lost_then_grace_then_started() {
    let w = World::new();
    w.adopt(100, ID_A, None);
    w.plan_holder(200, None);
    w.on("lock:pass", 2, |w| w.kill(100));
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            lost(30, ID_A, 100),
            grace(30, 95),
            started(125, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 95]);
    let names = w.names();
    let spans = w.pass_spans();
    let (lock2, rel2) = spans[1];
    assert!(
        names[lock2..=rel2.unwrap()]
            .iter()
            .all(|n| !n.starts_with("start:")),
        "The pass that logged lost started a holder: {names:?}"
    );
    assert_eq!(w.count_prefix("start:"), 1);
}

/// A holder whose record is removed is lost to the keeper, and the process is left alone.
#[test]
fn a_holder_whose_record_is_removed_gives_lost_then_grace_and_is_left_running() {
    let w = World::new();
    w.adopt(100, ID_A, None);
    w.plan_holder(200, None);
    w.on("lock:pass", 2, |w| w.set_record(None));
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            lost(30, ID_A, 100),
            grace(30, 95),
            started(125, ID_A, 200),
        ],
    );
    assert!(
        w.is_alive(100),
        "A process that is no longer trusted is left running"
    );
    assert!(w.stops().is_empty());
}

/// A holder that exits at once (the ports still held): a `failed` line for the start, then
/// `backoff seconds=30`.
#[test]
fn a_holder_that_exits_at_once_gives_failed_then_backoff_30() {
    let w = World::new();
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_fail(EXITED_AT_ONCE);
    let ran = w.run(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            failed_start(30, ID_A, EXITED_AT_ONCE),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30]);
    assert!(w.sleeps()[0].last_line.ends_with(" backoff seconds=30"));
}

/// Another connection holds the forwards: a `failed` line after each start, with `backoff` waits
/// growing to `backoff seconds=300`, and staying there.
#[test]
fn another_connection_holding_the_forwards_gives_backoffs_growing_to_300() {
    let w = World::new();
    for _ in 0..8 {
        w.plan_fail(EXITED_AT_ONCE);
    }
    let ran = w.run(&["-MaxPasses", "8"]);
    assert_eq!(ran.exit, 0);
    let waits = [30u64, 60, 120, 240, 300, 300, 300];
    let mut expected = Vec::new();
    let mut t = 0i64;
    for wait in waits {
        expected.push(failed_start(t, ID_A, EXITED_AT_ONCE));
        expected.push(backoff(t, wait));
        t += wait as i64;
    }
    expected.push(failed_start(t, ID_A, EXITED_AT_ONCE));
    assert_lines(&w, &ran, expected);
    assert_eq!(w.sleeps_secs(), waits.to_vec());
}

/// A second keeper starts: one `another keeper is running pid=<n>` line, from the second keeper,
/// which exits 0 with no pass and no wait.
#[test]
fn a_second_keeper_logs_who_runs_and_exits_without_a_pass() {
    let w = World::new();
    *w.keeper_mode.borrow_mut() = KeeperMode::Busy(Some(4242));
    let ran = w.run(&["-MaxPasses", "5"]);
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![another_keeper(0, Some(4242))]);
    assert_eq!(w.count("lock:pass"), 0);
    assert!(w.sleeps().is_empty());
}

/// The keeper is killed: the task starts a new keeper, whose first pass logs `kept` for a holder
/// that still runs.
#[test]
fn a_new_keeper_keeps_the_holder_the_last_one_started() {
    let w = World::new();
    w.plan_holder(100, None);
    let first = w.run(&["-MaxPasses", "1"]);
    assert_eq!(first.exit, 0);
    assert_eq!(first.stdout, started(0, ID_A, 100));
    // The first keeper is gone; its holder runs on and its record stands.
    let second = w.run(&["-MaxPasses", "2"]);
    assert_eq!(second.exit, 0);
    assert_eq!(
        second.stdout,
        [kept(0, ID_A, 100), kept(30, ID_A, 100)].concat()
    );
    assert_eq!(
        w.starts().len(),
        1,
        "No new holder, and no new login on the box"
    );
    assert_eq!(w.sleeps_secs(), vec![30]);
    assert_eq!(w.count("lock:keeper"), 2);
}

/// The keeper is killed and its holder is gone with it: the new keeper's first pass starts one at
/// once if a lease is ready, with no grace.
#[test]
fn a_new_keeper_whose_predecessors_holder_is_dead_starts_at_once_with_no_grace() {
    let w = World::new();
    w.plan_holder(100, None);
    w.plan_holder(200, None);
    let first = w.run(&["-MaxPasses", "1"]);
    assert_eq!(first.exit, 0);
    w.kill(100);
    let second = w.run(&["-MaxPasses", "1"]);
    assert_eq!(second.exit, 0);
    assert_eq!(second.stdout, started(0, ID_A, 200));
    assert!(
        w.sleeps().is_empty(),
        "No grace is waited by a keeper that had no holder"
    );
}

/// The same, with no lease ready: the first pass logs `idle`.
#[test]
fn a_new_keeper_with_a_dead_record_and_no_ready_lease_logs_idle() {
    let w = World::new();
    w.adopt(100, ID_A, Some(secs(0)));
    w.set_leases(vec![]);
    let ran = w.run(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0)]);
}

/// The keeper's state lives only in its process: the count of fast failures starts at 0 in a new
/// keeper. The first keeper's third failure would have waited 120; the new keeper's first waits 30.
#[test]
fn a_new_keeper_starts_with_no_count_of_fast_failures() {
    let w = World::new();
    for _ in 0..5 {
        w.plan_fail(EXITED_AT_ONCE);
    }
    let first = w.run(&["-MaxPasses", "3"]);
    assert_eq!(first.exit, 0);
    assert_eq!(w.sleeps_secs(), vec![30, 60]);
    let second = w.run(&["-MaxPasses", "2"]);
    assert_eq!(second.exit, 0);
    assert_eq!(w.sleeps_secs(), vec![30, 60, 30]);
    assert!(second.stdout.contains(" backoff seconds=30\n"));
}

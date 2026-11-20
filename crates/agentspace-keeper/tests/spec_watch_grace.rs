//! Release grace, #0068's `lost` row and adoption (keeper.md, Watching and Release grace;
//! E0.06-T02-S05).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. When a look finds a tracked
//! holder gone, the keeper MUST NOT start a holder until `-ReleaseGraceSeconds` has passed,
//! counted from that look; after a stop, from the end of the pass that stopped it; after a pass
//! that logged `lost`, from the end of that pass. A tracked holder a pass cannot find is `lost`,
//! and that pass starts nothing. A keeper adopts a trusted holder it did not start, as proven.

mod common;
mod watch;

use agentspace_keeper_api::{ProcessInfo, Query, Stopped};
use common::{ID_A, ID_B, LATER, alias_of, holder_for, ready, released};
use watch::*;

/// A `failed` line of a stop: it names the record's lease, alias and PID.
fn failed_stop(t: i64, lease: &str, pid: u32, reason: &str) -> String {
    common::line(
        &at(t),
        "failed",
        lease,
        &alias_of(lease),
        &pid.to_string(),
        Some(reason),
    )
}

const SHORT: [&str; 8] = [
    "-IntervalSeconds",
    "5",
    "-ReleaseGraceSeconds",
    "10",
    "-BackoffBaseSeconds",
    "40",
    "-MaxPasses",
    "0",
];

/// The flags of a timeline with a short interval, a grace of 10 and a first backoff of 40, so a
/// proven holder's end (the grace, 10) differs from an unproven one's (the backoff, 40).
fn short(passes: &str) -> Vec<&str> {
    let mut v = SHORT.to_vec();
    *v.last_mut().unwrap() = passes;
    v
}

#[test]
fn a_stop_is_followed_by_the_grace_counted_from_the_end_of_the_stopping_pass() {
    let w = World::new();
    w.set_leases(vec![
        ready(ID_A, "2026-11-19T12:00:10Z"),
        ready(ID_B, LATER),
    ]);
    w.adopt(100, ID_A, None);
    w.plan_holder(200, None);
    let ran = w.run(&[
        "-IntervalSeconds",
        "5",
        "-ReleaseGraceSeconds",
        "20",
        "-MaxPasses",
        "4",
    ]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            kept(5, ID_A, 100),
            stopped(10, ID_A, 100),
            grace(10, 20),
            started(30, ID_B, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![5, 5, 20]);
    let sleeps = w.sleeps();
    assert!(w.mono_of(&format!("start:{ID_B}"), 1) >= sleeps[2].at + secs(20));
}

/// The grace applies to a holder the keeper adopted, whether it died or was stopped: an adopted
/// holder is proven, so its end is the grace (10), not the backoff (40).
#[test]
fn an_adopted_holder_that_ends_gets_the_grace() {
    let w = World::new();
    w.adopt(100, ID_A, Some(secs(7)));
    w.plan_holder(200, None);
    let ran = w.run(&short("3"));
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            kept(5, ID_A, 100),
            grace(10, 10),
            started(20, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![5, 5, 10]);
}

/// A tracked holder whose pass finds no trusted holder is `lost`: nothing is started in that
/// pass; the wait after it is the grace, counted from its end.
#[test]
fn a_lost_proven_holder_waits_the_grace_after_the_pass_and_starts_nothing_in_it() {
    let w = World::new();
    w.adopt(100, ID_A, None);
    w.plan_holder(200, None);
    w.on("lock:pass", 2, |w| w.kill(100));
    let ran = w.run(&short("3"));
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            lost(5, ID_A, 100),
            grace(5, 10),
            started(15, ID_A, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![5, 10]);
    let sleeps = w.sleeps();
    assert!(w.mono_of(&format!("start:{ID_A}"), 1) >= sleeps[1].at + secs(10));
    // The pass that logged `lost` started nothing: the one start is the pass after the wait.
    let names = w.names();
    let (lock2, rel2) = w.pass_spans()[1];
    assert!(
        names[lock2..=rel2.unwrap()]
            .iter()
            .all(|n| !n.starts_with("start:"))
    );
}

/// An unproven tracked holder that is lost counts a fast failure each time, so the wait after
/// `lost` is the longer backoff once that passes the grace: 95, 95, then 120.
#[test]
fn an_unproven_holder_lost_each_time_waits_95_95_then_a_backoff_of_120() {
    let w = World::new();
    for pid in 100..104 {
        w.plan_holder(pid, None);
    }
    w.on("lock:pass", 2, |w| w.kill(100));
    w.on("lock:pass", 4, |w| w.kill(101));
    w.on("lock:pass", 6, |w| w.kill(102));
    let ran = w.run(&["-MaxPasses", "7"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            lost(30, ID_A, 100),
            grace(30, 95),
            started(125, ID_A, 101),
            lost(155, ID_A, 101),
            grace(155, 95),
            started(250, ID_A, 102),
            lost(280, ID_A, 102),
            backoff(280, 120),
            started(400, ID_A, 103),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 95, 30, 95, 30, 120]);
}

/// `lost` is logged whether or not a lease is ready: here the lease has just expired, and the
/// pass logs `lost`, not `idle`.
#[test]
fn lost_is_logged_when_no_lease_is_ready_and_then_idle_follows_the_grace() {
    let w = World::new();
    w.set_leases(vec![ready(ID_A, "2026-11-19T12:00:30Z")]);
    w.adopt(100, ID_A, None);
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
            idle(125),
        ],
    );
}

/// The record naming a process that is not trusted is lost too, and the process is left running.
#[test]
fn a_record_naming_an_untrusted_process_is_lost_and_the_process_is_left_alone() {
    let w = World::new();
    w.adopt(100, ID_A, None);
    w.plan_holder(200, None);
    w.on("lock:pass", 2, |w| {
        w.answer_query(
            100,
            Query::Running(ProcessInfo {
                process_started_at: "2026-11-19T11:00:00.000Z".to_string(),
                command_line: Some(common::FakeMatcher::command_for(&w.ssh, &alias_of(ID_A))),
            }),
        );
    });
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
    assert!(w.stops().is_empty());
    assert_eq!(w.record().map(|r| r.pid), Some(200));
}

/// The rows are taken in order: a trusted holder in the record wins over the `lost` row. The
/// keeper tracks one holder, the record now names another that is trusted (one a one-pass run
/// started, say), and the pass keeps it. The keeper adopts it with no grace, as proven, in place
/// of the holder it tracked: when it ends, the wait is the grace (10), not the backoff (40).
#[test]
fn a_trusted_record_naming_another_holder_is_kept_and_adopted_with_no_grace() {
    let w = World::new();
    w.plan_holder(100, None);
    w.plan_holder(400, None);
    w.on("lock:pass", 2, |w| {
        let rec = holder_for(300, ID_A);
        w.add_process(&rec, Some(secs(7)));
        w.set_record(Some(rec));
    });
    let ran = w.run(&short("4"));
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            kept(5, ID_A, 300),
            kept(10, ID_A, 300),
            grace(15, 10),
            started(25, ID_A, 400),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![5, 5, 5, 10]);
    // No grace follows the adoption: the wait after the `kept` of the adopted holder is the
    // interval alone, with no keeper line before it.
    assert!(w.sleeps()[1].last_line.contains(" kept "));
}

/// The same record, with the other holder's lease not ready: the pass stops it, and the grace
/// follows that stop as after any other.
#[test]
fn a_trusted_record_whose_lease_is_not_ready_is_stopped_and_the_grace_follows() {
    let w = World::new();
    w.set_leases(vec![ready(ID_A, FAR), released(ID_B, FAR)]);
    w.plan_holder(100, None);
    w.plan_holder(200, None);
    w.on("lock:pass", 2, |w| {
        let rec = holder_for(300, ID_B);
        w.add_process(&rec, None);
        w.set_record(Some(rec));
    });
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            stopped(30, ID_B, 300),
            grace(30, 95),
            started(125, ID_A, 200),
        ],
    );
    assert_eq!(w.count("stop:300"), 1);
}

/// A tried stop that fails keeps the record and is followed by the grace; the next pass finds the
/// holder still trusted and stops it again.
#[test]
fn a_failed_stop_is_followed_by_the_grace_and_tried_again() {
    let w = World::new();
    w.set_leases(vec![ready(ID_A, "2026-11-19T12:01:00Z")]);
    w.adopt(100, ID_A, None);
    w.set_stop(Stopped::StillRunning);
    let ran = w.run(&["-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    let reason = "The holder is still running after it was stopped.";
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, ID_A, 100),
            kept(30, ID_A, 100),
            failed_stop(60, ID_A, 100, reason),
            grace(60, 95),
            failed_stop(155, ID_A, 100, reason),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 95]);
    assert_eq!(w.stops().len(), 2);
    assert_eq!(w.count_prefix("start:"), 0);
    assert_eq!(w.record().map(|r| r.pid), Some(100), "The record is kept");
}

/// The grace does not apply to a holder stopped because its record could not be written: that
/// failure gets the backoff only, and it is a fast failure, so the backoff grows.
#[test]
fn a_holder_stopped_because_its_record_could_not_be_written_gets_the_backoff_only() {
    let w = World::new();
    *w.write_error.borrow_mut() = Some("disk full".to_string());
    for pid in 100..103 {
        w.plan_holder(pid, None);
    }
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    let reason =
        "The record could not be written, so the holder was stopped, or had exited: disk full";
    assert_lines(
        &w,
        &ran,
        vec![
            failed_stop(0, ID_A, 100, reason),
            backoff(0, 30),
            failed_stop(30, ID_A, 101, reason),
            backoff(30, 60),
            failed_stop(90, ID_A, 102, reason),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 60]);
    assert_eq!(
        w.count("stop:100"),
        1,
        "The unrecorded holder is stopped at once"
    );
}

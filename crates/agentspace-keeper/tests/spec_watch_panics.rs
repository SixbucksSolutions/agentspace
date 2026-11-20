//! A panic in a look, a pass or a wait ends nothing (keeper.md, Fast failures and backoff;
//! Watching; Reasons; architect, 2026-11-19, for E0.06-T02-S05, ruling 3).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. The keeper MUST catch a
//! panic that escapes its look, its pass or its wait, log the matching reason with the panic's
//! message as `<error>`, and go on as for any other error there, to `-MaxPasses`. Each fake panics
//! from a hook of the scripted world, at one named event.

mod common;
mod watch;

use common::ID_A;
use watch::*;

/// A look whose `Processes::query` panics is a look whose check failed: the line says so, the
/// holder is no longer tracked, and, never proven, it is a fast failure, so the wait is the longer
/// of the grace and the first backoff. The next pass finds the record trusted and adopts the
/// holder, and the keeper goes on to `-MaxPasses`.
#[test]
fn a_look_that_panics_is_logged_and_the_keeper_goes_on() {
    let w = World::new();
    w.plan_holder(100, None);
    // The first query of the holder is the look before the second pass.
    w.on("query:100", 1, |_| panic!("The fake query broke."));
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            failed(
                30,
                "The look at the holder stopped on an error: The fake query broke.",
            ),
            grace(30, 95),
            kept(125, ID_A, 100),
            kept(155, ID_A, 100),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 95, 30]);
    assert_eq!(w.count("lock:pass"), 3);
}

/// A pass that panics inside (here the record's read, under the pass lock) is a pass that failed
/// before deciding: the line says so with the panic's message, the pass lock is released, the
/// tracked holder stays tracked, and the next pass keeps it. The panic's message is a `String`.
#[test]
fn a_pass_that_panics_is_logged_and_the_keeper_goes_on() {
    let w = World::new();
    w.plan_holder(100, None);
    w.on("store:read", 2, |_| panic!("The fake record broke: {}", 7));
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, ID_A, 100),
            failed(30, "The pass stopped on an error: The fake record broke: 7"),
            kept(60, ID_A, 100),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30]);
    assert_eq!(w.count("lock:pass"), 3);
    assert_eq!(
        w.count("release:pass"),
        3,
        "The pass lock is released when its pass panics"
    );
    assert_eq!(w.count_prefix("start:"), 1, "The tracked holder is kept");
}

/// A panic whose payload is neither a `&str` nor a `String` is logged with a fixed text.
#[test]
fn a_panic_with_no_message_is_logged_with_a_fixed_text() {
    let w = World::new();
    w.set_leases(vec![]);
    w.on("lock:pass", 1, |_| std::panic::panic_any(42_u8));
    let ran = w.run(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed(
                0,
                "The pass stopped on an error: The panic carried no message.",
            ),
            idle(30),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30]);
}

/// A wait whose `Clock::sleep` panics is logged as the wait's error, and the keeper goes on to its
/// next pass. The fake clock moved nothing for the sleep that panicked.
#[test]
fn a_wait_that_panics_is_logged_and_the_keeper_goes_on() {
    let w = World::new();
    w.set_leases(vec![]);
    w.on("sleep:30", 1, |_| panic!("The fake sleep broke."));
    let ran = w.run(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0),
            failed(0, "The wait stopped on an error: The fake sleep broke."),
            idle(0),
            idle(30),
        ],
    );
    assert_eq!(w.count("lock:pass"), 3);
}

/// A wait whose line panics as it is logged still waits its full time: the line is the wait's
/// error, and the sleep follows.
#[test]
fn a_wait_whose_line_panics_still_waits_its_full_time() {
    let w = World::new();
    w.plan_fail(EXITED_AT_ONCE);
    // The first read of the UTC clock after the first pass is the one for the backoff's line.
    w.on("release:pass", 1, |w| {
        w.panic_on_next_utc_read("The fake UTC clock broke.")
    });
    let ran = w.run(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    let lines: Vec<&str> = ran.stdout.lines().collect();
    assert!(
        lines[0].ends_with(EXITED_AT_ONCE),
        "The first pass failed its start: {lines:?}"
    );
    assert!(
        lines[1].ends_with("reason=The wait stopped on an error: The fake UTC clock broke."),
        "The wait's line panicked: {lines:?}"
    );
    assert_eq!(lines.len(), 3, "The keeper went on to its second pass");
    assert_eq!(
        w.sleeps_secs(),
        vec![30],
        "The wait was still its full time"
    );
}

//! The keeper's start of the guard against a fake clock and a fake guard: when it is due, where in
//! the loop it is made, what it asks of the start, and what it changes in nothing else
//! (docs/keeper.md, Starting the guard: When the keeper starts it, How the keeper starts it, The
//! guard's environment, The keeper's lines for it, What a failed start changes; guard.lock: tried,
//! never held; Watching; Release grace; Conformance cases for the guard's start; E0.08-T03-S03,
//! #0179).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every duration is read off
//! the scripted world's fake clock (`watch`), whose `sleep` advances time, so a time in an
//! expected line is the start time plus the sleeps so far, and a keeper that waits 300 seconds
//! waits none. **The guard is a fake**: the world's keeper is given a `FakeGuard`, which starts
//! nothing and records each try of `guard.lock` and each start (the program, its arguments, its
//! working folder and the variables it was given), and gives the process IDs it was told to. The
//! world still names a stand-in guard and tripwires in `AGENTSPACE_GUARD_CMD` and the three seams,
//! in its throwaway home (AGENTS.md), and `guard.lock` is the real file of that home when a test
//! says so (`use_real_lock`). The real start, with a real stand-in program, is `guard_program.rs`'s.
//!
//! A line is compared through `shape`, which writes a `guard started` line as `GUARD <time>` and a
//! failed start's line as `GUARDFAIL <time> <why>` and leaves every other line as it is. A variable
//! the guard is given is compared by name, and a mismatch is reported by the name alone.

mod common;
mod watch;

use agentspace_keeper::GuardLockTry;
use common::{
    FIRST_PID, ID_A, LockStep, PassMode, StartStep, guard_lines, guard_started_line,
    parse_guard_failed, parse_guard_started, released,
};
use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use watch::{
    EXITED_AT_ONCE, FAR, KeeperMode, World, another_keeper, at, backoff, failed, failed_start,
    grace, idle, kept, skipped, started,
};

const MISSING: &str = "The program that AGENTSPACE_GUARD_CMD names does not exist.";
const NOT_INSTALLED: &str = "The operator program is not installed beside this program.";
const NO_SYSTEM_ROOT: &str =
    "Windows' command interpreter cannot be found, because SystemRoot is not set.";

/// The lines of `raw`, a `guard started` line as `GUARD <time>` and a failed start's as
/// `GUARDFAIL <time> <why>`.
fn shape(raw: &str) -> Vec<String> {
    raw.split_inclusive('\n')
        .map(|l| {
            if l.contains(" guard started pid=") {
                format!("GUARD {}\n", parse_guard_started(l).0)
            } else if l.contains(" reason=The guard could not be started: ") {
                let (time, why) = parse_guard_failed(l);
                format!("GUARDFAIL {time} {why}\n")
            } else {
                l.to_string()
            }
        })
        .collect()
}

/// The expected `GUARD` line for a start at `t` seconds.
fn g(t: i64) -> String {
    format!("GUARD {}\n", at(t))
}

/// The expected `GUARDFAIL` line for a failed start at `t` seconds.
fn gf(t: i64, why: &str) -> String {
    format!("GUARDFAIL {} {why}\n", at(t))
}

/// The run gave exactly `expected`, as `shape` writes them, on stdout and in the log, and nothing
/// on stderr.
fn assert_shape(w: &World, ran: &common::Ran, expected: Vec<String>) {
    assert_eq!(ran.stderr, "", "Nothing is written to stderr");
    let raw = w.log_raw();
    assert_eq!(
        ran.stdout, raw,
        "Every line logged is also written to stdout"
    );
    assert_eq!(shape(&raw), expected, "The log's lines");
}

fn no_leases(w: &World) {
    w.set_leases(vec![]);
}

fn guard_times(raw: &str) -> Vec<String> {
    guard_lines(raw)
        .iter()
        .map(|l| {
            if l.contains(" guard started pid=") {
                parse_guard_started(l).0
            } else {
                parse_guard_failed(l).0
            }
        })
        .collect()
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

/// The command interpreter a `.cmd` guard is run through: `<SystemRoot>\System32\cmd.exe`, with
/// the world's `SystemRoot`.
fn cmd_exe(w: &World) -> PathBuf {
    let root = w.vars.borrow().get("SystemRoot").cloned().unwrap();
    PathBuf::from(format!(r"{}\System32\cmd.exe", root.trim_end_matches('\\')))
}

/// A stub file that is a program as far as the keeper can tell: it exists. Nothing starts it.
fn stub_program(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"stub").unwrap();
    path.to_path_buf()
}

fn operator_beside(w: &World) -> PathBuf {
    stub_program(&w.own_exe.with_file_name("agentspace-operator.exe"))
}

fn guard_cmd_is(w: &World, program: &Path) {
    w.set_var("AGENTSPACE_GUARD_CMD", &program.to_string_lossy());
}

// ---- When the start is due ----

/// G1, on the fake clock. The first iteration starts the guard, before the first pass is taken:
/// one `guard started` line with the process ID the start returned and no `host=`, then the
/// pass's `idle`; one start; the stand-in guard of the throwaway home never ran; exit 0.
#[test]
fn the_guard_is_started_at_the_first_iteration_before_the_pass() {
    let w = World::new();
    no_leases(&w);
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        w.log_raw(),
        format!("{}{}", guard_started_line(&at(0), FIRST_PID), idle(0)),
        "A guard started line with the pid, then the pass's"
    );
    assert_shape(&w, &ran, vec![g(0), idle(0)]);
    assert_eq!(w.guard.starts().len(), 1);
    assert!(
        common::guard_runs_in(&w.home).is_empty(),
        "The guard stand-in ran"
    );
    assert!(!common::tripwire_hit(&w.home), "A tripwire ran");
}

/// G2. A start not again before 300 seconds: passes 299 seconds apart give one start.
#[test]
fn the_guard_is_not_started_again_299_seconds_after_the_first_attempt() {
    let w = World::new();
    no_leases(&w);
    let ran = w.run_guard(&["-IntervalSeconds", "299", "-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0), idle(299)]);
    assert_eq!(w.guard.starts().len(), 1);
}

/// G2. Again at 300 seconds: the second pass is 300 seconds after the first attempt, so a start is
/// due, and is made before that pass.
#[test]
fn the_guard_is_started_again_300_seconds_after_the_first_attempt() {
    let w = World::new();
    no_leases(&w);
    let ran = w.run_guard(&["-IntervalSeconds", "300", "-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0), g(300), idle(300)]);
    assert_eq!(w.guard.starts().len(), 2);
}

/// Over four passes 100 seconds apart, a start at the first and one at 300, none at 100 or 200.
#[test]
fn between_attempts_no_start_is_made_in_the_passes_before_the_interval() {
    let w = World::new();
    no_leases(&w);
    let ran = w.run_guard(&["-IntervalSeconds", "100", "-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(guard_times(&w.log_raw()), vec![at(0), at(300)]);
    assert_eq!(
        w.log(),
        [idle(0), idle(100), idle(200), idle(300)].concat(),
        "Every pass's line"
    );
    assert_eq!(w.guard.starts().len(), 2);
}

/// The interval is `-GuardIntervalSeconds`, 300 by default: an interval of 7 gives starts at 0, 7
/// and 14 over passes every 7 seconds, each from the end of the attempt before.
#[test]
fn the_guard_interval_is_the_argument_and_counts_from_the_last_attempt() {
    let w = World::new();
    no_leases(&w);
    let ran = w.run_guard(&[
        "-IntervalSeconds",
        "7",
        "-GuardIntervalSeconds",
        "7",
        "-MaxPasses",
        "3",
    ]);
    assert_eq!(ran.exit, 0);
    assert_eq!(guard_times(&w.log_raw()), vec![at(0), at(7), at(14)]);
    assert_eq!(w.guard.starts().len(), 3);
}

/// G4. A start that falls due in a wait is made there, and the wait ends when it would have:
/// `-IntervalSeconds 20 -GuardIntervalSeconds 12`: a start at 0, the pass, a start at 12 in the
/// wait, and the second pass still at 20. `-MaxPasses` counts passes, never starts.
#[test]
fn a_start_that_falls_due_in_a_wait_is_made_there_and_the_wait_ends_when_it_would() {
    let w = World::new();
    no_leases(&w);
    let ran = w.run_guard(&[
        "-IntervalSeconds",
        "20",
        "-GuardIntervalSeconds",
        "12",
        "-MaxPasses",
        "2",
    ]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0), g(12), idle(20)]);
    assert_eq!(
        w.sleeps_secs().iter().sum::<u64>(),
        20,
        "The wait ended when it would have"
    );
    assert_eq!(w.count("lock:pass"), 2, "-MaxPasses counts passes alone");
    assert_eq!(w.guard.starts().len(), 2);
}

/// A wait takes one start, however long: a second start that falls due in the same wait is made
/// just before the next pass. A failed start's backoff is a wait with its line already written, and
/// it is unchanged: `backoff seconds=30` at the pass, a start at 10 in the wait, one at 30 before
/// the pass, and the pass at 30.
#[test]
fn a_wait_takes_one_start_and_the_next_is_made_just_before_the_pass() {
    let w = World::new();
    w.plan_fail(EXITED_AT_ONCE);
    w.plan_fail(EXITED_AT_ONCE);
    let ran = w.run_guard(&["-GuardIntervalSeconds", "10", "-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![
            g(0),
            failed_start(0, ID_A, EXITED_AT_ONCE),
            backoff(0, 30),
            g(10),
            g(30),
            failed_start(30, ID_A, EXITED_AT_ONCE),
        ],
    );
    assert_eq!(
        w.sleeps_secs().iter().sum::<u64>(),
        30,
        "The backoff waited its seconds"
    );
    assert_eq!(w.guard.starts().len(), 3);
}

/// A holder ended by a release is a holder a look finds gone, with its release grace (keeper.md,
/// Release grace; D59, as D63 changed it): the release ended its `ssh.exe` and the lease is
/// released. With a guard starting beside it: no `stopped` line, `grace seconds=95` at the look,
/// a start that falls due in the grace made there, and the pass after the grace at 125, never
/// sooner.
#[test]
fn a_holder_ended_by_a_release_is_found_gone_with_its_grace_beside_the_guards_starts() {
    let w = World::new();
    w.adopt(700, ID_A, None);
    // The release writes `released` and ends the holder's process, between the first pass and the
    // next look.
    w.on("sleep:30", 1, |w| {
        w.kill(700);
        w.set_leases(vec![released(ID_A, FAR)]);
    });
    let ran = w.run_guard(&["-GuardIntervalSeconds", "60", "-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![
            g(0),
            kept(0, ID_A, 700),
            grace(30, 95),
            g(60),
            g(125),
            idle(125),
        ],
    );
    assert!(
        w.stops().is_empty(),
        "The keeper stopped nothing: the release ended the holder"
    );
    assert_eq!(
        w.sleeps_secs().iter().sum::<u64>(),
        125,
        "The grace ran its 95 seconds from the look, and the starts changed no wait"
    );
    assert_eq!(w.count("lock:pass"), 2);
    assert_eq!(w.guard.starts().len(), 3);
}

/// The same release for a holder the keeper started less than 60 seconds before it ended:
/// unproven, so a fast failure, and the hold-off the longer of the grace and the backoff, which is
/// the grace's 95 seconds at the first failure, with no `stopped` line. The guard's start changes
/// neither.
#[test]
fn an_unproven_holder_ended_by_a_release_is_a_fast_failure_with_the_guard_running() {
    let w = World::new();
    w.plan_holder(710, None);
    w.on("sleep:30", 1, |w| {
        w.kill(710);
        w.set_leases(vec![released(ID_A, FAR)]);
    });
    let ran = w.run_guard(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![g(0), started(0, ID_A, 710), grace(30, 95), idle(125)],
    );
    assert!(w.stops().is_empty());
    assert_eq!(w.guard.starts().len(), 1);
}

// ---- guard.lock ----

/// G5, on the fake clock. While `guard.lock` is held by another handle sharing nothing, no start and
/// no line, and the passes go on; the lock is closed during the second pass, and the start is made
/// no later than just before the third, without waiting `-GuardIntervalSeconds`. A lock found held
/// is not an attempt.
#[test]
fn a_held_guard_lock_is_not_an_attempt_and_the_start_stays_due() {
    use std::os::windows::fs::OpenOptionsExt;
    let w = World::new();
    no_leases(&w);
    w.guard.use_real_lock();
    std::fs::create_dir_all(w.home.join(".agentspace")).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(w.home.join(".agentspace").join("guard.lock"))
        .expect("The test holds guard.lock");
    let lock = Rc::new(RefCell::new(Some(lock)));
    let held = lock.clone();
    w.on("lock:pass", 2, move |_| {
        held.borrow_mut().take();
    });
    let ran = w.run_guard(&["-IntervalSeconds", "1", "-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    let shaped = shape(&w.log_raw());
    assert_eq!(shaped.len(), 4, "{shaped:?}");
    assert_eq!(shaped[0], idle(0), "No line while the lock is held");
    assert_eq!(shaped[1], idle(1), "The passes go on");
    assert!(
        shaped[2] == g(1) || shaped[2] == g(2),
        "The start comes once the lock is free and no later than before the third pass: {shaped:?}"
    );
    assert_eq!(shaped[3], idle(2));
    assert_eq!(w.guard.starts().len(), 1);
    assert!(
        !w.guard.tries().is_empty() && w.guard.tries().iter().all(|h| same(h, &w.home)),
        "The lock is tried under the keeper's home"
    );
    drop(lock);
}

/// A lock found held is tried again at the next point the keeper may try (keeper.md, When the
/// keeper starts it, P18 and P19): just before each pass, and once in a wait, at its beginning
/// when the start is already due. Scripted, with no file: held for the first three tries. The
/// tries are before the first pass, at the beginning of its wait, before the second pass, and at
/// the beginning of its wait, where it is free: the start is made at 10 seconds, after the second
/// pass's line and before the third, with no wait of the guard interval.
#[test]
fn a_start_found_held_is_tried_again_at_the_next_point_it_may() {
    let w = World::new();
    no_leases(&w);
    w.guard.script_locks(vec![
        LockStep::Answer(GuardLockTry::Held),
        LockStep::Answer(GuardLockTry::Held),
        LockStep::Answer(GuardLockTry::Held),
    ]);
    let ran = w.run_guard(&["-IntervalSeconds", "10", "-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![idle(0), idle(10), g(10), idle(20)]);
    assert_eq!(w.guard.tries().len(), 4, "{:?}", w.guard.tries());
    assert_eq!(w.guard.starts().len(), 1);
}

/// A lock found held starts nothing in the pass either, and the pass lines are as without a guard.
#[test]
fn a_keeper_whose_lock_is_always_held_starts_no_guard_and_logs_no_guard_line() {
    let w = World::new();
    no_leases(&w);
    w.guard.script_locks(
        (0..20)
            .map(|_| LockStep::Answer(GuardLockTry::Held))
            .collect(),
    );
    let ran = w.run_guard(&["-IntervalSeconds", "30", "-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        w.log_raw(),
        [idle(0), idle(30), idle(60), idle(90)].concat()
    );
    assert!(w.guard.starts().is_empty());
}

/// G6, on the fake clock. A folder in place of `guard.lock`: the attempt fails with the lock's
/// reason and Windows' error number alone, nothing is started, and the failure repeats once per
/// interval, not once per pass.
#[test]
fn a_guard_lock_that_cannot_be_opened_fails_the_attempt() {
    let w = World::new();
    no_leases(&w);
    w.guard.use_real_lock();
    std::fs::create_dir_all(w.home.join(".agentspace").join("guard.lock")).unwrap();
    let ran = w.run_guard(&[
        "-IntervalSeconds",
        "1",
        "-GuardIntervalSeconds",
        "5",
        "-MaxPasses",
        "7",
    ]);
    assert_eq!(ran.exit, 0);
    let raw = w.log_raw();
    let fails = guard_lines(&raw);
    assert_eq!(fails.len(), 2, "{raw}");
    let mut times = Vec::new();
    for line in &fails {
        let (time, why) = parse_guard_failed(line);
        let n = why
            .strip_prefix("guard.lock could not be opened: Windows error ")
            .and_then(|r| r.strip_suffix('.'))
            .unwrap_or_else(|| panic!("Not the lock's reason: {line:?}"));
        assert!(
            !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()),
            "The error is its number alone: {line:?}"
        );
        times.push(time);
    }
    assert_eq!(times, vec![at(0), at(5)]);
    assert_eq!(
        w.log(),
        (0..7).map(idle).collect::<Vec<_>>().concat(),
        "Every pass's lines as without the failure"
    );
    assert!(w.guard.starts().is_empty());
}

/// A lock that could not be opened, and for which the system gave no number, is still a failed
/// attempt and never a free lock; the number is only what the system gave.
#[test]
fn a_lock_that_cannot_be_opened_is_never_taken_for_a_free_one() {
    let w = World::new();
    no_leases(&w);
    w.guard
        .script_locks(vec![LockStep::Answer(GuardLockTry::Failed(Some(5)))]);
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![
            gf(0, "guard.lock could not be opened: Windows error 5."),
            idle(0),
        ],
    );
    assert!(w.guard.starts().is_empty());
}

/// G7. With no `<home>\.agentspace` folder (the state folder is elsewhere), a `guard started`
/// line, and the keeper creates neither the folder nor `guard.lock`: the try opens only a file
/// that exists.
#[test]
fn the_try_creates_neither_the_agentspace_folder_nor_the_lock() {
    let w = World::new();
    no_leases(&w);
    w.guard.use_real_lock();
    assert!(!w.home.join(".agentspace").exists());
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0)]);
    assert!(
        !w.home.join(".agentspace").exists(),
        "The keeper made the .agentspace folder"
    );
}

/// With the folder there and no lock in it, still no lock is made.
#[test]
fn the_try_makes_no_lock_in_an_existing_folder() {
    let w = World::new();
    no_leases(&w);
    w.guard.use_real_lock();
    let dir = w.home.join(".agentspace");
    std::fs::create_dir_all(&dir).unwrap();
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0)]);
    assert!(!dir.join("guard.lock").exists(), "A guard.lock was made");
}

/// A free lock is closed again, at once, and its bytes are as they were: it can be taken with no
/// sharing after the run, and holds what it held.
#[test]
fn a_free_lock_is_closed_and_left_as_it_was() {
    use std::os::windows::fs::OpenOptionsExt;
    let w = World::new();
    no_leases(&w);
    w.guard.use_real_lock();
    let dir = w.home.join(".agentspace");
    std::fs::create_dir_all(&dir).unwrap();
    let lock = dir.join("guard.lock");
    std::fs::write(&lock, b"").unwrap();
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0)]);
    let again = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&lock);
    assert!(again.is_ok(), "The keeper still holds guard.lock");
    drop(again);
    assert_eq!(std::fs::read(&lock).unwrap(), Vec::<u8>::new());
}

/// The lock is tried at each attempt, once, under the keeper's home, and never by a pass: a pass
/// is never an attempt of the start.
#[test]
fn the_lock_is_tried_once_per_attempt_under_the_keepers_home() {
    let w = World::new();
    no_leases(&w);
    let ran = w.run_guard(&["-IntervalSeconds", "300", "-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    let tries = w.guard.tries();
    assert_eq!(tries.len(), 2, "One try per attempt: {tries:?}");
    assert!(tries.iter().all(|h| same(h, &w.home)));
    assert_eq!(w.guard.starts().len(), 2);
}

/// `AGENTSPACE_STATE_DIR` does not move `guard.lock`: a keeper given another state folder tries
/// the lock under its home, as the guard it starts does, and the home it starts the guard in is
/// the same. A relative `USERPROFILE` is made full against the keeper's working folder, for the
/// try, the working folder and the variable alike.
#[test]
fn the_lock_is_under_the_home_and_a_relative_home_is_made_full() {
    let w = World::new();
    no_leases(&w);
    w.set_var("USERPROFILE", "home");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    let starts = w.guard.starts();
    assert_eq!(starts.len(), 1);
    assert!(same(&starts[0].cwd, &w.home), "The working folder is full");
    starts[0].assert_given_path("USERPROFILE", &w.home);
    assert!(same(&w.guard.tries()[0], &w.home));
}

// ---- What a failed start changes ----

/// G8. `AGENTSPACE_GUARD_CMD` naming no file, with a holder running through a ready lease: the
/// `failed` line once, the holder kept in every pass, no hold-off, no record changed, no start or
/// stop, the waits as without it, exit 0 at `-MaxPasses`.
#[test]
fn a_failed_start_changes_no_holder_no_pass_and_no_wait() {
    let w = World::new();
    let rec = w.adopt(100, ID_A, None);
    guard_cmd_is(&w, &w.home.join("no-such-guard.cmd"));
    let ran = w.run_guard(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![
            gf(0, MISSING),
            kept(0, ID_A, 100),
            kept(30, ID_A, 100),
            kept(60, ID_A, 100),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30]);
    assert!(w.starts().is_empty(), "No holder was started");
    assert!(w.stops().is_empty(), "No holder was stopped");
    assert_eq!(w.record(), Some(rec), "The record is as it was");
    assert_eq!(w.count("lock:pass"), 3);
    assert!(w.guard.starts().is_empty());
}

/// A failed start in the same pass as a started holder: the pass's lines are as without the
/// failure, and the backoff is the holder's alone.
#[test]
fn a_failed_start_does_not_change_the_lines_of_a_pass_that_starts_a_holder() {
    let w = World::new();
    w.plan_holder(100, None);
    guard_cmd_is(&w, &w.home.join("no-such-guard.cmd"));
    let ran = w.run_guard(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![gf(0, MISSING), started(0, ID_A, 100), kept(30, ID_A, 100)],
    );
}

/// A failed start counts as an attempt: it is not tried again for `-GuardIntervalSeconds`, and a
/// failure that lasts writes one line each interval, never one per pass.
#[test]
fn a_failure_that_lasts_writes_one_line_every_interval() {
    let w = World::new();
    no_leases(&w);
    guard_cmd_is(&w, &w.home.join("no-such-guard.cmd"));
    let ran = w.run_guard(&[
        "-IntervalSeconds",
        "1",
        "-GuardIntervalSeconds",
        "5",
        "-MaxPasses",
        "12",
    ]);
    assert_eq!(ran.exit, 0);
    let raw = w.log_raw();
    assert_eq!(
        guard_times(&raw),
        vec![at(0), at(5), at(10)],
        "One line per interval: {raw}"
    );
    for line in guard_lines(&raw) {
        assert_eq!(parse_guard_failed(&line).1, MISSING);
    }
    assert_eq!(w.count("lock:pass"), 12);
}

/// A line that cannot be logged is dropped, and the attempt stands: a guard that started still
/// runs, and the attempt still counts, so there is no retry before the interval.
#[test]
fn a_guard_line_that_cannot_be_logged_leaves_the_attempt_standing() {
    let w = World::new();
    no_leases(&w);
    // A folder where the log file goes: every write of a line fails.
    std::fs::create_dir_all(w.state.join("board-holder.log")).unwrap();
    let ran = w.run_guard(&[
        "-IntervalSeconds",
        "1",
        "-GuardIntervalSeconds",
        "3",
        "-MaxPasses",
        "5",
    ]);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        w.guard.starts().len(),
        2,
        "Two attempts, at 0 and at 3: a dropped line does not undo one"
    );
}

/// A detached start that fails fails the attempt with `The detached start failed: <error>`, the
/// error the start gave, in its own words, and the next attempt is an interval later.
#[test]
fn a_detached_start_that_fails_gives_its_own_error_in_the_line() {
    let w = World::new();
    no_leases(&w);
    let error = "Win32_Process.Create returned 9: the path was not found.";
    w.guard
        .script_starts(vec![StartStep::Answer(Err(error.to_string()))]);
    let ran = w.run_guard(&[
        "-IntervalSeconds",
        "1",
        "-GuardIntervalSeconds",
        "5",
        "-MaxPasses",
        "6",
    ]);
    assert_eq!(ran.exit, 0);
    let shaped = shape(&w.log_raw());
    assert_eq!(
        shaped[0],
        gf(0, &format!("The detached start failed: {error}"))
    );
    assert_eq!(
        guard_times(&w.log_raw()),
        vec![at(0), at(5)],
        "The next attempt is an interval later"
    );
    assert_eq!(w.guard.starts().len(), 2);
    assert_eq!(
        guard_lines(&w.log_raw())[1],
        guard_started_line(&at(5), FIRST_PID),
        "The second attempt started"
    );
}

/// A call that reached its bound is the detached start's own error:
/// `The call did not return within 10 seconds.`
#[test]
fn a_call_that_reached_its_bound_is_a_failed_start_with_the_bounds_text() {
    let w = World::new();
    no_leases(&w);
    w.guard.script_starts(vec![StartStep::Answer(Err(
        "The call did not return within 10 seconds.".to_string(),
    ))]);
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![
            gf(
                0,
                "The detached start failed: The call did not return within 10 seconds.",
            ),
            idle(0),
        ],
    );
}

/// An error that escapes the attempt, a panic included, is `The start stopped on an error:
/// <error>`, counts as an attempt, and ends nothing.
#[test]
fn a_panic_in_the_start_is_a_failed_attempt_and_the_loop_goes_on() {
    let w = World::new();
    no_leases(&w);
    w.guard
        .script_starts(vec![StartStep::Panic("The fake start broke.".to_string())]);
    let ran = w.run_guard(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![
            gf(0, "The start stopped on an error: The fake start broke."),
            idle(0),
            idle(30),
        ],
    );
    assert_eq!(w.guard.starts().len(), 1, "An attempt, not retried");
}

/// A clock that cannot be read just before a pass is an error that escaped the start: it is logged
/// as the start's, and it counts as an attempt that moves the guard's last start, so the next is
/// not due at the next pass but `-GuardIntervalSeconds` after the one that was due (keeper.md,
/// What a failed start changes). The start that fell due at 60 fails on the clock, and the pass at
/// 90 starts nothing.
#[test]
fn a_clock_that_panics_before_a_pass_counts_as_an_attempt_that_delays_the_next() {
    let w = World::new();
    no_leases(&w);
    // The second wait of 30 seconds ends at 60, and the next read of the clock is the one just
    // before the pass.
    w.on("sleep:30", 2, |w| {
        w.panic_on_next_monotonic_read("The fake monotonic clock broke.")
    });
    let ran = w.run_guard(&["-GuardIntervalSeconds", "60", "-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![
            g(0),
            idle(0),
            idle(30),
            gf(
                60,
                "The start stopped on an error: The fake monotonic clock broke.",
            ),
            idle(60),
            idle(90),
        ],
    );
    assert_eq!(
        w.guard.starts().len(),
        1,
        "The failed attempt started nothing"
    );
}

/// The same for the try of the lock.
#[test]
fn a_panic_in_the_lock_try_is_a_failed_attempt_and_the_loop_goes_on() {
    let w = World::new();
    no_leases(&w);
    w.guard
        .script_locks(vec![LockStep::Panic("The fake try broke.".to_string())]);
    let ran = w.run_guard(&["-MaxPasses", "2"]);
    assert_eq!(ran.exit, 0);
    assert_shape(
        &w,
        &ran,
        vec![
            gf(0, "The start stopped on an error: The fake try broke."),
            idle(0),
            idle(30),
        ],
    );
    assert!(w.guard.starts().is_empty());
}

// ---- The program ----

/// An unset `AGENTSPACE_GUARD_CMD`, and no `agentspace-operator.exe` in the folder of the keeper's
/// own program: the table's reason, and nothing started.
#[test]
fn with_no_guard_program_and_no_operator_beside_the_keeper_the_start_fails() {
    let w = World::new();
    no_leases(&w);
    w.unset_var("AGENTSPACE_GUARD_CMD");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![gf(0, NOT_INSTALLED), idle(0)]);
    assert!(w.guard.starts().is_empty());
}

/// An empty `AGENTSPACE_GUARD_CMD` is an unset one: it names no program, and the operator beside
/// the keeper is the program, which is not there.
#[test]
fn an_empty_guard_program_variable_is_an_unset_one() {
    let w = World::new();
    no_leases(&w);
    w.set_var("AGENTSPACE_GUARD_CMD", "");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![gf(0, NOT_INSTALLED), idle(0)]);
    assert!(w.guard.starts().is_empty());
}

/// With `AGENTSPACE_GUARD_CMD` unset, the program is `agentspace-operator.exe` in the folder the
/// keeper's own executable runs from, with exactly the one argument `guard`, in the home folder.
#[test]
fn with_no_guard_program_the_operator_beside_the_keeper_is_started_with_the_one_argument() {
    let w = World::new();
    no_leases(&w);
    w.unset_var("AGENTSPACE_GUARD_CMD");
    let operator = operator_beside(&w);
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0)]);
    let starts = w.guard.starts();
    assert_eq!(starts.len(), 1);
    assert!(same(&starts[0].program, &operator), "The program");
    assert_eq!(starts[0].args, vec!["guard".to_string()]);
    assert!(same(&starts[0].cwd, &w.home), "The working folder");
}

/// `AGENTSPACE_GUARD_CMD`, when set and not empty, is the program in place of the operator beside
/// the keeper, and an executable gets exactly the one argument `guard`, never `-Quiet` or another.
#[test]
fn a_named_guard_program_is_used_in_place_of_the_operator_beside_the_keeper() {
    let w = World::new();
    no_leases(&w);
    operator_beside(&w);
    let program = stub_program(&w.home.join("a-guard.exe"));
    guard_cmd_is(&w, &program);
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0)]);
    let starts = w.guard.starts();
    assert!(same(&starts[0].program, &program));
    assert_eq!(starts[0].args, vec!["guard".to_string()]);
}

/// The program is looked for at every attempt, so an operator installed after a failed attempt is
/// used at the next one, with no restart: the first attempt fails, the operator appears during the
/// first pass, the second attempt starts it.
#[test]
fn an_operator_installed_after_a_failed_attempt_is_used_at_the_next_one() {
    let w = World::new();
    no_leases(&w);
    w.unset_var("AGENTSPACE_GUARD_CMD");
    let operator = w.own_exe.with_file_name("agentspace-operator.exe");
    let to = operator.clone();
    w.on("lock:pass", 1, move |_| {
        stub_program(&to);
    });
    let ran = w.run_guard(&[
        "-IntervalSeconds",
        "5",
        "-GuardIntervalSeconds",
        "5",
        "-MaxPasses",
        "2",
    ]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![gf(0, NOT_INSTALLED), idle(0), g(5), idle(5)]);
    assert!(same(&w.guard.starts()[0].program, &operator));
}

/// A relative `AGENTSPACE_GUARD_CMD` is made full against the keeper's working folder (the
/// throwaway folder that holds the home), not the home, and the guard is still started in the
/// home folder.
#[test]
fn a_relative_guard_program_is_made_full_against_the_keepers_working_folder() {
    let w = World::new();
    no_leases(&w);
    let program = stub_program(&w.root().join("rel").join("guard.exe"));
    w.set_var("AGENTSPACE_GUARD_CMD", r"rel\guard.exe");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0)]);
    let starts = w.guard.starts();
    assert!(same(&starts[0].program, &program), "The program is full");
    assert!(same(&starts[0].cwd, &w.home), "The working folder");
    // The keeper's own variable is passed as it has it, never made full.
    starts[0].assert_given(
        "AGENTSPACE_GUARD_CMD",
        OsString::from(r"rel\guard.exe").as_os_str(),
    );
}

/// A path that names a folder is not a file, so it is a program that cannot be found.
#[test]
fn a_guard_program_that_is_a_folder_is_a_program_that_cannot_be_found() {
    let w = World::new();
    no_leases(&w);
    let folder = w.home.join("a-folder.cmd");
    std::fs::create_dir_all(&folder).unwrap();
    guard_cmd_is(&w, &folder);
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![gf(0, MISSING), idle(0)]);
    assert!(w.guard.starts().is_empty());
}

/// A `.cmd` or `.bat` program, its extension compared without regard to case, is run through
/// Windows' command interpreter: `<system folder>\cmd.exe /d /c <program> guard`, the program one
/// argument whatever its path holds (a space included), and `guard` its last and only argument.
#[test]
fn a_batch_guard_is_run_through_the_command_interpreter_with_the_one_argument_after_it() {
    for name in [
        "guard-standin.cmd",
        "guard-standin.CMD",
        "guard-standin.bat",
        "guard standin.Bat",
    ] {
        let w = World::new();
        no_leases(&w);
        let program = stub_program(&w.home.join("a folder").join(name));
        guard_cmd_is(&w, &program);
        let ran = w.run_guard(&["-MaxPasses", "1"]);
        assert_eq!(ran.exit, 0, "{name}");
        assert_shape(&w, &ran, vec![g(0), idle(0)]);
        let starts = w.guard.starts();
        assert_eq!(starts.len(), 1, "{name}");
        assert!(
            same(&starts[0].program, &cmd_exe(&w)),
            "The program of {name} is the command interpreter"
        );
        assert_eq!(starts[0].args.len(), 4, "{name}");
        assert_eq!(&starts[0].args[..2], ["/d", "/c"], "{name}");
        assert!(
            same(Path::new(&starts[0].args[2]), &program),
            "The batch is the third argument of {name}"
        );
        assert_eq!(starts[0].args[3], "guard", "{name}");
    }
}

/// With `SystemRoot` unset or empty a batch is a program that cannot be found, and nothing is
/// started.
#[test]
fn a_batch_guard_with_no_system_root_cannot_be_found() {
    for (name, system_root) in [
        ("guard-standin.cmd", None),
        ("guard-standin.cmd", Some("")),
        ("guard-standin.BAT", None),
        ("guard-standin.Bat", Some("")),
    ] {
        let w = World::new();
        no_leases(&w);
        let program = stub_program(&w.home.join(name));
        guard_cmd_is(&w, &program);
        match system_root {
            None => w.unset_var("SystemRoot"),
            Some(v) => w.set_var("SystemRoot", v),
        }
        let ran = w.run_guard(&["-MaxPasses", "1"]);
        assert_eq!(ran.exit, 0, "{name}");
        assert_shape(&w, &ran, vec![gf(0, NO_SYSTEM_ROOT), idle(0)]);
        assert!(w.guard.starts().is_empty(), "{name}");
    }
}

/// A program that is not a batch needs no command interpreter, so an unset `SystemRoot` is nothing
/// to it.
#[test]
fn an_executable_guard_does_not_need_the_system_root() {
    let w = World::new();
    no_leases(&w);
    let program = stub_program(&w.home.join("guard-program.exe"));
    guard_cmd_is(&w, &program);
    w.unset_var("SystemRoot");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), idle(0)]);
    assert!(same(&w.guard.starts()[0].program, &program));
}

// ---- The guard's environment ----

/// G10, on the fake clock. A keeper given `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` as
/// relative paths, `AGENTSPACE_PROXY_COMMAND`, `AGENTSPACE_AWS_CMD` and variables of its own: the
/// guard is given `USERPROFILE` as the home, the two folders full (against the keeper's working
/// folder), the proxy command and every `AGENTSPACE_*_CMD`, a lower-case name included, exactly as
/// given, and none of the variables that are not one of those. A mismatch is reported by name.
#[test]
fn the_guard_is_given_the_keepers_home_folders_proxy_and_seams_and_nothing_else() {
    let w = World::new();
    no_leases(&w);
    let proxy = r"C:\fake\proxy.cmd -W %h:%p";
    w.set_var("AGENTSPACE_STATE_DIR", "rel-state");
    w.set_var("AGENTSPACE_SSH_DIR", "rel-ssh");
    w.set_var("AGENTSPACE_PROXY_COMMAND", proxy);
    w.set_var("AGENTSPACE_AWS_CMD", "aws-relative.cmd");
    w.set_var("agentspace_mixed_cMd", "mixed-value");
    w.set_var("AGENTSPACE_EMPTY_CMD", "");
    w.set_var("AGENTSPACE__CMD", "no-name-between");
    w.set_var("AGENTSPACE_OWN_VARIABLE", "own");
    w.set_var("AGENTSPACE_TIMEZONE", "UTC");
    w.set_var("AGENTSPACE_DIAGNOSTIC", "1");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    let starts = w.guard.starts();
    assert_eq!(starts.len(), 1);
    let s = &starts[0];
    s.assert_given_path("USERPROFILE", &w.home);
    s.assert_given_path("AGENTSPACE_STATE_DIR", &w.root().join("rel-state"));
    s.assert_given_path("AGENTSPACE_SSH_DIR", &w.root().join("rel-ssh"));
    s.assert_given(
        "AGENTSPACE_PROXY_COMMAND",
        OsString::from(proxy).as_os_str(),
    );
    s.assert_given(
        "AGENTSPACE_AWS_CMD",
        OsString::from("aws-relative.cmd").as_os_str(),
    );
    s.assert_given(
        "agentspace_mixed_cMd",
        OsString::from("mixed-value").as_os_str(),
    );
    let trip = w.home.join(common::TRIPWIRE);
    s.assert_given("AGENTSPACE_SSH_CMD", trip.as_os_str());
    s.assert_given("AGENTSPACE_SCP_CMD", trip.as_os_str());
    s.assert_given(
        "AGENTSPACE_GUARD_CMD",
        w.home.join(common::STANDIN).as_os_str(),
    );
    for name in [
        "AGENTSPACE_EMPTY_CMD",
        "AGENTSPACE__CMD",
        "AGENTSPACE_OWN_VARIABLE",
        "AGENTSPACE_TIMEZONE",
        "AGENTSPACE_DIAGNOSTIC",
    ] {
        s.assert_not_given(name);
    }
}

/// A value the logon environment holds and the keeper does not have set, and not empty, is
/// removed, so the guard never takes a folder, a ProxyCommand line or a stand-in that the keeper
/// did not use: the start names each with an empty value.
#[test]
fn what_the_keeper_does_not_have_is_removed_from_the_guards_environment() {
    let w = World::new();
    no_leases(&w);
    for name in [
        "AGENTSPACE_AWS_CMD",
        "AGENTSPACE_SSH_CMD",
        "AGENTSPACE_SCP_CMD",
    ] {
        w.unset_var(name);
    }
    w.set_var("AGENTSPACE_STATE_DIR", "");
    w.set_var("AGENTSPACE_SSH_DIR", "");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    let starts = w.guard.starts();
    assert_eq!(starts.len(), 1);
    for name in [
        "AGENTSPACE_STATE_DIR",
        "AGENTSPACE_SSH_DIR",
        "AGENTSPACE_PROXY_COMMAND",
        "AGENTSPACE_AWS_CMD",
        "AGENTSPACE_SSH_CMD",
        "AGENTSPACE_SCP_CMD",
    ] {
        starts[0].assert_removed(name);
    }
}

/// The keeper's own variables are named to the start's seam, never their values: the seam is told
/// the names of the world's variables.
#[test]
fn the_start_seam_is_told_the_names_of_the_keepers_variables() {
    use agentspace_keeper::GuardStarter;
    let w = World::new();
    no_leases(&w);
    w.set_var("AGENTSPACE_SOMETHING_CMD", "x");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    let names = w.guard.variable_names();
    assert!(
        names
            .iter()
            .any(|n| n.to_string_lossy() == "AGENTSPACE_SOMETHING_CMD"),
        "The keeper's seam names are not given"
    );
    w.guard.starts()[0].assert_given("AGENTSPACE_SOMETHING_CMD", OsString::from("x").as_os_str());
}

// ---- Where the start is not ----

/// The start never comes inside a pass: it never runs while the keeper holds `board-holder.lock`,
/// so no guard line is written between a pass's lock and its release, however often a start falls
/// due.
#[test]
fn the_start_is_never_inside_a_pass() {
    let w = World::new();
    no_leases(&w);
    let seen: Rc<RefCell<Vec<(&'static str, usize)>>> = Rc::new(RefCell::new(Vec::new()));
    for nth in 1..=4 {
        for (event, label) in [("lock:pass", "lock"), ("release:pass", "release")] {
            let seen = seen.clone();
            w.on(event, nth, move |w| {
                seen.borrow_mut()
                    .push((label, guard_lines(&w.log_raw()).len()));
            });
        }
    }
    let ran = w.run_guard(&[
        "-IntervalSeconds",
        "1",
        "-GuardIntervalSeconds",
        "1",
        "-MaxPasses",
        "4",
    ]);
    assert_eq!(ran.exit, 0);
    let seen = seen.borrow().clone();
    assert_eq!(seen.len(), 8, "{seen:?}");
    for pair in seen.chunks(2) {
        assert_eq!(pair[0].0, "lock");
        assert_eq!(pair[1].0, "release");
        assert_eq!(
            pair[0].1, pair[1].1,
            "A guard line was written inside a pass: {seen:?}"
        );
    }
    assert!(
        guard_lines(&w.log_raw()).len() >= 4,
        "A start was due each time: {}",
        w.log_raw()
    );
}

/// G13, on the fake clock. A pass that logs `skipped` (it could not take `board-holder.lock`)
/// changes nothing for the guard: the start was made all the same, before the pass.
#[test]
fn a_skipped_pass_still_has_its_start() {
    let w = World::new();
    no_leases(&w);
    w.pass_modes.borrow_mut().push_back(PassMode::Skipped);
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_shape(&w, &ran, vec![g(0), skipped(0)]);
    assert_eq!(w.guard.starts().len(), 1);
}

/// G16, on the fake clock. A keeper that finds another keeper's lock held logs its one line,
/// exits 0, and starts no guard: no line, no try, no start.
#[test]
fn a_keeper_that_finds_another_running_starts_no_guard() {
    let w = World::new();
    no_leases(&w);
    *w.keeper_mode.borrow_mut() = KeeperMode::Busy(Some(4242));
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(ran.stdout, another_keeper(0, Some(4242)));
    assert_eq!(w.log_raw(), another_keeper(0, Some(4242)));
    assert_eq!(w.count("lock:pass"), 0, "No pass");
    assert!(w.guard.tries().is_empty(), "No try of guard.lock");
    assert!(w.guard.starts().is_empty());
}

/// A keeper that cannot take its lock for another reason exits 1 and starts no guard either.
#[test]
fn a_keeper_that_cannot_take_its_lock_starts_no_guard() {
    let w = World::new();
    no_leases(&w);
    *w.keeper_mode.borrow_mut() = KeeperMode::Failed("boom".to_string());
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 1);
    assert_eq!(
        w.log_raw(),
        failed(0, "The keeper lock could not be taken: boom")
    );
    assert!(w.guard.tries().is_empty());
    assert!(w.guard.starts().is_empty());
}

/// A keeper that ends before its loop, for no home folder, starts no guard.
#[test]
fn a_keeper_with_no_home_folder_starts_no_guard() {
    let w = World::new();
    no_leases(&w);
    w.unset_var("USERPROFILE");
    let ran = w.run_guard(&["-MaxPasses", "1"]);
    assert_eq!(ran.exit, 1);
    assert!(w.guard.tries().is_empty());
    assert!(w.guard.starts().is_empty());
}

/// G12, in process. A one-pass run never tries `guard.lock`, never starts a guard and logs no line
/// for one, whatever `AGENTSPACE_GUARD_CMD` holds; and no pass of a keeper, which is a `-Watch`
/// run's, is a pass of a one-pass run.
#[test]
fn a_one_pass_run_never_tries_the_lock_or_starts_a_guard() {
    let h = common::Harness::new();
    let ran = h.pass(common::NOW);
    assert_eq!(ran.exit, 0, "{}", ran.stderr);
    assert!(
        guard_lines(&ran.stdout).is_empty(),
        "A one-pass run logged a guard line: {}",
        ran.stdout
    );
    assert!(
        h.fakes.guard.tries().is_empty(),
        "A one-pass run tried guard.lock"
    );
    assert!(
        h.fakes.guard.starts().is_empty(),
        "A one-pass run started a guard"
    );
    assert!(
        guard_lines(&h.log()).is_empty(),
        "A guard line is in the log"
    );
}

/// Passes of a keeper that is running and is not its first iteration never tried the lock before
/// the start was due: `-MaxPasses` of passes a second apart try it once.
#[test]
fn a_pass_is_not_an_attempt() {
    let w = World::new();
    no_leases(&w);
    let ran = w.run_guard(&["-IntervalSeconds", "1", "-MaxPasses", "6"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        w.guard.tries().len(),
        1,
        "The lock is tried once, at the attempt"
    );
    assert_eq!(w.guard.starts().len(), 1);
}

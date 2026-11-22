//! The keeper's start of the guard, as the built program does it (docs/keeper.md, Starting the
//! guard; Conformance cases for the guard's start, G1 to G16; E0.08-T03-S03, #0179).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every test runs the built
//! program as a separate process, with the real clock, in a throwaway home and a throwaway state
//! folder, through `Bin` of the shared harness, and **with `AGENTSPACE_GUARD_CMD` naming a
//! stand-in guard** in that home and tripwires for the AWS, ssh and scp seams (AGENTS.md: unset,
//! the program's guard would be the build's real operator). The stand-in is a `.cmd` that appends
//! one block per run to a file of its own: its arguments, its working folder and the variables
//! the case names. A case reports a mismatch by a variable's name, never by its value. No test
//! runs a real guard, and none touches `guard.log`.
//!
//! The tests whose subject is the keeper's clock (a start at 299 and at 300 seconds, a start that
//! falls due in a wait) run against the fake clock, in `guard_clock.rs`; the ones here read time
//! off the real one, with `-IntervalSeconds 1` and `-GuardIntervalSeconds` of a few seconds.

mod common;

use common::*;
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

const IDLE_TAIL: &str = " idle lease=- alias=- pid=-\n";

/// `-Watch -SshExe <the stub>` and `extra`.
fn keeper(b: &Bin, extra: &[&str]) -> BinRan {
    let ssh = b.ssh.to_string_lossy().into_owned();
    let mut args = vec!["-Watch", "-SshExe", ssh.as_str()];
    args.extend_from_slice(extra);
    b.run(&args)
}

fn lines_of(text: &str) -> Vec<String> {
    text.split_inclusive('\n').map(str::to_string).collect()
}

fn idle_count(text: &str) -> usize {
    text.split_inclusive('\n')
        .filter(|l| l.ends_with(IDLE_TAIL))
        .count()
}

fn log_of(b: &Bin) -> String {
    std::fs::read_to_string(b.state.join("board-holder.log")).expect("The log was written")
}

fn agentspace_dir(b: &Bin) -> std::path::PathBuf {
    b.home.join(".agentspace")
}

fn lock_path(b: &Bin) -> std::path::PathBuf {
    agentspace_dir(b).join("guard.lock")
}

/// Opens a file for reading and writing, created if missing, sharing nothing: a handle that
/// refuses every other open, as a guard's hold of `guard.lock` does (state.md, The guard's lock).
fn hold_sharing_nothing(path: &Path) -> std::fs::File {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(path)
        .expect("The test holds the lock")
}

fn same_path(a: &str, b: &Path) -> bool {
    a.eq_ignore_ascii_case(&b.to_string_lossy())
}

fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// ---- G1: the first iteration ----

/// G1. The first iteration of a keeper starts the guard before its first pass: one `guard started`
/// line with no `host=`, before the pass's `idle` line; the stand-in ran once, with exactly the
/// one argument `guard`, in the working folder `<home>`; exit 0. The line is also on stdout.
#[test]
fn the_first_iteration_starts_the_guard_before_the_first_pass() {
    let b = Bin::new();
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    assert_eq!(r.stderr, "");
    let lines = lines_of(&r.stdout);
    assert_eq!(lines.len(), 2, "A guard line and the pass's: {lines:?}");
    let time = guard_started_time(&lines[0]);
    assert!(
        lines[1].ends_with(IDLE_TAIL),
        "The second line is the pass's: {:?}",
        lines[1]
    );
    assert!(
        time.as_str() <= &lines[1][..20],
        "The guard's start is not before the pass's time"
    );
    assert!(!lines[0].contains("host="), "A guard line has no host=");
    assert_eq!(log_of(&b), r.stdout, "stdout is the log's lines");

    let runs = wait_for_runs(&b.standin_dir(), 1);
    assert_eq!(
        runs[0].bracket("arg1").as_deref(),
        Some("guard"),
        "The first argument"
    );
    assert_eq!(
        runs[0].bracket("arg2").as_deref(),
        Some(""),
        "There is exactly the one argument"
    );
    let cwd = runs[0]
        .bracket("cwd")
        .expect("The stand-in recorded its folder");
    assert!(
        same_path(&cwd, &b.home),
        "The guard's working folder is the home folder"
    );
    settle_runs(&b.standin_dir(), 1);
    assert!(!tripwire_hit(&b.standin_dir()), "A tripwire ran");
}

/// `guard started pid=<pid>` names the process the detached start returned, in decimal digits
/// (keeper.md, The keeper's lines for it). The process is a `.cmd` stand-in, so it is the command
/// interpreter that runs it: the test checks that the process of that ID runs the stand-in, and
/// ends it.
#[test]
fn the_started_line_names_the_process_by_its_decimal_pid() {
    let b = Bin::new();
    let staying = StayingGuard::new(&b.standin_dir());
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    wait_for_runs(&b.standin_dir(), 1);
    let (_, pid) = parse_guard_started(&guard_lines(&r.stdout)[0]);
    let process = StandInProcess::new(pid, STANDIN);
    staying.end();
    process.end();
    assert!(!process_exists(pid), "The test could not end the stand-in");
}

// ---- G2, G3: the spacing ----

/// G2. With `-GuardIntervalSeconds` not given, five passes a second apart start one guard: at most
/// one start every 300 seconds.
#[test]
fn a_keeper_starts_one_guard_in_five_passes_a_second_apart() {
    let b = Bin::new();
    let r = keeper(&b, &["-IntervalSeconds", "1", "-MaxPasses", "5"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    assert_eq!(guard_lines(&r.stdout).len(), 1, "{}", r.stdout);
    assert_eq!(idle_count(&r.stdout), 5);
    wait_for_runs(&b.standin_dir(), 1);
    settle_runs(&b.standin_dir(), 1);
}

/// G3. With `-GuardIntervalSeconds 3` over about ten seconds, two or more starts, each one's time
/// at least 3 seconds after the one before's, and one run of the stand-in for each.
#[test]
fn a_short_guard_interval_starts_the_guard_again_no_sooner_than_it_says() {
    let b = Bin::new();
    let r = keeper(
        &b,
        &[
            "-IntervalSeconds",
            "1",
            "-GuardIntervalSeconds",
            "3",
            "-MaxPasses",
            "11",
        ],
    );
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    let starts = guard_lines(&r.stdout);
    assert!(starts.len() >= 2, "Two or more starts: {}", r.stdout);
    let times: Vec<i64> = starts
        .iter()
        .map(|l| secs_of(&guard_started_time(l)))
        .collect();
    for pair in times.windows(2) {
        assert!(
            pair[1] - pair[0] >= 3,
            "Two starts closer than 3 seconds: {times:?}"
        );
    }
    assert_eq!(idle_count(&r.stdout), 11, "Every pass logged its line");
    wait_for_runs(&b.standin_dir(), starts.len());
    settle_runs(&b.standin_dir(), starts.len());
}

// ---- G5, G6: guard.lock ----

/// G5. While `guard.lock` is held by another handle that shares nothing, no start and no line, and
/// the passes go on; once it is closed, a `guard started` line no later than just before the first
/// pass that starts after the close, without waiting `-GuardIntervalSeconds`.
#[test]
fn a_held_guard_lock_delays_the_start_until_it_is_free() {
    let b = Bin::new();
    let lock = hold_sharing_nothing(&lock_path(&b));
    // The lock is closed once the keeper's log shows two passes, so that the passes are seen to go
    // on while it is held, however slowly the program starts.
    let log = b.state.join("board-holder.log");
    let closed = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            let seen = std::fs::read_to_string(&log).unwrap_or_default();
            if idle_count(&seen) >= 2 || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let at = unix_seconds();
        drop(lock);
        at
    });
    let r = keeper(&b, &["-IntervalSeconds", "1", "-MaxPasses", "6"]);
    let closed_at = closed.join().unwrap();
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    let lines = lines_of(&r.stdout);
    let first = lines
        .iter()
        .position(|l| is_guard_line(l))
        .unwrap_or_else(|| panic!("No guard line after the lock was closed: {}", r.stdout));
    assert!(
        first >= 2,
        "A guard line came while the lock was held: {}",
        r.stdout
    );
    let time = guard_started_time(&lines[first]);
    let at = secs_of(&time);
    assert!(
        at >= closed_at,
        "The start is before the lock was closed ({at} against {closed_at})"
    );
    assert!(
        at <= closed_at + 3,
        "The start waited for more than the next pass ({at} against {closed_at})"
    );
    assert_eq!(
        guard_lines(&r.stdout).len(),
        1,
        "One start, never one per pass"
    );
    assert_eq!(idle_count(&r.stdout), 6, "The passes went on");
    wait_for_runs(&b.standin_dir(), 1);
    settle_runs(&b.standin_dir(), 1);
}

/// G6. A folder in place of `guard.lock` is not a free lock: `failed ... The guard could not be
/// started: guard.lock could not be opened: Windows error <n>.`, no run of the stand-in, the next
/// such line no sooner than 5 seconds later (`-GuardIntervalSeconds 5`), and every pass's lines as
/// without it.
#[test]
fn a_lock_that_cannot_be_opened_fails_the_attempt_once_per_interval() {
    let b = Bin::new();
    std::fs::create_dir_all(lock_path(&b)).unwrap();
    let r = keeper(
        &b,
        &[
            "-IntervalSeconds",
            "1",
            "-GuardIntervalSeconds",
            "5",
            "-MaxPasses",
            "7",
        ],
    );
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    let fails = guard_lines(&r.stdout);
    assert_eq!(
        fails.len(),
        2,
        "One line per interval, not per pass: {}",
        r.stdout
    );
    let times: Vec<i64> = fails
        .iter()
        .map(|l| {
            let (time, why) = parse_guard_failed(l);
            let n = why
                .strip_prefix("guard.lock could not be opened: Windows error ")
                .and_then(|r| r.strip_suffix('.'))
                .unwrap_or_else(|| panic!("The reason is not the lock's: {l:?}"));
            assert!(
                !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()),
                "The error is its number alone: {l:?}"
            );
            secs_of(&time)
        })
        .collect();
    assert!(times[1] - times[0] >= 5, "Closer than 5 seconds: {times:?}");
    assert_eq!(
        idle_count(&r.stdout),
        7,
        "Every pass's line is as without it"
    );
    assert_eq!(
        lines_of(&r.stdout)
            .iter()
            .filter(|l| !is_guard_line(l) && !l.ends_with(IDLE_TAIL))
            .count(),
        0,
        "No other line"
    );
    settle_runs(&b.standin_dir(), 0);
}

// ---- G7: what the keeper never makes ----

/// G7. With no `<home>\.agentspace` folder (the state folder is elsewhere), a `guard started`
/// line, and the keeper creates neither the folder nor `guard.lock`.
#[test]
fn the_keeper_creates_neither_the_agentspace_folder_nor_the_lock() {
    let b = Bin::new();
    assert!(!agentspace_dir(&b).exists());
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    guard_started_time(&guard_lines(&r.stdout)[0]);
    wait_for_runs(&b.standin_dir(), 1);
    settle_runs(&b.standin_dir(), 1);
    assert!(
        !agentspace_dir(&b).exists(),
        "The keeper made the .agentspace folder"
    );
}

/// The same with the folder there and no lock in it: the keeper makes no `guard.lock`.
#[test]
fn the_keeper_never_creates_the_lock_in_an_existing_folder() {
    let b = Bin::new();
    std::fs::create_dir_all(agentspace_dir(&b)).unwrap();
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    assert_eq!(guard_lines(&r.stdout).len(), 1);
    let runs = wait_for_runs(&b.standin_dir(), 1);
    settle_runs(&b.standin_dir(), 1);
    assert!(!lock_path(&b).exists(), "A guard.lock was made");
    assert_eq!(runs[0].get("lockprobe").as_deref(), Some("none"));
}

/// A free `guard.lock` is closed before the guard starts, so the guard can take it
/// (keeper.md, guard.lock: tried, never held): the stand-in opens it for append and succeeds. The
/// keeper never writes it either.
#[test]
fn a_free_lock_is_closed_before_the_guard_starts_and_never_written() {
    let b = Bin::new();
    std::fs::create_dir_all(agentspace_dir(&b)).unwrap();
    std::fs::write(lock_path(&b), b"").unwrap();
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    assert_eq!(guard_lines(&r.stdout).len(), 1, "{}", r.stdout);
    let runs = wait_for_runs(&b.standin_dir(), 1);
    assert_eq!(
        runs[0].get("lockprobe").as_deref(),
        Some("ok"),
        "The keeper still held guard.lock when the guard started"
    );
    assert_eq!(
        std::fs::read(lock_path(&b)).unwrap(),
        Vec::<u8>::new(),
        "The keeper wrote guard.lock"
    );
}

// ---- G8, G9: the program ----

/// G8, from outside: `AGENTSPACE_GUARD_CMD` naming no file fails the attempt with the table's
/// reason, once per interval, and every pass goes on as without it, exit 0 at `-MaxPasses`.
#[test]
fn a_guard_program_that_does_not_exist_fails_the_attempt_and_the_keeper_goes_on() {
    let mut b = Bin::new();
    let missing = b.full(&["home", "no-such-guard.cmd"]);
    b.set("AGENTSPACE_GUARD_CMD", &missing);
    let r = keeper(&b, &["-IntervalSeconds", "1", "-MaxPasses", "3"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    let fails = guard_lines(&r.stdout);
    assert_eq!(fails.len(), 1, "One attempt: {}", r.stdout);
    let (_, why) = parse_guard_failed(&fails[0]);
    assert_eq!(
        why,
        "The program that AGENTSPACE_GUARD_CMD names does not exist."
    );
    assert!(
        !fails[0].contains(&*b.home.to_string_lossy()),
        "The line names a path"
    );
    assert_eq!(idle_count(&r.stdout), 3, "Every pass logged its line");
    assert_eq!(lines_of(&r.stdout).len(), 4, "No other line: {}", r.stdout);
}

/// A directory is not a file: `AGENTSPACE_GUARD_CMD` naming one is a program that cannot be found.
#[test]
fn a_guard_program_that_is_a_folder_does_not_exist_either() {
    let mut b = Bin::new();
    let folder = b.full(&["home", "a-folder.cmd"]);
    std::fs::create_dir_all(&folder).unwrap();
    b.set("AGENTSPACE_GUARD_CMD", &folder);
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    let (_, why) = parse_guard_failed(&guard_lines(&r.stdout)[0]);
    assert_eq!(
        why,
        "The program that AGENTSPACE_GUARD_CMD names does not exist."
    );
}

/// G9. A keeper run in a working folder that holds the stand-in, with `AGENTSPACE_GUARD_CMD` the
/// stand-in's relative path: the stand-in runs, so the value was made full against the keeper's
/// working folder (`Bin` runs in `<home>`). Its working folder is still the home folder.
#[test]
fn a_relative_guard_program_is_made_full_against_the_keepers_working_folder() {
    let mut b = Bin::new();
    let folder = b.home.join("rel");
    write_standin(&folder, "guard-rel.cmd");
    b.set("AGENTSPACE_GUARD_CMD", r"rel\guard-rel.cmd");
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    guard_started_time(&guard_lines(&r.stdout)[0]);
    let runs = wait_for_runs(&folder, 1);
    assert_eq!(runs[0].bracket("arg1").as_deref(), Some("guard"));
    let cwd = runs[0]
        .bracket("cwd")
        .expect("The stand-in recorded its folder");
    assert!(same_path(&cwd, &b.home), "The guard's folder is the home");
    // It is the keeper's own variable, passed as given.
    runs[0].assert_var("AGENTSPACE_GUARD_CMD", r"rel\guard-rel.cmd");
}

/// G14. A stand-in `.cmd` whose path holds a space still runs, and receives exactly the one
/// argument `guard`.
#[test]
fn a_batch_guard_whose_path_holds_a_space_runs_and_receives_the_one_argument() {
    let mut b = Bin::new();
    let folder = b.home.join("a folder");
    let program = write_standin(&folder, "guard standin.cmd");
    b.set("AGENTSPACE_GUARD_CMD", &program);
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    guard_started_time(&guard_lines(&r.stdout)[0]);
    let runs = wait_for_runs(&folder, 1);
    assert_eq!(runs[0].bracket("arg1").as_deref(), Some("guard"));
    assert_eq!(runs[0].bracket("arg2").as_deref(), Some(""));
    settle_runs(&folder, 1);
}

// ---- G10: the guard's environment ----

/// G10. A keeper given `AGENTSPACE_STATE_DIR` as a relative path, `AGENTSPACE_SSH_DIR`,
/// `AGENTSPACE_PROXY_COMMAND`, `AGENTSPACE_AWS_CMD`, and variables of the case's own: the stand-in
/// sees `USERPROFILE` as `<home>`, the state and ssh folders full, the proxy command and every
/// `AGENTSPACE_*_CMD` exactly as given (a lower-case or mixed-case name included), and none of the
/// variables that are not one of those. A mismatch is reported by the variable's name.
#[test]
fn the_guard_gets_the_keepers_home_folders_proxy_and_seams_and_nothing_else() {
    let mut b = Bin::new();
    let proxy = r"C:\fake\proxy.cmd -W %h:%p";
    b.set("AGENTSPACE_STATE_DIR", "rel-state")
        .set("AGENTSPACE_SSH_DIR", "rel-ssh")
        .set("AGENTSPACE_PROXY_COMMAND", proxy)
        .set("AGENTSPACE_AWS_CMD", "aws-relative.cmd")
        .set("agentspace_mixed_cMd", "mixed-value")
        .set("AGENTSPACE_EMPTY_CMD", "")
        .set("AGENTSPACE__CMD", "no-name-between")
        .set("AGENTSPACE_OWN_VARIABLE", "own")
        .set("AGENTSPACE_TIMEZONE", "UTC")
        .set("AGENTSPACE_DIAGNOSTIC", "1");
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    assert_eq!(guard_lines(&r.stdout).len(), 1, "{}", r.stdout);
    let runs = wait_for_runs(&b.standin_dir(), 1);
    let run = &runs[0];
    run.assert_path_var("USERPROFILE", &b.home);
    run.assert_path_var("AGENTSPACE_STATE_DIR", &b.home.join("rel-state"));
    run.assert_path_var("AGENTSPACE_SSH_DIR", &b.home.join("rel-ssh"));
    run.assert_var("AGENTSPACE_PROXY_COMMAND", proxy);
    run.assert_var("AGENTSPACE_AWS_CMD", "aws-relative.cmd");
    run.assert_var("AGENTSPACE_GUARD_CMD", &b.standin().to_string_lossy());
    let trip = b
        .standin_dir()
        .join(TRIPWIRE)
        .to_string_lossy()
        .into_owned();
    run.assert_var("AGENTSPACE_SSH_CMD", &trip);
    run.assert_var("AGENTSPACE_SCP_CMD", &trip);
    run.assert_var("AGENTSPACE_MIXED_CMD", "mixed-value");
    for name in [
        "AGENTSPACE_EMPTY_CMD",
        "AGENTSPACE__CMD",
        "AGENTSPACE_OWN_VARIABLE",
        "AGENTSPACE_TIMEZONE",
        "AGENTSPACE_DIAGNOSTIC",
    ] {
        run.assert_absent(name);
    }
    assert!(!tripwire_hit(&b.standin_dir()), "A tripwire ran");
}

/// A folder variable and the proxy command that the keeper does not have are not in the guard's
/// environment either, whatever the logon environment holds (keeper.md, The guard's environment:
/// a value the logon environment holds and the keeper does not have set is removed).
#[test]
fn a_variable_the_keeper_does_not_have_is_not_in_the_guards_environment() {
    let b = Bin::new();
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    let runs = wait_for_runs(&b.standin_dir(), 1);
    for name in [
        "AGENTSPACE_SSH_DIR",
        "AGENTSPACE_PROXY_COMMAND",
        "AGENTSPACE_TIMEZONE",
        "AGENTSPACE_DIAGNOSTIC",
    ] {
        runs[0].assert_absent(name);
    }
    runs[0].assert_path_var("AGENTSPACE_STATE_DIR", &b.state);
}

// ---- G11: the guard outlives the keeper and holds none of its handles ----

/// G11. A keeper `-MaxPasses 1` whose stand-in stays alive: the keeper exits 0 while the stand-in
/// still runs (so the stand-in holds no handle on the keeper's standard streams, or the run would
/// not return until it ended), the stand-in goes on after the keeper has ended, and a new keeper
/// takes `board-holder-keeper.lock` while it runs, logging no `another keeper is running` line, so
/// the guard holds no handle of the keeper's lock. The case then ends the stand-in. It needs no
/// process ID: the stand-in's ticks show it alive.
#[test]
fn the_guard_outlives_its_keeper_and_holds_none_of_its_handles() {
    let b = Bin::new();
    let dir = b.standin_dir();
    let staying = StayingGuard::new(&dir);
    let started = Instant::now();
    let first = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(first.status, Some(0), "{}", first.stderr);
    assert!(
        started.elapsed() < Duration::from_secs(50),
        "The keeper's run waited for the guard"
    );
    assert_eq!(guard_lines(&first.stdout).len(), 1, "{}", first.stdout);
    wait_for_runs(&dir, 1);

    // The keeper has ended, and the stand-in goes on.
    let deadline = Instant::now() + Duration::from_secs(20);
    while ticks_in(&dir) < 1 {
        assert!(Instant::now() < deadline, "The stand-in never ticked");
        std::thread::sleep(Duration::from_millis(100));
    }
    let before = ticks_in(&dir);
    std::thread::sleep(Duration::from_millis(3500));
    assert!(
        ticks_in(&dir) > before,
        "The guard did not outlive its keeper"
    );

    // The next keeper's guard is not asked to stay.
    staying.no_more_stays();
    let second = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(second.status, Some(0), "{}", second.stderr);
    assert!(
        !second.stdout.contains("another keeper is running"),
        "The guard holds a handle of the keeper's lock: {}",
        second.stdout
    );
    assert_eq!(
        idle_count(&second.stdout),
        1,
        "The second keeper ran a pass"
    );
    wait_for_runs(&dir, 2);

    staying.end();
}

// ---- G12, G13, G16: when no guard is started ----

/// G12. A one-pass run, with `AGENTSPACE_GUARD_CMD` set, starts no guard and logs no guard line.
#[test]
fn a_one_pass_run_starts_no_guard() {
    let b = Bin::new();
    let ssh = b.ssh.to_string_lossy().into_owned();
    let r = b.run(&["-SshExe", &ssh, "-Now", "2026-11-19T12:00:00Z"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "2026-11-19T12:00:00Z idle lease=- alias=- pid=-\n"
    );
    settle_runs(&b.standin_dir(), 0);
    assert!(
        !agentspace_dir(&b).exists(),
        "A one-pass run never opens guard.lock"
    );
    assert!(!tripwire_hit(&b.standin_dir()), "A tripwire ran");
}

/// G13. A keeper `-MaxPasses 1` while the case holds `board-holder.lock`: the pass logs `skipped`,
/// and the guard starts all the same, at once and not after the pass's wait for the lock, since
/// the start is not inside a pass and never waits on `board-holder.lock`.
#[test]
fn the_guard_starts_while_the_pass_lock_is_held_by_another() {
    let b = Bin::new();
    std::fs::create_dir_all(&b.state).unwrap();
    let pass_lock = hold_sharing_nothing(&b.state.join("board-holder.lock"));
    let ssh = b.ssh.to_string_lossy().into_owned();
    let mut cmd = b.command(&["-Watch", "-SshExe", &ssh, "-MaxPasses", "1"]);
    let mut child = cmd.spawn().expect("The keeper starts");
    // The pass waits up to 10 seconds for the lock; the stand-in runs well before that.
    let started = Instant::now();
    let deadline = started + Duration::from_secs(8);
    while guard_runs_in(&b.standin_dir()).is_empty() {
        assert!(
            Instant::now() < deadline,
            "The guard did not start while the pass waited for its lock"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        child.try_wait().unwrap().is_none(),
        "The keeper ended before the guard was seen: the start waited for the pass"
    );
    let out = child.wait_with_output().expect("The keeper ends");
    drop(pass_lock);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let lines = lines_of(&stdout);
    assert_eq!(lines.len(), 2, "{stdout}");
    guard_started_time(&lines[0]);
    assert!(
        lines[1].ends_with(" skipped lease=- alias=- pid=- reason=locked\n"),
        "The pass logged {:?}",
        lines[1]
    );
    settle_runs(&b.standin_dir(), 1);
}

/// G16. A second keeper, started while the case holds `board-holder-keeper.lock` as a keeper
/// would (reading shared, its PID in it): its one `another keeper is running pid=<pid>` line, exit
/// 0, no run of the stand-in and no guard line. A keeper that cannot take its lock starts no
/// guard.
#[test]
fn a_keeper_that_cannot_take_its_lock_starts_no_guard() {
    let b = Bin::new();
    std::fs::create_dir_all(&b.state).unwrap();
    let mut keeper_lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(1)
        .open(b.state.join("board-holder-keeper.lock"))
        .expect("The test holds the keeper lock");
    keeper_lock.write_all(b"4242").unwrap();
    keeper_lock.flush().unwrap();
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(0), "{}", r.stderr);
    let lines = lines_of(&r.stdout);
    assert_eq!(lines.len(), 1, "{}", r.stdout);
    assert!(
        lines[0].ends_with(" another keeper is running pid=4242\n"),
        "{:?}",
        lines[0]
    );
    assert!(guard_lines(&r.stdout).is_empty());
    settle_runs(&b.standin_dir(), 0);
    drop(keeper_lock);
}

/// A keeper that ends before its first pass, for a missing home folder, starts no guard either.
#[test]
fn a_keeper_with_no_home_folder_starts_no_guard() {
    let mut b = Bin::new();
    b.unset("USERPROFILE");
    let r = keeper(&b, &["-MaxPasses", "1"]);
    assert_eq!(r.status, Some(1));
    assert_eq!(r.stdout, "");
    settle_runs(&b.standin_dir(), 0);
}

/// The stand-ins' standard handles are the program's own null ones: the keeper run with null
/// handles (as a task with no console has them) still starts a guard, and the guard's start
/// writes nothing the keeper reads.
#[test]
fn a_keeper_with_null_handles_starts_the_guard() {
    let b = Bin::new();
    let ssh = b.ssh.to_string_lossy().into_owned();
    let mut cmd = b.command(&["-Watch", "-SshExe", &ssh, "-MaxPasses", "1"]);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let status = cmd.status().expect("The keeper runs");
    assert_eq!(status.code(), Some(0));
    let log = log_of(&b);
    assert_eq!(guard_lines(&log).len(), 1, "{log}");
    wait_for_runs(&b.standin_dir(), 1);
    settle_runs(&b.standin_dir(), 1);
}

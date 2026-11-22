//! The guard's files and lock (docs/state.md, The guard's files, The guard's log, The guard's
//! lock) and a provisioning run's lock as the guard's reader sees it (The provisioning run's
//! lock): the paths beside hosts.json, a lock taken without waiting, tried without being kept, and
//! never taken for a free one when it cannot be opened.
//!
//! guard.lock is held from a second handle in this process, and from another process through the
//! test-only `hosts-guard-lock-helper`. provision.lock held by another process is shown through
//! the crate's `hosts-run-lock-helper`.

mod common;

use agentspace_hosts::{
    GuardLock, HostsError, guard_is_running, guard_lock_path, guard_log_path, hosts_path,
    run_is_running, try_guard_lock, try_run_lock,
};
use common::*;
use std::path::Path;
use std::time::Duration;

const PROJECT: &str = "agentspace";

fn lock_file(t: &TempHome) -> std::path::PathBuf {
    t.state().join("guard.lock")
}

fn is_cannot_lock_guard(e: &HostsError) -> bool {
    matches!(e, HostsError::CannotLockGuard(_))
}

fn take(home: &Path) -> GuardLock {
    try_guard_lock(home)
        .expect("A free lock is no error")
        .expect("A free lock is taken")
}

fn free_to_take(home: &Path) -> bool {
    free_within(FREE_WITHIN, || {
        try_guard_lock(home).map(|l| l.is_some()).unwrap_or(false)
    })
}

// --- The paths --------------------------------------------------------------------------------

#[test]
fn the_guards_files_are_beside_the_registry_under_the_home() {
    let t = TempHome::new();
    assert_eq!(guard_lock_path(t.path()), t.state().join("guard.lock"));
    assert_eq!(guard_log_path(t.path()), t.state().join("guard.log"));
    assert_eq!(
        guard_lock_path(t.path()).parent(),
        hosts_path(t.path()).parent(),
        "guard.lock is beside hosts.json"
    );
    assert_eq!(
        guard_log_path(t.path()).parent(),
        hosts_path(t.path()).parent(),
        "guard.log is beside hosts.json"
    );
}

#[test]
fn the_guards_paths_follow_the_home_alone() {
    // AGENTSPACE_STATE_DIR does not move them: the functions take the home and nothing else, and
    // the crate reads no environment variable, so a different home is the only thing that moves
    // them.
    let a = TempHome::new();
    let b = TempHome::new();
    assert_ne!(guard_lock_path(a.path()), guard_lock_path(b.path()));
    assert_ne!(guard_log_path(a.path()), guard_log_path(b.path()));
    assert!(guard_lock_path(a.path()).starts_with(a.path()));
    assert!(guard_log_path(a.path()).starts_with(a.path()));
    assert_eq!(
        guard_lock_path(a.path()),
        guard_lock_path(a.path()),
        "The same home always gives the same path"
    );
}

#[test]
fn the_two_files_are_distinct() {
    let t = TempHome::new();
    assert_ne!(guard_lock_path(t.path()), guard_log_path(t.path()));
}

// --- Taking it --------------------------------------------------------------------------------

#[test]
fn taking_a_free_guard_lock_gives_it() {
    let t = TempHome::new();
    assert!(try_guard_lock(t.path()).expect("A free lock").is_some());
}

#[test]
fn taking_it_makes_the_folder_and_the_lock_file_and_nothing_else() {
    let t = TempHome::new();
    assert!(!t.state().exists());
    let _lock = take(t.path());
    assert!(lock_file(&t).is_file());
    assert_eq!(
        TempHome::names_in(&t.state()),
        ["guard.lock"],
        "Taking the lock writes no log and no other file"
    );
    assert!(!guard_log_path(t.path()).exists());
}

#[test]
fn a_second_try_while_it_is_held_finds_it_held_without_waiting() {
    let t = TempHome::new();
    let first = take(t.path());
    let started = std::time::Instant::now();
    let second = try_guard_lock(t.path()).expect("A held lock is not an error");
    assert!(second.is_none(), "The lock was held, so none is given");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "It is always taken without waiting: one try"
    );
    drop(first);
}

#[test]
fn it_stays_held_for_as_long_as_the_value_lives() {
    let t = TempHome::new();
    let lock = take(t.path());
    for _ in 0..3 {
        assert!(try_guard_lock(t.path()).unwrap().is_none());
    }
    drop(lock);
    assert!(free_to_take(t.path()), "A dropped lock is free again");
}

#[test]
fn the_file_is_never_deleted_and_stays_empty() {
    let t = TempHome::new();
    drop(take(t.path()));
    let path = lock_file(&t);
    assert!(path.is_file(), "The file is never deleted");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    // And again, over the file that is already there.
    assert!(free_to_take(t.path()));
    assert!(path.is_file());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
}

#[test]
fn taking_it_over_an_existing_folder_and_file_keeps_what_else_is_there() {
    let t = TempHome::new();
    t.put_hosts(b"x");
    let _lock = take(t.path());
    assert_eq!(t.hosts_bytes(), b"x", "The registry is not touched");
    assert_eq!(TempHome::names_in(&t.state()), ["guard.lock", "hosts.json"]);
}

#[test]
fn each_home_has_its_own_guard_lock() {
    let a = TempHome::new();
    let b = TempHome::new();
    let _ga = take(a.path());
    assert!(try_guard_lock(b.path()).unwrap().is_some());
}

#[test]
fn debug_prints_the_name_and_no_path() {
    let t = TempHome::new();
    let lock = take(t.path());
    assert_eq!(format!("{lock:?}"), "GuardLock { .. }");
}

#[test]
fn the_guard_lock_and_a_hosts_run_lock_are_separate_locks() {
    let t = TempHome::new();
    let guard = take(t.path());
    assert!(
        try_run_lock(t.path(), PROJECT).unwrap().is_some(),
        "A held guard lock does not hold a provisioning run's"
    );
    assert!(!run_is_running(t.path(), PROJECT).unwrap());
    drop(guard);
    let run = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    assert!(
        try_guard_lock(t.path()).unwrap().is_some(),
        "A held run lock does not hold the guard's"
    );
    assert!(!guard_is_running(t.path()).unwrap());
    drop(run);
}

// --- guard_is_running -------------------------------------------------------------------------

#[test]
fn a_missing_file_and_folder_is_a_lock_nobody_holds_and_asking_creates_neither() {
    let t = TempHome::new();
    assert!(!t.state().exists());
    assert!(!guard_is_running(t.path()).expect("A missing file is no error"));
    assert!(
        !t.state().exists(),
        "Asking must not create the folder either"
    );
}

#[test]
fn a_missing_file_in_an_existing_folder_is_free_and_asking_does_not_create_it() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state()).unwrap();
    assert!(!guard_is_running(t.path()).unwrap());
    assert!(!lock_file(&t).exists(), "Asking must not create the file");
    assert!(TempHome::names_in(&t.state()).is_empty());
}

#[test]
fn a_guard_is_running_exactly_while_its_lock_is_held() {
    let t = TempHome::new();
    assert!(!guard_is_running(t.path()).unwrap());
    let lock = take(t.path());
    assert!(guard_is_running(t.path()).unwrap());
    drop(lock);
    assert!(
        free_within(FREE_WITHIN, || !guard_is_running(t.path()).unwrap()),
        "No guard is running once its lock is dropped"
    );
}

#[test]
fn asking_whether_a_guard_is_running_does_not_keep_the_lock() {
    // The keeper releases the lock at once if it got it, only to learn whether a guard is running.
    let t = TempHome::new();
    drop(take(t.path()));
    for _ in 0..3 {
        assert!(
            free_within(FREE_WITHIN, || !guard_is_running(t.path()).unwrap()),
            "No guard is running while nobody holds the lock"
        );
        assert!(
            free_to_take(t.path()),
            "guard_is_running must not leave the lock held"
        );
    }
}

#[test]
fn asking_does_not_disturb_a_guard_that_holds_the_lock() {
    let t = TempHome::new();
    let lock = take(t.path());
    assert!(guard_is_running(t.path()).unwrap());
    assert!(guard_is_running(t.path()).unwrap());
    assert!(
        try_guard_lock(t.path()).unwrap().is_none(),
        "The holder must still hold it"
    );
    drop(lock);
}

#[test]
fn asking_after_a_missing_file_leaves_the_next_take_to_create_it() {
    let t = TempHome::new();
    assert!(!guard_is_running(t.path()).unwrap());
    let _lock = take(t.path());
    assert!(lock_file(&t).is_file());
}

#[test]
fn another_thread_finds_it_held_and_then_free() {
    let t = TempHome::new();
    let lock = take(t.path());
    let home = t.path().to_path_buf();
    let h = std::thread::spawn(move || {
        let held = try_guard_lock(&home).unwrap().is_none();
        let running = guard_is_running(&home).unwrap();
        (held, running)
    });
    assert_eq!(h.join().unwrap(), (true, true));
    drop(lock);
    let home = t.path().to_path_buf();
    let h = std::thread::spawn(move || free_to_take(&home));
    assert!(h.join().unwrap(), "A dropped lock is free again");
}

// --- Failures that are not "held" -------------------------------------------------------------

#[test]
fn a_folder_at_the_locks_path_is_an_error_not_a_free_lock_and_not_a_held_one() {
    let t = TempHome::new();
    std::fs::create_dir_all(lock_file(&t)).unwrap();
    let e = try_guard_lock(t.path())
        .expect_err("A folder at the path is a failure of the taker's own, never none or a lock");
    assert!(is_cannot_lock_guard(&e), "The error names the guard's lock");
    assert_message(&e, t.path());
    let e = guard_is_running(t.path())
        .expect_err("A folder at the path is not a free lock either, and not a held one");
    assert!(is_cannot_lock_guard(&e), "The error names the guard's lock");
    assert_message(&e, t.path());
    assert!(lock_file(&t).is_dir(), "Nothing is repaired or removed");
}

#[test]
fn a_file_where_the_folder_belongs_is_an_error() {
    let t = TempHome::new();
    std::fs::write(t.state(), b"x").unwrap();
    let e = try_guard_lock(t.path()).expect_err("Something at .agentspace that is not a folder");
    assert!(is_cannot_lock_guard(&e), "The error names the guard's lock");
    assert_message(&e, t.path());
    let e = guard_is_running(t.path()).expect_err(
        "A file where the folder belongs is not a free lock to the keeper's try either",
    );
    assert!(is_cannot_lock_guard(&e), "The error names the guard's lock");
    assert_message(&e, t.path());
    assert!(t.state().is_file(), "Nothing is repaired or removed");
}

#[test]
fn a_file_where_the_home_folder_belongs_is_an_error() {
    let t = TempHome::new();
    let home_as_file = t.path().join("a-file");
    std::fs::write(&home_as_file, b"x").unwrap();
    let e = try_guard_lock(&home_as_file).expect_err("A file is not a home folder");
    assert!(is_cannot_lock_guard(&e), "The error names the guard's lock");
    assert_message(&e, t.path());
}

// --- A provisioning run's lock held by another process -----------------------------------------

#[test]
fn a_lock_held_by_another_process_is_reported_held_and_is_still_held_afterwards() {
    let t = TempHome::new();
    let mut helper = Helper::start(t.path(), PROJECT, 5000);
    assert_eq!(helper.next_line(Duration::from_secs(30)), "held");

    assert!(
        run_is_running(t.path(), PROJECT).unwrap(),
        "A lock held by another process is a run running"
    );
    // The check released nothing of the holder's: it is still held, to this process and to another.
    assert!(
        try_run_lock(t.path(), PROJECT).unwrap().is_none(),
        "Asking must leave the other process holding it"
    );
    assert!(run_is_running(t.path(), PROJECT).unwrap());
    let mut second = Helper::start(t.path(), PROJECT, 0);
    assert_eq!(
        second.next_line(Duration::from_secs(30)),
        "busy",
        "The helper still holds it for a third taker"
    );
    assert_eq!(second.wait_exit(Duration::from_secs(30)), Some(0));

    // It was the holder that kept it, and it lets go on its own when it has held it for its time.
    assert_eq!(helper.wait_exit(Duration::from_secs(30)), Some(0));
    assert!(
        free_within(Duration::from_secs(10), || !run_is_running(
            t.path(),
            PROJECT
        )
        .unwrap()),
        "The lock is free once the holder has exited"
    );
    assert!(
        free_within(FREE_WITHIN, || try_run_lock(t.path(), PROJECT)
            .unwrap()
            .is_some()),
        "The lock is free once the holder has exited"
    );
}

#[test]
fn asking_about_a_run_held_by_another_process_does_not_free_it_for_a_killed_holders_sake() {
    // A holder that is killed frees the lock, and until then every ask finds it held.
    let t = TempHome::new();
    let mut helper = Helper::start(t.path(), PROJECT, 120_000);
    assert_eq!(helper.next_line(Duration::from_secs(30)), "held");
    for _ in 0..5 {
        assert!(run_is_running(t.path(), PROJECT).unwrap());
    }
    assert!(try_run_lock(t.path(), PROJECT).unwrap().is_none());
    helper.kill();
    assert!(
        free_within(Duration::from_secs(15), || !run_is_running(
            t.path(),
            PROJECT
        )
        .unwrap()),
        "A killed run leaves the lock free"
    );
}

#[test]
fn a_run_held_by_another_process_does_not_hold_the_guards_lock() {
    let t = TempHome::new();
    let mut helper = Helper::start(t.path(), PROJECT, 3000);
    assert_eq!(helper.next_line(Duration::from_secs(30)), "held");
    assert!(try_guard_lock(t.path()).unwrap().is_some());
    assert!(!guard_is_running(t.path()).unwrap());
    helper.kill();
}

// --- The guard's lock across processes, through the guard helper program ------------------------

/// A running `hosts-guard-lock-helper`, killed when dropped if still running.
struct GuardHelper {
    child: std::process::Child,
    lines: std::io::Lines<std::io::BufReader<std::process::ChildStdout>>,
}

impl GuardHelper {
    fn start(home: &Path, hold_ms: u64) -> GuardHelper {
        use std::io::BufRead;
        use std::process::{Command, Stdio};
        let mut child = Command::new(env!("CARGO_BIN_EXE_hosts-guard-lock-helper"))
            .arg(home)
            .arg(hold_ms.to_string())
            .env("USERPROFILE", home)
            .env("HOME", home)
            .env("LOCALAPPDATA", home)
            .env("APPDATA", home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("The guard helper must start");
        let out = child.stdout.take().expect("The helper's stdout is piped");
        GuardHelper {
            child,
            lines: std::io::BufReader::new(out).lines(),
        }
    }

    /// The helper's one line, `held` or `busy`.
    fn first_line(&mut self) -> String {
        self.lines
            .next()
            .expect("The helper must print a line")
            .expect("The helper's line must be readable")
            .trim()
            .to_string()
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for GuardHelper {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.kill();
        }
    }
}

#[test]
fn a_guard_lock_held_by_another_process_is_reported_held_and_is_still_held_afterwards() {
    let t = TempHome::new();
    let mut helper = GuardHelper::start(t.path(), 120_000);
    assert_eq!(helper.first_line(), "held");
    assert!(
        guard_is_running(t.path()).unwrap(),
        "A lock held by another process is a guard running"
    );
    assert!(
        try_guard_lock(t.path()).unwrap().is_none(),
        "Asking must leave the other process holding it"
    );
    assert!(guard_is_running(t.path()).unwrap());
    let mut second = GuardHelper::start(t.path(), 0);
    assert_eq!(second.first_line(), "busy", "A third taker finds it held");
    helper.kill();
    assert!(
        free_within(Duration::from_secs(15), || !guard_is_running(t.path())
            .unwrap()),
        "A killed guard leaves the lock free"
    );
    assert!(free_to_take(t.path()));
    assert!(lock_file(&t).is_file(), "The file is never deleted");
}

#[test]
fn a_helper_finds_the_guard_lock_we_hold_busy() {
    let t = TempHome::new();
    let lock = take(t.path());
    let mut helper = GuardHelper::start(t.path(), 0);
    assert_eq!(helper.first_line(), "busy");
    drop(lock);
    assert!(
        free_within(FREE_WITHIN, || {
            let mut h = GuardHelper::start(t.path(), 0);
            h.first_line() == "held"
        }),
        "Once we let go, the other process gets it"
    );
}

#[test]
fn a_program_the_holder_starts_does_not_keep_the_guard_lock() {
    // The handle MUST NOT be inheritable, so no program the guard starts holds it.
    let t = TempHome::new();
    let lock = take(t.path());
    // Started while we hold the guard lock, in another home so that it takes its own lock, and
    // living for four seconds.
    let other = TempHome::new();
    let mut child = GuardHelper::start(other.path(), 4000);
    assert_eq!(child.first_line(), "held");
    drop(lock);
    assert!(
        free_to_take(t.path()),
        "The lock stayed held by the program its holder started: its handle was inherited"
    );
    child.kill();
}

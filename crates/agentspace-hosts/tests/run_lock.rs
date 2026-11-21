//! The provisioning run's lock (docs/state.md, The provisioning run's lock): held for a run's whole
//! life, taken without waiting, released however the holder ends, across threads and processes.

mod common;

use agentspace_hosts::{run_is_running, try_run_lock};
use common::*;
use std::time::Duration;

const PROJECT: &str = "agentspace";

fn lock_file(t: &TempHome, project: &str) -> std::path::PathBuf {
    t.state().join("hosts").join(project).join("provision.lock")
}

// --- In one process ---------------------------------------------------------------------------

#[test]
fn taking_a_free_run_lock_gives_it() {
    let t = TempHome::new();
    let lock = try_run_lock(t.path(), PROJECT).expect("A free lock is taken");
    assert!(lock.is_some());
}

#[test]
fn taking_it_makes_the_hosts_folder_and_the_lock_file() {
    let t = TempHome::new();
    assert!(!t.state().exists());
    let _lock = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    assert!(lock_file(&t, PROJECT).is_file());
}

#[test]
fn a_second_try_while_it_is_held_finds_it_held_without_waiting() {
    let t = TempHome::new();
    let first = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    let started = std::time::Instant::now();
    let second = try_run_lock(t.path(), PROJECT).expect("A held lock is not an error");
    assert!(second.is_none(), "The lock was held, so none is given");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "It is always taken without waiting: one try"
    );
    drop(first);
}

#[test]
fn dropping_the_lock_frees_it() {
    let t = TempHome::new();
    drop(try_run_lock(t.path(), PROJECT).unwrap().unwrap());
    assert!(
        free_within(FREE_WITHIN, || try_run_lock(t.path(), PROJECT)
            .unwrap()
            .is_some()),
        "A dropped lock is free again"
    );
}

#[test]
fn the_lock_is_held_for_as_long_as_the_value_lives() {
    let t = TempHome::new();
    let lock = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    for _ in 0..3 {
        assert!(try_run_lock(t.path(), PROJECT).unwrap().is_none());
    }
    drop(lock);
    assert!(
        free_within(FREE_WITHIN, || try_run_lock(t.path(), PROJECT)
            .unwrap()
            .is_some()),
        "A dropped lock is free again"
    );
}

#[test]
fn the_file_is_never_deleted_and_stays_empty() {
    let t = TempHome::new();
    drop(try_run_lock(t.path(), PROJECT).unwrap().unwrap());
    let path = lock_file(&t, PROJECT);
    assert!(path.is_file(), "The file is never deleted");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
}

#[test]
fn each_project_has_its_own_lock() {
    let t = TempHome::new();
    let a = try_run_lock(t.path(), "alpha").unwrap();
    let b = try_run_lock(t.path(), "beta").unwrap();
    assert!(
        a.is_some() && b.is_some(),
        "A run of one host never blocks another's"
    );
    assert!(lock_file(&t, "alpha").is_file());
    assert!(lock_file(&t, "beta").is_file());
}

#[test]
fn each_home_has_its_own_lock() {
    let a = TempHome::new();
    let b = TempHome::new();
    let _ga = try_run_lock(a.path(), PROJECT).unwrap().unwrap();
    assert!(try_run_lock(b.path(), PROJECT).unwrap().is_some());
}

#[test]
fn the_box_has_a_run_lock_of_its_own_too() {
    let t = TempHome::new();
    let lock = try_run_lock(t.path(), "swimcoach").unwrap();
    assert!(lock.is_some());
    assert!(lock_file(&t, "swimcoach").is_file());
}

#[test]
fn taking_the_run_lock_writes_no_registry_and_no_other_file_in_the_hosts_folder() {
    let t = TempHome::new();
    let _l = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    assert!(!t.hosts_json().exists());
    assert!(!t.state().join("hosts.lock").exists());
    assert_eq!(
        TempHome::names_in(&t.state().join("hosts").join(PROJECT)),
        ["provision.lock"],
        "The writer MUST NOT put any other file in a host's folder"
    );
}

// --- run_is_running ---------------------------------------------------------------------------

#[test]
fn a_missing_file_is_a_lock_nobody_holds() {
    let t = TempHome::new();
    assert!(!run_is_running(t.path(), PROJECT).expect("A missing file is no error"));
}

#[test]
fn a_run_is_running_exactly_while_its_lock_is_held() {
    let t = TempHome::new();
    assert!(!run_is_running(t.path(), PROJECT).unwrap());
    let lock = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    assert!(run_is_running(t.path(), PROJECT).unwrap());
    drop(lock);
    assert!(
        free_within(FREE_WITHIN, || !run_is_running(t.path(), PROJECT).unwrap()),
        "No run is running once its lock is dropped"
    );
}

#[test]
fn asking_whether_a_run_is_running_does_not_keep_the_lock() {
    // The driver releases the lock at once if it got it, only to learn whether a run is running.
    let t = TempHome::new();
    drop(try_run_lock(t.path(), PROJECT).unwrap().unwrap());
    for _ in 0..3 {
        assert!(
            free_within(FREE_WITHIN, || !run_is_running(t.path(), PROJECT).unwrap()),
            "No run is running while nobody holds the lock"
        );
        assert!(
            free_within(FREE_WITHIN, || try_run_lock(t.path(), PROJECT)
                .unwrap()
                .is_some()),
            "run_is_running must not leave the lock held"
        );
    }
}

#[test]
fn asking_does_not_disturb_a_run_that_holds_the_lock() {
    let t = TempHome::new();
    let lock = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    assert!(run_is_running(t.path(), PROJECT).unwrap());
    assert!(run_is_running(t.path(), PROJECT).unwrap());
    assert!(
        try_run_lock(t.path(), PROJECT).unwrap().is_none(),
        "The holder must still hold it"
    );
    drop(lock);
}

#[test]
fn a_run_of_one_project_is_not_a_run_of_another() {
    let t = TempHome::new();
    let _lock = try_run_lock(t.path(), "alpha").unwrap().unwrap();
    assert!(run_is_running(t.path(), "alpha").unwrap());
    assert!(!run_is_running(t.path(), "beta").unwrap());
}

// --- Failures that are not "held" -------------------------------------------------------------

#[test]
fn a_folder_at_the_locks_path_is_an_error_not_a_free_lock_and_not_a_held_one() {
    let t = TempHome::new();
    std::fs::create_dir_all(lock_file(&t, PROJECT)).unwrap();
    let e = try_run_lock(t.path(), PROJECT)
        .expect_err("A folder at the path is a failure of the taker's own, never none or a lock");
    assert_message(&e, t.path());
    let e = run_is_running(t.path(), PROJECT)
        .expect_err("A folder at the path is not a free lock either");
    assert_message(&e, t.path());
    assert!(
        lock_file(&t, PROJECT).is_dir(),
        "Nothing is repaired or removed"
    );
}

#[test]
fn a_file_at_the_hosts_folders_path_is_an_error() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state()).unwrap();
    std::fs::write(t.state().join("hosts"), b"x").unwrap();
    let e = try_run_lock(t.path(), PROJECT)
        .expect_err("Something at hosts that is not a folder fails the taker");
    assert_message(&e, t.path());
    assert!(t.state().join("hosts").is_file());
}

#[test]
fn a_file_at_the_project_folders_path_is_an_error() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state().join("hosts")).unwrap();
    std::fs::write(t.state().join("hosts").join(PROJECT), b"x").unwrap();
    let e = try_run_lock(t.path(), PROJECT)
        .expect_err("Something at hosts/<project> that is not a folder fails the taker");
    assert_message(&e, t.path());
    assert!(t.state().join("hosts").join(PROJECT).is_file());
}

#[test]
fn a_file_where_the_home_folder_belongs_is_an_error() {
    let t = TempHome::new();
    let home_as_file = t.path().join("a-file");
    std::fs::write(&home_as_file, b"x").unwrap();
    let e = try_run_lock(&home_as_file, PROJECT).expect_err("A file is not a home folder");
    assert_message(&e, t.path());
}

// --- Across threads ---------------------------------------------------------------------------

#[test]
fn another_thread_finds_it_held_and_then_free() {
    let t = TempHome::new();
    let lock = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    let home = t.path().to_path_buf();
    let h = std::thread::spawn(move || {
        let held = try_run_lock(&home, PROJECT).unwrap().is_none();
        let running = run_is_running(&home, PROJECT).unwrap();
        (held, running)
    });
    assert_eq!(h.join().unwrap(), (true, true));
    drop(lock);
    let home = t.path().to_path_buf();
    let h = std::thread::spawn(move || {
        free_within(FREE_WITHIN, || {
            try_run_lock(&home, PROJECT).unwrap().is_some()
        })
    });
    assert!(h.join().unwrap(), "A dropped lock is free again");
}

// --- Across processes -------------------------------------------------------------------------

#[test]
fn a_helper_process_that_holds_the_lock_makes_us_find_it_held() {
    let t = TempHome::new();
    let mut helper = Helper::start(t.path(), PROJECT, 4000);
    assert_eq!(helper.next_line(Duration::from_secs(30)), "held");
    assert!(
        try_run_lock(t.path(), PROJECT).unwrap().is_none(),
        "Another process holds the lock"
    );
    assert!(run_is_running(t.path(), PROJECT).unwrap());
    assert_eq!(helper.wait_exit(Duration::from_secs(30)), Some(0));
    assert!(
        free_within(Duration::from_secs(10), || !run_is_running(
            t.path(),
            PROJECT
        )
        .unwrap()),
        "The lock must be free once the holder has exited"
    );
    assert!(
        free_within(FREE_WITHIN, || try_run_lock(t.path(), PROJECT)
            .unwrap()
            .is_some()),
        "The lock must be free once the holder has exited"
    );
}

#[test]
fn a_helper_process_finds_the_lock_we_hold_busy() {
    let t = TempHome::new();
    let lock = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    let mut helper = Helper::start(t.path(), PROJECT, 0);
    assert_eq!(helper.next_line(Duration::from_secs(30)), "busy");
    assert_eq!(helper.wait_exit(Duration::from_secs(30)), Some(0));
    drop(lock);
    // A helper makes one try, so a "busy" right after the drop is retried with a fresh helper.
    assert!(
        free_within(FREE_WITHIN, || {
            let mut helper = Helper::start(t.path(), PROJECT, 0);
            let line = helper.next_line(Duration::from_secs(30));
            assert_eq!(helper.wait_exit(Duration::from_secs(30)), Some(0));
            line == "held"
        }),
        "Once we let go, the other process gets it"
    );
}

#[test]
fn two_helper_processes_never_hold_it_together() {
    let t = TempHome::new();
    let mut first = Helper::start(t.path(), PROJECT, 4000);
    assert_eq!(first.next_line(Duration::from_secs(30)), "held");
    let mut second = Helper::start(t.path(), PROJECT, 4000);
    assert_eq!(second.next_line(Duration::from_secs(30)), "busy");
    assert_eq!(second.wait_exit(Duration::from_secs(30)), Some(0));
    first.kill();
}

#[test]
fn a_helper_for_another_project_does_not_hold_ours() {
    let t = TempHome::new();
    let mut helper = Helper::start(t.path(), "beta", 3000);
    assert_eq!(helper.next_line(Duration::from_secs(30)), "held");
    assert!(try_run_lock(t.path(), PROJECT).unwrap().is_some());
    assert!(run_is_running(t.path(), "beta").unwrap());
    helper.kill();
}

#[test]
fn a_killed_holder_leaves_the_lock_free() {
    // The operating system releases it however the run ends (state.md).
    let t = TempHome::new();
    let mut helper = Helper::start(t.path(), PROJECT, 120_000);
    assert_eq!(helper.next_line(Duration::from_secs(30)), "held");
    assert!(try_run_lock(t.path(), PROJECT).unwrap().is_none());
    helper.kill();
    assert!(
        free_within(Duration::from_secs(15), || {
            try_run_lock(t.path(), PROJECT)
                .map(|l| l.is_some())
                .unwrap_or(false)
        }),
        "A killed run must leave the lock free"
    );
    assert!(
        lock_file(&t, PROJECT).is_file(),
        "The file is never deleted"
    );
}

#[test]
fn a_program_the_holder_starts_does_not_keep_the_lock() {
    // The handle MUST NOT be inheritable, so no program the run starts holds it.
    let t = TempHome::new();
    let lock = try_run_lock(t.path(), PROJECT).unwrap().unwrap();
    // Started while we hold the run lock, and living for four seconds.
    let mut other = Helper::start(t.path(), "other-project", 4000);
    assert_eq!(other.next_line(Duration::from_secs(30)), "held");
    drop(lock);
    // A bounded retry, not one try (architect, 2026-11-20, for v0.07-T02): on Linux, a fork made
    // by a test running in parallel holds a duplicate of the lock's descriptor until that child's
    // exec closes it, so one try right after the drop can find the lock held although nothing
    // inherited it. The helper lives 4 seconds, so a handle it inherited is still held after the
    // 2 seconds polled here, and real inheritance is still caught.
    assert!(
        free_within(FREE_WITHIN, || try_run_lock(t.path(), PROJECT)
            .unwrap()
            .is_some()),
        "The lock stayed held by the program its holder started: its handle was inherited"
    );
    other.kill();
}

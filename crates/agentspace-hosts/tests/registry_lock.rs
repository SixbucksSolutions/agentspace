//! The host registry lock (docs/state.md, The host registry lock): `hosts.lock`, held with no
//! sharing, never deleted, not inheritable, waited for.

mod common;

use agentspace_hosts::{lock, read, try_lock};
use common::*;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn taking_the_lock_makes_the_agentspace_folder_and_hosts_lock() {
    let t = TempHome::new();
    assert!(!t.state().exists());
    let g = lock(t.path()).expect("The lock must be taken");
    assert!(t.state().join("hosts.lock").is_file());
    drop(g);
}

#[test]
fn taking_the_lock_does_not_make_the_registry() {
    let t = TempHome::new();
    let _g = lock(t.path()).unwrap();
    assert!(!t.hosts_json().exists());
}

#[test]
fn the_lock_file_is_never_deleted_and_stays_empty() {
    let t = TempHome::new();
    drop(lock(t.path()).unwrap());
    let path = t.state().join("hosts.lock");
    assert!(path.is_file(), "The lock file is never deleted");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    drop(lock(t.path()).unwrap());
    assert!(path.is_file());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
}

#[test]
fn the_lock_is_free_again_at_once_after_it_is_dropped() {
    let t = TempHome::new();
    drop(lock(t.path()).unwrap());
    let started = Instant::now();
    drop(lock(t.path()).unwrap());
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn the_lock_of_one_home_does_not_hold_up_another() {
    let a = TempHome::new();
    let b = TempHome::new();
    let _ga = lock(a.path()).unwrap();
    let started = Instant::now();
    let _gb = lock(b.path()).unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_second_taker_on_another_thread_waits_until_the_first_drops_it() {
    let t = TempHome::new();
    let first = lock(t.path()).unwrap();
    let home = t.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let r = lock(&home);
        let got_it_at = Instant::now();
        let ok = r.is_ok();
        tx.send((got_it_at, ok)).unwrap();
        drop(r);
    });
    // The second taker must still be waiting a while after it started.
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        rx.try_recv().is_err(),
        "The second taker got the lock while the first held it"
    );
    let released_at = Instant::now();
    drop(first);
    let (got_it_at, ok) = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("The second taker must get the lock once the first lets go");
    assert!(ok);
    assert!(
        got_it_at >= released_at,
        "The second taker got it before the first dropped it"
    );
    waiter.join().unwrap();
}

#[test]
fn many_takers_one_at_a_time_never_overlap() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    let t = TempHome::new();
    let inside = Arc::new(AtomicU32::new(0));
    let worst = Arc::new(AtomicU32::new(0));
    let mut threads = Vec::new();
    for _ in 0..6 {
        let home = t.path().to_path_buf();
        let inside = inside.clone();
        let worst = worst.clone();
        threads.push(std::thread::spawn(move || {
            for _ in 0..5 {
                let g = lock(&home).expect("The lock must be taken in the end");
                let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                worst.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                inside.fetch_sub(1, Ordering::SeqCst);
                drop(g);
            }
        }));
    }
    for th in threads {
        th.join().unwrap();
    }
    assert_eq!(
        worst.load(Ordering::SeqCst),
        1,
        "Two takers held the lock at once"
    );
}

#[test]
fn a_reader_does_not_wait_for_the_lock() {
    let t = TempHome::new();
    let _g = lock(t.path()).unwrap();
    let started = Instant::now();
    assert_eq!(read(t.path()).unwrap(), seed());
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_folder_at_the_locks_path_fails_at_once_and_is_never_reported_busy() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state().join("hosts.lock")).unwrap();
    let started = Instant::now();
    let e = lock(t.path()).expect_err("A folder is not a lock");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "A failure other than the lock being held must not be waited for"
    );
    assert!(
        is_cannot_lock(&e),
        "Expected a cannot-lock error, got {e:?}"
    );
    assert_message(&e, t.path());
}

#[test]
fn a_file_where_the_agentspace_folder_belongs_fails_at_once() {
    let t = TempHome::new();
    std::fs::write(t.state(), b"x").unwrap();
    let started = Instant::now();
    let e = lock(t.path()).expect_err("A file is not a folder");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(is_folder_file(&e), "{e:?}");
    assert_message(&e, t.path());
}

#[test]
fn a_file_where_the_home_folder_belongs_fails_at_once() {
    let t = TempHome::new();
    let home_as_file = t.path().join("a-file");
    std::fs::write(&home_as_file, b"x").unwrap();
    let started = Instant::now();
    let e = lock(&home_as_file).expect_err("A file is not a home folder");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(is_folder_file(&e), "{e:?}");
    assert_message(&e, t.path());
}

#[test]
fn the_locks_handle_is_not_inherited_by_a_program_started_while_it_is_held() {
    // A program the holder starts must not keep the lock after the holder lets it go
    // (state.md: "the handle MUST NOT be inheritable").
    let t = TempHome::new();
    let guard = lock(t.path()).unwrap();
    // The helper runs for four seconds, holding a run lock of another project.
    let mut helper = Helper::start(t.path(), "other-project", 4000);
    assert_eq!(helper.next_line(Duration::from_secs(20)), "held");
    drop(guard);
    assert!(
        free_within(FREE_WITHIN, || try_lock(t.path())
            .expect("The lock must be takeable once its holder drops it")
            .is_some()),
        "The lock stayed held by the program the holder started: its handle was inherited"
    );
    helper.kill();
}

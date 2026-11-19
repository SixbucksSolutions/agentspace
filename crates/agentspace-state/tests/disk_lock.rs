//! The lease state lock (docs/state.md, The lease state lock), including against a PowerShell
//! process that holds `leases.lock` with FileShare.None as the prototype does.
#![cfg(windows)]

mod disk_support;

use std::time::{Duration, Instant};

use agentspace_state::{StoreError, lock};
use disk_support::{Deny, hold, sandbox};

#[test]
fn lock_creates_the_state_folder_and_the_lock_file() {
    let t = sandbox();
    let dir = t.path().join("a").join("state");
    let g = lock(&dir).expect("The lock must be taken");
    assert!(dir.join("leases.lock").is_file());
    drop(g);
}

#[test]
fn lock_leaves_an_existing_lock_file_in_place_after_drop() {
    let t = sandbox();
    let g = lock(t.path()).expect("The lock must be taken");
    drop(g);
    assert!(
        t.path().join("leases.lock").is_file(),
        "The lock file is never deleted"
    );
}

#[test]
fn a_second_lock_in_one_process_waits_until_the_guard_is_dropped() {
    let t = sandbox();
    let first = lock(t.path()).expect("The lock must be taken");
    let dir = t.path().to_path_buf();
    let started = Instant::now();
    let waiter = std::thread::spawn(move || {
        let r = lock(&dir);
        let waited = started.elapsed();
        (r.map(|_g| ()), waited)
    });
    std::thread::sleep(Duration::from_secs(2));
    assert!(!waiter.is_finished(), "The second lock must still wait");
    drop(first);
    let (r, waited) = waiter.join().unwrap();
    assert!(
        r.is_ok(),
        "The second lock must succeed once the first lets go"
    );
    assert!(waited >= Duration::from_secs(2), "Waited only {waited:?}");
    assert!(waited < Duration::from_secs(10), "Waited {waited:?}");
}

#[test]
fn the_lock_is_free_again_after_the_guard_is_dropped() {
    let t = sandbox();
    drop(lock(t.path()).expect("The lock must be taken"));
    let started = Instant::now();
    drop(lock(t.path()).expect("The lock must be free again"));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn another_process_that_lets_go_in_a_few_seconds_makes_lock_wait_then_succeed() {
    let t = sandbox();
    let path = t.path().join("leases.lock");
    let _child = hold(&path, "OpenOrCreate", "ReadWrite", "None", 4);
    let started = Instant::now();
    let g = lock(t.path()).expect("The lock must succeed after the other process exits");
    let waited = started.elapsed();
    drop(g);
    assert!(
        waited >= Duration::from_secs(1),
        "Must have waited for the holder, waited {waited:?}"
    );
    assert!(waited < Duration::from_secs(25), "Waited {waited:?}");
}

#[test]
fn another_process_holding_the_lock_makes_lock_wait_thirty_seconds_then_busy() {
    let t = sandbox();
    let path = t.path().join("leases.lock");
    let _child = hold(&path, "OpenOrCreate", "ReadWrite", "None", 120);
    let started = Instant::now();
    let r = lock(t.path());
    let waited = started.elapsed();
    assert!(
        matches!(r, Err(StoreError::Busy)),
        "Expected Busy, got {}",
        match &r {
            Ok(_) => "a guard".to_string(),
            Err(e) => format!("{e:?}"),
        }
    );
    assert!(
        waited >= Duration::from_secs(29) && waited <= Duration::from_secs(31),
        "Busy after {waited:?}, expected 30 s plus or minus 1"
    );
}

#[test]
fn access_denied_on_the_lock_file_fails_at_once_and_is_not_busy() {
    let t = sandbox();
    std::fs::write(t.path().join("leases.lock"), b"").unwrap();
    let _deny = Deny::on(&t.path().join("leases.lock"));
    let started = Instant::now();
    let r = lock(t.path());
    let waited = started.elapsed();
    assert!(
        matches!(r, Err(StoreError::CannotLock(_))),
        "Expected CannotLock, got {}",
        match &r {
            Ok(_) => "a guard".to_string(),
            Err(e) => format!("{e:?}"),
        }
    );
    assert!(waited < Duration::from_secs(5), "Took {waited:?}");
}

#[test]
fn a_folder_at_the_lock_path_fails_at_once_as_cannot_lock() {
    let t = sandbox();
    std::fs::create_dir(t.path().join("leases.lock")).unwrap();
    let started = Instant::now();
    let r = lock(t.path());
    let waited = started.elapsed();
    assert!(matches!(r, Err(StoreError::CannotLock(_))));
    assert!(waited < Duration::from_secs(5), "Took {waited:?}");
}

#[test]
fn a_file_where_the_state_folder_must_be_fails_at_once_as_cannot_lock() {
    let t = sandbox();
    let f = t.path().join("a-file");
    std::fs::write(&f, b"x").unwrap();
    let started = Instant::now();
    let r = lock(&f);
    assert_eq!(
        r.unwrap_err(),
        StoreError::CannotLock("the state folder is a file".to_string())
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_cannot_lock_message_names_no_path() {
    let t = sandbox();
    std::fs::create_dir(t.path().join("leases.lock")).unwrap();
    match lock(t.path()) {
        Err(StoreError::CannotLock(why)) => {
            assert!(
                !why.contains(t.path().to_str().unwrap()),
                "Message was: {why}"
            );
            assert!(!why.is_empty());
        }
        _ => panic!("Expected CannotLock"),
    }
}

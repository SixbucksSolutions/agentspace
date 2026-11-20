//! `Locks` on Windows (docs/keeper.md, Locks; docs/state.md, The keeper's lock files).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The lock is the open handle, so these tests look at the handle from outside: a second open of
//! the file in this process with chosen share modes, and a second process (this test program run
//! as a role) that takes or holds the lock. Lock files live only in throwaway folders under the
//! temp folder, and every child is ended by a guard through its own handle.
#![cfg(windows)]

mod common;

use std::any::Any;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use agentspace_keeper_api::{Locks, Taken};
use common::*;

const PASS_LOCK: &str = "board-holder.lock";
const KEEPER_LOCK: &str = "board-holder-keeper.lock";
const SHARING_VIOLATION: i32 = 32;
const SHARE_READ: u32 = 1;
const SHARE_ALL: u32 = 7;
const SHARE_READ_WRITE: u32 = 3;

/// The entry the roles run; with no role in the environment it does nothing.
#[test]
fn role_entry() {
    role_main();
}

fn held(taken: Taken) -> Box<dyn Any> {
    match taken {
        Taken::Held(guard) => guard,
        other => panic!(
            "The lock was meant to be taken, and the take gave {}.",
            describe(&other)
        ),
    }
}

fn release(sb: &Sandbox, name: &str) {
    fs::write(sb.dir.join(format!("release-{name}")), "x").expect("The release file writes");
}

fn opened_with_violation(result: std::io::Result<fs::File>) -> bool {
    matches!(result, Err(e) if e.raw_os_error() == Some(SHARING_VIOLATION))
}

fn open_read(path: &Path, share: u32) -> std::io::Result<fs::File> {
    OpenOptions::new().read(true).share_mode(share).open(path)
}

fn open_write(path: &Path, share: u32) -> std::io::Result<fs::File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(share)
        .open(path)
}

// ---------------------------------------------------------------- board-holder.lock

#[test]
fn pass_lock_is_created_empty_and_shares_nothing() {
    let sb = Sandbox::new("pass-share");
    let path = sb.state().join(PASS_LOCK);
    assert!(!path.exists());

    let guard = held(locks().pass(&sb.state()));

    assert!(path.exists(), "The lock file is created if missing");
    assert_eq!(
        fs::metadata(&path)
            .expect("The lock file has attributes")
            .len(),
        0
    );
    assert!(
        opened_with_violation(open_read(&path, SHARE_ALL)),
        "A second open for reading is refused: the lock shares nothing"
    );
    assert!(
        opened_with_violation(open_write(&path, SHARE_ALL)),
        "A second open for writing is refused: the lock shares nothing"
    );
    assert!(
        fs::remove_file(&path).is_err(),
        "The held lock file cannot be deleted"
    );
    drop(guard);
    assert!(
        path.exists(),
        "A release leaves the file: neither file is ever removed"
    );
    assert!(
        open_read(&path, SHARE_ALL).is_ok(),
        "Dropping the lock closes its handle"
    );
}

#[test]
fn pass_lock_is_free_again_after_it_is_dropped() {
    let sb = Sandbox::new("pass-redo");
    drop(held(locks().pass(&sb.state())));
    let start = Instant::now();
    let _again = held(locks().pass(&sb.state()));
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "A free lock is taken at once"
    );
}

#[test]
fn pass_lock_leftover_file_locks_nothing() {
    let sb = Sandbox::new("pass-leftover");
    fs::write(
        sb.state().join(PASS_LOCK),
        "left over by a process that is gone",
    )
    .expect("Seeds");
    let start = Instant::now();
    let _guard = held(locks().pass(&sb.state()));
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "A leftover file is no lock"
    );
}

#[test]
fn pass_lock_in_another_state_folder_is_not_seen() {
    let sb = Sandbox::new("pass-folders");
    let other = sb.dir.join("state2");
    fs::create_dir_all(&other).expect("The second state folder is made");
    let _first = held(locks().pass(&sb.state()));
    let start = Instant::now();
    let _second = held(locks().pass(&other));
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "Each state folder has its own lock"
    );
}

#[test]
fn pass_lock_second_process_is_skipped_after_about_ten_seconds() {
    let sb = Sandbox::new("pass-skip");
    let _guard = held(locks().pass(&sb.state()));
    let mut taker = spawn_role(&sb, "take-pass", "taker");

    let (what, ms) = split_result(&wait_for_file(&sb, "result-taker"));

    assert_eq!(
        what, "skipped",
        "A second taker that cannot take it in time is skipped"
    );
    assert!(
        (9_000..=15_000).contains(&ms),
        "It tries every 100 ms for about 10 seconds first, and took {ms} ms"
    );
    let _ = taker.child.wait();
}

#[test]
fn pass_lock_held_by_another_process_skips_this_one_after_about_ten_seconds() {
    let sb = Sandbox::new("pass-skip-here");
    let mut holder = spawn_role(&sb, "hold-pass", "holder");
    assert_eq!(wait_for_file(&sb, "ready-holder"), "held");

    let start = Instant::now();
    let taken = locks().pass(&sb.state());
    let waited = start.elapsed();

    assert_eq!(describe(&taken), "skipped");
    assert!(
        waited >= Duration::from_secs(9) && waited <= Duration::from_secs(15),
        "It waited about 10 seconds"
    );
    drop(taken);

    // Windows closes a lock's handle when its process ends, a kill included.
    holder
        .child
        .kill()
        .expect("The test's own child can be killed");
    let _ = holder.child.wait();
    let start = Instant::now();
    let _guard = held(locks().pass(&sb.state()));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "A lock of a process that was killed is free"
    );
}

#[test]
fn pass_lock_is_taken_by_a_waiter_when_it_is_released_within_the_wait() {
    let sb = Sandbox::new("pass-retry");
    let guard = held(locks().pass(&sb.state()));
    let mut taker = spawn_role(&sb, "take-pass", "taker");
    wait_for_pid(&sb, "taker");
    std::thread::sleep(Duration::from_secs(2));
    drop(guard);

    let (what, ms) = split_result(&wait_for_file(&sb, "result-taker"));

    assert_eq!(
        what, "held",
        "A taker that waits gets the lock once it is free"
    );
    assert!(
        (1_000..9_500).contains(&ms),
        "It waited for the release, and not for the whole wait: {ms} ms"
    );
    release(&sb, "taker");
    let _ = taker.child.wait();
}

#[test]
fn pass_lock_is_not_inherited_by_a_child_process() {
    // docs/state.md: "The handles MUST NOT be inheritable, so no child process holds a lock."
    let sb = Sandbox::new("pass-inherit");
    let guard = held(locks().pass(&sb.state()));
    let _child = spawn_role(&sb, "sleep", "child");
    wait_for_pid(&sb, "child");
    drop(guard);

    let start = Instant::now();
    let taken = locks().pass(&sb.state());

    assert_eq!(
        describe(&taken),
        "held",
        "A child that outlives the lock does not keep it"
    );
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
fn pass_lock_in_a_folder_that_is_missing_fails_and_does_not_wait() {
    // keeper.md: "An error other than the file being in use fails the pass."
    let sb = Sandbox::new("pass-missing");
    let start = Instant::now();
    let taken = locks().pass(&sb.dir.join("no-such-folder"));
    assert_eq!(describe(&taken), "failed");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "An error that is not 'in use' is not retried"
    );
}

// ---------------------------------------------------------------- board-holder-keeper.lock

#[test]
fn keeper_lock_holds_the_pid_and_shares_read_only() {
    let sb = Sandbox::new("keeper-share");
    let path = sb.state().join(KEEPER_LOCK);

    let guard = held(locks().keeper(&sb.state()));

    // A reader opens it for reading, sharing read and write.
    let mut reader = open_read(&path, SHARE_READ_WRITE).expect("The PID can be read while held");
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).expect("The PID reads");
    assert_eq!(
        bytes,
        std::process::id().to_string().into_bytes(),
        "The content is the keeper's PID as decimal digits and nothing else"
    );
    drop(reader);
    assert!(
        opened_with_violation(open_read(&path, SHARE_READ)),
        "The lock shares read only: a reader that refuses a writer is refused"
    );
    assert!(
        opened_with_violation(open_write(&path, SHARE_ALL)),
        "The lock shares read only: a writer is refused"
    );
    assert!(
        fs::remove_file(&path).is_err(),
        "The held lock file cannot be deleted"
    );
    drop(guard);
    assert!(path.exists(), "A release leaves the file");
    assert!(
        open_write(&path, SHARE_ALL).is_ok(),
        "Dropping the lock closes its handle"
    );
}

#[test]
fn keeper_lock_replaces_what_the_file_held() {
    let sb = Sandbox::new("keeper-stale");
    let path = sb.state().join(KEEPER_LOCK);
    fs::write(&path, "9".repeat(40)).expect("Seeds");

    let _guard = held(locks().keeper(&sb.state()));

    let mut reader = open_read(&path, SHARE_READ_WRITE).expect("The PID can be read while held");
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).expect("The PID reads");
    assert_eq!(
        bytes,
        std::process::id().to_string().into_bytes(),
        "A leftover file locks nothing, and its content is replaced with the PID"
    );
}

#[test]
fn keeper_lock_is_free_again_after_it_is_dropped() {
    let sb = Sandbox::new("keeper-redo");
    drop(held(locks().keeper(&sb.state())));
    let _again = held(locks().keeper(&sb.state()));
}

#[test]
fn keeper_lock_in_another_state_folder_is_not_seen() {
    let sb = Sandbox::new("keeper-folders");
    let other = sb.dir.join("state2");
    fs::create_dir_all(&other).expect("The second state folder is made");
    let _first = held(locks().keeper(&sb.state()));
    let _second = held(locks().keeper(&other));
}

#[test]
fn the_two_locks_are_held_at_once_by_one_process() {
    let sb = Sandbox::new("both");
    let _pass = held(locks().pass(&sb.state()));
    let _keeper = held(locks().keeper(&sb.state()));
}

#[test]
fn keeper_lock_second_process_is_busy_with_the_holders_pid_and_does_not_wait() {
    let sb = Sandbox::new("keeper-busy");
    let _guard = held(locks().keeper(&sb.state()));
    let mut taker = spawn_role(&sb, "take-keeper", "taker");

    let (what, ms) = split_result(&wait_for_file(&sb, "result-taker"));

    assert_eq!(
        what,
        format!("busy:{}", std::process::id()),
        "A second keeper is told which PID holds the lock"
    );
    assert!(
        ms < 3_000,
        "A second taker tries once and never waits: {ms} ms"
    );
    let _ = taker.child.wait();
}

#[test]
fn keeper_lock_held_by_another_process_is_busy_here_with_its_pid_and_free_when_it_is_killed() {
    let sb = Sandbox::new("keeper-busy-here");
    let mut holder = spawn_role(&sb, "hold-keeper", "holder");
    assert_eq!(wait_for_file(&sb, "ready-holder"), "held");

    let start = Instant::now();
    let taken = locks().keeper(&sb.state());
    let took = start.elapsed();

    assert_eq!(describe(&taken), format!("busy:{}", holder.pid()));
    assert!(
        took < Duration::from_secs(3),
        "A keeper's second taker never waits"
    );
    drop(taken);

    holder
        .child
        .kill()
        .expect("The test's own child can be killed");
    let _ = holder.child.wait();
    let _guard = held(locks().keeper(&sb.state()));
}

#[test]
fn keeper_lock_busy_reads_the_pid_from_the_first_digits_it_finds() {
    // keeper.md: it "reads the first 32 bytes it can, and if they are 1 to 10 digits after
    // trimming blanks" gives the PID, else none. Something other than a keeper holds the file
    // here, sharing read only as a keeper does.
    let sb = Sandbox::new("keeper-digits");
    let path = sb.state().join(KEEPER_LOCK);
    let cases: [(&str, Option<u32>); 5] = [
        ("4242", Some(4242)),
        (" 4242 ", Some(4242)),
        ("not a pid", None),
        ("12345678901", None),
        ("", None),
    ];
    for (content, expect) in cases {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(SHARE_READ)
            .open(&path)
            .expect("The test takes the file the way a keeper does");
        file.write_all(content.as_bytes())
            .expect("The content writes");
        file.flush().expect("The content flushes");

        let taken = locks().keeper(&sb.state());

        match taken {
            Taken::Busy(pid) => assert_eq!(pid, expect, "The PID read from {content:?}"),
            other => panic!(
                "A held keeper lock is busy, and the take gave {}.",
                describe(&other)
            ),
        }
        drop(file);
    }
}

#[test]
fn keeper_lock_is_not_inherited_by_a_child_process() {
    // docs/state.md: "The handles MUST NOT be inheritable, so no child process holds a lock."
    let sb = Sandbox::new("keeper-inherit");
    let guard = held(locks().keeper(&sb.state()));
    let _child = spawn_role(&sb, "sleep", "child");
    wait_for_pid(&sb, "child");
    drop(guard);

    let taken = locks().keeper(&sb.state());

    assert_eq!(
        describe(&taken),
        "held",
        "A child that outlives the lock does not keep it"
    );
}

#[test]
fn keeper_lock_in_a_folder_that_is_missing_fails() {
    // keeper.md: "Any other error taking it" is a failure, not a busy lock.
    let sb = Sandbox::new("keeper-missing");
    let taken = locks().keeper(&sb.dir.join("no-such-folder"));
    assert_eq!(describe(&taken), "failed");
}

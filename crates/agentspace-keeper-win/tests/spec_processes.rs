//! `Processes::query` on Windows (docs/keeper.md, Trusting the record).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Each test starts a real process of its own, a role of this test program that sleeps, and reads
//! the process's creation time from the kernel by its own handle, so the expected value does not
//! come from the code under test.
#![cfg(windows)]

mod common;

use std::time::Duration;

use agentspace_keeper_api::{Processes, Query};
use common::*;

/// The entry the roles run; with no role in the environment it does nothing.
#[test]
fn role_entry() {
    role_main();
}

fn running(query: Query) -> agentspace_keeper_api::ProcessInfo {
    match query {
        Query::Running(info) => info,
        Query::Gone => panic!("The process runs, and the query says it is gone."),
        Query::CannotOpen(code) => {
            panic!("The process runs, and the query cannot open it ({code}).")
        }
    }
}

#[test]
fn query_of_a_live_process_gives_the_creation_time_as_the_kernel_reads_it() {
    let sb = Sandbox::new("q-live");
    let before = now_unix_ms();
    let child = spawn_role(&sb, "sleep", "one");
    let proc = Proc::open(child.pid());
    let after = now_unix_ms();

    let info = running(processes().query(child.pid()));

    // The form: UTC, to the millisecond, `YYYY-MM-DDTHH:MM:SS.fffZ`.
    let text = &info.process_started_at;
    assert_eq!(text.len(), 24, "The creation time has the fixed length");
    let bytes = text.as_bytes();
    for (i, expect) in [
        (4, b'-'),
        (7, b'-'),
        (10, b'T'),
        (13, b':'),
        (16, b':'),
        (19, b'.'),
        (23, b'Z'),
    ] {
        assert_eq!(bytes[i], expect, "The creation time has its separators");
    }
    // The value: exactly what the kernel reports for the process, truncated to the millisecond.
    assert_eq!(text, &proc.created_string());
    // And close to when the test started it.
    let created = proc.created_unix_ms();
    assert!(
        created >= before - 2_000 && created <= after + 2_000,
        "The creation time is within two seconds of the start"
    );
}

#[test]
fn query_of_a_live_process_reads_its_command_line_back() {
    let sb = Sandbox::new("q-cmd");
    let child = spawn_role(&sb, "sleep", "one");
    let _proc = Proc::open(child.pid());

    let info = running(processes().query(child.pid()));

    let line = info
        .command_line
        .expect("The command line of a process of the same user reads");
    assert!(
        line.contains("role_entry"),
        "The command line carries the test's own argument"
    );
    assert!(
        line.contains("--exact"),
        "The command line carries every argument"
    );
    assert!(
        line.contains("--nocapture"),
        "The command line carries every argument"
    );
    assert!(
        line.starts_with('"'),
        "The command line is the one Windows holds, with the program quoted first"
    );
}

#[test]
fn query_of_the_tests_own_process_runs() {
    let info = running(processes().query(std::process::id()));
    assert!(info.command_line.is_some());
    assert_eq!(info.process_started_at.len(), 24);
}

#[test]
fn query_of_the_same_process_twice_gives_the_same_creation_time() {
    let sb = Sandbox::new("q-twice");
    let child = spawn_role(&sb, "sleep", "one");
    let _proc = Proc::open(child.pid());

    let first = running(processes().query(child.pid())).process_started_at;
    std::thread::sleep(Duration::from_millis(150));
    let second = running(processes().query(child.pid())).process_started_at;

    assert_eq!(first, second, "A process has one creation time");
}

#[test]
fn query_of_a_process_that_has_gone_says_gone() {
    let sb = Sandbox::new("q-gone");
    let mut spawned = spawn_role(&sb, "sleep", "one");
    let pid = spawned.pid();
    wait_for_pid(&sb, "one");
    spawned
        .child
        .kill()
        .expect("The test's own child can be killed");
    spawned
        .child
        .wait()
        .expect("The test's own child can be reaped");
    // Dropping the guard drops the child's handle, which frees the process object.
    drop(spawned);

    // A handle someone else holds can keep a dead process visible for a moment.
    wait_for(
        "the process to be gone for a query",
        Duration::from_secs(10),
        || is_gone_by_query(pid),
    );
    assert_eq!(processes().query(pid), Query::Gone);
}

#[test]
fn query_of_a_process_that_exited_but_whose_handle_is_still_open_says_gone() {
    // keeper.md, Trusting the record: "A process with the recorded PID is running". A process
    // that has exited is not running, even while some handle to it keeps its object around.
    let sb = Sandbox::new("q-exited");
    let mut spawned = spawn_role(&sb, "sleep", "one");
    let pid = spawned.pid();
    let proc = Proc::open(pid);
    wait_for_pid(&sb, "one");
    proc.terminate();
    assert!(proc.wait_exit(Duration::from_secs(5)), "The process ends");
    // The test still holds two handles to it, its child's and its own.
    let answer = processes().query(pid);
    assert_eq!(answer, Query::Gone, "An exited process is not running");
    let _ = spawned.child.wait();
}

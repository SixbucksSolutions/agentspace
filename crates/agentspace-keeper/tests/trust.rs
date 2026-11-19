//! Trusting the record (docs/keeper.md, A pass: Trusting the record).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **The pass trusts the
//! recorded holder only when all of these hold:** (1) the record can be read and is valid, which
//! the record store applies by giving none for a record that is not; (2) a process with the
//! recorded PID is running and can be opened to query it; (3) its creation time equals the
//! record's `process_started_at` exactly; (4) its command line is the holder command for the
//! pass's `-SshExe` and the record's alias, by the matcher. **A record that fails any of these is
//! removed, and the process it names, if any, MUST be left running and untouched.** The removal
//! has no log line of its own; the pass goes on as with no holder.

mod common;

use agentspace_keeper_api::{ProcessInfo, Query};
use common::*;

fn rec() -> agentspace_keeper_api::HolderRecord {
    holder_for(4242, ID_A)
}

/// A running process that is the recorded one, but for the one field the test changes.
fn running(
    rec: &agentspace_keeper_api::HolderRecord,
    started: &str,
    command: Option<String>,
) -> Query {
    let _ = rec;
    Query::Running(ProcessInfo {
        process_started_at: started.to_string(),
        command_line: command,
    })
}

fn assert_dropped_and_left_alone(h: &Harness) {
    let e = h.events();
    assert!(
        has(&e, "query:4242"),
        "The recorded PID was never queried: {e:?}"
    );
    assert!(
        has(&e, "store:remove"),
        "The untrusted record was not removed: {e:?}"
    );
    assert_eq!(h.record(), None, "The untrusted record is still there");
    assert!(
        !has_prefix(&e, "stop:"),
        "The process of an untrusted record MUST be left running and untouched: {e:?}"
    );
    assert_eq!(h.stops(), 0);
    assert_lock_released_last(&e);
}

#[test]
fn a_record_whose_every_check_holds_is_trusted() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.pass(NOW);
    assert_eq!(
        h.log(),
        line(NOW, "kept", ID_A, &alias_of(ID_A), "4242", None)
    );
    assert!(!has(&h.events(), "store:remove"));
}

#[test]
fn no_record_means_no_query_and_goes_on_as_with_no_holder() {
    let h = Harness::new();
    h.pass(NOW);
    let e = h.events();
    assert!(!has_prefix(&e, "query:"), "{e:?}");
    assert!(!has_prefix(&e, "stop:"), "{e:?}");
    assert_eq!(h.log(), idle_line(NOW));
}

#[test]
fn a_record_is_looked_up_by_its_recorded_pid() {
    let h = Harness::new();
    let mut r = rec();
    r.pid = 777;
    h.trust(&r);
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.pass(NOW);
    assert!(has(&h.events(), "query:777"));
    assert_eq!(
        h.log(),
        line(NOW, "kept", ID_A, &alias_of(ID_A), "777", None)
    );
}

#[test]
fn a_process_that_is_gone_drops_the_record_and_there_is_nothing_to_leave() {
    let h = Harness::new();
    h.record_with_query(&rec(), Query::Gone);
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_dropped_and_left_alone(&h);
    // The removal has no line of its own: the idle line is the only sign.
    assert_eq!(h.log(), idle_line(NOW));
}

#[test]
fn a_process_that_runs_but_cannot_be_opened_to_query_fails_item_2_and_is_left_running() {
    let h = Harness::new();
    h.record_with_query(&rec(), Query::CannotOpen(5));
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.start_echo(5151);
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    let e = h.events();
    assert!(has(&e, "store:remove"), "{e:?}");
    assert!(!has_prefix(&e, "stop:"), "{e:?}");
    // The pass went on as with no holder: a ready lease gets a start.
    assert_eq!(
        h.log(),
        line(NOW, "started", ID_A, &alias_of(ID_A), "5151", None)
    );
}

#[test]
fn a_creation_time_that_differs_by_one_millisecond_is_another_process() {
    // PID reuse: the PID with the start time is the holder's identity.
    for started in [
        "2026-11-19T11:30:00.124Z",
        "2026-11-19T11:30:00.122Z",
        "2026-11-19T11:30:01.123Z",
        "2026-11-19T11:30:00.123z",
    ] {
        let h = Harness::new();
        let r = rec();
        let command = FakeMatcher::command_for(&h.ssh, &r.alias);
        h.record_with_query(&r, running(&r, started, Some(command)));
        let ran = h.pass(NOW);
        assert_eq!(ran.exit, 0);
        assert_dropped_and_left_alone(&h);
        assert_eq!(h.log(), idle_line(NOW), "for {started}");
    }
}

#[test]
fn a_command_line_that_is_not_the_holder_command_is_untrusted() {
    for command in [
        "notepad.exe",
        "",
        "-N -o ExitOnForwardFailure=yes agentspace-demo-3f2a9c1e",
    ] {
        let h = Harness::new();
        let r = rec();
        h.record_with_query(
            &r,
            running(&r, &r.process_started_at, Some(command.to_string())),
        );
        let ran = h.pass(NOW);
        assert_eq!(ran.exit, 0);
        assert_dropped_and_left_alone(&h);
        assert_eq!(h.log(), idle_line(NOW), "for {command:?}");
    }
}

#[test]
fn a_command_line_that_could_not_be_read_is_untrusted() {
    // The pin gives `command_line: None` for one that could not be read. Item 4 of Trusting the
    // record needs the command line to be the holder command, which one that cannot be read
    // cannot be shown to be, so the record fails it.
    let h = Harness::new();
    let r = rec();
    h.record_with_query(&r, running(&r, &r.process_started_at, None));
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0);
    assert_dropped_and_left_alone(&h);
    assert_eq!(h.log(), idle_line(NOW));
}

#[test]
fn the_matcher_is_asked_with_the_passs_ssh_exe_and_the_records_alias() {
    let h = Harness::new();
    let mut r = rec();
    // The record's alias is not the lease's: the matcher MUST be given the record's.
    r.alias = "agentspace-demo-aaaaaaaa".to_string();
    h.trust(&r);
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.pass(NOW);
    let calls = h.fakes.matcher.calls.borrow();
    assert!(!calls.is_empty(), "The command line was never matched");
    for (command, ssh, alias) in calls.iter() {
        assert_eq!(
            command,
            &FakeMatcher::command_for(&h.ssh, "agentspace-demo-aaaaaaaa")
        );
        assert_eq!(ssh, &h.ssh, "The pass's -SshExe");
        assert_eq!(alias, "agentspace-demo-aaaaaaaa", "The record's alias");
    }
}

#[test]
fn a_holder_started_with_another_ssh_exe_is_untrusted_and_orphaned_not_stopped() {
    // The same holder command, but for the ssh program of an earlier run.
    let h = Harness::new();
    let r = rec();
    let other_ssh = h.tmp.path().join("other-ssh");
    h.record_with_query(
        &r,
        running(
            &r,
            &r.process_started_at,
            Some(FakeMatcher::command_for(&other_ssh, &r.alias)),
        ),
    );
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.start_echo(5151);
    h.pass(NOW);
    assert!(has(&h.events(), "store:remove"));
    assert_eq!(h.stops(), 0, "The orphan MUST NOT be stopped");
    // Only this line is the sign that a holder was lost.
    assert_eq!(
        h.log(),
        line(NOW, "started", ID_A, &alias_of(ID_A), "5151", None)
    );
}

#[test]
fn an_untrusted_record_is_removed_before_the_next_holder_is_started_and_recorded() {
    let h = Harness::new();
    h.record_with_query(&rec(), Query::Gone);
    h.write_leases(vec![ready(ID_B, LATER)]);
    h.start_echo(5151);
    h.pass(NOW);
    let e = h.events();
    assert!(
        pos(&e, "store:remove") < pos(&e, "start:9b1d0c77-2222-4333-9444-555566667777"),
        "{e:?}"
    );
    assert!(
        pos(&e, "start:9b1d0c77-2222-4333-9444-555566667777") < pos(&e, "store:write"),
        "{e:?}"
    );
    assert_eq!(
        h.record().map(|r| (r.pid, r.lease_id)),
        Some((5151, ID_B.to_string()))
    );
}

#[test]
fn an_untrusted_record_with_no_ready_lease_leaves_no_record_and_one_idle_line() {
    let h = Harness::new();
    h.record_with_query(&rec(), Query::Gone);
    h.write_leases(vec![released(ID_A, LATER)]);
    h.pass(NOW);
    assert_eq!(h.record(), None);
    assert_eq!(h.log(), idle_line(NOW));
    assert_eq!(h.log().lines().count(), 1);
}

#[test]
fn a_record_whose_process_failed_a_check_is_never_stopped_even_when_its_lease_is_gone() {
    // A holder whose record was dropped is never stopped by any pass.
    let h = Harness::new();
    h.record_with_query(&rec(), Query::Gone);
    h.write_leases(vec![]);
    h.pass(NOW);
    h.pass(NOW);
    assert_eq!(h.stops(), 0);
}

//! The act table's trusted-holder rows and the stop table (docs/keeper.md, A pass: The order of a
//! pass, step 7; Keeping a holder; Stopping a holder).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **A trusted holder whose
//! record's lease is ready, by ID compared without regard to ASCII case (state.md, The holder
//! record; architect, 2026-11-19), and whose ready lease's `ssh.host_alias` equals the recorded
//! alias exactly, is kept.** **A trusted holder whose lease is not ready, or
//! whose lease's alias is no longer the recorded one, is stopped, and the pass starts nothing.**
//! `stopped` MUST be logged only when the pass no longer finds the recorded holder, and the record
//! MUST be kept while a look still finds it running, so the next pass tries again.

mod common;

use agentspace_keeper_api::{HolderRecord, Started, Stopped};
use common::*;

fn rec() -> HolderRecord {
    holder_for(4242, ID_A)
}

fn kept_line(r: &HolderRecord) -> String {
    line(NOW, "kept", &r.lease_id, &r.alias, &r.pid.to_string(), None)
}

fn stopped_line(r: &HolderRecord) -> String {
    line(
        NOW,
        "stopped",
        &r.lease_id,
        &r.alias,
        &r.pid.to_string(),
        None,
    )
}

fn failed_stop_line(r: &HolderRecord, reason: &str) -> String {
    line(
        NOW,
        "failed",
        &r.lease_id,
        &r.alias,
        &r.pid.to_string(),
        Some(reason),
    )
}

// ---- Row 1: keep ----

#[test]
fn a_trusted_holder_whose_lease_is_ready_under_the_recorded_alias_is_kept() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![ready(ID_A, LATER)]);
    let before = h.log_bytes();
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0);
    assert_eq!(h.log(), kept_line(&r));
    assert_stdout_is_log_since(&h, &before, &ran);
    let e = h.events();
    for none in ["store:write", "store:remove"] {
        assert!(
            !has(&e, none),
            "A kept holder changes nothing, but {none} happened: {e:?}"
        );
    }
    assert!(
        !has_prefix(&e, "stop:") && !has_prefix(&e, "start:"),
        "{e:?}"
    );
    assert_eq!(h.record(), Some(r));
    assert_lock_released_last(&e);
}

#[test]
fn a_holder_already_running_through_a_ready_lease_is_kept_even_when_another_expires_later() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![
        ready(ID_A, LATER),
        ready(ID_B, LATEST),
        ready(ID_C, LATEST),
    ]);
    h.start_echo(5151);
    h.pass(NOW);
    assert_eq!(h.log(), kept_line(&r));
    assert_eq!(h.starts(), 0);
    assert_eq!(h.stops(), 0);
}

#[test]
fn the_kept_line_names_the_records_lease_alias_and_pid() {
    let h = Harness::new();
    let r = holder_for(31337, ID_B);
    h.trust(&r);
    h.write_leases(vec![ready(ID_B, LATER)]);
    h.pass(NOW);
    assert_eq!(
        h.log(),
        "2026-11-19T12:00:00Z kept lease=9b1d0c77-2222-4333-9444-555566667777 alias=agentspace-demo-9b1d0c77 pid=31337 host=swimcoach\n"
    );
}

// ---- Row 2: stop, and start nothing ----

fn assert_stopped_and_nothing_started(h: &Harness, r: &HolderRecord) {
    let ran_exit_ok = h.stops() == 1;
    assert!(
        ran_exit_ok,
        "The holder was not stopped exactly once: {:?}",
        h.events()
    );
    let (stopped, ssh) = h.fakes.processes.stops.borrow()[0].clone();
    assert_eq!(&stopped, r, "The stop is for the recorded holder");
    assert_eq!(ssh, h.ssh, "The stop is given the pass's -SshExe");
    assert_eq!(
        h.starts(),
        0,
        "A pass that stops a holder MUST NOT start one"
    );
    assert!(!has(&h.events(), "store:write"));
}

#[test]
fn a_trusted_holder_whose_lease_is_not_ready_is_stopped() {
    let cases: Vec<(&str, Vec<serde_json::Value>)> = vec![
        ("released", vec![released(ID_A, LATER)]),
        ("expired at the pass time", vec![ready(ID_A, NOW)]),
        (
            "expired before it",
            vec![ready(ID_A, "2026-11-19T11:00:00Z")],
        ),
        ("pending", vec![pending(ID_A)]),
        ("unsatisfiable", vec![unsatisfiable(ID_A)]),
        ("gone from the file", vec![ready(ID_B, LATER)]),
        ("no leases at all", vec![]),
    ];
    for (why, leases) in cases {
        let h = Harness::new();
        let r = rec();
        h.trust(&r);
        h.write_leases(leases);
        let before = h.log_bytes();
        let ran = h.pass(NOW);
        assert_eq!(ran.exit, 0, "{why}");
        assert_eq!(h.log(), stopped_line(&r), "{why}");
        assert_stdout_is_log_since(&h, &before, &ran);
        assert_stopped_and_nothing_started(&h, &r);
        assert_eq!(
            h.record(),
            None,
            "{why}: the stopped holder's record is removed"
        );
        assert_lock_released_last(&h.events());
    }
}

#[test]
fn a_holder_whose_lease_is_not_ready_is_stopped_and_nothing_is_started_even_with_another_ready_lease()
 {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![released(ID_A, LATER), ready(ID_B, LATEST)]);
    h.start_echo(5151);
    h.pass(NOW);
    assert_eq!(h.log(), stopped_line(&r));
    assert_stopped_and_nothing_started(&h, &r);
}

#[test]
fn the_next_pass_not_the_same_one_starts_a_holder_for_another_ready_lease() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![released(ID_A, LATER), ready(ID_B, LATEST)]);
    h.start_echo(5151);
    h.pass(NOW);
    h.pass("2026-11-19T12:00:30Z");
    assert_eq!(
        h.log(),
        format!(
            "{}{}",
            stopped_line(&r),
            line(
                "2026-11-19T12:00:30Z",
                "started",
                ID_B,
                &alias_of(ID_B),
                "5151",
                None
            )
        )
    );
}

#[test]
fn a_holder_whose_leases_alias_changed_is_stopped_under_the_recorded_alias() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![ready_with_alias(
        ID_A,
        LATER,
        "agentspace-other-3f2a9c1e",
    )]);
    h.start_echo(5151);
    h.pass(NOW);
    assert_eq!(h.log(), stopped_line(&r));
    assert_stopped_and_nothing_started(&h, &r);
}

#[test]
fn the_alias_is_compared_exactly_case_included() {
    let h = Harness::new();
    let mut r = rec();
    r.alias = "Agentspace-demo-3f2a9c1e".to_string();
    h.trust(&r);
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.pass(NOW);
    assert_eq!(h.log(), stopped_line(&r));
}

#[test]
fn the_lease_id_is_compared_without_regard_to_ascii_case() {
    // The reader MAY accept a record whose lease_id is in any case and keeps it as stored, and the
    // keeper compares it with a lease's ID without regard to ASCII case (state.md, The holder
    // record; architect, 2026-11-19, for E0.06-T01-S06). So an upper-case ID names the lease a
    // state file stores in lower case, and its holder is kept. This test expected `stopped`
    // before that ruling. The line names the lease as the record stores it.
    let h = Harness::new();
    let mut r = rec();
    r.lease_id = ID_A.to_uppercase();
    h.trust(&r);
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.pass(NOW);
    assert_eq!(h.log(), kept_line(&r));
    assert_eq!(h.stops(), 0);
    assert_eq!(h.starts(), 0);
    assert_eq!(h.record(), Some(r));
}

#[test]
fn a_lease_id_that_differs_by_more_than_case_names_no_lease() {
    // Without regard to case is not without regard to the ID: ID_D differs from ID_C in one digit.
    let h = Harness::new();
    let mut r = holder_for(4242, ID_C);
    r.lease_id = ID_C.to_uppercase();
    h.trust(&r);
    h.write_leases(vec![ready_with_alias(ID_D, LATER, &alias_of(ID_C))]);
    h.pass(NOW);
    assert_eq!(h.log(), stopped_line(&r));
    assert_eq!(h.starts(), 0);
}

#[test]
fn a_ready_lease_of_another_id_with_the_recorded_alias_does_not_keep_the_holder() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![ready_with_alias(ID_B, LATER, &alias_of(ID_A))]);
    h.pass(NOW);
    assert_eq!(h.log(), stopped_line(&r));
}

// ---- The stop table: one row each ----

struct Row {
    result: Stopped,
    record_removed: bool,
    action: &'static str,
    reason: Option<&'static str>,
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            result: Stopped::All,
            record_removed: true,
            action: "stopped",
            reason: None,
        },
        Row {
            result: Stopped::Gone,
            record_removed: true,
            action: "stopped",
            reason: None,
        },
        Row {
            result: Stopped::CannotOpenToStop(5),
            record_removed: false,
            action: "failed",
            reason: Some("The holder could not be opened to stop it: Windows error 5."),
        },
        Row {
            result: Stopped::NotStopped("it was not the recorded process".into()),
            record_removed: false,
            action: "failed",
            reason: Some(
                "The holder could not be stopped: it was not the recorded process, but the holder is still running.",
            ),
        },
        Row {
            result: Stopped::Error("the handle was refused".into()),
            record_removed: false,
            action: "failed",
            reason: Some("The holder could not be stopped: the handle was refused"),
        },
        Row {
            result: Stopped::StillRunning,
            record_removed: false,
            action: "failed",
            reason: Some("The holder is still running after it was stopped."),
        },
        Row {
            result: Stopped::TreeLeft(vec![101, 202]),
            record_removed: true,
            action: "failed",
            reason: Some("The holder stopped, but these processes of its tree did not: 101,202."),
        },
    ]
}

#[test]
fn every_row_of_the_stop_table_logs_its_line_and_does_what_it_says_to_the_record() {
    for row in rows() {
        let h = Harness::new();
        let r = rec();
        h.trust(&r);
        h.write_leases(vec![released(ID_A, LATER), ready(ID_B, LATEST)]);
        h.start_echo(5151);
        h.set_stop(row.result.clone());
        let before = h.log_bytes();
        let ran = h.pass(NOW);
        let what = format!("{:?}", row.result);
        // A failed start or stop exits 0: the pass ran and logged what happened.
        assert_eq!(ran.exit, 0, "{what}");
        assert_eq!(
            h.log(),
            line(
                NOW,
                row.action,
                &r.lease_id,
                &r.alias,
                &r.pid.to_string(),
                row.reason
            ),
            "{what}"
        );
        assert_stdout_is_log_since(&h, &before, &ran);
        assert_eq!(
            h.record().is_none(),
            row.record_removed,
            "Record for {what}"
        );
        assert_eq!(h.stops(), 1, "{what}");
        assert_eq!(
            h.starts(),
            0,
            "A pass that tries to stop MUST NOT start ({what})"
        );
        assert_lock_released_last(&h.events());
    }
}

#[test]
fn a_kept_record_means_the_next_pass_tries_the_stop_again() {
    for result in [
        Stopped::CannotOpenToStop(5),
        Stopped::NotStopped("why".into()),
        Stopped::Error("e".into()),
        Stopped::StillRunning,
    ] {
        let h = Harness::new();
        let r = rec();
        h.trust(&r);
        h.write_leases(vec![released(ID_A, LATER), ready(ID_B, LATEST)]);
        h.start_echo(5151);
        h.set_stop(result.clone());
        h.pass(NOW);
        h.pass("2026-11-19T12:00:30Z");
        assert_eq!(h.stops(), 2, "The stop was not tried again for {result:?}");
        assert_eq!(
            h.starts(),
            0,
            "A holder was started while the old one still runs ({result:?})"
        );
        assert_eq!(h.record(), Some(r));
    }
}

#[test]
fn a_removed_record_means_the_next_pass_finds_no_holder() {
    for result in [Stopped::All, Stopped::Gone, Stopped::TreeLeft(vec![9])] {
        let h = Harness::new();
        let r = rec();
        h.trust(&r);
        h.write_leases(vec![released(ID_A, LATER)]);
        h.set_stop(result.clone());
        h.pass(NOW);
        h.pass("2026-11-19T12:00:30Z");
        assert_eq!(h.stops(), 1, "{result:?}");
        assert!(
            h.log().ends_with(&idle_line("2026-11-19T12:00:30Z")),
            "{}",
            h.log()
        );
    }
}

#[test]
fn a_single_process_left_in_the_tree_is_one_pid_with_no_comma() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![]);
    h.set_stop(Stopped::TreeLeft(vec![77]));
    h.pass(NOW);
    assert_eq!(
        h.log(),
        failed_stop_line(
            &r,
            "The holder stopped, but these processes of its tree did not: 77."
        )
    );
}

#[test]
fn a_reason_with_control_characters_is_squeezed_to_one_space_per_run_and_trimmed() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.write_leases(vec![]);
    h.set_stop(Stopped::Error(
        "line one\r\n\tline two\u{7f}\u{0}end \n".into(),
    ));
    h.pass(NOW);
    assert_eq!(
        h.log(),
        failed_stop_line(&r, "The holder could not be stopped: line one line two end")
    );
}

#[test]
fn a_stop_is_only_asked_of_a_trusted_holder() {
    // The matcher decides it is no holder: nothing is stopped, whatever the lease says.
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.fakes.processes.queries.borrow_mut().insert(
        r.pid,
        agentspace_keeper_api::Query::Running(agentspace_keeper_api::ProcessInfo {
            process_started_at: r.process_started_at.clone(),
            command_line: Some("something else".into()),
        }),
    );
    h.write_leases(vec![]);
    h.pass(NOW);
    assert_eq!(h.stops(), 0);
    assert_eq!(h.log(), idle_line(NOW));
}

// ---- Rows 4 and 5: idle and start, once the holder rows are out of the way ----

#[test]
fn with_no_trusted_holder_and_no_ready_lease_the_pass_is_idle_and_changes_nothing() {
    let h = Harness::new();
    h.write_leases(vec![pending(ID_A), released(ID_B, LATER)]);
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0);
    assert_eq!(h.log(), "2026-11-19T12:00:00Z idle lease=- alias=- pid=-\n");
    let e = h.events();
    assert!(!has(&e, "store:write") && !has(&e, "store:remove"), "{e:?}");
    assert!(
        !has_prefix(&e, "stop:") && !has_prefix(&e, "start:"),
        "{e:?}"
    );
}

#[test]
fn a_start_row_needs_no_trusted_holder_a_kept_holder_wins_over_a_ready_lease() {
    let h = Harness::new();
    let r = rec();
    h.trust(&r);
    h.set_start(Started::Failed("must not be called".into()));
    h.write_leases(vec![ready(ID_A, LATER), ready(ID_B, LATEST)]);
    h.pass(NOW);
    assert_eq!(h.starts(), 0);
}

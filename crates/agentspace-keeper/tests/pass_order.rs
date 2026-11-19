//! The order of a pass, up to and including reading the lease state (docs/keeper.md, A pass: The
//! order of a pass; Which leases are ready; Locks).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Steps: take
//! `board-holder.lock` (a pass that cannot logs `skipped` and looks at nothing else), check the
//! matcher against its own cases (a failure ends the pass with `failed` and exit 1, trusting
//! nothing), rotate, read the lease state (a failure logs `failed` and exit 1, and leaves the holder
//! and the record as they are), then the rest; and release the lock whatever happened.
//! **The pass time is fixed when the pass begins, before the lock is taken.**

mod common;

use agentspace_keeper_api::Query;
use common::*;

// ---- Step 1: the pass lock ----

#[test]
fn the_pass_lock_is_taken_first_and_nothing_is_looked_at_before_it() {
    let h = Harness::new();
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0, "stderr was {:?}", r.stderr);
    let e = h.events();
    let lock = pos(&e, "lock:pass");
    // Only a read of the clock for the pass time MAY come before it.
    assert!(
        e[..lock].iter().all(|x| x == "clock"),
        "Something was looked at before the pass lock: {e:?}"
    );
    assert!(lock < pos(&e, "self_check"));
    assert!(lock < pos(&e, "store:read"));
    assert_lock_released_last(&e);
}

#[test]
fn the_pass_lock_is_asked_for_the_state_folder() {
    let h = Harness::new();
    h.pass(NOW);
    assert_eq!(
        h.fakes.locks.pass_dirs.borrow().as_slice(),
        std::slice::from_ref(&h.state)
    );
}

#[test]
fn a_one_pass_run_never_takes_the_keeper_lock() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.set_start(agentspace_keeper_api::Started::Holder(holder_for(
        4242, ID_A,
    )));
    h.pass(NOW);
    assert!(!has(&h.events(), "lock:keeper"), "{:?}", h.events());
}

#[test]
fn a_pass_that_cannot_take_the_lock_logs_skipped_and_looks_at_nothing_else() {
    let h = Harness::new();
    *h.fakes.locks.pass_mode.borrow_mut() = PassMode::Skipped;
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.trust(&holder_for(4242, ID_B));
    let before = h.log_bytes();
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_eq!(h.log(), line(NOW, "skipped", "-", "-", "-", Some("locked")));
    assert_stdout_is_log_since(&h, &before, &r);
    let e = h.events();
    for later in ["self_check", "store:read", "store:write", "store:remove"] {
        assert!(
            !has(&e, later),
            "{later} happened after a skipped lock: {e:?}"
        );
    }
    for later in ["query:", "stop:", "start:"] {
        assert!(
            !has_prefix(&e, later),
            "{later} happened after a skipped lock: {e:?}"
        );
    }
    assert!(
        !has(&e, "release:pass"),
        "Nothing was held, so nothing is released: {e:?}"
    );
}

#[test]
fn a_skipped_line_never_reads_as_idle() {
    let h = Harness::new();
    *h.fakes.locks.pass_mode.borrow_mut() = PassMode::Skipped;
    h.pass(NOW);
    assert!(!h.log().contains(" idle "));
}

#[test]
fn a_skipped_pass_does_not_rotate_the_holders_log() {
    let h = Harness::new();
    *h.fakes.locks.pass_mode.borrow_mut() = PassMode::Skipped;
    std::fs::write(h.ssh_log_path(), vec![b'x'; 1_048_577]).unwrap();
    h.pass(NOW);
    assert!(h.ssh_log_path().exists());
    assert!(!h.ssh_log_1_path().exists());
}

#[test]
fn a_skipped_line_carries_the_pass_time_not_the_time_the_wait_ended() {
    // The pass time is fixed before the lock is taken, and a skipped line is written about 10
    // seconds later, when the lock wait ends.
    let h = Harness::new();
    *h.fakes.locks.pass_mode.borrow_mut() = PassMode::Skipped;
    h.fakes.locks.wait_ms.set(10_000);
    let r = h.pass_on_clock();
    assert_eq!(r.exit, 0);
    assert_eq!(h.log(), line(NOW, "skipped", "-", "-", "-", Some("locked")));
    let e = h.events();
    assert!(
        pos(&e, "clock") < pos(&e, "lock:pass"),
        "The pass time is read before the lock: {e:?}"
    );
}

#[test]
fn a_lock_that_fails_for_another_reason_fails_the_pass_with_exit_1() {
    let h = Harness::new();
    *h.fakes.locks.pass_mode.borrow_mut() = PassMode::Failed("Access is denied.".into());
    h.write_leases(vec![ready(ID_A, LATER)]);
    let before = h.log_bytes();
    let r = h.pass(NOW);
    assert_eq!(r.exit, 1);
    assert_eq!(
        h.log(),
        failed_line(NOW, "The pass stopped on an error: Access is denied.")
    );
    assert_stdout_is_log_since(&h, &before, &r);
    let e = h.events();
    assert!(!has(&e, "self_check"));
    assert!(!has_prefix(&e, "start:"));
}

// ---- Step 2: the matcher's self-check ----

#[test]
fn the_matcher_checks_itself_after_the_lock_and_before_anything_is_read() {
    let h = Harness::new();
    h.trust(&holder_for(4242, ID_A));
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.pass(NOW);
    let e = h.events();
    assert!(pos(&e, "lock:pass") < pos(&e, "self_check"));
    assert!(pos(&e, "self_check") < pos(&e, "store:read"));
    assert!(pos(&e, "self_check") < pos(&e, "query:4242"));
}

#[test]
fn the_matcher_checks_itself_at_the_start_of_every_pass() {
    let h = Harness::new();
    h.pass(NOW);
    h.pass(NOW);
    h.pass(NOW);
    assert_eq!(h.events().iter().filter(|e| *e == "self_check").count(), 3);
}

#[test]
fn a_matcher_that_fails_ends_the_pass_with_a_failed_line_and_exit_1_and_trusts_nothing() {
    let h = Harness::new();
    *h.fakes.matcher.self_check_result.borrow_mut() = Err("case one; case two".into());
    h.write_leases(vec![ready(ID_A, LATER)]);
    let rec = holder_for(4242, ID_A);
    h.trust(&rec);
    let before = h.log_bytes();
    let r = h.pass(NOW);
    assert_eq!(r.exit, 1);
    // `Err(text)` from the matcher is the failing cases alone (`<case>; <case>`), and the pass
    // adds `The holder command matcher failed its own checks: ` before them (architect,
    // 2026-11-19, for E0.06-T01-S06).
    assert_eq!(
        h.log(),
        failed_line(
            NOW,
            "The pass stopped on an error: The holder command matcher failed its own checks: case one; case two"
        )
    );
    assert_stdout_is_log_since(&h, &before, &r);
    let e = h.events();
    assert!(!has_prefix(&e, "query:"), "It trusted something: {e:?}");
    assert!(!has_prefix(&e, "stop:"), "It stopped something: {e:?}");
    assert!(!has_prefix(&e, "start:"), "It started something: {e:?}");
    assert!(!has(&e, "store:remove") && !has(&e, "store:write"), "{e:?}");
    assert_eq!(h.record(), Some(rec), "The record is left as it was");
    assert_lock_released_last(&e);
}

#[test]
fn a_failed_self_check_does_not_rotate() {
    // Rotation is step 3, after the self-check of step 2.
    let h = Harness::new();
    *h.fakes.matcher.self_check_result.borrow_mut() = Err("c".into());
    std::fs::write(h.ssh_log_path(), vec![b'x'; 1_048_577]).unwrap();
    h.pass(NOW);
    assert!(h.ssh_log_path().exists());
    assert!(!h.ssh_log_1_path().exists());
}

// ---- Step 4: the lease state ----

#[test]
fn a_missing_state_file_is_no_leases_not_an_error() {
    let h = Harness::new();
    assert!(!h.leases_path().exists());
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_eq!(h.log(), idle_line(NOW));
}

#[test]
fn a_state_file_with_no_leases_is_idle() {
    let h = Harness::new();
    h.write_leases(vec![]);
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_eq!(h.log(), idle_line(NOW));
}

const UNREADABLE: &str = "Lease state could not be read: the state file is not valid";

fn assert_unreadable_fails_the_pass(h: &Harness, reason: &str) {
    let rec = holder_for(4242, ID_A);
    h.trust(&rec);
    h.set_start(agentspace_keeper_api::Started::Holder(holder_for(
        5151, ID_B,
    )));
    let before = h.log_bytes();
    let r = h.pass(NOW);
    assert_eq!(r.exit, 1, "stderr was {:?}", r.stderr);
    assert_eq!(h.log(), failed_line(NOW, reason));
    assert_stdout_is_log_since(h, &before, &r);
    let e = h.events();
    assert!(!has_prefix(&e, "query:"), "The record was looked at: {e:?}");
    assert!(!has_prefix(&e, "stop:"), "The holder was stopped: {e:?}");
    assert!(!has_prefix(&e, "start:"), "A holder was started: {e:?}");
    assert!(
        !has(&e, "store:remove") && !has(&e, "store:write"),
        "The record changed: {e:?}"
    );
    assert_eq!(h.record(), Some(rec), "The record is left as it was");
    assert_lock_released_last(&e);
}

#[test]
fn a_file_that_is_not_json_fails_the_pass_and_leaves_the_holder_and_the_record() {
    let h = Harness::new();
    h.write_leases_raw(b"{ not json");
    assert_unreadable_fails_the_pass(&h, UNREADABLE);
}

#[test]
fn a_file_that_does_not_pass_its_schema_fails_the_pass() {
    for bytes in [
        &br#"{"version":1,"leases":[]}"#[..],
        br#"{"version":0}"#,
        br#"{"version":0,"leases":[{"id":"not-an-id"}]}"#,
        br#"[]"#,
        b"",
        &[0xff, 0xfe, 0x00][..],
    ] {
        let h = Harness::new();
        h.write_leases_raw(bytes);
        assert_unreadable_fails_the_pass(&h, UNREADABLE);
    }
}

#[test]
fn two_records_under_one_id_make_the_file_not_valid_and_no_lease_is_picked() {
    // Gaps FA-19 and FB-18: the schema cannot refuse it, and the keeper reads it as the lease
    // commands do, as a file that is not valid. It never picks a lease from such a file, even a
    // ready one, and even though the other record is a different lease.
    let h = Harness::new();
    h.write_leases(vec![
        ready(ID_A, LATER),
        ready(ID_B, LATEST),
        released(ID_A, LATER),
    ]);
    assert_unreadable_fails_the_pass(&h, UNREADABLE);
}

#[test]
fn two_ready_records_under_one_id_are_not_valid_either() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER), ready(ID_A, LATER)]);
    assert_unreadable_fails_the_pass(&h, UNREADABLE);
}

#[test]
fn a_member_named_twice_or_a_time_that_names_no_instant_makes_the_file_not_valid() {
    let h = Harness::new();
    let mut text = String::from_utf8(file_bytes(vec![ready(ID_A, LATER)])).unwrap();
    text = text.replacen(
        "\"project\":\"demo\"",
        "\"project\":\"demo\",\"project\":\"demo\"",
        1,
    );
    h.write_leases_raw(text.as_bytes());
    assert_unreadable_fails_the_pass(&h, UNREADABLE);

    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, "2026-02-30T00:00:00Z")]);
    assert_unreadable_fails_the_pass(&h, UNREADABLE);
}

#[test]
fn something_at_the_state_file_that_is_not_a_file_is_not_a_missing_file() {
    let h = Harness::new();
    std::fs::create_dir_all(h.leases_path()).unwrap();
    h.trust(&holder_for(4242, ID_A));
    let r = h.pass(NOW);
    assert_eq!(r.exit, 1);
    let log = h.log();
    assert!(
        log.starts_with(&format!(
            "{NOW} failed lease=- alias=- pid=- reason=Lease state could not be read: "
        )),
        "{log:?}"
    );
    assert!(log.ends_with('\n') && log.lines().count() == 1);
    assert!(!has_prefix(&h.events(), "stop:"));
    assert!(!has_prefix(&h.events(), "start:"));
}

#[test]
fn a_byte_order_mark_and_json_whitespace_are_read_through() {
    // Tolerance is the state file's (state.md, Read tolerantly).
    let h = Harness::new();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"\r\n {\r\n \"version\" : 0 ,\r\n \"leases\" : [ ] }\r\n\r\n");
    h.write_leases_raw(&bytes);
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_eq!(h.log(), idle_line(NOW));
}

#[test]
fn the_state_file_is_never_written_by_a_pass() {
    let h = Harness::new();
    let original = file_bytes(vec![ready(ID_A, LATER), pending(ID_B)]);
    h.write_leases_raw(&original);
    h.set_start(agentspace_keeper_api::Started::Holder(holder_for(
        4242, ID_A,
    )));
    h.pass(NOW);
    assert_eq!(std::fs::read(h.leases_path()).unwrap(), original);
    // And no temporary file of one is left beside it.
    let names: Vec<String> = std::fs::read_dir(&h.state)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names.iter().any(|n| n.starts_with("leases.json.")),
        "{names:?}"
    );
}

// ---- The pass time ----

#[test]
fn now_is_the_pass_time_and_every_line_carries_it() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.set_start(agentspace_keeper_api::Started::Holder(holder_for(
        4242, ID_A,
    )));
    // The fake clock says something else entirely.
    h.fakes.clock.set_ms(ms_of(2030, 1, 1, 0, 0, 0));
    let r = h.pass("2026-11-19T12:34:56Z");
    assert_eq!(r.exit, 0);
    assert_eq!(
        h.log(),
        line(
            "2026-11-19T12:34:56Z",
            "started",
            ID_A,
            &alias_of(ID_A),
            "4242",
            None
        )
    );
}

#[test]
fn without_now_the_pass_time_is_the_current_utc_second_truncated() {
    let h = Harness::new();
    h.fakes.clock.set_ms(ms_of(2026, 11, 19, 12, 0, 7) + 999);
    let r = h.pass_on_clock();
    assert_eq!(r.exit, 0);
    assert_eq!(h.log(), idle_line("2026-11-19T12:00:07Z"));
}

#[test]
fn the_pass_time_is_read_once_before_the_lock_is_taken_and_every_line_carries_it() {
    // Waiting for the lock takes 8 seconds and the start 90, on the fake clock. The line still
    // carries the time the pass began.
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.set_start(agentspace_keeper_api::Started::Holder(holder_for(
        4242, ID_A,
    )));
    h.fakes.locks.wait_ms.set(8_000);
    h.fakes.starter.takes_ms.set(90_000);
    // 1 MiB + 1 on the holder's log, so that a rotated line is written too.
    std::fs::write(h.ssh_log_path(), vec![b'x'; 1_048_577]).unwrap();
    let r = h.pass_on_clock();
    assert_eq!(r.exit, 0);
    assert_eq!(
        h.log(),
        format!(
            "{}{}",
            line(NOW, "rotated", "-", "-", "-", None),
            line(NOW, "started", ID_A, &alias_of(ID_A), "4242", None)
        )
    );
    let e = h.events();
    assert!(pos(&e, "clock") < pos(&e, "lock:pass"), "{e:?}");
}

#[test]
fn a_lease_expiring_at_the_pass_time_is_decided_against_the_pass_time_not_a_later_reading() {
    // The lock wait moves the clock past the lease's expiry. The pass is decided at the time it
    // began, when the lease was still ready.
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, "2026-11-19T12:00:05Z")]);
    h.set_start(agentspace_keeper_api::Started::Holder(holder_for(
        4242, ID_A,
    )));
    h.fakes.locks.wait_ms.set(10_000);
    let r = h.pass_on_clock();
    assert_eq!(r.exit, 0);
    assert_eq!(
        h.log(),
        line(NOW, "started", ID_A, &alias_of(ID_A), "4242", None)
    );
}

// ---- Exit codes and streams of a whole pass ----

#[test]
fn a_pass_writes_nothing_to_stderr_and_ends_by_releasing_the_lock() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.trust(&holder_for(4242, ID_A));
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_eq!(r.stderr, "");
    assert_lock_released_last(&h.events());
}

#[test]
fn a_gone_process_does_not_change_the_exit_code() {
    let h = Harness::new();
    h.record_with_query(&holder_for(4242, ID_A), Query::Gone);
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
}

//! The rotation of `board-holder-ssh.log` (docs/keeper.md, Log lines: The holder's log).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **At the start of every
//! pass that holds `board-holder.lock`, before the lease state is read, a `board-holder-ssh.log`
//! larger than 1 MiB (1,048,576 bytes) is renamed to `board-holder-ssh.log.1`, replacing any older
//! one, and the pass logs `<time> rotated lease=- alias=- pid=-`.** A failure to rotate, or to log
//! the rotation, MUST NOT stop the pass, and is not logged.

mod common;

use agentspace_keeper_api::Started;
use common::*;

const MIB: usize = 1_048_576;

fn rotated(time: &str) -> String {
    line(time, "rotated", "-", "-", "-", None)
}

fn make(h: &Harness, bytes: usize) {
    std::fs::write(h.ssh_log_path(), vec![b'x'; bytes]).unwrap();
}

#[test]
fn a_log_larger_than_one_mib_is_renamed_and_the_rotation_is_logged() {
    let h = Harness::new();
    let big: Vec<u8> = (0..MIB + 1).map(|i| (i % 251) as u8).collect();
    std::fs::write(h.ssh_log_path(), &big).unwrap();
    let before = h.log_bytes();
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0);
    assert!(
        !h.ssh_log_path().exists(),
        "The next holder opens a new file, so the old name is free"
    );
    assert_eq!(
        std::fs::read(h.ssh_log_1_path()).unwrap(),
        big,
        "The whole file moved"
    );
    assert_eq!(h.log(), format!("{}{}", rotated(NOW), idle_line(NOW)));
    assert_stdout_is_log_since(&h, &before, &ran);
}

#[test]
fn the_limit_is_strictly_larger_than_1_048_576_bytes() {
    let h = Harness::new();
    make(&h, MIB);
    h.pass(NOW);
    assert!(
        h.ssh_log_path().exists(),
        "A log of exactly 1 MiB is not rotated"
    );
    assert!(!h.ssh_log_1_path().exists());
    assert_eq!(h.log(), idle_line(NOW));

    make(&h, MIB + 1);
    h.pass("2026-11-19T12:00:30Z");
    assert!(!h.ssh_log_path().exists());
    assert_eq!(
        std::fs::metadata(h.ssh_log_1_path()).unwrap().len(),
        (MIB + 1) as u64
    );
}

#[test]
fn a_small_or_missing_log_is_left_alone_and_nothing_is_logged_for_it() {
    let h = Harness::new();
    h.pass(NOW);
    assert_eq!(h.log(), idle_line(NOW));
    std::fs::write(h.ssh_log_path(), b"ssh: some output\n").unwrap();
    h.pass("2026-11-19T12:00:30Z");
    assert_eq!(
        std::fs::read(h.ssh_log_path()).unwrap(),
        b"ssh: some output\n"
    );
    assert!(!h.ssh_log_1_path().exists());
    assert!(!h.log().contains(" rotated "));
}

#[test]
fn an_older_dot_one_is_replaced() {
    let h = Harness::new();
    std::fs::write(h.ssh_log_1_path(), b"the older one").unwrap();
    make(&h, MIB + 10);
    h.pass(NOW);
    let now = std::fs::read(h.ssh_log_1_path()).unwrap();
    assert_eq!(now.len(), MIB + 10);
    assert_ne!(now, b"the older one");
}

#[test]
fn the_rotation_comes_before_the_lease_state_is_read() {
    // An unreadable state file fails the pass with exit 1, and the rotation had already happened.
    let h = Harness::new();
    h.write_leases_raw(b"not json");
    make(&h, MIB + 1);
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 1);
    assert!(!h.ssh_log_path().exists());
    assert!(h.ssh_log_1_path().exists());
    assert_eq!(
        h.log(),
        format!(
            "{}{}",
            rotated(NOW),
            failed_line(
                NOW,
                "Lease state could not be read: the state file is not valid"
            )
        )
    );
}

#[test]
fn a_rotated_line_precedes_the_action_line_of_the_same_pass() {
    // Where the rotation comes after the matcher's self-check is pass_order.rs's.
    let h = Harness::new();
    make(&h, MIB + 1);
    h.pass(NOW);
    assert!(h.log().starts_with(&rotated(NOW)));
}

#[test]
fn every_pass_that_holds_the_lock_rotates_whatever_its_action() {
    // A kept holder.
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.trust(&holder_for(4242, ID_A));
    make(&h, MIB + 1);
    h.pass(NOW);
    assert_eq!(h.log().lines().count(), 2);
    assert!(h.log().starts_with(&rotated(NOW)));

    // A start.
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.start_echo(4242);
    make(&h, MIB + 1);
    h.pass(NOW);
    assert!(h.log().starts_with(&rotated(NOW)));

    // A failed start.
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.set_start(Started::Failed("x".into()));
    make(&h, MIB + 1);
    h.pass(NOW);
    assert!(h.log().starts_with(&rotated(NOW)));
}

#[test]
fn a_rotation_is_logged_once_the_next_pass_finds_a_small_or_missing_log() {
    let h = Harness::new();
    make(&h, MIB + 1);
    h.pass(NOW);
    h.pass("2026-11-19T12:00:30Z");
    assert_eq!(h.log().matches(" rotated ").count(), 1);
}

#[test]
fn a_failure_to_rotate_does_not_stop_the_pass_and_is_not_logged() {
    // A folder, with a file in it, where `board-holder-ssh.log.1` belongs: the rename fails.
    let h = Harness::new();
    std::fs::create_dir_all(h.ssh_log_1_path()).unwrap();
    std::fs::write(h.ssh_log_1_path().join("keep"), b"x").unwrap();
    make(&h, MIB + 1);
    let before = h.log_bytes();
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0, "stderr was {:?}", ran.stderr);
    assert_eq!(ran.stderr, "");
    assert_eq!(
        h.log(),
        idle_line(NOW),
        "A rotation that failed is not logged"
    );
    assert_stdout_is_log_since(&h, &before, &ran);
    assert!(h.ssh_log_path().exists());
}

#[test]
fn a_rotated_line_that_cannot_be_logged_changes_nothing_about_the_rotation() {
    // With the log unwritable every line is dropped, the rotated one included; the rename stands.
    let h = Harness::new();
    std::fs::create_dir_all(h.log_path()).unwrap();
    make(&h, MIB + 1);
    h.pass(NOW);
    assert!(!h.ssh_log_path().exists());
    assert!(h.ssh_log_1_path().exists());
}

#[test]
fn the_pass_writes_nothing_else_to_the_holders_log() {
    let h = Harness::new();
    std::fs::write(h.ssh_log_path(), b"holder output\n").unwrap();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.start_echo(4242);
    h.pass(NOW);
    assert_eq!(std::fs::read(h.ssh_log_path()).unwrap(), b"holder output\n");
}

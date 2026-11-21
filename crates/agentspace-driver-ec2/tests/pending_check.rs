//! `check_pending` through the `Driver` trait (docs/driver.md, check_pending and Starting a
//! provisioning run, from `check_pending`; E0.07-T03-S01, #0150).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. It starts the run again
//! under the same rule as place, tries the registry lock without waiting, never writes the lease
//! state file, never takes its lock, issues no credential and writes no alias. Every start goes
//! through a fake `Detach`.
#![cfg(windows)]

mod common;
#[path = "common/pending.rs"]
mod pending;

use agentspace_driver_api::{Driver, Failure, LeaseRef};
use agentspace_hosts::HostState;
use common::*;
use pending::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PROJECT: &str = "agentspace";

fn world(project: &str) -> (Fx, Arc<FakeDetach>) {
    let mut fx = Fx::with_ca();
    fx.install_operator();
    let detach = FakeDetach::observing(&fx.home, project);
    (fx, detach)
}

fn check(fx: &Fx, d: &impl Driver, project: &str) -> Result<(), Failure> {
    d.check_pending(&fx.ctx, &LeaseRef { id: ID, project })
}

fn record(state: &str) -> H {
    match state {
        "ready" => H::ready(PROJECT),
        "creating" => H::creating(PROJECT),
        _ => H::failed(PROJECT),
    }
}

// ---- Starting the run ----------------------------------------------------------------------------------------

#[test]
fn a_ready_host_gets_its_run_started_and_only_last_attempt_is_written() {
    // driver.md, from `check_pending`: a ready record is treated as `creating` and `failed` are,
    // but for its state, which stays `ready` (conformance C12 and C19).
    let (fx, detach) = world(PROJECT);
    fx.write_hosts(&[H::ready(PROJECT).instance(INSTANCE)]);
    let d = fx.driver_with(&detach);
    let t0 = now();
    check(&fx, &d, PROJECT).expect("check_pending succeeds");
    let t1 = now();
    let got = fx.host(PROJECT).unwrap();
    assert_eq!(
        got.state,
        HostState::Ready,
        "The driver never takes a host out of ready"
    );
    assert_eq!(got.instance_id.as_deref(), Some(INSTANCE));
    let at = epoch_of(
        got.last_attempt_at
            .as_deref()
            .expect("last_attempt_at is set"),
    );
    assert!(t0 <= at && at <= t1);
    assert_eq!(
        fx.hosts_bytes().unwrap(),
        hosts_text(&[H::ready(PROJECT).instance(INSTANCE).attempt(at)]).into_bytes(),
        "Only last_attempt_at changed"
    );
    let start = detach.only();
    assert_eq!(start.program, fx.ctx.operator_program.clone().unwrap());
    assert_eq!(start.args, ["provision", "-Host", PROJECT]);
    assert_eq!(start.cwd, fx.ctx.home);
    assert_run_env(&start, &fx.ctx);
    assert!(
        start.seen.hosts_lock_free,
        "hosts.lock is let go before the start"
    );
    assert!(
        start.seen.host_dir_exists,
        "The host folder is made before the start"
    );
    assert!(!start.seen.provision_lock_exists && !start.seen.provision_log_exists);
}

#[test]
fn a_ready_host_with_a_recent_attempt_or_a_held_run_lock_starts_nothing() {
    for held in [false, true] {
        let (fx, detach) = world(PROJECT);
        let h = if held {
            H::ready(PROJECT).attempted_ago(3600)
        } else {
            H::ready(PROJECT).attempted_ago(120)
        };
        fx.write_hosts(&[h]);
        let _run = held.then(|| {
            agentspace_hosts::try_run_lock(&fx.home, PROJECT)
                .unwrap()
                .expect("The run lock is free")
        });
        let before = fx.hosts_bytes();
        let d = fx.driver_with(&detach);
        check(&fx, &d, PROJECT).expect("check_pending succeeds");
        assert_eq!(detach.count(), 0, "held {held}");
        assert_eq!(fx.hosts_bytes(), before, "held {held}");
    }
}

#[test]
fn a_creating_or_failed_host_gets_state_creating_and_its_run_when_due() {
    for state in ["creating", "failed"] {
        for attempt in [None, Some(300), Some(3600)] {
            let (fx, detach) = world(PROJECT);
            let mut h = record(state).instance(INSTANCE);
            if let Some(ago) = attempt {
                h = h.attempted_ago(ago);
            }
            fx.write_hosts(&[h]);
            let d = fx.driver_with(&detach);
            let t0 = now();
            check(&fx, &d, PROJECT).expect("check_pending succeeds");
            let t1 = now();
            let got = fx.host(PROJECT).unwrap();
            assert_eq!(got.state, HostState::Creating, "{state} {attempt:?}");
            assert_eq!(got.instance_id.as_deref(), Some(INSTANCE));
            let at = epoch_of(got.last_attempt_at.as_deref().unwrap());
            assert!(t0 <= at && at <= t1);
            assert_eq!(detach.only().args, ["provision", "-Host", PROJECT]);
        }
    }
}

#[test]
fn a_creating_or_failed_host_with_a_recent_attempt_or_a_held_lock_is_left_alone() {
    for state in ["creating", "failed"] {
        for held in [false, true] {
            let (fx, detach) = world(PROJECT);
            let h = record(state).attempted_ago(if held { 3600 } else { 280 });
            fx.write_hosts(&[h]);
            let _run = held.then(|| {
                agentspace_hosts::try_run_lock(&fx.home, PROJECT)
                    .unwrap()
                    .expect("The run lock is free")
            });
            let before = fx.hosts_bytes();
            let d = fx.driver_with(&detach);
            check(&fx, &d, PROJECT).expect("check_pending succeeds");
            assert_eq!(detach.count(), 0, "{state} held {held}");
            assert_eq!(fx.hosts_bytes(), before, "{state} held {held}");
        }
    }
}

#[test]
fn a_second_check_within_five_minutes_starts_nothing() {
    let (fx, detach) = world(PROJECT);
    fx.write_hosts(&[H::ready(PROJECT)]);
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).unwrap();
    let after_first = fx.hosts_bytes();
    check(&fx, &d, PROJECT).unwrap();
    check(&fx, &d, PROJECT).unwrap();
    assert_eq!(detach.count(), 1, "At most one run in any 5 minutes");
    assert_eq!(fx.hosts_bytes(), after_first);
}

#[test]
fn a_place_and_a_check_share_the_five_minute_rule() {
    // A run started by a place is not started again by a get within 5 minutes.
    let (fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    let d = fx.driver_with(&detach);
    fx.place_for(&d, PROJECT).expect("Pending");
    check(&fx, &d, PROJECT).unwrap();
    assert_eq!(detach.count(), 1);
}

// ---- A project with no record -----------------------------------------------------------------------------

#[test]
fn a_listed_project_with_no_record_gets_its_record_and_its_run() {
    // Conformance C25, for a listed project.
    let (fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    let d = fx.driver_with(&detach);
    let t0 = now();
    check(&fx, &d, PROJECT).unwrap();
    let t1 = now();
    let got = fx.host(PROJECT).expect("The record was added");
    assert_eq!(got.state, HostState::Creating);
    assert_eq!(got.stack, "agentspace-host-agentspace");
    let at = epoch_of(got.last_attempt_at.as_deref().unwrap());
    assert!(t0 <= at && at <= t1);
    assert_eq!(
        fx.hosts_bytes().unwrap(),
        hosts_text(&[H::creating(PROJECT).attempt(at)]).into_bytes()
    );
    assert_eq!(detach.only().args, ["provision", "-Host", PROJECT]);
}

#[test]
fn an_unlisted_project_with_no_record_is_left_alone_and_succeeds() {
    // `get` never makes a lease unsatisfiable, and the driver writes nothing.
    for list in [None, Some(&b"{\"version\":0,\"projects\":[]}\n"[..])] {
        let (fx, detach) = world("unlisted");
        if let Some(bytes) = list {
            fx.write_projects_raw(bytes);
        }
        let d = fx.driver_with(&detach);
        check(&fx, &d, "unlisted").expect("check_pending succeeds");
        assert_eq!(detach.count(), 0);
        assert!(fx.hosts_bytes().is_none(), "The registry is not written");
        assert!(!agentspace_hosts::hosts_dir(&fx.home).exists());
    }
}

#[test]
fn a_project_with_a_record_needs_no_list() {
    // The project list is read only for a project with no record.
    let (fx, detach) = world(PROJECT);
    fx.write_projects_raw(b"{");
    fx.write_hosts(&[H::creating(PROJECT)]);
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).expect("A list that is not valid does not matter here");
    assert_eq!(detach.count(), 1);
}

// ---- What it never does -----------------------------------------------------------------------------------

#[test]
fn it_never_writes_the_lease_state_file_and_never_makes_a_credential_or_an_alias() {
    let (mut fx, detach) = world(PROJECT);
    let state = fx.state_elsewhere();
    fx.write_hosts(&[H::ready(PROJECT)]);
    // The state folder does not exist: it is not made.
    let ssh_before = fx.ssh_names();
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).unwrap();
    assert_eq!(detach.count(), 1);
    assert!(!state.exists(), "The state folder is not made");
    assert!(!fx.has_files_of(PROJECT, ID), "No credential and no alias");
    assert_eq!(
        fx.ssh_names(),
        ssh_before,
        "The OpenSSH folder is untouched"
    );

    // A state file with a pending lease is byte for byte as it was: the lease is never placed
    // here, though its host is ready and its CA is good.
    let (mut fx, detach) = world(PROJECT);
    fx.state_elsewhere();
    fx.write_hosts(&[H::ready(PROJECT)]);
    fx.write_leases(&[L::pending(ID, PROJECT), L::pending(OTHER_ID, PROJECT)]);
    let before = fx.leases_bytes();
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).unwrap();
    assert_eq!(detach.count(), 1);
    assert_eq!(
        fx.leases_bytes(),
        before,
        "The lease state file is as it was"
    );
    assert!(!fx.has_files_of(PROJECT, ID) && !fx.has_files_of(PROJECT, OTHER_ID));
}

#[test]
fn it_never_takes_the_lease_state_lock_and_never_waits_for_it() {
    // A command that changes leases holds the lock: get MUST NOT wait for it.
    let (mut fx, detach) = world(PROJECT);
    let state = fx.state_elsewhere();
    fx.write_hosts(&[H::ready(PROJECT)]);
    fx.write_leases(&[L::pending(ID, PROJECT)]);
    let held = hold_lease_lock(&state);
    let d = fx.driver_with(&detach);
    let begun = Instant::now();
    check(&fx, &d, PROJECT).expect("check_pending succeeds");
    let took = begun.elapsed();
    assert!(
        took < Duration::from_secs(5),
        "It waited for the lease state lock: {took:?}"
    );
    assert_eq!(detach.count(), 1, "The run is started all the same");
    drop(held);
}

#[test]
fn a_held_registry_lock_starts_nothing_and_succeeds_without_waiting() {
    // Conformance C13: it tries hosts.lock once.
    for state in ["ready", "creating", "failed"] {
        let (fx, detach) = world(PROJECT);
        fx.write_hosts(&[record(state)]);
        let held = agentspace_hosts::lock(&fx.home).expect("The registry lock is free");
        let before = fx.hosts_bytes();
        let d = fx.driver_with(&detach);
        let begun = Instant::now();
        let got = check(&fx, &d, PROJECT);
        let took = begun.elapsed();
        drop(held);
        assert!(got.is_ok(), "{state}: it succeeds");
        assert!(
            took < Duration::from_secs(5),
            "{state}: it waited for hosts.lock: {took:?}"
        );
        assert_eq!(detach.count(), 0, "{state}");
        assert_eq!(fx.hosts_bytes(), before, "{state}");
    }
}

#[test]
fn it_runs_no_ca_check() {
    // place_pending checks the CA when it places the lease, not this.
    let mut fx = Fx::without_ca();
    fx.install_operator();
    fx.write_hosts(&[H::ready(PROJECT)]);
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).expect("check_pending succeeds with no CA");
    assert_eq!(detach.count(), 1);
    assert!(!fx.ca_dir().exists(), "No CA is made");
}

// ---- Failures the CLI drops -----------------------------------------------------------------------------------

#[test]
fn a_registry_that_cannot_be_read_is_a_failure_and_starts_nothing() {
    for bytes in [&b"{"[..], &b"{\"version\":0,\"hosts\":[]}\n"[..]] {
        let (fx, detach) = world(PROJECT);
        fx.write_projects(&[PROJECT]);
        fx.write_hosts_raw(bytes);
        let d = fx.driver_with(&detach);
        let f = check(&fx, &d, PROJECT).expect_err("A registry that is not valid fails it");
        fx.assert_failure_plain(&f);
        assert_eq!(detach.count(), 0);
        assert_eq!(fx.hosts_bytes().as_deref(), Some(bytes));
    }
}

#[test]
fn a_project_list_that_cannot_be_read_is_a_failure_for_a_project_with_no_record() {
    let (fx, detach) = world(PROJECT);
    fx.write_projects_raw(b"{");
    let d = fx.driver_with(&detach);
    let f = check(&fx, &d, PROJECT).expect_err("A list that is not valid fails it");
    fx.assert_failure_plain(&f);
    assert_eq!(detach.count(), 0);
    assert!(fx.hosts_bytes().is_none());
}

#[test]
fn a_missing_operator_program_is_a_failure_when_a_run_is_due_and_writes_nothing() {
    // Conformance C19's second half and C31: nothing is written, not even last_attempt_at.
    for programs in [0, 1] {
        let (mut fx, detach) = world(PROJECT);
        fx.write_hosts(&[H::ready(PROJECT).instance(INSTANCE)]);
        fx.ctx.operator_program = if programs == 0 {
            None
        } else {
            Some(fx.bin.join("no such program.exe"))
        };
        let before = fx.hosts_bytes();
        let d = fx.driver_with(&detach);
        let f = check(&fx, &d, PROJECT).expect_err("A run is due and has no program");
        assert!(f.message.contains("Nothing was recorded."), "{}", f.message);
        fx.assert_failure_plain(&f);
        assert_eq!(detach.count(), 0);
        assert_eq!(
            fx.hosts_bytes(),
            before,
            "The record is as it was, `ready` included"
        );
    }
    // A listed project's record is not added either.
    let (mut fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    fx.ctx.operator_program = None;
    let d = fx.driver_with(&detach);
    assert!(check(&fx, &d, PROJECT).is_err());
    assert!(fx.hosts_bytes().is_none());
    // And with no run due, the missing program is not looked for.
    let (mut fx, detach) = world(PROJECT);
    fx.write_hosts(&[H::ready(PROJECT).attempted_ago(60)]);
    fx.ctx.operator_program = None;
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).expect("No run is due");
}

#[test]
fn a_start_that_fails_fails_nothing_and_leaves_a_ready_host_ready() {
    for state in ["ready", "creating"] {
        let (fx, _) = world(PROJECT);
        fx.write_hosts(&[record(state)]);
        let detach = FakeDetach::failing("The start could not be made.");
        let d = fx.driver_with(&detach);
        check(&fx, &d, PROJECT).expect("A start that fails fails nothing");
        assert_eq!(detach.count(), 1, "{state}");
        let got = fx.host(PROJECT).unwrap();
        let want = if state == "ready" {
            HostState::Ready
        } else {
            HostState::Creating
        };
        assert_eq!(got.state, want, "{state}");
        assert!(got.last_attempt_at.is_some(), "The attempt is recorded");
        // So the next check is not due for 5 minutes.
        check(&fx, &d, PROJECT).unwrap();
        assert_eq!(detach.count(), 1);
    }
}

#[test]
fn a_cmd_program_is_started_through_the_command_interpreter_here_too() {
    let (mut fx, detach) = world(PROJECT);
    fx.write_hosts(&[H::ready(PROJECT)]);
    let program = fx.install_stand_in("cmd");
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).unwrap();
    let start = detach.only();
    assert_eq!(
        start.program,
        std::path::PathBuf::from(format!(
            "{}\\System32\\cmd.exe",
            system_root().trim_end_matches(['\\', '/'])
        ))
    );
    assert_eq!(
        start.args,
        [
            "/d".to_string(),
            "/c".to_string(),
            program.display().to_string(),
            "provision".to_string(),
            "-Host".to_string(),
            PROJECT.to_string()
        ]
    );
}

#[test]
fn the_run_gets_the_same_four_variables_as_from_place() {
    let (mut fx, detach) = world(PROJECT);
    fx.write_hosts(&[H::ready(PROJECT)]);
    let state = fx.state_elsewhere();
    fx.ctx.ssh_dir = fx.root.join("ssh elsewhere");
    fx.ctx.state_dir_env = Some(state);
    fx.ctx.ssh_dir_env = Some(fx.ctx.ssh_dir.clone());
    fx.ctx.proxy_override = Some("my-transport --stdio".to_string());
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).unwrap();
    let start = detach.only();
    assert_run_env(&start, &fx.ctx);
    assert!(start.effective("AGENTSPACE_STATE_DIR").is_some());
    assert!(start.effective("AGENTSPACE_SSH_DIR").is_some());
    assert!(start.effective("AGENTSPACE_PROXY_COMMAND").is_some());
}

#[test]
fn a_folder_the_cli_did_not_have_set_is_absent_from_the_run() {
    let (mut fx, detach) = world(PROJECT);
    fx.write_hosts(&[H::ready(PROJECT)]);
    fx.state_elsewhere();
    fx.ctx.ssh_dir = fx.root.join("ssh elsewhere");
    let d = fx.driver_with(&detach);
    check(&fx, &d, PROJECT).unwrap();
    let start = detach.only();
    assert_run_env(&start, &fx.ctx);
    assert!(start.effective("AGENTSPACE_STATE_DIR").is_none());
    assert!(start.effective("AGENTSPACE_SSH_DIR").is_none());
    assert!(start.effective("AGENTSPACE_PROXY_COMMAND").is_none());
}

// ---- The bound ------------------------------------------------------------------------------------------------

#[test]
fn it_ends_within_twenty_seconds_even_when_the_start_does_not_return() {
    // driver.md, check_pending: when the start has not returned within the 20 seconds, `get`
    // returns at the bound regardless, and the start counts as made. This test takes about 20
    // seconds: the fake's start returns after 27, and the driver MUST NOT wait for it.
    let (fx, _) = world(PROJECT);
    fx.write_hosts(&[H::ready(PROJECT)]);
    let detach = FakeDetach::blocking(Duration::from_secs(27));
    let d = fx.driver_with(&detach);
    let begun = Instant::now();
    let got = check(&fx, &d, PROJECT);
    let took = begun.elapsed();
    assert!(got.is_ok(), "check_pending succeeds at the bound");
    assert!(
        took < Duration::from_secs(24),
        "It waited for the start: {took:?}"
    );
    assert_eq!(detach.count(), 1, "The start counts as made");
    let got = fx.host(PROJECT).unwrap();
    assert!(
        got.last_attempt_at.is_some(),
        "last_attempt_at was written before the start"
    );
    // A second check within 5 minutes starts no second run.
    check(&fx, &d, PROJECT).unwrap();
    assert_eq!(detach.count(), 1);
}

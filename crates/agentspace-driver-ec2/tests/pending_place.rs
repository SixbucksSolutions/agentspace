//! Place through the project's host: ready, not listed, pending and the provisioning run's start
//! (docs/driver.md, Where it places a lease and Starting a provisioning run; docs/state.md, The
//! host registry, The project list and A lease alias for a host that is not the box; E0.07-T03-S01,
//! #0150).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. Every start goes
//! through a fake `Detach`, so no test starts a process, and every file is in a throwaway folder.
#![cfg(windows)]

mod common;
#[path = "common/pending.rs"]
mod pending;

use agentspace_driver_api::{Driver, Placed, Reason};
use agentspace_hosts::HostState;
use common::*;
use pending::*;
use std::fs;
use std::time::{Duration, Instant};

const PROJECT: &str = "agentspace";
const AGENTSPACE_ALIAS: &str = "agentspace-agentspace-3f2a9c1e";

/// A home with a CA and the operator program installed, and a fake start that looks at the home
/// when it is made.
fn world(project: &str) -> (Fx, std::sync::Arc<FakeDetach>) {
    let mut fx = Fx::with_ca();
    fx.install_operator();
    let detach = FakeDetach::observing(&fx.home, project);
    (fx, detach)
}

fn no_capacity_message(project: &str) -> String {
    format!(
        "No capacity can be found for project '{project}': it has no host and is not on agentspace's project list."
    )
}

const LOOKUP_PREFIX: &str = "The lease's host could not be looked up: ";
const LOOKUP_SUFFIX: &str = ". Nothing was recorded.";

#[track_caller]
fn assert_pending(got: &Result<Placed, agentspace_driver_api::Failure>) {
    assert!(
        matches!(got, Ok(Placed::Pending)),
        "Expected pending: {got:?}"
    );
}

/// A failed look-up: in agentspace's own words, with the file named by what it is, and nothing
/// recorded.
#[track_caller]
fn assert_lookup_failure(fx: &Fx, got: Result<Placed, agentspace_driver_api::Failure>) {
    let f = got.expect_err("The place MUST fail");
    assert!(f.message.starts_with(LOOKUP_PREFIX), "{}", f.message);
    assert!(f.message.ends_with(LOOKUP_SUFFIX), "{}", f.message);
    fx.assert_failure_plain(&f);
    assert!(f.leftovers.is_empty(), "{:?}", f.leftovers);
}

// ---- A project whose host is ready ----------------------------------------------------------------

#[test]
fn a_box_lease_is_placed_with_v06s_alias_bytes_and_starts_nothing() {
    let (fx, detach) = world("swimcoach");
    let d = fx.driver_with(&detach);
    let got = fx.place_for(&d, "swimcoach");
    match got {
        Ok(Placed::Ready {
            host_alias,
            user,
            expires_at,
        }) => {
            assert_eq!(host_alias, ALIAS);
            assert_eq!(user, "ubuntu");
            assert!(expires_at > now());
        }
        other => panic!("Expected ready: {other:?}"),
    }
    assert_alias_bytes(&fx, ID, "swimcoach", false);
    assert_eq!(
        Fx::names(&fx.lease_dir(ID)),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
    assert_eq!(detach.count(), 0, "A ready host starts nothing");
    assert!(
        fx.hosts_bytes().is_none(),
        "A missing hosts.json stays missing for the box"
    );
    assert!(
        !agentspace_hosts::host_dir(&fx.home, "swimcoach").exists(),
        "No host folder is made for a ready host"
    );
}

#[test]
fn a_box_lease_with_a_transport_override_keeps_the_box_form() {
    let (mut fx, detach) = world("swimcoach");
    fx.ctx.proxy_override = Some("my-transport --stdio %n".to_string());
    let d = fx.driver_with(&detach);
    fx.place_for(&d, "swimcoach").expect("Ready");
    assert_alias_bytes(&fx, ID, "swimcoach", false);
}

#[test]
fn a_box_lease_is_placed_the_same_beside_other_hosts_records_and_leaves_them_alone() {
    let (fx, detach) = world("swimcoach");
    // The box's record holds the default size's type, as one does that a deploy or a run has
    // recorded since v0.09: a record with no type meets no size, and place would answer pending
    // (D67; the P3 ruling keeps this test's meaning).
    fx.write_hosts_with(
        Some(DEFAULT_TYPE),
        &[H::creating(PROJECT).attempted_ago(30)],
    );
    let before = fx.hosts_bytes();
    let d = fx.driver_with(&detach);
    assert!(matches!(
        fx.place_for(&d, "swimcoach"),
        Ok(Placed::Ready { .. })
    ));
    assert_alias_bytes(&fx, ID, "swimcoach", false);
    assert_eq!(detach.count(), 0);
    assert_eq!(fx.hosts_bytes(), before, "The registry is not written");
}

#[test]
fn a_ready_host_that_is_not_the_box_gets_its_own_key_lines_and_starts_nothing() {
    let (fx, detach) = world(PROJECT);
    // No instance_id: place MUST NOT read, check for or depend on it.
    fx.write_hosts(&[H::ready(PROJECT)]);
    let before = fx.hosts_bytes();
    let d = fx.driver_with(&detach);
    match fx.place_for(&d, PROJECT) {
        Ok(Placed::Ready {
            host_alias, user, ..
        }) => {
            assert_eq!(host_alias, AGENTSPACE_ALIAS);
            assert_eq!(user, "ubuntu");
        }
        other => panic!("Expected ready: {other:?}"),
    }
    assert_alias_bytes(&fx, ID, PROJECT, true);
    assert_eq!(detach.count(), 0);
    assert_eq!(
        fx.hosts_bytes(),
        before,
        "A ready host's record is not written, last_attempt_at included"
    );
}

#[test]
fn a_ready_host_with_a_transport_override_has_both_the_override_and_the_host_key_lines() {
    let (mut fx, detach) = world(PROJECT);
    fx.ctx.proxy_override = Some("my-transport --stdio".to_string());
    fx.write_hosts(&[H::ready(PROJECT).instance(INSTANCE)]);
    let d = fx.driver_with(&detach);
    fx.place_for(&d, PROJECT).expect("Ready");
    assert_alias_bytes(&fx, ID, PROJECT, true);
}

#[test]
fn a_record_places_its_project_whether_or_not_the_project_is_listed() {
    // Listed, not listed and an invalid list all place a project that has a ready record.
    for list in [
        None,
        Some(&b"{"[..]),
        Some(&b"{\"version\":0,\"projects\":[]}\n"[..]),
    ] {
        let (fx, detach) = world(PROJECT);
        fx.write_hosts(&[H::ready(PROJECT)]);
        if let Some(bytes) = list {
            fx.write_projects_raw(bytes);
        }
        let d = fx.driver_with(&detach);
        assert!(matches!(
            fx.place_for(&d, PROJECT),
            Ok(Placed::Ready { .. })
        ));
        assert_eq!(detach.count(), 0);
    }
}

#[test]
fn a_ready_host_is_placed_while_its_run_lock_is_held_and_whatever_its_last_attempt() {
    // driver.md, from place: a ready record answers ready and starts nothing (conformance C20).
    for attempt in [None, Some(30), Some(3600)] {
        let (fx, detach) = world(PROJECT);
        let mut record = H::ready(PROJECT);
        if let Some(ago) = attempt {
            record = record.attempted_ago(ago);
        }
        fx.write_hosts(&[record]);
        let _run = agentspace_hosts::try_run_lock(&fx.home, PROJECT)
            .unwrap()
            .expect("The run lock is free");
        let before = fx.hosts_bytes();
        let d = fx.driver_with(&detach);
        assert!(matches!(
            fx.place_for(&d, PROJECT),
            Ok(Placed::Ready { .. })
        ));
        assert_eq!(detach.count(), 0);
        assert_eq!(fx.hosts_bytes(), before);
    }
}

// ---- A project with no host that is not listed ------------------------------------------------------------

#[test]
fn an_unlisted_project_with_no_host_is_no_capacity_and_nothing_is_issued_or_started() {
    // No CA: a no_capacity answer runs no CA check, as before v0.07.
    let mut fx = Fx::without_ca();
    fx.install_operator();
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    let got = fx.place_for(&d, "unlisted");
    match got {
        Ok(Placed::Unsatisfiable { reasons }) => {
            assert_eq!(reasons.len(), 1, "Exactly one reason");
            let Reason { code, message } = &reasons[0];
            assert_eq!(*code, "no_capacity");
            assert_eq!(message, &no_capacity_message("unlisted"));
            fx.assert_plain(message);
        }
        other => panic!("Expected unsatisfiable: {other:?}"),
    }
    assert_eq!(detach.count(), 0);
    assert!(fx.hosts_bytes().is_none(), "The registry is not written");
    assert!(!fx.has_files_of("unlisted", ID), "Nothing is issued");
    assert!(fx.ssh_names().is_empty(), "The OpenSSH folder is untouched");
    assert!(!agentspace_hosts::hosts_dir(&fx.home).exists());
}

#[test]
fn a_project_the_list_does_not_name_is_no_capacity_too() {
    let (fx, detach) = world("unlisted");
    fx.write_projects(&["other"]);
    let d = fx.driver_with(&detach);
    match fx.place_for(&d, "unlisted") {
        Ok(Placed::Unsatisfiable { reasons }) => {
            assert_eq!(reasons.len(), 1);
            assert_eq!(reasons[0].code, "no_capacity");
        }
        other => panic!("Expected unsatisfiable: {other:?}"),
    }
    assert_eq!(detach.count(), 0);
    assert!(fx.hosts_bytes().is_none());
}

#[test]
fn an_unsupported_request_lists_its_unsupported_reasons_alone_and_reads_neither_file() {
    let (fx, detach) = world("unlisted");
    // Neither file is read for an unsupported request, so neither being invalid matters.
    fx.write_hosts_raw(b"not json");
    fx.write_projects_raw(b"not json");
    let hosts_before = fx.hosts_bytes();
    let d = fx.driver_with(&detach);
    let req = fx.request_for("unlisted", ID, "windows/amd64", "agent-linux", "m", 600);
    match d.place(&fx.ctx, &req) {
        Ok(Placed::Unsatisfiable { reasons }) => {
            let codes: Vec<&str> = reasons.iter().map(|r| r.code).collect();
            assert_eq!(codes, ["unsupported_platform"], "No no_capacity beside it");
        }
        other => panic!("Expected unsatisfiable: {other:?}"),
    }
    assert_eq!(detach.count(), 0);
    assert_eq!(fx.hosts_bytes(), hosts_before);
}

// ---- A project that has no host yet: pending -------------------------------------------------------------------

#[test]
fn a_listed_project_with_no_record_is_pending_adds_its_record_and_starts_one_run() {
    let (fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    let d = fx.driver_with(&detach);
    let t0 = now();
    let got = fx.place_for(&d, PROJECT);
    let t1 = now();
    assert_pending(&got);

    // The record: added `creating`, with last_attempt_at in the same write, beside the box's.
    let record = fx.host(PROJECT).expect("The project has a record");
    assert_eq!(record.state, HostState::Creating);
    assert_eq!(record.stack, "agentspace-host-agentspace");
    assert_eq!(record.instance_id, None);
    let at = epoch_of(record.last_attempt_at.as_deref().expect("last_attempt_at"));
    assert!(t0 <= at && at <= t1, "last_attempt_at is within the call");
    assert_eq!(
        fx.hosts_bytes().unwrap(),
        hosts_text(&[H::creating(PROJECT).attempt(at)]).into_bytes(),
        "The registry's bytes: the box's record first, then the new one"
    );

    // One run, as driver.md says it is started.
    let start = detach.only();
    assert_eq!(start.program, fx.ctx.operator_program.clone().unwrap());
    assert_eq!(start.args, ["provision", "-Host", PROJECT]);
    assert_eq!(
        start.cwd, fx.ctx.home,
        "The working folder is the home folder"
    );
    assert_run_env(&start, &fx.ctx);

    // The order: the registry was written and its lock let go, and the folder made, before the
    // start; and the driver never made provision.lock or provision.log.
    assert!(
        start.seen.hosts_lock_free,
        "hosts.lock is let go before the start"
    );
    assert!(
        start.seen.host_dir_exists,
        "hosts\\<project> is made before the start"
    );
    assert!(
        !start.seen.provision_lock_exists,
        "The driver never makes provision.lock"
    );
    assert!(
        !start.seen.provision_log_exists,
        "The driver never makes provision.log"
    );
    assert_eq!(
        start.seen.hosts_json.as_deref(),
        fx.hosts_bytes().as_deref(),
        "The registry was written before the start"
    );

    // Nothing was issued.
    assert!(!fx.has_files_of(PROJECT, ID));
    assert!(fx.ssh_names().is_empty());
}

#[test]
fn a_pending_answer_checks_the_ca_first_and_a_failed_check_writes_and_starts_nothing() {
    let mut fx = Fx::without_ca();
    fx.install_operator();
    fx.write_projects(&[PROJECT]);
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    let f = fx
        .place_for(&d, PROJECT)
        .expect_err("No CA fails the place");
    assert_eq!(f.message, NO_CA);
    assert!(f.leftovers.is_empty());
    assert_eq!(detach.count(), 0);
    assert!(fx.hosts_bytes().is_none(), "The registry is not written");
    assert!(!fx.ca_dir().exists(), "The CA is never created");

    // A CA that fails a check is the same, for a record that is `creating`.
    let mut fx = Fx::with_ca();
    fx.install_operator();
    fx.write_hosts(&[H::creating(PROJECT)]);
    fs::write(fx.ca_dir().join("agentspace_ca.pub"), b"not a key\n").unwrap();
    let before = fx.hosts_bytes();
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    let f = fx
        .place_for(&d, PROJECT)
        .expect_err("A bad CA fails the place");
    assert_eq!(f.message, CA_UNUSABLE);
    assert_eq!(detach.count(), 0);
    assert_eq!(fx.hosts_bytes(), before);
}

#[test]
fn a_record_that_is_creating_or_failed_is_pending_and_its_run_is_started_when_due() {
    // Not listed: the record wins over the list (conformance C24). Due: no attempt yet, or one at
    // least 5 minutes before now.
    for state in ["creating", "failed"] {
        for attempt in [None, Some(300), Some(3600)] {
            let (fx, detach) = world(PROJECT);
            let mut record = if state == "creating" {
                H::creating(PROJECT)
            } else {
                H::failed(PROJECT)
            };
            record = record.instance(INSTANCE);
            if let Some(ago) = attempt {
                record = record.attempted_ago(ago);
            }
            fx.write_hosts(&[record]);
            let d = fx.driver_with(&detach);
            let t0 = now();
            assert_pending(&fx.place_for(&d, PROJECT));
            let t1 = now();
            let got = fx.host(PROJECT).unwrap();
            assert_eq!(got.state, HostState::Creating, "{state} {attempt:?}");
            assert_eq!(
                got.instance_id.as_deref(),
                Some(INSTANCE),
                "An instance ID the record held is kept"
            );
            let at = epoch_of(got.last_attempt_at.as_deref().unwrap());
            assert!(t0 <= at && at <= t1, "{state} {attempt:?}");
            let start = detach.only();
            assert_eq!(start.args, ["provision", "-Host", PROJECT]);
            assert!(!fx.has_files_of(PROJECT, ID), "Nothing is issued");
        }
    }
}

#[test]
fn a_second_place_within_five_minutes_starts_nothing_and_leaves_last_attempt_alone() {
    let (fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    let after_first = fx.hosts_bytes();
    assert_eq!(detach.count(), 1);
    assert_pending(&fx.place_for(&d, PROJECT));
    assert_eq!(detach.count(), 1, "At most one run in any 5 minutes");
    assert_eq!(
        fx.hosts_bytes(),
        after_first,
        "last_attempt_at is as it was"
    );
}

#[test]
fn a_last_attempt_under_five_minutes_old_starts_nothing_and_writes_nothing() {
    for state in ["creating", "failed"] {
        let (fx, detach) = world(PROJECT);
        let record = if state == "creating" {
            H::creating(PROJECT)
        } else {
            H::failed(PROJECT)
        };
        fx.write_hosts(&[record.instance(INSTANCE).attempted_ago(280)]);
        let before = fx.hosts_bytes();
        let d = fx.driver_with(&detach);
        assert_pending(&fx.place_for(&d, PROJECT));
        assert_eq!(detach.count(), 0, "{state}");
        assert_eq!(
            fx.hosts_bytes(),
            before,
            "{state}: a record that existed is not written when no run is due"
        );
        assert!(!agentspace_hosts::host_dir(&fx.home, PROJECT).exists());
    }
}

#[test]
fn a_last_attempt_exactly_five_minutes_old_is_due() {
    let (fx, detach) = world(PROJECT);
    fx.write_hosts(&[H::failed(PROJECT).attempted_ago(300)]);
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    assert_eq!(detach.count(), 1, "5 minutes or more is due");
}

#[test]
fn a_held_run_lock_starts_nothing_and_a_new_record_has_no_last_attempt() {
    // Conformance C23: a listed project with no record, while the project's provision.lock is held.
    let (fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    let run = agentspace_hosts::try_run_lock(&fx.home, PROJECT)
        .unwrap()
        .expect("The run lock is free");
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    assert_eq!(detach.count(), 0, "A run is running");
    assert_eq!(
        fx.hosts_bytes().unwrap(),
        hosts_text(&[H::creating(PROJECT)]).into_bytes(),
        "The record is added `creating` with no last_attempt_at"
    );
    drop(run);
    // Let go, a later place is due at once, since the record has no attempt.
    assert_pending(&fx.place_for(&d, PROJECT));
    assert_eq!(detach.count(), 1);
}

#[test]
fn a_held_run_lock_leaves_a_record_as_it_was() {
    // Conformance C10.
    for state in ["creating", "failed"] {
        let (fx, detach) = world(PROJECT);
        let record = if state == "creating" {
            H::creating(PROJECT)
        } else {
            H::failed(PROJECT)
        };
        fx.write_hosts(&[record.attempted_ago(3600)]);
        let _run = agentspace_hosts::try_run_lock(&fx.home, PROJECT)
            .unwrap()
            .expect("The run lock is free");
        let before = fx.hosts_bytes();
        let d = fx.driver_with(&detach);
        assert_pending(&fx.place_for(&d, PROJECT));
        assert_eq!(detach.count(), 0, "{state}");
        assert_eq!(
            fx.hosts_bytes(),
            before,
            "{state}: state and last_attempt_at as they were"
        );
    }
}

#[test]
fn place_lets_go_of_the_run_lock_it_tried() {
    // The driver tries provision.lock and releases it at once if it took it: the test can take it
    // right after.
    let (fx, detach) = world(PROJECT);
    fx.write_hosts(&[H::creating(PROJECT).attempted_ago(3600)]);
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    let run = agentspace_hosts::try_run_lock(&fx.home, PROJECT).unwrap();
    assert!(run.is_some(), "The driver does not keep provision.lock");
}

// ---- The program that is started -------------------------------------------------------------------------

#[test]
fn a_run_that_is_due_with_no_operator_program_fails_the_place_and_writes_nothing() {
    // driver.md step 3: the three reasons, each with its own words, and no path.
    let message =
        |why: &str| format!("The lease's host cannot be made: {why}. Nothing was recorded.");
    for case in 0..3 {
        let (mut fx, detach) = world(PROJECT);
        fx.write_projects(&[PROJECT]);
        let want = match case {
            0 => {
                fx.ctx.operator_program = None;
                message("agentspace's operator program is not installed beside this program")
            }
            1 => {
                fx.ctx.operator_program = Some(fx.bin.join("no such program.exe"));
                message("the program that AGENTSPACE_PROVISION_CMD names does not exist")
            }
            _ => {
                fx.install_stand_in("cmd");
                fx.ctx.system_root = None;
                message(
                    "Windows' command interpreter cannot be found, because SystemRoot is not set",
                )
            }
        };
        let d = fx.driver_with(&detach);
        let f = fx.place_for(&d, PROJECT).expect_err("The place MUST fail");
        assert_eq!(f.message, want, "case {case}");
        fx.assert_failure_plain(&f);
        assert!(f.leftovers.is_empty());
        assert_eq!(detach.count(), 0);
        assert!(
            fx.hosts_bytes().is_none(),
            "case {case}: the registry is not written, not even the record a start would add"
        );
        assert!(!fx.has_files_of(PROJECT, ID));
    }
}

#[test]
fn an_empty_system_root_is_not_a_command_interpreter_either() {
    let (mut fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    fx.install_stand_in("BAT");
    fx.ctx.system_root = Some(String::new());
    let d = fx.driver_with(&detach);
    let f = fx.place_for(&d, PROJECT).expect_err("The place MUST fail");
    assert!(f.message.contains("SystemRoot is not set"), "{}", f.message);
    assert_eq!(detach.count(), 0);
    assert!(fx.hosts_bytes().is_none());
}

#[test]
fn a_run_that_is_not_due_never_looks_for_the_operator_program() {
    // Conformance C31: the same place with the program missing is pending, and the registry stays.
    for program in [None, Some("no such program.exe")] {
        let (mut fx, detach) = world(PROJECT);
        fx.write_hosts(&[H::creating(PROJECT).attempted_ago(60)]);
        fx.ctx.operator_program = program.map(|p| fx.bin.join(p));
        let before = fx.hosts_bytes();
        let d = fx.driver_with(&detach);
        assert_pending(&fx.place_for(&d, PROJECT));
        assert_eq!(detach.count(), 0);
        assert_eq!(fx.hosts_bytes(), before);
    }
    // And a held run lock makes the run not due the same way.
    let (mut fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    fx.ctx.operator_program = None;
    let _run = agentspace_hosts::try_run_lock(&fx.home, PROJECT)
        .unwrap()
        .unwrap();
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
}

#[test]
fn a_cmd_or_bat_program_is_started_through_the_command_interpreter() {
    // Conformance C18: <system folder>\cmd.exe /d /c "<program>" provision -Host <project>, with the
    // extension compared without regard to case and a trailing separator of SystemRoot dropped.
    for (ext, root_tail) in [("cmd", ""), ("CMD", ""), ("bat", "\\"), ("Bat", "/")] {
        let (mut fx, detach) = world(PROJECT);
        fx.write_projects(&[PROJECT]);
        let program = fx.install_stand_in(ext);
        let root = system_root();
        fx.ctx.system_root = Some(format!("{}{root_tail}", root.trim_end_matches(['\\', '/'])));
        let d = fx.driver_with(&detach);
        assert_pending(&fx.place_for(&d, PROJECT));
        let start = detach.only();
        let interpreter = std::path::PathBuf::from(format!(
            "{}\\System32\\cmd.exe",
            root.trim_end_matches(['\\', '/'])
        ));
        assert_eq!(start.program, interpreter, "{ext}");
        assert_eq!(
            start.args,
            [
                "/d".to_string(),
                "/c".to_string(),
                program.display().to_string(),
                "provision".to_string(),
                "-Host".to_string(),
                PROJECT.to_string()
            ],
            "{ext}"
        );
        assert_eq!(start.cwd, fx.ctx.home);
    }
}

#[test]
fn an_exe_program_is_started_as_it_is_and_needs_no_system_root() {
    let (mut fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    fx.ctx.system_root = None;
    let program = fx.install_stand_in("exe");
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    let start = detach.only();
    assert_eq!(start.program, program);
    assert_eq!(start.args, ["provision", "-Host", PROJECT]);
}

// ---- The run's environment -----------------------------------------------------------------------------------

#[test]
fn the_run_gets_the_four_variables_and_no_other() {
    let (mut fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    // Non-default folders and an override: each is passed, exactly.
    let state = fx.state_elsewhere();
    fx.ctx.ssh_dir = fx.root.join("ssh elsewhere");
    fx.ctx.state_dir_env = Some(state);
    fx.ctx.ssh_dir_env = Some(fx.ctx.ssh_dir.clone());
    fx.ctx.proxy_override = Some("my-transport --stdio".to_string());
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    let start = detach.only();
    assert_run_env(&start, &fx.ctx);
    assert!(start.effective("AGENTSPACE_STATE_DIR").is_some());
    assert!(start.effective("AGENTSPACE_SSH_DIR").is_some());
    assert!(start.effective("AGENTSPACE_PROXY_COMMAND").is_some());
}

#[test]
fn a_folder_variable_the_cli_had_set_to_its_default_is_still_passed() {
    // The context's `state_dir_env` and `ssh_dir_env` say whether the CLI had the variable, which
    // the resolved folders cannot: a variable set to the default value is present all the same.
    let (mut fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    fx.ctx.state_dir_env = Some(fx.ctx.state_dir.clone());
    fx.ctx.ssh_dir_env = Some(fx.ctx.ssh_dir.clone());
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    let start = detach.only();
    assert_run_env(&start, &fx.ctx);
    assert!(start.effective("AGENTSPACE_STATE_DIR").is_some());
    assert!(start.effective("AGENTSPACE_SSH_DIR").is_some());
}

#[test]
fn a_folder_the_cli_did_not_have_set_is_absent_whatever_the_resolved_folder() {
    // Resolved folders that are not the defaults, with no variable behind them: nothing is passed.
    let (mut fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    fx.state_elsewhere();
    fx.ctx.ssh_dir = fx.root.join("ssh elsewhere");
    assert!(fx.ctx.state_dir_env.is_none() && fx.ctx.ssh_dir_env.is_none());
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    let start = detach.only();
    assert_run_env(&start, &fx.ctx);
    assert!(start.effective("AGENTSPACE_STATE_DIR").is_none());
    assert!(start.effective("AGENTSPACE_SSH_DIR").is_none());
}

#[test]
fn a_variable_the_cli_did_not_have_is_absent_from_the_run() {
    // Default folders and no override: the CLI had none of the three set, so none is in the run's
    // environment, and the logon environment's value of one is removed (an empty value says so).
    let (fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    assert_eq!(fx.ctx.state_dir, fx.home.join(".agentspace"));
    assert_eq!(fx.ctx.ssh_dir, fx.home.join(".ssh"));
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    let start = detach.only();
    assert_run_env(&start, &fx.ctx);
    for name in [
        "AGENTSPACE_STATE_DIR",
        "AGENTSPACE_SSH_DIR",
        "AGENTSPACE_PROXY_COMMAND",
    ] {
        assert!(start.effective(name).is_none(), "{name} is present");
    }
}

// ---- A start that fails -----------------------------------------------------------------------------------------

#[test]
fn a_start_that_fails_changes_nothing_more_and_the_place_still_answers_pending() {
    let (fx, _) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    let detach = FakeDetach::failing("The start could not be made.");
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    assert_eq!(detach.count(), 1, "The start was tried");
    let record = fx.host(PROJECT).unwrap();
    assert_eq!(record.state, HostState::Creating);
    assert!(
        record.last_attempt_at.is_some(),
        "The record stays as step 4 wrote it"
    );
    assert!(!fx.has_files_of(PROJECT, ID));
    // The next start is due 5 minutes later, not now.
    let after = fx.hosts_bytes();
    assert_pending(&fx.place_for(&d, PROJECT));
    assert_eq!(detach.count(), 1);
    assert_eq!(fx.hosts_bytes(), after);
}

// ---- A registry or list that cannot be read ----------------------------------------------------------------------------

#[test]
fn a_registry_that_is_not_valid_fails_the_place_for_every_project_and_is_never_repaired() {
    let bad: [&[u8]; 4] = [
        b"{",
        b"{\"version\":0,\"hosts\":[]}\n",
        b"{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n",
        b"{\"version\":1,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n",
    ];
    for bytes in bad {
        for project in ["swimcoach", PROJECT] {
            let (fx, detach) = world(project);
            fx.write_projects(&[PROJECT]);
            fx.write_hosts_raw(bytes);
            let d = fx.driver_with(&detach);
            assert_lookup_failure(&fx, fx.place_for(&d, project));
            assert_eq!(detach.count(), 0);
            assert_eq!(fx.hosts_bytes().as_deref(), Some(bytes), "Never repaired");
            assert!(!fx.has_files_of(project, ID));
        }
    }
}

#[test]
fn a_folder_where_the_registry_belongs_fails_the_place() {
    let (fx, detach) = world("swimcoach");
    fs::create_dir_all(agentspace_hosts::hosts_path(&fx.home)).unwrap();
    let d = fx.driver_with(&detach);
    assert_lookup_failure(&fx, fx.place_for(&d, "swimcoach"));
    assert!(!fx.has_files_of("swimcoach", ID));
}

#[test]
fn a_project_list_that_is_not_valid_fails_a_project_with_no_record_and_writes_nothing() {
    let bad: [&[u8]; 3] = [
        b"{",
        b"{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-02-30\"}]}\n",
        b"{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\"},{\"name\":\"agentspace\",\"approved\":\"2026-11-20\"}]}\n",
    ];
    for bytes in bad {
        let (fx, detach) = world(PROJECT);
        fx.write_projects_raw(bytes);
        let d = fx.driver_with(&detach);
        assert_lookup_failure(&fx, fx.place_for(&d, PROJECT));
        assert_eq!(detach.count(), 0);
        assert!(fx.hosts_bytes().is_none(), "The registry is not written");
    }
    // A folder where the list belongs is the same.
    let (fx, detach) = world(PROJECT);
    fs::create_dir_all(agentspace_hosts::projects_path(&fx.home)).unwrap();
    let d = fx.driver_with(&detach);
    assert_lookup_failure(&fx, fx.place_for(&d, PROJECT));
    assert_eq!(detach.count(), 0);
}

#[test]
fn a_project_list_that_is_not_valid_matters_only_to_a_project_with_no_record() {
    // Conformance C21.
    let (fx, detach) = world("swimcoach");
    fx.write_projects_raw(b"{");
    let d = fx.driver_with(&detach);
    assert!(matches!(
        fx.place_for(&d, "swimcoach"),
        Ok(Placed::Ready { .. })
    ));
    assert_eq!(detach.count(), 0);

    let (fx, detach) = world(PROJECT);
    fx.write_projects_raw(b"{");
    fx.write_hosts(&[H::creating(PROJECT)]);
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    assert_eq!(detach.count(), 1);
}

#[test]
fn a_registry_lock_held_for_a_short_while_is_waited_for() {
    // Conformance C22, the first half: the hold is let go after under 30 seconds.
    let (fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    let held = agentspace_hosts::lock(&fx.home).expect("The registry lock is free");
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        drop(held);
    });
    let begun = Instant::now();
    let d = fx.driver_with(&detach);
    assert_pending(&fx.place_for(&d, PROJECT));
    let waited = begun.elapsed();
    releaser.join().unwrap();
    assert!(
        waited >= Duration::from_millis(1000),
        "Place waited for the lock: {waited:?}"
    );
    assert_eq!(
        detach.count(),
        1,
        "Once it is let go, it succeeds as it would have"
    );
    assert!(fx.host(PROJECT).is_some());
}

#[test]
fn a_registry_lock_held_past_thirty_seconds_fails_the_place_and_changes_nothing() {
    // Conformance C22, the second half. This test takes about 30 seconds: place MUST NOT wait
    // longer than that.
    let (fx, detach) = world(PROJECT);
    fx.write_projects(&[PROJECT]);
    let held = agentspace_hosts::lock(&fx.home).expect("The registry lock is free");
    let begun = Instant::now();
    let d = fx.driver_with(&detach);
    let got = fx.place_for(&d, PROJECT);
    let waited = begun.elapsed();
    drop(held);
    assert!(
        waited >= Duration::from_secs(28) && waited <= Duration::from_secs(45),
        "Place waits for about 30 seconds: {waited:?}"
    );
    assert_lookup_failure(&fx, got);
    assert_eq!(detach.count(), 0);
    assert!(fx.hosts_bytes().is_none());
}

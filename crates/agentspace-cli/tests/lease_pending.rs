//! The lease CLI and a lease that is `pending` (E0.07-T03-S01, #0150).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (`agentspace lease get`, Environment), docs/driver.md (check_pending,
//! The operator program) and docs/state.md (The lease state file, The home folder), never from the
//! program's code (D31). Each test runs through `agentspace_cli::execute_in` with a fake driver, in
//! its own throwaway folder under the temp folder, and never changes its own process's
//! environment, so no test can start a provisioning run.
#![cfg(windows)]

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use agentspace_driver_api::Context;
use fake_driver::{Call, Handle, fail, fail_with};
use lab::{ID1, ID2, ID3, ID4, Lab, NOW, Rec};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

fn check_pending(id: &str, project: &str) -> Call {
    Call::CheckPending {
        id: id.to_string(),
        project: project.to_string(),
    }
}

// ---- get of a pending lease ---------------------------------------------------------------------------------

#[test]
fn get_of_a_pending_lease_asks_its_driver_once_and_stays_read_only() {
    let (lab, h) = Lab::with_fake();
    let pending = Rec::pending(ID1, "swim");
    lab.seed(&[pending.clone(), Rec::ready(ID2, "swim")]);
    std::fs::write(lab.lock_file(), b"").expect("The lock file can be made");
    let before = lab.state_bytes();
    // At the call, the lease state lock is free (get never takes it) and the file is as it was.
    let lock = lab::lock_probe(lab.lock_file());
    let state = lab::state_probe(lab.state_file());
    h.set_probe(Arc::new(move |call| {
        format!("{}\n{}", lock(call), state(call))
    }));
    let ran = lab.run(&["lease", "get", ID1]);
    assert_eq!(
        ran.lease_doc(),
        pending.doc(NOW),
        "It prints the lease as stored"
    );
    assert_eq!(h.calls(), [check_pending(ID1, "swim")]);
    let probes = h.probes();
    assert!(
        probes[0].starts_with("free\n"),
        "get MUST NOT hold the lease state lock: {}",
        probes[0]
    );
    assert_eq!(
        lab.state_bytes(),
        before,
        "get MUST NOT write the lease state file"
    );
    assert_eq!(ran.now_calls, 1, "The clock is read once per command");
}

#[test]
fn get_asks_after_it_has_read_the_records_and_prints_what_it_read() {
    // Whatever the driver does, the document is the stored record's: here the driver's call finds
    // the state file already changed by someone else, and the output is still what get read.
    let (lab, h) = Lab::with_fake();
    let pending = Rec::pending(ID1, "swim");
    lab.seed(std::slice::from_ref(&pending));
    h.set_probe(lab::state_probe(lab.state_file()));
    let ran = lab.run(&["lease", "get", ID1]);
    assert!(h.probes()[0].contains("\"state\":\"pending\""));
    assert_eq!(ran.lease_doc(), pending.doc(NOW));
}

#[test]
fn get_of_a_lease_in_any_other_state_asks_no_driver_and_so_does_list() {
    let (lab, h) = Lab::with_fake();
    let recs = [
        Rec::ready(ID1, "swim"),
        Rec::expired(ID2, "swim"),
        Rec::released(ID3, "swim"),
        Rec::released_from_pending(&lab::uuid_n(5), "swim"),
        Rec::unsatisfiable(&lab::uuid_n(6), "swim"),
    ];
    lab.seed(&recs);
    for rec in &recs {
        lab.run(&["lease", "get", &rec.id]).lease_doc();
    }
    // A pending lease is in the file now, and list never asks about it.
    let mut all = recs.to_vec();
    all.push(Rec::pending(ID4, "swim"));
    lab.seed(&all);
    lab.run(&["lease", "list"]).list_doc();
    lab.run(&["lease", "list", "--state", "pending"]).list_doc();
    assert!(
        h.calls().is_empty(),
        "Only get of a pending lease asks check_pending: {:?}",
        h.calls()
    );
}

#[test]
fn create_renew_and_release_never_ask_check_pending() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    lab.run(&["lease", "renew", ID1]).lease_doc();
    lab.run(&["lease", "release", ID1]).lease_doc();
    assert!(
        h.calls()
            .iter()
            .all(|c| !matches!(c, Call::CheckPending { .. })),
        "{:?}",
        h.calls()
    );
}

#[test]
fn a_failure_of_check_pending_changes_neither_the_output_nor_the_exit_code() {
    for failure in [
        fail("The lease's host cannot be made: nothing was recorded."),
        fail_with("A message.", &["~/leftover"]),
    ] {
        let (lab, h) = Lab::with_fake();
        let pending = Rec::pending(ID1, "swim");
        lab.seed(std::slice::from_ref(&pending));
        let before = lab.state_bytes();
        h.script_check_pending(Err(failure));
        let ran = lab.run(&["lease", "get", ID1]);
        assert_eq!(ran.lease_doc(), pending.doc(NOW));
        assert!(ran.stderr.is_empty(), "Nothing of the failure is printed");
        assert_eq!(h.calls(), [check_pending(ID1, "swim")]);
        assert_eq!(lab.state_bytes(), before);
    }
}

#[test]
fn a_pending_record_with_no_driver_or_an_unknown_driver_asks_none() {
    // Conformance C26: it is printed, exit 0, and nothing is started.
    for driver in [None, Some("not-a-driver-here")] {
        let (lab, h) = Lab::with_fake();
        let pending = Rec::pending(ID1, "swim").driver(driver);
        lab.seed(std::slice::from_ref(&pending));
        let before = lab.state_bytes();
        let ran = lab.run(&["lease", "get", ID1]);
        assert_eq!(ran.lease_doc(), pending.doc(NOW), "{driver:?}");
        assert!(h.calls().is_empty(), "{driver:?}");
        assert_eq!(lab.state_bytes(), before);
    }
}

#[test]
fn get_asks_the_driver_the_record_names() {
    let mut lab = Lab::new();
    let alpha = lab.fake("alpha");
    let beta = lab.fake("beta");
    lab.seed(&[Rec::pending(ID1, "swim").driver(Some("beta"))]);
    lab.run(&["lease", "get", ID1]).lease_doc();
    assert!(alpha.calls().is_empty());
    assert_eq!(beta.calls(), [check_pending(ID1, "swim")]);
}

#[test]
fn get_of_a_pending_lease_with_no_home_still_prints_it() {
    // With a state folder given, get needs no home (lease-api.md, Exit codes and errors), and the
    // check is dropped whatever it does.
    for empty in [false, true] {
        let (mut lab, _h) = Lab::with_fake();
        let pending = Rec::pending(ID1, "swim");
        lab.seed(std::slice::from_ref(&pending));
        if empty {
            lab.set_var("USERPROFILE", "");
        } else {
            lab.unset_var("USERPROFILE");
        }
        let before = lab.state_bytes();
        let ran = lab.run(&["lease", "get", ID1]);
        assert_eq!(ran.lease_doc(), pending.doc(NOW));
        assert_eq!(lab.state_bytes(), before);
    }
}

// ---- The context a driver is given ---------------------------------------------------------------------------

/// The context `get` of a pending lease gives the driver, and the one `create` gives `place`.
fn contexts_of_get_and_create(lab: &Lab, h: &Handle) -> (Context, Context) {
    lab.seed(&[Rec::pending(ID1, "swim")]);
    lab.run(&["lease", "get", ID1]).lease_doc();
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    let mut seen = h.contexts();
    assert_eq!(seen.len(), 2, "get asked once and create placed once");
    let create = seen.pop().unwrap();
    let get = seen.pop().unwrap();
    (get, create)
}

#[test]
fn the_context_names_the_resolved_folders_and_the_variables_the_cli_had() {
    let (mut lab, h) = Lab::with_fake();
    lab.set_var("AGENTSPACE_PROXY_COMMAND", "my-transport --stdio %n");
    let (get, create) = contexts_of_get_and_create(&lab, &h);
    for ctx in [get, create] {
        assert_eq!(ctx.home, lab.home);
        assert_eq!(ctx.ssh_dir, lab.ssh);
        assert_eq!(ctx.state_dir, lab.state);
        // Checked by name, and a mismatch never prints a value.
        assert!(
            ctx.state_dir_env == Some(lab.state.clone()),
            "state_dir_env is not the state folder the CLI had"
        );
        assert!(
            ctx.ssh_dir_env == Some(lab.ssh.clone()),
            "ssh_dir_env is not the ssh folder the CLI had"
        );
        assert_eq!(
            ctx.proxy_override.as_deref(),
            Some("my-transport --stdio %n"),
            "The override is exactly as given"
        );
        assert_eq!(ctx.own_exe, lab.exe);
    }
}

#[test]
fn the_folder_variables_the_cli_did_not_have_are_none_in_the_context() {
    for empty in [false, true] {
        let (mut lab, h) = Lab::with_fake();
        for var in ["AGENTSPACE_STATE_DIR", "AGENTSPACE_SSH_DIR"] {
            if empty {
                lab.set_var(var, "");
            } else {
                lab.unset_var(var);
            }
        }
        lab.set_var("AGENTSPACE_PROXY_COMMAND", "");
        // With no AGENTSPACE_STATE_DIR the records are read from the home's default folder, so the
        // lab seeds them there.
        lab.state = lab.home.join(".agentspace");
        let (get, create) = contexts_of_get_and_create(&lab, &h);
        for ctx in [get, create] {
            assert_eq!(ctx.state_dir, lab.home.join(".agentspace"), "empty {empty}");
            assert_eq!(ctx.ssh_dir, lab.home.join(".ssh"), "empty {empty}");
            assert!(
                ctx.state_dir_env.is_none(),
                "state_dir_env is set, empty {empty}"
            );
            assert!(
                ctx.ssh_dir_env.is_none(),
                "ssh_dir_env is set, empty {empty}"
            );
            assert_eq!(ctx.proxy_override, None, "empty {empty}");
        }
    }
}

#[test]
fn a_relative_folder_variable_is_made_full_in_the_context_against_the_working_folder() {
    let (mut lab, h) = Lab::with_fake();
    lab.set_var("AGENTSPACE_STATE_DIR", "state-rel");
    lab.set_var("AGENTSPACE_SSH_DIR", "ssh-rel");
    // get reads the records from the made-full folder.
    let state = lab.cwd.join("state-rel");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        state.join("leases.json"),
        format!(
            "{{\"version\":0,\"leases\":[{}]}}\n",
            serde_json::to_string(&Rec::pending(ID1, "swim").record()).unwrap()
        ),
    )
    .unwrap();
    lab.run(&["lease", "get", ID1]).lease_doc();
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    for ctx in h.contexts() {
        assert_eq!(ctx.state_dir, lab.cwd.join("state-rel"));
        assert!(
            ctx.state_dir_env == Some(lab.cwd.join("state-rel")),
            "state_dir_env is not the relative folder made full"
        );
        assert_eq!(ctx.ssh_dir, lab.cwd.join("ssh-rel"));
        assert!(
            ctx.ssh_dir_env == Some(lab.cwd.join("ssh-rel")),
            "ssh_dir_env is not the relative folder made full"
        );
    }
}

// ---- The operator program --------------------------------------------------------------------------------------

/// What the context's `operator_program` is for `get` of a pending lease and for `create`, which
/// MUST agree.
fn operator_program(lab: &Lab, h: &Handle) -> Option<PathBuf> {
    let (get, create) = contexts_of_get_and_create(lab, h);
    assert_eq!(
        get.operator_program, create.operator_program,
        "get and create resolve the program alike"
    );
    get.operator_program
}

fn beside(lab: &Lab) -> PathBuf {
    lab.exe
        .parent()
        .expect("The program has a folder")
        .join("agentspace-operator.exe")
}

fn install_beside(lab: &Lab) -> PathBuf {
    let program = beside(lab);
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    std::fs::write(&program, b"").unwrap();
    program
}

#[test]
fn the_operator_program_is_the_one_beside_the_cli_when_no_variable_names_one() {
    for empty in [false, true] {
        let (mut lab, h) = Lab::with_fake();
        let program = install_beside(&lab);
        if empty {
            lab.set_var("AGENTSPACE_PROVISION_CMD", "");
        }
        assert_eq!(operator_program(&lab, &h), Some(program), "empty {empty}");
    }
}

#[test]
fn agentspace_provision_cmd_names_the_operator_program_whether_or_not_the_file_exists() {
    // The driver decides whether the program can be found, and says which of its three reasons.
    for exists in [false, true] {
        let (mut lab, h) = Lab::with_fake();
        install_beside(&lab);
        let stand_in = lab.root.join("stand in").join("run.cmd");
        if exists {
            std::fs::create_dir_all(stand_in.parent().unwrap()).unwrap();
            std::fs::write(&stand_in, b"@exit /b 0\r\n").unwrap();
        }
        lab.set_var(
            "AGENTSPACE_PROVISION_CMD",
            stand_in.clone().into_os_string(),
        );
        assert_eq!(
            operator_program(&lab, &h),
            Some(stand_in),
            "exists {exists}"
        );
    }
}

#[test]
fn a_relative_agentspace_provision_cmd_is_made_full_against_the_working_folder() {
    // Conformance C30: the run's own working folder is the home folder, so the CLI makes it full.
    let (mut lab, h) = Lab::with_fake();
    lab.set_var("AGENTSPACE_PROVISION_CMD", "stand-in.cmd");
    assert_eq!(
        operator_program(&lab, &h),
        Some(lab.cwd.join("stand-in.cmd"))
    );
}

#[test]
fn agentspace_operator_cmd_never_names_the_runs_program() {
    // It names the operator under test, never the provisioning run's program (state.md, The
    // provisioning run's start): not with a program beside the CLI, not with none, and not beside
    // a variable that does name the run's program.
    let operator_under_test = |lab: &Lab| lab.root.join("under test").join("operator.exe");

    let (mut lab, h) = Lab::with_fake();
    let program = install_beside(&lab);
    let under_test = operator_under_test(&lab);
    lab.set_var("AGENTSPACE_OPERATOR_CMD", under_test.into_os_string());
    assert_eq!(operator_program(&lab, &h), Some(program));

    let (mut lab, h) = Lab::with_fake();
    let under_test = operator_under_test(&lab);
    lab.set_var(
        "AGENTSPACE_OPERATOR_CMD",
        under_test.clone().into_os_string(),
    );
    assert_ne!(
        operator_program(&lab, &h),
        Some(under_test),
        "With no program beside the CLI the answer is none, never AGENTSPACE_OPERATOR_CMD"
    );

    let (mut lab, h) = Lab::with_fake();
    install_beside(&lab);
    let stand_in = lab.root.join("stand-in.cmd");
    lab.set_var(
        "AGENTSPACE_OPERATOR_CMD",
        operator_under_test(&lab).into_os_string(),
    );
    lab.set_var(
        "AGENTSPACE_PROVISION_CMD",
        stand_in.clone().into_os_string(),
    );
    assert_eq!(operator_program(&lab, &h), Some(stand_in));
}

#[test]
fn create_answering_pending_records_a_pending_lease_and_asks_place_alone() {
    let (lab, h) = Lab::with_fake();
    h.script_place(Ok(agentspace_driver_api::Placed::Pending));
    let doc = lab
        .run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    assert_eq!(doc["state"], "pending");
    for absent in ["expires_at", "ssh", "reasons"] {
        assert!(
            doc.get(absent).is_none(),
            "A pending document has no {absent}"
        );
    }
    assert_eq!(h.calls().len(), 1, "Only place was asked: {:?}", h.calls());
    assert!(matches!(h.calls()[0], Call::Place { .. }));
    let record = lab.records().remove(0);
    assert_eq!(record["state"], json!("pending"));
    assert_eq!(
        record["driver"],
        json!("fake"),
        "The record carries the driver's name"
    );
}

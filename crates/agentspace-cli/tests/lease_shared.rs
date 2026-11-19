//! What the five lease commands share (E0.05-T03-S02, #0106): the lock, unreadable records, the
//! home folder, and the `internal` messages' rules.
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (Concurrency and atomicity; Exit codes and errors; Messages), docs/state.md
//! (The home folder; The lease state file; The lease state lock) and docs/driver.md, never from the
//! program's code (D31). Each test runs through `agentspace_cli::execute_in` in its own throwaway
//! folder under the temp folder.

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use lab::{
    ID1, ID2, ID3, ID4, Lab, MSG_NOT_VALID, Rec, msg_bad_project, msg_no_home, msg_not_found,
    msg_not_ready, msg_ttl_range,
};
use serde_json::json;

const ID5: &str = "0c7f2b5e-8a64-4f19-b3d2-6e1a9c4d8f07";

/// The three commands that change leases, with the arguments that reach their driver for ID1.
fn changing() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("create", vec!["lease", "create", "--project", "swim"]),
        ("renew", vec!["lease", "renew", ID1]),
        ("release", vec!["lease", "release", ID1]),
    ]
}

// ---- Unreadable records ------------------------------------------------------------------------

#[test]
fn a_changing_command_refuses_unreadable_records_before_it_asks_a_driver() {
    let good = Rec::ready(ID1, "swim").record();
    let mut bad_time = Rec::ready(ID2, "swim").record();
    bad_time["created_at"] = json!("2026-02-30T00:00:00Z");
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("not JSON", b"{".to_vec()),
        ("wrong version", br#"{"version":1,"leases":[]}"#.to_vec()),
        (
            "two records, one id",
            serde_json::to_vec(&json!({"version": 0, "leases": [good.clone(), good]})).unwrap(),
        ),
        (
            "a time that is no instant",
            serde_json::to_vec(&json!({"version": 0, "leases": [bad_time]})).unwrap(),
        ),
    ];
    for (what, bytes) in files {
        for (sub, line) in changing() {
            let (lab, h) = Lab::with_fake();
            lab.seed_raw(&bytes);
            lab.run(&line)
                .expect_error("internal", None, Some(MSG_NOT_VALID));
            assert!(h.calls().is_empty(), "{what}, {sub}: no driver is asked");
            assert_eq!(
                lab.state_bytes().as_deref(),
                Some(bytes.as_slice()),
                "{what}, {sub}"
            );
        }
    }
}

#[test]
fn a_changing_command_refuses_something_that_is_not_a_file_at_the_state_file() {
    for (sub, line) in changing() {
        let (lab, h) = Lab::with_fake();
        std::fs::create_dir_all(lab.state_file()).unwrap();
        let message = lab.run(&line).internal();
        assert!(
            message.starts_with("The lease records could not be read: ")
                && message.ends_with(". Nothing was changed."),
            "{sub}: {message}"
        );
        assert!(h.calls().is_empty(), "{sub}");
    }
}

// ---- The lock ----------------------------------------------------------------------------------

#[test]
fn a_changing_command_that_cannot_open_the_lock_fails_at_once_and_does_not_call_it_busy() {
    for (sub, line) in changing() {
        let (lab, h) = Lab::with_fake();
        lab.seed(&[Rec::ready(ID1, "swim")]);
        let before = lab.state_bytes();
        std::fs::create_dir_all(lab.lock_file()).unwrap();
        let start = std::time::Instant::now();
        let message = lab.run(&line).internal();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "{sub}: it waited"
        );
        assert!(
            message.starts_with("The lease records could not be locked: ")
                && message.ends_with(". Nothing was changed."),
            "{sub}: {message}"
        );
        assert!(h.calls().is_empty(), "{sub}");
        assert_eq!(lab.state_bytes(), before, "{sub}");
    }
}

#[cfg(windows)]
fn busy(line: &[&str], sub: &str) {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let before = lab.state_bytes();
    let held = lab::LockHold::take(&lab);
    let start = std::time::Instant::now();
    let ran = lab.run(line);
    let took = start.elapsed();
    ran.expect_error(
        "internal",
        None,
        Some("The lease records are busy: another agentspace lease command held them for 30 seconds. Try again."),
    );
    assert!(
        took >= std::time::Duration::from_secs(29),
        "{sub}: it gave up after {took:?}, and MUST wait up to 30 seconds"
    );
    assert!(
        took <= std::time::Duration::from_secs(40),
        "{sub}: it waited {took:?}, and MUST NOT wait longer than 30 seconds"
    );
    assert!(h.calls().is_empty(), "{sub}: no driver is asked");
    assert_eq!(lab.state_bytes(), before, "{sub}: nothing changed");
    // get and list do not wait, while the lock is held.
    let start = std::time::Instant::now();
    assert_eq!(lab.run(&["lease", "get", ID1]).lease_doc()["id"], ID1);
    assert_eq!(lab.run(&["lease", "list"]).list_ids(), vec![ID1]);
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
    drop(held);
}

#[cfg(windows)]
#[test]
fn create_waits_30_seconds_for_the_lock_and_then_fails_busy() {
    busy(&["lease", "create", "--project", "swim"], "create");
}

#[cfg(windows)]
#[test]
fn renew_waits_30_seconds_for_the_lock_and_then_fails_busy() {
    busy(&["lease", "renew", ID1], "renew");
}

#[cfg(windows)]
#[test]
fn release_waits_30_seconds_for_the_lock_and_then_fails_busy() {
    busy(&["lease", "release", ID1], "release");
}

#[cfg(windows)]
#[test]
fn a_command_that_waits_for_the_lock_goes_on_when_it_is_freed_in_time() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let held = lab::LockHold::take(&lab);
    let freer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        drop(held);
    });
    let doc = lab.run(&["lease", "renew", ID1]).lease_doc();
    freer.join().unwrap();
    assert_eq!(doc["id"], ID1);
    assert_eq!(h.calls().len(), 2);
}

// ---- No home -----------------------------------------------------------------------------------

fn seeded_lab_without_home(empty: bool) -> (Lab, fake_driver::Handle) {
    let (mut lab, h) = Lab::with_fake();
    lab.seed(&[
        Rec::ready(ID1, "swim"),
        Rec::expired(ID2, "swim"),
        Rec::pending(ID3, "swim"),
        Rec::unsatisfiable(ID4, "swim"),
        Rec::released(ID5, "swim"),
    ]);
    if empty {
        lab.set_var("USERPROFILE", "");
    } else {
        lab.unset_var("USERPROFILE");
    }
    (lab, h)
}

#[test]
fn with_a_state_folder_given_only_what_needs_the_home_fails_with_no_home() {
    for empty in [false, true] {
        let (lab, h) = seeded_lab_without_home(empty);
        let before = lab.state_bytes();

        // Commands that touch no file under the home still run.
        assert_eq!(lab.run(&["lease", "get", ID1]).lease_doc()["id"], ID1);
        assert_eq!(lab.run(&["lease", "list"]).list_ids().len(), 5);

        // The records are read first, so these come before the no-home failure.
        lab.run(&["lease", "renew", ID5.replace('0', "1").as_str()])
            .expect_lease_error(
                "not_found",
                &ID5.replace('0', "1"),
                None,
                &msg_not_found(&ID5.replace('0', "1")),
            );
        lab.run(&["lease", "renew", ID2]).expect_lease_error(
            "invalid_state",
            ID2,
            Some("expired"),
            &msg_not_ready(ID2, "expired"),
        );
        // So do the value errors, which come before the records.
        lab.run(&["lease", "renew", ID1, "--ttl", "48h"])
            .expect_error("out_of_range", Some("ttl"), Some(&msg_ttl_range("48h")));
        lab.run(&["lease", "create", "--project", "Bad_Name"])
            .expect_error(
                "invalid_value",
                Some("project"),
                Some(&msg_bad_project("Bad_Name")),
            );

        // What needs the home, with a driver present, fails before the driver is asked.
        // ARCHITECT-QUESTION answered by the dev lead (FB-22): the first point that needs a home.
        lab.run(&["lease", "create", "--project", "swim"])
            .expect_error("internal", None, Some(&msg_no_home("create")));
        lab.run(&["lease", "renew", ID1]).expect_error(
            "internal",
            None,
            Some(&msg_no_home("renew")),
        );
        lab.run(&["lease", "release", ID1]).expect_error(
            "internal",
            None,
            Some(&msg_no_home("release")),
        );
        lab.run(&["lease", "release", ID5]).expect_error(
            "internal",
            None,
            Some(&msg_no_home("release")),
        );
        assert!(h.calls().is_empty(), "No driver is asked without a home");
        assert_eq!(lab.state_bytes(), before, "Nothing is written");

        // A release of a pending or an unsatisfiable lease touches no file, and succeeds.
        let doc = lab.run(&["lease", "release", ID3]).lease_doc();
        assert_eq!(doc["state"], "released");
        let doc = lab.run(&["lease", "release", ID4]).lease_doc();
        assert_eq!(doc["state"], "unsatisfiable");
        assert!(h.calls().is_empty());
    }
}

#[test]
fn with_the_default_state_folder_every_command_fails_with_no_home_and_writes_nothing() {
    for empty in [false, true] {
        let mut lab = Lab::new();
        let h = lab.fake("fake");
        lab.unset_var("AGENTSPACE_STATE_DIR");
        if empty {
            lab.set_var("USERPROFILE", "");
        } else {
            lab.unset_var("USERPROFILE");
        }
        for sub in ["get", "list", "create", "renew", "release"] {
            let line: Vec<&str> = match sub {
                "create" => vec!["lease", "create", "--project", "swim"],
                "list" => vec!["lease", "list"],
                other => vec!["lease", other, ID1],
            };
            lab.run(&line)
                .expect_error("internal", None, Some(&msg_no_home(sub)));
        }
        assert!(h.calls().is_empty());
        // Nothing is made anywhere: the throwaway folder holds its home and its working folder only.
        assert!(!lab.state.exists() && !lab.ssh.exists());
        assert_eq!(std::fs::read_dir(&lab.cwd).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&lab.home).unwrap().count(), 0);

        // A value error still comes first.
        lab.run(&["lease", "get", "not-an-id"]).expect_error(
            "invalid_value",
            Some("id"),
            Some(&lab::msg_bad_id("not-an-id")),
        );
    }
}

#[test]
fn with_no_driver_and_no_home_the_default_state_folder_still_fails_with_no_home_first() {
    let mut lab = Lab::new();
    lab.unset_var("AGENTSPACE_STATE_DIR");
    lab.unset_var("USERPROFILE");
    lab.run(&["lease", "create", "--project", "swim"])
        .expect_error("internal", None, Some(&msg_no_home("create")));
}

// ---- Messages ----------------------------------------------------------------------------------

#[test]
fn a_changing_commands_internal_messages_name_no_path_and_no_throwaway_folder() {
    // `expect_error` checks every internal message for an absolute path, the folder and the user's
    // name; this runs the messages the CLI composes itself through it.
    let (lab, _h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    std::fs::create_dir_all(lab.lock_file()).unwrap();
    for (_, line) in changing() {
        let message = lab.run(&line).internal();
        assert!(
            !message.contains(lab.root.to_string_lossy().as_ref()),
            "{message}"
        );
    }
}

// ---- A working folder that cannot be read ------------------------------------------------------

/// lease-api.md, Messages: a relative folder in the environment with no working folder to make it
/// full against (architect, 2026-11-19).
fn msg_no_cwd(sub: &str) -> String {
    format!(
        "agentspace lease {sub} failed: The working folder cannot be read, so a relative folder in the environment cannot be made a full path. Nothing was changed."
    )
}

fn line_for(sub: &str) -> Vec<&str> {
    match sub {
        "create" => vec!["lease", "create", "--project", "swim"],
        "list" => vec!["lease", "list"],
        other => vec!["lease", other, ID1],
    }
}

#[test]
fn with_no_working_folder_a_relative_folder_that_is_needed_fails_and_writes_nothing() {
    // A relative name no other test uses: with no working folder it MUST NOT be made anywhere.
    let rel = format!("agentspace-no-cwd-test-{}", std::process::id());
    // Which variable is relative, and which commands need its folder (the first point a command
    // needs a folder, gap FB-22): the rest run.
    let all = ["get", "list", "create", "renew", "release"];
    let cases: [(&str, bool, &[&str]); 4] = [
        ("AGENTSPACE_STATE_DIR", false, &all),
        // With the state folder unset, the default one is under the relative home.
        ("USERPROFILE", true, &all),
        // Renew reads no AGENTSPACE_SSH_DIR (lease-api.md, Environment).
        ("AGENTSPACE_SSH_DIR", false, &["create", "release"]),
        // An absolute state folder: get and list need no home.
        ("USERPROFILE", false, &["create", "renew", "release"]),
    ];
    for (var, unset_state, needed) in cases {
        let mut lab = Lab::new();
        let h = lab.fake("fake");
        if var != "AGENTSPACE_STATE_DIR" {
            lab.seed(&[Rec::ready(ID1, "swim")]);
        }
        lab.set_var(var, rel.as_str());
        if unset_state {
            lab.unset_var("AGENTSPACE_STATE_DIR");
        }
        lab.lose_cwd();
        for sub in all {
            let what = format!("{var} relative, {sub}");
            let before = lab.state_bytes();
            h.clear();
            let ran = lab.run(&line_for(sub));
            if needed.contains(&sub) {
                ran.expect_error("internal", None, Some(&msg_no_cwd(sub)));
                assert!(h.calls().is_empty(), "{what}: no driver is asked");
                assert_eq!(lab.state_bytes(), before, "{what}: nothing is written");
            } else {
                assert_eq!(ran.code, 0, "{what}: {}", ran.stderr_text());
            }
        }
        assert!(
            !std::path::Path::new(&rel).exists(),
            "{var}: the relative folder was made against some other folder"
        );
    }
}

#[test]
fn with_no_working_folder_absolute_folders_still_run() {
    let mut lab = Lab::new();
    let h = lab.fake("fake");
    lab.lose_cwd();
    lab.script_ids(&[ID2]);
    lab.seed(&[Rec::ready(ID1, "swim")]);
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    lab.run(&["lease", "get", ID1]).lease_doc();
    lab.run(&["lease", "list"]).list_doc();
    lab.run(&["lease", "renew", ID1]).lease_doc();
    let doc = lab.run(&["lease", "release", ID1]).lease_doc();
    assert_eq!(doc["state"], "released");
    assert_eq!(
        h.contexts()[0].ssh_dir,
        lab.ssh,
        "The given ssh folder is used"
    );
}

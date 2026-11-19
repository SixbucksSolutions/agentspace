//! `agentspace lease create` (E0.05-T03-S02, #0106).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (Fields, Concurrency and atomicity, `create`, Exit codes and errors,
//! Messages), docs/driver.md (Ordering and concurrency, The driver's name, Failure) and docs/state.md,
//! never from the program's code (D31). The driver is a fake that logs every call, and each test
//! runs through `agentspace_cli::execute_in` in its own throwaway folder under the temp folder.

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use agentspace_driver_api::Placed;
use common::{
    MSG_NO_PROJECT, msg_empty_value, msg_needs_value, msg_twice, msg_unexpected,
    msg_unknown_option_sub,
};
use fake_driver::{Call, alias_of, fail, fail_with, reason};
use lab::{
    ID1, ID2, ID3, Lab, MSG_NO_DRIVER, NOW, Rec, count_of, fmt_time, msg_bad_platform,
    msg_bad_profile, msg_bad_project, msg_bad_size, msg_ttl_m, msg_ttl_no_unit,
    msg_ttl_not_duration, msg_ttl_range, same_alias_id,
};
use serde_json::{Value, json};

/// A pending or unsatisfiable result the records refuse: nothing was issued, so nothing is undone.
const MSG_NOT_VALID_WRITE: &str =
    "The lease records could not be written: the change is not valid. Nothing was changed.";

/// A ready place whose record write failed and whose undo (the driver's release) succeeded
/// (architect ruling, 2026-11-19, main bfb9a1b).
const MSG_NOT_VALID_UNDONE: &str = "agentspace lease create failed: The lease records could not be written: the change is not valid. No lease was recorded.";

fn place_call(
    id: &str,
    project: &str,
    platform: &str,
    profile: &str,
    size: &str,
    ttl: u32,
) -> Call {
    Call::Place {
        id: id.to_string(),
        project: project.to_string(),
        platform: platform.to_string(),
        profile: profile.to_string(),
        size: size.to_string(),
        ttl_seconds: ttl,
    }
}

fn default_place(id: &str, project: &str) -> Call {
    place_call(id, project, "linux/arm64", "agent-linux", "m", 28800)
}

fn release_call(id: &str, project: &str) -> Call {
    Call::Release {
        id: id.to_string(),
        project: project.to_string(),
    }
}

fn expected_ready(id: &str, project: &str, ttl: u32, expires_at: i64) -> Value {
    json!({
        "id": id,
        "project": project,
        "state": "ready",
        "platform": "linux/arm64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": ttl,
        "created_at": fmt_time(NOW),
        "expires_at": fmt_time(expires_at),
        "ssh": {"host_alias": alias_of(project, id), "user": "ubuntu"},
    })
}

fn with_driver_member(doc: &Value, driver: &str) -> Value {
    let mut v = doc.clone();
    v["driver"] = json!(driver);
    v
}

// ---- Success -----------------------------------------------------------------------------------

#[test]
fn create_places_a_lease_and_records_it_with_the_drivers_name() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    let doc = ran.lease_doc();
    let want = expected_ready(ID1, "swim", 28800, NOW + 28800);
    assert_eq!(doc, want);
    assert_eq!(h.calls(), [default_place(ID1, "swim")]);
    assert_eq!(lab.records(), [with_driver_member(&want, "fake")]);
    assert_eq!(ran.now_calls, 1, "The clock is read once per command");
    assert_eq!(ran.draws, 1);
    let bytes = lab.state_bytes().expect("The state file is written");
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        text.ends_with('\n') && text.matches('\n').count() == 1,
        "{text:?}"
    );
    assert!(common::is_compact(text.trim_end()), "{text:?}");
    assert_eq!(
        std::fs::metadata(lab.lock_file())
            .expect("The lock file stays")
            .len(),
        0,
        "The lock file is empty and never deleted"
    );
}

#[test]
fn create_normalizes_every_value_before_it_asks_the_driver() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    let ran = lab.run(&[
        "lease",
        "create",
        "--project",
        " Swim-Coach\n",
        "--platform",
        " Linux/AARCH64/v8 ",
        "--profile",
        "Agent-Linux@03",
        "--size",
        " M ",
        "--ttl",
        "1h 30m",
    ]);
    let doc = ran.lease_doc();
    assert_eq!(
        h.calls(),
        [place_call(
            ID1,
            "swim-coach",
            "linux/arm64",
            "agent-linux@3",
            "m",
            5400
        )]
    );
    assert_eq!(doc["project"], "swim-coach");
    assert_eq!(doc["platform"], "linux/arm64");
    assert_eq!(doc["profile"], "agent-linux@3");
    assert_eq!(doc["size"], "m");
    assert_eq!(doc["ttl_seconds"], 5400);
    // The record carries the same normalized values.
    let rec = lab.record_of(ID1).unwrap();
    assert_eq!(rec["platform"], "linux/arm64");
    assert_eq!(rec["profile"], "agent-linux@3");
    assert_eq!(rec["ttl_seconds"], 5400);
}

#[test]
fn create_accepts_the_ttl_limits_and_passes_them_unclamped() {
    for (ttl, seconds) in [
        ("1m", 60),
        ("24h", 86400),
        ("1440m", 86400),
        ("PT24H", 86400),
        ("60s", 60),
        ("P1D", 86400),
    ] {
        let (lab, h) = Lab::with_fake();
        let ran = lab.run(&["lease", "create", "--project", "swim", "--ttl", ttl]);
        assert_eq!(ran.lease_doc()["ttl_seconds"], seconds, "{ttl}");
        match &h.calls()[0] {
            Call::Place { ttl_seconds, .. } => assert_eq!(*ttl_seconds, seconds, "{ttl}"),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn create_prints_what_the_driver_answers_and_not_what_it_computes() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    h.script_place(Ok(Placed::Ready {
        host_alias: "agentspace-swim-0badcafe".to_string(),
        user: "agent_1".to_string(),
        expires_at: NOW + 28800 + 7,
    }));
    let doc = lab
        .run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    assert_eq!(doc["expires_at"], fmt_time(NOW + 28800 + 7));
    assert_eq!(
        doc["ssh"],
        json!({"host_alias": "agentspace-swim-0badcafe", "user": "agent_1"})
    );
    assert_eq!(doc["created_at"], fmt_time(NOW));
    let rec = lab.record_of(ID1).unwrap();
    assert_eq!(rec["ssh"], doc["ssh"]);
    assert_eq!(rec["expires_at"], doc["expires_at"]);
}

#[test]
fn create_records_a_pending_lease() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    h.script_place(Ok(Placed::Pending));
    let doc = lab
        .run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    let want = json!({
        "id": ID1, "project": "swim", "state": "pending", "platform": "linux/arm64",
        "profile": "agent-linux", "size": "m", "ttl_seconds": 28800, "created_at": fmt_time(NOW),
    });
    assert_eq!(doc, want);
    assert_eq!(lab.records(), [with_driver_member(&want, "fake")]);
    assert_eq!(
        h.calls().len(),
        1,
        "Nothing is released for a pending lease"
    );
}

#[test]
fn create_records_an_unsatisfiable_lease_with_exit_0() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    let reasons = vec![
        reason(
            "unsupported_platform",
            "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only.",
        ),
        reason(
            "unsupported_size",
            "Size 'xl' is not supported; v0 supports size m only.",
        ),
    ];
    h.script_place(Ok(Placed::Unsatisfiable { reasons }));
    let ran = lab.run(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--platform",
        "Windows/AMD64",
        "--size",
        "xl",
    ]);
    let doc = ran.lease_doc();
    let want = json!({
        "id": ID1, "project": "swim", "state": "unsatisfiable", "platform": "windows/amd64",
        "profile": "agent-linux", "size": "xl", "ttl_seconds": 28800, "created_at": fmt_time(NOW),
        "reasons": [
            {"code": "unsupported_platform", "message": "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only."},
            {"code": "unsupported_size", "message": "Size 'xl' is not supported; v0 supports size m only."},
        ],
    });
    assert_eq!(doc, want);
    assert_eq!(lab.records(), [with_driver_member(&want, "fake")]);
    assert_eq!(
        h.calls().len(),
        1,
        "Nothing is released for an unsatisfiable lease"
    );
}

#[test]
fn create_appends_records_in_creation_order_and_keeps_the_others() {
    let (mut lab, _h) = Lab::with_fake();
    let old = Rec::released(ID3, "other");
    lab.seed(std::slice::from_ref(&old));
    lab.script_ids(&[ID1, ID2]);
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    let recs = lab.records();
    assert_eq!(recs.len(), 3);
    assert_eq!(recs[0], old.record(), "An older record is untouched");
    assert_eq!(recs[1]["id"], ID1);
    assert_eq!(recs[2]["id"], ID2);
}

#[test]
fn create_never_prints_the_drivers_name() {
    let mut lab = Lab::new();
    let _h = lab.fake("backend-name-for-test");
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    ran.lease_doc();
    assert!(!ran.stdout_text().contains("backend-name-for-test"));
    assert!(ran.stderr.is_empty());
    let id = lab.records()[0]["id"].as_str().unwrap().to_string();
    assert_eq!(
        lab.record_of(&id).unwrap()["driver"],
        "backend-name-for-test"
    );
}

#[test]
fn create_with_the_default_state_folder_keeps_its_files_under_the_homes_dot_agentspace() {
    let (mut lab, _h) = Lab::with_fake();
    lab.unset_var("AGENTSPACE_STATE_DIR");
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    let dir = lab.home.join(".agentspace");
    assert!(dir.join("leases.json").is_file());
    assert!(dir.join("leases.lock").is_file());
    assert!(!lab.state.exists(), "The other state folder is not used");
}

// ---- Usage and value errors --------------------------------------------------------------------

#[test]
fn create_usage_errors_ask_no_driver_and_write_nothing() {
    let (lab, h) = Lab::with_fake();
    let run = |args: &[&str]| {
        let mut line = vec!["lease", "create"];
        line.extend_from_slice(args);
        lab.run(&line)
    };
    run(&[]).expect_error("usage", Some("project"), Some(MSG_NO_PROJECT));
    run(&["--project"]).expect_error(
        "usage",
        Some("project"),
        Some(&msg_needs_value("project", "create")),
    );
    run(&["--project", ""]).expect_error(
        "usage",
        Some("project"),
        Some(&msg_empty_value("project", "create")),
    );
    run(&["--project", "a", "--project", "b"]).expect_error(
        "usage",
        Some("project"),
        Some(&msg_twice("project")),
    );
    run(&["--project", "a", "--ttl"]).expect_error(
        "usage",
        Some("ttl"),
        Some(&msg_needs_value("ttl", "create")),
    );
    run(&["--project", "a", "--size", ""]).expect_error(
        "usage",
        Some("size"),
        Some(&msg_empty_value("size", "create")),
    );
    run(&["--project", "a", "--platform", "x", "--platform", "y"]).expect_error(
        "usage",
        Some("platform"),
        Some(&msg_twice("platform")),
    );
    run(&["--project", "a", "--bogus", "x"]).expect_error(
        "usage",
        None,
        Some(&msg_unknown_option_sub("--bogus", "create")),
    );
    run(&["--project", "a", "extra"]).expect_error(
        "usage",
        None,
        Some(&msg_unexpected("extra", "create")),
    );
    assert!(h.calls().is_empty());
    assert!(!lab.state.exists(), "A usage error makes no state folder");
}

#[test]
fn create_reports_the_first_bad_value_in_the_tables_order() {
    let (lab, h) = Lab::with_fake();
    let ttl_range = msg_ttl_range("48h");
    #[allow(clippy::type_complexity)]
    let cases: Vec<(Vec<&str>, &str, &str, String)> = vec![
        (
            vec![
                "--project",
                "Bad_Name",
                "--platform",
                "linux",
                "--profile",
                "-x",
                "--size",
                "toolongsize",
                "--ttl",
                "8",
            ],
            "invalid_value",
            "project",
            msg_bad_project("Bad_Name"),
        ),
        (
            vec![
                "--project",
                "ok",
                "--platform",
                "linux",
                "--profile",
                "-x",
                "--size",
                "toolongsize",
                "--ttl",
                "8",
            ],
            "invalid_value",
            "platform",
            msg_bad_platform("linux"),
        ),
        (
            vec![
                "--project",
                "ok",
                "--profile",
                "-x",
                "--size",
                "toolongsize",
                "--ttl",
                "8",
            ],
            "invalid_value",
            "profile",
            msg_bad_profile("-x"),
        ),
        (
            vec!["--project", "ok", "--size", "toolongsize", "--ttl", "8"],
            "invalid_value",
            "size",
            msg_bad_size("toolongsize"),
        ),
        (
            vec!["--project", "ok", "--ttl", "8"],
            "invalid_value",
            "ttl",
            msg_ttl_no_unit("8"),
        ),
        (
            vec!["--project", "ok", "--ttl", "8M"],
            "invalid_value",
            "ttl",
            msg_ttl_m("8M"),
        ),
        (
            vec!["--project", "ok", "--ttl", "1.5h"],
            "invalid_value",
            "ttl",
            msg_ttl_not_duration("1.5h"),
        ),
        // A value that is out of range comes after every value that is malformed and before none
        // that comes later in the table's order.
        (
            vec!["--project", "ok", "--platform", "linux", "--ttl", "48h"],
            "invalid_value",
            "platform",
            msg_bad_platform("linux"),
        ),
        (
            vec!["--project", "ok", "--size", "toolongsize", "--ttl", "48h"],
            "invalid_value",
            "size",
            msg_bad_size("toolongsize"),
        ),
        (
            vec!["--project", "ok", "--ttl", "48h"],
            "out_of_range",
            "ttl",
            ttl_range,
        ),
        (
            vec!["--project", "ok", "--ttl", "59s"],
            "out_of_range",
            "ttl",
            msg_ttl_range("59s"),
        ),
        (
            vec!["--project", "ok", "--ttl", "24h1s"],
            "out_of_range",
            "ttl",
            msg_ttl_range("24h1s"),
        ),
        (
            vec!["--project", "ok", "--ttl", "0m"],
            "out_of_range",
            "ttl",
            msg_ttl_range("0m"),
        ),
        (
            vec!["--project", "ok", "--ttl", "99999999999999999999h"],
            "out_of_range",
            "ttl",
            msg_ttl_range("99999999999999999999h"),
        ),
    ];
    for (args, code, field, message) in cases {
        let mut line = vec!["lease", "create"];
        line.extend_from_slice(&args);
        lab.run(&line)
            .expect_error(code, Some(field), Some(&message));
    }
    assert!(h.calls().is_empty(), "No driver is asked for a bad value");
    assert!(!lab.state.exists(), "A bad value makes no state folder");
}

// ---- Failures of the driver --------------------------------------------------------------------

#[test]
fn a_failed_place_is_internal_with_the_drivers_message_after_the_prefix() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    lab.seed(&[Rec::released(ID2, "other")]);
    let before = lab.state_bytes();
    h.script_place(Err(fail(
        "The lease's credential could not be issued. Nothing was changed.",
    )));
    lab.run(&["lease", "create", "--project", "swim"]).expect_error(
        "internal",
        None,
        Some("agentspace lease create failed: The lease's credential could not be issued. Nothing was changed."),
    );
    assert_eq!(
        h.calls(),
        [default_place(ID1, "swim")],
        "A failed place is not released"
    );
    assert_eq!(lab.state_bytes(), before, "No lease is recorded");
}

#[test]
fn a_failed_place_prints_each_leftover_once() {
    // The driver's message names the leftovers; the CLI prints it after its prefix and adds nothing.
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    let l1 = "~/.agentspace/leases/3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f/id_ed25519";
    let l2 = "the host alias file agentspace-swim-3f2a9c1e";
    let message = format!(
        "The lease's credential could not be issued, and these could not be removed: {l1}, {l2}."
    );
    h.script_place(Err(fail_with(&message, &[l1, l2])));
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    let text = ran.internal();
    assert_eq!(text, format!("agentspace lease create failed: {message}"));
    assert_eq!(count_of(&text, l1), 1);
    assert_eq!(count_of(&text, l2), 1);
    assert!(lab.records().is_empty());
}

#[test]
fn a_drivers_message_is_printed_with_its_control_characters_escaped_and_nothing_more() {
    let (lab, h) = Lab::with_fake();
    h.script_place(Err(fail("Cause\ngone\u{7f}\u{1}end\u{0}")));
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    let text = ran.internal();
    assert_eq!(
        text,
        "agentspace lease create failed: Cause\\u000agone\\u007f\\u0001end\\u0000"
    );
    // In the JSON document the backslash is itself escaped.
    assert!(
        ran.stderr_text().contains("Cause\\\\u000agone"),
        "{}",
        ran.stderr_text()
    );
}

// ---- No driver ---------------------------------------------------------------------------------

#[test]
fn create_with_no_driver_says_so_and_records_nothing() {
    let lab = Lab::new();
    lab.run(&["lease", "create", "--project", "swim"])
        .expect_error("internal", None, Some(MSG_NO_DRIVER));
    assert!(lab.records().is_empty());
}

#[test]
fn with_no_driver_a_bad_value_or_unreadable_records_come_first() {
    let lab = Lab::new();
    lab.run(&["lease", "create", "--project", "swim", "--ttl", "48h"])
        .expect_error("out_of_range", Some("ttl"), Some(&msg_ttl_range("48h")));
    lab.seed_raw(b"{");
    lab.run(&["lease", "create", "--project", "swim"])
        .expect_error("internal", None, Some(lab::MSG_NOT_VALID));
}

#[test]
fn with_no_driver_a_missing_home_does_not_matter_when_the_state_folder_is_given() {
    // The no-driver failure never reaches a point that needs the home folder.
    let mut lab = Lab::new();
    lab.unset_var("USERPROFILE");
    lab.run(&["lease", "create", "--project", "swim"])
        .expect_error("internal", None, Some(MSG_NO_DRIVER));
}

// ---- A record write that fails after a place ---------------------------------------------------

fn ready_with(user: &str, alias: &str) -> Placed {
    Placed::Ready {
        host_alias: alias.to_string(),
        user: user.to_string(),
        expires_at: NOW + 28800,
    }
}

#[test]
fn a_ready_result_the_records_refuse_is_undone_with_a_release_and_reports_the_first_failure() {
    let good_alias = alias_of("swim", ID1);
    let long_user = "a".repeat(33);
    let results = vec![
        ("an uppercase user", ready_with("Ubuntu", &good_alias)),
        ("an empty user", ready_with("", &good_alias)),
        (
            "a user of 33 characters",
            ready_with(&long_user, &good_alias),
        ),
        ("an empty alias", ready_with("ubuntu", "")),
        (
            "an alias in the wrong form",
            ready_with("ubuntu", "not-an-alias"),
        ),
    ];
    for (what, result) in results {
        let (mut lab, h) = Lab::with_fake();
        lab.script_ids(&[ID1]);
        h.script_place(Ok(result));
        #[cfg(windows)]
        h.set_probe(lab::lock_probe(lab.lock_file()));
        lab.run(&["lease", "create", "--project", "swim"])
            .expect_error("internal", None, Some(MSG_NOT_VALID_UNDONE));
        assert_eq!(
            h.calls(),
            [default_place(ID1, "swim"), release_call(ID1, "swim")],
            "{what}"
        );
        #[cfg(windows)]
        assert_eq!(
            h.probes(),
            ["held", "held"],
            "{what}: the lock is held at the place and still at the undo's release"
        );
        assert!(lab.records().is_empty(), "{what}: no lease is recorded");
    }
}

#[test]
fn the_undo_that_leaves_files_names_each_of_them_once_after_the_first_failure() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    h.script_place(Ok(ready_with("Ubuntu", &alias_of("swim", ID1))));
    let l1 = "~/.agentspace/leases/3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f/id_ed25519";
    let l2 = "the host alias file agentspace-swim-3f2a9c1e";
    h.script_release(Err(fail_with("Cleanup exploded.", &[l1, l2])));
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    let text = ran.internal();
    assert_eq!(
        text,
        format!(
            "agentspace lease create failed: The lease records could not be written: the change is not valid. No lease was recorded, and these could not be removed: {l1}, {l2}."
        )
    );
    assert_eq!(count_of(&text, l1), 1);
    assert_eq!(count_of(&text, l2), 1);
    assert!(
        !text.contains("Cleanup exploded"),
        "The release's own message does not replace the first failure"
    );
    assert_eq!(
        h.calls(),
        [default_place(ID1, "swim"), release_call(ID1, "swim")]
    );
    assert!(
        lab.records().is_empty(),
        "No lease is recorded, so none is ready"
    );
}

#[test]
fn an_undo_that_fails_and_names_nothing_says_that_some_files_remain() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    h.script_place(Ok(ready_with("Ubuntu", &alias_of("swim", ID1))));
    h.script_release(Err(fail("Cleanup exploded.")));
    lab.run(&["lease", "create", "--project", "swim"]).expect_error(
        "internal",
        None,
        Some("agentspace lease create failed: The lease records could not be written: the change is not valid. No lease was recorded, but some of the lease's files could not be removed."),
    );
}

#[test]
fn a_pending_or_unsatisfiable_result_the_records_refuse_releases_nothing() {
    let bad = vec![
        (
            "no reasons",
            Placed::Unsatisfiable {
                reasons: Vec::new(),
            },
        ),
        (
            "a reason code that is not one",
            Placed::Unsatisfiable {
                reasons: vec![reason("bogus_code", "Nope.")],
            },
        ),
        (
            "a reason message that starts with a space",
            Placed::Unsatisfiable {
                reasons: vec![reason("no_capacity", " Nope.")],
            },
        ),
        (
            "the same code twice",
            Placed::Unsatisfiable {
                reasons: vec![reason("no_capacity", "One."), reason("no_capacity", "Two.")],
            },
        ),
    ];
    for (what, result) in bad {
        let (mut lab, h) = Lab::with_fake();
        lab.script_ids(&[ID1]);
        h.script_place(Ok(result));
        lab.run(&["lease", "create", "--project", "swim"])
            .expect_error("internal", None, Some(MSG_NOT_VALID_WRITE));
        assert_eq!(
            h.calls(),
            [default_place(ID1, "swim")],
            "{what}: only a ready place is released"
        );
        assert!(lab.records().is_empty(), "{what}");
    }
}

#[cfg(windows)]
#[test]
fn a_record_that_cannot_be_written_after_a_ready_place_is_undone_too() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    lab.seed(&[Rec::released(ID2, "other")]);
    std::fs::write(lab.lock_file(), b"").unwrap();
    let before = lab.state_bytes();
    h.set_probe(lab::lock_probe(lab.lock_file()));
    let _deny = lab::DenyWrite::new(&lab.state);
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    let text = ran.internal();
    assert!(
        text.starts_with(
            "agentspace lease create failed: The lease records could not be written: "
        ) && text.ends_with(". No lease was recorded."),
        "{text}"
    );
    assert_eq!(
        h.calls(),
        [default_place(ID1, "swim"), release_call(ID1, "swim")]
    );
    assert_eq!(
        h.probes(),
        ["held", "held"],
        "The lock is held at the place and still at the undo's release"
    );
    assert_eq!(lab.state_bytes(), before);

    // And when that release leaves files, the first failure is reported and they are named.
    h.clear();
    let l1 = "~/.agentspace/leases/x/id_ed25519";
    h.script_release(Err(fail_with("Cleanup exploded.", &[l1])));
    let text = lab
        .run(&["lease", "create", "--project", "swim"])
        .internal();
    assert!(
        text.starts_with(
            "agentspace lease create failed: The lease records could not be written: "
        ) && text.ends_with(&format!(
            ". No lease was recorded, and these could not be removed: {l1}."
        )),
        "{text}"
    );
    assert_eq!(count_of(&text, l1), 1);
}

// ---- The alias redraw --------------------------------------------------------------------------

#[test]
fn create_draws_again_while_a_kept_leases_alias_is_the_same_in_any_state() {
    let kept = [
        ("ready", Rec::ready(ID1, "swim")),
        ("expired", Rec::expired(ID1, "swim")),
        ("pending", Rec::pending(ID1, "swim")),
        ("released", Rec::released(ID1, "swim")),
        ("unsatisfiable", Rec::unsatisfiable(ID1, "swim")),
    ];
    for (what, rec) in kept {
        let (mut lab, h) = Lab::with_fake();
        lab.seed(&[rec]);
        let clash = same_alias_id(ID1);
        lab.script_ids(&[&clash, ID2]);
        let doc = lab
            .run(&["lease", "create", "--project", "swim"])
            .lease_doc();
        assert_eq!(doc["id"], ID2, "{what}");
        assert_eq!(
            h.calls(),
            [default_place(ID2, "swim")],
            "{what}: the driver gets the free ID"
        );
    }
}

#[test]
fn create_draws_as_many_times_as_it_takes() {
    let (mut lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let clash = same_alias_id(ID1);
    let clash2 = format!("{}-3333-4333-a333-333333333333", &ID1[..8]);
    lab.script_ids(&[&clash, &clash2, &clash, ID2]);
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    assert_eq!(ran.lease_doc()["id"], ID2);
    assert_eq!(ran.draws, 4);
    assert_eq!(h.calls(), [default_place(ID2, "swim")]);
}

#[test]
fn an_alias_that_differs_in_project_or_in_its_first_8_digits_is_not_a_clash() {
    let (mut lab, h) = Lab::with_fake();
    // Same first 8 digits, but another project: another alias.
    lab.seed(&[Rec::ready(ID1, "other")]);
    let same_prefix = same_alias_id(ID1);
    lab.script_ids(&[&same_prefix]);
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    assert_eq!(ran.lease_doc()["id"], same_prefix.as_str());
    assert_eq!(ran.draws, 1);
    assert_eq!(h.calls(), [default_place(&same_prefix, "swim")]);
    // The same project, but other first 8 digits.
    let (mut lab2, h2) = Lab::with_fake();
    lab2.seed(&[Rec::ready(ID1, "swim")]);
    lab2.script_ids(&[ID2]);
    let ran = lab2.run(&["lease", "create", "--project", "swim"]);
    assert_eq!(ran.lease_doc()["id"], ID2);
    assert_eq!(ran.draws, 1);
    assert_eq!(h2.calls(), [default_place(ID2, "swim")]);
}

#[test]
fn create_gives_up_with_internal_when_every_draw_collides() {
    let (mut lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let before = lab.state_bytes();
    lab.always_id(&same_alias_id(ID1));
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    let text = ran.internal();
    assert!(
        text.starts_with("agentspace lease create failed: "),
        "{text}"
    );
    assert!(
        ran.draws >= 100,
        "It drew {} times, and MUST draw at least 100",
        ran.draws
    );
    assert!(ran.draws <= lab::GUARD, "It never gave up");
    assert!(h.calls().is_empty(), "No driver is asked");
    assert_eq!(lab.state_bytes(), before);
}

// ---- The lock, the context and the folders -----------------------------------------------------

#[cfg(windows)]
#[test]
fn create_holds_the_state_lock_while_the_driver_places() {
    let (lab, h) = Lab::with_fake();
    h.set_probe(lab::lock_probe(lab.lock_file()));
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    assert_eq!(h.probes(), ["held"]);
    // It is released when the command ends.
    let after = lab::lock_probe(lab.lock_file());
    assert_eq!(after(&h.calls()[0]), "free");
}

#[cfg(windows)]
#[test]
fn create_reads_the_clock_once_it_holds_the_lock() {
    // Read before the lock is taken, the clock says NOW; read once it is held, as after a wait on
    // it, it says NOW + 5. The lease is created at the time read under the lock.
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    lab.seed(&[]);
    std::fs::write(lab.lock_file(), b"").unwrap();
    lab.clock_by_lock(NOW, NOW + 5);
    let ran = lab.run(&["lease", "create", "--project", "swim"]);
    let doc = ran.lease_doc();
    assert_eq!(doc["created_at"], fmt_time(NOW + 5));
    assert_eq!(lab.record_of(ID1).unwrap()["created_at"], fmt_time(NOW + 5));
    assert_eq!(ran.now_calls, 1, "The clock is read once per command");
    assert_eq!(h.calls(), [default_place(ID1, "swim")]);
}

#[test]
fn create_has_not_recorded_the_lease_while_the_driver_places() {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    lab.seed(&[]);
    h.set_probe(lab::state_probe(lab.state_file()));
    let before = lab.state_bytes().map(|b| String::from_utf8(b).unwrap());
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    assert_eq!(h.probes(), [before.unwrap()]);
}

#[test]
fn create_hands_the_driver_a_context_the_cli_built() {
    let (mut lab, h) = Lab::with_fake();
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    let c = h.contexts().remove(0);
    assert_eq!(c.home, lab.home);
    assert_eq!(c.ssh_dir, lab.ssh);
    assert_eq!(c.proxy_override, None);
    assert_eq!(c.system_root, None);
    assert_eq!(c.own_exe, lab.exe);

    // AGENTSPACE_PROXY_COMMAND and SystemRoot reach the Context when they are set and not empty.
    h.clear();
    lab.set_var("AGENTSPACE_PROXY_COMMAND", "forwarder --flag");
    lab.set_var("SystemRoot", "C:\\Windows");
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    let c = h.contexts().remove(0);
    assert_eq!(c.proxy_override.as_deref(), Some("forwarder --flag"));
    assert_eq!(c.system_root.as_deref(), Some("C:\\Windows"));

    // Empty means unset.
    h.clear();
    lab.set_var("AGENTSPACE_PROXY_COMMAND", "");
    lab.set_var("SystemRoot", "");
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    let c = h.contexts().remove(0);
    assert_eq!(c.proxy_override, None);
    assert_eq!(c.system_root, None);

    h.clear();
    lab.unset_var("AGENTSPACE_PROXY_COMMAND");
    lab.unset_var("SystemRoot");
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    let c = h.contexts().remove(0);
    assert_eq!(c.proxy_override, None);
    assert_eq!(c.system_root, None);
}

#[test]
fn the_ssh_folder_is_the_variables_made_full_or_the_homes_dot_ssh() {
    let (mut lab, h) = Lab::with_fake();
    let ssh_of = |lab: &Lab, h: &fake_driver::Handle| {
        h.clear();
        lab.run(&["lease", "create", "--project", "swim"])
            .lease_doc();
        h.contexts().remove(0).ssh_dir
    };
    lab.unset_var("AGENTSPACE_SSH_DIR");
    assert_eq!(ssh_of(&lab, &h), lab.home.join(".ssh"));
    lab.set_var("AGENTSPACE_SSH_DIR", "");
    assert_eq!(ssh_of(&lab, &h), lab.home.join(".ssh"));
    lab.set_var("AGENTSPACE_SSH_DIR", "rel-ssh");
    assert_eq!(
        ssh_of(&lab, &h),
        lab.cwd.join("rel-ssh"),
        "A relative folder is made full against the working folder"
    );
}

#[test]
fn a_relative_home_is_made_full_against_the_working_folder() {
    let (mut lab, h) = Lab::with_fake();
    lab.set_var("USERPROFILE", "rel-home");
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    assert_eq!(h.contexts().remove(0).home, lab.cwd.join("rel-home"));
}

#[cfg(windows)]
#[test]
fn a_proxy_command_that_is_not_valid_text_fails_create_before_any_driver_is_asked() {
    use std::os::windows::ffi::OsStringExt;
    let (mut lab, h) = Lab::with_fake();
    lab.set_var(
        "AGENTSPACE_PROXY_COMMAND",
        std::ffi::OsString::from_wide(&[0x61, 0xD800, 0x62]),
    );
    lab.run(&["lease", "create", "--project", "swim"]).expect_error(
        "internal",
        None,
        Some("agentspace lease create failed: AGENTSPACE_PROXY_COMMAND is not valid text. Nothing was recorded."),
    );
    assert!(h.calls().is_empty());
    assert!(lab.records().is_empty());
}

#[cfg(windows)]
#[test]
fn a_system_root_that_is_not_valid_text_is_read_as_unset() {
    use std::os::windows::ffi::OsStringExt;
    let (mut lab, h) = Lab::with_fake();
    lab.set_var("SystemRoot", std::ffi::OsString::from_wide(&[0x43, 0xD800]));
    lab.run(&["lease", "create", "--project", "swim"])
        .lease_doc();
    assert_eq!(h.contexts().remove(0).system_root, None);
}

#[test]
fn a_state_folder_that_is_a_file_fails_create_before_the_driver_is_asked() {
    let (lab, h) = Lab::with_fake();
    std::fs::write(&lab.state, b"not a folder").unwrap();
    lab.run(&["lease", "create", "--project", "swim"])
        .internal();
    assert!(h.calls().is_empty());
}

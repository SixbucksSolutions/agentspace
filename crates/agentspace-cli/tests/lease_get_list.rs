//! `agentspace lease get` and `agentspace lease list` (E0.05-T03-S02, #0106).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (Commands, `get` and `list`; Exit codes and errors; Messages), docs/state.md
//! (The lease state file) and the schemas under `schemas/`, never from the program's code (D31).
//! Each test runs through `agentspace_cli::execute_in` in its own throwaway folder under the temp
//! folder, and never changes its own process's environment.

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use common::{
    msg_empty_value, msg_needs_value, msg_no_id, msg_twice, msg_unexpected, msg_unknown_option_sub,
};
use lab::{
    ID1, ID2, ID3, ID4, Lab, MSG_NOT_VALID, NOW, Rec, fmt_time, msg_bad_id, msg_bad_project,
    msg_bad_state, msg_not_found,
};
use serde_json::{Value, json};

#[test]
fn the_time_helper_matches_known_instants() {
    assert_eq!(fmt_time(0), "1970-01-01T00:00:00Z");
    assert_eq!(fmt_time(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(fmt_time(NOW), "2027-01-15T08:00:00Z");
}

// ---- get ---------------------------------------------------------------------------------------

#[test]
fn get_prints_each_stored_state_as_its_document() {
    let lab = Lab::new();
    let recs = vec![
        Rec::ready(ID1, "swim"),
        Rec::expired(ID2, "swim"),
        Rec::pending(ID3, "swim"),
        Rec::released(ID4, "swim"),
        Rec::released_from_pending(&lab::uuid_n(5), "swim"),
        Rec::unsatisfiable(&lab::uuid_n(6), "swim"),
    ];
    lab.seed(&recs);
    let before = lab.state_bytes();
    for rec in &recs {
        let ran = lab.run(&["lease", "get", &rec.id]);
        let doc = ran.lease_doc();
        assert_eq!(doc, rec.doc(NOW), "get {} ({})", rec.id, rec.state);
        assert_eq!(ran.now_calls, 1, "The clock is read once per command");
    }
    assert_eq!(lab.state_bytes(), before, "get changes nothing");
}

#[test]
fn get_decides_expiry_to_the_second() {
    let mut lab = Lab::new();
    lab.seed(&[
        Rec::ready(ID1, "swim").expiring(NOW),
        Rec::ready(ID2, "swim").expiring(NOW + 1),
    ]);
    lab.set_now(NOW);
    let a = lab.run(&["lease", "get", ID1]).lease_doc();
    assert_eq!(a["state"], "expired", "At its expires_at it is expired");
    assert!(a.get("ssh").is_none(), "An expired lease has no ssh");
    assert_eq!(a["expires_at"], fmt_time(NOW));
    let b = lab.run(&["lease", "get", ID2]).lease_doc();
    assert_eq!(b["state"], "ready", "One second before it is ready");
    lab.set_now(NOW + 1);
    let b = lab.run(&["lease", "get", ID2]).lease_doc();
    assert_eq!(b["state"], "expired");
}

#[test]
fn get_never_prints_the_drivers_name_and_needs_no_driver() {
    let lab = Lab::new();
    lab.seed(&[
        Rec::ready(ID1, "swim").driver(Some("secret-backend-name")),
        Rec::released(ID2, "swim").driver(Some("secret-backend-name")),
        Rec::pending(ID3, "swim").driver(Some("secret-backend-name")),
    ]);
    for id in [ID1, ID2, ID3] {
        let ran = lab.run(&["lease", "get", id]);
        let doc = ran.lease_doc();
        assert!(doc.get("driver").is_none(), "No driver member");
        assert!(!ran.stdout_text().contains("secret-backend-name"));
        assert!(!ran.stderr_text().contains("secret-backend-name"));
    }
    let ran = lab.run(&["lease", "list"]);
    ran.list_doc();
    assert!(!ran.stdout_text().contains("secret-backend-name"));
}

#[test]
fn get_normalizes_the_id_and_reports_an_unknown_one_as_not_found() {
    let lab = Lab::new();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let ran = lab.run(&["lease", "get", " 3F2A9C1E-7B4D-4E6A-8C2F-0A1B2C3D4E5F\n"]);
    assert_eq!(ran.lease_doc()["id"], ID1);
    let ran = lab.run(&["lease", "get", &format!("  {}  ", ID2.to_uppercase())]);
    ran.expect_lease_error("not_found", ID2, None, &msg_not_found(ID2));
}

#[test]
fn get_of_a_fresh_home_is_not_found_and_makes_nothing() {
    let lab = Lab::new();
    let ran = lab.run(&["lease", "get", ID1]);
    ran.expect_lease_error("not_found", ID1, None, &msg_not_found(ID1));
    assert!(!lab.state.exists(), "get makes no state folder");
    let ran = lab.run(&["lease", "list"]);
    assert_eq!(ran.stdout_text(), "{\"leases\":[]}\n");
    assert!(!lab.state.exists(), "list makes no state folder");
}

#[test]
fn get_usage_and_value_errors() {
    let lab = Lab::new();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    lab.run(&["lease", "get"])
        .expect_error("usage", Some("id"), Some(&msg_no_id("get")));
    lab.run(&["lease", "get", ID1, ID2]).expect_error(
        "usage",
        None,
        Some(&msg_unexpected(ID2, "get")),
    );
    // lease-api.md, Exit codes and errors: `get <bad id> --ttl 8h` is `usage`.
    lab.run(&["lease", "get", "not-an-id", "--ttl", "8h"])
        .expect_error("usage", None, Some(&msg_unknown_option_sub("--ttl", "get")));
    lab.run(&["lease", "get", "not-an-id"]).expect_error(
        "invalid_value",
        Some("id"),
        Some(&msg_bad_id("not-an-id")),
    );
    // A value error comes before a records error.
    lab.seed_raw(b"{");
    lab.run(&["lease", "get", "not-an-id"]).expect_error(
        "invalid_value",
        Some("id"),
        Some(&msg_bad_id("not-an-id")),
    );
}

/// The state files that do not validate (state.md, The lease state file).
fn invalid_state_files() -> Vec<(&'static str, Vec<u8>)> {
    let good = Rec::ready(ID1, "swim").record();
    let doc = |leases: Vec<Value>| {
        let mut t = serde_json::to_string(&json!({"version": 0, "leases": leases})).unwrap();
        t.push('\n');
        t.into_bytes()
    };
    let mut bad_time = Rec::ready(ID2, "swim").record();
    bad_time["created_at"] = json!("2026-02-30T00:00:00Z");
    let mut leap = Rec::ready(ID2, "swim").record();
    leap["created_at"] = json!("2026-06-30T23:59:60Z");
    let mut extra = Rec::ready(ID2, "swim").record();
    extra["surprise"] = json!(1);
    let mut no_ssh = Rec::ready(ID2, "swim").record();
    no_ssh.as_object_mut().unwrap().remove("ssh");
    let mut bad_driver = Rec::ready(ID2, "swim").record();
    bad_driver["driver"] = json!("tab\there");
    let dup_member = format!(
        "{{\"version\":0,\"leases\":[{}]}}",
        serde_json::to_string(&good)
            .unwrap()
            .replacen("{", "{\"id\":\"x\",", 1)
    );
    vec![
        ("not JSON", b"{".to_vec()),
        ("empty", Vec::new()),
        ("wrong version", br#"{"version":1,"leases":[]}"#.to_vec()),
        ("no leases member", br#"{"version":0}"#.to_vec()),
        ("trailing text", b"{\"version\":0,\"leases\":[]} x".to_vec()),
        ("not UTF-8", vec![0xff, 0xfe, b'{', b'}']),
        ("two records, one id", doc(vec![good.clone(), good.clone()])),
        ("time that is no instant", doc(vec![bad_time])),
        ("a leap second", doc(vec![leap])),
        ("an unknown member", doc(vec![extra])),
        ("ready with no ssh", doc(vec![no_ssh])),
        ("a driver with a control character", doc(vec![bad_driver])),
        ("a record with one member twice", dup_member.into_bytes()),
    ]
}

#[test]
fn get_and_list_refuse_a_state_file_that_does_not_validate() {
    for (what, bytes) in invalid_state_files() {
        let lab = Lab::new();
        lab.seed_raw(&bytes);
        for line in [vec!["lease", "get", ID1], vec!["lease", "list"]] {
            lab.run(&line)
                .expect_error("internal", None, Some(MSG_NOT_VALID));
        }
        assert_eq!(
            lab.state_bytes().as_deref(),
            Some(bytes.as_slice()),
            "{what}"
        );
    }
}

#[test]
fn get_and_list_read_a_state_file_tolerantly() {
    // state.md: a reader MUST accept a leading byte order mark and any amount of JSON whitespace.
    let lab = Lab::new();
    let doc = json!({"version": 0, "leases": [Rec::ready(ID1, "swim").record()]});
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"\r\n  ");
    bytes.extend_from_slice(serde_json::to_string_pretty(&doc).unwrap().as_bytes());
    bytes.extend_from_slice(b"\r\n\r\n");
    lab.seed_raw(&bytes);
    assert_eq!(lab.run(&["lease", "get", ID1]).lease_doc()["id"], ID1);
    assert_eq!(lab.run(&["lease", "list"]).list_ids(), vec![ID1]);
    // A number written with a fraction of zero is form only.
    let text = serde_json::to_string(&doc)
        .unwrap()
        .replace("\"ttl_seconds\":3600", "\"ttl_seconds\":3600.0");
    lab.seed_raw(text.as_bytes());
    let got = lab.run(&["lease", "get", ID1]).lease_doc();
    assert_eq!(got["ttl_seconds"], 3600);
}

#[test]
fn get_and_list_refuse_something_that_is_not_a_file_at_the_state_file() {
    // state.md, gap FB-21: a folder at leases.json is not a missing file.
    let lab = Lab::new();
    std::fs::create_dir_all(lab.state_file()).unwrap();
    for line in [vec!["lease", "get", ID1], vec!["lease", "list"]] {
        let message = lab.run(&line).internal();
        assert!(
            message.starts_with("The lease records could not be read: ")
                && message.ends_with(". Nothing was changed."),
            "{message}"
        );
    }
}

#[cfg(windows)]
#[test]
fn get_and_list_never_wait_for_the_lock() {
    let lab = Lab::new();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let _held = lab::LockHold::take(&lab);
    let start = std::time::Instant::now();
    assert_eq!(lab.run(&["lease", "get", ID1]).lease_doc()["id"], ID1);
    assert_eq!(lab.run(&["lease", "list"]).list_ids(), vec![ID1]);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "get and list waited for the lock"
    );
}

#[test]
fn get_and_list_with_a_folder_at_the_lock_still_work() {
    let lab = Lab::new();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    std::fs::create_dir_all(lab.lock_file()).unwrap();
    assert_eq!(lab.run(&["lease", "get", ID1]).lease_doc()["id"], ID1);
    assert_eq!(lab.run(&["lease", "list"]).list_ids(), vec![ID1]);
}

// ---- list --------------------------------------------------------------------------------------

#[test]
fn list_prints_every_kept_lease_oldest_first() {
    let lab = Lab::new();
    // The same created_at for three, so the id breaks the tie, by characters.
    let t = NOW - 500;
    let recs = [
        Rec::ready(ID4, "swim").created(t),
        Rec::pending(ID2, "other").created(t),
        Rec::released(ID3, "swim").created(t - 60),
        Rec::unsatisfiable(ID1, "swim").created(t),
        Rec::expired(&lab::uuid_n(9), "swim").created(t + 60),
    ];
    lab.seed(&recs);
    let ran = lab.run(&["lease", "list"]);
    let doc = ran.list_doc();
    let want = [
        ID3.to_string(),
        ID1.to_string(),
        ID4.to_string(),
        ID2.to_string(),
        lab::uuid_n(9),
    ];
    // ID1 = 3f.., ID4 = 6b.., ID2 = 9b.., compared as strings.
    let got = ran.list_ids();
    assert_eq!(got, want);
    let docs: Vec<Value> = want
        .iter()
        .map(|id| recs.iter().find(|r| &r.id == id).unwrap().doc(NOW))
        .collect();
    assert_eq!(doc, json!({ "leases": docs }));
    assert_eq!(ran.now_calls, 1);
}

#[test]
fn list_with_no_leases_prints_an_empty_list() {
    let lab = Lab::new();
    lab.seed(&[]);
    let ran = lab.run(&["lease", "list"]);
    assert_eq!(ran.stdout_text(), "{\"leases\":[]}\n");
    assert_eq!(ran.list_doc()["leases"].as_array().unwrap().len(), 0);
}

#[test]
fn list_filters_by_project_and_state() {
    let lab = Lab::new();
    let recs = [
        Rec::ready(ID1, "swim").created(NOW - 600),
        Rec::expired(ID2, "swim").created(NOW - 500),
        Rec::pending(ID3, "swim").created(NOW - 400),
        Rec::released(ID4, "other").created(NOW - 300),
        Rec::unsatisfiable(&lab::uuid_n(7), "other").created(NOW - 200),
    ];
    lab.seed(&recs);
    let ids = |args: &[&str]| {
        let mut line = vec!["lease", "list"];
        line.extend_from_slice(args);
        lab.run(&line).list_ids()
    };
    let u7 = lab::uuid_n(7);
    assert_eq!(ids(&["--project", "swim"]), [ID1, ID2, ID3]);
    assert_eq!(ids(&["--project", "  SWIM\n"]), [ID1, ID2, ID3]);
    assert_eq!(ids(&["--project", "sw"]), Vec::<&str>::new());
    assert_eq!(
        ids(&["--state", "ready"]),
        [ID1],
        "ready leaves out an expired one"
    );
    assert_eq!(ids(&["--state", "expired"]), [ID2]);
    assert_eq!(ids(&["--state", "ready,pending"]), [ID1, ID3]);
    assert_eq!(
        ids(&["--state", "ready", "--state", "released"]),
        [ID1, ID4]
    );
    assert_eq!(ids(&["--state", " READY , Expired "]), [ID1, ID2]);
    assert_eq!(
        ids(&["--state=unsatisfiable,ready,ready"]),
        [ID1, u7.as_str()]
    );
    assert_eq!(
        ids(&["--project", "swim", "--state", "ready,expired,pending"]),
        [ID1, ID2, ID3]
    );
    assert_eq!(
        ids(&["--project", "other", "--state", "ready"]),
        Vec::<&str>::new()
    );
    assert_eq!(
        lab.run(&["lease", "list", "--state", "pending"])
            .stdout_text()
            .matches("\"id\"")
            .count(),
        1
    );
}

#[test]
fn list_decides_every_state_at_one_instant() {
    // The clock answers NOW, then NOW + 1, and so on. Five leases expire at NOW + 2: a command that
    // read the clock again for each would find the last of them expired.
    let mut lab = Lab::new();
    let recs: Vec<Rec> = (0..5)
        .map(|n| {
            Rec::ready(&lab::uuid_n(n), "swim")
                .expiring(NOW + 2)
                .created(NOW - 100 + i64::from(n))
        })
        .collect();
    lab.seed(&recs);
    lab.tick_from(NOW);
    let ran = lab.run(&["lease", "list"]);
    let doc = ran.list_doc();
    for lease in doc["leases"].as_array().unwrap() {
        assert_eq!(lease["state"], "ready", "{lease}");
    }
    assert_eq!(
        ran.now_calls, 1,
        "The clock is read once for the whole list"
    );
    lab.tick_from(NOW);
    let ran = lab.run(&["lease", "list", "--state", "ready"]);
    assert_eq!(ran.list_ids().len(), 5);
    lab.tick_from(NOW + 2);
    let ran = lab.run(&["lease", "list", "--state", "expired"]);
    assert_eq!(ran.list_ids().len(), 5);
}

#[test]
fn list_usage_and_value_errors() {
    let lab = Lab::new();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let run = |args: &[&str]| {
        let mut line = vec!["lease", "list"];
        line.extend_from_slice(args);
        lab.run(&line)
    };
    run(&["--project"]).expect_error(
        "usage",
        Some("project"),
        Some(&msg_needs_value("project", "list")),
    );
    run(&["--project", ""]).expect_error(
        "usage",
        Some("project"),
        Some(&msg_empty_value("project", "list")),
    );
    run(&["--project", "a", "--project", "b"]).expect_error(
        "usage",
        Some("project"),
        Some(&msg_twice("project")),
    );
    run(&["--state"]).expect_error(
        "usage",
        Some("state"),
        Some(&msg_needs_value("state", "list")),
    );
    run(&["--state", ""]).expect_error(
        "usage",
        Some("state"),
        Some(&msg_empty_value("state", "list")),
    );
    run(&["--ttl", "8h"]).expect_error(
        "usage",
        None,
        Some(&msg_unknown_option_sub("--ttl", "list")),
    );
    run(&["extra"]).expect_error("usage", None, Some(&msg_unexpected("extra", "list")));
    for (arg, name) in [
        ("nope", "nope"),
        ("ready,", ""),
        ("ready,,pending", ""),
        (" ", ""),
    ] {
        run(&["--state", arg]).expect_error(
            "invalid_value",
            Some("state"),
            Some(&msg_bad_state(name)),
        );
    }
    // The project is checked before the state.
    run(&["--state", "nope", "--project", "Bad_Name"]).expect_error(
        "invalid_value",
        Some("project"),
        Some(&msg_bad_project("Bad_Name")),
    );
}

//! `agentspace lease renew` (E0.05-T03-S02, #0106).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (`renew`, Concurrency and atomicity, Exit codes and errors, Messages),
//! docs/driver.md (Renew a lease, Ordering and concurrency, The driver's name), docs/state.md
//! (Renewal, The lease state file) and the ruling on card #0106 about the `Renewal`, never from the
//! program's code (D31). The driver is a fake that logs every call, and each test runs through
//! `agentspace_cli::execute_in` in its own throwaway folder under the temp folder.

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use common::{
    msg_empty_value, msg_needs_value, msg_no_id, msg_twice, msg_unexpected, msg_unknown_option_sub,
};
use fake_driver::{Call, fail, fail_with};
use lab::{
    ID1, ID2, ID3, ID4, Lab, NOW, Rec, count_of, fmt_time, msg_bad_id, msg_not_found,
    msg_not_ready, msg_ttl_no_unit, msg_ttl_range, msg_unknown_driver,
};
use serde_json::json;

fn renew_call(id: &str, project: &str, ttl: u32) -> Call {
    Call::Renew {
        id: id.to_string(),
        project: project.to_string(),
        ttl_seconds: ttl,
    }
}

/// The calls of a renew that was recorded and committed.
fn renewed(id: &str, ttl: u32) -> Vec<Call> {
    vec![renew_call(id, "swim", ttl), Call::Commit]
}

fn assert_never_dropped(h: &fake_driver::Handle) {
    assert!(
        !h.calls().contains(&Call::RenewalDropped),
        "A renewal was dropped without commit() or rollback(): {:?}",
        h.calls()
    );
}

// ---- Success -----------------------------------------------------------------------------------

#[test]
fn renew_gives_a_new_expiry_records_it_and_then_commits() {
    let (lab, h) = Lab::with_fake();
    let old = Rec::ready(ID1, "swim").ttl(3600);
    lab.seed(std::slice::from_ref(&old));
    h.set_probe(lab::state_probe(lab.state_file()));
    let ran = lab.run(&["lease", "renew", ID1]);
    let doc = ran.lease_doc();
    // Without --ttl the lease's current ttl_seconds is asked for again.
    let mut want = old.doc(NOW);
    want["expires_at"] = json!(fmt_time(NOW + 3600));
    assert_eq!(doc, want);
    assert_eq!(h.calls(), renewed(ID1, 3600));
    // The record is written before the renewal is committed, and not before the driver renewed.
    let probes = h.probes();
    assert!(
        probes[0].contains(&fmt_time(NOW + 1800)),
        "At renew the record is the old one"
    );
    assert!(
        probes[1].contains(&format!("\"expires_at\":\"{}\"", fmt_time(NOW + 3600))),
        "At commit the record is written: {}",
        probes[1]
    );
    let mut want_rec = old.record();
    want_rec["expires_at"] = json!(fmt_time(NOW + 3600));
    assert_eq!(lab.records(), [want_rec]);
    assert_eq!(ran.now_calls, 1, "The clock is read once per command");
    assert_never_dropped(&h);
}

#[test]
fn renew_prints_the_renewals_expiry_and_the_ttl_asked_for_whether_longer_or_shorter() {
    for (ttl, seconds) in [("2h", 7200_u32), ("1m", 60), ("24h", 86400), ("PT1H", 3600)] {
        let (lab, h) = Lab::with_fake();
        let old = Rec::ready(ID1, "swim").ttl(3600);
        lab.seed(std::slice::from_ref(&old));
        let expires = NOW + i64::from(seconds) + 3;
        h.script_renew(Ok(expires));
        let doc = lab.run(&["lease", "renew", ID1, "--ttl", ttl]).lease_doc();
        assert_eq!(doc["expires_at"], fmt_time(expires), "{ttl}");
        assert_eq!(doc["ttl_seconds"], seconds, "{ttl}");
        assert_eq!(doc["created_at"], old.doc(NOW)["created_at"]);
        assert_eq!(doc["ssh"], old.doc(NOW)["ssh"], "The alias and user stay");
        assert_eq!(h.calls(), renewed(ID1, seconds), "{ttl}");
        let rec = lab.record_of(ID1).unwrap();
        assert_eq!(rec["ttl_seconds"], seconds);
        assert_eq!(rec["expires_at"], fmt_time(expires));
        assert_never_dropped(&h);
    }
}

#[test]
fn renew_accepts_the_id_in_any_case_with_whitespace() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let doc = lab
        .run(&["lease", "renew", &format!(" {} ", ID1.to_uppercase())])
        .lease_doc();
    assert_eq!(doc["id"], ID1);
    assert_eq!(h.calls(), renewed(ID1, 3600));
}

#[test]
fn renew_leaves_the_other_records_alone() {
    let (lab, _h) = Lab::with_fake();
    let others = [
        Rec::released(ID2, "swim"),
        Rec::pending(ID3, "swim"),
        Rec::ready(ID4, "swim"),
    ];
    let mut all = vec![Rec::ready(ID1, "swim")];
    all.extend(others.iter().cloned());
    lab.seed(&all);
    lab.run(&["lease", "renew", ID1]).lease_doc();
    let recs = lab.records();
    assert_eq!(recs.len(), 4);
    for (got, want) in recs[1..].iter().zip(&others) {
        assert_eq!(got, &want.record());
    }
}

// ---- The driver's name -------------------------------------------------------------------------

#[test]
fn renew_finds_its_driver_by_the_stored_name() {
    let mut lab = Lab::new();
    let alpha = lab.fake("alpha");
    let beta = lab.fake("beta");
    lab.seed(&[Rec::ready(ID1, "swim").driver(Some("beta"))]);
    lab.run(&["lease", "renew", ID1]).lease_doc();
    assert!(alpha.calls().is_empty());
    assert_eq!(beta.calls(), renewed(ID1, 3600));
}

#[test]
fn a_stored_ready_record_without_a_driver_is_the_ec2_drivers_and_stays_without_one() {
    let mut lab = Lab::new();
    let other = lab.fake("other");
    let ec2 = lab.fake("ec2");
    let old = Rec::ready(ID1, "swim").driver(None);
    lab.seed(std::slice::from_ref(&old));
    lab.run(&["lease", "renew", ID1]).lease_doc();
    assert!(other.calls().is_empty());
    assert_eq!(ec2.calls(), renewed(ID1, 3600));
    let rec = lab.record_of(ID1).unwrap();
    assert!(
        rec.get("driver").is_none(),
        "renew MUST NOT add driver: {rec}"
    );
}

#[test]
fn an_unknown_driver_fails_internal_without_naming_it_and_changes_nothing() {
    for drivers in [&["fake"][..], &[][..], &["ec2x", "other"][..]] {
        let mut lab = Lab::new();
        let handles: Vec<_> = drivers.iter().map(|n| lab.fake(n)).collect();
        lab.seed(&[Rec::ready(ID1, "swim").driver(Some("secret-backend-name"))]);
        let before = lab.state_bytes();
        let ran = lab.run(&["lease", "renew", ID1]);
        ran.expect_error("internal", None, Some(&msg_unknown_driver("renew")));
        assert!(!ran.stderr_text().contains("secret-backend-name"));
        assert!(!ran.stdout_text().contains("secret-backend-name"));
        assert!(handles.iter().all(|h| h.calls().is_empty()));
        assert_eq!(lab.state_bytes(), before);
    }
}

#[test]
fn a_ready_record_without_a_driver_fails_the_same_way_when_there_is_no_ec2_driver() {
    for drivers in [&[][..], &["fake"][..]] {
        let mut lab = Lab::new();
        let handles: Vec<_> = drivers.iter().map(|n| lab.fake(n)).collect();
        lab.seed(&[Rec::ready(ID1, "swim").driver(None)]);
        let ran = lab.run(&["lease", "renew", ID1]);
        ran.expect_error("internal", None, Some(&msg_unknown_driver("renew")));
        assert!(
            !ran.stderr_text().contains("ec2"),
            "The default driver's name is never printed"
        );
        assert!(handles.iter().all(|h| h.calls().is_empty()));
    }
}

// ---- The order of errors -----------------------------------------------------------------------

#[test]
fn renew_refuses_every_state_but_ready_with_invalid_state_and_asks_no_driver() {
    let (lab, h) = Lab::with_fake();
    let recs = [
        Rec::expired(ID1, "swim"),
        Rec::ready(ID2, "swim").expiring(NOW),
        Rec::pending(ID3, "swim"),
        Rec::released(ID4, "swim"),
        Rec::unsatisfiable(&lab::uuid_n(5), "swim"),
        Rec::released_from_pending(&lab::uuid_n(6), "swim"),
    ];
    lab.seed(&recs);
    let before = lab.state_bytes();
    let want = [
        "expired",
        "expired",
        "pending",
        "released",
        "unsatisfiable",
        "released",
    ];
    for (rec, state) in recs.iter().zip(want) {
        lab.run(&["lease", "renew", &rec.id]).expect_lease_error(
            "invalid_state",
            &rec.id,
            Some(state),
            &msg_not_ready(&rec.id, state),
        );
    }
    assert!(h.calls().is_empty());
    assert_eq!(lab.state_bytes(), before);
}

#[test]
fn a_lease_one_second_from_its_expiry_is_still_ready_to_renew() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim").expiring(NOW + 1)]);
    lab.run(&["lease", "renew", ID1]).lease_doc();
    assert_eq!(h.calls(), renewed(ID1, 3600));
}

#[test]
fn invalid_state_comes_before_an_unknown_driver() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[
        Rec::expired(ID1, "swim").driver(Some("secret-backend-name")),
        Rec::released(ID2, "swim").driver(Some("secret-backend-name")),
    ]);
    lab.run(&["lease", "renew", ID1]).expect_lease_error(
        "invalid_state",
        ID1,
        Some("expired"),
        &msg_not_ready(ID1, "expired"),
    );
    lab.run(&["lease", "renew", ID2]).expect_lease_error(
        "invalid_state",
        ID2,
        Some("released"),
        &msg_not_ready(ID2, "released"),
    );
    assert!(h.calls().is_empty());
}

#[test]
fn renew_reports_errors_in_lease_apis_order() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::expired(ID1, "swim")]);
    // `renew <id of an expired lease> --ttl 48h` is out_of_range, not invalid_state.
    lab.run(&["lease", "renew", ID1, "--ttl", "48h"])
        .expect_error("out_of_range", Some("ttl"), Some(&msg_ttl_range("48h")));
    // `renew <unknown id> --ttl 8` is invalid_value for ttl, not not_found.
    lab.run(&["lease", "renew", ID2, "--ttl", "8"])
        .expect_error("invalid_value", Some("ttl"), Some(&msg_ttl_no_unit("8")));
    // An unknown id with a TTL out of range is out_of_range, not not_found.
    lab.run(&["lease", "renew", ID2, "--ttl", "48h"])
        .expect_error("out_of_range", Some("ttl"), Some(&msg_ttl_range("48h")));
    // The id is checked before the TTL.
    lab.run(&["lease", "renew", "not-an-id", "--ttl", "8"])
        .expect_error("invalid_value", Some("id"), Some(&msg_bad_id("not-an-id")));
    lab.run(&["lease", "renew", "not-an-id", "--ttl", "48h"])
        .expect_error("invalid_value", Some("id"), Some(&msg_bad_id("not-an-id")));
    // A usage error comes before every value error.
    lab.run(&["lease", "renew", "not-an-id", "--ttl"])
        .expect_error("usage", Some("ttl"), Some(&msg_needs_value("ttl", "renew")));
    // Unreadable records come before not_found and invalid_state.
    lab.seed_raw(b"[]");
    lab.run(&["lease", "renew", ID1])
        .expect_error("internal", None, Some(lab::MSG_NOT_VALID));
    lab.run(&["lease", "renew", ID2])
        .expect_error("internal", None, Some(lab::MSG_NOT_VALID));
    assert!(h.calls().is_empty());
}

#[test]
fn renew_of_an_unknown_lease_is_not_found() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let before = lab.state_bytes();
    lab.run(&["lease", "renew", &format!(" {} ", ID2.to_uppercase())])
        .expect_lease_error("not_found", ID2, None, &msg_not_found(ID2));
    assert!(h.calls().is_empty());
    assert_eq!(lab.state_bytes(), before);
}

#[test]
fn renew_usage_errors() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let run = |args: &[&str]| {
        let mut line = vec!["lease", "renew"];
        line.extend_from_slice(args);
        lab.run(&line)
    };
    run(&[]).expect_error("usage", Some("id"), Some(&msg_no_id("renew")));
    run(&["--ttl", "8h"]).expect_error("usage", Some("id"), Some(&msg_no_id("renew")));
    run(&[ID1, "--ttl"]).expect_error("usage", Some("ttl"), Some(&msg_needs_value("ttl", "renew")));
    run(&[ID1, "--ttl", ""]).expect_error(
        "usage",
        Some("ttl"),
        Some(&msg_empty_value("ttl", "renew")),
    );
    run(&[ID1, "--ttl", "8h", "--ttl", "9h"]).expect_error(
        "usage",
        Some("ttl"),
        Some(&msg_twice("ttl")),
    );
    run(&[ID1, ID2]).expect_error("usage", None, Some(&msg_unexpected(ID2, "renew")));
    run(&[ID1, "--project", "x"]).expect_error(
        "usage",
        None,
        Some(&msg_unknown_option_sub("--project", "renew")),
    );
    assert!(h.calls().is_empty());
}

// ---- Failures ----------------------------------------------------------------------------------

#[test]
fn a_failed_renew_is_internal_with_the_drivers_message_and_changes_nothing() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let before = lab.state_bytes();
    h.script_renew(Err(fail(
        "The lease's key could not be found, so no certificate can be issued. Nothing was changed.",
    )));
    lab.run(&["lease", "renew", ID1]).expect_error(
        "internal",
        None,
        Some("agentspace lease renew failed: The lease's key could not be found, so no certificate can be issued. Nothing was changed."),
    );
    // There is no renewal to commit or roll back.
    assert_eq!(h.calls(), [renew_call(ID1, "swim", 3600)]);
    assert_eq!(lab.state_bytes(), before);
}

#[test]
fn a_failed_renew_prints_each_leftover_once_and_escapes_control_characters() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let l1 = "~/.agentspace/leases/3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f/renew-0.pub";
    let message = format!("The credential could not be renewed\nand {l1} could not be removed.");
    h.script_renew(Err(fail_with(&message, &[l1])));
    let text = lab.run(&["lease", "renew", ID1]).internal();
    assert_eq!(
        text,
        format!(
            "agentspace lease renew failed: The credential could not be renewed\\u000aand {l1} could not be removed."
        )
    );
    assert_eq!(count_of(&text, l1), 1);
}

#[cfg(windows)]
#[test]
fn a_record_that_cannot_be_written_rolls_the_renewal_back_and_never_commits() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    std::fs::write(lab.lock_file(), b"").unwrap();
    let before = lab.state_bytes();
    let _deny = lab::DenyWrite::new(&lab.state);
    let text = lab.run(&["lease", "renew", ID1]).internal();
    assert!(
        text.starts_with("The lease records could not be written: ")
            && text.ends_with(". Nothing was changed."),
        "{text}"
    );
    assert_eq!(h.calls(), [renew_call(ID1, "swim", 3600), Call::Rollback]);
    assert_eq!(
        lab.state_bytes(),
        before,
        "The lease keeps its expires_at and ttl_seconds"
    );
}

#[cfg(windows)]
#[test]
fn a_rollback_that_fails_says_that_the_previous_certificate_was_not_put_back() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    std::fs::write(lab.lock_file(), b"").unwrap();
    let before = lab.state_bytes();
    h.script_rollback(Err(fail("Rollback exploded.")));
    let _deny = lab::DenyWrite::new(&lab.state);
    let text = lab.run(&["lease", "renew", ID1]).internal();
    assert!(
        text.starts_with("agentspace lease renew failed: The lease records could not be written: ")
            && text.ends_with(
                ". Its previous certificate could not be put back; renew the lease again."
            ),
        "{text}"
    );
    assert!(
        !text.contains("Rollback exploded"),
        "The first failure is reported, not the rollback's"
    );
    assert_eq!(h.calls(), [renew_call(ID1, "swim", 3600), Call::Rollback]);
    assert_eq!(lab.state_bytes(), before);
}

#[cfg(windows)]
#[test]
fn renew_holds_the_state_lock_while_the_driver_renews_and_until_it_commits() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    h.set_probe(lab::lock_probe(lab.lock_file()));
    lab.run(&["lease", "renew", ID1]).lease_doc();
    assert_eq!(h.probes(), ["held", "held"], "Held at renew and at commit");
}

#[test]
fn renew_hands_the_driver_the_home_and_the_programs_own_file() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    lab.run(&["lease", "renew", ID1]).lease_doc();
    let c = h.contexts().remove(0);
    assert_eq!(c.home, lab.home);
    assert_eq!(c.own_exe, lab.exe);
}

#[test]
fn renew_of_a_fresh_home_is_not_found_and_records_nothing() {
    let (lab, h) = Lab::with_fake();
    lab.run(&["lease", "renew", ID1]).expect_lease_error(
        "not_found",
        ID1,
        None,
        &msg_not_found(ID1),
    );
    assert!(h.calls().is_empty());
    assert!(lab.records().is_empty());
}

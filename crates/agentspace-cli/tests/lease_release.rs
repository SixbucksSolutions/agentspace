//! `agentspace lease release` (E0.05-T03-S02, #0106).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (`release`, Concurrency and atomicity, Messages), docs/driver.md (Release a
//! lease, Ordering and concurrency, The driver's name) and docs/state.md (Release, and what it
//! leaves; The lease state file), never from the program's code (D31). The driver is a fake that logs
//! every call, and each test runs through `agentspace_cli::execute_in` in its own throwaway folder
//! under the temp folder.

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use common::{msg_no_id, msg_unexpected, msg_unknown_option_sub};
use fake_driver::{Call, fail, fail_with};
use lab::{
    ID1, ID2, ID3, ID4, Lab, NOW, Rec, count_of, fmt_time, msg_bad_id, msg_not_found,
    msg_unknown_driver,
};
use serde_json::{Value, json};

const MSG_FOREIGN_ALIAS: &str = "The lease's host alias file belongs to another lease or to no lease, so nothing was deleted. A person has to move that file.";
const MSG_NOT_DELETED: &str = "The lease's local key, certificate or host alias could not be deleted: a file is in use. Run release again to finish.";

fn check(id: &str, project: &str) -> Call {
    Call::CheckRelease {
        id: id.to_string(),
        project: project.to_string(),
    }
}

fn release(id: &str, project: &str) -> Call {
    Call::Release {
        id: id.to_string(),
        project: project.to_string(),
    }
}

/// The two calls of a release that reaches its driver.
fn both(id: &str) -> Vec<Call> {
    vec![check(id, "swim"), release(id, "swim")]
}

/// The lease document `release` prints for a lease that was ever ready or pending, as a record
/// without `driver`: `released`, no `ssh`, the first `released_at`, and `expires_at` kept.
fn released_doc(rec: &Rec, at: i64) -> Value {
    let mut v = rec.record();
    let o = v.as_object_mut().unwrap();
    o.remove("driver");
    o.remove("ssh");
    o.insert("state".into(), json!("released"));
    o.insert("released_at".into(), json!(fmt_time(at)));
    v
}

// ---- By the stored state ------------------------------------------------------------------------

#[test]
fn release_of_a_ready_or_expired_lease_checks_records_released_and_then_releases() {
    for (what, rec) in [
        ("ready", Rec::ready(ID1, "swim")),
        ("expired", Rec::expired(ID1, "swim")),
    ] {
        let (mut lab, h) = Lab::with_fake();
        lab.seed(std::slice::from_ref(&rec));
        lab.set_now(NOW + 100);
        h.set_probe(lab::state_probe(lab.state_file()));
        let ran = lab.run(&["lease", "release", ID1]);
        let doc = ran.lease_doc();
        let want = released_doc(&rec, NOW + 100);
        assert_eq!(doc, want, "{what}");
        assert_eq!(h.calls(), both(ID1), "{what}");
        // The order: check_release, then the record is written `released`, then release.
        let probes = h.probes();
        assert!(
            probes[0].contains("\"state\":\"ready\""),
            "{what}: At the check the record is as it was"
        );
        assert!(
            probes[1].contains("\"state\":\"released\"")
                && probes[1].contains(&fmt_time(NOW + 100)),
            "{what}: At release the record is already released: {}",
            probes[1]
        );
        let mut want_rec = want.clone();
        want_rec["driver"] = json!("fake");
        assert_eq!(lab.records(), [want_rec], "{what}");
        assert_eq!(ran.now_calls, 1, "The clock is read once per command");
    }
}

#[test]
fn release_of_a_released_lease_asks_the_driver_again_and_changes_no_record() {
    let cases = [
        ("released", Rec::released(ID1, "swim")),
        (
            "released from pending",
            Rec::released_from_pending(ID1, "swim"),
        ),
    ];
    for (what, rec) in cases {
        let (mut lab, h) = Lab::with_fake();
        lab.seed(std::slice::from_ref(&rec));
        let before = lab.state_bytes();
        lab.set_now(NOW + 5000);
        let doc = lab.run(&["lease", "release", ID1]).lease_doc();
        assert_eq!(
            doc,
            rec.doc(NOW),
            "{what}: prints the lease with its first released_at"
        );
        assert_eq!(h.calls(), both(ID1), "{what}");
        assert_eq!(
            lab.state_bytes(),
            before,
            "{what}: the record does not change"
        );
    }
}

#[test]
fn release_of_a_pending_lease_with_a_driver_hands_it_to_that_driver() {
    // docs/driver.md, Release a lease: from v0.07 a `pending` record with a `driver` is released as
    // a `ready` one is, so a placement killed part way leaves no key, certificate or alias behind
    // (pin P16; card #0150's second comment).
    let mut lab = Lab::new();
    let h = lab.fake("secret-backend-name");
    let rec = Rec::pending(ID1, "swim").driver(Some("secret-backend-name"));
    lab.seed(std::slice::from_ref(&rec));
    lab.set_now(NOW + 100);
    h.set_probe(lab::state_probe(lab.state_file()));
    let ran = lab.run(&["lease", "release", ID1]);
    let doc = ran.lease_doc();
    let want = released_doc(&rec, NOW + 100);
    assert_eq!(doc, want);
    assert!(
        doc.get("expires_at").is_none(),
        "A pending lease has no expires_at"
    );
    assert_eq!(h.calls(), both(ID1), "The check, then the release");
    // The order is a ready lease's: checked while the record is as it was, released after it is
    // written `released`.
    let probes = h.probes();
    assert!(probes[0].contains("\"state\":\"pending\""), "{}", probes[0]);
    assert!(
        probes[1].contains("\"state\":\"released\"") && probes[1].contains(&fmt_time(NOW + 100)),
        "{}",
        probes[1]
    );
    let mut want_rec = want.clone();
    want_rec["driver"] = json!("secret-backend-name");
    assert_eq!(lab.records(), [want_rec], "The driver member stays");
    assert!(!ran.stdout_text().contains("secret-backend-name"));
}

#[test]
fn a_failed_release_of_a_pending_lease_is_the_drivers_failure_and_the_record_stays_released() {
    let (lab, h) = Lab::with_fake();
    let rec = Rec::pending(ID1, "swim");
    lab.seed(std::slice::from_ref(&rec));
    h.script_release(Err(fake_driver::fail(
        "The lease's credential or alias could not be deleted. Run release again.",
    )));
    let ran = lab.run(&["lease", "release", ID1]);
    ran.internal();
    assert_eq!(h.calls(), both(ID1));
    assert_eq!(
        lab.records()[0]["state"],
        "released",
        "As for a ready lease, the record is written before the files are removed"
    );
}

#[test]
fn release_of_a_pending_lease_with_no_driver_writes_the_record_only_and_asks_no_driver() {
    let (mut lab, h) = Lab::with_fake();
    let other = lab.fake("other");
    let rec = Rec::pending(ID1, "swim").driver(None);
    lab.seed(std::slice::from_ref(&rec));
    lab.set_now(NOW + 100);
    let ran = lab.run(&["lease", "release", ID1]);
    let doc = ran.lease_doc();
    assert_eq!(doc, released_doc(&rec, NOW + 100));
    assert!(
        h.calls().is_empty() && other.calls().is_empty(),
        "No driver is asked"
    );
    assert_eq!(
        lab.records(),
        [released_doc(&rec, NOW + 100)],
        "It still has no driver"
    );
}

#[test]
fn release_of_an_unsatisfiable_lease_changes_nothing_and_asks_no_driver() {
    let (lab, h) = Lab::with_fake();
    let rec = Rec::unsatisfiable(ID1, "swim");
    lab.seed(&[rec.clone(), Rec::ready(ID2, "swim")]);
    let before = lab.state_bytes();
    let doc = lab.run(&["lease", "release", ID1]).lease_doc();
    assert_eq!(doc, rec.doc(NOW));
    assert!(h.calls().is_empty());
    assert_eq!(lab.state_bytes(), before);
    // It needs no driver at all, even when the record names one this build does not have.
    let lab = Lab::new();
    lab.seed(&[rec.clone().driver(Some("secret-backend-name"))]);
    assert_eq!(
        lab.run(&["lease", "release", ID1]).lease_doc(),
        rec.doc(NOW)
    );
}

#[test]
fn release_leaves_every_other_record_alone_and_keeps_driver_members_as_they_are() {
    let (lab, _h) = Lab::with_fake();
    let others = [
        Rec::pending(ID2, "swim"),
        Rec::released(ID3, "swim").driver(None),
        Rec::ready(ID4, "other"),
    ];
    let mut all = vec![Rec::ready(ID1, "swim")];
    all.extend(others.iter().cloned());
    lab.seed(&all);
    lab.run(&["lease", "release", ID1]).lease_doc();
    let recs = lab.records();
    assert_eq!(recs.len(), 4);
    for (got, want) in recs[1..].iter().zip(&others) {
        assert_eq!(got, &want.record());
    }
}

// ---- The driver's name -------------------------------------------------------------------------

#[test]
fn release_finds_its_driver_by_the_stored_name() {
    let mut lab = Lab::new();
    let alpha = lab.fake("alpha");
    let beta = lab.fake("beta");
    lab.seed(&[Rec::ready(ID1, "swim").driver(Some("alpha"))]);
    lab.run(&["lease", "release", ID1]).lease_doc();
    assert_eq!(alpha.calls(), both(ID1));
    assert!(beta.calls().is_empty());
}

#[test]
fn a_ready_or_released_record_without_a_driver_is_the_ec2_drivers_and_stays_without_one() {
    for rec in [
        Rec::ready(ID1, "swim").driver(None),
        Rec::released(ID1, "swim").driver(None),
    ] {
        let mut lab = Lab::new();
        let other = lab.fake("other");
        let ec2 = lab.fake("ec2");
        lab.seed(std::slice::from_ref(&rec));
        lab.run(&["lease", "release", ID1]).lease_doc();
        assert!(other.calls().is_empty());
        assert_eq!(ec2.calls(), both(ID1));
        assert!(
            lab.record_of(ID1).unwrap().get("driver").is_none(),
            "release MUST NOT add driver"
        );
    }
}

#[test]
fn an_unknown_driver_fails_internal_without_naming_it_and_changes_nothing() {
    let recs = [
        ("ready", Rec::ready(ID1, "swim")),
        ("expired", Rec::expired(ID1, "swim")),
        ("released", Rec::released(ID1, "swim")),
    ];
    for (what, rec) in recs {
        for drivers in [&["fake"][..], &[][..]] {
            let mut lab = Lab::new();
            let handles: Vec<_> = drivers.iter().map(|n| lab.fake(n)).collect();
            lab.seed(&[rec.clone().driver(Some("secret-backend-name"))]);
            let before = lab.state_bytes();
            let ran = lab.run(&["lease", "release", ID1]);
            ran.expect_error("internal", None, Some(&msg_unknown_driver("release")));
            assert!(!ran.stderr_text().contains("secret-backend-name"), "{what}");
            assert!(handles.iter().all(|h| h.calls().is_empty()), "{what}");
            assert_eq!(lab.state_bytes(), before, "{what}: nothing is recorded");
        }
    }
}

#[test]
fn a_pending_lease_whose_driver_this_build_lacks_fails_internal_as_a_ready_one_does() {
    // docs/driver.md, Release a lease: a `pending` record with a `driver` is handed to that driver
    // as a `ready` one is (pin P16), so a driver this build does not have fails the release as it
    // does for a `ready` lease, without naming it, and the record stays `pending`.
    for drivers in [&["fake"][..], &[][..], &["ec2x", "other"][..]] {
        let mut lab = Lab::new();
        let handles: Vec<_> = drivers.iter().map(|n| lab.fake(n)).collect();
        lab.seed(&[Rec::pending(ID1, "swim").driver(Some("secret-backend-name"))]);
        let before = lab.state_bytes();
        let ran = lab.run(&["lease", "release", ID1]);
        ran.expect_error("internal", None, Some(&msg_unknown_driver("release")));
        assert!(!ran.stderr_text().contains("secret-backend-name"));
        assert!(ran.stdout.is_empty(), "Nothing on stdout");
        assert!(
            handles.iter().all(|h| h.calls().is_empty()),
            "No driver is asked"
        );
        assert_eq!(lab.state_bytes(), before, "The record is unchanged");
        assert_eq!(lab.records()[0]["state"], "pending");
    }
}

#[test]
fn a_ready_record_without_a_driver_fails_the_same_way_when_there_is_no_ec2_driver() {
    let lab = Lab::new();
    lab.seed(&[Rec::ready(ID1, "swim").driver(None)]);
    let ran = lab.run(&["lease", "release", ID1]);
    ran.expect_error("internal", None, Some(&msg_unknown_driver("release")));
    assert!(!ran.stderr_text().contains("ec2"));
}

// ---- Failures ----------------------------------------------------------------------------------

#[test]
fn a_failed_check_changes_nothing_and_releases_nothing() {
    for rec in [Rec::ready(ID1, "swim"), Rec::released(ID1, "swim")] {
        let (lab, h) = Lab::with_fake();
        lab.seed(std::slice::from_ref(&rec));
        let before = lab.state_bytes();
        h.script_check_release(Err(fail(MSG_FOREIGN_ALIAS)));
        lab.run(&["lease", "release", ID1]).expect_error(
            "internal",
            None,
            Some(&format!(
                "agentspace lease release failed: {MSG_FOREIGN_ALIAS}"
            )),
        );
        assert_eq!(h.calls(), [check(ID1, "swim")]);
        assert_eq!(lab.state_bytes(), before, "The record is not written");
    }
}

#[test]
fn a_failed_delete_leaves_the_record_released_and_a_retry_finishes() {
    let (mut lab, h) = Lab::with_fake();
    let rec = Rec::ready(ID1, "swim");
    lab.seed(std::slice::from_ref(&rec));
    h.script_release(Err(fail(MSG_NOT_DELETED)));
    lab.set_now(NOW + 100);
    lab.run(&["lease", "release", ID1]).expect_error(
        "internal",
        None,
        Some(&format!(
            "agentspace lease release failed: {MSG_NOT_DELETED}"
        )),
    );
    assert_eq!(h.calls(), both(ID1));
    let mut want_rec = released_doc(&rec, NOW + 100);
    want_rec["driver"] = json!("fake");
    assert_eq!(
        lab.records(),
        [want_rec.clone()],
        "The record stays released, never ready without its files"
    );
    let after_failure = lab.state_bytes();

    // The retry, a little later, finishes the job and keeps the first released_at.
    h.clear();
    lab.set_now(NOW + 500);
    let doc = lab.run(&["lease", "release", ID1]).lease_doc();
    assert_eq!(doc, released_doc(&rec, NOW + 100));
    assert_eq!(h.calls(), both(ID1));
    assert_eq!(
        lab.state_bytes(),
        after_failure,
        "The retry changes no record"
    );
}

#[test]
fn a_failed_delete_prints_each_leftover_once_and_escapes_control_characters() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let l1 = "~/.agentspace/leases/3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f/id_ed25519";
    let message =
        format!("The lease's local key could not be deleted: {l1}\nRun release again to finish.");
    h.script_release(Err(fail_with(&message, &[l1])));
    let text = lab.run(&["lease", "release", ID1]).internal();
    assert_eq!(
        text,
        format!(
            "agentspace lease release failed: The lease's local key could not be deleted: {l1}\\u000aRun release again to finish."
        )
    );
    assert_eq!(count_of(&text, l1), 1);
}

#[cfg(windows)]
#[test]
fn a_record_that_cannot_be_written_is_a_failed_release_before_any_file_is_deleted() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    std::fs::write(lab.lock_file(), b"").unwrap();
    let before = lab.state_bytes();
    let _deny = lab::DenyWrite::new(&lab.state);
    let text = lab.run(&["lease", "release", ID1]).internal();
    assert!(
        text.starts_with("The lease records could not be written: ")
            && text.ends_with(". Nothing was changed."),
        "{text}"
    );
    assert_eq!(
        h.calls(),
        [check(ID1, "swim")],
        "release is not called when the record was not written"
    );
    assert_eq!(lab.state_bytes(), before);
}

#[cfg(windows)]
#[test]
fn release_holds_the_state_lock_through_the_driver_calls() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    h.set_probe(lab::lock_probe(lab.lock_file()));
    lab.run(&["lease", "release", ID1]).lease_doc();
    assert_eq!(h.probes(), ["held", "held"]);
}

#[cfg(windows)]
#[test]
fn release_reads_the_clock_once_it_holds_the_lock() {
    // Read before the lock is taken, the clock says NOW; read once it is held, as after a wait on
    // it, it says NOW + 100. The lease is released at the time it was read under the lock.
    let (mut lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    std::fs::write(lab.lock_file(), b"").unwrap();
    lab.clock_by_lock(NOW, NOW + 100);
    let ran = lab.run(&["lease", "release", ID1]);
    let doc = ran.lease_doc();
    assert_eq!(doc["released_at"], fmt_time(NOW + 100));
    assert_eq!(
        lab.record_of(ID1).unwrap()["released_at"],
        fmt_time(NOW + 100)
    );
    assert_eq!(ran.now_calls, 1, "The clock is read once per command");
    assert_eq!(h.calls(), both(ID1));
}

#[test]
fn release_succeeds_with_a_proxy_command_that_is_not_valid_text_and_passes_no_override() {
    // lease-api.md, Environment: release does not read AGENTSPACE_PROXY_COMMAND.
    let (mut lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    lab.set_var(
        "AGENTSPACE_PROXY_COMMAND",
        common::not_unicode("ssh", "jump"),
    );
    let doc = lab.run(&["lease", "release", ID1]).lease_doc();
    assert_eq!(doc["state"], "released");
    assert_eq!(h.calls(), both(ID1));
    for c in h.contexts() {
        assert_eq!(c.proxy_override, None);
    }
}

#[test]
fn release_hands_the_driver_the_ssh_folder_it_was_given() {
    let (mut lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim"), Rec::ready(ID2, "swim")]);
    lab.run(&["lease", "release", ID1]).lease_doc();
    let contexts = h.contexts();
    assert_eq!(
        contexts.len(),
        2,
        "One for the check and one for the release"
    );
    for c in &contexts {
        assert_eq!(c.home, lab.home);
        assert_eq!(c.ssh_dir, lab.ssh);
        assert_eq!(c.own_exe, lab.exe);
    }
    h.clear();
    lab.unset_var("AGENTSPACE_SSH_DIR");
    lab.run(&["lease", "release", ID2]).lease_doc();
    assert_eq!(h.contexts()[0].ssh_dir, lab.home.join(".ssh"));
}

// ---- Errors in lease-api.md's order ------------------------------------------------------------

#[test]
fn release_of_an_unknown_lease_is_not_found() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let before = lab.state_bytes();
    lab.run(&["lease", "release", &format!(" {} ", ID2.to_uppercase())])
        .expect_lease_error("not_found", ID2, None, &msg_not_found(ID2));
    assert!(h.calls().is_empty());
    assert_eq!(lab.state_bytes(), before);
}

#[test]
fn release_usage_and_value_errors() {
    let (lab, h) = Lab::with_fake();
    lab.seed(&[Rec::ready(ID1, "swim")]);
    let run = |args: &[&str]| {
        let mut line = vec!["lease", "release"];
        line.extend_from_slice(args);
        lab.run(&line)
    };
    run(&[]).expect_error("usage", Some("id"), Some(&msg_no_id("release")));
    run(&[ID1, ID2]).expect_error("usage", None, Some(&msg_unexpected(ID2, "release")));
    run(&[ID1, "--ttl", "8h"]).expect_error(
        "usage",
        None,
        Some(&msg_unknown_option_sub("--ttl", "release")),
    );
    run(&["not-an-id", "--ttl", "8h"]).expect_error(
        "usage",
        None,
        Some(&msg_unknown_option_sub("--ttl", "release")),
    );
    run(&["not-an-id"]).expect_error("invalid_value", Some("id"), Some(&msg_bad_id("not-an-id")));
    lab.seed_raw(b"{");
    run(&["not-an-id"]).expect_error("invalid_value", Some("id"), Some(&msg_bad_id("not-an-id")));
    run(&[ID1]).expect_error("internal", None, Some(lab::MSG_NOT_VALID));
    assert!(h.calls().is_empty());
}

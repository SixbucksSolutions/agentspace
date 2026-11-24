//! `sized_for` in the lease document, in every state a lease can be in, and no `plan` or `driver`
//! in any output (docs/lease-api.md, Fields and Output; docs/state.md, What a plan lease adds;
//! D74 to D76, D81).
//!
//! Member order is checked on the serialized text, not on a parsed map. A document is checked
//! against v1's lease output schema with `validate_output` and with `validate_schema` by the
//! schema's name, the two ways the crate offers, which share its draft 2020-12 validator.

mod common;
mod plan_support;

use agentspace_state::{
    OutputKind, SizedFor, StateFile, lease_document, list_document, parse, validate_output,
    validate_schema,
};
use common::*;
use plan_support::*;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

const BEFORE: i64 = EXPIRES_UNIX - 1;
const AT: i64 = EXPIRES_UNIX;
const NAMED: &str = r#"{"plan":"v0.10","turn":"T01","agents":2}"#;
const UNNAMED: &str = r#"{"turn":"T01","agents":2}"#;
const RELEASED_AT: &str = "2026-11-19T10:30:00Z";
const UNSUPPORTED_PLATFORM: &str = r#"[{"code":"unsupported_platform","message":"Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only."}]"#;

fn parsed(records: Vec<Value>) -> StateFile {
    parse(&bytes_of(records)).expect("Fixture must parse")
}

fn doc_text(rec: Value, now: i64) -> String {
    let f = parsed(vec![rec]);
    serde_json::to_string(&lease_document(&f.leases[0], now)).unwrap()
}

/// The text of a lease document up to `created_at`, in lease-api.md's order: id, project, state,
/// platform, profile, size, sized_for, ttl_seconds, created_at. The tail is the caller's, with
/// its own leading comma and the closing brace.
fn head(id: &str, state: &str, platform: &str, size: &str, sized_for: &str) -> String {
    format!(
        "{{\"id\":\"{id}\",\"project\":\"demo\",\"state\":\"{state}\",\"platform\":\"{platform}\",\
         \"profile\":\"agent-linux\",\"size\":\"{size}\",\"sized_for\":{sized_for},\
         \"ttl_seconds\":3600,\"created_at\":\"{CREATED}\""
    )
}

const SSH: &str = r#","ssh":{"host_alias":"agentspace-demo-3f2a9c1e","user":"ubuntu"}"#;

fn too_large_reasons() -> String {
    format!(r#","reasons":[{{"code":"plan_too_large","message":"{TOO_LARGE_MESSAGE}"}}]"#)
}

// ---- One document per state, as exact text ----

#[test]
fn a_pending_plan_lease_prints_sized_for_after_size() {
    let want = format!("{}}}", head(ID_A, "pending", "linux/arm64", "s", NAMED));
    assert_eq!(doc_text(plan_pending(ID_A), BEFORE), want);
    // A pending lease reads the same whatever the time.
    assert_eq!(doc_text(plan_pending(ID_A), i64::MAX), want);
}

#[test]
fn a_ready_plan_lease_prints_sized_for_before_ttl_and_ssh_last() {
    let want = format!(
        "{},\"expires_at\":\"{EXPIRES}\"{SSH}}}",
        head(ID_A, "ready", "linux/arm64", "s", NAMED)
    );
    assert_eq!(doc_text(plan_ready(ID_A), BEFORE), want);
}

#[test]
fn an_expired_plan_lease_keeps_sized_for_and_loses_ssh() {
    let want = format!(
        "{},\"expires_at\":\"{EXPIRES}\"}}",
        head(ID_A, "expired", "linux/arm64", "s", NAMED)
    );
    for now in [AT, AT + 1, i64::MAX] {
        assert_eq!(doc_text(plan_ready(ID_A), now), want, "now = {now}");
    }
}

#[test]
fn a_released_plan_lease_keeps_sized_for_whether_it_was_ever_ready() {
    let from_ready = format!(
        "{},\"expires_at\":\"{EXPIRES}\",\"released_at\":\"{RELEASED_AT}\"}}",
        head(ID_A, "released", "linux/arm64", "s", NAMED)
    );
    let from_pending = format!(
        "{},\"released_at\":\"{RELEASED_AT}\"}}",
        head(ID_A, "released", "linux/arm64", "s", NAMED)
    );
    for now in [BEFORE, AT, i64::MAX] {
        assert_eq!(doc_text(plan_released_from_ready(ID_A), now), from_ready);
        assert_eq!(
            doc_text(plan_released_from_pending(ID_A), now),
            from_pending
        );
    }
}

#[test]
fn an_unsatisfiable_plan_lease_keeps_sized_for_before_its_reasons() {
    let want = format!(
        "{},\"reasons\":{UNSUPPORTED_PLATFORM}}}",
        head(ID_A, "unsatisfiable", "windows/amd64", "s", NAMED)
    );
    assert_eq!(doc_text(plan_unsatisfiable(ID_A), BEFORE), want);
    assert_eq!(doc_text(plan_unsatisfiable(ID_A), i64::MAX), want);
}

#[test]
fn a_plan_too_large_lease_prints_xl_its_one_reason_and_sized_for() {
    let sf = r#"{"plan":"v0.11","turn":"T02","agents":33}"#;
    let want = format!(
        "{}{}}}",
        head(ID_A, "unsatisfiable", "linux/arm64", "xl", sf),
        too_large_reasons()
    );
    assert_eq!(doc_text(plan_too_large(ID_A), BEFORE), want);
    assert_eq!(doc_text(plan_too_large(ID_A), i64::MAX), want);
}

#[test]
fn a_plan_name_that_is_absent_leaves_plan_out_of_sized_for() {
    // The plan has no name: sized_for is {"turn", "agents"}, never "plan":null.
    let rec = with(
        plan_pending(ID_A),
        "sized_for",
        Some(sized_for(None, "T01", 2)),
    );
    let want = format!("{}}}", head(ID_A, "pending", "linux/arm64", "s", UNNAMED));
    let got = doc_text(rec, BEFORE);
    assert_eq!(got, want);
    assert!(!got.contains("null"), "{got}");
}

#[test]
fn sized_for_members_stand_in_the_order_plan_turn_agents_whatever_the_stored_order() {
    // A stored sized_for whose members are in another order is read, and printed in the table's.
    let rec = with(
        plan_pending(ID_A),
        "sized_for",
        Some(json!({"agents": 2, "turn": "T01", "plan": "v0.10"})),
    );
    let want = format!("{}}}", head(ID_A, "pending", "linux/arm64", "s", NAMED));
    assert_eq!(doc_text(rec, BEFORE), want);
}

#[test]
fn a_planted_wrong_order_is_not_equal_to_the_text() {
    // The checks above compare text; this shows a document whose sized_for stands after
    // created_at, as the v1 samples print it, is not what they accept.
    let planted = format!(
        "{{\"id\":\"{ID_A}\",\"project\":\"demo\",\"state\":\"pending\",\"platform\":\"linux/arm64\",\
         \"profile\":\"agent-linux\",\"size\":\"s\",\"ttl_seconds\":3600,\"created_at\":\"{CREATED}\",\
         \"sized_for\":{NAMED}}}"
    );
    assert_ne!(doc_text(plan_pending(ID_A), BEFORE), planted);
}

// ---- A lease made without a plan has no sized_for ----

#[test]
fn a_lease_made_without_a_plan_prints_no_sized_for_in_any_state() {
    let id = |n: u8| format!("{n:x}f2a9c1e-1111-4222-8333-444455556666");
    let recs = vec![
        pending(&id(1), "demo"),
        ready(&id(2), "demo"),
        released_from_ready(&id(3), "demo"),
        with(released_from_ready(&id(4), "demo"), "expires_at", None),
        unsatisfiable(&id(5), "demo"),
    ];
    let f = parsed(recs);
    for now in [BEFORE, AT, i64::MAX] {
        for r in &f.leases {
            let d = lease_document(r, now);
            let t = serde_json::to_string(&d).unwrap();
            assert!(d.get("sized_for").is_none(), "{t}");
            assert!(
                !t.contains("sized_for") && !t.contains("plan_too_large"),
                "{t}"
            );
            assert!(validate_output(OutputKind::Lease, &d), "{t}");
        }
    }
    // And its text is v0's: the ready one, whole.
    let want = format!(
        "{{\"id\":\"{ID_A}\",\"project\":\"demo\",\"state\":\"ready\",\"platform\":\"linux/arm64\",\
         \"profile\":\"agent-linux\",\"size\":\"m\",\"ttl_seconds\":3600,\"created_at\":\"{CREATED}\",\
         \"expires_at\":\"{EXPIRES}\"{SSH}}}"
    );
    assert_eq!(doc_text(ready(ID_A, "demo"), BEFORE), want);
}

#[test]
fn a_plan_lease_prints_what_the_same_lease_without_a_plan_prints_plus_sized_for() {
    let pairs: Vec<(Value, Value)> = vec![
        (
            plan_pending(ID_A),
            with(pending(ID_A, "demo"), "size", Some(json!("s"))),
        ),
        (
            plan_ready(ID_A),
            with(ready(ID_A, "demo"), "size", Some(json!("s"))),
        ),
        (
            plan_released_from_ready(ID_A),
            with(released_from_ready(ID_A, "demo"), "size", Some(json!("s"))),
        ),
        (
            plan_released_from_pending(ID_A),
            with(
                with(released_from_ready(ID_A, "demo"), "expires_at", None),
                "size",
                Some(json!("s")),
            ),
        ),
        (
            plan_unsatisfiable(ID_A),
            with(unsatisfiable(ID_A, "demo"), "size", Some(json!("s"))),
        ),
    ];
    for (planned_rec, plain) in pairs {
        let state = planned_rec["state"].as_str().unwrap().to_string();
        for now in [BEFORE, AT] {
            let with_plan = {
                let f = parsed(vec![planned_rec.clone()]);
                lease_document(&f.leases[0], now)
            };
            let without = {
                let f = parsed(vec![plain.clone()]);
                lease_document(&f.leases[0], now)
            };
            assert!(with_plan.get("sized_for").is_some(), "{state}");
            assert!(without.get("sized_for").is_none(), "{state}");
            let mut stripped = with_plan.clone();
            stripped.as_object_mut().unwrap().shift_remove("sized_for");
            assert_eq!(stripped, without, "{state} at {now}");
        }
    }
}

#[test]
fn a_record_given_or_stripped_of_its_members_prints_or_loses_sized_for() {
    let mut f = parsed(vec![with(pending(ID_A, "demo"), "size", Some(json!("s")))]);
    assert!(
        lease_document(&f.leases[0], BEFORE)
            .get("sized_for")
            .is_none()
    );
    f.leases[0].sized_for = Some(SizedFor {
        plan: Some("v0.10".to_string()),
        turn: "T01".to_string(),
        agents: 2,
    });
    f.leases[0].plan = Some(minimal_plan());
    let t = serde_json::to_string(&lease_document(&f.leases[0], BEFORE)).unwrap();
    assert_eq!(
        t,
        format!("{}}}", head(ID_A, "pending", "linux/arm64", "s", NAMED))
    );
    f.leases[0].sized_for = None;
    f.leases[0].plan = None;
    assert!(
        lease_document(&f.leases[0], BEFORE)
            .get("sized_for")
            .is_none()
    );
}

// ---- The plan and the driver reach no output ----

#[test]
fn the_plan_and_the_driver_are_in_no_document() {
    let rich = |rec: Value| with(rec, "plan", Some(rich_plan()));
    let recs = vec![
        rich(plan_pending(ID_A)),
        rich(plan_ready(ID_B)),
        rich(plan_released_from_ready(ID_C)),
        rich(plan_released_from_pending(
            "a1b2c3d4-5555-4666-a777-888899990000",
        )),
        rich(plan_unsatisfiable("b1b2c3d4-5555-4666-a777-888899990000")),
        rich(plan_too_large("c1b2c3d4-5555-4666-a777-888899990000")),
    ];
    let f = parsed(recs);
    // Each of these is in the stored plan and in no output; "plan" and "v0.10" are not here,
    // since sized_for holds the plan's name.
    let forbidden = [
        "\"turns\"",
        "stories",
        "subagents",
        "\"model\"",
        "\"effort\"",
        "drafter",
        "reviewer",
        "checker",
        "Opus",
        "Sonnet",
        "\"after\"",
        "\"driver\"",
        "ec2",
        "\"plan\":{",
    ];
    for now in [BEFORE, AT, i64::MAX] {
        for r in &f.leases {
            let d = lease_document(r, now);
            let t = serde_json::to_string(&d).unwrap();
            assert!(d.get("plan").is_none() && d.get("driver").is_none(), "{t}");
            for bad in forbidden {
                assert!(!t.contains(bad), "{bad} in {t}");
            }
            assert!(d.get("sized_for").is_some(), "{t}");
        }
        let l = serde_json::to_string(&list_document(&f, None, &[], now)).unwrap();
        for bad in forbidden {
            assert!(!l.contains(bad), "{bad} in the list at {now}");
        }
    }
}

// ---- Checked against v1's lease output schema ----

#[test]
fn every_plan_document_validates_both_ways() {
    let f = parsed(vec![
        plan_pending(ID_A),
        plan_ready(ID_B),
        plan_released_from_ready(ID_C),
        plan_released_from_pending("a1b2c3d4-5555-4666-a777-888899990000"),
        plan_unsatisfiable("b1b2c3d4-5555-4666-a777-888899990000"),
        plan_too_large("c1b2c3d4-5555-4666-a777-888899990000"),
        with(
            plan_pending("d1b2c3d4-5555-4666-a777-888899990000"),
            "sized_for",
            Some(sized_for(None, "T01", 2)),
        ),
    ]);
    for now in [BEFORE, AT, i64::MAX] {
        for r in &f.leases {
            let d = lease_document(r, now);
            assert!(validate_output(OutputKind::Lease, &d), "{d}");
            assert_eq!(validate_schema("lease.output", &d), Some(true), "{d}");
        }
        let l = list_document(&f, None, &[], now);
        assert!(validate_output(OutputKind::LeaseList, &l), "{l}");
        assert_eq!(validate_schema("lease-list.output", &l), Some(true), "{l}");
    }
}

#[test]
fn the_checks_fail_a_document_with_a_planted_fault() {
    // Each plant is one change to a document that passes, so a check that always passed fails here.
    let f = parsed(vec![plan_ready(ID_A), plan_too_large(ID_B)]);
    let good = lease_document(&f.leases[0], BEFORE);
    let too_large = lease_document(&f.leases[1], BEFORE);
    assert!(validate_output(OutputKind::Lease, &good));
    assert!(validate_output(OutputKind::Lease, &too_large));
    let plant = |d: &Value, f: &dyn Fn(&mut Value)| {
        let mut p = d.clone();
        f(&mut p);
        p
    };
    let planted = [
        plant(&good, &|d| d["sized_for"]["agents"] = json!(1)),
        plant(&good, &|d| d["sized_for"]["agents"] = json!(1026)),
        plant(&good, &|d| d["sized_for"]["plan"] = Value::Null),
        plant(&good, &|d| d["sized_for"] = Value::Null),
        plant(&good, &|d| d["sized_for"]["extra"] = json!(1)),
        plant(&good, &|d| {
            d["sized_for"].as_object_mut().unwrap().shift_remove("turn");
        }),
        plant(&good, &|d| d["plan"] = rich_plan()),
        plant(&good, &|d| d["driver"] = json!("ec2")),
        plant(&too_large, &|d| d["size"] = json!("m")),
        plant(&too_large, &|d| {
            d.as_object_mut().unwrap().shift_remove("sized_for");
        }),
        plant(&too_large, &|d| {
            d["reasons"] = json!([
                {"code": "plan_too_large", "message": "Too many."},
                {"code": "no_capacity", "message": "None."}
            ]);
        }),
    ];
    for d in planted {
        assert!(!validate_output(OutputKind::Lease, &d), "{d}");
        assert_eq!(validate_schema("lease.output", &d), Some(false), "{d}");
    }
}

// ---- The v1 samples ----

fn samples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/samples")
}

fn sample_names(prefix: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(samples_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix) && n.ends_with(".json"))
        .collect();
    v.sort();
    v
}

fn sample(name: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(samples_dir().join(name)).unwrap()).unwrap()
}

/// A stored record that prints as `doc`, and the time to print it at. The stored record is the
/// document with `driver` and `plan` added, and an `expired` document stored as `ready` with an
/// `ssh` member, since `expired` is never stored (docs/state.md, The lease state file).
fn record_for(doc: &Value) -> (Value, i64) {
    let mut rec = doc.clone();
    let mut now = 0;
    if rec["state"] == "expired" {
        rec["state"] = json!("ready");
        let alias = format!(
            "agentspace-{}-{}",
            doc["project"].as_str().unwrap(),
            &doc["id"].as_str().unwrap()[..8]
        );
        rec["ssh"] = json!({"host_alias": alias, "user": "ubuntu"});
        now = i64::MAX;
    }
    rec["driver"] = json!("ec2");
    // A lease made without a plan has neither sized_for nor plan.
    if doc.get("sized_for").is_some() {
        rec["plan"] = minimal_plan();
    }
    (rec, now)
}

#[test]
fn the_v1_plan_samples_are_what_the_records_print() {
    let names = sample_names("lease.output.plan-");
    assert!(names.len() >= 12, "{names:?}");
    for n in names {
        let doc = sample(&n);
        assert!(
            validate_output(OutputKind::Lease, &doc),
            "{n} is a v1 document"
        );
        assert!(doc.get("sized_for").is_some(), "{n}");
        let (rec, now) = record_for(&doc);
        let f = parse(&bytes_of(vec![rec])).unwrap_or_else(|_| panic!("{n} must be storable"));
        assert_eq!(lease_document(&f.leases[0], now), doc, "{n}");
    }
}

#[test]
fn the_v1_list_samples_hold_the_documents_the_records_print() {
    for n in [
        "lease-list.output.plan-mixed.json",
        "lease-list.output.plan-only.json",
    ] {
        let list = sample(n);
        assert!(validate_output(OutputKind::LeaseList, &list), "{n}");
        let items = list["leases"].as_array().unwrap();
        assert!(!items.is_empty(), "{n}");
        for doc in items {
            let (rec, now) = record_for(doc);
            let f = parse(&bytes_of(vec![rec])).unwrap_or_else(|_| panic!("{n} must be storable"));
            assert_eq!(lease_document(&f.leases[0], now), *doc, "{n}");
        }
    }
}

#[test]
fn the_v1_sized_for_samples_that_must_fail_fail() {
    let dir = samples_dir().join("invalid");
    let mut n = 0;
    for e in fs::read_dir(dir).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        let leasey = (name.starts_with("lease.output.") && name.contains("sized-for"))
            || name.starts_with("lease.output.plan-too-large");
        if leasey {
            let d: Value = serde_json::from_str(
                &fs::read_to_string(samples_dir().join("invalid").join(&name)).unwrap(),
            )
            .unwrap();
            assert!(!validate_output(OutputKind::Lease, &d), "{name}");
            assert_eq!(validate_schema("lease.output", &d), Some(false), "{name}");
            n += 1;
        }
    }
    assert!(n >= 20, "Only {n} samples were found");
}

// ---- The list ----

#[test]
fn a_list_prints_each_lease_as_the_lease_does_with_sized_for_only_where_it_has_one() {
    // All created at one second, so the ID order decides: ID_C (0a..), ID_A (3f..), ID_B (9b..).
    let f = parsed(vec![
        plan_ready(ID_A),
        plan_pending(ID_B),
        pending(ID_C, "demo"),
    ]);
    let list = list_document(&f, None, &[], BEFORE);
    let text = serde_json::to_string(&list).unwrap();
    let docs: Vec<String> = [ID_C, ID_A, ID_B]
        .iter()
        .map(|id| {
            let r = f.leases.iter().find(|r| r.id == *id).unwrap();
            serde_json::to_string(&lease_document(r, BEFORE)).unwrap()
        })
        .collect();
    assert_eq!(text, format!("{{\"leases\":[{}]}}", docs.join(",")));
    assert!(!docs[0].contains("sized_for"));
    assert!(docs[1].contains("\"size\":\"s\",\"sized_for\":"));
    assert!(docs[2].contains("\"size\":\"s\",\"sized_for\":"));
    assert!(validate_output(OutputKind::LeaseList, &list));
}

#[test]
fn a_list_keeps_sized_for_through_the_filters_and_after_expiry() {
    let f = parsed(vec![
        plan_ready(ID_A),
        plan_too_large(ID_B),
        pending(ID_C, "demo"),
    ]);
    let expired = list_document(&f, None, &["expired"], AT);
    let item = &expired["leases"][0];
    assert_eq!(item["state"], "expired");
    assert_eq!(
        item["sized_for"],
        json!({"plan": "v0.10", "turn": "T01", "agents": 2})
    );
    assert!(item.get("ssh").is_none());
    let unsat = list_document(&f, Some("demo"), &["unsatisfiable"], AT);
    assert_eq!(unsat["leases"].as_array().unwrap().len(), 1);
    assert_eq!(unsat["leases"][0]["size"], "xl");
    assert_eq!(unsat["leases"][0]["sized_for"]["agents"], 33);
    assert_eq!(unsat["leases"][0]["reasons"].as_array().unwrap().len(), 1);
    assert_eq!(unsat["leases"][0]["reasons"][0]["code"], "plan_too_large");
    for l in [&expired, &unsat] {
        assert!(validate_output(OutputKind::LeaseList, l));
    }
}

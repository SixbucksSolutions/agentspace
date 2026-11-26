//! The lease record's `plan_source` (docs/state.md, The lease state file and What a plan lease
//! adds; the state schema schemas/state/leases.state.schema.json; D86, D87, D89).
//!
//! Every refusal is checked twice, by `parse` and by the schema's own name, and each family starts
//! from a baseline that is valid, so a checker that refuses everything fails on the baseline.

mod common;
mod plan_support;

use agentspace_state::{
    PlanSource, Record, StateFile, lease_document, parse, serialize, validate_schema,
};
use common::*;
use plan_support::*;
use serde_json::{Value, json};

const BOARD: &str = "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f";
const MAX: u64 = 9_007_199_254_740_991;
const SAMPLE: &str = "leases.state.plan-source.json";

fn source(board: Value, ticket: Value, revision: Value) -> Value {
    json!({"board": board, "ticket": ticket, "revision": revision})
}

fn good_source() -> Value {
    source(json!(BOARD), json!(233), json!(7))
}

/// A record with `plan_source` appended after its last member, where state.md writes it.
fn sourced(rec: Value, src: Value) -> Value {
    with(rec, "plan_source", Some(src))
}

fn base() -> Value {
    sourced(plan_pending(ID_A), good_source())
}

fn parsed(records: Vec<Value>) -> StateFile {
    parse(&bytes_of(records)).expect("Fixture must parse")
}

fn text_of(f: &StateFile) -> String {
    String::from_utf8(serialize(f).expect("Must serialize")).unwrap()
}

fn compact(doc: &Value) -> String {
    format!("{}\n", serde_json::to_string(doc).unwrap())
}

/// Whether a file of this record is valid, by `parse` and by the schema, which must agree.
fn valid(rec: Value) -> bool {
    let records = vec![rec];
    let by_schema = validate_schema("leases.state", &file_of(records.clone()));
    let by_parse = parse(&bytes_of(records)).is_ok();
    assert_eq!(
        by_schema,
        Some(by_parse),
        "parse and the schema must agree on the file"
    );
    by_parse
}

/// The baseline with one `plan_source` member changed, or removed when `val` is `None`.
fn with_member(key: &str, val: Option<Value>) -> Value {
    let mut src = good_source();
    let o = src.as_object_mut().unwrap();
    match val {
        Some(v) => {
            o.insert(key.to_string(), v);
        }
        None => {
            o.shift_remove(key);
        }
    }
    sourced(plan_pending(ID_A), src)
}

fn source_of(r: &Record) -> Option<(String, u64, u64)> {
    r.plan_source
        .as_ref()
        .map(|s| (s.board.clone(), s.ticket, s.revision))
}

// ---- The baseline and the samples ----

#[test]
fn the_baseline_record_is_valid() {
    assert!(valid(base()));
    assert!(valid(plan_pending(ID_A)));
}

#[test]
fn the_plan_source_sample_parses_with_its_members() {
    let f = parse(state_sample_text(SAMPLE).as_bytes()).unwrap();
    assert_eq!(f.leases.len(), 4);
    assert_eq!(source_of(&f.leases[0]), Some((BOARD.to_string(), 233, 7)));
    assert_eq!(source_of(&f.leases[1]), Some((BOARD.to_string(), 1, 1)));
    assert_eq!(source_of(&f.leases[2]), Some((BOARD.to_string(), MAX, MAX)));
    // The fourth is a plan lease from a file: a plan, but no plan_source.
    assert!(source_of(&f.leases[3]).is_none());
    assert!(f.leases[3].plan.is_some());
    assert_eq!(
        f.leases[0].plan_source,
        Some(PlanSource {
            board: BOARD.to_string(),
            ticket: 233,
            revision: 7
        })
    );
}

#[test]
fn the_plan_source_sample_is_valid_by_the_schema_and_writes_back_as_its_own_text() {
    assert_eq!(
        validate_schema("leases.state", &state_sample(SAMPLE)),
        Some(true)
    );
    let f = parse(state_sample_text(SAMPLE).as_bytes()).unwrap();
    assert_eq!(text_of(&f), compact(&state_sample(SAMPLE)));
}

#[test]
fn every_valid_state_sample_reads_and_writes_back_as_its_own_text() {
    let names = state_sample_names("");
    assert!(names.len() >= 6, "{names:?}");
    assert!(names.iter().any(|n| n == SAMPLE), "{names:?}");
    for n in names {
        let doc = state_sample(&n);
        assert_eq!(validate_schema("leases.state", &doc), Some(true), "{n}");
        let f = parse(state_sample_text(&n).as_bytes()).unwrap_or_else(|_| panic!("{n}"));
        assert_eq!(text_of(&f), compact(&doc), "{n}");
        // Only the plan_source sample's board leases hold the member.
        if n != SAMPLE {
            assert!(f.leases.iter().all(|r| r.plan_source.is_none()), "{n}");
        }
    }
}

#[test]
fn the_v0_10_samples_without_plan_source_still_read() {
    for n in [
        "leases.state.plan-leases.json",
        "leases.state.smallest-plan.json",
        "leases.state.without-plan.json",
        "leases.state.plan-too-large.json",
        "leases.state.empty.json",
    ] {
        let f = parse(state_sample_text(n).as_bytes()).unwrap_or_else(|_| panic!("{n}"));
        assert!(f.leases.iter().all(|r| r.plan_source.is_none()), "{n}");
        assert!(!text_of(&f).contains("plan_source"), "{n}");
    }
}

#[test]
fn the_plan_source_invalid_samples_are_present_and_refused() {
    let names = state_sample_names("invalid");
    for want in [
        "leases.state.plan-source-extra-member.json",
        "leases.state.plan-source-missing-board.json",
        "leases.state.plan-source-missing-revision.json",
        "leases.state.plan-source-missing-ticket.json",
        "leases.state.plan-source-null.json",
        "leases.state.plan-source-revision-zero.json",
        "leases.state.plan-source-ticket-string.json",
        "leases.state.plan-source-ticket-too-large.json",
        "leases.state.plan-source-ticket-zero.json",
        "leases.state.plan-source-uppercase-board.json",
        "leases.state.plan-source-without-plan.json",
    ] {
        assert!(names.iter().any(|n| n == want), "{want} is missing");
    }
    for n in names {
        let rel = format!("invalid/{n}");
        assert_eq!(
            validate_schema("leases.state", &state_sample(&rel)),
            Some(false),
            "{n}"
        );
        assert!(parse(state_sample_text(&rel).as_bytes()).is_err(), "{n}");
    }
}

// ---- A record with it, and one without ----

#[test]
fn a_record_with_plan_source_keeps_it_through_a_round_trip() {
    let f = parsed(vec![base()]);
    assert_eq!(source_of(&f.leases[0]), Some((BOARD.to_string(), 233, 7)));
    let back = parse(&serialize(&f).unwrap()).unwrap();
    assert_eq!(
        source_of(&back.leases[0]),
        Some((BOARD.to_string(), 233, 7))
    );
    assert_eq!(serialize(&f).unwrap(), serialize(&back).unwrap());
}

#[test]
fn a_record_without_plan_source_reads_as_before_and_writes_back_without_the_member() {
    for rec in [
        plan_pending(ID_A),
        pending(ID_A, "demo"),
        ready(ID_B, "demo"),
    ] {
        let f = parsed(vec![rec.clone()]);
        assert!(f.leases[0].plan_source.is_none());
        let text = text_of(&f);
        assert!(!text.contains("plan_source"), "{text}");
        assert_eq!(text, compact(&file_of(vec![rec])));
    }
}

#[test]
fn a_record_in_each_state_keeps_plan_source_through_a_round_trip() {
    let recs: Vec<Value> = vec![
        plan_pending(ID_A),
        plan_ready(ID_B),
        plan_released_from_ready(ID_C),
        plan_released_from_pending("a1b2c3d4-5555-4666-a777-888899990000"),
        plan_unsatisfiable("b1b2c3d4-5555-4666-a777-888899990000"),
        plan_too_large("0b1c2d3e-0000-4000-8000-000000000a01"),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, r)| sourced(r, source(json!(BOARD), json!(10 + i), json!(20 + i))))
    .collect();
    let f = parsed(recs.clone());
    let back = parse(&serialize(&f).unwrap()).unwrap();
    for (i, r) in back.leases.iter().enumerate() {
        assert_eq!(
            source_of(r),
            Some((BOARD.to_string(), 10 + i as u64, 20 + i as u64)),
            "{}",
            r.id
        );
        assert_eq!(r.plan.as_ref().unwrap(), &recs[i]["plan"], "{}", r.id);
    }
    assert_eq!(text_of(&f), text_of(&back));
}

#[test]
fn plan_source_never_reaches_the_lease_document() {
    let f = parsed(vec![sourced(plan_ready(ID_B), good_source())]);
    let doc = lease_document(&f.leases[0], EXPIRES_UNIX - 60);
    let text = serde_json::to_string(&doc).unwrap();
    assert!(!text.contains("plan_source"), "{text}");
    assert!(!text.contains(BOARD), "{text}");
}

// ---- Written last, after plan ----

#[test]
fn plan_source_is_written_last_after_plan() {
    for rec in [
        plan_pending(ID_A),
        plan_ready(ID_A),
        plan_released_from_ready(ID_A),
        plan_unsatisfiable(ID_A),
        plan_too_large(ID_A),
    ] {
        let f = parsed(vec![sourced(rec, good_source())]);
        let text = text_of(&f);
        let tail = format!(
            ",\"plan_source\":{{\"board\":\"{BOARD}\",\"ticket\":233,\"revision\":7}}}}]}}\n"
        );
        assert!(text.ends_with(&tail), "{text}");
        assert_eq!(text.matches("\"plan_source\"").count(), 1, "{text}");
        let plan_at = text.find(",\"plan\":{").expect("plan is written");
        let driver_at = text.find("\"driver\":").expect("driver is written");
        assert!(driver_at < plan_at, "{text}");
        assert!(plan_at < text.find("\"plan_source\"").unwrap(), "{text}");
    }
}

#[test]
fn plan_source_is_written_last_whatever_order_it_was_read_in() {
    // A reader MUST NOT depend on order, and the writer fixes it.
    let mut rec = base();
    let o = rec.as_object_mut().unwrap();
    let src = o.shift_remove("plan_source").unwrap();
    let mut out = serde_json::Map::new();
    out.insert("plan_source".to_string(), src);
    for (k, v) in o.iter() {
        out.insert(k.clone(), v.clone());
    }
    let f = parsed(vec![Value::Object(out)]);
    let text = text_of(&f);
    assert!(text.ends_with("\"revision\":7}}]}\n"), "{text}");
    let at = text.find("\"plan_source\"").unwrap();
    assert!(text.find(",\"plan\":{").unwrap() < at, "{text}");
}

#[test]
fn the_members_of_plan_source_are_written_board_ticket_revision() {
    let text = text_of(&parsed(vec![base()]));
    assert!(
        text.contains(&format!(
            "\"plan_source\":{{\"board\":\"{BOARD}\",\"ticket\":233,\"revision\":7}}"
        )),
        "{text}"
    );
}

// ---- Without a plan ----

#[test]
fn plan_source_without_plan_is_refused() {
    // On records with no plan and no sized_for.
    assert!(!valid(sourced(pending(ID_A, "demo"), good_source())));
    assert!(!valid(sourced(ready(ID_A, "demo"), good_source())));
    // The baseline with its plan dropped, beside sized_for and driver.
    assert!(!valid(with(base(), "plan", None)));
    // With sized_for dropped too.
    assert!(!valid(with(with(base(), "plan", None), "sized_for", None)));
}

// ---- Each member's bounds ----

#[test]
fn ticket_bounds() {
    let t = |v: Value| {
        valid(sourced(
            plan_pending(ID_A),
            source(json!(BOARD), v, json!(7)),
        ))
    };
    assert!(!t(json!(0)), "0");
    assert!(t(json!(1)), "1");
    assert!(t(json!(2)), "2");
    assert!(t(json!(MAX - 1)), "MAX - 1");
    assert!(t(json!(MAX)), "MAX");
    assert!(!t(json!(MAX + 1)), "MAX + 1");
    assert!(!t(json!(-1)), "-1");
    assert!(!t(json!(1.5)), "1.5");
    assert!(!t(json!("233")), "a string");
    assert!(!t(json!(true)), "a bool");
    assert!(!t(Value::Null), "null");
    assert!(!t(json!([233])), "an array");
}

#[test]
fn revision_bounds() {
    let r = |v: Value| {
        valid(sourced(
            plan_pending(ID_A),
            source(json!(BOARD), json!(233), v),
        ))
    };
    assert!(!r(json!(0)), "0");
    assert!(r(json!(1)), "1");
    assert!(r(json!(2)), "2");
    assert!(r(json!(MAX - 1)), "MAX - 1");
    assert!(r(json!(MAX)), "MAX");
    assert!(!r(json!(MAX + 1)), "MAX + 1");
    assert!(!r(json!(-1)), "-1");
    assert!(!r(json!(0.5)), "0.5");
    assert!(!r(json!("7")), "a string");
    assert!(!r(json!(false)), "a bool");
    assert!(!r(Value::Null), "null");
}

/// The schema's "integer" accepts a number written with a zero fraction, up to the largest bound,
/// so a read accepts it too, and the writer puts it back as digits alone.
#[test]
fn a_whole_number_written_with_a_zero_fraction_reads_as_the_schema_allows() {
    let text = String::from_utf8(bytes_of(vec![base()])).unwrap();
    for (from, to, want) in [
        ("\"ticket\":233", "\"ticket\":233.0", 233),
        ("\"ticket\":233", "\"ticket\":9007199254740990.0", MAX - 1),
        ("\"ticket\":233", "\"ticket\":9007199254740991.0", MAX),
    ] {
        let t = text.replace(from, to);
        assert_ne!(t, text, "{to}");
        let doc: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(validate_schema("leases.state", &doc), Some(true), "{to}");
        let f = parse(t.as_bytes()).unwrap_or_else(|_| panic!("{to}"));
        assert_eq!(source_of(&f.leases[0]), Some((BOARD.to_string(), want, 7)));
        assert!(text_of(&f).contains(&format!("\"ticket\":{want},")), "{to}");
    }
}

#[test]
fn a_number_beyond_u64_is_refused_and_does_not_panic() {
    let text = String::from_utf8(bytes_of(vec![base()])).unwrap();
    let huge = text.replace("\"ticket\":233", "\"ticket\":18446744073709551616");
    assert_ne!(text, huge);
    assert!(parse(huge.as_bytes()).is_err());
    let huge = text.replace(
        "\"revision\":7",
        "\"revision\":123456789012345678901234567890",
    );
    assert_ne!(text, huge);
    assert!(parse(huge.as_bytes()).is_err());
}

#[test]
fn board_must_be_a_lowercase_uuid() {
    let b = |v: Value| valid(sourced(plan_pending(ID_A), source(v, json!(233), json!(7))));
    assert!(b(json!(BOARD)));
    assert!(b(json!("00000000-0000-0000-0000-000000000000")));
    assert!(b(json!("ffffffff-ffff-ffff-ffff-ffffffffffff")));
    // Uppercase, in whole or in part.
    assert!(!b(json!(BOARD.to_uppercase())));
    assert!(!b(json!("0A1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f")));
    assert!(!b(json!("0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4E5f")));
    // Not a UUID.
    assert!(!b(json!("")));
    assert!(!b(json!("not-a-uuid")));
    assert!(!b(json!("0a1b2c3d0a1b4c2d8e3f0a1b2c3d4e5f")), "no hyphens");
    assert!(
        !b(json!("0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5")),
        "one short"
    );
    assert!(
        !b(json!("0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f0")),
        "one long"
    );
    assert!(!b(json!("0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5g")), "not hex");
    assert!(!b(json!(format!(" {BOARD}"))), "leading space");
    assert!(!b(json!(format!("{BOARD} "))), "trailing space");
    assert!(!b(json!(format!("{BOARD}\n"))), "trailing newline");
    assert!(!b(json!(format!("{{{BOARD}}}"))), "braces");
    // Not a string.
    assert!(!b(Value::Null));
    assert!(!b(json!(7)));
    assert!(!b(json!([BOARD])));
}

#[test]
fn the_uppercase_board_is_refused_not_normalized_by_a_read() {
    assert!(
        parse(&bytes_of(vec![sourced(
            plan_pending(ID_A),
            source(json!(BOARD.to_uppercase()), json!(233), json!(7))
        )]))
        .is_err()
    );
}

// ---- Wrong shapes ----

#[test]
fn a_member_missing_is_refused() {
    assert!(valid(with_member("ticket", Some(json!(233)))));
    for k in ["board", "ticket", "revision"] {
        assert!(!valid(with_member(k, None)), "{k}");
    }
}

#[test]
fn an_extra_member_is_refused() {
    assert!(!valid(with_member("token", Some(json!("x")))));
    assert!(!valid(with_member(
        "url",
        Some(json!("http://127.0.0.1:1/"))
    )));
    assert!(!valid(with_member("extra", Some(Value::Null))));
}

#[test]
fn a_member_set_to_null_is_refused() {
    for k in ["board", "ticket", "revision"] {
        assert!(!valid(with_member(k, Some(Value::Null))), "{k}");
    }
}

#[test]
fn plan_source_that_is_not_an_object_is_refused() {
    for v in [
        Value::Null,
        json!(BOARD),
        json!(233),
        json!([]),
        json!([BOARD, 233, 7]),
        json!(true),
        json!({}),
    ] {
        assert!(!valid(sourced(plan_pending(ID_A), v.clone())), "{v}");
    }
}

#[test]
fn a_duplicate_plan_source_member_is_refused() {
    let text = String::from_utf8(bytes_of(vec![base()])).unwrap();
    let dup = text.replace("\"revision\":7}", "\"revision\":7,\"revision\":8}");
    assert_ne!(text, dup);
    assert!(parse(dup.as_bytes()).is_err());
}

// ---- A document the schema refuses ----

#[test]
fn the_state_schema_refuses_a_bad_document_by_name() {
    let bad = file_of(vec![with_member("revision", Some(json!(0)))]);
    assert_eq!(validate_schema("leases.state", &bad), Some(false));
    assert!(parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    let good = file_of(vec![base()]);
    assert_eq!(validate_schema("leases.state", &good), Some(true));
}

#[test]
fn a_refused_file_says_only_that_it_is_not_valid() {
    let e = parse(&bytes_of(vec![with_member("ticket", Some(json!(0)))]))
        .map(|_| ())
        .expect_err("Must be refused");
    assert_eq!(e.0, "the state file is not valid");
}

#[test]
fn a_file_with_one_bad_record_among_good_ones_is_refused_whole() {
    let bad = with_member("board", Some(json!("NOPE")));
    assert!(parse(&bytes_of(vec![base(), bad])).is_err());
    assert!(parse(&bytes_of(vec![base(), plan_pending(ID_B)])).is_ok());
}

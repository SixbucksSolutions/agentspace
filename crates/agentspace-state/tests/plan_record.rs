//! The lease state file's plan members: `sized_for`, `plan` and the `plan_too_large` record
//! (docs/state.md, The lease state file and What a plan lease adds; the state schema
//! schemas/state/leases.state.schema.json; D74, D75, D81).
//!
//! Every refusal is checked twice, by `parse` and by the schema's own name, and each family starts
//! from a baseline that is valid, so a checker that refuses everything fails on the baseline.

mod common;
mod plan_support;

use agentspace_state::{Record, SizedFor, StateFile, parse, serialize, validate_schema};
use common::*;
use plan_support::*;
use serde_json::{Value, json};

const NOT_VALID: &str = "the state file is not valid";
const CHANGE_NOT_VALID: &str = "the change is not valid";

fn parsed(records: Vec<Value>) -> StateFile {
    parse(&bytes_of(records)).expect("Fixture must parse")
}

fn text_of(f: &StateFile) -> String {
    String::from_utf8(serialize(f).expect("Must serialize")).unwrap()
}

/// What a record's `sized_for` holds, as plain values.
fn sized_of(r: &Record) -> Option<(Option<String>, String, u32)> {
    r.sized_for
        .as_ref()
        .map(|s| (s.plan.clone(), s.turn.clone(), s.agents))
}

/// Whether a file of these records is valid, by `parse` and by the schema, which must agree.
fn valid(records: Vec<Value>) -> bool {
    let by_schema = validate_schema("leases.state", &file_of(records.clone()));
    let by_parse = parse(&bytes_of(records)).is_ok();
    assert_eq!(
        by_schema,
        Some(by_parse),
        "parse and the schema must agree on the file"
    );
    by_parse
}

fn valid_rec(rec: Value) -> bool {
    valid(vec![rec])
}

/// A record with one member of its `sized_for` changed, or removed when `val` is `None`.
fn in_sized_for(mut rec: Value, key: &str, val: Option<Value>) -> Value {
    let o = rec["sized_for"].as_object_mut().unwrap();
    match val {
        Some(v) => {
            o.insert(key.to_string(), v);
        }
        None => {
            o.shift_remove(key);
        }
    }
    rec
}

/// A record whose plan is `minimal_plan()` changed by `f`.
fn plan_edited(rec: Value, f: impl FnOnce(&mut Value)) -> Value {
    let mut p = minimal_plan();
    f(&mut p);
    with(rec, "plan", Some(p))
}

fn subagent(p: &mut Value) -> &mut serde_json::Map<String, Value> {
    p.pointer_mut("/turns/0/stories/0/subagents/0")
        .unwrap()
        .as_object_mut()
        .unwrap()
}

/// A file's text with `from` (which must occur once) replaced by `to`.
fn text_with(records: Vec<Value>, from: &str, to: &str) -> Vec<u8> {
    let text = String::from_utf8(bytes_of(records)).unwrap();
    assert_eq!(text.matches(from).count(), 1, "{from}");
    text.replace(from, to).into_bytes()
}

/// The positions of these markers in a text, which must each occur once and stand in this order.
fn assert_in_order(text: &str, markers: &[&str]) {
    let mut last = 0;
    for m in markers {
        assert_eq!(text.matches(m).count(), 1, "{m} in {text}");
        let at = text.find(m).unwrap();
        assert!(at >= last, "{m} is out of order in {text}");
        last = at;
    }
}

// ---- The valid samples ----

#[test]
fn the_valid_plan_samples_parse_with_their_members() {
    let f = parse(state_sample_text("leases.state.plan-leases.json").as_bytes()).unwrap();
    assert_eq!(f.leases.len(), 4);
    assert_eq!(
        sized_of(&f.leases[0]),
        Some((Some("v0.10".to_string()), "T01".to_string(), 4))
    );
    assert_eq!(f.leases[0].size, "s");
    // A plan with no name: sized_for has no plan either.
    assert_eq!(sized_of(&f.leases[1]), Some((None, "T03".to_string(), 4)));
    assert_eq!(sized_of(&f.leases[2]), Some((None, "T02".to_string(), 6)));
    // A lease made without a plan has neither member.
    assert!(sized_of(&f.leases[3]).is_none() && f.leases[3].plan.is_none());

    let doc = state_sample("leases.state.plan-leases.json");
    for i in 0..3 {
        assert_eq!(
            f.leases[i]
                .plan
                .as_ref()
                .expect("A plan lease keeps its plan"),
            &doc["leases"][i]["plan"],
            "lease {i}"
        );
    }
    // A null the plan held is kept, as parsed.
    let p = f.leases[2].plan.as_ref().unwrap();
    assert!(p.as_object().unwrap().contains_key("name") && p["name"].is_null());
    let a = &p["turns"][0]["stories"][0]["subagents"][0];
    assert!(a.as_object().unwrap().contains_key("after") && a["after"].is_null());
    assert_eq!(
        p["turns"][0]["stories"][0]["subagents"][2]["after"],
        json!([])
    );
}

#[test]
fn the_smallest_plan_sample_parses() {
    let f = parse(state_sample_text("leases.state.smallest-plan.json").as_bytes()).unwrap();
    assert_eq!(f.leases.len(), 1);
    assert_eq!(sized_of(&f.leases[0]), Some((None, "T01".to_string(), 2)));
    assert!(f.leases[0].plan.is_some());
}

#[test]
fn the_sample_without_a_plan_has_neither_member() {
    let f = parse(state_sample_text("leases.state.without-plan.json").as_bytes()).unwrap();
    assert_eq!(f.leases.len(), 5);
    for r in &f.leases {
        assert!(sized_of(r).is_none() && r.plan.is_none(), "{}", r.id);
    }
}

#[test]
fn the_plan_too_large_sample_is_an_unsatisfiable_xl_record() {
    let f = parse(state_sample_text("leases.state.plan-too-large.json").as_bytes()).unwrap();
    let r = &f.leases[0];
    assert_eq!(r.state, "unsatisfiable");
    assert_eq!(r.size, "xl");
    assert_eq!(
        sized_of(r),
        Some((Some("v0.11".to_string()), "T02".to_string(), 33))
    );
    assert!(r.plan.is_some());
    assert!(r.driver.is_some());
    assert_eq!(r.reasons.as_ref().expect("It has reasons").len(), 1);
    assert_eq!(r.reasons.as_ref().unwrap()[0].code, "plan_too_large");
}

#[test]
fn every_valid_state_sample_is_valid_by_the_schema_too() {
    let names = state_sample_names("");
    assert!(names.len() >= 4, "{names:?}");
    for n in names {
        let doc = state_sample(&n);
        assert_eq!(validate_schema("leases.state", &doc), Some(true), "{n}");
        assert!(parse(state_sample_text(&n).as_bytes()).is_ok(), "{n}");
    }
}

#[test]
fn every_invalid_state_sample_is_refused_by_parse_and_by_the_schema() {
    let names = state_sample_names("invalid");
    // The plan and sized_for samples the state schema ships, found by name so that an empty or
    // renamed folder cannot make this pass.
    for want in [
        "leases.state.plan-lease-without-driver.json",
        "leases.state.plan-null.json",
        "leases.state.plan-too-large-beside-another-reason.json",
        "leases.state.plan-too-large-size-m.json",
        "leases.state.plan-too-large-twice.json",
        "leases.state.plan-too-large-without-sized-for.json",
        "leases.state.plan-without-sized-for.json",
        "leases.state.sized-for-agents-1026.json",
        "leases.state.sized-for-agents-one.json",
        "leases.state.sized-for-plan-null.json",
        "leases.state.sized-for-without-plan.json",
        "leases.state.sized-for-without-turn.json",
        "leases.state.unknown-reason.json",
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

#[test]
fn a_refused_plan_file_says_only_that_it_is_not_valid() {
    // The plan's model and effort reach no output (D74).
    let text = state_sample_text("invalid/leases.state.plan-effort-with-space.json");
    let e = parse(text.as_bytes())
        .map(|_| ())
        .expect_err("Must be refused");
    assert_eq!(e.0, NOT_VALID);
}

// ---- Read, then written back ----

#[test]
fn a_valid_sample_is_written_back_as_its_own_compact_text() {
    for n in state_sample_names("") {
        let f = parse(state_sample_text(&n).as_bytes()).unwrap();
        let want = format!("{}\n", serde_json::to_string(&state_sample(&n)).unwrap());
        assert_eq!(text_of(&f), want, "{n}");
    }
}

#[test]
fn a_written_file_reads_and_writes_again_unchanged() {
    for n in state_sample_names("") {
        let f = parse(state_sample_text(&n).as_bytes()).unwrap();
        let once = serialize(&f).unwrap();
        let again = serialize(&parse(&once).unwrap()).unwrap();
        assert_eq!(once, again, "{n}");
    }
}

#[test]
fn a_record_in_each_state_keeps_both_members_through_a_round_trip() {
    let recs = vec![
        plan_pending(ID_A),
        plan_ready(ID_B),
        plan_released_from_ready(ID_C),
        plan_released_from_pending("a1b2c3d4-5555-4666-a777-888899990000"),
        plan_unsatisfiable("b1b2c3d4-5555-4666-a777-888899990000"),
        plan_too_large("c1b2c3d4-5555-4666-a777-888899990000"),
    ];
    let f = parsed(recs.clone());
    for (r, rec) in f.leases.iter().zip(&recs) {
        assert!(r.sized_for.is_some() && r.plan.is_some(), "{}", r.id);
        assert_eq!(r.plan.as_ref().unwrap(), &rec["plan"], "{}", r.id);
    }
    let back = parse(&serialize(&f).unwrap()).unwrap();
    for (r, rec) in back.leases.iter().zip(&recs) {
        assert_eq!(r.plan.as_ref().unwrap(), &rec["plan"], "{}", r.id);
        assert_eq!(
            sized_of(r).unwrap().1,
            rec["sized_for"]["turn"].as_str().unwrap()
        );
    }
}

#[test]
fn members_are_written_in_the_order_state_md_lists_them() {
    // create writes id, project, state, platform, profile, size, sized_for, ttl_seconds,
    // created_at, then expires_at and ssh or reasons, then driver, then plan.
    let id = |s: &str| format!("\"id\":\"{s}\"");
    let cases: Vec<(Value, Vec<&str>)> = vec![
        (
            plan_pending(ID_A),
            vec![
                "\"size\":\"s\"",
                "\"sized_for\":",
                "\"ttl_seconds\":",
                "\"created_at\":",
                "\"driver\":",
                ",\"plan\":{\"turns\"",
            ],
        ),
        (
            plan_ready(ID_A),
            vec![
                "\"size\":\"s\"",
                "\"sized_for\":",
                "\"ttl_seconds\":",
                "\"created_at\":",
                "\"expires_at\":",
                "\"ssh\":",
                "\"driver\":",
                ",\"plan\":{\"turns\"",
            ],
        ),
        (
            plan_released_from_ready(ID_A),
            vec![
                "\"sized_for\":",
                "\"created_at\":",
                "\"expires_at\":",
                "\"released_at\":",
                "\"driver\":",
                ",\"plan\":{\"turns\"",
            ],
        ),
        (
            plan_unsatisfiable(ID_A),
            vec![
                "\"sized_for\":",
                "\"created_at\":",
                "\"reasons\":",
                "\"driver\":",
                ",\"plan\":{\"turns\"",
            ],
        ),
        (
            plan_too_large(ID_A),
            vec![
                "\"size\":\"xl\"",
                "\"sized_for\":",
                "\"ttl_seconds\":",
                "\"created_at\":",
                "\"reasons\":",
                "\"driver\":",
                ",\"plan\":{\"name\":\"v0.10\"",
            ],
        ),
    ];
    for (rec, markers) in cases {
        let state = rec["state"].as_str().unwrap().to_string();
        let text = text_of(&parsed(vec![rec]));
        let mut all = vec![id(ID_A)];
        all.extend(markers.iter().map(|m| m.to_string()));
        let refs: Vec<&str> = all.iter().map(String::as_str).collect();
        assert_in_order(&text, &refs);
        assert!(text.contains(&state), "{state}");
    }
}

#[test]
fn a_ready_plan_record_is_written_byte_for_byte() {
    let f = parsed(vec![plan_ready(ID_A)]);
    let want = format!(
        concat!(
            "{{\"version\":0,\"leases\":[{{\"id\":\"{id}\",\"project\":\"demo\",\"state\":\"ready\",",
            "\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"s\",",
            "\"sized_for\":{{\"plan\":\"v0.10\",\"turn\":\"T01\",\"agents\":2}},",
            "\"ttl_seconds\":3600,\"created_at\":\"{c}\",\"expires_at\":\"{e}\",",
            "\"ssh\":{{\"host_alias\":\"agentspace-demo-3f2a9c1e\",\"user\":\"ubuntu\"}},",
            "\"driver\":\"ec2\",",
            "\"plan\":{{\"turns\":[{{\"name\":\"T01\",\"stories\":[{{\"name\":\"S01\",",
            "\"subagents\":[{{\"name\":\"a\",\"model\":\"Opus\",\"effort\":\"high\"}}]}}]}}]}}}}]}}\n"
        ),
        id = ID_A,
        c = CREATED,
        e = EXPIRES
    );
    assert_eq!(text_of(&f), want);
}

#[test]
fn members_may_stand_in_any_order_in_the_file_and_are_written_in_the_order_above() {
    // Reading is tolerant of member order; writing is not.
    let rec = plan_pending(ID_A);
    let mut m = serde_json::Map::new();
    m.insert("plan".to_string(), rec["plan"].clone());
    m.insert("driver".to_string(), rec["driver"].clone());
    m.insert("sized_for".to_string(), rec["sized_for"].clone());
    for (k, v) in rec.as_object().unwrap() {
        if !m.contains_key(k) {
            m.insert(k.clone(), v.clone());
        }
    }
    let shuffled = bytes_of(vec![Value::Object(m)]);
    let f = parse(&shuffled).expect("Member order is not a rule for a reader");
    assert_eq!(text_of(&f), text_of(&parsed(vec![rec])));
}

#[test]
fn a_record_without_a_plan_is_written_as_v0_wrote_it() {
    let f = parse(state_sample_text("leases.state.without-plan.json").as_bytes()).unwrap();
    let t = text_of(&f);
    assert!(!t.contains("sized_for"), "{t}");
    assert!(!t.contains("\"plan\""), "{t}");
    assert!(!t.contains("null"), "{t}");
    assert!(!t.contains("plan_too_large"), "{t}");
    // And it is the sample's own compact text.
    assert_eq!(
        t,
        format!(
            "{}\n",
            serde_json::to_string(&state_sample("leases.state.without-plan.json")).unwrap()
        )
    );
}

#[test]
fn sized_for_leaves_out_a_plan_name_that_is_absent_never_null() {
    let f = parsed(vec![plan_pending(ID_A)]);
    assert!(
        text_of(&f).contains("\"sized_for\":{\"plan\":\"v0.10\",\"turn\":\"T01\",\"agents\":2},")
    );

    let mut f = parsed(vec![plan_pending(ID_A)]);
    f.leases[0].sized_for = Some(SizedFor {
        plan: None,
        turn: "T01".to_string(),
        agents: 2,
    });
    let t = text_of(&f);
    assert!(
        t.contains("\"sized_for\":{\"turn\":\"T01\",\"agents\":2},"),
        "{t}"
    );
    assert!(!t.contains("\"plan\":null"), "{t}");
    assert!(!t.contains("null"), "{t}");
    // And reads back as absent.
    let back = parse(t.as_bytes()).unwrap();
    assert_eq!(
        sized_of(&back.leases[0]),
        Some((None, "T01".to_string(), 2))
    );
}

#[test]
fn a_plan_is_stored_compactly_in_its_files_member_order_with_its_nulls_and_re_escaped_strings() {
    // A plan written loosely, with whitespace, members in a non-canonical order, a `null` name,
    // a null `after`, and strings written with escapes.
    let loose_plan = concat!(
        "{ \"turns\" : [ { \"stories\" : [ { \"subagents\" : [ { \"model\" : \"Op\\u0075s\", ",
        "\"name\" : \"a\", \"effort\" : \"high\", \"after\" : null } ], \"name\" : \"S\\/01\" } ], ",
        "\"name\" : \"T01\" } ], \"name\" : null }"
    );
    let want_plan = concat!(
        "{\"turns\":[{\"stories\":[{\"subagents\":[{\"model\":\"Opus\",\"name\":\"a\",",
        "\"effort\":\"high\",\"after\":null}],\"name\":\"S/01\"}],\"name\":\"T01\"}],\"name\":null}"
    );
    let head = serde_json::to_string(&with(
        with(pending(ID_A, "demo"), "size", Some(json!("s"))),
        "sized_for",
        Some(sized_for(Some("v0.10"), "T01", 2)),
    ))
    .unwrap();
    let head = head.strip_suffix('}').unwrap();
    let file = format!(
        "\u{feff}\r\n{{ \"version\" : 0 , \"leases\" : [ {head} , \"plan\" : {loose_plan} }} ]\r\n}}\r\n"
    );
    let f = parse(file.as_bytes()).expect("The file is valid");
    let t = text_of(&f);
    assert!(t.ends_with(&format!(",\"plan\":{want_plan}}}]}}\n")), "{t}");
    // No whitespace is kept outside strings.
    assert!(!t.contains(' ') && !t.contains('\r') && !t.contains('\t') && !t.contains('\u{feff}'));
    // The parsed plan keeps its member order, as text.
    let plan = f.leases[0].plan.as_ref().unwrap();
    assert_eq!(serde_json::to_string(plan).unwrap(), want_plan);
}

// ---- Refused on read, by parse and by the schema ----

#[test]
fn the_baseline_plan_records_are_valid() {
    for rec in [
        plan_pending(ID_A),
        plan_ready(ID_A),
        plan_released_from_ready(ID_A),
        plan_released_from_pending(ID_A),
        plan_unsatisfiable(ID_A),
        plan_too_large(ID_A),
    ] {
        assert!(valid_rec(rec.clone()), "{rec}");
    }
}

#[test]
fn sized_for_and_plan_come_together_with_driver_in_every_state() {
    for rec in [
        plan_pending(ID_A),
        plan_ready(ID_A),
        plan_released_from_ready(ID_A),
        plan_released_from_pending(ID_A),
        plan_unsatisfiable(ID_A),
        plan_too_large(ID_A),
    ] {
        let state = rec["state"].as_str().unwrap().to_string();
        assert!(
            !valid_rec(with(rec.clone(), "sized_for", None)),
            "{state}: sized_for gone"
        );
        assert!(
            !valid_rec(with(rec.clone(), "plan", None)),
            "{state}: plan gone"
        );
        assert!(
            !valid_rec(with(rec.clone(), "driver", None)),
            "{state}: driver gone"
        );
        // Without both of the two, driver stays optional, as for a record that was never planned.
        let neither = with(with(rec.clone(), "sized_for", None), "plan", None);
        if state != "unsatisfiable" || rec["reasons"][0]["code"] != "plan_too_large" {
            assert!(valid_rec(with(neither, "driver", None)), "{state}: neither");
        }
    }
}

#[test]
fn null_is_never_accepted_for_either_member_or_for_the_plan_name() {
    let base = plan_pending(ID_A);
    assert!(valid_rec(base.clone()));
    assert!(!valid_rec(with(
        base.clone(),
        "sized_for",
        Some(Value::Null)
    )));
    assert!(!valid_rec(with(base.clone(), "plan", Some(Value::Null))));
    assert!(!valid_rec(in_sized_for(
        base.clone(),
        "plan",
        Some(Value::Null)
    )));
    assert!(!valid_rec(in_sized_for(
        base.clone(),
        "turn",
        Some(Value::Null)
    )));
    assert!(!valid_rec(in_sized_for(base, "agents", Some(Value::Null))));
}

#[test]
fn sized_for_has_exactly_turn_and_agents_and_an_optional_plan() {
    let base = plan_pending(ID_A);
    assert!(valid_rec(in_sized_for(base.clone(), "plan", None)));
    assert!(!valid_rec(in_sized_for(base.clone(), "turn", None)));
    assert!(!valid_rec(in_sized_for(base.clone(), "agents", None)));
    assert!(!valid_rec(in_sized_for(
        base.clone(),
        "extra",
        Some(json!(1))
    )));
    assert!(!valid_rec(in_sized_for(
        base.clone(),
        "model",
        Some(json!("Opus"))
    )));
    for not_object in [
        json!("T01"),
        json!([]),
        json!(["T01", 2]),
        json!(7),
        json!(true),
    ] {
        assert!(
            !valid_rec(with(base.clone(), "sized_for", Some(not_object.clone()))),
            "{not_object}"
        );
    }
    assert!(!valid_rec(with(base, "sized_for", Some(json!({})))));
}

#[test]
fn sized_for_agents_is_an_integer_from_2_to_1025() {
    let base = plan_pending(ID_A);
    for ok in [
        json!(2),
        json!(3),
        json!(32),
        json!(33),
        json!(1024),
        json!(1025),
    ] {
        assert!(
            valid_rec(in_sized_for(base.clone(), "agents", Some(ok.clone()))),
            "{ok}"
        );
    }
    for bad in [
        json!(0),
        json!(1),
        json!(-2),
        json!(1026),
        json!(100000),
        json!(4.5),
        json!("4"),
        json!(true),
        json!([4]),
    ] {
        assert!(
            !valid_rec(in_sized_for(base.clone(), "agents", Some(bad.clone()))),
            "{bad}"
        );
    }
}

#[test]
fn an_agents_count_written_with_a_zero_fraction_is_form_only() {
    // docs/state.md: a number written with a fraction of zero, for an integer member, is accepted
    // and written back as an integer.
    let doc = text_with(vec![plan_pending(ID_A)], "\"agents\":2", "\"agents\":2.0");
    let f = parse(&doc).expect("2.0 is form only");
    assert_eq!(sized_of(&f.leases[0]).unwrap().2, 2);
    let t = text_of(&f);
    assert!(t.contains("\"agents\":2}"), "{t}");
    assert!(!t.contains("2.0"), "{t}");
}

#[test]
fn a_name_in_sized_for_is_one_to_64_printable_ascii_characters_with_no_space_at_an_end() {
    let base = plan_pending(ID_A);
    let ok = [
        "T01".to_string(),
        "T 01".to_string(),
        "x".to_string(),
        "x".repeat(64),
        "!~".to_string(),
        "a b c".to_string(),
    ];
    let bad = [
        String::new(),
        " T01".to_string(),
        "T01 ".to_string(),
        " ".to_string(),
        "T01\n".to_string(),
        "\nT01".to_string(),
        "x".repeat(65),
        "T\u{fc}".to_string(),
        "T\t1".to_string(),
        "\u{3000}T01".to_string(),
        "T\u{7f}".to_string(),
    ];
    for key in ["plan", "turn"] {
        for n in &ok {
            assert!(
                valid_rec(in_sized_for(base.clone(), key, Some(json!(n)))),
                "{key} {n:?}"
            );
        }
        for n in &bad {
            assert!(
                !valid_rec(in_sized_for(base.clone(), key, Some(json!(n)))),
                "{key} {n:?}"
            );
        }
        // Not a string.
        for v in [json!(7), json!(true), json!(["T01"]), json!({})] {
            assert!(
                !valid_rec(in_sized_for(base.clone(), key, Some(v.clone()))),
                "{key} {v}"
            );
        }
    }
}

#[test]
fn plan_must_be_a_plan_document() {
    let base = plan_pending(ID_A);
    assert!(valid_rec(plan_edited(base.clone(), |_| {})));
    assert!(valid_rec(with(base.clone(), "plan", Some(rich_plan()))));
    for bad in [
        json!([]),
        json!("x"),
        json!(1),
        json!(true),
        json!({}),
        json!({"name": "v0.10"}),
        json!({"turns": []}),
        json!({"turns": null}),
        json!({"turns": {}}),
    ] {
        assert!(
            !valid_rec(with(base.clone(), "plan", Some(bad.clone()))),
            "{bad}"
        );
    }
}

#[test]
fn a_plan_that_breaks_the_plan_schema_is_refused_at_every_depth() {
    let base = plan_pending(ID_A);
    let cases: Vec<(&str, Value)> = vec![
        (
            "plan unknown member",
            plan_edited(base.clone(), |p| {
                p["extra"] = json!(1);
            }),
        ),
        (
            "turn unknown member",
            plan_edited(base.clone(), |p| {
                p["turns"][0]["extra"] = json!(1);
            }),
        ),
        (
            "turn without stories",
            plan_edited(base.clone(), |p| {
                p["turns"][0]
                    .as_object_mut()
                    .unwrap()
                    .shift_remove("stories");
            }),
        ),
        (
            "turn without name",
            plan_edited(base.clone(), |p| {
                p["turns"][0].as_object_mut().unwrap().shift_remove("name");
            }),
        ),
        (
            "empty stories",
            plan_edited(base.clone(), |p| {
                p["turns"][0]["stories"] = json!([]);
            }),
        ),
        (
            "story without subagents",
            plan_edited(base.clone(), |p| {
                p["turns"][0]["stories"][0]
                    .as_object_mut()
                    .unwrap()
                    .shift_remove("subagents");
            }),
        ),
        (
            "empty subagents",
            plan_edited(base.clone(), |p| {
                p["turns"][0]["stories"][0]["subagents"] = json!([]);
            }),
        ),
        (
            "subagent without model",
            plan_edited(base.clone(), |p| {
                subagent(p).shift_remove("model");
            }),
        ),
        (
            "subagent without effort",
            plan_edited(base.clone(), |p| {
                subagent(p).shift_remove("effort");
            }),
        ),
        (
            "subagent without name",
            plan_edited(base.clone(), |p| {
                subagent(p).shift_remove("name");
            }),
        ),
        (
            "subagent model null",
            plan_edited(base.clone(), |p| {
                subagent(p).insert("model".to_string(), Value::Null);
            }),
        ),
        (
            "subagent effort null",
            plan_edited(base.clone(), |p| {
                subagent(p).insert("effort".to_string(), Value::Null);
            }),
        ),
        (
            "subagent effort with a space",
            plan_edited(base.clone(), |p| {
                subagent(p).insert("effort".to_string(), json!("very high"));
            }),
        ),
        (
            "subagent model with a trailing space",
            plan_edited(base.clone(), |p| {
                subagent(p).insert("model".to_string(), json!("Opus "));
            }),
        ),
        (
            "subagent unknown member",
            plan_edited(base.clone(), |p| {
                subagent(p).insert("extra".to_string(), json!(1));
            }),
        ),
        (
            "subagent after not an array",
            plan_edited(base.clone(), |p| {
                subagent(p).insert("after".to_string(), json!("b"));
            }),
        ),
        (
            "plan name leading space",
            plan_edited(base.clone(), |p| {
                p["name"] = json!(" v0.10");
            }),
        ),
    ];
    for (what, rec) in cases {
        assert!(!valid_rec(rec), "{what}");
    }
}

#[test]
fn what_no_schema_can_check_in_a_plan_leaves_the_file_valid() {
    // docs/state.md: a cycle in `after`, an `after` naming no subagent of its story, a name given
    // twice. create checked them before it wrote the record; a reader does not check again.
    let a = |name: &str, after: Option<Value>| {
        let mut m = json!({"name": name, "model": "Opus", "effort": "high"});
        if let Some(v) = after {
            m["after"] = v;
        }
        m
    };
    let cases: Vec<(&str, Value)> = vec![
        (
            "a cycle",
            json!({"turns": [{"name": "T01", "stories": [{"name": "S01", "subagents": [
                a("a", Some(json!(["b"]))), a("b", Some(json!(["a"])))]}]}]}),
        ),
        (
            "after names itself",
            json!({"turns": [{"name": "T01", "stories": [{"name": "S01", "subagents": [
                a("a", Some(json!(["a"]))), a("b", None)]}]}]}),
        ),
        (
            "after names no subagent",
            json!({"turns": [{"name": "T01", "stories": [{"name": "S01", "subagents": [
                a("a", Some(json!(["ghost"]))), a("b", None)]}]}]}),
        ),
        (
            "a subagent named twice",
            json!({"turns": [{"name": "T01", "stories": [{"name": "S01", "subagents": [
                a("a", None), a("a", None)]}]}]}),
        ),
        (
            "a story named twice",
            json!({"turns": [{"name": "T01", "stories": [
                {"name": "S01", "subagents": [a("a", None)]},
                {"name": "S01", "subagents": [a("a", None)]}]}]}),
        ),
        (
            "a turn named twice",
            json!({"turns": [
                {"name": "T01", "stories": [{"name": "S01", "subagents": [a("a", None)]}]},
                {"name": "T01", "stories": [{"name": "S01", "subagents": [a("a", None)]}]}]}),
        ),
    ];
    for (what, plan) in cases {
        let rec = with(plan_pending(ID_A), "plan", Some(plan.clone()));
        assert!(valid_rec(rec.clone()), "{what}");
        let f = parsed(vec![rec]);
        assert_eq!(f.leases[0].plan.as_ref().unwrap(), &plan, "{what}");
        // And it is written back as it was read.
        let back = parse(&serialize(&f).unwrap()).unwrap();
        assert_eq!(back.leases[0].plan.as_ref().unwrap(), &plan, "{what}");
    }
}

#[test]
fn a_member_named_twice_in_sized_for_or_in_the_plan_is_not_valid() {
    let recs = || vec![plan_pending(ID_A)];
    // The same value twice, and a different one.
    for (from, to) in [
        ("\"turn\":\"T01\"", "\"turn\":\"T01\",\"turn\":\"T01\""),
        ("\"turn\":\"T01\"", "\"turn\":\"T01\",\"turn\":\"T02\""),
        ("\"agents\":2", "\"agents\":2,\"agents\":2"),
        ("\"agents\":2", "\"agents\":2,\"agents\":3"),
        (
            "\"plan\":\"v0.10\"",
            "\"plan\":\"v0.10\",\"plan\":\"v0.10\"",
        ),
        (
            "\"model\":\"Opus\"",
            "\"model\":\"Opus\",\"model\":\"Opus\"",
        ),
        (
            "\"effort\":\"high\"",
            "\"effort\":\"high\",\"effort\":\"low\"",
        ),
        ("\"turns\":[", "\"turns\":[],\"turns\":["),
    ] {
        let doc = text_with(recs(), from, to);
        assert_eq!(
            parse(&doc).map(|_| ()).expect_err("Must be refused").0,
            NOT_VALID,
            "{to}"
        );
    }
    // The record's own `plan` and `sized_for` members, twice.
    let plan = serde_json::to_string(&minimal_plan()).unwrap();
    let doc = text_with(
        recs(),
        &format!(",\"plan\":{plan}"),
        &format!(",\"plan\":{plan},\"plan\":{plan}"),
    );
    assert!(parse(&doc).is_err());
    let sf = "\"sized_for\":{\"plan\":\"v0.10\",\"turn\":\"T01\",\"agents\":2}";
    let doc = text_with(recs(), sf, &format!("{sf},{sf}"));
    assert!(parse(&doc).is_err());
}

// ---- The plan_too_large record ----

#[test]
fn a_plan_too_large_record_is_unsatisfiable_xl_with_its_one_reason() {
    assert!(valid_rec(plan_too_large(ID_A)));
    // Whatever its platform: a too-large plan with an unsupported platform lists the one reason.
    let w = with(
        plan_too_large(ID_A),
        "platform",
        Some(json!("windows/amd64")),
    );
    assert!(valid_rec(w));
    // Any plan lease that is not too large may be unsatisfiable for another reason and any size.
    for size in ["s", "m", "l", "xl"] {
        let rec = with(plan_unsatisfiable(ID_A), "size", Some(json!(size)));
        assert!(valid_rec(rec), "{size}");
    }
}

#[test]
fn plan_too_large_needs_size_xl() {
    for size in ["s", "m", "l"] {
        assert!(
            !valid_rec(with(plan_too_large(ID_A), "size", Some(json!(size)))),
            "{size}"
        );
    }
}

#[test]
fn plan_too_large_is_always_the_only_reason() {
    let other = |code: &str| json!({"code": code, "message": "Another reason."});
    let own = json!({"code": "plan_too_large", "message": TOO_LARGE_MESSAGE});
    for code in [
        "unsupported_platform",
        "unsupported_profile",
        "unsupported_size",
        "no_capacity",
    ] {
        // After it, and before it.
        for reasons in [json!([own, other(code)]), json!([other(code), own])] {
            assert!(
                !valid_rec(with(plan_too_large(ID_A), "reasons", Some(reasons.clone()))),
                "{code}: {reasons}"
            );
        }
    }
    // Twice.
    assert!(!valid_rec(with(
        plan_too_large(ID_A),
        "reasons",
        Some(json!([own, own]))
    )));
    // Alone is valid, and several other reasons together still are.
    assert!(valid_rec(with(
        plan_too_large(ID_A),
        "reasons",
        Some(json!([own]))
    )));
    let two = json!([other("unsupported_platform"), other("unsupported_profile")]);
    assert!(valid_rec(with(
        plan_unsatisfiable(ID_A),
        "reasons",
        Some(two)
    )));
}

#[test]
fn plan_too_large_is_refused_in_a_record_that_is_not_unsatisfiable() {
    let reasons = json!([{"code": "plan_too_large", "message": TOO_LARGE_MESSAGE}]);
    // A pending, ready or released record may not hold reasons at all; with plan_too_large too.
    for rec in [
        plan_pending(ID_A),
        plan_ready(ID_A),
        plan_released_from_ready(ID_A),
    ] {
        let state = rec["state"].as_str().unwrap().to_string();
        let rec = with(
            with(rec, "size", Some(json!("xl"))),
            "reasons",
            Some(reasons.clone()),
        );
        assert!(!valid_rec(rec), "{state}");
    }
}

#[test]
fn plan_too_large_needs_sized_for_plan_and_driver() {
    assert!(!valid_rec(with(plan_too_large(ID_A), "sized_for", None)));
    assert!(!valid_rec(with(plan_too_large(ID_A), "plan", None)));
    assert!(!valid_rec(with(plan_too_large(ID_A), "driver", None)));
    // Neither member, as a record of v0.09 would hold it: the reason alone makes it invalid.
    let bare = with(with(plan_too_large(ID_A), "sized_for", None), "plan", None);
    assert!(!valid_rec(bare.clone()));
    assert!(!valid_rec(with(bare, "driver", None)));
}

#[test]
fn a_reason_code_that_is_no_reason_is_refused() {
    let rec = with(
        plan_too_large(ID_A),
        "reasons",
        Some(json!([{"code": "plan_far_too_large", "message": "Too many."}])),
    );
    assert!(!valid_rec(rec));
}

// ---- Refused on write ----

fn written_refusal(f: &StateFile) {
    let e = serialize(f).expect_err("The write must be refused");
    assert_eq!(e.0, CHANGE_NOT_VALID);
}

fn plan_file() -> StateFile {
    parsed(vec![plan_pending(ID_A)])
}

#[test]
fn a_record_built_with_both_members_is_written_and_read_back() {
    // Start from a record that has neither, and give it both, as create would.
    let mut f = parsed(vec![with(pending(ID_A, "demo"), "size", Some(json!("s")))]);
    assert!(f.leases[0].sized_for.is_none() && f.leases[0].plan.is_none());
    f.leases[0].sized_for = Some(SizedFor {
        plan: Some("v0.10".to_string()),
        turn: "T01".to_string(),
        agents: 2,
    });
    f.leases[0].plan = Some(minimal_plan());
    let bytes = serialize(&f).expect("Must be written");
    assert_eq!(
        String::from_utf8(bytes.clone()).unwrap(),
        text_of(&plan_file())
    );
    let back = parse(&bytes).unwrap();
    assert_eq!(
        sized_of(&back.leases[0]),
        Some((Some("v0.10".to_string()), "T01".to_string(), 2))
    );
    assert_eq!(back.leases[0].plan.as_ref().unwrap(), &minimal_plan());
}

#[test]
fn the_baseline_for_the_written_refusals_is_written() {
    assert!(serialize(&plan_file()).is_ok());
    let mut f = plan_file();
    f.leases[0].sized_for.as_mut().unwrap().agents = 2;
    assert!(serialize(&f).is_ok());
    f.leases[0].sized_for.as_mut().unwrap().agents = 1025;
    assert!(serialize(&f).is_ok());
}

#[test]
fn one_member_without_the_other_or_without_driver_is_not_written() {
    let mut f = plan_file();
    f.leases[0].sized_for = None;
    written_refusal(&f);
    let mut f = plan_file();
    f.leases[0].plan = None;
    written_refusal(&f);
    let mut f = plan_file();
    f.leases[0].driver = None;
    written_refusal(&f);
}

#[test]
fn a_sized_for_that_would_not_validate_is_not_written() {
    for agents in [0, 1, 1026, u32::MAX] {
        let mut f = plan_file();
        f.leases[0].sized_for.as_mut().unwrap().agents = agents;
        written_refusal(&f);
    }
    for bad in ["", " x", "x ", "x\n", "T\u{fc}"] {
        let mut f = plan_file();
        f.leases[0].sized_for.as_mut().unwrap().turn = bad.to_string();
        written_refusal(&f);
        let mut f = plan_file();
        f.leases[0].sized_for.as_mut().unwrap().plan = Some(bad.to_string());
        written_refusal(&f);
    }
    let mut f = plan_file();
    f.leases[0].sized_for.as_mut().unwrap().turn = "x".repeat(65);
    written_refusal(&f);
}

#[test]
fn a_plan_that_would_not_validate_is_not_written() {
    for bad in [
        Value::Null,
        json!([]),
        json!("x"),
        json!(1),
        json!({}),
        json!({"turns": []}),
        json!({"turns": [], "extra": 1}),
    ] {
        let mut f = plan_file();
        f.leases[0].plan = Some(bad.clone());
        let e = serialize(&f)
            .err()
            .unwrap_or_else(|| panic!("{bad} was written"));
        assert_eq!(e.0, CHANGE_NOT_VALID, "{bad}");
    }
}

#[test]
fn a_plan_too_large_record_that_would_not_validate_is_not_written() {
    let too_large =
        || parse(state_sample_text("leases.state.plan-too-large.json").as_bytes()).unwrap();
    assert!(serialize(&too_large()).is_ok());

    let mut f = too_large();
    f.leases[0].size = "m".to_string();
    written_refusal(&f);

    let mut f = too_large();
    f.leases[0].sized_for = None;
    f.leases[0].plan = None;
    written_refusal(&f);

    let mut f = too_large();
    f.leases[0].driver = None;
    written_refusal(&f);

    for state in ["pending", "ready", "released"] {
        let mut f = too_large();
        f.leases[0].state = state.to_string();
        written_refusal(&f);
    }

    // Beside another reason, taken from a record that holds one on its own.
    let mut f = too_large();
    let mut more = parsed(vec![unsatisfiable(ID_B, "demo")]).leases.remove(0);
    let mut reasons = f.leases[0].reasons.take().unwrap();
    reasons.extend(more.reasons.take().unwrap());
    f.leases[0].reasons = Some(reasons);
    written_refusal(&f);

    // Before another reason.
    let mut f = too_large();
    let mut more = parsed(vec![unsatisfiable(ID_B, "demo")]).leases.remove(0);
    let mut reasons = more.reasons.take().unwrap();
    reasons.extend(f.leases[0].reasons.take().unwrap());
    f.leases[0].reasons = Some(reasons);
    written_refusal(&f);

    // Twice.
    let mut f = too_large();
    let mut again = too_large().leases.remove(0);
    f.leases[0]
        .reasons
        .as_mut()
        .unwrap()
        .extend(again.reasons.take().unwrap());
    written_refusal(&f);
}

// ---- The checkers are proven to refuse a record that is not valid, and to pass one that is ----

#[test]
fn the_checkers_pass_a_valid_record_and_fail_a_planted_invalid_one() {
    // `valid_rec` says yes to a good record and no to one with a planted fault, in the same run,
    // so a checker that always agreed could not pass here.
    assert!(valid_rec(plan_pending(ID_A)));
    assert!(!valid_rec(in_sized_for(
        plan_pending(ID_A),
        "agents",
        Some(json!(1))
    )));
    // `written_refusal` fails on a file that is written.
    let ok = plan_file();
    let caught = std::panic::catch_unwind(|| written_refusal(&plan_file()));
    assert!(caught.is_err(), "written_refusal must fail on a valid file");
    assert!(serialize(&ok).is_ok());
}

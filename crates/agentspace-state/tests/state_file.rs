//! The lease state file's rules (docs/state.md, The lease state file; lease-api.md, States and
//! list).

mod common;

use agentspace_state::{
    NotValid, OutputKind, Record, Ssh, StateFile, lease_document, list_document, parse, serialize,
    validate_output,
};
use common::*;
use serde_json::{Value, json};

fn parsed(records: Vec<Value>) -> StateFile {
    parse(&bytes_of(records)).expect("Fixture must parse")
}

fn is_valid(bytes: &[u8]) -> bool {
    parse(bytes).is_ok()
}

fn err(r: Result<StateFile, NotValid>) -> String {
    match r {
        Ok(_) => panic!("Expected the file not to be valid"),
        Err(e) => e.0,
    }
}

// ---- Reading: bytes ----

#[test]
fn a_plain_file_parses() {
    let f = parsed(vec![ready(ID_A, "demo"), pending(ID_B, "other")]);
    assert_eq!(f.leases.len(), 2);
    assert_eq!(f.leases[0].id, ID_A);
    assert_eq!(f.leases[1].id, ID_B);
}

#[test]
fn an_empty_file_of_no_leases_parses() {
    let f = parse(br#"{"version":0,"leases":[]}"#).unwrap();
    assert!(f.leases.is_empty());
}

#[test]
fn a_leading_byte_order_mark_is_accepted() {
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend(bytes_of(vec![ready(ID_A, "demo")]));
    assert_eq!(parse(&b).unwrap().leases.len(), 1);
}

#[test]
fn only_one_byte_order_mark_is_accepted() {
    let mut b = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    b.extend(bytes_of(vec![]));
    assert!(!is_valid(&b));
}

#[test]
fn a_byte_order_mark_inside_the_file_is_not_valid() {
    assert!(!is_valid(
        "{\"version\":0,\u{feff}\"leases\":[]}".as_bytes()
    ));
}

#[test]
fn any_json_whitespace_is_accepted() {
    let a = ID_A;
    let pretty = " \t\r\n{ \n\t\"version\" :\r\n 0 ,\n \"leases\":\t[ \n]\r\n} \n\n\r\n \t";
    assert!(is_valid(pretty.as_bytes()));
    let rec = serde_json::to_string_pretty(&ready(a, "demo")).unwrap();
    let doc =
        format!("\u{feff}{{\r\n  \"version\": 0,\r\n  \"leases\": [\r\n{rec}\r\n  ]\r\n}}\r\n");
    assert_eq!(parse(doc.as_bytes()).unwrap().leases.len(), 1);
}

#[test]
fn whitespace_json_does_not_allow_is_refused() {
    // Form feed, vertical tab and no-break space are not JSON whitespace.
    for ws in ["\u{0c}", "\u{0b}", "\u{a0}", "\u{3000}"] {
        let doc = format!("{ws}{{\"version\":0,\"leases\":[]}}");
        assert!(!is_valid(doc.as_bytes()), "{ws:?}");
    }
}

#[test]
fn bytes_that_are_not_utf8_are_not_valid() {
    let mut b = br#"{"version":0,"leases":[],"#.to_vec();
    b.extend([0xFF, 0xFE]);
    assert!(!is_valid(&b));
    // A well-formed document holding an invalid byte inside a string.
    let mut rec = br#"{"version":0,"leases":[{"id":""#.to_vec();
    rec.push(0xC3); // A lone lead byte.
    rec.extend(br#""}]}"#);
    assert!(!is_valid(&rec));
    // A UTF-16 file.
    let utf16: Vec<u8> = "{\"version\":0,\"leases\":[]}"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    assert!(!is_valid(&utf16));
}

#[test]
fn empty_and_junk_input_is_not_valid() {
    assert!(!is_valid(b""));
    assert!(!is_valid(b"\xEF\xBB\xBF"));
    assert!(!is_valid(b"not json"));
    assert!(!is_valid(b"[]"));
    assert!(!is_valid(b"null"));
    assert!(!is_valid(br#"{"version":0,"leases":[]} trailing"#));
}

#[test]
fn the_why_of_a_file_that_does_not_validate() {
    // lease-api.md, Messages: "Records unreadable".
    assert_eq!(
        err(parse(br#"{"version":1,"leases":[]}"#)),
        "the state file is not valid"
    );
}

// ---- Reading: version, schema, ids, dates ----

#[test]
fn version_other_than_zero_is_not_valid() {
    for v in ["1", "-1", "2", "\"0\"", "null", "0.5", "true"] {
        let doc = format!("{{\"version\":{v},\"leases\":[]}}");
        assert!(!is_valid(doc.as_bytes()), "{v}");
    }
    assert!(!is_valid(br#"{"leases":[]}"#));
    assert!(!is_valid(br#"{"version":0}"#));
}

#[test]
fn a_schema_failure_is_not_valid() {
    let bad = [
        with(ready(ID_A, "demo"), "project", Some(json!("Demo"))),
        with(ready(ID_A, "demo"), "id", Some(json!(format!("{ID_A}\n")))),
        with(ready(ID_A, "demo"), "id", Some(json!(ID_A.to_uppercase()))),
        with(ready(ID_A, "demo"), "ssh", None),
        with(ready(ID_A, "demo"), "expires_at", None),
        with(ready(ID_A, "demo"), "extra", Some(json!(1))),
        with(ready(ID_A, "demo"), "ttl_seconds", Some(json!(59))),
        with(ready(ID_A, "demo"), "ttl_seconds", Some(json!(86401))),
        with(ready(ID_A, "demo"), "state", Some(json!("expired"))),
        with(ready(ID_A, "demo"), "released_at", Some(json!(CREATED))),
        with(pending(ID_A, "demo"), "expires_at", Some(json!(EXPIRES))),
        with(unsatisfiable(ID_A, "demo"), "reasons", None),
        with(
            ready(ID_A, "demo"),
            "created_at",
            Some(json!("2026-11-19T10:00:00+00:00")),
        ),
        with(ready(ID_A, "demo"), "driver", Some(json!(""))),
        with(ready(ID_A, "demo"), "driver", Some(json!("ec\n2"))),
        with(
            ready(ID_A, "demo"),
            "ssh",
            Some(json!({"host_alias": "agentspace-demo-3f2a9c1e", "user": "Ubuntu"})),
        ),
        // One reason code twice.
        with(
            unsatisfiable(ID_A, "demo"),
            "reasons",
            Some(json!([
                {"code": "unsupported_platform",
                 "message": "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only."},
                {"code": "unsupported_platform",
                 "message": "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only."}
            ])),
        ),
        // A reason whose message starts with a space.
        with(
            unsatisfiable(ID_A, "demo"),
            "reasons",
            Some(json!([
                {"code": "unsupported_platform",
                 "message": " Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only."}
            ])),
        ),
    ];
    for rec in bad {
        assert!(!is_valid(&bytes_of(vec![rec.clone()])), "{rec}");
    }
}

#[test]
fn a_repeated_id_is_not_valid() {
    let doc = bytes_of(vec![ready(ID_A, "demo"), pending(ID_A, "other")]);
    assert_eq!(err(parse(&doc)), "the state file is not valid");
    let doc = bytes_of(vec![
        ready(ID_A, "demo"),
        pending(ID_B, "demo"),
        pending(ID_A, "demo"),
    ]);
    assert!(!is_valid(&doc));
}

#[test]
fn distinct_ids_with_the_same_start_are_valid() {
    let id2 = "3f2a9c1e-1111-4222-8333-444455556667";
    assert!(is_valid(&bytes_of(vec![
        ready(ID_A, "demo"),
        pending(id2, "demo")
    ])));
}

/// A file's text with `from` (which must occur once) replaced by `to`.
fn text_with(records: Vec<Value>, from: &str, to: &str) -> Vec<u8> {
    let text = String::from_utf8(bytes_of(records)).unwrap();
    assert_eq!(text.matches(from).count(), 1, "{from}");
    text.replace(from, to).into_bytes()
}

#[test]
fn a_member_named_twice_in_a_record_is_not_valid() {
    // The same value twice, a different one, and either order: whichever a parser would keep.
    let one = format!("\"id\":\"{ID_A}\"");
    for to in [
        format!("{one},{one}"),
        format!("{one},\"id\":\"{ID_B}\""),
        format!("\"id\":\"{ID_B}\",{one}"),
    ] {
        let doc = text_with(vec![ready(ID_A, "demo")], &one, &to);
        assert_eq!(err(parse(&doc)), "the state file is not valid", "{to}");
    }
    let doc = text_with(
        vec![ready(ID_A, "demo")],
        "\"project\":\"demo\"",
        "\"project\":\"demo\",\"project\":\"demo\"",
    );
    assert!(!is_valid(&doc));
}

#[test]
fn a_member_named_twice_at_the_top_level_is_not_valid() {
    let doc = text_with(
        vec![ready(ID_A, "demo")],
        "\"version\":0",
        "\"version\":0,\"version\":0",
    );
    assert_eq!(err(parse(&doc)), "the state file is not valid");
    let doc = text_with(
        vec![ready(ID_A, "demo")],
        "\"version\":0",
        "\"version\":1,\"version\":0",
    );
    assert!(!is_valid(&doc));
}

#[test]
fn a_member_named_twice_in_a_nested_object_is_not_valid() {
    let doc = text_with(
        vec![ready(ID_A, "demo")],
        "\"user\":\"ubuntu\"",
        "\"user\":\"ubuntu\",\"user\":\"root\"",
    );
    assert_eq!(err(parse(&doc)), "the state file is not valid");
    let doc = text_with(
        vec![unsatisfiable(ID_A, "demo")],
        "\"code\":\"unsupported_platform\"",
        "\"code\":\"unsupported_platform\",\"code\":\"other\"",
    );
    assert_eq!(err(parse(&doc)), "the state file is not valid");
}

#[test]
fn a_member_named_twice_in_any_record_is_not_valid() {
    // The second record of three, not the first.
    let two = format!("\"id\":\"{ID_B}\"");
    let doc = text_with(
        vec![
            ready(ID_A, "demo"),
            pending(ID_B, "demo"),
            pending(ID_C, "demo"),
        ],
        &two,
        &format!("{two},{two}"),
    );
    assert!(!is_valid(&doc));
}

#[test]
fn a_member_name_written_with_an_escape_still_repeats() {
    let one = format!("\"id\":\"{ID_A}\"");
    let doc = text_with(
        vec![ready(ID_A, "demo")],
        &one,
        &format!("{one},\"\\u0069d\":\"{ID_A}\""),
    );
    assert!(!is_valid(&doc));
}

#[test]
fn a_member_named_in_two_records_is_valid() {
    // The same names in different objects are not a repeat.
    let doc = bytes_of(vec![
        ready(ID_A, "demo"),
        ready(ID_B, "demo"),
        unsatisfiable(ID_C, "demo"),
    ]);
    assert_eq!(parse(&doc).unwrap().leases.len(), 3);
    let two_reasons = with(
        unsatisfiable(ID_A, "demo"),
        "reasons",
        Some(json!([
            {"code": "unsupported_platform", "message": "One."},
            {"code": "no_capacity", "message": "Two."}
        ])),
    );
    assert!(is_valid(&bytes_of(vec![two_reasons])));
}

#[test]
fn very_deep_nesting_is_not_valid_and_does_not_overflow() {
    let deep = format!("{}{}", "[".repeat(200_000), "]".repeat(200_000));
    assert_eq!(err(parse(deep.as_bytes())), "the state file is not valid");
    let deep = format!(
        "{{\"version\":0,\"leases\":[],\"x\":{}{}}}",
        "{\"a\":".repeat(100_000),
        "}".repeat(100_000)
    );
    assert!(!is_valid(deep.as_bytes()));
}

#[test]
fn an_integer_written_with_a_zero_fraction_is_accepted_and_written_as_an_integer() {
    let doc = text_with(
        vec![ready(ID_A, "demo")],
        "\"ttl_seconds\":3600",
        "\"ttl_seconds\":3600.0",
    );
    let f = parse(&doc).expect("3600.0 is form only");
    assert_eq!(f.leases[0].ttl_seconds, 3600);
    let out = String::from_utf8(serialize(&f).unwrap()).unwrap();
    assert!(out.contains("\"ttl_seconds\":3600,"), "{out}");
    assert!(!out.contains("3600.0"), "{out}");
}

#[test]
fn an_impossible_date_is_not_valid() {
    // Gap FA-10: matches the pattern, names no real instant.
    for t in [
        "2026-02-30T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2100-02-29T00:00:00Z",
        "2026-11-19T24:00:00Z",
        "2026-11-19T10:00:60Z",
    ] {
        assert!(
            !is_valid(&bytes_of(vec![with(
                ready(ID_A, "demo"),
                "created_at",
                Some(json!(t))
            )])),
            "created_at {t}"
        );
        assert!(
            !is_valid(&bytes_of(vec![with(
                ready(ID_A, "demo"),
                "expires_at",
                Some(json!(t))
            )])),
            "expires_at {t}"
        );
        assert!(
            !is_valid(&bytes_of(vec![with(
                released_from_ready(ID_A, "demo"),
                "released_at",
                Some(json!(t))
            )])),
            "released_at {t}"
        );
    }
}

#[test]
fn a_real_leap_day_is_valid() {
    let rec = with(
        ready(ID_A, "demo"),
        "created_at",
        Some(json!("2024-02-29T10:00:00Z")),
    );
    assert!(is_valid(&bytes_of(vec![rec])));
}

#[test]
fn times_stay_the_strings_they_were() {
    let f = parsed(vec![ready(ID_A, "demo"), released_from_ready(ID_B, "demo")]);
    assert_eq!(f.leases[0].created_at, CREATED);
    assert_eq!(f.leases[0].expires_at.as_deref(), Some(EXPIRES));
    assert_eq!(
        f.leases[1].released_at.as_deref(),
        Some("2026-11-19T10:30:00Z")
    );
}

#[test]
fn a_ready_record_without_driver_is_valid() {
    let rec = with(ready(ID_A, "demo"), "driver", None);
    let f = parse(&bytes_of(vec![
        rec,
        with(released_from_ready(ID_B, "demo"), "driver", None),
    ]))
    .unwrap();
    assert!(f.leases[0].driver.is_none());
    assert!(f.leases[1].driver.is_none());
    // And its lease document is a normal one.
    let doc = lease_document(&f.leases[0], EXPIRES_UNIX - 1);
    assert_eq!(doc["state"], "ready");
    assert!(validate_output(OutputKind::Lease, &doc));
}

#[test]
fn pending_and_unsatisfiable_without_driver_are_valid() {
    let a = with(pending(ID_A, "demo"), "driver", None);
    let b = with(unsatisfiable(ID_B, "demo"), "driver", None);
    assert!(is_valid(&bytes_of(vec![a, b])));
}

#[test]
fn driver_is_kept_when_present() {
    let f = parsed(vec![ready(ID_A, "demo")]);
    assert_eq!(f.leases[0].driver.as_deref(), Some("ec2"));
}

// ---- The lease document ----

#[test]
fn ready_is_expired_at_the_exact_second() {
    let f = parsed(vec![ready(ID_A, "demo")]);
    let r = &f.leases[0];

    let before = lease_document(r, EXPIRES_UNIX - 1);
    assert_eq!(before["state"], "ready");
    assert!(before.get("ssh").is_some());
    assert_eq!(before["expires_at"], EXPIRES);

    let at = lease_document(r, EXPIRES_UNIX);
    assert_eq!(at["state"], "expired");
    assert!(at.get("ssh").is_none());
    assert_eq!(at["expires_at"], EXPIRES);

    let after = lease_document(r, EXPIRES_UNIX + 1);
    assert_eq!(after["state"], "expired");
    assert!(after.get("ssh").is_none());

    for d in [&before, &at, &after] {
        assert!(validate_output(OutputKind::Lease, d), "{d}");
    }
}

#[test]
fn expiry_is_decided_by_whole_seconds_not_by_distance() {
    let f = parsed(vec![ready(ID_A, "demo")]);
    let r = &f.leases[0];
    assert_eq!(lease_document(r, 0)["state"], "ready");
    assert_eq!(lease_document(r, i64::MAX)["state"], "expired");
}

#[test]
fn the_ready_document_is_exactly_the_contracts() {
    let f = parsed(vec![ready(ID_A, "demo")]);
    let d = lease_document(&f.leases[0], EXPIRES_UNIX - 100);
    let want = json!({
        "id": ID_A, "project": "demo", "state": "ready", "platform": "linux/arm64",
        "profile": "agent-linux", "size": "m", "ttl_seconds": 3600,
        "created_at": CREATED, "expires_at": EXPIRES,
        "ssh": {"host_alias": "agentspace-demo-3f2a9c1e", "user": "ubuntu"}
    });
    assert_eq!(d, want);
}

#[test]
fn driver_is_never_shown() {
    let f = parsed(vec![
        ready(ID_A, "demo"),
        pending(ID_B, "demo"),
        released_from_ready(ID_C, "demo"),
        unsatisfiable("a1b2c3d4-5555-4666-a777-888899990000", "demo"),
    ]);
    for now in [EXPIRES_UNIX - 1, EXPIRES_UNIX, EXPIRES_UNIX + 1] {
        for r in &f.leases {
            let d = lease_document(r, now);
            assert!(d.get("driver").is_none(), "{d}");
            assert!(!d.to_string().contains("ec2"), "{d}");
            assert!(validate_output(OutputKind::Lease, &d), "{d}");
        }
        let l = list_document(&f, None, &[], now);
        assert!(!l.to_string().contains("ec2"));
        assert!(!l.to_string().contains("driver"));
    }
}

#[test]
fn only_a_stored_ready_record_expires() {
    let f = parsed(vec![
        pending(ID_A, "demo"),
        released_from_ready(ID_B, "demo"),
        unsatisfiable(ID_C, "demo"),
    ]);
    let late = EXPIRES_UNIX + 100_000;
    assert_eq!(lease_document(&f.leases[0], late)["state"], "pending");
    let released = lease_document(&f.leases[1], late);
    assert_eq!(released["state"], "released");
    assert_eq!(released["expires_at"], EXPIRES);
    assert_eq!(released["released_at"], "2026-11-19T10:30:00Z");
    assert_eq!(lease_document(&f.leases[2], late)["state"], "unsatisfiable");
    assert!(lease_document(&f.leases[2], late).get("reasons").is_some());
}

#[test]
fn every_state_document_validates() {
    let f = parsed(vec![
        pending(ID_A, "demo"),
        ready(ID_B, "demo"),
        released_from_ready(ID_C, "demo"),
        with(
            released_from_ready("a1b2c3d4-5555-4666-a777-888899990000", "demo"),
            "expires_at",
            None,
        ),
        unsatisfiable("b1b2c3d4-5555-4666-a777-888899990000", "demo"),
    ]);
    for r in &f.leases {
        assert!(validate_output(
            OutputKind::Lease,
            &lease_document(r, EXPIRES_UNIX - 1)
        ));
    }
}

// ---- The list document ----

fn list_fixture() -> StateFile {
    // Created out of order on purpose; the ids are chosen so that character order matters:
    // a digit sorts before a letter, and a capital would sort before both (none is allowed).
    let at = |rec: Value, t: &str| with(rec, "created_at", Some(json!(t)));
    parsed(vec![
        at(
            pending("a1b2c3d4-5555-4666-a777-888899990000", "demo"),
            "2026-11-19T10:00:05Z",
        ),
        at(ready(ID_B, "demo"), "2026-11-19T10:00:05Z"),
        at(released_from_ready(ID_A, "other"), "2026-11-19T10:00:05Z"),
        at(unsatisfiable(ID_C, "other"), "2026-11-19T09:59:59Z"),
        at(
            pending("ffffffff-0000-4000-8000-000000000000", "demo"),
            "2026-11-19T10:00:06Z",
        ),
        at(
            pending("00000000-0000-4000-8000-000000000001", "demo"),
            "2026-11-18T23:00:00Z",
        ),
    ])
}

fn ids(doc: &Value) -> Vec<String> {
    doc["leases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn list_is_wrapped_in_leases_and_validates() {
    let f = list_fixture();
    let d = list_document(&f, None, &[], EXPIRES_UNIX - 1);
    assert!(d.as_object().unwrap().len() == 1);
    assert!(validate_output(OutputKind::LeaseList, &d), "{d}");
    let empty = list_document(&StateFile { leases: vec![] }, None, &[], 0);
    assert_eq!(empty, json!({"leases": []}));
    assert!(validate_output(OutputKind::LeaseList, &empty));
}

#[test]
fn list_is_ordered_by_created_at_then_id_character_by_character() {
    let f = list_fixture();
    let got = ids(&list_document(&f, None, &[], EXPIRES_UNIX - 1));
    assert_eq!(
        got,
        vec![
            "00000000-0000-4000-8000-000000000001", // 2026-11-18
            ID_C,                                   // 09:59:59
            ID_A,                                   // 10:00:05, id 3f..
            ID_B,                                   // 10:00:05, id 9b..
            "a1b2c3d4-5555-4666-a777-888899990000", // 10:00:05, id a1.. (a after 9)
            "ffffffff-0000-4000-8000-000000000000", // 10:00:06
        ]
    );
}

#[test]
fn list_order_does_not_depend_on_file_order() {
    let mut f = list_fixture();
    let a = ids(&list_document(&f, None, &[], 0));
    f.leases.reverse();
    let b = ids(&list_document(&f, None, &[], 0));
    assert_eq!(a, b);
}

#[test]
fn list_filters_by_project() {
    let f = list_fixture();
    let demo = list_document(&f, Some("demo"), &[], 0);
    assert_eq!(demo["leases"].as_array().unwrap().len(), 4);
    assert!(
        demo["leases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l["project"] == "demo")
    );
    let other = list_document(&f, Some("other"), &[], 0);
    assert_eq!(ids(&other), vec![ID_C, ID_A]);
    let none = list_document(&f, Some("nobody"), &[], 0);
    assert_eq!(none, json!({"leases": []}));
    // An exact match: a prefix is not a match.
    assert_eq!(
        list_document(&f, Some("dem"), &[], 0),
        json!({"leases": []})
    );
}

#[test]
fn list_filters_by_states() {
    let f = list_fixture();
    let now = EXPIRES_UNIX - 1;
    let pend = list_document(&f, None, &["pending"], now);
    assert_eq!(pend["leases"].as_array().unwrap().len(), 3);
    let two = list_document(&f, None, &["released", "unsatisfiable"], now);
    assert_eq!(ids(&two), vec![ID_C, ID_A]);
    assert_eq!(ids(&list_document(&f, None, &["ready"], now)), vec![ID_B]);
}

#[test]
fn list_state_filter_uses_the_derived_state() {
    let f = list_fixture();
    // One instant for the whole list: ready before, expired from the second on.
    assert_eq!(
        ids(&list_document(&f, None, &["ready"], EXPIRES_UNIX - 1)),
        vec![ID_B]
    );
    assert!(ids(&list_document(&f, None, &["expired"], EXPIRES_UNIX - 1)).is_empty());
    assert!(ids(&list_document(&f, None, &["ready"], EXPIRES_UNIX)).is_empty());
    assert_eq!(
        ids(&list_document(&f, None, &["expired"], EXPIRES_UNIX)),
        vec![ID_B]
    );
    let doc = list_document(&f, None, &["expired"], EXPIRES_UNIX);
    assert!(doc["leases"][0].get("ssh").is_none());
    assert!(validate_output(OutputKind::LeaseList, &doc));
}

#[test]
fn list_filters_combine() {
    let f = list_fixture();
    let d = list_document(&f, Some("demo"), &["pending", "ready"], 0);
    assert_eq!(d["leases"].as_array().unwrap().len(), 4);
    let d = list_document(&f, Some("other"), &["pending"], 0);
    assert_eq!(d, json!({"leases": []}));
    let d = list_document(&f, Some("other"), &["unsatisfiable"], 0);
    assert_eq!(ids(&d), vec![ID_C]);
}

#[test]
fn list_items_are_lease_documents() {
    let f = list_fixture();
    let now = EXPIRES_UNIX;
    let d = list_document(&f, None, &[], now);
    let by_id = |id: &str| f.leases.iter().find(|r| r.id == id).unwrap();
    for item in d["leases"].as_array().unwrap() {
        let id = item["id"].as_str().unwrap();
        assert_eq!(*item, lease_document(by_id(id), now));
    }
}

// ---- Writing ----

#[test]
fn serialize_writes_compact_bytes_in_states_order_with_one_lf() {
    let f = parsed(vec![ready(ID_A, "demo")]);
    let got = serialize(&f).unwrap();
    let want = format!(
        concat!(
            "{{\"version\":0,\"leases\":[{{\"id\":\"{id}\",\"project\":\"demo\",\"state\":\"ready\",",
            "\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",",
            "\"ttl_seconds\":3600,\"created_at\":\"{c}\",\"expires_at\":\"{e}\",",
            "\"ssh\":{{\"host_alias\":\"agentspace-demo-3f2a9c1e\",\"user\":\"ubuntu\"}},",
            "\"driver\":\"ec2\"}}]}}\n"
        ),
        id = ID_A,
        c = CREATED,
        e = EXPIRES
    );
    assert_eq!(String::from_utf8(got).unwrap(), want);
}

#[test]
fn serialize_orders_every_state_s_members() {
    let f = parsed(vec![pending(ID_A, "demo"), unsatisfiable(ID_B, "demo")]);
    let got = String::from_utf8(serialize(&f).unwrap()).unwrap();
    let want = format!(
        concat!(
            "{{\"version\":0,\"leases\":[",
            "{{\"id\":\"{a}\",\"project\":\"demo\",\"state\":\"pending\",\"platform\":\"linux/arm64\",",
            "\"profile\":\"agent-linux\",\"size\":\"m\",\"ttl_seconds\":3600,\"created_at\":\"{c}\",",
            "\"driver\":\"ec2\"}},",
            "{{\"id\":\"{b}\",\"project\":\"demo\",\"state\":\"unsatisfiable\",\"platform\":\"windows/amd64\",",
            "\"profile\":\"agent-linux\",\"size\":\"m\",\"ttl_seconds\":3600,\"created_at\":\"{c}\",",
            "\"reasons\":[{{\"code\":\"unsupported_platform\",\"message\":\"Platform 'windows/amd64' is not ",
            "supported; v0 supports linux/arm64 only.\"}}],\"driver\":\"ec2\"}}",
            "]}}\n"
        ),
        a = ID_A,
        b = ID_B,
        c = CREATED
    );
    assert_eq!(got, want);
}

#[test]
fn serialize_released_keeps_expires_before_released_at() {
    let f = parsed(vec![released_from_ready(ID_A, "demo")]);
    let got = String::from_utf8(serialize(&f).unwrap()).unwrap();
    let e = got.find("\"expires_at\"").unwrap();
    let r = got.find("\"released_at\"").unwrap();
    let d = got.find("\"driver\"").unwrap();
    assert!(e < r && r < d, "{got}");
}

#[test]
fn serialize_has_no_bom_no_whitespace_and_one_final_lf() {
    let f = parsed(vec![ready(ID_A, "demo"), pending(ID_B, "x")]);
    let b = serialize(&f).unwrap();
    assert!(!b.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert_eq!(*b.last().unwrap(), b'\n');
    let body = &b[..b.len() - 1];
    assert!(!body.contains(&b'\n') && !body.contains(&b'\r') && !body.contains(&b'\t'));
    assert!(!body.windows(2).any(|w| w == b", " || w == b": "));
    assert!(std::str::from_utf8(&b).is_ok());
}

#[test]
fn serialize_of_an_empty_file() {
    let b = serialize(&StateFile { leases: vec![] }).unwrap();
    assert_eq!(b, b"{\"version\":0,\"leases\":[]}\n");
}

#[test]
fn serialize_omits_a_missing_driver() {
    let f = parsed(vec![with(ready(ID_A, "demo"), "driver", None)]);
    let s = String::from_utf8(serialize(&f).unwrap()).unwrap();
    assert!(!s.contains("driver"));
    assert!(!s.contains("null"));
}

#[test]
fn serialize_keeps_the_time_strings() {
    let f = parsed(vec![ready(ID_A, "demo")]);
    let s = String::from_utf8(serialize(&f).unwrap()).unwrap();
    assert!(s.contains(CREATED) && s.contains(EXPIRES));
}

#[test]
fn parse_after_serialize_round_trips() {
    let f = parsed(vec![
        ready(ID_A, "demo"),
        pending(ID_B, "x"),
        released_from_ready(ID_C, "x"),
        unsatisfiable("a1b2c3d4-5555-4666-a777-888899990000", "y"),
        with(
            ready("b1b2c3d4-5555-4666-a777-888899990000", "z"),
            "driver",
            None,
        ),
    ]);
    let bytes = serialize(&f).unwrap();
    let again = parse(&bytes).unwrap();
    assert_eq!(serialize(&again).unwrap(), bytes);
}

#[test]
fn parse_then_serialize_normalizes_a_loose_file() {
    // BOM, whitespace and reordered members in; compact, ordered, no BOM out.
    let rec = serde_json::to_string_pretty(&ready(ID_A, "demo")).unwrap();
    let loose = format!("\u{feff}\r\n{{\"leases\":[{rec}],\"version\":0}}\r\n\r\n");
    let f = parse(loose.as_bytes()).unwrap();
    let out = serialize(&f).unwrap();
    assert_eq!(out, serialize(&parsed(vec![ready(ID_A, "demo")])).unwrap());
    assert!(out.starts_with(b"{\"version\":0,\"leases\":["));
}

#[test]
fn serialize_refuses_a_repeated_id() {
    let mut f = parsed(vec![ready(ID_A, "demo"), pending(ID_B, "demo")]);
    f.leases[1].id = ID_A.to_string();
    assert_eq!(serialize(&f).err().unwrap().0, "two records share an ID");
}

#[test]
fn serialize_refuses_a_document_that_would_not_validate() {
    let mut f = parsed(vec![ready(ID_A, "demo")]);
    f.leases[0].project = "Demo".to_string();
    assert_eq!(serialize(&f).err().unwrap().0, "the change is not valid");
}

#[test]
fn serialize_refuses_an_impossible_date() {
    let mut f = parsed(vec![ready(ID_A, "demo")]);
    f.leases[0].created_at = "2026-02-30T00:00:00Z".to_string();
    assert!(serialize(&f).is_err());
}

#[test]
fn serialize_refuses_a_state_with_the_wrong_members() {
    let mut f = parsed(vec![ready(ID_A, "demo")]);
    f.leases[0].ssh = None;
    assert!(serialize(&f).is_err());
    let mut g = parsed(vec![pending(ID_A, "demo")]);
    g.leases[0].expires_at = Some(EXPIRES.to_string());
    assert!(serialize(&g).is_err());
}

#[test]
fn messages_name_no_path_user_or_library() {
    let m = err(parse(br#"{"version":2,"leases":[]}"#));
    assert!(!m.contains('/') && !m.contains('\\'));
    assert!(!m.to_lowercase().contains("serde") && !m.to_lowercase().contains("expected"));
}

// ---- Ruling 4 (architect, 2026-11-19): a time that names no instant ----

#[test]
fn a_ready_record_with_an_impossible_expiry_is_expired_with_no_ssh() {
    let record = Record {
        sized_for: None,
        plan: None,
        plan_source: None,
        id: ID_A.to_string(),
        project: "demo".to_string(),
        state: "ready".to_string(),
        platform: "linux/arm64".to_string(),
        profile: "agent-linux".to_string(),
        size: "m".to_string(),
        ttl_seconds: 3600,
        created_at: CREATED.to_string(),
        expires_at: Some("2026-02-30T00:00:00Z".to_string()),
        released_at: None,
        ssh: Some(Ssh {
            host_alias: "agentspace-demo-3f2a9c1e".to_string(),
            user: "ubuntu".to_string(),
        }),
        reasons: None,
        driver: Some("ec2".to_string()),
    };
    // Whatever the clock says, the lease is not shown as ready.
    for now in [0, EXPIRES_UNIX, i64::MAX] {
        let d = lease_document(&record, now);
        assert_eq!(d["state"], "expired", "now = {now}");
        assert!(d.get("ssh").is_none(), "now = {now}");
        assert_eq!(d["expires_at"], "2026-02-30T00:00:00Z");
    }
}

// ---- Members appear in the order lease-api.md's Fields lists them ----

/// The names of an object's members, in the order they stand in the text.
fn member_names(v: &Value) -> Vec<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

fn names(extra: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = [
        "id",
        "project",
        "state",
        "platform",
        "profile",
        "size",
        "ttl_seconds",
        "created_at",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.extend(extra.iter().map(|s| s.to_string()));
    v
}

#[test]
fn a_lease_document_lists_its_members_in_fields_order() {
    let f = parsed(vec![
        ready(ID_A, "demo"),
        pending(ID_B, "demo"),
        released_from_ready(ID_C, "demo"),
        unsatisfiable("ffffffff-0000-4000-8000-000000000000", "demo"),
    ]);
    let want = [
        names(&["expires_at", "ssh"]),
        names(&[]),
        names(&["expires_at", "released_at"]),
        names(&["reasons"]),
    ];
    for (r, want) in f.leases.iter().zip(want) {
        let d = lease_document(r, EXPIRES_UNIX - 1);
        let text = serde_json::to_string(&d).unwrap();
        assert_eq!(member_names(&d), want, "{text}");
    }
    // The ssh object keeps host_alias before user.
    let text = serde_json::to_string(&lease_document(&f.leases[0], EXPIRES_UNIX - 1)).unwrap();
    assert!(
        text.ends_with(r#""ssh":{"host_alias":"agentspace-demo-3f2a9c1e","user":"ubuntu"}}"#),
        "{text}"
    );
}

#[test]
fn an_expired_lease_document_keeps_the_order_without_ssh() {
    let f = parsed(vec![ready(ID_A, "demo")]);
    let d = lease_document(&f.leases[0], EXPIRES_UNIX);
    assert_eq!(member_names(&d), names(&["expires_at"]));
    assert_eq!(
        serde_json::to_string(&d).unwrap(),
        format!(
            "{{\"id\":\"{ID_A}\",\"project\":\"demo\",\"state\":\"expired\",\
             \"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\
             \"ttl_seconds\":3600,\"created_at\":\"{CREATED}\",\"expires_at\":\"{EXPIRES}\"}}"
        )
    );
}

#[test]
fn a_list_document_lists_its_leases_with_members_in_fields_order() {
    let f = parsed(vec![ready(ID_A, "demo"), pending(ID_B, "demo")]);
    // Both created at one second, so the ID order decides: ID_A (3f..) before ID_B (9b..).
    let d = list_document(&f, None, &[], EXPIRES_UNIX);
    assert_eq!(
        serde_json::to_string(&d).unwrap(),
        format!(
            "{{\"leases\":[\
             {{\"id\":\"{ID_A}\",\"project\":\"demo\",\"state\":\"expired\",\
             \"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\
             \"ttl_seconds\":3600,\"created_at\":\"{CREATED}\",\"expires_at\":\"{EXPIRES}\"}},\
             {{\"id\":\"{ID_B}\",\"project\":\"demo\",\"state\":\"pending\",\
             \"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\
             \"ttl_seconds\":3600,\"created_at\":\"{CREATED}\"}}]}}"
        )
    );
    // And with ssh present, before the lease expires.
    let d = list_document(&f, None, &["ready"], EXPIRES_UNIX - 1);
    assert_eq!(
        serde_json::to_string(&d).unwrap(),
        format!(
            "{{\"leases\":[\
             {{\"id\":\"{ID_A}\",\"project\":\"demo\",\"state\":\"ready\",\
             \"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\
             \"ttl_seconds\":3600,\"created_at\":\"{CREATED}\",\"expires_at\":\"{EXPIRES}\",\
             \"ssh\":{{\"host_alias\":\"agentspace-demo-3f2a9c1e\",\"user\":\"ubuntu\"}}}}]}}"
        )
    );
}

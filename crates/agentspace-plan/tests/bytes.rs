//! Checks 3 and 4 of lease-api.md's Plan table, and the file's byte-level rules the crate sees:
//! UTF-8, the leading byte order mark, JSON, nesting and the file's size (check 1).
//!
//! RFC 2119 keywords here are the specs' own.

mod common;

use common::*;
use serde_json::{Value, json};

fn valid_text() -> String {
    one_story_plan(vec![sub("a"), sub("b")]).to_string()
}

fn with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

// ---- The leading byte order mark ----

#[test]
fn a_leading_utf8_byte_order_mark_is_accepted_and_ignored() {
    let plan = parse_ok(&with_bom(&valid_text()));
    assert_eq!(plan.busiest(), ("T1", 3));
    assert_eq!(plan.document(), &one_story_plan(vec![sub("a"), sub("b")]));
}

#[test]
fn a_byte_order_mark_is_not_part_of_the_first_member() {
    // If the mark stayed in the text, "{" would not be the first character.
    let plan = parse_ok(&with_bom(
        r#"{"name":"n","turns":[{"name":"T","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]}]}"#,
    ));
    assert_eq!(plan.name(), Some("n"));
}

#[test]
fn a_byte_order_mark_alone_is_not_json() {
    assert_eq!(parse_err(&[0xEF, 0xBB, 0xBF]), NOT_JSON);
    assert_eq!(parse_err(&with_bom("   \n")), NOT_JSON);
}

#[test]
fn a_second_byte_order_mark_is_refused() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&with_bom(&valid_text()));
    assert_eq!(parse_err(&bytes), NOT_JSON);
}

#[test]
fn a_byte_order_mark_after_leading_whitespace_is_refused() {
    let mut bytes = b" ".to_vec();
    bytes.extend_from_slice(&with_bom(&valid_text()));
    assert_eq!(parse_err(&bytes), NOT_JSON);
}

#[test]
fn a_byte_order_mark_between_tokens_or_after_the_document_is_refused() {
    let mid = "{\u{feff}\"turns\":[]}";
    assert_eq!(parse_err(mid.as_bytes()), NOT_JSON);
    let after = format!("{}\u{feff}", valid_text());
    assert_eq!(parse_err(after.as_bytes()), NOT_JSON);
    let after_bom_start = with_bom(&format!("{}\u{feff}", valid_text()));
    assert_eq!(parse_err(&after_bom_start), NOT_JSON);
}

#[test]
fn a_byte_order_mark_inside_a_string_is_refused_as_a_name_of_the_wrong_form() {
    // U+FEFF in a string is valid JSON, and no name, model or effort may hold it.
    let mut p = one_story_plan(vec![sub("a")]);
    p["turns"][0]["name"] = json!("T\u{feff}");
    assert_eq!(err(&p), SCHEMA);
    let mut p = one_story_plan(vec![sub("a")]);
    p["turns"][0]["stories"][0]["subagents"][0]["model"] = json!("\u{feff}Opus");
    assert_eq!(err(&p), SCHEMA);
}

// ---- UTF-8 ----

#[test]
fn bytes_that_are_not_utf8_are_refused_with_the_utf8_message() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("a lone 0xFF", vec![0xFF]),
        ("a lone continuation byte", vec![0x80]),
        ("a truncated two-byte sequence", vec![0xC3]),
        ("an overlong NUL", vec![0xC0, 0x80]),
        ("an encoded surrogate", vec![0xED, 0xA0, 0x80]),
        ("a code point past U+10FFFF", vec![0xF4, 0x90, 0x80, 0x80]),
        (
            "a byte that never starts a sequence",
            vec![0xF5, 0x80, 0x80, 0x80],
        ),
    ];
    for (what, bad) in cases {
        let mut inside = br#"{"name":""#.to_vec();
        inside.extend_from_slice(&bad);
        inside.extend_from_slice(br#"","turns":[]}"#);
        assert_eq!(parse_err(&inside), NOT_UTF8, "{what}, inside a name");
        assert_eq!(parse_err(&bad), NOT_UTF8, "{what}, alone");
        let mut after = valid_text().into_bytes();
        after.extend_from_slice(&bad);
        assert_eq!(parse_err(&after), NOT_UTF8, "{what}, after a plan");
    }
}

#[test]
fn bad_utf8_is_reported_before_bad_json() {
    // Not JSON and not UTF-8: check 3 is first.
    assert_eq!(parse_err(&[b'{', 0xFF, b'}', b'}', b'}']), NOT_UTF8);
    // A byte order mark does not change which check is first.
    assert_eq!(parse_err(&[0xEF, 0xBB, 0xBF, 0xFF]), NOT_UTF8);
}

#[test]
fn utf16_with_a_byte_order_mark_is_refused_as_not_utf8() {
    let text = valid_text();
    let mut le = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        le.extend_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(parse_err(&le), NOT_UTF8);
    let mut be = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        be.extend_from_slice(&unit.to_be_bytes());
    }
    assert_eq!(parse_err(&be), NOT_UTF8);
}

#[test]
fn utf16_without_a_byte_order_mark_is_refused() {
    // ASCII text in UTF-16 is valid UTF-8 with a NUL between its characters, so the first check
    // it fails is JSON (check 4), by the table's order (N2 in the report).
    let text = valid_text();
    let mut le = Vec::new();
    for unit in text.encode_utf16() {
        le.extend_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(parse_err(&le), NOT_JSON);
    let mut be = Vec::new();
    for unit in text.encode_utf16() {
        be.extend_from_slice(&unit.to_be_bytes());
    }
    assert_eq!(parse_err(&be), NOT_JSON);
}

#[test]
fn well_formed_non_ascii_text_is_utf8_and_then_a_schema_fault_in_a_name() {
    // A valid UTF-8 name that is not printable ASCII passes checks 3 and 4 and fails check 6.
    let p = one_story_plan(vec![sub("caf\u{e9}")]);
    assert_eq!(err(&p), SCHEMA);
    let p = named_plan(
        "\u{65E5}\u{672C}",
        vec![turn("T", vec![story("S", vec![sub("a")])])],
    );
    assert_eq!(err(&p), SCHEMA);
}

// ---- JSON ----

#[test]
fn text_that_is_not_one_json_document_is_refused() {
    let bad: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("spaces", b"   ".to_vec()),
        ("whitespace of every JSON kind", b" \t\r\n \n".to_vec()),
        (
            "a non-JSON space first",
            "\u{a0}{\"turns\":[]}".as_bytes().to_vec(),
        ),
        ("text before the document", b"x{\"turns\":[]}".to_vec()),
        (
            "text after the document",
            format!("{} x", valid_text()).into_bytes(),
        ),
        (
            "two documents",
            format!("{}{}", valid_text(), valid_text()).into_bytes(),
        ),
        (
            "two documents on two lines",
            format!("{}\n{}", valid_text(), valid_text()).into_bytes(),
        ),
        ("a trailing comma", br#"{"turns":[],}"#.to_vec()),
        (
            "a trailing comma in an array",
            br#"{"turns":[1,]}"#.to_vec(),
        ),
        ("single quotes", b"{'turns':[]}".to_vec()),
        ("unquoted names", b"{turns:[]}".to_vec()),
        ("a comment", b"{\"turns\":[]}//x".to_vec()),
        (
            "a truncated document",
            valid_text().as_bytes()[..20].to_vec(),
        ),
        ("NaN", br#"{"turns":NaN}"#.to_vec()),
        (
            "a raw control character in a string",
            b"{\"name\":\"a\x01b\",\"turns\":[]}".to_vec(),
        ),
        (
            "a raw newline in a string",
            b"{\"name\":\"a\nb\",\"turns\":[]}".to_vec(),
        ),
        ("a bad escape", br#"{"name":"a\qb","turns":[]}"#.to_vec()),
        (
            "a short unicode escape",
            br#"{"name":"a\u00","turns":[]}"#.to_vec(),
        ),
        ("a leading plus", b"+1".to_vec()),
        ("a bare word", b"turns".to_vec()),
    ];
    for (what, bytes) in bad {
        assert_eq!(parse_err(&bytes), NOT_JSON, "{what}");
    }
}

#[test]
fn whitespace_around_the_document_is_fine() {
    let text = format!(" \t\r\n{}\r\n\n ", valid_text());
    assert_eq!(parse_ok(text.as_bytes()).busiest(), ("T1", 3));
    assert_eq!(parse_ok(&with_bom(&text)).busiest(), ("T1", 3));
}

#[test]
fn a_string_with_an_unpaired_surrogate_is_not_json() {
    let at_name = |escape: &str| {
        format!(
            r#"{{"name":"{escape}","turns":[{{"name":"T","stories":[{{"name":"S","subagents":[{{"name":"a","model":"m","effort":"e"}}]}}]}}]}}"#
        )
    };
    for lone in [
        r"\ud800",
        r"\udbff",
        r"\udc00",
        r"\udfff",
        r"\ud800x",
        r"\ud800\ud800",
        r"\udc00\ud800",
    ] {
        assert_eq!(parse_err(at_name(lone).as_bytes()), NOT_JSON, "{lone}");
    }
    // A pair in the right order is one character, valid JSON, and not a printable ASCII name.
    let pair = format!("{}{}", r"\ud83d", r"\ude00");
    assert_eq!(parse_err(at_name(&pair).as_bytes()), SCHEMA);
}

#[test]
fn an_unpaired_surrogate_in_a_member_name_is_not_json_either() {
    assert_eq!(parse_err(br#"{"\ud800":1,"turns":[]}"#), NOT_JSON);
}

#[test]
fn json_is_checked_before_a_repeated_member() {
    assert_eq!(parse_err(br#"{"a":1,"a":2,"turns":[],}"#), NOT_JSON);
}

#[test]
fn escapes_that_spell_ascii_are_the_same_text() {
    let text = br#"{"name":"v0.09\/x","turns":[{"name":"T","stories":[{"name":"S","subagents":[
        {"name":"a","model":"Opus","effort":"high"}]}]}]}"#;
    let plan = parse_ok(text);
    assert_eq!(plan.name(), Some("v0.09/x"));
    assert_eq!(
        plan.document()["turns"][0]["stories"][0]["subagents"][0]["model"],
        "Opus"
    );
}

// ---- Nesting ----

fn nested_arrays(levels: usize) -> String {
    format!("{}{}", "[".repeat(levels), "]".repeat(levels))
}

#[test]
fn a_document_nested_to_64_levels_is_read_and_then_refused_for_its_shape() {
    // A parser MUST allow at least 64 levels, so a document this deep is never "not valid JSON";
    // it is a document that is not shaped like a plan.
    assert_eq!(parse_err(nested_arrays(64).as_bytes()), SCHEMA);
    let objects = format!("{}1{}", "{\"a\":".repeat(63), "}".repeat(63));
    assert_eq!(parse_err(objects.as_bytes()), SCHEMA);
    // A plan whose unknown member nests 60 levels deep is refused as a shape, not as JSON.
    let mut p = one_story_plan(vec![sub("a")]);
    let mut deep: Value = json!(1);
    for _ in 0..58 {
        deep = json!([deep]);
    }
    p["extra"] = deep;
    assert_eq!(err(&p), SCHEMA);
}

#[test]
fn a_document_nested_past_the_parsers_limit_never_panics() {
    // The parser MAY limit nesting, so either refusal below is allowed; the crate MUST NOT panic or
    // overflow its stack, and MUST NOT accept the document, which is never a plan.
    // Within the file's size: 40000 object levels is 200000 bytes.
    for levels in [129, 1000, 40_000] {
        let why = parse_err(nested_arrays(levels).as_bytes());
        assert!(why == NOT_JSON || why == SCHEMA, "{levels} levels: {why}");
        let objects = format!("{}1{}", "{\"a\":".repeat(levels), "}".repeat(levels));
        let why = parse_err(objects.as_bytes());
        assert!(
            why == NOT_JSON || why == SCHEMA,
            "{levels} object levels: {why}"
        );
    }
}

#[test]
fn a_document_nested_past_the_depth_a_plan_can_have_is_never_a_plan() {
    // A plan's deepest value is at level 9; wrapping a plan in one more array is not a plan.
    let wrapped = format!("[{}]", valid_text());
    assert_eq!(parse_err(wrapped.as_bytes()), SCHEMA);
}

// ---- The file's size (check 1) ----

/// A valid plan padded with trailing spaces to exactly `total` bytes.
fn padded_to(total: usize, bom: bool) -> Vec<u8> {
    let mut bytes = if bom {
        with_bom(&valid_text())
    } else {
        valid_text().into_bytes()
    };
    assert!(bytes.len() <= total);
    bytes.resize(total, b' ');
    bytes
}

#[test]
fn a_file_of_exactly_262144_bytes_is_accepted() {
    assert_eq!(parse_ok(&padded_to(262_144, false)).busiest(), ("T1", 3));
}

#[test]
fn a_file_of_262144_bytes_with_its_byte_order_mark_counted_is_accepted() {
    assert_eq!(parse_ok(&padded_to(262_144, true)).busiest(), ("T1", 3));
}

#[test]
fn a_file_one_byte_past_the_bound_is_refused_as_larger() {
    // The crate's input is bytes, and lease-api.md has the CLI check a file's size before it reads
    // the file, so the crate checking it as well is a choice the specs do not pin (N3 in the
    // report). A crate that does MUST give check 1's message, and this takes it as the choice.
    for bom in [false, true] {
        assert_eq!(
            parse_err(&padded_to(262_145, bom)),
            TOO_LARGE,
            "with a mark: {bom}"
        );
    }
}

#[test]
fn check_1_is_first_whatever_else_is_wrong_with_a_larger_file() {
    assert_eq!(parse_err(&vec![0xFF; 300_000]), TOO_LARGE);
    let mut not_json = vec![b'x'; 300_000];
    not_json[0] = b'{';
    assert_eq!(parse_err(&not_json), TOO_LARGE);
    assert_eq!(parse_err(&vec![b' '; 1_000_000]), TOO_LARGE);
}

#[test]
fn the_most_a_plan_can_hold_in_one_file_fits_in_it() {
    // 32 turns of 1 story of 32 subagents is 1024 subagents: it must be a plan within 262144 bytes,
    // and v0.06's own shape (six turns, twenty-six stories) is much smaller.
    let turns: Vec<Value> = (0..32)
        .map(|t| turn(&format!("T{t}"), vec![story("S", antichain(32))]))
        .collect();
    let text = plan(turns).to_string();
    assert!(text.len() < 262_144, "{} bytes", text.len());
    assert_eq!(parse_ok(text.as_bytes()).busiest(), ("T0", 33));
}

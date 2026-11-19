//! The holder record: its bytes and every reader rule (docs/state.md, The holder record).
mod common;

use agentspace_keeper_api::HolderRecord;
use common::*;

const SCHEMA: &str = "board-holder.state.schema.json";

/// A record's bytes from four JSON values, each spliced in as given (so `"x"` needs its quotes).
fn raw(pid: &str, started: &str, lease: &str, alias: &str) -> Vec<u8> {
    format!(
        "{{\"pid\":{pid},\"process_started_at\":{started},\"lease_id\":{lease},\"alias\":{alias}}}\n"
    )
    .into_bytes()
}

fn q(text: &str) -> String {
    format!("\"{text}\"")
}

/// The record's bytes with each text member quoted (no JSON escapes are applied).
fn with(pid: &str, started: &str, lease: &str, alias: &str) -> Vec<u8> {
    raw(pid, &q(started), &q(lease), &q(alias))
}

fn read_with(pid: &str, started: &str, lease: &str, alias: &str) -> Option<HolderRecord> {
    parse_holder_record(&with(pid, started, lease, alias))
}

// ---- Bytes ----

#[test]
fn the_record_is_written_as_exact_compact_bytes() {
    let want = format!(
        "{{\"pid\":4242,\"process_started_at\":\"{STARTED_AT}\",\"lease_id\":\"{LEASE_ID}\",\"alias\":\"{ALIAS}\"}}\n"
    );
    assert_eq!(holder_record_bytes(&good_record()), want.into_bytes());
}

#[test]
fn the_written_record_meets_its_schema_and_the_file_rules() {
    let v = assert_valid_json_file(&holder_record_bytes(&good_record()), SCHEMA);
    assert_eq!(v["pid"], 4242);
}

#[test]
fn the_extreme_pids_are_written_as_integers() {
    for pid in [1u32, 2147483647] {
        let mut r = good_record();
        r.pid = pid;
        let v = assert_valid_json_file(&holder_record_bytes(&r), SCHEMA);
        assert_eq!(v["pid"], pid);
    }
}

#[test]
fn the_schema_check_can_fail() {
    // A control: the check above finds a bad record, so a clean pass means something.
    let bad: serde_json::Value =
        serde_json::from_str(r#"{"pid":0,"process_started_at":"x","lease_id":"y","alias":"z"}"#)
            .unwrap();
    assert!(!validator(SCHEMA).is_valid(&bad));
}

#[test]
fn a_written_record_reads_back_the_same() {
    let r = good_record();
    assert_eq!(parse_holder_record(&holder_record_bytes(&r)), Some(r));
}

// ---- The reader's rules: a record that fails is no record ----

#[test]
fn a_good_record_is_read() {
    assert_eq!(
        read_with("4242", STARTED_AT, LEASE_ID, ALIAS),
        Some(good_record())
    );
}

#[test]
fn a_record_without_its_final_lf_is_still_read() {
    // Architect's ruling, 2026-11-19 (state.md, The holder record): a reader MUST NOT require the
    // final LF, and accepts whitespace JSON allows.
    let mut b = with("4242", STARTED_AT, LEASE_ID, ALIAS);
    b.pop();
    assert_eq!(parse_holder_record(&b), Some(good_record()));
}

#[test]
fn an_empty_file_is_no_record() {
    assert_eq!(parse_holder_record(b""), None);
}

#[test]
fn text_that_is_not_json_is_no_record() {
    for b in [&b"not json\n"[..], b"{", b"{\"pid\":4242,", b"\x00\x01\x02"] {
        assert_eq!(parse_holder_record(b), None, "{b:?}");
    }
}

#[test]
fn bytes_that_are_not_utf8_are_no_record() {
    let mut b = with("4242", STARTED_AT, LEASE_ID, ALIAS);
    b.splice(1..1, [0xFF, 0xFE]);
    assert_eq!(parse_holder_record(&b), None);
}

#[test]
fn json_that_is_not_an_object_is_no_record() {
    for b in [&b"[]\n"[..], b"null\n", b"4242\n", b"\"x\"\n", b"true\n"] {
        assert_eq!(parse_holder_record(b), None, "{b:?}");
    }
}

#[test]
fn a_missing_member_is_no_record() {
    let full = [
        ("pid", "4242".to_string()),
        ("process_started_at", q(STARTED_AT)),
        ("lease_id", q(LEASE_ID)),
        ("alias", q(ALIAS)),
    ];
    for skip in 0..full.len() {
        let members: Vec<String> = full
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, (k, v))| format!("\"{k}\":{v}"))
            .collect();
        let b = format!("{{{}}}\n", members.join(",")).into_bytes();
        assert_eq!(parse_holder_record(&b), None, "Without {}", full[skip].0);
    }
}

#[test]
fn a_pid_out_of_range_or_not_an_integer_is_no_record() {
    for pid in [
        "0",
        "-1",
        "2147483648",
        "4294967296",
        "99999999999999999999",
        "1.5",
        "4242.5",
        "\"4242\"",
        "null",
        "true",
        "[4242]",
    ] {
        let b = raw(pid, &q(STARTED_AT), &q(LEASE_ID), &q(ALIAS));
        assert_eq!(parse_holder_record(&b), None, "pid {pid}");
    }
}

#[test]
fn the_pid_range_is_one_to_2147483647() {
    for pid in [1u32, 2147483647] {
        let r = read_with(&pid.to_string(), STARTED_AT, LEASE_ID, ALIAS).expect("In range");
        assert_eq!(r.pid, pid);
    }
}

#[test]
fn a_start_time_of_the_wrong_form_is_no_record() {
    for t in [
        "",
        "2026-11-19T13:45:07Z",
        "2026-11-19T13:45:07.12Z",
        "2026-11-19T13:45:07.1234567Z",
        "2026-11-19T13:45:07.123",
        "2026-11-19 13:45:07.123Z",
        "2026-11-19T13:45:07.123+00:00",
        "2026-13-19T13:45:07.123Z",
        "2026-11-32T13:45:07.123Z",
        "2026-11-19T24:45:07.123Z",
        "2026-11-19T13:60:07.123Z",
        "2026-11-19T13:45:60.123Z",
        "2026-11-19T13:45:07.123Z\\n",
        " 2026-11-19T13:45:07.123Z",
        "2026-11-19T13:45:07.123z",
        "yesterday",
    ] {
        assert_eq!(read_with("4242", t, LEASE_ID, ALIAS), None, "time {t:?}");
    }
    for t in ["20", "null", "[]", "true"] {
        let b = raw("4242", t, &q(LEASE_ID), &q(ALIAS));
        assert_eq!(parse_holder_record(&b), None, "time {t}");
    }
}

#[test]
fn a_lease_id_that_is_not_a_uuid_is_no_record() {
    for id in [
        "",
        "not-a-uuid",
        "0123abcd-4def-4abc-9def-0123456789a",
        "0123abcd-4def-4abc-9def-0123456789abc",
        "0123abcd4def4abc9def0123456789ab",
        "0123abcd-4def-4abc-9def-0123456789ag",
        "{0123abcd-4def-4abc-9def-0123456789ab}",
        "0123abcd-4def-4abc-9def-0123456789ab\\n",
        " 0123abcd-4def-4abc-9def-0123456789ab",
    ] {
        assert_eq!(read_with("4242", STARTED_AT, id, ALIAS), None, "id {id:?}");
    }
    for id in ["7", "null", "[]"] {
        let b = raw("4242", &q(STARTED_AT), id, &q(ALIAS));
        assert_eq!(parse_holder_record(&b), None, "id {id}");
    }
}

#[test]
fn a_lease_id_in_upper_case_or_of_another_version_is_read_and_kept_as_stored() {
    // state.md, The holder record: a reader MAY accept any 8-4-4-4-12 hex UUID in any case, and
    // keeps `lease_id` as stored (architect, 2026-11-19, for E0.06-T01-S07); the keeper compares
    // it with a lease's ID without regard to ASCII case.
    for id in [
        LEASE_ID.to_uppercase(),
        "0123ABCD-4def-4ABC-9def-0123456789Ab".to_string(),
        "0123abcd-1def-1abc-1def-0123456789ab".to_string(),
    ] {
        let r = read_with("4242", STARTED_AT, &id, ALIAS)
            .unwrap_or_else(|| panic!("Lease ID {id} is no record"));
        assert_eq!(r.lease_id, id);
    }
}

#[test]
fn whitespace_json_allows_is_accepted_around_and_between_members() {
    // state.md, The holder record: a reader accepts whitespace JSON allows, and does not require
    // the final LF (architect, 2026-11-19, for E0.06-T01-S07).
    let text = format!(
        " \t\r\n{{ \"pid\" :\t4242 ,\r\n  \"process_started_at\": \"{STARTED_AT}\",\n  \"lease_id\":\"{LEASE_ID}\" ,\"alias\" : \"{ALIAS}\"\r\n}}\r\n\r\n"
    );
    assert_eq!(parse_holder_record(text.as_bytes()), Some(good_record()));
}

#[test]
fn what_json_does_not_call_whitespace_is_refused() {
    // A vertical tab, a form feed and a no-break space are not JSON whitespace.
    for lead in ["\u{0B}", "\u{0C}", "\u{A0}"] {
        let mut b = lead.as_bytes().to_vec();
        b.extend(with("4242", STARTED_AT, LEASE_ID, ALIAS));
        assert_eq!(parse_holder_record(&b), None, "{lead:?}");
    }
}

#[test]
fn a_byte_order_mark_and_a_member_the_schema_does_not_name_are_tolerated_and_ignored() {
    // state.md, The holder record (architect, 2026-11-19, for E0.06-T01-S07).
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend(with("4242", STARTED_AT, LEASE_ID, ALIAS));
    assert_eq!(parse_holder_record(&b), Some(good_record()));
    let extra = format!(
        "{{\"pid\":4242,\"note\":{{\"x\":[1,null]}},\"process_started_at\":\"{STARTED_AT}\",\"lease_id\":\"{LEASE_ID}\",\"alias\":\"{ALIAS}\",\"z\":\"y\"}}\n"
    );
    assert_eq!(parse_holder_record(extra.as_bytes()), Some(good_record()));
    // A byte order mark after a blank, or UTF-16's, is not one.
    let mut late = b" ".to_vec();
    late.extend([0xEF, 0xBB, 0xBF]);
    late.extend(with("4242", STARTED_AT, LEASE_ID, ALIAS));
    assert_eq!(parse_holder_record(&late), None);
    let mut utf16 = vec![0xFF, 0xFE];
    utf16.extend(with("4242", STARTED_AT, LEASE_ID, ALIAS));
    assert_eq!(parse_holder_record(&utf16), None);
}

#[test]
fn a_member_named_twice_is_no_record() {
    let text = format!(
        "{{\"pid\":4242,\"process_started_at\":\"{STARTED_AT}\",\"lease_id\":\"{LEASE_ID}\",\"alias\":\"{ALIAS}\",\"alias\":\"{ALIAS}\"}}\n"
    );
    assert_eq!(parse_holder_record(text.as_bytes()), None);
}

#[test]
fn an_alias_that_does_not_meet_the_rule_is_no_record() {
    // Gap FB-23: `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z`. Each is spliced in as JSON text, so
    // `\n`, `\t` and `é` below are JSON escapes the parser turns into the character.
    let too_long = format!("a{}", "b".repeat(128));
    for alias in [
        "",
        ".leading-dot",
        "-leading-hyphen",
        "_leading",
        "has space",
        "has/slash",
        "has\\\\slash",
        "has:colon",
        "tab\\tinside",
        "trailing\\n",
        "trailing\\r\\n",
        "h\\u00e9llo",
        "-oProxyCommand=x",
        too_long.as_str(),
    ] {
        assert_eq!(
            read_with("4242", STARTED_AT, LEASE_ID, alias),
            None,
            "alias {alias:?}"
        );
    }
    for alias in ["null", "7", "[]", "true"] {
        let b = raw("4242", &q(STARTED_AT), &q(LEASE_ID), alias);
        assert_eq!(parse_holder_record(&b), None, "alias {alias}");
    }
}

#[test]
fn an_alias_the_rule_allows_is_read_even_where_the_schema_is_stricter() {
    // The strict schema's form is the writer's; the reader accepts any alias the rule allows.
    let longest = format!("a{}", "b".repeat(127));
    for alias in [
        "a",
        "A",
        "0",
        "Ssh.Host-1",
        "x..y--z",
        // FB-23's rule allows `_` after the first character. This case was on the refused list,
        // a test defect (architect, 2026-11-19, for E0.06-T01-S07).
        "has_underscore",
        "a_",
        "9.",
        "Z-",
        "agentspace-demo-0123abcd",
        longest.as_str(),
    ] {
        let r = read_with("4242", STARTED_AT, LEASE_ID, alias)
            .unwrap_or_else(|| panic!("Alias {alias:?} is no record"));
        assert_eq!(r.alias, alias);
    }
}

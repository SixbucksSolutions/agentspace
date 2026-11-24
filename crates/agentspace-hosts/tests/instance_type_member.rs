//! A record's `instance_type` (docs/state.md, "The host registry", the member table and the
//! bullets under it; "The seed"; "The instance type", "Its form, and programs built before
//! v0.09"; E0.09-T02-S01, card #0200): reading it, writing it back byte for byte, the values that
//! make a file not valid, and the default only a missing file reads as.
//!
//! This file needs nothing of the crate beyond the `Host.instance_type` field, so it builds even
//! when the setter of `tests/instance_type.rs` has not landed.

mod common;

use agentspace_hosts::{Host, HostState, Hosts, HostsError, lock, read, write};
use common::*;

const SAMPLE_WITH_TYPES: &str =
    include_str!("../../../schemas/state/samples/hosts.state.instance-type.json");
const SAMPLE_TYPE_ON_ONE: &str =
    include_str!("../../../schemas/state/samples/hosts.state.instance-type-on-one.json");
const SAMPLE_FULL: &str = include_str!("../../../schemas/state/samples/hosts.state.full.json");
const SAMPLE_SEED: &str = include_str!("../../../schemas/state/samples/hosts.state.seed.json");

const INVALID_TYPE_SAMPLES: &[(&str, &str)] = &[
    (
        "instance-type-bad-form",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.instance-type-bad-form.json"
        ),
    ),
    (
        "instance-type-not-string",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.instance-type-not-string.json"
        ),
    ),
    (
        "instance-type-too-long",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.instance-type-too-long.json"
        ),
    ),
    (
        "null-instance-type",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.null-instance-type.json"),
    ),
];

fn read_text(text: &str) -> Result<Hosts, HostsError> {
    let t = TempHome::new();
    t.put_hosts(text);
    read(t.path())
}

/// The file is read as not valid, as every other invalid record is (read_registry.rs's way), and
/// nothing in agentspace repairs, rewrites or removes it.
fn assert_refused(label: &str, bytes: impl AsRef<[u8]>) {
    let t = TempHome::new();
    t.put_hosts(bytes.as_ref());
    let before = t.hosts_bytes();
    match read(t.path()) {
        Ok(h) => panic!(
            "{label}: a file that is not valid was read as {} records",
            h.hosts.len()
        ),
        Err(e) => {
            assert!(
                is_not_valid(&e),
                "{label}: expected a not-valid error, got {e:?}"
            );
            assert_message(&e, t.path());
        }
    }
    assert_eq!(
        t.hosts_bytes(),
        before,
        "{label}: nothing in agentspace repairs, rewrites or removes a file that is not valid"
    );
}

/// A document of the box's record and one more, whose `instance_type` member is `member`: the
/// JSON text of its value, such as `"m9g.xlarge"` with its quotes, or `4`.
fn doc_with_type(member: &str) -> String {
    doc_with(&format!(
        "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_type\":{member}}}"
    ))
}

fn write_new(t: &TempHome, h: &Hosts) -> Result<(), HostsError> {
    let guard = lock(t.path())?;
    write(t.path(), h, &guard)
}

fn typed(mut h: Host, ty: &str) -> Host {
    h.instance_type = Some(ty.to_string());
    h
}

// --- Reading: a record with the member, and one without it ------------------------------------

#[test]
fn a_record_with_an_instance_type_reads_it_as_the_string_stored() {
    let h = read_text(&doc_with_type("\"m9g.2xlarge\"")).unwrap();
    assert_eq!(h.hosts.len(), 2);
    assert_eq!(h.hosts[1].instance_type.as_deref(), Some("m9g.2xlarge"));
    // The box's record in the same file has no member and reads without one (pin P12's "no type").
    assert_eq!(h.hosts[0].instance_type, None);
}

#[test]
fn a_record_without_an_instance_type_reads_none() {
    let h = read_text(&doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\"}",
    ))
    .unwrap();
    assert_eq!(h.hosts[1].instance_type, None);
}

#[test]
fn the_member_is_read_in_a_record_of_each_state() {
    for (text, state) in [
        ("creating", HostState::Creating),
        ("ready", HostState::Ready),
        ("failed", HostState::Failed),
    ] {
        let doc = doc_with(&format!(
            "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"{text}\",\"instance_type\":\"c7g.large\"}}"
        ));
        let h = read_text(&doc).unwrap();
        assert_eq!(h.hosts[1].state, state, "{text}");
        assert_eq!(
            h.hosts[1].instance_type.as_deref(),
            Some("c7g.large"),
            "{text}"
        );
    }
}

#[test]
fn the_box_may_carry_an_instance_type() {
    let doc = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_type\":\"m9g.xlarge\"}]}";
    let h = read_text(doc).unwrap();
    assert_eq!(h.hosts[0].instance_type.as_deref(), Some("m9g.xlarge"));
}

#[test]
fn a_reader_does_not_depend_on_member_order_for_the_type() {
    let doc = "{\"hosts\":[{\"instance_type\":\"m9g.large\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\",\"instance_id\":\"i-1234567890abcdef0\",\"state\":\"ready\",\"stack\":\"agentbox\",\"project\":\"swimcoach\"}],\"version\":0}";
    let h = read_text(doc).unwrap();
    assert_eq!(
        h.hosts[0],
        Host {
            instance_id: Some("i-1234567890abcdef0".to_string()),
            instance_type: Some("m9g.large".to_string()),
            last_attempt_at: Some("2026-11-24T19:05:42Z".to_string()),
            ..box_host()
        }
    );
}

#[test]
fn the_samples_with_a_type_are_read_with_it() {
    let h = read_text(SAMPLE_WITH_TYPES).unwrap();
    let types: Vec<Option<&str>> = h.hosts.iter().map(|r| r.instance_type.as_deref()).collect();
    assert_eq!(
        types,
        [
            Some("m9g.xlarge"),
            Some("m9g.2xlarge"),
            Some("m7i-flex.large")
        ]
    );
    let h = read_text(SAMPLE_TYPE_ON_ONE).unwrap();
    assert_eq!(h.hosts[0].instance_type, None);
    assert_eq!(h.hosts[1].instance_type.as_deref(), Some("m9g.large"));
}

#[test]
fn a_type_of_each_allowed_shape_is_read() {
    // `^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*$`, at most 64 characters (state.md, the member table).
    let longest = format!("{}.{}", "a".repeat(32), "b".repeat(31)); // 64 characters
    assert_eq!(longest.len(), 64);
    for ty in [
        "m9g.xlarge",
        "m9g.2xlarge",
        "c7g.metal",
        "a1.xlarge",
        "m7i-flex.large",
        "mac2-m2.metal",
        "0.0",
        "a.b",
        "a-.b-",
        longest.as_str(),
    ] {
        let h = read_text(&doc_with_type(&format!("\"{ty}\"")))
            .unwrap_or_else(|e| panic!("{ty:?} was refused: {e:?}"));
        assert_eq!(h.hosts[1].instance_type.as_deref(), Some(ty), "{ty}");
    }
}

#[test]
fn a_type_the_driver_does_not_know_is_still_valid_in_the_file() {
    // The map is the driver's; the registry owns only the member's form (state.md, "Who reads it").
    let h = read_text(&doc_with_type("\"zz9.plural-z-alpha\"")).unwrap();
    assert_eq!(
        h.hosts[1].instance_type.as_deref(),
        Some("zz9.plural-z-alpha")
    );
}

// --- The default only a missing file has, and only a reader that places reads ----------------
//
// N1: the dev lead's tasking says "a read that finds no file sees `Some(m9g.xlarge)`"; the
// crate as the implementer left it splits that into `read`, the read a change starts from, whose
// missing file is the seed document with no type, and `read_for_placing`, whose missing file is the
// implied record. state.md ("The seed") names no function: it says a reader that finds no file reads
// the record as if it held the type, and a writer that finds no file starts from the seed document
// with no type. Both halves are tested through the two functions; if the dev lead pins one `read`
// instead, `placing_read` below and the tests of `read`'s missing file change together.

/// The read of a reader that places a lease (the one place the crate's name for it is spelled).
fn placing_read(home: &std::path::Path) -> Result<Hosts, HostsError> {
    agentspace_hosts::read_for_placing(home)
}

#[test]
fn a_reader_that_finds_no_file_reads_the_boxs_record_with_the_default_type_and_creates_no_file() {
    // state.md, "The seed", first bullet of "From v0.09 ..."; pins P2c and P31.
    let t = TempHome::new();
    let h = placing_read(t.path()).unwrap();
    assert_eq!(h.hosts.len(), 1);
    assert_eq!(h.hosts[0].instance_type.as_deref(), Some("m9g.xlarge"));
    assert_eq!(h, seed_as_read_for_placing());
    assert!(
        TempHome::names_in(t.path()).is_empty(),
        "Reading never creates the file, its lock or its folder"
    );
    // Reading again changes nothing either, in a home whose folder exists.
    std::fs::create_dir_all(t.state()).unwrap();
    assert_eq!(placing_read(t.path()).unwrap(), seed_as_read_for_placing());
    assert!(
        TempHome::names_in(&t.state()).is_empty(),
        "A read of a missing file leaves nothing in the folder"
    );
}

#[test]
fn the_default_is_read_without_the_lock_and_while_it_is_held() {
    let t = TempHome::new();
    let _held = lock(t.path()).unwrap();
    assert_eq!(placing_read(t.path()).unwrap(), seed_as_read_for_placing());
}

#[test]
fn a_file_that_exists_keeps_a_box_record_with_no_type_without_one_for_every_reader() {
    // state.md: "A file that exists keeps its records as they are: a box record written before
    // v0.09, or from the seed document, has no instance_type." The default is for a missing file.
    for (label, text) in [("seed", SAMPLE_SEED), ("full", SAMPLE_FULL)] {
        let t = TempHome::new();
        t.put_hosts(text);
        assert_eq!(
            read(t.path()).unwrap().hosts[0].instance_type,
            None,
            "{label}"
        );
        assert_eq!(
            placing_read(t.path()).unwrap().hosts[0].instance_type,
            None,
            "{label}: the default is read only from a missing file"
        );
    }
}

#[test]
fn an_existing_file_reads_the_same_for_a_writer_and_for_a_reader_that_places() {
    let t = TempHome::new();
    t.put_hosts(ROUND_TRIP_FILE);
    assert_eq!(read(t.path()).unwrap(), placing_read(t.path()).unwrap());
}

#[test]
fn a_file_that_is_not_valid_is_an_error_for_the_placing_reader_never_the_default() {
    // The default is no fallback: a file that is not valid reads as an error, never as the seed.
    for text in ["not json".to_string(), doc_with_type("4")] {
        let t = TempHome::new();
        t.put_hosts(&text);
        let e = placing_read(t.path()).expect_err("A file that is not valid is an error");
        assert!(is_not_valid(&e), "{e:?}");
        assert!(read(t.path()).is_err());
    }
    let t = TempHome::new();
    std::fs::create_dir_all(t.hosts_json()).unwrap();
    assert!(
        placing_read(t.path()).is_err(),
        "A folder at the registry is not a missing file"
    );
}

#[test]
fn the_read_a_change_starts_from_gives_the_seed_with_no_type_for_a_missing_file() {
    // Pin P31: the seed document's record has no type, so `read` (the writers' read) and
    // Hosts::seed() give it as it is, and only the placing read adds the default.
    let t = TempHome::new();
    let start = read(t.path()).unwrap();
    assert_eq!(start, seed());
    assert_eq!(start, Hosts::seed());
    assert_eq!(start.hosts[0].instance_type, None);
    assert_ne!(start, placing_read(t.path()).unwrap());
    assert_eq!(Hosts::implied(), seed_as_read_for_placing());
    assert_eq!(agentspace_hosts::DEFAULT_INSTANCE_TYPE, "m9g.xlarge");
}

// --- Writing: no default, and the member's place in the bytes ---------------------------------

#[test]
fn a_write_that_finds_no_file_writes_the_seed_record_with_no_type() {
    // Pin P31: a first change writes the seed document's record, as it is, with no instance_type;
    // a writer MUST NOT write the default m9g.xlarge into the file. The change is another
    // project's first lease (the driver's), made as every writer makes one: lock, read, change,
    // write.
    let t = TempHome::new();
    let guard = lock(t.path()).unwrap();
    let mut h = read(t.path()).unwrap();
    h.hosts.push(host("agentspace", HostState::Creating));
    write(t.path(), &h, &guard).unwrap();
    drop(guard);
    let text = String::from_utf8(t.hosts_bytes()).unwrap();
    assert_eq!(
        text,
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\"}]}\n"
    );
    assert!(!text.contains("instance_type"));
    assert_eq!(read(t.path()).unwrap().hosts[0].instance_type, None);
}

#[test]
fn a_write_that_starts_from_the_seed_document_writes_no_type_either() {
    let t = TempHome::new();
    let guard = lock(t.path()).unwrap();
    let mut h = Hosts::seed();
    h.hosts.push(host("agentspace", HostState::Creating));
    write(t.path(), &h, &guard).unwrap();
    drop(guard);
    assert!(
        !String::from_utf8(t.hosts_bytes())
            .unwrap()
            .contains("instance_type")
    );
}

#[test]
fn writing_the_seed_alone_is_the_documents_bytes_with_no_type() {
    let t = TempHome::new();
    write_new(&t, &Hosts::seed()).unwrap();
    assert_eq!(
        t.hosts_bytes(),
        b"{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n"
    );
}

#[test]
fn a_file_with_a_box_record_without_a_type_is_written_back_without_one() {
    // The default is read only from a missing file: an existing file's untyped box is not given
    // the default by a read and a write (pin P31).
    let t = TempHome::new();
    t.put_hosts(SAMPLE_SEED);
    let guard = lock(t.path()).unwrap();
    let mut h = read(t.path()).unwrap();
    h.hosts.push(host("agentspace", HostState::Creating));
    write(t.path(), &h, &guard).unwrap();
    drop(guard);
    let text = String::from_utf8(t.hosts_bytes()).unwrap();
    assert!(!text.contains("instance_type"), "{text}");
}

#[test]
fn the_member_is_written_after_instance_id_and_before_last_attempt_at() {
    // Pin P4: the order of the member table, which fixes a record's bytes.
    let t = TempHome::new();
    let mut r = full_host(
        "agentspace",
        HostState::Ready,
        "i-1234567890abcdef0",
        "2026-11-24T19:05:42Z",
    );
    r.instance_type = Some("m9g.2xlarge".to_string());
    write_new(&t, &with_box(vec![r])).unwrap();
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-1234567890abcdef0\",\"instance_type\":\"m9g.2xlarge\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\"}]}\n"
    );
}

#[test]
fn the_member_alone_is_written_alone_and_an_absent_one_adds_no_bytes() {
    let t = TempHome::new();
    let with_type = typed(host("agentspace", HostState::Ready), "c7g.large");
    let without = host("example-two", HostState::Creating);
    write_new(&t, &with_box(vec![with_type, without])).unwrap();
    let text = String::from_utf8(t.hosts_bytes()).unwrap();
    assert!(text.contains(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_type\":\"c7g.large\"}"
    ));
    // A record without it is written as it was before v0.09.
    assert!(text.contains(
        "{\"project\":\"example-two\",\"stack\":\"agentspace-host-example-two\",\"state\":\"creating\"}"
    ));
    assert_eq!(text.matches("instance_type").count(), 1);
    assert!(!text.contains("null"));
}

#[test]
fn a_type_written_reads_back_the_same_and_the_box_may_hold_one() {
    let t = TempHome::new();
    let mut b = box_host();
    b.instance_type = Some("m9g.xlarge".to_string());
    let h = hosts(vec![
        b,
        typed(
            full_host("a", HostState::Failed, "i-1234abcd", "2026-01-01T00:00:00Z"),
            "t4g.nano",
        ),
        host("b", HostState::Creating),
    ]);
    write_new(&t, &h).unwrap();
    assert_eq!(read(t.path()).unwrap(), h);
}

// --- A round trip, byte for byte --------------------------------------------------------------

const ROUND_TRIP_FILE: &str = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-0123456789abcdef0\",\"instance_type\":\"m9g.xlarge\",\"last_attempt_at\":\"2026-11-24T18:30:00Z\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\"},{\"project\":\"example-two\",\"stack\":\"agentspace-host-example-two\",\"state\":\"failed\",\"instance_type\":\"m7i-flex.large\",\"last_attempt_at\":\"2026-11-24T19:10:00Z\"}]}\n";

#[test]
fn a_file_with_types_is_written_back_byte_for_byte_unchanged() {
    let t = TempHome::new();
    t.put_hosts(ROUND_TRIP_FILE);
    let guard = lock(t.path()).unwrap();
    let h = read(t.path()).unwrap();
    write(t.path(), &h, &guard).unwrap();
    drop(guard);
    assert_eq!(t.hosts_bytes(), ROUND_TRIP_FILE.as_bytes());
}

#[test]
fn the_sample_in_the_writers_own_form_is_written_back_byte_for_byte() {
    let t = TempHome::new();
    let bytes = format!("{}\n", SAMPLE_TYPE_ON_ONE.trim_end());
    t.put_hosts(&bytes);
    let guard = lock(t.path()).unwrap();
    let h = read(t.path()).unwrap();
    write(t.path(), &h, &guard).unwrap();
    drop(guard);
    assert_eq!(t.hosts_bytes(), bytes.as_bytes());
}

#[test]
fn an_unrelated_change_keeps_every_type_exactly_and_every_other_byte_as_the_rules_say() {
    // An existing writer's change (the driver's `last_attempt_at` at a run's start, and
    // `provision`'s `instance_id`): read, change, write. state.md: "Every writer but deploy keeps a
    // record's instance_type exactly as it read it" (pin P12).
    let t = TempHome::new();
    t.put_hosts(ROUND_TRIP_FILE);
    let guard = lock(t.path()).unwrap();
    let mut h = read(t.path()).unwrap();
    h.hosts[0].last_attempt_at = Some("2026-11-24T20:00:00Z".to_string());
    h.hosts[1].state = HostState::Ready;
    h.hosts[1].instance_id = Some("i-0fedcba9876543210".to_string());
    h.hosts[1].last_attempt_at = Some("2026-11-24T20:00:01Z".to_string());
    h.hosts[2].instance_id = Some("i-0123456789abcdef1".to_string());
    write(t.path(), &h, &guard).unwrap();
    drop(guard);
    let expected = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-0123456789abcdef0\",\"instance_type\":\"m9g.xlarge\",\"last_attempt_at\":\"2026-11-24T20:00:00Z\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-0fedcba9876543210\",\"last_attempt_at\":\"2026-11-24T20:00:01Z\"},{\"project\":\"example-two\",\"stack\":\"agentspace-host-example-two\",\"state\":\"failed\",\"instance_id\":\"i-0123456789abcdef1\",\"instance_type\":\"m7i-flex.large\",\"last_attempt_at\":\"2026-11-24T19:10:00Z\"}]}\n";
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
}

#[test]
fn a_loosely_written_file_with_types_is_written_back_compact_in_the_tables_order() {
    // A byte order mark, whitespace, CRLF and members in another order are read (D20), and the
    // write puts the member where the table says: one LF, no BOM, no whitespace.
    let t = TempHome::new();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(
        b"{ \"hosts\" : [ {\"instance_type\" : \"m9g.large\", \"last_attempt_at\":\"2026-11-24T19:05:42Z\",\r\n \"state\":\"ready\",\"stack\":\"agentspace-host-agentspace\",\"project\":\"agentspace\",\"instance_id\":\"i-1234abcd\"},\r\n {\"state\":\"ready\",\"stack\":\"agentbox\",\"project\":\"swimcoach\"} ],\r\n \"version\": 0 }\r\n\r\n",
    );
    t.put_hosts(bytes);
    let guard = lock(t.path()).unwrap();
    let h = read(t.path()).unwrap();
    write(t.path(), &h, &guard).unwrap();
    drop(guard);
    // Records are kept in the order found (a reader keeps the order it finds).
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        "{\"version\":0,\"hosts\":[{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-1234abcd\",\"instance_type\":\"m9g.large\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\"},{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n"
    );
}

#[test]
fn an_older_file_without_types_is_written_back_as_it_was_before_v0_09() {
    let t = TempHome::new();
    let bytes = format!("{}\n", SAMPLE_FULL.replace(['\r', '\n', ' '], ""));
    t.put_hosts(SAMPLE_FULL);
    let guard = lock(t.path()).unwrap();
    let h = read(t.path()).unwrap();
    write(t.path(), &h, &guard).unwrap();
    drop(guard);
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        // The sample's strings hold no spaces, so stripping its whitespace gives the compact form.
        bytes
    );
}

// --- A value that makes the file not valid ----------------------------------------------------

#[test]
fn a_type_that_is_not_a_string_makes_the_file_not_valid() {
    for value in [
        "4",
        "4.5",
        "0",
        "true",
        "false",
        "null",
        "[]",
        "[\"m9g.xlarge\"]",
        "{}",
        "{\"m9g\":\"xlarge\"}",
    ] {
        assert_refused(&format!("instance_type {value}"), doc_with_type(value));
    }
}

#[test]
fn null_is_never_accepted_for_the_type_on_any_record() {
    assert_refused(
        "null on the box",
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_type\":null}]}",
    );
}

#[test]
fn a_type_that_breaks_the_pattern_makes_the_file_not_valid() {
    for ty in [
        "M9G.xlarge",
        "m9g.XLARGE",
        "M9G.XLARGE",
        "m9gxlarge",
        "xlarge",
        ".xlarge",
        "m9g.",
        ".",
        "",
        "-m9g.xlarge",
        "m9g.-xlarge",
        "m9g..xlarge",
        "m9g.x.large",
        "m9g.xlarge ",
        " m9g.xlarge",
        "m9g .xlarge",
        "m9g_x.large",
        "m9g.xlarge\\n",
        "m9g.xlarge\\r",
        "m9g\\n.xlarge",
        "m9g.xlarge\\u0000",
        "m\u{e9}g.xlarge",
    ] {
        assert_refused(
            &format!("instance_type {ty:?}"),
            doc_with_type(&format!("\"{ty}\"")),
        );
    }
}

#[test]
fn a_type_that_ends_in_a_line_feed_is_not_valid() {
    // The pattern ends in $(?!\n): a bare $ would let a trailing line feed through.
    let doc = doc_with_type("\"m9g.xlarge\\n\"");
    assert!(
        doc.contains("\\n\""),
        "The test's own document must hold the escape"
    );
    assert_refused("instance_type with a trailing line feed", doc);
}

#[test]
fn a_type_over_sixty_four_characters_makes_the_file_not_valid() {
    let over = format!("{}.{}", "a".repeat(32), "b".repeat(32)); // 65 characters
    assert_eq!(over.len(), 65);
    assert_refused("65 characters", doc_with_type(&format!("\"{over}\"")));
    let way_over = format!("m9g.{}", "a".repeat(200));
    assert_refused("204 characters", doc_with_type(&format!("\"{way_over}\"")));
}

#[test]
fn a_type_named_twice_makes_the_file_not_valid() {
    assert_refused(
        "instance_type twice",
        doc_with(
            "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_type\":\"m9g.xlarge\",\"instance_type\":\"m9g.xlarge\"}",
        ),
    );
}

#[test]
fn an_invalid_record_in_the_box_makes_the_file_not_valid_too() {
    assert_refused(
        "bad type on the box",
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_type\":\"M9G.XLARGE\"}]}",
    );
}

#[test]
fn every_invalid_type_sample_is_refused() {
    for (label, text) in INVALID_TYPE_SAMPLES {
        assert_refused(label, text);
    }
}

#[test]
fn a_file_that_is_not_valid_by_its_type_is_never_read_as_the_seed_or_its_default() {
    // The file holds a host that a rebuild from the seed would lose.
    let t = TempHome::new();
    t.put_hosts(doc_with_type("4"));
    let e = read(t.path()).expect_err("A bad type must not be read as the seed");
    assert!(is_not_valid(&e), "{e:?}");
}

// --- The writer refuses a type that would not be valid ----------------------------------------

#[test]
fn a_change_that_would_leave_a_bad_type_is_not_written() {
    let t = TempHome::new();
    write_new(&t, &with_box(vec![host("agentspace", HostState::Ready)])).unwrap();
    let before = t.hosts_bytes();
    let over = format!("{}.{}", "a".repeat(32), "b".repeat(32));
    for ty in [
        "M9G.xlarge",
        "m9gxlarge",
        ".xlarge",
        "",
        "m9g.xlarge\n",
        "m9g.x.large",
        over.as_str(),
    ] {
        let h = with_box(vec![typed(host("agentspace", HostState::Ready), ty)]);
        let e = write_new(&t, &h).expect_err("A type that is not valid must not be written");
        assert!(is_write_refusal(&e), "{ty:?}: {e:?}");
        assert_message(&e, t.path());
        assert_eq!(t.hosts_bytes(), before, "{ty:?}: the old file must stay");
    }
    assert_eq!(
        TempHome::names_in(&t.state()),
        ["hosts.json", "hosts.lock"],
        "No temporary file is left"
    );
}

#[test]
fn a_refused_first_write_with_a_bad_type_creates_no_registry() {
    let t = TempHome::new();
    let h = with_box(vec![typed(
        host("agentspace", HostState::Ready),
        "NOT.valid",
    )]);
    let e = write_new(&t, &h).expect_err("A type that is not valid must not be written");
    assert!(is_write_refusal(&e), "{e:?}");
    assert!(!t.hosts_json().exists());
}

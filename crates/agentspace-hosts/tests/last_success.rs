//! A record's `last_success_at` and its setter (docs/state.md, "The host registry", the member
//! table and the bullets under it; "The last success"; docs/decisions.md D77 and D81;
//! E0.10-T02-S03, card #0216): reading it, writing it back byte for byte, the values that make a
//! file not valid, and `set_last_success_at`, which sets one record's member under `hosts.lock`,
//! re-reading the file under the lock, and changes nothing else.
//!
//! The setter is D81's `agentspace_hosts::set_last_success_at(home: &Path, project: &str, at:
//! &str) -> Result<(), HostsError>`, which takes and releases the host registry lock itself, as
//! `set_instance_type` does. Every call of it in this file goes through `set_success` below.

mod common;

use agentspace_hosts::{Host, HostState, Hosts, HostsError, lock, read, try_lock, write};
use common::*;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// The one call of the setter.
fn set_success(home: &Path, project: &str, at: &str) -> Result<(), HostsError> {
    agentspace_hosts::set_last_success_at(home, project, at)
}

const SAMPLE_LAST_SUCCESS: &str =
    include_str!("../../../schemas/state/samples/hosts.state.last-success.json");
const SAMPLE_OFFSET: &str = include_str!(
    "../../../schemas/state/samples/invalid/hosts.state.last-success-with-offset.json"
);
const SAMPLE_NULL: &str =
    include_str!("../../../schemas/state/samples/invalid/hosts.state.null-last-success.json");

/// Four records of every shape the setter MUST leave alone, in the member table's order: the box
/// with a success; a ready host whose success is at its attempt's second; a creating host with an
/// attempt and no success; a failed host with a type, an attempt and no success.
const FILE: &str = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-0123456789abcdef0\",\"last_attempt_at\":\"2026-11-24T18:30:00Z\",\"last_success_at\":\"2026-11-24T18:41:07Z\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.large\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\",\"last_success_at\":\"2026-11-24T19:05:42Z\"},{\"project\":\"example-two\",\"stack\":\"agentspace-host-example-two\",\"state\":\"creating\",\"last_attempt_at\":\"2026-11-24T19:06:00Z\"},{\"project\":\"failed-one\",\"stack\":\"agentspace-host-failed-one\",\"state\":\"failed\",\"instance_id\":\"i-0123456789abcdef1\",\"instance_type\":\"c7g.large\",\"last_attempt_at\":\"2026-11-24T19:10:00Z\"}]}\n";

const SEED_BYTES: &str = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n";

fn put_file() -> TempHome {
    let t = TempHome::new();
    t.put_hosts(FILE);
    t
}

fn file_text(t: &TempHome) -> String {
    String::from_utf8(t.hosts_bytes()).unwrap()
}

/// The names of the folder's entries are the registry and its lock alone: no temporary file.
fn assert_nothing_left_over(t: &TempHome) {
    assert_eq!(
        TempHome::names_in(&t.state()),
        ["hosts.json", "hosts.lock"],
        "Only the registry and its lock are in the folder"
    );
}

/// The lock is free again: a setter never leaves it held, after a success or after a refusal.
fn assert_lock_is_free(t: &TempHome) {
    assert!(
        free_within(FREE_WITHIN, || try_lock(t.path())
            .expect("The lock must be takeable")
            .is_some()),
        "The setter left the host registry lock held"
    );
}

fn read_text(text: &str) -> Result<Hosts, HostsError> {
    let t = TempHome::new();
    t.put_hosts(text);
    read(t.path())
}

/// The file is read as not valid, and nothing in agentspace repairs, rewrites or removes it.
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

/// A document of the box's record and one more, whose `last_success_at` member is `member`: the
/// JSON text of its value, such as `"2026-11-24T18:41:07Z"` with its quotes, or `4`.
fn doc_with_success(member: &str) -> String {
    doc_with(&format!(
        "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"last_attempt_at\":\"2026-11-24T18:30:00Z\",\"last_success_at\":{member}}}"
    ))
}

fn write_new(t: &TempHome, h: &Hosts) -> Result<(), HostsError> {
    let guard = lock(t.path())?;
    write(t.path(), h, &guard)
}

// --- Reading: a record with the member, and one without it ------------------------------------

#[test]
fn a_record_with_a_last_success_reads_it_as_the_string_stored() {
    let h = read_text(&doc_with_success("\"2026-11-24T18:41:07Z\"")).unwrap();
    assert_eq!(h.hosts.len(), 2);
    assert_eq!(
        h.hosts[1].last_success_at.as_deref(),
        Some("2026-11-24T18:41:07Z")
    );
    // The box's record in the same file has no member and reads without one.
    assert_eq!(h.hosts[0].last_success_at, None);
}

#[test]
fn a_record_without_a_last_success_reads_none_and_a_written_before_v0_10_file_is_valid() {
    // state.md, the member table: optional, "A record written before v0.10 ... is valid".
    let h = read_text(&doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"last_attempt_at\":\"2026-11-24T18:30:00Z\"}",
    ))
    .unwrap();
    assert_eq!(h.hosts[1].last_success_at, None);
    assert_eq!(
        h.hosts[1].last_attempt_at.as_deref(),
        Some("2026-11-24T18:30:00Z")
    );
}

#[test]
fn the_member_is_read_in_a_record_of_each_state_and_with_no_attempt_beside_it() {
    for (text, state) in [
        ("creating", HostState::Creating),
        ("ready", HostState::Ready),
        ("failed", HostState::Failed),
    ] {
        let doc = doc_with(&format!(
            "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"{text}\",\"last_success_at\":\"2026-11-24T18:41:07Z\"}}"
        ));
        let h = read_text(&doc).unwrap();
        assert_eq!(h.hosts[1].state, state, "{text}");
        assert_eq!(
            h.hosts[1].last_success_at.as_deref(),
            Some("2026-11-24T18:41:07Z"),
            "{text}"
        );
        assert_eq!(h.hosts[1].last_attempt_at, None, "{text}");
    }
}

#[test]
fn a_success_before_the_attempt_is_read_as_stored_and_never_repaired() {
    // state.md: "before last_attempt_at, which only a person's edit or a clock set back during the
    // run can make": the file is valid, and the reader keeps both strings as stored.
    let h = read_text(&doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"last_attempt_at\":\"2026-11-24T19:00:00Z\",\"last_success_at\":\"2026-11-24T18:00:00Z\"}",
    ))
    .unwrap();
    assert_eq!(
        h.hosts[1].last_attempt_at.as_deref(),
        Some("2026-11-24T19:00:00Z")
    );
    assert_eq!(
        h.hosts[1].last_success_at.as_deref(),
        Some("2026-11-24T18:00:00Z")
    );
}

#[test]
fn a_reader_does_not_depend_on_member_order_for_the_success() {
    let doc = "{\"hosts\":[{\"last_success_at\":\"2026-11-24T19:30:00Z\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\",\"instance_id\":\"i-1234567890abcdef0\",\"state\":\"ready\",\"stack\":\"agentbox\",\"project\":\"swimcoach\"}],\"version\":0}";
    let h = read_text(doc).unwrap();
    assert_eq!(
        h.hosts[0],
        Host {
            instance_id: Some("i-1234567890abcdef0".to_string()),
            last_attempt_at: Some("2026-11-24T19:05:42Z".to_string()),
            last_success_at: Some("2026-11-24T19:30:00Z".to_string()),
            ..box_host()
        }
    );
}

#[test]
fn the_sample_with_successes_is_read_with_them() {
    let h = read_text(SAMPLE_LAST_SUCCESS).unwrap();
    let successes: Vec<Option<&str>> = h
        .hosts
        .iter()
        .map(|r| r.last_success_at.as_deref())
        .collect();
    assert_eq!(
        successes,
        [
            Some("2026-11-24T18:41:07Z"),
            Some("2026-11-24T19:05:42Z"),
            None,
            Some("2026-11-24T18:55:30Z"),
        ]
    );
    // The third record, "example-three", has a success before its attempt: valid, as stored.
    assert_eq!(
        h.hosts[3].last_attempt_at.as_deref(),
        Some("2026-11-24T19:20:00Z")
    );
}

// --- Reading: the values that make a file not valid -------------------------------------------

#[test]
fn a_success_of_the_wrong_form_makes_the_file_not_valid() {
    // The member's form is `last_attempt_at`'s: `YYYY-MM-DDTHH:MM:SSZ`, UTC (state.md, the table).
    for (label, member) in [
        ("offset", "\"2026-11-24T18:41:07+00:00\""),
        ("lowercase z", "\"2026-11-24T18:41:07z\""),
        ("no zone", "\"2026-11-24T18:41:07\""),
        ("fraction", "\"2026-11-24T18:41:07.5Z\""),
        ("date only", "\"2026-11-24\""),
        ("empty", "\"\""),
        ("trailing line feed", "\"2026-11-24T18:41:07Z\\n\""),
        ("number", "20261124184107"),
        ("true", "true"),
        ("array", "[]"),
        ("object", "{}"),
        ("null", "null"),
    ] {
        assert_refused(label, doc_with_success(member));
    }
}

#[test]
fn the_invalid_samples_are_refused() {
    assert_refused("offset sample", SAMPLE_OFFSET);
    assert_refused("null sample", SAMPLE_NULL);
}

#[test]
fn a_success_that_names_no_real_instant_makes_the_file_not_valid() {
    // state.md: "A time that matches the pattern but names no real instant, such as
    // 2026-02-30T00:00:00Z, makes the file not valid; so does a leap second, :60", for each of
    // `last_attempt_at` and `last_success_at`.
    for (label, member) in [
        ("30 February", "\"2026-02-30T00:00:00Z\""),
        ("hour 24", "\"2026-11-24T24:00:00Z\""),
        ("leap second", "\"2026-12-31T23:59:60Z\""),
        ("month 13", "\"2026-13-01T00:00:00Z\""),
    ] {
        assert_refused(label, doc_with_success(member));
    }
}

#[test]
fn a_repeated_success_member_makes_the_file_not_valid() {
    assert_refused(
        "repeated member",
        doc_with(
            "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"last_success_at\":\"2026-11-24T18:41:07Z\",\"last_success_at\":\"2026-11-24T18:41:08Z\"}",
        ),
    );
}

// --- Writing it back --------------------------------------------------------------------------

#[test]
fn a_file_with_successes_is_written_back_byte_for_byte() {
    // state.md, "Its form": the member is written after `last_attempt_at`, the last, in the
    // table's order; a record without it is written as it was before v0.10.
    let t = TempHome::new();
    t.put_hosts(FILE);
    let h = read(t.path()).unwrap();
    let again = TempHome::new();
    write_new(&again, &h).unwrap();
    assert_eq!(again.hosts_bytes(), FILE.as_bytes());
    assert_eq!(read(again.path()).unwrap(), h);
}

#[test]
fn the_member_is_written_last_whatever_order_a_record_was_built_in() {
    let t = TempHome::new();
    let h = with_box(vec![Host {
        last_success_at: Some("2026-11-24T19:30:00Z".to_string()),
        instance_type: Some("m9g.large".to_string()),
        ..full_host(
            "agentspace",
            HostState::Ready,
            "i-0fedcba9876543210",
            "2026-11-24T19:05:42Z",
        )
    }]);
    write_new(&t, &h).unwrap();
    let text = file_text(&t);
    assert!(
        text.contains(
            "\"instance_type\":\"m9g.large\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\",\"last_success_at\":\"2026-11-24T19:30:00Z\"}"
        ),
        "The member follows last_attempt_at, the last of the table: {text}"
    );
    assert!(
        !text.contains("null"),
        "A member that is absent is left out"
    );
}

#[test]
fn a_record_without_the_member_is_written_without_it() {
    let t = TempHome::new();
    write_new(&t, &with_box(vec![host("agentspace", HostState::Ready)])).unwrap();
    assert!(!file_text(&t).contains("last_success_at"));
}

#[test]
fn a_change_whose_success_is_not_valid_is_refused_and_writes_nothing() {
    let t = put_file();
    let mut h = read(t.path()).unwrap();
    for bad in [
        "2026-11-24T18:41:07+00:00",
        "2026-02-30T00:00:00Z",
        "yesterday",
        "",
    ] {
        h.hosts[2].last_success_at = Some(bad.to_string());
        let e = write_new(&t, &h).expect_err("A success that is not valid must not be written");
        assert!(is_change_not_valid(&e), "{bad:?}: {e:?}");
        assert_message(&e, t.path());
        assert_eq!(
            t.hosts_bytes(),
            FILE.as_bytes(),
            "{bad:?}: the old file must stay"
        );
    }
}

// --- The setter: it changes that record's member and nothing else -----------------------------

#[test]
fn it_sets_a_success_on_a_record_that_has_none_and_every_other_byte_is_as_read() {
    let t = put_file();
    set_success(t.path(), "example-two", "2026-11-24T19:12:30Z").unwrap();
    let expected = FILE.replace(
        "\"state\":\"creating\",\"last_attempt_at\":\"2026-11-24T19:06:00Z\"}",
        "\"state\":\"creating\",\"last_attempt_at\":\"2026-11-24T19:06:00Z\",\"last_success_at\":\"2026-11-24T19:12:30Z\"}",
    );
    assert_ne!(expected, FILE, "The test's own replacement must hit");
    assert_eq!(file_text(&t), expected);
    assert_nothing_left_over(&t);
    assert_eq!(
        read(t.path()).unwrap().hosts[2].last_success_at.as_deref(),
        Some("2026-11-24T19:12:30Z")
    );
}

#[test]
fn it_sets_a_success_on_a_failed_record_with_a_type_and_keeps_both() {
    let t = put_file();
    set_success(t.path(), "failed-one", "2026-11-24T19:20:00Z").unwrap();
    let expected = FILE.replace(
        "\"last_attempt_at\":\"2026-11-24T19:10:00Z\"}",
        "\"last_attempt_at\":\"2026-11-24T19:10:00Z\",\"last_success_at\":\"2026-11-24T19:20:00Z\"}",
    );
    assert_ne!(expected, FILE);
    assert_eq!(file_text(&t), expected);
    let after = read(t.path()).unwrap();
    assert_eq!(
        after.hosts[3].state,
        HostState::Failed,
        "The state is not the setter's"
    );
    assert_eq!(after.hosts[3].instance_type.as_deref(), Some("c7g.large"));
}

#[test]
fn it_replaces_a_success_the_record_holds() {
    let t = put_file();
    set_success(t.path(), "swimcoach", "2026-11-24T20:00:00Z").unwrap();
    let expected = FILE.replace("\"2026-11-24T18:41:07Z\"", "\"2026-11-24T20:00:00Z\"");
    assert_ne!(expected, FILE);
    assert_eq!(file_text(&t), expected);
}

#[test]
fn it_writes_the_time_it_is_given_as_a_string_and_does_not_touch_the_attempt() {
    // The caller's clock is the time (state.md, "Who writes it", item 3); the setter keeps the
    // string as given, and never writes `last_attempt_at`.
    let t = put_file();
    set_success(t.path(), "agentspace", "2026-11-24T19:05:43Z").unwrap();
    let after = read(t.path()).unwrap();
    assert_eq!(
        after.hosts[1].last_success_at.as_deref(),
        Some("2026-11-24T19:05:43Z")
    );
    assert_eq!(
        after.hosts[1].last_attempt_at.as_deref(),
        Some("2026-11-24T19:05:42Z")
    );
}

#[test]
fn setting_the_success_a_record_already_holds_leaves_the_file_as_it_was() {
    let t = put_file();
    set_success(t.path(), "agentspace", "2026-11-24T19:05:42Z").unwrap();
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

#[test]
fn it_changes_no_other_record_and_no_other_member_of_the_one_it_changes() {
    let t = put_file();
    let before = read(t.path()).unwrap();
    for (i, project) in ["swimcoach", "agentspace", "example-two", "failed-one"]
        .iter()
        .enumerate()
    {
        set_success(t.path(), project, "2026-11-24T22:00:00Z").unwrap();
        let after = read(t.path()).unwrap();
        assert_eq!(after.hosts.len(), 4);
        for (j, (b, a)) in before.hosts.iter().zip(after.hosts.iter()).enumerate() {
            if j <= i {
                assert_eq!(
                    a.last_success_at.as_deref(),
                    Some("2026-11-24T22:00:00Z"),
                    "{project}: record {j}"
                );
                assert_eq!(
                    Host {
                        last_success_at: None,
                        ..a.clone()
                    },
                    Host {
                        last_success_at: None,
                        ..b.clone()
                    },
                    "{project}: record {j} changed in a member other than last_success_at"
                );
            } else {
                assert_eq!(a, b, "{project}: record {j} must be as it was");
            }
        }
    }
    assert_nothing_left_over(&t);
}

#[test]
fn it_writes_a_loosely_written_file_back_compact_with_the_one_change() {
    let t = TempHome::new();
    t.put_hosts(
        "\u{feff}{\n  \"version\": 0,\n  \"hosts\": [\n    {\"project\": \"swimcoach\", \"stack\": \"agentbox\", \"state\": \"ready\"},\r\n    {\"project\": \"agentspace\", \"stack\": \"agentspace-host-agentspace\", \"state\": \"ready\", \"last_attempt_at\": \"2026-11-24T19:05:42Z\"}\n  ]\n}\n",
    );
    set_success(t.path(), "agentspace", "2026-11-24T19:30:00Z").unwrap();
    assert_eq!(
        file_text(&t),
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\",\"last_success_at\":\"2026-11-24T19:30:00Z\"}]}\n"
    );
}

#[test]
fn it_works_in_a_home_whose_folder_already_exists_without_the_file() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state()).unwrap();
    set_success(t.path(), "swimcoach", "2026-11-24T19:30:00Z").unwrap();
    assert_eq!(
        file_text(&t),
        SEED_BYTES.replace(
            "\"state\":\"ready\"}",
            "\"state\":\"ready\",\"last_success_at\":\"2026-11-24T19:30:00Z\"}"
        )
    );
}

// --- The setter: a time that is not valid is refused ------------------------------------------

#[test]
fn it_refuses_a_time_that_would_not_be_valid_and_writes_nothing() {
    // state.md: "A change that would not be valid is not written." (the member's form and the
    // real-instant rule).
    let t = put_file();
    for at in [
        "",
        "yesterday",
        "2026-11-24T18:41:07+00:00",
        "2026-11-24T18:41:07",
        "2026-11-24T18:41:07z",
        "2026-11-24T18:41:07.5Z",
        "2026-11-24T18:41:07Z\n",
        " 2026-11-24T18:41:07Z",
        "2026-02-30T00:00:00Z",
        "2026-12-31T23:59:60Z",
    ] {
        let e = set_success(t.path(), "example-two", at)
            .expect_err("A time that is not valid must not be written");
        assert!(is_change_not_valid(&e), "{at:?}: {e:?}");
        assert_message(&e, t.path());
        assert_eq!(
            t.hosts_bytes(),
            FILE.as_bytes(),
            "{at:?}: the old file must stay"
        );
    }
    assert_nothing_left_over(&t);
    assert_lock_is_free(&t);
}

#[test]
fn it_refuses_a_bad_time_when_there_is_no_file_and_creates_no_registry() {
    let t = TempHome::new();
    let e = set_success(t.path(), "swimcoach", "not a time")
        .expect_err("A time that is not valid must not be written");
    assert_message(&e, t.path());
    assert!(!t.hosts_json().exists());
}

// --- The setter: a project with no record -----------------------------------------------------

#[test]
fn it_refuses_a_project_with_no_record_and_writes_nothing() {
    // state.md, "The last success", item 4: "a record no longer there is such a failure". The
    // refusal is `Unwritable(NoSuchRecord)` as `set_instance_type`'s is; nothing is written.
    let t = put_file();
    let e = set_success(t.path(), "no-such-project", "2026-11-24T19:30:00Z")
        .expect_err("A project with no record has no success to record");
    assert_message(&e, t.path());
    assert!(
        matches!(
            e,
            HostsError::Unwritable(agentspace_hosts::Failure::NoSuchRecord)
        ),
        "{e:?}"
    );
    assert_eq!(
        t.hosts_bytes(),
        FILE.as_bytes(),
        "The file must stay as it was"
    );
    assert_nothing_left_over(&t);
    assert_lock_is_free(&t);
}

#[test]
fn it_refuses_a_project_with_no_record_when_there_is_no_file_too_and_creates_none() {
    let t = TempHome::new();
    let e = set_success(t.path(), "agentspace", "2026-11-24T19:30:00Z")
        .expect_err("A project with no record has no success to record");
    assert_message(&e, t.path());
    assert!(
        !t.hosts_json().exists(),
        "A refusal writes no file, not even the seed's"
    );
    assert_lock_is_free(&t);
}

#[test]
fn it_never_adds_a_record() {
    let t = put_file();
    let _ = set_success(t.path(), "brand-new", "2026-11-24T19:30:00Z");
    assert_eq!(read(t.path()).unwrap().hosts.len(), 4);
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

#[test]
fn a_project_name_is_compared_as_the_normalized_string_stored() {
    let t = put_file();
    assert!(set_success(t.path(), "Agentspace", "2026-11-24T19:30:00Z").is_err());
    assert!(set_success(t.path(), "", "2026-11-24T19:30:00Z").is_err());
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

// --- The setter: a file that is not valid, or something that is not a file --------------------

#[test]
fn it_refuses_a_file_that_is_not_valid_and_repairs_nothing() {
    // state.md, "The last success", item 4: "A file that has become not valid ... is such a
    // failure."
    for bytes in [
        "not json".to_string(),
        FILE.replace("\"2026-11-24T19:10:00Z\"", "\"2026-02-30T00:00:00Z\""),
        FILE.replace(
            "\"last_success_at\":\"2026-11-24T18:41:07Z\"",
            "\"last_success_at\":null",
        ),
        "{\"version\":0,\"hosts\":[]}".to_string(),
    ] {
        let t = TempHome::new();
        t.put_hosts(&bytes);
        let e = set_success(t.path(), "agentspace", "2026-11-24T19:30:00Z")
            .expect_err("A file that is not valid is an error, never the seed");
        assert!(is_file_problem(&e), "{e:?}");
        assert_message(&e, t.path());
        assert_eq!(
            t.hosts_bytes(),
            bytes.as_bytes(),
            "Nothing in agentspace repairs, rewrites or removes a file that is not valid"
        );
        assert_lock_is_free(&t);
    }
}

#[test]
fn it_refuses_a_folder_at_the_registry_and_changes_nothing() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.hosts_json()).unwrap();
    let e = set_success(t.path(), "swimcoach", "2026-11-24T19:30:00Z")
        .expect_err("A folder is not a missing file");
    assert_message(&e, t.path());
    assert!(t.hosts_json().is_dir());
    assert!(TempHome::names_in(&t.hosts_json()).is_empty());
}

#[test]
fn a_folder_at_the_locks_path_fails_at_once_and_is_never_reported_busy() {
    let t = put_file();
    std::fs::create_dir_all(t.state().join("hosts.lock")).unwrap();
    let started = std::time::Instant::now();
    let e = set_success(t.path(), "agentspace", "2026-11-24T19:30:00Z")
        .expect_err("A folder is not a lock");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "A failure other than the lock being held must not be waited for"
    );
    assert!(is_cannot_lock(&e), "{e:?}");
    assert_message(&e, t.path());
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

// --- The setter: no file, the seed's record ---------------------------------------------------

#[test]
fn with_no_file_the_boxs_success_is_written_on_the_seed_record_and_never_an_implied_type() {
    // state.md, "The seed": a writer that finds no file starts from the seed document, whose
    // record has no `instance_type`; the implied `m9g.xlarge` is never written. (N1: the setter
    // follows `set_instance_type`'s way with no file, as D81 says it does under the lock.)
    let t = TempHome::new();
    set_success(t.path(), "swimcoach", "2026-11-24T19:30:00Z").unwrap();
    let text = file_text(&t);
    assert_eq!(
        text,
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"last_success_at\":\"2026-11-24T19:30:00Z\"}]}\n"
    );
    assert!(!text.contains("instance_type"));
    assert_nothing_left_over(&t);
}

// --- Under hosts.lock -------------------------------------------------------------------------

#[test]
fn it_takes_the_lock_and_lets_it_go() {
    let t = put_file();
    set_success(t.path(), "agentspace", "2026-11-24T19:30:00Z").unwrap();
    assert!(t.state().join("hosts.lock").is_file());
    assert_eq!(
        std::fs::metadata(t.state().join("hosts.lock"))
            .unwrap()
            .len(),
        0,
        "The lock's content is empty and meaningless"
    );
    assert_lock_is_free(&t);
}

#[test]
fn it_waits_for_a_held_lock_and_writes_only_once_it_has_it() {
    // A setter on another thread must still be waiting a while after it started, must not have
    // written, and must finish once the holder lets go. The first check can only pass late on a
    // slow machine, never fail early on one.
    let t = put_file();
    let held = lock(t.path()).unwrap();
    let home = t.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let setter = std::thread::spawn(move || {
        let r = set_success(&home, "agentspace", "2026-11-24T19:30:00Z");
        tx.send(r.is_ok()).unwrap();
    });
    std::thread::sleep(Duration::from_millis(1000));
    assert!(
        rx.try_recv().is_err(),
        "The setter finished while another holder had the lock: it did not take it"
    );
    assert_eq!(
        t.hosts_bytes(),
        FILE.as_bytes(),
        "The setter wrote before it held the lock"
    );
    drop(held);
    assert!(
        rx.recv_timeout(Duration::from_secs(20))
            .expect("The setter must finish once the lock is let go"),
        "The setter must succeed once it has the lock"
    );
    setter.join().unwrap();
    assert_eq!(
        read(t.path()).unwrap().hosts[1].last_success_at.as_deref(),
        Some("2026-11-24T19:30:00Z")
    );
}

#[test]
fn it_reads_the_file_again_under_the_lock_and_keeps_a_change_made_while_it_waited() {
    // The driver's change at a run's start, which removes the success, is made while the setter
    // waits; the setter re-reads under the lock, so the other record's change is kept and its own
    // lands too: no lost update.
    let t = put_file();
    let held = lock(t.path()).unwrap();
    let home = t.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let setter = std::thread::spawn(move || {
        tx.send(set_success(&home, "agentspace", "2026-11-24T19:30:00Z").is_ok())
            .unwrap();
    });
    std::thread::sleep(Duration::from_millis(500));
    let mut h = read(t.path()).unwrap();
    h.hosts
        .push(host("added-while-waiting", HostState::Creating));
    h.hosts[0].last_attempt_at = Some("2026-11-24T21:00:00Z".to_string());
    h.hosts[0].last_success_at = None;
    write(t.path(), &h, &held).unwrap();
    assert!(rx.try_recv().is_err(), "The setter must still be waiting");
    drop(held);
    assert!(rx.recv_timeout(Duration::from_secs(20)).unwrap());
    setter.join().unwrap();
    let after = read(t.path()).unwrap();
    assert_eq!(after.hosts.len(), 5, "The record the holder added is kept");
    assert_eq!(after.hosts[4].project, "added-while-waiting");
    assert_eq!(
        after.hosts[0].last_attempt_at.as_deref(),
        Some("2026-11-24T21:00:00Z"),
        "The time the holder wrote is kept"
    );
    assert_eq!(
        after.hosts[0].last_success_at, None,
        "The removal the holder wrote is kept"
    );
    assert_eq!(
        after.hosts[1].last_success_at.as_deref(),
        Some("2026-11-24T19:30:00Z")
    );
}

#[test]
fn a_record_added_while_the_setter_waits_is_found_by_its_read_under_the_lock() {
    let t = put_file();
    let held = lock(t.path()).unwrap();
    let home = t.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let setter = std::thread::spawn(move || {
        tx.send(set_success(&home, "later-project", "2026-11-24T19:30:00Z").is_ok())
            .unwrap();
    });
    std::thread::sleep(Duration::from_millis(500));
    let mut h = read(t.path()).unwrap();
    h.hosts.push(host("later-project", HostState::Creating));
    write(t.path(), &h, &held).unwrap();
    drop(held);
    assert!(
        rx.recv_timeout(Duration::from_secs(20)).unwrap(),
        "The setter read the file before it held the lock, so it missed the record the holder added"
    );
    setter.join().unwrap();
    let after = read(t.path()).unwrap();
    assert_eq!(after.hosts[4].project, "later-project");
    assert_eq!(
        after.hosts[4].last_success_at.as_deref(),
        Some("2026-11-24T19:30:00Z")
    );
}

#[test]
fn setters_of_different_records_at_once_all_land_and_so_do_other_setters() {
    let t = TempHome::new();
    let mut records = Vec::new();
    for i in 0..8 {
        records.push(host(&format!("project-{i}"), HostState::Ready));
    }
    {
        let guard = lock(t.path()).unwrap();
        write(t.path(), &with_box(records), &guard).unwrap();
    }
    let mut threads = Vec::new();
    for i in 0..8 {
        let home = t.path().to_path_buf();
        threads.push(std::thread::spawn(move || {
            set_success(
                &home,
                &format!("project-{i}"),
                &format!("2026-11-24T19:3{i}:00Z"),
            )
            .unwrap();
            // The member's neighbour, written by another setter of the same crate under the same
            // lock, is not lost to this one's write.
            agentspace_hosts::set_instance_type(&home, &format!("project-{i}"), Some("m9g.large"))
                .unwrap();
        }));
    }
    for th in threads {
        th.join().unwrap();
    }
    let after: Hosts = read(t.path()).unwrap();
    assert_eq!(after.hosts.len(), 9);
    assert_eq!(after.hosts[0].last_success_at, None);
    for i in 0..8 {
        assert_eq!(
            after.hosts[i + 1].last_success_at.as_deref(),
            Some(format!("2026-11-24T19:3{i}:00Z").as_str()),
            "project-{i}: a concurrent setter lost its success"
        );
        assert_eq!(
            after.hosts[i + 1].instance_type.as_deref(),
            Some("m9g.large"),
            "project-{i}: a concurrent setter lost its type"
        );
    }
    assert_nothing_left_over(&t);
    assert_lock_is_free(&t);
}

#[test]
fn a_reader_is_never_blocked_by_a_setter_and_sees_the_old_file_or_the_new_one() {
    let t = put_file();
    let home = t.path().to_path_buf();
    let setter = std::thread::spawn(move || {
        for i in 0..20 {
            set_success(
                &home,
                "example-two",
                &format!("2026-11-24T19:{:02}:00Z", 10 + i),
            )
            .unwrap();
        }
    });
    while !setter.is_finished() {
        let h = read(t.path()).expect("A reader sees a whole file, never a partial one");
        assert_eq!(h.hosts.len(), 4);
    }
    setter.join().unwrap();
    assert_eq!(
        read(t.path()).unwrap().hosts[2].last_success_at.as_deref(),
        Some("2026-11-24T19:29:00Z")
    );
}

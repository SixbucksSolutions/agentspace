//! Reading the host registry (docs/state.md, The host registry): every form the file takes, the
//! seed, the tolerant read, and every file that is not valid.

mod common;

use agentspace_hosts::{BOX_PROJECT, BOX_STACK, Host, HostState, Hosts, read};
use common::*;

const SAMPLE_FULL: &str = include_str!("../../../schemas/state/samples/hosts.state.full.json");
const SAMPLE_CREATING_AND_FAILED: &str =
    include_str!("../../../schemas/state/samples/hosts.state.creating-and-failed.json");
const SAMPLE_SEED: &str = include_str!("../../../schemas/state/samples/hosts.state.seed.json");

const INVALID_SAMPLES: &[(&str, &str)] = &[
    (
        "bad-instance-id",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.bad-instance-id.json"),
    ),
    (
        "bare-array",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.bare-array.json"),
    ),
    (
        "extra-member",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.extra-member.json"),
    ),
    (
        "extra-top-level-member",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.extra-top-level-member.json"
        ),
    ),
    (
        "missing-stack",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.missing-stack.json"),
    ),
    (
        "missing-state",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.missing-state.json"),
    ),
    (
        "null-instance-id",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.null-instance-id.json"),
    ),
    (
        "stack-of-other-name",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.stack-of-other-name.json"),
    ),
    (
        "time-with-offset",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.time-with-offset.json"),
    ),
    (
        "unknown-state",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.unknown-state.json"),
    ),
    (
        "uppercase-project",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.uppercase-project.json"),
    ),
    (
        "wrong-version",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.wrong-version.json"),
    ),
];

fn read_text(text: &str) -> Result<Hosts, agentspace_hosts::HostsError> {
    let t = TempHome::new();
    t.put_hosts(text);
    read(t.path())
}

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

// --- The seed ---------------------------------------------------------------------------------

#[test]
fn a_missing_file_reads_as_the_boxs_record_alone() {
    let t = TempHome::new();
    let h = read(t.path()).expect("A missing file is the seed");
    assert_eq!(h.hosts.len(), 1);
    let b = &h.hosts[0];
    assert_eq!(b.project, "swimcoach");
    assert_eq!(b.stack, "agentbox");
    assert_eq!(b.state, HostState::Ready);
    assert_eq!(b.instance_id, None);
    assert_eq!(b.last_attempt_at, None);
    // `read` is the read a change starts from: the seed document's record has no type (pin P31).
    assert_eq!(b.instance_type, None);
}

#[test]
fn the_seed_is_the_pinned_names() {
    let t = TempHome::new();
    let h = read(t.path()).unwrap();
    assert_eq!(h.hosts[0].project, BOX_PROJECT);
    assert_eq!(h.hosts[0].stack, BOX_STACK);
    assert_eq!(h, seed());
}

#[test]
fn reading_a_missing_file_writes_nothing_at_all() {
    let t = TempHome::new();
    read(t.path()).unwrap();
    read(t.path()).unwrap();
    assert!(
        TempHome::names_in(t.path()).is_empty(),
        "Reading never creates the file, its lock or its folder"
    );
}

#[test]
fn a_missing_agentspace_folder_is_a_missing_file_not_an_error() {
    let t = TempHome::new();
    assert!(!t.state().exists());
    assert_eq!(read(t.path()).unwrap(), seed());
}

#[test]
fn an_existing_agentspace_folder_without_the_file_reads_the_seed_too() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state()).unwrap();
    assert_eq!(read(t.path()).unwrap(), seed());
    assert!(!t.hosts_json().exists(), "Reading never creates the file");
}

#[test]
fn the_seed_sample_reads_as_the_seed() {
    assert_eq!(read_text(SAMPLE_SEED).unwrap(), seed());
}

// --- Every form of a record -------------------------------------------------------------------

#[test]
fn each_state_is_read() {
    for (text, state) in [
        ("creating", HostState::Creating),
        ("ready", HostState::Ready),
        ("failed", HostState::Failed),
    ] {
        let doc = doc_with(&format!(
            "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"{text}\"}}"
        ));
        let h = read_text(&doc).unwrap();
        assert_eq!(h.hosts.len(), 2, "{text}");
        assert_eq!(h.hosts[1].state, state, "{text}");
    }
}

#[test]
fn optional_members_absent_read_as_none() {
    let doc = doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\"}",
    );
    let h = read_text(&doc).unwrap();
    assert_eq!(h.hosts[1].instance_id, None);
    assert_eq!(h.hosts[1].last_attempt_at, None);
}

#[test]
fn an_instance_id_alone_is_read() {
    let doc = doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-1234567890abcdef0\"}",
    );
    let h = read_text(&doc).unwrap();
    assert_eq!(
        h.hosts[1].instance_id.as_deref(),
        Some("i-1234567890abcdef0")
    );
    assert_eq!(h.hosts[1].last_attempt_at, None);
}

#[test]
fn a_last_attempt_alone_is_read_as_the_string_it_is_stored_as() {
    let doc = doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\",\"last_attempt_at\":\"2026-11-20T19:05:42Z\"}",
    );
    let h = read_text(&doc).unwrap();
    assert_eq!(h.hosts[1].instance_id, None);
    assert_eq!(
        h.hosts[1].last_attempt_at.as_deref(),
        Some("2026-11-20T19:05:42Z")
    );
}

#[test]
fn both_optional_members_present_are_read() {
    let doc = doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"failed\",\"instance_id\":\"i-fedcba9876543210f\",\"last_attempt_at\":\"2026-11-20T19:10:00Z\"}",
    );
    let h = read_text(&doc).unwrap();
    assert_eq!(
        h.hosts[1],
        full_host(
            "agentspace",
            HostState::Failed,
            "i-fedcba9876543210f",
            "2026-11-20T19:10:00Z"
        )
    );
}

#[test]
fn the_boxs_record_may_carry_an_instance_id_and_a_time() {
    let doc = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-1234567890abcdef0\",\"last_attempt_at\":\"2026-11-20T18:30:00Z\"}]}";
    let h = read_text(doc).unwrap();
    assert!(is_box(&h.hosts[0]));
    assert_eq!(
        h.hosts[0].instance_id.as_deref(),
        Some("i-1234567890abcdef0")
    );
}

fn is_box(h: &Host) -> bool {
    agentspace_hosts::is_box(h)
}

#[test]
fn an_eight_digit_instance_id_is_accepted() {
    let doc = doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-1234abcd\"}",
    );
    assert!(read_text(&doc).is_ok());
}

#[test]
fn the_full_sample_is_read_in_order() {
    let h = read_text(SAMPLE_FULL).unwrap();
    assert_eq!(h.hosts.len(), 2);
    assert_eq!(h.hosts[0].project, "swimcoach");
    assert_eq!(
        h.hosts[0].instance_id.as_deref(),
        Some("i-0123456789abcdef0")
    );
    assert_eq!(
        h.hosts[0].last_attempt_at.as_deref(),
        Some("2026-11-20T18:30:00Z")
    );
    assert_eq!(
        h.hosts[1],
        full_host(
            "agentspace",
            HostState::Ready,
            "i-0fedcba9876543210",
            "2026-11-20T19:05:42Z"
        )
    );
}

#[test]
fn the_creating_and_failed_sample_is_read_in_order() {
    let h = read_text(SAMPLE_CREATING_AND_FAILED).unwrap();
    let names: Vec<&str> = h.hosts.iter().map(|r| r.project.as_str()).collect();
    assert_eq!(names, ["swimcoach", "agentspace", "example-two"]);
    assert_eq!(h.hosts[1].state, HostState::Creating);
    assert_eq!(h.hosts[1].instance_id, None);
    assert_eq!(
        h.hosts[1].last_attempt_at.as_deref(),
        Some("2026-11-20T19:05:42Z")
    );
    assert_eq!(h.hosts[2].state, HostState::Failed);
    assert_eq!(
        h.hosts[2].instance_id.as_deref(),
        Some("i-0123456789abcdef0")
    );
}

#[test]
fn the_boxs_record_need_not_be_first_to_be_valid_but_order_is_kept() {
    // Only a person's edit can reorder; a reader keeps the order it finds (state.md, hosts is
    // "in the order they were added").
    let doc = "{\"version\":0,\"hosts\":[{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\"},{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}";
    let h = read_text(doc).unwrap();
    assert_eq!(h.hosts[0].project, "agentspace");
    assert_eq!(h.hosts[1].project, "swimcoach");
}

#[test]
fn a_reader_does_not_depend_on_member_order() {
    let doc = "{\"hosts\":[{\"state\":\"ready\",\"stack\":\"agentbox\",\"project\":\"swimcoach\"},{\"last_attempt_at\":\"2026-11-20T19:05:42Z\",\"instance_id\":\"i-1234567890abcdef0\",\"state\":\"ready\",\"stack\":\"agentspace-host-agentspace\",\"project\":\"agentspace\"}],\"version\":0}";
    let h = read_text(doc).unwrap();
    assert_eq!(h.hosts.len(), 2);
    assert_eq!(
        h.hosts[1],
        full_host(
            "agentspace",
            HostState::Ready,
            "i-1234567890abcdef0",
            "2026-11-20T19:05:42Z"
        )
    );
}

#[test]
fn project_names_of_one_and_thirty_two_characters_are_accepted() {
    let long = "a".repeat(32);
    for project in ["a", "7", &long, "a-b", "ab-1-cd"] {
        let doc = doc_with(&format!(
            "{{\"project\":\"{project}\",\"stack\":\"agentspace-host-{project}\",\"state\":\"ready\"}}"
        ));
        assert!(read_text(&doc).is_ok(), "{project}");
    }
}

#[test]
fn an_unnormalized_project_name_is_refused() {
    let too_long = "a".repeat(33);
    for project in ["", "-a", "a-", "A", "a_b", "a b", "a.b", &too_long] {
        let doc = doc_with(&format!(
            "{{\"project\":\"{project}\",\"stack\":\"agentspace-host-{project}\",\"state\":\"ready\"}}"
        ));
        assert_refused(&format!("project {project:?}"), doc);
    }
}

// --- The bytes: read tolerantly (D20) ---------------------------------------------------------

#[test]
fn a_leading_byte_order_mark_is_accepted() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(SAMPLE_SEED.as_bytes());
    assert_eq!(
        read_text(std::str::from_utf8(&bytes).unwrap()).unwrap(),
        seed()
    );
}

#[test]
fn any_amount_of_json_whitespace_is_accepted() {
    let doc = " \r\n\t{ \"version\" :\t0 ,\r\n \"hosts\" : [ \r\n {\"project\": \"swimcoach\" , \"stack\":\"agentbox\",\n\"state\" : \"ready\"} ] }\r\n\r\n ";
    assert_eq!(read_text(doc).unwrap(), seed());
}

#[test]
fn a_file_with_no_trailing_line_feed_is_accepted() {
    assert_eq!(read_text(SAMPLE_SEED.trim_end()).unwrap(), seed());
}

#[test]
fn a_version_written_as_zero_point_zero_is_accepted() {
    let doc = format!("{{\"version\":0.0,\"hosts\":[{BOX_RECORD}]}}");
    assert_eq!(read_text(&doc).unwrap(), seed());
}

// --- A file that is not valid is an error, never the seed -------------------------------------

#[test]
fn every_invalid_sample_is_refused() {
    for (label, text) in INVALID_SAMPLES {
        assert_refused(label, text);
    }
}

#[test]
fn a_file_that_is_not_valid_is_never_read_as_the_seed() {
    // A host that is creating would be lost by a rebuild from the seed.
    let doc = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\",\"mystery\":1}]}";
    assert_refused("extra member in a second record", doc);
}

#[test]
fn an_empty_file_is_not_valid() {
    assert_refused("empty", "");
    assert_refused("only whitespace", " \r\n");
}

#[test]
fn text_that_is_not_json_is_not_valid() {
    assert_refused("words", "this is not json");
    assert_refused("cut off", "{\"version\":0,\"hosts\":[");
    assert_refused("two documents", format!("{SAMPLE_SEED}{SAMPLE_SEED}"));
    assert_refused("trailing words", format!("{SAMPLE_SEED}x"));
}

#[test]
fn bytes_that_are_not_utf_8_are_not_valid() {
    let mut bytes = SAMPLE_SEED.as_bytes().to_vec();
    // Put an invalid byte inside a string: 0xFF never occurs in UTF-8.
    let at = bytes.iter().position(|b| *b == b'w').unwrap();
    bytes[at] = 0xFF;
    assert_refused("invalid UTF-8", bytes);
    assert_refused(
        "latin-1 byte",
        b"{\"version\":0,\"hosts\":[],\"x\":\"\xE9\"}",
    );
}

#[test]
fn a_version_other_than_the_integer_zero_is_not_valid() {
    for v in ["1", "-1", "\"0\"", "null", "true", "0.5", "[0]"] {
        let doc = format!("{{\"version\":{v},\"hosts\":[{BOX_RECORD}]}}");
        assert_refused(&format!("version {v}"), doc);
    }
}

#[test]
fn a_missing_version_or_hosts_is_not_valid() {
    assert_refused("no version", format!("{{\"hosts\":[{BOX_RECORD}]}}"));
    assert_refused("no hosts", "{\"version\":0}");
}

#[test]
fn a_top_level_that_is_not_an_object_is_not_valid() {
    assert_refused("null", "null");
    assert_refused("number", "0");
    assert_refused("string", "\"hosts\"");
}

#[test]
fn a_record_that_is_not_an_object_is_not_valid() {
    assert_refused("string record", "{\"version\":0,\"hosts\":[\"swimcoach\"]}");
    assert_refused(
        "null record",
        format!("{{\"version\":0,\"hosts\":[{BOX_RECORD},null]}}"),
    );
}

#[test]
fn an_object_that_names_a_member_twice_is_not_valid_whichever_value_a_parser_keeps() {
    assert_refused(
        "version twice",
        format!("{{\"version\":0,\"version\":0,\"hosts\":[{BOX_RECORD}]}}"),
    );
    assert_refused(
        "hosts twice",
        format!("{{\"version\":0,\"hosts\":[{BOX_RECORD}],\"hosts\":[{BOX_RECORD}]}}"),
    );
    assert_refused(
        "state twice with the same value",
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"state\":\"ready\"}]}",
    );
    assert_refused(
        "state twice, the valid one last",
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"stopped\",\"state\":\"ready\"}]}",
    );
    assert_refused(
        "state twice, the valid one first",
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"state\":\"stopped\"}]}",
    );
    assert_refused(
        "instance_id twice",
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-1234abcd\",\"instance_id\":\"i-1234abcd\"}]}",
    );
}

#[test]
fn null_is_never_accepted_for_an_optional_member() {
    for member in ["instance_id", "last_attempt_at"] {
        let doc = format!(
            "{{\"version\":0,\"hosts\":[{{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"{member}\":null}}]}}"
        );
        assert_refused(member, doc);
    }
}

#[test]
fn a_state_must_be_one_of_the_three_in_lower_case() {
    for s in ["Ready", "READY", "stopped", "", "pending", "ready "] {
        let doc = doc_with(&format!(
            "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"{s}\"}}"
        ));
        assert_refused(&format!("state {s:?}"), doc);
    }
}

#[test]
fn an_instance_id_must_match_its_pattern() {
    for id in [
        "i-0123",
        "i-123456789",
        "i-1234567890abcdef",
        "i-1234567890abcdef01",
        "i-1234567890ABCDEF0",
        "I-1234abcd",
        "1234abcd",
        "i-1234abcg",
        "",
    ] {
        let doc = doc_with(&format!(
            "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"{id}\"}}"
        ));
        assert_refused(&format!("instance id {id:?}"), doc);
    }
}

#[test]
fn a_value_that_ends_in_a_line_feed_is_not_valid() {
    // The patterns end in $(?!\n): a bare $ would let a trailing line feed through.
    assert_refused(
        "project",
        doc_with(
            "{\"project\":\"agentspace\\n\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\"}",
        ),
    );
    assert_refused(
        "stack",
        doc_with(
            "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\\n\",\"state\":\"ready\"}",
        ),
    );
    assert_refused(
        "instance_id",
        doc_with(
            "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-1234abcd\\n\"}",
        ),
    );
    assert_refused(
        "last_attempt_at",
        doc_with(
            "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"last_attempt_at\":\"2026-11-20T19:05:42Z\\n\"}",
        ),
    );
}

#[test]
fn a_time_must_be_in_the_contracts_form() {
    for at in [
        "2026-11-20",
        "2026-11-20T19:05:42",
        "2026-11-20T19:05:42+00:00",
        "2026-11-20 19:05:42Z",
        "2026-11-20T19:05:42.5Z",
        "2026-11-20t19:05:42z",
        "2026-13-20T19:05:42Z",
        "2026-00-20T19:05:42Z",
        "2026-11-00T19:05:42Z",
        "2026-11-32T19:05:42Z",
        "2026-11-20T24:00:00Z",
        "2026-11-20T19:60:00Z",
        "",
    ] {
        let doc = doc_with(&format!(
            "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\",\"last_attempt_at\":\"{at}\"}}"
        ));
        assert_refused(&format!("time {at:?}"), doc);
    }
}

#[test]
fn a_time_that_names_no_real_instant_is_not_valid() {
    for at in [
        "2026-02-30T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-06-31T12:00:00Z",
        "2100-02-29T00:00:00Z",
        "2026-11-31T00:00:00Z",
        "2026-11-20T23:59:60Z",
        "2026-06-30T23:59:60Z",
    ] {
        let doc = doc_with(&format!(
            "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\",\"last_attempt_at\":\"{at}\"}}"
        ));
        assert_refused(&format!("time {at}"), doc);
    }
}

#[test]
fn real_edge_times_are_accepted() {
    for at in [
        "2028-02-29T00:00:00Z",
        "2000-02-29T23:59:59Z",
        "2026-12-31T23:59:59Z",
        "2026-01-01T00:00:00Z",
        "2026-04-30T12:00:00Z",
    ] {
        let doc = doc_with(&format!(
            "{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\",\"last_attempt_at\":\"{at}\"}}"
        ));
        let h = read_text(&doc).unwrap_or_else(|e| panic!("{at} refused: {e:?}"));
        assert_eq!(h.hosts[1].last_attempt_at.as_deref(), Some(at));
    }
}

// --- What makes a file not valid beyond its schema --------------------------------------------

#[test]
fn two_records_with_one_project_are_not_valid() {
    let doc = format!(
        "{{\"version\":0,\"hosts\":[{BOX_RECORD},{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\"}},{{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"creating\"}}]}}"
    );
    assert_refused("duplicate project", doc);
}

#[test]
fn two_records_for_the_boxs_project_are_not_valid() {
    let doc = format!("{{\"version\":0,\"hosts\":[{BOX_RECORD},{BOX_RECORD}]}}");
    assert_refused("duplicate box", doc);
}

#[test]
fn a_stack_named_for_another_project_is_not_valid() {
    let doc = doc_with(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-swimcoach\",\"state\":\"ready\"}",
    );
    assert_refused("stack of another project", doc);
}

#[test]
fn a_stack_that_is_a_prefix_or_extension_of_the_projects_is_not_valid() {
    assert_refused(
        "prefix",
        doc_with(
            "{\"project\":\"agentspace-two\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\"}",
        ),
    );
    assert_refused(
        "extension",
        doc_with(
            "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace-two\",\"state\":\"ready\"}",
        ),
    );
}

#[test]
fn a_non_box_project_on_the_agentbox_stack_is_not_valid() {
    // Exactly one record has the stack agentbox, and its project is swimcoach.
    let doc = doc_with("{\"project\":\"agentspace\",\"stack\":\"agentbox\",\"state\":\"ready\"}");
    assert_refused("second agentbox record", doc);
}

#[test]
fn the_boxs_project_with_another_stack_is_not_valid() {
    let doc = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentspace-host-swimcoach\",\"state\":\"ready\"}]}";
    assert_refused("swimcoach on a host stack and no box record", doc);
}

#[test]
fn a_file_with_no_box_record_is_not_valid() {
    assert_refused("no hosts at all", "{\"version\":0,\"hosts\":[]}");
    assert_refused(
        "only another host",
        "{\"version\":0,\"hosts\":[{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\"}]}",
    );
}

#[test]
fn the_only_agentbox_record_must_be_swimcoachs() {
    let doc = "{\"version\":0,\"hosts\":[{\"project\":\"agentspace\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}";
    assert_refused("agentbox for another project", doc);
}

// --- Something at the path that is not a file -------------------------------------------------

#[test]
fn a_folder_at_the_registrys_path_is_an_error_not_a_missing_file() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.hosts_json()).unwrap();
    let e = read(t.path()).expect_err("A folder is not a missing file");
    assert!(is_file_problem(&e), "{e:?}");
    assert_message(&e, t.path());
    assert!(t.hosts_json().is_dir(), "Nothing is repaired or removed");
}

#[test]
fn a_file_where_the_home_folder_belongs_is_an_error() {
    let t = TempHome::new();
    let home_as_file = t.path().join("a-file");
    std::fs::write(&home_as_file, b"x").unwrap();
    let e = read(&home_as_file).expect_err("A file is not a home folder");
    assert_message(&e, t.path());
}

#[test]
fn a_file_where_the_agentspace_folder_belongs_is_an_error() {
    let t = TempHome::new();
    std::fs::write(t.state(), b"x").unwrap();
    let e = read(t.path()).expect_err("A file is not the agentspace folder");
    assert_message(&e, t.path());
}

// --- A reader sees the old file or the new one ------------------------------------------------

#[test]
fn reading_leaves_the_file_byte_for_byte_as_it_was() {
    let t = TempHome::new();
    t.put_hosts(SAMPLE_FULL);
    let before = t.hosts_bytes();
    read(t.path()).unwrap();
    assert_eq!(t.hosts_bytes(), before);
    assert_eq!(
        TempHome::names_in(&t.state()),
        ["hosts.json"],
        "Reading leaves nothing beside the file"
    );
}

#[test]
fn reading_takes_no_lock_so_it_works_while_the_lock_is_held() {
    let t = TempHome::new();
    t.put_hosts(SAMPLE_FULL);
    let _held = agentspace_hosts::lock(t.path()).unwrap();
    assert_eq!(read(t.path()).unwrap().hosts.len(), 2);
}

#[test]
fn a_record_is_a_plain_value_that_compares_by_content() {
    let a: Hosts = seed();
    let b = read_text(SAMPLE_SEED).unwrap();
    assert_eq!(a, b);
    assert_ne!(a, with_box(vec![host("agentspace", HostState::Creating)]));
}

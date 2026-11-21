//! The project list (docs/state.md, The project list; D54): which projects may have a host made.

mod common;

use agentspace_hosts::{is_listed, read_projects};
use common::*;

const SAMPLE_AGENTSPACE: &str =
    include_str!("../../../schemas/state/samples/projects.state.agentspace.json");
const SAMPLE_EMPTY: &str = include_str!("../../../schemas/state/samples/projects.state.empty.json");

const INVALID_SAMPLES: &[(&str, &str)] = &[
    (
        "bad-date",
        include_str!("../../../schemas/state/samples/invalid/projects.state.bad-date.json"),
    ),
    (
        "bare-names",
        include_str!("../../../schemas/state/samples/invalid/projects.state.bare-names.json"),
    ),
    (
        "date-with-time",
        include_str!("../../../schemas/state/samples/invalid/projects.state.date-with-time.json"),
    ),
    (
        "extra-member",
        include_str!("../../../schemas/state/samples/invalid/projects.state.extra-member.json"),
    ),
    (
        "missing-approved",
        include_str!("../../../schemas/state/samples/invalid/projects.state.missing-approved.json"),
    ),
    (
        "missing-name",
        include_str!("../../../schemas/state/samples/invalid/projects.state.missing-name.json"),
    ),
    (
        "uppercase-name",
        include_str!("../../../schemas/state/samples/invalid/projects.state.uppercase-name.json"),
    ),
    (
        "wrong-version",
        include_str!("../../../schemas/state/samples/invalid/projects.state.wrong-version.json"),
    ),
];

fn entry(name: &str, approved: &str) -> String {
    format!("{{\"name\":\"{name}\",\"approved\":\"{approved}\"}}")
}

fn doc(entries: &[String]) -> String {
    format!("{{\"version\":0,\"projects\":[{}]}}", entries.join(","))
}

fn listed_in(text: &str, project: &str) -> bool {
    let t = TempHome::new();
    t.put_projects(text);
    let p = read_projects(t.path()).expect("The list must be read");
    is_listed(&p, project)
}

fn assert_refused(label: &str, bytes: impl AsRef<[u8]>) {
    let t = TempHome::new();
    t.put_projects(bytes.as_ref());
    let before = std::fs::read(t.projects_json()).unwrap();
    match read_projects(t.path()) {
        Ok(_) => panic!("{label}: a file that is not valid was read"),
        Err(e) => {
            assert!(
                is_not_valid(&e),
                "{label}: expected a not-valid error, got {e:?}"
            );
            assert_message(&e, t.path());
        }
    }
    assert_eq!(
        std::fs::read(t.projects_json()).unwrap(),
        before,
        "{label}: nothing repairs or rewrites the list"
    );
}

// --- Missing lists nothing --------------------------------------------------------------------

#[test]
fn a_missing_file_lists_nothing() {
    let t = TempHome::new();
    let p = read_projects(t.path()).expect("A missing file is no error");
    assert!(!is_listed(&p, "agentspace"));
    assert!(!is_listed(&p, "swimcoach"));
    assert!(!is_listed(&p, ""));
}

#[test]
fn reading_a_missing_list_writes_nothing() {
    let t = TempHome::new();
    read_projects(t.path()).unwrap();
    assert!(TempHome::names_in(t.path()).is_empty());
}

#[test]
fn a_missing_file_in_an_existing_folder_lists_nothing_and_stays_missing() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state()).unwrap();
    let p = read_projects(t.path()).unwrap();
    assert!(!is_listed(&p, "agentspace"));
    assert!(!t.projects_json().exists());
}

// --- What is listed ---------------------------------------------------------------------------

#[test]
fn the_agentspace_sample_lists_agentspace_alone() {
    assert!(listed_in(SAMPLE_AGENTSPACE, "agentspace"));
    assert!(!listed_in(SAMPLE_AGENTSPACE, "swimcoach"));
    assert!(!listed_in(SAMPLE_AGENTSPACE, "example-two"));
}

#[test]
fn the_empty_sample_lists_nothing() {
    assert!(!listed_in(SAMPLE_EMPTY, "agentspace"));
}

#[test]
fn every_entry_of_a_longer_list_is_listed() {
    let text = doc(&[
        entry("agentspace", "2026-11-20"),
        entry("example-two", "2026-11-21"),
        entry("a", "2026-01-01"),
    ]);
    for name in ["agentspace", "example-two", "a"] {
        assert!(listed_in(&text, name), "{name}");
    }
    assert!(!listed_in(&text, "example"));
}

#[test]
fn a_project_is_listed_only_when_a_name_equals_it_exactly() {
    let text = doc(&[entry("agentspace", "2026-11-20")]);
    assert!(listed_in(&text, "agentspace"));
    assert!(!listed_in(&text, "agentspac"));
    assert!(!listed_in(&text, "agentspace2"));
    assert!(!listed_in(&text, "agentspace-two"));
    assert!(!listed_in(&text, "agent"));
    assert!(!listed_in(&text, ""));
}

#[test]
fn listing_does_not_normalize_the_projects_it_is_asked_about() {
    let text = doc(&[entry("agentspace", "2026-11-20")]);
    assert!(!listed_in(&text, "Agentspace"));
    assert!(!listed_in(&text, "AGENTSPACE"));
    assert!(!listed_in(&text, " agentspace"));
    assert!(!listed_in(&text, "agentspace "));
    assert!(!listed_in(&text, "agentspace\n"));
}

#[test]
fn names_of_one_and_thirty_two_characters_are_accepted() {
    let long = "a".repeat(32);
    for name in ["a", "7", long.as_str(), "a-b", "ab-1-cd"] {
        assert!(
            listed_in(&doc(&[entry(name, "2026-11-20")]), name),
            "{name}"
        );
    }
}

#[test]
fn an_unnormalized_name_is_refused_not_normalized() {
    let too_long = "a".repeat(33);
    for name in [
        "",
        "-a",
        "a-",
        "A",
        "AgentSpace",
        "a_b",
        "a b",
        "a.b",
        too_long.as_str(),
    ] {
        assert_refused(&format!("name {name:?}"), doc(&[entry(name, "2026-11-20")]));
    }
}

#[test]
fn a_name_that_ends_in_a_line_feed_is_refused() {
    assert_refused(
        "name",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\\n\",\"approved\":\"2026-11-20\"}]}",
    );
    assert_refused(
        "approved",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\\n\"}]}",
    );
}

// --- Dates ------------------------------------------------------------------------------------

#[test]
fn real_dates_are_accepted() {
    for date in [
        "2026-11-20",
        "2026-01-01",
        "2026-12-31",
        "2028-02-29",
        "2000-02-29",
        "2026-04-30",
        "0001-01-01",
    ] {
        assert!(
            listed_in(&doc(&[entry("agentspace", date)]), "agentspace"),
            "{date}"
        );
    }
}

#[test]
fn a_date_in_the_future_is_valid() {
    // Nothing compares `approved` with the clock.
    assert!(listed_in(
        &doc(&[entry("agentspace", "2999-12-31")]),
        "agentspace"
    ));
}

#[test]
fn a_date_that_names_no_real_day_is_not_valid() {
    for date in [
        "2026-02-30",
        "2026-04-31",
        "2026-02-29",
        "2100-02-29",
        "1900-02-29",
        "2026-06-31",
        "2026-09-31",
        "2026-11-31",
        "2028-02-30",
    ] {
        assert_refused(&format!("date {date}"), doc(&[entry("agentspace", date)]));
    }
}

#[test]
fn a_date_that_is_not_in_the_form_is_not_valid() {
    for date in [
        "2026-13-01",
        "2026-00-10",
        "2026-11-00",
        "2026-11-32",
        "2026-1-20",
        "2026-11-2",
        "26-11-20",
        "20261120",
        "2026/11/20",
        "2026-11-20T00:00:00Z",
        "2026-11-20 ",
        " 2026-11-20",
        "",
        "today",
    ] {
        assert_refused(&format!("date {date:?}"), doc(&[entry("agentspace", date)]));
    }
}

// --- Invalid files ----------------------------------------------------------------------------

#[test]
fn every_invalid_sample_is_refused() {
    for (label, text) in INVALID_SAMPLES {
        assert_refused(label, text);
    }
}

#[test]
fn two_entries_with_one_name_are_not_valid() {
    assert_refused(
        "duplicate name",
        doc(&[
            entry("agentspace", "2026-11-20"),
            entry("agentspace", "2026-11-21"),
        ]),
    );
    assert_refused(
        "duplicate name, same date",
        doc(&[
            entry("agentspace", "2026-11-20"),
            entry("example-two", "2026-11-20"),
            entry("agentspace", "2026-11-20"),
        ]),
    );
}

#[test]
fn an_object_that_names_a_member_twice_is_not_valid() {
    assert_refused(
        "version twice",
        "{\"version\":0,\"version\":0,\"projects\":[]}",
    );
    assert_refused(
        "projects twice",
        "{\"version\":0,\"projects\":[],\"projects\":[]}",
    );
    assert_refused(
        "name twice",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"name\":\"agentspace\",\"approved\":\"2026-11-20\"}]}",
    );
    assert_refused(
        "approved twice, the valid one last",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-02-30\",\"approved\":\"2026-11-20\"}]}",
    );
    assert_refused(
        "approved twice, the valid one first",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"approved\":\"2026-02-30\"}]}",
    );
}

#[test]
fn a_version_other_than_the_integer_zero_is_not_valid() {
    for v in ["1", "-1", "\"0\"", "null", "true", "0.5"] {
        assert_refused(
            &format!("version {v}"),
            format!("{{\"version\":{v},\"projects\":[]}}"),
        );
    }
}

#[test]
fn a_missing_member_is_not_valid() {
    assert_refused("no version", "{\"projects\":[]}");
    assert_refused("no projects", "{\"version\":0}");
}

#[test]
fn an_empty_file_or_text_that_is_not_json_is_not_valid() {
    assert_refused("empty", "");
    assert_refused("whitespace", "  \r\n");
    assert_refused("words", "agentspace");
    assert_refused("cut off", "{\"version\":0,\"projects\":[");
    assert_refused("two documents", format!("{SAMPLE_EMPTY}{SAMPLE_EMPTY}"));
}

#[test]
fn a_top_level_that_is_not_an_object_is_not_valid() {
    assert_refused("array", "[]");
    assert_refused("null", "null");
}

#[test]
fn bytes_that_are_not_utf_8_are_not_valid() {
    let mut bytes = SAMPLE_AGENTSPACE.as_bytes().to_vec();
    let at = bytes.iter().position(|b| *b == b'g').unwrap();
    bytes[at] = 0xFF;
    assert_refused("invalid UTF-8", bytes);
}

#[test]
fn a_file_that_is_not_valid_lists_nothing_but_is_an_error() {
    // Not the same as a missing file: a mistyped list MUST NOT answer "not listed".
    let t = TempHome::new();
    t.put_projects("{\"version\":0,\"projects\":[{\"name\":\"agentspace\"}]}");
    assert!(read_projects(t.path()).is_err());
}

// --- Tolerant bytes (a person writes it) ------------------------------------------------------

#[test]
fn a_leading_byte_order_mark_is_accepted() {
    let t = TempHome::new();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(SAMPLE_AGENTSPACE.as_bytes());
    t.put_projects(bytes);
    let p = read_projects(t.path()).expect("A byte order mark is accepted");
    assert!(is_listed(&p, "agentspace"));
}

#[test]
fn whitespace_and_line_endings_that_json_allows_are_accepted() {
    let text = "\r\n  {\r\n\t\"version\" : 0 ,\r\n\t\"projects\" : [\r\n\t\t{ \"name\" : \"agentspace\" ,\r\n\t\t  \"approved\" : \"2026-11-20\" }\r\n\t]\r\n}\r\n\r\n";
    assert!(listed_in(text, "agentspace"));
}

#[test]
fn a_file_with_no_final_line_feed_is_accepted() {
    assert!(listed_in(SAMPLE_AGENTSPACE.trim_end(), "agentspace"));
}

#[test]
fn members_in_any_order_are_accepted() {
    let text =
        "{\"projects\":[{\"approved\":\"2026-11-20\",\"name\":\"agentspace\"}],\"version\":0}";
    assert!(listed_in(text, "agentspace"));
}

#[test]
fn a_version_written_as_zero_point_zero_is_accepted() {
    assert!(listed_in(
        "{\"version\":0.0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\"}]}",
        "agentspace"
    ));
}

// --- Something at the path that is not a file -------------------------------------------------

#[test]
fn a_folder_at_the_lists_path_is_an_error_not_a_missing_file() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.projects_json()).unwrap();
    let e = read_projects(t.path()).expect_err("A folder is not a missing file");
    assert!(is_file_problem(&e), "{e:?}");
    assert_message(&e, t.path());
    assert!(t.projects_json().is_dir());
}

#[test]
fn a_file_where_the_agentspace_folder_belongs_is_an_error() {
    let t = TempHome::new();
    std::fs::write(t.state(), b"x").unwrap();
    let e = read_projects(t.path()).expect_err("A file is not the agentspace folder");
    assert_message(&e, t.path());
}

#[test]
fn a_file_where_the_home_folder_belongs_is_an_error() {
    let t = TempHome::new();
    let home_as_file = t.path().join("a-file");
    std::fs::write(&home_as_file, b"x").unwrap();
    let e = read_projects(&home_as_file).expect_err("A file is not a home folder");
    assert_message(&e, t.path());
}

// --- Reading changes nothing ------------------------------------------------------------------

#[test]
fn reading_leaves_the_list_byte_for_byte_and_makes_nothing_beside_it() {
    let t = TempHome::new();
    t.put_projects(SAMPLE_AGENTSPACE);
    read_projects(t.path()).unwrap();
    read_projects(t.path()).unwrap();
    assert_eq!(
        std::fs::read(t.projects_json()).unwrap(),
        SAMPLE_AGENTSPACE.as_bytes()
    );
    assert_eq!(TempHome::names_in(&t.state()), ["projects.json"]);
}

#[test]
fn the_registry_and_the_list_are_read_apart() {
    let t = TempHome::new();
    t.put_projects(SAMPLE_AGENTSPACE);
    // A list does not make a host: the registry still reads as the seed.
    assert_eq!(agentspace_hosts::read(t.path()).unwrap(), seed());
    // And a registry that is not valid does not make the list unreadable.
    t.put_hosts("not json");
    assert!(read_projects(t.path()).is_ok());
}

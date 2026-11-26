//! The project list's `platforms` (docs/state.md, The project list; D91, D101; E0.12-T02-S01,
//! card #0250): which platforms an entry approves, the answer for a platform its project is not
//! approved for, each rule on the member's form, and the lists v0.11 wrote read unchanged.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The predicate is
//! `is_approved(&projects, project, platform) -> bool` (the dev lead's pin P3, architect's to
//! confirm); every call of it in this file goes through `approved` below.

mod common;

use agentspace_hosts::{is_approved, is_listed, read_projects};
use common::*;

const SAMPLE_PLATFORMS: &str =
    include_str!("../../../schemas/state/samples/projects.state.platforms.json");
const SAMPLE_VARIANT: &str =
    include_str!("../../../schemas/state/samples/projects.state.platforms-variant.json");
const SAMPLE_V0_11: &str =
    include_str!("../../../schemas/state/samples/projects.state.agentspace.json");

const INVALID_SAMPLES: &[(&str, &str)] = &[
    (
        "platforms-aarch64",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-aarch64.json"
        ),
    ),
    (
        "platforms-arm64-v8",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-arm64-v8.json"
        ),
    ),
    (
        "platforms-bare-names",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-bare-names.json"
        ),
    ),
    (
        "platforms-date-with-time",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-date-with-time.json"
        ),
    ),
    (
        "platforms-duplicate-item",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-duplicate-item.json"
        ),
    ),
    (
        "platforms-duplicate-platform",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-duplicate-platform.json"
        ),
    ),
    (
        "platforms-empty",
        include_str!("../../../schemas/state/samples/invalid/projects.state.platforms-empty.json"),
    ),
    (
        "platforms-extra-member",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-extra-member.json"
        ),
    ),
    (
        "platforms-missing-approved",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-missing-approved.json"
        ),
    ),
    (
        "platforms-missing-platform",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-missing-platform.json"
        ),
    ),
    (
        "platforms-null",
        include_str!("../../../schemas/state/samples/invalid/projects.state.platforms-null.json"),
    ),
    (
        "platforms-uppercase-platform",
        include_str!(
            "../../../schemas/state/samples/invalid/projects.state.platforms-uppercase-platform.json"
        ),
    ),
];

fn approval(platform: &str, approved: &str) -> String {
    format!("{{\"platform\":\"{platform}\",\"approved\":\"{approved}\"}}")
}

/// An entry with `platforms` holding the given approvals.
fn entry_with(name: &str, approvals: &[String]) -> String {
    format!(
        "{{\"name\":\"{name}\",\"approved\":\"2026-11-20\",\"platforms\":[{}]}}",
        approvals.join(",")
    )
}

fn entry_without(name: &str) -> String {
    format!("{{\"name\":\"{name}\",\"approved\":\"2026-11-20\"}}")
}

fn doc(entries: &[String]) -> String {
    format!("{{\"version\":0,\"projects\":[{}]}}", entries.join(","))
}

/// A list's text, read afresh by each question so that no test names the reader's result type.
struct Loaded(String);

/// The list, which MUST be read as valid.
fn read_text(text: &str) -> Loaded {
    let t = TempHome::new();
    t.put_projects(text);
    read_projects(t.path()).expect("The list must be read");
    Loaded(text.to_string())
}

/// The one call of the predicate.
fn approved(p: &Loaded, project: &str, platform: &str) -> bool {
    let t = TempHome::new();
    t.put_projects(&p.0);
    let list = read_projects(t.path()).expect("The list must be read");
    is_approved(&list, project, platform)
}

fn listed(p: &Loaded, project: &str) -> bool {
    let t = TempHome::new();
    t.put_projects(&p.0);
    let list = read_projects(t.path()).expect("The list must be read");
    is_listed(&list, project)
}

fn assert_refused(label: &str, bytes: impl AsRef<[u8]>) {
    let t = TempHome::new();
    t.put_projects(bytes.as_ref());
    let before = std::fs::read(t.projects_json()).unwrap();
    match read_projects(t.path()) {
        Ok(_) => panic!("{label}: a file that is not valid was read"),
        Err(e) => {
            assert!(is_not_valid(&e), "{label}: expected not valid, got {e:?}");
            assert_message(&e, t.path());
        }
    }
    assert_eq!(
        std::fs::read(t.projects_json()).unwrap(),
        before,
        "{label}: nothing repairs or rewrites the list"
    );
}

// --- An entry without platforms ---------------------------------------------------------------

#[test]
fn an_entry_without_platforms_approves_linux_arm64() {
    let p = read_text(&doc(&[entry_without("agentspace")]));
    assert!(approved(&p, "agentspace", "linux/arm64"));
}

#[test]
fn an_entry_without_platforms_approves_linux_arm64_alone() {
    let p = read_text(&doc(&[entry_without("agentspace")]));
    assert!(!approved(&p, "agentspace", "linux/amd64"));
    assert!(!approved(&p, "agentspace", "windows/amd64"));
    assert!(!approved(&p, "agentspace", "windows/arm64"));
    assert!(!approved(&p, "agentspace", ""));
}

#[test]
fn a_v0_11_list_reads_unchanged_and_approves_linux_arm64_for_its_projects() {
    let p = read_text(SAMPLE_V0_11);
    assert!(listed(&p, "agentspace"));
    assert!(approved(&p, "agentspace", "linux/arm64"));
    assert!(!approved(&p, "agentspace", "linux/amd64"));
    assert!(!approved(&p, "agentspace", "windows/amd64"));
}

#[test]
fn a_project_with_no_entry_is_approved_for_no_platform() {
    let p = read_text(&doc(&[entry_without("agentspace")]));
    for platform in ["linux/arm64", "linux/amd64", "windows/amd64"] {
        assert!(!approved(&p, "example-two", platform), "{platform}");
        assert!(!approved(&p, "", platform), "{platform}");
    }
}

#[test]
fn a_missing_list_approves_no_project_on_any_platform() {
    let t = TempHome::new();
    let p = read_projects(t.path()).unwrap();
    for platform in ["linux/arm64", "linux/amd64", "windows/amd64"] {
        assert!(!is_approved(&p, "agentspace", platform), "{platform}");
        assert!(!is_approved(&p, "swimcoach", platform), "{platform}");
    }
}

// --- An entry with platforms ------------------------------------------------------------------

#[test]
fn an_entry_with_platforms_approves_exactly_the_platforms_it_lists() {
    let p = read_text(&doc(&[entry_with(
        "agentspace",
        &[
            approval("linux/arm64", "2026-11-20"),
            approval("windows/amd64", "2026-11-26"),
        ],
    )]));
    assert!(approved(&p, "agentspace", "linux/arm64"));
    assert!(approved(&p, "agentspace", "windows/amd64"));
    assert!(!approved(&p, "agentspace", "linux/amd64"));
}

#[test]
fn an_entry_with_platforms_that_omits_linux_arm64_does_not_approve_it() {
    // state.md: "linux/arm64 is approved only if it is listed, whatever the entry's approved
    // says".
    let p = read_text(&doc(&[entry_with(
        "agentspace",
        &[approval("windows/amd64", "2026-11-26")],
    )]));
    assert!(approved(&p, "agentspace", "windows/amd64"));
    assert!(!approved(&p, "agentspace", "linux/arm64"));
    assert!(!approved(&p, "agentspace", "linux/amd64"));
    // The project is still listed: its entry exists.
    assert!(listed(&p, "agentspace"));
}

#[test]
fn an_entry_listing_linux_amd64_alone_approves_that_alone() {
    let p = read_text(&doc(&[entry_with(
        "agentspace",
        &[approval("linux/amd64", "2026-11-26")],
    )]));
    assert!(approved(&p, "agentspace", "linux/amd64"));
    assert!(!approved(&p, "agentspace", "linux/arm64"));
    assert!(!approved(&p, "agentspace", "windows/amd64"));
}

#[test]
fn every_platform_of_a_full_entry_is_approved() {
    let p = read_text(&doc(&[entry_with(
        "agentspace",
        &[
            approval("linux/arm64", "2026-11-20"),
            approval("linux/amd64", "2026-11-26"),
            approval("windows/amd64", "2026-11-26"),
        ],
    )]));
    for platform in ["linux/arm64", "linux/amd64", "windows/amd64"] {
        assert!(approved(&p, "agentspace", platform), "{platform}");
    }
}

#[test]
fn the_entrys_approved_date_approves_no_platform_by_itself() {
    // Reading `approved` as an approval of linux/arm64 for an entry that holds platforms is the
    // slip this pins: the entry's date is when the project was first approved, and nothing more.
    let p = read_text(&doc(&[entry_with(
        "agentspace",
        &[approval("linux/amd64", "2026-11-20")],
    )]));
    assert!(!approved(&p, "agentspace", "linux/arm64"));
}

#[test]
fn an_approval_is_compared_exactly_and_never_normalized() {
    let p = read_text(&doc(&[entry_with(
        "agentspace",
        &[approval("windows/amd64", "2026-11-26")],
    )]));
    assert!(approved(&p, "agentspace", "windows/amd64"));
    for asked in [
        "Windows/amd64",
        "WINDOWS/AMD64",
        "windows/x86_64",
        "windows/amd64/v3",
        "windows/amd64 ",
        " windows/amd64",
        "windows/amd64\n",
        "windows-amd64",
        "windows",
        "amd64",
        "windows/arm64",
        "",
    ] {
        assert!(!approved(&p, "agentspace", asked), "{asked:?}");
    }
}

#[test]
fn the_asked_project_is_compared_exactly_too() {
    let p = read_text(&doc(&[entry_with(
        "agentspace",
        &[approval("windows/amd64", "2026-11-26")],
    )]));
    assert!(!approved(&p, "Agentspace", "windows/amd64"));
    assert!(!approved(&p, "agentspace-two", "windows/amd64"));
    assert!(!approved(&p, "agent", "windows/amd64"));
}

#[test]
fn each_entry_approves_its_own_platforms_and_not_another_entrys() {
    let p = read_text(SAMPLE_PLATFORMS);
    // agentspace: all three.
    for platform in ["linux/arm64", "linux/amd64", "windows/amd64"] {
        assert!(
            approved(&p, "agentspace", platform),
            "agentspace {platform}"
        );
    }
    // example-one has no platforms: linux/arm64 alone.
    assert!(approved(&p, "example-one", "linux/arm64"));
    assert!(!approved(&p, "example-one", "linux/amd64"));
    assert!(!approved(&p, "example-one", "windows/amd64"));
    // example-two lists windows/amd64 alone, so linux/arm64 is not approved for it.
    assert!(approved(&p, "example-two", "windows/amd64"));
    assert!(!approved(&p, "example-two", "linux/arm64"));
    assert!(!approved(&p, "example-two", "linux/amd64"));
    // example-three lists linux/arm64 and an unsupported platform.
    assert!(approved(&p, "example-three", "linux/arm64"));
    assert!(!approved(&p, "example-three", "linux/amd64"));
    assert!(!approved(&p, "example-three", "windows/amd64"));
    // No entry.
    assert!(!approved(&p, "example-four", "linux/arm64"));
}

#[test]
fn every_entry_of_the_platforms_sample_is_listed() {
    let p = read_text(SAMPLE_PLATFORMS);
    for name in ["agentspace", "example-one", "example-two", "example-three"] {
        assert!(listed(&p, name), "{name}");
    }
    assert!(!listed(&p, "example-four"));
}

#[test]
fn a_platform_the_project_is_not_approved_for_is_not_approved_whatever_the_other_entries_say() {
    // example-two's windows/amd64 approval does not reach agentspace's list, and the reverse.
    let p = read_text(&doc(&[
        entry_with("agentspace", &[approval("linux/arm64", "2026-11-20")]),
        entry_with("example-two", &[approval("windows/amd64", "2026-11-26")]),
    ]));
    assert!(!approved(&p, "agentspace", "windows/amd64"));
    assert!(!approved(&p, "example-two", "linux/arm64"));
    assert!(approved(&p, "agentspace", "linux/arm64"));
    assert!(approved(&p, "example-two", "windows/amd64"));
}

// --- An approval of a platform agentspace does not support ------------------------------------

#[test]
fn an_unsupported_platform_may_be_listed_and_the_entry_is_valid() {
    // state.md: "Approving a platform agentspace does not support has no effect ... and the entry
    // is valid", a platform with a variant included.
    let p = read_text(SAMPLE_VARIANT);
    assert!(listed(&p, "agentspace"));
    // The supported platform beside it is approved as listed.
    assert!(approved(&p, "agentspace", "linux/arm64"));
    // A supported platform the entry does not list is not approved.
    assert!(!approved(&p, "agentspace", "linux/amd64"));
    assert!(!approved(&p, "agentspace", "windows/amd64"));
}

#[test]
fn an_entry_listing_an_unsupported_platform_alone_is_valid_and_approves_no_supported_one() {
    let p = read_text(&doc(&[entry_with(
        "agentspace",
        &[approval("windows/arm64", "2026-11-26")],
    )]));
    assert!(listed(&p, "agentspace"));
    for platform in ["linux/arm64", "linux/amd64", "windows/amd64"] {
        assert!(!approved(&p, "agentspace", platform), "{platform}");
    }
}

// --- Dates, and the form of the member --------------------------------------------------------

#[test]
fn an_approvals_date_is_checked_as_the_entrys_is() {
    for date in [
        "2026-02-30",
        "2026-04-31",
        "2026-02-29",
        "2026-13-01",
        "2026-11-20T00:00:00Z",
        "",
    ] {
        assert_refused(
            &format!("approval date {date:?}"),
            doc(&[entry_with("agentspace", &[approval("linux/arm64", date)])]),
        );
    }
}

#[test]
fn an_approvals_real_dates_are_accepted_and_a_future_one_too() {
    for date in ["2026-11-26", "2028-02-29", "2999-12-31", "0001-01-01"] {
        let p = read_text(&doc(&[entry_with(
            "agentspace",
            &[approval("linux/amd64", date)],
        )]));
        assert!(approved(&p, "agentspace", "linux/amd64"), "{date}");
    }
}

#[test]
fn two_approvals_of_one_supported_platform_are_not_valid_whatever_their_dates() {
    for platform in ["linux/arm64", "linux/amd64", "windows/amd64"] {
        assert_refused(
            &format!("{platform} twice, one date"),
            doc(&[entry_with(
                "agentspace",
                &[
                    approval(platform, "2026-11-20"),
                    approval(platform, "2026-11-20"),
                ],
            )]),
        );
        assert_refused(
            &format!("{platform} twice, two dates"),
            doc(&[entry_with(
                "agentspace",
                &[
                    approval(platform, "2026-11-20"),
                    approval(platform, "2026-11-26"),
                ],
            )]),
        );
    }
}

#[test]
fn two_approvals_of_one_unsupported_platform_with_two_dates_are_not_valid() {
    // The schema cannot refuse these; state.md's rule does, and a reader treats the file as one
    // that fails its schema.
    assert_refused(
        "windows/arm64 twice, two dates",
        doc(&[entry_with(
            "agentspace",
            &[
                approval("windows/arm64", "2026-11-20"),
                approval("windows/arm64", "2026-11-26"),
            ],
        )]),
    );
    assert_refused(
        "variant twice, two dates",
        doc(&[entry_with(
            "agentspace",
            &[
                approval("linux/amd64/v3", "2026-11-20"),
                approval("linux/amd64/v3", "2026-11-26"),
            ],
        )]),
    );
}

#[test]
fn the_same_platform_in_two_entries_is_two_approvals_and_valid() {
    let p = read_text(&doc(&[
        entry_with("agentspace", &[approval("windows/amd64", "2026-11-26")]),
        entry_with("example-two", &[approval("windows/amd64", "2026-11-26")]),
    ]));
    assert!(approved(&p, "agentspace", "windows/amd64"));
    assert!(approved(&p, "example-two", "windows/amd64"));
}

#[test]
fn an_empty_platforms_array_is_not_valid_and_does_not_mean_arm64_alone() {
    assert_refused(
        "platforms []",
        doc(&[
            "{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":[]}".to_string(),
        ]),
    );
}

#[test]
fn platforms_is_never_null_and_never_a_list_of_names() {
    assert_refused(
        "platforms null",
        doc(&[
            "{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":null}".to_string(),
        ]),
    );
    assert_refused(
        "platforms as names",
        doc(&[
            "{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":[\"linux/arm64\"]}"
                .to_string(),
        ]),
    );
    assert_refused(
        "platforms as an object",
        doc(&["{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":{\"platform\":\"linux/arm64\",\"approved\":\"2026-11-20\"}}".to_string()]),
    );
}

#[test]
fn an_approval_holds_exactly_a_platform_and_a_date() {
    assert_refused(
        "extra member",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":[{\"platform\":\"linux/arm64\",\"approved\":\"2026-11-20\",\"note\":\"x\"}]}]}",
    );
    assert_refused(
        "no date",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":[{\"platform\":\"linux/arm64\"}]}]}",
    );
    assert_refused(
        "no platform",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":[{\"approved\":\"2026-11-20\"}]}]}",
    );
}

#[test]
fn an_approvals_platform_must_be_in_the_normalized_form() {
    for platform in [
        "Linux/Arm64",
        "linux/aarch64",
        "linux/arm64/v8",
        "linux",
        "linux/",
        "/arm64",
        "linux-arm64",
        " linux/arm64",
        "linux/arm64\n",
        "",
    ] {
        assert_refused(
            &format!("platform {platform:?}"),
            doc(&[entry_with(
                "agentspace",
                &[approval(&platform.replace('\n', "\\n"), "2026-11-20")],
            )]),
        );
    }
}

#[test]
fn an_entry_still_needs_its_approved_date_beside_platforms() {
    assert_refused(
        "no entry date",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"platforms\":[{\"platform\":\"linux/arm64\",\"approved\":\"2026-11-20\"}]}]}",
    );
}

#[test]
fn a_member_named_twice_in_an_approval_is_not_valid() {
    assert_refused(
        "platform twice",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":[{\"platform\":\"linux/arm64\",\"platform\":\"linux/amd64\",\"approved\":\"2026-11-20\"}]}]}",
    );
    assert_refused(
        "platforms twice",
        "{\"version\":0,\"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\",\"platforms\":[{\"platform\":\"linux/arm64\",\"approved\":\"2026-11-20\"}],\"platforms\":[{\"platform\":\"linux/arm64\",\"approved\":\"2026-11-20\"}]}]}",
    );
}

#[test]
fn every_invalid_platforms_sample_is_refused() {
    for (label, text) in INVALID_SAMPLES {
        assert_refused(label, text);
    }
}

#[test]
#[should_panic(expected = "not valid was read")]
fn the_refusal_checker_fails_on_a_valid_list() {
    assert_refused("planted", SAMPLE_PLATFORMS);
}

#[test]
fn the_form_rules_hold_for_a_list_a_person_wrote_tolerantly() {
    // A byte order mark and CRLF are accepted with platforms present, as without.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(SAMPLE_PLATFORMS.replace('\n', "\r\n").as_bytes());
    let t = TempHome::new();
    t.put_projects(bytes);
    let p = read_projects(t.path()).expect("A byte order mark and CR LF are accepted");
    assert!(is_approved(&p, "example-two", "windows/amd64"));
    assert!(!is_approved(&p, "example-two", "linux/arm64"));
}

#[test]
fn a_list_with_platforms_is_left_byte_for_byte_by_a_read() {
    let t = TempHome::new();
    t.put_projects(SAMPLE_PLATFORMS);
    read_projects(t.path()).unwrap();
    assert_eq!(
        std::fs::read(t.projects_json()).unwrap(),
        SAMPLE_PLATFORMS.as_bytes()
    );
    assert_eq!(TempHome::names_in(&t.state()), ["projects.json"]);
}

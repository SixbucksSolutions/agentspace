//! The host registry keyed by project and platform (docs/state.md, The host registry, A host's
//! platform and names; D91, D101; E0.12-T02-S01, card #0250): a record's `platform`, read and
//! written, the validity rules that mention it, and the files v0.11 wrote read unchanged.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every expectation is written from
//! state.md and `schemas/state/hosts.state.schema.json`; a refused file or change is checked by
//! the not-valid error and by the file being left as it was.

mod common;

use agentspace_hosts::{Host, HostState, Hosts, HostsError, lock, read, write};
use common::*;

const SAMPLE_PLATFORMS: &str =
    include_str!("../../../schemas/state/samples/hosts.state.platforms.json");
const SAMPLE_ARM64_WRITTEN: &str =
    include_str!("../../../schemas/state/samples/hosts.state.platform-linux-arm64-written.json");

/// Every invalid sample v0.12's schema work added for the member, by name.
const INVALID_SAMPLES: &[(&str, &str)] = &[
    (
        "platform-aarch64",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.platform-aarch64.json"),
    ),
    (
        "platform-absent-with-other-stack",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.platform-absent-with-other-stack.json"
        ),
    ),
    (
        "platform-amd64-with-arm64-stack",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.platform-amd64-with-arm64-stack.json"
        ),
    ),
    (
        "platform-arm64-with-other-stack",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.platform-arm64-with-other-stack.json"
        ),
    ),
    (
        "platform-null",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.platform-null.json"),
    ),
    (
        "platform-uppercase",
        include_str!("../../../schemas/state/samples/invalid/hosts.state.platform-uppercase.json"),
    ),
    (
        "platform-windows-agentbox",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.platform-windows-agentbox.json"
        ),
    ),
    (
        "platform-with-variant",
        include_str!(
            "../../../schemas/state/samples/invalid/hosts.state.platform-with-variant.json"
        ),
    ),
];

const WINDOWS: &str = "windows/amd64";
const LINUX_AMD64: &str = "linux/amd64";
const ARM64: &str = "linux/arm64";
const WINDOWS_STACK: &str = "agentspace-windows-amd64-host-agentspace";
const LINUX_AMD64_STACK: &str = "agentspace-linux-amd64-host-agentspace";
const ARM64_STACK: &str = "agentspace-host-agentspace";

fn read_text(text: &str) -> Result<Hosts, HostsError> {
    let t = TempHome::new();
    t.put_hosts(text);
    read(t.path())
}

fn write_new(t: &TempHome, h: &Hosts) -> Result<(), HostsError> {
    let guard = lock(t.path())?;
    write(t.path(), h, &guard)
}

fn written_text(h: &Hosts) -> String {
    let t = TempHome::new();
    write_new(&t, h).expect("The change is valid and must be written");
    String::from_utf8(t.hosts_bytes()).unwrap()
}

fn assert_read_refused(label: &str, text: &str) {
    let t = TempHome::new();
    t.put_hosts(text);
    let before = t.hosts_bytes();
    match read(t.path()) {
        Ok(h) => panic!(
            "{label}: a file that is not valid was read as {} records",
            h.hosts.len()
        ),
        Err(e) => {
            assert!(is_not_valid(&e), "{label}: expected not valid, got {e:?}");
            assert_message(&e, t.path());
        }
    }
    assert_eq!(
        t.hosts_bytes(),
        before,
        "{label}: the file must be left alone"
    );
}

fn assert_write_refused(label: &str, h: &Hosts) {
    let t = TempHome::new();
    t.put_hosts(format!("{}\n", doc_of(&[BOX])));
    let before = t.hosts_bytes();
    match write_new(&t, h) {
        Ok(()) => panic!("{label}: a change that is not valid was written"),
        Err(e) => {
            assert!(is_write_refusal(&e), "{label}: unexpected error {e:?}");
            assert_message(&e, t.path());
        }
    }
    assert_eq!(
        t.hosts_bytes(),
        before,
        "{label}: a refused write must leave the old file"
    );
}

// ---------------------------------------------------------------------------------------------
// A record as a tuple, so each case is both a file to read and a change to write.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Rec {
    project: &'static str,
    platform: Option<&'static str>,
    stack: &'static str,
}

const BOX: Rec = Rec {
    project: "swimcoach",
    platform: None,
    stack: "agentbox",
};
const ARM64_HOST: Rec = Rec {
    project: "agentspace",
    platform: None,
    stack: ARM64_STACK,
};
const WINDOWS_HOST: Rec = Rec {
    project: "agentspace",
    platform: Some(WINDOWS),
    stack: WINDOWS_STACK,
};
const LINUX_AMD64_HOST: Rec = Rec {
    project: "agentspace",
    platform: Some(LINUX_AMD64),
    stack: LINUX_AMD64_STACK,
};

fn rec(project: &'static str, platform: Option<&'static str>, stack: &'static str) -> Rec {
    Rec {
        project,
        platform,
        stack,
    }
}

/// The record's bytes in state.md's member order, `ready`.
fn json_of(r: &Rec) -> String {
    let platform = match r.platform {
        Some(p) => format!("\"platform\":\"{p}\","),
        None => String::new(),
    };
    format!(
        "{{\"project\":\"{}\",{platform}\"stack\":\"{}\",\"state\":\"ready\"}}",
        r.project, r.stack
    )
}

/// A whole document, as a writer writes it, without the line feed.
fn doc_of(records: &[Rec]) -> String {
    let parts: Vec<String> = records.iter().map(json_of).collect();
    format!("{{\"version\":0,\"hosts\":[{}]}}", parts.join(","))
}

fn host_of(r: &Rec) -> Host {
    Host {
        platform: r.platform.map(str::to_string),
        stack: r.stack.to_string(),
        ..host(r.project, HostState::Ready)
    }
}

fn hosts_of(records: &[Rec]) -> Hosts {
    hosts(records.iter().map(host_of).collect())
}

// --- What a record's platform reads as --------------------------------------------------------

#[test]
fn a_record_without_platform_reads_as_linux_arm64() {
    let h = read_text(&doc_of(&[BOX, ARM64_HOST])).unwrap();
    for r in &h.hosts {
        assert_eq!(r.platform, None, "{}", r.project);
        assert_eq!(r.platform(), "linux/arm64", "{}", r.project);
    }
}

#[test]
fn a_record_with_platform_reads_it() {
    let h = read_text(&doc_of(&[BOX, WINDOWS_HOST])).unwrap();
    assert_eq!(h.hosts[1].platform.as_deref(), Some("windows/amd64"));
    assert_eq!(h.hosts[1].platform(), "windows/amd64");
    let h = read_text(&doc_of(&[BOX, LINUX_AMD64_HOST])).unwrap();
    assert_eq!(h.hosts[1].platform.as_deref(), Some("linux/amd64"));
    assert_eq!(h.hosts[1].platform(), "linux/amd64");
}

#[test]
fn an_explicit_linux_arm64_is_read_as_written_and_means_linux_arm64() {
    // state.md: a record that holds "platform":"linux/arm64" is valid, and is kept as it is.
    let explicit = rec("agentspace", Some(ARM64), ARM64_STACK);
    let h = read_text(&doc_of(&[BOX, explicit])).unwrap();
    assert_eq!(h.hosts[1].platform.as_deref(), Some("linux/arm64"));
    assert_eq!(h.hosts[1].platform(), "linux/arm64");
}

#[test]
fn the_missing_file_reads_as_the_box_with_no_platform() {
    let t = TempHome::new();
    let h = read(t.path()).unwrap();
    assert_eq!(h.hosts[0].platform, None);
    assert_eq!(h.hosts[0].platform(), "linux/arm64");
}

#[test]
fn the_platforms_sample_reads_each_records_platform() {
    let h = read_text(SAMPLE_PLATFORMS).unwrap();
    let platforms: Vec<Option<&str>> = h.hosts.iter().map(|r| r.platform.as_deref()).collect();
    assert_eq!(
        platforms,
        [None, None, Some("linux/amd64"), Some("windows/amd64")]
    );
    let meant: Vec<&str> = h.hosts.iter().map(Host::platform).collect();
    assert_eq!(
        meant,
        ["linux/arm64", "linux/arm64", "linux/amd64", "windows/amd64"]
    );
    // The records stay in the file's order, and the other members are read with them.
    let projects: Vec<&str> = h.hosts.iter().map(|r| r.project.as_str()).collect();
    assert_eq!(
        projects,
        ["swimcoach", "agentspace", "agentspace", "agentspace"]
    );
    assert_eq!(h.hosts[2].stack, "agentspace-linux-amd64-host-agentspace");
    assert_eq!(h.hosts[3].stack, "agentspace-windows-amd64-host-agentspace");
    assert_eq!(h.hosts[3].state, HostState::Creating);
}

#[test]
fn the_explicit_arm64_sample_reads_the_member() {
    let h = read_text(SAMPLE_ARM64_WRITTEN).unwrap();
    assert_eq!(h.hosts[0].platform, None);
    assert_eq!(h.hosts[1].platform.as_deref(), Some("linux/arm64"));
}

#[test]
fn a_reader_does_not_depend_on_member_order_for_the_platform() {
    let doc = "{\"hosts\":[{\"state\":\"creating\",\"stack\":\"agentspace-windows-amd64-host-agentspace\",\"platform\":\"windows/amd64\",\"project\":\"agentspace\"},{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}],\"version\":0}";
    let h = read_text(doc).unwrap();
    assert_eq!(h.hosts[0].platform.as_deref(), Some("windows/amd64"));
    assert_eq!(h.hosts[0].project, "agentspace");
}

#[test]
fn two_records_of_one_project_on_different_platforms_are_valid() {
    let all = [BOX, ARM64_HOST, LINUX_AMD64_HOST, WINDOWS_HOST];
    let h = read_text(&doc_of(&all)).expect("Three hosts of one project are three hosts");
    assert_eq!(h.hosts.len(), 4);
    // And a change that holds them is written.
    write_new(&TempHome::new(), &hosts_of(&all)).unwrap();
}

#[test]
fn an_explicit_arm64_and_another_platform_of_one_project_are_two_hosts_and_valid() {
    let all = [
        BOX,
        rec("agentspace", Some(ARM64), ARM64_STACK),
        WINDOWS_HOST,
    ];
    assert!(read_text(&doc_of(&all)).is_ok());
    write_new(&TempHome::new(), &hosts_of(&all)).unwrap();
}

#[test]
fn a_swimcoach_host_of_another_platform_is_a_record_like_any_other() {
    // state.md rule 3: only swimcoach's linux/arm64 host is the box.
    let all = [
        BOX,
        rec(
            "swimcoach",
            Some(WINDOWS),
            "agentspace-windows-amd64-host-swimcoach",
        ),
    ];
    assert!(read_text(&doc_of(&all)).is_ok());
    write_new(&TempHome::new(), &hosts_of(&all)).unwrap();
}

#[test]
fn the_box_may_hold_an_explicit_linux_arm64() {
    // Rule 3: the box's platform is linux/arm64, which an explicit member says too.
    let boxed = [rec("swimcoach", Some(ARM64), "agentbox")];
    let h = read_text(&doc_of(&boxed)).expect("An explicit linux/arm64 box is the box");
    assert_eq!(h.hosts[0].platform.as_deref(), Some("linux/arm64"));
    write_new(&TempHome::new(), &hosts_of(&boxed)).unwrap();
}

#[test]
fn a_record_of_a_platform_agentspace_does_not_support_is_valid_when_its_form_and_stack_are() {
    // state.md: "A record of a platform that agentspace does not support, which only a person's
    // edit can make, is valid when its form and its stack are".
    let odd = rec(
        "agentspace",
        Some("linux/riscv64"),
        "agentspace-linux-riscv64-host-agentspace",
    );
    let h = read_text(&doc_of(&[BOX, odd])).expect("Valid by its form and its stack");
    assert_eq!(h.hosts[1].platform(), "linux/riscv64");
    write_new(&TempHome::new(), &hosts_of(&[BOX, odd])).unwrap();
}

// --- What a writer writes ---------------------------------------------------------------------

#[test]
fn a_record_with_a_platform_is_written_after_its_project_and_before_its_stack() {
    let h = hosts_of(&[BOX, ARM64_HOST, WINDOWS_HOST]);
    // state.md's example, byte for byte.
    let want = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"platform\":\"windows/amd64\",\"stack\":\"agentspace-windows-amd64-host-agentspace\",\"state\":\"ready\"}]}\n";
    assert_eq!(written_text(&h), want);
}

#[test]
fn a_record_without_a_platform_is_written_without_the_member() {
    let text = written_text(&hosts_of(&[BOX, ARM64_HOST]));
    assert!(!text.contains("platform"), "{text}");
}

#[test]
fn a_written_linux_arm64_member_is_kept_exactly() {
    // state.md: no writer drops a person's "platform":"linux/arm64".
    let explicit = rec("agentspace", Some(ARM64), ARM64_STACK);
    let text = written_text(&hosts_of(&[BOX, explicit]));
    assert_eq!(
        text,
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"platform\":\"linux/arm64\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\"}]}\n"
    );
}

#[test]
fn a_rewrite_of_a_file_with_an_explicit_linux_arm64_keeps_it() {
    // Read it, write what was read, and the explicit member is still there, byte for byte.
    let compact = format!("{}\n", SAMPLE_ARM64_WRITTEN.trim_end());
    assert!(compact.contains("\"platform\":\"linux/arm64\""));
    let t = TempHome::new();
    t.put_hosts(&compact);
    let h = read(t.path()).unwrap();
    write_new(&t, &h).unwrap();
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), compact);
}

#[test]
fn a_rewrite_of_the_platforms_sample_keeps_every_record_and_its_platform_and_order() {
    let t = TempHome::new();
    t.put_hosts(SAMPLE_PLATFORMS);
    let h = read(t.path()).unwrap();
    write_new(&t, &h).unwrap();
    let text = String::from_utf8(t.hosts_bytes()).unwrap();
    assert_eq!(text.matches("\"platform\"").count(), 2, "{text}");
    assert!(text.contains(
        "{\"project\":\"agentspace\",\"platform\":\"linux/amd64\",\"stack\":\"agentspace-linux-amd64-host-agentspace\",\"state\":\"ready\","
    ));
    assert!(text.contains(
        "{\"project\":\"agentspace\",\"platform\":\"windows/amd64\",\"stack\":\"agentspace-windows-amd64-host-agentspace\",\"state\":\"creating\","
    ));
    assert_eq!(read(t.path()).unwrap(), h);
}

#[test]
fn a_record_added_for_another_platform_goes_at_the_end_and_no_record_moves() {
    // state.md: "a writer that adds a record appends it at the end of the array" (pin P46).
    let t = TempHome::new();
    t.put_hosts(format!("{}\n", doc_of(&[BOX, ARM64_HOST])));
    let mut h = read(t.path()).unwrap();
    h.hosts.push(host_of(&WINDOWS_HOST));
    write_new(&t, &h).unwrap();
    let back = read(t.path()).unwrap();
    assert_eq!(back.hosts.len(), 3);
    assert_eq!(back.hosts[0].project, "swimcoach");
    assert_eq!(back.hosts[1].platform(), "linux/arm64");
    assert_eq!(back.hosts[2].platform(), "windows/amd64");
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        format!("{}\n", doc_of(&[BOX, ARM64_HOST, WINDOWS_HOST]))
    );
}

#[test]
fn every_member_of_a_platform_record_is_written_in_the_tables_order() {
    let mut r = host_of(&WINDOWS_HOST);
    r.state = HostState::Ready;
    r.instance_id = Some("i-0123456789abcdef1".to_string());
    r.instance_type = Some("m8a.xlarge".to_string());
    r.last_attempt_at = Some("2026-11-27T18:00:00Z".to_string());
    r.last_success_at = Some("2026-11-27T18:09:30Z".to_string());
    let text = written_text(&with_box(vec![r]));
    assert!(
        text.contains(
            "{\"project\":\"agentspace\",\"platform\":\"windows/amd64\",\"stack\":\"agentspace-windows-amd64-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-0123456789abcdef1\",\"instance_type\":\"m8a.xlarge\",\"last_attempt_at\":\"2026-11-27T18:00:00Z\",\"last_success_at\":\"2026-11-27T18:09:30Z\"}"
        ),
        "{text}"
    );
}

#[test]
fn what_is_written_for_each_valid_case_is_the_cases_bytes_and_reads_back_the_same() {
    let cases: Vec<(&str, Vec<Rec>)> = vec![
        (
            "three platforms",
            vec![BOX, ARM64_HOST, LINUX_AMD64_HOST, WINDOWS_HOST],
        ),
        (
            "explicit arm64 box",
            vec![rec("swimcoach", Some(ARM64), "agentbox")],
        ),
        (
            "unsupported platform",
            vec![
                BOX,
                rec(
                    "agentspace",
                    Some("linux/riscv64"),
                    "agentspace-linux-riscv64-host-agentspace",
                ),
            ],
        ),
    ];
    for (label, records) in cases {
        let h = hosts_of(&records);
        let text = written_text(&h);
        assert_eq!(text, format!("{}\n", doc_of(&records)), "{label}");
        assert_eq!(read_text(&text).unwrap(), h, "{label}");
    }
}

// --- Files as v0.11 wrote them ----------------------------------------------------------------

/// A registry as v0.11 wrote it: every member but `platform`, in the table's order.
const V0_11_FILE: &str = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-0123456789abcdef0\",\"last_attempt_at\":\"2026-11-24T18:30:00Z\",\"last_success_at\":\"2026-11-24T18:41:07Z\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.large\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\",\"last_success_at\":\"2026-11-24T19:05:42Z\"},{\"project\":\"example-two\",\"stack\":\"agentspace-host-example-two\",\"state\":\"creating\",\"last_attempt_at\":\"2026-11-24T19:06:00Z\"},{\"project\":\"failed-one\",\"stack\":\"agentspace-host-failed-one\",\"state\":\"failed\",\"instance_id\":\"i-0123456789abcdef1\",\"instance_type\":\"c7g.large\",\"last_attempt_at\":\"2026-11-24T19:10:00Z\"}]}\n";

#[test]
fn a_file_v0_11_wrote_reads_with_no_platform_on_any_record() {
    let h = read_text(V0_11_FILE).unwrap();
    assert_eq!(h.hosts.len(), 4);
    for r in &h.hosts {
        assert_eq!(r.platform, None, "{}", r.project);
        assert_eq!(r.platform(), "linux/arm64", "{}", r.project);
    }
    assert_eq!(h.hosts[1].instance_type.as_deref(), Some("m9g.large"));
}

#[test]
fn a_file_v0_11_wrote_is_written_back_byte_for_byte_with_no_platform_added() {
    let t = TempHome::new();
    t.put_hosts(V0_11_FILE);
    let h = read(t.path()).unwrap();
    write_new(&t, &h).unwrap();
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), V0_11_FILE);
}

#[test]
fn the_v0_11_samples_read_with_no_platform_on_any_record() {
    for (label, text) in [
        (
            "full",
            include_str!("../../../schemas/state/samples/hosts.state.full.json"),
        ),
        (
            "creating-and-failed",
            include_str!("../../../schemas/state/samples/hosts.state.creating-and-failed.json"),
        ),
        (
            "seed",
            include_str!("../../../schemas/state/samples/hosts.state.seed.json"),
        ),
        (
            "instance-type",
            include_str!("../../../schemas/state/samples/hosts.state.instance-type.json"),
        ),
        (
            "last-success",
            include_str!("../../../schemas/state/samples/hosts.state.last-success.json"),
        ),
    ] {
        let h = read_text(text).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        for r in &h.hosts {
            assert_eq!(r.platform, None, "{label}: {}", r.project);
        }
    }
}

// --- Files and changes that are not valid -----------------------------------------------------

/// Each case: a label and the records, box first, whose file is not valid and whose change is not
/// to be written.
fn refused_cases() -> Vec<(&'static str, Vec<Rec>)> {
    vec![
        // Rule 1: two records share a project and a platform.
        (
            "two records of one project on windows/amd64",
            vec![BOX, WINDOWS_HOST, WINDOWS_HOST],
        ),
        (
            "two records of one project on linux/amd64",
            vec![BOX, LINUX_AMD64_HOST, LINUX_AMD64_HOST],
        ),
        (
            "two records of one project, none with a platform",
            vec![BOX, ARM64_HOST, ARM64_HOST],
        ),
        (
            "two records of one project, both holding linux/arm64",
            vec![
                BOX,
                rec("agentspace", Some(ARM64), ARM64_STACK),
                rec("agentspace", Some(ARM64), ARM64_STACK),
            ],
        ),
        (
            "one record with and one without linux/arm64",
            vec![BOX, ARM64_HOST, rec("agentspace", Some(ARM64), ARM64_STACK)],
        ),
        (
            "one record holding linux/arm64 and then one without",
            vec![BOX, rec("agentspace", Some(ARM64), ARM64_STACK), ARM64_HOST],
        ),
        (
            "two records of one unsupported platform",
            vec![
                BOX,
                rec(
                    "agentspace",
                    Some("linux/riscv64"),
                    "agentspace-linux-riscv64-host-agentspace",
                ),
                rec(
                    "agentspace",
                    Some("linux/riscv64"),
                    "agentspace-linux-riscv64-host-agentspace",
                ),
            ],
        ),
        // Rule 2: a stack that is not D91's name for the record's own project and platform.
        (
            "an arm64 record with another project's stack",
            vec![BOX, rec("agentspace", None, "agentspace-host-swimcoach")],
        ),
        (
            "an explicit arm64 record with another project's stack",
            vec![BOX, rec("agentspace", Some(ARM64), "agentspace-host-other")],
        ),
        (
            "a windows record with a linux platform in its stack",
            vec![BOX, rec("agentspace", Some(WINDOWS), LINUX_AMD64_STACK)],
        ),
        (
            "a linux/amd64 record with a windows platform in its stack",
            vec![BOX, rec("agentspace", Some(LINUX_AMD64), WINDOWS_STACK)],
        ),
        (
            "a windows record with another project's stack",
            vec![
                BOX,
                rec(
                    "agentspace",
                    Some(WINDOWS),
                    "agentspace-windows-amd64-host-swimcoach",
                ),
            ],
        ),
        (
            "a windows/arm64 record with a windows/amd64 stack",
            vec![BOX, rec("agentspace", Some("windows/arm64"), WINDOWS_STACK)],
        ),
        (
            "a linux/amd64 record with an arm64-form stack",
            vec![BOX, rec("agentspace", Some(LINUX_AMD64), ARM64_STACK)],
        ),
        (
            "an arm64 record with another platform's form of stack",
            vec![BOX, rec("agentspace", None, LINUX_AMD64_STACK)],
        ),
        // Rule 3: the box.
        ("no box record", vec![ARM64_HOST]),
        (
            "two box records",
            vec![BOX, rec("swimcoach", None, "agentbox")],
        ),
        (
            "a box record of another project",
            vec![rec("agentspace", None, "agentbox")],
        ),
        (
            "a box record on windows/amd64",
            vec![rec("swimcoach", Some(WINDOWS), "agentbox")],
        ),
        (
            "a box record on linux/amd64 beside none other",
            vec![rec("swimcoach", Some(LINUX_AMD64), "agentbox")],
        ),
        (
            "a box record on windows/amd64 beside the box",
            vec![BOX, rec("swimcoach", Some(WINDOWS), "agentbox")],
        ),
    ]
}

#[test]
fn every_file_that_breaks_a_validity_rule_is_refused_on_read() {
    for (label, records) in refused_cases() {
        assert_read_refused(label, &doc_of(&records));
    }
}

#[test]
fn every_change_that_breaks_a_validity_rule_is_refused_on_write_and_the_file_stays() {
    for (label, records) in refused_cases() {
        assert_write_refused(label, &hosts_of(&records));
    }
}

#[test]
fn every_invalid_platform_sample_is_refused() {
    for (label, text) in INVALID_SAMPLES {
        assert_read_refused(label, text);
    }
}

#[test]
fn a_platform_that_is_not_in_the_normalized_form_is_refused_on_write() {
    // A reader and a writer MUST NOT normalize: each of these is refused, none rewritten.
    for platform in [
        "Linux/AMD64",
        "linux/aarch64",
        "linux/amd64/v3",
        "linux",
        "linux/",
        "/amd64",
        "",
        " linux/amd64",
        "linux/amd64 ",
        "linux/amd64\n",
        "linux-amd64",
    ] {
        let r = Host {
            platform: Some(platform.to_string()),
            stack: LINUX_AMD64_STACK.to_string(),
            ..host("agentspace", HostState::Ready)
        };
        assert_write_refused(&format!("platform {platform:?}"), &with_box(vec![r]));
    }
}

#[test]
fn a_platform_with_a_line_feed_at_its_end_is_refused_on_read() {
    assert_read_refused(
        "platform ending in a line feed",
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"platform\":\"linux/amd64\\n\",\"stack\":\"agentspace-linux-amd64-host-agentspace\",\"state\":\"creating\"}]}",
    );
}

#[test]
fn a_platform_named_twice_in_a_record_is_refused_on_read() {
    assert_read_refused(
        "platform twice",
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"platform\":\"linux/amd64\",\"platform\":\"linux/amd64\",\"stack\":\"agentspace-linux-amd64-host-agentspace\",\"state\":\"creating\"}]}",
    );
}

// The two checkers above, shown to fail on a value they must not pass: a file or change that IS
// valid makes each panic, so a refusal that never came cannot read as one.

#[test]
#[should_panic(expected = "not valid was read")]
fn the_read_checker_fails_on_a_valid_file() {
    assert_read_refused("planted", &doc_of(&[BOX, WINDOWS_HOST]));
}

#[test]
#[should_panic(expected = "not valid was written")]
fn the_write_checker_fails_on_a_valid_change() {
    assert_write_refused("planted", &hosts_of(&[BOX, WINDOWS_HOST]));
}

#[test]
fn a_refused_change_with_a_platform_leaves_no_temporary_file() {
    let t = TempHome::new();
    t.put_hosts(format!("{}\n", doc_of(&[BOX])));
    let before = TempHome::names_in(&t.state());
    let bad = hosts_of(&[BOX, WINDOWS_HOST, WINDOWS_HOST]);
    assert!(write_new(&t, &bad).is_err());
    let after = TempHome::names_in(&t.state());
    let extra: Vec<&String> = after
        .iter()
        .filter(|n| !before.contains(n) && n.as_str() != "hosts.lock")
        .collect();
    assert!(extra.is_empty(), "Left {extra:?} behind");
}

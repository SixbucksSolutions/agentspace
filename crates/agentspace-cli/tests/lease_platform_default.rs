//! Each platform's default profile, and what a lease is asked as (E0.12-T03-S01, #0254).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (Platform, Profile, `create`, Versioning) and docs/driver.md (Place a
//! lease), with D93, never from the program's code (D31). The driver is a fake that logs every
//! call, and each test runs through `agentspace_cli::execute_in` in its own throwaway folder.
//!
//! The default is the platform's whether or not the caller names a profile, it is chosen by the CLI
//! before any driver is asked, and a stored record is printed as it was written, never re-derived
//! (lease-api.md, Profile; architect, 2026-11-26, for v0.12-T01, pins P2 and P25).

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use agentspace_driver_api::Placed;
use fake_driver::Call;
use lab::{ID1, ID2, Lab, NOW, Rec};
use serde_json::{Value, json};

/// What the lease CLI asks a driver, as the one place it made.
fn the_place(calls: &[Call]) -> (String, String, String) {
    assert_eq!(calls.len(), 1, "One place was asked: {calls:?}");
    match &calls[0] {
        Call::Place {
            platform,
            profile,
            size,
            ..
        } => (platform.clone(), profile.clone(), size.clone()),
        other => panic!("The one call is not a place: {other:?}"),
    }
}

/// A create with the given extra flags, run against a fresh fake. Returns the document it printed,
/// the record it stored and what it asked the driver for: platform, profile and size.
fn create(more: &[&str]) -> (Value, Value, (String, String, String)) {
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    // A driver that cannot place a lease answers pending, so the answer never decides a profile.
    h.script_place(Ok(Placed::Pending));
    let mut line = vec!["lease", "create", "--project", "swim"];
    line.extend_from_slice(more);
    let doc = lab.run(&line).lease_doc();
    let rec = lab.record_of(ID1).expect("The lease is recorded");
    (doc, rec, the_place(&h.calls()))
}

#[test]
fn a_lease_that_names_no_platform_and_no_profile_is_linux_arm64_with_agent_linux() {
    let (doc, rec, (platform, profile, _)) = create(&[]);
    assert_eq!(platform, "linux/arm64");
    assert_eq!(profile, "agent-linux");
    assert_eq!(doc["platform"], "linux/arm64");
    assert_eq!(doc["profile"], "agent-linux");
    assert_eq!(rec["profile"], "agent-linux");
}

#[test]
fn linux_amd64_with_no_profile_gets_agent_linux() {
    let (doc, rec, (platform, profile, _)) = create(&["--platform", "linux/amd64"]);
    assert_eq!(platform, "linux/amd64");
    assert_eq!(
        profile, "agent-linux",
        "The driver is asked for agent-linux"
    );
    assert_eq!(doc["platform"], "linux/amd64");
    assert_eq!(doc["profile"], "agent-linux");
    assert_eq!(rec["platform"], "linux/amd64");
    assert_eq!(
        rec["profile"], "agent-linux",
        "The record keeps what was printed"
    );
}

#[test]
fn windows_amd64_with_no_profile_gets_agent_windows() {
    // D93: before v0.12 this was unsatisfiable with agent-linux; the default is part of supporting
    // the platform.
    let (doc, rec, (platform, profile, _)) = create(&["--platform", "windows/amd64"]);
    assert_eq!(platform, "windows/amd64");
    assert_eq!(
        profile, "agent-windows",
        "The driver is asked for agent-windows"
    );
    assert_eq!(doc["platform"], "windows/amd64");
    assert_eq!(doc["profile"], "agent-windows");
    assert_eq!(rec["platform"], "windows/amd64");
    assert_eq!(
        rec["profile"], "agent-windows",
        "The record keeps what was printed"
    );
}

#[test]
fn a_platform_agentspace_does_not_support_keeps_agent_linux_as_its_default() {
    // lease-api.md, Profile: "A platform agentspace does not support has no default profile of its
    // own, so a lease on one that names no profile prints agent-linux" (pin P2). windows/arm64 is
    // D98's.
    for platform in ["windows/arm64", "darwin/arm64", "linux/amd64/v3"] {
        let (doc, rec, (asked_platform, profile, _)) = create(&["--platform", platform]);
        assert_eq!(asked_platform, platform);
        assert_eq!(profile, "agent-linux", "{platform}");
        assert_eq!(doc["profile"], "agent-linux", "{platform}");
        assert_eq!(rec["profile"], "agent-linux", "{platform}");
    }
}

#[test]
fn the_default_follows_the_normalized_platform_not_the_text_given() {
    // The default is chosen after normalization: x86_64 is amd64, aarch64 is arm64, case and
    // whitespace go, and the variant v8 of arm64 is dropped (lease-api.md, Platform).
    let cases = [
        (" Windows/AMD64 ", "windows/amd64", "agent-windows"),
        ("windows/x86_64", "windows/amd64", "agent-windows"),
        ("windows/x86-64", "windows/amd64", "agent-windows"),
        ("Linux/X86_64", "linux/amd64", "agent-linux"),
        ("linux/aarch64/v8", "linux/arm64", "agent-linux"),
        ("windows/aarch64", "windows/arm64", "agent-linux"),
    ];
    for (given, platform, profile) in cases {
        let (doc, _, (asked_platform, asked_profile, _)) = create(&["--platform", given]);
        assert_eq!(asked_platform, platform, "{given:?}");
        assert_eq!(asked_profile, profile, "{given:?}");
        assert_eq!(doc["platform"], platform, "{given:?}");
        assert_eq!(doc["profile"], profile, "{given:?}");
    }
}

#[test]
fn a_profile_that_is_named_is_asked_as_it_is_whatever_the_platforms_default() {
    // The default fills a gap and never replaces a value: a driver judges what was named.
    let cases = [
        ("windows/amd64", "agent-linux", "agent-linux"),
        ("linux/amd64", "agent-windows", "agent-windows"),
        ("linux/arm64", "agent-windows", "agent-windows"),
        ("windows/amd64", "Agent-Windows@03", "agent-windows@3"),
        ("windows/arm64", "agent-windows", "agent-windows"),
    ];
    for (platform, given, profile) in cases {
        let (doc, rec, (asked_platform, asked_profile, _)) =
            create(&["--platform", platform, "--profile", given]);
        assert_eq!(asked_platform, platform, "{platform} {given}");
        assert_eq!(asked_profile, profile, "{platform} {given}");
        assert_eq!(doc["profile"], profile, "{platform} {given}");
        assert_eq!(rec["profile"], profile, "{platform} {given}");
    }
}

#[test]
fn a_profile_named_with_no_platform_is_judged_on_linux_arm64() {
    let (doc, _, (platform, profile, _)) = create(&["--profile", "agent-windows"]);
    assert_eq!(platform, "linux/arm64");
    assert_eq!(profile, "agent-windows");
    assert_eq!(doc["platform"], "linux/arm64");
    assert_eq!(doc["profile"], "agent-windows");
}

#[test]
fn a_size_does_not_change_the_default() {
    for size in ["s", "m", "l", "xl", "xxl"] {
        let (doc, _, (_, profile, asked_size)) =
            create(&["--platform", "windows/amd64", "--size", size]);
        assert_eq!(profile, "agent-windows", "{size}");
        assert_eq!(asked_size, size);
        assert_eq!(doc["profile"], "agent-windows", "{size}");
    }
}

#[test]
fn an_unsatisfiable_windows_amd64_lease_with_no_profile_prints_agent_windows() {
    // Whatever the driver answers, the default the CLI printed is agent-windows.
    let (mut lab, h) = Lab::with_fake();
    lab.script_ids(&[ID1]);
    h.script_place(Ok(Placed::Unsatisfiable {
        reasons: vec![fake_driver::reason(
            "unsupported_size",
            "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl.",
        )],
    }));
    let doc = lab
        .run(&[
            "lease",
            "create",
            "--project",
            "swim",
            "--platform",
            "windows/amd64",
            "--size",
            "xxl",
        ])
        .lease_doc();
    assert_eq!(doc["state"], "unsatisfiable");
    assert_eq!(doc["platform"], "windows/amd64");
    assert_eq!(doc["profile"], "agent-windows");
    assert_eq!(doc["size"], "xxl");
}

// ---- A stored record is printed as it was written ---------------------------------------------

/// A record written before v0.12 for `--platform windows/amd64` with no profile: unsatisfiable,
/// with agent-linux, and the reasons v2 gave it.
fn old_windows_record(id: &str) -> Rec {
    let mut r = Rec::unsatisfiable(id, "swim");
    r.platform = "windows/amd64".to_string();
    r.profile = "agent-linux".to_string();
    r.size = "m".to_string();
    r.reasons = vec![(
        "unsupported_platform".to_string(),
        "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only.".to_string(),
    )];
    r
}

#[test]
fn get_and_list_print_a_stored_windows_lease_with_the_profile_it_was_written_with() {
    // lease-api.md, Profile (pin P25): "a lease created before v0.12 with --platform windows/amd64
    // and no --profile keeps agent-linux and stays unsatisfiable".
    let lab = Lab::new();
    let rec = old_windows_record(ID1);
    lab.seed(std::slice::from_ref(&rec));
    let before = lab.state_bytes();
    let got = lab.run(&["lease", "get", ID1]).lease_doc();
    assert_eq!(got["profile"], "agent-linux");
    assert_eq!(got["state"], "unsatisfiable");
    assert_eq!(got, rec.doc(NOW));
    let listed = lab.run(&["lease", "list"]).list_doc();
    let item = listed["leases"]
        .as_array()
        .expect("list has leases")
        .iter()
        .find(|l| l["id"] == ID1)
        .expect("The lease is listed")
        .clone();
    assert_eq!(item["profile"], "agent-linux");
    assert_eq!(
        lab.state_bytes(),
        before,
        "Nothing was re-derived or written"
    );
}

#[test]
fn a_stored_ready_windows_lease_keeps_its_profile_through_renew_and_release() {
    let (mut lab, h) = Lab::with_fake();
    let mut rec = Rec::ready(ID2, "swim");
    rec.platform = "windows/amd64".to_string();
    rec.profile = "agent-windows".to_string();
    rec.ssh = Some((fake_driver::alias_of("swim", ID2), "agent".to_string()));
    lab.seed(std::slice::from_ref(&rec));
    lab.set_now(NOW);
    h.script_renew(Ok(NOW + 7200));
    let renewed = lab.run(&["lease", "renew", ID2, "--ttl", "2h"]).lease_doc();
    assert_eq!(renewed["platform"], "windows/amd64");
    assert_eq!(renewed["profile"], "agent-windows");
    assert_eq!(
        renewed["ssh"],
        json!({"host_alias": rec.alias(), "user": "agent"})
    );
    let released = lab.run(&["lease", "release", ID2]).lease_doc();
    assert_eq!(released["state"], "released");
    assert_eq!(released["platform"], "windows/amd64");
    assert_eq!(released["profile"], "agent-windows");
}

//! Place by project and platform: the three platforms, each one's profile, login user and size
//! column, the project list's approval of a platform, and a host of each platform of its own
//! (docs/driver.md, What it supports, Sizes and instance types, Where it places a lease, On a
//! `ready` host and Starting a provisioning run, and the v0.12 conformance cases M1 to M14;
//! docs/decisions.md, D91, D93, D94, D95, D98; E0.12-T03-S01, #0254).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. Every start goes
//! through a fake `Detach`, so no test starts a process, every file is in a throwaway folder, and
//! no test reaches a real host or calls AWS. Every message is written out here from the page, and
//! never read from the program.
#![cfg(windows)]

mod common;
#[path = "common/pending.rs"]
mod pending;

use agentspace_driver_api::{Driver, Failure, LeaseRef, Placed, Reason};
use agentspace_hosts::{HostKey, HostState};
use common::*;
use pending::*;
use std::sync::Arc;

const PROJECT: &str = "agentspace";
const ARM: &str = "linux/arm64";
const AMD: &str = "linux/amd64";
const WIN: &str = "windows/amd64";

/// A home with a CA and the operator program installed, and a fake start that records each start.
fn world() -> (Fx, Arc<FakeDetach>) {
    let mut fx = Fx::with_ca();
    fx.install_operator();
    (fx, FakeDetach::new())
}

fn codes(reasons: &[Reason]) -> Vec<&'static str> {
    reasons.iter().map(|r| r.code).collect()
}

/// The reasons of an `unsatisfiable` answer, or a panic that says what came instead.
#[track_caller]
fn unsatisfiable(got: Result<Placed, Failure>) -> Vec<Reason> {
    match got {
        Ok(Placed::Unsatisfiable { reasons }) => reasons,
        other => panic!("Expected unsatisfiable, got {other:?}"),
    }
}

/// The lease placed `ready`, on the host of `platform`: its alias, the platform's login user, and
/// every file of the credential, with the certificate's principal and the alias file's bytes.
#[track_caller]
fn assert_ready_on(fx: &Fx, got: Result<Placed, Failure>, id: &str, platform: &str) {
    let user = login_user_of(platform);
    match got {
        Ok(Placed::Ready {
            host_alias,
            user: got_user,
            expires_at,
        }) => {
            assert_eq!(host_alias, alias_for(PROJECT, id), "{platform}");
            assert_eq!(got_user, user, "{platform}: the returned user");
            assert!(expires_at > now(), "{platform}");
        }
        other => panic!("{platform}: expected ready, got {other:?}"),
    }
    let cert = cert_info(&fx.lease_dir(id).join("id_ed25519-cert.pub"));
    assert_eq!(cert.principals, [user], "{platform}: the principal");
    assert_eq!(cert.key_id, id, "{platform}");
    assert_alias_bytes_on(fx, id, PROJECT, platform);
}

// ---- The reasons' messages, byte for byte ---------------------------------------------------------------

const MSG_PLATFORM_ARM: &str = "Platform 'windows/arm64' is not supported; agentspace supports linux/arm64, linux/amd64 and windows/amd64.";

#[test]
fn an_unsupported_platform_with_each_of_the_three_defaults_lists_the_platform_alone() {
    // M2: windows/arm64 with the default profile, with agent-linux and with agent-windows; each
    // lists `unsupported_platform` alone, with the v0.12 message.
    let (fx, detach) = world();
    let d = fx.driver_with(&detach);
    for profile in ["agent-linux", "agent-windows"] {
        let reasons = unsatisfiable(fx.place_on(&d, PROJECT, ID, "windows/arm64", profile, "m"));
        assert_eq!(codes(&reasons), ["unsupported_platform"], "{profile}");
        assert_eq!(reasons[0].message, MSG_PLATFORM_ARM, "{profile}");
    }
    assert_eq!(detach.count(), 0);
}

#[test]
fn a_platform_with_a_variant_is_not_the_platform_without_it() {
    // M2, pin P21: linux/amd64/v3 is unsupported_platform, with its own value quoted.
    let (fx, detach) = world();
    let d = fx.driver_with(&detach);
    let reasons = unsatisfiable(fx.place_on(&d, PROJECT, ID, "linux/amd64/v3", "agent-linux", "m"));
    assert_eq!(codes(&reasons), ["unsupported_platform"]);
    assert_eq!(
        reasons[0].message,
        "Platform 'linux/amd64/v3' is not supported; agentspace supports linux/arm64, linux/amd64 and windows/amd64."
    );
}

#[test]
fn an_unsupported_platform_with_an_unsupported_profile_and_size_lists_all_three_in_order() {
    // M2: agent-linux@1 with xxl on windows/arm64.
    let (fx, detach) = world();
    let d = fx.driver_with(&detach);
    let reasons =
        unsatisfiable(fx.place_on(&d, PROJECT, ID, "windows/arm64", "agent-linux@1", "xxl"));
    assert_eq!(
        codes(&reasons),
        [
            "unsupported_platform",
            "unsupported_profile",
            "unsupported_size"
        ]
    );
    assert_eq!(reasons[0].message, MSG_PLATFORM_ARM);
    assert_eq!(
        reasons[1].message,
        "Profile 'agent-linux@1' is not supported on platform 'windows/arm64'; agentspace supports agent-linux on linux/arm64 and linux/amd64, and agent-windows on windows/amd64, each with no version only."
    );
    assert_eq!(
        reasons[2].message,
        "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl."
    );
}

#[test]
fn a_profile_that_is_not_the_platforms_own_is_unsupported_profile_alone() {
    // M3, D93: the profile's message names the request's profile and platform.
    let cases = [
        (WIN, "agent-linux"),
        (AMD, "agent-windows"),
        (ARM, "agent-windows"),
    ];
    for (platform, profile) in cases {
        let (fx, detach) = world();
        let d = fx.driver_with(&detach);
        let reasons = unsatisfiable(fx.place_on(&d, PROJECT, ID, platform, profile, "m"));
        assert_eq!(
            codes(&reasons),
            ["unsupported_profile"],
            "{platform} {profile}"
        );
        assert_eq!(
            reasons[0].message,
            format!(
                "Profile '{profile}' is not supported on platform '{platform}'; agentspace supports agent-linux on linux/arm64 and linux/amd64, and agent-windows on windows/amd64, each with no version only."
            ),
            "{platform} {profile}"
        );
        assert_eq!(detach.count(), 0);
    }
}

#[test]
fn a_versioned_profile_is_unsupported_on_every_supported_platform() {
    // Any profile with a version is unsupported on every platform (pin P21).
    let (fx, detach) = world();
    let d = fx.driver_with(&detach);
    for platform in [ARM, AMD, WIN] {
        for profile in ["agent-linux@1", "agent-windows@1", "agent-windows@12"] {
            let reasons = unsatisfiable(fx.place_on(&d, PROJECT, ID, platform, profile, "m"));
            assert_eq!(
                codes(&reasons),
                ["unsupported_profile"],
                "{platform} {profile}"
            );
        }
    }
}

#[test]
fn the_size_is_judged_on_its_own_on_every_platform() {
    // The size's message keeps v0's wording on every platform, and a supported platform and
    // profile with an unsupported size lists unsupported_size alone.
    let (fx, detach) = world();
    let d = fx.driver_with(&detach);
    for platform in [ARM, AMD, WIN] {
        let reasons = unsatisfiable(fx.place_default_on(&d, PROJECT, ID, platform, "xxl"));
        assert_eq!(codes(&reasons), ["unsupported_size"], "{platform}");
        assert_eq!(
            reasons[0].message, "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl.",
            "{platform}"
        );
    }
}

#[test]
fn an_unsupported_request_needs_no_ca_and_reads_neither_file() {
    // M2 and M3: no CA, a hosts.json and a projects.json that do not validate, and the answers
    // are the same; nothing is written and nothing is started.
    let mut fx = Fx::without_ca();
    fx.install_operator();
    let detach = FakeDetach::new();
    fx.write_hosts_raw(b"not json");
    fx.write_projects_raw(b"not json");
    let hosts_before = fx.hosts_bytes();
    let before = fx.snapshot();
    let d = fx.driver_with(&detach);
    let lines = [
        ("windows/arm64", "agent-linux", "m"),
        ("windows/arm64", "agent-windows", "m"),
        (WIN, "agent-linux", "m"),
        (AMD, "agent-windows", "m"),
        (ARM, "agent-windows", "m"),
        (WIN, "agent-windows", "xxl"),
    ];
    for (platform, profile, size) in lines {
        let reasons = unsatisfiable(fx.place_on(&d, PROJECT, ID, platform, profile, size));
        assert!(!reasons.is_empty(), "{platform} {profile} {size}");
    }
    assert_eq!(fx.hosts_bytes(), hosts_before);
    fx.assert_unchanged(&before, "An unsupported place");
    assert_eq!(detach.count(), 0);
}

// ---- A lease is placed by its own platform's host ---------------------------------------------------------

#[test]
fn a_ready_host_serves_a_lease_on_its_platform_as_that_platforms_user() {
    // M1, D95: each platform's host, `ready` at the platform's `m`, places a default lease `ready`
    // at once, with the platform's user as the certificate's principal, the returned user and the
    // alias's login user, and the host's own key name; it starts nothing and writes no registry.
    let cases = [(ARM, "m9g.xlarge"), (AMD, U_M), (WIN, U_M)];
    for (platform, instance_type) in cases {
        let (fx, detach) = world();
        fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD, WIN])]);
        fx.write_hosts(&[H::ready(PROJECT).on(platform).typed(instance_type)]);
        let hosts_before = fx.hosts_bytes();
        let d = fx.driver_with(&detach);
        let got = fx.place_default_on(&d, PROJECT, ID, platform, "m");
        assert_ready_on(&fx, got, ID, platform);
        assert_eq!(detach.count(), 0, "{platform}");
        assert_eq!(fx.hosts_bytes(), hosts_before, "{platform}");
        // ssh reads the alias back with the user and the host-key name of that host.
        let out = fx.ssh_g(&alias_for(PROJECT, ID));
        assert_eq!(
            values(&out, "user"),
            [login_user_of(platform)],
            "{platform}\n{out}"
        );
        assert_eq!(
            values(&out, "hostkeyalias"),
            [host_key_name(PROJECT, platform)],
            "{platform}\n{out}"
        );
    }
}

#[test]
fn the_certificate_principal_is_the_platforms_user_and_not_ubuntu_on_windows() {
    // The one place the three platforms differ in a certificate (D95; pin P19). Fails if the
    // driver signs every lease for `ubuntu`.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[WIN])]);
    fx.write_hosts(&[H::ready(PROJECT).on(WIN).typed(U_M)]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, WIN, "m");
    assert!(
        matches!(&got, Ok(Placed::Ready { user, .. }) if user == "agent"),
        "{got:?}"
    );
    let cert = cert_info(&fx.cert_of(ID));
    assert_eq!(cert.principals, ["agent"]);
}

impl Fx {
    fn cert_of(&self, id: &str) -> std::path::PathBuf {
        self.lease_dir(id).join("id_ed25519-cert.pub")
    }
}

#[test]
fn a_ready_linux_arm64_host_does_not_meet_a_lease_on_another_platform() {
    // D91: with only the arm64 host `ready`, a linux/amd64 lease and a windows/amd64 lease are each
    // `pending`, never `ready`; each makes its own record and starts its own run (M6).
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD, WIN])]);
    fx.write_hosts(&[H::ready(PROJECT)]);
    let arm_record_before = fx.host_on(PROJECT, ARM).expect("The arm64 record");
    let d = fx.driver_with(&detach);
    for (n, platform) in [(1u32, AMD), (2, WIN)] {
        let id = id_n(n);
        let got = fx.place_default_on(&d, PROJECT, &id, platform, "m");
        assert!(matches!(got, Ok(Placed::Pending)), "{platform}: {got:?}");
        assert!(
            !fx.has_files_of(PROJECT, &id),
            "{platform}: pending issues nothing"
        );
    }
    let starts = detach.all();
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0].args, run_args(PROJECT, AMD));
    assert_eq!(starts[1].args, run_args(PROJECT, WIN));
    assert_eq!(starts[0].args.len(), 5);
    assert_eq!(
        fx.host_on(PROJECT, ARM),
        Some(arm_record_before),
        "The arm64 record is as it was"
    );
    let amd = fx
        .host_on(PROJECT, AMD)
        .expect("A linux/amd64 record was added");
    assert_eq!(amd.state, HostState::Creating);
    assert_eq!(amd.stack, "agentspace-linux-amd64-host-agentspace");
    let win = fx
        .host_on(PROJECT, WIN)
        .expect("A windows/amd64 record was added");
    assert_eq!(win.state, HostState::Creating);
    assert_eq!(win.stack, "agentspace-windows-amd64-host-agentspace");
}

#[test]
fn a_new_platforms_record_is_appended_after_the_others_and_nothing_before_it_changes() {
    // M6, pin P46: the registry's bytes are the box's record, the arm64 record as it was, then the
    // new record with `platform`, the stack of D91's table, `creating` and `last_attempt_at`.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, WIN])]);
    fx.write_hosts(&[H::ready(PROJECT)]);
    let d = fx.driver_with(&detach);
    let t0 = now();
    let got = fx.place_default_on(&d, PROJECT, ID, WIN, "m");
    let t1 = now();
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    let record = fx.host_on(PROJECT, WIN).expect("The new record");
    let at = epoch_of(record.last_attempt_at.as_deref().expect("last_attempt_at"));
    assert!(t0 <= at && at <= t1);
    assert_eq!(
        fx.hosts_bytes().unwrap(),
        hosts_text(&[H::ready(PROJECT), H::creating(PROJECT).on(WIN).attempt(at)]).into_bytes()
    );
    let start = detach.only();
    assert_eq!(
        start.args,
        ["provision", "-Host", PROJECT, "-Platform", WIN]
    );
    assert_eq!(start.program, fx.ctx.operator_program.clone().unwrap());
}

#[test]
fn a_windows_run_has_a_folder_of_its_own_and_the_arm64_folder_is_not_made() {
    // M6: `hosts\<project>.windows-amd64\` is made and `hosts\<project>\` is not.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[WIN])]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, WIN, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    let own = agentspace_hosts::host_dir_keyed(&fx.home, &HostKey::new(PROJECT, WIN));
    assert!(own.is_dir(), "The platform's own host folder is made");
    assert!(
        own.ends_with("agentspace.windows-amd64"),
        "The folder is named <project>.<os>-<arch>"
    );
    assert!(
        !agentspace_hosts::host_dir_keyed(&fx.home, &HostKey::new(PROJECT, ARM)).exists(),
        "The arm64 folder is not made"
    );
}

#[test]
fn a_linux_arm64_lease_still_starts_three_arguments_and_adds_no_platform_member() {
    // A run for linux/arm64 is the three arguments, byte for byte as before v0.12 (pin P30), and
    // its record has no `platform` (pin P16).
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::plain(PROJECT)]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, ARM, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(detach.only().args, ["provision", "-Host", PROJECT]);
    let text = String::from_utf8(fx.hosts_bytes().unwrap()).unwrap();
    assert!(!text.contains("\"platform\""), "{text}");
    assert!(
        text.contains("\"stack\":\"agentspace-host-agentspace\""),
        "{text}"
    );
}

// ---- The project list approves each platform on its own ----------------------------------------------------

fn no_host_message(project: &str) -> String {
    format!(
        "No capacity can be found for project '{project}': it has no host and is not on agentspace's project list."
    )
}

fn not_approved_message(project: &str, platform: &str) -> String {
    format!(
        "No capacity can be found for project '{project}' on platform '{platform}': it has no host there and is not approved for that platform on agentspace's project list."
    )
}

/// A `no_capacity` answer with exactly this message, and nothing written or started.
#[track_caller]
fn assert_no_capacity(
    fx: &Fx,
    detach: &Arc<FakeDetach>,
    got: Result<Placed, Failure>,
    message: &str,
) {
    let reasons = unsatisfiable(got);
    assert_eq!(codes(&reasons), ["no_capacity"]);
    assert_eq!(reasons[0].message, message);
    assert_eq!(detach.count(), 0, "Nothing is started");
    assert!(!fx.has_files_of(PROJECT, ID), "Nothing is issued");
}

#[test]
fn an_entry_with_no_platforms_approves_linux_arm64_alone() {
    // An entry without `platforms`: arm64 is pending; linux/amd64 and windows/amd64 are
    // no_capacity with the message that names the platform (M4, pin P3, pin P22).
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::plain(PROJECT)]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, ARM, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(detach.count(), 1);
    for platform in [AMD, WIN] {
        let (fx, detach) = world();
        fx.write_project_entries(&[PE::plain(PROJECT)]);
        let d = fx.driver_with(&detach);
        let got = fx.place_default_on(&d, PROJECT, ID, platform, "m");
        assert_no_capacity(&fx, &detach, got, &not_approved_message(PROJECT, platform));
        assert!(
            fx.hosts_bytes().is_none(),
            "{platform}: nothing is written to the registry"
        );
    }
}

#[test]
fn a_project_with_no_entry_gets_the_v0_07_message_on_every_platform() {
    // M4 and M5: no entry names the project, so the message is v0.07's, whatever platform was asked.
    for platform in [ARM, AMD, WIN] {
        let (fx, detach) = world();
        fx.write_project_entries(&[PE::plain("someone-else")]);
        let d = fx.driver_with(&detach);
        let got = fx.place_default_on(&d, PROJECT, ID, platform, "m");
        assert_no_capacity(&fx, &detach, got, &no_host_message(PROJECT));
        assert!(fx.hosts_bytes().is_none(), "{platform}");
    }
}

#[test]
fn an_entry_that_omits_linux_arm64_does_not_approve_it() {
    // M5: an entry whose `platforms` approves linux/amd64 alone: the default platform is
    // no_capacity with the v0.12 message for linux/arm64, never v0.07's.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[AMD])]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, ARM, "m");
    assert_no_capacity(&fx, &detach, got, &not_approved_message(PROJECT, ARM));
    assert!(fx.hosts_bytes().is_none());
    // And linux/amd64 is approved.
    let got = fx.place_default_on(&d, PROJECT, id_n(2).as_str(), AMD, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
}

#[test]
fn each_platform_in_an_entry_is_approved_on_its_own() {
    for approved in [ARM, AMD, WIN] {
        for asked in [ARM, AMD, WIN] {
            let (fx, detach) = world();
            fx.write_project_entries(&[PE::approving(PROJECT, &[approved])]);
            let d = fx.driver_with(&detach);
            let got = fx.place_default_on(&d, PROJECT, ID, asked, "m");
            if asked == approved {
                assert!(matches!(got, Ok(Placed::Pending)), "{asked}: {got:?}");
                assert_eq!(detach.count(), 1, "{asked}");
            } else {
                assert_no_capacity(&fx, &detach, got, &not_approved_message(PROJECT, asked));
            }
        }
    }
}

#[test]
fn a_host_on_another_platform_does_not_stand_for_an_approval() {
    // M4: the project's linux/arm64 record is `ready` and its entry has no `platforms`:
    // `create --platform linux/amd64` is no_capacity with the v0.12 message; it makes no lease
    // folder or alias, starts nothing, and leaves the registry as it was; the default platform is
    // `ready`.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::plain(PROJECT)]);
    fx.write_hosts(&[H::ready(PROJECT)]);
    let hosts_before = fx.hosts_bytes();
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, AMD, "m");
    assert_no_capacity(&fx, &detach, got, &not_approved_message(PROJECT, AMD));
    assert_eq!(fx.hosts_bytes(), hosts_before);
    // With no entry for the project, the message is v0.07's.
    fx.write_project_entries(&[PE::plain("someone-else")]);
    let got = fx.place_default_on(&d, PROJECT, ID, AMD, "m");
    assert_no_capacity(&fx, &detach, got, &no_host_message(PROJECT));
    // The default platform is placed by its own record.
    let got = fx.place_default_on(&d, PROJECT, ID, ARM, "m");
    assert_ready_on(&fx, got, ID, ARM);
}

#[test]
fn a_record_on_the_platform_places_the_lease_whatever_the_list_says() {
    // Pin P3: the list governs only making a host.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::plain("someone-else")]);
    fx.write_hosts(&[H::ready(PROJECT).on(WIN).typed(U_M)]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, WIN, "m");
    assert_ready_on(&fx, got, ID, WIN);
}

#[test]
fn a_project_list_that_cannot_be_read_fails_only_a_lease_on_a_platform_with_no_host() {
    // M13: the project's linux/arm64 record is `ready` and projects.json does not validate: the
    // default platform is `ready`; linux/amd64 fails with `internal`, records nothing, starts
    // nothing and leaves the registry as it was.
    let (fx, detach) = world();
    fx.write_hosts(&[H::ready(PROJECT)]);
    fx.write_projects_raw(b"not json");
    let hosts_before = fx.hosts_bytes();
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, ARM, "m");
    assert_ready_on(&fx, got, ID, ARM);
    let id2 = id_n(2);
    let got = fx.place_default_on(&d, PROJECT, &id2, AMD, "m");
    let f = got.expect_err("A list that cannot be read fails the lease");
    fx.assert_failure_plain(&f);
    assert!(!fx.has_files_of(PROJECT, &id2));
    assert_eq!(detach.count(), 0);
    assert_eq!(fx.hosts_bytes(), hosts_before);
}

#[test]
fn swimcoach_on_another_platform_is_no_capacity_with_v0_07s_message_and_no_registry() {
    // M14: with no hosts.json and no projects.json, linux/amd64 for swimcoach is no_capacity with
    // v0.07's message; hosts.json stays absent; the default platform is `ready`.
    let (fx, detach) = world();
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, "swimcoach", ID, AMD, "m");
    let reasons = unsatisfiable(got);
    assert_eq!(codes(&reasons), ["no_capacity"]);
    assert_eq!(reasons[0].message, no_host_message("swimcoach"));
    assert!(fx.hosts_bytes().is_none());
    assert_eq!(detach.count(), 0);
}

// ---- The registry by project and platform ------------------------------------------------------------------

#[test]
fn two_records_of_one_project_and_platform_fail_a_supported_place_with_nothing_changed() {
    // M12, pin P16: one with an explicit "platform":"linux/arm64" beside one with none.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::plain(PROJECT)]);
    fx.write_hosts_raw(
        concat!(
            "{\"version\":0,\"hosts\":[",
            "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_type\":\"m9g.xlarge\"},",
            "{\"project\":\"agentspace\",\"platform\":\"linux/arm64\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_type\":\"m9g.xlarge\"}",
            "]}\n"
        )
        .as_bytes(),
    );
    let before = fx.snapshot();
    let d = fx.driver_with(&detach);
    let f = fx
        .place_default_on(&d, PROJECT, ID, ARM, "m")
        .expect_err("A registry that is not valid fails the place");
    fx.assert_failure_plain(&f);
    assert_eq!(detach.count(), 0);
    fx.assert_unchanged(&before, "A failed place");
}

#[test]
fn records_of_one_project_on_two_platforms_are_valid_and_each_lease_is_placed_by_its_own() {
    // M12: a record on linux/arm64 at m9g.xlarge and one on linux/amd64 at m8a.xlarge.
    let (fx, detach) = world();
    fx.write_hosts(&[H::ready(PROJECT), H::ready(PROJECT).on(AMD).typed(U_M)]);
    let d = fx.driver_with(&detach);
    for (n, platform) in [(1u32, ARM), (2, AMD)] {
        let id = id_n(n);
        let got = fx.place_default_on(&d, PROJECT, &id, platform, "m");
        assert_ready_on(&fx, got, &id, platform);
    }
    assert_eq!(detach.count(), 0);
}

// ---- The size's column is the host's platform's ---------------------------------------------------------------

#[test]
fn a_type_of_another_column_meets_no_size_so_the_lease_is_pending_and_a_run_starts() {
    // M9, pin P28: a linux/amd64 record `ready` at m9g.xlarge meets no size; the default size is
    // `pending`, with and without a live `ready` lease of the project on linux/amd64.
    for with_live_lease in [false, true] {
        let (fx, detach) = world();
        fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD])]);
        fx.write_hosts(&[H::ready(PROJECT).on(AMD).typed("m9g.xlarge")]);
        if with_live_lease {
            fx.write_leases(&[L::ready(&id_n(7), PROJECT).on(AMD)]);
        }
        let d = fx.driver_with(&detach);
        let got = fx.place_default_on(&d, PROJECT, ID, AMD, "m");
        assert!(
            matches!(got, Ok(Placed::Pending)),
            "live lease {with_live_lease}: {got:?}"
        );
        assert_eq!(detach.only().args, run_args(PROJECT, AMD));
    }
}

#[test]
fn the_m8a_column_places_by_the_hosts_type_on_both_amd64_platforms() {
    // M8: with a live `ready` lease of the project on the platform and its record `ready` at
    // `m8a.2xlarge`, size m is `ready` (a larger host) and xl is D66's no_capacity, each starting
    // nothing; with no live lease and the record at `m8a.4xlarge`, the default size is `pending`
    // and starts a run with five arguments.
    for platform in [AMD, WIN] {
        let (fx, detach) = world();
        fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD, WIN])]);
        fx.write_hosts(&[H::ready(PROJECT).on(platform).typed(U_L)]);
        fx.write_leases(&[L::ready(&id_n(7), PROJECT).on(platform)]);
        let d = fx.driver_with(&detach);
        let got = fx.place_default_on(&d, PROJECT, ID, platform, "m");
        assert_ready_on(&fx, got, ID, platform);
        let got = fx.place_default_on(&d, PROJECT, &id_n(2), platform, "xl");
        let reasons = unsatisfiable(got);
        assert_eq!(codes(&reasons), ["no_capacity"], "{platform}");
        assert_eq!(
            reasons[0].message,
            no_capacity_at_size(PROJECT, "xl"),
            "{platform}"
        );
        assert_eq!(detach.count(), 0, "{platform}");

        let (fx, detach) = world();
        fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD, WIN])]);
        fx.write_hosts(&[H::ready(PROJECT).on(platform).typed(U_XL)]);
        let d = fx.driver_with(&detach);
        let got = fx.place_default_on(&d, PROJECT, ID, platform, "m");
        assert!(matches!(got, Ok(Placed::Pending)), "{platform}: {got:?}");
        assert_eq!(
            detach.only().args,
            run_args(PROJECT, platform),
            "{platform}"
        );
    }
}

#[test]
fn each_size_is_exact_at_its_own_type_in_the_platforms_column() {
    // The map, shown through place: on the host `ready` at the type of size S, a default lease of
    // size S is `ready` at once (row 3), on each of the three columns.
    let columns: [(&str, [&str; 4]); 3] = [
        (
            ARM,
            ["m9g.large", "m9g.xlarge", "m9g.2xlarge", "m9g.4xlarge"],
        ),
        (AMD, [U_S, U_M, U_L, U_XL]),
        (WIN, [U_S, U_M, U_L, U_XL]),
    ];
    for (platform, types) in columns {
        for (size, instance_type) in ["s", "m", "l", "xl"].into_iter().zip(types) {
            let (fx, detach) = world();
            fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD, WIN])]);
            fx.write_hosts(&[H::ready(PROJECT).on(platform).typed(instance_type)]);
            let d = fx.driver_with(&detach);
            let got = fx.place_default_on(&d, PROJECT, ID, platform, size);
            assert_ready_on(&fx, got, ID, platform);
            assert_eq!(detach.count(), 0, "{platform} {size}");
        }
    }
}

#[test]
fn the_arm64_type_does_not_serve_an_amd64_size_and_the_amd64_type_does_not_serve_arm64() {
    // The columns do not cross: m8a.xlarge on a linux/arm64 record meets no size either.
    let (fx, detach) = world();
    fx.write_hosts(&[H::ready(PROJECT).typed(U_M)]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, ARM, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(detach.only().args, run_args(PROJECT, ARM));
}

// ---- A resize counts the project's leases on one platform alone ------------------------------------------------

#[test]
fn a_live_ready_lease_on_another_platform_holds_nothing_here() {
    // M7: with `ready` records on linux/arm64 at m9g.xlarge and on linux/amd64 at m8a.xlarge, and
    // a live `ready` lease on linux/arm64 only, `--platform linux/amd64 --size l` is `pending`
    // with a run of five arguments (row 6), while the same size on linux/arm64 is D66's
    // no_capacity (row 5).
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD])]);
    fx.write_hosts(&[H::ready(PROJECT), H::ready(PROJECT).on(AMD).typed(U_M)]);
    fx.write_leases(&[L::ready(&id_n(7), PROJECT)]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, AMD, "l");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(detach.only().args, run_args(PROJECT, AMD));
    let got = fx.place_default_on(&d, PROJECT, &id_n(2), ARM, "l");
    let reasons = unsatisfiable(got);
    assert_eq!(reasons[0].message, no_capacity_at_size(PROJECT, "l"));
    assert_eq!(detach.count(), 1, "The refusal starts nothing");
}

#[test]
fn a_pending_lease_on_another_platform_does_not_hold_this_platforms_leases_pending() {
    // M7: with a stored `pending` lease of the project on linux/arm64 at l and none on
    // linux/amd64, a default linux/amd64 lease is `ready` and starts nothing (D91).
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD])]);
    fx.write_hosts(&[H::ready(PROJECT), H::ready(PROJECT).on(AMD).typed(U_M)]);
    fx.write_leases(&[L::pending(&id_n(7), PROJECT).size("l")]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, AMD, "m");
    assert_ready_on(&fx, got, ID, AMD);
    assert_eq!(detach.count(), 0);
    // The same pending lease does hold the arm64 host's new leases pending.
    let got = fx.place_default_on(&d, PROJECT, &id_n(2), ARM, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
}

#[test]
fn a_pending_lease_on_the_same_platform_holds_new_leases_pending() {
    // Row 1 on a platform: a `pending` lease of the project on linux/amd64 holds the host's new
    // linux/amd64 leases `pending`, and a run is due.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[AMD])]);
    fx.write_hosts(&[H::ready(PROJECT).on(AMD).typed(U_M)]);
    fx.write_leases(&[L::pending(&id_n(7), PROJECT).on(AMD).size("l")]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, AMD, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(detach.only().args, run_args(PROJECT, AMD));
}

// ---- Each host has a lock of its own ------------------------------------------------------------------------------

#[test]
fn another_hosts_run_lock_does_not_hold_this_hosts_run() {
    // M10: with the arm64 host's provision.lock held, a linux/amd64 lease whose record is
    // `creating` with no `last_attempt_at` starts one run with five arguments.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD])]);
    fx.write_hosts(&[H::ready(PROJECT), H::creating(PROJECT).on(AMD)]);
    let held = agentspace_hosts::try_run_lock_keyed(&fx.home, &HostKey::new(PROJECT, ARM))
        .expect("The lock can be tried")
        .expect("The arm64 host's lock is free to take");
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, AMD, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(detach.only().args, run_args(PROJECT, AMD));
    drop(held);
}

#[test]
fn this_hosts_own_run_lock_holds_its_run_and_the_registry_is_as_it_was() {
    // M10: with the linux/amd64 host's own lock held, the same lease starts nothing and leaves
    // the registry as it was.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, AMD])]);
    fx.write_hosts(&[H::ready(PROJECT), H::creating(PROJECT).on(AMD)]);
    let hosts_before = fx.hosts_bytes();
    let held = agentspace_hosts::try_run_lock_keyed(&fx.home, &HostKey::new(PROJECT, AMD))
        .expect("The lock can be tried")
        .expect("The amd64 host's lock is free to take");
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, AMD, "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(detach.count(), 0);
    assert_eq!(fx.hosts_bytes(), hosts_before);
    drop(held);
}

// ---- Renew and release of a Windows lease ---------------------------------------------------------------------------

#[test]
fn a_renewed_windows_certificate_names_the_same_user_as_the_first() {
    // M15, pins P19 and P25: the new certificate's principal is `agent`, as the lease's platform
    // says, and the alias stays.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[WIN])]);
    fx.write_hosts(&[H::ready(PROJECT).on(WIN).typed(U_M)]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, WIN, "m");
    assert_ready_on(&fx, got, ID, WIN);
    fx.write_leases(&[L::ready(ID, PROJECT).on(WIN)]);
    let alias_before = std::fs::read(fx.alias_file_of(PROJECT, ID)).unwrap();
    let lease = LeaseRef {
        id: ID,
        project: PROJECT,
    };
    let renewal = d
        .renew(&fx.ctx, &lease, 7200)
        .expect("A Windows lease renews");
    renewal.commit();
    let cert = cert_info(&fx.cert_of(ID));
    assert_eq!(cert.principals, ["agent"], "The renewed certificate");
    assert_eq!(
        std::fs::read(fx.alias_file_of(PROJECT, ID)).unwrap(),
        alias_before
    );
}

#[test]
fn a_windows_lease_is_released_with_its_files_removed() {
    // M15: release removes the alias file and the lease folder.
    let (fx, detach) = world();
    fx.write_project_entries(&[PE::approving(PROJECT, &[WIN])]);
    fx.write_hosts(&[H::ready(PROJECT).on(WIN).typed(U_M)]);
    let d = fx.driver_with(&detach);
    let got = fx.place_default_on(&d, PROJECT, ID, WIN, "m");
    assert_ready_on(&fx, got, ID, WIN);
    let lease = LeaseRef {
        id: ID,
        project: PROJECT,
    };
    d.release(&fx.ctx, &lease)
        .expect("A Windows lease is released");
    assert!(!fx.has_files_of(PROJECT, ID));
}

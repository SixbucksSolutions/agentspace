//! `place_pending` and `check_pending` by project and platform: a provisioning run places only the
//! pending leases of its own host, the project's on one platform, with that platform's user, and
//! meets and answers sizes in that platform's column alone; a `get` of a pending lease starts the
//! run for the lease's own platform (docs/driver.md, place_pending, check_pending and Starting a
//! provisioning run, and the v0.12 conformance case M11; docs/decisions.md, D91, D94, D95;
//! E0.12-T03-S01, #0254).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. `place_pending` never
//! writes the host registry, starts nothing and makes no call to AWS. Every start of a run is a fake
//! `Detach`'s, and every file is in a throwaway folder.
#![cfg(windows)]

mod common;
#[path = "common/pending.rs"]
mod pending;

use agentspace_driver_api::{Driver, Failure, LeaseRef};
use agentspace_driver_ec2::{PendingPlaced, Unplaced, place_pending, place_pending_keyed};
use agentspace_hosts::HostKey;
use common::*;
use pending::*;
use serde_json::{Value, json};

const PROJECT: &str = "agentspace";
const ARM: &str = "linux/arm64";
const AMD: &str = "linux/amd64";
const WIN: &str = "windows/amd64";
const PLATFORMS: [&str; 3] = [ARM, AMD, WIN];
const A: &str = "aaaaaaaa-1111-4111-8111-111111111111";
const B: &str = "bbbbbbbb-2222-4222-8222-222222222222";
const C: &str = "cccccccc-3333-4333-8333-333333333333";
const D: &str = "dddddddd-4444-4444-8444-444444444444";
const R: &str = "ffffffff-6666-4666-8666-666666666666";

/// The type that is `m` on `platform`: the host of that platform at its default size.
fn type_m(platform: &str) -> &'static str {
    if platform == ARM { "m9g.xlarge" } else { U_M }
}

/// A home with a CA, a state folder that is not the default, and one `ready` host of the project
/// on each platform, each at its platform's `m`.
fn world() -> Fx {
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    fx.write_hosts(&[
        H::ready(PROJECT),
        H::ready(PROJECT).on(AMD).typed(U_M),
        H::ready(PROJECT).on(WIN).typed(U_M),
    ]);
    fx
}

fn as_written(l: &L) -> Value {
    serde_json::from_str(&l.json()).expect("A record is JSON")
}

/// What a call for this host did: how many it placed, and what became of each lease it did not.
#[track_caller]
fn call(fx: &Fx, platform: &str) -> (usize, Vec<(&'static str, String)>) {
    let got: Result<PendingPlaced, Failure> =
        place_pending_keyed(&fx.ctx, &HostKey::new(PROJECT, platform));
    let got = match got {
        Ok(got) => got,
        Err(f) => panic!("The call failed as a whole: {}", f.message),
    };
    let kinds = got
        .unplaced
        .iter()
        .map(|u| {
            let kind = match u {
                Unplaced::Failed(_) => "failed",
                Unplaced::Refused { .. } => "refused",
                Unplaced::Left { .. } => "left",
            };
            (kind, u.lease_id().to_string())
        })
        .collect();
    (got.placed, kinds)
}

/// A lease placed on its platform's host: `ready`, with that platform's user in `ssh`, in the
/// certificate's principal and in the alias, and every other member as it was.
#[track_caller]
fn assert_placed(fx: &Fx, before: &L) {
    let got = fx.lease_value(&before.id);
    let user = login_user_of(&before.platform);
    let mut want = as_written(before);
    {
        let o = want.as_object_mut().unwrap();
        o.insert("state".into(), json!("ready"));
        o.insert("expires_at".into(), got["expires_at"].clone());
        o.insert(
            "ssh".into(),
            json!({"host_alias": alias_for(&before.project, &before.id), "user": user}),
        );
    }
    assert_eq!(
        got, want,
        "Only state, expires_at and ssh change: platform and profile stay"
    );
    let cert = cert_info(&fx.lease_dir(&before.id).join("id_ed25519-cert.pub"));
    assert_eq!(
        cert.principals,
        [user],
        "{}: the principal",
        before.platform
    );
    assert_eq!(cert.valid_to, expires_of(&got));
    assert_alias_bytes_on(fx, &before.id, &before.project, &before.platform);
}

/// A lease left as it was: its record, and no file of its own.
#[track_caller]
fn assert_untouched(fx: &Fx, before: &L) {
    assert_eq!(
        fx.lease_value(&before.id),
        as_written(before),
        "{}",
        before.id
    );
    assert!(
        !fx.has_files_of(&before.project, &before.id),
        "{}",
        before.id
    );
}

fn want(list: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    list.iter().map(|(k, id)| (*k, id.to_string())).collect()
}

// ---- A run places its own platform's pending leases alone --------------------------------------------

#[test]
fn a_run_places_only_the_pending_leases_of_its_platform_with_that_platforms_user() {
    // For each platform in turn: with one pending lease of the project on each platform, the call
    // for that platform's host places that platform's lease alone.
    for platform in PLATFORMS {
        let fx = world();
        let leases: Vec<L> = [(A, ARM), (B, AMD), (C, WIN)]
            .into_iter()
            .map(|(id, p)| L::pending(id, PROJECT).on(p))
            .collect();
        fx.write_leases(&leases);
        let hosts_before = fx.hosts_bytes();
        assert_eq!(call(&fx, platform), (1, Vec::new()), "{platform}");
        for l in &leases {
            if l.platform == platform {
                assert_placed(&fx, l);
            } else {
                assert_untouched(&fx, l);
            }
        }
        assert_eq!(
            fx.hosts_bytes(),
            hosts_before,
            "The registry is never written"
        );
    }
}

#[test]
fn a_windows_lease_is_placed_with_agent_as_its_user_and_not_ubuntu() {
    // D95, pin P19: the one thing that differs in a placed lease. Fails if every lease gets ubuntu.
    let fx = world();
    let w = L::pending(A, PROJECT).on(WIN);
    fx.write_leases(std::slice::from_ref(&w));
    assert_eq!(call(&fx, WIN), (1, Vec::new()));
    let got = fx.lease_value(A);
    assert_eq!(got["ssh"]["user"], "agent");
    assert_eq!(got["profile"], "agent-windows");
    assert_eq!(
        cert_info(&fx.lease_dir(A).join("id_ed25519-cert.pub")).principals,
        ["agent"]
    );
    assert_placed(&fx, &w);
}

#[test]
fn place_pending_with_no_platform_means_linux_arm64() {
    // driver.md: a call that names no platform is the `linux/arm64` host's, and keeps its
    // signature (D101).
    let fx = world();
    let a = L::pending(A, PROJECT);
    let b = L::pending(B, PROJECT).on(AMD);
    let c = L::pending(C, PROJECT).on(WIN);
    fx.write_leases(&[a.clone(), b.clone(), c.clone()]);
    let got = place_pending(&fx.ctx, PROJECT).expect("The call succeeds");
    assert_eq!(got.placed, 1);
    assert_placed(&fx, &a);
    assert_untouched(&fx, &b);
    assert_untouched(&fx, &c);
}

#[test]
fn a_run_for_one_platform_leaves_another_projects_leases_on_it_alone() {
    let fx = world();
    let mine = L::pending(A, PROJECT).on(AMD);
    let other = L::pending(B, "someone-else").on(AMD);
    fx.write_leases(&[mine.clone(), other.clone()]);
    assert_eq!(call(&fx, AMD), (1, Vec::new()));
    assert_placed(&fx, &mine);
    assert_untouched(&fx, &other);
}

#[test]
fn a_run_places_nothing_when_its_platforms_host_is_not_ready_whatever_the_other_hosts_are() {
    // The arm64 and windows hosts are `ready`; the linux/amd64 record is `creating`. The call for
    // linux/amd64 places nothing and succeeds, and its pending lease stays as it was.
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    fx.write_hosts(&[
        H::ready(PROJECT),
        H::creating(PROJECT).on(AMD),
        H::ready(PROJECT).on(WIN).typed(U_M),
    ]);
    let a = L::pending(A, PROJECT).on(AMD);
    fx.write_leases(std::slice::from_ref(&a));
    assert_eq!(call(&fx, AMD), (0, Vec::new()));
    assert_untouched(&fx, &a);
    // And with no record on the platform at all.
    let mut fx2 = Fx::with_ca();
    fx2.state_elsewhere();
    fx2.write_hosts(&[H::ready(PROJECT)]);
    let w = L::pending(B, PROJECT).on(WIN);
    fx2.write_leases(std::slice::from_ref(&w));
    assert_eq!(call(&fx2, WIN), (0, Vec::new()));
    assert_untouched(&fx2, &w);
}

// ---- Sizes are met and answered in the platform's column ------------------------------------------------

#[test]
fn a_host_meets_a_lease_by_its_own_platforms_column() {
    // Each platform's host at its `l`: leases of size s, m and l are placed, xl is not.
    for platform in [AMD, WIN] {
        let mut fx = Fx::with_ca();
        fx.state_elsewhere();
        fx.write_hosts(&[H::ready(PROJECT).on(platform).typed(U_L)]);
        let s = L::pending(A, PROJECT).on(platform).size("s");
        let m = L::pending(B, PROJECT).on(platform).size("m");
        let l = L::pending(C, PROJECT).on(platform).size("l");
        let xl = L::pending(D, PROJECT).on(platform).size("xl");
        fx.write_leases(&[s.clone(), m.clone(), l.clone(), xl.clone()]);
        // Step 3's rule, pin P41: the three just placed `ready` hold the host, so the lease the
        // host does not meet is answered at once, in the same call, `no_capacity`.
        assert_eq!(
            call(&fx, platform),
            (3, want(&[("refused", D)])),
            "{platform}"
        );
        for placed in [&s, &m, &l] {
            assert_placed(&fx, placed);
        }
        let got = fx.lease_value(&xl.id);
        assert_eq!(got["state"], "unsatisfiable", "{platform}");
        assert_eq!(
            got["reasons"][0]["message"],
            no_capacity_at_size(PROJECT, "xl"),
            "{platform}"
        );
    }
}

#[test]
fn a_type_of_another_column_meets_no_size_so_nothing_is_placed_and_nothing_is_answered() {
    // Pin P40 and pin P28: a linux/amd64 record at `m9g.xlarge` holds a type the map does not hold
    // for it: no lease is placed, and none is refused, whatever holds the host.
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    fx.write_hosts(&[H::ready(PROJECT).on(AMD).typed("m9g.xlarge")]);
    let a = L::pending(A, PROJECT).on(AMD);
    let holder = L::ready(R, PROJECT).on(AMD);
    fx.write_leases(&[a.clone(), holder.clone()]);
    assert_eq!(call(&fx, AMD), (0, want(&[("left", A)])));
    assert_untouched(&fx, &a);
}

#[test]
fn an_unmet_lease_is_refused_only_while_a_live_ready_lease_on_its_own_platform_holds_the_host() {
    // D66, D68 and D91: the windows host at `m`, a pending `xl` lease on windows.
    // 1. A live `ready` lease of the project on linux/amd64 holds nothing here: left.
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    fx.write_hosts(&[
        H::ready(PROJECT).on(WIN).typed(U_M),
        H::ready(PROJECT).on(AMD).typed(U_M),
    ]);
    let xl = L::pending(A, PROJECT).on(WIN).size("xl");
    let other_platform_holder = L::ready(R, PROJECT).on(AMD);
    fx.write_leases(&[xl.clone(), other_platform_holder.clone()]);
    assert_eq!(call(&fx, WIN), (0, want(&[("left", A)])));
    assert_untouched(&fx, &xl);

    // 2. A live `ready` lease of the project on windows/amd64 holds it: refused, with D66's message.
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    fx.write_hosts(&[H::ready(PROJECT).on(WIN).typed(U_M)]);
    let holder = L::ready(R, PROJECT).on(WIN);
    fx.write_leases(&[xl.clone(), holder]);
    assert_eq!(call(&fx, WIN), (0, want(&[("refused", A)])));
    let got = fx.lease_value(A);
    assert_eq!(got["state"], "unsatisfiable");
    assert_eq!(
        got["reasons"],
        json!([{
            "code": "no_capacity",
            "message": no_capacity_at_size(PROJECT, "xl"),
        }])
    );
    assert_eq!(got["platform"], WIN);
    assert_eq!(got["profile"], "agent-windows");
}

#[test]
fn a_lease_the_platforms_host_meets_is_placed_beside_another_platforms_larger_pending_lease() {
    // The pending `xl` lease on linux/arm64 neither blocks nor holds the windows host's `m`.
    let fx = world();
    let big_arm = L::pending(A, PROJECT).size("xl");
    let win = L::pending(B, PROJECT).on(WIN);
    fx.write_leases(&[big_arm.clone(), win.clone()]);
    assert_eq!(call(&fx, WIN), (1, Vec::new()));
    assert_placed(&fx, &win);
    assert_untouched(&fx, &big_arm);
}

// ---- check_pending starts the run for the lease's own platform ---------------------------------------------

#[test]
fn a_get_of_a_pending_windows_lease_starts_the_run_with_five_arguments_and_the_stored_platform() {
    // M11: the stand-in is started once, with `provision -Host <project> -Platform windows/amd64`;
    // that record's `last_attempt_at` is set and no other record's; the lease state file is byte
    // for byte as it was, with no lease folder or alias file made.
    let mut fx = Fx::with_ca();
    fx.install_operator();
    fx.write_hosts(&[
        H::ready(PROJECT),
        H::ready(PROJECT).on(AMD).typed(U_M),
        H::ready(PROJECT).on(WIN).typed(U_M),
    ]);
    fx.write_leases(&[L::pending(A, PROJECT).on(WIN)]);
    let detach = FakeDetach::new();
    let leases_before = fx.leases_bytes();
    let d = fx.driver_with(&detach);
    let t0 = now();
    d.check_pending(
        &fx.ctx,
        &LeaseRef {
            id: A,
            project: PROJECT,
            platform: WIN,
        },
    )
    .expect("The check succeeds");
    let t1 = now();
    assert_eq!(detach.only().args, run_args(PROJECT, WIN));
    let win = fx.host_on(PROJECT, WIN).expect("The windows record");
    let at = epoch_of(win.last_attempt_at.as_deref().expect("last_attempt_at"));
    assert!(t0 <= at && at <= t1);
    for other in [ARM, AMD] {
        let record = fx.host_on(PROJECT, other).expect("The other record");
        assert_eq!(
            record.last_attempt_at, None,
            "{other}: no other record is written"
        );
    }
    assert_eq!(
        fx.leases_bytes(),
        leases_before,
        "The lease state file is as it was"
    );
    assert!(!fx.has_files_of(PROJECT, A));
}

#[test]
fn a_get_of_a_pending_lease_on_each_platform_starts_that_platforms_run() {
    // The same, for each of the three: three arguments on linux/arm64, five on the others.
    for platform in PLATFORMS {
        let mut fx = Fx::with_ca();
        fx.install_operator();
        fx.write_hosts(&[
            H::ready(PROJECT).typed(type_m(ARM)),
            H::ready(PROJECT).on(AMD).typed(U_M),
            H::ready(PROJECT).on(WIN).typed(U_M),
        ]);
        fx.write_leases(&[L::pending(A, PROJECT).on(platform)]);
        let detach = FakeDetach::new();
        let d = fx.driver_with(&detach);
        d.check_pending(
            &fx.ctx,
            &LeaseRef {
                id: A,
                project: PROJECT,
                platform,
            },
        )
        .expect("The check succeeds");
        let start = detach.only();
        assert_eq!(start.args, run_args(PROJECT, platform), "{platform}");
        assert_eq!(
            start.args.len(),
            if platform == ARM { 3 } else { 5 },
            "{platform}"
        );
    }
}

#[test]
fn another_platforms_recent_attempt_does_not_hold_this_platforms_run() {
    // Each host has its own `last_attempt_at`: the arm64 host tried a minute ago, which is within
    // the 5-minute rule for it alone, and the pending lease on windows still starts its run.
    let mut fx = Fx::with_ca();
    fx.install_operator();
    fx.write_hosts(&[
        H::ready(PROJECT).attempted_ago(60),
        H::ready(PROJECT).on(WIN).typed(U_M),
    ]);
    fx.write_leases(&[L::pending(A, PROJECT).on(WIN)]);
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    d.check_pending(
        &fx.ctx,
        &LeaseRef {
            id: A,
            project: PROJECT,
            platform: WIN,
        },
    )
    .expect("The check succeeds");
    assert_eq!(detach.only().args, run_args(PROJECT, WIN));
}

#[test]
fn this_platforms_recent_attempt_holds_its_own_run() {
    // The windows host tried a minute ago and has not succeeded since: no run is due.
    let mut fx = Fx::with_ca();
    fx.install_operator();
    fx.write_hosts(&[H::ready(PROJECT).on(WIN).typed(U_M).attempted_ago(60)]);
    fx.write_leases(&[L::pending(A, PROJECT).on(WIN)]);
    let hosts_before = fx.hosts_bytes();
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    d.check_pending(
        &fx.ctx,
        &LeaseRef {
            id: A,
            project: PROJECT,
            platform: WIN,
        },
    )
    .expect("The check succeeds");
    assert_eq!(detach.count(), 0);
    assert_eq!(fx.hosts_bytes(), hosts_before);
}

#[test]
fn a_get_of_a_pending_lease_on_a_platform_with_no_record_and_no_approval_starts_nothing() {
    // From `check_pending` a project that is not on the list for the platform is nothing at all:
    // no run, no record.
    let mut fx = Fx::with_ca();
    fx.install_operator();
    fx.write_project_entries(&[PE::plain(PROJECT)]);
    fx.write_leases(&[L::pending(A, PROJECT).on(WIN)]);
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    d.check_pending(
        &fx.ctx,
        &LeaseRef {
            id: A,
            project: PROJECT,
            platform: WIN,
        },
    )
    .expect("The check succeeds");
    assert_eq!(detach.count(), 0);
    assert!(
        fx.hosts_bytes().is_none(),
        "Nothing is written to the registry"
    );
}

#[test]
fn a_get_of_a_pending_lease_on_an_approved_platform_with_no_record_adds_its_record_and_starts_its_run()
 {
    let mut fx = Fx::with_ca();
    fx.install_operator();
    fx.write_project_entries(&[PE::approving(PROJECT, &[ARM, WIN])]);
    fx.write_leases(&[L::pending(A, PROJECT).on(WIN)]);
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    d.check_pending(
        &fx.ctx,
        &LeaseRef {
            id: A,
            project: PROJECT,
            platform: WIN,
        },
    )
    .expect("The check succeeds");
    assert_eq!(detach.only().args, run_args(PROJECT, WIN));
    let record = fx.host_on(PROJECT, WIN).expect("The record was added");
    assert_eq!(record.stack, "agentspace-windows-amd64-host-agentspace");
}

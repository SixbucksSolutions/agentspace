//! `place_pending`, and the release and renew of a lease that was pending (docs/driver.md,
//! place_pending and Release a lease; docs/state.md, The lease state file, Creation in order and
//! Release, and what it leaves; E0.07-T03-S01, #0150).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. `place_pending` places
//! each `pending` lease of a project once the project's host is `ready`, one hold of the lease
//! state lock for each, and never writes the host registry or starts anything.
#![cfg(windows)]

#[path = "common/clients.rs"]
mod clients;
mod common;
#[path = "common/pending.rs"]
mod pending;

use agentspace_driver_api::{Driver, Failure, LeaseRef};
use agentspace_driver_ec2::{PendingPlaced, place_pending};
use clients::{Lab, random_uuid_v4};
use common::*;
use pending::*;
use serde_json::{Value, json};
use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

const PROJECT: &str = "agentspace";
const A: &str = "aaaaaaaa-1111-4111-8111-111111111111";
const B: &str = "bbbbbbbb-2222-4222-8222-222222222222";
const C: &str = "cccccccc-3333-4333-8333-333333333333";
const D: &str = "dddddddd-4444-4444-8444-444444444444";
const E: &str = "eeeeeeee-5555-4555-8555-555555555555";

/// A home with a CA, the proxy program beside the program, a state folder that is not the default,
/// and a ready host for `PROJECT`.
fn world() -> Fx {
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    fx.write_hosts(&[H::ready(PROJECT)]);
    fx
}

/// The record as the lease CLI wrote it, as a document.
fn as_written(l: &L) -> Value {
    serde_json::from_str(&l.json()).expect("A record is JSON")
}

/// What a call answered, as how many it placed and the ID of each lease it could not place. A
/// lease that cannot be placed MUST be in `failed`, never an `Err` of the whole call, and each
/// failure's message is plain: agentspace's own words, with no path.
#[track_caller]
fn outcome(fx: &Fx, got: Result<PendingPlaced, Failure>) -> (usize, Vec<String>) {
    let got = match got {
        Ok(got) => got,
        Err(f) => panic!("The call failed as a whole: {}", f.message),
    };
    for message in got.unplaced.iter().filter_map(|u| u.message()) {
        fx.assert_plain(message);
    }
    let failed = got
        .unplaced
        .iter()
        .map(|u| u.lease_id().to_string())
        .collect();
    (got.placed, failed)
}

/// `n` placed and the leases `failed` left pending.
fn was(n: usize, failed: &[&str]) -> (usize, Vec<String>) {
    (n, failed.iter().map(|id| id.to_string()).collect())
}

/// Checks a lease that was placed: `ready`, with an expiry that is its certificate's valid-before
/// time, an `ssh` of the alias and `ubuntu`, and every other member as it was.
#[track_caller]
fn assert_placed(fx: &Fx, before: &L, t0: i64, t1: i64, other_host: bool) {
    let got = fx.lease_value(&before.id);
    let mut want = as_written(before);
    let alias = alias_for(&before.project, &before.id);
    {
        let o = want.as_object_mut().unwrap();
        o.insert("state".into(), json!("ready"));
        o.insert("expires_at".into(), got["expires_at"].clone());
        o.insert("ssh".into(), json!({"host_alias": alias, "user": "ubuntu"}));
    }
    assert_eq!(got, want, "Only state, expires_at and ssh change");
    let expires = expires_of(&got);
    let ttl = i64::from(before.ttl);
    assert!(
        t0 + ttl <= expires && expires <= t1 + ttl,
        "The TTL starts when the lease becomes ready, not when it was created"
    );
    let cert = cert_info(&fx.lease_dir(&before.id).join("id_ed25519-cert.pub"));
    assert_eq!(
        cert.valid_to, expires,
        "expires_at is the certificate's valid-before time"
    );
    assert_eq!(cert.principals, ["ubuntu"]);
    assert_eq!(cert.key_id, before.id);
    assert_eq!(
        fx.lease_files(&before.id),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
    assert_alias_bytes(fx, &before.id, &before.project, other_host);
}

// ---- What it places -----------------------------------------------------------------------------------------

#[test]
fn it_places_every_pending_lease_of_the_project_and_leaves_every_other_record_alone() {
    let fx = world();
    let a = L::pending(A, PROJECT).ttl(3600);
    let b = L::pending(B, PROJECT).ttl(7200);
    let other_project = L::pending(C, "other");
    let other_driver = L::pending(D, PROJECT).driver(Some("fake"));
    let no_driver = L::pending(E, PROJECT).driver(None);
    let released = L::released("ffffffff-6666-4666-8666-666666666666", PROJECT);
    fx.write_leases(&[
        a.clone(),
        other_project.clone(),
        b.clone(),
        other_driver.clone(),
        no_driver.clone(),
        released.clone(),
    ]);
    let hosts_before = fx.hosts_bytes();
    let t0 = now();
    let got = place_pending(&fx.ctx, PROJECT);
    let t1 = now();
    assert_eq!(outcome(&fx, got), was(2, &[]));

    assert_placed(&fx, &a, t0, t1, true);
    assert_placed(&fx, &b, t0, t1, true);
    for untouched in [&other_project, &other_driver, &no_driver, &released] {
        assert_eq!(
            fx.lease_value(&untouched.id),
            as_written(untouched),
            "A record that is not this project's, this driver's or pending is as it was"
        );
        assert!(!fx.has_files_of(&untouched.project, &untouched.id));
    }
    // The order of the file is kept, and it is written as the CLI writes it.
    let ids: Vec<String> = fx
        .lease_values()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, [A, C, B, D, E, "ffffffff-6666-4666-8666-666666666666"]);
    let bytes = fx.leases_bytes().unwrap();
    assert!(bytes.ends_with(b"}\n") && !bytes.ends_with(b"\n\n"));
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(!bytes.windows(2).any(|w| w == b"\r\n"));
    assert_eq!(
        fx.hosts_bytes(),
        hosts_before,
        "The host registry is never written"
    );
}

#[test]
fn a_second_call_places_nothing() {
    let fx = world();
    fx.write_leases(&[L::pending(A, PROJECT)]);
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(1, &[]));
    let after = fx.leases_bytes();
    let files = fx.snapshot();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(0, &[]));
    assert_eq!(fx.leases_bytes(), after);
    assert_eq!(fx.snapshot(), files);
}

#[test]
fn a_lease_released_while_pending_is_never_placed() {
    // The state file is re-read for each lease: a record that is no longer `pending` is skipped.
    let fx = world();
    let released = L::released(A, PROJECT);
    let pending = L::pending(B, PROJECT);
    fx.write_leases(&[released.clone(), pending.clone()]);
    let t0 = now();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(1, &[]));
    let t1 = now();
    assert_eq!(fx.lease_value(A), as_written(&released));
    assert!(!fx.has_files_of(PROJECT, A));
    assert_placed(&fx, &pending, t0, t1, true);
}

#[test]
fn the_boxs_pending_leases_are_placed_with_the_boxs_alias() {
    // swimcoach has no record in a missing hosts.json: the seed's record is the box's, ready.
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    let l = L::pending(A, "swimcoach");
    fx.write_leases(std::slice::from_ref(&l));
    let t0 = now();
    assert_eq!(
        outcome(&fx, place_pending(&fx.ctx, "swimcoach")),
        was(1, &[])
    );
    let t1 = now();
    assert_placed(&fx, &l, t0, t1, false);
    assert!(
        fx.hosts_bytes().is_none(),
        "A missing registry stays missing"
    );
}

#[test]
fn a_transport_override_is_the_proxy_command_of_the_alias() {
    let mut fx = world();
    fx.ctx.proxy_override = Some("my-transport --stdio".to_string());
    let l = L::pending(A, PROJECT);
    fx.write_leases(std::slice::from_ref(&l));
    let t0 = now();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(1, &[]));
    assert_placed(&fx, &l, t0, now(), true);
}

#[test]
fn it_uses_the_state_folder_the_context_names_and_no_other() {
    let fx = world();
    // A pending lease in the default folder is not this call's.
    let elsewhere = fx.home.join(".agentspace");
    write_leases(&elsewhere, &[L::pending(A, PROJECT)]);
    let before = fs::read(elsewhere.join("leases.json")).unwrap();
    // The context's own folder has none: a missing file means no leases.
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(0, &[]));
    assert_eq!(fs::read(elsewhere.join("leases.json")).unwrap(), before);
    assert!(!fx.has_files_of(PROJECT, A));
}

// ---- A host that is not ready ----------------------------------------------------------------------------------

#[test]
fn a_host_that_is_not_ready_or_has_no_record_places_nothing() {
    for extra in [
        vec![H::creating(PROJECT)],
        vec![H::failed(PROJECT).instance(INSTANCE)],
        vec![],
    ] {
        let fx = world();
        fx.write_hosts(&extra);
        fx.write_leases(&[L::pending(A, PROJECT)]);
        let leases = fx.leases_bytes();
        let hosts = fx.hosts_bytes();
        assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(0, &[]));
        assert_eq!(fx.leases_bytes(), leases);
        assert_eq!(fx.hosts_bytes(), hosts, "The registry is never written");
        assert!(!fx.has_files_of(PROJECT, A));
        assert!(fx.ssh_names().is_empty());
    }
}

#[test]
fn a_registry_that_cannot_be_read_fails_the_call_and_every_lease_stays_pending() {
    let fx = world();
    fx.write_hosts_raw(b"{");
    fx.write_leases(&[L::pending(A, PROJECT)]);
    let leases = fx.leases_bytes();
    let f = place_pending(&fx.ctx, PROJECT).expect_err("The call fails");
    fx.assert_failure_plain(&f);
    assert_eq!(fx.leases_bytes(), leases);
    assert_eq!(
        fx.hosts_bytes().as_deref(),
        Some(&b"{"[..]),
        "Never repaired"
    );
    assert!(!fx.has_files_of(PROJECT, A));
}

#[test]
fn a_lease_state_file_that_is_not_valid_fails_the_call_and_is_not_changed() {
    let fx = world();
    fs::create_dir_all(&fx.ctx.state_dir).unwrap();
    fs::write(fx.ctx.state_dir.join("leases.json"), b"{").unwrap();
    let f = place_pending(&fx.ctx, PROJECT).expect_err("The call fails");
    fx.assert_failure_plain(&f);
    assert_eq!(fx.leases_bytes().as_deref(), Some(&b"{"[..]));
    assert!(!fx.has_files_of(PROJECT, A));
}

// ---- A lease that cannot be placed -------------------------------------------------------------------------------

#[test]
fn a_lease_that_fails_stays_pending_and_the_others_are_still_placed() {
    // Two leases, one fails: a foreign file at its alias, which place_pending MUST keep. The call
    // does not fail, and the lease that can be placed is, whichever comes first in the file.
    for failing_first in [true, false] {
        let fx = world();
        let bad = L::pending(A, PROJECT);
        let good = L::pending(B, PROJECT);
        let order = if failing_first {
            vec![bad.clone(), good.clone()]
        } else {
            vec![good.clone(), bad.clone()]
        };
        fx.write_leases(&order);
        let foreign = plant_foreign_alias(&fx, PROJECT, A);
        let t0 = now();
        assert_eq!(
            outcome(&fx, place_pending(&fx.ctx, PROJECT)),
            was(1, &[A]),
            "first {failing_first}: one placed, the other named in failed"
        );
        let t1 = now();
        assert_eq!(
            fx.lease_value(A),
            as_written(&bad),
            "The failed lease stays pending"
        );
        assert!(
            !fx.lease_dir(A).exists(),
            "Every file the failed placement made is removed, and its folder"
        );
        assert_eq!(
            fs::read(fx.alias_file_of(PROJECT, A)).unwrap(),
            foreign,
            "The foreign file is kept"
        );
        assert_placed(&fx, &good, t0, t1, true);
    }
}

#[test]
fn a_foreign_alias_with_another_leases_owner_mark_is_kept_too() {
    let fx = world();
    fx.write_leases(&[L::pending(A, PROJECT)]);
    fs::create_dir_all(fx.alias_dir()).unwrap();
    let foreign = format!(
        "# agentspace lease {OTHER_ID}; written by agentspace\nHost {}\n",
        alias_for(PROJECT, A)
    );
    fs::write(fx.alias_file_of(PROJECT, A), foreign.as_bytes()).unwrap();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(0, &[A]));
    assert_eq!(fx.lease_value(A)["state"], "pending");
    assert_eq!(
        fs::read(fx.alias_file_of(PROJECT, A)).unwrap(),
        foreign.as_bytes()
    );
    assert!(!fx.lease_dir(A).exists());
}

#[test]
fn a_missing_ca_fails_each_lease_and_is_never_created() {
    let mut fx = Fx::without_ca();
    fx.state_elsewhere();
    fx.write_hosts(&[H::ready(PROJECT)]);
    fx.write_leases(&[L::pending(A, PROJECT), L::pending(B, PROJECT)]);
    let before = fx.leases_bytes();
    assert_eq!(
        outcome(&fx, place_pending(&fx.ctx, PROJECT)),
        was(0, &[A, B])
    );
    assert_eq!(fx.leases_bytes(), before);
    assert!(
        !fx.ca_dir().exists(),
        "place_pending MUST NOT create the CA"
    );
    assert!(!fx.has_files_of(PROJECT, A) && !fx.has_files_of(PROJECT, B));
}

#[test]
fn a_ca_that_fails_a_check_fails_the_lease_and_nothing_is_changed() {
    let fx = world();
    fs::write(fx.ca_dir().join("agentspace_ca.pub"), b"not a key\n").unwrap();
    fx.write_leases(&[L::pending(A, PROJECT)]);
    let before = fx.leases_bytes();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(0, &[A]));
    assert_eq!(fx.leases_bytes(), before);
    assert!(!fx.has_files_of(PROJECT, A));
}

#[test]
fn a_default_transport_program_that_is_not_there_fails_the_lease() {
    let mut fx = Fx::with_ca_proxy(None);
    fx.state_elsewhere();
    fx.write_hosts(&[H::ready(PROJECT)]);
    let lease = L::pending(A, PROJECT);
    fx.write_leases(std::slice::from_ref(&lease));
    let before = fx.leases_bytes();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(0, &[A]));
    assert_eq!(fx.leases_bytes(), before, "The lease stays pending");
    assert!(
        !fx.has_files_of(PROJECT, A),
        "Nothing a failed placement made is left"
    );

    // A transport override replaces the whole line, and the program is then not looked for.
    fx.ctx.proxy_override = Some("my-transport --stdio".to_string());
    let t0 = now();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(1, &[]));
    assert_placed(&fx, &lease, t0 - 1, now() + 1, true);
}

#[test]
fn a_killed_placements_leftovers_are_removed_first_and_the_lease_is_placed() {
    // A placement killed after its key was made leaves key files and an alias with this lease's
    // owner mark: the next placement removes them first, and its key is a new one.
    for leave_alias in [false, true] {
        let fx = world();
        let l = L::pending(A, PROJECT);
        fx.write_leases(std::slice::from_ref(&l));
        fs::create_dir_all(fx.lease_dir(A)).unwrap();
        fs::write(fx.lease_dir(A).join("id_ed25519"), b"stale key").unwrap();
        fs::write(fx.lease_dir(A).join("id_ed25519.pub"), b"stale public key").unwrap();
        fs::write(
            fx.lease_dir(A).join("id_ed25519-cert.pub"),
            b"stale certificate",
        )
        .unwrap();
        fs::write(
            fx.lease_dir(A).join("renew-ab12.pub"),
            b"a renew's leftover",
        )
        .unwrap();
        if leave_alias {
            fs::create_dir_all(fx.alias_dir()).unwrap();
            fs::write(
                fx.alias_file_of(PROJECT, A),
                format!("# agentspace lease {A}; stale\nHost whatever\n").as_bytes(),
            )
            .unwrap();
        }
        let t0 = now();
        assert_eq!(
            outcome(&fx, place_pending(&fx.ctx, PROJECT)),
            was(1, &[]),
            "alias left {leave_alias}"
        );
        assert_placed(&fx, &l, t0, now(), true);
        assert_ne!(
            fs::read(fx.lease_dir(A).join("id_ed25519")).unwrap(),
            b"stale key",
            "The key is a new one"
        );
    }
}

// ---- Steps 4 and 7 are not a release: they end nothing and log nothing -----------------------------------------

/// What a killed placement leaves for the lease `id`: a stale key in its folder, and an alias file
/// that carries its owner mark.
fn plant_leftovers(fx: &Fx, id: &str) {
    fs::create_dir_all(fx.lease_dir(id)).unwrap();
    fs::write(fx.lease_dir(id).join("id_ed25519"), b"stale key").unwrap();
    fs::create_dir_all(fx.alias_dir()).unwrap();
    fs::write(
        fx.alias_file_of(PROJECT, id),
        format!("# agentspace lease {id}; stale\nHost whatever\n").as_bytes(),
    )
    .unwrap();
}

#[test]
fn removing_a_killed_placements_leftovers_ends_nothing_and_logs_nothing() {
    // Step 4 (driver.md, "For which leases"): the removal is not a release. A stand-in client of the
    // lease's alias, which no real lease that is `pending` can have, stays running, and the state
    // folder has no `release.log`. The lease ID is drawn at random for this run.
    let fx = world();
    let id = random_uuid_v4();
    let l = L::pending(&id, PROJECT);
    fx.write_leases(std::slice::from_ref(&l));
    plant_leftovers(&fx, &id);
    let mut lab = Lab::new();
    let client = lab.start("ssh.exe", &[&alias_for(PROJECT, &id)]);

    let t0 = now();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(1, &[]));
    assert_placed(&fx, &l, t0, now(), true);
    assert_ne!(
        fs::read(fx.lease_dir(&id).join("id_ed25519")).unwrap(),
        b"stale key",
        "The leftovers were removed and a new key made"
    );
    assert!(lab.alive(client), "A removal of leftovers ended a client");
    assert!(
        !fx.ctx.state_dir.join("release.log").exists(),
        "A removal of leftovers logged a release"
    );
}

#[test]
fn removing_the_files_of_a_lease_that_could_not_be_written_ends_nothing_and_logs_nothing() {
    // Step 7: the `ready` record cannot be written, because the lease state file is held open
    // without sharing a delete, so that its replacement fails after the contract's 5 seconds. The
    // lease stays `pending` and its files go as release removes them, ending nothing and logging
    // nothing. The lease ID is drawn at random for this run.
    let fx = world();
    let id = random_uuid_v4();
    let l = L::pending(&id, PROJECT);
    fx.write_leases(std::slice::from_ref(&l));
    plant_leftovers(&fx, &id);
    let mut lab = Lab::new();
    let client = lab.start("ssh.exe", &[&alias_for(PROJECT, &id)]);
    let before = fx.leases_bytes();
    // FILE_SHARE_READ | FILE_SHARE_WRITE, and no FILE_SHARE_DELETE.
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(fx.ctx.state_dir.join("leases.json"))
        .expect("The test can hold the lease state file");

    let got = outcome(&fx, place_pending(&fx.ctx, PROJECT));
    drop(held);
    assert_eq!(got, was(0, &[id.as_str()]), "The write failed");
    assert_eq!(fx.leases_bytes(), before, "The lease stays pending");
    assert!(
        !fx.has_files_of(PROJECT, &id),
        "Its files were removed, the folder included"
    );
    assert!(lab.alive(client), "The removal ended a client");
    assert!(
        !fx.ctx.state_dir.join("release.log").exists(),
        "The removal logged a release"
    );
}

// ---- Locks -----------------------------------------------------------------------------------------------------------

#[test]
fn it_waits_for_the_lease_state_lock_and_then_places() {
    let fx = world();
    let l = L::pending(A, PROJECT);
    fx.write_leases(std::slice::from_ref(&l));
    let held = hold_lease_lock(&fx.ctx.state_dir);
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        drop(held);
    });
    let begun = Instant::now();
    let t0 = now();
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(1, &[]));
    let waited = begun.elapsed();
    releaser.join().unwrap();
    assert!(
        waited >= Duration::from_millis(1000),
        "It waited for the lock: {waited:?}"
    );
    assert_placed(&fx, &l, t0, now(), true);
}

#[test]
fn it_lets_go_of_the_lease_state_lock_when_it_is_done() {
    let fx = world();
    fx.write_leases(&[L::pending(A, PROJECT), L::pending(B, PROJECT)]);
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(2, &[]));
    let again = hold_lease_lock(&fx.ctx.state_dir);
    drop(again);
}

#[test]
fn a_lease_state_lock_held_past_thirty_seconds_fails_the_call_and_every_lease_stays_pending() {
    // This test takes about 30 seconds: the call MUST NOT wait longer.
    let fx = world();
    fx.write_leases(&[L::pending(A, PROJECT)]);
    let before = fx.leases_bytes();
    let held = hold_lease_lock(&fx.ctx.state_dir);
    let begun = Instant::now();
    let got = place_pending(&fx.ctx, PROJECT);
    let waited = begun.elapsed();
    drop(held);
    assert!(
        waited >= Duration::from_secs(28) && waited <= Duration::from_secs(45),
        "It waits for about 30 seconds: {waited:?}"
    );
    let f = got.expect_err("The call fails");
    fx.assert_failure_plain(&f);
    assert_eq!(fx.leases_bytes(), before);
    assert!(!fx.has_files_of(PROJECT, A));
}

// ---- A lease that was placed is like one place made ready ---------------------------------------------------

#[test]
fn a_placed_lease_is_renewed_and_released_like_any_ready_lease() {
    let fx = world();
    let l = L::pending(A, PROJECT);
    fx.write_leases(std::slice::from_ref(&l));
    assert_eq!(outcome(&fx, place_pending(&fx.ctx, PROJECT)), was(1, &[]));
    let first = expires_of(&fx.lease_value(A));
    let d = fx.driver();
    let lease = LeaseRef {
        id: A,
        project: PROJECT,
        platform: "linux/arm64",
    };
    // Renew: a new certificate, the alias and the key unchanged.
    let alias_before = fs::read(fx.alias_file_of(PROJECT, A)).unwrap();
    let key_before = fs::read(fx.lease_dir(A).join("id_ed25519")).unwrap();
    let renewal = d.renew(&fx.ctx, &lease, 7200).expect("A renew works");
    assert!(renewal.expires_at() > first);
    renewal.commit();
    assert_eq!(
        fs::read(fx.alias_file_of(PROJECT, A)).unwrap(),
        alias_before
    );
    assert_eq!(
        fs::read(fx.lease_dir(A).join("id_ed25519")).unwrap(),
        key_before
    );
    // Release: the alias, then the folder.
    d.check_release(&fx.ctx, &lease).expect("Checked");
    d.release(&fx.ctx, &lease).expect("A release works");
    assert!(!fx.has_files_of(PROJECT, A));
    // Neither read the registry nor started anything: there is no seam for a start here, and the
    // registry is as the test wrote it.
    assert_eq!(
        fx.hosts_bytes().unwrap(),
        hosts_text(&[H::ready(PROJECT)]).into_bytes()
    );
}

// ---- Release of a lease that is still pending ----------------------------------------------------------------------

/// What a placement killed after its key was made leaves: a real place's files for a lease whose
/// record is still `pending`.
fn killed_after_the_key(fx: &Fx, project: &str, how_far: u8) {
    let detach = FakeDetach::new();
    let d = fx.driver_with(&detach);
    fx.place_project(&d, project, A)
        .expect("A real place makes the files");
    assert_eq!(detach.count(), 0);
    if how_far == 0 {
        // Killed between the key and the certificate: no certificate, no alias.
        fs::remove_file(fx.lease_dir(A).join("id_ed25519-cert.pub")).unwrap();
        fs::remove_file(fx.alias_file_of(project, A)).unwrap();
    } else if how_far == 1 {
        // Killed between the certificate and the alias.
        fs::remove_file(fx.alias_file_of(project, A)).unwrap();
    }
}

#[test]
fn releasing_a_pending_lease_after_a_killed_placement_leaves_nothing() {
    for (project, host, how_far) in [
        ("swimcoach", None, 0u8),
        ("swimcoach", None, 1),
        ("swimcoach", None, 2),
        (PROJECT, Some(H::ready(PROJECT)), 0),
        (PROJECT, Some(H::ready(PROJECT)), 1),
        (PROJECT, Some(H::ready(PROJECT)), 2),
    ] {
        let mut fx = Fx::with_ca();
        fx.state_elsewhere();
        if let Some(h) = &host {
            fx.write_hosts(std::slice::from_ref(h));
        }
        killed_after_the_key(&fx, project, how_far);
        assert!(
            fx.lease_dir(A).exists(),
            "{project} {how_far}: a placement left its files"
        );
        fx.write_leases(&[L::pending(A, project)]);
        let hosts_before = fx.hosts_bytes();
        let d = fx.driver();
        let lease = LeaseRef {
            id: A,
            project,
            platform: "linux/arm64",
        };
        d.check_release(&fx.ctx, &lease).expect("Checked");
        d.release(&fx.ctx, &lease).expect("A release works");
        assert!(
            !fx.has_files_of(project, A),
            "{project} {how_far}: no key, certificate or alias is left"
        );
        assert!(
            fx.alias_dir().is_dir() && Fx::names(&fx.alias_dir()).is_empty(),
            "{project} {how_far}: agentspace.d holds nothing"
        );
        assert_eq!(
            fs::read(fx.config()).unwrap(),
            b"Include agentspace.d/*\n",
            "The Include line stays"
        );
        assert_eq!(
            fx.hosts_bytes(),
            hosts_before,
            "The host and its record are kept"
        );
        // A repeat finishes nothing and fails nothing.
        d.release(&fx.ctx, &lease).expect("A release repeats");
    }
}

#[test]
fn releasing_a_pending_lease_that_has_no_files_succeeds_and_makes_none() {
    let fx = Fx::with_ca();
    let d = fx.driver();
    let lease = LeaseRef {
        id: A,
        project: PROJECT,
        platform: "linux/arm64",
    };
    d.check_release(&fx.ctx, &lease).expect("Checked");
    d.release(&fx.ctx, &lease).expect("A release works");
    assert!(!fx.has_files_of(PROJECT, A));
}

#[test]
fn releasing_a_pending_lease_never_removes_a_foreign_alias_file() {
    let fx = Fx::with_ca();
    let foreign = plant_foreign_alias(&fx, PROJECT, A);
    fs::create_dir_all(fx.lease_dir(A)).unwrap();
    fs::write(fx.lease_dir(A).join("id_ed25519"), b"a key").unwrap();
    let d = fx.driver();
    let lease = LeaseRef {
        id: A,
        project: PROJECT,
        platform: "linux/arm64",
    };
    let f = d
        .check_release(&fx.ctx, &lease)
        .expect_err("A foreign file is refused");
    assert_eq!(f.message, ALIAS_FOREIGN);
    let f = d
        .release(&fx.ctx, &lease)
        .expect_err("A foreign file is refused");
    assert_eq!(f.message, ALIAS_FOREIGN);
    assert_eq!(fs::read(fx.alias_file_of(PROJECT, A)).unwrap(), foreign);
    assert!(
        fx.lease_dir(A).join("id_ed25519").exists(),
        "Nothing else is deleted"
    );
}

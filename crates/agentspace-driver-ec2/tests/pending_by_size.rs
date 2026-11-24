//! `place_pending` meets the resize: it places only the leases its host meets, and answers the rest
//! by whether a live `ready` lease holds the host (docs/driver.md, place_pending; docs/decisions.md,
//! D66, D68 and D71's pins P1, P20, P21 and P37; docs/state.md, The lease state file;
//! E0.09-T03-S01, #0203).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. `place_pending` never
//! writes the host registry, starts nothing and makes no call to AWS. Every file is in a throwaway
//! folder.
#![cfg(windows)]

mod common;
#[path = "common/pending.rs"]
mod pending;

use agentspace_driver_api::Failure;
use agentspace_driver_ec2::{PendingPlaced, Unplaced, place_pending};
use common::*;
use pending::*;
use serde_json::{Value, json};
use std::fs;

const PROJECT: &str = "agentspace";
const A: &str = "aaaaaaaa-1111-4111-8111-111111111111";
const B: &str = "bbbbbbbb-2222-4222-8222-222222222222";
const C: &str = "cccccccc-3333-4333-8333-333333333333";
const D: &str = "dddddddd-4444-4444-8444-444444444444";
/// The lease that holds the host: live and `ready`.
const R: &str = "ffffffff-6666-4666-8666-666666666666";

/// A home with a CA, the proxy program beside the program, a state folder that is not the default,
/// and a `ready` host for `PROJECT` of this type.
fn world(instance_type: Option<&str>) -> Fx {
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    let host = match instance_type {
        Some(t) => H::ready(PROJECT).typed(t),
        None => H::ready_untyped(PROJECT),
    };
    fx.write_hosts(&[host]);
    fx
}

fn holder() -> L {
    L::ready(R, PROJECT)
}

/// The record as the lease CLI wrote it, as a document.
fn as_written(l: &L) -> Value {
    serde_json::from_str(&l.json()).expect("A record is JSON")
}

/// What a call did: how many it placed and, in order, what became of each lease it did not place
/// `ready`. A call that fails as a whole is a test failure.
#[track_caller]
fn call(fx: &Fx) -> (usize, Vec<(&'static str, String)>) {
    let got: Result<PendingPlaced, Failure> = place_pending(&fx.ctx, PROJECT);
    let got = match got {
        Ok(got) => got,
        Err(f) => panic!("The call failed as a whole: {}", f.message),
    };
    for message in got.unplaced.iter().filter_map(|u| u.message()) {
        fx.assert_plain(message);
    }
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

fn want(list: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    list.iter().map(|(k, id)| (*k, id.to_string())).collect()
}

/// A lease that was placed: `ready`, with the credential and alias of a host that is not the box,
/// and every other member as it was, its `size` included.
#[track_caller]
fn assert_placed(fx: &Fx, before: &L, t0: i64, t1: i64) {
    let got = fx.lease_value(&before.id);
    let mut want = as_written(before);
    let alias = alias_for(&before.project, &before.id);
    {
        let o = want.as_object_mut().unwrap();
        o.insert("state".into(), json!("ready"));
        o.insert("expires_at".into(), got["expires_at"].clone());
        o.insert("ssh".into(), json!({"host_alias": alias, "user": "ubuntu"}));
    }
    assert_eq!(
        got, want,
        "Only state, expires_at and ssh change: the size asked for stays"
    );
    let expires = expires_of(&got);
    let ttl = i64::from(before.ttl);
    assert!(t0 + ttl <= expires && expires <= t1 + ttl);
    assert_eq!(
        fx.lease_files(&before.id),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
    assert_alias_bytes(fx, &before.id, &before.project, true);
}

/// A lease answered `unsatisfiable`: its state and one reason are all that changed.
#[track_caller]
fn assert_refused(fx: &Fx, before: &L) {
    let got = fx.lease_value(&before.id);
    let mut want = as_written(before);
    {
        let o = want.as_object_mut().unwrap();
        o.insert("state".into(), json!("unsatisfiable"));
        o.insert(
            "reasons".into(),
            json!([{
                "code": "no_capacity",
                "message": no_capacity_at_size(&before.project, &before.size),
            }]),
        );
    }
    assert_eq!(
        got, want,
        "It adds reasons and changes state, and no other member, driver included"
    );
    assert!(got.get("expires_at").is_none() && got.get("ssh").is_none());
    assert!(!fx.has_files_of(&before.project, &before.id));
}

/// A lease left as it was: its record, and no file of its own.
#[track_caller]
fn assert_left(fx: &Fx, before: &L) {
    assert_eq!(fx.lease_value(&before.id), as_written(before));
    assert!(!fx.has_files_of(&before.project, &before.id));
}

// ---- A host that meets the leases ---------------------------------------------------------------------

#[test]
fn a_larger_host_places_every_lease_it_meets_and_each_keeps_the_size_asked_for() {
    let fx = world(Some("m9g.2xlarge"));
    let a = L::pending(A, PROJECT).size("s");
    let b = L::pending(B, PROJECT).size("m");
    let c = L::pending(C, PROJECT).size("l");
    fx.write_leases(&[a.clone(), b.clone(), c.clone()]);
    let hosts_before = fx.hosts_bytes();
    let t0 = now();
    let got = call(&fx);
    let t1 = now();
    assert_eq!(got, (3, Vec::new()));
    for l in [&a, &b, &c] {
        assert_placed(&fx, l, t0, t1);
    }
    assert_eq!(
        fx.hosts_bytes(),
        hosts_before,
        "The registry is never written"
    );
}

#[test]
fn a_lease_the_host_does_not_meet_is_left_pending_when_no_live_ready_lease_holds_it() {
    let fx = world(Some("m9g.xlarge"));
    let a = L::pending(A, PROJECT).size("l");
    let d = L::pending(D, PROJECT).size("xl");
    fx.write_leases(&[a.clone(), d.clone()]);
    let hosts_before = fx.hosts_bytes();
    assert_eq!(
        call(&fx),
        (0, want(&[("left", A), ("left", D)])),
        "Nothing holds the host: both stay pending for the next run, which resizes it"
    );
    assert_left(&fx, &a);
    assert_left(&fx, &d);
    assert_eq!(fx.hosts_bytes(), hosts_before);
    // A second call leaves the same leases again and changes nothing.
    let after = fx.leases_bytes();
    assert_eq!(call(&fx), (0, want(&[("left", A), ("left", D)])));
    assert_eq!(fx.leases_bytes(), after);
}

#[test]
fn the_leases_the_call_places_hold_the_host_for_the_leases_it_does_not_meet() {
    // Step 2 places every lease the host meets, and step 3 then reads the file afresh: the leases
    // step 2 wrote `ready` are live ready leases of the project, so a lease the host does not meet
    // is refused, whatever its place in the file (driver.md, place_pending, step 3).
    let fx = world(Some("m9g.xlarge"));
    let a = L::pending(A, PROJECT).size("l");
    let b = L::pending(B, PROJECT).size("m");
    let c = L::pending(C, PROJECT).size("s");
    let d = L::pending(D, PROJECT).size("xl");
    fx.write_leases(&[a.clone(), b.clone(), c.clone(), d.clone()]);
    let hosts_before = fx.hosts_bytes();
    let t0 = now();
    let got = call(&fx);
    let t1 = now();
    assert_eq!(
        got,
        (2, want(&[("refused", A), ("refused", D)])),
        "B and C are met and placed; A and D are refused, since B and C now hold the host"
    );
    assert_placed(&fx, &b, t0, t1);
    assert_placed(&fx, &c, t0, t1);
    assert_refused(&fx, &a);
    assert_refused(&fx, &d);
    assert_eq!(fx.hosts_bytes(), hosts_before);
    // The file's order is kept.
    let ids: Vec<String> = fx
        .lease_values()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, [A, B, C, D]);
    // A second call has nothing pending to place or answer.
    let after = fx.leases_bytes();
    assert_eq!(call(&fx), (0, Vec::new()));
    assert_eq!(fx.leases_bytes(), after);
}

// ---- A live ready lease holds the host ----------------------------------------------------------------

#[test]
fn a_lease_the_host_does_not_meet_is_answered_no_capacity_while_a_live_ready_lease_holds_it() {
    let fx = world(Some("m9g.xlarge"));
    let a = L::pending(A, PROJECT).size("l");
    let b = L::pending(B, PROJECT).size("m");
    let c = L::pending(C, PROJECT).size("xl");
    let r = holder();
    fx.write_leases(&[a.clone(), r.clone(), b.clone(), c.clone()]);
    let hosts_before = fx.hosts_bytes();
    let t0 = now();
    let got = call(&fx);
    let t1 = now();
    assert_eq!(
        got,
        (1, want(&[("refused", A), ("refused", C)])),
        "B is met and placed; A and C are refused, whatever their place in the file"
    );
    assert_refused(&fx, &a);
    assert_refused(&fx, &c);
    assert_placed(&fx, &b, t0, t1);
    assert_eq!(
        fx.lease_value(R),
        as_written(&r),
        "The lease that holds the host is as it was"
    );
    assert_eq!(
        fx.hosts_bytes(),
        hosts_before,
        "The registry is never written"
    );
    let ids: Vec<String> = fx
        .lease_values()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, [A, R, B, C]);
    let bytes = fx.leases_bytes().unwrap();
    assert!(bytes.ends_with(b"}\n") && !bytes.ends_with(b"\n\n"));
}

#[test]
fn the_refusal_names_the_leases_own_project_and_size_byte_for_byte() {
    let fx = world(Some("m9g.large"));
    let a = L::pending(A, PROJECT).size("xl");
    fx.write_leases(&[holder(), a]);
    assert_eq!(call(&fx), (0, want(&[("refused", A)])));
    let record = fx.lease_value(A);
    assert_eq!(record["state"], "unsatisfiable");
    let reasons = record["reasons"].as_array().expect("reasons");
    assert_eq!(reasons.len(), 1, "Exactly one reason");
    assert_eq!(reasons[0]["code"], "no_capacity");
    assert_eq!(
        reasons[0]["message"],
        "No capacity can be found for project 'agentspace' at size 'xl': its host is serving other leases at a smaller size. Release them, or ask for a size they fit."
    );
    fx.assert_plain(reasons[0]["message"].as_str().unwrap());
}

#[test]
fn the_unplaced_leases_are_in_the_order_the_call_tried_them() {
    // Step 2 places the leases the host meets, in the file's order, so a met lease that fails is
    // tried before an unmet one that comes earlier in the file; step 3 then judges the unmet ones
    // in the file's order. For a host that a live ready lease holds, and for one that none holds.
    for held in [true, false] {
        let fx = world(Some("m9g.xlarge"));
        let a = L::pending(A, PROJECT).size("l");
        let b = L::pending(B, PROJECT).size("m");
        let c = L::pending(C, PROJECT).size("xl");
        let d = L::pending(D, PROJECT).size("l");
        let mut leases = vec![a.clone(), b.clone(), c.clone(), d.clone()];
        if held {
            leases.push(holder());
        }
        fx.write_leases(&leases);
        // B cannot be placed: a foreign file stands at its alias.
        let foreign = plant_foreign_alias(&fx, PROJECT, B);
        let (placed, kinds) = call(&fx);
        let second = if held { "refused" } else { "left" };
        assert_eq!(placed, 0, "held {held}");
        assert_eq!(
            kinds,
            want(&[("failed", B), (second, A), (second, C), (second, D)]),
            "held {held}: B was tried in step 2, then A, C and D in the file's order"
        );
        assert_eq!(fx.lease_value(B), as_written(&b), "B stays pending");
        assert_eq!(fs::read(fx.alias_file_of(PROJECT, B)).unwrap(), foreign);
        for l in [&a, &c, &d] {
            if held {
                assert_refused(&fx, l);
            } else {
                assert_left(&fx, l);
            }
        }
    }
}

#[test]
fn a_refused_lease_loses_its_leftover_files_and_one_that_cannot_lose_them_stays_pending() {
    // Pin P21: the leftover credential files of a refused lease are removed first, as release
    // removes them; a removal that fails leaves the lease pending and names it as failed.
    let fx = world(Some("m9g.xlarge"));
    let a = L::pending(A, PROJECT).size("l");
    let b = L::pending(B, PROJECT).size("xl");
    fx.write_leases(&[a.clone(), b.clone(), holder()]);
    // A's leftovers: a killed placement's key and public key.
    fs::create_dir_all(fx.lease_dir(A)).unwrap();
    fs::write(fx.lease_dir(A).join("id_ed25519"), b"a key").unwrap();
    fs::write(fx.lease_dir(A).join("id_ed25519.pub"), b"a public key").unwrap();
    // B's alias place holds a file that is nobody's: it MUST be kept, and B stays pending.
    let foreign = plant_foreign_alias(&fx, PROJECT, B);
    let got = call(&fx);
    assert_eq!(got, (0, want(&[("refused", A), ("failed", B)])));
    assert_refused(&fx, &a);
    assert!(!fx.lease_dir(A).exists(), "The leftovers are removed");
    assert_eq!(fx.lease_value(B), as_written(&b), "B stays pending");
    assert_eq!(fs::read(fx.alias_file_of(PROJECT, B)).unwrap(), foreign);
}

#[test]
fn only_a_live_ready_lease_of_the_project_holds_the_host_when_leases_are_answered() {
    let cases: Vec<(&str, Vec<L>, &str)> = vec![
        ("a live ready lease", vec![holder()], "refused"),
        ("an expired lease", vec![L::expired(R, PROJECT)], "left"),
        (
            "a released lease",
            vec![L::released_after_ready(R, PROJECT)],
            "left",
        ),
        (
            "an unsatisfiable lease",
            vec![L::unsatisfiable(R, PROJECT)],
            "left",
        ),
        (
            "a pending lease of another driver",
            vec![L::pending(R, PROJECT).driver(Some("fake"))],
            "left",
        ),
        (
            "another project's live ready lease",
            vec![L::ready(R, "other-project")],
            "left",
        ),
        ("no other lease", vec![], "left"),
    ];
    for (what, others, kind) in cases {
        let fx = world(Some("m9g.xlarge"));
        let a = L::pending(A, PROJECT).size("l");
        let mut leases = vec![a.clone()];
        leases.extend(others);
        fx.write_leases(&leases);
        assert_eq!(call(&fx), (0, want(&[(kind, A)])), "{what}");
        if kind == "refused" {
            assert_refused(&fx, &a);
        } else {
            assert_left(&fx, &a);
        }
    }
}

// ---- A record that meets nothing ----------------------------------------------------------------------

#[test]
fn a_record_with_no_type_or_a_type_the_map_does_not_hold_meets_no_size() {
    // Such a record meets no size, so nothing is placed, and it is not known to be smaller than any
    // lease, so every lease is left pending, written nothing, whatever leases hold the host
    // (architect, 2026-11-23, for v0.09-T03, pin P40).
    for ty in [None, Some("m9g.8xlarge"), Some("c7g.large")] {
        for held in [false, true] {
            let fx = world(ty);
            let a = L::pending(A, PROJECT).size("s");
            let b = L::pending(B, PROJECT).size("m");
            let mut leases = vec![a.clone(), b.clone()];
            if held {
                leases.push(holder());
            }
            fx.write_leases(&leases);
            let hosts_before = fx.hosts_bytes();
            let leases_before = fx.leases_bytes();
            assert_eq!(
                call(&fx),
                (0, want(&[("left", A), ("left", B)])),
                "type {ty:?}, held {held}: nothing is placed, and nothing is answered"
            );
            for l in [&a, &b] {
                assert_left(&fx, l);
            }
            assert_eq!(
                fx.leases_bytes(),
                leases_before,
                "type {ty:?}, held {held}: the lease state file is not written"
            );
            assert_eq!(fx.hosts_bytes(), hosts_before, "type {ty:?}");
        }
    }
}

// ---- Records a run would not place --------------------------------------------------------------------

#[test]
fn a_pending_lease_with_a_size_outside_the_four_is_neither_placed_nor_answered() {
    // Only a hand edit makes one. Answering it no_capacity would name capacity for a size no
    // backend supports, so it stays pending until it is released (pin P20).
    let fx = world(Some("m9g.xlarge"));
    let a = L::pending(A, PROJECT).size("xxl");
    let b = L::pending(B, PROJECT).size("m");
    fx.write_leases(&[a.clone(), b.clone(), holder()]);
    let t0 = now();
    let (placed, kinds) = call(&fx);
    let t1 = now();
    assert_eq!(placed, 1);
    assert!(
        !kinds.contains(&("refused", A.to_string())),
        "A is not answered: {kinds:?}"
    );
    assert_eq!(fx.lease_value(A), as_written(&a), "A stays pending");
    assert!(!fx.has_files_of(PROJECT, A));
    assert_placed(&fx, &b, t0, t1);
}

#[test]
fn a_pending_lease_of_another_driver_or_project_is_not_this_calls_even_when_unmet() {
    let fx = world(Some("m9g.xlarge"));
    let other_driver = L::pending(A, PROJECT).size("xl").driver(Some("fake"));
    let no_driver = L::pending(B, PROJECT).size("xl").driver(None);
    let other_project = L::pending(C, "other-project").size("xl");
    fx.write_leases(&[
        other_driver.clone(),
        no_driver.clone(),
        other_project.clone(),
        holder(),
    ]);
    assert_eq!(call(&fx), (0, Vec::new()));
    for l in [&other_driver, &no_driver, &other_project] {
        assert_left(&fx, l);
    }
}

#[test]
fn a_host_that_is_not_ready_or_has_no_record_places_and_answers_nothing() {
    for extra in [
        vec![H::creating(PROJECT).typed("m9g.large")],
        vec![H::failed(PROJECT).typed("m9g.large").instance(INSTANCE)],
        vec![],
    ] {
        let fx = world(Some("m9g.xlarge"));
        fx.write_hosts(&extra);
        fx.write_leases(&[
            L::pending(A, PROJECT).size("xl"),
            L::pending(B, PROJECT).size("s"),
            holder(),
        ]);
        let leases = fx.leases_bytes();
        let hosts = fx.hosts_bytes();
        assert_eq!(call(&fx), (0, Vec::new()));
        assert_eq!(fx.leases_bytes(), leases);
        assert_eq!(fx.hosts_bytes(), hosts, "The registry is never written");
        assert!(!fx.has_files_of(PROJECT, A) && !fx.has_files_of(PROJECT, B));
    }
}

#[test]
fn a_lease_the_host_meets_is_placed_whether_or_not_a_live_ready_lease_holds_it() {
    let fx = world(Some("m9g.4xlarge"));
    let a = L::pending(A, PROJECT).size("xl");
    let b = L::pending(B, PROJECT).size("s");
    // The holder is built once: its times are taken from the clock when it is built.
    let r = holder();
    fx.write_leases(&[r.clone(), a.clone(), b.clone()]);
    let t0 = now();
    assert_eq!(call(&fx), (2, Vec::new()));
    let t1 = now();
    assert_placed(&fx, &a, t0, t1);
    assert_placed(&fx, &b, t0, t1);
    assert_eq!(fx.lease_value(R), as_written(&r));
}

#[test]
fn the_boxs_implied_record_meets_m_and_below_and_is_never_written() {
    // A missing hosts.json reads as the box's record at m9g.xlarge (P2c).
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    assert!(fx.hosts_bytes().is_none());
    let a = L::pending(A, "swimcoach").size("m");
    let b = L::pending(B, "swimcoach").size("l");
    let c = L::pending(C, "swimcoach").size("s");
    fx.write_leases(&[a.clone(), b.clone(), c.clone()]);
    let got = place_pending(&fx.ctx, "swimcoach").expect("The call succeeds");
    assert_eq!(got.placed, 2, "The leases for m and s are met");
    assert_eq!(got.unplaced.len(), 1);
    assert!(
        matches!(&got.unplaced[0], Unplaced::Refused { lease_id } if lease_id == B),
        "The one for l is refused, since the two placed now hold the host: {:?}",
        got.unplaced
    );
    assert!(
        fx.hosts_bytes().is_none(),
        "A missing hosts.json stays missing"
    );
    assert_eq!(fx.lease_value(A)["state"], "ready");
    assert_eq!(fx.lease_value(C)["state"], "ready");
    assert_refused(&fx, &b);

    // With nothing else, the lease for l is left pending.
    let mut fx = Fx::with_ca();
    fx.state_elsewhere();
    fx.write_leases(std::slice::from_ref(&b));
    let got = place_pending(&fx.ctx, "swimcoach").expect("The call succeeds");
    assert_eq!(got.placed, 0);
    assert!(matches!(&got.unplaced[..], [Unplaced::Left { lease_id }] if lease_id == B));
    assert_left(&fx, &b);
    assert!(fx.hosts_bytes().is_none());
}

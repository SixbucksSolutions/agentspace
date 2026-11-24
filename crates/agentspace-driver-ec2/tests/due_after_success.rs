//! A run is due at once after a run that succeeded (docs/driver.md, Starting a provisioning run,
//! steps 2 and 4 and the bullets after them, "From `check_pending`", After the run; docs/state.md,
//! The last success; docs/decisions.md, D77 and its change note; E0.10-T02-S03, card #0216).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. The rule, from
//! driver.md step 2: **a run is due only when no program holds the project's `provision.lock`, and
//! either the record's `last_attempt_at` is absent or at least 5 minutes before now, or the run that
//! started at `last_attempt_at` succeeded**, which it did when the record's `last_success_at` is
//! present and at or after its `last_attempt_at`. Step 4: **the write that sets `last_attempt_at`
//! removes `last_success_at` in the same write**, and the driver writes it nowhere else.
//!
//! Both entry points are tried against each rule: `get`, which is `check_pending`, and `create`,
//! which is `place`, whose `ready` host at a smaller size than the lease's is row 6 of driver.md's
//! table and so starts a run when one is due (conformance Y2 to Y5, E0.10-T04-S01). Every start
//! goes through a fake `Detach`. No test sleeps or waits on the clock: each seeds times relative
//! to now with minutes of margin, never within seconds of the 5 minutes.
#![cfg(windows)]

mod common;
#[path = "common/pending.rs"]
mod pending;

use agentspace_driver_api::{Driver, LeaseRef, Placed};
use agentspace_driver_ec2::Ec2Driver;
use common::*;
use pending::*;
use std::sync::Arc;

const PROJECT: &str = "agentspace";
const BOX: &str = "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}";
const STATES: [&str; 3] = ["ready", "creating", "failed"];

#[derive(Clone, Copy, Debug)]
enum Via {
    /// A `get` of the `pending` lease: `check_pending`.
    Get,
    /// A `create --size l`: `place`, which answers `pending` for each of the three states.
    Create,
}

const VIAS: [Via; 2] = [Via::Get, Via::Create];

fn world() -> (Fx, Arc<FakeDetach>) {
    let mut fx = Fx::with_ca();
    fx.install_operator();
    fx.write_leases(&[]);
    let detach = FakeDetach::observing(&fx.home, PROJECT);
    (fx, detach)
}

/// The project's record as step 4 writes the members, in the table's order: the type is there so
/// that `ready` is row 6 of place's table for a `create --size l`, and is no part of the rule.
fn record(state: &str, attempt: Option<i64>, success: Option<i64>) -> String {
    let mut s = format!(
        "{{\"project\":\"{PROJECT}\",\"stack\":\"agentspace-host-{PROJECT}\",\"state\":\"{state}\",\"instance_type\":\"{DEFAULT_TYPE}\""
    );
    if let Some(at) = attempt {
        s.push_str(&format!(",\"last_attempt_at\":\"{}\"", iso(at)));
    }
    if let Some(at) = success {
        s.push_str(&format!(",\"last_success_at\":\"{}\"", iso(at)));
    }
    s.push('}');
    s
}

/// The registry's bytes: the box's record, then these.
fn doc(records: &[String]) -> Vec<u8> {
    format!(
        "{{\"version\":0,\"hosts\":[{BOX},{}]}}\n",
        records.join(",")
    )
    .into_bytes()
}

/// Seeds the project's record: its `last_attempt_at` is `age` seconds before now, and its
/// `last_success_at` is `delta` seconds after that (before it when `delta` is negative), or absent.
/// Returns the seconds the attempt was at.
fn seed(fx: &Fx, state: &str, age: i64, delta: Option<i64>) -> i64 {
    let at = now() - age;
    fx.write_hosts_raw(&doc(&[record(state, Some(at), delta.map(|d| at + d))]));
    at
}

/// One `get` or one `create`, which must succeed and answer `pending`.
#[track_caller]
fn enter(fx: &Fx, d: &Ec2Driver, via: Via) {
    match via {
        Via::Get => d
            .check_pending(
                &fx.ctx,
                &LeaseRef {
                    id: ID,
                    project: PROJECT,
                },
            )
            .expect("check_pending succeeds"),
        Via::Create => match fx.place_sized(d, PROJECT, &id_n(1), "l") {
            Ok(Placed::Pending) => {}
            other => panic!("Expected pending: {other:?}"),
        },
    }
}

/// A run was started and its write is what step 4 says: `last_attempt_at` is now, `last_success_at`
/// is gone, a `ready` record stays `ready` and any other is `creating`, and nothing else changed.
#[track_caller]
fn assert_started(fx: &Fx, detach: &FakeDetach, state: &str, t0: i64, t1: i64, label: &str) {
    assert_eq!(detach.count(), 1, "{label}: one run is started");
    let start = detach.only();
    assert_eq!(start.args, ["provision", "-Host", PROJECT], "{label}");
    assert!(
        start.seen.hosts_lock_free,
        "{label}: hosts.lock is let go before the start"
    );
    let got = fx.host(PROJECT).expect("The record is there");
    let at = epoch_of(
        got.last_attempt_at
            .as_deref()
            .expect("last_attempt_at is set"),
    );
    assert!(
        t0 <= at && at <= t1,
        "{label}: last_attempt_at is the time of the start"
    );
    assert_eq!(
        got.last_success_at, None,
        "{label}: the write that sets last_attempt_at removes last_success_at"
    );
    let after = if state == "ready" {
        "ready"
    } else {
        "creating"
    };
    assert_eq!(
        fx.hosts_bytes().unwrap(),
        doc(&[record(after, Some(at), None)]),
        "{label}: only last_attempt_at, last_success_at and, for a record not ready, state changed"
    );
}

/// No run was started and the registry is as it was, byte for byte.
#[track_caller]
fn assert_waits(fx: &Fx, detach: &FakeDetach, before: &Option<Vec<u8>>, label: &str) {
    assert_eq!(detach.count(), 0, "{label}: no run is started");
    assert_eq!(
        &fx.hosts_bytes(),
        before,
        "{label}: the registry is as it was, so nothing was written"
    );
}

// ---- Due at once after a success -----------------------------------------------------------------------------

#[test]
fn a_success_at_or_after_the_attempt_makes_the_next_start_due_at_once() {
    // (age of the attempt, seconds from the attempt to the success): a success in the very second
    // of the attempt reads as one (pin P24), and one after it is one, whatever the time since,
    // even a few seconds after the start and well inside the 5 minutes.
    for state in STATES {
        for via in VIAS {
            for (age, delta) in [(10, 0), (240, 1), (3600, 0), (3600, 45), (86_400, 600)] {
                let (fx, detach) = world();
                seed(&fx, state, age, Some(delta));
                let d = fx.driver_with(&detach);
                let t0 = now();
                enter(&fx, &d, via);
                let t1 = now();
                assert_started(
                    &fx,
                    &detach,
                    state,
                    t0,
                    t1,
                    &format!("{state} {via:?} age {age} delta {delta}"),
                );
            }
        }
    }
}

#[test]
fn a_record_with_a_success_and_no_attempt_is_due_by_the_absent_attempt_alone() {
    // state.md: a record with no `last_attempt_at` is due by that alone, whatever `last_success_at`
    // holds.
    for via in VIAS {
        let (fx, detach) = world();
        fx.write_hosts_raw(&doc(&[record("ready", None, Some(now() - 30))]));
        let d = fx.driver_with(&detach);
        let t0 = now();
        enter(&fx, &d, via);
        let t1 = now();
        assert_started(&fx, &detach, "ready", t0, t1, &format!("{via:?}"));
    }
}

#[test]
fn after_a_success_only_the_start_after_it_is_due_at_once_and_only_a_new_success_makes_the_next_so()
{
    // driver.md: "After a run that succeeded, the next start is due at once ... and sets
    // last_attempt_at again and removes last_success_at as step 4 says, so the start after that
    // waits 5 minutes unless that run succeeds in turn." The run's success is written here the way
    // a run writes it, to the record, under the registry lock.
    for via in VIAS {
        let (fx, detach) = world();
        seed(&fx, "ready", 3600, Some(30));
        let d = fx.driver_with(&detach);
        enter(&fx, &d, via);
        assert_eq!(
            detach.count(),
            1,
            "{via:?}: the success makes the first start due"
        );
        let after_first = fx.hosts_bytes();
        for again in [Via::Get, Via::Create, Via::Get] {
            enter(&fx, &d, again);
        }
        assert_eq!(
            detach.count(),
            1,
            "{via:?}: the start before the run has succeeded in turn waits 5 minutes"
        );
        assert_eq!(fx.hosts_bytes(), after_first, "{via:?}");

        // That run succeeds: its end writes `last_success_at`, at or after its `last_attempt_at`.
        {
            let guard = agentspace_hosts::lock(&fx.home).expect("The registry lock is free");
            let mut hosts = agentspace_hosts::read(&fx.home).expect("The registry reads");
            let rec = hosts
                .hosts
                .iter_mut()
                .find(|h| h.project == PROJECT)
                .expect("The record is there");
            rec.last_success_at = Some(iso(now()));
            agentspace_hosts::write(&fx.home, &hosts, &guard).expect("The registry is written");
        }
        enter(&fx, &d, via);
        assert_eq!(
            detach.count(),
            2,
            "{via:?}: a success of the run that started last makes the next start due at once"
        );
        assert_eq!(
            fx.host(PROJECT).unwrap().last_success_at,
            None,
            "{via:?}: and that start removed it again"
        );
        enter(&fx, &d, Via::Get);
        assert_eq!(detach.count(), 2, "{via:?}: the start after it waits again");
    }
}

// ---- Not due: a failure, a death, a start whose run has not taken its lock ----------------------------------

#[test]
fn without_a_success_of_the_run_that_started_last_the_five_minutes_hold() {
    // In each case `last_success_at` is absent, or before `last_attempt_at` by a second or by a
    // run's length: a run that failed or died, or a start whose run has not yet taken its lock.
    for state in STATES {
        for via in VIAS {
            for (age, delta) in [
                (60, None),
                (60, Some(-120)),
                (60, Some(-1)),
                (240, None),
                (240, Some(-3600)),
                (1, None),
            ] {
                let (fx, detach) = world();
                seed(&fx, state, age, delta);
                let before = fx.hosts_bytes();
                let d = fx.driver_with(&detach);
                enter(&fx, &d, via);
                enter(&fx, &d, via);
                assert_waits(
                    &fx,
                    &detach,
                    &before,
                    &format!("{state} {via:?} age {age} delta {delta:?}"),
                );
            }
        }
    }
}

#[test]
fn five_minutes_after_a_run_that_did_not_succeed_a_run_is_due_as_before_and_drops_a_stale_success()
{
    // driver.md: "a run is started for a host at most once in any 5 minutes, unless the last run
    // succeeded". The same write that sets `last_attempt_at` removes a `last_success_at` that is
    // before it.
    for state in STATES {
        for via in VIAS {
            for (age, delta) in [
                (400, None),
                (400, Some(-90)),
                (3600, None),
                (3600, Some(-1)),
            ] {
                let (fx, detach) = world();
                seed(&fx, state, age, delta);
                let d = fx.driver_with(&detach);
                let t0 = now();
                enter(&fx, &d, via);
                let t1 = now();
                assert_started(
                    &fx,
                    &detach,
                    state,
                    t0,
                    t1,
                    &format!("{state} {via:?} age {age} delta {delta:?}"),
                );
                // And the start after that one waits.
                let after = fx.hosts_bytes();
                enter(&fx, &d, via);
                assert_eq!(detach.count(), 1, "{state} {via:?}");
                assert_eq!(fx.hosts_bytes(), after, "{state} {via:?}");
            }
        }
    }
}

// ---- A held lock stops a start that a success would make due -------------------------------------------------

#[test]
fn a_held_provision_lock_stops_a_start_that_a_success_would_make_due() {
    for state in STATES {
        for via in VIAS {
            for (age, delta) in [(60, 0), (240, 1), (3600, 30)] {
                let (fx, detach) = world();
                seed(&fx, state, age, Some(delta));
                let before = fx.hosts_bytes();
                let d = fx.driver_with(&detach);
                let label = format!("{state} {via:?} age {age} delta {delta}");
                {
                    let _run = agentspace_hosts::try_run_lock(&fx.home, PROJECT)
                        .unwrap()
                        .expect("The run lock is free");
                    enter(&fx, &d, via);
                    enter(&fx, &d, via);
                    assert_waits(&fx, &detach, &before, &label);
                }
                // The lock was the only thing that stopped it.
                let t0 = now();
                enter(&fx, &d, via);
                let t1 = now();
                assert_started(&fx, &detach, state, t0, t1, &label);
            }
        }
    }
}

// ---- A start that fails still removes the success -----------------------------------------------------------

#[test]
fn a_start_that_fails_leaves_the_record_as_step_four_wrote_it_and_starts_no_loop() {
    // driver.md: "A start that fails changes nothing more, and fails nothing: the record stays as
    // step 4 wrote it, so the next start is due 5 minutes later." Step 4 removed the success, so a
    // run that could not be started is not read as one that succeeded.
    for state in STATES {
        for via in VIAS {
            let mut fx = Fx::with_ca();
            fx.install_operator();
            fx.write_leases(&[]);
            let detach = FakeDetach::failing("The program could not be started.");
            let d = fx.driver_with(&detach);
            seed(&fx, state, 3600, Some(10));
            let t0 = now();
            enter(&fx, &d, via);
            let t1 = now();
            assert_eq!(detach.count(), 1, "{state} {via:?}");
            let got = fx.host(PROJECT).unwrap();
            let at = epoch_of(got.last_attempt_at.as_deref().unwrap());
            assert!(t0 <= at && at <= t1, "{state} {via:?}");
            assert_eq!(got.last_success_at, None, "{state} {via:?}");
            let after = fx.hosts_bytes();
            enter(&fx, &d, via);
            enter(&fx, &d, Via::Get);
            assert_eq!(
                detach.count(),
                1,
                "{state} {via:?}: the next start waits 5 minutes"
            );
            assert_eq!(fx.hosts_bytes(), after, "{state} {via:?}");
        }
    }
}

// ---- The driver never writes a success -----------------------------------------------------------------------

#[test]
fn the_driver_never_writes_a_success_of_its_own() {
    // A record that never held one is not given one by a start, whatever the state: only the run
    // writes it (driver.md, "`last_success_at` is the time a run that succeeded ended").
    for state in STATES {
        for via in VIAS {
            let (fx, detach) = world();
            seed(&fx, state, 3600, None);
            let d = fx.driver_with(&detach);
            enter(&fx, &d, via);
            let text = String::from_utf8(fx.hosts_bytes().unwrap()).unwrap();
            assert!(
                !text.contains("last_success_at"),
                "{state} {via:?}: a start writes no success"
            );
        }
    }
}

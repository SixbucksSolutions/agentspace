//! Live leases and the lease horizon (docs/state.md, Live leases and the lease horizon; lease-api.md,
//! States and Times; D56, D63): which leases are live, how each one ends, and a project's last lease
//! end and horizon, as functions of the records, the reading's time and the launch time.
//!
//! Records are read through `parse` where that is natural, so the times are the contract's strings.

mod common;

use agentspace_state::{
    Record, format_time, is_live, last_lease_end, lease_end, lease_horizon, live_count, parse,
    parse_time,
};
use common::*;
use serde_json::{Value, json};

const PROJECT: &str = "demo";
const OTHER: &str = "elsewhere";
const LAUNCH: &str = "2026-11-19T07:30:00Z";

/// A made-up, valid ID for the n-th record of a test.
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

/// The Unix seconds of a contract time.
fn at(text: &str) -> i64 {
    parse_time(text).expect("A fixture time must be a real instant")
}

/// A time on 2026-11-19, as the contract writes it.
fn t(hms: &str) -> String {
    format!("2026-11-19T{hms}Z")
}

/// A record of the given stored state, as the file would hold it.
fn lease(
    n: u32,
    project: &str,
    state: &str,
    expires: Option<&str>,
    released: Option<&str>,
) -> Value {
    let mut rec = json!({
        "id": id(n),
        "project": project,
        "state": state,
        "platform": "linux/arm64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": 3600,
        "created_at": "2026-11-19T07:00:00Z",
        "driver": "ec2"
    });
    let o = rec.as_object_mut().unwrap();
    if state == "ready" {
        o.insert(
            "ssh".into(),
            json!({"host_alias": format!("agentspace-{project}-{n:08}"), "user": "ubuntu"}),
        );
    }
    if state == "unsatisfiable" {
        o.insert(
            "reasons".into(),
            json!([{"code": "unsupported_platform", "message": "Platform 'windows/amd64' is not supported."}]),
        );
    }
    if let Some(e) = expires {
        o.insert("expires_at".into(), json!(e));
    }
    if let Some(r) = released {
        o.insert("released_at".into(), json!(r));
    }
    rec
}

fn ready_until(n: u32, project: &str, expires: &str) -> Value {
    lease(n, project, "ready", Some(expires), None)
}

fn pending_lease(n: u32, project: &str) -> Value {
    lease(n, project, "pending", None, None)
}

fn released(n: u32, project: &str, expires: Option<&str>, released_at: &str) -> Value {
    lease(n, project, "released", expires, Some(released_at))
}

fn unsat(n: u32, project: &str) -> Value {
    lease(n, project, "unsatisfiable", None, None)
}

/// Records read through `parse`, which is what a program holds.
fn records(values: Vec<Value>) -> Vec<Record> {
    parse(&bytes_of(values))
        .expect("The fixture records must be a valid file")
        .leases
}

fn one(value: Value) -> Record {
    records(vec![value]).remove(0)
}

fn horizon(recs: &[Record], now: &str) -> i64 {
    lease_horizon(recs, PROJECT, at(now), at(LAUNCH))
}

// ---- Which leases are live ----

#[test]
fn a_pending_lease_is_live() {
    let r = one(pending_lease(1, PROJECT));
    assert!(is_live(&r, at(&t("08:00:00"))));
    // It has no expires_at, so no later time makes it not live.
    assert!(is_live(&r, at("2030-01-01T00:00:00Z")));
}

#[test]
fn a_ready_lease_is_live_until_its_expiry_to_the_second() {
    let r = one(ready_until(1, PROJECT, &t("10:00:00")));
    let end = at(&t("10:00:00"));
    assert!(is_live(&r, end - 3600), "Long before");
    assert!(is_live(&r, end - 1), "One second before is live");
    assert!(!is_live(&r, end), "At expires_at it is not live");
    assert!(!is_live(&r, end + 1), "One second after is not live");
    assert!(!is_live(&r, end + 86_400), "A day after is not live");
}

#[test]
fn a_ready_lease_past_its_expiry_is_not_live() {
    // A record stored ready whose expires_at has passed is expired, which is not live.
    let r = one(ready_until(1, PROJECT, &t("08:00:00")));
    assert!(!is_live(&r, at(&t("09:45:00"))));
}

#[test]
fn a_released_lease_is_not_live() {
    let r = one(released(1, PROJECT, Some(&t("11:00:00")), &t("09:30:00")));
    // Not live before its expires_at either: release ended it.
    assert!(!is_live(&r, at(&t("09:45:00"))));
    assert!(!is_live(&r, at(&t("09:00:00"))));
}

#[test]
fn a_released_lease_that_was_never_ready_is_not_live() {
    let r = one(released(1, PROJECT, None, &t("09:10:00")));
    assert!(!is_live(&r, at(&t("09:50:00"))));
}

#[test]
fn an_unsatisfiable_lease_is_not_live() {
    let r = one(unsat(1, PROJECT));
    assert!(!is_live(&r, at(&t("09:50:00"))));
}

#[test]
fn a_state_that_is_none_of_the_four_is_not_live() {
    // `expired` is never stored, and a hand-built record with it, or with a made-up state, is not live.
    for state in ["expired", "bogus", ""] {
        let mut r = one(pending_lease(1, PROJECT));
        r.state = state.to_string();
        assert!(!is_live(&r, at(&t("09:50:00"))), "State {state:?}");
    }
}

// ---- How each lease ends ----

#[test]
fn a_live_lease_has_no_end() {
    let now = at(&t("09:45:00"));
    assert_eq!(lease_end(&one(pending_lease(1, PROJECT)), now), None);
    assert_eq!(
        lease_end(&one(ready_until(2, PROJECT, &t("10:00:00"))), now),
        None
    );
}

#[test]
fn an_unsatisfiable_lease_has_no_end() {
    let r = one(unsat(1, PROJECT));
    assert_eq!(lease_end(&r, at(&t("09:45:00"))), None);
    assert_eq!(lease_end(&r, at("2031-01-01T00:00:00Z")), None);
}

#[test]
fn a_ready_lease_at_or_past_its_expiry_ends_at_its_expires_at() {
    let r = one(ready_until(1, PROJECT, &t("10:00:00")));
    let end = at(&t("10:00:00"));
    assert_eq!(lease_end(&r, end - 1), None, "Still live one second before");
    assert_eq!(lease_end(&r, end), Some(end), "At expires_at it has ended");
    assert_eq!(lease_end(&r, end + 1), Some(end));
    assert_eq!(lease_end(&r, end + 86_400), Some(end));
}

#[test]
fn a_lease_released_before_it_expired_ends_at_its_released_at() {
    // Example B: released 09:30, expires_at 11:00.
    let r = one(released(1, PROJECT, Some(&t("11:00:00")), &t("09:30:00")));
    let want = Some(at(&t("09:30:00")));
    assert_eq!(lease_end(&r, at(&t("09:45:00"))), want);
    assert_eq!(lease_end(&r, at(&t("10:05:00"))), want);
    assert_eq!(lease_end(&r, at(&t("11:30:00"))), want);
}

#[test]
fn a_lease_released_after_it_expired_ends_at_its_expires_at() {
    // Example E: released 09:35, expires_at 09:20, so it stopped being live at 09:20.
    let r = one(released(1, PROJECT, Some(&t("09:20:00")), &t("09:35:00")));
    let want = Some(at(&t("09:20:00")));
    assert_eq!(lease_end(&r, at(&t("09:45:00"))), want);
    assert_eq!(lease_end(&r, at(&t("11:30:00"))), want);
}

#[test]
fn a_lease_released_in_the_second_it_expired_ends_there() {
    let r = one(released(1, PROJECT, Some(&t("09:20:00")), &t("09:20:00")));
    assert_eq!(lease_end(&r, at(&t("09:45:00"))), Some(at(&t("09:20:00"))));
}

#[test]
fn a_lease_released_while_pending_ends_at_its_released_at() {
    // Example G: never ready, so no expires_at.
    let r = one(released(1, PROJECT, None, &t("09:10:00")));
    assert_eq!(lease_end(&r, at(&t("09:50:00"))), Some(at(&t("09:10:00"))));
    assert_eq!(lease_end(&r, at(&t("10:10:00"))), Some(at(&t("09:10:00"))));
}

#[test]
fn an_end_that_has_come_does_not_move_with_the_reading_time() {
    let r = one(ready_until(1, PROJECT, &t("10:00:00")));
    let end = at(&t("10:00:00"));
    assert_eq!(lease_end(&r, end + 1), lease_end(&r, end + 1_000_000));
}

// ---- Counting a project's live leases ----

#[test]
fn live_count_counts_pending_and_unexpired_ready_leases() {
    let now = at(&t("09:45:00"));
    let recs = records(vec![
        pending_lease(1, PROJECT),
        ready_until(2, PROJECT, &t("10:00:00")),
        ready_until(3, PROJECT, &t("09:45:01")),
    ]);
    assert_eq!(live_count(&recs, PROJECT, now), 3);
}

#[test]
fn live_count_leaves_out_every_lease_that_is_not_live() {
    let now = at(&t("09:45:00"));
    let recs = records(vec![
        ready_until(1, PROJECT, &t("10:00:00")),
        ready_until(2, PROJECT, &t("09:45:00")), // Expired at the reading's own second.
        ready_until(3, PROJECT, &t("08:00:00")),
        released(4, PROJECT, Some(&t("11:00:00")), &t("09:30:00")),
        released(5, PROJECT, None, &t("09:10:00")),
        unsat(6, PROJECT),
    ]);
    assert_eq!(live_count(&recs, PROJECT, now), 1);
}

#[test]
fn live_count_of_nothing_is_zero() {
    let now = at(&t("09:45:00"));
    assert_eq!(live_count(&[], PROJECT, now), 0);
    let recs = records(vec![unsat(1, PROJECT)]);
    assert_eq!(live_count(&recs, PROJECT, now), 0);
}

#[test]
fn live_count_follows_the_reading_time() {
    let recs = records(vec![ready_until(1, PROJECT, &t("10:00:00"))]);
    assert_eq!(live_count(&recs, PROJECT, at(&t("09:59:59"))), 1);
    assert_eq!(live_count(&recs, PROJECT, at(&t("10:00:00"))), 0);
}

#[test]
fn live_count_ignores_another_projects_leases() {
    let now = at(&t("09:45:00"));
    let recs = records(vec![
        ready_until(1, PROJECT, &t("10:00:00")),
        ready_until(2, OTHER, &t("10:00:00")),
        pending_lease(3, OTHER),
    ]);
    assert_eq!(live_count(&recs, PROJECT, now), 1);
    assert_eq!(live_count(&recs, OTHER, now), 2);
    assert_eq!(live_count(&recs, "nobody", now), 0);
}

#[test]
fn a_project_is_matched_exactly_not_by_prefix_or_case() {
    let now = at(&t("09:45:00"));
    let recs = records(vec![
        pending_lease(1, "demo-two"),
        pending_lease(2, "dem"),
        pending_lease(3, "demo"),
    ]);
    assert_eq!(live_count(&recs, "demo", now), 1);
    assert_eq!(live_count(&recs, "DEMO", now), 0);
    assert_eq!(live_count(&recs, "", now), 0);
}

// ---- The last lease end ----

#[test]
fn the_last_lease_end_is_the_latest_end() {
    let now = at(&t("11:30:00"));
    let recs = records(vec![
        released(1, PROJECT, Some(&t("12:00:00")), &t("09:30:00")),
        ready_until(2, PROJECT, &t("10:00:00")),
        ready_until(3, PROJECT, &t("08:00:00")),
        released(4, PROJECT, Some(&t("09:20:00")), &t("09:35:00")),
    ]);
    assert_eq!(
        last_lease_end(&recs, PROJECT, now),
        Some(at(&t("10:00:00")))
    );
}

#[test]
fn the_last_lease_end_can_be_a_release() {
    let now = at(&t("11:30:00"));
    let recs = records(vec![
        ready_until(1, PROJECT, &t("09:00:00")),
        released(2, PROJECT, Some(&t("12:00:00")), &t("10:15:00")),
    ]);
    assert_eq!(
        last_lease_end(&recs, PROJECT, now),
        Some(at(&t("10:15:00")))
    );
}

#[test]
fn the_last_lease_end_uses_the_expiry_of_a_lease_released_after_it_expired() {
    let now = at(&t("11:30:00"));
    let recs = records(vec![released(
        1,
        PROJECT,
        Some(&t("09:20:00")),
        &t("09:35:00"),
    )]);
    assert_eq!(
        last_lease_end(&recs, PROJECT, now),
        Some(at(&t("09:20:00")))
    );
}

#[test]
fn a_live_lease_has_no_end_to_count() {
    let now = at(&t("09:45:00"));
    let recs = records(vec![
        ready_until(1, PROJECT, &t("10:00:00")),
        pending_lease(2, PROJECT),
    ]);
    assert_eq!(last_lease_end(&recs, PROJECT, now), None);
}

#[test]
fn an_expiry_still_ahead_is_no_end_yet() {
    // Example A: no end at 09:45, and its end is 10:00 at 10:05.
    let recs = records(vec![ready_until(1, PROJECT, &t("10:00:00"))]);
    assert_eq!(last_lease_end(&recs, PROJECT, at(&t("09:45:00"))), None);
    assert_eq!(
        last_lease_end(&recs, PROJECT, at(&t("10:05:00"))),
        Some(at(&t("10:00:00")))
    );
}

#[test]
fn unsatisfiable_leases_have_no_end_to_count() {
    let recs = records(vec![unsat(1, PROJECT), unsat(2, PROJECT)]);
    assert_eq!(last_lease_end(&recs, PROJECT, at(&t("11:30:00"))), None);
}

#[test]
fn a_project_with_no_lease_has_no_last_lease_end() {
    assert_eq!(last_lease_end(&[], PROJECT, at(&t("11:30:00"))), None);
}

#[test]
fn the_last_lease_end_ignores_another_projects_leases() {
    let now = at(&t("11:30:00"));
    let recs = records(vec![
        released(1, OTHER, None, &t("11:00:00")),
        ready_until(2, OTHER, &t("11:10:00")),
        released(3, PROJECT, None, &t("09:10:00")),
    ]);
    assert_eq!(
        last_lease_end(&recs, PROJECT, now),
        Some(at(&t("09:10:00")))
    );
    assert_eq!(last_lease_end(&recs, "nobody", now), None);
}

// ---- The horizon ----

#[test]
fn the_horizon_is_the_latest_expiry_of_the_live_ready_leases() {
    let recs = records(vec![
        ready_until(1, PROJECT, &t("10:00:00")),
        ready_until(2, PROJECT, &t("10:40:00")),
        ready_until(3, PROJECT, &t("10:20:00")),
    ]);
    assert_eq!(horizon(&recs, &t("09:45:00")), at(&t("10:40:00")));
}

#[test]
fn a_released_leases_expiry_adds_nothing_to_the_horizon() {
    // Example B's 11:00 does not count, so A's 10:00 is the horizon at 09:45.
    let recs = records(vec![
        ready_until(1, PROJECT, &t("10:00:00")),
        released(2, PROJECT, Some(&t("11:00:00")), &t("09:30:00")),
    ]);
    assert_eq!(horizon(&recs, &t("09:45:00")), at(&t("10:00:00")));
}

#[test]
fn an_expired_leases_expiry_adds_nothing_to_the_horizon_while_another_is_live() {
    let recs = records(vec![
        ready_until(1, PROJECT, &t("09:50:00")),
        ready_until(2, PROJECT, &t("09:40:00")),
    ]);
    assert_eq!(horizon(&recs, &t("09:45:00")), at(&t("09:50:00")));
}

#[test]
fn only_pending_leases_make_the_reading_time_the_horizon() {
    // Example F at 09:50 and at 10:10: the horizon moves with the reading.
    let recs = records(vec![
        pending_lease(1, PROJECT),
        released(2, PROJECT, None, &t("09:10:00")),
    ]);
    assert_eq!(horizon(&recs, &t("09:50:00")), at(&t("09:50:00")));
    assert_eq!(horizon(&recs, &t("10:10:00")), at(&t("10:10:00")));
}

#[test]
fn several_pending_leases_also_make_the_reading_time_the_horizon() {
    let recs = records(vec![pending_lease(1, PROJECT), pending_lease(2, PROJECT)]);
    assert_eq!(horizon(&recs, &t("09:50:00")), at(&t("09:50:00")));
}

#[test]
fn a_live_ready_lease_beside_a_pending_one_gives_its_expiry() {
    let recs = records(vec![
        pending_lease(1, PROJECT),
        ready_until(2, PROJECT, &t("10:30:00")),
    ]);
    assert_eq!(horizon(&recs, &t("09:45:00")), at(&t("10:30:00")));
}

#[test]
fn with_no_live_lease_the_horizon_is_the_last_lease_end() {
    // Example at 10:05 and 11:30: A, B, C, D and E, and the last lease end is A's 10:00.
    let recs = records(vec![
        ready_until(1, PROJECT, &t("10:00:00")),
        released(2, PROJECT, Some(&t("11:00:00")), &t("09:30:00")),
        ready_until(3, PROJECT, &t("08:00:00")),
        unsat(4, PROJECT),
        released(5, PROJECT, Some(&t("09:20:00")), &t("09:35:00")),
    ]);
    assert_eq!(horizon(&recs, &t("10:05:00")), at(&t("10:00:00")));
    assert_eq!(horizon(&recs, &t("11:30:00")), at(&t("10:00:00")));
}

#[test]
fn the_first_example_at_each_of_its_readings() {
    let recs = records(vec![
        ready_until(1, PROJECT, &t("10:00:00")),
        released(2, PROJECT, Some(&t("11:00:00")), &t("09:30:00")),
        ready_until(3, PROJECT, &t("08:00:00")),
        unsat(4, PROJECT),
        released(5, PROJECT, Some(&t("09:20:00")), &t("09:35:00")),
    ]);
    let now = at(&t("09:45:00"));
    assert_eq!(live_count(&recs, PROJECT, now), 1);
    assert_eq!(
        last_lease_end(&recs, PROJECT, now),
        Some(at(&t("09:30:00")))
    );
    assert_eq!(horizon(&recs, &t("09:45:00")), at(&t("10:00:00")));
    for hms in ["10:05:00", "11:30:00"] {
        let now = at(&t(hms));
        assert_eq!(live_count(&recs, PROJECT, now), 0, "At {hms}");
        assert_eq!(
            last_lease_end(&recs, PROJECT, now),
            Some(at(&t("10:00:00"))),
            "At {hms}"
        );
    }
}

#[test]
fn the_horizon_of_released_leases_alone_is_the_latest_end() {
    let recs = records(vec![
        released(1, PROJECT, None, &t("09:10:00")),
        released(2, PROJECT, Some(&t("12:00:00")), &t("09:50:00")),
    ]);
    assert_eq!(horizon(&recs, &t("11:00:00")), at(&t("09:50:00")));
}

#[test]
fn with_no_lease_at_all_the_horizon_is_the_launch_time() {
    assert_eq!(horizon(&[], &t("09:45:00")), at(LAUNCH));
}

#[test]
fn with_only_unsatisfiable_leases_the_horizon_is_the_launch_time() {
    let recs = records(vec![unsat(1, PROJECT)]);
    assert_eq!(horizon(&recs, &t("09:45:00")), at(LAUNCH));
}

#[test]
fn with_only_another_projects_leases_the_horizon_is_the_launch_time() {
    let recs = records(vec![
        ready_until(1, OTHER, &t("10:00:00")),
        pending_lease(2, OTHER),
        released(3, OTHER, None, &t("09:00:00")),
    ]);
    assert_eq!(horizon(&recs, &t("09:45:00")), at(LAUNCH));
}

#[test]
fn another_projects_live_lease_does_not_give_the_horizon() {
    let recs = records(vec![
        ready_until(1, OTHER, &t("12:00:00")),
        released(2, PROJECT, None, &t("09:10:00")),
    ]);
    assert_eq!(horizon(&recs, &t("09:45:00")), at(&t("09:10:00")));
}

#[test]
fn the_horizon_is_not_raised_to_the_launch_time_when_a_lease_gives_one() {
    // The guard sends the later of the horizon and the launch time (P12); the horizon is not it.
    let late_launch = at("2026-11-19T12:00:00Z");
    let now = at(&t("09:45:00"));

    let live = records(vec![ready_until(1, PROJECT, &t("10:00:00"))]);
    assert_eq!(
        lease_horizon(&live, PROJECT, now, late_launch),
        at(&t("10:00:00")),
        "A live lease's expiry stays below a later launch time"
    );

    let pending = records(vec![pending_lease(1, PROJECT)]);
    assert_eq!(
        lease_horizon(&pending, PROJECT, now, late_launch),
        now,
        "A pending lease's horizon is the reading's time, below a later launch time"
    );

    let ended = records(vec![released(1, PROJECT, None, &t("09:10:00"))]);
    assert_eq!(
        lease_horizon(&ended, PROJECT, at(&t("10:05:00")), late_launch),
        at(&t("09:10:00")),
        "The last lease end stays below a later launch time"
    );
}

#[test]
fn the_launch_time_is_given_only_when_nothing_else_is() {
    let early_launch = at("2026-11-19T06:00:00Z");
    let now = at(&t("09:45:00"));
    assert_eq!(lease_horizon(&[], PROJECT, now, early_launch), early_launch);
    let ended = records(vec![released(1, PROJECT, None, &t("09:10:00"))]);
    assert_eq!(
        lease_horizon(&ended, PROJECT, now, early_launch),
        at(&t("09:10:00"))
    );
}

#[test]
fn a_lease_expiring_at_the_reading_second_is_no_longer_live_for_the_horizon() {
    // Live at 09:59:59, so the horizon is its expiry; at 10:00:00 it has ended, at the same time.
    let recs = records(vec![ready_until(1, PROJECT, &t("10:00:00"))]);
    assert_eq!(horizon(&recs, &t("09:59:59")), at(&t("10:00:00")));
    assert_eq!(horizon(&recs, &t("10:00:00")), at(&t("10:00:00")));
    assert_eq!(live_count(&recs, PROJECT, at(&t("10:00:00"))), 0);
}

// ---- Times that do not parse: no panic, and a host is kept ----

#[test]
fn a_ready_lease_whose_expiry_does_not_parse_is_live() {
    let now = at(&t("09:45:00"));
    for bad in [
        "",
        "tomorrow",
        "2026-13-45T99:99:99Z",
        "2026-11-19 10:00:00",
    ] {
        let mut r = one(ready_until(1, PROJECT, &t("10:00:00")));
        r.expires_at = Some(bad.to_string());
        assert!(is_live(&r, now), "Expiry {bad:?}");
        assert_eq!(lease_end(&r, now), None, "Expiry {bad:?}");
        assert_eq!(live_count(std::slice::from_ref(&r), PROJECT, now), 1);
    }
}

#[test]
fn a_ready_lease_with_no_expiry_is_live() {
    let mut r = one(ready_until(1, PROJECT, &t("10:00:00")));
    r.expires_at = None;
    let now = at(&t("09:45:00"));
    assert!(is_live(&r, now));
    assert_eq!(lease_end(&r, now), None);
    assert_eq!(live_count(std::slice::from_ref(&r), PROJECT, now), 1);
}

#[test]
fn a_live_lease_with_an_expiry_that_does_not_parse_never_gives_a_horizon_before_the_reading() {
    // The unparseable time is ignored, so no live lease has an expires_at to use, which is the
    // pending case: the reading's time. It MUST NOT panic, and the host is kept.
    let mut r = one(ready_until(1, PROJECT, &t("10:00:00")));
    r.expires_at = Some("tomorrow".to_string());
    let now = at(&t("09:45:00"));
    let h = lease_horizon(std::slice::from_ref(&r), PROJECT, now, at(LAUNCH));
    assert!(
        h >= now,
        "A live lease must not give a horizon before the reading"
    );
}

#[test]
fn a_good_expiry_is_used_beside_one_that_does_not_parse() {
    let mut bad = one(ready_until(1, PROJECT, &t("10:00:00")));
    bad.expires_at = Some("tomorrow".to_string());
    let good = one(ready_until(2, PROJECT, &t("10:30:00")));
    let recs = vec![bad, good];
    assert_eq!(horizon(&recs, &t("09:45:00")), at(&t("10:30:00")));
    assert_eq!(live_count(&recs, PROJECT, at(&t("09:45:00"))), 2);
}

#[test]
fn an_end_that_does_not_parse_is_ignored() {
    // The bad record's expiry is earlier than the good end, so ignoring its release time, or
    // falling back to its expiry, both leave the good record's end as the latest.
    let mut bad = one(released(1, PROJECT, Some(&t("08:00:00")), &t("07:30:00")));
    bad.released_at = Some("never".to_string());
    let good = one(released(2, PROJECT, None, &t("09:10:00")));
    let recs = vec![bad, good];
    assert_eq!(
        last_lease_end(&recs, PROJECT, at(&t("11:00:00"))),
        Some(at(&t("09:10:00")))
    );
}

#[test]
fn a_released_lease_whose_times_do_not_parse_does_not_panic_and_is_not_live() {
    let mut r = one(released(1, PROJECT, Some(&t("08:00:00")), &t("07:30:00")));
    r.released_at = Some("never".to_string());
    r.expires_at = Some("soon".to_string());
    let now = at(&t("11:00:00"));
    assert!(!is_live(&r, now));
    assert_eq!(lease_end(&r, now), None);
    assert_eq!(last_lease_end(std::slice::from_ref(&r), PROJECT, now), None);
    assert_eq!(
        lease_horizon(std::slice::from_ref(&r), PROJECT, now, at(LAUNCH)),
        at(LAUNCH)
    );
}

// ---- They read only their arguments ----

#[test]
fn the_functions_do_not_change_the_records_they_read() {
    let recs = records(vec![
        pending_lease(1, PROJECT),
        ready_until(2, PROJECT, &t("10:00:00")),
        released(3, PROJECT, Some(&t("11:00:00")), &t("09:30:00")),
        unsat(4, PROJECT),
    ]);
    let before = recs.clone();
    let now = at(&t("10:30:00"));
    for r in &recs {
        let _ = (is_live(r, now), lease_end(r, now));
    }
    let _ = (
        live_count(&recs, PROJECT, now),
        last_lease_end(&recs, PROJECT, now),
        lease_horizon(&recs, PROJECT, now, at(LAUNCH)),
    );
    assert_eq!(recs, before);
}

#[test]
fn the_fixture_times_are_the_contracts_strings() {
    // Guards the test's own fixtures: a time the tests write is one the contract writes back.
    let r = one(ready_until(1, PROJECT, &t("10:00:00")));
    assert_eq!(r.expires_at.as_deref(), Some("2026-11-19T10:00:00Z"));
    assert_eq!(
        format_time(at("2026-11-19T10:00:00Z")),
        "2026-11-19T10:00:00Z"
    );
}

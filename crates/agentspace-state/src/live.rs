//! Live leases and the lease horizon (docs/state.md, "Live leases and the lease horizon"; D56,
//! D57, D63). RFC 2119 keywords in this module's comments, and the capitals are load-bearing.
//!
//! Each function is a function of the lease state file's records and of one reading's time, which
//! a program takes once for a run, in Unix seconds. They read only their arguments, write
//! nothing, read no file and never panic. A program MUST NOT call them for a state file that is
//! not valid, cannot be read or is missing, and MUST NOT take that for "no live lease".
//!
//! A time string that does not parse (a `Record` built by hand, since `parse` refuses such a file)
//! errs toward keeping a host: a stored `ready` record whose `expires_at` does not parse is live,
//! and an end that does not parse is ignored.

use crate::{Record, parse_time};

/// The instant a time string names, or `None` when it is absent or names no instant.
fn instant(t: Option<&str>) -> Option<i64> {
    t.and_then(parse_time)
}

/// Whether a lease is live as of `now`: stored `pending`, or stored `ready` while `now` is before
/// its `expires_at`, to the second. A stored `ready` record at or after its `expires_at` is
/// `expired` and so not live. `released` and `unsatisfiable` are not live, and neither is a
/// stored state that is none of the four. A stored `ready` record with no `expires_at`, or one
/// that does not parse, is live.
pub fn is_live(lease: &Record, now: i64) -> bool {
    match lease.state.as_str() {
        "pending" => true,
        "ready" => match lease.expires_at.as_deref() {
            None => true,
            Some(t) => parse_time(t).is_none_or(|at| now < at),
        },
        _ => false,
    }
}

/// The instant a lease stopped being live as of `now`, or `None` for a live lease and for an
/// `unsatisfiable` one, which never became `ready`. A stored `ready` record at or after its
/// `expires_at` ends at its `expires_at`. A `released` lease ends at the earlier of its
/// `released_at` and its `expires_at`, so a lease released after it expired ends when it
/// expired; a `released` lease with no `expires_at` (never `ready`) ends at its `released_at`. A
/// time that does not parse is ignored, so an end is `None` when none of the times that decide it
/// parses.
pub fn lease_end(lease: &Record, now: i64) -> Option<i64> {
    match lease.state.as_str() {
        "ready" => {
            let at = instant(lease.expires_at.as_deref())?;
            (now >= at).then_some(at)
        }
        "released" => {
            let released = instant(lease.released_at.as_deref());
            let expires = instant(lease.expires_at.as_deref());
            match (released, expires) {
                (Some(r), Some(e)) => Some(r.min(e)),
                (r, e) => r.or(e),
            }
        }
        _ => None,
    }
}

/// How many of a project's leases are live as of `now`: those whose `project` equals `project`
/// exactly. Another project's leases are not counted.
pub fn live_count(leases: &[Record], project: &str, now: i64) -> usize {
    leases
        .iter()
        .filter(|l| l.project == project && is_live(l, now))
        .count()
}

/// The project's last lease end: the latest `lease_end` among the project's leases, or `None`
/// when none of them has one. Another project's leases are ignored.
pub fn last_lease_end(leases: &[Record], project: &str, now: i64) -> Option<i64> {
    leases
        .iter()
        .filter(|l| l.project == project)
        .filter_map(|l| lease_end(l, now))
        .max()
}

/// The project's horizon as of `now`: the latest `expires_at` among its live leases that have one;
/// with live leases but none that has one (only `pending` leases), `now`; with no live lease, its
/// last lease end; with none of these, `launch_time`, which the caller takes from the host and
/// passes in. It does NOT take the later of this and the launch time: that is the guard's send,
/// not the horizon. A released lease's own `expires_at` adds nothing: only a live lease's does.
/// Another project's leases are ignored.
pub fn lease_horizon(leases: &[Record], project: &str, now: i64, launch_time: i64) -> i64 {
    let mut any_live = false;
    let mut latest: Option<i64> = None;
    for lease in leases
        .iter()
        .filter(|l| l.project == project && is_live(l, now))
    {
        any_live = true;
        if let Some(at) = instant(lease.expires_at.as_deref()) {
            latest = Some(latest.map_or(at, |l| l.max(at)));
        }
    }
    if any_live {
        return latest.unwrap_or(now);
    }
    last_lease_end(leases, project, now).unwrap_or(launch_time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format_time;

    /// An arbitrary reading's time: 2026-11-22T09:45:00Z.
    const NOW: i64 = 1_795_340_700;
    const LAUNCH: i64 = NOW - 100_000;

    fn t(offset: i64) -> String {
        format_time(NOW + offset)
    }

    fn rec(project: &str, state: &str, expires: Option<i64>, released: Option<i64>) -> Record {
        Record {
            sized_for: None,
            plan: None,
            plan_source: None,
            id: "00000000-0000-4000-8000-000000000000".into(),
            project: project.into(),
            state: state.into(),
            platform: "linux-arm64".into(),
            profile: "default".into(),
            size: "small".into(),
            ttl_seconds: 3600,
            created_at: t(-7200),
            expires_at: expires.map(t),
            released_at: released.map(t),
            ssh: None,
            reasons: None,
            driver: None,
        }
    }

    // The table of state.md, row by row.

    #[test]
    fn stored_pending_is_live_with_no_end() {
        let l = rec("p", "pending", None, None);
        assert!(is_live(&l, NOW));
        assert_eq!(lease_end(&l, NOW), None);
    }

    #[test]
    fn stored_ready_before_expiry_is_live_with_no_end() {
        let l = rec("p", "ready", Some(600), None);
        assert!(is_live(&l, NOW));
        assert_eq!(lease_end(&l, NOW), None);
    }

    #[test]
    fn stored_ready_after_expiry_is_not_live_and_ends_at_expiry() {
        let l = rec("p", "ready", Some(-600), None);
        assert!(!is_live(&l, NOW));
        assert_eq!(lease_end(&l, NOW), Some(NOW - 600));
    }

    #[test]
    fn released_with_expiry_ends_at_the_earlier_of_the_two() {
        // Released before it would have expired.
        let early = rec("p", "released", Some(3600), Some(-900));
        assert!(!is_live(&early, NOW));
        assert_eq!(lease_end(&early, NOW), Some(NOW - 900));
        // Released after it expired: the end is its expires_at.
        let late = rec("p", "released", Some(-1500), Some(-1200));
        assert!(!is_live(&late, NOW));
        assert_eq!(lease_end(&late, NOW), Some(NOW - 1500));
    }

    #[test]
    fn released_with_no_expiry_ends_at_released_at() {
        let l = rec("p", "released", None, Some(-2100));
        assert!(!is_live(&l, NOW));
        assert_eq!(lease_end(&l, NOW), Some(NOW - 2100));
    }

    #[test]
    fn unsatisfiable_is_not_live_with_no_end() {
        let l = rec("p", "unsatisfiable", None, None);
        assert!(!is_live(&l, NOW));
        assert_eq!(lease_end(&l, NOW), None);
    }

    #[test]
    fn a_stored_state_that_is_none_of_the_four_is_not_live() {
        for state in ["expired", "", "Ready", "bogus"] {
            let l = rec("p", state, Some(600), Some(-60));
            assert!(!is_live(&l, NOW), "{state}");
            assert_eq!(lease_end(&l, NOW), None, "{state}");
        }
    }

    // The boundary, to the second.

    #[test]
    fn ready_is_live_one_second_before_expiry_and_not_at_it() {
        let l = rec("p", "ready", Some(0), None);
        assert!(is_live(&l, NOW - 1));
        assert_eq!(lease_end(&l, NOW - 1), None);
        assert!(!is_live(&l, NOW));
        assert_eq!(lease_end(&l, NOW), Some(NOW));
        assert!(!is_live(&l, NOW + 1));
        assert_eq!(lease_end(&l, NOW + 1), Some(NOW));
    }

    #[test]
    fn the_horizon_moves_from_expiry_to_the_end_at_the_boundary() {
        let leases = [rec("p", "ready", Some(0), None)];
        assert_eq!(lease_horizon(&leases, "p", NOW - 1, LAUNCH), NOW);
        assert_eq!(live_count(&leases, "p", NOW - 1), 1);
        assert_eq!(live_count(&leases, "p", NOW), 0);
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW);
        assert_eq!(last_lease_end(&leases, "p", NOW - 1), None);
        assert_eq!(last_lease_end(&leases, "p", NOW), Some(NOW));
    }

    // Counting, ends and the horizon.

    #[test]
    fn live_count_counts_the_projects_live_leases() {
        let leases = [
            rec("p", "pending", None, None),
            rec("p", "ready", Some(600), None),
            rec("p", "ready", Some(-600), None),
            rec("p", "released", Some(600), Some(-60)),
            rec("p", "unsatisfiable", None, None),
        ];
        assert_eq!(live_count(&leases, "p", NOW), 2);
        assert_eq!(live_count(&leases, "none", NOW), 0);
        assert_eq!(live_count(&[], "p", NOW), 0);
    }

    #[test]
    fn a_project_is_matched_exactly() {
        let leases = [rec("swimcoach", "pending", None, None)];
        assert_eq!(live_count(&leases, "swimcoach", NOW), 1);
        assert_eq!(live_count(&leases, "Swimcoach", NOW), 0);
        assert_eq!(live_count(&leases, "swim", NOW), 0);
        assert_eq!(live_count(&leases, "swimcoach ", NOW), 0);
        assert_eq!(live_count(&leases, "", NOW), 0);
    }

    #[test]
    fn last_lease_end_is_the_latest_end() {
        let leases = [
            rec("p", "released", None, Some(-3000)),
            rec("p", "ready", Some(-600), None),
            rec("p", "released", Some(-1500), Some(-1200)),
            rec("p", "unsatisfiable", None, None),
            rec("p", "pending", None, None),
        ];
        assert_eq!(last_lease_end(&leases, "p", NOW), Some(NOW - 600));
    }

    #[test]
    fn last_lease_end_is_none_with_no_end() {
        let leases = [
            rec("p", "pending", None, None),
            rec("p", "ready", Some(600), None),
            rec("p", "unsatisfiable", None, None),
        ];
        assert_eq!(last_lease_end(&leases, "p", NOW), None);
        assert_eq!(last_lease_end(&[], "p", NOW), None);
    }

    #[test]
    fn the_horizon_is_the_latest_expiry_among_live_leases() {
        let leases = [
            rec("p", "ready", Some(600), None),
            rec("p", "ready", Some(1800), None),
            rec("p", "ready", Some(900), None),
        ];
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW + 1800);
    }

    #[test]
    fn a_released_leases_expiry_adds_nothing_to_the_horizon() {
        let leases = [
            rec("p", "ready", Some(600), None),
            rec("p", "released", Some(6000), Some(-1800)),
        ];
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW + 600);
    }

    #[test]
    fn a_ready_lease_sets_the_horizon_beside_a_pending_one() {
        let leases = [
            rec("p", "pending", None, None),
            rec("p", "ready", Some(1200), None),
        ];
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW + 1200);
    }

    #[test]
    fn a_pending_only_horizon_is_the_readings_time() {
        let leases = [rec("p", "pending", None, None)];
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW);
        assert_eq!(lease_horizon(&leases, "p", NOW + 20, LAUNCH), NOW + 20);
        // Another lease that ended earlier does not change it.
        let with_ended = [
            rec("p", "pending", None, None),
            rec("p", "released", None, Some(-3000)),
        ];
        assert_eq!(lease_horizon(&with_ended, "p", NOW, LAUNCH), NOW);
    }

    #[test]
    fn with_no_live_lease_the_horizon_is_the_last_lease_end() {
        let leases = [
            rec("p", "ready", Some(-600), None),
            rec("p", "released", Some(3600), Some(-1800)),
            rec("p", "unsatisfiable", None, None),
        ];
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW - 600);
    }

    #[test]
    fn with_no_lease_with_an_end_the_horizon_is_the_launch_time() {
        assert_eq!(lease_horizon(&[], "p", NOW, LAUNCH), LAUNCH);
        let leases = [rec("p", "unsatisfiable", None, None)];
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), LAUNCH);
    }

    #[test]
    fn the_horizon_is_not_the_later_of_itself_and_the_launch_time() {
        // A launch time after the last lease end does not raise the horizon.
        let leases = [rec("p", "released", None, Some(-3000))];
        let launch = NOW - 10;
        assert_eq!(lease_horizon(&leases, "p", NOW, launch), NOW - 3000);
        // And a launch time after a live lease's expiry does not either.
        let live = [rec("p", "ready", Some(60), None)];
        assert_eq!(lease_horizon(&live, "p", NOW, NOW + 5000), NOW + 60);
    }

    #[test]
    fn the_second_example_of_state_md() {
        // F pending, G released while pending 35 minutes before the reading.
        let leases = [
            rec("p", "pending", None, None),
            rec("p", "released", None, Some(-2100)),
        ];
        assert_eq!(live_count(&leases, "p", NOW), 1);
        assert_eq!(last_lease_end(&leases, "p", NOW), Some(NOW - 2100));
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW);
        assert_eq!(lease_horizon(&leases, "p", NOW + 1200, LAUNCH), NOW + 1200);
    }

    #[test]
    fn the_first_example_of_state_md() {
        // At 09:45 (NOW): A ready to 10:00, B released 09:30 (expiry 11:00), C ready, expired
        // at 08:00, D unsatisfiable, E released 09:35 with expiry 09:20.
        let leases = [
            rec("p", "ready", Some(900), None),
            rec("p", "released", Some(4500), Some(-900)),
            rec("p", "ready", Some(-6300), None),
            rec("p", "unsatisfiable", None, None),
            rec("p", "released", Some(-1500), Some(-600)),
        ];
        assert_eq!(live_count(&leases, "p", NOW), 1);
        assert_eq!(last_lease_end(&leases, "p", NOW), Some(NOW - 900));
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW + 900);
        // At 10:05 and at 11:30.
        for later in [1200, 6300] {
            assert_eq!(live_count(&leases, "p", NOW + later), 0);
            assert_eq!(last_lease_end(&leases, "p", NOW + later), Some(NOW + 900));
            assert_eq!(lease_horizon(&leases, "p", NOW + later, LAUNCH), NOW + 900);
        }
    }

    // Another project's leases.

    #[test]
    fn another_projects_leases_are_ignored_by_every_function() {
        let leases = [
            rec("other", "ready", Some(9000), None),
            rec("other", "pending", None, None),
            rec("other", "released", None, Some(-60)),
            rec("p", "released", None, Some(-3000)),
        ];
        assert_eq!(live_count(&leases, "p", NOW), 0);
        assert_eq!(last_lease_end(&leases, "p", NOW), Some(NOW - 3000));
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW - 3000);
        let only_other = [rec("other", "ready", Some(9000), None)];
        assert_eq!(live_count(&only_other, "p", NOW), 0);
        assert_eq!(last_lease_end(&only_other, "p", NOW), None);
        assert_eq!(lease_horizon(&only_other, "p", NOW, LAUNCH), LAUNCH);
    }

    // Times that do not parse.

    #[test]
    fn a_ready_lease_whose_expiry_does_not_parse_is_live_with_no_end() {
        let mut l = rec("p", "ready", Some(600), None);
        l.expires_at = Some("tomorrow".into());
        assert!(is_live(&l, NOW));
        assert_eq!(lease_end(&l, NOW), None);
        // It is live, but has no expires_at that parses: the horizon is the reading's time.
        let leases = [l];
        assert_eq!(live_count(&leases, "p", NOW), 1);
        assert_eq!(lease_horizon(&leases, "p", NOW, LAUNCH), NOW);
    }

    #[test]
    fn a_ready_lease_with_no_expiry_is_live() {
        let l = rec("p", "ready", None, None);
        assert!(is_live(&l, NOW));
        assert_eq!(lease_end(&l, NOW), None);
        assert_eq!(lease_horizon(&[l], "p", NOW, LAUNCH), NOW);
    }

    #[test]
    fn a_released_end_that_does_not_parse_is_ignored() {
        let mut both = rec("p", "released", Some(-600), Some(-300));
        both.released_at = Some("2026-13-45T99:99:99Z".into());
        assert_eq!(lease_end(&both, NOW), Some(NOW - 600));
        let mut neither = rec("p", "released", Some(-600), Some(-300));
        neither.released_at = Some("".into());
        neither.expires_at = Some("x".into());
        assert_eq!(lease_end(&neither, NOW), None);
        let none_stored = rec("p", "released", None, None);
        assert_eq!(lease_end(&none_stored, NOW), None);
        // An end that does not parse leaves the horizon to the launch time.
        assert_eq!(lease_horizon(&[neither], "p", NOW, LAUNCH), LAUNCH);
    }

    #[test]
    fn a_released_lease_with_only_an_unparseable_released_at_ends_at_its_expiry() {
        let mut l = rec("p", "released", Some(-600), None);
        l.released_at = Some("soon".into());
        assert_eq!(lease_end(&l, NOW), Some(NOW - 600));
    }

    #[test]
    fn a_time_that_does_not_parse_never_panics() {
        for bad in [
            "",
            "0",
            "2026-11-22 09:45:00",
            "2026-02-30T00:00:00Z",
            "\u{0}",
        ] {
            let mut l = rec("p", "ready", None, None);
            l.expires_at = Some(bad.into());
            l.released_at = Some(bad.into());
            for state in ["pending", "ready", "released", "unsatisfiable"] {
                l.state = state.into();
                let leases = [l.clone()];
                let _ = is_live(&l, NOW);
                let _ = lease_end(&l, NOW);
                let _ = live_count(&leases, "p", NOW);
                let _ = last_lease_end(&leases, "p", NOW);
                let _ = lease_horizon(&leases, "p", NOW, LAUNCH);
            }
        }
    }
}

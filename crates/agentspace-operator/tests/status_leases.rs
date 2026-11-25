//! `status -All`'s `Leases` and `Guard` lines, and what it says when the lease state file cannot be
//! read (docs/operator.md: status, status -All; Every host at once; Calls by command; state.md:
//! The lease state file, Live leases and the lease horizon). Every case runs the built program
//! against the rig's stand-ins, with a lease state file written into the throwaway home and its
//! times set relative to the real clock, since the program has no clock seam.
//!
//! The `Guard` line's time is in the laptop's zone, read when it is printed. A case makes the
//! expected text with `agentspace_zone::local_clock_time` for the instant it expects, and never
//! assumes a zone.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;

use rig::*;

// ---------------------------------------------------------------------------------------------
// The laptop's zone, read by the one formatter, and the 12-hour form
// ---------------------------------------------------------------------------------------------

/// `<h:mm><am|pm>` for the instant `unix`, in the laptop's zone, made by
/// `agentspace_zone::local_clock_time`, the one formatter of the guard's stop time (D63). The
/// 12-hour form, the rounding and the daylight saving rule are tested in `agentspace-zone`; a case
/// here shows that the line is made from the right instant.
fn clock_text(unix: i64) -> String {
    agentspace_zone::local_clock_time(unix).expect("the system gives the laptop's zone")
}

/// The next whole minute at or after `unix`: a time with seconds rounds up.
fn ceil_minute(unix: i64) -> i64 {
    (unix + 59).div_euclid(60) * 60
}

/// What the line prints for the stop time `stop`, when it is still to come.
fn shown(stop: i64) -> String {
    clock_text(ceil_minute(stop))
}

fn guard_line(time: &str) -> String {
    format!("Guard     unexplained; stops at {time}\n")
}

fn leases_line(n: usize) -> String {
    format!("Leases    {n} live\n")
}

/// The five lines of a host's report, for an instance in `state`.
fn five(id: &str, state: &str) -> String {
    let ssm = if state == "running" { "online" } else { "-" };
    format!(
        "Instance  {id}\nState     {state}\nType      m9g.xlarge\nDisk      8 GiB gp3 ({VOL})\nSSM       {ssm}\n"
    )
}

/// The `Host` line of a record: the box's stack is `agentbox`, every other host's is
/// `agentspace-host-<project>`.
fn host_line(project: &str, state: &str) -> String {
    let stack = if project == "swimcoach" {
        "agentbox".to_string()
    } else {
        format!("agentspace-host-{project}")
    };
    format!("Host      {project} ({stack}), {state}\n")
}

// ---------------------------------------------------------------------------------------------
// The rig
// ---------------------------------------------------------------------------------------------

const BOX: &str = "swimcoach";
const HOST: &str = "agentspace";
const MAKING: &str = "example";

fn leases_path(rig: &Rig) -> PathBuf {
    rig.state_dir().join("leases.json")
}

fn write_leases(rig: &Rig, records: &[String]) {
    fs::create_dir_all(rig.state_dir()).expect("the state folder");
    fs::write(leases_path(rig), leases_doc(records)).expect("the lease state file");
}

fn write_leases_bytes(rig: &Rig, bytes: &[u8]) {
    fs::create_dir_all(rig.state_dir()).expect("the state folder");
    fs::write(leases_path(rig), bytes).expect("the lease state file");
}

/// What `describe-instances` answers for the `n`th host of the run: an instance in `state`,
/// launched at the Unix instant `launch` (long ago when `None`).
fn instance_of(rig: &Rig, n: u32, state: &str, launch: Option<i64>) {
    let launch = launch.map_or_else(old_launch, iso_at);
    rig.ok_nth("ec2-describe-instances", n, &full_instance(state, &launch));
}

/// The box and `agentspace`'s host, both with an instance, and a third host being made.
fn three_hosts() -> Rig {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec(HOST, "ready", Some(OTHER_ID)),
        host_rec(MAKING, "creating", None),
    ]);
    rig
}

/// The box (`stopped`, so it never gets a `Guard` line) and `agentspace`'s host, running since
/// `launch`.
fn box_and_a_running_host(launch: i64) -> Rig {
    let rig = Rig::standard();
    rig.write_hosts(&[box_rec(Some(ID)), host_rec(HOST, "ready", Some(OTHER_ID))]);
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "running", Some(launch));
    rig
}

/// The whole report of `box_and_a_running_host`, with `host`'s `Leases` and `Guard` lines.
fn report_of_box_and_host(n_box: usize, n_host: usize, guard: Option<&str>) -> String {
    format!(
        "{}{}{}{}{}{}{}",
        host_line(BOX, "ready"),
        five(ID, "stopped"),
        leases_line(n_box),
        host_line(HOST, "ready"),
        five(OTHER_ID, "running"),
        leases_line(n_host),
        guard.map_or_else(String::new, guard_line)
    )
}

// ---------------------------------------------------------------------------------------------
// The spec's example
// ---------------------------------------------------------------------------------------------

#[test]
fn the_report_of_the_box_under_a_lease_a_host_with_none_and_a_host_being_made() {
    // operator.md, status -All's example: the box under a live lease, agentspace's host running
    // with none, and a host still being made under a pending lease.
    let now = now_unix();
    let rig = three_hosts();
    instance_of(&rig, 1, "running", None);
    instance_of(&rig, 2, "running", Some(now - 300));
    write_leases(
        &rig,
        &[
            ready_lease(1, BOX),
            pending_lease(2, MAKING),
            lease_rec(3, HOST, "released", Some(now - 7200), Some(now - 7300)),
        ],
    );
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "", "status prints to stderr alone");
    assert_eq!(
        out.err(),
        format!(
            "{}{}{}{}{}{}{}{}{}{}",
            host_line(BOX, "ready"),
            five(ID, "running"),
            leases_line(1),
            host_line(HOST, "ready"),
            five(OTHER_ID, "running"),
            leases_line(0),
            guard_line(&shown(now - 300 + 900)),
            host_line(MAKING, "creating"),
            "Instance  none recorded\n",
            leases_line(1),
        )
    );
}

#[test]
fn the_labels_are_padded_to_ten_characters() {
    let now = now_unix();
    let rig = box_and_a_running_host(now - 300);
    write_leases(&rig, &[]);
    let out = rig.run(&["status", "-All"]);
    let err = out.err();
    assert!(
        err.contains("\nLeases    0 live\n"),
        "Leases and four spaces: {err:?}"
    );
    assert!(
        err.contains("\nGuard     unexplained; stops at "),
        "Guard and five spaces: {err:?}"
    );
    // Each label fills 10 characters, as the other lines' do.
    for line in err.lines() {
        let (label, rest) = line.split_at(10.min(line.len()));
        if label.starts_with("Leases") || label.starts_with("Guard") {
            assert!(label.ends_with(' '), "{line:?}");
            assert!(!rest.starts_with(' '), "{line:?}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The Leases line
// ---------------------------------------------------------------------------------------------

/// Runs `status -All` over the box (stopped) and `agentspace`'s host (stopped, so no `Guard`
/// line), with `records` in the lease state file, and returns the two counts it printed.
fn counts_for(records: &[String]) -> (usize, usize) {
    let rig = Rig::standard();
    rig.write_hosts(&[box_rec(Some(ID)), host_rec(HOST, "ready", Some(OTHER_ID))]);
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "stopped", None);
    write_leases(&rig, records);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let err = out.err();
    assert!(!err.contains("Guard"), "no host is running: {err:?}");
    let counts: Vec<usize> = err
        .lines()
        .filter_map(|l| l.strip_prefix("Leases    "))
        .map(|rest| {
            rest.strip_suffix(" live")
                .unwrap_or_else(|| panic!("{rest:?}"))
                .parse()
                .expect("a count in decimal")
        })
        .collect();
    assert_eq!(counts.len(), 2, "one line per host: {err:?}");
    assert_eq!(
        err,
        format!(
            "{}{}{}{}{}{}",
            host_line(BOX, "ready"),
            five(ID, "stopped"),
            leases_line(counts[0]),
            host_line(HOST, "ready"),
            five(OTHER_ID, "stopped"),
            leases_line(counts[1]),
        )
    );
    (counts[0], counts[1])
}

#[test]
fn zero_one_and_two_live_leases_are_counted_for_every_host() {
    let now = now_unix();
    assert_eq!(counts_for(&[]), (0, 0), "an empty file is no leases");
    assert_eq!(
        counts_for(&[pending_lease(1, HOST)]),
        (0, 1),
        "a pending lease is live"
    );
    assert_eq!(
        counts_for(&[ready_lease(1, HOST)]),
        (0, 1),
        "an unexpired ready lease is live"
    );
    assert_eq!(
        counts_for(&[ready_lease(1, HOST), pending_lease(2, HOST)]),
        (0, 2)
    );
    assert_eq!(
        counts_for(&[
            ready_lease(1, HOST),
            ready_lease(2, HOST),
            pending_lease(3, HOST),
            ready_lease(4, BOX),
        ]),
        (1, 3)
    );
    // Live until a minute from now: still live at the report.
    assert_eq!(
        counts_for(&[lease_rec(1, HOST, "ready", Some(now + 120), None)]),
        (0, 1)
    );
}

#[test]
fn an_expired_a_released_and_an_unsatisfiable_lease_are_not_counted() {
    let now = now_unix();
    let expired = lease_rec(1, HOST, "ready", Some(now - 3600), None);
    // A stored `ready` lease at or past its `expires_at` is `expired`, whatever the file says.
    assert_eq!(counts_for(std::slice::from_ref(&expired)), (0, 0));
    let released_after_use = lease_rec(2, HOST, "released", Some(now + 3600), Some(now - 60));
    assert_eq!(counts_for(&[released_after_use]), (0, 0));
    let released_from_pending = lease_rec(3, HOST, "released", None, Some(now - 60));
    assert_eq!(counts_for(&[released_from_pending]), (0, 0));
    let unsatisfiable = lease_rec(4, HOST, "unsatisfiable", None, None);
    assert_eq!(counts_for(&[unsatisfiable]), (0, 0));
    // Each beside one live lease: only the live one counts.
    assert_eq!(
        counts_for(&[
            expired,
            lease_rec(5, HOST, "released", Some(now + 3600), Some(now - 60)),
            lease_rec(6, HOST, "unsatisfiable", None, None),
            pending_lease(7, HOST),
        ]),
        (0, 1)
    );
}

#[test]
fn another_projects_leases_are_not_counted_and_the_project_is_compared_exactly() {
    // Live leases of projects whose names resemble the host's, and of an unrelated one.
    assert_eq!(
        counts_for(&[
            pending_lease(1, "agentspace-2"),
            pending_lease(2, "agentspace2"),
            pending_lease(3, "agentspac"),
            pending_lease(4, "xagentspace"),
            ready_lease(5, "other"),
        ]),
        (0, 0)
    );
    assert_eq!(
        counts_for(&[
            pending_lease(1, "agentspace-2"),
            pending_lease(2, HOST),
            ready_lease(3, "other"),
        ]),
        (0, 1)
    );
}

#[test]
fn the_box_counts_swimcoachs_leases_and_no_lease_of_its_stack_s_name() {
    // The box's record is project `swimcoach`, stack `agentbox`: its leases are swimcoach's.
    assert_eq!(
        counts_for(&[ready_lease(1, BOX), pending_lease(2, BOX)]),
        (2, 0)
    );
    // A lease of a project called `agentbox` is no lease of the box.
    assert_eq!(counts_for(&[pending_lease(1, "agentbox")]), (0, 0));
}

#[test]
fn a_host_being_made_shows_its_count_after_instance_none_recorded() {
    let rig = three_hosts();
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "stopped", None);
    write_leases(&rig, &[pending_lease(1, MAKING), pending_lease(2, MAKING)]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        format!(
            "{}{}{}{}{}{}{}{}{}",
            host_line(BOX, "ready"),
            five(ID, "stopped"),
            leases_line(0),
            host_line(HOST, "ready"),
            five(OTHER_ID, "stopped"),
            leases_line(0),
            host_line(MAKING, "creating"),
            "Instance  none recorded\n",
            leases_line(2),
        )
    );
    // The same host with no lease: a count of 0, still after the instance line, and no Guard.
    let rig = three_hosts();
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "stopped", None);
    write_leases(&rig, &[]);
    let out = rig.run(&["status", "-All"]);
    assert!(
        out.err()
            .ends_with("Instance  none recorded\nLeases    0 live\n"),
        "{:?}",
        out.err()
    );
}

#[test]
fn a_failed_host_prints_no_leases_line() {
    // operator.md: a host whose call failed prints its failure line after its Host line, and no
    // Leases or Guard line; the report goes on.
    let now = now_unix();
    let rig = box_and_a_running_host(now - 300);
    rig.fail_nth(
        "ec2-describe-instances",
        1,
        "An error occurred: boom\n",
        254,
    );
    write_leases(&rig, &[]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(1));
    assert_eq!(
        out.err(),
        format!(
            "{}agentspace: An error occurred: boom\n{}{}{}{}agentspace: 1 of 2 hosts failed.\n",
            host_line(BOX, "ready"),
            host_line(HOST, "ready"),
            five(OTHER_ID, "running"),
            leases_line(0),
            guard_line(&shown(now - 300 + 900)),
        )
    );
}

// ---------------------------------------------------------------------------------------------
// The Guard line: when it prints
// ---------------------------------------------------------------------------------------------

#[test]
fn a_running_host_a_live_lease_explains_gets_no_guard_line() {
    let now = now_unix();
    let ended = lease_rec(9, HOST, "released", Some(now - 900), Some(now - 950));
    for (what, records) in [
        ("a ready lease", vec![ready_lease(1, HOST)]),
        ("a pending lease", vec![pending_lease(1, HOST)]),
        (
            "a ready lease beside an ended one",
            vec![ended.clone(), ready_lease(1, HOST)],
        ),
        (
            "a pending lease beside an ended one",
            vec![pending_lease(1, HOST), ended],
        ),
    ] {
        let rig = box_and_a_running_host(now - 300);
        write_leases(&rig, &records);
        let out = rig.run(&["status", "-All"]);
        assert_eq!(out.code, Some(0), "{what}: {:?}", out.err());
        assert_eq!(out.err(), report_of_box_and_host(0, 1, None), "{what}");
    }
}

#[test]
fn a_host_that_is_not_running_gets_no_guard_line_whatever_its_leases() {
    // `pending`, `stopping`, `stopped`, `shutting-down` and `terminated` get none, and a host with
    // `Instance  none recorded` gets its count alone.
    let now = now_unix();
    for state in [
        "pending",
        "stopping",
        "stopped",
        "shutting-down",
        "terminated",
    ] {
        let rig = Rig::standard();
        rig.write_hosts(&[
            box_rec(Some(ID)),
            host_rec(HOST, "ready", Some(OTHER_ID)),
            host_rec(MAKING, "creating", None),
        ]);
        instance_of(&rig, 1, state, Some(now - 300));
        instance_of(&rig, 2, state, Some(now - 300));
        write_leases(&rig, &[]);
        let out = rig.run(&["status", "-All"]);
        assert_eq!(out.code, Some(0), "{state}: {:?}", out.err());
        assert_eq!(
            out.err(),
            format!(
                "{}{}{}{}{}{}{}{}{}",
                host_line(BOX, "ready"),
                five(ID, state),
                leases_line(0),
                host_line(HOST, "ready"),
                five(OTHER_ID, state),
                leases_line(0),
                host_line(MAKING, "creating"),
                "Instance  none recorded\n",
                leases_line(0),
            ),
            "{state}"
        );
    }
}

#[test]
fn only_a_running_host_with_no_live_lease_gets_the_guard_line_and_each_gets_its_own() {
    let now = now_unix();
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec("alpha", "ready", Some(OTHER_ID)),
        host_rec("beta", "ready", Some(THIRD_ID)),
        host_rec("gamma", "ready", Some("i-0123456789abcdef0")),
    ]);
    instance_of(&rig, 1, "running", Some(now - 400)); // the box, explained
    instance_of(&rig, 2, "running", Some(now - 300)); // alpha, unexplained
    instance_of(&rig, 3, "stopped", Some(now - 300)); // beta
    instance_of(&rig, 4, "running", Some(now - 200)); // gamma, unexplained, a later launch
    write_leases(&rig, &[ready_lease(1, BOX), pending_lease(2, "beta")]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        format!(
            "{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
            host_line(BOX, "ready"),
            five(ID, "running"),
            leases_line(1),
            host_line("alpha", "ready"),
            five(OTHER_ID, "running"),
            leases_line(0),
            guard_line(&shown(now - 300 + 900)),
            host_line("beta", "ready"),
            five(THIRD_ID, "stopped"),
            leases_line(1),
            host_line("gamma", "ready"),
            five("i-0123456789abcdef0", "running"),
            leases_line(0),
            guard_line(&shown(now - 200 + 900)),
        )
    );
}

// ---------------------------------------------------------------------------------------------
// The Guard line: its time
// ---------------------------------------------------------------------------------------------

/// Runs the report with `host`'s `Guard` line expected at the stop time `stop`: the host launched
/// at `launch`, the file holding `records`.
fn assert_stops_at(what: &str, launch: i64, records: &[String], stop: i64) {
    let rig = box_and_a_running_host(launch);
    write_leases(&rig, records);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{what}: {:?}", out.err());
    assert_eq!(
        out.err(),
        report_of_box_and_host(0, 0, Some(&shown(stop))),
        "{what}"
    );
}

#[test]
fn the_stop_time_is_the_launch_time_plus_15_minutes_for_a_project_with_no_lease_end() {
    let now = now_unix();
    assert_stops_at("an empty file", now - 300, &[], now - 300 + 900);
    // Leases of other projects add no end to this one.
    assert_stops_at(
        "another project's leases",
        now - 300,
        &[
            lease_rec(1, BOX, "released", Some(now - 100), Some(now - 100)),
            pending_lease(2, "other"),
        ],
        now - 300 + 900,
    );
    // An unsatisfiable lease has no end, so it adds none.
    assert_stops_at(
        "an unsatisfiable lease",
        now - 300,
        &[lease_rec(1, HOST, "unsatisfiable", None, None)],
        now - 300 + 900,
    );
}

#[test]
fn the_since_is_a_released_leases_released_at_when_that_is_the_later() {
    let now = now_unix();
    // Launched two hours ago; released two minutes ago, before its `expires_at`.
    assert_stops_at(
        "released before it expired",
        now - 7200,
        &[lease_rec(
            1,
            HOST,
            "released",
            Some(now + 3000),
            Some(now - 120),
        )],
        now - 120 + 900,
    );
    // A lease released while it was `pending`: its `released_at` is its end.
    assert_stops_at(
        "released from pending",
        now - 7200,
        &[lease_rec(1, HOST, "released", None, Some(now - 180))],
        now - 180 + 900,
    );
}

#[test]
fn the_since_is_an_expired_leases_expires_at_when_that_is_the_later() {
    let now = now_unix();
    assert_stops_at(
        "a stored ready lease past its expires_at",
        now - 7200,
        &[lease_rec(1, HOST, "ready", Some(now - 240), None)],
        now - 240 + 900,
    );
}

#[test]
fn a_lease_released_after_it_expired_ended_when_it_expired() {
    // P11: the end of a released lease is the earlier of `released_at` and `expires_at`.
    let now = now_unix();
    assert_stops_at(
        "released 140 seconds after it expired",
        now - 7200,
        &[lease_rec(
            1,
            HOST,
            "released",
            Some(now - 200),
            Some(now - 60),
        )],
        now - 200 + 900,
    );
}

#[test]
fn the_launch_time_is_the_since_when_it_is_later_than_the_last_lease_end() {
    let now = now_unix();
    assert_stops_at(
        "launched after the lease ended",
        now - 120,
        &[lease_rec(
            1,
            HOST,
            "released",
            Some(now - 3600),
            Some(now - 3700),
        )],
        now - 120 + 900,
    );
    assert_stops_at(
        "launched after an expired lease",
        now - 120,
        &[lease_rec(1, HOST, "ready", Some(now - 3600), None)],
        now - 120 + 900,
    );
    // An unsatisfiable lease adds no end, so a launch later than the one real end is the since.
    assert_stops_at(
        "an unsatisfiable lease beside an earlier end",
        now - 300,
        &[
            lease_rec(1, HOST, "released", Some(now - 3000), Some(now - 3100)),
            lease_rec(2, HOST, "unsatisfiable", None, None),
        ],
        now - 300 + 900,
    );
}

#[test]
fn the_last_lease_end_is_the_latest_among_the_projects_leases_that_are_not_live() {
    let now = now_unix();
    assert_stops_at(
        "two ended leases",
        now - 7200,
        &[
            lease_rec(1, HOST, "released", Some(now - 700), Some(now - 600)),
            lease_rec(2, HOST, "released", Some(now - 400), Some(now - 200)),
            lease_rec(3, HOST, "ready", Some(now - 5000), None),
        ],
        now - 400 + 900,
    );
    // The project is compared exactly: another project's later end is not this one's.
    assert_stops_at(
        "another project's later end",
        now - 7200,
        &[
            lease_rec(1, HOST, "released", Some(now - 700), Some(now - 600)),
            lease_rec(
                2,
                "agentspace-2",
                "released",
                Some(now - 100),
                Some(now - 50),
            ),
            lease_rec(3, BOX, "released", Some(now - 100), Some(now - 50)),
        ],
        // The own lease was released at now-600, after it expired at now-700: it ended at now-700.
        now - 700 + 900,
    );
}

#[test]
fn seconds_round_up_to_the_next_whole_minute() {
    let now = now_unix();
    // A launch on a minute boundary 5 to 6 minutes ago: its stop time falls on a minute boundary,
    // and a launch one or 59 seconds later rounds up to the next minute.
    let aligned = (now - 300).div_euclid(60) * 60;
    for (what, launch) in [
        ("on the minute", aligned),
        ("a second after", aligned + 1),
        ("59 seconds after", aligned + 59),
        ("a second before", aligned - 1),
    ] {
        let rig = box_and_a_running_host(launch);
        write_leases(&rig, &[]);
        let out = rig.run(&["status", "-All"]);
        assert_eq!(out.code, Some(0), "{what}: {:?}", out.err());
        let want = clock_text(ceil_minute(launch + 900));
        assert_eq!(
            out.err(),
            report_of_box_and_host(0, 0, Some(&want)),
            "{what}"
        );
    }
    // And the two ends of the rule differ by one minute on the clock.
    assert_ne!(
        clock_text(aligned + 900),
        clock_text(aligned + 900 + 60),
        "a minute apart prints as another minute"
    );
}

#[test]
fn a_stop_time_that_has_passed_shows_the_reports_time_never_a_time_in_the_past() {
    // The launch was two hours ago: the guard stops the host at its next run, so what prints is
    // the report's time, the next whole minute at or after the clock when the report was read
    // (the case accepts that minute or the next).
    let now = now_unix();
    for (what, launch, records) in [
        ("an old launch", now - 7200, vec![]),
        (
            "an old lease end",
            now - 90_000,
            vec![lease_rec(
                1,
                HOST,
                "released",
                Some(now - 8000),
                Some(now - 8100),
            )],
        ),
        ("exactly the grace ago", now - 900, vec![]),
        ("a launch of 2020", 1_577_836_800, vec![]),
    ] {
        let rig = box_and_a_running_host(launch);
        write_leases(&rig, &records);
        let before = now_unix();
        let out = rig.run(&["status", "-All"]);
        let after = now_unix();
        assert_eq!(out.code, Some(0), "{what}: {:?}", out.err());
        let mut candidates = Vec::new();
        let mut minute = ceil_minute(before);
        while minute <= ceil_minute(after) {
            candidates.push(report_of_box_and_host(0, 0, Some(&clock_text(minute))));
            minute += 60;
        }
        assert!(
            candidates.contains(&out.err()),
            "{what}: the report's minute, or the next: {:?}",
            out.err()
        );
    }
}

#[test]
fn the_printed_time_is_never_more_than_the_grace_after_the_report() {
    // A launch just now: the stop time is the grace after it, and the line prints it.
    let now = now_unix();
    let rig = box_and_a_running_host(now);
    write_leases(&rig, &[]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        report_of_box_and_host(0, 0, Some(&shown(now + 900)))
    );
}

#[test]
fn the_time_is_in_the_laptops_zone_and_agentspace_timezone_does_not_change_it() {
    // AGENTSPACE_TIMEZONE is the token push's alone (operator.md, status -All). The rig sets it to
    // America/New_York for every run; a different one changes nothing.
    let now = now_unix();
    let want = report_of_box_and_host(0, 0, Some(&shown(now - 300 + 900)));
    for zone in [
        "America/New_York",
        "Asia/Kolkata",
        "Pacific/Chatham",
        "not a zone",
    ] {
        let rig = box_and_a_running_host(now - 300);
        write_leases(&rig, &[]);
        let out = rig
            .op(&["status", "-All"])
            .env("AGENTSPACE_TIMEZONE", zone)
            .run();
        assert_eq!(out.code, Some(0), "{zone}: {:?}", out.err());
        assert_eq!(out.err(), want, "{zone}");
    }
    let rig = box_and_a_running_host(now - 300);
    write_leases(&rig, &[]);
    let out = rig
        .op(&["status", "-All"])
        .env_remove("AGENTSPACE_TIMEZONE")
        .run();
    assert_eq!(out.err(), want, "no override");
}

#[test]
fn the_guard_line_is_in_the_form_h_mm_with_am_or_pm() {
    let now = now_unix();
    let rig = box_and_a_running_host(now - 300);
    write_leases(&rig, &[]);
    let out = rig.run(&["status", "-All"]);
    let err = out.err();
    let line = err
        .lines()
        .find_map(|l| l.strip_prefix("Guard     unexplained; stops at "))
        .expect("a Guard line");
    let (digits, half) = line.split_at(line.len() - 2);
    assert!(half == "am" || half == "pm", "{line:?}");
    let (hour, minute) = digits.split_once(':').expect("h:mm");
    assert!(
        !hour.starts_with('0') && (1..=12).contains(&hour.parse::<u32>().unwrap()),
        "{line:?}"
    );
    assert_eq!(minute.len(), 2, "{line:?}");
    assert!(minute.parse::<u32>().unwrap() < 60, "{line:?}");
    assert!(!line.contains(' '), "no space before am or pm: {line:?}");
}

// ---------------------------------------------------------------------------------------------
// No lease ID, alias or other member is ever printed
// ---------------------------------------------------------------------------------------------

#[test]
fn no_lease_id_or_alias_is_ever_printed() {
    let now = now_unix();
    let rig = three_hosts();
    instance_of(&rig, 1, "running", None);
    instance_of(&rig, 2, "running", Some(now - 300));
    write_leases(
        &rig,
        &[
            ready_lease(1, BOX),
            ready_lease(2, BOX),
            pending_lease(3, MAKING),
            lease_rec(4, HOST, "released", Some(now - 200), Some(now - 100)),
            lease_rec(5, HOST, "unsatisfiable", None, None),
        ],
    );
    for quiet in [false, true] {
        let out = if quiet {
            rig.run(&["status", "-All", "-Quiet"])
        } else {
            rig.run(&["status", "-All"])
        };
        let err = out.err();
        assert_eq!(out.code, Some(0), "{err:?}");
        assert_eq!(out.out(), "");
        for marker in ["5eedc0de", "feed00", "-0000-4000-", "ubuntu", "driver"] {
            assert!(!err.contains(marker), "{marker}: {err:?}");
        }
    }
}

#[test]
fn no_lease_id_is_printed_in_the_unreadable_message_either() {
    let rig = box_and_a_running_host(now_unix() - 300);
    // Two records with one id: the message names the file and a problem, not the id.
    write_leases(&rig, &[ready_lease(1, HOST), ready_lease(1, HOST)]);
    let out = rig.run(&["status", "-All"]);
    let err = out.err();
    assert!(err.contains("cannot be read"), "{err:?}");
    assert!(!err.contains("5eedc0de"), "{err:?}");
    assert!(!err.contains("feed00"), "{err:?}");
}

// ---------------------------------------------------------------------------------------------
// A lease state file that cannot be read (P18, P24, P25, P27)
// ---------------------------------------------------------------------------------------------

/// The message for the lease state file of `rig`, up to its reason.
fn unreadable_prefix(path: &std::path::Path) -> String {
    format!(
        "agentspace: The lease state file {} cannot be read: ",
        path.display()
    )
}

/// The report of the three hosts with no `Leases` or `Guard` line.
fn bare_report() -> String {
    format!(
        "{}{}{}{}{}{}{}",
        host_line(BOX, "ready"),
        five(ID, "running"),
        host_line(HOST, "ready"),
        five(OTHER_ID, "running"),
        host_line(MAKING, "creating"),
        "Instance  none recorded\n",
        ""
    )
}

/// Asserts `out` is the bare report of `three_hosts`, then the one message, with the reason in
/// the one-line form, and returns the reason.
fn assert_unreadable(what: &str, out: &Out, rig: &Rig, quiet_exit: i32) -> String {
    assert_eq!(out.code, Some(quiet_exit), "{what}: {:?}", out.err());
    assert_eq!(out.out(), "", "{what}");
    let err = out.err();
    let bare = bare_report();
    let rest = err.strip_prefix(bare.as_str()).unwrap_or_else(|| {
        panic!("{what}: every host's five lines and no Leases or Guard line: {err:?}")
    });
    let prefix = unreadable_prefix(&leases_path(rig));
    let message = rest
        .strip_prefix(prefix.as_str())
        .unwrap_or_else(|| panic!("{what}: the message names the file's full path: {rest:?}"));
    let reason = message
        .strip_suffix(".\n")
        .unwrap_or_else(|| panic!("{what}: the message ends with a period: {message:?}"));
    assert!(
        !reason.is_empty() && !reason.contains('\n'),
        "{what}: {reason:?}"
    );
    assert!(!err.contains("Leases    "), "{what}");
    assert!(!err.contains("Guard     "), "{what}");
    assert_eq!(
        err.matches("cannot be read").count(),
        1,
        "{what}: said once"
    );
    reason.to_string()
}

fn three_running_hosts_rig() -> Rig {
    let rig = three_hosts();
    instance_of(&rig, 1, "running", None);
    instance_of(&rig, 2, "running", None);
    rig
}

#[test]
fn a_missing_lease_state_file_is_not_no_leases() {
    let rig = three_running_hosts_rig();
    let out = rig.run(&["status", "-All"]);
    let reason = assert_unreadable("missing", &out, &rig, 0);
    assert_eq!(reason, "the file does not exist");
    assert!(
        !leases_path(&rig).exists(),
        "status -All never writes the file, and never creates it"
    );
}

#[test]
fn a_file_that_is_not_valid_or_cannot_be_opened_is_not_no_leases_either() {
    let valid_ready = ready_lease(1, HOST);
    let duplicate = vec![valid_ready.clone(), valid_ready.clone()];
    let bad_time = lease_rec(2, HOST, "ready", Some(now_unix() + 3600), None)
        .replace(&iso_z(now_unix() + 3600)[..10], "2026-02-30");
    let no_state = valid_ready.replace("\"state\":\"ready\",", "");
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("not JSON", b"{ this is not json".to_vec()),
        ("an empty file", Vec::new()),
        ("not UTF-8", vec![0xff, 0xfe, 0xfd]),
        (
            "the wrong version",
            b"{\"version\":1,\"leases\":[]}\n".to_vec(),
        ),
        ("no leases member", b"{\"version\":0}\n".to_vec()),
        ("a record that fails the schema", leases_doc(&[no_state])),
        ("two records with one id", leases_doc(&duplicate)),
        ("a time that names no instant", leases_doc(&[bad_time])),
    ];
    for (what, bytes) in cases {
        let rig = three_running_hosts_rig();
        write_leases_bytes(&rig, &bytes);
        let before = fs::read(leases_path(&rig)).unwrap();
        let out = rig.run(&["status", "-All"]);
        assert_unreadable(what, &out, &rig, 0);
        assert_eq!(
            fs::read(leases_path(&rig)).unwrap(),
            before,
            "{what}: the file's bytes are unchanged"
        );
    }
}

#[test]
fn a_folder_at_the_path_is_not_a_file() {
    let rig = three_running_hosts_rig();
    fs::create_dir_all(leases_path(&rig)).unwrap();
    let out = rig.run(&["status", "-All"]);
    let reason = assert_unreadable("a folder", &out, &rig, 0);
    assert_ne!(
        reason, "the file does not exist",
        "a folder is not a missing file"
    );
    assert!(leases_path(&rig).is_dir());
}

#[test]
fn a_file_that_cannot_be_opened_is_not_no_leases() {
    let rig = three_running_hosts_rig();
    write_leases(&rig, &[ready_lease(1, HOST)]);
    // Held open with no sharing, so the program cannot open it.
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(leases_path(&rig))
        .expect("the case holds the file");
    let out = rig.run(&["status", "-All"]);
    drop(held);
    let reason = assert_unreadable("a file held open", &out, &rig, 0);
    assert_ne!(reason, "the file does not exist");
}

#[test]
fn the_lease_state_file_never_changes_the_exit_code() {
    // Exit 0 when every host's part passed, and 1 when one failed, whatever the file is.
    for setup in 0..3 {
        let rig = three_running_hosts_rig();
        match setup {
            0 => {}
            1 => write_leases_bytes(&rig, b"not json"),
            _ => fs::create_dir_all(leases_path(&rig)).unwrap(),
        }
        let out = rig.run(&["status", "-All"]);
        assert_eq!(out.code, Some(0), "setup {setup}: {:?}", out.err());

        let rig = three_running_hosts_rig();
        match setup {
            0 => {}
            1 => write_leases_bytes(&rig, b"not json"),
            _ => fs::create_dir_all(leases_path(&rig)).unwrap(),
        }
        rig.fail_nth(
            "ec2-describe-instances",
            2,
            "An error occurred: boom\n",
            254,
        );
        let out = rig.run(&["status", "-All"]);
        assert_eq!(out.code, Some(1), "setup {setup}: a failed host fails it");
        let err = out.err();
        assert!(
            err.ends_with("agentspace: 1 of 3 hosts failed.\n"),
            "setup {setup}: {err:?}"
        );
        // The message is not a host's failure, so it is not counted, and it comes before the count.
        let message_at = err.find("cannot be read").expect("the message");
        let last_host_at = err.rfind("Host      example").expect("the last host");
        let count_at = err.rfind("1 of 3 hosts failed").unwrap();
        assert!(
            last_host_at < message_at && message_at < count_at,
            "setup {setup}: {err:?}"
        );
    }
}

#[test]
fn the_message_prints_once_after_the_last_host_and_whatever_quiet_says() {
    // One rig for both runs: the message names the rig's file.
    let rig = three_running_hosts_rig();
    let loud = rig.run(&["status", "-All"]);
    let quiet = rig.run(&["status", "-All", "-Quiet"]);
    assert_eq!(quiet.code, Some(0));
    assert_eq!(
        quiet.err(),
        loud.err(),
        "-Quiet withholds none of status's report"
    );
    assert!(quiet.err().contains("cannot be read"));

    // With a valid file, the Leases and Guard lines print whatever -Quiet says.
    let now = now_unix();
    let rig = box_and_a_running_host(now - 300);
    write_leases(&rig, &[]);
    let loud = rig.run(&["status", "-All"]);
    let rig = box_and_a_running_host(now - 300);
    write_leases(&rig, &[]);
    let quiet = rig.run(&["status", "-All", "-Quiet"]);
    assert_eq!(quiet.err(), loud.err());
    assert_eq!(
        quiet.err(),
        report_of_box_and_host(0, 0, Some(&shown(now - 300 + 900)))
    );
}

#[test]
fn a_missing_file_still_shows_each_hosts_state() {
    // It never prints a count it could not read, and never guesses one, and it still shows each
    // host's State, which a turn's close reads (D51).
    let rig = three_hosts();
    instance_of(&rig, 1, "stopped", None);
    instance_of(&rig, 2, "stopped", None);
    let out = rig.run(&["status", "-All"]);
    let err = out.err();
    assert_eq!(out.code, Some(0));
    assert_eq!(err.matches("State     stopped\n").count(), 2, "{err:?}");
    assert!(!err.contains("Leases"), "{err:?}");
}

#[test]
fn an_expired_sign_in_ends_the_command_at_once_and_the_file_s_line_is_not_printed() {
    for missing in [true, false] {
        let rig = three_running_hosts_rig();
        if !missing {
            write_leases_bytes(&rig, b"not json");
        }
        rig.fail_nth(
            "ec2-describe-instances",
            2,
            "The SSO session has expired\n",
            255,
        );
        let out = rig.run(&["status", "-All"]);
        let err = out.err();
        assert_eq!(out.code, Some(1), "{err:?}");
        assert_eq!(err.lines().last(), Some(SIGN_IN), "{err:?}");
        assert!(
            !err.contains("cannot be read"),
            "missing {missing}: {err:?}"
        );
        assert!(!err.contains("hosts failed"), "{err:?}");
    }
}

#[test]
fn the_lease_state_file_is_read_without_its_lock_and_never_written() {
    let now = now_unix();
    // Without the lock held: the run creates no lock file, and writes nothing in the state folder.
    let rig = box_and_a_running_host(now - 300);
    write_leases(&rig, &[ready_lease(1, BOX)]);
    let bytes = fs::read(leases_path(&rig)).unwrap();
    let tree = rig.tree(&rig.state_dir());
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        fs::read(leases_path(&rig)).unwrap(),
        bytes,
        "the file's bytes"
    );
    assert_eq!(
        rig.tree(&rig.state_dir()),
        tree,
        "nothing created beside it"
    );
    assert!(rig.tmp_files().is_empty());

    // With the lease state lock held by the case, the run finishes at once and reports.
    let rig = box_and_a_running_host(now - 300);
    write_leases(&rig, &[ready_lease(1, BOX)]);
    let lock = rig.state_dir().join("leases.lock");
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(&lock)
        .expect("the case holds the lease state lock");
    // A run that took the lock would wait its 30 seconds for it and fail with the lock's message,
    // so the bound is the lock's wait, not how fast a loaded machine runs the operator (#0189).
    let out = rig.op(&["status", "-All"]).run();
    drop(held);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        report_of_box_and_host(1, 0, Some(&shown(now - 300 + 900)))
    );
    assert!(
        out.elapsed < std::time::Duration::from_secs(30),
        "no wait on the lock: {:?}",
        out.elapsed
    );
}

// ---------------------------------------------------------------------------------------------
// Which folder it reads
// ---------------------------------------------------------------------------------------------

#[test]
fn agentspace_state_dir_moves_the_lease_state_file_and_nothing_else() {
    let now = now_unix();
    let rig = box_and_a_running_host(now - 300);
    let moved = rig.root().join("moved state");
    fs::create_dir_all(&moved).unwrap();
    fs::write(
        moved.join("leases.json"),
        leases_doc(&[ready_lease(1, HOST)]),
    )
    .unwrap();
    // The default place holds a file that is not valid: it is never read.
    write_leases_bytes(&rig, b"not json");
    let out = rig
        .op(&["status", "-All"])
        .env("AGENTSPACE_STATE_DIR", &moved)
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), report_of_box_and_host(0, 1, None));
    // The host registry stays under the home: it was read from there, and nothing was written to
    // the moved folder.
    assert_eq!(
        fs::read_dir(&moved).unwrap().count(),
        1,
        "nothing is created in the moved folder"
    );
}

#[test]
fn the_message_names_the_moved_file_s_full_path() {
    let rig = three_running_hosts_rig();
    let moved = rig.root().join("moved state");
    fs::create_dir_all(&moved).unwrap();
    let out = rig
        .op(&["status", "-All"])
        .env("AGENTSPACE_STATE_DIR", &moved)
        .run();
    let err = out.err();
    let want = format!(
        "agentspace: The lease state file {} cannot be read: the file does not exist.\n",
        moved.join("leases.json").display()
    );
    assert!(err.ends_with(&want), "{err:?}");
    assert_eq!(out.code, Some(0));
}

#[test]
fn an_empty_agentspace_state_dir_is_the_default_folder() {
    let now = now_unix();
    let rig = box_and_a_running_host(now - 300);
    write_leases(&rig, &[ready_lease(1, HOST)]);
    let out = rig
        .op(&["status", "-All"])
        .env("AGENTSPACE_STATE_DIR", "")
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), report_of_box_and_host(0, 1, None));
}

// ---------------------------------------------------------------------------------------------
// LaunchTime, and the AWS calls
// ---------------------------------------------------------------------------------------------

/// The instance answer of a running host with the given `LaunchTime` text, or none.
fn running_with_launch_text(launch: Option<&str>) -> String {
    instance_json(
        "running",
        launch,
        Some("m9g.xlarge"),
        Some("/dev/sda1"),
        &[("/dev/sda1", VOL)],
    )
}

#[test]
fn a_running_unexplained_host_with_no_launch_time_fails_its_part_alone() {
    let message = "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime.";
    for (what, answer) in [
        ("no LaunchTime", running_with_launch_text(None)),
        (
            "no UTC offset",
            running_with_launch_text(Some("2026-11-21T10:00:00")),
        ),
        ("not a time", running_with_launch_text(Some("yesterday"))),
    ] {
        let rig = box_and_a_running_host(now_unix() - 300);
        rig.ok_nth("ec2-describe-instances", 2, &answer);
        write_leases(&rig, &[]);
        let out = rig.run(&["status", "-All"]);
        assert_eq!(out.code, Some(1), "{what}: {:?}", out.err());
        assert_eq!(
            out.err(),
            format!(
                "{}{}{}{}{message}\nagentspace: 1 of 2 hosts failed.\n",
                host_line(BOX, "ready"),
                five(ID, "stopped"),
                leases_line(0),
                host_line(HOST, "ready"),
            ),
            "{what}: that host's part fails before its five lines, and the report goes on"
        );
    }
}

#[test]
fn the_launch_time_is_read_only_for_a_host_that_gets_a_guard_line() {
    let now = now_unix();
    let no_launch = running_with_launch_text(None);
    // Explained by a live lease: not read, so no failure.
    let rig = box_and_a_running_host(now - 300);
    rig.ok_nth("ec2-describe-instances", 2, &no_launch);
    write_leases(&rig, &[pending_lease(1, HOST)]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), report_of_box_and_host(0, 1, None));
    // Not running: not read.
    let rig = box_and_a_running_host(now - 300);
    rig.ok_nth(
        "ec2-describe-instances",
        2,
        &instance_json(
            "stopped",
            None,
            Some("m9g.xlarge"),
            Some("/dev/sda1"),
            &[("/dev/sda1", VOL)],
        ),
    );
    write_leases(&rig, &[]);
    let out = rig.run(&["status", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    // A file that is missing or not valid: never read either.
    for bytes in [None, Some(b"not json".to_vec())] {
        let rig = box_and_a_running_host(now - 300);
        rig.ok_nth("ec2-describe-instances", 2, &no_launch);
        if let Some(bytes) = bytes {
            write_leases_bytes(&rig, &bytes);
        }
        let out = rig.run(&["status", "-All"]);
        assert_eq!(out.code, Some(0), "{:?}", out.err());
        assert!(!out.err().contains("LaunchTime"), "{:?}", out.err());
    }
}

#[test]
fn status_all_makes_the_same_aws_calls_as_before_v0_08() {
    // One describe-instances, one describe-volumes and, for a running host, one ssm call per host
    // that has an instance; nothing else, whatever the lease state file holds.
    let expected = [
        "ec2-describe-instances",
        "ec2-describe-volumes",
        "ssm-describe-instance-information",
        "ec2-describe-instances",
        "ec2-describe-volumes",
        "ssm-describe-instance-information",
    ];
    let now = now_unix();
    for setup in 0..4 {
        let rig = box_and_a_running_host(now - 300);
        instance_of(&rig, 1, "running", None);
        match setup {
            0 => write_leases(&rig, &[ready_lease(1, BOX)]),
            1 => write_leases(&rig, &[]),
            2 => write_leases_bytes(&rig, b"not json"),
            _ => {}
        }
        let out = rig.run(&["status", "-All"]);
        assert_eq!(out.code, Some(0), "setup {setup}: {:?}", out.err());
        assert_eq!(rig.keys(), expected, "setup {setup}");
        let calls = rig.calls();
        for (index, id) in [(0, ID), (3, OTHER_ID)] {
            assert_eq!(
                calls[index].args,
                aws_args("ec2", "describe-instances", &["--instance-ids", id]),
                "setup {setup}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// status and status -Host print the five lines alone; down -All never reads the file
// ---------------------------------------------------------------------------------------------

#[test]
fn status_and_status_host_print_the_five_lines_alone_and_never_read_the_lease_state_file() {
    for setup in 0..3 {
        for args in [vec!["status"], vec!["status", "-Host", "swimcoach"]] {
            let rig = Rig::standard();
            rig.write_box_id(ID);
            // A running host that no lease explains, with no launch time: `status -All` would fail
            // it, and a lease state file that is not valid would be its message.
            rig.ok("ec2-describe-instances", &running_with_launch_text(None));
            match setup {
                0 => {}
                1 => write_leases_bytes(&rig, b"not json"),
                _ => write_leases(&rig, &[ready_lease(1, BOX)]),
            }
            let out = rig.run(&args);
            assert_eq!(
                out.code,
                Some(0),
                "setup {setup}, {args:?}: {:?}",
                out.err()
            );
            assert_eq!(
                out.err(),
                five(ID, "running"),
                "setup {setup}, {args:?}: the five lines alone"
            );
            assert_eq!(out.out(), "");
        }
    }
}

#[test]
fn status_makes_no_new_call_and_reads_no_launch_time_without_all() {
    let rig = Rig::standard();
    rig.write_box_id(ID);
    write_leases(&rig, &[]);
    let out = rig.run(&["status"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ssm-describe-instance-information"
        ]
    );
}

#[test]
fn down_all_never_reads_the_lease_state_file() {
    let rig = three_hosts();
    write_leases_bytes(&rig, b"not json");
    let out = rig.run(&["down", "-All"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert!(!out.err().contains("lease state file"), "{:?}", out.err());
    assert!(!out.err().contains("Leases"), "{:?}", out.err());
    assert!(!out.err().contains("Guard"), "{:?}", out.err());
}

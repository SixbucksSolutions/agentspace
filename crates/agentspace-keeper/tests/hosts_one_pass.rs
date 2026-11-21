//! One pass over several hosts (docs/keeper.md, Hosts, A pass, The hosts of a pass, Which host a
//! lease is on, Which lease, The holder's log, Conformance cases for a host per project;
//! E0.07-T03-S05, #0154).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Each test is named for the
//! behavior it shows, and `H<n>` names the conformance case of keeper.md it matches. A pass
//! groups the ready leases by the host each is on, takes a turn for each host, the box first and
//! then the others in the ordinal order of their projects, and in each turn keeps, stops or
//! starts the one holder of that host, on that host's record and that host's leases alone. A
//! failure in one host's turn never changes another's.
//!
//! The files are real, in a throwaway home: the state folder holds `leases.json`, each host's
//! record (`board-holder.json` for the box, `board-holder.<project>.json` for another host) and
//! the logs, and `<home>\.agentspace\hosts.json` is the host registry, which
//! `AGENTSPACE_STATE_DIR` never moves. The processes, the matcher, the starter, the locks and the
//! clock are fakes of the pinned traits (`tests/hosts_world/mod.rs`).

mod hosts_world;

use agentspace_keeper_api::Stopped;
use hosts_world::*;
use serde_json::Value;

fn sw(n: u32, expires: &str) -> Lease {
    Lease::new(BOX, n, expires)
}

fn lease(project: &str, n: u32) -> Lease {
    Lease::new(project, n, FAR)
}

fn record_json(w: &World, project: Option<&str>) -> Value {
    let bytes = w
        .record_bytes(project)
        .unwrap_or_else(|| panic!("There is no record for {project:?}"));
    serde_json::from_slice(&bytes).expect("A record is JSON")
}

// ---- The box alone, as v0.06 (H1, H2) ----

/// H2. With no registry, a ready lease of any project is on the box: its holder is the box's,
/// `board-holder.json` is written, and no `board-holder.<project>.json` appears.
#[test]
fn with_no_registry_a_ready_lease_of_any_project_is_the_boxs() {
    let w = World::new();
    let l = lease("alpha", 1);
    w.set_leases(std::slice::from_ref(&l));
    w.plan(&l, World::hold(4242, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![started(0, BOX, &l, 4242)]);
    assert_eq!(w.record_names(), vec!["board-holder.json".to_string()]);
    let r = record_json(&w, None);
    assert_eq!(r["pid"], 4242);
    assert_eq!(r["lease_id"], l.id.as_str());
    assert_eq!(r["alias"], l.alias().as_str());
}

/// A missing registry is no file to create: the keeper never writes `hosts.json` or takes
/// `hosts.lock`, whatever the pass does (keeper.md, Hosts).
#[test]
fn the_keeper_never_creates_the_registry_or_its_lock() {
    let w = World::new();
    let l = sw(1, FAR);
    w.set_leases(std::slice::from_ref(&l));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert!(!w.registry_path().exists());
    assert!(!w.agentspace_dir().join("hosts.lock").exists());
}

/// H1, with a registry that holds the box's record alone: the box behaves as without one, line
/// for line.
#[test]
fn a_registry_with_the_box_alone_reads_as_no_registry() {
    for registry in [false, true] {
        let w = World::new();
        if registry {
            w.registry(&[]);
        }
        let l = sw(1, FAR);
        w.set_leases(std::slice::from_ref(&l));
        w.plan(&l, World::hold(100, None));
        let ran = w.pass();
        assert_eq!(ran.exit, 0, "registry present: {registry}");
        assert_lines(&w, &ran, vec![started(0, BOX, &l, 100)]);
        let ran = w.pass();
        assert_eq!(ran.exit, 0);
        assert_eq!(w.count_prefix("start:"), 1, "registry present: {registry}");
        assert!(w.log().ends_with(&kept(0, BOX, &l, 100)));
    }
}

/// With only the box and nothing to do, a pass logs exactly one line, the box's `idle` with no
/// `host=` field, as v0.06 did.
#[test]
fn with_only_the_box_and_no_lease_a_pass_logs_the_boxs_idle_line_alone() {
    let w = World::new();
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None)]);
}

/// H17. A registry with the box and `alpha`, both ready, and no lease, no record and no log for
/// any host: one line, the box's `idle`, and none for `alpha`.
#[test]
fn a_ready_host_with_nothing_to_do_gets_no_line() {
    let w = World::new();
    w.registry(&["alpha"]);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None)]);
}

/// H17, in a keeper: each pass logs exactly the box's `idle`, never a line for `alpha`.
#[test]
fn a_keeper_with_nothing_to_do_logs_the_boxs_idle_line_each_pass() {
    let w = World::new();
    w.registry(&["alpha"]);
    let ran = w.watch(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![idle(0, None), idle(30, None), idle(60, None)],
    );
}

// ---- Two hosts (H3) ----

/// H3. A registry with the box and `alpha`, both ready; a ready swimcoach lease and a ready
/// `alpha` lease. One pass starts two holders, the box's first, each recorded in its host's
/// file, and the second start begins only after the first host's turn has ended.
#[test]
fn one_pass_starts_a_holder_on_each_host_the_box_first() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1, FAR), lease("alpha", 2));
    w.set_leases(&[a.clone(), s.clone()]);
    w.plan(&s, World::hold(100, None));
    w.plan(&a, World::hold(200, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![started(0, BOX, &s, 100), started(0, "alpha", &a, 200)],
    );
    assert_eq!(
        w.record_names(),
        vec![
            "board-holder.alpha.json".to_string(),
            "board-holder.json".to_string()
        ]
    );
    let box_record = record_json(&w, None);
    assert_eq!(box_record["pid"], 100);
    assert_eq!(box_record["lease_id"], s.id.as_str());
    let alpha_record = record_json(&w, Some("alpha"));
    assert_eq!(alpha_record["pid"], 200);
    assert_eq!(alpha_record["lease_id"], a.id.as_str());
    assert_eq!(alpha_record["alias"], a.alias().as_str());
    // One start after another: the first host's holder is recorded before the second begins.
    assert_eq!(w.started_leases(), vec![s.id.clone(), a.id.clone()]);
    let starts = w.starts();
    assert!(starts[0].records_then.is_empty());
    assert_eq!(
        starts[1].records_then,
        vec!["board-holder.json".to_string()]
    );
}

/// Each host's holder is started through a lease of its own host and never another's: two hosts
/// that both need a holder get both in one pass, whatever order the lease file lists them in.
#[test]
fn a_hosts_lease_is_never_chosen_for_another_host() {
    let w = World::new();
    w.registry(&["alpha", "beta"]);
    let (s, a, b) = (sw(3, FAR), lease("alpha", 1), lease("beta", 2));
    w.set_leases(&[b.clone(), a.clone(), s.clone()]);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    let lines = log_lines(&w);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(
        lines[0].contains(&format!("started lease={} ", s.id))
            && lines[0].ends_with(" host=swimcoach")
    );
    assert!(
        lines[1].contains(&format!("started lease={} ", a.id)) && lines[1].ends_with(" host=alpha")
    );
    assert!(
        lines[2].contains(&format!("started lease={} ", b.id)) && lines[2].ends_with(" host=beta")
    );
}

/// H4, differing expiries. Each host's holder goes through that host's latest-expiring lease,
/// whatever another host's leases expire at.
#[test]
fn each_hosts_holder_goes_through_that_hosts_latest_expiring_lease() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s_early, s_late) = (sw(1, "2026-11-19T13:00:00Z"), sw(2, "2026-11-19T14:00:00Z"));
    let (a_far, a_late) = (
        Lease::new("alpha", 3, FAR),
        Lease::new("alpha", 4, "2026-11-19T15:00:00Z"),
    );
    w.set_leases(&[
        s_early.clone(),
        s_late.clone(),
        a_late.clone(),
        a_far.clone(),
    ]);
    w.plan(&s_late, World::hold(100, None));
    w.plan(&a_far, World::hold(200, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, BOX, &s_late, 100),
            started(0, "alpha", &a_far, 200),
        ],
    );
    assert_eq!(
        w.started_leases(),
        vec![s_late.id.clone(), a_far.id.clone()]
    );
}

/// H4, equal expiries. Ties go to the lowest lease ID in ordinal order, on each host on its own.
#[test]
fn ties_on_each_host_go_to_that_hosts_lowest_lease_id() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s5, s3) = (sw(5, FAR), sw(3, FAR));
    let (a9, a7) = (lease("alpha", 9), lease("alpha", 7));
    w.set_leases(&[s5, a9, s3.clone(), a7.clone()]);
    w.plan(&s3, World::hold(100, None));
    w.plan(&a7, World::hold(200, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![started(0, BOX, &s3, 100), started(0, "alpha", &a7, 200)],
    );
}

// ---- A lease on a host that is not ready is the box's (H5) ----

/// H5. A ready `alpha` lease, `alpha`'s record `creating`, then `failed`, then absent: the lease
/// is on the box each time. The box's holder goes through it, and no `board-holder.alpha.json` is
/// written.
#[test]
fn a_lease_whose_host_is_not_ready_is_on_the_box() {
    for registry in [Some("creating"), Some("failed"), None] {
        let w = World::new();
        match registry {
            Some(state) => w.registry_states(&[("alpha", state)]),
            None => w.registry(&[]),
        }
        let l = lease("alpha", 1);
        w.set_leases(std::slice::from_ref(&l));
        w.plan(&l, World::hold(100, None));
        let ran = w.pass();
        assert_eq!(ran.exit, 0, "alpha {registry:?}");
        assert_lines(&w, &ran, vec![started(0, BOX, &l, 100)]);
        assert_eq!(
            w.record_names(),
            vec!["board-holder.json".to_string()],
            "alpha {registry:?}"
        );
    }
}

// ---- Keeping, stopping and starting, host by host ----

/// A box holder through a lease that is ready on another host is not kept: the box's turn stops
/// it, starts nothing, and the other host's turn, in the same pass, starts its own holder.
#[test]
fn a_holder_through_a_lease_on_another_host_is_stopped_and_that_host_starts_its_own() {
    let w = World::new();
    w.registry(&["alpha"]);
    let a = lease("alpha", 2);
    w.set_leases(std::slice::from_ref(&a));
    w.adopt(None, 100, &a, None);
    w.plan(&a, World::hold(200, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![stopped(0, BOX, &a, 100), started(0, "alpha", &a, 200)],
    );
    assert_eq!(w.stopped_pids(), vec![100]);
    assert_eq!(
        w.record_names(),
        vec!["board-holder.alpha.json".to_string()]
    );
}

/// A stop starts nothing on its own host that pass; the next pass starts the holder for the
/// host's other ready lease. The box's turn is `idle` in both.
#[test]
fn a_stop_starts_nothing_on_its_host_that_pass_and_the_next_pass_does() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (a1, a2) = (lease("alpha", 1), lease("alpha", 2));
    w.set_lease_records(vec![a1.released_json(), a2.json()]);
    w.adopt(Some("alpha"), 200, &a1, None);
    w.plan(&a2, World::hold(201, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None), stopped(0, "alpha", &a1, 200)]);
    assert_eq!(w.count_prefix("start:"), 0);
    assert!(w.record_bytes(Some("alpha")).is_none());
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert!(w.log().ends_with(&format!(
        "{}{}",
        idle(0, None),
        started(0, "alpha", &a2, 201)
    )));
    assert_eq!(record_json(&w, Some("alpha"))["pid"], 201);
}

/// Two hosts' holders are each kept by their own host's lease.
#[test]
fn each_hosts_trusted_holder_is_kept_through_its_own_hosts_lease() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1, FAR), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    w.adopt(None, 100, &s, None);
    w.adopt(Some("alpha"), 200, &a, None);
    let before = (w.record_bytes(None), w.record_bytes(Some("alpha")));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![kept(0, BOX, &s, 100), kept(0, "alpha", &a, 200)],
    );
    assert_eq!(w.count_prefix("start:"), 0);
    assert!(w.stops().is_empty());
    assert_eq!(
        (w.record_bytes(None), w.record_bytes(Some("alpha"))),
        before
    );
}

/// Releasing a lease stops only its host's holder: both hosts' leases released at once, both are
/// stopped in their turns, each removing only its own record.
#[test]
fn a_stop_on_one_host_leaves_the_other_hosts_holder_and_record_alone() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1, FAR), lease("alpha", 2));
    w.set_lease_records(vec![s.json(), a.released_json()]);
    w.adopt(None, 100, &s, None);
    w.adopt(Some("alpha"), 200, &a, None);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![kept(0, BOX, &s, 100), stopped(0, "alpha", &a, 200)],
    );
    assert_eq!(w.stopped_pids(), vec![200]);
    assert!(w.is_alive(100));
    assert!(!w.is_alive(200));
    assert_eq!(w.record_names(), vec!["board-holder.json".to_string()]);
}

// ---- A failure on one host never stops another's (H16 and the card) ----

/// A start that fails on `alpha` is `alpha`'s `failed` line with its `host=`; the box's holder is
/// kept and `gamma`'s is started in the same pass, and the one-pass run exits 0.
#[test]
fn a_failed_start_on_one_host_does_not_touch_the_others() {
    let w = World::new();
    w.registry(&["alpha", "gamma"]);
    let (s, a, g) = (sw(1, FAR), lease("alpha", 2), lease("gamma", 3));
    w.set_leases(&[s.clone(), a.clone(), g.clone()]);
    w.adopt(None, 100, &s, None);
    w.plan(&a, World::fail(&exited_at_once(Some("alpha"))));
    w.plan(&g, World::hold(300, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, BOX, &s, 100),
            failed_start(0, "alpha", &a, &exited_at_once(Some("alpha"))),
            started(0, "gamma", &g, 300),
        ],
    );
    assert_eq!(
        w.record_names(),
        vec![
            "board-holder.gamma.json".to_string(),
            "board-holder.json".to_string()
        ]
    );
}

/// A stop that fails on `alpha` keeps `alpha`'s record and logs its `failed` line with its
/// `host=`; the box's stop, in its own turn, is not affected, and the run exits 0.
#[test]
fn a_failed_stop_on_one_host_does_not_stop_the_others_stop() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1, FAR), lease("alpha", 2));
    w.set_lease_records(vec![s.released_json(), a.released_json()]);
    w.adopt(None, 100, &s, None);
    w.adopt(Some("alpha"), 200, &a, None);
    w.set_stop_for(200, Stopped::StillRunning);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            stopped(0, BOX, &s, 100),
            failed_stop(
                0,
                "alpha",
                &a,
                200,
                "The holder is still running after it was stopped.",
            ),
        ],
    );
    assert_eq!(w.stopped_pids(), vec![100, 200]);
    assert_eq!(
        w.record_names(),
        vec!["board-holder.alpha.json".to_string()]
    );
}

/// H16. A record of `alpha` that cannot be removed ends `alpha`'s turn with a `failed` line that
/// carries `host=alpha`; `gamma`'s turn, which comes after it, still runs and keeps its holder,
/// and the one-pass run exits 1.
#[cfg(windows)]
#[test]
fn a_record_that_cannot_be_removed_ends_only_its_hosts_turn() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    let w = World::new();
    w.registry(&["alpha", "gamma"]);
    let (a, g) = (lease("alpha", 2), lease("gamma", 3));
    w.set_leases(std::slice::from_ref(&g));
    // `alpha`'s record names a process that is gone, so it is not trusted and is to be removed.
    std::fs::write(w.record_path(Some("alpha")), World::record_text(777, &a)).unwrap();
    w.adopt(Some("gamma"), 300, &g, None);
    // Held open for reading, sharing read only: it can be read, but not deleted.
    let held = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(w.record_path(Some("alpha")))
        .expect("The record can be held open");
    let ran = w.pass();
    drop(held);
    assert_eq!(ran.exit, 1);
    let lines = log_lines(&w);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(format!("{}\n", lines[0]), idle(0, None));
    assert!(
        lines[1].starts_with(&format!(
            "{NOW} failed lease=- alias=- pid=- host=alpha reason=The pass stopped on an error: The record could not be removed: "
        )),
        "{}",
        lines[1]
    );
    assert_eq!(format!("{}\n", lines[2]), kept(0, "gamma", &g, 300));
    assert!(w.is_alive(300));
    assert!(w.record_bytes(Some("alpha")).is_some());
}

// ---- Records of hosts the registry does not name (H7, H8) ----

/// H7. A running holder recorded in `board-holder.gamma.json` for a project with no record in the
/// registry: the pass gives `gamma` a turn, and stops that holder. The box's turn comes first.
#[test]
fn a_record_of_a_host_the_registry_lacks_gets_a_turn_and_its_holder_is_stopped() {
    let w = World::new();
    w.registry(&[]);
    let g = lease("gamma", 3);
    w.adopt(Some("gamma"), 300, &g, None);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None), stopped(0, "gamma", &g, 300)]);
    assert_eq!(w.stopped_pids(), vec![300]);
    assert!(w.record_names().is_empty());
}

/// A record that names no trusted holder is removed and its process, if any, left alone; its
/// host's turn is `idle` with `host=`.
#[test]
fn an_untrusted_record_of_another_host_is_removed_and_its_process_left_alone() {
    let w = World::new();
    w.registry(&[]);
    let g = lease("gamma", 3);
    let rec = w.adopt(Some("gamma"), 300, &g, None);
    // A process with that PID runs, but it is not the holder the record names.
    w.kill(rec.pid);
    w.add_process(
        &agentspace_keeper_api::HolderRecord {
            process_started_at: "2026-11-19T11:00:00.000Z".to_string(),
            ..rec.clone()
        },
        None,
    );
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None), idle(0, Some("gamma"))]);
    assert!(w.stops().is_empty());
    assert!(w.is_alive(300));
    assert!(w.record_names().is_empty());
}

/// The box's turn comes first, then every other host's in the ordinal order of its project,
/// whatever order the records were made in.
#[test]
fn the_hosts_turns_are_the_box_then_the_other_projects_in_ordinal_order() {
    let w = World::new();
    w.registry(&[]);
    for (project, n) in [("zeta", 3u32), ("alpha", 1), ("mid", 2)] {
        let l = lease(project, n);
        std::fs::write(
            w.record_path(Some(project)),
            World::record_text(900 + n, &l),
        )
        .unwrap();
    }
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            idle(0, Some("alpha")),
            idle(0, Some("mid")),
            idle(0, Some("zeta")),
        ],
    );
}

/// A record's project is compared without regard to case and taken in lowercase, so
/// `board-holder.Alpha.json` is `alpha`'s record. A file whose project is not a normalized
/// project, a temporary file of a replacement, and the box's own name are not records.
#[cfg(windows)]
#[test]
fn only_files_named_for_a_normalized_project_are_records() {
    let w = World::new();
    w.registry(&[]);
    let a = lease("alpha", 1);
    // The record, in another case, naming a trusted running holder.
    let rec = agentspace_keeper_api::HolderRecord {
        pid: 200,
        process_started_at: STARTED_AT.to_string(),
        lease_id: a.id.clone(),
        alias: a.alias(),
    };
    std::fs::write(
        w.state.join("board-holder.Alpha.json"),
        World::record_text(200, &a),
    )
    .unwrap();
    w.add_process(&rec, None);
    // Not records: each holds a trusted holder's record, so a pass that read one would stop it.
    let long = "a".repeat(33);
    let not_records = [
        "board-holder.-bad.json".to_string(),
        "board-holder.bad_name.json".to_string(),
        "board-holder.bad-.json".to_string(),
        format!("board-holder.{long}.json"),
        "board-holder.alpha.json.1a2b3c.tmp".to_string(),
        "board-holder.json.1a2b3c.tmp".to_string(),
    ];
    for (i, name) in not_records.iter().enumerate() {
        let n = 10 + i as u32;
        let l = lease("alpha", n);
        std::fs::write(w.state.join(name), World::record_text(300 + n, &l)).unwrap();
        w.add_process(
            &agentspace_keeper_api::HolderRecord {
                pid: 300 + n,
                process_started_at: STARTED_AT.to_string(),
                lease_id: l.id.clone(),
                alias: l.alias(),
            },
            None,
        );
    }
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None), stopped(0, "alpha", &a, 200)]);
    assert_eq!(w.stopped_pids(), vec![200]);
    for name in &not_records {
        assert!(w.state.join(name).exists(), "{name} was removed");
    }
}

/// H8. A `board-holder.swimcoach.json` naming a running holder is never read, removed or
/// stopped: the box's record is `board-holder.json` alone.
#[test]
fn a_record_named_for_the_boxs_project_is_never_read_removed_or_stopped() {
    let w = World::new();
    w.registry(&[]);
    let s = sw(1, FAR);
    w.adopt(Some(BOX), 100, &s, None);
    let before = w.record_bytes(Some(BOX));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None)]);
    assert!(w.stops().is_empty());
    assert!(w.is_alive(100));
    assert_eq!(w.record_bytes(Some(BOX)), before);
    assert_eq!(w.count_prefix("start:"), 0);
}

/// H8, with a box holder of its own beside it: the box's turn uses `board-holder.json` alone.
#[test]
fn the_boxs_turn_uses_its_own_record_beside_a_file_named_for_its_project() {
    let w = World::new();
    w.registry(&[]);
    let (s1, s2) = (sw(1, FAR), sw(2, FAR));
    w.set_leases(std::slice::from_ref(&s1));
    w.adopt(None, 100, &s1, None);
    w.adopt(Some(BOX), 101, &s2, None);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![kept(0, BOX, &s1, 100)]);
    assert!(w.stops().is_empty());
    assert!(w.record_bytes(Some(BOX)).is_some());
}

// ---- A hand edit of the registry (H6, H18) ----

/// H6. `alpha`'s holder is running and recorded, and the registry now has `alpha` `creating`, as a
/// person's edit would leave it: the next pass stops it, removes its record, and keeps the box's
/// holder beside it.
#[test]
fn a_host_taken_out_of_ready_by_an_edit_has_its_holder_stopped_and_the_box_kept() {
    let w = World::new();
    w.registry_states(&[("alpha", "creating")]);
    let (s, a) = (sw(1, FAR), lease("alpha", 2));
    // `alpha`'s lease is still ready, so it is on the box now; the box's own lease holds its holder.
    w.set_leases(&[s.clone(), a.clone()]);
    w.adopt(None, 100, &s, None);
    w.adopt(Some("alpha"), 200, &a, None);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![kept(0, BOX, &s, 100), stopped(0, "alpha", &a, 200)],
    );
    assert_eq!(w.stopped_pids(), vec![200]);
    assert!(w.is_alive(100));
    assert_eq!(w.record_names(), vec!["board-holder.json".to_string()]);
}

/// H18. `AGENTSPACE_STATE_DIR` is not `<USERPROFILE>\.agentspace`, and a `hosts.json` that is not
/// JSON sits in the state folder: the registry is read from `<USERPROFILE>\.agentspace` and never
/// from the state folder.
#[test]
fn the_registry_is_read_from_the_home_folder_never_from_the_state_folder() {
    let w = World::new();
    w.registry(&["alpha"]);
    std::fs::write(w.state.join("hosts.json"), b"not json\n").unwrap();
    let a = lease("alpha", 2);
    w.set_leases(std::slice::from_ref(&a));
    w.plan(&a, World::hold(200, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None), started(0, "alpha", &a, 200)]);
}

// ---- A registry that cannot be read (H12) ----

fn registry_cases() -> Vec<(&'static str, Option<Vec<u8>>)> {
    let box_record = r#"{"project":"swimcoach","stack":"agentbox","state":"ready"}"#;
    let alpha = r#"{"project":"alpha","stack":"agentspace-host-alpha","state":"ready"}"#;
    vec![
        ("not JSON", Some(b"not json\n".to_vec())),
        (
            "fails its schema",
            Some(format!("{{\"version\":1,\"hosts\":[{box_record}]}}\n").into_bytes()),
        ),
        (
            "two records share a project",
            Some(format!("{{\"version\":0,\"hosts\":[{box_record},{alpha},{alpha}]}}\n").into_bytes()),
        ),
        (
            "no agentbox record",
            Some(format!("{{\"version\":0,\"hosts\":[{alpha}]}}\n").into_bytes()),
        ),
        (
            "the agentbox record's project is not swimcoach",
            Some(
                format!(
                    "{{\"version\":0,\"hosts\":[{{\"project\":\"other\",\"stack\":\"agentbox\",\"state\":\"ready\"}},{alpha}]}}\n"
                )
                .into_bytes(),
            ),
        ),
        ("a folder in its place", None),
    ]
}

/// H12. Each of: a registry that is not JSON, one that fails its schema, one in which two records
/// share a project, one with no `agentbox` record, one whose `agentbox` record's project is not
/// `swimcoach`, and a folder in its place; with holders running on two hosts. Each gives one
/// `failed lease=- alias=- pid=- reason=Host registry could not be read:` line, exit 1, both
/// holders running and both records unchanged.
#[test]
fn a_registry_that_cannot_be_read_fails_the_pass_and_leaves_every_holder_and_record() {
    for (what, bytes) in registry_cases() {
        let w = World::new();
        match &bytes {
            Some(b) => w.registry_raw(b),
            None => std::fs::create_dir_all(w.registry_path()).unwrap(),
        }
        let (s, a) = (sw(1, FAR), lease("alpha", 2));
        w.set_leases(&[s.clone(), a.clone()]);
        w.adopt(None, 100, &s, None);
        w.adopt(Some("alpha"), 200, &a, None);
        let before = (w.record_bytes(None), w.record_bytes(Some("alpha")));
        let ran = w.pass();
        assert_eq!(ran.exit, 1, "{what}");
        let lines = log_lines(&w);
        assert_eq!(lines.len(), 1, "{what}: {lines:?}");
        let prefix =
            format!("{NOW} failed lease=- alias=- pid=- reason=Host registry could not be read: ");
        assert!(lines[0].starts_with(&prefix), "{what}: {}", lines[0]);
        if bytes.is_some() {
            assert!(
                lines[0].ends_with("the host registry is not valid"),
                "{what}: {}",
                lines[0]
            );
        } else {
            // A folder cannot be opened: it is unreadable, never "not valid" (keeper.md, Hosts;
            // architect, 2026-11-21, for v0.07-T03).
            assert!(
                lines[0].ends_with("the host registry could not be read: Windows error 5."),
                "{what}: {}",
                lines[0]
            );
        }
        assert_eq!(ran.stdout, format!("{}\n", lines[0]), "{what}");
        assert_eq!(ran.stderr, "", "{what}");
        assert!(w.stops().is_empty(), "{what}");
        assert_eq!(w.count_prefix("start:"), 0, "{what}");
        assert!(w.is_alive(100) && w.is_alive(200), "{what}");
        assert_eq!(
            (w.record_bytes(None), w.record_bytes(Some("alpha"))),
            before,
            "{what}"
        );
        assert_pass_lock_released(&w);
    }
}

/// An unreadable lease state fails the pass the same way, with every holder and record left as
/// they are (keeper.md, A pass, step 4).
#[test]
fn a_lease_state_that_cannot_be_read_leaves_every_hosts_holder_and_record() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1, FAR), lease("alpha", 2));
    w.adopt(None, 100, &s, None);
    w.adopt(Some("alpha"), 200, &a, None);
    std::fs::write(w.state.join("leases.json"), b"not json\n").unwrap();
    let before = (w.record_bytes(None), w.record_bytes(Some("alpha")));
    let ran = w.pass();
    assert_eq!(ran.exit, 1);
    let lines = log_lines(&w);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with(&format!(
            "{NOW} failed lease=- alias=- pid=- reason=Lease state could not be read: "
        )),
        "{}",
        lines[0]
    );
    assert!(w.stops().is_empty());
    assert!(w.is_alive(100) && w.is_alive(200));
    assert_eq!(
        (w.record_bytes(None), w.record_bytes(Some("alpha"))),
        before
    );
}

/// H13. The keeper reads the registry without its lock: a pass runs while `hosts.lock` is held
/// with no sharing, and never writes the registry, touches its time stamps or creates the lock.
#[cfg(windows)]
#[test]
fn a_held_registry_lock_never_delays_a_pass_and_the_registry_is_never_written() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    let w = World::new();
    w.registry(&["alpha"]);
    let lock = w.agentspace_dir().join("hosts.lock");
    let (s, a) = (sw(1, FAR), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    w.plan(&s, World::hold(100, None));
    w.plan(&a, World::hold(200, None));
    let bytes_before = std::fs::read(w.registry_path()).unwrap();
    let modified_before = std::fs::metadata(w.registry_path())
        .and_then(|m| m.modified())
        .unwrap();
    let held = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(&lock)
        .expect("The registry's lock can be held");
    let ran = w.pass();
    drop(held);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![started(0, BOX, &s, 100), started(0, "alpha", &a, 200)],
    );
    assert_eq!(std::fs::read(w.registry_path()).unwrap(), bytes_before);
    let modified_after = std::fs::metadata(w.registry_path())
        .and_then(|m| m.modified())
        .unwrap();
    assert_eq!(modified_after, modified_before);
}

// ---- Each host's holder log is rotated on its own (H14) ----

fn write_sized(w: &World, name: &str, len: usize) {
    std::fs::write(w.state.join(name), vec![b'x'; len]).unwrap();
}

const MIB: usize = 1_048_576;

/// H14. `board-holder-ssh.<project>.log` over 1 MiB is renamed to `.log.1`, and
/// `rotated lease=- alias=- pid=- host=<project>` is logged after the box's `rotated` line, the
/// other hosts' in the ordinal order of their projects; before any host's action line. A log of
/// exactly 1 MiB, one named for the box's project and a name that is no project's are left.
#[test]
fn each_hosts_holder_log_over_one_mib_is_rotated_after_the_boxs() {
    let w = World::new();
    write_sized(&w, "board-holder-ssh.log", MIB + 1);
    write_sized(&w, "board-holder-ssh.gamma.log", MIB * 2);
    write_sized(&w, "board-holder-ssh.alpha.log", MIB + 1);
    write_sized(&w, "board-holder-ssh.beta.log", MIB);
    write_sized(&w, "board-holder-ssh.swimcoach.log", MIB + 1);
    write_sized(&w, "board-holder-ssh.Bad_Name.log", MIB + 1);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            rotated(0, None),
            rotated(0, Some("alpha")),
            rotated(0, Some("gamma")),
            idle(0, None),
        ],
    );
    for (log, old, len) in [
        ("board-holder-ssh.log", "board-holder-ssh.log.1", MIB + 1),
        (
            "board-holder-ssh.alpha.log",
            "board-holder-ssh.alpha.log.1",
            MIB + 1,
        ),
        (
            "board-holder-ssh.gamma.log",
            "board-holder-ssh.gamma.log.1",
            MIB * 2,
        ),
    ] {
        assert!(!w.state.join(log).exists(), "{log} was not renamed");
        assert_eq!(
            std::fs::metadata(w.state.join(old)).unwrap().len() as usize,
            len
        );
    }
    for name in [
        "board-holder-ssh.beta.log",
        "board-holder-ssh.swimcoach.log",
        "board-holder-ssh.Bad_Name.log",
    ] {
        assert!(w.state.join(name).exists(), "{name} was rotated");
        assert!(!w.state.join(format!("{name}.1")).exists());
    }
}

/// The rotation comes before the registry is read, since each log is named by its project: a
/// registry that cannot be read still has its hosts' logs rotated, and the rotation's line comes
/// before the `failed` line.
#[test]
fn a_hosts_log_is_rotated_even_when_the_registry_cannot_be_read() {
    let w = World::new();
    w.registry_raw(b"not json\n");
    write_sized(&w, "board-holder-ssh.alpha.log", MIB + 1);
    let ran = w.pass();
    assert_eq!(ran.exit, 1);
    let lines = log_lines(&w);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(format!("{}\n", lines[0]), rotated(0, Some("alpha")));
    assert!(lines[1].contains("reason=Host registry could not be read: "));
    assert!(w.state.join("board-holder-ssh.alpha.log.1").exists());
}

// ---- Adoption of v0.06's record (H15) ----

/// H15. Upgrade. A running holder is recorded in v0.06's form in `board-holder.json`, through a
/// ready swimcoach lease, with no registry: the first pass logs `kept ... host=swimcoach` and
/// adopts it. No new start, the record unchanged to the byte and the time stamp, and nothing else
/// written to the state folder but the log.
#[test]
fn an_upgraded_keeper_keeps_the_box_holder_v006_recorded_and_writes_nothing_to_it() {
    for registry in [false, true] {
        let w = World::new();
        if registry {
            w.registry(&[]);
        }
        let s = sw(1, FAR);
        w.set_leases(std::slice::from_ref(&s));
        w.adopt(None, 4242, &s, None);
        let bytes = w.record_bytes(None).unwrap();
        assert_eq!(
            String::from_utf8(bytes.clone()).unwrap(),
            World::record_text(4242, &s),
            "The fixture is v0.06's form"
        );
        let modified = std::fs::metadata(w.record_path(None))
            .and_then(|m| m.modified())
            .unwrap();
        let ran = w.pass();
        assert_eq!(ran.exit, 0, "registry present: {registry}");
        assert_lines(&w, &ran, vec![kept(0, BOX, &s, 4242)]);
        assert_eq!(w.count_prefix("start:"), 0);
        assert!(w.stops().is_empty());
        assert_eq!(w.record_bytes(None), Some(bytes));
        assert_eq!(
            std::fs::metadata(w.record_path(None))
                .and_then(|m| m.modified())
                .unwrap(),
            modified
        );
        assert_eq!(
            w.state_names(),
            vec![
                "board-holder.json".to_string(),
                "board-holder.log".to_string(),
                "leases.json".to_string()
            ]
        );
    }
}

/// An upgraded keeper reads v0.06's record with v0.06's rules: a record whose `lease_id` is in
/// another case than the lease's is the same lease (state.md, The holder record).
#[test]
fn v006s_record_is_read_with_v006s_rules() {
    let w = World::new();
    let s = sw(0xabcdef12, FAR);
    w.set_leases(std::slice::from_ref(&s));
    let upper = Lease {
        id: s.id.to_uppercase(),
        ..s.clone()
    };
    let rec = agentspace_keeper_api::HolderRecord {
        pid: 4242,
        process_started_at: STARTED_AT.to_string(),
        lease_id: upper.id.clone(),
        alias: s.alias(),
    };
    std::fs::write(
        w.record_path(None),
        format!(
            "{{\"pid\":4242,\"process_started_at\":\"{STARTED_AT}\",\"lease_id\":\"{}\",\"alias\":\"{}\"}}\n",
            upper.id,
            s.alias()
        ),
    )
    .unwrap();
    w.add_process(&rec, None);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![format!(
            "{} kept lease={} alias={} pid=4242 host=swimcoach\n",
            at(0),
            upper.id,
            s.alias()
        )],
    );
}

// ---- The holder's log is its host's (keeper.md, The handshake, steps 1 and 7) ----

/// keeper.md, Starting a holder, item 2 and The handshake, step 1 (architect, 2026-11-21, for
/// v0.07-T03): the start request names the log of the host it starts on. The box's names none, so
/// the starter writes `board-holder-ssh.log` as in v0.06, and another host's names
/// `board-holder-ssh.<project>.log` in the state folder, as a full path.
#[test]
fn the_start_request_names_the_log_of_the_host_it_starts_on() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1, FAR), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    let starts = w.starts();
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0].lease_id, s.id);
    assert_eq!(starts[0].log, None, "The box's request names a log");
    assert_eq!(starts[1].lease_id, a.id);
    assert_eq!(
        starts[1].log.as_deref(),
        Some(w.state.join("board-holder-ssh.alpha.log").as_path()),
        "alpha's request does not name board-holder-ssh.alpha.log in the state folder"
    );
}

/// Two hosts that are not the box get two holders writing two logs: each start names its own
/// host's `board-holder-ssh.<project>.log`, and no two starts name the same one.
#[test]
fn two_hosts_holders_write_two_logs() {
    let w = World::new();
    w.registry(&["alpha", "beta"]);
    let (a, b) = (lease("alpha", 2), lease("beta", 3));
    w.set_leases(&[a.clone(), b.clone()]);
    w.plan(&a, World::hold(200, None));
    w.plan(&b, World::hold(300, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            started(0, "alpha", &a, 200),
            started(0, "beta", &b, 300),
        ],
    );
    let logs: Vec<(String, Option<std::path::PathBuf>)> = w
        .starts()
        .into_iter()
        .map(|st| (st.lease_id, st.log))
        .collect();
    assert_eq!(
        logs,
        vec![
            (
                a.id.clone(),
                Some(w.state.join("board-holder-ssh.alpha.log"))
            ),
            (
                b.id.clone(),
                Some(w.state.join("board-holder-ssh.beta.log"))
            ),
        ]
    );
}

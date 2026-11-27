//! One pass, and a keeper, over a project's hosts on several platforms (docs/keeper.md, A host's
//! key and name, Which host a lease is on, The hosts of a pass, Which lease, Trusting the record,
//! The holder's log, Conformance cases for a host per project and platform; docs/state.md, The
//! holder record, Another host's holder, A host's platform and names; D50 under D91; E0.12-T03-S05,
//! #0258).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **From v0.12 a host is
//! keyed by its project and its platform, and the keeper holds one holder per host.** A pass
//! groups the ready leases by project and platform, takes a turn for each host, and in each turn
//! keeps, stops or starts the one holder of that host through that host's own leases alone: a lease
//! of the project on another platform never extends, moves or ends another host's holder. A host
//! of a platform other than `linux/arm64` is named `<project>.<os>-<arch>` in the keeper's files
//! and lines, and its record carries `platform`; a `linux/arm64` host keeps v0.11's names and bytes,
//! and a record with no `platform` is `linux/arm64`'s.
//!
//! Each test is named for the behavior it shows, and `H<n>` names the conformance case of
//! keeper.md it matches. Every expectation is taken from keeper.md and state.md, never from the
//! crate's code (D31, D42). The files are real, in a throwaway home (`tests/hosts_world/mod.rs`):
//! the registry is `<home>\.agentspace\hosts.json`, the lease state and each host's record and log
//! are in the throwaway state folder, and the processes, the matcher, the starter, the locks and
//! the clock are fakes of the pinned traits. Every keeper of these tests has a stand-in guard.

mod hosts_world;

use hosts_world::*;
use serde_json::Value;

const ARM: &str = "linux/arm64";
const LIN: &str = "linux/amd64";
const WIN: &str = "windows/amd64";

/// The host names of `alpha`'s two other hosts (keeper.md, A host's key and name).
const LIN_HOST: &str = "alpha.linux-amd64";
const WIN_HOST: &str = "alpha.windows-amd64";

/// An expiry after `NOW`, before `FAR`.
const LATER: &str = "2026-11-19T14:00:00Z";
/// An expiry after `NOW`, before `LATER`.
const SOON: &str = "2026-11-19T13:00:00Z";
/// An expiry before `NOW`: a lease that has reached it is no longer ready.
const PAST: &str = "2026-11-19T11:00:00Z";

fn arm(n: u32, expires: &str) -> Lease {
    Lease::on(ARM, "alpha", n, expires)
}

fn lin(n: u32, expires: &str) -> Lease {
    Lease::on(LIN, "alpha", n, expires)
}

fn win(n: u32, expires: &str) -> Lease {
    Lease::on(WIN, "alpha", n, expires)
}

fn record_text_of(w: &World, host: Option<&str>) -> String {
    String::from_utf8(
        w.record_bytes(host)
            .unwrap_or_else(|| panic!("There is no record for {host:?}")),
    )
    .expect("A record is UTF-8")
}

fn record_json(w: &World, host: Option<&str>) -> Value {
    serde_json::from_str(&record_text_of(w, host)).expect("A record is JSON")
}

// ---- Two hosts of one project, each with its own holder (H19) ----

/// H19. A registry with the box and `alpha`'s `linux/arm64` and `windows/amd64` hosts, all
/// `ready`, and a ready `alpha` lease on each platform: one pass starts two holders, one through
/// each lease and never one through both, `host=alpha` and `host=alpha.windows-amd64`, after the
/// box's turn, whatever order the lease file lists the leases in.
#[test]
fn one_pass_starts_a_holder_on_each_of_a_projects_two_hosts() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a, x) = (arm(1, FAR), win(2, FAR));
    w.set_leases(&[x.clone(), a.clone()]);
    w.plan(&a, World::hold(200, None));
    w.plan(&x, World::hold(300, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            started(0, "alpha", &a, 200),
            started(0, WIN_HOST, &x, 300),
        ],
    );
    assert_eq!(w.started_leases(), vec![a.id.clone(), x.id.clone()]);
}

/// H19, the other half: each holder is recorded in its own host's file, and each launch request
/// names its own host's log: `board-holder-ssh.alpha.log` and
/// `board-holder-ssh.alpha.windows-amd64.log`.
#[test]
fn each_of_the_two_holders_has_its_own_record_file_and_its_own_log() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a, x) = (arm(1, FAR), win(2, FAR));
    w.set_leases(&[a.clone(), x.clone()]);
    w.plan(&a, World::hold(200, None));
    w.plan(&x, World::hold(300, None));
    assert_eq!(w.pass().exit, 0);
    assert_eq!(
        w.record_names(),
        vec![
            "board-holder.alpha.json".to_string(),
            "board-holder.alpha.windows-amd64.json".to_string()
        ]
    );
    assert_eq!(record_json(&w, Some("alpha"))["pid"], 200);
    assert_eq!(record_json(&w, Some("alpha"))["lease_id"], a.id.as_str());
    assert_eq!(record_json(&w, Some(WIN_HOST))["pid"], 300);
    assert_eq!(record_json(&w, Some(WIN_HOST))["lease_id"], x.id.as_str());
    let logs: Vec<String> = w
        .starts()
        .iter()
        .map(|s| {
            s.log
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .expect("Another host's start names its own log")
        })
        .collect();
    assert_eq!(
        logs,
        vec![
            "board-holder-ssh.alpha.log".to_string(),
            "board-holder-ssh.alpha.windows-amd64.log".to_string()
        ]
    );
}

/// D91: a project holds up to three hosts. All three of `alpha`'s are ready with a lease each, and
/// a pass starts three holders, one per host, each through its own platform's lease.
#[test]
fn a_project_with_three_hosts_gets_three_holders() {
    let w = World::new();
    w.registry_hosts(&[
        ("alpha", ARM, "ready"),
        ("alpha", LIN, "ready"),
        ("alpha", WIN, "ready"),
    ]);
    let (a, l, x) = (arm(1, FAR), lin(2, FAR), win(3, FAR));
    w.set_leases(&[x.clone(), l.clone(), a.clone()]);
    w.plan(&a, World::hold(200, None));
    w.plan(&l, World::hold(300, None));
    w.plan(&x, World::hold(400, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            started(0, "alpha", &a, 200),
            started(0, LIN_HOST, &l, 300),
            started(0, WIN_HOST, &x, 400),
        ],
    );
}

/// state.md, The holder record: a writer omits `platform` for a `linux/arm64` host, the box's
/// included, and writes it, last, for a host of any other platform. So a `linux/arm64` record
/// keeps the bytes it had before v0.12.
#[test]
fn only_a_host_that_is_not_linux_arm64_has_a_platform_in_its_record() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (s, a, x) = (Lease::new(BOX, 1, FAR), arm(2, FAR), win(3, FAR));
    w.set_leases(&[s.clone(), a.clone(), x.clone()]);
    w.plan(&s, World::hold(100, None));
    w.plan(&a, World::hold(200, None));
    w.plan(&x, World::hold(300, None));
    assert_eq!(w.pass().exit, 0);

    let box_text = record_text_of(&w, None);
    assert!(
        box_text.starts_with("{\"pid\":100,\"process_started_at\":\""),
        "{box_text}"
    );
    assert!(
        box_text.ends_with(&format!("\"alias\":\"{}\"}}\n", s.alias())),
        "The box's record is v0.11's: {box_text}"
    );
    let arm_text = record_text_of(&w, Some("alpha"));
    assert!(
        arm_text.ends_with(&format!("\"alias\":\"{}\"}}\n", a.alias())),
        "A linux/arm64 host's record is v0.11's: {arm_text}"
    );
    assert!(!box_text.contains("platform") && !arm_text.contains("platform"));

    let win_text = record_text_of(&w, Some(WIN_HOST));
    assert!(
        win_text.starts_with("{\"pid\":300,\"process_started_at\":\""),
        "{win_text}"
    );
    assert!(
        win_text.ends_with(&format!(
            "\"alias\":\"{}\",\"platform\":\"windows/amd64\"}}\n",
            x.alias()
        )),
        "A windows/amd64 host's record ends with its platform: {win_text}"
    );
}

// ---- A pass groups by project and platform: each host holds through its own lease ----

/// H4 by platform. Each host's holder goes through the latest-expiring ready lease of that host's
/// platform, whatever a lease of the project on another platform expires at: `alpha`'s
/// `linux/arm64` holder goes through the lease that expires at `LATER`, though a `windows/amd64`
/// lease of `alpha` expires at `FAR`.
#[test]
fn each_host_holds_through_the_lease_of_its_own_platform_that_expires_last() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a_soon, a_later) = (arm(1, SOON), arm(2, LATER));
    let (x_far, x_soon) = (win(3, FAR), win(4, SOON));
    w.set_leases(&[
        a_soon.clone(),
        x_soon.clone(),
        a_later.clone(),
        x_far.clone(),
    ]);
    w.plan(&a_later, World::hold(200, None));
    w.plan(&x_far, World::hold(300, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            started(0, "alpha", &a_later, 200),
            started(0, WIN_HOST, &x_far, 300),
        ],
    );
    assert_eq!(
        w.started_leases(),
        vec![a_later.id.clone(), x_far.id.clone()]
    );
}

/// H4's tie by platform. Equal expiries on each host go to that host's lowest lease ID in ordinal
/// order, on each host on its own: the `linux/arm64` host's lowest is `00000007`, though a
/// `windows/amd64` lease of the project has a lower ID, `00000003`.
#[test]
fn ties_go_to_the_lowest_lease_id_of_the_hosts_own_platform() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a9, a7) = (arm(9, FAR), arm(7, FAR));
    let (x5, x3) = (win(5, FAR), win(3, FAR));
    w.set_leases(&[a9, x5, a7.clone(), x3.clone()]);
    w.plan(&a7, World::hold(200, None));
    w.plan(&x3, World::hold(300, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            started(0, "alpha", &a7, 200),
            started(0, WIN_HOST, &x3, 300),
        ],
    );
}

/// A lease of the project on another platform never extends a holder that runs: the
/// `linux/arm64` holder through its own lease, which expires soon, is kept as it is, and the
/// `windows/amd64` lease that expires much later gets a holder of its own host. Nothing is
/// stopped.
#[test]
fn a_later_lease_of_another_platform_neither_moves_nor_extends_a_running_holder() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a, x) = (arm(1, SOON), win(2, FAR));
    w.set_leases(&[a.clone(), x.clone()]);
    w.adopt(Some("alpha"), 100, &a, None);
    w.plan(&x, World::hold(300, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            kept(0, "alpha", &a, 100),
            started(0, WIN_HOST, &x, 300),
        ],
    );
    assert!(w.stopped_pids().is_empty());
    assert_eq!(w.started_leases(), vec![x.id.clone()]);
}

/// A holder's lease ending ends that host's holder, and only that one: the `windows/amd64`
/// lease is released, expired or gone, the `linux/arm64` lease is ready far into the future, and
/// the `windows/amd64` holder is stopped (its record removed, its process gone) while the
/// `linux/arm64` holder is kept in the same pass (H23).
#[test]
fn a_lease_ending_on_one_platform_ends_only_that_hosts_holder() {
    for form in ["released", "expired", "absent"] {
        let w = World::new();
        w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
        let (a, x) = (arm(1, FAR), win(2, FAR));
        let x_ending = match form {
            "released" => x.released_json(),
            "expired" => win(2, PAST).json(),
            _ => serde_json::json!(null),
        };
        let mut records = vec![a.json()];
        if !x_ending.is_null() {
            records.push(x_ending);
        }
        w.set_lease_records(records);
        w.adopt(Some("alpha"), 100, &a, None);
        w.adopt_text(
            Some(WIN_HOST),
            300,
            &x,
            None,
            &World::record_text_with_platform(300, &x, WIN),
        );
        let ran = w.pass();
        assert_eq!(ran.exit, 0, "{form}");
        assert_lines(
            &w,
            &ran,
            vec![
                idle(0, None),
                kept(0, "alpha", &a, 100),
                stopped(0, WIN_HOST, &x, 300),
            ],
        );
        assert_eq!(w.stopped_pids(), vec![300], "{form}");
        assert!(w.is_alive(100), "{form}: The linux/arm64 holder runs");
        assert!(!w.is_alive(300), "{form}: The windows/amd64 holder is gone");
        assert_eq!(
            w.record_names(),
            vec!["board-holder.alpha.json".to_string()],
            "{form}"
        );
    }
}

/// The same the other way round: the `linux/arm64` lease ends and the `windows/amd64` lease is
/// ready, so the `linux/arm64` holder is stopped and the `windows/amd64` holder is kept.
#[test]
fn a_lease_ending_on_the_arm_host_leaves_the_other_platforms_holder_running() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a, x) = (arm(1, FAR), win(2, FAR));
    w.set_lease_records(vec![a.released_json(), x.json()]);
    w.adopt(Some("alpha"), 100, &a, None);
    w.adopt_text(
        Some(WIN_HOST),
        300,
        &x,
        None,
        &World::record_text_with_platform(300, &x, WIN),
    );
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            stopped(0, "alpha", &a, 100),
            kept(0, WIN_HOST, &x, 300),
        ],
    );
    assert_eq!(w.stopped_pids(), vec![100]);
    assert!(w.is_alive(300));
    assert_eq!(
        w.record_names(),
        vec!["board-holder.alpha.windows-amd64.json".to_string()]
    );
}

/// A stop starts nothing on its host that pass, as for any host: the `windows/amd64` host's holder
/// is stopped, and its other ready lease gets a holder in the next pass, never through the
/// `linux/arm64` host's lease.
#[test]
fn a_stop_on_one_platforms_host_starts_that_hosts_next_lease_in_the_next_pass() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a, x1, x2) = (arm(1, FAR), win(2, FAR), win(3, FAR));
    w.set_lease_records(vec![a.json(), x1.released_json(), x2.json()]);
    w.adopt(Some("alpha"), 100, &a, None);
    w.adopt_text(
        Some(WIN_HOST),
        300,
        &x1,
        None,
        &World::record_text_with_platform(300, &x1, WIN),
    );
    w.plan(&x2, World::hold(301, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            kept(0, "alpha", &a, 100),
            stopped(0, WIN_HOST, &x1, 300),
        ],
    );
    assert_eq!(w.count_prefix("start:"), 0);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert!(w.log().ends_with(&format!(
        "{}{}{}",
        idle(0, None),
        kept(0, "alpha", &a, 100),
        started(0, WIN_HOST, &x2, 301)
    )));
    assert_eq!(w.started_leases(), vec![x2.id.clone()]);
}

// ---- A lease is on its own platform's host, or on no host (H21, H22) ----

/// H21. A registry with the box and `alpha`'s `linux/arm64` host `ready`, and a record of `alpha`'s
/// `windows/amd64` host that is `creating`; a ready `alpha` lease for each platform, the
/// `windows/amd64` one expiring later. The `windows/amd64` lease is not on the box: the
/// `linux/arm64` host's holder goes through the `linux/arm64` lease alone, and no line, record or
/// log of `alpha.windows-amd64` appears.
#[test]
fn a_lease_whose_platforms_host_is_not_ready_is_on_no_host() {
    for state in ["creating", "failed"] {
        let w = World::new();
        w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, state)]);
        let (a, x) = (arm(1, SOON), win(2, FAR));
        w.set_leases(&[x.clone(), a.clone()]);
        w.plan(&a, World::hold(200, None));
        let ran = w.pass();
        assert_eq!(ran.exit, 0, "{state}");
        assert_lines(&w, &ran, vec![idle(0, None), started(0, "alpha", &a, 200)]);
        assert_eq!(w.started_leases(), vec![a.id.clone()], "{state}");
        assert_eq!(
            w.record_names(),
            vec!["board-holder.alpha.json".to_string()],
            "{state}"
        );
        assert!(
            !w.state_names()
                .iter()
                .any(|n| n.contains("windows-amd64") || n == "board-holder.json"),
            "{state}: {:?}",
            w.state_names()
        );
        assert!(!w.log().contains("windows-amd64"), "{state}");
    }
}

/// H22. A ready `windows/amd64` lease of `alpha`, with no registry and with a registry that holds
/// the box alone: the lease is on no host. No holder is started, the pass logs only the box's
/// `idle`, and no `board-holder.alpha.windows-amd64.json` appears. It is never the box's, since
/// the box is a `linux/arm64` host.
#[test]
fn with_no_host_of_its_platform_a_lease_is_on_no_host_and_never_the_boxs() {
    for registry in [false, true] {
        for platform in [WIN, LIN] {
            let w = World::new();
            if registry {
                w.registry(&[]);
            }
            let x = Lease::on(platform, "alpha", 1, FAR);
            w.set_leases(std::slice::from_ref(&x));
            let ran = w.pass();
            assert_eq!(ran.exit, 0, "{platform}, registry {registry}");
            assert_lines(&w, &ran, vec![idle(0, None)]);
            assert_eq!(w.count_prefix("start:"), 0, "{platform}");
            assert!(w.record_names().is_empty(), "{platform}");
        }
    }
}

/// A lease of `alpha` on a platform whose only `ready` host is another project's is on no host:
/// the registry keys a host by project and platform, so `beta`'s `windows/amd64` host is not
/// `alpha`'s, and `alpha`'s `windows/amd64` lease is not on it, nor on the box.
#[test]
fn another_projects_host_on_the_same_platform_is_not_a_leases_host() {
    let w = World::new();
    w.registry_hosts(&[("beta", WIN, "ready")]);
    let x = win(1, FAR);
    w.set_leases(std::slice::from_ref(&x));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![idle(0, None)]);
    assert_eq!(w.count_prefix("start:"), 0);
    assert!(w.record_names().is_empty());
}

// ---- The box's project on another platform is another host (H26) ----

/// H26. `swimcoach` has a `windows/amd64` host `ready` beside the box. A running stub is the box's
/// holder through a `swimcoach` `linux/arm64` lease, in `board-holder.json`, and another is the
/// `windows/amd64` host's, through a `swimcoach` `windows/amd64` lease, in
/// `board-holder.swimcoach.windows-amd64.json`. The box's turn uses `board-holder.json` alone, the
/// other host's turn uses its own file, and neither is read as the other's.
#[test]
fn the_boxs_project_on_another_platform_is_another_host_with_its_own_record() {
    let w = World::new();
    w.registry_hosts(&[(BOX, WIN, "ready")]);
    let s = Lease::on(ARM, BOX, 1, FAR);
    let sx = Lease::on(WIN, BOX, 2, FAR);
    w.set_leases(&[sx.clone(), s.clone()]);
    w.adopt(None, 100, &s, None);
    w.adopt_text(
        Some("swimcoach.windows-amd64"),
        200,
        &sx,
        None,
        &World::record_text_with_platform(200, &sx, WIN),
    );
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, BOX, &s, 100),
            kept(0, "swimcoach.windows-amd64", &sx, 200),
        ],
    );
    assert!(w.stopped_pids().is_empty());
    assert_eq!(w.count_prefix("start:"), 0);
    assert_eq!(
        w.record_names(),
        vec![
            "board-holder.json".to_string(),
            "board-holder.swimcoach.windows-amd64.json".to_string()
        ]
    );
}

// ---- The turns follow the registry's file order (H20) ----

/// H20. A registry holding, after the box's record and in this file order, `beta`'s `linux/amd64`
/// host, `alpha`'s `linux/arm64` host, `beta`'s `linux/arm64` host and `alpha`'s `linux/amd64`
/// host, all `ready`, with `alpha` before `beta` in ordinal order, and a ready lease on each: the
/// turns follow the registry's file order and no sorted order (pin P46).
#[test]
fn the_turns_follow_the_registrys_file_order_and_no_sorted_order() {
    let w = World::new();
    w.registry_hosts(&[
        ("beta", LIN, "ready"),
        ("alpha", ARM, "ready"),
        ("beta", ARM, "ready"),
        ("alpha", LIN, "ready"),
    ]);
    let (b_lin, a_arm, b_arm, a_lin) = (
        Lease::on(LIN, "beta", 1, FAR),
        Lease::on(ARM, "alpha", 2, FAR),
        Lease::on(ARM, "beta", 3, FAR),
        Lease::on(LIN, "alpha", 4, FAR),
    );
    w.set_leases(&[a_lin.clone(), b_arm.clone(), a_arm.clone(), b_lin.clone()]);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            started(0, "beta.linux-amd64", &b_lin, 9001),
            started(0, "alpha", &a_arm, 9002),
            started(0, "beta", &b_arm, 9003),
            started(0, LIN_HOST, &a_lin, 9004),
        ],
    );
}

// ---- Which files are a host's record, and which record is valid (H24, H25) ----

/// H24. Each in turn: a `board-holder.alpha.linux-arm64.json`, a `board-holder.alpha.windows-arm64.json`, a
/// `board-holder.alpha.windows-amd64.extra.json` and a `board-holder.alpha.-amd64.json`, each
/// naming a running stub. None is a record: the keeper never reads, removes or stops any of them,
/// and no line names them.
#[test]
fn a_file_whose_name_is_no_hosts_record_is_left_alone() {
    for host in [
        "alpha.linux-arm64",
        "alpha.windows-arm64",
        "alpha.windows-amd64.extra",
        "alpha.-amd64",
    ] {
        let w = World::new();
        w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
        let x = win(1, FAR);
        w.set_leases(&[]);
        w.adopt_text(
            Some(host),
            500,
            &x,
            None,
            &World::record_text_with_platform(500, &x, WIN),
        );
        let ran = w.pass();
        assert_eq!(ran.exit, 0, "{host}");
        assert_lines(&w, &ran, vec![idle(0, None)]);
        assert!(w.is_alive(500), "{host}: The stub is left running");
        assert!(w.stopped_pids().is_empty(), "{host}");
        assert!(
            w.record_path(Some(host)).is_file(),
            "{host}: The file is left"
        );
    }
}

/// H25. A `board-holder.alpha.windows-amd64.json` whose `platform` is not `windows/amd64`, or
/// is absent, which is `linux/arm64`, naming a running stub, with `alpha`'s `windows/amd64` host
/// `ready` and a ready lease of it: the record is not valid, so it is removed and the stub is left
/// running, as for any record that fails Trusting the record, and the pass starts a holder
/// through the lease, whose record is then valid.
#[test]
fn a_record_whose_platform_is_not_its_files_platform_is_no_record() {
    for member in [
        Some("linux/amd64"),
        Some("linux/arm64"),
        None,
        Some("windows/arm64"),
    ] {
        let w = World::new();
        w.registry_hosts(&[("alpha", WIN, "ready")]);
        let x = win(1, FAR);
        w.set_leases(std::slice::from_ref(&x));
        let text = match member {
            Some(p) => World::record_text_with_platform(500, &x, p),
            None => World::record_text(500, &x),
        };
        w.adopt_text(Some(WIN_HOST), 500, &x, None, &text);
        w.plan(&x, World::hold(300, None));
        let ran = w.pass();
        assert_eq!(ran.exit, 0, "{member:?}");
        assert_lines(&w, &ran, vec![idle(0, None), started(0, WIN_HOST, &x, 300)]);
        assert!(
            w.is_alive(500),
            "{member:?}: The stub of the record that is no record is left running"
        );
        assert!(w.stopped_pids().is_empty(), "{member:?}");
        let rec = record_json(&w, Some(WIN_HOST));
        assert_eq!(
            rec["pid"], 300,
            "{member:?}: The record is the new holder's"
        );
        assert_eq!(rec["platform"], WIN, "{member:?}");
    }
}

/// The same for a `linux/arm64` host's file: a `board-holder.alpha.json` that holds a `platform`
/// other than `linux/arm64` is not valid, and so is no record. Its stub is left running, and the
/// pass starts a holder through the lease.
#[test]
fn a_linux_arm64_hosts_record_that_names_another_platform_is_no_record() {
    for member in ["windows/amd64", "linux/amd64"] {
        let w = World::new();
        w.registry_hosts(&[("alpha", ARM, "ready")]);
        let a = arm(1, FAR);
        w.set_leases(std::slice::from_ref(&a));
        w.adopt_text(
            Some("alpha"),
            500,
            &a,
            None,
            &World::record_text_with_platform(500, &a, member),
        );
        w.plan(&a, World::hold(200, None));
        let ran = w.pass();
        assert_eq!(ran.exit, 0, "{member}");
        assert_lines(&w, &ran, vec![idle(0, None), started(0, "alpha", &a, 200)]);
        assert!(w.is_alive(500), "{member}");
        assert!(w.stopped_pids().is_empty(), "{member}");
        let text = record_text_of(&w, Some("alpha"));
        assert!(
            !text.contains("platform"),
            "{member}: The new record is v0.11's: {text}"
        );
    }
}

/// The box's own `board-holder.json` is a `linux/arm64` host's file too: a `platform` in it that
/// is not `linux/arm64` makes it no record.
#[test]
fn the_boxs_record_that_names_another_platform_is_no_record() {
    let w = World::new();
    let s = Lease::new(BOX, 1, FAR);
    w.set_leases(std::slice::from_ref(&s));
    w.adopt_text(
        None,
        500,
        &s,
        None,
        &World::record_text_with_platform(500, &s, WIN),
    );
    w.plan(&s, World::hold(100, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(&w, &ran, vec![started(0, BOX, &s, 100)]);
    assert!(w.is_alive(500));
    assert!(w.stopped_pids().is_empty());
}

/// state.md, The holder record: an explicit `"platform":"linux/arm64"` is read as equal to an
/// absent one, so it is valid in the file of a `linux/arm64` host, the box's included, and its
/// holder is kept (pin P16). This guards against reading too strictly; it holds before v0.12 too.
#[test]
fn an_explicit_linux_arm64_platform_is_the_same_as_none() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready")]);
    let (s, a) = (Lease::new(BOX, 1, FAR), arm(2, FAR));
    w.set_leases(&[s.clone(), a.clone()]);
    w.adopt_text(
        None,
        100,
        &s,
        None,
        &World::record_text_with_platform(100, &s, ARM),
    );
    w.adopt_text(
        Some("alpha"),
        200,
        &a,
        None,
        &World::record_text_with_platform(200, &a, ARM),
    );
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![kept(0, BOX, &s, 100), kept(0, "alpha", &a, 200)],
    );
    assert_eq!(w.count_prefix("start:"), 0);
}

// ---- A record written before v0.12 is linux/arm64's ----

/// A record written before v0.12 has no `platform`, and is `linux/arm64`'s: with a registry that
/// holds `alpha`'s `windows/amd64` host beside its `linux/arm64` one, the box's and `alpha`'s
/// v0.11 records are read as their `linux/arm64` hosts' and kept, and the `windows/amd64` host,
/// which has no record, gets its own holder rather than any of them.
#[test]
fn a_record_written_before_v012_is_read_as_linux_arm64s_and_kept() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (s, a, x) = (Lease::new(BOX, 1, FAR), arm(2, FAR), win(3, FAR));
    w.set_leases(&[x.clone(), a.clone(), s.clone()]);
    w.adopt_text(None, 100, &s, None, &World::record_text(100, &s));
    w.adopt_text(Some("alpha"), 200, &a, None, &World::record_text(200, &a));
    w.plan(&x, World::hold(300, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, BOX, &s, 100),
            kept(0, "alpha", &a, 200),
            started(0, WIN_HOST, &x, 300),
        ],
    );
    assert!(w.stopped_pids().is_empty());
    assert_eq!(w.started_leases(), vec![x.id.clone()]);
    // v0.11's bytes are left as they were: a keeper never rewrites a record it keeps.
    assert_eq!(
        record_text_of(&w, Some("alpha")),
        World::record_text(200, &a)
    );
}

/// A record written before v0.12 is `linux/arm64`'s and never another platform's: the same
/// no-`platform` record in the file of `alpha`'s `windows/amd64` host is not a record (H25), but
/// in `alpha`'s `linux/arm64` file, with only a `windows/amd64` lease ready, it names a holder
/// whose lease is not on that host, and so it is stopped.
#[test]
fn a_record_written_before_v012_is_not_the_other_platforms_hosts_holder() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a, x) = (arm(1, FAR), win(2, FAR));
    // The linux/arm64 lease has ended; the windows/amd64 one is ready.
    w.set_lease_records(vec![a.released_json(), x.json()]);
    w.adopt_text(Some("alpha"), 200, &a, None, &World::record_text(200, &a));
    w.plan(&x, World::hold(300, None));
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            stopped(0, "alpha", &a, 200),
            started(0, WIN_HOST, &x, 300),
        ],
    );
    assert_eq!(w.stopped_pids(), vec![200]);
}

// ---- A holder's log is per host (H27) ----

const MIB: usize = 1_048_576;

fn write_sized(w: &World, name: &str, len: usize) {
    std::fs::write(w.state.join(name), vec![b'x'; len]).unwrap();
}

/// H27. `board-holder-ssh.alpha.windows-amd64.log` over 1 MiB is renamed to `.log.1`, and
/// `rotated lease=- alias=- pid=- host=alpha.windows-amd64` is logged after the `rotated` line of
/// any host whose host name is before it in ordinal order: `alpha` is, and
/// `beta.linux-amd64` and `swimcoach.windows-amd64` are not. A log whose name carries
/// `linux-arm64`, or a platform that is not supported, is no host's and is left, as is one of
/// exactly 1 MiB.
#[test]
fn each_hosts_holder_log_is_rotated_by_its_host_name_in_ordinal_order() {
    let w = World::new();
    write_sized(&w, "board-holder-ssh.swimcoach.windows-amd64.log", MIB + 1);
    write_sized(&w, "board-holder-ssh.beta.linux-amd64.log", MIB + 1);
    write_sized(&w, "board-holder-ssh.alpha.windows-amd64.log", MIB + 1);
    write_sized(&w, "board-holder-ssh.alpha.log", MIB + 1);
    write_sized(&w, "board-holder-ssh.alpha.linux-amd64.log", MIB);
    write_sized(&w, "board-holder-ssh.alpha.linux-arm64.log", MIB + 1);
    write_sized(&w, "board-holder-ssh.alpha.windows-arm64.log", MIB + 1);
    let ran = w.pass();
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            rotated(0, Some("alpha")),
            rotated(0, Some(WIN_HOST)),
            rotated(0, Some("beta.linux-amd64")),
            rotated(0, Some("swimcoach.windows-amd64")),
            idle(0, None),
        ],
    );
    for log in [
        "board-holder-ssh.alpha.log",
        "board-holder-ssh.alpha.windows-amd64.log",
        "board-holder-ssh.beta.linux-amd64.log",
        "board-holder-ssh.swimcoach.windows-amd64.log",
    ] {
        assert!(!w.state.join(log).exists(), "{log} was not renamed");
        assert_eq!(
            std::fs::metadata(w.state.join(format!("{log}.1")))
                .unwrap()
                .len() as usize,
            MIB + 1,
            "{log}"
        );
    }
    for log in [
        "board-holder-ssh.alpha.linux-amd64.log",
        "board-holder-ssh.alpha.linux-arm64.log",
        "board-holder-ssh.alpha.windows-arm64.log",
    ] {
        assert!(w.state.join(log).exists(), "{log} was rotated");
        assert!(!w.state.join(format!("{log}.1")).exists(), "{log}");
    }
}

// ---- A keeper: one host's failures, backoff and grace are its own (H28) ----

/// H28. A keeper tracking holders on `alpha`'s `linux/arm64` and `windows/amd64` hosts; the
/// `windows/amd64` one exits at once on every start, the other's stays alive. The failures,
/// backoffs and graces are the `windows/amd64` host's alone: its `backoff seconds=<n>
/// host=alpha.windows-amd64` lines grow with its count, while the `linux/arm64` host's holder is
/// kept every pass.
#[test]
fn one_platforms_failures_back_off_alone_while_the_other_platforms_holder_is_kept() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let (a, x) = (arm(1, FAR), win(2, FAR));
    w.set_leases(&[a.clone(), x.clone()]);
    w.plan_always(&a, World::hold(200, None));
    let why = exited_at_once(Some(WIN_HOST));
    w.plan_always(&x, World::fail(&why));
    let ran = w.watch(&["-MaxPasses", "5"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            // t0: alpha's start holds; the windows host's first start fails: count 1, backoff 30.
            idle(0, None),
            started(0, "alpha", &a, 200),
            failed_start(0, WIN_HOST, &x, &why),
            backoff(0, 30, Some(WIN_HOST)),
            // t30: the backoff has run; it fails again: count 2, backoff 60.
            idle(30, None),
            kept(30, "alpha", &a, 200),
            failed_start(30, WIN_HOST, &x, &why),
            backoff(30, 60, Some(WIN_HOST)),
            // t60: the windows host is in its backoff, so it has no line; the other is kept.
            idle(60, None),
            kept(60, "alpha", &a, 200),
            // t90: count 3, backoff 120.
            idle(90, None),
            kept(90, "alpha", &a, 200),
            failed_start(90, WIN_HOST, &x, &why),
            backoff(90, 120, Some(WIN_HOST)),
            // t120: still in the windows host's backoff.
            idle(120, None),
            kept(120, "alpha", &a, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 30, 30]);
    assert_eq!(w.count(&format!("start:{}", a.id)), 1);
    assert_eq!(w.count(&format!("start:{}", x.id)), 3);
    assert!(w.is_alive(200));
}

/// keeper.md, Watching, step 1 (pin P46): a look walks the hosts it tracks a holder on in the
/// order their turns took in the keeper's last pass, which is `hosts.json`'s file order, never the
/// ordinal order of their host names. `beta`'s `linux/amd64` host's record comes before `alpha`'s
/// `linux/arm64` host's, and both holders end before the look at t60: its grace lines come in that
/// file order, `beta.linux-amd64` before `alpha`, though `alpha` is first in ordinal order.
#[test]
fn a_look_walks_the_tracked_hosts_in_the_registrys_file_order() {
    let w = World::new();
    w.registry_hosts(&[("beta", LIN, "ready"), ("alpha", ARM, "ready")]);
    let (b, a) = (Lease::on(LIN, "beta", 1, FAR), arm(2, FAR));
    w.set_leases(&[a.clone(), b.clone()]);
    // Each holder lives 45 seconds, so the look at t60 finds both gone.
    w.plan(&b, World::hold(300, Some(secs(45))));
    w.plan(&a, World::hold(200, Some(secs(45))));
    let ran = w.watch(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            started(0, "beta.linux-amd64", &b, 300),
            started(0, "alpha", &a, 200),
            idle(30, None),
            kept(30, "beta.linux-amd64", &b, 300),
            kept(30, "alpha", &a, 200),
            // t60: the look finds both gone, and walks them in the registry's file order.
            grace(60, 95, Some("beta.linux-amd64")),
            grace(60, 95, Some("alpha")),
            idle(60, None),
        ],
    );
}

/// H23, in a keeper. The `windows/amd64` host's lease expires at t90 while the `linux/arm64`
/// host's stays ready: the pass stops the `windows/amd64` holder, `stopped ...
/// host=alpha.windows-amd64`, and the keeper logs `grace seconds=95 host=alpha.windows-amd64` for
/// that host alone. The `linux/arm64` holder is kept in every pass, and a new `windows/amd64`
/// lease that is ready meanwhile gets no start until that host's grace has run, then one at the
/// first pass after it.
#[test]
fn a_keeper_stops_one_platforms_holder_with_its_own_grace_and_keeps_the_other() {
    let w = World::new();
    w.registry_hosts(&[("alpha", ARM, "ready"), ("alpha", WIN, "ready")]);
    let a = arm(1, FAR);
    // The windows host's first lease expires at t90 (12:01:30); its second is ready from t120.
    let x1 = win(2, "2026-11-19T12:01:30Z");
    let x2 = win(3, FAR);
    w.set_leases(&[a.clone(), x1.clone()]);
    w.plan_always(&a, World::hold(200, None));
    w.plan(&x1, World::hold(300, None));
    w.plan(&x2, World::hold(301, None));
    let later = [a.clone(), x1.clone(), x2.clone()];
    w.on("lock:pass", 5, move |w| w.set_leases(&later));
    let ran = w.watch(&["-MaxPasses", "8"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            started(0, "alpha", &a, 200),
            started(0, WIN_HOST, &x1, 300),
            idle(30, None),
            kept(30, "alpha", &a, 200),
            kept(30, WIN_HOST, &x1, 300),
            idle(60, None),
            kept(60, "alpha", &a, 200),
            kept(60, WIN_HOST, &x1, 300),
            // t90: the pass time equals the windows lease's expiry, so it is not ready: stop, grace.
            idle(90, None),
            kept(90, "alpha", &a, 200),
            stopped(90, WIN_HOST, &x1, 300),
            grace(90, 95, Some(WIN_HOST)),
            // t120, t150, t180: the second lease is ready, but the windows host's grace runs to t185.
            idle(120, None),
            kept(120, "alpha", &a, 200),
            idle(150, None),
            kept(150, "alpha", &a, 200),
            idle(180, None),
            kept(180, "alpha", &a, 200),
            // t210: the first pass after the grace starts the windows host's holder.
            idle(210, None),
            kept(210, "alpha", &a, 200),
            started(210, WIN_HOST, &x2, 301),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30; 7]);
    assert_eq!(w.stopped_pids(), vec![300]);
    assert!(w.is_alive(200));
}

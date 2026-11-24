//! Place decides by the size asked for, the host's type and the project's other leases
//! (docs/driver.md, Where it places a lease, On a `ready` host and Sizes and instance types;
//! docs/decisions.md, D65, D66 and D71's pins P2, P20, P27, P31; docs/lease-api.md, Fields;
//! E0.09-T03-S01, #0203).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. Every start goes
//! through a fake `Detach`, so no test starts a process, and every file is in a throwaway folder.
//! Place makes no call to AWS: the only thing it can start is the run through that fake, and the
//! tests that expect no run assert that it made no start.
#![cfg(windows)]

mod common;
#[path = "common/pending.rs"]
mod pending;

use agentspace_driver_api::{Failure, Placed};
use agentspace_hosts::HostState;
use common::*;
use pending::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PROJECT: &str = "agentspace";

/// D65's four sizes, smallest first, and each one's type (driver.md, Sizes and instance types).
const SIZES: [&str; 4] = ["s", "m", "l", "xl"];
const TYPES: [&str; 4] = ["m9g.large", "m9g.xlarge", "m9g.2xlarge", "m9g.4xlarge"];

/// A home with a CA and the operator program installed, and a fake start that looks at the home
/// when it is made.
fn world() -> (Fx, Arc<FakeDetach>) {
    world_for(PROJECT)
}

fn world_for(project: &str) -> (Fx, Arc<FakeDetach>) {
    let mut fx = Fx::with_ca();
    fx.install_operator();
    let detach = FakeDetach::observing(&fx.home, project);
    (fx, detach)
}

/// One place of one lease, with what the disk held before it.
struct Run {
    id: String,
    project: String,
    size: String,
    got: Result<Placed, Failure>,
    detach: Arc<FakeDetach>,
    hosts_before: Option<Vec<u8>>,
    leases_before: Option<Vec<u8>>,
    ssh_before: Vec<String>,
    snapshot_before: Snapshot,
    host_before: Option<agentspace_hosts::Host>,
    t0: i64,
    t1: i64,
}

/// Places lease number `n` of `project` at `size`, over the files the test has seeded.
fn attempt(fx: &Fx, n: u32, project: &str, size: &str) -> Run {
    let id = id_n(n);
    let detach = FakeDetach::observing(&fx.home, project);
    let d = fx.driver_with(&detach);
    let hosts_before = fx.hosts_bytes();
    let leases_before = fx.leases_bytes();
    let ssh_before = fx.ssh_names();
    let snapshot_before = fx.snapshot();
    let host_before = hosts_before.as_ref().and_then(|_| fx.host(project));
    let t0 = now();
    let got = fx.place_sized(&d, project, &id, size);
    let t1 = now();
    Run {
        id,
        project: project.to_string(),
        size: size.to_string(),
        got,
        detach,
        hosts_before,
        leases_before,
        ssh_before,
        snapshot_before,
        host_before,
        t0,
        t1,
    }
}

impl Run {
    /// The lease was placed `ready` on the host, with its key lines, and nothing was started or
    /// written to the host registry or the lease state file.
    #[track_caller]
    fn assert_ready(&self, fx: &Fx, why: &str) {
        match &self.got {
            Ok(Placed::Ready {
                host_alias,
                user,
                expires_at,
            }) => {
                assert_eq!(host_alias, &alias_for(&self.project, &self.id), "{why}");
                assert_eq!(user, "ubuntu", "{why}");
                assert!(*expires_at > now(), "{why}");
            }
            other => panic!("{why}: expected ready, got {other:?}"),
        }
        assert_eq!(
            self.detach.count(),
            0,
            "{why}: A ready answer starts nothing"
        );
        assert_eq!(
            fx.hosts_bytes(),
            self.hosts_before,
            "{why}: A ready answer writes nothing to the host registry"
        );
        assert_eq!(
            fx.leases_bytes(),
            self.leases_before,
            "{why}: Place never writes the lease state file"
        );
        assert_eq!(
            fx.lease_files(&self.id),
            ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"],
            "{why}"
        );
        assert_alias_bytes(fx, &self.id, &self.project, self.project != "swimcoach");
    }

    /// The lease is `pending`, one run was started as driver.md says, and the registry was written
    /// as a started run writes it: the record's `last_attempt_at`, its state as `state` says, and
    /// nothing else of it.
    #[track_caller]
    fn assert_pending_started(&self, fx: &Fx, why: &str, state: HostState) {
        assert!(
            matches!(self.got, Ok(Placed::Pending)),
            "{why}: expected pending, got {:?}",
            self.got
        );
        let start = self.detach.only();
        assert_eq!(
            start.args,
            ["provision", "-Host", self.project.as_str()],
            "{why}: The run's command line carries no type (driver.md, A resizing run)"
        );
        assert_eq!(start.program, fx.ctx.operator_program.clone().unwrap());
        assert_run_env(&start, &fx.ctx);
        assert!(
            start.seen.hosts_lock_free,
            "{why}: hosts.lock is let go first"
        );
        assert!(
            !start.seen.provision_log_exists,
            "{why}: The driver never makes provision.log"
        );
        let after = fx.host(&self.project).expect("The project has a record");
        assert_eq!(after.state, state, "{why}");
        let at = epoch_of(after.last_attempt_at.as_deref().expect("last_attempt_at"));
        assert!(
            self.t0 <= at && at <= self.t1,
            "{why}: last_attempt_at is now"
        );
        if let Some(before) = &self.host_before {
            assert_eq!(
                after.instance_type, before.instance_type,
                "{why}: Place never changes a host's type"
            );
            assert_eq!(after.instance_id, before.instance_id, "{why}");
            assert_eq!(after.stack, before.stack, "{why}");
        }
        assert_eq!(
            fx.leases_bytes(),
            self.leases_before,
            "{why}: Place never writes the lease state file"
        );
        assert!(
            !fx.has_files_of(&self.project, &self.id),
            "{why}: A pending answer issues nothing"
        );
        assert_eq!(fx.ssh_names(), self.ssh_before, "{why}");
    }

    /// The lease is `pending`, no run was started, and the registry was not written.
    #[track_caller]
    fn assert_pending_not_started(&self, fx: &Fx, why: &str) {
        assert!(
            matches!(self.got, Ok(Placed::Pending)),
            "{why}: expected pending, got {:?}",
            self.got
        );
        assert_eq!(self.detach.count(), 0, "{why}: No run is due");
        assert_eq!(fx.hosts_bytes(), self.hosts_before, "{why}");
        assert_eq!(fx.leases_bytes(), self.leases_before, "{why}");
        assert!(!fx.has_files_of(&self.project, &self.id), "{why}");
    }

    /// The lease is `unsatisfiable` with the one reason `no_capacity` and D66's message, and
    /// nothing at all was written: no lease folder, no alias, no registry write and no run.
    #[track_caller]
    fn assert_no_capacity(&self, fx: &Fx, why: &str) {
        match &self.got {
            Ok(Placed::Unsatisfiable { reasons }) => {
                assert_eq!(reasons.len(), 1, "{why}: Exactly one reason");
                assert_eq!(reasons[0].code, "no_capacity", "{why}");
                assert_eq!(
                    reasons[0].message,
                    no_capacity_at_size(&self.project, &self.size),
                    "{why}"
                );
                fx.assert_plain(&reasons[0].message);
            }
            other => panic!("{why}: expected unsatisfiable, got {other:?}"),
        }
        assert_eq!(self.detach.count(), 0, "{why}: A refusal starts no run");
        fx.assert_unchanged(
            &self.snapshot_before,
            &format!("{why}: A refusal changes nothing on the disk"),
        );
    }
}

fn other_ready(n: u32) -> L {
    L::ready(&id_n(n), PROJECT)
}

// ---- D66's table, every row ---------------------------------------------------------------------

#[test]
fn every_row_of_d66s_table_by_the_hosts_type_the_size_asked_for_and_another_live_ready_lease() {
    // The host's type against the size asked for, with and without another live `ready` lease of
    // the project: exactly (ready, whatever holds the host), larger (ready when another holds it,
    // otherwise pending and a run), smaller (no_capacity when another holds it, otherwise pending
    // and a run).
    let (fx, _) = world();
    let mut n = 0;
    for (ti, ty) in TYPES.iter().enumerate() {
        for (si, size) in SIZES.iter().enumerate() {
            for other in [false, true] {
                n += 1;
                fx.write_hosts(&[H::ready(PROJECT).typed(ty)]);
                if other {
                    fx.write_leases(&[other_ready(5000 + n)]);
                } else {
                    fx.write_leases(&[]);
                }
                let run = attempt(&fx, n, PROJECT, size);
                let why = format!("host {ty}, size {size}, another live ready lease {other}");
                match (ti.cmp(&si), other) {
                    (std::cmp::Ordering::Equal, _) => run.assert_ready(&fx, &why),
                    (std::cmp::Ordering::Greater, true) => run.assert_ready(&fx, &why),
                    (std::cmp::Ordering::Greater, false) | (std::cmp::Ordering::Less, false) => {
                        run.assert_pending_started(&fx, &why, HostState::Ready)
                    }
                    (std::cmp::Ordering::Less, true) => run.assert_no_capacity(&fx, &why),
                }
            }
        }
    }
    assert_eq!(n, 32);
}

#[test]
fn a_ready_host_at_exactly_the_size_is_ready_whatever_holds_it_and_whether_or_not_a_run_holds_the_lock()
 {
    // Row 3. Another live ready lease, a pending lease of another project, and a held
    // provision.lock change nothing.
    let (fx, _) = world();
    for (i, size) in SIZES.iter().enumerate() {
        let n = 10 + i as u32;
        fx.write_hosts(&[H::ready(PROJECT).typed(TYPES[i]).attempted_ago(30)]);
        fx.write_leases(&[
            other_ready(6000 + n),
            L::pending(&id_n(7000 + n), "other-project").size("xl"),
        ]);
        let run_lock = agentspace_hosts::try_run_lock(&fx.home, PROJECT)
            .unwrap()
            .expect("The run lock is free");
        let run = attempt(&fx, n, PROJECT, size);
        run.assert_ready(&fx, &format!("size {size}, run lock held"));
        drop(run_lock);
    }
}

#[test]
fn a_larger_host_with_another_live_ready_lease_is_placed_with_the_hosts_own_key_lines() {
    let (fx, _) = world();
    fx.write_hosts(&[H::ready(PROJECT).typed("m9g.2xlarge")]);
    fx.write_leases(&[other_ready(5001)]);
    let run = attempt(&fx, 1, PROJECT, "m");
    run.assert_ready(&fx, "l host, m asked for, another live ready lease");
}

#[test]
fn a_larger_host_with_no_other_live_ready_lease_is_pending_and_starts_a_resizing_run() {
    for (i, size) in ["s", "m", "l"].iter().enumerate() {
        let (fx, _) = world();
        fx.write_hosts(&[H::ready(PROJECT).typed("m9g.4xlarge").instance(INSTANCE)]);
        fx.write_leases(&[]);
        let run = attempt(&fx, 20 + i as u32, PROJECT, size);
        run.assert_pending_started(&fx, size, HostState::Ready);
        assert_eq!(
            fx.host(PROJECT).unwrap().instance_type.as_deref(),
            Some("m9g.4xlarge"),
            "Place never changes the type: the run does"
        );
    }
}

#[test]
fn a_smaller_host_with_no_other_live_ready_lease_is_pending_and_starts_a_resizing_run() {
    for (i, size) in ["m", "l", "xl"].iter().enumerate() {
        let (fx, _) = world();
        fx.write_hosts(&[H::ready(PROJECT).typed("m9g.large")]);
        let run = attempt(&fx, 30 + i as u32, PROJECT, size);
        run.assert_pending_started(&fx, size, HostState::Ready);
    }
}

// ---- The refusal --------------------------------------------------------------------------------------

#[test]
fn the_no_capacity_message_is_byte_for_byte_and_names_the_project_and_the_size() {
    // No CA: an unsatisfiable answer runs no CA check, so it still answers. The expected text is
    // written out here and not built, so a change to the builder cannot hide a change to the
    // message.
    let mut fx = Fx::without_ca();
    fx.install_operator();
    fx.write_hosts(&[
        H::ready("agentspace").typed("m9g.large"),
        H::ready("my-project").typed("m9g.xlarge"),
    ]);
    fx.write_leases(&[
        L::ready(&id_n(5001), "agentspace"),
        L::ready(&id_n(5002), "my-project"),
    ]);
    let want_xl = "No capacity can be found for project 'agentspace' at size 'xl': its host is serving other leases at a smaller size. Release them, or ask for a size they fit.";
    let want_l = "No capacity can be found for project 'my-project' at size 'l': its host is serving other leases at a smaller size. Release them, or ask for a size they fit.";
    for (project, size, want) in [("agentspace", "xl", want_xl), ("my-project", "l", want_l)] {
        let before = fx.snapshot();
        let detach = FakeDetach::new();
        let d = fx.driver_with(&detach);
        match fx.place_sized(&d, project, &id_n(1), size) {
            Ok(Placed::Unsatisfiable { reasons }) => {
                assert_eq!(reasons.len(), 1, "Exactly one reason");
                assert_eq!(reasons[0].code, "no_capacity");
                assert_eq!(reasons[0].message, want);
                fx.assert_plain(&reasons[0].message);
            }
            other => panic!("Expected unsatisfiable: {other:?}"),
        }
        assert_eq!(detach.count(), 0);
        fx.assert_unchanged(&before, "Nothing was written");
        assert!(!fx.ca_dir().exists(), "No CA was made");
    }
}

#[test]
fn a_refusal_changes_nothing_and_a_later_create_is_judged_afresh() {
    let (fx, _) = world();
    fx.write_hosts(&[H::ready(PROJECT).typed("m9g.large")]);
    fx.write_leases(&[other_ready(5001)]);
    attempt(&fx, 1, PROJECT, "xl").assert_no_capacity(&fx, "While the other lease holds the host");
    // The same request again is refused again: nothing was recorded.
    attempt(&fx, 2, PROJECT, "xl").assert_no_capacity(&fx, "Again");
    // A size the host meets is placed, though the other lease still holds it.
    attempt(&fx, 3, PROJECT, "s").assert_ready(&fx, "A size the host meets");
    // Once the other lease is released, the same request is pending and starts a run.
    fx.write_leases(&[L::released_after_ready(&id_n(5001), PROJECT)]);
    attempt(&fx, 4, PROJECT, "xl").assert_pending_started(
        &fx,
        "After the other lease's release",
        HostState::Ready,
    );
}

#[test]
fn only_a_live_ready_lease_of_the_project_holds_a_host() {
    // Host m9g.large, size m asked for: smaller. Only a live ready lease of the project makes it
    // no_capacity; every other record leaves it pending with a run. An expired, a released and an
    // unsatisfiable lease hold nothing, and neither does a pending lease that a run would not
    // place (another driver's), nor another project's lease.
    let holders: Vec<(&str, Vec<L>, bool)> = vec![
        ("a live ready lease", vec![other_ready(5001)], true),
        (
            "an expired lease",
            vec![L::expired(&id_n(5001), PROJECT)],
            false,
        ),
        (
            "a released lease",
            vec![L::released_after_ready(&id_n(5001), PROJECT)],
            false,
        ),
        (
            "a lease released while pending",
            vec![L::released(&id_n(5001), PROJECT)],
            false,
        ),
        (
            "an unsatisfiable lease",
            vec![L::unsatisfiable(&id_n(5001), PROJECT)],
            false,
        ),
        (
            "a pending lease of another driver",
            vec![L::pending(&id_n(5001), PROJECT).driver(Some("fake"))],
            false,
        ),
        (
            "another project's live ready lease",
            vec![L::ready(&id_n(5001), "other-project")],
            false,
        ),
        ("no lease", vec![], false),
    ];
    let (fx, _) = world();
    for (i, (what, leases, holds)) in holders.iter().enumerate() {
        fx.write_hosts(&[H::ready(PROJECT).typed("m9g.large")]);
        fx.write_leases(leases);
        let run = attempt(&fx, 100 + i as u32, PROJECT, "m");
        let why = format!("host smaller than m, {what}");
        if *holds {
            run.assert_no_capacity(&fx, &why);
        } else {
            run.assert_pending_started(&fx, &why, HostState::Ready);
        }
    }
    // The same on a larger host, size s asked for: only a live ready lease makes it ready.
    for (i, (what, leases, holds)) in holders.iter().enumerate() {
        fx.write_hosts(&[H::ready(PROJECT).typed("m9g.xlarge")]);
        fx.write_leases(leases);
        let run = attempt(&fx, 200 + i as u32, PROJECT, "s");
        let why = format!("host larger than s, {what}");
        if *holds {
            run.assert_ready(&fx, &why);
        } else {
            run.assert_pending_started(&fx, &why, HostState::Ready);
        }
    }
}

#[test]
fn the_expiry_of_a_ready_lease_decides_whether_it_holds_the_host() {
    // A lease stored ready is live while its expires_at is after the reading's time. One that
    // expired a minute ago holds nothing; one that expires in an hour holds the host.
    let (fx, _) = world();
    fx.write_hosts(&[H::ready(PROJECT).typed("m9g.large")]);
    fx.write_leases(&[L::expired(&id_n(5001), PROJECT)]);
    attempt(&fx, 1, PROJECT, "l").assert_pending_started(&fx, "Expired", HostState::Ready);
    fx.write_leases(&[L::expired(&id_n(5001), PROJECT), other_ready(5002)]);
    attempt(&fx, 2, PROJECT, "l").assert_no_capacity(&fx, "One expired, one live");
}

#[test]
fn a_refusal_needs_no_ca_but_a_pending_answer_checks_it_first() {
    // Pending, by row 1, row 2 and row 6, fails with the no-CA wording and writes and starts
    // nothing. An unsatisfiable answer needs no CA.
    for what in ["row 1", "row 2", "row 6"] {
        let mut fx = Fx::without_ca();
        fx.install_operator();
        match what {
            "row 1" => {
                fx.write_hosts(&[H::ready(PROJECT)]);
                fx.write_leases(&[L::pending(&id_n(5001), PROJECT).size("l")]);
            }
            "row 2" => fx.write_hosts(&[H::ready_untyped(PROJECT)]),
            _ => fx.write_hosts(&[H::ready(PROJECT).typed("m9g.2xlarge")]),
        }
        let before = fx.snapshot();
        let detach = FakeDetach::new();
        let d = fx.driver_with(&detach);
        let f = fx
            .place_sized(&d, PROJECT, &id_n(1), "m")
            .expect_err("No CA fails a pending place");
        assert_eq!(f.message, NO_CA, "{what}");
        assert_eq!(detach.count(), 0, "{what}");
        fx.assert_unchanged(&before, &format!("{what}: Nothing was written"));
    }
}

// ---- Row 1: a pending lease a run would place ---------------------------------------------------------

#[test]
fn while_the_project_has_a_pending_lease_a_run_would_place_every_new_lease_is_pending() {
    // Each case is one that the table alone would answer ready or no_capacity, and a pending ec2
    // lease with a size in the four holds it pending: P20.
    let cases: [(&str, &str, bool); 6] = [
        ("m", "l", false),
        ("m", "l", true),
        ("s", "l", true),
        ("xl", "l", true),
        ("m", "s", false),
        ("l", "xl", true),
    ];
    for (i, (size, lease_size, other)) in cases.iter().enumerate() {
        let (fx, _) = world();
        fx.write_hosts(&[H::ready(PROJECT)]);
        let mut leases = vec![L::pending(&id_n(5001), PROJECT).size(lease_size)];
        if *other {
            leases.push(other_ready(5002));
        }
        fx.write_leases(&leases);
        let run = attempt(&fx, 300 + i as u32, PROJECT, size);
        run.assert_pending_started(
            &fx,
            &format!("size {size}, a pending {lease_size} lease, another ready lease {other}"),
            HostState::Ready,
        );
    }
}

#[test]
fn a_pending_lease_a_run_would_not_place_does_not_hold_the_projects_leases_pending() {
    // A hand-edited record: another driver, no driver, a size outside the four, or another
    // project's lease. None of them holds a new lease pending, so the table decides.
    let (fx, _) = world();
    let hand_edited: Vec<(&str, L)> = vec![
        (
            "another driver",
            L::pending(&id_n(5001), PROJECT).driver(Some("fake")),
        ),
        ("no driver", L::pending(&id_n(5001), PROJECT).driver(None)),
        (
            "a size outside the four",
            L::pending(&id_n(5001), PROJECT).size("xxl"),
        ),
        (
            "another project",
            L::pending(&id_n(5001), "other-project").size("l"),
        ),
    ];
    for (i, (what, lease)) in hand_edited.iter().enumerate() {
        let n = 400 + 10 * i as u32;
        // Exactly: ready.
        fx.write_hosts(&[H::ready(PROJECT)]);
        fx.write_leases(std::slice::from_ref(lease));
        attempt(&fx, n, PROJECT, "m").assert_ready(&fx, what);
        // Larger with another live ready lease: ready.
        fx.write_leases(&[lease.clone(), other_ready(5002)]);
        attempt(&fx, n + 1, PROJECT, "s").assert_ready(&fx, what);
        // Smaller with another live ready lease: no_capacity, not pending.
        attempt(&fx, n + 2, PROJECT, "xl").assert_no_capacity(&fx, what);
    }
}

#[test]
fn a_run_that_is_not_due_or_is_running_starts_nothing_for_rows_1_2_and_6() {
    // The 5-minute rule and the held provision.lock, as every pending answer of place has them
    // (P27). The registry is not written and no run is started.
    let rows: [(&str, H, Vec<L>); 3] = [
        (
            "row 1",
            H::ready(PROJECT),
            vec![L::pending(&id_n(5001), PROJECT).size("l")],
        ),
        ("row 2", H::ready_untyped(PROJECT), vec![]),
        ("row 6", H::ready(PROJECT).typed("m9g.2xlarge"), vec![]),
    ];
    for (what, host, leases) in rows {
        let (fx, _) = world();
        // Under 5 minutes old.
        fx.write_hosts(&[host.clone().attempted_ago(120)]);
        fx.write_leases(&leases);
        attempt(&fx, 1, PROJECT, "m").assert_pending_not_started(&fx, &format!("{what}, recent"));
        // Exactly 5 minutes old is due.
        fx.write_hosts(&[host.clone().attempted_ago(300)]);
        attempt(&fx, 2, PROJECT, "m").assert_pending_started(
            &fx,
            &format!("{what}, 5 minutes"),
            HostState::Ready,
        );
        // Due, but a run holds provision.lock.
        fx.write_hosts(&[host.clone().attempted_ago(3600)]);
        let run_lock = agentspace_hosts::try_run_lock(&fx.home, PROJECT)
            .unwrap()
            .expect("The run lock is free");
        attempt(&fx, 3, PROJECT, "m")
            .assert_pending_not_started(&fx, &format!("{what}, a run is running"));
        drop(run_lock);
        // Let go, the same place starts a run.
        attempt(&fx, 4, PROJECT, "m").assert_pending_started(
            &fx,
            &format!("{what}, after the run"),
            HostState::Ready,
        );
        // The driver does not keep provision.lock.
        assert!(
            agentspace_hosts::try_run_lock(&fx.home, PROJECT)
                .unwrap()
                .is_some(),
            "{what}: The driver kept provision.lock"
        );
    }
}

// ---- Row 2: a record with no instance_type ------------------------------------------------------------

#[test]
fn a_ready_record_with_no_instance_type_is_pending_and_starts_a_run_never_no_capacity() {
    // Whatever size is asked for, and whether or not another live ready lease holds the host
    // (P2): the run records the type and places or refuses the lease by it.
    let (fx, _) = world();
    let mut n = 500;
    for size in SIZES {
        for other in [false, true] {
            n += 1;
            fx.write_hosts(&[H::ready_untyped(PROJECT).instance(INSTANCE)]);
            if other {
                fx.write_leases(&[other_ready(5001)]);
            } else {
                fx.write_leases(&[]);
            }
            let run = attempt(&fx, n, PROJECT, size);
            run.assert_pending_started(
                &fx,
                &format!("No type, size {size}, another live ready lease {other}"),
                HostState::Ready,
            );
            assert_eq!(
                fx.host(PROJECT).unwrap().instance_type,
                None,
                "Place does not write a type"
            );
        }
    }
}

#[test]
fn a_ready_record_with_a_type_the_map_does_not_hold_is_pending_and_starts_a_run() {
    // A well-formed type outside the map meets no size (P14): a hand edit only.
    let (fx, _) = world();
    let mut n = 600;
    for ty in ["m9g.8xlarge", "c7g.large", "m9g.metal"] {
        for (size, other) in [("s", true), ("m", false), ("xl", true), ("l", false)] {
            n += 1;
            fx.write_hosts(&[H::ready(PROJECT).typed(ty)]);
            fx.write_leases(&if other {
                vec![other_ready(5001)]
            } else {
                vec![]
            });
            attempt(&fx, n, PROJECT, size).assert_pending_started(
                &fx,
                &format!("Type {ty}, size {size}, another live ready lease {other}"),
                HostState::Ready,
            );
        }
    }
}

// ---- No hosts.json: the box's implied record ----------------------------------------------------------

#[test]
fn with_no_registry_the_box_is_a_record_at_m9g_xlarge_that_is_read_and_never_written() {
    let (fx, _) = world_for("swimcoach");
    assert!(fx.hosts_bytes().is_none());

    // A default lease is ready, as before v0.09, and the registry stays missing.
    fx.write_leases(&[]);
    let run = attempt(&fx, 1, "swimcoach", "m");
    run.assert_ready(&fx, "default size, no registry");
    assert!(
        fx.hosts_bytes().is_none(),
        "A missing hosts.json stays missing"
    );

    // A larger host with another live ready lease of swimcoach: ready.
    fx.write_leases(&[L::ready(&id_n(5001), "swimcoach")]);
    attempt(&fx, 2, "swimcoach", "s").assert_ready(&fx, "s on the box with another lease");
    assert!(fx.hosts_bytes().is_none());

    // Smaller, with another live ready lease: no_capacity, and the registry stays missing.
    attempt(&fx, 3, "swimcoach", "xl").assert_no_capacity(&fx, "xl on the box with another lease");
    attempt(&fx, 4, "swimcoach", "l").assert_no_capacity(&fx, "l on the box with another lease");
    assert!(fx.hosts_bytes().is_none());
}

#[test]
fn with_no_registry_and_no_other_lease_a_size_other_than_m_is_pending_and_starts_a_run() {
    for (i, size) in ["s", "l", "xl"].iter().enumerate() {
        let (fx, _) = world_for("swimcoach");
        fx.write_leases(&[]);
        let run = attempt(&fx, 10 + i as u32, "swimcoach", size);
        assert!(
            matches!(run.got, Ok(Placed::Pending)),
            "{size}: {:?}",
            run.got
        );
        let start = run.detach.only();
        assert_eq!(start.args, ["provision", "-Host", "swimcoach"], "{size}");
        // The write from a missing file is the seed with last_attempt_at, and never the implied
        // type (P31).
        let box_record = fx.host("swimcoach").expect("The box's record");
        assert_eq!(box_record.state, HostState::Ready, "{size}");
        assert_eq!(box_record.instance_type, None, "{size}: P31");
        let at = epoch_of(
            box_record
                .last_attempt_at
                .as_deref()
                .expect("last_attempt_at"),
        );
        assert!(run.t0 <= at && at <= run.t1, "{size}");
        assert!(!fx.has_files_of("swimcoach", &run.id), "{size}");
    }
}

#[test]
fn with_no_registry_a_pending_lease_a_run_would_place_holds_the_boxs_new_leases_pending() {
    let (fx, _) = world_for("swimcoach");
    fx.write_leases(&[L::pending(&id_n(5001), "swimcoach").size("l")]);
    let run = attempt(&fx, 1, "swimcoach", "m");
    assert!(matches!(run.got, Ok(Placed::Pending)), "{:?}", run.got);
    assert_eq!(run.detach.only().args, ["provision", "-Host", "swimcoach"]);
}

#[test]
fn a_write_from_a_missing_registry_has_no_type_so_the_boxs_next_lease_is_pending_once() {
    // P31: the first create of a listed project with no record writes the registry, and the seed
    // it writes has no instance_type for the box. The box's next lease then answers pending by
    // row 2, and starts a run.
    let (fx, detach) = world_for(PROJECT);
    fx.write_projects(&[PROJECT]);
    fx.write_leases(&[]);
    let d = fx.driver_with(&detach);
    let got = fx.place_sized(&d, PROJECT, &id_n(1), "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(detach.count(), 1);
    let on_disk = String::from_utf8(fx.hosts_bytes().expect("The registry was written")).unwrap();
    assert!(
        !on_disk.contains("instance_type"),
        "The seed a write puts on disk has no type, the box's included"
    );
    assert_eq!(fx.host("swimcoach").unwrap().instance_type, None);

    let detach_box = FakeDetach::observing(&fx.home, "swimcoach");
    let d = fx.driver_with(&detach_box);
    let got = fx.place_sized(&d, "swimcoach", &id_n(2), "m");
    assert!(matches!(got, Ok(Placed::Pending)), "{got:?}");
    assert_eq!(
        detach_box.only().args,
        ["provision", "-Host", "swimcoach"],
        "The box's record has no type, so a default lease costs one run"
    );
    assert!(!fx.has_files_of("swimcoach", &id_n(2)));
}

// ---- creating, failed and no record: D48 and D54 as before ---------------------------------------------

#[test]
fn a_creating_or_failed_record_is_pending_for_every_size_whatever_the_leases_hold() {
    let mut n = 700;
    for state in ["creating", "failed"] {
        for size in SIZES {
            for other in [false, true] {
                n += 1;
                let (fx, _) = world();
                fx.write_hosts(&[if state == "creating" {
                    H::creating(PROJECT)
                } else {
                    H::failed(PROJECT)
                }
                .typed("m9g.large")]);
                fx.write_leases(&if other {
                    vec![
                        other_ready(5001),
                        L::pending(&id_n(5002), PROJECT).size("xl"),
                    ]
                } else {
                    vec![]
                });
                attempt(&fx, n, PROJECT, size).assert_pending_started(
                    &fx,
                    &format!("{state}, size {size}, other {other}"),
                    HostState::Creating,
                );
            }
        }
    }
}

#[test]
fn a_creating_record_does_not_read_the_lease_state_file() {
    // The lease state file is read only for a ready record.
    let (fx, _) = world();
    fx.write_hosts(&[H::creating(PROJECT)]);
    std::fs::create_dir_all(&fx.ctx.state_dir).unwrap();
    std::fs::write(fx.ctx.state_dir.join("leases.json"), b"{").unwrap();
    attempt(&fx, 1, PROJECT, "l").assert_pending_started(&fx, "creating", HostState::Creating);
}

#[test]
fn a_project_with_no_record_is_pending_when_listed_and_no_capacity_when_not_for_every_size() {
    for (i, size) in SIZES.iter().enumerate() {
        let (fx, detach) = world();
        fx.write_projects(&[PROJECT]);
        let d = fx.driver_with(&detach);
        let got = fx.place_sized(&d, PROJECT, &id_n(800 + i as u32), size);
        assert!(matches!(got, Ok(Placed::Pending)), "{size}: {got:?}");
        assert_eq!(detach.only().args, ["provision", "-Host", PROJECT]);
        assert_eq!(fx.host(PROJECT).unwrap().state, HostState::Creating);

        let (fx, detach) = world_for("unlisted");
        let d = fx.driver_with(&detach);
        match fx.place_sized(&d, "unlisted", &id_n(900 + i as u32), size) {
            Ok(Placed::Unsatisfiable { reasons }) => {
                assert_eq!(reasons.len(), 1);
                assert_eq!(reasons[0].code, "no_capacity");
                assert_eq!(
                    reasons[0].message,
                    "No capacity can be found for project 'unlisted': it has no host and is not on agentspace's project list.",
                    "{size}: v0.07's message, not D66's"
                );
            }
            other => panic!("{size}: expected unsatisfiable: {other:?}"),
        }
        assert_eq!(detach.count(), 0);
        assert!(fx.hosts_bytes().is_none());
    }
}

// ---- The lease state file -----------------------------------------------------------------------------

#[test]
fn a_lease_state_file_that_is_not_valid_fails_the_place_of_a_ready_record_and_writes_nothing() {
    // Read for every supported request on a ready record, so for the exactly row too.
    for (i, size) in ["s", "m", "xl"].iter().enumerate() {
        let (fx, _) = world();
        fx.write_hosts(&[H::ready(PROJECT)]);
        std::fs::create_dir_all(&fx.ctx.state_dir).unwrap();
        std::fs::write(fx.ctx.state_dir.join("leases.json"), b"{").unwrap();
        let run = attempt(&fx, 1000 + i as u32, PROJECT, size);
        let f = run
            .got
            .as_ref()
            .expect_err("An unreadable lease state file fails the place");
        assert!(
            f.message
                .starts_with("The lease's host could not be looked up: "),
            "{size}: {}",
            f.message
        );
        assert!(
            f.message.ends_with(". Nothing was recorded."),
            "{size}: {}",
            f.message
        );
        assert!(
            f.message
                .contains("agentspace's lease records could not be read"),
            "{size}: {}",
            f.message
        );
        fx.assert_failure_plain(f);
        assert!(f.leftovers.is_empty(), "{size}");
        assert_eq!(run.detach.count(), 0, "{size}");
        fx.assert_unchanged(&run.snapshot_before, &format!("{size}: Nothing is written"));
    }
}

#[test]
fn a_missing_lease_state_file_means_no_leases() {
    let (fx, _) = world();
    fx.write_hosts(&[H::ready(PROJECT)]);
    assert!(fx.leases_bytes().is_none());
    attempt(&fx, 1, PROJECT, "m").assert_ready(&fx, "exactly");
    attempt(&fx, 2, PROJECT, "s").assert_pending_started(
        &fx,
        "larger, no other lease",
        HostState::Ready,
    );
    assert!(fx.leases_bytes().is_none(), "Place never creates the file");
}

#[test]
fn an_unsupported_size_is_answered_without_reading_the_registry_or_the_lease_state_file() {
    let (fx, detach) = world();
    fx.write_hosts_raw(b"not json");
    std::fs::create_dir_all(&fx.ctx.state_dir).unwrap();
    std::fs::write(fx.ctx.state_dir.join("leases.json"), b"{").unwrap();
    let before = fx.snapshot();
    let d = fx.driver_with(&detach);
    match fx.place_sized(&d, PROJECT, &id_n(1), "xxl") {
        Ok(Placed::Unsatisfiable { reasons }) => {
            assert_eq!(reasons.len(), 1);
            assert_eq!(reasons[0].code, "unsupported_size");
            assert_eq!(
                reasons[0].message,
                "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl."
            );
        }
        other => panic!("Expected unsatisfiable: {other:?}"),
    }
    assert_eq!(detach.count(), 0);
    fx.assert_unchanged(&before, "An unsupported size reads and writes nothing");
}

#[test]
fn place_reads_the_lease_state_file_under_its_callers_lock_and_never_takes_the_lock_itself() {
    // The CLI holds the lease state lock for the whole place. The driver reads the file in that
    // hold, so it MUST NOT try to take the lock, which its own process holds.
    let (fx, _) = world();
    fx.write_hosts(&[H::ready(PROJECT).typed("m9g.2xlarge")]);
    fx.write_leases(&[other_ready(5001)]);
    let _lock = hold_lease_lock(&fx.ctx.state_dir);
    let begun = Instant::now();
    let run = attempt(&fx, 1, PROJECT, "m");
    assert!(
        begun.elapsed() < Duration::from_secs(10),
        "Place waited for a lock its caller holds"
    );
    run.assert_ready(
        &fx,
        "larger host, another live ready lease, caller holds the lock",
    );
    let run = attempt(&fx, 2, PROJECT, "l");
    run.assert_ready(&fx, "exactly, caller holds the lock");
}

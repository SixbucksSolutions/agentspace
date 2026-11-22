//! The guard's decision for each host (docs/operator.md, guard, steps 6 and 7; D56, D57; docs/state.md,
//! Live leases and the lease horizon), tested through `agentspace_guard::decide::decide` alone.
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing. Every test below
//! fails if the rule it names is broken. All IDs, names and times are invented.

use std::cell::RefCell;

use agentspace_box::{Described, DescribedState};
use agentspace_guard::GRACE_SECONDS;
use agentspace_guard::decide::{Action, HostToDecide, ProvisionLock, decide};
use agentspace_state::{Record, format_time};

/// The run's time in the tests: 2026-11-22T09:45:00Z, to the second.
const NOW: i64 = 1_795_340_700;
/// The project of the host under test.
const PROJECT: &str = "alpha";
/// The instance ID of the host under test.
const ID: &str = "i-0123456789abcdef0";
/// A launch time long before any grace of the tests, so a host with it is long past its grace.
const OLD_LAUNCH: i64 = NOW - 100_000;

// ---------------------------------------------------------------------------------------------
// Builders.
// ---------------------------------------------------------------------------------------------

/// A lease record of `project` in the stored `state`, with the given `expires_at` and
/// `released_at` in Unix seconds.
fn lease(project: &str, state: &str, expires: Option<i64>, released: Option<i64>) -> Record {
    Record {
        id: "00000000-0000-4000-8000-000000000000".to_string(),
        project: project.to_string(),
        state: state.to_string(),
        platform: "linux-arm64".to_string(),
        profile: "default".to_string(),
        size: "small".to_string(),
        ttl_seconds: 3600,
        created_at: format_time(NOW - 200_000),
        expires_at: expires.map(format_time),
        released_at: released.map(format_time),
        ssh: None,
        reasons: None,
        driver: None,
    }
}

/// A stored `ready` lease of `PROJECT` expiring at `expires`.
fn ready(expires: i64) -> Record {
    lease(PROJECT, "ready", Some(expires), None)
}

/// A stored `pending` lease of `PROJECT`.
fn pending() -> Record {
    lease(PROJECT, "pending", None, None)
}

/// A `released` lease of `PROJECT` that was once `ready`.
fn released(expires: i64, released_at: i64) -> Record {
    lease(PROJECT, "released", Some(expires), Some(released_at))
}

/// A `released` lease of `PROJECT` that was never `ready`, so it has no `expires_at`.
fn released_never_ready(released_at: i64) -> Record {
    lease(PROJECT, "released", None, Some(released_at))
}

fn host(project: &str, id: &str) -> HostToDecide {
    HostToDecide {
        project: project.to_string(),
        instance_id: id.to_string(),
    }
}

fn running(id: &str, launch_time: i64) -> Described {
    described(id, DescribedState::Running { launch_time })
}

fn described(id: &str, state: DescribedState) -> Described {
    Described {
        id: id.to_string(),
        state,
    }
}

fn other_state(state: &str) -> DescribedState {
    DescribedState::Other {
        state: state.to_string(),
    }
}

fn not_tried() -> ProvisionLock {
    ProvisionLock::NotTried {
        path: "C:\\example\\.agentspace\\hosts\\alpha\\provision.lock".to_string(),
        error: "Access is denied. (os error 5)".to_string(),
    }
}

/// Runs `decide`, recording every project `provision_held` was called with, in call order.
fn decide_recording(
    hosts: &[HostToDecide],
    leases: &[Record],
    states: &[Described],
    lock: impl Fn(&str) -> ProvisionLock,
    now: i64,
) -> (Vec<Action>, Vec<String>) {
    let calls = RefCell::new(Vec::new());
    let provision_held = |project: &str| {
        calls.borrow_mut().push(project.to_string());
        lock(project)
    };
    let actions = decide(hosts, leases, states, &provision_held, now);
    (actions, calls.into_inner())
}

/// Decides the one host of `PROJECT` and `ID` whose instance is `state`, with the given lock
/// outcome. Returns its one action and the projects the lock was tried for.
fn one(
    leases: &[Record],
    state: DescribedState,
    lock: ProvisionLock,
    now: i64,
) -> (Action, Vec<String>) {
    let (mut actions, calls) = decide_recording(
        &[host(PROJECT, ID)],
        leases,
        &[described(ID, state)],
        |_| lock.clone(),
        now,
    );
    assert_eq!(actions.len(), 1, "One action per host");
    (actions.remove(0), calls)
}

/// `one` for a running instance launched at `launch`.
fn run(leases: &[Record], launch: i64, lock: ProvisionLock, now: i64) -> (Action, Vec<String>) {
    one(
        leases,
        DescribedState::Running {
            launch_time: launch,
        },
        lock,
        now,
    )
}

fn explained(horizon: i64) -> Action {
    Action::Explained {
        project: PROJECT.to_string(),
        instance_id: ID.to_string(),
        horizon,
    }
}

fn unexplained(stop_at: i64, horizon: i64) -> Action {
    Action::Unexplained {
        project: PROJECT.to_string(),
        instance_id: ID.to_string(),
        stop_at,
        horizon,
    }
}

fn provisioning(stop_at: i64, horizon: i64) -> Action {
    Action::Provisioning {
        project: PROJECT.to_string(),
        instance_id: ID.to_string(),
        stop_at,
        horizon,
    }
}

fn stop(stop_at: i64, horizon: i64) -> Action {
    Action::Stop {
        project: PROJECT.to_string(),
        instance_id: ID.to_string(),
        stop_at,
        horizon,
    }
}

// ---------------------------------------------------------------------------------------------
// The grace.
// ---------------------------------------------------------------------------------------------

/// The grace is 15 minutes (D56, D63).
#[test]
fn grace_is_fifteen_minutes() {
    assert_eq!(GRACE_SECONDS, 900);
}

// ---------------------------------------------------------------------------------------------
// Explained (step 6: a live lease explains its host, whatever the host is doing).
// ---------------------------------------------------------------------------------------------

/// A live `ready` lease explains its host, however long ago the host launched, and the lock is
/// not tried.
#[test]
fn a_ready_lease_explains_its_host() {
    let (action, calls) = run(&[ready(NOW + 600)], OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, explained(NOW + 600));
    assert!(calls.is_empty(), "The lock is tried only for a due host");
}

/// A `pending` lease is live (step 6): a host being made is explained.
#[test]
fn a_pending_lease_explains_its_host() {
    let (action, calls) = run(&[pending()], OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, explained(NOW));
    assert!(calls.is_empty());
}

/// A project's leases are those whose `project` equals the host's exactly: leases of another
/// project, a longer name, a shorter one and a name of another case explain nothing.
#[test]
fn leases_of_other_projects_do_not_explain_a_host() {
    let others = [
        lease("beta", "ready", Some(NOW + 5000), None),
        lease("beta", "pending", None, None),
        lease("alpha-2", "ready", Some(NOW + 5000), None),
        lease("Alpha", "ready", Some(NOW + 5000), None),
        lease("alph", "pending", None, None),
    ];
    let (action, calls) = run(&others, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, stop(OLD_LAUNCH + GRACE_SECONDS, OLD_LAUNCH));
    assert_eq!(calls, vec![PROJECT.to_string()]);
}

/// A lease of another project, with a later expiry, beside the host's own live lease does not
/// move the host's horizon.
#[test]
fn another_project_s_later_expiry_is_not_the_horizon() {
    let leases = [
        lease("beta", "ready", Some(NOW + 9000), None),
        ready(NOW + 600),
    ];
    let (action, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, explained(NOW + 600));
}

/// A stored `ready` lease whose `expires_at` is exactly the run's time is expired, so not live
/// (state.md: at or after `expires_at`).
#[test]
fn a_ready_lease_expiring_at_the_run_s_time_is_not_live() {
    let (action, calls) = run(&[ready(NOW)], OLD_LAUNCH, ProvisionLock::Free, NOW);
    // Its end is NOW, later than the launch, so the host's since is NOW.
    assert_eq!(action, unexplained(NOW + GRACE_SECONDS, NOW));
    assert!(calls.is_empty(), "Not yet due, so the lock is not tried");
}

/// A stored `ready` lease expiring one second after the run's time is still live.
#[test]
fn a_ready_lease_expiring_a_second_after_the_run_s_time_is_live() {
    let (action, _) = run(&[ready(NOW + 1)], OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, explained(NOW + 1));
}

/// An expired `ready` lease explains nothing, and its end is its `expires_at`.
#[test]
fn an_expired_ready_lease_ends_at_its_expiry() {
    let (action, calls) = run(&[ready(NOW - 100)], OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(NOW - 100 + GRACE_SECONDS, NOW - 100));
    assert!(calls.is_empty());
}

/// Leases are read at the run's time: the same record is live at one time and expired at a later
/// one.
#[test]
fn leases_are_read_at_the_run_s_time() {
    let leases = [ready(NOW + 600)];
    let (live, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(live, explained(NOW + 600));
    let (expired, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW + 600);
    assert_eq!(expired, unexplained(NOW + 600 + GRACE_SECONDS, NOW + 600));
}

// ---------------------------------------------------------------------------------------------
// Unexplained: the since, the stop time and the grace's edge.
// ---------------------------------------------------------------------------------------------

/// A host that launched later than the project's last lease end has the launch as its since.
#[test]
fn since_is_the_launch_when_the_launch_is_later_than_the_last_lease_end() {
    let launch = NOW - 100;
    let (action, _) = run(&[ready(NOW - 5000)], launch, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(launch + GRACE_SECONDS, launch));
}

/// A last lease end later than the launch is the since.
#[test]
fn since_is_the_last_lease_end_when_it_is_later_than_the_launch() {
    let launch = NOW - 5000;
    let (action, _) = run(&[ready(NOW - 100)], launch, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(NOW - 100 + GRACE_SECONDS, NOW - 100));
}

/// A host whose project never held a lease has its launch as its since.
#[test]
fn since_is_the_launch_for_a_project_that_never_leased() {
    let launch = NOW - 120;
    let (action, calls) = run(&[], launch, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(launch + GRACE_SECONDS, launch));
    assert!(calls.is_empty());
}

/// Leases of other projects alone leave the project never leased.
#[test]
fn since_is_the_launch_when_only_other_projects_hold_leases() {
    let launch = NOW - 120;
    let leases = [
        lease("beta", "released", Some(NOW - 10), Some(NOW - 20)),
        lease("beta", "ready", Some(NOW - 5), None),
    ];
    let (action, _) = run(&leases, launch, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(launch + GRACE_SECONDS, launch));
}

/// An `unsatisfiable` lease has no end, so the launch is the since.
#[test]
fn an_unsatisfiable_lease_has_no_end() {
    let launch = NOW - 120;
    let leases = [lease(PROJECT, "unsatisfiable", None, None)];
    let (action, _) = run(&leases, launch, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(launch + GRACE_SECONDS, launch));
}

/// The last lease end is the latest end among the project's leases that are not live.
#[test]
fn the_last_lease_end_is_the_latest_end() {
    let leases = [
        released(NOW - 400, NOW - 1000),
        ready(NOW - 700),
        released_never_ready(NOW - 300),
        lease(PROJECT, "unsatisfiable", None, None),
    ];
    let (action, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(NOW - 300 + GRACE_SECONDS, NOW - 300));
}

/// A released lease ends at its `released_at` when that is earlier than its `expires_at` (P11).
#[test]
fn a_released_lease_ends_at_released_at_when_that_is_earlier() {
    let leases = [released(NOW + 5000, NOW - 300)];
    let (action, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(NOW - 300 + GRACE_SECONDS, NOW - 300));
}

/// A lease released after it expired ended at its `expires_at`, the earlier of the two (P11).
#[test]
fn a_released_lease_ends_at_its_expiry_when_that_is_earlier() {
    let leases = [released(NOW - 500, NOW - 100)];
    let (action, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(NOW - 500 + GRACE_SECONDS, NOW - 500));
}

/// A released lease that was never `ready` has no `expires_at`, and ends at its `released_at`.
#[test]
fn a_released_lease_that_was_never_ready_ends_at_released_at() {
    let leases = [released_never_ready(NOW - 200)];
    let (action, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(NOW - 200 + GRACE_SECONDS, NOW - 200));
}

/// A released lease's `expires_at`, later than the run's time, explains nothing and does not
/// move the since: only a live lease's counts.
#[test]
fn a_released_lease_s_later_expiry_does_not_explain_the_host() {
    let leases = [released(NOW + 9000, NOW - 50)];
    let (action, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, unexplained(NOW - 50 + GRACE_SECONDS, NOW - 50));
}

/// One second before the stop time the host is not due: it is `Unexplained`, and the lock is not
/// tried, even if a provisioning run holds it.
#[test]
fn one_second_before_the_stop_time_the_host_is_not_due() {
    let launch = NOW - GRACE_SECONDS + 1;
    let (action, calls) = run(&[], launch, ProvisionLock::Held, NOW);
    assert_eq!(action, unexplained(NOW + 1, launch));
    assert!(
        calls.is_empty(),
        "provision_held MUST NOT be called before the host is due"
    );
}

/// At the stop time, to the second, the host is due.
#[test]
fn at_the_stop_time_the_host_is_due() {
    let launch = NOW - GRACE_SECONDS;
    let (action, calls) = run(&[], launch, ProvisionLock::Free, NOW);
    assert_eq!(action, stop(NOW, launch));
    assert_eq!(calls, vec![PROJECT.to_string()]);
}

/// A second after the stop time the host is due.
#[test]
fn one_second_after_the_stop_time_the_host_is_due() {
    let launch = NOW - GRACE_SECONDS - 1;
    let (action, _) = run(&[], launch, ProvisionLock::Free, NOW);
    assert_eq!(action, stop(NOW - 1, launch));
}

/// The grace is counted from the last lease end when that is the since, and the edge holds there
/// too.
#[test]
fn the_grace_edge_holds_from_a_lease_end() {
    let (due, _) = run(
        &[ready(NOW - GRACE_SECONDS)],
        OLD_LAUNCH,
        ProvisionLock::Free,
        NOW,
    );
    assert_eq!(due, stop(NOW, NOW - GRACE_SECONDS));
    let (not_due, calls) = run(
        &[ready(NOW - GRACE_SECONDS + 1)],
        OLD_LAUNCH,
        ProvisionLock::Free,
        NOW,
    );
    assert_eq!(
        not_due,
        unexplained(NOW + 1, NOW - GRACE_SECONDS + 1),
        "A last lease end one second later is one second before due"
    );
    assert!(calls.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Due: the provisioning lock.
// ---------------------------------------------------------------------------------------------

/// Due and the lock held: the host is left, with its stop time and horizon.
#[test]
fn a_due_host_whose_provisioning_run_holds_the_lock_is_provisioning() {
    let launch = NOW - 5000;
    let (action, calls) = run(&[], launch, ProvisionLock::Held, NOW);
    assert_eq!(action, provisioning(launch + GRACE_SECONDS, launch));
    assert_eq!(calls, vec![PROJECT.to_string()]);
}

/// Due and the lock free: the host is stopped.
#[test]
fn a_due_host_with_a_free_lock_is_stopped() {
    let launch = NOW - 5000;
    let (action, calls) = run(&[], launch, ProvisionLock::Free, NOW);
    assert_eq!(action, stop(launch + GRACE_SECONDS, launch));
    assert_eq!(calls, vec![PROJECT.to_string()]);
}

/// Due and the lock not tried: the host is not stopped, and the action carries the path and the
/// error the run prints in `Cannot try <path>: <error>.`
#[test]
fn a_due_host_whose_lock_could_not_be_tried_carries_the_path_and_error() {
    let launch = NOW - 5000;
    let (action, calls) = run(&[], launch, not_tried(), NOW);
    assert_eq!(
        action,
        Action::LockNotTried {
            project: PROJECT.to_string(),
            instance_id: ID.to_string(),
            path: "C:\\example\\.agentspace\\hosts\\alpha\\provision.lock".to_string(),
            error: "Access is denied. (os error 5)".to_string(),
            horizon: launch,
        }
    );
    assert_eq!(calls, vec![PROJECT.to_string()]);
}

/// The lock is tried for the host's own project.
#[test]
fn the_lock_is_tried_for_the_host_s_project() {
    let (_, calls) = decide_recording(
        &[host("gamma", ID)],
        &[],
        &[running(ID, OLD_LAUNCH)],
        |_| ProvisionLock::Free,
        NOW,
    );
    assert_eq!(calls, vec!["gamma".to_string()]);
}

/// A host that is explained, or not yet due, never has its lock tried, whatever the lock says.
#[test]
fn the_lock_is_not_tried_for_explained_or_not_due_hosts() {
    let (actions, calls) = decide_recording(
        &[
            host("explained", "i-0123456789abcdef1"),
            host("notdue", "i-0123456789abcdef2"),
        ],
        &[lease("explained", "ready", Some(NOW + 600), None)],
        &[
            running("i-0123456789abcdef1", OLD_LAUNCH),
            running("i-0123456789abcdef2", NOW - 10),
        ],
        |_| ProvisionLock::Held,
        NOW,
    );
    assert!(matches!(actions[0], Action::Explained { .. }));
    assert!(matches!(actions[1], Action::Unexplained { .. }));
    assert!(calls.is_empty());
}

// ---------------------------------------------------------------------------------------------
// The instance's state in EC2.
// ---------------------------------------------------------------------------------------------

/// Any state but `running` is `Nothing`: no lock try, even for a project with no lease whose host
/// would be long past its grace were it running.
#[test]
fn an_instance_in_another_state_is_nothing() {
    for state in [
        "pending",
        "stopping",
        "stopped",
        "shutting-down",
        "terminated",
        "rebooting",
    ] {
        let (action, calls) = one(&[], other_state(state), ProvisionLock::Free, NOW);
        assert_eq!(
            action,
            Action::Nothing {
                project: PROJECT.to_string(),
                instance_id: ID.to_string(),
            },
            "State {state}"
        );
        assert!(calls.is_empty(), "State {state}");
    }
}

/// An instance in another state is `Nothing` whether or not a live lease explains it.
#[test]
fn an_instance_in_another_state_is_nothing_with_a_live_lease_too() {
    let (action, calls) = one(
        &[ready(NOW + 600)],
        other_state("stopped"),
        ProvisionLock::Free,
        NOW,
    );
    assert!(matches!(action, Action::Nothing { .. }));
    assert!(calls.is_empty());
}

/// An instance EC2 does not know (`Unknown`) is `NotFound`.
#[test]
fn an_unknown_instance_is_not_found() {
    let (action, calls) = one(&[], DescribedState::Unknown, ProvisionLock::Free, NOW);
    assert_eq!(
        action,
        Action::NotFound {
            project: PROJECT.to_string(),
            instance_id: ID.to_string(),
        }
    );
    assert!(calls.is_empty());
}

/// A host with no element in the answer at all is `NotFound`, as is a host whose ID only another
/// element carries.
#[test]
fn a_host_with_no_instance_in_the_answer_is_not_found() {
    let expected = Action::NotFound {
        project: PROJECT.to_string(),
        instance_id: ID.to_string(),
    };
    for states in [vec![], vec![running("i-0123456789abcdef9", OLD_LAUNCH)]] {
        let (actions, calls) = decide_recording(
            &[host(PROJECT, ID)],
            &[],
            &states,
            |_| ProvisionLock::Free,
            NOW,
        );
        assert_eq!(actions, vec![expected.clone()]);
        assert!(calls.is_empty());
    }
}

/// An `Unexpected` member is carried through, for either member, and the host is neither decided
/// nor tried for its lock, though it would be due.
#[test]
fn an_unexpected_member_is_carried_and_the_host_is_not_decided() {
    for member in ["State.Name", "LaunchTime"] {
        let (action, calls) = one(
            &[],
            DescribedState::Unexpected {
                member: member.to_string(),
            },
            ProvisionLock::Free,
            NOW,
        );
        assert_eq!(
            action,
            Action::Unexpected {
                project: PROJECT.to_string(),
                instance_id: ID.to_string(),
                member: member.to_string(),
            }
        );
        assert!(calls.is_empty(), "Member {member}");
    }
}

// ---------------------------------------------------------------------------------------------
// The horizon sent (step 7; D57; P12).
// ---------------------------------------------------------------------------------------------

/// An explained host is sent the latest `expires_at` among its project's live leases.
#[test]
fn the_horizon_of_an_explained_host_is_the_latest_live_expiry() {
    let leases = [ready(NOW + 600), ready(NOW + 3000), ready(NOW + 1200)];
    let (action, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, explained(NOW + 3000));
}

/// A project with only `pending` leases has the run's time as its horizon.
#[test]
fn the_horizon_of_a_pending_only_project_is_the_run_s_time() {
    let (action, _) = run(
        &[pending(), pending()],
        OLD_LAUNCH,
        ProvisionLock::Free,
        NOW,
    );
    assert_eq!(action, explained(NOW));
}

/// A live `ready` lease's expiry is the horizon beside a `pending` lease.
#[test]
fn the_horizon_with_pending_and_ready_leases_is_the_ready_expiry() {
    let (action, _) = run(
        &[pending(), ready(NOW + 900)],
        OLD_LAUNCH,
        ProvisionLock::Free,
        NOW,
    );
    assert_eq!(action, explained(NOW + 900));
}

/// Neither an expired lease's, a released lease's nor another project's `expires_at` adds to an
/// explained host's horizon.
#[test]
fn the_horizon_ignores_the_expiry_of_leases_that_are_not_live() {
    let leases = [
        ready(NOW + 600),
        released(NOW + 9000, NOW - 10),
        ready(NOW - 10),
        lease("beta", "ready", Some(NOW + 9000), None),
    ];
    let (action, _) = run(&leases, OLD_LAUNCH, ProvisionLock::Free, NOW);
    assert_eq!(action, explained(NOW + 600));
}

/// What is sent is the later of the project's horizon and the host's `LaunchTime` (P12): a host
/// launched after the horizon is sent its launch time.
#[test]
fn the_horizon_is_never_before_the_launch_for_an_explained_host() {
    let launch = NOW + 5000;
    let (action, _) = run(&[ready(NOW + 600)], launch, ProvisionLock::Free, NOW);
    assert_eq!(action, explained(launch));
}

/// An unexplained host's horizon plus the grace is its stop time, in every `Provisioning` and
/// `Stop` it can be, whichever of the launch and the last lease end is the since; a
/// `LockNotTried` carries the since as its horizon.
#[test]
fn a_due_host_s_horizon_plus_the_grace_is_its_stop_time() {
    // Each case: the project's leases, the launch, and the since both must give. Every since is
    // at least a grace before NOW, so each host is due.
    let cases: [(Vec<Record>, i64, i64); 4] = [
        (vec![], NOW - 5000, NOW - 5000),
        (vec![ready(NOW - 1000)], NOW - 5000, NOW - 1000),
        (vec![ready(NOW - 5000)], NOW - 1000, NOW - 1000),
        (
            vec![released(NOW + 700, NOW - 2000)],
            NOW - 5000,
            NOW - 2000,
        ),
    ];
    for (leases, launch, since) in cases {
        let (free, _) = run(&leases, launch, ProvisionLock::Free, NOW);
        assert_eq!(free, stop(since + GRACE_SECONDS, since));
        let (held, _) = run(&leases, launch, ProvisionLock::Held, NOW);
        assert_eq!(held, provisioning(since + GRACE_SECONDS, since));
        let (failed, _) = run(&leases, launch, not_tried(), NOW);
        match failed {
            Action::LockNotTried { horizon, .. } => assert_eq!(horizon, since),
            other => panic!("Expected LockNotTried, got {other:?}"),
        }
    }
}

/// A not-yet-due unexplained host's horizon plus the grace is its stop time.
#[test]
fn a_not_yet_due_host_s_horizon_plus_the_grace_is_its_stop_time() {
    let (action, _) = run(&[ready(NOW - 100)], NOW - 5000, ProvisionLock::Free, NOW);
    match action {
        Action::Unexplained {
            stop_at, horizon, ..
        } => {
            assert_eq!(horizon, NOW - 100);
            assert_eq!(horizon + GRACE_SECONDS, stop_at);
        }
        other => panic!("Expected Unexplained, got {other:?}"),
    }
}

/// A host launched after its project's last lease end is sent its launch time, not a time from
/// before its launch (P12), in each action that carries a horizon.
#[test]
fn a_host_launched_after_its_last_lease_end_is_sent_its_launch() {
    let leases = [released(NOW - 3000, NOW - 4000)];
    let launch = NOW - 100;
    let (not_due, _) = run(&leases, launch, ProvisionLock::Free, NOW);
    assert_eq!(not_due, unexplained(launch + GRACE_SECONDS, launch));
    let launch = NOW - GRACE_SECONDS;
    let (due, _) = run(&leases, launch, ProvisionLock::Free, NOW);
    assert_eq!(due, stop(NOW, launch));
    let (held, _) = run(&leases, launch, ProvisionLock::Held, NOW);
    assert_eq!(held, provisioning(NOW, launch));
}

// ---------------------------------------------------------------------------------------------
// Several hosts.
// ---------------------------------------------------------------------------------------------

/// One action per host in the input's order, whatever the order of the EC2 answer, each carrying
/// its own host's project and instance ID; `provision_held` is called once for each due host and
/// for no other.
#[test]
fn one_action_per_host_in_order_and_the_lock_is_tried_for_due_hosts_only() {
    let ids: Vec<String> = (1..=7).map(|n| format!("i-0123456789abcdef{n}")).collect();
    let hosts = vec![
        host("explained", &ids[0]),
        host("due-free", &ids[1]),
        host("stopped", &ids[2]),
        host("missing", &ids[3]),
        host("not-due", &ids[4]),
        host("due-held", &ids[5]),
        host("odd", &ids[6]),
    ];
    let leases = vec![
        lease("explained", "ready", Some(NOW + 600), None),
        lease("not-due", "released", Some(NOW + 600), Some(NOW - 60)),
        lease("stopped", "ready", Some(NOW - 5000), None),
    ];
    // The answer lists the hosts in reverse order.
    let states = vec![
        described(
            &ids[6],
            DescribedState::Unexpected {
                member: "LaunchTime".to_string(),
            },
        ),
        running(&ids[5], NOW - 5000),
        running(&ids[4], NOW - 5000),
        described(&ids[3], DescribedState::Unknown),
        described(&ids[2], other_state("stopped")),
        running(&ids[1], NOW - 5000),
        running(&ids[0], NOW - 5000),
    ];
    let (actions, mut calls) = decide_recording(
        &hosts,
        &leases,
        &states,
        |project| {
            if project == "due-held" {
                ProvisionLock::Held
            } else {
                ProvisionLock::Free
            }
        },
        NOW,
    );
    assert_eq!(
        actions,
        vec![
            Action::Explained {
                project: "explained".to_string(),
                instance_id: ids[0].clone(),
                horizon: NOW + 600,
            },
            Action::Stop {
                project: "due-free".to_string(),
                instance_id: ids[1].clone(),
                stop_at: NOW - 5000 + GRACE_SECONDS,
                horizon: NOW - 5000,
            },
            Action::Nothing {
                project: "stopped".to_string(),
                instance_id: ids[2].clone(),
            },
            Action::NotFound {
                project: "missing".to_string(),
                instance_id: ids[3].clone(),
            },
            Action::Unexplained {
                project: "not-due".to_string(),
                instance_id: ids[4].clone(),
                stop_at: NOW - 60 + GRACE_SECONDS,
                horizon: NOW - 60,
            },
            Action::Provisioning {
                project: "due-held".to_string(),
                instance_id: ids[5].clone(),
                stop_at: NOW - 5000 + GRACE_SECONDS,
                horizon: NOW - 5000,
            },
            Action::Unexpected {
                project: "odd".to_string(),
                instance_id: ids[6].clone(),
                member: "LaunchTime".to_string(),
            },
        ]
    );
    calls.sort();
    assert_eq!(calls, vec!["due-free".to_string(), "due-held".to_string()]);
}

/// Each host is decided by its own project's leases: two hosts of different projects, one
/// explained and one not.
#[test]
fn each_host_is_decided_by_its_own_project_s_leases() {
    let hosts = [
        host("one", "i-0123456789abcdef1"),
        host("two", "i-0123456789abcdef2"),
    ];
    let leases = [lease("one", "pending", None, None)];
    let states = [
        running("i-0123456789abcdef1", OLD_LAUNCH),
        running("i-0123456789abcdef2", OLD_LAUNCH),
    ];
    let (actions, calls) = decide_recording(&hosts, &leases, &states, |_| ProvisionLock::Free, NOW);
    assert!(matches!(actions[0], Action::Explained { .. }));
    assert!(matches!(actions[1], Action::Stop { .. }));
    assert_eq!(calls, vec!["two".to_string()]);
}

/// `provision_held` is called at most once for each host, though the same project and instance
/// appear twice in the input.
#[test]
fn the_lock_is_tried_at_most_once_for_each_host() {
    let hosts = [host(PROJECT, ID), host(PROJECT, ID)];
    let (actions, calls) = decide_recording(
        &hosts,
        &[],
        &[running(ID, OLD_LAUNCH)],
        |_| ProvisionLock::Free,
        NOW,
    );
    assert_eq!(actions.len(), 2);
    assert!(calls.len() <= 2, "At most one try for each host");
}

// ---------------------------------------------------------------------------------------------
// Odd inputs: decide never panics.
// ---------------------------------------------------------------------------------------------

/// No hosts, no action, and no lock try.
#[test]
fn no_hosts_gives_no_actions() {
    let (actions, calls) = decide_recording(
        &[],
        &[ready(NOW + 1)],
        &[running(ID, OLD_LAUNCH)],
        |_| ProvisionLock::Free,
        NOW,
    );
    assert!(actions.is_empty());
    assert!(calls.is_empty());
}

/// Empty leases and an empty answer: every host is `NotFound`, and nothing panics.
#[test]
fn empty_leases_and_an_empty_answer_give_not_found_for_every_host() {
    let hosts = [
        host("one", "i-0123456789abcdef1"),
        host("two", "i-0123456789abcdef2"),
    ];
    let (actions, calls) = decide_recording(&hosts, &[], &[], |_| ProvisionLock::Free, NOW);
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().all(|a| matches!(a, Action::NotFound { .. })));
    assert!(calls.is_empty());
}

/// Duplicate hosts and duplicate elements in the answer give one action per host and no panic.
#[test]
fn duplicate_hosts_and_duplicate_answers_do_not_panic() {
    let hosts = [host(PROJECT, ID), host(PROJECT, ID), host("other", ID)];
    let states = [
        running(ID, OLD_LAUNCH),
        running(ID, NOW - 10),
        described(ID, DescribedState::Unknown),
    ];
    let leases = [ready(NOW + 600), ready(NOW + 600), pending()];
    let (actions, _) = decide_recording(&hosts, &leases, &states, |_| ProvisionLock::Free, NOW);
    assert_eq!(actions.len(), 3);
}

/// A negative run's time, a huge one and extremes of the launch time never panic, and a host is
/// still given one action.
#[test]
fn extreme_times_do_not_panic() {
    for now in [i64::MIN, -1, 0, i64::MAX / 2, i64::MAX] {
        for launch in [i64::MIN, -1, 0, NOW, i64::MAX / 2, i64::MAX] {
            let sets = [
                vec![],
                vec![ready(NOW)],
                vec![pending()],
                vec![released(NOW, NOW)],
            ];
            for leases in sets {
                for lock in [ProvisionLock::Free, ProvisionLock::Held] {
                    // `run` asserts one action for the one host.
                    run(&leases, launch, lock, now);
                }
            }
        }
    }
}

/// A negative run's time still decides a lease by its instants: a lease expiring after it is
/// live.
#[test]
fn a_negative_run_s_time_decides_by_instants() {
    let (action, _) = run(&[ready(NOW)], -5000, ProvisionLock::Free, -1);
    assert_eq!(action, explained(NOW));
}

/// Lease records whose times do not parse (a hand-built `Record`) do not panic.
#[test]
fn unparseable_lease_times_do_not_panic() {
    let mut odd = ready(NOW + 600);
    odd.expires_at = Some("not a time".to_string());
    let mut odd_released = released(NOW, NOW);
    odd_released.released_at = Some(String::new());
    let (actions, _) = decide_recording(
        &[host(PROJECT, ID)],
        &[odd, odd_released],
        &[running(ID, OLD_LAUNCH)],
        |_| ProvisionLock::Free,
        NOW,
    );
    assert_eq!(actions.len(), 1);
}

//! The guard's decision for each host (operator.md, guard, step 6, and the horizon of step 7; D56,
//! D57).
//!
//! **Pinned by the dev lead for E0.08-T03-S01 (#0177)**, so the decision's implementer and its
//! test author build against the same types. [`decide`] is pure: it reads no file, takes no lock,
//! reads no clock and makes no AWS call. The run (src/run.rs) gathers its inputs and carries out
//! what it returns.

use agentspace_box::{Described, DescribedState};
use agentspace_state::{Record, last_lease_end, lease_horizon, live_count};

use crate::GRACE_SECONDS;

/// A host the run found an instance ID for (operator.md, guard, step 4), in the registry's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostToDecide {
    /// The host's project, as the registry records it.
    pub project: String,
    /// The instance ID step 4 found.
    pub instance_id: String,
}

/// The outcome of one try of a host's `provision.lock` (operator.md, guard, step 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionLock {
    /// Nobody holds it, a missing folder or file included.
    Free,
    /// A provisioning run holds it.
    Held,
    /// It could not be tried for any other reason. `path` is the lock's full path and `error` the
    /// host registry library's text for the error, for `Cannot try <path>: <error>.`
    NotTried { path: String, error: String },
}

/// What the run MUST do for one host. Every variant that a host's horizon is sent for carries the
/// value to send: the later of the project's horizon and the host's `LaunchTime` (step 7, P12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// The instance is in any state but `running`: no line, no stop and no horizon.
    Nothing {
        project: String,
        instance_id: String,
    },
    /// The instance is missing from the answer: the host's part fails with
    /// `Instance <id> was not found.`, and no horizon is sent.
    NotFound {
        project: String,
        instance_id: String,
    },
    /// A member of the instance's element is missing or not of its form (`State.Name`, or
    /// `LaunchTime` for a running instance): the host's part fails with operator.md's message for
    /// `member`, logged with `step=instance`, and no horizon is sent.
    Unexpected {
        project: String,
        instance_id: String,
        member: String,
    },
    /// Running, and the project holds a live lease: nothing logged, and the horizon sent.
    Explained {
        project: String,
        instance_id: String,
        horizon: i64,
    },
    /// Running, unexplained, and the run's time is before `stop_at`: logged `unexplained` with
    /// `stop_at`, its line printed, and the horizon sent.
    Unexplained {
        project: String,
        instance_id: String,
        stop_at: i64,
        horizon: i64,
    },
    /// Due, and a provisioning run holds `provision.lock`: logged `unexplained` with `stop_at` and
    /// `provisioning=running`, its line printed, not stopped, and the horizon sent.
    Provisioning {
        project: String,
        instance_id: String,
        stop_at: i64,
        horizon: i64,
    },
    /// Due, and `provision.lock` could not be tried: the host's part fails with
    /// `Cannot try <path>: <error>.`, not stopped, and the horizon sent.
    LockNotTried {
        project: String,
        instance_id: String,
        path: String,
        error: String,
        horizon: i64,
    },
    /// Due, and `provision.lock` is free: stopped with one `ec2 stop-instances`. If the stop fails,
    /// the host's part fails and the horizon is sent; if it succeeds, no horizon is sent.
    Stop {
        project: String,
        instance_id: String,
        stop_at: i64,
        horizon: i64,
    },
}

/// Decides every host in `hosts`, returning exactly one [`Action`] per host, in `hosts`' order.
///
/// - `leases` are the lease state file's records, read at `now`.
/// - `ec2_states` is `describe_hosts`' answer; a host's instance is the element whose `id` is its
///   instance ID, and a host with none is [`Action::NotFound`].
/// - `provision_held` is called with a project only for a host that is due, at most once per
///   host, and never for any other host.
/// - `now` is the run's time in Unix seconds.
pub fn decide(
    hosts: &[HostToDecide],
    leases: &[Record],
    ec2_states: &[Described],
    provision_held: &dyn Fn(&str) -> ProvisionLock,
    now: i64,
) -> Vec<Action> {
    hosts
        .iter()
        .map(|host| decide_host(host, leases, ec2_states, provision_held, now))
        .collect()
}

/// One host's decision (operator.md, guard, steps 5 to 7).
fn decide_host(
    host: &HostToDecide,
    leases: &[Record],
    ec2_states: &[Described],
    provision_held: &dyn Fn(&str) -> ProvisionLock,
    now: i64,
) -> Action {
    let project = host.project.clone();
    let instance_id = host.instance_id.clone();

    // Step 5: the host's instance is the first element whose ID is its own. An ID EC2 does not
    // know is missing from the answer, which `describe_hosts` reports as `Unknown`.
    let state = ec2_states
        .iter()
        .find(|described| described.id == host.instance_id)
        .map(|described| &described.state);
    let launch_time = match state {
        None | Some(DescribedState::Unknown) => {
            return Action::NotFound {
                project,
                instance_id,
            };
        }
        Some(DescribedState::Unexpected { member }) => {
            return Action::Unexpected {
                project,
                instance_id,
                member: member.clone(),
            };
        }
        Some(DescribedState::Other { .. }) => {
            return Action::Nothing {
                project,
                instance_id,
            };
        }
        Some(DescribedState::Running { launch_time }) => *launch_time,
    };

    // Step 7: the later of the project's horizon and the host's launch time (P12).
    let horizon = lease_horizon(leases, &host.project, now, launch_time).max(launch_time);

    // Step 6: a live lease explains the host, whatever it is doing.
    if live_count(leases, &host.project, now) > 0 {
        return Action::Explained {
            project,
            instance_id,
            horizon,
        };
    }

    // Unexplained: its since is the later of its launch time and the project's last lease end,
    // or its launch time alone, and its stop time is the since plus the grace.
    let since =
        last_lease_end(leases, &host.project, now).map_or(launch_time, |end| end.max(launch_time));
    let stop_at = since.saturating_add(GRACE_SECONDS);
    if now < stop_at {
        return Action::Unexplained {
            project,
            instance_id,
            stop_at,
            horizon,
        };
    }

    // Due: the host's provisioning lock is tried once, and only now.
    match provision_held(&host.project) {
        ProvisionLock::Held => Action::Provisioning {
            project,
            instance_id,
            stop_at,
            horizon,
        },
        ProvisionLock::NotTried { path, error } => Action::LockNotTried {
            project,
            instance_id,
            path,
            error,
            horizon,
        },
        ProvisionLock::Free => Action::Stop {
            project,
            instance_id,
            stop_at,
            horizon,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_state::format_time;
    use std::cell::RefCell;

    /// An arbitrary run's time: 2026-11-22T09:45:00Z.
    const NOW: i64 = 1_795_340_700;
    const A: &str = "i-0123456789abcdef0";
    const B: &str = "i-0123456789abcdef1";
    const C: &str = "i-0123456789abcdef2";

    fn host(project: &str, id: &str) -> HostToDecide {
        HostToDecide {
            project: project.to_string(),
            instance_id: id.to_string(),
        }
    }

    fn running(id: &str, launch_time: i64) -> Described {
        Described {
            id: id.to_string(),
            state: DescribedState::Running { launch_time },
        }
    }

    fn described(id: &str, state: DescribedState) -> Described {
        Described {
            id: id.to_string(),
            state,
        }
    }

    /// A lease of `project`, its times as offsets from `NOW`.
    fn lease(project: &str, state: &str, expires: Option<i64>, released: Option<i64>) -> Record {
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
            created_at: format_time(NOW - 7200),
            expires_at: expires.map(|o| format_time(NOW + o)),
            released_at: released.map(|o| format_time(NOW + o)),
            ssh: None,
            reasons: None,
            driver: None,
        }
    }

    /// A `provision_held` that records each project it is asked about and answers `answer`.
    struct Lock {
        asked: RefCell<Vec<String>>,
        answer: ProvisionLock,
    }

    impl Lock {
        fn new(answer: ProvisionLock) -> Self {
            Lock {
                asked: RefCell::new(Vec::new()),
                answer,
            }
        }

        fn run(
            &self,
            hosts: &[HostToDecide],
            leases: &[Record],
            ec2: &[Described],
            now: i64,
        ) -> Vec<Action> {
            let held = |project: &str| {
                self.asked.borrow_mut().push(project.to_string());
                self.answer.clone()
            };
            decide(hosts, leases, ec2, &held, now)
        }

        fn asked(&self) -> Vec<String> {
            self.asked.borrow().clone()
        }
    }

    fn other(id: &str, state: &str) -> Described {
        described(
            id,
            DescribedState::Other {
                state: state.into(),
            },
        )
    }

    #[test]
    fn one_action_per_host_in_the_hosts_order() {
        let lock = Lock::new(ProvisionLock::Free);
        let hosts = [host("c", C), host("a", A), host("b", B)];
        let ec2 = [
            other(A, "stopped"),
            described(B, DescribedState::Unknown),
            other(C, "pending"),
        ];
        let actions = lock.run(&hosts, &[], &ec2, NOW);
        assert_eq!(
            actions,
            [
                Action::Nothing {
                    project: "c".into(),
                    instance_id: C.into()
                },
                Action::Nothing {
                    project: "a".into(),
                    instance_id: A.into()
                },
                Action::NotFound {
                    project: "b".into(),
                    instance_id: B.into()
                },
            ]
        );
        assert!(lock.run(&[], &[], &ec2, NOW).is_empty());
    }

    #[test]
    fn a_host_missing_from_the_answer_or_unknown_to_it_is_not_found() {
        let lock = Lock::new(ProvisionLock::Free);
        let hosts = [host("a", A), host("b", B)];
        let ec2 = [described(B, DescribedState::Unknown)];
        let actions = lock.run(&hosts, &[], &ec2, NOW);
        assert_eq!(actions.len(), 2);
        for action in actions {
            assert!(matches!(action, Action::NotFound { .. }), "{action:?}");
        }
        assert!(lock.asked().is_empty());
    }

    #[test]
    fn an_unexpected_member_fails_that_host_alone() {
        let lock = Lock::new(ProvisionLock::Free);
        let hosts = [host("a", A), host("b", B)];
        let ec2 = [
            described(
                A,
                DescribedState::Unexpected {
                    member: "LaunchTime".into(),
                },
            ),
            running(B, NOW - 60),
        ];
        let actions = lock.run(&hosts, &[], &ec2, NOW);
        assert_eq!(
            actions[0],
            Action::Unexpected {
                project: "a".into(),
                instance_id: A.into(),
                member: "LaunchTime".into()
            }
        );
        assert!(matches!(actions[1], Action::Unexplained { .. }));
    }

    #[test]
    fn the_first_element_with_the_hosts_id_is_its_instance() {
        let lock = Lock::new(ProvisionLock::Free);
        let ec2 = [
            other(B, "running"),
            other(A, "stopped"),
            running(A, NOW - 100_000),
        ];
        let actions = lock.run(&[host("a", A)], &[], &ec2, NOW);
        assert!(matches!(actions[0], Action::Nothing { .. }), "{actions:?}");
        assert!(lock.asked().is_empty());
    }

    #[test]
    fn a_live_lease_explains_the_host_and_sends_its_horizon() {
        let lock = Lock::new(ProvisionLock::Free);
        let leases = [lease("p", "ready", Some(1800), None)];
        let actions = lock.run(&[host("p", A)], &leases, &[running(A, NOW - 100_000)], NOW);
        assert_eq!(
            actions,
            [Action::Explained {
                project: "p".into(),
                instance_id: A.into(),
                horizon: NOW + 1800
            }]
        );
        assert!(lock.asked().is_empty());
    }

    #[test]
    fn a_pending_lease_explains_the_host_and_its_horizon_is_the_runs_time() {
        let lock = Lock::new(ProvisionLock::Free);
        let leases = [
            lease("p", "pending", None, None),
            lease("p", "released", None, Some(-100_000)),
        ];
        let actions = lock.run(&[host("p", A)], &leases, &[running(A, NOW - 50_000)], NOW);
        assert_eq!(
            actions,
            [Action::Explained {
                project: "p".into(),
                instance_id: A.into(),
                horizon: NOW
            }]
        );
    }

    #[test]
    fn the_horizon_sent_is_never_before_the_launch_time() {
        let lock = Lock::new(ProvisionLock::Free);
        // Explained, with a live lease's expiry before the launch time.
        let leases = [lease("p", "ready", Some(60), None)];
        let launch = NOW + 5000;
        let actions = lock.run(&[host("p", A)], &leases, &[running(A, launch)], NOW);
        assert!(matches!(actions[0], Action::Explained { horizon, .. } if horizon == launch));
        // Unexplained, with a last lease end before the launch time.
        let leases = [lease("p", "released", None, Some(-3000))];
        let launch = NOW - 10;
        let actions = lock.run(&[host("p", A)], &leases, &[running(A, launch)], NOW);
        assert_eq!(
            actions,
            [Action::Unexplained {
                project: "p".into(),
                instance_id: A.into(),
                stop_at: launch + GRACE_SECONDS,
                horizon: launch
            }]
        );
    }

    #[test]
    fn another_projects_lease_explains_nothing() {
        let lock = Lock::new(ProvisionLock::Free);
        let leases = [
            lease("other", "ready", Some(9000), None),
            lease("other", "pending", None, None),
        ];
        let launch = NOW - 60;
        let actions = lock.run(&[host("p", A)], &leases, &[running(A, launch)], NOW);
        assert_eq!(
            actions,
            [Action::Unexplained {
                project: "p".into(),
                instance_id: A.into(),
                stop_at: launch + GRACE_SECONDS,
                horizon: launch
            }]
        );
    }

    #[test]
    fn an_unexplained_hosts_since_is_the_later_of_launch_and_last_lease_end() {
        let lock = Lock::new(ProvisionLock::Free);
        // The last lease end after the launch time.
        let leases = [
            lease("p", "ready", Some(-300), None),
            lease("p", "released", None, Some(-600)),
        ];
        let actions = lock.run(&[host("p", A)], &leases, &[running(A, NOW - 5000)], NOW);
        assert_eq!(
            actions,
            [Action::Unexplained {
                project: "p".into(),
                instance_id: A.into(),
                stop_at: NOW - 300 + GRACE_SECONDS,
                horizon: NOW - 300
            }]
        );
        // No lease at all: the launch time alone.
        let actions = lock.run(&[host("p", A)], &[], &[running(A, NOW - 120)], NOW);
        assert_eq!(
            actions,
            [Action::Unexplained {
                project: "p".into(),
                instance_id: A.into(),
                stop_at: NOW - 120 + GRACE_SECONDS,
                horizon: NOW - 120
            }]
        );
        assert!(lock.asked().is_empty());
    }

    #[test]
    fn a_lease_at_its_expiry_no_longer_explains_the_host() {
        let lock = Lock::new(ProvisionLock::Free);
        let leases = [lease("p", "ready", Some(0), None)];
        let ec2 = [running(A, NOW - 100_000)];
        let before = lock.run(&[host("p", A)], &leases, &ec2, NOW - 1);
        assert!(matches!(before[0], Action::Explained { horizon, .. } if horizon == NOW));
        let at = lock.run(&[host("p", A)], &leases, &ec2, NOW);
        assert!(
            matches!(at[0], Action::Unexplained { stop_at, horizon, .. }
                if stop_at == NOW + GRACE_SECONDS && horizon == NOW),
            "{at:?}"
        );
    }

    #[test]
    fn a_host_is_due_at_its_stop_time_to_the_second() {
        let launch = NOW - GRACE_SECONDS;
        let ec2 = [running(A, launch)];
        // One second before: not due, and the lock is not tried.
        let lock = Lock::new(ProvisionLock::Free);
        let actions = lock.run(&[host("p", A)], &[], &ec2, NOW - 1);
        assert!(matches!(actions[0], Action::Unexplained { stop_at, .. } if stop_at == NOW));
        assert!(lock.asked().is_empty());
        // At the stop time, and after it: due.
        for now in [NOW, NOW + 1, NOW + 100_000] {
            let lock = Lock::new(ProvisionLock::Free);
            let actions = lock.run(&[host("p", A)], &[], &ec2, now);
            assert_eq!(
                actions,
                [Action::Stop {
                    project: "p".into(),
                    instance_id: A.into(),
                    stop_at: NOW,
                    horizon: launch
                }]
            );
            assert_eq!(lock.asked(), ["p"]);
        }
    }

    #[test]
    fn a_due_host_follows_its_provisioning_lock() {
        let launch = NOW - 10_000;
        let ec2 = [running(A, launch)];
        let stop_at = launch + GRACE_SECONDS;

        let lock = Lock::new(ProvisionLock::Held);
        assert_eq!(
            lock.run(&[host("p", A)], &[], &ec2, NOW),
            [Action::Provisioning {
                project: "p".into(),
                instance_id: A.into(),
                stop_at,
                horizon: launch
            }]
        );
        assert_eq!(lock.asked(), ["p"]);

        let lock = Lock::new(ProvisionLock::NotTried {
            path: r"C:\fake\hosts\p\provision.lock".into(),
            error: "Access is denied.".into(),
        });
        assert_eq!(
            lock.run(&[host("p", A)], &[], &ec2, NOW),
            [Action::LockNotTried {
                project: "p".into(),
                instance_id: A.into(),
                path: r"C:\fake\hosts\p\provision.lock".into(),
                error: "Access is denied.".into(),
                horizon: launch
            }]
        );
        assert_eq!(lock.asked(), ["p"]);

        let lock = Lock::new(ProvisionLock::Free);
        assert_eq!(
            lock.run(&[host("p", A)], &[], &ec2, NOW),
            [Action::Stop {
                project: "p".into(),
                instance_id: A.into(),
                stop_at,
                horizon: launch
            }]
        );
        assert_eq!(lock.asked(), ["p"]);
    }

    #[test]
    fn the_lock_is_tried_only_for_due_hosts_once_each() {
        const D: &str = "i-0123456789abcdef3";
        const E: &str = "i-0123456789abcdef4";
        const F: &str = "i-0123456789abcdef5";
        const G: &str = "i-0123456789abcdef6";
        let lock = Lock::new(ProvisionLock::Held);
        let hosts = [
            host("explained", A),
            host("early", B),
            host("due", C),
            host("stopped", D),
            host("missing", E),
            host("bad", F),
            host("due2", G),
        ];
        let ec2 = [
            running(A, NOW - 100_000),
            running(B, NOW - 60),
            running(C, NOW - 100_000),
            other(D, "stopped"),
            described(
                F,
                DescribedState::Unexpected {
                    member: "State.Name".into(),
                },
            ),
            running(G, NOW - 100_000),
        ];
        let leases = [lease("explained", "pending", None, None)];
        let actions = lock.run(&hosts, &leases, &ec2, NOW);
        assert_eq!(actions.len(), hosts.len());
        assert_eq!(lock.asked(), ["due", "due2"]);
    }

    #[test]
    fn the_horizon_sent_plus_the_grace_is_an_unexplained_hosts_stop_time() {
        let sets: Vec<Vec<Record>> = vec![
            vec![],
            vec![lease("p", "released", None, Some(-600))],
            vec![lease("p", "released", Some(-900), Some(-100))],
            vec![lease("p", "ready", Some(-50), None)],
            vec![lease("p", "ready", Some(0), None)],
            vec![lease("p", "unsatisfiable", None, None)],
            vec![
                lease("p", "released", None, Some(300)),
                lease("p", "ready", Some(-2000), None),
            ],
        ];
        let mut checked = 0;
        for leases in &sets {
            for launch in [NOW - 100_000, NOW - 700, NOW - 50, NOW, NOW + 400] {
                for now in [NOW - 1000, NOW, NOW + 1000, NOW + 100_000] {
                    let lock = Lock::new(ProvisionLock::Held);
                    let actions = lock.run(&[host("p", A)], leases, &[running(A, launch)], now);
                    match &actions[0] {
                        Action::Unexplained {
                            stop_at, horizon, ..
                        }
                        | Action::Provisioning {
                            stop_at, horizon, ..
                        } => {
                            assert_eq!(horizon + GRACE_SECONDS, *stop_at, "{leases:?}");
                            checked += 1;
                        }
                        Action::Explained { .. } => {}
                        other => panic!("{other:?}"),
                    }
                }
            }
        }
        assert!(checked > 100, "{checked}");
    }

    #[test]
    fn duplicate_hosts_are_each_decided() {
        let lock = Lock::new(ProvisionLock::Free);
        let hosts = [host("p", A), host("p", A)];
        let actions = lock.run(&hosts, &[], &[running(A, NOW - 100_000)], NOW);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], actions[1]);
        assert!(matches!(actions[0], Action::Stop { .. }));
        assert_eq!(lock.asked(), ["p", "p"]);
    }

    #[test]
    fn extreme_times_and_names_never_panic() {
        let leases = [
            lease("", "pending", None, None),
            lease("p", "released", None, Some(-600)),
        ];
        for launch in [i64::MIN, -1, 0, NOW, i64::MAX] {
            for now in [i64::MIN, -1, 0, NOW, i64::MAX] {
                for project in ["", "p", "q"] {
                    let lock = Lock::new(ProvisionLock::Free);
                    let actions =
                        lock.run(&[host(project, A)], &leases, &[running(A, launch)], now);
                    assert_eq!(actions.len(), 1);
                    assert!(lock.asked().len() <= 1);
                }
            }
        }
        // The stop time saturates rather than overflowing.
        let lock = Lock::new(ProvisionLock::Free);
        let actions = lock.run(&[host("q", A)], &[], &[running(A, i64::MAX)], NOW);
        assert_eq!(
            actions,
            [Action::Unexplained {
                project: "q".into(),
                instance_id: A.into(),
                stop_at: i64::MAX,
                horizon: i64::MAX
            }]
        );
    }
}

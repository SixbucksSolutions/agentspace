//! The guard's decision for each host (operator.md, guard, step 6, and the horizon of step 7; D56,
//! D57).
//!
//! **Pinned by the dev lead for E0.08-T03-S01 (#0177)**, so the decision's implementer and its
//! test author build against the same types. [`decide`] is pure: it reads no file, takes no lock,
//! reads no clock and makes no AWS call. The run (src/run.rs) gathers its inputs and carries out
//! what it returns.

use agentspace_box::Described;
use agentspace_state::Record;

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
    /// system's own error text, for `Cannot try <path>: <error>.`
    NotTried { path: String, error: String },
}

/// What the run MUST do for one host. Every variant that a host's horizon is sent for carries the
/// value to send: the later of the project's horizon and the host's `LaunchTime` (step 7, P12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// The instance is in any state but `running`: no line, no stop and no horizon.
    Nothing { project: String, instance_id: String },
    /// The instance is missing from the answer: the host's part fails with
    /// `Instance <id> was not found.`, and no horizon is sent.
    NotFound { project: String, instance_id: String },
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
    let _ = (hosts, leases, ec2_states, provision_held, now);
    Vec::new()
}

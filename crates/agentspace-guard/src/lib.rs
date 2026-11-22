//! The guard's run, `agentspace-operator guard [-Quiet]` (docs/operator.md, guard; D56, D57, D63).
//!
//! **The architect's stub, pinned before v0.08-T03 (D63).** Its public API is fixed:
//! E0.08-T03-S01 (#0177) builds the run behind it without changing a signature, and
//! E0.08-T03-S02 (#0178) calls it from the operator's `guard` command. Until #0177 merges, [`run`]
//! returns the failure `agentspace: guard is not built yet.`

use std::path::Path;

pub mod decide;

use agentspace_operator_api::{Aws, CallerFailure, Context, Failure, Say};

/// The grace, a constant and not a parameter (D56, D63): 15 minutes, in seconds.
pub const GRACE_SECONDS: i64 = 15 * 60;

/// What a run reads beside the context, each resolved once by the operator before the call.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    /// `USERPROFILE` and the system folder, as every operator command gets them.
    pub ctx: &'a Context,
    /// The state folder whose `leases.json` the run reads: `AGENTSPACE_STATE_DIR` when it is set
    /// and not empty, made full, else `%USERPROFILE%\.agentspace` (operator.md, Environment
    /// variables), as the ProxyCommand resolves it.
    pub state_dir: &'a Path,
    /// The run's time, read once from the UTC clock to the whole second, in Unix seconds
    /// (operator.md, guard, step 3). The operator passes the clock's reading; a test passes its
    /// own. Every lease of the run is read at it, and every comparison with the grace uses it.
    pub now: i64,
}

/// What a run that went to its end did (operator.md, guard, step 8), for the operator's closing
/// line and exit code.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// `guard.lock` was held by another program: the run read nothing, called nothing, logged
    /// nothing and printed nothing, and every count below is 0. The operator exits 0.
    pub lock_held: bool,
    /// `<m>`: the host registry's records.
    pub hosts: usize,
    /// `<r>`: the hosts step 6 read as `running`.
    pub running: usize,
    /// `<s>`: the hosts this run stopped.
    pub stopped: usize,
    /// `<u>`: the hosts this run logged `unexplained`.
    pub unexplained: usize,
    /// `<n>`: the hosts whose part failed. The operator prints `-All`'s count line and exits 1
    /// when it is more than 0, and exits 0 otherwise.
    pub failed: usize,
    /// A `guard.log` line was dropped (operator.md, guard, Its log). The run printed
    /// `agentspace: Cannot write the guard's log <path>: <error>.` once, through `say.warning`, at
    /// the first line it dropped, and went on. The operator exits 1 when it is `true`, whatever
    /// `failed` is, and prints the count line only when `failed` is more than 0 (architect,
    /// 2026-11-22, for v0.08-T03).
    pub log_dropped: bool,
}

/// One run of the guard (operator.md, guard, steps 1 to 8).
///
/// - **`Ok(report)`** for a run that went to its end, including one that found `guard.lock` held
///   ([`Report::lock_held`]) and one in which hosts' parts failed ([`Report::failed`]). The run
///   has printed its own lines through `say`, the closing `Hosts:` line among them unless
///   `-Quiet`, and logged to `guard.log`; the operator prints nothing more but the count line of a
///   failed host, and exits by the report.
/// - **A host's failure, and a dropped log line, go through `say.warning`**, which the guard uses
///   for failures alone: operator.md gives it no other warning. The operator's `Say` for `guard`
///   MUST print `warning` whatever `-Quiet` says, and suppress `progress` under it (architect,
///   2026-11-22, for v0.08-T03).
/// - **`Err(failure)`** for a failure that ends the run, with the message operator.md gives:
///   `guard.lock` that cannot be taken, a host registry or lease state file that cannot be read,
///   a failed `ec2 describe-instances`, and #0034's sign-in message from any call. The run has
///   logged it; the operator prints the failure and exits 1.
pub fn run(input: &Input, say: &dyn Say, aws: &dyn Aws) -> Result<Report, Failure> {
    let _ = (input, say, aws);
    Err(Failure {
        message: "agentspace: guard is not built yet.".to_string(),
        caller: CallerFailure::Same,
    })
}

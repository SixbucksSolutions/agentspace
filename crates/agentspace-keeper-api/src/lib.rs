//! The pinned boundary of the keeper's crates (docs/keeper.md, docs/state.md; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The architect committed this crate before v0.06-T01 from the pin on cards #0116 and #0117, so
//! that T01 calls it with fakes while T02 implements it on Windows. A change to it is the
//! architect's. It holds no logic.

#![forbid(unsafe_code)]

use std::any::Any;
use std::path::Path;
use std::time::Duration;

/// `<state folder>\board-holder.json` (docs/state.md, The holder record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HolderRecord {
    pub pid: u32,
    /// The holder's creation time in UTC, `YYYY-MM-DDTHH:MM:SS.fffZ`.
    pub process_started_at: String,
    /// The lease ID, normalized.
    pub lease_id: String,
    /// The lease's `ssh.host_alias`, which the holder was started with.
    pub alias: String,
}

/// What a query reads from a running process (keeper.md, Trusting the record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    /// Its creation time in UTC, truncated to the millisecond, `YYYY-MM-DDTHH:MM:SS.fffZ`.
    pub process_started_at: String,
    /// Its command line read back, or `None` when it could not be read.
    pub command_line: Option<String>,
}

/// The answer to a query of one PID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Running(ProcessInfo),
    Gone,
    /// It runs but could not be opened to query it; the Windows error number.
    CannotOpen(u32),
}

/// What a stop found, one variant per row of keeper.md's stop table, in its order
/// (keeper.md, Stopping a holder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stopped {
    /// The holder, and every descendant the stop could open and check, stopped.
    All,
    /// Nothing stopped, and a new look finds the holder gone or no longer the recorded one, or
    /// cannot open it.
    Gone,
    /// It could not be opened with the right to terminate it, and still runs; the Windows error.
    CannotOpenToStop(u32),
    /// Nothing stopped for another reason, and it still runs: `<why>`.
    NotStopped(String),
    /// The stop met an error: `<error>`.
    Error(String),
    /// It still runs after it was terminated.
    StillRunning,
    /// The holder stopped, but these processes of its tree did not.
    TreeLeft(Vec<u32>),
}

/// The holder command and its matcher (keeper.md, The holder command).
pub trait Matcher {
    /// The holder's command line for this ssh program and alias.
    fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String;
    /// Whether a command line read back from a process is that holder command.
    fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool;
    /// The matcher's check against its own cases, run at the start of each pass.
    fn self_check(&self) -> Result<(), String>;
}

/// The processes the keeper looks at and stops.
pub trait Processes {
    fn query(&self, pid: u32) -> Query;
    /// Stops the recorded holder's tree, checking each process on its own handle.
    fn stop(&self, holder: &HolderRecord, ssh_exe: &Path, matcher: &dyn Matcher) -> Stopped;
}

/// What a pass hands the starter (keeper.md, Starting a holder).
#[derive(Debug, Clone, Copy)]
pub struct StartRequest<'a> {
    /// The state folder, a full path.
    pub state_dir: &'a Path,
    /// The holder's working folder: the pass's `%USERPROFILE%`.
    pub profile: &'a Path,
    /// `-SshExe`.
    pub ssh_exe: &'a Path,
    pub lease_id: &'a str,
    pub alias: &'a str,
    /// The keeper's own program, which the launcher runs as (D42).
    pub own_exe: &'a Path,
    /// The holder's log, a full path: the log of the host the holder is started on (keeper.md,
    /// The handshake, steps 1 and 7; architect, 2026-11-21, for v0.07-T03). `None` is the box's
    /// `board-holder-ssh.log` in `state_dir`, so a caller that names no log MUST get the same
    /// request bytes and messages as before this member existed. Another host's start MUST name
    /// its own `board-holder-ssh.<project>.log`, which the starter writes into the launch request
    /// and names in its "exited at once" reason.
    pub log: Option<&'a Path>,
}

/// The outcome of a start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// The holder that started, not yet recorded.
    Holder(HolderRecord),
    /// The whole `reason=` text keeper.md gives for the failure.
    Failed(String),
}

/// The launch handshake (keeper.md, Starting a holder).
pub trait Starter {
    fn start(&self, req: &StartRequest, matcher: &dyn Matcher) -> Started;
}

/// The outcome of taking a lock (keeper.md, Locks).
pub enum Taken {
    /// Held until this value is dropped.
    Held(Box<dyn Any>),
    /// The keeper lock, held by another; the PID read from it, if it gave one.
    Busy(Option<u32>),
    /// The pass lock, not taken within 10 seconds.
    Skipped,
    /// Any other error taking it: `<error>`.
    Failed(String),
}

/// `board-holder.lock` and `board-holder-keeper.lock` (docs/state.md, The keeper's lock files).
pub trait Locks {
    fn pass(&self, state_dir: &Path) -> Taken;
    fn keeper(&self, state_dir: &Path) -> Taken;
}

/// The keeper's clocks (keeper.md, Times).
pub trait Clock {
    /// The UTC clock, in milliseconds since the Unix epoch.
    fn utc_now_ms(&self) -> i64;
    /// A clock that local time and the time zone never move.
    fn monotonic(&self) -> Duration;
    fn sleep(&self, d: Duration);
}

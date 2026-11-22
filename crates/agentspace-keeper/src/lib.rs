//! The keeper's front and its pass (docs/keeper.md; D42).
//!
//! RFC 2119 keywords in this crate's comments, and the capitals are load-bearing.
//!
//! `agentspace-keeper.exe` holds the board's forwards on each host through one ready lease of
//! that host (keeper.md, What the keeper does; D50). This crate is its portable half: the command
//! line, one pass with a turn per host, and the log lines. What a pass asks of Windows is behind
//! the pinned boundary in `agentspace-keeper-api`, and each host's holder record is behind
//! [`Records`], this crate's own trait (D42, the architect's ruling for E0.06-T01-S06). The host
//! registry is read through `agentspace-hosts`, without its lock (keeper.md, Hosts).
//!
//! # The public API
//!
//! - [`run`]: the whole program, given its arguments, its [`Env`], its [`Deps`] and its two
//!   streams; it returns the exit code. With `-Watch` it is a keeper: the keeper lock, then a
//!   loop of passes with the look, the interval, the backoff and the release grace (keeper.md,
//!   Watching), every duration on `Clock::monotonic` and `Clock::sleep`.
//! - [`parse_args`]: the command line and its usage checks, giving [`Options`] (with
//!   [`WatchOptions`]) or the usage error's message; [`USAGE`] is the two usage lines, and
//!   [`PROGRAM`] the name they and every message use.
//! - [`run_pass`]: one pass, given a [`PassContext`] and the [`Deps`]; it returns a
//!   [`PassReport`], whose [`Action`] a keeper (`-Watch`) tracks by.
//! - [`Records`]: the holder record's file, read, written and removed; [`FileRecords`] is the
//!   real one.
//! - [`Deps`]: the pinned traits and [`Records`], as one pass uses them, and the keeper's
//!   [`GuardStarter`].
//! - [`GuardStarter`]: the keeper's seam for the guard's start, the try of `guard.lock` and the
//!   detached start (keeper.md, Starting the guard; D56, D63); [`DetachGuard`] is the real one,
//!   over `agentspace-detach`, and a test gives its own, which starts nothing.
//!
//! A library: it writes to no stream but the ones it is given, and reads no environment but
//! through [`Env`].

#![forbid(unsafe_code)]

mod args;
mod front;
mod guard;
mod hosts;
mod log;
mod pass;
mod records;
mod watch;

use agentspace_keeper_api::{Clock, HolderRecord, Locks, Matcher, Processes, Starter};
use std::path::Path;

pub use args::{Options, PROGRAM, USAGE, WatchOptions, parse_args};
pub use front::{Env, run};
pub use guard::{DetachGuard, GUARD_CMD, GuardLockTry, GuardStarter, START_BOUND};
pub use pass::{Action, PassContext, PassReport, run_pass};
pub use records::FileRecords;

/// The holder records: `<state folder>\board-holder.json` for the box (docs/state.md, The holder
/// record), and `<state folder>\board-holder.<project>.json` for another host (state.md, Another
/// host's holder).
///
/// The real one is [`FileRecords`], over `agentspace-keeper-files` and `agentspace-state`
/// (E0.06-T03-S04, #0129); a pass is tested against a fake.
///
/// `read`, `write` and `remove` are the box's record, as in v0.06. The `_host` methods take the
/// host as `agentspace-keeper-files` names one, `None` for the box and `Some(project)` for another
/// host; a pass calls only them. Their defaults (E0.07-T03-S05) give the box's to the three
/// methods above, and another host's to that host's real file in the state folder, so a fake of
/// v0.06's three methods still serves the box, and another host's record is a file a test can
/// see in its throwaway state folder.
pub trait Records {
    /// The box's record, or `None` when there is none. A record that cannot be read or parsed,
    /// or whose members do not have their forms, MUST be read as `None` (state.md, The holder
    /// record).
    fn read(&self, state_dir: &Path) -> Option<HolderRecord>;
    /// Writes the box's record, replacing any other atomically. The error is the `<error>` of
    /// keeper.md's two texts for a record that cannot be written.
    fn write(&self, state_dir: &Path, record: &HolderRecord) -> Result<(), String>;
    /// Removes the box's record. A record that is already gone MUST be `Ok`.
    fn remove(&self, state_dir: &Path) -> Result<(), String>;

    /// A host's record, read as [`Records::read`] reads the box's.
    fn read_host(&self, state_dir: &Path, project: Option<&str>) -> Option<HolderRecord> {
        match project {
            None => self.read(state_dir),
            Some(_) => records::read_file(state_dir, project),
        }
    }
    /// Writes a host's record, as [`Records::write`] writes the box's.
    fn write_host(
        &self,
        state_dir: &Path,
        project: Option<&str>,
        record: &HolderRecord,
    ) -> Result<(), String> {
        match project {
            None => self.write(state_dir, record),
            Some(_) => records::write_file(state_dir, project, record),
        }
    }
    /// Removes a host's record, as [`Records::remove`] removes the box's.
    fn remove_host(&self, state_dir: &Path, project: Option<&str>) -> Result<(), String> {
        match project {
            None => self.remove(state_dir),
            Some(_) => records::remove_file(state_dir, project),
        }
    }
}

/// What a pass works through: the pinned keeper boundary and the record; and what a keeper's
/// start of the guard works through.
#[derive(Clone, Copy)]
pub struct Deps<'a> {
    pub processes: &'a dyn Processes,
    pub matcher: &'a dyn Matcher,
    pub starter: &'a dyn Starter,
    pub locks: &'a dyn Locks,
    pub clock: &'a dyn Clock,
    pub records: &'a dyn Records,
    /// The guard's start (keeper.md, Starting the guard). Only a keeper (`-Watch`) uses it; a pass
    /// and a one-pass run never do. A test MUST give a fake that starts no real program, and MUST
    /// still set `AGENTSPACE_GUARD_CMD` to a stand-in in its throwaway home (conformance.md, The
    /// harness; pin P15).
    pub guard: &'a dyn GuardStarter,
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod watch_tests;

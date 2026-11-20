//! The keeper's front and its pass (docs/keeper.md; D42).
//!
//! RFC 2119 keywords in this crate's comments, and the capitals are load-bearing.
//!
//! `agentspace-keeper.exe` holds the board's forwards through one ready lease (keeper.md, What
//! the keeper does). This crate is its portable half: the command line, one pass, and the log
//! lines. What a pass asks of Windows is behind the pinned boundary in `agentspace-keeper-api`,
//! and the holder record's file is behind [`Records`], this crate's own trait (D42, the
//! architect's ruling for E0.06-T01-S06).
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
//! - [`Deps`]: the pinned traits and [`Records`], as one pass uses them.
//!
//! A library: it writes to no stream but the ones it is given, and reads no environment but
//! through [`Env`].

#![forbid(unsafe_code)]

mod args;
mod front;
mod log;
mod pass;
mod records;
mod watch;

use agentspace_keeper_api::{Clock, HolderRecord, Locks, Matcher, Processes, Starter};
use std::path::Path;

pub use args::{Options, PROGRAM, USAGE, WatchOptions, parse_args};
pub use front::{Env, run};
pub use pass::{Action, PassContext, PassReport, run_pass};
pub use records::FileRecords;

/// `<state folder>\board-holder.json`, the holder record (docs/state.md, The holder record).
///
/// The real one is [`FileRecords`], over `agentspace-keeper-files` and `agentspace-state`
/// (E0.06-T03-S04, #0129); a pass is tested against a fake.
pub trait Records {
    /// The record, or `None` when there is none. A record that cannot be read or parsed, or
    /// whose members do not have their forms, MUST be read as `None` (state.md, The holder
    /// record).
    fn read(&self, state_dir: &Path) -> Option<HolderRecord>;
    /// Writes the record, replacing any other atomically. The error is the `<error>` of
    /// keeper.md's two texts for a record that cannot be written.
    fn write(&self, state_dir: &Path, record: &HolderRecord) -> Result<(), String>;
    /// Removes the record. A record that is already gone MUST be `Ok`.
    fn remove(&self, state_dir: &Path) -> Result<(), String>;
}

/// What a pass works through: the pinned keeper boundary and the record.
#[derive(Clone, Copy)]
pub struct Deps<'a> {
    pub processes: &'a dyn Processes,
    pub matcher: &'a dyn Matcher,
    pub starter: &'a dyn Starter,
    pub locks: &'a dyn Locks,
    pub clock: &'a dyn Clock,
    pub records: &'a dyn Records,
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod watch_tests;

//! The keeper's processes and locks on Windows (docs/keeper.md; docs/state.md; D42).
//!
//! RFC 2119 keywords in this crate's comments, and the capitals are load-bearing.
//!
//! This crate implements two traits of the pinned keeper boundary, `agentspace-keeper-api`:
//!
//! | Item | What it is |
//! |---|---|
//! | `WinProcesses` | The pinned `Processes`: `query` reads a process's creation time and command line (keeper.md, Trusting the record), and `stop` ends a trusted holder's tree and nothing else (keeper.md, Stopping a holder). |
//! | `WinLocks` | The pinned `Locks`: `board-holder.lock` for a pass, with its 10-second wait, and `board-holder-keeper.lock` for a keeper, with its PID (keeper.md, Locks; state.md, The keeper's lock files). |
//! | `system_folder` | The Windows system folder as Windows reports it, the `<System32>` of keeper.md's holder command. |
//!
//! # Where it builds
//!
//! The keeper runs on Windows only (keeper.md, Where it runs), so the three items above exist only
//! when the target is Windows. On any other target the crate builds with its portable half alone:
//! the creation time's form, and the logic of a query and a stop over the `Os` seam, which its unit
//! tests drive with a fake on every platform. So the workspace still builds and tests on Linux.
//!
//! # Unsafe code
//!
//! Only `sys`, the one module that calls Windows, MAY use `unsafe`; the rest of the crate denies
//! it. A library crate: it never writes to stdout or stderr, and never panics on bad input.

#![deny(unsafe_code)]

mod process;
mod time;

#[cfg(windows)]
mod lock;
#[cfg(windows)]
#[allow(unsafe_code)]
mod sys;

#[cfg(all(test, windows))]
mod win_tests;

#[cfg(windows)]
pub use lock::WinLocks;
#[cfg(windows)]
pub use sys::{WinProcesses, system_folder};

use std::time::Duration;

/// How long a stop waits for each process of the tree to exit, in each round (keeper.md, Stopping
/// a holder).
pub const STOP_WAIT_PER_PROCESS: Duration = Duration::from_secs(2);

/// How many times a stop walks the tree and terminates what it found (keeper.md, Stopping a
/// holder).
pub const STOP_ROUNDS: u32 = 5;

/// How long a pass waits for `board-holder.lock` before it is skipped (keeper.md,
/// `board-holder.lock`).
pub const PASS_LOCK_WAIT: Duration = Duration::from_secs(10);

/// How often a pass tries `board-holder.lock` again while it waits.
pub const PASS_LOCK_RETRY: Duration = Duration::from_millis(100);

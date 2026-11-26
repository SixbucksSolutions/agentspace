//! The ProxyCommand's calls on a system that is not Windows. These are NOT THE CONTRACT'S: they
//! exist so that the crate builds and its portable logic is tested on Linux (the laptop is
//! Windows).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::path::Path;

/// Opens the lease state file for reading. No other system has Windows's share modes.
pub fn open_shared(path: &Path) -> io::Result<File> {
    File::open(path)
}

/// No other system refuses an open on a sharing violation.
pub fn is_sharing_violation(_error: &io::Error) -> bool {
    false
}

/// Does nothing: the PATH folders are Windows paths. The tunnel's own command gets its `PATH`
/// whatever the system.
pub fn set_path(_value: &OsStr) -> bool {
    false
}

/// Guards nothing: nothing `up`'s steps start writes to stdout (operator.md, Streams).
pub struct StdoutGuard;

impl StdoutGuard {
    pub fn new() -> StdoutGuard {
        StdoutGuard
    }

    pub fn restore(&mut self) {}
}

/// Does nothing: only a Windows console has the output mode ssh needs repaired.
pub fn repair_console() {}

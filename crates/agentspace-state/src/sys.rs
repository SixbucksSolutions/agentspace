//! The system half, chosen by platform: the Windows half is the contract's, and the other is std's
//! nearest equivalent (unix.rs), which is not.

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(not(windows))]
#[path = "unix.rs"]
mod imp;

pub use imp::{create_temp_file, open_lock, open_read, replace};

/// Why the lock file could not be opened.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenFailure {
    /// Another process holds it: the only failure that is tried again.
    Held,
    /// Anything else, such as access denied or a folder at its path.
    Other,
}

/// Why a file could not be moved over another.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplaceFailure {
    /// Another process holds a file without sharing delete: tried again, up to a deadline. Only
    /// the Windows half makes it.
    #[cfg_attr(not(windows), allow(dead_code))]
    Held,
    /// Anything else: fails at once.
    Other,
}

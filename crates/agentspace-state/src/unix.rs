//! The same interface as windows.rs for a system other than Windows, with std's nearest
//! equivalents. NOT THE CONTRACT'S: contract v0's caller runs on Windows (state.md, D8), and
//! nothing here excludes the prototype, which locks with a share mode. It exists so that the crate
//! builds, and so a later caller has a place to start.

use crate::sys::{OpenFailure, ReplaceFailure};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;

/// Opens `leases.lock` and takes an advisory lock on it, which is let go when the file is closed.
pub fn open_lock(path: &Path) -> Result<File, OpenFailure> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|_| OpenFailure::Other)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(OpenFailure::Held),
        Err(TryLockError::Error(_)) => Err(OpenFailure::Other),
    }
}

/// Opens a file for reading. There is no share mode here.
pub fn open_read(path: &Path) -> io::Result<File> {
    File::open(path)
}

/// Renames `from` over `to`, which is atomic on these systems. It is one attempt, since nothing
/// here holds a file against a rename.
pub fn replace(from: &Path, to: &Path) -> Result<(), ReplaceFailure> {
    fs::rename(from, to).map_err(|_| ReplaceFailure::Other)
}

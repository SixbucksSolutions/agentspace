//! Opening the lock files and reading, the way docs/state.md says (The host registry lock, The
//! provisioning run's lock, Replacing a file). Everything here is safe std: on Windows the share
//! mode is `OpenOptionsExt::share_mode`, and elsewhere an advisory lock, which is not the contract's
//! (docs/state.md, D8). std opens every handle non-inheritable, so no program a process starts
//! holds a lock.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Why a lock file could not be opened.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenFailure {
    /// Another process holds it: the only failure that is tried again.
    Held,
    /// The file is missing, and the caller asked for no creation.
    Missing,
    /// Anything else, such as access denied or a folder at its path.
    Other,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    /// ERROR_SHARING_VIOLATION and ERROR_LOCK_VIOLATION: another process holds the file.
    const SHARING_VIOLATION: i32 = 32;
    const LOCK_VIOLATION: i32 = 33;
    /// FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE.
    const SHARE_ALL: u32 = 1 | 2 | 4;

    /// Opens a lock file for reading and writing with share mode 0.
    pub fn open_lock(path: &Path, create: bool) -> Result<File, OpenFailure> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .truncate(false)
            .share_mode(0)
            .open(path)
            .map_err(|e| match e.raw_os_error() {
                Some(SHARING_VIOLATION | LOCK_VIOLATION) => OpenFailure::Held,
                _ if e.kind() == io::ErrorKind::NotFound => OpenFailure::Missing,
                _ => OpenFailure::Other,
            })
    }

    /// Opens a file for reading sharing read, write and delete, so that a reader never blocks a
    /// writer's rename.
    pub fn open_read(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .share_mode(SHARE_ALL)
            .open(path)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::fs::TryLockError;

    /// Opens a lock file for reading and writing and takes an advisory lock on it, let go when the
    /// file is closed.
    pub fn open_lock(path: &Path, create: bool) -> Result<File, OpenFailure> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .truncate(false)
            .open(path)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    OpenFailure::Missing
                } else {
                    OpenFailure::Other
                }
            })?;
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
}

pub use imp::{open_lock, open_read};

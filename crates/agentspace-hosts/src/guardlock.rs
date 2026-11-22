//! The guard's lock (docs/state.md, The guard's lock): held for the whole life of a guard run, so a
//! held lock means a guard is running (D56, D63). The keeper only tries it and never holds it.

use crate::error::{Failure, HostsError};
use crate::folder;
use crate::paths::{agentspace_dir, guard_lock_path};
use crate::sys::{self, OpenFailure};
use std::fs::File;
use std::path::Path;

/// The guard's lock, held until this is dropped. It is held with the file open and no sharing, so
/// a second process's `try_guard_lock` finds it held. The operating system lets it go however the
/// process ends, so a killed guard leaves the lock free. The handle is not inheritable, so no
/// program the guard starts holds it.
pub struct GuardLock {
    _file: File,
}

/// Prints the type's name alone: the file's own `Debug` carries its full path, which MUST NOT reach
/// a log or a failed assertion. The type has no `Display`.
impl std::fmt::Debug for GuardLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GuardLock { .. }")
    }
}

/// Tries the guard's lock once, without waiting: `Some(lock)` when this call has it, held until
/// the lock is dropped, and `None` when another process or another handle holds it. The taker
/// makes `<home>\.agentspace` first when it is missing. `guard.lock` is never deleted, and its
/// content is empty.
///
/// A failure to open it other than its being held, such as access denied or a folder at its path,
/// or a file where the folder belongs, is an error and not a free lock: the caller runs nothing.
pub fn try_guard_lock(home: &Path) -> Result<Option<GuardLock>, HostsError> {
    folder::ensure(&agentspace_dir(home)).map_err(HostsError::CannotLockGuard)?;
    match sys::open_lock(&guard_lock_path(home), true) {
        Ok(file) => Ok(Some(GuardLock { _file: file })),
        Err(OpenFailure::Held) => Ok(None),
        Err(OpenFailure::Missing | OpenFailure::Other) => {
            Err(HostsError::CannotLockGuard(Failure::CannotOpenLock))
        }
    }
}

/// Whether a guard is running: tries the lock and lets it go at once if it got it. A missing lock
/// file, or folder, is a lock nobody holds, and this call creates neither. The same errors as
/// `try_guard_lock` otherwise.
pub fn guard_is_running(home: &Path) -> Result<bool, HostsError> {
    if folder::is_blocked(&agentspace_dir(home)) {
        return Err(HostsError::CannotLockGuard(Failure::FolderIsAFile));
    }
    match sys::open_lock(&guard_lock_path(home), false) {
        Ok(_free) => Ok(false),
        Err(OpenFailure::Held) => Ok(true),
        Err(OpenFailure::Missing) => Ok(false),
        Err(OpenFailure::Other) => Err(HostsError::CannotLockGuard(Failure::CannotOpenLock)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn the_lock_is_held_until_dropped_and_a_second_try_finds_it_held() {
        let t = tempfile::tempdir().unwrap();
        let lock = try_guard_lock(t.path()).unwrap().unwrap();
        assert!(guard_lock_path(t.path()).is_file());
        assert!(guard_is_running(t.path()).unwrap());
        assert!(try_guard_lock(t.path()).unwrap().is_none());
        // Another handle in another thread finds it held too, without waiting.
        let path = t.path().to_path_buf();
        let other = std::thread::spawn(move || try_guard_lock(&path).map(|l| l.is_some()))
            .join()
            .unwrap();
        assert_eq!(other, Ok(false));
        drop(lock);
        assert!(!guard_is_running(t.path()).unwrap());
        let again = try_guard_lock(t.path()).unwrap();
        assert!(again.is_some());
        assert!(guard_lock_path(t.path()).is_file(), "Never deleted");
    }

    #[test]
    fn the_lock_is_released_on_drop() {
        let t = tempfile::tempdir().unwrap();
        {
            let _held = try_guard_lock(t.path()).unwrap().unwrap();
            assert!(guard_is_running(t.path()).unwrap());
        }
        assert!(!guard_is_running(t.path()).unwrap());
        assert!(try_guard_lock(t.path()).unwrap().is_some());
    }

    #[test]
    fn asking_creates_neither_the_file_nor_the_folder() {
        let t = tempfile::tempdir().unwrap();
        assert!(!guard_is_running(t.path()).unwrap());
        assert!(!agentspace_dir(t.path()).exists());
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 0);
        // With the folder present and the file missing, the file is still not created.
        fs::create_dir_all(agentspace_dir(t.path())).unwrap();
        assert!(!guard_is_running(t.path()).unwrap());
        assert!(!guard_lock_path(t.path()).exists());
    }

    #[test]
    fn taking_makes_the_folder_and_an_empty_file() {
        let t = tempfile::tempdir().unwrap();
        let _held = try_guard_lock(t.path()).unwrap().unwrap();
        assert!(agentspace_dir(t.path()).is_dir());
        assert_eq!(fs::metadata(guard_lock_path(t.path())).unwrap().len(), 0);
    }

    #[test]
    fn debug_prints_the_name_and_no_path() {
        let t = tempfile::tempdir().unwrap();
        let held = try_guard_lock(t.path()).unwrap().unwrap();
        assert_eq!(format!("{held:?}"), "GuardLock { .. }");
    }

    #[test]
    fn a_folder_at_the_lock_is_an_error_not_a_free_lock() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(guard_lock_path(t.path())).unwrap();
        assert_eq!(
            try_guard_lock(t.path()).unwrap_err(),
            HostsError::CannotLockGuard(Failure::CannotOpenLock)
        );
        assert_eq!(
            guard_is_running(t.path()).unwrap_err(),
            HostsError::CannotLockGuard(Failure::CannotOpenLock)
        );
        assert!(guard_lock_path(t.path()).is_dir(), "Left as it was");
    }

    #[test]
    fn a_file_where_the_folder_belongs_is_an_error_and_is_left_alone() {
        let t = tempfile::tempdir().unwrap();
        fs::write(agentspace_dir(t.path()), b"x").unwrap();
        assert_eq!(
            try_guard_lock(t.path()).unwrap_err(),
            HostsError::CannotLockGuard(Failure::FolderIsAFile)
        );
        assert_eq!(
            guard_is_running(t.path()).unwrap_err(),
            HostsError::CannotLockGuard(Failure::FolderIsAFile)
        );
        assert_eq!(fs::read(agentspace_dir(t.path())).unwrap(), b"x");
    }
}

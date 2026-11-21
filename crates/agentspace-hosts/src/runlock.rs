//! A host's provisioning run lock (docs/state.md, The provisioning run's lock): held for the whole
//! life of a run, so a held lock means a run is running (D55).

use crate::error::{Failure, HostsError};
use crate::folder;
use crate::model::is_normalized_project;
use crate::paths::{host_dir, host_run_lock_path};
use crate::sys::{self, OpenFailure};
use std::fs::File;
use std::path::Path;

/// A host's run lock, held until this is dropped. It is held with the file open and no sharing, so
/// a second process's `try_run_lock` finds it held. The operating system lets it go however the
/// process ends, so a killed run leaves the lock free. The handle is not inheritable, so no
/// program the run starts holds it.
pub struct RunLock {
    _file: File,
}

/// Prints the type's name alone: the file's own `Debug` carries its full path, which MUST NOT reach
/// a log or a failed assertion. The type has no `Display`.
impl std::fmt::Debug for RunLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RunLock { .. }")
    }
}

/// Tries a host's run lock once, without waiting: `Some(lock)` when this call has it, held until
/// the lock is dropped, and `None` when another process or another handle holds it. The taker
/// makes the host's folder first when it is missing. `provision.lock` is never deleted, and its
/// content is empty.
///
/// A failure to open it other than its being held, such as access denied or a folder at its path,
/// or a file where a folder belongs, is an error and not a free lock: the caller starts or runs
/// nothing (docs/state.md). A `project` that is not in the contract's normalized form is
/// `BadProject`, and no path is made from it.
pub fn try_run_lock(home: &Path, project: &str) -> Result<Option<RunLock>, HostsError> {
    if !is_normalized_project(project) {
        return Err(HostsError::BadProject);
    }
    folder::ensure(&host_dir(home, project)).map_err(HostsError::CannotLockRun)?;
    match sys::open_lock(&host_run_lock_path(home, project), true) {
        Ok(file) => Ok(Some(RunLock { _file: file })),
        Err(OpenFailure::Held) => Ok(None),
        Err(OpenFailure::Missing | OpenFailure::Other) => {
            Err(HostsError::CannotLockRun(Failure::CannotOpenLock))
        }
    }
}

/// Whether a run is running for the host: tries the lock and lets it go at once if it got it. A
/// missing lock file, or folder, is a lock nobody holds, and this call creates neither. The same
/// errors as `try_run_lock` otherwise.
pub fn run_is_running(home: &Path, project: &str) -> Result<bool, HostsError> {
    if !is_normalized_project(project) {
        return Err(HostsError::BadProject);
    }
    if folder::is_blocked(&host_dir(home, project)) {
        return Err(HostsError::CannotLockRun(Failure::FolderIsAFile));
    }
    match sys::open_lock(&host_run_lock_path(home, project), false) {
        Ok(_free) => Ok(false),
        Err(OpenFailure::Held) => Ok(true),
        Err(OpenFailure::Missing) => Ok(false),
        Err(OpenFailure::Other) => Err(HostsError::CannotLockRun(Failure::CannotOpenLock)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::hosts_dir;
    use std::fs;

    #[test]
    fn the_lock_is_held_until_dropped_and_a_second_try_finds_it_held() {
        let t = tempfile::tempdir().unwrap();
        assert!(!run_is_running(t.path(), "agentspace").unwrap());
        assert!(!hosts_dir(t.path()).exists(), "Asking creates nothing");
        let lock = try_run_lock(t.path(), "agentspace").unwrap().unwrap();
        assert!(host_run_lock_path(t.path(), "agentspace").is_file());
        assert!(run_is_running(t.path(), "agentspace").unwrap());
        assert!(try_run_lock(t.path(), "agentspace").unwrap().is_none());
        // Another host's lock is another file.
        let other = try_run_lock(t.path(), "example-two").unwrap().unwrap();
        assert!(!run_is_running(t.path(), "unrelated").unwrap());
        drop(lock);
        assert!(!run_is_running(t.path(), "agentspace").unwrap());
        assert!(try_run_lock(t.path(), "agentspace").unwrap().is_some());
        assert!(
            host_run_lock_path(t.path(), "agentspace").is_file(),
            "Never deleted"
        );
        drop(other);
    }

    #[test]
    fn debug_prints_the_name_and_no_path() {
        let t = tempfile::tempdir().unwrap();
        let held = try_run_lock(t.path(), "agentspace").unwrap().unwrap();
        let shown = format!("{held:?}");
        assert!(!shown.contains(std::path::MAIN_SEPARATOR));
        assert!(!shown.contains('/'));
        let temp_name = t.path().file_name().unwrap().to_string_lossy().into_owned();
        assert!(!shown.contains(&temp_name));
        assert!(!shown.contains("provision.lock"));
    }

    #[test]
    fn a_folder_or_a_file_in_the_way_is_an_error_not_a_free_lock() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(host_run_lock_path(t.path(), "agentspace")).unwrap();
        assert_eq!(
            try_run_lock(t.path(), "agentspace").unwrap_err(),
            HostsError::CannotLockRun(Failure::CannotOpenLock)
        );
        assert_eq!(
            run_is_running(t.path(), "agentspace").unwrap_err(),
            HostsError::CannotLockRun(Failure::CannotOpenLock)
        );
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(crate::paths::agentspace_dir(t.path())).unwrap();
        fs::write(hosts_dir(t.path()), b"x").unwrap();
        assert_eq!(
            try_run_lock(t.path(), "agentspace").unwrap_err(),
            HostsError::CannotLockRun(Failure::FolderIsAFile)
        );
        assert_eq!(
            run_is_running(t.path(), "agentspace").unwrap_err(),
            HostsError::CannotLockRun(Failure::FolderIsAFile)
        );
        assert_eq!(fs::read(hosts_dir(t.path())).unwrap(), b"x");
    }

    #[test]
    fn a_project_that_is_not_normalized_makes_no_path() {
        let t = tempfile::tempdir().unwrap();
        for bad in ["", "..", "a/b", "A", "a\\b"] {
            assert_eq!(
                try_run_lock(t.path(), bad).unwrap_err(),
                HostsError::BadProject
            );
            assert_eq!(
                run_is_running(t.path(), bad).unwrap_err(),
                HostsError::BadProject
            );
        }
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 0);
    }
}

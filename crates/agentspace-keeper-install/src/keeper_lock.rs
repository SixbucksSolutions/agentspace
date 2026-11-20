//! Finding a running keeper by `board-holder-keeper.lock` (keeper.md, The installer, step 3, and
//! Locks; state.md, The keeper's lock files). The lock is the open handle, never the file's
//! content, so this tries to take it as a keeper does and reads the PID as a second keeper reads
//! it. It never reads `AGENTSPACE_STATE_DIR` and never matches a command line (FB-27): the caller
//! passes the folder, which is `%USERPROFILE%\.agentspace`.

use agentspace_keeper_files::KEEPER_LOCK_FILE;
use std::fs::{File, OpenOptions};
use std::path::Path;

/// Whether a keeper holds the keeper lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockState {
    /// The file is missing, or opening it as a keeper does succeeded.
    NotHeld,
    /// Another process has the file open. `pid` is the digits the file gives, as a second keeper
    /// reads them: `None` when it gives none.
    Held { pid: Option<String> },
}

/// Windows' share mode bits.
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 1;
#[cfg(windows)]
const FILE_SHARE_WRITE: u32 = 2;

/// `ERROR_SHARING_VIOLATION` and `ERROR_LOCK_VIOLATION`.
#[cfg(windows)]
const HELD_ERRORS: [i32; 2] = [32, 33];

/// Opens the lock file as a keeper does: for reading and writing, sharing read only, never
/// creating it.
fn open_as_a_keeper(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(FILE_SHARE_READ);
    }
    options.open(path)
}

#[cfg(windows)]
/// Opens the lock file as a reader of its PID does: for reading, sharing read and write.
fn open_to_read_the_pid(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    options.open(path)
}

#[cfg(windows)]
/// The PID digits the lock file gives, from the first 32 bytes it can read, as a second keeper
/// reads them: `None` when it gives none or cannot be opened.
fn read_pid(path: &Path) -> Option<String> {
    use agentspace_keeper_files::{KEEPER_PID_READ_BYTES, keeper_pid_digits};
    use std::io::Read;
    let file = open_to_read_the_pid(path).ok()?;
    let mut bytes = Vec::new();
    // A read that fails part way keeps what it read before the failure.
    let _ = file
        .take(KEEPER_PID_READ_BYTES as u64)
        .read_to_end(&mut bytes);
    keeper_pid_digits(&bytes)
}

/// Whether the keeper lock in `state_folder` is held. The lock is held when the file exists and
/// opening it as a keeper does fails because another process has it open; an open that succeeds
/// is closed at once. A missing file is not held, and the file is never created.
///
/// `Err` holds a message for any other failure to open the file.
pub fn check_keeper_lock(state_folder: &Path) -> Result<LockState, String> {
    let path = state_folder.join(KEEPER_LOCK_FILE);
    match open_as_a_keeper(&path) {
        // Closed at once, by the drop.
        Ok(_) => Ok(LockState::NotHeld),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LockState::NotHeld),
        #[cfg(windows)]
        Err(error)
            if error
                .raw_os_error()
                .is_some_and(|c| HELD_ERRORS.contains(&c)) =>
        {
            Ok(LockState::Held {
                pid: read_pid(&path),
            })
        }
        // Any other failure to open a lock file that exists ends the installer with exit 1 and
        // this message (architect, 2026-11-19, for E0.06-T02-S06).
        Err(error) => Err(format!(
            "The keeper lock {} could not be checked: {error}",
            path.display()
        )),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    fn throwaway(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("keeper-install-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// Takes the lock as a keeper does: created if missing, sharing read only.
    fn take_as_a_keeper(folder: &Path, content: &[u8]) -> File {
        use std::io::Write;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(FILE_SHARE_READ)
            .open(folder.join(KEEPER_LOCK_FILE))
            .unwrap();
        file.write_all(content).unwrap();
        file
    }

    #[test]
    fn a_missing_file_is_not_held_and_is_not_created() {
        let folder = throwaway("missing");
        assert_eq!(check_keeper_lock(&folder), Ok(LockState::NotHeld));
        assert!(!folder.join(KEEPER_LOCK_FILE).exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_missing_folder_is_not_held() {
        let folder = throwaway("nofolder").join("absent");
        assert_eq!(check_keeper_lock(&folder), Ok(LockState::NotHeld));
    }

    #[test]
    fn a_leftover_file_locks_nothing() {
        let folder = throwaway("leftover");
        std::fs::write(folder.join(KEEPER_LOCK_FILE), b"4242").unwrap();
        assert_eq!(check_keeper_lock(&folder), Ok(LockState::NotHeld));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_held_lock_gives_its_pid() {
        let folder = throwaway("held");
        let holder = take_as_a_keeper(&folder, b"4242");
        assert_eq!(
            check_keeper_lock(&folder),
            Ok(LockState::Held {
                pid: Some("4242".to_string())
            })
        );
        drop(holder);
        assert_eq!(check_keeper_lock(&folder), Ok(LockState::NotHeld));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_held_lock_with_no_pid_gives_none() {
        let folder = throwaway("nopid");
        let _holder = take_as_a_keeper(&folder, b"not a pid");
        assert_eq!(
            check_keeper_lock(&folder),
            Ok(LockState::Held { pid: None })
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_held_lock_with_an_empty_file_gives_none() {
        let folder = throwaway("empty");
        let _holder = take_as_a_keeper(&folder, b"");
        assert_eq!(
            check_keeper_lock(&folder),
            Ok(LockState::Held { pid: None })
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_lock_that_cannot_be_opened_for_another_reason_is_an_error() {
        let folder = throwaway("unopenable");
        // A folder where the file should be: opening it is refused with access denied.
        std::fs::create_dir_all(folder.join(KEEPER_LOCK_FILE)).unwrap();
        let error = check_keeper_lock(&folder).unwrap_err();
        assert!(error.starts_with(&format!(
            "The keeper lock {} could not be checked: ",
            folder.join(KEEPER_LOCK_FILE).display()
        )));
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The check opens as a keeper does, sharing read: a reader of the PID, which opens for
    /// reading and shares read and write, is not a keeper and does not hold the lock.
    #[test]
    fn a_pid_reader_does_not_hold_the_lock() {
        let folder = throwaway("reader");
        std::fs::write(folder.join(KEEPER_LOCK_FILE), b"4242").unwrap();
        let _reader = open_to_read_the_pid(&folder.join(KEEPER_LOCK_FILE)).unwrap();
        assert_eq!(check_keeper_lock(&folder), Ok(LockState::NotHeld));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_pid_with_blanks_around_it_is_trimmed() {
        let folder = throwaway("blanks");
        let _holder = take_as_a_keeper(&folder, b" 4242\r\n");
        assert_eq!(
            check_keeper_lock(&folder),
            Ok(LockState::Held {
                pid: Some("4242".to_string())
            })
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_check_itself_leaves_the_lock_free() {
        let folder = throwaway("free");
        std::fs::write(folder.join(KEEPER_LOCK_FILE), b"1").unwrap();
        assert_eq!(check_keeper_lock(&folder), Ok(LockState::NotHeld));
        // A keeper can still take it afterwards: the check closed its handle.
        let _holder = take_as_a_keeper(&folder, b"5");
        let _ = std::fs::remove_dir_all(&folder);
    }
}

//! The lease state file on the disk (docs/state.md, The lease state file, The lease state lock and
//! Replacing a file): the lock, the read and the atomic replace.

use crate::sys::{self, OpenFailure, ReplaceFailure};
use crate::{StateFile, parse, serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Why the lease state file could not be read, locked or written. Each string is the `<why>` of
/// the matching message in lease-api.md (Messages): agentspace's own words, with no path, user name
/// or other program's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// Another command held the lock until the wait ran out: "Records busy".
    Busy,
    /// The lock could not be opened for a reason that is not another command holding it:
    /// "Records cannot be locked".
    CannotLock(String),
    /// The file could not be read, or a folder is where a folder is not wanted: "Records
    /// unreadable", for a reason other than its content.
    Unreadable(String),
    /// The file's content is not valid: "Records unreadable", with `the state file is not valid`.
    NotValid(String),
    /// The file could not be written, or the change would not be valid: "Records unwritable".
    Unwritable(String),
}

/// A file where the state folder belongs, as `read`, `lock` and `write` each report it (architect,
/// 2026-11-19: each folder is checked where it is used).
const WHY_STATE_FOLDER_FILE: &str = "the state folder is a file";
const WHY_LOCK_FOLDER: &str = "the state folder could not be made";
const WHY_LOCK_OPEN: &str = "the lock file could not be opened";
const WHY_OPEN: &str = "the state file could not be opened";
const WHY_READ: &str = "the state file could not be read";
const WHY_NOT_A_FILE: &str = "a folder is where the state file belongs";
const WHY_WRITE_FOLDER: &str = "the state folder could not be made";
const WHY_WRITE_TEMP: &str = "the new state file could not be written";
const WHY_REPLACE: &str = "the state file could not be replaced";

/// How long a command waits for the lease state lock (state.md, The lease state lock).
const LOCK_WAIT: Duration = Duration::from_secs(30);
/// How long a rename that finds the file held is tried again (state.md, Replacing a file).
const REPLACE_WAIT: Duration = Duration::from_secs(10);
/// The gap between tries. The prototype uses 20 milliseconds for the file and 50 for the lock; a
/// rewrite MAY pick its own.
const RETRY_EVERY: Duration = Duration::from_millis(20);

/// The deadlines, apart from the contract's so that a test can shorten them.
#[derive(Clone, Copy)]
struct Timing {
    lock_wait: Duration,
    replace_wait: Duration,
    retry_every: Duration,
}

const CONTRACT: Timing = Timing {
    lock_wait: LOCK_WAIT,
    replace_wait: REPLACE_WAIT,
    retry_every: RETRY_EVERY,
};

/// The lease state lock, held until this is dropped. The operating system lets the lock go when
/// the handle closes, however the process ends.
#[derive(Debug)]
pub struct LockGuard {
    _file: File,
}

fn lock_path(state_dir: &Path) -> PathBuf {
    state_dir.join("leases.lock")
}

fn state_path(state_dir: &Path) -> PathBuf {
    state_dir.join("leases.json")
}

/// Waits until `wait` has passed since `start`, but no longer than one gap, and says whether time
/// is left to try again.
fn pause(start: Instant, wait: Duration, every: Duration) -> bool {
    let elapsed = start.elapsed();
    if elapsed >= wait {
        return false;
    }
    std::thread::sleep(every.min(wait - elapsed));
    true
}

/// Takes the lease state lock: `<state folder>\leases.lock`, created with the state folder when
/// missing, opened for reading and writing with share mode 0. Only a sharing or lock violation is
/// tried again, until 30 seconds have passed, and then the records are `Busy`. Any other failure,
/// such as access denied or a folder at the path, is `CannotLock` at once.
pub fn lock(state_dir: &Path) -> Result<LockGuard, StoreError> {
    lock_with(state_dir, CONTRACT)
}

/// Whether a file stands where the state folder, or a folder above it, belongs.
///
/// Walks up from the state folder (itself included) to its first existing ancestor, and says whether
/// that is something other than a folder. Architect, 2026-11-19 (docs/state.md, The home folder: a
/// value that names an existing file, not a folder, makes every command that needs that folder
/// fail): `USERPROFILE` naming a file and `AGENTSPACE_STATE_DIR` under a file are both caught
/// here, and "path not found" means no leases only when every existing ancestor is a folder.
///
/// A link is followed (`fs::metadata`), as the state folder MAY be a link to a folder; a link to a
/// file is a file, and a link that leads nowhere is missing. A path that is missing, or under a
/// file (the system's "not a directory"), moves the walk up one level; any other failure to inspect
/// ends it with `false`, as nothing is known. The walk stops at the root and never panics.
fn state_folder_is_blocked(state_dir: &Path) -> bool {
    for ancestor in state_dir.ancestors() {
        match fs::metadata(ancestor) {
            Ok(m) => return !m.is_dir(),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(_) => return false,
        }
    }
    false
}

fn lock_with(state_dir: &Path, timing: Timing) -> Result<LockGuard, StoreError> {
    if state_folder_is_blocked(state_dir) {
        return Err(StoreError::CannotLock(WHY_STATE_FOLDER_FILE.to_string()));
    }
    fs::create_dir_all(state_dir)
        .map_err(|_| StoreError::CannotLock(WHY_LOCK_FOLDER.to_string()))?;
    let path = lock_path(state_dir);
    let start = Instant::now();
    loop {
        match sys::open_lock(&path) {
            Ok(file) => return Ok(LockGuard { _file: file }),
            Err(OpenFailure::Other) => {
                return Err(StoreError::CannotLock(WHY_LOCK_OPEN.to_string()));
            }
            Err(OpenFailure::Held) => {
                if !pause(start, timing.lock_wait, timing.retry_every) {
                    return Err(StoreError::Busy);
                }
            }
        }
    }
}

/// Reads `<state folder>\leases.json`, sharing read, write and delete, so that it never blocks a
/// writer's rename and sees the old file or the new one. A missing file is no leases. A folder at
/// its path, or a file that cannot be opened or read, is `Unreadable` (gap FB-21), and a file that
/// does not validate is `NotValid`. It takes no lock.
pub fn read(state_dir: &Path) -> Result<StateFile, StoreError> {
    // A file where the state folder, or a folder above it, belongs is not a missing folder.
    if state_folder_is_blocked(state_dir) {
        return Err(StoreError::Unreadable(WHY_STATE_FOLDER_FILE.to_string()));
    }
    let path = state_path(state_dir);
    let mut file = match sys::open_read(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // A missing file is no leases, and so is a missing folder.
            return Ok(StateFile { leases: Vec::new() });
        }
        Err(_) => {
            let why = if path.is_dir() {
                WHY_NOT_A_FILE
            } else {
                WHY_OPEN
            };
            return Err(StoreError::Unreadable(why.to_string()));
        }
    };
    // Where a folder can be opened, as on other systems, the handle says so.
    if file.metadata().is_ok_and(|m| m.is_dir()) {
        return Err(StoreError::Unreadable(WHY_NOT_A_FILE.to_string()));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| StoreError::Unreadable(WHY_READ.to_string()))?;
    parse(&bytes).map_err(|e| StoreError::NotValid(e.0))
}

/// 128 random bits for a temporary file's name. Its job is to not repeat, not to be secret, and
/// `create_new` catches a repeat anyway.
fn random_bits() -> u128 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    h.write_u128(nanos);
    h.write_u32(std::process::id());
    h.write_u64(COUNT.fetch_add(1, Ordering::Relaxed));
    let high = h.finish();
    h.write_u8(1);
    let low = h.finish();
    (u128::from(high) << 64) | u128::from(low)
}

/// Makes `leases.json.<32 hex digits>.tmp` in the folder, a name a reader never takes for the
/// file, and returns it with the new file open.
fn create_temp(state_dir: &Path) -> Option<(PathBuf, File)> {
    for _ in 0..8 {
        let path = state_dir.join(format!("leases.json.{:032x}.tmp", random_bits()));
        match sys::create_temp_file(&path) {
            Ok(f) => return Some((path, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// Writes the state file: the bytes of `file` go into a temporary file in the same folder, are
/// flushed to the disk, and are moved over `leases.json` in one rename (on Windows a POSIX rename
/// through the temporary file's handle; see `sys::replace`). A rename that fails because another process holds the file
/// without sharing delete is tried again for up to 10 seconds, within the time the caller holds
/// the lock. The temporary file is deleted if anything fails before the move, and a failed
/// rename leaves none either. A change that would not validate, or that repeats an ID, is refused
/// before anything is written. `held` is the proof that the caller holds the lock; it is not
/// checked against the folder.
pub fn write(state_dir: &Path, file: &StateFile, held: &LockGuard) -> Result<(), StoreError> {
    write_with(state_dir, file, held, CONTRACT)
}

fn write_with(
    state_dir: &Path,
    file: &StateFile,
    _held: &LockGuard,
    timing: Timing,
) -> Result<(), StoreError> {
    let bytes = serialize(file).map_err(|e| StoreError::Unwritable(e.0))?;
    if state_folder_is_blocked(state_dir) {
        return Err(StoreError::Unwritable(WHY_STATE_FOLDER_FILE.to_string()));
    }
    fs::create_dir_all(state_dir)
        .map_err(|_| StoreError::Unwritable(WHY_WRITE_FOLDER.to_string()))?;
    let unwritable = |why: &str| StoreError::Unwritable(why.to_string());
    let Some((temp, mut handle)) = create_temp(state_dir) else {
        return Err(unwritable(WHY_WRITE_TEMP));
    };
    let remove_temp = || {
        let _ = fs::remove_file(&temp);
    };
    // Written and flushed to the disk (FlushFileBuffers). The handle stays open for the rename,
    // which Windows does through it, and is closed before the temporary file is deleted.
    if handle.write_all(&bytes).is_err() || handle.sync_all().is_err() {
        drop(handle);
        remove_temp();
        return Err(unwritable(WHY_WRITE_TEMP));
    }

    let target = state_path(state_dir);
    let start = Instant::now();
    loop {
        match sys::replace(&handle, &temp, &target) {
            Ok(()) => return Ok(()),
            Err(failure) => {
                // A folder at the path is not a held file, and waiting does not change it.
                if target.is_dir() {
                    drop(handle);
                    remove_temp();
                    return Err(unwritable(WHY_NOT_A_FILE));
                }
                if failure == ReplaceFailure::Other
                    || !pause(start, timing.replace_wait, timing.retry_every)
                {
                    drop(handle);
                    remove_temp();
                    return Err(unwritable(WHY_REPLACE));
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A throwaway folder under the temp folder, removed when dropped.
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new() -> TempDir {
            static COUNT: AtomicU64 = AtomicU64::new(0);
            let name = format!(
                "agentspace-state-unit-{}-{}",
                std::process::id(),
                COUNT.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(name);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TempDir;
    use super::*;
    use crate::Record;

    fn short() -> Timing {
        Timing {
            lock_wait: Duration::from_millis(300),
            replace_wait: Duration::from_millis(300),
            retry_every: Duration::from_millis(10),
        }
    }

    fn record(id: &str) -> Record {
        Record {
            sized_for: None,
            plan: None,
            plan_source: None,
            id: id.to_string(),
            project: "demo".to_string(),
            state: "pending".to_string(),
            platform: "linux/arm64".to_string(),
            profile: "agent-linux".to_string(),
            size: "m".to_string(),
            ttl_seconds: 3600,
            created_at: "2026-11-19T10:00:00Z".to_string(),
            expires_at: None,
            released_at: None,
            ssh: None,
            reasons: None,
            driver: Some("ec2".to_string()),
        }
    }

    const ID_A: &str = "0f0f0f0f-0f0f-4f0f-8f0f-0f0f0f0f0f0f";
    const ID_B: &str = "1a1a1a1a-1a1a-4a1a-8a1a-1a1a1a1a1a1a";

    fn file_of(ids: &[&str]) -> StateFile {
        StateFile {
            leases: ids.iter().map(|i| record(i)).collect(),
        }
    }

    fn temps(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn lock_creates_the_state_folder_and_the_lock_file_and_lets_go_on_drop() {
        let t = TempDir::new();
        let dir = t.path().join("a").join("b");
        let guard = lock_with(&dir, short()).unwrap();
        assert!(dir.join("leases.lock").is_file());
        // A second try while the first is held waits out the time, then is busy.
        let began = Instant::now();
        assert_eq!(lock_with(&dir, short()).unwrap_err(), StoreError::Busy);
        assert!(began.elapsed() >= Duration::from_millis(250));
        drop(guard);
        lock_with(&dir, short()).unwrap();
        // The file is never deleted.
        assert!(dir.join("leases.lock").is_file());
    }

    #[test]
    fn a_lock_held_for_a_short_while_is_waited_for() {
        let t = TempDir::new();
        let guard = lock_with(t.path(), short()).unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(guard);
        });
        let long = Timing {
            lock_wait: Duration::from_secs(5),
            ..short()
        };
        assert!(lock_with(t.path(), long).is_ok());
        releaser.join().unwrap();
    }

    /// Bounded by the lock's own deadline rather than a fixed figure, for the reason given on the
    /// state file's test below (#0189): with the gap between tries as long as the deadline, a lock
    /// that tried again returns only once `lock_wait` has passed.
    #[test]
    fn a_folder_at_the_lock_fails_at_once_and_is_not_busy() {
        let t = TempDir::new();
        fs::create_dir(t.path().join("leases.lock")).unwrap();
        let timing = Timing {
            lock_wait: Duration::from_secs(60),
            retry_every: Duration::from_secs(60),
            ..short()
        };
        let began = Instant::now();
        let e = lock_with(t.path(), timing).unwrap_err();
        let took = began.elapsed();
        assert_eq!(e, StoreError::CannotLock(WHY_LOCK_OPEN.to_string()));
        assert!(took < timing.lock_wait, "{took:?}");
    }

    #[test]
    fn a_file_at_the_state_folder_cannot_be_locked() {
        let t = TempDir::new();
        let f = t.path().join("a-file");
        fs::write(&f, b"x").unwrap();
        let e = lock_with(&f, short()).unwrap_err();
        assert_eq!(e, StoreError::CannotLock(WHY_STATE_FOLDER_FILE.to_string()));
    }

    #[test]
    fn a_missing_file_or_folder_is_no_leases() {
        let t = TempDir::new();
        assert_eq!(read(t.path()).unwrap().leases.len(), 0);
        assert_eq!(read(&t.path().join("nope")).unwrap().leases.len(), 0);
    }

    #[test]
    fn a_folder_at_the_state_file_cannot_be_read() {
        let t = TempDir::new();
        fs::create_dir(t.path().join("leases.json")).unwrap();
        let e = read(t.path()).unwrap_err();
        assert_eq!(e, StoreError::Unreadable(WHY_NOT_A_FILE.to_string()));
    }

    #[test]
    fn a_file_at_the_state_folder_cannot_be_read() {
        let t = TempDir::new();
        let f = t.path().join("a-file");
        fs::write(&f, b"x").unwrap();
        assert_eq!(
            read(&f).unwrap_err(),
            StoreError::Unreadable(WHY_STATE_FOLDER_FILE.to_string())
        );
    }

    #[test]
    fn a_file_at_the_state_folder_cannot_be_written() {
        let t = TempDir::new();
        let guard = lock_with(t.path(), short()).unwrap();
        let f = t.path().join("a-file");
        fs::write(&f, b"x").unwrap();
        let e = write_with(&f, &file_of(&[ID_A]), &guard, short()).unwrap_err();
        assert_eq!(e, StoreError::Unwritable(WHY_STATE_FOLDER_FILE.to_string()));
        assert_eq!(fs::read(&f).unwrap(), b"x");
    }

    #[test]
    fn a_file_that_does_not_validate_is_not_valid_in_the_contracts_words() {
        let t = TempDir::new();
        fs::write(
            t.path().join("leases.json"),
            b"{\"version\":1,\"leases\":[]}",
        )
        .unwrap();
        assert_eq!(
            read(t.path()).unwrap_err(),
            StoreError::NotValid("the state file is not valid".to_string())
        );
    }

    #[test]
    fn a_write_then_a_read_round_trips_and_leaves_no_temporary_file() {
        let t = TempDir::new();
        let guard = lock_with(t.path(), short()).unwrap();
        let f = file_of(&[ID_A, ID_B]);
        write_with(t.path(), &f, &guard, short()).unwrap();
        assert_eq!(read(t.path()).unwrap(), f);
        assert_eq!(
            fs::read(t.path().join("leases.json")).unwrap(),
            serialize(&f).unwrap()
        );
        assert!(temps(t.path()).is_empty());
        // A second write replaces the first.
        let g = file_of(&[ID_B]);
        write_with(t.path(), &g, &guard, short()).unwrap();
        assert_eq!(read(t.path()).unwrap(), g);
        assert!(temps(t.path()).is_empty());
    }

    #[test]
    fn a_write_creates_the_state_folder() {
        let t = TempDir::new();
        // The guard is on one folder and the write goes to another, which does not exist, so only
        // the write's own create_dir_all can make it (the guard is not checked against the folder).
        let guard = lock_with(&t.path().join("locked"), short()).unwrap();
        let dir = t.path().join("made").join("deeper");
        assert!(!dir.exists());
        write_with(&dir, &file_of(&[ID_A]), &guard, short()).unwrap();
        assert!(dir.is_dir());
        assert_eq!(read(&dir).unwrap().leases.len(), 1);
    }

    /// Where a state folder would be, under a file, in each shape the environment can give it:
    /// `<file>\.agentspace` (the home names a file) and `<file>\sub` (`AGENTSPACE_STATE_DIR` is
    /// under a file), and one deeper than that.
    fn under_a_file(t: &TempDir) -> (PathBuf, Vec<PathBuf>) {
        let f = t.path().join("a-file");
        fs::write(&f, b"x").unwrap();
        let dirs = vec![
            f.join(".agentspace"),
            f.join("sub"),
            f.join("a").join("b").join("c"),
        ];
        (f, dirs)
    }

    /// The file is untouched and nothing was made under or beside it.
    fn nothing_created(t: &TempDir, f: &Path, dirs: &[PathBuf]) {
        assert_eq!(fs::read(f).unwrap(), b"x");
        assert!(f.is_file());
        for d in dirs {
            assert!(!d.exists(), "{d:?} must not exist");
        }
        let names: Vec<_> = fs::read_dir(t.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    // Architect, 2026-11-19 (docs/state.md, The home folder): read, lock and write each walk up to
    // the first existing ancestor and fail when it is not a folder.

    #[test]
    fn a_state_folder_under_a_file_is_not_no_leases_and_nothing_is_created() {
        let t = TempDir::new();
        let (f, dirs) = under_a_file(&t);
        for d in &dirs {
            assert_eq!(
                read(d).unwrap_err(),
                StoreError::Unreadable(WHY_STATE_FOLDER_FILE.to_string()),
                "{d:?}"
            );
        }
        nothing_created(&t, &f, &dirs);
    }

    #[test]
    fn a_state_folder_under_a_file_cannot_be_locked_and_nothing_is_created() {
        let t = TempDir::new();
        let (f, dirs) = under_a_file(&t);
        for d in &dirs {
            assert_eq!(
                lock_with(d, short()).unwrap_err(),
                StoreError::CannotLock(WHY_STATE_FOLDER_FILE.to_string()),
                "{d:?}"
            );
        }
        nothing_created(&t, &f, &dirs);
    }

    #[test]
    fn a_state_folder_under_a_file_cannot_be_written_and_nothing_is_created() {
        let t = TempDir::new();
        let other = TempDir::new();
        let guard = lock_with(other.path(), short()).unwrap();
        let (f, dirs) = under_a_file(&t);
        for d in &dirs {
            assert_eq!(
                write_with(d, &file_of(&[ID_A]), &guard, short()).unwrap_err(),
                StoreError::Unwritable(WHY_STATE_FOLDER_FILE.to_string()),
                "{d:?}"
            );
        }
        nothing_created(&t, &f, &dirs);
    }

    #[test]
    fn a_missing_state_folder_under_a_folder_is_no_leases_and_is_not_created() {
        let t = TempDir::new();
        let dir = t.path().join("a").join("b");
        assert_eq!(read(&dir).unwrap().leases.len(), 0);
        assert!(!t.path().join("a").exists());
    }

    #[test]
    fn the_walk_stops_at_the_root_without_panicking() {
        assert!(!state_folder_is_blocked(Path::new("")));
        let root = std::env::temp_dir()
            .ancestors()
            .last()
            .unwrap()
            .to_path_buf();
        assert!(!state_folder_is_blocked(&root));
    }

    #[test]
    fn a_change_that_repeats_an_id_is_refused_and_nothing_is_written() {
        let t = TempDir::new();
        let guard = lock_with(t.path(), short()).unwrap();
        let e = write_with(t.path(), &file_of(&[ID_A, ID_A]), &guard, short()).unwrap_err();
        assert_eq!(
            e,
            StoreError::Unwritable("two records share an ID".to_string())
        );
        let mut bad = file_of(&[ID_A]);
        bad.leases[0].state = "released".to_string();
        let e = write_with(t.path(), &bad, &guard, short()).unwrap_err();
        assert_eq!(
            e,
            StoreError::Unwritable("the change is not valid".to_string())
        );
        assert!(!t.path().join("leases.json").exists());
        assert!(temps(t.path()).is_empty());
    }

    /// A folder at the state file is not a held file: the write fails on the first rename, with
    /// its own `<why>`, and is never tried again (#0189).
    ///
    /// The bound is the retry's own deadline, never a fixed wall-clock figure. A fixed 200 ms once
    /// timed the whole write, which includes work no retry rule governs: the first schema check in
    /// the test process builds every embedded validator (about 40 ms in a quiet debug build, and
    /// many times that beside a parallel workspace build), and the temporary file is flushed to the
    /// disk, which waits on a busy disk. Under load those passed 200 ms with no retry at all. The
    /// gap between tries is set as long as `replace_wait`, so a write that tried the rename again,
    /// even once, returns only once `replace_wait` has passed (`pause`): finishing before it shows
    /// the rename was not tried again, however slow the machine is.
    #[test]
    fn a_folder_at_the_state_file_fails_a_write_at_once_leaving_no_temporary_file() {
        let t = TempDir::new();
        fs::create_dir(t.path().join("leases.json")).unwrap();
        let guard = lock_with(t.path(), short()).unwrap();
        let timing = Timing {
            replace_wait: Duration::from_secs(60),
            retry_every: Duration::from_secs(60),
            ..short()
        };
        let began = Instant::now();
        let e = write_with(t.path(), &file_of(&[ID_A]), &guard, timing).unwrap_err();
        let took = began.elapsed();
        assert_eq!(e, StoreError::Unwritable(WHY_NOT_A_FILE.to_string()));
        assert!(took < timing.replace_wait, "{took:?}");
        assert!(temps(t.path()).is_empty());
    }

    #[test]
    fn the_temporary_name_is_32_hex_digits_between_the_file_name_and_tmp() {
        let t = TempDir::new();
        let (path, handle) = create_temp(t.path()).unwrap();
        drop(handle);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let hex = name
            .strip_prefix("leases.json.")
            .and_then(|n| n.strip_suffix(".tmp"))
            .unwrap();
        assert_eq!(hex.len(), 32);
        assert!(
            hex.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        let (other, _) = create_temp(t.path()).unwrap();
        assert_ne!(path, other);
    }

    /// Opens `path` the way a process that shares only read would, which blocks a rename over it.
    #[cfg(windows)]
    fn hold_sharing_read(path: &Path) -> File {
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(path)
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn a_file_held_without_sharing_delete_is_tried_again_then_fails_with_no_temporary_file() {
        let t = TempDir::new();
        let guard = lock_with(t.path(), short()).unwrap();
        write_with(t.path(), &file_of(&[ID_A]), &guard, short()).unwrap();
        let held = hold_sharing_read(&t.path().join("leases.json"));
        let began = Instant::now();
        let e = write_with(t.path(), &file_of(&[ID_B]), &guard, short()).unwrap_err();
        assert_eq!(e, StoreError::Unwritable(WHY_REPLACE.to_string()));
        assert!(
            began.elapsed() >= Duration::from_millis(250),
            "{:?}",
            began.elapsed()
        );
        assert!(temps(t.path()).is_empty());
        // Let go, and the next write goes through, with the old content still there until then.
        drop(held);
        write_with(t.path(), &file_of(&[ID_B]), &guard, short()).unwrap();
        assert_eq!(read(t.path()).unwrap(), file_of(&[ID_B]));
    }

    #[cfg(windows)]
    #[test]
    fn a_hold_that_ends_during_the_retry_lets_the_write_through() {
        let t = TempDir::new();
        let guard = lock_with(t.path(), short()).unwrap();
        write_with(t.path(), &file_of(&[ID_A]), &guard, short()).unwrap();
        let held = hold_sharing_read(&t.path().join("leases.json"));
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(held);
        });
        let long = Timing {
            replace_wait: Duration::from_secs(5),
            ..short()
        };
        write_with(t.path(), &file_of(&[ID_B]), &guard, long).unwrap();
        releaser.join().unwrap();
        assert_eq!(read(t.path()).unwrap(), file_of(&[ID_B]));
    }

    #[cfg(windows)]
    #[test]
    fn a_reader_sharing_read_write_and_delete_does_not_stop_a_write() {
        let t = TempDir::new();
        let guard = lock_with(t.path(), short()).unwrap();
        write_with(t.path(), &file_of(&[ID_A]), &guard, short()).unwrap();
        // The crate's own reader holds the file open while the write happens.
        let held = sys::open_read(&t.path().join("leases.json")).unwrap();
        write_with(t.path(), &file_of(&[ID_B]), &guard, short()).unwrap();
        drop(held);
        assert_eq!(read(t.path()).unwrap(), file_of(&[ID_B]));
    }
}

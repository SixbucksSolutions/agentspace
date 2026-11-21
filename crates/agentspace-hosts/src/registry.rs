//! The host registry on the disk: its read, its lock and its atomic write (docs/state.md, The host
//! registry, The host registry lock and Replacing a file).

use crate::error::{Failure, HostsError};
use crate::folder;
use crate::model::{Hosts, parse, serialize};
use crate::paths::{agentspace_dir, hosts_lock_path, hosts_path};
use crate::sys::{self, OpenFailure};
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::time::{Duration, Instant};

use std::path::Path;

/// How long a taker waits for the host registry lock (docs/state.md, The host registry lock): 30
/// seconds, and a taker MUST NOT wait longer.
const LOCK_WAIT: Duration = Duration::from_secs(30);
/// The gap between tries; a rewrite MAY pick its own.
const RETRY_EVERY: Duration = Duration::from_millis(20);

/// The host registry lock, held until this is dropped. The operating system lets the lock go when
/// the handle closes, however the process ends, so a crashed command never leaves it held.
pub struct HostsLock {
    _file: File,
}

/// Prints the type's name alone: the file's own `Debug` carries its full path, which MUST NOT reach
/// a log or a failed assertion. The type has no `Display`.
impl std::fmt::Debug for HostsLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostsLock { .. }")
    }
}

/// Takes the host registry lock: `<home>\.agentspace\hosts.lock`, with `.agentspace` made first
/// when missing, opened for reading and writing with no sharing, never inheritable. A taker that
/// finds it held keeps trying for up to 30 seconds, and then the registry is `Busy`. Any other
/// failure to open it, such as access denied or a folder at its path, is an error at once and is
/// never reported as busy.
pub fn lock(home: &Path) -> Result<HostsLock, HostsError> {
    lock_waiting(home, LOCK_WAIT)
}

/// Tries the host registry lock once, without waiting: `Some(lock)` when this call has it, and
/// `None` when another process or handle holds it. It is what `get`'s `check_pending` uses, which
/// starts nothing when the lock is held (docs/state.md, The host registry lock). Any other failure
/// to open it, such as access denied or a folder at its path, is an error at once, as `lock` gives
/// it, and is never reported as a held lock.
///
/// Added at review (architect, 2026-11-20, for v0.07-T02: accepted into the pin).
pub fn try_lock(home: &Path) -> Result<Option<HostsLock>, HostsError> {
    match lock_waiting(home, Duration::ZERO) {
        Ok(held) => Ok(Some(held)),
        Err(HostsError::Busy) => Ok(None),
        Err(other) => Err(other),
    }
}

fn lock_waiting(home: &Path, wait: Duration) -> Result<HostsLock, HostsError> {
    folder::ensure(&agentspace_dir(home)).map_err(HostsError::CannotLock)?;
    let path = hosts_lock_path(home);
    let start = Instant::now();
    loop {
        match sys::open_lock(&path, true) {
            Ok(file) => return Ok(HostsLock { _file: file }),
            Err(OpenFailure::Held) => {
                let elapsed = start.elapsed();
                if elapsed >= wait {
                    return Err(HostsError::Busy);
                }
                std::thread::sleep(RETRY_EVERY.min(wait - elapsed));
            }
            Err(OpenFailure::Missing | OpenFailure::Other) => {
                return Err(HostsError::CannotLock(Failure::CannotOpenLock));
            }
        }
    }
}

/// Reads `<home>\.agentspace\hosts.json`, sharing read, write and delete, so that it never blocks a
/// writer's rename and sees the old file or the new one. It takes no lock and writes nothing.
///
/// A missing file, and a missing `.agentspace`, read as the seed: the box's record alone. Only a
/// missing file does. Something at the path that is not a file, a file that cannot be opened or
/// read, and a file that is not valid are each an error, never a missing file (docs/state.md).
pub fn read(home: &Path) -> Result<Hosts, HostsError> {
    if folder::is_blocked(&agentspace_dir(home)) {
        return Err(HostsError::Unreadable(Failure::FolderIsAFile));
    }
    let path = hosts_path(home);
    let unreadable = |why: Failure| HostsError::Unreadable(why);
    let mut file = match sys::open_read(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Hosts::seed()),
        Err(_) if path.is_dir() => return Err(unreadable(Failure::NotAFile)),
        Err(_) => return Err(unreadable(Failure::CannotOpen)),
    };
    // Where a folder can be opened, as on other systems, the handle says so.
    if file.metadata().is_ok_and(|m| m.is_dir()) {
        return Err(unreadable(Failure::NotAFile));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| unreadable(Failure::CannotRead))?;
    parse(&bytes).map_err(HostsError::NotValid)
}

/// Writes `<home>\.agentspace\hosts.json` atomically, through `hosts.json.<hex>.tmp` in the same
/// folder, flushed before the rename, by agentspace-state's `write_atomically`. A change that
/// would not be valid is refused before anything is written. `.agentspace` is made first when
/// missing. `_held` is the proof that the caller holds the lock; it is not checked against `home`.
///
/// The writer MUST hold the lock from before it read the file until after this returns, and MUST
/// change only the record or records its change is about (docs/state.md): a caller that found no
/// file starts from `Hosts::seed()`, which `read` gives.
pub fn write(home: &Path, hosts: &Hosts, _held: &HostsLock) -> Result<(), HostsError> {
    let bytes =
        serialize(hosts).map_err(|why| HostsError::Unwritable(Failure::ChangeNotValid(why)))?;
    folder::ensure(&agentspace_dir(home)).map_err(HostsError::Unwritable)?;
    let path = hosts_path(home);
    agentspace_state::write_atomically(&path, &bytes).map_err(|_| {
        HostsError::Unwritable(if path.is_dir() {
            Failure::NotAFile
        } else {
            Failure::CannotReplace
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Invalid;
    use crate::model::{Host, HostState};
    use std::fs;

    fn home() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn the_wait_ceiling_is_thirty_seconds_and_lock_uses_it() {
        assert_eq!(LOCK_WAIT, Duration::from_secs(30));
        // `lock` MUST wait through `LOCK_WAIT` and no other value: its source says so.
        let source = include_str!("registry.rs");
        let body = source
            .split("pub fn lock(home: &Path)")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .unwrap();
        let needle = format!("lock_waiting(home, {})", "LOCK_WAIT");
        assert!(body.contains(&needle), "`lock` MUST wait through LOCK_WAIT");
    }

    #[test]
    fn try_lock_gives_some_when_free_and_none_at_once_when_held() {
        let t = home();
        let first = try_lock(t.path()).unwrap();
        assert!(first.is_some());
        assert!(hosts_lock_path(t.path()).is_file());
        // Held by this handle: another thread's try gives none, without waiting.
        let path = t.path().to_path_buf();
        let began = Instant::now();
        let second = std::thread::spawn(move || try_lock(&path).map(|l| l.is_some()))
            .join()
            .unwrap();
        assert_eq!(second, Ok(false));
        assert!(began.elapsed() < Duration::from_millis(500));
        drop(first);
        assert!(try_lock(t.path()).unwrap().is_some());
    }

    #[test]
    fn try_lock_with_a_folder_at_the_lock_is_an_error_not_none() {
        let t = home();
        fs::create_dir_all(hosts_lock_path(t.path())).unwrap();
        assert_eq!(
            try_lock(t.path()).map(|l| l.is_some()),
            Err(HostsError::CannotLock(Failure::CannotOpenLock))
        );
        let t = home();
        fs::write(agentspace_dir(t.path()), b"x").unwrap();
        assert_eq!(
            try_lock(t.path()).map(|l| l.is_some()),
            Err(HostsError::CannotLock(Failure::FolderIsAFile))
        );
    }

    #[test]
    fn debug_prints_the_name_and_no_path() {
        let t = home();
        let held = lock(t.path()).unwrap();
        let shown = format!("{held:?}");
        assert!(!shown.contains(std::path::MAIN_SEPARATOR));
        assert!(!shown.contains('/'));
        let temp_name = t.path().file_name().unwrap().to_string_lossy().into_owned();
        assert!(!shown.contains(&temp_name));
        assert!(!shown.contains("hosts.lock"));
    }

    fn short() -> Duration {
        Duration::from_millis(300)
    }

    fn added() -> Hosts {
        let mut h = Hosts::seed();
        h.hosts.push(Host {
            project: "agentspace".to_string(),
            stack: "agentspace-host-agentspace".to_string(),
            state: HostState::Creating,
            instance_id: None,
            last_attempt_at: Some("2026-11-20T19:05:42Z".to_string()),
        });
        h
    }

    fn leftovers(home: &Path) -> Vec<String> {
        fs::read_dir(agentspace_dir(home))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_missing_file_or_folder_reads_as_the_seed_and_nothing_is_written() {
        let t = home();
        assert_eq!(read(t.path()).unwrap(), Hosts::seed());
        assert!(!agentspace_dir(t.path()).exists());
        fs::create_dir_all(agentspace_dir(t.path())).unwrap();
        assert_eq!(read(t.path()).unwrap(), Hosts::seed());
        assert!(leftovers(t.path()).is_empty());
    }

    #[test]
    fn the_first_change_writes_the_seed_with_it_and_a_second_read_sees_the_change() {
        let t = home();
        let held = lock(t.path()).unwrap();
        let mut hosts = read(t.path()).unwrap();
        hosts.hosts.push(added().hosts.remove(1));
        write(t.path(), &hosts, &held).unwrap();
        drop(held);
        let read_back = read(t.path()).unwrap();
        assert_eq!(read_back, added());
        assert_eq!(read_back.hosts[0].project, "swimcoach");
        let mut names = leftovers(t.path());
        names.sort();
        assert_eq!(
            names,
            ["hosts.json", "hosts.lock"],
            "No temporary file is left"
        );
        let bytes = fs::read(hosts_path(t.path())).unwrap();
        assert_eq!(bytes, serialize(&added()).unwrap());
    }

    #[test]
    fn a_change_that_would_not_be_valid_is_not_written() {
        let t = home();
        let held = lock(t.path()).unwrap();
        let mut hosts = added();
        hosts.hosts.push(hosts.hosts[1].clone());
        assert_eq!(
            write(t.path(), &hosts, &held),
            Err(HostsError::Unwritable(Failure::ChangeNotValid(
                Invalid::RepeatedProject
            )))
        );
        assert!(!hosts_path(t.path()).exists());
    }

    #[test]
    fn a_file_that_is_not_valid_is_an_error_never_the_seed() {
        let t = home();
        fs::create_dir_all(agentspace_dir(t.path())).unwrap();
        fs::write(hosts_path(t.path()), b"{\"version\":0,\"hosts\":[]}").unwrap();
        assert_eq!(
            read(t.path()),
            Err(HostsError::NotValid(Invalid::BoxRecord))
        );
        fs::write(hosts_path(t.path()), b"\xFF").unwrap();
        assert_eq!(read(t.path()), Err(HostsError::NotValid(Invalid::NotUtf8)));
    }

    #[test]
    fn a_byte_order_mark_and_whitespace_are_read() {
        let t = home();
        fs::create_dir_all(agentspace_dir(t.path())).unwrap();
        fs::write(
            hosts_path(t.path()),
            b"\xEF\xBB\xBF{\n  \"version\": 0,\n  \"hosts\": [ {\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"} ]\n}\r\n",
        )
        .unwrap();
        assert_eq!(read(t.path()).unwrap(), Hosts::seed());
    }

    #[test]
    fn a_folder_at_the_registry_is_not_valid_and_is_neither_seeded_nor_replaced() {
        let t = home();
        fs::create_dir_all(hosts_path(t.path())).unwrap();
        assert_eq!(
            read(t.path()),
            Err(HostsError::Unreadable(Failure::NotAFile))
        );
        let held = lock(t.path()).unwrap();
        assert_eq!(
            write(t.path(), &added(), &held),
            Err(HostsError::Unwritable(Failure::NotAFile))
        );
        assert!(hosts_path(t.path()).is_dir());
        assert_eq!(
            leftovers(t.path()).len(),
            2,
            "hosts.json and hosts.lock alone"
        );
    }

    #[test]
    fn a_file_where_the_agentspace_folder_belongs_fails_each_call_and_writes_nothing() {
        let t = home();
        fs::write(agentspace_dir(t.path()), b"x").unwrap();
        assert_eq!(
            read(t.path()),
            Err(HostsError::Unreadable(Failure::FolderIsAFile))
        );
        assert_eq!(
            lock(t.path()).unwrap_err(),
            HostsError::CannotLock(Failure::FolderIsAFile)
        );
        assert_eq!(fs::read(agentspace_dir(t.path())).unwrap(), b"x");
    }

    #[test]
    fn the_lock_creates_its_folder_and_file_excludes_a_second_taker_and_lets_go_on_drop() {
        let t = home();
        let first = lock_waiting(t.path(), short()).unwrap();
        assert!(hosts_lock_path(t.path()).is_file());
        let began = Instant::now();
        assert_eq!(
            lock_waiting(t.path(), short()).unwrap_err(),
            HostsError::Busy
        );
        assert!(began.elapsed() >= Duration::from_millis(250));
        drop(first);
        lock_waiting(t.path(), short()).unwrap();
        assert!(
            hosts_lock_path(t.path()).is_file(),
            "The file is never deleted"
        );
    }

    #[test]
    fn a_lock_held_for_a_short_while_is_waited_for() {
        let t = home();
        let first = lock_waiting(t.path(), short()).unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(first);
        });
        assert!(lock_waiting(t.path(), Duration::from_secs(5)).is_ok());
        releaser.join().unwrap();
    }

    #[test]
    fn a_folder_at_the_lock_fails_at_once_and_is_not_busy() {
        let t = home();
        fs::create_dir_all(hosts_lock_path(t.path())).unwrap();
        let began = Instant::now();
        assert_eq!(
            lock_waiting(t.path(), short()).unwrap_err(),
            HostsError::CannotLock(Failure::CannotOpenLock)
        );
        assert!(began.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn a_reader_is_never_blocked_by_the_lock() {
        let t = home();
        let held = lock(t.path()).unwrap();
        write(t.path(), &added(), &held).unwrap();
        assert_eq!(read(t.path()).unwrap(), added());
    }
}

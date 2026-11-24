//! The host registry on the disk: its read, its lock and its atomic write (docs/state.md, The host
//! registry, The host registry lock and Replacing a file).

use crate::error::{Failure, HostsError};
use crate::folder;
use crate::model::{Host, Hosts, is_normalized_project, parse, serialize};
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
/// A missing file, and a missing `.agentspace`, read as the seed: the box's record alone, **with no
/// `instance_type`**. Only a missing file does. Something at the path that is not a file, a file
/// that cannot be opened or read, and a file that is not valid are each an error, never a missing
/// file (docs/state.md).
///
/// **This is the read a change starts from**: a writer that finds no file starts from the seed
/// document, makes its change and writes the whole document, and the record it writes MUST NOT
/// hold the `m9g.xlarge` that a missing file implies on a read (docs/state.md, "The seed"; pin
/// P31). A reader that places a lease by a host's `instance_type` calls `read_for_placing`.
pub fn read(home: &Path) -> Result<Hosts, HostsError> {
    Ok(load(home)?.unwrap_or_else(Hosts::seed))
}

/// Reads the registry as a reader that places a lease reads it (docs/state.md, "The seed"; pin
/// P2c): as `read` does, but a missing file reads as the seed's one record **holding
/// `instance_type` `m9g.xlarge`**, D65's `m`. It takes no lock and writes nothing, and reading never
/// creates the file. A file that exists is read as it is, so a record with no `instance_type`
/// stays without one. The result MUST NOT be the start of a write, since the implied type is no
/// reading of the stack and is never written (pin P31): a writer calls `read`, or one of
/// `set_instance_type` and `set_instance`.
pub fn read_for_placing(home: &Path) -> Result<Hosts, HostsError> {
    Ok(load(home)?.unwrap_or_else(Hosts::implied))
}

/// The registry as the disk holds it: `None` when there is no file, which only a missing file
/// gives, and no default of any kind. `read` and `read_for_placing` choose what a missing file
/// reads as, and a writer never gets the implied type from here.
fn load(home: &Path) -> Result<Option<Hosts>, HostsError> {
    if folder::is_blocked(&agentspace_dir(home)) {
        return Err(HostsError::Unreadable(Failure::FolderIsAFile));
    }
    let path = hosts_path(home);
    let unreadable = |why: Failure| HostsError::Unreadable(why);
    let mut file = match sys::open_read(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
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
    parse(&bytes).map(Some).map_err(HostsError::NotValid)
}

/// Writes `<home>\.agentspace\hosts.json` atomically, through `hosts.json.<hex>.tmp` in the same
/// folder, flushed before the rename, by agentspace-state's `write_atomically`. A change that
/// would not be valid is refused before anything is written. `.agentspace` is made first when
/// missing. `_held` is the proof that the caller holds the lock; it is not checked against `home`.
///
/// The writer MUST hold the lock from before it read the file until after this returns, and MUST
/// change only the record or records its change is about (docs/state.md): a caller that found no
/// file starts from `Hosts::seed()`, which `read` gives, and never from `Hosts::implied()`.
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

/// Sets or removes a host's `instance_type`, and nothing else of any record (docs/state.md, "The
/// instance type"; D67, D71; pins P11, P26 and P30): `Some(type)` sets it and `None` removes it. It
/// takes the host registry lock, waiting for up to 30 seconds (`Busy` when that runs out),
/// re-reads the file under it, changes that record's member, writes the whole document as `write`
/// does and releases the lock after the rename, so it never holds the lock across a call to AWS.
///
/// `None` is step 1 of a run's deploy step and of a person's `deploy -InstanceType`: the type is
/// removed before the stack changes. **With no file it writes the file**: a writer starts from the
/// seed document, whose record has no `instance_type`, so removing the type that the implied record
/// holds on a read writes the seed's record, and no `instance_type` is ever written from the
/// implied default (pins P2c, P11 and P31). With a file whose record already holds the value,
/// nothing is written. A project with no record fails with `Unwritable(NoSuchRecord)` and nothing
/// is written, since no operator command adds a record; a `project` that is not normalized is
/// `BadProject`; and a type that does not match the member's form is refused as every invalid
/// change is, with nothing written.
pub fn set_instance_type(
    home: &Path,
    project: &str,
    instance_type: Option<&str>,
) -> Result<(), HostsError> {
    change_record(home, project, LOCK_WAIT, |h| {
        h.instance_type = instance_type.map(str::to_string);
    })
}

/// Sets or removes a host's `instance_id` and `instance_type` together, in one write, and nothing
/// else of any record (docs/state.md, "The instance type", steps 2 and 4; pins P11, P23 and P30),
/// under the host registry lock as `set_instance_type` is. `None` removes a member and `Some` sets
/// it:
/// - `(None, None)` is step 2, once `cloudformation deploy` has succeeded: both removed.
/// - `(Some(id), Some(type))` is step 4, the read-back: both written in one write.
/// - `(Some(id), None)` is a read-back whose `InstanceType` is missing or malformed: the ID alone,
///   and the record left with no type.
///
/// The rules for a missing file, a project with no record and an invalid value are
/// `set_instance_type`'s. **It writes no `instance_type` it was not given**: with no file the
/// seed's record is written with what the call sets, and the implied `m9g.xlarge` never.
pub fn set_instance(
    home: &Path,
    project: &str,
    instance_id: Option<&str>,
    instance_type: Option<&str>,
) -> Result<(), HostsError> {
    change_record(home, project, LOCK_WAIT, |h| {
        h.instance_id = instance_id.map(str::to_string);
        h.instance_type = instance_type.map(str::to_string);
    })
}

/// The change both setters make: take the lock, re-read the file under it, apply `change` to the
/// one record of `project`, and write. The start is the file, or with no file the seed document and
/// never the implied record, and a missing file is always written, since the change is then a first
/// change (docs/state.md, "The seed"). A file that exists and does not change is left alone.
fn change_record(
    home: &Path,
    project: &str,
    wait: Duration,
    change: impl FnOnce(&mut Host),
) -> Result<(), HostsError> {
    if !is_normalized_project(project) {
        return Err(HostsError::BadProject);
    }
    let held = lock_waiting(home, wait)?;
    let found = load(home)?;
    let file_existed = found.is_some();
    let mut hosts = found.unwrap_or_else(Hosts::seed);
    let before = hosts.clone();
    let record = hosts
        .hosts
        .iter_mut()
        .find(|h| h.project == project)
        .ok_or(HostsError::Unwritable(Failure::NoSuchRecord))?;
    change(record);
    if file_existed && hosts == before {
        return Ok(());
    }
    write(home, &hosts, &held)
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
            instance_type: None,
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

    // The default of a missing file (docs/state.md, "The seed"; pins P2c and P31).

    #[test]
    fn a_missing_file_reads_for_placing_as_the_seed_holding_the_default_type_and_writes_nothing() {
        let t = home();
        let hosts = read_for_placing(t.path()).unwrap();
        assert_eq!(hosts, Hosts::implied());
        assert_eq!(hosts.hosts.len(), 1);
        assert_eq!(hosts.hosts[0].project, "swimcoach");
        assert_eq!(hosts.hosts[0].instance_type.as_deref(), Some("m9g.xlarge"));
        assert!(
            !agentspace_dir(t.path()).exists(),
            "Reading creates nothing"
        );
        // A missing `.agentspace` made, and a missing file in it, read the same, and make no file.
        fs::create_dir_all(agentspace_dir(t.path())).unwrap();
        assert_eq!(read_for_placing(t.path()).unwrap(), Hosts::implied());
        assert!(leftovers(t.path()).is_empty());
    }

    #[test]
    fn the_read_a_change_starts_from_never_holds_the_implied_type() {
        let t = home();
        let hosts = read(t.path()).unwrap();
        assert_eq!(hosts, Hosts::seed());
        assert_eq!(hosts.hosts[0].instance_type, None);
        // The one difference between the two reads of a missing file is that member.
        let mut implied = read_for_placing(t.path()).unwrap();
        implied.hosts[0].instance_type = None;
        assert_eq!(implied, hosts);
    }

    #[test]
    fn a_file_that_exists_reads_for_placing_as_it_is() {
        let t = home();
        let held = lock(t.path()).unwrap();
        write(t.path(), &Hosts::seed(), &held).unwrap();
        // The seed's own record has no type, and a file that exists keeps it without one.
        assert_eq!(read_for_placing(t.path()).unwrap(), Hosts::seed());
        let mut typed = added();
        typed.hosts[1].instance_type = Some("m9g.2xlarge".to_string());
        write(t.path(), &typed, &held).unwrap();
        assert_eq!(read_for_placing(t.path()).unwrap(), typed);
        assert_eq!(read(t.path()).unwrap(), typed);
    }

    #[test]
    fn read_for_placing_fails_as_read_does() {
        let t = home();
        fs::create_dir_all(agentspace_dir(t.path())).unwrap();
        fs::write(hosts_path(t.path()), b"{\"version\":0,\"hosts\":[]}").unwrap();
        assert_eq!(
            read_for_placing(t.path()),
            Err(HostsError::NotValid(Invalid::BoxRecord))
        );
        let t = home();
        fs::create_dir_all(hosts_path(t.path())).unwrap();
        assert_eq!(
            read_for_placing(t.path()),
            Err(HostsError::Unreadable(Failure::NotAFile))
        );
        let t = home();
        fs::write(agentspace_dir(t.path()), b"x").unwrap();
        assert_eq!(
            read_for_placing(t.path()),
            Err(HostsError::Unreadable(Failure::FolderIsAFile))
        );
    }

    // The setters (docs/state.md, "The instance type"; pins P11, P26, P30 and P31).

    const SEED_BYTES: &[u8] =
        b"{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n";

    fn registry_of(t: &tempfile::TempDir, hosts: &Hosts) {
        let held = lock(t.path()).unwrap();
        write(t.path(), hosts, &held).unwrap();
    }

    fn bytes_of(t: &tempfile::TempDir) -> Vec<u8> {
        fs::read(hosts_path(t.path())).unwrap()
    }

    #[test]
    fn removing_the_type_with_no_file_writes_the_seed_document_without_one() {
        let t = home();
        set_instance_type(t.path(), "swimcoach", None).unwrap();
        // The file is made, though the implied record held a type and the seed's holds none.
        assert_eq!(bytes_of(&t), SEED_BYTES);
        assert!(
            !String::from_utf8(bytes_of(&t))
                .unwrap()
                .contains("instance_type")
        );
        let mut names = leftovers(t.path());
        names.sort();
        assert_eq!(names, ["hosts.json", "hosts.lock"]);
        // The default is read no more from it: the file now says the box has no type.
        assert_eq!(read_for_placing(t.path()).unwrap(), Hosts::seed());
    }

    #[test]
    fn no_setter_writes_the_default_from_a_missing_file() {
        // The same writers, given a missing file, write the seed's record with what they were
        // given, and `m9g.xlarge` only when they were given it.
        let t = home();
        set_instance(t.path(), "swimcoach", Some("i-0123456789abcdef0"), None).unwrap();
        let text = String::from_utf8(bytes_of(&t)).unwrap();
        assert!(text.contains("\"instance_id\":\"i-0123456789abcdef0\""));
        assert!(!text.contains("instance_type"), "{text}");
        assert!(!text.contains("m9g.xlarge"), "{text}");
        let t = home();
        set_instance(t.path(), "swimcoach", None, None).unwrap();
        assert_eq!(bytes_of(&t), SEED_BYTES);
        let t = home();
        set_instance_type(t.path(), "swimcoach", Some("m9g.2xlarge")).unwrap();
        let hosts = read(t.path()).unwrap();
        assert_eq!(hosts.hosts[0].instance_type.as_deref(), Some("m9g.2xlarge"));
    }

    #[test]
    fn a_change_to_another_member_with_no_file_starts_from_the_seed_and_never_the_implied_type() {
        // The setters above overwrite `instance_type` whatever they start from, so they cannot
        // show where a change starts. A change that leaves the member alone can: it writes the
        // record as it started, which MUST be the seed's, with no type (pin P31).
        let t = home();
        change_record(t.path(), "swimcoach", short(), |h| {
            h.last_attempt_at = Some("2026-11-20T19:05:42Z".to_string());
        })
        .unwrap();
        let text = String::from_utf8(bytes_of(&t)).unwrap();
        assert!(text.contains("\"last_attempt_at\":\"2026-11-20T19:05:42Z\""));
        assert!(!text.contains("instance_type"), "{text}");
        assert!(!text.contains("m9g.xlarge"), "{text}");
        assert_eq!(read(t.path()).unwrap().hosts[0].instance_type, None);
    }

    #[test]
    fn a_project_with_no_record_fails_and_writes_nothing() {
        // With no file: the seed holds the box's record alone.
        let t = home();
        assert_eq!(
            set_instance_type(t.path(), "agentspace", Some("m9g.large")),
            Err(HostsError::Unwritable(Failure::NoSuchRecord))
        );
        assert_eq!(
            set_instance(t.path(), "agentspace", Some("i-0123456789abcdef0"), None),
            Err(HostsError::Unwritable(Failure::NoSuchRecord))
        );
        assert!(!hosts_path(t.path()).exists());
        // With a file: the bytes stay as they were.
        let t = home();
        registry_of(&t, &Hosts::seed());
        let before = bytes_of(&t);
        assert_eq!(
            set_instance_type(t.path(), "agentspace", None),
            Err(HostsError::Unwritable(Failure::NoSuchRecord))
        );
        assert_eq!(bytes_of(&t), before);
        assert!(
            Failure::NoSuchRecord
                .to_string()
                .starts_with("the registry has no record")
        );
    }

    #[test]
    fn a_project_not_in_its_normalized_form_is_refused_before_the_lock() {
        let t = home();
        for bad in ["", "Swim", "a b", "a\n", "../x"] {
            assert_eq!(
                set_instance_type(t.path(), bad, None),
                Err(HostsError::BadProject),
                "{bad:?}"
            );
        }
        assert!(!agentspace_dir(t.path()).exists());
    }

    #[test]
    fn setting_the_type_changes_that_record_alone_and_in_the_tables_order() {
        let t = home();
        let mut two = added();
        two.hosts[1].instance_id = Some("i-0fedcba9876543210".to_string());
        registry_of(&t, &two);
        set_instance_type(t.path(), "agentspace", Some("m9g.2xlarge")).unwrap();
        let mut expected = two.clone();
        expected.hosts[1].instance_type = Some("m9g.2xlarge".to_string());
        assert_eq!(read(t.path()).unwrap(), expected);
        let text = String::from_utf8(bytes_of(&t)).unwrap();
        assert!(text.contains(
            "\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.2xlarge\",\"last_attempt_at\":\"2026-11-20T19:05:42Z\"}"
        ));
        // The type removed again gives the file as it was.
        set_instance_type(t.path(), "agentspace", None).unwrap();
        assert_eq!(bytes_of(&t), serialize(&two).unwrap());
        // The box's record, and its place, were never touched.
        assert_eq!(read(t.path()).unwrap().hosts[0], Hosts::seed().hosts[0]);
    }

    #[test]
    fn an_existing_file_that_does_not_change_is_not_rewritten() {
        let t = home();
        fs::create_dir_all(agentspace_dir(t.path())).unwrap();
        // Hand-formatted, which a rewrite would make compact.
        let spaced = b"{ \"version\": 0,\n  \"hosts\": [ {\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"} ] }\n";
        fs::write(hosts_path(t.path()), spaced).unwrap();
        set_instance_type(t.path(), "swimcoach", None).unwrap();
        set_instance(t.path(), "swimcoach", None, None).unwrap();
        assert_eq!(bytes_of(&t), spaced);
        // A change is written, compactly.
        set_instance_type(t.path(), "swimcoach", Some("m9g.large")).unwrap();
        assert_ne!(bytes_of(&t), spaced);
    }

    #[test]
    fn the_id_and_the_type_are_removed_and_written_in_one_write_each() {
        let t = home();
        let mut full = added();
        full.hosts[0].instance_id = Some("i-0123456789abcdef0".to_string());
        full.hosts[0].instance_type = Some("m9g.xlarge".to_string());
        full.hosts[1].instance_id = Some("i-0fedcba9876543210".to_string());
        full.hosts[1].instance_type = Some("m9g.large".to_string());
        registry_of(&t, &full);
        // Step 2: both removed from one record.
        set_instance(t.path(), "agentspace", None, None).unwrap();
        let mut expected = full.clone();
        expected.hosts[1].instance_id = None;
        expected.hosts[1].instance_type = None;
        assert_eq!(read(t.path()).unwrap(), expected);
        // Step 4: both written.
        set_instance(
            t.path(),
            "agentspace",
            Some("i-0aaaaaaaaaaaaaaaa"),
            Some("m9g.2xlarge"),
        )
        .unwrap();
        expected.hosts[1].instance_id = Some("i-0aaaaaaaaaaaaaaaa".to_string());
        expected.hosts[1].instance_type = Some("m9g.2xlarge".to_string());
        assert_eq!(read(t.path()).unwrap(), expected);
        // A read-back with no type: the ID alone, the record left with none.
        set_instance(t.path(), "agentspace", Some("i-0bbbbbbbbbbbbbbbb"), None).unwrap();
        expected.hosts[1].instance_id = Some("i-0bbbbbbbbbbbbbbbb".to_string());
        expected.hosts[1].instance_type = None;
        assert_eq!(read(t.path()).unwrap(), expected);
        // The box's record kept its own, throughout.
        assert_eq!(
            read(t.path()).unwrap().hosts[0].instance_type.as_deref(),
            Some("m9g.xlarge")
        );
    }

    #[test]
    fn a_value_that_is_not_of_its_form_is_refused_and_the_file_is_left_as_it_was() {
        let t = home();
        registry_of(&t, &added());
        let before = bytes_of(&t);
        let too_long = format!("m9g.{}", "a".repeat(61));
        for bad in [
            "M9G.XLARGE",
            "m9g",
            "m9g.xlarge\n",
            "m9g.xlarge ",
            "m9g.x.y",
            "",
            too_long.as_str(),
        ] {
            assert_eq!(
                set_instance_type(t.path(), "agentspace", Some(bad)),
                Err(HostsError::Unwritable(Failure::ChangeNotValid(
                    Invalid::Schema
                ))),
                "{bad:?}"
            );
            assert_eq!(bytes_of(&t), before, "{bad:?}");
        }
        assert_eq!(
            set_instance(t.path(), "agentspace", Some("i-0123"), Some("m9g.large")),
            Err(HostsError::Unwritable(Failure::ChangeNotValid(
                Invalid::Schema
            )))
        );
        assert_eq!(bytes_of(&t), before);
        // Sixty-four characters is the longest that is accepted.
        let longest = format!("m9g.{}", "a".repeat(60));
        assert_eq!(longest.len(), 64);
        set_instance_type(t.path(), "agentspace", Some(&longest)).unwrap();
    }

    #[test]
    fn a_file_that_is_not_valid_is_never_written_over_by_a_setter() {
        let t = home();
        fs::create_dir_all(agentspace_dir(t.path())).unwrap();
        fs::write(hosts_path(t.path()), b"{\"version\":0,\"hosts\":[]}").unwrap();
        assert_eq!(
            set_instance_type(t.path(), "swimcoach", None),
            Err(HostsError::NotValid(Invalid::BoxRecord))
        );
        assert_eq!(bytes_of(&t), b"{\"version\":0,\"hosts\":[]}");
        let t = home();
        fs::create_dir_all(hosts_path(t.path())).unwrap();
        assert_eq!(
            set_instance_type(t.path(), "swimcoach", None),
            Err(HostsError::Unreadable(Failure::NotAFile))
        );
        assert!(hosts_path(t.path()).is_dir());
    }

    #[test]
    fn a_setter_holds_the_lock_while_it_works_and_waits_for_it_no_longer_than_it_is_given() {
        let t = home();
        let held = lock(t.path()).unwrap();
        let began = Instant::now();
        assert_eq!(
            change_record(t.path(), "swimcoach", short(), |h| h.instance_type = None),
            Err(HostsError::Busy)
        );
        assert!(began.elapsed() >= Duration::from_millis(250));
        assert!(!hosts_path(t.path()).exists(), "Nothing was written");
        // The lock is released after the write, so the next taker gets it.
        drop(held);
        set_instance_type(t.path(), "swimcoach", None).unwrap();
        assert!(try_lock(t.path()).unwrap().is_some());
        // The change reads the file under the lock: a change made while it waited is kept.
        let t = home();
        registry_of(&t, &Hosts::seed());
        let held = lock(t.path()).unwrap();
        let path = t.path().to_path_buf();
        let waiter = std::thread::spawn(move || {
            change_record(&path, "swimcoach", Duration::from_secs(5), |h| {
                h.instance_type = Some("m9g.large".to_string());
            })
        });
        std::thread::sleep(Duration::from_millis(100));
        write(t.path(), &added(), &held).unwrap();
        drop(held);
        waiter.join().unwrap().unwrap();
        let after = read(t.path()).unwrap();
        assert_eq!(after.hosts.len(), 2, "The other writer's record is kept");
        assert_eq!(after.hosts[0].instance_type.as_deref(), Some("m9g.large"));
    }

    #[test]
    fn a_folder_at_the_lock_fails_a_setter_at_once_and_writes_nothing() {
        let t = home();
        fs::create_dir_all(hosts_lock_path(t.path())).unwrap();
        assert_eq!(
            set_instance_type(t.path(), "swimcoach", None),
            Err(HostsError::CannotLock(Failure::CannotOpenLock))
        );
        assert!(!hosts_path(t.path()).exists());
    }
}

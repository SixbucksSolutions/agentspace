//! The alias file and the `Include` line on disk (docs/state.md, "Replacing a file", "The alias
//! file", "The `Include` line" and "Release, and what it leaves").
//!
//! RFC 2119 keywords, and the capitals are load-bearing. A function here never writes to stdout or
//! stderr and never panics on bad input. Each message is agentspace's own words, the `<why>` of
//! lease-api.md's message: no path, no user name, no other program's or library's error text.

use crate::windows::{is_link_or_read_only, random_bytes, replace_keeping_rules};
use crate::{
    IncludePlan, host_alias_name, host_owner_mark, include_plan, is_valid_alias,
    is_valid_host_name, owner_mark,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Which kind of failure an [`AliasFailure`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasFailureKind {
    /// The OpenSSH client configuration is the problem, and a person has to fix it: the `Include`
    /// line is below a `Host` or `Match` block, or the config is a link, read-only or not a file.
    Config,
    /// The alias file's place holds another lease's file, a file with no owner mark, or something
    /// that is not a file.
    Foreign,
    /// Anything else: a folder or file could not be made, read, written, replaced or deleted.
    Io,
}

/// A refusal or a failure, with the `<why>` of lease-api.md's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasFailure {
    pub kind: AliasFailureKind,
    pub message: String,
}

impl fmt::Display for AliasFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AliasFailure {}

fn fail(kind: AliasFailureKind, message: &str) -> AliasFailure {
    AliasFailure {
        kind,
        message: message.to_string(),
    }
}

fn io_fail(message: &str) -> AliasFailure {
    fail(AliasFailureKind::Io, message)
}

const FOREIGN_WHY: &str =
    "the host alias file belongs to another lease or to no lease, so a person has to move it";
const HOST_FOREIGN_WHY: &str = "the operator's alias file for the host belongs to another host, to a lease or to no host, so a person has to move it";
const INVALID_ALIAS_WHY: &str = "the host alias is not a valid name";

/// The folder the `Include` line reads.
fn alias_dir(ssh_dir: &Path) -> PathBuf {
    ssh_dir.join("agentspace.d")
}

/// `<ssh folder>\agentspace.d\<alias>`. An alias that does not meet the alias rule, or is `.` or
/// `..`, is refused, so that no path can leave the folder.
fn alias_path(ssh_dir: &Path, alias: &str) -> Result<PathBuf, AliasFailure> {
    if !is_valid_alias(alias) || alias == "." || alias == ".." {
        return Err(io_fail(INVALID_ALIAS_WHY));
    }
    Ok(alias_dir(ssh_dir).join(alias))
}

/// Whether `path` exists and is not a folder (a link counts as what it points to, and a link that
/// points nowhere is not a folder).
fn exists_and_not_a_folder(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => !fs::metadata(path).is_ok_and(|m| m.is_dir()),
        Ok(m) => !m.is_dir(),
        Err(_) => false,
    }
}

/// Refuses a file where the OpenSSH client folder or `agentspace.d` has to be a folder, before
/// anything is read, written or deleted: on Windows a path through a file reads as "not found",
/// which would otherwise pass for a missing alias file. A folder that is not there is fine.
fn check_folders(ssh_dir: &Path) -> Result<(), AliasFailure> {
    // The <why>s are the architect's ruling of 2026-11-19 (docs/state.md, Release and what it
    // leaves, step 1): the CLI prints them through lease-api.md's "Anything else" row.
    if exists_and_not_a_folder(ssh_dir) {
        return Err(io_fail("the OpenSSH client folder is a file, not a folder"));
    }
    if exists_and_not_a_folder(&alias_dir(ssh_dir)) {
        return Err(io_fail(
            "the folder for host aliases is a file, not a folder",
        ));
    }
    Ok(())
}

/// Whether the file at `path` holds exactly the bytes of `original`, by SHA-256 (docs/state.md,
/// "The `Include` line": the backup's hash is checked against the bytes read). A file that cannot
/// be read does not match.
fn backup_matches(path: &Path, original: &[u8]) -> bool {
    fs::read(path).is_ok_and(|copied| Sha256::digest(&copied) == Sha256::digest(original))
}

/// 32 lowercase hex digits from the operating system's random source.
fn random_hex() -> io::Result<String> {
    let mut buf = [0u8; 16];
    random_bytes(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// `YYYYMMDDTHHMMSSZ` for a Unix time, in UTC. A time before 1970 is handled by the same rules.
fn format_utc(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Civil date from a day count (Howard Hinnant's algorithm), valid for every i64 day count
    // that a Unix time can give.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// Writes `bytes` to a new file `<dir>\<name>.agentspace-tmp-<32 hex digits>`, which never
/// overwrites a file, and returns its path. Its access rules are the folder's. A failure removes
/// the file again.
fn write_temp(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let mut last = io::Error::other("no temporary name");
    for _ in 0..8 {
        let path = dir.join(format!("{name}.agentspace-tmp-{}", random_hex()?));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                last = e;
                continue;
            }
            Err(e) => return Err(e),
        };
        let done = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        return match done {
            Ok(()) => Ok(path),
            Err(e) => {
                let _ = fs::remove_file(&path);
                Err(e)
            }
        };
    }
    Err(last)
}

/// Makes sure `<ssh folder>\config` holds the `Include` line (docs/state.md, "The `Include`
/// line"), as a `create` needs before it writes an alias file.
///
/// `agentspace.d` is created first. With no config, one holding the line and one LF is created.
/// With the line above the first `Host` or `Match` line, nothing is written. With the line only
/// below one, [`AliasFailureKind::Config`] and nothing changes. Otherwise a config that is a link,
/// another reparse point or read-only is refused with [`AliasFailureKind::Config`] before any
/// backup, and else the config is backed up as `config.agentspace-backup-<UTC time>`, never over a
/// file and with its SHA-256 checked against the bytes read, and replaced through
/// `config.agentspace-tmp-<32 hex digits>` so that it keeps its own access rules; its owner becomes
/// the user running this (docs/state.md, Replacing a file).
/// `now_unix` is the Unix time the backup's name is made from.
pub fn ensure_include(ssh_dir: &Path, now_unix: i64) -> Result<(), AliasFailure> {
    check_folders(ssh_dir)?;
    fs::create_dir_all(alias_dir(ssh_dir))
        .map_err(|_| io_fail("the folder for the host aliases could not be created"))?;

    let config = ssh_dir.join("config");
    let meta = match fs::symlink_metadata(&config) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return create_config(&config),
        Err(_) => {
            return Err(io_fail(
                "the OpenSSH client configuration could not be read",
            ));
        }
    };
    let is_dir = if meta.file_type().is_symlink() {
        fs::metadata(&config).is_ok_and(|m| m.is_dir())
    } else {
        meta.is_dir()
    };
    if is_dir {
        return Err(fail(
            AliasFailureKind::Config,
            "its configuration is a folder, not a file, so a person has to fix it",
        ));
    }
    let original = fs::read(&config)
        .map_err(|_| io_fail("the OpenSSH client configuration could not be read"))?;

    let updated = match include_plan(&original) {
        IncludePlan::Found => return Ok(()),
        IncludePlan::MustMove => {
            return Err(fail(
                AliasFailureKind::Config,
                "its Include line for agentspace is inside a Host block, and has to move to the top of the file",
            ));
        }
        IncludePlan::Insert(bytes) => bytes,
    };
    if is_link_or_read_only(&meta) {
        return Err(fail(
            AliasFailureKind::Config,
            "its configuration is a link or read-only, so a person has to add the line Include agentspace.d/* to the top of the file",
        ));
    }

    // The backup: a new file, never over one, and its hash is the hash of the bytes read.
    let backup = ssh_dir.join(format!("config.agentspace-backup-{}", format_utc(now_unix)));
    const BACKUP_WHY: &str = "the OpenSSH client configuration could not be backed up";
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
        .map_err(|_| io_fail(BACKUP_WHY))?;
    file.write_all(&original)
        .and_then(|()| file.sync_all())
        .map_err(|_| io_fail(BACKUP_WHY))?;
    drop(file);
    if !backup_matches(&backup, &original) {
        return Err(io_fail(BACKUP_WHY));
    }

    // The replace: through a temporary file in the same folder, in one attempt.
    const REPLACE_WHY: &str = "the OpenSSH client configuration could not be updated";
    let tmp = write_temp(ssh_dir, "config", &updated).map_err(|_| io_fail(REPLACE_WHY))?;
    if replace_keeping_rules(&config, &tmp).is_err() {
        // If the config is gone, the temporary file is the only new copy and MUST be kept.
        if fs::symlink_metadata(&config).is_ok() {
            let _ = fs::remove_file(&tmp);
        }
        return Err(io_fail(REPLACE_WHY));
    }
    Ok(())
}

/// A new config holding exactly the line and one LF, with the folder's access rules.
fn create_config(config: &Path) -> Result<(), AliasFailure> {
    const WHY: &str = "the OpenSSH client configuration could not be created";
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config)
        .map_err(|_| io_fail(WHY))?;
    file.write_all(b"Include agentspace.d/*\n")
        .and_then(|()| file.sync_all())
        .map_err(|_| io_fail(WHY))
}

/// What is at an alias file's place.
enum Existing {
    Missing,
    /// A file whose owner mark names the caller (a lease or a host), with its bytes.
    Mine(Vec<u8>),
    /// Another owner's file, a file with no owner mark, or something that is not a file.
    Foreign,
}

/// Reads what is at `path` without following a link: a file is `Mine` when `is_mine` says its
/// owner mark names the caller, and anything else that is there is `Foreign`.
fn existing(
    path: &Path,
    is_mine: &dyn Fn(&[u8]) -> bool,
    read_why: &str,
) -> Result<Existing, AliasFailure> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Existing::Missing),
        Err(_) => return Err(io_fail(read_why)),
    };
    if !meta.file_type().is_file() {
        return Ok(Existing::Foreign);
    }
    let bytes = fs::read(path).map_err(|_| io_fail(read_why))?;
    if is_mine(&bytes) {
        Ok(Existing::Mine(bytes))
    } else {
        Ok(Existing::Foreign)
    }
}

/// Whether the owner mark of `bytes` names the lease `id`, compared without regard to ASCII case.
fn names_lease(bytes: &[u8], id: &str) -> bool {
    owner_mark(bytes).is_some_and(|mark| mark == id.to_lowercase())
}

/// Whether the owner mark of `bytes` names the host `project`, compared without regard to ASCII
/// case.
fn names_host(bytes: &[u8], project: &str) -> bool {
    host_owner_mark(bytes).is_some_and(|mark| mark == project.to_lowercase())
}

/// The write both kinds of alias file share: `name` is the file's name in `agentspace.d` and the
/// start of its temporary file's name, which is in `<ssh folder>`, never in `agentspace.d`.
fn write_owned(
    ssh_dir: &Path,
    name: &str,
    dest: &Path,
    bytes: &[u8],
    is_mine: &dyn Fn(&[u8]) -> bool,
    foreign_why: &str,
    write_why: &str,
) -> Result<(), AliasFailure> {
    check_folders(ssh_dir)?;
    match existing(dest, is_mine, write_why)? {
        Existing::Foreign => return Err(fail(AliasFailureKind::Foreign, foreign_why)),
        Existing::Mine(old) if old == bytes => return Ok(()),
        Existing::Mine(_) | Existing::Missing => {}
    }
    fs::create_dir_all(alias_dir(ssh_dir)).map_err(|_| io_fail(write_why))?;
    let tmp = write_temp(ssh_dir, name, bytes).map_err(|_| io_fail(write_why))?;
    if fs::rename(&tmp, dest).is_err() {
        let _ = fs::remove_file(&tmp);
        return Err(io_fail(write_why));
    }
    Ok(())
}

/// The owner check both kinds of alias file share: a missing file and one that names the caller
/// give `Ok`, anything else [`AliasFailureKind::Foreign`].
fn check_owned(
    ssh_dir: &Path,
    path: &Path,
    is_mine: &dyn Fn(&[u8]) -> bool,
    foreign_why: &str,
) -> Result<(), AliasFailure> {
    check_folders(ssh_dir)?;
    match existing(path, is_mine, "the host alias file could not be read")? {
        Existing::Foreign => Err(fail(AliasFailureKind::Foreign, foreign_why)),
        Existing::Missing | Existing::Mine(_) => Ok(()),
    }
}

/// The removal both kinds of alias file share, after the owner check.
fn remove_owned(
    ssh_dir: &Path,
    path: &Path,
    is_mine: &dyn Fn(&[u8]) -> bool,
    foreign_why: &str,
) -> Result<(), AliasFailure> {
    check_folders(ssh_dir)?;
    match existing(path, is_mine, "the host alias file could not be read")? {
        Existing::Foreign => Err(fail(AliasFailureKind::Foreign, foreign_why)),
        Existing::Missing => Ok(()),
        Existing::Mine(_) => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(io_fail("the host alias file could not be deleted")),
        },
    }
}

/// Writes the lease's alias file, `<ssh folder>\agentspace.d\<alias>` (docs/state.md, "The alias
/// file"), through `<ssh folder>\<alias>.agentspace-tmp-<32 hex digits>`, which is moved into
/// `agentspace.d` in one attempt.
///
/// An existing file that names this lease and holds the same bytes is left alone, and one that
/// names this lease and holds other bytes is replaced. Another lease's file, a host's operator
/// alias, a file with no owner mark, and something that is not a file give
/// [`AliasFailureKind::Foreign`] and change nothing. `id` is compared without regard to ASCII case.
pub fn write_alias(
    ssh_dir: &Path,
    alias: &str,
    id: &str,
    bytes: &[u8],
) -> Result<(), AliasFailure> {
    let dest = alias_path(ssh_dir, alias)?;
    write_owned(
        ssh_dir,
        alias,
        &dest,
        bytes,
        &|b| names_lease(b, id),
        FOREIGN_WHY,
        "the host alias file could not be written",
    )
}

/// Checks the owner mark of the alias file a `release` is about to remove (docs/state.md,
/// "Release, and what it leaves", step 1): a missing file and one that names this lease give `Ok`.
/// Another lease's file, a file with no owner mark, and something that is not a file, a folder
/// included, give [`AliasFailureKind::Foreign`].
pub fn check_alias_owner(ssh_dir: &Path, alias: &str, id: &str) -> Result<(), AliasFailure> {
    let path = alias_path(ssh_dir, alias)?;
    check_owned(ssh_dir, &path, &|b| names_lease(b, id), FOREIGN_WHY)
}

/// Removes the lease's alias file. The owner mark is checked first, as [`check_alias_owner`] does,
/// and nothing is deleted when it fails. A missing file is `Ok`. A failure to delete is
/// [`AliasFailureKind::Io`].
pub fn remove_alias(ssh_dir: &Path, alias: &str, id: &str) -> Result<(), AliasFailure> {
    let path = alias_path(ssh_dir, alias)?;
    remove_owned(ssh_dir, &path, &|b| names_lease(b, id), FOREIGN_WHY)
}

/// `<ssh folder>\agentspace.d\agentspace-host.<host name>` and its name. `project` is the host's
/// name: the project for a `linux/arm64` host, and from v0.12 `<project>.<os>-<arch>` for a host of
/// another platform (D91). One that is not a host's name, as the crate's `is_valid_host_name` says,
/// is refused, so that no path can leave the folder and no two hosts share a file (Windows drops a
/// trailing dot).
fn host_alias_path(ssh_dir: &Path, project: &str) -> Result<(String, PathBuf), AliasFailure> {
    let name = host_alias_name(project);
    if !is_valid_host_name(project) || !is_valid_alias(&name) {
        return Err(io_fail(INVALID_ALIAS_WHY));
    }
    let path = alias_dir(ssh_dir).join(&name);
    Ok((name, path))
}

/// Writes a host's operator alias, `<ssh folder>\agentspace.d\agentspace-host.<project>` (docs/
/// state.md, "A host's operator alias"), through
/// `<ssh folder>\agentspace-host.<project>.agentspace-tmp-<32 hex digits>`, which is moved into
/// `agentspace.d` in one attempt.
///
/// An existing file that names this host and holds the same bytes is left alone, and one that names
/// this host and holds other bytes is replaced. Another host's file, a lease's alias, a file with
/// no owner mark, and something that is not a file, a symbolic link or another reparse point
/// included, give [`AliasFailureKind::Foreign`] and change nothing. `project` is compared without
/// regard to ASCII case.
pub fn write_host_alias(ssh_dir: &Path, project: &str, bytes: &[u8]) -> Result<(), AliasFailure> {
    let (name, dest) = host_alias_path(ssh_dir, project)?;
    write_owned(
        ssh_dir,
        &name,
        &dest,
        bytes,
        &|b| names_host(b, project),
        HOST_FOREIGN_WHY,
        "the operator's alias file for the host could not be written",
    )
}

/// Checks the owner mark of a host's operator alias: a missing file and one that names this host
/// give `Ok`. Another host's file, a lease's alias, a file with no owner mark, and something that
/// is not a file give [`AliasFailureKind::Foreign`].
pub fn check_host_alias_owner(ssh_dir: &Path, project: &str) -> Result<(), AliasFailure> {
    let (_, path) = host_alias_path(ssh_dir, project)?;
    check_owned(
        ssh_dir,
        &path,
        &|b| names_host(b, project),
        HOST_FOREIGN_WHY,
    )
}

/// Removes a host's operator alias. The owner mark is checked first, as [`check_host_alias_owner`]
/// does, and nothing is deleted when it fails. A missing file is `Ok`. A failure to delete is
/// [`AliasFailureKind::Io`].
///
/// **No agentspace command calls this**: a host's operator alias is never removed by agentspace
/// (docs/state.md, "A host's operator alias"). It exists for a person's tooling and for tests.
pub fn remove_host_alias(ssh_dir: &Path, project: &str) -> Result<(), AliasFailure> {
    let (_, path) = host_alias_path(ssh_dir, project)?;
    remove_owned(
        ssh_dir,
        &path,
        &|b| names_host(b, project),
        HOST_FOREIGN_WHY,
    )
}

/// The ProxyCommand program beside the running executable (docs/operator.md, "How each program
/// finds the others"): `agentspace-ssh-proxy.exe`, or when that is not there,
/// `agentspace-ssh-proxy.cmd`, as a path in the executable's own folder. `None` when neither is a
/// file there. `own_exe` is the executable the operating system started, as a full path: a
/// relative one gives `None`, because the working folder is never searched.
pub fn find_proxy_program(own_exe: &Path) -> Option<PathBuf> {
    if !own_exe.is_absolute() {
        return None;
    }
    let folder = own_exe.parent()?;
    ["agentspace-ssh-proxy.exe", "agentspace-ssh-proxy.cmd"]
        .iter()
        .map(|name| folder.join(name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scratch() -> Scratch {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p =
            std::env::temp_dir().join(format!("agentspace-alias-unit-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }

    const ID: &str = "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";
    const OTHER: &str = "aaaaaaaa-5b7d-4a60-8c11-0d2e4f6a8b90";

    fn file_bytes(id: &str, extra: &str) -> Vec<u8> {
        format!("# agentspace lease {id}; test\nHost x\n{extra}").into_bytes()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn utc_time_is_formatted_as_the_name_needs() {
        assert_eq!(format_utc(0), "19700101T000000Z");
        assert_eq!(format_utc(1_000_000_000), "20010909T014640Z");
        assert_eq!(format_utc(1_795_129_200), "20261119T230000Z");
        assert_eq!(format_utc(951_782_400), "20000229T000000Z");
        assert_eq!(format_utc(-1), "19691231T235959Z");
        // A silly time still gives a name and does not panic.
        assert!(!format_utc(i64::MAX).is_empty());
        assert!(!format_utc(i64::MIN).is_empty());
    }

    #[test]
    fn random_names_are_32_hex_digits_and_differ() {
        let a = random_hex().unwrap();
        let b = random_hex().unwrap();
        assert_eq!(a.len(), 32);
        assert!(
            a.bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(a, b);
    }

    #[test]
    fn no_config_makes_one_with_the_line_and_one_lf_and_no_backup() {
        let s = scratch();
        let ssh = s.0.join("ssh dir %");
        ensure_include(&ssh, 0).unwrap();
        assert_eq!(
            fs::read(ssh.join("config")).unwrap(),
            b"Include agentspace.d/*\n"
        );
        assert!(ssh.join("agentspace.d").is_dir());
        assert_eq!(names(&ssh), ["agentspace.d", "config"]);
    }

    #[test]
    fn a_found_line_writes_nothing_but_still_makes_the_folder() {
        let s = scratch();
        let before = b"# top\ninclude   agentspace.d/*\nHost a\n  User b\n";
        fs::write(s.0.join("config"), before).unwrap();
        ensure_include(&s.0, 5).unwrap();
        assert_eq!(fs::read(s.0.join("config")).unwrap(), before);
        assert_eq!(names(&s.0), ["agentspace.d", "config"]);
    }

    #[test]
    fn a_line_below_a_block_is_a_config_failure_and_changes_nothing() {
        let s = scratch();
        let before = b"Host a\n  User b\nInclude agentspace.d/*\n";
        fs::write(s.0.join("config"), before).unwrap();
        let e = ensure_include(&s.0, 5).unwrap_err();
        assert_eq!(e.kind, AliasFailureKind::Config);
        assert!(e.message.contains("has to move to the top of the file"));
        assert_eq!(fs::read(s.0.join("config")).unwrap(), before);
        assert_eq!(names(&s.0), ["agentspace.d", "config"]);
    }

    #[test]
    fn an_insert_backs_up_first_and_keeps_every_original_byte() {
        let s = scratch();
        let before = b"\xEF\xBB\xBFHost a\r\n  User b".to_vec();
        fs::write(s.0.join("config"), &before).unwrap();
        ensure_include(&s.0, 1_795_129_200).unwrap();
        assert_eq!(
            fs::read(s.0.join("config")).unwrap(),
            b"\xEF\xBB\xBFInclude agentspace.d/*\r\nHost a\r\n  User b"
        );
        assert_eq!(
            fs::read(s.0.join("config.agentspace-backup-20261119T230000Z")).unwrap(),
            before
        );
        // No temporary file stays behind.
        assert_eq!(
            names(&s.0),
            [
                "agentspace.d",
                "config",
                "config.agentspace-backup-20261119T230000Z"
            ]
        );
    }

    #[test]
    fn a_second_insert_in_the_same_second_does_not_overwrite_the_backup() {
        let s = scratch();
        fs::write(s.0.join("config"), b"Host a\n").unwrap();
        let backup = s.0.join("config.agentspace-backup-19700101T000000Z");
        fs::write(&backup, b"older").unwrap();
        let e = ensure_include(&s.0, 0).unwrap_err();
        assert_eq!(e.kind, AliasFailureKind::Io);
        assert_eq!(fs::read(&backup).unwrap(), b"older");
        assert_eq!(fs::read(s.0.join("config")).unwrap(), b"Host a\n");
    }

    #[test]
    fn a_read_only_config_is_refused_before_any_backup() {
        let s = scratch();
        let config = s.0.join("config");
        fs::write(&config, b"Host a\n").unwrap();
        let mut perms = fs::metadata(&config).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&config, perms.clone()).unwrap();
        let e = ensure_include(&s.0, 0).unwrap_err();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(&config, perms).unwrap();
        assert_eq!(e.kind, AliasFailureKind::Config);
        assert!(e.message.contains("link or read-only"));
        assert_eq!(names(&s.0), ["agentspace.d", "config"]);
        assert_eq!(fs::read(&config).unwrap(), b"Host a\n");
    }

    #[test]
    fn a_config_that_is_a_folder_is_a_config_failure() {
        let s = scratch();
        fs::create_dir(s.0.join("config")).unwrap();
        let e = ensure_include(&s.0, 0).unwrap_err();
        assert_eq!(e.kind, AliasFailureKind::Config);
    }

    #[test]
    fn an_agentspace_d_that_is_a_file_is_an_io_failure() {
        let s = scratch();
        fs::write(s.0.join("agentspace.d"), b"x").unwrap();
        let e = ensure_include(&s.0, 0).unwrap_err();
        assert_eq!(e.kind, AliasFailureKind::Io);
    }

    #[test]
    fn a_new_alias_is_written_and_no_temporary_file_stays() {
        let s = scratch();
        let bytes = file_bytes(ID, "  User u\n");
        write_alias(&s.0, "agentspace-p-3f2a9c1e", ID, &bytes).unwrap();
        assert_eq!(
            fs::read(s.0.join("agentspace.d").join("agentspace-p-3f2a9c1e")).unwrap(),
            bytes
        );
        assert_eq!(names(&s.0), ["agentspace.d"]);
        assert_eq!(names(&s.0.join("agentspace.d")), ["agentspace-p-3f2a9c1e"]);
    }

    #[test]
    fn the_same_bytes_write_nothing_and_other_bytes_replace() {
        let s = scratch();
        let a = file_bytes(ID, "  User u\n");
        write_alias(&s.0, "x", ID, &a).unwrap();
        let path = s.0.join("agentspace.d").join("x");
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        write_alias(&s.0, "x", &ID.to_uppercase(), &a).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        let b = file_bytes(ID, "  User v\n");
        write_alias(&s.0, "x", ID, &b).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b);
        assert_eq!(names(&s.0.join("agentspace.d")), ["x"]);
        assert_eq!(names(&s.0), ["agentspace.d"]);
    }

    #[test]
    fn another_leases_file_and_an_unmarked_file_and_a_folder_are_refused() {
        let s = scratch();
        let dir = s.0.join("agentspace.d");
        fs::create_dir(&dir).unwrap();
        let theirs = file_bytes(OTHER, "  User u\n");
        fs::write(dir.join("a"), &theirs).unwrap();
        fs::write(dir.join("b"), b"Host b\n").unwrap();
        fs::create_dir(dir.join("c")).unwrap();
        let mine = file_bytes(ID, "  User u\n");
        for alias in ["a", "b", "c"] {
            let e = write_alias(&s.0, alias, ID, &mine).unwrap_err();
            assert_eq!(e.kind, AliasFailureKind::Foreign, "{alias}");
            let e = check_alias_owner(&s.0, alias, ID).unwrap_err();
            assert_eq!(e.kind, AliasFailureKind::Foreign, "{alias}");
            let e = remove_alias(&s.0, alias, ID).unwrap_err();
            assert_eq!(e.kind, AliasFailureKind::Foreign, "{alias}");
        }
        assert_eq!(fs::read(dir.join("a")).unwrap(), theirs);
        assert_eq!(fs::read(dir.join("b")).unwrap(), b"Host b\n");
        assert!(dir.join("c").is_dir());
        assert_eq!(names(&dir), ["a", "b", "c"]);
        assert_eq!(names(&s.0), ["agentspace.d"]);
    }

    #[test]
    fn remove_deletes_this_leases_file_and_a_missing_file_is_fine() {
        let s = scratch();
        remove_alias(&s.0, "x", ID).unwrap();
        check_alias_owner(&s.0, "x", ID).unwrap();
        write_alias(&s.0, "x", ID, &file_bytes(ID, "")).unwrap();
        check_alias_owner(&s.0, "x", ID).unwrap();
        remove_alias(&s.0, "x", ID).unwrap();
        assert!(names(&s.0.join("agentspace.d")).is_empty());
    }

    #[test]
    fn a_host_project_the_registry_does_not_name_is_refused_and_nothing_is_written() {
        let s = scratch();
        for project in ["A", "a_b", "a.b", "..", "x.", "-a", "a-", "a\n", ""] {
            let e = write_host_alias(&s.0, project, b"x").unwrap_err();
            assert_eq!(e.kind, AliasFailureKind::Io, "{project:?}");
            assert!(
                check_host_alias_owner(&s.0, project).is_err(),
                "{project:?}"
            );
            assert!(remove_host_alias(&s.0, project).is_err(), "{project:?}");
        }
        assert!(names(&s.0).is_empty());
        // `x` and `x.` would be one file on Windows; only the first is a project.
        write_host_alias(&s.0, "x", b"# agentspace host x; t\n").unwrap();
        assert!(write_host_alias(&s.0, "x.", b"# agentspace host x.; t\n").is_err());
        assert_eq!(
            fs::read(s.0.join("agentspace.d").join("agentspace-host.x")).unwrap(),
            b"# agentspace host x; t\n"
        );
    }

    #[test]
    fn a_name_that_could_leave_the_folder_is_refused() {
        let s = scratch();
        for alias in ["..", ".", "a/b", "a\\b", "-x", ""] {
            assert!(write_alias(&s.0, alias, ID, b"x").is_err(), "{alias:?}");
            assert!(remove_alias(&s.0, alias, ID).is_err(), "{alias:?}");
            assert!(check_alias_owner(&s.0, alias, ID).is_err(), "{alias:?}");
        }
        assert!(names(&s.0).is_empty());
    }

    /// `icacls` for a file, without the file's own path: its access list. The owner is not kept
    /// by the replace, and is not compared (architect, 2026-11-19; docs/state.md, Replacing a file).
    #[cfg(windows)]
    fn rules(path: &Path) -> String {
        use std::process::Command;
        let p = path.to_string_lossy().into_owned();
        let icacls = Command::new("icacls").arg(&p).output().unwrap();
        String::from_utf8_lossy(&icacls.stdout).replace(&p, "")
    }

    #[cfg(windows)]
    #[test]
    fn an_insert_keeps_the_configs_own_access_list() {
        use std::process::Command;
        let s = scratch();
        let config = s.0.join("config");
        fs::write(&config, b"Host a\n").unwrap();
        // An access list of its own: protected, and the current user alone, read and write.
        let who = Command::new("whoami").output().unwrap();
        let who = String::from_utf8_lossy(&who.stdout).trim().to_string();
        let out = Command::new("icacls")
            .arg(&config)
            .args(["/inheritance:r", "/grant:r"])
            .arg(format!("{who}:(R,W)"))
            .output()
            .unwrap();
        assert!(out.status.success());
        let before = rules(&config);
        ensure_include(&s.0, 0).unwrap();
        assert_eq!(rules(&config), before);
        assert!(before.contains("(R,W)"), "{before}");
        assert_eq!(
            fs::read(&config).unwrap(),
            b"Include agentspace.d/*\nHost a\n"
        );
    }

    /// Windows OpenSSH reads an alias file this module wrote, in a folder whose path holds a space
    /// and a `%`. The throwaway folder's access list is made OpenSSH-safe first, because the temp
    /// folder of some machines grants other principals access, which ssh refuses.
    #[cfg(windows)]
    #[test]
    fn openssh_reads_a_written_alias() {
        use crate::{AliasFile, alias_file_bytes};
        use std::process::Command;
        let s = scratch();
        let ssh_dir = s.0.join("ssh dir %7");
        fs::create_dir_all(&ssh_dir).unwrap();
        let who = String::from_utf8_lossy(&Command::new("whoami").output().unwrap().stdout)
            .trim()
            .to_string();
        let out = Command::new("icacls")
            .arg(&ssh_dir)
            .args(["/inheritance:r", "/grant:r"])
            .arg(format!("{who}:(OI)(CI)F"))
            .arg("/grant:r")
            .arg("SYSTEM:(OI)(CI)F")
            .output()
            .unwrap();
        assert!(out.status.success());
        let alias = "agentspace-p-3f2a9c1e";
        let bytes = alias_file_bytes(&AliasFile {
            id: ID,
            comment: "test",
            alias,
            user: "ubuntu",
            key_path: "C:/Users/x y/%%/id",
            cert_path: "C:/Users/x y/%%/id-cert.pub",
            host_key_alias: "agentspace",
            known_hosts: None,
            proxy_command: "\"C:\\a b\\p.exe\" agentspace-p-3f2a9c1e",
        })
        .unwrap();
        ensure_include(&ssh_dir, 0).unwrap();
        write_alias(&ssh_dir, alias, ID, &bytes).unwrap();
        let cfg = s.0.join("top");
        let inc = ssh_dir.join("agentspace.d").join("*");
        fs::write(
            &cfg,
            format!("Include \"{}\"\n", inc.to_string_lossy().replace('\\', "/")),
        )
        .unwrap();
        let root = std::env::var("SystemRoot").unwrap();
        let out = Command::new(format!("{root}\\System32\\OpenSSH\\ssh.exe"))
            .env("USERPROFILE", &s.0)
            .arg("-G")
            .arg("-F")
            .arg(&cfg)
            .arg(alias)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains("\nuser ubuntu\n"), "{text}");
        assert!(text.contains("\nhostkeyalias agentspace\n"), "{text}");
        assert!(
            text.contains("\nidentityfile C:/Users/x y/%%/id\n"),
            "{text}"
        );
        assert!(
            text.contains("\nproxycommand \"C:\\a b\\p.exe\" agentspace-p-3f2a9c1e\n"),
            "{text}"
        );
    }

    #[test]
    fn messages_hold_no_path() {
        let s = scratch();
        let ssh = s.0.to_string_lossy().into_owned();
        fs::write(s.0.join("agentspace.d"), b"x").unwrap();
        let e = ensure_include(&s.0, 0).unwrap_err();
        assert!(!e.message.contains(&ssh));
    }

    #[test]
    fn the_proxy_program_is_found_beside_the_executable() {
        let s = scratch();
        let exe = s.0.join("agentspace.exe");
        assert_eq!(find_proxy_program(&exe), None);
        let cmd = s.0.join("agentspace-ssh-proxy.cmd");
        fs::write(&cmd, b"@echo off\r\n").unwrap();
        assert_eq!(find_proxy_program(&exe), Some(cmd.clone()));
        let proxy = s.0.join("agentspace-ssh-proxy.exe");
        fs::write(&proxy, b"x").unwrap();
        assert_eq!(find_proxy_program(&exe), Some(proxy));
        // A folder of that name is not a program.
        fs::remove_file(s.0.join("agentspace-ssh-proxy.exe")).unwrap();
        fs::remove_file(&cmd).unwrap();
        fs::create_dir(s.0.join("agentspace-ssh-proxy.exe")).unwrap();
        assert_eq!(find_proxy_program(&exe), None);
        assert_eq!(find_proxy_program(Path::new("")), None);
    }

    fn host_bytes(project: &str, extra: &str) -> Vec<u8> {
        format!("# agentspace host {project}; test\nHost x\n{extra}").into_bytes()
    }

    #[test]
    fn a_host_alias_is_written_replaced_checked_and_removed() {
        let s = scratch();
        let a = host_bytes("proj", "  User u\n");
        let path = s.0.join("agentspace.d").join("agentspace-host.proj");
        check_host_alias_owner(&s.0, "proj").unwrap();
        remove_host_alias(&s.0, "proj").unwrap();
        write_host_alias(&s.0, "proj", &a).unwrap();
        assert_eq!(fs::read(&path).unwrap(), a);
        // The temporary file does not stay, in the ssh folder or in agentspace.d.
        assert_eq!(names(&s.0), ["agentspace.d"]);
        assert_eq!(names(&s.0.join("agentspace.d")), ["agentspace-host.proj"]);
        // The same bytes write nothing, other bytes replace.
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        write_host_alias(&s.0, "proj", &a).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        let b = host_bytes("PROJ", "  User v\n");
        write_host_alias(&s.0, "proj", &b).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b);
        check_host_alias_owner(&s.0, "proj").unwrap();
        remove_host_alias(&s.0, "proj").unwrap();
        assert!(names(&s.0.join("agentspace.d")).is_empty());
    }

    /// From v0.12 a host of another platform has its own file, named by its host name, and its
    /// mark names that host alone (docs/state.md, "A host's operator alias"; D91, pin P12).
    #[test]
    fn another_platforms_host_alias_is_its_own_file_and_mark() {
        let s = scratch();
        let arm = host_bytes("proj", "");
        let win = host_bytes("proj.windows-amd64", "");
        write_host_alias(&s.0, "proj", &arm).unwrap();
        write_host_alias(&s.0, "proj.windows-amd64", &win).unwrap();
        let mut found = names(&s.0.join("agentspace.d"));
        found.sort();
        assert_eq!(
            found,
            ["agentspace-host.proj", "agentspace-host.proj.windows-amd64"]
        );
        assert_eq!(
            check_host_alias_owner(&s.0, "proj.windows-amd64").map_err(|e| e.kind),
            Ok(())
        );
        // The linux/arm64 host's mark does not name another platform's host: a file at that host's
        // name that carries it is foreign, and is neither replaced nor removed.
        let other =
            s.0.join("agentspace.d")
                .join("agentspace-host.proj.linux-amd64");
        fs::write(&other, &arm).unwrap();
        for result in [
            check_host_alias_owner(&s.0, "proj.linux-amd64"),
            write_host_alias(&s.0, "proj.linux-amd64", &win),
            remove_host_alias(&s.0, "proj.linux-amd64"),
        ] {
            assert_eq!(result.unwrap_err().kind, AliasFailureKind::Foreign);
        }
        assert_eq!(fs::read(&other).unwrap(), arm);
        // A name that is no host's is refused before any path is made.
        for bad in [
            "proj.linux-arm64",
            "proj.windows",
            "proj.windows-amd64.x",
            "proj.",
            "proj.Windows-amd64",
            "proj.windows-amd64-v3",
            "proj.windows/amd64",
            "proj..windows-amd64",
        ] {
            assert!(write_host_alias(&s.0, bad, &win).is_err(), "{bad:?}");
            assert!(check_host_alias_owner(&s.0, bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_host_alias_refuses_what_is_not_its_own_and_leaves_it() {
        let s = scratch();
        let dir = s.0.join("agentspace.d");
        fs::create_dir(&dir).unwrap();
        let mine = host_bytes("proj", "");
        let files: [(&str, Vec<u8>); 3] = [
            ("agentspace-host.a", host_bytes("other", "")),
            ("agentspace-host.b", file_bytes(ID, "")),
            ("agentspace-host.c", b"Host c\n".to_vec()),
        ];
        for (name, bytes) in &files {
            fs::write(dir.join(name), bytes).unwrap();
        }
        fs::create_dir(dir.join("agentspace-host.d")).unwrap();
        for (project, expect_kept) in [("a", 0), ("b", 1), ("c", 2), ("d", 3)] {
            let e = write_host_alias(&s.0, project, &mine).unwrap_err();
            assert_eq!(e.kind, AliasFailureKind::Foreign, "{project}");
            let e = check_host_alias_owner(&s.0, project).unwrap_err();
            assert_eq!(e.kind, AliasFailureKind::Foreign, "{project}");
            let e = remove_host_alias(&s.0, project).unwrap_err();
            assert_eq!(e.kind, AliasFailureKind::Foreign, "{project}");
            if let Some((name, bytes)) = files.get(expect_kept) {
                assert_eq!(fs::read(dir.join(name)).unwrap(), *bytes, "{project}");
            }
        }
        assert!(dir.join("agentspace-host.d").is_dir());
        assert_eq!(names(&s.0), ["agentspace.d"]);
    }

    #[test]
    fn the_two_owner_checks_never_cross() {
        let s = scratch();
        let lease = file_bytes(ID, "");
        let host = host_bytes("proj", "");
        let path = s.0.join("agentspace.d").join("agentspace-host.proj");
        // An operator alias is no lease's: a lease's write, check and remove refuse it.
        write_host_alias(&s.0, "proj", &host).unwrap();
        let alias = "agentspace-host.proj";
        assert_eq!(
            write_alias(&s.0, alias, ID, &lease).unwrap_err().kind,
            AliasFailureKind::Foreign
        );
        assert_eq!(
            check_alias_owner(&s.0, alias, ID).unwrap_err().kind,
            AliasFailureKind::Foreign
        );
        assert_eq!(
            remove_alias(&s.0, alias, ID).unwrap_err().kind,
            AliasFailureKind::Foreign
        );
        assert_eq!(fs::read(&path).unwrap(), host);
        // A lease's alias is no host's: the host's write and remove refuse it.
        fs::write(&path, &lease).unwrap();
        assert_eq!(
            write_host_alias(&s.0, "proj", &host).unwrap_err().kind,
            AliasFailureKind::Foreign
        );
        assert_eq!(
            remove_host_alias(&s.0, "proj").unwrap_err().kind,
            AliasFailureKind::Foreign
        );
        assert_eq!(fs::read(&path).unwrap(), lease);
    }

    #[test]
    fn a_host_alias_name_that_could_leave_the_folder_is_refused() {
        let s = scratch();
        for project in ["", "a/b", "a\\b", "a b", "a;b"] {
            assert!(
                write_host_alias(&s.0, project, b"x").is_err(),
                "{project:?}"
            );
            assert!(remove_host_alias(&s.0, project).is_err(), "{project:?}");
            assert!(
                check_host_alias_owner(&s.0, project).is_err(),
                "{project:?}"
            );
        }
        assert!(names(&s.0).is_empty());
    }

    #[test]
    fn a_backup_is_checked_byte_for_byte_by_its_hash() {
        let s = scratch();
        let original = b"Host a\r\n  User b\n";
        let copy = s.0.join("copy");
        fs::write(&copy, original).unwrap();
        assert!(backup_matches(&copy, original));
        // One byte different, the same length, a prefix, an extension, nothing, and no file.
        for other in [
            &b"Host a\r\n  User c\n"[..],
            b"Host a\r\n  User b",
            b"Host a\r\n  User b\n\n",
            b"",
        ] {
            fs::write(&copy, other).unwrap();
            assert!(!backup_matches(&copy, original), "{other:?}");
        }
        assert!(!backup_matches(&s.0.join("missing"), original));
    }
}

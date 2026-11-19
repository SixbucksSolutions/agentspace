//! `write_alias`, `check_alias_owner` and `remove_alias` on Windows (docs/state.md, "The alias
//! file" and "Release and what it leaves"). Every folder is a throwaway under the temp folder.
#![cfg(windows)]

mod common;

use agentspace_alias::*;
use common::*;
use std::fs;
use std::path::PathBuf;

const PROXY: &str = "\"C:\\bin\\agentspace-ssh-proxy.exe\" agentspace-swimcoach-3f2a9c1e";

fn world(tag: &str) -> (Scratch, PathBuf, PathBuf) {
    let s = scratch(tag);
    let dir = ssh_dir(&s);
    ensure_include(&dir, NOW).unwrap();
    let file = dir.join("agentspace.d").join(ALIAS);
    (s, dir, file)
}

fn bytes_for(s: &Scratch, id: &str) -> Vec<u8> {
    alias_bytes(&s.0.join("home"), id, ALIAS, PROXY)
}

fn set_readonly(p: &std::path::Path, on: bool) {
    let mut perm = fs::metadata(p).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(on);
    fs::set_permissions(p, perm).unwrap();
}

#[test]
fn write_alias_puts_the_bytes_in_agentspace_d_and_leaves_no_temporary_file() {
    let (s, dir, file) = world("w1");
    let bytes = bytes_for(&s, ID);
    write_alias(&dir, ALIAS, ID, &bytes).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
    // No temporary file in the ssh folder, and nothing but the alias file in agentspace.d.
    assert!(tmps(&dir).is_empty(), "{:?}", names(&dir));
    assert_eq!(names(&dir.join("agentspace.d")), [ALIAS]);
}

#[test]
fn an_alias_file_in_the_state_md_form_is_recognized_as_its_leases() {
    let (s, dir, file) = world("w2");
    let bytes = bytes_for(&s, ID);
    fs::write(&file, &bytes).unwrap();
    check_alias_owner(&dir, ALIAS, ID).unwrap();
    // Not another lease's.
    let err = check_alias_owner(&dir, ALIAS, OTHER_ID).expect_err("another lease's file");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
}

#[test]
fn the_same_lease_with_the_same_bytes_writes_nothing() {
    let (s, dir, file) = world("w3");
    let bytes = bytes_for(&s, ID);
    write_alias(&dir, ALIAS, ID, &bytes).unwrap();
    // A read-only file makes any replace fail, so success proves nothing was written.
    set_readonly(&file, true);
    write_alias(&dir, ALIAS, ID, &bytes).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(tmps(&dir).is_empty());
}

#[test]
fn the_same_lease_with_other_bytes_is_replaced() {
    let (s, dir, file) = world("w4");
    let old = bytes_for(&s, ID);
    write_alias(&dir, ALIAS, ID, &old).unwrap();
    let mut new = old.clone();
    new.extend_from_slice(b"  ForwardAgent no\n");
    write_alias(&dir, ALIAS, ID, &new).unwrap();
    assert_eq!(fs::read(&file).unwrap(), new);
    assert!(tmps(&dir).is_empty());
    assert_eq!(names(&dir.join("agentspace.d")), [ALIAS]);
}

#[test]
fn another_leases_alias_file_is_never_replaced() {
    let (s, dir, file) = world("w5");
    let theirs = bytes_for(&s, OTHER_ID);
    fs::write(&file, &theirs).unwrap();
    let err = write_alias(&dir, ALIAS, ID, &bytes_for(&s, ID)).expect_err("a foreign file");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
    assert_eq!(fs::read(&file).unwrap(), theirs);
    assert!(tmps(&dir).is_empty(), "a temporary file was left behind");
}

#[test]
fn a_file_with_no_owner_mark_is_never_replaced() {
    for theirs in [
        &b"Host something\n  User me\n"[..],
        b"",
        b"# agentspace lease no-semicolon\n",
        b"\n# agentspace lease 3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90; second line\n",
    ] {
        let (s, dir, file) = world("w6");
        fs::write(&file, theirs).unwrap();
        let err = write_alias(&dir, ALIAS, ID, &bytes_for(&s, ID)).expect_err("no owner mark");
        assert_eq!(err.kind, AliasFailureKind::Foreign);
        assert_eq!(fs::read(&file).unwrap(), theirs);
        assert!(tmps(&dir).is_empty());
    }
}

#[test]
fn a_folder_at_the_alias_files_path_refuses_the_write() {
    let (s, dir, file) = world("w7");
    fs::create_dir(&file).unwrap();
    fs::write(file.join("keep.txt"), b"x").unwrap();
    let r = write_alias(&dir, ALIAS, ID, &bytes_for(&s, ID));
    let err = r.expect_err("a folder at the path");
    assert_plain_message(&err.message, &s);
    assert!(file.is_dir());
    assert_eq!(fs::read(file.join("keep.txt")).unwrap(), b"x");
    assert!(tmps(&dir).is_empty(), "a temporary file was left behind");
}

#[test]
fn remove_alias_deletes_its_own_file() {
    let (s, dir, file) = world("r1");
    write_alias(&dir, ALIAS, ID, &bytes_for(&s, ID)).unwrap();
    remove_alias(&dir, ALIAS, ID).unwrap();
    assert!(!file.exists());
    // agentspace.d stays: agentspace never removes it.
    assert!(dir.join("agentspace.d").is_dir());
}

#[test]
fn remove_alias_of_a_missing_file_is_not_a_failure() {
    let (_s, dir, file) = world("r2");
    assert!(!file.exists());
    remove_alias(&dir, ALIAS, ID).unwrap();
    // Also when agentspace.d itself is missing.
    let s2 = scratch("r2b");
    let dir2 = ssh_dir(&s2);
    remove_alias(&dir2, ALIAS, ID).unwrap();
}

#[test]
fn remove_alias_never_deletes_another_leases_file() {
    let (s, dir, file) = world("r3");
    let theirs = bytes_for(&s, OTHER_ID);
    fs::write(&file, &theirs).unwrap();
    let err = remove_alias(&dir, ALIAS, ID).expect_err("another lease's file");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
    assert_eq!(fs::read(&file).unwrap(), theirs);
}

#[test]
fn remove_alias_never_deletes_a_file_with_no_owner_mark() {
    let (_s, dir, file) = world("r4");
    fs::write(&file, b"Host mine\n  User me\n").unwrap();
    let err = remove_alias(&dir, ALIAS, ID).expect_err("no owner mark");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_eq!(fs::read(&file).unwrap(), b"Host mine\n  User me\n");
}

#[test]
fn remove_alias_refuses_a_folder_at_the_path_and_deletes_nothing() {
    let (s, dir, file) = world("r5");
    fs::create_dir(&file).unwrap();
    fs::write(file.join("inside.txt"), b"x").unwrap();
    let err = remove_alias(&dir, ALIAS, ID).expect_err("a folder at the path");
    assert_plain_message(&err.message, &s);
    assert!(file.is_dir());
    assert_eq!(fs::read(file.join("inside.txt")).unwrap(), b"x");
    // An empty folder is refused too.
    let (_s2, dir2, file2) = world("r5b");
    fs::create_dir(&file2).unwrap();
    assert!(remove_alias(&dir2, ALIAS, ID).is_err());
    assert!(file2.is_dir());
}

#[test]
fn check_alias_owner_refuses_a_file_with_no_mark() {
    let (_s, dir, file) = world("c1");
    fs::write(&file, b"Host mine\n").unwrap();
    assert!(check_alias_owner(&dir, ALIAS, ID).is_err());
}

/// Every one of the four calls that reach `agentspace.d`, with the same arguments, in the order
/// the tests below name them in their failure text.
fn every_call(dir: &std::path::Path, s: &Scratch) -> Vec<(&'static str, Result<(), AliasFailure>)> {
    vec![
        ("ensure_include", ensure_include(dir, NOW)),
        (
            "write_alias",
            write_alias(dir, ALIAS, ID, &bytes_for(s, ID)),
        ),
        ("check_alias_owner", check_alias_owner(dir, ALIAS, ID)),
        ("remove_alias", remove_alias(dir, ALIAS, ID)),
    ]
}

#[test]
fn a_file_where_the_ssh_folder_must_be_is_refused_and_nothing_changes() {
    let s = scratch("f1");
    let home = s.0.join("home");
    fs::create_dir_all(&home).unwrap();
    let dir = home.join(".ssh");
    fs::write(&dir, b"not a folder").unwrap();
    for (call, result) in every_call(&dir, &s) {
        let err = result.expect_err(call);
        assert_eq!(err.kind, AliasFailureKind::Io, "{call}");
        assert!(err.message.contains("is a file, not a folder"), "{call}");
        assert_plain_message(&err.message, &s);
    }
    assert_eq!(fs::read(&dir).unwrap(), b"not a folder");
    assert_eq!(names(&home), [".ssh"]);
    assert_eq!(names(&s.0), ["home"]);
}

#[test]
fn a_file_where_agentspace_d_must_be_is_refused_and_nothing_changes() {
    let s = scratch("f2");
    let dir = ssh_dir(&s);
    fs::write(dir.join("config"), b"Host a\n").unwrap();
    let d = dir.join("agentspace.d");
    fs::write(&d, b"not a folder").unwrap();
    for (call, result) in every_call(&dir, &s) {
        let err = result.expect_err(call);
        assert_eq!(err.kind, AliasFailureKind::Io, "{call}");
        assert!(err.message.contains("is a file, not a folder"), "{call}");
        assert_plain_message(&err.message, &s);
    }
    assert_eq!(fs::read(&d).unwrap(), b"not a folder");
    assert_eq!(fs::read(dir.join("config")).unwrap(), b"Host a\n");
    // No backup, no temporary file and no new entry of any kind.
    assert_eq!(names(&dir), ["agentspace.d", "config"]);
}

/// Denies the current user the right to add a file to `dir` itself (not inherited by what is
/// created under it), and lifts the denial again when dropped, so that the folder can be deleted.
struct NoNewFiles(std::path::PathBuf);

impl NoNewFiles {
    fn new(dir: &std::path::Path) -> Self {
        let user = std::env::var("USERNAME").unwrap();
        let out = std::process::Command::new("icacls")
            .arg(dir)
            .args(["/deny", &format!("{user}:(WD)")])
            .output()
            .unwrap();
        assert!(out.status.success(), "icacls could not deny the folder");
        NoNewFiles(dir.to_path_buf())
    }
}

impl Drop for NoNewFiles {
    fn drop(&mut self) {
        let user = std::env::var("USERNAME").unwrap();
        let _ = std::process::Command::new("icacls")
            .arg(&self.0)
            .args(["/remove:d", &user])
            .output();
    }
}

#[test]
fn the_temporary_file_is_made_in_the_ssh_folder_not_in_agentspace_d() {
    let (s, dir, file) = world("t1");
    let bytes = bytes_for(&s, ID);
    {
        // New files cannot be made in the ssh folder itself, while agentspace.d still takes them.
        let _deny = NoNewFiles::new(&dir);
        assert!(
            fs::write(dir.join("probe"), b"x").is_err(),
            "the ssh folder still takes new files"
        );
        fs::write(dir.join("agentspace.d").join("probe"), b"x").unwrap();
        fs::remove_file(dir.join("agentspace.d").join("probe")).unwrap();
        // The temporary file belongs in the ssh folder, so the write fails and leaves nothing.
        let err = write_alias(&dir, ALIAS, ID, &bytes).expect_err("no new file in the ssh folder");
        assert_eq!(err.kind, AliasFailureKind::Io);
        assert_plain_message(&err.message, &s);
        assert!(!file.exists());
        assert!(names(&dir.join("agentspace.d")).is_empty());
    }
    // With the denial lifted the same call works, so the denial was what stopped it.
    write_alias(&dir, ALIAS, ID, &bytes).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(tmps(&dir).is_empty());
}

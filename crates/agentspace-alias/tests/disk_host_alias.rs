//! `write_host_alias` and `remove_host_alias` on Windows, and the two owner marks never crossing
//! (docs/state.md, "A host's operator alias" and "The alias file"; E0.07-T02-S03, #0148). Every
//! folder is a throwaway under the temp folder.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

mod common;

use agentspace_alias::*;
use common::*;
use std::fs;
use std::path::{Path, PathBuf};

const PROJECT: &str = "examplehost";
const OTHER_PROJECT: &str = "otherhost";
const HOST_ALIAS: &str = "agentspace-host.examplehost";
const OTHER_HOST_ALIAS: &str = "agentspace-host.otherhost";
const PROXY: &str = "\"C:\\bin\\agentspace-ssh-proxy.exe\" agentspace-examplehost-3f2a9c1e";

fn world(tag: &str) -> (Scratch, PathBuf, PathBuf) {
    let s = scratch(tag);
    let dir = ssh_dir(&s);
    ensure_include(&dir, NOW).unwrap();
    let file = dir.join("agentspace.d").join(HOST_ALIAS);
    (s, dir, file)
}

/// The host alias file's bytes for `project`, through the public API.
fn host_bytes(project: &str, comment: &str) -> Vec<u8> {
    let key = format!("C:/Users/someone/.agentspace/hosts/{project}/id_ed25519");
    let cert = format!("C:/Users/someone/.agentspace/hosts/{project}/id_ed25519-cert.pub");
    let proxy = format!("\"C:\\bin\\agentspace-ssh-proxy.exe\" agentspace-host.{project}");
    host_alias_file_bytes(&HostAliasFile {
        project,
        comment,
        key_path: &key,
        cert_path: &cert,
        known_hosts: "C:/Users/someone/.agentspace/known_hosts",
        proxy_command: &proxy,
    })
    .unwrap()
}

fn lease_bytes(s: &Scratch, id: &str) -> Vec<u8> {
    alias_bytes(&s.0.join("home"), id, ALIAS, PROXY)
}

fn set_readonly(p: &Path, on: bool) {
    let mut perm = fs::metadata(p).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(on);
    fs::set_permissions(p, perm).unwrap();
}

// ---------- write_host_alias ----------

#[test]
fn write_host_alias_puts_the_bytes_in_agentspace_d_and_leaves_no_temporary_file() {
    let (_s, dir, file) = world("hw1");
    let bytes = host_bytes(PROJECT, "Written by agentspace. Do not edit.");
    write_host_alias(&dir, PROJECT, &bytes).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
    // No temporary file in the ssh folder, and nothing but the alias file in agentspace.d.
    assert!(tmps(&dir).is_empty(), "{:?}", names(&dir));
    assert_eq!(names(&dir.join("agentspace.d")), [HOST_ALIAS]);
    // The file's name is `agentspace-host.<project>`, and the first line is the host's owner mark.
    assert!(
        fs::read(&file)
            .unwrap()
            .starts_with(b"# agentspace host examplehost; ")
    );
}

#[test]
fn the_same_host_with_the_same_bytes_writes_nothing() {
    let (_s, dir, file) = world("hw2");
    let bytes = host_bytes(PROJECT, "c");
    write_host_alias(&dir, PROJECT, &bytes).unwrap();
    // A read-only file makes any replace fail, so success proves nothing was written.
    set_readonly(&file, true);
    write_host_alias(&dir, PROJECT, &bytes).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(tmps(&dir).is_empty());
}

#[test]
fn the_same_host_with_other_bytes_is_replaced() {
    let (_s, dir, file) = world("hw3");
    let old = host_bytes(PROJECT, "first comment");
    write_host_alias(&dir, PROJECT, &old).unwrap();
    let new = host_bytes(PROJECT, "second comment");
    assert_ne!(old, new);
    write_host_alias(&dir, PROJECT, &new).unwrap();
    assert_eq!(fs::read(&file).unwrap(), new);
    assert!(tmps(&dir).is_empty());
    assert_eq!(names(&dir.join("agentspace.d")), [HOST_ALIAS]);
}

#[test]
fn another_hosts_alias_file_is_never_replaced() {
    let (s, dir, file) = world("hw4");
    // A file at this host's path whose owner mark names another host.
    let theirs = host_bytes(OTHER_PROJECT, "c");
    fs::write(&file, &theirs).unwrap();
    let err = write_host_alias(&dir, PROJECT, &host_bytes(PROJECT, "c"))
        .expect_err("another host's file");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
    assert_eq!(fs::read(&file).unwrap(), theirs);
    assert!(tmps(&dir).is_empty(), "a temporary file was left behind");
}

#[test]
fn a_leases_alias_file_at_the_host_aliass_path_is_never_replaced() {
    let (s, dir, file) = world("hw5");
    let theirs = lease_bytes(&s, ID);
    fs::write(&file, &theirs).unwrap();
    let err =
        write_host_alias(&dir, PROJECT, &host_bytes(PROJECT, "c")).expect_err("a lease's file");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
    assert_eq!(fs::read(&file).unwrap(), theirs);
    assert!(tmps(&dir).is_empty());
}

#[test]
fn a_file_with_no_owner_mark_at_the_host_aliass_path_is_never_replaced() {
    for theirs in [
        &b"Host something\n  User me\n"[..],
        b"",
        b"# agentspace host examplehost no-semicolon\n",
        b"# agentspace host ; no project\n",
        b"\n# agentspace host examplehost; second line\n",
        b"Host mine\n# agentspace host examplehost; second line\n",
    ] {
        let (_s, dir, file) = world("hw6");
        fs::write(&file, theirs).unwrap();
        let err =
            write_host_alias(&dir, PROJECT, &host_bytes(PROJECT, "c")).expect_err("no owner mark");
        assert_eq!(err.kind, AliasFailureKind::Foreign, "{theirs:?}");
        assert_eq!(fs::read(&file).unwrap(), theirs);
        assert!(tmps(&dir).is_empty());
    }
}

#[test]
fn a_folder_at_the_host_aliass_path_refuses_the_write() {
    let (s, dir, file) = world("hw7");
    fs::create_dir(&file).unwrap();
    fs::write(file.join("keep.txt"), b"x").unwrap();
    let err = write_host_alias(&dir, PROJECT, &host_bytes(PROJECT, "c"))
        .expect_err("a folder at the path");
    assert_plain_message(&err.message, &s);
    assert!(file.is_dir());
    assert_eq!(fs::read(file.join("keep.txt")).unwrap(), b"x");
    assert!(tmps(&dir).is_empty(), "a temporary file was left behind");
    // An empty folder is refused too.
    let (_s2, dir2, file2) = world("hw7b");
    fs::create_dir(&file2).unwrap();
    assert!(write_host_alias(&dir2, PROJECT, &host_bytes(PROJECT, "c")).is_err());
    assert!(file2.is_dir());
}

#[test]
fn a_symbolic_link_at_the_host_aliass_path_is_never_followed_or_replaced() {
    let (s, dir, file) = world("hw8");
    let target = s.0.join("real-alias");
    let theirs = host_bytes(PROJECT, "a person's own copy");
    fs::write(&target, &theirs).unwrap();
    if let Err(e) = std::os::windows::fs::symlink_file(&target, &file) {
        // Creating a symbolic link needs Developer Mode or a privilege.
        eprintln!("SKIPPED: a symbolic link could not be made here: {e}");
        return;
    }
    // Even a link to a file that names this host is not replaced, and not written through.
    let err = write_host_alias(&dir, PROJECT, &host_bytes(PROJECT, "new comment"))
        .expect_err("a link at the path");
    assert_plain_message(&err.message, &s);
    assert!(
        fs::symlink_metadata(&file)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), theirs);
    assert!(tmps(&dir).is_empty());
    // Removing never removes the link either.
    assert!(remove_host_alias(&dir, PROJECT).is_err());
    assert!(
        fs::symlink_metadata(&file)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), theirs);
}

#[test]
fn two_hosts_alias_files_sit_side_by_side_and_each_changes_only_its_own() {
    let (_s, dir, file) = world("hw9");
    let other = dir.join("agentspace.d").join(OTHER_HOST_ALIAS);
    let a = host_bytes(PROJECT, "a");
    let b = host_bytes(OTHER_PROJECT, "b");
    write_host_alias(&dir, PROJECT, &a).unwrap();
    write_host_alias(&dir, OTHER_PROJECT, &b).unwrap();
    assert_eq!(fs::read(&file).unwrap(), a);
    assert_eq!(fs::read(&other).unwrap(), b);
    let a2 = host_bytes(PROJECT, "a2");
    write_host_alias(&dir, PROJECT, &a2).unwrap();
    assert_eq!(fs::read(&file).unwrap(), a2);
    assert_eq!(fs::read(&other).unwrap(), b);
    remove_host_alias(&dir, OTHER_PROJECT).unwrap();
    assert!(!other.exists());
    assert_eq!(fs::read(&file).unwrap(), a2);
    assert!(tmps(&dir).is_empty());
}

#[test]
fn a_host_alias_sits_beside_a_leases_alias_without_touching_it() {
    let (s, dir, file) = world("hw10");
    let lease_file = dir.join("agentspace.d").join(ALIAS);
    let lease = lease_bytes(&s, ID);
    write_alias(&dir, ALIAS, ID, &lease).unwrap();
    write_host_alias(&dir, PROJECT, &host_bytes(PROJECT, "c")).unwrap();
    assert_eq!(fs::read(&lease_file).unwrap(), lease);
    assert!(file.is_file());
    remove_host_alias(&dir, PROJECT).unwrap();
    assert_eq!(fs::read(&lease_file).unwrap(), lease);
    assert!(!file.exists());
}

// ---------- The owner marks never cross ----------

#[test]
fn a_leases_write_over_an_operator_alias_is_refused_and_leaves_it() {
    let (s, dir, file) = world("x1");
    let host = host_bytes(PROJECT, "c");
    fs::write(&file, &host).unwrap();
    // A lease's `create` that meets an operator alias treats it as a file with no owner mark.
    let err =
        write_alias(&dir, HOST_ALIAS, ID, &lease_bytes(&s, ID)).expect_err("an operator alias");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
    assert_eq!(fs::read(&file).unwrap(), host);
    assert!(tmps(&dir).is_empty());
}

#[test]
fn a_leases_remove_of_an_operator_alias_is_refused_and_leaves_it() {
    let (s, dir, file) = world("x2");
    let host = host_bytes(PROJECT, "c");
    fs::write(&file, &host).unwrap();
    let err = remove_alias(&dir, HOST_ALIAS, ID).expect_err("an operator alias");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
    assert_eq!(fs::read(&file).unwrap(), host);
}

#[test]
fn a_leases_owner_check_never_matches_an_operator_alias() {
    let (s, dir, file) = world("x3");
    let host = host_bytes(PROJECT, "c");
    fs::write(&file, &host).unwrap();
    for id in [ID, OTHER_ID, PROJECT, "host"] {
        let err = check_alias_owner(&dir, HOST_ALIAS, id).expect_err("an operator alias");
        assert_eq!(err.kind, AliasFailureKind::Foreign, "{id}");
        assert_plain_message(&err.message, &s);
    }
    assert_eq!(fs::read(&file).unwrap(), host);
}

#[test]
fn a_hosts_write_over_a_leases_file_is_refused_and_leaves_it() {
    let (s, dir, file) = world("x5");
    let lease = lease_bytes(&s, ID);
    fs::write(&file, &lease).unwrap();
    let err =
        write_host_alias(&dir, PROJECT, &host_bytes(PROJECT, "c")).expect_err("a lease's file");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_eq!(fs::read(&file).unwrap(), lease);
}

#[test]
fn a_hosts_remove_of_a_leases_file_is_refused_and_leaves_it() {
    let (s, dir, file) = world("x6");
    let lease = lease_bytes(&s, ID);
    fs::write(&file, &lease).unwrap();
    let err = remove_host_alias(&dir, PROJECT).expect_err("a lease's file");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
    assert_eq!(fs::read(&file).unwrap(), lease);
}

#[test]
fn a_hosts_remove_of_another_hosts_file_is_refused_and_leaves_it() {
    let (s, dir, file) = world("x7");
    let theirs = host_bytes(OTHER_PROJECT, "c");
    fs::write(&file, &theirs).unwrap();
    let err = remove_host_alias(&dir, PROJECT).expect_err("another host's file");
    assert_eq!(err.kind, AliasFailureKind::Foreign);
    assert_plain_message(&err.message, &s);
    assert_eq!(fs::read(&file).unwrap(), theirs);
}

// ---------- remove_host_alias ----------

#[test]
fn remove_host_alias_deletes_its_own_file_and_keeps_agentspace_d() {
    let (_s, dir, file) = world("hr1");
    write_host_alias(&dir, PROJECT, &host_bytes(PROJECT, "c")).unwrap();
    remove_host_alias(&dir, PROJECT).unwrap();
    assert!(!file.exists());
    // agentspace.d stays: agentspace never removes it.
    assert!(dir.join("agentspace.d").is_dir());
    assert!(tmps(&dir).is_empty());
}

#[test]
fn remove_host_alias_of_a_missing_file_is_not_a_failure() {
    let (_s, dir, file) = world("hr2");
    assert!(!file.exists());
    remove_host_alias(&dir, PROJECT).unwrap();
    // Also when agentspace.d itself is missing.
    let s2 = scratch("hr2b");
    let dir2 = ssh_dir(&s2);
    remove_host_alias(&dir2, PROJECT).unwrap();
}

#[test]
fn remove_host_alias_never_deletes_a_file_with_no_owner_mark() {
    for theirs in [
        &b"Host mine\n  User me\n"[..],
        b"",
        b"# agentspace host examplehost no-semicolon\n",
    ] {
        let (_s, dir, file) = world("hr3");
        fs::write(&file, theirs).unwrap();
        let err = remove_host_alias(&dir, PROJECT).expect_err("no owner mark");
        assert_eq!(err.kind, AliasFailureKind::Foreign, "{theirs:?}");
        assert_eq!(fs::read(&file).unwrap(), theirs);
    }
}

#[test]
fn remove_host_alias_refuses_a_folder_at_the_path_and_deletes_nothing() {
    let (s, dir, file) = world("hr4");
    fs::create_dir(&file).unwrap();
    fs::write(file.join("inside.txt"), b"x").unwrap();
    let err = remove_host_alias(&dir, PROJECT).expect_err("a folder at the path");
    assert_plain_message(&err.message, &s);
    assert!(file.is_dir());
    assert_eq!(fs::read(file.join("inside.txt")).unwrap(), b"x");
    // An empty folder is refused too.
    let (_s2, dir2, file2) = world("hr4b");
    fs::create_dir(&file2).unwrap();
    assert!(remove_host_alias(&dir2, PROJECT).is_err());
    assert!(file2.is_dir());
}

// ---------- A file where a folder has to be ----------

fn every_host_call(dir: &Path) -> Vec<(&'static str, Result<(), AliasFailure>)> {
    vec![
        (
            "write_host_alias",
            write_host_alias(dir, PROJECT, &host_bytes(PROJECT, "c")),
        ),
        ("remove_host_alias", remove_host_alias(dir, PROJECT)),
    ]
}

#[test]
fn a_file_where_the_ssh_folder_must_be_is_refused_and_nothing_changes() {
    let s = scratch("hf1");
    let home = s.0.join("home");
    fs::create_dir_all(&home).unwrap();
    let dir = home.join(".ssh");
    fs::write(&dir, b"not a folder").unwrap();
    for (call, result) in every_host_call(&dir) {
        let err = result.expect_err(call);
        assert_eq!(err.kind, AliasFailureKind::Io, "{call}");
        assert_plain_message(&err.message, &s);
    }
    assert_eq!(fs::read(&dir).unwrap(), b"not a folder");
    assert_eq!(names(&home), [".ssh"]);
    assert_eq!(names(&s.0), ["home"]);
}

#[test]
fn a_file_where_agentspace_d_must_be_is_refused_and_nothing_changes() {
    let s = scratch("hf2");
    let dir = ssh_dir(&s);
    fs::write(dir.join("config"), b"Host a\n").unwrap();
    let d = dir.join("agentspace.d");
    fs::write(&d, b"not a folder").unwrap();
    for (call, result) in every_host_call(&dir) {
        let err = result.expect_err(call);
        assert_eq!(err.kind, AliasFailureKind::Io, "{call}");
        assert_plain_message(&err.message, &s);
    }
    assert_eq!(fs::read(&d).unwrap(), b"not a folder");
    assert_eq!(fs::read(dir.join("config")).unwrap(), b"Host a\n");
    // No backup, no temporary file and no new entry of any kind.
    assert_eq!(names(&dir), ["agentspace.d", "config"]);
}

// ---------- The temporary file's place ----------

/// Denies the current user the right to add a file to `dir` itself (not inherited by what is
/// created under it), and lifts the denial again when dropped, so that the folder can be deleted.
struct NoNewFiles(PathBuf);

impl NoNewFiles {
    fn new(dir: &Path) -> Self {
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
fn the_host_aliass_temporary_file_is_made_in_the_ssh_folder_not_in_agentspace_d() {
    let (s, dir, file) = world("ht1");
    let bytes = host_bytes(PROJECT, "c");
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
        let err =
            write_host_alias(&dir, PROJECT, &bytes).expect_err("no new file in the ssh folder");
        assert_eq!(err.kind, AliasFailureKind::Io);
        assert_plain_message(&err.message, &s);
        assert!(!file.exists());
        assert!(names(&dir.join("agentspace.d")).is_empty());
    }
    // With the denial lifted the same call works, so the denial was what stopped it.
    write_host_alias(&dir, PROJECT, &bytes).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(tmps(&dir).is_empty());
}

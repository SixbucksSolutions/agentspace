//! `ensure_include` on Windows (docs/state.md, "The `Include` line" and "Replacing a file").
//! Every folder is a throwaway under the temp folder.
#![cfg(windows)]

mod common;

use agentspace_alias::*;
use common::*;
use std::fs;

const LF_LINE: &[u8] = b"Include agentspace.d/*\n";

fn config_with(bytes: &[u8]) -> (Scratch, std::path::PathBuf, std::path::PathBuf) {
    let s = scratch("inc");
    let dir = ssh_dir(&s);
    let cfg = dir.join("config");
    fs::write(&cfg, bytes).unwrap();
    (s, dir, cfg)
}

#[test]
fn no_config_is_created_with_the_line_and_one_lf_and_no_backup() {
    let s = scratch("noconf");
    // The ssh folder itself is missing, and is created.
    let dir = s.0.join("home").join(".ssh");
    ensure_include(&dir, NOW).unwrap();
    assert_eq!(fs::read(dir.join("config")).unwrap(), LF_LINE);
    assert!(dir.join("agentspace.d").is_dir());
    assert_eq!(names(&dir), ["agentspace.d", "config"]);
}

#[test]
fn line_present_leaves_the_bytes_alone_and_creates_agentspace_d() {
    for original in [
        &b"Include agentspace.d/*\r\nHost a\r\n  User x\r\n"[..],
        b"\xEF\xBB\xBFinclude \t AGENTSPACE.D/*  \nHost a\n",
        b"# note\nInclude agentspace.d/*",
    ] {
        let (_s, dir, cfg) = config_with(original);
        ensure_include(&dir, NOW).unwrap();
        assert_eq!(fs::read(&cfg).unwrap(), original);
        assert!(dir.join("agentspace.d").is_dir());
        assert!(backups(&dir).is_empty());
        assert!(tmps(&dir).is_empty());
    }
}

#[test]
fn a_second_call_changes_nothing_and_makes_no_second_backup() {
    let (_s, dir, cfg) = config_with(b"Host a\n  User x\n");
    ensure_include(&dir, NOW).unwrap();
    let after_first = fs::read(&cfg).unwrap();
    ensure_include(&dir, NOW + 5).unwrap();
    assert_eq!(fs::read(&cfg).unwrap(), after_first);
    assert_eq!(backups(&dir).len(), 1);
}

#[test]
fn insert_puts_the_line_first_and_keeps_every_other_byte() {
    let cases: [(&[u8], Vec<u8>); 7] = [
        // CR LF anywhere makes the new line end in CR LF; no final line ending is kept as none.
        (
            b"Host a\r\n  User x",
            b"Include agentspace.d/*\r\nHost a\r\n  User x".to_vec(),
        ),
        // Only LFs: LF.
        (
            b"Host a\n  User x\n",
            b"Include agentspace.d/*\nHost a\n  User x\n".to_vec(),
        ),
        // A CR LF later in the file, even with LF first.
        (
            b"# c\nHost a\r\n",
            b"Include agentspace.d/*\r\n# c\nHost a\r\n".to_vec(),
        ),
        // BOM kept in front.
        (
            b"\xEF\xBB\xBFHost a\n",
            b"\xEF\xBB\xBFInclude agentspace.d/*\nHost a\n".to_vec(),
        ),
        // Empty config: the line and one LF.
        (b"", b"Include agentspace.d/*\n".to_vec()),
        // Only a BOM.
        (
            b"\xEF\xBB\xBF",
            b"\xEF\xBB\xBFInclude agentspace.d/*\n".to_vec(),
        ),
        // A line that is not equal gets a second, exact line above it.
        (
            b"Include agentspace.d/* # aliases\n",
            b"Include agentspace.d/*\nInclude agentspace.d/* # aliases\n".to_vec(),
        ),
    ];
    for (original, expected) in cases {
        let (_s, dir, cfg) = config_with(original);
        ensure_include(&dir, NOW).unwrap();
        assert_eq!(
            fs::read(&cfg).unwrap(),
            expected,
            "original {:?}",
            String::from_utf8_lossy(original)
        );
        assert!(dir.join("agentspace.d").is_dir());
        assert!(tmps(&dir).is_empty(), "a temporary file was left behind");
    }
}

#[test]
fn the_backup_has_the_utc_name_and_the_originals_sha256() {
    let original: &[u8] = b"Host a\r\n  User x\r\n\xFF\xFE binary tail";
    let (s, dir, cfg) = config_with(original);
    let kept = s.0.join("original-copy");
    fs::write(&kept, original).unwrap();
    let before = sha256(&cfg);
    ensure_include(&dir, NOW).unwrap();
    assert_eq!(
        backups(&dir),
        [format!("config.agentspace-backup-{NOW_STAMP}")]
    );
    let backup = dir.join(format!("config.agentspace-backup-{NOW_STAMP}"));
    assert_eq!(fs::read(&backup).unwrap(), original);
    assert_eq!(sha256(&backup), before);
    assert_eq!(sha256(&backup), sha256(&kept));
    assert_ne!(sha256(&cfg), before, "the config should have changed");
}

#[test]
fn a_backup_is_never_overwritten() {
    let original: &[u8] = b"Host a\n";
    let (_s, dir, cfg) = config_with(original);
    let existing = dir.join(format!("config.agentspace-backup-{NOW_STAMP}"));
    fs::write(&existing, b"someone else's file").unwrap();
    let r = ensure_include(&dir, NOW);
    assert!(r.is_err(), "the insert went ahead over an existing backup");
    assert_eq!(fs::read(&existing).unwrap(), b"someone else's file");
    assert_eq!(fs::read(&cfg).unwrap(), original, "the config was changed");
    assert!(tmps(&dir).is_empty());
}

#[test]
fn an_insert_keeps_the_configs_own_access_list() {
    // The replace keeps the config's access list; its owner becomes the user running it, which
    // OpenSSH accepts, so the owner is not compared (architect, 2026-11-19; docs/state.md,
    // Replacing a file).
    let (_s, dir, cfg) = config_with(b"Host a\n  User x\n");
    let user = std::env::var("USERNAME").unwrap();
    // A protected list of its own: the user with full control, plus Everyone (S-1-1-0) read.
    let c = q(&cfg);
    ps(&format!(
        "icacls {c} /inheritance:r | Out-Null; \
         icacls {c} /grant:r '{user}:(F)' | Out-Null; \
         icacls {c} /grant '*S-1-1-0:(R)' | Out-Null"
    ));
    let dacl = |s: String| s[s.find("D:").unwrap_or(0)..].to_string();
    let before = dacl(sddl(&cfg));
    assert!(
        before.starts_with("D:P"),
        "the list should be protected: {before}"
    );
    assert!(
        before.contains(";;;WD)"),
        "Everyone entry missing: {before}"
    );
    ensure_include(&dir, NOW).unwrap();
    assert_eq!(
        fs::read(&cfg).unwrap(),
        [LF_LINE, b"Host a\n  User x\n"].concat()
    );
    assert_eq!(
        dacl(sddl(&cfg)),
        before,
        "access list changed by the replace"
    );
}

#[test]
fn a_symbolic_link_config_is_refused_with_no_backup_and_is_not_written_through() {
    let s = scratch("link");
    let dir = ssh_dir(&s);
    let target = s.0.join("real-config");
    let original: &[u8] = b"Host a\n  User x\n";
    fs::write(&target, original).unwrap();
    let link = dir.join("config");
    if let Err(e) = std::os::windows::fs::symlink_file(&target, &link) {
        // Creating a symbolic link needs Developer Mode or a privilege.
        eprintln!("SKIPPED: a symbolic link could not be made here: {e}");
        return;
    }
    let err = ensure_include(&dir, NOW).expect_err("a link config must be refused");
    assert_eq!(err.kind, AliasFailureKind::Config);
    assert_plain_message(&err.message, &s);
    assert!(backups(&dir).is_empty(), "a backup was made");
    assert!(tmps(&dir).is_empty());
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), original);
}

#[test]
fn a_symbolic_link_config_that_already_holds_the_line_is_only_read() {
    let s = scratch("linkok");
    let dir = ssh_dir(&s);
    let target = s.0.join("real-config");
    let original: &[u8] = b"Include agentspace.d/*\nHost a\n";
    fs::write(&target, original).unwrap();
    let link = dir.join("config");
    if let Err(e) = std::os::windows::fs::symlink_file(&target, &link) {
        eprintln!("SKIPPED: a symbolic link could not be made here: {e}");
        return;
    }
    ensure_include(&dir, NOW).unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), original);
    assert!(backups(&dir).is_empty());
    assert!(dir.join("agentspace.d").is_dir());
}

#[test]
fn a_read_only_config_is_refused_with_no_backup_and_keeps_its_attribute() {
    let original: &[u8] = b"Host a\n  User x\n";
    let (s, dir, cfg) = config_with(original);
    let mut perm = fs::metadata(&cfg).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&cfg, perm).unwrap();
    let err = ensure_include(&dir, NOW).expect_err("a read-only config must be refused");
    assert_eq!(err.kind, AliasFailureKind::Config);
    assert_plain_message(&err.message, &s);
    assert!(backups(&dir).is_empty(), "a backup was made");
    assert!(tmps(&dir).is_empty());
    assert!(fs::metadata(&cfg).unwrap().permissions().readonly());
    assert_eq!(fs::read(&cfg).unwrap(), original);
}

#[test]
fn a_read_only_config_that_already_holds_the_line_is_fine() {
    let original: &[u8] = b"Include agentspace.d/*\nHost a\n";
    let (_s, dir, cfg) = config_with(original);
    let mut perm = fs::metadata(&cfg).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&cfg, perm).unwrap();
    ensure_include(&dir, NOW).unwrap();
    assert_eq!(fs::read(&cfg).unwrap(), original);
    assert!(backups(&dir).is_empty());
}

#[test]
fn an_include_found_only_below_a_block_is_refused_with_config_kind() {
    for original in [
        &b"Host a\n  User x\nInclude agentspace.d/*\n"[..],
        b"# c\r\nmatch all\r\n  Include  agentspace.d/*\r\n",
        b"Host=a\nInclude AGENTSPACE.D/*",
    ] {
        let (s, dir, cfg) = config_with(original);
        let err = ensure_include(&dir, NOW).expect_err("the line below a block must be refused");
        assert_eq!(err.kind, AliasFailureKind::Config);
        assert_plain_message(&err.message, &s);
        assert_eq!(fs::read(&cfg).unwrap(), original);
        assert!(backups(&dir).is_empty());
        assert!(tmps(&dir).is_empty());
    }
}

#[test]
fn a_commented_host_word_is_not_a_block() {
    // The line below a commented `Host` is still above the first block, so it is found.
    let original: &[u8] = b"# Host a\nInclude agentspace.d/*\nHost b\n";
    let (_s, dir, cfg) = config_with(original);
    ensure_include(&dir, NOW).unwrap();
    assert_eq!(fs::read(&cfg).unwrap(), original);
    assert!(backups(&dir).is_empty());
}

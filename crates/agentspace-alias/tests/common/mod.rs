//! Helpers shared by the Windows disk tests (E0.05-T02-S03). Every folder is a throwaway under the
//! temp folder, and no test reads the real `~/.ssh` or `.agentspace`.
#![cfg(windows)]
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

pub const ID: &str = "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";
pub const OTHER_ID: &str = "99999999-5b7d-4a60-8c11-0d2e4f6a8b90";
pub const ALIAS: &str = "agentspace-swimcoach-3f2a9c1e";
pub const LINE: &str = "Include agentspace.d/*";

/// 2023-11-14 22:13:20 UTC.
pub const NOW: i64 = 1_700_000_000;
pub const NOW_STAMP: &str = "20231114T221320Z";

static COUNTER: AtomicU32 = AtomicU32::new(0);

pub struct Scratch(pub PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        clear_readonly(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn clear_readonly(p: &Path) {
    if let Ok(md) = fs::symlink_metadata(p) {
        if md.file_type().is_symlink() {
            return;
        }
        if md.permissions().readonly() && md.is_file() {
            let mut perm = md.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = fs::set_permissions(p, perm);
        }
        if md.is_dir()
            && let Ok(rd) = fs::read_dir(p)
        {
            for e in rd.flatten() {
                clear_readonly(&e.path());
            }
        }
    }
}

/// A fresh scratch folder whose name holds a space and a `%`.
pub fn scratch(tag: &str) -> Scratch {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!(
        "agentspace alias %t {}-{tag}-{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

/// A scratch folder and an `.ssh` folder in it (created).
pub fn ssh_dir(s: &Scratch) -> PathBuf {
    let d = s.0.join("home").join(".ssh");
    fs::create_dir_all(&d).unwrap();
    d
}

pub fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

pub fn backups(dir: &Path) -> Vec<String> {
    names(dir)
        .into_iter()
        .filter(|n| n.starts_with("config.agentspace-backup-"))
        .collect()
}

pub fn tmps(dir: &Path) -> Vec<String> {
    names(dir)
        .into_iter()
        .filter(|n| n.contains(".agentspace-tmp-"))
        .collect()
}

/// Runs a Windows PowerShell 5.1 command and returns its trimmed stdout. It is started by its full
/// path, and without the `PSModulePath` that a PowerShell 7 parent hands down, which would make
/// 5.1 fail to load its own modules.
pub fn ps(script: &str) -> String {
    let root = std::env::var("SystemRoot").expect("SystemRoot must be set");
    let out = Command::new(format!(
        "{root}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"
    ))
    .env_remove("PSModulePath")
    .args(["-NoProfile", "-NonInteractive", "-Command", script])
    .output()
    .expect("powershell.exe must run");
    assert!(
        out.status.success(),
        "powershell failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A path as a PowerShell single-quoted literal.
pub fn q(p: &Path) -> String {
    format!("'{}'", p.display().to_string().replace('\'', "''"))
}

pub fn sha256(p: &Path) -> String {
    ps(&format!(
        "(Get-FileHash -LiteralPath {} -Algorithm SHA256).Hash",
        q(p)
    ))
}

pub fn sddl(p: &Path) -> String {
    ps(&format!("(Get-Acl -LiteralPath {}).Sddl", q(p)))
}

/// A message is agentspace's own words: no absolute path, no user name.
pub fn assert_plain_message(msg: &str, scratch: &Scratch) {
    assert!(!msg.is_empty(), "the message is empty");
    let root = scratch.0.display().to_string();
    assert!(!msg.contains(&root), "message holds a path: {msg}");
    assert!(
        !msg.contains(":\\") && !msg.contains(":/"),
        "message holds a path: {msg}"
    );
    if let Ok(user) = std::env::var("USERNAME")
        && user.len() >= 4
    {
        assert!(
            !msg.to_lowercase().contains(&user.to_lowercase()),
            "message holds the user name: {msg}"
        );
    }
}

/// Makes `dir` OpenSSH-safe for everything created under it afterwards: its own rules only (no
/// inheritance from the temp folder, which on some machines holds an entry for an unknown
/// account that OpenSSH refuses), the user, SYSTEM and Administrators with full control.
pub fn make_openssh_safe(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let user = std::env::var("USERNAME").unwrap();
    let d = q(dir);
    ps(&format!(
        "icacls {d} /inheritance:r | Out-Null; \
         icacls {d} /grant:r '{user}:(OI)(CI)F' '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null"
    ));
}

/// The alias file's bytes for `id`, in state.md's form, built through T01's public API.
pub fn alias_bytes(home: &Path, id: &str, alias: &str, proxy: &str) -> Vec<u8> {
    use agentspace_alias::*;
    let home_s = home.display().to_string();
    let key = lease_file_text(&home_s, id, "id_ed25519");
    let cert = lease_file_text(&home_s, id, "id_ed25519-cert.pub");
    alias_file_bytes(&AliasFile {
        id,
        comment: "Written by agentspace. Do not edit.",
        alias,
        user: "ubuntu",
        key_path: &key,
        cert_path: &cert,
        host_key_alias: "agentspace",
        known_hosts: None,
        proxy_command: proxy,
    })
    .unwrap()
}

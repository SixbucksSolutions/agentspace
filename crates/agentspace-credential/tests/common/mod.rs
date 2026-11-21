//! Shared helpers for the Windows disk tests (E0.05-T02-S02, #0100).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every file lives in a throwaway home under
//! the temp folder, and a test MUST NOT touch the real profile. The CA a test uses is made by the
//! operator's `ca`, run in process through `agentspace_ca::ca` with a context that names only the
//! throwaway home (architect, 2026-11-20, for E0.06-T06-S01).
#![cfg(windows)]
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const ID: &str = "0f9c2a1e-1234-4abc-8def-0123456789ab";
pub const FULL: u32 = 0x1F01FF;
pub const MODIFY: u32 = 0x1301BF;
pub const SID_SYSTEM: &str = "S-1-5-18";
pub const SID_ADMINS: &str = "S-1-5-32-544";
pub const SID_EVERYONE: &str = "S-1-1-0";

/// A throwaway home that holds a CA. Dropping it deletes the folder.
pub struct Home {
    _tmp: tempfile::TempDir,
    pub home: PathBuf,
    /// Whether the operator's `ca` made the CA (otherwise the fallback of state.md did).
    pub by_operator: bool,
}

impl Home {
    pub fn ca_dir(&self) -> PathBuf {
        self.home.join(".agentspace").join("ca")
    }
    pub fn ca_key(&self) -> PathBuf {
        self.ca_dir().join("agentspace_ca")
    }
    pub fn ca_pub(&self) -> PathBuf {
        self.ca_dir().join("agentspace_ca.pub")
    }
    pub fn leases_dir(&self) -> PathBuf {
        self.home.join(".agentspace").join("leases")
    }
    pub fn lease_dir(&self, id: &str) -> PathBuf {
        self.leases_dir().join(id)
    }
    /// `%USERPROFILE%\.agentspace\hosts\<project>`: a host's folder (state.md, A host's folder).
    pub fn host_dir(&self, project: &str) -> PathBuf {
        self.home.join(".agentspace").join("hosts").join(project)
    }
}

fn run_out(cmd: &mut Command) -> Output {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("A tool a test needs could not be started")
}

/// Windows PowerShell 5.1 by its full path, with the parent's `PSModulePath` removed. A parent
/// under PowerShell 7 hands its module path down, and 5.1 then fails to load its own modules.
fn windows_powershell() -> Command {
    let root = std::env::var_os("SystemRoot").expect("SystemRoot MUST be set");
    let exe = Path::new(&root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let mut cmd = Command::new(exe);
    cmd.env_remove("PSModulePath");
    cmd
}

/// A home with no CA.
pub fn empty_home() -> Home {
    let tmp = tempfile::Builder::new()
        .prefix("agentspace-credtest-")
        .tempdir()
        .expect("A throwaway folder could not be made");
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join(".agentspace")).unwrap();
    fs::create_dir_all(home.join(".ssh")).unwrap();
    fs::create_dir_all(home.join("AppData").join("Local")).unwrap();
    fs::create_dir_all(home.join("AppData").join("Roaming")).unwrap();
    Home {
        _tmp: tmp,
        home,
        by_operator: false,
    }
}

/// A `Say` that prints nothing, for the in-process `ca`.
struct NullSay;

impl agentspace_operator_api::Say for NullSay {
    fn mode(&self) -> agentspace_operator_api::Mode {
        agentspace_operator_api::Mode::Person
    }
    fn progress(&self, _: &str, _: Option<agentspace_operator_api::CallerLine>) {}
    fn warning(&self, _: &str) {}
    fn detail(&self, _: &str) {}
}

/// A home whose CA the operator's `ca` made, falling back to state.md's own steps if it fails.
pub fn home_with_ca() -> Home {
    let mut h = empty_home();
    assert!(
        h.home.starts_with(std::env::temp_dir()),
        "The home is not under the temp folder"
    );
    // The operator's own `ca`, run in process as agentspace-sync's tests do (architect,
    // 2026-11-20, for E0.06-T06-S01). The context names the throwaway home and nothing of the real
    // profile: no `LOCALAPPDATA`, and a made-up user and computer name.
    let ctx = agentspace_operator_api::Context {
        home: h.home.clone(),
        local_app_data: None,
        system_root: PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot MUST be set")),
        nested: false,
    };
    let env = agentspace_ca::CaEnv {
        user_name: Some("someone".to_string()),
        computer_name: Some("EXAMPLEPC".to_string()),
        ..agentspace_ca::CaEnv::default()
    };
    let ran = agentspace_ca::ca(&ctx, &env, &NullSay);
    let made = ran.is_ok() && h.ca_key().is_file() && h.ca_pub().is_file();
    if made {
        h.by_operator = true;
        return h;
    }
    // Fallback: state.md's The CA, step by step.
    let _ = fs::remove_dir_all(h.ca_dir());
    fs::create_dir_all(h.ca_dir()).unwrap();
    let sid = user_sid();
    icacls(
        &h.ca_dir(),
        &["/inheritance:r", "/grant:r", &format!("*{sid}:(OI)(CI)F")],
    );
    let out = run_out(Command::new("ssh-keygen").args([
        "-q",
        "-t",
        "ed25519",
        "-N",
        "",
        "-C",
        "agentspace-ca test",
        "-f",
        h.ca_key().to_str().unwrap(),
    ]));
    assert!(
        out.status.success(),
        "ssh-keygen could not make the fallback CA"
    );
    icacls(&h.ca_key(), &["/reset"]);
    icacls(&h.ca_key(), &["/inheritance:d"]);
    h
}

pub fn icacls(path: &Path, args: &[&str]) -> String {
    let out = run_out(Command::new("icacls").arg(path).args(args));
    assert!(
        out.status.success(),
        "icacls failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The user SID of the current process, read at run time.
pub fn user_sid() -> String {
    let out = run_out(Command::new("whoami").args(["/user", "/fo", "csv", "/nh"]));
    assert!(out.status.success(), "whoami could not give the user SID");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let sid = text
        .trim()
        .rsplit(',')
        .next()
        .unwrap()
        .trim()
        .trim_matches('"')
        .to_string();
    assert!(sid.starts_with("S-1-"), "Unexpected SID shape");
    sid
}

pub fn username() -> String {
    std::env::var("USERNAME").expect("USERNAME MUST be set")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ace {
    pub sid: String,
    pub allow: bool,
    pub rights: u32,
    pub inherited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acl {
    pub owner: String,
    pub protected: bool,
    pub aces: Vec<Ace>,
}

/// Reads an owner, the protected flag and every entry by SID, with Windows PowerShell.
pub fn acl(path: &Path) -> Acl {
    let script = "$a = Get-Acl -LiteralPath $env:ACLPATH; \
        'OWNER ' + $a.GetOwner([System.Security.Principal.SecurityIdentifier]).Value; \
        'PROT ' + $a.AreAccessRulesProtected; \
        foreach ($r in $a.GetAccessRules($true,$true,[System.Security.Principal.SecurityIdentifier])) { \
        'ACE ' + $r.IdentityReference.Value + ' ' + $r.AccessControlType + ' ' + [int]$r.FileSystemRights + ' ' + $r.IsInherited }";
    let out = run_out(
        windows_powershell()
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("ACLPATH", path),
    );
    assert!(
        out.status.success(),
        "Reading the access rules failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut owner = String::new();
    let mut protected = false;
    let mut aces = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("OWNER ") {
            owner = v.to_string();
        } else if let Some(v) = line.strip_prefix("PROT ") {
            protected = v == "True";
        } else if let Some(v) = line.strip_prefix("ACE ") {
            let f: Vec<&str> = v.split(' ').collect();
            assert_eq!(f.len(), 4, "Unexpected ACE line: {line}");
            aces.push(Ace {
                sid: f[0].to_string(),
                allow: f[1] == "Allow",
                rights: (f[2].parse::<i64>().unwrap() as i32) as u32,
                inherited: f[3] == "True",
            });
        }
    }
    assert!(!owner.is_empty(), "No owner was read for the path");
    Acl {
        owner,
        protected,
        aces,
    }
}

pub fn sha256(path: &Path) -> String {
    let out = run_out(
        windows_powershell()
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-FileHash -Algorithm SHA256 -LiteralPath $env:HASHPATH).Hash",
            ])
            .env("HASHPATH", path),
    );
    assert!(out.status.success(), "Hashing failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Runs Windows OpenSSH's ssh-keygen with stdin closed, so a prompt fails rather than waits.
pub fn ssh_keygen(args: &[&str]) -> (bool, String, String) {
    let out = run_out(Command::new("ssh-keygen").args(args).env("TZ", "UTC"));
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Names of the entries in a folder, sorted.
pub fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Snapshot of the CA: hashes of both files and the access rules of the folder and the key.
pub fn ca_snapshot(h: &Home) -> (String, String, Acl, Acl) {
    (
        sha256(&h.ca_key()),
        sha256(&h.ca_pub()),
        acl(&h.ca_dir()),
        acl(&h.ca_key()),
    )
}

pub struct CertInfo {
    pub cert_type: String,
    pub public_fp: String,
    pub ca_fp: String,
    pub key_id: String,
    pub serial: String,
    pub valid_from: i64,
    pub valid_to: i64,
    pub principals: Vec<String>,
    pub critical: Vec<String>,
    pub extensions: Vec<String>,
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parses `2026-11-19T10:00:00` (UTC, since TZ=UTC is set for the child) into epoch seconds.
fn parse_time(s: &str) -> i64 {
    let (date, time) = s.split_once('T').expect("A time in ssh-keygen -L has a T");
    let d: Vec<i64> = date.split('-').map(|x| x.parse().unwrap()).collect();
    let t: Vec<i64> = time.split(':').map(|x| x.parse().unwrap()).collect();
    days_from_civil(d[0], d[1], d[2]) * 86400 + t[0] * 3600 + t[1] * 60 + t[2]
}

fn fp_of(v: &str) -> String {
    v.split_whitespace()
        .find(|w| w.starts_with("SHA256:"))
        .unwrap_or("")
        .to_string()
}

pub fn fingerprint(pubfile: &Path) -> String {
    let (ok, out, err) = ssh_keygen(&["-l", "-f", pubfile.to_str().unwrap()]);
    assert!(ok, "ssh-keygen -l failed: {err}");
    fp_of(&out)
}

/// Reads a certificate with `ssh-keygen -L`, in UTC.
pub fn cert_info(cert: &Path) -> CertInfo {
    let (ok, out, err) = ssh_keygen(&["-L", "-f", cert.to_str().unwrap()]);
    assert!(ok, "ssh-keygen -L failed: {err}");
    parse_cert_listing(&out)
}

/// Runs `ssh-keygen` with every profile variable pointed into the throwaway home (#0137), through
/// `Command::env`, never `set_var`. Stdin is closed, so a prompt fails rather than waits.
pub fn ssh_keygen_in(h: &Home, args: &[&str]) -> (bool, String, String) {
    let out = run_out(
        Command::new("ssh-keygen")
            .args(args)
            .env("USERPROFILE", &h.home)
            .env("HOME", &h.home)
            .env("LOCALAPPDATA", h.home.join("AppData").join("Local"))
            .env("APPDATA", h.home.join("AppData").join("Roaming"))
            .env("TZ", "UTC"),
    );
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Reads a certificate with `ssh-keygen -L`, run in the throwaway home, in UTC.
pub fn cert_info_in(h: &Home, cert: &Path) -> CertInfo {
    let (ok, out, err) = ssh_keygen_in(h, &["-L", "-f", cert.to_str().unwrap()]);
    assert!(ok, "ssh-keygen -L failed: {err}");
    parse_cert_listing(&out)
}

/// The fingerprint of a public key or certificate file, read with `ssh-keygen -l` in the home.
pub fn fingerprint_in(h: &Home, file: &Path) -> String {
    let (ok, out, err) = ssh_keygen_in(h, &["-l", "-f", file.to_str().unwrap()]);
    assert!(ok, "ssh-keygen -l failed: {err}");
    fp_of(&out)
}

fn parse_cert_listing(out: &str) -> CertInfo {
    let mut info = CertInfo {
        cert_type: String::new(),
        public_fp: String::new(),
        ca_fp: String::new(),
        key_id: String::new(),
        serial: String::new(),
        valid_from: 0,
        valid_to: 0,
        principals: vec![],
        critical: vec![],
        extensions: vec![],
    };
    let mut section = 0; // 1 principals, 2 critical, 3 extensions
    for raw in out.lines() {
        let line = raw.trim();
        if let Some(v) = line.strip_prefix("Type:") {
            info.cert_type = v.trim().to_string();
            section = 0;
        } else if let Some(v) = line.strip_prefix("Public key:") {
            info.public_fp = fp_of(v);
            section = 0;
        } else if let Some(v) = line.strip_prefix("Signing CA:") {
            info.ca_fp = fp_of(v);
            section = 0;
        } else if let Some(v) = line.strip_prefix("Key ID:") {
            info.key_id = v.trim().trim_matches('"').to_string();
            section = 0;
        } else if let Some(v) = line.strip_prefix("Serial:") {
            info.serial = v.trim().to_string();
            section = 0;
        } else if let Some(v) = line.strip_prefix("Valid:") {
            let w: Vec<&str> = v.split_whitespace().collect();
            assert_eq!(w.len(), 4, "Unexpected Valid line: {line}");
            info.valid_from = parse_time(w[1]);
            info.valid_to = parse_time(w[3]);
            section = 0;
        } else if line.starts_with("Principals:") {
            section = 1;
        } else if let Some(v) = line.strip_prefix("Critical Options:") {
            section = 2;
            let v = v.trim();
            if !v.is_empty() && v != "(none)" {
                info.critical.push(v.to_string());
            }
        } else if line.starts_with("Extensions:") {
            section = 3;
        } else if !line.is_empty() {
            match section {
                1 => info.principals.push(line.to_string()),
                2 => info.critical.push(line.to_string()),
                3 => info.extensions.push(line.to_string()),
                _ => {}
            }
        }
    }
    info
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

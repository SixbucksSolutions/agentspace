//! Shared helpers for the EC2 driver's integration tests (E0.05-T03-S01, #0105).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every file lives in a throwaway folder
//! under the temp folder, and a test MUST NOT touch the real profile. The CA a test uses is made by
//! the operator's `ca`, run in process through `agentspace_ca::ca` with a context that names only
//! the throwaway home (architect, 2026-11-20, for E0.06-T06-S01).
#![cfg(windows)]
#![allow(dead_code)]

use agentspace_driver_api::{Context, Driver, Failure, LeaseRef, PlaceRequest, Placed};
use agentspace_driver_ec2::Ec2Driver;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

pub const ID: &str = "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";
pub const OTHER_ID: &str = "99999999-5b7d-4a60-8c11-0d2e4f6a8b90";
pub const PROJECT: &str = "swimcoach";
pub const ALIAS: &str = "agentspace-swimcoach-3f2a9c1e";
pub const OTHER_ALIAS: &str = "agentspace-swimcoach-99999999";

pub const NO_CA: &str = "agentspace's certificate authority has not been set up, so no certificate can be issued. Nothing was changed; the operator sets it up.";
pub const CA_UNUSABLE: &str = "agentspace's certificate authority is not usable. Nothing was changed; the operator's ca command says why.";
pub const KEY_MISSING: &str =
    "The lease's key could not be found, so no certificate can be issued. Nothing was changed.";
pub const ALIAS_FOREIGN: &str = "The lease's host alias file belongs to another lease or to no lease, so nothing was deleted. A person has to move that file.";

pub type Snapshot = BTreeMap<String, Option<Vec<u8>>>;

pub struct Fx {
    _tmp: tempfile::TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    pub ssh_dir: PathBuf,
    pub bin: PathBuf,
    pub ctx: Context,
}

fn run_out(cmd: &mut Command) -> Output {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("A tool a test needs could not be started")
}

/// The real `SystemRoot`, read for the test's own context only.
pub fn system_root() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string())
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

pub fn user_sid() -> String {
    static SID: OnceLock<String> = OnceLock::new();
    SID.get_or_init(|| {
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
    })
    .clone()
}

/// Makes `dir` OpenSSH-safe for what is created under it afterwards: its own rules only.
fn make_openssh_safe(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let sid = user_sid();
    icacls(dir, &["/inheritance:r"]);
    icacls(
        dir,
        &[
            "/grant:r",
            &format!("*{sid}:(OI)(CI)F"),
            "*S-1-5-18:(OI)(CI)F",
            "*S-1-5-32-544:(OI)(CI)F",
        ],
    );
}

/// Makes the CA in the throwaway home with the operator's own `ca`, run in process through
/// `agentspace_ca::ca` as agentspace-sync's tests do (architect, 2026-11-20, for E0.06-T06-S01).
/// The context names the throwaway home and nothing of the real profile: no `LOCALAPPDATA`, and a
/// made-up user and computer name.
fn make_ca(home: &Path) {
    assert!(
        home.starts_with(std::env::temp_dir()),
        "The home is not under the temp folder"
    );
    let ctx = agentspace_operator_api::Context {
        home: home.to_path_buf(),
        local_app_data: None,
        system_root: PathBuf::from(system_root()),
        nested: false,
    };
    let env = agentspace_ca::CaEnv {
        user_name: Some("someone".to_string()),
        computer_name: Some("EXAMPLEPC".to_string()),
        ..agentspace_ca::CaEnv::default()
    };
    if let Err(f) = agentspace_ca::ca(&ctx, &env, &NullSay) {
        panic!(
            "The operator's ca could not make the throwaway CA: {}",
            f.message
        );
    }
    let key = home.join(".agentspace").join("ca").join("agentspace_ca");
    let public = home
        .join(".agentspace")
        .join("ca")
        .join("agentspace_ca.pub");
    assert!(
        key.is_file() && public.is_file(),
        "The operator's ca made no throwaway CA"
    );
    assert!(
        key.starts_with(home),
        "The CA is not under the throwaway home"
    );
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

impl Fx {
    fn build(with_ca: bool, ssh_elsewhere: bool, proxy_ext: Option<&str>) -> Fx {
        let tmp = tempfile::Builder::new()
            .prefix("asdrv-")
            .tempdir()
            .expect("A throwaway folder could not be made");
        let root = tmp.path().to_path_buf();
        assert!(
            root.starts_with(std::env::temp_dir()),
            "The throwaway folder is not under the temp folder"
        );
        let home = root.join("home dir");
        fs::create_dir_all(home.join("AppData").join("Local")).unwrap();
        fs::create_dir_all(home.join("AppData").join("Roaming")).unwrap();
        let ssh_dir = if ssh_elsewhere {
            root.join("ssh elsewhere")
        } else {
            home.join(".ssh")
        };
        make_openssh_safe(&ssh_dir);
        let bin = root.join("bin dir");
        fs::create_dir_all(&bin).unwrap();
        let own_exe = bin.join("agentspace.exe");
        fs::write(&own_exe, b"").unwrap();
        if let Some(ext) = proxy_ext {
            fs::write(bin.join(format!("agentspace-ssh-proxy.{ext}")), b"").unwrap();
        }
        if with_ca {
            make_ca(&home);
        }
        let ctx = Context {
            home: home.clone(),
            ssh_dir: ssh_dir.clone(),
            proxy_override: None,
            own_exe,
            system_root: Some(system_root()),
            state_dir: home.join(".agentspace"),
            operator_program: None,
            state_dir_env: None,
            ssh_dir_env: None,
        };
        Fx {
            _tmp: tmp,
            root,
            home,
            ssh_dir,
            bin,
            ctx,
        }
    }

    /// A home with a CA, `.ssh` under the home, and the proxy program `.exe` beside the program.
    pub fn with_ca() -> Fx {
        Fx::build(true, false, Some("exe"))
    }
    /// A home with no CA at all.
    pub fn without_ca() -> Fx {
        Fx::build(false, false, Some("exe"))
    }
    pub fn with_ca_ssh_elsewhere() -> Fx {
        Fx::build(true, true, Some("exe"))
    }
    /// The proxy program beside the program has the given extension (or none exists).
    pub fn with_ca_proxy(ext: Option<&str>) -> Fx {
        Fx::build(true, false, ext)
    }

    /// The driver, whose release walks for clients only inside a folder of the throwaway root that
    /// holds nothing, so that no test that releases a lease can end a real process (driver.md,
    /// "How it is checked"). A test that needs a stand-in client builds its own driver over the
    /// folder of its stand-ins.
    pub fn driver(&self) -> Ec2Driver {
        Ec2Driver::new().with_walk_under(self.root.join("no clients"))
    }
    pub fn lease(&self) -> LeaseRef<'static> {
        LeaseRef {
            id: ID,
            project: PROJECT,
        }
    }
    pub fn ca_dir(&self) -> PathBuf {
        self.home.join(".agentspace").join("ca")
    }
    pub fn leases_dir(&self) -> PathBuf {
        self.home.join(".agentspace").join("leases")
    }
    pub fn lease_dir(&self, id: &str) -> PathBuf {
        self.leases_dir().join(id)
    }
    pub fn key(&self) -> PathBuf {
        self.lease_dir(ID).join("id_ed25519")
    }
    pub fn pubkey(&self) -> PathBuf {
        self.lease_dir(ID).join("id_ed25519.pub")
    }
    pub fn cert(&self) -> PathBuf {
        self.lease_dir(ID).join("id_ed25519-cert.pub")
    }
    pub fn alias_dir(&self) -> PathBuf {
        self.ssh_dir.join("agentspace.d")
    }
    pub fn alias_file(&self) -> PathBuf {
        self.alias_dir().join(ALIAS)
    }
    pub fn config(&self) -> PathBuf {
        self.ssh_dir.join("config")
    }

    pub fn request<'a>(
        &self,
        platform: &'a str,
        profile: &'a str,
        size: &'a str,
        ttl: u32,
    ) -> PlaceRequest<'a> {
        PlaceRequest {
            id: ID,
            project: PROJECT,
            platform,
            profile,
            size,
            ttl_seconds: ttl,
        }
    }

    pub fn place(&self, ttl: u32) -> Result<Placed, Failure> {
        self.driver().place(
            &self.ctx,
            &self.request("linux/arm64", "agent-linux", "m", ttl),
        )
    }

    /// A place that MUST answer ready with the lease's alias and user. Returns `expires_at`.
    pub fn place_ready(&self, ttl: u32) -> i64 {
        match self.place(ttl) {
            Ok(Placed::Ready {
                host_alias,
                user,
                expires_at,
            }) => {
                assert_eq!(host_alias, ALIAS);
                assert_eq!(user, "ubuntu");
                expires_at
            }
            other => panic!("Expected a ready place, got {other:?}"),
        }
    }

    pub fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    /// Every file and folder under the throwaway root, with each file's bytes.
    pub fn snapshot(&self) -> Snapshot {
        fn walk(base: &Path, dir: &Path, out: &mut Snapshot) {
            for e in fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                let rel = p.strip_prefix(base).unwrap().display().to_string();
                if p.is_dir() {
                    out.insert(rel, None);
                    walk(base, &p, out);
                } else {
                    // A lock file that a test holds open for exclusive use cannot be read: it is
                    // recorded by name, with a marker no real file holds.
                    let bytes = fs::read(&p).unwrap_or_else(|_| b"<held open>".to_vec());
                    out.insert(rel, Some(bytes));
                }
            }
        }
        let mut out = Snapshot::new();
        walk(&self.root, &self.root, &mut out);
        out
    }

    /// Checks the throwaway root is as `before` found it, and names each file that was added,
    /// removed or changed, never its bytes. A lock file is left out of the comparison on both
    /// sides: a lock is a file that is made when it is first taken and never deleted
    /// (state.md, Concurrency), so reading the registry under `hosts.lock` makes one without
    /// writing anything.
    #[track_caller]
    pub fn assert_unchanged(&self, before: &Snapshot, why: &str) {
        let after = self.snapshot();
        let is_lock = |name: &str| name.ends_with(".lock");
        let mut diffs: Vec<String> = Vec::new();
        for (name, bytes) in after.iter().filter(|(n, _)| !is_lock(n)) {
            match before.get(name) {
                None => diffs.push(format!("added {name}")),
                Some(old) if old != bytes => diffs.push(format!("changed {name}")),
                Some(_) => {}
            }
        }
        for name in before.keys().filter(|n| !is_lock(n)) {
            if !after.contains_key(name) {
                diffs.push(format!("removed {name}"));
            }
        }
        assert!(diffs.is_empty(), "{why}: {diffs:?}");
    }

    /// Checks a message is agentspace's own words: no path, no folder name, no backend name.
    pub fn assert_plain(&self, msg: &str) {
        assert!(!msg.is_empty(), "The message is empty");
        assert!(!msg.starts_with(' '), "The message starts with a space");
        assert!(
            !msg.chars().any(|c| c.is_control()),
            "The message holds a control character: {msg:?}"
        );
        let low = msg.to_lowercase();
        let paths = vec![
            self.root.clone(),
            self.home.clone(),
            self.ssh_dir.clone(),
            std::env::temp_dir(),
            PathBuf::from(self.root.file_name().unwrap()),
        ];
        for p in paths {
            let s = p.display().to_string().to_lowercase();
            assert!(!low.contains(&s), "The message holds a path: {msg}");
            assert!(
                !low.contains(&s.replace('\\', "/")),
                "The message holds a path: {msg}"
            );
        }
        assert!(!msg.contains('\\'), "The message holds a backslash: {msg}");
        let b = msg.as_bytes();
        for i in 0..b.len().saturating_sub(2) {
            assert!(
                !(b[i].is_ascii_alphabetic()
                    && b[i + 1] == b':'
                    && (b[i + 2] == b'\\' || b[i + 2] == b'/')),
                "The message holds a drive path: {msg}"
            );
        }
        assert!(!low.contains("ec2"), "The message names the driver: {msg}");
        assert!(
            !low.contains("amazon"),
            "The message names the backend: {msg}"
        );
        assert!(
            !low.split(|c: char| !c.is_ascii_alphanumeric())
                .any(|w| w == "aws"),
            "The message names the backend: {msg}"
        );
        for var in ["USERNAME", "COMPUTERNAME"] {
            if let Ok(v) = std::env::var(var)
                && v.len() >= 4
            {
                assert!(
                    !low.contains(&v.to_lowercase()),
                    "The message holds the {var}: {msg}"
                );
            }
        }
    }

    /// A failure: its message is plain and its leftovers are named without an absolute path.
    pub fn assert_failure_plain(&self, f: &Failure) {
        self.assert_plain(&f.message);
        let folder = self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase();
        for l in &f.leftovers {
            assert!(
                l.starts_with("~/") || l.starts_with("the host alias file "),
                "A leftover is not home-relative: {l}"
            );
            assert!(!l.contains('\\'), "A leftover has a backslash: {l}");
            let low = l.to_lowercase();
            assert!(
                !low.contains(&folder),
                "A leftover holds the throwaway folder: {l}"
            );
            assert!(
                !low.contains("ec2") && !low.contains("amazon"),
                "A leftover names the backend: {l}"
            );
        }
    }

    /// Every home-relative leftover exists, and every file left under the lease folder is named by
    /// itself or by a folder above it (lease-api.md, Concurrency and atomicity).
    pub fn assert_leftovers_exact(&self, f: &Failure, id: &str) {
        let named: Vec<PathBuf> = f
            .leftovers
            .iter()
            .filter_map(|l| l.strip_prefix("~/"))
            .map(|rest| rest.split('/').fold(self.home.clone(), |p, c| p.join(c)))
            .collect();
        for p in &named {
            assert!(
                p.exists(),
                "A named leftover does not exist: {}",
                p.display()
            );
        }
        fn files(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    files(&p, out);
                } else {
                    out.push(p);
                }
            }
        }
        let dir = self.lease_dir(id);
        if dir.is_dir() {
            let mut all = Vec::new();
            files(&dir, &mut all);
            for file in all {
                assert!(
                    named.iter().any(|n| file.starts_with(n)),
                    "A file left behind is not named: {}",
                    file.display()
                );
            }
        }
    }

    /// `ssh -G -F <absolute config>` for the alias, through a config of the test's own whose
    /// `Include` is absolute, so the real `~/.ssh` is never read.
    pub fn ssh_g(&self, alias: &str) -> String {
        let cfg_dir = self.root.join("gcfg");
        make_openssh_safe(&cfg_dir);
        let top = cfg_dir.join("top.config");
        let d = slashed(&self.alias_dir());
        fs::write(&top, format!("Include \"{d}/*\"\n")).unwrap();
        let ssh = Path::new(&system_root())
            .join("System32")
            .join("OpenSSH")
            .join("ssh.exe");
        assert!(ssh.is_file(), "Windows OpenSSH is not installed");
        let out = run_out(
            Command::new(ssh)
                .env("USERPROFILE", &self.home)
                .env("HOME", &self.home)
                .arg("-G")
                .arg("-F")
                .arg(&top)
                .arg(alias),
        );
        assert!(
            out.status.success(),
            "ssh -G failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

pub fn values(out: &str, key: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.strip_prefix(&format!("{key} ")))
        .map(str::to_string)
        .collect()
}

/// A path as the alias file holds it: `\` as `/`.
pub fn slashed(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// A deny rule that stops deletes, lifted when it is dropped (so also when a test fails).
pub struct Deny {
    folder: PathBuf,
}

impl Deny {
    /// `flags` is an icacls inheritance and rights spec, as in `(CI)(DE,DC)`.
    pub fn new(folder: &Path, flags: &str) -> Deny {
        let d = Deny {
            folder: folder.to_path_buf(),
        };
        icacls(folder, &["/deny", &format!("*{}:{flags}", user_sid())]);
        d
    }
    pub fn lift(&self) {
        if !self.folder.exists() {
            return;
        }
        let sid = user_sid();
        let _ = run_out(
            Command::new("icacls")
                .arg(&self.folder)
                .args(["/remove:d", &format!("*{sid}")]),
        );
        let _ = run_out(
            Command::new("icacls")
                .arg(&self.folder)
                .args(["/reset", "/T", "/C"]),
        );
    }
}

impl Drop for Deny {
    fn drop(&mut self) {
        self.lift();
    }
}

// ---- Certificates, read with ssh-keygen -L in UTC ----

pub fn ssh_keygen(args: &[&str]) -> (bool, String, String) {
    // Windows' own OpenSSH, as `ssh_g` uses, so the UTC reading never depends on PATH.
    let keygen = Path::new(&system_root())
        .join("System32")
        .join("OpenSSH")
        .join("ssh-keygen.exe");
    let out = run_out(Command::new(keygen).args(args).env("TZ", "UTC"));
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

pub struct CertInfo {
    pub cert_type: String,
    pub public_fp: String,
    pub ca_fp: String,
    pub key_id: String,
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

pub fn parse_time(s: &str) -> i64 {
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

pub fn cert_info(cert: &Path) -> CertInfo {
    let (ok, out, err) = ssh_keygen(&["-L", "-f", cert.to_str().unwrap()]);
    assert!(ok, "ssh-keygen -L failed: {err}");
    let mut info = CertInfo {
        cert_type: String::new(),
        public_fp: String::new(),
        ca_fp: String::new(),
        key_id: String::new(),
        valid_from: 0,
        valid_to: 0,
        principals: vec![],
        critical: vec![],
        extensions: vec![],
    };
    let mut section = 0;
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
        } else if line.starts_with("Serial:") {
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

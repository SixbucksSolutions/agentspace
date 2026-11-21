//! Shared helpers of agentspace-sync's tests (E0.06-T03-S02, #0127).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The tests are written from
//! docs/operator.md (deploy, sync, The test seam, Messages, AWS failures, Calls by command,
//! Starting the box and waiting for it, What sync installs, The box's files, The token push) and
//! docs/state.md (Files in the checkout) alone.
//!
//! **How a test runs.** `sync` and `deploy` read the process environment (the seam's variables,
//! `PATH` for `go`) and start programs, and a test MUST NOT change its own process's environment.
//! So each scenario runs in a CHILD of the test's own binary: the parent builds a throwaway rig,
//! starts the same test again with `--exact` under a controlled environment and working folder
//! (`Rig::run`), and the child runs the body. The environment holds a throwaway home, the seam's
//! four variables naming stand-ins, and a `_POSIX_FD_STATE` variable that the library MUST remove
//! before it starts anything. Every program the library starts is a copy of one small stand-in
//! (`standin.rs.txt`, compiled with rustc once per machine into the temp folder) that records its
//! call into one shared log and answers by rules. The log carries the stand-ins' calls and the
//! `Say` events in one order.
//!
//! **No test reaches the real `go` or `C:\Projects\localswim`** (operator.md, The test seam, "Not
//! covered"; architect, 2026-11-19, for E0.06-T03-S02): a build from that checkout would read it.
//! - **`Rig::do_sync` and `Rig::do_deploy` run the crate's hidden `sync_using` and `deploy_using`**
//!   with the operator's own clock, CA check and push, a `go` finder that names the rig's stand-in
//!   or nothing, and a localswim folder inside the rig. Every test that could reach step 7 uses
//!   them.
//! - **The child's `PATH` is always one folder of the rig that holds nothing**, so no `go` can be
//!   found on it, whatever this machine has.
//! - **`Rig::do_pinned_sync` and `Rig::do_pinned_deploy` are the only calls of the pinned `sync`
//!   and `deploy`**, and each refuses to run unless it is in such a child. With no `go` on `PATH`
//!   the pinned pair skips the build and never looks at the checkout's folder. A unit test of the
//!   crate (`no_test_calls_the_pinned_pair_but_the_rigs_guarded_two`) keeps it so.
//! - **No test lists, reads or tests for the existence of `C:\Projects\localswim`**, and no
//!   stand-in runs there. Its name appears only as text, in the skip message the spec gives.
//!
//! **A test MUST NOT print what names this machine or its user.** A message, a line or a path is
//! compared with `same`, which shows both only with the rig's folder masked. The stand-ins record
//! the values of a few named, non-secret variables, never an environment.
#![cfg(windows)]
#![allow(dead_code)]

use agentspace_sync::Target;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use agentspace_ca::CaEnv;
use agentspace_operator_api::{CallerLine, Context, Failure, Mode, Say};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

pub mod certs;
pub mod expect;
pub mod hosts;
pub mod provlog;
pub mod times;

pub const US: char = '\u{1f}';
/// A made-up instance ID (it MUST NOT match the identifier scan).
pub const ID: &str = "i-1234567890abcdef0";
/// Another, for a deploy that finds a new instance.
pub const ID2: &str = "i-fedcba9876543210f";
pub const STAGING: &str = "/tmp/agentspace-system";
pub const CLAUDE_MD: &str = "# Test instructions\nSecond line.\n";
pub const KEY_LINE: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMP user@example";
/// A made-up token in the form the push accepts: 43 base64url characters.
pub const TOKEN: &str = "dummy-token-0123456789-0123456789-012345678";
pub const BASE: &str = "http://127.0.0.1:18811";
pub const ZONE: &str = "America/New_York";

pub fn fnv(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn norm(s: &str) -> String {
    s.replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The files under `dir` as `(relative path with /, length, hash)`, sorted.
pub fn listing(dir: &Path) -> Vec<(String, u64, u64)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, u64, u64)>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let b = fs::read(&p).unwrap();
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, b.len() as u64, fnv(&b)));
            }
        }
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v);
    v.sort();
    v
}

// ------------------------------------------------------------------ The stand-in program

static STANDIN: OnceLock<PathBuf> = OnceLock::new();

fn standin_exe() -> &'static Path {
    STANDIN.get_or_init(|| {
        let src = include_str!("standin.rs.txt");
        let dir = std::env::temp_dir().join(format!(
            "agentspace-sync-standin-{:016x}",
            fnv(src.as_bytes())
        ));
        let exe = dir.join("standin.exe");
        if !exe.is_file() {
            fs::create_dir_all(&dir).unwrap();
            let id = std::process::id();
            let tmp_src = dir.join(format!("standin-{id}.rs"));
            let tmp_exe = dir.join(format!("standin-{id}.exe"));
            fs::write(&tmp_src, src).unwrap();
            let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let out = Command::new(rustc)
                .args(["--edition", "2021", "-C", "opt-level=0", "-o"])
                .arg(&tmp_exe)
                .arg(&tmp_src)
                .output()
                .expect("rustc could not be started to build the stand-in");
            assert!(
                out.status.success(),
                "The stand-in did not compile: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let _ = fs::rename(&tmp_exe, &exe);
            let _ = fs::remove_file(&tmp_exe);
            let _ = fs::remove_file(&tmp_src);
        }
        exe
    })
}

// ------------------------------------------------------------------ Say

/// A `Say` that records what it is given, in the rig's log among the stand-ins' calls. A quiet
/// one records and prints nothing, as `-Quiet` does.
pub struct RecSay {
    log: PathBuf,
    quiet: bool,
    pub events: RefCell<Vec<(String, String)>>,
}

impl RecSay {
    fn note(&self, kind: &str, text: &str) {
        if self.quiet {
            return;
        }
        self.events
            .borrow_mut()
            .push((kind.to_string(), text.to_string()));
        append(&self.log, &format!("SAY{US}{kind}{US}{text}\n"));
    }
    pub fn progress_lines(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .filter(|(k, _)| k == "progress")
            .map(|(_, t)| t.clone())
            .collect()
    }
    pub fn warnings(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .filter(|(k, _)| k == "warning")
            .map(|(_, t)| t.clone())
            .collect()
    }
}

impl Say for RecSay {
    fn mode(&self) -> Mode {
        Mode::Person
    }
    fn progress(&self, person: &str, _caller: Option<CallerLine>) {
        self.note("progress", person);
    }
    fn warning(&self, person: &str) {
        self.note("warning", person);
    }
    fn detail(&self, _text: &str) {}
}

struct NullSay;
impl Say for NullSay {
    fn mode(&self) -> Mode {
        Mode::Person
    }
    fn progress(&self, _: &str, _: Option<CallerLine>) {}
    fn warning(&self, _: &str) {}
    fn detail(&self, _: &str) {}
}

fn append(path: &Path, text: &str) {
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(text.as_bytes());
    }
}

// ------------------------------------------------------------------ The log

#[derive(Clone)]
pub struct Rec {
    pub name: String,
    pub n: u32,
    pub cwd: String,
    pub nested: String,
    pub fd: usize,
    pub goos: String,
    pub goarch: String,
    pub cgo: String,
    pub args: Vec<String>,
}

pub enum Item {
    Call(Rec),
    Say(String, String),
}

/// A file as `(relative path, length, hash)`.
pub type Entry = (String, u64, u64);
/// A folder snapshot: `(program, call number, arg, entries)`.
pub type DirSnap = (String, u32, String, Vec<Entry>);

#[derive(Default)]
pub struct Log {
    pub items: Vec<Item>,
    /// `(program, call number, arg, length, hash)`
    pub files: Vec<(String, u32, String, u64, u64)>,
    /// `(program, call number, arg, entries)`
    pub dirs: Vec<DirSnap>,
    /// `(program, call number, path, content with line ends written \n and \r)`: a watched file
    /// as it was when the program was called.
    pub watches: Vec<(String, u32, String, String)>,
    /// `(program, call number, path, held or free)`: a probed lock file as the program found it.
    pub probes: Vec<(String, u32, String, String)>,
}

impl Log {
    pub fn calls(&self, name: &str) -> Vec<&Rec> {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::Call(r) if r.name == name => Some(r),
                _ => None,
            })
            .collect()
    }
    /// Every call of ssh, scp and go, in order.
    pub fn remote_calls(&self) -> Vec<&Rec> {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::Call(r) if r.name != "aws" => Some(r),
                _ => None,
            })
            .collect()
    }
    pub fn all_calls(&self) -> Vec<&Rec> {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::Call(r) => Some(r),
                _ => None,
            })
            .collect()
    }
    /// The calls of ssh, scp and go, and the `progress` lines that `keep` lets through, in order,
    /// as `ssh#2` and `say: <text>`.
    pub fn timeline(&self, keep: impl Fn(&str) -> bool) -> Vec<String> {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::Call(r) if r.name != "aws" => Some(format!("{}#{}", r.name, r.n)),
                Item::Say(k, t) if k == "progress" && keep(t) => Some(format!("say: {t}")),
                _ => None,
            })
            .collect()
    }
    pub fn file_snap(&self, name: &str, n: u32) -> Vec<(String, u64, u64)> {
        self.files
            .iter()
            .filter(|f| f.0 == name && f.1 == n)
            .map(|f| (f.2.clone(), f.3, f.4))
            .collect()
    }
    pub fn dir_snap(&self, name: &str, n: u32) -> Option<(String, Vec<Entry>)> {
        self.dirs
            .iter()
            .find(|d| d.0 == name && d.1 == n)
            .map(|d| (d.2.clone(), d.3.clone()))
    }
}

// ------------------------------------------------------------------ The rig

pub struct ChildOut {
    pub stdout: String,
    pub stderr: String,
}

pub struct Rig {
    pub root: PathBuf,
    pub home: PathBuf,
    pub lad: PathBuf,
    pub cwd: PathBuf,
    pub child: bool,
    /// Whether the `go` finder the rig gives `sync` names the rig's stand-in `go`.
    pub go: bool,
    /// Whether the rig's localswim folder is there.
    checkout: Cell<bool>,
    use_lad: Cell<bool>,
    nested: Cell<bool>,
    system_root: RefCell<Option<PathBuf>>,
    rules: RefCell<BTreeMap<String, Vec<String>>>,
    confirm: RefCell<Option<String>>,
    /// Files whose content each stand-in records at each call (parent only).
    watched: RefCell<Vec<PathBuf>>,
    /// Lock files each stand-in tries to open at each call (parent only).
    probed: RefCell<Vec<PathBuf>>,
    /// The stand-ins' read-only actions, as `readonly=` lines (parent only).
    readonly: RefCell<Vec<(String, String)>>,
    /// Whether the ssh stand-in keeps each call's whole standard input (parent only).
    capture_stdin: Cell<bool>,
    /// The copy of the test binary the child runs from, beside which a file for the operator's
    /// own folder can be put (parent only).
    operator_exe: RefCell<Option<PathBuf>>,
    _tmp: Option<TempDir>,
}

pub fn real_system_root() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows"))
}

pub fn ca_env() -> CaEnv {
    CaEnv {
        user_name: Some("someone".to_string()),
        computer_name: Some("EXAMPLEPC".to_string()),
        ..CaEnv::default()
    }
}

impl Rig {
    /// A rig whose `go` finder names the rig's stand-in `go` (`go`), or finds none. The child's
    /// `PATH` holds nothing either way, and the rig has its own localswim folder unless
    /// [`Rig::without_checkout`] removes it.
    pub fn new(go: bool) -> Rig {
        if let Some(dir) = std::env::var_os("AS_RIG_DIR") {
            return Rig::build(PathBuf::from(dir), go, true, None);
        }
        let tmp = tempfile::Builder::new()
            .prefix("assync-")
            .tempdir()
            .unwrap();
        let root = tmp.path().to_path_buf();
        let rig = Rig::build(root, go, false, Some(tmp));
        for d in [
            "home",
            "lad",
            "cwd",
            "bin",
            "gobin",
            "emptybin",
            "tmp",
            "appdata",
            "localswim",
        ] {
            fs::create_dir_all(rig.root.join(d)).unwrap();
        }
        for (dir, name) in [
            ("bin", "aws"),
            ("bin", "ssh"),
            ("bin", "scp"),
            ("gobin", "go"),
        ] {
            fs::copy(
                standin_exe(),
                rig.root.join(dir).join(format!("{name}.exe")),
            )
            .unwrap();
        }
        // The box's record, with a made-up instance ID: from v0.07 the registry holds it, where
        // the retired instance-ID cache did (operator.md, The test seam, step 2).
        rig.write_home(
            ".agentspace/hosts.json",
            format!(
                "{{\"version\":0,\"hosts\":[{{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"{ID}\"}}]}}\n"
            )
            .as_bytes(),
        );
        rig
    }

    fn build(root: PathBuf, go: bool, child: bool, tmp: Option<TempDir>) -> Rig {
        Rig {
            home: root.join("home"),
            lad: root.join("lad"),
            cwd: root.join("cwd"),
            root,
            child,
            go,
            checkout: Cell::new(true),
            use_lad: Cell::new(false),
            nested: Cell::new(false),
            system_root: RefCell::new(None),
            rules: RefCell::new(BTreeMap::new()),
            confirm: RefCell::new(None),
            watched: RefCell::new(Vec::new()),
            probed: RefCell::new(Vec::new()),
            readonly: RefCell::new(Vec::new()),
            capture_stdin: Cell::new(false),
            operator_exe: RefCell::new(None),
            _tmp: tmp,
        }
    }

    pub fn log_path(&self) -> PathBuf {
        self.root.join("calls.log")
    }

    pub fn exe(&self, name: &str) -> PathBuf {
        if name == "go" {
            self.root.join("gobin").join("go.exe")
        } else {
            self.root.join("bin").join(format!("{name}.exe"))
        }
    }

    // ---- Setup (the file effects run in the parent only; the flags run in both)

    /// The path of a file under the throwaway home, with `/` as the separator in `rel`.
    pub fn home_path(&self, rel: &str) -> PathBuf {
        let mut p = self.home.clone();
        for part in rel.split('/') {
            p.push(part);
        }
        p
    }

    pub fn write_home(&self, rel: &str, bytes: &[u8]) {
        if self.child {
            return;
        }
        let p = self.home_path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    pub fn remove_home(&self, rel: &str) {
        if self.child {
            return;
        }
        let p = self.home_path(rel);
        if p.is_dir() {
            fs::remove_dir_all(p).unwrap();
        } else {
            let _ = fs::remove_file(p);
        }
    }

    pub fn mkdir_home(&self, rel: &str) {
        if self.child {
            return;
        }
        fs::create_dir_all(self.home_path(rel)).unwrap();
    }

    pub fn with_claude_md(&self) {
        self.write_home(".claude/CLAUDE.md", CLAUDE_MD.as_bytes());
    }

    pub fn with_deploy_inputs(&self) {
        self.write_home(".ssh/id_ed25519.pub", format!("{KEY_LINE}\r\n").as_bytes());
        self.write_home(
            ".agentspace/stack-parameters.json",
            br#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE"}}"#,
        );
    }

    /// The rig's own localswim checkout, which the build runs in. It stands in for
    /// `C:\Projects\localswim`, which no test reaches.
    pub fn localswim(&self) -> PathBuf {
        self.root.join("localswim")
    }

    /// Removes the rig's localswim folder: the laptop has no checkout.
    pub fn without_checkout(&self) {
        self.checkout.set(false);
        if self.child {
            return;
        }
        fs::remove_dir_all(self.localswim()).unwrap();
    }

    /// Whether the rig's localswim folder is there, as the rig was set up.
    pub fn has_checkout(&self) -> bool {
        self.checkout.get()
    }

    pub fn set_nested(&self, nested: bool) {
        self.nested.set(nested);
    }

    pub fn set_system_root(&self, p: PathBuf) {
        *self.system_root.borrow_mut() = Some(p);
    }

    /// A rule for a stand-in: the `n`th call (0 for any) whose arguments contain `contains` (empty
    /// for any) exits with `code` and writes `out` and `err` (`\n` in them is a line feed). The
    /// first rule that matches wins, and a rule added here is tried before the defaults.
    pub fn rule(&self, prog: &str, n: u32, contains: &str, code: i32, out: &str, err: &str) {
        self.rules
            .borrow_mut()
            .entry(prog.to_string())
            .or_default()
            .push(format!(
                "rule={n}{US}{contains}{US}{code}{US}{out}{US}{err}"
            ));
    }

    /// Makes the `n`th call of `prog` (0 for any) whose arguments contain `contains` set the file
    /// at `path` read-only before it answers, so that the next replace of that file fails.
    pub fn readonly_at(&self, prog: &str, n: u32, contains: &str, path: &Path) {
        self.readonly.borrow_mut().push((
            prog.to_string(),
            format!("readonly={n}{US}{contains}{US}{}", path.display()),
        ));
    }

    /// Makes the CA in the throwaway home, as `ca` does.
    pub fn make_ca(&self) {
        if self.child {
            return;
        }
        let ctx = self.ctx();
        if let Err(f) = agentspace_ca::ca(&ctx, &ca_env(), &NullSay) {
            panic!("The CA could not be made: {}", self.mask(&f.message));
        }
    }

    /// The localswim receipt and a dummy credential under the rig's `LOCALAPPDATA`, and an ssh
    /// stand-in that confirms the push as the box would.
    pub fn enable_push(&self) {
        self.use_lad.set(true);
        let content = format!("LOCALSWIM_URL={BASE}\nLOCALSWIM_TOKEN={TOKEN}\n");
        let hash: String = Sha256::digest(content.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        *self.confirm.borrow_mut() = Some(format!("{hash}|{ZONE}"));
        if self.child {
            return;
        }
        let cred_dir = self.lad.join("localswim-v2").join("credentials").join("c1");
        fs::create_dir_all(&cred_dir).unwrap();
        let receipt = serde_json::json!({
            "base": BASE,
            "credential_directory": cred_dir.to_string_lossy(),
        });
        fs::write(
            self.lad.join("localswim-v2").join("service.json"),
            receipt.to_string(),
        )
        .unwrap();
        let mut plain = Vec::new();
        for u in TOKEN.encode_utf16() {
            plain.extend(u.to_le_bytes());
        }
        let blob = win::protect(&plain);
        let hex: String = blob.iter().map(|b| format!("{b:02x}")).collect();
        let credential = serde_json::json!({
            "format": "localswim-credential",
            "format_version": 1,
            "role": "bot",
            "token_dpapi_hex": hex,
        });
        fs::write(cred_dir.join("bot.json"), credential.to_string()).unwrap();
    }

    pub fn ctx(&self) -> Context {
        Context {
            home: self.home.clone(),
            local_app_data: if self.use_lad.get() {
                Some(self.lad.clone())
            } else {
                None
            },
            system_root: self
                .system_root
                .borrow()
                .clone()
                .unwrap_or_else(real_system_root),
            nested: self.nested.get(),
        }
    }

    // ---- Running

    fn flush_cfgs(&self) {
        let rules = self.rules.borrow();
        let defaults_aws = [
            format!(
                "rule=0{US}describe-instances{US}0{US}{}{US}",
                json_instance("running")
            ),
            format!("rule=0{US}describe-stacks{US}0{US}{}{US}", json_stack(ID2)),
        ];
        for prog in ["aws", "ssh", "scp", "go"] {
            let dir = if prog == "go" { "gobin" } else { "bin" };
            let mut text = format!("log={}\n", self.log_path().display());
            if prog != "go" {
                for w in self.watched.borrow().iter() {
                    text.push_str(&format!("watch={}\n", w.display()));
                }
                for p in self.probed.borrow().iter() {
                    text.push_str(&format!("probe={}\n", p.display()));
                }
            }
            if prog == "ssh" && self.capture_stdin.get() {
                text.push_str(&format!("stdin_dir={}\n", self.stdin_dir().display()));
            }
            if prog == "ssh" {
                text.push_str(&format!(
                    "stdin={}\n",
                    self.root.join("stdin.bin").display()
                ));
                if let Some(c) = self.confirm.borrow().as_ref() {
                    text.push_str(&format!("confirm={c}\n"));
                }
            }
            for (p, line) in self.readonly.borrow().iter() {
                if p == prog {
                    text.push_str(line);
                    text.push('\n');
                }
            }
            if let Some(v) = rules.get(prog) {
                for r in v {
                    text.push_str(r);
                    text.push('\n');
                }
            }
            if prog == "aws" {
                for r in &defaults_aws {
                    text.push_str(r);
                    text.push('\n');
                }
            }
            fs::write(self.root.join(dir).join(format!("{prog}.cfg")), text).unwrap();
            let _ = fs::remove_file(self.root.join(dir).join(format!("{prog}.count")));
        }
    }

    /// In the parent: starts this test again as a child of the test binary, under the rig's
    /// environment and working folder, and checks that it passed. In the child: runs `body`.
    pub fn run(&self, body: impl FnOnce(&Rig)) -> Option<ChildOut> {
        if self.child {
            body(self);
            return None;
        }
        self.flush_cfgs();
        let name = std::thread::current()
            .name()
            .expect("A test thread has a name")
            .to_string();
        let sysroot = real_system_root();
        // One folder that holds nothing: no `go` can be found on the child's `PATH`. The rig's
        // stand-in `go` is in another folder, and reaches `sync` only through its `go` finder.
        let path = self.empty_path_folder();
        let tmp = self.root.join("tmp");
        let exe = self
            .operator_exe
            .borrow()
            .clone()
            .unwrap_or_else(|| std::env::current_exe().unwrap());
        let out = Command::new(exe)
            .args([name.as_str(), "--exact", "--nocapture", "--test-threads=1"])
            .env_clear()
            .env("SystemRoot", &sysroot)
            .env("windir", &sysroot)
            .env("USERPROFILE", &self.home)
            .env("HOME", &self.home)
            .env("LOCALAPPDATA", &self.lad)
            .env("APPDATA", self.root.join("appdata"))
            .env("TEMP", &tmp)
            .env("TMP", &tmp)
            .env("PATH", &path)
            .env("AGENTSPACE_AWS_CMD", self.exe("aws"))
            .env("AGENTSPACE_SSH_CMD", self.exe("ssh"))
            .env("AGENTSPACE_SCP_CMD", self.exe("scp"))
            .env("AGENTSPACE_TIMEZONE", ZONE)
            .env("AB12CD34_POSIX_FD_STATE", "1")
            .env("AS_RIG_DIR", &self.root)
            .env("RUST_BACKTRACE", "0")
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .output()
            .expect("The test binary could not be started again");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        if !out.status.success() {
            let text = self.mask(&format!("{stdout}\n{stderr}"));
            let tail: Vec<&str> = text.lines().rev().take(60).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            panic!(
                "The isolated run failed (exit {:?}):\n{}",
                out.status.code(),
                tail.join("\n")
            );
        }
        // A child that matched no test exits 0 too, and would pass the scenario unrun.
        assert!(
            stdout.contains("test result: ok. 1 passed;"),
            "The isolated run MUST have run this one test, and it ran none"
        );
        Some(ChildOut { stdout, stderr })
    }

    // ---- The library

    pub fn say(&self) -> RecSay {
        RecSay {
            log: self.log_path(),
            quiet: false,
            events: RefCell::new(Vec::new()),
        }
    }

    pub fn quiet_say(&self) -> RecSay {
        RecSay {
            log: self.log_path(),
            quiet: true,
            events: RefCell::new(Vec::new()),
        }
    }

    fn empty_path_folder(&self) -> PathBuf {
        self.root.join("emptybin")
    }

    /// The `go` the rig's finder gives `sync`: the rig's stand-in, or none.
    pub fn found_go(&self) -> Option<PathBuf> {
        self.go.then(|| self.exe("go"))
    }

    /// `sync` as the operator runs it, but for `go` and the localswim folder, which are the
    /// rig's: the crate's hidden `sync_using`, with its own clock, CA check and push.
    pub fn do_sync(&self, say: &RecSay) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::sync_using(&ctx, &ca_env(), say, &run, &run, seams)
        })
    }

    /// `deploy`, as [`Rig::do_sync`] runs `sync`.
    pub fn do_deploy(&self, say: &RecSay) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::deploy_using(&ctx, &ca_env(), say, &run, &run, seams)
        })
    }

    /// Refuses to go on unless no `go` can be found from here: this is the rig's child, and its
    /// `PATH` is the rig's one folder, which holds nothing. The pinned `sync` and `deploy` look
    /// for `go` on `PATH`, and with none they skip the build without looking at the checkout.
    fn refuse_unless_no_go_can_be_found(&self) {
        assert!(
            self.child,
            "The pinned sync and deploy run only in the rig's child, whose PATH holds nothing"
        );
        let empty = self.empty_path_folder();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let folders: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert!(
            folders.len() == 1 && folders[0] == empty,
            "The child's PATH MUST be the rig's one empty folder; it names {} folders",
            folders.len()
        );
        let holds_nothing = fs::read_dir(&empty)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        assert!(
            holds_nothing,
            "The folder on the child's PATH MUST hold nothing"
        );
    }

    /// The pinned `sync` itself, where no `go` can be found.
    pub fn do_pinned_sync(&self, say: &RecSay) -> Result<(), Failure> {
        self.refuse_unless_no_go_can_be_found();
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        agentspace_sync::sync(&ctx, &ca_env(), Target::Box, say, &run, &run)
    }

    /// The pinned `deploy` itself, where no `go` can be found.
    pub fn do_pinned_deploy(&self, say: &RecSay) -> Result<(), Failure> {
        self.refuse_unless_no_go_can_be_found();
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        agentspace_sync::deploy(&ctx, &ca_env(), Target::Box, say, &run, &run)
    }

    // ---- Reading what happened

    pub fn log(&self) -> Log {
        let text = fs::read_to_string(self.log_path()).unwrap_or_default();
        let mut log = Log::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split(US).collect();
            match f[0] {
                "CALL" => log.items.push(Item::Call(Rec {
                    name: f[1].to_string(),
                    n: f[2].parse().unwrap(),
                    cwd: f[3].to_string(),
                    nested: f[4].to_string(),
                    fd: f[5].parse().unwrap(),
                    goos: f[6].to_string(),
                    goarch: f[7].to_string(),
                    cgo: f[8].to_string(),
                    args: f[10..].iter().map(|s| s.to_string()).collect(),
                })),
                "SAY" => log
                    .items
                    .push(Item::Say(f[1].to_string(), f[2].to_string())),
                "WATCH" => log.watches.push((
                    f[1].to_string(),
                    f[2].parse().unwrap(),
                    f[3].to_string(),
                    f.get(4).copied().unwrap_or("").to_string(),
                )),
                "PROBE" => log.probes.push((
                    f[1].to_string(),
                    f[2].parse().unwrap(),
                    f[3].to_string(),
                    f[4].to_string(),
                )),
                "SNAPF" => log.files.push((
                    f[1].to_string(),
                    f[2].parse().unwrap(),
                    f[3].to_string(),
                    f[4].parse().unwrap(),
                    f[5].parse().unwrap(),
                )),
                "SNAPD" => log.dirs.push((
                    f[1].to_string(),
                    f[2].parse().unwrap(),
                    f[3].to_string(),
                    vec![],
                )),
                "SNAPE" => {
                    let n: u32 = f[2].parse().unwrap();
                    if let Some(d) = log
                        .dirs
                        .iter_mut()
                        .rev()
                        .find(|d| d.0 == f[1] && d.1 == n && d.2 == f[3])
                    {
                        d.3.push((
                            f[4].to_string(),
                            f[5].parse().unwrap(),
                            f[6].parse().unwrap(),
                        ));
                    }
                }
                _ => {}
            }
        }
        log
    }

    // ---- Assertions that never show this machine's names

    pub fn mask(&self, s: &str) -> String {
        let root = self.root.to_string_lossy().into_owned();
        let mut out = s.replace(&root, "<rig>");
        out = out.replace(&root.replace('\\', "/"), "<rig>");
        let sr = real_system_root().to_string_lossy().into_owned();
        out.replace(&sr, "<SystemRoot>")
    }

    pub fn same(&self, what: &str, got: &str, want: &str) {
        if got != want {
            panic!(
                "{what}: expected\n  {}\nbut got\n  {}",
                self.mask(want),
                self.mask(got)
            );
        }
    }

    pub fn same_path(&self, what: &str, got: &str, want: &Path) {
        let want = want.to_string_lossy().into_owned();
        if norm(got) != norm(&want) {
            panic!(
                "{what}: expected path\n  {}\nbut got\n  {}",
                self.mask(&want),
                self.mask(got)
            );
        }
    }

    /// Compares two folders that exist, whatever form each path is written in.
    pub fn same_folder(&self, what: &str, got: &str, want: &Path) {
        match (fs::canonicalize(got), fs::canonicalize(want)) {
            (Ok(a), Ok(b)) if a == b => {}
            _ => self.same_path(what, got, want),
        }
    }

    pub fn same_list(&self, what: &str, got: &[String], want: &[String]) {
        if got != want {
            panic!(
                "{what}: expected\n  {}\nbut got\n  {}",
                self.mask(&want.join("\n  ")),
                self.mask(&got.join("\n  "))
            );
        }
    }

    pub fn check(&self, ok: bool, what: &str) {
        if !ok {
            panic!("{}", self.mask(what));
        }
    }

    /// The failure of a command that MUST have failed.
    pub fn failure(&self, r: Result<(), Failure>, what: &str) -> Failure {
        match r {
            Ok(()) => panic!("{what}: the command succeeded, but it MUST have failed"),
            Err(f) => f,
        }
    }

    /// Checks that a command succeeded, showing a failure's message masked.
    pub fn succeeded(&self, r: Result<(), Failure>, what: &str) {
        if let Err(f) = r {
            panic!("{what}: failed with {}", self.mask(&f.message));
        }
    }
}

pub fn json_instance(state: &str) -> String {
    format!(
        "{{\"Reservations\":[{{\"Instances\":[{{\"State\":{{\"Name\":\"{state}\"}},\"LaunchTime\":\"2020-01-01T00:00:00+00:00\"}}]}}]}}"
    )
}

pub fn json_stack(id: &str) -> String {
    format!(
        "{{\"Stacks\":[{{\"Outputs\":[{{\"OutputKey\":\"InstanceId\",\"OutputValue\":\"{id}\"}}]}}]}}"
    )
}

/// The AWS arguments the start-and-wait step's first call has (operator.md, AWS failures, FB-07).
pub fn describe_instances_args(id: &str) -> Vec<String> {
    [
        "ec2",
        "describe-instances",
        "--region",
        "us-west-2",
        "--output",
        "json",
        "--instance-ids",
        id,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn svec(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ------------------------------------------------------------------ The one Windows module

pub mod win {
    //! The only module of the tests that calls Windows: `CryptProtectData`, for a dummy
    //! credential (operator.md, The Bot credential).
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };

    /// Sets the creation time of an existing folder to `unix_secs`, for the 24-hour sweep's test
    /// (operator.md, The operator's folder, one per command: the creation time is the measure).
    pub fn set_dir_creation_time(path: &std::path::Path, unix_secs: i64) {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING, SetFileTime,
        };
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let ticks = ((unix_secs + 11_644_473_600) as u64) * 10_000_000;
        let time = FILETIME {
            dwLowDateTime: ticks as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        };
        // SAFETY: `wide` is a NUL-terminated path that outlives the call, `time` outlives the
        // call, and the handle is closed before the function ends.
        unsafe {
            let handle = CreateFileW(
                wide.as_ptr(),
                FILE_WRITE_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            );
            assert!(
                handle != INVALID_HANDLE_VALUE,
                "A throwaway folder could not be opened to set its creation time"
            );
            let ok = SetFileTime(handle, &time, std::ptr::null(), std::ptr::null());
            CloseHandle(handle);
            assert!(
                ok != 0,
                "A throwaway folder's creation time could not be set"
            );
        }
    }

    pub fn protect(plain: &[u8]) -> Vec<u8> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: plain.len() as u32,
            pbData: plain.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: `input` points at `plain`, which outlives the call; `out` is filled by the call
        // and freed below with `LocalFree`, as the API requires.
        unsafe {
            let ok = CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            );
            assert!(
                ok != 0 && !out.pbData.is_null(),
                "CryptProtectData failed for a dummy credential"
            );
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as *mut core::ffi::c_void);
            bytes
        }
    }
}

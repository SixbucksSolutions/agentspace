//! The throwaway world the lease commands' tests run in (E0.05-T03-S02).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. A `Lab` is one throwaway
//! folder under the temp folder, with a home, a state folder, an OpenSSH client folder and a
//! working folder in it. It runs `agentspace_cli::execute_in` with a `World` whose variable lookup
//! points `USERPROFILE`, `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` there, so a test never
//! touches the real home and never changes its own process's environment.
//!
//! Every expectation is taken from docs/lease-api.md, docs/state.md and docs/driver.md, and from the
//! schemas under `schemas/`, never from the program's code (D31).
//!
//! A test file includes this module, and `fake_driver`, with `#[path = "common/lab.rs"] mod lab;`
//! after `mod common;`.

#![allow(dead_code)]

use crate::common::{Run, failure_problems, is_compact, repo_root};
use agentspace_cli::{World, execute_in};
use agentspace_driver_api::Driver;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::fake_driver::{FakeDriver, Handle, ISSUE_TIME, alias_of};

/// The clock every run starts with, in whole seconds since the Unix epoch: 2027-01-15T08:00:00Z.
pub const NOW: i64 = ISSUE_TIME;

// ---- Times and IDs -----------------------------------------------------------------------------

/// A time in the contract's form, `YYYY-MM-DDTHH:MM:SSZ` (lease-api.md, Times).
pub fn fmt_time(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// A lowercase UUIDv4 for `n`, whose first 8 hex digits differ for every `n`.
pub fn uuid_n(n: u32) -> String {
    format!("{:08x}-1111-4111-8111-{:012x}", 0xc0de_0000_u32 + n, n)
}

/// A second UUIDv4 that shares `id`'s first 8 hex digits, so it has the same alias in a project.
pub fn same_alias_id(id: &str) -> String {
    format!("{}-2222-4222-9222-222222222222", &id[..8])
}

// ---- Records and the documents they print ------------------------------------------------------

/// One record of the lease state file (state.md, The lease state file), and what it prints as.
#[derive(Clone, Debug)]
pub struct Rec {
    pub id: String,
    pub project: String,
    /// The state as stored: `pending`, `ready`, `released` or `unsatisfiable`.
    pub state: &'static str,
    pub platform: String,
    pub profile: String,
    pub size: String,
    pub ttl_seconds: u32,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub released_at: Option<i64>,
    pub ssh: Option<(String, String)>,
    pub reasons: Vec<(String, String)>,
    pub driver: Option<String>,
}

impl Rec {
    fn base(id: &str, project: &str, state: &'static str) -> Rec {
        Rec {
            id: id.to_string(),
            project: project.to_string(),
            state,
            platform: "linux/arm64".to_string(),
            profile: "agent-linux".to_string(),
            size: "m".to_string(),
            ttl_seconds: 3600,
            created_at: NOW - 1000,
            expires_at: None,
            released_at: None,
            ssh: None,
            reasons: Vec::new(),
            driver: Some("fake".to_string()),
        }
    }

    /// Stored `ready`, expiring 30 minutes after `NOW`, placed by the driver named `fake`.
    pub fn ready(id: &str, project: &str) -> Rec {
        let mut r = Rec::base(id, project, "ready");
        r.expires_at = Some(NOW + 1800);
        r.ssh = Some((alias_of(project, id), "ubuntu".to_string()));
        r
    }

    /// Stored `ready` with an expiry 10 seconds before `NOW`: the lease is `expired` as read.
    pub fn expired(id: &str, project: &str) -> Rec {
        Rec::ready(id, project).expiring(NOW - 10)
    }

    pub fn pending(id: &str, project: &str) -> Rec {
        Rec::base(id, project, "pending")
    }

    /// Released after it was ready: keeps its `expires_at`.
    pub fn released(id: &str, project: &str) -> Rec {
        let mut r = Rec::base(id, project, "released");
        r.expires_at = Some(NOW - 500);
        r.released_at = Some(NOW - 100);
        r
    }

    /// Released from `pending`: has no `expires_at`.
    pub fn released_from_pending(id: &str, project: &str) -> Rec {
        let mut r = Rec::base(id, project, "released");
        r.released_at = Some(NOW - 100);
        r
    }

    pub fn unsatisfiable(id: &str, project: &str) -> Rec {
        let mut r = Rec::base(id, project, "unsatisfiable");
        r.size = "xl".to_string();
        r.reasons = vec![(
            "unsupported_size".to_string(),
            "Size 'xl' is not supported; v0 supports size m only.".to_string(),
        )];
        r
    }

    pub fn expiring(mut self, at: i64) -> Rec {
        self.expires_at = Some(at);
        self
    }

    pub fn created(mut self, at: i64) -> Rec {
        self.created_at = at;
        self
    }

    pub fn ttl(mut self, seconds: u32) -> Rec {
        self.ttl_seconds = seconds;
        self
    }

    pub fn driver(mut self, name: Option<&str>) -> Rec {
        self.driver = name.map(str::to_string);
        self
    }

    /// The record as the state file stores it.
    pub fn record(&self) -> Value {
        let mut o = serde_json::Map::new();
        o.insert("id".into(), json!(self.id));
        o.insert("project".into(), json!(self.project));
        o.insert("state".into(), json!(self.state));
        o.insert("platform".into(), json!(self.platform));
        o.insert("profile".into(), json!(self.profile));
        o.insert("size".into(), json!(self.size));
        o.insert("ttl_seconds".into(), json!(self.ttl_seconds));
        o.insert("created_at".into(), json!(fmt_time(self.created_at)));
        if let Some(t) = self.expires_at {
            o.insert("expires_at".into(), json!(fmt_time(t)));
        }
        if let Some(t) = self.released_at {
            o.insert("released_at".into(), json!(fmt_time(t)));
        }
        if let Some((alias, user)) = &self.ssh {
            o.insert("ssh".into(), json!({"host_alias": alias, "user": user}));
        }
        if !self.reasons.is_empty() {
            let reasons: Vec<Value> = self
                .reasons
                .iter()
                .map(|(c, m)| json!({"code": c, "message": m}))
                .collect();
            o.insert("reasons".into(), Value::Array(reasons));
        }
        if let Some(d) = &self.driver {
            o.insert("driver".into(), json!(d));
        }
        Value::Object(o)
    }

    /// The lease document a command that reads the lease at `now` prints: a stored `ready` record
    /// whose `expires_at` is at or before `now` is `expired` and has no `ssh` (lease-api.md,
    /// States), and `driver` is never printed.
    pub fn doc(&self, now: i64) -> Value {
        let mut v = self.record();
        let o = v.as_object_mut().expect("A record is an object");
        o.remove("driver");
        if self.state == "ready" && self.expires_at.is_some_and(|t| t <= now) {
            o.insert("state".into(), json!("expired"));
            o.remove("ssh");
        }
        v
    }

    /// The alias a lease has, whatever its state (lease-api.md, Fields).
    pub fn alias(&self) -> String {
        alias_of(&self.project, &self.id)
    }
}

// ---- The lab -----------------------------------------------------------------------------------

enum Clock {
    Fixed(i64),
    /// Returns the next second on every call, so a command that reads the clock twice can be seen.
    Ticking(i64),
    /// Answers `held` while the lease state lock is held and `free` while it is not, so a test can
    /// see whether the clock was read before or after the command took the lock.
    ByLock {
        lock_file: PathBuf,
        free: i64,
        held: i64,
    },
}

enum IdSource {
    Counter,
    Script(VecDeque<String>),
    /// The same ID every time, until `GUARD` draws, so that a build with no bound ends the test.
    Always(String),
}

/// The most draws an always-colliding source answers before it gives a free ID.
pub const GUARD: usize = 5000;

pub struct Lab {
    pub root: PathBuf,
    pub home: PathBuf,
    pub state: PathBuf,
    pub ssh: PathBuf,
    pub cwd: PathBuf,
    pub exe: PathBuf,
    /// Whether the world's working folder can be read; `false` passes `cwd: None`.
    cwd_known: bool,
    vars: BTreeMap<String, OsString>,
    clock: Mutex<Clock>,
    now_calls: AtomicUsize,
    ids: Mutex<IdSource>,
    draws: AtomicUsize,
    next_id: AtomicU32,
    drivers: Vec<Box<dyn Driver>>,
}

static LAB_COUNT: AtomicU32 = AtomicU32::new(0);

impl Lab {
    /// A throwaway world with no driver. `USERPROFILE`, `AGENTSPACE_STATE_DIR` and
    /// `AGENTSPACE_SSH_DIR` point into it; the state and ssh folders are not made.
    pub fn new() -> Lab {
        let n = LAB_COUNT.fetch_add(1, Ordering::SeqCst);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir()
            .join("agentspace-lease-tests")
            .join(format!("t{}-{n}-{stamp}", std::process::id()));
        let home = root.join("home");
        let state = root.join("state");
        let ssh = root.join("ssh");
        let cwd = root.join("cwd");
        std::fs::create_dir_all(&home).expect("The throwaway home can be made");
        std::fs::create_dir_all(&cwd).expect("The throwaway working folder can be made");
        let mut vars = BTreeMap::new();
        vars.insert("USERPROFILE".to_string(), home.clone().into_os_string());
        vars.insert(
            "AGENTSPACE_STATE_DIR".to_string(),
            state.clone().into_os_string(),
        );
        vars.insert(
            "AGENTSPACE_SSH_DIR".to_string(),
            ssh.clone().into_os_string(),
        );
        Lab {
            exe: root.join("bin").join("agentspace.exe"),
            root,
            home,
            state,
            ssh,
            cwd,
            cwd_known: true,
            vars,
            clock: Mutex::new(Clock::Fixed(NOW)),
            now_calls: AtomicUsize::new(0),
            ids: Mutex::new(IdSource::Counter),
            draws: AtomicUsize::new(0),
            next_id: AtomicU32::new(1),
            drivers: Vec::new(),
        }
    }

    /// A lab with one fake driver named `fake`.
    pub fn with_fake() -> (Lab, Handle) {
        let mut lab = Lab::new();
        let h = lab.fake("fake");
        (lab, h)
    }

    /// Adds a fake driver named `name` to the world's drivers.
    pub fn fake(&mut self, name: &str) -> Handle {
        let (d, h) = FakeDriver::new(name);
        self.drivers.push(Box::new(d));
        h
    }

    pub fn set_var(&mut self, key: &str, value: impl Into<OsString>) {
        self.vars.insert(key.to_string(), value.into());
    }

    pub fn unset_var(&mut self, key: &str) {
        self.vars.remove(key);
    }

    pub fn set_now(&mut self, secs: i64) {
        *self.clock.lock().unwrap() = Clock::Fixed(secs);
    }

    /// The clock answers `secs`, then `secs + 1`, and so on, one more for every read.
    pub fn tick_from(&mut self, secs: i64) {
        *self.clock.lock().unwrap() = Clock::Ticking(secs);
    }

    /// The clock answers `held` while `leases.lock` is held and `free` while it is not. The lock
    /// file MUST exist before the run.
    pub fn clock_by_lock(&mut self, free: i64, held: i64) {
        *self.clock.lock().unwrap() = Clock::ByLock {
            lock_file: self.lock_file(),
            free,
            held,
        };
    }

    /// The world's working folder cannot be read: runs pass `cwd: None`.
    pub fn lose_cwd(&mut self) {
        self.cwd_known = false;
    }

    /// `new_id` answers these in order, then fresh IDs that collide with nothing.
    pub fn script_ids(&mut self, ids: &[&str]) {
        *self.ids.lock().unwrap() = IdSource::Script(ids.iter().map(|s| s.to_string()).collect());
    }

    /// `new_id` answers `id` every time, up to `GUARD` draws.
    pub fn always_id(&mut self, id: &str) {
        *self.ids.lock().unwrap() = IdSource::Always(id.to_string());
    }

    pub fn state_file(&self) -> PathBuf {
        self.state.join("leases.json")
    }

    pub fn lock_file(&self) -> PathBuf {
        self.state.join("leases.lock")
    }

    /// Writes the state file with these records, as the program writes it: compact, then one LF.
    pub fn seed(&self, recs: &[Rec]) {
        let leases: Vec<Value> = recs.iter().map(Rec::record).collect();
        self.seed_value(&json!({"version": 0, "leases": leases}));
    }

    pub fn seed_value(&self, doc: &Value) {
        let mut text = serde_json::to_string(doc).expect("A document can be written");
        text.push('\n');
        self.seed_raw(text.as_bytes());
    }

    pub fn seed_raw(&self, bytes: &[u8]) {
        std::fs::create_dir_all(&self.state).expect("The state folder can be made");
        std::fs::write(self.state_file(), bytes).expect("The state file can be written");
    }

    /// The state file's bytes, or `None` when there is none.
    pub fn state_bytes(&self) -> Option<Vec<u8>> {
        std::fs::read(self.state_file()).ok()
    }

    /// Every record in the state file, in order; none when there is no file.
    pub fn records(&self) -> Vec<Value> {
        let Some(bytes) = self.state_bytes() else {
            return Vec::new();
        };
        let doc: Value = serde_json::from_slice(&bytes).expect("The state file is JSON");
        doc["leases"]
            .as_array()
            .expect("The state file has leases")
            .clone()
    }

    pub fn record_of(&self, id: &str) -> Option<Value> {
        self.records().into_iter().find(|r| r["id"] == id)
    }

    /// Runs the program on these arguments.
    pub fn run(&self, line: &[&str]) -> Ran {
        let os: Vec<OsString> = line.iter().map(OsString::from).collect();
        self.run_os(&os)
    }

    pub fn run_os(&self, line: &[OsString]) -> Ran {
        self.now_calls.store(0, Ordering::SeqCst);
        self.draws.store(0, Ordering::SeqCst);
        let vars = self.vars.clone();
        let var = move |k: &str| vars.get(k).cloned();
        let now = || {
            self.now_calls.fetch_add(1, Ordering::SeqCst);
            match &mut *self.clock.lock().unwrap() {
                Clock::Fixed(t) => *t,
                Clock::Ticking(t) => {
                    let v = *t;
                    *t += 1;
                    v
                }
                Clock::ByLock {
                    lock_file,
                    free,
                    held,
                } => {
                    if lock_is_held(lock_file) {
                        *held
                    } else {
                        *free
                    }
                }
            }
        };
        let fresh = || uuid_n(self.next_id.fetch_add(1, Ordering::SeqCst) + 1000);
        let new_id = || {
            let n = self.draws.fetch_add(1, Ordering::SeqCst) + 1;
            match &mut *self.ids.lock().unwrap() {
                IdSource::Counter => fresh(),
                IdSource::Script(q) => q.pop_front().unwrap_or_else(fresh),
                IdSource::Always(id) => {
                    if n > GUARD {
                        fresh()
                    } else {
                        id.clone()
                    }
                }
            }
        };
        let world = World {
            var: &var,
            cwd: self.cwd_known.then_some(self.cwd.as_path()),
            own_exe: &self.exe,
            now: &now,
            new_id: &new_id,
            drivers: &self.drivers,
        };
        let out = execute_in(line, &world);
        Ran {
            code: out.exit_code,
            stdout: out.stdout,
            stderr: out.stderr,
            now_calls: self.now_calls.load(Ordering::SeqCst),
            draws: self.draws.load(Ordering::SeqCst),
            root: self.root.clone(),
        }
    }
}

impl Default for Lab {
    fn default() -> Lab {
        Lab::new()
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// ---- What a run printed ------------------------------------------------------------------------

pub struct Ran {
    pub code: u8,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// How many times the run read `World::now`.
    pub now_calls: usize,
    /// How many times the run called `World::new_id`.
    pub draws: usize,
    root: PathBuf,
}

/// The problems a document has against one of the schemas under `schemas/`.
pub fn schema_problems(schema_file: &str, doc: &Value) -> Vec<String> {
    let path = repo_root().join("schemas").join(schema_file);
    let text = std::fs::read_to_string(&path).expect("The schema can be read");
    let schema: Value = serde_json::from_str(&text).expect("The schema is JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("The schema can be built");
    validator
        .iter_errors(doc)
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
}

/// True when the message has what an `internal` message MUST NOT have: an absolute path, the
/// throwaway folder, or the user's name (lease-api.md, Exit codes and errors).
pub fn leak_problems(message: &str, root: &Path) -> Vec<String> {
    let mut p = Vec::new();
    let root_text = root.to_string_lossy();
    if message.contains(root_text.as_ref()) {
        p.push("The message holds the throwaway folder's path".to_string());
    }
    if let Some(name) = root.file_name().and_then(|n| n.to_str())
        && message.contains(name)
    {
        p.push("The message holds the throwaway folder's name".to_string());
    }
    for token in message.split_whitespace() {
        let t = token.trim_matches(|c: char| c == '\'' || c == '"' || c == '`' || c == '(');
        let bytes = t.as_bytes();
        let drive = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/');
        if drive || t.starts_with('/') || t.starts_with("\\\\") {
            p.push(format!("The message holds an absolute path: {t}"));
        }
    }
    if let Ok(user) = std::env::var("USERNAME")
        && user.chars().count() >= 4
        && message.to_lowercase().contains(&user.to_lowercase())
    {
        p.push("The message holds the user's name".to_string());
    }
    p
}

impl Ran {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    fn as_run(&self) -> Run {
        Run {
            status: Some(i32::from(self.code)),
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
        }
    }

    /// Checks that the run failed as lease-api.md says it MUST (exit code of the error code, nothing
    /// on stdout, one compact valid error document on stderr, `field` as given, the message when
    /// one is given), and returns the error document. An `internal` message is also checked for
    /// paths and names.
    #[track_caller]
    pub fn expect_error(&self, code: &str, field: Option<&str>, message: Option<&str>) -> Value {
        let mut problems = failure_problems(&self.as_run(), code, field, message);
        let doc: Value =
            serde_json::from_slice(self.stderr.trim_ascii_end()).unwrap_or(Value::Null);
        if let Some(m) = doc["message"].as_str() {
            problems.extend(leak_problems(m, &self.root));
        }
        assert!(
            problems.is_empty(),
            "Expected {code}: {}\nExit {}, stdout {:?}, stderr {:?}",
            problems.join("; "),
            self.code,
            self.stdout_text(),
            self.stderr_text()
        );
        doc
    }

    /// Like `expect_error`, for an `invalid_state` or a `not_found`, with the members they carry.
    #[track_caller]
    pub fn expect_lease_error(
        &self,
        code: &str,
        lease_id: &str,
        state: Option<&str>,
        message: &str,
    ) {
        let doc = self.expect_error(code, None, Some(message));
        assert_eq!(doc["lease_id"], lease_id, "The error's lease_id");
        match state {
            Some(s) => assert_eq!(doc["state"], s, "The error's state"),
            None => assert!(doc.get("state").is_none(), "No state on {code}"),
        }
    }

    /// The `internal` error's message, with the exit code checked.
    #[track_caller]
    pub fn internal(&self) -> String {
        let doc = self.expect_error("internal", None, None);
        doc["message"].as_str().unwrap().to_string()
    }

    /// Checks that the run succeeded as lease-api.md says it MUST (exit 0, nothing on stderr, one
    /// compact JSON document and one line feed on stdout), and returns the document.
    #[track_caller]
    pub fn success(&self) -> Value {
        assert_eq!(
            self.code,
            0,
            "Exit {}, stdout {:?}, stderr {:?}",
            self.code,
            self.stdout_text(),
            self.stderr_text()
        );
        assert!(self.stderr.is_empty(), "Stderr: {:?}", self.stderr_text());
        let text = std::str::from_utf8(&self.stdout).expect("Stdout is UTF-8");
        assert!(
            !text.starts_with('\u{feff}'),
            "Stdout has a byte order mark"
        );
        assert!(
            text.ends_with('\n') && text.matches('\n').count() == 1 && !text.contains('\r'),
            "Stdout is not one line ending in one line feed: {text:?}"
        );
        let body = text.trim_end_matches('\n');
        assert!(is_compact(body), "Stdout is not compact: {body:?}");
        serde_json::from_str(body).expect("Stdout is one JSON document")
    }

    /// A successful run's lease document, checked against `lease.output.schema.json`.
    #[track_caller]
    pub fn lease_doc(&self) -> Value {
        let doc = self.success();
        let p = schema_problems("lease.output.schema.json", &doc);
        assert!(
            p.is_empty(),
            "The lease document breaks its schema: {p:?}\n{doc}"
        );
        doc
    }

    /// A successful run's list document, checked against `lease-list.output.schema.json`.
    #[track_caller]
    pub fn list_doc(&self) -> Value {
        let doc = self.success();
        let p = schema_problems("lease-list.output.schema.json", &doc);
        assert!(
            p.is_empty(),
            "The list document breaks its schema: {p:?}\n{doc}"
        );
        doc
    }

    /// The ids of a list document's leases, in order.
    pub fn list_ids(&self) -> Vec<String> {
        self.list_doc()["leases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["id"].as_str().unwrap().to_string())
            .collect()
    }
}

// ---- Windows fixtures --------------------------------------------------------------------------

/// A deny rule on a folder that stops any file being made in it, lifted when it is dropped, even
/// when the test fails. It makes a record write fail after the folder, the lock and the state file
/// exist: the state file's temporary file cannot be made.
#[cfg(windows)]
pub struct DenyWrite {
    dir: PathBuf,
}

#[cfg(windows)]
impl DenyWrite {
    #[track_caller]
    pub fn new(dir: &Path) -> DenyWrite {
        let guard = DenyWrite {
            dir: dir.to_path_buf(),
        };
        let out = std::process::Command::new("icacls")
            .arg(dir)
            .arg("/deny")
            .arg("*S-1-1-0:(WD)")
            .output()
            .expect("icacls runs");
        assert!(out.status.success(), "icacls did not set the deny rule");
        let probe = dir.join("deny-probe");
        let made = std::fs::File::create(&probe).is_ok();
        let _ = std::fs::remove_file(&probe);
        assert!(!made, "The deny rule did not stop a file being made");
        guard
    }
}

#[cfg(windows)]
impl Drop for DenyWrite {
    fn drop(&mut self) {
        let _ = std::process::Command::new("icacls")
            .arg(&self.dir)
            .arg("/remove:d")
            .arg("*S-1-1-0")
            .output();
    }
}

/// Holds the lease state lock the way state.md says every implementation holds it: `leases.lock`
/// open for reading and writing with share mode 0. Released when dropped.
#[cfg(windows)]
pub struct LockHold {
    _file: std::fs::File,
}

#[cfg(windows)]
impl LockHold {
    #[track_caller]
    pub fn take(lab: &Lab) -> LockHold {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::create_dir_all(&lab.state).expect("The state folder can be made");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(lab.lock_file())
            .expect("The test can take the lock");
        LockHold { _file: file }
    }
}

/// `held` when `leases.lock` cannot be opened because another handle holds it without sharing,
/// `free` when it can be opened, and the error otherwise.
#[cfg(windows)]
fn lock_state(lock_file: &Path) -> String {
    use std::os::windows::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(lock_file)
    {
        Ok(_) => "free".to_string(),
        Err(e) if matches!(e.raw_os_error(), Some(32 | 33)) => "held".to_string(),
        Err(e) => format!("error {e}"),
    }
}

/// Whether `leases.lock` is held by another handle right now. Off Windows it is never seen held.
fn lock_is_held(lock_file: &Path) -> bool {
    #[cfg(windows)]
    {
        lock_state(lock_file) == "held"
    }
    #[cfg(not(windows))]
    {
        let _ = lock_file;
        false
    }
}

/// A probe that says `held` when `leases.lock` cannot be opened because another handle holds it
/// without sharing, and `free` when it can be opened.
#[cfg(windows)]
pub fn lock_probe(lock_file: PathBuf) -> crate::fake_driver::Probe {
    std::sync::Arc::new(move |_| lock_state(&lock_file))
}

// ---- Messages, as lease-api.md's Messages table words them -------------------------------------

pub const ID1: &str = "3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f";
pub const ID2: &str = "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d";
pub const ID3: &str = "1b4e28ba-2fa1-4d3b-a3fc-1bc3d9e2b7a0";
pub const ID4: &str = "6ba7b810-9dad-41d1-80b4-00c04fd430c8";

pub fn msg_bad_id(v: &str) -> String {
    format!("Lease ID '{v}' is not a UUIDv4. Give the id that create printed.")
}

pub fn msg_not_found(id: &str) -> String {
    format!("No lease has ID '{id}'.")
}

pub fn msg_bad_project(v: &str) -> String {
    format!(
        "Project '{v}' is not a project name. Use 1 to 32 letters, digits and '-', starting and ending with a letter or digit."
    )
}

pub fn msg_bad_platform(v: &str) -> String {
    format!("Platform '{v}' is not an os/arch pair. Give one as in 'linux/arm64'.")
}

pub fn msg_bad_profile(v: &str) -> String {
    format!(
        "Profile '{v}' is not a profile name. Give name or name@version, as in 'agent-linux'; a version is 1 or more."
    )
}

pub fn msg_bad_size(v: &str) -> String {
    format!("Size '{v}' is not a size. Use 1 to 8 letters and digits, as in 'm'.")
}

pub fn msg_ttl_no_unit(v: &str) -> String {
    format!("TTL '{v}' has no unit. Give one, as in '8h'.")
}

pub fn msg_ttl_m(v: &str) -> String {
    format!(
        "TTL '{v}' uses 'M', which could mean months or minutes. Write minutes as 'm' or 'min'."
    )
}

pub fn msg_ttl_not_duration(v: &str) -> String {
    format!(
        "TTL '{v}' is not a duration. Give one as in '8h', '90m', '1h30m', '2 hours' or 'PT8H'."
    )
}

pub fn msg_ttl_range(v: &str) -> String {
    format!("TTL '{v}' is outside the allowed range. Give from 1m to 24h; it is never clamped.")
}

pub fn msg_bad_state(name: &str) -> String {
    format!(
        "State '{name}' is not a state. Use pending, ready, expired, released or unsatisfiable."
    )
}

pub const MSG_NOT_VALID: &str =
    "The lease records could not be read: the state file is not valid. Nothing was changed.";

pub const MSG_UNKNOWN_DRIVER_TAIL: &str =
    "The lease was placed by a driver this agentspace does not have. Nothing was changed.";

pub fn msg_unknown_driver(sub: &str) -> String {
    format!("agentspace lease {sub} failed: {MSG_UNKNOWN_DRIVER_TAIL}")
}

pub fn msg_no_home(sub: &str) -> String {
    format!(
        "agentspace lease {sub} failed: The home folder is not set (USERPROFILE is unset or empty), so agentspace cannot find its files. Nothing was changed."
    )
}

pub const MSG_NO_DRIVER: &str =
    "agentspace lease create failed: No lease driver is available. Nothing was recorded.";

/// Renew's advice for a state that is not `ready` (lease-api.md, Messages).
pub fn msg_not_ready(id: &str, state: &str) -> String {
    let advice = match state {
        "expired" => "Create a new lease.".to_string(),
        "pending" => "Wait for it to be ready.".to_string(),
        other => format!("The {other} state is final."),
    };
    format!("Lease '{id}' is {state}, and only a ready lease can be renewed. {advice}")
}

/// How many times `needle` appears in `hay`.
pub fn count_of(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

/// A probe that returns the state file's text, or `missing`.
pub fn state_probe(state_file: PathBuf) -> crate::fake_driver::Probe {
    std::sync::Arc::new(move |_| {
        std::fs::read_to_string(&state_file).unwrap_or_else(|_| "missing".to_string())
    })
}

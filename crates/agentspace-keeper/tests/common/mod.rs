//! Shared fakes, fixtures and adapters for the keeper's front and pass (E0.06-T01-S06).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/keeper.md, docs/state.md and the pinned `agentspace-keeper-api`, never from the
//! crate's code (D31, D42). Fixtures only ever name made-up values. Every test works in a
//! throwaway folder under the temp folder, and no test reads the real environment: the library
//! tests hand the pass its variables, and the binary tests point USERPROFILE, HOME, LOCALAPPDATA
//! and APPDATA at a throwaway home.
//!
//! **The adapters.** Every call into the crate's own API beyond the pin is gathered in the section
//! "The adapters" below, and nowhere else. The dev lead adapts that section, and only that
//! section, to the implementation. The API it uses, as the crate has it:
//!
//! - `agentspace_keeper::Records`, the trait of the holder record's file (architect,
//!   2026-11-19, D42): `read(&self, &Path) -> Option<HolderRecord>`, `write(&self, &Path,
//!   &HolderRecord) -> Result<(), String>`, `remove(&self, &Path) -> Result<(), String>`. The
//!   `&Path` is the state folder.
//! - `agentspace_keeper::Deps { processes, matcher, starter, locks, clock, records }`, each a
//!   `&dyn` of the matching trait.
//! - `agentspace_keeper::Env { var, cwd, own_exe }`: `var` looks up an environment variable,
//!   `cwd` is the working folder, `own_exe` is the keeper's own program's path.
//! - `agentspace_keeper::run(args: &[String], env: &Env, deps: &Deps, stdout, stderr) -> i32`:
//!   the whole run of a one-pass keeper, command line included, writing only to the two streams
//!   it is given, and returning the exit code.
//!
//! The alias refusal of keeper.md's "Before the start" is not public API (architect, 2026-11-19,
//! for E0.06-T01-S06): no schema-valid lease state can reach it, so it is tested by a unit test
//! inside the crate (`src/pass.rs`), and has no adapter here.

#![allow(dead_code)]

mod fake_guard;
mod guard;

pub use fake_guard::*;
pub use guard::*;

use agentspace_keeper_api::{
    Clock, HolderRecord, Locks, Matcher, ProcessInfo, Processes, Query, StartRequest, Started,
    Starter, Stopped, Taken,
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

/// The pass time most tests give with `-Now`.
pub const NOW: &str = "2026-11-19T12:00:00Z";
/// An expiry an hour after `NOW`.
pub const LATER: &str = "2026-11-19T13:00:00Z";
/// An expiry two hours after `NOW`.
pub const LATEST: &str = "2026-11-19T14:00:00Z";

pub const ID_A: &str = "3f2a9c1e-1111-4222-8333-444455556666";
pub const ID_B: &str = "9b1d0c77-2222-4333-9444-555566667777";
pub const ID_C: &str = "0a0a0a0a-3333-4444-a555-666677778888";
pub const ID_D: &str = "0a0a0a0b-3333-4444-a555-666677778888";

/// The alias a lease of project `demo` has.
pub fn alias_of(id: &str) -> String {
    format!("agentspace-demo-{}", &id[..8])
}

fn lease_base(id: &str, state: &str) -> Value {
    json!({
        "id": id,
        "project": "demo",
        "state": state,
        "platform": "linux/arm64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": 3600,
        "created_at": "2026-11-19T10:00:00Z",
        "driver": "ec2"
    })
}

/// A stored `ready` lease that expires at `expires_at`.
pub fn ready(id: &str, expires_at: &str) -> Value {
    let mut v = lease_base(id, "ready");
    let o = v.as_object_mut().unwrap();
    o.insert("expires_at".into(), json!(expires_at));
    o.insert(
        "ssh".into(),
        json!({"host_alias": alias_of(id), "user": "ubuntu"}),
    );
    v
}

/// A stored `ready` lease with another alias than the one its ID gives.
pub fn ready_with_alias(id: &str, expires_at: &str, alias: &str) -> Value {
    let mut v = ready(id, expires_at);
    v["ssh"]["host_alias"] = json!(alias);
    v
}

pub fn pending(id: &str) -> Value {
    lease_base(id, "pending")
}

/// A lease released after it was ready; it keeps its `expires_at`.
pub fn released(id: &str, expires_at: &str) -> Value {
    let mut v = lease_base(id, "released");
    let o = v.as_object_mut().unwrap();
    o.insert("expires_at".into(), json!(expires_at));
    o.insert("released_at".into(), json!("2026-11-19T10:30:00Z"));
    v
}

pub fn unsatisfiable(id: &str) -> Value {
    let mut v = lease_base(id, "unsatisfiable");
    v["platform"] = json!("windows/amd64");
    v["reasons"] = json!([{"code": "unsupported_platform",
        "message": "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only."}]);
    v
}

pub fn file_bytes(records: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({"version": 0, "leases": records})).unwrap()
}

/// A holder with a made-up PID and start time, for the lease `id` and its own alias.
pub fn holder_for(pid: u32, id: &str) -> HolderRecord {
    HolderRecord {
        pid,
        process_started_at: "2026-11-19T11:30:00.123Z".to_string(),
        lease_id: id.to_string(),
        alias: alias_of(id),
    }
}

/// One log line as the keeper writes it, LF included.
///
/// A line that names a holder carries ` host=swimcoach` after `pid`, the box's project, and before
/// `reason` (keeper.md, Log lines: A pass's line; E0.07-T03-S05): `started`, `kept`, `stopped`
/// and `lost`, and a `failed` that names a lease, which is a start or a stop. Every other line of
/// the box reads exactly as v0.06's: `idle`, `rotated`, `skipped`, and a `failed` with `lease=-`.
pub fn line(
    time: &str,
    action: &str,
    lease: &str,
    alias: &str,
    pid: &str,
    reason: Option<&str>,
) -> String {
    let names_a_holder = matches!(action, "started" | "kept" | "stopped" | "lost")
        || (action == "failed" && lease != "-");
    let host = if names_a_holder {
        " host=swimcoach"
    } else {
        ""
    };
    match reason {
        Some(r) => {
            format!("{time} {action} lease={lease} alias={alias} pid={pid}{host} reason={r}\n")
        }
        None => format!("{time} {action} lease={lease} alias={alias} pid={pid}{host}\n"),
    }
}

/// The `idle` line.
pub fn idle_line(time: &str) -> String {
    line(time, "idle", "-", "-", "-", None)
}

/// The `failed` line of a pass's own error.
pub fn failed_line(time: &str, reason: &str) -> String {
    line(time, "failed", "-", "-", "-", Some(reason))
}

/// Milliseconds since the Unix epoch of a UTC time (proleptic Gregorian, civil-from-days).
pub fn ms_of(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    ((days * 24 + h) * 60 + mi) * 60_000 + s * 1000
}

/// `NOW` as milliseconds.
pub fn now_ms() -> i64 {
    ms_of(2026, 11, 19, 12, 0, 0)
}

/// A path that holds the three given parts, whatever the platform's separator.
pub fn join(base: &Path, parts: &[&str]) -> PathBuf {
    let mut p = base.to_path_buf();
    for part in parts {
        p.push(part);
    }
    p
}

// ---------------------------------------------------------------------------------------------
// The fakes of the pinned traits
// ---------------------------------------------------------------------------------------------

/// What the fakes saw, in order, for one run. Each is one short text:
/// `clock`, `lock:pass`, `lock:keeper`, `release:pass`, `self_check`, `store:read`,
/// `store:write`, `store:remove`, `query:<pid>`, `stop:<pid>`, `start:<lease id>`.
pub type Events = Rc<RefCell<Vec<String>>>;

pub struct FakeClock {
    events: Events,
    ms: Cell<i64>,
    mono: Cell<Duration>,
}

impl FakeClock {
    pub fn advance(&self, ms: i64) {
        self.ms.set(self.ms.get() + ms);
    }
    pub fn set_ms(&self, ms: i64) {
        self.ms.set(ms);
    }
}

impl Clock for FakeClock {
    fn utc_now_ms(&self) -> i64 {
        self.events.borrow_mut().push("clock".to_string());
        self.ms.get()
    }
    fn monotonic(&self) -> Duration {
        self.mono.get()
    }
    fn sleep(&self, d: Duration) {
        self.mono.set(self.mono.get() + d);
        self.advance(d.as_millis() as i64);
    }
}

/// How the fake pass lock answers.
#[derive(Clone)]
pub enum PassMode {
    Held,
    Skipped,
    Failed(String),
    Busy(Option<u32>),
}

struct Guard(Events);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.borrow_mut().push("release:pass".to_string());
    }
}

pub struct FakeLocks {
    events: Events,
    clock: Rc<FakeClock>,
    pub pass_mode: RefCell<PassMode>,
    /// How many milliseconds the pass lock "waits" before it answers.
    pub wait_ms: Cell<i64>,
    pub pass_dirs: RefCell<Vec<PathBuf>>,
}

impl Locks for FakeLocks {
    fn pass(&self, state_dir: &Path) -> Taken {
        self.events.borrow_mut().push("lock:pass".to_string());
        self.pass_dirs.borrow_mut().push(state_dir.to_path_buf());
        self.clock.advance(self.wait_ms.get());
        match self.pass_mode.borrow().clone() {
            PassMode::Held => Taken::Held(Box::new(Guard(self.events.clone()))),
            PassMode::Skipped => Taken::Skipped,
            PassMode::Failed(e) => Taken::Failed(e),
            PassMode::Busy(p) => Taken::Busy(p),
        }
    }
    fn keeper(&self, _state_dir: &Path) -> Taken {
        self.events.borrow_mut().push("lock:keeper".to_string());
        Taken::Failed("A one-pass run never takes the keeper lock.".to_string())
    }
}

pub struct FakeMatcher {
    events: Events,
    pub self_check_result: RefCell<Result<(), String>>,
    /// Every `is_holder` call: the command line, the ssh program and the alias.
    pub calls: RefCell<Vec<(String, PathBuf, String)>>,
}

impl FakeMatcher {
    /// The command this fake reads as the holder's: its own `holder_command`.
    pub fn command_for(ssh_exe: &Path, alias: &str) -> String {
        format!(
            "\"{}\" -N -o ExitOnForwardFailure=yes {}",
            ssh_exe.display(),
            alias
        )
    }
}

impl Matcher for FakeMatcher {
    fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String {
        FakeMatcher::command_for(ssh_exe, alias)
    }
    fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool {
        self.calls.borrow_mut().push((
            command_line.to_string(),
            ssh_exe.to_path_buf(),
            alias.to_string(),
        ));
        command_line == FakeMatcher::command_for(ssh_exe, alias)
    }
    fn self_check(&self) -> Result<(), String> {
        self.events.borrow_mut().push("self_check".to_string());
        self.self_check_result.borrow().clone()
    }
}

pub struct FakeProcesses {
    events: Events,
    pub queries: RefCell<HashMap<u32, Query>>,
    pub stop_result: RefCell<Stopped>,
    /// Every stop: the holder named, and the ssh program given.
    pub stops: RefCell<Vec<(HolderRecord, PathBuf)>>,
}

impl Processes for FakeProcesses {
    fn query(&self, pid: u32) -> Query {
        self.events.borrow_mut().push(format!("query:{pid}"));
        self.queries
            .borrow()
            .get(&pid)
            .cloned()
            .unwrap_or(Query::Gone)
    }
    fn stop(&self, holder: &HolderRecord, ssh_exe: &Path, _matcher: &dyn Matcher) -> Stopped {
        self.events
            .borrow_mut()
            .push(format!("stop:{}", holder.pid));
        self.stops
            .borrow_mut()
            .push((holder.clone(), ssh_exe.to_path_buf()));
        self.stop_result.borrow().clone()
    }
}

/// What a start was handed, copied, with what the matcher it was given answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestSeen {
    pub state_dir: PathBuf,
    pub profile: PathBuf,
    pub ssh_exe: PathBuf,
    pub lease_id: String,
    pub alias: String,
    pub own_exe: PathBuf,
    /// `matcher.holder_command(ssh_exe, alias)` on the matcher the pass handed the starter.
    pub matcher_command: String,
}

pub struct FakeStarter {
    events: Events,
    clock: Rc<FakeClock>,
    pub result: RefCell<Started>,
    /// How many milliseconds a start "takes" on the fake clock.
    pub takes_ms: Cell<i64>,
    pub requests: RefCell<Vec<RequestSeen>>,
    /// When set, a start gives a holder with this PID for the lease and alias it was asked for,
    /// whatever `result` says.
    pub echo_pid: Cell<Option<u32>>,
}

impl Starter for FakeStarter {
    fn start(&self, req: &StartRequest, matcher: &dyn Matcher) -> Started {
        self.events
            .borrow_mut()
            .push(format!("start:{}", req.lease_id));
        self.clock.advance(self.takes_ms.get());
        self.requests.borrow_mut().push(RequestSeen {
            state_dir: req.state_dir.to_path_buf(),
            profile: req.profile.to_path_buf(),
            ssh_exe: req.ssh_exe.to_path_buf(),
            lease_id: req.lease_id.to_string(),
            alias: req.alias.to_string(),
            own_exe: req.own_exe.to_path_buf(),
            matcher_command: matcher.holder_command(req.ssh_exe, req.alias),
        });
        if let Some(pid) = self.echo_pid.get() {
            return Started::Holder(HolderRecord {
                pid,
                process_started_at: "2026-11-19T11:59:59.000Z".to_string(),
                lease_id: req.lease_id.to_string(),
                alias: req.alias.to_string(),
            });
        }
        self.result.borrow().clone()
    }
}

/// The fake of the holder record's file (architect, 2026-11-19, D42). It holds the record as
/// `Option<HolderRecord>` with "a record that fails is no record" already applied.
pub struct FakeStore {
    events: Events,
    pub record: RefCell<Option<HolderRecord>>,
    /// When set, `write` fails with this error and the record stays as it was.
    pub write_error: RefCell<Option<String>>,
    /// For each write: whether `board-holder.log` already held a `started` line at that moment.
    pub log_had_started_at_write: RefCell<Vec<bool>>,
    pub writes: RefCell<Vec<HolderRecord>>,
    pub dirs: RefCell<Vec<PathBuf>>,
}

impl FakeStore {
    fn read_record(&self, state_dir: &Path) -> Option<HolderRecord> {
        self.events.borrow_mut().push("store:read".to_string());
        self.dirs.borrow_mut().push(state_dir.to_path_buf());
        self.record.borrow().clone()
    }
    fn write_record(&self, state_dir: &Path, record: &HolderRecord) -> Result<(), String> {
        self.events.borrow_mut().push("store:write".to_string());
        self.dirs.borrow_mut().push(state_dir.to_path_buf());
        let log = std::fs::read_to_string(state_dir.join("board-holder.log")).unwrap_or_default();
        self.log_had_started_at_write
            .borrow_mut()
            .push(log.contains(" started lease="));
        if let Some(e) = self.write_error.borrow().clone() {
            return Err(e);
        }
        self.writes.borrow_mut().push(record.clone());
        *self.record.borrow_mut() = Some(record.clone());
        Ok(())
    }
    fn remove_record(&self, state_dir: &Path) {
        self.events.borrow_mut().push("store:remove".to_string());
        self.dirs.borrow_mut().push(state_dir.to_path_buf());
        *self.record.borrow_mut() = None;
    }
}

pub struct Fakes {
    pub processes: FakeProcesses,
    pub matcher: FakeMatcher,
    pub starter: FakeStarter,
    pub locks: FakeLocks,
    pub clock: Rc<FakeClock>,
    pub store: FakeStore,
    /// The keeper's start of the guard (a keeper's alone), which starts nothing.
    pub guard: FakeGuard,
}

// ---------------------------------------------------------------------------------------------
// The adapters: the only code that names the crate's own API
// ---------------------------------------------------------------------------------------------

impl agentspace_keeper::Records for FakeStore {
    fn read(&self, state_dir: &Path) -> Option<HolderRecord> {
        self.read_record(state_dir)
    }
    fn write(&self, state_dir: &Path, record: &HolderRecord) -> Result<(), String> {
        self.write_record(state_dir, record)
    }
    fn remove(&self, state_dir: &Path) -> Result<(), String> {
        self.remove_record(state_dir);
        Ok(())
    }
}

/// What a run in process gave back.
pub struct Ran {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}

/// The one place that builds the keeper's `Env` and `Deps` and calls `run`. Every in-process test
/// of the keeper, in every world of these tests, goes through it, so a field the crate adds to
/// either is adapted here and nowhere else. The guard's start is the crate's own seam,
/// `agentspace_keeper::GuardStarter` (`Deps::guard`); every world gives it a `FakeGuard`, which
/// starts nothing, and the world of a test of the real start is the built program (`Bin`).
#[allow(clippy::too_many_arguments)]
pub fn keeper_run(
    args: &[String],
    vars: &BTreeMap<String, String>,
    cwd: &Path,
    own_exe: &Path,
    processes: &dyn Processes,
    matcher: &dyn Matcher,
    starter: &dyn Starter,
    locks: &dyn Locks,
    clock: &dyn Clock,
    records: &dyn agentspace_keeper::Records,
    guard: &dyn agentspace_keeper::GuardStarter,
) -> Ran {
    let var = |name: &str| -> Option<OsString> { vars.get(name).map(OsString::from) };
    let env = agentspace_keeper::Env {
        var: &var,
        cwd,
        own_exe,
    };
    let deps = agentspace_keeper::Deps {
        processes,
        matcher,
        starter,
        locks,
        clock,
        records,
        guard,
    };
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit = agentspace_keeper::run(args, &env, &deps, &mut stdout, &mut stderr);
    Ran {
        exit,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    }
}

/// Runs the keeper's whole command line in process, against the fakes and a world of the given
/// variables (nothing is read from the real environment).
pub fn execute_in_process(
    args: &[OsString],
    vars: &BTreeMap<String, String>,
    cwd: &Path,
    own_exe: &Path,
    fakes: &Fakes,
) -> Ran {
    let args: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    keeper_run(
        &args,
        vars,
        cwd,
        own_exe,
        &fakes.processes,
        &fakes.matcher,
        &fakes.starter,
        &fakes.locks,
        &*fakes.clock,
        &fakes.store,
        &fakes.guard,
    )
}

// ---------------------------------------------------------------------------------------------
// The harness for pass tests
// ---------------------------------------------------------------------------------------------

pub struct Harness {
    pub tmp: TempDir,
    pub home: PathBuf,
    pub state: PathBuf,
    /// A file that exists, to give as `-SshExe`.
    pub ssh: PathBuf,
    pub own_exe: PathBuf,
    pub events: Events,
    pub fakes: Fakes,
    pub vars: BTreeMap<String, String>,
}

impl Harness {
    pub fn new() -> Harness {
        let tmp = tempfile::Builder::new()
            .prefix("agentspace-keeper-test-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let root = plain(
            tmp.path()
                .canonicalize()
                .expect("The throwaway folder has a full path"),
        );
        let home = root.join("home");
        let state = root.join("state");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        let ssh = root.join("ssh-stub");
        std::fs::write(&ssh, b"stub").unwrap();
        let own_exe = root.join("bin").join("agentspace-keeper");
        let events: Events = Rc::new(RefCell::new(Vec::new()));
        let clock = Rc::new(FakeClock {
            events: events.clone(),
            ms: Cell::new(now_ms()),
            mono: Cell::new(Duration::ZERO),
        });
        let fakes = Fakes {
            processes: FakeProcesses {
                events: events.clone(),
                queries: RefCell::new(HashMap::new()),
                stop_result: RefCell::new(Stopped::All),
                stops: RefCell::new(Vec::new()),
            },
            matcher: FakeMatcher {
                events: events.clone(),
                self_check_result: RefCell::new(Ok(())),
                calls: RefCell::new(Vec::new()),
            },
            starter: FakeStarter {
                events: events.clone(),
                clock: clock.clone(),
                result: RefCell::new(Started::Failed("The starter was not set up.".to_string())),
                takes_ms: Cell::new(0),
                requests: RefCell::new(Vec::new()),
                echo_pid: Cell::new(None),
            },
            locks: FakeLocks {
                events: events.clone(),
                clock: clock.clone(),
                pass_mode: RefCell::new(PassMode::Held),
                wait_ms: Cell::new(0),
                pass_dirs: RefCell::new(Vec::new()),
            },
            clock,
            store: FakeStore {
                events: events.clone(),
                record: RefCell::new(None),
                write_error: RefCell::new(None),
                log_had_started_at_write: RefCell::new(Vec::new()),
                writes: RefCell::new(Vec::new()),
                dirs: RefCell::new(Vec::new()),
            },
            guard: FakeGuard::new(),
        };
        let mut vars = BTreeMap::new();
        vars.insert(
            "USERPROFILE".to_string(),
            home.to_string_lossy().into_owned(),
        );
        vars.insert("HOME".to_string(), home.to_string_lossy().into_owned());
        vars.insert(
            "LOCALAPPDATA".to_string(),
            home.to_string_lossy().into_owned(),
        );
        vars.insert("APPDATA".to_string(), home.to_string_lossy().into_owned());
        vars.insert(
            "AGENTSPACE_STATE_DIR".to_string(),
            state.to_string_lossy().into_owned(),
        );
        // The real system folder: a `.cmd` stand-in guard is run through its cmd.exe.
        vars.insert(
            "SystemRoot".to_string(),
            system_root().to_string_lossy().into_owned(),
        );
        for (name, path) in guard_vars(&home) {
            vars.insert(name.to_string(), path.to_string_lossy().into_owned());
        }
        Harness {
            tmp,
            home,
            state,
            ssh,
            own_exe,
            events,
            fakes,
            vars,
        }
    }

    /// The pass's whole command line: `-SshExe <the stub> -Now <now>`.
    pub fn pass(&self, now: &str) -> Ran {
        let ssh = self.ssh.to_string_lossy().into_owned();
        self.run(&["-SshExe", &ssh, "-Now", now])
    }

    /// A pass with the pass time from the clock: `-SshExe <the stub>` only.
    pub fn pass_on_clock(&self) -> Ran {
        let ssh = self.ssh.to_string_lossy().into_owned();
        self.run(&["-SshExe", &ssh])
    }

    pub fn run(&self, args: &[&str]) -> Ran {
        let os: Vec<OsString> = args.iter().map(OsString::from).collect();
        execute_in_process(&os, &self.vars, self.tmp.path(), &self.own_exe, &self.fakes)
    }

    pub fn events(&self) -> Vec<String> {
        self.events.borrow().clone()
    }

    pub fn clear_events(&self) {
        self.events.borrow_mut().clear();
    }

    pub fn leases_path(&self) -> PathBuf {
        self.state.join("leases.json")
    }

    pub fn write_leases(&self, records: Vec<Value>) {
        self.write_leases_raw(&file_bytes(records));
    }

    pub fn write_leases_raw(&self, bytes: &[u8]) {
        std::fs::create_dir_all(&self.state).unwrap();
        std::fs::write(self.leases_path(), bytes).unwrap();
    }

    pub fn log_path(&self) -> PathBuf {
        self.state.join("board-holder.log")
    }

    /// The bytes of `board-holder.log`, empty when there is none.
    pub fn log_bytes(&self) -> Vec<u8> {
        std::fs::read(self.log_path()).unwrap_or_default()
    }

    pub fn log(&self) -> String {
        String::from_utf8(self.log_bytes()).expect("The log is UTF-8")
    }

    pub fn ssh_log_path(&self) -> PathBuf {
        self.state.join("board-holder-ssh.log")
    }

    pub fn ssh_log_1_path(&self) -> PathBuf {
        self.state.join("board-holder-ssh.log.1")
    }

    /// Records `rec` as the holder record, and makes its process a trusted one: running, started
    /// when the record says, with the holder command for this harness's `-SshExe` and the
    /// record's alias.
    pub fn trust(&self, rec: &HolderRecord) {
        *self.fakes.store.record.borrow_mut() = Some(rec.clone());
        self.fakes.processes.queries.borrow_mut().insert(
            rec.pid,
            Query::Running(ProcessInfo {
                process_started_at: rec.process_started_at.clone(),
                command_line: Some(FakeMatcher::command_for(&self.ssh, &rec.alias)),
            }),
        );
    }

    /// Records `rec` and makes its process answer `q`.
    pub fn record_with_query(&self, rec: &HolderRecord, q: Query) {
        *self.fakes.store.record.borrow_mut() = Some(rec.clone());
        self.fakes.processes.queries.borrow_mut().insert(rec.pid, q);
    }

    pub fn set_start(&self, s: Started) {
        *self.fakes.starter.result.borrow_mut() = s;
    }

    /// Makes a start give a holder with this PID for whichever lease it is asked for.
    pub fn start_echo(&self, pid: u32) {
        self.fakes.starter.echo_pid.set(Some(pid));
    }

    pub fn set_stop(&self, s: Stopped) {
        *self.fakes.processes.stop_result.borrow_mut() = s;
    }

    pub fn record(&self) -> Option<HolderRecord> {
        self.fakes.store.record.borrow().clone()
    }

    pub fn stops(&self) -> usize {
        self.fakes.processes.stops.borrow().len()
    }

    pub fn starts(&self) -> usize {
        self.fakes.starter.requests.borrow().len()
    }
}

/// The index of the first event that equals `name`.
pub fn pos(events: &[String], name: &str) -> usize {
    events
        .iter()
        .position(|e| e == name)
        .unwrap_or_else(|| panic!("Event {name} did not happen; events were {events:?}"))
}

pub fn has(events: &[String], name: &str) -> bool {
    events.iter().any(|e| e == name)
}

/// Whether any event starts with `prefix`.
pub fn has_prefix(events: &[String], prefix: &str) -> bool {
    events.iter().any(|e| e.starts_with(prefix))
}

/// The pass lock was taken and, whatever happened, released, and released last.
pub fn assert_lock_released_last(events: &[String]) {
    assert!(
        has(events, "lock:pass"),
        "The pass lock was never taken: {events:?}"
    );
    assert_eq!(
        events.last().map(String::as_str),
        Some("release:pass"),
        "The pass lock was not released last: {events:?}"
    );
    assert_eq!(
        events.iter().filter(|e| *e == "release:pass").count(),
        1,
        "The pass lock was not released exactly once: {events:?}"
    );
}

/// `ran.stdout` is exactly the lines this run appended to `board-holder.log` after `before`.
pub fn assert_stdout_is_log_since(h: &Harness, before: &[u8], ran: &Ran) {
    let now = h.log_bytes();
    assert!(
        now.starts_with(before),
        "The log was not appended to, it was rewritten"
    );
    assert_eq!(
        ran.stdout.as_bytes(),
        &now[before.len()..],
        "Every line logged must also be written to stdout, as it is logged"
    );
}

// ---------------------------------------------------------------------------------------------
// The binary
// ---------------------------------------------------------------------------------------------

/// The built binary, `agentspace-keeper` (`agentspace-keeper.exe` on Windows).
pub const BIN: &str = env!("CARGO_BIN_EXE_agentspace-keeper");

/// The usage lines every usage error ends with (keeper.md, Usage errors).
pub const USAGE: &str = "Usage: agentspace-keeper [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]\n       agentspace-keeper -Watch [-SshExe <full path>] [-IntervalSeconds <n>] [-MaxPasses <n>] [-BackoffBaseSeconds <n>] [-BackoffMaxSeconds <n>] [-ReleaseGraceSeconds <n>]\n";

/// The whole stderr of a usage error with this message.
pub fn usage_error(message: &str) -> String {
    format!("agentspace-keeper: {message}\n{USAGE}")
}

pub struct BinRan {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// A run of the binary in a throwaway home.
pub struct Bin {
    pub tmp: TempDir,
    pub home: PathBuf,
    pub state: PathBuf,
    /// A full path that exists, to give as `-SshExe`.
    pub ssh: PathBuf,
    vars: Vec<(String, Option<OsString>)>,
}

impl Bin {
    pub fn new() -> Bin {
        let tmp = tempfile::Builder::new()
            .prefix("agentspace-keeper-bin-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let root = plain(
            tmp.path()
                .canonicalize()
                .expect("The throwaway folder has a full path"),
        );
        let home = root.join("home");
        let state = root.join("state");
        std::fs::create_dir_all(&home).unwrap();
        let ssh = root.join("ssh-stub");
        std::fs::write(&ssh, b"stub").unwrap();
        let mut b = Bin {
            tmp,
            home: home.clone(),
            state: state.clone(),
            ssh,
            vars: Vec::new(),
        };
        for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
            b.set(name, &home);
        }
        b.set("AGENTSPACE_STATE_DIR", &state);
        // The real system folder: a `.cmd` stand-in guard is run through its cmd.exe.
        b.set("SystemRoot", system_root());
        // Every run of the keeper has a stand-in guard and tripwires (AGENTS.md; guard.rs).
        for (name, path) in guard_vars(&b.standin_dir()) {
            b.set(name, path);
        }
        b
    }

    pub fn set(&mut self, name: &str, value: impl Into<OsString>) -> &mut Bin {
        self.vars.retain(|(n, _)| n != name);
        self.vars.push((name.to_string(), Some(value.into())));
        self
    }

    pub fn unset(&mut self, name: &str) -> &mut Bin {
        self.vars.retain(|(n, _)| n != name);
        self.vars.push((name.to_string(), None));
        self
    }

    /// A path under the throwaway root that no test makes, which is full on every platform.
    pub fn full(&self, parts: &[&str]) -> PathBuf {
        join(self.tmp.path(), parts)
    }

    /// The folder of the stand-in guard, the tripwires and their records, beside the home folder,
    /// never in it: a case that reads the home as untouched stays so.
    pub fn standin_dir(&self) -> PathBuf {
        self.home.parent().unwrap().join("standins")
    }

    /// The stand-in guard of this home.
    pub fn standin(&self) -> PathBuf {
        self.standin_dir().join(STANDIN)
    }

    /// Gives a command a test builds itself the variables of this home that name the stand-in
    /// guard and the tripwires, and the real system folder.
    pub fn guard_env(&self, cmd: &mut Command) {
        for (name, path) in guard_var_paths(&self.standin_dir()) {
            cmd.env(name, path);
        }
        cmd.env("SystemRoot", system_root());
    }

    /// A run of the binary in this throwaway home, not yet started: its variables, its working
    /// folder and its piped standard streams.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.args(args)
            .current_dir(&self.home)
            .env_remove("AGENTSPACE_SSH_DIR")
            .env_remove("AGENTSPACE_PROXY_COMMAND")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in &self.vars {
            match value {
                Some(v) => {
                    cmd.env(name, v);
                }
                None => {
                    cmd.env_remove(name);
                }
            }
        }
        cmd
    }

    pub fn run(&self, args: &[&str]) -> BinRan {
        let mut cmd = self.command(args);
        let child = cmd.spawn().expect("The binary starts");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });
        let out = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("The binary ends within a minute")
            .expect("The binary's output can be read");
        BinRan {
            status: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// Whether the run left `board-holder.log` or any other file in the state folder.
    pub fn state_is_empty(&self) -> bool {
        match std::fs::read_dir(&self.state) {
            Ok(mut d) => d.next().is_none(),
            Err(_) => true,
        }
    }

    pub fn log_exists(&self) -> bool {
        self.state.join("board-holder.log").exists()
    }
}

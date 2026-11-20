//! A scripted world for the keeper's `-Watch` tests (E0.06-T02-S05).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/keeper.md (Watching, Fast failures and backoff, Release grace, What a keeper does over
//! time, Locks, Log lines, Times) and the pinned `agentspace-keeper-api`, never from the crate's
//! code. Nothing here reads the real environment: the keeper is handed variables of a throwaway
//! home, and every state folder is a fresh folder under the temp folder.
//!
//! **How a timeline is driven.** One `World` holds every fake of the pinned traits and the whole
//! script. The fake `Clock`'s `sleep` advances its monotonic time and its UTC time together, and
//! records each sleep with the last line `board-holder.log` held at that moment, so a test can say
//! that a `grace seconds=95` line was logged, and that 95 seconds were then waited. Processes live
//! on the monotonic clock: a started or adopted process is given a lifetime, and is gone from
//! `query` once that much monotonic time has passed. A hook runs when the n-th event of a name
//! happens (for example the 2nd `lock:pass`), which is how a holder dies between a look and its
//! pass. Each pass and start takes no time on the clock, so a time in an expected log line is the
//! start time plus the sleeps so far.
//!
//! The events are one short text each: `lock:keeper`, `release:keeper`, `lock:pass`,
//! `release:pass`, `self_check`, `store:read`, `store:write`, `store:remove`, `query:<pid>`,
//! `stop:<pid>`, `start:<lease id>` and `sleep:<seconds>`.

#![allow(dead_code)]

use crate::common::{self, FakeMatcher, PassMode, Ran, ready};
use agentspace_keeper_api::{
    Clock, HolderRecord, Locks, Matcher, ProcessInfo, Processes, Query, StartRequest, Started,
    Starter, Stopped, Taken,
};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tempfile::TempDir;

/// An expiry no timeline reaches.
pub const FAR: &str = "2030-01-01T00:00:00Z";

/// The reason of a start whose holder exited at once, as keeper.md's table gives its start.
pub const EXITED_AT_ONCE: &str =
    "The holder could not be started: The holder exited at once; see board-holder-ssh.log.";

/// `NOW` plus `secs` seconds, in the form of a log line's time.
pub fn at(secs: i64) -> String {
    let total = 12 * 3600 + secs;
    format!(
        "2026-11-19T{:02}:{:02}:{:02}Z",
        total / 3600,
        total % 3600 / 60,
        total % 60
    )
}

pub type Hook = Rc<dyn Fn(&World)>;

/// What a start gives.
pub enum StartPlan {
    /// A holder that lives this long on the monotonic clock (`None`: for ever).
    Holder {
        pid: u32,
        lifetime: Option<Duration>,
    },
    /// A failed start with the whole `reason=` text.
    Fail(String),
}

/// How the fake keeper lock answers.
#[derive(Clone)]
pub enum KeeperMode {
    Held,
    Busy(Option<u32>),
    Failed(String),
}

struct Proc {
    info: ProcessInfo,
    born: Duration,
    lifetime: Option<Duration>,
}

/// One sleep, as the clock saw it.
#[derive(Debug, Clone)]
pub struct SleepRec {
    pub secs: u64,
    pub ms: u128,
    /// The last line of `board-holder.log` when the sleep began, without its LF.
    pub last_line: String,
    /// The monotonic time when the sleep began.
    pub at: Duration,
}

pub struct World {
    pub tmp: TempDir,
    pub home: PathBuf,
    pub state: PathBuf,
    pub ssh: PathBuf,
    pub own_exe: PathBuf,
    vars: BTreeMap<String, String>,
    events: RefCell<Vec<(String, Duration)>>,
    counts: RefCell<HashMap<String, usize>>,
    hooks: RefCell<Vec<(String, usize, Hook)>>,
    ms: Cell<i64>,
    mono: Cell<Duration>,
    sleeps: RefCell<Vec<SleepRec>>,
    procs: RefCell<HashMap<u32, Proc>>,
    overrides: RefCell<HashMap<u32, Query>>,
    pub stop_result: RefCell<Stopped>,
    stops: RefCell<Vec<HolderRecord>>,
    plans: RefCell<VecDeque<StartPlan>>,
    starts: RefCell<Vec<(String, String)>>,
    pub pass_modes: RefCell<VecDeque<PassMode>>,
    pub keeper_mode: RefCell<KeeperMode>,
    record: RefCell<Option<HolderRecord>>,
    pub write_error: RefCell<Option<String>>,
    pub remove_error: RefCell<Option<String>>,
    pub self_check: RefCell<Result<(), String>>,
    born_counter: Cell<u32>,
    /// A message the next read of the UTC clock panics with, once.
    utc_panic: RefCell<Option<String>>,
}

struct Guard {
    w: Rc<World>,
    name: &'static str,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.w.note(self.name);
    }
}

impl World {
    /// A world with one ready lease, `ID_A`, that expires far off.
    pub fn new() -> Rc<World> {
        let tmp = tempfile::Builder::new()
            .prefix("agentspace-keeper-watch-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let root = tmp.path().to_path_buf();
        let home = root.join("home");
        let state = root.join("state");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        let ssh = root.join("ssh-stub");
        std::fs::write(&ssh, b"stub").unwrap();
        let own_exe = root.join("bin").join("agentspace-keeper");
        let mut vars = BTreeMap::new();
        for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
            vars.insert(name.to_string(), home.to_string_lossy().into_owned());
        }
        vars.insert(
            "AGENTSPACE_STATE_DIR".to_string(),
            state.to_string_lossy().into_owned(),
        );
        vars.insert(
            "SystemRoot".to_string(),
            root.join("winroot").to_string_lossy().into_owned(),
        );
        let w = Rc::new(World {
            tmp,
            home,
            state,
            ssh,
            own_exe,
            vars,
            events: RefCell::new(Vec::new()),
            counts: RefCell::new(HashMap::new()),
            hooks: RefCell::new(Vec::new()),
            ms: Cell::new(common::now_ms()),
            mono: Cell::new(Duration::ZERO),
            sleeps: RefCell::new(Vec::new()),
            procs: RefCell::new(HashMap::new()),
            overrides: RefCell::new(HashMap::new()),
            stop_result: RefCell::new(Stopped::All),
            stops: RefCell::new(Vec::new()),
            plans: RefCell::new(VecDeque::new()),
            starts: RefCell::new(Vec::new()),
            pass_modes: RefCell::new(VecDeque::new()),
            keeper_mode: RefCell::new(KeeperMode::Held),
            record: RefCell::new(None),
            write_error: RefCell::new(None),
            remove_error: RefCell::new(None),
            self_check: RefCell::new(Ok(())),
            born_counter: Cell::new(0),
            utc_panic: RefCell::new(None),
        });
        w.set_leases(vec![ready(common::ID_A, FAR)]);
        w
    }

    // -----------------------------------------------------------------------------------------
    // The script
    // -----------------------------------------------------------------------------------------

    pub fn set_leases(&self, records: Vec<Value>) {
        self.set_leases_raw(&common::file_bytes(records));
    }

    pub fn set_leases_raw(&self, bytes: &[u8]) {
        std::fs::create_dir_all(&self.state).unwrap();
        std::fs::write(self.state.join("leases.json"), bytes).unwrap();
    }

    /// Runs `f` when the `nth` (1-based) event called `event` happens.
    pub fn on(&self, event: &str, nth: usize, f: impl Fn(&World) + 'static) {
        self.hooks
            .borrow_mut()
            .push((event.to_string(), nth, Rc::new(f)));
    }

    pub fn plan_holder(&self, pid: u32, lifetime: Option<Duration>) {
        self.plans
            .borrow_mut()
            .push_back(StartPlan::Holder { pid, lifetime });
    }

    pub fn plan_fail(&self, reason: &str) {
        self.plans
            .borrow_mut()
            .push_back(StartPlan::Fail(reason.to_string()));
    }

    /// A holder the keeper did not start: recorded, running, and trusted for `lease`, born now.
    pub fn adopt(&self, pid: u32, lease: &str, lifetime: Option<Duration>) -> HolderRecord {
        let rec = common::holder_for(pid, lease);
        self.add_process(&rec, lifetime);
        *self.record.borrow_mut() = Some(rec.clone());
        rec
    }

    /// Makes the process of `rec` run and be the holder `rec` names, with no record written.
    pub fn add_process(&self, rec: &HolderRecord, lifetime: Option<Duration>) {
        self.procs.borrow_mut().insert(
            rec.pid,
            Proc {
                info: ProcessInfo {
                    process_started_at: rec.process_started_at.clone(),
                    command_line: Some(FakeMatcher::command_for(&self.ssh, &rec.alias)),
                },
                born: self.mono.get(),
                lifetime,
            },
        );
    }

    pub fn set_record(&self, rec: Option<HolderRecord>) {
        *self.record.borrow_mut() = rec;
    }

    pub fn record(&self) -> Option<HolderRecord> {
        self.record.borrow().clone()
    }

    pub fn kill(&self, pid: u32) {
        self.procs.borrow_mut().remove(&pid);
    }

    /// Makes `query(pid)` answer `q` from now on, whatever the process does.
    pub fn answer_query(&self, pid: u32, q: Query) {
        self.overrides.borrow_mut().insert(pid, q);
    }

    pub fn is_alive(&self, pid: u32) -> bool {
        self.procs.borrow().contains_key(&pid)
    }

    pub fn set_stop(&self, s: Stopped) {
        *self.stop_result.borrow_mut() = s;
    }

    /// Steps the UTC clock alone, as a hand-set time or a time sync does.
    pub fn step_utc(&self, ms: i64) {
        self.ms.set(self.ms.get() + ms);
    }

    pub fn set_utc_ms(&self, ms: i64) {
        self.ms.set(ms);
    }

    /// Makes the next read of the UTC clock panic with `message`, once.
    pub fn panic_on_next_utc_read(&self, message: &str) {
        *self.utc_panic.borrow_mut() = Some(message.to_string());
    }

    // -----------------------------------------------------------------------------------------
    // What happened
    // -----------------------------------------------------------------------------------------

    fn note(&self, name: &str) {
        self.events
            .borrow_mut()
            .push((name.to_string(), self.mono.get()));
        let n = {
            let mut c = self.counts.borrow_mut();
            let e = c.entry(name.to_string()).or_insert(0);
            *e += 1;
            *e
        };
        let todo: Vec<Hook> = self
            .hooks
            .borrow()
            .iter()
            .filter(|(e, k, _)| e == name && *k == n)
            .map(|(_, _, h)| h.clone())
            .collect();
        for h in todo {
            h(self);
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .map(|(n, _)| n.clone())
            .collect()
    }

    pub fn count(&self, name: &str) -> usize {
        self.events
            .borrow()
            .iter()
            .filter(|(n, _)| n == name)
            .count()
    }

    pub fn count_prefix(&self, prefix: &str) -> usize {
        self.events
            .borrow()
            .iter()
            .filter(|(n, _)| n.starts_with(prefix))
            .count()
    }

    /// The monotonic time of the `nth` (1-based) event called `name`.
    pub fn mono_of(&self, name: &str, nth: usize) -> Duration {
        self.events
            .borrow()
            .iter()
            .filter(|(n, _)| n == name)
            .nth(nth - 1)
            .unwrap_or_else(|| panic!("Event {name} number {nth} did not happen"))
            .1
    }

    /// The waits of the keeper's loop: every sleep but the log writer's pause between its tries,
    /// exactly 50 ms (keeper.md, Log lines), which a build MAY make on the clock, and is no wait
    /// of the loop. Any other sleep is kept, a wait of 0 seconds or of part of a second included,
    /// so a wait shorter than the interval is never hidden from a test.
    pub fn sleeps(&self) -> Vec<SleepRec> {
        self.sleeps
            .borrow()
            .iter()
            .filter(|s| s.ms != 50)
            .cloned()
            .collect()
    }

    pub fn sleeps_secs(&self) -> Vec<u64> {
        self.sleeps().iter().map(|s| s.secs).collect()
    }

    pub fn stops(&self) -> Vec<HolderRecord> {
        self.stops.borrow().clone()
    }

    /// Every start: the lease and the alias it was asked for.
    pub fn starts(&self) -> Vec<(String, String)> {
        self.starts.borrow().clone()
    }

    pub fn log_path(&self) -> PathBuf {
        self.state.join("board-holder.log")
    }

    pub fn log(&self) -> String {
        std::fs::read(self.log_path())
            .map(|b| String::from_utf8(b).expect("The log is UTF-8"))
            .unwrap_or_default()
    }

    fn last_log_line(&self) -> String {
        self.log().lines().last().unwrap_or_default().to_string()
    }

    /// The indexes, in `names()`, of the passes' lock and release events, paired in order.
    pub fn pass_spans(&self) -> Vec<(usize, Option<usize>)> {
        let names = self.names();
        let mut spans = Vec::new();
        let mut open: Option<usize> = None;
        for (i, n) in names.iter().enumerate() {
            if n == "lock:pass" {
                open = Some(i);
            } else if n == "release:pass"
                && let Some(s) = open.take()
            {
                spans.push((s, Some(i)));
            }
        }
        if let Some(s) = open {
            spans.push((s, None));
        }
        spans
    }

    // -----------------------------------------------------------------------------------------
    // Running
    // -----------------------------------------------------------------------------------------

    /// Runs `-Watch -SshExe <the stub>` with `extra` after them.
    pub fn run(self: &Rc<World>, extra: &[&str]) -> Ran {
        let mut args = vec![
            "-Watch".to_string(),
            "-SshExe".to_string(),
            self.ssh.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        self.run_args(&args)
    }

    pub fn run_args(self: &Rc<World>, args: &[String]) -> Ran {
        let var = |name: &str| -> Option<OsString> { self.vars.get(name).map(OsString::from) };
        let env = agentspace_keeper::Env {
            var: &var,
            cwd: self.tmp.path(),
            own_exe: &self.own_exe,
        };
        let procs = Procs(self.clone());
        let matcher = Match(self.clone());
        let starter = Start(self.clone());
        let locks = Lock(self.clone());
        let clock = Clk(self.clone());
        let records = Store(self.clone());
        let deps = agentspace_keeper::Deps {
            processes: &procs,
            matcher: &matcher,
            starter: &starter,
            locks: &locks,
            clock: &clock,
            records: &records,
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
}

// ---------------------------------------------------------------------------------------------
// The fakes of the pinned traits, each a thin view of the world
// ---------------------------------------------------------------------------------------------

struct Procs(Rc<World>);
struct Match(Rc<World>);
struct Start(Rc<World>);
struct Lock(Rc<World>);
struct Clk(Rc<World>);
struct Store(Rc<World>);

impl Processes for Procs {
    fn query(&self, pid: u32) -> Query {
        let w = &self.0;
        w.note(&format!("query:{pid}"));
        if let Some(q) = w.overrides.borrow().get(&pid) {
            return q.clone();
        }
        let now = w.mono.get();
        match w.procs.borrow().get(&pid) {
            Some(p) if p.lifetime.is_none_or(|l| now < p.born + l) => {
                Query::Running(p.info.clone())
            }
            _ => Query::Gone,
        }
    }

    fn stop(&self, holder: &HolderRecord, _ssh_exe: &Path, _matcher: &dyn Matcher) -> Stopped {
        let w = &self.0;
        w.note(&format!("stop:{}", holder.pid));
        w.stops.borrow_mut().push(holder.clone());
        let r = w.stop_result.borrow().clone();
        if matches!(r, Stopped::All | Stopped::Gone | Stopped::TreeLeft(_)) {
            w.procs.borrow_mut().remove(&holder.pid);
        }
        r
    }
}

impl Matcher for Match {
    fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String {
        FakeMatcher::command_for(ssh_exe, alias)
    }
    fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool {
        command_line == FakeMatcher::command_for(ssh_exe, alias)
    }
    fn self_check(&self) -> Result<(), String> {
        self.0.note("self_check");
        self.0.self_check.borrow().clone()
    }
}

impl Starter for Start {
    fn start(&self, req: &StartRequest, matcher: &dyn Matcher) -> Started {
        let w = &self.0;
        w.note(&format!("start:{}", req.lease_id));
        w.starts
            .borrow_mut()
            .push((req.lease_id.to_string(), req.alias.to_string()));
        let plan = w.plans.borrow_mut().pop_front();
        match plan {
            Some(StartPlan::Holder { pid, lifetime }) => {
                let n = w.born_counter.get() + 1;
                w.born_counter.set(n);
                let started_at = format!("2026-11-19T11:59:{:02}.{:03}Z", n % 60, n % 1000);
                let info = ProcessInfo {
                    process_started_at: started_at.clone(),
                    command_line: Some(matcher.holder_command(req.ssh_exe, req.alias)),
                };
                w.procs.borrow_mut().insert(
                    pid,
                    Proc {
                        info,
                        born: w.mono.get(),
                        lifetime,
                    },
                );
                Started::Holder(HolderRecord {
                    pid,
                    process_started_at: started_at,
                    lease_id: req.lease_id.to_string(),
                    alias: req.alias.to_string(),
                })
            }
            Some(StartPlan::Fail(reason)) => Started::Failed(reason),
            None => Started::Failed("The test's starter has no plan left.".to_string()),
        }
    }
}

impl Locks for Lock {
    fn pass(&self, _state_dir: &Path) -> Taken {
        let w = &self.0;
        w.note("lock:pass");
        let mode = w
            .pass_modes
            .borrow_mut()
            .pop_front()
            .unwrap_or(PassMode::Held);
        match mode {
            PassMode::Held => Taken::Held(Box::new(Guard {
                w: w.clone(),
                name: "release:pass",
            })),
            PassMode::Skipped => Taken::Skipped,
            PassMode::Failed(e) => Taken::Failed(e),
            PassMode::Busy(p) => Taken::Busy(p),
        }
    }

    fn keeper(&self, _state_dir: &Path) -> Taken {
        let w = &self.0;
        w.note("lock:keeper");
        let mode = w.keeper_mode.borrow().clone();
        match mode {
            KeeperMode::Held => Taken::Held(Box::new(Guard {
                w: w.clone(),
                name: "release:keeper",
            })),
            KeeperMode::Busy(p) => Taken::Busy(p),
            KeeperMode::Failed(e) => Taken::Failed(e),
        }
    }
}

impl Clock for Clk {
    fn utc_now_ms(&self) -> i64 {
        let panic_with = self.0.utc_panic.borrow_mut().take();
        if let Some(message) = panic_with {
            panic!("{message}");
        }
        self.0.ms.get()
    }
    fn monotonic(&self) -> Duration {
        self.0.mono.get()
    }
    fn sleep(&self, d: Duration) {
        let w = &self.0;
        w.sleeps.borrow_mut().push(SleepRec {
            secs: d.as_secs(),
            ms: d.as_millis(),
            last_line: w.last_log_line(),
            at: w.mono.get(),
        });
        w.note(&format!("sleep:{}", d.as_secs()));
        w.mono.set(w.mono.get() + d);
        w.ms.set(w.ms.get() + d.as_millis() as i64);
    }
}

impl agentspace_keeper::Records for Store {
    fn read(&self, _state_dir: &Path) -> Option<HolderRecord> {
        self.0.note("store:read");
        self.0.record.borrow().clone()
    }
    fn write(&self, _state_dir: &Path, record: &HolderRecord) -> Result<(), String> {
        self.0.note("store:write");
        if let Some(e) = self.0.write_error.borrow().clone() {
            return Err(e);
        }
        *self.0.record.borrow_mut() = Some(record.clone());
        Ok(())
    }
    fn remove(&self, _state_dir: &Path) -> Result<(), String> {
        self.0.note("store:remove");
        if let Some(e) = self.0.remove_error.borrow().clone() {
            return Err(e);
        }
        *self.0.record.borrow_mut() = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Expected lines
// ---------------------------------------------------------------------------------------------

fn pid_text(pid: u32) -> String {
    pid.to_string()
}

pub fn started(t: i64, lease: &str, pid: u32) -> String {
    common::line(
        &at(t),
        "started",
        lease,
        &common::alias_of(lease),
        &pid_text(pid),
        None,
    )
}

pub fn kept(t: i64, lease: &str, pid: u32) -> String {
    common::line(
        &at(t),
        "kept",
        lease,
        &common::alias_of(lease),
        &pid_text(pid),
        None,
    )
}

pub fn stopped(t: i64, lease: &str, pid: u32) -> String {
    common::line(
        &at(t),
        "stopped",
        lease,
        &common::alias_of(lease),
        &pid_text(pid),
        None,
    )
}

pub fn lost(t: i64, lease: &str, pid: u32) -> String {
    common::line(
        &at(t),
        "lost",
        lease,
        &common::alias_of(lease),
        &pid_text(pid),
        None,
    )
}

pub fn idle(t: i64) -> String {
    common::idle_line(&at(t))
}

pub fn skipped(t: i64) -> String {
    common::line(&at(t), "skipped", "-", "-", "-", Some("locked"))
}

/// A pass's own error: `failed lease=- alias=- pid=- reason=<reason>`.
pub fn failed(t: i64, reason: &str) -> String {
    common::failed_line(&at(t), reason)
}

/// A failed start of `lease` with no holder.
pub fn failed_start(t: i64, lease: &str, reason: &str) -> String {
    common::line(
        &at(t),
        "failed",
        lease,
        &common::alias_of(lease),
        "-",
        Some(reason),
    )
}

pub fn grace(t: i64, secs: u64) -> String {
    format!("{} grace seconds={secs}\n", at(t))
}

pub fn backoff(t: i64, secs: u64) -> String {
    format!("{} backoff seconds={secs}\n", at(t))
}

pub fn another_keeper(t: i64, pid: Option<u32>) -> String {
    match pid {
        Some(p) => format!("{} another keeper is running pid={p}\n", at(t)),
        None => format!("{} another keeper is running pid=-\n", at(t)),
    }
}

pub fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

// ---------------------------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------------------------

/// Stdout is exactly these lines, the log file holds the same, nothing went to stderr, and the
/// keeper's look never touched the record: it is read, written and removed only by a pass, which
/// holds the pass lock (keeper.md, Watching, step 1; Locks).
pub fn assert_lines(w: &World, ran: &Ran, expected: Vec<String>) {
    let want = expected.concat();
    assert_eq!(ran.stdout, want, "Stdout is not the lines expected");
    assert_eq!(w.log(), want, "board-holder.log is not the lines expected");
    assert_eq!(ran.stderr, "", "Nothing is written to stderr");
    assert_record_only_in_passes(w);
}

pub fn assert_record_only_in_passes(w: &World) {
    let mut in_pass = false;
    for n in w.names() {
        match n.as_str() {
            "lock:pass" => in_pass = true,
            "release:pass" => in_pass = false,
            "store:read" | "store:write" | "store:remove" => {
                assert!(in_pass, "The record was used outside a pass: {n}")
            }
            _ => {}
        }
    }
}

/// Every sleep was at least `floor` seconds (keeper.md, Watching: every iteration waits at least
/// `-IntervalSeconds`).
pub fn assert_sleeps_at_least(w: &World, floor: u64) {
    for s in w.sleeps_secs() {
        assert!(
            s >= floor,
            "A wait of {s} seconds is under the floor of {floor}"
        );
    }
}

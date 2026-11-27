//! A scripted world for the keeper's tests of a host per project (E0.07-T03-S05, #0154).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/keeper.md (Hosts, A pass, Watching, Release grace, Log lines, The holder's log,
//! Stopping and starting the keeper, Conformance cases for a host per project), docs/state.md
//! (The host registry, Another host's holder) and the pinned `agentspace-keeper-api`, never from
//! the crate's code (D31, D42). Nothing here reads the real environment: the keeper is handed the
//! variables of a throwaway home, and every folder is a fresh one under the temp folder.
//!
//! **What it uses of the crate.** Only `agentspace_keeper::{run, Env, Deps, FileRecords}`. The
//! holder records are the real files, through `FileRecords`, so that a record's file name is
//! observed, never assumed: a host's record is `board-holder.<project>.json` and the box's is
//! `board-holder.json` (docs/state.md, Another host's holder). The host registry is a real
//! `hosts.json` under the throwaway `USERPROFILE`'s `.agentspace`, which `AGENTSPACE_STATE_DIR`
//! never moves (keeper.md, Hosts). The holders' processes, the matcher, the starter, the locks
//! and the clock are fakes of the pinned traits.
//!
//! **Who a start is for.** A fake start is told apart by its lease's ID, never by a host: a lease
//! has a `project`, an alias `agentspace-<project>-<8 hex digits>`, and the host it is on is the
//! pass's finding. What the fake starter gives is scripted per lease ID, and the host a holder was
//! started on is read from the record file it was written to and from the line `started` carries.
//!
//! **How a timeline is driven**, as in `tests/watch/mod.rs`: the fake clock's `sleep` advances its
//! monotonic and UTC times together, processes live on the monotonic clock, and a hook runs when
//! the n-th event of a name happens (the 3rd `lock:pass`, say), which is how a lease appears, or a
//! record goes, between two passes. Each pass and start takes no time on the clock, so a time in
//! an expected line is the start time plus the sleeps so far.
//!
//! The events are one short text each: `lock:keeper`, `release:keeper`, `lock:pass`,
//! `release:pass`, `self_check`, `query:<pid>`, `stop:<pid>`, `start:<lease id>` and
//! `sleep:<seconds>`.

#![allow(dead_code)]

// The shared harness, for its stand-in guard, its one place that runs the keeper in process, and
// its helpers (AGENTS.md: every check that runs a keeper names a stand-in guard).
#[path = "../common/mod.rs"]
mod common;

use agentspace_keeper::FileRecords;
use agentspace_keeper_api::{
    Clock, HolderRecord, Locks, Matcher, ProcessInfo, Processes, Query, StartRequest, Started,
    Starter, Stopped, Taken,
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tempfile::TempDir;

/// The pass time most tests give with `-Now`.
pub const NOW: &str = "2026-11-19T12:00:00Z";
/// An expiry no timeline reaches.
pub const FAR: &str = "2030-01-01T00:00:00Z";
/// The box's project (D43, D55).
pub const BOX: &str = "swimcoach";
/// The creation time every made-up holder process has.
pub const STARTED_AT: &str = "2026-11-19T11:30:00.123Z";

/// Milliseconds since the Unix epoch of `NOW`.
pub fn now_ms() -> i64 {
    // 2026-11-19T12:00:00Z
    let days: i64 = {
        let (y, mo, d) = (2026i64, 11i64, 19i64);
        let y2 = if mo <= 2 { y - 1 } else { y };
        let era = y2.div_euclid(400);
        let yoe = y2 - era * 400;
        let mp = (mo + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    };
    (days * 24 + 12) * 3_600_000
}

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

pub fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

// ---------------------------------------------------------------------------------------------
// Leases
// ---------------------------------------------------------------------------------------------

/// A ready lease of one project, with an ID made from a small number.
#[derive(Debug, Clone)]
pub struct Lease {
    pub project: String,
    pub id: String,
    pub expires: String,
    /// The lease's normalized platform (`linux/arm64` unless `Lease::on` says otherwise).
    pub platform: String,
}

impl Lease {
    /// The lease `n` of `project`; its ID starts with the 8 hex digits of `n`, so a lower `n` is a
    /// lower ID in ordinal order.
    pub fn new(project: &str, n: u32, expires: &str) -> Lease {
        Lease::on("linux/arm64", project, n, expires)
    }

    /// The lease `n` of `project` on `platform`, a normalized platform (D91). The ID is made as
    /// `Lease::new` makes it, so two leases of one project on two platforms have two IDs when
    /// their `n` differ.
    pub fn on(platform: &str, project: &str, n: u32, expires: &str) -> Lease {
        Lease {
            project: project.to_string(),
            id: format!("{n:08x}-1111-4222-8333-444455556666"),
            expires: expires.to_string(),
            platform: platform.to_string(),
        }
    }

    /// The profile and the SSH user a lease of this platform has (lease-api.md, Profile and
    /// Fields; D93, D95).
    fn profile_and_user(&self) -> (&'static str, &'static str) {
        if self.platform == "windows/amd64" {
            ("agent-windows", "agent")
        } else {
            ("agent-linux", "ubuntu")
        }
    }

    /// The lease's `ssh.host_alias`: `agentspace-<project>-<first 8 digits of its ID>`.
    pub fn alias(&self) -> String {
        format!("agentspace-{}-{}", self.project, &self.id[..8])
    }

    /// The lease as a stored record, in state `ready`.
    pub fn json(&self) -> Value {
        let (profile, user) = self.profile_and_user();
        json!({
            "id": self.id,
            "project": self.project,
            "state": "ready",
            "platform": self.platform,
            "profile": profile,
            "size": "m",
            "ttl_seconds": 3600,
            "created_at": "2026-11-19T10:00:00Z",
            "driver": "ec2",
            "expires_at": self.expires,
            "ssh": {"host_alias": self.alias(), "user": user}
        })
    }

    /// The same lease, released: its record keeps its `expires_at`.
    pub fn released_json(&self) -> Value {
        let (profile, _) = self.profile_and_user();
        json!({
            "id": self.id,
            "project": self.project,
            "state": "released",
            "platform": self.platform,
            "profile": profile,
            "size": "m",
            "ttl_seconds": 3600,
            "created_at": "2026-11-19T10:00:00Z",
            "driver": "ec2",
            "expires_at": self.expires,
            "released_at": "2026-11-19T10:30:00Z"
        })
    }
}

// ---------------------------------------------------------------------------------------------
// The scripted starts
// ---------------------------------------------------------------------------------------------

/// What a start of one lease gives.
#[derive(Clone)]
pub enum Behavior {
    /// A holder with this PID that lives this long on the monotonic clock (`None`: for ever).
    Hold {
        pid: u32,
        lifetime: Option<Duration>,
    },
    /// A failed start, with the whole `reason=` text.
    Fail(String),
}

/// What a start was handed, copied.
#[derive(Debug, Clone)]
pub struct StartSeen {
    pub lease_id: String,
    pub alias: String,
    /// The request's holder log: `None` for the box's `board-holder-ssh.log`, or the full path of
    /// another host's log.
    pub log: Option<PathBuf>,
    /// The holder records that were in the state folder when the start began, by file name.
    pub records_then: Vec<String>,
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
    pub at: Duration,
}

pub use common::Ran;

pub type Hook = Rc<dyn Fn(&World)>;

pub struct World {
    /// The keeper's start of the guard: it starts nothing, and records what it was asked.
    pub guard: common::FakeGuard,
    pub tmp: TempDir,
    /// The throwaway `USERPROFILE`.
    pub home: PathBuf,
    /// The throwaway state folder, which is not `<home>\.agentspace`.
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
    stop_results: RefCell<HashMap<u32, Stopped>>,
    stops: RefCell<Vec<HolderRecord>>,
    queued: RefCell<HashMap<String, VecDeque<Behavior>>>,
    sticky: RefCell<HashMap<String, Behavior>>,
    starts: RefCell<Vec<StartSeen>>,
    auto_pid: Cell<u32>,
    born_counter: Cell<u32>,
    pub self_check: RefCell<Result<(), String>>,
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
    /// A world with no lease, no registry and no record.
    pub fn new() -> Rc<World> {
        let tmp = tempfile::Builder::new()
            .prefix("agentspace-keeper-hosts-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let root = common::plain(
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
        let mut vars = BTreeMap::new();
        for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
            vars.insert(name.to_string(), home.to_string_lossy().into_owned());
        }
        vars.insert(
            "AGENTSPACE_STATE_DIR".to_string(),
            state.to_string_lossy().into_owned(),
        );
        // The real system folder: a `.cmd` stand-in guard is run through its cmd.exe.
        vars.insert(
            "SystemRoot".to_string(),
            common::system_root().to_string_lossy().into_owned(),
        );
        // Every keeper of these tests has a stand-in guard and tripwires (AGENTS.md).
        for (name, path) in common::guard_vars(&home) {
            vars.insert(name.to_string(), path.to_string_lossy().into_owned());
        }
        Rc::new(World {
            guard: common::FakeGuard::new(),
            tmp,
            home,
            state,
            ssh,
            own_exe,
            vars,
            events: RefCell::new(Vec::new()),
            counts: RefCell::new(HashMap::new()),
            hooks: RefCell::new(Vec::new()),
            ms: Cell::new(now_ms()),
            mono: Cell::new(Duration::ZERO),
            sleeps: RefCell::new(Vec::new()),
            procs: RefCell::new(HashMap::new()),
            stop_results: RefCell::new(HashMap::new()),
            stops: RefCell::new(Vec::new()),
            queued: RefCell::new(HashMap::new()),
            sticky: RefCell::new(HashMap::new()),
            starts: RefCell::new(Vec::new()),
            auto_pid: Cell::new(9000),
            born_counter: Cell::new(0),
            self_check: RefCell::new(Ok(())),
        })
    }

    // -----------------------------------------------------------------------------------------
    // Files: the lease state, the host registry, the records, the logs
    // -----------------------------------------------------------------------------------------

    /// Writes the lease state file with these ready leases.
    pub fn set_leases(&self, leases: &[Lease]) {
        self.set_lease_records(leases.iter().map(Lease::json).collect());
    }

    pub fn set_lease_records(&self, records: Vec<Value>) {
        let bytes = serde_json::to_vec(&json!({"version": 0, "leases": records})).unwrap();
        std::fs::create_dir_all(&self.state).unwrap();
        std::fs::write(self.state.join("leases.json"), bytes).unwrap();
    }

    /// `<home>\.agentspace`, where the host registry is, whatever `AGENTSPACE_STATE_DIR` says.
    pub fn agentspace_dir(&self) -> PathBuf {
        self.home.join(".agentspace")
    }

    pub fn registry_path(&self) -> PathBuf {
        self.agentspace_dir().join("hosts.json")
    }

    /// A registry with the box and each of these projects, all `ready`.
    pub fn registry(&self, projects: &[&str]) {
        let states: Vec<(&str, &str)> = projects.iter().map(|p| (*p, "ready")).collect();
        self.registry_states(&states);
    }

    /// A registry with the box (`ready`) and these projects in these states.
    pub fn registry_states(&self, hosts: &[(&str, &str)]) {
        let mut records = vec![json!({"project": BOX, "stack": "agentbox", "state": "ready"})];
        for (project, state) in hosts {
            records.push(json!({
                "project": project,
                "stack": format!("agentspace-host-{project}"),
                "state": state
            }));
        }
        let mut bytes = serde_json::to_vec(&json!({"version": 0, "hosts": records})).unwrap();
        bytes.push(b'\n');
        self.registry_raw(&bytes);
    }

    /// A registry with the box (`ready`) and these hosts, each as `(project, platform, state)`,
    /// in this file order (keeper.md, A host's key and name; state.md, A host's platform and
    /// names). A `linux/arm64` record has no `platform` member and the stack
    /// `agentspace-host-<p>`; a host of any other platform has the member and the stack
    /// `agentspace-<os>-<arch>-host-<p>` (D91).
    pub fn registry_hosts(&self, hosts: &[(&str, &str, &str)]) {
        let mut records = vec![json!({"project": BOX, "stack": "agentbox", "state": "ready"})];
        for (project, platform, state) in hosts {
            if *platform == "linux/arm64" {
                records.push(json!({
                    "project": project,
                    "stack": format!("agentspace-host-{project}"),
                    "state": state
                }));
            } else {
                records.push(json!({
                    "project": project,
                    "platform": platform,
                    "stack": format!(
                        "agentspace-{}-host-{project}",
                        platform.replace('/', "-")
                    ),
                    "state": state
                }));
            }
        }
        let mut bytes = serde_json::to_vec(&json!({"version": 0, "hosts": records})).unwrap();
        bytes.push(b'\n');
        self.registry_raw(&bytes);
    }

    pub fn registry_raw(&self, bytes: &[u8]) {
        std::fs::create_dir_all(self.agentspace_dir()).unwrap();
        std::fs::write(self.registry_path(), bytes).unwrap();
    }

    /// The file name of a host's record: `board-holder.json` for the box (`None`), else
    /// `board-holder.<project>.json` (docs/state.md, Another host's holder).
    pub fn record_name(project: Option<&str>) -> String {
        match project {
            None => "board-holder.json".to_string(),
            Some(p) => format!("board-holder.{p}.json"),
        }
    }

    pub fn record_path(&self, project: Option<&str>) -> PathBuf {
        self.state.join(World::record_name(project))
    }

    /// The bytes of a host's record file, `None` when there is none.
    pub fn record_bytes(&self, project: Option<&str>) -> Option<Vec<u8>> {
        std::fs::read(self.record_path(project)).ok()
    }

    /// A record file as v0.06 wrote it (compact JSON, one LF), through the real reader's rules.
    pub fn record_text(pid: u32, lease: &Lease) -> String {
        format!(
            "{{\"pid\":{pid},\"process_started_at\":\"{STARTED_AT}\",\"lease_id\":\"{}\",\"alias\":\"{}\"}}\n",
            lease.id,
            lease.alias()
        )
    }

    /// A holder the keeper did not start: its record in the host's file (`None`: the box's) and
    /// its process running as the holder that record names, born now.
    pub fn adopt(
        &self,
        project: Option<&str>,
        pid: u32,
        lease: &Lease,
        lifetime: Option<Duration>,
    ) -> HolderRecord {
        let rec = HolderRecord {
            pid,
            process_started_at: STARTED_AT.to_string(),
            lease_id: lease.id.clone(),
            alias: lease.alias(),
        };
        std::fs::write(self.record_path(project), World::record_text(pid, lease)).unwrap();
        self.add_process(&rec, lifetime);
        rec
    }

    /// A record file's text as v0.12 writes it for a host that is not `linux/arm64`: v0.06's
    /// members, then `platform` last (state.md, The holder record).
    pub fn record_text_with_platform(pid: u32, lease: &Lease, platform: &str) -> String {
        format!(
            "{{\"pid\":{pid},\"process_started_at\":\"{STARTED_AT}\",\"lease_id\":\"{}\",\"alias\":\"{}\",\"platform\":\"{platform}\"}}\n",
            lease.id,
            lease.alias()
        )
    }

    /// A holder the keeper did not start, as `adopt` makes one, with `text` as the record file
    /// named for `host`: a host name, where `None` is the box's `board-holder.json`, `Some("p")`
    /// is `board-holder.p.json` and `Some("p.windows-amd64")` is
    /// `board-holder.p.windows-amd64.json`.
    pub fn adopt_text(
        &self,
        host: Option<&str>,
        pid: u32,
        lease: &Lease,
        lifetime: Option<Duration>,
        text: &str,
    ) -> HolderRecord {
        let rec = HolderRecord {
            pid,
            process_started_at: STARTED_AT.to_string(),
            lease_id: lease.id.clone(),
            alias: lease.alias(),
        };
        std::fs::write(self.record_path(host), text).unwrap();
        self.add_process(&rec, lifetime);
        rec
    }

    /// Makes the process of `rec` run, as the holder `rec` names, with no record written.
    pub fn add_process(&self, rec: &HolderRecord, lifetime: Option<Duration>) {
        self.procs.borrow_mut().insert(
            rec.pid,
            Proc {
                info: ProcessInfo {
                    process_started_at: rec.process_started_at.clone(),
                    command_line: Some(command_for(&self.ssh, &rec.alias)),
                },
                born: self.mono.get(),
                lifetime,
            },
        );
    }

    /// The names of the files in the state folder, sorted.
    pub fn state_names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.state)
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// The names of the holder records in the state folder, sorted: `board-holder.json` and
    /// every `board-holder.<project>.json`.
    pub fn record_names(&self) -> Vec<String> {
        self.state_names()
            .into_iter()
            .filter(|n| {
                n.starts_with("board-holder") && n.ends_with(".json") && !n.contains("launch")
            })
            .collect()
    }

    pub fn log_path(&self) -> PathBuf {
        self.state.join("board-holder.log")
    }

    /// The log without the keeper's lines for the guard's start: a case that existed before v0.08
    /// reads a keeper's log line for line and ignores them (keeper.md, Conformance cases for the
    /// guard's start: H1).
    pub fn log(&self) -> String {
        common::without_guard_lines(&self.log_raw())
    }

    pub fn log_raw(&self) -> String {
        std::fs::read(self.log_path())
            .map(|b| String::from_utf8(b).expect("The log is UTF-8"))
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------------------------
    // The script
    // -----------------------------------------------------------------------------------------

    /// Runs `f` when the `nth` (1-based) event called `event` happens.
    pub fn on(&self, event: &str, nth: usize, f: impl Fn(&World) + 'static) {
        self.hooks
            .borrow_mut()
            .push((event.to_string(), nth, Rc::new(f)));
    }

    /// The next start of `lease` gives this.
    pub fn plan(&self, lease: &Lease, behavior: Behavior) {
        self.queued
            .borrow_mut()
            .entry(lease.id.clone())
            .or_default()
            .push_back(behavior);
    }

    /// Every start of `lease` that has no planned behavior left gives this.
    pub fn plan_always(&self, lease: &Lease, behavior: Behavior) {
        self.sticky.borrow_mut().insert(lease.id.clone(), behavior);
    }

    pub fn hold(pid: u32, lifetime: Option<Duration>) -> Behavior {
        Behavior::Hold { pid, lifetime }
    }

    pub fn fail(reason: &str) -> Behavior {
        Behavior::Fail(reason.to_string())
    }

    /// What `stop` gives for the holder with this PID (default: `Stopped::All`).
    pub fn set_stop_for(&self, pid: u32, s: Stopped) {
        self.stop_results.borrow_mut().insert(pid, s);
    }

    pub fn kill(&self, pid: u32) {
        self.procs.borrow_mut().remove(&pid);
    }

    pub fn is_alive(&self, pid: u32) -> bool {
        self.procs.borrow().contains_key(&pid)
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

    /// The waits of the keeper's loop: every sleep but the log writer's pause between its tries,
    /// exactly 50 ms (keeper.md, Log lines).
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

    /// Every stop, in order: the holder it was asked to stop.
    pub fn stops(&self) -> Vec<HolderRecord> {
        self.stops.borrow().clone()
    }

    pub fn stopped_pids(&self) -> Vec<u32> {
        self.stops().iter().map(|h| h.pid).collect()
    }

    pub fn starts(&self) -> Vec<StartSeen> {
        self.starts.borrow().clone()
    }

    /// The leases the starts were for, in order.
    pub fn started_leases(&self) -> Vec<String> {
        self.starts().iter().map(|s| s.lease_id.clone()).collect()
    }

    // -----------------------------------------------------------------------------------------
    // Running
    // -----------------------------------------------------------------------------------------

    /// One pass at the pass time `NOW`: `-SshExe <the stub> -Now <NOW>`.
    pub fn pass(self: &Rc<World>) -> Ran {
        let ssh = self.ssh.to_string_lossy().into_owned();
        self.run_args(&[
            "-SshExe".to_string(),
            ssh,
            "-Now".to_string(),
            NOW.to_string(),
        ])
    }

    /// Runs `-Watch -SshExe <the stub>` with `extra` after them.
    pub fn watch(self: &Rc<World>, extra: &[&str]) -> Ran {
        let mut args = vec![
            "-Watch".to_string(),
            "-SshExe".to_string(),
            self.ssh.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        self.run_args(&args)
    }

    /// Runs the keeper's command line. A keeper's (`-Watch`) gets `-GuardIntervalSeconds 86400`
    /// added, so that the guard's start comes once, at its first iteration, and never again in a
    /// timeline, and the guard's lines are left out of the stdout it gives back (keeper.md,
    /// Conformance cases for the guard's start: H1); `log_raw` has them.
    pub fn run_args(self: &Rc<World>, args: &[String]) -> Ran {
        let mut args = args.to_vec();
        if args.iter().any(|a| a.eq_ignore_ascii_case("-Watch"))
            && !args
                .iter()
                .any(|a| a.eq_ignore_ascii_case("-GuardIntervalSeconds"))
        {
            args.push("-GuardIntervalSeconds".to_string());
            args.push("86400".to_string());
        }
        let procs = Procs(self.clone());
        let matcher = Match(self.clone());
        let starter = Start(self.clone());
        let locks = Lock(self.clone());
        let clock = Clk(self.clone());
        let records = FileRecords::new();
        self.guard.set_names(self.vars.keys());
        let root = self
            .home
            .parent()
            .expect("The home is inside the throwaway folder")
            .to_path_buf();
        let mut ran = common::keeper_run(
            &args,
            &self.vars,
            &root,
            &self.own_exe,
            &procs,
            &matcher,
            &starter,
            &locks,
            &clock,
            &records,
            &self.guard,
        );
        ran.stdout = common::without_guard_lines(&ran.stdout);
        ran
    }
}

/// The holder command the fake matcher reads as a holder's own.
pub fn command_for(ssh_exe: &Path, alias: &str) -> String {
    format!(
        "\"{}\" -N -o ExitOnForwardFailure=yes {}",
        ssh_exe.display(),
        alias
    )
}

// ---------------------------------------------------------------------------------------------
// The fakes of the pinned traits, each a thin view of the world
// ---------------------------------------------------------------------------------------------

struct Procs(Rc<World>);
struct Match(Rc<World>);
struct Start(Rc<World>);
struct Lock(Rc<World>);
struct Clk(Rc<World>);

impl Processes for Procs {
    fn query(&self, pid: u32) -> Query {
        let w = &self.0;
        w.note(&format!("query:{pid}"));
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
        let r = w
            .stop_results
            .borrow()
            .get(&holder.pid)
            .cloned()
            .unwrap_or(Stopped::All);
        if matches!(r, Stopped::All | Stopped::Gone | Stopped::TreeLeft(_)) {
            w.procs.borrow_mut().remove(&holder.pid);
        }
        r
    }
}

impl Matcher for Match {
    fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String {
        command_for(ssh_exe, alias)
    }
    fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool {
        command_line == command_for(ssh_exe, alias)
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
        w.starts.borrow_mut().push(StartSeen {
            lease_id: req.lease_id.to_string(),
            alias: req.alias.to_string(),
            log: req.log.map(Path::to_path_buf),
            records_then: w.record_names(),
        });
        let planned = w
            .queued
            .borrow_mut()
            .get_mut(req.lease_id)
            .and_then(VecDeque::pop_front);
        let behavior = planned
            .or_else(|| w.sticky.borrow().get(req.lease_id).cloned())
            .unwrap_or_else(|| {
                let pid = w.auto_pid.get() + 1;
                w.auto_pid.set(pid);
                Behavior::Hold {
                    pid,
                    lifetime: None,
                }
            });
        match behavior {
            Behavior::Hold { pid, lifetime } => {
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
            Behavior::Fail(reason) => Started::Failed(reason),
        }
    }
}

impl Locks for Lock {
    fn pass(&self, _state_dir: &Path) -> Taken {
        let w = &self.0;
        w.note("lock:pass");
        Taken::Held(Box::new(Guard {
            w: w.clone(),
            name: "release:pass",
        }))
    }

    fn keeper(&self, _state_dir: &Path) -> Taken {
        let w = &self.0;
        w.note("lock:keeper");
        Taken::Held(Box::new(Guard {
            w: w.clone(),
            name: "release:keeper",
        }))
    }
}

impl Clock for Clk {
    fn utc_now_ms(&self) -> i64 {
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
            at: w.mono.get(),
        });
        w.note(&format!("sleep:{}", d.as_secs()));
        w.mono.set(w.mono.get() + d);
        w.ms.set(w.ms.get() + d.as_millis() as i64);
    }
}

// ---------------------------------------------------------------------------------------------
// Expected lines (keeper.md, Log lines)
// ---------------------------------------------------------------------------------------------

/// The ` host=<project>` field a line carries, or nothing.
fn host_field(host: Option<&str>) -> String {
    match host {
        Some(p) => format!(" host={p}"),
        None => String::new(),
    }
}

/// A line that names a holder: `<time> <action> lease=<id> alias=<alias> pid=<pid> host=<project>`.
/// `host` is the host's project, the box's `swimcoach` included.
pub fn holder_line(t: i64, action: &str, host: &str, lease: &Lease, pid: u32) -> String {
    format!(
        "{} {action} lease={} alias={} pid={pid} host={host}\n",
        at(t),
        lease.id,
        lease.alias()
    )
}

pub fn started(t: i64, host: &str, lease: &Lease, pid: u32) -> String {
    holder_line(t, "started", host, lease, pid)
}

pub fn kept(t: i64, host: &str, lease: &Lease, pid: u32) -> String {
    holder_line(t, "kept", host, lease, pid)
}

pub fn stopped(t: i64, host: &str, lease: &Lease, pid: u32) -> String {
    holder_line(t, "stopped", host, lease, pid)
}

pub fn lost(t: i64, host: &str, lease: &Lease, pid: u32) -> String {
    holder_line(t, "lost", host, lease, pid)
}

/// A failed start: no PID, the host's own field, then the reason.
pub fn failed_start(t: i64, host: &str, lease: &Lease, reason: &str) -> String {
    format!(
        "{} failed lease={} alias={} pid=- host={host} reason={reason}\n",
        at(t),
        lease.id,
        lease.alias()
    )
}

/// A failed stop of the holder with this PID.
pub fn failed_stop(t: i64, host: &str, lease: &Lease, pid: u32, reason: &str) -> String {
    format!(
        "{} failed lease={} alias={} pid={pid} host={host} reason={reason}\n",
        at(t),
        lease.id,
        lease.alias()
    )
}

/// `idle`: the box's has no host field, another host's has one.
pub fn idle(t: i64, host: Option<&str>) -> String {
    format!("{} idle lease=- alias=- pid=-{}\n", at(t), host_field(host))
}

/// `rotated`: the box's has no host field, another host's has one.
pub fn rotated(t: i64, host: Option<&str>) -> String {
    format!(
        "{} rotated lease=- alias=- pid=-{}\n",
        at(t),
        host_field(host)
    )
}

/// An error of one host's turn: `failed lease=- alias=- pid=- [host=<project> ]reason=<reason>`.
/// The box's turn has no host field (keeper.md, A pass's line).
pub fn failed_turn(t: i64, host: Option<&str>, reason: &str) -> String {
    format!(
        "{} failed lease=- alias=- pid=-{} reason={reason}\n",
        at(t),
        host_field(host)
    )
}

pub fn grace(t: i64, secs: u64, host: Option<&str>) -> String {
    format!("{} grace seconds={secs}{}\n", at(t), host_field(host))
}

pub fn backoff(t: i64, secs: u64, host: Option<&str>) -> String {
    format!("{} backoff seconds={secs}{}\n", at(t), host_field(host))
}

/// The reason of a start whose holder exited at once, as keeper.md's table gives it, naming the
/// log of the host the start was on (`None`: the box's `board-holder-ssh.log`).
pub fn exited_at_once(host: Option<&str>) -> String {
    let log = match host {
        None => "board-holder-ssh.log".to_string(),
        Some(p) => format!("board-holder-ssh.{p}.log"),
    };
    format!("The holder could not be started: The holder exited at once; see {log}.")
}

// ---------------------------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------------------------

/// Stdout is exactly these lines, the log file holds the same, and nothing went to stderr.
pub fn assert_lines(w: &World, ran: &Ran, expected: Vec<String>) {
    let want = expected.concat();
    assert_eq!(ran.stdout, want, "Stdout is not the lines expected");
    assert_eq!(w.log(), want, "board-holder.log is not the lines expected");
    assert_eq!(ran.stderr, "", "Nothing is written to stderr");
}

/// The one line of `board-holder.log` that is not `want`'s prefix check: every line the log has,
/// split on LF, without the empty tail.
pub fn log_lines(w: &World) -> Vec<String> {
    w.log().lines().map(str::to_string).collect()
}

/// The pass lock was taken and, whatever happened, released, and released last.
pub fn assert_pass_lock_released(w: &World) {
    let events = w.names();
    let took = events.iter().filter(|e| *e == "lock:pass").count();
    let released = events.iter().filter(|e| *e == "release:pass").count();
    assert_eq!(took, released, "A pass lock was not released: {events:?}");
}

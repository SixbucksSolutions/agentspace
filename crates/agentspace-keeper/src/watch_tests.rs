//! The keeper's loop, walked on a fake clock and fakes for every trait (docs/keeper.md,
//! Watching). The implementer's own tests; the test author's are `tests/spec_watch*.rs`.

use crate::{Deps, Env, Records, run};
use agentspace_keeper_api::{
    Clock, HolderRecord, Locks, Matcher, ProcessInfo, Processes, Query, StartRequest, Started,
    Starter, Stopped, Taken,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

const BASE_SECS: i64 = 1_795_000_000;
const LEASE: &str = "0a0a0a0a-1111-4222-a333-444455556666";
const ALPHA_LEASE: &str = "0b0b0b0b-1111-4222-a333-444455556666";
const ALPHA_ALIAS: &str = "agentspace-alpha-0b0b0b0b";
const ALIAS: &str = "agentspace-demo-0a0a0a0a";

struct FakeClock {
    mono: Cell<Duration>,
    ms: Cell<i64>,
    sleeps: RefCell<Vec<u64>>,
}

impl Clock for FakeClock {
    fn utc_now_ms(&self) -> i64 {
        self.ms.get()
    }
    fn monotonic(&self) -> Duration {
        self.mono.get()
    }
    fn sleep(&self, d: Duration) {
        self.sleeps.borrow_mut().push(d.as_secs());
        self.mono.set(self.mono.get() + d);
        self.ms.set(self.ms.get() + d.as_millis() as i64);
    }
}

struct World {
    clock: Rc<FakeClock>,
    /// A holder's PID and when, on the monotonic clock, it ends; `None` never.
    dies_at: RefCell<HashMap<u32, Option<Duration>>>,
    /// How long each start's holder lives, in the order of the starts; `None` for ever. When the
    /// list runs out, a holder lives for ever.
    lifetimes: RefCell<Vec<Option<u64>>>,
    /// The start's answer, when it is a failure, by the start's number from 1.
    fail_starts: RefCell<Vec<u32>>,
    starts: Cell<u32>,
    record: RefCell<Option<HolderRecord>>,
    /// The record reads from this number (from 1) on find nothing, as when it was removed.
    vanish_from_read: Cell<Option<u32>>,
    reads: Cell<u32>,
    keeper_lock: RefCell<Option<Taken>>,
    pass_locks: Cell<u32>,
    /// A second host, `alpha`, `ready` in the registry with a ready lease of its own, whose every
    /// start fails.
    alpha: Cell<bool>,
    /// Each holder's alias, by PID.
    aliases: RefCell<HashMap<u32, String>>,
}

impl World {
    fn new() -> Rc<World> {
        Rc::new(World {
            clock: Rc::new(FakeClock {
                mono: Cell::new(Duration::from_secs(1000)),
                ms: Cell::new(BASE_SECS * 1000),
                sleeps: RefCell::new(Vec::new()),
            }),
            dies_at: RefCell::new(HashMap::new()),
            lifetimes: RefCell::new(Vec::new()),
            fail_starts: RefCell::new(Vec::new()),
            starts: Cell::new(0),
            record: RefCell::new(None),
            vanish_from_read: Cell::new(None),
            reads: Cell::new(0),
            keeper_lock: RefCell::new(None),
            pass_locks: Cell::new(0),
            alpha: Cell::new(false),
            aliases: RefCell::new(HashMap::new()),
        })
    }
}

struct W(Rc<World>);

fn started_at(pid: u32) -> String {
    format!("2026-11-19T10:00:{:02}.000Z", pid % 60)
}

impl Matcher for W {
    fn holder_command(&self, _: &Path, alias: &str) -> String {
        format!("holder {alias}")
    }
    fn is_holder(&self, command_line: &str, _: &Path, alias: &str) -> bool {
        command_line == format!("holder {alias}")
    }
    fn self_check(&self) -> Result<(), String> {
        Ok(())
    }
}

impl Processes for W {
    fn query(&self, pid: u32) -> Query {
        let alive = match self.0.dies_at.borrow().get(&pid) {
            None => false,
            Some(None) => true,
            Some(Some(t)) => self.0.clock.monotonic() < *t,
        };
        if alive {
            Query::Running(ProcessInfo {
                process_started_at: started_at(pid),
                command_line: Some(format!(
                    "holder {}",
                    self.0
                        .aliases
                        .borrow()
                        .get(&pid)
                        .map_or(ALIAS, String::as_str)
                )),
            })
        } else {
            Query::Gone
        }
    }
    fn stop(&self, holder: &HolderRecord, _: &Path, _: &dyn Matcher) -> Stopped {
        self.0.dies_at.borrow_mut().remove(&holder.pid);
        Stopped::All
    }
}

impl Starter for W {
    fn start(&self, req: &StartRequest, _: &dyn Matcher) -> Started {
        let n = self.0.starts.get() + 1;
        self.0.starts.set(n);
        if self.0.fail_starts.borrow().contains(&n) {
            return Started::Failed("The holder could not be started: It failed.".to_string());
        }
        if req.alias == ALPHA_ALIAS {
            return Started::Failed("The holder could not be started: It failed.".to_string());
        }
        let pid = 100 + n;
        self.0
            .aliases
            .borrow_mut()
            .insert(pid, req.alias.to_string());
        let life = {
            let l = self.0.lifetimes.borrow();
            l.get(n as usize - 1).copied().flatten()
        };
        let dies = life.map(|s| self.0.clock.monotonic() + Duration::from_secs(s));
        self.0.dies_at.borrow_mut().insert(pid, dies);
        Started::Holder(HolderRecord {
            pid,
            process_started_at: started_at(pid),
            lease_id: req.lease_id.to_string(),
            alias: req.alias.to_string(),
        })
    }
}

impl Locks for W {
    fn pass(&self, _: &Path) -> Taken {
        self.0.pass_locks.set(self.0.pass_locks.get() + 1);
        Taken::Held(Box::new(()))
    }
    fn keeper(&self, _: &Path) -> Taken {
        self.0
            .keeper_lock
            .borrow_mut()
            .take()
            .unwrap_or(Taken::Held(Box::new(())))
    }
}

impl Records for W {
    fn read(&self, _: &Path) -> Option<HolderRecord> {
        let n = self.0.reads.get() + 1;
        self.0.reads.set(n);
        if self.0.vanish_from_read.get().is_some_and(|v| n >= v) {
            *self.0.record.borrow_mut() = None;
        }
        self.0.record.borrow().clone()
    }
    fn write(&self, _: &Path, record: &HolderRecord) -> Result<(), String> {
        *self.0.record.borrow_mut() = Some(record.clone());
        Ok(())
    }
    fn remove(&self, _: &Path) -> Result<(), String> {
        *self.0.record.borrow_mut() = None;
        Ok(())
    }
}

struct Ran {
    code: i32,
    /// The lines of the log, without their times.
    lines: Vec<String>,
    sleeps: Vec<u64>,
    stdout: String,
}

/// Runs `agentspace-keeper -Watch` with `extra` arguments in a throwaway home.
fn watch(world: &Rc<World>, extra: &[&str]) -> Ran {
    let tmp = tempfile::Builder::new()
        .prefix("agentspace-keeper-unit-watch-")
        .tempdir()
        .unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let home = root.join("home");
    let state = root.join("state");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let ssh = root.join("ssh-stub");
    std::fs::write(&ssh, b"stub").unwrap();
    let leases = serde_json::json!({"version": 0, "leases": [{
        "id": LEASE, "project": "demo", "state": "ready", "platform": "linux/arm64",
        "profile": "agent-linux", "size": "m", "ttl_seconds": 3600,
        "created_at": "2026-11-19T10:00:00Z", "driver": "ec2",
        "expires_at": "2036-11-19T10:00:00Z",
        "ssh": {"host_alias": ALIAS, "user": "ubuntu"}
    }]});
    let mut leases = leases;
    if world.alpha.get() {
        if let Some(list) = leases["leases"].as_array_mut() {
            list.push(serde_json::json!({
                "id": ALPHA_LEASE, "project": "alpha", "state": "ready",
                "platform": "linux/arm64", "profile": "agent-linux", "size": "m",
                "ttl_seconds": 3600, "created_at": "2026-11-19T10:00:00Z", "driver": "ec2",
                "expires_at": "2036-11-19T10:00:00Z",
                "ssh": {"host_alias": ALPHA_ALIAS, "user": "ubuntu"}
            }));
        }
        std::fs::create_dir_all(home.join(".agentspace")).unwrap();
        std::fs::write(
            home.join(".agentspace").join("hosts.json"),
            r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"},{"project":"alpha","stack":"agentspace-host-alpha","state":"ready"}]}"#,
        )
        .unwrap();
    }
    std::fs::write(state.join("leases.json"), leases.to_string()).unwrap();

    let home_os = home.clone().into_os_string();
    let state_os = state.clone().into_os_string();
    let var = move |name: &str| -> Option<OsString> {
        match name {
            "USERPROFILE" | "HOME" => Some(home_os.clone()),
            "AGENTSPACE_STATE_DIR" => Some(state_os.clone()),
            _ => None,
        }
    };
    let own = root.join("agentspace-keeper");
    let env = Env {
        var: &var,
        cwd: &root,
        own_exe: &own,
    };
    let w = W(world.clone());
    let deps = Deps {
        processes: &w,
        matcher: &w,
        starter: &w,
        locks: &w,
        clock: &*world.clock,
        records: &w,
    };
    let mut args: Vec<String> = vec!["-Watch".into(), "-SshExe".into()];
    args.push(ssh.to_string_lossy().into_owned());
    args.extend(extra.iter().map(|s| s.to_string()));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &env, &deps, &mut out, &mut err);
    assert!(err.is_empty(), "Nothing is written to stderr");
    let log = std::fs::read_to_string(state.join("board-holder.log")).unwrap_or_default();
    Ran {
        code,
        lines: log
            .lines()
            .map(|l| l.split_once(' ').unwrap().1.to_string())
            .collect(),
        sleeps: world.clock.sleeps.borrow().clone(),
        stdout: String::from_utf8(out).unwrap(),
    }
}

fn started(pid: u32) -> String {
    format!("started lease={LEASE} alias={ALIAS} pid={pid} host=swimcoach")
}
fn kept(pid: u32) -> String {
    format!("kept lease={LEASE} alias={ALIAS} pid={pid} host=swimcoach")
}
fn lost(pid: u32) -> String {
    format!("lost lease={LEASE} alias={ALIAS} pid={pid} host=swimcoach")
}

#[test]
fn a_proven_holder_that_dies_is_followed_by_the_grace_and_a_start() {
    let w = World::new();
    // The first holder lives 100 seconds, the second for ever.
    *w.lifetimes.borrow_mut() = vec![Some(100), None];
    let ran = watch(&w, &["-MaxPasses", "7"]);
    assert_eq!(ran.code, 0);
    // Passes at 0, 30, 60 and 90 seconds; the look at 120 finds it gone (proven at 60).
    assert_eq!(
        ran.lines,
        vec![
            started(101),
            kept(101),
            kept(101),
            kept(101),
            "grace seconds=95".to_string(),
            started(102),
            kept(102),
            kept(102),
        ]
    );
    // Six waits between seven passes: 30 x 3, then the grace after the look, then 30, 30.
    assert_eq!(ran.sleeps, vec![30, 30, 30, 30, 95, 30, 30]);
    assert_eq!(ran.stdout.lines().count(), ran.lines.len());
}

#[test]
fn a_holder_that_keeps_dying_unproven_waits_95_95_120_240_300() {
    let w = World::new();
    *w.lifetimes.borrow_mut() = vec![Some(10); 8];
    let ran = watch(&w, &["-MaxPasses", "6"]);
    let waits: Vec<&String> = ran
        .lines
        .iter()
        .filter(|l| l.starts_with("grace") || l.starts_with("backoff"))
        .collect();
    assert_eq!(
        waits,
        vec![
            "grace seconds=95",
            "grace seconds=95",
            "backoff seconds=120",
            "backoff seconds=240",
            "backoff seconds=300"
        ]
    );
}

#[test]
fn a_start_that_keeps_failing_waits_30_60_120_240_300_300() {
    let w = World::new();
    *w.fail_starts.borrow_mut() = (1..=20).collect();
    let ran = watch(&w, &["-MaxPasses", "7"]);
    assert_eq!(ran.sleeps, vec![30, 60, 120, 240, 300, 300]);
    let backoffs = ran
        .lines
        .iter()
        .filter(|l| l.starts_with("backoff"))
        .count();
    assert_eq!(backoffs, 6);
}

#[test]
fn a_holder_that_is_proven_resets_the_count() {
    let w = World::new();
    // Start 1 fails, start 2 lives 200 seconds (proven), start 3 dies at once.
    *w.fail_starts.borrow_mut() = vec![1];
    *w.lifetimes.borrow_mut() = vec![None, Some(200), Some(5), Some(5)];
    let ran = watch(&w, &["-MaxPasses", "14"]);
    // After the proven holder (start 2) died the count was 0, so the next unproven death is the
    // first in a row: the grace, not a longer backoff.
    let after_proven: Vec<&String> = ran
        .lines
        .iter()
        .skip_while(|l| !l.contains("pid=102"))
        .filter(|l| l.starts_with("grace") || l.starts_with("backoff"))
        .collect();
    assert_eq!(after_proven[0], "grace seconds=95");
    assert_eq!(after_proven[1], "grace seconds=95");
}

#[test]
fn a_holder_that_ends_between_the_look_and_the_pass_is_lost() {
    let w = World::new();
    // The record vanishes at the second read, in the second pass, while the process runs.
    w.vanish_from_read.set(Some(2));
    *w.lifetimes.borrow_mut() = vec![None, None];
    let ran = watch(&w, &["-MaxPasses", "3"]);
    assert_eq!(
        ran.lines,
        vec![
            started(101),
            lost(101),
            "grace seconds=95".to_string(),
            started(102),
        ]
    );
    assert_eq!(ran.sleeps, vec![30, 95]);
}

#[test]
fn a_trusted_holder_the_keeper_did_not_start_is_adopted_with_no_grace_later_than_its_end() {
    let w = World::new();
    let adopted = HolderRecord {
        pid: 55,
        process_started_at: started_at(55),
        lease_id: LEASE.to_string(),
        alias: ALIAS.to_string(),
    };
    *w.record.borrow_mut() = Some(adopted);
    w.dies_at
        .borrow_mut()
        .insert(55, Some(Duration::from_secs(1000 + 45)));
    // Two earlier fast failures are not part of a new keeper: its count starts at 0, and an
    // adopted holder is proven, so its end is followed by the grace alone.
    let ran = watch(&w, &["-MaxPasses", "4"]);
    assert_eq!(
        ran.lines,
        vec![
            kept(55),
            kept(55),
            "grace seconds=95".to_string(),
            // The record still names the dead holder: untrusted, removed; no tracked holder, so
            // the pass starts one.
            started(101),
            kept(101),
        ]
    );
}

#[test]
fn the_keeper_lock_refuses_a_second_keeper() {
    let w = World::new();
    *w.keeper_lock.borrow_mut() = Some(Taken::Busy(Some(4242)));
    let ran = watch(&w, &[]);
    assert_eq!(ran.code, 0);
    assert_eq!(ran.lines, vec!["another keeper is running pid=4242"]);
    assert_eq!(w.pass_locks.get(), 0);
    assert!(ran.sleeps.is_empty());

    let w = World::new();
    *w.keeper_lock.borrow_mut() = Some(Taken::Busy(None));
    let ran = watch(&w, &[]);
    assert_eq!(ran.lines, vec!["another keeper is running pid=-"]);
}

#[test]
fn a_keeper_lock_that_fails_ends_with_exit_1_and_no_pass() {
    let w = World::new();
    *w.keeper_lock.borrow_mut() = Some(Taken::Failed("Access is denied.".to_string()));
    let ran = watch(&w, &[]);
    assert_eq!(ran.code, 1);
    assert_eq!(
        ran.lines,
        vec![
            "failed lease=- alias=- pid=- reason=The keeper lock could not be taken: Access is denied."
        ]
    );
    assert_eq!(w.pass_locks.get(), 0);
}

#[test]
fn every_wait_is_at_least_the_interval_and_the_last_pass_is_not_waited_after() {
    let w = World::new();
    *w.lifetimes.borrow_mut() = vec![None];
    let ran = watch(&w, &["-MaxPasses", "3", "-IntervalSeconds", "200"]);
    assert_eq!(ran.sleeps, vec![200, 200]);
}

/// One host's failures back off on that host alone (keeper.md, Fast failures and backoff; D50):
/// alpha's backoff grows with alpha's count, carries `host=alpha`, and holds off only alpha's
/// starts, while the box's holder is kept in a pass every interval.
#[test]
fn one_hosts_backoff_holds_off_only_that_host() {
    let w = World::new();
    w.alpha.set(true);
    *w.lifetimes.borrow_mut() = vec![None];
    let ran = watch(&w, &["-MaxPasses", "4"]);
    assert_eq!(ran.code, 0);
    let alpha_failed = format!(
        "failed lease={ALPHA_LEASE} alias={ALPHA_ALIAS} pid=- host=alpha reason=The holder could not be started: It failed."
    );
    assert_eq!(
        ran.lines,
        vec![
            started(101),
            alpha_failed.clone(),
            "backoff seconds=30 host=alpha".to_string(),
            kept(101),
            alpha_failed.clone(),
            "backoff seconds=60 host=alpha".to_string(),
            // alpha is in its backoff: its turn starts nothing and logs no line.
            kept(101),
            kept(101),
            alpha_failed,
        ]
    );
    // The box has no hold-off, so every wait is the interval.
    assert_eq!(ran.sleeps, vec![30, 30, 30]);
}

//! The pass and the front, against fakes of the pinned traits and the record. Every file is in a
//! throwaway folder under the temp folder, and the environment is the test's own map.

use crate::{Action, Deps, Env, PassContext, Records, run, run_pass};
use agentspace_keeper_api::{
    Clock, HolderRecord, Locks, Matcher, ProcessInfo, Processes, Query, StartRequest, Started,
    Starter, Stopped, Taken,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

// ---------------------------------------------------------------------------------------------
// A throwaway folder.

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!(
            "agentspace-keeper-unit-{}-{}-{nanos}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------------------------
// The fakes: one world that plays every part and records what was asked of it.

const ID_A: &str = "11111111-1111-4111-8111-111111111111";
const ID_B: &str = "22222222-2222-4222-8222-222222222222";
const ID_C: &str = "33333333-3333-4333-8333-333333333333";
const ALIAS_A: &str = "agentspace-demo-11111111";
const ALIAS_B: &str = "agentspace-demo-22222222";
const ALIAS_C: &str = "agentspace-demo-33333333";
const STARTED_AT: &str = "2026-11-18T05:06:59.123Z";
/// 2026-11-18T05:07:00Z.
const NOW: i64 = 1_794_978_420;
const NOW_TEXT: &str = "2026-11-18T05:07:00Z";

#[derive(Clone, Copy, PartialEq)]
enum LockMode {
    Held,
    Skipped,
    Busy,
    Failed,
}

type Calls = Rc<RefCell<Vec<String>>>;

struct Guard(Calls);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.borrow_mut().push("unlock".into());
    }
}

struct World {
    calls: Calls,
    now_ms: Cell<i64>,
    lock: Cell<LockMode>,
    self_check: RefCell<Result<(), String>>,
    record: RefCell<Option<HolderRecord>>,
    write_fails: RefCell<Option<String>>,
    remove_fails: RefCell<Option<String>>,
    processes: RefCell<HashMap<u32, Query>>,
    is_holder: Cell<bool>,
    stop: RefCell<Stopped>,
    start: RefCell<Started>,
    request: RefCell<Option<Request>>,
}

/// What a start was handed: the state folder, the profile, `-SshExe`, the lease, its alias and
/// the keeper's own program.
type Request = (PathBuf, PathBuf, PathBuf, String, String, PathBuf);

fn holder(pid: u32, lease: &str, alias: &str) -> HolderRecord {
    HolderRecord {
        pid,
        process_started_at: STARTED_AT.into(),
        lease_id: lease.into(),
        alias: alias.into(),
    }
}

fn running(started_at: &str) -> Query {
    Query::Running(ProcessInfo {
        process_started_at: started_at.into(),
        command_line: Some("holder command".into()),
    })
}

impl World {
    fn new() -> World {
        World {
            calls: Rc::new(RefCell::new(Vec::new())),
            now_ms: Cell::new(NOW * 1000 + 999),
            lock: Cell::new(LockMode::Held),
            self_check: RefCell::new(Ok(())),
            record: RefCell::new(None),
            write_fails: RefCell::new(None),
            remove_fails: RefCell::new(None),
            processes: RefCell::new(HashMap::new()),
            is_holder: Cell::new(true),
            stop: RefCell::new(Stopped::All),
            start: RefCell::new(Started::Holder(holder(4242, ID_A, ALIAS_A))),
            request: RefCell::new(None),
        }
    }

    fn call(&self, what: impl Into<String>) {
        self.calls.borrow_mut().push(what.into());
    }

    fn called(&self, what: &str) -> bool {
        self.calls.borrow().iter().any(|c| c == what)
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    /// A recorded holder that is running and trusted.
    fn trusted(&self, r: HolderRecord) {
        self.processes
            .borrow_mut()
            .insert(r.pid, running(&r.process_started_at));
        *self.record.borrow_mut() = Some(r);
    }

    fn deps(&self) -> Deps<'_> {
        Deps {
            processes: self,
            matcher: self,
            starter: self,
            locks: self,
            clock: self,
            records: self,
            guard: self,
        }
    }
}

/// The guard's start: it records the try and the start, and starts nothing.
impl crate::GuardStarter for World {
    fn try_lock(&self, _: &Path) -> crate::GuardLockTry {
        self.call("guard lock");
        crate::GuardLockTry::Free
    }
    fn start(
        &self,
        program: &Path,
        args: &[&str],
        _: &Path,
        _: &[(&str, &std::ffi::OsStr)],
    ) -> Result<Option<u32>, String> {
        self.call(format!(
            "guard start {} {}",
            program
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            args.join(" ")
        ));
        Ok(Some(31244))
    }
}

impl Clock for World {
    fn utc_now_ms(&self) -> i64 {
        self.call("now");
        let v = self.now_ms.get();
        // Each reading is a minute later, so a second reading would show in a line.
        self.now_ms.set(v + 60_000);
        v
    }
    fn monotonic(&self) -> Duration {
        Duration::ZERO
    }
    fn sleep(&self, d: Duration) {
        self.call(format!("sleep {}", d.as_millis()));
    }
}

impl Locks for World {
    fn pass(&self, _: &Path) -> Taken {
        self.call("lock");
        match self.lock.get() {
            LockMode::Held => Taken::Held(Box::new(Guard(self.calls.clone()))),
            LockMode::Skipped => Taken::Skipped,
            LockMode::Busy => Taken::Busy(Some(7)),
            LockMode::Failed => Taken::Failed("the lock file is a folder".into()),
        }
    }
    fn keeper(&self, _: &Path) -> Taken {
        self.call("keeper lock");
        Taken::Held(Box::new(Guard(self.calls.clone())))
    }
}

impl Matcher for World {
    fn holder_command(&self, _: &Path, alias: &str) -> String {
        format!("holder {alias}")
    }
    fn is_holder(&self, _: &str, _: &Path, alias: &str) -> bool {
        self.call(format!("is_holder {alias}"));
        self.is_holder.get()
    }
    fn self_check(&self) -> Result<(), String> {
        self.call("self_check");
        self.self_check.borrow().clone()
    }
}

impl Processes for World {
    fn query(&self, pid: u32) -> Query {
        self.call(format!("query {pid}"));
        self.processes
            .borrow()
            .get(&pid)
            .cloned()
            .unwrap_or(Query::Gone)
    }
    fn stop(&self, holder: &HolderRecord, _: &Path, _: &dyn Matcher) -> Stopped {
        self.call(format!("stop {}", holder.pid));
        self.stop.borrow().clone()
    }
}

impl Starter for World {
    fn start(&self, req: &StartRequest, _: &dyn Matcher) -> Started {
        self.call(format!("start {}", req.lease_id));
        *self.request.borrow_mut() = Some((
            req.state_dir.into(),
            req.profile.into(),
            req.ssh_exe.into(),
            req.lease_id.into(),
            req.alias.into(),
            req.own_exe.into(),
        ));
        self.start.borrow().clone()
    }
}

impl Records for World {
    fn read(&self, _: &Path) -> Option<HolderRecord> {
        self.call("read record");
        self.record.borrow().clone()
    }
    fn write(&self, _: &Path, record: &HolderRecord) -> Result<(), String> {
        self.call(format!("write record {}", record.pid));
        if let Some(e) = self.write_fails.borrow().clone() {
            return Err(e);
        }
        *self.record.borrow_mut() = Some(record.clone());
        Ok(())
    }
    fn remove(&self, _: &Path) -> Result<(), String> {
        self.call("remove record");
        if let Some(e) = self.remove_fails.borrow().clone() {
            return Err(e);
        }
        *self.record.borrow_mut() = None;
        Ok(())
    }
}

/// Stdout, which notes each write in the calls so its order against the lock shows.
struct Out {
    bytes: Vec<u8>,
    calls: Calls,
}

impl Write for Out {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.calls.borrow_mut().push("stdout".into());
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The lease state file.

fn lease(id: &str, state: &str, expires_at: Option<&str>, alias: &str) -> String {
    let mut s = format!(
        r#"{{"id":"{id}","project":"demo","state":"{state}","platform":"linux/arm64","profile":"default","size":"small","ttl_seconds":3600,"created_at":"2026-11-18T04:00:00Z""#
    );
    if let Some(t) = expires_at {
        s.push_str(&format!(r#","expires_at":"{t}""#));
    }
    match state {
        "ready" => s.push_str(&format!(
            r#","ssh":{{"host_alias":"{alias}","user":"ubuntu"}}"#
        )),
        "released" => s.push_str(r#","released_at":"2026-11-18T04:30:00Z""#),
        _ => {}
    }
    s.push_str(r#","driver":"ec2"}"#);
    s
}

fn write_leases(state_dir: &Path, leases: &[String]) {
    let doc = format!(r#"{{"version":0,"leases":[{}]}}"#, leases.join(","));
    std::fs::write(state_dir.join("leases.json"), doc).unwrap();
}

/// A folder with a real file to stand for ssh, and a state folder.
struct Place {
    _t: TempDir,
    state: PathBuf,
    profile: PathBuf,
    ssh: PathBuf,
    own: PathBuf,
}

impl Place {
    fn new() -> Place {
        let t = TempDir::new();
        let state = t.path().join("state");
        let profile = t.path().join("home");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let ssh = t.path().join("ssh.exe");
        std::fs::write(&ssh, b"stand-in").unwrap();
        let own = t.path().join("agentspace-keeper.exe");
        Place {
            _t: t,
            state,
            profile,
            ssh,
            own,
        }
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.state.join("board-holder.log")).unwrap_or_default()
    }
}

struct Ran {
    action: Action,
    exit_code: i32,
    stdout: String,
}

fn pass_with(
    world: &World,
    place: &Place,
    now: Option<i64>,
    tracked: Option<&HolderRecord>,
) -> Ran {
    let ctx = PassContext {
        state_dir: &place.state,
        profile: &place.profile,
        ssh_exe: &place.ssh,
        own_exe: &place.own,
        now,
        tracked,
    };
    let mut out = Out {
        bytes: Vec::new(),
        calls: world.calls.clone(),
    };
    let report = run_pass(&ctx, &world.deps(), &mut out);
    Ran {
        action: report.action,
        exit_code: report.exit_code,
        stdout: String::from_utf8(out.bytes).unwrap(),
    }
}

fn pass(world: &World, place: &Place) -> Ran {
    pass_with(world, place, Some(NOW), None)
}

/// ` host=swimcoach` on the box's lines that name a holder, and nothing on its others
/// (keeper.md, A pass's line).
fn box_host(lease: &str) -> &'static str {
    if lease == "-" { "" } else { " host=swimcoach" }
}

fn line(action: &str, lease: &str, alias: &str, pid: &str) -> String {
    let host = box_host(lease);
    format!("{NOW_TEXT} {action} lease={lease} alias={alias} pid={pid}{host}\n")
}

fn failed(lease: &str, alias: &str, pid: &str, reason: &str) -> String {
    let host = box_host(lease);
    format!("{NOW_TEXT} failed lease={lease} alias={alias} pid={pid}{host} reason={reason}\n")
}

// ---------------------------------------------------------------------------------------------
// The order of a pass.

#[test]
fn a_skipped_pass_looks_at_nothing_and_exits_0() {
    for mode in [LockMode::Skipped, LockMode::Busy] {
        let (w, p) = (World::new(), Place::new());
        w.lock.set(mode);
        w.trusted(holder(10, ID_A, ALIAS_A));
        let r = pass(&w, &p);
        assert_eq!(r.action, Action::Skipped);
        assert_eq!(r.exit_code, 0);
        let want = format!("{NOW_TEXT} skipped lease=- alias=- pid=- reason=locked\n");
        assert_eq!(p.log(), want);
        assert_eq!(r.stdout, want);
        assert_eq!(w.calls(), vec!["lock", "stdout"]);
    }
}

#[test]
fn a_lock_error_fails_the_pass_with_exit_1() {
    let (w, p) = (World::new(), Place::new());
    w.lock.set(LockMode::Failed);
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 1);
    assert_eq!(r.action, Action::Failed);
    assert_eq!(
        p.log(),
        failed(
            "-",
            "-",
            "-",
            "The pass stopped on an error: the lock file is a folder"
        )
    );
    assert!(!w.called("self_check"));
}

#[test]
fn a_matcher_that_fails_its_cases_trusts_nothing_and_exits_1() {
    let (w, p) = (World::new(), Place::new());
    *w.self_check.borrow_mut() = Err("case 1; case 2".into());
    w.trusted(holder(10, ID_A, ALIAS_A));
    write_leases(
        &p.state,
        &[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A)],
    );
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 1);
    assert_eq!(
        p.log(),
        failed(
            "-",
            "-",
            "-",
            "The pass stopped on an error: The holder command matcher failed its own checks: case 1; case 2"
        )
    );
    assert_eq!(w.calls(), vec!["lock", "self_check", "stdout", "unlock"]);
    assert!(w.record.borrow().is_some());
}

#[test]
fn the_pass_time_is_fixed_before_the_lock_and_truncated_to_the_second() {
    let (w, p) = (World::new(), Place::new());
    let r = pass_with(&w, &p, None, None);
    assert_eq!(r.action, Action::Idle);
    // The clock read NOW plus 999 ms, once, before the lock.
    assert_eq!(p.log(), line("idle", "-", "-", "-"));
    assert_eq!(w.calls()[..2], ["now".to_string(), "lock".to_string()]);
    assert_eq!(w.calls().iter().filter(|c| *c == "now").count(), 1);
}

#[test]
fn now_is_the_pass_time_and_no_clock_is_read() {
    let (w, p) = (World::new(), Place::new());
    pass_with(&w, &p, Some(NOW + 3600), None);
    assert_eq!(p.log(), "2026-11-18T06:07:00Z idle lease=- alias=- pid=-\n");
    assert!(!w.called("now"));
}

#[test]
fn the_action_line_is_logged_before_the_lock_is_released() {
    let (w, p) = (World::new(), Place::new());
    pass(&w, &p);
    let calls = w.calls();
    let unlock = calls.iter().position(|c| c == "unlock").unwrap();
    let out = calls.iter().position(|c| c == "stdout").unwrap();
    assert!(out < unlock, "{calls:?}");
}

#[test]
fn every_line_goes_to_the_log_and_stdout_alike_and_the_log_is_appended() {
    let (w, p) = (World::new(), Place::new());
    std::fs::write(p.state.join("board-holder.log"), "earlier\n").unwrap();
    let r = pass(&w, &p);
    assert_eq!(p.log(), format!("earlier\n{}", line("idle", "-", "-", "-")));
    assert_eq!(r.stdout, line("idle", "-", "-", "-"));
}

#[test]
fn a_line_that_cannot_be_logged_is_dropped_and_the_pass_exits_1() {
    let (w, p) = (World::new(), Place::new());
    std::fs::create_dir_all(p.state.join("board-holder.log")).unwrap();
    write_leases(
        &p.state,
        &[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A)],
    );
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 1);
    // What the pass did stands.
    assert_eq!(r.action, Action::Started(holder(4242, ID_A, ALIAS_A)));
    assert!(w.record.borrow().is_some());
    assert_eq!(r.stdout, "");
    // Five tries, 50 ms apart, for the line and again for the failed line in its place.
    let sleeps = w.calls().iter().filter(|c| *c == "sleep 50").count();
    assert_eq!(sleeps, 8);
}

#[test]
fn a_missing_state_file_is_no_leases() {
    let (w, p) = (World::new(), Place::new());
    let r = pass(&w, &p);
    assert_eq!((r.action, r.exit_code), (Action::Idle, 0));
}

#[test]
fn an_unreadable_state_file_fails_the_pass_and_leaves_the_record() {
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    std::fs::write(p.state.join("leases.json"), "{not json").unwrap();
    let r = pass(&w, &p);
    assert_eq!((r.action, r.exit_code), (Action::Failed, 1));
    assert_eq!(
        p.log(),
        failed(
            "-",
            "-",
            "-",
            "Lease state could not be read: the state file is not valid"
        )
    );
    assert!(w.record.borrow().is_some());
    assert!(!w.called("read record"));
    assert!(!w.called("stop 10"));
}

#[test]
fn a_state_file_with_two_records_under_one_id_is_not_valid() {
    let (w, p) = (World::new(), Place::new());
    write_leases(
        &p.state,
        &[
            lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A),
            lease(ID_A, "ready", Some("2026-11-18T07:00:00Z"), ALIAS_A),
        ],
    );
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 1);
    assert_eq!(
        p.log(),
        failed(
            "-",
            "-",
            "-",
            "Lease state could not be read: the state file is not valid"
        )
    );
    assert!(!w.called(&format!("start {ID_A}")));
}

#[test]
fn a_folder_at_the_state_files_path_cannot_be_read() {
    let (w, p) = (World::new(), Place::new());
    std::fs::create_dir_all(p.state.join("leases.json")).unwrap();
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 1);
    assert!(p.log().contains("reason=Lease state could not be read: "));
}

// ---------------------------------------------------------------------------------------------
// The holder's log.

#[test]
fn a_holders_log_over_1_mib_is_rotated_before_the_lease_state_is_read() {
    let (w, p) = (World::new(), Place::new());
    let ssh_log = p.state.join("board-holder-ssh.log");
    let old = p.state.join("board-holder-ssh.log.1");
    std::fs::write(&ssh_log, vec![b'x'; 1_048_577]).unwrap();
    std::fs::write(&old, b"older").unwrap();
    // An unreadable state file shows the rotation came first.
    std::fs::write(p.state.join("leases.json"), "{").unwrap();
    let r = pass(&w, &p);
    assert!(!ssh_log.exists());
    assert_eq!(std::fs::metadata(&old).unwrap().len(), 1_048_577);
    let want = format!(
        "{}{}",
        line("rotated", "-", "-", "-"),
        failed(
            "-",
            "-",
            "-",
            "Lease state could not be read: the state file is not valid"
        )
    );
    assert_eq!(p.log(), want);
    assert_eq!(r.stdout, want);
}

#[test]
fn a_holders_log_of_exactly_1_mib_is_not_rotated() {
    let (w, p) = (World::new(), Place::new());
    let ssh_log = p.state.join("board-holder-ssh.log");
    std::fs::write(&ssh_log, vec![b'x'; 1_048_576]).unwrap();
    pass(&w, &p);
    assert!(ssh_log.exists());
    assert!(!p.state.join("board-holder-ssh.log.1").exists());
    assert_eq!(p.log(), line("idle", "-", "-", "-"));
}

#[test]
fn a_skipped_pass_does_not_rotate() {
    let (w, p) = (World::new(), Place::new());
    w.lock.set(LockMode::Skipped);
    let ssh_log = p.state.join("board-holder-ssh.log");
    std::fs::write(&ssh_log, vec![b'x'; 1_048_577]).unwrap();
    pass(&w, &p);
    assert!(ssh_log.exists());
}

// ---------------------------------------------------------------------------------------------
// Which leases are ready, and which lease.

#[test]
fn a_lease_that_expires_at_the_pass_time_is_not_ready() {
    let (w, p) = (World::new(), Place::new());
    write_leases(&p.state, &[lease(ID_A, "ready", Some(NOW_TEXT), ALIAS_A)]);
    assert_eq!(pass(&w, &p).action, Action::Idle);
    write_leases(
        &p.state,
        &[lease(ID_A, "ready", Some("2026-11-18T05:07:01Z"), ALIAS_A)],
    );
    assert!(matches!(pass(&w, &p).action, Action::Started(_)));
}

#[test]
fn only_a_stored_ready_lease_is_ready() {
    let (w, p) = (World::new(), Place::new());
    write_leases(
        &p.state,
        &[
            lease(ID_A, "released", Some("2026-11-18T06:00:00Z"), ALIAS_A),
            lease(ID_B, "pending", None, ALIAS_B),
        ],
    );
    assert_eq!(pass(&w, &p).action, Action::Idle);
    assert_eq!(p.log(), line("idle", "-", "-", "-"));
}

#[test]
fn the_latest_expiry_is_chosen_and_a_tie_goes_to_the_lowest_id() {
    let (w, p) = (World::new(), Place::new());
    write_leases(
        &p.state,
        &[
            lease(ID_C, "ready", Some("2026-11-18T07:00:00Z"), ALIAS_C),
            lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A),
            lease(ID_B, "ready", Some("2026-11-18T07:00:00Z"), ALIAS_B),
        ],
    );
    *w.start.borrow_mut() = Started::Holder(holder(4242, ID_B, ALIAS_B));
    let r = pass(&w, &p);
    assert_eq!(r.action, Action::Started(holder(4242, ID_B, ALIAS_B)));
    assert_eq!(p.log(), line("started", ID_B, ALIAS_B, "4242"));
}

// ---------------------------------------------------------------------------------------------
// Trusting the record.

fn untrusted_case(setup: impl Fn(&World)) {
    // With a lease ready, the pass goes on as with no holder: the record goes, its process is
    // left alone, and a holder is started.
    let (w, p) = (World::new(), Place::new());
    write_leases(
        &p.state,
        &[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A)],
    );
    w.trusted(holder(10, ID_A, ALIAS_A));
    setup(&w);
    let r = pass(&w, &p);
    assert!(w.called("remove record"));
    assert!(!w.called("stop 10"), "{:?}", w.calls());
    assert_eq!(r.action, Action::Started(holder(4242, ID_A, ALIAS_A)));
    assert_eq!(p.log(), line("started", ID_A, ALIAS_A, "4242"));

    // With none ready, it logs idle.
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    setup(&w);
    let r = pass(&w, &p);
    assert!(w.called("remove record"));
    assert!(!w.called("stop 10"));
    assert_eq!(r.action, Action::Idle);
    assert_eq!(p.log(), line("idle", "-", "-", "-"));
}

#[test]
fn a_record_whose_process_is_gone_is_removed() {
    untrusted_case(|w| {
        w.processes.borrow_mut().insert(10, Query::Gone);
    });
}

#[test]
fn a_record_whose_process_cannot_be_opened_is_removed_and_the_process_left() {
    untrusted_case(|w| {
        w.processes.borrow_mut().insert(10, Query::CannotOpen(5));
    });
}

#[test]
fn a_record_whose_start_time_differs_is_removed() {
    untrusted_case(|w| {
        w.processes
            .borrow_mut()
            .insert(10, running("2026-11-18T05:06:59.124Z"));
    });
}

#[test]
fn a_record_whose_command_line_cannot_be_read_is_removed() {
    untrusted_case(|w| {
        w.processes.borrow_mut().insert(
            10,
            Query::Running(ProcessInfo {
                process_started_at: STARTED_AT.into(),
                command_line: None,
            }),
        );
    });
}

#[test]
fn a_record_whose_command_line_is_not_the_holder_command_is_removed() {
    untrusted_case(|w| w.is_holder.set(false));
}

#[test]
fn a_record_that_cannot_be_removed_fails_the_pass() {
    let (w, p) = (World::new(), Place::new());
    *w.record.borrow_mut() = Some(holder(10, ID_A, ALIAS_A));
    *w.remove_fails.borrow_mut() = Some("denied".into());
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 1);
    assert_eq!(
        p.log(),
        failed(
            "-",
            "-",
            "-",
            "The pass stopped on an error: The record could not be removed: denied"
        )
    );
}

#[test]
fn the_matcher_is_asked_about_the_records_alias() {
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    write_leases(
        &p.state,
        &[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A)],
    );
    pass(&w, &p);
    assert!(w.called(&format!("is_holder {ALIAS_A}")));
}

// ---------------------------------------------------------------------------------------------
// The act table.

#[test]
fn a_trusted_holder_whose_lease_is_ready_under_its_alias_is_kept() {
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    // Another lease expires later; the holder is kept all the same.
    write_leases(
        &p.state,
        &[
            lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A),
            lease(ID_B, "ready", Some("2026-11-18T09:00:00Z"), ALIAS_B),
        ],
    );
    let r = pass(&w, &p);
    assert_eq!(r.action, Action::Kept(holder(10, ID_A, ALIAS_A)));
    assert_eq!(p.log(), line("kept", ID_A, ALIAS_A, "10"));
    assert!(!w.called("stop 10"));
    assert!(w.record.borrow().is_some());
}

#[test]
fn a_trusted_record_wins_over_a_tracked_holder() {
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    write_leases(
        &p.state,
        &[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A)],
    );
    let tracked = holder(99, ID_B, ALIAS_B);
    let r = pass_with(&w, &p, Some(NOW), Some(&tracked));
    assert_eq!(r.action, Action::Kept(holder(10, ID_A, ALIAS_A)));

    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    let r = pass_with(&w, &p, Some(NOW), Some(&tracked));
    assert_eq!(r.action, Action::Stopped(holder(10, ID_A, ALIAS_A)));
}

fn stop_case(leases: &[String]) {
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    write_leases(&p.state, leases);
    let r = pass(&w, &p);
    assert_eq!(r.action, Action::Stopped(holder(10, ID_A, ALIAS_A)));
    assert_eq!(r.exit_code, 0);
    assert_eq!(p.log(), line("stopped", ID_A, ALIAS_A, "10"));
    assert!(w.called("stop 10"));
    assert!(w.record.borrow().is_none());
    // A pass that stops a holder starts nothing, though another lease is ready.
    assert!(!w.calls().iter().any(|c| c.starts_with("start ")));
}

#[test]
fn a_trusted_holder_whose_lease_expired_is_stopped() {
    stop_case(&[
        lease(ID_A, "ready", Some(NOW_TEXT), ALIAS_A),
        lease(ID_B, "ready", Some("2026-11-18T09:00:00Z"), ALIAS_B),
    ]);
}

#[test]
fn a_trusted_holder_whose_lease_was_released_or_is_gone_is_stopped() {
    stop_case(&[lease(
        ID_A,
        "released",
        Some("2026-11-18T06:00:00Z"),
        ALIAS_A,
    )]);
    stop_case(&[lease(ID_B, "ready", Some("2026-11-18T09:00:00Z"), ALIAS_B)]);
    stop_case(&[]);
}

#[test]
fn a_trusted_holder_whose_alias_changed_is_stopped() {
    stop_case(&[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_B)]);
}

#[test]
fn a_keeper_that_tracks_a_holder_and_finds_none_trusted_logs_lost_and_starts_nothing() {
    for record in [None, Some(holder(10, ID_A, ALIAS_A))] {
        let (w, p) = (World::new(), Place::new());
        // Untrusted, if there: its process is gone.
        *w.record.borrow_mut() = record.clone();
        write_leases(
            &p.state,
            &[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A)],
        );
        let tracked = holder(25160, ID_C, ALIAS_C);
        let r = pass_with(&w, &p, Some(NOW), Some(&tracked));
        assert_eq!(r.action, Action::Lost(tracked.clone()));
        assert_eq!(r.exit_code, 0);
        assert_eq!(p.log(), line("lost", ID_C, ALIAS_C, "25160"));
        assert!(!w.calls().iter().any(|c| c.starts_with("start ")));
        assert_eq!(w.called("remove record"), record.is_some());
    }
}

#[test]
fn no_holder_and_no_ready_lease_is_idle() {
    let (w, p) = (World::new(), Place::new());
    let r = pass(&w, &p);
    assert_eq!((r.action, r.exit_code), (Action::Idle, 0));
    assert_eq!(p.log(), line("idle", "-", "-", "-"));
    assert_eq!(r.stdout, line("idle", "-", "-", "-"));
}

// ---------------------------------------------------------------------------------------------
// The stop table.

fn stop_row(found: Stopped, kept: bool, want: &str) {
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    *w.stop.borrow_mut() = found;
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 0);
    assert_eq!(p.log(), want);
    assert_eq!(w.record.borrow().is_some(), kept, "{want}");
    match r.action {
        Action::Stopped(h) | Action::StopFailed(h) => assert_eq!(h.pid, 10),
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_row_of_the_stop_table() {
    stop_row(Stopped::All, false, &line("stopped", ID_A, ALIAS_A, "10"));
    stop_row(Stopped::Gone, false, &line("stopped", ID_A, ALIAS_A, "10"));
    stop_row(
        Stopped::CannotOpenToStop(5),
        true,
        &failed(
            ID_A,
            ALIAS_A,
            "10",
            "The holder could not be opened to stop it: Windows error 5.",
        ),
    );
    stop_row(
        Stopped::NotStopped("its command line no longer matched".into()),
        true,
        &failed(
            ID_A,
            ALIAS_A,
            "10",
            "The holder could not be stopped: its command line no longer matched, but the holder is still running.",
        ),
    );
    stop_row(
        Stopped::Error("Access is denied.".into()),
        true,
        &failed(
            ID_A,
            ALIAS_A,
            "10",
            "The holder could not be stopped: Access is denied.",
        ),
    );
    stop_row(
        Stopped::StillRunning,
        true,
        &failed(
            ID_A,
            ALIAS_A,
            "10",
            "The holder is still running after it was stopped.",
        ),
    );
    stop_row(
        Stopped::TreeLeft(vec![31, 32]),
        false,
        &failed(
            ID_A,
            ALIAS_A,
            "10",
            "The holder stopped, but these processes of its tree did not: 31,32.",
        ),
    );
}

#[test]
fn a_record_that_cannot_be_removed_after_a_stop_is_not_reported() {
    // After a stop the failed removal is not reported and the line stays as it is; the next pass
    // removes the record (keeper.md, Exit codes; architect, 2026-11-19, for E0.06-T01-S06).
    let rows = [
        (Stopped::All, line("stopped", ID_A, ALIAS_A, "10")),
        (Stopped::Gone, line("stopped", ID_A, ALIAS_A, "10")),
        (
            Stopped::TreeLeft(vec![31]),
            failed(
                ID_A,
                ALIAS_A,
                "10",
                "The holder stopped, but these processes of its tree did not: 31.",
            ),
        ),
    ];
    for (found, want) in rows {
        let (w, p) = (World::new(), Place::new());
        w.trusted(holder(10, ID_A, ALIAS_A));
        *w.stop.borrow_mut() = found;
        *w.remove_fails.borrow_mut() = Some("denied".into());
        let r = pass(&w, &p);
        assert_eq!(r.exit_code, 0, "{want}");
        assert_eq!(p.log(), want);
        assert_eq!(r.stdout, want);
        assert!(w.called("remove record"), "{want}");
        assert!(!p.log().contains("denied"), "{want}");
    }
}

#[test]
fn a_stop_error_with_a_line_break_is_one_line() {
    stop_row(
        Stopped::Error("first\r\nsecond\r\n".into()),
        true,
        &failed(
            ID_A,
            ALIAS_A,
            "10",
            "The holder could not be stopped: first second",
        ),
    );
}

// ---------------------------------------------------------------------------------------------
// Starting a holder.

fn ready_a(p: &Place) {
    write_leases(
        &p.state,
        &[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A)],
    );
}

#[test]
fn a_start_hands_the_starter_the_pass_and_records_before_it_says_started() {
    let (w, p) = (World::new(), Place::new());
    ready_a(&p);
    let r = pass(&w, &p);
    assert_eq!(r.action, Action::Started(holder(4242, ID_A, ALIAS_A)));
    assert_eq!(r.exit_code, 0);
    assert_eq!(p.log(), line("started", ID_A, ALIAS_A, "4242"));
    let req = w.request.borrow().clone().unwrap();
    assert_eq!(
        req,
        (
            p.state.clone(),
            p.profile.clone(),
            p.ssh.clone(),
            ID_A.to_string(),
            ALIAS_A.to_string(),
            p.own.clone()
        )
    );
    let calls = w.calls();
    let write = calls.iter().position(|c| c == "write record 4242").unwrap();
    let out = calls.iter().position(|c| c == "stdout").unwrap();
    assert!(write < out);
    assert_eq!(*w.record.borrow(), Some(holder(4242, ID_A, ALIAS_A)));
}

#[test]
fn a_failed_start_logs_the_starters_whole_reason() {
    let (w, p) = (World::new(), Place::new());
    ready_a(&p);
    *w.start.borrow_mut() = Started::Failed(
        "The holder could not be started: The holder exited at once; see board-holder-ssh.log."
            .into(),
    );
    let r = pass(&w, &p);
    assert_eq!((r.action, r.exit_code), (Action::StartFailed(None), 0));
    assert_eq!(
        p.log(),
        failed(
            ID_A,
            ALIAS_A,
            "-",
            "The holder could not be started: The holder exited at once; see board-holder-ssh.log."
        )
    );
    assert!(w.record.borrow().is_none());
}

#[test]
fn an_ssh_exe_that_is_not_a_file_is_refused_before_the_start() {
    let (w, p) = (World::new(), Place::new());
    ready_a(&p);
    let missing = p.state.join("no-ssh.exe");
    let ctx = PassContext {
        state_dir: &p.state,
        profile: &p.profile,
        ssh_exe: &missing,
        own_exe: &p.own,
        now: Some(NOW),
        tracked: None,
    };
    let mut out = Vec::new();
    let r = run_pass(&ctx, &w.deps(), &mut out);
    assert_eq!((r.action, r.exit_code), (Action::StartFailed(None), 0));
    assert_eq!(
        p.log(),
        failed(
            ID_A,
            ALIAS_A,
            "-",
            &format!(
                "The holder could not be started: SshExe '{}' does not exist.",
                missing.display()
            )
        )
    );
    assert!(!w.calls().iter().any(|c| c.starts_with("start ")));
}

#[test]
fn a_record_that_cannot_be_written_stops_the_holder() {
    let cases = [
        (
            Stopped::All,
            "The record could not be written, so the holder was stopped, or had exited: disk full",
        ),
        (
            Stopped::Gone,
            "The record could not be written, so the holder was stopped, or had exited: disk full",
        ),
        (
            Stopped::TreeLeft(vec![3]),
            "The record could not be written, so the holder was stopped, or had exited: disk full",
        ),
        (
            Stopped::StillRunning,
            "The record could not be written, and the holder could not be stopped, so it runs unrecorded: disk full",
        ),
        (
            Stopped::CannotOpenToStop(5),
            "The record could not be written, and the holder could not be stopped, so it runs unrecorded: disk full",
        ),
        (
            Stopped::NotStopped("why".into()),
            "The record could not be written, and the holder could not be stopped, so it runs unrecorded: disk full",
        ),
        (
            Stopped::Error("e".into()),
            "The record could not be written, and the holder could not be stopped, so it runs unrecorded: disk full",
        ),
    ];
    for (found, reason) in cases {
        let (w, p) = (World::new(), Place::new());
        ready_a(&p);
        *w.write_fails.borrow_mut() = Some("disk full".into());
        *w.stop.borrow_mut() = found;
        let r = pass(&w, &p);
        assert_eq!(
            (r.action, r.exit_code),
            (Action::StartFailed(Some(4242)), 0)
        );
        assert!(w.called("stop 4242"));
        assert_eq!(p.log(), failed(ID_A, ALIAS_A, "4242", reason));
    }
}

// ---------------------------------------------------------------------------------------------
// The front.

struct Front {
    code: i32,
    stdout: String,
    stderr: String,
}

/// The stand-in guard every run of `front` names, in its throwaway folder.
const STAND_IN: &str = "guard-stand-in.exe";

fn front(args: &[&str], vars: &[(&str, &Path)], world: &World, cwd: &Path) -> Front {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut map: HashMap<String, OsString> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.as_os_str().to_owned()))
        .collect();
    // Every run of the keeper names a stand-in guard in its throwaway folder (AGENTS.md;
    // conformance.md, The harness); the fake guard starts nothing all the same.
    let stand_in = cwd.join(STAND_IN);
    std::fs::write(&stand_in, b"stand-in").unwrap();
    map.insert("AGENTSPACE_GUARD_CMD".into(), stand_in.into_os_string());
    let var = move |name: &str| map.get(name).cloned();
    let own = cwd.join("agentspace-keeper.exe");
    let env = Env {
        var: &var,
        cwd,
        own_exe: &own,
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &env, &world.deps(), &mut out, &mut err);
    Front {
        code,
        stdout: String::from_utf8(out).unwrap(),
        stderr: String::from_utf8(err).unwrap(),
    }
}

const USAGE_LINES: &str = "Usage: agentspace-keeper [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]\n       agentspace-keeper -Watch [-SshExe <full path>] [-IntervalSeconds <n>] [-MaxPasses <n>] [-BackoffBaseSeconds <n>] [-BackoffMaxSeconds <n>] [-ReleaseGraceSeconds <n>]\n";

#[test]
fn a_usage_error_writes_three_lines_to_stderr_logs_nothing_and_exits_2() {
    let t = TempDir::new();
    let home = t.path().join("home");
    let w = World::new();
    // Without USERPROFILE, too: the usage checks come first.
    for vars in [vec![("USERPROFILE", home.as_path())], vec![]] {
        let f = front(&["-Bogus"], &vars, &w, t.path());
        assert_eq!(f.code, 2);
        assert_eq!(
            f.stderr,
            format!("agentspace-keeper: Unknown argument '-Bogus'.\n{USAGE_LINES}")
        );
        assert_eq!(f.stdout, "");
    }
    assert!(!home.exists());
    assert!(w.calls().is_empty());
}

#[test]
fn an_unset_or_empty_userprofile_ends_the_run_before_its_first_pass() {
    let t = TempDir::new();
    let state = t.path().join("state");
    let empty = PathBuf::from("");
    let w = World::new();
    let sets: [Vec<(&str, &Path)>; 4] = [
        vec![],
        vec![("USERPROFILE", &empty)],
        vec![("AGENTSPACE_STATE_DIR", &state)],
        vec![("USERPROFILE", &empty), ("AGENTSPACE_STATE_DIR", &state)],
    ];
    for vars in sets {
        let f = front(&["-SshExe", r"C:\x\ssh.exe"], &vars, &w, t.path());
        assert_eq!(f.code, 1);
        assert_eq!(
            f.stderr,
            "agentspace-keeper: USERPROFILE is not set, so there is no home folder.\n"
        );
        assert_eq!(f.stdout, "");
    }
    assert!(!state.exists());
    assert!(w.calls().is_empty());
}

#[test]
fn the_state_folder_is_userprofiles_agentspace_and_is_created() {
    let t = TempDir::new();
    let home = t.path().join("home");
    let w = World::new();
    let f = front(
        &["-SshExe", r"C:\x\ssh.exe", "-Now", NOW_TEXT],
        &[("USERPROFILE", &home)],
        &w,
        t.path(),
    );
    assert_eq!(f.code, 0);
    assert_eq!(f.stderr, "");
    assert_eq!(f.stdout, line("idle", "-", "-", "-"));
    let log = std::fs::read_to_string(home.join(".agentspace").join("board-holder.log")).unwrap();
    assert_eq!(log, f.stdout);
}

#[test]
fn agentspace_state_dir_moves_the_state_folder_and_a_relative_value_is_made_full() {
    let t = TempDir::new();
    let home = t.path().join("home");
    let ssh = t.path().join("ssh.exe");
    std::fs::write(&ssh, b"stand-in").unwrap();
    let w = World::new();
    let relative = PathBuf::from("rel-state");
    let state = t.path().join("rel-state");
    std::fs::create_dir_all(&state).unwrap();
    write_leases(
        &state,
        &[lease(ID_A, "ready", Some("2026-11-18T06:00:00Z"), ALIAS_A)],
    );
    let f = front(
        &["-SshExe", ssh.to_str().unwrap(), "-Now", NOW_TEXT],
        &[("USERPROFILE", &home), ("AGENTSPACE_STATE_DIR", &relative)],
        &w,
        t.path(),
    );
    assert_eq!(f.code, 0, "{}", f.stderr);
    assert_eq!(f.stdout, line("started", ID_A, ALIAS_A, "4242"));
    let req = w.request.borrow().clone().unwrap();
    assert_eq!(req.0, state);
    assert_eq!(req.1, home);
    assert_eq!(req.2, ssh);
    assert!(!home.join(".agentspace").exists());
}

#[test]
fn a_state_folder_that_cannot_be_created_ends_the_run_with_exit_1() {
    let t = TempDir::new();
    let home = t.path().join("home");
    let file = t.path().join("a-file");
    std::fs::write(&file, b"x").unwrap();
    let w = World::new();
    let f = front(
        &["-SshExe", r"C:\x\ssh.exe"],
        &[("USERPROFILE", &home), ("AGENTSPACE_STATE_DIR", &file)],
        &w,
        t.path(),
    );
    assert_eq!(f.code, 1);
    // The line's text is keeper.md's (Exit codes; architect, 2026-11-19, for E0.06-T01-S06); the
    // `<error>` after it is the system's.
    let start = format!(
        "agentspace-keeper: The state folder '{}' could not be created: ",
        file.display()
    );
    assert!(f.stderr.starts_with(&start), "{}", f.stderr);
    assert!(f.stderr.len() > start.len() + 1, "{}", f.stderr);
    assert!(f.stderr.ends_with('\n'), "{}", f.stderr);
    assert_eq!(f.stderr.lines().count(), 1);
    assert_eq!(f.stdout, "");
    assert!(w.calls().is_empty());
    // Nothing is logged, and the file in the folder's place is left as it was.
    assert_eq!(std::fs::read(&file).unwrap(), b"x");
    assert!(!home.exists());
}

#[test]
fn the_default_ssh_comes_from_systemroot() {
    let t = TempDir::new();
    let home = t.path().join("home");
    let root = PathBuf::from(r"C:\Windows");
    let w = World::new();
    let f = front(
        &["-Now", NOW_TEXT],
        &[("USERPROFILE", &home), ("SystemRoot", &root)],
        &w,
        t.path(),
    );
    assert_eq!(f.code, 0);
    let f = front(&[], &[("USERPROFILE", &home)], &w, t.path());
    assert_eq!(f.code, 2);
    assert_eq!(
        f.stderr,
        format!(
            "agentspace-keeper: SshExe '\\System32\\OpenSSH\\ssh.exe' is not a full path.\n{USAGE_LINES}"
        )
    );
}

// Until E0.06-T02-S05 this test asserted that -Watch runs one pass; -Watch now loops, so it ends
// at -MaxPasses, after taking the keeper lock once, before the first pass.
#[test]
fn watch_takes_the_keeper_lock_once_and_ends_at_max_passes() {
    let t = TempDir::new();
    let home = t.path().join("home");
    let w = World::new();
    let f = front(
        &["-Watch", "-SshExe", r"C:\x\ssh.exe", "-MaxPasses", "1"],
        &[("USERPROFILE", &home)],
        &w,
        t.path(),
    );
    assert_eq!(f.code, 0);
    // From v0.08 the keeper's first line is its start of the guard, before the pass's.
    let lines: Vec<&str> = f.stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines[0].ends_with(" guard started pid=31244"),
        "{}",
        lines[0]
    );
    assert!(lines[1].ends_with(" idle lease=- alias=- pid=-"));
    let calls = w.calls();
    assert_eq!(calls.iter().filter(|c| *c == "keeper lock").count(), 1);
    assert_eq!(calls.first().map(String::as_str), Some("keeper lock"));
    assert!(!calls.iter().any(|c| c.starts_with("sleep")));
    // The start comes after the keeper lock and before the pass's lock, not inside the pass.
    let at = |what: &str| calls.iter().position(|c| c == what).unwrap();
    assert!(at("keeper lock") < at("guard lock"));
    assert!(at(&format!("guard start {STAND_IN} guard")) < at("lock"));
}

/// keeper.md, When the keeper starts it (G12): a one-pass run never tries `guard.lock` and starts
/// no guard, though `AGENTSPACE_GUARD_CMD` is set.
#[test]
fn a_one_pass_run_never_starts_the_guard() {
    let t = TempDir::new();
    let home = t.path().join("home");
    let w = World::new();
    let f = front(
        &["-SshExe", r"C:\x\ssh.exe", "-Now", NOW_TEXT],
        &[("USERPROFILE", &home)],
        &w,
        t.path(),
    );
    assert_eq!(f.code, 0);
    assert_eq!(f.stdout, format!("{NOW_TEXT} idle lease=- alias=- pid=-\n"));
    assert!(!w.calls().iter().any(|c| c.starts_with("guard")));
}

// ---------------------------------------------------------------------------------------------
// A turn per host (keeper.md, The hosts of a pass; D50). The fake records serve the box; another
// host's record is its real file in the throwaway state folder, through `Records`' defaults.

const ALPHA_REGISTRY: &str = r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"},{"project":"alpha","stack":"agentspace-host-alpha","state":"ready"}]}"#;

fn write_registry(place: &Place, doc: &str) {
    let dir = place.profile.join(".agentspace");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hosts.json"), doc).unwrap();
}

/// A ready lease of `project`.
fn project_lease(id: &str, project: &str, alias: &str) -> String {
    lease(id, "ready", Some("2026-11-18T06:00:00Z"), alias)
        .replace(r#""project":"demo""#, &format!(r#""project":"{project}""#))
}

#[test]
fn a_pass_starts_a_holder_on_each_host_the_boxs_first_each_in_its_own_record() {
    let (w, p) = (World::new(), Place::new());
    write_registry(&p, ALPHA_REGISTRY);
    write_leases(
        &p.state,
        &[
            project_lease(ID_A, "swimcoach", ALIAS_A),
            project_lease(ID_B, "alpha", ALIAS_B),
        ],
    );
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 0);
    let starts: Vec<String> = w
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("start "))
        .collect();
    assert_eq!(
        starts,
        vec![format!("start {ID_A}"), format!("start {ID_B}")]
    );
    let log = p.log();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(lines[0].contains(" started ") && lines[0].ends_with(" host=swimcoach"));
    assert!(lines[1].contains(&format!(" started lease={ID_B} ")));
    assert!(lines[1].ends_with(" host=alpha"));
    assert!(
        w.record.borrow().is_some(),
        "The box's record is the box's own"
    );
    assert!(p.state.join("board-holder.alpha.json").is_file());
    assert!(!p.state.join("board-holder.swimcoach.json").exists());
}

#[test]
fn a_lease_whose_host_is_not_ready_is_on_the_box() {
    let (w, p) = (World::new(), Place::new());
    write_registry(
        &p,
        &ALPHA_REGISTRY.replace(r#""state":"ready"}]"#, r#""state":"creating"}]"#),
    );
    write_leases(&p.state, &[project_lease(ID_B, "alpha", ALIAS_B)]);
    let r = pass(&w, &p);
    assert_eq!(r.action, Action::Started(holder(4242, ID_A, ALIAS_A)));
    assert!(w.called(&format!("start {ID_B}")));
    assert_eq!(p.log().lines().count(), 1);
    assert!(!p.state.join("board-holder.alpha.json").exists());
}

#[test]
fn a_host_whose_record_is_in_the_state_folder_gets_a_turn_and_its_idle_line_names_it() {
    let (w, p) = (World::new(), Place::new());
    // A record of a host the registry does not name, whose process is gone.
    std::fs::write(
        p.state.join("board-holder.zeta.json"),
        format!(
            r#"{{"pid":77,"process_started_at":"{STARTED_AT}","lease_id":"{ID_A}","alias":"{ALIAS_A}"}}"#
        ),
    )
    .unwrap();
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 0);
    assert_eq!(
        p.log(),
        format!(
            "{NOW_TEXT} idle lease=- alias=- pid=-\n{NOW_TEXT} idle lease=- alias=- pid=- host=zeta\n"
        )
    );
    assert!(
        !p.state.join("board-holder.zeta.json").exists(),
        "An untrusted record is removed"
    );
}

#[test]
fn a_holder_on_a_host_that_left_ready_is_stopped_and_the_boxs_is_kept() {
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    // alpha's holder, trusted, in its own record; alpha is not in the registry, so alpha's lease
    // is on the box, and no longer on alpha.
    w.processes.borrow_mut().insert(20, running(STARTED_AT));
    std::fs::write(
        p.state.join("board-holder.alpha.json"),
        format!(
            r#"{{"pid":20,"process_started_at":"{STARTED_AT}","lease_id":"{ID_B}","alias":"{ALIAS_B}"}}"#
        ),
    )
    .unwrap();
    write_leases(
        &p.state,
        &[
            project_lease(ID_A, "swimcoach", ALIAS_A),
            project_lease(ID_B, "alpha", ALIAS_B),
        ],
    );
    let r = pass(&w, &p);
    assert_eq!(r.action, Action::Kept(holder(10, ID_A, ALIAS_A)));
    let log = p.log();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert_eq!(lines[0], line("kept", ID_A, ALIAS_A, "10").trim_end());
    assert_eq!(
        lines[1],
        format!("{NOW_TEXT} stopped lease={ID_B} alias={ALIAS_B} pid=20 host=alpha")
    );
    assert!(w.called("stop 20"));
    assert!(!w.called("stop 10"));
    assert!(!p.state.join("board-holder.alpha.json").exists());
    assert!(!w.calls().iter().any(|c| c.starts_with("start ")));
}

#[test]
fn a_registry_that_is_not_valid_fails_the_pass_and_touches_no_holder() {
    let (w, p) = (World::new(), Place::new());
    w.trusted(holder(10, ID_A, ALIAS_A));
    write_registry(&p, "not json");
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 1);
    assert_eq!(r.action, Action::Failed);
    assert_eq!(
        p.log(),
        failed(
            "-",
            "-",
            "-",
            "Host registry could not be read: the host registry is not valid"
        )
    );
    assert!(!w.called("read record"));
    assert!(!w.called("stop 10"));
    assert!(w.record.borrow().is_some());
}

#[test]
fn the_box_failing_its_turn_does_not_stop_another_hosts_turn() {
    let (w, p) = (World::new(), Place::new());
    write_registry(&p, ALPHA_REGISTRY);
    // The box's record is untrusted and cannot be removed: its turn ends on that error.
    *w.record.borrow_mut() = Some(holder(10, ID_A, ALIAS_A));
    *w.remove_fails.borrow_mut() = Some("denied".into());
    write_leases(&p.state, &[project_lease(ID_B, "alpha", ALIAS_B)]);
    let r = pass(&w, &p);
    assert_eq!(r.exit_code, 1, "The run exits 1 once the pass has ended");
    let log = p.log();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert_eq!(
        format!("{}\n", lines[0]),
        failed(
            "-",
            "-",
            "-",
            "The pass stopped on an error: The record could not be removed: denied"
        )
    );
    assert!(lines[1].contains(&format!(" started lease={ID_B} ")));
    assert!(lines[1].ends_with(" host=alpha"));
}

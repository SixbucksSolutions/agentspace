//! Shared helpers for the Windows keeper tests (docs/keeper.md; docs/state.md).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Every test that needs a second process starts this test program again, with
//! `--exact role_entry`, and a role in its environment. The role does a small job (sleep, hold a
//! lock, take a lock and say what it got) and says so through files in the test's throwaway
//! folder. A process a test starts is ended by a guard, through its own handle and never by name,
//! and every role also ends itself after a minute, so nothing outlives a crashed run for long.
//! Every child gets a throwaway home and a throwaway working folder.
#![cfg(windows)]
#![allow(dead_code)]

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use agentspace_keeper_api::{Locks, Matcher, Processes, Taken};
use agentspace_keeper_win::{WinLocks, WinProcesses};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

const ROLE_VAR: &str = "AGENTSPACE_KEEPER_TEST_ROLE";
const DIR_VAR: &str = "AGENTSPACE_KEEPER_TEST_DIR";
const NAME_VAR: &str = "AGENTSPACE_KEEPER_TEST_NAME";

/// How long a role lives at most, so a stray one ends by itself.
const ROLE_LIFETIME: Duration = Duration::from_secs(60);

/// The crate's implementations, built in one place so a name that differs is fixed here only.
pub fn processes() -> WinProcesses {
    WinProcesses::new()
}

pub fn locks() -> WinLocks {
    WinLocks::new()
}

/// A throwaway folder under the temp folder, removed (best effort) when it drops.
pub struct Sandbox {
    pub dir: PathBuf,
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

impl Sandbox {
    pub fn new(label: &str) -> Sandbox {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "agentspace-keeper-win-test-{label}-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(dir.join("home")).expect("The throwaway folder can be made");
        fs::create_dir_all(dir.join("state")).expect("The throwaway state folder can be made");
        Sandbox { dir }
    }

    /// The state folder the locks are taken in.
    pub fn state(&self) -> PathBuf {
        self.dir.join("state")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The command that runs this test program as a role, in a throwaway home and working folder.
pub fn role_command(sb: &Sandbox, role: &str, name: &str) -> Command {
    role_command_in(&sb.dir, role, name)
}

fn role_command_in(dir: &Path, role: &str, name: &str) -> Command {
    let home = dir.join("home");
    let mut cmd = Command::new(std::env::current_exe().expect("The test program has a path"));
    cmd.args(["--exact", "role_entry", "--nocapture", "--test-threads=1"])
        .current_dir(dir)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("LOCALAPPDATA", &home)
        .env("APPDATA", &home)
        .env(ROLE_VAR, role)
        .env(DIR_VAR, dir)
        .env(NAME_VAR, name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// A child test program that is killed and reaped when it drops.
pub struct Spawned {
    pub child: Child,
}

impl Spawned {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Spawned {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn spawn_role(sb: &Sandbox, role: &str, name: &str) -> Spawned {
    let child = role_command(sb, role, name)
        .spawn()
        .expect("The test program starts as a role");
    Spawned { child }
}

/// Polls until the condition holds, or panics after the timeout.
pub fn wait_for(what: &str, timeout: Duration, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        if start.elapsed() > timeout {
            panic!("Gave up waiting for {what}.");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Reads a role's pid file, waiting for it.
pub fn wait_for_pid(sb: &Sandbox, name: &str) -> u32 {
    let path = sb.dir.join(format!("{name}.pid"));
    wait_for(
        &format!("the pid file of {name}"),
        Duration::from_secs(30),
        || path.exists(),
    );
    fs::read_to_string(&path)
        .expect("The pid file reads")
        .trim()
        .parse()
        .expect("The pid file holds a number")
}

pub fn wait_for_file(sb: &Sandbox, name: &str) -> String {
    let path = sb.dir.join(name);
    wait_for(&format!("the file {name}"), Duration::from_secs(40), || {
        path.exists()
    });
    fs::read_to_string(&path).expect("The file reads")
}

/// A process opened by PID on a handle this test holds, so its PID cannot be reused while the
/// test looks at it. Dropping it terminates the process: it is only ever a process the test made.
pub struct Proc {
    pub pid: u32,
    handle: HANDLE,
}

impl Proc {
    pub fn open(pid: u32) -> Proc {
        // SAFETY: Plain call with a PID; the handle is closed in Drop.
        let handle = unsafe {
            OpenProcess(
                PROCESS_TERMINATE | PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            )
        };
        assert!(!handle.is_null(), "A process the test made can be opened");
        Proc { pid, handle }
    }

    /// Whether it still runs.
    pub fn is_running(&self) -> bool {
        // SAFETY: The handle is valid until Drop.
        unsafe { WaitForSingleObject(self.handle, 0) != WAIT_OBJECT_0 }
    }

    /// Waits up to the timeout for it to exit.
    pub fn wait_exit(&self, timeout: Duration) -> bool {
        let ms = timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        // SAFETY: The handle is valid until Drop.
        unsafe { WaitForSingleObject(self.handle, ms) == WAIT_OBJECT_0 }
    }

    pub fn terminate(&self) {
        // SAFETY: The handle is valid until Drop.
        unsafe {
            TerminateProcess(self.handle, 1);
        }
    }

    fn times(&self) -> [u64; 2] {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: The handle is valid, and the four out values are live locals.
        let ok = unsafe {
            GetProcessTimes(
                self.handle,
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        };
        assert!(ok != 0, "The process times can be read");
        let join = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
        [join(created), join(exited)]
    }

    /// Its creation time in milliseconds since the Unix epoch, read by the kernel and not by the
    /// code under test.
    pub fn created_unix_ms(&self) -> i64 {
        (self.times()[0] / 10_000) as i64 - 11_644_473_600_000
    }

    /// Its creation time as the keeper writes it: UTC, `YYYY-MM-DDTHH:MM:SS.fffZ`.
    pub fn created_string(&self) -> String {
        format_unix_ms(self.created_unix_ms())
    }

    /// Its exit time in 100 ns ticks; only meaningful once it has exited.
    pub fn exit_ticks(&self) -> u64 {
        self.times()[1]
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        self.terminate();
        // SAFETY: The handle is valid, and is not used again.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

/// Milliseconds since the Unix epoch as `YYYY-MM-DDTHH:MM:SS.fffZ`.
pub fn format_unix_ms(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

pub fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("The clock is after 1970")
        .as_millis() as i64
}

/// The tree a stop works on: holder, one middle process, and two leaves under it, all running this
/// test program as roles. The holder is the test's own child, so the test reaps it.
pub struct Tree {
    pub holder_child: Spawned,
    pub holder: Proc,
    pub mid: Proc,
    pub leaf_a: Proc,
    pub leaf_b: Proc,
}

impl Tree {
    pub fn all(&self) -> [&Proc; 4] {
        [&self.holder, &self.mid, &self.leaf_a, &self.leaf_b]
    }
}

pub fn start_tree(sb: &Sandbox) -> Tree {
    let holder_child = spawn_role(sb, "holder", "holder");
    let holder_pid = holder_child.pid();
    let mid_pid = wait_for_pid(sb, "mid");
    let leaf_a_pid = wait_for_pid(sb, "leaf_a");
    let leaf_b_pid = wait_for_pid(sb, "leaf_b");
    assert_eq!(wait_for_pid(sb, "holder"), holder_pid);
    Tree {
        holder: Proc::open(holder_pid),
        mid: Proc::open(mid_pid),
        leaf_a: Proc::open(leaf_a_pid),
        leaf_b: Proc::open(leaf_b_pid),
        holder_child,
    }
}

/// A fake matcher: it answers as told, and notes what it was asked.
pub struct FakeMatcher {
    pub answer: bool,
    pub seen: RefCell<Vec<(String, PathBuf, String)>>,
}

impl FakeMatcher {
    pub fn new(answer: bool) -> FakeMatcher {
        FakeMatcher {
            answer,
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl Matcher for FakeMatcher {
    fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String {
        format!(
            "\"{}\" -N -o ExitOnForwardFailure=yes {alias}",
            ssh_exe.display()
        )
    }

    fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool {
        self.seen.borrow_mut().push((
            command_line.to_string(),
            ssh_exe.to_path_buf(),
            alias.to_string(),
        ));
        self.answer
    }

    fn self_check(&self) -> Result<(), String> {
        Ok(())
    }
}

pub const ALIAS: &str = "alias-for-test";

pub fn fake_ssh() -> PathBuf {
    std::env::temp_dir().join("agentspace-keeper-win-test-fake-ssh.exe")
}

/// The record of a running process, with the start time the kernel reports.
pub fn record_of(proc: &Proc) -> agentspace_keeper_api::HolderRecord {
    agentspace_keeper_api::HolderRecord {
        pid: proc.pid,
        process_started_at: proc.created_string(),
        lease_id: "0123456789abcdef".to_string(),
        alias: ALIAS.to_string(),
    }
}

/// A one-word account of what a lock take gave, for a test to compare and for a role to write.
pub fn describe(taken: &Taken) -> String {
    match taken {
        Taken::Held(_) => "held".to_string(),
        Taken::Busy(Some(pid)) => format!("busy:{pid}"),
        Taken::Busy(None) => "busy:-".to_string(),
        Taken::Skipped => "skipped".to_string(),
        Taken::Failed(_) => "failed".to_string(),
    }
}

fn write_file(dir: &Path, name: &str, text: &str) {
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, text).expect("A role file writes");
    fs::rename(&tmp, dir.join(name)).expect("A role file renames into place");
}

fn spawn_role_child(dir: &Path, role: &str, name: &str) -> Child {
    role_command_in(dir, role, name)
        .spawn()
        .expect("A role starts a role")
}

/// Ends the children a role started, by their own handles, when the role itself ends.
fn end_kids(kids: Vec<Child>) {
    for mut kid in kids {
        let _ = kid.kill();
        let _ = kid.wait();
    }
}

/// Sleeps until the role's lifetime ends, or until its release file appears.
fn idle(dir: &Path, name: &str) {
    let release = dir.join(format!("release-{name}"));
    let start = Instant::now();
    while start.elapsed() < ROLE_LIFETIME && !release.exists() {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The body of `role_entry`: nothing at all when the test program was not started as a role.
pub fn role_main() {
    let Ok(role) = std::env::var(ROLE_VAR) else {
        return;
    };
    let dir = PathBuf::from(std::env::var(DIR_VAR).expect("A role has its folder"));
    let name = std::env::var(NAME_VAR).unwrap_or_default();
    let state = dir.join("state");
    write_file(
        &dir,
        &format!("{name}.pid"),
        &std::process::id().to_string(),
    );
    match role.as_str() {
        "sleep" => idle(&dir, &name),
        "holder" => {
            let kids = vec![spawn_role_child(&dir, "mid", "mid")];
            idle(&dir, &name);
            end_kids(kids);
        }
        "mid" => {
            let kids = vec![
                spawn_role_child(&dir, "sleep", "leaf_a"),
                spawn_role_child(&dir, "sleep", "leaf_b"),
            ];
            idle(&dir, &name);
            end_kids(kids);
        }
        "hold-pass" | "hold-keeper" => {
            let locks = WinLocks::new();
            let taken = if role == "hold-pass" {
                locks.pass(&state)
            } else {
                locks.keeper(&state)
            };
            write_file(&dir, &format!("ready-{name}"), &describe(&taken));
            if matches!(taken, Taken::Held(_)) {
                idle(&dir, &name);
            }
        }
        "take-pass" | "take-keeper" => {
            let locks = WinLocks::new();
            let start = Instant::now();
            let taken = if role == "take-pass" {
                locks.pass(&state)
            } else {
                locks.keeper(&state)
            };
            let ms = start.elapsed().as_millis();
            write_file(
                &dir,
                &format!("result-{name}"),
                &format!("{}@{ms}", describe(&taken)),
            );
            if matches!(taken, Taken::Held(_)) {
                idle(&dir, &name);
            }
        }
        other => panic!("Unknown role {other}"),
    }
    std::process::exit(0);
}

/// Splits a role's result, `<what>@<milliseconds>`.
pub fn split_result(text: &str) -> (String, u128) {
    let (what, ms) = text.split_once('@').expect("A result has an at sign");
    (
        what.to_string(),
        ms.trim().parse().expect("A result has milliseconds"),
    )
}

/// Whether a process is `Query::Gone`, by the code under test.
pub fn is_gone_by_query(pid: u32) -> bool {
    matches!(processes().query(pid), agentspace_keeper_api::Query::Gone)
}

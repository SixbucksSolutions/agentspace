//! What the specs share (docs/keeper.md, Starting a holder; docs/state.md, A holder's launch files).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! Every spec file includes this module. It holds the pieces that observe and end processes, the
//! fake matcher, the directory watcher that records the order the launch files appear and
//! disappear in, and the stand-in holders (`stub_*`): ignored tests of these spec binaries, which
//! a spec starts as a child process, through the real launcher or by hand, by running the spec
//! binary again with `--exact common::<name> --ignored --nocapture`.
//!
//! **A spec starts and ends only processes it started or that a launch it ran started**, by PID
//! and, before it ends one, by comparing the creation time. Nothing is ended by name.
//!
//! **A spec never prints the values of an environment.** A stand-in holder reports a variable as
//! present or absent, or as equal to a value or not.

#![allow(dead_code)]
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agentspace_keeper_api::{HolderRecord, Matcher, StartRequest, Started, Starter};
use agentspace_keeper_files::{LaunchResult, parse_launch_result, split_command_line};
use windows_sys::Win32::Foundation::{
    CloseHandle, FILETIME, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, GetFileType, ReadDirectoryChangesW, ReadFile,
};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetConsoleWindow, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessTimes, OpenProcess, TerminateProcess,
    WaitForSingleObject,
};

/// The stand-in launcher program the crate builds for its tests.
pub fn testhost() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_agentspace-launch-testhost").replace('/', "\\"))
}

/// The spec binary itself, which is the stand-in holder.
pub fn stub_exe() -> PathBuf {
    std::env::current_exe().expect("The spec's own program path is known.")
}

/// The stand-in holder's command line for one of this module's `stub_*` tests.
pub fn stub_args(name: &str) -> String {
    format!("--exact common::{name} --ignored --nocapture")
}

/// A full command line for one stand-in holder: `"<spec binary>" --exact common::<name> ...`.
pub fn stub_command(name: &str) -> String {
    format!("\"{}\" {}", stub_exe().display(), stub_args(name))
}

// ---------------------------------------------------------------------------------------------
// The environment of a pass process
// ---------------------------------------------------------------------------------------------

/// The variable that a start MUST NOT carry to the holder (keeper.md, Streams and environment).
pub const PROBE_VAR: &str = "AGENTSPACE_SPEC_PROBE";

/// Makes the spec process handle Ctrl+C normally, once. A harness may start this process with
/// Ctrl+C ignored, which every child would inherit and which would hide what a new process group
/// does, so that only a process started in a new group has it ignored. It changes no variable:
/// **a spec MUST NOT change its own process's environment** (#0137). What a process needs in its
/// environment it is given when it is started: `Fixture::pass_environment`.
pub fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: A plain Win32 call that takes no pointer.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 0);
        }
    });
}

static SERIAL: Mutex<()> = Mutex::new(());

/// A lock for the specs of one binary that create processes in process, so that no other spec of
/// the binary creates a process while one holds an inheritable handle.
pub fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------------------------
// Times
// ---------------------------------------------------------------------------------------------

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("The clock is after the epoch.")
        .as_millis() as i64
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn format_parts(ms: i64, fraction: &str) -> String {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{fraction}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60
    )
}

/// `YYYY-MM-DDTHH:MM:SS.fffZ`.
pub fn format_ms(ms: i64) -> String {
    format_parts(ms, &format!("{:03}", ms.rem_euclid(1000)))
}

/// `YYYY-MM-DDTHH:MM:SS.fffffffZ`, seven fraction digits, as a request's `not_after` has.
pub fn format_ms7(ms: i64) -> String {
    format_parts(ms, &format!("{:03}0000", ms.rem_euclid(1000)))
}

/// Reads either form back, to the millisecond.
pub fn parse_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 24 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let days = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?);
    let secs = n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)?;
    Some((days * 86_400 + secs) * 1000 + n(20..23)?)
}

// ---------------------------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------------------------

const PROCESS_TERMINATE: u32 = 0x0001;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const STILL_ACTIVE_CODE: u32 = 259;

fn creation_of(h: HANDLE) -> Option<String> {
    let mut c = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut e = c;
    let mut k = c;
    let mut u = c;
    // SAFETY: `h` is a valid process handle and the four pointers name live locals.
    let ok = unsafe { GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) };
    if ok == 0 {
        return None;
    }
    let ft = (u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime);
    Some(format_ms((ft / 10_000) as i64 - 11_644_473_600_000))
}

/// The creation time of a process, and whether it still runs; `None` when it cannot be opened (it
/// is gone, or not ours to look at).
pub fn look(pid: u32) -> Option<(String, bool)> {
    // SAFETY: Plain Win32 calls on a handle that is closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let started = creation_of(h);
        let mut code = 0u32;
        let got = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        Some((started?, got != 0 && code == STILL_ACTIVE_CODE))
    }
}

/// Whether the process with this PID runs and was created at this time.
pub fn running(pid: u32, started: &str) -> bool {
    look(pid).is_some_and(|(s, alive)| alive && s == started)
}

/// Ends a process by PID, but only when its creation time is the one this spec was told, so that a
/// PID that was reused is never touched.
pub fn kill_verified(pid: u32, started: &str) {
    // SAFETY: Plain Win32 calls on a handle that is closed before returning.
    unsafe {
        let h = OpenProcess(
            PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
            0,
            pid,
        );
        if h.is_null() {
            return;
        }
        if creation_of(h).as_deref() == Some(started) {
            TerminateProcess(h, 1);
            WaitForSingleObject(h, 5000);
        }
        CloseHandle(h);
    }
}

/// Everything a spec started that must be ended when it ends: processes by PID and creation time,
/// and children it holds.
#[derive(Clone, Default)]
pub struct Reaper {
    tracked: Arc<Mutex<Vec<(u32, String)>>>,
    children: Arc<Mutex<Vec<Child>>>,
}

impl Reaper {
    pub fn track(&self, pid: u32, started: &str) {
        let mut t = self.tracked.lock().unwrap_or_else(|e| e.into_inner());
        if !t.iter().any(|(p, s)| *p == pid && s == started) {
            t.push((pid, started.to_string()));
        }
    }

    pub fn track_child(&self, child: Child) {
        self.children
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(child);
    }

    pub fn tracked(&self) -> Vec<(u32, String)> {
        self.tracked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn kill_all(&self) {
        for (pid, started) in self.tracked() {
            kill_verified(pid, &started);
        }
        for child in self
            .children
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter_mut()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A throwaway folder with a state folder and a home folder in it, a throwaway home for a pass
/// process's environment, and the guard that ends every process the spec started.
pub struct Fixture {
    pub root: PathBuf,
    pub state: PathBuf,
    pub profile: PathBuf,
    /// The throwaway home that a pass process's `USERPROFILE` and `HOME` name, with `local`,
    /// `roaming` and `state-probe` folders in it (`pass_environment`). It is inside `root`, so it
    /// is removed with it.
    pub home: PathBuf,
    pub reaper: Reaper,
}

impl Fixture {
    pub fn new(tag: &str) -> Fixture {
        init();
        static N: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "agentspace-launch-spec-{tag}-{}-{}-{}",
            std::process::id(),
            now_ms(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let state = root.join("state");
        let profile = root.join("profile");
        let home = root.join("pass-home");
        for folder in [
            &state,
            &profile,
            &home,
            &home.join("local"),
            &home.join("roaming"),
            &home.join("state-probe"),
        ] {
            fs::create_dir_all(folder).expect("A throwaway folder can be made.");
        }
        Fixture {
            root,
            state,
            profile,
            home,
            reaper: Reaper::default(),
        }
    }

    /// Gives a process that is about to be started the environment a keeper's pass has, as
    /// values of that child alone: `USERPROFILE`, `HOME`, `LOCALAPPDATA` and `APPDATA` in the
    /// throwaway home, and the two variables that only the keeper's environment has,
    /// `AGENTSPACE_STATE_DIR` and the probe. This spec process's own environment is not changed.
    pub fn pass_environment<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        command
            .env("USERPROFILE", &self.home)
            .env("HOME", &self.home)
            .env("LOCALAPPDATA", self.home.join("local"))
            .env("APPDATA", self.home.join("roaming"))
            .env("AGENTSPACE_STATE_DIR", self.home.join("state-probe"))
            .env(PROBE_VAR, "1")
    }

    pub fn log(&self) -> PathBuf {
        self.state.join("board-holder-ssh.log")
    }

    /// The names in the state folder that are launch files.
    pub fn launch_files(&self) -> Vec<String> {
        let mut out: Vec<String> = fs::read_dir(&self.state)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.starts_with("board-holder-launch."))
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.reaper.kill_all();
        for _ in 0..20 {
            if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// Polls `f` every 20 ms until it gives `Some`, or the timeout passes.
pub fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() >= end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A file's text, read sharing everything, or empty when it cannot be read.
pub fn read_text(path: &Path) -> String {
    fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// Whether two paths name the same place as Windows compares them: separators, case and a
/// trailing separator ignored.
pub fn same_path(a: &str, b: &Path) -> bool {
    let norm = |s: &str| {
        s.trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    norm(a) == norm(&b.to_string_lossy())
}

// ---------------------------------------------------------------------------------------------
// The fake matcher
// ---------------------------------------------------------------------------------------------

/// How the fake matcher answers `is_holder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accept {
    /// When the line's tokens are the holder command's, the program compared without regard to
    /// case.
    Tokens,
    Always,
    Never,
}

/// A matcher whose holder command is `"<ssh_exe>" <extra>` with the alias after it when
/// `with_alias` is set, so that the stand-in holder can be any program with any arguments.
#[derive(Debug, Clone)]
pub struct FakeMatcher {
    pub extra: String,
    pub with_alias: bool,
    pub accept: Accept,
}

impl FakeMatcher {
    /// The command of one of this module's stand-in holders, `tokens` matching.
    pub fn stub(name: &str) -> FakeMatcher {
        FakeMatcher {
            extra: stub_args(name),
            with_alias: false,
            accept: Accept::Tokens,
        }
    }
}

impl Matcher for FakeMatcher {
    fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String {
        let mut line = format!("\"{}\" {}", ssh_exe.display(), self.extra);
        if self.with_alias {
            line.push(' ');
            line.push_str(alias);
        }
        line
    }

    fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool {
        match self.accept {
            Accept::Always => true,
            Accept::Never => false,
            Accept::Tokens => {
                let want = split_command_line(&self.holder_command(ssh_exe, alias));
                let got = split_command_line(command_line);
                want.len() == got.len()
                    && want.iter().zip(&got).enumerate().all(|(i, (w, g))| {
                        if i == 0 {
                            w.eq_ignore_ascii_case(g)
                        } else {
                            w == g
                        }
                    })
            }
        }
    }

    fn self_check(&self) -> Result<(), String> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Watching the state folder
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Request,
    Claim,
    Result,
}

/// One change to a launch file, in the order the file system reported it.
#[derive(Debug, Clone)]
pub struct Change {
    pub kind: Kind,
    pub hex: String,
    /// A file that appeared, by creation or by a rename onto its name.
    pub added: bool,
    pub at: Instant,
    pub wall_ms: i64,
}

/// What a launch's three files held, read when they appeared.
#[derive(Debug, Clone, Default)]
pub struct Captured {
    pub hex: String,
    pub request: Option<Vec<u8>>,
    pub claim: Option<Vec<u8>>,
    pub result: Option<Vec<u8>>,
}

#[derive(Default)]
struct Seen {
    changes: Vec<Change>,
    captured: Vec<Captured>,
    names: Vec<String>,
}

/// Watches a folder with `ReadDirectoryChangesW`, which reports each creation, rename and removal
/// in order, and reads each launch file as it appears. It also tells the reaper of every holder a
/// launch result names, so a holder the pass never returned is still ended.
pub struct DirWatch {
    seen: Arc<Mutex<Seen>>,
    dir: PathBuf,
    thread: Option<JoinHandle<()>>,
}

const STOP_SENTINEL: &str = "spec-stop.sentinel";
const READY_SENTINEL: &str = "spec-ready.sentinel";

fn classify(name: &str) -> Option<(Kind, String)> {
    let rest = name.strip_prefix("board-holder-launch.")?;
    if rest.ends_with(".tmp") {
        return None;
    }
    if let Some(h) = rest.strip_suffix(".result.json") {
        return Some((Kind::Result, h.to_string()));
    }
    if let Some(h) = rest.strip_suffix(".claim") {
        return Some((Kind::Claim, h.to_string()));
    }
    rest.strip_suffix(".json")
        .map(|h| (Kind::Request, h.to_string()))
}

fn entry_of<'a>(seen: &'a mut Seen, hex: &str) -> &'a mut Captured {
    if let Some(i) = seen.captured.iter().position(|c| c.hex == hex) {
        return &mut seen.captured[i];
    }
    seen.captured.push(Captured {
        hex: hex.to_string(),
        ..Captured::default()
    });
    seen.captured.last_mut().expect("Just pushed.")
}

fn read_retry(path: &Path, whole: impl Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
    let mut last = None;
    for _ in 0..300 {
        if let Ok(b) = fs::read(path)
            && whole(&b)
        {
            return Some(b);
        } else if let Ok(b) = fs::read(path) {
            last = Some(b);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    last
}

fn capture(dir: &Path, name: &str, seen: &Mutex<Seen>, reaper: &Reaper) {
    let Some((kind, hex)) = classify(name) else {
        return;
    };
    let path = dir.join(name);
    let bytes = match kind {
        // A claim is created, then written: wait for its line.
        Kind::Claim => read_retry(&path, |b| b.ends_with(b"\n")),
        _ => read_retry(&path, |b| !b.is_empty()),
    };
    if kind == Kind::Result
        && let Some(b) = &bytes
        && let Ok(LaunchResult::Started {
            pid,
            process_started_at,
            ..
        }) = parse_launch_result(b)
    {
        reaper.track(pid, &process_started_at);
    }
    let mut s = seen.lock().unwrap_or_else(|e| e.into_inner());
    let c = entry_of(&mut s, &hex);
    // The first capture of each file is kept: a late launcher may make a claim of the same name.
    let slot = match kind {
        Kind::Request => &mut c.request,
        Kind::Claim => &mut c.claim,
        Kind::Result => &mut c.result,
    };
    if slot.is_none() {
        *slot = bytes;
    }
}

struct SendHandle(HANDLE);
// SAFETY: A directory handle is used by the one watcher thread only.
unsafe impl Send for SendHandle {}

impl DirWatch {
    /// Starts watching, and returns once the watcher has seen a file made after it began.
    pub fn start(dir: &Path, reaper: &Reaper) -> DirWatch {
        let file = OpenOptions::new()
            .access_mode(0x1) // FILE_LIST_DIRECTORY
            .share_mode(7)
            .custom_flags(0x0200_0000) // FILE_FLAG_BACKUP_SEMANTICS
            .open(dir)
            .expect("The state folder can be opened for watching.");
        let seen = Arc::new(Mutex::new(Seen::default()));
        let (s2, d2, r2) = (seen.clone(), dir.to_path_buf(), reaper.clone());
        let thread = std::thread::spawn(move || {
            let handle = SendHandle(file.as_raw_handle() as HANDLE);
            let mut buf = vec![0u32; 16 * 1024];
            loop {
                let mut got = 0u32;
                // SAFETY: A synchronous call on a directory handle that `file` keeps open, with a
                // DWORD-aligned buffer of the length given.
                let ok = unsafe {
                    ReadDirectoryChangesW(
                        handle.0,
                        buf.as_mut_ptr().cast::<c_void>(),
                        (buf.len() * 4) as u32,
                        0,
                        FILE_NOTIFY_CHANGE_FILE_NAME,
                        &mut got,
                        std::ptr::null_mut(),
                        None,
                    )
                };
                if ok == 0 {
                    break;
                }
                let bytes: &[u8] = {
                    // SAFETY: The buffer holds at least `got` initialized bytes.
                    unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), got as usize) }
                };
                let mut events = Vec::new();
                let mut at = 0usize;
                while at + 12 <= bytes.len() {
                    let next = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
                    let action = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap());
                    let len =
                        u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
                    let units: Vec<u16> = bytes[at + 12..at + 12 + len]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    events.push((action, String::from_utf16_lossy(&units)));
                    if next == 0 {
                        break;
                    }
                    at += next;
                }
                let mut stop = false;
                for (action, name) in events {
                    let now = Instant::now();
                    let wall = now_ms();
                    let added = action == 1 || action == 5;
                    let removed = action == 2;
                    {
                        let mut s = s2.lock().unwrap_or_else(|e| e.into_inner());
                        s.names.push(name.clone());
                        if (added || removed)
                            && let Some((kind, hex)) = classify(&name)
                        {
                            s.changes.push(Change {
                                kind,
                                hex,
                                added,
                                at: now,
                                wall_ms: wall,
                            });
                        }
                    }
                    if added {
                        if name == STOP_SENTINEL {
                            stop = true;
                        } else {
                            capture(&d2, &name, &s2, &r2);
                        }
                    }
                }
                if stop {
                    break;
                }
            }
        });
        let watch = DirWatch {
            seen,
            dir: dir.to_path_buf(),
            thread: Some(thread),
        };
        // Wait until the watcher is armed: it sees a file made after it started.
        let ready = dir.join(READY_SENTINEL);
        for _ in 0..200 {
            let _ = fs::write(&ready, b"x");
            let _ = fs::remove_file(&ready);
            std::thread::sleep(Duration::from_millis(25));
            if watch
                .seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .names
                .iter()
                .any(|n| n == READY_SENTINEL)
            {
                return watch;
            }
        }
        panic!("The watcher never saw a file made after it started.");
    }

    /// Stops the watcher after it has taken in everything reported so far.
    pub fn stop(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = fs::write(self.dir.join(STOP_SENTINEL), b"x");
            let _ = t.join();
            let _ = fs::remove_file(self.dir.join(STOP_SENTINEL));
        }
    }

    /// Every launch file's appearance and removal, in order.
    pub fn changes(&self) -> Vec<Change> {
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .changes
            .clone()
    }

    pub fn captured(&self) -> Vec<Captured> {
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .captured
            .clone()
    }
}

impl Drop for DirWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The index of the first change of this kind, added or removed, in a list of changes.
pub fn index_of(changes: &[Change], kind: Kind, added: bool) -> Option<usize> {
    changes
        .iter()
        .position(|c| c.kind == kind && c.added == added)
}

// ---------------------------------------------------------------------------------------------
// Starting through the starter
// ---------------------------------------------------------------------------------------------

pub const LEASE: &str = "0a1b2c3d-0000-4000-8000-000000000001";
pub const ALIAS: &str = "agentspace-spec-0a1b2c3d";

/// Runs the real starter once, and tells the fixture's reaper of the holder it returns. The
/// watcher it ran under is returned, stopped, with every change it saw.
pub fn start_watched(
    fx: &Fixture,
    own_exe: &Path,
    ssh_exe: &Path,
    matcher: &FakeMatcher,
    lease: &str,
    alias: &str,
) -> (Started, DirWatch) {
    let starter = agentspace_launch::WmiStarter::new();
    let req = StartRequest {
        state_dir: &fx.state,
        profile: &fx.profile,
        ssh_exe,
        lease_id: lease,
        alias,
        own_exe,
        log: None,
    };
    // The watcher tells the reaper of any holder a result names, even one the start fails on.
    let mut watch = DirWatch::start(&fx.state, &fx.reaper);
    let out = starter.start(&req, matcher);
    watch.stop();
    if let Started::Holder(rec) = &out {
        fx.reaper.track(rec.pid, &rec.process_started_at);
    }
    (out, watch)
}

/// The same, without the watcher's changes.
pub fn start_with(
    fx: &Fixture,
    own_exe: &Path,
    ssh_exe: &Path,
    matcher: &FakeMatcher,
    lease: &str,
    alias: &str,
) -> Started {
    start_watched(fx, own_exe, ssh_exe, matcher, lease, alias).0
}

/// The same, with the common stand-in holder and the real stand-in launcher.
pub fn start_stub(fx: &Fixture, name: &str) -> Started {
    let exe = stub_exe();
    start_with(
        fx,
        &testhost(),
        &exe,
        &FakeMatcher::stub(name),
        LEASE,
        ALIAS,
    )
}

pub fn holder_of(out: Started) -> HolderRecord {
    match out {
        Started::Holder(rec) => rec,
        Started::Failed(why) => panic!("The start failed: {why}"),
    }
}

/// Runs the real starter once, as the stand-in holder `stub`, in a process of its own that has the
/// environment of a keeper's pass (`Fixture::pass_environment`) as values given to it when it is
/// started, and not in this spec process, whose environment a spec MUST NOT change (#0137). That
/// process is the spec binary run again as `pass_helper`. The holder's PID and creation time are
/// returned and told to the fixture's reaper, and so is any holder the pass's result names when
/// the start fails.
pub fn start_in_pass_process(fx: &Fixture, stub: &str) -> Result<(u32, String), String> {
    let report_path = fx.root.join(format!("pass-report-{}.txt", now_ms()));
    let mut command = Command::new(stub_exe());
    command
        .raw_arg(stub_args("pass_helper"))
        .env("SPEC_STATE", &fx.state)
        .env("SPEC_PROFILE", &fx.profile)
        .env("SPEC_STUB", stub)
        .env("SPEC_REPORT", &report_path)
        .current_dir(&fx.profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    fx.pass_environment(&mut command);
    let mut watch = DirWatch::start(&fx.state, &fx.reaper);
    let mut child = command
        .spawn()
        .map_err(|_| "The pass's process could not be started.".to_string())?;
    let report = wait_for(Duration::from_secs(90), || {
        fs::read_to_string(&report_path).ok()
    });
    if wait_for(Duration::from_secs(20), || child.try_wait().ok().flatten()).is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    watch.stop();
    let report = report.ok_or_else(|| "The pass's process gave no report.".to_string())?;
    if let Some(why) = report.lines().find_map(|l| l.strip_prefix("failed=")) {
        return Err(why.to_string());
    }
    let line = report
        .lines()
        .find_map(|l| l.strip_prefix("holder="))
        .ok_or_else(|| "The pass's report names no holder.".to_string())?;
    let (pid, started) = line
        .split_once(' ')
        .ok_or_else(|| "The pass's report names no start time.".to_string())?;
    let pid: u32 = pid
        .parse()
        .map_err(|_| "The pass's report names no process ID.".to_string())?;
    fx.reaper.track(pid, started);
    Ok((pid, started.to_string()))
}

// ---------------------------------------------------------------------------------------------
// Files with chosen share modes, and the inheritable handles of the spec process
// ---------------------------------------------------------------------------------------------

/// Opens (or creates) a file for reading and writing with this share mode, and makes its handle
/// inheritable, so that a child created with `bInheritHandles` would carry it.
pub fn open_inheritable(path: &Path, share: u32) -> File {
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(share)
        .open(path)
        .expect("The lock file can be opened.");
    // SAFETY: The handle belongs to `f`, which is alive.
    let ok = unsafe {
        SetHandleInformation(
            f.as_raw_handle() as HANDLE,
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        )
    };
    assert!(ok != 0, "The handle can be made inheritable.");
    f
}

/// Creates a child with `bInheritHandles` set and nothing else restricted, the way a careless
/// launcher would, from the program `exe` and a command line. The child is tracked by the reaper.
pub fn spawn_inheriting_everything(fx: &Fixture, exe: &Path, command_line: &str) -> u32 {
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
    };
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let app = wide(&exe.to_string_lossy());
    let mut cmd = wide(command_line);
    let cwd = wide(&fx.profile.to_string_lossy());
    // SAFETY: All-zero is a valid STARTUPINFOW and PROCESS_INFORMATION before their fields are set.
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: Every pointer names a live, NUL-terminated buffer or a live struct.
    let ok = unsafe {
        CreateProcessW(
            app.as_ptr(),
            cmd.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_NO_WINDOW,
            std::ptr::null(),
            cwd.as_ptr(),
            &si,
            &mut pi,
        )
    };
    assert!(ok != 0, "The control child can be created.");
    // SAFETY: Both handles came from the call above.
    unsafe {
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
    }
    let started = wait_for(Duration::from_secs(5), || {
        look(pi.dwProcessId).map(|(s, _)| s)
    })
    .expect("The control child can be looked at.");
    fx.reaper.track(pi.dwProcessId, &started);
    pi.dwProcessId
}

/// Deletes a file the way a rotation or a cleanup does; whether it was allowed.
pub fn can_delete(path: &Path) -> bool {
    fs::remove_file(path).is_ok()
}

// ---------------------------------------------------------------------------------------------
// Job objects
// ---------------------------------------------------------------------------------------------

/// A job object that ends every process in it when its last handle is closed.
pub struct KillOnCloseJob(HANDLE);

impl KillOnCloseJob {
    pub fn new() -> KillOnCloseJob {
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        // SAFETY: Plain Win32 calls; the information struct is all zero but the limit flag.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            assert!(!job.is_null(), "A job object can be made.");
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            assert!(ok != 0, "The job can be set to end its processes on close.");
            KillOnCloseJob(job)
        }
    }

    /// Puts a child in the job.
    pub fn add(&self, child: &Child) {
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        // SAFETY: Both handles are alive.
        let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) };
        assert!(
            ok != 0,
            "The child can be put in the job (Windows error {}).",
            // SAFETY: Reads the calling thread's last error.
            unsafe { GetLastError() }
        );
    }

    /// Whether the process with this PID is in this job.
    pub fn contains(&self, pid: u32) -> bool {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        // SAFETY: The process handle is closed before returning.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            assert!(
                !h.is_null(),
                "The process can be opened to ask about its job."
            );
            let mut inside = 0;
            let ok = IsProcessInJob(h, self.0, &mut inside);
            CloseHandle(h);
            assert!(ok != 0, "The job of the process can be asked.");
            inside != 0
        }
    }

    /// Closes the job's handle, which ends what is in it.
    pub fn close(self) {
        drop(self);
    }
}

impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        // SAFETY: The handle is the job's own and is closed once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Reports of the stand-in holders
// ---------------------------------------------------------------------------------------------

/// `key=value` lines of a log, as a map of the last value of each key.
pub fn report(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Spawns the report stand-in with a chosen way of being started, as the control that shows a
/// check can see a difference, and returns its report.
pub fn control_report(
    fx: &Fixture,
    flags: u32,
    stdin: Stdio,
    expect: &str,
) -> HashMap<String, String> {
    control_report_in(fx, flags, stdin, expect, false)
}

/// The same, and with `with_pass_environment` the control is given the pass's environment
/// (`Fixture::pass_environment`) as values of its own, the way a pass process has it. A holder
/// that sees a variable of the pass reports it, which is how a check on the holder's environment
/// is shown to be able to see one.
pub fn control_report_in(
    fx: &Fixture,
    flags: u32,
    stdin: Stdio,
    expect: &str,
    with_pass_environment: bool,
) -> HashMap<String, String> {
    use std::os::windows::process::CommandExt;
    let out = fx.root.join(format!("control-{}.txt", now_ms()));
    let file = File::create(&out).expect("The control's output file can be made.");
    let _ = fs::write(fx.profile.join("spec-expect.txt"), expect);
    let mut command = Command::new(stub_exe());
    command
        .raw_arg(stub_args("stub_report"))
        .current_dir(&fx.profile)
        .stdin(stdin)
        .stdout(Stdio::from(
            file.try_clone().expect("The file can be cloned."),
        ))
        .stderr(Stdio::from(file))
        .creation_flags(flags);
    if with_pass_environment {
        fx.pass_environment(&mut command);
    }
    let child = command
        .spawn()
        .expect("The control stand-in can be started.");
    let pid = child.id();
    fx.reaper.track_child(child);
    let got = wait_for(Duration::from_secs(20), || {
        let t = read_text(&out);
        t.contains("ctrl_c_ignored=").then_some(t)
    })
    .unwrap_or_default();
    if let Some((started, _)) = look(pid) {
        kill_verified(pid, &started);
    }
    report(&got)
}

// ---------------------------------------------------------------------------------------------
// The stand-in holders
// ---------------------------------------------------------------------------------------------

fn out(line: &str) {
    println!("{line}");
}

/// Prints a line saying it started, then sleeps up to 120 seconds. The spec ends it.
#[test]
#[ignore = "A stand-in holder that a spec starts as a child process."]
pub fn stub_sleep() {
    out(&format!("stub-start pid={}", std::process::id()));
    std::thread::sleep(Duration::from_secs(120));
}

/// Prints a line per 100 ms on stdout and another on stderr, `tick N` and `err N`, for up to 40
/// seconds, so that a log holds both, in order, and keeps growing.
#[test]
#[ignore = "A stand-in holder that a spec starts as a child process."]
pub fn stub_chatty() {
    out(&format!("stub-start pid={}", std::process::id()));
    for i in 0..400 {
        println!("tick {i}");
        eprintln!("err {i}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Appends a line to `spec-starts.txt` in its working folder, then sleeps 5 seconds: the way a
/// spec counts how many holders a launch made.
#[test]
#[ignore = "A stand-in holder that a spec starts as a child process."]
pub fn stub_marker() {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open("spec-starts.txt")
        .expect("The marker file can be opened.");
    let _ = writeln!(f, "start {}", std::process::id());
    drop(f);
    std::thread::sleep(Duration::from_secs(5));
}

/// Exits at once, with a line in its output.
#[test]
#[ignore = "A stand-in holder that a spec starts as a child process."]
pub fn stub_exit() {
    out(&format!("stub-start pid={}", std::process::id()));
}

#[repr(C)]
struct ProcessBasicInformation {
    exit_status: i32,
    peb: *const u8,
    rest: [usize; 4],
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process: HANDLE,
        class: u32,
        info: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

/// Whether the process was created with Ctrl+C ignored, which is what `CREATE_NEW_PROCESS_GROUP`
/// does to it: the console flags of the process parameters in its own PEB.
fn ctrl_c_ignored() -> Option<bool> {
    if !cfg!(target_pointer_width = "64") {
        return None;
    }
    let mut pbi = ProcessBasicInformation {
        exit_status: 0,
        peb: std::ptr::null(),
        rest: [0; 4],
    };
    let mut returned = 0u32;
    // SAFETY: Class 0 is ProcessBasicInformation, which fills a struct of this layout, for the
    // current process. The PEB of the current process is readable, and its process parameters
    // (offset 0x20) hold the console flags at offset 0x18 on a 64-bit Windows.
    unsafe {
        let status = NtQueryInformationProcess(
            GetCurrentProcess(),
            0,
            (&mut pbi as *mut ProcessBasicInformation).cast::<c_void>(),
            std::mem::size_of::<ProcessBasicInformation>() as u32,
            &mut returned,
        );
        if status != 0 || pbi.peb.is_null() {
            return None;
        }
        let params = *(pbi.peb.add(0x20) as *const *const u8);
        if params.is_null() {
            return None;
        }
        let flags = *(params.add(0x18) as *const u32);
        Some(flags & 1 != 0)
    }
}

fn stdin_kind() -> &'static str {
    // SAFETY: Plain Win32 calls on the process's own standard handle.
    unsafe {
        let h = GetStdHandle(STD_INPUT_HANDLE);
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            return "none";
        }
        match GetFileType(h) {
            2 => {
                let mut mode = 0u32;
                if GetConsoleMode(h, &mut mode) != 0 {
                    return "console";
                }
                let mut byte = [0u8; 1];
                let mut n = 0u32;
                let ok = ReadFile(h, byte.as_mut_ptr(), 1, &mut n, std::ptr::null_mut());
                if (ok != 0 && n == 0) || (ok == 0 && GetLastError() == 38) {
                    "eof"
                } else {
                    "char"
                }
            }
            3 => "pipe",
            1 => "file",
            _ => "other",
        }
    }
}

/// Reports, as `key=value` lines on stdout, what a holder was started with: its working folder,
/// what its stdin is, whether its stdout and stderr are one handle, whether it has a console window,
/// whether it was started in a new process group, and which variables it sees. It never prints a
/// value of the environment. Then it sleeps up to 30 seconds.
///
/// `spec-expect.txt` in its working folder, when there is one, names the `USERPROFILE` of the
/// spec's own process on its first line.
#[test]
#[ignore = "A stand-in holder that a spec starts as a child process."]
pub fn stub_report() {
    out(&format!("stub-start pid={}", std::process::id()));
    let cwd = std::env::current_dir().unwrap_or_default();
    out(&format!("cwd={}", cwd.display()));
    out(&format!("stdin={}", stdin_kind()));
    // SAFETY: Plain Win32 calls on the process's own standard handles and console.
    let (same, window) = unsafe {
        (
            GetStdHandle(STD_OUTPUT_HANDLE) == GetStdHandle(STD_ERROR_HANDLE),
            GetConsoleWindow().is_null(),
        )
    };
    out(&format!("stdout_is_stderr={same}"));
    out(&format!(
        "console_window={}",
        if window { "none" } else { "present" }
    ));
    out(&format!(
        "probe_var={}",
        if std::env::var_os(PROBE_VAR).is_some() {
            "present"
        } else {
            "absent"
        }
    ));
    out(&format!(
        "state_dir_var={}",
        if std::env::var_os("AGENTSPACE_STATE_DIR").is_some() {
            "present"
        } else {
            "absent"
        }
    ));
    let expected = fs::read_to_string("spec-expect.txt").ok();
    let mine = std::env::var("USERPROFILE").ok();
    out(&format!(
        "userprofile_is_the_passes={}",
        match (&expected, &mine) {
            (Some(e), Some(m)) =>
                (e.lines().next().unwrap_or("").eq_ignore_ascii_case(m)).to_string(),
            _ => "unknown".to_string(),
        }
    ));
    out(&format!(
        "ctrl_c_ignored={}",
        ctrl_c_ignored().map_or("unknown".to_string(), |b| b.to_string())
    ));
    std::thread::sleep(Duration::from_secs(30));
}

// ---------------------------------------------------------------------------------------------
// The process that asks for a start from inside a job (spec_survives_job.rs)
// ---------------------------------------------------------------------------------------------

/// The pass of `spec_survives_job`: it waits for the go file, starts a control child in the same
/// job, starts a holder through the real starter, writes what it made to the report file, and
/// waits to be ended with the job. It reads its paths from variables only it was given.
#[test]
#[ignore = "The process a spec puts in a job object; it is started by that spec."]
pub fn job_helper() {
    let var = |k: &str| PathBuf::from(std::env::var_os(k).expect("The spec gave this variable."));
    let (state, profile, go, report_path) = (
        var("SPEC_STATE"),
        var("SPEC_PROFILE"),
        var("SPEC_GO"),
        var("SPEC_REPORT"),
    );
    assert!(
        wait_for(Duration::from_secs(60), || go.exists().then_some(())).is_some(),
        "The go file came."
    );
    let control = Command::new(stub_exe())
        .raw_arg(stub_args("stub_sleep"))
        .current_dir(&profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("The control child can be started.");
    let control_pid = control.id();
    let control_started =
        wait_for(Duration::from_secs(5), || look(control_pid).map(|(s, _)| s)).unwrap_or_default();
    let testhost = testhost();
    let matcher = FakeMatcher {
        extra: "--sleep 120".to_string(),
        with_alias: false,
        accept: Accept::Tokens,
    };
    let starter = agentspace_launch::WmiStarter::new();
    let req = StartRequest {
        state_dir: &state,
        profile: &profile,
        ssh_exe: &testhost,
        lease_id: LEASE,
        alias: ALIAS,
        own_exe: &testhost,
        log: None,
    };
    let mut text = format!("control={control_pid} {control_started}\n");
    match starter.start(&req, &matcher) {
        Started::Holder(rec) => {
            text.push_str(&format!("holder={} {}\n", rec.pid, rec.process_started_at));
        }
        Started::Failed(why) => text.push_str(&format!("failed={why}\n")),
    }
    let tmp = report_path.with_extension("tmp");
    fs::write(&tmp, text).expect("The report can be written.");
    fs::rename(&tmp, &report_path).expect("The report can be put in place.");
    std::thread::sleep(Duration::from_secs(90));
    drop(control);
}

/// The pass of `start_in_pass_process`: it starts one holder, the stand-in named by `SPEC_STUB`,
/// through the real starter, writes what it made to the report file, and exits. It reads its paths
/// from variables only it was given, and the environment it runs in is the one its spec gave it.
#[test]
#[ignore = "The process a spec starts as a pass; it is started by that spec."]
pub fn pass_helper() {
    let text_of = |k: &str| std::env::var(k).expect("The spec gave this variable.");
    let (state, profile, report_path, stub) = (
        PathBuf::from(text_of("SPEC_STATE")),
        PathBuf::from(text_of("SPEC_PROFILE")),
        PathBuf::from(text_of("SPEC_REPORT")),
        text_of("SPEC_STUB"),
    );
    let (exe, testhost) = (stub_exe(), testhost());
    let req = StartRequest {
        state_dir: &state,
        profile: &profile,
        ssh_exe: &exe,
        lease_id: LEASE,
        alias: ALIAS,
        own_exe: &testhost,
        log: None,
    };
    let text = match agentspace_launch::WmiStarter::new().start(&req, &FakeMatcher::stub(&stub)) {
        Started::Holder(rec) => format!("holder={} {}\n", rec.pid, rec.process_started_at),
        Started::Failed(why) => format!("failed={why}\n"),
    };
    let tmp = report_path.with_extension("tmp");
    fs::write(&tmp, text).expect("The report can be written.");
    fs::rename(&tmp, &report_path).expect("The report can be put in place.");
}

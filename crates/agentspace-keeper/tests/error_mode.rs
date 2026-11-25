//! The keeper's process error mode (#0225, #0226, D81; docs/keeper.md, "Streams and environment",
//! "Starting the guard" and "The keeper's lines for it").
//!
//! The test starts the built keeper from a parent whose error mode is 0
//! (`CREATE_DEFAULT_ERROR_MODE`), which is how a conformance keeper file starts it with
//! `Start-Process -WindowStyle Hidden`, with `AGENTSPACE_GUARD_CMD` naming a file called `.exe`
//! whose first bytes are not `MZ`. Windows treats such a file as a 16-bit program, and a process at
//! error mode 0 that starts one waits at a modal "Unsupported 16-Bit Application" dialog for a
//! person. The keeper MUST instead make its one attempt at the guard at once, fail it with its own
//! line, and go on (keeper.md, What a failed start changes).
//!
//! The test ends the process it started by its own PID, under a timeout, and works only in a
//! throwaway home. A guard that did start, which the file is meant to make impossible, is ended by
//! the PID the keeper's line gives, and fails the test.
#![cfg(windows)]

use std::fs;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The process creation flag that gives the child error mode 0, whatever the parent's is.
const CREATE_DEFAULT_ERROR_MODE: u32 = 0x0400_0000;

/// How long the keeper has to make its one pass. A dialog never ends by itself, so the margin only
/// has to cover a loaded machine.
const LIMIT: Duration = Duration::from_secs(40);

/// A throwaway home that is removed when it goes out of scope.
struct Home {
    root: PathBuf,
}

impl Home {
    fn new(name: &str) -> Home {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("error-mode-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("The throwaway home is made");
        Home { root }
    }

    /// A path inside the home, written with `\` between its parts as Windows and the program
    /// under test write one.
    fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    /// Makes a folder inside the home and returns it.
    fn folder(&self, relative: &str) -> PathBuf {
        let folder = self.path(relative);
        fs::create_dir_all(&folder).expect("A folder of the throwaway home is made");
        folder
    }

    /// Writes a file inside the home and returns its path.
    fn file(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let file = self.path(relative);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).expect("A folder of the throwaway home is made");
        }
        fs::write(&file, bytes).expect("A file of the throwaway home is written");
        file
    }

    /// Hides the home's path in text that is shown.
    fn mask(&self, text: &str) -> String {
        text.replace(self.root.to_string_lossy().as_ref(), "<home>")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// A file named `.exe` whose first bytes are not `MZ`.
fn not_a_program(home: &Home, relative: &str) -> PathBuf {
    home.file(relative, b"This is text, not a Windows program.\r\n")
}

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Ends the child by its own handle when the test leaves, whatever the outcome.
struct EndOnDrop(Child);

impl Drop for EndOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn drain(mut pipe: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    receiver
}

/// Runs `cmd` at error mode 0 and returns how it ended. `None` for the code means that the child
/// had not exited within `LIMIT`, and was ended by its own handle.
fn run_at_error_mode_0(mut cmd: Command) -> Outcome {
    cmd.creation_flags(CREATE_DEFAULT_ERROR_MODE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = EndOnDrop(cmd.spawn().expect("The keeper starts"));
    let stdout = drain(child.0.stdout.take().expect("A piped stdout"));
    let stderr = drain(child.0.stderr.take().expect("A piped stderr"));
    let deadline = Instant::now() + LIMIT;
    let code = loop {
        match child.0.try_wait().expect("The child can be waited for") {
            Some(status) => break Some(status.code().unwrap_or(-1)),
            None if Instant::now() >= deadline => {
                let _ = child.0.kill();
                let _ = child.0.wait();
                break None;
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    };
    let text = |receiver: mpsc::Receiver<String>| {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_default()
    };
    Outcome {
        code,
        stdout: text(stdout),
        stderr: text(stderr),
    }
}

/// The PID of a guard that the keeper says it started (`<time> guard started pid=<pid>`), if any.
fn started_guard_pid(stdout: &str) -> Option<u32> {
    stdout.lines().find_map(|line| {
        let (_, tail) = line.split_once(" guard started pid=")?;
        tail.trim().parse().ok()
    })
}

/// Whether the keeper made its attempt at the guard and failed it with its own line, as it MUST for
/// a program that cannot be started (keeper.md, The keeper's lines for it), and went on to exit 0
/// at `-MaxPasses` (What a failed start changes). Pure, so that a planted outcome can show that it
/// rejects.
///
/// The line is `<time> failed lease=- alias=- pid=- reason=The guard could not be started: <why>`,
/// and for a program that exists, `<why>` is `The detached start failed: <error>`.
fn judge(outcome: &Outcome) -> Result<(), String> {
    let Some(code) = outcome.code else {
        return Err(format!(
            "No exit within {} s: a hard-error dialog likely",
            LIMIT.as_secs()
        ));
    };
    if let Some(pid) = started_guard_pid(&outcome.stdout) {
        return Err(format!(
            "The keeper says it started a guard from a file that is not a program (pid {pid})"
        ));
    }
    let wanted = " failed lease=- alias=- pid=- reason=The guard could not be started: The detached start failed: ";
    let failed = outcome
        .stdout
        .lines()
        .filter(|line| line.contains(wanted) && !line.ends_with(wanted))
        .count();
    if failed != 1 {
        return Err(format!(
            "{failed} lines say the detached start failed, not exactly 1"
        ));
    }
    if code != 0 {
        return Err(format!("The exit code is {code}, not 0"));
    }
    if !outcome.stderr.is_empty() {
        return Err("The keeper wrote to stderr".to_string());
    }
    Ok(())
}

/// Ends a guard the keeper started by its PID, which the keeper's own line gave. It is the only
/// process this test touches that it did not start itself, and only if a guard was started, which
/// the test then reports as a failure.
fn end_started_guard(stdout: &str) {
    if let Some(pid) = started_guard_pid(stdout) {
        let root = std::env::var_os("SystemRoot").unwrap_or_default();
        let _ = Command::new(Path::new(&root).join("System32").join("taskkill.exe"))
            .args(["/PID", &pid.to_string(), "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn a_guard_that_is_not_a_program_fails_its_attempt_at_once() {
    let home = Home::new("keeper");
    let guard = not_a_program(&home, "bin/guard.exe");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentspace-keeper"));
    cmd.args(["-Watch", "-MaxPasses", "1", "-IntervalSeconds", "1"])
        .current_dir(home.folder("work"))
        .env("USERPROFILE", &home.root)
        .env("HOME", &home.root)
        .env("LOCALAPPDATA", home.folder("local"))
        .env("APPDATA", home.folder("roaming"))
        .env("AGENTSPACE_STATE_DIR", home.folder("state"))
        .env("AGENTSPACE_SSH_DIR", home.folder("ssh"))
        // Every program a keeper could start is named as a stand-in, the guard first: a check that
        // runs a keeper MUST name its guard (keeper.md, Checking the keeper).
        .env("AGENTSPACE_GUARD_CMD", &guard)
        .env("AGENTSPACE_AWS_CMD", &guard)
        .env("AGENTSPACE_SSH_CMD", &guard)
        .env("AGENTSPACE_SCP_CMD", &guard);
    for name in [
        "AGENTSPACE_NESTED",
        "AGENTSPACE_DIAGNOSTIC",
        "AGENTSPACE_PROXY_COMMAND",
        "AGENTSPACE_TIMEZONE",
    ] {
        cmd.env_remove(name);
    }
    let outcome = run_at_error_mode_0(cmd);
    end_started_guard(&outcome.stdout);
    if let Err(reason) = judge(&outcome) {
        panic!(
            "{reason}\nexit: {:?}\nstdout: {}\nstderr: {}",
            outcome.code,
            home.mask(&outcome.stdout),
            home.mask(&outcome.stderr)
        );
    }
}

// The checker is proven on planted outcomes: each one is wrong in one way, and each is rejected.

fn planted(code: Option<i32>, stdout: &str, stderr: &str) -> Outcome {
    Outcome {
        code,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

#[test]
fn the_judge_rejects_planted_outcomes() {
    let failed = "2026-11-24T20:00:00Z failed lease=- alias=- pid=- reason=The guard could not be started: The detached start failed: Windows error 193.\n";
    let idle = "2026-11-24T20:00:00Z idle\n";
    assert_eq!(
        judge(&planted(Some(0), &format!("{failed}{idle}"), "")),
        Ok(())
    );
    for bad in [
        planted(None, failed, ""),
        planted(Some(1), failed, ""),
        planted(Some(0), failed, "agentspace-keeper: an error\n"),
        planted(Some(0), idle, ""),
        planted(Some(0), &format!("{failed}{failed}"), ""),
        planted(
            Some(0),
            "2026-11-24T20:00:00Z failed lease=- alias=- pid=- reason=The guard could not be started: The detached start failed: \n",
            "",
        ),
        planted(
            Some(0),
            "2026-11-24T20:00:00Z failed lease=- alias=- pid=- reason=The guard could not be started: The operator program is not installed beside this program.\n",
            "",
        ),
        planted(
            Some(0),
            &format!("{failed}2026-11-24T20:00:01Z guard started pid=4242\n"),
            "",
        ),
    ] {
        assert!(judge(&bad).is_err());
    }
    assert_eq!(
        started_guard_pid("2026-11-24T20:00:01Z guard started pid=4242\n"),
        Some(4242)
    );
    assert_eq!(
        started_guard_pid("2026-11-24T20:00:01Z guard started pid=-\n"),
        None
    );
}

#[test]
fn the_planted_file_is_not_a_program() {
    let home = Home::new("planted-file");
    let file = not_a_program(&home, "bin/guard.exe");
    let bytes = fs::read(&file).unwrap();
    assert_ne!(&bytes[..2], b"MZ");
    assert_eq!(
        file.extension().and_then(|extension| extension.to_str()),
        Some("exe")
    );
}

// ---------------------------------------------------------------------------------------------
// The process's own error mode: the program's `main` MUST set `SEM_FAILCRITICALERRORS |
// SEM_NOOPENFILEERRORBOX` before it starts or opens anything (#0225, D81; the rule is stated in
// docs/operator.md, Programs it starts, and docs/keeper.md, Streams and environment). A test of a
// start above can pass because a person pressed OK at the dialog, or because that start is one that
// another guard already covers; the mode is read from outside, from the finished process's handle,
// and so depends on neither.
// ---------------------------------------------------------------------------------------------

/// `SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX`.
const WANTED_MODE: u32 = 0x8001;

/// `NtQueryInformationProcess`'s class `ProcessDefaultHardErrorMode`.
const PROCESS_DEFAULT_HARD_ERROR_MODE: u32 = 12;

// This module is the only code of the file that calls Windows beside the credential's, if any.
mod win {
    use super::PROCESS_DEFAULT_HARD_ERROR_MODE;
    use std::ffi::c_void;

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process: *mut c_void,
            class: u32,
            info: *mut c_void,
            length: u32,
            returned: *mut u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetErrorMode(mode: u32) -> u32;
        fn GetCurrentProcess() -> *mut c_void;
    }

    /// The error mode of the process `handle` names, in `SetErrorMode`'s form, or the failing
    /// status.
    ///
    /// The kernel keeps `SEM_FAILCRITICALERRORS` inverted in the value it reports
    /// (`ProcessDefaultHardErrorMode`: bit 0 set means that a hard error is shown), as `kernel32`
    /// does when it reads and sets the mode, so the first bit is flipped here.
    pub fn error_mode_of(handle: *mut c_void) -> Result<u32, i32> {
        let mut raw = 0u32;
        let mut returned = 0u32;
        // SAFETY: `raw` is a `u32`, the size the class gives, and outlives the call; `handle` is a
        // process handle that the caller holds open.
        let status = unsafe {
            NtQueryInformationProcess(
                handle,
                PROCESS_DEFAULT_HARD_ERROR_MODE,
                (&raw mut raw).cast(),
                4,
                &raw mut returned,
            )
        };
        if status < 0 { Err(status) } else { Ok(raw ^ 1) }
    }

    /// This process's own error mode, read through its pseudo handle.
    pub fn own_error_mode() -> Result<u32, i32> {
        // SAFETY: `GetCurrentProcess` takes nothing and returns a pseudo handle that needs no close.
        error_mode_of(unsafe { GetCurrentProcess() })
    }

    /// Sets this process's error mode and returns the previous one.
    pub fn set_own_error_mode(mode: u32) -> u32 {
        // SAFETY: `SetErrorMode` takes a bit set and has no other effect than the mode.
        unsafe { SetErrorMode(mode) }
    }
}

/// Runs `cmd` at error mode 0 to its end (or `LIMIT`, when it is ended by its own handle), and
/// returns its exit code and its error mode read through its handle.
fn exit_code_and_error_mode(mut cmd: Command) -> (Option<i32>, Result<u32, i32>) {
    use std::os::windows::io::AsRawHandle;
    cmd.creation_flags(CREATE_DEFAULT_ERROR_MODE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = EndOnDrop(cmd.spawn().expect("The program starts"));
    let deadline = Instant::now() + LIMIT;
    let code = loop {
        match child.0.try_wait().expect("The child can be waited for") {
            Some(status) => break Some(status.code().unwrap_or(-1)),
            None if Instant::now() >= deadline => {
                let _ = child.0.kill();
                let _ = child.0.wait();
                break None;
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    (code, win::error_mode_of(child.0.as_raw_handle()))
}

/// Whether the error mode read is the wanted one, or why not. Pure, so that planted values can show
/// that it rejects.
fn judge_mode(mode: Result<u32, i32>) -> Result<(), String> {
    match mode {
        Ok(WANTED_MODE) => Ok(()),
        Ok(mode) => Err(format!(
            "The error mode is {mode:#x}, not {WANTED_MODE:#x}: a start of a file that is not a program would wait at a dialog"
        )),
        Err(status) => Err(format!(
            "The error mode could not be read: status {status:#x}"
        )),
    }
}

#[test]
fn the_probe_reads_the_mode_of_a_process_that_sets_it_and_of_one_that_does_not() {
    // This process's own mode: set to the wanted one, read back, and put back as it was.
    let before = win::set_own_error_mode(WANTED_MODE);
    let own = win::own_error_mode();
    win::set_own_error_mode(before);
    assert_eq!(own, Ok(WANTED_MODE));

    // A process that sets nothing, started at error mode 0, reads as 0.
    let home = Home::new("calibration");
    let root = std::env::var_os("SystemRoot").expect("SystemRoot is set");
    let mut cmd = Command::new(Path::new(&root).join("System32").join("cmd.exe"));
    cmd.args(["/d", "/c", "exit 0"])
        .current_dir(home.folder("work"));
    assert_eq!(exit_code_and_error_mode(cmd), (Some(0), Ok(0)));
}

#[test]
fn the_mode_judge_rejects_planted_values() {
    assert_eq!(judge_mode(Ok(0x8001)), Ok(()));
    for bad in [Ok(0), Ok(0x8000), Ok(1), Ok(0x8003), Err(-1)] {
        assert!(judge_mode(bad).is_err());
    }
}

#[test]
fn the_keeper_sets_its_error_mode_first() {
    let home = Home::new("mode");
    // An argument it does not know is a usage error: exit 2, nothing logged, nothing started
    // (keeper.md, Usage errors).
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentspace-keeper"));
    cmd.arg("-Bogus")
        .current_dir(home.folder("work"))
        .env("USERPROFILE", &home.root)
        .env("HOME", &home.root)
        .env("LOCALAPPDATA", home.folder("local"))
        .env("APPDATA", home.folder("roaming"))
        .env("AGENTSPACE_STATE_DIR", home.folder("state"));
    let (code, mode) = exit_code_and_error_mode(cmd);
    assert_eq!(code, Some(2));
    if let Err(reason) = judge_mode(mode) {
        panic!("{reason}");
    }
}

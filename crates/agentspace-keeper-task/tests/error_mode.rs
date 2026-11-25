//! The task launcher's process error mode (#0225, #0226, D81; docs/keeper.md, "The task's
//! launcher" and "Streams and environment").
//!
//! The test copies the built launcher into a throwaway folder, beside a file named
//! `agentspace-keeper.exe` whose first bytes are not `MZ`, and starts the copy from a parent whose
//! error mode is 0 (`CREATE_DEFAULT_ERROR_MODE`). Windows treats such a file as a 16-bit program,
//! and a process at error mode 0 that starts one waits at a modal "Unsupported 16-Bit Application"
//! dialog for a person. The launcher MUST instead exit 1 at once, logging `Could not start
//! <keeper's full path>: <kind>: <message>` (keeper.md, The task's launcher).
//!
//! The test ends the process it started by its own PID, under a timeout, and works only in a
//! throwaway home.
//!
//! **What this does not measure:** the error mode a Task Scheduler task gives the launcher. That
//! needs a registered task, which is Task Scheduler's and a person's yes
//! (docs/keeper.md, The installed task), and no spec line measures it. The launcher's own `main`
//! is what keeps a launcher at error mode 0 from waiting, whatever mode it inherits, and this test
//! starts it at 0.
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

/// How long the launcher has to fail. A dialog never ends by itself, so the margin only has to
/// cover a loaded machine.
const LIMIT: Duration = Duration::from_secs(40);

/// The launcher's error log, in the state folder (keeper.md, The task's launcher).
const ERROR_LOG: &str = "board-holder-launcher-errors.log";

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

/// How the launcher ended: its exit code, `None` when it had not exited within `LIMIT` and was
/// ended by its own handle, and what it wrote to its streams, which it has none of but may write.
struct Outcome {
    code: Option<i32>,
    streams: String,
}

fn run_at_error_mode_0(mut cmd: Command) -> Outcome {
    cmd.creation_flags(CREATE_DEFAULT_ERROR_MODE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = EndOnDrop(cmd.spawn().expect("The launcher starts"));
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
        streams: format!("{}{}", text(stdout), text(stderr)),
    }
}

/// Whether the launcher failed as it MUST, or why not, given its exit code, its error log's text
/// and the keeper's full path. Pure, so that a planted outcome can show that it rejects.
///
/// The log has one line, `<time> Could not start <keeper's full path>: <kind>: <message>`, where
/// `<time>` is UTC in ISO 8601 with seven fractional digits and a `Z`, and `<kind>` is the error
/// kind's name (keeper.md, The task's launcher; state.md, The keeper's logs). The launcher exits 1
/// and writes nothing to its streams.
fn judge(code: Option<i32>, streams: &str, log: &str, keeper: &Path) -> Result<(), String> {
    let Some(code) = code else {
        return Err(format!(
            "No exit within {} s: a hard-error dialog likely",
            LIMIT.as_secs()
        ));
    };
    if code != 1 {
        return Err(format!("The exit code is {code}, not 1"));
    }
    if !streams.is_empty() {
        return Err("The launcher wrote to a stream".to_string());
    }
    let lines: Vec<&str> = log.lines().collect();
    let [line] = lines.as_slice() else {
        return Err(format!("The log has {} lines, not 1", lines.len()));
    };
    let Some((time, message)) = line.split_once(' ') else {
        return Err("The log line has no time".to_string());
    };
    if !is_log_time(time) {
        return Err("The log line's time is not an ISO 8601 UTC time of seven digits".to_string());
    }
    let Some(rest) = message.strip_prefix("Could not start ") else {
        return Err("The log line is not a `Could not start` line".to_string());
    };
    // The keeper's path as the launcher wrote it, with the verbatim prefix Windows may add and its
    // case ignored, as Windows ignores it.
    let wanted = keeper.to_string_lossy().to_lowercase();
    let Some(after_path) = rest
        .to_lowercase()
        .strip_prefix(&wanted)
        .map(str::to_string)
        .or_else(|| {
            rest.to_lowercase()
                .strip_prefix(&format!(r"\\?\{wanted}"))
                .map(str::to_string)
        })
    else {
        return Err("The log line does not name the keeper's full path".to_string());
    };
    let original_tail = &rest[rest.len() - after_path.len()..];
    let Some(kind_and_message) = original_tail.strip_prefix(": ") else {
        return Err("The path is not followed by a colon".to_string());
    };
    let Some((kind, error)) = kind_and_message.split_once(": ") else {
        return Err("The kind is not followed by a message".to_string());
    };
    if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("The kind is not an error kind's name".to_string());
    }
    if error.trim().is_empty() {
        return Err("The error has no message".to_string());
    }
    Ok(())
}

/// `2026-11-18T05:07:00.1234567Z`.
fn is_log_time(time: &str) -> bool {
    let bytes = time.as_bytes();
    bytes.len() == 28
        && bytes.iter().enumerate().all(|(i, byte)| match i {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 | 16 => *byte == b':',
            19 => *byte == b'.',
            27 => *byte == b'Z',
            _ => byte.is_ascii_digit(),
        })
}

#[test]
fn a_keeper_that_is_not_a_program_fails_the_launcher_at_once() {
    let home = Home::new("launcher");
    let install = home.folder("install");
    let launcher = install.join("agentspace-keeper-task.exe");
    fs::copy(env!("CARGO_BIN_EXE_agentspace-keeper-task"), &launcher)
        .expect("The built launcher is copied into the throwaway folder");
    let keeper = not_a_program(&home, "install/agentspace-keeper.exe");
    let state = home.folder("state");
    let mut cmd = Command::new(&launcher);
    cmd.arg("-Watch")
        .current_dir(home.folder("work"))
        .env("USERPROFILE", &home.root)
        .env("HOME", &home.root)
        .env("LOCALAPPDATA", home.folder("local"))
        .env("APPDATA", home.folder("roaming"))
        .env("AGENTSPACE_STATE_DIR", &state)
        .env("AGENTSPACE_SSH_DIR", home.folder("ssh"))
        // A keeper that did start would find a guard that is not a program too.
        .env("AGENTSPACE_GUARD_CMD", &keeper);
    let outcome = run_at_error_mode_0(cmd);
    let log = fs::read(state.join(ERROR_LOG))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    if let Err(reason) = judge(outcome.code, &outcome.streams, &log, &keeper) {
        panic!(
            "{reason}\nexit: {:?}\nstreams: {}\nlog: {}",
            outcome.code,
            home.mask(&outcome.streams),
            home.mask(&log)
        );
    }
}

// The checker is proven on planted outcomes: each one is wrong in one way, and each is rejected.

#[test]
fn the_judge_rejects_planted_outcomes() {
    let keeper = Path::new(r"C:\throwaway\install\agentspace-keeper.exe");
    let good = "2026-11-24T20:00:00.1234567Z Could not start C:\\throwaway\\install\\agentspace-keeper.exe: Uncategorized: The operating system cannot run %1. (os error 216)\r\n";
    assert_eq!(judge(Some(1), "", good, keeper), Ok(()));
    // The verbatim prefix and a different case name the same file.
    let verbatim = good.replace("Could not start C:", r"Could not start \\?\c:");
    assert_eq!(judge(Some(1), "", &verbatim, keeper), Ok(()));
    let other = good.replace("install", "elsewhere");
    let no_kind = good.replace("Uncategorized: ", "");
    let no_time = good.replace("2026-11-24T20:00:00.1234567Z ", "");
    let short_time = good.replace(".1234567Z", ".12345Z");
    let no_message = good.replace("The operating system cannot run %1. (os error 216)", " ");
    let waited = good.replace("Could not start", "Could not wait for");
    let twice = format!("{good}{good}");
    for (code, streams, log) in [
        (None, "", good),
        (Some(0), "", good),
        (Some(2), "", good),
        (Some(1), "text", good),
        (Some(1), "", ""),
        (Some(1), "", other.as_str()),
        (Some(1), "", no_kind.as_str()),
        (Some(1), "", no_time.as_str()),
        (Some(1), "", short_time.as_str()),
        (Some(1), "", no_message.as_str()),
        (Some(1), "", waited.as_str()),
        (Some(1), "", twice.as_str()),
    ] {
        assert!(
            judge(code, streams, log, keeper).is_err(),
            "A planted outcome was accepted: {code:?} {streams:?} {log:?}"
        );
    }
}

#[test]
fn the_planted_file_is_not_a_program() {
    let home = Home::new("planted-file");
    let file = not_a_program(&home, "install/agentspace-keeper.exe");
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
fn the_launcher_sets_its_error_mode_first() {
    let home = Home::new("mode");
    let install = home.folder("install");
    let launcher = install.join("agentspace-keeper-task.exe");
    fs::copy(env!("CARGO_BIN_EXE_agentspace-keeper-task"), &launcher)
        .expect("The built launcher is copied into the throwaway folder");
    // An argument that holds a quote is refused with exit 2 before anything is started
    // (keeper.md, The task's launcher).
    let mut cmd = Command::new(&launcher);
    cmd.arg("a\"b")
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

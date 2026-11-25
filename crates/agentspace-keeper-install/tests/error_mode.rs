//! The installer's process error mode (#0225, #0226, D81; docs/keeper.md, "Streams and
//! environment": the keeper's, the task launcher's and the installer's `main` MUST each set the
//! process's error mode to `SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX` first).
//!
//! **The installer is given no program to start**: it registers and starts a scheduled task through
//! Task Scheduler and starts no process of its own (keeper.md, The installer, "A real install"
//! steps 4 and 5), and no check runs it (conformance.md, FB-25). So the dialog of #0225 cannot be
//! made from outside by pointing it at a file that is not a program. What can be shown from outside
//! is the rule itself: the installer, started at error mode 0 (`CREATE_DEFAULT_ERROR_MODE`), has
//! error mode `0x8001` once it has run. The test runs the one invocation that reaches no Task
//! Scheduler call and changes nothing, a usage error (`-Bogus`, keeper.md, The installer, usage
//! errors), and reads the finished process's error mode through its handle.
//!
//! The probe is calibrated first, against a process whose mode the test sets itself and against one
//! that sets nothing, so that a reading of 0 or of `0x8001` means what it says.
//!
//! The test ends every process it started by its own PID, under a timeout, and works only in a
//! throwaway home.
#![cfg(windows)]

use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The process creation flag that gives the child error mode 0, whatever the parent's is.
const CREATE_DEFAULT_ERROR_MODE: u32 = 0x0400_0000;

/// `SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX`.
const WANTED_MODE: u32 = 0x8001;

/// How long a child has to finish. The installer's usage error takes milliseconds.
const LIMIT: Duration = Duration::from_secs(40);

/// `NtQueryInformationProcess`'s class `ProcessDefaultHardErrorMode`.
const PROCESS_DEFAULT_HARD_ERROR_MODE: u32 = 12;

// This module is the only code of the file that calls Windows.
mod win {
    use super::{PROCESS_DEFAULT_HARD_ERROR_MODE, c_void};

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

    /// Makes a folder inside the home and returns it.
    fn folder(&self, relative: &str) -> PathBuf {
        let folder = relative
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part));
        fs::create_dir_all(&folder).expect("A folder of the throwaway home is made");
        folder
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
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

/// What a finished child left: its exit code and its streams' text, and its error mode as read
/// through its handle while the handle was still open.
struct Finished {
    code: Option<i32>,
    streams: String,
    mode: Result<u32, i32>,
}

/// Runs `cmd` at error mode 0 and, once it has exited (a child that has not exited within `LIMIT`
/// is ended by its own handle and has no code), reads its error mode.
fn run_at_error_mode_0(mut cmd: Command) -> Finished {
    cmd.creation_flags(CREATE_DEFAULT_ERROR_MODE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = EndOnDrop(cmd.spawn().expect("The child starts"));
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
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let mode = win::error_mode_of(child.0.as_raw_handle());
    let text = |receiver: mpsc::Receiver<String>| {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_default()
    };
    Finished {
        code,
        streams: format!("{}{}", text(stdout), text(stderr)),
        mode,
    }
}

/// The installer's usage error for an argument it does not know (keeper.md, The installer): two
/// lines on stderr, exit 2, nothing changed.
fn installer_with_a_usage_error(home: &Home) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentspace-keeper-install"));
    cmd.arg("-Bogus")
        .current_dir(home.folder("work"))
        .env("USERPROFILE", &home.root)
        .env("HOME", &home.root)
        .env("LOCALAPPDATA", home.folder("local"))
        .env("APPDATA", home.folder("roaming"))
        .env("AGENTSPACE_STATE_DIR", home.folder("state"));
    cmd
}

/// Whether a finished installer is as it MUST be, or why not. Pure, so that a planted outcome can
/// show that it rejects.
fn judge(finished: &Finished) -> Result<(), String> {
    let Some(code) = finished.code else {
        return Err(format!("No exit within {} s", LIMIT.as_secs()));
    };
    if code != 2 {
        return Err(format!("The exit code is {code}, not 2"));
    }
    if !finished
        .streams
        .starts_with("agentspace-keeper-install: Unknown argument '-Bogus'.")
    {
        return Err("The usage error is not the installer's".to_string());
    }
    match finished.mode {
        Ok(WANTED_MODE) => Ok(()),
        Ok(mode) => Err(format!(
            "The installer's error mode is {mode:#x}, not {WANTED_MODE:#x}: a start of a file that is not a program would wait at a dialog"
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
    let finished = run_at_error_mode_0(cmd);
    assert_eq!(finished.code, Some(0));
    assert_eq!(finished.mode, Ok(0));
}

#[test]
fn the_installer_sets_its_error_mode_first() {
    let home = Home::new("installer");
    let finished = run_at_error_mode_0(installer_with_a_usage_error(&home));
    if let Err(reason) = judge(&finished) {
        panic!(
            "{reason}\nexit: {:?}\nmode: {:?}\nstreams: {}",
            finished.code,
            finished.mode,
            finished
                .streams
                .replace(home.root.to_string_lossy().as_ref(), "<home>")
        );
    }
}

// The checker is proven on planted outcomes: each one is wrong in one way, and each is rejected.

#[test]
fn the_judge_rejects_planted_outcomes() {
    let usage = "agentspace-keeper-install: Unknown argument '-Bogus'.\nUsage: agentspace-keeper-install [-DryRun | -Uninstall]\n";
    let finished = |code, streams: &str, mode| Finished {
        code,
        streams: streams.to_string(),
        mode,
    };
    assert_eq!(judge(&finished(Some(2), usage, Ok(0x8001))), Ok(()));
    for bad in [
        finished(None, usage, Ok(0x8001)),
        finished(Some(0), usage, Ok(0x8001)),
        finished(Some(2), "", Ok(0x8001)),
        finished(Some(2), usage, Ok(0)),
        finished(Some(2), usage, Ok(0x8000)),
        finished(Some(2), usage, Ok(1)),
        finished(Some(2), usage, Err(-1)),
    ] {
        assert!(judge(&bad).is_err());
    }
}

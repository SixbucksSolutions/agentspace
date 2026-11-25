//! What the two hard-error specs share (#0191): error modes, a file that is not a program, and a
//! bounded start that watches the desktop for an "Unsupported 16-Bit Application" message box.
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.

#![allow(dead_code)]
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::System::Diagnostics::Debug::{
    GetErrorMode, SetErrorMode, SetThreadErrorMode,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId,
};

/// This process's error mode.
pub fn process_mode() -> u32 {
    // SAFETY: reads this process's own error mode.
    unsafe { GetErrorMode() }
}

/// Sets this process's error mode to exactly `mode`.
pub fn set_process_mode(mode: u32) {
    // SAFETY: sets this process's own error mode; no pointer is passed.
    unsafe { SetErrorMode(mode) };
    assert_eq!(process_mode(), mode, "The process's error mode can be set.");
}

/// Sets the calling thread's error mode to exactly `mode`.
pub fn set_thread_mode(mode: u32) {
    // SAFETY: sets this thread's own error mode; no old mode is asked for.
    let ok = unsafe { SetThreadErrorMode(mode, std::ptr::null_mut()) };
    assert_ne!(ok, 0, "The thread's error mode can be set.");
}

/// Puts the process's error mode back as it was when this was made, however the spec ends.
pub struct ProcessModeBack(u32);

impl ProcessModeBack {
    pub fn new() -> ProcessModeBack {
        ProcessModeBack(process_mode())
    }
}

impl Drop for ProcessModeBack {
    fn drop(&mut self) {
        // SAFETY: sets this process's own error mode; no pointer is passed.
        unsafe { SetErrorMode(self.0) };
    }
}

/// A throwaway folder under Cargo's temporary folder for tests, inside the build's target folder.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Scratch {
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR").replace('/', "\\"))
            .join(format!("agentspace-launch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("The scratch folder can be made.");
        Scratch(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A `.exe` in `dir` that is not a Windows program, which Windows takes for a 16-bit one.
pub fn not_a_program(dir: &Path) -> PathBuf {
    let path = dir.join("not-a-program.exe");
    std::fs::write(&path, b"This is not a program.").expect("The file can be made.");
    path
}

/// How long a start that cannot succeed is given to return its error. Each takes milliseconds
/// alone; the bound is for a laptop under a parallel build.
pub const START_BOUND: Duration = Duration::from_secs(10);

/// What the title of the message box Windows raises for a 16-bit program holds.
const SIXTEEN_BIT: &str = "16-Bit";

/// A window on the desktop: its handle, and the PID of the process that owns it. Its title is
/// never kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub handle: usize,
    pub pid: u32,
}

unsafe extern "system" fn note_16_bit(hwnd: HWND, found: LPARAM) -> i32 {
    let mut title = [0u16; 256];
    // SAFETY: the buffer is ours and its length is given.
    let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    if n > 0 && String::from_utf16_lossy(&title[..n as usize]).contains(SIXTEEN_BIT) {
        let mut pid = 0u32;
        // SAFETY: the window came from EnumWindows; the out-parameter is ours.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        // SAFETY: `found` is the `Vec` that `sixteen_bit_windows` passed, alive for the call.
        let found = unsafe { &mut *(found as *mut Vec<Window>) };
        found.push(Window {
            handle: hwnd as usize,
            pid,
        });
    }
    1
}

/// The desktop's top-level windows whose titles hold `16-Bit`. No other window's title is kept.
pub fn sixteen_bit_windows() -> Vec<Window> {
    let mut found: Vec<Window> = Vec::new();
    // SAFETY: the callback only reads titles and pushes to `found`, which outlives the call.
    unsafe { EnumWindows(Some(note_16_bit), &mut found as *mut Vec<Window> as LPARAM) };
    found
}

/// How a bounded start was stuck.
#[derive(Debug)]
pub enum Stuck {
    /// A `16-Bit` window that was not there before the start appeared while it ran.
    MessageBox(Vec<Window>),
    /// It did not return within `START_BOUND`.
    NoAnswer,
}

/// Runs `start` on a thread of its own whose error mode is set to 0 first, so nothing of the
/// caller's thread helps it, and gives what it returned with how long it took, or how it was
/// stuck. The desktop is looked at every 100 ms while it runs, and a `16-Bit` window that was not
/// there before it began ends the wait at once.
///
/// **A start that is stuck leaves its thread waiting on the message box**, which the test
/// program's end takes with it. A test MUST NOT press the message box.
pub fn start_bounded<T: Send + 'static>(
    start: impl FnOnce() -> T + Send + 'static,
) -> Result<(T, Duration), Stuck> {
    let before = sixteen_bit_windows();
    let (send, receive) = mpsc::channel();
    let began = Instant::now();
    std::thread::spawn(move || {
        set_thread_mode(0);
        let _ = send.send(start());
    });
    loop {
        match receive.recv_timeout(Duration::from_millis(100)) {
            Ok(out) => {
                let took = began.elapsed();
                let new = new_windows(&before);
                return if new.is_empty() {
                    Ok((out, took))
                } else {
                    Err(Stuck::MessageBox(new))
                };
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("The start's thread panicked."),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let new = new_windows(&before);
        if !new.is_empty() {
            return Err(Stuck::MessageBox(new));
        }
        if began.elapsed() > START_BOUND {
            return Err(Stuck::NoAnswer);
        }
    }
}

fn new_windows(before: &[Window]) -> Vec<Window> {
    sixteen_bit_windows()
        .into_iter()
        .filter(|w| !before.contains(w))
        .collect()
}

/// The failure message for a stuck start. It names the windows' owning PIDs, never a title.
pub fn stuck_message(stuck: &Stuck) -> String {
    match stuck {
        Stuck::MessageBox(windows) => format!(
            "The start raised {} 16-Bit message box window(s), owned by PID(s) {:?}, and did not \
             return (#0191).",
            windows.len(),
            windows.iter().map(|w| w.pid).collect::<Vec<_>>()
        ),
        Stuck::NoAnswer => format!(
            "The start did not return within {} seconds: it is waiting on something, such as a \
             message box on the desktop (#0191).",
            START_BOUND.as_secs()
        ),
    }
}

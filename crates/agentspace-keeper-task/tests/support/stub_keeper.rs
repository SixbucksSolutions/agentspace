//! TEST-ONLY stand-in for `agentspace-keeper.exe`. It MUST NOT be installed or shipped.
//!
//! The launcher's tests copy it, as `agentspace-keeper.exe`, into a throwaway folder beside a copy
//! of the launcher. It records what it saw into `stub-record.txt` in its own folder, writes one
//! line to stdout and one to stderr, and exits with the code in `stub-exit-code.txt` in its own
//! folder (decimal, up to 4294967295), or 0 when that file is not there.
//!
//! The record has one `key=value` per line:
//! - `args`: the arguments, as `Debug` of a list of `OsString`s.
//! - `cwd`: the working folder.
//! - `console_window`: `null` when the process has no console window (`GetConsoleWindow` gives
//!   null), `present` when it has one.
//! - `env_count` and `env_digest`: see `envdigest.rs`; no value of the environment is recorded.
//! - `var.<NAME>`: the value of each variable the tests name below, or `<unset>`.

#![deny(unsafe_code)]

mod envdigest;

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::exit;

/// The variables whose values the record carries: the four the tests point at a throwaway home,
/// the state folder, and one marker the tests make up.
const NAMED: [&str; 6] = [
    "USERPROFILE",
    "HOME",
    "LOCALAPPDATA",
    "APPDATA",
    "AGENTSPACE_STATE_DIR",
    "AGENTSPACE_LAUNCHER_TEST_MARKER",
];

/// The one Windows call, in a module of its own.
#[cfg(windows)]
mod console {
    #![allow(unsafe_code)]

    /// Whether this process has a console window.
    pub fn has_window() -> Option<bool> {
        // SAFETY: GetConsoleWindow takes no arguments and only returns a handle.
        let hwnd = unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() };
        Some(!hwnd.is_null())
    }
}

#[cfg(not(windows))]
mod console {
    pub fn has_window() -> Option<bool> {
        None
    }
}

fn main() {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(_) => exit(98),
    };
    let dir: PathBuf = match exe.parent() {
        Some(dir) => dir.to_path_buf(),
        None => exit(98),
    };
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut record = String::new();
    let _ = writeln!(record, "args={args:?}");
    let cwd = std::env::current_dir().map(|p| p.display().to_string());
    let _ = writeln!(record, "cwd={}", cwd.unwrap_or_default());
    let window = match console::has_window() {
        Some(true) => "present",
        Some(false) => "null",
        None => "unknown",
    };
    let _ = writeln!(record, "console_window={window}");
    let (count, hash) = envdigest::digest(std::env::vars_os());
    let _ = writeln!(record, "env_count={count}");
    let _ = writeln!(record, "env_digest={hash:016x}");
    for name in NAMED {
        let value = std::env::var_os(name);
        let shown = value.map_or_else(
            || "<unset>".to_string(),
            |v| v.to_string_lossy().into_owned(),
        );
        let _ = writeln!(record, "var.{name}={shown}");
    }
    if fs::write(dir.join("stub-record.txt"), record).is_err() {
        exit(99);
    }
    // Whether the launcher passes its streams on: a test that gives the launcher pipes reads these.
    let _ = writeln!(std::io::stdout(), "stub stdout");
    let _ = writeln!(std::io::stderr(), "stub stderr");
    let code = fs::read_to_string(dir.join("stub-exit-code.txt"))
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
        .unwrap_or(0);
    // On Windows the exit code is 32 bits: an i32 of the same bits passes any of them on.
    exit(code as i32);
}

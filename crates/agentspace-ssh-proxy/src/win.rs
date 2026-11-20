//! The ProxyCommand's Windows calls, and the only module of this crate with `unsafe`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Rust edition 2024 makes `std::env::set_var` unsafe, and the operator never changes its own
//! environment (operator.md, Environment variables). The ProxyCommand's step 2 puts two folders
//! first on `PATH` for everything `up`'s steps start, so it does it through the Windows API,
//! which locks the environment block itself.
//!
//! - [`set_path`] is `SetEnvironmentVariableW` for `PATH`.
//! - [`StdoutGuard`] sends the process's standard output handle to its standard error handle for
//!   as long as it lives, so that nothing `up`'s steps write or let a program write to stdout
//!   reaches the SSH connection (operator.md, The ProxyCommand, step 3).
//! - [`repair_console`] leaves a console on standard error in the output mode ssh needs
//!   (operator.md, The ProxyCommand, What it MUST NOT do: "Leave the console broken").

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{
    CONSOLE_MODE, DISABLE_NEWLINE_AUTO_RETURN, ENABLE_PROCESSED_OUTPUT, ENABLE_WRAP_AT_EOL_OUTPUT,
    GetConsoleMode, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode,
    SetStdHandle,
};
use windows_sys::Win32::System::Environment::SetEnvironmentVariableW;

/// Sets this process's `PATH`, which every program started afterwards inherits. `false` when the
/// call fails.
///
/// The ProxyCommand MAY set its own `PATH`, once, from its `main`, before any thread starts, in
/// this Windows module (architect, 2026-11-19, for E0.06-T03-S03; operator.md, The ProxyCommand,
/// step 2 with FB-10). The ruling that the operator never changes its own environment binds the
/// operator's libraries, which run inside other programs; no library calls this. It is reached
/// only from step 2 of `proxy`, which only `main`'s call to `run` reaches, after the argument and
/// alias checks and before any program or thread is started.
pub fn set_path(value: &OsStr) -> bool {
    let name: Vec<u16> = "PATH".encode_utf16().chain(Some(0)).collect();
    let value: Vec<u16> = value.encode_wide().chain(Some(0)).collect();
    // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the call.
    unsafe { SetEnvironmentVariableW(name.as_ptr(), value.as_ptr()) != 0 }
}

/// While it lives, standard output is standard error. Dropping it, or [`StdoutGuard::restore`],
/// gives the process its own standard output back.
pub struct StdoutGuard {
    saved: HANDLE,
    active: bool,
}

impl StdoutGuard {
    /// Redirects standard output to standard error. Does nothing, and guards nothing, when the
    /// process has no standard error to send it to.
    pub fn new() -> StdoutGuard {
        // SAFETY: GetStdHandle and SetStdHandle take no pointers.
        unsafe {
            let saved = GetStdHandle(STD_OUTPUT_HANDLE);
            let err = GetStdHandle(STD_ERROR_HANDLE);
            let usable = !err.is_null() && err != INVALID_HANDLE_VALUE;
            let active = usable && SetStdHandle(STD_OUTPUT_HANDLE, err) != 0;
            StdoutGuard { saved, active }
        }
    }

    /// Puts the process's own standard output back. It MUST be called before the tunnel starts.
    pub fn restore(&mut self) {
        if self.active {
            // SAFETY: `saved` is the handle GetStdHandle gave for standard output.
            unsafe {
                SetStdHandle(STD_OUTPUT_HANDLE, self.saved);
            }
            self.active = false;
        }
    }
}

impl Drop for StdoutGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

/// The output mode ssh needs: processed output and wrap at end of line on, and
/// `DISABLE_NEWLINE_AUTO_RETURN` off, every other bit as it was.
pub fn repaired_mode(mode: CONSOLE_MODE) -> CONSOLE_MODE {
    (mode | ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT) & !DISABLE_NEWLINE_AUTO_RETURN
}

/// When standard error is a console, sets its output mode to [`repaired_mode`] of what it is.
/// Does nothing when standard error is not a console, or the mode is already right.
pub fn repair_console() {
    // SAFETY: GetStdHandle takes no pointers; GetConsoleMode writes one CONSOLE_MODE to a local
    // that outlives the call, and fails for a handle that is not a console's.
    unsafe {
        let err = GetStdHandle(STD_ERROR_HANDLE);
        if err.is_null() || err == INVALID_HANDLE_VALUE {
            return;
        }
        let mut mode: CONSOLE_MODE = 0;
        if GetConsoleMode(err, &mut mode) == 0 {
            return;
        }
        let want = repaired_mode(mode);
        if want != mode {
            SetConsoleMode(err, want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn the_console_mode_ssh_needs() {
        // Both bits turned on, the auto-return bit turned off, the others kept.
        assert_eq!(repaired_mode(0), 3);
        assert_eq!(repaired_mode(DISABLE_NEWLINE_AUTO_RETURN), 3);
        assert_eq!(repaired_mode(0x4 | 0x8 | 0x10), 0x4 | 0x10 | 3);
        assert_eq!(repaired_mode(3), 3);
    }

    const MARK_GUARDED: &str = "GUARDED-WRITE";
    const MARK_CHILD: &str = "GUARDED-CHILD";
    const MARK_AFTER: &str = "AFTER-RESTORE";

    /// Run only by [`the_guard_sends_stdout_to_stderr`], in a process of its own, since it changes
    /// the process's standard output handle.
    #[test]
    #[ignore = "run in a child process by the_guard_sends_stdout_to_stderr"]
    fn guard_child() {
        let mut guard = StdoutGuard::new();
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{MARK_GUARDED}");
        let _ = out.flush();
        // A program started while the guard lives inherits the redirected handle.
        let system_root = std::env::var_os("SystemRoot").expect("SystemRoot");
        let cmd = std::path::Path::new(&system_root)
            .join("System32")
            .join("cmd.exe");
        let _ = Command::new(cmd)
            .args(["/d", "/c", "echo", MARK_CHILD])
            .stdin(Stdio::null())
            .status();
        guard.restore();
        let _ = writeln!(out, "{MARK_AFTER}");
        let _ = out.flush();
    }

    #[test]
    fn the_guard_sends_stdout_to_stderr() {
        let home =
            std::env::temp_dir().join(format!("agentspace-ssh-proxy-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let exe = std::env::current_exe().unwrap();
        let system_root = std::env::var_os("SystemRoot").expect("SystemRoot");
        let out = Command::new(exe)
            .args([
                "--exact",
                "platform::tests::guard_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env_clear()
            .env("SystemRoot", &system_root)
            .env("USERPROFILE", &home)
            .env("HOME", &home)
            .env("LOCALAPPDATA", &home)
            .env("APPDATA", &home)
            .env("TEMP", &home)
            .env("TMP", &home)
            .current_dir(&home)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&home);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "the child test failed");
        assert!(
            !stdout.contains(MARK_GUARDED) && !stdout.contains(MARK_CHILD),
            "a write to stdout while the guard lives reached stdout"
        );
        assert!(
            stderr.contains(MARK_GUARDED) && stderr.contains(MARK_CHILD),
            "a write to stdout while the guard lives did not reach stderr"
        );
        assert!(
            stdout.contains(MARK_AFTER),
            "stdout is not the process's own after restore"
        );
    }
}

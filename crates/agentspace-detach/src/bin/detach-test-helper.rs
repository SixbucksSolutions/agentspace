//! `detach-test-helper`: test-only, never installed. The program this crate's tests start
//! detached (pinned by E0.07-T02-S02, #0147):
//! - `write <file> <var> <delay-ms>` sleeps `delay-ms`, then writes to `<file>` one line,
//!   `set:<value>` with the value of the variable `<var>` (`set:` alone when it is set and
//!   empty), or `unset` when it is not set, and exits 0. It exits 1 when it cannot write the file.
//! - `detach <file> <var> <value> <delay-ms>` reads one line from its standard input (the go
//!   signal), then starts this same program as `write <file> <var> <delay-ms>` through
//!   `WmiDetach`, in `<file>`'s folder, with `<var>` set to `<value>`. On success it prints
//!   `started` and flushes, then sleeps up to 60 seconds, since the test is expected to kill it.
//!   On a refusal it prints `refused: <message>` and exits 1. On a system other than Windows it
//!   starts through `NoDetach`, which refuses.
//!
//! Anything else exits 2. `<file>` is made full against the working folder first, so the program
//! it starts writes the same file from its own working folder.

#![forbid(unsafe_code)]

use agentspace_detach::Detach;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let args: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
    let code = match args.as_slice() {
        [mode, file, var, delay] if *mode == "write" => match delay_of(delay) {
            Some(delay) => write(Path::new(file), var, delay),
            None => 2,
        },
        [mode, file, var, value, delay] if *mode == "detach" => {
            match (var.to_str(), delay.to_str(), delay_of(delay)) {
                (Some(var), Some(delay), Some(_)) => detach(Path::new(file), var, value, delay),
                _ => 2,
            }
        }
        _ => 2,
    };
    std::process::exit(code);
}

fn delay_of(text: &OsStr) -> Option<Duration> {
    text.to_str()?
        .parse::<u64>()
        .ok()
        .map(Duration::from_millis)
}

fn write(file: &Path, var: &OsStr, delay: Duration) -> i32 {
    std::thread::sleep(delay);
    let line = match std::env::var_os(var) {
        Some(value) => format!("set:{}\n", value.to_string_lossy()),
        None => "unset\n".to_string(),
    };
    match std::fs::write(file, line) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn detach(file: &Path, var: &str, value: &OsStr, delay: &str) -> i32 {
    let mut go = String::new();
    let _ = std::io::stdin().lock().read_line(&mut go);
    let answer = start(file, var, value, delay);
    let mut out = std::io::stdout().lock();
    match answer {
        Ok(()) => {
            let _ = writeln!(out, "started");
            let _ = out.flush();
            drop(out);
            std::thread::sleep(Duration::from_secs(60));
            0
        }
        Err(why) => {
            let _ = writeln!(out, "refused: {why}");
            let _ = out.flush();
            1
        }
    }
}

fn start(file: &Path, var: &str, value: &OsStr, delay: &str) -> Result<(), String> {
    let file = std::path::absolute(file)
        .map_err(|e| format!("The file's path could not be made full: {e}"))?;
    let folder = file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let exe = std::env::current_exe()
        .map_err(|e| format!("This program's own path could not be read: {e}"))?;
    let file = file
        .to_str()
        .ok_or_else(|| "The file's path is not Unicode.".to_string())?;
    detacher().start(&exe, &["write", file, var, delay], &folder, &[(var, value)])
}

#[cfg(windows)]
fn detacher() -> agentspace_detach::WmiDetach {
    agentspace_detach::WmiDetach
}

#[cfg(not(windows))]
fn detacher() -> agentspace_detach::NoDetach {
    agentspace_detach::NoDetach
}

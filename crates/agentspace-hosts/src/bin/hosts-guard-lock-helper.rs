//! Test-only helper: `hosts-guard-lock-helper <home> <hold-ms>`.
//!
//! Tries the guard's lock once, prints `held` or `busy` on stdout and exits 0. When it printed
//! `held`, it keeps the lock for `<hold-ms>` milliseconds before it exits. A failure prints one
//! line to stderr and exits 2. This is a program, not a library, so it MAY write to stdout and
//! stderr.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [home, hold_ms] = args.as_slice() else {
        eprintln!("Usage: hosts-guard-lock-helper <home> <hold-ms>");
        return ExitCode::from(2);
    };
    let Ok(hold_ms) = hold_ms.parse::<u64>() else {
        eprintln!("The hold time is not a number of milliseconds.");
        return ExitCode::from(2);
    };
    match agentspace_hosts::try_guard_lock(&PathBuf::from(home)) {
        Ok(Some(lock)) => {
            // Flushed before the wait, so the parent reads `held` while the lock is still held.
            println!("held");
            let _ = std::io::stdout().flush();
            std::thread::sleep(Duration::from_millis(hold_ms));
            drop(lock);
            ExitCode::SUCCESS
        }
        Ok(None) => {
            println!("busy");
            let _ = std::io::stdout().flush();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}

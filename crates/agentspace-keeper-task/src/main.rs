//! `agentspace-keeper-task.exe`, the keeper task's launcher (docs/keeper.md, The task's launcher).
//!
//! A Windows-subsystem program, so it has no console: the logon task runs it, and it runs the
//! keeper with no window. Everything it does is in the library.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use agentspace_keeper_task::{Env, run};
use std::ffi::OsString;

fn main() {
    // First, before anything is started or opened: the task's launcher starts the keeper, and a
    // process at error mode 0 that starts a file that is not a program waits for a person at a
    // modal dialog (#0225, D81); its children inherit this mode.
    agentspace_launch::quiet_hard_errors();
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let code = run(&args, std::env::current_exe(), &Env::from_process());
    std::process::exit(code);
}

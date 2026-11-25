//! The `agentspace` program: reads its arguments and its own executable, and hands both to the
//! library (docs/decisions.md, D38).

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // First, before anything is started or opened: no hard-error message box, whatever error mode
    // the parent gave this process (#0225; D81, as changed during v0.10-T02).
    agentspace_launch::quiet_hard_errors();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // When the operating system cannot name the executable, an empty path is passed, and any
    // command that needs a sibling program fails the step that needs it (architect, 2026-11-19).
    let own_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::new());
    agentspace_cli::main_with(args, own_exe)
}

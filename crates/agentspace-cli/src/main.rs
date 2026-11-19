//! The `agentspace` program: reads its arguments and its own executable, and hands both to the
//! library (docs/decisions.md, D38).

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // GAP: when the operating system cannot name the executable, an empty path is passed, and any
    // command that needs a sibling program fails the step that needs it.
    let own_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::new());
    agentspace_cli::main_with(args, own_exe)
}

//! `agentspace-keeper-install [-DryRun | -Uninstall]` (docs/keeper.md, The installer). A thin
//! wrapper: the library does the work.

use agentspace_keeper_install::{Mode, parse_args, run, usage_error_lines};
use std::io::Write;

fn main() {
    // First, before anything is started or opened: a process at error mode 0 that starts a file
    // that is not a program waits for a person at a modal dialog (#0225, D81); its children inherit
    // this mode.
    agentspace_launch::quiet_hard_errors();
    // An argument that is not valid text is shown with replacement characters, as `Unknown
    // argument '<raw>'.` names what the caller gave as nearly as it can.
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let mode: Mode = match parse_args(&args) {
        Ok(mode) => mode,
        Err(message) => {
            let mut stderr = std::io::stderr().lock();
            for line in usage_error_lines(&message) {
                let _ = writeln!(stderr, "{line}");
            }
            std::process::exit(2);
        }
    };
    let code = run(
        mode,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    std::process::exit(code);
}

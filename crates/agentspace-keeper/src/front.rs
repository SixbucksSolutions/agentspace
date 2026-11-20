//! The program's front: the command line, the environment and the state folder, then the pass
//! (docs/keeper.md, Command line; Exit codes; Streams and environment).

use crate::Deps;
use crate::args::{PROGRAM, USAGE, parse_args};
use crate::pass::{PassContext, run_pass};
use crate::watch::{Keeping, watch};
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

/// The keeper's own environment, as the program found it.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    /// Reads one variable of the keeper's own environment; `None` when it is unset. The binary
    /// reads the real one; a test gives its own.
    pub var: &'a dyn Fn(&str) -> Option<OsString>,
    /// The working folder, against which a relative `USERPROFILE` or `AGENTSPACE_STATE_DIR` is
    /// made full.
    pub cwd: &'a Path,
    /// The keeper's own program, a full path.
    pub own_exe: &'a Path,
}

/// Runs the keeper's program with `args`, the arguments after its own name, and returns its exit
/// code (keeper.md, Exit codes).
///
/// The usage checks come first: a usage error writes its three lines to `stderr`, logs nothing,
/// and gives 2. Then `USERPROFILE` unset or empty, or a state folder that cannot be created, ends
/// the run before its first pass with one line on `stderr`, nothing logged, and 1. Otherwise one
/// pass runs, and its lines go to `board-holder.log` and `stdout`.
///
/// With `-Watch` the run is a keeper (keeper.md, Watching): it takes the keeper lock once, then
/// runs passes in a loop until `-MaxPasses` have run, and never exits for an error once the loop
/// has started.
pub fn run(
    args: &[String],
    env: &Env,
    deps: &Deps,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let options = match parse_args(args, env.var) {
        Ok(o) => o,
        Err(message) => {
            let _ = write!(stderr, "{PROGRAM}: {message}\n{USAGE}");
            let _ = stderr.flush();
            return 2;
        }
    };

    // An error before the first pass: exit 1, nothing logged, one line on stderr.
    let before_first_pass = |stderr: &mut dyn Write, message: &str| {
        let _ = writeln!(stderr, "{PROGRAM}: {message}");
        let _ = stderr.flush();
        1
    };

    // A holder needs its working folder, so USERPROFILE is needed whether or not
    // AGENTSPACE_STATE_DIR is set (architect, 2026-11-19, gaps FA-18 and FB-19).
    let paths = match agentspace_state::resolve_paths(env.var, env.cwd) {
        Ok(p) => p,
        Err(_) => {
            return before_first_pass(stderr, "The folders could not be found.");
        }
    };
    let (Some(profile), Some(state_dir)) = (paths.home, paths.state_dir) else {
        return before_first_pass(
            stderr,
            "USERPROFILE is not set, so there is no home folder.",
        );
    };

    // The keeper creates the state folder if it is missing.
    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        // The line's text is keeper.md's (Exit codes; architect, 2026-11-19, for E0.06-T01-S06).
        return before_first_pass(
            stderr,
            &format!(
                "The state folder '{}' could not be created: {e}",
                state_dir.display()
            ),
        );
    }

    if let Some(watch_options) = options.watch {
        let keeping = Keeping {
            state_dir: &state_dir,
            profile: &profile,
            ssh_exe: &options.ssh_exe,
            own_exe: env.own_exe,
            options: watch_options,
        };
        return watch(&keeping, deps, stdout);
    }

    let ctx = PassContext {
        state_dir: &state_dir,
        profile: &profile,
        ssh_exe: &options.ssh_exe,
        own_exe: env.own_exe,
        now: options.now,
        tracked: None,
    };
    run_pass(&ctx, deps, stdout).exit_code
}

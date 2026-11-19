//! `agentspace-keeper.exe` (docs/keeper.md; D42).
//!
//! The binary is thin: it reads the real arguments and environment, and wires the pass to its
//! parts. Until v0.06-T02 builds the Windows halves (`Processes` and `Locks` in
//! `agentspace-keeper-win`, `Starter` in `agentspace-launch`) and E0.06-T03-S04 wires them with
//! the matcher and the record, every part but the clock is a stand-in that fails clearly. So the
//! command line, the usage errors, the `USERPROFILE` check and the state folder are the real
//! program's, and a pass logs `failed` at its first step, naming what is missing.

#![forbid(unsafe_code)]

use agentspace_keeper::{Deps, Env, Records, run};
use agentspace_keeper_api::{
    Clock, HolderRecord, Locks, Matcher, Processes, Query, StartRequest, Started, Starter, Stopped,
    Taken,
};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What every stand-in says.
const MISSING: &str =
    "This build of agentspace-keeper has no Windows half yet; v0.06-T02 and T03 add it.";

/// The real clocks.
struct SystemClock;

impl Clock for SystemClock {
    fn utc_now_ms(&self) -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
            Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX) - 1,
        }
    }

    fn monotonic(&self) -> Duration {
        static START: OnceLock<Instant> = OnceLock::new();
        START.get_or_init(Instant::now).elapsed()
    }

    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// The stand-in for every part T02 and T03 build.
struct Missing;

impl Locks for Missing {
    fn pass(&self, _: &Path) -> Taken {
        Taken::Failed(MISSING.to_string())
    }
    fn keeper(&self, _: &Path) -> Taken {
        Taken::Failed(MISSING.to_string())
    }
}

impl Matcher for Missing {
    fn holder_command(&self, _: &Path, _: &str) -> String {
        String::new()
    }
    fn is_holder(&self, _: &str, _: &Path, _: &str) -> bool {
        false
    }
    fn self_check(&self) -> Result<(), String> {
        Err(MISSING.to_string())
    }
}

impl Processes for Missing {
    fn query(&self, _: u32) -> Query {
        Query::Gone
    }
    fn stop(&self, _: &HolderRecord, _: &Path, _: &dyn Matcher) -> Stopped {
        Stopped::Error(MISSING.to_string())
    }
}

impl Starter for Missing {
    fn start(&self, _: &StartRequest, _: &dyn Matcher) -> Started {
        Started::Failed(format!("The holder could not be started: {MISSING}"))
    }
}

impl Records for Missing {
    fn read(&self, _: &Path) -> Option<HolderRecord> {
        None
    }
    fn write(&self, _: &Path, _: &HolderRecord) -> Result<(), String> {
        Err(MISSING.to_string())
    }
    fn remove(&self, _: &Path) -> Result<(), String> {
        Err(MISSING.to_string())
    }
}

fn main() {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let var = |name: &str| std::env::var_os(name);
    let cwd = std::env::current_dir().unwrap_or_default();
    let own_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(""));
    let env = Env {
        var: &var,
        cwd: &cwd,
        own_exe: &own_exe,
    };
    let clock = SystemClock;
    let deps = Deps {
        processes: &Missing,
        matcher: &Missing,
        starter: &Missing,
        locks: &Missing,
        clock: &clock,
        records: &Missing,
    };
    let code = run(
        &args,
        &env,
        &deps,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    std::process::exit(code);
}

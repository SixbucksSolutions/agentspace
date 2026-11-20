//! `agentspace-keeper.exe` (docs/keeper.md; D42).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! The binary is thin: it reads the real arguments and environment, and wires the pass to the real
//! parts:
//!
//! | Part | Is |
//! |---|---|
//! | `Processes`, `Locks` | `agentspace-keeper-win`'s `WinProcesses` and `WinLocks` (Windows only) |
//! | `Starter` | `agentspace-launch`'s `WmiStarter` |
//! | `Matcher` | `agentspace-keeper-files`' `HolderMatcher`, for the system folder Windows reports |
//! | `Records` | `FileRecords`, the holder record's file |
//! | `Clock` | the system's |
//!
//! **The launcher role comes first.** The keeper's own program is the launcher a start creates, run
//! as `agentspace-keeper -Launch <request>` (D42): exactly those two arguments go to
//! `agentspace_launch::run_launcher`, before any usage check, and nothing else runs. The argument
//! is private. It MUST NOT appear in a usage line, and it is no part of the interface.
//!
//! **The program prints nothing of a panic but its own words, and never exits 101** (keeper.md,
//! Fast failures and backoff; architect, 2026-11-19, for E0.06-T02-S05 and E0.06-T03-S04). `main`
//! installs a panic hook that writes nothing, since the runtime's default writes `thread 'main'
//! panicked at ...` to stderr. A panic that escapes the run, outside a keeper's loop (a one-pass
//! run, or before the first pass) or the launcher role, ends the program with exit 1 and the one
//! stderr line `agentspace-keeper: The keeper stopped on an error: <panic message>`. The library
//! sets no hook.

#![forbid(unsafe_code)]

use agentspace_keeper::{Deps, Env, FileRecords, PROGRAM, run};
use agentspace_keeper_api::Clock;
use agentspace_keeper_files::HolderMatcher;
use agentspace_launch::{WmiStarter, launcher_request, run_launcher};
use std::any::Any;
use std::ffi::OsString;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// The processes and locks of a system that is not Windows. The keeper runs on Windows only
/// (keeper.md, Where it runs); this stand-in lets the workspace build and test elsewhere, and
/// every pass that reaches it logs `failed`.
#[cfg(not(windows))]
struct NotWindows;

#[cfg(not(windows))]
const NOT_WINDOWS: &str = "The keeper runs only on Windows.";

#[cfg(not(windows))]
impl agentspace_keeper_api::Locks for NotWindows {
    fn pass(&self, _: &std::path::Path) -> agentspace_keeper_api::Taken {
        agentspace_keeper_api::Taken::Failed(NOT_WINDOWS.to_string())
    }
    fn keeper(&self, _: &std::path::Path) -> agentspace_keeper_api::Taken {
        agentspace_keeper_api::Taken::Failed(NOT_WINDOWS.to_string())
    }
}

#[cfg(not(windows))]
impl agentspace_keeper_api::Processes for NotWindows {
    fn query(&self, _: u32) -> agentspace_keeper_api::Query {
        agentspace_keeper_api::Query::Gone
    }
    fn stop(
        &self,
        _: &agentspace_keeper_api::HolderRecord,
        _: &std::path::Path,
        _: &dyn agentspace_keeper_api::Matcher,
    ) -> agentspace_keeper_api::Stopped {
        agentspace_keeper_api::Stopped::Error(NOT_WINDOWS.to_string())
    }
}

/// The system folder as Windows reports it: keeper.md's `<System32>`. When it cannot be had, the
/// matcher gets an empty one, which is not a full path, so its `self_check` fails and each pass
/// logs `failed` rather than the program ending.
#[cfg(windows)]
fn system_folder() -> PathBuf {
    agentspace_keeper_win::system_folder().unwrap_or_default()
}

#[cfg(not(windows))]
fn system_folder() -> PathBuf {
    PathBuf::new()
}

/// A panic's message: its payload when that is a `&str` or a `String`, else a fixed text. Each
/// run of control characters is one space, so the message is one line.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    let text = if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        String::new()
    };
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if c.is_control() {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "The panic carried no message.".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Installs the panic hook that writes nothing.
fn install_silent_panic_hook() {
    std::panic::set_hook(Box::new(|_| {}));
}

/// The one line a panic that escaped the run ends the program with.
fn escaped_line(payload: &(dyn Any + Send)) -> String {
    format!(
        "{PROGRAM}: The keeper stopped on an error: {}",
        panic_message(payload)
    )
}

/// Runs the keeper, and catches a panic that escapes it: exit 1 and one line on `stderr`.
fn guarded_run(
    args: &[String],
    env: &Env,
    deps: &Deps,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| run(args, env, deps, stdout, stderr))) {
        Ok(code) => code,
        Err(payload) => {
            // A failed write to stderr changes nothing; there is nowhere else to say it.
            let _ = writeln!(stderr, "{}", escaped_line(&*payload));
            let _ = stderr.flush();
            1
        }
    }
}

/// The keeper's own command line, run against the real parts.
fn keeper(args: &[String]) -> i32 {
    let var = |name: &str| std::env::var_os(name);
    let cwd = std::env::current_dir().unwrap_or_default();
    let own_exe = std::env::current_exe().unwrap_or_default();
    let env = Env {
        var: &var,
        cwd: &cwd,
        own_exe: &own_exe,
    };
    let clock = SystemClock;
    let matcher = HolderMatcher::new(system_folder());
    let starter = WmiStarter::new();
    let records = FileRecords::new();
    #[cfg(windows)]
    let (processes, locks) = (
        agentspace_keeper_win::WinProcesses::new(),
        agentspace_keeper_win::WinLocks::new(),
    );
    #[cfg(not(windows))]
    let (processes, locks) = (NotWindows, NotWindows);
    let deps = Deps {
        processes: &processes,
        matcher: &matcher,
        starter: &starter,
        locks: &locks,
        clock: &clock,
        records: &records,
    };
    guarded_run(
        args,
        &env,
        &deps,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}

/// Runs `f`, and catches a panic that escapes it: exit 1 and the one line on `stderr`, which `f`
/// does not write to.
fn guarded(stderr: &mut dyn Write, f: impl FnOnce() -> i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(code) => code,
        Err(payload) => {
            // A failed write to stderr changes nothing; there is nowhere else to say it.
            let _ = writeln!(stderr, "{}", escaped_line(&*payload));
            let _ = stderr.flush();
            1
        }
    }
}

/// The whole program over `raw`, the arguments after its own name: the hook first, then the
/// launcher role or the keeper's own command line, each guarded. `main` passes the real
/// `keeper` and `run_launcher`; a test passes its own.
fn program(
    raw: &[OsString],
    keeper: &dyn Fn(&[String]) -> i32,
    launcher: &dyn Fn(&std::path::Path) -> i32,
) -> i32 {
    // Before anything that can panic.
    install_silent_panic_hook();
    let mut stderr = std::io::stderr();
    // The launcher role comes before every usage check, and does nothing else. A panic that
    // escapes it ends it as one that escapes the keeper does (architect, 2026-11-19).
    if let Some(request) = launcher_request(raw) {
        return guarded(&mut stderr, || launcher(&request));
    }
    // A panic while the parts are being made, outside `run`, gets the same line and exit.
    guarded(&mut stderr, || {
        let args: Vec<String> = raw
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        keeper(&args)
    })
}

fn main() {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    std::process::exit(program(&raw, &keeper, &run_launcher));
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_keeper_api::{
        HolderRecord, Locks, Matcher, Processes, Query, StartRequest, Started, Starter, Stopped,
        Taken,
    };
    use std::path::Path;
    use std::process::Command;

    /// What a fake's `pass` lock does.
    #[derive(Clone, Copy)]
    enum Panic {
        Str,
        String,
        Other,
        Lines,
        Nothing,
    }

    /// A fake whose pass lock panics, which is outside every loop of a keeper: a one-pass run
    /// takes the lock straight from `run`.
    struct Fake(Panic);

    impl Locks for Fake {
        fn pass(&self, _: &Path) -> Taken {
            match self.0 {
                Panic::Str => panic!("The lock fell over."),
                Panic::String => panic!("{}", String::from("The lock fell over, formatted.")),
                Panic::Other => std::panic::panic_any(42_u32),
                Panic::Lines => panic!("First line.\r\nSecond line.\n\nThird.  "),
                Panic::Nothing => Taken::Skipped,
            }
        }
        fn keeper(&self, _: &Path) -> Taken {
            Taken::Skipped
        }
    }

    impl Matcher for Fake {
        fn holder_command(&self, _: &Path, _: &str) -> String {
            String::new()
        }
        fn is_holder(&self, _: &str, _: &Path, _: &str) -> bool {
            false
        }
        fn self_check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    impl Processes for Fake {
        fn query(&self, _: u32) -> Query {
            Query::Gone
        }
        fn stop(&self, _: &HolderRecord, _: &Path, _: &dyn Matcher) -> Stopped {
            Stopped::Gone
        }
    }

    impl Starter for Fake {
        fn start(&self, _: &StartRequest, _: &dyn Matcher) -> Started {
            Started::Failed("Not started.".to_string())
        }
    }

    /// Runs `guarded_run` over the fake in a throwaway home; gives the code, stdout and stderr.
    fn run_fake(fake: &Fake, args: &[&str]) -> (i32, String, String) {
        let dir = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-main-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let root = dir.path().canonicalize().expect("It has a full path");
        let home = root.join("home");
        let state = root.join("state");
        std::fs::create_dir_all(&home).unwrap();
        let var = |name: &str| -> Option<OsString> {
            match name {
                "USERPROFILE" => Some(home.clone().into_os_string()),
                "AGENTSPACE_STATE_DIR" => Some(state.clone().into_os_string()),
                "SystemRoot" => Some(root.join("winroot").into_os_string()),
                _ => None,
            }
        };
        let env = Env {
            var: &var,
            cwd: &home,
            own_exe: &root.join("keeper.exe"),
        };
        let records = FileRecords::new();
        let clock = SystemClock;
        let deps = Deps {
            processes: fake,
            matcher: fake,
            starter: fake,
            locks: fake,
            clock: &clock,
            records: &records,
        };
        let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = guarded_run(&args, &env, &deps, &mut out, &mut err);
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        )
    }

    #[test]
    fn a_panic_that_escapes_the_run_is_exit_1_and_one_line() {
        // The pass lock panics outside any loop; keeper.md, Fast failures and backoff.
        for (panic, message) in [
            (Panic::Str, "The lock fell over."),
            (Panic::String, "The lock fell over, formatted."),
            (Panic::Other, "The panic carried no message."),
        ] {
            let (code, out, err) = run_fake(&Fake(panic), &["-Now", "2026-11-19T12:00:00Z"]);
            assert_eq!(code, 1, "Exit 1, never 101, for {message}");
            assert_eq!(out, "");
            assert_eq!(
                err,
                format!("agentspace-keeper: The keeper stopped on an error: {message}\n")
            );
        }
    }

    #[test]
    fn the_line_is_one_line_however_many_the_message_has() {
        let (code, _, err) = run_fake(&Fake(Panic::Lines), &[]);
        assert_eq!(code, 1);
        assert_eq!(
            err,
            "agentspace-keeper: The keeper stopped on an error: First line. Second line. Third.\n"
        );
    }

    #[test]
    fn a_run_that_does_not_panic_is_the_runs_own_code() {
        let (code, _, err) = run_fake(&Fake(Panic::Nothing), &["-Now", "2026-11-19T12:00:00Z"]);
        assert_eq!(code, 0);
        assert_eq!(err, "");
    }

    #[test]
    fn a_usage_error_is_still_exit_2() {
        let (code, _, err) = run_fake(&Fake(Panic::Str), &["-Bogus"]);
        assert_eq!(code, 2);
        assert!(err.starts_with("agentspace-keeper: Unknown argument '-Bogus'.\n"));
    }

    /// The variable that makes the two child tests below run.
    const CHILD_VAR: &str = "AGENTSPACE_KEEPER_TEST_HOOK_CHILD";

    /// Runs only as the child of `the_hook_writes_nothing`: installs `main`'s hook, then panics
    /// and catches it, as the keeper does.
    #[test]
    fn the_hook_child() {
        if std::env::var_os(CHILD_VAR).is_none() {
            return;
        }
        install_silent_panic_hook();
        let caught = catch_unwind(|| panic!("This message MUST NOT reach stderr."));
        assert!(caught.is_err());
    }

    /// Runs only as the control's child: the runtime's default hook, then a caught panic.
    #[test]
    fn the_default_hook_child() {
        if std::env::var_os(CHILD_VAR).is_none() {
            return;
        }
        let caught = catch_unwind(|| panic!("This message is written by the default hook."));
        assert!(caught.is_err());
    }

    /// Runs this test program again for one child test, in a throwaway home.
    fn run_child(test: &str) -> (bool, String, String) {
        let home = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-hook-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let mut cmd = Command::new(std::env::current_exe().expect("The test program has a path"));
        cmd.args([test, "--exact", "--nocapture"])
            .env(CHILD_VAR, "1")
            .current_dir(home.path());
        for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
            cmd.env(name, home.path());
        }
        let out = cmd.output().expect("The child test program runs");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    #[test]
    fn the_hook_writes_nothing() {
        let (ok, stdout, stderr) = run_child("tests::the_hook_child");
        assert!(ok, "The child test passes");
        assert!(
            stdout.contains("1 passed"),
            "The child test ran, or this proves nothing"
        );
        assert!(!stderr.contains("panicked"), "The hook wrote a report");
        assert!(
            !stderr.contains("MUST NOT reach"),
            "The hook wrote the message"
        );
        assert!(
            !stdout.contains("MUST NOT reach"),
            "The hook wrote the message"
        );
    }

    /// Runs only as a child: the whole `program` with a keeper that panics.
    #[test]
    fn the_program_keeper_panic_child() {
        if std::env::var_os(CHILD_VAR).is_none() {
            return;
        }
        let raw = [
            OsString::from("-Now"),
            OsString::from("2026-11-19T12:00:00Z"),
        ];
        let code = program(&raw, &|_| panic!("Boom in the keeper."), &|_| 0);
        assert_eq!(code, 1);
    }

    /// Runs only as a child: the whole `program` as the launcher, with a launcher that panics.
    #[test]
    fn the_program_launcher_panic_child() {
        if std::env::var_os(CHILD_VAR).is_none() {
            return;
        }
        let raw = [OsString::from("-Launch"), OsString::from("x")];
        let code = program(&raw, &|_| 0, &|_| panic!("Boom in the launcher."));
        assert_eq!(code, 1);
    }

    /// `program` installs the hook before it runs either role, and a panic in either is exit 1
    /// with exactly the one line on stderr, never the runtime's report.
    #[test]
    fn a_panic_in_either_role_is_the_one_line_and_exit_1() {
        for (child, message) in [
            (
                "tests::the_program_keeper_panic_child",
                "Boom in the keeper.",
            ),
            (
                "tests::the_program_launcher_panic_child",
                "Boom in the launcher.",
            ),
        ] {
            let (ok, stdout, stderr) = run_child(child);
            assert!(ok, "The child test {child} passes");
            assert!(stdout.contains("1 passed"), "The child test {child} ran");
            assert_eq!(
                stderr,
                format!("agentspace-keeper: The keeper stopped on an error: {message}\n"),
                "For {child}"
            );
        }
    }

    #[test]
    fn guarded_passes_a_code_through_and_writes_nothing() {
        let mut err = Vec::new();
        assert_eq!(guarded(&mut err, || 2), 2);
        assert!(err.is_empty());
    }

    /// keeper.md, The holder command: a matcher whose system folder is unknown fails its own
    /// check, so every pass logs `failed` and trusts nothing (step 2).
    #[test]
    fn a_matcher_with_no_system_folder_fails_its_self_check() {
        assert!(HolderMatcher::new(PathBuf::new()).self_check().is_err());
    }

    #[test]
    fn without_the_hook_the_child_would_write_the_report() {
        // The control: the same panic under the runtime's default hook does write, so the test
        // above can fail.
        let (ok, _, stderr) = run_child("tests::the_default_hook_child");
        assert!(ok, "The child test passes");
        assert!(stderr.contains("panicked"), "The default hook reports");
    }
}

//! The operator's front: command line, help, usage errors and messages (docs/operator.md; D42)
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! # Public API
//!
//! - [`parse`] reads a command line into [`Parsed`]: help ([`HelpLevel`]), a [`Request`] to run,
//!   or the whole `agentspace: ` line of a usage error or an `-Alias` refusal.
//! - [`help`] gives the help text of a [`HelpLevel`], byte for byte as operator.md gives it.
//! - [`Command`] names the ten commands; [`Command::name`] is operator.md's spelling.
//! - [`Printer`] implements the pinned [`Say`] for a person's command and for caller mode, to any
//!   writer, and prints a [`Failure`] with [`Printer::failure`].
//! - [`context_from`] builds the pinned [`Context`] from an injectable variable lookup, or fails
//!   with the [`Failure`] for an unset or empty `USERPROFILE`, then `SystemRoot` ([`NO_HOME`],
//!   [`NO_SYSTEM_ROOT`]), so a [`Context`] never holds an empty `home` or `system_root`; the
//!   failure's caller line is [`COULD_NOT_REACH`]. [`diagnostic_from`] reads
//!   `AGENTSPACE_DIAGNOSTIC` the same way. [`process_env`] is the process's own environment as
//!   such a lookup.
//! - [`run`] is the program: it parses, checks the home and the system folder, prints and
//!   dispatches, and returns the exit code, 0 or 1.
//! - [`dispatch`] is where a parsed command runs and gives an [`Outcome`] or a [`Failure`], over
//!   [`Parts`]: the pinned [`Aws`], [`Remote`]
//!   and [`Say`], a [`Clock`], a [`Report`] for `status`'s lines and a [`Reach`] for the token
//!   push, `ca`, a host's key, certificate and alias, `sync`, `deploy`, `bootstrap` and
//!   `provision`. The instance ID is the host registry's ([`HostCache`]). [`up`],
//!   [`down`], [`status`] and [`grow`] are the commands themselves.
//! - [`Env`] is the program's environment as it started, copied once from a lookup; [`run`]
//!   builds the [`Context`], the [`CaEnv`] and every program's seam variables from it.
//!
//! The library reads no environment variable but through the lookup [`run`] is given, once, and
//! never mutates the process environment (the "environment at start" rule is agentspace-run's,
//! which starts every program). It writes to stdout only the help text, and to stderr only through
//! [`Printer`] and the streams of the programs it starts.

#![forbid(unsafe_code)]

mod access;
mod cache;
mod cli;
mod commands;
mod guard;
mod help_text;
mod reach;
mod say;
mod status;
#[cfg(test)]
mod testing;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

pub use access::Access;
pub use agentspace_box::{Clock, InstanceIdCache, SystemClock};
pub use agentspace_ca::CaEnv;
pub use agentspace_operator_api::{
    Aws, CallerFailure, CallerLine, Context, Failure, Mode, Remote, Say,
};
pub use agentspace_sync::Target;
pub use cache::HostCache;
pub use cli::{Command, DEFAULT_ALIAS, DEFAULT_GIB, HelpLevel, Parsed, Request, help, parse};
pub use commands::{HostRef, Outcome, Parts, Reach, Report, dispatch, down, grow, status, up};
pub use guard::{GuardRun, GuardSay, guard, guard_with};
pub use reach::{Env, SystemReach};
pub use say::{COULD_NOT_REACH, NOT_AVAILABLE, NOT_READY, Printer, caller_warning};

/// A source of environment variables: the process's own, or a test's.
pub type EnvLookup<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// The process's own environment, as a lookup.
pub fn process_env(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

fn non_empty(lookup: EnvLookup, name: &str) -> Option<OsString> {
    lookup(name).filter(|v| !v.is_empty())
}

/// The message for an unset or empty `USERPROFILE`.
pub const NO_HOME: &str = "agentspace: USERPROFILE is not set, so there is no home folder.";
/// The message for an unset or empty `SystemRoot`.
pub const NO_SYSTEM_ROOT: &str =
    "agentspace: SystemRoot is not set, so Windows' programs cannot be found.";
/// The message when help cannot be written to stdout (a closed pipe or a full device). operator.md
/// gives no text for it; the architect rules on this one.
pub const HELP_NOT_WRITTEN: &str = "agentspace: Could not write the help to stdout.";

fn could_not_reach(message: &str) -> Failure {
    Failure {
        message: message.to_string(),
        caller: CallerFailure::CouldNotReach,
    }
}

/// Builds the pinned [`Context`] from `lookup`: `USERPROFILE`, `LOCALAPPDATA` (`None` when unset
/// or empty), `SystemRoot`, and whether `AGENTSPACE_NESTED` is set and not empty. The program
/// builds it once, as it starts, after the command line is checked.
///
/// Fails with [`NO_HOME`] when `USERPROFILE` is unset or empty, and then with [`NO_SYSTEM_ROOT`]
/// when `SystemRoot` is, in that order; in caller mode either is the [`COULD_NOT_REACH`] line.
pub fn context_from(lookup: EnvLookup) -> Result<Context, Failure> {
    let home = non_empty(lookup, "USERPROFILE").ok_or_else(|| could_not_reach(NO_HOME))?;
    let system_root =
        non_empty(lookup, "SystemRoot").ok_or_else(|| could_not_reach(NO_SYSTEM_ROOT))?;
    Ok(Context {
        home: PathBuf::from(home),
        local_app_data: non_empty(lookup, "LOCALAPPDATA").map(PathBuf::from),
        system_root: PathBuf::from(system_root),
        nested: non_empty(lookup, "AGENTSPACE_NESTED").is_some(),
    })
}

/// Whether `AGENTSPACE_DIAGNOSTIC` is set and not empty. The program reads it once, as it starts.
pub fn diagnostic_from(lookup: EnvLookup) -> bool {
    non_empty(lookup, "AGENTSPACE_DIAGNOSTIC").is_some()
}

/// The whole program for a person's command: reads `args` (the program's own name excluded),
/// writes help to `stdout` and every other message to `stderr`, and returns the exit code, which
/// is 0 or 1 and nothing else.
///
/// A usage error or an `-Alias` refusal writes nothing to `stdout` and starts nothing. A missing
/// `USERPROFILE` or `SystemRoot` fails every command after those checks and before `dispatch`. A
/// failure is one `agentspace: ` line.
pub fn run(
    args: &[String],
    lookup: EnvLookup,
    stdout: &mut dyn Write,
    stderr: Box<dyn Write + Send>,
) -> i32 {
    match parse(args) {
        Parsed::Help(level) => {
            let written = stdout
                .write_all(help(level).as_bytes())
                .and_then(|()| stdout.flush());
            match written {
                Ok(()) => 0,
                // A failure MUST print one `agentspace: ` line (operator.md, Exit codes), even
                // when what failed is help's own write.
                Err(_) => {
                    let say = Printer::new(Mode::Person, false, false, stderr);
                    say.failure(&Failure {
                        message: HELP_NOT_WRITTEN.to_string(),
                        caller: CallerFailure::Same,
                    });
                    1
                }
            }
        }
        Parsed::Failure(message) => {
            let say = Printer::new(Mode::Person, false, false, stderr);
            say.failure(&Failure {
                message,
                caller: CallerFailure::Same,
            });
            1
        }
        Parsed::Run(request) => {
            let say = Printer::new(Mode::Person, request.quiet, false, stderr);
            let result = context_from(lookup).and_then(|context| {
                // The environment is read once, here; nothing below reads it again.
                let env = Env::snapshot(lookup);
                let ca_env = env.ca_env();
                // A person's command: what `ec2 wait instance-stopped` writes to stderr passes
                // through (operator.md, Starting the box and waiting for it, step 1).
                let runner = agentspace_run::Runner::with_lookup(context.clone(), env.lookup())
                    .passing_ignored_stderr();
                let reach = SystemReach::new(env);
                let clock = SystemClock::new();
                dispatch(
                    &request,
                    &Parts {
                        ctx: &context,
                        ca_env: &ca_env,
                        say: &say,
                        report: &say,
                        aws: &runner,
                        remote: &runner,
                        clock: &clock,
                        reach: &reach,
                    },
                )
            });
            match result {
                Ok(outcome) => outcome.exit_code(),
                Err(failure) => {
                    say.failure(&failure);
                    1
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    fn go(line: &[&str]) -> (i32, String, String) {
        let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let err = Capture::default();
        let lookup = env(&[
            ("USERPROFILE", "C:\\Users\\someone"),
            ("SystemRoot", "C:\\Windows"),
        ]);
        let code = run(&args, &lookup, &mut out, Box::new(err.clone()));
        let err_text = String::from_utf8(err.0.lock().unwrap().clone()).unwrap();
        (code, String::from_utf8(out).unwrap(), err_text)
    }

    /// Runs `line` in process in a throwaway home, with the AWS CLI, ssh and scp named as files
    /// that do not exist, so that nothing real can be reached.
    fn go_isolated(line: &[&str], name: &str) -> (i32, String, String, String) {
        let home = crate::testing::TempHome::new(name);
        let userprofile = home.path().join("home");
        let missing = home.path().join("no-such-program");
        let root = home.path().join("root");
        let lookup = env(&[
            ("USERPROFILE", userprofile.to_str().unwrap()),
            ("SystemRoot", root.to_str().unwrap()),
            ("AGENTSPACE_AWS_CMD", missing.to_str().unwrap()),
            ("AGENTSPACE_SSH_CMD", missing.to_str().unwrap()),
            ("AGENTSPACE_SCP_CMD", missing.to_str().unwrap()),
        ]);
        let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let err = Capture::default();
        let code = run(&args, &lookup, &mut out, Box::new(err.clone()));
        let err_text = String::from_utf8(err.0.lock().unwrap().clone()).unwrap();
        (
            code,
            String::from_utf8(out).unwrap(),
            err_text,
            missing.display().to_string(),
        )
    }

    #[test]
    fn an_aws_command_whose_seam_program_cannot_be_run_fails_and_never_falls_back() {
        for command in ["up", "down", "status", "grow"] {
            let (code, out, err, missing) = go_isolated(&[command], "seam-aws");
            assert_eq!((code, out.as_str()), (1, ""), "{command}");
            assert_eq!(
                err,
                format!("agentspace: AGENTSPACE_AWS_CMD names {missing}, which cannot be run.\n"),
                "{command}"
            );
        }
    }

    #[test]
    fn a_failure_prints_even_with_quiet_and_up_writes_nothing_to_stdout() {
        let (code, out, err, _) = go_isolated(&["up", "-Quiet"], "seam-quiet");
        assert_eq!(code, 1);
        assert_eq!(out, "");
        assert_eq!(err.lines().count(), 1);
        assert!(err.starts_with("agentspace: AGENTSPACE_AWS_CMD names"));
    }

    #[test]
    fn help_goes_to_stdout_with_exit_0_and_nothing_on_stderr() {
        let (code, out, err) = go(&["grow", "-h"]);
        assert_eq!(code, 0);
        assert_eq!(out, help(HelpLevel::Command(Command::Grow)));
        assert_eq!(err, "");
    }

    struct Broken;

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn help_that_cannot_be_written_fails_with_one_agentspace_line() {
        let args = vec!["up".to_string(), "-h".to_string()];
        let err = Capture::default();
        let lookup = env(&[]);
        let code = run(&args, &lookup, &mut Broken, Box::new(err.clone()));
        assert_eq!(code, 1);
        assert_eq!(
            String::from_utf8(err.0.lock().unwrap().clone()).unwrap(),
            format!("{HELP_NOT_WRITTEN}\n")
        );
    }

    #[test]
    fn a_usage_error_writes_one_line_to_stderr_and_nothing_to_stdout() {
        let (code, out, err) = go(&["up", "-Alias", "a b"]);
        assert_eq!(code, 1);
        assert_eq!(out, "");
        assert_eq!(err.lines().count(), 1);
        assert!(err.starts_with("agentspace: -Alias MUST be"));
    }

    #[test]
    fn the_context_is_built_from_the_lookup() {
        let lookup = env(&[
            ("USERPROFILE", "C:\\Users\\someone"),
            ("LOCALAPPDATA", "C:\\Users\\someone\\AppData\\Local"),
            ("SystemRoot", "C:\\Windows"),
            ("AGENTSPACE_NESTED", "1"),
        ]);
        let ctx = context_from(&lookup).unwrap();
        assert_eq!(ctx.home, PathBuf::from("C:\\Users\\someone"));
        assert_eq!(
            ctx.local_app_data,
            Some(PathBuf::from("C:\\Users\\someone\\AppData\\Local"))
        );
        assert_eq!(ctx.system_root, PathBuf::from("C:\\Windows"));
        assert!(ctx.nested);
    }

    #[test]
    fn an_empty_or_unset_variable_is_none_or_false() {
        let lookup = env(&[
            ("USERPROFILE", "C:\\Users\\someone"),
            ("SystemRoot", "C:\\Windows"),
            ("LOCALAPPDATA", ""),
            ("AGENTSPACE_NESTED", ""),
        ]);
        let ctx = context_from(&lookup).unwrap();
        assert_eq!(ctx.local_app_data, None);
        assert!(!ctx.nested);
        assert!(!diagnostic_from(&lookup));
        assert!(!diagnostic_from(&env(&[("AGENTSPACE_DIAGNOSTIC", "")])));
        assert!(diagnostic_from(&env(&[("AGENTSPACE_DIAGNOSTIC", "1")])));
    }

    fn go_with(line: &[&str], pairs: &[(&str, &str)]) -> (i32, String, String) {
        let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let err = Capture::default();
        let lookup = env(pairs);
        let code = run(&args, &lookup, &mut out, Box::new(err.clone()));
        let err_text = String::from_utf8(err.0.lock().unwrap().clone()).unwrap();
        (code, String::from_utf8(out).unwrap(), err_text)
    }

    #[test]
    fn every_command_fails_without_a_home_or_a_system_folder() {
        let no_home = "agentspace: USERPROFILE is not set, so there is no home folder.\n";
        let no_root = "agentspace: SystemRoot is not set, so Windows' programs cannot be found.\n";
        for c in Command::ALL {
            // `provision` needs `-Host`, which a usage error would otherwise report first.
            let line: Vec<&str> = if c == Command::Provision {
                vec![c.name(), "-Host", "agentspace"]
            } else {
                vec![c.name()]
            };
            // USERPROFILE unset or empty, SystemRoot present or not: the home is checked first.
            for pairs in [
                vec![("SystemRoot", "C:\\Windows")],
                vec![("USERPROFILE", ""), ("SystemRoot", "C:\\Windows")],
                vec![],
                vec![("USERPROFILE", ""), ("SystemRoot", "")],
            ] {
                let (code, out, err) = go_with(&line, &pairs);
                assert_eq!(
                    (code, out.as_str(), err.as_str()),
                    (1, "", no_home),
                    "{c:?}"
                );
            }
            // SystemRoot unset or empty.
            for pairs in [
                vec![("USERPROFILE", "C:\\Users\\someone")],
                vec![("USERPROFILE", "C:\\Users\\someone"), ("SystemRoot", "")],
            ] {
                let (code, out, err) = go_with(&line, &pairs);
                assert_eq!(
                    (code, out.as_str(), err.as_str()),
                    (1, "", no_root),
                    "{c:?}"
                );
            }
        }
    }

    #[test]
    fn the_home_check_comes_after_help_usage_errors_and_alias_checks() {
        let none: &[(&str, &str)] = &[];
        let (code, out, err) = go_with(&["up", "-h"], none);
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (0, help(HelpLevel::Command(Command::Up)), "")
        );
        let (code, _, err) = go_with(&["status", "-GiB", "abc"], none);
        assert_eq!(
            (code, err.as_str()),
            (1, "agentspace: -GiB MUST be a whole number, not 'abc'.\n")
        );
        let (code, _, err) = go_with(&["status", "-Alias", "x"], none);
        assert_eq!(
            (code, err.as_str()),
            (1, "agentspace: -Alias applies to up alone.\n")
        );
        let (code, _, err) = go_with(&["up", "-Alias", "bad!"], none);
        assert!(err.starts_with("agentspace: -Alias MUST be"), "{err}");
        assert_eq!(code, 1);
    }

    #[test]
    fn a_missing_home_or_system_folder_is_the_could_not_reach_line_in_caller_mode() {
        for (variable, message) in [("USERPROFILE", NO_HOME), ("SystemRoot", NO_SYSTEM_ROOT)] {
            let pairs: Vec<(&str, &str)> = [
                ("USERPROFILE", "C:\\Users\\someone"),
                ("SystemRoot", "C:\\Windows"),
            ]
            .into_iter()
            .filter(|(k, _)| *k != variable)
            .collect();
            let lookup = env(&pairs);
            let failure = context_from(&lookup).unwrap_err();
            assert_eq!(failure.message, message);
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
            let cap = Capture::default();
            let say = Printer::new(Mode::Caller, false, false, Box::new(cap.clone()));
            say.failure(&failure);
            assert_eq!(
                String::from_utf8(cap.0.lock().unwrap().clone()).unwrap(),
                format!("{COULD_NOT_REACH}\n")
            );
        }
    }
}

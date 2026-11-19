//! The program: reads the command line, runs the command, and prints one document (lease-api.md,
//! The program and Output).

use std::cell::Cell;
use std::ffi::OsString;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Once;

use serde_json::{Map, Value};

use crate::parse::{Parsed, parse};
use crate::quote::escape_controls;
use crate::{ErrorCode, LeaseError, ParsedCommand, Sub, commands};

/// What a run produced, before any of it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The exit code, one of the table's.
    pub exit_code: u8,
    /// What goes to stdout: help, or one JSON document and a line feed; nothing on failure.
    pub stdout: Vec<u8>,
    /// What goes to stderr: one error document and a line feed on failure; nothing on success.
    pub stderr: Vec<u8>,
    /// The subcommand the run reached, which a failure to write stdout names in its message.
    pub sub: Option<Sub>,
}

/// What a run that did not fail printed.
enum Printed {
    Help(&'static str),
    Document(Value),
}

thread_local! {
    /// Set while a run is under `catch_unwind`, so a panic prints nothing of its own.
    static SILENT: Cell<bool> = const { Cell::new(false) };
}

static HOOK: Once = Once::new();

type Hook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

/// A hook that passes a panic to `previous` unless a run is under way on this thread.
fn silenced_during_a_run(previous: Hook) -> Hook {
    Box::new(move |info| {
        if !SILENT.with(Cell::get) {
            previous(info);
        }
    })
}

/// Keeps the default panic report away from stderr while a run is under way, and leaves it alone
/// for every other panic in the process.
fn install_hook() {
    HOOK.call_once(|| {
        std::panic::set_hook(silenced_during_a_run(std::panic::take_hook()));
    });
}

/// Sets `SILENT` for as long as it lives.
struct Silence;

impl Silence {
    fn begin() -> Silence {
        SILENT.with(|s| s.set(true));
        Silence
    }
}

impl Drop for Silence {
    fn drop(&mut self) {
        SILENT.with(|s| s.set(false));
    }
}

type Runner<'a> = &'a dyn Fn(ParsedCommand, &Path) -> Result<Value, LeaseError>;

fn work(
    args: &[OsString],
    own_exe: &Path,
    sub: &Cell<Option<Sub>>,
    runner: Runner<'_>,
) -> Result<Printed, LeaseError> {
    match parse(args) {
        Parsed::Help(text, at) => {
            sub.set(at);
            Ok(Printed::Help(text))
        }
        Parsed::Error(error) => Err(error),
        Parsed::Command(cmd) => {
            sub.set(Some(cmd.sub));
            runner(cmd, own_exe).map(Printed::Document)
        }
    }
}

/// The error document: compact, on one line, with a line feed after it.
fn error_document(error: &LeaseError) -> Vec<u8> {
    // The message is agentspace's own words, but it MUST NOT hold a control character, MUST NOT
    // be empty and MUST NOT start with a space, whoever built it.
    let escaped = escape_controls(&error.message);
    let mut message = escaped.trim_start_matches(' ').to_owned();
    if message.is_empty() {
        message = "agentspace lease failed.".to_owned();
    }
    let mut doc = Map::new();
    doc.insert("code".to_owned(), Value::from(error.code.as_str()));
    doc.insert("message".to_owned(), Value::from(message));
    for (name, value) in [
        ("field", &error.field),
        ("lease_id", &error.lease_id),
        ("state", &error.state),
    ] {
        if let Some(value) = value {
            doc.insert(name.to_owned(), Value::from(value.as_str()));
        }
    }
    let mut bytes = Value::Object(doc).to_string().into_bytes();
    bytes.push(b'\n');
    bytes
}

fn failure(error: &LeaseError, sub: Option<Sub>) -> Outcome {
    Outcome {
        exit_code: error.code.exit_code(),
        stdout: Vec::new(),
        stderr: error_document(error),
        sub,
    }
}

/// Runs the program on `args`, which are the arguments after the program's name, and returns what
/// it would print. A panic anywhere in the run ends as one `internal` error document and exit 1.
pub fn execute(args: &[OsString], own_exe: &Path) -> Outcome {
    execute_with(args, own_exe, &commands::run)
}

fn execute_with(args: &[OsString], own_exe: &Path, runner: Runner<'_>) -> Outcome {
    install_hook();
    let sub = Cell::new(None);
    let result = {
        let _quiet = Silence::begin();
        catch_unwind(AssertUnwindSafe(|| work(args, own_exe, &sub, runner)))
    };
    match result {
        Ok(Ok(Printed::Help(text))) => Outcome {
            exit_code: 0,
            stdout: text.as_bytes().to_vec(),
            stderr: Vec::new(),
            sub: sub.get(),
        },
        Ok(Ok(Printed::Document(value))) => {
            let mut stdout = value.to_string().into_bytes();
            stdout.push(b'\n');
            Outcome {
                exit_code: 0,
                stdout,
                stderr: Vec::new(),
                sub: sub.get(),
            }
        }
        Ok(Err(error)) => failure(&error, sub.get()),
        Err(_) => {
            let place = match sub.get() {
                Some(sub) => format!("agentspace lease {} failed", sub.name()),
                None => "agentspace lease failed".to_owned(),
            };
            failure(
                &LeaseError {
                    code: ErrorCode::Internal,
                    message: format!("{place}: An unexpected failure stopped the command."),
                    field: None,
                    lease_id: None,
                    state: None,
                },
                sub.get(),
            )
        }
    }
}

/// Runs the program on `args`, the arguments after the program's name, and writes its output to
/// stdout and stderr. `own_exe` is the program's own executable, from the operating system and
/// never from the first argument.
pub fn main_with(args: Vec<OsString>, own_exe: PathBuf) -> ExitCode {
    let outcome = execute(&args, &own_exe);
    let mut stdout = std::io::stdout().lock();
    let out: Option<&mut dyn Write> = if stdout_is_missing(&stdout) {
        None
    } else {
        Some(&mut stdout)
    };
    let code = deliver(outcome, out, &mut std::io::stderr().lock());
    ExitCode::from(code)
}

/// Whether the program was started without a stdout. On Windows, `std` reports a write to a missing
/// handle (none, or an invalid one) as done, so the handle is looked at here (lease-api.md, Output;
/// architect, 2026-11-19).
#[cfg(windows)]
fn stdout_is_missing(stdout: &std::io::StdoutLock<'_>) -> bool {
    use std::os::windows::io::{AsRawHandle, RawHandle};
    let handle = stdout.as_raw_handle();
    handle.is_null() || handle == (-1isize) as RawHandle
}

/// Whether the program was started without a stdout. On a system other than Windows, a closed
/// stdout MAY read as one that discards its output, because Rust's runtime reopens it on
/// `/dev/null` before `main`, so there is nothing to see (lease-api.md, Output; architect,
/// 2026-11-19).
#[cfg(not(windows))]
fn stdout_is_missing(_stdout: &std::io::StdoutLock<'_>) -> bool {
    false
}

/// Writes `data` and flushes, so a failure at either step is a failed write.
fn write_all(out: &mut dyn Write, data: &[u8]) -> std::io::Result<()> {
    out.write_all(data)?;
    out.flush()
}

/// Writes an outcome to the two streams and returns the exit code. A write to stdout that fails,
/// at the write or at the flush, is a failure whatever the run's result was: 0 MUST mean done
/// (lease-api.md, Output; D36), so the caller gets one `internal` error document on stderr and
/// exit 1. When stderr cannot be written either, the exit code is still 1. `out` is `None` when
/// the program has no stdout at all, which counts as a failed write.
fn deliver(outcome: Outcome, out: Option<&mut dyn Write>, err: &mut dyn Write) -> u8 {
    // An empty stdout is never written, so a run that failed has nothing to fail at.
    let wrote =
        outcome.stdout.is_empty() || out.is_some_and(|out| write_all(out, &outcome.stdout).is_ok());
    if !wrote {
        let place = match outcome.sub {
            Some(sub) => format!("agentspace lease {} failed", sub.name()),
            None => "agentspace lease failed".to_owned(),
        };
        let failed = failure(
            &LeaseError::internal(format!("{place}: The document could not be written.")),
            outcome.sub,
        );
        let _ = write_all(err, &failed.stderr);
        return failed.exit_code;
    }
    if !outcome.stderr.is_empty() {
        // The error document is the only report of the failure, and the exit code carries it when
        // this write fails.
        let _ = write_all(err, &outcome.stderr);
    }
    outcome.exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn a_panic_in_a_command_is_one_internal_document_and_exit_1() {
        let out = execute_with(&args(&["lease", "get", "x"]), Path::new(""), &|_, _| {
            panic!("a path /home/someone/secret")
        });
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            String::from_utf8(out.stderr).unwrap(),
            "{\"code\":\"internal\",\"message\":\"agentspace lease get failed: An unexpected failure stopped the command.\"}\n"
        );
    }

    #[test]
    fn success_is_one_compact_document_and_a_line_feed_in_the_given_order() {
        let value: Value = serde_json::from_str(r#"{"id":"1","state":"ready","a":1}"#).unwrap();
        let out = execute_with(&args(&["lease", "list"]), Path::new(""), &move |_, _| {
            Ok(value.clone())
        });
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, b"{\"id\":\"1\",\"state\":\"ready\",\"a\":1}\n");
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn an_error_document_carries_its_fields_and_the_codes_exit_code() {
        let out = execute_with(&args(&["lease", "get", "x"]), Path::new(""), &|_, _| {
            Err(LeaseError {
                code: ErrorCode::InvalidState,
                message: "Lease is ready.".to_owned(),
                field: None,
                lease_id: Some("3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f".to_owned()),
                state: Some("ready".to_owned()),
            })
        });
        assert_eq!(out.exit_code, 4);
        assert!(out.stdout.is_empty());
        assert_eq!(
            String::from_utf8(out.stderr).unwrap(),
            "{\"code\":\"invalid_state\",\"message\":\"Lease is ready.\",\"lease_id\":\"3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f\",\"state\":\"ready\"}\n"
        );
    }

    #[test]
    fn a_message_is_never_empty_never_starts_with_a_space_and_never_holds_a_control_character() {
        for (given, shown) in [
            ("", "agentspace lease failed."),
            ("  x", "x"),
            ("a\nb", "a\\u000ab"),
        ] {
            let doc = error_document(&LeaseError::internal(given));
            let value: Value = serde_json::from_slice(&doc).unwrap();
            assert_eq!(value["message"], shown);
        }
    }

    #[test]
    fn exit_codes_are_the_tables() {
        let codes = [
            (ErrorCode::Internal, 1),
            (ErrorCode::Usage, 2),
            (ErrorCode::InvalidValue, 2),
            (ErrorCode::OutOfRange, 2),
            (ErrorCode::NotFound, 3),
            (ErrorCode::InvalidState, 4),
        ];
        for (code, exit) in codes {
            assert_eq!(code.exit_code(), exit);
        }
    }

    #[test]
    fn this_build_answers_every_subcommand_with_internal() {
        let out = execute(&args(&["lease", "list"]), Path::new(""));
        assert_eq!(out.exit_code, 1);
        assert_eq!(
            String::from_utf8(out.stderr).unwrap(),
            "{\"code\":\"internal\",\"message\":\"agentspace lease list failed: This build does not have the command yet.\"}\n"
        );
    }

    /// A writer that fails at the write, or at the flush only.
    struct Broken {
        fail_write: bool,
        fail_flush: bool,
        got: Vec<u8>,
    }

    impl Broken {
        fn new(fail_write: bool, fail_flush: bool) -> Broken {
            Broken {
                fail_write,
                fail_flush,
                got: Vec::new(),
            }
        }
    }

    impl Write for Broken {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.fail_write {
                return Err(std::io::Error::other("no space"));
            }
            self.got.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.fail_flush {
                Err(std::io::Error::other("no space"))
            } else {
                Ok(())
            }
        }
    }

    fn good_outcome(sub: Option<Sub>) -> Outcome {
        Outcome {
            exit_code: 0,
            stdout: b"{\"a\":1}\n".to_vec(),
            stderr: Vec::new(),
            sub,
        }
    }

    const FAILED_WRITE: &str = "{\"code\":\"internal\",\"message\":\"agentspace lease failed: The document could not be written.\"}\n";

    #[test]
    fn a_stdout_write_that_fails_is_one_internal_document_on_stderr_and_exit_1() {
        let mut out = Broken::new(true, false);
        let mut err = Broken::new(false, false);
        assert_eq!(deliver(good_outcome(None), Some(&mut out), &mut err), 1);
        assert_eq!(String::from_utf8(err.got).unwrap(), FAILED_WRITE);
        assert!(out.got.is_empty());
    }

    #[test]
    fn a_stdout_flush_that_fails_is_a_failed_write() {
        let mut out = Broken::new(false, true);
        let mut err = Broken::new(false, false);
        assert_eq!(deliver(good_outcome(None), Some(&mut out), &mut err), 1);
        assert_eq!(String::from_utf8(err.got).unwrap(), FAILED_WRITE);
    }

    #[test]
    fn a_failed_write_after_a_subcommand_names_it() {
        let mut out = Broken::new(true, false);
        let mut err = Broken::new(false, false);
        assert_eq!(
            deliver(good_outcome(Some(Sub::Create)), Some(&mut out), &mut err),
            1
        );
        assert_eq!(
            String::from_utf8(err.got).unwrap(),
            "{\"code\":\"internal\",\"message\":\"agentspace lease create failed: The document could not be written.\"}\n"
        );
    }

    #[test]
    fn exit_1_stands_when_stderr_cannot_be_written_either() {
        for (fail_write, fail_flush) in [(true, false), (false, true)] {
            let mut out = Broken::new(true, false);
            let mut err = Broken::new(fail_write, fail_flush);
            assert_eq!(deliver(good_outcome(None), Some(&mut out), &mut err), 1);
        }
    }

    #[test]
    fn a_stdout_that_works_leaves_the_exit_code_alone() {
        let mut out = Broken::new(false, false);
        let mut err = Broken::new(false, false);
        assert_eq!(deliver(good_outcome(None), Some(&mut out), &mut err), 0);
        assert_eq!(out.got, b"{\"a\":1}\n");
        assert!(err.got.is_empty());
    }

    #[test]
    fn a_failed_run_keeps_its_own_exit_code_when_stderr_cannot_be_written() {
        let outcome = failure(
            &LeaseError {
                code: ErrorCode::NotFound,
                message: "x".to_owned(),
                field: None,
                lease_id: None,
                state: None,
            },
            Some(Sub::Get),
        );
        let mut out = Broken::new(true, false);
        let mut err = Broken::new(true, false);
        assert_eq!(deliver(outcome, Some(&mut out), &mut err), 3);
    }

    #[test]
    fn no_stdout_at_all_is_a_failed_write() {
        let mut err = Broken::new(false, false);
        assert_eq!(deliver(good_outcome(None), None, &mut err), 1);
        assert_eq!(String::from_utf8(err.got).unwrap(), FAILED_WRITE);
    }

    #[test]
    fn no_stdout_at_all_leaves_a_run_with_nothing_to_write_alone() {
        let outcome = failure(&LeaseError::internal("x"), None);
        let mut err = Broken::new(false, false);
        assert_eq!(deliver(outcome, None, &mut err), 1);
        assert_eq!(
            String::from_utf8(err.got).unwrap(),
            "{\"code\":\"internal\",\"message\":\"x\"}\n"
        );
    }

    const NO_RUNNER: Runner<'static> = &|_, _| panic!("help never reaches a command");

    #[test]
    fn help_names_the_subcommand_only_when_read_before_the_first_help_request() {
        let at = |list: &[&str]| execute_with(&args(list), Path::new(""), NO_RUNNER).sub;
        assert_eq!(at(&["lease", "create", "--help"]), Some(Sub::Create));
        assert_eq!(at(&["lease", "--help", "create"]), None);
        assert_eq!(at(&["--help", "lease", "create"]), None);
        assert_eq!(at(&["--help"]), None);
    }

    #[test]
    fn a_failed_write_of_help_names_the_subcommand_read_before_the_help_request() {
        let failed = |list: &[&str]| {
            let outcome = execute_with(&args(list), Path::new(""), NO_RUNNER);
            assert_eq!(outcome.exit_code, 0);
            let mut out = Broken::new(true, false);
            let mut err = Broken::new(false, false);
            assert_eq!(deliver(outcome, Some(&mut out), &mut err), 1);
            String::from_utf8(err.got).unwrap()
        };
        assert_eq!(
            failed(&["lease", "create", "--help"]),
            "{\"code\":\"internal\",\"message\":\"agentspace lease create failed: The document could not be written.\"}\n"
        );
        assert_eq!(failed(&["lease", "--help", "create"]), FAILED_WRITE);
        assert_eq!(failed(&["--help", "lease", "create"]), FAILED_WRITE);
    }

    thread_local! {
        /// How many panics the recording hook was handed on this thread.
        static REPORTED: Cell<u32> = const { Cell::new(0) };
    }

    #[test]
    fn a_panic_in_a_run_is_not_reported_and_one_outside_a_run_is() {
        // Puts a recording hook under the same wrapping, so this test sees what the wrapping hands
        // on. A panic on another thread reaches the recording hook but is counted on that thread
        // only. This test MUST NOT call `install_hook` itself: the next test needs to see that a
        // run does.
        let installed = std::panic::take_hook();
        std::panic::set_hook(silenced_during_a_run(Box::new(|_| {
            REPORTED.with(|n| n.set(n.get() + 1));
        })));

        let out = execute_with(&args(&["lease", "get", "x"]), Path::new(""), &|_, _| {
            panic!("inside a run")
        });
        assert_eq!(out.exit_code, 1);
        let inside = REPORTED.with(Cell::get);

        let outside = catch_unwind(|| panic!("outside a run"));
        assert!(outside.is_err());
        let after = REPORTED.with(Cell::get);

        std::panic::set_hook(installed);
        assert_eq!(inside, 0, "A panic in a run MUST NOT be reported");
        assert_eq!(after, 1, "A panic outside a run MUST be reported");
    }

    #[test]
    fn a_run_installs_the_hook_that_keeps_a_panic_report_off_stderr() {
        execute_with(&args(&["lease", "get", "x"]), Path::new(""), &|_, _| {
            panic!("inside a run")
        });
        assert!(HOOK.is_completed());
    }
}

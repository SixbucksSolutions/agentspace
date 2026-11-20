//! The programs the operator starts: the AWS CLI, ssh and scp (docs/operator.md; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! # Public API
//!
//! - [`Runner`]: built from a pinned [`Context`], by [`Runner::new`] (the seam variables are read
//!   from the real environment) or [`Runner::with_lookup`] (an injectable variable lookup, for
//!   tests). It implements the pinned [`Aws`] and [`Remote`]. [`Runner::passing_ignored_stderr`]
//!   opts in to an [`Output::Ignore`] call's stderr reaching the program's own stderr.
//! - [`prepare_child_env`]: applies the environment rules to any [`std::process::Command`]; later
//!   stories (the push's ssh, the tunnel, `go build`) MUST call it on theirs.
//! - [`ssh_program`]: the ssh the push's own pipe-wired ssh MUST be started from (T02).
//! - [`ssh_program_with`]: the same, over an injectable variable lookup.
//!
//! # What this crate guarantees
//!
//! - **Which program runs** (operator.md, Programs it starts, The test seam): the AWS CLI at its
//!   fixed path, else `aws` on `PATH`; Windows OpenSSH's `ssh.exe` and `scp.exe` under
//!   `%SystemRoot%\System32\OpenSSH`, by full path and never through `PATH` (#0027); or the one
//!   path `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` or `AGENTSPACE_SCP_CMD` names, read each
//!   time a program is about to start. A value that names nothing runnable fails and MUST NOT
//!   fall back to the default.
//! - **The environment at start** (operator.md, Environment variables): every program gets its
//!   environment with every variable whose name ends `_POSIX_FD_STATE` removed (#0026) and
//!   `AGENTSPACE_NESTED` set to `1`.
//! - **Standard input** is closed (`/dev/null` or `NUL`) for every program started here.
//! - **Sign-in test** (#0034): `expired`, `aws login` or `Unable to locate credentials` in the
//!   CLI's error text, whatever the case, is [`AwsError::SignIn`].
//! - **The remote command** is one argument, the last, and is refused when it holds a `%`, a
//!   double quote or a control character, or ends in a backslash (gap FA-15).
//!
//! The library writes to stdout and stderr only what the spec passes through: `ssh`'s and `scp`'s
//! streams, a [`Output::PassThrough`] call's streams, and an [`Output::Ignore`] call's stderr when
//! the runner opted in. An [`Output::Ignore`] call's stdout is never passed.
//!
//! **A program that uses [`Output::PassThrough`] MUST NOT hold its stdout or stderr lock across a
//! command; the passed-through output is written from other threads** (architect, 2026-11-20, for
//! E0.06-T05-S02, #0139). A `StdoutLock` is reentrant only on the thread that holds it, so a copy
//! thread's first write would wait for the lock while the call waits for the copy thread.

#![forbid(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;

use agentspace_operator_api::{
    Aws, AwsError, Call, CallerFailure, Context, Failure, Output, Remote,
};
use serde_json::Value;

/// The seam variable that names the AWS CLI.
const AWS_VAR: &str = "AGENTSPACE_AWS_CMD";
/// The seam variable that names `ssh`.
const SSH_VAR: &str = "AGENTSPACE_SSH_CMD";
/// The seam variable that names `scp`.
const SCP_VAR: &str = "AGENTSPACE_SCP_CMD";
/// The AWS CLI v2's fixed path (operator.md, Programs it starts).
const AWS_FIXED_PATH: &str = r"C:\Program Files\Amazon\AWSCLIV2\aws.exe";
/// The region every call passes (operator.md, AWS failures).
const REGION: &str = "us-west-2";

type Lookup = dyn Fn(&str) -> Option<OsString> + Send + Sync;

/// Starts the AWS CLI, ssh and scp. Implements [`Aws`] and [`Remote`].
#[derive(Clone)]
pub struct Runner {
    ctx: Context,
    lookup: Arc<Lookup>,
    /// Whether an [`Output::Ignore`] call's stderr goes to the program's own stderr
    /// ([`Runner::passing_ignored_stderr`]); withheld when `false`, the default.
    pass_ignored_stderr: bool,
}

impl std::fmt::Debug for Runner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("ctx", &self.ctx)
            .field("pass_ignored_stderr", &self.pass_ignored_stderr)
            .finish_non_exhaustive()
    }
}

impl Runner {
    /// A runner that reads the seam variables from the real environment, each time a program is
    /// about to start.
    pub fn new(ctx: Context) -> Self {
        Self::with_lookup(ctx, |name| std::env::var_os(name))
    }

    /// A runner that reads the seam variables through `lookup`, which MUST answer for a
    /// variable's name as the process environment would.
    pub fn with_lookup<F>(ctx: Context, lookup: F) -> Self
    where
        F: Fn(&str) -> Option<OsString> + Send + Sync + 'static,
    {
        Self {
            ctx,
            lookup: Arc::new(lookup),
            pass_ignored_stderr: false,
        }
    }

    /// The same runner, but one that sends an [`Output::Ignore`] call's stderr to the program's
    /// own stderr, as the program wrote it (operator.md, Starting the box and waiting for it,
    /// step 1: what `ec2 wait instance-stopped` writes to stderr passes through in a person's
    /// `up`). Its stdout is never passed, so `up` still writes nothing to stdout.
    ///
    /// **By default that stderr is withheld**, as the ProxyCommand's caller mode requires (What a
    /// caller sees). The operator's program MUST opt in for a person's command, and the
    /// ProxyCommand MUST NOT.
    pub fn passing_ignored_stderr(mut self) -> Self {
        self.pass_ignored_stderr = true;
        self
    }

    /// The ssh this runner starts (see [`ssh_program_with`]).
    pub fn ssh_program(&self) -> Result<PathBuf, Failure> {
        ssh_program_with(&self.ctx, &*self.lookup)
    }

    fn aws_program(&self) -> Result<Program, AwsError> {
        match seam(&*self.lookup, AWS_VAR) {
            Some(value) => Program::from_seam(AWS_VAR, &value).map_err(AwsError::CannotRun),
            None if Path::new(AWS_FIXED_PATH).is_file() => {
                Ok(Program::default(PathBuf::from(AWS_FIXED_PATH)))
            }
            None => find_on_path(&*self.lookup)
                .map(Program::default)
                .ok_or_else(|| {
                    AwsError::CannotRun(format!(
                        "agentspace: The AWS CLI is not installed: {AWS_FIXED_PATH} is missing and no aws is on PATH."
                    ))
                }),
        }
    }

    fn run_remote(&self, program: &Program, args: &[&OsStr]) -> Result<i32, Failure> {
        let mut cmd = command(&program.path);
        cmd.args(args).stdin(Stdio::null());
        match cmd.status() {
            Ok(status) => Ok(exit_code(status)),
            Err(_) => Err(Failure {
                message: program.cannot_start.clone(),
                caller: CallerFailure::CouldNotReach,
            }),
        }
    }
}

/// A program about to be started, with the line its start failing gives: the seam's message for
/// a program a seam variable names, since a file the system cannot start is one that names
/// nothing runnable (operator.md, The test seam), or "Could not run" for a default (Programs it
/// starts, "When a default program cannot be run").
struct Program {
    path: PathBuf,
    cannot_start: String,
}

impl Program {
    fn default(path: PathBuf) -> Self {
        let cannot_start = cannot_run_message(&path);
        Self { path, cannot_start }
    }

    fn from_seam(var: &str, value: &OsStr) -> Result<Self, String> {
        let path = runnable(var, value)?;
        Ok(Self {
            path,
            cannot_start: seam_message(var, value),
        })
    }
}

/// The ssh the token push starts, found from the real environment: the program
/// `AGENTSPACE_SSH_CMD` names, or Windows OpenSSH's `ssh.exe` by full path.
///
/// A value that names nothing runnable fails, and MUST NOT fall back to the default.
pub fn ssh_program(ctx: &Context) -> Result<PathBuf, Failure> {
    ssh_program_with(ctx, &|name| std::env::var_os(name))
}

/// [`ssh_program`] over an injectable variable lookup.
pub fn ssh_program_with(
    ctx: &Context,
    lookup: &dyn Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, Failure> {
    program_or_default(ctx, lookup, SSH_VAR, "ssh.exe").map(|program| program.path)
}

/// The seam variable's value, or `None` when it is unset, empty or only whitespace.
fn seam(lookup: &dyn Fn(&str) -> Option<OsString>, name: &str) -> Option<OsString> {
    let value = lookup(name)?;
    if value.to_string_lossy().trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn program_or_default(
    ctx: &Context,
    lookup: &dyn Fn(&str) -> Option<OsString>,
    var: &str,
    exe: &str,
) -> Result<Program, Failure> {
    match seam(lookup, var) {
        Some(value) => Program::from_seam(var, &value).map_err(|message| Failure {
            message,
            caller: CallerFailure::CouldNotReach,
        }),
        None => {
            let path = ctx.system_root.join("System32").join("OpenSSH").join(exe);
            if path.is_file() {
                Ok(Program::default(path))
            } else {
                Err(Failure {
                    message: format!(
                        "agentspace: {} is missing; install the Windows OpenSSH client.",
                        path.display()
                    ),
                    caller: CallerFailure::CouldNotReach,
                })
            }
        }
    }
}

/// The `aws` the AWS CLI's fallback finds on `PATH` (read through `lookup`), as a full path.
fn find_on_path(lookup: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let path = lookup("PATH")?;
    let names: &[&str] = if cfg!(windows) {
        &["aws.exe", "aws.cmd", "aws.bat"]
    } else {
        &["aws"]
    };
    std::env::split_paths(&path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| is_runnable_file(candidate))
}

fn is_runnable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

/// A seam value taken whole as one path, resolved against the working folder, never looked up on
/// `PATH`. Fails with the spec's message when it names nothing runnable.
fn runnable(var: &str, value: &OsStr) -> Result<PathBuf, String> {
    let fail = || seam_message(var, value);
    let path = std::path::absolute(Path::new(value)).map_err(|_| fail())?;
    if is_runnable_file(&path) {
        Ok(path)
    } else {
        Err(fail())
    }
}

/// The seam's line for a value that names nothing runnable (operator.md, The test seam).
fn seam_message(var: &str, value: &OsStr) -> String {
    format!(
        "agentspace: {var} names {}, which cannot be run.",
        value.to_string_lossy()
    )
}

/// The line for a default program that exists but cannot be started (operator.md, Programs it
/// starts, "When a default program cannot be run").
fn cannot_run_message(program: &Path) -> String {
    format!("agentspace: Could not run {}.", program.display())
}

fn exit_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    -1
}

/// Whether `name` is a variable the operator MUST remove before it starts a program (#0026).
fn is_posix_fd_state(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    const SUFFIX: &str = "_POSIX_FD_STATE";
    if cfg!(windows) {
        // Windows names its variables without regard to case.
        name.to_ascii_uppercase().ends_with(SUFFIX)
    } else {
        name.ends_with(SUFFIX)
    }
}

/// Gives `cmd` the environment the operator gives everything it starts (operator.md,
/// Environment variables): every variable of the operator's own environment whose name ends
/// `_POSIX_FD_STATE` removed (#0026), and `AGENTSPACE_NESTED` set to `1`.
///
/// Every program the operator starts MUST go through this, since the operator cannot change its
/// own environment without `unsafe`.
pub fn prepare_child_env(cmd: &mut Command) {
    // A variable already set on the command with `.env()` is as bad as an inherited one.
    let own: Vec<OsString> = cmd
        .get_envs()
        .map(|(name, _)| name.to_os_string())
        .collect();
    prepare_with_env(
        cmd,
        own.into_iter()
            .chain(std::env::vars_os().map(|(name, _)| name)),
    );
}

fn prepare_with_env(cmd: &mut Command, env_names: impl Iterator<Item = OsString>) {
    for name in env_names {
        if is_posix_fd_state(&name) {
            cmd.env_remove(name);
        }
    }
    cmd.env("AGENTSPACE_NESTED", "1");
}

/// A command for `program`, prepared by [`prepare_child_env`].
fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    prepare_child_env(&mut cmd);
    cmd
}

/// Whether a call passes `--output json` (operator.md, AWS failures; Calls by command): every
/// call but `cloudformation deploy` and `ec2 wait instance-stopped` does, whether or not its
/// output is read. The tunnel is not started here. The call's shape decides, not its [`Output`]:
/// an [`Output::Ignore`], [`Output::PassThrough`] or [`Output::Checked`] call of any other shape
/// still passes it.
fn passes_output_json(call: &Call) -> bool {
    let deploy = call.service == "cloudformation" && call.operation == "deploy";
    let wait_stopped = call.service == "ec2"
        && call.operation == "wait"
        && call.args.first() == Some(&"instance-stopped");
    !(deploy || wait_stopped)
}

/// The AWS CLI's arguments, in gap FB-07's order.
fn aws_args(call: &Call) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![call.service.into(), call.operation.into()];
    args.extend(["--region", REGION].map(OsString::from));
    if passes_output_json(call) {
        args.extend(["--output", "json"].map(OsString::from));
    }
    args.extend(call.args.iter().map(OsString::from));
    args
}

/// The sign-in test (#0034).
fn is_sign_in_failure(error_text: &str) -> bool {
    let text = error_text.to_lowercase();
    ["expired", "aws login", "unable to locate credentials"]
        .iter()
        .any(|phrase| text.contains(phrase))
}

/// The failure for an AWS CLI's error text and exit status.
fn aws_failure(stderr: &[u8], stdout: &[u8], status: ExitStatus) -> AwsError {
    let mut text = String::from_utf8_lossy(stderr).trim().to_string();
    if text.is_empty() {
        text = String::from_utf8_lossy(stdout).trim().to_string();
    }
    if is_sign_in_failure(&text) {
        return AwsError::SignIn;
    }
    if text.is_empty() {
        text = format!("The AWS CLI failed with exit code {}.", exit_code(status));
    }
    AwsError::Failed(text)
}

impl Aws for Runner {
    fn call(&self, call: &Call) -> Result<Value, AwsError> {
        let program = self.aws_program()?;
        let cannot_run = || AwsError::CannotRun(program.cannot_start.clone());
        let mut cmd = command(&program.path);
        cmd.args(aws_args(call)).stdin(Stdio::null());
        match call.output {
            Output::Json | Output::Checked => {
                let out = cmd
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|_| cannot_run())?;
                if !out.status.success() {
                    return Err(aws_failure(&out.stderr, &out.stdout, out.status));
                }
                if call.output == Output::Checked {
                    // The answer is not read beyond the exit code (operator.md, The AWS side).
                    return Ok(Value::Null);
                }
                let text = String::from_utf8_lossy(&out.stdout);
                let text = text.trim_start_matches('\u{feff}').trim();
                if text.is_empty() {
                    return Ok(Value::Null);
                }
                serde_json::from_str(text).map_err(|_| {
                    AwsError::Failed(format!(
                        "Unexpected answer from {} {}: not JSON.",
                        call.service, call.operation
                    ))
                })
            }
            Output::PassThrough => {
                let mut child = cmd
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|_| cannot_run())?;
                // Both streams pass through as written, and are kept for the error text.
                let out = tee(child.stdout.take(), std::io::stdout);
                let err = tee(child.stderr.take(), std::io::stderr);
                let status = child.wait().map_err(|_| cannot_run())?;
                let (out, err) = (out.join(), err.join());
                if status.success() {
                    return Ok(Value::Null);
                }
                Err(aws_failure(&err, &out, status))
            }
            Output::Ignore => {
                // The exit code is not checked: the call after it is the check. The stdout is
                // never passed; the stderr only when the runner was built to pass it.
                let stderr = if self.pass_ignored_stderr {
                    Stdio::inherit()
                } else {
                    Stdio::null()
                };
                cmd.stdout(Stdio::null())
                    .stderr(stderr)
                    .status()
                    .map_err(|_| cannot_run())?;
                Ok(Value::Null)
            }
        }
    }
}

/// A thread that copies one of a child's streams to the operator's own, as it is written, and
/// keeps a copy.
struct Tee(Option<std::thread::JoinHandle<Vec<u8>>>);

impl Tee {
    fn join(self) -> Vec<u8> {
        self.0.and_then(|h| h.join().ok()).unwrap_or_default()
    }
}

fn tee<R, W>(stream: Option<R>, sink: fn() -> W) -> Tee
where
    R: Read + Send + 'static,
    W: Write + 'static,
{
    Tee(stream.map(|mut stream| {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                kept.extend_from_slice(&buf[..n]);
                let mut out = sink();
                let _ = out.write_all(&buf[..n]);
                let _ = out.flush();
            }
            kept
        })
    }))
}

/// Gap FA-15's rule: no `%`, no double quote, no control character, no trailing backslash.
fn check_remote_command(command: &str) -> Result<(), Failure> {
    let reason = if command.contains('%') {
        "it holds a percent sign"
    } else if command.contains('"') {
        "it holds a double quote"
    } else if command.chars().any(char::is_control) {
        "it holds a control character"
    } else if command.ends_with('\\') {
        "it ends in a backslash"
    } else {
        return Ok(());
    };
    Err(Failure {
        message: format!("agentspace: The remote command cannot be sent: {reason}."),
        caller: CallerFailure::CouldNotReach,
    })
}

impl Remote for Runner {
    fn ssh(&self, options: &[&str], remote_command: Option<&str>) -> Result<i32, Failure> {
        if let Some(remote) = remote_command {
            check_remote_command(remote)?;
        }
        let program = program_or_default(&self.ctx, &*self.lookup, SSH_VAR, "ssh.exe")?;
        // The remote command is one argument, the last. On Windows `Command` quotes each argument
        // to the C runtime's rules in full (quotes around one with a space or a tab, a double
        // quote escaped, and the backslashes before one doubled), and for a `.cmd` stand-in to
        // `cmd.exe`'s; a command that passed the check above holds nothing either quoting cannot
        // give back whole.
        let args: Vec<&OsStr> = options
            .iter()
            .copied()
            .chain(remote_command)
            .map(OsStr::new)
            .collect();
        self.run_remote(&program, &args)
    }

    fn scp(&self, args: &[&str]) -> Result<i32, Failure> {
        let program = program_or_default(&self.ctx, &*self.lookup, SCP_VAR, "scp.exe")?;
        let args: Vec<&OsStr> = args.iter().copied().map(OsStr::new).collect();
        self.run_remote(&program, &args)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;

    // A script written in one thread can be "text file busy" when another thread forks while its
    // file is open for writing, so the tests that start programs run one at a time.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A throwaway folder under the temp folder, removed when it drops.
    struct Tmp(tempfile::TempDir);

    impl std::ops::Deref for Tmp {
        type Target = Path;
        fn deref(&self) -> &Path {
            self.0.path()
        }
    }

    fn temp_dir() -> Tmp {
        Tmp(tempfile::Builder::new()
            .prefix("agentspace-run-unit-")
            .tempdir()
            .unwrap())
    }

    fn ctx(dir: &Path) -> Context {
        Context {
            home: dir.join("home"),
            local_app_data: None,
            system_root: dir.join("root"),
            nested: false,
        }
    }

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn runner(dir: &Path, vars: &[(&str, &Path)]) -> Runner {
        let map: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.as_os_str().to_os_string()))
            .collect();
        Runner::with_lookup(ctx(dir), move |name| map.get(name).cloned())
    }

    #[test]
    fn aws_arguments_follow_fb07_order() {
        let _serial = serial();
        let dir = temp_dir();
        let aws = script(
            &dir,
            "aws",
            "printf '%s\\n' \"$@\" > \"$0.args\"; echo '{\"a\":1}'",
        );
        let r = runner(&dir, &[(AWS_VAR, &aws)]);
        let call = Call {
            service: "ec2",
            operation: "describe-instances",
            args: &["--instance-ids", "i-0123456789abcdef0"],
            output: Output::Json,
        };
        let v = r.call(&call).unwrap();
        assert_eq!(v["a"], 1);
        let got = std::fs::read_to_string(dir.join("aws.args")).unwrap();
        let want = "ec2\ndescribe-instances\n--region\nus-west-2\n--output\njson\n--instance-ids\ni-0123456789abcdef0\n";
        assert_eq!(got, want);
    }

    #[test]
    fn sign_in_phrases_in_any_case() {
        for phrase in [
            "Your Token Has EXPIRED",
            "Run AWS LOGIN",
            "unable to locate CREDENTIALS",
        ] {
            assert!(is_sign_in_failure(phrase), "{phrase}");
        }
        assert!(!is_sign_in_failure("Access denied"));
    }

    #[test]
    fn other_failure_gives_trimmed_text() {
        let _serial = serial();
        let dir = temp_dir();
        let aws = script(&dir, "aws", "echo '  boom  ' >&2; exit 254");
        let r = runner(&dir, &[(AWS_VAR, &aws)]);
        let call = Call {
            service: "ec2",
            operation: "x",
            args: &[],
            output: Output::Json,
        };
        assert_eq!(r.call(&call), Err(AwsError::Failed("boom".into())));
    }

    #[test]
    fn sign_in_failure_and_ignore_unchecked_exit() {
        let _serial = serial();
        let dir = temp_dir();
        let aws = script(
            &dir,
            "aws",
            "echo 'The SSO session has expired' >&2; exit 255",
        );
        let r = runner(&dir, &[(AWS_VAR, &aws)]);
        let mut call = Call {
            service: "ec2",
            operation: "x",
            args: &[],
            output: Output::Json,
        };
        assert_eq!(r.call(&call), Err(AwsError::SignIn));
        call.output = Output::PassThrough;
        assert_eq!(r.call(&call), Err(AwsError::SignIn));
        call.output = Output::Ignore;
        assert_eq!(r.call(&call), Ok(Value::Null));
    }

    #[test]
    fn seam_names_nothing_runnable_never_falls_back() {
        let _serial = serial();
        let dir = temp_dir();
        let missing = dir.join("missing");
        let r = runner(
            &dir,
            &[(AWS_VAR, &missing), (SSH_VAR, &dir), (SCP_VAR, &missing)],
        );
        let call = Call {
            service: "ec2",
            operation: "x",
            args: &[],
            output: Output::Json,
        };
        let want = format!(
            "agentspace: {AWS_VAR} names {}, which cannot be run.",
            missing.display()
        );
        assert_eq!(r.call(&call), Err(AwsError::CannotRun(want)));
        let f = r.ssh(&[], None).unwrap_err();
        assert_eq!(
            f.message,
            format!(
                "agentspace: {SSH_VAR} names {}, which cannot be run.",
                dir.display()
            )
        );
        assert!(r.scp(&[]).is_err());
        let plain = dir.join("plain");
        std::fs::write(&plain, "x").unwrap();
        assert!(runnable(SSH_VAR, plain.as_os_str()).is_err());
    }

    #[test]
    fn blank_seam_means_default() {
        let _serial = serial();
        let dir = temp_dir();
        let blank = Runner::with_lookup(ctx(&dir), |_| Some(OsString::from("  \t ")));
        let want = dir.join("root/System32/OpenSSH/ssh.exe");
        std::fs::create_dir_all(want.parent().unwrap()).unwrap();
        std::fs::write(&want, "x").unwrap();
        assert_eq!(blank.ssh_program().unwrap(), want);
        assert_eq!(ssh_program_with(&ctx(&dir), &|_| None).unwrap(), want);
    }

    #[test]
    fn path_with_spaces_taken_whole() {
        let _serial = serial();
        let tmp = temp_dir();
        let dir = tmp.join("a folder");
        std::fs::create_dir_all(&dir).unwrap();
        let ssh = script(&dir, "my ssh", "exit 7");
        let r = runner(&dir, &[(SSH_VAR, &ssh)]);
        assert_eq!(r.ssh(&["-o", "X=y"], Some("echo hi")).unwrap(), 7);
    }

    #[test]
    fn remote_command_is_one_last_argument() {
        let _serial = serial();
        let dir = temp_dir();
        let ssh = script(
            &dir,
            "ssh",
            "printf '%s\\n' \"$#\" \"$@\" > \"$0.args\"; exit 0",
        );
        let r = runner(&dir, &[(SSH_VAR, &ssh)]);
        let code = r
            .ssh(&["-o", "BatchMode=yes", "devbox"], Some("cat a & echo b"))
            .unwrap();
        assert_eq!(code, 0);
        let got = std::fs::read_to_string(dir.join("ssh.args")).unwrap();
        assert_eq!(got, "4\n-o\nBatchMode=yes\ndevbox\ncat a & echo b\n");
    }

    #[test]
    fn remote_command_refusals() {
        let _serial = serial();
        let dir = temp_dir();
        let ssh = script(&dir, "ssh", "exit 0");
        let r = runner(&dir, &[(SSH_VAR, &ssh)]);
        for bad in ["a%b", "a\"b", "a\nb", "a\tb", "a\u{7f}b", "ends\\"] {
            assert!(r.ssh(&[], Some(bad)).is_err(), "{bad:?}");
        }
        assert_eq!(r.ssh(&[], Some("a & b | c < d > e ^ f ! g")).unwrap(), 0);
    }

    #[test]
    fn stdin_is_closed_and_environment_is_set() {
        let _serial = serial();
        let dir = temp_dir();
        let ssh = script(
            &dir,
            "ssh",
            "if read x; then echo read > \"$0.out\"; else echo eof > \"$0.out\"; fi; \
             echo \"$AGENTSPACE_NESTED\" >> \"$0.out\"",
        );
        let r = runner(&dir, &[(SSH_VAR, &ssh)]);
        r.ssh(&[], None).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("ssh.out")).unwrap(),
            "eof\n1\n"
        );
    }

    #[test]
    fn posix_fd_state_variables_are_removed() {
        let names = ["PATH", "c28f_POSIX_FD_STATE", "X_POSIX_FD_STATEX"].map(OsString::from);
        let mut cmd = Command::new("x");
        prepare_with_env(&mut cmd, names.into_iter());
        let envs: HashMap<_, _> = cmd.get_envs().collect();
        assert_eq!(envs[OsStr::new("c28f_POSIX_FD_STATE")], None);
        assert!(!envs.contains_key(OsStr::new("PATH")));
        assert!(!envs.contains_key(OsStr::new("X_POSIX_FD_STATEX")));
        assert_eq!(envs[OsStr::new("AGENTSPACE_NESTED")], Some(OsStr::new("1")));
    }
}

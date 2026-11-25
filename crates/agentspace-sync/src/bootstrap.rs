//! `bootstrap` (docs/operator.md, bootstrap; D46, D49).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The script is the repository's `box\bootstrap.sh`, embedded byte for byte**, and MUST be sent
//!   to the host on an ssh's stdin exactly as it is carried, and never written to a file on the
//!   laptop.
//! - **The ssh gets a pipe for its stdin**, to which the operator writes the script's bytes and then
//!   closes it, so `bootstrap` reads nothing from a person. Its stdout and stderr are the
//!   operator's, so the script's output passes through. There is no time limit of the operator's
//!   own: the ssh is waited for until it exits.
//! - **The ssh is `ssh -o ClearAllForwardings=yes <alias> <remote command>`**, started as
//!   [`agentspace_run::ssh_found`] finds the program (so `AGENTSPACE_SSH_CMD` stands in for it) and
//!   with the environment `agentspace-run` gives every program. The pinned `Remote` has no stdin, so
//!   this ssh is started here.
//! - **A start that fails MUST answer with the line the finder gave** (#0227): `agentspace:
//!   AGENTSPACE_SSH_CMD names <value>, which cannot be run.` for an ssh the seam variable names,
//!   since a file the system cannot start is one that names nothing runnable (operator.md, The test
//!   seam), and `agentspace: Could not run <path>.` for the default (Programs it starts).
//! - **The remote command is the operator's own words** (D34), one argument, the last, and holds no
//!   `%`, no double quote and no control character and does not end in a backslash (gap FA-15). It
//!   runs the script it reads from its stdin with bash as the login user, and exits with the
//!   script's status.
//! - **A non-zero exit, 255 for an ssh that could not connect included, fails** with `agentspace:
//!   The toolchain bootstrap failed on the instance.`

use std::io::Write;
use std::process::{Command, Stdio};

use agentspace_box::start_and_wait;
use agentspace_ca::CaEnv;
use agentspace_hosts::Target;
use agentspace_operator_api::{Aws, CallerFailure, Context, Failure, Remote, Say};
use agentspace_run::Program;

use crate::files::Places;
use crate::syncing::plan;
use crate::{Seams, embedded, host, steps};

/// The remote command: bash reads the script from stdin, to its end, and exits with its status.
pub(crate) const REMOTE_COMMAND: &str = "bash -s";

const FAILED: &str = "agentspace: The toolchain bootstrap failed on the instance.";

fn failure(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::CouldNotReach,
    }
}

/// [`crate::bootstrap`] over [`Seams`].
pub(crate) fn run(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    // The pinned `Remote` has no stdin, so the bootstrap's own ssh is started below.
    let _ = remote;
    let places = Places::new(ctx);
    // The host is looked up once, as the command starts, before any program starts.
    let plan = plan(&places, target)?;

    // Step 1: start the host and wait for it.
    let instance_id = start_and_wait(aws, say, seams.clock, &plan.cache, &plan.host.stack)
        .map_err(|failure| plan.hint(failure))?;

    // Step 2: for a host that is not the box, the CA check, then its key, certificate and alias.
    if !plan.is_box {
        (seams.ca_check)(ctx, ca_env)?;
        host::prepare(ctx, &places, &plan.host.project, seams)?;
    }

    // Step 3: the ssh, with the script on its stdin.
    say.progress("Running the toolchain bootstrap on the instance...", None);
    let code = ssh_with_stdin(
        || agentspace_run::ssh_found(ctx),
        &plan.alias,
        REMOTE_COMMAND,
        embedded::BOOTSTRAP,
    )?;

    // Step 4: any non-zero exit fails.
    if code != 0 {
        return Err(failure(FAILED.to_string()));
    }

    // Step 5.
    say.progress(&format!("Bootstrapped: {instance_id}"), None);
    Ok(())
}

/// One ssh with forwards cleared, `alias`, and `remote_command` as its last argument, whose stdin is
/// a pipe that `stdin` is written to and closed; returns its exit code. `find` is the finder, which
/// the command passes as [`agentspace_run::ssh_found`] over the real environment and a test over a
/// lookup of its own.
fn ssh_with_stdin(
    find: impl FnOnce() -> Result<Program, Failure>,
    alias: &str,
    remote_command: &str,
    stdin: &'static [u8],
) -> Result<i32, Failure> {
    let program = find()?;
    run_ssh(&program, alias, remote_command, stdin)
}

/// [`ssh_with_stdin`] for a program that is already found. A start that fails gives the line the
/// finder gave with the program (#0227).
fn run_ssh(
    program: &Program,
    alias: &str,
    remote_command: &str,
    stdin: &'static [u8],
) -> Result<i32, Failure> {
    let mut command = Command::new(&program.path);
    command
        .args(["-o", steps::CLEAR_FORWARDS, alias, remote_command])
        .stdin(Stdio::piped());
    agentspace_run::prepare_child_env(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| failure(program.cannot_start.clone()))?;
    // Written from a thread of its own, so that a script larger than the pipe never waits on an ssh
    // that is waiting for the operator. A write that fails because ssh has already exited is not
    // an error here: the exit code reports it.
    let writer = child.stdin.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let _ = pipe.write_all(stdin);
            // Dropping the pipe closes it.
        })
    });
    let status = child.wait();
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    match status {
        Ok(status) => Ok(status.code().unwrap_or(-1)),
        Err(_) => Err(failure(FAILED.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{Scratch, ctx_in, raw_stand_in};
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    const SSH_VAR: &str = "AGENTSPACE_SSH_CMD";

    /// Bootstrap's step 3 as the command runs it, with the ssh found by `agentspace-run`'s finder
    /// over a lookup in which `AGENTSPACE_SSH_CMD` is `seam` (unset for `None`) and nothing else is
    /// set, in place of the real environment.
    fn bootstrap_ssh(ctx: &Context, seam: Option<&OsStr>) -> Result<i32, Failure> {
        let seam = seam.map(OsStr::to_os_string);
        let lookup = move |name: &str| if name == SSH_VAR { seam.clone() } else { None };
        ssh_with_stdin(
            || agentspace_run::ssh_found_with(ctx, &lookup),
            "agentspace-host.demo",
            REMOTE_COMMAND,
            embedded::BOOTSTRAP,
        )
    }

    /// The seam's line (operator.md, The test seam), for a value given as `value`.
    fn seam_line(value: &Path) -> String {
        format!(
            "agentspace: {SSH_VAR} names {}, which cannot be run.",
            value.display()
        )
    }

    /// A file named `ssh.exe` in `dir` that holds no PE image, which Windows takes for a 16-bit
    /// program and cannot start. The process's error mode is made quiet first (#0225), so that its
    /// start fails at once under any runner and never waits at a modal dialog.
    fn not_a_program(dir: &Path) -> PathBuf {
        agentspace_launch::quiet_hard_errors();
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("ssh.exe");
        std::fs::write(&path, b"not a program").unwrap();
        path
    }

    /// The default ssh's path under the context's `SystemRoot` (operator.md, Programs it starts).
    fn default_ssh(ctx: &Context) -> PathBuf {
        ctx.system_root
            .join("System32")
            .join("OpenSSH")
            .join("ssh.exe")
    }

    #[test]
    fn the_remote_command_keeps_fa15s_rule() {
        let command = REMOTE_COMMAND;
        assert!(!command.contains('%'));
        assert!(!command.contains('"'));
        assert!(!command.chars().any(char::is_control));
        assert!(!command.ends_with('\\'));
    }

    #[cfg(windows)]
    #[test]
    fn the_ssh_gets_its_arguments_and_the_script_on_its_stdin_and_its_exit_code_comes_back() {
        let dir = Scratch::new("bootstrap-ssh");
        // Records its arguments, then copies its stdin to a file, then exits with 7.
        let program = raw_stand_in(
            dir.path(),
            "ssh",
            "@echo off\r\necho %1 %2 %3 %4 > \"%~dp0args.txt\"\r\nfindstr \"^\" > \"%~dp0stdin.txt\"\r\nexit /b 7\r\n",
        );
        let code = bootstrap_ssh(&ctx_in(dir.path()), Some(program.as_os_str())).unwrap();
        assert_eq!(code, 7);
        let args = std::fs::read_to_string(dir.path().join("args.txt")).unwrap();
        // Quotes aside, which a `.cmd` stand-in's quoting adds.
        assert_eq!(
            args.trim().replace('"', ""),
            "-o ClearAllForwardings=yes agentspace-host.demo bash -s"
        );
        let stdin = std::fs::read_to_string(dir.path().join("stdin.txt")).unwrap();
        let sent: Vec<&str> = stdin.lines().collect();
        let script = std::str::from_utf8(embedded::BOOTSTRAP).unwrap();
        let want: Vec<&str> = script.lines().collect();
        assert_eq!(sent, want);
    }

    /// #0227: a seam value that names a file that does not exist answers with the seam's line.
    #[test]
    fn a_seam_naming_a_missing_file_fails_with_the_seams_line() {
        let dir = Scratch::new("bootstrap-seam-missing");
        let missing = dir.path().join("nowhere").join("ssh.exe");
        let failure = bootstrap_ssh(&ctx_in(dir.path()), Some(missing.as_os_str())).unwrap_err();
        assert_eq!(failure.message, seam_line(&missing));
        assert!(matches!(failure.caller, CallerFailure::CouldNotReach));
    }

    /// #0227: a seam value that names a folder answers with the seam's line.
    #[test]
    fn a_seam_naming_a_folder_fails_with_the_seams_line() {
        let dir = Scratch::new("bootstrap-seam-folder");
        let folder = dir.path().join("ssh.exe");
        std::fs::create_dir_all(&folder).unwrap();
        let failure = bootstrap_ssh(&ctx_in(dir.path()), Some(folder.as_os_str())).unwrap_err();
        assert_eq!(failure.message, seam_line(&folder));
        assert!(matches!(failure.caller, CallerFailure::CouldNotReach));
    }

    /// #0227: a seam value that names a file the system cannot start (a file named `ssh.exe` that
    /// holds no program) names nothing runnable, and answers with the seam's line, never the
    /// default's "Could not run" (operator.md, The test seam).
    #[test]
    fn a_seam_naming_a_file_that_is_not_a_program_fails_with_the_seams_line() {
        let dir = Scratch::new("bootstrap-seam-not-a-program");
        let ctx = ctx_in(dir.path());
        let file = not_a_program(&dir.path().join("bin"));
        let failure = bootstrap_ssh(&ctx, Some(file.as_os_str())).unwrap_err();
        assert_eq!(failure.message, seam_line(&file));
        assert!(matches!(failure.caller, CallerFailure::CouldNotReach));
    }

    /// The default ssh, which exists but cannot be started, keeps the default's line (operator.md,
    /// Programs it starts, "When a default program cannot be run").
    #[test]
    fn a_default_ssh_that_is_not_a_program_fails_with_could_not_run() {
        let dir = Scratch::new("bootstrap-default-not-a-program");
        let ctx = ctx_in(dir.path());
        let path = default_ssh(&ctx);
        assert_eq!(not_a_program(path.parent().unwrap()), path);
        let failure = bootstrap_ssh(&ctx, None).unwrap_err();
        assert_eq!(
            failure.message,
            format!("agentspace: Could not run {}.", path.display())
        );
        assert!(matches!(failure.caller, CallerFailure::CouldNotReach));
    }

    /// The default ssh, found and then gone before its start, keeps the default's line too.
    #[test]
    fn a_default_ssh_gone_before_its_start_fails_with_could_not_run() {
        let dir = Scratch::new("bootstrap-default-gone");
        let ctx = ctx_in(dir.path());
        let path = default_ssh(&ctx);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();
        let found = agentspace_run::ssh_found_with(&ctx, &|_| None).unwrap();
        std::fs::remove_file(&path).unwrap();
        let failure = ssh_with_stdin(
            || Ok(found),
            "agentspace",
            REMOTE_COMMAND,
            embedded::BOOTSTRAP,
        )
        .unwrap_err();
        assert_eq!(
            failure.message,
            format!("agentspace: Could not run {}.", path.display())
        );
    }

    #[test]
    fn the_script_is_carried_whole() {
        assert!(embedded::BOOTSTRAP.starts_with(b"#!/bin/bash\n"));
        assert!(!embedded::BOOTSTRAP.contains(&b'\r'));
    }
}

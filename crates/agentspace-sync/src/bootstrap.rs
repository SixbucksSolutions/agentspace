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
//!   [`agentspace_run::ssh_program`] finds the program (so `AGENTSPACE_SSH_CMD` stands in for it) and
//!   with the environment `agentspace-run` gives every program. The pinned `Remote` has no stdin, so
//!   this ssh is started here.
//! - **The remote command is the operator's own words** (D34), one argument, the last, and holds no
//!   `%`, no double quote and no control character and does not end in a backslash (gap FA-15). It
//!   runs the script it reads from its stdin with bash as the login user, and exits with the
//!   script's status.
//! - **A non-zero exit, 255 for an ssh that could not connect included, fails** with `agentspace:
//!   The toolchain bootstrap failed on the instance.`

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use agentspace_box::start_and_wait;
use agentspace_ca::CaEnv;
use agentspace_hosts::Target;
use agentspace_operator_api::{Aws, CallerFailure, Context, Failure, Remote, Say};

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
    let code = ssh_with_stdin(ctx, &plan.alias, REMOTE_COMMAND, embedded::BOOTSTRAP)?;

    // Step 4: any non-zero exit fails.
    if code != 0 {
        return Err(failure(FAILED.to_string()));
    }

    // Step 5.
    say.progress(&format!("Bootstrapped: {instance_id}"), None);
    Ok(())
}

/// One ssh with forwards cleared, `alias`, and `remote_command` as its last argument, whose stdin is
/// a pipe that `stdin` is written to and closed; returns its exit code.
fn ssh_with_stdin(
    ctx: &Context,
    alias: &str,
    remote_command: &str,
    stdin: &'static [u8],
) -> Result<i32, Failure> {
    let program = agentspace_run::ssh_program(ctx)?;
    run_ssh(&program, alias, remote_command, stdin)
}

/// [`ssh_with_stdin`] for a program that is already found.
fn run_ssh(
    program: &Path,
    alias: &str,
    remote_command: &str,
    stdin: &'static [u8],
) -> Result<i32, Failure> {
    let mut command = Command::new(program);
    command
        .args(["-o", steps::CLEAR_FORWARDS, alias, remote_command])
        .stdin(Stdio::piped());
    agentspace_run::prepare_child_env(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| failure(format!("agentspace: Could not run {}.", program.display())))?;
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
        use crate::tests::{Scratch, raw_stand_in};
        let dir = Scratch::new("bootstrap-ssh");
        // Records its arguments, then copies its stdin to a file, then exits with 7.
        let program = raw_stand_in(
            dir.path(),
            "ssh",
            "@echo off\r\necho %1 %2 %3 %4 > \"%~dp0args.txt\"\r\nfindstr \"^\" > \"%~dp0stdin.txt\"\r\nexit /b 7\r\n",
        );
        let code = run_ssh(
            &program,
            "agentspace-host.demo",
            REMOTE_COMMAND,
            embedded::BOOTSTRAP,
        )
        .unwrap();
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

    #[test]
    fn a_program_that_cannot_be_started_fails_with_its_own_message() {
        let dir = std::env::temp_dir().join("agentspace-sync-no-such-ssh.exe");
        let failure = run_ssh(&dir, "agentspace", REMOTE_COMMAND, embedded::BOOTSTRAP).unwrap_err();
        assert_eq!(
            failure.message,
            format!("agentspace: Could not run {}.", dir.display())
        );
    }

    #[test]
    fn the_script_is_carried_whole() {
        assert!(embedded::BOOTSTRAP.starts_with(b"#!/bin/bash\n"));
        assert!(!embedded::BOOTSTRAP.contains(&b'\r'));
    }
}

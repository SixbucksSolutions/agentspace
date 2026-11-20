//! The tunnel (docs/operator.md, The ProxyCommand, step 6; What a caller sees, The tunnel).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The tunnel is the AWS CLI's `ssm start-session`, started with the ProxyCommand's own standard
//! input and output. It is the AWS CLI at its fixed path alone, with no fallback to `aws` on
//! `PATH` (B-33), or the one program `AGENTSPACE_AWS_CMD` names, taken whole. Its standard error
//! is withheld from the caller unless `AGENTSPACE_DIAGNOSTIC` is set.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use agentspace_operator_api::{CallerFailure, Failure};

/// The tunnel's AWS CLI, at its fixed path (operator.md, The ProxyCommand, step 6).
pub const AWS_FIXED_PATH: &str = r"C:\Program Files\Amazon\AWSCLIV2\aws.exe";
/// The seam variable that names the AWS CLI (operator.md, The test seam).
pub const AWS_VAR: &str = "AGENTSPACE_AWS_CMD";
/// What a tunnel that exits non-zero gets (operator.md, What a caller sees, The tunnel).
pub const CONNECTION_FAILED: &str = "agentspace: The connection to the host failed. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.";

/// The program that runs the tunnel, and the line its failing to start gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tunnel {
    pub program: PathBuf,
    cannot_start: String,
}

fn could_not_reach(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::CouldNotReach,
    }
}

/// Whether `path` is a file the system can run: a file on Windows, one with an execute bit
/// elsewhere.
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

/// Finds the tunnel's program. `seam` is the value of `AGENTSPACE_AWS_CMD`, read once as the
/// program started.
///
/// - **Unset, empty or only whitespace** is the AWS CLI's fixed path. When that file is missing
///   the tunnel fails, with no fallback.
/// - **Any other value is one path, taken whole**, resolved against the working folder and never
///   looked up on `PATH`. A value that names nothing runnable fails, and MUST NOT fall back to
///   the default.
///
/// Every failure is the "Could not reach the host" line to a caller.
pub fn resolve(seam: Option<&OsStr>) -> Result<Tunnel, Failure> {
    match seam.filter(|v| !v.to_string_lossy().trim().is_empty()) {
        Some(value) => {
            let message = format!(
                "agentspace: {AWS_VAR} names {}, which cannot be run.",
                value.to_string_lossy()
            );
            let path = std::path::absolute(Path::new(value))
                .map_err(|_| could_not_reach(message.clone()))?;
            if !is_runnable_file(&path) {
                return Err(could_not_reach(message));
            }
            Ok(Tunnel {
                program: path,
                cannot_start: message,
            })
        }
        None => {
            let path = PathBuf::from(AWS_FIXED_PATH);
            if !path.is_file() {
                return Err(could_not_reach(format!(
                    "agentspace: {AWS_FIXED_PATH} is missing."
                )));
            }
            Ok(Tunnel {
                cannot_start: format!("agentspace: Could not run {AWS_FIXED_PATH}."),
                program: path,
            })
        }
    }
}

/// The tunnel's arguments, in the order operator.md gives them.
pub fn arguments(instance_id: &str) -> Vec<String> {
    [
        "ssm",
        "start-session",
        "--region",
        "us-west-2",
        "--target",
        instance_id,
        "--document-name",
        "AWS-StartSSHSession",
        "--parameters",
        "portNumber=22",
    ]
    .map(String::from)
    .to_vec()
}

/// Whether an argument holds only letters, digits, `-`, `.` and `=`, which nothing needs quoted.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_plain(argument: &str) -> bool {
    !argument.is_empty()
        && argument
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'='))
}

/// Adds the tunnel's arguments exactly as they are written. On Windows, `Command` quotes an
/// argument that holds `=` when the program is a `.cmd` stand-in, so that the stand-in would see
/// `"portNumber=22"` where the real AWS CLI sees `portNumber=22`. A plain argument is passed raw,
/// so a stand-in is started exactly as the real program would be (operator.md, The test seam);
/// any other argument is passed with the usual quoting.
fn add_arguments(cmd: &mut Command, arguments: Vec<String>) {
    for argument in arguments {
        #[cfg(windows)]
        if is_plain(&argument) {
            use std::os::windows::process::CommandExt;
            cmd.raw_arg(&argument);
            continue;
        }
        cmd.arg(argument);
    }
}

/// Gives `cmd` the tunnel's arguments, `PATH` and standard streams.
///
/// - `PATH` is `path`, the two folders first (step 2), set on the command itself so that the
///   tunnel's AWS CLI finds Session Manager's plugin whatever the system.
/// - The environment then goes through `agentspace-run`'s function: every `*_POSIX_FD_STATE`
///   variable removed and `AGENTSPACE_NESTED` set to `1`.
/// - Standard input and output are the ProxyCommand's own. Standard error is the ProxyCommand's
///   only when `diagnostic`, and is otherwise discarded: what the AWS CLI and Session Manager's
///   plugin write there is backend text.
pub fn prepare(cmd: &mut Command, instance_id: &str, path: &OsStr, diagnostic: bool) {
    add_arguments(cmd, arguments(instance_id));
    cmd.env("PATH", path);
    agentspace_run::prepare_child_env(cmd);
    cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit());
    cmd.stderr(if diagnostic {
        Stdio::inherit()
    } else {
        Stdio::null()
    });
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
    1
}

/// Runs the tunnel to its end and returns its exit code. A tunnel that cannot be started is the
/// "Could not reach the host" failure.
pub fn run(
    tunnel: &Tunnel,
    instance_id: &str,
    path: &OsString,
    diagnostic: bool,
) -> Result<i32, Failure> {
    let mut cmd = Command::new(&tunnel.program);
    prepare(&mut cmd, instance_id, path, diagnostic);
    match cmd.status() {
        Ok(status) => Ok(exit_code(status)),
        Err(_) => Err(could_not_reach(tunnel.cannot_start.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arguments_are_operator_mds_line() {
        assert_eq!(
            arguments("i-1234567890abcdef0").join(" "),
            "ssm start-session --region us-west-2 --target i-1234567890abcdef0 --document-name AWS-StartSSHSession --parameters portNumber=22"
        );
    }

    #[test]
    fn a_seam_value_that_names_nothing_runnable_never_falls_back() {
        let missing = std::env::temp_dir().join("agentspace-ssh-proxy-no-such-aws");
        let failure = resolve(Some(missing.as_os_str())).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {AWS_VAR} names {}, which cannot be run.",
                missing.display()
            )
        );
        // A folder is not a program either.
        let folder = std::env::temp_dir();
        assert!(resolve(Some(folder.as_os_str())).is_err());
    }

    #[test]
    fn a_blank_seam_value_means_the_fixed_path() {
        for blank in ["", "   ", "\t "] {
            let result = resolve(Some(OsStr::new(blank)));
            match result {
                Ok(tunnel) => assert_eq!(tunnel.program, PathBuf::from(AWS_FIXED_PATH)),
                Err(failure) => assert_eq!(
                    failure.message,
                    format!("agentspace: {AWS_FIXED_PATH} is missing.")
                ),
            }
        }
        let by_default = resolve(None);
        let by_blank = resolve(Some(OsStr::new(" ")));
        assert_eq!(by_default, by_blank);
    }

    #[test]
    fn a_missing_fixed_path_has_no_fallback_to_path() {
        // The fixed file is not on a Linux box, and on a laptop with the AWS CLI the failure
        // cannot be reached, so only the shape of the failure is checked.
        if let Err(failure) = resolve(None) {
            assert!(failure.message.ends_with("is missing."));
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        }
    }

    // A stand-in that writes what it sees to the file named by its first argument's folder.
    fn throwaway(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agentspace-ssh-proxy-tunnel-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(windows)]
    fn stand_in(dir: &Path) -> PathBuf {
        let path = dir.join("tunnel.cmd");
        let out = dir.join("seen.txt");
        let body = format!(
            "@echo off\r\necho [%AGENTSPACE_NESTED%]>\"{out}\"\r\necho [%C28F_POSIX_FD_STATE%]>>\"{out}\"\r\necho %PATH%>>\"{out}\"\r\necho %*>>\"{out}\"\r\nexit /b 5\r\n",
            out = out.display()
        );
        std::fs::write(&path, body).unwrap();
        path
    }

    #[cfg(unix)]
    fn stand_in(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("tunnel.sh");
        let out = dir.join("seen.txt");
        let body = format!(
            "#!/bin/sh\necho \"[$AGENTSPACE_NESTED]\" > '{out}'\necho \"[$C28F_POSIX_FD_STATE]\" >> '{out}'\necho \"$PATH\" >> '{out}'\necho \"$@\" >> '{out}'\nexit 5\n",
            out = out.display()
        );
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// A command for the stand-in, with the throwaway home every started program gets in a test,
    /// prepared as the tunnel is.
    fn prepared(program: &Path, home: &Path, diagnostic: bool) -> Command {
        let mut cmd = Command::new(program);
        for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
            cmd.env(name, home);
        }
        cmd.current_dir(home);
        // A variable set on the command, as an inherited one would be, MUST be removed.
        cmd.env("C28F_POSIX_FD_STATE", "leaked");
        prepare(
            &mut cmd,
            "i-1234567890abcdef0",
            OsStr::new("first;second;"),
            diagnostic,
        );
        cmd
    }

    #[test]
    fn a_started_tunnel_sees_nested_set_and_no_posix_fd_state() {
        let dir = throwaway("env");
        let program = stand_in(&dir);
        let tunnel = resolve(Some(program.as_os_str())).unwrap();
        let status = prepared(&tunnel.program, &dir, true).status().unwrap();
        assert_eq!(exit_code(status), 5, "the tunnel's exit code is returned");
        let seen = std::fs::read_to_string(dir.join("seen.txt")).unwrap();
        let lines: Vec<&str> = seen.lines().map(str::trim).collect();
        // Values are compared, never printed (AGENTS.md).
        assert!(lines[0] == "[1]", "AGENTSPACE_NESTED is not 1");
        assert!(lines[1] == "[]", "a *_POSIX_FD_STATE variable was passed");
        assert!(
            lines[2].starts_with("first;second;"),
            "PATH is not the one given"
        );
        assert!(lines[3].contains("--target i-1234567890abcdef0"));
        // A stand-in sees the arguments as the real program would, none of them quoted.
        assert!(
            lines[3].ends_with("--parameters portNumber=22"),
            "{}",
            lines[3]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tunnel_that_cannot_be_started_is_could_not_reach() {
        let dir = throwaway("nostart");
        let tunnel = Tunnel {
            program: dir.join("gone.exe"),
            cannot_start: "agentspace: Could not run the tunnel.".to_string(),
        };
        let failure = run(&tunnel, "i-1234567890abcdef0", &OsString::from("x"), false).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        assert_eq!(failure.message, "agentspace: Could not run the tunnel.");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

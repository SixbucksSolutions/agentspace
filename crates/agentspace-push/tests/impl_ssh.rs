//! The push's ssh, as `SystemSsh` runs it (operator.md, What the laptop sends, steps 2 to 5; The
//! test seam; Environment variables). The implementer's own tests, with a stand-in ssh.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Windows only.
#![cfg(windows)]

mod impl_support;

use agentspace_push::{SSH_LIMIT, STDOUT_WAIT, SshResult, SshRun, SystemSsh, ssh_args};
use impl_support::{
    Bystander, CONTENT, Proc, Sandbox, end_own_process, ended_within, is_running, remote_command,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SECOND: Duration = Duration::from_secs(1);

fn ssh(sb: &Sandbox) -> SystemSsh {
    SystemSsh::with_lookup(&sb.context(), sb.lookup(None))
        .with_command_setup(sb.setup())
        .with_limits(Duration::from_secs(20), Duration::from_secs(10))
}

fn standard_args() -> Vec<String> {
    ssh_args("agentspace", &remote_command("America/New_York"))
}

fn run(ssh: &SystemSsh, args: &[String], stdin: &[u8]) -> SshResult {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    ssh.run(&args, stdin)
}

#[test]
fn the_limits_are_the_specs() {
    assert_eq!(SSH_LIMIT, Duration::from_secs(20));
    assert_eq!(STDOUT_WAIT, Duration::from_secs(10));
}

#[test]
fn the_ssh_gets_exactly_the_arguments_and_the_remote_command_is_one_argument_the_last() {
    let sb = Sandbox::new();
    sb.mode("ok");
    let args = standard_args();
    let result = run(&ssh(&sb), &args, CONTENT.as_bytes());
    assert!(matches!(result, SshResult::Exited { code: 0, .. }));
    let seen = sb.args(1);
    assert_eq!(seen.len(), 6, "The ssh got {} arguments", seen.len());
    assert_eq!(
        seen[..5],
        [
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "BatchMode=yes",
            "agentspace"
        ]
    );
    // The last argument is the whole remote command, with its spaces, quotes of its own,
    // semicolons, pipes and angle brackets.
    assert_eq!(seen[5], args[5]);
    assert!(seen[5].contains("timedatectl set-timezone America/New_York"));
}

#[test]
fn the_stdin_is_exactly_the_content_and_is_closed() {
    let sb = Sandbox::new();
    sb.mode("ok");
    let result = run(&ssh(&sb), &standard_args(), CONTENT.as_bytes());
    // The stand-in reads to the end of its stdin, so it saw the end: stdin was closed. Its answer
    // has the hash of exactly these bytes, and the file holds them with no byte order mark.
    assert!(matches!(result, SshResult::Exited { code: 0, .. }));
    let got = sb.stdin(1);
    assert_eq!(got, CONTENT.as_bytes());
    assert!(!got.starts_with(&[0xEF, 0xBB, 0xBF]));
}

#[test]
fn an_exit_code_and_a_stdout_are_given_back_as_they_were() {
    let sb = Sandbox::new();
    sb.mode("noise");
    let SshResult::Exited { code, stdout } = run(&ssh(&sb), &standard_args(), CONTENT.as_bytes())
    else {
        panic!("The ssh did not exit on its own");
    };
    assert_eq!(code, 0);
    let text = String::from_utf8(stdout).expect("UTF-8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "Welcome to the dummy box");
    assert_eq!(lines[1].len(), "  -".len() + 64);

    for want in [255, 1, 7] {
        sb.mode(&format!("exit {want}"));
        let result = run(&ssh(&sb), &standard_args(), CONTENT.as_bytes());
        assert!(
            matches!(&result, SshResult::Exited { code, stdout } if *code == want && stdout.is_empty()),
            "The code {want} was not given back"
        );
    }
}

#[test]
fn the_ssh_sees_nested_and_no_posix_fd_state_and_the_token_is_in_no_variable() {
    let sb = Sandbox::new();
    sb.mode("ok");
    let setup = sb.setup();
    // A variable of the kind Windows OpenSSH leaves (card #0026), set on the command as an
    // inherited one would be, in two spellings of the name.
    let ssh =
        SystemSsh::with_lookup(&sb.context(), sb.lookup(None)).with_command_setup(move |cmd| {
            setup(cmd);
            cmd.env("0A1B2C3D_POSIX_FD_STATE", "1:2");
            cmd.env("0a1b2c3d_posix_fd_state", "1:2");
            cmd.env("AGENTSPACE_NESTED", "something else");
        });
    let result = run(&ssh, &standard_args(), CONTENT.as_bytes());
    assert!(matches!(result, SshResult::Exited { code: 0, .. }));
    let env = sb.env(1);
    let nested = env.iter().find(|(name, _)| name == "AGENTSPACE_NESTED");
    // A captured environment's values are never printed: a mismatch is reported by name.
    assert!(
        nested.map(|(_, v)| v.as_str()) == Some("1"),
        "AGENTSPACE_NESTED is not 1"
    );
    let leftover = env
        .iter()
        .filter(|(name, _)| name.to_ascii_uppercase().ends_with("_POSIX_FD_STATE"))
        .count();
    assert_eq!(
        leftover, 0,
        "{leftover} POSIX_FD_STATE variables reached the ssh"
    );
    // Secrecy: nothing the ssh sees carries the token or the content.
    let text = sb.env_text(1);
    assert!(
        !text.contains(impl_support::TOKEN),
        "A variable holds the token"
    );
    assert!(
        !text.contains("LOCALSWIM_TOKEN"),
        "A variable holds the content"
    );
    assert!(!sb.args(1).iter().any(|a| a.contains(impl_support::TOKEN)));
}

#[test]
fn the_ssh_runs_in_the_folder_it_was_given_and_the_seam_path_may_hold_spaces() {
    let sb = Sandbox::new();
    assert!(sb.ssh_cmd().to_string_lossy().contains("stand in"));
    sb.mode("ok");
    run(&ssh(&sb), &standard_args(), CONTENT.as_bytes());
    let cwd: PathBuf = sb.cwd(1);
    assert_eq!(
        cwd.canonicalize().ok(),
        sb.work.canonicalize().ok(),
        "The stand-in did not run in its throwaway folder"
    );
}

#[test]
fn a_program_that_cannot_be_run_never_falls_back_to_the_default() {
    let sb = Sandbox::new();
    let args = standard_args();
    // A seam value that names nothing runnable: a missing file, a folder.
    for value in [sb.root.join("no such file.cmd"), sb.standin.clone()] {
        let value = value.into_os_string();
        let lookup: std::sync::Arc<agentspace_push::Lookup> =
            std::sync::Arc::new(move |name: &str| {
                (name == "AGENTSPACE_SSH_CMD").then(|| value.clone())
            });
        let result = run(
            &SystemSsh::with_lookup(&sb.context(), lookup),
            &args,
            CONTENT.as_bytes(),
        );
        assert_eq!(result, SshResult::CouldNotStart);
    }
    // An unset, empty or whitespace seam value means the default, which is `ssh.exe` under the
    // system root's OpenSSH folder: the sandbox's holds none, so nothing starts, and the real ssh
    // is never reached.
    for value in [None, Some(""), Some("   ")] {
        let lookup: std::sync::Arc<agentspace_push::Lookup> =
            std::sync::Arc::new(move |name: &str| {
                (name == "AGENTSPACE_SSH_CMD")
                    .then(|| value.map(std::ffi::OsString::from))
                    .flatten()
            });
        let result = run(
            &SystemSsh::with_lookup(&sb.context(), lookup),
            &args,
            CONTENT.as_bytes(),
        );
        assert_eq!(result, SshResult::CouldNotStart, "{value:?}");
    }
    assert_eq!(sb.calls(), 0);
}

#[test]
fn an_argument_that_would_not_survive_the_command_line_is_never_sent() {
    let sb = Sandbox::new();
    sb.mode("ok");
    for last in [
        "echo 100%",
        "echo \"x\"",
        "echo\r\nx",
        "echo x\\",
        "echo\u{7}x",
    ] {
        let mut args = standard_args();
        *args.last_mut().unwrap() = last.to_string();
        let result = run(&ssh(&sb), &args, CONTENT.as_bytes());
        assert_eq!(result, SshResult::CouldNotStart, "{last:?}");
    }
    assert_eq!(sb.calls(), 0, "The stand-in was started");
}

#[test]
fn a_write_after_the_ssh_has_exited_is_not_an_error() {
    let sb = Sandbox::new();
    sb.mode("noread-exit");
    // Far more than a pipe holds, so the write is still going when the ssh is gone.
    let big = vec![b'x'; 4 << 20];
    let result = run(&ssh(&sb), &standard_args(), &big);
    assert!(matches!(result, SshResult::Exited { code: 0, .. }));
}

#[test]
fn a_write_that_blocks_is_cut_off_by_the_limit() {
    let sb = Sandbox::new();
    sb.mode("noread-hang");
    let big = vec![b'x'; 4 << 20];
    let started = Instant::now();
    let short = ssh(&sb).with_limits(4 * SECOND, 10 * SECOND);
    let result = run(&short, &standard_args(), &big);
    let took = started.elapsed();
    assert_eq!(result, SshResult::TimedOut);
    assert!(took >= 4 * SECOND && took < 12 * SECOND, "It took {took:?}");
    if let Some(pid) = sb.pid("ps", 1) {
        assert!(
            ended_within(pid, 5 * SECOND),
            "The stand-in is still running"
        );
    }
}

#[test]
fn a_stdout_that_does_not_end_is_waited_for_a_while_and_taken_as_empty() {
    let sb = Sandbox::new();
    // The ssh answers and exits at once, and a process it started holds the stdout open.
    sb.mode("lingering");
    let short = ssh(&sb).with_limits(20 * SECOND, 2 * SECOND);
    let started = Instant::now();
    let result = run(&short, &standard_args(), CONTENT.as_bytes());
    let took = started.elapsed();
    let sleeper = sb.pid("grand", 1);
    if let Some(pid) = sleeper {
        end_own_process(pid);
    }
    let SshResult::Exited { code, stdout } = result else {
        panic!("The ssh did not exit on its own");
    };
    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "The stdout was not taken as empty");
    assert!(took >= 2 * SECOND && took < 9 * SECOND, "It took {took:?}");
}

#[test]
fn at_the_limit_the_whole_tree_is_ended_the_ssh_too_and_nothing_else() {
    let sb = Sandbox::new();
    sb.mode("hangtree");
    let bystander = Bystander::start(&sb.work);
    let short = ssh(&sb).with_limits(10 * SECOND, 10 * SECOND);
    let started = Instant::now();
    let result = run(&short, &standard_args(), CONTENT.as_bytes());
    let took = started.elapsed();

    let tree: Vec<(&str, Option<Proc>)> = ["ps", "grand", "parent"]
        .into_iter()
        .map(|kind| (kind, sb.pid(kind, 1)))
        .collect();
    // Whatever happens next, none of the stand-in's own processes is left running by this test.
    let cleanup = || {
        for (_, pid) in &tree {
            if let Some(pid) = pid {
                end_own_process(*pid);
            }
        }
    };
    assert_eq!(
        result,
        SshResult::TimedOut,
        "The ssh was not stopped at its limit"
    );
    assert!(
        took >= 10 * SECOND && took < 15 * SECOND,
        "It took {took:?}"
    );
    for (kind, pid) in &tree {
        let pid = pid.unwrap_or_else(|| {
            cleanup();
            panic!("The stand-in did not record its {kind} process in time")
        });
        let gone = ended_within(pid, 3 * SECOND);
        if !gone {
            cleanup();
        }
        assert!(
            gone,
            "The {kind} process of the ssh's tree is still running"
        );
    }
    // Not any other process: the bystander, a sibling started by this test, is untouched.
    assert!(
        is_running(bystander.id()),
        "A process outside the tree was ended"
    );
    cleanup();
}

#[test]
fn at_the_limit_the_ssh_itself_is_ended_when_it_outlives_its_tree() {
    // The `.cmd` itself spins and starts no process, so only the termination of the ssh's own
    // process ends it: a stand-in whose children are its whole work ends with them, and cannot
    // show that the ssh is terminated last (What the laptop sends, step 4).
    let sb = Sandbox::new();
    sb.mode("roothang");
    let short = ssh(&sb).with_limits(3 * SECOND, 10 * SECOND);
    let result = run(&short, &standard_args(), CONTENT.as_bytes());
    let spin = sb.standin.join("spin.txt");
    let size = || std::fs::metadata(&spin).map(|m| m.len()).unwrap_or(0);
    let ran = size() > 0;
    std::thread::sleep(Duration::from_millis(300));
    let before = size();
    std::thread::sleep(SECOND);
    let after = size();
    // Whatever the outcome, the stand-in stops on its own once this file exists.
    std::fs::write(sb.standin.join("stop.txt"), "stop").expect("The stop file");
    assert_eq!(result, SshResult::TimedOut);
    assert!(ran, "The stand-in did not run");
    assert_eq!(
        after, before,
        "The ssh itself is still running after the limit"
    );
}

#[test]
fn the_real_limit_is_twenty_seconds_from_the_start() {
    let sb = Sandbox::new();
    sb.mode("hang");
    // The operator's own limits, which nothing here shortens.
    let ssh = SystemSsh::with_lookup(&sb.context(), sb.lookup(None)).with_command_setup(sb.setup());
    let started = Instant::now();
    let result = run(&ssh, &standard_args(), CONTENT.as_bytes());
    let took = started.elapsed();
    assert_eq!(result, SshResult::TimedOut);
    assert!(
        took >= Duration::from_secs(20) && took < Duration::from_secs(26),
        "It took {took:?}"
    );
    if let Some(pid) = sb.pid("ps", 1) {
        assert!(
            ended_within(pid, 3 * SECOND),
            "The stand-in is still running"
        );
        end_own_process(pid);
    }
}

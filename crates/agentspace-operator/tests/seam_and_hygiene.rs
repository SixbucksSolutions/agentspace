//! The test seam itself, and what every program the operator starts MUST be given: its
//! environment, its stdin, its path (docs/operator.md: The test seam, Programs it starts,
//! Environment variables, Streams, Every ssh runs the ProxyCommand).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use std::path::PathBuf;

use rig::*;

fn cached_rig() -> Rig {
    let rig = Rig::standard();
    rig.write_cache(ID);
    rig
}

// ---- The rig's own eyes ----

#[test]
fn the_stand_in_sees_a_leaked_stdin_a_leaked_variable_and_a_leaked_secret() {
    // Without this, a test that shows "nothing leaked" could pass because nothing could be seen.
    use std::io::Write;
    use std::process::{Command, Stdio};
    let rig = cached_rig();
    std::fs::write(rig.ctl.join("secret.txt"), "a-dummy-secret-text").unwrap();
    let mut child = Command::new(rig.aws_exe())
        .args(["ec2", "describe-instances"])
        .env_clear()
        .env("STANDIN_DIR", &rig.ctl)
        .env("X_POSIX_FD_STATE", "1")
        .env("AGENTSPACE_NESTED", "1")
        .env("HIDING_PLACE", "prefix a-dummy-secret-text suffix")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the stand-in starts");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(SENTINEL.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
    let calls = rig.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].stdin, SENTINEL.as_bytes());
    assert_eq!(calls[0].fd_state, 1);
    assert_eq!(calls[0].nested, "1");
    assert!(calls[0].secret);
    assert_eq!(calls[0].key, "ec2-describe-instances");
}

// ---- A variable that names nothing runnable ----

#[test]
fn an_aws_variable_that_names_nothing_runnable_fails_the_command_and_never_falls_back() {
    for command in [vec!["up"], vec!["down"], vec!["status"], vec!["grow"]] {
        for kind in [
            "a file that does not exist",
            "a folder",
            "a file the system cannot start",
        ] {
            let rig = cached_rig();
            let value: PathBuf = match kind {
                "a folder" => rig.work.clone(),
                "a file the system cannot start" => {
                    let p = rig.work.join("not-a-program.exe");
                    std::fs::write(&p, "this is not an executable").unwrap();
                    p
                }
                _ => rig.work.join("no-such-aws.exe"),
            };
            let shown = value.to_string_lossy().into_owned();
            let out = rig.op(&command).env("AGENTSPACE_AWS_CMD", &value).run();
            assert_fails_with(
                &out,
                &format!("agentspace: AGENTSPACE_AWS_CMD names {shown}, which cannot be run."),
                &format!("{command:?}, {kind}"),
            );
            assert_eq!(out.err().lines().count(), 1, "{command:?}, {kind}");
            assert!(
                rig.calls().is_empty(),
                "{command:?}, {kind}: no stand-in runs in its place"
            );
        }
    }
}

#[test]
fn the_ssh_variable_naming_nothing_runnable_fails_grow_and_warns_in_up() {
    let rig = cached_rig();
    let value = rig.work.join("no-such-ssh.exe");
    let out = rig.op(&["grow"]).env("AGENTSPACE_SSH_CMD", &value).run();
    assert_fails_with(
        &out,
        &format!(
            "agentspace: AGENTSPACE_SSH_CMD names {}, which cannot be run.",
            value.display()
        ),
        "grow",
    );
    assert!(rig.calls_of("ssh").is_empty());

    // `up` over a missing ssh pushes nothing and still succeeds (up_push.rs has the warning).
    let rig = cached_rig();
    rig.localswim();
    let out = rig
        .op(&["up", "-Quiet"])
        .env("AGENTSPACE_SSH_CMD", &value)
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
}

#[test]
fn a_variable_that_no_command_reaches_is_never_read() {
    // A stopped box: grow starts no ssh, so a broken AGENTSPACE_SSH_CMD and AGENTSPACE_SCP_CMD do
    // not matter; down and status start neither.
    for command in [vec!["down"], vec!["status"], vec!["up"]] {
        let rig = cached_rig();
        rig.instance("stopped");
        let bad = rig.work.join("nothing-here.exe");
        let out = rig
            .op(&command)
            .env("AGENTSPACE_SSH_CMD", &bad)
            .env("AGENTSPACE_SCP_CMD", &bad)
            .run();
        assert_eq!(out.code, Some(0), "{command:?}: {:?}", out.err());
    }
    let rig = cached_rig();
    rig.instance("stopped");
    let bad = rig.work.join("nothing-here.exe");
    let out = rig
        .op(&["grow"])
        .env("AGENTSPACE_SSH_CMD", &bad)
        .env("AGENTSPACE_SCP_CMD", &bad)
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
}

#[test]
fn a_relative_value_is_resolved_against_the_working_folder_and_spaces_are_part_of_it() {
    // The working folder is `work`; the stand-ins are in a sibling folder whose name has a space.
    let rig = cached_rig();
    let out = rig
        .op(&["up"])
        .env("AGENTSPACE_AWS_CMD", "..\\stand ins\\aws.exe")
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), format!("Ready: {ID}\n"));
    assert_eq!(rig.keys().len(), 2);
}

// ---- The default ssh is by full path, never found through PATH ----

#[test]
fn the_default_ssh_is_windows_openssh_by_full_path_and_is_not_found_on_path() {
    // SystemRoot holds nothing, and PATH holds a folder with an `ssh.exe` stand-in. A program
    // found on PATH would run (and be recorded); the full path does not exist.
    let rig = cached_rig();
    let ssh = rig
        .system_root
        .join("System32")
        .join("OpenSSH")
        .join("ssh.exe");
    let out = rig
        .op(&["grow"])
        .env_remove("AGENTSPACE_SSH_CMD")
        .env("PATH", &rig.standins)
        .run();
    assert_fails_with(
        &out,
        &format!(
            "agentspace: {} is missing; install the Windows OpenSSH client.",
            ssh.display()
        ),
        "grow with the default ssh",
    );
    assert!(rig.calls_of("ssh").is_empty(), "ssh was found through PATH");

    // The push warns as for any failure, and starts nothing found on PATH.
    let rig = cached_rig();
    rig.localswim();
    let out = rig
        .op(&["up"])
        .env_remove("AGENTSPACE_SSH_CMD")
        .env("PATH", &rig.standins)
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert!(rig.calls_of("ssh").is_empty(), "ssh was found through PATH");
    assert_eq!(
        out.err_lines().last().map(String::as_str),
        Some(
            "agentspace: Could not send the localswim token; the board will not work on the box this session."
        )
    );
}

// ---- Every program gets the environment the spec gives it ----

#[test]
fn every_program_gets_nested_set_and_no_posix_fd_state_variable() {
    let planted = [
        "0F1E2D3C_4B5A_6978_8796_A5B4C3D2E1F0_POSIX_FD_STATE",
        "ANOTHER_POSIX_FD_STATE",
    ];
    for command in [vec!["up"], vec!["down"], vec!["status"], vec!["grow"]] {
        let rig = cached_rig();
        rig.localswim();
        rig.instance("stopped");
        if command[0] == "grow" {
            rig.instance("running");
        }
        let mut op = rig.op(&command);
        for name in planted {
            op = op.env(name, "1");
        }
        let out = op.run();
        assert_eq!(out.code, Some(0), "{command:?}: {:?}", out.err());
        let calls = rig.calls();
        assert!(!calls.is_empty(), "{command:?}");
        for c in calls {
            assert_eq!(
                c.nested, "1",
                "{command:?}: {} got AGENTSPACE_NESTED={}",
                c.key, c.nested
            );
            assert_eq!(
                c.fd_state, 0,
                "{command:?}: {} saw a *_POSIX_FD_STATE variable",
                c.key
            );
        }
    }
}

#[test]
fn a_nested_value_of_the_operators_own_is_not_what_children_get() {
    // The operator sets AGENTSPACE_NESTED to 1 for everything it starts, whatever it read.
    let rig = cached_rig();
    let out = rig.op(&["down"]).env("AGENTSPACE_NESTED", "yes").run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(rig.calls()[0].nested, "1");
}

// ---- stdin ----

#[test]
fn no_program_a_command_starts_gets_the_operators_stdin() {
    for command in [vec!["up"], vec!["down"], vec!["status"], vec!["grow"]] {
        let rig = cached_rig();
        rig.instance("stopped");
        let out = rig.run(&command);
        assert_eq!(out.code, Some(0), "{command:?}: {:?}", out.err());
        assert!(
            !rig.calls().is_empty(),
            "{command:?}: something was started"
        );
        for c in rig.calls() {
            assert!(
                c.stdin.is_empty(),
                "{command:?}: {} was given {} bytes of the operator's stdin",
                c.key,
                c.stdin.len()
            );
        }
    }
}

#[test]
fn no_command_waits_for_its_stdin() {
    for command in [vec!["up"], vec!["down"], vec!["status"], vec!["grow"]] {
        let rig = cached_rig();
        rig.instance("stopped");
        let out = rig
            .op(&command)
            .hold_stdin()
            .timeout(std::time::Duration::from_secs(60))
            .run();
        assert_eq!(out.code, Some(0), "{command:?}: {:?}", out.err());
    }
}

// ---- A .cmd file as a stand-in ----

/// Splits a command line as the C runtime does for text with no backslash before a quote: on
/// spaces outside double quotes, the quotes removed.
fn split_command_line(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut any = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            ' ' if !quoted => {
                if any {
                    args.push(std::mem::take(&mut current));
                    any = false;
                }
            }
            c => {
                current.push(c);
                any = true;
            }
        }
    }
    if any {
        args.push(current);
    }
    args
}

fn write_cmd(path: &std::path::Path, lines: &[&str]) {
    std::fs::write(path, lines.join("\r\n") + "\r\n").expect("a .cmd stand-in");
}

#[test]
fn a_cmd_file_can_stand_in_for_the_aws_cli() {
    let rig = Rig::new();
    rig.write_cache(ID);
    let dir = &rig.standins;
    std::fs::write(dir.join("di.json"), full_instance("running", &old_launch())).unwrap();
    std::fs::write(dir.join("dv.json"), "{\"Volumes\":[{\"Size\":8}]}").unwrap();
    std::fs::write(dir.join("ssm.json"), ssm_json("Online")).unwrap();
    let cmd = dir.join("aws.cmd");
    write_cmd(
        &cmd,
        &[
            "@echo off",
            "if \"%~2\"==\"describe-instances\" type \"%~dp0di.json\" & exit /b 0",
            "if \"%~2\"==\"describe-volumes\" type \"%~dp0dv.json\" & exit /b 0",
            "if \"%~2\"==\"describe-instance-information\" type \"%~dp0ssm.json\" & exit /b 0",
            "echo unexpected call 1>&2",
            "exit /b 9",
        ],
    );
    let out = rig
        .op(&["status"])
        .real_windows()
        .env("AGENTSPACE_AWS_CMD", &cmd)
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        out.err(),
        format!(
            "Instance  {ID}\nState     running\nType      m9g.xlarge\nDisk      8 GiB gp3 ({VOL})\nSSM       online\n"
        )
    );
}

#[test]
fn a_cmd_stand_in_for_ssh_gets_the_remote_command_as_one_quoted_argument() {
    // FA-15: one pair of double quotes gives the remote command back whole to a .cmd stand-in.
    let rig = cached_rig();
    rig.localswim();
    let cmd = rig.standins.join("ssh.cmd");
    write_cmd(
        &cmd,
        &[
            "@echo off",
            ">\"%~dp0ssh-args.txt\" echo %*",
            "\"%SystemRoot%\\System32\\findstr.exe\" \"^\" >\"%~dp0ssh-stdin.txt\"",
            "exit /b 0",
        ],
    );
    let out = rig
        .op(&["up"])
        .real_windows()
        .env("AGENTSPACE_SSH_CMD", &cmd)
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let line = std::fs::read_to_string(rig.standins.join("ssh-args.txt")).expect("the ssh was run");
    let line = line.trim_end();
    // Rust quotes an argument that holds `=` for a batch file, so `%*` shows `"BatchMode=yes"`;
    // the line is read back as the C runtime reads a command line.
    let args = split_command_line(line);
    assert_eq!(
        args.len(),
        6,
        "{} arguments, with the remote command one",
        args.len()
    );
    assert_eq!(
        &args[..5],
        [
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "BatchMode=yes",
            "agentspace"
        ]
    );
    assert_remote_command_rule(&args[5], "the push's remote command");
    let raw = line.rsplit(' ').next().unwrap_or("");
    assert!(
        line.ends_with('"') && !raw.is_empty(),
        "the remote command is wrapped in double quotes"
    );
    let stdin = std::fs::read_to_string(rig.standins.join("ssh-stdin.txt")).expect("the stdin");
    assert!(
        stdin.starts_with("LOCALSWIM_URL="),
        "the content reached the .cmd's stdin"
    );
}

// ---- deploy and sync are dispatched to the sync crate ----

#[test]
fn deploy_and_sync_reach_the_sync_crates_functions_and_start_no_program_of_their_own() {
    // What holds whether agentspace-sync is the pinned stub (`<command> is not built yet.`) or
    // E0.06-T03-S02's build, which stops in this rig before any program: `deploy` at the missing
    // `id_ed25519.pub`, `sync` at its CA check's missing `ssh-keygen.exe`. The rig gives the run no
    // PATH, so no `sync` can find the real `go`.
    for command in ["deploy", "sync"] {
        let rig = cached_rig();
        let out = rig.run(&[command]);
        assert_eq!(out.code, Some(1), "{command}: {:?}", out.err());
        assert_eq!(
            out.out(),
            "",
            "{command}: stdout MUST be empty on a failure"
        );
        let lines = out.err_lines();
        assert_eq!(lines.len(), 1, "{command}: one line, not {lines:?}");
        assert!(lines[0].starts_with("agentspace: "), "{command}: {lines:?}");
        assert!(rig.calls().is_empty(), "{command}: a stand-in was started");
    }
}

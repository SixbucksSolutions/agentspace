//! Checks that need a real process environment and a real stdin: the test binary runs itself as a
//! child (`child_*` tests, which do nothing unless `AGENTSPACE_TEST_CHILD` is set), because a test
//! cannot call `set_var` (edition 2024) and the library must be seen reading the real environment.
//!
//! Covers: no program inherits a stdin; `AGENTSPACE_NESTED=1` for every program started; every
//! `*_POSIX_FD_STATE` variable removed; `ssh_program` (docs/operator.md: Environment variables).

mod common;

use agentspace_operator_api::{Aws, Call, Context, Output, Remote};
use common::{Rig, Standin};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CHILD: &str = "AGENTSPACE_TEST_CHILD";
const DIR: &str = "AGENTSPACE_TEST_DIR";
const NESTED: &str = "AGENTSPACE_TEST_NESTED";

fn child_ctx(dir: &Path) -> Context {
    Context {
        home: dir.join("home"),
        local_app_data: None,
        system_root: dir.join("root"),
        nested: std::env::var_os(NESTED).is_some_and(|v| v == "1"),
    }
}

/// Runs this test binary again, running only `test`, with a throwaway home and the given extra
/// environment, and with a pipe on its stdin that holds data nobody should read.
fn run_child(test: &str, dir: &Path, extra: &[(&str, &std::ffi::OsStr)]) {
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, test)
        .env(DIR, dir)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("LOCALAPPDATA", home.join("local"))
        .env("APPDATA", home.join("roaming"))
        .env_remove("AGENTSPACE_NESTED")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    // Held open: a program that inherited this pipe would read it or wait on it.
    let _ = stdin.write_all(b"LEAKED-STDIN-DATA\n");
    let _ = stdin.flush();
    let out = child.wait_with_output().unwrap();
    drop(stdin);
    assert!(
        out.status.success(),
        "Child {test} failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn is_child(test: &str) -> Option<PathBuf> {
    (std::env::var(CHILD).as_deref() == Ok(test))
        .then(|| PathBuf::from(std::env::var(DIR).unwrap()))
}

// ---------------------------------------------------------------- The child side

#[test]
fn child_start_every_program() {
    let Some(dir) = is_child("child_start_every_program") else {
        return;
    };
    let r = common::make_runner_real_env(child_ctx(&dir));
    let aws_call = Call {
        service: "ec2",
        operation: "describe-instances",
        args: &[],
        output: Output::Json,
    };
    r.call(&aws_call).unwrap();
    let pass = Call {
        output: Output::PassThrough,
        ..aws_call
    };
    r.call(&pass).unwrap();
    let ignore = Call {
        output: Output::Ignore,
        ..aws_call
    };
    r.call(&ignore).unwrap();
    assert_eq!(r.ssh(&["devbox"], Some("true")), Ok(0));
    assert_eq!(r.scp(&["a", "devbox:b"]), Ok(0));
}

#[test]
fn child_ssh_program() {
    let Some(dir) = is_child("child_ssh_program") else {
        return;
    };
    let ctx = child_ctx(&dir);
    let found = common::ssh_program(&ctx);
    let text = std::fs::read_to_string(dir.join("expect.txt")).unwrap();
    match text.trim().strip_prefix("err:") {
        Some(message) => assert_eq!(found.unwrap_err().message, message),
        None => assert_eq!(found.unwrap(), PathBuf::from(text.trim())),
    }
}

// ---------------------------------------------------------------- The parent side

fn start_rig(nested: bool) -> (Rig, Standin, Standin, Standin) {
    let rig = Rig::with_nested(nested);
    let aws = rig.standin("aws").stdout("{}").write();
    let ssh = rig.standin("ssh").write();
    let scp = rig.standin("scp").write();
    (rig, aws, ssh, scp)
}

fn run_start_every_program(nested: bool) -> (Rig, Standin, Standin, Standin) {
    let (rig, aws, ssh, scp) = start_rig(nested);
    run_child(
        "child_start_every_program",
        rig.dir.path(),
        &[
            ("AGENTSPACE_AWS_CMD", &aws.value()),
            ("AGENTSPACE_SSH_CMD", &ssh.value()),
            ("AGENTSPACE_SCP_CMD", &scp.value()),
            (NESTED, if nested { "1".as_ref() } else { "0".as_ref() }),
            // Windows OpenSSH hands each process a variable like these (card #0026), and the
            // operator must remove every one, whatever the GUID.
            ("0B1F7A52_9C3D_4E8A_POSIX_FD_STATE", "1".as_ref()),
            ("BAR_POSIX_FD_STATE", "2".as_ref()),
            ("KEEP_ME_FD_STATE_NOT", "kept".as_ref()),
        ],
    );
    (rig, aws, ssh, scp)
}

#[test]
fn no_program_inherits_a_stdin() {
    let (_rig, aws, ssh, scp) = run_start_every_program(false);
    for (name, s) in [("aws", &aws), ("ssh", &ssh), ("scp", &scp)] {
        assert!(s.ran(), "{name} was not started");
        assert_eq!(s.stdin(), "", "{name} read the test process's stdin");
        assert!(!s.stdin().contains("LEAKED"), "{name}");
    }
}

#[test]
fn every_program_gets_nested_set_to_one() {
    for nested in [false, true] {
        let (_rig, aws, ssh, scp) = run_start_every_program(nested);
        for (name, s) in [("aws", &aws), ("ssh", &ssh), ("scp", &scp)] {
            assert_eq!(
                s.env_var("AGENTSPACE_NESTED").as_deref(),
                Some("1"),
                "{name} (nested={nested}) lacks AGENTSPACE_NESTED=1"
            );
        }
    }
}

#[test]
fn no_program_gets_a_variable_ending_posix_fd_state() {
    let (_rig, aws, ssh, scp) = run_start_every_program(false);
    for (name, s) in [("aws", &aws), ("ssh", &ssh), ("scp", &scp)] {
        let names = s.fd_state_names();
        assert!(
            names.is_empty(),
            "{name} got {} *_POSIX_FD_STATE variable(s), named {names:?}",
            names.len()
        );
        // Sanity: the rest of the environment still reaches the program.
        assert_eq!(
            s.env_var("KEEP_ME_FD_STATE_NOT").as_deref(),
            Some("kept"),
            "{name} lost an unrelated variable"
        );
    }
}

#[test]
fn ssh_program_reads_the_seam_whole_and_defaults_to_open_ssh_under_system_root() {
    // A value: the path, taken whole.
    let rig = Rig::new();
    let ssh = rig.standin("spaced dir/ssh").write();
    std::fs::write(
        rig.dir.path().join("expect.txt"),
        ssh.path().display().to_string(),
    )
    .unwrap();
    run_child(
        "child_ssh_program",
        rig.dir.path(),
        &[("AGENTSPACE_SSH_CMD", &ssh.value())],
    );

    // Unset: Windows OpenSSH's ssh.exe under the context's SystemRoot, by full path.
    drop(rig); // The rig's lock is held until it drops; a shadowed one would deadlock.
    let rig = Rig::new();
    let expected = rig
        .ctx
        .system_root
        .join("System32")
        .join("OpenSSH")
        .join("ssh.exe");
    rig.default_openssh("ssh.exe").write();
    std::fs::write(
        rig.dir.path().join("expect.txt"),
        expected.display().to_string(),
    )
    .unwrap();
    run_child("child_ssh_program", rig.dir.path(), &[]);

    // Whitespace: the same default.
    run_child(
        "child_ssh_program",
        rig.dir.path(),
        &[("AGENTSPACE_SSH_CMD", " \t ".as_ref())],
    );
}

#[test]
fn ssh_program_fails_with_the_seam_message_for_a_value_naming_nothing() {
    let rig = Rig::new();
    let value = rig.dir.path().join("no such ssh").display().to_string();
    // A working default is there to be (wrongly) fallen back to.
    rig.default_openssh("ssh.exe").write();
    std::fs::write(
        rig.dir.path().join("expect.txt"),
        format!(
            "err:{}",
            common::cannot_run_message("AGENTSPACE_SSH_CMD", &value)
        ),
    )
    .unwrap();
    run_child(
        "child_ssh_program",
        rig.dir.path(),
        &[("AGENTSPACE_SSH_CMD", value.as_ref())],
    );
}

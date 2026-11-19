//! The test seam's rules for AGENTSPACE_AWS_CMD, _SSH_CMD and _SCP_CMD (docs/operator.md: The
//! test seam, Programs it starts; cards #0085, #0027).

mod common;

use agentspace_operator_api::{Aws, AwsError, Call, CallerFailure, Output, Remote};
use common::{Rig, cannot_run_message, not_runnable_file};

fn aws_call() -> Call<'static> {
    Call {
        service: "ec2",
        operation: "describe-instances",
        args: &[],
        output: Output::Json,
    }
}

// ---------------------------------------------------------------- One path, taken whole

#[test]
fn a_path_with_a_space_is_one_program_for_each_variable() {
    let rig = Rig::new();
    let aws = rig.standin("my tools/aws dir").stdout("{}").write();
    let ssh = rig.standin("my tools/ssh dir").write();
    let scp = rig.standin("my tools/scp dir").write();
    rig.env.set("AGENTSPACE_AWS_CMD", aws.value());
    rig.env.set("AGENTSPACE_SSH_CMD", ssh.value());
    rig.env.set("AGENTSPACE_SCP_CMD", scp.value());
    let r = rig.runner();
    r.call(&aws_call()).unwrap();
    assert_eq!(r.ssh(&["devbox"], Some("true")), Ok(0));
    assert_eq!(r.scp(&["a", "devbox:b"]), Ok(0));
    assert!(aws.ran() && ssh.ran() && scp.ran());
    // The value's words are not arguments.
    assert_eq!(scp.args(), ["a", "devbox:b"]);
}

#[test]
fn a_value_with_arguments_is_not_split() {
    // "<prog> --flag" is one path that names nothing, so it fails, and the program is not run.
    let rig = Rig::new();
    let ssh = rig.standin("ssh").write();
    let value = format!("{} --flag", ssh.path().display());
    rig.env.set("AGENTSPACE_SSH_CMD", value.clone());
    let f = rig.runner().ssh(&["devbox"], None).unwrap_err();
    assert_eq!(f.message, cannot_run_message("AGENTSPACE_SSH_CMD", &value));
    assert!(!ssh.ran());
}

// ---------------------------------------------------------------- Read each time

#[test]
fn the_variable_is_read_each_time_a_program_starts() {
    let rig = Rig::new();
    let first = rig.standin("first").write();
    let second = rig.standin("second").write();
    let r = rig.runner(); // Built before either is set.
    rig.env.set("AGENTSPACE_SCP_CMD", first.value());
    assert_eq!(r.scp(&["one"]), Ok(0));
    rig.env.set("AGENTSPACE_SCP_CMD", second.value());
    assert_eq!(r.scp(&["two"]), Ok(0));
    assert_eq!(first.args(), ["one"]);
    assert_eq!(second.args(), ["two"]);
}

#[test]
fn a_variable_set_after_a_failure_takes_effect_on_the_next_call() {
    let rig = Rig::new();
    let r = rig.runner();
    rig.env.set("AGENTSPACE_SSH_CMD", "/nonexistent/ssh");
    assert!(r.ssh(&["devbox"], None).is_err());
    let ssh = rig.standin("ssh").write();
    rig.env.set("AGENTSPACE_SSH_CMD", ssh.value());
    assert_eq!(r.ssh(&["devbox"], None), Ok(0));
}

#[test]
fn each_variable_governs_only_its_own_program() {
    let rig = Rig::new();
    let scp = rig.standin("scp").write();
    rig.env.set("AGENTSPACE_SCP_CMD", scp.value());
    rig.env.set("AGENTSPACE_SSH_CMD", "/nonexistent/ssh");
    assert_eq!(rig.runner().scp(&["a", "b"]), Ok(0));
}

// ---------------------------------------------------------------- Unset, empty, whitespace: the default

fn default_ssh_rig() -> (Rig, common::Standin) {
    // The default ssh is `%SystemRoot%\System32\OpenSSH\ssh.exe`, by full path, never PATH (#0027).
    let rig = Rig::new();
    let ssh = rig.default_openssh("ssh.exe").exit(0).write();
    (rig, ssh)
}

#[test]
fn unset_means_the_default_ssh_under_system_root() {
    let (rig, ssh) = default_ssh_rig();
    let got = rig.runner().ssh(&["devbox"], Some("true"));
    if common::started_default(got, &ssh) {
        assert_eq!(ssh.args(), ["devbox", "true"]);
    }
}

#[test]
fn empty_and_whitespace_mean_the_default_ssh() {
    for value in ["", " ", "   ", "\t", " \t "] {
        let (rig, ssh) = default_ssh_rig();
        rig.env.set("AGENTSPACE_SSH_CMD", value);
        common::started_default(rig.runner().ssh(&["devbox"], None), &ssh);
    }
}

#[test]
fn unset_means_the_default_scp_under_system_root() {
    let rig = Rig::new();
    let scp = rig.default_openssh("scp.exe").write();
    for value in [None, Some(""), Some("  ")] {
        match value {
            Some(v) => rig.env.set("AGENTSPACE_SCP_CMD", v),
            None => rig.env.unset("AGENTSPACE_SCP_CMD"),
        }
        common::started_default(rig.runner().scp(&["a", "devbox:b"]), &scp);
    }
}

#[test]
fn empty_or_whitespace_aws_value_is_the_default_not_a_cannot_run_failure() {
    // The default AWS CLI is a fixed path, else `aws` on PATH; whichever is found, the seam's
    // refusal must not appear. A made-up service keeps even a real CLI from reaching the network.
    for value in ["", "  \t "] {
        let rig = Rig::new();
        rig.env.set("AGENTSPACE_AWS_CMD", value);
        let c = Call {
            service: "zz-no-such-service",
            operation: "zz-no-such-operation",
            args: &[],
            output: Output::Json,
        };
        if let Err(AwsError::CannotRun(m)) = rig.runner().call(&c) {
            assert!(!m.contains("AGENTSPACE_AWS_CMD"), "{m}");
        }
    }
}

// ---------------------------------------------------------------- A value naming nothing runnable

#[test]
fn a_missing_file_a_folder_and_a_non_executable_each_fail_and_never_fall_back() {
    let rig = Rig::new();
    // A working default is there to be (wrongly) fallen back to.
    let default_ssh = rig.default_openssh("ssh.exe").write();
    let default_scp = rig.default_openssh("scp.exe").write();
    let folder = rig.dir.path().join("a folder");
    std::fs::create_dir_all(&folder).unwrap();
    let plain = not_runnable_file(&rig.dir.path().join("plain dir"));
    let values = [
        rig.dir.path().join("no such file").display().to_string(),
        folder.display().to_string(),
        plain.display().to_string(),
    ];
    let r = rig.runner();
    for value in &values {
        rig.env.set("AGENTSPACE_SSH_CMD", value.as_str());
        rig.env.set("AGENTSPACE_SCP_CMD", value.as_str());
        rig.env.set("AGENTSPACE_AWS_CMD", value.as_str());

        let f = r.ssh(&["devbox"], Some("true")).unwrap_err();
        assert_eq!(
            f.message,
            cannot_run_message("AGENTSPACE_SSH_CMD", value),
            "ssh {value}"
        );

        let f = r.scp(&["a", "devbox:b"]).unwrap_err();
        assert_eq!(
            f.message,
            cannot_run_message("AGENTSPACE_SCP_CMD", value),
            "scp {value}"
        );

        assert_eq!(
            r.call(&aws_call()),
            Err(AwsError::CannotRun(cannot_run_message(
                "AGENTSPACE_AWS_CMD",
                value
            ))),
            "aws {value}"
        );
    }
    assert!(!default_ssh.ran() && !default_scp.ran(), "Fell back");
}

#[cfg(unix)]
#[test]
fn an_executable_file_the_system_cannot_start_gives_the_seam_message() {
    // Marked executable, so only starting it shows that it cannot run: its interpreter does not
    // exist. It is still a value that names nothing runnable, never "Could not run". (On Windows,
    // the non-executable case above is this one: a text file passes as a file and fails to start.)
    use std::os::unix::fs::PermissionsExt;
    let rig = Rig::new();
    let dir = rig.dir.path().join("broken dir");
    std::fs::create_dir_all(&dir).unwrap();
    let broken = dir.join("broken");
    std::fs::write(&broken, "#!/nonexistent/interpreter\n").unwrap();
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o755)).unwrap();
    let value = broken.display().to_string();
    for var in [
        "AGENTSPACE_AWS_CMD",
        "AGENTSPACE_SSH_CMD",
        "AGENTSPACE_SCP_CMD",
    ] {
        rig.env.set(var, value.as_str());
    }
    let r = rig.runner();
    let f = r.ssh(&["devbox"], Some("true")).unwrap_err();
    assert_eq!(f.message, cannot_run_message("AGENTSPACE_SSH_CMD", &value));
    let f = r.scp(&["a", "devbox:b"]).unwrap_err();
    assert_eq!(f.message, cannot_run_message("AGENTSPACE_SCP_CMD", &value));
    for output in [Output::Json, Output::PassThrough, Output::Ignore] {
        let c = Call {
            output,
            ..aws_call()
        };
        assert_eq!(
            r.call(&c),
            Err(AwsError::CannotRun(cannot_run_message(
                "AGENTSPACE_AWS_CMD",
                &value
            ))),
            "{output:?}"
        );
    }
}

#[test]
fn cannot_run_failures_convert_to_could_not_reach_for_a_caller() {
    // From<AwsError> and the Failure the Remote returns: the caller's line is `CouldNotReach`.
    let rig = Rig::new();
    rig.env.set("AGENTSPACE_SSH_CMD", "/nonexistent/ssh");
    let f = rig.runner().ssh(&["devbox"], None).unwrap_err();
    assert_eq!(f.caller, CallerFailure::CouldNotReach);
}

#[test]
fn the_default_ssh_and_scp_are_never_found_on_path() {
    // #0027: with no seam value and no Windows OpenSSH under SystemRoot, an ssh or scp on PATH
    // is never started; the default's "is missing" line is the answer.
    let rig = Rig::new();
    let (ssh_name, scp_name) = if cfg!(windows) {
        ("ssh.cmd", "scp.cmd")
    } else {
        ("ssh", "scp")
    };
    let ssh = rig.standin("path ssh").named(ssh_name).write();
    let scp = rig.standin("path scp").named(scp_name).write();
    let path = std::env::join_paths([&ssh.dir, &scp.dir]).unwrap();
    rig.env.set("PATH", path);
    let r = rig.runner();
    let openssh = rig.ctx.system_root.join("System32").join("OpenSSH");
    for (name, got) in [
        ("ssh.exe", r.ssh(&["devbox"], Some("true"))),
        ("scp.exe", r.scp(&["a", "devbox:b"])),
    ] {
        assert_eq!(
            got.unwrap_err().message,
            format!(
                "agentspace: {} is missing; install the Windows OpenSSH client.",
                openssh.join(name).display()
            )
        );
    }
    assert!(!ssh.ran() && !scp.ran(), "A program on PATH was started");
}

#[test]
fn a_bare_name_is_never_looked_up_on_path() {
    // A bare name is looked up nowhere: the seam never uses PATH. `sh` exists on PATH on Unix,
    // `cmd` on Windows, and neither may be started by its bare name.
    let bare = if cfg!(windows) { "cmd" } else { "sh" };
    let rig = Rig::new();
    rig.env.set("AGENTSPACE_SCP_CMD", bare);
    let f = rig.runner().scp(&["a", "b"]).unwrap_err();
    assert_eq!(f.message, cannot_run_message("AGENTSPACE_SCP_CMD", bare));
}

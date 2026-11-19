//! The architect's rulings of 2026-11-19 for E0.06-T01-S02 (docs/operator.md: Programs it starts,
//! "When a default program cannot be run"; AWS failures; Calls by command): the messages of a
//! default program that cannot be run, the FA-15 refusal, the error text of a failed AWS call,
//! an answer that is not JSON, and which calls pass `--output json`.

mod common;

use agentspace_operator_api::{Aws, AwsError, Call, CallerFailure, Output, Remote};
use common::{Rig, not_runnable_file};

const AWS_FIXED_PATH: &str = r"C:\Program Files\Amazon\AWSCLIV2\aws.exe";

fn call<'a>(output: Output, args: &'a [&'a str]) -> Call<'a> {
    Call {
        service: "ec2",
        operation: "describe-instances",
        args,
        output,
    }
}

fn aws_with_seam(rig: &Rig, s: &common::Standin) {
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
}

// ---------------------------------------------------------------- A default that is missing

#[test]
fn a_missing_default_ssh_and_scp_name_their_path_and_say_to_install_openssh() {
    let rig = Rig::new();
    let r = rig.runner();
    for (name, f) in [
        ("ssh.exe", r.ssh(&["devbox"], Some("true"))),
        ("scp.exe", r.scp(&["a", "devbox:b"])),
    ] {
        let path = rig
            .ctx
            .system_root
            .join("System32")
            .join("OpenSSH")
            .join(name);
        let f = f.unwrap_err();
        assert_eq!(
            f.message,
            format!(
                "agentspace: {} is missing; install the Windows OpenSSH client.",
                path.display()
            )
        );
        assert_eq!(f.caller, CallerFailure::CouldNotReach);
    }
}

#[test]
fn a_missing_default_ssh_program_fails_the_same_for_the_push() {
    let rig = Rig::new();
    let path = rig
        .ctx
        .system_root
        .join("System32")
        .join("OpenSSH")
        .join("ssh.exe");
    let f = agentspace_run::ssh_program_with(&rig.ctx, &|_| None).unwrap_err();
    assert_eq!(
        f.message,
        format!(
            "agentspace: {} is missing; install the Windows OpenSSH client.",
            path.display()
        )
    );
    assert_eq!(f.caller, CallerFailure::CouldNotReach);
}

#[test]
fn a_default_ssh_and_scp_that_exist_but_cannot_start_say_could_not_run() {
    let rig = Rig::new();
    let dir = rig.ctx.system_root.join("System32").join("OpenSSH");
    for name in ["ssh.exe", "scp.exe"] {
        let plain = not_runnable_file(&dir);
        std::fs::rename(&plain, dir.join(name)).unwrap();
    }
    let r = rig.runner();
    for (name, f) in [
        ("ssh.exe", r.ssh(&["devbox"], None)),
        ("scp.exe", r.scp(&["a", "devbox:b"])),
    ] {
        let f = f.unwrap_err();
        assert_eq!(
            f.message,
            format!("agentspace: Could not run {}.", dir.join(name).display())
        );
        assert_eq!(f.caller, CallerFailure::CouldNotReach);
    }
}

// ---------------------------------------------------------------- The AWS CLI found neither way

#[test]
fn an_aws_cli_found_neither_way_says_it_is_not_installed() {
    if std::path::Path::new(AWS_FIXED_PATH).is_file() {
        return; // A real install at the fixed path: this case cannot be reached here.
    }
    let rig = Rig::new();
    let want = Err(AwsError::CannotRun(format!(
        "agentspace: The AWS CLI is not installed: {AWS_FIXED_PATH} is missing and no aws is on PATH."
    )));
    // PATH unset, and PATH naming a folder with no aws in it.
    assert_eq!(rig.runner().call(&call(Output::Json, &[])), want);
    rig.env.set("PATH", rig.dir.path().as_os_str());
    assert_eq!(rig.runner().call(&call(Output::Json, &[])), want);
    rig.env.set("PATH", "");
    assert_eq!(rig.runner().call(&call(Output::Json, &[])), want);
}

#[test]
fn an_aws_on_path_is_the_fallback_when_the_fixed_path_is_missing() {
    if std::path::Path::new(AWS_FIXED_PATH).is_file() {
        return;
    }
    let rig = Rig::new();
    let name = if cfg!(windows) { "aws.cmd" } else { "aws" };
    let s = rig.standin("bin").named(name).stdout("{}").write();
    rig.env.set("PATH", s.dir.as_os_str());
    assert_eq!(
        rig.runner().call(&call(Output::Json, &[])),
        Ok(serde_json::json!({}))
    );
    assert!(s.ran());
}

// ---------------------------------------------------------------- The FA-15 refusal

#[test]
fn the_fa15_refusal_names_the_reason_and_never_the_command() {
    let (rig, ssh) = {
        let rig = Rig::new();
        let ssh = rig.standin("ssh").write();
        rig.env.set("AGENTSPACE_SSH_CMD", ssh.value());
        (rig, ssh)
    };
    let r = rig.runner();
    for (bad, reason) in [
        ("secret%one", "it holds a percent sign"),
        ("secret\"one", "it holds a double quote"),
        ("secret\none", "it holds a control character"),
        ("secretone\\", "it ends in a backslash"),
    ] {
        let f = r.ssh(&["devbox"], Some(bad)).unwrap_err();
        assert_eq!(
            f.message,
            format!("agentspace: The remote command cannot be sent: {reason}.")
        );
        assert!(!f.message.contains("secret"));
        assert_eq!(f.caller, CallerFailure::CouldNotReach);
    }
    assert!(!ssh.ran());
}

// ---------------------------------------------------------------- The error text

#[test]
fn error_text_is_stderr_then_stdout_then_the_exit_code() {
    for output in [Output::Json, Output::PassThrough] {
        // Both: stderr wins.
        let rig = Rig::new();
        let s = rig
            .standin("aws")
            .exit(2)
            .stdout("out")
            .stderr("err")
            .write();
        aws_with_seam(&rig, &s);
        assert_eq!(
            rig.runner().call(&call(output, &[])),
            Err(AwsError::Failed("err".to_string())),
            "{output:?}"
        );
        // Stderr empty (or blank): stdout, trimmed.
        drop(rig);
        let rig = Rig::new();
        let s = rig
            .standin("aws")
            .exit(2)
            .stdout("\n out text \n")
            .stderr("  \n")
            .write();
        aws_with_seam(&rig, &s);
        assert_eq!(
            rig.runner().call(&call(output, &[])),
            Err(AwsError::Failed("out text".to_string())),
            "{output:?}"
        );
        // Both empty: the exit code.
        drop(rig);
        let rig = Rig::new();
        let s = rig.standin("aws").exit(3).write();
        aws_with_seam(&rig, &s);
        assert_eq!(
            rig.runner().call(&call(output, &[])),
            Err(AwsError::Failed(
                "The AWS CLI failed with exit code 3.".to_string()
            )),
            "{output:?}"
        );
    }
}

#[test]
fn an_answer_that_is_not_json_names_the_service_and_operation() {
    let rig = Rig::new();
    let s = rig.standin("aws").stdout("this is not json").write();
    aws_with_seam(&rig, &s);
    assert_eq!(
        rig.runner().call(&call(Output::Json, &[])),
        Err(AwsError::Failed(
            "Unexpected answer from ec2 describe-instances: not JSON.".to_string()
        ))
    );
}

// ---------------------------------------------------------------- Which calls pass --output json

fn args_of(rig: &Rig, c: &Call) -> Vec<String> {
    let s = rig.standin("aws").write();
    aws_with_seam(rig, &s);
    rig.runner().call(c).unwrap();
    s.args()
}

fn has_output_json(args: &[String]) -> bool {
    args.windows(2)
        .any(|w| w[0] == "--output" && w[1] == "json")
}

#[test]
fn every_call_but_deploy_and_the_stopped_wait_passes_output_json() {
    // The call's shape decides, not the way its answer is handled.
    let shapes: [(&str, &str, &[&str], bool); 6] = [
        (
            "cloudformation",
            "deploy",
            &["--stack-name", "agentbox"],
            false,
        ),
        (
            "ec2",
            "wait",
            &["instance-stopped", "--instance-ids", "i-0123456789abcdef0"],
            false,
        ),
        (
            "ec2",
            "wait",
            &["instance-running", "--instance-ids", "i-0123456789abcdef0"],
            true,
        ),
        (
            "ec2",
            "start-instances",
            &["--instance-ids", "i-0123456789abcdef0"],
            true,
        ),
        (
            "ec2",
            "stop-instances",
            &["--instance-ids", "i-0123456789abcdef0"],
            true,
        ),
        (
            "ec2",
            "modify-volume",
            &["--volume-id", "vol-0123456789abcdef0", "--size", "12"],
            true,
        ),
    ];
    for (service, operation, args, want) in shapes {
        for output in [Output::Json, Output::PassThrough, Output::Ignore] {
            let rig = Rig::new();
            let c = Call {
                service,
                operation,
                args,
                output,
            };
            let got = {
                let s = rig.standin("aws").stdout("{}").write();
                aws_with_seam(&rig, &s);
                rig.runner().call(&c).unwrap();
                s.args()
            };
            assert_eq!(
                has_output_json(&got),
                want,
                "{service} {operation} {args:?} as {output:?}: {got:?}"
            );
            assert_eq!(&got[..4], [service, operation, "--region", "us-west-2"]);
        }
    }
}

#[test]
fn an_ignored_call_of_another_shape_still_passes_output_json() {
    let rig = Rig::new();
    let got = args_of(&rig, &call(Output::Ignore, &["--instance-ids", "i-0"]));
    assert_eq!(
        got,
        [
            "ec2",
            "describe-instances",
            "--region",
            "us-west-2",
            "--output",
            "json",
            "--instance-ids",
            "i-0"
        ]
    );
}

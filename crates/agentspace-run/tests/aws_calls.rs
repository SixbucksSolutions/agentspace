//! The AWS CLI's call shapes, its sign-in test and its failure text (docs/operator.md: Calls by
//! command, AWS failures; cards #0034, FB-07).

mod common;

use agentspace_operator_api::{Aws, AwsError, Call, Output};
use common::Rig;
use serde_json::json;

fn call<'a>(output: Output, args: &'a [&'a str]) -> Call<'a> {
    Call {
        service: "ec2",
        operation: "describe-instances",
        args,
        output,
    }
}

fn aws_standin(rig: &Rig) -> common::Standin {
    rig.standin("aws")
}

// ---------------------------------------------------------------- Argument order (FB-07)

#[test]
fn json_call_orders_service_operation_region_output_then_own() {
    let rig = Rig::new();
    let s = aws_standin(&rig).stdout("{}").write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    let own = ["--instance-ids", "i-0123456789abcdef0"];
    rig.runner().call(&call(Output::Json, &own)).unwrap();
    assert_eq!(
        s.args(),
        [
            "ec2",
            "describe-instances",
            "--region",
            "us-west-2",
            "--output",
            "json",
            "--instance-ids",
            "i-0123456789abcdef0"
        ]
    );
}

#[test]
fn json_call_with_no_own_arguments() {
    let rig = Rig::new();
    let s = aws_standin(&rig).stdout("{}").write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    rig.runner().call(&call(Output::Json, &[])).unwrap();
    assert_eq!(
        s.args(),
        [
            "ec2",
            "describe-instances",
            "--region",
            "us-west-2",
            "--output",
            "json"
        ]
    );
}

#[test]
fn pass_through_call_has_region_but_no_output_flag() {
    // cloudformation deploy passes `--region` and no `--output json`.
    let rig = Rig::new();
    let s = aws_standin(&rig).write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    let own = [
        "--stack-name",
        "agentbox",
        "--capabilities",
        "CAPABILITY_IAM",
    ];
    let c = Call {
        service: "cloudformation",
        operation: "deploy",
        args: &own,
        output: Output::PassThrough,
    };
    assert_eq!(rig.runner().call(&c), Ok(serde_json::Value::Null));
    assert_eq!(
        s.args(),
        [
            "cloudformation",
            "deploy",
            "--region",
            "us-west-2",
            "--stack-name",
            "agentbox",
            "--capabilities",
            "CAPABILITY_IAM"
        ]
    );
}

#[test]
fn ignore_call_has_region_and_no_output_flag() {
    // Per operator.md (AWS failures), `ec2 wait instance-stopped` passes no `--output json`.
    // tests/rulings.rs covers the other call shapes.
    let rig = Rig::new();
    let s = aws_standin(&rig).write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    let own = ["--instance-ids", "i-0123456789abcdef0"];
    let c = Call {
        service: "ec2",
        operation: "wait",
        args: &["instance-stopped", own[0], own[1]],
        output: Output::Ignore,
    };
    rig.runner().call(&c).unwrap();
    assert_eq!(
        s.args(),
        [
            "ec2",
            "wait",
            "--region",
            "us-west-2",
            "instance-stopped",
            "--instance-ids",
            "i-0123456789abcdef0"
        ]
    );
}

#[test]
fn own_arguments_survive_spaces_and_quotes_whole() {
    let rig = Rig::new();
    let s = aws_standin(&rig).stdout("{}").write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    let own = [
        "--comment",
        "agentspace up: patch check",
        "--parameters",
        "commands=ls -l",
    ];
    rig.runner().call(&call(Output::Json, &own)).unwrap();
    let args = s.args();
    assert_eq!(&args[6..], own);
}

// ---------------------------------------------------------------- Json

#[test]
fn json_output_is_parsed() {
    let rig = Rig::new();
    let s = aws_standin(&rig)
        .stdout(r#"{"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":"i-0123456789abcdef0"}]}]}"#)
        .write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    let v = rig.runner().call(&call(Output::Json, &[])).unwrap();
    assert_eq!(
        v,
        json!({"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":"i-0123456789abcdef0"}]}]})
    );
}

#[test]
fn json_output_that_is_not_json_is_a_failure_not_a_panic() {
    // Ruled 2026-11-19: `Failed` with the "Unexpected answer" text (tests/rulings.rs).
    let rig = Rig::new();
    let s = aws_standin(&rig).stdout("this is not json").write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    let r = rig.runner().call(&call(Output::Json, &[]));
    assert!(matches!(r, Err(AwsError::Failed(_))), "{r:?}");
}

#[test]
fn pass_through_and_ignore_give_null_on_success() {
    for output in [Output::PassThrough, Output::Ignore] {
        let rig = Rig::new();
        let s = aws_standin(&rig).stdout("some text").write();
        rig.env.set("AGENTSPACE_AWS_CMD", s.value());
        assert_eq!(
            rig.runner().call(&call(output, &[])),
            Ok(serde_json::Value::Null)
        );
    }
}

// ---------------------------------------------------------------- Failure text and sign-in (#0034)

#[test]
fn other_failure_gives_the_cli_text_trimmed() {
    for output in [Output::Json, Output::PassThrough] {
        let rig = Rig::new();
        let s = aws_standin(&rig)
            .exit(254)
            .stderr("\n  An error occurred (ValidationError): Stack does not exist \n\n")
            .write();
        rig.env.set("AGENTSPACE_AWS_CMD", s.value());
        assert_eq!(
            rig.runner().call(&call(output, &[])),
            Err(AwsError::Failed(
                "An error occurred (ValidationError): Stack does not exist".to_string()
            )),
            "{output:?}"
        );
    }
}

#[test]
fn each_sign_in_phrase_gives_sign_in_in_any_case() {
    let phrases = [
        "The SSO session has expired",
        "THE TOKEN HAS EXPIRED",
        "ExPiReD",
        "Run aws login to refresh",
        "Please AWS LOGIN again",
        "Aws Login required",
        "Unable to locate credentials. You can configure credentials by running \"aws configure\".",
        "UNABLE TO LOCATE CREDENTIALS",
        "unable to locate credentials",
    ];
    for output in [Output::Json, Output::PassThrough] {
        for phrase in phrases {
            let rig = Rig::new();
            let s = aws_standin(&rig)
                .exit(255)
                .stderr(&format!("\nSome prefix. {phrase}\n"))
                .write();
            rig.env.set("AGENTSPACE_AWS_CMD", s.value());
            assert_eq!(
                rig.runner().call(&call(output, &[])),
                Err(AwsError::SignIn),
                "{output:?}: {phrase}"
            );
        }
    }
}

#[test]
fn sign_in_text_on_stdout_alone_is_not_a_sign_in_failure() {
    // The test reads the CLI's error text; a stdout that mentions it is not that.
    let rig = Rig::new();
    let s = aws_standin(&rig)
        .exit(1)
        .stdout("expired")
        .stderr("boom")
        .write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    assert_eq!(
        rig.runner().call(&call(Output::Json, &[])),
        Err(AwsError::Failed("boom".to_string()))
    );
}

#[test]
fn success_with_sign_in_words_on_stderr_is_not_a_failure() {
    let rig = Rig::new();
    let s = aws_standin(&rig)
        .stdout("{}")
        .stderr("note: token expired soon, aws login advised")
        .write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    assert_eq!(rig.runner().call(&call(Output::Json, &[])), Ok(json!({})));
}

#[test]
fn ignore_never_fails_on_the_exit_code() {
    for (code, err) in [(1, "boom"), (255, "The token has expired"), (3, "")] {
        let rig = Rig::new();
        let s = aws_standin(&rig).exit(code).stderr(err).write();
        rig.env.set("AGENTSPACE_AWS_CMD", s.value());
        assert_eq!(
            rig.runner().call(&call(Output::Ignore, &[])),
            Ok(serde_json::Value::Null),
            "exit {code}"
        );
        assert!(s.ran());
    }
}

#[test]
fn pass_through_still_scans_the_error_text() {
    let rig = Rig::new();
    let s = aws_standin(&rig)
        .exit(255)
        .stderr("Error: Your session has expired")
        .write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    assert_eq!(
        rig.runner().call(&call(Output::PassThrough, &[])),
        Err(AwsError::SignIn)
    );
}

#[test]
fn pass_through_failure_without_sign_in_words_keeps_the_cli_text() {
    let rig = Rig::new();
    let s = aws_standin(&rig)
        .exit(1)
        .stdout("Waiting for changeset...\n")
        .stderr("Failed to create the changeset: Waiter ChangeSetCreateComplete failed\n")
        .write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    assert_eq!(
        rig.runner().call(&call(Output::PassThrough, &[])),
        Err(AwsError::Failed(
            "Failed to create the changeset: Waiter ChangeSetCreateComplete failed".to_string()
        ))
    );
}

// ---------------------------------------------------------------- Checked (operator.md, The AWS side)

#[test]
fn checked_call_passes_region_and_output_json_in_fb07_order() {
    let rig = Rig::new();
    let s = aws_standin(&rig).stdout("{}").write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    let own = ["--instance-ids", "i-1234567890abcdef0"];
    rig.runner().call(&call(Output::Checked, &own)).unwrap();
    assert_eq!(
        s.args(),
        [
            "ec2",
            "describe-instances",
            "--region",
            "us-west-2",
            "--output",
            "json",
            "--instance-ids",
            "i-1234567890abcdef0"
        ]
    );
}

#[test]
fn checked_call_never_reads_the_answer_whatever_it_is() {
    // Exit 0 with an answer that is not JSON, empty, or JSON: all succeed, none is parsed.
    for text in ["this is not json", "", "{\"a\": 1}", "[1,"] {
        let rig = Rig::new();
        let s = aws_standin(&rig).stdout(text).write();
        rig.env.set("AGENTSPACE_AWS_CMD", s.value());
        assert_eq!(
            rig.runner().call(&call(Output::Checked, &[])),
            Ok(serde_json::Value::Null),
            "{text:?}"
        );
        assert!(s.ran());
    }
}

#[test]
fn checked_call_fails_on_a_non_zero_exit_with_the_clis_text() {
    let rig = Rig::new();
    let s = aws_standin(&rig)
        .exit(254)
        .stdout("not json")
        .stderr("\n An error occurred (UnauthorizedOperation): no \n")
        .write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    assert_eq!(
        rig.runner().call(&call(Output::Checked, &[])),
        Err(AwsError::Failed(
            "An error occurred (UnauthorizedOperation): no".to_string()
        ))
    );
}

#[test]
fn checked_call_failing_with_sign_in_words_is_the_sign_in_failure() {
    let rig = Rig::new();
    let s = aws_standin(&rig)
        .exit(255)
        .stderr("The SSO session has expired")
        .write();
    rig.env.set("AGENTSPACE_AWS_CMD", s.value());
    assert_eq!(
        rig.runner().call(&call(Output::Checked, &[])),
        Err(AwsError::SignIn)
    );
}

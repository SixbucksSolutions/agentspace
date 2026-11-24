//! `deploy`'s passthrough of the AWS CLI's `cloudformation deploy` output, through the built
//! program and its `main` (docs/operator.md: deploy, step 4; Streams; The test seam). Defect
//! #0139: the real AWS CLI writes progress lines to stdout while it runs, and the installed
//! operator then never returned, because `main` held the stdout lock for the whole run while the
//! passthrough's copy thread waited on that lock. The sync crate's own tests run `deploy` in a
//! child of their test binary, whose stdout nothing holds, so only a run through `main` shows it.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each run has a deadline: a run that does
//! not end within it is stopped, and the test fails saying that the operator did not end.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;
use std::fs;
use std::time::Duration;

/// Well past what a run takes (under 2 seconds here) even beside a parallel workspace build, and
/// the rig's own limit for a run: #0139's hang never ends, so any limit shows it (#0189).
const DEADLINE: Duration = Duration::from_secs(90);

const DEPLOYING: &str = "Deploying stack 'agentbox' in us-west-2...";

/// What the real CLI prints to stdout during a deploy that has nothing to change.
const CLI_STDOUT: &str = "\nWaiting for changeset to be created..\n\nNo changes to deploy. Stack agentbox is up to date\n";

/// A rig with `deploy`'s two inputs in its home, and a stack whose output names [`ID`].
fn deploy_rig() -> Rig {
    let rig = Rig::standard();
    fs::create_dir_all(rig.profile.join(".ssh")).expect("the .ssh folder");
    fs::write(
        rig.profile.join(".ssh").join("id_ed25519.pub"),
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMP user@example\r\n",
    )
    .expect("the public key");
    fs::create_dir_all(rig.profile.join(".agentspace")).expect("the state folder");
    fs::write(
        rig.profile
            .join(".agentspace")
            .join("stack-parameters.json"),
        r#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE"}}"#,
    )
    .expect("the stack parameters");
    rig
}

#[test]
fn a_deploy_whose_cli_writes_to_stdout_passes_it_through_and_goes_on_to_step_5() {
    let rig = deploy_rig();
    rig.answer(
        "cloudformation-deploy",
        None,
        Some(CLI_STDOUT),
        Some("a line on the CLI's stderr\n"),
        Some(0),
    );
    let out = rig.op(&["deploy"]).timeout(DEADLINE).run();

    assert_eq!(
        out.out(),
        CLI_STDOUT,
        "the CLI's stdout MUST reach the operator's stdout as written"
    );
    let lines = out.err_lines();
    assert_eq!(
        lines.first().map(String::as_str),
        Some(DEPLOYING),
        "step 3's line first; stderr was {:?}",
        out.err()
    );
    assert!(
        lines.iter().any(|l| l == "a line on the CLI's stderr"),
        "the CLI's stderr MUST reach the operator's stderr; stderr was {:?}",
        out.err()
    );
    let instance = format!("Instance: {ID}");
    assert!(
        lines.contains(&instance),
        "deploy MUST go on to step 5 and print {instance:?}; stderr was {:?}",
        out.err()
    );
    // Step 6's sync fails in the rig's home, which holds no CA, so the exit code is not 0; what
    // this test holds is that the run ended and reached step 5.
    assert_eq!(
        rig.keys(),
        ["cloudformation-deploy", "cloudformation-describe-stacks"],
        "deploy's two AWS calls, in order, and nothing of sync's after its failure"
    );
}

#[test]
fn a_failed_deploy_whose_cli_wrote_to_stdout_ends_with_deploys_failure() {
    let rig = deploy_rig();
    rig.answer(
        "cloudformation-deploy",
        None,
        Some("\nWaiting for changeset to be created..\n"),
        Some("An error occurred (ValidationError) when calling the CreateChangeSet operation\n"),
        Some(255),
    );
    let out = rig.op(&["deploy"]).timeout(DEADLINE).run();

    assert_eq!(out.code, Some(1), "exit code; stderr was {:?}", out.err());
    assert_eq!(
        out.out(),
        "\nWaiting for changeset to be created..\n",
        "the CLI's stdout MUST reach the operator's stdout as written"
    );
    assert_eq!(
        out.err_lines().last().map(String::as_str),
        Some("agentspace: Stack deploy failed; see the stack events in the console."),
        "the last stderr line; stderr was {:?}",
        out.err()
    );
    assert_eq!(
        rig.keys(),
        ["cloudformation-deploy"],
        "no call after the failed deploy"
    );
}

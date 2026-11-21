//! `deploy` (docs/operator.md, deploy steps 1 to 6; AWS failures; Calls by command, The stack,
//! the host registry in Laptop files and The operator's folder, one per command; docs/state.md, Files in the checkout, the stack
//! parameters file).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs `deploy` in a child of the
//! test binary under a throwaway home, with stand-ins for the AWS CLI, ssh, scp and go.
#![cfg(windows)]

mod common;

use agentspace_operator_api::{CallerFailure, SIGN_IN_MESSAGE};
use common::expect::*;
use common::*;
use std::fs;
use std::path::PathBuf;

fn template_path(rig: &Rig) -> PathBuf {
    rig.home
        .join(".agentspace")
        .join("operator")
        .join("agentspace.yaml")
}

/// The AWS CLI's `cloudformation deploy` arguments for the box. From v0.07 the template is in the
/// command's own folder, `operator\<hex>\agentspace.yaml`, whose `<hex>` only the call knows, so
/// this reads the template's path from the deploy call the stand-in logged, checks that it is
/// there, and expects it in that place (operator.md, Calls by command; The operator's folder, one
/// per command).
fn deploy_args(rig: &Rig, params: &[&str]) -> Vec<String> {
    let log = rig.log();
    let actual: Vec<String> = log
        .calls("aws")
        .into_iter()
        .find(|c| c.args.get(1).map(String::as_str) == Some("deploy"))
        .map(|c| c.args.clone())
        .unwrap_or_default();
    let template = match actual.iter().position(|a| a == "--template-file") {
        Some(i) if i + 1 < actual.len() => actual[i + 1].clone(),
        _ => String::new(),
    };
    let in_place = resolve(rig, &template, Folder::PerCommand, "The template file");
    rig.same_path(
        "The template file named to the AWS CLI",
        &in_place,
        &template_path(rig),
    );
    let mut v = svec(&[
        "cloudformation",
        "deploy",
        "--region",
        "us-west-2",
        "--stack-name",
        "agentbox",
        "--template-file",
    ]);
    v.push(template);
    v.extend(svec(&[
        "--capabilities",
        "CAPABILITY_IAM",
        "--no-fail-on-empty-changeset",
        "--tags",
        "project=agentbox",
        "--parameter-overrides",
    ]));
    v.push(format!("SshPublicKey={KEY_LINE}"));
    v.extend(params.iter().map(|s| s.to_string()));
    v
}

/// Nothing at all was started, so AWS was never called.
fn nothing_started(rig: &Rig, what: &str) {
    rig.check(
        rig.log().all_calls().is_empty(),
        &format!("{what}: no program MUST have been started"),
    );
}

#[test]
fn deploy_makes_its_calls_in_order_and_runs_sync_last() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.rule(
        "aws",
        0,
        "cloudformation deploy",
        0,
        "deploy stand-in output\\n",
        "",
    );
    let Some(out) = rig
        .run(|rig| {
            let fp = fingerprint(rig);
            let say = rig.say();
            rig.succeeded(rig.do_deploy(&say), "deploy");
            let log = rig.log();

            // The AWS calls, in order, with their arguments (Calls by command; FB-07).
            let aws = log.calls("aws");
            rig.check(aws.len() == 4, &format!("deploy and sync MUST make 4 AWS calls here, and made {}", aws.len()));
            rig.same_list(
                "The cloudformation deploy call",
                &aws[0].args,
                &deploy_args(
                    rig,
                    &["SubnetId=SUBNET_VALUE", "VpcId=VPC_VALUE"],
                ),
            );
            rig.same_list(
                "The describe-stacks call",
                &aws[1].args,
                &svec(&[
                    "cloudformation",
                    "describe-stacks",
                    "--region",
                    "us-west-2",
                    "--output",
                    "json",
                    "--stack-name",
                    "agentbox",
                ]),
            );
            // sync starts the box named by the new stack output, not by the old one the record held.
            rig.same_list("The first call of sync's start-and-wait", &aws[2].args, &describe_instances_args(ID2));

            // The template is written before the call that reads it, and is the repository's.
            let snap = log.file_snap("aws", 1);
            let want = fs::read(repo_root().join("infra").join("agentspace.yaml")).unwrap();
            rig.check(
                snap.len() == 1 && snap[0].1 == want.len() as u64 && snap[0].2 == fnv(&want),
                "The template file MUST exist, byte for byte the repository's infra/agentspace.yaml, when the deploy call starts",
            );
            let in_place = resolve(rig, &snap[0].0, Folder::PerCommand, "The template file");
            rig.same_path("The template file named to the AWS CLI", &in_place, &template_path(rig));

            // The registry: the box's record holds the new ID, and nothing else of it changed. The
            // retired cache is neither read nor written (operator.md, Laptop files).
            let record = rig.record("swimcoach");
            rig.check(
                record.instance_id.as_deref() == Some(ID2)
                    && record.state.as_str() == "ready"
                    && record.last_attempt_at.is_none(),
                "The box's record MUST hold the new instance ID, and deploy MUST change nothing else in it",
            );
            rig.check(
                !rig.home_path(".agentspace/instance-id.json").exists(),
                "No v0.07 program writes the retired instance-ID cache",
            );

            // The messages, and where they stand among the calls.
            let pos = |pred: &dyn Fn(&Item) -> bool| log.items.iter().position(pred);
            let deploying = pos(&|i| matches!(i, Item::Say(k, t) if k == "progress" && t == "Deploying stack 'agentbox' in us-west-2..."));
            let deploy_call = pos(&|i| matches!(i, Item::Call(r) if r.name == "aws" && r.args.get(1).map(|s| s.as_str()) == Some("deploy")));
            let stacks_call = pos(&|i| matches!(i, Item::Call(r) if r.name == "aws" && r.args.get(1).map(|s| s.as_str()) == Some("describe-stacks")));
            let instance = pos(&|i| matches!(i, Item::Say(k, t) if k == "progress" && *t == format!("Instance: {ID2}")));
            let first_sync = pos(&|i| matches!(i, Item::Say(k, t) if k == "progress" && t.starts_with("Copying box/system")));
            rig.check(
                matches!((deploying, deploy_call, stacks_call, instance, first_sync),
                    (Some(a), Some(b), Some(c), Some(d), Some(e)) if a < b && b < c && c < d && d < e),
                "deploy MUST print `Deploying stack 'agentbox' in us-west-2...`, call cloudformation deploy, read the stack's output, print `Instance: <id>`, then run sync",
            );

            rig.same_list(
                "sync's messages and calls",
                &timeline_of(&log, |_| true),
                &expected_timeline(&fp, false, false),
            );
            check_calls(rig, &log, false, false);
            check_environment(rig, &log);
        })
        else { return };
    assert!(
        out.stdout.contains("deploy stand-in output"),
        "cloudformation deploy's output MUST be passed through"
    );
}

#[test]
fn deploy_without_parameters_passes_only_the_key() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(
        ".agentspace/stack-parameters.json",
        b"{\"version\":0,\"parameters\":{}}",
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        let log = rig.log();
        rig.same_list(
            "The cloudformation deploy call",
            &log.calls("aws")[0].args,
            &deploy_args(rig, &[]),
        );
    });
}

#[test]
fn deploy_accepts_a_byte_order_mark_and_json_whitespace_in_the_parameters_file() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    let mut text = vec![0xEF, 0xBB, 0xBF];
    text.extend_from_slice(
        b"\r\n  {\r\n \"version\" : 0 ,\r\n \"parameters\" : { \"SubnetId\" : \"SUBNET_VALUE\" }\r\n}\r\n",
    );
    rig.write_home(".agentspace/stack-parameters.json", &text);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        rig.same_list(
            "The cloudformation deploy call",
            &rig.log().calls("aws")[0].args,
            &deploy_args(rig, &["SubnetId=SUBNET_VALUE"]),
        );
    });
}

#[test]
fn deploy_sends_its_own_template_and_leaves_what_a_v0_06_operator_wrote_alone() {
    // The template a v0.06 operator left in the operator's folder is never sent: each command writes
    // its own, the embedded one, in a folder of its own (operator.md, The operator's folder, one per
    // command: what a v0.06 operator wrote is left).
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(
        ".agentspace/operator/agentspace.yaml",
        b"a template somebody edited by hand",
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        let want = fs::read(repo_root().join("infra").join("agentspace.yaml")).unwrap();
        let snap = rig.log().file_snap("aws", 1);
        rig.check(
            snap.len() == 1 && snap[0].2 == fnv(&want),
            "The template sent MUST be the embedded one at each deploy, whatever a v0.06 operator left",
        );
        rig.check(
            fs::read(template_path(rig)).unwrap() == b"a template somebody edited by hand",
            "What a v0.06 operator left in the operator's folder MUST be left alone",
        );
    });
}

// ---------------------------------------------------------------- Step 1: the public key

#[test]
fn step_1_a_missing_empty_or_unreadable_public_key_fails_naming_the_file() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.remove_home(".ssh/id_ed25519.pub");
    rig.run(|rig| {
        // The three messages of operator.md, deploy, step 1 (architect, 2026-11-19, for
        // E0.06-T03-S02), each with the file's full path.
        let key = rig.home.join(".ssh").join("id_ed25519.pub");
        let missing = format!("agentspace: {} does not exist.", key.display());
        let empty = format!("agentspace: {} is empty.", key.display());
        let unreadable = format!("agentspace: Cannot read {}.", key.display());
        let run_case = |label: &str, want: &str| {
            let say = rig.say();
            let f = rig.failure(rig.do_deploy(&say), label);
            rig.same(label, &f.message, want);
            nothing_started(rig, label);
        };
        run_case("A missing public key", &missing);
        fs::write(&key, b"").unwrap();
        run_case("An empty public key", &empty);
        // A file of whitespace alone is empty, since the key is read trimmed.
        fs::write(&key, b" \r\n\t\r\n").unwrap();
        run_case("A whitespace-only public key", &empty);
        fs::write(&key, b"\n").unwrap();
        run_case("A public key that is one line ending", &empty);
        fs::remove_file(&key).unwrap();
        fs::create_dir_all(&key).unwrap();
        run_case("A public key that cannot be read", &unreadable);
    });
}

#[test]
fn step_1_the_public_key_is_passed_trimmed() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(
        ".ssh/id_ed25519.pub",
        format!("  \t{KEY_LINE} \r\n\r\n").as_bytes(),
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        rig.same_list(
            "The cloudformation deploy call",
            &rig.log().calls("aws")[0].args,
            &deploy_args(rig, &["SubnetId=SUBNET_VALUE", "VpcId=VPC_VALUE"]),
        );
    });
}

#[test]
fn step_1_the_public_key_is_checked_before_the_parameters_file() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.remove_home(".ssh/id_ed25519.pub");
    rig.remove_home(".agentspace/stack-parameters.json");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        rig.check(
            f.message.contains("id_ed25519.pub") && !f.message.contains("stack-parameters"),
            "The public key (step 1) MUST be checked before the parameters file (step 2)",
        );
    });
}

// ---------------------------------------------------------------- Step 2: the parameters file

#[test]
fn step_2_a_missing_parameters_file_names_the_example() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.remove_home(".agentspace/stack-parameters.json");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        let path = rig.home.join(".agentspace").join("stack-parameters.json");
        rig.same(
            "The failure message",
            &f.message,
            &format!(
                "agentspace: Missing {}; copy infra\\stack-parameters.example.json and fill it in.",
                path.display()
            ),
        );
        nothing_started(rig, "A missing parameters file");
    });
}

#[test]
fn step_2_a_parameters_file_that_is_not_valid_fails_before_aws_is_called() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.run(|rig| {
        let path = rig.home.join(".agentspace").join("stack-parameters.json");
        let prefix = format!("agentspace: {} is not a valid stack parameters file: ", path.display());
        let cases: &[(&str, &[u8])] = &[
            ("An empty file", b""),
            ("Text that is not JSON", b"SubnetId=SUBNET_VALUE\n"),
            ("A JSON array", b"[]"),
            ("A truncated object", b"{\"version\":0,\"parameters\":{"),
            ("No version", b"{\"parameters\":{}}"),
            ("A version that is not 0", b"{\"version\":1,\"parameters\":{}}"),
            ("No parameters member", b"{\"version\":0}"),
            ("Parameters that are not an object", b"{\"version\":0,\"parameters\":[]}"),
            ("A value that is not a string", b"{\"version\":0,\"parameters\":{\"A\":1}}"),
            ("An empty value", b"{\"version\":0,\"parameters\":{\"A\":\"\"}}"),
            ("A value with a space around it", b"{\"version\":0,\"parameters\":{\"A\":\" x\"}}"),
            ("A name that starts with a digit", b"{\"version\":0,\"parameters\":{\"1a\":\"x\"}}"),
            ("A member the schema does not know", b"{\"version\":0,\"parameters\":{},\"extra\":1}"),
        ];
        for (label, bytes) in cases {
            fs::write(&path, bytes).unwrap();
            let say = rig.say();
            let f = rig.failure(rig.do_deploy(&say), label);
            let reason = f.message.strip_prefix(&prefix);
            rig.check(
                matches!(reason, Some(r) if r.len() > 1 && r.ends_with('.') && !r.contains('\n') && !r.contains('\r')),
                &format!("{label}: the failure MUST be `agentspace: <path> is not a valid stack parameters file: <reason>.` on one line; it was `{}`", f.message),
            );
            nothing_started(rig, label);
        }
        // A file that cannot be opened: a folder where the file has to be.
        fs::remove_file(&path).unwrap();
        fs::create_dir_all(&path).unwrap();
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "A folder in place of the file");
        rig.check(
            f.message.starts_with(&prefix) && f.message.ends_with('.') && !f.message.contains('\n'),
            "A parameters file that cannot be opened MUST fail with the same message form",
        );
        nothing_started(rig, "A folder in place of the file");
    });
}

/// Runs `deploy` over a parameters file that MUST NOT be taken as valid, and checks the message's
/// form, that its reason names `member`, and that AWS was never called.
fn not_valid_naming(bytes: &[u8], member: &str, label: &str) {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(".agentspace/stack-parameters.json", bytes);
    let (member, label) = (member.to_string(), label.to_string());
    rig.run(move |rig| {
        let path = rig.home.join(".agentspace").join("stack-parameters.json");
        let prefix = format!(
            "agentspace: {} is not a valid stack parameters file: ",
            path.display()
        );
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), &label);
        let reason = f.message.strip_prefix(&prefix);
        rig.check(
            matches!(reason, Some(r) if r.contains(member.as_str()) && r.ends_with('.') && !r.contains('\n') && !r.contains('\r')),
            &format!("{label}: the failure MUST be `agentspace: <path> is not a valid stack parameters file: <reason>.` on one line, the reason naming {member}; it was `{}`", f.message),
        );
        nothing_started(rig, &label);
        rig.check(
            say.progress_lines().is_empty(),
            &format!("{label}: deploy MUST fail before it prints `Deploying`"),
        );
    });
}

#[test]
fn step_2_a_parameters_file_that_names_ssh_public_key_is_not_valid() {
    // deploy sets SshPublicKey itself from id_ed25519.pub; a file that could override it would put
    // another key on the box with no word said (operator.md, deploy, step 2).
    not_valid_naming(
        br#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","SshPublicKey":"ssh-ed25519 AAAAOTHERKEY other@example","VpcId":"VPC_VALUE"}}"#,
        "SshPublicKey",
        "A file that names SshPublicKey",
    );
}

#[test]
fn step_2_a_parameters_file_that_names_only_ssh_public_key_is_not_valid() {
    not_valid_naming(
        br#"{"version":0,"parameters":{"SshPublicKey":"ssh-ed25519 AAAAOTHERKEY other@example"}}"#,
        "SshPublicKey",
        "A file whose one parameter is SshPublicKey",
    );
}

#[test]
fn step_2_a_parameters_file_that_names_a_parameter_twice_is_not_valid() {
    // A JSON reader that keeps the last of two members would pass SECOND here and say nothing.
    not_valid_naming(
        br#"{"version":0,"parameters":{"SubnetId":"FIRST","VpcId":"VPC_VALUE","SubnetId":"SECOND"}}"#,
        "SubnetId",
        "A file that names SubnetId twice",
    );
}

#[test]
fn step_2_a_parameters_file_that_names_a_top_member_twice_is_not_valid() {
    not_valid_naming(
        br#"{"version":0,"parameters":{"SubnetId":"FIRST"},"parameters":{"SubnetId":"SECOND"}}"#,
        "parameters",
        "A file that names parameters twice",
    );
}

#[test]
fn step_2_a_template_that_cannot_be_written_fails_before_aws_is_called() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    // A file where the operator's folder has to be: the first folder that cannot be made is named
    // itself, never one below it, and nor is a command's own folder (operator.md, What the
    // prototype took from its checkout, FA-16).
    rig.write_home(".agentspace/operator", b"a file where a folder belongs");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        let operator = rig.home.join(".agentspace").join("operator");
        rig.same(
            "The failure message",
            &f.message,
            &format!("agentspace: Cannot write {}.", operator.display()),
        );
        nothing_started(rig, "An unwritable template");
    });
}

// ---------------------------------------------------------------- Step 4: the stack deploy

fn sign_in_case(stderr: &str) {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.rule("aws", 0, "cloudformation deploy", 255, "", stderr);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        rig.same("The failure message", &f.message, SIGN_IN_MESSAGE);
        rig.check(
            f.caller == CallerFailure::Same,
            "The sign-in message is the caller's line too",
        );
        let log = rig.log();
        rig.check(
            log.all_calls().len() == 1,
            "Nothing may follow a failed stack deploy: no describe-stacks, no sync",
        );
    });
}

#[test]
fn step_4_an_expired_sign_in_fails_with_the_sign_in_message_not_the_deploy_message() {
    sign_in_case(
        "Unable to locate credentials. You can configure credentials by running \"aws configure\".\\n",
    );
}

#[test]
fn step_4_the_sign_in_test_matches_expired_without_regard_to_case() {
    sign_in_case("An error occurred: The SSO session has EXPIRED\\n");
}

#[test]
fn step_4_the_sign_in_test_matches_aws_login() {
    sign_in_case("Error loading credentials. Run Aws Login and retry\\n");
}

#[test]
fn step_4_another_failed_deploy_fails_with_its_own_message_and_its_output_passes_through() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.rule(
        "aws",
        0,
        "cloudformation deploy",
        1,
        "",
        "stack failed stand-in line\\n",
    );
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        rig.same(
            "The failure message",
            &f.message,
            "agentspace: Stack deploy failed; see the stack events in the console.",
        );
        rig.check(
            rig.log().all_calls().len() == 1,
            "Nothing may follow a failed stack deploy",
        );
    }) else {
        return;
    };
    assert!(
        out.stderr.contains("stack failed stand-in line"),
        "The deploy's error output MUST be passed through"
    );
}

// ---------------------------------------------------------------- The pinned deploy

#[test]
fn the_pinned_deploy_is_that_deploy_when_no_go_is_on_path() {
    // The pinned entry point itself, where no `go` can be found (the rig refuses otherwise): the
    // same calls in the same order, ending in its sync, which skips the build.
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_pinned_deploy(&say), "The pinned deploy");
        let log = rig.log();
        let aws = log.calls("aws");
        rig.check(
            aws.len() == 4,
            &format!(
                "The pinned deploy and its sync MUST make 4 AWS calls here, and made {}",
                aws.len()
            ),
        );
        rig.same_list(
            "The cloudformation deploy call",
            &aws[0].args,
            &deploy_args(rig, &["SubnetId=SUBNET_VALUE", "VpcId=VPC_VALUE"]),
        );
        rig.same_list(
            "The first call of its sync's start-and-wait",
            &aws[2].args,
            &describe_instances_args(ID2),
        );
        rig.check(
            rig.record("swimcoach").instance_id.as_deref() == Some(ID2),
            "The box's record MUST hold the new ID",
        );
        rig.same_list(
            "Its sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, false, false),
        );
        check_calls(rig, &log, false, false);
        check_environment(rig, &log);
    });
}

// ---------------------------------------------------------------- Step 5: the instance ID

#[test]
fn step_5_a_stack_with_no_instance_id_output_fails_and_leaves_the_record_with_none() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.rule(
        "aws",
        0,
        "describe-stacks",
        0,
        "{\"Stacks\":[{\"Outputs\":[]}]}",
        "",
    );
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        rig.same(
            "The failure message",
            &f.message,
            "agentspace: Stack 'agentbox' has no InstanceId output.",
        );
        rig.check(
            rig.record("swimcoach").instance_id.is_none(),
            "No instance ID may be written, and the old one MUST be gone (it is removed first)",
        );
        rig.check(
            rig.log().remote_calls().is_empty() && rig.log().calls("aws").len() == 2,
            "sync MUST NOT run after a failed read of the stack's output",
        );
    });
}

#[test]
fn step_5_a_failed_read_of_the_stack_fails_and_leaves_the_record_with_none() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.rule(
        "aws",
        0,
        "describe-stacks",
        1,
        "",
        "describe failed stand-in line\\n",
    );
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        rig.same(
            "The failure message",
            &f.message,
            "agentspace: describe failed stand-in line",
        );
        rig.check(
            rig.record("swimcoach").instance_id.is_none(),
            "If the read fails, deploy fails with no instance ID in the record",
        );
    });
}

// ---------------------------------------------------------------- Step 6: sync

#[test]
fn step_6_deploy_fails_when_its_sync_fails_and_the_instance_id_stays_in_the_record() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.rule("ssh", 1, "", 1, "", "");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        rig.same(
            "deploy's failure is its sync's own",
            &f.message,
            "agentspace: Removing the old staging folder on the instance failed.",
        );
        rig.check(
            rig.record("swimcoach").instance_id.as_deref() == Some(ID2),
            "The stack was deployed, so its instance ID stays in the record",
        );
    });
}

#[test]
fn step_6_deploy_fails_with_the_ca_check_when_there_is_no_ca_after_the_stack_is_deployed() {
    let rig = Rig::new(false);
    rig.with_deploy_inputs();
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        let want = match agentspace_ca::check_for_sync(&rig.ctx(), &ca_env()) {
            Err(w) => w,
            Ok(_) => panic!("The CA check passed, so this case is not a CA failure"),
        };
        rig.same("deploy's failure is its sync's CA check", &f.message, &want.message);
        let log = rig.log();
        rig.check(
            log.calls("aws").len() == 2 && log.remote_calls().is_empty(),
            "The stack is deployed and its output read, and sync then stops at the CA before the box is touched",
        );
    });
}

#[test]
fn step_6_a_sign_in_that_expires_in_syncs_start_and_wait_reaches_deploy() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.rule(
        "aws",
        0,
        "describe-instances",
        255,
        "",
        "Unable to locate credentials\\n",
    );
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        rig.same("The failure message", &f.message, SIGN_IN_MESSAGE);
    });
}

#[test]
fn step_6_deploy_quiet_quiets_its_sync_step_too() {
    // -Quiet is the Say that deploy was given (the pin); deploy MUST hand sync that same one, and
    // neither may print on its own.
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    let Some(out) = rig.run(|rig| {
        let say = rig.quiet_say();
        rig.succeeded(rig.do_deploy(&say), "deploy -Quiet");
    }) else {
        return;
    };
    for text in [
        "Deploying stack",
        "Instance: ",
        "Copying box/system",
        "Copying the agentspace SSH CA",
        "Skipping the localswim CLI",
    ] {
        assert!(
            !out.stdout.contains(text) && !out.stderr.contains(text),
            "deploy -Quiet MUST print none of its own or its sync's progress: `{text}` reached the streams"
        );
    }
}

#[test]
fn step_6_deploy_gives_sync_the_say_it_was_given() {
    // The same lines that deploy -Quiet suppresses MUST reach a Say that is not quiet, which shows
    // that they went through the Say deploy was handed.
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        let lines = say.progress_lines();
        for want in [
            "Deploying stack 'agentbox' in us-west-2...".to_string(),
            format!("Instance: {ID2}"),
            "Copying box/system to the instance...".to_string(),
        ] {
            rig.check(
                lines.contains(&want),
                &format!("The Say MUST be given `{want}`"),
            );
        }
    });
}

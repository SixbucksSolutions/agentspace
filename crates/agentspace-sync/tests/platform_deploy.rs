//! `deploy -Host <project> -Platform <platform>`: the image a new stack is given, the parameters
//! after it, and the type last (docs/operator.md, deploy, steps 3 and 4 and "A stack it creates";
//! The platform seam, the image, instance-type and creation-parameter rows; Calls by command; AWS
//! failures; Messages; The stack; D94, D97; E0.12-T03-S03, #0256, pins P41, P43, P59 of v0.12-T01
//! and P15, P16 of v0.12-T02).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `deploy_keyed_using` in a child of the test binary, under a throwaway home with a throwaway CA,
//! with stand-ins for the AWS CLI, ssh, scp and go.
//!
//! - **A new `linux/amd64` or `windows/amd64` stack reads its image parameter once**, by the
//!   parameter's name, after the existence check and before `cloudformation deploy`, and passes
//!   `ImageId=<image>` after `CaPublicKey=<CA key>`; `windows/amd64` then passes `RootVolumeGiB=30`
//!   and `HostOs=windows`; `InstanceType` is last, `-InstanceType`'s value or `m8a.xlarge`.
//! - **An update passes no `ImageId`**, makes no `ssm` call, and passes `InstanceType` alone when
//!   `-InstanceType` is given. A `linux/arm64` host passes none of these, as before v0.12.
//! - **A failed or malformed read fails `deploy`** before any call that changes anything, and the
//!   resolved image ID is in no message.
//!
//! The stack's `deploy` of a Windows host goes on to `sync`, whose Windows rows are
//! E0.12-T03-S06's: a test of the deploy's own parameters accepts either outcome of that last step,
//! and `a_windows_deploy_goes_on_to_sync` is the one that does not.
#![cfg(windows)]

mod common;
mod hostcalls;
mod platformcalls;
mod waitcalls;

use agentspace_operator_api::SIGN_IN_MESSAGE;
use common::hosts::PROJECT;
use common::*;
use hostcalls::setup::*;
use platformcalls::*;
use waitcalls::SIGN_IN_TEXT;

/// The overrides of a stack `deploy` creates on `platform` (operator.md, deploy, "A stack it
/// creates"): the key, the two members of the file, `HostName` and `CaPublicKey`, then the image,
/// then the platform's other creation parameters, then `InstanceType` last.
fn creation(rig: &Rig, platform: &str, instance_type: Option<&str>) -> Vec<String> {
    let mut v = vec![
        format!("SshPublicKey={KEY_LINE}"),
        "SubnetId=SUBNET_VALUE".to_string(),
        "VpcId=VPC_VALUE".to_string(),
        format!("HostName={}", stack(platform)),
        format!("CaPublicKey={}", ca_key(rig)),
    ];
    if platform != ARM64 {
        v.push(format!("ImageId={IMAGE}"));
    }
    if platform == WINDOWS {
        v.push("RootVolumeGiB=30".to_string());
        v.push("HostOs=windows".to_string());
    }
    match (platform, instance_type) {
        (_, Some(t)) => v.push(format!("InstanceType={t}")),
        (ARM64, None) => {}
        (_, None) => v.push("InstanceType=m8a.xlarge".to_string()),
    }
    v
}

/// A rig for a new stack on `platform`: the host's record `creating`, no stack, the image
/// parameter answered, and the stack made once deployed, holding `type` for its read-back.
fn new_stack_rig(platform: &'static str, stack_type: &'static str) -> Rig {
    let rig = plat_rig(false, &[PRec::new(platform, "creating")]);
    rig.stack_missing_typed(&stack(platform), stack_type);
    if platform != ARM64 {
        rig.image_is(IMAGE);
    }
    rig.rule(
        "aws",
        0,
        "cloudformation deploy",
        0,
        "deploy stand-in output\\n",
        "",
    );
    rig
}

/// Runs a new stack's deploy on `platform` and checks what `deploy` asked of AWS up to the stack's
/// creation: the existence check, the image's one read for a platform that has one, the
/// `cloudformation deploy` and the read-back, in that order.
fn creates(platform: &'static str, instance_type: Option<&'static str>) {
    let default_type = if platform == ARM64 {
        "m9g.xlarge"
    } else {
        "m8a.xlarge"
    };
    let rig = new_stack_rig(platform, instance_type.unwrap_or(default_type));
    rig.run(move |rig| {
        let stack = stack(platform);
        let say = rig.say();
        let result = rig.do_deploy_keyed(&say, &key(platform), instance_type);
        let ended = result.as_ref().err().map(|f| f.message.clone());
        if platform != WINDOWS {
            rig.succeeded(result, "deploy -Host -Platform");
        }
        let log = rig.log();
        let aws = log.calls("aws");
        rig.check(
            aws.len() >= 4,
            &format!("deploy MUST make its calls to AWS; it made {} and ended with {ended:?}", aws.len()),
        );
        let reads = if platform == ARM64 { 0 } else { 1 };

        rig.same_list(
            "The existence check, the first AWS call",
            &aws[0].args,
            &describe_stacks_args(&stack),
        );
        let mut at = 1;
        if reads == 1 {
            rig.same_list(
                "The image's read, after the existence check and before cloudformation deploy",
                &aws[at].args,
                &get_parameter_args(parameter(platform)),
            );
            at += 1;
        }
        rig.check(
            aws.iter()
                .filter(|c| c.args.get(1).map(String::as_str) == Some("get-parameter"))
                .count()
                == reads,
            "The image parameter MUST be read once for a platform that has one, and never for linux/arm64",
        );
        let template = template_arg(&aws[at].args);
        rig.same_list(
            "The cloudformation deploy call: the overrides, each one argument, in order",
            &aws[at].args,
            &deploy_call_args(&stack, &template, &creation(rig, platform, instance_type)),
        );
        check_template(rig, &log, (at + 1) as u32, &template);
        rig.same_list(
            "The call that reads the instance ID and type back",
            &aws[at + 1].args,
            &describe_stacks_args(&stack),
        );
        let lines = say.progress_lines();
        rig.same(
            "The first message",
            &lines[0],
            &format!("Deploying stack '{stack}' in us-west-2..."),
        );

        // The host's own record, folder and lock: nothing of another platform's host is made.
        let host = host_name(platform);
        let record = rig.record_on(platform);
        rig.check(
            record.instance_id.as_deref() == Some(ID2)
                && record.instance_type.as_deref() == Some(instance_type.unwrap_or(default_type))
                && record.state.as_str() == "creating"
                && record.stack == stack,
            "deploy MUST record the instance ID and the type the stack reports in its own record, never a state",
        );
        rig.check(
            rig.home_path(&format!(".agentspace/hosts/{host}/provision.lock")).exists(),
            "deploy takes its host's own provision.lock, in the host's own folder",
        );
        let folders: Vec<String> = std::fs::read_dir(rig.home_path(".agentspace/hosts"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        rig.same_list("The hosts' folders", &folders, &[host]);
    });
}

#[test]
fn a_new_linux_amd64_stack_reads_its_image_and_passes_it_then_the_platforms_type() {
    creates(AMD64, None);
}

#[test]
fn a_new_windows_stack_passes_the_image_the_volume_the_os_and_then_the_platforms_type() {
    creates(WINDOWS, None);
}

#[test]
fn an_instance_type_given_replaces_the_platforms_type_and_stays_last_on_linux_amd64() {
    creates(AMD64, Some("m8a.2xlarge"));
}

#[test]
fn an_instance_type_given_replaces_the_platforms_type_and_stays_last_on_windows() {
    creates(WINDOWS, Some("m8a.4xlarge"));
}

#[test]
fn a_new_linux_arm64_stack_passes_none_of_these_as_before_v0_12() {
    creates(ARM64, None);
}

#[test]
fn a_new_linux_arm64_stack_given_a_type_passes_it_last_and_still_no_image() {
    creates(ARM64, Some("m9g.2xlarge"));
}

/// The file names InstanceType (m9g.8xlarge) and RootVolumeGiB (40); a new host takes only
/// SubnetId and VpcId from it, so neither reaches the stack, and windows' volume is 30.
fn the_file_sets_neither_volume_nor_type(platform: &'static str) {
    let rig = new_stack_rig(platform, "m8a.xlarge");
    rig.run(move |rig| {
        let say = rig.say();
        let _ = rig.do_deploy_keyed(&say, &key(platform), None);
        let log = rig.log();
        rig.check(
            log.calls("aws")
                .iter()
                .any(|c| c.args.get(1).map(String::as_str) == Some("deploy")),
            "Setup: the stack MUST have been created, or no call was checked",
        );
        for c in log.calls("aws") {
            rig.check(
                !c.args
                    .iter()
                    .any(|a| a.contains("m9g.8xlarge") || a == "RootVolumeGiB=40"),
                &format!(
                    "{platform}: the file's InstanceType and RootVolumeGiB MUST NOT reach AWS"
                ),
            );
        }
    });
}

#[test]
fn the_parameters_file_never_sets_a_new_linux_amd64_stacks_volume_or_type() {
    the_file_sets_neither_volume_nor_type(AMD64);
}

#[test]
fn the_parameters_file_never_sets_a_new_windows_stacks_volume_or_type() {
    the_file_sets_neither_volume_nor_type(WINDOWS);
}

#[test]
fn a_windows_deploy_goes_on_to_sync_over_the_hosts_alias() {
    // Depends on E0.12-T03-S06's rows: a Windows host's sync is the seam's.
    let rig = new_stack_rig(WINDOWS, "m8a.xlarge");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_keyed(&say, &key(WINDOWS), None),
            "deploy -Host -Platform windows/amd64",
        );
        let log = rig.log();
        let a = alias(WINDOWS);
        rig.check(
            !log.calls("ssh").is_empty() && log.calls("ssh").iter().all(|c| c.args.contains(&a)),
            "sync, deploy's step 6, MUST run over the Windows host's alias",
        );
        rig.same_list(
            "The first call of sync's start-and-wait, for the instance the stack made",
            &log.calls("aws")[4].args,
            &describe_instances_args(ID2),
        );
    });
}

// ---------------------------------------------------------------- An update

/// A host's stack that exists is updated: no `ssm` call, no parameter at all, and with
/// `-InstanceType` the one `InstanceType=<type>`.
fn updates(platform: &'static str, instance_type: Option<&'static str>) {
    let rig = plat_rig(false, &[PRec::new(platform, "ready").id(ID2)]);
    rig.remove_home(".ssh/id_ed25519.pub");
    rig.remove_home(".agentspace/stack-parameters.json");
    rig.stack_status("UPDATE_COMPLETE");
    rig.run(move |rig| {
        let stack = stack(platform);
        let say = rig.say();
        let result = rig.do_deploy_keyed(&say, &key(platform), instance_type);
        let ended = result.as_ref().err().map(|f| f.message.clone());
        if platform != WINDOWS {
            rig.succeeded(result, "deploy -Host -Platform");
        }
        let log = rig.log();
        let aws = log.calls("aws");
        rig.check(
            aws.len() >= 3,
            &format!(
                "deploy MUST make its calls to AWS; it made {} and ended with {ended:?}",
                aws.len()
            ),
        );
        rig.same_list(
            "The existence check",
            &aws[0].args,
            &describe_stacks_args(&stack),
        );
        let template = template_arg(&aws[1].args);
        let want_overrides: Vec<String> = instance_type
            .map(|t| vec![format!("InstanceType={t}")])
            .unwrap_or_default();
        rig.same_list(
            "The cloudformation deploy call of an update: no image, no volume, no OS",
            &aws[1].args,
            &deploy_call_args(&stack, &template, &want_overrides),
        );
        rig.check(
            !aws.iter().any(|c| {
                c.args.first().map(String::as_str) == Some("ssm")
                    && c.args.get(1).map(String::as_str) == Some("get-parameter")
            }),
            "An update MUST NOT read the image parameter: a host keeps the image it was made from",
        );
        rig.check(
            !aws.iter().any(|c| {
                c.args.iter().any(|a| {
                    a.starts_with("ImageId=")
                        || a.starts_with("HostOs=")
                        || a.starts_with("RootVolumeGiB=")
                })
            }),
            "An update MUST pass no ImageId, HostOs or RootVolumeGiB",
        );
    });
}

#[test]
fn a_linux_amd64_update_passes_no_image_and_makes_no_ssm_call() {
    updates(AMD64, None);
}

#[test]
fn a_windows_update_passes_no_image_volume_or_os() {
    updates(WINDOWS, None);
}

#[test]
fn a_linux_amd64_update_given_a_type_passes_that_type_alone() {
    updates(AMD64, Some("m8a.2xlarge"));
}

#[test]
fn a_windows_update_given_a_type_passes_that_type_alone() {
    updates(WINDOWS, Some("m8a.2xlarge"));
}

#[test]
fn a_linux_arm64_update_is_as_before_v0_12() {
    updates(ARM64, None);
}

// ---------------------------------------------------------------- A read that fails

/// One scripted answer of `ssm get-parameter`: its exit code, its output and its error text, and
/// the failure `deploy` MUST give for it.
struct Variant {
    code: i32,
    out: String,
    err: String,
    message: String,
}

const NO_VALUE: &str = "agentspace: Unexpected answer from ssm get-parameter: no Parameter.Value.";

fn answers(raw: &str) -> Variant {
    Variant {
        code: 0,
        out: raw.to_string(),
        err: String::new(),
        message: NO_VALUE.to_string(),
    }
}

fn value(v: &str) -> Variant {
    answers(&format!("{{\"Parameter\":{{\"Value\":\"{v}\"}}}}"))
}

/// A new stack's deploy, run once for each variant in turn, whose image read goes wrong each time:
/// the failure's message, nothing that changes anything started, the registry as it was, and the
/// value in no message. The stand-in answers the nth existence check and the nth read with the nth
/// variant, so one rig serves every run.
fn read_matrix(platform: &'static str, variants: Vec<Variant>) {
    // The record holds a type, which a deploy that got as far as step 4 would have removed.
    let rig = plat_rig(
        false,
        &[PRec::new(platform, "creating").id(ID).itype("m8a.xlarge")],
    );
    let missing = format!(
        "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id {} does not exist\\n",
        stack(platform)
    );
    for v in &variants {
        rig.seq("aws", "describe-stacks", 255, "", &missing);
        rig.seq("aws", "get-parameter", v.code, &v.out, &v.err);
    }
    rig.run(move |rig| {
        let before = rig.registry_text();
        for (n, v) in variants.iter().enumerate() {
            let at = rig.log().calls("aws").len();
            let say = rig.say();
            let f = rig.failure(
                rig.do_deploy_keyed(&say, &key(platform), None),
                &format!("deploy, variant {n}"),
            );
            rig.same(&format!("The failure of variant {n}"), &f.message, &v.message);
            let log = rig.log();
            let seen: Vec<String> = log.calls("aws")[at..]
                .iter()
                .map(|c| c.args.get(1).cloned().unwrap_or_default())
                .collect();
            rig.check(
                seen == ["describe-stacks", "get-parameter"],
                &format!("Variant {n}: only the existence check and the read MUST be asked of AWS: {seen:?}"),
            );
            rig.check(
                log.remote_calls().is_empty(),
                &format!("Variant {n}: no ssh, scp or go MUST have been started"),
            );
            rig.check(
                rig.registry_text() == before,
                &format!("Variant {n}: the registry MUST be as it was, with no type or ID removed"),
            );
            rig.check(
                !f.message.contains("ami-") && !say.events.borrow().iter().any(|(_, t)| t.contains("ami-")),
                &format!("Variant {n}: the image ID, or what was read for one, MUST be in no message"),
            );
        }
    });
}

#[test]
fn a_failed_read_fails_deploy_with_the_clis_own_text_on_linux_amd64() {
    let text = "An error occurred (ParameterNotFound) when calling the GetParameter operation: no such parameter";
    read_matrix(
        AMD64,
        vec![Variant {
            code: 254,
            out: String::new(),
            err: format!("{text}\\n"),
            message: format!("agentspace: {text}"),
        }],
    );
}

#[test]
fn a_failed_read_fails_deploy_with_the_clis_own_text_on_windows() {
    let text = "An error occurred (ParameterNotFound) when calling the GetParameter operation: no such parameter";
    read_matrix(
        WINDOWS,
        vec![Variant {
            code: 254,
            out: String::new(),
            err: format!("{text}\\n"),
            message: format!("agentspace: {text}"),
        }],
    );
}

#[test]
fn a_read_that_fails_with_an_expired_sign_in_gives_the_sign_in_message() {
    let variant = || Variant {
        code: 255,
        out: String::new(),
        err: format!("{SIGN_IN_TEXT}\\n"),
        message: SIGN_IN_MESSAGE.to_string(),
    };
    read_matrix(AMD64, vec![variant()]);
}

#[test]
fn a_windows_read_that_fails_with_an_expired_sign_in_gives_the_sign_in_message() {
    read_matrix(
        WINDOWS,
        vec![Variant {
            code: 255,
            out: String::new(),
            err: format!("{SIGN_IN_TEXT}\\n"),
            message: SIGN_IN_MESSAGE.to_string(),
        }],
    );
}

/// The answers with no `Parameter.Value` that is a string.
const NO_VALUE_ANSWERS: [&str; 8] = [
    "{}",
    "{\"Parameter\":{}}",
    "{\"Parameter\":{\"Name\":\"x\"}}",
    "{\"Parameter\":{\"Value\":42}}",
    "{\"Parameter\":{\"Value\":null}}",
    "{\"Parameter\":{\"Value\":[\"ami-0a0a0a0a\"]}}",
    "{\"Parameter\":\"ami-0a0a0a0a\"}",
    "{\"Value\":\"ami-0a0a0a0a\"}",
];

/// Values that are not `^ami-[0-9a-f]{8}([0-9a-f]{9})?\z`: eight digits, or seventeen, lowercase,
/// and nothing after. `\u000a` is a JSON escape for the line feed `\z` does not allow.
const BAD_VALUES: [&str; 14] = [
    "",
    "ami-",
    "ami-0a0a0a0",
    "ami-0a0a0a0a0",
    "ami-0a0a0a0a0a0a0a0a",
    "ami-0b0b0b0b0b0b0b0b0b",
    "ami-0A0A0A0A",
    "AMI-0a0a0a0a",
    "ami-0g0g0g0g",
    " ami-0a0a0a0a",
    "ami-0a0a0a0a ",
    "ami-0a0a0a0a\\u000a",
    "i-0a0a0a0a",
    "not an image",
];

#[test]
fn an_answer_that_is_not_json_fails_with_the_not_json_message() {
    read_matrix(
        AMD64,
        vec![Variant {
            code: 0,
            out: "oops, not json".to_string(),
            err: String::new(),
            message: "agentspace: Unexpected answer from ssm get-parameter: not JSON.".to_string(),
        }],
    );
}

#[test]
fn an_answer_with_no_value_fails_with_the_unexpected_answer_message_on_linux_amd64() {
    read_matrix(
        AMD64,
        NO_VALUE_ANSWERS.iter().map(|raw| answers(raw)).collect(),
    );
}

#[test]
fn an_answer_with_no_value_fails_with_the_unexpected_answer_message_on_windows() {
    read_matrix(
        WINDOWS,
        NO_VALUE_ANSWERS.iter().map(|raw| answers(raw)).collect(),
    );
}

#[test]
fn a_value_that_is_not_an_image_id_fails_with_the_same_message_on_linux_amd64() {
    read_matrix(AMD64, BAD_VALUES.iter().map(|v| value(v)).collect());
}

#[test]
fn a_value_that_is_not_an_image_id_fails_with_the_same_message_on_windows() {
    read_matrix(WINDOWS, BAD_VALUES.iter().map(|v| value(v)).collect());
}

#[test]
fn an_image_id_of_seventeen_digits_is_passed_whole() {
    let rig = plat_rig(false, &[PRec::new(AMD64, "creating")]);
    rig.stack_missing_typed(&stack(AMD64), "m8a.xlarge");
    rig.image_is(IMAGE17);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy_keyed(&say, &key(AMD64), None), "deploy");
        let log = rig.log();
        let deploy = log
            .calls("aws")
            .into_iter()
            .find(|c| c.args.get(1).map(String::as_str) == Some("deploy"))
            .expect("cloudformation deploy MUST have been started");
        rig.check(
            deploy
                .args
                .iter()
                .any(|a| *a == format!("ImageId={IMAGE17}")),
            "ImageId MUST be the value read, as one argument",
        );
    });
}

// ---------------------------------------------------------------- The resolved image ID is in no message

/// A deploy that succeeds prints no line, and the command's own streams hold no text, with the
/// image ID in it. (The stand-in `aws` is given the ID as an argument, which is a call and no
/// message.)
fn the_image_id_is_in_no_message(platform: &'static str) {
    let rig = new_stack_rig(platform, "m8a.xlarge");
    let Some(out) = rig.run(move |rig| {
        let say = rig.say();
        let result = rig.do_deploy_keyed(&say, &key(platform), None);
        let message = result.err().map(|f| f.message).unwrap_or_default();
        for (kind, text) in say.events.borrow().iter() {
            rig.check(
                !text.contains(IMAGE),
                &format!("A {kind} line holds the image ID: {text}"),
            );
        }
        rig.check(!message.contains(IMAGE), "The failure holds the image ID");
        rig.check(
            rig.log()
                .calls("aws")
                .iter()
                .any(|c| c.args.iter().any(|a| a == &format!("ImageId={IMAGE}"))),
            "Setup: the stack's creation was given the image",
        );
    }) else {
        return;
    };
    assert!(
        !out.stdout.contains(IMAGE) && !out.stderr.contains(IMAGE),
        "{platform}: the command's own streams MUST NOT hold the image ID"
    );
}

#[test]
fn the_resolved_image_id_is_in_no_message_of_a_linux_amd64_deploy() {
    the_image_id_is_in_no_message(AMD64);
}

#[test]
fn the_resolved_image_id_is_in_no_message_of_a_windows_deploy() {
    the_image_id_is_in_no_message(WINDOWS);
}

// ---------------------------------------------------------------- Messages name the host by its name

fn rollback_names_the_platform(platform: &'static str) {
    let rig = plat_rig(false, &[PRec::new(platform, "creating")]);
    rig.stack_status("ROLLBACK_COMPLETE");
    rig.run(move |rig| {
        let stack = stack(platform);
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_keyed(&say, &key(platform), None),
            "deploy -Host -Platform",
        );
        rig.same(
            "The failure message: ` -Platform <platform>` follows -Host <project>",
            &f.message,
            &format!(
                "agentspace: Stack '{stack}' is ROLLBACK_COMPLETE, which no update can start from. Look at its events, delete it with 'aws cloudformation delete-stack --region us-west-2 --stack-name {stack}', then run 'agentspace-operator provision -Host {PROJECT} -Platform {platform}' again."
            ),
        );
    });
}

#[test]
fn a_rollback_complete_linux_amd64_stack_names_the_platform_in_its_command() {
    rollback_names_the_platform(AMD64);
}

#[test]
fn a_rollback_complete_windows_stack_names_the_platform_in_its_command() {
    rollback_names_the_platform(WINDOWS);
}

#[test]
fn a_platform_with_no_record_is_refused_naming_the_host_and_the_platform() {
    // A project's host on one platform never stands in for its host on another.
    let rig = plat_rig(false, &[PRec::new(ARM64, "ready").id(ID2)]);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_keyed(&say, &key(AMD64), None),
            "deploy -Host -Platform linux/amd64",
        );
        rig.same(
            "The failure message",
            &f.message,
            &format!(
                "agentspace: Project '{PROJECT}.linux-amd64' has no host; provision -Host {PROJECT} -Platform linux/amd64 makes one."
            ),
        );
        rig.check(
            rig.log().all_calls().is_empty(),
            "No program MUST start for a project with no record on the platform",
        );
    });
}

// ---------------------------------------------------------------- The parameters file

fn names_member_case(member: &'static str, platform: &'static str) {
    let rig = plat_rig(false, &[PRec::new(platform, "creating")]);
    rig.stack_missing_with(&stack(platform));
    let text = format!(
        "{{\"version\":0,\"parameters\":{{\"SubnetId\":\"SUBNET_VALUE\",\"VpcId\":\"VPC_VALUE\",\"{member}\":\"x\"}}}}"
    );
    rig.write_home(".agentspace/stack-parameters.json", text.as_bytes());
    rig.run(move |rig| {
        let path = rig.home.join(".agentspace").join("stack-parameters.json");
        let prefix = format!(
            "agentspace: {} is not a valid stack parameters file: ",
            path.display()
        );
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_keyed(&say, &key(platform), None),
            "deploy",
        );
        let reason = f.message.strip_prefix(&prefix);
        rig.check(
            matches!(reason, Some(r) if r.contains(member) && r.ends_with('.') && !r.contains('\n')),
            &format!("A file that names {member} MUST fail as `agentspace: <path> is not a valid stack parameters file: <reason>.`, the reason naming it; it was `{}`", f.message),
        );
        rig.check(
            !rig.log().calls("aws").iter().any(|c| {
                c.args.get(1).map(String::as_str) == Some("deploy")
                    || c.args.get(1).map(String::as_str) == Some("get-parameter")
            }),
            "Nothing that changes anything MUST have been started, and no image read",
        );
    });
}

#[test]
fn a_parameters_file_that_names_host_os_is_not_valid_for_a_new_linux_arm64_host() {
    names_member_case("HostOs", ARM64);
}

#[test]
fn a_parameters_file_that_names_host_os_is_not_valid_for_a_new_linux_amd64_host() {
    names_member_case("HostOs", AMD64);
}

#[test]
fn a_parameters_file_that_names_host_os_is_not_valid_for_a_new_windows_host() {
    names_member_case("HostOs", WINDOWS);
}

#[test]
fn a_parameters_file_that_names_host_os_is_not_valid_for_the_box() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(
        ".agentspace/stack-parameters.json",
        br#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE","HostOs":"windows"}}"#,
    );
    rig.run(|rig| {
        let path = rig.home.join(".agentspace").join("stack-parameters.json");
        let prefix = format!(
            "agentspace: {} is not a valid stack parameters file: ",
            path.display()
        );
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_keyed(&say, &agentspace_hosts::HostKey::linux_arm64("swimcoach"), None),
            "deploy",
        );
        rig.check(
            matches!(f.message.strip_prefix(&prefix), Some(r) if r.contains("HostOs") && r.ends_with('.')),
            &format!("The box's deploy MUST refuse a file that names HostOs, naming it; it was `{}`", f.message),
        );
        rig.check(
            rig.log().all_calls().is_empty(),
            "The box's deploy MUST fail before any program starts",
        );
    });
}

#[test]
fn the_boxs_key_deploys_the_box_with_no_image_and_no_new_parameter() {
    // The box is swimcoach's linux/arm64 host: no get-parameter, no ImageId, no HostName, no
    // CaPublicKey, so its change set stays empty (D44).
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.stack_status("UPDATE_COMPLETE");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_keyed(
                &say,
                &agentspace_hosts::HostKey::linux_arm64("swimcoach"),
                None,
            ),
            "deploy",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        rig.check(
            aws.iter()
                .all(|c| c.args.get(1).map(String::as_str) != Some("get-parameter")),
            "The box MUST NOT read an image parameter",
        );
        let template = template_arg(&aws[0].args);
        rig.same_list(
            "The box's cloudformation deploy: its key and the file's members, nothing else",
            &aws[0].args,
            &deploy_call_args(
                "agentbox",
                &template,
                &[
                    format!("SshPublicKey={KEY_LINE}"),
                    "SubnetId=SUBNET_VALUE".to_string(),
                    "VpcId=VPC_VALUE".to_string(),
                ],
            ),
        );
    });
}

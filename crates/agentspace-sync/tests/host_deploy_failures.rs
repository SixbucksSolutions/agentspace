//! `deploy -Host`'s failures: the existence check's statuses, the parameters a new host's stack
//! needs, the public key, the CA, the registry, and what each leaves (docs/operator.md, deploy, the
//! existence check and steps 1 to 5; AWS failures; Calls by command; The host registry, as the
//! operator uses it).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each case runs the crate's hidden
//! `deploy_target_using` for a host in a child of the test binary, with one thing wrong, and checks
//! the failure's message and that nothing that changes anything was started.
#![cfg(windows)]

mod common;
mod hostcalls;

use agentspace_operator_api::{Failure, SIGN_IN_MESSAGE};
use agentspace_sync::Target;
use common::hosts::*;
use common::*;
use hostcalls::setup::*;

/// Runs a host's deploy in a rig that `setup` has changed, and gives `check` its failure and log.
fn case(beside: bool, setup: impl FnOnce(&Rig), check: impl FnOnce(&Rig, &Failure, &Log)) {
    let rig = host_rig(beside);
    setup(&rig);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        let log = rig.log();
        check(rig, &f, &log);
    });
}

/// No `cloudformation deploy`, no ssh and no scp was started: the failure came before any call that
/// changes anything.
fn changed_nothing(rig: &Rig, log: &Log, what: &str) {
    rig.check(
        !log.calls("aws")
            .iter()
            .any(|c| c.args.get(1).map(String::as_str) == Some("deploy")),
        &format!("{what}: no cloudformation deploy MUST have been started"),
    );
    rig.check(
        log.remote_calls().is_empty(),
        &format!("{what}: no ssh, scp or go MUST have been started"),
    );
}

/// The existence check was the one AWS call, and nothing else was started.
fn only_the_existence_check(rig: &Rig, log: &Log, what: &str) {
    let aws = log.calls("aws");
    let seen: Vec<String> = aws
        .iter()
        .map(|c| c.args.get(1).cloned().unwrap_or_default())
        .collect();
    rig.check(
        aws.len() == 1 && aws[0].args.get(1).map(String::as_str) == Some("describe-stacks"),
        &format!(
            "{what}: the existence check MUST be the one AWS call; the AWS calls were {seen:?}"
        ),
    );
    changed_nothing(rig, log, what);
}

// ---------------------------------------------------------------- The existence check

#[test]
fn a_stack_in_rollback_complete_fails_with_the_delete_message_before_any_deploy() {
    case(
        true,
        |rig| rig.stack_status("ROLLBACK_COMPLETE"),
        |rig, f, log| {
            let (stack, _) = names();
            rig.same(
                "The failure message",
                &f.message,
                &format!(
                    "agentspace: Stack '{stack}' is ROLLBACK_COMPLETE, which no update can start from. Look at its events, delete it with 'aws cloudformation delete-stack --region us-west-2 --stack-name {stack}', then run 'agentspace-operator provision -Host {PROJECT}' again."
                ),
            );
            only_the_existence_check(rig, log, "ROLLBACK_COMPLETE");
        },
    );
}

fn in_progress_case(status: &'static str) {
    case(
        true,
        move |rig| rig.stack_status(status),
        move |rig, f, log| {
            let (stack, _) = names();
            rig.same(
                "The failure message",
                &f.message,
                &format!(
                    "agentspace: Stack '{stack}' is {status}; try again once that operation has finished."
                ),
            );
            only_the_existence_check(rig, log, status);
        },
    );
}

#[test]
fn a_stack_in_create_in_progress_fails_with_the_try_later_message() {
    in_progress_case("CREATE_IN_PROGRESS");
}

#[test]
fn a_stack_in_update_rollback_in_progress_fails_with_the_try_later_message() {
    in_progress_case("UPDATE_ROLLBACK_IN_PROGRESS");
}

#[test]
fn a_stack_in_delete_in_progress_fails_with_the_try_later_message() {
    in_progress_case("DELETE_IN_PROGRESS");
}

fn no_update_case(status: &'static str) {
    case(
        true,
        move |rig| rig.stack_status(status),
        move |rig, f, log| {
            let (stack, _) = names();
            rig.same(
                "The failure message",
                &f.message,
                &format!(
                    "agentspace: Stack '{stack}' is {status}, which no update can start from. Look at its events; it can still hold the host's instance and disk, so do not delete it unless nothing on the host is to be kept."
                ),
            );
            rig.check(
                !f.message.contains("delete-stack"),
                "The message MUST name no delete command: deleting such a stack destroys the host's disk",
            );
            only_the_existence_check(rig, log, status);
        },
    );
}

#[test]
fn a_stack_in_update_rollback_failed_fails_and_names_no_delete() {
    no_update_case("UPDATE_ROLLBACK_FAILED");
}

#[test]
fn a_stack_in_delete_failed_fails_and_names_no_delete() {
    no_update_case("DELETE_FAILED");
}

#[test]
fn an_expired_sign_in_in_the_existence_check_fails_with_the_sign_in_message() {
    case(
        true,
        |rig| {
            rig.rule(
                "aws",
                1,
                "describe-stacks",
                255,
                "",
                "Unable to locate credentials. You can configure credentials by running \"aws configure\".\\n",
            )
        },
        |rig, f, log| {
            rig.same("The failure message", &f.message, SIGN_IN_MESSAGE);
            only_the_existence_check(rig, log, "A sign-in failure");
        },
    );
}

#[test]
fn another_failure_of_the_existence_check_fails_with_the_cli_s_own_text() {
    case(
        true,
        |rig| {
            rig.rule(
                "aws",
                1,
                "describe-stacks",
                255,
                "",
                "Throttling: Rate exceeded\\n",
            )
        },
        |rig, f, log| {
            rig.same(
                "The failure message",
                &f.message,
                "agentspace: Throttling: Rate exceeded",
            );
            only_the_existence_check(rig, log, "Another failure");
        },
    );
}

#[test]
fn a_missing_stack_is_found_in_any_case_and_created() {
    // The error text holding `does not exist` without regard to case means no stack (operator.md,
    // deploy).
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.rule(
        "aws",
        1,
        "describe-stacks",
        255,
        "",
        &format!("An error occurred (ValidationError): Stack with id {stack} DOES NOT EXIST\\n"),
    );
    rig.stack_status("CREATE_COMPLETE");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        let template = template_arg(&aws[1].args);
        rig.same_list(
            "The cloudformation deploy call",
            &aws[1].args,
            &deploy_call_args(&stack, &template, &creation_overrides(rig, &stack)),
        );
    });
}

// ---------------------------------------------------------------- What a new host's stack needs

/// A parameters file `text` that does not name both `SubnetId` and `VpcId` fails a new host's stack.
fn missing_subnet_or_vpc_case(text: &'static [u8]) {
    case(
        true,
        move |rig| {
            let (stack, _) = names();
            rig.stack_missing(&stack);
            rig.write_home(".agentspace/stack-parameters.json", text);
        },
        |rig, f, log| {
            let path = rig.home.join(".agentspace").join("stack-parameters.json");
            rig.same(
                "The failure message",
                &f.message,
                &format!(
                    "agentspace: {} MUST name SubnetId and VpcId for a new host's stack.",
                    path.display()
                ),
            );
            changed_nothing(rig, log, "A file without SubnetId and VpcId");
        },
    );
}

#[test]
fn a_parameters_file_without_a_vpc_fails_a_new_hosts_stack() {
    missing_subnet_or_vpc_case(br#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE"}}"#);
}

#[test]
fn a_parameters_file_without_a_subnet_fails_a_new_hosts_stack() {
    missing_subnet_or_vpc_case(
        br#"{"version":0,"parameters":{"VpcId":"VPC_VALUE","InstanceType":"m9g.2xlarge"}}"#,
    );
}

#[test]
fn a_parameters_file_with_no_parameters_fails_a_new_hosts_stack() {
    missing_subnet_or_vpc_case(br#"{"version":0,"parameters":{}}"#);
}

#[test]
fn an_answer_with_no_stack_status_fails_with_the_unexpected_answer_message() {
    no_status_case(r#"{"Stacks":[{}]}"#);
}

#[test]
fn an_answer_whose_stack_status_is_not_a_string_fails_the_same_way() {
    no_status_case(r#"{"Stacks":[{"StackStatus":3}]}"#);
}

fn no_status_case(answer: &'static str) {
    case(
        true,
        move |rig| rig.rule("aws", 0, "describe-stacks", 0, answer, ""),
        |rig, f, log| {
            rig.same(
                "The failure message",
                &f.message,
                "agentspace: Unexpected answer from cloudformation describe-stacks: no Stacks[0].StackStatus.",
            );
            only_the_existence_check(rig, log, "No StackStatus");
        },
    );
}

fn names_member_case(member: &'static str) {
    case(
        true,
        move |rig| {
            let (stack, _) = names();
            rig.stack_missing(&stack);
            let text = format!(
                "{{\"version\":0,\"parameters\":{{\"SubnetId\":\"SUBNET_VALUE\",\"VpcId\":\"VPC_VALUE\",\"{member}\":\"x\"}}}}"
            );
            rig.write_home(".agentspace/stack-parameters.json", text.as_bytes());
        },
        move |rig, f, log| {
            let path = rig.home.join(".agentspace").join("stack-parameters.json");
            let prefix = format!(
                "agentspace: {} is not a valid stack parameters file: ",
                path.display()
            );
            let reason = f.message.strip_prefix(&prefix);
            rig.check(
                matches!(reason, Some(r) if r.contains(member) && r.ends_with('.') && !r.contains('\n')),
                &format!("A file that names {member} MUST fail as `agentspace: <path> is not a valid stack parameters file: <reason>.`, the reason naming it; it was `{}`", f.message),
            );
            changed_nothing(rig, log, member);
        },
    );
}

#[test]
fn a_parameters_file_that_names_host_name_is_not_valid_for_a_new_host() {
    names_member_case("HostName");
}

#[test]
fn a_parameters_file_that_names_ca_public_key_is_not_valid_for_a_new_host() {
    names_member_case("CaPublicKey");
}

#[test]
fn a_missing_public_key_fails_a_new_hosts_stack_naming_the_file() {
    case(
        true,
        |rig| {
            let (stack, _) = names();
            rig.stack_missing(&stack);
            rig.remove_home(".ssh/id_ed25519.pub");
        },
        |rig, f, log| {
            let key = rig.home.join(".ssh").join("id_ed25519.pub");
            rig.same(
                "The failure message",
                &f.message,
                &format!("agentspace: {} does not exist.", key.display()),
            );
            changed_nothing(rig, log, "A missing public key");
        },
    );
}

#[test]
fn a_ca_that_is_not_there_fails_a_new_hosts_stack_with_the_cas_message_before_any_deploy() {
    case(
        true,
        |rig| {
            let (stack, _) = names();
            rig.stack_missing(&stack);
            rig.remove_home(".agentspace/ca");
        },
        |rig, f, log| {
            let want = match agentspace_ca::check_for_sync(&rig.ctx(), &ca_env()) {
                Err(w) => w,
                Ok(_) => panic!("The CA check passed, so this case is not a CA failure"),
            };
            rig.same("The failure message", &f.message, &want.message);
            changed_nothing(rig, log, "A missing CA");
        },
    );
}

// ---------------------------------------------------------------- The instance ID, and sync

#[test]
fn a_stack_with_no_instance_id_output_fails_and_leaves_the_record_with_none() {
    case(
        true,
        |rig| {
            rig.seed_hosts(&[Seed::new(PROJECT, "creating").id(ID)]);
            rig.rule(
                "aws",
                0,
                "describe-stacks",
                0,
                &json_stack_status("CREATE_COMPLETE", None),
                "",
            );
        },
        |rig, f, log| {
            let (stack, _) = names();
            rig.same(
                "The failure message",
                &f.message,
                &format!("agentspace: Stack '{stack}' has no InstanceId output."),
            );
            rig.check(
                rig.record(PROJECT).instance_id.is_none(),
                "The old instance ID MUST be removed first, and none written when the read fails",
            );
            rig.check(
                log.remote_calls().is_empty()
                    && !log
                        .calls("aws")
                        .iter()
                        .any(|c| c.args.get(1).map(String::as_str) == Some("describe-instances")),
                "Nothing MUST follow a failed read: no sync",
            );
        },
    );
}

#[test]
fn a_failed_sync_fails_deploy_with_the_syncs_message() {
    let rig = host_rig(true);
    rig.stack_status("CREATE_COMPLETE");
    rig.rule("ssh", 1, "", 1, "", "");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        rig.same(
            "The failure message",
            &f.message,
            "agentspace: Removing the old staging folder on the instance failed.",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's folder MUST be removed as the command ends, after a failure too",
        );
    });
}

#[test]
fn a_registry_that_is_not_valid_fails_before_any_program_starts() {
    case(
        true,
        |rig| rig.write_home(".agentspace/hosts.json", b"{ this is not json"),
        |rig, f, log| {
            let path = rig.hosts_json_path();
            let prefix = format!(
                "agentspace: The host registry {} cannot be read: ",
                path.display()
            );
            rig.check(
                f.message.starts_with(&prefix) && f.message.ends_with('.') && !f.message.contains('\n'),
                &format!("The failure MUST be `agentspace: The host registry <path> cannot be read: <reason>.`; it was `{}`", f.message),
            );
            rig.check(
                log.all_calls().is_empty(),
                "A registry that cannot be read MUST fail before any program starts",
            );
            rig.check(
                rig.hosts_bytes().as_deref() == Some(&b"{ this is not json"[..]),
                "A registry that is not valid is never repaired",
            );
        },
    );
}

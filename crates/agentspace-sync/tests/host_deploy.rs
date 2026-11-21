//! `deploy -Host` for a host that is not the box: creating its stack, updating it, the registry's
//! `instance_id`, and the operator's folder of the command's own (docs/operator.md, deploy; Which
//! host a command acts on; Calls by command; The stack; The operator's folder, one per command;
//! docs/state.md, The host registry).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `deploy_target_using` for `Target::Host` in a child of the test binary, under a throwaway home,
//! with stand-ins for the AWS CLI, ssh, scp and go. The failures are in `host_deploy_failures`.
//!
//! **A new host's stack takes only `SubnetId` and `VpcId` from the parameters file**, so the file
//! of these tests also names `InstanceType` and `RootVolumeGiB`, and no test of a new host's stack
//! passes either.
#![cfg(windows)]

mod common;
mod hostcalls;

use agentspace_sync::Target;
use common::expect::*;
use common::hosts::*;
use common::*;
use hostcalls::setup::*;

#[test]
fn a_new_hosts_stack_is_created_with_only_the_creation_parameters() {
    let rig = host_rig(true);
    let (stack, alias) = names();
    rig.seed_hosts(&[Seed::new(PROJECT, "creating").id(ID)]);
    rig.stack_missing(&stack);
    rig.rule(
        "aws",
        0,
        "cloudformation deploy",
        0,
        "deploy stand-in output\\n",
        "",
    );
    watch_registry(&rig);
    let Some(out) = rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        let log = rig.log();
        let aws = log.calls("aws");

        // The existence check first, then the deploy, then the output's read, then sync's start.
        rig.same_list(
            "The existence check",
            &aws[0].args,
            &describe_stacks_args(&stack),
        );
        let template = template_arg(&aws[1].args);
        rig.same_list(
            "The cloudformation deploy call",
            &aws[1].args,
            &deploy_call_args(&stack, &template, &creation_overrides(rig, &stack)),
        );
        let hex = check_template(rig, &log, 2, &template);
        rig.same_list(
            "The call that reads the instance ID",
            &aws[2].args,
            &describe_stacks_args(&stack),
        );
        rig.same_list(
            "The first call of sync's start-and-wait, for the instance the stack made",
            &aws[3].args,
            &describe_instances_args(ID2),
        );

        // CaPublicKey is the key's type and base64 alone: one space, no comment.
        let ca = ca_key(rig);
        rig.check(
            ca.split(' ').count() == 2 && ca.starts_with("ssh-ed25519 "),
            "CaPublicKey MUST be the key type and the base64 key, joined by one space",
        );

        // The messages, in order, then sync's.
        let lines = say.progress_lines();
        rig.same(
            "The first message",
            &lines[0],
            &format!("Deploying stack '{stack}' in us-west-2..."),
        );
        rig.same("The second message", &lines[1], &format!("Instance: {ID2}"));

        // The registry: the old ID removed before the read, the new one written, nothing else.
        rig.check(
            log.watched("aws", 2, "hosts.json")
                .unwrap_or("")
                .contains(ID),
            "Before the deploy call the record still holds the instance ID it had",
        );
        rig.check(
            !log.watched("aws", 3, "hosts.json")
                .unwrap_or("x")
                .contains("instance_id"),
            "The instance ID MUST be removed from the record before it is read from the stack",
        );
        rig.check(
            log.watched("aws", 4, "hosts.json")
                .unwrap_or("")
                .contains(&format!("\"instance_id\":\"{ID2}\"")),
            "The record MUST hold the instance ID the stack's output names before sync starts it",
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_id.as_deref() == Some(ID2)
                && record.stack == stack
                && record.state.as_str() == "creating"
                && record.last_attempt_at.is_none(),
            "deploy MUST write the instance ID into the record and nothing else, never a state",
        );
        rig.check(
            rig.record("swimcoach").instance_id.is_none(),
            "The box's record MUST be left as it was",
        );

        // Its sync, over the host's alias, writing into the same folder the template is in.
        rig.same_list(
            "sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, false, false),
        );
        check_calls_with(rig, &log, false, false, &alias, Folder::PerCommand);
        let scp = log.calls("scp");
        rig.same(
            "The folder of the template and of box/system",
            &command_folder_of(&scp[0].args[4]).unwrap_or_default(),
            &hex,
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's folder MUST be removed as the command ends",
        );
        check_environment(rig, &log);
    }) else {
        return;
    };
    assert!(
        out.stdout.contains("deploy stand-in output"),
        "cloudformation deploy's output MUST be passed through"
    );
}

#[test]
fn a_stack_in_review_in_progress_is_created_with_the_creation_parameters() {
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.stack_first_status("REVIEW_IN_PROGRESS");
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

/// An update of a host's stack in `status`: no parameter at all is passed, so every one keeps its
/// previous value, and neither the public key, the parameters file nor the CA's key is read.
fn update_case(status: &'static str, state: &'static str) {
    let rig = host_rig(true);
    let (stack, alias) = names();
    rig.remove_home(".ssh/id_ed25519.pub");
    rig.remove_home(".agentspace/stack-parameters.json");
    rig.seed_hosts(&[Seed::new(PROJECT, state).id(ID)]);
    rig.stack_status(status);
    rig.run(move |rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        rig.same_list(
            "The existence check",
            &aws[0].args,
            &describe_stacks_args(&stack),
        );
        let template = template_arg(&aws[1].args);
        rig.same_list(
            "The cloudformation deploy call",
            &aws[1].args,
            &deploy_call_args(&stack, &template, &[]),
        );
        rig.check(
            !aws[1]
                .args
                .iter()
                .any(|a| a.contains("--parameter-overrides")),
            "An update MUST pass no parameter at all, so every one keeps its previous value",
        );
        check_template(rig, &log, 2, &template);
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_id.as_deref() == Some(ID2) && record.state.as_str() == state,
            "The record MUST hold the stack's instance ID and keep its state",
        );
        rig.same_list(
            "sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, false, false),
        );
        check_calls_with(rig, &log, false, false, &alias, Folder::PerCommand);
    });
}

#[test]
fn a_create_complete_stack_is_updated_with_no_parameter() {
    update_case("CREATE_COMPLETE", "ready");
}

#[test]
fn an_update_complete_stack_is_updated_with_no_parameter() {
    update_case("UPDATE_COMPLETE", "creating");
}

#[test]
fn an_update_rollback_complete_stack_is_updated_with_no_parameter() {
    update_case("UPDATE_ROLLBACK_COMPLETE", "ready");
}

#[test]
fn an_import_complete_stack_is_updated_with_no_parameter() {
    update_case("IMPORT_COMPLETE", "failed");
}

#[test]
fn an_import_rollback_complete_stack_is_updated_with_no_parameter() {
    update_case("IMPORT_ROLLBACK_COMPLETE", "ready");
}

#[test]
fn deploy_by_itself_writes_no_state_and_leaves_the_attempt_alone() {
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "failed")
        .id(ID)
        .attempt("2026-11-20T10:00:00Z")]);
    rig.stack_status("CREATE_COMPLETE");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.state.as_str() == "failed"
                && record.last_attempt_at.as_deref() == Some("2026-11-20T10:00:00Z")
                && record.instance_id.as_deref() == Some(ID2),
            "deploy MUST write only the record's instance ID: never a state, never the attempt's time",
        );
    });
}

#[test]
fn a_leftover_instance_id_cache_is_never_read_and_is_left_as_it_was() {
    // The retired cache holds `ID`; the host's own ID is the registry's, and then the stack's
    // output (operator.md, Laptop files: no v0.07 program reads, writes or removes it).
    let rig = host_rig(true);
    rig.write_home(
        ".agentspace/instance-id.json",
        format!("{{\"version\":0,\"instance_id\":\"{ID}\"}}").as_bytes(),
    );
    rig.seed_hosts(&[Seed::new(PROJECT, "creating")]);
    rig.stack_status("CREATE_COMPLETE");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        let log = rig.log();
        rig.same_list(
            "The first call of sync's start-and-wait",
            &log.calls("aws")[3].args,
            &describe_instances_args(ID2),
        );
        rig.check(
            std::fs::read(rig.home_path(".agentspace/instance-id.json")).ok()
                == Some(format!("{{\"version\":0,\"instance_id\":\"{ID}\"}}").into_bytes()),
            "A host's deploy MUST leave the retired instance-ID cache as it was",
        );
    });
}

// ---------------------------------------------------------------- The operator's folder

#[test]
fn an_old_command_folder_is_swept_and_a_younger_one_and_other_names_are_left() {
    let rig = host_rig(true);
    rig.stack_status("CREATE_COMPLETE");
    let old = "0123456789abcdef0123456789abcdef";
    let young = "fedcba9876543210fedcba9876543210";
    let upper = "ABCDEF0123456789ABCDEF0123456789";
    let short = "0123456789abcdef0123456789abcde";
    rig.leftover_folder(old, 25);
    rig.leftover_folder(young, 23);
    rig.leftover_folder(upper, 30);
    rig.leftover_folder(short, 30);
    rig.leftover_folder("system", 30);
    rig.write_home(".agentspace/operator/agentspace.yaml", b"a v0.06 template");
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        let left = rig.operator_entries();
        rig.check(
            !left.iter().any(|n| n == old),
            "A folder of 32 hex digits created more than 24 hours ago MUST be swept, with what is in it",
        );
        for keep in [young, upper, short, "system", "agentspace.yaml"] {
            rig.check(
                left.iter().any(|n| n == keep),
                &format!("The sweep MUST leave `{keep}`: a younger folder, or a name that is not 32 lowercase hex digits"),
            );
        }
        rig.check(
            rig.command_folders() == vec![young.to_string()],
            "The command's own folder MUST be removed as it ends, and only the younger one left",
        );
    });
}

#[test]
fn the_command_folder_is_removed_after_a_failed_deploy_too() {
    let rig = host_rig(true);
    rig.stack_status("CREATE_COMPLETE");
    rig.rule("aws", 0, "cloudformation deploy", 1, "", "stack failed\\n");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        rig.same(
            "The failure message",
            &f.message,
            "agentspace: Stack deploy failed; see the stack events in the console.",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        let template = template_arg(&aws[1].args);
        check_template(rig, &log, 2, &template);
        rig.check(
            rig.command_folders().is_empty(),
            "The command's folder MUST be removed as the command ends, after a failure too",
        );
    });
}

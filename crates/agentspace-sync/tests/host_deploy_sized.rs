//! `deploy -Host <project> -InstanceType <type>`, through the hidden twin of the public
//! `deploy_with_instance_type` (docs/operator.md, deploy, step 4; Command line, the host checks 5
//! and 6; architect, 2026-11-21, for v0.07-T03, confirming it as public API).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `deploy_target_sized_using` in a child of the test binary, under a throwaway home, with
//! stand-ins for the AWS CLI, ssh, scp and go.
//!
//! - **An existing host's stack gets `InstanceType=<type>` alone**, so every other parameter keeps
//!   its previous value.
//! - **A stack it creates gets `InstanceType=<type>` with the creation parameters**, and never the
//!   box's `InstanceType` from the parameters file.
//! - **The box is sized through `stack-parameters.json` alone**: `-InstanceType` without `-Host`, or
//!   with `-Host swimcoach`, is refused before anything starts.
#![cfg(windows)]

mod common;
mod hostcalls;

use agentspace_sync::Target;
use common::hosts::*;
use common::*;
use hostcalls::setup::*;

const TYPE: &str = "m9g.2xlarge";

/// The overrides after `--parameter-overrides` in a `cloudformation deploy` call, sorted.
fn overrides_of(args: &[String]) -> Vec<String> {
    let mut v: Vec<String> = match args.iter().position(|a| a == "--parameter-overrides") {
        Some(i) => args[i + 1..].to_vec(),
        None => Vec::new(),
    };
    v.sort();
    v
}

#[test]
fn an_existing_hosts_stack_gets_the_instance_type_alone() {
    let rig = host_rig(true);
    let (stack, _) = names();
    // The update path reads neither the key nor the parameters file.
    rig.remove_home(".ssh/id_ed25519.pub");
    rig.remove_home(".agentspace/stack-parameters.json");
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID)]);
    rig.stack_status("CREATE_COMPLETE");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(TYPE)),
            "deploy -Host -InstanceType",
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
            &deploy_call_args(&stack, &template, &[format!("InstanceType={TYPE}")]),
        );
        rig.check(
            rig.record(PROJECT).instance_id.as_deref() == Some(ID2),
            "The record MUST hold the stack's instance ID after the deploy",
        );
        rig.check(
            !log.calls("ssh").is_empty(),
            "deploy MUST go on to its sync step",
        );
    });
}

#[test]
fn a_new_hosts_stack_gets_the_instance_type_with_the_creation_parameters() {
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.stack_missing(&stack);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(TYPE)),
            "deploy -Host -InstanceType",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        let template = template_arg(&aws[1].args);
        let call = &aws[1].args;
        let at = call
            .iter()
            .position(|a| a == "--parameter-overrides")
            .unwrap_or(call.len());
        rig.same_list(
            "The cloudformation deploy call up to its overrides",
            &call[..at.min(call.len())],
            &deploy_call_args(&stack, &template, &[]),
        );
        let mut want = creation_overrides(rig, &stack);
        want.push(format!("InstanceType={TYPE}"));
        want.sort();
        rig.same_list(
            "The creation parameters and the instance type, and no other member of the file",
            &overrides_of(call),
            &want,
        );
        rig.check(
            !call
                .iter()
                .any(|a| a.contains("m9g.8xlarge") || a.contains("RootVolumeGiB")),
            "The box's own InstanceType and RootVolumeGiB MUST NOT reach a new host's stack",
        );
    });
}

/// `-InstanceType` for `target`, which is the box: refused with the host check's message before
/// any program starts or any file is read.
fn refused_for_the_box(target: Target<'static>) {
    let rig = Rig::new(false);
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_sized(&say, target, Some(TYPE)),
            "deploy -InstanceType for the box",
        );
        rig.same(
            "The refusal",
            &f.message,
            "agentspace: -InstanceType needs -Host <project> of a host that is not the box; the box is sized through stack-parameters.json.",
        );
        rig.check(
            rig.log().all_calls().is_empty(),
            "Nothing MUST be started before the host checks",
        );
        rig.check(
            say.events.borrow().is_empty(),
            "Nothing MUST be printed before the refusal",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "No folder of the command's own is left",
        );
    });
}

#[test]
fn the_instance_type_without_a_host_is_refused_for_the_box() {
    refused_for_the_box(Target::Box);
}

#[test]
fn the_instance_type_with_the_boxs_own_project_is_refused_as_for_the_box() {
    refused_for_the_box(Target::Host("swimcoach"));
}

#[test]
fn without_an_instance_type_the_box_deploys_as_before() {
    // The sized twin with `None` is the box's deploy, exactly as `deploy_using`: every member of
    // the file, and no `HostName` or `CaPublicKey`.
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host("swimcoach"), None),
            "deploy -Host swimcoach",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        let template = template_arg(&aws[0].args);
        rig.same_list(
            "The box's cloudformation deploy call",
            &aws[0].args,
            &[
                svec(&[
                    "cloudformation",
                    "deploy",
                    "--region",
                    "us-west-2",
                    "--stack-name",
                    "agentbox",
                    "--template-file",
                ]),
                vec![template.clone()],
                svec(&[
                    "--capabilities",
                    "CAPABILITY_IAM",
                    "--no-fail-on-empty-changeset",
                    "--tags",
                    "project=agentbox",
                    "--parameter-overrides",
                ]),
                vec![format!("SshPublicKey={KEY_LINE}")],
                svec(&["SubnetId=SUBNET_VALUE", "VpcId=VPC_VALUE"]),
            ]
            .concat(),
        );
    });
}

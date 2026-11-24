//! `deploy` records its host's instance type, and sizes the box like every host (E0.09-T03-S02,
//! #0204; docs/operator.md, deploy, steps 4 and 5, and its lock; docs/state.md, The instance type,
//! The order of the locks; docs/decisions.md, D67 and its change notes).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `deploy_target_sized_using` or `deploy_using` in a child of the test binary, under a throwaway
//! home, with stand-ins for the AWS CLI, ssh, scp and go. Every stand-in records, at each call, the
//! registry's content and whether a lock is held, which is how a test sees what a record said
//! while a step ran.
//!
//! - **The type is recorded after a deploy that succeeds, from the stack's `InstanceType`
//!   parameter as `describe-stacks` reads it back**, never from `-InstanceType` and never from an
//!   older record (state.md, The instance type; pin P23).
//! - **A deploy that passes `-InstanceType` removes the record's type before `cloudformation
//!   deploy` runs**, and a deploy that fails or whose read-back fails leaves none (pins P11 and
//!   P30). A deploy that passes none and fails before its read-back keeps the type.
//! - **A stack with no one `InstanceType` parameter of the form fails the deploy** (pin P22).
//! - **The box is sized like every host**: its plain `deploy` passes no `InstanceType` and reads
//!   none from `stack-parameters.json`, and `deploy -InstanceType <type>` passes it (D67, D71; pin
//!   P6).
//! - **A person's `deploy` holds its host's `provision.lock` for its life, and is refused while
//!   another program holds it** (D67's change note; pins P34 and P36).
#![cfg(windows)]

mod common;
mod hostcalls;

use std::time::{Duration, Instant};

use agentspace_sync::Target;
use common::hosts::*;
use common::*;
use hostcalls::setup::*;

/// The type the stack holds in these tests, and a different one the record may hold before.
const BEFORE: &str = "m9g.large";
const AFTER: &str = "m9g.2xlarge";
/// A type no test's stack holds: where a test needs a type that must reach no call.
const FILE_TYPE: &str = "m9g.16xlarge";

fn aws_named(log: &Log, verb: &str) -> Vec<Rec> {
    log.calls("aws")
        .into_iter()
        .filter(|c| c.args.get(1).map(String::as_str) == Some(verb))
        .cloned()
        .collect()
}

/// The registry record of `project` as it was when the `n`th aws call started.
fn record_at(log: &Log, n: u32, project: &str) -> String {
    match log.watched("aws", n, "hosts.json") {
        Some(content) => record_in(content, project),
        None => panic!("The registry was not watched at aws call {n}"),
    }
}

fn has(record: &str, member: &str) -> bool {
    record.contains(&format!("\"{member}\":"))
}

fn names_type(record: &str, t: &str) -> bool {
    record.contains(&format!("\"instance_type\":\"{t}\""))
}

// ------------------------------------------------------------------ The type recorded

#[test]
fn a_new_stack_records_the_type_describe_stacks_reads_back() {
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.seed_hosts(&[Seed::new(PROJECT, "creating")]);
    rig.stack_missing_typed(&stack, "m9g.4xlarge");
    watch_registry(&rig);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), None),
            "deploy -Host",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        let template = template_arg(&aws[1].args);
        rig.same_list(
            "The cloudformation deploy call: no InstanceType without -InstanceType",
            &aws[1].args,
            &deploy_call_args(&stack, &template, &creation_overrides(rig, &stack)),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.as_deref() == Some("m9g.4xlarge")
                && record.instance_id.as_deref() == Some(ID2)
                && record.state.as_str() == "creating"
                && record.last_attempt_at.is_none(),
            "The record MUST hold the type the stack reports, its instance ID, and nothing else new",
        );
        rig.check(
            rig.record("swimcoach").instance_type.is_none(),
            "The box's record MUST be left as it was",
        );
        rig.check(
            say.warnings().is_empty(),
            "Recording the type MUST print no warning",
        );
    });
}

#[test]
fn an_update_records_the_type_the_stack_holds_after_it_and_keeps_the_old_one_until_then() {
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.stack_status_typed("CREATE_COMPLETE", AFTER);
    watch_registry(&rig);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), None),
            "deploy -Host",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        rig.same_list(
            "The existence check",
            &aws[0].args,
            &describe_stacks_args(&stack),
        );
        rig.check(
            aws[1].args.get(1).map(String::as_str) == Some("deploy")
                && !aws[1].args.iter().any(|a| a.contains("InstanceType")),
            "A deploy without -InstanceType MUST pass no InstanceType: the stack keeps its own",
        );
        rig.same_list(
            "The read-back",
            &aws[2].args,
            &describe_stacks_args(&stack),
        );
        rig.check(
            aws[3].args.get(1).map(String::as_str) == Some("describe-instances"),
            "deploy MUST make no AWS call that it did not before v0.09: the one read gives the instance ID and the type (pin P23)",
        );
        // A deploy that passes no InstanceType writes nothing to the record at step 4: the type
        // is kept until the read-back (pin P30).
        let at_deploy = record_at(&log, aws[1].n, PROJECT);
        rig.check(
            names_type(&at_deploy, BEFORE) && at_deploy.contains(&format!("\"instance_id\":\"{ID}\"")),
            &format!("Without -InstanceType the record MUST keep its type and its instance ID through cloudformation deploy: {at_deploy}"),
        );
        // Step 5 removes the instance ID, and the type with it in the same write, before the read.
        let at_read = record_at(&log, aws[2].n, PROJECT);
        rig.check(
            !has(&at_read, "instance_id") && !has(&at_read, "instance_type"),
            &format!("Before the read-back the record MUST hold neither the instance ID nor the type: {at_read}"),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.as_deref() == Some(AFTER)
                && record.instance_id.as_deref() == Some(ID2)
                && record.state.as_str() == "ready",
            "The record MUST hold the type the stack reports after the deploy, not the one it held before",
        );
        rig.check(
            !log.calls("ssh").is_empty(),
            "deploy MUST go on to its sync step",
        );
    });
}

#[test]
fn an_update_with_an_instance_type_removes_the_type_before_cloudformation_deploy() {
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.stack_status_typed("CREATE_COMPLETE", AFTER);
    watch_registry(&rig);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(AFTER)),
            "deploy -Host -InstanceType",
        );
        let log = rig.log();
        let deploy = &aws_named(&log, "deploy")[0];
        let template = template_arg(&deploy.args);
        rig.same_list(
            "The cloudformation deploy call",
            &deploy.args,
            &deploy_call_args(&stack, &template, &[format!("InstanceType={AFTER}")]),
        );
        let at_deploy = record_at(&log, deploy.n, PROJECT);
        rig.check(
            !has(&at_deploy, "instance_type")
                && at_deploy.contains(&format!("\"instance_id\":\"{ID}\""))
                && at_deploy.contains("\"state\":\"ready\""),
            &format!("The record's type MUST be removed before cloudformation deploy runs, changing nothing else: {at_deploy}"),
        );
        rig.check(
            rig.record(PROJECT).instance_type.as_deref() == Some(AFTER),
            "The record MUST hold the type the stack reports once the deploy has succeeded",
        );
    });
}

#[test]
fn the_type_recorded_is_the_stacks_own_never_the_one_asked_for() {
    // The stack reports `BEFORE` although `AFTER` was asked for: what is stored is exactly what
    // describe-stacks returns (state.md, The instance type).
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(AFTER)]);
    rig.stack_status_typed("CREATE_COMPLETE", BEFORE);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(AFTER)),
            "deploy -Host -InstanceType",
        );
        rig.check(
            rig.record(PROJECT).instance_type.as_deref() == Some(BEFORE),
            "The recorded type MUST be what describe-stacks reads back, never -InstanceType's value",
        );
    });
}

#[test]
fn a_new_stack_with_an_instance_type_records_what_the_stack_reports_and_has_none_during_the_deploy()
{
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.seed_hosts(&[Seed::new(PROJECT, "creating").itype(BEFORE)]);
    rig.stack_missing_typed(&stack, AFTER);
    watch_registry(&rig);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(AFTER)),
            "deploy -Host -InstanceType",
        );
        let log = rig.log();
        let deploy = &aws_named(&log, "deploy")[0];
        let mut want = creation_overrides(rig, &stack);
        want.push(format!("InstanceType={AFTER}"));
        let template = template_arg(&deploy.args);
        rig.same_list(
            "The cloudformation deploy call",
            &deploy.args,
            &deploy_call_args(&stack, &template, &want),
        );
        rig.check(
            !has(&record_at(&log, deploy.n, PROJECT), "instance_type"),
            "A type the record held MUST be removed before cloudformation deploy runs",
        );
        rig.check(
            rig.record(PROJECT).instance_type.as_deref() == Some(AFTER),
            "The record MUST hold the type the new stack reports",
        );
    });
}

#[test]
fn recording_the_type_waits_for_the_registry_lock() {
    // The removal and the write are each under hosts.lock (state.md, The instance type): a holder
    // of the lock makes a deploy that passes -InstanceType wait for it, and then go on.
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.stack_status_typed("CREATE_COMPLETE", AFTER);
    rig.run(|rig| {
        let home = rig.home.clone();
        let (ready, held) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let lock = match agentspace_hosts::lock(&home) {
                Ok(lock) => lock,
                Err(_) => panic!("The throwaway registry lock could not be taken"),
            };
            let _ = ready.send(());
            std::thread::sleep(Duration::from_millis(1500));
            drop(lock);
        });
        if held.recv().is_err() {
            panic!("The holder of the registry lock never took it");
        }
        let start = Instant::now();
        let say = rig.say();
        let done = rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(AFTER));
        let waited = start.elapsed();
        let _ = holder.join();
        rig.succeeded(done, "deploy -Host -InstanceType");
        rig.check(
            waited >= Duration::from_millis(1000),
            "A deploy MUST wait for the registry lock another holds, not write past it",
        );
        rig.check(
            rig.record(PROJECT).instance_type.as_deref() == Some(AFTER),
            "The deploy MUST go on to record the type once the lock is free",
        );
    });
}

// ------------------------------------------------------------------ The type not recorded

/// A host that was `ready` at `BEFORE` with `ID`, whose `cloudformation deploy` fails.
fn failing_deploy_rig() -> Rig {
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.stack_status_typed("CREATE_COMPLETE", AFTER);
    rig.rule("aws", 0, "cloudformation deploy", 1, "", "boom\\n");
    watch_registry(&rig);
    rig
}

const DEPLOY_FAILED: &str = "agentspace: Stack deploy failed; see the stack events in the console.";

#[test]
fn a_failed_deploy_that_passed_an_instance_type_leaves_the_record_with_no_type() {
    let rig = failing_deploy_rig();
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(AFTER)),
            "deploy -Host -InstanceType",
        );
        rig.same("The failure's message", &f.message, DEPLOY_FAILED);
        let log = rig.log();
        rig.check(
            log.calls("aws").len() == 2 && log.remote_calls().is_empty(),
            "Nothing MUST follow a failed cloudformation deploy: no read-back and no sync",
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.is_none()
                && record.instance_id.as_deref() == Some(ID)
                && record.state.as_str() == "ready",
            "The removal is not undone: the record MUST hold no type, and keep its instance ID and state",
        );
    });
}

#[test]
fn a_failed_deploy_that_passed_no_instance_type_keeps_the_type() {
    // It asked for no change of type, so it changed nothing the record says (pin P30).
    let rig = failing_deploy_rig();
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), None),
            "deploy -Host",
        );
        rig.same("The failure's message", &f.message, DEPLOY_FAILED);
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.as_deref() == Some(BEFORE)
                && record.instance_id.as_deref() == Some(ID),
            "A deploy that passed no InstanceType and failed before its read-back MUST keep the type",
        );
    });
}

/// A deploy whose `cloudformation deploy` succeeds and whose read-back fails.
fn failed_read_back_case(instance_type: Option<&'static str>) {
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.read_back_fails("read-back stand-in error\\n");
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), instance_type),
            "deploy -Host",
        );
        rig.same(
            "The failure's message",
            &f.message,
            "agentspace: read-back stand-in error",
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_type.is_none() && record.instance_id.is_none(),
            "A failed read-back MUST leave the record with no type and no instance ID, never a stale one",
        );
        rig.check(
            record.state.as_str() == "ready"
                && rig.log().remote_calls().is_empty(),
            "deploy MUST keep the state and not go on to sync",
        );
    });
}

#[test]
fn a_failed_read_back_after_an_instance_type_leaves_no_type() {
    failed_read_back_case(Some(AFTER));
}

#[test]
fn a_failed_read_back_without_an_instance_type_removes_the_type_too() {
    // The one way a deploy that passed none loses the type (pin P30).
    failed_read_back_case(None);
}

// ------------------------------------------------------------------ P22: no InstanceType parameter

/// A read-back `answer` that holds no one `InstanceType` of the form: the deploy fails, writes the
/// instance ID it read alone, leaves no type and does not go on to sync.
fn p22_case(answer: String) {
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.read_back_answers(&answer);
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), None),
            "deploy -Host",
        );
        rig.same(
            "The failure's message",
            &f.message,
            &format!("agentspace: Stack '{stack}' has no InstanceType parameter."),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_id.as_deref() == Some(ID2) && record.instance_type.is_none(),
            "The instance ID it read MUST still be written, alone: the record holds no type",
        );
        rig.check(
            record.state.as_str() == "ready"
                && rig.log().remote_calls().is_empty()
                && aws_named(&rig.log(), "describe-instances").is_empty(),
            "deploy MUST NOT go on to sync",
        );
    });
}

fn answer_with(parameters: Option<&str>) -> String {
    json_stack_with(Some("CREATE_COMPLETE"), Some(ID2), parameters)
}

fn answer_with_value(value_member: &str) -> String {
    answer_with(Some(&format!(
        "[{{\"ParameterKey\":\"InstanceType\"{value_member}}}]"
    )))
}

#[test]
fn an_answer_with_no_parameters_fails_the_deploy() {
    p22_case(answer_with(None));
}

#[test]
fn an_answer_with_no_element_keyed_instance_type_fails_the_deploy() {
    p22_case(answer_with(Some(&json_params(&[
        ("RootVolumeGiB", "8"),
        ("SubnetId", "SUBNET_VALUE"),
    ]))));
}

#[test]
fn an_answer_with_two_instance_type_elements_fails_the_deploy() {
    p22_case(answer_with(Some(&json_params(&[
        ("InstanceType", "m9g.xlarge"),
        ("RootVolumeGiB", "8"),
        ("InstanceType", "m9g.xlarge"),
    ]))));
}

#[test]
fn an_instance_type_element_with_no_value_fails_the_deploy() {
    p22_case(answer_with_value(""));
}

#[test]
fn an_instance_type_value_that_is_not_a_string_fails_the_deploy() {
    p22_case(answer_with_value(",\"ParameterValue\":3"));
}

#[test]
fn an_instance_type_value_in_capitals_fails_the_deploy() {
    p22_case(answer_with_value(",\"ParameterValue\":\"M9G.XLARGE\""));
}

#[test]
fn an_instance_type_value_with_no_dot_fails_the_deploy() {
    p22_case(answer_with_value(",\"ParameterValue\":\"xlarge\""));
}

#[test]
fn an_instance_type_value_ending_in_a_line_feed_fails_the_deploy() {
    // The schema's `$(?!\n)` refuses it; `\u000a` is a JSON escape the stand-in leaves alone.
    p22_case(answer_with_value(
        ",\"ParameterValue\":\"m9g.xlarge\\u000a\"",
    ));
}

#[test]
fn an_instance_type_value_over_64_characters_fails_the_deploy() {
    let long = format!("{}.{}", "a".repeat(30), "b".repeat(34));
    assert_eq!(long.len(), 65);
    p22_case(answer_with_value(&format!(
        ",\"ParameterValue\":\"{long}\""
    )));
}

#[test]
fn a_missing_instance_id_is_reported_before_a_missing_instance_type() {
    // `InstanceId` is checked first, as before (operator.md, deploy, step 5).
    let rig = host_rig(true);
    let (stack, _) = names();
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.read_back_answers(&json_stack_with(Some("CREATE_COMPLETE"), None, None));
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), None),
            "deploy -Host",
        );
        rig.same(
            "The failure's message",
            &f.message,
            &format!("agentspace: Stack '{stack}' has no InstanceId output."),
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.instance_id.is_none() && record.instance_type.is_none(),
            "A stack with no InstanceId output MUST write nothing: neither an instance ID nor a type",
        );
    });
}

#[test]
fn the_boxs_deploy_fails_for_a_stack_with_no_instance_type_parameter() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.rule(
        "aws",
        0,
        "describe-stacks",
        0,
        &json_stack_with(Some("CREATE_COMPLETE"), Some(ID2), None),
        "",
    );
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_deploy(&say), "deploy");
        rig.same(
            "The failure's message",
            &f.message,
            "agentspace: Stack 'agentbox' has no InstanceType parameter.",
        );
        let record = rig.record("swimcoach");
        rig.check(
            record.instance_id.as_deref() == Some(ID2) && record.instance_type.is_none(),
            "The instance ID MUST be written alone, and the box's record hold no type",
        );
        rig.check(
            rig.log().remote_calls().is_empty(),
            "deploy MUST NOT go on to sync",
        );
    });
}

// ------------------------------------------------------------------ The box

/// A rig for the box's deploy: a CA, a key, a parameters file that names `InstanceType` (a value no
/// stack holds and no call may carry) and `RootVolumeGiB`, and a registry whose box record holds
/// `BEFORE`. Every stand-in watches the registry.
fn box_rig() -> Rig {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(
        ".agentspace/stack-parameters.json",
        format!(
            "{{\"version\":0,\"parameters\":{{\"SubnetId\":\"SUBNET_VALUE\",\"VpcId\":\"VPC_VALUE\",\"InstanceType\":\"{FILE_TYPE}\",\"RootVolumeGiB\":\"40\"}}}}"
        )
        .as_bytes(),
    );
    rig.seed_hosts_box(Seed::new("swimcoach", "ready").id(ID).itype(BEFORE), &[]);
    rig.stack_status_typed("CREATE_COMPLETE", AFTER);
    watch_registry(&rig);
    rig
}

fn box_overrides(extra: &[String]) -> Vec<String> {
    let mut v = vec![
        format!("SshPublicKey={KEY_LINE}"),
        "SubnetId=SUBNET_VALUE".to_string(),
        "VpcId=VPC_VALUE".to_string(),
        "RootVolumeGiB=40".to_string(),
    ];
    v.extend(extra.iter().cloned());
    v
}

fn nothing_carries_the_files_type(rig: &Rig, log: &Log) {
    rig.check(
        log.all_calls()
            .iter()
            .all(|c| c.args.iter().all(|a| !a.contains(FILE_TYPE))),
        "stack-parameters.json's InstanceType MUST reach no call: it is not passed and not read",
    );
}

#[test]
fn the_boxs_plain_deploy_passes_no_instance_type_and_reads_none_from_the_file() {
    let rig = box_rig();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        let log = rig.log();
        let aws = log.calls("aws");
        let template = template_arg(&aws[0].args);
        rig.same_list(
            "The box's cloudformation deploy call",
            &aws[0].args,
            &deploy_call_args("agentbox", &template, &box_overrides(&[])),
        );
        nothing_carries_the_files_type(rig, &log);
        rig.same_list(
            "The read-back",
            &aws[1].args,
            &describe_stacks_args("agentbox"),
        );
        rig.check(
            aws[2].args.get(1).map(String::as_str) == Some("describe-instances"),
            "The box's deploy MUST make no AWS call that it did not before v0.09",
        );
        let at_deploy = record_at(&log, aws[0].n, "swimcoach");
        rig.check(
            names_type(&at_deploy, BEFORE),
            &format!("A deploy without -InstanceType MUST leave the box's type in place until its read-back: {at_deploy}"),
        );
        let record = rig.record("swimcoach");
        rig.check(
            record.instance_type.as_deref() == Some(AFTER)
                && record.instance_id.as_deref() == Some(ID2),
            "The box's record MUST hold the type the stack reports after the deploy",
        );
    });
}

/// `deploy -InstanceType <type>` for the box, by either spelling of its target.
fn box_with_a_type_case(target: Target<'static>) {
    let rig = box_rig();
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, target, Some(AFTER)),
            "deploy -InstanceType for the box",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        let template = template_arg(&aws[0].args);
        rig.same_list(
            "The box's cloudformation deploy call, InstanceType last",
            &aws[0].args,
            &deploy_call_args(
                "agentbox",
                &template,
                &box_overrides(&[format!("InstanceType={AFTER}")]),
            ),
        );
        nothing_carries_the_files_type(rig, &log);
        let at_deploy = record_at(&log, aws[0].n, "swimcoach");
        rig.check(
            !has(&at_deploy, "instance_type") && at_deploy.contains(&format!("\"instance_id\":\"{ID}\"")),
            &format!("The box's type MUST be removed before cloudformation deploy runs, changing nothing else: {at_deploy}"),
        );
        rig.check(
            rig.record("swimcoach").instance_type.as_deref() == Some(AFTER),
            "The box's record MUST hold the type the stack reports",
        );
    });
}

#[test]
fn the_boxs_deploy_with_an_instance_type_and_no_host_passes_it() {
    box_with_a_type_case(Target::Box);
}

#[test]
fn the_boxs_deploy_with_an_instance_type_and_its_own_project_passes_it() {
    box_with_a_type_case(Target::Host("swimcoach"));
}

#[test]
fn a_missing_registry_is_written_as_the_seed_before_the_boxs_deploy_that_passes_a_type() {
    // A missing hosts.json reads as the box's record at m9g.xlarge, so removing its type writes
    // the file: the seed document's record, with no instance_type (operator.md, deploy, step 4;
    // pins P2c, P11 and P31).
    let rig = box_rig();
    rig.remove_home(".agentspace/hosts.json");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Box, Some(AFTER)),
            "deploy -InstanceType for the box",
        );
        let log = rig.log();
        let aws = log.calls("aws");
        let at_deploy = log.watched("aws", aws[0].n, "hosts.json").unwrap_or("<none>");
        rig.same(
            "The registry when cloudformation deploy starts",
            at_deploy,
            "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\\n",
        );
        let record = rig.record("swimcoach");
        rig.check(
            record.instance_type.as_deref() == Some(AFTER) && record.instance_id.as_deref() == Some(ID2),
            "The box's record MUST end holding the type and the instance ID the stack reports",
        );
    });
}

#[test]
fn a_missing_registry_is_not_written_before_the_boxs_deploy_that_passes_no_type() {
    let rig = box_rig();
    rig.remove_home(".agentspace/hosts.json");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        let log = rig.log();
        let aws = log.calls("aws");
        rig.same(
            "The registry when cloudformation deploy starts",
            log.watched("aws", aws[0].n, "hosts.json")
                .unwrap_or("<none>"),
            "<missing>",
        );
        let record = rig.record("swimcoach");
        rig.check(
            record.instance_type.as_deref() == Some(AFTER)
                && record.instance_id.as_deref() == Some(ID2),
            "The deploy MUST record the type and the instance ID the stack reports",
        );
    });
}

// ------------------------------------------------------------------ The provision lock

/// Whether every probe of the lock file whose name ends `file_name` found it held (`want`) or
/// free, over at least one call.
fn probes_all(log: &Log, file_name: &str, want: bool) -> bool {
    let probes = log.probed(file_name);
    !probes.is_empty() && probes.iter().all(|p| p.2 == want)
}

#[test]
fn a_hosts_deploy_holds_its_provision_lock_for_its_whole_life_and_no_other_lock_during_a_call() {
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.stack_status_typed("CREATE_COMPLETE", AFTER);
    rig.probe_lock(agentspace_hosts::host_run_lock_path(&rig.home, PROJECT));
    rig.probe_lock(agentspace_hosts::hosts_lock_path(&rig.home));
    rig.probe_lock(agentspace_state_lock_path(&rig));
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(AFTER)),
            "deploy -Host -InstanceType",
        );
        let log = rig.log();
        rig.check(
            probes_all(&log, "provision.lock", true),
            "provision.lock MUST be held at every program a deploy starts, its sync's included",
        );
        rig.check(
            log.probed("provision.lock").len() == log.all_calls().len(),
            "Every program the deploy started MUST have been probed",
        );
        rig.check(
            probes_all(&log, "hosts.lock", false),
            "hosts.lock MUST NOT be held during a call to AWS or a program of sync",
        );
        rig.check(
            log.probed("leases.lock").iter().all(|p| !p.2),
            "A deploy never takes the lease state lock",
        );
        rig.check(
            matches!(
                agentspace_hosts::try_run_lock(&rig.home, PROJECT),
                Ok(Some(_))
            ),
            "provision.lock MUST be released when deploy ends",
        );
        rig.check(
            rig.provision_log(PROJECT).unwrap_or_default().is_empty(),
            "deploy MUST write no line to provision.log, which is the run's alone",
        );
    });
}

fn agentspace_state_lock_path(rig: &Rig) -> std::path::PathBuf {
    rig.home.join(".agentspace").join("leases.lock")
}

#[test]
fn the_boxs_deploy_holds_swimcoachs_provision_lock_for_its_whole_life_too() {
    let rig = box_rig();
    rig.probe_lock(agentspace_hosts::host_run_lock_path(&rig.home, "swimcoach"));
    rig.probe_lock(agentspace_hosts::hosts_lock_path(&rig.home));
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        let log = rig.log();
        rig.check(
            probes_all(&log, "provision.lock", true)
                && log.probed("provision.lock").len() == log.all_calls().len(),
            "provision.lock MUST be held at every program the box's deploy starts",
        );
        rig.check(
            probes_all(&log, "hosts.lock", false),
            "hosts.lock MUST NOT be held during a call to AWS",
        );
        rig.check(
            matches!(
                agentspace_hosts::try_run_lock(&rig.home, "swimcoach"),
                Ok(Some(_))
            ),
            "provision.lock MUST be released when the box's deploy ends",
        );
    });
}

/// The message a deploy prints for a held `provision.lock`, for `project`.
fn held_message(project: &str) -> String {
    format!(
        "agentspace: A provisioning run or deploy for '{project}' is already running; the deploy did not run."
    )
}

/// A deploy of `project` (`target`) while another program holds its `provision.lock`: refused at
/// once, with no AWS call, no other lock, no file written and no line in `provision.log`.
fn refused_while_held(rig: &Rig, project: &str, target: Target<'_>, instance_type: Option<&str>) {
    let before = rig.hosts_bytes();
    let held = match agentspace_hosts::try_run_lock(&rig.home, project) {
        Ok(Some(lock)) => lock,
        _ => panic!("The throwaway run lock could not be taken"),
    };
    let entries_before = rig.operator_entries();
    let say = rig.say();
    let f = rig.failure(
        rig.do_deploy_sized(&say, target, instance_type),
        "deploy while provision.lock is held",
    );
    rig.same("The refusal", &f.message, &held_message(project));
    rig.check(
        rig.log().all_calls().is_empty(),
        "A refused deploy MUST make no AWS call and start no program",
    );
    rig.check(
        rig.hosts_bytes() == before,
        "A refused deploy MUST change nothing in the registry, a removal of the type included",
    );
    rig.check(
        rig.provision_log(project).unwrap_or_default().is_empty(),
        "A refused deploy MUST write no line to provision.log, which is the run's alone",
    );
    rig.check(
        rig.operator_entries() == entries_before && say.events.borrow().is_empty(),
        "A refused deploy MUST write no file of the operator's and print nothing",
    );
    drop(held);
    rig.check(
        matches!(
            agentspace_hosts::try_run_lock(&rig.home, project),
            Ok(Some(_))
        ),
        "The refused deploy MUST NOT leave the lock held",
    );
}

#[test]
fn a_hosts_deploy_is_refused_while_a_run_holds_its_provision_lock() {
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.stack_status("CREATE_COMPLETE");
    rig.run(|rig| refused_while_held(rig, PROJECT, Target::Host(PROJECT), None));
}

#[test]
fn a_hosts_deploy_with_an_instance_type_is_refused_before_it_removes_the_type() {
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.stack_status("CREATE_COMPLETE");
    rig.run(|rig| refused_while_held(rig, PROJECT, Target::Host(PROJECT), Some(AFTER)));
}

#[test]
fn the_boxs_deploy_is_refused_while_a_run_holds_swimcoachs_provision_lock() {
    let rig = box_rig();
    rig.run(|rig| refused_while_held(rig, "swimcoach", Target::Box, None));
}

#[test]
fn the_boxs_deploy_with_an_instance_type_is_refused_while_the_lock_is_held() {
    let rig = box_rig();
    rig.run(|rig| refused_while_held(rig, "swimcoach", Target::Host("swimcoach"), Some(AFTER)));
}

#[test]
fn a_held_lock_refuses_a_deploy_before_its_missing_public_key_does() {
    // "A `deploy` that would fail at step 1 on a missing public key is refused for the lock first"
    // (operator.md, deploy; pin P34).
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.remove_home(".ssh/id_ed25519.pub");
    rig.run(|rig| refused_while_held(rig, PROJECT, Target::Host(PROJECT), None));
}

#[test]
fn a_provision_lock_that_cannot_be_taken_for_another_reason_fails_the_deploy_before_any_call() {
    // A folder stands where the lock file belongs (state.md, The provisioning run's lock).
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).itype(BEFORE)]);
    rig.mkdir_home(".agentspace/hosts/demo/provision.lock");
    rig.run(|rig| {
        let before = rig.hosts_bytes();
        let say = rig.say();
        let f = rig.failure(
            rig.do_deploy_sized(&say, Target::Host(PROJECT), Some(AFTER)),
            "deploy -Host",
        );
        let path = agentspace_hosts::host_run_lock_path(&rig.home, PROJECT);
        let prefix = format!("agentspace: Cannot take {}: ", path.display());
        rig.check(
            norm(&f.message).starts_with(&norm(&prefix))
                && f.message.ends_with('.')
                && !f.message.contains('\n'),
            &format!(
                "The failure MUST be `agentspace: Cannot take <path>: <error>.`; it was `{}`",
                f.message
            ),
        );
        rig.check(
            rig.log().all_calls().is_empty() && rig.hosts_bytes() == before,
            "No AWS call and no registry change MUST precede the failure",
        );
    });
}

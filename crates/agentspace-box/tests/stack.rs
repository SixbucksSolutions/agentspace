//! The stack's name as a parameter (docs/operator.md, Which host a command acts on; D44, D55;
//! story E0.07-T02-S04): every AWS call and every message that names a stack names the one it was
//! given, and with the box's `agentbox` each is byte for byte what it was before v0.07.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Nothing here touches a file: the fake
//! cache gives what a read would, and records each write.

mod common;
use common::*;

const STACKS: &str = "cloudformation describe-stacks";
const DESCRIBE: &str = "ec2 describe-instances";
const START: &str = "ec2 start-instances";
const INFO: &str = "ssm describe-instance-information";
const SEND: &str = "ssm send-command";
const POLL: &str = "ssm get-command-invocation";

/// A host that is not the box (D44: `agentspace-host-<project>`).
const HOST_STACK: &str = "agentspace-host-agentspace";
const BOX_STACK: &str = "agentbox";

fn cache_json(id: &str) -> Vec<u8> {
    format!(r#"{{"version":0,"instance_id":"{id}"}}"#).into_bytes()
}

fn stack_with_id(stack: &str, id: &str) -> Result<Value, AwsError> {
    Ok(json!({"Stacks":[{"StackName":stack,"Outputs":[
        {"OutputKey":"InstanceId","OutputValue":id}
    ]}]}))
}

fn no_output(stack: &str) -> String {
    format!("agentspace: Stack '{stack}' has no InstanceId output.")
}

#[test]
fn the_rebuild_asks_for_the_stack_it_was_given() {
    for stack in [HOST_STACK, BOX_STACK, "agentspace-host-other"] {
        let sim = Sim::new(Mode::Person);
        sim.script(STACKS, vec![stack_with_id(stack, ID)]);
        assert_eq!(
            api::rebuild_cache_for(&sim.aws, stack),
            Ok((ID.to_string(), cache_json(ID)))
        );
        let calls = sim.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].text(),
            format!("cloudformation describe-stacks --stack-name {stack}")
        );
        assert_eq!(calls[0].output, Output::Json);
    }
}

#[test]
fn the_box_forms_ask_for_agentbox_as_before() {
    let sim = Sim::new(Mode::Person);
    sim.script(STACKS, vec![stack_with_id(BOX_STACK, ID)]);
    assert_eq!(
        api::rebuild_cache(&sim.aws),
        Ok((ID.to_string(), cache_json(ID)))
    );
    assert_eq!(
        sim.calls()[0].text(),
        "cloudformation describe-stacks --stack-name agentbox"
    );
}

#[test]
fn a_stack_with_no_instance_id_output_fails_naming_the_stack_it_was_given() {
    let answers = [
        Ok(json!({"Stacks":[{"Outputs":[]}]})),
        Ok(json!({"Stacks":[{}]})),
        Ok(json!({"Stacks":[]})),
        Ok(json!({})),
        Ok(json!({"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":"not an ID"}]}]})),
    ];
    for answer in answers {
        let sim = Sim::new(Mode::Person);
        sim.script(STACKS, vec![answer.clone()]);
        let failure = api::rebuild_cache_for(&sim.aws, HOST_STACK).unwrap_err();
        assert_eq!(failure.message, no_output(HOST_STACK), "{answer:?}");
        assert!(!failure.message.contains(BOX_STACK), "{answer:?}");
        assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{answer:?}");
    }
}

#[test]
fn a_failed_describe_stacks_fails_with_the_aws_failure_whatever_the_stack() {
    for error in [
        AwsError::SignIn,
        AwsError::Failed(format!("Stack with id {HOST_STACK} does not exist")),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(STACKS, vec![Err(error.clone())]);
        assert_eq!(
            api::rebuild_cache_for(&sim.aws, HOST_STACK),
            Err(Failure::from(error))
        );
        assert_eq!(sim.count(STACKS), 1);
    }
}

#[test]
fn a_cache_that_counts_as_missing_is_rebuilt_from_the_stack_it_was_given() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(STACKS, vec![stack_with_id(HOST_STACK, ID)]);
        stopped_then_patched(&sim);
        let cache = FakeCache::new(&sim, CacheFile::Absent);
        assert_eq!(api::up_for(&sim, &cache, HOST_STACK), Ok(ID.to_string()));
        assert_eq!(sim.keys(), vec![STACKS, DESCRIBE, START, INFO, SEND, POLL]);
        assert_eq!(
            sim.calls_of(STACKS)[0].text(),
            format!("cloudformation describe-stacks --stack-name {HOST_STACK}")
        );
        assert_eq!(sim.cache_writes(), vec![cache_json(ID)]);
    });
}

#[test]
fn a_good_cache_asks_for_no_stack_whatever_its_name() {
    let sim = Sim::new(Mode::Person);
    stopped_then_patched(&sim);
    let cache = FakeCache::new(&sim, CacheFile::Bytes(cache_json(ID)));
    assert_eq!(api::up_for(&sim, &cache, HOST_STACK), Ok(ID.to_string()));
    assert_eq!(sim.keys(), vec![DESCRIBE, START, INFO, SEND, POLL]);
    assert!(sim.cache_writes().is_empty());
}

#[test]
fn up_with_no_instance_id_output_names_the_stack_and_asks_nothing_of_the_box() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(STACKS, vec![Ok(json!({"Stacks":[{"Outputs":[]}]}))]);
        stopped_then_patched(&sim);
        let cache = FakeCache::new(&sim, CacheFile::Absent);
        let failure = api::up_for(&sim, &cache, HOST_STACK).unwrap_err();
        assert_eq!(failure.message, no_output(HOST_STACK));
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        // A caller never sees the stack's name (operator.md, What a caller sees).
        assert_eq!(
            render_failure(Mode::Caller, &failure),
            "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why."
        );
        assert_eq!(sim.keys(), vec![STACKS]);
        assert!(sim.cache_writes().is_empty());
        assert!(sim.person_lines().is_empty());
    });
}

#[test]
fn a_host_and_the_box_differ_only_in_the_stack_in_every_call_and_message() {
    let run = |stack: &str| {
        let sim = Sim::new(Mode::Person);
        sim.script(STACKS, vec![stack_with_id(stack, ID)]);
        stopped_then_patched(&sim);
        let cache = FakeCache::new(&sim, CacheFile::Absent);
        assert_eq!(api::up_for(&sim, &cache, stack), Ok(ID.to_string()));
        let calls: Vec<String> = sim.calls().iter().map(|c| c.text()).collect();
        (calls, sim.person_lines(), sim.cache_writes())
    };
    let (box_calls, box_lines, box_writes) = run(BOX_STACK);
    let (host_calls, host_lines, host_writes) = run(HOST_STACK);
    assert_eq!(box_lines, host_lines);
    assert_eq!(box_writes, host_writes);
    assert_eq!(box_calls.len(), host_calls.len());
    for (b, h) in box_calls.iter().zip(&host_calls) {
        assert_eq!(&b.replace(BOX_STACK, HOST_STACK), h);
    }
    // No call or line of the host's names the box's stack.
    for text in host_calls.iter().chain(&host_lines) {
        assert!(!text.contains(BOX_STACK), "{text}");
    }
}

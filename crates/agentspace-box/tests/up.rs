//! `up`'s steps 1 and 2 together (docs/operator.md, up): the instance ID read from the cache, or
//! rebuilt from the stack when the cache counts as missing, the cache written then; and only then
//! the start-and-wait step. Every failure ends the steps at once.
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

const NO_OUTPUT: &str = "agentspace: Stack 'agentbox' has no InstanceId output.";
const COULD_NOT_REACH: &str = "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.";

fn cache_json(id: &str) -> Vec<u8> {
    format!(r#"{{"version":0,"instance_id":"{id}"}}"#).into_bytes()
}

fn stack_with_id(id: &str) -> Result<Value, AwsError> {
    Ok(json!({"Stacks":[{"StackName":"agentbox","Outputs":[
        {"OutputKey":"InstanceId","OutputValue":id}
    ]}]}))
}

fn cache_events(sim: &Sim) -> usize {
    sim.events()
        .iter()
        .filter(|e| matches!(e, Event::CacheRead | Event::CacheWrite(_)))
        .count()
}

#[test]
fn a_good_cache_gives_the_id_with_no_call_to_the_stack_and_no_write() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        stopped_then_patched(&sim);
        let cache = FakeCache::new(&sim, CacheFile::Bytes(cache_json(ID)));
        assert_eq!(api::up(&sim, &cache), Ok(ID.to_string()));
        assert_eq!(sim.keys(), vec![DESCRIBE, START, INFO, SEND, POLL]);
        assert!(sim.cache_writes().is_empty());
        // The cache is read once, before anything is asked of AWS.
        assert_eq!(sim.events()[0], Event::CacheRead);
        assert_eq!(cache_events(&sim), 1);
        assert_eq!(
            sim.person_lines(),
            vec![format!("Starting {ID}..."), format!("Ready: {ID}")]
        );
    });
}

#[test]
fn the_cached_id_is_the_one_every_call_names() {
    // An ID of the schema's other length, so that no call could have taken it from elsewhere.
    let id = "i-0123abcd";
    let sim = Sim::new(Mode::Person);
    stopped_then_patched(&sim);
    let cache = FakeCache::new(&sim, CacheFile::Bytes(cache_json(id)));
    assert_eq!(api::up(&sim, &cache), Ok(id.to_string()));
    for call in sim.calls() {
        assert!(call.text().contains(id), "{}", call.text());
    }
    assert_eq!(
        sim.person_lines(),
        vec![format!("Starting {id}..."), format!("Ready: {id}")]
    );
}

#[test]
fn a_cache_that_counts_as_missing_is_rebuilt_from_the_stack_and_written_before_the_box_is_asked() {
    let missing: Vec<(&str, CacheFile)> = vec![
        ("No file", CacheFile::Absent),
        ("Empty", CacheFile::Bytes(Vec::new())),
        ("Only whitespace", CacheFile::Bytes(b" \r\n\t".to_vec())),
        (
            "Only a byte order mark",
            CacheFile::Bytes(vec![0xEF, 0xBB, 0xBF]),
        ),
        ("Not JSON", CacheFile::Bytes(b"{nope".to_vec())),
        (
            "The prototype's plain ID",
            CacheFile::Bytes(ID.as_bytes().to_vec()),
        ),
        (
            "Off its schema",
            CacheFile::Bytes(br#"{"version":1,"instance_id":"i-0123456789abcdef0"}"#.to_vec()),
        ),
        // The stale ID of a cache that does not match its schema is never used.
        (
            "An extra member",
            CacheFile::Bytes(
                br#"{"version":0,"instance_id":"i-0000000000000000a","x":1}"#.to_vec(),
            ),
        ),
    ];
    for (name, file) in missing {
        both_modes(|mode| {
            let sim = Sim::new(mode);
            sim.script(STACKS, vec![stack_with_id(ID)]);
            stopped_then_patched(&sim);
            let cache = FakeCache::new(&sim, file.clone());
            assert_eq!(api::up(&sim, &cache), Ok(ID.to_string()), "{name}");
            assert_eq!(
                sim.keys(),
                vec![STACKS, DESCRIBE, START, INFO, SEND, POLL],
                "{name}"
            );
            // The cache's bytes exactly, written once, after the stack answered and before the
            // first call that names the instance.
            assert_eq!(sim.cache_writes(), vec![cache_json(ID)], "{name}");
            let written = sim.index_of(|e| matches!(e, Event::CacheWrite(_)));
            assert_before(sim.index_of_call(STACKS), written);
            assert_before(written, sim.index_of_call(DESCRIBE));
            let stacks = &sim.calls_of(STACKS)[0];
            assert_eq!(
                stacks.text(),
                "cloudformation describe-stacks --stack-name agentbox"
            );
            assert_eq!(stacks.output, Output::Json);
        });
    }
}

#[test]
fn a_cache_that_cannot_be_opened_fails_with_fa16s_message_and_asks_nothing() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(STACKS, vec![stack_with_id(ID)]);
        stopped_then_patched(&sim);
        let cache = FakeCache::new(&sim, CacheFile::Denied);
        let failure = api::up(&sim, &cache).unwrap_err();
        assert_eq!(
            failure.message,
            format!(
                "agentspace: Cannot read the instance-ID cache {}; remove it and run the command again.",
                cache.full_path().display()
            )
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        assert_eq!(render_failure(Mode::Caller, &failure), COULD_NOT_REACH);
        // It is not rebuilt: no call, no write, no message.
        assert!(sim.calls().is_empty());
        assert!(sim.cache_writes().is_empty());
        assert!(sim.person_lines().is_empty());
    });
}

#[test]
fn a_stack_with_no_instance_id_output_fails_writes_no_cache_and_asks_nothing_of_the_box() {
    let answers = [
        Ok(json!({"Stacks":[{"StackName":"agentbox","Outputs":[]}]})),
        Ok(json!({"Stacks":[{"StackName":"agentbox"}]})),
        Ok(json!({"Stacks":[]})),
        Ok(json!({})),
        Ok(json!({"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":""}]}]})),
        Ok(json!({"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":"not an ID"}]}]})),
    ];
    for answer in answers {
        both_modes(|mode| {
            let sim = Sim::new(mode);
            sim.script(STACKS, vec![answer.clone()]);
            stopped_then_patched(&sim);
            let cache = FakeCache::new(&sim, CacheFile::Absent);
            let failure = api::up(&sim, &cache).unwrap_err();
            assert_eq!(failure.message, NO_OUTPUT, "{answer:?}");
            assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{answer:?}");
            assert_eq!(render_failure(Mode::Caller, &failure), COULD_NOT_REACH);
            assert_eq!(sim.keys(), vec![STACKS], "{answer:?}");
            assert!(sim.cache_writes().is_empty(), "{answer:?}");
            assert!(sim.person_lines().is_empty(), "{answer:?}");
        });
    }
}

#[test]
fn a_describe_stacks_that_fails_fails_under_aws_failures_and_writes_no_cache() {
    for error in [
        AwsError::SignIn,
        AwsError::Failed("Stack with id agentbox does not exist".to_string()),
        AwsError::CannotRun(
            "agentspace: AGENTSPACE_AWS_CMD names x, which cannot be run.".to_string(),
        ),
    ] {
        let sim = Sim::new(Mode::Caller);
        sim.script(STACKS, vec![Err(error.clone())]);
        stopped_then_patched(&sim);
        let cache = FakeCache::new(&sim, CacheFile::Absent);
        let failure = api::up(&sim, &cache).unwrap_err();
        assert_eq!(failure, Failure::from(error.clone()));
        assert_eq!(sim.keys(), vec![STACKS]);
        assert!(sim.cache_writes().is_empty());
        // Only #0034's sign-in message reaches a caller as it is.
        let shown = render_failure(Mode::Caller, &failure);
        if error == AwsError::SignIn {
            assert_eq!(shown, SIGN_IN_MESSAGE);
        } else {
            assert_eq!(shown, COULD_NOT_REACH);
        }
    }
}

#[test]
fn a_cache_that_cannot_be_written_fails_the_command_before_the_box_is_asked() {
    // What a caller sees: "a cache that cannot be read or written" is "Could not reach the host",
    // whatever the failure the program's write named.
    for named in [CallerFailure::CouldNotReach, CallerFailure::Same] {
        both_modes(|mode| {
            let sim = Sim::new(mode);
            sim.script(STACKS, vec![stack_with_id(ID)]);
            stopped_then_patched(&sim);
            let cache = FakeCache::new(&sim, CacheFile::Absent).failing_writes(Failure {
                message: "agentspace: Cannot write the instance-ID cache.".to_string(),
                caller: named,
            });
            let failure = api::up(&sim, &cache).unwrap_err();
            assert_eq!(
                failure.message,
                "agentspace: Cannot write the instance-ID cache."
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
            assert_eq!(render_failure(Mode::Caller, &failure), COULD_NOT_REACH);
            assert_eq!(sim.keys(), vec![STACKS]);
            assert_eq!(sim.cache_writes(), vec![cache_json(ID)]);
            assert!(sim.person_lines().is_empty());
        });
    }
}

#[test]
fn a_failure_of_the_step_after_a_rebuild_is_the_steps_own() {
    let sim = Sim::new(Mode::Person);
    sim.script(STACKS, vec![stack_with_id(ID)]);
    sim.script(DESCRIBE, vec![state("terminated")]);
    let cache = FakeCache::new(&sim, CacheFile::Absent);
    let failure = api::up(&sim, &cache).unwrap_err();
    assert_eq!(
        failure.message,
        format!("agentspace: Instance {ID} is terminated.")
    );
    assert_eq!(failure.caller, CallerFailure::NotAvailable);
    assert_eq!(sim.keys(), vec![STACKS, DESCRIBE]);
}

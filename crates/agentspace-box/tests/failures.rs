//! FB-08: which members of `ec2 describe-instances` the start-and-wait step reads, and the
//! "Unexpected answer" failure for each one that is missing or not of its form.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;

const DESCRIBE: &str = "ec2 describe-instances";
const START: &str = "ec2 start-instances";
const INFO: &str = "ssm describe-instance-information";

fn unexpected(member: &str) -> String {
    format!("agentspace: Unexpected answer from ec2 describe-instances: no {member}.")
}

const STATE_NAME: &str = "Reservations[0].Instances[0].State.Name";
const LAUNCH_TIME: &str = "Reservations[0].Instances[0].LaunchTime";

#[test]
fn a_first_answer_with_no_usable_state_name_fails_with_the_member_named() {
    let bad: Vec<(&str, Result<Value, AwsError>)> = vec![
        ("Null", Ok(Value::Null)),
        ("An array", Ok(json!([]))),
        ("An empty object", Ok(json!({}))),
        ("A string", Ok(json!("running"))),
        ("No reservations", Ok(json!({"Reservations":[]}))),
        ("Reservations not an array", Ok(json!({"Reservations":{}}))),
        (
            "A reservation with no instances",
            Ok(json!({"Reservations":[{}]})),
        ),
        (
            "Instances empty",
            Ok(json!({"Reservations":[{"Instances":[]}]})),
        ),
        (
            "Instances not an array",
            Ok(json!({"Reservations":[{"Instances":{}}]})),
        ),
        (
            "An instance with no state",
            Ok(json!({"Reservations":[{"Instances":[{}]}]})),
        ),
        (
            "State that is null",
            Ok(json!({"Reservations":[{"Instances":[{"State":null}]}]})),
        ),
        (
            "State with no name",
            Ok(json!({"Reservations":[{"Instances":[{"State":{}}]}]})),
        ),
        (
            "State with a code only",
            Ok(json!({"Reservations":[{"Instances":[{"State":{"Code":16}}]}]})),
        ),
        (
            "A name that is null",
            Ok(json!({"Reservations":[{"Instances":[{"State":{"Name":null}}]}]})),
        ),
        (
            "A name that is a number",
            Ok(json!({"Reservations":[{"Instances":[{"State":{"Name":16}}]}]})),
        ),
        (
            "A name that is an array",
            Ok(json!({"Reservations":[{"Instances":[{"State":{"Name":["running"]}}]}]})),
        ),
        // Only the first reservation and instance count.
        (
            "Only the second instance has a state",
            Ok(json!({"Reservations":[{"Instances":[{},{"State":{"Name":"running"}}]}]})),
        ),
        (
            "Only the second reservation has a state",
            Ok(
                json!({"Reservations":[{"Instances":[]},{"Instances":[{"State":{"Name":"running"}}]}]}),
            ),
        ),
    ];
    for (name, answer) in bad {
        both_modes(|mode| {
            let sim = Sim::new(mode);
            sim.script(DESCRIBE, vec![answer.clone()]);
            let failure = sim.run().unwrap_err();
            assert_eq!(failure.message, unexpected(STATE_NAME), "{name}");
            assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{name}");
            // It ends at once: nothing else is asked, and nothing is printed.
            assert_eq!(sim.keys(), vec![DESCRIBE], "{name}");
            assert!(sim.person_lines().is_empty(), "{name}");
        });
    }
}

#[test]
fn a_second_answer_with_no_usable_launch_time_fails_with_the_member_named() {
    let bad: Vec<(&str, Result<Value, AwsError>)> = vec![
        ("Null", Ok(Value::Null)),
        ("An empty object", Ok(json!({}))),
        ("No reservations", Ok(json!({"Reservations":[]}))),
        (
            "No instances",
            Ok(json!({"Reservations":[{"Instances":[]}]})),
        ),
        (
            "An instance with no launch time",
            Ok(json!({"Reservations":[{"Instances":[{}]}]})),
        ),
        (
            "State only",
            Ok(json!({"Reservations":[{"Instances":[{"State":{"Name":"running"}}]}]})),
        ),
        (
            "A launch time that is null",
            instance_with(json!({"LaunchTime":null})),
        ),
        (
            "A launch time that is a number",
            instance_with(json!({"LaunchTime":1795089600})),
        ),
        (
            "A launch time that is an array",
            instance_with(json!({"LaunchTime":["2026-11-19T11:00:00+00:00"]})),
        ),
        (
            "An empty launch time",
            instance_with(json!({"LaunchTime":""})),
        ),
        (
            "A launch time that is not a time",
            instance_with(json!({"LaunchTime":"yesterday"})),
        ),
        (
            "A launch time of text",
            instance_with(json!({"LaunchTime":"not a time"})),
        ),
        // `LaunchTime` with no UTC offset is a member not of its form (operator.md, Calls by
        // command, the architect's ruling of 2026-11-19 for E0.06-T01-S03).
        (
            "A launch time with no offset",
            instance_with(json!({"LaunchTime":"2026-11-19T11:00:00"})),
        ),
        (
            "A launch time with a fraction and no offset",
            instance_with(json!({"LaunchTime":"2026-11-19T11:00:00.000"})),
        ),
        (
            "A launch time with a space and no offset",
            instance_with(json!({"LaunchTime":"2026-11-19 11:00:00"})),
        ),
        (
            "A date alone",
            instance_with(json!({"LaunchTime":"2026-11-19"})),
        ),
        (
            "Only the second instance has a launch time",
            Ok(json!({"Reservations":[{"Instances":[{},{"LaunchTime":iso(3600, Style::Z)}]}]})),
        ),
    ];
    for (name, second) in bad {
        both_modes(|mode| {
            let sim = Sim::new(mode);
            sim.script(DESCRIBE, vec![state("running"), second.clone()]);
            let failure = sim.run().unwrap_err();
            assert_eq!(failure.message, unexpected(LAUNCH_TIME), "{name}");
            assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{name}");
            assert_eq!(sim.keys(), vec![DESCRIBE, DESCRIBE], "{name}");
            assert!(sim.person_lines().is_empty(), "{name}");
        });
    }
}

#[test]
fn a_launch_time_that_is_not_there_never_fails_a_box_that_does_not_read_it() {
    // The second call, and so LaunchTime, is read only for a running box (FB-09).
    for name in ["pending", "stopped", "stopping"] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state(name)]);
        sim.script("ec2 wait instance-stopped", vec![Ok(Value::Null)]);
        sim.script(START, vec![Ok(json!({}))]);
        online_with(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()), "{name}");
        assert_eq!(sim.count(DESCRIBE), 1, "{name}");
    }
}

#[test]
fn a_state_that_is_not_there_never_fails_the_launch_time_call() {
    // Missing, or of another form: the second call reads LaunchTime alone.
    for members in [
        json!({"LaunchTime":iso(3600, Style::Plus00)}),
        json!({"State":null,"LaunchTime":iso(3600, Style::Plus00)}),
        json!({"State":{"Name":7},"LaunchTime":iso(3600, Style::Plus00)}),
        json!({"State":{"Name":"stopped"},"LaunchTime":iso(3600, Style::Plus00)}),
        json!({"State":{"Name":"terminated"},"LaunchTime":iso(3600, Style::Plus00),"InstanceType":5}),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(
            DESCRIBE,
            vec![state("running"), instance_with(members.clone())],
        );
        assert_eq!(sim.run(), Ok(()), "{members}");
        // The state of the second answer is not read, so the first one decided: no start.
        assert_eq!(sim.count(START), 0, "{members}");
        assert_eq!(sim.count(INFO), 0, "{members}");
    }
}

#[test]
fn members_other_commands_read_never_fail_this_one() {
    // status and grow read InstanceType, RootDeviceName and the block device mappings; the
    // start-and-wait step does not, whether missing or of another form.
    let sim = Sim::new(Mode::Person);
    sim.script(
        DESCRIBE,
        vec![
            instance_with(json!({
                "State":{"Name":"running"},
                "InstanceType":false,
                "RootDeviceName":["/dev/sda1"],
                "BlockDeviceMappings":"none"
            })),
            instance_with(json!({
                "LaunchTime":iso(3600, Style::Plus00),
                "InstanceType":{},
                "RootDeviceName":null,
                "BlockDeviceMappings":[{"DeviceName":"/dev/other"}]
            })),
        ],
    );
    assert_eq!(sim.run(), Ok(()));
}

#[test]
fn the_first_call_reads_no_launch_time_even_for_a_running_box() {
    // Step 1 reads State.Name only: a first answer for a running box with no LaunchTime is fine,
    // since the second call is the one that reads it.
    let sim = Sim::new(Mode::Person);
    sim.script(
        DESCRIBE,
        vec![state("running"), launched(&iso(3600, Style::Plus00))],
    );
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.count(DESCRIBE), 2);
}

#[test]
fn the_unexpected_answer_message_is_one_line_and_names_no_instance() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![Ok(json!({}))]);
    let failure = sim.run().unwrap_err();
    assert!(!failure.message.contains('\n'));
    assert!(!failure.message.contains(ID));
}

// -- The same form for the wait's other calls (operator.md, Calls by command, the architect's
// rulings of 2026-11-19 for E0.06-T01-S03: "The same form holds for every call's members") -----------

#[test]
fn an_agent_answer_with_no_usable_ping_status_fails_with_the_member_named() {
    // The member is named by its path as the table writes it,
    // `InstanceInformationList[0].PingStatus`, whichever part of it is missing. Only a list that
    // is there and empty is "not online".
    let bad = [
        Value::Null,
        json!({}),
        json!([]),
        json!({"InstanceInformationList":null}),
        json!({"InstanceInformationList":{}}),
        json!({"InstanceInformationList":"Online"}),
        json!({"InstanceInformationList":[{}]}),
        json!({"InstanceInformationList":[null]}),
        json!({"InstanceInformationList":["Online"]}),
        json!({"InstanceInformationList":[{"PingStatus":null}]}),
        json!({"InstanceInformationList":[{"PingStatus":1}]}),
        json!({"InstanceInformationList":[{"PingStatus":["Online"]}]}),
        // Only the first element counts.
        json!({"InstanceInformationList":[{},{"PingStatus":"Online"}]}),
    ];
    for answer in bad {
        both_modes(|mode| {
            let sim = Sim::new(mode);
            sim.script(DESCRIBE, vec![state("pending")]);
            sim.script(INFO, vec![Ok(answer.clone())]);
            let failure = sim.run().unwrap_err();
            assert_eq!(
                failure.message,
                "agentspace: Unexpected answer from ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
                "{answer}"
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{answer}");
            assert_eq!(sim.keys(), vec![DESCRIBE, INFO], "{answer}");
            assert!(sim.person_lines().is_empty(), "{answer}");
        });
    }
}

#[test]
fn members_of_the_agents_answer_that_are_not_read_never_fail_the_wait() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    sim.script(
        INFO,
        vec![Ok(json!({
            "NextToken": 7,
            "InstanceInformationList":[
                {"PingStatus":"Online","InstanceId":null,"PlatformName":["x"],"AgentVersion":3},
                "not an object"
            ]
        }))],
    );
    patch_words(&sim, &["patched"]);
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.count(INFO), 1);
}

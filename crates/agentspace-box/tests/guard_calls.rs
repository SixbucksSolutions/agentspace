//! The guard's three AWS calls (story E0.08-T02-S03, card #0175): one `ec2 describe-instances` for
//! every host, `ec2 stop-instances` and the horizon's `ssm send-command`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Written from docs/operator.md alone, "guard" and "Calls by command" and "AWS failures", and from
//! D56, D57 and D63. Every call goes through the stand-in `Aws` of `common`, which records it, so
//! no test calls real AWS. The command text of the horizon's send is the operator's own (D34) and
//! operator.md does not give its bytes, so its tests hold what operator.md requires of it and no
//! more.

mod common;
use agentspace_box::{
    Described, DescribedState, HORIZON_COMMENT, describe_hosts, send_horizon, stop_host,
};
use common::*;

const DESCRIBE: &str = "ec2 describe-instances";
const STOP: &str = "ec2 stop-instances";
const SEND: &str = "ssm send-command";
const INVOCATION: &str = "ssm get-command-invocation";

const A: &str = "i-0123456789abcdef0";
const B: &str = "i-0123456789abcdef1";
const C: &str = "i-0123456789abcdef2";
const D: &str = "i-0123456789abcdef3";

/// 2026-11-19T08:12:34Z, in seconds since the Unix epoch.
const T0: i64 = 1_795_075_954;

// ---------------------------------------------------------------------------------------------
// Builders and readers.
// ---------------------------------------------------------------------------------------------

/// One element of `Reservations[].Instances[]`, with the members the guard reads.
fn element(id: &str, name: &str, launch: Option<&str>) -> Value {
    let mut member = json!({"InstanceId": id, "State": {"Name": name}});
    if let Some(time) = launch {
        member["LaunchTime"] = json!(time);
    }
    member
}

/// An answer whose reservations hold the given instances, one reservation per inner list.
fn answer(reservations: Vec<Vec<Value>>) -> Result<Value, AwsError> {
    let reservations: Vec<Value> = reservations
        .into_iter()
        .map(|instances| json!({"Instances": instances}))
        .collect();
    Ok(json!({"Reservations": reservations}))
}

/// A stand-in that answers `ec2 describe-instances` with `answers`. Any other call is unscripted,
/// and the stand-in fails the test on it.
fn sim_describing(answers: Vec<Result<Value, AwsError>>) -> Sim {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, answers);
    sim
}

fn sim_sending(answer: Result<Value, AwsError>) -> Sim {
    let sim = Sim::new(Mode::Person);
    sim.script(SEND, vec![answer]);
    // Scripted only so that a call to it is counted, never to be made.
    sim.script(INVOCATION, vec![Ok(json!({"Status": "Success"}))]);
    sim.script(
        "ssm describe-instance-information",
        vec![Ok(json!({"InstanceInformationList": []}))],
    );
    sim
}

fn sim_stopping(answer: Result<Value, AwsError>) -> Sim {
    let sim = Sim::new(Mode::Person);
    sim.script(STOP, vec![answer]);
    sim
}

fn show(state: &DescribedState) -> String {
    match state {
        DescribedState::Unknown => "Unknown".to_string(),
        DescribedState::Running { launch_time } => {
            format!("Running {{ launch_time: {launch_time} }}")
        }
        DescribedState::Other { state } => format!("Other {{ state: {state:?} }}"),
        DescribedState::Unexpected { member } => format!("Unexpected {{ member: {member:?} }}"),
    }
}

fn describe(sim: &Sim, ids: &[&str]) -> Vec<Described> {
    match describe_hosts(&sim.aws, ids) {
        Ok(described) => described,
        Err(failure) => panic!("describe_hosts failed: {}", failure.message),
    }
}

fn the<'a>(described: &'a [Described], id: &str) -> &'a DescribedState {
    let found: Vec<&Described> = described.iter().filter(|d| d.id == id).collect();
    assert_eq!(found.len(), 1, "Expected one result for the given ID {id}");
    &found[0].state
}

fn ids_of(described: &[Described]) -> Vec<String> {
    described.iter().map(|d| d.id.clone()).collect()
}

fn assert_running(described: &[Described], id: &str, expected: i64) {
    let state = the(described, id);
    assert!(
        matches!(state, DescribedState::Running { launch_time } if *launch_time == expected),
        "Expected Running with launch_time {expected} for {id}, got {}",
        show(state)
    );
}

fn assert_other(described: &[Described], id: &str, expected: &str) {
    let state = the(described, id);
    assert!(
        matches!(state, DescribedState::Other { state } if state == expected),
        "Expected Other with state {expected:?} for {id}, got {}",
        show(state)
    );
}

fn assert_unknown(described: &[Described], id: &str) {
    let state = the(described, id);
    assert!(
        matches!(state, DescribedState::Unknown),
        "Expected Unknown for {id}, got {}",
        show(state)
    );
}

fn assert_unexpected(described: &[Described], id: &str, member: &str) {
    let state = the(described, id);
    assert!(
        matches!(state, DescribedState::Unexpected { member: m } if m.contains(member)),
        "Expected Unexpected naming {member} for {id}, got {}",
        show(state)
    );
}

/// The one call the sim recorded, which MUST be `key`.
fn only_call(sim: &Sim, key: &str) -> Rec {
    let calls = sim.calls();
    assert_eq!(
        calls.len(),
        1,
        "Expected exactly one AWS call, got {:?}",
        sim.keys()
    );
    assert_eq!(calls[0].key(), key);
    calls[0].clone()
}

fn args(rec: &Rec) -> Vec<&str> {
    rec.args.iter().map(String::as_str).collect()
}

// ---------------------------------------------------------------------------------------------
// ec2 describe-instances, for every host at once.
// ---------------------------------------------------------------------------------------------

#[test]
fn one_describe_call_names_every_id_in_one_comma_joined_filter_argument() {
    let sim = sim_describing(vec![answer(vec![vec![
        element(A, "running", Some("2026-11-19T08:12:34+00:00")),
        element(B, "stopped", None),
        element(C, "running", Some("2026-11-19T08:12:34+00:00")),
    ]])]);
    describe(&sim, &[A, B, C]);
    let call = only_call(&sim, DESCRIBE);
    assert_eq!(call.service, "ec2");
    assert_eq!(call.operation, "describe-instances");
    assert_eq!(
        args(&call),
        vec![
            "--filters",
            "Name=instance-id,Values=i-0123456789abcdef0,i-0123456789abcdef1,i-0123456789abcdef2"
        ],
        "The filter is one argument, the IDs in the given order joined by commas"
    );
}

#[test]
fn the_ids_are_joined_in_the_order_given_not_sorted() {
    let sim = sim_describing(vec![answer(vec![vec![]])]);
    let _ = describe(&sim, &[C, A, D, B]);
    let call = only_call(&sim, DESCRIBE);
    assert_eq!(
        args(&call),
        vec![
            "--filters",
            "Name=instance-id,Values=i-0123456789abcdef2,i-0123456789abcdef0,i-0123456789abcdef3,i-0123456789abcdef1"
        ]
    );
}

#[test]
fn a_single_id_is_one_filter_with_no_comma() {
    let sim = sim_describing(vec![answer(vec![vec![element(A, "stopped", None)]])]);
    let described = describe(&sim, &[A]);
    let call = only_call(&sim, DESCRIBE);
    assert_eq!(
        args(&call),
        vec!["--filters", "Name=instance-id,Values=i-0123456789abcdef0"]
    );
    assert_eq!(ids_of(&described), vec![A.to_string()]);
}

#[test]
fn the_ids_are_never_named_in_instance_ids() {
    // EC2 refuses the whole of an --instance-ids call that names an unknown instance
    // (operator.md, guard, step 5).
    let sim = sim_describing(vec![answer(vec![vec![]])]);
    describe(&sim, &[A, B]);
    let call = only_call(&sim, DESCRIBE);
    assert!(
        !call.args.iter().any(|a| a == "--instance-ids"),
        "The call names the IDs with --instance-ids"
    );
}

#[test]
fn the_describe_call_is_read_as_json() {
    let sim = sim_describing(vec![answer(vec![vec![element(
        A,
        "running",
        Some("2026-11-19T08:12:34Z"),
    )]])]);
    describe(&sim, &[A]);
    assert_eq!(only_call(&sim, DESCRIBE).output, Output::Json);
}

#[test]
fn no_ids_make_no_call_and_give_no_results() {
    let sim = sim_describing(vec![answer(vec![vec![element(
        A,
        "running",
        Some("2026-11-19T08:12:34Z"),
    )]])]);
    let described = describe(&sim, &[]);
    assert!(described.is_empty(), "A result for no ID");
    assert!(sim.calls().is_empty(), "A call for no ID: {:?}", sim.keys());
}

#[test]
fn one_result_per_given_id_in_the_given_order_whatever_the_answers_order() {
    // The answer lists the instances in another order than the IDs were given.
    let sim = sim_describing(vec![answer(vec![vec![
        element(C, "stopped", None),
        element(A, "running", Some("2026-11-19T08:12:34Z")),
        element(B, "stopping", None),
    ]])]);
    let described = describe(&sim, &[A, B, C]);
    assert_eq!(
        ids_of(&described),
        vec![A.to_string(), B.to_string(), C.to_string()]
    );
    assert_running(&described, A, T0);
    assert_other(&described, B, "stopping");
    assert_other(&described, C, "stopped");
}

#[test]
fn each_hosts_state_is_read_from_its_own_element_across_several_reservations() {
    let sim = sim_describing(vec![answer(vec![
        vec![element(D, "pending", None)],
        vec![
            element(B, "running", Some("2026-11-19T09:00:00Z")),
            element(A, "stopped", None),
        ],
        vec![],
        vec![element(C, "running", Some("2026-11-19T10:00:00Z"))],
    ])]);
    let described = describe(&sim, &[A, B, C, D]);
    assert_eq!(sim.count(DESCRIBE), 1);
    assert_eq!(
        ids_of(&described),
        vec![A.to_string(), B.to_string(), C.to_string(), D.to_string()]
    );
    assert_other(&described, A, "stopped");
    assert_running(&described, B, T0 + 47 * 60 + 26);
    assert_running(&described, C, T0 + 107 * 60 + 26);
    assert_other(&described, D, "pending");
}

#[test]
fn the_first_elements_state_is_not_given_to_every_host() {
    // A reader of Reservations[0].Instances[0] alone would give every host A's state.
    let sim = sim_describing(vec![answer(vec![
        vec![element(A, "running", Some("2026-11-19T08:12:34Z"))],
        vec![element(B, "terminated", None)],
    ])]);
    let described = describe(&sim, &[B, A]);
    assert_other(&described, B, "terminated");
    assert_running(&described, A, T0);
}

#[test]
fn an_id_ec2_does_not_know_is_unknown_and_the_others_are_still_read() {
    let sim = sim_describing(vec![answer(vec![vec![
        element(A, "running", Some("2026-11-19T08:12:34Z")),
        element(C, "stopped", None),
    ]])]);
    let described = describe(&sim, &[A, B, C]);
    assert_eq!(
        ids_of(&described),
        vec![A.to_string(), B.to_string(), C.to_string()]
    );
    assert_running(&described, A, T0);
    assert_unknown(&described, B);
    assert_other(&described, C, "stopped");
}

#[test]
fn an_answer_with_no_reservations_makes_every_id_unknown() {
    let sim = sim_describing(vec![Ok(json!({"Reservations": []}))]);
    let described = describe(&sim, &[A, B]);
    assert_eq!(ids_of(&described), vec![A.to_string(), B.to_string()]);
    assert_unknown(&described, A);
    assert_unknown(&described, B);
}

#[test]
fn an_element_with_no_string_instance_id_is_ignored() {
    let sim = sim_describing(vec![Ok(json!({"Reservations": [{"Instances": [
        {"State": {"Name": "running"}, "LaunchTime": "2026-11-19T08:12:34Z"},
        {"InstanceId": 7, "State": {"Name": "running"}, "LaunchTime": "2026-11-19T08:12:34Z"},
        {"InstanceId": null, "State": {"Name": "stopped"}},
        {"InstanceId": B, "State": {"Name": "stopped"}}
    ]}]}))]);
    let described = describe(&sim, &[A, B]);
    assert_unknown(&described, A);
    assert_other(&described, B, "stopped");
}

#[test]
fn an_instance_the_answer_holds_that_was_not_asked_for_is_not_a_result() {
    let sim = sim_describing(vec![answer(vec![vec![
        element(A, "running", Some("2026-11-19T08:12:34Z")),
        element(D, "running", Some("2026-11-19T08:12:34Z")),
    ]])]);
    let described = describe(&sim, &[A]);
    assert_eq!(ids_of(&described), vec![A.to_string()]);
}

// What a running host's LaunchTime is read as.

#[test]
fn a_running_hosts_launch_time_is_utc_unix_seconds_in_every_offset_form() {
    let cases: Vec<(&str, i64)> = vec![
        ("2026-11-19T08:12:34Z", T0),
        ("2026-11-19T08:12:34+00:00", T0),
        ("2026-11-19T08:12:34.000Z", T0),
        // A leap day, and the epoch itself.
        ("2024-02-29T00:00:00Z", 1_709_164_800),
        ("1970-01-01T00:00:00Z", 0),
        // A non-zero offset is converted: the day rolls over.
        ("2026-11-19T20:30:00-07:00", 1_795_145_400),
        // The year rolls over.
        ("2026-12-31T19:00:00-05:00", 1_798_761_600),
        // A half-hour offset east of UTC.
        ("2026-11-19T08:12:34+05:30", T0 - 19_800),
        ("2026-11-19T08:12:34.250+05:30", T0 - 19_800),
    ];
    for (text, expected) in cases {
        let sim = sim_describing(vec![answer(vec![vec![element(A, "running", Some(text))]])]);
        let described = describe(&sim, &[A]);
        let state = the(&described, A);
        assert!(
            matches!(state, DescribedState::Running { launch_time } if *launch_time == expected),
            "LaunchTime {text} should be {expected}, got {}",
            show(state)
        );
    }
}

#[test]
fn a_launch_times_fraction_of_a_second_is_truncated_not_rounded() {
    for text in [
        "2026-11-19T08:12:34.000Z",
        "2026-11-19T08:12:34.500Z",
        "2026-11-19T08:12:34.999Z",
        "2026-11-19T08:12:34.999999+00:00",
    ] {
        let sim = sim_describing(vec![answer(vec![vec![element(A, "running", Some(text))]])]);
        let described = describe(&sim, &[A]);
        assert_running(&described, A, T0);
    }
}

#[test]
fn each_running_host_is_given_its_own_launch_time() {
    let sim = sim_describing(vec![answer(vec![vec![
        element(A, "running", Some("2026-11-19T08:12:34Z")),
        element(B, "running", Some("2026-11-19T08:12:35Z")),
        element(C, "running", Some("2026-11-19T08:12:34-07:00")),
    ]])]);
    let described = describe(&sim, &[C, B, A]);
    assert_running(&described, A, T0);
    assert_running(&described, B, T0 + 1);
    assert_running(&described, C, T0 + 7 * 3600);
}

// What is not running.

#[test]
fn every_state_but_running_is_other_and_keeps_its_name() {
    for name in [
        "pending",
        "stopping",
        "stopped",
        "shutting-down",
        "terminated",
    ] {
        let sim = sim_describing(vec![answer(vec![vec![element(A, name, None)]])]);
        let described = describe(&sim, &[A]);
        assert_other(&described, A, name);
    }
}

#[test]
fn a_non_running_host_with_a_launch_time_is_still_other() {
    // A stopped instance keeps its LaunchTime in the answer, and the guard never reads it.
    let sim = sim_describing(vec![answer(vec![vec![
        element(A, "stopped", Some("2026-11-19T08:12:34Z")),
        element(B, "pending", Some("2026-11-19T08:12:34Z")),
    ]])]);
    let described = describe(&sim, &[A, B]);
    assert_other(&described, A, "stopped");
    assert_other(&described, B, "pending");
}

#[test]
fn a_non_running_host_with_no_launch_time_is_not_a_failure() {
    // Only a running host's LaunchTime is read (operator.md, which command reads which member).
    let sim = sim_describing(vec![answer(vec![vec![
        element(A, "stopped", None),
        element(B, "terminated", None),
        element(C, "shutting-down", None),
    ]])]);
    let described = describe(&sim, &[A, B, C]);
    for id in [A, B, C] {
        assert!(
            matches!(the(&described, id), DescribedState::Other { .. }),
            "A non-running host with no LaunchTime is not Other: {}",
            show(the(&described, id))
        );
    }
}

#[test]
fn a_non_running_hosts_launch_time_of_the_wrong_form_is_not_a_failure() {
    let sim = sim_describing(vec![Ok(json!({"Reservations": [{"Instances": [
        {"InstanceId": A, "State": {"Name": "stopped"}, "LaunchTime": "garbage"},
        {"InstanceId": B, "State": {"Name": "stopped"}, "LaunchTime": 12345},
        {"InstanceId": C, "State": {"Name": "stopped"}, "LaunchTime": "2026-11-19T08:12:34"}
    ]}]}))]);
    let described = describe(&sim, &[A, B, C]);
    for id in [A, B, C] {
        assert_other(&described, id, "stopped");
    }
}

// What fails one host alone.

#[test]
fn a_running_element_with_no_launch_time_is_that_hosts_failure_alone() {
    let sim = sim_describing(vec![answer(vec![vec![
        element(A, "running", None),
        element(B, "running", Some("2026-11-19T08:12:34Z")),
        element(C, "stopped", None),
    ]])]);
    let described = describe(&sim, &[A, B, C, D]);
    assert_eq!(
        ids_of(&described),
        vec![A.to_string(), B.to_string(), C.to_string(), D.to_string()]
    );
    assert_unexpected(&described, A, "LaunchTime");
    assert_running(&described, B, T0);
    assert_other(&described, C, "stopped");
    assert_unknown(&described, D);
}

#[test]
fn a_running_launch_time_not_of_its_form_is_that_hosts_failure_alone() {
    let bad: Vec<(&str, Value)> = vec![
        ("no UTC offset", json!("2026-11-19T08:12:34")),
        ("not a time", json!("yesterday")),
        ("empty", json!("")),
        ("a number", json!(1_795_075_954)),
        ("null", Value::Null),
        ("an array", json!(["2026-11-19T08:12:34Z"])),
    ];
    for (what, value) in bad {
        let mut broken = element(A, "running", None);
        broken["LaunchTime"] = value;
        let sim = sim_describing(vec![answer(vec![vec![
            broken,
            element(B, "running", Some("2026-11-19T08:12:34Z")),
        ]])]);
        let described = describe(&sim, &[A, B]);
        assert!(
            matches!(the(&described, A), DescribedState::Unexpected { .. }),
            "A LaunchTime that is {what} should be Unexpected, got {}",
            show(the(&described, A))
        );
        assert_running(&described, B, T0);
    }
}

#[test]
fn a_running_element_with_no_state_name_is_that_hosts_failure_alone() {
    let sim = sim_describing(vec![Ok(json!({"Reservations": [{"Instances": [
        {"InstanceId": A, "LaunchTime": "2026-11-19T08:12:34Z"},
        {"InstanceId": B, "State": {}, "LaunchTime": "2026-11-19T08:12:34Z"},
        {"InstanceId": C, "State": {"Name": 16}},
        {"InstanceId": D, "State": {"Name": "running"}, "LaunchTime": "2026-11-19T08:12:34Z"}
    ]}]}))]);
    let described = describe(&sim, &[A, B, C, D]);
    for id in [A, B, C] {
        assert_unexpected(&described, id, "State.Name");
    }
    assert_running(&described, D, T0);
}

// A failed call.

#[test]
fn a_failed_describe_call_fails_with_the_cli_s_error_text() {
    let sim = sim_describing(vec![failed("An error occurred (UnauthorizedOperation)")]);
    let failure = describe_hosts(&sim.aws, &[A, B]).expect_err("A failure");
    assert_eq!(
        failure.message,
        "agentspace: An error occurred (UnauthorizedOperation)"
    );
    assert_eq!(sim.count(DESCRIBE), 1, "The describe call is not retried");
}

#[test]
fn a_describe_call_that_cannot_run_fails_with_its_message() {
    let message = "agentspace: Cannot run the AWS CLI: not found.";
    let sim = sim_describing(vec![Err(AwsError::CannotRun(message.to_string()))]);
    let failure = describe_hosts(&sim.aws, &[A]).expect_err("A failure");
    assert_eq!(failure.message, message);
}

#[test]
fn an_expired_sign_in_in_the_describe_call_is_exactly_the_sign_in_message() {
    let sim = sim_describing(vec![Err(AwsError::SignIn)]);
    let failure = describe_hosts(&sim.aws, &[A]).expect_err("A failure");
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(failure.caller, CallerFailure::Same);
    assert_eq!(sim.count(DESCRIBE), 1);
}

// ---------------------------------------------------------------------------------------------
// ec2 stop-instances.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_stop_is_one_stop_instances_call_for_the_one_instance() {
    let sim = sim_stopping(Ok(json!({"StoppingInstances": []})));
    stop_host(&sim.aws, A).expect("The stop is accepted");
    let call = only_call(&sim, STOP);
    assert_eq!(call.service, "ec2");
    assert_eq!(call.operation, "stop-instances");
    assert_eq!(args(&call), vec!["--instance-ids", "i-0123456789abcdef0"]);
}

#[test]
fn a_stop_is_json_and_checked_by_its_exit_code_alone() {
    let sim = sim_stopping(Ok(json!({})));
    stop_host(&sim.aws, A).expect("The stop is accepted");
    assert_eq!(only_call(&sim, STOP).output, Output::Checked);
}

#[test]
fn a_stop_reads_nothing_of_the_answer() {
    for answer in [
        Ok(Value::Null),
        Ok(json!({})),
        Ok(json!([])),
        Ok(json!("stopping")),
        Ok(json!({"StoppingInstances": [{"InstanceId": B, "CurrentState": {"Name": "running"}}]})),
    ] {
        let sim = sim_stopping(answer);
        stop_host(&sim.aws, A).expect("An answer is not read, only the exit code");
        assert_eq!(sim.calls().len(), 1);
    }
}

#[test]
fn a_stop_does_not_wait_or_describe() {
    let sim = Sim::new(Mode::Person);
    sim.script(STOP, vec![Ok(json!({}))]);
    sim.script("ec2 wait instance-stopped", vec![Ok(Value::Null)]);
    sim.script(DESCRIBE, vec![state("stopped")]);
    stop_host(&sim.aws, A).expect("The stop is accepted");
    assert_eq!(sim.keys(), vec![STOP.to_string()]);
    assert!(sim.elapsed().is_zero(), "The stop slept");
}

#[test]
fn a_failed_stop_fails_with_the_cli_s_error_text() {
    let sim = sim_stopping(failed("An error occurred (IncorrectInstanceState)"));
    let failure = stop_host(&sim.aws, A).expect_err("A failure");
    assert!(
        failure.message.starts_with("agentspace: "),
        "Message does not start with agentspace: {}",
        failure.message
    );
    assert!(
        failure
            .message
            .contains("An error occurred (IncorrectInstanceState)"),
        "Message holds no error text: {}",
        failure.message
    );
    assert_eq!(sim.count(STOP), 1, "A failed stop is not retried");
}

#[test]
fn a_failed_stop_has_the_form_the_guard_prints_or_the_plain_aws_failure() {
    // The guard prints `Could not stop <id>: <error>` (operator.md, guard, step 6) with <error> the
    // AWS CLI's error text as AWS failures gives it; AWS failures gives `agentspace: <text>`.
    // Either the library gives the plain failure and the guard adds its words, or it gives the
    // whole.
    let sim = sim_stopping(failed("boom"));
    let message = stop_host(&sim.aws, A).expect_err("A failure").message;
    let plain = "agentspace: boom".to_string();
    let whole = format!("agentspace: Could not stop {A}: boom");
    assert!(
        message == plain || message == whole,
        "Neither form of the failure: {message}"
    );
}

#[test]
fn an_expired_sign_in_in_a_stop_is_exactly_the_sign_in_message() {
    let sim = sim_stopping(Err(AwsError::SignIn));
    let failure = stop_host(&sim.aws, A).expect_err("A failure");
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(failure.caller, CallerFailure::Same);
    assert_eq!(sim.count(STOP), 1);
}

#[test]
fn a_stop_that_cannot_run_fails_with_its_message() {
    let message = "agentspace: Cannot run the AWS CLI: not found.";
    let sim = sim_stopping(Err(AwsError::CannotRun(message.to_string())));
    let failure = stop_host(&sim.aws, A).expect_err("A failure");
    assert!(failure.message.contains(message) || failure.message == message);
}

// ---------------------------------------------------------------------------------------------
// ssm send-command, the lease horizon.
// ---------------------------------------------------------------------------------------------

/// The `--parameters` argument of a recorded send, and the command in it.
fn parameters_of(call: &Rec) -> String {
    call.args
        .get(7)
        .expect("A --parameters value as the eighth argument")
        .clone()
}

fn command_of(call: &Rec) -> String {
    let parameters = parameters_of(call);
    parameters
        .strip_prefix("commands=")
        .unwrap_or_else(|| panic!("--parameters does not start with commands="))
        .to_string()
}

/// The maximal runs of decimal digits in `text`.
fn digit_runs(text: &str) -> Vec<String> {
    let mut runs = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            current.push(c);
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

/// The words of `text`: its pieces between whitespace, quotes and shell operators.
fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| {
        !(c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '$' | '{' | '}'))
    })
    .filter(|w| !w.is_empty())
    .map(str::to_string)
    .collect()
}

fn send(horizon: i64) -> (Sim, Rec) {
    let sim = sim_sending(Ok(json!({"Command": {"CommandId": "cmd-0"}})));
    send_horizon(&sim.aws, A, horizon).expect("The send is accepted");
    let call = only_call(&sim, SEND);
    (sim, call)
}

#[test]
fn a_send_is_one_send_command_with_the_pinned_arguments_but_the_command_s_text() {
    let (_sim, call) = send(T0);
    assert_eq!(call.service, "ssm");
    assert_eq!(call.operation, "send-command");
    let all = args(&call);
    assert_eq!(all.len(), 8, "Arguments: {all:?}");
    assert_eq!(
        &all[..7],
        &[
            "--instance-ids",
            "i-0123456789abcdef0",
            "--document-name",
            "AWS-RunShellScript",
            "--comment",
            "agentspace guard: lease horizon",
            "--parameters",
        ]
    );
    assert!(
        all[7].starts_with("commands="),
        "--parameters is not commands=<command>: {}",
        all[7]
    );
    assert!(all[7].len() > "commands=".len(), "An empty command");
}

#[test]
fn the_horizon_comment_is_the_pinned_one_and_is_what_is_sent() {
    assert_eq!(HORIZON_COMMENT, "agentspace guard: lease horizon");
    let (_sim, call) = send(T0);
    assert_eq!(call.args[4], "--comment");
    assert_eq!(call.args[5], HORIZON_COMMENT);
}

#[test]
fn a_send_is_json_and_checked_by_its_exit_code_alone() {
    let (_sim, call) = send(T0);
    assert_eq!(call.output, Output::Checked);
}

#[test]
fn a_send_goes_to_the_instance_it_is_given() {
    let sim = sim_sending(Ok(json!({})));
    send_horizon(&sim.aws, D, T0).expect("The send is accepted");
    let call = only_call(&sim, SEND);
    assert_eq!(call.args[0], "--instance-ids");
    assert_eq!(call.args[1], D, "One call names one instance");
}

#[test]
fn a_send_never_polls_the_invocation_or_asks_after_the_agent() {
    let (sim, _call) = send(T0);
    assert_eq!(sim.count(INVOCATION), 0, "A get-command-invocation");
    assert_eq!(sim.count("ssm describe-instance-information"), 0);
    assert_eq!(sim.keys(), vec![SEND.to_string()]);
    assert!(sim.elapsed().is_zero(), "The send slept");
}

#[test]
fn a_send_reads_nothing_of_the_answer() {
    // No Command.CommandId, and answers of every shape: only the exit code counts.
    for answer in [
        Ok(Value::Null),
        Ok(json!({})),
        Ok(json!([])),
        Ok(json!({"Command": {}})),
        Ok(json!({"Command": {"CommandId": 7}})),
        Ok(json!({"Command": {"CommandId": "cmd-0"}})),
    ] {
        let sim = sim_sending(answer);
        send_horizon(&sim.aws, A, T0).expect("An answer is not read, only the exit code");
        assert_eq!(sim.keys(), vec![SEND.to_string()]);
    }
}

#[test]
fn the_command_holds_the_horizon_s_decimal_digits_as_a_word_of_its_own() {
    for horizon in [T0, T0 + 1, 4_102_444_800, 1_000_000_000] {
        let (_sim, call) = send(horizon);
        let command = command_of(&call);
        let digits = horizon.to_string();
        assert!(
            words_of(&command).contains(&digits),
            "The horizon {digits} is not a word of its own in the command: {command}"
        );
    }
}

#[test]
fn the_command_holds_the_horizon_file_s_path() {
    let (_sim, call) = send(T0);
    let command = command_of(&call);
    assert!(
        command.contains("/run/agentspace/lease-horizon"),
        "The command does not name /run/agentspace/lease-horizon: {command}"
    );
}

#[test]
fn the_command_holds_no_other_horizon() {
    // Only the horizon sent is a number of five digits or more. Modes and the like are shorter.
    for horizon in [T0, T0 + 1, 4_102_444_800] {
        let (_sim, call) = send(horizon);
        let command = command_of(&call);
        let digits = horizon.to_string();
        for run in digit_runs(&command) {
            assert!(
                run.len() < 5 || run == digits,
                "The command holds a number {run} that is not the horizon {digits}: {command}"
            );
        }
    }
}

#[test]
fn two_horizons_give_two_commands_that_each_hold_only_their_own() {
    let (_sim, first) = send(T0);
    let (_sim, second) = send(T0 + 60);
    let (first, second) = (command_of(&first), command_of(&second));
    assert_ne!(first, second);
    assert!(first.contains(&T0.to_string()) && !first.contains(&(T0 + 60).to_string()));
    assert!(second.contains(&(T0 + 60).to_string()) && !second.contains(&T0.to_string()));
    // Apart from the horizon the two are the same text.
    assert_eq!(
        first.replace(&T0.to_string(), "H"),
        second.replace(&(T0 + 60).to_string(), "H")
    );
}

#[test]
fn the_command_holds_no_comma_bracket_or_equals_sign() {
    // The AWS CLI's shorthand syntax for --parameters splits on them (operator.md, guard, step 7).
    for horizon in [T0, 4_102_444_800] {
        let (_sim, call) = send(horizon);
        let command = command_of(&call);
        for bad in [',', '[', ']', '='] {
            assert!(
                !command.contains(bad),
                "The command holds {bad:?}: {command}"
            );
        }
    }
}

#[test]
fn the_command_holds_nothing_that_cannot_survive_a_command_line() {
    // No control character in the command, and no backslash at its end, which `commands=<command>`
    // would carry to the CLI as one argument (the patch question's own rules, FA-15).
    let (_sim, call) = send(T0);
    let command = command_of(&call);
    assert!(
        !command.chars().any(|c| c.is_control()),
        "A control character in the command: {command:?}"
    );
    assert!(!command.ends_with('\\'), "A backslash ends the command");
}

#[test]
fn the_command_renames_a_temporary_file_over_the_horizon_file() {
    // operator.md, guard, step 7: the temporary file is renamed over /run/agentspace/lease-horizon,
    // so a reader finds the old file or the new one whole. The text is the operator's own (D34),
    // so only the rename, which a shell does with mv, and its target are checked.
    let (_sim, call) = send(T0);
    let command = command_of(&call);
    assert!(
        words_of(&command).iter().any(|w| w == "mv"),
        "No rename of the temporary file (mv): {command}"
    );
    // The rename's target is the horizon file, and the file is the last thing named in it.
    let target = command
        .rfind("/run/agentspace/lease-horizon")
        .expect("The target");
    let rename = command.rfind("mv").expect("The rename");
    assert!(
        rename < target,
        "The horizon file is named before the rename, not as its target: {command}"
    );
}

#[test]
fn a_failed_send_fails_with_the_cli_s_error_text() {
    let sim = sim_sending(failed("An error occurred (InvalidInstanceId)"));
    let failure = send_horizon(&sim.aws, A, T0).expect_err("A failure");
    assert!(failure.message.starts_with("agentspace: "));
    assert!(
        failure
            .message
            .contains("An error occurred (InvalidInstanceId)"),
        "Message holds no error text: {}",
        failure.message
    );
    assert_eq!(sim.count(SEND), 1, "A failed send is not retried");
    assert_eq!(sim.count(INVOCATION), 0);
}

#[test]
fn a_failed_send_has_the_form_the_guard_prints_or_the_plain_aws_failure() {
    // `Could not send the lease horizon to <id>: <error>` (operator.md, guard, step 7).
    let sim = sim_sending(failed("boom"));
    let message = send_horizon(&sim.aws, A, T0)
        .expect_err("A failure")
        .message;
    let plain = "agentspace: boom".to_string();
    let whole = format!("agentspace: Could not send the lease horizon to {A}: boom");
    assert!(
        message == plain || message == whole,
        "Neither form of the failure: {message}"
    );
}

#[test]
fn an_expired_sign_in_in_a_send_is_exactly_the_sign_in_message() {
    let sim = sim_sending(Err(AwsError::SignIn));
    let failure = send_horizon(&sim.aws, A, T0).expect_err("A failure");
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(failure.caller, CallerFailure::Same);
    assert_eq!(sim.count(SEND), 1);
}

#[test]
fn a_send_that_cannot_run_fails_with_its_message() {
    let message = "agentspace: Cannot run the AWS CLI: not found.";
    let sim = sim_sending(Err(AwsError::CannotRun(message.to_string())));
    let failure = send_horizon(&sim.aws, A, T0).expect_err("A failure");
    assert!(failure.message.contains(message) || failure.message == message);
}

// ---------------------------------------------------------------------------------------------
// The three together: no call the guard does not make.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_run_of_all_three_makes_no_call_but_its_own_and_never_polls_an_invocation() {
    let sim = Sim::new(Mode::Person);
    sim.script(
        DESCRIBE,
        vec![answer(vec![vec![
            element(A, "running", Some("2026-11-19T08:12:34Z")),
            element(B, "running", Some("2026-11-19T08:12:34Z")),
        ]])],
    );
    sim.script(STOP, vec![Ok(json!({}))]);
    sim.script(SEND, vec![Ok(json!({"Command": {"CommandId": "cmd-0"}}))]);
    sim.script(INVOCATION, vec![Ok(json!({"Status": "Success"}))]);
    let described = describe(&sim, &[A, B]);
    assert_running(&described, A, T0);
    stop_host(&sim.aws, A).expect("The stop is accepted");
    send_horizon(&sim.aws, B, T0).expect("The send is accepted");
    assert_eq!(
        sim.keys(),
        vec![DESCRIBE.to_string(), STOP.to_string(), SEND.to_string()]
    );
    assert_eq!(sim.count(INVOCATION), 0);
}

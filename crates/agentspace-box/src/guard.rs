//! The guard's three AWS calls (operator.md, guard; Calls by command, the `guard` row, and What is
//! read from each answer).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The guard's decisions belong to the program. This module makes the calls and reads the one
//! answer the guard reads: [`describe_hosts`] asks EC2 about every host's instance at once,
//! [`stop_host`] asks it to stop one, and [`send_horizon`] sends one running host its lease
//! horizon. **None of them prints or logs**: the guard does, with the failure text each returns.

use agentspace_operator_api::{Aws, Call, CallerFailure, Failure, Output};
use serde_json::Value;

use crate::launch_time;

/// The comment the lease horizon is sent with (operator.md, guard, step 7; D63).
pub const HORIZON_COMMENT: &str = "agentspace guard: lease horizon";

/// What `ec2 describe-instances` says of one host's ID, in the order the IDs were given
/// (operator.md, guard, steps 5 and 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Described {
    /// The instance ID, as it was given.
    pub id: String,
    /// What the answer holds for it.
    pub state: DescribedState,
}

/// What the answer holds for one instance ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescribedState {
    /// No element of `Reservations[].Instances[]` has the ID: EC2 does not know it. The guard fails
    /// that host's part with `Instance <id> was not found.` (step 5).
    Unknown,
    /// `State.Name` is `running`. `launch_time` is its `LaunchTime` in Unix seconds, in UTC, a
    /// fraction of a second truncated.
    Running { launch_time: i64 },
    /// Any other state (`pending`, `stopping`, `stopped`, `shutting-down`, `terminated`, or a
    /// name not yet known): the guard does nothing for the host (step 6). Its `LaunchTime` is not
    /// read, so a bad one MUST NOT fail the host.
    Other { state: String },
    /// A member of the host's element is missing or not of its form, and the host's part fails with
    /// `Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].<member>.`,
    /// where `member` is `State.Name`, or `LaunchTime` for a running instance (step 6; Calls by
    /// command). The guard composes that message, which stderr prints after
    /// `agentspace: Host '<project>': ` and the log writes with `step=instance`. It fails that host
    /// alone.
    Unexpected { member: String },
}

/// One `ec2 describe-instances --filters Name=instance-id,Values=<id>,<id>,...` (JSON), the IDs in
/// the order given and joined by commas (operator.md, Calls by command, the `guard` row), read as
/// that table's guard rows say: every element of `Reservations[].Instances[]` is looked at by its
/// `InstanceId`, an element with no string `InstanceId` is ignored, and the first element with a
/// given ID is that host's.
///
/// Returns one [`Described`] per ID, in the order given. **Makes no call when `ids` is empty**
/// (step 5). A failed call returns its [`Failure`], #0034's sign-in message included, as every
/// call of this crate does. An answer with no `Reservations` array is the failure
/// `agentspace: Unexpected answer from ec2 describe-instances: no Reservations.`, since a filter
/// that matches nothing still gets an empty array; a reservation with no `Instances` array holds
/// no element.
pub fn describe_hosts(aws: &dyn Aws, ids: &[&str]) -> Result<Vec<Described>, Failure> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let filter = format!("Name=instance-id,Values={}", ids.join(","));
    let answer = aws.call(&Call {
        service: "ec2",
        operation: "describe-instances",
        args: &["--filters", &filter],
        output: Output::Json,
    })?;
    let reservations = answer
        .get("Reservations")
        .and_then(Value::as_array)
        .ok_or_else(|| Failure {
            message: "agentspace: Unexpected answer from ec2 describe-instances: no Reservations."
                .to_string(),
            caller: CallerFailure::CouldNotReach,
        })?;
    let elements: Vec<&Value> = reservations
        .iter()
        .filter_map(|reservation| reservation.get("Instances").and_then(Value::as_array))
        .flatten()
        .collect();

    Ok(ids
        .iter()
        .map(|&id| Described {
            id: id.to_string(),
            state: elements
                .iter()
                .find(|element| element.get("InstanceId").and_then(Value::as_str) == Some(id))
                .map_or(DescribedState::Unknown, |element| described_state(element)),
        })
        .collect())
}

/// The state of one host's element: `State.Name`, and for `running` alone, `LaunchTime`.
fn described_state(element: &Value) -> DescribedState {
    let Some(name) = element.pointer("/State/Name").and_then(Value::as_str) else {
        return unexpected("State.Name");
    };
    if name != "running" {
        return DescribedState::Other {
            state: name.to_string(),
        };
    }
    let launch_time = element
        .get("LaunchTime")
        .and_then(Value::as_str)
        .and_then(launch_time::parse)
        .and_then(|nanos| i64::try_from(nanos.div_euclid(1_000_000_000)).ok());
    match launch_time {
        Some(launch_time) => DescribedState::Running { launch_time },
        None => unexpected("LaunchTime"),
    }
}

fn unexpected(member: &str) -> DescribedState {
    DescribedState::Unexpected {
        member: member.to_string(),
    }
}

/// One `ec2 stop-instances --instance-ids <id>` (JSON), which asks EC2 to stop the instance and
/// does not wait (operator.md, guard, step 6; Calls by command). Its answer is read no further
/// than its exit code, and nothing is printed: the guard logs and prints. A failure is returned as
/// the crate's calls return one, and the guard words it `Could not stop <id>: <error>`.
///
/// This is a function of its own, making the same call as `down`'s stop, which stays in the
/// operator crate (the architect, 2026-11-22, for v0.08-T02).
pub fn stop_host(aws: &dyn Aws, id: &str) -> Result<(), Failure> {
    aws.call(&Call {
        service: "ec2",
        operation: "stop-instances",
        args: &["--instance-ids", id],
        output: Output::Checked,
    })?;
    Ok(())
}

/// The remote command that writes the horizon, for `horizon` Unix seconds (operator.md, guard,
/// step 7). As root, it makes `/run/agentspace` if it is missing (mode 0755), writes the horizon
/// and one line feed to a temporary file beside the target, makes it mode 0644 and renames it over
/// `/run/agentspace/lease-horizon`, so a reader finds the old file or the new one whole. Its exit
/// status is that of the chain, so it is 0 only when every step succeeded.
///
/// It holds the horizon's decimal digits as a word of its own, and no comma, bracket, equals sign,
/// `%`, double quote or control character, and does not end in a backslash, so that the AWS CLI's
/// shorthand for `--parameters` does not split it (operator.md, guard, step 7; FA-15). The
/// temporary file's name is fixed, which two runs never contend for: `guard.lock` admits one guard
/// at a time, and a run that failed midway leaves a file the next run replaces.
fn horizon_command(horizon: i64) -> String {
    format!(
        "umask 022; mkdir -p -m 755 /run/agentspace \
&& echo {horizon} > /run/agentspace/lease-horizon.new \
&& chmod 644 /run/agentspace/lease-horizon.new \
&& mv -f /run/agentspace/lease-horizon.new /run/agentspace/lease-horizon"
    )
}

/// One `ssm send-command --instance-ids <id> --document-name AWS-RunShellScript --comment
/// "agentspace guard: lease horizon" --parameters commands=<command>`, which runs `<command>` as
/// root on the host and writes `/run/agentspace/lease-horizon` (operator.md, guard, step 7; D63).
/// It reads nothing of the answer but its exit code, never polls the invocation and does not wait.
/// A failure is returned as the crate's calls return one, and the guard words it `Could not send
/// the lease horizon to <id>: <error>`.
///
/// `horizon` is Unix seconds and MUST NOT be negative: a negative one makes no call and returns a
/// failure, since the file holds a decimal integer with no sign.
pub fn send_horizon(aws: &dyn Aws, id: &str, horizon: i64) -> Result<(), Failure> {
    if horizon < 0 {
        return Err(Failure {
            message: "agentspace: The lease horizon cannot be negative.".to_string(),
            caller: CallerFailure::Same,
        });
    }
    let parameters = format!("commands={}", horizon_command(horizon));
    aws.call(&Call {
        service: "ssm",
        operation: "send-command",
        args: &[
            "--instance-ids",
            id,
            "--document-name",
            "AWS-RunShellScript",
            "--comment",
            HORIZON_COMMENT,
            "--parameters",
            &parameters,
        ],
        output: Output::Checked,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_operator_api::AwsError;
    use serde_json::json;
    use std::cell::RefCell;

    const A: &str = "i-0123456789abcdef0";
    const B: &str = "i-0123456789abcdef1";
    const C: &str = "i-0123456789abcdef2";

    /// An AWS that records each call and answers one canned result.
    struct Recorder {
        calls: RefCell<Vec<(String, Output)>>,
        answer: Result<Value, AwsError>,
    }

    impl Recorder {
        fn new(answer: Result<Value, AwsError>) -> Self {
            Recorder {
                calls: RefCell::new(Vec::new()),
                answer,
            }
        }
        fn lines(&self) -> Vec<String> {
            self.calls.borrow().iter().map(|(l, _)| l.clone()).collect()
        }
    }

    impl Aws for Recorder {
        fn call(&self, call: &Call) -> Result<Value, AwsError> {
            let mut line = vec![call.service, call.operation];
            line.extend(call.args);
            self.calls.borrow_mut().push((line.join(" "), call.output));
            self.answer.clone()
        }
    }

    fn instance(id: &str, state: &str, launch: Option<&str>) -> Value {
        let mut element = json!({"InstanceId": id, "State": {"Name": state}});
        if let Some(launch) = launch {
            element["LaunchTime"] = json!(launch);
        }
        element
    }

    fn answer(instances: Vec<Value>) -> Value {
        json!({"Reservations": [{"Instances": instances}]})
    }

    fn describe(answer: Value, ids: &[&str]) -> (Vec<Described>, Recorder) {
        let aws = Recorder::new(Ok(answer));
        let described = describe_hosts(&aws, ids).unwrap();
        (described, aws)
    }

    #[test]
    fn the_call_names_every_id_in_one_filter() {
        let (_, aws) = describe(answer(vec![]), &[A, B, C]);
        assert_eq!(
            aws.lines(),
            [format!(
                "ec2 describe-instances --filters Name=instance-id,Values={A},{B},{C}"
            )]
        );
        assert_eq!(aws.calls.borrow()[0].1, Output::Json);
    }

    #[test]
    fn one_id_is_one_filter_with_no_extra_comma() {
        let (_, aws) = describe(answer(vec![]), &[A]);
        assert_eq!(
            aws.lines(),
            [format!(
                "ec2 describe-instances --filters Name=instance-id,Values={A}"
            )]
        );
    }

    #[test]
    fn no_ids_make_no_call() {
        let aws = Recorder::new(Err(AwsError::Failed("must not be called".to_string())));
        assert_eq!(describe_hosts(&aws, &[]).unwrap(), Vec::new());
        assert!(aws.lines().is_empty());
    }

    #[test]
    fn an_unknown_id_is_unknown_and_the_others_are_read() {
        let (described, _) = describe(answer(vec![instance(B, "stopped", None)]), &[A, B]);
        assert_eq!(
            described,
            [
                Described {
                    id: A.to_string(),
                    state: DescribedState::Unknown
                },
                Described {
                    id: B.to_string(),
                    state: DescribedState::Other {
                        state: "stopped".to_string()
                    }
                },
            ]
        );
    }

    #[test]
    fn the_results_follow_the_given_order_not_the_answers() {
        let (described, _) = describe(
            json!({"Reservations": [
                {"Instances": [instance(C, "pending", None)]},
                {"Instances": [instance(A, "running", Some("2026-11-19T08:12:34+00:00"))]},
            ]}),
            &[A, B, C],
        );
        let states: Vec<_> = described
            .iter()
            .map(|d| (d.id.as_str(), &d.state))
            .collect();
        assert_eq!(
            states,
            [
                (
                    A,
                    &DescribedState::Running {
                        launch_time: 1_795_075_954
                    }
                ),
                (B, &DescribedState::Unknown),
                (
                    C,
                    &DescribedState::Other {
                        state: "pending".to_string()
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_running_host_has_its_launch_time_in_utc_seconds() {
        for (text, seconds) in [
            ("2026-11-19T08:12:34+00:00", 1_795_075_954),
            ("2026-11-19T08:12:34.999Z", 1_795_075_954),
            ("2026-11-19T00:12:34-08:00", 1_795_075_954),
        ] {
            let (described, _) = describe(answer(vec![instance(A, "running", Some(text))]), &[A]);
            assert_eq!(
                described[0].state,
                DescribedState::Running {
                    launch_time: seconds
                },
                "{text}"
            );
        }
    }

    #[test]
    fn a_missing_or_bad_launch_time_fails_that_running_host_alone() {
        for launch in [
            None,
            Some("2026-11-19T08:12:34"),
            Some("not a time"),
            Some(""),
        ] {
            let (described, _) = describe(
                answer(vec![
                    instance(A, "running", launch),
                    instance(B, "running", Some("2026-11-19T08:12:34Z")),
                ]),
                &[A, B],
            );
            assert_eq!(
                described[0].state,
                DescribedState::Unexpected {
                    member: "LaunchTime".to_string()
                },
                "{launch:?}"
            );
            assert_eq!(
                described[1].state,
                DescribedState::Running {
                    launch_time: 1_795_075_954
                }
            );
        }
        // A launch time that is not a string is as missing.
        let mut element = instance(A, "running", None);
        element["LaunchTime"] = json!(5);
        let (described, _) = describe(answer(vec![element]), &[A]);
        assert_eq!(
            described[0].state,
            DescribedState::Unexpected {
                member: "LaunchTime".to_string()
            }
        );
    }

    #[test]
    fn a_launch_time_is_read_for_a_running_host_only() {
        let (described, _) = describe(
            answer(vec![
                instance(A, "stopped", Some("garbage")),
                instance(B, "pending", None),
            ]),
            &[A, B],
        );
        assert!(
            described
                .iter()
                .all(|d| matches!(d.state, DescribedState::Other { .. }))
        );
    }

    #[test]
    fn a_missing_state_name_fails_that_host_alone() {
        for element in [
            json!({"InstanceId": A}),
            json!({"InstanceId": A, "State": {}}),
            json!({"InstanceId": A, "State": {"Name": 16}}),
            json!({"InstanceId": A, "State": "running"}),
        ] {
            let (described, _) =
                describe(answer(vec![element, instance(B, "stopped", None)]), &[A, B]);
            assert_eq!(
                described[0].state,
                DescribedState::Unexpected {
                    member: "State.Name".to_string()
                }
            );
            assert!(matches!(described[1].state, DescribedState::Other { .. }));
        }
    }

    #[test]
    fn an_element_with_no_string_instance_id_is_ignored() {
        let (described, _) = describe(
            answer(vec![
                json!({"State": {"Name": "running"}}),
                json!({"InstanceId": 7, "State": {"Name": "running"}}),
                json!(null),
                instance(A, "stopped", None),
            ]),
            &[A],
        );
        assert_eq!(
            described[0].state,
            DescribedState::Other {
                state: "stopped".to_string()
            }
        );
    }

    #[test]
    fn a_reservation_with_no_instances_holds_none() {
        let (described, _) = describe(
            json!({"Reservations": [
                {},
                {"Instances": "x"},
                {"Instances": [instance(A, "stopped", None)]},
            ]}),
            &[A, B],
        );
        assert!(matches!(described[0].state, DescribedState::Other { .. }));
        assert_eq!(described[1].state, DescribedState::Unknown);
    }

    #[test]
    fn an_empty_answer_leaves_every_host_unknown() {
        let (described, _) = describe(json!({"Reservations": []}), &[A, B]);
        assert!(described.iter().all(|d| d.state == DescribedState::Unknown));
    }

    #[test]
    fn an_answer_with_no_reservations_is_an_unexpected_answer() {
        for malformed in [
            json!({}),
            json!({"Reservations": null}),
            json!({"Reservations": "x"}),
            json!(null),
            json!("text"),
            json!([]),
        ] {
            let aws = Recorder::new(Ok(malformed.clone()));
            let failure = describe_hosts(&aws, &[A]).unwrap_err();
            assert_eq!(
                failure.message,
                "agentspace: Unexpected answer from ec2 describe-instances: no Reservations.",
                "{malformed}"
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        }
    }

    #[test]
    fn a_failed_describe_returns_the_crates_failure() {
        let aws = Recorder::new(Err(AwsError::Failed("Throttled".to_string())));
        let failure = describe_hosts(&aws, &[A]).unwrap_err();
        assert_eq!(failure.message, "agentspace: Throttled");
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);

        let aws = Recorder::new(Err(AwsError::SignIn));
        let failure = describe_hosts(&aws, &[A]).unwrap_err();
        assert_eq!(failure.message, agentspace_operator_api::SIGN_IN_MESSAGE);
        assert_eq!(failure.caller, CallerFailure::Same);
    }

    #[test]
    fn a_stop_is_the_one_call_and_reads_nothing() {
        let aws = Recorder::new(Ok(Value::Null));
        stop_host(&aws, A).unwrap();
        assert_eq!(
            aws.lines(),
            [format!("ec2 stop-instances --instance-ids {A}")]
        );
        assert_eq!(aws.calls.borrow()[0].1, Output::Checked);
    }

    #[test]
    fn a_failed_stop_is_returned() {
        let aws = Recorder::new(Err(AwsError::Failed("Denied".to_string())));
        assert_eq!(
            stop_host(&aws, A).unwrap_err().message,
            "agentspace: Denied"
        );
        let aws = Recorder::new(Err(AwsError::SignIn));
        assert_eq!(
            stop_host(&aws, A).unwrap_err().message,
            agentspace_operator_api::SIGN_IN_MESSAGE
        );
    }

    const COMMAND: &str = "umask 022; mkdir -p -m 755 /run/agentspace \
&& echo 1795075954 > /run/agentspace/lease-horizon.new \
&& chmod 644 /run/agentspace/lease-horizon.new \
&& mv -f /run/agentspace/lease-horizon.new /run/agentspace/lease-horizon";

    #[test]
    fn a_send_is_the_one_call_with_the_commands_text() {
        let aws = Recorder::new(Ok(Value::Null));
        send_horizon(&aws, A, 1_795_075_954).unwrap();
        assert_eq!(
            aws.lines(),
            [format!(
                "ssm send-command --instance-ids {A} --document-name AWS-RunShellScript --comment agentspace guard: lease horizon --parameters commands={COMMAND}"
            )]
        );
        assert_eq!(aws.calls.borrow()[0].1, Output::Checked);
        assert_eq!(HORIZON_COMMENT, "agentspace guard: lease horizon");
    }

    #[test]
    fn the_command_holds_the_horizon_as_a_word_and_nothing_the_cli_splits_on() {
        for horizon in [0, 7, 1_795_075_954, i64::MAX] {
            let command = horizon_command(horizon);
            assert!(
                command.split_whitespace().any(|w| w == horizon.to_string()),
                "{command}"
            );
            for c in command.chars() {
                assert!(
                    !matches!(c, ',' | '[' | ']' | '=' | '%' | '"') && !c.is_control(),
                    "{c:?}"
                );
            }
            assert!(!command.ends_with('\\'));
            assert!(command.is_ascii());
        }
    }

    #[test]
    fn the_command_writes_the_file_whole_and_renames_it() {
        let command = horizon_command(5);
        let write = command
            .find("echo 5 > /run/agentspace/lease-horizon.new")
            .unwrap();
        let chmod = command
            .find("chmod 644 /run/agentspace/lease-horizon.new")
            .unwrap();
        let rename = command
            .find("mv -f /run/agentspace/lease-horizon.new /run/agentspace/lease-horizon")
            .unwrap();
        assert!(command.starts_with("umask 022; mkdir -p -m 755 /run/agentspace "));
        assert!(write < chmod && chmod < rename);
        // Each step runs only when the one before succeeded, so the exit status is the chain's.
        assert_eq!(command.matches("&&").count(), 3);
    }

    #[test]
    fn a_failed_send_is_returned() {
        let aws = Recorder::new(Err(AwsError::Failed("InvalidInstanceId".to_string())));
        let failure = send_horizon(&aws, A, 5).unwrap_err();
        assert_eq!(failure.message, "agentspace: InvalidInstanceId");
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        let aws = Recorder::new(Err(AwsError::SignIn));
        assert_eq!(
            send_horizon(&aws, A, 5).unwrap_err().message,
            agentspace_operator_api::SIGN_IN_MESSAGE
        );
    }

    #[test]
    fn a_negative_horizon_makes_no_call() {
        let aws = Recorder::new(Ok(Value::Null));
        assert!(send_horizon(&aws, A, -1).is_err());
        assert!(aws.lines().is_empty());
    }
}

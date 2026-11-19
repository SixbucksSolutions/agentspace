//! The start-and-wait step (operator.md, Starting the box and waiting for it; What up waits for).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

use std::time::Duration;

use agentspace_operator_api::{Aws, Call, CallerFailure, CallerLine, Failure, Output, Say};
use serde_json::Value;

use crate::answer::{
    self, COMMAND_ID, INSTANCE_INFORMATION_LIST, LAUNCH_TIME, PING_STATUS, STATE_NAME,
};
use crate::clock::Clock;
use crate::launch_time;

/// The comment the patch question is sent with (operator.md, What up waits for).
pub const PATCH_COMMENT: &str = "agentspace up: patch check";

/// The patch question, run as root by `AWS-RunShellScript` on today's box: `patched` when this
/// boot's marker exists, otherwise `pending` when the patch unit is enabled, and `unknown` when it
/// is not (operator.md, What up waits for).
///
/// It MUST hold no `,`, `[`, `]` or `=`, on which the AWS CLI's shorthand syntax for
/// `--parameters` splits, and none of what FA-15 forbids: no `%`, no double quote, no control
/// character, and no backslash at its end (operator.md, What the laptop sends). A test checks it.
pub const PATCH_QUESTION: &str = "if test -e /run/agentspace/patched; then echo patched; \
elif systemctl is-enabled --quiet agentspace-patch.service; then echo pending; \
else echo unknown; fi";

/// The wait's limit, from its start (step 3).
const WAIT_LIMIT: Duration = Duration::from_secs(15 * 60);
/// The pause between two checks (step 3).
const CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// How long a box MUST have been running to go unasked (step 2).
const RUNNING_LONG_ENOUGH: Duration = Duration::from_secs(20 * 60);
/// The pause between two polls of the invocation (What up waits for).
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// The most polls of one invocation; that many without an end count as `pending`.
const POLL_LIMIT: usize = 20;

const WAITING_FOR_SESSION_MANAGER: &str = "Waiting for Session Manager...";
const PATCHING: &str = "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";

const DESCRIBE_INSTANCES: &str = "ec2 describe-instances";
const DESCRIBE_INSTANCE_INFORMATION: &str = "ssm describe-instance-information";
const SEND_COMMAND: &str = "ssm send-command";

/// The start-and-wait step that `up`, `sync` and every nested `up` share, for `instance_id`
/// (operator.md, Starting the box and waiting for it):
///
/// 1. Reads the instance's state. `running` and `pending` start nothing. `stopped` prints
///    `Starting <id>...` and starts it. `stopping` prints `Waiting for <id> to finish
///    stopping...`, runs `ec2 wait instance-stopped` without checking how it ended, then prints
///    `Starting <id>...` and starts it. Any other state fails with `agentspace: Instance <id> is
///    <state>.`
/// 2. Waits unless the state was `running` and the launch time, read by a second
///    `ec2 describe-instances` that runs only for a `running` box (FB-09), is 20 minutes ago or
///    more.
/// 3. Waits for at most 15 minutes, checking every 5 seconds, printing each message only when it
///    differs from the last one printed.
/// 4. Prints `Ready: <id>`.
///
/// In caller mode [`Say`] prints the caller's lines in place of these, and each [`Failure`]
/// carries the caller's line in place of its message.
pub fn start_and_wait_for(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    instance_id: &str,
) -> Result<(), Failure> {
    let id = instance_id;
    let state = describe_instance(aws, id, STATE_NAME)?;
    match state.as_str() {
        "running" | "pending" => {}
        "stopped" => {
            say.progress(&format!("Starting {id}..."), Some(CallerLine::Starting));
            start_instance(aws, id)?;
        }
        "stopping" => {
            say.progress(
                &format!("Waiting for {id} to finish stopping..."),
                Some(CallerLine::Starting),
            );
            // The wait's exit code is not checked (B-34): the start request after it is the check.
            let _ = aws.call(&Call {
                service: "ec2",
                operation: "wait",
                args: &["instance-stopped", "--instance-ids", id],
                output: Output::Ignore,
            });
            say.progress(&format!("Starting {id}..."), Some(CallerLine::Starting));
            start_instance(aws, id)?;
        }
        other => {
            return Err(Failure {
                message: format!("agentspace: Instance {id} is {other}."),
                caller: CallerFailure::NotAvailable,
            });
        }
    }

    if state != "running" || !running_long_enough(aws, clock, id)? {
        wait(aws, say, clock, id)?;
    }

    say.progress(&format!("Ready: {id}"), Some(CallerLine::Ready));
    Ok(())
}

/// One `ec2 describe-instances --instance-ids <id>` (JSON), and the one member this step reads
/// from it (FB-08).
fn describe_instance(aws: &dyn Aws, id: &str, member: answer::Member) -> Result<String, Failure> {
    let answer = aws.call(&Call {
        service: "ec2",
        operation: "describe-instances",
        args: &["--instance-ids", id],
        output: Output::Json,
    })?;
    answer::string(&answer, DESCRIBE_INSTANCES, member).map(str::to_string)
}

/// `ec2 start-instances --instance-ids <id>` (JSON), read no further than its exit code.
fn start_instance(aws: &dyn Aws, id: &str) -> Result<(), Failure> {
    aws.call(&Call {
        service: "ec2",
        operation: "start-instances",
        args: &["--instance-ids", id],
        output: Output::Json,
    })?;
    Ok(())
}

/// Step 2's second `ec2 describe-instances`: whether the launch time is 20 minutes ago or more,
/// compared in UTC. A launch time in the future is not.
fn running_long_enough(aws: &dyn Aws, clock: &dyn Clock, id: &str) -> Result<bool, Failure> {
    let text = describe_instance(aws, id, LAUNCH_TIME)?;
    let launched = launch_time::parse(&text)
        .ok_or_else(|| answer::unexpected(DESCRIBE_INSTANCES, LAUNCH_TIME))?;
    let now = launch_time::nanos_since_epoch(clock.utc_now());
    Ok(now - launched >= RUNNING_LONG_ENOUGH.as_nanos() as i128)
}

/// Step 3: checks until the box is ready, or fails at the 15-minute deadline.
///
/// The timing is the architect's ruling of 2026-11-19 for E0.06-T01-S03 (operator.md, Calls by
/// command): the first check is at once, and the 5 seconds are a pause after each check; the
/// deadline is tested after each check that was not ready, so a check that began before it and
/// found the box ready still ends the wait; and the last pause is cut short to end on the
/// deadline, so the last check begins at it.
fn wait(aws: &dyn Aws, say: &dyn Say, clock: &dyn Clock, id: &str) -> Result<(), Failure> {
    let started = clock.now();
    let mut last: Option<&str> = None;
    loop {
        let message = if !session_manager_online(aws, id)? {
            WAITING_FOR_SESSION_MANAGER
        } else if patch_pending(aws, clock, id)? {
            PATCHING
        } else {
            return Ok(());
        };
        if last != Some(message) {
            say.progress(message, Some(CallerLine::Waiting));
            last = Some(message);
        }
        let elapsed = clock.now().saturating_sub(started);
        if elapsed >= WAIT_LIMIT {
            return Err(Failure {
                message: format!(
                    "agentspace: {id} was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
                ),
                caller: CallerFailure::NotReady,
            });
        }
        // The last pause never runs past the deadline, so the last check is made at it.
        clock.sleep(CHECK_INTERVAL.min(WAIT_LIMIT - elapsed));
    }
}

/// `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON): online only
/// when `InstanceInformationList[0].PingStatus` is `Online`, and an empty list is not online.
fn session_manager_online(aws: &dyn Aws, id: &str) -> Result<bool, Failure> {
    let filter = format!("Key=InstanceIds,Values={id}");
    let answer = aws.call(&Call {
        service: "ssm",
        operation: "describe-instance-information",
        args: &["--filters", &filter],
        output: Output::Json,
    })?;
    // An answer with no list has no `InstanceInformationList[0].PingStatus`, the member's path as
    // Calls by command writes it; only a list that is there and empty is "not online".
    let list = answer
        .pointer(INSTANCE_INFORMATION_LIST)
        .and_then(Value::as_array)
        .ok_or_else(|| answer::unexpected(DESCRIBE_INSTANCE_INFORMATION, PING_STATUS))?;
    if list.is_empty() {
        return Ok(false);
    }
    Ok(answer::string(&answer, DESCRIBE_INSTANCE_INFORMATION, PING_STATUS)? == "Online")
}

/// Asks the box whether its patch run has finished (operator.md, What up waits for), and whether
/// the answer is `pending`. Every other answer counts as ready: `patched`, `unknown`, and an empty
/// or unexpected answer from a failed check (Q6).
///
/// The invocation is polled at once and then each second, at most 20 times. A poll that fails, or
/// whose status is `Pending`, `InProgress` or `Delayed`, polls again; any other status ends the
/// poll, and the answer is `StandardOutputContent`, trimmed. 20 polls without an end count as
/// `pending`.
///
/// The poll never gives the "Unexpected answer" failure (operator.md, Calls by command, the
/// architect's rulings of 2026-11-19 for E0.06-T01-S03): an answer with no string `Status` is not
/// readable yet, and is polled again; one that has ended with no `StandardOutputContent` is the
/// empty answer, which counts as ready (Q6).
fn patch_pending(aws: &dyn Aws, clock: &dyn Clock, id: &str) -> Result<bool, Failure> {
    let parameters = format!("commands={PATCH_QUESTION}");
    let sent = aws.call(&Call {
        service: "ssm",
        operation: "send-command",
        args: &[
            "--instance-ids",
            id,
            "--document-name",
            "AWS-RunShellScript",
            "--comment",
            PATCH_COMMENT,
            "--parameters",
            &parameters,
        ],
        output: Output::Json,
    })?;
    let command_id = answer::string(&sent, SEND_COMMAND, COMMAND_ID)?;

    for poll in 0..POLL_LIMIT {
        // The first poll is at once, and each pause of a second is between two polls: 20 polls
        // take 19 pauses, and none follows the last.
        if poll > 0 {
            clock.sleep(POLL_INTERVAL);
        }
        // A failed poll is polled again: the invocation is often not readable for a moment after
        // send-command (operator.md, Calls by command).
        let Ok(invocation) = aws.call(&Call {
            service: "ssm",
            operation: "get-command-invocation",
            args: &["--command-id", command_id, "--instance-id", id],
            output: Output::Json,
        }) else {
            continue;
        };
        let status = invocation.get("Status").and_then(Value::as_str);
        if matches!(status, None | Some("Pending" | "InProgress" | "Delayed")) {
            continue;
        }
        let stdout = invocation
            .get("StandardOutputContent")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Ok(stdout.trim() == "pending");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_operator_api::{AwsError, Mode};
    use serde_json::json;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::time::SystemTime;

    const ID: &str = "i-0123456789abcdef0";

    #[test]
    fn the_patch_question_holds_nothing_forbidden() {
        for c in PATCH_QUESTION.chars() {
            assert!(!matches!(c, ',' | '[' | ']' | '=' | '%' | '"'), "{c:?}");
            assert!(!c.is_control(), "{c:?}");
        }
        assert!(!PATCH_QUESTION.ends_with('\\'));
        assert!(PATCH_QUESTION.is_ascii());
    }

    /// A fake AWS that answers each call, in order, from a script keyed by its operation.
    struct Script {
        answers: RefCell<VecDeque<(&'static str, Result<Value, AwsError>)>>,
        calls: RefCell<Vec<String>>,
    }

    impl Script {
        fn new(answers: Vec<(&'static str, Result<Value, AwsError>)>) -> Self {
            Script {
                answers: RefCell::new(answers.into()),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Aws for Script {
        fn call(&self, call: &Call) -> Result<Value, AwsError> {
            let mut line = vec![call.service, call.operation];
            line.extend(call.args);
            self.calls.borrow_mut().push(line.join(" "));
            let (operation, answer) = self
                .answers
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| panic!("Unscripted call {}", line.join(" ")));
            assert_eq!(operation, call.operation);
            answer
        }
    }

    struct Fake {
        now: Cell<Duration>,
        utc: SystemTime,
        lines: RefCell<Vec<String>>,
    }

    impl Clock for Fake {
        fn now(&self) -> Duration {
            self.now.get()
        }
        fn utc_now(&self) -> SystemTime {
            self.utc + self.now.get()
        }
        fn sleep(&self, d: Duration) {
            self.now.set(self.now.get() + d);
        }
    }

    impl Say for Fake {
        fn mode(&self) -> Mode {
            Mode::Person
        }
        fn progress(&self, person: &str, _: Option<CallerLine>) {
            self.lines.borrow_mut().push(person.to_string());
        }
        fn warning(&self, person: &str) {
            self.lines.borrow_mut().push(person.to_string());
        }
        fn detail(&self, _: &str) {}
    }

    fn fake() -> Fake {
        Fake {
            now: Cell::new(Duration::ZERO),
            utc: SystemTime::UNIX_EPOCH + Duration::from_secs(1_795_075_954),
            lines: RefCell::new(Vec::new()),
        }
    }

    fn state(name: &str) -> (&'static str, Result<Value, AwsError>) {
        (
            "describe-instances",
            Ok(json!({"Reservations":[{"Instances":[{"State":{"Name":name}}]}]})),
        )
    }

    fn launched(time: &str) -> (&'static str, Result<Value, AwsError>) {
        (
            "describe-instances",
            Ok(json!({"Reservations":[{"Instances":[{"LaunchTime":time}]}]})),
        )
    }

    fn ping(status: Option<&str>) -> (&'static str, Result<Value, AwsError>) {
        let list = match status {
            Some(s) => json!([{"PingStatus": s}]),
            None => json!([]),
        };
        (
            "describe-instance-information",
            Ok(json!({ "InstanceInformationList": list })),
        )
    }

    fn sent() -> (&'static str, Result<Value, AwsError>) {
        (
            "send-command",
            Ok(json!({"Command":{"CommandId":"c-0123"}})),
        )
    }

    fn invocation(status: &str, out: &str) -> (&'static str, Result<Value, AwsError>) {
        (
            "get-command-invocation",
            Ok(json!({"Status":status,"StandardOutputContent":out})),
        )
    }

    #[test]
    fn a_box_running_long_enough_is_not_asked() {
        let aws = Script::new(vec![state("running"), launched("2026-11-19T07:52:34Z")]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(*f.lines.borrow(), vec![format!("Ready: {ID}")]);
        assert_eq!(aws.calls.borrow().len(), 2);
    }

    #[test]
    fn a_stopped_box_is_started_and_waited_for() {
        let aws = Script::new(vec![
            state("stopped"),
            ("start-instances", Ok(json!({}))),
            ping(None),
            ping(Some("ConnectionLost")),
            ping(Some("Online")),
            sent(),
            ("get-command-invocation", Err(AwsError::Failed("x".into()))),
            invocation("InProgress", ""),
            invocation("Success", "pending\n"),
            ping(Some("Online")),
            sent(),
            invocation("Success", " patched \n"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![
                format!("Starting {ID}..."),
                WAITING_FOR_SESSION_MANAGER.to_string(),
                PATCHING.to_string(),
                format!("Ready: {ID}"),
            ]
        );
        assert!(aws.answers.borrow().is_empty());
        // 3 pauses of 5 seconds between checks, and 2 of 1 between the first question's 3 polls.
        assert_eq!(f.now.get(), Duration::from_secs(17));
    }

    #[test]
    fn the_deadline_fails_the_wait() {
        let mut answers = vec![state("pending")];
        for _ in 0..181 {
            answers.push(ping(None));
        }
        let aws = Script::new(answers);
        let f = fake();
        let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::NotReady);
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {ID} was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
            )
        );
        assert_eq!(f.now.get(), WAIT_LIMIT);
        assert_eq!(*f.lines.borrow(), vec![WAITING_FOR_SESSION_MANAGER]);
        assert!(aws.answers.borrow().is_empty());
    }

    #[test]
    fn a_stopping_box_waits_unchecked_then_starts() {
        let aws = Script::new(vec![
            state("stopping"),
            (
                "wait",
                Err(AwsError::Failed("Waiter InstanceStopped failed".into())),
            ),
            ("start-instances", Ok(Value::Null)),
            ping(Some("Online")),
            sent(),
            invocation("Success", "unknown"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![
                format!("Waiting for {ID} to finish stopping..."),
                format!("Starting {ID}..."),
                format!("Ready: {ID}"),
            ]
        );
        assert_eq!(
            aws.calls.borrow()[1],
            format!("ec2 wait instance-stopped --instance-ids {ID}")
        );
    }

    #[test]
    fn a_failed_start_fails() {
        let aws = Script::new(vec![
            state("stopped"),
            ("start-instances", Err(AwsError::SignIn)),
        ]);
        let f = fake();
        let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::Same);
        assert_eq!(*f.lines.borrow(), vec![format!("Starting {ID}...")]);
    }

    #[test]
    fn any_other_state_fails() {
        for name in ["terminated", "shutting-down", ""] {
            let aws = Script::new(vec![state(name)]);
            let f = fake();
            let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
            assert_eq!(
                failure.message,
                format!("agentspace: Instance {ID} is {name}.")
            );
            assert_eq!(failure.caller, CallerFailure::NotAvailable);
            assert!(f.lines.borrow().is_empty());
        }
    }

    #[test]
    fn only_a_running_box_has_its_launch_time_read() {
        // A pending box waits with no second describe-instances (FB-09).
        let aws = Script::new(vec![
            state("pending"),
            ping(Some("Online")),
            sent(),
            invocation("Success", "patched"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        let describes = aws
            .calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("ec2 describe-instances"))
            .count();
        assert_eq!(describes, 1);
    }

    #[test]
    fn the_twenty_minute_rule() {
        // Exactly 20 minutes ago is long enough.
        let aws = Script::new(vec![state("running"), launched("2026-11-19T07:52:34Z")]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        // A second less waits, and so does a launch time in the future.
        for time in ["2026-11-19T07:52:35Z", "2026-11-19T09:00:00+00:00"] {
            let aws = Script::new(vec![
                state("running"),
                launched(time),
                ping(Some("Online")),
                sent(),
                invocation("Success", "patched"),
            ]);
            let f = fake();
            start_and_wait_for(&aws, &f, &f, ID).unwrap();
            assert!(aws.answers.borrow().is_empty());
        }
    }

    #[test]
    fn a_bad_launch_time_is_an_unexpected_answer() {
        for answer in [
            json!({"Reservations":[{"Instances":[{}]}]}),
            json!({"Reservations":[{"Instances":[{"LaunchTime":"yesterday"}]}]}),
            json!({"Reservations":[{"Instances":[{"LaunchTime":12}]}]}),
        ] {
            let aws = Script::new(vec![state("running"), ("describe-instances", Ok(answer))]);
            let f = fake();
            let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
            assert_eq!(
                failure.message,
                "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime."
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        }
    }

    #[test]
    fn twenty_polls_without_an_end_count_as_pending() {
        let mut answers = vec![state("pending"), ping(Some("Online")), sent()];
        for i in 0..POLL_LIMIT {
            answers.push(match i % 3 {
                0 => (
                    "get-command-invocation",
                    Err(AwsError::Failed("InvocationDoesNotExist".into())),
                ),
                1 => invocation("Pending", ""),
                _ => invocation("Delayed", ""),
            });
        }
        answers.extend([ping(Some("Online")), sent(), invocation("Failed", "")]);
        let aws = Script::new(answers);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![PATCHING.to_string(), format!("Ready: {ID}")]
        );
        // 19 pauses between 20 polls, then 5 seconds before the next check, whose poll is at once.
        assert_eq!(f.now.get(), Duration::from_secs(19 + 5));
    }

    #[test]
    fn an_invocation_with_no_status_polls_again_and_no_stdout_is_ready() {
        let aws = Script::new(vec![
            state("pending"),
            ping(Some("Online")),
            sent(),
            ("get-command-invocation", Ok(json!({}))),
            ("get-command-invocation", Ok(json!({"Status":"Success"}))),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(*f.lines.borrow(), vec![format!("Ready: {ID}")]);
        assert!(aws.answers.borrow().is_empty());
    }

    #[test]
    fn a_missing_member_is_an_unexpected_answer() {
        type Answers = Vec<(&'static str, Result<Value, AwsError>)>;
        let cases: Vec<(Answers, &str)> = vec![
            // The path is the one Calls by command's table writes for this call, as FB-08's rule
            // gives it ("its path as this table writes it"), with or without the list.
            (
                vec![("describe-instance-information", Ok(json!({})))],
                "ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
            ),
            (
                vec![(
                    "describe-instance-information",
                    Ok(json!({"InstanceInformationList":{}})),
                )],
                "ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
            ),
            (
                vec![(
                    "describe-instance-information",
                    Ok(json!({"InstanceInformationList":[{}]})),
                )],
                "ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
            ),
            (
                vec![
                    ping(Some("Online")),
                    ("send-command", Ok(json!({"Command":{}}))),
                ],
                "ssm send-command: no Command.CommandId.",
            ),
        ];
        for (mut answers, tail) in cases {
            answers.insert(0, state("pending"));
            let aws = Script::new(answers);
            let f = fake();
            let failure = start_and_wait_for(&aws, &f, &f, ID).unwrap_err();
            assert_eq!(
                failure.message,
                format!("agentspace: Unexpected answer from {tail}")
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        }
    }

    #[test]
    fn the_calls_carry_their_arguments_in_order() {
        let aws = Script::new(vec![
            state("pending"),
            ping(Some("Online")),
            sent(),
            invocation("Success", "patched"),
        ]);
        let f = fake();
        start_and_wait_for(&aws, &f, &f, ID).unwrap();
        assert_eq!(
            *aws.calls.borrow(),
            vec![
                format!("ec2 describe-instances --instance-ids {ID}"),
                format!("ssm describe-instance-information --filters Key=InstanceIds,Values={ID}"),
                format!(
                    "ssm send-command --instance-ids {ID} --document-name AWS-RunShellScript --comment agentspace up: patch check --parameters commands={PATCH_QUESTION}"
                ),
                format!("ssm get-command-invocation --command-id c-0123 --instance-id {ID}"),
            ]
        );
    }
}

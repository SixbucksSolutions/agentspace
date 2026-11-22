//! `sync`'s patch wait (docs/operator.md, sync, step 5, and The patch wait; D61's change, #0163;
//! P13, P21).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **It runs right after the install (row 4) exited 0, and before row 5.** It starts no ssh or
//!   scp: its calls are the AWS CLI's, through Session Manager's Run Command.
//! - **It asks `up`'s question, [`agentspace_box::PATCH_QUESTION`]**, with its own comment,
//!   [`PATCH_COMMENT`], so Run Command's history tells `sync`'s questions from `up`'s.
//! - **Only `patched` and `failed` end it**, each from an invocation that ended in `Success`.
//!   Every other outcome is a check that was not ready: `pending`, `unknown`, an empty or
//!   unexpected answer, an invocation that ended in another status, 20 polls without an end, and a
//!   failed `ssm send-command` other than #0034's sign-in failure. It has none of the four
//!   exceptions of `up`'s wait.
//! - **A `failed` answer MUST NOT fail `sync`**: it ends the wait at once with one warning, and
//!   `sync` goes on (P21).
//! - **It fails at its 30-minute deadline**, with the host's alias in its `journalctl` hint, and
//!   on #0034's sign-in failure or a failed `ssm describe-instance-information` at once.
//! - **Its timing is `up`'s wait's**: the first check at once, a pause of 5 seconds after each
//!   check that was not ready, the deadline tested after each such check, and the last pause cut
//!   short to end on it; the invocation polled at once and then each second, at most 20 times.

use std::time::Duration;

use agentspace_box::{Clock, PATCH_QUESTION};
use agentspace_operator_api::{Aws, AwsError, Call, Failure, Output, Say};
use serde_json::Value;

use crate::step_failed;

/// The comment `sync`'s question is sent with (operator.md, The patch wait, step 2).
pub(crate) const PATCH_COMMENT: &str = "agentspace sync: patch check";

/// The wait's limit, from its start (P13).
pub(crate) const WAIT_LIMIT: Duration = Duration::from_secs(30 * 60);
/// The pause after each check that was not ready.
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// The pause between two polls of one invocation.
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// The most polls of one invocation; that many without an end is a check that was not ready.
const POLL_LIMIT: usize = 20;

const WAITING_FOR_SESSION_MANAGER: &str = "Waiting for Session Manager...";
const PATCHING: &str = "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";

/// How the wait ended, when it did not fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatchOutcome {
    /// The host answered `patched`, and `Patched: <id>` was printed.
    Patched,
    /// The host answered `failed`, and the warning was printed; `sync` goes on (P21).
    Failed,
}

/// What one check found.
enum Check {
    NotOnline,
    NotReady,
    Patched,
    Failed,
}

/// The wait, for the host `id`, whose alias `alias` the deadline's and the warning's `journalctl`
/// hint names (`agentspace` for the box, `agentspace-host.<project>` for every other host).
pub(crate) fn wait(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    id: &str,
    alias: &str,
) -> Result<PatchOutcome, Failure> {
    say.progress(
        &format!("Checking that {id} has finished this boot's patch run..."),
        None,
    );
    let started = clock.now();
    let mut last: Option<&str> = None;
    loop {
        let message = match check(aws, clock, id)? {
            Check::Patched => {
                say.progress(&format!("Patched: {id}"), None);
                return Ok(PatchOutcome::Patched);
            }
            Check::Failed => {
                say.warning(&format!(
                    "agentspace: The patch run on {id} failed; sync goes on without it. Check: ssh {alias} journalctl -u agentspace-patch"
                ));
                return Ok(PatchOutcome::Failed);
            }
            Check::NotOnline => WAITING_FOR_SESSION_MANAGER,
            Check::NotReady => PATCHING,
        };
        if last != Some(message) {
            say.progress(message, None);
            last = Some(message);
        }
        let elapsed = clock.now().saturating_sub(started);
        if elapsed >= WAIT_LIMIT {
            return Err(step_failed(&format!(
                "agentspace: {id} did not finish its patch run within 30 minutes; check: ssh {alias} journalctl -u agentspace-patch"
            )));
        }
        // The last pause never runs past the deadline, so the last check is made at it.
        clock.sleep(CHECK_INTERVAL.min(WAIT_LIMIT - elapsed));
    }
}

/// One check: Session Manager's agent, then, when it is `Online`, the question.
fn check(aws: &dyn Aws, clock: &dyn Clock, id: &str) -> Result<Check, Failure> {
    if !session_manager_online(aws, id)? {
        return Ok(Check::NotOnline);
    }
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
    });
    let sent = match sent {
        Ok(sent) => sent,
        // #0034's sign-in failure fails `sync` at once, and ends `-All` too.
        Err(AwsError::SignIn) => return Err(AwsError::SignIn.into()),
        // A host rebooting for its patch run refuses commands while Session Manager can still
        // read it `Online`.
        Err(_) => return Ok(Check::NotOnline),
    };
    let command_id = sent
        .pointer("/Command/CommandId")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            step_failed(
                "agentspace: Unexpected answer from ssm send-command: no Command.CommandId.",
            )
        })?;

    for poll in 0..POLL_LIMIT {
        // The first poll is at once, and each pause of a second is between two polls.
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
        match status {
            None | Some("Pending" | "InProgress" | "Delayed") => continue,
            // Only an invocation that ended in `Success` answers; an empty answer is not ready in
            // this wait alone (operator.md, Calls by command).
            Some("Success") => {
                let stdout = invocation
                    .get("StandardOutputContent")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                return Ok(match stdout.trim() {
                    "patched" => Check::Patched,
                    "failed" => Check::Failed,
                    _ => Check::NotReady,
                });
            }
            Some(_) => return Ok(Check::NotReady),
        }
    }
    Ok(Check::NotReady)
}

/// `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON): online only
/// when `InstanceInformationList[0].PingStatus` is `Online`, and an empty list is not online. A
/// failed call fails `sync`, under AWS failures, as it fails `up`.
fn session_manager_online(aws: &dyn Aws, id: &str) -> Result<bool, Failure> {
    let filter = format!("Key=InstanceIds,Values={id}");
    let answer = aws.call(&Call {
        service: "ssm",
        operation: "describe-instance-information",
        args: &["--filters", &filter],
        output: Output::Json,
    })?;
    let unexpected = || {
        step_failed(
            "agentspace: Unexpected answer from ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
        )
    };
    let list = answer
        .pointer("/InstanceInformationList")
        .and_then(Value::as_array)
        .ok_or_else(unexpected)?;
    let Some(first) = list.first() else {
        return Ok(false);
    };
    let status = first
        .get("PingStatus")
        .and_then(Value::as_str)
        .ok_or_else(unexpected)?;
    Ok(status == "Online")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_operator_api::{CallerLine, Mode, SIGN_IN_MESSAGE};
    use serde_json::json;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::time::SystemTime;

    const ID: &str = "i-0123456789abcdef0";

    type Answer = (&'static str, Result<Value, AwsError>);

    /// A fake AWS that answers each call, in order, from a script keyed by its operation, and then
    /// from `then`, repeated, when the script has run out.
    struct Script {
        answers: RefCell<VecDeque<Answer>>,
        then: Option<Vec<Answer>>,
        calls: RefCell<Vec<String>>,
    }

    impl Script {
        fn new(answers: Vec<Answer>) -> Self {
            Script {
                answers: RefCell::new(answers.into()),
                then: None,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Aws for Script {
        fn call(&self, call: &Call) -> Result<Value, AwsError> {
            let mut line = vec![call.service, call.operation];
            line.extend(call.args);
            self.calls.borrow_mut().push(line.join(" "));
            let mut answers = self.answers.borrow_mut();
            if answers.is_empty()
                && let Some(then) = &self.then
            {
                answers.extend(then.iter().cloned());
            }
            let (operation, answer) = answers
                .pop_front()
                .unwrap_or_else(|| panic!("Unscripted call {}", line.join(" ")));
            assert_eq!(operation, call.operation);
            answer
        }
    }

    struct Fake {
        now: Cell<Duration>,
        lines: RefCell<Vec<String>>,
        sleeps: RefCell<Vec<Duration>>,
    }

    impl Clock for Fake {
        fn now(&self) -> Duration {
            self.now.get()
        }
        fn utc_now(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + self.now.get()
        }
        fn sleep(&self, d: Duration) {
            self.sleeps.borrow_mut().push(d);
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
            self.lines.borrow_mut().push(format!("warning: {person}"));
        }
        fn detail(&self, _: &str) {}
    }

    fn fake() -> Fake {
        Fake {
            now: Cell::new(Duration::ZERO),
            lines: RefCell::new(Vec::new()),
            sleeps: RefCell::new(Vec::new()),
        }
    }

    fn ping(status: Option<&str>) -> Answer {
        let list = match status {
            Some(s) => json!([{ "PingStatus": s }]),
            None => json!([]),
        };
        (
            "describe-instance-information",
            Ok(json!({ "InstanceInformationList": list })),
        )
    }

    fn sent() -> Answer {
        (
            "send-command",
            Ok(json!({"Command":{"CommandId":"c-0123"}})),
        )
    }

    fn invocation(status: &str, out: &str) -> Answer {
        (
            "get-command-invocation",
            Ok(json!({"Status": status, "StandardOutputContent": out})),
        )
    }

    /// One online check that ends in `answer`.
    fn asked(answer: Answer) -> Vec<Answer> {
        vec![ping(Some("Online")), sent(), answer]
    }

    fn checking() -> String {
        format!("Checking that {ID} has finished this boot's patch run...")
    }

    fn patched() -> String {
        format!("Patched: {ID}")
    }

    fn warning(alias: &str) -> String {
        format!(
            "warning: agentspace: The patch run on {ID} failed; sync goes on without it. Check: ssh {alias} journalctl -u agentspace-patch"
        )
    }

    #[test]
    fn patched_ends_the_wait_with_its_line() {
        let aws = Script::new(asked(invocation("Success", " patched \n")));
        let f = fake();
        assert_eq!(
            wait(&aws, &f, &f, ID, "agentspace").unwrap(),
            PatchOutcome::Patched
        );
        assert_eq!(*f.lines.borrow(), vec![checking(), patched()]);
        assert!(aws.answers.borrow().is_empty());
        assert_eq!(f.now.get(), Duration::ZERO);
    }

    #[test]
    fn the_calls_carry_their_arguments_in_order_with_syncs_comment() {
        let aws = Script::new(asked(invocation("Success", "patched")));
        let f = fake();
        wait(&aws, &f, &f, ID, "agentspace").unwrap();
        assert_eq!(
            *aws.calls.borrow(),
            vec![
                format!("ssm describe-instance-information --filters Key=InstanceIds,Values={ID}"),
                format!(
                    "ssm send-command --instance-ids {ID} --document-name AWS-RunShellScript --comment agentspace sync: patch check --parameters commands={PATCH_QUESTION}"
                ),
                format!("ssm get-command-invocation --command-id c-0123 --instance-id {ID}"),
            ]
        );
        // No `ec2 describe-instances` of its own: the wait never reads the launch time.
        assert!(aws.calls.borrow().iter().all(|c| !c.starts_with("ec2")));
    }

    #[test]
    fn failed_ends_the_wait_at_once_with_one_warning_naming_the_alias() {
        for (first, alias) in [(true, "agentspace"), (false, "agentspace-host.demo")] {
            let mut answers = Vec::new();
            if !first {
                answers.extend(asked(invocation("Success", "pending")));
            }
            answers.extend(asked(invocation("Success", "failed\n")));
            let aws = Script::new(answers);
            let f = fake();
            assert_eq!(wait(&aws, &f, &f, ID, alias).unwrap(), PatchOutcome::Failed);
            let mut want = vec![checking()];
            if !first {
                want.push(PATCHING.to_string());
            }
            want.push(warning(alias));
            assert_eq!(*f.lines.borrow(), want);
            // No further call after the `failed` answer.
            assert!(aws.answers.borrow().is_empty());
            assert!(!f.lines.borrow().contains(&patched()));
        }
    }

    #[test]
    fn only_patched_and_failed_from_a_success_end_it() {
        let not_ready = [
            invocation("Success", "pending"),
            invocation("Success", "unknown"),
            invocation("Success", ""),
            invocation("Success", "something else"),
            ("get-command-invocation", Ok(json!({"Status":"Success"}))),
            invocation("Failed", "failed"),
            invocation("Failed", "patched"),
            invocation("TimedOut", ""),
            invocation("Cancelled", "patched"),
        ];
        for answer in not_ready {
            let shown = format!("{answer:?}");
            let mut answers = asked(answer);
            answers.extend(asked(invocation("Success", "patched")));
            let aws = Script::new(answers);
            let f = fake();
            assert_eq!(
                wait(&aws, &f, &f, ID, "agentspace").unwrap(),
                PatchOutcome::Patched,
                "{shown}"
            );
            assert_eq!(
                *f.lines.borrow(),
                vec![checking(), PATCHING.to_string(), patched()],
                "{shown}"
            );
            assert!(aws.answers.borrow().is_empty(), "{shown}");
            assert_eq!(f.now.get(), CHECK_INTERVAL, "{shown}");
        }
    }

    #[test]
    fn twenty_polls_without_an_end_are_not_ready() {
        let mut answers = vec![ping(Some("Online")), sent()];
        for i in 0..POLL_LIMIT {
            answers.push(match i % 4 {
                0 => (
                    "get-command-invocation",
                    Err(AwsError::Failed("InvocationDoesNotExist".into())),
                ),
                1 => invocation("Pending", ""),
                2 => invocation("InProgress", "patched"),
                _ => ("get-command-invocation", Ok(json!({}))),
            });
        }
        answers.extend(asked(invocation("Success", "patched")));
        let aws = Script::new(answers);
        let f = fake();
        wait(&aws, &f, &f, ID, "agentspace").unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![checking(), PATCHING.to_string(), patched()]
        );
        // 19 pauses of a second between 20 polls, then 5 seconds before the next check.
        assert_eq!(f.now.get(), Duration::from_secs(19 + 5));
        assert!(aws.answers.borrow().is_empty());
    }

    #[test]
    fn a_wait_through_a_reboot_prints_each_phase_again_when_it_changes() {
        let mut answers = vec![
            ping(Some("Online")),
            sent(),
            invocation("Success", "pending"),
        ];
        answers.push(ping(Some("ConnectionLost")));
        answers.push(ping(Some("Online")));
        answers.push((
            "send-command",
            Err(AwsError::Failed(
                "InvalidInstanceId: not in a valid state".into(),
            )),
        ));
        answers.push(ping(None));
        answers.extend(asked(invocation("Success", "pending")));
        answers.extend(asked(invocation("Success", "pending")));
        answers.extend(asked(invocation("Success", "patched")));
        let aws = Script::new(answers);
        let f = fake();
        wait(&aws, &f, &f, ID, "agentspace").unwrap();
        assert_eq!(
            *f.lines.borrow(),
            vec![
                checking(),
                PATCHING.to_string(),
                WAITING_FOR_SESSION_MANAGER.to_string(),
                PATCHING.to_string(),
                patched(),
            ]
        );
        assert!(aws.answers.borrow().is_empty());
    }

    #[test]
    fn the_sign_in_failure_at_send_command_fails_at_once() {
        let aws = Script::new(vec![
            ping(Some("Online")),
            ("send-command", Err(AwsError::SignIn)),
        ]);
        let f = fake();
        let failure = wait(&aws, &f, &f, ID, "agentspace").unwrap_err();
        assert_eq!(failure.message, SIGN_IN_MESSAGE);
        assert_eq!(*f.lines.borrow(), vec![checking()]);
        assert!(f.sleeps.borrow().is_empty());
    }

    #[test]
    fn a_failed_describe_instance_information_fails_at_once() {
        for (error, message) in [
            (
                AwsError::Failed("AccessDeniedException: no".into()),
                "agentspace: AccessDeniedException: no".to_string(),
            ),
            (AwsError::SignIn, SIGN_IN_MESSAGE.to_string()),
        ] {
            // After a check that was not ready, too.
            let aws = Script::new(vec![
                ping(None),
                ("describe-instance-information", Err(error)),
            ]);
            let f = fake();
            let failure = wait(&aws, &f, &f, ID, "agentspace").unwrap_err();
            assert_eq!(failure.message, message);
            assert_eq!(
                *f.lines.borrow(),
                vec![checking(), WAITING_FOR_SESSION_MANAGER.to_string()]
            );
        }
    }

    #[test]
    fn an_answer_with_a_missing_member_fails() {
        for (answers, message) in [
            (
                vec![("describe-instance-information", Ok(json!({})))],
                "agentspace: Unexpected answer from ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
            ),
            (
                vec![(
                    "describe-instance-information",
                    Ok(json!({"InstanceInformationList":[{}]})),
                )],
                "agentspace: Unexpected answer from ssm describe-instance-information: no InstanceInformationList[0].PingStatus.",
            ),
            (
                vec![
                    ping(Some("Online")),
                    ("send-command", Ok(json!({"Command":{}}))),
                ],
                "agentspace: Unexpected answer from ssm send-command: no Command.CommandId.",
            ),
        ] {
            let aws = Script::new(answers);
            let f = fake();
            assert_eq!(
                wait(&aws, &f, &f, ID, "agentspace").unwrap_err().message,
                message
            );
        }
    }

    #[test]
    fn the_deadline_is_thirty_minutes_and_names_the_alias() {
        for alias in ["agentspace", "agentspace-host.demo"] {
            let mut aws = Script::new(Vec::new());
            aws.then = Some(asked(invocation("Success", "pending")));
            let f = fake();
            let failure = wait(&aws, &f, &f, ID, alias).unwrap_err();
            assert_eq!(
                failure.message,
                format!(
                    "agentspace: {ID} did not finish its patch run within 30 minutes; check: ssh {alias} journalctl -u agentspace-patch"
                )
            );
            assert_eq!(f.now.get(), WAIT_LIMIT);
            assert_eq!(*f.lines.borrow(), vec![checking(), PATCHING.to_string()]);
            // 360 pauses of 5 seconds, the last ending on the deadline; 361 checks.
            let sleeps = f.sleeps.borrow();
            assert_eq!(sleeps.len(), 360);
            assert!(sleeps.iter().all(|d| *d == CHECK_INTERVAL));
            assert_eq!(aws.calls.borrow().len(), 361 * 3);
        }
    }

    #[test]
    fn the_last_pause_is_cut_short_to_end_on_the_deadline() {
        // Each check takes 12 seconds of polling (13 polls), so the checks begin 17 seconds apart:
        // the one that begins at 1785 seconds ends at 1797, and the pause after it is 3 seconds.
        let mut check = vec![ping(Some("Online")), sent()];
        for _ in 0..12 {
            check.push(invocation("InProgress", ""));
        }
        check.push(invocation("Success", "pending"));
        let mut aws = Script::new(Vec::new());
        aws.then = Some(check);
        let f = fake();
        let failure = wait(&aws, &f, &f, ID, "agentspace").unwrap_err();
        assert!(failure.message.contains("within 30 minutes"));
        let pauses: Vec<Duration> = f
            .sleeps
            .borrow()
            .iter()
            .copied()
            .filter(|d| *d != POLL_INTERVAL)
            .collect();
        assert_eq!(pauses.last(), Some(&Duration::from_secs(3)));
        assert!(
            pauses[..pauses.len() - 1]
                .iter()
                .all(|d| *d == CHECK_INTERVAL)
        );
        // The last check began at the deadline, and the deadline was tested after it.
        assert_eq!(f.now.get(), WAIT_LIMIT + Duration::from_secs(12));
    }

    #[test]
    fn a_check_that_finds_it_patched_at_the_deadline_still_ends_the_wait() {
        let mut answers = Vec::new();
        for _ in 0..360 {
            answers.push(ping(None));
        }
        answers.extend(asked(invocation("Success", "patched")));
        let aws = Script::new(answers);
        let f = fake();
        assert_eq!(
            wait(&aws, &f, &f, ID, "agentspace").unwrap(),
            PatchOutcome::Patched
        );
        assert_eq!(f.now.get(), WAIT_LIMIT);
    }
}

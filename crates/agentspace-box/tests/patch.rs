//! The patch question of "What up waits for": its comment and its command's forbidden characters,
//! the invocation polled once a second up to 20 times, and each answer.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const DESCRIBE: &str = "ec2 describe-instances";
const INFO: &str = "ssm describe-instance-information";
const SEND: &str = "ssm send-command";
const POLL: &str = "ssm get-command-invocation";

const PATCHING: &str = "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";

/// A pending box that is online from the first look, with `polls` for the first question and
/// `patch_words`'s commands (`cmd-n`). `polls` replaces the invocation's answers, the last
/// repeating.
fn sim_with_polls(mode: Mode, polls: Vec<Result<Value, AwsError>>) -> Sim {
    let sim = Sim::new(mode);
    sim.script(DESCRIBE, vec![state("pending")]);
    sim.script(INFO, vec![online()]);
    patch_words(&sim, &["patched"]);
    sim.script(POLL, polls);
    sim
}

fn success(stdout: &str) -> Result<Value, AwsError> {
    invocation("Success", stdout)
}

#[test]
fn the_question_is_sent_to_the_instance_with_its_comment_as_json() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(&sim, &["patched"]);
    sim.run().unwrap();
    let send = &sim.calls_of(SEND)[0];
    assert_eq!(send.output, Output::Json);
    assert_eq!(send.args.len(), 8, "{:?}", send.args);
    assert_eq!(
        send.args[..7].to_vec(),
        vec![
            "--instance-ids",
            ID,
            "--document-name",
            "AWS-RunShellScript",
            "--comment",
            "agentspace up: patch check",
            "--parameters",
        ]
    );
    assert!(send.args[7].starts_with("commands="), "{:?}", send.args[7]);
}

#[test]
fn the_comment_and_the_command_hold_nothing_the_cli_would_split_on() {
    // operator.md: no comma, bracket or equals sign, which the AWS CLI's shorthand syntax for
    // --parameters would split on. FA-15 adds `%`, a double quote, control characters and a
    // trailing backslash.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(&sim, &["patched"]);
    sim.run().unwrap();
    let send = &sim.calls_of(SEND)[0];
    let comment = &send.args[5];
    let question = send.args[7].strip_prefix("commands=").unwrap();
    assert!(!question.is_empty());
    for (name, text) in [("comment", comment.as_str()), ("question", question)] {
        for bad in [',', '[', ']', '='] {
            assert!(!text.contains(bad), "The {name} holds {bad:?}: {text:?}");
        }
    }
    for bad in ['%', '"'] {
        assert!(
            !question.contains(bad),
            "The question holds {bad:?}: {question:?}"
        );
    }
    assert!(
        !question.chars().any(|c| c.is_control()),
        "The question holds a control character: {question:?}"
    );
    assert!(
        !question.ends_with('\\'),
        "The question ends in a backslash: {question:?}"
    );
    assert!(question.is_ascii(), "{question:?}");
}

#[test]
fn the_question_can_give_each_of_the_three_answers() {
    // The box MUST answer with exactly one of three words; the command has to be able to say each.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(&sim, &["patched"]);
    sim.run().unwrap();
    let question = sim.calls_of(SEND)[0].args[7].clone();
    for word in ["patched", "pending", "unknown"] {
        assert!(question.contains(word), "No {word:?} in {question:?}");
    }
}

#[test]
fn the_invocation_is_polled_by_the_command_id_the_send_returned() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(&sim, &["pending", "patched"]);
    sim.run().unwrap();
    let polls = sim.calls_of(POLL);
    assert_eq!(polls.len(), 2);
    for (n, poll) in polls.iter().enumerate() {
        assert_eq!(poll.output, Output::Json);
        assert_eq!(
            poll.args,
            vec![
                "--command-id".to_string(),
                format!("{CMD_PREFIX}{n}"),
                "--instance-id".to_string(),
                ID.to_string()
            ]
        );
    }
}

#[test]
fn the_invocation_is_polled_once_a_second_until_it_ends() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    sim.script(INFO, vec![online()]);
    patch_words(&sim, &["patched"]);
    let times = Rc::new(RefCell::new(Vec::<Duration>::new()));
    let t = times.clone();
    let answers = RefCell::new(vec![
        invocation("Pending", ""),
        invocation("InProgress", ""),
        invocation("Delayed", ""),
        success("patched\n"),
    ]);
    sim.script_fn(POLL, move |_, clock| {
        t.borrow_mut().push(clock.elapsed());
        answers.borrow_mut().remove(0)
    });
    assert_eq!(sim.run(), Ok(()));
    let times = times.borrow();
    assert_eq!(times.len(), 4);
    for pair in times.windows(2) {
        assert_eq!(
            pair[1] - pair[0],
            secs(1),
            "Polled once a second: {times:?}"
        );
    }
    assert_eq!(sim.count(SEND), 1);
}

#[test]
fn a_status_that_is_pending_in_progress_or_delayed_is_polled_again_whatever_its_output() {
    for status in ["Pending", "InProgress", "Delayed"] {
        let sim = sim_with_polls(
            Mode::Person,
            vec![invocation(status, "unknown"), success("patched")],
        );
        assert_eq!(sim.run(), Ok(()), "Status {status}");
        assert_eq!(sim.count(POLL), 2, "Status {status}");
        assert_eq!(sim.count(SEND), 1, "Status {status} ends nothing");
    }
}

#[test]
fn an_invocation_that_cannot_be_read_yet_is_polled_again() {
    for error in [
        AwsError::Failed("InvocationDoesNotExist".to_string()),
        AwsError::Failed("Throttled".to_string()),
    ] {
        let sim = sim_with_polls(
            Mode::Person,
            vec![Err(error.clone()), Err(error), success("patched")],
        );
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(sim.count(POLL), 3);
        // It was not a failure of the step: no message but Ready.
        assert_eq!(sim.person_lines(), vec![format!("Ready: {ID}")]);
    }
}

/// How many 1-second sleeps lie between the first question and the next look at the agent, and
/// whether every other sleep there is the 5-second one between rounds.
fn polling_sleeps(sim: &Sim) -> (usize, Vec<Duration>) {
    let events = sim.events();
    let first_send = events
        .iter()
        .position(|e| matches!(e, Event::Call(r) if r.key() == SEND))
        .expect("A question");
    let next_info = events[first_send..]
        .iter()
        .position(|e| matches!(e, Event::Call(r) if r.key() == INFO))
        .expect("A second look")
        + first_send;
    let sleeps: Vec<Duration> = events[first_send..next_info]
        .iter()
        .filter_map(|e| match e {
            Event::Sleep(d) => Some(*d),
            _ => None,
        })
        .collect();
    let ones = sleeps.iter().filter(|d| **d == secs(1)).count();
    let others = sleeps.into_iter().filter(|d| *d != secs(1)).collect();
    (ones, others)
}

fn twenty_then_patched(unended: Result<Value, AwsError>) {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    sim.script(INFO, vec![online()]);
    patch_words(&sim, &["patched"]);
    let mut answers: Vec<Result<Value, AwsError>> = vec![unended; 20];
    answers.push(success("patched"));
    sim.script(POLL, answers);
    assert_eq!(sim.run(), Ok(()));
    // Twenty polls without an end count as `pending`: the first question is given up, and the
    // next look at the agent asks a second one, whose one poll ends the wait.
    let mut expected = vec![DESCRIBE, INFO, SEND];
    expected.extend(vec![POLL; 20]);
    expected.extend([INFO, SEND, POLL]);
    assert_eq!(sim.keys(), expected);
    assert_eq!(
        sim.person_lines(),
        vec![PATCHING.to_string(), format!("Ready: {ID}")]
    );
    let (ones, others) = polling_sleeps(&sim);
    // The invocation is polled at once and then each second, at most 20 times (operator.md, Calls
    // by command, the architect's ruling of 2026-11-19 for E0.06-T01-S03, "The waits' timing"):
    // 20 polls, the 19 pauses between them, and then the wait's own 5 seconds.
    assert_eq!(ones, 19, "Pauses of a second");
    assert_eq!(others, vec![secs(5)], "Other sleeps in the poll");
    assert_eq!(sim.elapsed(), secs(19 + 5));
}

#[test]
fn twenty_polls_without_an_end_count_as_pending() {
    twenty_then_patched(invocation("InProgress", ""));
}

#[test]
fn twenty_polls_that_cannot_be_read_also_count_as_pending() {
    twenty_then_patched(failed("InvocationDoesNotExist"));
}

#[test]
fn twenty_polls_with_no_status_also_count_as_pending() {
    twenty_then_patched(Ok(json!({})));
}

#[test]
fn the_first_poll_is_at_once_after_the_question() {
    // Polled at once and then each second: no pause lies between the question and its first poll.
    let sim = sim_with_polls(
        Mode::Person,
        vec![invocation("InProgress", ""), success("patched")],
    );
    assert_eq!(sim.run(), Ok(()));
    let events = sim.events();
    let send = events
        .iter()
        .position(|e| matches!(e, Event::Call(r) if r.key() == SEND))
        .expect("A question");
    assert!(
        matches!(&events[send + 1], Event::Call(r) if r.key() == POLL),
        "{:?}",
        events[send + 1]
    );
    assert_eq!(events[send + 2], Event::Sleep(secs(1)));
    assert_eq!(sim.sleeps(), vec![secs(1)]);
}

#[test]
fn an_invocation_with_no_string_status_is_not_readable_yet_and_is_polled_again() {
    // operator.md, Calls by command, the architect's ruling of 2026-11-19 for E0.06-T01-S03: an
    // answer with no string `Status` is not readable yet, and is polled again. It is never the
    // "Unexpected answer" failure, and its output, if it has any, is not the answer.
    for unreadable in [
        json!({}),
        json!({"Status":null}),
        json!({"Status":7}),
        json!({"Status":["Success"]}),
        json!({"StandardOutputContent":"patched"}),
        json!({"StandardOutputContent":"pending"}),
        json!([]),
        Value::Null,
    ] {
        both_modes(|mode| {
            let sim = sim_with_polls(mode, vec![Ok(unreadable.clone()), success("patched")]);
            assert_eq!(sim.run(), Ok(()), "{unreadable}");
            assert_eq!(sim.count(POLL), 2, "{unreadable}");
            assert_eq!(sim.count(SEND), 1, "{unreadable} ends nothing");
            assert_eq!(
                sim.person_lines(),
                vec![format!("Ready: {ID}")],
                "{unreadable}"
            );
        });
    }
}

#[test]
fn an_answer_on_the_twentieth_poll_is_still_read() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    sim.script(INFO, vec![online()]);
    patch_words(&sim, &["patched"]);
    let mut answers: Vec<Result<Value, AwsError>> =
        (0..19).map(|_| invocation("InProgress", "")).collect();
    answers.push(success("patched"));
    sim.script(POLL, answers);
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.count(POLL), 20);
    assert_eq!(sim.count(SEND), 1);
    assert_eq!(sim.person_lines(), vec![format!("Ready: {ID}")]);
}

#[test]
fn what_counts_as_ready_is_every_answer_but_pending() {
    let ready_answers = [
        invocation("Success", "patched"),
        invocation("Success", "patched\n"),
        invocation("Success", " \tpatched \r\n"),
        invocation("Success", "unknown"),
        invocation("Success", "unknown\n"),
        // Exceptions 2 and 3 of "Starting the box": an empty or unexpected answer from a failed
        // check counts as finished.
        invocation("Success", ""),
        invocation("Success", "\n"),
        invocation("Success", "something else"),
        invocation("Failed", ""),
        invocation("Failed", "error text"),
        invocation("TimedOut", ""),
        invocation("Cancelled", ""),
        invocation("Cancelling", ""),
        invocation("DeliveryTimedOut", ""),
        invocation("ExecutionTimedOut", ""),
        invocation("Undeliverable", ""),
        invocation("Terminated", ""),
        // An invocation that has ended with no StandardOutputContent is the empty answer, which
        // counts as ready (operator.md, Calls by command, the architect's ruling of 2026-11-19
        // for E0.06-T01-S03; Q6).
        Ok(json!({"Status":"Success"})),
        Ok(json!({"Status":"Failed"})),
        Ok(json!({"Status":"Success","StandardOutputContent":null})),
    ];
    for answer in ready_answers {
        both_modes(|mode| {
            let sim = sim_with_polls(mode, vec![answer.clone()]);
            assert_eq!(sim.run(), Ok(()), "{answer:?}");
            assert_eq!(sim.count(SEND), 1, "{answer:?} asks no second question");
            assert_eq!(
                sim.person_lines(),
                vec![format!("Ready: {ID}")],
                "{answer:?}"
            );
        });
    }
}

#[test]
fn a_pending_answer_is_trimmed_gives_the_message_and_asks_again() {
    let pending_answers = [
        success("pending"),
        success("pending\n"),
        success("  pending\r\n"),
        // The answer is the invocation's stdout whatever the status that ended the poll.
        invocation("Failed", "pending"),
    ];
    for answer in pending_answers {
        both_modes(|mode| {
            let sim = sim_with_polls(mode, vec![answer.clone(), success("patched")]);
            assert_eq!(sim.run(), Ok(()), "{answer:?}");
            assert_eq!(sim.count(SEND), 2, "{answer:?} asks again");
            assert_eq!(
                sim.person_lines(),
                vec![PATCHING.to_string(), format!("Ready: {ID}")],
                "{answer:?}"
            );
            if mode == Mode::Caller {
                assert_eq!(
                    sim.rendered(),
                    vec!["Waiting for the host to be ready...", "The host is ready."]
                );
            }
        });
    }
}

#[test]
fn patched_and_unknown_print_no_patching_message() {
    for word in ["patched", "unknown"] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state("pending")]);
        online_with(&sim, &[word]);
        assert_eq!(sim.run(), Ok(()));
        assert!(
            !sim.person_lines().contains(&PATCHING.to_string()),
            "{word} prints the patching message"
        );
    }
}

#[test]
fn ready_is_never_printed_before_the_answer_that_lets_it() {
    // up MUST NOT print Ready before the box's patch run has finished.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(&sim, &["pending", "pending", "patched"]);
    assert_eq!(sim.run(), Ok(()));
    let events = sim.events();
    let ready = events
        .iter()
        .position(
            |e| matches!(e, Event::Progress { person, .. } if *person == format!("Ready: {ID}")),
        )
        .unwrap();
    // Nothing after Ready, and the third question's poll before it.
    assert_eq!(ready, events.len() - 1);
    let last_poll = events
        .iter()
        .rposition(|e| matches!(e, Event::Call(r) if r.key() == POLL))
        .unwrap();
    assert!(last_poll < ready);
    assert_eq!(sim.count(SEND), 3);
}

#[test]
fn a_new_question_gets_its_own_command_id() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(&sim, &["pending", "patched"]);
    sim.run().unwrap();
    let ids: Vec<String> = sim
        .calls_of(POLL)
        .iter()
        .map(|c| c.arg_after("--command-id").unwrap())
        .collect();
    assert_eq!(ids, vec!["cmd-0", "cmd-1"]);
}

#[test]
fn a_question_that_fails_to_send_fails_the_step_at_once() {
    for error in [
        AwsError::SignIn,
        AwsError::Failed("AccessDenied".to_string()),
        AwsError::CannotRun(
            "agentspace: AGENTSPACE_AWS_CMD names x, which cannot be run.".to_string(),
        ),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state("pending")]);
        sim.script(INFO, vec![online()]);
        sim.script(SEND, vec![Err(error.clone())]);
        let failure = sim.run().unwrap_err();
        assert_eq!(failure, Failure::from(error));
        assert_eq!(sim.keys(), vec![DESCRIBE, INFO, SEND]);
    }
}

#[test]
fn a_send_answer_with_no_command_id_fails_the_step_without_a_backend_word_for_a_caller() {
    // FB-08's "Unexpected answer" form holds for every call's members (operator.md, Calls by
    // command, the architect's rulings of 2026-11-19 for E0.06-T01-S03), with the member's path
    // as the table writes it, `Command.CommandId`. A caller never sees it (CouldNotReach).
    for answer in [
        Ok(json!({})),
        Ok(json!({"Command":{}})),
        Ok(json!({"Command":{"CommandId":7}})),
        Ok(Value::Null),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state("pending")]);
        sim.script(INFO, vec![online()]);
        sim.script(SEND, vec![answer.clone()]);
        let failure = sim.run().unwrap_err();
        assert_eq!(
            failure.message,
            "agentspace: Unexpected answer from ssm send-command: no Command.CommandId.",
            "{answer:?}"
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{answer:?}");
        assert_eq!(sim.count(POLL), 0);
    }
}

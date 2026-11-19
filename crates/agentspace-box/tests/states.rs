//! Step 1 and step 2 of "Starting the box and waiting for it": each instance state and its
//! message, the 20-minute rule, and the second `describe-instances` only for a running box
//! (gap FB-09).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;

const DESCRIBE: &str = "ec2 describe-instances";
const START: &str = "ec2 start-instances";
const WAIT: &str = "ec2 wait instance-stopped";
const INFO: &str = "ssm describe-instance-information";
const SEND: &str = "ssm send-command";
const POLL: &str = "ssm get-command-invocation";

fn ready_line() -> String {
    format!("Ready: {ID}")
}

#[test]
fn a_running_box_launched_long_ago_is_ready_without_waiting() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        old_running(&sim);
        assert_eq!(sim.run(), Ok(()));
        // The second describe-instances reads the launch time; nothing else is asked.
        assert_eq!(sim.keys(), vec![DESCRIBE, DESCRIBE]);
        assert!(sim.sleeps().is_empty());
        assert_eq!(sim.person_lines(), vec![ready_line()]);
        match mode {
            Mode::Person => assert_eq!(sim.rendered(), vec![ready_line()]),
            Mode::Caller => assert_eq!(sim.rendered(), vec!["The host is ready."]),
        }
    });
}

#[test]
fn describe_instances_is_asked_for_the_id_and_read_as_json() {
    let sim = Sim::new(Mode::Person);
    old_running(&sim);
    sim.run().unwrap();
    for call in sim.calls_of(DESCRIBE) {
        assert_eq!(call.output, Output::Json);
        assert_eq!(
            call.text(),
            format!("ec2 describe-instances --instance-ids {ID}")
        );
        // The runner adds the region and the output format (gap FB-07); the crate does not.
        assert!(!call.text().contains("--region"));
        assert!(!call.text().contains("--output"));
    }
}

#[test]
fn the_twenty_minute_rule_is_at_twenty_minutes_exactly() {
    // 20 minutes ago or more: no wait.
    for ago in [20 * 60, 20 * 60 + 1, 60 * 60, 4 * 3600] {
        let sim = Sim::new(Mode::Person);
        sim.script(
            DESCRIBE,
            vec![state("running"), launched(&iso(ago, Style::Plus00))],
        );
        assert_eq!(sim.run(), Ok(()), "Launched {ago}s ago");
        assert_eq!(sim.count(INFO), 0, "Launched {ago}s ago must not wait");
    }
    // Under 20 minutes ago, or in the future: wait.
    for ago in [19 * 60 + 59, 10 * 60, 1, 0, -60, -3 * 3600] {
        let sim = Sim::new(Mode::Person);
        sim.script(
            DESCRIBE,
            vec![state("running"), launched(&iso(ago, Style::Plus00))],
        );
        online_with(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()), "Launched {ago}s ago");
        assert_eq!(sim.count(INFO), 1, "Launched {ago}s ago must wait");
        assert_eq!(sim.count(SEND), 1, "Launched {ago}s ago must ask the box");
    }
}

#[test]
fn the_launch_time_is_compared_in_utc_whatever_form_it_is_written_in() {
    // The same instant, 25 minutes ago, written four ways: it does not wait. Written 15 minutes
    // ago in the same four ways: it waits.
    for style in [Style::Plus00, Style::Z, Style::ZFrac, Style::Minus7] {
        let sim = Sim::new(Mode::Person);
        sim.script(
            DESCRIBE,
            vec![state("running"), launched(&iso(25 * 60, style))],
        );
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(sim.count(INFO), 0, "25 minutes ago does not wait");
    }
    for style in [Style::Plus00, Style::Z, Style::ZFrac, Style::Minus7] {
        let sim = Sim::new(Mode::Person);
        sim.script(
            DESCRIBE,
            vec![state("running"), launched(&iso(15 * 60, style))],
        );
        online_with(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(sim.count(INFO), 1, "15 minutes ago waits");
    }
}

#[test]
fn a_running_box_launched_recently_reads_the_launch_time_then_waits() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(
            DESCRIBE,
            vec![state("running"), launched(&iso(5 * 60, Style::Plus00))],
        );
        online_with(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(sim.keys(), vec![DESCRIBE, DESCRIBE, INFO, SEND, POLL]);
        // A running box has nothing to start, and prints no start message.
        assert_eq!(sim.person_lines(), vec![ready_line()]);
    });
}

#[test]
fn a_pending_box_waits_without_the_second_describe_instances() {
    // FB-09: the second describe-instances runs only when step 1 read `running`.
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(DESCRIBE, vec![state("pending")]);
        online_with(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(sim.keys(), vec![DESCRIBE, INFO, SEND, POLL]);
        assert_eq!(sim.count(START), 0);
        assert_eq!(sim.person_lines(), vec![ready_line()]);
    });
}

#[test]
fn a_stopped_box_is_started_then_waited_for_without_a_second_describe_instances() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        stopped_then_patched(&sim);
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(sim.keys(), vec![DESCRIBE, START, INFO, SEND, POLL]);
        assert_eq!(
            sim.person_lines(),
            vec![format!("Starting {ID}..."), ready_line()]
        );
        // The message comes before the request, and Ready after everything.
        assert_before(
            sim.index_of_line(&format!("Starting {ID}...")),
            sim.index_of_call(START),
        );
        assert_before(sim.index_of_call(POLL), sim.index_of_line(&ready_line()));
        let start = &sim.calls_of(START)[0];
        assert_eq!(
            start.text(),
            format!("ec2 start-instances --instance-ids {ID}")
        );
        assert_eq!(start.output, Output::Json);
        match mode {
            Mode::Person => {}
            Mode::Caller => assert_eq!(
                sim.rendered(),
                vec!["Starting the host...", "The host is ready."]
            ),
        }
    });
}

#[test]
fn a_stopping_box_is_waited_for_then_started_with_the_waiters_exit_unchecked() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(DESCRIBE, vec![state("stopping")]);
        sim.script(WAIT, vec![Ok(Value::Null)]);
        sim.script(START, vec![Ok(json!({}))]);
        online_with(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(sim.keys(), vec![DESCRIBE, WAIT, START, INFO, SEND, POLL]);
        assert_eq!(
            sim.person_lines(),
            vec![
                format!("Waiting for {ID} to finish stopping..."),
                format!("Starting {ID}..."),
                ready_line()
            ]
        );
        let waiter = &sim.calls_of(WAIT)[0];
        assert_eq!(
            waiter.text(),
            format!("ec2 wait instance-stopped --instance-ids {ID}")
        );
        // Its output is not read and its exit code not checked.
        assert_eq!(waiter.output, Output::Ignore);
        // Each message before the call it announces, and the start after the wait.
        let stopping = format!("Waiting for {ID} to finish stopping...");
        let starting = format!("Starting {ID}...");
        assert_before(sim.index_of_line(&stopping), sim.index_of_call(WAIT));
        assert_before(sim.index_of_call(WAIT), sim.index_of_line(&starting));
        assert_before(sim.index_of_line(&starting), sim.index_of_call(START));
        if mode == Mode::Caller {
            assert_eq!(
                sim.rendered(),
                vec!["Starting the host...", "The host is ready."]
            );
        }
    });
}

#[test]
fn a_waiter_that_gave_up_does_not_fail_the_step_the_start_request_is_the_check() {
    // B-34: the wait's exit code is not checked, whatever it says.
    for answer in [
        failed("Waiter InstanceStopped failed: Max attempts exceeded"),
        failed("A client error occurred"),
        Ok(Value::Null),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state("stopping")]);
        sim.script(WAIT, vec![answer]);
        sim.script(START, vec![Ok(json!({}))]);
        online_with(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(
            sim.count(START),
            1,
            "The start request follows a failed wait"
        );
    }
}

#[test]
fn a_start_request_that_fails_after_a_waiter_that_gave_up_fails_the_step_with_its_own_text() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("stopping")]);
    sim.script(WAIT, vec![failed("Max attempts exceeded")]);
    sim.script(
        START,
        vec![failed(
            "An error occurred (IncorrectInstanceState) when calling StartInstances",
        )],
    );
    let failure = sim.run().unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: An error occurred (IncorrectInstanceState) when calling StartInstances"
    );
    assert_eq!(failure.caller, CallerFailure::CouldNotReach);
    // It ends at once: nothing after the start request.
    assert_eq!(sim.keys(), vec![DESCRIBE, WAIT, START]);
}

#[test]
fn a_start_request_that_fails_for_the_sign_in_gives_the_sign_in_message() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("stopped")]);
    sim.script(START, vec![Err(AwsError::SignIn)]);
    let failure = sim.run().unwrap_err();
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(failure.caller, CallerFailure::Same);
    assert_eq!(sim.keys(), vec![DESCRIBE, START]);
}

#[test]
fn any_other_state_fails_with_the_state_and_starts_nothing() {
    for name in [
        "shutting-down",
        "terminated",
        "rebooting",
        "Running",
        "RUNNING",
        " running",
    ] {
        both_modes(|mode| {
            let sim = Sim::new(mode);
            sim.script(DESCRIBE, vec![state(name)]);
            let failure = sim.run().unwrap_err();
            assert_eq!(
                failure.message,
                format!("agentspace: Instance {ID} is {name}.")
            );
            assert_eq!(failure.caller, CallerFailure::NotAvailable);
            // One describe-instances and nothing more: no start, no wait, no message.
            assert_eq!(sim.keys(), vec![DESCRIBE], "State {name:?}");
            assert!(sim.person_lines().is_empty());
        });
    }
}

#[test]
fn the_state_names_are_read_exactly_as_the_cli_writes_them() {
    // A name that differs only in case is another state, which fails: EC2's names are lower case.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("Stopped")]);
    let failure = sim.run().unwrap_err();
    assert_eq!(
        failure.message,
        format!("agentspace: Instance {ID} is Stopped.")
    );
    assert_eq!(sim.count(START), 0);
}

#[test]
fn a_describe_instances_that_fails_fails_the_step_in_both_calls() {
    // The first call, then the second (which only a running box makes).
    for second in [false, true] {
        for error in [
            AwsError::SignIn,
            AwsError::Failed("Boom".to_string()),
            AwsError::CannotRun(
                "agentspace: AGENTSPACE_AWS_CMD names x, which cannot be run.".to_string(),
            ),
        ] {
            let sim = Sim::new(Mode::Person);
            let answers = if second {
                vec![state("running"), Err(error.clone())]
            } else {
                vec![Err(error.clone())]
            };
            sim.script(DESCRIBE, answers);
            let failure = sim.run().unwrap_err();
            assert_eq!(failure, Failure::from(error));
            // Nothing is asked after the failed call.
            assert_eq!(sim.keys(), vec![DESCRIBE; if second { 2 } else { 1 }]);
        }
    }
}

#[test]
fn a_member_the_step_does_not_read_never_fails_it() {
    // FB-08: each reader reads only its own members. The first call reads State.Name alone, and
    // its LaunchTime may be missing or of another form; the second reads LaunchTime alone, and
    // its State may be missing or of another form.
    let sim = Sim::new(Mode::Person);
    sim.script(
        DESCRIBE,
        vec![
            instance_with(
                json!({"State":{"Name":"running"},"LaunchTime":12345,"InstanceType":null}),
            ),
            instance_with(json!({"State":"not an object","LaunchTime":iso(40 * 60, Style::Z)})),
        ],
    );
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.count(INFO), 0);

    let sim = Sim::new(Mode::Person);
    sim.script(
        DESCRIBE,
        vec![instance_with(
            json!({"State":{"Name":"stopped"},"LaunchTime":"not a time"}),
        )],
    );
    sim.script(START, vec![Ok(json!({}))]);
    online_with(&sim, &["patched"]);
    assert_eq!(sim.run(), Ok(()));
    // A stopped box never reads its launch time, so the second describe-instances never runs.
    assert_eq!(sim.count(DESCRIBE), 1);
}

#[test]
fn only_the_first_reservation_and_instance_are_read() {
    let sim = Sim::new(Mode::Person);
    sim.script(
        DESCRIBE,
        vec![Ok(json!({"Reservations":[
            {"Instances":[{"State":{"Name":"pending"}},{"State":{"Name":"terminated"}}]},
            {"Instances":[{"State":{"Name":"terminated"}}]}
        ]}))],
    );
    online_with(&sim, &["patched"]);
    assert_eq!(sim.run(), Ok(()));
}

#[test]
fn no_progress_line_prints_twice_in_a_row_in_any_scenario() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("stopping")]);
    sim.script(WAIT, vec![Ok(Value::Null)]);
    sim.script(START, vec![Ok(json!({}))]);
    sim.script(
        INFO,
        vec![ping("ConnectionLost"), ping("ConnectionLost"), online()],
    );
    patch_words(&sim, &["pending", "pending", "patched"]);
    assert_eq!(sim.run(), Ok(()));
    assert_no_consecutive_repeats(&sim.person_lines());
}

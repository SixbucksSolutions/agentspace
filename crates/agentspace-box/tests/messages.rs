//! Every message of the start-and-wait step in both modes: the person's lines and failures, the
//! caller's lines and failures from "What a caller sees", and the caller's checked against that
//! section's list of backend text.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;

const DESCRIBE: &str = "ec2 describe-instances";
const START: &str = "ec2 start-instances";
const WAIT: &str = "ec2 wait instance-stopped";
const INFO: &str = "ssm describe-instance-information";

const SM: &str = "Waiting for Session Manager...";
const PATCHING: &str = "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";

fn starting() -> String {
    format!("Starting {ID}...")
}
fn stopping() -> String {
    format!("Waiting for {ID} to finish stopping...")
}
fn ready() -> String {
    format!("Ready: {ID}")
}

// -- The instrument: prove the scan finds something before believing that it found nothing -------

#[test]
fn the_backend_scan_finds_what_it_is_meant_to_find() {
    let leaks = [
        format!("Starting {ID}..."),
        "Instance i-0123abcd is stopped.".to_string(),
        "Waiting for Session Manager...".to_string(),
        "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...".to_string(),
        "Running in us-west-2".to_string(),
        "An EC2 error".to_string(),
        "The AWS CLI said".to_string(),
        "SSM agent".to_string(),
        "CloudFormation stack".to_string(),
        "Stack agentbox".to_string(),
        "The box is m9g.xlarge".to_string(),
        "Volume vol-0123456789abcdef0".to_string(),
        "State: shutting-down".to_string(),
        "State: terminated".to_string(),
        "ssh agentspace journalctl -u agentspace-patch".to_string(),
        "The instance is pending.".to_string(),
    ];
    for leak in leaks {
        assert!(!backend_text(&leak).is_empty(), "The scan missed: {leak:?}");
    }
    for clean in [
        "Starting the host...",
        "Waiting for the host to be ready...",
        "The host is ready.",
        "agentspace: The host is not available, and could not be started.",
        "agentspace: The host was not ready within 15 minutes; try again later.",
        "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.",
    ] {
        assert!(
            backend_text(clean).is_empty(),
            "A false find in {clean:?}: {:?}",
            backend_text(clean)
        );
    }
}

// -- The scenarios --------------------------------------------------------------------------------

struct Success {
    name: &'static str,
    build: fn(&Sim),
    person: Vec<String>,
    caller: Vec<&'static str>,
}

const CALLER_READY: &str = "The host is ready.";
const CALLER_STARTING: &str = "Starting the host...";
const CALLER_WAITING: &str = "Waiting for the host to be ready...";

fn successes() -> Vec<Success> {
    vec![
        Success {
            name: "A box running for hours",
            build: old_running,
            person: vec![ready()],
            caller: vec![CALLER_READY],
        },
        Success {
            name: "A box that is up and patched",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("pending")]);
                online_with(sim, &["patched"]);
            },
            person: vec![ready()],
            caller: vec![CALLER_READY],
        },
        Success {
            name: "A stopped box",
            build: stopped_then_patched,
            person: vec![starting(), ready()],
            caller: vec![CALLER_STARTING, CALLER_READY],
        },
        Success {
            name: "A stopping box",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("stopping")]);
                sim.script(WAIT, vec![Ok(Value::Null)]);
                sim.script(START, vec![Ok(json!({}))]);
                online_with(sim, &["patched"]);
            },
            person: vec![stopping(), starting(), ready()],
            caller: vec![CALLER_STARTING, CALLER_READY],
        },
        Success {
            name: "A pending box whose agent is not up yet",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("pending")]);
                sim.script(INFO, vec![no_information(), online()]);
                patch_words(sim, &["patched"]);
            },
            person: vec![SM.to_string(), ready()],
            caller: vec![CALLER_WAITING, CALLER_READY],
        },
        Success {
            name: "A pending box that is patching",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("pending")]);
                online_with(sim, &["pending", "patched"]);
            },
            person: vec![PATCHING.to_string(), ready()],
            caller: vec![CALLER_WAITING, CALLER_READY],
        },
        Success {
            name: "A stopped box through every phase",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("stopped")]);
                sim.script(START, vec![Ok(json!({}))]);
                sim.script(
                    INFO,
                    vec![no_information(), online(), no_information(), online()],
                );
                patch_words(sim, &["pending", "unknown"]);
            },
            person: vec![
                starting(),
                SM.to_string(),
                PATCHING.to_string(),
                SM.to_string(),
                ready(),
            ],
            caller: vec![CALLER_STARTING, CALLER_WAITING, CALLER_READY],
        },
        Success {
            name: "A stopping box through every phase",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("stopping")]);
                sim.script(WAIT, vec![failed("Max attempts exceeded")]);
                sim.script(START, vec![Ok(json!({}))]);
                sim.script(INFO, vec![no_information(), online()]);
                patch_words(sim, &["pending", "patched"]);
            },
            person: vec![
                stopping(),
                starting(),
                SM.to_string(),
                PATCHING.to_string(),
                ready(),
            ],
            caller: vec![CALLER_STARTING, CALLER_WAITING, CALLER_READY],
        },
        Success {
            name: "A running box that started recently",
            build: |sim| {
                sim.script(
                    DESCRIBE,
                    vec![state("running"), launched(&iso(60, Style::Z))],
                );
                sim.script(INFO, vec![online()]);
                patch_words(sim, &["pending", "patched"]);
            },
            person: vec![PATCHING.to_string(), ready()],
            caller: vec![CALLER_WAITING, CALLER_READY],
        },
    ]
}

#[test]
fn a_persons_lines_are_the_ones_operator_md_gives_in_order() {
    for scenario in successes() {
        let sim = Sim::new(Mode::Person);
        (scenario.build)(&sim);
        assert_eq!(sim.run(), Ok(()), "{}", scenario.name);
        assert_eq!(sim.person_lines(), scenario.person, "{}", scenario.name);
        assert_eq!(sim.rendered(), scenario.person, "{}", scenario.name);
        assert_no_consecutive_repeats(&sim.person_lines());
    }
}

#[test]
fn a_callers_lines_are_the_three_progress_lines_each_at_most_once_in_order() {
    for scenario in successes() {
        let sim = Sim::new(Mode::Caller);
        (scenario.build)(&sim);
        assert_eq!(sim.run(), Ok(()), "{}", scenario.name);
        assert_eq!(sim.rendered(), scenario.caller, "{}", scenario.name);
    }
}

#[test]
fn a_callers_lines_hold_no_backend_text() {
    for scenario in successes() {
        let sim = Sim::new(Mode::Caller);
        (scenario.build)(&sim);
        sim.run().unwrap();
        for line in sim.rendered() {
            assert_no_backend_text(&line);
        }
    }
    for line in [CallerLine::Starting, CallerLine::Waiting, CallerLine::Ready] {
        assert_no_backend_text(caller_line_text(line));
    }
}

#[test]
fn the_crate_asks_for_the_callers_line_that_replaces_each_of_the_persons() {
    // The person's text, and which of the caller's three lines stands in for it
    // (operator.md, What a caller sees).
    for scenario in successes() {
        let sim = Sim::new(Mode::Caller);
        (scenario.build)(&sim);
        sim.run().unwrap();
        for (person, caller) in sim.progress_pairs() {
            if person == starting() {
                assert_eq!(caller, Some(CallerLine::Starting), "{person}");
            } else if person == stopping() {
                // `Waiting for <id> to finish stopping...` and `Starting <id>...` make one line,
                // `Starting the host...`; either `None` or `Some(Starting)` prints it once.
                assert!(
                    matches!(caller, None | Some(CallerLine::Starting)),
                    "{person}: {caller:?}"
                );
            } else if person == SM || person == PATCHING {
                assert_eq!(caller, Some(CallerLine::Waiting), "{person}");
            } else if person == ready() {
                assert_eq!(caller, Some(CallerLine::Ready), "{person}");
            } else {
                panic!("A line operator.md does not give: {person:?}");
            }
        }
    }
}

#[test]
fn the_callers_lines_come_in_the_order_the_pin_gives() {
    // Starting, then Waiting, then Ready, never back.
    for scenario in successes() {
        let sim = Sim::new(Mode::Caller);
        (scenario.build)(&sim);
        sim.run().unwrap();
        let rank = |l: CallerLine| match l {
            CallerLine::Starting => 0,
            CallerLine::Waiting => 1,
            CallerLine::Ready => 2,
        };
        let asked: Vec<i32> = sim
            .progress_pairs()
            .into_iter()
            .filter_map(|(_, c)| c.map(rank))
            .collect();
        assert!(
            asked.windows(2).all(|w| w[0] <= w[1]),
            "{}: {asked:?}",
            scenario.name
        );
        assert_eq!(asked.last(), Some(&2), "{}", scenario.name);
    }
}

#[test]
fn the_step_prints_the_same_in_both_modes_and_leaves_the_rewrite_to_say() {
    // The crate does not branch on the mode: it gives both texts, and `Say` picks.
    for scenario in successes() {
        let person = Sim::new(Mode::Person);
        let caller = Sim::new(Mode::Caller);
        (scenario.build)(&person);
        (scenario.build)(&caller);
        assert_eq!(person.run(), caller.run());
        assert_eq!(
            person.progress_pairs(),
            caller.progress_pairs(),
            "{}",
            scenario.name
        );
        assert_eq!(person.calls(), caller.calls(), "{}", scenario.name);
        assert_eq!(person.sleeps(), caller.sleeps(), "{}", scenario.name);
    }
}

#[test]
fn the_step_prints_no_warning_and_no_detail_of_its_own() {
    // Only the token push warns (operator.md, Messages); the detail the backend adds is for a
    // caller who asks, and this step has none to give that a failure's message does not.
    for scenario in successes() {
        let sim = Sim::new(Mode::Caller);
        (scenario.build)(&sim);
        sim.run().unwrap();
        assert!(sim.warnings().is_empty(), "{}", scenario.name);
    }
}

#[test]
fn a_callers_lines_hold_no_backend_text_even_with_diagnostics_on() {
    // AGENTSPACE_DIAGNOSTIC may add detail, but the progress lines stay the caller's.
    for scenario in successes() {
        let sim = Sim::with_diagnostic(Mode::Caller, true);
        (scenario.build)(&sim);
        sim.run().unwrap();
        let lines = sim.rendered();
        for line in sim.rendered() {
            // Detail, if the crate gives any, is allowed to hold backend text; a progress line is not.
            if lines.len() == scenario.caller.len() {
                assert_no_backend_text(&line);
            }
        }
    }
}

#[test]
fn every_persons_line_starts_with_a_capital() {
    for scenario in successes() {
        for line in scenario.person {
            assert_starts_with_capital(&line);
        }
    }
    for line in [CallerLine::Starting, CallerLine::Waiting, CallerLine::Ready] {
        assert_starts_with_capital(caller_line_text(line));
    }
}

// -- The failures -----------------------------------------------------------------------------------

struct FailureCase {
    name: &'static str,
    build: fn(&Sim),
    person: String,
    caller: CallerFailure,
    /// What a caller sees in its place.
    caller_line: String,
}

const NOT_AVAILABLE: &str = "agentspace: The host is not available, and could not be started.";
const NOT_READY: &str = "agentspace: The host was not ready within 15 minutes; try again later.";
const COULD_NOT_REACH: &str = "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.";

fn failures() -> Vec<FailureCase> {
    vec![
        FailureCase {
            name: "A terminated box",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("terminated")]);
            },
            person: format!("agentspace: Instance {ID} is terminated."),
            caller: CallerFailure::NotAvailable,
            caller_line: NOT_AVAILABLE.to_string(),
        },
        FailureCase {
            name: "A box shutting down",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("shutting-down")]);
            },
            person: format!("agentspace: Instance {ID} is shutting-down."),
            caller: CallerFailure::NotAvailable,
            caller_line: NOT_AVAILABLE.to_string(),
        },
        FailureCase {
            name: "A box that was never ready",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("pending")]);
                sim.script(INFO, vec![no_information()]);
            },
            person: format!(
                "agentspace: {ID} was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
            ),
            caller: CallerFailure::NotReady,
            caller_line: NOT_READY.to_string(),
        },
        FailureCase {
            name: "A patch run that never finished",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("pending")]);
                online_with(sim, &["pending"]);
            },
            person: format!(
                "agentspace: {ID} was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
            ),
            caller: CallerFailure::NotReady,
            caller_line: NOT_READY.to_string(),
        },
        FailureCase {
            name: "An AWS failure",
            build: |sim| {
                sim.script(DESCRIBE, vec![failed("An error occurred (Throttling) calling DescribeInstances")]);
            },
            person: "agentspace: An error occurred (Throttling) calling DescribeInstances".to_string(),
            caller: CallerFailure::CouldNotReach,
            caller_line: COULD_NOT_REACH.to_string(),
        },
        FailureCase {
            name: "A missing AWS CLI",
            build: |sim| {
                sim.script(
                    DESCRIBE,
                    vec![Err(AwsError::CannotRun(
                        "agentspace: AGENTSPACE_AWS_CMD names x, which cannot be run.".to_string(),
                    ))],
                );
            },
            person: "agentspace: AGENTSPACE_AWS_CMD names x, which cannot be run.".to_string(),
            caller: CallerFailure::CouldNotReach,
            caller_line: COULD_NOT_REACH.to_string(),
        },
        FailureCase {
            name: "An unexpected answer",
            build: |sim| {
                sim.script(DESCRIBE, vec![Ok(json!({}))]);
            },
            person: "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].State.Name.".to_string(),
            caller: CallerFailure::CouldNotReach,
            caller_line: COULD_NOT_REACH.to_string(),
        },
        FailureCase {
            name: "A start request that fails",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("stopped")]);
                sim.script(START, vec![failed("InsufficientInstanceCapacity")]);
            },
            person: "agentspace: InsufficientInstanceCapacity".to_string(),
            caller: CallerFailure::CouldNotReach,
            caller_line: COULD_NOT_REACH.to_string(),
        },
        FailureCase {
            name: "An expired sign-in",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("stopped")]);
                sim.script(START, vec![Err(AwsError::SignIn)]);
            },
            person: SIGN_IN_MESSAGE.to_string(),
            caller: CallerFailure::Same,
            // The one exception: it names AWS, and a caller sees it as it is (A-14).
            caller_line: SIGN_IN_MESSAGE.to_string(),
        },
        FailureCase {
            name: "An expired sign-in while waiting",
            build: |sim| {
                sim.script(DESCRIBE, vec![state("pending")]);
                sim.script(INFO, vec![Err(AwsError::SignIn)]);
            },
            person: SIGN_IN_MESSAGE.to_string(),
            caller: CallerFailure::Same,
            caller_line: SIGN_IN_MESSAGE.to_string(),
        },
    ]
}

#[test]
fn a_persons_failures_are_the_messages_operator_md_gives() {
    for case in failures() {
        let sim = Sim::new(Mode::Person);
        (case.build)(&sim);
        let failure = sim.run().unwrap_err();
        assert_eq!(failure.message, case.person, "{}", case.name);
        assert_eq!(
            render_failure(Mode::Person, &failure),
            case.person,
            "{}",
            case.name
        );
        assert_eq!(failure.caller, case.caller, "{}", case.name);
        assert_starts_with_capital(&failure.message);
        assert!(!failure.message.contains('\n'), "{}", case.name);
    }
}

#[test]
fn a_callers_failures_are_the_callers_table_and_hold_no_backend_text() {
    for case in failures() {
        let sim = Sim::new(Mode::Caller);
        (case.build)(&sim);
        let failure = sim.run().unwrap_err();
        assert_eq!(failure.caller, case.caller, "{}", case.name);
        let shown = render_failure(Mode::Caller, &failure);
        assert_eq!(shown, case.caller_line, "{}", case.name);
        if failure.caller == CallerFailure::Same {
            // #0034's sign-in message, which names AWS, is the one exception.
            assert_eq!(shown, SIGN_IN_MESSAGE);
        } else {
            assert_no_backend_text(&shown);
            assert!(shown.starts_with("agentspace: "), "{}", case.name);
        }
    }
}

#[test]
fn only_the_sign_in_failure_is_passed_to_a_caller_as_it_is() {
    for case in failures() {
        let same = case.caller == CallerFailure::Same;
        assert_eq!(same, case.person == SIGN_IN_MESSAGE, "{}", case.name);
    }
}

#[test]
fn a_failure_prints_no_ready_and_a_callers_progress_lines_before_it_hold_no_backend_text() {
    for case in failures() {
        let sim = Sim::new(Mode::Caller);
        (case.build)(&sim);
        sim.run().unwrap_err();
        assert!(sim.index_of_line(&ready()).is_none(), "{}", case.name);
        for line in sim.rendered() {
            assert_no_backend_text(&line);
        }
    }
}

#[test]
fn a_failure_is_the_last_thing_the_step_does() {
    for case in failures() {
        let sim = Sim::new(Mode::Person);
        (case.build)(&sim);
        sim.run().unwrap_err();
        // No sleep after the last call: the step stops at the failing call or at the deadline.
        let events = sim.events();
        let last_progress = events
            .iter()
            .rposition(|e| matches!(e, Event::Progress { .. }));
        let last_call = events.iter().rposition(|e| matches!(e, Event::Call(_)));
        assert!(last_call.is_some(), "{}", case.name);
        let _ = last_progress;
        assert!(
            !matches!(events.last(), Some(Event::Progress { person, .. }) if *person == ready())
        );
    }
}

#[test]
fn the_start_failure_prints_starting_but_the_unavailable_state_prints_nothing() {
    // A stopped box announces the start before asking; a box in any other state is refused before
    // any message.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("stopped")]);
    sim.script(START, vec![failed("Nope")]);
    sim.run().unwrap_err();
    assert_eq!(sim.person_lines(), vec![starting()]);

    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("terminated")]);
    sim.run().unwrap_err();
    assert!(sim.person_lines().is_empty());
}

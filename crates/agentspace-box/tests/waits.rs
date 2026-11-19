//! Step 3 of "Starting the box and waiting for it": the 15-minute wait, checked every 5 seconds,
//! its messages printed only when they differ from the last, and the deadline.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const DESCRIBE: &str = "ec2 describe-instances";
const START: &str = "ec2 start-instances";
const WAIT: &str = "ec2 wait instance-stopped";
const INFO: &str = "ssm describe-instance-information";
const SEND: &str = "ssm send-command";

const SM: &str = "Waiting for Session Manager...";
const PATCHING: &str = "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";

fn deadline_message() -> String {
    format!(
        "agentspace: {ID} was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
    )
}

/// Scripts `ssm describe-instance-information` from `pings` (the last repeats) and returns the
/// fake clock's reading at each call.
fn timed_info(sim: &Sim, pings: Vec<Result<Value, AwsError>>) -> Rc<RefCell<Vec<Duration>>> {
    let times = Rc::new(RefCell::new(Vec::new()));
    let t = times.clone();
    let answers = RefCell::new(pings.into_iter().collect::<std::collections::VecDeque<_>>());
    sim.script_fn(INFO, move |_, clock| {
        t.borrow_mut().push(clock.elapsed());
        let mut a = answers.borrow_mut();
        if a.len() > 1 {
            a.pop_front().unwrap()
        } else {
            a[0].clone()
        }
    });
    times
}

#[test]
fn a_box_that_is_not_online_is_checked_every_five_seconds_and_says_so_once() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(DESCRIBE, vec![state("pending")]);
        let mut pings = vec![no_information(); 4];
        pings.push(online());
        let times = timed_info(&sim, pings);
        patch_words(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()));
        let times = times.borrow();
        assert_eq!(times.len(), 5);
        for pair in times.windows(2) {
            assert_eq!(
                pair[1] - pair[0],
                secs(5),
                "Checked every 5 seconds: {times:?}"
            );
        }
        assert!(sim.sleeps().iter().all(|d| *d == secs(5)));
        assert_eq!(
            sim.person_lines(),
            vec![SM.to_string(), format!("Ready: {ID}")]
        );
        if mode == Mode::Caller {
            assert_eq!(
                sim.rendered(),
                vec!["Waiting for the host to be ready...", "The host is ready."]
            );
        }
    });
}

#[test]
fn the_first_look_for_the_agent_costs_no_time() {
    // The 5 seconds are a pause after each check (operator.md, Calls by command, the architect's
    // ruling of 2026-11-19 for E0.06-T01-S03, "The waits' timing"), so a box already Online is
    // asked at once: no pause comes before the first look.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(&sim, &["patched"]);
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.elapsed(), secs(0));
}

#[test]
fn the_agent_is_online_only_when_its_ping_status_is_online() {
    for status in [
        "ConnectionLost",
        "Inactive",
        "online",
        "ONLINE",
        "",
        "Offline",
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state("pending")]);
        sim.script(INFO, vec![ping(status), online()]);
        patch_words(&sim, &["patched"]);
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(sim.count(INFO), 2, "PingStatus {status:?} is not Online");
        assert_eq!(sim.count(SEND), 1);
    }
    // An empty list is not online either.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    sim.script(INFO, vec![no_information(), online()]);
    patch_words(&sim, &["patched"]);
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.count(INFO), 2);
}

#[test]
fn the_agent_is_asked_about_through_the_instance_and_read_as_json() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(&sim, &["patched"]);
    sim.run().unwrap();
    let call = &sim.calls_of(INFO)[0];
    assert_eq!(
        call.text(),
        format!("ssm describe-instance-information --filters Key=InstanceIds,Values={ID}")
    );
    assert_eq!(call.output, Output::Json);
}

#[test]
fn a_message_prints_only_when_it_differs_from_the_last_one_printed() {
    // Offline, offline, online and pending, offline again (the patch run's reboot), online and
    // pending, online and pending, online and patched.
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(DESCRIBE, vec![state("pending")]);
        sim.script(
            INFO,
            vec![
                no_information(),
                ping("ConnectionLost"),
                online(),
                no_information(),
                online(),
                online(),
                online(),
            ],
        );
        patch_words(&sim, &["pending", "pending", "pending", "patched"]);
        assert_eq!(sim.run(), Ok(()));
        assert_eq!(
            sim.person_lines(),
            vec![
                SM.to_string(),
                PATCHING.to_string(),
                SM.to_string(),
                PATCHING.to_string(),
                format!("Ready: {ID}"),
            ]
        );
        assert_no_consecutive_repeats(&sim.person_lines());
        if mode == Mode::Caller {
            // The caller's waiting line prints at most once per run, however many phases.
            assert_eq!(
                sim.rendered(),
                vec!["Waiting for the host to be ready...", "The host is ready."]
            );
        }
    });
}

#[test]
fn a_long_patch_run_shows_its_message_once() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    online_with(
        &sim,
        &["pending", "pending", "pending", "pending", "patched"],
    );
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(
        sim.person_lines(),
        vec![PATCHING.to_string(), format!("Ready: {ID}")]
    );
    // A new question each time the box is looked at, 5 seconds apart.
    assert_eq!(sim.count(SEND), 5);
}

#[test]
fn the_wait_gives_up_after_fifteen_minutes_with_the_deadline_message() {
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(DESCRIBE, vec![state("pending")]);
        sim.script(INFO, vec![no_information()]);
        let failure = sim.run().unwrap_err();
        assert_eq!(failure.message, deadline_message());
        assert_eq!(failure.caller, CallerFailure::NotReady);
        // From the start of the wait, 15 minutes exactly: the deadline is tested after each check
        // that was not ready, so the check at 900 seconds is the last, the 181st, and no pause
        // follows it (operator.md, Calls by command, "The waits' timing").
        assert_eq!(sim.elapsed(), secs(900));
        assert_eq!(sim.count(INFO), 181);
        assert_eq!(sim.sleeps(), vec![secs(5); 180]);
        assert!(matches!(sim.events().last(), Some(Event::Call(r)) if r.key() == INFO));
        // Never Ready, and the message that explained the wait printed once.
        assert_eq!(sim.person_lines(), vec![SM.to_string()]);
        assert!(sim.index_of_line(&format!("Ready: {ID}")).is_none());
    });
}

#[test]
fn a_patch_run_that_never_finishes_also_ends_in_the_deadline() {
    // A failed run writes no marker, so the answer stays `pending` until the deadline.
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(DESCRIBE, vec![state("pending")]);
        online_with(&sim, &["pending"]);
        let failure = sim.run().unwrap_err();
        assert_eq!(failure.message, deadline_message());
        assert_eq!(failure.caller, CallerFailure::NotReady);
        // Each round is a look, a question and its answer at the first poll, and 5 seconds.
        assert_eq!(sim.elapsed(), secs(900));
        assert_eq!(sim.count(SEND), 181);
        assert_eq!(sim.person_lines(), vec![PATCHING.to_string()]);
    });
}

#[test]
fn the_fifteen_minutes_count_from_the_start_of_the_wait_not_from_the_waiters_start() {
    // The stopped box's `ec2 wait` takes 12 minutes (its own default limit is about 10). The wait
    // for the box then still gets its own 15 minutes.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("stopping")]);
    sim.script_fn(WAIT, |_, clock| {
        clock.advance(secs(12 * 60));
        Ok(Value::Null)
    });
    sim.script(START, vec![Ok(json!({}))]);
    sim.script(INFO, vec![no_information()]);
    let failure = sim.run().unwrap_err();
    assert_eq!(failure.message, deadline_message());
    let in_wait = sim.elapsed() - secs(12 * 60);
    assert_eq!(in_wait, secs(900));
    assert_eq!(sim.count(INFO), 181);
}

#[test]
fn time_spent_before_the_wait_does_not_shorten_it() {
    // A start request that takes time (modelled by a slow describe-instances on a young box).
    let sim = Sim::new(Mode::Person);
    sim.script_fn(DESCRIBE, |_, clock| {
        clock.advance(secs(60));
        state("pending")
    });
    sim.script(INFO, vec![no_information()]);
    let failure = sim.run().unwrap_err();
    assert_eq!(failure.caller, CallerFailure::NotReady);
    let in_wait = sim.elapsed() - secs(60);
    assert_eq!(in_wait, secs(900));
}

#[test]
fn a_box_online_just_before_the_deadline_is_ready() {
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    let mut pings = vec![no_information(); 170];
    pings.push(online());
    sim.script(INFO, pings);
    patch_words(&sim, &["patched"]);
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.elapsed(), secs(170 * 5));
}

#[test]
fn a_failed_check_of_the_agent_fails_the_step_at_once() {
    for error in [
        AwsError::SignIn,
        AwsError::Failed("Throttled".to_string()),
        AwsError::CannotRun(
            "agentspace: AGENTSPACE_AWS_CMD names x, which cannot be run.".to_string(),
        ),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state("pending")]);
        sim.script(INFO, vec![no_information(), Err(error.clone())]);
        let failure = sim.run().unwrap_err();
        assert_eq!(failure, Failure::from(error));
        assert_eq!(sim.count(INFO), 2);
        assert_eq!(sim.count(SEND), 0);
    }
}

#[test]
fn every_wait_of_every_start_path_checks_the_agent_with_no_other_pause() {
    // Whatever the path in, the only sleeps of the wait are its 5-second ones, and the stopped
    // box's start and the stopping box's waiter add none of their own.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("stopping")]);
    sim.script(WAIT, vec![Ok(Value::Null)]);
    sim.script(START, vec![Ok(json!({}))]);
    sim.script(INFO, vec![no_information(), no_information(), online()]);
    patch_words(&sim, &["patched"]);
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.sleeps(), vec![secs(5), secs(5)]);
}

// -- The waits' timing (operator.md, Calls by command, the architect's rulings of 2026-11-19 for
// E0.06-T01-S03): a pause of 5 seconds after each check, the deadline tested after each check that
// was not ready, and the last pause cut short to end on the deadline. -----------------------------

/// Scripts a look at the agent that itself takes `each` and never finds it, and returns the fake
/// clock's reading as each look began.
fn slow_looks(sim: &Sim, each: Duration) -> Rc<RefCell<Vec<Duration>>> {
    let began = Rc::new(RefCell::new(Vec::new()));
    let b = began.clone();
    sim.script_fn(INFO, move |_, clock| {
        b.borrow_mut().push(clock.elapsed());
        clock.advance(each);
        no_information()
    });
    began
}

#[test]
fn the_last_pause_is_cut_short_to_end_on_the_deadline() {
    // Each look takes 3 seconds, so a round is 8: the 113th look ends at 899 seconds, a second
    // short of the deadline. The pause after it is that 1 second, and the last look begins at 900.
    both_modes(|mode| {
        let sim = Sim::new(mode);
        sim.script(DESCRIBE, vec![state("pending")]);
        let began = slow_looks(&sim, secs(3));
        let failure = sim.run().unwrap_err();
        assert_eq!(failure.message, deadline_message());
        assert_eq!(failure.caller, CallerFailure::NotReady);
        let began = began.borrow();
        assert_eq!(began.len(), 114);
        assert_eq!(
            began.last(),
            Some(&secs(900)),
            "The last look is at the deadline"
        );
        let mut expected = vec![secs(5); 112];
        expected.push(secs(1));
        assert_eq!(sim.sleeps(), expected);
        // The step ends with that look: it took 3 seconds, and no pause follows it.
        assert_eq!(sim.elapsed(), secs(903));
        assert_eq!(sim.person_lines(), vec![SM.to_string()]);
    });
}

#[test]
fn no_pause_ever_runs_past_the_deadline() {
    // Whatever a look takes, a pause that begins before the deadline ends on it or before it.
    for each in [1, 2, 3, 4, 6, 7, 11, 13, 61] {
        let sim = Sim::new(Mode::Person);
        sim.script(DESCRIBE, vec![state("pending")]);
        slow_looks(&sim, secs(each));
        sim.run().unwrap_err();
        let mut at = Duration::ZERO;
        for event in sim.events() {
            match event {
                Event::Call(r) if r.key() == INFO => at += secs(each),
                Event::Sleep(d) => {
                    assert!(d > Duration::ZERO && d <= secs(5), "A pause of {d:?}");
                    assert!(
                        at < secs(900),
                        "A pause that begins at {at:?} ({each}s looks)"
                    );
                    at += d;
                    assert!(
                        at <= secs(900),
                        "A pause that ends at {at:?} ({each}s looks)"
                    );
                }
                _ => {}
            }
        }
        assert!(at >= secs(900), "Gave up early at {at:?} ({each}s looks)");
    }
}

#[test]
fn a_look_that_ends_past_the_deadline_is_the_last_and_no_pause_follows_it() {
    // A look of 7 seconds: the 75th begins at 888 and ends at 895, the pause after it is 5 seconds
    // to the deadline, and the 76th, which begins at 900 and ends at 907, is the last.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    let began = slow_looks(&sim, secs(7));
    let failure = sim.run().unwrap_err();
    assert_eq!(failure.message, deadline_message());
    assert_eq!(began.borrow().last(), Some(&secs(900)));
    assert_eq!(sim.elapsed(), secs(907));
    assert!(matches!(sim.events().last(), Some(Event::Call(r)) if r.key() == INFO));
}

/// A box that is not online for 179 looks (to 890 seconds) and online at the 180th, at 895, whose
/// question is then answered `word` at its 11th poll, 10 seconds later: 5 seconds past the
/// deadline.
fn answered_past_the_deadline(mode: Mode, word: &'static str) -> Sim {
    let sim = Sim::new(mode);
    sim.script(DESCRIBE, vec![state("pending")]);
    let mut pings = vec![no_information(); 179];
    pings.push(online());
    sim.script(INFO, pings);
    sim.script(SEND, vec![sent("cmd-0")]);
    let mut polls = vec![invocation("InProgress", ""); 10];
    polls.push(invocation("Success", word));
    sim.script("ssm get-command-invocation", polls);
    sim
}

#[test]
fn a_check_that_finds_the_box_ready_past_the_deadline_still_ends_the_wait() {
    // The deadline is tested after each check that was not ready, and only then: a check that
    // began before it and found the box ready is not a failure.
    for word in ["patched", "unknown", ""] {
        both_modes(|mode| {
            let sim = answered_past_the_deadline(mode, word);
            assert_eq!(sim.run(), Ok(()), "{word:?}");
            assert_eq!(sim.elapsed(), secs(905));
            assert_eq!(
                sim.person_lines(),
                vec![SM.to_string(), format!("Ready: {ID}")]
            );
        });
    }
}

#[test]
fn a_check_that_finds_the_box_pending_past_the_deadline_fails_at_once() {
    both_modes(|mode| {
        let sim = answered_past_the_deadline(mode, "pending");
        let failure = sim.run().unwrap_err();
        assert_eq!(failure.message, deadline_message());
        assert_eq!(failure.caller, CallerFailure::NotReady);
        // No pause and no further look after the check that ran past the deadline.
        assert_eq!(sim.elapsed(), secs(905));
        assert_eq!(sim.count(INFO), 180);
        assert_eq!(sim.count(SEND), 1);
        assert!(sim.index_of_line(&format!("Ready: {ID}")).is_none());
    });
}

#[test]
fn a_box_ready_at_the_last_look_at_the_deadline_is_ready() {
    // The 181st look, at 900 seconds, is still a check: a box that is ready then is ready.
    let sim = Sim::new(Mode::Person);
    sim.script(DESCRIBE, vec![state("pending")]);
    let mut pings = vec![no_information(); 180];
    pings.push(online());
    sim.script(INFO, pings);
    patch_words(&sim, &["patched"]);
    assert_eq!(sim.run(), Ok(()));
    assert_eq!(sim.elapsed(), secs(900));
    assert_eq!(sim.count(INFO), 181);
}

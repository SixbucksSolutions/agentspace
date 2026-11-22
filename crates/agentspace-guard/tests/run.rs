//! The guard's run, `agentspace_guard::run` (story E0.08-T03-S01, card #0177): the calls it makes,
//! what it logs, what it says, what it never writes, and every failure it can meet.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Written from docs/operator.md, "guard", "Its log", "Calls by command" and "AWS failures", and
//! from docs/state.md, "The guard's files", "The provisioning run's lock", "The host registry" and
//! "Live leases and the lease horizon", against the pinned API in `src/lib.rs`. Every run goes
//! through the stand-ins of `run_support`: a recording `Aws`, a recording `Say` and a throwaway
//! home under `%TEMP%`. No test calls real AWS, and none sets a process variable.
//!
//! Where operator.md leaves a form open, a test holds what it requires and no more:
//! - a host's part that fails, and a dropped log line, are said through `Say::warning` and no other
//!   channel (the guard's failure channel; architect, 2026-11-22, for v0.08-T03), and a progress
//!   line goes through `Say::progress` alone;
//! - the progress line of an unexplained host is matched up to its `<h:mm><am|pm>`, which is read
//!   in the laptop's zone and so cannot be fixed by a test;
//! - the horizon's command is the operator's own (D34), so a test holds the horizon's digits as one
//!   word of it, and no comma, bracket or equals sign.
#![cfg(windows)]

mod run_support;

use run_support::*;

// ---------------------------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------------------------

fn strings(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.to_string()).collect()
}

/// The mixed registry of operator.md, guard, steps 4 to 7, in this order:
///
/// | Project | Record | Instance | Lease state | Decided |
/// |---|---|---|---|---|
/// | `swimcoach` | the box, ID `id(0)` | running since 08:00 | live `ready` until 23:00 | explained |
/// | `alpha` | `ready`, ID `id(1)` | running since 08:00 | released at 19:30 | due at 19:45: stopped |
/// | `beta` | `ready`, no ID, stack gives `ZEROS` | running since 09:00 | released at 19:50 | unexplained, stops 20:05 |
/// | `gamma` | `creating`, no ID | none | none | skipped |
/// | `delta` | `ready`, ID `id(3)` | stopped | none | nothing |
/// | `epsilon` | `ready`, ID `id(4)` | pending, no `LaunchTime` | none | nothing |
///
/// The run's time is `now()`, 20:00:00 on 2026-11-22.
fn mixed(name: &str) -> World {
    let w = World::new(name);
    w.write_hosts(&[
        box_host(Some(&id(0))),
        ready_host("alpha", Some(&id(1))),
        ready_host("beta", None),
        host("gamma", &stack_of("gamma"), "creating", None),
        ready_host("delta", Some(&id(3))),
        ready_host("epsilon", Some(&id(4))),
    ]);
    w.write_leases(&[
        lease_ready("swimcoach", &today("23:00:00")),
        lease_released("alpha", &today("19:30:00"), Some(&today("22:00:00"))),
        lease_released("beta", &today("19:50:00"), None),
    ]);
    w.aws.set_stack_id(&stack_of("beta"), ZEROS);
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(&id(1), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(ZEROS, "running", Some(&launch("09:00:00")));
    w.aws
        .add_instance(&id(3), "stopped", Some(&launch("07:00:00")));
    w.aws.add_instance(&id(4), "pending", None);
    w
}

fn alpha_stopped() -> String {
    format!("host=alpha stopped since={}", today("19:30:00"))
}

fn beta_unexplained() -> String {
    format!(
        "host=beta unexplained since={} stops={}",
        today("19:50:00"),
        today("20:05:00")
    )
}

fn sign_in_line() -> String {
    format!("failed step=SSS reason={SIGN_IN_REASON}")
}

fn sign_in_at(step: &str) -> String {
    sign_in_line().replace("SSS", step)
}

/// A box that a live lease explains, and two hosts that no lease explains and that are due: the
/// run's time is 20:00, and both last leases ended at 19:00.
fn two_due(name: &str) -> World {
    let w = World::new(name);
    w.write_hosts(&[
        box_host(Some(&id(0))),
        ready_host("alpha", Some(&id(1))),
        ready_host("zeta", Some(&id(5))),
    ]);
    w.write_leases(&[
        lease_ready("swimcoach", &today("23:00:00")),
        lease_released("alpha", &today("19:00:00"), Some(&today("21:00:00"))),
        lease_released("zeta", &today("19:00:00"), None),
    ]);
    for n in [0, 1, 5] {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    w
}

// ---------------------------------------------------------------------------------------------
// The calls, in order (operator.md, guard, steps 4 to 7; Calls by command).
// ---------------------------------------------------------------------------------------------

/// Steps 4 to 7: the stack read for the one record with no ID, then one describe, one stop for
/// the due host, and one send for each running host that was not stopped, in the registry's order.
#[test]
fn the_calls_come_in_the_order_of_the_steps() {
    let w = mixed("calls-order");
    w.run_ok(now());
    assert_eq!(
        w.call_keys(),
        strings(&[
            "cloudformation describe-stacks",
            "ec2 describe-instances",
            "ec2 stop-instances",
            "ssm send-command",
            "ssm send-command",
        ])
    );
    assert_eq!(w.stopped_ids(), vec![id(1)]);
    assert_eq!(w.sent_ids(), vec![id(0), ZEROS.to_string()]);
}

/// Step 4: `cloudformation describe-stacks --stack-name <stack>` (JSON), for a `ready` record with
/// no ID alone, in the registry's order; none for `creating` `gamma`, and none for a record with
/// an ID.
#[test]
fn a_ready_record_with_no_id_is_read_from_its_stack() {
    let w = mixed("stack-read");
    w.run_ok(now());
    let calls = w.calls_of("cloudformation describe-stacks");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].args,
        strings(&["--stack-name", "agentspace-host-beta"])
    );
    assert_eq!(calls[0].output, Output::Json);
}

/// Step 5: one describe, with the IDs in a filter, in the registry's order, `Name=instance-id,
/// Values=<id>,<id>,...` as one argument; the ID that came from the stack is in its place in the
/// registry's order and not last; `gamma`'s record, which holds none, names nothing.
#[test]
fn one_describe_names_every_id_in_the_registrys_order_in_a_filter() {
    let w = mixed("describe-filter");
    w.run_ok(now());
    let calls = w.calls_of("ec2 describe-instances");
    assert_eq!(calls.len(), 1, "One describe, however many hosts");
    let values = [id(0), id(1), ZEROS.to_string(), id(3), id(4)].join(",");
    assert_eq!(
        calls[0].args,
        vec![
            "--filters".to_string(),
            format!("Name=instance-id,Values={values}")
        ]
    );
    assert_eq!(calls[0].output, Output::Json);
}

/// Step 6: one `ec2 stop-instances --instance-ids <id>` for a due host, which names one instance;
/// no stop for the explained, the unexplained that is not yet due, the stopped, the pending and
/// the `creating` host.
#[test]
fn one_stop_for_the_due_host_and_none_for_the_others() {
    let w = mixed("one-stop");
    w.run_ok(now());
    let stops = w.calls_of("ec2 stop-instances");
    assert_eq!(stops.len(), 1);
    assert_eq!(stops[0].args, vec!["--instance-ids".to_string(), id(1)]);
    assert!(
        matches!(stops[0].output, Output::Checked | Output::Json),
        "A stop's exit code is read, its answer is not"
    );
}

/// Step 7: one `ssm send-command` per running host not stopped, `--instance-ids <id>
/// --document-name AWS-RunShellScript --comment "agentspace guard: lease horizon" --parameters
/// commands=<command>`, in the registry's order, and none for the host that was stopped, the
/// stopped one and the pending one. Nothing is polled.
#[test]
fn one_send_per_running_host_not_stopped_with_the_horizon_comment() {
    let w = mixed("sends");
    w.run_ok(now());
    let sends = w.calls_of("ssm send-command");
    assert_eq!(sends.len(), 2);
    for (send, host_id) in sends.iter().zip([id(0), ZEROS.to_string()]) {
        assert_eq!(send.args.len(), 8, "Eight arguments, one instance");
        assert_eq!(
            send.args[..7].to_vec(),
            vec![
                "--instance-ids".to_string(),
                host_id,
                "--document-name".to_string(),
                "AWS-RunShellScript".to_string(),
                "--comment".to_string(),
                "agentspace guard: lease horizon".to_string(),
                "--parameters".to_string(),
            ]
        );
        assert!(send.args[7].starts_with("commands="));
        assert!(matches!(send.output, Output::Checked | Output::Json));
    }
    assert!(
        !w.call_keys()
            .iter()
            .any(|k| k == "ssm get-command-invocation"),
        "The guard never polls an invocation"
    );
}

/// Step 7: the command holds the horizon's decimal digits as one word of its own and no comma,
/// bracket or equals sign, which the AWS CLI's shorthand would split on.
#[test]
fn the_command_holds_the_horizon_as_one_word_and_no_shorthand_separator() {
    let w = mixed("command-form");
    w.run_ok(now());
    for send in w.calls_of("ssm send-command") {
        let command = command_of(&send);
        let horizon = horizon_of(&send);
        assert!(horizon > 0);
        assert!(!command.contains(','), "A comma");
        assert!(!command.contains('['), "A bracket");
        assert!(!command.contains(']'), "A bracket");
        assert!(!command.contains('='), "An equals sign");
        assert!(!command.is_empty());
        assert!(
            !command.contains(&format!("{horizon}0")),
            "The digits are a word, not the start of a longer number"
        );
    }
}

/// Step 7: a horizon is sent to every running host at every run, whether or not it changed, so a
/// second run, a minute later, sends the same again.
#[test]
fn every_run_sends_every_running_host_its_horizon_again() {
    let w = mixed("send-again");
    w.run_ok(now());
    w.run_ok(now() + 60);
    assert_eq!(w.sent_ids().len(), 4, "Two sends for each of two runs");
    let horizons = w.horizons();
    assert_eq!(
        horizons[0].1, horizons[2].1,
        "An explained host's horizon is the same"
    );
}

// ---------------------------------------------------------------------------------------------
// What it sends (operator.md, guard, step 7; state.md, Live leases and the lease horizon).
// ---------------------------------------------------------------------------------------------

/// Step 7, P12: the later of the project's horizon and the host's `LaunchTime`, for each case of
/// the horizon: the latest `expires_at` of the live leases, the reading's time for a project whose
/// only live leases are `pending`, the reading's time kept when a live `ready` lease also
/// exists, the last lease end, and the host's launch time; and for an unexplained host the
/// horizon sent plus the grace is the stop time the log says.
#[test]
fn the_horizon_sent_is_the_later_of_the_projects_horizon_and_the_launch_time() {
    let w = World::new("horizons");
    w.write_hosts(&[
        box_host(Some(&id(0))),
        ready_host("pone", Some(&id(1))),
        ready_host("ptwo", Some(&id(2))),
        ready_host("pthree", Some(&id(3))),
        ready_host("pfour", Some(&id(4))),
        ready_host("pfive", Some(&id(5))),
    ]);
    w.write_leases(&[
        // The box: two live leases, the later `expires_at` wins; an expired and a released one
        // with a later `expires_at` add nothing.
        lease_ready("swimcoach", &today("22:00:00")),
        lease_ready("swimcoach", &today("23:30:00")),
        lease_ready("swimcoach", &today("18:00:00")),
        lease_released(
            "swimcoach",
            &today("19:00:00"),
            Some("2026-11-23T05:00:00Z"),
        ),
        // Only a pending lease: the reading's time.
        lease_pending("pone"),
        // A pending lease and a live ready one: the ready one's `expires_at`.
        lease_pending("ptwo"),
        lease_ready("ptwo", &today("21:00:00")),
        // No live lease: the last lease end, since it is later than the launch.
        lease_released("pthree", &today("19:50:00"), None),
        // No lease at all: the launch time.
        // A last lease end before the launch: the launch time.
        lease_released("pfive", &today("19:20:00"), None),
    ]);
    let launches = [
        (0, "08:00:00"),
        (1, "08:00:00"),
        (2, "08:00:00"),
        (3, "09:00:00"),
        (4, "19:50:10"),
        (5, "19:50:20"),
    ];
    for (n, at) in launches {
        w.aws.add_instance(&id(n), "running", Some(&launch(at)));
    }
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 6,
            running: 6,
            stopped: 0,
            unexplained: 3,
            failed: 0,
            log_dropped: false
        }
    );
    let expected = vec![
        (id(0), t(&today("23:30:00"))),
        (id(1), now()),
        (id(2), t(&today("21:00:00"))),
        (id(3), t(&today("19:50:00"))),
        (id(4), t(&today("19:50:10"))),
        (id(5), t(&today("19:50:20"))),
    ];
    assert_eq!(w.horizons(), expected);
    // The log's stop times are the horizons sent plus the grace, for the three unexplained.
    let rests = w.rests();
    assert_eq!(rests.len(), 3);
    for (rest, (project, (_, horizon))) in rests.iter().zip([
        ("pthree", &expected[3]),
        ("pfour", &expected[4]),
        ("pfive", &expected[5]),
    ]) {
        let stops = format!("stops={}", z(horizon + GRACE_SECONDS));
        assert!(
            rest.starts_with(&format!("host={project} unexplained since="))
                && rest.ends_with(&stops),
            "The stop time is the horizon plus the grace: {rest}"
        );
    }
}

/// The grace is 15 minutes, a constant.
#[test]
fn the_grace_is_fifteen_minutes() {
    assert_eq!(GRACE_SECONDS, 900);
}

/// Step 6, the log's form: a `LaunchTime` with fractions of a second is truncated to the whole
/// second before it is written or compared, one with an offset is read in UTC, and the `Z` form
/// is read too; the since is the launch time when the project has no lease, and the horizon sent
/// to such a host is that truncated since.
#[test]
fn a_launch_time_is_read_in_utc_and_truncated_to_the_second() {
    let w = World::new("launch-forms");
    w.write_hosts(&[
        box_host(Some(&id(0))),
        ready_host("alpha", Some(&id(1))),
        ready_host("beta", Some(&id(2))),
    ]);
    w.write_leases(&[]);
    w.aws
        .add_instance(&id(0), "running", Some("2026-11-22T19:30:45.987654+00:00"));
    w.aws
        .add_instance(&id(1), "running", Some("2026-11-22T11:31:45-08:00"));
    w.aws
        .add_instance(&id(2), "running", Some("2026-11-22T19:32:45Z"));
    let at = t(&today("19:40:00"));
    let report = w.run_ok(at);
    assert_eq!(report.unexplained, 3);
    assert_eq!(
        w.rests(),
        strings(&[
            "host=swimcoach unexplained since=2026-11-22T19:30:45Z stops=2026-11-22T19:45:45Z",
            "host=alpha unexplained since=2026-11-22T19:31:45Z stops=2026-11-22T19:46:45Z",
            "host=beta unexplained since=2026-11-22T19:32:45Z stops=2026-11-22T19:47:45Z",
        ])
    );
    assert_eq!(
        w.horizons(),
        vec![
            (id(0), t("2026-11-22T19:30:45Z")),
            (id(1), t("2026-11-22T19:31:45Z")),
            (id(2), t("2026-11-22T19:32:45Z")),
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// guard.log, byte for byte (operator.md, Its log; state.md, The guard's log).
// ---------------------------------------------------------------------------------------------

/// Its log: one line for a host stopped and one for a host found unexplained, in the registry's
/// order, and nothing for the explained, the stopped, the pending and the skipped host, nor for a
/// horizon sent; `<since>` is the later of the launch and the project's last lease end, and the
/// lines carry no `provisioning=` field when no provisioning run holds the lock.
#[test]
fn the_log_holds_a_stopped_line_and_an_unexplained_line_and_nothing_else() {
    let w = mixed("log-lines");
    w.run_ok(now());
    assert_eq!(w.rests(), vec![alpha_stopped(), beta_unexplained()]);
}

/// Its log: a line starts with the time it is written, read from the clock for that line and not
/// the run's time, as `YYYY-MM-DDTHH:MM:SSZ` (state.md, The guard's log).
#[test]
fn a_lines_time_is_the_clock_and_not_the_runs_time() {
    let w = mixed("log-clock");
    let before = std::time::SystemTime::now();
    w.run_ok(now());
    let after = std::time::SystemTime::now();
    let secs = |s: std::time::SystemTime| {
        s.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
    };
    let log = w.log();
    assert_eq!(log.len(), 2);
    for (time, _) in log {
        let at = t(&time);
        assert!(
            at >= secs(before) - 1 && at <= secs(after) + 1,
            "A line's time is the time it is written"
        );
    }
}

/// state.md, The guard's log: it is created when the first line is written, with the folder made
/// first, and a run with nothing to log creates nothing: a run that finds every host explained.
#[test]
fn a_run_with_nothing_to_log_creates_no_log() {
    let w = World::new("no-log");
    w.write_hosts(&[box_host(Some(&id(0)))]);
    w.write_leases(&[lease_ready("swimcoach", &today("23:00:00"))]);
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    let report = w.run_ok(now());
    assert_eq!(report.unexplained, 0);
    assert!(!w.log_exists());
    assert_eq!(
        w.sent_ids(),
        vec![id(0)],
        "The horizon is still sent, and not logged"
    );
}

/// Its log: nothing for a host that is not `running`, whatever its leases, and no stop and no
/// send: `pending`, `stopping`, `stopped`, `shutting-down` and `terminated`.
#[test]
fn a_host_that_is_not_running_gets_nothing() {
    let w = World::new("not-running");
    w.write_hosts(&[
        box_host(Some(&id(0))),
        ready_host("pone", Some(&id(1))),
        ready_host("ptwo", Some(&id(2))),
        ready_host("pthree", Some(&id(3))),
        ready_host("pfour", Some(&id(4))),
    ]);
    w.write_leases(&[]);
    for (n, state) in [
        "pending",
        "stopping",
        "stopped",
        "shutting-down",
        "terminated",
    ]
    .into_iter()
    .enumerate()
    {
        w.aws
            .add_instance(&id(n as u32), state, Some(&launch("07:00:00")));
    }
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 5,
            running: 0,
            stopped: 0,
            unexplained: 0,
            failed: 0,
            log_dropped: false
        }
    );
    assert!(w.stopped_ids().is_empty());
    assert!(w.sent_ids().is_empty());
    assert!(!w.log_exists());
    assert_eq!(
        w.progress(),
        strings(&["Hosts: 5; running 0, stopped 0, unexplained 0."])
    );
}

/// Its log, step 6: a log line is appended to a log that exists, never replacing what it holds.
#[test]
fn a_line_is_appended_to_a_log_that_exists() {
    let w = mixed("log-append");
    std::fs::create_dir_all(w.agentspace()).unwrap();
    std::fs::write(w.guard_log(), b"An earlier line\n").unwrap();
    w.run_ok(now());
    let bytes = std::fs::read(w.guard_log()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "An earlier line");
    assert!(lines[1].ends_with(&alpha_stopped()));
}

/// Its log: a reason has each control character written as `?`, so a line holds no CR and no LF
/// of a message.
#[test]
fn a_control_character_in_a_reason_is_written_as_a_question_mark() {
    let w = mixed("log-control");
    w.aws.fail_stop(
        &id(1),
        AwsError::Failed("one\ntwo\tthree\rfour\u{1}".into()),
    );
    w.run_ok(now());
    let expected = format!(
        "host=alpha failed step=stop reason=Could not stop {}: one?two?three?four?",
        id(1)
    );
    assert_eq!(w.rests(), vec![expected, beta_unexplained()]);
}

/// Its log: a run's lines come in the order of its steps: a host's `instance` failure of step 4,
/// then each host's step 6 line in the registry's order, then each `horizon` failure of step 7.
#[test]
fn the_lines_come_in_the_order_of_the_steps() {
    let w = mixed("log-order");
    w.aws.set_stack(
        &stack_of("beta"),
        Err(AwsError::Failed(
            "Stack with id agentspace-host-beta does not exist".into(),
        )),
    );
    w.aws
        .fail_send(&id(0), AwsError::Failed("Target is not connected".into()));
    let report = w.run_ok(now());
    assert_eq!(report.failed, 2);
    assert_eq!(
        w.rests(),
        vec![
            "host=beta failed step=instance reason=Stack with id agentspace-host-beta does not exist"
                .to_string(),
            alpha_stopped(),
            format!(
                "host=swimcoach failed step=horizon reason=Could not send the lease horizon to {}: Target is not connected",
                id(0)
            ),
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// What it says, and the report (operator.md, guard, steps 6 and 8; Its streams).
// ---------------------------------------------------------------------------------------------

/// Step 6 and step 8: the three progress lines, in the registry's order and then the closing
/// line, all through `Say::progress` with no caller form; the stop time is `<h:mm><am|pm>`, in
/// the laptop's zone, so it is matched by its form.
#[test]
fn it_says_each_stop_each_unexplained_host_and_the_closing_line() {
    let w = mixed("says");
    w.run_ok(now());
    let lines = w.progress();
    assert_eq!(lines.len(), 3, "Stopping, unexplained and the closing line");
    assert_eq!(
        lines[0],
        format!(
            "Stopping the host of 'alpha' ({}): no live lease explains it.",
            id(1)
        )
    );
    assert!(
        is_unexplained_progress(&lines[1], "beta", ZEROS),
        "The unexplained line: {}",
        lines[1]
    );
    assert_eq!(lines[2], "Hosts: 6; running 3, stopped 1, unexplained 1.");
    assert!(
        w.events()
            .iter()
            .all(|e| matches!(e, Event::Progress { .. })),
        "Nothing else is said by a run that failed nothing"
    );
}

/// Step 8: the report's counts: `<m>` the registry's records, `<r>` the hosts read as `running`,
/// `<s>` the hosts this run stopped, `<u>` the hosts logged `unexplained`.
#[test]
fn the_report_counts_the_hosts() {
    let w = mixed("report");
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 6,
            running: 3,
            stopped: 1,
            unexplained: 1,
            failed: 0,
            log_dropped: false
        }
    );
}

/// Step 6: a host the project has a live lease for is explained, whatever it is doing: nothing
/// logged, no stop, and the run says only its closing line.
#[test]
fn an_explained_host_is_neither_logged_nor_stopped_nor_mentioned() {
    let w = World::new("explained");
    w.write_hosts(&[box_host(Some(&id(0)))]);
    w.write_leases(&[lease_ready("swimcoach", &today("23:00:00"))]);
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 1,
            running: 1,
            stopped: 0,
            unexplained: 0,
            failed: 0,
            log_dropped: false
        }
    );
    assert_eq!(
        w.progress(),
        strings(&["Hosts: 1; running 1, stopped 0, unexplained 0."])
    );
    assert!(w.stopped_ids().is_empty());
}

/// Step 4: no ID found for any host, no describe call; the record that holds none and is not
/// `ready` is skipped with no call and no line.
#[test]
fn no_id_found_means_no_describe() {
    let w = World::new("no-ids");
    w.write_hosts(&[host("swimcoach", "agentbox", "failed", None)]);
    w.write_leases(&[]);
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 1,
            running: 0,
            stopped: 0,
            unexplained: 0,
            failed: 0,
            log_dropped: false
        }
    );
    assert!(w.calls().is_empty());
    assert!(!w.log_exists());
    assert_eq!(
        w.progress(),
        strings(&["Hosts: 1; running 0, stopped 0, unexplained 0."])
    );
}

/// Step 4: a record that holds an ID has it named in the describe and is decided, whatever its
/// state: a `creating` record with an ID, no stack read for it.
#[test]
fn a_creating_record_that_holds_an_id_is_decided_like_any() {
    let w = World::new("creating-with-id");
    w.write_hosts(&[
        box_host(Some(&id(0))),
        host("gamma", &stack_of("gamma"), "creating", Some(&id(2))),
    ]);
    w.write_leases(&[lease_ready("swimcoach", &today("23:00:00"))]);
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(&id(2), "running", Some(&launch("19:50:00")));
    let report = w.run_ok(now());
    assert!(
        !w.call_keys()
            .contains(&"cloudformation describe-stacks".to_string())
    );
    let describe = &w.calls_of("ec2 describe-instances")[0];
    assert_eq!(
        describe.args[1],
        format!("Name=instance-id,Values={},{}", id(0), id(2))
    );
    assert_eq!(report.unexplained, 1);
    assert_eq!(
        w.rests(),
        vec![format!(
            "host=gamma unexplained since={} stops={}",
            today("19:50:00"),
            today("20:05:00")
        )]
    );
}

/// Step 5: an element of the answer is the one whose `InstanceId` equals the host's, wherever it
/// is: in any reservation, in any order, beside an element with no string `InstanceId`, which is
/// ignored.
#[test]
fn an_element_is_found_by_its_instance_id_and_one_with_none_is_ignored() {
    let w = mixed("find-by-id");
    w.aws
        .add_raw_instance(json!({"State": {"Name": "running"}, "LaunchTime": launch("01:00:00")}));
    w.aws
        .add_raw_instance(json!({"InstanceId": 7, "State": {"Name": "running"}}));
    let report = w.run_ok(now());
    assert_eq!(report.failed, 0);
    assert_eq!(w.rests(), vec![alpha_stopped(), beta_unexplained()]);
}

// ---------------------------------------------------------------------------------------------
// The calls' times and the stop time (operator.md, guard, step 6).
// ---------------------------------------------------------------------------------------------

fn alpha_ending_at(name: &str, end: &str) -> World {
    let w = World::new(name);
    w.write_hosts(&[box_host(Some(&id(0))), ready_host("alpha", Some(&id(1)))]);
    w.write_leases(&[
        lease_ready("swimcoach", &today("23:00:00")),
        lease_released("alpha", end, None),
    ]);
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(&id(1), "running", Some(&launch("08:00:00")));
    w
}

/// Step 6: at the stop time, the run's time equal to it, the host is due and stopped.
#[test]
fn a_host_is_due_at_its_stop_time() {
    let w = alpha_ending_at("due-at", &today("19:45:00"));
    let report = w.run_ok(now());
    assert_eq!(report.stopped, 1);
    assert_eq!(w.stopped_ids(), vec![id(1)]);
    assert_eq!(
        w.rests(),
        vec![format!("host=alpha stopped since={}", today("19:45:00"))]
    );
}

/// Step 6: one second before the stop time the host is logged unexplained with the stop time, said
/// with the stop time, and not stopped; its provisioning lock is not tried then.
#[test]
fn a_host_is_not_due_a_second_before_its_stop_time() {
    let w = alpha_ending_at("due-before", &today("19:45:00"));
    let report = w.run_ok(now() - 1);
    assert_eq!(report.stopped, 0);
    assert_eq!(report.unexplained, 1);
    assert!(w.stopped_ids().is_empty());
    assert_eq!(
        w.rests(),
        vec![format!(
            "host=alpha unexplained since={} stops={}",
            today("19:45:00"),
            today("20:00:00")
        )]
    );
    assert!(is_unexplained_progress(&w.progress()[0], "alpha", &id(1)));
}

/// Step 6, the since: the later of the launch and the project's last lease end, so a host
/// launched after its lease ended has its launch as its since; and a project with no lease has
/// its launch alone.
#[test]
fn the_since_is_the_later_of_the_launch_and_the_last_lease_end() {
    let w = World::new("since");
    w.write_hosts(&[
        box_host(Some(&id(0))),
        ready_host("alpha", Some(&id(1))),
        ready_host("beta", Some(&id(2))),
    ]);
    w.write_leases(&[
        lease_released("swimcoach", &today("19:00:00"), Some(&today("23:00:00"))),
        lease_released("alpha", &today("19:20:00"), None),
    ]);
    // The box: the end 19:00, the launch 19:30; alpha: the end 19:20, the launch 19:10; beta: none, the launch 19:33.
    w.aws
        .add_instance(&id(0), "running", Some(&launch("19:30:00")));
    w.aws
        .add_instance(&id(1), "running", Some(&launch("19:10:00")));
    w.aws
        .add_instance(&id(2), "running", Some(&launch("19:33:00")));
    w.run_ok(t(&today("19:34:00")));
    assert_eq!(
        w.rests(),
        strings(&[
            "host=swimcoach unexplained since=2026-11-22T19:30:00Z stops=2026-11-22T19:45:00Z",
            "host=alpha unexplained since=2026-11-22T19:20:00Z stops=2026-11-22T19:35:00Z",
            "host=beta unexplained since=2026-11-22T19:33:00Z stops=2026-11-22T19:48:00Z",
        ])
    );
}

/// state.md, Live leases: a `ready` lease is live until its `expires_at`, and at or after it the
/// lease is `expired`, ending then: a lease that expires at the run's time explains nothing, and
/// one that expires a second later does; a `pending` lease is live.
#[test]
fn a_lease_is_live_until_its_expiry_and_a_pending_lease_is_live() {
    let w = World::new("liveness");
    w.write_hosts(&[
        box_host(Some(&id(0))),
        ready_host("alpha", Some(&id(1))),
        ready_host("beta", Some(&id(2))),
    ]);
    w.write_leases(&[
        lease_ready("swimcoach", &z(now())),
        lease_ready("alpha", &z(now() + 1)),
        lease_pending("beta"),
    ]);
    for n in 0..3 {
        w.aws
            .add_instance(&id(n), "running", Some(&launch("08:00:00")));
    }
    let report = w.run_ok(now());
    assert_eq!(report.unexplained, 1);
    assert_eq!(
        w.rests(),
        vec![format!(
            "host=swimcoach unexplained since={} stops={}",
            today("20:00:00"),
            today("20:15:00")
        )]
    );
    assert!(w.stopped_ids().is_empty());
}

/// state.md, A project's leases: a lease of another project explains no host, a lease of a
/// project with no host record explains none and not the box, and a released or an unsatisfiable
/// lease is not live: so the box is guarded like any host, and stopped.
#[test]
fn only_a_live_lease_of_the_hosts_own_project_explains_it() {
    let w = World::new("scoping");
    w.write_hosts(&[box_host(Some(&id(0))), ready_host("alpha", Some(&id(1)))]);
    w.write_leases(&[
        lease_ready("nohost", &today("23:00:00")),
        lease_ready("alphb", &today("23:00:00")),
        lease_unsatisfiable("alpha"),
        lease_released("alpha", &today("19:00:00"), None),
    ]);
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    w.aws
        .add_instance(&id(1), "running", Some(&launch("08:00:00")));
    let report = w.run_ok(now());
    assert_eq!(report.stopped, 2);
    assert_eq!(w.stopped_ids(), vec![id(0), id(1)]);
    assert_eq!(
        w.rests(),
        vec![
            format!("host=swimcoach stopped since={}", today("08:00:00")),
            format!("host=alpha stopped since={}", today("19:00:00")),
        ]
    );
    assert!(w.sent_ids().is_empty());
}

/// operator.md, What the guard can race: the guard sees the leases of the state folder it is
/// given, not `%USERPROFILE%\.agentspace`'s: a live lease there explains the host, and a file in
/// the home with no lease explains nothing.
#[test]
fn the_leases_are_read_from_the_state_folder_it_is_given() {
    let mut w = World::new("state-dir");
    w.write_hosts(&[box_host(Some(&id(0)))]);
    // The home's own leases.json holds none.
    w.write_leases(&[]);
    w.state_dir = w.root.join("elsewhere");
    w.write_leases(&[lease_ready("swimcoach", &today("23:00:00"))]);
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    let report = w.run_ok(now());
    assert_eq!(report.stopped, 0);
    assert!(w.stopped_ids().is_empty());
    assert!(!w.log_exists());
}

// ---------------------------------------------------------------------------------------------
// guard.lock (operator.md, guard, step 1; state.md, The guard's lock).
// ---------------------------------------------------------------------------------------------

/// Step 1: a `guard.lock` held by another handle ends the run with success and a report that says
/// so, before anything else: no call, nothing logged, nothing said.
#[test]
fn a_held_guard_lock_ends_the_run_at_once_and_quietly() {
    let w = mixed("lock-held");
    std::fs::create_dir_all(w.agentspace()).unwrap();
    let _lock = hold(&w.guard_lock());
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: true,
            ..Report::default()
        }
    );
    assert!(w.calls().is_empty());
    assert!(!w.log_exists());
    assert!(w.events().is_empty(), "Nothing is printed");
}

/// Step 1: the lock is taken before any file is read, so a held lock hides a registry that is
/// not valid and a lease state file that is missing.
#[test]
fn a_held_guard_lock_comes_before_every_read() {
    let w = World::new("lock-first");
    std::fs::create_dir_all(w.agentspace()).unwrap();
    std::fs::write(w.hosts_path(), b"This is not JSON").unwrap();
    let _lock = hold(&w.guard_lock());
    let report = w.run_ok(now());
    assert!(report.lock_held);
    assert!(w.calls().is_empty());
    assert!(!w.log_exists());
    assert!(w.events().is_empty());
}

/// Step 1: the lock is made if it is missing, with `%USERPROFILE%\.agentspace` made first, empty,
/// never deleted, and held until the run exits: free after it. The run here fails at the lease
/// state file, which is missing, after taking the lock.
#[test]
fn the_guard_lock_is_made_empty_kept_and_released() {
    let w = World::new("lock-made");
    assert!(!w.agentspace().exists());
    let _ = w.run(now());
    assert!(w.guard_lock().is_file(), "The lock file is made");
    assert_eq!(std::fs::metadata(w.guard_lock()).unwrap().len(), 0);
    let again = try_hold(&w.guard_lock());
    assert!(again.is_ok(), "The lock is free after the run");
}

/// Step 1, state.md: it is held for the whole run, so every call is made with it held, and a
/// provisioning lock tried for a due host is free again by the time of the stop. The probe
/// tries each file with no sharing at every call.
#[test]
fn the_guard_lock_is_held_at_every_call_and_the_provisioning_lock_is_not() {
    let w = mixed("lock-during");
    // The provision lock exists, free.
    std::fs::create_dir_all(w.provision_lock("alpha").parent().unwrap()).unwrap();
    drop(try_hold(&w.provision_lock("alpha")).unwrap());
    w.aws
        .probe_locks(vec![w.guard_lock(), w.provision_lock("alpha")]);
    w.run_ok(now());
    let probed = w.aws.probed();
    assert_eq!(probed.len(), 5);
    for (key, _, held) in &probed {
        assert!(held[0], "guard.lock is held at {key}");
        assert!(
            !held[1],
            "alpha's provision.lock is not held by the guard at {key}"
        );
    }
}

/// Step 1: a lock that cannot be taken for another reason than being held, a folder at its path,
/// fails the run with `agentspace: Cannot take <path>: <error>.`, logs it with `step=lock` and no
/// `host=` field, reads nothing and calls nothing.
#[test]
fn a_guard_lock_that_cannot_be_taken_fails_the_run() {
    let w = mixed("lock-folder");
    std::fs::create_dir_all(w.guard_lock()).unwrap();
    let failure = w.run_err(now());
    let prefix = format!("agentspace: Cannot take {}: ", w.guard_lock().display());
    w.expect(
        failure.message.starts_with(&prefix) && failure.message.ends_with('.'),
        "The failure's message",
        &failure.message,
    );
    let rests = w.rests();
    assert_eq!(rests.len(), 1);
    let reason = &failure.message["agentspace: ".len()..];
    w.expect_eq(
        &rests[0],
        &format!("failed step=lock reason={reason}"),
        "The log's line",
    );
    assert!(w.calls().is_empty());
}

// ---------------------------------------------------------------------------------------------
// The files it reads (operator.md, guard, steps 2 and 3).
// ---------------------------------------------------------------------------------------------

/// Step 2: a missing host registry is the box's record alone: `ready`, no ID, so the box's stack
/// `agentbox` is read, and the file is never written.
#[test]
fn a_missing_registry_is_the_boxs_record_alone() {
    let w = World::new("hosts-missing");
    w.write_leases(&[lease_ready("swimcoach", &today("23:00:00"))]);
    w.aws.set_stack_id("agentbox", &id(0));
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    let report = w.run_ok(now());
    assert_eq!(report.hosts, 1);
    assert_eq!(report.running, 1);
    let stacks = w.calls_of("cloudformation describe-stacks");
    assert_eq!(stacks.len(), 1);
    assert_eq!(stacks[0].args, strings(&["--stack-name", "agentbox"]));
    assert_eq!(w.sent_ids(), vec![id(0)]);
    assert!(
        !w.hosts_path().exists(),
        "The guard does not write the registry"
    );
}

/// Step 2: a registry that cannot be read or is not valid fails the run with `agentspace: The
/// host registry <path> cannot be read: <reason>.`, logged with `step=hosts` and no `host=`, and
/// no call is made.
#[test]
fn a_registry_that_is_not_valid_fails_the_run() {
    let w = World::new("hosts-invalid");
    w.write_leases(&[]);
    std::fs::create_dir_all(w.agentspace()).unwrap();
    std::fs::write(w.hosts_path(), b"This is not JSON").unwrap();
    let failure = w.run_err(now());
    let prefix = format!(
        "agentspace: The host registry {} cannot be read: ",
        w.hosts_path().display()
    );
    w.expect(
        failure.message.starts_with(&prefix) && failure.message.ends_with('.'),
        "The failure's message",
        &failure.message,
    );
    let rests = w.rests();
    assert_eq!(rests.len(), 1);
    let reason = &failure.message["agentspace: ".len()..];
    w.expect_eq(
        &rests[0],
        &format!("failed step=hosts reason={reason}"),
        "The log's line",
    );
    assert!(w.calls().is_empty());
}

/// Step 3, P18: a missing lease state file fails the run with `agentspace: The lease state file
/// <path> cannot be read: the file does not exist.`, one `failed step=leases` line with no
/// `host=` field, no call at all, nothing stopped, and the path is the state folder's, not the
/// home's.
#[test]
fn a_missing_lease_file_fails_the_run_and_stops_nothing() {
    let mut w = mixed("leases-missing");
    w.state_dir = w.root.join("elsewhere");
    std::fs::create_dir_all(&w.state_dir).unwrap();
    let failure = w.run_err(now());
    let message = format!(
        "agentspace: The lease state file {} cannot be read: the file does not exist.",
        w.leases_path().display()
    );
    w.expect_eq(&failure.message, &message, "The failure's message");
    let rests = w.rests();
    assert_eq!(rests.len(), 1);
    w.expect_eq(
        &rests[0],
        &format!(
            "failed step=leases reason={}",
            &message["agentspace: ".len()..]
        ),
        "The log's line",
    );
    assert!(!rests[0].contains("host="), "The line has no host field");
    assert!(w.calls().is_empty(), "Not even a stack read");
    assert!(w.progress().iter().all(|l| !l.starts_with("Hosts:")));
}

/// Step 3: a lease state file that cannot be read as valid fails the run the same way, with
/// nothing stopped and no horizon sent: bad JSON, a wrong version, two records with one `id`.
#[test]
fn a_lease_file_that_is_not_valid_fails_the_run_and_stops_nothing() {
    let one = lease_ready("swimcoach", &today("23:00:00"));
    let duplicate = json!({"version": 0, "leases": [one.clone(), one]}).to_string();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("bad-json", b"{\"version\":0,\"leases\":[".to_vec()),
        ("wrong-version", br#"{"version":1,"leases":[]}"#.to_vec()),
        ("duplicate-id", duplicate.into_bytes()),
        ("empty", Vec::new()),
    ];
    for (name, bytes) in cases {
        let w = mixed(&format!("leases-{name}"));
        w.write_leases_raw(&bytes);
        let failure = w.run_err(now());
        let prefix = format!(
            "agentspace: The lease state file {} cannot be read: ",
            w.leases_path().display()
        );
        w.expect(
            failure.message.starts_with(&prefix) && failure.message.ends_with('.'),
            &format!("The failure's message, {name}"),
            &failure.message,
        );
        let rests = w.rests();
        assert_eq!(rests.len(), 1, "{name}");
        assert!(
            rests[0].starts_with("failed step=leases reason=The lease state file "),
            "{name}"
        );
        assert!(w.calls().is_empty(), "{name}: no call");
    }
}

// ---------------------------------------------------------------------------------------------
// The provisioning lock (operator.md, guard, step 6; state.md, The provisioning run's lock).
// ---------------------------------------------------------------------------------------------

/// Step 6: a due host whose `provision.lock` is held is left: logged `unexplained` with its stop
/// time and `provisioning=running`, said with the provisioning line, not stopped, and still sent
/// its horizon; it counts as unexplained and not as stopped.
#[test]
fn a_host_whose_provisioning_run_holds_its_lock_is_left_and_sent_its_horizon() {
    let w = mixed("provision-held");
    let _lock = hold(&w.provision_lock("alpha"));
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 6,
            running: 3,
            stopped: 0,
            unexplained: 2,
            failed: 0,
            log_dropped: false
        }
    );
    assert!(w.stopped_ids().is_empty());
    assert_eq!(w.sent_ids(), vec![id(0), id(1), ZEROS.to_string()]);
    assert_eq!(
        w.rests(),
        vec![
            format!(
                "host=alpha unexplained since={} stops={} provisioning=running",
                today("19:30:00"),
                today("19:45:00")
            ),
            beta_unexplained(),
        ]
    );
    assert_eq!(
        w.progress()[0],
        format!(
            "The host of 'alpha' ({}) has no live lease, but a provisioning run is working on it; it is not stopped.",
            id(1)
        )
    );
}

/// Step 6: the lock is tried only for a host that is due: a host not yet due whose lock is held
/// is logged plainly, with no `provisioning=` field.
#[test]
fn the_provisioning_lock_is_tried_only_for_a_due_host() {
    let w = mixed("provision-held-not-due");
    let _lock = hold(&w.provision_lock("beta"));
    w.run_ok(now());
    assert_eq!(w.rests(), vec![alpha_stopped(), beta_unexplained()]);
}

/// Step 6: the guard never creates the host's folder nor the lock file to try it: with no
/// `hosts` folder, the host is stopped and no `hosts` folder exists afterwards; with the host's
/// folder and no lock, no lock file is made.
#[test]
fn the_guard_never_creates_the_hosts_folder_or_the_lock_file_to_try_it() {
    let w = mixed("provision-missing");
    w.run_ok(now());
    assert_eq!(w.stopped_ids(), vec![id(1)]);
    assert!(!w.agentspace().join("hosts").exists());

    let w = mixed("provision-no-file");
    std::fs::create_dir_all(w.provision_lock("alpha").parent().unwrap()).unwrap();
    w.run_ok(now());
    assert_eq!(w.stopped_ids(), vec![id(1)]);
    assert!(!w.provision_lock("alpha").exists());
}

/// Step 6: a lock that exists and is free is a free lock: the host is stopped, the file is not
/// deleted, and the guard's try left it free.
#[test]
fn a_free_provisioning_lock_lets_the_host_stop_and_is_left_in_place() {
    let w = mixed("provision-free");
    std::fs::create_dir_all(w.provision_lock("alpha").parent().unwrap()).unwrap();
    drop(try_hold(&w.provision_lock("alpha")).unwrap());
    w.run_ok(now());
    assert_eq!(w.stopped_ids(), vec![id(1)]);
    assert!(w.provision_lock("alpha").is_file());
    assert!(try_hold(&w.provision_lock("alpha")).is_ok());
}

/// Step 6 and What it MUST NOT do: a lock that cannot be tried for any reason but its being held,
/// a folder at its path, is not a free lock: the host's part fails with `Cannot try <path>:
/// <error>.`, logged with `step=lock`, the host is not stopped, and it is sent its horizon.
#[test]
fn a_provisioning_lock_that_cannot_be_tried_fails_the_hosts_part() {
    let w = mixed("provision-folder");
    std::fs::create_dir_all(w.provision_lock("alpha")).unwrap();
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 6,
            running: 3,
            stopped: 0,
            unexplained: 1,
            failed: 1,
            log_dropped: false
        }
    );
    assert!(w.stopped_ids().is_empty());
    assert_eq!(w.sent_ids(), vec![id(0), id(1), ZEROS.to_string()]);
    let rests = w.rests();
    assert_eq!(rests.len(), 2);
    let prefix = format!(
        "host=alpha failed step=lock reason=Cannot try {}: ",
        w.provision_lock("alpha").display()
    );
    w.expect(
        rests[0].starts_with(&prefix) && rests[0].ends_with('.'),
        "The log's line",
        &rests[0],
    );
    assert_eq!(rests[1], beta_unexplained());
}

// ---------------------------------------------------------------------------------------------
// A host's part that fails (operator.md, guard, steps 4 to 7; A host's part that fails).
// ---------------------------------------------------------------------------------------------

/// Step 6: a stop that fails fails the host's part with `Could not stop <id>: <error>`, logged
/// with `step=stop`, the host is not counted stopped and is sent its horizon, another host's stop
/// is still made and logged, and `Report::failed` counts the host; the message is said.
#[test]
fn a_failed_stop_fails_the_hosts_part_and_the_run_goes_on() {
    let w = two_due("stop-fails");
    w.aws
        .fail_stop(&id(1), AwsError::Failed("Not authorized".into()));
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 3,
            running: 3,
            stopped: 1,
            unexplained: 0,
            failed: 1,
            log_dropped: false
        }
    );
    assert_eq!(w.stopped_ids(), vec![id(1), id(5)]);
    assert_eq!(
        w.sent_ids(),
        vec![id(0), id(1)],
        "The host still running is sent its horizon"
    );
    assert_eq!(
        w.rests(),
        vec![
            format!(
                "host=alpha failed step=stop reason=Could not stop {}: Not authorized",
                id(1)
            ),
            format!("host=zeta stopped since={}", today("19:00:00")),
        ]
    );
    w.expect_host_failure_warned(
        "alpha",
        &format!("Could not stop {}: Not authorized", id(1)),
    );
}

/// Step 7: a send that fails fails the host's part with `Could not send the lease horizon to
/// <id>: <error>`, logged with `step=horizon` after the step 6 lines, and the next host is still
/// sent its horizon.
#[test]
fn a_failed_send_fails_the_hosts_part_and_the_next_host_is_still_sent() {
    let w = mixed("send-fails");
    w.aws
        .fail_send(&id(0), AwsError::Failed("Target is not connected".into()));
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 6,
            running: 3,
            stopped: 1,
            unexplained: 1,
            failed: 1,
            log_dropped: false
        }
    );
    assert_eq!(w.sent_ids(), vec![id(0), ZEROS.to_string()]);
    assert_eq!(
        w.rests(),
        vec![
            alpha_stopped(),
            beta_unexplained(),
            format!(
                "host=swimcoach failed step=horizon reason=Could not send the lease horizon to {}: Target is not connected",
                id(0)
            ),
        ]
    );
    w.expect_host_failure_warned(
        "swimcoach",
        &format!(
            "Could not send the lease horizon to {}: Target is not connected",
            id(0)
        ),
    );
}

/// Step 5: a host whose instance is not in the answer fails its part with `Instance <id> was not
/// found.`, logged with `step=instance` at its place in step 6's order, and the other hosts are
/// still guarded; it is neither stopped nor sent a horizon.
#[test]
fn a_host_whose_instance_is_not_in_the_answer_fails_its_part() {
    let w = mixed("not-found");
    w.aws.remove_instance(&id(3));
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 6,
            running: 3,
            stopped: 1,
            unexplained: 1,
            failed: 1,
            log_dropped: false
        }
    );
    assert_eq!(
        w.rests(),
        vec![
            alpha_stopped(),
            beta_unexplained(),
            format!(
                "host=delta failed step=instance reason=Instance {} was not found.",
                id(3)
            ),
        ]
    );
    assert_eq!(w.stopped_ids(), vec![id(1)]);
    assert_eq!(w.sent_ids(), vec![id(0), ZEROS.to_string()]);
    w.expect_host_failure_warned("delta", &format!("Instance {} was not found.", id(3)));
}

/// Step 5: an answer with no instance at all, which is what EC2 gives a filter that matches
/// nothing, fails every host's part and stops nothing; the run itself goes to its end.
#[test]
fn an_empty_answer_fails_every_hosts_part() {
    let w = mixed("empty-answer");
    w.aws.answer_describe_with(Ok(json!({"Reservations": []})));
    let report = w.run_ok(now());
    assert_eq!(report.hosts, 6);
    assert_eq!(report.failed, 5);
    assert_eq!(report.running, 0);
    assert!(w.stopped_ids().is_empty());
    assert!(w.sent_ids().is_empty());
    assert_eq!(
        w.rests(),
        vec![
            format!(
                "host=swimcoach failed step=instance reason=Instance {} was not found.",
                id(0)
            ),
            format!(
                "host=alpha failed step=instance reason=Instance {} was not found.",
                id(1)
            ),
            format!("host=beta failed step=instance reason=Instance {ZEROS} was not found."),
            format!(
                "host=delta failed step=instance reason=Instance {} was not found.",
                id(3)
            ),
            format!(
                "host=epsilon failed step=instance reason=Instance {} was not found.",
                id(4)
            ),
        ]
    );
}

/// Step 6: a `running` instance whose `LaunchTime` is missing or not of its form fails the host's
/// part with `Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].
/// LaunchTime.`, logged with `step=instance`, whether or not a lease explains the host: the host
/// is neither decided, stopped nor sent a horizon, and every other host is still guarded.
#[test]
fn a_running_instance_with_no_usable_launch_time_fails_its_hosts_part() {
    let cases: [(&str, Option<&str>); 3] = [
        ("missing", None),
        ("garbage", Some("yesterday")),
        ("no-offset", Some("2026-11-22T08:00:00")),
    ];
    for (name, bad) in cases {
        let w = mixed(&format!("launch-{name}"));
        // The box is explained, and alpha is due: both fail the same way.
        w.aws.set_launch_time(&id(0), bad);
        w.aws.set_launch_time(&id(1), bad);
        let report = w.run_ok(now());
        assert_eq!(report.hosts, 6, "{name}");
        assert_eq!(report.failed, 2, "{name}");
        assert_eq!(report.stopped, 0, "{name}");
        let reason = "Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].LaunchTime.";
        assert_eq!(
            w.rests(),
            vec![
                format!("host=swimcoach failed step=instance reason={reason}"),
                format!("host=alpha failed step=instance reason={reason}"),
                beta_unexplained(),
            ],
            "{name}"
        );
        assert!(w.stopped_ids().is_empty(), "{name}");
        assert_eq!(w.sent_ids(), vec![ZEROS.to_string()], "{name}");
        w.expect_host_failure_warned("swimcoach", reason);
        w.expect_host_failure_warned("alpha", reason);
    }
}

/// Step 4: a stack read that fails fails that host's part, logged with `step=instance`, the
/// failure's text as its reason; the host's ID is not in the describe, and the run goes on to
/// guard the others.
#[test]
fn a_failed_stack_read_fails_that_hosts_part_and_the_run_goes_on() {
    let w = mixed("stack-fails");
    w.aws.set_stack(
        &stack_of("beta"),
        Err(AwsError::Failed(
            "Stack with id agentspace-host-beta does not exist".into(),
        )),
    );
    let report = w.run_ok(now());
    assert_eq!(
        report,
        Report {
            lock_held: false,
            hosts: 6,
            running: 2,
            stopped: 1,
            unexplained: 0,
            failed: 1,
            log_dropped: false
        }
    );
    assert_eq!(
        w.rests(),
        vec![
            "host=beta failed step=instance reason=Stack with id agentspace-host-beta does not exist"
                .to_string(),
            alpha_stopped(),
        ]
    );
    let describe = &w.calls_of("ec2 describe-instances")[0];
    assert_eq!(
        describe.args[1],
        format!(
            "Name=instance-id,Values={},{},{},{}",
            id(0),
            id(1),
            id(3),
            id(4)
        )
    );
    assert_eq!(w.sent_ids(), vec![id(0)]);
    w.expect_host_failure_warned("beta", "Stack with id agentspace-host-beta does not exist");
}

/// Step 4: a stack with no `InstanceId` output fails the host's part with `Stack '<stack>' has no
/// InstanceId output.`
#[test]
fn a_stack_with_no_instance_id_output_fails_that_hosts_part() {
    let w = mixed("stack-no-output");
    w.aws
        .set_stack(&stack_of("beta"), Ok(json!({"Stacks": [{"Outputs": []}]})));
    let report = w.run_ok(now());
    assert_eq!(report.failed, 1);
    assert_eq!(
        w.rests()[0],
        "host=beta failed step=instance reason=Stack 'agentspace-host-beta' has no InstanceId output."
    );
}

// ---------------------------------------------------------------------------------------------
// A failure that ends the run (operator.md, guard, steps 5 and Its streams).
// ---------------------------------------------------------------------------------------------

/// Step 5: a failed describe fails the run with the call's failure message, logged as one
/// `failed step=describe` line with no `host=` field, and no stop and no send is made.
#[test]
fn a_failed_describe_fails_the_run_and_stops_nothing() {
    let w = mixed("describe-fails");
    let text =
        "An error occurred (RequestLimitExceeded) when calling the DescribeInstances operation";
    w.aws
        .answer_describe_with(Err(AwsError::Failed(text.into())));
    let failure = w.run_err(now());
    assert_eq!(failure.message, format!("agentspace: {text}"));
    assert_eq!(
        w.rests(),
        vec![format!("failed step=describe reason={text}")]
    );
    assert_eq!(
        w.call_keys(),
        strings(&["cloudformation describe-stacks", "ec2 describe-instances"])
    );
    assert!(w.progress().iter().all(|l| !l.starts_with("Hosts:")));
}

/// Step 5: an answer with no `Reservations` member is not an answer of EC2's: it fails the run
/// the same way, with a message that names the call, and stops nothing.
#[test]
fn an_answer_with_no_reservations_fails_the_run() {
    let w = mixed("no-reservations");
    w.aws.answer_describe_with(Ok(json!({})));
    let failure = w.run_err(now());
    assert!(
        failure
            .message
            .starts_with("agentspace: Unexpected answer from ec2 describe-instances"),
        "The message names the call: {}",
        failure.message
    );
    let rests = w.rests();
    assert_eq!(rests.len(), 1);
    assert!(
        rests[0].starts_with(
            "failed step=describe reason=Unexpected answer from ec2 describe-instances"
        )
    );
    assert!(w.stopped_ids().is_empty());
    assert!(w.sent_ids().is_empty());
}

/// AWS failures, #0034: an expired sign-in at the describe fails the run with the message
/// exactly, logs `failed step=describe reason=<that message without its prefix>` with no
/// `host=` field, and makes no further call.
#[test]
fn an_expired_sign_in_at_the_describe_ends_the_run() {
    let w = mixed("sign-in-describe");
    w.aws.answer_describe_with(Err(AwsError::SignIn));
    let failure = w.run_err(now());
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(w.rests(), vec![sign_in_at("describe")]);
    assert_eq!(
        w.call_keys(),
        strings(&["cloudformation describe-stacks", "ec2 describe-instances"])
    );
}

/// #0034 at step 4: an expired sign-in at a stack read ends the run, logged with `step=instance`
/// and no `host=` field, with no further call, the next host's stack read included.
#[test]
fn an_expired_sign_in_at_a_stack_read_ends_the_run() {
    let w = World::new("sign-in-stack");
    w.write_hosts(&[
        box_host(Some(&id(0))),
        ready_host("beta", None),
        ready_host("zeta", None),
    ]);
    w.write_leases(&[]);
    w.aws.set_stack(&stack_of("beta"), Err(AwsError::SignIn));
    w.aws.set_stack_id(&stack_of("zeta"), &id(5));
    let failure = w.run_err(now());
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(w.rests(), vec![sign_in_at("instance")]);
    assert_eq!(w.call_keys(), strings(&["cloudformation describe-stacks"]));
}

/// #0034 at step 6: an expired sign-in at a stop ends the run, logged with `step=stop` and no
/// `host=` field; a stop already requested stands and is logged; no further stop and no horizon
/// is sent.
#[test]
fn an_expired_sign_in_at_a_stop_ends_the_run() {
    let w = two_due("sign-in-stop");
    w.aws.fail_stop(&id(5), AwsError::SignIn);
    let failure = w.run_err(now());
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(w.stopped_ids(), vec![id(1), id(5)]);
    assert!(w.sent_ids().is_empty());
    assert_eq!(
        w.rests(),
        vec![
            format!("host=alpha stopped since={}", today("19:00:00")),
            sign_in_at("stop"),
        ]
    );
}

/// #0034 at step 7: an expired sign-in at a send ends the run, logged with `step=horizon` and no
/// `host=` field, after the step 6 lines; no further send is made.
#[test]
fn an_expired_sign_in_at_a_send_ends_the_run() {
    let w = mixed("sign-in-send");
    w.aws.fail_send(&id(0), AwsError::SignIn);
    let failure = w.run_err(now());
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(w.sent_ids(), vec![id(0)]);
    assert_eq!(
        w.rests(),
        vec![alpha_stopped(), beta_unexplained(), sign_in_at("horizon")]
    );
}

// ---------------------------------------------------------------------------------------------
// What it never writes (operator.md, guard, What it MUST NOT do; state.md, The guard's files).
// ---------------------------------------------------------------------------------------------

/// What it MUST NOT do: write the host registry or the lease state file, nor write back an
/// instance ID read from a stack, nor take `hosts.lock` or the lease state lock, nor make any
/// folder of its own but `.agentspace`: the bytes of both files are the same after the run, and
/// the folder holds `guard.lock`, `guard.log` and the two files alone, the home holding nothing
/// else.
#[test]
fn it_writes_neither_state_file_and_makes_no_other_file() {
    let w = mixed("writes-nothing");
    let hosts = std::fs::read(w.hosts_path()).unwrap();
    let leases = std::fs::read(w.leases_path()).unwrap();
    w.run_ok(now());
    assert_eq!(std::fs::read(w.hosts_path()).unwrap(), hosts, "hosts.json");
    assert_eq!(
        std::fs::read(w.leases_path()).unwrap(),
        leases,
        "leases.json"
    );
    assert_eq!(
        w.listing(&w.agentspace()),
        strings(&["guard.lock", "guard.log", "hosts.json", "leases.json"])
    );
    assert_eq!(w.listing(&w.home), strings(&[".agentspace"]));
}

/// The same for a registry that is missing: it is not made, and a run that logs nothing makes
/// no log.
#[test]
fn a_missing_registry_is_not_made() {
    let w = World::new("writes-nothing-missing");
    w.write_leases(&[lease_ready("swimcoach", &today("23:00:00"))]);
    w.aws.set_stack_id("agentbox", &id(0));
    w.aws
        .add_instance(&id(0), "running", Some(&launch("08:00:00")));
    w.run_ok(now());
    assert_eq!(
        w.listing(&w.agentspace()),
        strings(&["guard.lock", "leases.json"])
    );
}

/// Its log: a line that cannot be written, here because the log's path is a folder, changes
/// nothing the run decided or did: the stop and the sends are all made, and the run goes to its end
/// with `Ok`. The message `agentspace: Cannot write the guard's log <path>: <error>.` is said
/// exactly once, through `Say::warning`, at the first line dropped (alpha's `stopped` line, before
/// beta's progress line and long before the closing line), and the report says `log_dropped`
/// without counting the drop as a host's failure.
#[test]
fn a_line_that_cannot_be_written_is_warned_once_at_the_first_drop_and_reported() {
    let w = mixed("log-dropped");
    std::fs::create_dir_all(w.agentspace()).unwrap();
    std::fs::create_dir_all(w.guard_log()).unwrap();
    let report = w.run_ok(now());
    assert!(report.log_dropped, "The report says a line was dropped");
    assert_eq!(report.failed, 0, "A drop is not a host's failure");
    assert_eq!(
        (
            report.hosts,
            report.running,
            report.stopped,
            report.unexplained
        ),
        (6, 3, 1, 1),
        "A drop changes nothing the run decided"
    );
    assert_eq!(w.stopped_ids(), vec![id(1)]);
    assert_eq!(w.sent_ids(), vec![id(0), ZEROS.to_string()]);

    // Said once, as a warning, in the form the spec gives.
    let needle = "Cannot write the guard's log ";
    let events = w.events();
    let drops: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| match e {
            Event::Warning(text) | Event::Detail(text) => text.contains(needle),
            Event::Progress { person, .. } => person.contains(needle),
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(drops.len(), 1, "Said exactly once");
    let at = drops[0];
    let Event::Warning(text) = &events[at] else {
        panic!("The drop is said through warning, not another channel");
    };
    w.expect(
        text.starts_with("agentspace: Cannot write the guard's log ")
            && text.ends_with('.')
            && text.contains(": ")
            && !text.contains('\n'),
        "The message's form",
        text,
    );
    w.expect(
        text.contains(&w.guard_log().display().to_string()),
        "The message names the log's full path",
        text,
    );

    // At the point of the first drop: before the progress line of the host after it, and so
    // before the closing line; not at the end of the run.
    let position = |is: &dyn Fn(&str) -> bool| {
        events.iter().position(|e| match e {
            Event::Progress { person, .. } => is(person),
            _ => false,
        })
    };
    let beta = position(&|l| l.starts_with("The host of 'beta'")).expect("beta's progress line");
    let closing = position(&|l| l.starts_with("Hosts:")).expect("The closing line");
    assert!(
        at < beta,
        "Said at the first drop, before the next host's line"
    );
    assert!(beta < closing);
    assert_eq!(w.warnings().len(), 1, "Nothing else is warned");
    assert_eq!(
        w.progress().len(),
        3,
        "The progress lines are the run's own, and the drop is not one of them"
    );
}

/// Its log: a drop after which the run ends by a failure of its own has been said once already,
/// and the failure is the run's `Err`; the message is not said a second time.
#[test]
fn a_drop_before_a_failure_that_ends_the_run_is_said_once() {
    let w = mixed("log-dropped-then-ends");
    std::fs::create_dir_all(w.agentspace()).unwrap();
    std::fs::create_dir_all(w.guard_log()).unwrap();
    // A failed stack read for beta drops its `failed` line (the first drop); the sign-in message
    // at alpha's stop then ends the run, and its own line is dropped too.
    w.aws
        .set_stack(&stack_of("beta"), Err(AwsError::Failed("Not found".into())));
    w.aws.fail_stop(&id(1), AwsError::SignIn);
    let failure = w.run_err(now());
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    let needle = "Cannot write the guard's log ";
    let said = w.warnings().iter().filter(|l| l.contains(needle)).count();
    assert_eq!(said, 1, "Said once, though two lines were dropped");
}

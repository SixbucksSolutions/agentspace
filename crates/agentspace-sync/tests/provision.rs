//! `provision`'s run that succeeds: its steps in order, the records it writes, its log, its locks and
//! its folder (docs/operator.md, provision; The operator's folder, one per command; docs/state.md,
//! The host registry, Its states, A host's folder, The provisioning run's log and lock).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `provision_using` in a child of the test binary, under a throwaway home with a throwaway CA, with
//! stand-ins for the AWS CLI, ssh, scp and go. Every stand-in records, at each call, the registry's
//! content and whether the run's lock and the registry's lock are held, which is how a test sees
//! what a record said while a step ran. The failures are in `provision_failures`.
//!
//! **The last step calls the EC2 driver's `place_pending`** (#0150). The tests of what it
//! placed and left pending run against it; the others accept either outcome of that step, or make
//! the whole call fail with a lease state file that is not valid (driver.md, place_pending,
//! step 2.2).
#![cfg(windows)]

mod common;
mod hostcalls;

use common::hosts::*;
use common::provlog::LogLine;
use common::times::{now_secs, parse_utc};
use common::*;
use hostcalls::setup::*;

const COPYING: &str = "Copying box/system to the instance...";
const RUNNING: &str = "Running the toolchain bootstrap on the instance...";

/// The messages of a run that matter here, in order, for a host that is `project`'s.
fn expected_messages(project: &str, id: &str, sign_in: bool) -> Vec<String> {
    let mut v = vec![
        format!("Provisioning the host of '{project}'..."),
        format!("Deploying stack '{}' in us-west-2...", stack_of(project)),
        format!("Instance: {id}"),
        COPYING.to_string(),
        RUNNING.to_string(),
        format!("Bootstrapped: {id}"),
        format!("Recorded the host of '{project}' as ready."),
    ];
    if sign_in {
        v.push(sign_in_line(project));
    }
    v
}

fn run_messages(say: &RecSay, project: &str) -> Vec<String> {
    let keep = expected_messages(project, ID2, true);
    say.progress_lines()
        .into_iter()
        .filter(|l| {
            keep.contains(l) || l.starts_with("Instance: ") || l.starts_with("Bootstrapped: ")
        })
        .collect()
}

/// Checks the run's own lines: the six steps in order, `place` either way, then `run done`.
fn check_own_lines(rig: &Rig, project: &str) -> Vec<LogLine> {
    let lines = rig.own_log_lines(project);
    let names: Vec<String> = lines.iter().map(LogLine::name).collect();
    rig.check(
        names.len() == 8
            && names[..6]
                == svec(&[
                    "run begun",
                    "deploy done",
                    "start done",
                    "sync done",
                    "bootstrap done",
                    "record done",
                ])[..]
            && (names[6] == "place done" || names[6] == "place failed")
            && names[7] == "run done",
        &format!(
            "A run's own lines MUST be run begun, then done for each step it reached in order, then run done; they were {names:?}"
        ),
    );
    rig.check(
        lines
            .iter()
            .all(|l| l.reason.is_none() || l.outcome == "failed")
            && lines.iter().all(|l| (now_secs() - l.time).abs() < 600),
        "A line's time MUST be the time it was written, in UTC, and `reason` is for `failed` alone",
    );
    lines
}

/// The log holds the run's own lines alone (operator.md, provision, Its log; state.md, The
/// provisioning run's log; architect, 2026-11-21, for v0.07-T03): every line is one of them, and
/// the programs' output, which goes to the run's streams, is not in it.
fn check_log_holds_own_lines_alone(rig: &Rig, project: &str) {
    let text = rig.provision_log(project).unwrap_or_default();
    rig.check(
        !text.is_empty()
            && text
                .lines()
                .all(|l| common::provlog::parse_log_line(l).is_some()),
        &format!("Every line of the log MUST be one of the run's own lines:\n{text}"),
    );
    for out in [DEPLOY_OUT, INSTALL_OUT, BOOTSTRAP_OUT] {
        rig.check(
            !text.contains(out),
            &format!("The programs' output MUST NOT be copied into the log, and `{out}` is in it"),
        );
    }
}

#[test]
fn the_log_holds_the_runs_own_lines_alone_and_the_programs_output_goes_to_its_streams() {
    let rig = prov_rig(Kind::NoRecord);
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        check_log_holds_own_lines_alone(rig, PROJECT);
    }) else {
        return;
    };
    for line in [DEPLOY_OUT, INSTALL_OUT, BOOTSTRAP_OUT] {
        assert!(
            out.stdout.contains(line),
            "Each program's output MUST pass through to the run's stdout: `{line}` is missing"
        );
    }
}
#[test]
fn a_new_hosts_run_records_it_creating_then_ready_and_runs_every_step_in_order() {
    let rig = prov_rig(Kind::NoRecord);
    let Some(out) = rig.run(|rig| {
        let (stack, alias) = names();
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();

        // The log's own lines; that it holds nothing else is another test's.
        check_own_lines(rig, PROJECT);

        // The messages, the sign-in line last.
        rig.same_list(
            "The run's messages",
            &run_messages(&say, PROJECT),
            &expected_messages(PROJECT, ID2, true),
        );
        let events = say.events.borrow();
        rig.check(
            events.last().is_some_and(|(k, t)| k == "progress" && *t == sign_in_line(PROJECT)),
            "The sign-in line MUST be the run's last line, after step 9's warning if there was one",
        );
        drop(events);

        // What the registry said while each step ran.
        let at_first = log.watched("aws", 1, "hosts.json").unwrap_or("");
        let start = record_in(at_first, PROJECT);
        let attempt = attempt_in(&start);
        rig.check(
            state_is(&start, "creating")
                && !start.contains("instance_id")
                && attempt.as_deref().and_then(parse_utc).is_some_and(|t| (now_secs() - t).abs() < 600)
                && start.contains(&format!("\"stack\":\"{stack}\"")),
            &format!("When step 4 begins, the new record MUST be `creating` with the stack's name, the run's start time and no instance ID: {start}"),
        );
        for (prog, n) in [("ssh", 1), ("ssh", 3)] {
            let record = record_in(log.watched(prog, n, "hosts.json").unwrap_or(""), PROJECT);
            rig.check(
                state_is(&record, "creating") && record.contains(&format!("\"instance_id\":\"{ID2}\"")),
                &format!("While sync and bootstrap run, the record MUST be `creating` and hold the instance ID step 4 wrote: {record}"),
            );
        }
        let done = rig.record(PROJECT);
        rig.check(
            done.state.as_str() == "ready"
                && done.instance_id.as_deref() == Some(ID2)
                && done.last_attempt_at == attempt
                && done.stack == stack,
            "Step 8 MUST record the host `ready` with the instance ID, leaving last_attempt_at as step 3 wrote it",
        );
        rig.check(
            rig.read_hosts().hosts[0].project == "swimcoach",
            "The box's record MUST stay first in the registry",
        );

        // The locks: the run's held at every call, the registry's never.
        let run_lock = log.probed("provision.lock");
        let registry_lock = log.probed("hosts.lock");
        rig.check(
            !run_lock.is_empty() && run_lock.iter().all(|p| p.2),
            "provision.lock MUST be held for the whole run, at every program it started",
        );
        rig.check(
            !registry_lock.is_empty() && registry_lock.iter().all(|p| !p.2),
            "hosts.lock MUST NOT be held while a program runs",
        );
        rig.check(
            matches!(agentspace_hosts::try_run_lock(&rig.home, PROJECT), Ok(Some(_))),
            "provision.lock MUST be released when the run ends",
        );

        // Step 5 and the rest go over the host's alias, from one folder of the run's own.
        for c in log.calls("ssh") {
            rig.check(c.args.contains(&alias), "Every ssh of the run MUST name the host's alias");
        }
        for c in log.calls("scp") {
            rig.check(
                c.args.last().is_some_and(|a| a.starts_with(&format!("{alias}:"))),
                "Every scp of the run MUST copy to the host's alias",
            );
        }
        let aws = log.calls("aws");
        let template = template_arg(&aws[1].args);
        rig.same_list(
            "The cloudformation deploy call",
            &aws[1].args,
            &deploy_call_args(&stack, &template, &creation_overrides(rig, &stack)),
        );
        let hex = check_template(rig, &log, 2, &template);
        rig.same(
            "The folder of the template and of box/system: one for the whole run",
            &common::expect::command_folder_of(&log.calls("scp")[0].args[4]).unwrap_or_default(),
            &hex,
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The run's folder MUST be removed as the run ends",
        );
        rig.check(
            rig.cert_info(PROJECT).key_id == format!("operator-{PROJECT}")
                && rig.alias_file(PROJECT).is_file(),
            "The run's sync MUST leave the operator's certificate and alias for the host",
        );
        common::expect::check_environment(rig, &log);

        // Step 9 either placed the leases or failed, with the warning and the leases pending.
        let place = rig.own_log_lines(PROJECT);
        let failed = place[6].outcome == "failed";
        let warnings = say.warnings();
        if failed {
            // The reason's own last period is the warning's `;` position, as the spec's text reads.
            let reason = place[6].reason.clone().unwrap_or_default();
            let reason = reason.trim_end_matches('.').to_string();
            rig.check(
                warnings.len() == 1
                    && warnings[0].contains(&format!("The pending leases of '{PROJECT}' could not be placed: "))
                    && warnings[0].ends_with("; the next run places them.")
                    && warnings[0].contains(&reason),
                &format!("A failed step 9 MUST warn `agentspace: The pending leases of '<project>' could not be placed: <reason>; the next run places them.`: {warnings:?}"),
            );
        } else {
            rig.check(warnings.is_empty(), "A run whose leases were placed warns of nothing");
        }
    }) else {
        return;
    };
    assert!(
        out.stdout.contains(DEPLOY_OUT) && out.stdout.contains(INSTALL_OUT),
        "The programs' output MUST also pass through to the run's stdout"
    );
}

#[test]
fn a_ready_hosts_run_writes_only_the_attempt_at_its_start_and_keeps_ready_throughout() {
    let rig = prov_rig(Kind::Ready);
    rig.run(|rig| {
        let (stack, _) = names();
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        check_own_lines(rig, PROJECT);

        // Step 3 wrote last_attempt_at and nothing else: still `ready`, still the old instance ID.
        let first = record_in(log.watched("aws", 1, "hosts.json").unwrap_or(""), PROJECT);
        let attempt = attempt_in(&first);
        rig.check(
            state_is(&first, "ready")
                && first.contains(&format!("\"instance_id\":\"{ID}\""))
                && attempt.as_deref().and_then(parse_utc).is_some_and(|t| (now_secs() - t).abs() < 600)
                && attempt.as_deref() != Some(OLD_ATTEMPT),
            &format!("A ready host's record MUST stay `ready` and change only last_attempt_at at the run's start: {first}"),
        );
        // At every call of the run the record said `ready`: the project's leases kept reaching it.
        let states = log.watched_calls("hosts.json");
        rig.check(
            !states.is_empty()
                && states
                    .iter()
                    .all(|(_, _, c)| state_is(&record_in(c, PROJECT), "ready")),
            "A run MUST never take a ready host out of ready, not for one moment",
        );
        let done = rig.record(PROJECT);
        rig.check(
            done.state.as_str() == "ready"
                && done.instance_id.as_deref() == Some(ID2)
                && done.last_attempt_at == attempt
                && done.stack == stack,
            "A record that was ready keeps ready, with the instance ID the run learned, and the attempt step 3 wrote",
        );

        // Its deploy updated the stack: no parameter at all.
        let aws = log.calls("aws");
        rig.check(
            aws[1].args == deploy_call_args(&stack, &template_arg(&aws[1].args), &[]),
            "A run on a host whose stack exists MUST deploy with no parameter, so every one keeps its value",
        );
        rig.check(
            !say.progress_lines().iter().any(|l| l.starts_with("Sign in on this host")),
            "A host that was already ready has had its sign-ins: no sign-in line",
        );
        rig.same_list(
            "The run's messages",
            &run_messages(&say, PROJECT),
            &expected_messages(PROJECT, ID2, false),
        );
    });
}

/// A run on a host whose record is `kind` records it `creating` first and ends it `ready`.
fn never_ready_case(kind: Kind) {
    let rig = prov_rig(kind);
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        let first = record_in(log.watched("aws", 1, "hosts.json").unwrap_or(""), PROJECT);
        rig.check(
            state_is(&first, "creating") && attempt_in(&first).is_some(),
            &format!("A {kind:?} record MUST be set `creating`, with the run's start time, when the run begins: {first}"),
        );
        let done = rig.record(PROJECT);
        rig.check(
            done.state.as_str() == "ready" && done.instance_id.as_deref() == Some(ID2),
            "The run MUST end by recording the host ready",
        );
        rig.check(
            say.events
                .borrow()
                .last()
                .is_some_and(|(_, t)| *t == sign_in_line(PROJECT)),
            "A host that was not ready when the run began gets the sign-in line, last",
        );
    });
}

#[test]
fn a_creating_hosts_run_records_it_creating_first_and_ends_ready() {
    never_ready_case(Kind::Creating);
}

#[test]
fn a_failed_hosts_run_records_it_creating_first_and_ends_ready() {
    never_ready_case(Kind::Failed);
}

#[test]
fn a_quiet_run_prints_nothing_of_its_own_and_still_writes_its_log() {
    let rig = prov_rig(Kind::NoRecord);
    rig.run(|rig| {
        let say = rig.quiet_say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        rig.check(
            say.events.borrow().is_empty(),
            "-Quiet MUST suppress every progress message and warning of every step",
        );
        rig.check(
            rig.own_log_names(PROJECT).first().map(String::as_str) == Some("run begun")
                && rig.own_log_names(PROJECT).last().map(String::as_str) == Some("run done"),
            "-Quiet MUST NOT change the log",
        );
    });
}

#[test]
fn a_run_whose_leases_were_placed_logs_place_done_and_warns_of_nothing() {
    let rig = prov_rig(Kind::NoRecord);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let lines = rig.own_log_lines(PROJECT);
        let names: Vec<String> = lines.iter().map(LogLine::name).collect();
        rig.check(
            names.len() == 8 && names[6] == "place done" && names[7] == "run done",
            &format!("A run MUST log `place done` after `record done`, then `run done`: {names:?}"),
        );
        rig.check(
            lines[6].field.as_deref() == Some("placed=0"),
            "With no pending lease, `place done` MUST log that it placed none",
        );
        rig.check(
            say.warnings().is_empty(),
            "A run whose leases were placed warns of nothing",
        );
    });
}

// ---------------------------------------------------------------- Step 9's result in the log

/// A lease state file record of a `pending` lease of `project`, placed by the EC2 driver.
fn pending_lease(id: &str, project: &str) -> String {
    format!(
        "{{\"id\":\"{id}\",\"project\":\"{project}\",\"state\":\"pending\",\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\"ttl_seconds\":28800,\"created_at\":\"2026-11-21T08:00:00Z\",\"driver\":\"ec2\"}}"
    )
}

const LEASE_A: &str = "8d41b6a2-0c3e-4f57-a1d9-2e6b7c8f9a01";
const LEASE_B: &str = "1f2e3d4c-5b6a-4978-8a1b-2c3d4e5f6a7b";
const LEASE_OTHER: &str = "c0ffee00-1234-4abc-8def-0123456789ab";

#[test]
fn a_failed_place_pending_is_logged_with_its_reason_warned_and_the_run_still_succeeds() {
    // A lease state file that is not valid fails the whole call (driver.md, place_pending, step
    // 2.2).
    let rig = prov_rig(Kind::NoRecord);
    rig.write_home(
        ".agentspace/leases.json",
        b"this is not a lease state file\n",
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let lines = rig.own_log_lines(PROJECT);
        let names: Vec<String> = lines.iter().map(LogLine::name).collect();
        rig.same_list(
            "The run's own lines",
            &names,
            &svec(&[
                "run begun",
                "deploy done",
                "start done",
                "sync done",
                "bootstrap done",
                "record done",
                "place failed",
                "run done",
            ]),
        );
        let place = &lines[6];
        let reason = place.reason.clone().unwrap_or_default();
        rig.check(
            place.field.is_none() && !reason.is_empty() && !reason.starts_with("agentspace: "),
            "A whole call's failure MUST be one `place failed` line with its message, unprefixed, as the reason",
        );
        rig.same_list(
            "The warnings",
            &say.warnings(),
            &[format!(
                "agentspace: The pending leases of '{PROJECT}' could not be placed: {}; the next run places them.",
                reason.trim_end_matches('.')
            )],
        );
        rig.check(
            rig.record(PROJECT).state.as_str() == "ready",
            "A failed step 9 MUST leave the host `ready`",
        );
        rig.check(
            say.events
                .borrow()
                .last()
                .is_some_and(|(_, t)| *t == sign_in_line(PROJECT)),
            "The run goes on to step 10 after a failed step 9: the sign-in line is its last",
        );
        rig.check(
            std::fs::read(rig.home_path(".agentspace/leases.json")).ok()
                == Some(b"this is not a lease state file\n".to_vec()),
            "A failed step 9 MUST leave the lease state file as it was",
        );
    });
}

#[test]
fn place_pendings_result_is_logged_as_the_count_placed_then_one_line_per_lease_left_pending() {
    // Two pending leases of the project, and one of another project, which step 9 never names.
    // Lease A cannot be placed: a folder stands where its alias file goes, so removing a killed
    // placement's leftovers fails it (driver.md, place_pending, step 2.4). Lease B is placed or
    // not, as the throwaway home allows; either way the log names it rightly.
    let rig = prov_rig(Kind::NoRecord);
    let leases = format!(
        "{{\"version\":0,\"leases\":[{},{},{}]}}\n",
        pending_lease(LEASE_A, PROJECT),
        pending_lease(LEASE_OTHER, "otherproject"),
        pending_lease(LEASE_B, PROJECT)
    );
    rig.write_home(".agentspace/leases.json", leases.as_bytes());
    rig.mkdir_home(&format!(".ssh/agentspace.d/agentspace-{PROJECT}-8d41b6a2"));
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let lines = rig.own_log_lines(PROJECT);
        let names: Vec<String> = lines.iter().map(LogLine::name).collect();
        rig.check(
            names.len() >= 9
                && names[5] == "record done"
                && names[6] == "place done"
                && names.last().map(String::as_str) == Some("run done"),
            &format!("Step 9 MUST log `place done` after `record done`, its lease lines after it, then `run done`: {names:?}"),
        );
        let placed: usize = lines[6]
            .field
            .as_deref()
            .and_then(|f| f.strip_prefix("placed="))
            .and_then(|n| n.parse().ok())
            .unwrap_or(usize::MAX);
        let failed: Vec<&LogLine> = lines[7..lines.len() - 1].iter().collect();
        rig.check(
            failed.iter().all(|l| l.name() == "place failed" && l.reason.is_some()),
            "Each line between `place done` and `run done` MUST be a lease left pending, with why",
        );
        let ids: Vec<String> = failed
            .iter()
            .map(|l| l.field.clone().unwrap_or_default())
            .collect();
        rig.check(
            placed + failed.len() == 2,
            &format!("The count placed and the leases left MUST be the project's two pending leases: placed={placed}, left {ids:?}"),
        );
        rig.check(
            ids.first().map(String::as_str) == Some(&*format!("lease={LEASE_A}"))
                && ids.iter().all(|i| *i == format!("lease={LEASE_A}") || *i == format!("lease={LEASE_B}")),
            &format!("The leases left MUST be named by ID, in the order tried, and never another project's: {ids:?}"),
        );
        rig.check(
            !say.warnings().iter().any(|w| w.contains("could not be placed")),
            "A call that returned MUST NOT warn as a failed whole call does",
        );
        let text = std::fs::read_to_string(rig.home_path(".agentspace/leases.json")).unwrap_or_default();
        let at = text.find(LEASE_A).unwrap_or(0);
        rig.check(
            text[at..].split('}').next().is_some_and(|r| r.contains("\"state\":\"pending\"")),
            "A lease that could not be placed MUST stay pending",
        );
    });
}

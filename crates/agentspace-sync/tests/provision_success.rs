//! `provision` records that its run succeeded, and only then (docs/operator.md, provision, step 3
//! and step 9's end, "Then, when the run succeeded, it records the success", and the tests table
//! of E0.10-T04-S02; docs/state.md, The last success, The host registry; docs/driver.md, After
//! the run; docs/decisions.md, D77 and its change note; E0.10-T02-S03, card #0216).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `provision_using` in a child of the test binary, under a throwaway home with a throwaway CA, with
//! stand-ins for the AWS CLI, ssh, scp and go, as `provision.rs` does. What it judges:
//!
//! - **A run that placed or answered every `pending` lease of the project** and failed at no step
//!   writes `last_success_at`, once, at its end: after `place done`, before `run done`, at or after
//!   the `last_attempt_at` its step 3 wrote, in UTC in state.md's form.
//! - **A run that has not succeeded writes none** and goes on as before v0.10: a run whose placing
//!   failed as a whole (`place failed`), failed for one lease (`place failed lease=<id>`) or left
//!   one (`place left lease=<id>`) still exits 0, and a run that failed at step 4, 5, 6 or 7 exits
//!   1; each ends with the record holding none.
//! - **A lease the call answered `unsatisfiable`** (`place refused lease=<id>`) **is not a
//!   failure**, and the run records its success.
//! - **Step 3 removes a `last_success_at` the record holds**, in the write that sets
//!   `last_attempt_at`, so no stand-in program ever sees a success of a run that has begun (pin
//!   P24).
//!
//! No test waits for a time: a run's own clock is compared with the seeds' times, and the
//! seeds are years old.
#![cfg(windows)]

mod common;
mod hostcalls;

use common::hosts::*;
use common::leases::{self, FAR, IDS};
use common::provlog::LogLine;
use common::times::{now_secs, parse_utc};
use common::*;
use hostcalls::setup::*;

/// A success older than any run of these tests, at or after [`OLD_ATTEMPT`]: a seeded record that
/// says its last run succeeded, which step 3 of a new run MUST remove.
const OLD_SUCCESS: &str = "2026-11-19T00:00:05Z";

/// The box's record as the registry writes it.
const BOX_RECORD: &str = "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}";

/// Seeds `hosts.json` after `prov_rig`: the box, then, for a kind with a record, [`PROJECT`]'s
/// record as the run before it left it, holding a `last_success_at` at or after its
/// `last_attempt_at`, as a record does that was last written by a run that succeeded.
fn seed_with_success(rig: &Rig, kind: Kind) {
    let stack = stack_of(PROJECT);
    let record = match kind {
        // `prov_rig` wrote no registry for a project with no record, and none is written here.
        Kind::NoRecord => return,
        Kind::Creating | Kind::Failed => {
            let state = if kind == Kind::Creating {
                "creating"
            } else {
                "failed"
            };
            format!(
                "{{\"project\":\"{PROJECT}\",\"stack\":\"{stack}\",\"state\":\"{state}\",\"last_attempt_at\":\"{OLD_ATTEMPT}\",\"last_success_at\":\"{OLD_SUCCESS}\"}}"
            )
        }
        Kind::Ready => format!(
            "{{\"project\":\"{PROJECT}\",\"stack\":\"{stack}\",\"state\":\"ready\",\"instance_id\":\"{ID}\",\"last_attempt_at\":\"{OLD_ATTEMPT}\",\"last_success_at\":\"{OLD_SUCCESS}\"}}"
        ),
    };
    let text = format!("{{\"version\":0,\"hosts\":[{BOX_RECORD},{record}]}}\n");
    rig.write_home(".agentspace/hosts.json", text.as_bytes());
}

fn has_success(record_text: &str) -> bool {
    record_text.contains("\"last_success_at\"")
}

/// What step 3 leaves for every program of the run to see: the record carries the run's own start
/// time and no `last_success_at`, whether it held one or not. A program's watch of the registry
/// is its content at the call.
#[track_caller]
fn check_no_success_while_the_run_works(rig: &Rig, log: &Log) {
    let watched = log.watched_calls("hosts.json");
    rig.check(
        !watched.is_empty(),
        "The stand-ins MUST have watched the registry",
    );
    for (prog, n, content) in &watched {
        let record = record_in(content, PROJECT);
        rig.check(
            !record.is_empty() && !has_success(&record),
            &format!(
                "At {prog} call {n} the record MUST hold no last_success_at, since step 3 removed it and only the end of step 9 writes one: {record}"
            ),
        );
        rig.check(
            attempt_in(&record).is_some_and(|a| a != OLD_ATTEMPT),
            &format!("At {prog} call {n} the record MUST hold the run's own start time: {record}"),
        );
    }
}

/// The run's own `place` lines, in order.
fn place_lines(rig: &Rig) -> Vec<LogLine> {
    rig.own_log_lines(PROJECT)
        .into_iter()
        .filter(|l| l.step == "place")
        .collect()
}

/// The final record has no success, and the registry's bytes name none.
#[track_caller]
fn check_no_success_recorded(rig: &Rig, why: &str) {
    let record = rig.record(PROJECT);
    rig.check(
        record.last_success_at.is_none(),
        &format!(
            "{why}: the record MUST hold no last_success_at, and it holds {:?}",
            record.last_success_at
        ),
    );
    let bytes = rig.hosts_bytes().unwrap_or_default();
    rig.check(
        !String::from_utf8_lossy(&bytes).contains("last_success_at"),
        &format!("{why}: no record of the registry MUST name last_success_at"),
    );
}

/// The final record holds the success of a run that ended now: in state.md's form, at or after the
/// run's own `last_attempt_at`, not after now, written last in the record, and the rest of the
/// record as step 8 left it.
#[track_caller]
fn check_success_recorded(rig: &Rig) -> String {
    let record = rig.record(PROJECT);
    let attempt = record.last_attempt_at.clone().unwrap_or_default();
    let success = record.last_success_at.clone().unwrap_or_default();
    let attempted = parse_utc(&attempt);
    let succeeded = parse_utc(&success);
    rig.check(
        attempted.is_some() && attempt != OLD_ATTEMPT,
        &format!("The record MUST hold the run's own start time: {attempt:?}"),
    );
    rig.check(
        succeeded.is_some(),
        &format!(
            "last_success_at MUST be in state.md's form, YYYY-MM-DDTHH:MM:SSZ, and it is {success:?}"
        ),
    );
    let (a, s) = (attempted.unwrap_or(0), succeeded.unwrap_or(0));
    rig.check(
        s >= a,
        &format!(
            "last_success_at MUST be at or after last_attempt_at: {success} against {attempt}"
        ),
    );
    rig.check(
        s <= now_secs() && now_secs() - s < 600,
        &format!("last_success_at MUST be the time the run ended, now: {success}"),
    );
    rig.check(
        record.state.as_str() == "ready",
        "A run that succeeded leaves its host ready",
    );
    // The member is the record's last, after last_attempt_at, and the record is as step 8 left it.
    let text = String::from_utf8_lossy(&rig.hosts_bytes().unwrap_or_default()).into_owned();
    let record_text = record_in(&text, PROJECT);
    rig.check(
        record_text.ends_with(&format!(
            "\"last_attempt_at\":\"{attempt}\",\"last_success_at\":\"{success}\"}}"
        )),
        &format!("The record MUST end with last_attempt_at, then last_success_at: {record_text}"),
    );
    rig.check(
        record.instance_id.as_deref() == Some(ID2),
        "The success's write MUST NOT change the instance ID step 8 recorded",
    );
    success
}

// ---------------------------------------------------------------- A run that placed everything

/// A run with no pending lease, for a host as `kind` says: every kind ends with the success, and a
/// success the seeded record held is gone when the run's first program runs.
fn success_case(kind: Kind) {
    let rig = prov_rig(kind);
    seed_with_success(&rig, kind);
    rig.run(move |rig| {
        let box_before = rig.record("swimcoach");
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();

        // Step 3 removed a success the record held, and nothing wrote one before the end.
        check_no_success_while_the_run_works(rig, &log);

        // The run's own lines are as before v0.10: the success's write adds none.
        let names = rig.own_log_names(PROJECT);
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
                "place done",
                "run done",
            ]),
        );
        let text = rig.provision_log(PROJECT).unwrap_or_default();
        rig.check(
            !text.contains("success failed"),
            "A write that succeeds MUST add no line of its own, and a failed one is not here",
        );
        rig.check(
            say.warnings().is_empty(),
            "Recording a success MUST NOT warn of anything",
        );

        // The success is the run's, written after `place done` and before `run done`.
        let success = check_success_recorded(rig);
        let lines = rig.own_log_lines(PROJECT);
        let placed = parse_utc(&success).unwrap_or(0);
        rig.check(
            placed >= lines[6].time && placed <= lines[7].time,
            &format!(
                "last_success_at MUST be written after `place done` and before `run done`: the success {success}, place done at {}, run done at {}",
                lines[6].time, lines[7].time
            ),
        );
        rig.check(
            rig.record("swimcoach") == box_before,
            "The box's record MUST be left as it was",
        );
        rig.check(
            matches!(
                agentspace_hosts::try_run_lock(&rig.home, PROJECT),
                Ok(Some(_))
            ),
            "provision.lock MUST be released when the run ends",
        );
    });
}

#[test]
fn a_new_hosts_run_that_placed_everything_records_its_success() {
    success_case(Kind::NoRecord);
}

#[test]
fn a_creating_hosts_run_that_placed_everything_records_its_success_and_drops_the_old_one() {
    success_case(Kind::Creating);
}

#[test]
fn a_failed_hosts_run_that_placed_everything_records_its_success_and_drops_the_old_one() {
    success_case(Kind::Failed);
}

#[test]
fn a_ready_hosts_run_that_placed_everything_records_its_success_and_drops_the_old_one() {
    success_case(Kind::Ready);
}

// ---------------------------------------------------------------- Placing, and what a success is

#[test]
fn a_run_that_placed_its_pending_lease_records_its_success_after_placing_it() {
    // One pending lease of the project, at the size the host meets: the call places it, logs
    // `place done placed=1` and no `place failed` and no `place left`, and only then is the run's
    // success written (operator.md, provision, step 9 and its end).
    let rig = prov_rig(Kind::NoRecord);
    rig.write_home(
        ".agentspace/leases.json",
        leases::file(&[leases::pending(IDS[0], PROJECT, "m")]).as_bytes(),
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let lines = place_lines(rig);
        rig.check(
            lines.len() == 1
                && lines[0].outcome == "done"
                && lines[0].field.as_deref() == Some("placed=1"),
            &format!("The call MUST place the one lease and log nothing else of step 9: {lines:?}"),
        );
        let lease =
            std::fs::read_to_string(rig.home_path(".agentspace/leases.json")).unwrap_or_default();
        rig.check(
            lease.contains("\"state\":\"ready\"") && !lease.contains("\"state\":\"pending\""),
            "The lease MUST be placed `ready` before the run's success is recorded",
        );
        let success = check_success_recorded(rig);
        let placed_at = lines.first().map_or(0, |l| l.time);
        rig.check(
            parse_utc(&success).unwrap_or(0) >= placed_at,
            "The success MUST be written after `place done`",
        );
    });
}

#[test]
fn a_lease_the_call_answered_unsatisfiable_is_not_a_failure_and_the_run_records_its_success() {
    // A live ready lease holds the host, so the run passes no type and the host stays at the type
    // the stack reads back; the pending `xl` lease it does not meet is answered `unsatisfiable`
    // with `no_capacity`: `place refused`, which is not a failure (operator.md, provision, step 9's
    // end; pin P22).
    let rig = prov_rig(Kind::Ready);
    seed_with_success(&rig, Kind::Ready);
    rig.write_home(
        ".agentspace/leases.json",
        leases::file(&[
            leases::ready(IDS[0], PROJECT, "m", FAR),
            leases::pending(IDS[1], PROJECT, "xl"),
        ])
        .as_bytes(),
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let lines = place_lines(rig);
        let refused: Vec<String> = lines
            .iter()
            .filter(|l| l.outcome == "refused")
            .map(|l| l.field.clone().unwrap_or_default())
            .collect();
        rig.same_list(
            "The leases the call refused",
            &refused,
            &[format!("lease={}", IDS[1])],
        );
        rig.check(
            !lines
                .iter()
                .any(|l| l.outcome == "failed" || l.outcome == "left"),
            "No lease failed and none was left: the refused one was answered",
        );
        check_success_recorded(rig);
    });
}

#[test]
fn a_lease_left_pending_for_the_next_run_is_not_a_success() {
    // The pending `xl` lease asks for a host the stack does not give: the stand-in reads back
    // `m9g.xlarge`, which `xl` does not meet, and no live ready lease holds the host, so the call
    // leaves the lease `pending` for the next run: `place left lease=<id>`, and the run exits 0.
    let rig = prov_rig(Kind::Ready);
    seed_with_success(&rig, Kind::Ready);
    rig.write_home(
        ".agentspace/leases.json",
        leases::file(&[leases::pending(IDS[0], PROJECT, "xl")]).as_bytes(),
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let lines = place_lines(rig);
        let left: Vec<String> = lines
            .iter()
            .filter(|l| l.outcome == "left")
            .map(|l| l.field.clone().unwrap_or_default())
            .collect();
        rig.same_list(
            "The leases the call left pending",
            &left,
            &[format!("lease={}", IDS[0])],
        );
        check_no_success_recorded(rig, "A run that left a lease");
        rig.check(
            rig.record(PROJECT).state.as_str() == "ready",
            "The host stays ready",
        );
    });
}

#[test]
fn a_run_whose_placing_failed_as_a_whole_exits_ok_and_records_no_success() {
    // A lease state file that is not valid fails the whole call at step 9: the file is valid until
    // the bootstrap's ssh, the run's last program, changes it, as `provision.rs` does it.
    let rig = prov_rig_with(Kind::Ready, |rig| {
        rig.overwrite_at(
            "ssh",
            3,
            "",
            &rig.home_path(".agentspace/leases.json"),
            "this is not a lease state file\\n",
        );
    });
    seed_with_success(&rig, Kind::Ready);
    rig.write_home(
        ".agentspace/leases.json",
        b"{\"version\":0,\"leases\":[]}\n",
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let lines = place_lines(rig);
        rig.check(
            lines
                .iter()
                .any(|l| l.outcome == "failed" && l.field.is_none() && l.reason.is_some()),
            "The call's failure as a whole MUST be logged as one `place failed` line with a reason",
        );
        rig.check(
            say.warnings()
                .iter()
                .any(|w| w.contains("could not be placed")),
            "A whole call's failure warns, as it did before v0.10",
        );
        check_no_success_recorded(rig, "A run whose placing failed as a whole");
    });
}

#[test]
fn a_run_with_one_lease_that_failed_to_place_exits_ok_and_records_no_success() {
    // Lease A cannot be placed: a folder stands where its alias file goes (driver.md,
    // place_pending, step 2.4). The call returns, the run exits 0, and the run has not succeeded.
    let rig = prov_rig(Kind::Ready);
    seed_with_success(&rig, Kind::Ready);
    rig.write_home(
        ".agentspace/leases.json",
        leases::file(&[
            leases::pending(IDS[0], PROJECT, "m"),
            leases::pending(IDS[1], PROJECT, "m"),
        ])
        .as_bytes(),
    );
    rig.mkdir_home(&format!(
        ".ssh/agentspace.d/agentspace-{PROJECT}-{}",
        &IDS[0][..8]
    ));
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let failed: Vec<String> = place_lines(rig)
            .iter()
            .filter(|l| l.outcome == "failed")
            .map(|l| l.field.clone().unwrap_or_default())
            .collect();
        rig.check(
            failed.first().map(String::as_str) == Some(&*format!("lease={}", IDS[0])),
            &format!("The lease that could not be placed MUST be logged by its ID: {failed:?}"),
        );
        check_no_success_recorded(rig, "A run that failed to place one lease");
    });
}

// ---------------------------------------------------------------- A run that failed at a step

#[derive(Clone, Copy)]
enum Step {
    Deploy,
    Start,
    Sync,
    Bootstrap,
}

impl Step {
    fn inject(self, rig: &Rig) {
        match self {
            Step::Deploy => rig.rule("aws", 0, "cloudformation deploy", 1, "", "boom\\n"),
            Step::Start => rig.rule(
                "aws",
                0,
                "describe-instances",
                255,
                "",
                "Unable to locate credentials\\n",
            ),
            Step::Sync => rig.rule("ssh", 1, "", 1, "", ""),
            Step::Bootstrap => rig.rule("ssh", 3, "", 1, "", ""),
        }
    }
}

/// A run with `step` failing, for a host as `kind` says, whose record held a success: the run
/// exits 1, and the record ends with none, step 3 having removed the old one.
fn failed_step_case(step: Step, kind: Kind) {
    let rig = prov_rig_with(kind, |rig| step.inject(rig));
    seed_with_success(&rig, kind);
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        rig.check(
            !f.message.is_empty(),
            "A failed step fails the run with its message",
        );
        check_no_success_recorded(rig, "A run that failed at a step");
        let names = rig.own_log_names(PROJECT);
        rig.check(
            names.last().is_some_and(|n| n.ends_with(" failed"))
                && !names.iter().any(|n| n.starts_with("place")),
            &format!("The run stops at the step that failed, and never places: {names:?}"),
        );
        let first = rig
            .log()
            .watched("aws", 1, "hosts.json")
            .map(str::to_string);
        if let Some(content) = first {
            rig.check(
                !has_success(&record_in(&content, PROJECT)),
                "Step 3 MUST have removed the success before the first program ran",
            );
        }
    });
}

macro_rules! failed_step_test {
    ($name:ident, $step:expr, $kind:expr) => {
        #[test]
        fn $name() {
            failed_step_case($step, $kind);
        }
    };
}

failed_step_test!(
    a_failed_deploy_of_a_ready_host_records_no_success,
    Step::Deploy,
    Kind::Ready
);
failed_step_test!(
    a_failed_deploy_of_a_failed_host_records_no_success,
    Step::Deploy,
    Kind::Failed
);
failed_step_test!(
    a_failed_start_of_a_ready_host_records_no_success,
    Step::Start,
    Kind::Ready
);
failed_step_test!(
    a_failed_sync_of_a_ready_host_records_no_success,
    Step::Sync,
    Kind::Ready
);
failed_step_test!(
    a_failed_sync_of_a_creating_host_records_no_success,
    Step::Sync,
    Kind::Creating
);
failed_step_test!(
    a_failed_bootstrap_of_a_ready_host_records_no_success,
    Step::Bootstrap,
    Kind::Ready
);
failed_step_test!(
    a_failed_bootstrap_of_a_new_host_records_no_success,
    Step::Bootstrap,
    Kind::NoRecord
);

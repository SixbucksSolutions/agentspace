//! `sync`'s patch wait that never finishes: its 30-minute bound, its exact failure message, and what
//! it leaves a provisioning run (docs/operator.md, The patch wait, step 2 and "When the 30 minutes
//! run out"; sync; provision, "When step 4, 5, 6 or 7 fails"; Calls by command, the waits' timing;
//! D61's changes; E0.08-T03-S05, #0181).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Written from docs/operator.md alone. Each
//! test runs over a fake clock whose pauses take no time, so 30 minutes pass at once, and the
//! stand-in `aws` answers the same, forever, as its script's last check. The clock is the seam
//! `waitcalls` names.
//!
//! **The timing** is `up`'s wait's (Calls by command): the first check is at once, the 5 seconds are a
//! pause after each check that was not ready, the deadline is tested after each such check, and the
//! last pause is cut short to end on it. So the checks are at 0, 5, ..., 1800 seconds: 361 of them,
//! and the failure is the last check's.
#![cfg(windows)]

mod common;
mod hostcalls;
mod waitcalls;

use std::time::Duration;

use agentspace_sync::Target;
use common::hosts::*;
use common::provlog::LogLine;
use common::*;
use hostcalls::setup::*;
use waitcalls::*;

const THIRTY_MINUTES: Duration = Duration::from_secs(30 * 60);
const CHECKS: usize = 361;

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Checks that the clock ran from the wait's start to its 30 minutes and no further: the pauses
/// of the checks sum to exactly 30 minutes, the last cut short to end on the deadline.
fn check_clock(rig: &Rig, clock: &FakeClock) {
    rig.check(
        clock.elapsed() == THIRTY_MINUTES,
        &format!(
            "The wait MUST last 30 minutes from its start, its last pause cut short to end on the deadline; it lasted {:?}",
            clock.elapsed()
        ),
    );
    rig.check(
        clock
            .check_pauses()
            .iter()
            .all(|d| *d <= Duration::from_secs(5)),
        "No pause MAY be longer than 5 seconds",
    );
}

#[test]
fn a_run_that_never_finishes_fails_at_30_minutes_with_its_message_and_runs_no_row_after_4() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_claude_md();
    rig.script_checks(ID, &[Check::Answer("pending")]);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        let f = rig.failure(
            rig.do_sync_clocked(&say, Target::Box, &clock),
            "A patch run that never finishes MUST fail sync",
        );
        rig.same(
            "The failure message",
            &f.message,
            &deadline_message(ID, "agentspace"),
        );
        let log = rig.log();
        check_clock(rig, &clock);
        for op in [
            "describe-instance-information",
            "send-command",
            "get-command-invocation",
        ] {
            let n = ssm_calls(&log, op).len();
            rig.check(
                n == CHECKS,
                &format!("The wait MUST check every 5 seconds for 30 minutes, {CHECKS} checks, and made {n} calls of `{op}`"),
            );
        }
        rig.same_list(
            "The remote calls: rows 5 to 9 are not run",
            &rig.remote_names(&log),
            &names(&["ssh#1", "scp#1", "scp#2", "ssh#2"]),
        );
        // The messages: the first, and the phase's, once each.
        let says: Vec<String> = wait_timeline(&log)
            .into_iter()
            .filter(|l| l.starts_with("say: Checking") || l == PATCHING || l == WAITING || l.starts_with("say: Patched"))
            .collect();
        rig.same_list(
            "The wait's messages: `Checking`, then the patching message once, and no `Patched`",
            &says,
            &[checking(ID), PATCHING.to_string()],
        );
        rig.check(
            say.warnings().is_empty(),
            "The deadline is a failure, never a warning",
        );
    });
}

#[test]
fn a_host_that_never_comes_back_fails_at_30_minutes_with_the_hosts_alias_in_its_message() {
    // Session Manager's agent never comes online again: the wait asks no question, and the same
    // bound ends it.
    let rig = sync_rig(false);
    rig.with_claude_md();
    rig.script_checks(ID2, &[Check::Offline]);
    rig.run(|rig| {
        let alias = alias_of(PROJECT);
        let say = rig.say();
        let clock = FakeClock::new();
        let f = rig.failure(
            rig.do_sync_clocked(&say, Target::Host(PROJECT), &clock),
            "sync -Host",
        );
        rig.same(
            "The failure message: `agentspace` in it is the host's alias",
            &f.message,
            &deadline_message(ID2, &alias),
        );
        let log = rig.log();
        check_clock(rig, &clock);
        rig.check(
            ssm_calls(&log, "describe-instance-information").len() == CHECKS
                && ssm_calls(&log, "send-command").is_empty(),
            "Every check reads the agent and, with it never online, sends no question",
        );
        rig.same_list(
            "The remote calls: rows 5 to 9 are not run",
            &rig.remote_names(&log),
            &names(&["ssh#1", "scp#1", "scp#2", "ssh#2"]),
        );
        let says: Vec<String> = wait_timeline(&log)
            .into_iter()
            .filter(|l| l.starts_with("say: "))
            .filter(|l| !l.starts_with("say: Copying"))
            .collect();
        rig.same_list(
            "The wait's messages: the first, and `Waiting for Session Manager...` once",
            &says,
            &[checking(ID2), WAITING.to_string()],
        );
    });
}

#[test]
fn the_deadline_is_tested_after_a_check_that_was_not_ready_and_a_last_answer_still_counts() {
    // The last check, begun at the deadline, finds `patched`: the wait ends well, and no failure
    // is made of the 30 minutes (a check that began before the deadline and found the host ready
    // still ends the wait).
    let rig = Rig::new(false);
    rig.make_ca();
    // 360 checks that find the agent offline, then the one at the deadline.
    let mut checks = vec![Check::Offline; CHECKS - 1];
    checks.push(Check::Answer("patched"));
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_clocked(&say, Target::Box, &clock),
            "A check at the deadline that finds `patched` MUST end the wait well",
        );
        rig.check(
            clock.elapsed() == THIRTY_MINUTES,
            "The last check is made at the deadline, 30 minutes from the wait's start",
        );
        rig.check(
            say.progress_lines()
                .iter()
                .any(|l| *l == format!("Patched: {ID}")),
            "`Patched` MUST print",
        );
    });
}

// ---------------------------------------------------------------- provision

/// A provisioning run whose wait runs out: step 6's failure, logged, recorded as any failed step is.
fn deadline_case(kind: Kind) {
    let rig = prov_rig(kind);
    rig.script_checks(ID2, &[Check::Offline]);
    rig.run(move |rig| {
        let alias = alias_of(PROJECT);
        let say = rig.say();
        let clock = FakeClock::new();
        let f = rig.failure(
            rig.do_provision_clocked(&say, PROJECT, &clock),
            "provision",
        );
        let message = deadline_message(ID2, &alias);
        rig.same("The run's last line, the step's own failure message", &f.message, &message);
        check_clock(rig, &clock);
        let lines = rig.own_log_lines(PROJECT);
        let names: Vec<String> = lines.iter().map(LogLine::name).collect();
        rig.same_list(
            "The run's own lines up to step 6's failure, and nothing after it",
            &names,
            &names_of(&["run begun", "deploy done", "start done", "sync failed"]),
        );
        let reason = message.strip_prefix("agentspace: ").unwrap_or(&message);
        rig.check(
            lines.last().and_then(|l| l.reason.as_deref()) == Some(reason),
            "The failed line's reason MUST be the wait's message without its `agentspace: ` prefix: `sync failed reason=<id> did not finish its patch run within 30 minutes; check: ...`",
        );
        let record = rig.record(PROJECT);
        let ready = kind == Kind::Ready;
        rig.check(
            record.state.as_str() == if ready { "ready" } else { "failed" },
            &format!(
                "A host that {} ready MUST be recorded {} after the wait runs out; it is {}",
                if ready { "was" } else { "was never" },
                if ready { "ready (left as it was)" } else { "failed" },
                record.state.as_str()
            ),
        );
        rig.check(
            record.instance_id.as_deref() == Some(ID2),
            "The record keeps the instance ID the run learned",
        );
        let log = rig.log();
        rig.same_list(
            "The ssh calls: the removal of the staging folder and the install, and no bootstrap",
            &log.calls("ssh").iter().map(|r| format!("ssh#{}", r.n)).collect::<Vec<_>>(),
            &names_of(&["ssh#1", "ssh#2"]),
        );
        let progress = say.progress_lines();
        rig.check(
            !progress.iter().any(|l| l.starts_with("Recorded the host") || l.starts_with("Sign in on this host")),
            "A failed run MUST NOT print that the host is recorded ready, nor the sign-in pointer",
        );
        rig.check(
            matches!(agentspace_hosts::try_run_lock(&rig.home, PROJECT), Ok(Some(_))),
            "provision.lock MUST be released when the run fails",
        );
    });
}

fn names_of(items: &[&str]) -> Vec<String> {
    names(items)
}

#[test]
fn a_provisioning_run_whose_wait_runs_out_records_a_new_host_failed() {
    deadline_case(Kind::NoRecord);
}

#[test]
fn a_provisioning_run_whose_wait_runs_out_leaves_a_ready_host_ready() {
    deadline_case(Kind::Ready);
}

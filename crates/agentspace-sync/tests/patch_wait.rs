//! `sync`'s patch wait, for the box (docs/operator.md, sync, step 5 and the table of its calls;
//! The patch wait; What up waits for; Calls by command; AWS failures; D61's changes; E0.08-T03-S05,
//! #0181).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Written from docs/operator.md alone. Each
//! test runs `sync` in a child of the test binary under a throwaway home with a throwaway CA, with
//! stand-ins for the AWS CLI, ssh and scp, and a script of the wait's checks for the stand-in `aws`
//! (`waitcalls`). A test whose wait pauses runs over a fake clock, so the 5 seconds between two
//! checks cost nothing; the other tests have `patched` at once, and run over the operator's own
//! clock. The host's own alias, `sync -All`, `provision` and `deploy` are in `patch_wait_hosts`,
//! and the 30-minute bound is in `patch_wait_deadline`.
#![cfg(windows)]

mod common;
mod waitcalls;

use std::time::Duration;

use agentspace_operator_api::{CallerFailure, SIGN_IN_MESSAGE};
use agentspace_sync::Target;
use common::expect::*;
use common::*;
use waitcalls::*;

const FIVE: Duration = Duration::from_secs(5);
const ALIAS: &str = "agentspace";

/// A throwaway home for the box's `sync`: a CA and a `CLAUDE.md`, so rows 5 and 6 run, and no `go`,
/// so the localswim CLI is skipped.
fn box_rig() -> Rig {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_claude_md();
    rig
}

/// `sync`'s own lines and calls, with the wait's put right after the install.
fn expected(rig: &Rig, wait: Want) -> Vec<String> {
    with_wait(expected_timeline(&fingerprint(rig), true, false), wait)
}

/// The ssh and scp calls of a sync that ran every row: the install is `ssh#2`.
const ALL_ROWS: [&str; 6] = ["ssh#1", "scp#1", "scp#2", "ssh#2", "ssh#3", "scp#3"];
/// The ssh and scp calls of a sync that stopped at the wait, after row 4.
const THROUGH_ROW_4: [&str; 4] = ["ssh#1", "scp#1", "scp#2", "ssh#2"];

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------- Its place and its calls

#[test]
fn the_wait_runs_after_row_4_and_before_row_5_with_its_calls_in_order() {
    // The box was launched long ago (the stand-in's launch time is in 2020): `up`'s start asks
    // nothing of it, and the wait asks all the same.
    let rig = box_rig();
    rig.script_checks(ID, &[Check::Answer("patched")]);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&Check::Answer("patched"))
                .line(patched(ID)),
        );
        rig.same_list(
            "The order of sync's calls and messages, the wait between rows 4 and 5",
            &wait_timeline(&log),
            &want,
        );
        rig.check_ssm_args(&log, ID);
        rig.check_no_ec2_after_the_start(&log);
        // The two `ec2 describe-instances` before the wait are the start-and-wait step's, and the
        // wait made none of its own.
        let ec2 = log
            .calls("aws")
            .iter()
            .filter(|r| r.args.first().map(String::as_str) == Some("ec2"))
            .count();
        rig.check(
            ec2 == 2,
            &format!("The wait MUST make no `ec2 describe-instances` of its own; the run made {ec2} ec2 calls, and the start-and-wait step's are 2"),
        );
        rig.check(say.warnings().is_empty(), "A patched host gets no warning");
    });
}

#[test]
fn the_wait_is_not_made_when_row_4_fails() {
    let rig = box_rig();
    rig.rule("ssh", 2, "", 1, "", "");
    rig.script_checks(ID, &[Check::Answer("patched")]);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        rig.same(
            "The failure message",
            &f.message,
            "agentspace: Install on the instance failed.",
        );
        let log = rig.log();
        rig.check(
            ssm_count(&log) == 0,
            "A failed install MUST end sync before the wait: no `ssm` call",
        );
        rig.same_list(
            "What sync did before it failed",
            &wait_timeline(&log),
            &up_to_install(expected_timeline(&fingerprint(rig), true, false)),
        );
        rig.check(
            !say.progress_lines()
                .iter()
                .any(|l| l.starts_with("Checking that ") || l.starts_with("Patched: ")),
            "No message of the wait MUST print when the install failed",
        );
    });
}

#[test]
fn a_host_that_has_finished_its_patch_run_is_not_waited_for() {
    // The first check answers `patched`: no pause, one question, and the rows after it run.
    let rig = box_rig();
    rig.script_checks(ID, &[Check::Answer("patched")]);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let log = rig.log();
        rig.check(
            clock.sleeps().is_empty(),
            "The first check is at once, and `patched` ends the wait with no pause",
        );
        rig.same_list(
            "The wait's calls: one check",
            &wait_timeline(&log)
                .into_iter()
                .filter(|l| l.starts_with("aws "))
                .collect::<Vec<_>>(),
            &Want::new().check(&Check::Answer("patched")).0,
        );
        rig.same_list(
            "The ssh and scp calls",
            &rig.remote_names(&log),
            &names(&ALL_ROWS),
        );
    });
}

// ---------------------------------------------------------------- Waiting

#[test]
fn pending_then_patched_prints_the_patching_message_once_and_patched() {
    let rig = box_rig();
    let checks = [
        Check::Answer("pending"),
        Check::Answer("pending"),
        Check::Answer("patched"),
    ];
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .check(&checks[2])
                .line(patched(ID)),
        );
        rig.same_list(
            "The wait's calls and messages: the patching message once",
            &wait_timeline(&log),
            &want,
        );
        rig.same_list(
            "The pauses: 5 seconds after each check that was not ready, none before the first",
            &clock
                .check_pauses()
                .iter()
                .map(|d| format!("{d:?}"))
                .collect::<Vec<_>>(),
            &[format!("{FIVE:?}"), format!("{FIVE:?}")],
        );
        rig.check(
            say.warnings().is_empty(),
            "No warning for a run that finished",
        );
    });
}

#[test]
fn a_reboot_in_the_middle_of_the_wait_is_waited_through() {
    // Online and pending, then the agent gone, then Online with a command refused (a host that
    // reboots refuses commands while the agent still reads Online), then Online and pending,
    // then patched. Each phase message prints again when the phase changes.
    let rig = box_rig();
    let checks = [
        Check::Answer("pending"),
        Check::Offline,
        Check::SendFails,
        Check::Answer("pending"),
        Check::Answer("patched"),
    ];
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .line(WAITING)
                .check(&checks[2])
                .check(&checks[3])
                .line(PATCHING)
                .check(&checks[4])
                .line(patched(ID)),
        );
        rig.same_list(
            "The wait's calls and messages through a reboot",
            &wait_timeline(&log),
            &want,
        );
        rig.same_list(
            "The ssh and scp calls: rows 5 and 6 ran after the wait",
            &rig.remote_names(&log),
            &names(&ALL_ROWS),
        );
        rig.check(
            clock.check_pauses() == vec![FIVE; 4],
            "Four checks were not ready, each followed by a pause of 5 seconds",
        );
        rig.check(say.warnings().is_empty(), "A reboot is no warning");
    });
}

/// A check that is not ready, then `patched`: the wait goes on, prints the patching message
/// (`Waiting` for a check that never reached Session Manager's answer), and ends at `patched`.
fn keeps_waiting(first: Check) {
    let rig = box_rig();
    let checks = [first, Check::Answer("patched")];
    rig.script_checks(ID, &checks);
    rig.run(move |rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .line(patched(ID)),
        );
        rig.same_list(
            "A check that is not ready MUST keep the wait going, and `patched` ends it",
            &wait_timeline(&log),
            &want,
        );
        rig.check(
            clock.check_pauses() == vec![FIVE],
            "One check was not ready, and it was followed by a pause of 5 seconds",
        );
        rig.check(
            say.warnings().is_empty(),
            "A check that is not ready is no warning",
        );
    });
}

#[test]
fn an_unknown_answer_keeps_it_waiting() {
    // `up` lets `unknown` through; the wait does not (What up waits for).
    keeps_waiting(Check::Answer("unknown"));
}

#[test]
fn an_empty_answer_keeps_it_waiting() {
    keeps_waiting(Check::Answer(""));
}

#[test]
fn an_invocation_with_no_output_keeps_it_waiting() {
    keeps_waiting(Check::NoOutput);
}

#[test]
fn an_unexpected_answer_keeps_it_waiting() {
    keeps_waiting(Check::Answer("something else entirely"));
}

#[test]
fn an_invocation_that_ended_in_failed_keeps_it_waiting() {
    keeps_waiting(Check::Invocation("Failed", ""));
}

#[test]
fn an_invocation_that_ended_in_failed_with_the_output_failed_keeps_it_waiting() {
    // A status other than `Success` is a check that was not ready, whatever it printed: the word
    // `failed` ends the wait only as the answer of an invocation that succeeded.
    keeps_waiting(Check::Invocation("Failed", "failed"));
}

#[test]
fn an_invocation_that_ended_in_failed_with_the_output_patched_keeps_it_waiting() {
    keeps_waiting(Check::Invocation("Failed", "patched"));
}

#[test]
fn an_invocation_that_timed_out_keeps_it_waiting() {
    keeps_waiting(Check::Invocation("TimedOut", "patched"));
}

#[test]
fn twenty_polls_without_an_end_keep_it_waiting() {
    let rig = box_rig();
    let checks = [Check::NoEnd, Check::Answer("patched")];
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .line(patched(ID)),
        );
        rig.same_list(
            "The invocation is polled 20 times, and 20 polls without an end are a check that was not ready",
            &wait_timeline(&log),
            &want,
        );
        rig.check(
            ssm_calls(&log, "get-command-invocation").len() == 21,
            "20 polls of the first invocation and one of the second",
        );
        rig.check(
            clock.check_pauses() == vec![FIVE],
            "One check was not ready, and it was followed by a pause of 5 seconds",
        );
    });
}

#[test]
fn a_ready_answer_is_read_trimmed() {
    // The answer is the invocation's output, trimmed: the stand-in's has a line feed after it.
    let rig = box_rig();
    rig.script_checks(ID, &[Check::Answer("patched")]);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync(&say), "sync");
        rig.check(
            say.progress_lines()
                .iter()
                .any(|l| *l == format!("Patched: {ID}")),
            "`patched` and a line feed MUST be read as `patched`",
        );
    });
}

// ---------------------------------------------------------------- A patch run that failed

#[test]
fn a_failed_answer_after_pending_ends_the_wait_with_one_warning_and_sync_goes_on() {
    let rig = box_rig();
    let checks = [Check::Answer("pending"), Check::Answer("failed")];
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_clocked(&say, Target::Box, &clock),
            "A failed patch run MUST NOT fail sync",
        );
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .line(failed_warning(ID, ALIAS)),
        );
        rig.same_list(
            "The wait ends at `failed`, with its warning and no `Patched` line, and rows 5 to 9 run",
            &wait_timeline(&log),
            &want,
        );
        rig.same_list(
            "The ssh and scp calls: every row ran",
            &rig.remote_names(&log),
            &names(&ALL_ROWS),
        );
        rig.same_list(
            "The warnings: exactly one, on the failed run",
            &say.warnings(),
            &[failed_warning_text(ID, ALIAS)],
        );
        rig.check(
            ssm_count(&log) == 6,
            "No `ssm` call MAY follow the `failed` answer: two checks of three calls",
        );
        rig.check(
            clock.check_pauses() == vec![FIVE],
            "One pause, after the check that answered `pending`, and none after `failed`",
        );
    });
}

#[test]
fn a_failed_answer_as_the_first_answer_ends_the_wait_at_once() {
    let rig = box_rig();
    let checks = [Check::Answer("failed")];
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(failed_warning(ID, ALIAS)),
        );
        rig.same_list(
            "The wait ends at its first answer, `failed`",
            &wait_timeline(&log),
            &want,
        );
        rig.check(
            clock.sleeps().is_empty(),
            "A first answer of `failed` ends the wait with no pause",
        );
        rig.same_list(
            "The warning, once, with the box's alias",
            &say.warnings(),
            &[failed_warning_text(ID, ALIAS)],
        );
        rig.same_list(
            "Rows 5 to 9 run",
            &rig.remote_names(&log),
            &names(&ALL_ROWS),
        );
    });
}

#[test]
fn a_failed_answer_after_a_reboot_ends_the_wait_too() {
    let rig = box_rig();
    let checks = [
        Check::Answer("pending"),
        Check::Offline,
        Check::Answer("failed"),
    ];
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .line(WAITING)
                .check(&checks[2])
                .line(failed_warning(ID, ALIAS)),
        );
        rig.same_list("The wait", &wait_timeline(&log), &want);
    });
}

#[test]
fn a_failed_patch_run_does_not_hide_a_later_rows_failure() {
    // Row 6, the copy of CLAUDE.md, fails: sync exits 1 with that row's message, which is its last,
    // after the warning of the patch run.
    let rig = box_rig();
    rig.rule("scp", 3, "", 1, "", "");
    rig.script_checks(ID, &[Check::Answer("failed")]);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        rig.same(
            "The failure message",
            &f.message,
            "agentspace: Copying CLAUDE.md to the instance failed.",
        );
        rig.same_list(
            "The warning came before the failure, once",
            &say.warnings(),
            &[failed_warning_text(ID, ALIAS)],
        );
    });
}

#[test]
fn a_failed_answer_is_withheld_by_quiet_and_the_exit_code_is_the_same() {
    // `-Quiet` suppresses the warning (the operator's `Say`, which records nothing here) and does
    // not change what sync does: the same calls, and success.
    let rig = box_rig();
    rig.script_checks(ID, &[Check::Answer("failed")]);
    rig.run(|rig| {
        let say = rig.quiet_say();
        rig.succeeded(rig.do_sync(&say), "A quiet sync of a failed patch run");
        let log = rig.log();
        rig.same_list(
            "The ssh and scp calls: every row ran",
            &rig.remote_names(&log),
            &names(&ALL_ROWS),
        );
        rig.check(
            ssm_count(&log) == 3,
            "One check of three calls, and no call after `failed`",
        );
        rig.check(
            say.warnings().is_empty() && say.progress_lines().is_empty(),
            "A quiet run says nothing",
        );
    });
}

// ---------------------------------------------------------------- Failures that end sync

#[test]
fn an_expired_sign_in_at_send_command_fails_sync_at_once() {
    let rig = box_rig();
    rig.script_checks(ID, &[Check::SendSignIn]);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        rig.same("The failure message", &f.message, SIGN_IN_MESSAGE);
        rig.check(
            f.caller == CallerFailure::Same,
            "The sign-in message is the caller's line too",
        );
        let log = rig.log();
        rig.same_list(
            "Nothing runs after the failed send: not row 5, and no second check",
            &wait_timeline(&log)
                .into_iter()
                .filter(|l| l.starts_with("aws ") || l.starts_with("ssh#") || l.starts_with("scp#"))
                .collect::<Vec<_>>(),
            &["ssh#1", "scp#1", "scp#2", "ssh#2", DESCRIBE, SEND].map(String::from),
        );
    });
}

#[test]
fn an_expired_sign_in_in_the_middle_of_the_wait_fails_sync_at_once_too() {
    // A refused send is a check that was not ready; the sign-in's failure is the one exception.
    let rig = box_rig();
    rig.script_checks(ID, &[Check::Answer("pending"), Check::SendSignIn]);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        let f = rig.failure(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        rig.same("The failure message", &f.message, SIGN_IN_MESSAGE);
        let log = rig.log();
        rig.same_list(
            "The remote calls: none after the install",
            &rig.remote_names(&log),
            &names(&THROUGH_ROW_4),
        );
        rig.check(
            ssm_count(&log) == 5,
            "Three calls of the first check and two of the second, and none after the failure",
        );
        rig.check(
            say.warnings().is_empty(),
            "A sign-in failure is a failure, not a warning",
        );
    });
}

#[test]
fn an_expired_sign_in_at_describe_instance_information_fails_sync_with_its_message() {
    // AWS failures: every call whose failure fails the command fails with #0034's message when the
    // CLI's text says the sign-in has expired.
    let rig = box_rig();
    rig.script_checks(ID, &[Check::DescribeSignIn]);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        rig.same("The failure message", &f.message, SIGN_IN_MESSAGE);
        let log = rig.log();
        rig.same_list(
            "The remote calls: none after the install",
            &rig.remote_names(&log),
            &names(&THROUGH_ROW_4),
        );
    });
}

#[test]
fn a_failed_describe_instance_information_fails_sync() {
    let rig = box_rig();
    rig.script_checks(ID, &[Check::DescribeFails]);
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync(&say), "sync");
        rig.same(
            "The failure message: `agentspace: ` and the CLI's own text",
            &f.message,
            &format!("agentspace: {DESCRIBE_REFUSED}"),
        );
        let log = rig.log();
        rig.same_list(
            "The remote calls: none after the install",
            &rig.remote_names(&log),
            &names(&THROUGH_ROW_4),
        );
        rig.check(
            ssm_count(&log) == 1,
            "The failed describe is the one `ssm` call, and no send follows it",
        );
    });
}

#[test]
fn a_failed_describe_instance_information_in_the_middle_of_the_wait_fails_sync() {
    let rig = box_rig();
    rig.script_checks(ID, &[Check::Answer("pending"), Check::DescribeFails]);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        let f = rig.failure(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        rig.same(
            "The failure message",
            &f.message,
            &format!("agentspace: {DESCRIBE_REFUSED}"),
        );
        let log = rig.log();
        rig.same_list(
            "The remote calls: none after the install",
            &rig.remote_names(&log),
            &names(&THROUGH_ROW_4),
        );
    });
}

#[test]
fn a_refused_send_command_that_is_not_a_sign_in_failure_is_a_check_not_ready() {
    // The first check's send is refused, and the second goes through to `patched`.
    let rig = box_rig();
    let checks = [Check::SendFails, Check::Answer("patched")];
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let log = rig.log();
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(WAITING)
                .check(&checks[1])
                .line(patched(ID)),
        );
        rig.same_list(
            "A refused send is a check that was not ready, with the message `Waiting for Session Manager...`",
            &wait_timeline(&log),
            &want,
        );
    });
}

#[test]
fn an_agent_with_an_empty_list_is_not_online() {
    let rig = box_rig();
    let checks = [Check::EmptyList, Check::Answer("patched")];
    rig.script_checks(ID, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(rig.do_sync_clocked(&say, Target::Box, &clock), "sync");
        let want = expected(
            rig,
            Want::new()
                .line(checking(ID))
                .check(&checks[0])
                .line(WAITING)
                .check(&checks[1])
                .line(patched(ID)),
        );
        rig.same_list("The wait", &wait_timeline(&rig.log()), &want);
    });
}

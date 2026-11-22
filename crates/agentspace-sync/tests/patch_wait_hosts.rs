//! `sync`'s patch wait for a host that is not the box, for `sync -All`, and in `provision` and
//! `deploy`, whose step 6 is `sync` (docs/operator.md, The patch wait; sync; Every host at once;
//! provision, steps 6 and 10 and Its log; deploy, step 6; AWS failures; Calls by command; D61's
//! changes; E0.08-T03-S05, #0181).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Written from docs/operator.md alone. Each
//! test runs the crate's hidden functions in a child of the test binary, under a throwaway home with
//! a throwaway CA, with stand-ins for the AWS CLI, ssh and scp, and a script of the wait's checks
//! for the stand-in `aws` (`waitcalls`). The box's own wait is in `patch_wait`, and the 30-minute
//! bound in `patch_wait_deadline`.
#![cfg(windows)]

mod common;
mod hostcalls;
mod waitcalls;

use agentspace_operator_api::SIGN_IN_MESSAGE;
use agentspace_sync::Target;
use common::expect::*;
use common::hosts::*;
use common::provlog::{LogLine, PATCH_FAILED_LINE};
use common::*;
use hostcalls::setup::*;
use waitcalls::*;

/// A third host's instance, made up (it MUST NOT match the identifier scan).
const ID3: &str = "i-abcdef12345678901";

// ---------------------------------------------------------------- A host's own alias

#[test]
fn a_failed_answer_names_the_hosts_alias_in_its_warning_and_sync_goes_on() {
    let rig = sync_rig(false);
    rig.with_claude_md();
    let checks = [Check::Answer("pending"), Check::Answer("failed")];
    rig.script_checks(ID2, &checks);
    rig.run(|rig| {
        let alias = alias_of(PROJECT);
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_clocked(&say, Target::Host(PROJECT), &clock),
            "A failed patch run MUST NOT fail sync -Host",
        );
        let log = rig.log();
        let want = with_wait(
            expected_timeline(&fingerprint(rig), true, false),
            Want::new()
                .line(checking(ID2))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .line(failed_warning(ID2, &alias)),
        );
        rig.same_list("The wait", &wait_timeline(&log), &want);
        rig.same_list(
            "The warning: `agentspace` in the command is the host's alias",
            &say.warnings(),
            &[failed_warning_text(ID2, &alias)],
        );
        rig.check_ssm_args(&log, ID2);
        rig.check_no_ec2_after_the_start(&log);
        for c in log.calls("ssh") {
            rig.check(
                c.args.contains(&alias),
                "Every ssh of the run, the rows after the wait included, MUST go over the host's alias",
            );
        }
        rig.check(
            log.calls("ssh").len() == 3 && log.calls("scp").len() == 3,
            "Rows 5 to 9 MUST run after the failed answer: three ssh and three scp",
        );
    });
}

#[test]
fn a_host_that_is_not_the_box_gets_the_same_wait() {
    let rig = sync_rig(false);
    rig.with_claude_md();
    let checks = [Check::Answer("pending"), Check::Answer("patched")];
    rig.script_checks(ID2, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_clocked(&say, Target::Host(PROJECT), &clock),
            "sync -Host",
        );
        let want = with_wait(
            expected_timeline(&fingerprint(rig), true, false),
            Want::new()
                .line(checking(ID2))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .line(patched(ID2)),
        );
        rig.same_list("The wait", &wait_timeline(&rig.log()), &want);
    });
}

// ---------------------------------------------------------------- sync -All

/// The five records, in the file's order: the box, two `ready` hosts and two that are not.
fn all_rig() -> Rig {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.beside_operator(true);
    rig.write_home(
        ".agentspace/hosts.json",
        format!(
            "{{\"version\":0,\"hosts\":[\
             {{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"{ID}\"}},\
             {{\"project\":\"demo\",\"stack\":\"agentspace-host-demo\",\"state\":\"ready\",\"instance_id\":\"{ID2}\"}},\
             {{\"project\":\"beta\",\"stack\":\"agentspace-host-beta\",\"state\":\"creating\"}},\
             {{\"project\":\"gamma\",\"stack\":\"agentspace-host-gamma\",\"state\":\"failed\",\"last_attempt_at\":\"2026-11-20T10:00:00Z\"}},\
             {{\"project\":\"delta\",\"stack\":\"agentspace-host-delta\",\"state\":\"ready\",\"instance_id\":\"{ID3}\"}}]}}\n"
        )
        .as_bytes(),
    );
    rig
}

/// How many `ssm` calls of `op` named `id`.
fn ssm_for(log: &Log, op: &str, id: &str) -> usize {
    ssm_calls(log, op)
        .iter()
        .filter(|r| r.args.iter().any(|a| a.contains(id)))
        .count()
}

#[test]
fn sync_all_waits_for_each_host_in_its_own_part_and_a_failed_run_is_not_a_failed_host() {
    let rig = all_rig();
    rig.script_checks(ID, &[Check::Answer("patched")]);
    rig.script_checks(ID2, &[Check::Answer("pending"), Check::Answer("failed")]);
    rig.script_checks(ID3, &[Check::Answer("patched")]);
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_all_clocked(&say, &clock),
            "A host whose patch run failed MUST NOT fail sync -All",
        );
        let log = rig.log();
        for (op, id, n) in [
            ("describe-instance-information", ID, 1),
            ("send-command", ID, 1),
            ("get-command-invocation", ID, 1),
            ("describe-instance-information", ID2, 2),
            ("send-command", ID2, 2),
            ("get-command-invocation", ID2, 2),
            ("describe-instance-information", ID3, 1),
            ("send-command", ID3, 1),
            ("get-command-invocation", ID3, 1),
        ] {
            let got = ssm_for(&log, op, id);
            rig.check(
                got == n,
                &format!("Each host MUST wait in its own part: {n} calls of `{op}` for one host, and {got} were made"),
            );
        }
        rig.same_list(
            "The warnings: one, for the host whose run failed, with its alias",
            &say.warnings(),
            &[failed_warning_text(ID2, &alias_of("demo"))],
        );
        let progress = say.progress_lines();
        for id in [ID, ID2, ID3] {
            rig.check(
                progress.iter().any(|l| *l == format!("Checking that {id} has finished this boot's patch run...")),
                "Each host's part MUST print its own `Checking that` line",
            );
        }
        rig.check(
            !progress.iter().any(|l| *l == format!("Patched: {ID2}")),
            "No `Patched` line for the host whose run failed",
        );
        rig.check(
            progress.iter().filter(|l| l.starts_with("Patched: ")).count() == 2,
            "`Patched` for the two hosts that finished their runs",
        );
    }) else {
        return;
    };
    assert!(
        !out.stderr.contains("hosts failed."),
        "A host whose patch run failed MUST NOT be counted in the closing count"
    );
}

#[test]
fn sync_all_ends_at_once_at_the_sign_in_failure_of_a_hosts_wait() {
    let rig = all_rig();
    rig.script_checks(ID, &[Check::Answer("patched")]);
    rig.script_checks(ID2, &[Check::SendSignIn]);
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        let f = rig.failure(rig.do_sync_all_clocked(&say, &clock), "sync -All");
        rig.same("The message", &f.message, SIGN_IN_MESSAGE);
        let log = rig.log();
        rig.check(
            !log.all_calls().iter().any(|c| {
                c.args
                    .iter()
                    .any(|a| a.contains(ID3) || a.contains(&alias_of("delta")))
            }),
            "No host after the sign-in failure MUST be reached",
        );
    }) else {
        return;
    };
    assert!(
        out.stderr.contains("Host      demo ") && !out.stderr.contains("Host      delta "),
        "No Host line MUST follow the sign-in failure's host"
    );
}

#[test]
fn sync_all_counts_a_host_whose_describe_failed_and_goes_on_to_the_next() {
    let rig = all_rig();
    rig.script_checks(ID, &[Check::Answer("patched")]);
    rig.script_checks(ID2, &[Check::DescribeFails]);
    rig.script_checks(ID3, &[Check::Answer("patched")]);
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        let f = rig.failure(rig.do_sync_all_clocked(&say, &clock), "sync -All");
        rig.same(
            "The closing message: one of five hosts failed",
            &f.message,
            "agentspace: 1 of 5 hosts failed.",
        );
        let log = rig.log();
        rig.check(
            ssm_for(&log, "describe-instance-information", ID3) == 1,
            "The host after the one whose describe failed MUST be synced",
        );
    }) else {
        return;
    };
    assert!(
        out.stderr
            .lines()
            .any(|l| l == format!("agentspace: {DESCRIBE_REFUSED}")),
        "The failed host's message MUST be printed"
    );
}

// ---------------------------------------------------------------- provision

/// The ssh, scp and `ssm` calls of a run up to its third ssh, as `ssh#1` and `aws ssm send-command`.
fn calls_through_third_ssh(log: &Log) -> Vec<String> {
    let mut out = Vec::new();
    let mut ssh = 0;
    for i in &log.items {
        if let Item::Call(r) = i {
            if r.name == "aws" && r.args.first().map(String::as_str) == Some("ssm") {
                out.push(format!(
                    "aws ssm {}",
                    r.args.get(1).cloned().unwrap_or_default()
                ));
            } else if r.name == "ssh" || r.name == "scp" {
                out.push(format!("{}#{}", r.name, r.n));
                if r.name == "ssh" {
                    ssh += 1;
                    if ssh == 3 {
                        break;
                    }
                }
            }
        }
    }
    out
}

/// The text of the run's own lines in the log, with each line's time cut off.
fn log_texts(rig: &Rig) -> Vec<String> {
    rig.provision_log(PROJECT)
        .unwrap_or_default()
        .lines()
        .filter(|l| common::provlog::parse_log_line(l).is_some())
        .map(|l| l[21..].to_string())
        .collect()
}

#[test]
fn provisions_step_6_makes_the_waits_calls_after_the_install_and_before_the_bootstrap() {
    let rig = prov_rig(Kind::NoRecord);
    rig.script_checks(ID2, &[Check::Answer("patched")]);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let log = rig.log();
        rig.same_list(
            "sync's install, then the wait's calls, then the bootstrap's ssh",
            &calls_through_third_ssh(&log),
            &["ssh#1", "scp#1", "scp#2", "ssh#2", DESCRIBE, SEND, GET, "ssh#3"].map(String::from),
        );
        rig.check_ssm_args(&log, ID2);
        // The messages of the wait come within step 6: after sync's copy, before the bootstrap's.
        let progress = say.progress_lines();
        let at = |text: &str| progress.iter().position(|l| l == text);
        let copying = at("Copying box/system to the instance...");
        let checking_at = at(&format!("Checking that {ID2} has finished this boot's patch run..."));
        let patched_at = at(&format!("Patched: {ID2}"));
        let bootstrap = at("Running the toolchain bootstrap on the instance...");
        rig.check(
            copying < checking_at && checking_at < patched_at && patched_at < bootstrap,
            &format!("The wait's messages MUST come between sync's copy and the bootstrap's message: {copying:?} {checking_at:?} {patched_at:?} {bootstrap:?}"),
        );
        // A run whose patch run finished logs a plain `sync done`, with nothing after it.
        let texts = log_texts(rig);
        rig.check(
            texts.iter().any(|t| t == "sync done") && !texts.iter().any(|t| t.contains("patch run")),
            "A patched host's step 6 line MUST be `sync done` alone",
        );
    });
}

/// A provisioning run whose patch wait ends on `failed`: the step's line says so, the host is
/// recorded `ready`, and the run exits 0.
fn failed_patch_run_case(kind: Kind, checks: Vec<Check>) {
    let rig = prov_rig(kind);
    rig.script_checks(ID2, &checks);
    rig.run(move |rig| {
        let alias = alias_of(PROJECT);
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_provision_clocked(&say, PROJECT, &clock),
            "A failed patch run MUST NOT fail a provisioning run: step 6 is sync exiting 0",
        );
        let lines = rig.own_log_lines(PROJECT);
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
                && names[7] == "run done",
            &format!("The run MUST go on through every step; its own lines were {names:?}"),
        );
        rig.check(
            lines.iter().map(|l| l.patch_failed).collect::<Vec<_>>()
                == [false, false, false, true, false, false, false, false],
            "Step 6's line alone MUST carry the patch run's failure",
        );
        rig.check(
            log_texts(rig)
                .iter()
                .any(|t| t == PATCH_FAILED_LINE),
            "Step 6's line MUST be `<time> sync done; the patch run failed, see the host's journal`, exactly, with no other field",
        );
        let record = rig.record(PROJECT);
        rig.check(
            record.state.as_str() == "ready" && record.instance_id.as_deref() == Some(ID2),
            "The host MUST be recorded `ready`, with its instance ID",
        );
        let ours: Vec<String> = say
            .warnings()
            .into_iter()
            .filter(|w| w.contains("The patch run on "))
            .collect();
        rig.same_list(
            "The warning of the failed patch run, once, with the host's alias",
            &ours,
            &[failed_warning_text(ID2, &alias)],
        );
        let progress = say.progress_lines();
        rig.check(
            !progress.iter().any(|l| l.starts_with("Patched: ")),
            "No `Patched` line for a failed run",
        );
        rig.check(
            progress.iter().any(|l| l == "Running the toolchain bootstrap on the instance..."),
            "The bootstrap MUST run after a failed patch run",
        );
        // The last line of a run for a host that was not ready is the sign-in pointer.
        let last = say.events.borrow().last().cloned();
        if kind == Kind::Ready {
            rig.check(
                !progress.iter().any(|l| l.starts_with("Sign in on this host")),
                "A host that was ready has no sign-in line",
            );
        } else {
            rig.check(
                last.is_some_and(|(k, t)| k == "progress" && t == sign_in_line(PROJECT)),
                "A host that was not ready ends its run with the sign-in pointer",
            );
        }
    });
}

#[test]
fn a_failed_patch_run_is_logged_in_steps_6_line_and_the_run_goes_on() {
    failed_patch_run_case(Kind::NoRecord, vec![Check::Answer("failed")]);
}

#[test]
fn a_failed_patch_run_after_pending_is_logged_the_same_way() {
    failed_patch_run_case(
        Kind::NoRecord,
        vec![Check::Answer("pending"), Check::Answer("failed")],
    );
}

#[test]
fn a_failed_patch_run_of_a_ready_host_leaves_it_ready() {
    failed_patch_run_case(Kind::Ready, vec![Check::Answer("failed")]);
}

#[test]
fn the_new_last_line_of_a_run_is_the_sign_in_pointer_byte_for_byte() {
    // A host that was not `ready` ends its run with exactly this line (pin P20), which sends a
    // person to take a lease, and no longer to `status -Host`.
    let rig = prov_rig(Kind::NoRecord);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        let last = say.events.borrow().last().cloned();
        let want = "Sign in on this host once, by hand, under a lease: see \"Signing in on a new host\" in the agentspace README.";
        rig.check(
            last.as_ref().is_some_and(|(k, t)| k == "progress" && t == want),
            "The run's last line MUST be the sign-in pointer, exactly",
        );
        rig.check(
            !say.progress_lines().iter().any(|l| l.contains("agentspace-operator status")),
            "The pointer MUST NOT send a person to `status -Host` first",
        );
    });
}

// ---------------------------------------------------------------- deploy

#[test]
fn deploys_step_6_makes_the_waits_calls_after_its_install() {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    // sync starts the box the new stack's output names, `ID2`.
    rig.script_checks(ID2, &[Check::Answer("patched")]);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_deploy(&say), "deploy");
        let log = rig.log();
        let want = with_wait(
            expected_timeline(&fingerprint(rig), false, false),
            Want::new()
                .line(checking(ID2))
                .check(&Check::Answer("patched"))
                .line(patched(ID2)),
        );
        rig.same_list(
            "sync's rows, the wait between rows 4 and 5",
            &wait_timeline(&log),
            &want,
        );
        rig.check_ssm_args(&log, ID2);
    });
}

#[test]
fn deploy_for_a_host_makes_the_waits_calls_after_its_install() {
    let rig = host_rig(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID)]);
    rig.stack_status("CREATE_COMPLETE");
    rig.script_checks(ID2, &[Check::Answer("patched")]);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_deploy_target(&say, Target::Host(PROJECT)),
            "deploy -Host",
        );
        let log = rig.log();
        let want = with_wait(
            expected_timeline(&fingerprint(rig), false, false),
            Want::new()
                .line(checking(ID2))
                .check(&Check::Answer("patched"))
                .line(patched(ID2)),
        );
        rig.same_list(
            "sync's rows, the wait between rows 4 and 5",
            &wait_timeline(&log),
            &want,
        );
        rig.check_ssm_args(&log, ID2);
    });
}

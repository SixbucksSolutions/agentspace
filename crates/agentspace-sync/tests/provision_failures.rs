//! `provision`'s step failures: what each step's failure logs, prints and records, for a host that
//! was never `ready` and for one that was (docs/operator.md, provision, "When step 4, 5, 6 or 7
//! fails"; deploy, the existence check; AWS failures; docs/state.md, Its states).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each case runs the crate's hidden
//! `provision_using` in a child of the test binary, under a throwaway home, with one stand-in call
//! made to fail. The first line of the log's `reason` is the failure's message without its
//! `agentspace: ` prefix. The refusals before a step, the locks and the log are in
//! `provision_refusals`.
#![cfg(windows)]

mod common;
mod hostcalls;

use agentspace_operator_api::SIGN_IN_MESSAGE;
use common::hosts::*;
use common::times::{now_secs, parse_utc};
use common::*;
use hostcalls::setup::*;

/// The four steps that can fail the run (operator.md, provision, steps 4 to 7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Deploy,
    Start,
    Sync,
    Bootstrap,
}

impl Step {
    /// Makes this step's one program call fail.
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
            // The first ssh is sync's removal of the staging folder, and the third, with sync's
            // install between, is the bootstrap's: no CLAUDE.md, no go and no push here.
            Step::Sync => rig.rule("ssh", 1, "", 1, "", ""),
            Step::Bootstrap => rig.rule("ssh", 3, "", 1, "", ""),
        }
    }

    fn message(self) -> &'static str {
        match self {
            Step::Deploy => "agentspace: Stack deploy failed; see the stack events in the console.",
            Step::Start => SIGN_IN_MESSAGE,
            Step::Sync => "agentspace: Removing the old staging folder on the instance failed.",
            Step::Bootstrap => "agentspace: The toolchain bootstrap failed on the instance.",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Step::Deploy => "deploy",
            Step::Start => "start",
            Step::Sync => "sync",
            Step::Bootstrap => "bootstrap",
        }
    }

    /// The run's own lines up to and including this step's failure.
    fn lines(self) -> Vec<String> {
        let all = ["deploy", "start", "sync", "bootstrap"];
        let at = all.iter().position(|n| *n == self.name()).unwrap_or(0);
        let mut v = vec!["run begun".to_string()];
        v.extend(all[..at].iter().map(|n| format!("{n} done")));
        v.push(format!("{} failed", self.name()));
        v
    }

    /// The instance ID a record holds after this step failed: the stack's, once the deploy step
    /// has written it (operator.md, deploy, step 5), and what the record held before otherwise.
    fn learned(self) -> bool {
        self != Step::Deploy
    }
}

/// Runs a provisioning run with `step` failing, for a host as `kind` says, and checks everything the
/// failure leaves.
fn failure_case(step: Step, kind: Kind) {
    let rig = prov_rig_with(kind, |rig| step.inject(rig));
    rig.run(move |rig| {
        let box_before = rig.record("swimcoach");
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        rig.same("The failure's message", &f.message, step.message());
        let log = rig.log();

        // The log: begun, done for each step reached, then this step's failure, with its message as
        // the reason, and nothing after it.
        let lines = rig.own_log_lines(PROJECT);
        let names: Vec<String> = lines.iter().map(|l| l.name()).collect();
        rig.same_list("The run's own lines", &names, &step.lines());
        let reason = f.message.strip_prefix("agentspace: ").unwrap_or(&f.message);
        rig.check(
            lines.last().and_then(|l| l.reason.as_deref()) == Some(reason),
            "The failed line's reason MUST be the step's failure message without its `agentspace: ` prefix",
        );

        // The record: `failed` for a host never ready, left `ready` for one that was; the attempt
        // is the one step 3 wrote.
        let first = record_in(log.watched("aws", 1, "hosts.json").unwrap_or(""), PROJECT);
        let attempt = attempt_in(&first);
        rig.check(
            attempt.as_deref().and_then(parse_utc).is_some_and(|t| (now_secs() - t).abs() < 600),
            &format!("Step 3 MUST have written the run's start time: {first}"),
        );
        let record = rig.record(PROJECT);
        let ready = kind == Kind::Ready;
        rig.check(
            record.state.as_str() == if ready { "ready" } else { "failed" },
            &format!(
                "After a failed step a host that {} ready MUST be recorded {}, and it is {}",
                if ready { "was" } else { "was never" },
                if ready { "ready (left as it was)" } else { "failed" },
                record.state.as_str()
            ),
        );
        rig.check(
            record.last_attempt_at == attempt && attempt.as_deref() != Some(OLD_ATTEMPT),
            "last_attempt_at MUST stay what step 3 wrote",
        );
        let want_id = if step.learned() {
            Some(ID2)
        } else if ready {
            Some(ID)
        } else {
            None
        };
        rig.check(
            record.instance_id.as_deref() == want_id,
            &format!(
                "The record's instance ID MUST be {want_id:?} (the one the run learned, kept), and it is {:?}",
                record.instance_id
            ),
        );
        rig.check(
            rig.record("swimcoach") == box_before,
            "The box's record MUST be left as it was",
        );

        // What the run did not do after the failure, and what it released.
        let ssh: Vec<String> = log.calls("ssh").iter().map(|r| format!("ssh#{}", r.n)).collect();
        match step {
            Step::Deploy | Step::Start => rig.check(
                log.remote_calls().is_empty(),
                "No ssh or scp MUST follow a failed deploy or start",
            ),
            Step::Sync => rig.same_list("The ssh calls", &ssh, &svec(&["ssh#1"])),
            Step::Bootstrap => rig.same_list("The ssh calls", &ssh, &svec(&["ssh#1", "ssh#2", "ssh#3"])),
        }
        let progress = say.progress_lines();
        rig.check(
            !progress.iter().any(|l| l.starts_with("Recorded the host") || l.starts_with("Sign in on this host")),
            "A failed run MUST NOT print that the host is recorded ready, nor the sign-in line",
        );
        rig.check(
            matches!(agentspace_hosts::try_run_lock(&rig.home, PROJECT), Ok(Some(_))),
            "provision.lock MUST be released when the run fails",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The run's folder MUST be removed as the run ends, after a failure too",
        );
    });
}

#[test]
fn a_failed_deploy_records_a_new_host_failed() {
    failure_case(Step::Deploy, Kind::NoRecord);
}

#[test]
fn a_failed_deploy_leaves_a_ready_host_ready() {
    failure_case(Step::Deploy, Kind::Ready);
}

#[test]
fn a_failed_start_records_a_new_host_failed_with_the_sign_in_message_as_its_reason() {
    failure_case(Step::Start, Kind::NoRecord);
}

#[test]
fn a_failed_start_leaves_a_ready_host_ready() {
    failure_case(Step::Start, Kind::Ready);
}

#[test]
fn a_failed_sync_records_a_new_host_failed_and_keeps_the_instance_id() {
    failure_case(Step::Sync, Kind::NoRecord);
}

#[test]
fn a_failed_sync_leaves_a_ready_host_ready() {
    failure_case(Step::Sync, Kind::Ready);
}

#[test]
fn a_failed_bootstrap_records_a_new_host_failed_and_keeps_the_instance_id() {
    failure_case(Step::Bootstrap, Kind::NoRecord);
}

#[test]
fn a_failed_bootstrap_leaves_a_ready_host_ready() {
    failure_case(Step::Bootstrap, Kind::Ready);
}

#[test]
fn a_failed_sync_of_a_creating_host_records_it_failed() {
    failure_case(Step::Sync, Kind::Creating);
}

#[test]
fn a_failed_bootstrap_of_a_failed_host_records_it_failed_again() {
    failure_case(Step::Bootstrap, Kind::Failed);
}

// ---------------------------------------------------------------- The existence check's statuses

/// A stack whose existence check answers `status` fails the deploy step, with `message`, before any
/// `cloudformation deploy`, and the run records it as any failed step is recorded.
fn status_case(kind: Kind, status: &'static str, message: String) {
    let rig = prov_rig_with(kind, |rig| {
        rig.rule(
            "aws",
            1,
            "describe-stacks",
            0,
            &json_stack_status(status, None),
            "",
        )
    });
    rig.run(move |rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        rig.same("The failure's message", &f.message, &message);
        let log = rig.log();
        rig.check(
            !log.calls("aws")
                .iter()
                .any(|c| c.args.get(1).map(String::as_str) == Some("deploy"))
                && log.remote_calls().is_empty(),
            "The failure MUST come before any cloudformation deploy",
        );
        let names = rig.own_log_names(PROJECT);
        rig.same_list(
            "The run's own lines",
            &names,
            &svec(&["run begun", "deploy failed"]),
        );
        let reason = message
            .strip_prefix("agentspace: ")
            .unwrap_or(&message)
            .to_string();
        rig.check(
            rig.own_log_lines(PROJECT)
                .last()
                .and_then(|l| l.reason.clone())
                == Some(reason),
            "The message MUST be written to the run's log as the reason",
        );
        let want = if kind == Kind::Ready {
            "ready"
        } else {
            "failed"
        };
        rig.check(
            rig.record(PROJECT).state.as_str() == want,
            &format!("The host MUST be recorded `{want}`"),
        );
    });
}

fn rollback_message() -> String {
    let stack = stack_of(PROJECT);
    format!(
        "agentspace: Stack '{stack}' is ROLLBACK_COMPLETE, which no update can start from. Look at its events, delete it with 'aws cloudformation delete-stack --region us-west-2 --stack-name {stack}', then run 'agentspace-operator provision -Host {PROJECT}' again."
    )
}

#[test]
fn a_rollback_complete_stack_records_a_new_host_failed_with_the_message_in_its_log() {
    status_case(Kind::NoRecord, "ROLLBACK_COMPLETE", rollback_message());
}

#[test]
fn a_rollback_complete_stack_leaves_a_ready_host_ready() {
    status_case(Kind::Ready, "ROLLBACK_COMPLETE", rollback_message());
}

#[test]
fn a_stack_in_progress_fails_the_run_with_the_try_later_message() {
    let stack = stack_of(PROJECT);
    status_case(
        Kind::Creating,
        "UPDATE_IN_PROGRESS",
        format!(
            "agentspace: Stack '{stack}' is UPDATE_IN_PROGRESS; try again once that operation has finished."
        ),
    );
}

#[test]
fn a_control_character_in_a_failure_is_written_as_a_question_mark_in_the_log() {
    // The existence check's error text carries one; the message keeps it, the log's reason does
    // not (operator.md, provision, Its log, `reason`).
    let rig = prov_rig_with(Kind::NoRecord, |rig| {
        rig.rule(
            "aws",
            1,
            "describe-stacks",
            255,
            "",
            "Throttling:\u{1}Rate exceeded\\n",
        )
    });
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        rig.same(
            "The failure's message",
            &f.message,
            "agentspace: Throttling:\u{1}Rate exceeded",
        );
        let lines = rig.own_log_lines(PROJECT);
        rig.check(
            lines.last().and_then(|l| l.reason.as_deref()) == Some("Throttling:?Rate exceeded"),
            "Each control character in a reason MUST be written as `?`",
        );
        let text = rig.provision_log(PROJECT).unwrap_or_default();
        rig.check(
            !text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\r'),
            "The log MUST hold no control character of a reason",
        );
    });
}

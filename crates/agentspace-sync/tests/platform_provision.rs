//! `provision` by platform: the record, the project list's check, the host's own folder, log and
//! lock, and the closing line (docs/operator.md, provision, steps 1 to 3 and 10, Its log; A host's
//! platform and its names; Messages; docs/state.md, The project list, A host's folder, The
//! provisioning run's log and lock; D54, D91; E0.12-T03-S03, #0256, pins P3, P4, P12, P14 of
//! v0.12-T02 and P44, P58 of v0.12-T01).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `provision_keyed_using` in a child of the test binary, under a throwaway home with a throwaway
//! CA, with stand-ins for the AWS CLI, ssh, scp and go.
//!
//! - **A host on a platform other than `linux/arm64` is made only when the project list approves
//!   the platform for the project**, and a host that already exists on it is provisioned whatever
//!   the list says. A refusal is before any AWS call and writes nothing, its log and its host's
//!   folder included.
//! - **The closing line names README's "Signing in on a new Windows host" for `windows/amd64`**,
//!   and keeps today's text byte for byte on both Linux platforms (pin P14 of v0.12-T02).
//! - **Each host's run is its own**: its record, its folder's log and lock, its stack.
//!
//! The Windows run goes through E0.12-T03-S06's rows (`a_windows_...`) and fails with its "not
//! built" answer until it merges.
#![cfg(windows)]

mod common;
mod hostcalls;
mod platformcalls;
mod waitcalls;

use common::hosts::*;
use common::provlog::LogLine;
use common::*;
use hostcalls::setup::*;
use platformcalls::*;

/// Today's last line of a run for a host that was not `ready` (operator.md, provision, step 10).
const SIGN_IN: &str = "Sign in on this host once, by hand, under a lease: see \"Signing in on a new host\" in the agentspace README.";
/// The same line for `windows/amd64`: README's other heading (pin P14 of v0.12-T02).
const SIGN_IN_WINDOWS: &str = "Sign in on this host once, by hand, under a lease: see \"Signing in on a new Windows host\" in the agentspace README.";

/// What the registry says of the host on `platform` when its run begins.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Start {
    /// No record of it, though the project has one on another platform.
    NoRecord,
    /// A `creating` record, left by a run that died.
    Creating,
    /// A `ready` record, with an old instance ID and an old attempt.
    Ready,
}

const OLD: &str = "2026-11-19T00:00:00Z";

/// A throwaway home for a provisioning run of the host on `platform`: the project's `linux/arm64`
/// host is `ready` beside it (so a run that touched it would show), the project list approves
/// `approves` (`None`: an entry with no `platforms`), and the stand-ins answer as a stack that is
/// made, or exists.
fn prov_rig(platform: &'static str, start: Start, approves: Option<&[&str]>) -> Rig {
    // The project's linux/arm64 host is `ready` beside every other platform's, and absent when it
    // is the platform under test.
    let mut recs = Vec::new();
    if platform != ARM64 {
        recs.push(PRec::new(ARM64, "ready").id(ID2).attempt(OLD));
    }
    match start {
        Start::NoRecord => {}
        Start::Creating => recs.push(PRec::new(platform, "creating")),
        Start::Ready => recs.push(PRec::new(platform, "ready").id(ID).attempt(OLD)),
    }
    let rig = plat_rig(false, &recs);
    rig.list_project_for(approves);
    rig.capture_ssh_stdin();
    let wanted = stack(platform);
    match start {
        Start::Ready => rig.stack_status_typed("UPDATE_COMPLETE", "m8a.xlarge"),
        _ => rig.stack_missing_typed(&wanted, "m8a.xlarge"),
    }
    if platform != ARM64 {
        rig.image_is(IMAGE);
    }
    rig.rule(
        "aws",
        0,
        "cloudformation deploy",
        0,
        "DEPLOY-OUT-LINE\\n",
        "",
    );
    rig.watch_file(rig.hosts_json_path());
    rig
}

/// The record JSON of `PROJECT` on `platform` in the watched registry text, or an empty string.
fn record_text(rig: &Rig, platform: &str) -> String {
    let text = rig.registry_text();
    let marker = format!("{{\"project\":\"{PROJECT}\"");
    let mut from = 0;
    while let Some(at) = text[from..].find(&marker) {
        let start = from + at;
        let end = start
            + text[start..]
                .find('}')
                .map_or(text.len() - start, |e| e + 1);
        let record = &text[start..end];
        let is_other = record.contains("\"platform\":");
        if (platform == ARM64 && !is_other)
            || record.contains(&format!("\"platform\":\"{platform}\""))
        {
            return record.to_string();
        }
        from = end;
    }
    String::new()
}

fn nothing_was_written(rig: &Rig, before: &str, platform: &str) {
    rig.check(
        rig.log().all_calls().is_empty(),
        "A refused run MUST make no AWS call and start no program",
    );
    rig.check(
        rig.registry_text() == before,
        "A refused run MUST write no record",
    );
    let host = host_name(platform);
    rig.check(
        !rig.host_folder(&host).exists() && rig.provision_log(&host).is_none(),
        "A refused run MUST write nothing, its log and its host's folder included",
    );
}

// ---------------------------------------------------------------- The project list, by platform

fn list_path(rig: &Rig) -> String {
    rig.home_path(".agentspace/projects.json")
        .display()
        .to_string()
}

fn refuses_unapproved(platform: &'static str, approves: Option<&'static [&'static str]>) {
    let rig = prov_rig(platform, Start::NoRecord, approves);
    rig.run(move |rig| {
        let before = rig.registry_text();
        let say = rig.say();
        let f = rig.failure(
            rig.do_provision_keyed(&say, &key(platform)),
            "provision -Host -Platform",
        );
        same_message(
            rig,
            "The refusal names the project, never the host name, and the platform",
            &f.message,
            &format!(
                "agentspace: Project '{PROJECT}' has no host on {platform}, and {} does not approve that platform for it, so no host is made for it.",
                list_path(rig)
            ),
        );
        nothing_was_written(rig, &before, platform);
    });
}

#[test]
fn an_entry_with_no_platforms_does_not_approve_linux_amd64() {
    refuses_unapproved(AMD64, None);
}

#[test]
fn an_entry_with_no_platforms_does_not_approve_windows() {
    refuses_unapproved(WINDOWS, None);
}

#[test]
fn an_entry_whose_platforms_omit_linux_amd64_does_not_approve_it() {
    refuses_unapproved(AMD64, Some(&[WINDOWS]));
}

#[test]
fn an_entry_whose_platforms_omit_windows_does_not_approve_it() {
    refuses_unapproved(WINDOWS, Some(&[ARM64, AMD64]));
}

#[test]
fn an_entry_whose_platforms_omit_linux_arm64_does_not_approve_it_either() {
    // linux/arm64 included (operator.md, provision, step 1): its own host is the one missing.
    refuses_unapproved(ARM64, Some(&[AMD64]));
}

fn no_entry(platform: &'static str) {
    let rig = prov_rig(platform, Start::NoRecord, Some(&[]));
    rig.write_home(
        ".agentspace/projects.json",
        b"{\"version\":0,\"projects\":[]}\n",
    );
    rig.run(move |rig| {
        let before = rig.registry_text();
        let say = rig.say();
        let f = rig.failure(
            rig.do_provision_keyed(&say, &key(platform)),
            "provision -Host -Platform",
        );
        same_message(
            rig,
            "The no-entry refusal is v0.07's, byte for byte, on every platform",
            &f.message,
            &format!(
                "agentspace: Project '{PROJECT}' has no host and is not in {}, so no host is made for it.",
                list_path(rig)
            ),
        );
        nothing_was_written(rig, &before, platform);
    });
}

#[test]
fn a_project_with_no_entry_is_refused_with_v0_07s_message_for_linux_arm64() {
    no_entry(ARM64);
}

#[test]
fn a_project_with_no_entry_is_refused_with_v0_07s_message_for_linux_amd64() {
    no_entry(AMD64);
}

#[test]
fn a_project_with_no_entry_is_refused_with_v0_07s_message_for_windows() {
    no_entry(WINDOWS);
}

#[test]
fn a_host_that_exists_on_the_platform_is_provisioned_whatever_the_list_says() {
    // A `ready` linux/amd64 record, and an entry that approves linux/arm64 alone (no `platforms`):
    // D54 provisions a project with a host.
    let rig = prov_rig(AMD64, Start::Ready, None);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_provision_keyed(&say, &key(AMD64)),
            "provision -Host -Platform linux/amd64",
        );
        rig.check(
            rig.log()
                .calls("aws")
                .iter()
                .any(|c| c.args.get(1).map(String::as_str) == Some("deploy")),
            "The run MUST have gone on to deploy the host's stack",
        );
    });
}

#[test]
fn a_record_on_another_platform_does_not_stand_in_for_an_unapproved_one() {
    // The project has a ready linux/arm64 host, and its entry approves linux/arm64 alone: a
    // windows/amd64 lease's run is still refused, with the project's arm64 host untouched.
    let rig = prov_rig(WINDOWS, Start::NoRecord, None);
    rig.run(|rig| {
        let before = rig.registry_text();
        let say = rig.say();
        let f = rig.failure(
            rig.do_provision_keyed(&say, &key(WINDOWS)),
            "provision -Host -Platform windows/amd64",
        );
        rig.check(
            f.message.contains("has no host on windows/amd64"),
            &format!("The refusal MUST name the platform: {}", f.message),
        );
        nothing_was_written(rig, &before, WINDOWS);
    });
}

// ---------------------------------------------------------------- A run on another platform

#[test]
fn a_new_linux_amd64_hosts_run_makes_its_own_record_folder_log_and_stack() {
    let rig = prov_rig(AMD64, Start::NoRecord, Some(&[ARM64, AMD64]));
    let Some(out) = rig.run(|rig| {
        let host = host_name(AMD64);
        let arm64_before = record_text(rig, ARM64);
        let say = rig.say();
        rig.succeeded(
            rig.do_provision_keyed(&say, &key(AMD64)),
            "provision -Host -Platform linux/amd64",
        );
        let log = rig.log();

        // The messages carry the host's name, and the stack is the platform's.
        let lines = say.progress_lines();
        rig.check(
            lines.first().map(String::as_str) == Some("Provisioning the host of 'demo.linux-amd64'..."),
            &format!("The first message names the host by its host name: {lines:?}"),
        );
        for want in [
            "Deploying stack 'agentspace-linux-amd64-host-demo' in us-west-2...".to_string(),
            "Recorded the host of 'demo.linux-amd64' as ready.".to_string(),
            format!("Bootstrapped: {ID2}"),
        ] {
            rig.check(lines.contains(&want), &format!("The run MUST say `{want}`: {lines:?}"));
        }

        // The record: appended at the end of the file, with `platform`, its own stack.
        let text = rig.registry_text();
        let at_end = text
            .rfind(&format!("{{\"project\":\"{PROJECT}\",\"platform\":\"linux/amd64\""))
            .zip(text.find(&arm64_before));
        rig.check(
            at_end.is_some_and(|(amd64, arm64)| amd64 > arm64),
            "The new record MUST be appended after every other, the file's order kept",
        );
        let done = rig.record_on(AMD64);
        rig.check(
            done.state.as_str() == "ready"
                && done.instance_id.as_deref() == Some(ID2)
                && done.stack == "agentspace-linux-amd64-host-demo"
                && done.last_attempt_at.is_some(),
            "The run MUST record its host ready, with its own stack and the instance the stack made",
        );
        rig.same(
            "The linux/arm64 host's record is as it was, byte for byte",
            &record_text(rig, ARM64),
            &arm64_before,
        );
        rig.check(
            !record_text(rig, ARM64).contains("\"platform\""),
            "A linux/arm64 record MUST hold no platform",
        );

        // Its folder, log and lock are its own; the arm64 host's folder is not made or written.
        let names: Vec<String> = rig.own_log_names(&host);
        rig.same_list(
            "The run's own lines, in the host's log",
            &names[..6],
            &svec(&["run begun", "deploy done", "start done", "sync done", "bootstrap done", "record done"]),
        );
        rig.check(
            names.last().map(String::as_str) == Some("run done"),
            "The run ends with `run done`",
        );
        rig.check(
            rig.home_path(&format!(".agentspace/hosts/{host}/provision.lock")).exists(),
            "The run's lock is in the host's own folder",
        );
        rig.check(
            rig.provision_log(&host_name(ARM64)).is_none(),
            "The linux/arm64 host's log MUST NOT be touched",
        );
        for c in log.calls("ssh").iter().chain(log.calls("scp").iter()) {
            rig.check(
                c.args.iter().any(|a| a.starts_with(&alias(AMD64))),
                "Every ssh and scp of the run MUST name the host's alias",
            );
        }
        rig.check(
            rig.captured_stdin("ssh", log.calls("ssh").len() as u32) == Some(std::fs::read(repo_root().join("box").join("bootstrap.sh")).unwrap()),
            "The run's last ssh is the Linux bootstrap, on its stdin",
        );
        // The last line of the run: the Linux procedure's, byte for byte.
        let events = say.events.borrow();
        rig.check(
            events.last().is_some_and(|(k, t)| k == "progress" && t == SIGN_IN),
            "The sign-in line MUST be the run's last, today's text byte for byte",
        );
    }) else {
        return;
    };
    assert!(out.stdout.contains("DEPLOY-OUT-LINE"));
}

fn closing_line(platform: &'static str, want: &'static str, approves: &'static [&'static str]) {
    let rig = prov_rig(platform, Start::NoRecord, Some(approves));
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision_keyed(&say, &key(platform)), "provision");
        let events = say.events.borrow();
        rig.check(
            events
                .last()
                .is_some_and(|(k, t)| k == "progress" && t == want),
            &format!(
                "The run's last line MUST be `{want}`; it is {:?}",
                events.last()
            ),
        );
    });
}

#[test]
fn a_linux_arm64_run_ends_with_todays_sign_in_line_byte_for_byte() {
    closing_line(ARM64, SIGN_IN, &[ARM64]);
}

#[test]
fn a_linux_amd64_run_ends_with_todays_sign_in_line_byte_for_byte() {
    closing_line(AMD64, SIGN_IN, &[AMD64]);
}

#[test]
fn a_windows_run_ends_with_the_line_naming_the_windows_sign_in_procedure() {
    // Depends on E0.12-T03-S06's rows: the run must get through sync and bootstrap.
    closing_line(WINDOWS, SIGN_IN_WINDOWS, &[WINDOWS]);
}

#[test]
fn no_run_prints_the_sign_in_line_for_a_host_that_was_ready_when_it_began() {
    let rig = prov_rig(AMD64, Start::Ready, Some(&[AMD64]));
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision_keyed(&say, &key(AMD64)), "provision");
        rig.check(
            !say.progress_lines()
                .iter()
                .any(|l| l.starts_with("Sign in on this host once")),
            "A host that was ready has had its sign-ins: no line",
        );
        let done = rig.record_on(AMD64);
        rig.check(
            done.state.as_str() == "ready",
            "A run never takes a ready host out of ready",
        );
    });
}

#[test]
fn a_creating_linux_amd64_record_is_provisioned_in_place_and_the_arm64_host_is_untouched() {
    let rig = prov_rig(AMD64, Start::Creating, Some(&[AMD64]));
    rig.run(|rig| {
        let arm64_before = record_text(rig, ARM64);
        let say = rig.say();
        rig.succeeded(rig.do_provision_keyed(&say, &key(AMD64)), "provision");
        rig.check(
            rig.read_hosts()
                .hosts
                .iter()
                .filter(|h| h.project == PROJECT)
                .count()
                == 2,
            "The run MUST NOT add a second record for the platform",
        );
        rig.same(
            "The linux/arm64 record",
            &record_text(rig, ARM64),
            &arm64_before,
        );
        let lines: Vec<LogLine> = rig.own_log_lines(&host_name(AMD64));
        rig.check(
            lines.len() >= 8,
            "The run's lines are in the host's own log",
        );
    });
}

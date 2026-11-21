//! `sync -All`, through the hidden twin of the public `sync_all` (docs/operator.md, Every host at
//! once: `-All`; sync; architect, 2026-11-21, for v0.07-T03, confirming it as public API).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `sync_all_using` in a child of the test binary, under a throwaway home with a throwaway CA and a
//! registry of five records, with stand-ins for the AWS CLI, ssh, scp and go. The `Host` lines go
//! to the command's stderr, which the parent reads from the child's.
//!
//! - **Every record, in the file's order**, gets its `Host` line, whatever `-Quiet` says.
//! - **Only a `ready` host is synced**; any other is skipped with its progress line.
//! - **A host that fails ends its own part only**, and the command ends with `<n> of <m> hosts
//!   failed.`, `<m>` counting every record; **the sign-in message ends it at once**.
//! - **`system\` is written and the localswim CLI built once**, and every later part copies the
//!   same files.
#![cfg(windows)]

mod common;
mod hostcalls;

use agentspace_operator_api::SIGN_IN_MESSAGE;
use common::expect::command_folder_of;
use common::hosts::*;
use common::*;

/// A third host's instance, made up (it MUST NOT match the identifier scan).
const ID3: &str = "i-abcdef12345678901";

/// The five records, in the file's order: the box, two `ready` hosts and two that are not.
const RECORDS: [(&str, &str, &str); 5] = [
    ("swimcoach", "agentbox", "ready"),
    ("demo", "agentspace-host-demo", "ready"),
    ("beta", "agentspace-host-beta", "creating"),
    ("gamma", "agentspace-host-gamma", "failed"),
    ("delta", "agentspace-host-delta", "ready"),
];

/// A throwaway home with a CA, the ProxyCommand program beside the operator, and the registry of
/// [`RECORDS`], each `ready` one with an instance ID of its own.
fn all_rig(go: bool) -> Rig {
    let rig = Rig::new(go);
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

/// The `Host` line of a record (operator.md: the label padded with spaces to 10 characters).
fn host_line(project: &str, stack: &str, state: &str) -> String {
    format!("Host      {project} ({stack}), {state}")
}

/// The `Host` lines of the first `n` records.
fn host_lines(n: usize) -> Vec<String> {
    RECORDS[..n]
        .iter()
        .map(|(p, s, st)| host_line(p, s, st))
        .collect()
}

/// The lines of the child's stderr that are `Host` lines, in order.
fn host_lines_in(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|l| l.starts_with("Host      "))
        .map(str::to_string)
        .collect()
}

/// The alias each ssh named, in the order the hosts were first reached.
fn aliases_reached(log: &Log) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for c in log.calls("ssh") {
        if let Some(a) = c
            .args
            .iter()
            .find(|a| *a == "agentspace" || a.starts_with("agentspace-host."))
            && !v.contains(a)
        {
            v.push(a.clone());
        }
    }
    v
}

fn skip_line(state: &str) -> String {
    format!("Skipping: the host is {state}; its next provisioning run syncs it.")
}

#[test]
fn every_ready_host_is_synced_in_order_and_the_others_are_skipped_with_their_line() {
    let rig = all_rig(true);
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync_all(&say), "sync -All");
        let log = rig.log();
        rig.same_list(
            "The hosts reached, in the file's order, each over its own alias",
            &aliases_reached(&log),
            &svec(&["agentspace", "agentspace-host.demo", "agentspace-host.delta"]),
        );
        let instances: Vec<String> = log
            .calls("aws")
            .iter()
            .filter(|c| c.args.get(1).map(String::as_str) == Some("describe-instances"))
            .filter_map(|c| c.args.last().cloned())
            .fold(Vec::new(), |mut v, id| {
                if !v.contains(&id) {
                    v.push(id);
                }
                v
            });
        rig.same_list(
            "The instances started and waited for, each its record's",
            &instances,
            &svec(&[ID, ID2, ID3]),
        );
        let progress = say.progress_lines();
        rig.same_list(
            "The skip lines",
            &progress
                .iter()
                .filter(|l| l.starts_with("Skipping: the host is"))
                .cloned()
                .collect::<Vec<_>>(),
            &[skip_line("creating"), skip_line("failed")],
        );
        for (project, ..) in &RECORDS[2..4] {
            rig.check(
                !log.all_calls()
                    .iter()
                    .any(|c| c.args.iter().any(|a| a.contains(&alias_of(project)))),
                &format!("A host that is not ready MUST NOT be reached: {project}"),
            );
        }

        // box/system written once, and every part copies the same folder.
        let systems: Vec<String> = log
            .calls("scp")
            .iter()
            .filter(|c| c.args.iter().any(|a| a == "-r"))
            .filter_map(|c| c.args.get(4).cloned())
            .collect();
        rig.check(
            systems.len() == 3
                && systems.iter().all(|s| *s == systems[0])
                && command_folder_of(&systems[0]).is_some(),
            &format!("Each ready host MUST get the one box/system written in the command's own folder: {systems:?}"),
        );

        // The localswim CLI built once, and copied to each host from the one build.
        rig.check(
            log.calls("go").len() == 1,
            "The localswim CLI MUST be built once for the whole command",
        );
        let count = |line: &str| progress.iter().filter(|l| *l == line).count();
        rig.check(
            count("Building the localswim CLI for linux/arm64...") == 1
                && count("Copying the localswim CLI to the instance...") == 3,
            "The build's message MUST print once, in the first part, and each part copies the CLI",
        );
        let clis: Vec<String> = log
            .calls("scp")
            .iter()
            .filter(|c| c.args.last().is_some_and(|a| a.ends_with(":/tmp/localswim")))
            .filter_map(|c| c.args.get(3).cloned())
            .collect();
        rig.check(
            clis.len() == 3 && clis.iter().all(|c| *c == clis[0]),
            "Every part MUST copy the same build",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's one folder MUST be removed as it ends",
        );
    }) else {
        return;
    };
    assert_eq!(
        host_lines_in(&out.stderr),
        host_lines(5),
        "A Host line MUST be printed to stderr before each record, in the file's order"
    );
}

#[test]
fn the_host_lines_print_whatever_quiet_says() {
    let rig = all_rig(false);
    let Some(out) = rig.run(|rig| {
        let say = rig.quiet_say();
        rig.succeeded(rig.do_sync_all(&say), "sync -All -Quiet");
    }) else {
        return;
    };
    assert_eq!(
        host_lines_in(&out.stderr),
        host_lines(5),
        "-Quiet MUST NOT suppress the Host lines"
    );
}

#[test]
fn a_failed_host_ends_its_own_part_and_the_count_closes_the_command() {
    let rig = all_rig(false);
    rig.rule("ssh", 0, "agentspace-host.demo", 1, "", "");
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync_all(&say), "sync -All");
        rig.same(
            "The closing message",
            &f.message,
            "agentspace: 1 of 5 hosts failed.",
        );
        let log = rig.log();
        rig.same_list(
            "The hosts reached: the one after the failed host too",
            &aliases_reached(&log),
            &svec(&[
                "agentspace",
                "agentspace-host.demo",
                "agentspace-host.delta",
            ]),
        );
        rig.check(
            log.calls("ssh")
                .iter()
                .filter(|c| c.args.iter().any(|a| a == "agentspace-host.demo"))
                .count()
                == 1,
            "A failure MUST end that host's part: nothing after its failed step",
        );
    }) else {
        return;
    };
    let lines: Vec<&str> = out.stderr.lines().collect();
    let at = |what: &str| lines.iter().position(|l| *l == what);
    let demo = at(&host_line("demo", "agentspace-host-demo", "ready"));
    let failed = at("agentspace: Removing the old staging folder on the instance failed.");
    let beta = at(&host_line("beta", "agentspace-host-beta", "creating"));
    assert!(
        demo.is_some() && failed.is_some() && beta.is_some() && demo < failed && failed < beta,
        "The failed host's message MUST print in its own part, after its Host line"
    );
    assert_eq!(host_lines_in(&out.stderr), host_lines(5));
}

#[test]
fn a_failed_build_fails_every_later_part_without_building_again() {
    let rig = all_rig(true);
    rig.rule("go", 0, "", 1, "", "build broke\\n");
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync_all(&say), "sync -All");
        rig.same(
            "The closing message",
            &f.message,
            "agentspace: 3 of 5 hosts failed.",
        );
        rig.check(
            rig.log().calls("go").len() == 1,
            "A failed build MUST NOT be tried again by a later part",
        );
    }) else {
        return;
    };
    assert_eq!(
        out.stderr
            .lines()
            .filter(|l| *l == "agentspace: Building the localswim CLI failed.")
            .count(),
        3,
        "Every ready host's part MUST fail at the build with its message"
    );
}

#[test]
fn the_sign_in_message_ends_the_command_at_once() {
    let rig = all_rig(false);
    rig.rule(
        "aws",
        0,
        ID2,
        255,
        "",
        "Unable to locate credentials. You can configure credentials by running \"aws login\".\\n",
    );
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(rig.do_sync_all(&say), "sync -All");
        rig.same("The message", &f.message, SIGN_IN_MESSAGE);
        rig.check(
            !rig.log().all_calls().iter().any(|c| {
                c.args
                    .iter()
                    .any(|a| a.contains("delta") || a.contains(ID3))
            }),
            "No host after the sign-in failure MUST be reached",
        );
    }) else {
        return;
    };
    assert_eq!(
        host_lines_in(&out.stderr),
        host_lines(2),
        "No Host line MUST follow the sign-in failure's host"
    );
    assert!(
        !out.stderr.contains("hosts failed."),
        "The sign-in failure ends the command with its own message, not the count"
    );
}

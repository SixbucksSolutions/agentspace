//! `bootstrap`: the toolchain script sent to a host on its ssh's standard input (docs/operator.md,
//! bootstrap; Programs it starts; The test seam; What sync installs and The box's files, for what
//! the script installs).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `bootstrap_using` in a child of the test binary, under a throwaway home, with stand-ins for the
//! AWS CLI and ssh. The ssh stand-in keeps each call's whole standard input, which is how a test
//! sees the script the operator writes to the pipe and closes.
#![cfg(windows)]

mod common;
mod hostcalls;

use std::fs;

use agentspace_sync::Target;
use common::expect::*;
use common::hosts::*;
use common::*;
use hostcalls::setup::*;

const RUNNING: &str = "Running the toolchain bootstrap on the instance...";

/// The remote command of the one ssh a bootstrap starts, with its shape checked: `-o
/// ClearAllForwardings=yes <alias> <remote command>`, the command one argument and the last.
fn remote_command(rig: &Rig, log: &Log, alias: &str) -> String {
    let ssh = log.calls("ssh");
    rig.check(
        ssh.len() == 1 && log.calls("scp").is_empty(),
        &format!(
            "bootstrap MUST start one ssh and no scp, and started {} ssh and {} scp",
            ssh.len(),
            log.calls("scp").len()
        ),
    );
    let args = &ssh[0].args;
    rig.check(
        args.len() == 4
            && args[0] == "-o"
            && args[1] == "ClearAllForwardings=yes"
            && args[2] == alias,
        &format!("bootstrap's ssh MUST be `-o ClearAllForwardings=yes {alias} <remote command>`, the command one argument and the last"),
    );
    let cmd = args[3].clone();
    rig.check(
        !cmd.contains('%')
            && !cmd.contains('"')
            && !cmd.chars().any(|c| c.is_control())
            && !cmd.ends_with('\\')
            && !cmd.is_empty(),
        "The remote command MUST hold no %, no double quote, no control character, and not end in a backslash (FA-15)",
    );
    cmd
}

/// The script bootstrap sends: the repository's `box/bootstrap.sh`, as it is.
fn script() -> Vec<u8> {
    fs::read(repo_root().join("box").join("bootstrap.sh")).unwrap()
}

#[test]
fn bootstrap_sends_the_script_on_the_ssh_stdin_over_a_hosts_alias() {
    let rig = sync_rig(false);
    rig.capture_ssh_stdin();
    rig.watch_file(rig.alias_file(PROJECT));
    rig.watch_file(agentspace_hosts::host_cert_path(&rig.home, PROJECT));
    rig.run(|rig| {
        let alias = alias_of(PROJECT);
        let say = rig.say();
        rig.succeeded(
            rig.do_bootstrap(&say, Target::Host(PROJECT)),
            "bootstrap -Host",
        );
        let log = rig.log();

        // The host is started and waited for first, from its own record.
        rig.check(
            matches!(log.items.first(), Some(Item::Call(r)) if r.name == "aws"),
            "bootstrap MUST start the host and wait for it before it connects",
        );
        rig.same_list(
            "The first AWS call",
            &log.calls("aws")[0].args,
            &describe_instances_args(ID2),
        );

        // The one ssh: its remote command runs the script it reads from stdin with bash, as the
        // login user.
        let cmd = remote_command(rig, &log, &alias);
        rig.check(
            cmd.contains("bash") && !cmd.contains("sudo"),
            "The remote command MUST run the script from stdin with bash, as the login user",
        );
        rig.check(
            rig.captured_stdin("ssh", 1) == Some(script()),
            "The ssh's standard input MUST be box/bootstrap.sh's bytes, exactly as carried",
        );

        // The operator's key, certificate and alias were there when the ssh started.
        rig.check(
            log.watched("ssh", 1, &alias)
                .is_some_and(|c| c.starts_with(&format!("# agentspace host {PROJECT}; ")))
                && log
                    .watched("ssh", 1, "id_ed25519-cert.pub")
                    .is_some_and(|c| c != "<missing>"),
            "The operator's alias and certificate MUST be written before the ssh starts",
        );

        // The messages, and nothing written to a file on the laptop.
        let lines: Vec<String> = say
            .progress_lines()
            .into_iter()
            .filter(|l| l == RUNNING || l.starts_with("Bootstrapped: "))
            .collect();
        rig.same_list(
            "bootstrap's messages",
            &lines,
            &[RUNNING.to_string(), format!("Bootstrapped: {ID2}")],
        );
        rig.check(
            rig.operator_entries().is_empty(),
            "The bootstrap is sent on a pipe and never written to a file: no folder of the operator's is made",
        );
        rig.check(say.warnings().is_empty(), "bootstrap MUST print no warning");
        check_environment(rig, &log);
    });
}

#[test]
fn bootstrap_of_the_box_goes_over_agentspace_and_writes_no_key_certificate_or_alias() {
    let rig = Rig::new(false);
    rig.capture_ssh_stdin();
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_bootstrap(&say, Target::Box), "bootstrap");
        let log = rig.log();
        let cmd = remote_command(rig, &log, "agentspace");
        rig.check(
            cmd.contains("bash"),
            "The remote command MUST run the script with bash",
        );
        rig.check(
            rig.captured_stdin("ssh", 1) == Some(script()),
            "The ssh's standard input MUST be box/bootstrap.sh's bytes, exactly as carried",
        );
        rig.same_list(
            "The first AWS call: the box's own instance, from its record",
            &log.calls("aws")[0].args,
            &describe_instances_args(ID),
        );
        rig.check(
            say.progress_lines()
                .iter()
                .any(|l| l.starts_with("Bootstrapped: i-")),
            "bootstrap MUST print `Bootstrapped: <id>`",
        );
        rig.check(
            !rig.has_alias_files() && !rig.host_folder(PROJECT).exists(),
            "The operator writes no key, certificate or alias for the box",
        );
    });
}

fn failed_case(code: i32) {
    let rig = sync_rig(false);
    rig.rule("ssh", 1, "", code, "", "");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_bootstrap(&say, Target::Host(PROJECT)),
            "bootstrap -Host",
        );
        rig.same(
            "The failure message",
            &f.message,
            "agentspace: The toolchain bootstrap failed on the instance.",
        );
        let lines = say.progress_lines();
        rig.check(
            lines.iter().any(|l| l == RUNNING)
                && !lines.iter().any(|l| l.starts_with("Bootstrapped: ")),
            "A failed bootstrap MUST print its running message and never `Bootstrapped:`",
        );
    });
}

#[test]
fn a_bootstrap_whose_script_fails_fails_with_its_message() {
    failed_case(1);
}

#[test]
fn a_bootstrap_whose_ssh_cannot_connect_fails_with_the_same_message() {
    failed_case(255);
}

#[test]
fn a_ca_that_fails_its_check_stops_a_hosts_bootstrap_before_its_ssh() {
    let rig = sync_rig(false);
    rig.remove_home(".agentspace/ca");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_bootstrap(&say, Target::Host(PROJECT)),
            "bootstrap -Host",
        );
        let want = match agentspace_ca::check_for_sync(&rig.ctx(), &ca_env()) {
            Err(w) => w,
            Ok(_) => panic!("The CA check passed, so this case is not a CA failure"),
        };
        rig.same("The failure message", &f.message, &want.message);
        let log = rig.log();
        rig.check(
            log.calls("ssh").is_empty() && log.calls("scp").is_empty(),
            "No ssh MUST be started when the CA fails its check",
        );
        rig.check(
            !rig.alias_file(PROJECT).exists(),
            "No alias MUST be written when the CA fails its check",
        );
    });
}

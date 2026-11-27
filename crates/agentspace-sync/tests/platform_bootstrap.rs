//! `bootstrap` by platform (docs/operator.md, bootstrap; The platform seam, the bootstrap row;
//! bootstrap on a Windows host; E0.12-T03-S03, #0256, pin P16 of v0.12-T02).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `bootstrap_keyed_using` in a child of the test binary, under a throwaway home with a throwaway
//! CA, with stand-ins for the AWS CLI, ssh and scp. The ssh stand-in keeps each call's whole
//! standard input.
//!
//! - **Linux, both platforms: one ssh whose stdin is `box/bootstrap.sh`**, run by bash, and no scp.
//! - **Windows: one `scp` of `bootstrap.ps1` as `.agentspace-bootstrap.ps1`, then one ssh that runs
//!   it** under Windows PowerShell 5.1 with `-File` and removes the copy, with an empty stdin. A
//!   failed copy or a non-zero ssh fails with `bootstrap`'s own message. The Windows tests
//!   (`a_windows_...`) go through E0.12-T03-S06's rows and fail with its "not built" answer until
//!   it merges.
#![cfg(windows)]

mod common;
mod hostcalls;
mod platformcalls;
mod waitcalls;

use std::fs;

use common::expect::*;
use common::*;
use platformcalls::encoded::*;
use platformcalls::*;

const RUNNING: &str = "Running the toolchain bootstrap on the instance...";
const FAILED: &str = "agentspace: The toolchain bootstrap failed on the instance.";

/// The messages bootstrap prints, in order.
fn messages(say: &RecSay) -> Vec<String> {
    say.progress_lines()
        .into_iter()
        .filter(|l| l == RUNNING || l.starts_with("Bootstrapped: "))
        .collect()
}

fn bootstrap_sh() -> Vec<u8> {
    fs::read(repo_root().join("box").join("bootstrap.sh")).unwrap()
}

// ---------------------------------------------------------------- Both Linux platforms

fn linux_bootstraps_on_stdin(platform: &'static str, id: &'static str) {
    let rig = plat_rig(false, &[PRec::new(platform, "ready").id(id)]);
    rig.capture_ssh_stdin();
    rig.run(move |rig| {
        let a = alias(platform);
        let say = rig.say();
        rig.succeeded(
            rig.do_bootstrap_keyed(&say, &key(platform)),
            "bootstrap -Host -Platform",
        );
        let log = rig.log();
        rig.same_list(
            "The first AWS call: the host's own instance, from its record",
            &log.calls("aws")[0].args,
            &describe_instances_args(id),
        );
        let ssh = log.calls("ssh");
        rig.check(
            ssh.len() == 1 && log.calls("scp").is_empty(),
            &format!(
                "bootstrap MUST start one ssh and no scp, and started {} ssh and {} scp",
                ssh.len(),
                log.calls("scp").len()
            ),
        );
        rig.same_list(
            "The ssh's arguments",
            &ssh[0].args,
            &svec(&["-o", "ClearAllForwardings=yes", &a, "bash -s"]),
        );
        rig.check(
            rig.captured_stdin("ssh", 1) == Some(bootstrap_sh()),
            "The ssh's standard input MUST be box/bootstrap.sh's bytes, exactly as carried",
        );
        rig.same_list(
            "bootstrap's messages",
            &messages(&say),
            &[RUNNING.to_string(), format!("Bootstrapped: {id}")],
        );
        rig.check(
            rig.operator_entries().is_empty(),
            "The bootstrap is sent on a pipe and never written to a file on a Linux host",
        );
        let host = host_name(platform);
        let info = rig.cert_info(&host);
        rig.same(
            "The operator's certificate's key ID",
            &info.key_id,
            &format!("operator-{host}"),
        );
        rig.same_list("Its principal", &info.principals, &["ubuntu".to_string()]);
        check_environment(rig, &log);
    });
}

#[test]
fn a_linux_amd64_bootstrap_sends_the_script_on_the_ssh_stdin_over_its_alias() {
    linux_bootstraps_on_stdin(AMD64, ID3);
}

#[test]
fn a_linux_arm64_bootstrap_through_its_key_is_as_before_v0_12() {
    linux_bootstraps_on_stdin(ARM64, ID2);
}

// ---------------------------------------------------------------- A Windows host

fn windows_rig() -> Rig {
    let rig = plat_rig(false, &[PRec::new(WINDOWS, "ready").id(ID4)]);
    rig.capture_ssh_stdin();
    rig
}

#[test]
fn a_windows_bootstrap_copies_the_script_with_one_scp_then_runs_it_with_one_ssh() {
    let rig = windows_rig();
    rig.run(|rig| {
        let a = alias(WINDOWS);
        let say = rig.say();
        rig.succeeded(
            rig.do_bootstrap_keyed(&say, &key(WINDOWS)),
            "bootstrap -Host -Platform windows/amd64",
        );
        let log = rig.log();
        rig.same_list(
            "The first AWS call: the host's own instance",
            &log.calls("aws")[0].args,
            &describe_instances_args(ID4),
        );
        rig.same_list(
            "The message, the copy, then the run",
            &log.timeline(|t| t == RUNNING),
            &svec(&[&format!("say: {RUNNING}"), "scp#1", "ssh#1"]),
        );

        // The copy: `bootstrap.ps1` of the command's own folder, as `.agentspace-bootstrap.ps1`.
        let scp = log.calls("scp");
        rig.same_list(
            "The scp's arguments but its source",
            &[scp[0].args[..3].to_vec(), vec![scp[0].args[4].clone()]].concat(),
            &svec(&["-o", "ClearAllForwardings=yes", "-q", &format!("{a}:.agentspace-bootstrap.ps1")]),
        );
        let source = resolve(rig, &scp[0].args[3], Folder::PerCommand, "The scp's source");
        rig.same_path(
            "The scp's source: bootstrap.ps1 in the command's own folder",
            &source,
            &rig.operator_dir().join("bootstrap.ps1"),
        );
        let carried = fs::read(repo_root().join("box").join("windows").join("bootstrap.ps1")).unwrap();
        let snap = log.file_snap("scp", 1);
        rig.check(
            snap.len() == 1 && snap[0].1 == carried.len() as u64 && snap[0].2 == fnv(&carried),
            "The copy MUST be box/windows/bootstrap.ps1's bytes, as carried",
        );

        // The run: one ssh, whose remote command runs the copy with -File and removes it, and
        // whose stdin is empty.
        let ssh = log.calls("ssh");
        rig.check(ssh.len() == 1, "One ssh");
        rig.same_list(
            "The ssh's arguments before its remote command",
            &ssh[0].args[..3],
            &svec(&["-o", "ClearAllForwardings=yes", &a]),
        );
        let script = decode_encoded(last_arg(&ssh[0].args));
        rig.check(
            script.contains(".agentspace-bootstrap.ps1") && script.contains("-File"),
            "The remote command MUST run .agentspace-bootstrap.ps1 under Windows PowerShell 5.1 with -File",
        );
        rig.check(
            script.contains("Remove-Item") || script.contains("del ") || script.contains("rm "),
            "The remote command MUST remove the copy whatever the script's status",
        );
        rig.check(
            rig.captured_stdin("ssh", 1) == Some(Vec::new()),
            "The ssh's stdin MUST be empty and closed: the script is never sent on it",
        );
        rig.same_list(
            "bootstrap's messages",
            &messages(&say),
            &[RUNNING.to_string(), format!("Bootstrapped: {ID4}")],
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's folder MUST be removed as it ends",
        );
        let host = host_name(WINDOWS);
        let info = rig.cert_info(&host);
        rig.same("The certificate's key ID", &info.key_id, &format!("operator-{host}"));
        rig.same_list("Its principal", &info.principals, &["agent".to_string()]);
    });
}

#[test]
fn a_windows_bootstraps_failed_copy_fails_with_the_bootstrap_message_and_runs_nothing() {
    let rig = windows_rig();
    rig.rule("scp", 0, "", 1, "", "scp failed\\n");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_bootstrap_keyed(&say, &key(WINDOWS)),
            "bootstrap windows/amd64",
        );
        rig.same("The failure", &f.message, FAILED);
        let log = rig.log();
        rig.check(
            log.calls("ssh").is_empty(),
            "No ssh MUST run after a failed copy",
        );
        rig.check(
            !messages(&say)
                .iter()
                .any(|l| l.starts_with("Bootstrapped: ")),
            "No Bootstrapped line",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's folder MUST be removed after a failure too",
        );
    });
}

fn windows_ssh_exits(code: i32) {
    let rig = windows_rig();
    rig.rule("ssh", 0, "", code, "", "ssh failed\\n");
    rig.run(|rig| {
        let say = rig.say();
        let f = rig.failure(
            rig.do_bootstrap_keyed(&say, &key(WINDOWS)),
            "bootstrap windows/amd64",
        );
        rig.same("The failure", &f.message, FAILED);
        rig.check(
            !messages(&say)
                .iter()
                .any(|l| l.starts_with("Bootstrapped: ")),
            "No Bootstrapped line",
        );
        rig.check(
            rig.log().calls("scp").len() == 1,
            "The copy was made before the ssh failed",
        );
    });
}

#[test]
fn a_windows_bootstraps_ssh_that_exits_1_fails_with_the_bootstrap_message() {
    windows_ssh_exits(1);
}

#[test]
fn a_windows_bootstraps_ssh_that_cannot_connect_exit_255_fails_with_the_bootstrap_message() {
    windows_ssh_exits(255);
}

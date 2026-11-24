//! What the operator itself does for `deploy`, `sync`, `bootstrap` and `provision`: it checks the
//! command line, finds the host's record, and dispatches to agentspace-sync's functions with the
//! host's target; `sync -All` goes to agentspace-sync's `sync_all` whole (docs/operator.md:
//! Commands, Which host a command acts on, Every host at once; architect, 2026-11-21, for
//! v0.07-T03). What those functions do in full is the sync crate's, and tests there.
//!
//! **Each test runs the real functions against the rig's stand-ins**, and shows the earliest step
//! of each command at which its target can be seen: the rig has no `ssh-keygen.exe`, so every CA
//! check fails, and no run gets as far as a host, the real `go` or the localswim checkout (the rig
//! gives the run no `PATH`). `host_signing.rs` shows the steps after the CA check, with a CA.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use rig::*;

fn two_hosts() -> Rig {
    let rig = Rig::standard();
    rig.write_box_and("agentspace", "ready", ID, Some(OTHER_ID));
    rig
}

fn keygen_missing(rig: &Rig) -> String {
    format!(
        "agentspace: {} is missing; install the Windows OpenSSH client.",
        rig.system_root
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe")
            .display()
    )
}

fn public_key_missing(rig: &Rig) -> String {
    format!(
        "agentspace: {} does not exist.",
        rig.profile.join(".ssh").join("id_ed25519.pub").display()
    )
}

const NO_STACK_STATUS: &str =
    "agentspace: Unexpected answer from cloudformation describe-stacks: no Stacks[0].StackStatus.";

/// A command line's failure: exit 1, nothing on stdout, and its last stderr line.
fn last_line_of_failure(rig: &Rig, line: &[&str]) -> String {
    let out = rig.run(line);
    let err = out.err();
    assert_eq!(out.code, Some(1), "{line:?}: {err:?}");
    assert_eq!(out.out(), "", "{line:?}");
    err.lines().last().unwrap_or_default().to_string()
}

/// A command line's failure that starts no program: one `agentspace: ` line.
fn failure_of(rig: &Rig, line: &[&str]) -> String {
    let out = rig.run(line);
    let err = out.err();
    assert_eq!(out.code, Some(1), "{line:?}: {err:?}");
    assert_eq!(out.out(), "", "{line:?}");
    assert_eq!(err.lines().count(), 1, "{line:?}: {err:?}");
    assert!(rig.calls().is_empty(), "{line:?}: a program was started");
    err.trim_end().to_string()
}

/// Whether a recorded call's arguments hold `text` as one of them.
fn names(call: &Call, text: &str) -> bool {
    call.args.iter().any(|a| a == text)
}

// ---- bootstrap ----

#[test]
fn bootstrap_of_the_box_starts_it_and_runs_one_ssh_over_the_boxs_alias() {
    for line in [
        vec!["bootstrap"],
        vec!["bootstrap", "-Host", "swimcoach"],
        vec!["bootstrap", "-Host", " SwimCoach "],
    ] {
        let rig = two_hosts();
        let out = rig.run(&line);
        assert_eq!(out.code, Some(0), "{line:?}: {:?}", out.err());
        assert_eq!(
            out.err(),
            format!(
                "Ready: {ID}\nRunning the toolchain bootstrap on the instance...\nBootstrapped: {ID}\n"
            ),
            "{line:?}"
        );
        let ssh = rig.calls_of("ssh");
        assert_eq!(ssh.len(), 1, "{line:?}: one ssh");
        assert!(
            names(&ssh[0], "ClearAllForwardings=yes") && names(&ssh[0], "agentspace"),
            "{line:?}: {:?}",
            ssh[0].args
        );
        assert!(
            !ssh[0].stdin.is_empty(),
            "{line:?}: the bootstrap's bytes reach the ssh's stdin"
        );
        assert!(
            rig.calls_of("ec2-describe-instances")
                .iter()
                .all(|c| names(c, ID)),
            "{line:?}: the box's instance"
        );
        assert!(!rig.signed_anything(), "{line:?}: the box is never signed");
    }
}

#[test]
fn bootstrap_with_quiet_prints_none_of_its_progress() {
    let rig = two_hosts();
    let out = rig.run(&["bootstrap", "-Quiet"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), "");
    assert_eq!(rig.calls_of("ssh").len(), 1);
}

#[test]
fn bootstrap_of_a_host_starts_that_host_then_stops_at_its_ca_check_before_any_ssh() {
    for line in [
        vec!["bootstrap", "-Host", "agentspace"],
        vec!["bootstrap", "-Host", "Agentspace", "-Quiet"],
    ] {
        let rig = two_hosts();
        assert_eq!(
            last_line_of_failure(&rig, &line),
            keygen_missing(&rig),
            "{line:?}"
        );
        let described = rig.calls_of("ec2-describe-instances");
        assert!(
            !described.is_empty() && described.iter().all(|c| names(c, OTHER_ID)),
            "{line:?}: the host's own instance is started and waited for"
        );
        assert!(rig.calls_of("ssh").is_empty(), "{line:?}: no ssh");
        assert!(!rig.signed_anything(), "{line:?}: nothing signed");
    }
}

#[test]
fn bootstrap_for_a_project_with_no_record_fails_before_the_sync_crate_is_reached() {
    let rig = two_hosts();
    let line = failure_of(&rig, &["bootstrap", "-Host", "nosuch"]);
    assert_eq!(
        line,
        "agentspace: Project 'nosuch' has no host; provision -Host nosuch makes one."
    );
}

// ---- provision ----

#[test]
fn provision_of_an_unlisted_project_with_no_record_is_refused_and_writes_nothing() {
    for rig in [two_hosts(), Rig::standard()] {
        let before = rig.hosts_bytes();
        let message = failure_of(&rig, &["provision", "-Host", "NoSuch", "-Quiet"]);
        assert_eq!(
            message,
            format!(
                "agentspace: Project 'nosuch' has no host and is not in {}, so no host is made for it.",
                rig.state_dir().join("projects.json").display()
            )
        );
        assert_eq!(rig.hosts_bytes(), before, "no record is added");
        assert!(!rig.host_dir("nosuch").exists(), "no log, no host's folder");
    }
}

#[test]
fn provision_of_a_host_with_a_record_logs_and_deploys_that_hosts_stack_first() {
    // Its deploy step's existence check is the first AWS call, and names the host's own stack. The
    // rig's answer has no StackStatus, so the step fails there, and a `ready` host stays `ready`.
    for project in ["agentspace", "Agentspace"] {
        let rig = two_hosts();
        assert_eq!(
            last_line_of_failure(&rig, &["provision", "-Host", project]),
            NO_STACK_STATUS,
            "{project}"
        );
        let calls = rig.calls();
        assert_eq!(calls[0].key, "cloudformation-describe-stacks", "{project}");
        assert!(
            names(&calls[0], "agentspace-host-agentspace"),
            "{project}: {:?}",
            calls[0].args
        );
        assert!(
            rig.host_dir("agentspace").join("provision.log").is_file(),
            "{project}: the run's log"
        );
        let registry = String::from_utf8(rig.hosts_bytes().expect("the registry")).unwrap();
        assert!(
            registry.contains(
                "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\""
            ),
            "{project}: a ready host stays ready"
        );
    }
}

#[test]
fn provision_of_the_box_runs_the_boxs_deploy_first() {
    let rig = two_hosts();
    assert_eq!(
        last_line_of_failure(&rig, &["provision", "-Host", "swimcoach"]),
        public_key_missing(&rig)
    );
    assert!(
        rig.host_dir("swimcoach").join("provision.log").is_file(),
        "the box's log is in hosts\\swimcoach"
    );
    assert!(
        rig.calls_of("cloudformation-describe-stacks").is_empty(),
        "the box's deploy makes no existence check"
    );
}

// ---- sync ----

#[test]
fn sync_for_a_host_stops_at_its_ca_check_whatever_its_state() {
    for state in ["ready", "creating", "failed"] {
        let rig = Rig::standard();
        rig.write_box_and("agentspace", state, ID, Some(OTHER_ID));
        assert_eq!(
            failure_of(&rig, &["sync", "-Host", "agentspace"]),
            keygen_missing(&rig),
            "{state}"
        );
        assert!(!rig.signed_anything(), "{state}: nothing signed");
    }
}

#[test]
fn sync_without_a_host_and_for_swimcoach_is_the_boxs_sync_and_the_same_command() {
    for line in [
        vec!["sync"],
        vec!["sync", "-Host", "swimcoach"],
        vec!["sync", "-Host", " SwimCoach "],
    ] {
        let rig = two_hosts();
        assert_eq!(
            failure_of(&rig, &line),
            keygen_missing(&rig),
            "{line:?}: the box's sync checks the CA first, and the rig has no ssh-keygen.exe"
        );
    }
}

// ---- deploy ----

#[test]
fn deploy_for_a_host_checks_that_hosts_stack_first() {
    for line in [
        vec!["deploy", "-Host", "agentspace"],
        vec!["deploy", "-Host", "Agentspace", "-Quiet"],
    ] {
        let rig = two_hosts();
        assert_eq!(
            last_line_of_failure(&rig, &line),
            NO_STACK_STATUS,
            "{line:?}"
        );
        let calls = rig.calls();
        assert_eq!(
            calls.len(),
            1,
            "{line:?}: nothing after the existence check"
        );
        assert_eq!(calls[0].key, "cloudformation-describe-stacks");
        assert!(names(&calls[0], "agentspace-host-agentspace"), "{line:?}");
    }
}

#[test]
fn deploy_with_an_instance_type_passes_it_to_the_hosts_existing_stack_and_then_syncs() {
    for (line, itype) in [
        (
            vec![
                "deploy",
                "-Host",
                "agentspace",
                "-InstanceType",
                "m9g.2xlarge",
            ],
            "m9g.2xlarge",
        ),
        (
            vec![
                "deploy",
                "-InstanceType",
                "m7i.metal-24xl",
                "-Host",
                "Agentspace",
            ],
            "m7i.metal-24xl",
        ),
    ] {
        let rig = two_hosts();
        rig.ok(
            "cloudformation-describe-stacks",
            &format!(
                "{{\"Stacks\":[{{\"StackName\":\"agentspace-host-agentspace\",\"StackStatus\":\"UPDATE_COMPLETE\",\"Parameters\":[{{\"ParameterKey\":\"InstanceType\",\"ParameterValue\":\"{itype}\"}}],\"Outputs\":[{{\"OutputKey\":\"InstanceId\",\"OutputValue\":\"{OTHER_ID}\"}}]}}]}}"
            ),
        );
        rig.ok("cloudformation-deploy", "");
        // Step 6 is `sync -Host`, which stops at its CA check in this rig.
        assert_eq!(
            last_line_of_failure(&rig, &line),
            keygen_missing(&rig),
            "{line:?}"
        );
        let deploys = rig.calls_of("cloudformation-deploy");
        assert_eq!(deploys.len(), 1, "{line:?}");
        let overrides: Vec<&String> = deploys[0]
            .args
            .iter()
            .skip_while(|a| *a != "--parameter-overrides")
            .skip(1)
            .take_while(|a| !a.starts_with("--"))
            .collect();
        assert_eq!(
            overrides,
            [&format!("InstanceType={itype}")],
            "{line:?}: the type alone, to an existing stack: {:?}",
            deploys[0].args
        );
        assert!(
            names(&deploys[0], "agentspace-host-agentspace"),
            "{line:?}: {:?}",
            deploys[0].args
        );
    }
}

#[test]
fn deploy_without_a_host_and_for_swimcoach_is_the_boxs_deploy_and_the_same_command() {
    for line in [
        vec!["deploy"],
        vec!["deploy", "-Host", "swimcoach"],
        vec!["deploy", "-Host", "SwimCoach"],
    ] {
        let rig = two_hosts();
        assert_eq!(
            failure_of(&rig, &line),
            public_key_missing(&rig),
            "{line:?}: the box's deploy reads the public key first"
        );
    }
}

#[test]
fn deploy_for_a_project_with_no_record_is_refused_before_the_sync_crate() {
    // `deploy -Host` creates the stack of a host that has a record and no stack yet, and never a
    // record (operator.md, From the command line).
    let rig = two_hosts();
    assert_eq!(
        failure_of(
            &rig,
            &["deploy", "-Host", "nosuch", "-InstanceType", "m9g.xlarge"]
        ),
        "agentspace: Project 'nosuch' has no host; provision -Host nosuch makes one."
    );
}

// ---- sync -All ----

#[test]
fn sync_all_runs_each_ready_host_and_skips_the_others_by_name() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec("alpha", "creating", None),
        host_rec("agentspace", "ready", Some(OTHER_ID)),
        host_rec("zeta", "failed", Some(THIRD_ID)),
    ]);
    let out = rig.run(&["sync", "-All"]);
    let err = out.err();
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(out.code, Some(1), "{err:?}");
    assert_eq!(out.out(), "");
    let keygen = keygen_missing(&rig);
    assert_eq!(
        lines,
        [
            "Host      swimcoach (agentbox), ready",
            keygen.as_str(),
            "Host      alpha (agentspace-host-alpha), creating",
            "Skipping: the host is creating; its next provisioning run syncs it.",
            "Host      agentspace (agentspace-host-agentspace), ready",
            keygen.as_str(),
            "Host      zeta (agentspace-host-zeta), failed",
            "Skipping: the host is failed; its next provisioning run syncs it.",
            // Two hosts failed, of four records: a skipped host is no failure.
            "agentspace: 2 of 4 hosts failed.",
        ],
        "each line once: the operator prints none of sync -All's lines itself"
    );
    assert!(
        rig.calls().is_empty(),
        "every ready host's part stops at its CA check in this rig"
    );
    assert!(!rig.signed_anything());
}

#[test]
fn sync_all_prints_a_closing_count_of_the_hosts_that_failed() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec("alpha", "creating", None),
        host_rec("agentspace", "ready", Some(OTHER_ID)),
    ]);
    let out = rig.run(&["sync", "-All"]);
    let err = out.err();
    assert_eq!(out.code, Some(1), "{err:?}");
    assert_eq!(
        err.lines().last(),
        Some("agentspace: 2 of 3 hosts failed."),
        "n is the hosts that failed, m the records: {err:?}"
    );
}

#[test]
fn sync_all_with_quiet_keeps_the_host_lines_and_hides_the_skips() {
    let rig = Rig::standard();
    rig.write_hosts(&[
        box_rec(Some(ID)),
        host_rec("alpha", "creating", None),
        host_rec("agentspace", "ready", Some(OTHER_ID)),
    ]);
    let out = rig.run(&["sync", "-All", "-Quiet"]);
    let err = out.err();
    assert!(err.contains("Host      alpha (agentspace-host-alpha), creating"));
    assert!(!err.contains("Skipping"), "{err:?}");
    assert_eq!(out.code, Some(1), "{err:?}");
}

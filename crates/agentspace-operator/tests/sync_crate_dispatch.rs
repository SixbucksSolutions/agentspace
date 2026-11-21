//! What the operator itself does for `deploy`, `sync`, `bootstrap` and `provision`: it checks the
//! command line, finds the host's record, and dispatches to agentspace-sync's pinned functions
//! with the host's target (docs/operator.md: Commands, Which host a command acts on, Every host at
//! once). What those functions do is the sync crate's, and tests there.
//!
//! **These tests read the pinned stubs' messages** (`agentspace: <command> is not built yet.`),
//! which say which function and which target a line reached, as the sync crate's pin names them:
//! `sync -Host`, `deploy -Host`, `bootstrap` and `provision` are stubs on this branch, and the
//! box's `sync` and `deploy` are today's. Once the story that builds `bootstrap`, `provision` and
//! the host targets merges, the lines marked "stub" below change with it.
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

fn not_built(command: &str) -> String {
    format!("agentspace: {command} is not built yet.")
}

/// A command line's failure: one `agentspace: ` line, nothing started, nothing on stdout.
fn failure_of(rig: &Rig, line: &[&str]) -> String {
    let out = rig.run(line);
    let err = out.err();
    assert_eq!(out.code, Some(1), "{line:?}: {err:?}");
    assert_eq!(out.out(), "", "{line:?}");
    assert_eq!(err.lines().count(), 1, "{line:?}: {err:?}");
    assert!(
        rig.calls().is_empty(),
        "{line:?}: the operator starts no program of its own for these commands"
    );
    // Two rigs hold two throwaway folders, so a message that names a path of the rig names its own:
    // the rig's folder is written `<rig>`, and two failures compare equal when only it differs.
    err.trim_end()
        .replace(&rig.root().display().to_string(), "<rig>")
}

// ---- bootstrap and provision (stubs) ----

#[test]
fn bootstrap_is_dispatched_with_and_without_a_host() {
    for line in [
        vec!["bootstrap"],
        vec!["bootstrap", "-Host", "agentspace"],
        vec!["bootstrap", "-Host", "swimcoach"],
        vec!["bootstrap", "-Quiet"],
        vec!["bootstrap", "-Host", "agentspace", "-Quiet"],
    ] {
        let rig = two_hosts();
        assert_eq!(failure_of(&rig, &line), not_built("bootstrap"), "{line:?}"); // stub
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

#[test]
fn provision_is_dispatched_for_a_project_with_a_record_or_none() {
    // Only provision adds a record, so a project with no record is dispatched, not refused.
    for project in ["agentspace", "swimcoach", "nosuch", "Agentspace"] {
        let rig = two_hosts();
        assert_eq!(
            failure_of(&rig, &["provision", "-Host", project]),
            not_built("provision"), // stub
            "{project}"
        );
    }
    let rig = Rig::standard();
    assert_eq!(
        failure_of(&rig, &["provision", "-Host", "agentspace", "-Quiet"]),
        not_built("provision"), // stub
        "no registry at all"
    );
    assert_eq!(
        rig.hosts_bytes(),
        None,
        "the operator itself adds no record: that is the run's step 3"
    );
}

// ---- sync ----

#[test]
fn sync_for_a_host_is_dispatched_with_that_hosts_target() {
    let rig = two_hosts();
    assert_eq!(
        failure_of(&rig, &["sync", "-Host", "agentspace"]),
        not_built("sync -Host") // stub
    );
    // Whatever its state.
    for state in ["creating", "failed"] {
        let rig = Rig::standard();
        rig.write_box_and("agentspace", state, ID, Some(OTHER_ID));
        assert_eq!(
            failure_of(&rig, &["sync", "-Host", "agentspace"]),
            not_built("sync -Host"), // stub
            "{state}"
        );
    }
}

#[test]
fn sync_without_a_host_and_for_swimcoach_is_the_boxs_sync_and_the_same_command() {
    let rig = two_hosts();
    let plain = failure_of(&rig, &["sync"]);
    assert_eq!(
        plain,
        keygen_missing(&rig).replace(&rig.root().display().to_string(), "<rig>"),
        "the box's sync checks the CA first, and the rig has no ssh-keygen.exe"
    );
    let rig = two_hosts();
    assert_eq!(
        failure_of(&rig, &["sync", "-Host", "swimcoach"]),
        plain,
        "-Host swimcoach is the same as no -Host in every respect"
    );
    let rig = two_hosts();
    assert_eq!(failure_of(&rig, &["sync", "-Host", " SwimCoach "]), plain);
}

// ---- deploy ----

#[test]
fn deploy_for_a_host_is_dispatched_with_that_hosts_target() {
    for line in [
        vec!["deploy", "-Host", "agentspace"],
        vec!["deploy", "-Host", "Agentspace", "-Quiet"],
    ] {
        let rig = two_hosts();
        assert_eq!(
            failure_of(&rig, &line),
            not_built("deploy -Host"), // stub
            "{line:?}"
        );
    }
}

#[test]
fn deploy_with_an_instance_type_is_dispatched_and_not_refused_as_a_usage_error() {
    // The pinned `deploy` takes no instance type, so how the value reaches the sync crate is the
    // story's choice (see the report); what holds here is that the line passes every check, finds
    // the host, starts no program of the operator's own, and fails there, with a line that names
    // `deploy`, and not as a usage error.
    for line in [
        vec![
            "deploy",
            "-Host",
            "agentspace",
            "-InstanceType",
            "m9g.2xlarge",
        ],
        vec![
            "deploy",
            "-InstanceType",
            "m7i.metal-24xl",
            "-Host",
            "Agentspace",
        ],
    ] {
        let rig = two_hosts();
        let message = failure_of(&rig, &line);
        assert!(
            message.starts_with("agentspace: deploy") && message.ends_with("is not built yet."),
            "{line:?}: {message:?}" // stub
        );
    }
}

#[test]
fn deploy_without_a_host_and_for_swimcoach_is_the_boxs_deploy_and_the_same_command() {
    let rig = two_hosts();
    let plain = failure_of(&rig, &["deploy"]);
    assert!(
        !plain.contains("not built yet"),
        "the box's deploy is today's: {plain:?}"
    );
    let rig = two_hosts();
    assert_eq!(failure_of(&rig, &["deploy", "-Host", "swimcoach"]), plain);
    let rig = two_hosts();
    assert_eq!(failure_of(&rig, &["deploy", "-Host", "SwimCoach"]), plain);
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
    assert_eq!(lines.len(), 9, "eight lines and the closing count: {err:?}");
    assert_eq!(lines[0], "Host      swimcoach (agentbox), ready");
    assert!(
        lines[1].starts_with("agentspace: "),
        "the box's sync fails at its CA check in this rig: {err:?}"
    );
    assert_eq!(
        lines[2],
        "Host      alpha (agentspace-host-alpha), creating"
    );
    assert_eq!(
        lines[3],
        "Skipping: the host is creating; its next provisioning run syncs it."
    );
    assert_eq!(
        lines[4],
        "Host      agentspace (agentspace-host-agentspace), ready"
    );
    assert_eq!(lines[5], not_built("sync -Host")); // stub
    assert_eq!(lines[6], "Host      zeta (agentspace-host-zeta), failed");
    assert_eq!(
        lines[7],
        "Skipping: the host is failed; its next provisioning run syncs it."
    );
    // Two hosts failed, of four records: a skipped host is no failure.
    assert_eq!(lines[8], "agentspace: 2 of 4 hosts failed.");
    assert!(
        rig.calls().is_empty(),
        "sync -All itself starts no program: its parts are the sync crate's"
    );
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

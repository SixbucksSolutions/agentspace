//! What `provision` refuses or cannot start, before any step: a project with no host that is not
//! listed, a project list that cannot be read, a log that cannot be opened, a run already running,
//! and a registry whose lock is held (docs/operator.md, provision, steps 1 to 3 and "When step 3 or
//! step 8 cannot change the registry"; Which host a command acts on; docs/state.md, The
//! provisioning run's log and lock).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each case runs the crate's hidden
//! `provision_using` in a child of the test binary, under a throwaway home, and checks the message,
//! the log, that the registry was left as it was, and that no program was started.
#![cfg(windows)]

mod common;
mod hostcalls;

use common::hosts::*;
use common::*;
use hostcalls::setup::*;

/// Nothing was started, nothing was made for the host, and the registry is what it was.
fn changed_nothing(rig: &Rig, before: &Option<Vec<u8>>, what: &str) {
    rig.check(
        rig.log().all_calls().is_empty(),
        &format!("{what}: no program MUST have been started"),
    );
    rig.check(
        &rig.hosts_bytes() == before,
        &format!("{what}: the registry MUST be left as it was"),
    );
    rig.check(
        rig.operator_entries().is_empty(),
        &format!("{what}: no folder of the operator's MUST have been made"),
    );
}

// ---------------------------------------------------------------- Step 1: the record and the list

#[test]
fn a_project_with_no_host_that_is_not_listed_is_refused_and_nothing_is_written() {
    let rig = prov_rig(Kind::NoRecord);
    rig.list_projects(&["other"]);
    rig.run(|rig| {
        let before = rig.hosts_bytes();
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let path = agentspace_hosts::projects_path(&rig.home);
        same_message(
            rig,
            "The failure message",
            &f.message,
            &format!(
                "agentspace: Project '{PROJECT}' has no host and is not in {}, so no host is made for it.",
                path.display()
            ),
        );
        changed_nothing(rig, &before, "An unlisted project");
        rig.check(
            !rig.host_folder(PROJECT).exists(),
            "An unlisted project MUST get no log and no host folder",
        );
    });
}

#[test]
fn a_missing_project_list_lists_nothing_so_a_project_with_no_host_is_refused() {
    let rig = prov_rig(Kind::NoRecord);
    rig.remove_home(".agentspace/projects.json");
    rig.run(|rig| {
        let before = rig.hosts_bytes();
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        rig.check(
            f.message.starts_with(&format!(
                "agentspace: Project '{PROJECT}' has no host and is not in "
            )) && f.message.ends_with(", so no host is made for it."),
            &format!("A missing project list lists nothing: `{}`", f.message),
        );
        changed_nothing(rig, &before, "A missing project list");
        rig.check(
            !rig.host_folder(PROJECT).exists(),
            "No host folder MUST be made",
        );
    });
}

#[test]
fn a_project_list_that_cannot_be_read_fails_a_project_with_no_host() {
    let rig = prov_rig(Kind::NoRecord);
    rig.write_home(".agentspace/projects.json", b"{ not json");
    rig.run(|rig| {
        let before = rig.hosts_bytes();
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let path = agentspace_hosts::projects_path(&rig.home);
        let prefix = format!("agentspace: The project list {} cannot be read: ", path.display());
        rig.check(
            crate::common::norm(&f.message).starts_with(&crate::common::norm(&prefix))
                && f.message.ends_with('.')
                && !f.message.contains('\n'),
            &format!("The failure MUST be `agentspace: The project list <path> cannot be read: <reason>.`; it was `{}`", f.message),
        );
        changed_nothing(rig, &before, "A project list that cannot be read");
        rig.check(!rig.host_folder(PROJECT).exists(), "No host folder MUST be made");
    });
}

#[test]
fn a_project_list_that_cannot_be_read_never_fails_a_run_on_a_host_that_has_a_record() {
    let rig = prov_rig(Kind::Ready);
    rig.write_home(".agentspace/projects.json", b"{ not json");
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        rig.check(
            rig.record(PROJECT).state.as_str() == "ready",
            "The list is read only for a project with no record",
        );
    });
}

#[test]
fn a_project_with_a_record_is_provisioned_whatever_the_list_holds() {
    let rig = prov_rig(Kind::Failed);
    rig.list_projects(&["other"]);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_provision(&say, PROJECT), "provision");
        rig.check(
            rig.record(PROJECT).state.as_str() == "ready",
            "A host with a record is provisioned whether or not its project is listed",
        );
    });
}

// ---------------------------------------------------------------- Step 2: the log, then the lock

/// A run that finds `provision.lock` held fails with one `run failed` line, writes no record and
/// starts nothing.
fn held_lock_case(kind: Kind) {
    let rig = prov_rig(kind);
    rig.run(|rig| {
        let before = rig.hosts_bytes();
        let held = match agentspace_hosts::try_run_lock(&rig.home, PROJECT) {
            Ok(Some(lock)) => lock,
            _ => panic!("The throwaway run lock could not be taken"),
        };
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let reason = format!("A provisioning run for '{PROJECT}' is already running.");
        rig.same(
            "The failure's message",
            &f.message,
            &format!("agentspace: {reason}"),
        );
        let lines = rig.own_log_lines(PROJECT);
        rig.check(
            lines.len() == 1
                && lines[0].name() == "run failed"
                && lines[0].reason.as_deref() == Some(reason.as_str()),
            &format!("A run that finds the lock held MUST write one line, `run failed reason={reason}`, and no `run begun`: {lines:?}"),
        );
        changed_nothing(rig, &before, "A held run lock");
        rig.check(
            !say.progress_lines().iter().any(|l| l.starts_with("Provisioning the host")),
            "A run that did not begin prints nothing of its steps",
        );
        drop(held);
    });
}

#[test]
fn a_run_already_running_fails_with_one_log_line_and_writes_no_record() {
    held_lock_case(Kind::Ready);
}

#[test]
fn a_run_already_running_for_a_new_host_makes_no_record_either() {
    held_lock_case(Kind::NoRecord);
}

#[test]
fn a_log_that_cannot_be_opened_fails_the_run_with_no_record_and_no_lock() {
    let rig = prov_rig(Kind::NoRecord);
    // A folder where the log has to be.
    rig.mkdir_home(".agentspace/hosts/demo/provision.log");
    rig.run(|rig| {
        let before = rig.hosts_bytes();
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let path = agentspace_hosts::host_log_path(&rig.home, PROJECT);
        let prefix = format!("agentspace: Cannot open the provisioning log {}: ", path.display());
        rig.check(
            crate::common::norm(&f.message).starts_with(&crate::common::norm(&prefix))
                && f.message.ends_with('.')
                && !f.message.contains('\n'),
            &format!("The failure MUST be `agentspace: Cannot open the provisioning log <path>: <error>.`; it was `{}`", f.message),
        );
        changed_nothing(rig, &before, "A log that cannot be opened");
        rig.check(
            !agentspace_hosts::host_run_lock_path(&rig.home, PROJECT).exists(),
            "A run that cannot open its log MUST NOT take the lock",
        );
    });
}

#[test]
fn a_registry_whose_lock_is_held_fails_the_run_at_step_3_after_the_locks_30_seconds() {
    let rig = prov_rig(Kind::Ready);
    rig.run(|rig| {
        let before = rig.hosts_bytes();
        // Held here, in the run's own process, for the whole call: the lock is a file opened with
        // no sharing, so a second open fails whoever holds it.
        let held = match agentspace_hosts::lock(&rig.home) {
            Ok(lock) => lock,
            Err(_) => panic!("The throwaway registry lock could not be taken"),
        };
        let say = rig.say();
        let f = rig.failure(rig.do_provision(&say, PROJECT), "provision");
        let path = agentspace_hosts::hosts_lock_path(&rig.home);
        let message = format!(
            "agentspace: The host registry's lock {} is held by another program; try again.",
            path.display()
        );
        same_message(rig, "The failure's message", &f.message, &message);
        let lines = rig.own_log_lines(PROJECT);
        let reason = message.strip_prefix("agentspace: ").unwrap_or(&message);
        rig.check(
            lines.len() == 1
                && lines[0].name() == "run failed"
                && lines[0].reason.as_deref().map(norm) == Some(norm(reason)),
            &format!("Step 3's failure MUST be one `run failed` line with the registry's message, and no `run begun`: {lines:?}"),
        );
        changed_nothing(rig, &before, "A registry whose lock is held");
        drop(held);
        rig.check(
            matches!(agentspace_hosts::try_run_lock(&rig.home, PROJECT), Ok(Some(_))),
            "provision.lock MUST be released when the run fails",
        );
    });
}

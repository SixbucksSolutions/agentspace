//! PENDING-API: each host's holder record and ssh log paths by D55's names (docs/state.md,
//! Another host's holder; docs/keeper.md, Hosts, The hosts of a pass, The holder's log;
//! E0.07-T03-S05, #0154).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing.
//!
//! **This file is compiled out** (`#![cfg(any())]`): `agentspace-keeper-files` on `main` names the
//! box's files only, and the card says what the story adds ("each host's holder record and ssh log
//! paths by D55's names, the box's unchanged") without the names of the functions. The dev lead
//! MUST reconcile the names below with the implementer's, then delete the attribute. The names
//! written here are the test author's expectation, each with what it needs:
//!
//! - `holder_record_file(project: &str) -> String`: `board-holder.<project>.json`.
//! - `holder_log_file(project: &str) -> String`: `board-holder-ssh.<project>.log`.
//! - `holder_log_rotated_file(project: &str) -> String`: `board-holder-ssh.<project>.log.1`.
//! - `HOLDER_LOG_FILE`, `HOLDER_LOG_ROTATED_FILE`: the box's `board-holder-ssh.log` and
//!   `board-holder-ssh.log.1`, unchanged.
//! - `project_of_record_file(name: &str) -> Option<String>`: the project a file name is the
//!   record of, taken in lowercase; `None` for a name that is no host's record.
//! - `project_of_log_file(name: &str) -> Option<String>`: the same for a holder log's name.
#![cfg(any())]

use agentspace_keeper_files::{
    HOLDER_LOG_FILE, HOLDER_LOG_ROTATED_FILE, holder_log_file, holder_log_rotated_file,
    holder_record_file, project_of_log_file, project_of_record_file,
};

/// D55: another host's holder record and log are named by its project; the box's keep v0.06's
/// names, with no project in them.
#[test]
fn each_hosts_files_are_named_by_its_project() {
    assert_eq!(holder_record_file("alpha"), "board-holder.alpha.json");
    assert_eq!(holder_log_file("alpha"), "board-holder-ssh.alpha.log");
    assert_eq!(
        holder_log_rotated_file("alpha"),
        "board-holder-ssh.alpha.log.1"
    );
    assert_eq!(
        holder_record_file("my-project"),
        "board-holder.my-project.json"
    );
    assert_eq!(HOLDER_LOG_FILE, "board-holder-ssh.log");
    assert_eq!(HOLDER_LOG_ROTATED_FILE, "board-holder-ssh.log.1");
}

/// keeper.md, The hosts of a pass, item 3: a file named `board-holder.<project>.json` whose
/// `<project>` matches `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$` is a host's record, compared without
/// regard to case and taken in lowercase.
#[test]
fn a_record_file_is_found_by_its_name() {
    assert_eq!(
        project_of_record_file("board-holder.alpha.json").as_deref(),
        Some("alpha")
    );
    assert_eq!(
        project_of_record_file("board-holder.Alpha.json").as_deref(),
        Some("alpha")
    );
    assert_eq!(
        project_of_record_file("board-holder.a.json").as_deref(),
        Some("a")
    );
    let longest = format!("a{}z", "b".repeat(30));
    assert_eq!(longest.len(), 32);
    assert_eq!(
        project_of_record_file(&format!("board-holder.{longest}.json")).as_deref(),
        Some(longest.as_str())
    );
}

/// A name that is no host's record: the box's own, a project that is not normalized, a temporary
/// file of a replacement, a log, and the lock.
#[test]
fn a_name_that_is_no_hosts_record_is_none() {
    let too_long = "a".repeat(33);
    for name in [
        "board-holder.json",
        "board-holder..json",
        "board-holder.-bad.json",
        "board-holder.bad-.json",
        "board-holder.bad_name.json",
        "board-holder.al.pha.json",
        "board-holder.alpha.json.1a2b3c.tmp",
        "board-holder.log",
        "board-holder.lock",
        "board-holder-ssh.alpha.log",
        &format!("board-holder.{too_long}.json"),
    ] {
        assert_eq!(project_of_record_file(name), None, "{name}");
    }
}

/// The same for a holder's log, which the rotation finds by listing the state folder.
#[test]
fn a_log_file_is_found_by_its_name() {
    assert_eq!(
        project_of_log_file("board-holder-ssh.alpha.log").as_deref(),
        Some("alpha")
    );
    for name in [
        "board-holder-ssh.log",
        "board-holder-ssh.alpha.log.1",
        "board-holder-ssh.Bad_Name.log",
        "board-holder-ssh.-x.log",
        "board-holder.alpha.json",
    ] {
        assert_eq!(project_of_log_file(name), None, "{name}");
    }
}

//! Each host's holder record and ssh log names by D55's names, through the crate's public API
//! (docs/state.md, Another host's holder; docs/keeper.md, Hosts, The hosts of a pass, The holder's
//! log; E0.07-T03-S05, #0154).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. A host is named by
//! `Option<&str>`: `None` is the box, whose names MUST stay v0.06's, and `Some(project)` is another
//! host, whose names carry its normalized project. `src/hosts.rs`'s unit tests cover the rest of
//! the names; this file adds what they do not: the names as the crate exports them, and the
//! bounds of a record name's project.

use agentspace_keeper_files::{
    holder_record_file, holder_ssh_log_file, holder_ssh_log_rotated_file,
    project_of_holder_record_file,
};

/// D55: another host's holder record and log are named by its project; the box's keep v0.06's
/// names, with no project in them.
#[test]
fn each_hosts_files_are_named_by_its_project() {
    assert_eq!(holder_record_file(Some("alpha")), "board-holder.alpha.json");
    assert_eq!(
        holder_ssh_log_file(Some("alpha")),
        "board-holder-ssh.alpha.log"
    );
    assert_eq!(
        holder_ssh_log_rotated_file(Some("alpha")),
        "board-holder-ssh.alpha.log.1"
    );
    assert_eq!(
        holder_record_file(Some("my-project")),
        "board-holder.my-project.json"
    );
    assert_eq!(holder_record_file(None), "board-holder.json");
    assert_eq!(holder_ssh_log_file(None), "board-holder-ssh.log");
    assert_eq!(holder_ssh_log_rotated_file(None), "board-holder-ssh.log.1");
}

/// keeper.md, The hosts of a pass, item 3: a file named `board-holder.<project>.json` whose
/// `<project>` matches `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$` is a host's record, compared without
/// regard to case and taken in lowercase: from one character to 32.
#[test]
fn a_record_file_is_found_by_its_name_at_the_bounds_of_a_project() {
    assert_eq!(
        project_of_holder_record_file("board-holder.Alpha.json").as_deref(),
        Some("alpha")
    );
    assert_eq!(
        project_of_holder_record_file("board-holder.a.json").as_deref(),
        Some("a")
    );
    let longest = format!("a{}z", "b".repeat(30));
    assert_eq!(longest.len(), 32);
    assert_eq!(
        project_of_holder_record_file(&format!("board-holder.{longest}.json")).as_deref(),
        Some(longest.as_str())
    );
}

/// A name that is no host's record: a project that ends in a hyphen, and one of 33 characters.
#[test]
fn a_name_whose_project_is_not_normalized_is_no_hosts_record() {
    let too_long = "a".repeat(33);
    for name in [
        "board-holder.bad-.json",
        &format!("board-holder.{too_long}.json"),
    ] {
        assert_eq!(project_of_holder_record_file(name), None, "{name}");
    }
}

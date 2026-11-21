//! Each host's holder files, by D55's names (state.md, Another host's holder).
//!
//! RFC 2119 keywords in this module's comments, and the capitals are load-bearing. A host is
//! named by `Option<&str>`: `None` is the box, whose names are v0.06's and carry no project, and
//! `Some(project)` is another host, whose names carry its normalized project.

use crate::HOLDER_RECORD_FILE;

/// The box's holder log, in the state folder.
const BOX_SSH_LOG: &str = "board-holder-ssh.log";

/// Whether `s` is a normalized project name (lease-api.md, Project). It MUST be the host
/// registry's own rule, so it is `agentspace_hosts::is_normalized_project` itself, never a copy
/// that could drift from it. Lowercase only: a caller lowercases first where a name is compared
/// without regard to case.
pub fn is_project_name(s: &str) -> bool {
    agentspace_hosts::is_normalized_project(s)
}

/// The holder record's file name for a host: `board-holder.json` for the box (`None`),
/// `board-holder.<project>.json` for another host.
pub fn holder_record_file(project: Option<&str>) -> String {
    match project {
        None => HOLDER_RECORD_FILE.to_string(),
        Some(p) => format!("board-holder.{p}.json"),
    }
}

/// The holder record's temporary file for a host and one write's `<hex>`:
/// `board-holder.json.<hex>.tmp` for the box, `board-holder.<project>.json.<hex>.tmp` for
/// another host.
pub fn holder_record_tmp_file(project: Option<&str>, hex: &str) -> String {
    format!("{}.{hex}.tmp", holder_record_file(project))
}

/// The holder's log for a host: `board-holder-ssh.log` for the box,
/// `board-holder-ssh.<project>.log` for another host.
pub fn holder_ssh_log_file(project: Option<&str>) -> String {
    match project {
        None => BOX_SSH_LOG.to_string(),
        Some(p) => format!("board-holder-ssh.{p}.log"),
    }
}

/// The holder's log rotation for a host: the log's name and `.1`.
pub fn holder_ssh_log_rotated_file(project: Option<&str>) -> String {
    format!("{}.1", holder_ssh_log_file(project))
}

/// The project of a file named `board-holder.<project>.json`, in lowercase, or `None` for any
/// other name (keeper.md, The hosts of a pass, item 3). The name is compared without regard to
/// case, and `<project>` MUST match the normalized project's pattern. The box's own
/// `board-holder.json` has no project and so is `None`. A caller MUST ignore a project that is the
/// box's project: that file is not a record, and the keeper never reads, writes or removes it.
pub fn project_of_holder_record_file(name: &str) -> Option<String> {
    const PREFIX: &str = "board-holder.";
    const SUFFIX: &str = ".json";
    if !name.is_ascii() {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    let project = lower.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    is_project_name(project).then(|| project.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_keeps_v006_names() {
        assert_eq!(holder_record_file(None), "board-holder.json");
        assert_eq!(holder_ssh_log_file(None), "board-holder-ssh.log");
        assert_eq!(holder_ssh_log_rotated_file(None), "board-holder-ssh.log.1");
        assert_eq!(
            holder_record_tmp_file(None, "ab12"),
            "board-holder.json.ab12.tmp"
        );
    }

    #[test]
    fn another_host_is_named_by_its_project() {
        let p = Some("demo-1");
        assert_eq!(holder_record_file(p), "board-holder.demo-1.json");
        assert_eq!(holder_ssh_log_file(p), "board-holder-ssh.demo-1.log");
        assert_eq!(
            holder_ssh_log_rotated_file(p),
            "board-holder-ssh.demo-1.log.1"
        );
        assert_eq!(
            holder_record_tmp_file(p, "ab12"),
            "board-holder.demo-1.json.ab12.tmp"
        );
    }

    #[test]
    fn a_project_name_is_the_normalized_pattern() {
        for ok in ["a", "9", "ab", "a-b", &"a".repeat(32)] {
            assert!(is_project_name(ok), "{ok}");
        }
        for bad in [
            "",
            "-a",
            "a-",
            "-",
            "A",
            "a_b",
            "a.b",
            "a\n",
            &"a".repeat(33),
        ] {
            assert!(!is_project_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_record_name_gives_its_project_in_lowercase() {
        assert_eq!(
            project_of_holder_record_file("board-holder.demo.json").as_deref(),
            Some("demo")
        );
        assert_eq!(
            project_of_holder_record_file("Board-Holder.DEMO.JSON").as_deref(),
            Some("demo")
        );
    }

    #[test]
    fn a_name_that_is_no_record_gives_no_project() {
        for n in [
            "board-holder.json",
            "board-holder..json",
            "board-holder.-a.json",
            "board-holder.a_b.json",
            "board-holder.a.b.json",
            "board-holder.demo.json.ab12.tmp",
            "board-holder.lock",
            "board-holder.log",
            "board-holder-ssh.demo.log",
            "board-holder-launch.ab12.json",
            "board-holder.dé.json",
            "",
        ] {
            assert_eq!(project_of_holder_record_file(n), None, "{n}");
        }
    }
}

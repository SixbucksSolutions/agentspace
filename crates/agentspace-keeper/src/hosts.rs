//! Hosts: the host registry, which host a lease is on, and the other hosts' files the state folder
//! holds (docs/keeper.md, Hosts; Which host a lease is on; The hosts of a pass; The holder's log).

use agentspace_hosts::{BOX_PROJECT, Hosts, HostsError, Target, is_box, target_for};
use agentspace_keeper_files::{is_project_name, project_of_holder_record_file};
use std::path::Path;

/// A host, as a pass and a keeper key it: `None` for the box, `Some(project)` for another host.
/// Its order is the order of a pass's turns: the box first, then the others in the ordinal order
/// of their projects.
pub(crate) type HostKey = Option<String>;

/// The registry, read whole, without its lock, from `<home>\.agentspace\hosts.json` (keeper.md,
/// Hosts). A missing file is the box alone. The error is the `<error>` of keeper.md's reason
/// `Host registry could not be read: <error>`.
pub(crate) fn read_registry(home: &Path) -> Result<Hosts, String> {
    agentspace_hosts::read(home).map_err(|e| match e {
        // A file that is not valid, and something there that is not a file or cannot be opened
        // or read, which every reader treats as a file that is not valid (state.md, The host
        // registry).
        // Architect's to confirm: both read as exactly `the host registry is not valid`, keeper.md's
        // `<error>` for a file state.md calls not valid, so the line says nothing more of why.
        HostsError::NotValid(_) | HostsError::Unreadable(_) => {
            "the host registry is not valid".to_string()
        }
        other => other.to_string(),
    })
}

/// The box's project: the `project` of the registry's record whose stack is `agentbox`, which a
/// valid registry always holds and which is always `swimcoach` (state.md, The host registry).
pub(crate) fn box_project(hosts: &Hosts) -> String {
    hosts
        .hosts
        .iter()
        .find(|h| is_box(h))
        .map_or_else(|| BOX_PROJECT.to_string(), |h| h.project.clone())
}

/// The host a ready lease of `project` is on (keeper.md, Which host a lease is on; D55's last
/// row): the project's host when its record is `ready` and is not the box's, and the box
/// otherwise.
pub(crate) fn host_of(hosts: &Hosts, project: &str) -> HostKey {
    match target_for(hosts, project) {
        Target::Host(p) => Some(p.to_string()),
        Target::Box => None,
    }
}

/// The names in the state folder, those that are not UTF-8 left out. The error is the
/// `<error>` of `The pass stopped on an error: <error>`.
// Architect's to confirm: that `<error>` is `The state folder '<path>' could not be listed:
// <the system's error>`, which keeper.md leaves unworded.
pub(crate) fn list_state_folder(state_dir: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(state_dir).map_err(|e| {
        format!(
            "The state folder '{}' could not be listed: {e}",
            state_dir.display()
        )
    })?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            format!(
                "The state folder '{}' could not be listed: {e}",
                state_dir.display()
            )
        })?;
        if let Ok(name) = entry.file_name().into_string() {
            names.push(name);
        }
    }
    Ok(names)
}

/// The projects of the other hosts' records among `names` (keeper.md, The hosts of a pass, item
/// 3): each `board-holder.<project>.json` whose `<project>`, taken in lowercase, is a normalized
/// project and not `box_project`. Sorted ordinally, each once.
pub(crate) fn record_projects(names: &[String], box_project: &str) -> Vec<String> {
    let mut projects: Vec<String> = names
        .iter()
        .filter_map(|n| project_of_holder_record_file(n))
        .filter(|p| p != box_project)
        .collect();
    projects.sort_unstable();
    projects.dedup();
    projects
}

/// The project of a file named `board-holder-ssh.<project>.log`, in lowercase, or `None` for any
/// other name: the name compared without regard to case, and `<project>` a normalized project.
fn project_of_ssh_log_file(name: &str) -> Option<String> {
    if !name.is_ascii() {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    let project = lower
        .strip_prefix("board-holder-ssh.")?
        .strip_suffix(".log")?;
    is_project_name(project).then(|| project.to_string())
}

/// The projects of the other hosts' holder logs among `names` (keeper.md, The holder's log): each
/// `board-holder-ssh.<project>.log` whose `<project>` is a normalized project and not the box's,
/// matched as a record's name is. Sorted ordinally, each once.
pub(crate) fn ssh_log_projects(names: &[String]) -> Vec<String> {
    let mut projects: Vec<String> = names
        .iter()
        .filter_map(|n| project_of_ssh_log_file(n))
        .filter(|p| p != BOX_PROJECT)
        .collect();
    projects.sort_unstable();
    projects.dedup();
    projects
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_hosts::{Host, HostState};

    fn registry(records: &[(&str, &str, HostState)]) -> Hosts {
        Hosts {
            hosts: records
                .iter()
                .map(|(project, stack, state)| Host {
                    project: (*project).to_string(),
                    stack: (*stack).to_string(),
                    state: *state,
                    instance_id: None,
                    last_attempt_at: None,
                })
                .collect(),
        }
    }

    #[test]
    fn a_lease_is_on_its_projects_ready_host_and_else_on_the_box() {
        let hosts = registry(&[
            ("swimcoach", "agentbox", HostState::Ready),
            ("demo", "agentspace-host-demo", HostState::Ready),
            ("made", "agentspace-host-made", HostState::Creating),
            ("bad", "agentspace-host-bad", HostState::Failed),
        ]);
        assert_eq!(host_of(&hosts, "demo"), Some("demo".to_string()));
        assert_eq!(host_of(&hosts, "swimcoach"), None);
        assert_eq!(host_of(&hosts, "made"), None);
        assert_eq!(host_of(&hosts, "bad"), None);
        assert_eq!(host_of(&hosts, "none"), None);
        assert_eq!(box_project(&hosts), "swimcoach");
        assert_eq!(host_of(&Hosts::seed(), "demo"), None);
    }

    #[test]
    fn records_and_logs_are_found_by_name_and_the_boxs_project_is_left_out() {
        let names: Vec<String> = [
            "board-holder.json",
            "board-holder.demo.json",
            "BOARD-HOLDER.Zed.JSON",
            "board-holder.swimcoach.json",
            "board-holder.-bad.json",
            "board-holder.a.b.json",
            "board-holder.demo.json.0a1b.tmp",
            "board-holder-ssh.log",
            "board-holder-ssh.log.1",
            "board-holder-ssh.demo.log",
            "board-holder-ssh.demo.log.1",
            "board-holder-ssh.SwimCoach.log",
            "board-holder-ssh.alpha.log",
            "leases.json",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        assert_eq!(record_projects(&names, "swimcoach"), vec!["demo", "zed"]);
        assert_eq!(ssh_log_projects(&names), vec!["alpha", "demo"]);
    }

    #[test]
    fn a_missing_registry_is_the_box_alone_and_a_folder_is_not_valid() {
        let home = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-hosts-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let hosts = read_registry(home.path()).expect("A missing registry is the seed");
        assert_eq!(hosts, Hosts::seed());
        std::fs::create_dir_all(home.path().join(".agentspace").join("hosts.json")).unwrap();
        assert_eq!(
            read_registry(home.path()),
            Err("the host registry is not valid".to_string())
        );
    }
}

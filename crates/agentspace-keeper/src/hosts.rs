//! Hosts: the host registry, which host a lease is on, the order of a pass's turns, and the other
//! hosts' files the state folder holds (docs/keeper.md, Hosts; A host's key and name; Which host a
//! lease is on; The hosts of a pass; The holder's log).
//!
//! RFC 2119 keywords in this module's comments, and the capitals are load-bearing.
//!
//! **From v0.12 a host is keyed by its project and its platform** (D91), as
//! `agentspace_hosts::HostKey` (D101), and the keeper holds one holder per host. The box is
//! swimcoach's `linux/arm64` host, [`HostKey::is_box`]: a valid registry's one `agentbox` record
//! is always swimcoach's, with no platform but `linux/arm64` (state.md, The host registry), so the
//! key alone tells the box from every other host.

use agentspace_hosts::{BOX_PROJECT, HostKey, Hosts, HostsError, Target, target_for_keyed};
use agentspace_keeper_files::{host_of_holder_record_file, host_of_holder_ssh_log_file};
use std::collections::HashSet;
use std::path::Path;

/// The registry, read whole, without its lock, from `<home>\.agentspace\hosts.json` (keeper.md,
/// Hosts). A missing file is the box alone. The error is the `<error>` of keeper.md's reason
/// `Host registry could not be read: <error>`.
///
/// A registry that is not valid by state.md's rules is `the host registry is not valid`. One that
/// cannot be opened or read (access denied, a folder at its path) MUST NOT be reported as not
/// valid: it is `Windows error <n>.`, the number alone, never the system's localized text, so the
/// line reads `Host registry could not be read: Windows error <n>.` (keeper.md, Hosts and Reasons;
/// architect, 2026-11-21, for v0.07-T03, on #0154's review).
pub(crate) fn read_registry(home: &Path) -> Result<Hosts, String> {
    agentspace_hosts::read(home).map_err(|e| match e {
        HostsError::NotValid(_) => "the host registry is not valid".to_string(),
        HostsError::Unreadable(_) => unreadable_error(home),
        other => other.to_string(),
    })
}

/// The `<error>` of a registry that could not be opened or read. agentspace-hosts' read keeps no
/// system error number, so the file is opened once more, read-only, for it; `0` stands in should
/// that open now succeed or give no number, since the text MUST NOT carry the system's message.
fn unreadable_error(home: &Path) -> String {
    let n = std::fs::read(agentspace_hosts::hosts_path(home))
        .err()
        .and_then(|e| e.raw_os_error())
        .unwrap_or(0);
    format!("Windows error {n}.")
}

/// The box's key: swimcoach's `linux/arm64` host (D43, D55, D91).
pub(crate) fn box_key() -> HostKey {
    HostKey::linux_arm64(BOX_PROJECT)
}

/// The host a ready lease of `project` on `platform` is on, or `None` when it is on no host
/// (keeper.md, Which host a lease is on; D55's last row; architect, 2026-11-26, for v0.12-T01,
/// pin P13): the host keyed by the lease's project and platform when the registry's record of that
/// key is `ready` and is not the box's; otherwise the box for a `linux/arm64` lease, and no host
/// for a lease of any other platform. Both are compared exactly, as stored.
pub(crate) fn host_of(hosts: &Hosts, project: &str, platform: &str) -> Option<HostKey> {
    let key = HostKey::new(project, platform);
    match target_for_keyed(hosts, &key)? {
        Target::Host(_) => Some(key),
        Target::Box => Some(box_key()),
    }
}

/// The hosts of a pass, each once, in the order of their turns (keeper.md, The hosts of a pass;
/// A host's key and name; architect, 2026-11-26, for v0.12-T01, pin P46): the box first, always;
/// then every other host of `hosts` that the registry holds, in the order of its record in
/// `hosts.json`; then every one it does not hold, in the ordinal order of its host name. No rule
/// sorts a host the registry holds by its key.
pub(crate) fn turn_order(registry: &Hosts, hosts: &[HostKey]) -> Vec<HostKey> {
    let wanted: HashSet<&HostKey> = hosts.iter().collect();
    let mut order = vec![box_key()];
    let mut placed: HashSet<HostKey> = HashSet::from([box_key()]);
    for record in &registry.hosts {
        let key = record.key();
        if wanted.contains(&key) && placed.insert(key.clone()) {
            order.push(key);
        }
    }
    let mut rest: Vec<&HostKey> = hosts.iter().filter(|k| !placed.contains(*k)).collect();
    rest.sort_by_key(|k| k.host_name());
    for key in rest {
        if placed.insert(key.clone()) {
            order.push(key.clone());
        }
    }
    order
}

/// The names in the state folder, those that are not UTF-8 left out. The error is the
/// `<error>` of `The pass stopped on an error: <error>`: `The state folder could not be listed:
/// Windows error <n>.`, the number alone, never the system's localized text (keeper.md, The hosts
/// of a pass; architect, 2026-11-21, for v0.07-T03, on #0154's review).
pub(crate) fn list_state_folder(state_dir: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(state_dir).map_err(|e| listing_error(&e))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| listing_error(&e))?;
        if let Ok(name) = entry.file_name().into_string() {
            names.push(name);
        }
    }
    Ok(names)
}

/// The `<error>` of a listing that failed. Listing a folder fails only with a system error, so the
/// number is always there; `0` stands in should one ever lack it, since the text MUST NOT carry
/// the system's localized message.
fn listing_error(e: &std::io::Error) -> String {
    format!(
        "The state folder could not be listed: Windows error {}.",
        e.raw_os_error().unwrap_or(0)
    )
}

/// The hosts among `names` that `of` finds, the box's left out, each once, in the ordinal order
/// of their host names.
fn hosts_named(names: &[String], of: fn(&str) -> Option<HostKey>) -> Vec<HostKey> {
    let mut hosts: Vec<HostKey> = names
        .iter()
        .filter_map(|n| of(n))
        .filter(|k| !k.is_box())
        .collect();
    hosts.sort_by_key(HostKey::host_name);
    hosts.dedup();
    hosts
}

/// The other hosts' records among `names` (keeper.md, The hosts of a pass, item 3): each
/// `board-holder.<host name>.json` whose `<host name>`, taken in lowercase, is a project, or a
/// project, a `.` and `linux-amd64` or `windows-amd64`, and is not the box's. In the ordinal order
/// of their host names, each once.
pub(crate) fn record_hosts(names: &[String]) -> Vec<HostKey> {
    hosts_named(names, host_of_holder_record_file)
}

/// The other hosts' holder logs among `names` (keeper.md, The holder's log): each
/// `board-holder-ssh.<host name>.log` whose `<host name>` is a host's name and not the box's,
/// matched as a record's name is, in the ordinal order of the host names, each once. The registry
/// is not read yet when they are rotated, so its file order cannot order them.
pub(crate) fn ssh_log_hosts(names: &[String]) -> Vec<HostKey> {
    hosts_named(names, host_of_holder_ssh_log_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_hosts::{Host, HostState};

    fn registry(records: &[(&str, &str, HostState)]) -> Hosts {
        registry_on(
            &records
                .iter()
                .map(|(p, s, st)| (*p, None, *s, *st))
                .collect::<Vec<_>>(),
        )
    }

    fn registry_on(records: &[(&str, Option<&str>, &str, HostState)]) -> Hosts {
        Hosts {
            hosts: records
                .iter()
                .map(|(project, platform, stack, state)| Host {
                    last_success_at: None,
                    platform: platform.map(str::to_string),
                    project: (*project).to_string(),
                    stack: (*stack).to_string(),
                    state: *state,
                    instance_id: None,
                    instance_type: None,
                    last_attempt_at: None,
                })
                .collect(),
        }
    }

    fn arm(p: &str) -> Option<HostKey> {
        Some(HostKey::linux_arm64(p))
    }

    #[test]
    fn a_linux_arm64_lease_is_on_its_projects_ready_host_and_else_on_the_box() {
        let hosts = registry(&[
            ("swimcoach", "agentbox", HostState::Ready),
            ("demo", "agentspace-host-demo", HostState::Ready),
            ("made", "agentspace-host-made", HostState::Creating),
            ("bad", "agentspace-host-bad", HostState::Failed),
        ]);
        let on = |p: &str| host_of(&hosts, p, "linux/arm64");
        assert_eq!(on("demo"), arm("demo"));
        assert_eq!(on("swimcoach"), Some(box_key()));
        assert_eq!(on("made"), Some(box_key()));
        assert_eq!(on("bad"), Some(box_key()));
        assert_eq!(on("none"), Some(box_key()));
        assert_eq!(
            host_of(&Hosts::seed(), "demo", "linux/arm64"),
            Some(box_key())
        );
        assert!(box_key().is_box());
    }

    /// keeper.md, Which host a lease is on: a lease is on its own platform's host, and the box
    /// fallback is `linux/arm64`'s alone (D91; pin P13).
    #[test]
    fn a_lease_is_on_its_own_platforms_host_and_another_platform_never_on_the_box() {
        let hosts = registry_on(&[
            ("swimcoach", None, "agentbox", HostState::Ready),
            ("demo", None, "agentspace-host-demo", HostState::Ready),
            (
                "demo",
                Some("windows/amd64"),
                "agentspace-windows-amd64-host-demo",
                HostState::Ready,
            ),
            (
                "demo",
                Some("linux/amd64"),
                "agentspace-linux-amd64-host-demo",
                HostState::Creating,
            ),
            (
                "swimcoach",
                Some("windows/amd64"),
                "agentspace-windows-amd64-host-swimcoach",
                HostState::Ready,
            ),
        ]);
        assert_eq!(host_of(&hosts, "demo", "linux/arm64"), arm("demo"));
        assert_eq!(
            host_of(&hosts, "demo", "windows/amd64"),
            Some(HostKey::new("demo", "windows/amd64"))
        );
        assert_eq!(host_of(&hosts, "demo", "linux/amd64"), None);
        assert_eq!(host_of(&hosts, "other", "windows/amd64"), None);
        assert_eq!(host_of(&hosts, "demo", "linux/amd64/v3"), None);
        assert_eq!(
            host_of(&hosts, "swimcoach", "windows/amd64"),
            Some(HostKey::new("swimcoach", "windows/amd64"))
        );
        assert_eq!(host_of(&hosts, "swimcoach", "linux/arm64"), Some(box_key()));
        assert_eq!(host_of(&Hosts::seed(), "demo", "windows/amd64"), None);
        // A record with an explicit linux/arm64 is the same host as one without the member.
        let explicit = registry_on(&[
            ("swimcoach", None, "agentbox", HostState::Ready),
            (
                "demo",
                Some("linux/arm64"),
                "agentspace-host-demo",
                HostState::Ready,
            ),
        ]);
        assert_eq!(host_of(&explicit, "demo", "linux/arm64"), arm("demo"));
    }

    /// keeper.md, The hosts of a pass (pin P46): the box first, then the registry's hosts in its
    /// file order, then the rest by host name.
    #[test]
    fn the_turns_follow_the_registrys_file_order_then_host_names() {
        let hosts = registry_on(&[
            (
                "q",
                Some("linux/amd64"),
                "agentspace-linux-amd64-host-q",
                HostState::Ready,
            ),
            ("swimcoach", None, "agentbox", HostState::Ready),
            ("p", None, "agentspace-host-p", HostState::Ready),
            ("q", None, "agentspace-host-q", HostState::Creating),
            (
                "p",
                Some("linux/amd64"),
                "agentspace-linux-amd64-host-p",
                HostState::Ready,
            ),
        ]);
        let p_amd = HostKey::new("p", "linux/amd64");
        let q_amd = HostKey::new("q", "linux/amd64");
        let z_win = HostKey::new("z", "windows/amd64");
        let wanted = vec![
            z_win.clone(),
            HostKey::linux_arm64("y"),
            p_amd.clone(),
            HostKey::linux_arm64("q"),
            box_key(),
            HostKey::linux_arm64("p"),
            q_amd.clone(),
            HostKey::linux_arm64("p"),
        ];
        assert_eq!(
            turn_order(&hosts, &wanted),
            vec![
                box_key(),
                q_amd,
                HostKey::linux_arm64("p"),
                HostKey::linux_arm64("q"),
                p_amd,
                HostKey::linux_arm64("y"),
                z_win,
            ]
        );
        assert_eq!(turn_order(&Hosts::seed(), &[]), vec![box_key()]);
    }

    #[test]
    fn records_and_logs_are_found_by_host_name_and_the_boxs_is_left_out() {
        let names: Vec<String> = [
            "board-holder.json",
            "board-holder.demo.json",
            "BOARD-HOLDER.Zed.JSON",
            "board-holder.swimcoach.json",
            "board-holder.swimcoach.windows-amd64.json",
            "board-holder.demo.windows-amd64.json",
            "board-holder.demo.linux-arm64.json",
            "board-holder.demo.windows-arm64.json",
            "board-holder.demo.windows-amd64.extra.json",
            "board-holder.demo.-amd64.json",
            "board-holder.-bad.json",
            "board-holder.demo.json.0a1b.tmp",
            "board-holder-ssh.log",
            "board-holder-ssh.log.1",
            "board-holder-ssh.demo.log",
            "board-holder-ssh.demo.log.1",
            "board-holder-ssh.SwimCoach.log",
            "board-holder-ssh.alpha.log",
            "board-holder-ssh.alpha.linux-amd64.log",
            "board-holder-ssh.alpha.linux-arm64.log",
            "leases.json",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        assert_eq!(
            record_hosts(&names),
            vec![
                HostKey::linux_arm64("demo"),
                HostKey::new("demo", "windows/amd64"),
                HostKey::new("swimcoach", "windows/amd64"),
                HostKey::linux_arm64("zed"),
            ]
        );
        assert_eq!(
            ssh_log_hosts(&names),
            vec![
                HostKey::linux_arm64("alpha"),
                HostKey::new("alpha", "linux/amd64"),
                HostKey::linux_arm64("demo"),
            ]
        );
    }

    #[test]
    fn a_missing_registry_is_the_box_alone() {
        let home = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-hosts-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let hosts = read_registry(home.path()).expect("A missing registry is the seed");
        assert_eq!(hosts, Hosts::seed());
    }

    /// keeper.md, Hosts: a registry that is not valid by state.md's rules is
    /// `the host registry is not valid`.
    #[test]
    fn a_registry_that_is_not_valid_is_named_not_valid() {
        let home = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-hosts-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let dir = home.path().join(".agentspace");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("hosts.json"), b"not json").unwrap();
        assert_eq!(
            read_registry(home.path()),
            Err("the host registry is not valid".to_string())
        );
    }

    /// keeper.md, Hosts: a registry that cannot be opened or read MUST NOT be called not valid; it
    /// is named by the Windows error number alone (architect, 2026-11-21, for v0.07-T03).
    #[cfg(windows)]
    #[test]
    fn a_registry_that_cannot_be_read_is_named_by_the_windows_error_number_alone() {
        let home = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-hosts-")
            .tempdir()
            .expect("A throwaway folder can be made");
        // A folder where the file belongs: opening it is access denied.
        std::fs::create_dir_all(home.path().join(".agentspace").join("hosts.json")).unwrap();
        assert_eq!(
            read_registry(home.path()),
            Err("Windows error 5.".to_string())
        );
    }

    /// keeper.md, The hosts of a pass: an unlistable state folder's `<error>` names the Windows
    /// error by its number alone, never the system's localized text.
    #[cfg(windows)]
    #[test]
    fn an_unlistable_state_folder_is_named_by_the_windows_error_number_alone() {
        let root = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-list-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let missing = root.path().join("no-such-folder");
        assert_eq!(
            list_state_folder(&missing),
            Err("The state folder could not be listed: Windows error 3.".to_string())
        );
        let file = root.path().join("a-file");
        std::fs::write(&file, b"").expect("A throwaway file can be written");
        assert_eq!(
            list_state_folder(&file),
            Err("The state folder could not be listed: Windows error 267.".to_string())
        );
    }
}

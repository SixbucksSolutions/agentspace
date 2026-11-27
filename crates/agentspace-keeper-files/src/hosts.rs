//! Each host's holder files, by D55's names (state.md, Another host's holder).
//!
//! RFC 2119 keywords in this module's comments, and the capitals are load-bearing.
//!
//! - **The project-only functions** name a host by `Option<&str>`: `None` is the box, whose names
//!   are v0.06's and carry no project, and `Some(project)` is that project's `linux/arm64` host,
//!   whose names carry its normalized project. Every one keeps the signature and the names it had
//!   before v0.12.
//! - **The `_keyed` functions** name a host by its `agentspace_hosts::HostKey`, its project and
//!   its platform (D91, D101), and from v0.12 by its host name: `<project>` on `linux/arm64`, and
//!   `<project>.<os>-<arch>` on any other platform (state.md, A host's platform and names;
//!   keeper.md, A host's key and name; architect, 2026-11-26, for v0.12-T01, pin P12). The box,
//!   swimcoach's `linux/arm64` host, keeps v0.06's names, so every `linux/arm64` name is byte for
//!   byte what the project-only function gives.

use crate::HOLDER_RECORD_FILE;
use agentspace_hosts::HostKey;

/// The box's holder log, in the state folder.
const BOX_SSH_LOG: &str = "board-holder-ssh.log";

/// The platform part a host name MAY carry, and the platform each names: the two platforms v0.12
/// supports besides `linux/arm64`, which never appears in a name (keeper.md, The hosts of a pass,
/// item 3; architect, 2026-11-26, for v0.12-T01, pins P1, P12 and P21).
const PLATFORM_PARTS: [(&str, &str); 2] = [
    ("linux-amd64", "linux/amd64"),
    ("windows-amd64", "windows/amd64"),
];

/// The host a host name names, or `None` when it names none (keeper.md, A host's key and name;
/// The hosts of a pass, item 3): `<project>` is that project's `linux/arm64` host, and
/// `<project>.<part>`, where `<part>` is `linux-amd64` or `windows-amd64`, that project's host on
/// `linux/amd64` or `windows/amd64`. `<project>` MUST match the normalized project's pattern. Any
/// other platform part, `linux-arm64` among them, names no host, and neither does an unsupported
/// platform (pin P21). The name is taken as given: a caller lowercases it first where a name is
/// compared without regard to case. The box's project alone gives the box's key, which a caller
/// that finds it in a file's name MUST ignore, since the box's files carry no host name.
pub fn host_of_host_name(name: &str) -> Option<HostKey> {
    match name.split_once('.') {
        None => is_project_name(name).then(|| HostKey::linux_arm64(name)),
        Some((project, part)) => {
            if !is_project_name(project) {
                return None;
            }
            PLATFORM_PARTS
                .iter()
                .find(|(written, _)| *written == part)
                .map(|(_, platform)| HostKey::new(project, *platform))
        }
    }
}

/// The holder record's file name for the host `host`: `board-holder.json` for the box,
/// `board-holder.<host name>.json` for every other host, so `board-holder.<project>.json` for a
/// `linux/arm64` host and `board-holder.<project>.<os>-<arch>.json` for a host of another platform
/// (state.md, Another host's holder; pin P12).
pub fn holder_record_file_keyed(host: &HostKey) -> String {
    if host.is_box() {
        HOLDER_RECORD_FILE.to_string()
    } else {
        format!("board-holder.{}.json", host.host_name())
    }
}

/// The holder record's temporary file for the host `host` and one write's `<hex>`: its record's
/// name, then `.<hex>.tmp`.
pub fn holder_record_tmp_file_keyed(host: &HostKey, hex: &str) -> String {
    format!("{}.{hex}.tmp", holder_record_file_keyed(host))
}

/// The holder's log for the host `host`: `board-holder-ssh.log` for the box,
/// `board-holder-ssh.<host name>.log` for every other host (pin P12).
pub fn holder_ssh_log_file_keyed(host: &HostKey) -> String {
    if host.is_box() {
        BOX_SSH_LOG.to_string()
    } else {
        format!("board-holder-ssh.{}.log", host.host_name())
    }
}

/// The holder's log rotation for the host `host`: the log's name and `.1`.
pub fn holder_ssh_log_rotated_file_keyed(host: &HostKey) -> String {
    format!("{}.1", holder_ssh_log_file_keyed(host))
}

/// The host of a file named `<prefix><host name><suffix>`, the name compared without regard to
/// case and `<host name>` taken in lowercase.
fn host_of_file(name: &str, prefix: &str, suffix: &str) -> Option<HostKey> {
    if !name.is_ascii() {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    host_of_host_name(lower.strip_prefix(prefix)?.strip_suffix(suffix)?)
}

/// The host of a file named `board-holder.<host name>.json`, or `None` for any other name
/// (keeper.md, The hosts of a pass, item 3; architect, 2026-11-26, for v0.12-T01, pins P12 and
/// P21). The name is compared without regard to case, and `<host name>` is taken in lowercase and
/// read by [`host_of_host_name`]. So `board-holder.foo.linux-arm64.json` is no record. The box's
/// own `board-holder.json` holds no host name and so is `None`. **A caller MUST ignore a file that
/// gives the box's key**, `board-holder.<box's project>.json`: that file is not a record, and the
/// keeper never reads, writes or removes it. A file of the box's project on another platform is
/// a record like any other host's.
pub fn host_of_holder_record_file(name: &str) -> Option<HostKey> {
    host_of_file(name, "board-holder.", ".json")
}

/// The host of a file named `board-holder-ssh.<host name>.log`, or `None` for any other name,
/// matched as [`host_of_holder_record_file`] matches a record's name (keeper.md, The holder's
/// log). A caller MUST ignore a file that gives the box's key, as for a record.
pub fn host_of_holder_ssh_log_file(name: &str) -> Option<HostKey> {
    host_of_file(name, "board-holder-ssh.", ".log")
}

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

    /// state.md, A host's platform and names: every `linux/arm64` name is the project-only
    /// function's, the box's included, and another platform's carries its host name (pin P12).
    #[test]
    fn a_keyed_hosts_files_carry_its_host_name() {
        let boxed = HostKey::linux_arm64("swimcoach");
        assert_eq!(holder_record_file_keyed(&boxed), holder_record_file(None));
        assert_eq!(holder_ssh_log_file_keyed(&boxed), holder_ssh_log_file(None));
        assert_eq!(
            holder_ssh_log_rotated_file_keyed(&boxed),
            "board-holder-ssh.log.1"
        );
        assert_eq!(
            holder_record_tmp_file_keyed(&boxed, "ab"),
            "board-holder.json.ab.tmp"
        );
        let arm = HostKey::linux_arm64("demo");
        assert_eq!(
            holder_record_file_keyed(&arm),
            holder_record_file(Some("demo"))
        );
        assert_eq!(
            holder_ssh_log_file_keyed(&arm),
            holder_ssh_log_file(Some("demo"))
        );
        let win = HostKey::new("demo", "windows/amd64");
        assert_eq!(
            holder_record_file_keyed(&win),
            "board-holder.demo.windows-amd64.json"
        );
        assert_eq!(
            holder_record_tmp_file_keyed(&win, "ab"),
            "board-holder.demo.windows-amd64.json.ab.tmp"
        );
        assert_eq!(
            holder_ssh_log_file_keyed(&win),
            "board-holder-ssh.demo.windows-amd64.log"
        );
        assert_eq!(
            holder_ssh_log_rotated_file_keyed(&win),
            "board-holder-ssh.demo.windows-amd64.log.1"
        );
        // The box's project on another platform is another host, with names of its own.
        assert_eq!(
            holder_record_file_keyed(&HostKey::new("swimcoach", "linux/amd64")),
            "board-holder.swimcoach.linux-amd64.json"
        );
    }

    #[test]
    fn a_host_name_gives_its_project_and_platform() {
        assert_eq!(
            host_of_host_name("demo"),
            Some(HostKey::linux_arm64("demo"))
        );
        assert_eq!(
            host_of_host_name("demo.linux-amd64"),
            Some(HostKey::new("demo", "linux/amd64"))
        );
        assert_eq!(
            host_of_host_name("demo.windows-amd64"),
            Some(HostKey::new("demo", "windows/amd64"))
        );
        for bad in [
            "",
            "demo.linux-arm64",
            "demo.windows-arm64",
            "demo.windows-amd64.extra",
            "demo.-amd64",
            "demo.",
            ".windows-amd64",
            "Demo",
            "demo.Windows-amd64",
            "-a.linux-amd64",
            "demo.linux/amd64",
        ] {
            assert_eq!(host_of_host_name(bad), None, "{bad:?}");
        }
    }

    /// keeper.md, The hosts of a pass, item 3, and H24's names.
    #[test]
    fn a_files_name_gives_its_host_in_lowercase_or_none() {
        assert_eq!(
            host_of_holder_record_file("board-holder.demo.json"),
            Some(HostKey::linux_arm64("demo"))
        );
        assert_eq!(
            host_of_holder_record_file("BOARD-HOLDER.Demo.Windows-AMD64.JSON"),
            Some(HostKey::new("demo", "windows/amd64"))
        );
        assert_eq!(
            host_of_holder_record_file("board-holder.swimcoach.json"),
            Some(HostKey::linux_arm64("swimcoach"))
        );
        assert_eq!(
            host_of_holder_ssh_log_file("board-holder-ssh.demo.linux-amd64.log"),
            Some(HostKey::new("demo", "linux/amd64"))
        );
        for n in [
            "board-holder.json",
            "board-holder.demo.linux-arm64.json",
            "board-holder.demo.windows-arm64.json",
            "board-holder.demo.windows-amd64.extra.json",
            "board-holder.demo.-amd64.json",
            "board-holder.demo.windows-amd64.json.ab12.tmp",
            "board-holder-ssh.demo.windows-amd64.log",
            "board-holder.dé.json",
        ] {
            assert_eq!(host_of_holder_record_file(n), None, "{n}");
        }
        for n in [
            "board-holder-ssh.log",
            "board-holder-ssh.log.1",
            "board-holder-ssh.demo.windows-amd64.log.1",
            "board-holder-ssh.demo.linux-arm64.log",
            "board-holder.demo.json",
        ] {
            assert_eq!(host_of_holder_ssh_log_file(n), None, "{n}");
        }
    }
}

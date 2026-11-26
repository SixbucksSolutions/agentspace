//! A host's key, its project and its platform, and every name made from it (docs/state.md, "A
//! host's platform and names"; D91, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. **Every `linux/arm64` name is byte for byte
//! what it was before v0.12**: the functions that take a project alone call these with
//! [`HostKey::linux_arm64`].

use crate::model::{BOX_PROJECT, BOX_STACK, is_normalized_project};

/// The platform of every host before v0.12, and of a registry record without `platform` (D91,
/// D101). It never appears in a name.
pub const LINUX_ARM64: &str = "linux/arm64";

/// The prefix of every other `linux/arm64` host's stack (D44).
pub(crate) const STACK_PREFIX: &str = "agentspace-host-";

/// The prefix of a host's operator alias and of its key's name in agentspace's `known_hosts` (D47).
const ALIAS_PREFIX: &str = "agentspace-host.";

/// A host: its project and its platform (D101's `agentspace_hosts::HostKey`).
///
/// Both are held as given, in their normalized forms: the project in the lease contract's normalized
/// form, and the platform as `<os>/<arch>`, lowercase. **Nothing here normalizes either**: a stored
/// platform is never normalized (docs/state.md, "The platform is written in two forms"), and a
/// caller that has a lease's platform passes it as the lease records it. [`HostKey::is_valid`] says
/// whether a path MAY be made from the key; every function of this crate that makes a path or
/// writes a file checks it first.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostKey {
    /// The project the host serves.
    pub project: String,
    /// The platform the host serves, `<os>/<arch>`.
    pub platform: String,
}

impl HostKey {
    /// The host of `project` on `platform`, both used as given.
    pub fn new(project: impl Into<String>, platform: impl Into<String>) -> HostKey {
        HostKey {
            project: project.into(),
            platform: platform.into(),
        }
    }

    /// The `linux/arm64` host of `project`: the host every function that takes a project alone
    /// means (E0.12-T02-S01).
    pub fn linux_arm64(project: impl Into<String>) -> HostKey {
        HostKey::new(project, LINUX_ARM64)
    }

    /// Whether the platform is `linux/arm64`, compared exactly.
    pub fn is_linux_arm64(&self) -> bool {
        self.platform == LINUX_ARM64
    }

    /// Whether the key is the box's: swimcoach's `linux/arm64` host (D43, D55, D91).
    pub fn is_box(&self) -> bool {
        self.project == BOX_PROJECT && self.is_linux_arm64()
    }

    /// Whether a name or a path MAY be made from the key: the project is in the lease contract's
    /// normalized form, and the platform is a registry record's form, `^[a-z0-9]+/[a-z0-9]+$` with
    /// an architecture other than `aarch64` (docs/state.md, the member table's `platform`). A
    /// platform with a variant makes no host, so it makes no key that is valid.
    pub fn is_valid(&self) -> bool {
        is_normalized_project(&self.project) && is_host_platform(&self.platform)
    }

    /// The host's name (docs/state.md, "A host's name"): `<project>` on `linux/arm64`, and
    /// `<project>.<os>-<arch>` on any other platform, such as `agentspace.windows-amd64`.
    pub fn host_name(&self) -> String {
        if self.is_linux_arm64() {
            self.project.clone()
        } else {
            format!("{}.{}", self.project, platform_in_name(&self.platform))
        }
    }

    /// The host's CloudFormation stack (D44, D91): `agentbox` for the box, swimcoach's `linux/arm64`
    /// host; `agentspace-host-<project>` for every other `linux/arm64` host; and
    /// `agentspace-<os>-<arch>-host-<project>` on any other platform, swimcoach's included.
    pub fn stack_name(&self) -> String {
        if self.is_box() {
            BOX_STACK.to_string()
        } else if self.is_linux_arm64() {
            format!("{STACK_PREFIX}{}", self.project)
        } else {
            format!(
                "agentspace-{}-host-{}",
                platform_in_name(&self.platform),
                self.project
            )
        }
    }

    /// The host's operator alias, its file's name in `agentspace.d` and its `Host` name (D47,
    /// D91): `agentspace-host.<host name>`. The box has no operator alias; for its key this gives
    /// `agentspace-host.swimcoach`, as the project-only `host_alias` always has.
    pub fn operator_alias(&self) -> String {
        format!("{ALIAS_PREFIX}{}", self.host_name())
    }

    /// The name of the host's key in agentspace's `known_hosts`, and so a lease alias's
    /// `HostKeyAlias` on a host that is not the box: the same string as
    /// [`HostKey::operator_alias`], since D91's table gives one name for both.
    pub fn host_key_name(&self) -> String {
        self.operator_alias()
    }

    /// The name of the host's folder in `%USERPROFILE%\.agentspace\hosts`: its host name (D48, D55,
    /// D91).
    pub fn folder_name(&self) -> String {
        self.host_name()
    }
}

/// A normalized platform as a name writes it, `<os>-<arch>`: every `/` written `-` (D91). A
/// registry record's platform holds exactly one.
pub fn platform_in_name(platform: &str) -> String {
    platform.replace('/', "-")
}

/// Whether `s` is a platform in a registry record's form (docs/state.md, the member table's
/// `platform`): `^[a-z0-9]+/[a-z0-9]+$`, with no variant, and not one whose architecture is
/// `aarch64`, which normalizes to `arm64`.
pub fn is_host_platform(s: &str) -> bool {
    let plain = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    };
    match s.split_once('/') {
        Some((os, arch)) => plain(os) && plain(arch) && arch != "aarch64",
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linux_arm64_hosts_names_are_byte_for_byte_v011s() {
        let k = HostKey::linux_arm64("agentspace");
        assert_eq!(k.platform, "linux/arm64");
        assert_eq!(k.host_name(), "agentspace");
        assert_eq!(k.stack_name(), "agentspace-host-agentspace");
        assert_eq!(k.operator_alias(), "agentspace-host.agentspace");
        assert_eq!(k.host_key_name(), "agentspace-host.agentspace");
        assert_eq!(k.folder_name(), "agentspace");
        let b = HostKey::linux_arm64("swimcoach");
        assert!(b.is_box());
        assert_eq!(b.stack_name(), "agentbox");
        assert_eq!(b.folder_name(), "swimcoach");
    }

    #[test]
    fn another_platforms_names_carry_it_written_with_a_dash() {
        let w = HostKey::new("agentspace", "windows/amd64");
        assert_eq!(w.host_name(), "agentspace.windows-amd64");
        assert_eq!(w.stack_name(), "agentspace-windows-amd64-host-agentspace");
        assert_eq!(
            w.operator_alias(),
            "agentspace-host.agentspace.windows-amd64"
        );
        assert_eq!(w.host_key_name(), w.operator_alias());
        assert_eq!(w.folder_name(), "agentspace.windows-amd64");
        let l = HostKey::new("swimcoach", "linux/amd64");
        assert!(!l.is_box());
        assert_eq!(l.stack_name(), "agentspace-linux-amd64-host-swimcoach");
        assert_eq!(l.host_name(), "swimcoach.linux-amd64");
        assert_eq!(platform_in_name("linux/amd64"), "linux-amd64");
    }

    #[test]
    fn the_longest_names_stay_inside_their_limits() {
        let k = HostKey::new("a".repeat(32), "windows/amd64");
        assert_eq!(k.stack_name().len(), 62);
        assert_eq!(k.operator_alias().len(), 62);
    }

    #[test]
    fn a_key_is_valid_only_with_a_normalized_project_and_a_records_platform() {
        for ok in ["linux/arm64", "linux/amd64", "windows/amd64", "example/one"] {
            assert!(HostKey::new("p", ok).is_valid(), "{ok}");
        }
        for bad in [
            "",
            "linux",
            "linux/",
            "/arm64",
            "Linux/arm64",
            "linux/amd64/v3",
            "linux/aarch64",
            "linux/arm64\n",
            "linux-arm64",
            "../x",
            "a/b/c",
        ] {
            assert!(!HostKey::new("p", bad).is_valid(), "{bad:?}");
        }
        assert!(!HostKey::new("P", "linux/amd64").is_valid());
        assert!(!HostKey::new("a.b", "linux/amd64").is_valid());
    }
}

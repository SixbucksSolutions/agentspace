//! What every ssh and scp of `sync` and `bootstrap` carries (docs/operator.md, Every ssh runs the
//! ProxyCommand).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The remote commands of `sync`'s call table and the host's paths (operator.md, sync, rows 1 to
//! 8) are the platform seam's sync row (`HostPlatform::sync_plan`; `agentspace-platforms` for both
//! Linux platforms, E0.12-T03-S03): this crate keeps no copy of them. Each Linux command MUST hold
//! no `%`, no double quote and no control character, and MUST NOT end in a backslash (gap FA-15),
//! so it reaches ssh whole as one argument, the last; the tests below check the row's text.

/// Every ssh and scp `sync` starts clears the alias's forwards (operator.md, Every ssh runs the
/// ProxyCommand).
pub(crate) const CLEAR_FORWARDS: &str = "ClearAllForwardings=yes";

#[cfg(test)]
mod tests {
    use agentspace_operator_api::{HostPlatform, SyncPlan};

    /// The seam's `linux/arm64` sync row, which carries the commands this file held before
    /// E0.12-T03-S03 part 2 (pin P1).
    fn linux() -> SyncPlan {
        agentspace_platforms::LINUX_ARM64.sync_plan().unwrap()
    }

    #[test]
    fn every_remote_command_keeps_fa15s_rule() {
        let plan = linux();
        for command in [
            plan.clear_command.as_deref().unwrap(),
            plan.install_command.as_str(),
            plan.claude_folder_command.as_str(),
            plan.localswim_install_command.as_str(),
        ] {
            assert!(!command.contains('%'), "{command}");
            assert!(!command.contains('"'), "{command}");
            assert!(!command.chars().any(char::is_control), "{command}");
            assert!(!command.ends_with('\\'), "{command}");
            assert!(!command.contains("  "), "{command}");
        }
    }

    #[test]
    fn the_install_removes_the_staging_folder_only_after_it_succeeded() {
        let plan = linux();
        let (install, removal) = plan.install_command.split_once(" && ").unwrap();
        assert!(install.contains("bash /tmp/agentspace-system/install.sh"));
        assert!(removal.contains("rm -rf -- /tmp/agentspace-system"));
        let parts: Vec<&str> = plan.localswim_install_command.split(" && ").collect();
        assert_eq!(parts.len(), 3);
        assert!(parts[2].contains("rm -f -- /tmp/localswim"));
    }
}

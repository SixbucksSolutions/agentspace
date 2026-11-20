//! What `sync` asks of the box: the remote commands of its call table (docs/operator.md, sync,
//! rows 1, 4, 5 and 8), and the box's paths, all today's box's.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The text of each command is the operator's own, written from what the table says the remote
//! step MUST do (D34), never copied from the prototype. Each MUST hold no `%`, no double quote and
//! no control character, and MUST NOT end in a backslash (gap FA-15), so it reaches ssh whole as
//! one argument, the last. A test checks each. Every command that runs as root does so through
//! `sudo -n`, which never prompts: it fails at once where a password would be needed.

/// Every ssh and scp `sync` starts clears the alias's forwards (operator.md, Every ssh runs the
/// ProxyCommand).
pub(crate) const CLEAR_FORWARDS: &str = "ClearAllForwardings=yes";

/// The box's staging folder.
pub(crate) const STAGING: &str = "/tmp/agentspace-system";

/// The box's staging path for the localswim CLI.
pub(crate) const LOCALSWIM_STAGING: &str = "/tmp/localswim";

/// Row 1: remove the staging folder and everything in it. A folder that is not there is not an
/// error; exit 0 only when it is gone. As root, so that nothing an earlier install left in it can
/// keep it there.
pub(crate) const REMOVE_STAGING: &str =
    "sudo -n rm -rf -- /tmp/agentspace-system && test ! -e /tmp/agentspace-system";

/// Row 4: as root, run the staged install with bash (a copy from Windows carries no execute bit),
/// then, only if it exited 0, remove the staging folder. Exits with the install's status, or the
/// removal's.
pub(crate) const INSTALL: &str = "sudo -n bash /tmp/agentspace-system/install.sh \
&& sudo -n rm -rf -- /tmp/agentspace-system";

/// Row 5: as the login user, make sure `~/.claude` exists, with its parents. A folder already there
/// is left as it is; a new one gets the user's umask.
pub(crate) const MAKE_CLAUDE_FOLDER: &str = "mkdir -p -- ~/.claude";

/// Row 8: as root, install the staged binary at `/usr/local/lib/localswim/localswim`, owned by
/// root, mode 0755, creating its folder owned by root with mode 0755 (`/usr/local/lib` is the
/// image's own, root's and 0755, and is left as it is); then, only if that
/// succeeded, remove the staged copy. Exits non-zero if either failed.
pub(crate) const INSTALL_LOCALSWIM: &str = "sudo -n install -d -o root -g root -m 0755 \
/usr/local/lib/localswim \
&& sudo -n install -o root -g root -m 0755 /tmp/localswim /usr/local/lib/localswim/localswim \
&& sudo -n rm -f -- /tmp/localswim";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_remote_command_keeps_fa15s_rule() {
        for command in [
            REMOVE_STAGING,
            INSTALL,
            MAKE_CLAUDE_FOLDER,
            INSTALL_LOCALSWIM,
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
        let (install, removal) = INSTALL.split_once(" && ").unwrap();
        assert!(install.contains("bash /tmp/agentspace-system/install.sh"));
        assert!(removal.contains("rm -rf -- /tmp/agentspace-system"));
        let parts: Vec<&str> = INSTALL_LOCALSWIM.split(" && ").collect();
        assert_eq!(parts.len(), 3);
        assert!(parts[2].contains("rm -f -- /tmp/localswim"));
    }
}

//! Every path this crate's files have, made from the home folder (docs/state.md, The home folder).
//!
//! `home` is the value of `USERPROFILE`, made full by the caller, and none of these functions reads
//! the environment or the disk. `AGENTSPACE_STATE_DIR` moves none of them (docs/state.md, What moves
//! which files). A function that takes a project makes a path from it as given: a caller that has
//! not checked the project is in its normalized form MUST do so first (`is_normalized_project`),
//! since `..` would leave the host folder.

use std::path::{Path, PathBuf};

/// `<home>\.agentspace`, agentspace's own folder (D21).
pub fn agentspace_dir(home: &Path) -> PathBuf {
    home.join(".agentspace")
}

/// `<home>\.agentspace\hosts.json`, the host registry (D45).
pub fn hosts_path(home: &Path) -> PathBuf {
    agentspace_dir(home).join("hosts.json")
}

/// `<home>\.agentspace\hosts.lock`, the host registry lock.
pub fn hosts_lock_path(home: &Path) -> PathBuf {
    agentspace_dir(home).join("hosts.lock")
}

/// `<home>\.agentspace\guard.lock`, the guard's lock (D56, D63). `AGENTSPACE_STATE_DIR` does not
/// move it.
pub fn guard_lock_path(home: &Path) -> PathBuf {
    agentspace_dir(home).join("guard.lock")
}

/// `<home>\.agentspace\guard.log`, the guard's log (D56, D63). `AGENTSPACE_STATE_DIR` does not
/// move it.
pub fn guard_log_path(home: &Path) -> PathBuf {
    agentspace_dir(home).join("guard.log")
}

/// `<home>\.agentspace\projects.json`, the project list (D54).
pub fn projects_path(home: &Path) -> PathBuf {
    agentspace_dir(home).join("projects.json")
}

/// `<home>\.agentspace\known_hosts`, agentspace's own `known_hosts`, which only ssh writes (D47).
pub fn known_hosts_path(home: &Path) -> PathBuf {
    agentspace_dir(home).join("known_hosts")
}

/// `<home>\.agentspace\hosts`, the folder that holds every host's folder.
pub fn hosts_dir(home: &Path) -> PathBuf {
    agentspace_dir(home).join("hosts")
}

/// `<home>\.agentspace\hosts\<project>`, a host's folder.
pub fn host_dir(home: &Path, project: &str) -> PathBuf {
    hosts_dir(home).join(project)
}

/// A host's operator key, `id_ed25519` in its folder.
pub fn host_key_path(home: &Path, project: &str) -> PathBuf {
    host_dir(home, project).join("id_ed25519")
}

/// A host's operator certificate, `id_ed25519-cert.pub` in its folder.
pub fn host_cert_path(home: &Path, project: &str) -> PathBuf {
    host_dir(home, project).join("id_ed25519-cert.pub")
}

/// A host's provisioning run log, `provision.log` in its folder.
pub fn host_log_path(home: &Path, project: &str) -> PathBuf {
    host_dir(home, project).join("provision.log")
}

/// A host's provisioning run lock, `provision.lock` in its folder.
pub fn host_run_lock_path(home: &Path, project: &str) -> PathBuf {
    host_dir(home, project).join("provision.lock")
}

/// A host's push stamp, `localswim-pushed.json` in its folder.
pub fn host_stamp_path(home: &Path, project: &str) -> PathBuf {
    host_dir(home, project).join("localswim-pushed.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_path_is_where_state_md_puts_it() {
        let home = Path::new("h");
        let a = Path::new("h").join(".agentspace");
        assert_eq!(agentspace_dir(home), a);
        assert_eq!(hosts_path(home), a.join("hosts.json"));
        assert_eq!(hosts_lock_path(home), a.join("hosts.lock"));
        assert_eq!(guard_lock_path(home), a.join("guard.lock"));
        assert_eq!(guard_log_path(home), a.join("guard.log"));
        assert_eq!(projects_path(home), a.join("projects.json"));
        assert_eq!(known_hosts_path(home), a.join("known_hosts"));
        let d = a.join("hosts").join("p");
        assert_eq!(hosts_dir(home), a.join("hosts"));
        assert_eq!(host_dir(home, "p"), d);
        assert_eq!(host_key_path(home, "p"), d.join("id_ed25519"));
        assert_eq!(host_cert_path(home, "p"), d.join("id_ed25519-cert.pub"));
        assert_eq!(host_log_path(home, "p"), d.join("provision.log"));
        assert_eq!(host_run_lock_path(home, "p"), d.join("provision.lock"));
        assert_eq!(host_stamp_path(home, "p"), d.join("localswim-pushed.json"));
    }
}

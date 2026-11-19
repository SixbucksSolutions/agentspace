//! The folders (docs/state.md, The home folder and What moves which files).

use crate::store::StoreError;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// The folders the lease CLI works in, each a full path. A folder that needs the home folder and
/// has none is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// The value of `USERPROFILE`; `None` when it is unset or empty.
    pub home: Option<PathBuf>,
    /// `AGENTSPACE_STATE_DIR` when set and not empty, `<home>\.agentspace` otherwise.
    pub state_dir: Option<PathBuf>,
    /// `AGENTSPACE_SSH_DIR` when set and not empty, `<home>\.ssh` otherwise.
    pub ssh_dir: Option<PathBuf>,
}

/// A variable's value, with unset and empty alike meaning none.
fn non_empty(var: &dyn Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    var(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// A relative value is made full against the working folder, so that a file written later and read
/// by a program in another folder still names the same place (state.md, The home folder).
///
/// A value with a drive prefix or a root but not both (`C:rel`, `\rel`) is not absolute, and joining
/// it to `cwd` would drop `cwd` and leave it relative, so the system completes it with
/// `std::path::absolute` instead.
fn full(path: PathBuf, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    let has_prefix = matches!(path.components().next(), Some(Component::Prefix(_)));
    if (has_prefix || path.has_root())
        && let Ok(done) = std::path::absolute(&path)
    {
        return done;
    }
    cwd.join(path)
}

/// Finds the folders from the environment (state.md, The home folder; What moves which files).
///
/// `var` reads one environment variable, and `cwd` is the process's working folder. The home
/// folder is `USERPROFILE` alone, never the operating system's own idea of the profile. A relative
/// value is made full against `cwd`. Nothing is checked on the disk: a file where a folder must be
/// is reported where the folder is used (architect, 2026-11-19), by `read`, `lock` and `write` for
/// the state folder.
pub fn resolve_paths(
    var: &dyn Fn(&str) -> Option<OsString>,
    cwd: &Path,
) -> Result<Paths, StoreError> {
    let home = non_empty(var, "USERPROFILE").map(|p| full(p, cwd));
    let state_dir = match non_empty(var, "AGENTSPACE_STATE_DIR") {
        Some(p) => Some(full(p, cwd)),
        None => home.as_ref().map(|h| h.join(".agentspace")),
    };
    let ssh_dir = match non_empty(var, "AGENTSPACE_SSH_DIR") {
        Some(p) => Some(full(p, cwd)),
        None => home.as_ref().map(|h| h.join(".ssh")),
    };
    Ok(Paths {
        home,
        state_dir,
        ssh_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TempDir;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn unset_or_empty_home_is_none_and_so_are_the_folders_that_need_it() {
        let cwd = TempDir::new();
        for pairs in [vec![], vec![("USERPROFILE", "")]] {
            let p = resolve_paths(&env(&pairs), cwd.path()).unwrap();
            assert_eq!(
                p,
                Paths {
                    home: None,
                    state_dir: None,
                    ssh_dir: None
                }
            );
        }
    }

    #[test]
    fn the_home_comes_from_userprofile_and_the_folders_from_the_home() {
        let t = TempDir::new();
        let home = t.path().join("h");
        let p = resolve_paths(&env(&[("USERPROFILE", home.to_str().unwrap())]), t.path()).unwrap();
        assert_eq!(p.home, Some(home.clone()));
        assert_eq!(p.state_dir, Some(home.join(".agentspace")));
        assert_eq!(p.ssh_dir, Some(home.join(".ssh")));
    }

    #[test]
    fn a_relative_value_is_made_full_against_the_working_folder() {
        let t = TempDir::new();
        let p = resolve_paths(
            &env(&[
                ("USERPROFILE", "rel-home"),
                ("AGENTSPACE_SSH_DIR", "rel-ssh"),
            ]),
            t.path(),
        )
        .unwrap();
        assert_eq!(p.home, Some(t.path().join("rel-home")));
        assert_eq!(
            p.state_dir,
            Some(t.path().join("rel-home").join(".agentspace"))
        );
        assert_eq!(p.ssh_dir, Some(t.path().join("rel-ssh")));
        let q = resolve_paths(&env(&[("AGENTSPACE_STATE_DIR", "rel-state")]), t.path()).unwrap();
        assert_eq!(q.home, None);
        assert_eq!(q.state_dir, Some(t.path().join("rel-state")));
        assert_eq!(q.ssh_dir, None);
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_relative_or_root_relative_value_comes_out_full() {
        let t = TempDir::new();
        for value in ["C:rel", r"\rel"] {
            let p = resolve_paths(
                &env(&[("USERPROFILE", value), ("AGENTSPACE_SSH_DIR", value)]),
                t.path(),
            )
            .unwrap();
            let home = p.home.unwrap();
            assert!(home.is_absolute(), "{value} came out as {home:?}");
            assert!(p.ssh_dir.unwrap().is_absolute());
            assert!(p.state_dir.unwrap().is_absolute());
        }
    }

    #[test]
    fn each_override_applies_when_set_and_not_empty_and_moves_only_its_folder() {
        let t = TempDir::new();
        let home = t.path().join("h");
        let state = t.path().join("s");
        let ssh = t.path().join("x");
        let with = |pairs: &[(&str, &str)]| resolve_paths(&env(pairs), t.path()).unwrap();
        let h = home.to_str().unwrap();
        let p = with(&[
            ("USERPROFILE", h),
            ("AGENTSPACE_STATE_DIR", state.to_str().unwrap()),
        ]);
        assert_eq!(p.state_dir, Some(state.clone()));
        assert_eq!(p.ssh_dir, Some(home.join(".ssh")));
        let p = with(&[
            ("USERPROFILE", h),
            ("AGENTSPACE_SSH_DIR", ssh.to_str().unwrap()),
        ]);
        assert_eq!(p.state_dir, Some(home.join(".agentspace")));
        assert_eq!(p.ssh_dir, Some(ssh.clone()));
        let p = with(&[
            ("USERPROFILE", h),
            ("AGENTSPACE_STATE_DIR", ""),
            ("AGENTSPACE_SSH_DIR", ""),
        ]);
        assert_eq!(p.state_dir, Some(home.join(".agentspace")));
        assert_eq!(p.ssh_dir, Some(home.join(".ssh")));
        // With no home, the overrides still resolve.
        let p = with(&[
            ("AGENTSPACE_STATE_DIR", state.to_str().unwrap()),
            ("AGENTSPACE_SSH_DIR", ssh.to_str().unwrap()),
        ]);
        assert_eq!(
            (p.home, p.state_dir, p.ssh_dir),
            (None, Some(state), Some(ssh))
        );
    }

    #[test]
    fn a_file_where_a_folder_has_to_be_is_not_checked_here() {
        let t = TempDir::new();
        let file = t.path().join("a-file");
        std::fs::write(&file, b"x").unwrap();
        let f = file.to_str().unwrap();
        let home = t.path().join("h");
        let h = home.to_str().unwrap();
        let p = resolve_paths(&env(&[("USERPROFILE", f)]), t.path()).unwrap();
        assert_eq!(p.home, Some(file.clone()));
        let p = resolve_paths(
            &env(&[("USERPROFILE", h), ("AGENTSPACE_STATE_DIR", f)]),
            t.path(),
        )
        .unwrap();
        assert_eq!(p.state_dir, Some(file.clone()));
        let p = resolve_paths(
            &env(&[("USERPROFILE", h), ("AGENTSPACE_SSH_DIR", f)]),
            t.path(),
        )
        .unwrap();
        assert_eq!(p.ssh_dir, Some(file));
    }

    #[test]
    fn a_missing_folder_is_not_an_error() {
        let t = TempDir::new();
        let missing = t.path().join("not-yet");
        let r = resolve_paths(
            &env(&[("USERPROFILE", missing.to_str().unwrap())]),
            t.path(),
        );
        assert!(r.is_ok());
    }
}

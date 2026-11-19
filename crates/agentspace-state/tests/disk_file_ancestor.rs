//! A file where the home folder, or a folder above the state folder, belongs (docs/state.md, The
//! home folder; architect, 2026-11-19): `read`, `lock` and `write` each walk up from the state folder
//! to its first existing ancestor and fail when that is not a folder.
#![cfg(windows)]

mod disk_support;

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use agentspace_state::{StoreError, lock, read, resolve_paths, write};
use disk_support::{sandbox, state_of};

const WHY: &str = "the state folder is a file";

/// The state folder the environment gives, with only the given variables set.
fn state_dir(vars: &[(&str, &Path)], cwd: &Path) -> PathBuf {
    let map: HashMap<String, OsString> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.as_os_str().to_owned()))
        .collect();
    let var = |name: &str| map.get(name).cloned();
    resolve_paths(&var, cwd)
        .expect("Paths must resolve")
        .state_dir
        .expect("A state folder must resolve")
}

/// A file, and the two state folders that cannot exist under it: the first when the home names the
/// file (`<file>\.agentspace`), the second when `AGENTSPACE_STATE_DIR` is `<file>\sub`.
fn cases(root: &Path) -> (PathBuf, [PathBuf; 2]) {
    let f = root.join("a-file");
    std::fs::write(&f, b"x").unwrap();
    let home_names_a_file = state_dir(&[("USERPROFILE", &f)], root);
    let state_under_a_file = state_dir(
        &[
            ("USERPROFILE", &root.join("home")),
            ("AGENTSPACE_STATE_DIR", &f.join("sub")),
        ],
        root,
    );
    assert_eq!(home_names_a_file, f.join(".agentspace"));
    assert_eq!(state_under_a_file, f.join("sub"));
    (f, [home_names_a_file, state_under_a_file])
}

/// The file is untouched and nothing was made beside or under it.
fn nothing_created(root: &Path, f: &Path, dirs: &[PathBuf]) {
    assert_eq!(std::fs::read(f).unwrap(), b"x");
    for d in dirs {
        assert!(!d.exists(), "{d:?} must not exist");
    }
    let names: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

#[test]
fn a_home_or_state_folder_under_a_file_is_unreadable_and_not_no_leases() {
    let t = sandbox();
    let (f, dirs) = cases(t.path());
    for d in &dirs {
        let r = read(d);
        assert_eq!(
            r.as_ref().err(),
            Some(&StoreError::Unreadable(WHY.to_string())),
            "{d:?}"
        );
    }
    nothing_created(t.path(), &f, &dirs);
}

#[test]
fn a_home_or_state_folder_under_a_file_cannot_be_locked() {
    let t = sandbox();
    let (f, dirs) = cases(t.path());
    for d in &dirs {
        let r = lock(d);
        assert_eq!(
            r.as_ref().err(),
            Some(&StoreError::CannotLock(WHY.to_string())),
            "{d:?}"
        );
    }
    nothing_created(t.path(), &f, &dirs);
}

#[test]
fn a_home_or_state_folder_under_a_file_is_unwritable() {
    let t = sandbox();
    let other = sandbox();
    let guard = lock(other.path()).unwrap();
    let (f, dirs) = cases(t.path());
    for d in &dirs {
        let r = write(d, &state_of(vec![]), &guard);
        assert_eq!(
            r.as_ref().err(),
            Some(&StoreError::Unwritable(WHY.to_string())),
            "{d:?}"
        );
    }
    nothing_created(t.path(), &f, &dirs);
}

#[test]
fn a_missing_home_under_a_folder_is_still_no_leases_and_is_not_created() {
    let t = sandbox();
    let d = state_dir(&[("USERPROFILE", &t.path().join("not-yet"))], t.path());
    assert!(read(&d).unwrap().leases.is_empty());
    assert!(!t.path().join("not-yet").exists());
}

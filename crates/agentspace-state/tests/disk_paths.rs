//! The home, state and ssh folders (docs/state.md, The home folder; What moves which files).
#![cfg(windows)]

mod disk_support;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;

use agentspace_state::{Paths, StoreError, resolve_paths};
use disk_support::sandbox;

/// Resolves against `cwd` with only the given variables set; also returns the names asked for.
fn resolve(vars: &[(&str, &str)], cwd: &Path) -> (Result<Paths, StoreError>, Vec<String>) {
    let map: HashMap<String, OsString> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), OsString::from(v)))
        .collect();
    let asked = RefCell::new(Vec::new());
    let var = |name: &str| -> Option<OsString> {
        asked.borrow_mut().push(name.to_string());
        map.get(name).cloned()
    };
    let r = resolve_paths(&var, cwd);
    (r, asked.into_inner())
}

fn ok(vars: &[(&str, &str)], cwd: &Path) -> Paths {
    resolve(vars, cwd).0.expect("Paths must resolve")
}

fn s(p: &Path) -> String {
    p.to_str().unwrap().to_string()
}

#[test]
fn home_from_userprofile_gives_the_default_folders() {
    let t = sandbox();
    let home = t.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let p = ok(&[("USERPROFILE", &s(&home))], t.path());
    assert_eq!(p.home, Some(home.clone()));
    assert_eq!(p.state_dir, Some(home.join(".agentspace")));
    assert_eq!(p.ssh_dir, Some(home.join(".ssh")));
}

#[test]
fn resolving_never_creates_a_folder() {
    let t = sandbox();
    let home = t.path().join("home");
    std::fs::create_dir(&home).unwrap();
    ok(&[("USERPROFILE", &s(&home))], t.path());
    assert!(!home.join(".agentspace").exists());
    assert!(!home.join(".ssh").exists());
}

#[test]
fn a_missing_home_folder_is_not_an_error() {
    let t = sandbox();
    let home = t.path().join("not-yet");
    let p = ok(&[("USERPROFILE", &s(&home))], t.path());
    assert_eq!(p.home, Some(home.clone()));
    assert_eq!(p.state_dir, Some(home.join(".agentspace")));
}

#[test]
fn only_the_three_documented_variables_are_read() {
    let t = sandbox();
    let (r, asked) = resolve(&[("USERPROFILE", &s(t.path()))], t.path());
    r.unwrap();
    for name in &asked {
        assert!(
            ["USERPROFILE", "AGENTSPACE_STATE_DIR", "AGENTSPACE_SSH_DIR"].contains(&name.as_str()),
            "Variable {name} must not be read"
        );
    }
    assert!(asked.iter().any(|n| n == "USERPROFILE"));
}

#[test]
fn userprofile_unset_leaves_every_folder_none() {
    let t = sandbox();
    let p = ok(&[], t.path());
    assert_eq!(p.home, None);
    assert_eq!(p.state_dir, None);
    assert_eq!(p.ssh_dir, None);
}

#[test]
fn userprofile_empty_leaves_every_folder_none() {
    let t = sandbox();
    let p = ok(&[("USERPROFILE", "")], t.path());
    assert_eq!(p.home, None);
    assert_eq!(p.state_dir, None);
    assert_eq!(p.ssh_dir, None);
}

#[test]
fn a_relative_userprofile_is_made_full_against_the_working_folder() {
    let t = sandbox();
    let p = ok(&[("USERPROFILE", "rel-home")], t.path());
    let home = t.path().join("rel-home");
    assert_eq!(p.home, Some(home.clone()));
    assert_eq!(p.state_dir, Some(home.join(".agentspace")));
    assert_eq!(p.ssh_dir, Some(home.join(".ssh")));
    assert!(p.home.unwrap().is_absolute());
}

#[test]
fn state_dir_override_moves_only_the_state_folder() {
    let t = sandbox();
    let home = t.path().join("home");
    let st = t.path().join("elsewhere");
    let p = ok(
        &[
            ("USERPROFILE", &s(&home)),
            ("AGENTSPACE_STATE_DIR", &s(&st)),
        ],
        t.path(),
    );
    assert_eq!(p.home, Some(home.clone()));
    assert_eq!(p.state_dir, Some(st));
    assert_eq!(p.ssh_dir, Some(home.join(".ssh")));
}

#[test]
fn ssh_dir_override_moves_only_the_ssh_folder() {
    let t = sandbox();
    let home = t.path().join("home");
    let ssh = t.path().join("sshplace");
    let p = ok(
        &[("USERPROFILE", &s(&home)), ("AGENTSPACE_SSH_DIR", &s(&ssh))],
        t.path(),
    );
    assert_eq!(p.home, Some(home.clone()));
    assert_eq!(p.state_dir, Some(home.join(".agentspace")));
    assert_eq!(p.ssh_dir, Some(ssh));
}

#[test]
fn overrides_work_without_a_home() {
    let t = sandbox();
    let st = t.path().join("st");
    let ssh = t.path().join("ssh");
    let p = ok(
        &[
            ("AGENTSPACE_STATE_DIR", &s(&st)),
            ("AGENTSPACE_SSH_DIR", &s(&ssh)),
        ],
        t.path(),
    );
    assert_eq!(p.home, None);
    assert_eq!(p.state_dir, Some(st));
    assert_eq!(p.ssh_dir, Some(ssh));
}

#[test]
fn an_override_alone_leaves_the_other_folder_none_without_a_home() {
    let t = sandbox();
    let st = t.path().join("st");
    let p = ok(&[("AGENTSPACE_STATE_DIR", &s(&st))], t.path());
    assert_eq!(p.state_dir, Some(st));
    assert_eq!(p.ssh_dir, None);
    let ssh = t.path().join("ssh");
    let p = ok(&[("AGENTSPACE_SSH_DIR", &s(&ssh))], t.path());
    assert_eq!(p.state_dir, None);
    assert_eq!(p.ssh_dir, Some(ssh));
}

#[test]
fn empty_overrides_mean_the_defaults() {
    let t = sandbox();
    let home = t.path().join("home");
    let p = ok(
        &[
            ("USERPROFILE", &s(&home)),
            ("AGENTSPACE_STATE_DIR", ""),
            ("AGENTSPACE_SSH_DIR", ""),
        ],
        t.path(),
    );
    assert_eq!(p.state_dir, Some(home.join(".agentspace")));
    assert_eq!(p.ssh_dir, Some(home.join(".ssh")));
}

#[test]
fn empty_overrides_without_a_home_are_none() {
    let t = sandbox();
    let p = ok(
        &[("AGENTSPACE_STATE_DIR", ""), ("AGENTSPACE_SSH_DIR", "")],
        t.path(),
    );
    assert_eq!(p.state_dir, None);
    assert_eq!(p.ssh_dir, None);
}

#[test]
fn relative_overrides_are_made_full_against_the_working_folder() {
    let t = sandbox();
    let p = ok(
        &[
            ("USERPROFILE", &s(&t.path().join("home"))),
            ("AGENTSPACE_STATE_DIR", "rel-state"),
            ("AGENTSPACE_SSH_DIR", "rel-ssh"),
        ],
        t.path(),
    );
    assert_eq!(p.state_dir, Some(t.path().join("rel-state")));
    assert_eq!(p.ssh_dir, Some(t.path().join("rel-ssh")));
}

/// A file at a path that must be a folder is not this function's to report: the folder's own users
/// do (architect, 2026-11-19), so the paths still resolve.
#[test]
fn a_file_where_a_folder_must_be_does_not_stop_the_paths_resolving() {
    let t = sandbox();
    let f = t.path().join("a-file");
    std::fs::write(&f, b"x").unwrap();
    let home = t.path().join("home");

    let p = ok(&[("USERPROFILE", &s(&f))], t.path());
    assert_eq!(p.home, Some(f.clone()));
    assert_eq!(p.state_dir, Some(f.join(".agentspace")));
    assert_eq!(p.ssh_dir, Some(f.join(".ssh")));

    let p = ok(
        &[("USERPROFILE", &s(&home)), ("AGENTSPACE_STATE_DIR", &s(&f))],
        t.path(),
    );
    assert_eq!(p.state_dir, Some(f.clone()));

    let p = ok(
        &[("USERPROFILE", &s(&home)), ("AGENTSPACE_SSH_DIR", &s(&f))],
        t.path(),
    );
    assert_eq!(p.ssh_dir, Some(f.clone()));

    let p = ok(&[("AGENTSPACE_STATE_DIR", &s(&f))], t.path());
    assert_eq!(p.state_dir, Some(f));
}

#[test]
fn a_file_at_the_default_state_folder_still_resolves() {
    let t = sandbox();
    let home = t.path().join("home");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join(".agentspace"), b"x").unwrap();
    let p = ok(&[("USERPROFILE", &s(&home))], t.path());
    assert_eq!(p.state_dir, Some(home.join(".agentspace")));
}

#[test]
fn a_file_at_a_relative_override_still_resolves_made_full() {
    let t = sandbox();
    std::fs::write(t.path().join("rel-file"), b"x").unwrap();
    let p = ok(&[("AGENTSPACE_STATE_DIR", "rel-file")], t.path());
    assert_eq!(p.state_dir, Some(t.path().join("rel-file")));
}

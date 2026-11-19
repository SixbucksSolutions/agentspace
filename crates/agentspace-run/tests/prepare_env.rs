//! `prepare_child_env`: a Command passed through it and spawned directly sees
//! `AGENTSPACE_NESTED=1` and no `*_POSIX_FD_STATE` variable (architect's ruling, 2026-11-19;
//! docs/operator.md, Environment variables).

mod common;

use common::Rig;
use std::process::{Command, Stdio};

#[test]
fn prepare_child_env_removes_posix_fd_state_and_sets_nested() {
    let rig = Rig::new();
    let s = rig.standin("direct").write();
    let mut cmd = Command::new(s.path());
    cmd.env("FOO_POSIX_FD_STATE", "1")
        .env("0B1F7A52_9C3D_4E8A_POSIX_FD_STATE", "2")
        .env("KEEP_ME_FD_STATE_NOT", "kept")
        .env_remove("AGENTSPACE_NESTED")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    common::prepare_child_env(&mut cmd);
    assert!(cmd.status().unwrap().success());
    assert_eq!(s.env_var("AGENTSPACE_NESTED").as_deref(), Some("1"));
    let names = s.fd_state_names();
    assert!(
        names.is_empty(),
        "The program got {} *_POSIX_FD_STATE variable(s), named {names:?}",
        names.len()
    );
    assert_eq!(s.env_var("KEEP_ME_FD_STATE_NOT").as_deref(), Some("kept"));
}

#[test]
fn prepare_child_env_overrides_an_inherited_nested_value() {
    let rig = Rig::new();
    let s = rig.standin("direct").write();
    let mut cmd = Command::new(s.path());
    cmd.env("AGENTSPACE_NESTED", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    common::prepare_child_env(&mut cmd);
    assert!(cmd.status().unwrap().success());
    assert_eq!(s.env_var("AGENTSPACE_NESTED").as_deref(), Some("1"));
}

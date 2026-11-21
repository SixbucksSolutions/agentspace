//! The push stamp's path for a host (docs/state.md, The push stamp; A host's push stamp), and a push
//! for a host that keeps its own stamp (docs/operator.md, The stamp).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

mod win_support;

use agentspace_push::{Context, stamp_path, stamp_path_for};
use std::path::PathBuf;
use win_support::*;

fn context(home: &str) -> Context {
    Context {
        home: PathBuf::from(home),
        local_app_data: None,
        system_root: PathBuf::from("system"),
        nested: false,
    }
}

#[test]
fn the_boxs_stamp_path_is_v006s() {
    let ctx = context("throwaway-home");
    let want = PathBuf::from("throwaway-home")
        .join(".agentspace")
        .join("localswim-pushed.json");
    assert_eq!(stamp_path_for(&ctx, None), want);
    assert_eq!(stamp_path(&ctx), want);
}

#[test]
fn a_hosts_stamp_path_is_in_its_own_folder() {
    let ctx = context("throwaway-home");
    let want = PathBuf::from("throwaway-home")
        .join(".agentspace")
        .join("hosts")
        .join("alpha")
        .join("localswim-pushed.json");
    assert_eq!(stamp_path_for(&ctx, Some("alpha")), want);
    assert_ne!(
        stamp_path_for(&ctx, Some("alpha")),
        stamp_path_for(&ctx, Some("beta"))
    );
    assert_ne!(stamp_path_for(&ctx, Some("alpha")), stamp_path(&ctx));
}

fn host_stamp(rig: &Rig, project: &str) -> PathBuf {
    stamp_path_for(&rig.context(), Some(project))
}

#[test]
fn a_push_for_a_host_writes_its_own_stamp_and_never_the_boxs() {
    let rig = Rig::new();
    let path = host_stamp(&rig, "alpha");
    assert!(
        !path.parent().unwrap().exists(),
        "The folder exists already"
    );
    let say = FakeSay::person();
    rig.push_for(&say, DEFAULT_ALIAS, false, Some("alpha"));
    assert_eq!(rig.calls(), 1, "The push for a host sent nothing");
    assert_warnings(&say, &[], "A push for a host");
    assert_eq!(
        std::fs::read(&path).ok(),
        Some(rig.expected_stamp_file(DEFAULT_ZONE)),
        "The host's stamp is not the expected one"
    );
    assert!(rig.stamp_bytes().is_none(), "The box's stamp was written");
    let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["localswim-pushed.json".to_string()],
        "The host's folder holds a file besides the stamp"
    );
}

#[test]
fn a_host_stamp_that_matches_makes_the_next_push_for_that_host_not_due() {
    let rig = Rig::new();
    let first = FakeSay::person();
    rig.push_for(&first, DEFAULT_ALIAS, false, Some("alpha"));
    assert_eq!(rig.calls(), 1);
    let second = FakeSay::person();
    rig.push_for(&second, DEFAULT_ALIAS, false, Some("alpha"));
    assert_eq!(
        rig.calls(),
        1,
        "A matching host stamp did not stop the push"
    );
    assert!(second.silent(), "A push that is not due said something");
}

#[test]
fn a_push_for_a_host_never_reads_the_boxs_stamp() {
    let rig = Rig::new();
    rig.write_stamp_for(DEFAULT_ZONE);
    let say = FakeSay::person();
    rig.push_for(&say, DEFAULT_ALIAS, false, Some("alpha"));
    assert_eq!(
        rig.calls(),
        1,
        "The box's matching stamp stopped a push for a host"
    );
    assert!(host_stamp(&rig, "alpha").is_file());
}

#[test]
fn a_push_for_the_box_never_reads_a_hosts_stamp_and_leaves_it_alone() {
    let rig = Rig::new();
    let host = FakeSay::person();
    rig.push_for(&host, DEFAULT_ALIAS, false, Some("alpha"));
    let before = std::fs::read(host_stamp(&rig, "alpha")).expect("The host's stamp");
    let say = FakeSay::person();
    rig.push(&say, DEFAULT_ALIAS, false);
    assert_eq!(
        rig.calls(),
        2,
        "A host's matching stamp stopped the push for the box"
    );
    assert!(rig.stamp_matches(DEFAULT_ZONE), "The box's stamp is wrong");
    assert_eq!(
        std::fs::read(host_stamp(&rig, "alpha")).ok(),
        Some(before),
        "The push for the box changed a host's stamp"
    );
}

#[test]
fn two_hosts_keep_two_stamps() {
    let rig = Rig::new();
    for project in ["alpha", "beta"] {
        let say = FakeSay::person();
        rig.push_for(&say, DEFAULT_ALIAS, false, Some(project));
    }
    assert_eq!(rig.calls(), 2, "The second host's push was stopped");
    assert!(host_stamp(&rig, "alpha").is_file());
    assert!(host_stamp(&rig, "beta").is_file());
    assert!(rig.stamp_bytes().is_none(), "The box's stamp was written");
}

#[test]
fn a_host_stamp_that_cannot_be_written_warns_could_not_send_and_leaves_the_boxs_alone() {
    let rig = Rig::new();
    // A file where the host's folder should be.
    let hosts = rig.agentspace_dir().join("hosts");
    std::fs::create_dir_all(&hosts).expect("The hosts folder");
    std::fs::write(hosts.join("alpha"), b"not a folder").expect("A file in the folder's place");
    let say = FakeSay::person();
    rig.push_for(&say, DEFAULT_ALIAS, false, Some("alpha"));
    assert_warnings(&say, &[COULD_NOT_SEND], "A stamp that cannot be written");
    assert!(rig.stamp_bytes().is_none(), "The box's stamp was written");
}

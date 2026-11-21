//! Which host serves a project (docs/state.md, The host registry and Which host a connection
//! reaches; D55): `host_for` and `target_for`.

mod common;

use agentspace_hosts::{HostState, Target, host_for, read, target_for};
use common::*;

#[test]
fn host_for_finds_a_record_by_its_project_in_every_state() {
    let h = with_box(vec![
        host("a", HostState::Creating),
        host("b", HostState::Ready),
        host("c", HostState::Failed),
    ]);
    assert_eq!(host_for(&h, "a").unwrap().state, HostState::Creating);
    assert_eq!(host_for(&h, "b").unwrap().state, HostState::Ready);
    assert_eq!(host_for(&h, "c").unwrap().state, HostState::Failed);
}

#[test]
fn host_for_returns_the_record_itself() {
    let h = with_box(vec![full_host(
        "agentspace",
        HostState::Ready,
        "i-1234567890abcdef0",
        "2026-11-20T19:05:42Z",
    )]);
    let found = host_for(&h, "agentspace").unwrap();
    assert_eq!(found, &h.hosts[1]);
    assert_eq!(found.instance_id.as_deref(), Some("i-1234567890abcdef0"));
}

#[test]
fn host_for_finds_the_box_by_its_project() {
    let h = seed();
    let b = host_for(&h, "swimcoach").unwrap();
    assert_eq!(b.stack, "agentbox");
}

#[test]
fn host_for_a_project_with_no_record_is_none() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert!(host_for(&h, "example-two").is_none());
    assert!(host_for(&seed(), "agentspace").is_none());
}

#[test]
fn host_for_compares_the_project_exactly() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert!(host_for(&h, "Agentspace").is_none());
    assert!(host_for(&h, "agent").is_none());
    assert!(host_for(&h, "agentspace-two").is_none());
    assert!(host_for(&h, "agentspace ").is_none());
    assert!(host_for(&h, "").is_none());
}

#[test]
fn host_for_an_empty_registry_is_none() {
    assert!(host_for(&hosts(vec![]), "agentspace").is_none());
}

#[test]
fn a_project_with_no_record_goes_to_the_box() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert_eq!(target_for(&h, "example-two"), Target::Box);
    assert_eq!(target_for(&seed(), "agentspace"), Target::Box);
}

#[test]
fn a_project_whose_host_is_creating_goes_to_the_box() {
    let h = with_box(vec![host("agentspace", HostState::Creating)]);
    assert_eq!(target_for(&h, "agentspace"), Target::Box);
}

#[test]
fn a_project_whose_host_failed_goes_to_the_box() {
    let h = with_box(vec![host("agentspace", HostState::Failed)]);
    assert_eq!(target_for(&h, "agentspace"), Target::Box);
}

#[test]
fn a_project_whose_host_is_ready_goes_to_that_host() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert_eq!(target_for(&h, "agentspace"), Target::Host("agentspace"));
}

#[test]
fn a_ready_host_needs_no_instance_id_to_be_the_target() {
    // state.md: a ready record need not carry instance_id.
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert!(h.hosts[1].instance_id.is_none());
    assert_eq!(target_for(&h, "agentspace"), Target::Host("agentspace"));
}

#[test]
fn a_ready_host_with_a_time_and_an_id_is_the_target_too() {
    let h = with_box(vec![full_host(
        "agentspace",
        HostState::Ready,
        "i-1234567890abcdef0",
        "2026-11-20T19:05:42Z",
    )]);
    assert_eq!(target_for(&h, "agentspace"), Target::Host("agentspace"));
}

#[test]
fn the_box_is_the_target_of_its_own_project() {
    assert_eq!(target_for(&seed(), "swimcoach"), Target::Box);
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert_eq!(target_for(&h, "swimcoach"), Target::Box);
}

#[test]
fn the_target_follows_each_projects_own_record() {
    let h = with_box(vec![
        host("a", HostState::Ready),
        host("b", HostState::Creating),
        host("c", HostState::Failed),
        host("d", HostState::Ready),
    ]);
    assert_eq!(target_for(&h, "a"), Target::Host("a"));
    assert_eq!(target_for(&h, "b"), Target::Box);
    assert_eq!(target_for(&h, "c"), Target::Box);
    assert_eq!(target_for(&h, "d"), Target::Host("d"));
    assert_eq!(target_for(&h, "e"), Target::Box);
}

#[test]
fn the_target_compares_the_project_exactly() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert_eq!(target_for(&h, "Agentspace"), Target::Box);
    assert_eq!(target_for(&h, "agentspace-two"), Target::Box);
    assert_eq!(target_for(&h, ""), Target::Box);
}

#[test]
fn the_target_names_the_project_the_caller_can_use_for_the_hosts_alias() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    match target_for(&h, "agentspace") {
        Target::Host(p) => assert_eq!(
            agentspace_hosts::host_alias(p),
            "agentspace-host.agentspace"
        ),
        Target::Box => panic!("A ready host is the target"),
    }
}

#[test]
fn the_target_of_a_registry_read_from_the_disk_follows_the_file() {
    let t = TempHome::new();
    // A laptop with no registry sends every project to the box (the seed).
    let seeded = read(t.path()).unwrap();
    assert_eq!(target_for(&seeded, "agentspace"), Target::Box);
    t.put_hosts(include_str!(
        "../../../schemas/state/samples/hosts.state.full.json"
    ));
    let h = read(t.path()).unwrap();
    assert_eq!(target_for(&h, "agentspace"), Target::Host("agentspace"));
    assert_eq!(target_for(&h, "swimcoach"), Target::Box);
    t.put_hosts(include_str!(
        "../../../schemas/state/samples/hosts.state.creating-and-failed.json"
    ));
    let h = read(t.path()).unwrap();
    assert_eq!(target_for(&h, "agentspace"), Target::Box);
    assert_eq!(target_for(&h, "example-two"), Target::Box);
}

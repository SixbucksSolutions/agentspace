//! The pinned names (D55) and the paths from a home folder (docs/state.md: The host registry, The
//! project list, A host's folder, Agentspace's host keys, The host registry lock).

mod common;

use agentspace_hosts::{
    BOX_PROJECT, BOX_STACK, Host, HostState, host_alias, host_cert_path, host_dir, host_key_path,
    host_log_path, host_run_lock_path, host_stamp_path, hosts_lock_path, hosts_path, is_box,
    known_hosts_path, projects_path, stack_name,
};
use common::*;
use std::path::Path;

#[test]
fn the_box_is_swimcoachs_host_on_the_agentbox_stack() {
    assert_eq!(BOX_PROJECT, "swimcoach");
    assert_eq!(BOX_STACK, "agentbox");
}

#[test]
fn a_hosts_stack_is_agentspace_host_and_its_project() {
    assert_eq!(stack_name("agentspace"), "agentspace-host-agentspace");
    assert_eq!(stack_name("example-two"), "agentspace-host-example-two");
}

#[test]
fn the_longest_stack_name_is_forty_eight_characters() {
    // 16 for the prefix and 32 for the project (state.md, The host registry).
    let project = "a".repeat(32);
    assert_eq!(stack_name(&project).len(), 48);
}

#[test]
fn a_hosts_operator_alias_is_agentspace_host_dot_project() {
    assert_eq!(host_alias("agentspace"), "agentspace-host.agentspace");
    assert_eq!(host_alias("example-two"), "agentspace-host.example-two");
}

#[test]
fn the_box_record_is_the_box() {
    assert!(is_box(&box_host()));
}

#[test]
fn a_record_with_another_stack_is_not_the_box() {
    assert!(!is_box(&host("agentspace", HostState::Ready)));
    assert!(!is_box(&host("agentspace", HostState::Creating)));
    assert!(!is_box(&host("agentspace", HostState::Failed)));
}

#[test]
fn the_box_is_found_by_its_stack_whatever_its_state() {
    for state in [HostState::Creating, HostState::Ready, HostState::Failed] {
        let h = Host {
            state,
            ..box_host()
        };
        assert!(is_box(&h));
    }
}

#[test]
fn the_registry_is_hosts_json_in_the_agentspace_folder() {
    let home = Path::new("home-folder");
    assert_eq!(
        hosts_path(home),
        home.join(".agentspace").join("hosts.json")
    );
}

#[test]
fn the_registry_lock_is_hosts_lock_beside_the_registry() {
    let home = Path::new("home-folder");
    assert_eq!(
        hosts_lock_path(home),
        home.join(".agentspace").join("hosts.lock")
    );
}

#[test]
fn the_project_list_is_projects_json_in_the_agentspace_folder() {
    let home = Path::new("home-folder");
    assert_eq!(
        projects_path(home),
        home.join(".agentspace").join("projects.json")
    );
}

#[test]
fn agentspaces_known_hosts_is_in_the_agentspace_folder() {
    let home = Path::new("home-folder");
    assert_eq!(
        known_hosts_path(home),
        home.join(".agentspace").join("known_hosts")
    );
}

#[test]
fn a_hosts_folder_is_hosts_then_its_project() {
    let home = Path::new("home-folder");
    assert_eq!(
        host_dir(home, "agentspace"),
        home.join(".agentspace").join("hosts").join("agentspace")
    );
}

#[test]
fn a_hosts_files_are_in_its_folder_under_their_names() {
    let home = Path::new("home-folder");
    let dir = home.join(".agentspace").join("hosts").join("agentspace");
    assert_eq!(host_key_path(home, "agentspace"), dir.join("id_ed25519"));
    assert_eq!(
        host_cert_path(home, "agentspace"),
        dir.join("id_ed25519-cert.pub")
    );
    assert_eq!(host_log_path(home, "agentspace"), dir.join("provision.log"));
    assert_eq!(
        host_run_lock_path(home, "agentspace"),
        dir.join("provision.lock")
    );
    assert_eq!(
        host_stamp_path(home, "agentspace"),
        dir.join("localswim-pushed.json")
    );
}

#[test]
fn the_box_has_a_folder_of_its_own_under_its_project() {
    // state.md, A host's folder: the box MAY have hosts\swimcoach\.
    let home = Path::new("home-folder");
    assert_eq!(
        host_dir(home, BOX_PROJECT),
        home.join(".agentspace").join("hosts").join("swimcoach")
    );
}

#[test]
fn the_paths_follow_the_home_they_are_given_and_no_other() {
    // The home is a value, never read from the environment (state.md, The home folder).
    let a = TempHome::new();
    let b = TempHome::new();
    assert!(hosts_path(a.path()).starts_with(a.path()));
    assert!(hosts_path(b.path()).starts_with(b.path()));
    assert!(host_run_lock_path(a.path(), "x").starts_with(a.path()));
    assert!(known_hosts_path(b.path()).starts_with(b.path()));
}

#[test]
fn computing_a_path_creates_nothing() {
    let t = TempHome::new();
    let _ = hosts_path(t.path());
    let _ = host_dir(t.path(), "agentspace");
    let _ = host_run_lock_path(t.path(), "agentspace");
    assert!(TempHome::names_in(t.path()).is_empty());
}

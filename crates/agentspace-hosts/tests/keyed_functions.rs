//! Which host serves a project on a platform, and the functions that act on one host of a project
//! (docs/state.md, The host registry and A host's platform and names; D91, D101; E0.12-T02-S01,
//! card #0250): the keyed twins find a host by its project and its platform, and every function
//! other crates call today, which takes a project alone, keeps its signature and means
//! `linux/arm64`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The twins are named `<existing
//! name>_keyed` and take `&HostKey` where the existing one takes the project (the dev lead's pin
//! P1, architect's to confirm).

mod common;

use agentspace_hosts::{
    HostKey, HostState, Target, host_for, host_for_keyed, read, target_for, target_for_keyed,
};
use common::*;

const WINDOWS_STACK: &str = "agentspace-windows-amd64-host-agentspace";
const LINUX_AMD64_STACK: &str = "agentspace-linux-amd64-host-agentspace";

fn on(project: &str, platform: &str, stack: &str, state: HostState) -> agentspace_hosts::Host {
    agentspace_hosts::Host {
        platform: Some(platform.to_string()),
        stack: stack.to_string(),
        ..host(project, state)
    }
}

fn windows(state: HostState) -> agentspace_hosts::Host {
    on("agentspace", "windows/amd64", WINDOWS_STACK, state)
}

fn linux_amd64(state: HostState) -> agentspace_hosts::Host {
    on("agentspace", "linux/amd64", LINUX_AMD64_STACK, state)
}

fn win_key() -> HostKey {
    HostKey::new("agentspace", "windows/amd64")
}

fn amd64_key() -> HostKey {
    HostKey::new("agentspace", "linux/amd64")
}

fn arm64_key() -> HostKey {
    HostKey::linux_arm64("agentspace")
}

fn is_host_target(t: Option<Target<'_>>) -> bool {
    matches!(t, Some(Target::Host(_)))
}

// --- host_for_keyed ---------------------------------------------------------------------------

#[test]
fn a_host_is_found_by_its_project_and_its_platform() {
    let h = with_box(vec![
        host("agentspace", HostState::Ready),
        linux_amd64(HostState::Creating),
        windows(HostState::Failed),
    ]);
    let arm64 = host_for_keyed(&h, &arm64_key()).unwrap();
    assert_eq!(arm64, &h.hosts[1]);
    assert_eq!(arm64.state, HostState::Ready);
    let amd64 = host_for_keyed(&h, &amd64_key()).unwrap();
    assert_eq!(amd64, &h.hosts[2]);
    assert_eq!(amd64.state, HostState::Creating);
    let win = host_for_keyed(&h, &win_key()).unwrap();
    assert_eq!(win, &h.hosts[3]);
    assert_eq!(win.state, HostState::Failed);
}

#[test]
fn a_platform_with_no_record_has_no_host_even_when_the_project_has_one() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert!(host_for_keyed(&h, &win_key()).is_none());
    assert!(host_for_keyed(&h, &amd64_key()).is_none());
    let only_windows = with_box(vec![windows(HostState::Ready)]);
    assert!(host_for_keyed(&only_windows, &arm64_key()).is_none());
    assert!(host_for_keyed(&only_windows, &amd64_key()).is_none());
}

#[test]
fn a_record_without_platform_is_the_linux_arm64_host_and_an_explicit_one_is_too() {
    let none = with_box(vec![host("agentspace", HostState::Ready)]);
    assert!(host_for_keyed(&none, &arm64_key()).is_some());
    let explicit = with_box(vec![on(
        "agentspace",
        "linux/arm64",
        "agentspace-host-agentspace",
        HostState::Ready,
    )]);
    assert_eq!(
        host_for_keyed(&explicit, &arm64_key()),
        Some(&explicit.hosts[1])
    );
}

#[test]
fn the_box_is_swimcoachs_linux_arm64_host_alone() {
    let h = seed();
    let b = host_for_keyed(&h, &HostKey::linux_arm64("swimcoach")).unwrap();
    assert_eq!(b.stack, "agentbox");
    assert!(host_for_keyed(&h, &HostKey::new("swimcoach", "windows/amd64")).is_none());
}

#[test]
fn the_keyed_find_compares_the_project_and_the_platform_exactly() {
    let h = with_box(vec![windows(HostState::Ready)]);
    assert!(host_for_keyed(&h, &HostKey::new("Agentspace", "windows/amd64")).is_none());
    assert!(host_for_keyed(&h, &HostKey::new("agentspace", "Windows/amd64")).is_none());
    assert!(host_for_keyed(&h, &HostKey::new("agentspace", "windows-amd64")).is_none());
    assert!(host_for_keyed(&h, &HostKey::new("agentspace", "windows/amd64/v3")).is_none());
    assert!(host_for_keyed(&h, &HostKey::new("agent", "windows/amd64")).is_none());
}

#[test]
fn the_keyed_find_follows_a_file_read_from_the_disk() {
    let t = TempHome::new();
    t.put_hosts(include_str!(
        "../../../schemas/state/samples/hosts.state.platforms.json"
    ));
    let h = read(t.path()).unwrap();
    assert_eq!(
        host_for_keyed(&h, &arm64_key()).unwrap().stack,
        "agentspace-host-agentspace"
    );
    assert_eq!(
        host_for_keyed(&h, &amd64_key()).unwrap().state,
        HostState::Ready
    );
    assert_eq!(
        host_for_keyed(&h, &win_key()).unwrap().state,
        HostState::Creating
    );
    assert!(host_for_keyed(&h, &HostKey::new("example-two", "windows/amd64")).is_none());
}

// --- target_for_keyed -------------------------------------------------------------------------

// state.md, Which host a connection reaches, pin P13 (T01): a lease reaches the host of its project
// on its platform when that record is `ready`; **the fallback to the box is `linux/arm64`'s alone**,
// and a lease of any other platform whose project has no `ready` record on that platform reaches
// no host. So the keyed target answers `Some(Target::Host(..))` for a ready record, `Some(Target::Box)`
// for the `linux/arm64` fallback, and `None` for a platform with no host to reach. (The return
// type `Option<Target>` is the implementer's, found at compile time; the architect's to confirm.)

#[test]
fn a_ready_host_on_the_platform_is_the_target() {
    let ready = with_box(vec![windows(HostState::Ready)]);
    assert!(is_host_target(target_for_keyed(&ready, &win_key())));
}

#[test]
fn a_host_not_ready_on_another_platform_reaches_no_host_and_not_the_box() {
    for state in [HostState::Creating, HostState::Failed] {
        let h = with_box(vec![windows(state)]);
        assert!(
            target_for_keyed(&h, &win_key()).is_none(),
            "{state:?}: the box serves linux/arm64 alone"
        );
    }
}

#[test]
fn a_platform_with_no_record_reaches_no_host_and_not_the_box() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert!(target_for_keyed(&h, &win_key()).is_none());
    assert!(target_for_keyed(&h, &amd64_key()).is_none());
    assert!(target_for_keyed(&seed(), &win_key()).is_none());
}

#[test]
fn a_ready_host_on_another_platform_is_not_the_target() {
    // A project's hosts on the platforms it holds do not serve one another's leases (D91).
    let h = with_box(vec![
        host("agentspace", HostState::Ready),
        windows(HostState::Ready),
    ]);
    assert!(is_host_target(target_for_keyed(&h, &arm64_key())));
    assert!(is_host_target(target_for_keyed(&h, &win_key())));
    assert!(target_for_keyed(&h, &amd64_key()).is_none());
    // And a project whose only ready host is windows/amd64 falls back to the box on linux/arm64.
    let only_windows = with_box(vec![windows(HostState::Ready)]);
    assert_eq!(
        target_for_keyed(&only_windows, &arm64_key()),
        Some(Target::Box)
    );
}

#[test]
fn a_linux_arm64_host_not_ready_falls_back_to_the_box() {
    for state in [HostState::Creating, HostState::Failed] {
        let h = with_box(vec![host("agentspace", state)]);
        assert_eq!(
            target_for_keyed(&h, &arm64_key()),
            Some(Target::Box),
            "{state:?}"
        );
    }
    assert_eq!(
        target_for_keyed(&seed(), &arm64_key()),
        Some(Target::Box),
        "No record"
    );
}

#[test]
fn the_arm64_hosts_target_names_the_project_as_before() {
    let h = with_box(vec![host("agentspace", HostState::Ready)]);
    assert_eq!(
        target_for_keyed(&h, &arm64_key()),
        Some(Target::Host("agentspace"))
    );
}

#[test]
fn the_box_is_the_target_of_its_own_linux_arm64_key() {
    assert_eq!(
        target_for_keyed(&seed(), &HostKey::linux_arm64("swimcoach")),
        Some(Target::Box)
    );
}

// --- The functions other crates call mean linux/arm64 -----------------------------------------

#[test]
fn host_for_a_project_means_its_linux_arm64_host() {
    let h = with_box(vec![
        windows(HostState::Ready),
        host("agentspace", HostState::Creating),
        linux_amd64(HostState::Ready),
    ]);
    let found = host_for(&h, "agentspace").expect("The arm64 host is found");
    assert_eq!(found, &h.hosts[2]);
    assert_eq!(found.state, HostState::Creating);
}

#[test]
fn host_for_a_project_with_hosts_on_other_platforms_alone_is_none() {
    let h = with_box(vec![
        windows(HostState::Ready),
        linux_amd64(HostState::Ready),
    ]);
    assert!(host_for(&h, "agentspace").is_none());
}

#[test]
fn target_for_a_project_means_its_linux_arm64_host() {
    let only_windows = with_box(vec![windows(HostState::Ready)]);
    assert_eq!(target_for(&only_windows, "agentspace"), Target::Box);
    let both = with_box(vec![
        windows(HostState::Ready),
        host("agentspace", HostState::Ready),
    ]);
    assert_eq!(target_for(&both, "agentspace"), Target::Host("agentspace"));
    let arm64_creating = with_box(vec![
        windows(HostState::Ready),
        host("agentspace", HostState::Creating),
    ]);
    assert_eq!(target_for(&arm64_creating, "agentspace"), Target::Box);
}

#[test]
fn the_box_is_still_found_by_its_stack_beside_hosts_of_other_platforms() {
    let h = with_box(vec![on(
        "swimcoach",
        "windows/amd64",
        "agentspace-windows-amd64-host-swimcoach",
        HostState::Ready,
    )]);
    assert!(agentspace_hosts::is_box(&h.hosts[0]));
    assert!(!agentspace_hosts::is_box(&h.hosts[1]));
    assert_eq!(target_for(&h, "swimcoach"), Target::Box);
}

// --- The setters, keyed ----------------------------------------------------------------------

const TWO_HOSTS: &str = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.large\",\"last_attempt_at\":\"2026-11-27T19:05:42Z\",\"last_success_at\":\"2026-11-27T19:12:03Z\"},{\"project\":\"agentspace\",\"platform\":\"windows/amd64\",\"stack\":\"agentspace-windows-amd64-host-agentspace\",\"state\":\"creating\",\"last_attempt_at\":\"2026-11-27T20:00:00Z\"}]}\n";

fn put_two() -> TempHome {
    let t = TempHome::new();
    t.put_hosts(TWO_HOSTS);
    t
}

fn text_of(t: &TempHome) -> String {
    String::from_utf8(t.hosts_bytes()).unwrap()
}

#[test]
fn set_instance_keyed_sets_the_id_and_type_of_that_platforms_record_alone() {
    let t = put_two();
    agentspace_hosts::set_instance_keyed(
        t.path(),
        &win_key(),
        Some("i-0aaaaaaaaaaaaaaaa"),
        Some("m8a.xlarge"),
    )
    .unwrap();
    let expected = TWO_HOSTS.replace(
        "\"state\":\"creating\",\"last_attempt_at\"",
        "\"state\":\"creating\",\"instance_id\":\"i-0aaaaaaaaaaaaaaaa\",\"instance_type\":\"m8a.xlarge\",\"last_attempt_at\"",
    );
    assert_ne!(expected, TWO_HOSTS);
    assert_eq!(text_of(&t), expected);
}

#[test]
fn set_instance_keyed_on_the_arm64_key_leaves_the_windows_record_as_it_was() {
    let t = put_two();
    agentspace_hosts::set_instance_keyed(t.path(), &arm64_key(), None, None).unwrap();
    let expected = TWO_HOSTS.replace(
        "\"state\":\"ready\",\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.large\",",
        "\"state\":\"ready\",",
    );
    assert_ne!(expected, TWO_HOSTS);
    assert_eq!(text_of(&t), expected);
}

#[test]
fn set_instance_type_keyed_sets_that_platforms_type_alone() {
    let t = put_two();
    agentspace_hosts::set_instance_type_keyed(t.path(), &win_key(), Some("m8a.2xlarge")).unwrap();
    let expected = TWO_HOSTS.replace(
        "\"state\":\"creating\",\"last_attempt_at\"",
        "\"state\":\"creating\",\"instance_type\":\"m8a.2xlarge\",\"last_attempt_at\"",
    );
    assert_ne!(expected, TWO_HOSTS);
    assert_eq!(text_of(&t), expected);
}

#[test]
fn set_last_success_at_keyed_sets_that_platforms_success_alone() {
    let t = put_two();
    agentspace_hosts::set_last_success_at_keyed(t.path(), &win_key(), "2026-11-27T20:10:00Z")
        .unwrap();
    let expected = TWO_HOSTS.replace(
        "\"last_attempt_at\":\"2026-11-27T20:00:00Z\"}",
        "\"last_attempt_at\":\"2026-11-27T20:00:00Z\",\"last_success_at\":\"2026-11-27T20:10:00Z\"}",
    );
    assert_ne!(expected, TWO_HOSTS);
    assert_eq!(text_of(&t), expected);
}

#[test]
fn the_setters_that_take_a_project_alone_change_its_linux_arm64_record() {
    // The existing functions mean linux/arm64: with both hosts present, the windows record is as
    // it was after each of them.
    let t = put_two();
    agentspace_hosts::set_instance_type(t.path(), "agentspace", Some("m9g.2xlarge")).unwrap();
    let after_type = TWO_HOSTS.replace(
        "\"instance_type\":\"m9g.large\"",
        "\"instance_type\":\"m9g.2xlarge\"",
    );
    assert_ne!(after_type, TWO_HOSTS);
    assert_eq!(text_of(&t), after_type);

    let t = put_two();
    agentspace_hosts::set_last_success_at(t.path(), "agentspace", "2026-11-27T21:00:00Z").unwrap();
    let after_success = TWO_HOSTS.replace(
        "\"last_success_at\":\"2026-11-27T19:12:03Z\"",
        "\"last_success_at\":\"2026-11-27T21:00:00Z\"",
    );
    assert_ne!(after_success, TWO_HOSTS);
    assert_eq!(text_of(&t), after_success);

    let t = put_two();
    agentspace_hosts::set_instance(t.path(), "agentspace", Some("i-0aaaaaaaaaaaaaaaa"), None)
        .unwrap();
    let after_instance = TWO_HOSTS.replace(
        "\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.large\",",
        "\"instance_id\":\"i-0aaaaaaaaaaaaaaaa\",",
    );
    assert_ne!(after_instance, TWO_HOSTS);
    assert_eq!(text_of(&t), after_instance);
}

// --- The provisioning run's lock, keyed -------------------------------------------------------

fn lock_file(t: &TempHome, folder: &str) -> std::path::PathBuf {
    t.state().join("hosts").join(folder).join("provision.lock")
}

#[test]
fn a_platforms_run_lock_is_in_its_own_hosts_folder() {
    let t = TempHome::new();
    let _lock = agentspace_hosts::try_run_lock_keyed(t.path(), &win_key())
        .unwrap()
        .expect("A free lock is taken");
    assert!(lock_file(&t, "agentspace.windows-amd64").is_file());
    assert!(!lock_file(&t, "agentspace").exists());
}

#[test]
fn one_platforms_run_lock_does_not_exclude_another_platforms_of_the_same_project() {
    let t = TempHome::new();
    let windows = agentspace_hosts::try_run_lock_keyed(t.path(), &win_key())
        .unwrap()
        .expect("The windows lock is free");
    let arm64 = agentspace_hosts::try_run_lock(t.path(), "agentspace")
        .unwrap()
        .expect("The arm64 lock is a different lock");
    let amd64 = agentspace_hosts::try_run_lock_keyed(t.path(), &amd64_key())
        .unwrap()
        .expect("The linux/amd64 lock is a different lock");
    drop((windows, arm64, amd64));
}

#[test]
fn a_held_platform_run_lock_is_not_taken_twice() {
    let t = TempHome::new();
    let first = agentspace_hosts::try_run_lock_keyed(t.path(), &win_key())
        .unwrap()
        .unwrap();
    let second = agentspace_hosts::try_run_lock_keyed(t.path(), &win_key())
        .expect("A held lock is not an error");
    assert!(second.is_none(), "The windows lock was taken while held");
    drop(first);
    assert!(free_within(FREE_WITHIN, || {
        agentspace_hosts::try_run_lock_keyed(t.path(), &win_key())
            .unwrap()
            .is_some()
    }));
}

#[test]
fn the_project_only_run_lock_is_the_arm64_hosts_lock() {
    let t = TempHome::new();
    let held = agentspace_hosts::try_run_lock(t.path(), "agentspace")
        .unwrap()
        .unwrap();
    let again = agentspace_hosts::try_run_lock_keyed(t.path(), &arm64_key()).unwrap();
    assert!(
        again.is_none(),
        "The arm64 key must name the arm64 host's lock"
    );
    drop(held);
}

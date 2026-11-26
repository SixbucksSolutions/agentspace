//! A host's key and the names it gives (docs/state.md, A host's platform and names; D91, D101;
//! E0.12-T02-S01, card #0250): `HostKey { project, platform }`, its host name, stack, operator
//! alias, host-key name and folder name, byte for byte, and the keyed twins of the path functions.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every expected name below is written out
//! literally from state.md's table, never built by the function under test, so a test fails when
//! the function builds a name differently. The one place a name is taken from today's function is
//! the `linux/arm64` comparison, which is the rule: every `linux/arm64` name stays as it was.

mod common;

use agentspace_hosts::{
    BOX_PROJECT, HostKey, LINUX_ARM64, host_alias, host_dir, host_dir_keyed, host_key_path,
    host_key_path_keyed, host_run_lock_path, host_run_lock_path_keyed, host_stamp_path,
    host_stamp_path_keyed, platform_in_name, stack_name,
};
use std::path::Path;

const LINUX_AMD64: &str = "linux/amd64";
const WINDOWS_AMD64: &str = "windows/amd64";

/// One host's every name, as state.md's table gives them.
struct Names {
    host_name: &'static str,
    stack: &'static str,
    operator_alias: &'static str,
    host_key_name: &'static str,
    folder: &'static str,
}

const AGENTSPACE_ARM64: Names = Names {
    host_name: "agentspace",
    stack: "agentspace-host-agentspace",
    operator_alias: "agentspace-host.agentspace",
    host_key_name: "agentspace-host.agentspace",
    folder: "agentspace",
};

const AGENTSPACE_LINUX_AMD64: Names = Names {
    host_name: "agentspace.linux-amd64",
    stack: "agentspace-linux-amd64-host-agentspace",
    operator_alias: "agentspace-host.agentspace.linux-amd64",
    host_key_name: "agentspace-host.agentspace.linux-amd64",
    folder: "agentspace.linux-amd64",
};

const AGENTSPACE_WINDOWS_AMD64: Names = Names {
    host_name: "agentspace.windows-amd64",
    stack: "agentspace-windows-amd64-host-agentspace",
    operator_alias: "agentspace-host.agentspace.windows-amd64",
    host_key_name: "agentspace-host.agentspace.windows-amd64",
    folder: "agentspace.windows-amd64",
};

fn assert_names(label: &str, key: &HostKey, want: &Names) {
    assert_eq!(key.host_name(), want.host_name, "{label}: host name");
    assert_eq!(key.stack_name(), want.stack, "{label}: stack name");
    assert_eq!(
        key.operator_alias(),
        want.operator_alias,
        "{label}: operator alias"
    );
    assert_eq!(
        key.host_key_name(),
        want.host_key_name,
        "{label}: host key name"
    );
    assert_eq!(key.folder_name(), want.folder, "{label}: folder name");
}

// --- The key itself ---------------------------------------------------------------------------

#[test]
fn the_linux_arm64_constant_is_the_normalized_platform() {
    assert_eq!(LINUX_ARM64, "linux/arm64");
}

#[test]
fn a_key_holds_the_project_and_the_platform_it_was_made_with() {
    let key = HostKey::new("agentspace", WINDOWS_AMD64);
    assert_eq!(key.project, "agentspace");
    assert_eq!(key.platform, "windows/amd64");
}

#[test]
fn a_linux_arm64_key_holds_linux_arm64() {
    let key = HostKey::linux_arm64("agentspace");
    assert_eq!(key.project, "agentspace");
    assert_eq!(key.platform, "linux/arm64");
}

#[test]
fn the_platform_in_a_name_is_the_platform_with_its_slash_written_as_a_dash() {
    assert_eq!(platform_in_name("linux/arm64"), "linux-arm64");
    assert_eq!(platform_in_name(LINUX_AMD64), "linux-amd64");
    assert_eq!(platform_in_name(WINDOWS_AMD64), "windows-amd64");
}

// --- Names, byte for byte ---------------------------------------------------------------------

#[test]
fn a_linux_arm64_hosts_names_are_as_before_v0_12() {
    assert_names(
        "linux/arm64",
        &HostKey::linux_arm64("agentspace"),
        &AGENTSPACE_ARM64,
    );
}

#[test]
fn a_key_made_with_new_for_linux_arm64_has_the_same_names_as_linux_arm64() {
    assert_names(
        "new",
        &HostKey::new("agentspace", "linux/arm64"),
        &AGENTSPACE_ARM64,
    );
}

#[test]
fn a_linux_amd64_hosts_names_carry_its_platform() {
    assert_names(
        "linux/amd64",
        &HostKey::new("agentspace", LINUX_AMD64),
        &AGENTSPACE_LINUX_AMD64,
    );
}

#[test]
fn a_windows_amd64_hosts_names_carry_its_platform() {
    assert_names(
        "windows/amd64",
        &HostKey::new("agentspace", WINDOWS_AMD64),
        &AGENTSPACE_WINDOWS_AMD64,
    );
}

#[test]
fn the_names_follow_the_project_and_the_platform_alone() {
    // A project with digits and hyphens, on each platform.
    assert_names(
        "linux/arm64",
        &HostKey::new("example-2", "linux/arm64"),
        &Names {
            host_name: "example-2",
            stack: "agentspace-host-example-2",
            operator_alias: "agentspace-host.example-2",
            host_key_name: "agentspace-host.example-2",
            folder: "example-2",
        },
    );
    assert_names(
        "linux/amd64",
        &HostKey::new("example-2", LINUX_AMD64),
        &Names {
            host_name: "example-2.linux-amd64",
            stack: "agentspace-linux-amd64-host-example-2",
            operator_alias: "agentspace-host.example-2.linux-amd64",
            host_key_name: "agentspace-host.example-2.linux-amd64",
            folder: "example-2.linux-amd64",
        },
    );
    assert_names(
        "windows/amd64",
        &HostKey::new("example-2", WINDOWS_AMD64),
        &Names {
            host_name: "example-2.windows-amd64",
            stack: "agentspace-windows-amd64-host-example-2",
            operator_alias: "agentspace-host.example-2.windows-amd64",
            host_key_name: "agentspace-host.example-2.windows-amd64",
            folder: "example-2.windows-amd64",
        },
    );
}

#[test]
fn every_linux_arm64_name_equals_what_todays_function_gives() {
    for project in ["agentspace", "example-two", "a", "7", "ab-1-cd"] {
        let key = HostKey::linux_arm64(project);
        assert_eq!(key.stack_name(), stack_name(project), "{project}: stack");
        assert_eq!(
            key.operator_alias(),
            host_alias(project),
            "{project}: operator alias"
        );
        assert_eq!(
            key.host_key_name(),
            host_alias(project),
            "{project}: host key name"
        );
        assert_eq!(key.host_name(), project, "{project}: host name");
        assert_eq!(key.folder_name(), project, "{project}: folder name");
    }
}

#[test]
fn swimcoach_on_linux_arm64_is_the_box() {
    // D44 and state.md: `agentbox` is the box's stack, which is swimcoach's linux/arm64 host.
    let key = HostKey::linux_arm64(BOX_PROJECT);
    assert_eq!(key.stack_name(), "agentbox");
    assert_eq!(key.host_name(), "swimcoach");
    assert_eq!(key.folder_name(), "swimcoach");
    assert_eq!(key.operator_alias(), host_alias("swimcoach"));
    assert_eq!(
        HostKey::new("swimcoach", "linux/arm64").stack_name(),
        "agentbox"
    );
}

#[test]
fn swimcoach_on_another_platform_is_a_host_like_any_other() {
    // state.md, The host registry, rule 3: "A swimcoach host of another platform is a record like
    // any other project's, with its own stack". Only its linux/arm64 host is the box.
    assert_names(
        "swimcoach windows/amd64",
        &HostKey::new("swimcoach", WINDOWS_AMD64),
        &Names {
            host_name: "swimcoach.windows-amd64",
            stack: "agentspace-windows-amd64-host-swimcoach",
            operator_alias: "agentspace-host.swimcoach.windows-amd64",
            host_key_name: "agentspace-host.swimcoach.windows-amd64",
            folder: "swimcoach.windows-amd64",
        },
    );
    assert_eq!(
        HostKey::new("swimcoach", LINUX_AMD64).stack_name(),
        "agentspace-linux-amd64-host-swimcoach"
    );
}

#[test]
fn a_thirty_two_character_project_gives_the_longest_names() {
    let project = "a".repeat(32);

    let arm64 = HostKey::linux_arm64(&project);
    assert_eq!(arm64.stack_name(), format!("agentspace-host-{project}"));
    assert_eq!(arm64.stack_name().len(), 48);
    assert_eq!(arm64.operator_alias(), format!("agentspace-host.{project}"));
    assert_eq!(arm64.host_name(), project);

    let linux = HostKey::new(&project, LINUX_AMD64);
    assert_eq!(
        linux.stack_name(),
        format!("agentspace-linux-amd64-host-{project}")
    );
    assert_eq!(linux.stack_name().len(), 60);
    assert_eq!(
        linux.operator_alias(),
        format!("agentspace-host.{project}.linux-amd64")
    );
    assert_eq!(linux.host_name(), format!("{project}.linux-amd64"));
    assert_eq!(linux.folder_name(), format!("{project}.linux-amd64"));

    let windows = HostKey::new(&project, WINDOWS_AMD64);
    assert_eq!(
        windows.stack_name(),
        format!("agentspace-windows-amd64-host-{project}")
    );
    // state.md: "the longest stack is 62 characters ... the longest operator alias ... is 62".
    assert_eq!(windows.stack_name().len(), 62);
    assert_eq!(
        windows.operator_alias(),
        format!("agentspace-host.{project}.windows-amd64")
    );
    assert_eq!(windows.operator_alias().len(), 62);
    assert_eq!(windows.host_key_name().len(), 62);
    assert_eq!(windows.host_name(), format!("{project}.windows-amd64"));
    assert_eq!(windows.folder_name(), format!("{project}.windows-amd64"));
}

#[test]
fn no_name_of_one_platforms_host_equals_another_platforms() {
    // state.md: "No name of one host equals another host's".
    let keys = [
        HostKey::linux_arm64("agentspace"),
        HostKey::new("agentspace", LINUX_AMD64),
        HostKey::new("agentspace", WINDOWS_AMD64),
        HostKey::linux_arm64("agentspace-linux"),
    ];
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a.host_name(), b.host_name());
            assert_ne!(a.stack_name(), b.stack_name());
            assert_ne!(a.operator_alias(), b.operator_alias());
            assert_ne!(a.host_key_name(), b.host_key_name());
            assert_ne!(a.folder_name(), b.folder_name());
        }
    }
}

// --- The keyed twins of the path functions ----------------------------------------------------

#[test]
fn a_linux_arm64_keys_paths_are_what_the_project_functions_give() {
    let home = Path::new("home-folder");
    let key = HostKey::linux_arm64("agentspace");
    assert_eq!(host_dir_keyed(home, &key), host_dir(home, "agentspace"));
    assert_eq!(
        host_key_path_keyed(home, &key),
        host_key_path(home, "agentspace")
    );
    assert_eq!(
        host_run_lock_path_keyed(home, &key),
        host_run_lock_path(home, "agentspace")
    );
    assert_eq!(
        host_stamp_path_keyed(home, &key),
        host_stamp_path(home, "agentspace")
    );
}

#[test]
fn a_windows_hosts_folder_is_hosts_then_its_host_name() {
    // state.md: `hosts\<project>.<os>-<arch>\` for a host of another platform.
    let home = Path::new("home-folder");
    let key = HostKey::new("agentspace", WINDOWS_AMD64);
    let dir = home
        .join(".agentspace")
        .join("hosts")
        .join("agentspace.windows-amd64");
    assert_eq!(host_dir_keyed(home, &key), dir);
    assert_eq!(host_key_path_keyed(home, &key), dir.join("id_ed25519"));
    assert_eq!(
        host_run_lock_path_keyed(home, &key),
        dir.join("provision.lock")
    );
    assert_eq!(
        host_stamp_path_keyed(home, &key),
        dir.join("localswim-pushed.json")
    );
}

#[test]
fn a_linux_amd64_hosts_folder_is_its_own_and_not_the_arm64_hosts() {
    let home = Path::new("home-folder");
    let amd64 = host_dir_keyed(home, &HostKey::new("agentspace", LINUX_AMD64));
    assert_eq!(
        amd64,
        home.join(".agentspace")
            .join("hosts")
            .join("agentspace.linux-amd64")
    );
    assert_ne!(amd64, host_dir(home, "agentspace"));
    assert_ne!(
        amd64,
        host_dir_keyed(home, &HostKey::new("agentspace", WINDOWS_AMD64))
    );
}

#[test]
fn the_keyed_paths_follow_the_home_they_are_given_and_no_other() {
    let a = Path::new("home-a");
    let b = Path::new("home-b");
    let key = HostKey::new("agentspace", WINDOWS_AMD64);
    assert!(host_dir_keyed(a, &key).starts_with(a));
    assert!(host_dir_keyed(b, &key).starts_with(b));
    assert!(!host_dir_keyed(a, &key).starts_with(b));
}

//! The Linux rows against today's steps (E0.12-T03-S03, part 1; the dev lead's pin P1): each row,
//! for both Linux platforms, equals the fixture copied from today's producer, and the two
//! platforms differ only in the image, a new stack's instance type and the localswim target.

use std::path::{Path, PathBuf};

use agentspace_operator_api::{HostPlatform, ScriptDelivery};

use crate::{LINUX_AMD64, LINUX_ARM64, embedded, fixtures};

const BOTH: [&crate::Linux; 2] = [&LINUX_ARM64, &LINUX_AMD64];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files_under(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type().unwrap().is_dir() {
            files_under(&entry.path(), &relative, out);
        } else {
            out.push((relative, entry.path()));
        }
    }
}

/// The remote command of `script` on `platform`, which every row's text passes through.
fn remote(platform: &dyn HostPlatform, script: &str) -> String {
    platform.remote_command(script).unwrap()
}

#[test]
fn the_remote_command_is_the_script_itself_on_both_linux_platforms() {
    for platform in BOTH {
        for script in [
            "bash -s",
            fixtures::GROW_SCRIPT,
            fixtures::PUSH_SCRIPTS[1].1,
            "",
        ] {
            assert_eq!(remote(platform, script), script, "{}", platform.platform());
        }
    }
}

#[test]
fn the_bootstrap_is_box_bootstrap_sh_on_stdin_with_todays_remote_command() {
    let on_disk = std::fs::read(repo().join("box/bootstrap.sh")).unwrap();
    for platform in BOTH {
        let bootstrap = platform.bootstrap().unwrap();
        assert!(
            bootstrap.script == on_disk.as_slice(),
            "{}: the embedded bootstrap differs from box/bootstrap.sh",
            platform.platform()
        );
        assert!(bootstrap.script.starts_with(b"#!/bin/bash\n"));
        assert!(!bootstrap.script.contains(&b'\r'));
        assert_eq!(
            bootstrap.delivery,
            ScriptDelivery::Stdin {
                remote_command: fixtures::BOOTSTRAP_REMOTE_COMMAND.to_string()
            },
            "{}",
            platform.platform()
        );
    }
}

#[test]
fn the_sync_plans_files_are_box_system_byte_for_byte() {
    let mut on_disk = Vec::new();
    files_under(&repo().join("box/system"), "", &mut on_disk);
    on_disk.sort();
    let names: Vec<&str> = on_disk.iter().map(|(n, _)| n.as_str()).collect();
    for platform in BOTH {
        let plan = platform.sync_plan().unwrap();
        let embedded: Vec<&str> = plan.files.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            embedded,
            names,
            "{}: the file list differs",
            platform.platform()
        );
        for ((name, bytes), (_, path)) in plan.files.iter().zip(&on_disk) {
            let disk = std::fs::read(path).unwrap();
            assert!(*bytes == disk.as_slice(), "Embedded {name} differs");
        }
        assert!(names.contains(&"install.sh"));
    }
}

#[test]
fn the_sync_plans_steps_are_todays() {
    for platform in BOTH {
        let plan = platform.sync_plan().unwrap();
        assert_eq!(plan.staging, fixtures::STAGING);
        assert_eq!(plan.install_command, remote(platform, fixtures::INSTALL));
        assert_eq!(
            plan.clear_command.as_deref(),
            Some(remote(platform, fixtures::REMOVE_STAGING).as_str())
        );
        assert_eq!(plan.claude_md_path, fixtures::CLAUDE_MD_PATH);
        assert_eq!(
            plan.localswim_install_command,
            remote(platform, fixtures::INSTALL_LOCALSWIM)
        );
    }
}

#[test]
fn the_localswim_target_follows_the_architecture() {
    let (goos, goarch, file_name) = fixtures::LOCALSWIM_ARM64;
    let arm = LINUX_ARM64.sync_plan().unwrap().localswim;
    assert_eq!(
        (arm.goos, arm.goarch, arm.file_name),
        (goos, goarch, file_name)
    );
    let amd = LINUX_AMD64.sync_plan().unwrap().localswim;
    assert_eq!(
        (amd.goos, amd.goarch, amd.file_name),
        ("linux", "amd64", "localswim-linux-amd64")
    );
}

#[test]
fn the_patch_check_and_its_messages_are_todays() {
    for platform in BOTH {
        assert_eq!(
            platform.patch_check_command().unwrap(),
            fixtures::PATCH_QUESTION
        );
        let messages = platform.patch_messages();
        assert_eq!(messages.phase, fixtures::PATCH_PHASE);
        assert_eq!(messages.log_command, fixtures::PATCH_LOG_COMMAND);
        assert_eq!(platform.ssm_document(), "AWS-RunShellScript");
    }
}

#[test]
fn the_horizon_command_is_todays_for_each_horizon() {
    for platform in BOTH {
        for (horizon, want) in fixtures::HORIZONS {
            assert_eq!(
                platform.horizon_command(*horizon).unwrap(),
                *want,
                "{} {horizon}",
                platform.platform()
            );
        }
    }
}

#[test]
fn the_grow_script_is_todays() {
    for platform in BOTH {
        assert_eq!(
            remote(platform, &platform.grow_script().unwrap()),
            fixtures::GROW_SCRIPT
        );
    }
}

#[test]
fn the_push_zone_is_the_iana_name() {
    for platform in BOTH {
        assert_eq!(
            platform
                .push_zone("W. Europe Standard Time", "Europe/Paris")
                .unwrap(),
            "Europe/Paris"
        );
        assert_eq!(platform.push_zone("UTC", "").unwrap(), "");
    }
}

#[test]
fn the_push_script_is_todays_for_each_zone() {
    for platform in BOTH {
        for (zone, want) in fixtures::PUSH_SCRIPTS {
            assert_eq!(
                remote(platform, &platform.push_script(zone).unwrap()),
                *want,
                "{} {zone:?}",
                platform.platform()
            );
        }
    }
}

#[test]
fn a_zone_that_fails_the_check_gives_the_empty_zones_push_script() {
    let empty = LINUX_ARM64.push_script("").unwrap();
    for bad in ["UTC; reboot", "-UTC", "a b", "$(x)", "a\"b", "a%b", "UTC\n"] {
        assert_eq!(LINUX_ARM64.push_script(bad).unwrap(), empty, "{bad:?}");
    }
    assert_ne!(LINUX_ARM64.push_script("UTC").unwrap(), empty);
}

/// Every remote command a Linux row gives keeps FA-15's rule, as today's do: no `%`, no double
/// quote, no control character, and no backslash at its end.
#[test]
fn every_linux_remote_command_keeps_fa15s_rule() {
    for platform in BOTH {
        let plan = platform.sync_plan().unwrap();
        let ScriptDelivery::Stdin { remote_command } = platform.bootstrap().unwrap().delivery
        else {
            panic!("A Linux bootstrap goes on stdin");
        };
        let mut commands = vec![
            remote_command,
            plan.install_command,
            plan.clear_command.unwrap(),
            plan.localswim_install_command,
            platform.grow_script().unwrap(),
            platform.push_script("").unwrap(),
            platform.push_script("Etc/GMT+5").unwrap(),
        ];
        commands.push(platform.patch_check_command().unwrap());
        commands.push(platform.horizon_command(1764201600).unwrap());
        for command in commands {
            assert!(!command.contains('%'), "{command}");
            assert!(!command.contains('"'), "{command}");
            assert!(!command.chars().any(char::is_control), "{command}");
            assert!(!command.ends_with('\\'), "{command}");
        }
    }
}

/// The two Linux platforms differ only in the image, a new stack's instance type and the localswim
/// target (D94; pin P41 of v0.12-T01): every other row is equal.
#[test]
fn the_two_linux_platforms_differ_only_in_image_type_and_localswim_target() {
    let (arm, amd) = (&LINUX_ARM64, &LINUX_AMD64);
    assert_ne!(arm.image_parameter(), amd.image_parameter());
    assert_ne!(arm.new_stack_instance_type(), amd.new_stack_instance_type());
    assert_eq!(arm.image_parameter(), None);
    assert_eq!(arm.new_stack_instance_type(), None);
    assert_eq!(
        arm.new_stack_extra_parameters(),
        amd.new_stack_extra_parameters()
    );
    assert!(arm.new_stack_extra_parameters().is_empty());
    assert_eq!(arm.login_user(), amd.login_user());
    assert_eq!(arm.ssm_document(), amd.ssm_document());
    assert_eq!(arm.bootstrap().unwrap(), amd.bootstrap().unwrap());

    let mut arm_plan = arm.sync_plan().unwrap();
    let amd_plan = amd.sync_plan().unwrap();
    assert_ne!(arm_plan.localswim, amd_plan.localswim);
    // With the localswim target set aside, the two plans are equal.
    arm_plan.localswim = amd_plan.localswim;
    assert_eq!(arm_plan, amd_plan);

    assert_eq!(
        arm.patch_check_command().unwrap(),
        amd.patch_check_command().unwrap()
    );
    assert_eq!(arm.patch_messages(), amd.patch_messages());
    for horizon in [0, 1, 1764201600, u64::MAX] {
        assert_eq!(
            arm.horizon_command(horizon).unwrap(),
            amd.horizon_command(horizon).unwrap()
        );
    }
    assert_eq!(arm.grow_script().unwrap(), amd.grow_script().unwrap());
    assert_eq!(
        arm.push_zone("UTC", "Etc/UTC").unwrap(),
        amd.push_zone("UTC", "Etc/UTC").unwrap()
    );
    for (zone, _) in fixtures::PUSH_SCRIPTS {
        assert_eq!(
            arm.push_script(zone).unwrap(),
            amd.push_script(zone).unwrap()
        );
    }
    assert_eq!(
        arm.remote_command("x").unwrap(),
        amd.remote_command("x").unwrap()
    );
}

#[test]
fn the_embedded_bootstrap_is_the_one_the_row_gives() {
    assert!(LINUX_ARM64.bootstrap().unwrap().script == embedded::BOOTSTRAP);
}

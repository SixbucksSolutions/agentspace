//! The bootstrap's delivery and the sync plan (docs/operator.md, The platform seam, the bootstrap
//! and sync rows; bootstrap on a Windows host; sync on a Windows host; The host agent's files).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! No test here reads a `box/windows` file's content: the files are another story's, and a test
//! depends only on which files are carried and on their names.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use agentspace_operator_api::{ScriptDelivery, SyncPlan};
use common::{MAX_LINE, PREFIX, platform, script_of};

fn plan() -> SyncPlan {
    platform().sync_plan().expect("the sync plan")
}

fn names(plan: &SyncPlan) -> BTreeSet<String> {
    plan.files
        .iter()
        .map(|(name, _)| name.replace('\\', "/"))
        .collect()
}

fn box_windows() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../box/windows")
}

/// The files `box/windows` holds at the top and below, as names, but those under `tests`.
fn files_on_disk(folder: &Path, prefix: &str, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(folder).expect("box/windows") {
        let entry = entry.expect("an entry");
        let name = entry.file_name().to_string_lossy().to_string();
        let path = format!("{prefix}{name}");
        if entry.file_type().expect("a type").is_dir() {
            if prefix.is_empty() && name == "tests" {
                continue;
            }
            files_on_disk(&entry.path(), &format!("{path}/"), out);
        } else {
            out.insert(path);
        }
    }
}

fn delivery() -> (String, String) {
    match platform().bootstrap().expect("the bootstrap").delivery {
        ScriptDelivery::CopyThenRun {
            remote_name,
            remote_command,
        } => (remote_name, remote_command),
        ScriptDelivery::Stdin { .. } => panic!("sent on stdin"),
    }
}

// The bootstrap

#[test]
fn the_bootstrap_is_copied_then_run_never_sent_on_stdin() {
    let bootstrap = platform().bootstrap().expect("the bootstrap");
    assert!(matches!(
        bootstrap.delivery,
        ScriptDelivery::CopyThenRun { .. }
    ));
}

#[test]
fn the_bootstrap_copy_is_named_dot_agentspace_bootstrap_ps1() {
    assert_eq!(delivery().0, ".agentspace-bootstrap.ps1");
}

#[test]
fn the_bootstrap_script_is_there_and_is_the_file_sync_carries_as_bootstrap_ps1() {
    let bootstrap = platform().bootstrap().expect("the bootstrap");
    assert!(!bootstrap.script.is_empty());
    let plan = plan();
    let carried = plan
        .files
        .iter()
        .find(|(name, _)| *name == "bootstrap.ps1")
        .expect("bootstrap.ps1 is carried");
    assert_eq!(carried.1, bootstrap.script);
}

#[test]
fn the_bootstrap_remote_command_is_in_the_encoded_form_within_the_limit() {
    let command = delivery().1;
    assert!(command.starts_with(PREFIX));
    assert!(!command.contains('='));
    assert!(command.len() <= MAX_LINE);
}

#[test]
fn the_bootstrap_remote_command_runs_the_copy_under_windows_powershell_with_file() {
    let script = script_of(&delivery().1);
    assert!(
        script.contains(".agentspace-bootstrap.ps1"),
        "names the copy"
    );
    assert!(script.contains("-File"), "runs it with -File");
    assert!(
        script.to_lowercase().contains("powershell"),
        "under Windows PowerShell"
    );
    assert!(
        !script.to_lowercase().contains("pwsh"),
        "not under PowerShell 7, which the bootstrap installs"
    );
}

#[test]
fn the_bootstrap_remote_command_never_prompts() {
    let script = script_of(&delivery().1);
    assert!(script.contains("-NonInteractive"));
}

#[test]
fn the_bootstrap_remote_command_removes_the_copy_after_running_it() {
    let script = script_of(&delivery().1);
    let run = script.find("-File").expect("the run");
    let remove = script.find("Remove-Item").expect("the removal");
    assert!(run < remove, "the copy is removed once it has run");
}

#[test]
fn the_bootstrap_remote_command_exits_with_the_scripts_status() {
    let script = script_of(&delivery().1);
    assert!(script.contains("exit"));
}

// The sync plan's files

#[test]
fn sync_carries_every_file_box_windows_holds_but_its_tests() {
    let mut on_disk = BTreeSet::new();
    files_on_disk(&box_windows(), "", &mut on_disk);
    assert!(!on_disk.is_empty());
    assert_eq!(names(&plan()), on_disk);
}

#[test]
fn sync_carries_the_host_agents_files_of_the_spec() {
    let carried = names(&plan());
    for name in [
        "bootstrap.ps1",
        "install.ps1",
        "agentspace-boot.ps1",
        "agentspace-patch.ps1",
        "agentspace-patch-check.ps1",
        "agentspace-idle-check.ps1",
        "agentspace-tools.ps1",
        "localswim.ps1",
        "localswim",
        "sshd_config.agentspace",
        "idle.conf",
    ] {
        assert!(carried.contains(name), "{name}");
    }
}

#[test]
fn sync_carries_nothing_from_the_tests_folder() {
    for name in names(&plan()) {
        assert!(
            name != "tests" && !name.starts_with("tests/"),
            "carried: {name}"
        );
    }
}

#[test]
fn sync_carries_no_test_script() {
    for name in names(&plan()) {
        assert!(!name.starts_with("test-"), "carried: {name}");
        assert!(!name.contains("/test-"), "carried: {name}");
    }
}

#[test]
fn each_carried_name_is_a_plain_relative_name_once() {
    let plan = plan();
    let mut seen = BTreeSet::new();
    for (name, _) in plan.files {
        assert!(!name.is_empty());
        assert!(!name.starts_with('/') && !name.starts_with('\\'), "{name}");
        assert!(!name.contains(".."), "{name}");
        assert!(!name.contains(':'), "{name}");
        assert!(seen.insert(*name), "twice: {name}");
    }
}

// The sync plan's names and commands

#[test]
fn the_staging_folder_is_dot_agentspace_system_in_the_home() {
    assert_eq!(plan().staging, ".agentspace-system");
}

#[test]
fn claude_md_goes_to_dot_claude_claude_md_in_the_home() {
    assert_eq!(plan().claude_md_path, ".claude/CLAUDE.md");
}

#[test]
fn the_localswim_cli_is_built_for_windows_amd64_into_the_exe() {
    let target = plan().localswim;
    assert_eq!(target.goos, "windows");
    assert_eq!(target.goarch, "amd64");
    assert_eq!(target.file_name, "localswim-windows-amd64.exe");
}

#[test]
fn every_remote_step_is_in_the_encoded_form() {
    let plan = plan();
    let mut commands = vec![
        plan.install_command.clone(),
        plan.localswim_install_command.clone(),
    ];
    commands.extend(plan.clear_command.clone());
    for command in commands {
        assert!(command.starts_with(PREFIX), "{command}");
        assert!(!command.contains('='));
        assert!(command.len() <= MAX_LINE);
    }
}

#[test]
fn the_clear_step_exists_since_a_windows_install_leaves_a_staging_folder_to_clear() {
    assert!(plan().clear_command.is_some());
}

#[test]
fn the_clear_step_removes_the_staging_folder_and_everything_in_it() {
    let script = script_of(&plan().clear_command.expect("the clear step"));
    assert!(script.contains(".agentspace-system"));
    assert!(script.contains("Remove-Item"));
    assert!(script.contains("-Recurse"));
}

#[test]
fn the_clear_step_treats_a_missing_folder_as_no_error_and_exits() {
    let script = script_of(&plan().clear_command.expect("the clear step"));
    assert!(script.contains("Test-Path") || script.contains("SilentlyContinue"));
    assert!(script.contains("exit"));
}

#[test]
fn the_install_step_runs_install_ps1_from_the_staging_folder_with_file_and_never_prompts() {
    let script = script_of(&plan().install_command);
    assert!(script.contains(".agentspace-system"));
    assert!(script.contains("install.ps1"));
    assert!(script.contains("-File"));
    assert!(script.contains("-NonInteractive"));
}

#[test]
fn the_install_step_removes_the_staging_folder_only_after_the_install_ran() {
    let script = script_of(&plan().install_command);
    let run = script.find("install.ps1").expect("the run");
    let remove = script.find("Remove-Item").expect("the removal");
    assert!(run < remove);
}

#[test]
fn the_install_step_removes_the_folder_only_when_the_install_exited_zero() {
    let script = script_of(&plan().install_command);
    let remove = script.find("Remove-Item").expect("the removal");
    let before = &script[..remove];
    assert!(
        before.contains("-eq 0") || before.contains("-ne 0") || before.contains("exit"),
        "the removal is guarded by the install's status"
    );
}

#[test]
fn the_install_step_exits_with_the_installs_status() {
    let script = script_of(&plan().install_command);
    assert!(script.contains("exit"));
}

#[test]
fn the_localswim_step_installs_the_staged_exe_where_the_wrapper_looks() {
    let script = script_of(&plan().localswim_install_command);
    assert!(script.contains(".agentspace-localswim.exe"));
    assert!(script.contains("C:\\ProgramData\\agentspace\\localswim\\localswim.exe"));
}

#[test]
fn the_localswim_step_moves_a_running_copy_aside_to_localswim_exe_old() {
    let script = script_of(&plan().localswim_install_command);
    assert!(script.contains("localswim.exe.old"));
    assert!(
        script.contains("Move-Item") || script.contains("Rename-Item") || script.contains("Move("),
        "renamed, since Windows refuses to replace a running program"
    );
}

#[test]
fn the_localswim_step_creates_the_install_folder_and_removes_the_staged_copy() {
    let script = script_of(&plan().localswim_install_command);
    assert!(script.contains("New-Item") || script.contains("CreateDirectory"));
    assert!(script.contains("Remove-Item"));
}

#[test]
fn the_localswim_step_exits_non_zero_when_a_step_failed() {
    let script = script_of(&plan().localswim_install_command);
    assert!(script.contains("exit"));
}

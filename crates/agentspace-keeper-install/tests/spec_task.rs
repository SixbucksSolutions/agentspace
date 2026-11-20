//! The task definition the installer builds, field by field against keeper.md, The task's fields.
//! Nothing here touches Task Scheduler.

use agentspace_keeper_install::task_definition;
use std::path::PathBuf;

const USER: &str = "EXAMPLEDOM\\someone";
const BOUNDARY: &str = "2026-11-19T10:20:30Z";

fn folder() -> PathBuf {
    std::env::temp_dir().join("spec-task-folder").join("bin")
}

fn profile() -> PathBuf {
    std::env::temp_dir().join("spec-task-profile")
}

#[test]
fn name_and_folder() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert_eq!(def.name, "agentspace board holder");
    assert_eq!(def.folder, "\\");
}

#[test]
fn description() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert_eq!(
        def.description,
        "Keeps the agentspace board holder running while a lease is ready (D30)."
    );
}

#[test]
fn trigger_one_is_at_logon_of_the_installing_user() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert_eq!(def.logon_trigger_user, USER);
}

#[test]
fn trigger_two_starts_at_registration_in_utc_repeats_every_minute_with_no_end() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert_eq!(def.start_boundary, BOUNDARY);
    assert!(def.start_boundary.ends_with('Z'));
    assert_eq!(def.end_boundary, None);
    assert_eq!(def.repetition_interval, "PT1M");
    assert_eq!(def.repetition_duration, None);
    assert!(!def.stop_at_duration_end);
}

#[test]
fn the_start_boundary_is_whatever_was_passed() {
    let def = task_definition(&folder(), &profile(), USER, "2027-01-02T03:04:05Z");
    assert_eq!(def.start_boundary, "2027-01-02T03:04:05Z");
}

#[test]
fn principal_is_the_installing_user_interactive_limited() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert_eq!(def.principal_user, USER);
    assert_eq!(def.logon_type, "Interactive");
    assert_eq!(def.run_level, "Limited");
}

#[test]
fn both_users_follow_the_user_passed() {
    let def = task_definition(&folder(), &profile(), "OTHERDOM\\another", BOUNDARY);
    assert_eq!(def.logon_trigger_user, "OTHERDOM\\another");
    assert_eq!(def.principal_user, "OTHERDOM\\another");
}

#[test]
fn settings() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert_eq!(def.execution_time_limit, "PT0S");
    assert_eq!(def.multiple_instances, "IgnoreNew");
    assert!(def.start_when_available);
    assert!(!def.disallow_start_if_on_batteries);
    assert!(!def.stop_if_going_on_batteries);
    assert_eq!(def.restart_count, 3);
    assert_eq!(def.restart_interval, "PT1M");
}

#[test]
fn action_is_the_launcher_by_full_path_with_dash_watch() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert_eq!(def.execute, folder().join("agentspace-keeper-task.exe"));
    assert_eq!(def.arguments, "-Watch");
}

#[test]
fn action_working_folder_is_the_profile() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert_eq!(def.working_directory, profile());
}

#[test]
fn the_action_follows_the_folder_passed() {
    let other = std::env::temp_dir().join("spec-task-other");
    let def = task_definition(&other, &profile(), USER, BOUNDARY);
    assert_eq!(def.execute, other.join("agentspace-keeper-task.exe"));
}

#[test]
fn task_is_enabled_and_not_hidden() {
    let def = task_definition(&folder(), &profile(), USER, BOUNDARY);
    assert!(def.enabled);
    assert!(!def.hidden);
}

//! The dry run's lines, in order and form (keeper.md, The installer), from the library alone.

use agentspace_keeper_install::{dry_run_lines, task_definition};
use std::path::{Path, PathBuf};

const USER: &str = "EXAMPLEDOM\\someone";
const BOUNDARY_UTC: &str = "2026-11-19T10:20:30Z";

fn base() -> PathBuf {
    std::env::temp_dir().join("spec-dry-run")
}

fn profile() -> PathBuf {
    base().join("profile")
}

fn install() -> PathBuf {
    profile().join(".agentspace").join("bin")
}

fn lines_for(folder: &Path, install: &Path, shown: &str) -> Vec<String> {
    let def = task_definition(folder, &profile(), USER, BOUNDARY_UTC);
    dry_run_lines(&def, folder, install, shown)
}

fn expected_body(folder: &Path, shown: &str) -> Vec<String> {
    let f = folder.display();
    let p = profile();
    vec![
        "Name: agentspace board holder".to_string(),
        "Description: Keeps the agentspace board holder running while a lease is ready (D30)."
            .to_string(),
        format!("Trigger 1: AtLogOn user={USER}"),
        format!("Trigger 2: Once StartBoundary={shown} EndBoundary=none StopAtDurationEnd=False"),
        "Repetition 2: Interval=PT1M Duration=indefinite".to_string(),
        format!("Principal user: {USER}"),
        "RunLevel: Limited".to_string(),
        "LogonType: Interactive".to_string(),
        "ExecutionTimeLimit: PT0S (none)".to_string(),
        "MultipleInstances: IgnoreNew".to_string(),
        "StartWhenAvailable: True".to_string(),
        "DisallowStartIfOnBatteries: False".to_string(),
        "StopIfGoingOnBatteries: False".to_string(),
        "RestartCount: 3".to_string(),
        "RestartInterval: PT1M".to_string(),
        format!("Action Execute: {f}\\agentspace-keeper-task.exe"),
        "Arguments: -Watch".to_string(),
        format!("WorkingDirectory: {}", p.display()),
        format!("Runs: {f}\\agentspace-keeper.exe -Watch"),
        "Window: hidden".to_string(),
        "Start: a real install starts the task immediately after registering it.".to_string(),
        "Mode: dry run; nothing was registered or started.".to_string(),
    ]
}

fn warning(folder: &Path, install: &Path) -> String {
    format!(
        "Warning: {} is not the install folder {}. A real install from here is refused, because the task would run programs that a cleanup can remove.",
        folder.display(),
        install.display()
    )
}

#[test]
fn outside_the_install_folder_the_warning_comes_first_then_every_line_in_order() {
    let folder = base().join("build").join("debug");
    let mut expected = vec![warning(&folder, &install())];
    expected.extend(expected_body(&folder, BOUNDARY_UTC));
    assert_eq!(lines_for(&folder, &install(), BOUNDARY_UTC), expected);
}

#[test]
fn inside_the_install_folder_there_is_no_warning() {
    let folder = install();
    let lines = lines_for(&folder, &install(), BOUNDARY_UTC);
    assert_eq!(lines, expected_body(&folder, BOUNDARY_UTC));
    assert_eq!(lines.len(), 22);
    assert!(lines.iter().all(|l| !l.starts_with("Warning:")));
}

#[test]
fn folder_equal_to_install_without_regard_to_case_gets_no_warning() {
    let upper = PathBuf::from(install().display().to_string().to_uppercase());
    let lower = PathBuf::from(install().display().to_string().to_lowercase());
    for folder in [upper, lower] {
        let lines = lines_for(&folder, &install(), BOUNDARY_UTC);
        assert!(lines.iter().all(|l| !l.starts_with("Warning:")));
        assert_eq!(lines.len(), 22);
    }
}

#[test]
fn a_different_folder_gets_the_warning_naming_both_paths() {
    let folder = base().join("elsewhere");
    let lines = lines_for(&folder, &install(), BOUNDARY_UTC);
    assert_eq!(lines.len(), 23);
    assert_eq!(lines[0], warning(&folder, &install()));
}

#[test]
fn a_folder_that_only_starts_like_the_install_folder_gets_the_warning() {
    let folder = PathBuf::from(format!("{}2", install().display()));
    let lines = lines_for(&folder, &install(), BOUNDARY_UTC);
    assert!(lines[0].starts_with("Warning: "));
}

#[test]
fn trigger_two_always_prints_stop_at_duration_end_false() {
    for folder in [install(), base().join("elsewhere")] {
        let lines = lines_for(&folder, &install(), BOUNDARY_UTC);
        let trigger = lines
            .iter()
            .find(|l| l.starts_with("Trigger 2:"))
            .expect("A Trigger 2 line");
        assert!(trigger.ends_with(" StopAtDurationEnd=False"), "{trigger}");
        assert!(trigger.contains(" EndBoundary=none"), "{trigger}");
    }
}

#[test]
fn trigger_two_shows_the_boundary_as_the_definition_holds_it_in_utc() {
    // The architect's correction of FA-17 (2026-11-19): `<registration time>` is the definition's
    // own boundary, UTC with a `Z`, never local time with an offset.
    let def = task_definition(&install(), &profile(), USER, BOUNDARY_UTC);
    let lines = dry_run_lines(&def, &install(), &install(), &def.start_boundary);
    let trigger = lines.iter().find(|l| l.starts_with("Trigger 2:")).unwrap();
    assert!(
        trigger.contains(&format!("StartBoundary={BOUNDARY_UTC} ")),
        "{trigger}"
    );
}

#[test]
fn repetition_is_every_minute_with_indefinite_duration() {
    let lines = lines_for(&install(), &install(), BOUNDARY_UTC);
    assert!(
        lines
            .iter()
            .any(|l| l == "Repetition 2: Interval=PT1M Duration=indefinite")
    );
}

#[test]
fn every_path_is_expanded_with_no_placeholder_left() {
    let folder = base().join("build").join("debug");
    let lines = lines_for(&folder, &install(), BOUNDARY_UTC);
    for line in &lines {
        assert!(!line.contains('%'), "{line}");
        assert!(!line.contains("<folder>"), "{line}");
        assert!(!line.contains("<install>"), "{line}");
        assert!(!line.contains("<profile>"), "{line}");
    }
}

#[test]
fn lines_have_no_line_endings() {
    for line in lines_for(&base().join("x"), &install(), BOUNDARY_UTC) {
        assert!(!line.contains('\n') && !line.contains('\r'), "{line}");
    }
}

#[test]
fn the_user_appears_in_trigger_one_and_the_principal_line_alike() {
    let lines = lines_for(&install(), &install(), BOUNDARY_UTC);
    let t1 = lines.iter().find(|l| l.starts_with("Trigger 1:")).unwrap();
    let pr = lines
        .iter()
        .find(|l| l.starts_with("Principal user:"))
        .unwrap();
    assert_eq!(t1.strip_prefix("Trigger 1: AtLogOn user="), Some(USER));
    assert_eq!(pr.strip_prefix("Principal user: "), Some(USER));
}

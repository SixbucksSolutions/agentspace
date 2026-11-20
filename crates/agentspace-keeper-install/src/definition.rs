//! The task's definition as plain data, and the lines that print it (keeper.md, The task's fields;
//! The installer, `-DryRun`). Nothing here touches Task Scheduler or COM.

use std::path::{Path, PathBuf};

/// The task's name, in Task Scheduler's root folder.
pub const TASK_NAME: &str = "agentspace board holder";

/// The task's launcher, in the install folder.
pub const LAUNCHER_EXE: &str = "agentspace-keeper-task.exe";

/// The keeper, in the install folder.
pub const KEEPER_EXE: &str = "agentspace-keeper.exe";

/// The one argument the task's action passes.
pub const ACTION_ARGUMENTS: &str = "-Watch";

/// The task's definition as plain data (keeper.md, The task's fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDefinition {
    pub name: String,
    pub folder: String,
    pub description: String,
    pub logon_trigger_user: String,
    pub start_boundary: String,
    pub end_boundary: Option<String>,
    pub repetition_interval: String,
    pub repetition_duration: Option<String>,
    pub stop_at_duration_end: bool,
    pub principal_user: String,
    pub logon_type: String,
    pub run_level: String,
    pub execution_time_limit: String,
    pub multiple_instances: String,
    pub start_when_available: bool,
    pub disallow_start_if_on_batteries: bool,
    pub stop_if_going_on_batteries: bool,
    pub restart_count: u32,
    pub restart_interval: String,
    pub execute: PathBuf,
    pub arguments: String,
    pub working_directory: PathBuf,
    pub enabled: bool,
    pub hidden: bool,
}

/// The definition the installer registers. `folder` is the folder of the installer's own exe,
/// `profile` is `%USERPROFILE%`'s value, `user` is `DOMAIN\user`, and `start_boundary_utc` is the
/// registration time, UTC with a `Z`.
pub fn task_definition(
    folder: &Path,
    profile: &Path,
    user: &str,
    start_boundary_utc: &str,
) -> TaskDefinition {
    TaskDefinition {
        name: TASK_NAME.to_string(),
        folder: "\\".to_string(),
        description: "Keeps the agentspace board holder running while a lease is ready (D30)."
            .to_string(),
        logon_trigger_user: user.to_string(),
        start_boundary: start_boundary_utc.to_string(),
        end_boundary: None,
        repetition_interval: "PT1M".to_string(),
        repetition_duration: None,
        stop_at_duration_end: false,
        principal_user: user.to_string(),
        logon_type: "Interactive".to_string(),
        run_level: "Limited".to_string(),
        execution_time_limit: "PT0S".to_string(),
        multiple_instances: "IgnoreNew".to_string(),
        start_when_available: true,
        disallow_start_if_on_batteries: false,
        stop_if_going_on_batteries: false,
        restart_count: 3,
        restart_interval: "PT1M".to_string(),
        execute: folder.join(LAUNCHER_EXE),
        arguments: ACTION_ARGUMENTS.to_string(),
        working_directory: profile.to_path_buf(),
        enabled: true,
        hidden: false,
    }
}

/// What a trigger is, for the lines that print it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerKind {
    AtLogOn,
    Once,
    /// A trigger of another kind, by the name `Trigger <n>:` prints.
    Other(String),
}

/// One trigger of a task, as the lines print it: from the definition for a dry run, or read back
/// from the registered task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerInfo {
    pub kind: TriggerKind,
    /// The logon trigger's user.
    pub user: String,
    /// The time trigger's start boundary, as the caller shows it.
    pub start_boundary: String,
    /// The time trigger's end boundary; `None` or empty prints as `none`.
    pub end_boundary: Option<String>,
    /// The repetition interval; `None` or empty means the trigger repeats nothing.
    pub repetition_interval: Option<String>,
    /// The repetition duration; `None` or empty prints as `indefinite`.
    pub repetition_duration: Option<String>,
    /// Printed only when the repetition reports the field.
    pub stop_at_duration_end: Option<bool>,
}

/// `True` or `False`, as the dry run spells a flag.
fn flag(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|text| !text.is_empty())
}

/// The `Trigger <n>:` lines, and a `Repetition <n>:` line after any trigger that has an interval
/// (keeper.md, The installer, `-DryRun`). `n` counts from 1, so the dry run and a real install's
/// read-back print the same forms.
pub fn trigger_lines(triggers: &[TriggerInfo]) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, trigger) in triggers.iter().enumerate() {
        let n = index + 1;
        let line = match &trigger.kind {
            TriggerKind::AtLogOn => format!("Trigger {n}: AtLogOn user={}", trigger.user),
            TriggerKind::Once => {
                let mut line = format!(
                    "Trigger {n}: Once StartBoundary={} EndBoundary={}",
                    trigger.start_boundary,
                    non_empty(&trigger.end_boundary).unwrap_or("none")
                );
                if let Some(stop) = trigger.stop_at_duration_end {
                    line.push_str(&format!(" StopAtDurationEnd={}", flag(stop)));
                }
                line
            }
            TriggerKind::Other(kind) => format!("Trigger {n}: {kind}"),
        };
        lines.push(line);
        if let Some(interval) = non_empty(&trigger.repetition_interval) {
            lines.push(format!(
                "Repetition {n}: Interval={interval} Duration={}",
                non_empty(&trigger.repetition_duration).unwrap_or("indefinite")
            ));
        }
    }
    lines
}

/// The two triggers a definition holds, for the dry run's lines. `registration_time` is the
/// time shown for trigger 2's start boundary.
fn definition_triggers(def: &TaskDefinition, registration_time: &str) -> Vec<TriggerInfo> {
    vec![
        TriggerInfo {
            kind: TriggerKind::AtLogOn,
            user: def.logon_trigger_user.clone(),
            start_boundary: String::new(),
            end_boundary: None,
            repetition_interval: None,
            repetition_duration: None,
            stop_at_duration_end: None,
        },
        TriggerInfo {
            kind: TriggerKind::Once,
            user: String::new(),
            start_boundary: registration_time.to_string(),
            end_boundary: def.end_boundary.clone(),
            repetition_interval: Some(def.repetition_interval.clone()),
            repetition_duration: def.repetition_duration.clone(),
            stop_at_duration_end: Some(def.stop_at_duration_end),
        },
    ]
}

/// A path as it is compared: with `/` as `\`, no `\\?\` prefix, no trailing separator, in lower
/// case.
fn normalized(path: &Path) -> String {
    let text = path.display().to_string().replace('/', "\\");
    let text = text.strip_prefix("\\\\?\\").unwrap_or(&text);
    text.trim_end_matches('\\').to_lowercase()
}

/// Whether two folders are the same one, as full paths without regard to case. A path that can
/// be resolved on disk (a short name, a link) is compared resolved when the text differs. A real
/// install therefore builds its action from the install folder's own path, never from the folder
/// it was run from, so a link into the install folder never puts the link's path in the task.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    if normalized(a) == normalized(b) {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => normalized(&a) == normalized(&b),
        _ => false,
    }
}

/// The `Warning:` line for a folder that is not the install folder.
pub fn not_install_warning(folder: &Path, install: &Path) -> String {
    format!(
        "Warning: {} is not the install folder {}. A real install from here is refused, because the task would run programs that a cleanup can remove.",
        folder.display(),
        install.display()
    )
}

/// The dry run's lines, in keeper.md's order and forms, without line endings. `install` is
/// `%USERPROFILE%\.agentspace\bin`; `registration_time` is what `Trigger 2:` prints as
/// `StartBoundary=`, which the installer passes as the definition's own boundary, UTC with a `Z`,
/// `YYYY-MM-DDTHH:MM:SSZ` (architect, 2026-11-19, for E0.06-T02-S06, which corrected FA-17's first
/// ruling of local time with an offset: the dry run prints the boundary exactly as the definition
/// holds it).
pub fn dry_run_lines(
    def: &TaskDefinition,
    folder: &Path,
    install: &Path,
    registration_time: &str,
) -> Vec<String> {
    let mut lines = Vec::new();
    if !same_folder(folder, install) {
        lines.push(not_install_warning(folder, install));
    }
    lines.push(format!("Name: {}", def.name));
    lines.push(format!("Description: {}", def.description));
    lines.extend(trigger_lines(&definition_triggers(def, registration_time)));
    lines.push(format!("Principal user: {}", def.principal_user));
    lines.push(format!("RunLevel: {}", def.run_level));
    lines.push(format!("LogonType: {}", def.logon_type));
    lines.push(format!(
        "ExecutionTimeLimit: {}{}",
        def.execution_time_limit,
        if def.execution_time_limit == "PT0S" {
            " (none)"
        } else {
            ""
        }
    ));
    lines.push(format!("MultipleInstances: {}", def.multiple_instances));
    lines.push(format!(
        "StartWhenAvailable: {}",
        flag(def.start_when_available)
    ));
    lines.push(format!(
        "DisallowStartIfOnBatteries: {}",
        flag(def.disallow_start_if_on_batteries)
    ));
    lines.push(format!(
        "StopIfGoingOnBatteries: {}",
        flag(def.stop_if_going_on_batteries)
    ));
    lines.push(format!("RestartCount: {}", def.restart_count));
    lines.push(format!("RestartInterval: {}", def.restart_interval));
    lines.push(format!("Action Execute: {}", def.execute.display()));
    lines.push(format!("Arguments: {}", def.arguments));
    lines.push(format!(
        "WorkingDirectory: {}",
        def.working_directory.display()
    ));
    lines.push(format!(
        "Runs: {} {}",
        folder.join(KEEPER_EXE).display(),
        def.arguments
    ));
    lines.push("Window: hidden".to_string());
    lines.push(
        "Start: a real install starts the task immediately after registering it.".to_string(),
    );
    lines.push("Mode: dry run; nothing was registered or started.".to_string());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "EXAMPLEDOM\\someone";

    fn sample() -> TaskDefinition {
        task_definition(
            Path::new("C:\\example\\bin"),
            Path::new("C:\\example"),
            USER,
            "2026-11-19T10:20:30Z",
        )
    }

    #[test]
    fn the_definition_matches_the_tasks_fields_field_by_field() {
        let def = sample();
        assert_eq!(def.name, "agentspace board holder");
        assert_eq!(def.folder, "\\");
        assert_eq!(
            def.description,
            "Keeps the agentspace board holder running while a lease is ready (D30)."
        );
        assert_eq!(def.logon_trigger_user, USER);
        assert_eq!(def.start_boundary, "2026-11-19T10:20:30Z");
        assert_eq!(def.end_boundary, None);
        assert_eq!(def.repetition_interval, "PT1M");
        assert_eq!(def.repetition_duration, None);
        assert!(!def.stop_at_duration_end);
        assert_eq!(def.principal_user, USER);
        assert_eq!(def.logon_type, "Interactive");
        assert_eq!(def.run_level, "Limited");
        assert_eq!(def.execution_time_limit, "PT0S");
        assert_eq!(def.multiple_instances, "IgnoreNew");
        assert!(def.start_when_available);
        assert!(!def.disallow_start_if_on_batteries);
        assert!(!def.stop_if_going_on_batteries);
        assert_eq!(def.restart_count, 3);
        assert_eq!(def.restart_interval, "PT1M");
        assert_eq!(
            def.execute,
            Path::new("C:\\example\\bin").join(LAUNCHER_EXE)
        );
        assert_eq!(def.arguments, "-Watch");
        assert_eq!(def.working_directory, Path::new("C:\\example"));
        assert!(def.enabled);
        assert!(!def.hidden);
    }

    #[cfg(windows)]
    #[test]
    fn the_dry_run_prints_every_line_in_order() {
        let def = sample();
        let install = Path::new("C:\\example\\.agentspace\\bin");
        let lines = dry_run_lines(
            &def,
            Path::new("C:\\example\\bin"),
            install,
            &def.start_boundary,
        );
        let expected = [
            "Warning: C:\\example\\bin is not the install folder C:\\example\\.agentspace\\bin. A real install from here is refused, because the task would run programs that a cleanup can remove.",
            "Name: agentspace board holder",
            "Description: Keeps the agentspace board holder running while a lease is ready (D30).",
            "Trigger 1: AtLogOn user=EXAMPLEDOM\\someone",
            "Trigger 2: Once StartBoundary=2026-11-19T10:20:30Z EndBoundary=none StopAtDurationEnd=False",
            "Repetition 2: Interval=PT1M Duration=indefinite",
            "Principal user: EXAMPLEDOM\\someone",
            "RunLevel: Limited",
            "LogonType: Interactive",
            "ExecutionTimeLimit: PT0S (none)",
            "MultipleInstances: IgnoreNew",
            "StartWhenAvailable: True",
            "DisallowStartIfOnBatteries: False",
            "StopIfGoingOnBatteries: False",
            "RestartCount: 3",
            "RestartInterval: PT1M",
            "Action Execute: C:\\example\\bin\\agentspace-keeper-task.exe",
            "Arguments: -Watch",
            "WorkingDirectory: C:\\example",
            "Runs: C:\\example\\bin\\agentspace-keeper.exe -Watch",
            "Window: hidden",
            "Start: a real install starts the task immediately after registering it.",
            "Mode: dry run; nothing was registered or started.",
        ];
        assert_eq!(lines, expected);
    }

    #[test]
    fn no_warning_inside_the_install_folder_without_regard_to_case() {
        let def = sample();
        let lines = dry_run_lines(
            &def,
            Path::new("C:\\Example\\.AgentSpace\\BIN\\"),
            Path::new("c:\\example\\.agentspace\\bin"),
            "L",
        );
        assert_eq!(lines.len(), 22);
        assert!(lines[0].starts_with("Name:"));
    }

    #[test]
    fn a_folder_that_only_starts_like_the_install_folder_is_another() {
        assert!(!same_folder(
            Path::new("C:\\example\\bin2"),
            Path::new("C:\\example\\bin")
        ));
    }

    #[test]
    fn read_back_lines_use_the_same_forms() {
        let triggers = [
            TriggerInfo {
                kind: TriggerKind::AtLogOn,
                user: USER.to_string(),
                start_boundary: String::new(),
                end_boundary: None,
                repetition_interval: None,
                repetition_duration: None,
                stop_at_duration_end: None,
            },
            TriggerInfo {
                kind: TriggerKind::Once,
                user: String::new(),
                start_boundary: "2026-11-19T10:20:30Z".to_string(),
                end_boundary: Some(String::new()),
                repetition_interval: Some("PT1M".to_string()),
                repetition_duration: Some(String::new()),
                stop_at_duration_end: Some(false),
            },
        ];
        assert_eq!(
            trigger_lines(&triggers),
            [
                format!("Trigger 1: AtLogOn user={USER}"),
                "Trigger 2: Once StartBoundary=2026-11-19T10:20:30Z EndBoundary=none StopAtDurationEnd=False".to_string(),
                "Repetition 2: Interval=PT1M Duration=indefinite".to_string(),
            ]
        );
    }

    #[test]
    fn another_kind_prints_its_kind_and_a_repetition_only_with_an_interval() {
        let other = |interval: Option<&str>, duration: Option<&str>| TriggerInfo {
            kind: TriggerKind::Other("Daily".to_string()),
            user: String::new(),
            start_boundary: String::new(),
            end_boundary: None,
            repetition_interval: interval.map(str::to_string),
            repetition_duration: duration.map(str::to_string),
            stop_at_duration_end: None,
        };
        assert_eq!(trigger_lines(&[other(None, None)]), ["Trigger 1: Daily"]);
        assert_eq!(
            trigger_lines(&[other(Some("PT5M"), Some("PT1H"))]),
            [
                "Trigger 1: Daily",
                "Repetition 1: Interval=PT5M Duration=PT1H"
            ]
        );
    }

    #[test]
    fn a_time_trigger_without_the_stop_field_omits_it() {
        let trigger = TriggerInfo {
            kind: TriggerKind::Once,
            user: String::new(),
            start_boundary: "S".to_string(),
            end_boundary: Some("E".to_string()),
            repetition_interval: None,
            repetition_duration: None,
            stop_at_duration_end: None,
        };
        assert_eq!(
            trigger_lines(&[trigger]),
            ["Trigger 1: Once StartBoundary=S EndBoundary=E"]
        );
    }
}

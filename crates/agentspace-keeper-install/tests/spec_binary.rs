//! The built installer, run only with usage-error arguments and with `-DryRun`, in a throwaway
//! home and a throwaway working folder. Never run with no arguments or with `-Uninstall`.
//! No assert message shows the DOMAIN\user value.

use agentspace_keeper_install::{Mode, parse_args};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const EXE: &str = env!("CARGO_BIN_EXE_agentspace-keeper-install");
const USAGE: &str = "Usage: agentspace-keeper-install [-DryRun | -Uninstall]";

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// The guard on every run below: the installer is started only with arguments that the library's
/// own parser, which the binary uses, refuses or reads as `-DryRun`. A real install and
/// `-Uninstall` reach Task Scheduler, so a parser that wrongly accepted, say, `-Uninstall
/// -UNINSTALL` stops the test here instead of removing a real task.
fn refuse_a_mode_that_reaches_task_scheduler(args: &[&str]) {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    match parse_args(&owned) {
        Err(_) | Ok(Mode::DryRun) => {}
        Ok(Mode::Install) | Ok(Mode::Uninstall) => {
            panic!("Refused to start the installer in a mode that reaches Task Scheduler: {args:?}")
        }
    }
}

struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    work: PathBuf,
}

impl Sandbox {
    fn new() -> Sandbox {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "agentspace-keeper-install-spec-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let work = root.join("work");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        Sandbox { root, home, work }
    }

    fn install_folder(&self) -> PathBuf {
        self.home.join(".agentspace").join("bin")
    }

    fn run(&self, exe: &Path, args: &[&str]) -> Output {
        self.run_with_profile(exe, args, Some(self.home.as_os_str()))
    }

    /// Runs the installer with `USERPROFILE` set to `profile`, or unset with `None`. Every other
    /// variable a child could take a home from points into the sandbox.
    fn run_with_profile(&self, exe: &Path, args: &[&str], profile: Option<&OsStr>) -> Output {
        refuse_a_mode_that_reaches_task_scheduler(args);
        let mut command = Command::new(exe);
        command.args(args).env_clear().current_dir(&self.work);
        if let Some(profile) = profile {
            command.env("USERPROFILE", profile);
        }
        command
            .env("HOME", &self.home)
            .env("LOCALAPPDATA", self.home.join("AppData").join("Local"))
            .env("APPDATA", self.home.join("AppData").join("Roaming"));
        for name in ["SystemRoot", "windir"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.output().expect("The installer starts")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn lines_of(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

fn assert_usage_error(args: &[&str], message: &str) {
    let sandbox = Sandbox::new();
    let out = sandbox.run(Path::new(EXE), args);
    assert_eq!(out.status.code(), Some(2), "{args:?}");
    assert!(out.stdout.is_empty(), "{args:?}");
    assert_eq!(
        lines_of(&text(&out.stderr)),
        vec![
            format!("agentspace-keeper-install: {message}"),
            USAGE.to_string()
        ],
        "{args:?}"
    );
}

#[test]
fn unknown_argument_gives_two_stderr_lines_and_exit_2() {
    assert_usage_error(&["-Bogus"], "Unknown argument '-Bogus'.");
    assert_usage_error(&["-h"], "Unknown argument '-h'.");
    assert_usage_error(&["--help"], "Unknown argument '--help'.");
    assert_usage_error(&["-?"], "Unknown argument '-?'.");
    assert_usage_error(&["Positional"], "Unknown argument 'Positional'.");
}

#[test]
fn a_name_given_twice_gives_two_stderr_lines_and_exit_2() {
    assert_usage_error(&["-DryRun", "-dryrun"], "-DryRun is given twice.");
    assert_usage_error(&["-Uninstall", "-UNINSTALL"], "-Uninstall is given twice.");
}

#[test]
fn dry_run_with_uninstall_gives_two_stderr_lines_and_exit_2() {
    assert_usage_error(
        &["-DryRun", "-Uninstall"],
        "-DryRun and -Uninstall cannot be given together.",
    );
    assert_usage_error(
        &["-uninstall", "-dryrun"],
        "-DryRun and -Uninstall cannot be given together.",
    );
}

#[test]
fn the_first_failing_check_is_the_one_reported() {
    assert_usage_error(
        &["-DryRun", "-Uninstall", "-Bogus"],
        "Unknown argument '-Bogus'.",
    );
    assert_usage_error(
        &["-DryRun", "-Uninstall", "-DryRun"],
        "-DryRun is given twice.",
    );
}

/// Splits `Label: value` and returns the value of the first line with that label.
fn value<'a>(lines: &'a [String], label: &str) -> &'a str {
    let prefix = format!("{label}: ");
    lines
        .iter()
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("No {label} line"))
}

/// The DOMAIN\user form: a backslash between two non-empty parts.
fn assert_account_form(account: &str, what: &str) {
    let (domain, user) = account
        .split_once('\\')
        .unwrap_or_else(|| panic!("{what} has no backslash"));
    assert!(!domain.is_empty(), "{what} has an empty domain");
    assert!(!user.is_empty(), "{what} has an empty user");
    assert!(!user.contains('\\'), "{what} has a second backslash");
}

/// Checks `YYYY-MM-DDTHH:MM:SSZ`: the boundary as the definition holds it, UTC with a `Z`
/// (architect, 2026-11-19, correcting FA-17's first ruling of local time with an offset).
fn assert_utc_time_form(time: &str) {
    let b = time.as_bytes();
    assert_eq!(b.len(), 20, "The registration time has the wrong length");
    let digit = |i: usize| b[i].is_ascii_digit();
    for i in [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18] {
        assert!(digit(i), "Position {i} of the registration time");
    }
    assert_eq!(b[4], b'-');
    assert_eq!(b[7], b'-');
    assert_eq!(b[10], b'T');
    assert_eq!(b[13], b':');
    assert_eq!(b[16], b':');
    assert_eq!(b[19], b'Z');
    let num = |a: usize, z: usize| time[a..z].parse::<u32>().unwrap();
    assert!((1..=12).contains(&num(5, 7)));
    assert!((1..=31).contains(&num(8, 10)));
    assert!(num(11, 13) < 24);
    assert!(num(14, 16) < 60);
    assert!(num(17, 19) < 61);
}

fn strip_verbatim(path: &str) -> &str {
    path.strip_prefix("\\\\?\\").unwrap_or(path)
}

fn same_path_ignoring_case(a: &str, b: &str) -> bool {
    strip_verbatim(a).eq_ignore_ascii_case(strip_verbatim(b))
}

/// Checks everything a dry run shares, whatever folder it ran from. Returns the folder it
/// printed and the lines.
fn check_dry_run(sandbox: &Sandbox, out: &Output) -> (String, Vec<String>) {
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty(), "A dry run writes nothing to stderr");
    let lines = lines_of(&text(&out.stdout));
    let body: Vec<&String> = lines
        .iter()
        .filter(|l| !l.starts_with("Warning: "))
        .collect();
    assert_eq!(body.len(), 22, "The dry run's line count");

    // Fixed lines, in order.
    let fixed = [
        (0, "Name: agentspace board holder"),
        (
            1,
            "Description: Keeps the agentspace board holder running while a lease is ready (D30).",
        ),
        (4, "Repetition 2: Interval=PT1M Duration=indefinite"),
        (6, "RunLevel: Limited"),
        (7, "LogonType: Interactive"),
        (8, "ExecutionTimeLimit: PT0S (none)"),
        (9, "MultipleInstances: IgnoreNew"),
        (10, "StartWhenAvailable: True"),
        (11, "DisallowStartIfOnBatteries: False"),
        (12, "StopIfGoingOnBatteries: False"),
        (13, "RestartCount: 3"),
        (14, "RestartInterval: PT1M"),
        (16, "Arguments: -Watch"),
        (19, "Window: hidden"),
        (
            20,
            "Start: a real install starts the task immediately after registering it.",
        ),
        (21, "Mode: dry run; nothing was registered or started."),
    ];
    // Compared without printing the line, since a shifted line can be one that holds the user.
    for (i, expected) in fixed {
        assert!(body[i] == expected, "Line {i} is not {expected:?}");
    }

    // Accounts: the same value in Trigger 1 and Principal user, in the right form.
    let trigger1 = body[2]
        .strip_prefix("Trigger 1: AtLogOn user=")
        .expect("Trigger 1 form");
    let principal = body[5]
        .strip_prefix("Principal user: ")
        .expect("Principal user form");
    assert_account_form(trigger1, "Trigger 1's user");
    assert_account_form(principal, "The principal's user");
    assert!(trigger1 == principal, "Trigger 1 and the principal differ");

    // Trigger 2: the registration time in UTC form, no end, StopAtDurationEnd=False.
    let rest = body[3]
        .strip_prefix("Trigger 2: Once StartBoundary=")
        .expect("Trigger 2 form");
    let (time, tail) = rest.split_once(' ').expect("Trigger 2 has an end part");
    assert_utc_time_form(time);
    assert_eq!(tail, "EndBoundary=none StopAtDurationEnd=False");

    // Paths come from the folder the installer ran in and the throwaway home.
    let execute = value(&lines, "Action Execute");
    let folder = execute
        .strip_suffix("\\agentspace-keeper-task.exe")
        .expect("Action Execute names the launcher");
    assert_eq!(
        value(&lines, "Runs"),
        format!("{folder}\\agentspace-keeper.exe -Watch")
    );
    assert_eq!(
        value(&lines, "WorkingDirectory"),
        sandbox.home.display().to_string()
    );
    assert!(lines.iter().all(|l| !l.contains('%')));
    (folder.to_string(), lines)
}

#[test]
fn dry_run_from_the_build_folder_warns_on_stdout_and_names_the_throwaway_home() {
    let sandbox = Sandbox::new();
    let out = sandbox.run(Path::new(EXE), &["-DryRun"]);
    let (folder, lines) = check_dry_run(&sandbox, &out);
    let install = sandbox.install_folder().display().to_string();

    assert_eq!(lines.len(), 23);
    assert_eq!(
        lines[0],
        format!(
            "Warning: {folder} is not the install folder {install}. A real install from here is refused, because the task would run programs that a cleanup can remove."
        )
    );
    assert!(!same_path_ignoring_case(&folder, &install));
    let built = Path::new(EXE).parent().unwrap().display().to_string();
    assert!(same_path_ignoring_case(&folder, &built));
}

#[test]
fn dry_run_is_matched_without_regard_to_case() {
    let sandbox = Sandbox::new();
    let out = sandbox.run(Path::new(EXE), &["-dRyRuN"]);
    check_dry_run(&sandbox, &out);
}

#[test]
fn dry_run_from_the_install_folder_has_no_warning() {
    let sandbox = Sandbox::new();
    let install = sandbox.install_folder();
    std::fs::create_dir_all(&install).unwrap();
    let copy = install.join("agentspace-keeper-install.exe");
    std::fs::copy(EXE, &copy).unwrap();
    let out = sandbox.run(&copy, &["-DryRun"]);
    let (folder, lines) = check_dry_run(&sandbox, &out);
    assert_eq!(lines.len(), 22);
    assert!(lines.iter().all(|l| !l.starts_with("Warning:")));
    assert!(same_path_ignoring_case(
        &folder,
        &install.display().to_string()
    ));
}

/// keeper.md, The installer (architect, 2026-11-19): USERPROFILE unset or empty ends the
/// installer with exit 1 and one line, after the usage checks, `-DryRun` included.
#[test]
fn dry_run_without_userprofile_exits_one_with_the_ruled_line() {
    let line = "agentspace-keeper-install: USERPROFILE is not set, so there is no home folder.";
    let sandbox = Sandbox::new();
    for profile in [None, Some(OsStr::new(""))] {
        let out = sandbox.run_with_profile(Path::new(EXE), &["-DryRun"], profile);
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stdout.is_empty());
        assert_eq!(lines_of(&text(&out.stderr)), vec![line.to_string()]);
    }
}

#[test]
fn a_usage_error_comes_before_the_userprofile_check() {
    let sandbox = Sandbox::new();
    let out = sandbox.run_with_profile(Path::new(EXE), &["-Bogus"], None);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        lines_of(&text(&out.stderr)),
        vec![
            "agentspace-keeper-install: Unknown argument '-Bogus'.".to_string(),
            USAGE.to_string()
        ]
    );
}

#[test]
fn the_guard_refuses_a_real_install_and_an_uninstall() {
    for args in [&[][..], &["-Uninstall"][..], &["-uninstall"][..]] {
        let refused = std::panic::catch_unwind(|| refuse_a_mode_that_reaches_task_scheduler(args));
        assert!(refused.is_err(), "{args:?}");
    }
    refuse_a_mode_that_reaches_task_scheduler(&["-DryRun"]);
    refuse_a_mode_that_reaches_task_scheduler(&["-Uninstall", "-Uninstall"]);
}

#[test]
fn dry_run_writes_nothing_in_the_throwaway_home_or_working_folder() {
    let sandbox = Sandbox::new();
    let out = sandbox.run(Path::new(EXE), &["-DryRun"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(std::fs::read_dir(&sandbox.work).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&sandbox.home).unwrap().count(), 0);
}

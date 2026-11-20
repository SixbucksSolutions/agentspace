//! The built launcher, run as a program against a stub keeper (docs/keeper.md, The task's
//! launcher and The launcher's error log).
//!
//! Every test makes a throwaway root under the temp folder with three folders: `install`, which
//! holds a copy of the launcher and, unless the test leaves it out, a copy of the test-only stub
//! as `agentspace-keeper.exe`; `home`, the throwaway home; and `work`, the working folder the
//! launcher runs in. The launcher gets `USERPROFILE`, `HOME`, `LOCALAPPDATA` and `APPDATA` that
//! point into `home`, so its error log lands there. The launcher is started with
//! `DETACHED_PROCESS`, as Task Scheduler starts a windowless program, so it has no console to
//! lend its child. A test never prints a value of an environment: it compares, and names the
//! variable that differs.

#![cfg(windows)]

#[path = "support/envdigest.rs"]
mod envdigest;

use agentspace_keeper_task::format_log_time;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;
use tempfile::TempDir;

const DETACHED_PROCESS: u32 = 0x0000_0008;
const MARKER: &str = "AGENTSPACE_LAUNCHER_TEST_MARKER";
const STATE_DIR: &str = "AGENTSPACE_STATE_DIR";
const LOG: &str = "board-holder-launcher-errors.log";

const LAUNCHER: &str = env!("CARGO_BIN_EXE_agentspace-keeper-task");
const STUB: &str = env!("CARGO_BIN_EXE_agentspace-keeper-task-stub-keeper");

struct Fixture {
    _root: TempDir,
    install: PathBuf,
    home: PathBuf,
    work: PathBuf,
}

impl Fixture {
    fn new(with_keeper: bool) -> Fixture {
        let root = tempfile::Builder::new()
            .prefix("agentspace-keeper-task-test-")
            .tempdir()
            .unwrap();
        let install = root.path().join("install");
        let home = root.path().join("home");
        let work = root.path().join("work");
        for dir in [&install, &home, &work] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::copy(LAUNCHER, install.join("agentspace-keeper-task.exe")).unwrap();
        if with_keeper {
            fs::copy(STUB, install.join("agentspace-keeper.exe")).unwrap();
        }
        Fixture {
            _root: root,
            install,
            home,
            work,
        }
    }

    /// The variables the launcher is given: the process's own, with the four home variables
    /// pointed into the throwaway home, a marker, and no state folder.
    fn overrides(&self) -> Vec<(&'static str, OsString)> {
        let h = &self.home;
        vec![
            ("USERPROFILE", h.clone().into_os_string()),
            ("HOME", h.clone().into_os_string()),
            (
                "LOCALAPPDATA",
                h.join("AppData").join("Local").into_os_string(),
            ),
            (
                "APPDATA",
                h.join("AppData").join("Roaming").into_os_string(),
            ),
            (MARKER, OsString::from("a marker value with spaces")),
        ]
    }

    /// The launcher, ready to run: its own folder's copy, in `work`, detached from any console,
    /// with no standard streams.
    fn launcher(&self) -> Command {
        let mut command = Command::new(self.install.join("agentspace-keeper-task.exe"));
        command.current_dir(&self.work);
        command.env_remove(STATE_DIR);
        for (name, value) in self.overrides() {
            command.env(name, value);
        }
        command.creation_flags(DETACHED_PROCESS);
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        command
    }

    /// The environment the launcher is given, as a list.
    fn expected_env(&self, state_dir: Option<&Path>) -> Vec<(OsString, OsString)> {
        let mut changed: Vec<String> = self
            .overrides()
            .iter()
            .map(|(name, _)| name.to_uppercase())
            .collect();
        changed.push(STATE_DIR.to_string());
        let mut vars: Vec<(OsString, OsString)> = std::env::vars_os()
            .filter(|(name, _)| !changed.contains(&name.to_string_lossy().to_uppercase()))
            .collect();
        for (name, value) in self.overrides() {
            vars.push((OsString::from(name), value));
        }
        if let Some(dir) = state_dir {
            vars.push((OsString::from(STATE_DIR), dir.as_os_str().to_owned()));
        }
        vars
    }

    fn write_exit_code(&self, code: u32) {
        fs::write(self.install.join("stub-exit-code.txt"), code.to_string()).unwrap();
    }

    /// The stub's record in `folder`, as `key` to `value`.
    fn record_in(folder: &Path) -> Option<HashMap<String, String>> {
        let text = fs::read_to_string(folder.join("stub-record.txt")).ok()?;
        Some(
            text.lines()
                .filter_map(|l| l.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    fn record(&self) -> HashMap<String, String> {
        Fixture::record_in(&self.install).expect("The stub left no record.")
    }

    fn log_path(&self) -> PathBuf {
        self.home.join(".agentspace").join(LOG)
    }
}

/// The error log's lines, each split at its first space into its time and its message, after
/// checking the bytes: UTF-8, no byte order mark, every line ending in CR LF.
fn log_lines(path: &Path) -> Vec<(String, String)> {
    let bytes = fs::read(path).expect("The error log was not written.");
    assert!(
        !bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "The log has a BOM."
    );
    let text = String::from_utf8(bytes).expect("The log is not UTF-8.");
    assert!(text.ends_with("\r\n"), "The log does not end in CR LF.");
    let body = &text[..text.len() - 2];
    body.split("\r\n")
        .map(|line| {
            assert!(
                !line.contains('\n') && !line.contains('\r'),
                "A stray line ending."
            );
            let (time, message) = line.split_once(' ').expect("A line without a space.");
            (time.to_string(), message.to_string())
        })
        .collect()
}

/// Whether a time has the form `YYYY-MM-DDTHH:MM:SS.fffffffZ`.
fn is_log_time(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 28
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'.',
            27 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

fn os_args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

#[test]
fn it_is_a_windows_subsystem_program() {
    // The PE optional header's Subsystem field: 2 is the Windows GUI, with no console, and 3 is
    // the console. The offset, 68, is the same in PE32 and PE32+.
    let bytes = fs::read(LAUNCHER).unwrap();
    let pe = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0");
    let at = pe + 24 + 68;
    let subsystem = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    assert_eq!(
        subsystem, 2,
        "The launcher is not a Windows-subsystem program."
    );
    // And the stub is a console program, so a check that sees no window means something.
    let stub = fs::read(STUB).unwrap();
    let pe = u32::from_le_bytes(stub[0x3C..0x40].try_into().unwrap()) as usize;
    let at = pe + 24 + 68;
    assert_eq!(u16::from_le_bytes(stub[at..at + 2].try_into().unwrap()), 3);
}

#[test]
fn the_arguments_are_passed_on_unchanged() {
    let f = Fixture::new(true);
    let args = os_args(&[
        "-Watch",
        "two words",
        "--flag=\u{e9}x",
        "C:\\a\\b",
        "",
        "-x",
        " ",
        "tab\there",
        "a\\ b",
        "a\\\\b c",
    ]);
    let status = f.launcher().args(&args).status().unwrap();
    assert_eq!(status.code(), Some(0));
    assert_eq!(f.record()["args"], format!("{args:?}"));
}

#[test]
fn no_argument_at_all_is_passed_as_none() {
    let f = Fixture::new(true);
    assert_eq!(f.launcher().status().unwrap().code(), Some(0));
    assert_eq!(f.record()["args"], "[]");
}

#[test]
fn the_keepers_exit_code_is_passed_on() {
    for code in [0u32, 1, 2, 7, 255, 256, 0xC000_0005, u32::MAX] {
        let f = Fixture::new(true);
        f.write_exit_code(code);
        let status = f.launcher().arg("-Watch").status().unwrap();
        // Windows has the code as 32 bits, and the standard library reads it back as an i32.
        assert_eq!(
            status.code(),
            Some(code as i32),
            "Exit code {code} was not passed on."
        );
    }
}

#[test]
fn the_keepers_streams_are_not_redirected() {
    let f = Fixture::new(true);
    let out = f
        .launcher()
        .arg("-Watch")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    // The stub's lines reach the launcher's own pipes, so the launcher gave the keeper its own
    // streams as they were.
    assert!(String::from_utf8_lossy(&out.stdout).contains("stub stdout"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("stub stderr"));
}

#[test]
fn the_keeper_has_a_console_but_no_window() {
    // The stub is a console program with a console of its own. Without CREATE_NO_WINDOW in the
    // launcher, Windows gives it a new console with a window, and GetConsoleWindow is not null.
    let f = Fixture::new(true);
    assert_eq!(f.launcher().arg("-Watch").status().unwrap().code(), Some(0));
    assert_eq!(
        f.record()["console_window"],
        "null",
        "The keeper was given a console window."
    );
}

#[test]
fn the_keeper_is_found_in_the_launchers_own_folder_and_runs_in_its_working_folder() {
    let f = Fixture::new(true);
    // A decoy keeper in the working folder: a launcher that looked there would run it.
    fs::copy(STUB, f.work.join("agentspace-keeper.exe")).unwrap();
    assert_eq!(f.launcher().arg("-Watch").status().unwrap().code(), Some(0));
    assert!(
        Fixture::record_in(&f.work).is_none(),
        "The launcher ran the keeper from its working folder."
    );
    let cwd = PathBuf::from(&f.record()["cwd"]);
    assert_eq!(
        cwd.canonicalize().unwrap(),
        f.work.canonicalize().unwrap(),
        "The keeper did not run in the launcher's working folder."
    );
}

#[test]
fn a_keeper_missing_from_its_own_folder_is_never_looked_for_on_path() {
    let f = Fixture::new(false);
    // A decoy keeper in a folder put first on the launcher's PATH: a launcher that looked the
    // keeper up by its bare name would run it.
    let decoy = f.work.join("on-path");
    fs::create_dir_all(&decoy).unwrap();
    fs::copy(STUB, decoy.join("agentspace-keeper.exe")).unwrap();
    let mut path = decoy.clone().into_os_string();
    if let Some(rest) = std::env::var_os("PATH") {
        path.push(";");
        path.push(rest);
    }
    let status = f
        .launcher()
        .env("PATH", path)
        .arg("-Watch")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    assert!(
        Fixture::record_in(&decoy).is_none(),
        "The launcher ran a keeper it found on PATH."
    );
    assert_eq!(log_lines(&f.log_path()).len(), 1);
}

#[test]
fn the_environment_is_passed_on_unchanged() {
    for state in [None, Some("state-folder")] {
        let f = Fixture::new(true);
        let state_dir = state.map(|s| f.work.join(s));
        let mut command = f.launcher();
        if let Some(dir) = &state_dir {
            command.env(STATE_DIR, dir);
        }
        assert_eq!(command.arg("-Watch").status().unwrap().code(), Some(0));
        let record = f.record();
        let (count, hash) = envdigest::digest(f.expected_env(state_dir.as_deref()).into_iter());
        assert_eq!(
            record["env_count"],
            count.to_string(),
            "The count of variables differs."
        );
        assert_eq!(
            record["env_digest"],
            format!("{hash:016x}"),
            "A variable differs."
        );
        // And by name, so a failure says which one.
        let mut named = f.overrides();
        named.push((
            STATE_DIR,
            state_dir
                .clone()
                .map_or(OsString::from("<unset>"), PathBuf::into_os_string),
        ));
        for (name, want) in named {
            assert!(
                record[&format!("var.{name}")] == want.to_string_lossy(),
                "The variable {name} differs."
            );
        }
    }
}

#[test]
fn a_quote_is_refused_with_exit_2_and_the_keeper_is_not_started() {
    let f = Fixture::new(true);
    let status = f.launcher().args(["-Watch", "a\"b"]).status().unwrap();
    assert_eq!(status.code(), Some(2));
    assert!(
        Fixture::record_in(&f.install).is_none(),
        "The keeper was started."
    );
    let lines = log_lines(&f.log_path());
    assert_eq!(lines.len(), 1);
    assert!(is_log_time(&lines[0].0), "The time has the wrong form.");
    assert_eq!(
        lines[0].1,
        "Refused: argument 2 holds a quote or ends in a backslash; only plain arguments such as -Watch are passed on."
    );
}

#[test]
fn a_trailing_backslash_is_refused_with_exit_2_and_the_keeper_is_not_started() {
    let f = Fixture::new(true);
    let status = f.launcher().args(["C:\\dir\\"]).status().unwrap();
    assert_eq!(status.code(), Some(2));
    assert!(Fixture::record_in(&f.install).is_none());
    let lines = log_lines(&f.log_path());
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0].1,
        "Refused: argument 1 holds a quote or ends in a backslash; only plain arguments such as -Watch are passed on."
    );
}

#[test]
fn a_missing_keeper_exits_1_and_logs_the_errors_kind_and_message() {
    let f = Fixture::new(false);
    let before = format_log_time(SystemTime::now());
    let status = f.launcher().arg("-Watch").status().unwrap();
    let after = format_log_time(SystemTime::now());
    assert_eq!(status.code(), Some(1));
    // The log's folder is in the throwaway home, as the launcher's environment names it.
    let lines = log_lines(&f.log_path());
    assert_eq!(lines.len(), 1);
    let (time, message) = &lines[0];
    assert!(is_log_time(time), "The time has the wrong form.");
    assert!(
        before.as_str() <= time.as_str() && time.as_str() <= after.as_str(),
        "The time is not UTC now."
    );
    let keeper = f.install.join("agentspace-keeper.exe");
    assert_eq!(
        message.split(": ").next().unwrap(),
        format!("Could not start {}", keeper.display())
    );
    assert!(
        message.contains(": NotFound: "),
        "The kind is missing: {message}"
    );
    assert!(
        message.len() > keeper.display().to_string().len() + 30,
        "The message is missing."
    );
}

#[test]
fn a_second_failure_adds_a_line() {
    let f = Fixture::new(false);
    f.launcher().arg("-Watch").status().unwrap();
    f.launcher().args(["x\""]).status().unwrap();
    let lines = log_lines(&f.log_path());
    assert_eq!(lines.len(), 2);
    assert!(lines[0].1.starts_with("Could not start "));
    assert!(lines[1].1.starts_with("Refused: argument 1 "));
}

#[test]
fn the_state_folder_variable_moves_the_log() {
    let f = Fixture::new(false);
    let state = f.work.join("state");
    let status = f
        .launcher()
        .env(STATE_DIR, &state)
        .arg("-Watch")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    assert_eq!(log_lines(&state.join(LOG)).len(), 1);
    assert!(!f.log_path().exists(), "The log is in the home folder too.");
}

#[test]
fn a_log_that_cannot_be_written_changes_no_exit_code() {
    let f = Fixture::new(false);
    // The state folder would have to be made inside a file.
    let file = f.work.join("a-file");
    fs::write(&file, b"x").unwrap();
    let state = file.join("state");
    let missing = f
        .launcher()
        .env(STATE_DIR, &state)
        .arg("-Watch")
        .status()
        .unwrap();
    assert_eq!(missing.code(), Some(1));
    let refused = f
        .launcher()
        .env(STATE_DIR, &state)
        .arg("x\\")
        .status()
        .unwrap();
    assert_eq!(refused.code(), Some(2));
    assert!(!f.log_path().exists() && !state.exists());
}

#[test]
fn a_successful_run_writes_no_log() {
    let f = Fixture::new(true);
    f.write_exit_code(3);
    assert_eq!(f.launcher().arg("-Watch").status().unwrap().code(), Some(3));
    assert!(
        !f.log_path().exists(),
        "A keeper's exit code is not the launcher's failure."
    );
}

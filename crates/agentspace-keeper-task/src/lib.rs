//! The keeper task's launcher (docs/keeper.md, The task's launcher and The launcher's error log;
//! docs/state.md, The keeper task's launcher and The keeper's logs).
//!
//! The binary is thin. This library holds the rest, so it can be tested without a process:
//! `run` takes the arguments, the program's own path and the environment as values.
//!
//! The launcher runs `agentspace-keeper.exe` from its own folder with its own arguments, hidden
//! and in its own working folder, waits, and exits with the keeper's code. It writes only one
//! file, the error log, and only when it fails.

#![forbid(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The launcher's error log, in the state folder (docs/state.md, The keeper's logs).
pub const ERROR_LOG: &str = "board-holder-launcher-errors.log";

/// Windows's `CREATE_NO_WINDOW`: the keeper gets a console but no window for it.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The exit code for an argument the launcher refuses (keeper.md, The task's launcher).
pub const EXIT_REFUSED: i32 = 2;
/// The exit code when the keeper could not be started or waited for.
pub const EXIT_FAILED: i32 = 1;

/// The file name of the keeper next to the launcher (operator.md, How each program finds the
/// others): `agentspace-keeper.exe` on Windows.
pub fn keeper_file_name() -> String {
    format!("agentspace-keeper{}", std::env::consts::EXE_SUFFIX)
}

/// The two variables that say where the error log goes, as values, so a test passes its own.
/// It has no `Debug`, so no failing test can print the values it holds (AGENTS.md, "Terry's real
/// files in a review").
#[derive(Clone, Default)]
pub struct Env {
    /// `AGENTSPACE_STATE_DIR`.
    pub state_dir: Option<OsString>,
    /// `USERPROFILE`, the home folder (state.md, The home folder).
    pub userprofile: Option<OsString>,
}

impl Env {
    /// The two variables as this process has them.
    pub fn from_process() -> Env {
        Env {
            state_dir: std::env::var_os("AGENTSPACE_STATE_DIR"),
            userprofile: std::env::var_os("USERPROFILE"),
        }
    }

    /// The state folder, where the error log is: `AGENTSPACE_STATE_DIR` when it is set and not
    /// empty, else `.agentspace` in the `USERPROFILE` folder. `None` when neither names one.
    pub fn state_folder(&self) -> Option<PathBuf> {
        let set = |v: &Option<OsString>| v.as_ref().filter(|v| !v.is_empty()).cloned();
        if let Some(dir) = set(&self.state_dir) {
            return Some(PathBuf::from(dir));
        }
        set(&self.userprofile).map(|home| PathBuf::from(home).join(".agentspace"))
    }

    /// The error log's path.
    pub fn error_log(&self) -> Option<PathBuf> {
        self.state_folder().map(|dir| dir.join(ERROR_LOG))
    }
}

/// Whether an argument holds a `"` or ends in a `\`. Both are ASCII, so the bytes of the
/// operating system's string are enough.
fn is_refused(arg: &OsStr) -> bool {
    let bytes = arg.as_encoded_bytes();
    bytes.contains(&b'"') || bytes.last() == Some(&b'\\')
}

/// The log message for the first argument the launcher refuses, counting from 1, or `None` when
/// every argument is plain (keeper.md, The task's launcher).
pub fn refusal(args: &[OsString]) -> Option<String> {
    let n = args.iter().position(|a| is_refused(a))? + 1;
    Some(format!(
        "Refused: argument {n} holds a quote or ends in a backslash; only plain arguments such as -Watch are passed on."
    ))
}

/// A time in the error log's form: UTC, ISO 8601, seven fractional digits and a `Z`
/// (`2026-11-18T05:07:00.1234567Z`; keeper.md, The launcher's error log). A time before 1970 is
/// written as the epoch.
pub fn format_log_time(time: SystemTime) -> String {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let ticks = since.subsec_nanos() / 100;
    let (y, m, d) = civil_from_days(secs / 86_400);
    let rest = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ticks:07}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// The civil date of a day count from 1970-01-01 (proleptic Gregorian).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// One line of the error log: `<time> <message>` and CR LF (state.md, The keeper's logs).
pub fn log_line(time: SystemTime, message: &str) -> String {
    format!("{} {message}\r\n", format_log_time(time))
}

/// Appends one line to the error log. A failure at any step is ignored (keeper.md, The task's
/// launcher).
fn write_log(env: &Env, message: &str) {
    let Some(path) = env.error_log() else {
        return;
    };
    let _ = append(&path, &log_line(SystemTime::now(), message));
}

/// Appends a line, UTF-8 with no byte order mark, creating the folder when it is missing.
fn append(path: &Path, line: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())
}

/// An error's kind and message: `<Kind>: <message>`.
// The line is `Could not start <full keeper path>: <Kind>: <message>` (and `Could not wait for`
// in its place for a failed wait), where `<Kind>` is the name of Rust's `io::ErrorKind`, such as
// `NotFound`, and `<message>` is the operating system's text (keeper.md, The task's launcher;
// architect, 2026-11-19, for E0.06-T02-S07).
fn describe(e: &io::Error) -> String {
    format!("{:?}: {e}", e.kind())
}

/// Runs the launcher: refuses a bad argument (exit 2), else starts the keeper from the
/// launcher's own folder with the same arguments, hidden, waits, and returns its exit code.
/// A keeper that cannot be started or waited for gives exit 1. Every failure is logged to the
/// error log first. The working folder, the environment and the streams are the launcher's own,
/// untouched.
pub fn run(args: &[OsString], own_exe: io::Result<PathBuf>, env: &Env) -> i32 {
    if let Some(message) = refusal(args) {
        write_log(env, &message);
        return EXIT_REFUSED;
    }
    let name = keeper_file_name();
    // A launcher that cannot learn its own path logs one line saying so, and exits 1
    // (keeper.md, The task's launcher; architect, 2026-11-19, for E0.06-T02-S07).
    let exe = match own_exe {
        Ok(exe) => exe,
        Err(e) => {
            let why = describe(&e);
            write_log(
                env,
                &format!(
                    "Could not find this launcher's own path, so {name} was not started. {why}"
                ),
            );
            return EXIT_FAILED;
        }
    };
    let Some(dir) = exe.parent() else {
        write_log(
            env,
            &format!(
                "Could not find the folder this launcher runs from, so {name} was not started."
            ),
        );
        return EXIT_FAILED;
    };
    let keeper = dir.join(name);
    let mut command = Command::new(&keeper);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let why = describe(&e);
            write_log(env, &format!("Could not start {}: {why}", keeper.display()));
            return EXIT_FAILED;
        }
    };
    match child.wait() {
        // The exit code as the operating system has it. On Windows a code above i32::MAX is the
        // same 32 bits, so process::exit passes it on unchanged. A keeper ended by a signal
        // (not on Windows) has no code.
        Ok(status) => status.code().unwrap_or(EXIT_FAILED),
        Err(e) => {
            let why = describe(&e);
            write_log(
                env,
                &format!("Could not wait for {}: {why}", keeper.display()),
            );
            EXIT_FAILED
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    /// A launcher path in a folder that holds no keeper.
    fn exe_in(dir: &Path) -> io::Result<PathBuf> {
        Ok(dir.join("agentspace-keeper-task.exe"))
    }

    fn env_with_state(dir: PathBuf) -> Env {
        Env {
            state_dir: Some(dir.into_os_string()),
            userprofile: None,
        }
    }

    const REFUSED_1: &str = "Refused: argument 1 holds a quote or ends in a backslash; only plain arguments such as -Watch are passed on.";
    const REFUSED_2: &str = "Refused: argument 2 holds a quote or ends in a backslash; only plain arguments such as -Watch are passed on.";

    #[test]
    fn plain_arguments_are_not_refused() {
        assert_eq!(refusal(&os(&["-Watch"])), None);
        assert_eq!(refusal(&os(&[])), None);
        assert_eq!(refusal(&os(&["a b", "C:\\x\\y", "-Name=\u{e9}", ""])), None);
    }

    #[test]
    fn a_quote_is_refused_with_its_position() {
        assert_eq!(
            refusal(&os(&["-Watch", "a\"b"])).as_deref(),
            Some(REFUSED_2)
        );
        assert_eq!(
            refusal(&os(&["-Watch", "\"", "x\\"])).as_deref(),
            Some(REFUSED_2)
        );
    }

    #[test]
    fn a_trailing_backslash_is_refused_with_its_position() {
        assert_eq!(refusal(&os(&["C:\\dir\\"])).as_deref(), Some(REFUSED_1));
        assert_eq!(refusal(&os(&["\\"])).as_deref(), Some(REFUSED_1));
        assert_eq!(refusal(&os(&["\\", "-Watch"])).as_deref(), Some(REFUSED_1));
    }

    #[test]
    fn times_have_seven_fraction_digits_and_a_z() {
        let t = UNIX_EPOCH + Duration::new(1_794_978_420, 123_456_700);
        assert_eq!(format_log_time(t), "2026-11-18T05:07:00.1234567Z");
        assert_eq!(format_log_time(UNIX_EPOCH), "1970-01-01T00:00:00.0000000Z");
        // 100-nanosecond ticks: anything finer is cut, never rounded.
        let t = UNIX_EPOCH + Duration::new(0, 199);
        assert_eq!(format_log_time(t), "1970-01-01T00:00:00.0000001Z");
        // A leap day, and the last second of a day.
        let t = UNIX_EPOCH + Duration::new(951_868_799, 0);
        assert_eq!(format_log_time(t), "2000-02-29T23:59:59.0000000Z");
    }

    #[test]
    fn a_line_is_time_space_message_and_crlf() {
        let line = log_line(UNIX_EPOCH, "Hello.");
        assert_eq!(line, "1970-01-01T00:00:00.0000000Z Hello.\r\n");
    }

    #[test]
    fn the_state_folder_follows_state_dir_then_userprofile() {
        let log = |s: Option<&str>, u: Option<&str>| {
            Env {
                state_dir: s.map(OsString::from),
                userprofile: u.map(OsString::from),
            }
            .error_log()
        };
        let under_state = Some(PathBuf::from("S").join(ERROR_LOG));
        let under_home = Some(PathBuf::from("U").join(".agentspace").join(ERROR_LOG));
        assert_eq!(log(Some("S"), Some("U")), under_state);
        assert_eq!(log(None, Some("U")), under_home);
        // Empty means unset.
        assert_eq!(log(Some(""), Some("U")), under_home);
        assert_eq!(log(None, None), None);
        assert_eq!(log(Some(""), Some("")), None);
        // A state folder alone is enough.
        assert_eq!(log(Some("S"), None), under_state);
    }

    #[test]
    fn a_refusal_is_logged_and_exits_2_without_starting_anything() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_with_state(dir.path().join("state"));
        let code = run(&os(&["-Watch", "bad\\"]), exe_in(dir.path()), &env);
        assert_eq!(code, EXIT_REFUSED);
        let text = fs::read_to_string(env.error_log().unwrap()).unwrap();
        assert!(text.ends_with(&format!(" {REFUSED_2}\r\n")));
    }

    #[test]
    fn a_missing_keeper_is_logged_with_its_kind_and_exits_1() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_with_state(dir.path().join("state"));
        assert_eq!(run(&os(&["-Watch"]), exe_in(dir.path()), &env), EXIT_FAILED);
        let text = fs::read_to_string(env.error_log().unwrap()).unwrap();
        assert!(text.contains(" Could not start "), "{text}");
        assert!(text.contains("NotFound: "), "{text}");
        assert!(text.ends_with("\r\n"));
    }

    #[test]
    fn an_unwritable_log_changes_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        // The state folder's parent is a file, so no log can be made there.
        let file = dir.path().join("a-file");
        fs::write(&file, b"x").unwrap();
        let env = env_with_state(file.join("state"));
        assert_eq!(run(&os(&["x\""]), exe_in(dir.path()), &env), EXIT_REFUSED);
        assert_eq!(run(&os(&["-Watch"]), exe_in(dir.path()), &env), EXIT_FAILED);
        assert!(!file.join("state").exists());
    }

    #[test]
    fn no_place_for_a_log_means_no_log_and_the_same_exit() {
        let dir = tempfile::tempdir().unwrap();
        let code = run(&os(&["a\""]), exe_in(dir.path()), &Env::default());
        assert_eq!(code, EXIT_REFUSED);
    }

    #[test]
    fn an_unknown_own_path_exits_1_and_is_logged() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_with_state(dir.path().to_path_buf());
        let code = run(&os(&["-Watch"]), Err(io::Error::other("No path.")), &env);
        assert_eq!(code, EXIT_FAILED);
        let text = fs::read_to_string(env.error_log().unwrap()).unwrap();
        assert!(
            text.contains("Could not find this launcher's own path"),
            "{text}"
        );
    }
}

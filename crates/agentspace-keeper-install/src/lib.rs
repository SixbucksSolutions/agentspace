//! The keeper task's installer, `agentspace-keeper-install` (docs/keeper.md, The installer; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! # What is here
//!
//! - [`parse_args`] and [`Mode`]: the arguments and the usage errors.
//! - [`task_definition`], [`TaskDefinition`] and [`dry_run_lines`]: the task's definition as plain
//!   data, and the dry run's lines. Building them MUST NOT touch Task Scheduler or COM.
//! - [`flow`]: the steps of a real install, of `-Uninstall` and of `-DryRun`, which reach Task
//!   Scheduler only through [`flow::TaskStore`].
//! - [`keeper_lock`]: finding a running keeper by `board-holder-keeper.lock` under
//!   `%USERPROFILE%\.agentspace` alone (FB-27).
//! - `scheduler` (Windows only): the real [`flow::TaskStore`], through Task Scheduler's COM API.
//!   Only `main`, through [`run`], reaches it, and only for a real install and `-Uninstall`.
//!
//! # Other systems
//!
//! The crate builds off Windows, where [`run`] only reports that the installer runs on Windows.
//! The Windows calls sit behind `cfg(windows)`, and `unsafe` is confined to the two modules that
//! make them.

mod args;
mod clock;
mod definition;
pub mod flow;
pub mod keeper_lock;
#[cfg(windows)]
mod scheduler;
#[cfg(windows)]
mod sys;

pub use args::{Mode, PROGRAM_PREFIX, USAGE, parse_args, usage_error_lines};
pub use clock::{Now, Parts, format_utc, now_from_parts};
pub use definition::{
    ACTION_ARGUMENTS, KEEPER_EXE, LAUNCHER_EXE, TASK_NAME, TaskDefinition, TriggerInfo,
    TriggerKind, dry_run_lines, not_install_warning, same_folder, task_definition, trigger_lines,
};

use std::io::Write;

/// The message, after `agentspace-keeper-install: `, for `USERPROFILE` unset or empty (architect,
/// 2026-11-19, for E0.06-T02-S06). It ends the installer with exit 1 after the usage checks, in
/// every mode, `-DryRun` included.
pub const NO_PROFILE: &str = "USERPROFILE is not set, so there is no home folder.";

/// `%USERPROFILE%`'s value, from what the environment gives for it: `Err` holds [`NO_PROFILE`]
/// when it is unset or empty.
pub fn profile_from(value: Option<std::ffi::OsString>) -> Result<std::path::PathBuf, &'static str> {
    match value.filter(|value| !value.is_empty()) {
        Some(value) => Ok(std::path::PathBuf::from(value)),
        None => Err(NO_PROFILE),
    }
}

/// Runs the installer in `mode`, writing to `out` and `err`, and returns its exit code. A usage
/// error is the caller's: [`parse_args`] has already refused it.
///
/// `-DryRun` reads `USERPROFILE`, the current user's `DOMAIN\user` and the clock, and prints
/// [`dry_run_lines`]; it MUST NOT connect to Task Scheduler. A real install and `-Uninstall` are
/// the only modes that do, and a real install checks its folder and its programs first.
pub fn run(mode: Mode, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    #[cfg(windows)]
    {
        run_on_windows(mode, out, err)
    }
    #[cfg(not(windows))]
    {
        let _ = (mode, out);
        let _ = writeln!(
            err,
            "{PROGRAM_PREFIX}The installer runs on Windows only; it registers a Windows scheduled task."
        );
        1
    }
}

#[cfg(windows)]
fn run_on_windows(mode: Mode, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    use flow::{Host, dry_run, install, uninstall};

    let fail = |err: &mut dyn Write, message: &str| {
        let _ = writeln!(err, "{PROGRAM_PREFIX}{message}");
        1
    };
    // Before the mode is looked at, so it holds in every mode (architect, 2026-11-19).
    let profile = match profile_from(std::env::var_os("USERPROFILE")) {
        Ok(profile) => profile,
        Err(message) => return fail(err, message),
    };
    if mode == Mode::Uninstall {
        return uninstall(&profile, &scheduler::Scheduler, out, err);
    }
    let folder = match std::env::current_exe() {
        Ok(exe) => match exe.parent() {
            Some(folder) => folder.to_path_buf(),
            None => return fail(err, "The installer's own folder could not be found."),
        },
        Err(error) => {
            return fail(
                err,
                &format!("The installer's own folder could not be found: {error}"),
            );
        }
    };
    let user = match sys::current_user() {
        Ok(user) => user,
        Err(error) => return fail(err, &error),
    };
    let now = match sys::now() {
        Ok(now) => now,
        Err(error) => return fail(err, &error),
    };
    let host = Host {
        folder,
        profile,
        user,
        now,
    };
    match mode {
        Mode::DryRun => dry_run(&host, out),
        Mode::Install => install(&host, &scheduler::Scheduler, out, err),
        // Returned above, before anything but USERPROFILE was read.
        Mode::Uninstall => uninstall(&host.profile, &scheduler::Scheduler, out, err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_or_empty_profile_is_the_ruled_error() {
        assert_eq!(profile_from(None), Err(NO_PROFILE));
        assert_eq!(
            profile_from(Some(std::ffi::OsString::new())),
            Err(NO_PROFILE)
        );
        assert_eq!(
            NO_PROFILE,
            "USERPROFILE is not set, so there is no home folder."
        );
        let folder = std::env::temp_dir().join("keeper-install-profile");
        assert_eq!(
            profile_from(Some(folder.clone().into_os_string())),
            Ok(folder)
        );
    }

    /// The crate's own sources, every one of them, so a scan below covers all the code.
    const SOURCES: [(&str, &str); 9] = [
        ("args.rs", include_str!("args.rs")),
        ("clock.rs", include_str!("clock.rs")),
        ("definition.rs", include_str!("definition.rs")),
        ("flow.rs", include_str!("flow.rs")),
        ("keeper_lock.rs", include_str!("keeper_lock.rs")),
        ("lib.rs", include_str!("lib.rs")),
        ("main.rs", include_str!("main.rs")),
        ("scheduler.rs", include_str!("scheduler.rs")),
        ("sys.rs", include_str!("sys.rs")),
    ];

    fn count(needle: &str) -> usize {
        SOURCES
            .iter()
            .map(|(_, text)| text.matches(needle).count())
            .sum()
    }

    /// The needles are joined at run time, so this test's own text never matches them.
    fn joined(parts: &[&str]) -> String {
        parts.concat()
    }

    #[test]
    fn every_source_file_is_scanned() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let scanned: Vec<&str> = SOURCES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, scanned);
    }

    /// keeper.md, The installer, step 3: the installer MUST NOT read AGENTSPACE_STATE_DIR, and
    /// MUST NOT match a process's command line (FB-27). USERPROFILE is the one variable it reads.
    #[test]
    fn the_only_variable_read_is_userprofile_and_no_command_line_is_matched() {
        assert_eq!(count(&joined(&["\"AGENTSPACE", "_STATE_DIR\""])), 0);
        let reads = count(&joined(&["var", "_os("])) + count(&joined(&["env::", "var("]));
        assert_eq!(reads, 1, "Environment reads");
        assert_eq!(
            count(&joined(&["var", "_os(\"USERPROFILE\")"])),
            1,
            "The one read is USERPROFILE"
        );
        for needle in [
            joined(&["Command", "Line"]),
            joined(&["Toolhelp", "32"]),
            joined(&["Win32", "_Process"]),
            joined(&["Enum", "Processes"]),
            joined(&["QueryFullProcess", "ImageName"]),
            joined(&["sys", "info"]),
        ] {
            assert_eq!(count(&needle), 0, "{needle}");
        }
    }

    /// The scan can find something: each needle family matches text known to be there.
    #[test]
    fn the_scan_finds_what_it_looks_for() {
        assert_eq!(count(&joined(&["var", "_os(\"USERPROFILE\")"])), 1);
        assert!(count(&joined(&["AGENTSPACE", "_STATE_DIR"])) > 0);
        assert!(count(&joined(&["command", " line"])) > 0);
    }
}

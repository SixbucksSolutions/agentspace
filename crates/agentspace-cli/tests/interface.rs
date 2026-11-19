//! The interface that E0.05-T03-S02 (#0106) builds the commands on, as the card pins it: `Sub`,
//! `ParsedCommand`, `ErrorCode`, `LeaseError`, `commands::run` and `main_with`.
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. The card pins the names and
//! shapes and not their module paths, so these tests read them from the crate's root, and
//! `commands::run` from the `commands` module of the crate's root. `main_with` takes the arguments
//! after the program's own name (`lease create ...`), since the program's executable comes from
//! `current_exe` and never from the first argument (architect, 2026-11-19).

mod common;

use agentspace_cli::commands;
use agentspace_cli::{ErrorCode, LeaseError, ParsedCommand, Sub};
use common::*;
use std::ffi::OsString;
use std::path::Path;

fn name(sub: &Sub) -> &'static str {
    match sub {
        Sub::Create => "create",
        Sub::Get => "get",
        Sub::Renew => "renew",
        Sub::Release => "release",
        Sub::List => "list",
    }
}

fn code_name(code: &ErrorCode) -> &'static str {
    match code {
        ErrorCode::Internal => "internal",
        ErrorCode::Usage => "usage",
        ErrorCode::InvalidValue => "invalid_value",
        ErrorCode::OutOfRange => "out_of_range",
        ErrorCode::NotFound => "not_found",
        ErrorCode::InvalidState => "invalid_state",
    }
}

fn command_for(sub: Sub) -> ParsedCommand {
    let (flags, id): (Vec<(String, OsString)>, Option<OsString>) = match sub {
        Sub::Create => (vec![("project".to_string(), os("x"))], None),
        Sub::Get | Sub::Release => (vec![], Some(os(ID))),
        Sub::Renew => (vec![("ttl".to_string(), os("8h"))], Some(os(ID))),
        Sub::List => (
            vec![
                ("state".to_string(), os("ready")),
                ("state".to_string(), os("pending")),
            ],
            None,
        ),
    };
    ParsedCommand { sub, flags, id }
}

/// Runs `f` with an empty world (E0.05-T03-S02): no variable is set, the working folder is a
/// throwaway folder under the temp folder, the clock and the IDs are fixed, and there is no driver.
/// It never reads the process's environment, so it never reads a real home.
fn with_empty_world<T>(own_exe: &Path, f: impl FnOnce(&agentspace_cli::World) -> T) -> T {
    let cwd = scratch_dir();
    let var = |_: &str| None;
    let now = || 1_800_000_000_i64;
    let new_id = || "3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f".to_string();
    let world = agentspace_cli::World {
        var: &var,
        cwd: &cwd,
        own_exe,
        now: &now,
        new_id: &new_id,
        drivers: &[],
    };
    f(&world)
}

#[test]
fn run_answers_every_subcommand_with_internal_when_no_home_is_set() {
    // The own executable is a made-up path that holds a folder name no message may carry.
    let own_exe = Path::new("/nonexistent-folder-secret/agentspace");
    for sub in [Sub::Create, Sub::Get, Sub::Renew, Sub::Release, Sub::List] {
        let expected = format!(
            "agentspace lease {} failed: The home folder is not set (USERPROFILE is unset or empty), so agentspace cannot find its files. Nothing was changed.",
            name(&sub)
        );
        let result = with_empty_world(own_exe, |w| commands::run(command_for(sub), w));
        let Err(error) = result else {
            panic!("{expected}: run answered Ok, with no home folder");
        };
        assert_eq!(code_name(&error.code), "internal", "{expected}");
        assert_eq!(error.message, expected);
        assert_eq!(error.field, None, "{expected}");
        assert_eq!(error.lease_id, None, "{expected}");
        assert_eq!(error.state, None, "{expected}");
        assert!(
            !error.message.contains("secret"),
            "No path in {:?}",
            error.message
        );
    }
}

#[test]
fn run_never_panics_on_a_command_with_no_flags_or_an_odd_id() {
    let own_exe = Path::new("agentspace");
    for sub in [Sub::Create, Sub::Get, Sub::Renew, Sub::Release, Sub::List] {
        let label = name(&sub);
        let cmd = ParsedCommand {
            sub,
            flags: vec![],
            id: Some(os("")),
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_empty_world(own_exe, |w| commands::run(cmd, w).is_err())
        }));
        assert_eq!(result.ok(), Some(true), "{label}");
    }
}

#[test]
fn a_lease_error_carries_its_code_message_field_lease_id_and_state() {
    let error = LeaseError {
        code: ErrorCode::InvalidState,
        message: "A message.".to_string(),
        field: Some("ttl".to_string()),
        lease_id: Some(ID.to_string()),
        state: Some("ready".to_string()),
    };
    assert_eq!(code_name(&error.code), "invalid_state");
    assert_eq!(error.message, "A message.");
    assert_eq!(error.field.as_deref(), Some("ttl"));
    assert_eq!(error.lease_id.as_deref(), Some(ID));
    assert_eq!(error.state.as_deref(), Some("ready"));
}

#[test]
fn error_codes_are_the_six_of_the_table() {
    // An exhaustive match: a seventh code, or a missing one, stops this from compiling.
    let all = [
        ErrorCode::Internal,
        ErrorCode::Usage,
        ErrorCode::InvalidValue,
        ErrorCode::OutOfRange,
        ErrorCode::NotFound,
        ErrorCode::InvalidState,
    ];
    let names: Vec<&str> = all.iter().map(code_name).collect();
    assert_eq!(
        names,
        [
            "internal",
            "usage",
            "invalid_value",
            "out_of_range",
            "not_found",
            "invalid_state"
        ]
    );
}

#[test]
fn main_with_returns_the_exit_code_of_the_error_code_for_each_line() {
    let cases: [(&[&str], i32); 9] = [
        (&["lease", "create", "--project", "x"], 1),
        (&["lease", "list"], 0),
        (&["lease", "get", ID], 3),
        (&[], 2),
        (&["lease", "get"], 2),
        (&["frob"], 2),
        (&["--help"], 0),
        (&["lease", "create", "--help"], 0),
        (&["lease", "list", "--bogus", "--help"], 0),
    ];
    for (line, want) in cases {
        assert_eq!(in_process(&args(line)), want, "{line:?}");
    }
}

#[test]
fn main_with_ignores_the_own_executable_it_is_given_when_reading_the_line() {
    for exe in ["", "-h", "--bogus", "lease", "/nonexistent/agentspace"] {
        // An empty world has no home, so a list fails `internal`, whatever the own file is.
        let line = args(&["lease", "list"]);
        let out = with_empty_world(Path::new(exe), |w| agentspace_cli::execute_in(&line, w));
        assert_eq!(i32::from(out.exit_code), 1, "own_exe {exe:?}");
    }
}

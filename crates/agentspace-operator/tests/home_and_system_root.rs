//! The home and the system folder: after help, the usage errors and the `-Alias` checks, every
//! command fails when `USERPROFILE` is unset or empty, and then when `SystemRoot` is
//! (docs/operator.md, Command line, the architect's ruling of 2026-11-19 for E0.06-T01-S01).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;
use std::process::Command;

/// Ways a variable is missing.
#[derive(Clone, Copy, Debug)]
enum Gone {
    Unset,
    Empty,
}

const BOTH: [Gone; 2] = [Gone::Unset, Gone::Empty];

fn lose(c: &mut Command, name: &str, how: Gone) {
    match how {
        Gone::Unset => {
            c.env_remove(name);
        }
        Gone::Empty => {
            c.env(name, "");
        }
    }
}

/// Runs `args` with `USERPROFILE` and `SystemRoot` lost as given (None keeps the harness's).
fn run_without<S: AsRef<std::ffi::OsStr>>(
    args: &[S],
    profile: Option<Gone>,
    system_root: Option<Gone>,
) -> Run {
    run_with_env(args, |c| {
        if let Some(how) = profile {
            lose(c, "USERPROFILE", how);
        }
        if let Some(how) = system_root {
            lose(c, "SystemRoot", how);
        }
    })
}

/// Each way of losing one or both variables, with `how` applied to whichever are lost.
fn losses(how: Gone) -> [(Option<Gone>, Option<Gone>); 3] {
    [(Some(how), None), (None, Some(how)), (Some(how), Some(how))]
}

#[test]
fn every_command_fails_without_a_home() {
    for command in COMMANDS {
        for how in BOTH {
            let r = run_without(&[command], Some(how), None);
            assert_failure_run(
                &format!("{command}, USERPROFILE {how:?}"),
                &r,
                USERPROFILE_UNSET,
            );
        }
    }
}

#[test]
fn every_command_fails_without_a_system_folder() {
    for command in COMMANDS {
        for how in BOTH {
            let r = run_without(&[command], None, Some(how));
            assert_failure_run(
                &format!("{command}, SystemRoot {how:?}"),
                &r,
                SYSTEMROOT_UNSET,
            );
        }
    }
}

#[test]
fn the_home_is_checked_before_the_system_folder() {
    for command in COMMANDS {
        for profile in BOTH {
            for system_root in BOTH {
                let r = run_without(&[command], Some(profile), Some(system_root));
                assert_failure_run(
                    &format!("{command}, USERPROFILE {profile:?}, SystemRoot {system_root:?}"),
                    &r,
                    USERPROFILE_UNSET,
                );
            }
        }
    }
}

#[test]
fn a_home_and_a_system_folder_that_are_set_reach_the_command() {
    // The control: with both set, the same line reaches its stub.
    assert_reaches_command(&["status"], "status");
}

#[test]
fn the_check_applies_with_any_parameter_and_any_case() {
    for line in [
        "status -Quiet",
        "grow -GiB 4",
        "grow -GiB -3",
        "up -Alias devbox",
        "up -Alias devbox -Quiet",
        "DEPLOY",
    ] {
        let r = run_without(&words(line), Some(Gone::Unset), None);
        assert_failure_run(line, &r, USERPROFILE_UNSET);
        let r = run_without(&words(line), None, Some(Gone::Unset));
        assert_failure_run(line, &r, SYSTEMROOT_UNSET);
    }
}

#[test]
fn quiet_does_not_hide_either_failure() {
    for command in COMMANDS {
        let r = run_without(&[command, "-Quiet"], Some(Gone::Unset), None);
        assert_failure_run(&format!("{command} -Quiet"), &r, USERPROFILE_UNSET);
        let r = run_without(&["-Quiet", command], None, Some(Gone::Empty));
        assert_failure_run(&format!("-Quiet {command}"), &r, SYSTEMROOT_UNSET);
    }
}

#[test]
fn help_needs_neither() {
    for how in BOTH {
        for (profile, system_root) in losses(how) {
            let shown = format!("{profile:?} {system_root:?}");
            let r = run_without(&["-h"], profile, system_root);
            assert_eq!(r.code, Some(0), "-h, {shown}");
            assert_eq!(r.out(), help_text("top"), "-h, {shown}");
            assert_eq!(r.err(), "", "-h, {shown}");
            for command in COMMANDS {
                let r = run_without(&[command, "--help"], profile, system_root);
                assert_eq!(r.code, Some(0), "{command} --help, {shown}");
                assert_eq!(r.out(), help_text(command), "{command} --help, {shown}");
                assert_eq!(r.err(), "", "{command} --help, {shown}");
            }
        }
    }
}

#[test]
fn usage_errors_need_neither() {
    let cases = [
        ("", no_command()),
        ("bogus", unknown_command("bogus")),
        ("up status", unexpected("status")),
        ("status -x", unknown_parameter("-x")),
        ("status -Quiet -Quiet", given_twice("-Quiet")),
        ("status -GiB", needs_value("-GiB")),
        ("status -GiB abc", not_a_number("abc")),
    ];
    for (line, message) in cases {
        for how in BOTH {
            for (profile, system_root) in losses(how) {
                let r = run_without(&words(line), profile, system_root);
                assert_failure_run(
                    &format!("{line:?}, {profile:?} {system_root:?}"),
                    &r,
                    &message,
                );
            }
        }
    }
}

#[test]
fn the_alias_checks_run_before_both() {
    let cases = [
        ("status -Alias devbox", ALIAS_APPLIES),
        ("grow -Alias bad!", ALIAS_APPLIES),
        ("up -Alias bad!", ALIAS_INVALID),
        ("up -Alias -Fx", ALIAS_INVALID),
    ];
    for (line, message) in cases {
        for how in BOTH {
            for (profile, system_root) in losses(how) {
                let r = run_without(&words(line), profile, system_root);
                assert_failure_run(
                    &format!("{line:?}, {profile:?} {system_root:?}"),
                    &r,
                    message,
                );
            }
        }
    }
    // An empty alias is a refusal too, and still comes first.
    let r = run_without(&["up", "-Alias", ""], Some(Gone::Unset), Some(Gone::Unset));
    assert_failure_run("up -Alias ''", &r, ALIAS_INVALID);
}

#[test]
fn the_order_is_usage_errors_then_alias_then_the_home() {
    // Everything is wrong in each line below; the first check in the order speaks.
    let r = run_without(
        &["status", "-GiB", "abc", "-Alias", "bad!"],
        Some(Gone::Unset),
        Some(Gone::Unset),
    );
    assert_failure_run("usage error first", &r, &not_a_number("abc"));
    let r = run_without(
        &["status", "-Alias", "bad!"],
        Some(Gone::Unset),
        Some(Gone::Unset),
    );
    assert_failure_run("-Alias second", &r, ALIAS_APPLIES);
}

#[test]
fn the_failure_is_one_agentspace_line_and_nothing_else_is_written() {
    let r = run_without(&["status"], Some(Gone::Unset), Some(Gone::Unset));
    assert_eq!(r.out(), "");
    assert_eq!(r.err().lines().count(), 1);
    assert!(r.err().starts_with("agentspace: "));
    assert!(!r.err().contains('\r'));
    assert!(r.leftovers.is_empty(), "{:?}", r.leftovers);
}

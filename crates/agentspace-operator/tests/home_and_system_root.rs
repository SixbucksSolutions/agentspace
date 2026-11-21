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
            let r = run_without(&command_args(command), Some(how), None);
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
            let r = run_without(&command_args(command), None, Some(how));
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
                let r = run_without(&command_args(command), Some(profile), Some(system_root));
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
        let mut after = command_args(command);
        after.push("-Quiet");
        let r = run_without(&after, Some(Gone::Unset), None);
        assert_failure_run(&format!("{command} -Quiet"), &r, USERPROFILE_UNSET);
        let mut before = vec!["-Quiet"];
        before.extend(command_args(command));
        let r = run_without(&before, None, Some(Gone::Empty));
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
        ("status -Host bad!", not_a_project("bad!")),
        ("deploy -InstanceType bad", not_an_instance_type("bad")),
        ("status -Host", needs_value("-Host")),
        ("status -All -All", given_twice("-All")),
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
fn the_host_checks_need_neither_and_run_before_the_home() {
    // The `-Alias` checks and the host checks are part of the command line (operator.md, Command
    // line): they run before the home and the system folder, and read no file.
    let cases = [
        ("ca -Host y", HOST_NOT_FOR_CA),
        ("up -All", ALL_APPLIES),
        ("status -Host y -All", HOST_AND_ALL),
        ("provision", PROVISION_NEEDS_HOST),
        ("grow -InstanceType m9g.xlarge", INSTANCE_TYPE_APPLIES),
        ("deploy -InstanceType m9g.xlarge", INSTANCE_TYPE_NEEDS_HOST),
        (
            "deploy -Host swimcoach -InstanceType m9g.xlarge",
            INSTANCE_TYPE_NEEDS_HOST,
        ),
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
}

#[test]
fn a_command_that_names_a_host_still_needs_the_home_before_the_registry_is_looked_up() {
    // The host is looked up after the home and the system folder (operator.md, Command line), so
    // a missing home is reported, never "has no host".
    for how in BOTH {
        let r = run_without(&["status", "-Host", "agentspace"], Some(how), None);
        assert_failure_run("status -Host, home", &r, USERPROFILE_UNSET);
        let r = run_without(&["status", "-All"], None, Some(how));
        assert_failure_run("status -All, system root", &r, SYSTEMROOT_UNSET);
        let r = run_without(&["provision", "-Host", "agentspace"], Some(how), None);
        assert_failure_run("provision -Host, home", &r, USERPROFILE_UNSET);
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

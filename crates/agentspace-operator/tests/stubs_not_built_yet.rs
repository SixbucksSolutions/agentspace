//! TEMPORARY: the stubs of v0.06-T01-S01. Each command is dispatched to a stub that fails with
//! `agentspace: <command> is not built yet.` and exit 1. E0.06-T03 replaces the stubs, and DROPS
//! THIS FILE and `assert_reaches_command` in tests/common/cli.rs's stub line with it.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

#[test]
fn each_command_reaches_a_stub_that_fails_with_its_own_name() {
    for command in COMMANDS {
        let r = run(&[command]);
        assert_eq!(r.code, Some(1), "{command}");
        assert_eq!(r.out(), "", "{command}");
        assert_eq!(
            r.err(),
            format!("agentspace: {command} is not built yet.\n"),
            "{command}"
        );
    }
}

#[test]
fn the_stub_names_the_command_not_the_spelling_given() {
    // The dev lead's reading, confirmed by the architect, 2026-11-19: the stub names the command
    // in operator.md's lowercase spelling, whatever case was typed.
    assert_failure(&["STATUS"], "agentspace: status is not built yet.");
    assert_failure(&["Up"], "agentspace: up is not built yet.");
}

#[test]
fn the_stub_is_reached_with_every_parameter_the_command_takes() {
    assert_failure(
        &["deploy", "-Quiet"],
        "agentspace: deploy is not built yet.",
    );
    assert_failure(
        &["up", "-Alias", "devbox", "-Quiet"],
        "agentspace: up is not built yet.",
    );
    assert_failure(
        &["grow", "-GiB", "5", "-Quiet"],
        "agentspace: grow is not built yet.",
    );
    assert_failure(&["-Quiet", "sync"], "agentspace: sync is not built yet.");
}

#[test]
fn a_stub_failure_still_prints_under_quiet() {
    // Failures always print (Messages).
    assert_failure(
        &["-Quiet", "status"],
        "agentspace: status is not built yet.",
    );
}

#[test]
fn a_stub_starts_nothing_and_writes_nothing() {
    for command in COMMANDS {
        let r = run(&[command]);
        assert!(r.leftovers.is_empty(), "{command}: {:?}", r.leftovers);
        assert!(
            !r.err().contains("cannot be run"),
            "{command}: {:?}",
            r.err()
        );
    }
}

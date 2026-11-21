//! `deploy`, `sync`, `bootstrap` and `provision` are dispatched to agentspace-sync (D42, D55), whose
//! functions are built. In a throwaway home each stops at the earliest step the home cannot meet,
//! with that step's own message, starts no program and writes nothing (tests/common/cli.rs,
//! `assert_reaches_sync_crate`): `deploy` at the missing `id_ed25519.pub`, `sync` at its CA check,
//! `bootstrap` at its start-and-wait's first AWS call, and `provision` for an unlisted project with
//! no record at its step 1. What each does with a host's target is in `sync_crate_dispatch.rs`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

/// The commands agentspace-sync runs.
const SYNC_CRATE: [&str; 4] = ["deploy", "sync", "bootstrap", "provision"];

#[test]
fn each_sync_crate_command_stops_at_its_first_step_with_its_own_message() {
    for command in SYNC_CRATE {
        assert_reaches_sync_crate(&command_args(command), command);
    }
}

#[test]
fn the_command_is_reached_whatever_case_it_is_typed_in() {
    assert_reaches_sync_crate(&["SYNC"], "sync");
    assert_reaches_sync_crate(&["Deploy"], "deploy");
    assert_reaches_sync_crate(&["BOOTSTRAP"], "bootstrap");
    assert_reaches_sync_crate(&["Provision", "-Host", "x"], "provision");
}

#[test]
fn the_command_is_reached_with_every_parameter_it_takes() {
    assert_reaches_sync_crate(&["deploy", "-Quiet"], "deploy");
    assert_reaches_sync_crate(&["-Quiet", "sync"], "sync");
    assert_reaches_sync_crate(&["sync", "-GiB", "5"], "sync");
    assert_reaches_sync_crate(&["bootstrap", "-Quiet"], "bootstrap");
    assert_reaches_sync_crate(&["provision", "-Host", "x", "-Quiet"], "provision");
    assert_reaches_sync_crate(&["-quiet", "provision", "-host", " X "], "provision");
}

#[test]
fn the_failure_still_prints_under_quiet() {
    // Failures always print (Messages).
    assert_reaches_sync_crate(&["-Quiet", "deploy"], "deploy");
    assert_reaches_sync_crate(&["sync", "-quiet"], "sync");
    assert_reaches_sync_crate(&["bootstrap", "-quiet"], "bootstrap");
    assert_reaches_sync_crate(&["-Quiet", "provision", "-Host", "x"], "provision");
}

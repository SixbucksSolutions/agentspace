//! `deploy`, `sync`, `bootstrap` and `provision` are dispatched to agentspace-sync (D42, D55). Its
//! functions are the pinned stubs until the stories that build them merge: `bootstrap` and
//! `provision`, and `-Host` of `sync` and `deploy`, fail with `agentspace: <command> is not built
//! yet.`; the box's `sync` and `deploy` are today's. So every check here holds for both: in a
//! throwaway home each command fails with exit 1, one `agentspace: ` line on stderr, nothing on
//! stdout, and no program of the seam started (tests/common/cli.rs, `assert_reaches_sync_crate`).
//! The exact stub messages, which say which function and which target each line reached, are in
//! `sync_crate_dispatch.rs`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

/// The commands the sync crate runs.
const SYNC_CRATE: [&str; 4] = ["deploy", "sync", "bootstrap", "provision"];

#[test]
fn each_sync_crate_command_fails_there_with_one_line_and_starts_no_program() {
    for command in SYNC_CRATE {
        assert_reaches_sync_crate(&command_args(command));
    }
}

#[test]
fn the_command_is_reached_whatever_case_it_is_typed_in() {
    assert_reaches_sync_crate(&["SYNC"]);
    assert_reaches_sync_crate(&["Deploy"]);
    assert_reaches_sync_crate(&["BOOTSTRAP"]);
    assert_reaches_sync_crate(&["Provision", "-Host", "x"]);
}

#[test]
fn the_command_is_reached_with_every_parameter_it_takes() {
    assert_reaches_sync_crate(&["deploy", "-Quiet"]);
    assert_reaches_sync_crate(&["-Quiet", "sync"]);
    assert_reaches_sync_crate(&["sync", "-GiB", "5"]);
    assert_reaches_sync_crate(&["bootstrap", "-Quiet"]);
    assert_reaches_sync_crate(&["provision", "-Host", "x", "-Quiet"]);
    assert_reaches_sync_crate(&["-quiet", "provision", "-host", "x"]);
}

#[test]
fn the_failure_still_prints_under_quiet() {
    // Failures always print (Messages).
    assert_reaches_sync_crate(&["-Quiet", "deploy"]);
    assert_reaches_sync_crate(&["sync", "-quiet"]);
    assert_reaches_sync_crate(&["bootstrap", "-quiet"]);
    assert_reaches_sync_crate(&["-Quiet", "provision", "-Host", "x"]);
}

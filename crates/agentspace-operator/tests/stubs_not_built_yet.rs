//! `deploy` and `sync` are dispatched to agentspace-sync (D42). Its two functions are the pinned
//! stubs, which fail with `agentspace: <command> is not built yet.`, until E0.06-T03-S02 builds
//! them, and that story merges beside this one. So every check here holds for both: in a throwaway
//! home each command fails with exit 1, one `agentspace: ` line on stderr, nothing on stdout, and
//! no program of the seam started (tests/common/cli.rs, `assert_reaches_sync_crate`). Which
//! function each command reaches is the in-process dispatch test's (src/commands/tests.rs).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

/// The commands the sync crate runs.
const SYNC_CRATE: [&str; 2] = ["deploy", "sync"];

#[test]
fn each_sync_crate_command_fails_there_with_one_line_and_starts_no_program() {
    for command in SYNC_CRATE {
        assert_reaches_sync_crate(&[command]);
    }
}

#[test]
fn the_command_is_reached_whatever_case_it_is_typed_in() {
    assert_reaches_sync_crate(&["SYNC"]);
    assert_reaches_sync_crate(&["Deploy"]);
}

#[test]
fn the_command_is_reached_with_every_parameter_it_takes() {
    assert_reaches_sync_crate(&["deploy", "-Quiet"]);
    assert_reaches_sync_crate(&["-Quiet", "sync"]);
    assert_reaches_sync_crate(&["sync", "-GiB", "5"]);
}

#[test]
fn the_failure_still_prints_under_quiet() {
    // Failures always print (Messages).
    assert_reaches_sync_crate(&["-Quiet", "deploy"]);
    assert_reaches_sync_crate(&["sync", "-quiet"]);
}

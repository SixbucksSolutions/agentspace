//! What a holder started through WMI is (docs/keeper.md, Starting a holder, items 2 to 4, and
//! Streams and environment).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **A stand-in holder reports what it finds about itself** into the log, through the handle the
//! launcher gave it, and the spec reads the report: its working folder, what its stdin is, whether
//! its stdout and stderr are one handle, whether it has a console window, whether it was started in
//! a new process group, and which variables it sees. Each check has a control, a process started
//! another way, that shows the report can tell the difference.
#![cfg(windows)]

mod common;

use std::process::Stdio;
use std::time::Duration;

use agentspace_keeper_api::Started;
use common::{
    Fixture, control_report, holder_of, pass_userprofile, read_text, report, same_path, start_stub,
    wait_for,
};

const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

fn started_report(fx: &Fixture) -> std::collections::HashMap<String, String> {
    std::fs::write(
        fx.profile.join("spec-expect.txt"),
        pass_userprofile().to_string_lossy().as_bytes(),
    )
    .expect("The expectation file can be made.");
    let holder = holder_of(start_stub(fx, "stub_report"));
    let log = fx.log();
    let text = wait_for(Duration::from_secs(20), || {
        let t = read_text(&log);
        t.contains("ctrl_c_ignored=").then_some(t)
    })
    .expect("The holder reported.");
    let r = report(&text);
    assert!(
        text.contains(&format!("stub-start pid={}", holder.pid)),
        "The report is the holder's own."
    );
    r
}

#[test]
fn the_report_tells_a_process_group_a_stdin_and_a_window_apart() {
    // The controls: the checks below are of no use if they cannot see a difference.
    let fx = Fixture::new("controls");
    let plain = control_report(&fx, 0, Stdio::null(), "");
    let grouped = control_report(&fx, CREATE_NEW_PROCESS_GROUP, Stdio::null(), "");
    let piped = control_report(&fx, 0, Stdio::piped(), "");
    assert_eq!(
        plain.get("stdin").map(String::as_str),
        Some("eof"),
        "A stdin that is NUL reads as the end of a file."
    );
    assert_eq!(
        piped.get("stdin").map(String::as_str),
        Some("pipe"),
        "A stdin that is a pipe is told from NUL."
    );
    assert_eq!(
        plain.get("stdout_is_stderr").map(String::as_str),
        Some("false"),
        "Two separate handles are told from one."
    );
    if cfg!(target_pointer_width = "64") {
        assert_eq!(
            plain.get("ctrl_c_ignored").map(String::as_str),
            Some("false"),
            "A process in its parent's group does not have Ctrl+C ignored."
        );
        assert_eq!(
            grouped.get("ctrl_c_ignored").map(String::as_str),
            Some("true"),
            "A process started in a new process group has Ctrl+C ignored, and the report sees it."
        );
    }
}

#[test]
fn a_holder_runs_in_the_home_folder_with_nul_for_stdin_and_one_handle_for_both_outputs() {
    let fx = Fixture::new("process");
    let r = started_report(&fx);

    assert!(
        r.get("cwd").is_some_and(|c| same_path(c, &fx.profile)),
        "Item 4: the holder's working folder is the home folder the pass gave."
    );
    assert_eq!(
        r.get("stdin").map(String::as_str),
        Some("eof"),
        "Item 2: stdin is NUL."
    );
    assert_eq!(
        r.get("stdout_is_stderr").map(String::as_str),
        Some("true"),
        "Item 2: stdout and stderr are one handle."
    );
    if cfg!(target_pointer_width = "64") {
        assert_eq!(
            r.get("ctrl_c_ignored").map(String::as_str),
            Some("true"),
            "Item 4: the holder starts in a new process group."
        );
    }
    assert_ne!(
        r.get("console_window").map(String::as_str),
        Some("present"),
        "Item 4: the holder has no window."
    );
}

#[test]
fn the_holder_gets_the_logon_environment_and_never_the_keepers() {
    // The spec process has, as the keeper does, a changed USERPROFILE, an AGENTSPACE_STATE_DIR and
    // a variable of the spec's own (common::init). None may reach the holder.
    let fx = Fixture::new("env");
    let r = started_report(&fx);
    assert_eq!(
        r.get("probe_var").map(String::as_str),
        Some("absent"),
        "A variable set only in the keeper's environment does not reach the holder."
    );
    assert_eq!(
        r.get("state_dir_var").map(String::as_str),
        Some("absent"),
        "AGENTSPACE_STATE_DIR does not reach the holder."
    );
    assert_eq!(
        r.get("userprofile_is_the_passes").map(String::as_str),
        Some("false"),
        "The holder sees the logon environment's USERPROFILE, not the pass's changed one."
    );
}

#[test]
fn a_holder_that_exits_at_once_fails_the_start_with_the_log_pointer() {
    // Item 7 and step 7: a holder that is not running, or not the one the result names, fails the
    // start. A matcher that finds the command line not to be the holder's stands for the holder
    // that is not the one the result names.
    let fx = Fixture::new("exits");
    let exe = common::stub_exe();
    let matcher = common::FakeMatcher {
        accept: common::Accept::Never,
        ..common::FakeMatcher::stub("stub_sleep")
    };
    let out = common::start_with(
        &fx,
        &common::testhost(),
        &exe,
        &matcher,
        common::LEASE,
        common::ALIAS,
    );
    match out {
        Started::Failed(why) => assert_eq!(
            why,
            "The holder could not be started: The holder exited at once; see board-holder-ssh.log.",
            "Step 7 gives its reason as keeper.md has it."
        ),
        Started::Holder(_) => panic!("A holder that is not the holder command was recorded."),
    }
    assert!(
        fx.launch_files().is_empty(),
        "The three launch files were removed whatever happened."
    );
}

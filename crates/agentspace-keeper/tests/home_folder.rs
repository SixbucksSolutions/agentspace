//! USERPROFILE and the state folder (docs/keeper.md, Streams and environment; gaps FA-18, FB-19).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **When USERPROFILE is unset
//! or empty, the keeper MUST end before its first pass, whether or not `AGENTSPACE_STATE_DIR` is
//! set: exit 1, nothing logged, and the one line `agentspace-keeper: USERPROFILE is not set, so
//! there is no home folder.` on stderr, after the usage checks.** The state folder is
//! `AGENTSPACE_STATE_DIR` when set and not empty, else `%USERPROFILE%\.agentspace`, and the keeper
//! creates it if it is missing.

mod common;

use common::*;

const NO_HOME: &str = "agentspace-keeper: USERPROFILE is not set, so there is no home folder.\n";

fn assert_ended_before_the_first_pass(b: &Bin, args: &[&str]) {
    let r = b.run(args);
    assert_eq!(r.status, Some(1), "Exit code for {args:?}");
    assert_eq!(
        r.stdout, "",
        "Nothing is logged, so nothing reaches stdout ({args:?})"
    );
    assert_eq!(r.stderr, NO_HOME, "Stderr for {args:?}");
    assert!(!b.log_exists(), "Nothing is logged ({args:?})");
    assert!(
        b.state_is_empty(),
        "No file is made, a lock included ({args:?})"
    );
    assert!(
        std::fs::read_dir(&b.home).unwrap().next().is_none(),
        "The home folder is untouched ({args:?})"
    );
}

#[test]
fn an_unset_userprofile_ends_the_run_before_its_first_pass() {
    let mut b = Bin::new();
    b.unset("USERPROFILE");
    let ssh = b.ssh.to_string_lossy().into_owned();
    assert_ended_before_the_first_pass(&b, &["-SshExe", &ssh]);
    assert_ended_before_the_first_pass(&b, &["-SshExe", &ssh, "-Now", NOW]);
}

#[test]
fn an_empty_userprofile_ends_the_run_before_its_first_pass() {
    let mut b = Bin::new();
    b.set("USERPROFILE", "");
    let ssh = b.ssh.to_string_lossy().into_owned();
    assert_ended_before_the_first_pass(&b, &["-SshExe", &ssh]);
    assert_ended_before_the_first_pass(&b, &["-SshExe", &ssh, "-Now", NOW]);
}

#[test]
fn it_ends_whether_or_not_the_state_folder_is_given() {
    for state_dir in [Some("set"), None, Some("")] {
        for unset in [true, false] {
            let mut b = Bin::new();
            if unset {
                b.unset("USERPROFILE");
            } else {
                b.set("USERPROFILE", "");
            }
            match state_dir {
                Some("set") => {}
                Some(other) => {
                    b.set("AGENTSPACE_STATE_DIR", other);
                }
                None => {
                    b.unset("AGENTSPACE_STATE_DIR");
                }
            }
            let ssh = b.ssh.to_string_lossy().into_owned();
            assert_ended_before_the_first_pass(&b, &["-SshExe", &ssh, "-Now", NOW]);
        }
    }
}

#[test]
fn it_ends_a_watch_before_its_first_pass_too() {
    let mut b = Bin::new();
    b.set("USERPROFILE", "");
    let ssh = b.ssh.to_string_lossy().into_owned();
    assert_ended_before_the_first_pass(&b, &["-Watch", "-MaxPasses", "1", "-SshExe", &ssh]);
}

#[test]
fn the_lines_name_the_program_whatever_its_path_and_end_in_a_line_feed() {
    let mut b = Bin::new();
    b.unset("USERPROFILE");
    let r = b.run(&[]);
    // With no -SshExe the default comes from SystemRoot, which the throwaway home sets to a full
    // path, so the home folder is the check that fails.
    assert_eq!(r.status, Some(1));
    assert_eq!(r.stderr.lines().count(), 1);
    assert!(r.stderr.starts_with("agentspace-keeper: "));
    assert!(r.stderr.ends_with(".\n"));
}

// ---- The library: the same, with no trait called ----

#[test]
fn in_process_an_unset_or_empty_userprofile_calls_no_trait_and_logs_nothing() {
    for value in [None, Some("")] {
        for state_set in [true, false] {
            let mut h = Harness::new();
            match value {
                None => {
                    h.vars.remove("USERPROFILE");
                }
                Some(v) => {
                    h.vars.insert("USERPROFILE".into(), v.into());
                }
            }
            if !state_set {
                h.vars.remove("AGENTSPACE_STATE_DIR");
            }
            let r = h.pass(NOW);
            assert_eq!(r.exit, 1);
            assert_eq!(r.stderr, NO_HOME);
            assert_eq!(r.stdout, "");
            assert!(
                h.events().is_empty(),
                "A trait was called: {:?}",
                h.events()
            );
            assert!(!h.log_path().exists());
        }
    }
}

// ---- The state folder ----

#[test]
fn the_state_folder_is_the_variable_when_set_and_not_empty() {
    let h = Harness::new();
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_eq!(
        h.fakes.locks.pass_dirs.borrow().as_slice(),
        std::slice::from_ref(&h.state)
    );
    assert_eq!(h.log(), idle_line(NOW));
    assert!(!h.home.join(".agentspace").exists());
}

#[test]
fn with_the_variable_unset_or_empty_the_state_folder_is_dot_agentspace_under_the_profile() {
    for empty in [false, true] {
        let mut h = Harness::new();
        if empty {
            h.vars.insert("AGENTSPACE_STATE_DIR".into(), String::new());
        } else {
            h.vars.remove("AGENTSPACE_STATE_DIR");
        }
        let expected = h.home.join(".agentspace");
        let r = h.pass(NOW);
        assert_eq!(r.exit, 0, "stderr was {:?}", r.stderr);
        assert_eq!(
            h.fakes.locks.pass_dirs.borrow().as_slice(),
            std::slice::from_ref(&expected)
        );
        assert_eq!(
            std::fs::read_to_string(expected.join("board-holder.log")).unwrap(),
            idle_line(NOW)
        );
        assert!(!h.state.join("board-holder.log").exists());
    }
}

#[test]
fn the_keeper_creates_the_state_folder_when_it_is_missing() {
    let mut h = Harness::new();
    let nested = h.tmp.path().join("a").join("b").join("state");
    h.vars.insert(
        "AGENTSPACE_STATE_DIR".into(),
        nested.to_string_lossy().into_owned(),
    );
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0, "stderr was {:?}", r.stderr);
    assert_eq!(
        std::fs::read_to_string(nested.join("board-holder.log")).unwrap(),
        idle_line(NOW)
    );
}

#[test]
fn a_state_folder_that_cannot_be_made_ends_the_run_with_exit_1_and_nothing_logged() {
    // An error before the first pass ends any run with exit 1 and nothing logged. A file where the
    // state folder belongs is one.
    let mut h = Harness::new();
    let file = h.tmp.path().join("a-file");
    std::fs::write(&file, b"x").unwrap();
    h.vars.insert(
        "AGENTSPACE_STATE_DIR".into(),
        file.join("state").to_string_lossy().into_owned(),
    );
    let r = h.pass(NOW);
    assert_eq!(r.exit, 1);
    assert_eq!(r.stdout, "");
    // The line is `agentspace-keeper: The state folder '<path>' could not be created: <error>`
    // (keeper.md, Exit codes; architect, 2026-11-19, for E0.06-T01-S06). The `<error>` is the
    // system's, so only what comes before it is compared.
    let start = format!(
        "agentspace-keeper: The state folder '{}' could not be created: ",
        file.join("state").display()
    );
    assert!(
        r.stderr.starts_with(&start),
        "Stderr starts with {start:?}: {:?}",
        r.stderr
    );
    assert!(r.stderr.len() > start.len() + 1 && r.stderr.ends_with('\n'));
    assert_eq!(r.stderr.lines().count(), 1);
    assert!(h.fakes.starter.requests.borrow().is_empty());
    assert!(h.events().is_empty(), "No pass began: {:?}", h.events());
}

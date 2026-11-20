//! The launch handshake's three files (docs/keeper.md, The handshake; docs/state.md, A holder's
//! launch files).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **A watcher on the state folder sees each file appear and disappear in the order the system
//! reports it**, and reads each as it appears. The start is the real one: the real starter, WMI,
//! and the real launcher.
#![cfg(windows)]

mod common;

use agentspace_keeper_api::{Matcher, Started};
use agentspace_keeper_files::{
    Claim, LaunchResult, parse_claim, parse_launch_request, parse_launch_result,
    serialize_launch_result, split_command_line,
};
use common::{
    ALIAS, Accept, FakeMatcher, Fixture, Kind, LEASE, now_ms, parse_ms, running, same_path,
    start_watched, stub_exe, testhost,
};

#[test]
fn a_start_writes_the_request_is_claimed_and_answered_and_all_three_go_in_order() {
    let fx = Fixture::new("files");
    let exe = stub_exe();
    let matcher = FakeMatcher::stub("stub_sleep");
    let before = now_ms();
    let (out, watch) = start_watched(&fx, &testhost(), &exe, &matcher, LEASE, ALIAS);
    let holder = match out {
        Started::Holder(rec) => rec,
        Started::Failed(why) => panic!("The start failed: {why}"),
    };

    // The record names the holder, and echoes the lease and the alias.
    assert_eq!(holder.lease_id, LEASE);
    assert_eq!(holder.alias, ALIAS);
    assert!(running(holder.pid, &holder.process_started_at));
    assert_eq!(
        holder.process_started_at.len(),
        24,
        "YYYY-MM-DDTHH:MM:SS.fffZ."
    );
    assert!(
        parse_ms(&holder.process_started_at).is_some_and(|t| t >= before - 2000),
        "Its creation time is from this start."
    );
    assert!(
        !fx.state.join("board-holder.json").exists(),
        "The holder is returned, not recorded: the pass writes the record."
    );

    // The order the files appeared and went in (step 3, step 4, step 8).
    let changes = watch.changes();
    let kinds: Vec<(Kind, bool)> = changes.iter().map(|c| (c.kind, c.added)).collect();
    assert_eq!(
        kinds,
        vec![
            (Kind::Request, true),
            (Kind::Claim, true),
            (Kind::Result, true),
            (Kind::Request, false),
            (Kind::Claim, false),
            (Kind::Result, false),
        ],
        "The request is written, claimed, answered, then removed first, its claim next, the result last."
    );
    assert!(
        changes.iter().all(|c| c.hex == changes[0].hex),
        "All three files are named by one hex."
    );
    assert!(
        !changes[0].hex.is_empty()
            && changes[0]
                .hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-'),
        "The name is a hex."
    );
    assert!(
        fx.launch_files().is_empty(),
        "Nothing of the launch is left in the state folder."
    );

    let cap = &watch.captured()[0];

    // The request, as state.md has it.
    let request_bytes = cap.request.as_ref().expect("The request was read.");
    assert_eq!(
        request_bytes.last(),
        Some(&b'\n'),
        "One LF ends the request."
    );
    assert!(
        !request_bytes[..request_bytes.len() - 1].contains(&b'\n'),
        "The request is one line."
    );
    let request = parse_launch_request(request_bytes).expect("The request meets its schema.");
    assert!(
        same_path(&request.application, &exe),
        "The application is the ssh program."
    );
    assert_eq!(
        request.command_line,
        matcher.holder_command(&exe, ALIAS),
        "The command line is the holder command the matcher gave."
    );
    assert!(
        same_path(&request.log, &fx.log()),
        "The log is board-holder-ssh.log in the state folder."
    );
    assert!(
        same_path(&request.directory, &fx.profile),
        "The working folder is the home folder."
    );
    assert!(
        same_path(
            &request.result,
            &fx.state.join(format!(
                "board-holder-launch.{}.result.json",
                changes[0].hex
            ))
        ),
        "The result path is the result file of the same hex."
    );
    let not_after = parse_ms(&request.not_after).expect("The deadline is a time.");
    assert!(
        not_after >= before + 30_000 - 1 && not_after <= changes[0].wall_ms + 30_000,
        "The deadline is 30 seconds after the moment taken just before the request was written."
    );

    // The claim: `launcher <pid>` with the launcher's PID, which is neither the pass's nor the holder's.
    let claim = parse_claim(cap.claim.as_ref().expect("The claim was read."))
        .expect("The claim is `launcher <pid>` and one LF.");
    match claim {
        Claim::Launcher(pid) => {
            assert!(pid > 0);
            assert_ne!(pid, std::process::id(), "The launcher is not the pass.");
            assert_ne!(pid, holder.pid, "The launcher is not the holder.");
        }
        Claim::Abandoned => panic!("A launcher that answered claimed the request."),
    }

    // The result: the holder, as read back from the process.
    let result_bytes = cap.result.as_ref().expect("The result was read.");
    let result = parse_launch_result(result_bytes).expect("The result meets its schema.");
    match &result {
        LaunchResult::Started {
            pid,
            process_started_at,
            command_line,
        } => {
            assert_eq!(*pid, holder.pid);
            assert_eq!(process_started_at, &holder.process_started_at);
            let line = command_line
                .as_ref()
                .expect("The command line was read back.");
            assert!(
                matcher.is_holder(line, &exe, ALIAS),
                "The command line read back is the holder command."
            );
        }
        LaunchResult::NotStarted { error } => {
            panic!("The launcher answered with an error: {error}")
        }
    }
    assert_eq!(
        serialize_launch_result(&result).expect("Serializable."),
        *result_bytes,
        "The result's bytes are compact JSON in state.md's order, then one LF."
    );
}

#[test]
fn a_launcher_that_cannot_start_the_holder_answers_with_the_error_and_the_start_fails_with_it() {
    let fx = Fixture::new("files-error");
    // A file that exists and is not a program: the launcher's create fails with Windows' own text.
    let not_a_program = fx.root.join("not-a-program.exe");
    std::fs::write(&not_a_program, b"This is not a program.").expect("The file can be made.");
    let matcher = FakeMatcher::stub("stub_sleep");
    let (out, watch) = start_watched(&fx, &testhost(), &not_a_program, &matcher, LEASE, ALIAS);
    let why = match out {
        Started::Failed(why) => why,
        Started::Holder(_) => panic!("A file that is not a program was started."),
    };
    let cap = &watch.captured()[0];
    let result = parse_launch_result(cap.result.as_ref().expect("The launcher answered."))
        .expect("The answer meets its schema.");
    let LaunchResult::NotStarted { error } = result else {
        panic!("The launcher claimed to start something that is not a program.");
    };
    assert!(!error.is_empty());
    assert_eq!(
        why,
        format!("The holder could not be started: {error}"),
        "Step 6: an error in the result fails the start with that error as the reason."
    );
    let kinds: Vec<(Kind, bool)> = watch.changes().iter().map(|c| (c.kind, c.added)).collect();
    assert_eq!(
        kinds,
        vec![
            (Kind::Request, true),
            (Kind::Claim, true),
            (Kind::Result, true),
            (Kind::Request, false),
            (Kind::Claim, false),
            (Kind::Result, false),
        ],
        "The files are removed in order whatever happened."
    );
    assert!(fx.launch_files().is_empty());
}

#[test]
fn a_holder_that_the_matcher_does_not_know_is_not_recorded_and_the_files_are_still_removed() {
    let fx = Fixture::new("files-never");
    let exe = stub_exe();
    let matcher = FakeMatcher {
        accept: Accept::Never,
        ..FakeMatcher::stub("stub_sleep")
    };
    let (out, watch) = start_watched(&fx, &testhost(), &exe, &matcher, LEASE, ALIAS);
    assert!(matches!(out, Started::Failed(_)));
    let removed: Vec<Kind> = watch
        .changes()
        .iter()
        .filter(|c| !c.added)
        .map(|c| c.kind)
        .collect();
    assert_eq!(removed, vec![Kind::Request, Kind::Claim, Kind::Result]);
}

#[test]
fn an_ssh_exe_given_with_forward_slashes_is_still_started() {
    // keeper.md, The holder command, rule 2: Windows compares paths with separators resolved, and
    // the matcher's table has `<ssh>` with forward slashes as a match. A start MUST NOT refuse the
    // program for its separators, while the launch request's paths MUST be full paths with
    // backslashes (state.md, A holder's launch files).
    let fx = Fixture::new("files-slashes");
    let exe = stub_exe();
    let with_slashes = std::path::PathBuf::from(exe.to_string_lossy().replace('\\', "/"));
    let matcher = FakeMatcher::stub("stub_sleep");
    let (out, watch) = start_watched(&fx, &testhost(), &with_slashes, &matcher, LEASE, ALIAS);
    match out {
        Started::Holder(rec) => assert!(running(rec.pid, &rec.process_started_at)),
        Started::Failed(why) => panic!("A program given with forward slashes was refused: {why}"),
    }
    let request = parse_launch_request(
        watch.captured()[0]
            .request
            .as_ref()
            .expect("The request was read."),
    )
    .expect("The request meets its schema, so its paths are full paths.");
    assert!(same_path(&request.application, &exe));
}

#[test]
fn an_ssh_exe_given_with_forward_slashes_is_written_and_run_with_backslashes() {
    // state.md, A holder's launch files (architect, 2026-11-19, for E0.06-T02-S04): the starter
    // MUST turn each `/` of the `-SshExe` it was given into `\` before it writes the request, and
    // MUST build the holder's command line from that same path, so the request, the record and a
    // later match agree. So the command line in the request, and the one the launcher read back
    // from the holder it made, both name the program with backslashes.
    let fx = Fixture::new("files-slashes-line");
    let exe = stub_exe();
    let with_backslashes = exe.to_string_lossy().into_owned();
    assert!(
        !with_backslashes.contains('/'),
        "The spec's own path has backslashes only."
    );
    let with_slashes = std::path::PathBuf::from(with_backslashes.replace('\\', "/"));
    let matcher = FakeMatcher::stub("stub_sleep");
    let (out, watch) = start_watched(&fx, &testhost(), &with_slashes, &matcher, LEASE, ALIAS);
    let holder = match out {
        Started::Holder(rec) => rec,
        Started::Failed(why) => panic!("A program given with forward slashes was refused: {why}"),
    };
    assert!(running(holder.pid, &holder.process_started_at));

    let cap = &watch.captured()[0];
    let request = parse_launch_request(cap.request.as_ref().expect("The request was read."))
        .expect("The request meets its schema.");
    assert_eq!(
        request.application, with_backslashes,
        "The application is the program, written with backslashes."
    );
    assert_eq!(
        request.command_line,
        matcher.holder_command(&exe, ALIAS),
        "The holder's command line is built from the path with backslashes."
    );
    assert_eq!(
        split_command_line(&request.command_line).first(),
        Some(&with_backslashes),
        "The program in the request's command line has backslashes."
    );

    let result = parse_launch_result(cap.result.as_ref().expect("The result was read."))
        .expect("The result meets its schema.");
    let LaunchResult::Started { command_line, .. } = result else {
        panic!("The launcher answered with an error.");
    };
    let line = command_line.expect("The command line was read back.");
    assert_eq!(
        split_command_line(&line).first(),
        Some(&with_backslashes),
        "The program in the holder's own command line has backslashes."
    );
}

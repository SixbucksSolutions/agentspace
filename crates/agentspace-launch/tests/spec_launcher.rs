//! The launcher's half, run in process (docs/keeper.md, The handshake, steps 3 and 4; Starting a
//! holder, items 2 to 4 and 6; docs/state.md, A holder's launch files).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **`run_launcher` is called directly, with a request the spec wrote**, so each step of the
//! launcher is seen alone: the claim, the deadline, the answer, the errors, and the race for the
//! claim. The holder is created by the launcher with `CreateProcess` from this process, not through
//! WMI, so what is observed of it is what the launcher does, and not what WMI does.
#![cfg(windows)]

mod common;

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use agentspace_keeper_files::{
    Claim, LaunchRequest, LaunchResult, parse_claim, parse_launch_result, serialize_launch_request,
    split_command_line,
};
use agentspace_launch::run_launcher;
use common::{
    Fixture, format_ms7, look, now_ms, pass_userprofile, read_text, report, running, same_path,
    stub_command, stub_exe, wait_for,
};

const DEADLINE_ERROR: &str =
    "The launcher reached its deadline before it started the holder, so it started nothing.";

struct Req {
    request: PathBuf,
    claim: PathBuf,
    result: PathBuf,
    log: PathBuf,
    profile: PathBuf,
}

/// Writes a request for the stand-in `name` into a folder of the fixture, named by `hex`.
fn make(fx: &Fixture, hex: &str, name: &str, not_after_ms: i64) -> Req {
    let folder = fx.root.join(format!("launch-{hex}"));
    let state = folder.join("state");
    let profile = folder.join("profile");
    std::fs::create_dir_all(&state).expect("The state folder can be made.");
    std::fs::create_dir_all(&profile).expect("The home folder can be made.");
    let req = Req {
        request: state.join(format!("board-holder-launch.{hex}.json")),
        claim: state.join(format!("board-holder-launch.{hex}.claim")),
        result: state.join(format!("board-holder-launch.{hex}.result.json")),
        log: state.join("board-holder-ssh.log"),
        profile,
    };
    let body = serialize_launch_request(&LaunchRequest {
        application: stub_exe().to_string_lossy().into_owned(),
        command_line: stub_command(name),
        log: req.log.to_string_lossy().into_owned(),
        directory: req.profile.to_string_lossy().into_owned(),
        not_after: format_ms7(not_after_ms),
        result: req.result.to_string_lossy().into_owned(),
    })
    .expect("The request is valid.");
    std::fs::write(&req.request, body).expect("The request can be written.");
    req
}

fn in_30_seconds() -> i64 {
    now_ms() + 30_000
}

fn starts(req: &Req) -> usize {
    read_text(&req.profile.join("spec-starts.txt"))
        .lines()
        .count()
}

fn answer(req: &Req) -> LaunchResult {
    let bytes = std::fs::read(&req.result).expect("The launcher wrote a result.");
    parse_launch_result(&bytes).expect("The result meets its schema.")
}

fn track(fx: &Fixture, result: &LaunchResult) {
    if let LaunchResult::Started {
        pid,
        process_started_at,
        ..
    } = result
    {
        fx.reaper.track(*pid, process_started_at);
    }
}

#[test]
fn the_launcher_claims_starts_the_holder_and_answers_with_what_it_read_back() {
    let fx = Fixture::new("launcher-ok");
    let req = make(&fx, "aa01", "stub_report", in_30_seconds());
    std::fs::write(
        req.profile.join("spec-expect.txt"),
        pass_userprofile().to_string_lossy().as_bytes(),
    )
    .expect("The expectation file can be made.");
    std::fs::write(&req.log, b"OLD LINE\n").expect("The log exists before the start.");

    let code = run_launcher(&req.request);
    assert_eq!(code, 0, "Step 4: the launcher exits 0 when it answered.");

    assert_eq!(
        parse_claim(&std::fs::read(&req.claim).expect("The launcher made its claim.")),
        Some(Claim::Launcher(std::process::id())),
        "Step 3: the claim says `launcher <pid>` with the launcher's own PID."
    );
    let result = answer(&req);
    track(&fx, &result);
    let LaunchResult::Started {
        pid,
        process_started_at,
        command_line,
    } = result
    else {
        panic!("The launcher did not start the holder.");
    };
    assert!(
        running(pid, &process_started_at),
        "The holder runs, and its creation time is the one answered."
    );
    assert_eq!(
        look(pid).map(|(s, _)| s).as_deref(),
        Some(process_started_at.as_str())
    );
    let line = command_line.expect("The command line was read back from the new process.");
    assert_eq!(
        split_command_line(&line)[1..],
        split_command_line(&stub_command("stub_report"))[1..],
        "The command line read back is the request's."
    );
    assert!(
        req.request.exists(),
        "The launcher leaves the request for the pass to remove."
    );

    // What the holder is (items 2 to 4).
    let text = wait_for(Duration::from_secs(20), || {
        let t = read_text(&req.log);
        t.contains("ctrl_c_ignored=").then_some(t)
    })
    .expect("The holder's output reached the log.");
    assert!(
        text.starts_with("OLD LINE\n"),
        "Item 2: the log is appended to, not truncated."
    );
    let r = report(&text);
    assert!(
        r.get("cwd").is_some_and(|c| same_path(c, &req.profile)),
        "Item 4: the working folder is the request's."
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
            "Item 4: a new process group."
        );
    }
    assert_ne!(
        r.get("console_window").map(String::as_str),
        Some("present"),
        "Item 4: no window."
    );
}

#[test]
fn a_launcher_that_cannot_start_the_holder_answers_with_an_error_and_still_exits_0() {
    let fx = Fixture::new("launcher-errors");

    // A program that is not there: Windows' own text.
    let req = make(&fx, "bb01", "stub_marker", in_30_seconds());
    let missing = fx.root.join("nowhere").join("ssh.exe");
    let mut text = read_text(&req.request);
    text = text.replace(
        &stub_exe().to_string_lossy().replace('\\', "\\\\"),
        &missing.to_string_lossy().replace('\\', "\\\\"),
    );
    std::fs::write(&req.request, text).expect("The request can be changed.");
    assert_eq!(run_launcher(&req.request), 0);
    match answer(&req) {
        LaunchResult::NotStarted { error } => assert!(!error.is_empty(), "An error says why."),
        LaunchResult::Started { .. } => panic!("A program that is not there was started."),
    }
    assert_eq!(
        parse_claim(&std::fs::read(&req.claim).expect("Claimed.")),
        Some(Claim::Launcher(std::process::id()))
    );

    // A file that is not a program.
    let not_a_program = fx.root.join("not-a-program.exe");
    std::fs::write(&not_a_program, b"Not a program.").expect("The file can be made.");
    let req = make(&fx, "bb02", "stub_marker", in_30_seconds());
    let text = read_text(&req.request).replace(
        &stub_exe().to_string_lossy().replace('\\', "\\\\"),
        &not_a_program.to_string_lossy().replace('\\', "\\\\"),
    );
    std::fs::write(&req.request, text).expect("The request can be changed.");
    assert_eq!(run_launcher(&req.request), 0);
    assert!(matches!(
        answer(&req),
        LaunchResult::NotStarted { error } if !error.is_empty()
    ));

    // A working folder that is not there.
    let req = make(&fx, "bb03", "stub_marker", in_30_seconds());
    std::fs::remove_dir_all(&req.profile).expect("The home folder can be removed.");
    assert_eq!(run_launcher(&req.request), 0);
    assert!(matches!(
        answer(&req),
        LaunchResult::NotStarted { error } if !error.is_empty()
    ));
}

#[test]
fn a_log_that_cannot_be_opened_is_answered_in_keeper_mds_words_and_starts_nothing() {
    let fx = Fixture::new("launcher-log");
    let req = make(&fx, "cc01", "stub_marker", in_30_seconds());
    let missing_log = fx.root.join("no-such-folder").join("board-holder-ssh.log");
    let text = read_text(&req.request).replace(
        &req.log.to_string_lossy().replace('\\', "\\\\"),
        &missing_log.to_string_lossy().replace('\\', "\\\\"),
    );
    std::fs::write(&req.request, text).expect("The request can be changed.");

    assert_eq!(run_launcher(&req.request), 0);
    assert_eq!(
        answer(&req),
        LaunchResult::NotStarted {
            error: "The holder's log could not be opened.".to_string()
        },
        "Step 4 gives this error."
    );
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(starts(&req), 0, "No holder was started.");
}

#[test]
fn a_launcher_that_claims_after_its_deadline_starts_nothing_and_says_so() {
    let fx = Fixture::new("launcher-late");
    let req = make(&fx, "dd01", "stub_marker", now_ms() - 60_000);

    assert_eq!(run_launcher(&req.request), 0, "It still exits 0.");
    assert_eq!(
        answer(&req),
        LaunchResult::NotStarted {
            error: DEADLINE_ERROR.to_string()
        },
        "Step 4 gives this error."
    );
    assert_eq!(
        parse_claim(&std::fs::read(&req.claim).expect("It claimed first.")),
        Some(Claim::Launcher(std::process::id())),
        "It claims before it does anything else, the deadline included."
    );
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(starts(&req), 0, "No holder was started.");
}

#[test]
fn a_launcher_that_cannot_claim_starts_nothing_and_writes_nothing() {
    let fx = Fixture::new("launcher-lost");
    for (hex, content) in [("ee01", "launcher 1\n"), ("ee02", "abandoned\n")] {
        let req = make(&fx, hex, "stub_marker", in_30_seconds());
        std::fs::write(&req.claim, content).expect("The other's claim can be made.");
        let before = std::fs::read(&req.request).expect("The request is there.");

        let _ = run_launcher(&req.request);
        std::thread::sleep(Duration::from_millis(1500));
        assert!(
            !req.result.exists(),
            "A launcher that lost the claim writes no result ({hex})."
        );
        assert_eq!(starts(&req), 0, "It started nothing ({hex}).");
        assert_eq!(
            read_text(&req.claim),
            content,
            "The winner's claim stands ({hex})."
        );
        assert_eq!(
            std::fs::read(&req.request).expect("The request is still there."),
            before,
            "The launcher did not touch the request ({hex})."
        );
        assert!(!req.log.exists(), "It did not even open the log ({hex}).");
    }
}

#[test]
fn a_request_the_launcher_cannot_read_is_claimed_then_released_and_starts_nothing() {
    let fx = Fixture::new("launcher-unreadable");
    let exe = stub_exe().to_string_lossy().replace('\\', "\\\\");
    let good_log = "C:\\\\x\\\\board-holder-ssh.log";
    let cases: Vec<(&str, String)> = vec![
        (
            "relative application",
            format!(
                "{{\"application\":\"ssh.exe\",\"command_line\":\"ssh.exe\",\"log\":\"{good_log}\",\"directory\":\"C:\\\\x\",\"not_after\":\"2099-01-01T00:00:00.0000000Z\",\"result\":\"C:\\\\x\\\\r.json\"}}\n"
            ),
        ),
        (
            "a deadline with three fraction digits",
            format!(
                "{{\"application\":\"{exe}\",\"command_line\":\"x\",\"log\":\"{good_log}\",\"directory\":\"C:\\\\x\",\"not_after\":\"2099-01-01T00:00:00.000Z\",\"result\":\"C:\\\\x\\\\r.json\"}}\n"
            ),
        ),
        ("empty", String::new()),
        ("not JSON", "this is not json\n".to_string()),
    ];
    for (i, (what, body)) in cases.into_iter().enumerate() {
        let hex = format!("ff{i:02}");
        let req = make(&fx, &hex, "stub_marker", in_30_seconds());
        std::fs::write(&req.request, body).expect("The request can be written.");
        let _ = run_launcher(&req.request);
        std::thread::sleep(Duration::from_millis(800));
        assert!(
            !req.claim.exists(),
            "A launcher that claimed a request it cannot read removes its claim ({what})."
        );
        assert!(!req.result.exists(), "It writes no result ({what}).");
        assert_eq!(starts(&req), 0, "It starts nothing ({what}).");
    }

    // A request the pass has already removed.
    let req = make(&fx, "ff99", "stub_marker", in_30_seconds());
    std::fs::remove_file(&req.request).expect("The request can be removed.");
    let _ = run_launcher(&req.request);
    std::thread::sleep(Duration::from_millis(800));
    assert!(
        !req.claim.exists(),
        "No claim is left for a request that is gone."
    );
    assert!(!req.result.exists());
    assert_eq!(starts(&req), 0);
}

#[test]
fn the_claim_makes_at_most_one_holder_when_launchers_and_the_pass_race_for_a_request() {
    let fx = Fixture::new("launcher-race");
    let mut launcher_won = 0;
    let mut pass_won = 0;
    for round in 0..12 {
        let req = Arc::new(make(
            &fx,
            &format!("a{round:03}"),
            "stub_marker",
            in_30_seconds(),
        ));
        let barrier = Arc::new(Barrier::new(5));
        let mut threads = Vec::new();
        for _ in 0..4 {
            let (req, barrier) = (req.clone(), barrier.clone());
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                run_launcher(&req.request)
            }));
        }
        // The pass withdraws the request by claiming it itself, with `abandoned`.
        let pass = {
            let (req, barrier) = (req.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                std::thread::sleep(Duration::from_micros(300 * (round as u64 % 5)));
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&req.claim)
                {
                    Ok(mut f) => f.write_all(b"abandoned\n").is_ok(),
                    Err(_) => false,
                }
            })
        };
        for t in threads {
            let _ = t.join().expect("A launcher did not panic.");
        }
        let pass_claimed = pass.join().expect("The pass did not panic.");

        // Let a holder, if one was started, write its line; then look again for a second.
        let expected = if pass_claimed { 0 } else { 1 };
        let _ = wait_for(Duration::from_secs(3), || {
            (starts(&req) >= expected).then_some(())
        });
        std::thread::sleep(Duration::from_millis(700));
        let count = starts(&req);

        if pass_claimed {
            pass_won += 1;
            assert_eq!(
                parse_claim(&std::fs::read(&req.claim).expect("The claim is there.")),
                Some(Claim::Abandoned)
            );
            assert_eq!(
                count, 0,
                "Round {round}: the pass won the claim, so no holder started."
            );
            assert!(
                !req.result.exists(),
                "Round {round}: and no result was written."
            );
        } else {
            launcher_won += 1;
            assert_eq!(
                parse_claim(&std::fs::read(&req.claim).expect("The claim is there.")),
                Some(Claim::Launcher(std::process::id())),
                "Round {round}: a launcher holds the claim."
            );
            assert_eq!(count, 1, "Round {round}: one holder started, never two.");
            let result = answer(&req);
            track(&fx, &result);
            assert!(matches!(result, LaunchResult::Started { .. }));
        }
    }
    eprintln!("The launchers won {launcher_won} rounds and the pass won {pass_won}.");
}

#[test]
fn many_launchers_for_one_request_start_one_holder() {
    let fx = Fixture::new("launcher-many");
    for round in 0..8 {
        let req = Arc::new(make(
            &fx,
            &format!("b{round:03}"),
            "stub_marker",
            in_30_seconds(),
        ));
        let barrier = Arc::new(Barrier::new(6));
        let threads: Vec<_> = (0..6)
            .map(|_| {
                let (req, barrier) = (req.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    run_launcher(&req.request)
                })
            })
            .collect();
        let codes: Vec<i32> = threads
            .into_iter()
            .map(|t| t.join().expect("A launcher did not panic."))
            .collect();
        let _ = wait_for(Duration::from_secs(3), || (starts(&req) >= 1).then_some(()));
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(
            starts(&req),
            1,
            "Round {round}: exactly one holder started."
        );
        let result = answer(&req);
        track(&fx, &result);
        assert!(matches!(result, LaunchResult::Started { .. }));
        assert!(
            codes.contains(&0),
            "Round {round}: the launcher that answered exited 0."
        );
        assert_eq!(
            parse_claim(&std::fs::read(&req.claim).expect("Claimed.")),
            Some(Claim::Launcher(std::process::id()))
        );
    }
}

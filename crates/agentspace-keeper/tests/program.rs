//! The program, wired (E0.06-T03-S04; docs/keeper.md, Command line; Exit codes; Streams and
//! environment; Fast failures and backoff).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every test runs the built
//! program as a separate process in a throwaway home with a throwaway state folder, through
//! `Bin` of the shared harness, so none touches a real file of the user's. What the program does
//! about a panic is tested in the binary's own unit tests (`src/main.rs`), which need a fake.

mod common;

use agentspace_keeper_files::{
    LaunchRequest, LaunchResult, launch_request_file, parse_launch_result, result_file,
    serialize_launch_request,
};
use common::{BIN, Bin, USAGE};
use std::process::{Command, Stdio};

/// The two usage lines hold no `-Launch` (keeper.md, Command line: it is not part of the
/// interface; agentspace-launch: it MUST NOT appear in a usage line).
#[test]
fn the_usage_lines_do_not_name_the_launcher_argument() {
    assert!(
        !agentspace_keeper::USAGE
            .to_ascii_lowercase()
            .contains("launch"),
        "The USAGE text names the launcher"
    );
    assert!(!USAGE.to_ascii_lowercase().contains("launch"));
    // And so does every usage error the program writes, whatever its message.
    let bin = Bin::new();
    for args in [
        vec!["-Bogus"],
        vec!["-Watch", "-Watch"],
        vec!["-SshExe"],
        vec!["-IntervalSeconds", "5"],
        vec!["-Now", "never"],
        vec!["-Launch"],
        vec!["-Launch", "a", "b"],
    ] {
        let ran = bin.run(&args);
        assert_eq!(ran.status, Some(2), "For {args:?}");
        let (message, usage) = ran
            .stderr
            .split_once('\n')
            .expect("A usage error has a message line");
        assert!(message.starts_with("agentspace-keeper: "));
        assert_eq!(usage, USAGE, "For {args:?}");
        assert!(
            !usage.to_ascii_lowercase().contains("launch"),
            "A usage line names the launcher for {args:?}"
        );
        assert_eq!(ran.stdout, "");
    }
}

/// Only exactly `-Launch <request>` is the launcher; anything else with `-Launch` is the keeper's
/// own command line, in which it is an unknown argument.
#[test]
fn the_launcher_argument_is_the_launcher_only_with_one_request_after_it() {
    let bin = Bin::new();
    for args in [
        vec!["-Launch"],
        vec!["-Launch", "x", "y"],
        vec!["-Now", "2026-11-19T12:00:00Z", "-Launch", "x"],
    ] {
        let ran = bin.run(&args);
        assert_eq!(ran.status, Some(2), "For {args:?}");
        assert!(
            ran.stderr
                .starts_with("agentspace-keeper: Unknown argument '-Launch'.\n"),
            "For {args:?}: {}",
            ran.stderr.lines().next().unwrap_or("")
        );
    }
    // The name is compared exactly, case included (agentspace-launch's `launcher_request`), so
    // `-launch` is the keeper's too, and no launcher.
    let ran = bin.run(&["-launch", "x"]);
    assert_eq!(ran.status, Some(2));
    assert!(
        ran.stderr
            .starts_with("agentspace-keeper: Unknown argument '-launch'.\n")
    );
}

/// `-Launch <path>` goes to the launcher before any usage check, and does nothing else: a path
/// that names no request is the launcher's exit 2 with nothing written to either stream, where a
/// usage error would write three lines. No state folder is made.
#[test]
fn the_launcher_role_is_routed_before_the_usage_checks() {
    let bin = Bin::new();
    let ran = bin.run(&["-Launch", "not-a-request.json"]);
    assert_eq!(ran.status, Some(2));
    assert_eq!(ran.stderr, "", "The launcher writes nothing to stderr");
    assert_eq!(ran.stdout, "", "The launcher writes nothing to stdout");
    assert!(bin.state_is_empty(), "The launcher makes no state folder");
    assert!(!bin.state.exists());
}

/// A request whose deadline has passed, run through the program as the launcher: the launcher
/// claims it, starts nothing, and writes its result (keeper.md, The handshake). The result file
/// is what shows the launcher ran, since the keeper's own role never writes one.
#[cfg(windows)]
#[test]
fn the_launcher_role_answers_a_request_whose_deadline_has_passed() {
    let bin = Bin::new();
    std::fs::create_dir_all(&bin.state).unwrap();
    let hex = "0123456789abcdef0123456789abcdef";
    let request = bin.state.join(launch_request_file(hex));
    let result = bin.state.join(result_file(hex));
    let bytes = serialize_launch_request(&LaunchRequest {
        application: bin.ssh.to_string_lossy().into_owned(),
        command_line: format!("\"{}\" -N demo", bin.ssh.display()),
        log: bin
            .state
            .join("board-holder-ssh.log")
            .to_string_lossy()
            .into_owned(),
        directory: bin.home.to_string_lossy().into_owned(),
        not_after: "2000-01-01T00:00:00.0000000Z".to_string(),
        result: result.to_string_lossy().into_owned(),
    })
    .expect("The request is valid");
    std::fs::write(&request, bytes).unwrap();

    let ran = bin.run(&["-Launch", &request.to_string_lossy()]);
    assert_eq!(ran.status, Some(0));
    assert_eq!(ran.stderr, "");
    assert_eq!(ran.stdout, "");
    let answer = parse_launch_result(&std::fs::read(&result).expect("The result is written"))
        .expect("The result is valid");
    assert!(
        matches!(answer, LaunchResult::NotStarted { .. }),
        "A passed deadline starts nothing"
    );
    assert!(
        !bin.state.join("board-holder.log").exists(),
        "The launcher logs nothing"
    );
}

/// A one-pass run over the real parts: the lock is taken for real, the pass logs, and the line is
/// also written to stdout.
#[cfg(windows)]
#[test]
fn a_pass_over_the_real_parts_logs_idle_and_takes_the_pass_lock() {
    let bin = Bin::new();
    let ran = bin.run(&["-Now", "2026-11-19T12:00:00Z"]);
    assert_eq!(ran.status, Some(0), "{}", ran.stderr);
    let line = "2026-11-19T12:00:00Z idle lease=- alias=- pid=-\n";
    assert_eq!(ran.stdout, line);
    assert_eq!(ran.stderr, "");
    assert_eq!(
        std::fs::read_to_string(bin.state.join("board-holder.log")).unwrap(),
        line
    );
    assert!(
        bin.state.join("board-holder.lock").exists(),
        "The real lock file was made"
    );
}

/// A failed write of a log line to stdout MUST NOT count as an error (keeper.md, Exit codes; the
/// installed task's null handles). The program runs with both its pipes' reading ends gone, so
/// every write to them fails, and a one-pass run still ends with exit 0 and its line logged.
///
/// The test holds `board-holder.lock` while it drops the reading ends, so the pass waits for the
/// lock and writes its line only after they are gone, with no race.
#[cfg(windows)]
#[test]
fn a_failed_write_to_stdout_or_stderr_is_no_error() {
    use std::os::windows::fs::OpenOptionsExt;
    let bin = Bin::new();
    std::fs::create_dir_all(&bin.state).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(bin.state.join("board-holder.lock"))
        .expect("The test takes the pass lock");
    let mut cmd = Command::new(BIN);
    cmd.args(["-Now", "2026-11-19T12:00:00Z"])
        .arg("-SshExe")
        .arg(&bin.ssh)
        .current_dir(&bin.home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
        cmd.env(name, &bin.home);
    }
    cmd.env("AGENTSPACE_STATE_DIR", &bin.state);
    bin.guard_env(&mut cmd);
    let mut child = cmd.spawn().expect("The program starts");
    drop(child.stdout.take());
    drop(child.stderr.take());
    // The pass waits for the lock for up to 10 seconds; it gets it now, with its pipes dead.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    drop(lock);
    let status = child.wait().expect("The program ends");
    assert_eq!(status.code(), Some(0), "A dead stdout is no error");
    let log = std::fs::read_to_string(bin.state.join("board-holder.log"))
        .expect("The log was written whatever stdout did");
    assert_eq!(log, "2026-11-19T12:00:00Z idle lease=- alias=- pid=-\n");
}

/// The same for a keeper: every pass logs its line, and `-MaxPasses` ends it with exit 0, with
/// both pipes dead from before its first write.
#[cfg(windows)]
#[test]
fn a_keeper_with_dead_pipes_runs_its_passes() {
    use std::os::windows::fs::OpenOptionsExt;
    let bin = Bin::new();
    std::fs::create_dir_all(&bin.state).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(bin.state.join("board-holder.lock"))
        .expect("The test takes the pass lock");
    let mut cmd = Command::new(BIN);
    cmd.args(["-Watch", "-MaxPasses", "2", "-IntervalSeconds", "1"])
        .arg("-SshExe")
        .arg(&bin.ssh)
        .current_dir(&bin.home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
        cmd.env(name, &bin.home);
    }
    cmd.env("AGENTSPACE_STATE_DIR", &bin.state);
    bin.guard_env(&mut cmd);
    let mut child = cmd.spawn().expect("The program starts");
    drop(child.stdout.take());
    drop(child.stderr.take());
    std::thread::sleep(std::time::Duration::from_millis(1500));
    drop(lock);
    let status = child.wait().expect("The program ends");
    assert_eq!(status.code(), Some(0), "A dead stdout is no error");
    let log = std::fs::read_to_string(bin.state.join("board-holder.log"))
        .expect("The log was written whatever stdout did");
    assert_eq!(
        log.lines().filter(|l| l.contains(" idle ")).count(),
        2,
        "Both passes logged their line"
    );
}

/// The same with the standard handles on the null device, as a task with no console has them.
#[test]
fn a_one_pass_run_with_null_handles_ends_0_and_logs() {
    let bin = Bin::new();
    let mut cmd = Command::new(BIN);
    cmd.args(["-Now", "2026-11-19T12:00:00Z"])
        .arg("-SshExe")
        .arg(&bin.ssh)
        .current_dir(&bin.home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
        cmd.env(name, &bin.home);
    }
    cmd.env("AGENTSPACE_STATE_DIR", &bin.state);
    bin.guard_env(&mut cmd);
    let status = cmd.status().expect("The program runs");
    // On a system that is not Windows the pass fails at its lock and logs it; the line is the
    // log's, and the exit is 0 whichever the pass did.
    assert_eq!(status.code(), Some(0));
    let log = std::fs::read_to_string(bin.state.join("board-holder.log")).unwrap();
    assert_eq!(log.lines().count(), 1);
}

/// Only `board-holder.log`'s write counts: a log that cannot be written is the pass's own line
/// that could not be logged, exit 1, whatever stdout does.
#[test]
fn a_log_that_cannot_be_written_is_exit_1() {
    let bin = Bin::new();
    std::fs::create_dir_all(bin.state.join("board-holder.log")).unwrap();
    let ran = bin.run(&["-Now", "2026-11-19T12:00:00Z"]);
    assert_eq!(ran.status, Some(1));
    assert_eq!(ran.stdout, "", "A line not logged is not written to stdout");
}

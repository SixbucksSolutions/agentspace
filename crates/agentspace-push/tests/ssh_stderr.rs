//! What the push does with its ssh's stderr (operator.md, The token push, What the laptop sends,
//! step 2; Every ssh runs the ProxyCommand; What a caller sees; #0167, P14, architect, 2026-11-21,
//! for v0.08-T01; built by E0.08-T02-S04, #0176).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Windows only.
//!
//! **In the ProxyCommand's caller mode** the stderr is a pipe, and each line read from it reaches
//! the sink the caller chose, but a line that is exactly `The host is ready.`. These cases run the
//! push's ssh, `SystemSsh`, with `SshStderr::Relay` and a sink they own, as the ProxyCommand runs
//! it. **In a person's `up`, `up -Host` and `sync`** it is the operator's own stderr, inherited,
//! which is the process's: those cases run the push in a child process, this test program started
//! again with only its entry test selected, and the parent reads the child's stderr.
//!
//! The stand-in ssh lives in a throwaway folder, reads its stdin to the end, copies a file of
//! chosen bytes to its stderr as they are, and then answers as the box does or fails. The child's
//! environment is cleared and then given what it needs through `Command::env`; the push's own
//! seams (the stand-in, the zone) are given through its look-up of the environment.
#![cfg(windows)]

mod impl_support;

use agentspace_operator_api::{CallerLine, Mode};
use agentspace_push::{
    Context, Outcome, Say, SshResult, SshRun, SshStderr, StderrSink, SystemSsh, push_to_host_using,
    push_using_with, remote_command, ssh_args,
};
use impl_support::{ALIAS, CONTENT, INSTANCE, Lines, Sandbox};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const READY: &str = "The host is ready.";
const NEW_YORK: &str = "America/New_York";

// ---------------------------------------------------------------------------------------------
// The stand-in ssh
// ---------------------------------------------------------------------------------------------

fn hash_line() -> String {
    let mut h = Sha256::new();
    h.update(CONTENT.as_bytes());
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("{hex}  -")
}

/// What the ssh stand-in does after it has copied `stderr.bin` to its stderr.
#[derive(Clone, Copy, PartialEq)]
enum Then {
    /// Answers as the box does and exits 0.
    Confirm,
    /// Exits 255 with no hash line.
    Fail,
    /// Answers as the box does, and leaves a process holding the stderr pipe for about 7 seconds
    /// after it exits. The process's stdout is the null device.
    ConfirmLeavingAHolder,
    /// Waits for two minutes, so that only the limit ends it.
    Hang,
}

/// Writes the stand-in ssh: it logs its arguments, reads its stdin to the end, copies
/// `stderr_bytes` to its stderr byte for byte, and then does what `then` says.
///
/// `type` reads a file that begins with the bytes `FF FE` as UTF-16, so no case starts its bytes so.
fn write_ssh(sb: &Sandbox, stderr_bytes: &[u8], then: Then) {
    fs::write(sb.standin.join("stderr.bin"), stderr_bytes).unwrap();
    let mut s = String::from("@echo off\r\n");
    let mut line = |t: &str| {
        s.push_str(t);
        s.push_str("\r\n");
    };
    line(">>\"%~dp0ssh.log\" echo %*");
    line("findstr \"^\" >nul 2>nul");
    line("type \"%~dp0stderr.bin\" 1>&2");
    match then {
        Then::Confirm | Then::ConfirmLeavingAHolder => {
            if then == Then::ConfirmLeavingAHolder {
                // The holder leaves `holder-ended.txt` as it ends, so the test can wait for it.
                line(
                    "start \"\" /b cmd /c \"ping -n 7 127.0.0.1 >nul & type nul >\"%~dp0holder-ended.txt\"\"",
                );
            }
            line(&format!("echo {}", hash_line()));
            line(&format!("echo zone {NEW_YORK}"));
            line("exit /b 0");
        }
        Then::Fail => line("exit /b 255"),
        Then::Hang => {
            line("ping -n 120 127.0.0.1 >nul");
            line("exit /b 0");
        }
    }
    fs::write(sb.ssh_cmd(), s).unwrap();
}

fn rig(stderr_bytes: &[u8], then: Then) -> Sandbox {
    let sb = Sandbox::new();
    sb.write_localswim();
    write_ssh(&sb, stderr_bytes, then);
    sb
}

// ---------------------------------------------------------------------------------------------
// The ProxyCommand's caller mode: a pipe, read into a sink
// ---------------------------------------------------------------------------------------------

/// A sink the test owns, which stands for the ProxyCommand's stderr.
fn sink() -> (Arc<Mutex<Vec<u8>>>, StderrSink) {
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let as_sink: StderrSink = buf.clone();
    (buf, as_sink)
}

fn taken(buf: &Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    buf.lock().unwrap().clone()
}

fn relaying_ssh(sb: &Sandbox, sink: StderrSink, diagnostic: bool, limits: (u64, u64)) -> SystemSsh {
    SystemSsh::with_lookup(&sb.context(), sb.lookup(Some(NEW_YORK)))
        .with_command_setup(sb.setup())
        .with_limits(
            Duration::from_millis(limits.0),
            Duration::from_millis(limits.1),
        )
        .with_stderr(SshStderr::Relay { sink, diagnostic })
}

fn run_ssh(ssh: &SystemSsh) -> SshResult {
    let args = ssh_args(ALIAS, &remote_command(NEW_YORK));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    ssh.run(&args, CONTENT.as_bytes())
}

/// Runs the push's ssh with the relay, and gives back what reached the sink, and the result.
fn relayed(stderr_bytes: &[u8], then: Then, diagnostic: bool) -> (Vec<u8>, SshResult) {
    let sb = rig(stderr_bytes, then);
    let (buf, sink) = sink();
    let result = run_ssh(&relaying_ssh(&sb, sink, diagnostic, (20_000, 10_000)));
    (taken(&buf), result)
}

fn assert_exited(result: &SshResult, want_code: i32) {
    assert!(
        matches!(result, SshResult::Exited { code, .. } if *code == want_code),
        "The ssh did not exit with {want_code}: {result:?}"
    );
}

#[test]
fn in_caller_mode_the_ready_line_is_dropped_and_every_other_line_passes_on_in_order() {
    let ssh_err = b"Warning: Permanently added 'host' to the list of known hosts.\r\nThe host is ready.\r\nbanner\nThe host is ready.\nthe last line, no ending";
    let (shown, result) = relayed(ssh_err, Then::Confirm, false);
    assert_exited(&result, 0);
    assert_eq!(
        shown,
        b"Warning: Permanently added 'host' to the list of known hosts.\r\nbanner\nthe last line, no ending"
    );
}

#[test]
fn in_caller_mode_each_line_keeps_the_line_ending_it_came_with() {
    let (shown, result) = relayed(
        b"one\r\ntwo\n\r\n\nThe host is ready.\r\nthree\r\nlast",
        Then::Confirm,
        false,
    );
    assert_exited(&result, 0);
    assert_eq!(shown, b"one\r\ntwo\n\r\n\nthree\r\nlast");
}

#[test]
fn in_caller_mode_a_line_that_is_not_exactly_the_ready_line_is_kept() {
    let near: [&[u8]; 12] = [
        b"The host is ready. \n",
        b"The host is ready. \r\n",
        b" The host is ready.\n",
        b"\tThe host is ready.\n",
        b"the host is ready.\n",
        b"The Host is ready.\n",
        b"The host is ready\n",
        b"The host is ready..\n",
        b"The host is ready.x\n",
        b"x The host is ready.\n",
        // A CR that is not part of a CR LF is part of the line.
        b"The host is ready.\r\r\n",
        b"The host is ready.\xff\n",
    ];
    for kept in near {
        let mut ssh_err = b"top\n".to_vec();
        ssh_err.extend_from_slice(kept);
        ssh_err.extend_from_slice(b"The host is ready.\nbottom\n");
        let mut want = b"top\n".to_vec();
        want.extend_from_slice(kept);
        want.extend_from_slice(b"bottom\n");
        let (shown, result) = relayed(&ssh_err, Then::Confirm, false);
        assert_exited(&result, 0);
        assert_eq!(shown, want, "A near miss of the ready line was dropped");
    }
}

#[test]
fn in_caller_mode_the_last_line_is_judged_like_any_other() {
    // Exactly the ready line, with no ending: dropped.
    let (shown, _) = relayed(b"before\nThe host is ready.", Then::Confirm, false);
    assert_eq!(shown, b"before\n");
    // Any other last line with no ending: passed on as it is, with none added.
    let (shown, _) = relayed(b"first\nlast line, no ending", Then::Confirm, false);
    assert_eq!(shown, b"first\nlast line, no ending");
    // The ready line and a lone CR is not that line.
    let (shown, _) = relayed(b"top\nThe host is ready.\r", Then::Confirm, false);
    assert_eq!(shown, b"top\nThe host is ready.\r");
}

#[test]
fn in_caller_mode_bytes_are_never_decoded() {
    let ssh_err: &[u8] = b"first line\r\n\xff\xfe\xc3\x28 raw\r\nThe host is ready.\n\xc0\xaf\x80\x81\r\nnul\x00inside\n\xed\xa0\x80\nend\xe2\x82";
    let (shown, result) = relayed(ssh_err, Then::Confirm, false);
    assert_exited(&result, 0);
    assert_eq!(
        shown,
        b"first line\r\n\xff\xfe\xc3\x28 raw\r\n\xc0\xaf\x80\x81\r\nnul\x00inside\n\xed\xa0\x80\nend\xe2\x82"
    );
}

#[test]
fn in_caller_mode_with_diagnostic_set_nothing_is_dropped() {
    let ssh_err: &[u8] =
        b"The host is ready.\r\nwarning\nThe host is ready.\n\xff raw\r\nThe host is ready.";
    let (shown, result) = relayed(ssh_err, Then::Confirm, true);
    assert_exited(&result, 0);
    assert_eq!(shown, ssh_err);
}

#[test]
fn in_caller_mode_a_failing_ssh_still_shows_its_error_lines() {
    // Never the null device: a failed push's ssh errors reach the caller.
    let ssh_err: &[u8] = b"ssh: connect to host dummy port 22: Connection refused\r\nThe host is ready.\r\nssh: second error line\r\nlast error, no ending";
    let (shown, result) = relayed(ssh_err, Then::Fail, false);
    assert_exited(&result, 255);
    assert_eq!(
        shown,
        b"ssh: connect to host dummy port 22: Connection refused\r\nssh: second error line\r\nlast error, no ending"
    );
}

#[test]
fn in_caller_mode_the_rest_of_the_push_is_unchanged() {
    // The exit code and the stdout are the ssh's, as they are with the operator's stderr.
    let sb = rig(b"The host is ready.\nnoise\n", Then::Confirm);
    let (buf, sink) = sink();
    let result = run_ssh(&relaying_ssh(&sb, sink, false, (20_000, 10_000)));
    let SshResult::Exited { code, stdout } = result else {
        panic!("The ssh did not exit on its own");
    };
    assert_eq!(code, 0);
    let text = String::from_utf8(stdout).expect("UTF-8");
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        [hash_line().as_str(), "zone America/New_York"]
    );
    assert_eq!(taken(&buf), b"noise\n");
}

#[test]
fn in_caller_mode_a_lot_of_stderr_never_stops_the_ssh_and_arrives_whole() {
    // Far more than a pipe holds, with the last lines written just before the ssh exits.
    let mut ssh_err = Vec::new();
    let mut want = Vec::new();
    for n in 0..3000 {
        let text = format!("filler line {n:05} {}\r\n", "x".repeat(70));
        ssh_err.extend_from_slice(text.as_bytes());
        want.extend_from_slice(text.as_bytes());
        if n % 500 == 0 {
            ssh_err.extend_from_slice(b"The host is ready.\r\n");
        }
    }
    ssh_err.extend_from_slice(b"The very last line, no ending");
    want.extend_from_slice(b"The very last line, no ending");
    let (shown, result) = relayed(&ssh_err, Then::Confirm, false);
    assert!(
        matches!(&result, SshResult::Exited { code: 0, stdout } if !stdout.is_empty()),
        "A full pipe MUST NOT stop the ssh: {result:?}"
    );
    assert_eq!(shown.len(), want.len(), "The count of bytes");
    assert!(shown == want, "The lines arrived changed or out of order");
}

#[test]
fn in_caller_mode_the_lines_the_pipe_holds_at_the_ssh_exit_are_passed_on() {
    // The stand-in writes its last lines and exits at once, so those lines may be in the pipe,
    // unread, when the ssh exits: a push that stops reading at the exit loses them, and the sink
    // MUST hold them when the call returns.
    for round in 0..6 {
        let ssh_err = format!(
            "round {round} first\r\n{}ssh: the final error line\r\nthe very last, no ending",
            "padding padding padding padding padding\r\n".repeat(40)
        );
        let (shown, result) = relayed(ssh_err.as_bytes(), Then::Fail, false);
        assert_exited(&result, 255);
        assert!(
            shown.ends_with(b"ssh: the final error line\r\nthe very last, no ending"),
            "Round {round}: the last error lines were lost"
        );
    }
}

#[test]
fn in_caller_mode_a_process_holding_the_pipe_open_does_not_lengthen_the_push() {
    // The ssh exits, and a process it started keeps the stderr pipe open for about 7 seconds. The
    // pipe is read while the push runs and no longer: what it already holds is passed on first,
    // and the call returns without waiting for more.
    let sb = rig(
        b"ssh: a warning\r\nThe host is ready.\r\nssh: another warning\r\n",
        Then::ConfirmLeavingAHolder,
    );
    let (buf, sink) = sink();
    // The holder may hold the stdout pipe as well, which is waited for, here for at most 1.5
    // seconds.
    let ssh = relaying_ssh(&sb, sink, false, (20_000, 1_500));
    let started = Instant::now();
    let result = run_ssh(&ssh);
    let took = started.elapsed();
    assert!(
        matches!(result, SshResult::Exited { .. }),
        "The ssh did not exit on its own: {result:?}"
    );
    assert!(
        took < Duration::from_secs(4),
        "The push took {took:?}, which waited for the pipe's other holder"
    );
    assert_eq!(taken(&buf), b"ssh: a warning\r\nssh: another warning\r\n");
    // The pipe is read no longer: nothing reaches the sink while the holder still runs.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(taken(&buf), b"ssh: a warning\r\nssh: another warning\r\n");
    // The holder is this test's own process, and the test ends only once it has ended.
    let ended = sb.standin.join("holder-ended.txt");
    let until = Instant::now() + Duration::from_secs(20);
    while !ended.exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(ended.exists(), "The process holding the pipe did not end");
}

#[test]
fn in_caller_mode_the_limit_stops_the_ssh_and_the_reading_and_ends_the_push_at_once() {
    // The 20-second limit, shortened to 1.5 seconds, still stops the ssh's tree and ends the push
    // at once, and the pipe is read no longer after it.
    let sb = rig(b"ssh: written before the limit\r\n", Then::Hang);
    let (buf, sink) = sink();
    let ssh = relaying_ssh(&sb, sink, false, (1_500, 10_000));
    let started = Instant::now();
    let result = run_ssh(&ssh);
    let took = started.elapsed();
    assert!(
        matches!(result, SshResult::TimedOut),
        "The ssh was not timed out: {result:?}"
    );
    assert!(
        took >= Duration::from_millis(1_000),
        "Ended early: {took:?}"
    );
    assert!(
        took < Duration::from_secs(8),
        "The push took {took:?}: the limit, then at once"
    );
    // A line written, and so read, before the limit was passed on.
    let at_return = taken(&buf);
    assert_eq!(at_return, b"ssh: written before the limit\r\n");
    // And nothing reaches the sink after the call has returned.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(taken(&buf), at_return);
}

#[test]
fn a_whole_push_in_caller_mode_shows_a_failed_pushs_ssh_lines_and_its_own_warning() {
    let sb = rig(
        b"ssh: connect failed\r\nThe host is ready.\r\nssh: second\r\n",
        Then::Fail,
    );
    let (buf, sink) = sink();
    let say = Lines::default();
    let outcome = push_using_with(
        &sb.context(),
        &say,
        INSTANCE,
        ALIAS,
        false,
        sb.lookup(Some(NEW_YORK)),
        &|ssh| {
            ssh.with_command_setup(sb.setup())
                .with_stderr(SshStderr::Relay {
                    sink: sink.clone(),
                    diagnostic: false,
                })
        },
    );
    assert_eq!(outcome, Outcome::NotConfirmed);
    assert_eq!(taken(&buf), b"ssh: connect failed\r\nssh: second\r\n");
    assert!(
        say.printed()
            .contains(&agentspace_push::NOT_CONFIRMED.to_string()),
        "The warning is the push's own"
    );
}

// ---------------------------------------------------------------------------------------------
// A person's `up`, `up -Host` and `sync`: the operator's stderr, inherited
// ---------------------------------------------------------------------------------------------

const CHILD: &str = "ASP_STDERR_CHILD_MODE";
const CHILD_HOME: &str = "ASP_STDERR_CHILD_HOME";
const CHILD_LAD: &str = "ASP_STDERR_CHILD_LAD";
const CHILD_WORK: &str = "ASP_STDERR_CHILD_WORK";
const CHILD_SYSROOT: &str = "ASP_STDERR_CHILD_SYSROOT";
const CHILD_SSH: &str = "ASP_STDERR_CHILD_SSH";

/// A `Say` for a person's command: its lines go to the process's stderr, as the real one's do.
struct PersonSay;

impl Say for PersonSay {
    fn mode(&self) -> Mode {
        Mode::Person
    }
    fn progress(&self, person: &str, _caller: Option<CallerLine>) {
        eprintln!("{person}");
    }
    fn warning(&self, person: &str) {
        eprintln!("{person}");
    }
    fn detail(&self, _text: &str) {}
}

/// The entry the parent starts again. Without its variable it does nothing, so a plain run of the
/// suite passes it at once. `default` runs the push with its ssh as the operator builds it;
/// `inherit` asks for `SshStderr::Inherit` by name.
#[test]
fn child_entry() {
    let Some(mode) = std::env::var_os(CHILD) else {
        return;
    };
    let var = |name: &str| PathBuf::from(std::env::var_os(name).expect("A child variable"));
    let (home, lad, work, sysroot, ssh) = (
        var(CHILD_HOME),
        var(CHILD_LAD),
        var(CHILD_WORK),
        var(CHILD_SYSROOT),
        var(CHILD_SSH),
    );
    let ctx = Context {
        home: home.clone(),
        local_app_data: Some(lad.clone()),
        system_root: sysroot,
        nested: false,
    };
    let lookup: Arc<agentspace_push::Lookup> = Arc::new(move |name| match name {
        "AGENTSPACE_SSH_CMD" => Some(ssh.clone().into_os_string()),
        "AGENTSPACE_TIMEZONE" => Some(OsString::from(NEW_YORK)),
        _ => None,
    });
    let by_name = mode == "inherit";
    let outcome = push_to_host_using(
        &ctx,
        &PersonSay,
        INSTANCE,
        ALIAS,
        false,
        None,
        lookup,
        &|ssh| {
            let ssh = ssh.with_command_setup({
                let (home, lad, work) = (home.clone(), lad.clone(), work.clone());
                move |cmd: &mut Command| {
                    cmd.current_dir(&work)
                        .env("USERPROFILE", &home)
                        .env("HOME", &home)
                        .env("LOCALAPPDATA", &lad)
                        .env("APPDATA", &lad)
                        .env("TEMP", &work)
                        .env("TMP", &work);
                }
            });
            if by_name {
                ssh.with_stderr(SshStderr::Inherit)
            } else {
                ssh
            }
        },
    );
    println!("OUTCOME={outcome:?}");
}

struct Child {
    code: Option<i32>,
    stdout: String,
    stderr: Vec<u8>,
}

impl Child {
    fn outcome(&self) -> String {
        self.stdout
            .split("OUTCOME=")
            .nth(1)
            .and_then(|rest| rest.lines().next())
            .unwrap_or_default()
            .to_string()
    }
}

fn person_run(stderr_bytes: &[u8], then: Then, mode: &str) -> Child {
    let sb = rig(stderr_bytes, then);
    let root = PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set"));
    let mut cmd = Command::new(std::env::current_exe().expect("This test program"));
    cmd.args(["child_entry", "--exact", "--nocapture", "--test-threads=1"])
        .env_clear()
        .current_dir(&sb.work)
        .env("SystemRoot", &root)
        .env("windir", &root)
        .env("ComSpec", root.join("System32").join("cmd.exe"))
        .env("PATH", root.join("System32"))
        .env("TEMP", &sb.work)
        .env("TMP", &sb.work)
        .env(CHILD, mode)
        .env(CHILD_HOME, &sb.home)
        .env(CHILD_LAD, &sb.lad)
        .env(CHILD_WORK, &sb.work)
        .env(CHILD_SYSROOT, &sb.sysroot)
        .env(CHILD_SSH, sb.ssh_cmd())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("Start the child");
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err.read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        if let Some(st) = child.try_wait().expect("Wait") {
            break Some(st);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = String::from_utf8_lossy(&out_reader.join().unwrap()).into_owned();
    let stderr = err_reader.join().unwrap();
    assert!(status.is_some(), "The child did not finish in 90 seconds");
    Child {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    let (mut n, mut i) = (0, 0);
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            n += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    n
}

fn assert_child_ok(c: &Child) {
    assert_eq!(
        c.code,
        Some(0),
        "The child failed; its stdout:\n{}\nits stderr:\n{}",
        c.stdout,
        String::from_utf8_lossy(&c.stderr)
    );
    assert!(
        c.stdout.contains("1 passed"),
        "The entry test did not run: {}",
        c.stdout
    );
}

#[test]
fn in_a_persons_run_the_ssh_stderr_is_the_operators_and_shows_the_ready_line() {
    let ssh_err: &[u8] = b"The host is ready.\r\nWarning: Permanently added 'host' to the list of known hosts.\r\nThe host is ready.\nlast, no ending";
    for mode in ["default", "inherit"] {
        let c = person_run(ssh_err, Then::Confirm, mode);
        assert_child_ok(&c);
        assert_eq!(c.outcome(), "Some(Pushed)", "{mode}");
        assert!(
            contains(&c.stderr, ssh_err),
            "{mode}: a person's run shows every line of its ssh's stderr, as it wrote it; stderr:\n{}",
            String::from_utf8_lossy(&c.stderr)
        );
        assert_eq!(
            count(&c.stderr, READY.as_bytes()),
            2,
            "{mode}: a person's run drops nothing"
        );
    }
}

#[test]
fn in_a_persons_run_a_failing_push_still_shows_its_ssh_error_lines() {
    let ssh_err: &[u8] = b"ssh: connect to host dummy port 22: Connection refused\r\n";
    let c = person_run(ssh_err, Then::Fail, "default");
    assert_child_ok(&c);
    assert_eq!(c.outcome(), "Some(NotConfirmed)");
    assert!(contains(&c.stderr, ssh_err));
}

//! The architect's ruling P7 for E0.10-T03-S03 (#0225, #0227): a crate that starts a program itself
//! starts it through [`agentspace_run::start_guarded`], and a file Windows takes for a 16-bit
//! program then fails to start at once, with error 216, from a thread whose error mode is 0.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The command is the one `bootstrap`'s ssh is (agentspace-sync `bootstrap.rs`, `run_ssh`): built by
//! its caller, given the environment [`agentspace_run::prepare_child_env`] gives, and with a pipe for
//! its stdin. The test MUST NOT lean on an error mode it inherited, so it sets this process's error
//! mode and the starting thread's to 0 first, and only the guard can keep the dialog away. Its own
//! test binary, with this one test, whose only start is the guarded one, so that nothing starts a
//! program unguarded under those modes.
//!
//! Without the guard, the start puts a modal "Unsupported 16-Bit Application" dialog on the desktop
//! and waits for OK. The start runs on a thread of its own, and the test fails when it has not
//! answered within [`BOUND`]; the process then ends, and the waiting thread with it.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::Diagnostics::Debug::{
    GetErrorMode, GetThreadErrorMode, SetErrorMode, SetThreadErrorMode,
};

/// How long the start MAY take. One that fails as it should takes well under a second; one that
/// waits on the dialog never ends.
const BOUND: Duration = Duration::from_secs(5);

/// `ERROR_EXE_MACHINE_TYPE_MISMATCH`: the start of a file Windows takes for a 16-bit program, on a
/// thread whose error mode holds `SEM_FAILCRITICALERRORS`.
const MACHINE_TYPE_MISMATCH: i32 = 216;

#[test]
fn a_guarded_start_of_a_prepared_command_that_is_not_a_program_fails_at_once_from_a_thread_at_mode_0()
 {
    let dir = tempfile::Builder::new()
        .prefix("guarded-start-")
        .tempdir_in(Path::new(env!("CARGO_TARGET_TMPDIR")))
        .unwrap();
    let file = dir.path().join("ssh.exe");
    std::fs::write(&file, "this is not an executable").unwrap();

    // SAFETY: sets and reads this process's own error mode.
    let process_before = unsafe { SetErrorMode(0) };
    assert_eq!(unsafe { GetErrorMode() }, 0, "The process error mode is 0");

    let (tx, rx) = mpsc::channel();
    let starter = std::thread::spawn(move || {
        let mut thread_before = 0u32;
        // SAFETY: reads and sets this thread's own error mode; the out-parameter is a local.
        unsafe { SetThreadErrorMode(0, &mut thread_before) };
        let at_start = unsafe { GetThreadErrorMode() };
        // As `bootstrap`'s `run_ssh` builds its ssh.
        let mut cmd = Command::new(&file);
        cmd.args(["-o", "ClearAllForwardings=yes", "agentspace", "bash -s"])
            .stdin(Stdio::piped());
        agentspace_run::prepare_child_env(&mut cmd);
        let started = Instant::now();
        let outcome = match agentspace_run::start_guarded(&mut cmd) {
            Ok(mut child) => {
                // Not expected; a child that started anyway is ended by its own handle.
                let _ = child.kill();
                let _ = child.wait();
                Ok(())
            }
            Err(error) => Err(error.raw_os_error()),
        };
        let took = started.elapsed();
        let after = unsafe { GetThreadErrorMode() };
        // SAFETY: puts this thread's own error mode back; no old mode is asked for.
        unsafe { SetThreadErrorMode(thread_before, std::ptr::null_mut()) };
        let _ = tx.send((outcome, at_start, after, took));
    });

    let (outcome, at_start, after, took) = rx.recv_timeout(BOUND).unwrap_or_else(|_| {
        panic!(
            "The guarded start did not answer within {BOUND:?}: it is waiting on a dialog (#0225)"
        )
    });
    starter.join().unwrap();
    // SAFETY: puts this process's own error mode back.
    unsafe { SetErrorMode(process_before) };

    assert_eq!(at_start, 0, "The starting thread's error mode was 0");
    assert_eq!(
        outcome,
        Err(Some(MACHINE_TYPE_MISMATCH)),
        "The start failed with error 216"
    );
    assert!(took < BOUND, "The start took {took:?}");
    assert_eq!(
        after, 0,
        "The thread's error mode is put back after the start"
    );
}

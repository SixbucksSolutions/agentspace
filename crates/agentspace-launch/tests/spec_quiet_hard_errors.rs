//! `quiet_hard_errors` (#0191; architect's pin P21, 2026-11-24): from a process at error mode 0,
//! after the call, a start of a `.exe` that is not a Windows program returns its error at once,
//! with no message box, and the process's mode has both bits and keeps the others it had.
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **This spec is a test program of its own**, so the process error mode it sets reaches no other
//! spec. It sets the mode back as it found it when it ends.
#![cfg(windows)]

mod hard_errors;

use hard_errors::{
    ProcessModeBack, Scratch, not_a_program, process_mode, set_process_mode, start_bounded,
    stuck_message,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SEM_NOOPENFILEERRORBOX,
};

#[test]
fn after_quiet_hard_errors_a_process_at_mode_0_starts_a_non_program_with_an_error_at_once() {
    let _back = ProcessModeBack::new();
    let scratch = Scratch::new("quiet");
    let program = not_a_program(scratch.path());

    // Mode 0, then one other bit planted, which the call MUST keep.
    set_process_mode(0);
    set_process_mode(SEM_NOGPFAULTERRORBOX);

    agentspace_launch::quiet_hard_errors();

    // The start first: its thread's own mode is 0, so only the process's mode can keep the
    // message box away.
    let home = scratch.path().to_path_buf();
    let outcome = start_bounded(move || {
        std::process::Command::new(&program)
            .current_dir(&home)
            .env("USERPROFILE", &home)
            .env("HOME", &home)
            .env("LOCALAPPDATA", &home)
            .env("APPDATA", &home)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|mut child| {
                let _ = child.kill();
                let _ = child.wait();
            })
    });
    let (started, _took) = outcome.unwrap_or_else(|stuck| panic!("{}", stuck_message(&stuck)));
    let error = started.expect_err("A file that is not a program is not started.");
    assert!(
        error.raw_os_error().is_some(),
        "Windows says why the start failed."
    );

    assert_eq!(
        process_mode(),
        SEM_NOGPFAULTERRORBOX | SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX,
        "The process's mode has both bits, and the bit it had before is kept."
    );
}

//! The launcher's create never waits on a message box, with no help from any parent (#0191;
//! architect, 2026-11-24, option B).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **The process's error mode and the calling thread's are both 0 before the launcher runs**, as
//! in a process started hidden by `Start-Process`, so only the launcher's own guard around its
//! `CreateProcessW` can keep Windows' "Unsupported 16-Bit Application" message box away. The
//! request names a `.exe` that is not a Windows program. The launcher MUST answer with Windows'
//! error at once, with no `16-Bit` window on the desktop, and MUST leave the process's mode as it
//! found it.
//!
//! **This spec is a test program of its own**, so the process error mode it sets reaches no other
//! spec. It sets the mode back as it found it when it ends.
#![cfg(windows)]

mod hard_errors;

use agentspace_keeper_files::{
    LaunchRequest, LaunchResult, parse_launch_result, serialize_launch_request,
};
use agentspace_launch::run_launcher;
use hard_errors::{
    ProcessModeBack, Scratch, not_a_program, process_mode, set_process_mode, start_bounded,
    stuck_message,
};

#[test]
fn a_launcher_at_error_mode_0_answers_a_non_program_with_an_error_at_once_and_no_message_box() {
    let _back = ProcessModeBack::new();
    let scratch = Scratch::new("no-error-box");
    let program = not_a_program(scratch.path());
    let state = scratch.path().join("state");
    let profile = scratch.path().join("profile");
    std::fs::create_dir_all(&state).expect("The state folder can be made.");
    std::fs::create_dir_all(&profile).expect("The home folder can be made.");
    let request = state.join("board-holder-launch.dd01.json");
    let result = state.join("board-holder-launch.dd01.result.json");
    let body = serialize_launch_request(&LaunchRequest {
        application: program.to_string_lossy().into_owned(),
        command_line: format!("\"{}\" -N", program.display()),
        log: state
            .join("board-holder-ssh.log")
            .to_string_lossy()
            .into_owned(),
        directory: profile.to_string_lossy().into_owned(),
        not_after: "2099-01-01T00:00:00.0000000Z".to_string(),
        result: result.to_string_lossy().into_owned(),
    })
    .expect("The request is valid.");
    std::fs::write(&request, body).expect("The request can be written.");

    set_process_mode(0);
    let outcome = start_bounded(move || run_launcher(&request));
    let (code, _took) = outcome.unwrap_or_else(|stuck| panic!("{}", stuck_message(&stuck)));

    assert_eq!(code, 0, "The launcher answered, so it exits 0.");
    let answer = parse_launch_result(&std::fs::read(&result).expect("The launcher answered."))
        .expect("The answer meets its schema.");
    assert!(
        matches!(answer, LaunchResult::NotStarted { ref error } if !error.is_empty()),
        "A file that is not a program is not started, and Windows' own text says why."
    );
    assert_eq!(
        process_mode(),
        0,
        "The launcher quiets its own thread alone, never the process."
    );
}

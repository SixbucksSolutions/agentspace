//! #0191: a file Windows takes for a 16-bit program fails to start at once, with no dialog, however
//! the test process was started (docs/operator.md, The test seam; Programs it starts, "When a
//! default program cannot be run").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The test MUST NOT lean on an error mode it inherited: a run wrapper or a person's console can
//! hand the process `SEM_FAILCRITICALERRORS`, which hides the dialog without the crate's help. So
//! it sets this process's error mode and the starting thread's to 0 first, and only the crate's own
//! guard can keep the dialog away. Its own test binary, with this one test, so that no other test
//! starts a program under a process error mode of 0.
//!
//! Without the guard, the first start puts a modal "Unsupported 16-Bit Application" dialog on the
//! desktop and waits for OK. The starts run on a thread of their own, and the test fails when they
//! have not all answered within [`BOUND`]; the process then ends, and the waiting thread with it.
#![cfg(windows)]
#![allow(unsafe_code)]

mod common;

use agentspace_operator_api::{Aws, AwsError, Call, Output, Remote};
use agentspace_run::Runner;
use common::{Rig, cannot_run_message};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::Diagnostics::Debug::{
    GetErrorMode, GetThreadErrorMode, SetErrorMode, SetThreadErrorMode,
};

/// How long all the starts together MAY take. Each one that fails as it should takes well under a
/// second; one that waits on the dialog never ends.
const BOUND: Duration = Duration::from_secs(10);

/// A file named `.exe` that holds no PE image: Windows takes it for a 16-bit program.
fn not_a_program(dir: &Path, name: &str) -> String {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, "this is not an executable").unwrap();
    path.display().to_string()
}

#[test]
fn a_file_that_is_no_program_fails_to_start_at_once_with_no_dialog_whatever_the_inherited_error_mode()
 {
    // SAFETY: sets and reads this process's own error mode.
    unsafe { SetErrorMode(0) };
    assert_eq!(unsafe { GetErrorMode() }, 0, "The process error mode is 0");

    let rig = Rig::new();
    let aws = not_a_program(&rig.dir.path().join("seam"), "aws.exe");
    let ssh = not_a_program(&rig.dir.path().join("seam"), "ssh.exe");
    let scp = not_a_program(&rig.dir.path().join("seam"), "scp.exe");
    let openssh = rig.ctx.system_root.join("System32").join("OpenSSH");
    let default_ssh = not_a_program(&openssh, "ssh.exe");
    let default_scp = not_a_program(&openssh, "scp.exe");
    let path_dir = rig.dir.path().join("on path");
    let path_aws = not_a_program(&path_dir, "aws.exe");

    // Runners of their own, built from owned values, since the starts run on a thread of their own.
    let seam = rig.env.clone();
    seam.set("AGENTSPACE_AWS_CMD", &aws);
    seam.set("AGENTSPACE_SSH_CMD", &ssh);
    seam.set("AGENTSPACE_SCP_CMD", &scp);
    let seamed = Runner::with_lookup(rig.ctx.clone(), move |name| seam.get(name));
    let defaults_env = common::Env::default();
    defaults_env.set("PATH", path_dir.as_os_str());
    let defaults = Runner::with_lookup(rig.ctx.clone(), move |name| defaults_env.get(name));

    let (tx, rx) = mpsc::channel();
    let starter = std::thread::spawn(move || {
        // SAFETY: sets and reads this thread's own error mode.
        unsafe { SetThreadErrorMode(0, std::ptr::null_mut()) };
        let before = unsafe { GetThreadErrorMode() };
        let started = Instant::now();
        let call = |output| Call {
            service: "ec2",
            operation: "describe-instances",
            args: &[],
            output,
        };
        let aws_answers: Vec<Result<serde_json::Value, AwsError>> = [
            Output::Json,
            Output::Checked,
            Output::PassThrough,
            Output::Ignore,
        ]
        .into_iter()
        .map(|output| seamed.call(&call(output)))
        .collect();
        let remote = [
            seamed.ssh(&["devbox"], Some("true")).map_err(|f| f.message),
            seamed.scp(&["a", "devbox:b"]).map_err(|f| f.message),
            defaults
                .ssh(&["devbox"], Some("true"))
                .map_err(|f| f.message),
            defaults.scp(&["a", "devbox:b"]).map_err(|f| f.message),
        ];
        // The AWS CLI's fixed path is the real machine's, so the fallback on PATH is only reached
        // when it is missing there.
        let fallback = (!Path::new(r"C:\Program Files\Amazon\AWSCLIV2\aws.exe").is_file())
            .then(|| defaults.call(&call(Output::Json)));
        let after = unsafe { GetThreadErrorMode() };
        let _ = tx.send((
            aws_answers,
            remote,
            fallback,
            before,
            after,
            started.elapsed(),
        ));
    });

    let (aws_answers, remote, fallback, before, after, took) = rx.recv_timeout(BOUND).unwrap_or_else(
        |_| {
            panic!(
                "The starts did not all answer within {BOUND:?}: a start is waiting on a dialog (#0191)"
            )
        },
    );
    starter.join().unwrap();

    assert!(took < BOUND, "The starts took {took:?}");
    assert_eq!(before, 0, "The starting thread's error mode was 0");
    assert_eq!(
        after, 0,
        "The thread's error mode is put back after each start"
    );
    let seam_line = cannot_run_message;
    for answer in aws_answers {
        assert_eq!(
            answer,
            Err(AwsError::CannotRun(seam_line("AGENTSPACE_AWS_CMD", &aws)))
        );
    }
    assert_eq!(
        remote,
        [
            Err(seam_line("AGENTSPACE_SSH_CMD", &ssh)),
            Err(seam_line("AGENTSPACE_SCP_CMD", &scp)),
            Err(format!("agentspace: Could not run {default_ssh}.")),
            Err(format!("agentspace: Could not run {default_scp}.")),
        ]
    );
    if let Some(answer) = fallback {
        assert_eq!(
            answer,
            Err(AwsError::CannotRun(format!(
                "agentspace: Could not run {path_aws}."
            )))
        );
    }
}

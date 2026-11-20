//! A holder outlives the job of the process that asked for it (docs/keeper.md, Starting a holder,
//! items 1 and 5).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **Item 1: the holder MUST outlive the pass and every process above it**, so it MUST NOT be in
//! the job of the process that asked for it. A spec process is put in a job object that ends
//! everything in it when it is closed; that process starts a holder through the real starter and
//! the real launcher, WMI included; the spec closes the job; and the holder still runs.
//!
//! The same process also starts a plain child of its own, with the runtime's own process start, as
//! the control: the job MUST end that one, or the spec has shown nothing.
#![cfg(windows)]

mod common;

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use common::{Fixture, KillOnCloseJob, look, running, stub_args, stub_exe, wait_for};

fn pid_and_start(report: &str, key: &str) -> Option<(u32, String)> {
    let line = report
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")))?;
    let (pid, started) = line.split_once(' ')?;
    Some((pid.parse().ok()?, started.to_string()))
}

#[test]
fn a_holder_started_through_wmi_outlives_the_job_of_the_process_that_asked() {
    let fx = Fixture::new("job");
    let go = fx.root.join("go.txt");
    let report_path: PathBuf = fx.root.join("report.txt");

    let job = KillOnCloseJob::new();
    let mut helper = Command::new(stub_exe())
        .raw_arg(stub_args("job_helper"))
        .env("SPEC_STATE", &fx.state)
        .env("SPEC_PROFILE", &fx.profile)
        .env("SPEC_GO", &go)
        .env("SPEC_REPORT", &report_path)
        .current_dir(&fx.profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("The pass's process can be started.");
    // It waits for the go file, so it is in the job before it does anything.
    job.add(&helper);
    assert!(
        job.contains(helper.id()),
        "The pass's process is in the job."
    );
    std::fs::write(&go, b"go").expect("The go file can be made.");

    let report = wait_for(Duration::from_secs(90), || {
        std::fs::read_to_string(&report_path).ok()
    })
    .expect("The pass's process reported what it started.");
    assert!(
        !report.contains("failed="),
        "The start failed: {}",
        report
            .lines()
            .find(|l| l.starts_with("failed="))
            .unwrap_or("")
    );
    let (holder_pid, holder_started) = pid_and_start(&report, "holder").expect("A holder.");
    let (control_pid, control_started) = pid_and_start(&report, "control").expect("A control.");
    fx.reaper.track(holder_pid, &holder_started);
    fx.reaper.track(control_pid, &control_started);

    // Before the close: the control is in the job, the holder is not, and both run.
    assert!(running(control_pid, &control_started), "The control runs.");
    assert!(running(holder_pid, &holder_started), "The holder runs.");
    assert!(
        job.contains(control_pid),
        "The control, which the pass's process started itself, is in the job."
    );
    assert!(
        !job.contains(holder_pid),
        "Item 1: the holder is not in the job of the process that asked for it."
    );

    // Close the job: it ends the pass's process and the control.
    job.close();
    let exited = wait_for(Duration::from_secs(20), || helper.try_wait().ok().flatten());
    assert!(
        exited.is_some(),
        "The pass's process was ended with its job."
    );
    let control_gone = wait_for(Duration::from_secs(10), || {
        (!running(control_pid, &control_started)).then_some(())
    });
    assert!(
        control_gone.is_some(),
        "The control was ended with its job, so the job did what it was made to do."
    );

    // And the holder runs on, a moment later too.
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        running(holder_pid, &holder_started),
        "Item 1: the holder outlived the job of the process that asked for it."
    );
    assert_eq!(
        look(holder_pid).map(|(s, _)| s).as_deref(),
        Some(holder_started.as_str()),
        "It is the same process."
    );
}

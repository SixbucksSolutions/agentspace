//! The Windows stand-in's own checks (tests/common/mod.rs, `Standin`).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **A call of the stand-in MUST start at most two processes** (#0191, E0.10-T02-S04): every
//!   process start is what a loaded machine slows most, and a stand-in that started four per call
//!   left the rulings tests crawling for minutes under load. This binary puts its own process in a
//!   job object and reads how many processes the job has held, before and after one call. Its own
//!   test binary, so that no other test's processes are counted.
//! - **The stand-in MUST still see what it exists to see:** a line on its stdin, and a variable
//!   whose name ends `_POSIX_FD_STATE`. A check that "nothing leaked" passes vacuously otherwise.
#![cfg(windows)]
#![allow(unsafe_code)]

mod common;

use agentspace_operator_api::{Aws, Call, Output};
use common::Rig;
use std::io::Write;
use std::process::{Command, Stdio};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JobObjectBasicAccountingInformation, QueryInformationJobObject,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// How many processes `job` has held in its life, this test process included.
fn processes_held(job: HANDLE) -> u32 {
    // SAFETY: an all-zero accounting record is a valid value of this plain-data struct.
    let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `job` is a live job handle of ours; the buffer is the struct the class names, and
    // its size is given.
    let ok = unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicAccountingInformation,
            (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "The job's accounting could not be read");
    info.TotalProcesses
}

#[test]
fn a_call_of_the_stand_in_starts_at_most_two_processes() {
    // SAFETY: creates an unnamed job with default security, and puts this process in it; the job
    // sets no limit, and lives as long as the process.
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    assert!(!job.is_null(), "A job object could not be created");
    let assigned = unsafe { AssignProcessToJobObject(job, GetCurrentProcess()) };
    assert_ne!(assigned, 0, "This test process could not join its job");

    let rig = Rig::new();
    let aws = rig.standin("aws").stdout("{}").write();
    rig.env.set("AGENTSPACE_AWS_CMD", aws.value());
    let before = processes_held(job);
    rig.runner()
        .call(&Call {
            service: "ec2",
            operation: "describe-instances",
            args: &[],
            output: Output::Json,
        })
        .unwrap();
    let started = processes_held(job) - before;
    assert!(aws.ran(), "The stand-in was not started");
    assert!(
        started <= 2,
        "One call of the stand-in started {started} processes; at most 2 (cmd.exe and the one for /f runs)"
    );
}

#[test]
fn the_stand_in_sees_a_line_on_its_stdin_and_a_variable_ending_posix_fd_state() {
    let rig = Rig::new();
    let s = rig.standin("seen").write();
    let mut cmd = Command::new(s.path());
    // Only the planted name: one this process inherited (an OpenSSH session's) is removed.
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().ends_with("_POSIX_FD_STATE") {
            cmd.env_remove(name);
        }
    }
    let mut child = cmd
        .env("PLANTED_CHECK_POSIX_FD_STATE", "1")
        .env("PLANTED_CHECK_POSIX_FD_STATE_NOT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"LEAKED-STDIN-DATA\r\n").unwrap();
    drop(stdin);
    assert!(child.wait().unwrap().success());
    assert!(
        s.stdin().contains("LEAKED-STDIN-DATA"),
        "The stand-in did not record the line on its stdin"
    );
    assert_eq!(s.fd_state_names(), ["PLANTED_CHECK_POSIX_FD_STATE"]);
}

//! Item 3 of Starting a holder (docs/keeper.md): the holder inherits no handle but its stdin and
//! its log. A test binary of its own, because it makes an inheritable handle in this process,
//! which a child that another test started at the same time would also inherit.

#![cfg(windows)]
#![allow(unsafe_code)]

mod impl_support;

use agentspace_keeper_files::{LaunchResult, parse_launch_result, result_file};
use impl_support::{Proc, Scratch, testhost, wait_child, wait_for};
use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::time::Duration;
use windows_sys::Win32::Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation};

/// `board-holder.lock` as the pass holds it: sharing nothing, but here made inheritable, as a
/// careless keeper might leave it.
fn hold_inheritable(path: &Path) -> File {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(path)
        .unwrap();
    let ok = unsafe {
        SetHandleInformation(
            file.as_raw_handle() as HANDLE,
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        )
    };
    assert_ne!(ok, 0);
    file
}

/// Whether anything else still holds the file: an open that shares nothing is refused.
fn held_elsewhere(path: &Path) -> bool {
    OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .is_err()
}

#[test]
fn the_holder_inherits_no_handle_but_its_stdin_and_its_log() {
    let s = Scratch::new("handles");
    let lock = s.state().join("board-holder.lock");

    // The probe can see an inherited handle: a child started the ordinary way keeps it.
    let held = hold_inheritable(&lock);
    let mut control = s
        .command(&testhost())
        .args(["--sleep", "30"])
        .spawn()
        .unwrap();
    drop(held);
    let seen = held_elsewhere(&lock);
    control.kill().unwrap();
    control.wait().unwrap();
    assert!(
        seen,
        "The probe did not see a handle that a plain child inherited."
    );
    assert!(
        wait_for(Duration::from_secs(5), || (!held_elsewhere(&lock))
            .then_some(()))
        .is_some()
    );

    // The launcher inherits it, as the control did, and the holder it makes does not.
    let stub = s.stub("ssh-stub.cmd", "AGENTSPACE_LAUNCH_IMPL_MARKER", 60);
    let hex = "00000000000000000000000000000007";
    let request = s.request(hex, &stub, "agentspace-launchtest-0a1b2c3d", 30);
    let held = hold_inheritable(&lock);
    let mut launcher = s.launcher(&request);
    drop(held);
    assert_eq!(wait_child(&mut launcher, Duration::from_secs(20)), Some(0));
    let bytes = std::fs::read(s.state().join(result_file(hex))).unwrap();
    let LaunchResult::Started {
        pid,
        process_started_at,
        ..
    } = parse_launch_result(&bytes).unwrap()
    else {
        panic!("The launcher started nothing.");
    };
    let holder = Proc::open(pid, &process_started_at).unwrap();
    wait_for(Duration::from_secs(10), || {
        s.log_text().contains("err-line").then_some(())
    });
    let leaked = held_elsewhere(&lock);
    let running = holder.running();
    holder.stop_tree();
    assert!(running);
    assert!(!leaked, "The holder inherited board-holder.lock.");
}

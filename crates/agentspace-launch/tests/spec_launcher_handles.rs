//! A holder inherits no handle (docs/keeper.md, Starting a holder, item 3).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **Item 3: the holder inherits no handle but its stdout and stderr**, and in particular not
//! `board-holder-keeper.lock` or `board-holder.lock`. The launcher here runs in this process, which
//! holds both lock files open with inheritable handles, as a keeper does by mistake. If the holder
//! inherited either, the file would stay locked after this process closed its own handle, and it
//! could not be deleted while the holder runs.
//!
//! **The control is a child made with `bInheritHandles` and nothing else restricted**: the same
//! check MUST find the handle there, or it has shown nothing. This is the one spec in its binary,
//! so no other thread creates a process while an inheritable handle is open.
#![cfg(windows)]

mod common;

use std::time::Duration;

use agentspace_keeper_files::{
    LaunchRequest, LaunchResult, parse_launch_result, serialize_launch_request,
};
use agentspace_launch::run_launcher;
use common::{
    Fixture, can_delete, format_ms7, now_ms, open_inheritable, running,
    spawn_inheriting_everything, stub_command, stub_exe, wait_for,
};

#[test]
fn a_holder_made_by_the_launcher_holds_no_lock_file_open() {
    let fx = Fixture::new("handles");
    let pass_lock = fx.state.join("board-holder.lock");
    let keeper_lock = fx.state.join("board-holder-keeper.lock");

    // The control first: a child that inherits everything holds a file this process has closed.
    {
        let a = open_inheritable(&pass_lock, 0);
        let b = open_inheritable(&keeper_lock, 1);
        let control = spawn_inheriting_everything(&fx, &stub_exe(), &stub_command("stub_sleep"));
        drop(a);
        drop(b);
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !can_delete(&pass_lock),
            "The control: a child that inherited the handle keeps the file from being deleted."
        );
        assert!(
            !can_delete(&keeper_lock),
            "The control, for the other lock."
        );
        let started = common::look(control).map(|(s, _)| s).unwrap_or_default();
        common::kill_verified(control, &started);
        assert!(
            wait_for(Duration::from_secs(5), || {
                can_delete(&pass_lock).then_some(())
            })
            .is_some(),
            "Once the control ends, the file can be deleted: the control was the reason."
        );
        let _ = std::fs::remove_file(&keeper_lock);
    }

    // The launcher.
    let a = open_inheritable(&pass_lock, 0);
    let b = open_inheritable(&keeper_lock, 1);
    let log = fx.state.join("board-holder-ssh.log");
    let request = fx.state.join("board-holder-launch.c001.json");
    let result = fx.state.join("board-holder-launch.c001.result.json");
    let body = serialize_launch_request(&LaunchRequest {
        application: stub_exe().to_string_lossy().into_owned(),
        command_line: stub_command("stub_sleep"),
        log: log.to_string_lossy().into_owned(),
        directory: fx.profile.to_string_lossy().into_owned(),
        not_after: format_ms7(now_ms() + 30_000),
        result: result.to_string_lossy().into_owned(),
    })
    .expect("The request is valid.");
    std::fs::write(&request, body).expect("The request can be written.");

    assert_eq!(run_launcher(&request), 0);
    let answer = parse_launch_result(&std::fs::read(&result).expect("The launcher answered."))
        .expect("The answer meets its schema.");
    let LaunchResult::Started {
        pid,
        process_started_at,
        ..
    } = answer
    else {
        panic!("The launcher did not start the holder.");
    };
    fx.reaper.track(pid, &process_started_at);
    assert!(running(pid, &process_started_at), "The holder runs.");

    drop(a);
    drop(b);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        can_delete(&pass_lock),
        "Item 3: the holder did not inherit board-holder.lock."
    );
    assert!(
        can_delete(&keeper_lock),
        "Item 3: the holder did not inherit board-holder-keeper.lock."
    );
    assert!(
        running(pid, &process_started_at),
        "The holder ran through the check."
    );
}

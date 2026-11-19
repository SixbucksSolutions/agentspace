//! Starting a holder from a pass: what the pass hands the starter, the record, the `started` and
//! `failed` lines, and the refusals before a start (docs/keeper.md, A pass; Starting a holder:
//! Before the start, The handshake's last steps; Log lines).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **The pass writes the
//! record for the holder, and only then logs `started`, so a `started` line always follows its
//! record.** **If the record cannot be written, the pass stops the holder at once** and logs
//! `failed` with the holder's PID. **The pass refuses to start, and logs `failed lease=<id>
//! alias=<alias> pid=- reason=The holder could not be started: <why>`, when `-SshExe` is not a
//! file, the state folder's path or the keeper's own program path holds a `"`, or the alias is not
//! a host alias.** The handshake itself is the `Starter`'s, and is faked here.

mod common;

use agentspace_keeper_api::{HolderRecord, Started, Stopped};
use common::*;
use std::path::PathBuf;

fn holder() -> HolderRecord {
    HolderRecord {
        pid: 31337,
        process_started_at: "2026-11-19T11:59:58.500Z".to_string(),
        lease_id: ID_A.to_string(),
        alias: alias_of(ID_A),
    }
}

fn ready_a() -> Harness {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h
}

// ---- A start that works ----

#[test]
fn a_started_holder_is_recorded_then_logged() {
    let h = ready_a();
    h.set_start(Started::Holder(holder()));
    let before = h.log_bytes();
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        h.log(),
        line(NOW, "started", ID_A, &alias_of(ID_A), "31337", None)
    );
    assert_stdout_is_log_since(&h, &before, &ran);
    assert_eq!(
        h.fakes.store.writes.borrow().as_slice(),
        &[holder()],
        "The record is the holder the start gave"
    );
    assert_eq!(h.record(), Some(holder()));
    assert_eq!(
        h.fakes.store.log_had_started_at_write.borrow().as_slice(),
        &[false],
        "A started line MUST follow its record, never come before it"
    );
    let e = h.events();
    assert!(pos(&e, "start:3f2a9c1e-1111-4222-8333-444455556666") < pos(&e, "store:write"));
    assert_lock_released_last(&e);
}

#[test]
fn at_most_one_holder_is_started_per_pass() {
    let h = Harness::new();
    h.write_leases(vec![
        ready(ID_A, LATER),
        ready(ID_B, LATER),
        ready(ID_C, LATEST),
    ]);
    h.start_echo(4242);
    h.pass(NOW);
    assert_eq!(h.starts(), 1);
    assert_eq!(h.log().lines().count(), 1);
}

#[test]
fn the_start_is_handed_the_state_folder_the_profile_the_ssh_exe_the_lease_and_the_own_program() {
    let h = ready_a();
    h.start_echo(4242);
    h.pass(NOW);
    let seen = h.fakes.starter.requests.borrow();
    assert_eq!(seen.len(), 1);
    let s = &seen[0];
    assert_eq!(s.state_dir, h.state, "The state folder");
    assert_eq!(
        s.profile, h.home,
        "The holder's working folder is the pass's USERPROFILE"
    );
    assert_eq!(s.ssh_exe, h.ssh, "-SshExe");
    assert_eq!(s.lease_id, ID_A);
    assert_eq!(s.alias, alias_of(ID_A));
    assert_eq!(
        s.own_exe, h.own_exe,
        "The keeper's own program, which the launcher runs as"
    );
}

#[test]
fn the_starter_is_handed_the_passs_own_matcher() {
    let h = ready_a();
    h.start_echo(4242);
    h.pass(NOW);
    let seen = h.fakes.starter.requests.borrow();
    assert_eq!(
        seen[0].matcher_command,
        FakeMatcher::command_for(&h.ssh, &alias_of(ID_A))
    );
}

#[test]
fn the_profile_is_userprofile_even_when_the_state_folder_is_elsewhere() {
    let mut h = ready_a();
    let other = h.tmp.path().join("elsewhere-home");
    std::fs::create_dir_all(&other).unwrap();
    h.vars
        .insert("USERPROFILE".into(), other.to_string_lossy().into_owned());
    h.start_echo(4242);
    h.pass(NOW);
    assert_eq!(h.fakes.starter.requests.borrow()[0].profile, other);
    assert_eq!(h.fakes.starter.requests.borrow()[0].state_dir, h.state);
}

#[test]
fn a_start_that_takes_long_still_logs_the_pass_time() {
    let h = ready_a();
    h.set_start(Started::Holder(holder()));
    h.fakes.starter.takes_ms.set(95_000);
    h.pass_on_clock();
    assert_eq!(
        h.log(),
        line(NOW, "started", ID_A, &alias_of(ID_A), "31337", None)
    );
}

// ---- A start that fails ----

#[test]
fn a_failed_start_logs_failed_with_the_chosen_lease_and_no_pid_and_exits_0() {
    let h = ready_a();
    let reason =
        "The holder could not be started: The holder exited at once; see board-holder-ssh.log.";
    h.set_start(Started::Failed(reason.into()));
    let before = h.log_bytes();
    let ran = h.pass(NOW);
    assert_eq!(
        ran.exit, 0,
        "A failed start exits 0: the pass ran and logged what happened"
    );
    assert_eq!(
        h.log(),
        line(NOW, "failed", ID_A, &alias_of(ID_A), "-", Some(reason))
    );
    assert_stdout_is_log_since(&h, &before, &ran);
    assert_eq!(h.record(), None, "Nothing is recorded");
    assert!(!has(&h.events(), "store:write"));
    assert_lock_released_last(&h.events());
}

#[test]
fn the_reason_of_a_failed_start_is_the_text_the_starter_gave_whole() {
    for reason in [
        "The holder could not be started: NUL could not be opened.",
        "The holder could not be started: The launcher did not answer within 35 seconds. Its request was withdrawn, so no holder was started.",
        "The holder could not be started: The launcher did not answer within 35 seconds. A launcher claimed the request, or it could not be withdrawn, and no answer came in 60 more seconds, so a holder may be running unrecorded.",
    ] {
        let h = ready_a();
        h.set_start(Started::Failed(reason.into()));
        h.pass(NOW);
        assert_eq!(
            h.log(),
            line(NOW, "failed", ID_A, &alias_of(ID_A), "-", Some(reason))
        );
    }
}

#[test]
fn a_failed_starts_reason_has_each_run_of_control_characters_written_as_one_space_and_is_trimmed() {
    let h = ready_a();
    h.set_start(Started::Failed(
        "\n  a\r\nb\t\tc\u{7f}d\u{1}\u{2}e  \n".into(),
    ));
    h.pass(NOW);
    assert_eq!(
        h.log(),
        line(NOW, "failed", ID_A, &alias_of(ID_A), "-", Some("a b c d e"))
    );
    assert_eq!(h.log().lines().count(), 1, "The reason stays on one line");
}

#[test]
fn an_empty_reason_is_written_unknown() {
    for empty in ["", "   ", "\n\r\n", "\u{1}\u{7f}"] {
        let h = ready_a();
        h.set_start(Started::Failed(empty.into()));
        h.pass(NOW);
        assert_eq!(
            h.log(),
            line(NOW, "failed", ID_A, &alias_of(ID_A), "-", Some("Unknown.")),
            "for {empty:?}"
        );
    }
}

// ---- A holder that started but could not be recorded ----

fn write_fails(result: Stopped) -> Harness {
    let h = ready_a();
    h.set_start(Started::Holder(holder()));
    *h.fakes.store.write_error.borrow_mut() = Some("disk full".to_string());
    h.set_stop(result);
    h
}

#[test]
fn a_record_that_cannot_be_written_stops_the_holder_at_once_and_says_so() {
    for result in [Stopped::All, Stopped::Gone] {
        let h = write_fails(result.clone());
        let before = h.log_bytes();
        let ran = h.pass(NOW);
        assert_eq!(ran.exit, 0, "{result:?}");
        assert_eq!(
            h.log(),
            line(
                NOW,
                "failed",
                ID_A,
                &alias_of(ID_A),
                "31337",
                Some(
                    "The record could not be written, so the holder was stopped, or had exited: disk full"
                )
            ),
            "{result:?}"
        );
        assert_stdout_is_log_since(&h, &before, &ran);
        assert_eq!(h.stops(), 1);
        let (stopped, ssh) = h.fakes.processes.stops.borrow()[0].clone();
        assert_eq!(
            stopped,
            holder(),
            "The holder the start gave is the one stopped"
        );
        assert_eq!(ssh, h.ssh);
        assert!(
            !h.log().contains(" started "),
            "A started line MUST NOT be logged without a record"
        );
        assert_eq!(h.record(), None);
        assert_lock_released_last(&h.events());
    }
}

#[test]
fn a_holder_that_cannot_be_stopped_after_a_failed_record_is_said_to_run_unrecorded() {
    for result in [
        Stopped::CannotOpenToStop(5),
        Stopped::NotStopped("why".into()),
        Stopped::Error("boom".into()),
        Stopped::StillRunning,
    ] {
        let h = write_fails(result.clone());
        let ran = h.pass(NOW);
        assert_eq!(ran.exit, 0, "{result:?}");
        assert_eq!(
            h.log(),
            line(
                NOW,
                "failed",
                ID_A,
                &alias_of(ID_A),
                "31337",
                Some(
                    "The record could not be written, and the holder could not be stopped, so it runs unrecorded: disk full"
                )
            ),
            "{result:?}"
        );
        assert_eq!(h.stops(), 1);
    }
}

#[test]
fn a_record_that_cannot_be_written_logs_no_started_line_even_when_the_stop_works() {
    let h = write_fails(Stopped::All);
    h.pass(NOW);
    assert_eq!(h.log().lines().count(), 1);
    assert!(h.log().contains(" failed "));
}

// ---- Before the start ----

fn missing_ssh(h: &Harness) -> PathBuf {
    h.tmp.path().join("no-such-folder").join("ssh.exe")
}

#[test]
fn an_ssh_exe_that_is_not_a_file_refuses_the_start() {
    let h = ready_a();
    let ssh = missing_ssh(&h);
    let p = ssh.to_string_lossy().into_owned();
    let before = h.log_bytes();
    let ran = h.run(&["-SshExe", &p, "-Now", NOW]);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        h.log(),
        line(
            NOW,
            "failed",
            ID_A,
            &alias_of(ID_A),
            "-",
            Some(&format!(
                "The holder could not be started: SshExe '{p}' does not exist."
            ))
        )
    );
    assert_stdout_is_log_since(&h, &before, &ran);
    assert_eq!(h.starts(), 0, "A refused start never reaches the starter");
    assert_eq!(h.record(), None);
    assert_lock_released_last(&h.events());
}

#[test]
fn an_ssh_exe_that_is_a_folder_is_not_a_file_either() {
    let h = ready_a();
    let p = h.home.to_string_lossy().into_owned();
    h.run(&["-SshExe", &p, "-Now", NOW]);
    assert_eq!(
        h.log(),
        line(
            NOW,
            "failed",
            ID_A,
            &alias_of(ID_A),
            "-",
            Some(&format!(
                "The holder could not be started: SshExe '{p}' does not exist."
            ))
        )
    );
    assert_eq!(h.starts(), 0);
}

#[test]
fn an_ssh_exe_that_does_not_exist_matters_only_when_a_start_is_decided() {
    // Whether -SshExe exists is the pass's concern, and fails a start; it is no usage error, and
    // an idle pass never looks at it.
    let h = Harness::new();
    let p = missing_ssh(&h).to_string_lossy().into_owned();
    let ran = h.run(&["-SshExe", &p, "-Now", NOW]);
    assert_eq!(ran.exit, 0);
    assert_eq!(ran.stderr, "");
    assert_eq!(h.log(), idle_line(NOW));
}

#[test]
fn a_keeper_program_path_holding_a_quote_refuses_the_start() {
    let mut h = ready_a();
    h.own_exe = h.tmp.path().join("q\"x").join("agentspace-keeper");
    let p = h.own_exe.to_string_lossy().into_owned();
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0);
    assert_eq!(
        h.log(),
        line(
            NOW,
            "failed",
            ID_A,
            &alias_of(ID_A),
            "-",
            Some(&format!(
                "The holder could not be started: This program's path '{p}' holds a quote."
            ))
        )
    );
    assert_eq!(h.starts(), 0);
}

#[cfg(unix)]
#[test]
fn a_state_folder_path_holding_a_quote_refuses_the_start() {
    // A folder name holds a quote on Linux only; Windows cannot make one.
    let mut h = Harness::new();
    let state = h.tmp.path().join("st\"ate");
    std::fs::create_dir_all(&state).unwrap();
    h.vars.insert(
        "AGENTSPACE_STATE_DIR".into(),
        state.to_string_lossy().into_owned(),
    );
    h.state = state.clone();
    h.write_leases(vec![ready(ID_A, LATER)]);
    let p = state.to_string_lossy().into_owned();
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0, "stderr was {:?}", ran.stderr);
    assert_eq!(
        h.log(),
        line(
            NOW,
            "failed",
            ID_A,
            &alias_of(ID_A),
            "-",
            Some(&format!(
                "The holder could not be started: The state folder '{p}' holds a quote."
            ))
        )
    );
    assert_eq!(h.starts(), 0);
}

#[cfg(unix)]
#[test]
fn the_refusals_are_reported_in_the_tables_order() {
    // SshExe, then the state folder, then the program's own path.
    let mut h = Harness::new();
    let state = h.tmp.path().join("st\"ate");
    std::fs::create_dir_all(&state).unwrap();
    h.vars.insert(
        "AGENTSPACE_STATE_DIR".into(),
        state.to_string_lossy().into_owned(),
    );
    h.state = state.clone();
    h.own_exe = h.tmp.path().join("q\"x").join("agentspace-keeper");
    h.write_leases(vec![ready(ID_A, LATER)]);

    // The state folder before the program's path.
    h.pass(NOW);
    let first = h.log();
    assert!(first.contains("The state folder '"), "{first}");
    assert!(!first.contains("This program's path"), "{first}");

    // And -SshExe before both.
    let p = missing_ssh(&h).to_string_lossy().into_owned();
    std::fs::remove_file(h.log_path()).unwrap();
    h.run(&["-SshExe", &p, "-Now", NOW]);
    let second = h.log();
    assert!(
        second.contains(&format!("SshExe '{p}' does not exist.")),
        "{second}"
    );
    assert!(!second.contains("The state folder"), "{second}");
}

#[test]
fn a_missing_ssh_exe_is_reported_before_the_programs_own_path() {
    let mut h = ready_a();
    h.own_exe = h.tmp.path().join("q\"x").join("agentspace-keeper");
    let p = missing_ssh(&h).to_string_lossy().into_owned();
    h.run(&["-SshExe", &p, "-Now", NOW]);
    assert!(
        h.log().contains(&format!("SshExe '{p}' does not exist.")),
        "{}",
        h.log()
    );
    assert!(!h.log().contains("This program's path"));
}

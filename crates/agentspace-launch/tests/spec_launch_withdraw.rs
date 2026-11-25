//! A request nobody answers, and a launcher that claims first (docs/keeper.md, The handshake,
//! steps 2, 3 and 5; Timings of a start).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **The pass withdraws an unanswered request by claiming it itself**, with `abandoned`. If the
//! pass's claim wins, no launcher can claim the request and none starts a holder. If a launcher
//! claimed first, the pass waits on for its answer, up to 60 more seconds. These specs take as
//! long as the spec's bounds do: about 35 seconds, 38 seconds, and 95 seconds, and run side by
//! side.
//!
//! **The launcher in these specs is a program that never claims** (`hostname.exe`, which WMI
//! creates and which ends at once), or the spec's own code, which claims by hand.
#![cfg(windows)]

mod common;

use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use agentspace_keeper_api::Started;
use agentspace_keeper_files::{
    Claim, LaunchResult, parse_claim, parse_launch_request, serialize_launch_result,
};
use common::{
    ALIAS, Captured, Change, FakeMatcher, Fixture, Kind, LEASE, look, parse_ms, running,
    start_watched, stub_args, stub_exe, wait_for,
};

fn never_claims() -> PathBuf {
    let root = std::env::var_os("SystemRoot").expect("Windows has a system root.");
    PathBuf::from(root).join("System32").join("hostname.exe")
}

/// The one request in a state folder: its path, once it is whole.
fn request_in(state: &std::path::Path) -> Option<PathBuf> {
    std::fs::read_dir(state)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("board-holder-launch.")
                    && n.ends_with(".json")
                    && !n.ends_with(".result.json")
            })
        })
}

fn claim_path(request: &std::path::Path) -> PathBuf {
    let name = request.file_name().and_then(|n| n.to_str()).unwrap_or("");
    request.with_file_name(name.trim_end_matches(".json").to_string() + ".claim")
}

fn seconds(a: Instant, b: Instant) -> f64 {
    b.duration_since(a).as_secs_f64()
}

/// Seconds from the moment the pass took its deadline to a change the watcher saw, on the UTC
/// clock. The request's `not_after` is that moment plus 30 seconds, written by the pass itself, so
/// this needs no clock of the test's around the start: the watcher's setup, its teardown and the
/// start's other work are not in it. The watcher takes a change's time when it reads it, never
/// before the change happened, so load can only make this later, never earlier.
fn since_deadline_taken(cap: &Captured, change: &Change) -> f64 {
    let request = parse_launch_request(cap.request.as_ref().expect("The request was read."))
        .expect("The request meets its schema.");
    let not_after = parse_ms(&request.not_after).expect("The deadline is a time.");
    (change.wall_ms - (not_after - 30_000)) as f64 / 1000.0
}

/// The pass's withdrawal claim says `abandoned`, when the watcher could read it. The pass removes
/// its claim at once after it made it, so a watcher held back by load can find it already gone;
/// the reason's last sentence, which each caller checks, is then the pass's own report that its
/// `abandoned` claim was made, since it says so only once that claim's create succeeded.
fn assert_withdrawn_with_abandoned(cap: &Captured) {
    match cap.claim.as_ref() {
        Some(bytes) => assert_eq!(
            parse_claim(bytes),
            Some(Claim::Abandoned),
            "The pass's claim says `abandoned`."
        ),
        None => eprintln!("The pass removed its claim before the watcher could read it."),
    }
}

#[test]
fn an_unanswered_request_is_withdrawn_with_abandoned_at_35_seconds_and_a_late_launcher_starts_nothing()
 {
    let fx = Fixture::new("unanswered");
    let exe = stub_exe();
    let matcher = FakeMatcher::stub("stub_marker");
    let state = fx.state.clone();

    // A launcher that arrives once the pass has withdrawn the request, or while it removes its
    // files: it MUST NOT start a holder.
    // (The deterministic form of this case is in spec_launcher.rs, where the claim is made first.)
    let late = std::thread::spawn(move || {
        let request = wait_for(Duration::from_secs(30), || request_in(&state))?;
        std::thread::sleep(Duration::from_secs(34));
        let end = Instant::now() + Duration::from_secs(5);
        let mut called = false;
        while Instant::now() < end {
            let abandoned = std::fs::read(claim_path(&request))
                .map(|b| b == b"abandoned\n")
                .unwrap_or(false);
            if abandoned {
                let _ = agentspace_launch::run_launcher(&request);
                called = true;
                break;
            }
            std::thread::yield_now();
        }
        Some(called)
    });

    let (out, watch) = start_watched(&fx, &never_claims(), &exe, &matcher, LEASE, ALIAS);
    let late_result = late.join().expect("The late launcher did not panic.");
    assert!(late_result.is_some(), "The request was seen.");
    eprintln!(
        "The late launcher {} the pass's claim.",
        if late_result == Some(true) {
            "reached"
        } else {
            "missed"
        }
    );

    assert_eq!(
        out,
        Started::Failed(
            "The holder could not be started: The launcher did not answer within 35 seconds. \
             Its request was withdrawn, so no holder was started."
                .to_string()
        ),
        "Step 5: the reason, with the withdrawal."
    );

    let changes = watch.changes();
    let first_four: Vec<(Kind, bool)> = changes.iter().take(4).map(|c| (c.kind, c.added)).collect();
    assert_eq!(
        first_four,
        vec![
            (Kind::Request, true),
            (Kind::Claim, true),
            (Kind::Request, false),
            (Kind::Claim, false),
        ],
        "The request, the pass's claim, then the request's removal and the claim's."
    );
    assert!(
        changes.iter().all(|c| c.kind != Kind::Result),
        "No launcher answered, and the late one started nothing, so no result was ever written."
    );
    let cap = &watch.captured()[0];
    // When the 35 seconds are up, and not before: the lower bound is exact, since load can only
    // make the watcher's time later. The upper bound leaves 3 seconds for the pass's last 100 ms
    // look and for a loaded laptop's scheduling, and still tells this wait from any longer one:
    // the 60 more seconds of a claimed request, or 35 seconds counted from a create call that
    // took 3 seconds or more instead of from the request.
    let withdrawn = since_deadline_taken(cap, &changes[1]);
    assert!(
        (34.95..=38.0).contains(&withdrawn),
        "The withdrawal comes 35 seconds after the deadline was taken: {withdrawn:.2} seconds."
    );
    assert_withdrawn_with_abandoned(cap);
    assert!(
        !fx.profile.join("spec-starts.txt").exists(),
        "The late launcher started no holder."
    );
    assert!(fx.launch_files().is_empty(), "All three files are gone.");
}

#[test]
fn a_launcher_that_claimed_first_and_never_answers_holds_the_pass_for_60_more_seconds() {
    let fx = Fixture::new("claimed-silent");
    let exe = stub_exe();
    let matcher = FakeMatcher::stub("stub_marker");
    let state = fx.state.clone();

    // A launcher of the spec's own: it claims the request at once, and never answers.
    let claimant = std::thread::spawn(move || {
        let request = wait_for(Duration::from_secs(30), || request_in(&state))?;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(1)
            .open(claim_path(&request))
            .ok()?;
        f.write_all(b"launcher 4242\n").ok()?;
        Some(())
    });

    let started = Instant::now();
    let (out, watch) = start_watched(&fx, &never_claims(), &exe, &matcher, LEASE, ALIAS);
    let elapsed = started.elapsed().as_secs_f64();
    assert!(
        claimant.join().expect("No panic.").is_some(),
        "The spec's launcher claimed the request."
    );

    assert_eq!(
        out,
        Started::Failed(
            "The holder could not be started: The launcher did not answer within 35 seconds. \
             A launcher claimed the request, or it could not be withdrawn, and no answer came \
             in 60 more seconds, so a holder may be running unrecorded."
                .to_string()
        ),
        "Step 5: the pass lost the withdrawal, waited on, and says a holder may run unrecorded."
    );
    assert!(
        (93.0..=105.0).contains(&elapsed),
        "35 seconds, then 60 more, counted from the first try to withdraw: {elapsed:.1} seconds."
    );
    let cap = &watch.captured()[0];
    assert_eq!(
        parse_claim(cap.claim.as_ref().expect("The claim was read.")),
        Some(Claim::Launcher(4242)),
        "The launcher's claim stood: the pass's withdrawal did not replace it."
    );
    let kinds: Vec<(Kind, bool)> = watch.changes().iter().map(|c| (c.kind, c.added)).collect();
    assert_eq!(
        kinds,
        vec![
            (Kind::Request, true),
            (Kind::Claim, true),
            (Kind::Request, false),
            (Kind::Claim, false),
        ],
        "The pass still removes what it can, the request first, then the claim, whatever happened."
    );
    assert!(fx.launch_files().is_empty());
}

#[test]
fn a_launcher_that_claimed_first_and_answers_late_has_its_holder_recorded() {
    let fx = Fixture::new("claimed-late");
    let exe = stub_exe();
    let matcher = FakeMatcher::stub("stub_sleep");
    let state = fx.state.clone();
    let profile = fx.profile.clone();
    let reaper = fx.reaper.clone();
    let exe2 = exe.clone();

    // A launcher of the spec's own: it claims at once, then answers 38 seconds after the request,
    // past the 35 seconds of the first wait, with a holder it started itself.
    let claimant = std::thread::spawn(move || {
        let request = wait_for(Duration::from_secs(30), || request_in(&state))?;
        let seen = Instant::now();
        let bytes = std::fs::read(&request).ok()?;
        let parsed = parse_launch_request(&bytes).ok()?;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(claim_path(&request))
            .ok()?;
        f.write_all(b"launcher 4242\n").ok()?;
        drop(f);
        std::thread::sleep(Duration::from_secs(38).saturating_sub(seen.elapsed()));
        let child = Command::new(&exe2)
            .raw_arg(stub_args("stub_sleep"))
            .current_dir(&profile)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let pid = child.id();
        let created = wait_for(Duration::from_secs(5), || look(pid).map(|(s, _)| s))?;
        reaper.track(pid, &created);
        reaper.track_child(child);
        let line = format!("\"{}\" {}", exe2.display(), stub_args("stub_sleep"));
        let answer = serialize_launch_result(&LaunchResult::Started {
            pid,
            process_started_at: created.clone(),
            command_line: Some(line),
        })
        .ok()?;
        let result = PathBuf::from(&parsed.result);
        let tmp = result.with_extension("json.spec.tmp");
        std::fs::write(&tmp, answer).ok()?;
        std::fs::rename(&tmp, &result).ok()?;
        Some((pid, created))
    });

    let started = Instant::now();
    let (out, _watch) = start_watched(&fx, &never_claims(), &exe, &matcher, LEASE, ALIAS);
    let elapsed = started.elapsed().as_secs_f64();
    let (pid, created) = claimant
        .join()
        .expect("No panic.")
        .expect("The spec's launcher answered.");

    match out {
        Started::Holder(rec) => {
            assert_eq!(
                rec.pid, pid,
                "The pass records the holder the late answer names."
            );
            assert_eq!(rec.process_started_at, created);
            assert_eq!(rec.lease_id, LEASE);
            assert_eq!(rec.alias, ALIAS);
        }
        Started::Failed(why) => panic!("The late answer was not taken: {why}"),
    }
    assert!(
        (37.5..=50.0).contains(&elapsed),
        "The pass waited on past 35 seconds for the launcher that claimed first: {elapsed:.1} seconds."
    );
    assert!(running(pid, &created), "The holder runs.");
    assert!(fx.launch_files().is_empty(), "All three files are gone.");
}

#[test]
fn a_create_that_fails_does_not_wait_for_an_answer_and_withdraws_the_request_at_once() {
    let fx = Fixture::new("create-fails");
    let exe = stub_exe();
    let matcher = FakeMatcher::stub("stub_marker");
    let nowhere = fx.root.join("no-such-launcher").join("agentspace.exe");

    let (out, watch) = start_watched(&fx, &nowhere, &exe, &matcher, LEASE, ALIAS);
    let why = match out {
        Started::Failed(why) => why,
        Started::Holder(_) => panic!("A launcher that is not there started a holder."),
    };
    assert!(
        why.starts_with("The holder could not be started: "),
        "The reason starts as keeper.md's does."
    );
    assert!(
        why.contains("Win32_Process.Create"),
        "The reason names the create call that failed."
    );
    assert!(
        why.ends_with("Its request was withdrawn, so no holder was started."),
        "The request was withdrawn by the pass's claim."
    );
    let changes = watch.changes();
    let kinds: Vec<(Kind, bool)> = changes.iter().map(|c| (c.kind, c.added)).collect();
    assert_eq!(
        kinds,
        vec![
            (Kind::Request, true),
            (Kind::Claim, true),
            (Kind::Request, false),
            (Kind::Claim, false),
        ]
    );
    // No wait for an answer: a pass that waited would withdraw 35 seconds after the request at
    // the earliest. This one withdraws when its create tries end: one call for a launcher that
    // is not there, which WMI answers with a code, or at most three tries of 10 seconds and two
    // gaps of 2, 34 seconds. The two times are the watcher's, taken as it read each change, so
    // neither the watcher's setup and teardown nor the starter's own work after the withdrawal
    // is in the gap; a bound on the whole start counted those, which load stretches by seconds.
    let gap = seconds(changes[0].at, changes[1].at);
    assert!(
        gap < 35.0,
        "No wait for an answer: the withdrawal came {gap:.1} seconds after the request."
    );
    assert_withdrawn_with_abandoned(&watch.captured()[0]);
    assert!(fx.launch_files().is_empty());
}

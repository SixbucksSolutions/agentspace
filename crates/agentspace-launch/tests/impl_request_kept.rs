//! The implementer's test of step 8 for real (docs/keeper.md, The handshake, step 8; architect,
//! 2026-11-19, for E0.06-T02-S04): when the request cannot be removed, its claim MUST stay, so a
//! late launcher cannot claim it and start a holder no pass records. The start is the real one:
//! the real starter, WMI and the real launcher. Step 8 is tested with a fake system in
//! src/handshake.rs.
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
#![cfg(windows)]

mod common;

use agentspace_keeper_api::Started;
use agentspace_keeper_files::{Claim, parse_claim};
use common::{
    ALIAS, FakeMatcher, Fixture, LEASE, read_text, start_with, stub_exe, testhost, wait_for,
};
use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// `FILE_SHARE_READ | FILE_SHARE_WRITE`, and not `FILE_SHARE_DELETE`: while this hold is open,
/// the launcher can read the request, and nobody can remove it.
const SHARE_NO_DELETE: u32 = 0x1 | 0x2;

/// Whether a name in the state folder is a launch request, not its result.
fn is_request(name: &str) -> bool {
    name.starts_with("board-holder-launch.")
        && name.ends_with(".json")
        && !name.ends_with(".result.json")
}

/// Opens the start's request, as soon as it appears, with a share mode that refuses its removal,
/// and gives the hold and its path. It looks every millisecond until `stop` is set or 60 seconds
/// pass.
fn hold_the_request(state: PathBuf, stop: Arc<AtomicBool>) -> Option<(File, PathBuf)> {
    let began = Instant::now();
    while !stop.load(Ordering::SeqCst) && began.elapsed() < Duration::from_secs(60) {
        let names = fs::read_dir(&state)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for name in names.iter().filter(|n| is_request(n)) {
            let path = state.join(name);
            if let Ok(file) = OpenOptions::new()
                .read(true)
                .share_mode(SHARE_NO_DELETE)
                .open(&path)
            {
                return Some((file, path));
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    None
}

#[test]
fn a_request_the_pass_cannot_remove_keeps_its_claim_and_a_late_launcher_starts_nothing() {
    let fx = Fixture::new("kept");
    let stop = Arc::new(AtomicBool::new(false));
    let holder = {
        let state = fx.state.clone();
        let stop = stop.clone();
        std::thread::spawn(move || hold_the_request(state, stop))
    };
    let out = start_with(
        &fx,
        &testhost(),
        &stub_exe(),
        &FakeMatcher::stub("stub_sleep"),
        LEASE,
        ALIAS,
    );
    stop.store(true, Ordering::SeqCst);
    let (hold, request) = holder
        .join()
        .unwrap()
        .expect("The test held the request before the pass removed it.");

    // The start itself is unchanged: the holder is returned, and `start_with` has the fixture end
    // it when the test ends.
    assert!(
        matches!(&out, Started::Holder(_)),
        "The start failed: {out:?}"
    );

    // The request stays, its claim stays with it, and the result alone is gone.
    let name = request.file_name().unwrap().to_string_lossy().into_owned();
    let hex = name
        .strip_prefix("board-holder-launch.")
        .and_then(|n| n.strip_suffix(".json"))
        .unwrap()
        .to_string();
    let claim = fx.state.join(agentspace_keeper_files::claim_file(&hex));
    let result = fx.state.join(agentspace_keeper_files::result_file(&hex));
    assert!(request.exists(), "The request was removed.");
    assert!(
        claim.exists(),
        "The claim of a request that stays was removed."
    );
    assert!(!result.exists(), "The result was not removed.");
    let before = fs::read(&claim).unwrap();
    assert!(matches!(parse_claim(&before), Some(Claim::Launcher(_))));
    assert!(
        wait_for(Duration::from_secs(10), || read_text(&fx.log())
            .contains("stub-start")
            .then_some(()))
        .is_some(),
        "The holder wrote nothing to its log."
    );
    assert_eq!(read_text(&fx.log()).matches("stub-start").count(), 1);

    // A launcher that comes late, as from a create call that was abandoned, cannot claim the
    // request, so it starts nothing and writes nothing.
    assert_eq!(agentspace_launch::run_launcher(&request), 0);
    assert_eq!(fs::read(&claim).unwrap(), before, "The claim was changed.");
    std::thread::sleep(Duration::from_millis(1500));
    assert!(!result.exists(), "The late launcher answered.");
    assert_eq!(
        read_text(&fx.log()).matches("stub-start").count(),
        1,
        "The late launcher started a holder."
    );
    drop(hold);
}

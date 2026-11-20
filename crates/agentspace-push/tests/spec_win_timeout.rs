//! The push's 20-second limit, on Windows: a hung ssh, its whole tree, and nothing else
//! (docs/operator.md, What the laptop sends, steps 3 to 5).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The stand-in hangs for 90 seconds, so only the push's limit can end it. The processes it starts
//! record their PIDs beside it, and a sibling that the test starts itself, and hands to nobody,
//! shows that only the ssh's tree is ended. A rig ends whatever the stand-in left when it drops.
#![cfg(windows)]

mod win_support;

use std::time::Duration;

use win_support::*;

const LIMIT_LOW: f64 = 19.0;
const LIMIT_HIGH: f64 = 26.0;

#[test]
fn a_hung_ssh_is_stopped_at_20_seconds_with_its_whole_tree_and_only_that_tree() {
    let mut rig = Rig::new();
    let sibling = rig.spawn_sibling();
    assert!(process_alive(sibling), "The sibling did not start");
    rig.scenario("hangtree");

    let say = FakeSay::person();
    let took = rig.push(&say, DEFAULT_ALIAS, false);

    // The warning, and nothing recorded.
    assert_warnings(&say, &[TOOK_TOO_LONG], "A hung ssh");
    assert_eq!(say.progress_lines(), vec![SENDING.to_string()]);
    assert!(rig.stamp_bytes().is_none(), "A stamp was written");

    // The clock: stopped at the limit, and not after the 10 seconds that apply only to an ssh that
    // exited (step 5: never after a kill).
    let secs = took.as_secs_f64();
    assert!(
        secs >= LIMIT_LOW,
        "The push gave up before the limit, after {secs:.1} seconds"
    );
    assert!(
        secs <= LIMIT_HIGH,
        "The push took {secs:.1} seconds: the hung ssh was not stopped at the limit"
    );

    // The content was written and stdin closed inside the clock, so the stand-in read it all.
    assert!(
        without_bom(&rig.stdin(1)) == rig.content().as_bytes(),
        "The hung stand-in did not receive the content"
    );

    // The whole tree is gone: the stand-in's script host, and the child it started itself.
    let tree: Vec<Proc> = rig
        .procs_tagged("ssh")
        .into_iter()
        .chain(rig.procs_tagged("child"))
        .collect();
    assert_eq!(tree.len(), 2, "The stand-in did not record its tree");
    assert!(
        wait_all_gone(&tree, Duration::from_secs(3)),
        "A process of the ssh's tree is still running"
    );

    // Only that tree: the test's own sibling survives.
    assert!(
        process_alive(sibling),
        "A process that is no part of the ssh's tree was ended"
    );

    // Nothing was recorded, so the next push is due and goes through.
    rig.scenario("ok");
    let again = rig.up();
    assert_warnings(&again, &[], "The push after a hang");
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn an_ssh_that_never_reads_its_stdin_is_cut_off_by_the_same_limit() {
    let mut rig = Rig::new();
    // A URL far larger than a pipe's buffer, so that the write itself blocks.
    rig.base = format!("http://127.0.0.1:18811/{}", "a".repeat(1_048_576));
    rig.write_receipt();
    rig.scenario("noread");

    let say = FakeSay::person();
    let took = rig.push(&say, DEFAULT_ALIAS, false);

    assert_warnings(&say, &[TOOK_TOO_LONG], "A write that blocks");
    let secs = took.as_secs_f64();
    assert!(
        secs >= LIMIT_LOW,
        "The push gave up before the limit, after {secs:.1} seconds"
    );
    assert!(
        secs <= LIMIT_HIGH,
        "The push took {secs:.1} seconds: a blocked write was not cut off at the limit"
    );
    assert!(rig.stamp_bytes().is_none(), "A stamp was written");
    let tree = rig.procs_tagged("ssh");
    assert_eq!(tree.len(), 1);
    assert!(
        wait_all_gone(&tree, Duration::from_secs(3)),
        "The ssh is still running"
    );
}

#[test]
fn a_hung_ssh_with_no_child_is_stopped_too_and_a_sibling_survives() {
    let mut rig = Rig::new();
    let sibling = rig.spawn_sibling();
    rig.scenario("hang");
    let say = FakeSay::person();
    let took = rig.push(&say, DEFAULT_ALIAS, false);
    assert_warnings(&say, &[TOOK_TOO_LONG], "A hung ssh");
    assert!(took.as_secs_f64() >= LIMIT_LOW && took.as_secs_f64() <= LIMIT_HIGH);
    assert!(wait_all_gone(
        &rig.procs_tagged("ssh"),
        Duration::from_secs(3)
    ));
    assert!(
        process_alive(sibling),
        "A process outside the tree was ended"
    );
}

#[test]
fn an_ssh_that_exits_but_leaves_a_child_holding_stdout_is_waited_for_at_most_10_seconds() {
    let rig = Rig::new();
    rig.scenario("holdstdout");
    let say = FakeSay::person();
    let took = rig.push(&say, DEFAULT_ALIAS, false);

    // The ssh exited 0 on its own and printed nothing: no confirmation, not the time-out warning.
    assert_warnings(
        &say,
        &[NOT_CONFIRMED],
        "An ssh that exited and left stdout open",
    );
    let secs = took.as_secs_f64();
    assert!(
        secs < 19.0,
        "The push took {secs:.1} seconds: the wait for stdout is bounded at 10 seconds"
    );
    assert!(rig.stamp_bytes().is_none(), "A stamp was written");
}

fn without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes)
}

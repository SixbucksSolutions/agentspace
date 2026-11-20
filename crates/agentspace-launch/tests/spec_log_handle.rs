//! The holder's log handle is shared (docs/keeper.md, Starting a holder, item 2; docs/state.md, The
//! keeper's logs).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **Item 2: stdout and stderr are one handle that appends to `board-holder-ssh.log`**, opened
//! sharing read, write and delete, so that a later holder, a rotation or a reader is never refused
//! while an earlier holder still has it open. Every holder here is started through the real
//! starter and launcher, WMI included.
#![cfg(windows)]

mod common;

use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::time::Duration;

use agentspace_keeper_api::Started;
use common::{
    FakeMatcher, Fixture, holder_of, read_text, running, start_stub, start_with, stub_exe,
    testhost, wait_for,
};

fn tick_numbers(text: &str) -> Vec<u32> {
    text.lines()
        .filter_map(|l| l.strip_prefix("tick ")?.parse().ok())
        .collect()
}

#[test]
fn the_holders_stdout_and_stderr_land_in_the_log_in_order_and_the_old_content_stays() {
    let fx = Fixture::new("log-lands");
    std::fs::write(fx.log(), b"OLD LINE\n").expect("The log can be made first.");

    let holder = holder_of(start_stub(&fx, "stub_chatty"));
    let log = fx.log();
    let text = wait_for(Duration::from_secs(20), || {
        let t = read_text(&log);
        (tick_numbers(&t).len() >= 5 && t.contains("err 4")).then_some(t)
    })
    .expect("The holder's output reaches the log while it runs.");

    assert!(
        text.starts_with("OLD LINE\n"),
        "The log is appended to, never truncated or overwritten from the start."
    );
    assert!(
        text.contains(&format!("stub-start pid={}", holder.pid)),
        "The holder's own first line, with its own PID, is in the log."
    );
    // Stdout and stderr are one handle, so each `err N` comes straight after its `tick N`.
    let lines: Vec<&str> = text.lines().collect();
    let mut pairs = 0;
    for (i, l) in lines.iter().enumerate() {
        if let Some(n) = l.strip_prefix("tick ")
            && let Some(next) = lines.get(i + 1)
        {
            assert_eq!(
                *next,
                format!("err {n}"),
                "Stdout and stderr are one handle, so a line of each comes in order."
            );
            pairs += 1;
        }
    }
    assert!(pairs >= 4, "At least four pairs of lines were checked.");
}

#[test]
fn the_log_can_be_read_appended_to_and_rotated_while_the_holder_runs() {
    let fx = Fixture::new("log-shared");
    let holder = holder_of(start_stub(&fx, "stub_chatty"));
    let log = fx.log();
    let before = wait_for(Duration::from_secs(20), || {
        let t = read_text(&log);
        (tick_numbers(&t).len() >= 3).then_some(t)
    })
    .expect("The holder writes to the log.");
    let seen_before = tick_numbers(&before).len();

    // A reader is never refused: the read above worked while the holder had the log open. A
    // second reader that holds the file open with the widest sharing keeps working, too.
    let reader = OpenOptions::new()
        .read(true)
        .share_mode(7)
        .open(&log)
        .expect("A reader sharing everything is not refused.");

    // A writer is never refused, and appends after what the holder wrote.
    {
        let mut w = OpenOptions::new()
            .append(true)
            .share_mode(7)
            .open(&log)
            .expect("A second appender is not refused.");
        w.write_all(b"TEST-APPEND\n").expect("The append works.");
    }
    let after_append = wait_for(Duration::from_secs(10), || {
        let t = read_text(&log);
        let at = t.find("TEST-APPEND\n")?;
        let later = tick_numbers(&t[at..]);
        (!later.is_empty()).then_some(t)
    })
    .expect("The holder's output continues after what the test appended, so its handle appends.");
    assert_eq!(
        after_append.matches("TEST-APPEND\n").count(),
        1,
        "What the test appended was not overwritten by the holder's later writes."
    );

    // A rotation renames the log while the holder still writes to it (docs/state.md).
    let rotated = fx.state.join("board-holder-ssh.log.1");
    drop(reader);
    std::fs::rename(&log, &rotated).expect("The log can be renamed while the holder has it open.");
    let rotated_text = wait_for(Duration::from_secs(10), || {
        let t = read_text(&rotated);
        (tick_numbers(&t).len() > seen_before + 2).then_some(t)
    });
    assert!(
        rotated_text.is_some(),
        "The holder goes on writing to the renamed file."
    );
    assert!(
        running(holder.pid, &holder.process_started_at),
        "The holder still runs after the rotation."
    );
}

#[test]
fn a_later_holder_is_not_refused_while_an_earlier_one_still_has_the_log() {
    let fx = Fixture::new("log-later");
    let first = holder_of(start_stub(&fx, "stub_chatty"));
    assert!(
        wait_for(Duration::from_secs(20), || {
            tick_numbers(&read_text(&fx.log()))
                .len()
                .ge(&2)
                .then_some(())
        })
        .is_some(),
        "The first holder writes to the log."
    );

    // A later holder starts into the same log while the first still has it open.
    let exe = stub_exe();
    let matcher = FakeMatcher::stub("stub_exit");
    let second_started = start_with(
        &fx,
        &testhost(),
        &exe,
        &matcher,
        "0a1b2c3d-0000-4000-8000-000000000002",
        "agentspace-spec-second",
    );
    // The second holder is a stand-in that exits at once; the start may report that, which is a
    // result of item 7, never a refusal of the log.
    match &second_started {
        Started::Holder(_) => {}
        Started::Failed(why) => assert!(
            why.contains("exited at once"),
            "A later holder was refused for a reason other than its own exit: {why}"
        ),
    }
    let text = wait_for(Duration::from_secs(10), || {
        let t = read_text(&fx.log());
        (t.matches("stub-start pid=").count() >= 2).then_some(t)
    })
    .expect("Both holders' first lines reached the one log.");
    assert!(
        text.contains(&format!("stub-start pid={}", first.pid)),
        "The first holder's line is in the log."
    );
    assert!(
        running(first.pid, &first.process_started_at),
        "The first holder was not disturbed."
    );
}

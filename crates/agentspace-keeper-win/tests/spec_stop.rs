//! `Processes::stop` on Windows (docs/keeper.md, Stopping a holder).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Each test builds a real tree of processes of its own: the holder, a middle process under it
//! and two leaves under that, all roles of this test program. The test keeps its own handle to
//! each, so it can tell from the kernel whether a process runs and when it exited, and ends what
//! is left when it finishes. A fake matcher stands in for the holder command's matcher.
//!
//! Not covered, because a test cannot arrange it with processes it made: a descendant created
//! before its parent (a child cannot be made earlier than the parent it is created under, and a
//! parent PID can only be reused by luck), a descendant that survives its termination, a holder
//! that cannot be opened to terminate it (that needs another user's or elevated process), and a
//! child made during the stop (a race the test cannot pin down).
#![cfg(windows)]

mod common;

use std::time::Duration;

use agentspace_keeper_api::{HolderRecord, Processes, Stopped};
use common::*;

/// The entry the roles run; with no role in the environment it does nothing.
#[test]
fn role_entry() {
    role_main();
}

fn all_run(tree: &Tree) -> bool {
    tree.all().iter().all(|p| p.is_running())
}

#[test]
fn stop_ends_the_whole_tree_with_the_leaves_first_and_the_holder_last() {
    let sb = Sandbox::new("stop-tree");
    let tree = start_tree(&sb);
    let record = record_of(&tree.holder);
    let matcher = FakeMatcher::new(true);
    let ssh = fake_ssh();

    let result = processes().stop(&record, &ssh, &matcher);

    assert_eq!(result, Stopped::All, "A holder and its whole tree stopped");
    // keeper.md: the stop waits for each process to exit, so all are gone when it returns.
    for p in tree.all() {
        assert!(
            p.wait_exit(Duration::from_secs(5)),
            "Every process of the tree is gone"
        );
    }
    // The order, as far as the kernel's exit times show it: "The tree is terminated leaves first,
    // the holder last." Two exits in one timer tick tie, and a tie is not a wrong order.
    let (holder, mid) = (tree.holder.exit_ticks(), tree.mid.exit_ticks());
    let (a, b) = (tree.leaf_a.exit_ticks(), tree.leaf_b.exit_ticks());
    assert!(
        a <= mid && b <= mid,
        "The leaves end before the process above them"
    );
    assert!(mid <= holder, "The holder ends last");
    // The holder is the test's own child: it is reaped as an ended process.
    let mut tree = tree;
    let status = tree
        .holder_child
        .child
        .try_wait()
        .expect("The child can be asked");
    assert!(status.is_some(), "The holder ended");
}

#[test]
fn stop_checks_the_holder_with_the_matcher_for_the_ssh_program_and_alias_it_was_given() {
    let sb = Sandbox::new("stop-matcher-args");
    let tree = start_tree(&sb);
    let record = record_of(&tree.holder);
    let matcher = FakeMatcher::new(true);
    let ssh = fake_ssh();

    let result = processes().stop(&record, &ssh, &matcher);

    assert_eq!(result, Stopped::All);
    let seen = matcher.seen.borrow();
    assert!(
        !seen.is_empty(),
        "The stop asks the matcher about the holder's command line"
    );
    for (line, asked_ssh, asked_alias) in seen.iter() {
        assert!(
            line.contains("role_entry"),
            "The matcher is shown the process's command line"
        );
        assert_eq!(
            asked_ssh, &ssh,
            "The matcher gets the ssh program it was given"
        );
        assert_eq!(asked_alias, ALIAS, "The matcher gets the record's alias");
    }
}

#[test]
fn stop_leaves_a_process_outside_the_tree_alone_though_its_command_line_is_the_same() {
    // keeper.md: "Nothing is ever matched by command line alone, and nothing is stopped by a
    // sweep of the process list." The decoy has the holder's very command line.
    let sb = Sandbox::new("stop-decoy");
    let tree = start_tree(&sb);
    let decoy = spawn_role(&sb, "sleep", "decoy");
    wait_for_pid(&sb, "decoy");
    let decoy_proc = Proc::open(decoy.pid());
    let record = record_of(&tree.holder);

    let result = processes().stop(&record, &fake_ssh(), &FakeMatcher::new(true));

    assert_eq!(result, Stopped::All);
    assert!(!tree.holder.is_running());
    assert!(
        decoy_proc.is_running(),
        "A process outside the tree is left running"
    );
}

#[test]
fn stop_with_a_matcher_that_says_no_stops_nothing() {
    // keeper.md: the stop checks the holder's command line again on its handle, "by the same
    // rule"; a holder that is not the holder command is left running and untouched. The pinned
    // doc comment on `Stopped::Gone` covers it: "a new look finds the holder ... no longer the
    // recorded one".
    let sb = Sandbox::new("stop-no-match");
    let tree = start_tree(&sb);
    let record = record_of(&tree.holder);
    let matcher = FakeMatcher::new(false);

    let result = processes().stop(&record, &fake_ssh(), &matcher);

    assert_eq!(
        result,
        Stopped::Gone,
        "Nothing stopped, and a new look finds the process no longer the recorded one"
    );
    assert!(
        !matcher.seen.borrow().is_empty(),
        "The matcher decided, and was asked"
    );
    std::thread::sleep(Duration::from_millis(500));
    assert!(all_run(&tree), "Not one process of the tree was touched");
}

#[test]
fn stop_with_a_start_time_that_does_not_match_leaves_the_process_alone() {
    // keeper.md: "The PID with the start time is the holder's identity, and defeats PID reuse."
    let sb = Sandbox::new("stop-reuse");
    let tree = start_tree(&sb);
    let good = record_of(&tree.holder);

    let mut last_digit = good.process_started_at.clone().into_bytes();
    let at = last_digit.len() - 2;
    last_digit[at] = if last_digit[at] == b'9' {
        b'0'
    } else {
        last_digit[at] + 1
    };
    let one_ms_off = String::from_utf8(last_digit).expect("Digits stay text");

    let other_starts = [
        one_ms_off,
        "2000-01-01T00:00:00.000Z".to_string(),
        // The right time with the zone dropped: not the recorded text.
        good.process_started_at.trim_end_matches('Z').to_string(),
    ];
    for other in other_starts {
        let record = HolderRecord {
            process_started_at: other,
            ..good.clone()
        };

        let result = processes().stop(&record, &fake_ssh(), &FakeMatcher::new(true));

        assert_eq!(
            result,
            Stopped::Gone,
            "A record for another start of the PID is no longer the recorded holder"
        );
        assert!(
            all_run(&tree),
            "A process with another start time is left running"
        );
    }
}

#[test]
fn stop_of_a_holder_that_has_gone_says_gone() {
    let sb = Sandbox::new("stop-gone");
    let mut spawned = spawn_role(&sb, "sleep", "one");
    let pid = spawned.pid();
    let proc = Proc::open(pid);
    wait_for_pid(&sb, "one");
    let record = record_of(&proc);
    proc.terminate();
    assert!(proc.wait_exit(Duration::from_secs(5)));
    let _ = spawned.child.wait();
    drop(proc);
    drop(spawned);

    let result = processes().stop(&record, &fake_ssh(), &FakeMatcher::new(true));

    assert_eq!(result, Stopped::Gone);
}

#[test]
fn stop_does_not_take_in_a_descendant_whose_parent_died_before_the_walk() {
    // keeper.md: "A process whose parent died before the walk cannot be traced, and is left
    // alone." The middle process is ended first, so the leaves still name it as their parent,
    // and it is not the holder's child any more.
    let sb = Sandbox::new("stop-orphans");
    let tree = start_tree(&sb);
    tree.mid.terminate();
    assert!(
        tree.mid.wait_exit(Duration::from_secs(5)),
        "The middle process ends"
    );
    let record = record_of(&tree.holder);

    let result = processes().stop(&record, &fake_ssh(), &FakeMatcher::new(true));

    assert_eq!(
        result,
        Stopped::All,
        "The holder and every descendant the stop could check stopped"
    );
    assert!(!tree.holder.is_running(), "The holder stopped");
    assert!(
        tree.leaf_a.is_running(),
        "An orphan that cannot be traced is left running"
    );
    assert!(
        tree.leaf_b.is_running(),
        "An orphan that cannot be traced is left running"
    );
}

#[test]
fn stop_of_a_holder_with_no_children_stops_it() {
    let sb = Sandbox::new("stop-alone");
    let spawned = spawn_role(&sb, "sleep", "one");
    let proc = Proc::open(spawned.pid());
    wait_for_pid(&sb, "one");
    let record = record_of(&proc);

    let result = processes().stop(&record, &fake_ssh(), &FakeMatcher::new(true));

    assert_eq!(result, Stopped::All);
    assert!(proc.wait_exit(Duration::from_secs(5)), "The holder is gone");
}

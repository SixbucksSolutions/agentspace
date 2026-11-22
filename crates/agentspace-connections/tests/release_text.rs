//! The text of a release's line, from the two walks' results (docs/driver.md, "Where the count is
//! logged" and "The count"), over made-up results.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. These tests are pure: they start no
//! process and read no file. Every identity and error number is made up.

use agentspace_connections::line::release_text;
use agentspace_connections::{EndError, Identity, Result};

fn id(pid: u32, created: u64) -> Identity {
    Identity { pid, created }
}

fn not_all(ended: usize, not_ended: Vec<Identity>) -> Result<usize> {
    Err(EndError::NotAllEnded { ended, not_ended })
}

fn unreadable(windows_error: u32) -> Result<usize> {
    Err(EndError::ListUnreadable { windows_error })
}

// ---- The table's five rows ---------------------------------------------------------------------

#[test]
fn both_read_and_none_could_not_be_ended() {
    assert_eq!(
        release_text(&Ok(3), &Ok(0)),
        "Ended 3 of the lease's connections."
    );
}

#[test]
fn both_read_and_the_ended_counts_are_summed() {
    assert_eq!(
        release_text(&Ok(2), &Ok(1)),
        "Ended 3 of the lease's connections."
    );
}

#[test]
fn ended_zero_is_always_there() {
    assert_eq!(
        release_text(&Ok(0), &Ok(0)),
        "Ended 0 of the lease's connections."
    );
}

#[test]
fn both_read_and_some_could_not_be_ended() {
    assert_eq!(
        release_text(&Ok(0), &not_all(2, vec![id(100, 1), id(101, 2)])),
        "Ended 2 of the lease's connections; 2 could not be ended."
    );
}

#[test]
fn one_read_and_the_other_could_not_and_none_could_not_be_ended() {
    assert_eq!(
        release_text(&Ok(2), &unreadable(5)),
        "Ended 2 of the lease's connections; a look failed: Windows error 5."
    );
    assert_eq!(
        release_text(&unreadable(5), &Ok(2)),
        "Ended 2 of the lease's connections; a look failed: Windows error 5."
    );
}

#[test]
fn one_read_and_the_other_could_not_and_some_could_not_be_ended() {
    // The example of driver.md: the second walk could not read the list, after a first that ended
    // 3 clients and could not end 1.
    assert_eq!(
        release_text(&not_all(3, vec![id(100, 1)]), &unreadable(5)),
        "Ended 3 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
    );
    assert_eq!(
        release_text(&unreadable(5), &not_all(3, vec![id(100, 1)])),
        "Ended 3 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
    );
}

#[test]
fn neither_could_read_the_list() {
    assert_eq!(
        release_text(&unreadable(5), &unreadable(5)),
        "The lease's connections could not be looked for: Windows error 5."
    );
}

#[test]
fn when_both_could_not_read_the_error_is_the_first_walks() {
    assert_eq!(
        release_text(&unreadable(5), &unreadable(1722)),
        "The lease's connections could not be looked for: Windows error 5."
    );
    assert_eq!(
        release_text(&unreadable(1722), &unreadable(5)),
        "The lease's connections could not be looked for: Windows error 1722."
    );
}

#[test]
fn the_error_of_a_single_failed_walk_is_that_walks() {
    assert_eq!(
        release_text(&Ok(1), &unreadable(1722)),
        "Ended 1 of the lease's connections; a look failed: Windows error 1722."
    );
    assert_eq!(
        release_text(&unreadable(1722), &Ok(1)),
        "Ended 1 of the lease's connections; a look failed: Windows error 1722."
    );
}

#[test]
fn a_walk_that_could_not_read_the_list_adds_nothing() {
    assert_eq!(
        release_text(&unreadable(5), &Ok(0)),
        "Ended 0 of the lease's connections; a look failed: Windows error 5."
    );
}

// ---- A process counted once across the walks (pins P23 and P26) --------------------------------

#[test]
fn an_identity_in_both_not_ended_lists_is_counted_once() {
    // A spared ancestor, and a client neither walk could end: found by each walk, counted once.
    let x = id(100, 1);
    assert_eq!(
        release_text(&not_all(2, vec![x]), &not_all(0, vec![x])),
        "Ended 2 of the lease's connections; 1 could not be ended."
    );
    assert_eq!(
        release_text(&not_all(0, vec![x]), &not_all(0, vec![x])),
        "Ended 0 of the lease's connections; 1 could not be ended."
    );
}

#[test]
fn an_identity_only_in_the_first_walk_is_not_counted_when_the_second_read_the_list() {
    // The second walk ended it (its `ended` counts it) or it exited: it is not in `<k>`.
    let x = id(100, 1);
    assert_eq!(
        release_text(&not_all(0, vec![x]), &Ok(1)),
        "Ended 1 of the lease's connections."
    );
    assert_eq!(
        release_text(&not_all(3, vec![x]), &Ok(0)),
        "Ended 3 of the lease's connections."
    );
}

#[test]
fn an_identity_only_in_the_first_walk_stays_counted_when_the_second_could_not_read_the_list() {
    let x = id(100, 1);
    assert_eq!(
        release_text(&not_all(1, vec![x]), &unreadable(5)),
        "Ended 1 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
    );
}

#[test]
fn an_identity_only_in_the_second_walk_is_counted() {
    let y = id(200, 2);
    assert_eq!(
        release_text(&Ok(2), &not_all(0, vec![y])),
        "Ended 2 of the lease's connections; 1 could not be ended."
    );
    // The first walk could not read the list: its error and the second's counts both show.
    assert_eq!(
        release_text(&unreadable(5), &not_all(1, vec![y])),
        "Ended 1 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
    );
}

#[test]
fn the_pid_and_the_creation_time_are_both_compared() {
    // The same process ID with another creation time is another process: the first's is gone from
    // the second walk's list, and the second's is new.
    assert_eq!(
        release_text(&not_all(0, vec![id(100, 1)]), &not_all(0, vec![id(100, 2)])),
        "Ended 0 of the lease's connections; 1 could not be ended."
    );
    // The same creation time with another process ID likewise.
    assert_eq!(
        release_text(&not_all(0, vec![id(100, 1)]), &not_all(0, vec![id(101, 1)])),
        "Ended 0 of the lease's connections; 1 could not be ended."
    );
    // Two clients that share a process ID and differ in creation time are two, in one walk.
    assert_eq!(
        release_text(&not_all(0, vec![id(100, 1), id(100, 2)]), &Ok(0)),
        "Ended 0 of the lease's connections."
    );
    assert_eq!(
        release_text(
            &Ok(0),
            &not_all(0, vec![id(100, 1), id(100, 2), id(101, 1)])
        ),
        "Ended 0 of the lease's connections; 3 could not be ended."
    );
}

#[test]
fn a_mixed_pair_of_walks_counts_each_process_once() {
    let (x, y, z) = (id(100, 1), id(101, 2), id(102, 3));
    // X was left by the first walk and ended by the second, Y is in both, Z is new in the second.
    assert_eq!(
        release_text(&not_all(2, vec![x, y]), &not_all(1, vec![y, z])),
        "Ended 3 of the lease's connections; 2 could not be ended."
    );
}

#[test]
fn the_text_never_holds_a_process_id_or_creation_time() {
    let text = release_text(
        &not_all(1, vec![id(4_242_424, 133_000_000_000_000_000)]),
        &unreadable(5),
    );
    assert!(!text.contains("4242424"), "A process ID is in the text");
    assert!(
        !text.contains("133000000000000000"),
        "A creation time is in the text"
    );
}

#[test]
fn the_text_has_no_time_no_lease_id_and_no_line_feed() {
    for text in [
        release_text(&Ok(0), &Ok(0)),
        release_text(&unreadable(5), &unreadable(5)),
        release_text(&not_all(1, vec![id(1, 1)]), &unreadable(5)),
    ] {
        assert!(!text.contains('\n'));
        assert!(!text.contains("lease="));
        assert!(text.starts_with("Ended ") || text.starts_with("The lease's connections"));
        assert!(text.ends_with('.'));
    }
}

// ---- EndError ----------------------------------------------------------------------------------

#[test]
fn end_error_display_has_counts_and_the_windows_error_only() {
    let text = EndError::ListUnreadable {
        windows_error: 1_234_567,
    }
    .to_string();
    assert!(text.contains("1234567"), "The Windows error number");

    let text = EndError::NotAllEnded {
        ended: 7,
        not_ended: vec![id(4_242_424, 133_000_000_000_000_000), id(4_242_425, 1)],
    }
    .to_string();
    assert!(text.contains('7'), "The ended count");
    assert!(text.contains('2'), "The not-ended count");
    assert!(!text.contains("4242424"), "A process ID is in the text");
    assert!(!text.contains("4242425"), "A process ID is in the text");
    assert!(
        !text.contains("133000000000000000"),
        "A creation time is in the text"
    );
}

#[test]
fn end_error_is_an_error() {
    let boxed: Box<dyn std::error::Error> = Box::new(EndError::ListUnreadable { windows_error: 5 });
    assert!(!boxed.to_string().is_empty());
    assert_eq!(
        EndError::ListUnreadable { windows_error: 5 },
        EndError::ListUnreadable { windows_error: 5 }
    );
}

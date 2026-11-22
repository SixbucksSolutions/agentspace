//! The text a release logs for its two walks of `end_alias_clients`.
//!
//! RFC 2119 keywords; the capitals are load-bearing. The rules are docs/driver.md, "The count" and
//! "Where the count is logged". This module is pure: it counts what two walks returned and writes
//! the text that follows `lease=<id> ` in `release.log`, and it never reads a process, a file or the
//! clock. The EC2 driver's release calls it, so the counting rule has one home.

use std::collections::HashSet;

use crate::{EndError, Identity, Result};

/// What one walk told the release.
enum Walk {
    /// The walk read the process list: the clients it ended, and the identities of those it could
    /// not end.
    Read {
        ended: usize,
        not_ended: HashSet<Identity>,
    },
    /// The walk could not read the process list, for this Windows error.
    Unread { windows_error: u32 },
}

impl Walk {
    fn of(result: &Result<usize>) -> Self {
        match result {
            Ok(ended) => Walk::Read {
                ended: *ended,
                not_ended: HashSet::new(),
            },
            Err(EndError::NotAllEnded { ended, not_ended }) => Walk::Read {
                ended: *ended,
                not_ended: not_ended.iter().copied().collect(),
            },
            Err(EndError::ListUnreadable { windows_error }) => Walk::Unread {
                windows_error: *windows_error,
            },
        }
    }
}

/// The text of a release's line after `lease=<id> `, for its two walks' results.
///
/// The text has no time, no lease ID and no line feed. A walk that read the process list is an `Ok`
/// or an error with counts; one that could not is `EndError::ListUnreadable`, and it adds nothing to
/// either count and never makes the other walk's counts be thrown away.
///
/// `<n>`, the clients ended, is the two walks' ended counts summed. `<k>`, the clients that could
/// not be ended, counts each client once, by its [`Identity`]: an identity that both walks could not
/// end is counted once, and one that only the first walk could not end, when the second walk read
/// the list, was ended by the second walk (its `ended` already counts it) or exited, and is not in
/// `<k>`. So `<k>` holds only the clients still left when the second walk is done, or, when the
/// second walk could not read the list, those the first walk could not end. `<e>` is the Windows
/// error of the first walk that could not read the list.
#[must_use]
pub fn release_text(first: &Result<usize>, second: &Result<usize>) -> String {
    let first = Walk::of(first);
    let second = Walk::of(second);

    let (n, k, failed) = match (&first, &second) {
        (Walk::Unread { windows_error }, Walk::Unread { .. }) => {
            return format!(
                "The lease's connections could not be looked for: Windows error {windows_error}."
            );
        }
        (
            Walk::Read {
                ended: first_ended,
                not_ended: _,
            },
            Walk::Read {
                ended: second_ended,
                not_ended: second_left,
            },
        ) => {
            // Everything the second walk could not end is still left, and counted once, whether or
            // not the first walk found it too: an identity is in the set once. What only the first
            // walk could not end was ended by the second or has exited, and is not counted.
            (first_ended + second_ended, second_left.len(), None)
        }
        (Walk::Read { ended, not_ended }, Walk::Unread { windows_error })
        | (Walk::Unread { windows_error }, Walk::Read { ended, not_ended }) => {
            (*ended, not_ended.len(), Some(*windows_error))
        }
    };

    let mut text = format!("Ended {n} of the lease's connections");
    if k > 0 {
        text.push_str(&format!("; {k} could not be ended"));
    }
    if let Some(windows_error) = failed {
        text.push_str(&format!("; a look failed: Windows error {windows_error}"));
    }
    text.push('.');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(pid: u32, created: u64) -> Identity {
        Identity { pid, created }
    }

    fn left(ended: usize, not_ended: &[Identity]) -> Result<usize> {
        Err(EndError::NotAllEnded {
            ended,
            not_ended: not_ended.to_vec(),
        })
    }

    fn unread(windows_error: u32) -> Result<usize> {
        Err(EndError::ListUnreadable { windows_error })
    }

    // The five rows of the table.

    #[test]
    fn row_both_read_and_nothing_left() {
        assert_eq!(
            release_text(&Ok(2), &Ok(1)),
            "Ended 3 of the lease's connections."
        );
    }

    #[test]
    fn row_both_read_and_some_left() {
        let a = id(10, 100);
        let b = id(11, 101);
        assert_eq!(
            release_text(&left(3, &[a, b]), &left(0, &[a, b])),
            "Ended 3 of the lease's connections; 2 could not be ended."
        );
    }

    #[test]
    fn row_one_read_one_failed_and_nothing_left() {
        assert_eq!(
            release_text(&Ok(4), &unread(5)),
            "Ended 4 of the lease's connections; a look failed: Windows error 5."
        );
        assert_eq!(
            release_text(&unread(5), &Ok(4)),
            "Ended 4 of the lease's connections; a look failed: Windows error 5."
        );
    }

    #[test]
    fn row_one_read_one_failed_and_some_left() {
        let a = id(10, 100);
        // The example of the spec: a first walk that ended 3 and could not end 1, then a second that
        // could not read the list.
        assert_eq!(
            release_text(&left(3, &[a]), &unread(5)),
            "Ended 3 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
        );
        assert_eq!(
            release_text(&unread(5), &left(3, &[a])),
            "Ended 3 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
        );
    }

    #[test]
    fn row_neither_read() {
        assert_eq!(
            release_text(&unread(5), &unread(5)),
            "The lease's connections could not be looked for: Windows error 5."
        );
    }

    // `Ended 0` is always there when a walk read the list.

    #[test]
    fn ended_zero_is_always_present() {
        assert_eq!(
            release_text(&Ok(0), &Ok(0)),
            "Ended 0 of the lease's connections."
        );
        assert_eq!(
            release_text(&Ok(0), &unread(5)),
            "Ended 0 of the lease's connections; a look failed: Windows error 5."
        );
        assert_eq!(
            release_text(&unread(5), &Ok(0)),
            "Ended 0 of the lease's connections; a look failed: Windows error 5."
        );
        let a = id(10, 100);
        assert_eq!(
            release_text(&left(0, &[a]), &left(0, &[a])),
            "Ended 0 of the lease's connections; 1 could not be ended."
        );
        assert_eq!(
            release_text(&left(0, &[a]), &unread(5)),
            "Ended 0 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
        );
    }

    // `<e>` is the first failing walk's.

    #[test]
    fn error_number_is_the_first_failing_walks_when_both_failed() {
        assert_eq!(
            release_text(&unread(5), &unread(1722)),
            "The lease's connections could not be looked for: Windows error 5."
        );
        assert_eq!(
            release_text(&unread(1722), &unread(5)),
            "The lease's connections could not be looked for: Windows error 1722."
        );
    }

    #[test]
    fn error_number_is_the_failing_walks_when_one_failed() {
        assert_eq!(
            release_text(&Ok(1), &unread(1722)),
            "Ended 1 of the lease's connections; a look failed: Windows error 1722."
        );
        assert_eq!(
            release_text(&unread(1722), &Ok(1)),
            "Ended 1 of the lease's connections; a look failed: Windows error 1722."
        );
    }

    #[test]
    fn a_walk_that_failed_keeps_the_other_walks_counts() {
        let a = id(10, 100);
        // The first walk's ended clients stay in <n> when the second could not read the list.
        assert_eq!(
            release_text(&left(7, &[a]), &unread(5)),
            "Ended 7 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
        );
        // The second's stay when the first could not.
        assert_eq!(
            release_text(&unread(5), &left(7, &[a])),
            "Ended 7 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
        );
    }

    // Counting a process once across the walks, by its identity.

    #[test]
    fn a_process_both_walks_could_not_end_is_counted_once() {
        let spared = id(4242, 132_000_000_000_000_000);
        assert_eq!(
            release_text(&left(0, &[spared]), &left(0, &[spared])),
            "Ended 0 of the lease's connections; 1 could not be ended."
        );
        // Beside clients that were ended, in either walk.
        assert_eq!(
            release_text(&left(2, &[spared]), &left(1, &[spared])),
            "Ended 3 of the lease's connections; 1 could not be ended."
        );
    }

    #[test]
    fn a_process_the_second_walk_ended_is_no_longer_in_k() {
        let a = id(10, 100);
        // The first could not end it; the second did, and its `ended` counts it.
        assert_eq!(
            release_text(&left(0, &[a]), &Ok(1)),
            "Ended 1 of the lease's connections."
        );
    }

    #[test]
    fn a_process_the_second_walk_did_not_find_is_no_longer_in_k() {
        let a = id(10, 100);
        let b = id(11, 101);
        // It exited between the walks: the second walk read the list and did not find it.
        assert_eq!(
            release_text(&left(1, &[a]), &Ok(0)),
            "Ended 1 of the lease's connections."
        );
        // Of two the first could not end, one is still left.
        assert_eq!(
            release_text(&left(1, &[a, b]), &left(1, &[b])),
            "Ended 2 of the lease's connections; 1 could not be ended."
        );
    }

    #[test]
    fn a_process_only_the_second_walk_could_not_end_is_counted() {
        let late = id(30, 300);
        assert_eq!(
            release_text(&Ok(1), &left(0, &[late])),
            "Ended 1 of the lease's connections; 1 could not be ended."
        );
        let spared = id(10, 100);
        assert_eq!(
            release_text(&left(0, &[spared]), &left(0, &[spared, late])),
            "Ended 0 of the lease's connections; 2 could not be ended."
        );
    }

    #[test]
    fn identity_is_the_pid_and_the_creation_time_together() {
        // The same process ID with another creation time is another process.
        let first = id(10, 100);
        let reused = id(10, 200);
        assert_eq!(
            release_text(&left(0, &[first]), &left(0, &[reused])),
            "Ended 0 of the lease's connections; 1 could not be ended."
        );
        // A different process ID with the same creation time is another process too.
        let beside = id(11, 100);
        assert_eq!(
            release_text(&left(0, &[first]), &left(0, &[first, beside])),
            "Ended 0 of the lease's connections; 2 could not be ended."
        );
    }

    #[test]
    fn an_identity_listed_twice_by_one_walk_is_counted_once() {
        let a = id(10, 100);
        assert_eq!(
            release_text(&left(0, &[a, a]), &left(0, &[a, a])),
            "Ended 0 of the lease's connections; 1 could not be ended."
        );
        assert_eq!(
            release_text(&left(0, &[a, a]), &unread(5)),
            "Ended 0 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
        );
    }

    #[test]
    fn identities_alone_decide_the_count_not_the_number_of_ended() {
        let a = id(10, 100);
        let b = id(11, 101);
        let c = id(12, 102);
        assert_eq!(
            release_text(&left(5, &[a, b]), &left(2, &[b, c])),
            "Ended 7 of the lease's connections; 2 could not be ended."
        );
    }

    // The form of every text.

    #[test]
    fn a_text_has_no_line_feed_and_ends_in_a_full_stop() {
        let a = id(10, 100);
        let results = [
            Ok(0),
            Ok(3),
            left(1, &[a]),
            unread(5),
            left(0, &[a, id(2, 2)]),
        ];
        for first in &results {
            for second in &results {
                let text = release_text(first, second);
                assert!(!text.contains('\n') && !text.contains('\r'));
                assert!(text.ends_with('.'));
                assert!(
                    text.starts_with("Ended ")
                        || text.starts_with("The lease's connections could not be looked for: "),
                    "Unexpected start of a text"
                );
            }
        }
    }

    #[test]
    fn a_text_never_holds_a_process_identity() {
        let secret_pid = id(987_654_321, 876_543_210_123);
        let text = release_text(&left(0, &[secret_pid]), &left(0, &[secret_pid]));
        assert!(!text.contains("987654321"));
        assert!(!text.contains("876543210123"));
    }
}

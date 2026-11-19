//! Which leases are ready, and which lease a start goes through (docs/keeper.md, A pass: Which
//! leases are ready; Which lease).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **A lease is ready when its
//! stored state is `ready` and its `expires_at` is strictly after the pass time.** A start goes
//! through the ready lease with the latest `expires_at`, and among those that share it the one
//! whose lease ID is lowest in ordinal order.

mod common;

use common::*;

fn started(id: &str, pid: u32) -> String {
    line(NOW, "started", id, &alias_of(id), &pid.to_string(), None)
}

// ---- Which leases are ready ----

#[test]
fn a_ready_lease_expiring_after_the_pass_time_is_ready() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![ready(ID_A, "2026-11-19T12:00:01Z")]);
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_eq!(h.log(), started(ID_A, 4242));
}

#[test]
fn a_lease_expiring_at_the_pass_time_is_expired() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![ready(ID_A, NOW)]);
    let r = h.pass(NOW);
    assert_eq!(r.exit, 0);
    assert_eq!(h.log(), idle_line(NOW));
    assert_eq!(h.starts(), 0);
}

#[test]
fn a_lease_expired_before_the_pass_time_is_not_ready() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![ready(ID_A, "2026-11-19T11:59:59Z")]);
    h.pass(NOW);
    assert_eq!(h.log(), idle_line(NOW));
    assert_eq!(h.starts(), 0);
}

#[test]
fn readiness_is_decided_against_the_given_now_not_the_clock() {
    // The fake clock says it is long past the expiry. The pass time is -Now.
    let h = Harness::new();
    h.start_echo(4242);
    h.fakes.clock.set_ms(ms_of(2030, 1, 1, 0, 0, 0));
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.pass(NOW);
    assert_eq!(h.log(), started(ID_A, 4242));
}

#[test]
fn the_same_file_is_ready_at_one_pass_time_and_not_at_a_later_one() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.pass("2026-11-19T12:59:59Z");
    h.pass("2026-11-19T13:00:00Z");
    assert_eq!(
        h.log(),
        format!(
            "{}{}",
            line(
                "2026-11-19T12:59:59Z",
                "started",
                ID_A,
                &alias_of(ID_A),
                "4242",
                None
            ),
            // The first pass's record is the fake's, and its process is not running (no query
            // answer), so the second pass finds no trusted holder and no ready lease.
            idle_line("2026-11-19T13:00:00Z")
        )
    );
}

#[test]
fn only_a_stored_ready_state_counts_whatever_the_expiry() {
    for stored in [pending(ID_A), released(ID_A, LATEST), unsatisfiable(ID_A)] {
        let h = Harness::new();
        h.start_echo(4242);
        h.write_leases(vec![stored.clone()]);
        let r = h.pass(NOW);
        assert_eq!(r.exit, 0);
        assert_eq!(h.log(), idle_line(NOW), "for {stored}");
        assert_eq!(h.starts(), 0);
    }
}

#[test]
fn no_ready_lease_among_several_is_idle() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![
        pending(ID_A),
        released(ID_B, LATEST),
        ready(ID_C, "2026-11-19T11:00:00Z"),
    ]);
    h.pass(NOW);
    assert_eq!(h.log(), idle_line(NOW));
}

// ---- Which lease ----

#[test]
fn the_ready_lease_that_expires_last_is_chosen() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![
        ready(ID_A, LATER),
        ready(ID_B, LATEST),
        ready(ID_C, "2026-11-19T12:30:00Z"),
    ]);
    h.pass(NOW);
    assert_eq!(h.log(), started(ID_B, 4242));
    let seen = h.fakes.starter.requests.borrow();
    assert_eq!(seen.len(), 1, "At most one holder is started per pass");
    assert_eq!(seen[0].lease_id, ID_B);
    assert_eq!(seen[0].alias, alias_of(ID_B));
}

#[test]
fn the_choice_does_not_follow_the_order_of_the_file() {
    for order in [[ID_A, ID_B, ID_C], [ID_C, ID_B, ID_A], [ID_B, ID_A, ID_C]] {
        let h = Harness::new();
        h.start_echo(4242);
        // ID_C expires last.
        let recs = order
            .iter()
            .map(|id| {
                let at = match *id {
                    ID_C => LATEST,
                    ID_B => LATER,
                    _ => "2026-11-19T12:30:00Z",
                };
                ready(id, at)
            })
            .collect();
        h.write_leases(recs);
        h.pass(NOW);
        assert_eq!(h.log(), started(ID_C, 4242), "for order {order:?}");
    }
}

#[test]
fn among_leases_sharing_the_latest_expiry_the_lowest_id_in_ordinal_order_is_chosen() {
    // ID_C "0a0a0a0a-..." < ID_D "0a0a0a0b-..." < ID_A "3f2a9c1e-..." < ID_B "9b1d0c77-...".
    for order in [
        [ID_B, ID_A, ID_D, ID_C],
        [ID_C, ID_D, ID_A, ID_B],
        [ID_D, ID_B, ID_C, ID_A],
    ] {
        let h = Harness::new();
        h.start_echo(4242);
        h.write_leases(order.iter().map(|id| ready(id, LATER)).collect());
        h.pass(NOW);
        assert_eq!(h.log(), started(ID_C, 4242), "for order {order:?}");
    }
}

#[test]
fn a_later_expiry_beats_a_lower_id() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![ready(ID_C, LATER), ready(ID_B, LATEST)]);
    h.pass(NOW);
    assert_eq!(h.log(), started(ID_B, 4242));
}

#[test]
fn the_tie_is_between_those_sharing_the_latest_expiry_only() {
    // ID_C is the lowest ID, but it expires earlier than ID_A and ID_D, which share the latest.
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![
        ready(ID_C, LATER),
        ready(ID_A, LATEST),
        ready(ID_D, LATEST),
    ]);
    h.pass(NOW);
    assert_eq!(h.log(), started(ID_D, 4242));
}

#[test]
fn a_lease_that_is_not_ready_is_never_chosen_whatever_its_expiry_or_id() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![
        released(ID_C, "2026-11-19T20:00:00Z"),
        pending(ID_D),
        ready(ID_B, "2026-11-19T11:00:00Z"),
        ready(ID_A, LATER),
    ]);
    h.pass(NOW);
    assert_eq!(h.log(), started(ID_A, 4242));
}

#[test]
fn the_choice_is_stable_the_same_leases_always_give_the_same_choice() {
    let mut chosen = Vec::new();
    for _ in 0..3 {
        let h = Harness::new();
        h.start_echo(4242);
        h.write_leases(vec![ready(ID_B, LATER), ready(ID_A, LATER)]);
        h.pass(NOW);
        chosen.push(h.log());
    }
    assert!(chosen.windows(2).all(|w| w[0] == w[1]), "{chosen:?}");
    assert_eq!(chosen[0], started(ID_A, 4242));
}

#[test]
fn the_holder_goes_through_the_chosen_leases_alias_as_the_state_gives_it() {
    let h = Harness::new();
    h.start_echo(4242);
    h.write_leases(vec![ready_with_alias(
        ID_A,
        LATER,
        "agentspace-other-3f2a9c1e",
    )]);
    h.pass(NOW);
    assert_eq!(
        h.log(),
        line(
            NOW,
            "started",
            ID_A,
            "agentspace-other-3f2a9c1e",
            "4242",
            None
        )
    );
    assert_eq!(
        h.fakes.starter.requests.borrow()[0].alias,
        "agentspace-other-3f2a9c1e"
    );
}

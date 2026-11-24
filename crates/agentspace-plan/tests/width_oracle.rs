//! The width of a story's `after` graph against a brute-force largest antichain.
//!
//! The oracle (`common::oracle_width`) takes the transitive closure of the starts and tries every
//! subset of subagents for the largest one in which no member starts after another, directly or
//! through others. It shares nothing with the crate. It is run on:
//! - every graph on 1 to 6 subagents whose starts all point from a lower number to a higher one
//!   (every acyclic graph up to relabelling), written in order;
//! - a seeded random set of larger graphs of 7 to 12 subagents, written in a shuffled order, so
//!   an `after` names subagents written later, with transitive and redundant starts in them;
//! - a seeded random set of plans of several turns and stories, whose turns' agents and busiest
//!   turn are the sums and the first maximum of the oracle's widths.
//!
//! RFC 2119 keywords here are the specs' own.

mod common;

use common::*;

/// The most subagents whose every acyclic graph is tried: 2^15 graphs at 6.
const EXHAUSTIVE_UP_TO: usize = 6;

fn pairs(n: usize) -> Vec<(usize, usize)> {
    let mut all = Vec::new();
    for a in 0..n {
        for b in (a + 1)..n {
            all.push((a, b));
        }
    }
    all
}

fn agents_of_one_story(n: usize, edges: &[(usize, usize)], order: &[usize]) -> u32 {
    let plan = ok(&one_story_plan(graph_subagents(n, edges, order)));
    let (name, agents) = plan.busiest();
    assert_eq!(name, "T1");
    agents
}

#[test]
fn the_oracle_itself_is_right_on_the_shapes_whose_widths_are_known() {
    // A checker is only believed once it has found something: these widths are written by hand.
    assert_eq!(oracle_width(1, &[]), 1);
    assert_eq!(oracle_width(5, &[]), 5);
    assert_eq!(
        oracle_width(3, &[(0, 1), (1, 2)]),
        1,
        "a chain, closed through its middle"
    );
    assert_eq!(
        oracle_width(4, &[(0, 1), (0, 2), (1, 3), (2, 3)]),
        2,
        "a diamond"
    );
    assert_eq!(
        oracle_width(4, &[(0, 3), (1, 3), (2, 3)]),
        3,
        "#0203's join"
    );
    assert_eq!(oracle_width(4, &[(0, 2), (1, 2), (1, 3)]), 2, "the N shape");
    assert_eq!(
        oracle_width(4, &[(0, 1), (1, 2)]),
        2,
        "a chain of three and a free subagent"
    );
}

#[test]
fn every_acyclic_graph_up_to_six_subagents_has_the_oracles_width() {
    let mut graphs = 0u32;
    for n in 1..=EXHAUSTIVE_UP_TO {
        let all = pairs(n);
        let order: Vec<usize> = (0..n).collect();
        for mask in 0u32..(1u32 << all.len()) {
            let edges: Vec<(usize, usize)> = all
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, e)| *e)
                .collect();
            let expected = oracle_width(n, &edges) + 1;
            let got = agents_of_one_story(n, &edges, &order);
            assert_eq!(got, expected, "{n} subagents, edges {edges:?}");
            graphs += 1;
        }
    }
    // 1 + 2 + 8 + 64 + 1024 + 32768 graphs.
    assert_eq!(graphs, 33_867);
}

fn random_graph(rng: &mut Rng, n: usize) -> (Vec<(usize, usize)>, Vec<usize>) {
    // A random topological order, then edges only from earlier to later in it, so no cycle; the
    // written order is a different shuffle, so an `after` names a subagent written later.
    let mut topo: Vec<usize> = (0..n).collect();
    rng.shuffle(&mut topo);
    let density = 1 + rng.below(6) as u64; // 1/8 .. 6/8
    let mut edges = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if rng.chance(density, 8) {
                edges.push((topo[i], topo[j]));
            }
        }
    }
    rng.shuffle(&mut edges);
    let mut written: Vec<usize> = (0..n).collect();
    rng.shuffle(&mut written);
    (edges, written)
}

#[test]
fn random_graphs_of_seven_to_twelve_subagents_have_the_oracles_width() {
    let mut rng = Rng(0x00A5_E0D1);
    for round in 0..600 {
        let n = 7 + rng.below(6);
        let (edges, written) = random_graph(&mut rng, n);
        let expected = oracle_width(n, &edges) + 1;
        let got = agents_of_one_story(n, &edges, &written);
        assert_eq!(
            got, expected,
            "round {round}: {n} subagents, edges {edges:?}, written {written:?}"
        );
    }
}

#[test]
fn random_graphs_written_in_a_shuffled_order_agree_with_the_same_graph_in_order() {
    let mut rng = Rng(0x5EED_0002);
    for round in 0..300 {
        let n = 2 + rng.below(9);
        let (edges, written) = random_graph(&mut rng, n);
        let in_order: Vec<usize> = (0..n).collect();
        assert_eq!(
            agents_of_one_story(n, &edges, &written),
            agents_of_one_story(n, &edges, &in_order),
            "round {round}"
        );
    }
}

#[test]
fn random_plans_have_turns_that_sum_their_stories_and_a_first_busiest_turn() {
    let mut rng = Rng(0xB05_5E55);
    for round in 0..200 {
        let turn_count = 1 + rng.below(5);
        let mut turns = Vec::new();
        let mut expected: Vec<u32> = Vec::new();
        for t in 0..turn_count {
            let story_count = 1 + rng.below(4);
            let mut stories = Vec::new();
            let mut agents = 1; // the dev lead
            for s in 0..story_count {
                let n = 1 + rng.below(8);
                let (edges, written) = random_graph(&mut rng, n);
                agents += oracle_width(n, &edges);
                stories.push(story(
                    &format!("S{s}"),
                    graph_subagents(n, &edges, &written),
                ));
            }
            turns.push(turn(&format!("T{t}"), stories));
            expected.push(agents);
        }
        let most = *expected.iter().max().expect("a turn");
        let first = expected
            .iter()
            .position(|&a| a == most)
            .expect("the maximum");
        let plan = ok(&plan(turns));
        let (name, agents) = plan.busiest();
        assert_eq!(agents, most, "round {round}: turns {expected:?}");
        assert_eq!(
            name,
            format!("T{first}"),
            "round {round}: turns {expected:?}"
        );
    }
}

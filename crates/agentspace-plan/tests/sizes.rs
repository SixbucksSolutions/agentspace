//! The footprint, `size_for` and `most_agents` (lease-api.md, Plan, "The size picked"; D73, D75).
//!
//! RFC 2119 keywords here are the specs' own.

mod common;

use agentspace_plan::{PER_AGENT_MIB, PER_AGENT_MILLIVCPUS, most_agents, size_for};
use common::*;

#[test]
fn the_footprint_is_half_a_vcpu_and_2_gib_an_agent() {
    let millivcpus: u32 = PER_AGENT_MILLIVCPUS;
    let mib: u32 = PER_AGENT_MIB;
    assert_eq!(millivcpus, 500);
    assert_eq!(mib, 2048);
}

#[test]
fn the_largest_size_holds_32_agents() {
    assert_eq!(most_agents(), 32);
    assert_eq!(size_for(most_agents()), Some("xl"));
    assert_eq!(size_for(most_agents() + 1), None);
}

#[test]
fn size_for_at_each_edge_of_the_table() {
    // lease-api.md, Plan: 2 to 4 is s, 5 to 8 is m, 9 to 16 is l, 17 to 32 is xl, more is none.
    let edges: [(u32, Option<&str>); 18] = [
        (2, Some("s")),
        (3, Some("s")),
        (4, Some("s")),
        (5, Some("m")),
        (6, Some("m")),
        (8, Some("m")),
        (9, Some("l")),
        (10, Some("l")),
        (16, Some("l")),
        (17, Some("xl")),
        (18, Some("xl")),
        (31, Some("xl")),
        (32, Some("xl")),
        (33, None),
        (34, None),
        (64, None),
        (1025, None),
        (u32::MAX, None),
    ];
    for (agents, expected) in edges {
        assert_eq!(size_for(agents), expected, "size_for({agents})");
    }
}

#[test]
fn size_for_below_a_well_formed_plan_is_still_the_smallest_size() {
    // A well-formed plan has at least 2 agents, so 0 and 1 are not in the table. "The smallest of
    // s, m, l and xl" that holds them is s (N1 in the report).
    assert_eq!(size_for(0), Some("s"));
    assert_eq!(size_for(1), Some("s"));
}

#[test]
fn size_for_follows_the_floors_of_the_size_table_at_the_footprint() {
    // An oracle written from the Size table (2, 4, 8, 16 vCPUs; 8, 16, 32, 64 GiB) and the
    // footprint, with no reference to the crate.
    let sizes = [
        ("s", 2u64, 8u64),
        ("m", 4, 16),
        ("l", 8, 32),
        ("xl", 16, 64),
    ];
    for agents in 0u32..=80 {
        let expected = sizes
            .iter()
            .find(|(_, vcpus, gib)| {
                u64::from(agents) * u64::from(PER_AGENT_MILLIVCPUS) <= vcpus * 1000
                    && u64::from(agents) * u64::from(PER_AGENT_MIB) <= gib * 1024
            })
            .map(|(label, _, _)| *label);
        assert_eq!(size_for(agents), expected, "size_for({agents})");
    }
}

#[test]
fn the_size_never_falls_as_the_agents_rise() {
    let rank = |size: Option<&str>| match size {
        Some("s") => 0,
        Some("m") => 1,
        Some("l") => 2,
        Some("xl") => 3,
        None => 4,
        other => panic!("a size outside s, m, l, xl: {other:?}"),
    };
    let mut last = 0;
    for agents in 0u32..=70 {
        let now = rank(size_for(agents));
        assert!(
            now >= last,
            "size_for({agents}) is smaller than size_for({})",
            agents - 1
        );
        last = now;
    }
}

#[test]
fn a_plan_at_each_edge_gets_the_size_the_table_gives() {
    // agents at once, as busiest reports them, then the size. Each plan is built so that its one
    // turn has exactly that many agents with its dev lead.
    let edges: [(u32, Option<&str>); 12] = [
        (2, Some("s")),
        (4, Some("s")),
        (5, Some("m")),
        (8, Some("m")),
        (9, Some("l")),
        (16, Some("l")),
        (17, Some("xl")),
        (32, Some("xl")),
        (33, None),
        (34, None),
        (100, None),
        (1025, None),
    ];
    for (agents, expected) in edges {
        let plan = ok(&plan(vec![turn_with_agents("T01", agents)]));
        let (name, at_once) = plan.busiest();
        assert_eq!(name, "T01");
        assert_eq!(
            at_once, agents,
            "agents at once of the plan built for {agents}"
        );
        assert_eq!(
            size_for(at_once),
            expected,
            "size of a plan of {agents} agents"
        );
    }
}

#[test]
fn the_edges_made_of_stories_not_of_one_wide_story() {
    // 4 agents: two stories of one subagent each, plus the dev lead is 3; add a third story
    // of one is 4. 5 agents is a fourth story.
    for (stories, expected_agents, expected_size) in [
        (3usize, 4u32, Some("s")),
        (4, 5, Some("m")),
        (7, 8, Some("m")),
        (8, 9, Some("l")),
    ] {
        let one_each: Vec<_> = (0..stories)
            .map(|i| story(&format!("S{i}"), vec![sub("x")]))
            .collect();
        let plan = ok(&plan(vec![turn("T", one_each)]));
        let (_, agents) = plan.busiest();
        assert_eq!(agents, expected_agents);
        assert_eq!(size_for(agents), expected_size);
    }
}

#[test]
fn model_and_effort_change_no_size() {
    let a = json_plan_with("Opus", "high");
    let b = json_plan_with("Haiku 9.9 +24h", "0.1_x-y");
    assert_eq!(ok(&a).busiest(), ok(&b).busiest());
}

fn json_plan_with(model: &str, effort: &str) -> serde_json::Value {
    serde_json::json!({"turns": [{"name": "T", "stories": [{"name": "S", "subagents": [
        {"name": "a", "model": model, "effort": effort},
        {"name": "b", "model": model, "effort": effort}
    ]}]}]})
}

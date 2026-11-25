//! A story's peak (the width of its `after` graph), a turn's agents at once, the busiest turn
//! and the accessors (lease-api.md, Plan; D74).
//!
//! RFC 2119 keywords here are the specs' own.

mod common;

use common::*;
use serde_json::{Value, json};

// ---- A story's peak ----

#[test]
fn a_chain_has_a_peak_of_one() {
    for n in 1..=32 {
        assert_eq!(peak_of(chain(n)), 1, "a chain of {n}");
    }
}

#[test]
fn an_antichain_has_a_peak_of_its_number_of_subagents() {
    for n in 1..=32 {
        assert_eq!(
            peak_of(antichain(n)),
            n as u32,
            "{n} subagents with no after"
        );
    }
}

#[test]
fn a_diamond_has_a_peak_of_two() {
    let diamond = vec![
        sub("a"),
        sub_after("b", &["a"]),
        sub_after("c", &["a"]),
        sub_after("d", &["b", "c"]),
    ];
    assert_eq!(peak_of(diamond), 2);
}

#[test]
fn a_fork_has_a_peak_of_its_branches() {
    let fork = vec![
        sub("a"),
        sub_after("b", &["a"]),
        sub_after("c", &["a"]),
        sub_after("d", &["a"]),
    ];
    assert_eq!(peak_of(fork), 3);
}

#[test]
fn a_join_has_a_peak_of_its_inputs() {
    let join = vec![
        sub("a"),
        sub("b"),
        sub("c"),
        sub_after("d", &["a", "b", "c"]),
    ];
    assert_eq!(peak_of(join), 3);
}

#[test]
fn two_chains_side_by_side_have_a_peak_of_two() {
    let two = vec![
        sub("a"),
        sub_after("b", &["a"]),
        sub_after("c", &["b"]),
        sub("x"),
        sub_after("y", &["x"]),
        sub_after("z", &["y"]),
    ];
    assert_eq!(peak_of(two), 2);
}

#[test]
fn a_chain_and_a_free_subagent_have_a_peak_of_two() {
    let mut subagents = chain(3);
    subagents.push(sub("free"));
    assert_eq!(peak_of(subagents), 2);
}

#[test]
fn the_n_shape_has_a_peak_of_two() {
    // a < c, b < c, b < d.
    let n_shape = vec![
        sub("a"),
        sub("b"),
        sub_after("c", &["a", "b"]),
        sub_after("d", &["b"]),
    ];
    assert_eq!(peak_of(n_shape), 2);
}

#[test]
fn starting_after_through_others_counts_at_any_depth() {
    // c names only b, and b names only a, so a and c are not at once: the peak is 1, not 2.
    let through = vec![sub("a"), sub_after("b", &["a"]), sub_after("c", &["b"])];
    assert_eq!(peak_of(through), 1);
    // The same at depth 6, with a free subagent that holds the peak at 2.
    let mut deep = chain(6);
    deep.push(sub("free"));
    assert_eq!(peak_of(deep), 2);
}

#[test]
fn a_redundant_direct_start_changes_nothing() {
    let redundant = vec![
        sub("a"),
        sub_after("b", &["a"]),
        sub_after("c", &["a", "b"]),
    ];
    assert_eq!(peak_of(redundant), 1);
}

#[test]
fn an_after_may_name_a_subagent_written_later() {
    // The chain c, b, a is written backwards: a starts after none, b after a, c after b.
    let backwards = vec![sub_after("c", &["b"]), sub_after("b", &["a"]), sub("a")];
    assert_eq!(peak_of(backwards), 1);
    // A fork written with its root last.
    let fork = vec![
        sub_after("x", &["root"]),
        sub_after("y", &["root"]),
        sub("root"),
    ];
    assert_eq!(peak_of(fork), 2);
}

#[test]
fn an_empty_after_and_a_null_after_mean_none() {
    let none = vec![
        json!({"name": "a", "model": "Opus", "effort": "high", "after": []}),
        json!({"name": "b", "model": "Opus", "effort": "high", "after": null}),
        sub("c"),
    ];
    assert_eq!(peak_of(none), 3);
}

#[test]
fn the_widest_after_names_31_others() {
    // The 32nd subagent starts after the other 31, so it is alone at the end: the peak is 31.
    let mut subagents = antichain(31);
    let names: Vec<String> = (0..31).map(|i| format!("a{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    subagents.push(sub_after("last", &names));
    assert_eq!(subagents.len(), 32);
    assert_eq!(peak_of(subagents), 31);
}

#[test]
fn a_peak_stated_anywhere_is_not_read() {
    // A story of 3 free subagents has a peak of 3 whatever a card says; and a stated peak is a
    // member the schema refuses, so it cannot be smuggled in either.
    let mut with_peak = one_story_plan(antichain(3));
    with_peak["turns"][0]["stories"][0]["peak"] = json!(1);
    assert_eq!(err(&with_peak), SCHEMA);
}

// ---- #0203's plan ----

fn plan_0203() -> Value {
    json!({"name":"v0.09","turns":[{"name":"T03","stories":[{"name":"E0.09-T03-S01","subagents":[
      {"name":"implementer","model":"Opus","effort":"high"},
      {"name":"test author","model":"Sonnet","effort":"high"},
      {"name":"runner","model":"Sonnet","effort":"medium"},
      {"name":"reviewer","model":"Opus","effort":"high","after":["implementer","test author","runner"]}]}]}]})
}

#[test]
fn plan_0203_has_a_story_peak_of_three_and_four_agents_at_once() {
    let plan = ok(&plan_0203());
    assert_eq!(plan.name(), Some("v0.09"));
    assert_eq!(plan.busiest(), ("T03", 4));
    assert_eq!(agentspace_plan::size_for(4), Some("s"));
}

#[test]
fn plan_0203_is_read_from_the_text_the_architect_wrote_too() {
    // The exact text of lease-api.md's example, with its line breaks.
    let text = r#"{"name":"v0.09","turns":[{"name":"T03","stories":[{"name":"E0.09-T03-S01","subagents":[
  {"name":"implementer","model":"Opus","effort":"high"},
  {"name":"test author","model":"Sonnet","effort":"high"},
  {"name":"runner","model":"Sonnet","effort":"medium"},
  {"name":"reviewer","model":"Opus","effort":"high","after":["implementer","test author","runner"]}]}]}]}"#;
    let plan = parse_ok(text.as_bytes());
    assert_eq!(plan.busiest(), ("T03", 4));
    assert_eq!(plan.document(), &plan_0203());
}

// ---- A turn's agents at once ----

#[test]
fn a_turn_is_the_sum_of_its_stories_peaks_plus_one() {
    // Peaks 3 (join), 1 (chain), 2 (diamond) and 1 (one subagent): 7, and the dev lead is 8.
    let join = vec![
        sub("a"),
        sub("b"),
        sub("c"),
        sub_after("d", &["a", "b", "c"]),
    ];
    let diamond = vec![
        sub("a"),
        sub_after("b", &["a"]),
        sub_after("c", &["a"]),
        sub_after("d", &["b", "c"]),
    ];
    let p = plan(vec![turn(
        "T",
        vec![
            story("S1", join),
            story("S2", chain(5)),
            story("S3", diamond),
            story("S4", vec![sub("only")]),
        ],
    )]);
    assert_eq!(ok(&p).busiest(), ("T", 8));
}

#[test]
fn the_smallest_plan_has_two_agents_at_once() {
    // Every plan that is well formed has at least 2: one subagent and the dev lead.
    let p = one_story_plan(vec![sub("a")]);
    assert_eq!(ok(&p).busiest(), ("T1", 2));
}

#[test]
fn a_story_in_another_turn_does_not_add_to_this_one() {
    let p = plan(vec![
        turn("T1", vec![story("S", antichain(3))]),
        turn("T2", vec![story("S", antichain(2))]),
    ]);
    assert_eq!(ok(&p).busiest(), ("T1", 4));
}

#[test]
fn the_widest_possible_turn_has_1025_agents_at_once() {
    // 32 stories of 32 free subagents: 1024 and the dev lead. A turn within the bounds can have
    // this many (lease-api.md, "The bounds").
    let p = plan(vec![turn_with_agents("Wide", 1025)]);
    assert_eq!(ok(&p).busiest(), ("Wide", 1025));
}

// ---- The busiest turn ----

fn turns_with_agents(counts: &[u32]) -> Value {
    plan(
        counts
            .iter()
            .enumerate()
            .map(|(i, &agents)| turn_with_agents(&format!("T{i}"), agents))
            .collect(),
    )
}

#[test]
fn the_busiest_turn_is_the_one_with_the_most_agents() {
    assert_eq!(ok(&turns_with_agents(&[3, 9, 5])).busiest(), ("T1", 9));
    assert_eq!(ok(&turns_with_agents(&[3, 5, 9])).busiest(), ("T2", 9));
    assert_eq!(ok(&turns_with_agents(&[9, 5, 3])).busiest(), ("T0", 9));
    assert_eq!(ok(&turns_with_agents(&[7])).busiest(), ("T0", 7));
}

#[test]
fn on_a_tie_the_first_busiest_turn_wins() {
    assert_eq!(ok(&turns_with_agents(&[5, 5])).busiest(), ("T0", 5));
    assert_eq!(ok(&turns_with_agents(&[3, 6, 6])).busiest(), ("T1", 6));
    assert_eq!(
        ok(&turns_with_agents(&[3, 6, 4, 6, 6])).busiest(),
        ("T1", 6)
    );
    assert_eq!(ok(&turns_with_agents(&[4, 4, 4, 4])).busiest(), ("T0", 4));
    // A later turn that only ties an earlier maximum never replaces it, and a later strict
    // maximum does.
    assert_eq!(ok(&turns_with_agents(&[6, 6, 7, 7])).busiest(), ("T2", 7));
}

#[test]
fn a_tie_is_in_the_plans_order_not_in_name_order() {
    let p = plan(vec![
        turn_with_agents("zeta", 6),
        turn_with_agents("alpha", 6),
    ]);
    assert_eq!(ok(&p).busiest(), ("zeta", 6));
}

#[test]
fn the_busiest_turn_is_named_as_the_plan_writes_it() {
    let p = plan(vec![
        turn_with_agents("t 01 (Mixed Case)", 3),
        turn_with_agents("T 02 -- \"quoted\" \\ it's", 4),
    ]);
    assert_eq!(ok(&p).busiest(), ("T 02 -- \"quoted\" \\ it's", 4));
}

#[test]
fn the_busiest_turn_counts_peaks_not_subagents() {
    // T0 has 10 subagents in a chain (peak 1, 2 agents); T1 has 3 free ones (4 agents).
    let p = plan(vec![
        turn("T0", vec![story("S", chain(10))]),
        turn("T1", vec![story("S", antichain(3))]),
    ]);
    assert_eq!(ok(&p).busiest(), ("T1", 4));
}

// ---- The plan's name and its document ----

#[test]
fn the_name_is_the_plans_as_written() {
    let p = named_plan(
        "Epic v0.10 (plan)",
        vec![turn("T", vec![story("S", vec![sub("a")])])],
    );
    assert_eq!(ok(&p).name(), Some("Epic v0.10 (plan)"));
}

#[test]
fn a_plan_with_no_name_or_a_null_name_has_none() {
    let none = one_story_plan(vec![sub("a")]);
    assert_eq!(ok(&none).name(), None);
    let mut null_name = one_story_plan(vec![sub("a")]);
    null_name["name"] = Value::Null;
    assert_eq!(ok(&null_name).name(), None);
}

#[test]
fn the_document_is_the_document_as_parsed() {
    let mut p = named_plan(
        "n",
        vec![turn(
            "T",
            vec![story(
                "S",
                vec![
                    sub("a"),
                    json!({"name": "b", "model": "Opus 5.5 +24h", "effort": "x.y", "after": ["a"]}),
                    json!({"name": "c", "model": "Sonnet", "effort": "low", "after": []}),
                    json!({"name": "d", "model": "Sonnet", "effort": "low", "after": null}),
                ],
            )],
        )],
    );
    let parsed = ok(&p);
    assert_eq!(parsed.document(), &p);
    // Nothing is normalized or dropped: the same bytes in a different layout are one document.
    let pretty = serde_json::to_string_pretty(&p).expect("a value serializes");
    assert_eq!(parse_ok(pretty.as_bytes()).document(), &p);
    p["name"] = Value::Null;
    assert_eq!(ok(&p).document(), &p);
}

#[test]
fn the_document_keeps_model_and_effort_as_given() {
    let p = plan(vec![turn(
        "T",
        vec![story(
            "S",
            vec![json!({"name": "a", "model": "Opus 5.5 +24h", "effort": "XHigh"})],
        )],
    )]);
    let parsed = ok(&p);
    let kept = &parsed.document()["turns"][0]["stories"][0]["subagents"][0];
    assert_eq!(kept["model"], "Opus 5.5 +24h");
    assert_eq!(kept["effort"], "XHigh");
}

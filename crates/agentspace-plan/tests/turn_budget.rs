//! A turn's `budget_minutes`, a turn cut from a plan, and the TTL factor (docs/lease-api.md, Plan,
//! Turn and TTL; D84, D85; the architect's pin P24 for v0.11-T01).
//!
//! RFC 2119 keywords here are the specs' own. Every expectation is from the spec's text, its
//! tables and its worked numbers, never from the program's output.

mod common;

use agentspace_plan::{TTL_FACTOR_PERCENT, ttl_seconds_for};
use common::*;
use serde_json::{Value, json};

/// A one-turn plan, written as text, whose turn `T1` has `budget_minutes` written as `raw`
/// exactly, so a number keeps the form it is written in.
fn one_turn_with_budget(raw: &str) -> Vec<u8> {
    format!(
        concat!(
            r#"{{"turns":[{{"name":"T1","budget_minutes":{},"stories":[{{"name":"S1","#,
            r#""subagents":[{{"name":"a","model":"Opus","effort":"high"}}]}}]}}]}}"#
        ),
        raw
    )
    .into_bytes()
}

/// A turn with one story per entry of `peaks`, each of that many independent subagents.
fn turn_of_peaks(name: &str, peaks: &[usize]) -> Value {
    turn(
        name,
        peaks
            .iter()
            .enumerate()
            .map(|(i, &n)| story(&format!("S{i}"), antichain(n)))
            .collect(),
    )
}

fn with_budget(mut turn: Value, minutes: Value) -> Value {
    turn["budget_minutes"] = minutes;
    turn
}

/// Three turns: A has 3 + 1 = 4 agents at once, B has 5 + 2 + 1 = 8, C has 1 + 1 = 2.
fn three_turns() -> Value {
    plan(vec![
        turn_of_peaks("A", &[3]),
        turn_of_peaks("B", &[5, 2]),
        turn("C", vec![story("S0", chain(4))]), // a chain has a peak of 1
    ])
}

// ---- budget_minutes: what is accepted ----

#[test]
fn a_budget_of_every_edge_of_the_range_is_accepted_and_kept() {
    for minutes in [1u32, 2, 90, 959, 960] {
        let parsed = parse_ok(&one_turn_with_budget(&minutes.to_string()));
        assert_eq!(
            parsed.budget_minutes("T1"),
            Some(minutes),
            "budget {minutes}"
        );
    }
}

#[test]
fn a_budget_given_as_a_value_document_is_accepted() {
    let document = plan(vec![with_budget(
        turn("T1", vec![story("S1", vec![sub("a")])]),
        json!(45),
    )]);
    assert_eq!(ok(&document).budget_minutes("T1"), Some(45));
}

#[test]
fn a_null_budget_means_it_is_not_given() {
    let parsed = parse_ok(&one_turn_with_budget("null"));
    assert_eq!(parsed.budget_minutes("T1"), None);
    assert_eq!(parsed.busiest(), ("T1", 2));
    assert_eq!(parsed.turn_agents("T1"), Some(2));
}

#[test]
fn a_budget_may_be_surrounded_by_json_whitespace() {
    let parsed = parse_ok(&one_turn_with_budget("  \n 90 \t "));
    assert_eq!(parsed.budget_minutes("T1"), Some(90));
}

// ---- budget_minutes: what is refused, with check 6's <why> ----

fn assert_schema_refusal(raw: &str) {
    assert_eq!(
        parse_err(&one_turn_with_budget(raw)),
        "the document does not match the plan schema",
        "budget_minutes written as {raw}"
    );
}

#[test]
fn a_budget_below_the_range_is_refused() {
    assert_schema_refusal("0");
}

#[test]
fn a_budget_above_the_range_is_refused() {
    assert_schema_refusal("961");
    assert_schema_refusal("1000");
    assert_schema_refusal("4294967296");
}

#[test]
fn a_budget_beyond_every_integer_type_is_refused_as_json_or_as_schema() {
    // The spec refuses "a number the parser cannot hold" as check 4 and "a number above 960" as
    // check 6, and does not say which of them a number of 20 digits is: it depends on what the
    // build's parser can hold. Either way it is refused, and never accepted.
    let message = parse_err(&one_turn_with_budget("18446744073709551616"));
    assert!(
        message == NOT_JSON || message == SCHEMA,
        "refused with a message that is neither check 4's nor check 6's"
    );
}

#[test]
fn a_negative_budget_is_refused() {
    assert_schema_refusal("-1");
    assert_schema_refusal("-90");
    assert_schema_refusal("-960");
}

#[test]
fn a_string_budget_is_refused() {
    assert_schema_refusal(r#""90""#);
    assert_schema_refusal(r#""""#);
    assert_schema_refusal(r#""null""#);
}

#[test]
fn a_boolean_budget_is_refused() {
    assert_schema_refusal("true");
    assert_schema_refusal("false");
}

#[test]
fn an_array_or_object_budget_is_refused() {
    assert_schema_refusal("[]");
    assert_schema_refusal("[90]");
    assert_schema_refusal("{}");
    assert_schema_refusal(r#"{"minutes":90}"#);
}

#[test]
fn a_budget_with_a_fraction_is_refused() {
    assert_schema_refusal("90.5");
    assert_schema_refusal("0.5");
    assert_schema_refusal("1.0000001");
}

// Pin P24: a whole number not written as digits alone passes the plan's schema, and the program
// refuses it.

#[test]
fn a_budget_written_with_a_zero_fraction_is_refused() {
    assert_schema_refusal("90.0");
    assert_schema_refusal("1.0");
    assert_schema_refusal("960.0");
    assert_schema_refusal("90.00");
}

#[test]
fn a_budget_written_with_an_exponent_is_refused() {
    assert_schema_refusal("9e1");
    assert_schema_refusal("9E1");
    assert_schema_refusal("9e+1");
    assert_schema_refusal("90e0");
    assert_schema_refusal("0.9e2");
    assert_schema_refusal("9.0e1");
    assert_schema_refusal("1e2");
}

#[test]
fn a_budget_written_as_negative_zero_is_refused() {
    assert_schema_refusal("-0");
    assert_schema_refusal("-0.0");
}

#[test]
fn a_budget_the_json_parser_cannot_hold_is_refused_as_json_not_as_schema() {
    // Check 4 comes before the schema's check 6.
    assert_eq!(
        parse_err(&one_turn_with_budget("1e400")),
        "the file is not valid JSON"
    );
}

#[test]
fn a_budget_that_is_not_json_at_all_is_refused_as_json() {
    // A plus sign and a leading zero are not JSON numbers.
    assert_eq!(
        parse_err(&one_turn_with_budget("+90")),
        "the file is not valid JSON"
    );
    assert_eq!(
        parse_err(&one_turn_with_budget("090")),
        "the file is not valid JSON"
    );
}

#[test]
fn a_budget_fault_in_a_later_turn_refuses_the_plan() {
    let document = plan(vec![
        with_budget(turn_of_peaks("A", &[2]), json!(60)),
        with_budget(turn_of_peaks("B", &[2]), json!(961)),
    ]);
    assert_eq!(err(&document), SCHEMA);
    let document = plan(vec![
        turn_of_peaks("A", &[2]),
        with_budget(turn_of_peaks("B", &[2]), json!(0)),
    ]);
    assert_eq!(err(&document), SCHEMA);
}

#[test]
fn a_budget_fault_is_found_by_the_schema_check_before_the_names_checks() {
    // Check 6 comes before check 7: two turns of one name, one with a bad budget, is a schema fault.
    let document = plan(vec![
        with_budget(turn_of_peaks("T", &[1]), json!(0)),
        turn_of_peaks("T", &[1]),
    ]);
    assert_eq!(err(&document), SCHEMA);
    // And with a good budget the same two turns are check 7's.
    let document = plan(vec![
        with_budget(turn_of_peaks("T", &[1]), json!(60)),
        turn_of_peaks("T", &[1]),
    ]);
    assert_eq!(err(&document), "turn 'T' is named twice");
}

#[test]
fn a_budget_member_given_twice_is_check_5s_not_the_schemas() {
    let text = concat!(
        r#"{"turns":[{"name":"T1","budget_minutes":90,"budget_minutes":0,"stories":["#,
        r#"{"name":"S1","subagents":[{"name":"a","model":"Opus","effort":"high"}]}]}]}"#
    );
    assert_eq!(
        parse_err(text.as_bytes()),
        "an object has two members named 'budget_minutes'"
    );
}

#[test]
fn a_budget_is_a_member_of_a_turn_and_of_nothing_else() {
    let mut on_plan = plan(vec![turn_of_peaks("T1", &[1])]);
    on_plan["budget_minutes"] = json!(60);
    assert_eq!(err(&on_plan), SCHEMA);

    let mut on_story = plan(vec![turn_of_peaks("T1", &[1])]);
    on_story["turns"][0]["stories"][0]["budget_minutes"] = json!(60);
    assert_eq!(err(&on_story), SCHEMA);

    let mut on_subagent = plan(vec![turn_of_peaks("T1", &[1])]);
    on_subagent["turns"][0]["stories"][0]["subagents"][0]["budget_minutes"] = json!(60);
    assert_eq!(err(&on_subagent), SCHEMA);
}

#[test]
fn a_misspelled_budget_member_is_refused() {
    let mut document = plan(vec![turn_of_peaks("T1", &[1])]);
    document["turns"][0]["budget"] = json!(60);
    assert_eq!(err(&document), SCHEMA);
    let mut document = plan(vec![turn_of_peaks("T1", &[1])]);
    document["turns"][0]["Budget_minutes"] = json!(60);
    assert_eq!(err(&document), SCHEMA);
}

// ---- budget_minutes(turn): with and without a budget ----

#[test]
fn budget_minutes_is_that_turns_own_and_none_for_a_turn_without() {
    let document = plan(vec![
        with_budget(turn_of_peaks("A", &[3]), json!(120)),
        turn_of_peaks("B", &[5, 2]),
        with_budget(turn_of_peaks("C", &[1]), json!(1)),
        with_budget(turn_of_peaks("D", &[1]), Value::Null),
    ]);
    let parsed = ok(&document);
    assert_eq!(parsed.budget_minutes("A"), Some(120));
    assert_eq!(parsed.budget_minutes("B"), None);
    assert_eq!(parsed.budget_minutes("C"), Some(1));
    assert_eq!(parsed.budget_minutes("D"), None);
}

#[test]
fn a_turn_with_no_budget_gets_none_whatever_budgets_the_others_have() {
    let document = plan(vec![
        with_budget(turn_of_peaks("A", &[2]), json!(960)),
        turn_of_peaks("B", &[2]),
    ]);
    let parsed = ok(&document);
    assert_eq!(parsed.budget_minutes("B"), None);
    assert_eq!(parsed.budget_minutes("A"), Some(960));
}

#[test]
fn budget_minutes_of_a_name_that_is_no_turn_is_none() {
    let document = plan(vec![with_budget(turn_of_peaks("T01", &[2]), json!(90))]);
    let parsed = ok(&document);
    assert_eq!(parsed.budget_minutes("T02"), None);
    assert_eq!(parsed.budget_minutes(""), None);
    // Compared exactly: a case or whitespace difference is no match.
    assert_eq!(parsed.budget_minutes("t01"), None);
    assert_eq!(parsed.budget_minutes(" T01"), None);
    assert_eq!(parsed.budget_minutes("T01 "), None);
    assert_eq!(parsed.budget_minutes("T01\n"), None);
    assert_eq!(parsed.budget_minutes("T 01"), None);
    assert_eq!(parsed.budget_minutes("T01"), Some(90));
}

#[test]
fn budget_minutes_does_not_confuse_turns_whose_names_differ_only_by_case() {
    let document = plan(vec![
        with_budget(turn_of_peaks("T03", &[1]), json!(30)),
        with_budget(turn_of_peaks("t03", &[1]), json!(40)),
    ]);
    let parsed = ok(&document);
    assert_eq!(parsed.budget_minutes("T03"), Some(30));
    assert_eq!(parsed.budget_minutes("t03"), Some(40));
}

#[test]
fn a_budget_changes_nothing_a_plan_is_sized_by() {
    let without = ok(&three_turns());
    let mut document = three_turns();
    for (i, minutes) in [(0, 10), (1, 960), (2, 1)] {
        document["turns"][i]["budget_minutes"] = json!(minutes);
    }
    let with = ok(&document);
    assert_eq!(with.busiest(), without.busiest());
    for name in ["A", "B", "C"] {
        assert_eq!(with.turn_agents(name), without.turn_agents(name), "{name}");
    }
    assert_eq!(with.name(), without.name());
}

// ---- turn_agents: a turn cut from the plan ----

#[test]
fn a_turn_cut_from_a_plan_gives_that_turns_agents_and_not_the_busiest() {
    let parsed = ok(&three_turns());
    // The sum of its stories' peaks plus 1 for its dev lead.
    assert_eq!(parsed.turn_agents("A"), Some(4));
    assert_eq!(parsed.turn_agents("B"), Some(8));
    assert_eq!(parsed.turn_agents("C"), Some(2));
    // The busiest turn is B, as v1 reported it.
    assert_eq!(parsed.busiest(), ("B", 8));
}

#[test]
fn a_turn_that_is_not_the_busiest_is_never_given_the_busiest_turns_agents() {
    let parsed = ok(&plan(vec![
        turn_of_peaks("small", &[1]),
        turn_of_peaks("big", &[32, 32, 32]),
    ]));
    assert_eq!(parsed.turn_agents("small"), Some(2));
    assert_eq!(parsed.turn_agents("big"), Some(97));
    assert_eq!(parsed.busiest(), ("big", 97));
}

#[test]
fn a_turn_counts_each_stories_peak_from_its_starts_and_one_for_its_dev_lead() {
    // S0: three subagents, the last after the first two: peak 2. S1: a chain of three: peak 1.
    let s0 = story("S0", vec![sub("x"), sub("y"), sub_after("z", &["x", "y"])]);
    let s1 = story("S1", chain(3));
    let parsed = ok(&plan(vec![turn("T", vec![s0, s1])]));
    assert_eq!(parsed.turn_agents("T"), Some(2 + 1 + 1));
}

#[test]
fn a_tied_turn_is_cut_with_its_own_agents_and_the_busiest_is_the_first() {
    let parsed = ok(&plan(vec![
        turn_of_peaks("first", &[4]),
        turn_of_peaks("second", &[2, 2]),
        turn_of_peaks("third", &[1]),
    ]));
    assert_eq!(parsed.turn_agents("first"), Some(5));
    assert_eq!(parsed.turn_agents("second"), Some(5));
    assert_eq!(parsed.turn_agents("third"), Some(2));
    assert_eq!(parsed.busiest(), ("first", 5));
}

#[test]
fn the_busiest_turn_of_a_plan_gives_the_plans_busiest_agents() {
    let parsed = ok(&three_turns());
    let (name, agents) = parsed.busiest();
    assert_eq!(parsed.turn_agents(name), Some(agents));
}

#[test]
fn every_turn_of_a_plan_of_many_turns_gets_its_own_count() {
    let turns: Vec<Value> = (0..32u32)
        .map(|i| turn_with_agents(&format!("T{i}"), 2 + i))
        .collect();
    let parsed = ok(&plan(turns));
    for i in 0..32u32 {
        assert_eq!(
            parsed.turn_agents(&format!("T{i}")),
            Some(2 + i),
            "turn {i}"
        );
    }
    assert_eq!(parsed.busiest(), ("T31", 33));
}

#[test]
fn a_turn_of_the_largest_plan_shape_is_counted() {
    // 32 stories of 32 independent subagents, plus the dev lead: 1025, the contract's most.
    let parsed = ok(&plan(vec![
        turn_with_agents("big", 1025),
        turn_with_agents("small", 2),
    ]));
    assert_eq!(parsed.turn_agents("big"), Some(1025));
    assert_eq!(parsed.turn_agents("small"), Some(2));
}

// ---- turn_agents: a name that is no turn ----

#[test]
fn a_name_that_is_no_turn_of_the_plan_is_none() {
    let parsed = ok(&three_turns());
    assert_eq!(parsed.turn_agents("D"), None);
    assert_eq!(parsed.turn_agents("T01"), None);
    assert_eq!(parsed.turn_agents(""), None);
    // A story's name and a subagent's are no turn's.
    assert_eq!(parsed.turn_agents("S0"), None);
    assert_eq!(parsed.turn_agents("a0"), None);
}

#[test]
fn a_turn_name_is_compared_exactly_never_by_case_or_whitespace() {
    let parsed = ok(&plan(vec![turn_of_peaks("T01", &[2])]));
    assert_eq!(parsed.turn_agents("T01"), Some(3));
    assert_eq!(parsed.turn_agents("t01"), None);
    assert_eq!(parsed.turn_agents("T01 "), None);
    assert_eq!(parsed.turn_agents(" T01"), None);
    assert_eq!(parsed.turn_agents("\tT01"), None);
    assert_eq!(parsed.turn_agents("T01\r\n"), None);
    assert_eq!(parsed.turn_agents("T 01"), None);
    assert_eq!(parsed.turn_agents("T0"), None);
    assert_eq!(parsed.turn_agents("T011"), None);
}

#[test]
fn turns_whose_names_differ_only_by_case_or_inner_spaces_are_two_turns() {
    let parsed = ok(&plan(vec![
        turn_of_peaks("T03", &[1]),
        turn_of_peaks("t03", &[3]),
        turn_of_peaks("a b", &[5]),
        turn_of_peaks("a  b", &[7]),
    ]));
    assert_eq!(parsed.turn_agents("T03"), Some(2));
    assert_eq!(parsed.turn_agents("t03"), Some(4));
    assert_eq!(parsed.turn_agents("a b"), Some(6));
    assert_eq!(parsed.turn_agents("a  b"), Some(8));
}

#[test]
fn a_name_outside_the_form_is_no_turn_and_does_not_panic() {
    let parsed = ok(&three_turns());
    let long = "A".repeat(65);
    assert_eq!(parsed.turn_agents(&long), None);
    assert_eq!(parsed.turn_agents("\u{feff}A"), None);
    assert_eq!(parsed.turn_agents("\u{e9}"), None);
    assert_eq!(parsed.turn_agents("A\u{0}"), None);
    assert_eq!(parsed.budget_minutes(&long), None);
    assert_eq!(parsed.budget_minutes("\u{feff}A"), None);
}

#[test]
fn a_turn_name_with_spaces_inside_is_found() {
    let parsed = ok(&plan(vec![
        turn_of_peaks("first turn", &[2]),
        with_budget(turn_of_peaks("second turn", &[1]), json!(60)),
    ]));
    assert_eq!(parsed.turn_agents("first turn"), Some(3));
    assert_eq!(parsed.turn_agents("second turn"), Some(2));
    assert_eq!(parsed.budget_minutes("second turn"), Some(60));
    assert_eq!(parsed.budget_minutes("first turn"), None);
}

#[test]
fn a_turn_name_of_the_longest_form_is_found() {
    let long = format!("L{}M", "x".repeat(62)); // 64 characters
    assert_eq!(long.len(), 64);
    let parsed = ok(&plan(vec![with_budget(
        turn_of_peaks(&long, &[2]),
        json!(10),
    )]));
    assert_eq!(parsed.turn_agents(&long), Some(3));
    assert_eq!(parsed.budget_minutes(&long), Some(10));
    assert_eq!(parsed.turn_agents(&format!("{long} ")), None);
}

// ---- A v0.10 plan with no budget parses as before ----

#[test]
fn the_specs_own_v0_09_plan_parses_as_before() {
    // The plan lease-api.md prints for #0203: one turn, T03, 4 agents at once.
    let text = concat!(
        r#"{"name":"v0.09","turns":[{"name":"T03","stories":[{"name":"E0.09-T03-S01","subagents":["#,
        r#"{"name":"implementer","model":"Opus","effort":"high"},"#,
        r#"{"name":"test author","model":"Sonnet","effort":"high"},"#,
        r#"{"name":"runner","model":"Sonnet","effort":"medium"},"#,
        r#"{"name":"reviewer","model":"Opus","effort":"high","after":["implementer","test author","runner"]}]}]}]}"#
    );
    let parsed = parse_ok(text.as_bytes());
    assert_eq!(parsed.name(), Some("v0.09"));
    assert_eq!(parsed.busiest(), ("T03", 4));
    assert_eq!(parsed.turn_agents("T03"), Some(4));
    assert_eq!(parsed.budget_minutes("T03"), None);
}

#[test]
fn a_plan_with_no_budget_anywhere_has_no_budget_for_any_turn_and_the_same_busiest() {
    let parsed = ok(&three_turns());
    for name in ["A", "B", "C", "other"] {
        assert_eq!(parsed.budget_minutes(name), None, "{name}");
    }
    assert_eq!(parsed.busiest(), ("B", 8));
    assert!(!parsed.document().to_string().contains("budget_minutes"));
}

#[test]
fn a_plan_with_a_budget_keeps_it_in_the_document_it_records() {
    let document = plan(vec![with_budget(turn_of_peaks("T1", &[1]), json!(45))]);
    let parsed = ok(&document);
    assert_eq!(parsed.document()["turns"][0]["budget_minutes"], json!(45));
}

#[test]
fn v0_11s_own_plan_gives_the_specs_worked_numbers() {
    // lease-api.md, TTL: with no --turn the plan is for its busiest turn, T02, of 11 agents at once
    // and a budget of 210 minutes; with --turn T01, 9 agents and 180 minutes.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/plans/v0.11.json");
    let bytes = std::fs::read(path).expect("docs/plans/v0.11.json is in the repository");
    let parsed = parse_ok(&bytes);
    assert_eq!(parsed.busiest(), ("T02", 11));
    assert_eq!(parsed.turn_agents("T02"), Some(11));
    assert_eq!(parsed.budget_minutes("T02"), Some(210));
    assert_eq!(parsed.turn_agents("T01"), Some(9));
    assert_eq!(parsed.budget_minutes("T01"), Some(180));
    assert_eq!(ttl_seconds_for(210), 18900);
    assert_eq!(ttl_seconds_for(180), 16200);
    assert_eq!(parsed.turn_agents("t01"), None);
}

// ---- The TTL factor ----

#[test]
fn the_ttl_factor_is_one_and_a_half() {
    assert_eq!(TTL_FACTOR_PERCENT, 150);
}

#[test]
fn ttl_seconds_for_the_specs_table() {
    // lease-api.md, TTL: the table of budget_minutes to ttl_seconds.
    assert_eq!(ttl_seconds_for(1), 120);
    assert_eq!(ttl_seconds_for(45), 4080);
    assert_eq!(ttl_seconds_for(120), 10800);
    assert_eq!(ttl_seconds_for(180), 16200);
    assert_eq!(ttl_seconds_for(210), 18900);
    assert_eq!(ttl_seconds_for(960), 86400);
}

#[test]
fn ttl_seconds_for_rounds_a_half_minute_up_to_the_whole_minute() {
    // 179 minutes and a half is 268.5, rounded up to 269 minutes.
    assert_eq!(ttl_seconds_for(179), 16140);
    assert_eq!(ttl_seconds_for(180), 16200);
    // 3 minutes is 4.5, rounded up to 5; 2 is exactly 3.
    assert_eq!(ttl_seconds_for(2), 180);
    assert_eq!(ttl_seconds_for(3), 300);
    assert_eq!(ttl_seconds_for(4), 360);
    assert_eq!(ttl_seconds_for(5), 480);
}

#[test]
fn ttl_seconds_for_is_ceil_of_one_and_a_half_times_the_budget_in_whole_minutes() {
    for budget in 1u32..=960 {
        let minutes = (budget * 3).div_ceil(2);
        assert_eq!(ttl_seconds_for(budget), minutes * 60, "budget {budget}");
    }
}

#[test]
fn every_ttl_a_plan_gives_is_within_the_ttl_range_and_never_clamped() {
    let mut last = 0;
    for budget in 1u32..=960 {
        let ttl = ttl_seconds_for(budget);
        assert!((60..=86400).contains(&ttl), "budget {budget} gave {ttl}");
        assert_eq!(ttl % 60, 0, "budget {budget} gave {ttl}: a whole minute");
        assert!(ttl > last, "budget {budget} gave {ttl}, not above {last}");
        last = ttl;
    }
    assert_eq!(ttl_seconds_for(960), 86400);
    assert_eq!(ttl_seconds_for(1), 120);
}

#[test]
fn the_factor_and_ttl_seconds_for_agree() {
    // ttl = ceil(budget * factor / 100) minutes, in seconds.
    let factor = u64::from(TTL_FACTOR_PERCENT);
    for budget in [1u32, 2, 3, 45, 119, 179, 180, 210, 959, 960] {
        let minutes = (u64::from(budget) * factor).div_ceil(100);
        assert_eq!(
            u64::from(ttl_seconds_for(budget)),
            minutes * 60,
            "budget {budget}"
        );
    }
}

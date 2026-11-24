//! Checks 5 and 7 to 11 of lease-api.md's Plan table, and the order they report in: the faults a
//! JSON schema cannot see, each message byte for byte (the `<why>` alone, D81 change note P7).
//!
//! RFC 2119 keywords here are the specs' own.

mod common;

use common::*;
use serde_json::{Value, json};

fn two_turns_named(a: &str, b: &str) -> Value {
    plan(vec![
        turn(a, vec![story("S", vec![sub("x")])]),
        turn(b, vec![story("S", vec![sub("x")])]),
    ])
}

// ---- Check 7: turns ----

#[test]
fn two_turns_of_one_name_are_refused() {
    assert_eq!(
        err(&two_turns_named("T01", "T01")),
        "turn 'T01' is named twice"
    );
}

#[test]
fn turn_names_are_compared_exactly() {
    // T03 and t03 are two names; so are names that differ by an inner space.
    ok(&two_turns_named("T03", "t03"));
    ok(&two_turns_named("a b", "a  b"));
    ok(&two_turns_named("a", "A"));
}

#[test]
fn the_turn_reported_is_the_first_name_that_repeats_an_earlier_one() {
    let names = |list: &[&str]| {
        plan(
            list.iter()
                .map(|n| turn(n, vec![story("S", vec![sub("x")])]))
                .collect(),
        )
    };
    // A, B, A, B: the third turn is the first to repeat.
    assert_eq!(
        err(&names(&["A", "B", "A", "B"])),
        "turn 'A' is named twice"
    );
    // A, B, B, A: the third turn, B, repeats first, though A is the earlier name.
    assert_eq!(
        err(&names(&["A", "B", "B", "A"])),
        "turn 'B' is named twice"
    );
    // Three of one name is one fault.
    assert_eq!(err(&names(&["Z", "Z", "Z"])), "turn 'Z' is named twice");
    // The first and last of 32 turns.
    let mut thirty_two: Vec<String> = (0..31).map(|i| format!("t{i}")).collect();
    thirty_two.push("t0".to_string());
    let refs: Vec<&str> = thirty_two.iter().map(String::as_str).collect();
    assert_eq!(err(&names(&refs)), "turn 't0' is named twice");
}

#[test]
fn a_turn_name_is_quoted_raw() {
    let doubled = |n: &str| two_turns_named(n, n);
    assert_eq!(err(&doubled("it's")), "turn 'it's' is named twice");
    assert_eq!(
        err(&doubled("a b\\c\"d")),
        "turn 'a b\\c\"d' is named twice"
    );
    assert_eq!(err(&doubled("T 01")), "turn 'T 01' is named twice");
}

#[test]
fn two_names_equal_once_their_escapes_are_read_are_the_same_name() {
    // "T0" is T0.
    let bytes = br#"{"turns":[
        {"name":"T0","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]},
        {"name":"T0","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]}]}"#;
    assert_eq!(parse_err(bytes), "turn 'T0' is named twice");
}

// ---- Check 8: stories ----

fn stories_named(turn_name: &str, names: &[&str]) -> Value {
    plan(vec![turn(
        turn_name,
        names.iter().map(|n| story(n, vec![sub("x")])).collect(),
    )])
}

#[test]
fn two_stories_of_one_turn_with_one_name_are_refused() {
    assert_eq!(
        err(&stories_named("T01", &["S1", "S1"])),
        "story 'S1' is named twice in turn 'T01'"
    );
}

#[test]
fn story_names_are_compared_exactly_and_only_within_their_turn() {
    ok(&stories_named("T", &["S1", "s1"]));
    // The same story name in two turns, and a story named as its turn, are fine.
    ok(&plan(vec![
        turn(
            "T1",
            vec![story("S1", vec![sub("x")]), story("T1", vec![sub("x")])],
        ),
        turn("T2", vec![story("S1", vec![sub("x")])]),
    ]));
}

#[test]
fn the_story_reported_is_the_first_repeat_and_the_turn_is_the_one_that_holds_it() {
    assert_eq!(
        err(&stories_named("T", &["A", "B", "B", "A"])),
        "story 'B' is named twice in turn 'T'"
    );
    // The first turn is clean; the second has the repeat, so the second's name is quoted.
    let p = plan(vec![
        turn(
            "first",
            vec![story("S1", vec![sub("x")]), story("S2", vec![sub("x")])],
        ),
        turn(
            "second turn",
            vec![story("S1", vec![sub("x")]), story("S1", vec![sub("x")])],
        ),
        turn(
            "third",
            vec![story("Q", vec![sub("x")]), story("Q", vec![sub("x")])],
        ),
    ]);
    assert_eq!(err(&p), "story 'S1' is named twice in turn 'second turn'");
}

// ---- Check 9: subagents ----

fn subagents_named(story_name: &str, names: &[&str]) -> Value {
    plan(vec![turn(
        "T",
        vec![story(story_name, names.iter().map(|n| sub(n)).collect())],
    )])
}

#[test]
fn two_subagents_of_one_story_with_one_name_are_refused() {
    assert_eq!(
        err(&subagents_named("S1", &["a", "a"])),
        "subagent 'a' is named twice in story 'S1'"
    );
}

#[test]
fn subagent_names_are_compared_exactly_and_only_within_their_story() {
    ok(&subagents_named("S", &["a", "A"]));
    ok(&subagents_named("S", &["test author", "test  author"]));
    // The same subagent name in two stories of one turn, and in two turns, is fine; so is a
    // subagent named as its story.
    ok(&plan(vec![
        turn(
            "T1",
            vec![
                story("S1", vec![sub("implementer"), sub("S1")]),
                story("S2", vec![sub("implementer")]),
            ],
        ),
        turn("T2", vec![story("S1", vec![sub("implementer")])]),
    ]));
}

#[test]
fn the_subagent_reported_is_the_first_repeat_and_the_story_is_the_one_that_holds_it() {
    assert_eq!(
        err(&subagents_named("story two", &["a", "b", "b", "a"])),
        "subagent 'b' is named twice in story 'story two'"
    );
    let p = plan(vec![
        turn("T1", vec![story("clean", vec![sub("a"), sub("b")])]),
        turn(
            "T2",
            vec![
                story("first bad", vec![sub("x"), sub("y"), sub("x")]),
                story("second bad", vec![sub("q"), sub("q")]),
            ],
        ),
    ]);
    assert_eq!(err(&p), "subagent 'x' is named twice in story 'first bad'");
}

// ---- Check 10: after ----

#[test]
fn an_after_naming_no_subagent_of_the_story_is_refused() {
    let p = one_story_plan(vec![sub("a"), sub_after("b", &["ghost"])]);
    assert_eq!(
        err(&p),
        "subagent 'b' in story 'S1' starts after 'ghost', which is no subagent of that story"
    );
}

#[test]
fn an_after_naming_a_subagent_of_another_story_is_refused() {
    let p = plan(vec![turn(
        "T",
        vec![
            story("one", vec![sub("a")]),
            story("two", vec![sub_after("b", &["a"])]),
        ],
    )]);
    assert_eq!(
        err(&p),
        "subagent 'b' in story 'two' starts after 'a', which is no subagent of that story"
    );
}

#[test]
fn an_after_naming_a_subagent_of_the_same_name_in_another_turn_is_refused() {
    let p = plan(vec![
        turn("T1", vec![story("S", vec![sub("a")])]),
        turn("T2", vec![story("S", vec![sub_after("b", &["a"])])]),
    ]);
    assert_eq!(
        err(&p),
        "subagent 'b' in story 'S' starts after 'a', which is no subagent of that story"
    );
}

#[test]
fn an_after_is_matched_exactly() {
    // 'A' is not 'a', and 'a ' cannot even be a name; 'test author' needs its inner space.
    let p = one_story_plan(vec![sub("a"), sub_after("b", &["A"])]);
    assert_eq!(
        err(&p),
        "subagent 'b' in story 'S1' starts after 'A', which is no subagent of that story"
    );
    let p = one_story_plan(vec![sub("test author"), sub_after("b", &["test  author"])]);
    assert_eq!(
        err(&p),
        "subagent 'b' in story 'S1' starts after 'test  author', which is no subagent of that story"
    );
    ok(&one_story_plan(vec![
        sub("test author"),
        sub_after("b", &["test author"]),
    ]));
}

#[test]
fn the_after_reported_is_the_first_subagent_with_one_and_its_first_such_entry() {
    let p = plan(vec![turn(
        "T",
        vec![
            story("clean", vec![sub("a"), sub_after("b", &["a"])]),
            story(
                "dirty",
                vec![
                    sub("a"),
                    sub_after("b", &["a", "no1", "no2"]),
                    sub_after("c", &["zz"]),
                ],
            ),
        ],
    )]);
    assert_eq!(
        err(&p),
        "subagent 'b' in story 'dirty' starts after 'no1', which is no subagent of that story"
    );
    // A valid entry before the bad one is skipped, and the later subagent's entry is not first.
    let p = one_story_plan(vec![
        sub("a"),
        sub_after("b", &["a"]),
        sub_after("c", &["b", "missing"]),
        sub_after("d", &["other"]),
    ]);
    assert_eq!(
        err(&p),
        "subagent 'c' in story 'S1' starts after 'missing', which is no subagent of that story"
    );
}

// ---- Check 11: cycle ----

fn cycle_message(story_name: &str) -> String {
    format!("the subagents of story '{story_name}' start after each other in a cycle")
}

#[test]
fn a_subagent_that_starts_after_itself_is_a_cycle() {
    let p = one_story_plan(vec![sub_after("a", &["a"])]);
    assert_eq!(err(&p), cycle_message("S1"));
}

#[test]
fn two_subagents_that_start_after_each_other_are_a_cycle() {
    let p = one_story_plan(vec![sub_after("a", &["b"]), sub_after("b", &["a"])]);
    assert_eq!(err(&p), cycle_message("S1"));
}

#[test]
fn a_cycle_through_three_is_a_cycle() {
    let p = one_story_plan(vec![
        sub_after("a", &["c"]),
        sub_after("b", &["a"]),
        sub_after("c", &["b"]),
    ]);
    assert_eq!(err(&p), cycle_message("S1"));
}

#[test]
fn a_cycle_that_does_not_include_the_first_subagent_is_found() {
    let p = one_story_plan(vec![
        sub("free"),
        sub_after("b", &["free", "c"]),
        sub_after("c", &["b"]),
    ]);
    assert_eq!(err(&p), cycle_message("S1"));
}

#[test]
fn a_cycle_hanging_off_an_acyclic_tail_is_found() {
    // a < b, then b <-> c, then d after c: an acyclic prefix and suffix around a loop.
    let p = one_story_plan(vec![
        sub("a"),
        sub_after("b", &["a", "c"]),
        sub_after("c", &["b"]),
        sub_after("d", &["c"]),
    ]);
    assert_eq!(err(&p), cycle_message("S1"));
}

#[test]
fn a_ring_of_the_most_subagents_is_a_cycle() {
    let ring: Vec<Value> = (0..32)
        .map(|i| sub_after(&format!("r{i}"), &[&format!("r{}", (i + 31) % 32)]))
        .collect();
    assert_eq!(err(&one_story_plan(ring)), cycle_message("S1"));
}

#[test]
fn the_story_reported_is_the_first_with_a_cycle() {
    let p = plan(vec![
        turn(
            "T1",
            vec![story("fine", vec![sub("a"), sub_after("b", &["a"])])],
        ),
        turn(
            "T2",
            vec![
                story("first loop", vec![sub_after("a", &["a"])]),
                story(
                    "second loop",
                    vec![sub_after("a", &["b"]), sub_after("b", &["a"])],
                ),
            ],
        ),
    ]);
    assert_eq!(err(&p), cycle_message("first loop"));
}

#[test]
fn deep_acyclic_graphs_are_never_taken_for_cycles() {
    // The shapes a cycle detector might get wrong: a diamond, a ladder of shared joins, a long
    // chain written backwards, and a subagent reachable by two paths.
    ok(&one_story_plan(vec![
        sub("a"),
        sub_after("b", &["a"]),
        sub_after("c", &["a"]),
        sub_after("d", &["b", "c"]),
        sub_after("e", &["b", "c", "d"]),
        sub_after("f", &["a", "e"]),
    ]));
    let backwards: Vec<Value> = (0..32)
        .rev()
        .map(|i| {
            if i == 0 {
                sub("c0")
            } else {
                sub_after(&format!("c{i}"), &[&format!("c{}", i - 1)])
            }
        })
        .collect();
    ok(&one_story_plan(backwards));
}

// ---- The order of the checks ----

#[test]
fn the_first_check_that_fails_is_the_one_reported() {
    // Each plan fails every check from its own down to check 11, and only its own is reported.
    // Check 6 over check 7: a duplicate turn beside a schema fault.
    let mut p = two_turns_named("T", "T");
    p["turns"][1]["stories"][0]["subagents"][0]["effort"] = json!("bad effort");
    assert_eq!(err(&p), SCHEMA);

    // Check 7 over check 8: two turns of one name, and two stories of one name.
    let p = plan(vec![
        turn(
            "T",
            vec![story("S", vec![sub("x")]), story("S", vec![sub("x")])],
        ),
        turn("T", vec![story("S", vec![sub("x")])]),
    ]);
    assert_eq!(err(&p), "turn 'T' is named twice");

    // Check 8 over check 9, even when the repeated subagent is in an earlier story.
    let p = plan(vec![turn(
        "T",
        vec![
            story("A", vec![sub("x"), sub("x")]),
            story("B", vec![sub("y")]),
            story("B", vec![sub("y")]),
        ],
    )]);
    assert_eq!(err(&p), "story 'B' is named twice in turn 'T'");

    // Check 9 over check 10: a repeated subagent in a late story, a dangling after in an early one.
    let p = plan(vec![turn(
        "T",
        vec![
            story("A", vec![sub_after("a", &["ghost"])]),
            story("B", vec![sub("y"), sub("y")]),
        ],
    )]);
    assert_eq!(err(&p), "subagent 'y' is named twice in story 'B'");

    // Check 10 over check 11: a cycle in an early story, a dangling after in a late one.
    let p = plan(vec![turn(
        "T",
        vec![
            story("A", vec![sub_after("a", &["a"])]),
            story("B", vec![sub_after("b", &["ghost"])]),
        ],
    )]);
    assert_eq!(
        err(&p),
        "subagent 'b' in story 'B' starts after 'ghost', which is no subagent of that story"
    );
}

#[test]
fn a_dangling_after_in_a_story_with_a_cycle_is_reported_as_the_dangling_after() {
    let p = one_story_plan(vec![sub_after("a", &["a"]), sub_after("b", &["ghost"])]);
    assert_eq!(
        err(&p),
        "subagent 'b' in story 'S1' starts after 'ghost', which is no subagent of that story"
    );
}

// ---- Check 5: members ----

fn named_twice(member: &str) -> String {
    format!("an object has two members named '{member}'")
}

#[test]
fn a_member_given_twice_at_the_top_is_refused() {
    let text = br#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[
        {"name":"a","model":"m","effort":"e"}]}]}],"turns":[]}"#;
    assert_eq!(parse_err(text), named_twice("turns"));
}

#[test]
fn a_member_given_twice_at_each_level_is_refused() {
    // The plan's name, a turn's name, a story's subagents, a subagent's model and a subagent's after.
    assert_eq!(
        parse_err(br#"{"name":"a","name":"b","turns":[]}"#),
        named_twice("name")
    );
    assert_eq!(
        parse_err(
            br#"{"turns":[{"name":"T","name":"U","stories":[{"name":"S","subagents":[
                {"name":"a","model":"m","effort":"e"}]}]}]}"#
        ),
        named_twice("name")
    );
    assert_eq!(
        parse_err(
            br#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[
                {"name":"a","model":"m","effort":"e"}],"subagents":[]}]}]}"#
        ),
        named_twice("subagents")
    );
    assert_eq!(
        parse_err(
            br#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[
                {"name":"a","model":"m","model":"n","effort":"e"}]}]}]}"#
        ),
        named_twice("model")
    );
    assert_eq!(
        parse_err(
            br#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[
                {"name":"a","model":"m","effort":"e","after":[],"after":["b"]}]}]}]}"#
        ),
        named_twice("after")
    );
}

#[test]
fn the_same_value_given_twice_is_still_a_member_given_twice() {
    assert_eq!(
        parse_err(br#"{"name":"a","name":"a","turns":[]}"#),
        named_twice("name")
    );
}

#[test]
fn names_equal_once_their_escapes_are_read_are_one_member() {
    // "turns" is "turns".
    let text = br#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[
        {"name":"a","model":"m","effort":"e"}]}]}],"turns":[]}"#;
    assert_eq!(parse_err(text), named_twice("turns"));
}

#[test]
fn members_that_differ_only_in_case_are_two_members() {
    // Not check 5: "Turns" is a member that is not in the shape, which is check 6.
    let text = br#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[
        {"name":"a","model":"m","effort":"e"}]}]}],"Turns":[]}"#;
    assert_eq!(parse_err(text), SCHEMA);
}

#[test]
fn a_member_with_a_control_character_is_quoted_as_backslash_u_and_four_lowercase_hex() {
    assert_eq!(
        parse_err(br#"{"a\u0001":1,"a\u0001":2,"turns":[]}"#),
        named_twice("a\\u0001")
    );
    assert_eq!(
        parse_err(br#"{"x\ny":1,"x\ny":2,"turns":[]}"#),
        named_twice("x\\u000ay")
    );
    assert_eq!(
        parse_err(br#"{"\u001F":1,"\u001f":2,"turns":[]}"#),
        named_twice("\\u001f")
    );
    assert_eq!(
        parse_err(br#"{"a\u007Fb":1,"a\u007fb":2,"turns":[]}"#),
        named_twice("a\\u007fb")
    );
    assert_eq!(
        parse_err(br#"{"\u0000":1,"\u0000":2,"turns":[]}"#),
        named_twice("\\u0000")
    );
}

#[test]
fn an_empty_and_a_non_ascii_member_are_quoted_as_they_are() {
    assert_eq!(parse_err(br#"{"":1,"":2,"turns":[]}"#), named_twice(""));
    assert_eq!(
        parse_err("{\"\u{e9}\":1,\"\u{e9}\":2,\"turns\":[]}".as_bytes()),
        named_twice("\u{e9}")
    );
    // A pair of surrogates written as escapes is one character, quoted as that character.
    let high = r"\ud83d";
    let low = r"\ude00";
    let pair = format!("{high}{low}");
    let text = format!(r#"{{"{pair}":1,"{pair}":2,"turns":[]}}"#);
    assert_eq!(parse_err(text.as_bytes()), named_twice("\u{1F600}"));
}

#[test]
fn a_repeated_member_in_a_member_the_shape_does_not_have_is_still_found_first() {
    // Check 5 precedes check 6, which would refuse "extra" for not being in the shape.
    let text = br#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[
        {"name":"a","model":"m","effort":"e"}]}]}],"extra":{"k":1,"k":2}}"#;
    assert_eq!(parse_err(text), named_twice("k"));
}

#[test]
fn a_repeated_member_is_found_before_a_schema_fault_and_before_the_checks_after_it() {
    // Empty turns would be check 6, a repeated turn name check 7.
    assert_eq!(
        parse_err(br#"{"turns":[],"turns":[]}"#),
        named_twice("turns")
    );
    let text = br#"{"turns":[
        {"name":"T","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]},
        {"name":"T","name":"T","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]}]}"#;
    assert_eq!(parse_err(text), named_twice("name"));
}

#[test]
fn the_repeated_member_reported_is_the_first_in_document_order_across_sibling_objects() {
    let text = br#"{"turns":[
        {"name":"T1","stories":[{"name":"S","subagents":[
            {"name":"a","model":"m","effort":"e","after":[],"after":[]}]}]},
        {"name":"T2","name":"T3","stories":[{"name":"S","subagents":[
            {"name":"a","model":"m","effort":"e"}]}]}]}"#;
    assert_eq!(parse_err(text), named_twice("after"));
    let text = br#"{"turns":[
        {"name":"T1","name":"T9","stories":[{"name":"S","subagents":[
            {"name":"a","model":"m","effort":"e","after":[],"after":[]}]}]}]}"#;
    assert_eq!(parse_err(text), named_twice("name"));
}

#[test]
fn a_repeated_member_name_is_reported_once_however_often_it_repeats() {
    assert_eq!(
        parse_err(br#"{"a":1,"a":2,"a":3,"turns":[]}"#),
        named_twice("a")
    );
}

// ---- What no message carries ----

#[test]
fn no_message_carries_a_model_an_effort_or_a_leading_space_or_a_control_character() {
    // Every refusal of this file's, as a document with a distinctive model and effort.
    let secret = |mut p: Value| {
        for t in p["turns"].as_array_mut().expect("turns").iter_mut() {
            for s in t["stories"].as_array_mut().expect("stories").iter_mut() {
                for a in s["subagents"].as_array_mut().expect("subagents").iter_mut() {
                    a["model"] = json!("ZQmodelXQ");
                    a["effort"] = json!("ZQeffortXQ");
                }
            }
        }
        p
    };
    let refused = vec![
        secret(two_turns_named("T", "T")),
        secret(stories_named("T", &["S", "S"])),
        secret(subagents_named("S", &["a", "a"])),
        secret(one_story_plan(vec![sub_after("b", &["ghost"])])),
        secret(one_story_plan(vec![sub_after("a", &["a"])])),
    ];
    for p in refused {
        let why = err(&p);
        assert!(!why.contains("ZQmodelXQ"), "{why}");
        assert!(!why.contains("ZQeffortXQ"), "{why}");
        assert!(!why.is_empty() && !why.starts_with(' '), "{why}");
        assert!(!why.chars().any(|c| c.is_control()), "{why}");
        assert!(!why.ends_with('.'), "the CLI adds the full stop: {why}");
    }
    // A schema refusal whose model is the fault does not quote it either.
    let mut bad = one_story_plan(vec![sub("a")]);
    bad["turns"][0]["stories"][0]["subagents"][0]["model"] = json!(" ZQmodelXQ");
    assert_eq!(err(&bad), SCHEMA);
}

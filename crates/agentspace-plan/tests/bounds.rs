//! Check 6 of lease-api.md's Plan table: the shape `plan.input.schema.json` defines, each bound
//! at the bound and one past it, and each form of a name, a model and an effort.
//!
//! RFC 2119 keywords here are the specs' own.

mod common;

use common::*;
use serde_json::{Value, json};

/// A named edit to a plan that makes it refused.
type Case = (&'static str, Box<dyn Fn(&mut Value)>);

fn minimal() -> Value {
    one_story_plan(vec![sub("a")])
}

fn sub_path(p: &mut Value) -> &mut Value {
    &mut p["turns"][0]["stories"][0]["subagents"][0]
}

// ---- The document's shape ----

#[test]
fn a_document_that_is_not_an_object_is_refused() {
    for text in [
        "[]",
        "null",
        "\"turns\"",
        "1",
        "1.5",
        "true",
        "false",
        "[{\"turns\":[]}]",
        "{}",
    ] {
        assert_eq!(parse_err(text.as_bytes()), SCHEMA, "{text}");
    }
}

#[test]
fn a_required_member_that_is_missing_or_null_is_refused() {
    let cases: Vec<Case> = vec![
        (
            "no turns",
            Box::new(|p: &mut Value| {
                p.as_object_mut().unwrap().remove("turns");
            }),
        ),
        (
            "turns null",
            Box::new(|p: &mut Value| p["turns"] = Value::Null),
        ),
        (
            "turn without name",
            Box::new(|p: &mut Value| {
                p["turns"][0].as_object_mut().unwrap().remove("name");
            }),
        ),
        (
            "turn name null",
            Box::new(|p: &mut Value| p["turns"][0]["name"] = Value::Null),
        ),
        (
            "turn without stories",
            Box::new(|p: &mut Value| {
                p["turns"][0].as_object_mut().unwrap().remove("stories");
            }),
        ),
        (
            "stories null",
            Box::new(|p: &mut Value| p["turns"][0]["stories"] = Value::Null),
        ),
        (
            "story without name",
            Box::new(|p: &mut Value| {
                p["turns"][0]["stories"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("name");
            }),
        ),
        (
            "story name null",
            Box::new(|p: &mut Value| p["turns"][0]["stories"][0]["name"] = Value::Null),
        ),
        (
            "story without subagents",
            Box::new(|p: &mut Value| {
                p["turns"][0]["stories"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("subagents");
            }),
        ),
        (
            "subagents null",
            Box::new(|p: &mut Value| p["turns"][0]["stories"][0]["subagents"] = Value::Null),
        ),
        (
            "subagent without name",
            Box::new(|p: &mut Value| {
                sub_path(p).as_object_mut().unwrap().remove("name");
            }),
        ),
        (
            "subagent name null",
            Box::new(|p: &mut Value| sub_path(p)["name"] = Value::Null),
        ),
        (
            "subagent without model",
            Box::new(|p: &mut Value| {
                sub_path(p).as_object_mut().unwrap().remove("model");
            }),
        ),
        (
            "model null",
            Box::new(|p: &mut Value| sub_path(p)["model"] = Value::Null),
        ),
        (
            "subagent without effort",
            Box::new(|p: &mut Value| {
                sub_path(p).as_object_mut().unwrap().remove("effort");
            }),
        ),
        (
            "effort null",
            Box::new(|p: &mut Value| sub_path(p)["effort"] = Value::Null),
        ),
    ];
    for (what, edit) in cases {
        let mut p = minimal();
        edit(&mut p);
        assert_eq!(err(&p), SCHEMA, "{what}");
    }
}

#[test]
fn a_member_that_is_not_in_the_shape_is_refused_at_every_level() {
    let cases: Vec<Case> = vec![
        ("plan", Box::new(|p: &mut Value| p["extra"] = json!(1))),
        (
            "plan, a case away from turns",
            Box::new(|p: &mut Value| p["Turns"] = json!([])),
        ),
        (
            "turn",
            Box::new(|p: &mut Value| p["turns"][0]["extra"] = json!(null)),
        ),
        (
            "story",
            Box::new(|p: &mut Value| p["turns"][0]["stories"][0]["extra"] = json!({})),
        ),
        (
            "a story's stated peak",
            Box::new(|p: &mut Value| p["turns"][0]["stories"][0]["peak"] = json!(1)),
        ),
        (
            "subagent",
            Box::new(|p: &mut Value| sub_path(p)["extra"] = json!("x")),
        ),
        (
            "subagent's peak",
            Box::new(|p: &mut Value| sub_path(p)["peak"] = json!(2)),
        ),
        (
            "subagent's before",
            Box::new(|p: &mut Value| sub_path(p)["before"] = json!([])),
        ),
    ];
    for (what, edit) in cases {
        let mut p = minimal();
        edit(&mut p);
        assert_eq!(err(&p), SCHEMA, "{what}");
    }
}

#[test]
fn a_value_of_the_wrong_type_is_refused() {
    let cases: Vec<Case> = vec![
        (
            "name a number",
            Box::new(|p: &mut Value| p["name"] = json!(5)),
        ),
        (
            "name an object",
            Box::new(|p: &mut Value| p["name"] = json!({})),
        ),
        (
            "turns an object",
            Box::new(|p: &mut Value| p["turns"] = json!({})),
        ),
        (
            "turns a string",
            Box::new(|p: &mut Value| p["turns"] = json!("T")),
        ),
        (
            "a turn a string",
            Box::new(|p: &mut Value| p["turns"][0] = json!("T")),
        ),
        (
            "a turn an array",
            Box::new(|p: &mut Value| p["turns"][0] = json!([])),
        ),
        (
            "stories an object",
            Box::new(|p: &mut Value| p["turns"][0]["stories"] = json!({})),
        ),
        (
            "a story a string",
            Box::new(|p: &mut Value| p["turns"][0]["stories"][0] = json!("S")),
        ),
        (
            "subagents an object",
            Box::new(|p: &mut Value| p["turns"][0]["stories"][0]["subagents"] = json!({})),
        ),
        (
            "a subagent a string",
            Box::new(|p: &mut Value| p["turns"][0]["stories"][0]["subagents"][0] = json!("a")),
        ),
        (
            "model a number",
            Box::new(|p: &mut Value| sub_path(p)["model"] = json!(5)),
        ),
        (
            "model a list",
            Box::new(|p: &mut Value| sub_path(p)["model"] = json!(["Opus"])),
        ),
        (
            "effort a number",
            Box::new(|p: &mut Value| sub_path(p)["effort"] = json!(5)),
        ),
        (
            "effort a boolean",
            Box::new(|p: &mut Value| sub_path(p)["effort"] = json!(true)),
        ),
        (
            "after a string",
            Box::new(|p: &mut Value| sub_path(p)["after"] = json!("b")),
        ),
        (
            "after an object",
            Box::new(|p: &mut Value| sub_path(p)["after"] = json!({})),
        ),
        (
            "an after entry a number",
            Box::new(|p: &mut Value| sub_path(p)["after"] = json!([1])),
        ),
        (
            "an after entry null",
            Box::new(|p: &mut Value| sub_path(p)["after"] = json!([null])),
        ),
    ];
    for (what, edit) in cases {
        let mut p = minimal();
        edit(&mut p);
        assert_eq!(err(&p), SCHEMA, "{what}");
    }
}

#[test]
fn an_optional_member_may_be_absent_or_null() {
    let mut p = minimal();
    ok(&p);
    p["name"] = Value::Null;
    ok(&p);
    sub_path(&mut p)["after"] = Value::Null;
    ok(&p);
    sub_path(&mut p)["after"] = json!([]);
    ok(&p);
}

// ---- The bounds on the arrays ----

#[test]
fn an_empty_list_of_turns_stories_or_subagents_is_refused() {
    assert_eq!(err(&plan(vec![])), SCHEMA);
    assert_eq!(err(&plan(vec![turn("T", vec![])])), SCHEMA);
    assert_eq!(
        err(&plan(vec![turn("T", vec![story("S", vec![])])])),
        SCHEMA
    );
}

#[test]
fn up_to_32_turns_are_accepted_and_33_are_refused() {
    let turns = |n: usize| -> Value {
        plan(
            (0..n)
                .map(|i| turn(&format!("T{i}"), vec![story("S", vec![sub("a")])]))
                .collect(),
        )
    };
    assert_eq!(ok(&turns(1)).busiest(), ("T0", 2));
    assert_eq!(ok(&turns(31)).busiest(), ("T0", 2));
    assert_eq!(ok(&turns(32)).busiest(), ("T0", 2));
    assert_eq!(err(&turns(33)), SCHEMA);
    assert_eq!(err(&turns(100)), SCHEMA);
}

#[test]
fn up_to_32_stories_in_a_turn_are_accepted_and_33_are_refused() {
    let stories = |n: usize| -> Value {
        plan(vec![turn(
            "T",
            (0..n)
                .map(|i| story(&format!("S{i}"), vec![sub("a")]))
                .collect(),
        )])
    };
    assert_eq!(ok(&stories(1)).busiest(), ("T", 2));
    assert_eq!(ok(&stories(32)).busiest(), ("T", 33));
    assert_eq!(err(&stories(33)), SCHEMA);
}

#[test]
fn up_to_32_subagents_in_a_story_are_accepted_and_33_are_refused() {
    assert_eq!(ok(&one_story_plan(antichain(32))).busiest(), ("T1", 33));
    assert_eq!(err(&one_story_plan(antichain(33))), SCHEMA);
    assert_eq!(ok(&one_story_plan(chain(32))).busiest(), ("T1", 2));
    assert_eq!(err(&one_story_plan(chain(33))), SCHEMA);
}

#[test]
fn an_after_of_31_names_is_accepted_and_one_of_32_is_refused() {
    let names = |n: usize| -> Vec<String> { (0..n).map(|i| format!("a{i}")).collect() };
    let after = |n: usize| -> Value {
        let mut subagents = antichain(32);
        let n_names = names(n);
        let refs: Vec<&str> = n_names.iter().map(String::as_str).collect();
        // The last subagent, a31, starts after the first n of the others.
        subagents[31] = sub_after("a31", &refs);
        one_story_plan(subagents)
    };
    assert_eq!(ok(&after(31)).busiest(), ("T1", 32));
    assert_eq!(ok(&after(30)).busiest(), ("T1", 32));
    // 32 distinct names: the 32nd is not the subagent's own story's to name, and the schema
    // refuses the length before any name is looked up.
    assert_eq!(err(&after(32)), SCHEMA);
}

#[test]
fn an_after_that_names_one_subagent_twice_is_refused() {
    let p = one_story_plan(vec![sub("a"), sub_after("b", &["a", "a"])]);
    assert_eq!(err(&p), SCHEMA);
    // It is the schema's fault, reported ahead of the checks after it, a dangling name included.
    let p = one_story_plan(vec![sub("a"), sub_after("b", &["ghost", "ghost"])]);
    assert_eq!(err(&p), SCHEMA);
}

// ---- Names ----

const FORMS: usize = 6;

/// A plan with `value` as a name in the `place`-th position: the plan's, a turn's, a story's, a
/// subagent's, an `after` entry and (for the sixth) the subagent named by that entry.
fn with_name(place: usize, value: &str) -> Value {
    match place {
        0 => named_plan(value, vec![turn("T", vec![story("S", vec![sub("a")])])]),
        1 => plan(vec![turn(value, vec![story("S", vec![sub("a")])])]),
        2 => plan(vec![turn("T", vec![story(value, vec![sub("a")])])]),
        3 => one_story_plan(vec![sub(value)]),
        // The entry may name no subagent at all; used for refusals, where the schema is first.
        4 => one_story_plan(vec![sub("a"), sub_after("b", &[value])]),
        5 => one_story_plan(vec![sub(value), sub_after("b", &[value])]),
        _ => unreachable!(),
    }
}

fn accepted_names() -> Vec<String> {
    let mut names: Vec<String> = [
        "a",
        "A",
        "!",
        "~",
        "0",
        "T03",
        "E0.09-T03-S01",
        "test author",
        "a  b",
        "it's",
        "say \"x\"",
        "back\\slash",
        "slash/",
        "<tag>",
        "a+b",
        "100%",
        "{}",
        "[1]",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    names.push("x".repeat(64));
    names.push(format!("a{}b", " ".repeat(62)));
    names.push(format!("!{}~", "m".repeat(62)));
    names
}

fn refused_names() -> Vec<String> {
    let mut names: Vec<String> = [
        "",
        " ",
        "  ",
        " a",
        "a ",
        " a ",
        "a\n",
        "\na",
        "a\nb",
        "a\t",
        "a\tb",
        "a\r",
        "\u{7f}",
        "a\u{7f}",
        "a\u{0}",
        "\u{1}",
        "\u{e9}",
        "caf\u{e9}",
        "\u{3000}",
        "a\u{a0}b",
        "a\u{feff}",
        "a\u{2028}b",
        "\u{1F600}",
        "a\u{200b}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    names.push("x".repeat(65));
    names.push("x".repeat(100));
    names.push(format!("a{}b", " ".repeat(63)));
    names.push(format!("{}a", " ".repeat(63)));
    names
}

#[test]
fn names_of_the_right_form_are_accepted_in_every_place() {
    for value in accepted_names() {
        // Place 4 names a subagent that need not exist, so it is for refusals only.
        for place in [0, 1, 2, 3, 5] {
            let p = with_name(place, &value);
            let parsed = ok(&p);
            if place == 0 {
                assert_eq!(parsed.name(), Some(value.as_str()));
            }
        }
    }
}

#[test]
fn names_of_the_wrong_form_are_refused_in_every_place() {
    for value in refused_names() {
        for place in 0..FORMS {
            let p = with_name(place, &value);
            assert_eq!(err(&p), SCHEMA, "place {place}, name {value:?}");
        }
    }
}

#[test]
fn a_name_of_exactly_64_characters_is_the_most() {
    assert_eq!(
        ok(&with_name(1, &"n".repeat(64))).busiest().0,
        "n".repeat(64)
    );
    assert_eq!(err(&with_name(1, &"n".repeat(65))), SCHEMA);
}

#[test]
fn a_name_is_never_trimmed_or_case_folded() {
    // Neither " T" nor "T " is a name, so a plan that writes one is refused, not corrected.
    assert_eq!(err(&with_name(1, " T")), SCHEMA);
    assert_eq!(err(&with_name(1, "T ")), SCHEMA);
    let p = plan(vec![turn("MiXeD Case", vec![story("S", vec![sub("a")])])]);
    assert_eq!(ok(&p).busiest().0, "MiXeD Case");
}

#[test]
fn a_trailing_line_feed_is_not_let_through_by_the_end_anchor() {
    // The $ of a pattern matches before a final line feed in some engines (#0029).
    for place in 0..FORMS {
        assert_eq!(err(&with_name(place, "name\n")), SCHEMA, "place {place}");
    }
    let mut p = minimal();
    sub_path(&mut p)["model"] = json!("Opus\n");
    assert_eq!(err(&p), SCHEMA);
    let mut p = minimal();
    sub_path(&mut p)["effort"] = json!("high\n");
    assert_eq!(err(&p), SCHEMA);
}

// ---- Model and effort ----

fn with_model(value: &str) -> Value {
    let mut p = minimal();
    sub_path(&mut p)["model"] = json!(value);
    p
}

fn with_effort(value: &str) -> Value {
    let mut p = minimal();
    sub_path(&mut p)["effort"] = json!(value);
    p
}

#[test]
fn a_model_takes_the_form_of_a_name() {
    let mut accepted: Vec<String> = [
        "Opus",
        "Opus 5.5",
        "Opus 5.5 +24h",
        "Sol 6 2026-11-15 09:00 ET",
        "Fable",
        "x",
        "a b",
        "claude-sonnet-5-5",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    accepted.push("m".repeat(64));
    for value in accepted {
        let p = with_model(&value);
        let parsed = ok(&p);
        // Recorded as given.
        assert_eq!(parsed.document(), &p, "{value}");
    }
    let mut refused: Vec<String> = [
        "",
        " ",
        " Opus",
        "Opus ",
        "Opus\n",
        "Opus\t",
        "\u{e9}",
        "Op\u{7f}us",
        "a\u{0}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    refused.push("m".repeat(65));
    for value in refused {
        assert_eq!(err(&with_model(&value)), SCHEMA, "{value:?}");
    }
}

#[test]
fn an_effort_is_letters_digits_dot_underscore_and_hyphen() {
    let mut accepted: Vec<String> = [
        "high", "medium", "xhigh", "Low", "0", "a", "9", "x.y", "x_y", "x-y", "A9._-", "0.5",
        "x.y_z-1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    accepted.push(format!("e{}", "x".repeat(31)));
    for value in accepted {
        let p = with_effort(&value);
        assert_eq!(ok(&p).document(), &p, "{value}");
    }
    let mut refused: Vec<String> = [
        "", " ", ".", ".a", "_a", "-a", "a b", " high", "high ", "a+b", "a/b", "a:b", "a,b",
        "high\n", "\u{e9}", "a\u{e9}", "a\u{0}", "a\tb",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    refused.push(format!("e{}", "x".repeat(32)));
    refused.push("e".repeat(64));
    for value in refused {
        assert_eq!(err(&with_effort(&value)), SCHEMA, "{value:?}");
    }
}

#[test]
fn an_effort_of_32_characters_is_the_most_and_a_model_of_64() {
    ok(&with_effort(&"e".repeat(32)));
    assert_eq!(err(&with_effort(&"e".repeat(33))), SCHEMA);
    ok(&with_model(&"m".repeat(64)));
    assert_eq!(err(&with_model(&"m".repeat(65))), SCHEMA);
}

#[test]
fn model_and_effort_are_kept_exactly_and_are_free_of_any_list() {
    // Neither is checked against names of models or efforts, so new ones need no new release.
    let p = with_model("NoSuchModel 99.9 +1h");
    assert_eq!(ok(&p).document(), &p);
    let p = with_effort("ultra-9000");
    assert_eq!(ok(&p).document(), &p);
}

// ---- Faults and the first check that fails ----

#[test]
fn a_schema_fault_is_reported_ahead_of_checks_7_to_11() {
    // A dangling after and a cycle beside a bad effort: check 6 is the first to fail.
    let mut p = one_story_plan(vec![sub_after("a", &["a"]), sub_after("b", &["ghost"])]);
    sub_path(&mut p)["effort"] = json!("not ok");
    assert_eq!(err(&p), SCHEMA);
}

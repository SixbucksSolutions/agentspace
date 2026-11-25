//! `agentspace lease create --plan` and contract v1's plan lease (E0.10-T03-S01, #0219).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (contract v1: Fields, Plan, `create`, Exit codes and errors, Messages,
//! Schemas, Versioning), docs/state.md (What a plan lease adds) and D73 to D76 and D81 in
//! docs/decisions.md, never from the program's code. The driver is a fake that logs every call, and
//! each test runs through `agentspace_cli::execute_in` in its own throwaway folder under the temp
//! folder, but for the tests that start the built binary, which run it in a throwaway home with
//! every variable it reads passed through `Command::env`.
//!
//! A test never prints an environment's values, and a plan file's text is never put in a failure
//! message: a plan holds no secret, but its model and effort are in no output (D74), and a test
//! that shows it shows it by the marker words it planted.

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use agentspace_driver_api::Placed;
use common::{
    MSG_NO_PROJECT, Run, case, check_usage_cases, failure_problems, msg_empty_value,
    msg_needs_value, msg_twice, msg_unexpected, msg_unknown_option_sub, not_unicode,
};
use fake_driver::{Call, Handle, reason};
use lab::{
    Lab, MSG_NO_DRIVER, NOW, Ran, msg_bad_platform, msg_bad_profile, msg_bad_project,
    msg_not_ready, msg_ttl_no_unit, msg_ttl_range, schema_problems, uuid_n,
};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// ---- What the spec words --------------------------------------------------------------------------

/// lease-api.md, Messages: `--plan` with `--size`.
const MSG_PLAN_AND_SIZE: &str = "Options --plan and --size cannot be given together; the plan picks the size. Run 'agentspace lease create --help'.";

/// lease-api.md, Plan: the `<why>` of each check that has no name in it.
const WHY_LARGER: &str = "the file is larger than 262144 bytes";
const WHY_READ: &str = "the file could not be read";
const WHY_UTF8: &str = "the file is not valid UTF-8";
const WHY_JSON: &str = "the file is not valid JSON";
const WHY_SCHEMA: &str = "the document does not match the plan schema";

/// The most bytes a plan file MAY hold, a byte order mark counted (lease-api.md, Plan).
const MAX_FILE: usize = 262_144;

/// lease-api.md, Messages: `Plan '<value>' is not a valid plan: <why>.`
fn msg_plan(value: &str, why: &str) -> String {
    format!("Plan '{value}' is not a valid plan: {why}.")
}

/// lease-api.md, Fields: the message of `plan_too_large`, with 32 the most `xl` holds today.
fn msg_too_large(turn: &str, agents: usize) -> String {
    format!(
        "Plan's busiest turn '{turn}' has {agents} agents at once; the largest size, xl, holds 32."
    )
}

// ---- Building plans ---------------------------------------------------------------------------------

fn sub(name: &str) -> Value {
    json!({"name": name, "model": "Opus", "effort": "high"})
}

fn sub_after(name: &str, after: &[&str]) -> Value {
    json!({"name": name, "model": "Opus", "effort": "high", "after": after})
}

fn story(name: &str, subagents: Vec<Value>) -> Value {
    json!({"name": name, "subagents": subagents})
}

fn turn(name: &str, stories: Vec<Value>) -> Value {
    json!({"name": name, "stories": stories})
}

fn plan_of(turns: Vec<Value>) -> Value {
    json!({"turns": turns})
}

fn named(name: &str, turns: Vec<Value>) -> Value {
    json!({"name": name, "turns": turns})
}

/// A story of `n` subagents none of which starts after another: its peak is `n`.
fn wide(name: &str, n: usize) -> Value {
    story(
        name,
        (0..n).map(|i| sub(&format!("a{i}"))).collect::<Vec<_>>(),
    )
}

/// A plan of one turn, `T1`, with this many agents at once, its dev lead counted: stories of up to
/// 32 subagents, none starting after another (a plan has at most 32 stories, so up to 1025).
fn plan_with_agents(agents: usize) -> Value {
    assert!((2..=1025).contains(&agents));
    let mut peak = agents - 1;
    let mut stories = Vec::new();
    while peak > 0 {
        let n = peak.min(32);
        stories.push(wide(&format!("S{}", stories.len()), n));
        peak -= n;
    }
    plan_of(vec![turn("T1", stories)])
}

/// A plan of one turn, `T1`, with one story whose subagents start after one another as given.
fn one_story(subagents: Vec<Value>) -> Value {
    plan_of(vec![turn("T1", vec![story("S1", subagents)])])
}

/// A plan that holds every rule and nothing else: the base of the refusals below.
fn base() -> Value {
    json!({"name": "v", "turns": [{"name": "T1", "stories": [{"name": "S1", "subagents": [
        {"name": "a", "model": "Opus", "effort": "high"}]}]}]})
}

/// The base plan with the member at `path` set to `value`, added when it is absent. An empty path
/// replaces the whole document.
fn with(path: &str, value: Value) -> Value {
    let mut v = base();
    if path.is_empty() {
        return value;
    }
    let (parent, key) = path.rsplit_once('/').expect("A path has a slash");
    match v.pointer_mut(parent).expect("The parent is in the base") {
        Value::Object(m) => {
            m.insert(key.to_string(), value);
        }
        Value::Array(a) => a[key.parse::<usize>().expect("An index")] = value,
        other => panic!("{other:?} holds no members"),
    }
    v
}

/// The base plan with the member at `path` removed.
fn without(path: &str) -> Value {
    let mut v = base();
    let (parent, key) = path.rsplit_once('/').expect("A path has a slash");
    match v.pointer_mut(parent).expect("The parent is in the base") {
        Value::Object(m) => {
            m.shift_remove(key).expect("The member is in the base");
        }
        other => panic!("{other:?} holds no members"),
    }
    v
}

fn bytes_of(plan: &Value) -> Vec<u8> {
    serde_json::to_vec(plan).expect("A plan can be written")
}

/// A plan file's bytes, padded with trailing spaces to exactly `total` bytes, a byte order mark
/// counted when there is one.
fn padded(plan: &Value, total: usize, bom: bool) -> Vec<u8> {
    let mut b = Vec::new();
    if bom {
        b.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    b.extend(bytes_of(plan));
    assert!(b.len() <= total, "The plan is larger than the file asked");
    b.resize(total, b' ');
    b
}

// ---- Running create ---------------------------------------------------------------------------------

/// The program's own file in the lab's `bin` folder, which a plan lease's `create` looks beside for
/// agentspace's ProxyCommand program (lease-api.md, Messages: "Way to reach the host not found").
const PROXY_PROGRAM: &str = "agentspace-ssh-proxy.exe";

/// A lab with one fake driver, as `Lab::with_fake` makes, and the ProxyCommand program installed
/// beside the program, as an install has it, so that a `create` that makes the checks that come
/// before a driver is asked passes them. The tests of the checks themselves use `Lab::with_fake`.
fn installed_lab() -> (Lab, Handle) {
    let (lab, h) = Lab::with_fake();
    install_proxy(&lab);
    (lab, h)
}

fn install_proxy(lab: &Lab) {
    let bin = lab.exe.parent().expect("The program has a folder");
    std::fs::create_dir_all(bin).expect("The program's folder can be made");
    std::fs::write(bin.join(PROXY_PROGRAM), b"").expect("The proxy program can be written");
}

fn write_file(lab: &Lab, name: &str, bytes: &[u8]) {
    std::fs::write(lab.cwd.join(name), bytes).expect("The plan file can be written");
}

/// Runs `create --project swim --plan <name>` and more arguments, with the plan written to `name`.
fn create_plan(lab: &Lab, plan: &Value, more: &[&str]) -> Ran {
    write_file(lab, "p.json", &bytes_of(plan));
    create_file(lab, "p.json", more)
}

fn create_file(lab: &Lab, arg: &str, more: &[&str]) -> Ran {
    let mut line = vec!["lease", "create", "--project", "swim", "--plan", arg];
    line.extend_from_slice(more);
    lab.run(&line)
}

fn place_calls(h: &Handle) -> Vec<Call> {
    h.calls()
        .into_iter()
        .filter(|c| matches!(c, Call::Place { .. }))
        .collect()
}

/// The size a `place` call was asked for.
fn placed_size(call: &Call) -> String {
    match call {
        Call::Place { size, .. } => size.clone(),
        other => panic!("Not a place call: {other:?}"),
    }
}

/// A run of a command that was meant to fail, as `Run`.
fn as_run(ran: &Ran) -> Run {
    Run {
        status: Some(i32::from(ran.code)),
        stdout: ran.stdout.clone(),
        stderr: ran.stderr.clone(),
    }
}

/// The problems with a run that was meant to end in this error: the shape and the exit code of
/// lease-api.md's Output, the message given, and the error document against v1's error schema.
fn error_problems(ran: &Ran, code: &str, field: Option<&str>, message: &str) -> Vec<String> {
    error_doc_problems(ran, code, field, Some(message))
}

/// `error_problems` with a message that is only compared when one is given.
fn error_doc_problems(
    ran: &Ran,
    code: &str,
    field: Option<&str>,
    message: Option<&str>,
) -> Vec<String> {
    let mut p = failure_problems(&as_run(ran), code, field, message);
    if let Ok(doc) = serde_json::from_slice::<Value>(ran.stderr.trim_ascii_end()) {
        p.extend(schema_problems("error.output.schema.json", &doc));
    }
    p
}

/// Fails once with every problem listed.
#[track_caller]
fn assert_no_problems(problems: Vec<String>) {
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// `expect_error`, and the error document against v1's error schema.
#[track_caller]
fn expect_error_v1(ran: &Ran, code: &str, field: Option<&str>, message: &str) -> Value {
    let doc = ran.expect_error(code, field, Some(message));
    let p = schema_problems("error.output.schema.json", &doc);
    assert!(p.is_empty(), "The error document breaks v1's schema: {p:?}");
    doc
}

fn reason_codes(doc: &Value) -> Vec<String> {
    doc["reasons"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| r["code"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// What the lease state file holds, as the state schema reads it.
#[track_caller]
fn assert_state_file_valid(lab: &Lab) {
    let bytes = lab.state_bytes().expect("The state file is there");
    let doc: Value = serde_json::from_slice(&bytes).expect("The state file is JSON");
    let p = schema_problems("state/leases.state.schema.json", &doc);
    assert!(p.is_empty(), "The state file breaks its schema: {p:?}");
}

// ---- Which size a plan gets -----------------------------------------------------------------------------

#[test]
fn a_plan_gets_the_smallest_size_that_holds_its_agents_at_once() {
    // lease-api.md, Plan, today's footprint (D73): 2 to 4 agents `s`, 5 to 8 `m`, 9 to 16 `l`,
    // 17 to 32 `xl`. Each count is a plan of its own, and the size is the lease's, the record's and
    // the one the driver is asked for.
    let table = [
        (2, "s"),
        (3, "s"),
        (4, "s"),
        (5, "m"),
        (8, "m"),
        (9, "l"),
        (16, "l"),
        (17, "xl"),
        (32, "xl"),
    ];
    let mut problems = Vec::new();
    for (agents, size) in table {
        let (lab, h) = installed_lab();
        let ran = create_plan(&lab, &plan_with_agents(agents), &[]);
        if ran.code != 0 {
            problems.push(format!(
                "{agents} agents: exit {}, stderr {}",
                ran.code,
                ran.stderr_text()
            ));
            continue;
        }
        let doc = ran.lease_doc();
        if doc["size"] != size {
            problems.push(format!("{agents} agents: size {}, not {size}", doc["size"]));
        }
        if doc["sized_for"] != json!({"turn": "T1", "agents": agents}) {
            problems.push(format!("{agents} agents: sized_for {}", doc["sized_for"]));
        }
        if doc["state"] != "ready" {
            problems.push(format!("{agents} agents: state {}", doc["state"]));
        }
        let calls = place_calls(&h);
        if calls.len() != 1 || placed_size(&calls[0]) != size {
            problems.push(format!("{agents} agents: the driver was asked {calls:?}"));
        }
        let record = lab.records().remove(0);
        if record["size"] != size {
            problems.push(format!(
                "{agents} agents: the record's size {}",
                record["size"]
            ));
        }
    }
    assert_no_problems(problems);
}

#[test]
fn two_plans_of_different_peaks_get_different_sizes() {
    let (lab, h) = installed_lab();
    let small = create_plan(&lab, &plan_with_agents(4), &[]).lease_doc();
    let large = create_plan(&lab, &plan_with_agents(17), &[]).lease_doc();
    assert_eq!(small["size"], "s");
    assert_eq!(large["size"], "xl");
    assert_ne!(small["sized_for"]["agents"], large["sized_for"]["agents"]);
    let sizes: Vec<String> = place_calls(&h).iter().map(placed_size).collect();
    assert_eq!(sizes, ["s", "xl"]);
}

#[test]
fn a_plan_with_more_agents_than_the_largest_size_holds_is_never_given_less() {
    // 32 agents are the most `xl` holds, 33 the least that no size does.
    let (lab, h) = installed_lab();
    let doc = create_plan(&lab, &plan_with_agents(32), &[]).lease_doc();
    assert_eq!(doc["size"], "xl");
    assert_eq!(doc["state"], "ready");
    let doc = create_plan(&lab, &plan_with_agents(33), &[]).lease_doc();
    assert_eq!(doc["state"], "unsatisfiable");
    assert_eq!(doc["size"], "xl");
    assert_eq!(
        place_calls(&h).len(),
        1,
        "Only the plan that fits is placed"
    );
}

// ---- How a story's peak and a turn's agents are counted ------------------------------------------------

#[test]
fn a_storys_peak_is_the_width_of_its_starts_never_a_stated_number() {
    // lease-api.md, Plan: the most subagents in a set of which none starts after another, directly
    // or through others. A turn's agents are the sum of its stories' peaks and 1 for its dev lead.
    let cases: Vec<(&str, Value, u64)> = vec![
        (
            "#0203's plan from the spec: three at once, the reviewer after all three",
            plan_of(vec![turn(
                "T03",
                vec![story(
                    "E0.09-T03-S01",
                    vec![
                        json!({"name":"implementer","model":"Opus","effort":"high"}),
                        json!({"name":"test author","model":"Sonnet","effort":"high"}),
                        json!({"name":"runner","model":"Sonnet","effort":"medium"}),
                        json!({"name":"reviewer","model":"Opus","effort":"high","after":["implementer","test author","runner"]}),
                    ],
                )],
            )]),
            4,
        ),
        (
            "a chain, each after the one before: one at a time",
            one_story(vec![
                sub("a"),
                sub_after("b", &["a"]),
                sub_after("c", &["b"]),
                sub_after("d", &["c"]),
            ]),
            2,
        ),
        (
            "a chain's first and last are not at once, since the last starts after it through others",
            one_story(vec![
                sub("a"),
                sub_after("b", &["a"]),
                sub_after("c", &["b"]),
            ]),
            2,
        ),
        (
            "no after at all: every subagent at once",
            one_story((0..5).map(|i| sub(&format!("a{i}"))).collect()),
            6,
        ),
        (
            "a diamond: the middle two at once",
            one_story(vec![
                sub("a"),
                sub_after("b", &["a"]),
                sub_after("c", &["a"]),
                sub_after("d", &["b", "c"]),
            ]),
            3,
        ),
        (
            "a width that no layer holds: x, y and z are at once, though each layer has two",
            one_story(vec![
                sub("x"),
                sub("w"),
                sub_after("y", &["w"]),
                sub_after("v", &["w"]),
                sub_after("z", &["v"]),
            ]),
            4,
        ),
        (
            "an empty after is the same as none",
            one_story(vec![
                json!({"name":"a","model":"Opus","effort":"high","after":[]}),
                json!({"name":"b","model":"Opus","effort":"high","after":[]}),
            ]),
            3,
        ),
        (
            "a null after is the same as none",
            one_story(vec![
                json!({"name":"a","model":"Opus","effort":"high","after":null}),
                json!({"name":"b","model":"Opus","effort":"high","after":null}),
            ]),
            3,
        ),
        (
            "stories of one turn add: peaks 2, 3 and 3, and the dev lead",
            plan_of(vec![turn(
                "T1",
                vec![wide("S1", 2), wide("S2", 3), wide("S3", 3)],
            )]),
            9,
        ),
        (
            "a story of one subagent and the dev lead are the fewest, 2",
            one_story(vec![sub("a")]),
            2,
        ),
    ];
    let mut problems = Vec::new();
    for (what, plan, want) in cases {
        let (lab, _h) = installed_lab();
        let ran = create_plan(&lab, &plan, &[]);
        if ran.code != 0 {
            problems.push(format!("{what}: exit {}, {}", ran.code, ran.stderr_text()));
            continue;
        }
        let got = ran.lease_doc()["sized_for"]["agents"].as_u64();
        if got != Some(want) {
            problems.push(format!("{what}: {got:?} agents, not {want}"));
        }
    }
    assert_no_problems(problems);
}

#[test]
fn the_busiest_turn_is_the_first_such_turn_on_a_tie() {
    let plan = named(
        "v0.10",
        vec![
            turn("A", vec![wide("S", 4)]),
            turn("B", vec![wide("S", 6)]),
            turn("C", vec![wide("S", 6)]),
            turn("D", vec![wide("S", 2)]),
        ],
    );
    let (lab, _h) = installed_lab();
    let doc = create_plan(&lab, &plan, &[]).lease_doc();
    assert_eq!(
        doc["sized_for"],
        json!({"plan": "v0.10", "turn": "B", "agents": 7})
    );
    assert_eq!(doc["size"], "m");
}

#[test]
fn a_lease_is_sized_for_the_whole_plan_by_its_busiest_turn_alone() {
    // The first turn is the small one: nothing sizes for its sum, or its first turn, or its last.
    let plan = plan_of(vec![
        turn("T1", vec![wide("S", 3)]),
        turn("T2", vec![wide("S", 12)]),
        turn("T3", vec![wide("S", 3)]),
    ]);
    let (lab, _h) = installed_lab();
    let doc = create_plan(&lab, &plan, &[]).lease_doc();
    assert_eq!(doc["sized_for"], json!({"turn": "T2", "agents": 13}));
    assert_eq!(doc["size"], "l");
}

// ---- sized_for: what it holds, and in which states -----------------------------------------------------

#[test]
fn sized_for_prints_the_plans_name_and_the_turns_name_exactly_as_the_plan_writes_them() {
    let cases = [
        // A name MAY hold spaces inside it, any case, any printable ASCII.
        ("test author", "T 03", "t 03"),
        ("a \"quoted\" \\ name <&>", "T \"x\" \\ y", "T"),
        ("N", "t03", "T03"),
    ];
    for (plan_name, turn_name, other) in cases {
        let plan = named(
            plan_name,
            vec![
                turn(turn_name, vec![wide("S", 6)]),
                turn(other, vec![wide("S", 2)]),
            ],
        );
        let (lab, _h) = installed_lab();
        let doc = create_plan(&lab, &plan, &[]).lease_doc();
        assert_eq!(
            doc["sized_for"],
            json!({"plan": plan_name, "turn": turn_name, "agents": 7})
        );
    }
    // 64 characters, the most a name holds.
    let long = format!("x{}y", "-".repeat(62));
    assert_eq!(long.len(), 64);
    let (lab, _h) = installed_lab();
    let doc = create_plan(
        &lab,
        &named(&long, vec![turn(&long, vec![wide("S", 1)])]),
        &[],
    )
    .lease_doc();
    assert_eq!(doc["sized_for"]["plan"], long.as_str());
    assert_eq!(doc["sized_for"]["turn"], long.as_str());
}

#[test]
fn sized_for_has_no_plan_member_when_the_plans_name_is_absent_or_null() {
    for plan in [
        plan_of(vec![turn("T1", vec![wide("S", 2)])]),
        json!({"name": null, "turns": [turn("T1", vec![wide("S", 2)])]}),
    ] {
        let (lab, _h) = installed_lab();
        let ran = create_plan(&lab, &plan, &[]);
        let doc = ran.lease_doc();
        assert_eq!(doc["sized_for"], json!({"turn": "T1", "agents": 3}));
        assert!(
            doc["sized_for"].get("plan").is_none(),
            "A plan with no name is left out, never written as null"
        );
        assert!(
            ran.stdout_text().contains("\"sized_for\":{"),
            "sized_for is an object"
        );
        // The record keeps the plan as parsed, a null name included (state.md).
        assert_eq!(lab.records()[0]["plan"], plan);
        assert!(lab.records()[0]["sized_for"].get("plan").is_none());
    }
}

#[test]
fn a_lease_made_without_a_plan_has_no_sized_for_and_its_record_no_plan() {
    let (lab, _h) = installed_lab();
    let doc = lab
        .run(&["lease", "create", "--project", "swim", "--size", "l"])
        .lease_doc();
    assert!(doc.get("sized_for").is_none());
    assert_eq!(doc["size"], "l");
    let record = lab.records().remove(0);
    assert!(record.get("sized_for").is_none());
    assert!(record.get("plan").is_none());
}

#[test]
fn sized_for_is_printed_as_create_set_it_in_every_state_by_every_command() {
    let (mut lab, h) = installed_lab();
    let plan_a = named("pa", vec![turn("Ta", vec![wide("S", 3)])]);
    let plan_b = named("pb", vec![turn("Tb", vec![wide("S", 5), wide("S2", 2)])]);
    let plan_c = named("pc", vec![turn("Tc", vec![wide("S", 9)])]);
    let plan_d = named("pd", vec![turn("Td", vec![wide("S", 20), wide("S2", 13)])]);
    let plan_e = plan_of(vec![turn("Te", vec![wide("S", 1)])]);
    let plan_f = named("pf", vec![turn("Tf", vec![wide("S", 2)])]);

    // A is ready, B pending, C unsatisfiable by the driver, D unsatisfiable for its plan, E ready
    // for a minute, F ready and then released.
    let a = create_plan(&lab, &plan_a, &[]).lease_doc();
    h.script_place(Ok(Placed::Pending));
    let b = create_plan(&lab, &plan_b, &[]).lease_doc();
    h.script_place(Ok(Placed::Unsatisfiable {
        reasons: vec![reason(
            "unsupported_platform",
            "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only.",
        )],
    }));
    let c = create_plan(&lab, &plan_c, &["--platform", "windows/amd64"]).lease_doc();
    let d = create_plan(&lab, &plan_d, &[]).lease_doc();
    let e = create_plan(&lab, &plan_e, &["--ttl", "1m"]).lease_doc();
    let f_created = create_plan(&lab, &plan_f, &[]).lease_doc();

    assert_eq!(a["state"], "ready");
    assert_eq!(
        a["sized_for"],
        json!({"plan": "pa", "turn": "Ta", "agents": 4})
    );
    assert_eq!(b["state"], "pending");
    assert_eq!(
        b["sized_for"],
        json!({"plan": "pb", "turn": "Tb", "agents": 8}),
        "A pending lease says what it was sized for"
    );
    assert_eq!(b["size"], "m");
    assert_eq!(c["state"], "unsatisfiable");
    assert_eq!(
        c["sized_for"],
        json!({"plan": "pc", "turn": "Tc", "agents": 10})
    );
    assert_eq!(c["size"], "l");
    assert_eq!(reason_codes(&c), ["unsupported_platform"]);
    assert_eq!(d["state"], "unsatisfiable");
    assert_eq!(d["size"], "xl");
    assert_eq!(
        d["sized_for"],
        json!({"plan": "pd", "turn": "Td", "agents": 34})
    );
    assert_eq!(reason_codes(&d), ["plan_too_large"]);
    let id = |doc: &Value| doc["id"].as_str().unwrap().to_string();
    let (ia, ib, ic, id_d, ie, if_) = (id(&a), id(&b), id(&c), id(&d), id(&e), id(&f_created));

    // get: the same document, whatever the state.
    for doc in [&a, &b, &c, &d] {
        let got = lab
            .run(&["lease", "get", doc["id"].as_str().unwrap()])
            .lease_doc();
        assert_eq!(&got, doc, "get prints the lease as create did");
    }

    // list: every lease, each with its own sized_for, in the order of the contract.
    let listed = lab.run(&["lease", "list"]).list_doc();
    let leases = listed["leases"].as_array().unwrap();
    assert_eq!(leases.len(), 6);
    for doc in [&a, &b, &c, &d, &e, &f_created] {
        let found = leases
            .iter()
            .find(|l| l["id"] == doc["id"])
            .expect("listed");
        assert_eq!(found, doc);
    }
    // The state filters decide as get does, and print the same.
    let unsat = lab
        .run(&["lease", "list", "--state", "unsatisfiable"])
        .list_doc();
    assert_eq!(unsat["leases"].as_array().unwrap().len(), 2);
    assert!(
        unsat["leases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l.get("sized_for").is_some())
    );

    // renew: a ready lease prints its sized_for unchanged, and every other state refuses.
    let renewed = lab.run(&["lease", "renew", &ia, "--ttl", "2h"]).lease_doc();
    assert_eq!(renewed["sized_for"], a["sized_for"]);
    assert_eq!(renewed["size"], a["size"]);
    assert_eq!(renewed["ttl_seconds"], 7200);
    let refusal = lab.run(&["lease", "renew", &id_d]);
    let doc = expect_error_v1(
        &refusal,
        "invalid_state",
        None,
        &msg_not_ready(&id_d, "unsatisfiable"),
    );
    assert!(
        doc.get("sized_for").is_none(),
        "An error carries no sized_for"
    );
    let refusal = lab.run(&["lease", "renew", &ib]);
    expect_error_v1(
        &refusal,
        "invalid_state",
        None,
        &msg_not_ready(&ib, "pending"),
    );

    // release: ready and pending become released and keep sized_for, unsatisfiable stay as they were.
    let released_a = lab.run(&["lease", "release", &ia]).lease_doc();
    assert_eq!(released_a["state"], "released");
    assert_eq!(released_a["sized_for"], a["sized_for"]);
    let released_b = lab.run(&["lease", "release", &ib]).lease_doc();
    assert_eq!(released_b["state"], "released");
    assert_eq!(released_b["sized_for"], b["sized_for"]);
    assert!(released_b.get("expires_at").is_none());
    let released_c = lab.run(&["lease", "release", &ic]).lease_doc();
    assert_eq!(released_c, c);
    let released_d = lab.run(&["lease", "release", &id_d]).lease_doc();
    assert_eq!(released_d, d);
    let released_f = lab.run(&["lease", "release", &if_]).lease_doc();
    assert_eq!(released_f["state"], "released");
    assert_eq!(released_f["sized_for"], f_created["sized_for"]);
    let again = lab.run(&["lease", "get", &if_]).lease_doc();
    assert_eq!(again, released_f);

    // expired: a minute's lease, read after it ended.
    lab.set_now(NOW + 3600);
    let expired = lab.run(&["lease", "get", &ie]).lease_doc();
    assert_eq!(expired["state"], "expired");
    assert!(expired.get("ssh").is_none());
    assert_eq!(expired["sized_for"], e["sized_for"]);
    assert_eq!(expired["size"], e["size"]);
    let in_list = lab.run(&["lease", "list", "--state", "expired"]).list_doc();
    assert_eq!(in_list["leases"], json!([expired.clone()]));
    let released_e = lab.run(&["lease", "release", &ie]).lease_doc();
    assert_eq!(released_e["state"], "released");
    assert_eq!(released_e["sized_for"], e["sized_for"]);

    // The driver was asked to place A, B, C, E and F, and never D.
    assert_eq!(place_calls(&h).len(), 5);
    assert!(
        !place_calls(&h)
            .iter()
            .any(|c| matches!(c, Call::Place { size, .. } if size == "xl")),
        "No driver was asked for the too-large plan"
    );

    // The records keep sized_for and the plan, and the state file is valid.
    assert_state_file_valid(&lab);
    for (doc, plan) in [
        (&a, &plan_a),
        (&b, &plan_b),
        (&c, &plan_c),
        (&d, &plan_d),
        (&e, &plan_e),
        (&f_created, &plan_f),
    ] {
        let record = lab
            .record_of(doc["id"].as_str().unwrap())
            .expect("A record");
        assert_eq!(record["sized_for"], doc["sized_for"]);
        assert_eq!(&record["plan"], plan, "The record keeps the plan, whole");
        assert_eq!(record["driver"], "fake");
    }
}

#[test]
fn no_output_prints_the_plan_a_model_an_effort_or_a_subagent() {
    // lease-api.md, Fields and Plan: the plan itself, and each subagent's model and effort, are in
    // no output, an error's message included. Each is planted as a marker word, and a run that
    // prints one fails.
    let plan = json!({"name": "markplan", "turns": [{"name": "markturn", "stories": [
        {"name": "markstory", "subagents": [
            {"name": "marksub1", "model": "markmodel 9.9", "effort": "markeffort"},
            {"name": "marksub2", "model": "markmodel 9.9", "effort": "markeffort", "after": ["marksub1"]}]}]}]});
    let markers = ["markstory", "marksub", "markmodel", "markeffort"];
    let (lab, _h) = installed_lab();
    let ran = create_plan(&lab, &plan, &[]);
    let id = ran.lease_doc()["id"].as_str().unwrap().to_string();
    let mut outputs = vec![ran];
    outputs.push(lab.run(&["lease", "get", &id]));
    outputs.push(lab.run(&["lease", "list"]));
    outputs.push(lab.run(&["lease", "renew", &id]));
    outputs.push(lab.run(&["lease", "release", &id]));
    outputs.push(lab.run(&["lease", "renew", &id]));
    // A refusal names a story by its name, as the spec has it, and never a model or an effort.
    let cyc = json!({"name": "markplan", "turns": [{"name": "markturn", "stories": [
        {"name": "loopstory", "subagents": [
            {"name": "loop1", "model": "markmodel 9.9", "effort": "markeffort", "after": ["loop2"]},
            {"name": "loop2", "model": "markmodel 9.9", "effort": "markeffort", "after": ["loop1"]}]}]}]});
    outputs.push(create_plan(&lab, &cyc, &[]));
    let bad_schema = json!({"name": "markplan", "turns": [{"name": "markturn", "stories": [
        {"name": "schemastory", "subagents": [
            {"name": "schemasub", "model": "markmodel 9.9", "effort": "mark effort!"}]}]}]});
    outputs.push(create_plan(&lab, &bad_schema, &[]));
    let mut problems = Vec::new();
    for (i, ran) in outputs.iter().enumerate() {
        for text in [ran.stdout_text(), ran.stderr_text()] {
            for marker in markers {
                if text.contains(marker) {
                    problems.push(format!("Output {i} prints the marker '{marker}'"));
                }
            }
        }
    }
    assert_no_problems(problems);
}

// ---- What a plan lease is sent to the driver as --------------------------------------------------------

#[test]
fn the_other_flags_apply_to_a_plan_lease_as_they_do_without_a_plan() {
    let (lab, h) = installed_lab();
    write_file(&lab, "p.json", &bytes_of(&plan_with_agents(6)));
    let ran = lab.run(&[
        "lease",
        "create",
        "--project",
        " Swim-Coach\n",
        "--platform",
        " Linux/AARCH64/v8 ",
        "--profile",
        "Agent-Linux@03",
        "--ttl",
        "1h 30m",
        "--plan",
        "p.json",
    ]);
    let doc = ran.lease_doc();
    assert_eq!(doc["project"], "swim-coach");
    assert_eq!(doc["platform"], "linux/arm64");
    assert_eq!(doc["profile"], "agent-linux@3");
    assert_eq!(doc["ttl_seconds"], 5400);
    assert_eq!(doc["size"], "m");
    let calls = place_calls(&h);
    assert_eq!(calls.len(), 1);
    assert!(
        matches!(
            &calls[0],
            Call::Place { project, platform, profile, size, ttl_seconds, .. }
                if project == "swim-coach" && platform == "linux/arm64"
                    && profile == "agent-linux@3" && size == "m" && *ttl_seconds == 5400
        ),
        "{calls:?}"
    );
}

#[test]
fn a_plan_that_fits_is_judged_by_the_driver_as_a_size_lease_of_the_size_picked() {
    // lease-api.md, `create`: no driver is given the plan, and a plan that fits but asks for what
    // cannot be met gets sized_for, the size picked, and the reasons a --size lease would get.
    let (mut lab, h) = installed_lab();
    let reasons = vec![
        reason(
            "unsupported_platform",
            "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only.",
        ),
        reason(
            "unsupported_profile",
            "Profile 'other' is not supported; v0 supports agent-linux with no version only.",
        ),
    ];
    h.script_place(Ok(Placed::Unsatisfiable { reasons }));
    lab.script_ids(&[&uuid_n(1)]);
    let doc = create_plan(
        &lab,
        &plan_with_agents(9),
        &["--platform", "windows/amd64", "--profile", "other"],
    )
    .lease_doc();
    assert_eq!(doc["state"], "unsatisfiable");
    assert_eq!(doc["size"], "l");
    assert_eq!(doc["sized_for"], json!({"turn": "T1", "agents": 9}));
    assert_eq!(
        reason_codes(&doc),
        ["unsupported_platform", "unsupported_profile"]
    );
    assert_eq!(placed_size(&place_calls(&h)[0]), "l");

    // A driver that has not the size picked answers unsupported_size, and nothing is swapped.
    h.script_place(Ok(Placed::Unsatisfiable {
        reasons: vec![reason(
            "unsupported_size",
            "Size 'l' is not supported; v0 supports sizes s, m, l and xl.",
        )],
    }));
    let doc = create_plan(&lab, &plan_with_agents(9), &[]).lease_doc();
    assert_eq!(doc["size"], "l");
    assert_eq!(reason_codes(&doc), ["unsupported_size"]);
    assert!(doc.get("sized_for").is_some());
}

// ---- plan_too_large ------------------------------------------------------------------------------------------

#[test]
fn a_plan_over_32_agents_is_unsatisfiable_with_plan_too_large_as_its_only_reason() {
    let plan = named(
        "big",
        vec![
            turn("Small", vec![wide("S", 3)]),
            turn("Huge turn", vec![wide("S1", 20), wide("S2", 14)]),
        ],
    );
    let (lab, h) = installed_lab();
    let ran = create_plan(&lab, &plan, &[]);
    let doc = ran.lease_doc();
    assert_eq!(ran.code, 0, "An unsatisfiable lease is a successful create");
    assert_eq!(doc["state"], "unsatisfiable");
    assert_eq!(doc["size"], "xl", "The size is xl, the largest");
    assert_eq!(
        doc["sized_for"],
        json!({"plan": "big", "turn": "Huge turn", "agents": 35})
    );
    assert_eq!(
        doc["reasons"],
        json!([{"code": "plan_too_large", "message": msg_too_large("Huge turn", 35)}])
    );
    for absent in ["ssh", "expires_at", "released_at"] {
        assert!(
            doc.get(absent).is_none(),
            "No {absent} on an unsatisfiable lease"
        );
    }
    assert!(h.calls().is_empty(), "No driver is asked to place it");
    assert!(h.contexts().is_empty());

    // The record is all create writes: sized_for, the plan, the reason, the driver's name.
    let record = lab.records().remove(0);
    assert_eq!(record["state"], "unsatisfiable");
    assert_eq!(record["size"], "xl");
    assert_eq!(record["sized_for"], doc["sized_for"]);
    assert_eq!(record["plan"], plan);
    assert_eq!(record["reasons"], doc["reasons"]);
    assert_eq!(record["driver"], "fake");
    assert_state_file_valid(&lab);

    // Nothing is issued: no folder, no key, no alias, no host.
    assert!(!lab.ssh.exists(), "No OpenSSH alias folder is made");
    assert_eq!(
        std::fs::read_dir(&lab.home).unwrap().count(),
        0,
        "Nothing is written to the home"
    );
    let mut state_files: Vec<String> = std::fs::read_dir(&lab.state)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    state_files.sort();
    assert_eq!(state_files, ["leases.json", "leases.lock"]);
}

#[test]
fn plan_too_large_is_alone_whatever_the_platform_the_profile_or_the_ttl() {
    // lease-api.md, Fields: decided from the plan, before any driver is asked, so a too-large plan
    // with `--platform windows/amd64` gives the one reason plan_too_large.
    let plan = plan_with_agents(40);
    let cases: Vec<(Vec<&str>, &str, &str, u64)> = vec![
        (
            vec!["--platform", "windows/amd64"],
            "windows/amd64",
            "agent-linux",
            28800,
        ),
        (
            vec!["--profile", "other@2"],
            "linux/arm64",
            "other@2",
            28800,
        ),
        (
            vec![
                "--platform",
                "windows/amd64",
                "--profile",
                "other",
                "--ttl",
                "1h",
            ],
            "windows/amd64",
            "other",
            3600,
        ),
        (
            vec!["--platform", "Linux/AARCH64"],
            "linux/arm64",
            "agent-linux",
            28800,
        ),
    ];
    for (more, platform, profile, ttl) in cases {
        let (lab, h) = installed_lab();
        let doc = create_plan(&lab, &plan, &more).lease_doc();
        assert_eq!(reason_codes(&doc), ["plan_too_large"], "{more:?}");
        assert_eq!(doc["size"], "xl", "{more:?}");
        assert_eq!(doc["platform"], platform, "{more:?}");
        assert_eq!(doc["profile"], profile, "{more:?}");
        assert_eq!(doc["ttl_seconds"], ttl, "{more:?}");
        assert!(h.calls().is_empty(), "{more:?}");
    }
}

#[test]
fn a_plan_of_1025_agents_prints_the_most_sized_for_holds() {
    // 32 stories of 32 subagents and the dev lead: the largest a plan's bounds allow.
    let plan = plan_with_agents(1025);
    assert_eq!(plan["turns"][0]["stories"].as_array().unwrap().len(), 32);
    let (lab, h) = installed_lab();
    let ran = create_plan(&lab, &plan, &[]);
    let doc = ran.lease_doc();
    assert_eq!(doc["sized_for"], json!({"turn": "T1", "agents": 1025}));
    assert_eq!(
        doc["reasons"],
        json!([{"code": "plan_too_large", "message": msg_too_large("T1", 1025)}])
    );
    assert!(h.calls().is_empty());
}

#[test]
fn a_too_large_plan_still_needs_a_driver_and_every_check_made_before_one_is_asked() {
    // lease-api.md, `create`, "What `create` still does for it": with no driver it fails with
    // `internal` first, and it makes every check that create makes before a driver is asked.
    let plan = plan_with_agents(40);
    let lab = Lab::new();
    write_file(&lab, "p.json", &bytes_of(&plan));
    let ran = lab.run(&["lease", "create", "--project", "swim", "--plan", "p.json"]);
    ran.expect_error("internal", None, Some(MSG_NO_DRIVER));
    assert!(lab.records().is_empty());

    // A state folder that is a file: the records cannot be read or locked, so it is `internal`.
    let (lab, h) = installed_lab();
    std::fs::write(&lab.state, b"not a folder").unwrap();
    let ran = create_plan(&lab, &plan, &[]);
    ran.internal();
    assert!(h.calls().is_empty());

    // The same plan, well formed, with a bad plan in its place: the refusal comes first, since the
    // values are checked before the driver or the records.
    let lab = Lab::new();
    write_file(&lab, "p.json", b"{");
    let ran = lab.run(&["lease", "create", "--project", "swim", "--plan", "p.json"]);
    expect_error_v1(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("p.json", WHY_JSON),
    );
}

#[test]
fn a_too_large_plan_still_finds_the_way_to_reach_the_host_before_any_driver_is_asked() {
    // lease-api.md, `create` (pin P32): so that a plan_too_large answer still means the install is
    // sound, `create` fails as it does for any plan when agentspace's ProxyCommand program is not
    // beside the program and AGENTSPACE_PROXY_COMMAND is not set.
    let (mut lab, h) = Lab::with_fake();
    let ran = create_plan(&lab, &plan_with_agents(40), &[]);
    ran.expect_error(
        "internal",
        None,
        Some("agentspace lease create failed: The lease's way to reach its host cannot be found: agentspace's ProxyCommand program is not installed beside this program. No lease was recorded."),
    );
    assert!(h.calls().is_empty());
    assert!(lab.records().is_empty());

    // An override names the way instead, and the check has nothing to find.
    lab.set_var("AGENTSPACE_PROXY_COMMAND", "forwarder --flag");
    let doc = create_plan(&lab, &plan_with_agents(40), &[]).lease_doc();
    assert_eq!(reason_codes(&doc), ["plan_too_large"]);
    assert!(h.calls().is_empty());

    // The program beside the CLI is the way, too.
    let (lab, h) = installed_lab();
    let doc = create_plan(&lab, &plan_with_agents(40), &[]).lease_doc();
    assert_eq!(reason_codes(&doc), ["plan_too_large"]);
    assert!(h.calls().is_empty());
}

#[cfg(windows)]
#[test]
fn a_too_large_plan_makes_the_proxy_commands_check_before_any_driver_is_asked() {
    use std::os::windows::ffi::OsStringExt;
    let (mut lab, h) = installed_lab();
    lab.set_var(
        "AGENTSPACE_PROXY_COMMAND",
        OsString::from_wide(&[0x61, 0xD800, 0x62]),
    );
    let ran = create_plan(&lab, &plan_with_agents(40), &[]);
    ran.expect_error(
        "internal",
        None,
        Some("agentspace lease create failed: AGENTSPACE_PROXY_COMMAND is not valid text. Nothing was recorded."),
    );
    assert!(h.calls().is_empty());
    assert!(lab.records().is_empty());
}

#[test]
fn a_too_large_plan_with_the_built_program_needs_no_ca_and_starts_no_run() {
    // The built program's own driver, in a throwaway home with no CA and no hosts file: asked to
    // place the lease at any size, it would fail for want of a CA or start a run. Neither happens.
    let t = Throwaway::new();
    t.write_in_cwd(
        "big.json",
        &bytes_of(&named(
            "big",
            vec![turn("T", vec![wide("S", 20), wide("S2", 13)])],
        )),
    );
    let run = t.run(&[
        "lease",
        "create",
        "--project",
        "swimcoach",
        "--plan",
        "big.json",
    ]);
    assert_eq!(
        run.status,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.stderr.is_empty());
    let doc: Value = serde_json::from_slice(&run.stdout).expect("One JSON document");
    assert!(schema_problems("lease.output.schema.json", &doc).is_empty());
    assert_eq!(doc["state"], "unsatisfiable");
    assert_eq!(doc["size"], "xl");
    assert_eq!(reason_codes(&doc), ["plan_too_large"]);
    assert_eq!(
        doc["sized_for"],
        json!({"plan": "big", "turn": "T", "agents": 34})
    );
    let mut made: Vec<String> = std::fs::read_dir(t.home.join(".agentspace"))
        .expect("The state folder is made")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    made.sort();
    assert_eq!(
        made,
        ["leases.json", "leases.lock"],
        "The record and its lock are all that is made: no host, no CA, no lease folder"
    );
    assert!(!t.exists(".ssh"), "No alias is made");
}

// ---- A plan that is refused -------------------------------------------------------------------------------------

/// One file's bytes, and the reason it is refused with.
struct Refusal {
    what: &'static str,
    bytes: Vec<u8>,
    why: String,
}

fn refusal(what: &'static str, plan: &Value, why: &str) -> Refusal {
    Refusal {
        what,
        bytes: bytes_of(plan),
        why: why.to_string(),
    }
}

fn raw(what: &'static str, text: &str, why: &str) -> Refusal {
    Refusal {
        what,
        bytes: text.as_bytes().to_vec(),
        why: why.to_string(),
    }
}

/// Runs each file as `p.json` in one lab, and fails once with every case that is not refused with
/// exactly its message. Nothing is recorded and no driver is asked, by any of them.
#[track_caller]
fn check_refusals(cases: Vec<Refusal>) {
    let (lab, h) = installed_lab();
    let mut problems = Vec::new();
    for c in &cases {
        write_file(&lab, "p.json", &c.bytes);
        let ran = create_file(&lab, "p.json", &[]);
        let want = msg_plan("p.json", &c.why);
        for p in error_problems(&ran, "invalid_value", Some("plan"), &want) {
            problems.push(format!("{}: {p}", c.what));
        }
    }
    if !lab.records().is_empty() {
        problems.push("A refused plan recorded a lease".to_string());
    }
    if !h.calls().is_empty() {
        problems.push("A refused plan reached the driver".to_string());
    }
    assert_no_problems(problems);
}

#[test]
fn a_file_that_cannot_be_read_is_refused_and_so_is_one_that_is_not_a_regular_file() {
    let (lab, h) = installed_lab();
    std::fs::create_dir(lab.cwd.join("folder")).unwrap();
    let mut problems = Vec::new();
    // No file, a folder, a device, a name that is `-` (a file called `-`, never standard input), a
    // name that is only whitespace, a name with a leading space (never trimmed), and the `=` form
    // with a value that is `-h`.
    for (what, arg) in [
        ("no such file", "no-such.json"),
        ("a folder", "folder"),
        ("a device", "NUL"),
        ("a file called -", "-"),
        ("whitespace only", "   "),
        ("a leading space, never trimmed", " no-such.json"),
        ("a path through the parent folder", "..\\no-such.json"),
    ] {
        let ran = create_file(&lab, arg, &[]);
        for p in error_problems(
            &ran,
            "invalid_value",
            Some("plan"),
            &msg_plan(arg, WHY_READ),
        ) {
            problems.push(format!("{what}: {p}"));
        }
    }
    let ran = lab.run(&["lease", "create", "--project", "swim", "--plan=-h"]);
    for p in error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("-h", WHY_READ),
    ) {
        problems.push(format!("--plan=-h: {p}"));
    }
    assert!(lab.records().is_empty());
    assert!(h.calls().is_empty());
    assert_no_problems(problems);
}

#[test]
fn a_file_called_dash_is_read_as_a_file_and_standard_input_is_not() {
    let (lab, h) = installed_lab();
    write_file(&lab, "-", &bytes_of(&plan_with_agents(5)));
    let doc = create_file(&lab, "-", &[]).lease_doc();
    assert_eq!(doc["sized_for"]["agents"], 5);
    assert_eq!(place_calls(&h).len(), 1);
}

#[test]
fn the_argument_is_a_path_as_given_relative_to_the_working_folder_or_absolute() {
    let (lab, _h) = installed_lab();
    std::fs::create_dir(lab.cwd.join("sub")).unwrap();
    write_file(&lab, "sub/plans.json", &bytes_of(&plan_with_agents(5)));
    let doc = create_file(&lab, "sub/plans.json", &[]).lease_doc();
    assert_eq!(doc["sized_for"]["agents"], 5);
    let absolute = lab.cwd.join("sub").join("plans.json");
    let line = [
        OsString::from("lease"),
        OsString::from("create"),
        OsString::from("--project"),
        OsString::from("swim"),
        OsString::from("--plan"),
        absolute.into_os_string(),
    ];
    let doc = lab.run_os(&line).lease_doc();
    assert_eq!(doc["sized_for"]["agents"], 5);
    // The flag's name is read in any case, and both forms of a value give the same.
    for line in [
        vec![
            "lease",
            "create",
            "--project",
            "swim",
            "--PLAN",
            "sub/plans.json",
        ],
        vec![
            "lease",
            "create",
            "--project",
            "swim",
            "--Plan=sub/plans.json",
        ],
    ] {
        let doc = lab.run(&line).lease_doc();
        assert_eq!(doc["sized_for"]["agents"], 5, "{line:?}");
    }
}

#[test]
fn an_argument_that_is_not_valid_unicode_or_holds_u_feff_is_refused_with_no_file_opened() {
    // lease-api.md, Plan: refused with invalid_value for `plan`, "the file could not be read", and
    // the message writes each unpaired surrogate as \u and four lowercase hex digits.
    let (lab, h) = installed_lab();
    // A file the argument would name if it were read as lossy text, so a build that converts it is
    // seen to open a file.
    write_file(&lab, "p\u{fffd}.json", &bytes_of(&base()));
    let bad = not_unicode("p", ".json");
    let line: Vec<OsString> = ["lease", "create", "--project", "swim", "--plan"]
        .iter()
        .map(OsString::from)
        .chain([bad])
        .collect();
    let ran = lab.run_os(&line);
    assert_no_problems(error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("p\\ud800.json", WHY_READ),
    ));
    // U+FEFF in the argument, though a file of that name exists.
    write_file(&lab, "p\u{feff}.json", &bytes_of(&base()));
    let ran = create_file(&lab, "p\u{feff}.json", &[]);
    assert_no_problems(error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("p\u{feff}.json", WHY_READ),
    ));
    // A control character in the argument is written as \u and four lowercase hex digits.
    let ran = create_file(&lab, "a\nb.json", &[]);
    assert_no_problems(error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("a\\u000ab.json", WHY_READ),
    ));
    assert!(lab.records().is_empty());
    assert!(h.calls().is_empty());
}

#[cfg(windows)]
#[test]
fn a_file_another_program_holds_without_sharing_is_one_that_could_not_be_read() {
    use std::os::windows::fs::OpenOptionsExt;
    let (lab, h) = installed_lab();
    write_file(&lab, "p.json", &bytes_of(&base()));
    let _held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(lab.cwd.join("p.json"))
        .expect("The test can hold the file");
    let ran = create_file(&lab, "p.json", &[]);
    assert_no_problems(error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("p.json", WHY_READ),
    ));
    assert!(lab.records().is_empty());
    assert!(h.calls().is_empty());
}

#[test]
fn the_files_size_is_checked_first_and_262144_bytes_with_a_byte_order_mark_counted_is_the_most() {
    let plan = plan_with_agents(5);
    // 262144 bytes exactly is accepted, with or without a byte order mark counted in them.
    for bom in [false, true] {
        let (lab, _h) = installed_lab();
        write_file(&lab, "p.json", &padded(&plan, MAX_FILE, bom));
        let doc = create_file(&lab, "p.json", &[]).lease_doc();
        assert_eq!(doc["sized_for"]["agents"], 5, "bom {bom}");
    }
    // One byte more is refused as larger, whatever it holds: a plan, and what is no JSON at all.
    let junk = vec![b'x'; MAX_FILE + 1];
    let mut over_bom = padded(&plan, MAX_FILE, true);
    over_bom.push(b' ');
    check_refusals(vec![
        Refusal {
            what: "a valid plan padded to one byte over",
            bytes: padded(&plan, MAX_FILE + 1, false),
            why: WHY_LARGER.to_string(),
        },
        Refusal {
            what: "a byte order mark that is the byte over",
            bytes: over_bom,
            why: WHY_LARGER.to_string(),
        },
        Refusal {
            what: "junk, which fails every later check as well",
            bytes: junk,
            why: WHY_LARGER.to_string(),
        },
    ]);
}

#[test]
fn a_file_that_is_not_utf8_is_refused_before_it_is_read_as_json() {
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in serde_json::to_string(&base()).unwrap().encode_utf16() {
        utf16.extend(unit.to_le_bytes());
    }
    let mut no_bom = Vec::new();
    for unit in serde_json::to_string(&base()).unwrap().encode_utf16() {
        no_bom.extend(unit.to_le_bytes());
    }
    let mut latin = br#"{"name":"caf"#.to_vec();
    latin.push(0xE9);
    latin.extend(br#"","turns":[]}"#);
    check_refusals(vec![
        Refusal {
            what: "UTF-16 with its byte order mark",
            bytes: utf16,
            why: WHY_UTF8.to_string(),
        },
        Refusal {
            what: "UTF-16 without one: its bytes are valid UTF-8 and no JSON",
            bytes: no_bom,
            why: WHY_JSON.to_string(),
        },
        Refusal {
            what: "a Latin-1 byte inside a string",
            bytes: latin,
            why: WHY_UTF8.to_string(),
        },
        Refusal {
            what: "bad UTF-8 and bad JSON: the UTF-8 check comes first",
            bytes: b"{\xff".to_vec(),
            why: WHY_UTF8.to_string(),
        },
    ]);
}

#[test]
fn a_file_that_is_not_one_json_document_is_refused() {
    let valid = serde_json::to_string(&base()).unwrap();
    let with_extra_number = valid.replacen("{\"name\"", "{\"x\":1e400,\"name\"", 1);
    check_refusals(vec![
        raw("an empty file", "", WHY_JSON),
        raw("only whitespace", " \r\n\t ", WHY_JSON),
        raw("a byte order mark and nothing", "\u{feff}", WHY_JSON),
        raw("text after the document", &format!("{valid} x"), WHY_JSON),
        raw("two documents", &format!("{valid}{valid}"), WHY_JSON),
        raw("a comma after the document", &format!("{valid},"), WHY_JSON),
        raw("not JSON at all", "turns: []", WHY_JSON),
        raw("a truncated document", &valid[..valid.len() - 3], WHY_JSON),
        raw("a trailing comma", "{\"turns\":[],}", WHY_JSON),
        raw("single quotes", "{'turns':[]}", WHY_JSON),
        raw(
            "a second byte order mark after the first, outside a string",
            &format!("\u{feff}\u{feff}{valid}"),
            WHY_JSON,
        ),
        raw(
            "a byte order mark between members",
            &valid.replacen(",\"turns\"", ",\u{feff}\"turns\"", 1),
            WHY_JSON,
        ),
        raw(
            "an unpaired high surrogate in a string",
            "{\"name\":\"\\ud800\",\"turns\":[]}",
            WHY_JSON,
        ),
        raw(
            "an unpaired low surrogate in a string",
            "{\"name\":\"\\udc00\",\"turns\":[]}",
            WHY_JSON,
        ),
        raw(
            "a number the parser cannot hold, 1e400",
            &with_extra_number,
            WHY_JSON,
        ),
        raw("a bare word", "nope", WHY_JSON),
    ]);
}

#[test]
fn a_leading_utf8_byte_order_mark_is_accepted_and_ignored() {
    let (lab, _h) = installed_lab();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(bytes_of(&named("bom", vec![turn("T", vec![wide("S", 2)])])));
    write_file(&lab, "p.json", &bytes);
    let doc = create_file(&lab, "p.json", &[]).lease_doc();
    assert_eq!(
        doc["sized_for"],
        json!({"plan": "bom", "turn": "T", "agents": 3})
    );
    // The stored plan holds no byte order mark, and a reader MUST NOT depend on its bytes.
    assert_eq!(
        lab.records()[0]["plan"],
        named("bom", vec![turn("T", vec![wide("S", 2)])])
    );
}

#[test]
fn a_member_named_twice_is_refused_by_its_name_once_its_escapes_are_read() {
    let sub_ok = r#"{"name":"a","model":"M","effort":"e"}"#;
    let dup_turns = format!(
        r#"{{"turns":[{{"name":"T","stories":[{{"name":"S","subagents":[{sub_ok}]}}]}}],"turns":[]}}"#
    );
    check_refusals(vec![
        raw(
            "turns twice",
            &dup_turns,
            "an object has two members named 'turns'",
        ),
        raw(
            "a member twice, one of them written with an escape",
            r#"{"na\u006de":"x","name":"y","turns":[]}"#,
            "an object has two members named 'name'",
        ),
        raw(
            "a duplicate that the schema would also refuse: the members come first",
            r#"{"turns":[],"turns":[],"zzz":1}"#,
            "an object has two members named 'turns'",
        ),
        raw(
            "a duplicate deep in the document",
            r#"{"turns":[{"name":"T","name":"U","stories":[]}]}"#,
            "an object has two members named 'name'",
        ),
        raw(
            "the first member of a document order that repeats an earlier one",
            r#"{"b":1,"a":1,"b":2,"a":2,"turns":[]}"#,
            "an object has two members named 'b'",
        ),
        raw(
            "a name that holds a control character is written as \\u and four hex digits",
            "{\"a\\u000ab\":1,\"a\\u000ab\":2,\"turns\":[]}",
            "an object has two members named 'a\\u000ab'",
        ),
        raw(
            "a duplicate and a document that is not JSON after it: the JSON check comes first",
            r#"{"turns":[],"turns":[]} junk"#,
            WHY_JSON,
        ),
    ]);
}

#[test]
fn a_document_that_is_not_of_the_plans_shape_is_refused_by_the_schema() {
    let n65 = "x".repeat(65);
    let e33 = "e".repeat(33);
    let many_turns: Vec<Value> = (0..33)
        .map(|i| turn(&format!("T{i}"), vec![story("S", vec![sub("a")])]))
        .collect();
    let many_stories: Vec<Value> = (0..33)
        .map(|i| story(&format!("S{i}"), vec![sub("a")]))
        .collect();
    let many_subs: Vec<Value> = (0..33).map(|i| sub(&format!("a{i}"))).collect();
    let names32: Vec<String> = (0..32).map(|i| format!("n{i}")).collect();
    let cases: Vec<(&'static str, Value)> = vec![
        ("the document is an array", json!([])),
        ("the document is null", json!(null)),
        ("the document is a string", json!("plan")),
        ("the document is a number", json!(1)),
        ("an empty object: no turns", json!({})),
        ("no turns, a name only", json!({"name": "v"})),
        ("turns is null", with("/turns", json!(null))),
        ("turns is an object", with("/turns", json!({}))),
        ("an empty list of turns", with("/turns", json!([]))),
        ("33 turns", plan_of(many_turns)),
        ("an unknown member of the plan", with("/extra", json!(1))),
        ("a member's name in another case", with("/Turns", json!([]))),
        (
            "a turn that is not an object",
            with("/turns/0", json!("T1")),
        ),
        ("a turn with no name", without("/turns/0/name")),
        (
            "a turn with a null name",
            with("/turns/0/name", json!(null)),
        ),
        ("a turn with no stories", without("/turns/0/stories")),
        (
            "a turn with null stories",
            with("/turns/0/stories", json!(null)),
        ),
        (
            "an empty list of stories",
            with("/turns/0/stories", json!([])),
        ),
        ("33 stories", with("/turns/0/stories", json!(many_stories))),
        (
            "an unknown member of a turn",
            with("/turns/0/extra", json!(1)),
        ),
        ("a story with no name", without("/turns/0/stories/0/name")),
        (
            "a story with no subagents",
            without("/turns/0/stories/0/subagents"),
        ),
        (
            "an empty list of subagents",
            with("/turns/0/stories/0/subagents", json!([])),
        ),
        (
            "33 subagents",
            with("/turns/0/stories/0/subagents", json!(many_subs)),
        ),
        (
            "an unknown member of a story",
            with("/turns/0/stories/0/extra", json!(1)),
        ),
        (
            "a story's peak stated: a plan has no member for it",
            with("/turns/0/stories/0/peak", json!(3)),
        ),
        (
            "a subagent with no name",
            without("/turns/0/stories/0/subagents/0/name"),
        ),
        (
            "a subagent with no model",
            without("/turns/0/stories/0/subagents/0/model"),
        ),
        (
            "a subagent with no effort",
            without("/turns/0/stories/0/subagents/0/effort"),
        ),
        (
            "a null model",
            with("/turns/0/stories/0/subagents/0/model", json!(null)),
        ),
        (
            "a null effort",
            with("/turns/0/stories/0/subagents/0/effort", json!(null)),
        ),
        (
            "a null subagent name",
            with("/turns/0/stories/0/subagents/0/name", json!(null)),
        ),
        (
            "an unknown member of a subagent",
            with("/turns/0/stories/0/subagents/0/tools", json!([])),
        ),
        ("a plan name that is a number", with("/name", json!(7))),
        ("an empty plan name", with("/name", json!(""))),
        (
            "a plan name with a space at its start",
            with("/name", json!(" v")),
        ),
        (
            "a plan name with a space at its end",
            with("/name", json!("v ")),
        ),
        ("a plan name of 65 characters", with("/name", json!(n65))),
        (
            "a plan name that is not ASCII",
            with("/name", json!("v\u{e9}")),
        ),
        ("a plan name with a tab", with("/name", json!("a\tb"))),
        (
            "a plan name with a line feed at its end",
            with("/name", json!("v\n")),
        ),
        (
            "a plan name with a control character",
            with("/name", json!("a\u{1}b")),
        ),
        (
            "a plan name with a byte order mark inside",
            with("/name", json!("a\u{feff}b")),
        ),
        (
            "a plan name with an emoji",
            with("/name", json!("v\u{1F600}")),
        ),
        (
            "a turn name that is empty",
            with("/turns/0/name", json!("")),
        ),
        (
            "a turn name of 65 characters",
            with("/turns/0/name", json!(n65)),
        ),
        (
            "a turn name with a space at its start",
            with("/turns/0/name", json!(" T1")),
        ),
        (
            "a story name that is empty",
            with("/turns/0/stories/0/name", json!("")),
        ),
        (
            "a story name that is a number",
            with("/turns/0/stories/0/name", json!(1)),
        ),
        (
            "a subagent name that is empty",
            with("/turns/0/stories/0/subagents/0/name", json!("")),
        ),
        (
            "a subagent name with a space at its end",
            with("/turns/0/stories/0/subagents/0/name", json!("a ")),
        ),
        (
            "a model that is empty",
            with("/turns/0/stories/0/subagents/0/model", json!("")),
        ),
        (
            "a model with a space at its start",
            with("/turns/0/stories/0/subagents/0/model", json!(" Opus")),
        ),
        (
            "a model of 65 characters",
            with("/turns/0/stories/0/subagents/0/model", json!(n65)),
        ),
        (
            "a model that is not ASCII",
            with("/turns/0/stories/0/subagents/0/model", json!("Opu\u{e9}")),
        ),
        (
            "a model that is a list",
            with("/turns/0/stories/0/subagents/0/model", json!(["Opus"])),
        ),
        (
            "an effort that is empty",
            with("/turns/0/stories/0/subagents/0/effort", json!("")),
        ),
        (
            "an effort that starts with a dash",
            with("/turns/0/stories/0/subagents/0/effort", json!("-high")),
        ),
        (
            "an effort that starts with a dot",
            with("/turns/0/stories/0/subagents/0/effort", json!(".x")),
        ),
        (
            "an effort with a space",
            with("/turns/0/stories/0/subagents/0/effort", json!("very high")),
        ),
        (
            "an effort of 33 characters",
            with("/turns/0/stories/0/subagents/0/effort", json!(e33)),
        ),
        (
            "an effort that is not ASCII",
            with("/turns/0/stories/0/subagents/0/effort", json!("h\u{ef}gh")),
        ),
        (
            "an effort that is a number",
            with("/turns/0/stories/0/subagents/0/effort", json!(5)),
        ),
        (
            "an after that is a string",
            with("/turns/0/stories/0/subagents/0/after", json!("a")),
        ),
        (
            "an after of 32 names",
            with("/turns/0/stories/0/subagents/0/after", json!(names32)),
        ),
        (
            "an after that names one twice",
            with("/turns/0/stories/0/subagents/0/after", json!(["a", "a"])),
        ),
        (
            "an after of a number",
            with("/turns/0/stories/0/subagents/0/after", json!([1])),
        ),
        (
            "an after of an empty name",
            with("/turns/0/stories/0/subagents/0/after", json!([""])),
        ),
        (
            "an after of a name with a space at its end",
            with("/turns/0/stories/0/subagents/0/after", json!(["a "])),
        ),
        (
            "an after that is an object",
            with("/turns/0/stories/0/subagents/0/after", json!({})),
        ),
    ];
    check_refusals(
        cases
            .into_iter()
            .map(|(what, plan)| refusal(what, &plan, WHY_SCHEMA))
            .collect(),
    );
}

#[test]
fn a_document_nested_far_past_any_plan_is_refused_and_never_crashes_the_program() {
    // lease-api.md, Plan: a parser MAY limit the nesting, MUST allow at least 64 levels, and a
    // plan's deepest value is at level 9. The program is the built binary, so a build that runs out
    // of stack ends this one test and not the suite.
    let t = Throwaway::new();
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    t.write_in_cwd("deep.json", deep.as_bytes());
    let run = t.run(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--plan",
        "deep.json",
    ]);
    let problems = failure_problems(&run, "invalid_value", Some("plan"), None);
    assert_no_problems(problems);
    let doc: Value = serde_json::from_slice(&run.stderr).unwrap();
    let message = doc["message"].as_str().unwrap();
    assert!(
        message == msg_plan("deep.json", WHY_JSON) || message == msg_plan("deep.json", WHY_SCHEMA),
        "A document nested 100000 levels is not valid JSON for the parser's limit, or a plan the schema refuses: {message}"
    );
    // 64 levels are allowed by the parser, the document counting as level 1: a document whose extra
    // member holds 63 arrays inside one another is a JSON document 64 levels deep, and it is refused
    // by the schema, for a member that is not in the shape, and not as JSON.
    let nested = format!(
        "{{\"turns\":[],\"x\":{}{}}}",
        "[".repeat(63),
        "]".repeat(63)
    );
    t.write_in_cwd("nest.json", nested.as_bytes());
    let run = t.run(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--plan",
        "nest.json",
    ]);
    let message = failure_message(&run);
    assert_eq!(message, msg_plan("nest.json", WHY_SCHEMA));
}

fn failure_message(run: &Run) -> String {
    let doc: Value = serde_json::from_slice(&run.stderr).expect("An error document");
    doc["message"].as_str().expect("A message").to_string()
}

#[test]
fn the_names_and_what_they_repeat_are_refused_in_the_order_of_the_checks() {
    let mk =
        |names: &[&str]| -> Vec<Value> { names.iter().map(|n| story(n, vec![sub("a")])).collect() };
    let t_dup = |name: &str| turn(name, vec![story("S", vec![sub("a")])]);
    check_refusals(vec![
        refusal(
            "two turns of one name",
            &plan_of(vec![t_dup("T1"), t_dup("T1")]),
            "turn 'T1' is named twice",
        ),
        refusal(
            "the first turn name that repeats an earlier one, in document order",
            &plan_of(vec![t_dup("A"), t_dup("B"), t_dup("B"), t_dup("A")]),
            "turn 'B' is named twice",
        ),
        refusal(
            "a turn name that holds a quote, quoted raw",
            &plan_of(vec![t_dup("it's"), t_dup("it's")]),
            "turn 'it's' is named twice",
        ),
        refusal(
            "two stories of one turn of one name",
            &plan_of(vec![turn("T1", mk(&["S", "S"]))]),
            "story 'S' is named twice in turn 'T1'",
        ),
        refusal(
            "the first repeated story name, in document order",
            &plan_of(vec![turn("T1", mk(&["A", "B", "B", "A"]))]),
            "story 'B' is named twice in turn 'T1'",
        ),
        refusal(
            "a repeated story name in a later turn names that turn",
            &plan_of(vec![
                turn("T1", mk(&["A", "B"])),
                turn("Turn Two", mk(&["C", "C"])),
            ]),
            "story 'C' is named twice in turn 'Turn Two'",
        ),
        refusal(
            "two subagents of one story of one name",
            &one_story(vec![sub("a"), sub("a")]),
            "subagent 'a' is named twice in story 'S1'",
        ),
        refusal(
            "the first repeated subagent name, in document order",
            &one_story(vec![sub("a"), sub("b"), sub("b"), sub("a")]),
            "subagent 'b' is named twice in story 'S1'",
        ),
        refusal(
            "a repeated subagent name, in the second story",
            &plan_of(vec![turn(
                "T1",
                vec![
                    story("S1", vec![sub("a"), sub("b")]),
                    story("Story 2", vec![sub("x y"), sub("x y")]),
                ],
            )]),
            "subagent 'x y' is named twice in story 'Story 2'",
        ),
        refusal(
            "an after that names no subagent of its story",
            &one_story(vec![sub("a"), sub_after("b", &["zzz"])]),
            "subagent 'b' in story 'S1' starts after 'zzz', which is no subagent of that story",
        ),
        refusal(
            "an after that names a subagent of another story",
            &plan_of(vec![turn(
                "T1",
                vec![
                    story("S1", vec![sub("a")]),
                    story("S2", vec![sub_after("b", &["a"])]),
                ],
            )]),
            "subagent 'b' in story 'S2' starts after 'a', which is no subagent of that story",
        ),
        refusal(
            "an after compared exactly: A is not a",
            &one_story(vec![sub("a"), sub_after("b", &["A"])]),
            "subagent 'b' in story 'S1' starts after 'A', which is no subagent of that story",
        ),
        refusal(
            "the first subagent with such an entry, and its first such entry",
            &one_story(vec![
                sub("a"),
                sub_after("b", &["a", "x1", "x2"]),
                sub_after("c", &["y"]),
            ]),
            "subagent 'b' in story 'S1' starts after 'x1', which is no subagent of that story",
        ),
        refusal(
            "a subagent that starts after itself",
            &one_story(vec![sub_after("a", &["a"])]),
            "the subagents of story 'S1' start after each other in a cycle",
        ),
        refusal(
            "two subagents that start after each other",
            &one_story(vec![sub_after("a", &["b"]), sub_after("b", &["a"])]),
            "the subagents of story 'S1' start after each other in a cycle",
        ),
        refusal(
            "a cycle of three, behind a subagent that is outside it",
            &one_story(vec![
                sub("free"),
                sub_after("a", &["c", "free"]),
                sub_after("b", &["a"]),
                sub_after("c", &["b"]),
            ]),
            "the subagents of story 'S1' start after each other in a cycle",
        ),
        refusal(
            "the first story with a cycle",
            &plan_of(vec![turn(
                "T1",
                vec![
                    story("fine", vec![sub("a"), sub_after("b", &["a"])]),
                    story("first loop", vec![sub_after("a", &["a"])]),
                    story("second loop", vec![sub_after("a", &["a"])]),
                ],
            )]),
            "the subagents of story 'first loop' start after each other in a cycle",
        ),
        refusal(
            "turn names are checked before story names",
            &plan_of(vec![turn("T1", mk(&["S", "S"])), t_dup("T2"), t_dup("T2")]),
            "turn 'T2' is named twice",
        ),
        refusal(
            "story names are checked before subagent names",
            &plan_of(vec![turn(
                "T1",
                vec![
                    story("S1", vec![sub("a"), sub("a")]),
                    story("S2", vec![sub("a")]),
                    story("S2", vec![sub("a")]),
                ],
            )]),
            "story 'S2' is named twice in turn 'T1'",
        ),
        refusal(
            "subagent names are checked before an after that names no subagent",
            &plan_of(vec![turn(
                "T1",
                vec![
                    story("S1", vec![sub("a"), sub_after("b", &["nope"])]),
                    story("S2", vec![sub("a"), sub("a")]),
                ],
            )]),
            "subagent 'a' is named twice in story 'S2'",
        ),
        refusal(
            "an after that names no subagent is checked before a cycle, in any story",
            &plan_of(vec![turn(
                "T1",
                vec![
                    story("S1", vec![sub_after("a", &["a"])]),
                    story("S2", vec![sub_after("b", &["nope"])]),
                ],
            )]),
            "subagent 'b' in story 'S2' starts after 'nope', which is no subagent of that story",
        ),
        refusal(
            "the schema is checked before the names",
            &plan_of(vec![
                t_dup("T1"),
                t_dup("T1"),
                json!({"name": "T3", "stories": []}),
            ]),
            WHY_SCHEMA,
        ),
    ]);
}

#[test]
fn names_that_repeat_in_different_places_or_differ_in_case_are_not_repeats() {
    // lease-api.md, Plan, Names: unique among siblings only, compared exactly.
    let plan = plan_of(vec![
        turn(
            "T1",
            vec![
                story("S", vec![sub("a"), sub("A"), sub("a b"), sub("a  b")]),
                story("s", vec![sub("a")]),
            ],
        ),
        turn(
            "t1",
            vec![story("S", vec![sub("a"), sub_after("b", &["a"])])],
        ),
    ]);
    let (lab, _h) = installed_lab();
    let doc = create_plan(&lab, &plan, &[]).lease_doc();
    assert_eq!(doc["sized_for"], json!({"turn": "T1", "agents": 6}));
}

#[test]
fn every_bound_a_plan_may_reach_is_accepted() {
    let n64 = format!("n{}", "x".repeat(63));
    let e32 = format!("e{}", "x".repeat(31));
    let turns32: Vec<Value> = (0..32)
        .map(|i| turn(&format!("T{i}"), vec![story("S", vec![sub("a")])]))
        .collect();
    let stories32: Vec<Value> = (0..32)
        .map(|i| story(&format!("S{i}"), vec![sub("a")]))
        .collect();
    // The last of 32 subagents starts after the other 31, the most an after holds.
    let names31: Vec<String> = (0..31).map(|i| format!("a{i}")).collect();
    let mut subs32: Vec<Value> = (0..31).map(|i| sub(&format!("a{i}"))).collect();
    subs32.push(json!({"name": "last", "model": "Opus", "effort": "high", "after": names31}));
    let cases: Vec<(&str, Value)> = vec![
        ("32 turns", plan_of(turns32)),
        ("32 stories", plan_of(vec![turn("T1", stories32)])),
        ("32 subagents, one after 31 others", one_story(subs32)),
        ("names of 64 characters", {
            named(
                &n64,
                vec![turn(
                    &n64,
                    vec![story(
                        &n64,
                        vec![json!({"name": n64, "model": n64, "effort": e32, "after": []})],
                    )],
                )],
            )
        }),
        (
            "a model that holds a version and a pin",
            one_story(vec![
                json!({"name": "a", "model": "Opus 5.5 +24h", "effort": "high"}),
                json!({"name": "b", "model": "Sonnet 4.5", "effort": "medium-2.x_y"}),
            ]),
        ),
        (
            "a name that holds every printable punctuation mark",
            named(
                "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
                vec![turn("~ ~", vec![story("}", vec![sub("{")])])],
            ),
        ),
    ];
    let mut problems = Vec::new();
    for (what, plan) in cases {
        let (lab, _h) = installed_lab();
        let ran = create_plan(&lab, &plan, &[]);
        if ran.code != 0 {
            problems.push(format!("{what}: exit {}, {}", ran.code, ran.stderr_text()));
        } else if let Some(p) = schema_problems("lease.output.schema.json", &ran.success()).first()
        {
            problems.push(format!("{what}: {p}"));
        }
    }
    assert_no_problems(problems);
}

// ---- Where the plan is read, and in what order --------------------------------------------------------------

#[test]
fn the_values_are_checked_in_the_order_project_platform_profile_plan_size_ttl() {
    let (lab, h) = installed_lab();
    write_file(&lab, "bad.json", b"{");
    write_file(&lab, "ok.json", &bytes_of(&base()));
    let mut problems = Vec::new();
    let go = |extra: &[&str]| -> Ran {
        let mut line = vec!["lease", "create"];
        line.extend_from_slice(extra);
        lab.run(&line)
    };
    // A bad project comes before a bad plan, a bad platform before it, a bad profile before it.
    let ran = go(&["--project", "-x", "--plan", "bad.json"]);
    problems.extend(error_problems(
        &ran,
        "invalid_value",
        Some("project"),
        &msg_bad_project("-x"),
    ));
    let ran = go(&[
        "--project",
        "x",
        "--platform",
        "linux",
        "--plan",
        "bad.json",
    ]);
    problems.extend(error_problems(
        &ran,
        "invalid_value",
        Some("platform"),
        &msg_bad_platform("linux"),
    ));
    let ran = go(&["--project", "x", "--profile", "a b", "--plan", "bad.json"]);
    problems.extend(error_problems(
        &ran,
        "invalid_value",
        Some("profile"),
        &msg_bad_profile("a b"),
    ));
    // A bad plan comes before a TTL that is bad or out of range.
    let ran = go(&["--project", "x", "--plan", "bad.json", "--ttl", "8"]);
    problems.extend(error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("bad.json", WHY_JSON),
    ));
    let ran = go(&["--project", "x", "--ttl", "48h", "--plan", "bad.json"]);
    problems.extend(error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("bad.json", WHY_JSON),
    ));
    // A good plan, and then the TTL's own errors.
    let ran = go(&["--project", "x", "--plan", "ok.json", "--ttl", "8"]);
    problems.extend(error_problems(
        &ran,
        "invalid_value",
        Some("ttl"),
        &msg_ttl_no_unit("8"),
    ));
    let ran = go(&["--project", "x", "--plan", "ok.json", "--ttl", "48h"]);
    problems.extend(error_problems(
        &ran,
        "out_of_range",
        Some("ttl"),
        &msg_ttl_range("48h"),
    ));
    assert!(lab.records().is_empty());
    assert!(h.calls().is_empty());
    assert_no_problems(problems);
}

#[test]
fn a_refused_plan_is_refused_before_the_driver_or_the_records_are_looked_at() {
    // lease-api.md, Exit codes and errors: invalid_value before any internal error.
    let lab = Lab::new();
    std::fs::write(&lab.state, b"not a folder").unwrap();
    write_file(&lab, "p.json", b"nope");
    let ran = lab.run(&["lease", "create", "--project", "swim", "--plan", "p.json"]);
    expect_error_v1(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("p.json", WHY_JSON),
    );
}

#[cfg(windows)]
#[test]
fn a_plan_is_read_and_refused_without_waiting_for_the_lease_records() {
    // lease-api.md, Concurrency and atomicity: `create` reads and parses the plan before it starts
    // to wait, so a plan that is not well formed, or is large, is refused with invalid_value
    // without waiting for another command. Waiting would take 30 seconds and end as `internal`.
    let (lab, h) = installed_lab();
    let hold = lab::LockHold::take(&lab);
    write_file(&lab, "bad.json", b"{");
    write_file(&lab, "big.json", &padded(&base(), MAX_FILE + 1, false));
    let started = Instant::now();
    let mut problems = Vec::new();
    let ran = create_file(&lab, "bad.json", &[]);
    problems.extend(error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("bad.json", WHY_JSON),
    ));
    let ran = create_file(&lab, "big.json", &[]);
    problems.extend(error_problems(
        &ran,
        "invalid_value",
        Some("plan"),
        &msg_plan("big.json", WHY_LARGER),
    ));
    if started.elapsed() > Duration::from_secs(10) {
        problems.push(format!("The refusals took {:?}", started.elapsed()));
    }
    drop(hold);
    assert!(h.calls().is_empty());
    assert_no_problems(problems);
}

// ---- Usage: --plan and --size --------------------------------------------------------------------------------------

#[test]
fn plan_with_size_is_a_usage_error_with_field_plan_where_the_later_flag_stands() {
    let plan_size = |line: &[&str]| case(line, Some("plan"), MSG_PLAN_AND_SIZE);
    check_usage_cases(vec![
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--plan",
            "p.json",
            "--size",
            "m",
        ]),
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--size",
            "m",
            "--plan",
            "p.json",
        ]),
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--plan=p.json",
            "--size=m",
        ]),
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--SIZE",
            "m",
            "--Plan",
            "p.json",
        ]),
        plan_size(&[
            "lease",
            "create",
            "--plan",
            "p.json",
            "--size",
            "l",
            "--project",
            "x",
        ]),
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--plan",
            "p.json",
            "--size",
            "xxl",
        ]),
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--plan",
            "p.json",
            "--size",
            " ",
        ]),
        // The usage errors come before the values: a plan that could not be read, a project that is
        // not one, and a project that is missing.
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--plan",
            "no-such.json",
            "--size",
            "m",
        ]),
        plan_size(&[
            "lease",
            "create",
            "--project",
            "-x",
            "--plan",
            "p.json",
            "--size",
            "m",
        ]),
        plan_size(&["lease", "create", "--plan", "p.json", "--size", "m"]),
        // It is reported where the later flag stands: an unknown option after it does not come first.
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--plan",
            "p.json",
            "--size",
            "m",
            "--bogus",
        ]),
        // And a flag given twice before it does.
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "a.json",
                "--plan",
                "b.json",
                "--size",
                "m",
            ],
            Some("plan"),
            &msg_twice("plan"),
        ),
        plan_size(&[
            "lease",
            "create",
            "--project",
            "x",
            "--plan",
            "a.json",
            "--size",
            "m",
            "--plan",
            "b.json",
        ]),
    ]);
}

#[test]
fn a_later_flag_with_no_value_or_an_empty_one_is_reported_as_that_and_not_as_plan_with_size() {
    check_usage_cases(vec![
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "p.json",
                "--size",
            ],
            Some("size"),
            &msg_needs_value("size", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "p.json",
                "--size",
                "--ttl",
                "8h",
            ],
            Some("size"),
            &msg_needs_value("size", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "p.json",
                "--size",
                "",
            ],
            Some("size"),
            &msg_empty_value("size", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "p.json",
                "--size=",
            ],
            Some("size"),
            &msg_empty_value("size", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--size", "m", "--plan"],
            Some("plan"),
            &msg_needs_value("plan", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--size",
                "m",
                "--plan",
                "--ttl",
                "8h",
            ],
            Some("plan"),
            &msg_needs_value("plan", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--size",
                "m",
                "--plan",
                "",
            ],
            Some("plan"),
            &msg_empty_value("plan", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--size",
                "m",
                "--plan=",
            ],
            Some("plan"),
            &msg_empty_value("plan", "create"),
        ),
    ]);
}

#[test]
fn the_other_usage_errors_of_a_plan_line_are_the_ones_of_any_line() {
    check_usage_cases(vec![
        case(
            &["lease", "create", "--project", "x", "--plan"],
            Some("plan"),
            &msg_needs_value("plan", "create"),
        ),
        case(
            &["lease", "create", "--project", "x", "--plan", ""],
            Some("plan"),
            &msg_empty_value("plan", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "a.json",
                "--plan=b.json",
            ],
            Some("plan"),
            &msg_twice("plan"),
        ),
        case(
            &["lease", "create", "--plan", "p.json"],
            Some("project"),
            MSG_NO_PROJECT,
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "p.json",
                "extra",
            ],
            None,
            &msg_unexpected("extra", "create"),
        ),
        case(
            &[
                "lease", "create", "--bogus", "--plan", "p.json", "--size", "m",
            ],
            None,
            &msg_unknown_option_sub("--bogus", "create"),
        ),
        case(
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "p.json",
                "--bogus",
                "--size",
                "m",
            ],
            None,
            &msg_unknown_option_sub("--bogus", "create"),
        ),
        // `--plan` is a flag of create alone.
        case(
            &["lease", "get", "--plan", "p.json"],
            None,
            &msg_unknown_option_sub("--plan", "get"),
        ),
        case(
            &["lease", "list", "--plan", "p.json"],
            None,
            &msg_unknown_option_sub("--plan", "list"),
        ),
    ]);
}

// ---- Contract v1 keeps v0 for a command line with no plan --------------------------------------------------

/// The problems a lease or list document has against v0's schema and v1's, for a command line with
/// no `--plan`: it MUST validate against both (lease-api.md, Schemas and Versioning).
fn both_versions(schema: &str, doc: &Value) -> Vec<String> {
    let mut p: Vec<String> = schema_problems(schema, doc)
        .into_iter()
        .map(|e| format!("v1: {e}"))
        .collect();
    p.extend(
        schema_problems(&format!("v0/{schema}"), doc)
            .into_iter()
            .map(|e| format!("v0: {e}")),
    );
    p
}

/// A run's stdout, as `Ran::success` reads it, checked against both versions' schema `schema`.
#[track_caller]
fn plain_doc(ran: &Ran, schema: &str) -> Value {
    let doc = ran.success();
    let p = both_versions(schema, &doc);
    assert!(
        p.is_empty(),
        "A plan-less document breaks a schema: {p:?}\n{doc}"
    );
    doc
}

#[test]
fn a_command_line_with_no_plan_prints_documents_that_validate_against_v0_and_v1() {
    let (mut lab, h) = installed_lab();
    let created = plain_doc(
        &lab.run(&["lease", "create", "--project", "swim"]),
        "lease.output.schema.json",
    );
    h.script_place(Ok(Placed::Pending));
    let pending = plain_doc(
        &lab.run(&["lease", "create", "--project", "swim", "--size", "l"]),
        "lease.output.schema.json",
    );
    h.script_place(Ok(Placed::Unsatisfiable {
        reasons: vec![
            reason(
                "unsupported_platform",
                "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only.",
            ),
            reason(
                "unsupported_size",
                "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl.",
            ),
        ],
    }));
    let unsat = plain_doc(
        &lab.run(&[
            "lease",
            "create",
            "--project",
            "swim",
            "--platform",
            "windows/amd64",
            "--size",
            "xxl",
        ]),
        "lease.output.schema.json",
    );
    let short = plain_doc(
        &lab.run(&["lease", "create", "--project", "swim", "--ttl", "1m"]),
        "lease.output.schema.json",
    );
    let id = |d: &Value| d["id"].as_str().unwrap().to_string();
    let mut docs = vec![
        created.clone(),
        pending.clone(),
        unsat.clone(),
        short.clone(),
    ];
    for d in [&created, &pending, &unsat] {
        docs.push(plain_doc(
            &lab.run(&["lease", "get", &id(d)]),
            "lease.output.schema.json",
        ));
    }
    docs.push(plain_doc(
        &lab.run(&["lease", "renew", &id(&created), "--ttl", "1h"]),
        "lease.output.schema.json",
    ));
    let list = plain_doc(
        &lab.run(&["lease", "list"]),
        "lease-list.output.schema.json",
    );
    plain_doc(
        &lab.run(&["lease", "list", "--state", "unsatisfiable"]),
        "lease-list.output.schema.json",
    );
    plain_doc(
        &lab.run(&["lease", "list", "--project", "other"]),
        "lease-list.output.schema.json",
    );
    docs.push(plain_doc(
        &lab.run(&["lease", "release", &id(&pending)]),
        "lease.output.schema.json",
    ));
    docs.push(plain_doc(
        &lab.run(&["lease", "release", &id(&unsat)]),
        "lease.output.schema.json",
    ));
    docs.push(plain_doc(
        &lab.run(&["lease", "release", &id(&created)]),
        "lease.output.schema.json",
    ));
    lab.set_now(NOW + 3600);
    docs.push(plain_doc(
        &lab.run(&["lease", "get", &id(&short)]),
        "lease.output.schema.json",
    ));
    for d in docs.iter().chain(list["leases"].as_array().unwrap()) {
        assert!(
            d.get("sized_for").is_none(),
            "No sized_for without a plan: {d}"
        );
        assert!(
            !reason_codes(d).contains(&"plan_too_large".to_string()),
            "No plan_too_large without a plan"
        );
    }
    for record in lab.records() {
        assert!(
            record.get("sized_for").is_none(),
            "A record has no sized_for"
        );
        assert!(record.get("plan").is_none(), "A record has no plan");
    }
    assert_state_file_valid(&lab);
}

#[test]
fn every_error_a_command_line_with_no_plan_fails_with_validates_against_v0_and_v1() {
    let (lab, _h) = installed_lab();
    let released_id = {
        let doc = lab
            .run(&["lease", "create", "--project", "swim"])
            .lease_doc();
        let rid = doc["id"].as_str().unwrap().to_string();
        lab.run(&["lease", "release", &rid]).lease_doc();
        rid
    };
    let unknown = "0c7f2b5e-8a64-4f19-b3d2-6e1a9c4d8f07";
    let no_driver = Lab::new();
    let cases: Vec<(&str, Ran, &str, Option<&str>)> = vec![
        ("no subcommand", lab.run(&["lease"]), "usage", None),
        (
            "no project",
            lab.run(&["lease", "create"]),
            "usage",
            Some("project"),
        ),
        (
            "a flag with no value",
            lab.run(&["lease", "create", "--project", "x", "--ttl"]),
            "usage",
            Some("ttl"),
        ),
        ("no id", lab.run(&["lease", "get"]), "usage", Some("id")),
        (
            "an unknown option",
            lab.run(&["lease", "list", "--bogus"]),
            "usage",
            None,
        ),
        (
            "a bad id",
            lab.run(&["lease", "get", "nope"]),
            "invalid_value",
            Some("id"),
        ),
        (
            "a bad size",
            lab.run(&["lease", "create", "--project", "x", "--size", "??"]),
            "invalid_value",
            Some("size"),
        ),
        (
            "a bad state",
            lab.run(&["lease", "list", "--state", "nope"]),
            "invalid_value",
            Some("state"),
        ),
        (
            "a ttl out of range",
            lab.run(&["lease", "create", "--project", "x", "--ttl", "48h"]),
            "out_of_range",
            Some("ttl"),
        ),
        (
            "no such lease",
            lab.run(&["lease", "get", unknown]),
            "not_found",
            None,
        ),
        (
            "release of none",
            lab.run(&["lease", "release", unknown]),
            "not_found",
            None,
        ),
        (
            "renew of a released lease",
            lab.run(&["lease", "renew", &released_id]),
            "invalid_state",
            None,
        ),
        (
            "no driver",
            no_driver.run(&["lease", "create", "--project", "swim"]),
            "internal",
            None,
        ),
    ];
    let mut problems = Vec::new();
    for (what, ran, code, field) in cases {
        // The message of each is checked where its command is; this checks the document: its
        // exit code, its shape, its code and field, and v1's schema.
        for p in error_doc_problems(&ran, code, field, None) {
            problems.push(format!("{what}: {p}"));
        }
        match serde_json::from_slice::<Value>(ran.stderr.trim_ascii_end()) {
            Ok(doc) => {
                for p in schema_problems("v0/error.output.schema.json", &doc) {
                    problems.push(format!("{what}: v0: {p}"));
                }
            }
            Err(e) => problems.push(format!("{what}: not JSON: {e}")),
        }
    }
    assert_no_problems(problems);
}

#[test]
fn v0s_schemas_refuse_what_v1_adds_and_v1s_accept_it() {
    // The checker is not vacuous: a lease made with a plan fails v0's schema (lease-api.md,
    // Versioning), and passes v1's.
    let (lab, _h) = installed_lab();
    let ready = create_plan(&lab, &plan_with_agents(5), &[]).lease_doc();
    let big = create_plan(&lab, &plan_with_agents(40), &[]).lease_doc();
    for doc in [&ready, &big] {
        assert!(schema_problems("lease.output.schema.json", doc).is_empty());
        assert!(
            !schema_problems("v0/lease.output.schema.json", doc).is_empty(),
            "v0's schema accepts a plan lease: {doc}"
        );
    }
    let list = lab.run(&["lease", "list"]).list_doc();
    assert!(!schema_problems("v0/lease-list.output.schema.json", &list).is_empty());
    let error = json!({
        "code": "usage",
        "message": MSG_PLAN_AND_SIZE,
        "field": "plan"
    });
    assert!(schema_problems("error.output.schema.json", &error).is_empty());
    assert!(!schema_problems("v0/error.output.schema.json", &error).is_empty());
    // A v1 lease with a plan_too_large reason that is not alone, or with a size that is not xl, is
    // refused by v1's schema (lease-api.md, Schemas).
    for broken in [
        {
            let mut d = big.clone();
            d["reasons"] = json!([
                {"code": "plan_too_large", "message": "x"},
                {"code": "no_capacity", "message": "y"}
            ]);
            d
        },
        {
            let mut d = big.clone();
            d["size"] = json!("l");
            d
        },
        {
            let mut d = big.clone();
            d.as_object_mut().unwrap().remove("sized_for");
            d
        },
    ] {
        assert!(
            !schema_problems("lease.output.schema.json", &broken).is_empty(),
            "v1's schema accepts {broken}"
        );
    }
}

// ---- The built program, in a throwaway home --------------------------------------------------------------------

/// A throwaway home and a working folder, under the temp folder, for runs of the built `agentspace`.
struct Throwaway {
    _tmp: tempfile::TempDir,
    home: std::path::PathBuf,
}

impl Throwaway {
    fn new() -> Throwaway {
        let tmp = tempfile::Builder::new()
            .prefix("aslp-")
            .tempdir()
            .expect("A throwaway folder can be made");
        assert!(tmp.path().starts_with(std::env::temp_dir()));
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join("AppData").join("Local")).unwrap();
        std::fs::create_dir_all(home.join("AppData").join("Roaming")).unwrap();
        Throwaway { _tmp: tmp, home }
    }

    fn write_in_cwd(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.home.join(name), bytes).expect("The file can be written");
    }

    fn exists(&self, relative: &str) -> bool {
        self.home.join(relative.replace('/', "\\")).exists()
    }

    /// Runs the program in the home, as its working folder, with every variable agentspace reads
    /// passed through `Command::env`, and the provisioning program named a file that is not there,
    /// so that no run can ever be started. The way to reach the host is named too, as a command that
    /// is not there and is never run, so that the check for it passes whether or not the build put
    /// `agentspace-ssh-proxy.exe` beside the program (lease-api.md, Environment: the program is then
    /// not looked for). A workspace build places it and the isolation guard's child build does not.
    fn run(&self, line: &[&str]) -> Run {
        let mut cmd = Command::new(common::BIN);
        cmd.args(line)
            .current_dir(&self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("USERPROFILE", &self.home)
            .env("HOME", &self.home)
            .env("LOCALAPPDATA", self.home.join("AppData").join("Local"))
            .env("APPDATA", self.home.join("AppData").join("Roaming"))
            .env(
                "AGENTSPACE_PROVISION_CMD",
                self.home.join("no-such-run.cmd"),
            )
            .env("AGENTSPACE_PROXY_COMMAND", "no-such-forwarder --stdio");
        for var in [
            "AGENTSPACE_STATE_DIR",
            "AGENTSPACE_SSH_DIR",
            "AGENTSPACE_OPERATOR_CMD",
            "AGENTSPACE_CHECK_SEAM",
        ] {
            cmd.env_remove(var);
        }
        run_to_end(cmd)
    }
}

/// Runs a command to its end, and fails the test when it takes more than a minute, ending it by its
/// own handle.
fn run_to_end(mut cmd: Command) -> Run {
    let mut child = cmd.spawn().expect("The binary starts");
    let began = Instant::now();
    loop {
        match child.try_wait().expect("The child can be polled") {
            Some(_) => break,
            None if began.elapsed() > Duration::from_secs(60) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("The binary did not end within a minute");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let out = child.wait_with_output().expect("The output can be read");
    Run {
        status: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    }
}

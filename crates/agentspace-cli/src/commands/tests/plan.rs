//! `create --plan` against a fake driver in throwaway folders (lease-api.md, Plan and `create`;
//! docs/state.md, What a plan lease adds; D73 to D76, D81), and the documents of a command line
//! with no `--plan` against contract v0's schemas (D76).

use serde_json::json;

use super::*;

/// #0203's staffing plan (lease-api.md, Plan): its busiest turn, `T03`, has 4 agents at once.
const PLAN_0203: &str = r#"{"name":"v0.09","turns":[{"name":"T03","stories":[{"name":"E0.09-T03-S01","subagents":[
  {"name":"implementer","model":"Opus","effort":"high"},
  {"name":"test author","model":"Sonnet","effort":"high"},
  {"name":"runner","model":"Sonnet","effort":"medium"},
  {"name":"reviewer","model":"Opus","effort":"high","after":["implementer","test author","runner"]}]}]}]}"#;

/// A plan whose one turn, `T01`, has one story of `subagents` that start after none of the others:
/// `subagents` + 1 agents at once. No `name`.
fn wide_plan(subagents: usize) -> String {
    let list: Vec<Value> = (0..subagents)
        .map(|i| json!({"name": format!("a{i}"), "model": "Opus 5.5", "effort": "high"}))
        .collect();
    json!({"turns": [{"name": "T01", "stories": [{"name": "S01", "subagents": list}]}]}).to_string()
}

fn put_plan(env: &Env, name: &str, text: &str) {
    std::fs::write(env.dir.path().join(name), text).unwrap();
}

fn fake(log: &Log) -> Vec<Box<dyn Driver>> {
    vec![Box::new(Fake::new("fake", log))]
}

const TOO_LARGE_33: &str =
    "Plan's busiest turn 'T01' has 33 agents at once; the largest size, xl, holds 32.";

#[test]
fn create_with_a_plan_places_the_size_picked_and_prints_and_records_sized_for() {
    let env = Env::new();
    put_plan(&env, "plan.json", PLAN_0203);
    let log = Log::default();
    let out = env.run(
        &[
            "lease",
            "create",
            "--project",
            "demo",
            "--plan",
            "plan.json",
        ],
        &fake(&log),
        &[ID_A],
    );
    let d = doc(&out);
    // The driver is asked exactly as a `--size s` lease would ask it.
    assert_eq!(
        log_of(&log),
        vec![format!(
            "place {ID_A} demo linux/arm64 agent-linux s 28800 proxy=None"
        )]
    );
    assert_eq!(d["size"], "s");
    assert_eq!(
        d["sized_for"],
        json!({"plan": "v0.09", "turn": "T03", "agents": 4})
    );
    // In lease-api.md's order, right after `size`, and the plan itself is in no output.
    let keys: Vec<&str> = d.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        &keys[..7],
        [
            "id",
            "project",
            "state",
            "platform",
            "profile",
            "size",
            "sized_for"
        ]
    );
    assert!(d.get("plan").is_none());
    let text = String::from_utf8(out.stdout.clone()).unwrap();
    assert!(
        !text.contains("Sonnet") && !text.contains("medium"),
        "{text}"
    );
    // The record keeps both, the plan as parsed.
    let r = &env.records()[0];
    assert_eq!(
        r.sized_for,
        Some(agentspace_state::SizedFor {
            plan: Some("v0.09".to_owned()),
            turn: "T03".to_owned(),
            agents: 4,
        })
    );
    assert_eq!(
        r.plan,
        Some(serde_json::from_str::<Value>(PLAN_0203).unwrap())
    );
    assert_eq!(r.driver.as_deref(), Some("fake"));
}

#[test]
fn each_count_of_agents_gets_lease_apis_size() {
    for (subagents, size) in [
        (1, "s"),
        (3, "s"),
        (4, "m"),
        (7, "m"),
        (8, "l"),
        (15, "l"),
        (16, "xl"),
        (31, "xl"),
    ] {
        let env = Env::new();
        put_plan(&env, "p.json", &wide_plan(subagents));
        let log = Log::default();
        let d = doc(&env.run(
            &["lease", "create", "--project", "demo", "--plan=p.json"],
            &fake(&log),
            &[ID_A],
        ));
        assert_eq!(d["size"], size, "{subagents} subagents");
        // A plan with no name gives a sized_for with no plan member, never a null.
        assert_eq!(
            d["sized_for"],
            json!({"turn": "T01", "agents": subagents + 1}),
            "{subagents} subagents"
        );
        assert!(log_of(&log)[0].contains(&format!(" {size} 28800 ")));
    }
}

#[test]
fn a_plan_no_size_holds_is_plan_too_large_alone_at_xl_and_no_driver_places_it() {
    let mut env = Env::new();
    env.vars
        .insert("AGENTSPACE_PROXY_COMMAND".to_owned(), "proxy %h".into());
    put_plan(&env, "big.json", &wide_plan(32));
    let log = Log::default();
    // An unsupported platform and profile change nothing: the reason is alone.
    let out = env.run(
        &[
            "lease",
            "create",
            "--project",
            "demo",
            "--platform",
            "windows/amd64",
            "--profile",
            "agent-linux@2",
            "--plan",
            "big.json",
        ],
        &fake(&log),
        &[ID_A],
    );
    let d = doc(&out);
    assert!(log_of(&log).is_empty(), "{:?}", log_of(&log));
    assert_eq!(d["state"], "unsatisfiable");
    assert_eq!(d["size"], "xl");
    assert_eq!(
        d["reasons"],
        json!([{"code": "plan_too_large", "message": TOO_LARGE_33}])
    );
    assert_eq!(d["sized_for"], json!({"turn": "T01", "agents": 33}));
    let r = &env.records()[0];
    assert_eq!(r.state, "unsatisfiable");
    assert_eq!(r.driver.as_deref(), Some("fake"));
    assert!(r.plan.is_some());
    // No file was made for it: the throwaway home holds the state folder and the plan alone.
    let mut names: Vec<String> = std::fs::read_dir(env.dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, [".agentspace", "big.json"]);
}

#[test]
fn a_plan_too_large_still_needs_a_driver_first() {
    let env = Env::new();
    put_plan(&env, "big.json", &wide_plan(32));
    let out = env.run(
        &["lease", "create", "--project", "demo", "--plan", "big.json"],
        &[],
        &[ID_A],
    );
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        err(&out)["message"],
        "agentspace lease create failed: No lease driver is available. Nothing was recorded."
    );
    assert!(env.records().is_empty());
}

#[test]
fn a_plan_too_large_still_checks_the_way_to_reach_the_host() {
    let mut env = Env::new();
    put_plan(&env, "big.json", &wide_plan(32));
    let log = Log::default();
    // No override, and no ProxyCommand program beside the program: the place's own failure.
    let bin = env.dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    env.own_exe = bin.join("agentspace.exe");
    let line = ["lease", "create", "--project", "demo", "--plan", "big.json"];
    let out = env.run(&line, &fake(&log), &[ID_A]);
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        err(&out)["message"],
        "agentspace lease create failed: The lease's way to reach its host cannot be found: \
         agentspace's ProxyCommand program is not installed beside this program. No lease was \
         recorded."
    );
    assert!(env.records().is_empty());
    // With the program beside it, the lease is recorded.
    std::fs::write(bin.join("agentspace-ssh-proxy.exe"), b"").unwrap();
    let d = doc(&env.run(&line, &fake(&log), &[ID_A]));
    assert_eq!(d["reasons"][0]["code"], "plan_too_large");
    assert!(log_of(&log).is_empty());
}

#[test]
fn the_plan_is_checked_after_the_profile_and_before_the_ttl_and_before_any_wait() {
    let env = Env::new();
    let log = Log::default();
    let run = |line: &[&str]| err(&env.run(line, &fake(&log), &[ID_A]));
    let e = run(&[
        "lease",
        "create",
        "--project",
        "demo",
        "--ttl",
        "8",
        "--plan",
        "none.json",
        "--profile",
        "a b",
    ]);
    assert_eq!(e["field"], "profile");
    let e = run(&[
        "lease",
        "create",
        "--project",
        "demo",
        "--ttl",
        "8",
        "--plan",
        "none.json",
    ]);
    assert_eq!(e["code"], "invalid_value");
    assert_eq!(e["field"], "plan");
    assert_eq!(
        e["message"],
        "Plan 'none.json' is not a valid plan: the file could not be read."
    );
    // A refused plan records nothing, and is refused before the lock or the records are touched.
    assert!(!env.state_dir().exists());
    assert!(log_of(&log).is_empty());
}

#[test]
fn a_plan_parse_refuses_is_invalid_value_for_plan_with_its_why() {
    let env = Env::new();
    put_plan(
        &env,
        "cycle.json",
        r#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[
          {"name":"a","model":"m","effort":"e","after":["b"]},
          {"name":"b","model":"m","effort":"e","after":["a"]}]}]}]}"#,
    );
    let out = env.run(
        &[
            "lease",
            "create",
            "--project",
            "demo",
            "--plan",
            "cycle.json",
        ],
        &[],
        &[ID_A],
    );
    assert_eq!(out.exit_code, 2);
    let e = err(&out);
    assert_eq!(e["field"], "plan");
    assert_eq!(
        e["message"],
        "Plan 'cycle.json' is not a valid plan: the subagents of story 'S' start after each \
         other in a cycle."
    );
}

#[test]
fn plan_with_size_is_a_usage_error_for_plan() {
    let env = Env::new();
    put_plan(&env, "plan.json", PLAN_0203);
    let out = env.run(
        &[
            "lease",
            "create",
            "--project",
            "demo",
            "--plan",
            "plan.json",
            "--size",
            "m",
        ],
        &[],
        &[ID_A],
    );
    assert_eq!(out.exit_code, 2);
    let e = err(&out);
    assert_eq!(e["code"], "usage");
    assert_eq!(e["field"], "plan");
}

/// A `ready` lease made with a plan, as `create` records it.
fn planned(id: &str, state: &str) -> Record {
    let mut r = record(id, state, Some("fake"));
    r.size = "s".to_owned();
    r.sized_for = Some(agentspace_state::SizedFor {
        plan: Some("v0.09".to_owned()),
        turn: "T03".to_owned(),
        agents: 4,
    });
    r.plan = Some(serde_json::from_str(PLAN_0203).unwrap());
    r
}

#[test]
fn get_list_renew_and_release_print_sized_for_as_create_did_and_keep_the_plan() {
    let env = Env::new();
    env.put(vec![planned(ID_A, "ready"), planned(ID_B, "pending")]);
    let log = Log::default();
    let drivers = fake(&log);
    let want = json!({"plan": "v0.09", "turn": "T03", "agents": 4});
    let docs = [
        doc(&env.run(&["lease", "get", ID_A], &drivers, &[])),
        doc(&env.run(&["lease", "get", ID_B], &drivers, &[])),
        doc(&env.run(&["lease", "renew", ID_A, "--ttl", "2h"], &drivers, &[])),
        doc(&env.run(&["lease", "release", ID_A], &drivers, &[])),
        doc(&env.run(&["lease", "release", ID_B], &drivers, &[])),
    ];
    for d in &docs {
        assert_eq!(d["sized_for"], want, "{d}");
        assert!(d.get("plan").is_none(), "{d}");
    }
    let list = doc(&env.run(&["lease", "list"], &drivers, &[]));
    for d in list["leases"].as_array().unwrap() {
        assert_eq!(d["sized_for"], want, "{d}");
        assert!(d.get("plan").is_none(), "{d}");
    }
    for r in env.records() {
        assert_eq!(r.sized_for, planned(ID_A, "ready").sized_for);
        assert_eq!(r.plan, planned(ID_A, "ready").plan);
    }
}

/// Contract v0's schemas, kept unchanged under `schemas/v0/` (lease-api.md, Schemas; D76).
fn v0(name: &str) -> jsonschema::Validator {
    let text = match name {
        "lease" => include_str!("../../../../../schemas/v0/lease.output.schema.json"),
        "list" => include_str!("../../../../../schemas/v0/lease-list.output.schema.json"),
        _ => include_str!("../../../../../schemas/v0/error.output.schema.json"),
    };
    jsonschema::draft202012::new(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
}

#[test]
fn a_command_line_with_no_plan_prints_documents_v0s_schemas_accept() {
    let lease = v0("lease");
    let list = v0("list");
    let error = v0("error");
    // The check can fail: a plan lease's document is not v0's.
    let env = Env::new();
    env.put(vec![planned(ID_A, "ready")]);
    let d = doc(&env.run(&["lease", "get", ID_A], &[], &[]));
    assert!(
        !lease.is_valid(&d),
        "A plan lease's document MUST fail v0's schema"
    );

    let env = Env::new();
    let log = Log::default();
    let drivers = fake(&log);
    let mut docs = Vec::new();
    for (line, id) in [
        (&["lease", "create", "--project", "demo"][..], ID_A),
        (
            &["lease", "create", "--project", "demo", "--size", "xxl"][..],
            ID_B,
        ),
    ] {
        docs.push(doc(&env.run(line, &drivers, &[id])));
    }
    docs.push(doc(&env.run(&["lease", "get", ID_A], &drivers, &[])));
    docs.push(doc(&env.run(&["lease", "renew", ID_A], &drivers, &[])));
    docs.push(doc(&env.run(&["lease", "release", ID_A], &drivers, &[])));
    for d in &docs {
        assert!(lease.is_valid(d), "{d}");
    }
    assert!(list.is_valid(&doc(&env.run(&["lease", "list"], &drivers, &[]))));
    for line in [
        &["lease", "create", "--project", "demo", "--size", ""][..],
        &["lease", "create", "--project", "a b"][..],
        &["lease", "get", ID_B, "x"][..],
        &["lease", "renew", ID_A, "--ttl", "48h"][..],
        &["lease", "get", "aaaaaaaa-bbbb-4ccc-8ddd-000000000000"][..],
        &["lease", "renew", ID_A][..],
    ] {
        let out = env.run(line, &drivers, &[]);
        assert_ne!(out.exit_code, 0, "{line:?}");
        assert!(error.is_valid(&err(&out)), "{line:?}");
    }
}

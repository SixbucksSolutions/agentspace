//! `create --board --epic` and `--turn` against a stand-in board read and a fake driver in
//! throwaway folders (lease-api.md, The board read, The plan block, Board, Epic, Turn, TTL and
//! Exit codes and errors; docs/state.md, plan_source; D83 to D86). No test here opens a socket: the
//! board read is scripted, or reaches no further than the receipt and the credential.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use agentspace_board::{BoardError, Epic};
use agentspace_operator_api::Context as OpContext;
use agentspace_push::{FileRead, Files, Unprotect, Zeroizing};
use agentspace_state::PlanSource;
use serde_json::json;

use super::*;
use crate::commands::board::{EpicReader, Localswim};
use crate::commands::create::create_with;
use crate::{LeaseError, ParsedCommand, Sub};

const BOARD: &str = "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f";

/// Three turns: `T01` of 3 agents and 180 minutes, `T02`, the busiest, of 6 agents and 45 minutes,
/// and `T03` of 2 agents and no budget.
const PLAN_3: &str = r#"{"name":"v9.99","turns":[
  {"name":"T01","budget_minutes":180,"stories":[{"name":"S01","subagents":[
    {"name":"a","model":"Opus","effort":"high"},{"name":"b","model":"Opus","effort":"high"}]}]},
  {"name":"T02","budget_minutes":45,"stories":[{"name":"S01","subagents":[
    {"name":"a","model":"Opus","effort":"high"},{"name":"b","model":"Opus","effort":"high"},
    {"name":"c","model":"Opus","effort":"high"},{"name":"d","model":"Opus","effort":"high"},
    {"name":"e","model":"Opus","effort":"high"}]}]},
  {"name":"T03","stories":[{"name":"S01","subagents":[
    {"name":"a","model":"Opus","effort":"high"}]}]}]}"#;

/// `T01` fits, and `T02`, the busiest, has 33 agents at once; `T02` has a budget of 960 minutes.
fn plan_with_a_turn_too_large() -> String {
    let wide: Vec<Value> = (0..32)
        .map(|i| json!({"name": format!("a{i}"), "model": "Opus", "effort": "high"}))
        .collect();
    json!({"turns": [
        {"name": "T01", "stories": [{"name": "S", "subagents": [
            {"name": "a", "model": "Opus", "effort": "high"}]}]},
        {"name": "T02", "budget_minutes": 960, "stories": [{"name": "S", "subagents": wide}]}
    ]})
    .to_string()
}

/// A description that carries `plan` in its one block, among prose.
fn described(plan: &str) -> String {
    format!("The epic's prose.\r\n\n```agentspace-plan\n{plan}\n```\nMore prose, after it.\n")
}

fn epic_of(description: String) -> Epic {
    Epic {
        description,
        archived: false,
        revision: 41,
    }
}

/// A board read that answers as it is told, and records what it was asked.
struct Scripted {
    answer: Result<Epic, BoardError>,
    calls: RefCell<Vec<(Option<PathBuf>, String, u64)>>,
}

impl Scripted {
    fn new(answer: Result<Epic, BoardError>) -> Scripted {
        Scripted {
            answer,
            calls: RefCell::new(Vec::new()),
        }
    }

    fn plan(plan: &str) -> Scripted {
        Scripted::new(Ok(epic_of(described(plan))))
    }

    fn calls(&self) -> usize {
        self.calls.borrow().len()
    }
}

impl EpicReader for Scripted {
    fn read(&self, ctx: &OpContext, board: &str, ticket: u64) -> Result<Epic, BoardError> {
        self.calls
            .borrow_mut()
            .push((ctx.local_app_data.clone(), board.to_owned(), ticket));
        self.answer.clone()
    }
}

/// Runs `args` in `env`'s world, with `cwd` as its working folder, and `create` given `reader`.
fn run_in(
    env: &Env,
    args: &[&str],
    drivers: &[Box<dyn Driver>],
    ids: &[&str],
    reader: &dyn EpicReader,
    cwd: Option<&Path>,
) -> Outcome {
    let ids: RefCell<Vec<String>> =
        RefCell::new(ids.iter().rev().map(|s| (*s).to_owned()).collect());
    let vars = env.vars.clone();
    let var = move |name: &str| vars.get(name).cloned();
    let now = || NOW;
    let new_id = || ids.borrow_mut().pop().unwrap_or_default();
    let world = World {
        var: &var,
        cwd,
        own_exe: &env.own_exe,
        now: &now,
        new_id: &new_id,
        drivers,
    };
    let runner = |cmd: ParsedCommand, w: &World<'_>| -> Result<Value, LeaseError> {
        match cmd.sub {
            Sub::Create => create_with(&cmd, w, reader),
            _ => crate::commands::run(cmd, w),
        }
    };
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    crate::run::execute_with(&args, &world, &runner)
}

fn run(env: &Env, args: &[&str], drivers: &[Box<dyn Driver>], reader: &dyn EpicReader) -> Outcome {
    run_in(env, args, drivers, &[ID_A], reader, Some(env.dir.path()))
}

fn fake(log: &Log) -> Vec<Box<dyn Driver>> {
    vec![Box::new(Fake::new("fake", log))]
}

/// `lease create --project demo` and then `rest`.
fn create(rest: &[&'static str]) -> Vec<&'static str> {
    let mut line = vec!["lease", "create", "--project", "demo"];
    line.extend_from_slice(rest);
    line
}

fn board_line(rest: &[&'static str]) -> Vec<&'static str> {
    let mut line = create(&["--board", BOARD, "--epic", "0052"]);
    line.extend_from_slice(rest);
    line
}

// What a board lease is, and what it records.

#[test]
fn a_board_lease_is_the_plan_lease_and_records_where_its_plan_came_from() {
    let env = Env::new();
    let log = Log::default();
    let reader = Scripted::plan(PLAN_3);
    let d = doc(&run(&env, &board_line(&[]), &fake(&log), &reader));
    // The board was read once, by its normalized ID and ticket.
    let calls = reader.calls.borrow().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].1.as_str(), calls[0].2), (BOARD, 52));
    // The busiest turn, T02: 6 agents, size m, and its budget's TTL, 45 minutes and half again.
    assert_eq!(
        log_of(&log),
        vec![format!(
            "place {ID_A} demo linux/arm64 agent-linux m 4080 proxy=None"
        )]
    );
    assert_eq!(d["size"], "m");
    assert_eq!(d["ttl_seconds"], 4080);
    assert_eq!(
        d["sized_for"],
        json!({"plan": "v9.99", "turn": "T02", "agents": 6})
    );
    // No output carries the board, the ticket, the revision or the plan.
    let text = d.to_string();
    for absent in [BOARD, "52", "41", "budget", "Opus", "plan_source"] {
        assert!(!text.contains(absent), "{absent}");
    }
    let r = &env.records()[0];
    assert_eq!(
        r.plan_source,
        Some(PlanSource {
            board: BOARD.to_owned(),
            ticket: 52,
            revision: 41,
        })
    );
    assert_eq!(r.plan, Some(serde_json::from_str::<Value>(PLAN_3).unwrap()));
}

#[test]
fn the_same_plan_and_turn_give_the_same_lease_from_a_file_and_from_an_epic() {
    for turn in [None, Some("T01"), Some("T02"), Some("T03")] {
        let file_env = Env::new();
        std::fs::write(file_env.dir.path().join("p.json"), PLAN_3).unwrap();
        let mut file_line = create(&["--plan", "p.json"]);
        let mut epic_line = board_line(&[]);
        if let Some(turn) = turn {
            file_line.extend(["--turn", turn]);
            epic_line.extend(["--turn", turn]);
        }
        let log = Log::default();
        let unused = Scripted::new(Err(BoardError::UnknownBoard));
        let from_file = doc(&run(&file_env, &file_line, &fake(&log), &unused));
        assert_eq!(unused.calls(), 0, "A --plan lease reads no board");
        let epic_env = Env::new();
        let from_epic = doc(&run(
            &epic_env,
            &epic_line,
            &fake(&log),
            &Scripted::plan(PLAN_3),
        ));
        assert_eq!(from_file, from_epic, "{turn:?}");
        // Only the board lease records a plan_source.
        assert_eq!(file_env.records()[0].plan_source, None);
        assert!(epic_env.records()[0].plan_source.is_some());
        assert_eq!(file_env.records()[0].plan, epic_env.records()[0].plan);
    }
}

#[test]
fn a_lease_with_no_plan_records_neither_plan_nor_plan_source() {
    let env = Env::new();
    let log = Log::default();
    let reader = Scripted::plan(PLAN_3);
    doc(&run(&env, &create(&[]), &fake(&log), &reader));
    assert_eq!(reader.calls(), 0);
    let r = &env.records()[0];
    assert_eq!((r.plan.is_none(), r.plan_source.is_none()), (true, true));
    assert_eq!(r.ttl_seconds, 28800);
}

// The TTL a plan gives (lease-api.md, TTL; D85).

#[test]
fn the_ttl_is_the_lease_turns_budget_and_half_again_or_the_default_without_one() {
    for (turn, size, ttl) in [
        (None, "m", 4080),
        (Some("T01"), "s", 16200),
        (Some("T02"), "m", 4080),
        (Some("T03"), "s", 28800),
    ] {
        let env = Env::new();
        let log = Log::default();
        let mut line = board_line(&[]);
        if let Some(turn) = turn {
            line.extend(["--turn", turn]);
        }
        let d = doc(&run(&env, &line, &fake(&log), &Scripted::plan(PLAN_3)));
        assert_eq!(
            (d["size"].as_str(), d["ttl_seconds"].as_i64()),
            (Some(size), Some(ttl)),
            "{turn:?}"
        );
        assert_eq!(d["sized_for"]["turn"], turn.unwrap_or("T02"), "{turn:?}");
        assert!(
            log_of(&log)[0].ends_with(&format!(" {size} {ttl} proxy=None")),
            "{turn:?}"
        );
    }
}

#[test]
fn ttl_wins_over_the_turns_budget_and_over_the_default() {
    for turn in [None, Some("T01"), Some("T03")] {
        let env = Env::new();
        let log = Log::default();
        let mut line = board_line(&["--ttl", "2h"]);
        if let Some(turn) = turn {
            line.extend(["--turn", turn]);
        }
        let d = doc(&run(&env, &line, &fake(&log), &Scripted::plan(PLAN_3)));
        assert_eq!(d["ttl_seconds"], 7200, "{turn:?}");
    }
}

#[test]
fn turn_on_a_plan_file_sizes_for_that_turn_and_gives_its_ttl() {
    let env = Env::new();
    std::fs::write(env.dir.path().join("p.json"), PLAN_3).unwrap();
    let log = Log::default();
    let unused = Scripted::plan(PLAN_3);
    let d = doc(&run(
        &env,
        &create(&["--plan", "p.json", "--turn", "T01"]),
        &fake(&log),
        &unused,
    ));
    assert_eq!(d["size"], "s");
    assert_eq!(d["ttl_seconds"], 16200);
    assert_eq!(
        d["sized_for"],
        json!({"plan": "v9.99", "turn": "T01", "agents": 3})
    );
    assert_eq!(unused.calls(), 0);
}

// plan_too_large with and without --turn (lease-api.md, Fields; D84).

#[test]
fn plan_too_large_has_its_turn_message_with_turn_and_the_busiest_message_without() {
    let plan = plan_with_a_turn_too_large();
    let new_env = || {
        let mut env = Env::new();
        env.vars
            .insert("AGENTSPACE_PROXY_COMMAND".to_owned(), "proxy %h".into());
        std::fs::write(env.dir.path().join("big.json"), &plan).unwrap();
        env
    };
    let log = Log::default();
    let cases: [(Vec<&str>, &str); 3] = [
        (
            board_line(&["--turn", "T02"]),
            "Plan's turn 'T02' has 33 agents at once; the largest size, xl, holds 32.",
        ),
        (
            create(&["--plan", "big.json", "--turn", "T02"]),
            "Plan's turn 'T02' has 33 agents at once; the largest size, xl, holds 32.",
        ),
        (
            board_line(&[]),
            "Plan's busiest turn 'T02' has 33 agents at once; the largest size, xl, holds 32.",
        ),
    ];
    for (line, message) in cases {
        let d = doc(&run(&new_env(), &line, &fake(&log), &Scripted::plan(&plan)));
        assert_eq!(d["state"], "unsatisfiable");
        assert_eq!(d["size"], "xl");
        assert_eq!(
            d["reasons"],
            json!([{"code": "plan_too_large", "message": message}])
        );
        // An unsatisfiable lease carries the TTL its turn gives: 960 minutes, 24 hours.
        assert_eq!(d["ttl_seconds"], 86400);
    }
    assert!(
        log_of(&log).is_empty(),
        "No driver places a plan_too_large lease"
    );
    // A turn that fits is placed, though the busiest would not.
    let d = doc(&run(
        &new_env(),
        &board_line(&["--turn", "T01"]),
        &fake(&log),
        &Scripted::plan(&plan),
    ));
    assert_eq!(
        (d["size"].as_str(), d["state"].as_str()),
        (Some("s"), Some("ready"))
    );
}

// --turn against the plan (lease-api.md, Turn; Messages).

#[test]
fn a_turn_the_plan_lacks_is_invalid_value_for_turn_naming_the_plans_turns_from_either_source() {
    let env = Env::new();
    std::fs::write(env.dir.path().join("p.json"), PLAN_3).unwrap();
    let log = Log::default();
    for line in [
        board_line(&["--turn", "t01"]),
        create(&["--plan", "p.json", "--turn", "t01"]),
    ] {
        let reader = Scripted::plan(PLAN_3);
        let out = run(&env, &line, &fake(&log), &reader);
        assert_eq!(out.exit_code, 2);
        let e = err(&out);
        assert_eq!(e["code"], "invalid_value");
        assert_eq!(e["field"], "turn");
        assert_eq!(
            e["message"],
            "Turn 't01' is not a turn of the plan; its turns are 'T01', 'T02', 'T03'."
        );
    }
    assert!(log_of(&log).is_empty());
    assert!(
        !env.state_dir().exists(),
        "A refused turn touches no record"
    );
}

// Each outcome of the board read (lease-api.md, The board read, "Each outcome"; pin P3).

fn refusal(answer: Result<Epic, BoardError>) -> (Value, Env) {
    let env = Env::new();
    let log = Log::default();
    let reader = Scripted::new(answer);
    let out = run(
        &env,
        &create(&["--epic", " 0052 ", "--board", BOARD]),
        &fake(&log),
        &reader,
    );
    assert_ne!(out.exit_code, 0);
    assert_eq!(reader.calls(), 1);
    assert!(log_of(&log).is_empty());
    let e = err(&out);
    assert_eq!(out.exit_code, if e["code"] == "internal" { 1 } else { 2 });
    (e, env)
}

#[test]
fn each_board_read_outcome_is_framed_as_lease_api_gives_it_and_records_nothing() {
    let internal = |text: &str| {
        json!({
            "code": "internal",
            "message": format!("agentspace lease create failed: {text}. Nothing was recorded.")
        })
    };
    let invalid = |field: &str, message: &str| json!({"code": "invalid_value", "message": message, "field": field});
    let archived = Epic {
        archived: true,
        ..epic_of(described(PLAN_3))
    };
    let cases: Vec<(Result<Epic, BoardError>, Value)> = vec![
        (
            Err(BoardError::Internal(
                "The board at 127.0.0.1:49999 did not answer".to_owned(),
            )),
            internal("The board at 127.0.0.1:49999 did not answer"),
        ),
        (
            Err(BoardError::Internal(
                "The board refused agentspace's credential".to_owned(),
            )),
            internal("The board refused agentspace's credential"),
        ),
        (
            Err(BoardError::UnknownBoard),
            invalid(
                "board",
                &format!("No board of ID '{BOARD}' that agentspace can read."),
            ),
        ),
        (
            Err(BoardError::UnknownTicket),
            invalid(
                "epic",
                &format!(
                    "No card of ticket ' 0052 ' on board '{BOARD}'. Give the epic's ticket \
                     number, as in '52'."
                ),
            ),
        ),
        (
            Err(BoardError::TooLarge),
            invalid(
                "epic",
                "Epic ' 0052 ' carries no usable plan: the board's answer for this card is \
                 larger than 64 MiB.",
            ),
        ),
        (
            Ok(archived),
            invalid(
                "epic",
                "Epic ' 0052 ' is archived. Name an epic that is being worked.",
            ),
        ),
        (
            Ok(epic_of("Prose with no block.\n".to_owned())),
            invalid(
                "epic",
                "Epic ' 0052 ' carries no usable plan: its description has no agentspace-plan \
                 block.",
            ),
        ),
        (
            Ok(epic_of(format!(
                "{}{}",
                described(PLAN_3),
                described(PLAN_3)
            ))),
            invalid(
                "epic",
                "Epic ' 0052 ' carries no usable plan: its description has more than one \
                 agentspace-plan block.",
            ),
        ),
        (
            Ok(epic_of(described("{"))),
            invalid(
                "epic",
                "Epic ' 0052 ' carries no usable plan: the file is not valid JSON.",
            ),
        ),
        (
            Ok(epic_of(described(
                r#"{"turns":[{"name":"T","budget_minutes":90.0,"stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]}]}"#,
            ))),
            invalid(
                "epic",
                "Epic ' 0052 ' carries no usable plan: the document does not match the plan \
                 schema.",
            ),
        ),
    ];
    for (answer, want) in cases {
        let (e, env) = refusal(answer);
        assert_eq!(e, want);
        // Nothing was recorded, and the lease records were never reached.
        assert!(!env.state_dir().exists(), "{want}");
    }
}

#[test]
fn an_archived_epic_is_refused_before_its_description_is_read() {
    // An archived card whose description has no block is refused as archived.
    let (e, _env) = refusal(Ok(Epic {
        archived: true,
        ..epic_of(String::new())
    }));
    assert_eq!(
        e["message"],
        "Epic ' 0052 ' is archived. Name an epic that is being worked."
    );
}

// The order of the checks (lease-api.md, Exit codes and errors, step 2; pins P3 and P20).

#[test]
fn a_value_that_fails_its_form_or_range_reads_no_board() {
    let env = Env::new();
    let log = Log::default();
    let cases: [(Vec<&str>, &str, &str); 9] = [
        (board_line(&["--ttl", "8"]), "invalid_value", "ttl"),
        (board_line(&["--ttl", "48h"]), "out_of_range", "ttl"),
        (board_line(&["--turn", " T01"]), "invalid_value", "turn"),
        (
            create(&["--board", BOARD, "--epic", "#52"]),
            "invalid_value",
            "epic",
        ),
        (
            create(&["--board", "B", "--epic", "52"]),
            "invalid_value",
            "board",
        ),
        (
            board_line(&["--profile", "a b"]),
            "invalid_value",
            "profile",
        ),
        (
            board_line(&["--platform", "x"]),
            "invalid_value",
            "platform",
        ),
        // lease-api.md's own example: --ttl 8 with --epic and --board reads no board.
        (
            create(&["--epic", "7", "--board", BOARD, "--ttl", "8"]),
            "invalid_value",
            "ttl",
        ),
        (
            vec![
                "lease",
                "create",
                "--epic",
                "7",
                "--board",
                BOARD,
                "--project",
                "a b",
            ],
            "invalid_value",
            "project",
        ),
    ];
    for (line, code, field) in cases {
        let reader = Scripted::plan(PLAN_3);
        let out = run(&env, &line, &fake(&log), &reader);
        let e = err(&out);
        assert_eq!(
            (e["code"].as_str(), e["field"].as_str()),
            (Some(code), Some(field)),
            "{line:?}"
        );
        assert_eq!(reader.calls(), 0, "{line:?} read the board");
    }
    assert!(!env.state_dir().exists());
}

#[test]
fn the_values_forms_come_in_lease_apis_order_board_epic_turn_size_ttl() {
    let env = Env::new();
    let log = Log::default();
    let reader = Scripted::plan(PLAN_3);
    let field = |line: &[&str]| {
        let e = err(&run(&env, line, &fake(&log), &reader));
        e["field"].as_str().unwrap_or_default().to_owned()
    };
    assert_eq!(
        field(&create(&[
            "--ttl", "8", "--turn", " T", "--epic", "x", "--board", "y"
        ])),
        "board"
    );
    assert_eq!(
        field(&create(&[
            "--ttl", "8", "--turn", " T", "--epic", "x", "--board", BOARD
        ])),
        "epic"
    );
    assert_eq!(field(&board_line(&["--ttl", "8", "--turn", " T"])), "turn");
    // A bad plan file comes before a bad turn, and a bad profile before the plan.
    assert_eq!(
        field(&create(&["--turn", " T", "--plan", "none.json"])),
        "plan"
    );
    assert_eq!(
        field(&create(&[
            "--turn",
            " T",
            "--plan",
            "none.json",
            "--profile",
            "a b"
        ])),
        "profile"
    );
    // A turn the plan lacks comes after the TTL's range and after the board read.
    assert_eq!(field(&board_line(&["--turn", "T9", "--ttl", "48h"])), "ttl");
    assert_eq!(reader.calls(), 0);
    let refused = Scripted::new(Err(BoardError::UnknownTicket));
    let e = err(&run(
        &env,
        &board_line(&["--turn", "T9"]),
        &fake(&log),
        &refused,
    ));
    assert_eq!(e["field"], "epic");
}

#[test]
fn a_usage_error_reads_no_board() {
    let env = Env::new();
    let log = Log::default();
    let reader = Scripted::plan(PLAN_3);
    for line in [
        create(&["--board", BOARD]),
        create(&["--epic", "52"]),
        create(&["--turn", "T01"]),
        board_line(&["--size", "m"]),
        board_line(&["--bogus"]),
    ] {
        let out = run(&env, &line, &fake(&log), &reader);
        assert_eq!(err(&out)["code"], "usage", "{line:?}");
    }
    assert_eq!(reader.calls(), 0);
}

#[test]
fn the_board_is_read_before_the_lease_records_and_a_refused_epic_touches_none() {
    let env = Env::new();
    // Records that cannot be read: a board refusal is still what is reported.
    std::fs::create_dir_all(env.state_dir()).unwrap();
    std::fs::write(env.state_dir().join("leases.json"), b"not json").unwrap();
    let log = Log::default();
    for answer in [
        Err(BoardError::UnknownTicket),
        Err(BoardError::Internal(
            "The board's answer could not be read".to_owned(),
        )),
        Ok(epic_of("no block".to_owned())),
    ] {
        let reader = Scripted::new(answer);
        let e = err(&run(&env, &board_line(&[]), &fake(&log), &reader));
        assert!(
            !e["message"]
                .as_str()
                .unwrap()
                .starts_with("The lease records"),
            "{e}"
        );
        assert_eq!(reader.calls(), 1);
    }
    // A turn the plan lacks is reported before the records too.
    let e = err(&run(
        &env,
        &board_line(&["--turn", "T9"]),
        &fake(&log),
        &Scripted::plan(PLAN_3),
    ));
    assert_eq!(e["field"], "turn");
    // Nothing touched the state folder: no lock file, and the records as they were.
    let mut names: Vec<String> = std::fs::read_dir(env.state_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["leases.json"]);
    assert_eq!(
        std::fs::read(env.state_dir().join("leases.json")).unwrap(),
        b"not json"
    );
    // With a plan the board gives, the records are then read, and their failure is reported.
    let e = err(&run(
        &env,
        &board_line(&[]),
        &fake(&log),
        &Scripted::plan(PLAN_3),
    ));
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .starts_with("The lease records could not be read"),
        "{e}"
    );
    assert!(log_of(&log).is_empty());
}

// The board read's context, from the command's own environment (pin P1).

/// Files that are a map, which records each path read.
#[derive(Default)]
struct MapFiles {
    files: HashMap<PathBuf, FileRead>,
    reads: RefCell<Vec<PathBuf>>,
}

impl Files for MapFiles {
    fn read(&self, path: &Path) -> FileRead {
        self.reads.borrow_mut().push(path.to_path_buf());
        self.files.get(path).cloned().unwrap_or(FileRead::Missing)
    }
    fn write_stamp(&self, _path: &Path, _bytes: &[u8]) -> bool {
        false
    }
}

/// A decryption that refuses every blob.
struct NoUnprotect;

impl Unprotect for NoUnprotect {
    fn unprotect(&self, _blob: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        None
    }
}

fn receipt_at(dir: &Path) -> PathBuf {
    dir.join("localswim-v2").join("service.json")
}

/// A receipt whose credential folder holds no `bot.json`, so the read stops before any socket.
const RECEIPT: &[u8] =
    br#"{"base":"http://127.0.0.1:49999","credential_directory":"no-such-folder"}"#;

fn message_with(env: &Env, files: &MapFiles, cwd: Option<&Path>) -> String {
    let log = Log::default();
    let reader = Localswim {
        files,
        unprotect: &NoUnprotect,
    };
    let out = run_in(env, &board_line(&[]), &fake(&log), &[ID_A], &reader, cwd);
    assert_eq!(out.exit_code, 1);
    assert!(log_of(&log).is_empty());
    err(&out)["message"].as_str().unwrap().to_owned()
}

const FAILED: &str = "agentspace lease create failed: ";

#[test]
fn localappdata_unset_or_blank_is_the_first_outcome_and_reads_no_file() {
    for value in [None, Some(""), Some(" \t")] {
        let mut env = Env::new();
        if let Some(v) = value {
            env.vars.insert("LOCALAPPDATA".to_owned(), v.into());
        }
        let files = MapFiles::default();
        assert_eq!(
            message_with(&env, &files, Some(env.dir.path())),
            format!(
                "{FAILED}LOCALAPPDATA is unset or empty, so localswim's service receipt cannot \
                 be found. Nothing was recorded."
            ),
            "{value:?}"
        );
        assert!(files.reads.borrow().is_empty(), "{value:?}");
        assert!(!env.state_dir().exists());
    }
}

#[test]
fn the_receipt_is_read_under_the_worlds_localappdata() {
    let mut env = Env::new();
    let appdata = env.dir.path().join("appdata");
    env.vars
        .insert("LOCALAPPDATA".to_owned(), appdata.clone().into_os_string());
    // Nothing at the receipt's path.
    let files = MapFiles::default();
    assert_eq!(
        message_with(&env, &files, Some(env.dir.path())),
        format!(
            "{FAILED}localswim's service receipt was not found; localswim may not be running. \
             Nothing was recorded."
        )
    );
    assert!(
        *files.reads.borrow() == [receipt_at(&appdata)],
        "The receipt was not read at the world's LOCALAPPDATA"
    );
    // A receipt that breaks localswim's rules there.
    let mut files = MapFiles::default();
    files
        .files
        .insert(receipt_at(&appdata), FileRead::Bytes(b"[]".to_vec()));
    assert_eq!(
        message_with(&env, &files, Some(env.dir.path())),
        format!("{FAILED}localswim's service receipt could not be read. Nothing was recorded.")
    );
    // A good receipt, and no credential.
    let mut files = MapFiles::default();
    files
        .files
        .insert(receipt_at(&appdata), FileRead::Bytes(RECEIPT.to_vec()));
    assert_eq!(
        message_with(&env, &files, Some(env.dir.path())),
        format!("{FAILED}The board's credential could not be read. Nothing was recorded.")
    );
    assert_eq!(files.reads.borrow().len(), 2);
    assert!(!env.state_dir().exists());
}

#[test]
fn a_relative_localappdata_is_read_against_the_worlds_working_folder() {
    let mut env = Env::new();
    env.vars.insert("LOCALAPPDATA".to_owned(), "rel".into());
    let mut files = MapFiles::default();
    let full = env.dir.path().join("rel");
    files
        .files
        .insert(receipt_at(&full), FileRead::Bytes(RECEIPT.to_vec()));
    assert_eq!(
        message_with(&env, &files, Some(env.dir.path())),
        format!("{FAILED}The board's credential could not be read. Nothing was recorded.")
    );
    assert!(
        files.reads.borrow()[0] == receipt_at(&full),
        "The relative receipt was not made full against the working folder"
    );
    // With no working folder, the receipt cannot be read, and no file is.
    let mut env = Env::without_home();
    env.vars.insert("LOCALAPPDATA".to_owned(), "rel".into());
    let files = MapFiles::default();
    assert_eq!(
        message_with(&env, &files, None),
        format!("{FAILED}localswim's service receipt could not be read. Nothing was recorded.")
    );
    assert!(files.reads.borrow().is_empty());
}

#[test]
fn the_scripted_read_is_given_the_worlds_localappdata() {
    let mut env = Env::new();
    let appdata = env.dir.path().join("appdata");
    env.vars
        .insert("LOCALAPPDATA".to_owned(), appdata.clone().into_os_string());
    let log = Log::default();
    let reader = Scripted::plan(PLAN_3);
    doc(&run(&env, &board_line(&[]), &fake(&log), &reader));
    assert!(
        reader.calls.borrow()[0].0 == Some(appdata),
        "The board read's context did not carry the world's LOCALAPPDATA"
    );
}

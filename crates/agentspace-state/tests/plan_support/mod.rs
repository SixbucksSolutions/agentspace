//! Fixtures for the plan-record and sized_for tests (docs/state.md, What a plan lease adds;
//! docs/lease-api.md, Fields). Fixtures only ever name made-up values. The records are built in
//! the order docs/state.md writes them: `sized_for` right after `size`, `plan` last.
#![allow(dead_code)]

use crate::common::{pending, ready, released_from_ready, unsatisfiable, with};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::PathBuf;

/// `schemas/state/samples`, where the lease state schema's samples are.
pub fn state_samples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/state/samples")
}

/// A sample's text; `rel` is `name.json`, or `invalid/name.json`.
pub fn state_sample_text(rel: &str) -> String {
    fs::read_to_string(state_samples_dir().join(rel)).expect("The sample must be readable")
}

pub fn state_sample(rel: &str) -> Value {
    serde_json::from_str(&state_sample_text(rel)).expect("The sample must be JSON")
}

/// The names of the `leases.state.*.json` files in a samples folder (`""` or `"invalid"`).
pub fn state_sample_names(sub: &str) -> Vec<String> {
    let dir = state_samples_dir().join(sub);
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("leases.state.") && n.ends_with(".json"))
        .collect();
    v.sort();
    v
}

/// The smallest plan: one turn, one story, one subagent.
pub fn minimal_plan() -> Value {
    json!({"turns": [{"name": "T01", "stories": [
        {"name": "S01", "subagents": [{"name": "a", "model": "Opus", "effort": "high"}]}
    ]}]})
}

/// A plan with a name, two stories and a subagent that waits for another.
pub fn rich_plan() -> Value {
    json!({"name": "v0.10", "turns": [{"name": "T01", "stories": [
        {"name": "S01", "subagents": [
            {"name": "drafter", "model": "Opus", "effort": "high"},
            {"name": "reviewer", "model": "Opus", "effort": "high", "after": ["drafter"]}
        ]},
        {"name": "S02", "subagents": [
            {"name": "drafter", "model": "Opus", "effort": "high"},
            {"name": "checker", "model": "Sonnet", "effort": "medium"}
        ]}
    ]}]})
}

/// A `sized_for` member, in the lease document's order: plan, turn, agents.
pub fn sized_for(plan: Option<&str>, turn: &str, agents: u64) -> Value {
    let mut m = Map::new();
    if let Some(p) = plan {
        m.insert("plan".to_string(), json!(p));
    }
    m.insert("turn".to_string(), json!(turn));
    m.insert("agents".to_string(), json!(agents));
    Value::Object(m)
}

/// A record with `sized_for` put right after `size` and `plan` put last, which is where
/// docs/state.md writes them. The record must not hold either already.
pub fn planned(rec: Value, sized_for: Value, plan: Value) -> Value {
    let mut out = Map::new();
    for (k, v) in rec.as_object().unwrap() {
        assert!(
            k != "sized_for" && k != "plan",
            "The record already holds {k}"
        );
        out.insert(k.clone(), v.clone());
        if k == "size" {
            out.insert("sized_for".to_string(), sized_for.clone());
        }
    }
    out.insert("plan".to_string(), plan);
    Value::Object(out)
}

fn named_sf() -> Value {
    sized_for(Some("v0.10"), "T01", 2)
}

pub fn plan_pending(id: &str) -> Value {
    planned(
        with(pending(id, "demo"), "size", Some(json!("s"))),
        named_sf(),
        minimal_plan(),
    )
}

pub fn plan_ready(id: &str) -> Value {
    planned(
        with(ready(id, "demo"), "size", Some(json!("s"))),
        named_sf(),
        minimal_plan(),
    )
}

pub fn plan_released_from_ready(id: &str) -> Value {
    planned(
        with(released_from_ready(id, "demo"), "size", Some(json!("s"))),
        named_sf(),
        minimal_plan(),
    )
}

pub fn plan_released_from_pending(id: &str) -> Value {
    planned(
        with(
            with(released_from_ready(id, "demo"), "expires_at", None),
            "size",
            Some(json!("s")),
        ),
        named_sf(),
        minimal_plan(),
    )
}

/// An `unsatisfiable` plan lease whose one reason is the fixture's `unsupported_platform`.
pub fn plan_unsatisfiable(id: &str) -> Value {
    planned(
        with(unsatisfiable(id, "demo"), "size", Some(json!("s"))),
        named_sf(),
        minimal_plan(),
    )
}

pub const TOO_LARGE_MESSAGE: &str =
    "Plan's busiest turn 'T02' has 33 agents at once; the largest size, xl, holds 32.";

/// A `plan_too_large` record: unsatisfiable, `xl`, its one reason `plan_too_large`.
pub fn plan_too_large(id: &str) -> Value {
    let base = with(
        with(
            with(
                unsatisfiable(id, "demo"),
                "platform",
                Some(json!("linux/arm64")),
            ),
            "size",
            Some(json!("xl")),
        ),
        "reasons",
        Some(json!([{"code": "plan_too_large", "message": TOO_LARGE_MESSAGE}])),
    );
    planned(base, sized_for(Some("v0.11"), "T02", 33), rich_plan())
}

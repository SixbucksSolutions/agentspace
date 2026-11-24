//! Every sample in schemas/samples validates, and every one in schemas/samples/invalid fails
//! (lease-api.md, Schemas). The folders are read from the repo at test time.

use agentspace_state::validate_schema;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn schemas_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

fn json_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    v.sort();
    v
}

/// The contract's schemas: schemas/*.schema.json, named without `.schema.json`.
fn contract_schemas() -> Vec<String> {
    json_names(&schemas_dir())
        .into_iter()
        .filter_map(|n| n.strip_suffix(".schema.json").map(str::to_string))
        .collect()
}

fn schema_of<'a>(file: &str, schemas: &'a [String]) -> &'a str {
    let hits: Vec<&String> = schemas
        .iter()
        .filter(|s| file.starts_with(&format!("{s}.")))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "Sample {file} must match exactly one schema, got {hits:?}"
    );
    hits[0]
}

fn check_dir(dir: &Path, expect_valid: bool) -> Vec<String> {
    let schemas = contract_schemas();
    let mut used = Vec::new();
    for file in json_names(dir) {
        let schema = schema_of(&file, &schemas);
        let text = fs::read_to_string(dir.join(&file)).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();
        let got = validate_schema(schema, &doc);
        assert_eq!(
            got,
            Some(expect_valid),
            "Sample {file} against {schema}: expected valid = {expect_valid}"
        );
        used.push(schema.to_string());
    }
    used
}

/// Contract v1 has nine schemas: v0's eight and `plan.input` (D76). v0's eight, kept under
/// `schemas/v0/`, are not among them, since only files directly in `schemas/` are read.
#[test]
fn there_are_nine_contract_schemas() {
    let schemas = contract_schemas();
    assert_eq!(schemas.len(), 9, "{schemas:?}");
    assert!(schemas.iter().any(|s| s == "plan.input"), "{schemas:?}");
}

#[test]
fn every_valid_sample_validates_and_every_schema_has_one() {
    let used = check_dir(&schemas_dir().join("samples"), true);
    for s in contract_schemas() {
        assert!(used.contains(&s), "Schema {s} has no valid sample");
    }
}

#[test]
fn every_invalid_sample_fails_and_every_schema_has_one() {
    let used = check_dir(&schemas_dir().join("samples/invalid"), false);
    for s in contract_schemas() {
        assert!(used.contains(&s), "Schema {s} has no invalid sample");
    }
}

#[test]
fn unknown_schema_name_is_none() {
    assert_eq!(validate_schema("no-such.output", &json!({})), None);
    assert_eq!(validate_schema("", &json!({})), None);
}

#[test]
fn state_schema_is_known() {
    // The four state schemas have no sample files in the repo, so the lease state schema is
    // checked here with documents made in the test.
    let ok = json!({"version": 0, "leases": []});
    assert_eq!(validate_schema("leases.state", &ok), Some(true));
    let bad = json!({"version": 1, "leases": []});
    assert_eq!(validate_schema("leases.state", &bad), Some(false));
}

#[test]
fn pattern_lookahead_refuses_a_trailing_line_feed() {
    // The `(?!\n)` lookahead: a bare `$` would accept an ID followed by LF.
    let text = fs::read_to_string(schemas_dir().join("samples/lease.output.ready.json")).unwrap();
    let mut doc: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(validate_schema("lease.output", &doc), Some(true));
    let id = doc["id"].as_str().unwrap().to_string();
    doc["id"] = json!(format!("{id}\n"));
    assert_eq!(validate_schema("lease.output", &doc), Some(false));
}

#[test]
fn whitespace_class_with_unicode_escapes_works() {
    // Input patterns hold a class written with \u escapes: U+3000 is accepted around a value,
    // and U+FEFF, which is not White_Space, is not.
    let ok = json!({"project": "\u{3000}swimcoach\u{3000}"});
    assert_eq!(validate_schema("lease-create.input", &ok), Some(true));
    let bad = json!({"project": "\u{feff}swimcoach"});
    assert_eq!(validate_schema("lease-create.input", &bad), Some(false));
}

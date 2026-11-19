//! The twelve schemas, embedded at build time, and the validator that checks against them.
//!
//! The validator is draft 2020-12 and fetches nothing: the `jsonschema` crate is built without its
//! `resolve-http` and `resolve-file` features, and every schema here is self-contained, so a
//! reference outside its own document cannot be followed (D38).

use jsonschema::Validator;
use serde_json::Value;
use std::sync::OnceLock;

macro_rules! schema {
    ($path:literal) => {
        include_str!(concat!("../../../schemas/", $path))
    };
}

/// Each schema's name (its file name without `.schema.json`) and its text.
const SCHEMAS: [(&str, &str); 12] = [
    ("error.output", schema!("error.output.schema.json")),
    (
        "lease-create.input",
        schema!("lease-create.input.schema.json"),
    ),
    ("lease-get.input", schema!("lease-get.input.schema.json")),
    ("lease-list.input", schema!("lease-list.input.schema.json")),
    (
        "lease-list.output",
        schema!("lease-list.output.schema.json"),
    ),
    (
        "lease-release.input",
        schema!("lease-release.input.schema.json"),
    ),
    (
        "lease-renew.input",
        schema!("lease-renew.input.schema.json"),
    ),
    ("lease.output", schema!("lease.output.schema.json")),
    (
        "board-holder-launch-result.state",
        schema!("state/board-holder-launch-result.state.schema.json"),
    ),
    (
        "board-holder-launch.state",
        schema!("state/board-holder-launch.state.schema.json"),
    ),
    (
        "board-holder.state",
        schema!("state/board-holder.state.schema.json"),
    ),
    ("leases.state", schema!("state/leases.state.schema.json")),
];

/// A schema's validator, or `None` if the schema could not be built, which would be a defect.
type Built = Option<Validator>;

fn build(text: &str) -> Built {
    let schema: Value = serde_json::from_str(text).ok()?;
    jsonschema::draft202012::new(&schema).ok()
}

fn validators() -> &'static [(&'static str, Built)] {
    static VALIDATORS: OnceLock<Vec<(&'static str, Built)>> = OnceLock::new();
    VALIDATORS.get_or_init(|| SCHEMAS.iter().map(|&(n, t)| (n, build(t))).collect())
}

/// Checks a document against the named schema. `None` if there is no such schema. A schema that
/// could not be built refuses every document.
pub fn validate(name: &str, doc: &Value) -> Option<bool> {
    let (_, v) = validators().iter().find(|(n, _)| *n == name)?;
    Some(v.as_ref().is_some_and(|v| v.is_valid(doc)))
}

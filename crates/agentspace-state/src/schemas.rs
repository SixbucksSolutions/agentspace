//! The fourteen schemas, embedded at build time, and the validator that checks against them.
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
const SCHEMAS: [(&str, &str); 14] = [
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
    // The host registry (D45) and the project list (D54), for agentspace-hosts.
    ("hosts.state", schema!("state/hosts.state.schema.json")),
    (
        "projects.state",
        schema!("state/projects.state.schema.json"),
    ),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A schema that failed to build would refuse every document, so each of the fourteen MUST
    /// build, the three board-holder schemas included.
    #[test]
    fn every_embedded_schema_builds_with_the_validator() {
        assert_eq!(SCHEMAS.len(), 14);
        for (name, text) in SCHEMAS {
            assert!(build(text).is_some(), "Schema {name} must build");
        }
        for (name, built) in validators() {
            assert!(built.is_some(), "Schema {name} must build");
        }
        for name in [
            "board-holder.state",
            "board-holder-launch.state",
            "board-holder-launch-result.state",
        ] {
            assert!(SCHEMAS.iter().any(|(n, _)| *n == name), "{name}");
        }
    }

    #[test]
    fn the_fourteen_names_are_distinct() {
        let mut names: Vec<&str> = SCHEMAS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 14);
    }

    /// The two v0.07 schemas accept their samples and refuse their invalid ones (D45, D54).
    #[test]
    fn the_host_registry_and_project_list_schemas_check_their_samples() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/state/samples");
        let mut checked = 0;
        for (sub, valid) in [("", true), ("invalid", false)] {
            for entry in std::fs::read_dir(dir.join(sub)).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let schema = if name.starts_with("hosts.state.") {
                    "hosts.state"
                } else if name.starts_with("projects.state.") {
                    "projects.state"
                } else {
                    continue;
                };
                let doc: Value =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                assert_eq!(validate(schema, &doc), Some(valid), "{name}");
                checked += 1;
            }
        }
        assert!(checked >= 20, "Only {checked} samples were found");
    }
}

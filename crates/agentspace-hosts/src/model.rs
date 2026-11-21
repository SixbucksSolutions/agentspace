//! The host registry's records, their names and their bytes (docs/state.md, The host registry).

use crate::error::Invalid;
use crate::json;
use agentspace_state::{parse_time, validate_schema};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// The project whose host is the box (D43, D55).
pub const BOX_PROJECT: &str = "swimcoach";
/// The box's CloudFormation stack (D44).
pub const BOX_STACK: &str = "agentbox";

/// The prefix of every other host's stack (D44).
const STACK_PREFIX: &str = "agentspace-host-";

/// The schema's name in agentspace-state.
const SCHEMA: &str = "hosts.state";

/// A host record's `state` (docs/state.md, Its states).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostState {
    /// A run is under way, or one died with the record still `creating`.
    Creating,
    /// The host is made and set up, and takes leases.
    Ready,
    /// A run's step failed on a host that was never `ready`.
    Failed,
}

impl HostState {
    /// The state as the file spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            HostState::Creating => "creating",
            HostState::Ready => "ready",
            HostState::Failed => "failed",
        }
    }

    fn from_text(s: &str) -> Option<HostState> {
        match s {
            "creating" => Some(HostState::Creating),
            "ready" => Some(HostState::Ready),
            "failed" => Some(HostState::Failed),
            _ => None,
        }
    }
}

/// One record of the host registry. The strings are kept as stored: `last_attempt_at` stays the
/// string `YYYY-MM-DDTHH:MM:SSZ` it is stored as (docs/state.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// The project the host serves, in the lease contract's normalized form.
    pub project: String,
    /// The host's CloudFormation stack: `agentbox` or `agentspace-host-<project>`.
    pub stack: String,
    pub state: HostState,
    /// The host's EC2 instance ID, cached from its stack's `InstanceId` output.
    pub instance_id: Option<String>,
    /// The start time of the host's last provisioning run, UTC.
    pub last_attempt_at: Option<String>,
}

/// The host registry: one record per host, in the order they were added. Its `version` is always 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hosts {
    pub hosts: Vec<Host>,
}

impl Hosts {
    /// The seed: the box's record alone (D45, D55), which a missing file reads as.
    pub fn seed() -> Hosts {
        Hosts {
            hosts: vec![Host {
                project: BOX_PROJECT.to_string(),
                stack: BOX_STACK.to_string(),
                state: HostState::Ready,
                instance_id: None,
                last_attempt_at: None,
            }],
        }
    }
}

/// Where a lease's connection goes (docs/state.md, Which host a connection reaches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// Today's box.
    Box,
    /// The `ready` host of this project, which is not the box.
    Host(&'a str),
}

/// The stack a writer gives the record it adds for `project`, `agentspace-host-<project>` (D44):
/// the plain formula for every project, as `host_alias` is. The box's stack is `BOX_STACK`, which
/// only the seed carries, and a caller that wants an existing host's stack reads its record's
/// `stack` member.
// Architect's to confirm: stack_name(BOX_PROJECT) is `agentspace-host-swimcoach`, not `agentbox`;
// state.md says only the seed ever has the stack `agentbox`, and a record added for swimcoach would
// be a second record of one project, so no writer asks.
pub fn stack_name(project: &str) -> String {
    format!("{STACK_PREFIX}{project}")
}

/// A host's operator alias, `agentspace-host.<project>` (D47, D55).
pub fn host_alias(project: &str) -> String {
    format!("agentspace-host.{project}")
}

/// Whether the record is the box's: the record whose stack is `agentbox` (D55).
pub fn is_box(host: &Host) -> bool {
    host.stack == BOX_STACK
}

/// The record of `project`, whatever its state. Projects are compared as the normalized strings
/// they are stored as.
pub fn host_for<'a>(hosts: &'a Hosts, project: &str) -> Option<&'a Host> {
    hosts.hosts.iter().find(|h| h.project == project)
}

/// Where a connection for `project` goes (D55): the project's host when its record is `ready` and is
/// not the box, and the box otherwise, that is when the project has no record, when its record is
/// not `ready`, and when its record is the box's.
pub fn target_for<'a>(hosts: &'a Hosts, project: &str) -> Target<'a> {
    match host_for(hosts, project) {
        Some(host) if host.state == HostState::Ready && !is_box(host) => {
            Target::Host(&host.project)
        }
        _ => Target::Box,
    }
}

/// Whether `s` is in the lease contract's normalized project form,
/// `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`.
pub fn is_normalized_project(s: &str) -> bool {
    let b = s.as_bytes();
    let plain = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    match b {
        [] => false,
        [only] => plain(*only),
        [first, middle @ .., last] => {
            middle.len() <= 30
                && plain(*first)
                && plain(*last)
                && middle.iter().all(|&c| plain(c) || c == b'-')
        }
    }
}

fn text(o: &Map<String, Value>, key: &str) -> Result<Option<String>, Invalid> {
    match o.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(Invalid::Schema),
    }
}

fn record_from(v: &Value) -> Result<Host, Invalid> {
    let o = v.as_object().ok_or(Invalid::Schema)?;
    Ok(Host {
        project: text(o, "project")?.ok_or(Invalid::Schema)?,
        stack: text(o, "stack")?.ok_or(Invalid::Schema)?,
        state: text(o, "state")?
            .as_deref()
            .and_then(HostState::from_text)
            .ok_or(Invalid::Schema)?,
        instance_id: text(o, "instance_id")?,
        last_attempt_at: text(o, "last_attempt_at")?,
    })
}

/// The rules the schema cannot check (docs/state.md, "What makes a file not valid, beyond its
/// schema"), and the times that name no real instant.
fn check_rules(hosts: &[Host]) -> Result<(), Invalid> {
    let mut seen = HashSet::new();
    if !hosts.iter().all(|h| seen.insert(h.project.as_str())) {
        return Err(Invalid::RepeatedProject);
    }
    for h in hosts {
        if let Some(p) = h.stack.strip_prefix(STACK_PREFIX)
            && p != h.project
        {
            return Err(Invalid::StackOfAnotherProject);
        }
    }
    let mut boxes = hosts.iter().filter(|h| h.stack == BOX_STACK);
    match (boxes.next(), boxes.next()) {
        (Some(b), None) if b.project == BOX_PROJECT => {}
        _ => return Err(Invalid::BoxRecord),
    }
    if hosts.iter().any(|h| {
        h.last_attempt_at
            .as_deref()
            .is_some_and(|t| parse_time(t).is_none())
    }) {
        return Err(Invalid::TimeNamesNoInstant);
    }
    Ok(())
}

/// Reads the registry's bytes (docs/state.md, The host registry): a leading byte order mark and any
/// JSON whitespace are accepted; bytes that are not UTF-8, a document that fails its schema (a
/// `version` other than 0 and a `null` member among them), an object naming a member twice, two
/// records of one project, a stack of another project's name, a missing, doubled or misplaced box
/// record, and a time that names no real instant each make it not valid.
pub fn parse(bytes: &[u8]) -> Result<Hosts, Invalid> {
    let doc = json::parse(bytes)?;
    if validate_schema(SCHEMA, &doc) != Some(true) {
        return Err(Invalid::Schema);
    }
    let items = doc
        .get("hosts")
        .and_then(Value::as_array)
        .ok_or(Invalid::Schema)?;
    let hosts = items
        .iter()
        .map(record_from)
        .collect::<Result<Vec<_>, _>>()?;
    check_rules(&hosts)?;
    Ok(Hosts { hosts })
}

/// Writes the registry's bytes: UTF-8 without a byte order mark, compact JSON with each record's
/// members in the order of state.md's table, then one line feed. The result is read back with
/// `parse`, so a change that would not be valid is refused and nothing is produced.
pub fn serialize(hosts: &Hosts) -> Result<Vec<u8>, Invalid> {
    let mut out = String::from("{\"version\":0,\"hosts\":[");
    for (i, h) in hosts.hosts.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"project\":");
        out.push_str(&json::quote(&h.project));
        out.push_str(",\"stack\":");
        out.push_str(&json::quote(&h.stack));
        out.push_str(",\"state\":");
        out.push_str(&json::quote(h.state.as_str()));
        if let Some(id) = &h.instance_id {
            out.push_str(",\"instance_id\":");
            out.push_str(&json::quote(id));
        }
        if let Some(t) = &h.last_attempt_at {
            out.push_str(",\"last_attempt_at\":");
            out.push_str(&json::quote(t));
        }
        out.push('}');
    }
    out.push_str("]}\n");
    parse(out.as_bytes())?;
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(project: &str, stack: &str, state: HostState) -> Host {
        Host {
            project: project.to_string(),
            stack: stack.to_string(),
            state,
            instance_id: None,
            last_attempt_at: None,
        }
    }

    fn two() -> Hosts {
        let mut hosts = Hosts::seed();
        hosts.hosts.push(Host {
            instance_id: Some("i-1234567890abcdef0".to_string()),
            last_attempt_at: Some("2026-11-20T19:05:42Z".to_string()),
            ..host("agentspace", "agentspace-host-agentspace", HostState::Ready)
        });
        hosts
    }

    #[test]
    fn the_names_are_the_pinned_forms() {
        assert_eq!(BOX_PROJECT, "swimcoach");
        assert_eq!(BOX_STACK, "agentbox");
        assert_eq!(stack_name("agentspace"), "agentspace-host-agentspace");
        assert_eq!(host_alias("agentspace"), "agentspace-host.agentspace");
        let seed = Hosts::seed();
        assert!(is_box(&seed.hosts[0]));
        assert!(!is_box(&two().hosts[1]));
    }

    #[test]
    fn the_seed_serializes_to_the_documents_bytes_and_reads_back() {
        let bytes = serialize(&Hosts::seed()).unwrap();
        assert_eq!(
            bytes,
            b"{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n"
        );
        assert_eq!(parse(&bytes).unwrap(), Hosts::seed());
    }

    #[test]
    fn a_full_registry_round_trips_with_members_in_the_tables_order() {
        let bytes = serialize(&two()).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains(
            "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-1234567890abcdef0\",\"last_attempt_at\":\"2026-11-20T19:05:42Z\"}"
        ));
        assert!(text.ends_with("]}\n") && !text.ends_with("\n\n"));
        assert_eq!(parse(&bytes).unwrap(), two());
    }

    #[test]
    fn version_written_as_a_float_is_accepted_and_member_order_does_not_matter() {
        let text = br#"{"hosts":[{"state":"ready","stack":"agentbox","project":"swimcoach"}],"version":0.0}"#;
        assert_eq!(parse(text).unwrap(), Hosts::seed());
    }

    fn refused(text: &str) -> Invalid {
        parse(text.as_bytes()).unwrap_err()
    }

    const BOX: &str = r#"{"project":"swimcoach","stack":"agentbox","state":"ready"}"#;

    fn doc(records: &[&str]) -> String {
        format!("{{\"version\":0,\"hosts\":[{}]}}", records.join(","))
    }

    #[test]
    fn each_rule_beyond_the_schema_is_refused() {
        // Rule 1: two records share a project.
        let a = r#"{"project":"agentspace","stack":"agentspace-host-agentspace","state":"ready"}"#;
        assert_eq!(refused(&doc(&[BOX, a, a])), Invalid::RepeatedProject);
        // Rule 2: a stack of another project.
        let b = r#"{"project":"agentspace","stack":"agentspace-host-swimcoach","state":"ready"}"#;
        assert_eq!(refused(&doc(&[BOX, b])), Invalid::StackOfAnotherProject);
        // Rule 3: no box record, two, and a box of another project.
        assert_eq!(refused(&doc(&[a])), Invalid::BoxRecord);
        assert_eq!(refused(&doc(&[])), Invalid::BoxRecord);
        let box2 = r#"{"project":"other","stack":"agentbox","state":"ready"}"#;
        assert_eq!(refused(&doc(&[BOX, box2])), Invalid::BoxRecord);
        assert_eq!(refused(&doc(&[box2])), Invalid::BoxRecord);
        // A time that matches its pattern and names no instant. A leap second, `:60`, does not
        // match the pattern, so the schema refuses it.
        for t in ["2026-02-30T00:00:00Z", "2026-04-31T00:00:00Z"] {
            let r = format!(
                r#"{{"project":"agentspace","stack":"agentspace-host-agentspace","state":"creating","last_attempt_at":"{t}"}}"#
            );
            assert_eq!(
                refused(&doc(&[BOX, &r])),
                Invalid::TimeNamesNoInstant,
                "{t}"
            );
        }
    }

    #[test]
    fn the_schema_refuses_what_it_refuses() {
        for text in [
            r#"{"version":1,"hosts":[]}"#,
            r#"[{"project":"swimcoach"}]"#,
            r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","instance_id":null}]}"#,
            r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","region":"x"}]}"#,
            r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","instance_id":"i-0123"}]}"#,
            r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"stopped"}]}"#,
            r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","last_attempt_at":"2026-11-20T11:30:00-08:00"}]}"#,
            r#"{"version":0,"hosts":[{"project":"Swimcoach","stack":"agentbox","state":"ready"}]}"#,
        ] {
            assert_eq!(refused(text), Invalid::Schema, "{text}");
        }
    }

    #[test]
    fn a_leap_second_is_refused_by_the_schema() {
        let r = r#"{"project":"agentspace","stack":"agentspace-host-agentspace","state":"creating","last_attempt_at":"2026-11-20T23:59:60Z"}"#;
        assert_eq!(refused(&doc(&[BOX, r])), Invalid::Schema);
    }

    #[test]
    fn an_object_naming_a_member_twice_is_refused() {
        let text = r#"{"version":0,"hosts":[{"project":"swimcoach","project":"swimcoach","stack":"agentbox","state":"ready"}]}"#;
        assert_eq!(refused(text), Invalid::RepeatedMember);
    }

    #[test]
    fn a_change_that_would_not_be_valid_is_not_serialized() {
        let mut hosts = two();
        hosts.hosts[1].last_attempt_at = Some("2026-02-30T00:00:00Z".to_string());
        assert_eq!(serialize(&hosts).unwrap_err(), Invalid::TimeNamesNoInstant);
        let mut hosts = two();
        hosts.hosts.push(hosts.hosts[1].clone());
        assert_eq!(serialize(&hosts).unwrap_err(), Invalid::RepeatedProject);
        let mut hosts = two();
        hosts.hosts[1].project = "Bad Name".to_string();
        assert!(serialize(&hosts).is_err());
        let mut hosts = two();
        hosts.hosts.remove(0);
        assert_eq!(serialize(&hosts).unwrap_err(), Invalid::BoxRecord);
    }

    #[test]
    fn target_for_sends_a_project_without_a_ready_host_to_the_box() {
        let mut hosts = two();
        hosts.hosts.push(host(
            "creating-one",
            "agentspace-host-creating-one",
            HostState::Creating,
        ));
        hosts.hosts.push(host(
            "failed-one",
            "agentspace-host-failed-one",
            HostState::Failed,
        ));
        assert_eq!(target_for(&hosts, "agentspace"), Target::Host("agentspace"));
        assert_eq!(target_for(&hosts, "swimcoach"), Target::Box);
        assert_eq!(target_for(&hosts, "creating-one"), Target::Box);
        assert_eq!(target_for(&hosts, "failed-one"), Target::Box);
        assert_eq!(target_for(&hosts, "no-record"), Target::Box);
        assert_eq!(
            host_for(&hosts, "failed-one").unwrap().state,
            HostState::Failed
        );
        assert!(host_for(&hosts, "no-record").is_none());
    }

    /// The schemas' own samples (schemas/state/samples) read as the crate reads them: every valid
    /// one parses and serializes back to a document that parses to the same records, and every
    /// invalid one is refused.
    #[test]
    fn the_schemas_samples_are_read_as_the_schemas_say() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/state/samples");
        let mut seen = 0;
        for (sub, valid) in [("", true), ("invalid", false)] {
            for entry in std::fs::read_dir(dir.join(sub)).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                if !name.starts_with("hosts.state.") {
                    continue;
                }
                let bytes = std::fs::read(&path).unwrap();
                match (valid, parse(&bytes)) {
                    (true, Ok(hosts)) => {
                        assert_eq!(parse(&serialize(&hosts).unwrap()).unwrap(), hosts, "{name}")
                    }
                    (false, Err(_)) => {}
                    (valid, got) => panic!("Sample {name}: valid is {valid}, read as {got:?}"),
                }
                seen += 1;
            }
        }
        assert!(seen >= 14, "Only {seen} samples were found");
    }

    #[test]
    fn the_normalized_form_is_the_contracts() {
        for ok in ["a", "0", "ab", "a-b", "swimcoach", &"a".repeat(32)] {
            assert!(is_normalized_project(ok), "{ok}");
        }
        for bad in [
            "",
            "-",
            "a-",
            "-a",
            "A",
            "a_b",
            "a b",
            "a\n",
            "..",
            "a/b",
            &"a".repeat(33),
        ] {
            assert!(!is_normalized_project(bad), "{bad:?}");
        }
    }
}

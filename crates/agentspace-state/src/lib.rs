//! agentspace-state (docs/decisions.md, D38).
//!
//! The lease state file: its model, schema checks and bytes, and its place on the disk: the
//! folders (docs/state.md, The home folder), the lease state lock and the atomic replace; and live
//! leases, a project's last lease end and its horizon (docs/state.md, Live leases and the lease
//! horizon; D56, D63). RFC 2119 keywords in this crate's comments, and the capitals are
//! load-bearing.
//!
//! A library crate: it never writes to stdout or stderr. The Windows half is behind `cfg(windows)`;
//! on any other system the same interface builds with std's nearest equivalents, which are not the
//! contract's (state.md, D8).

#![deny(unsafe_code)]

mod atomic;
mod live;
mod paths;
mod schemas;
mod store;
mod sys;
mod time;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fmt;

pub use atomic::write_atomically;
pub use live::{is_live, last_lease_end, lease_end, lease_horizon, live_count};
pub use paths::{Paths, resolve_paths};
pub use store::{LockGuard, StoreError, lock, read, write};
pub use time::{format_time, now, parse_time};

/// One lease's `ssh` object.
#[derive(Debug, Clone, PartialEq)]
pub struct Ssh {
    pub host_alias: String,
    pub user: String,
}

/// One reason an unsatisfiable lease cannot be met.
#[derive(Debug, Clone, PartialEq)]
pub struct Reason {
    pub code: String,
    pub message: String,
}

/// One record of the lease state file, with the state as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub id: String,
    pub project: String,
    pub state: String,
    pub platform: String,
    pub profile: String,
    pub size: String,
    pub ttl_seconds: i64,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub released_at: Option<String>,
    pub ssh: Option<Ssh>,
    pub reasons: Option<Vec<Reason>>,
    pub driver: Option<String>,
    /// What a lease created with `--plan` was sized for (docs/state.md, What a plan lease adds;
    /// D75, D81). Stored after `size`, and printed there in the lease document.
    pub sized_for: Option<SizedFor>,
    /// The plan document a lease was created with, as parsed (docs/state.md, What a plan lease
    /// adds; D74, D81). Stored last, after `driver`, and never in any output (pin P25).
    pub plan: Option<serde_json::Value>,
}

/// A lease record's `sized_for`: the plan's name when it has one, its busiest turn, and that
/// turn's agents at once, its dev lead counted (docs/lease-api.md, Fields; D75, D81).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizedFor {
    pub plan: Option<String>,
    pub turn: String,
    pub agents: u32,
}

/// The lease state file. Its `version` is always 0.
#[derive(Debug, Clone, PartialEq)]
pub struct StateFile {
    pub leases: Vec<Record>,
}

/// Why a file is not valid or cannot be written: the `<why>` of lease-api.md's "Records
/// unreadable" or "Records unwritable" message.
#[derive(Debug, Clone, PartialEq)]
pub struct NotValid(pub String);

/// Which output document is being checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Lease,
    LeaseList,
    Error,
}

/// The `<why>` of a file that does not validate (lease-api.md, "Records unreadable").
const WHY_FILE: &str = "the state file is not valid";
/// The `<why>` of a change that would not validate (lease-api.md, "Records unwritable").
const WHY_CHANGE: &str = "the change is not valid";
/// The `<why>` of a change that would repeat an ID (lease-api.md, "Records unwritable").
const WHY_REPEAT: &str = "two records share an ID";

fn not_valid(why: &str) -> NotValid {
    NotValid(why.to_string())
}

fn text(o: &Map<String, Value>, key: &str) -> Option<String> {
    o.get(key)?.as_str().map(str::to_string)
}

fn integer(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    // A schema's "integer" also accepts 3600.0.
    let f = v.as_f64()?;
    (f.fract() == 0.0 && f.abs() < 9.0e15).then_some(f as i64)
}

/// A stored `sized_for` (state.md, What a plan lease adds). `plan` is absent when the plan had no
/// name; a `null` there, which the schema refuses, is read as nothing here rather than accepted.
fn sized_for_from(v: &Value) -> Option<SizedFor> {
    let o = v.as_object()?;
    let plan = match o.get("plan") {
        None => None,
        Some(p) => Some(p.as_str()?.to_string()),
    };
    Some(SizedFor {
        plan,
        turn: text(o, "turn")?,
        agents: u32::try_from(integer(o.get("agents")?)?).ok()?,
    })
}

/// `sized_for` as the lease document prints it: `plan`, `turn` and `agents`, in lease-api.md's
/// order (Fields), with `plan` left out when the plan had no name, never written as `null`.
fn sized_for_value(s: &SizedFor) -> Value {
    let mut m = Map::new();
    if let Some(p) = &s.plan {
        m.insert("plan".into(), p.clone().into());
    }
    m.insert("turn".into(), s.turn.clone().into());
    m.insert("agents".into(), s.agents.into());
    Value::Object(m)
}

fn record_from(v: &Value) -> Option<Record> {
    let o = v.as_object()?;
    let ssh = match o.get("ssh") {
        None => None,
        Some(s) => {
            let s = s.as_object()?;
            Some(Ssh {
                host_alias: text(s, "host_alias")?,
                user: text(s, "user")?,
            })
        }
    };
    let reasons = match o.get("reasons") {
        None => None,
        Some(list) => {
            let mut out = Vec::new();
            for r in list.as_array()? {
                let r = r.as_object()?;
                out.push(Reason {
                    code: text(r, "code")?,
                    message: text(r, "message")?,
                });
            }
            Some(out)
        }
    };
    let optional = |key: &str| match o.get(key) {
        None => Some(None),
        Some(v) => v.as_str().map(|s| Some(s.to_string())),
    };
    let sized_for = match o.get("sized_for") {
        None => None,
        Some(s) => Some(sized_for_from(s)?),
    };
    // The schema refuses a `null` plan, so a present member is always the plan itself.
    let plan = o.get("plan").cloned();
    Some(Record {
        sized_for,
        plan,
        id: text(o, "id")?,
        project: text(o, "project")?,
        state: text(o, "state")?,
        platform: text(o, "platform")?,
        profile: text(o, "profile")?,
        size: text(o, "size")?,
        ttl_seconds: integer(o.get("ttl_seconds")?)?,
        created_at: text(o, "created_at")?,
        expires_at: optional("expires_at")?,
        released_at: optional("released_at")?,
        ssh,
        reasons,
        driver: optional("driver")?,
    })
}

/// The members of a record that a lease document shares, as stored, in lease-api.md's order
/// (Fields): `sized_for` right after `size`. `driver` and `plan` are never among them (D1; pin P25).
fn base_members(r: &Record) -> Map<String, Value> {
    let mut o = Map::new();
    o.insert("id".into(), r.id.clone().into());
    o.insert("project".into(), r.project.clone().into());
    o.insert("state".into(), r.state.clone().into());
    o.insert("platform".into(), r.platform.clone().into());
    o.insert("profile".into(), r.profile.clone().into());
    o.insert("size".into(), r.size.clone().into());
    if let Some(s) = &r.sized_for {
        o.insert("sized_for".into(), sized_for_value(s));
    }
    o.insert("ttl_seconds".into(), r.ttl_seconds.into());
    o.insert("created_at".into(), r.created_at.clone().into());
    if let Some(t) = &r.expires_at {
        o.insert("expires_at".into(), t.clone().into());
    }
    if let Some(t) = &r.released_at {
        o.insert("released_at".into(), t.clone().into());
    }
    if let Some(s) = &r.ssh {
        let mut m = Map::new();
        m.insert("host_alias".into(), s.host_alias.clone().into());
        m.insert("user".into(), s.user.clone().into());
        o.insert("ssh".into(), Value::Object(m));
    }
    if let Some(list) = &r.reasons {
        let items = list
            .iter()
            .map(|x| {
                let mut m = Map::new();
                m.insert("code".into(), x.code.clone().into());
                m.insert("message".into(), x.message.clone().into());
                Value::Object(m)
            })
            .collect();
        o.insert("reasons".into(), Value::Array(items));
    }
    o
}

/// A record's members as a JSON object, `driver` and `plan` included, for the schema check.
fn record_value(r: &Record) -> Value {
    let mut o = base_members(r);
    if let Some(d) = &r.driver {
        o.insert("driver".into(), d.clone().into());
    }
    if let Some(p) = &r.plan {
        o.insert("plan".into(), p.clone());
    }
    Value::Object(o)
}

/// Every time of a record names a real instant (state.md, gap FA-10).
fn times_are_real(r: &Record) -> bool {
    parse_time(&r.created_at).is_some()
        && r.expires_at
            .as_deref()
            .is_none_or(|t| parse_time(t).is_some())
        && r.released_at
            .as_deref()
            .is_none_or(|t| parse_time(t).is_some())
}

fn ids_repeat(leases: &[Record]) -> bool {
    let mut seen = HashSet::new();
    !leases.iter().all(|r| seen.insert(r.id.as_str()))
}

/// A pass over a JSON text that fails on an object naming one member twice, at any depth. A JSON
/// parser keeps one of the two values silently, so this check MUST run beside it (state.md, The
/// lease state file). Member names are compared after their escapes are decoded. Nesting deeper
/// than serde_json's recursion limit is an error, not a stack overflow.
struct NoRepeatedMember;

impl<'de> DeserializeSeed<'de> for NoRepeatedMember {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for NoRepeatedMember {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(NoRepeatedMember)?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key) {
                return Err(de::Error::custom("a member is named twice"));
            }
            map.next_value_seed(NoRepeatedMember)?;
        }
        Ok(())
    }
}

/// Whether the text is one JSON value in which no object names a member twice.
fn has_no_repeated_member(text: &str) -> bool {
    let mut de = serde_json::Deserializer::from_str(text);
    NoRepeatedMember.deserialize(&mut de).is_ok() && de.end().is_ok()
}

/// Reads the state file's bytes (state.md, The lease state file). A leading UTF-8 byte order mark
/// and any JSON whitespace are accepted. Bytes that are not UTF-8, a document that fails its
/// schema (a `version` other than 0 included), an object that names one member twice, two records
/// with one `id`, and a time that names no real instant each make it not valid. Times stay the
/// strings they were.
pub fn parse(bytes: &[u8]) -> Result<StateFile, NotValid> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| not_valid(WHY_FILE))?;
    if !has_no_repeated_member(text) {
        return Err(not_valid(WHY_FILE));
    }
    let doc: Value = serde_json::from_str(text).map_err(|_| not_valid(WHY_FILE))?;
    if schemas::validate("leases.state", &doc) != Some(true) {
        return Err(not_valid(WHY_FILE));
    }
    let items = doc
        .get("leases")
        .and_then(Value::as_array)
        .ok_or_else(|| not_valid(WHY_FILE))?;
    let mut leases = Vec::with_capacity(items.len());
    for item in items {
        leases.push(record_from(item).ok_or_else(|| not_valid(WHY_FILE))?);
    }
    if !leases.iter().all(times_are_real) || ids_repeat(&leases) {
        return Err(not_valid(WHY_FILE));
    }
    Ok(StateFile { leases })
}

fn push_string(out: &mut String, s: &str) {
    match serde_json::to_string(s) {
        Ok(quoted) => out.push_str(&quoted),
        // Serializing a string cannot fail; an empty string is the safe fallback.
        Err(_) => out.push_str("\"\""),
    }
}

/// A value written compactly, its objects' members in the order they hold (`preserve_order`):
/// `sized_for`, and the plan as `create` parsed it (state.md, What a plan lease adds).
fn push_value(out: &mut String, v: &Value) {
    match serde_json::to_string(v) {
        Ok(text) => out.push_str(&text),
        // A Value has only string keys, so serializing it cannot fail; null is the safe fallback,
        // and the schema check that runs before any bytes are produced never accepts it here.
        Err(_) => out.push_str("null"),
    }
}

fn push_member(out: &mut String, first: &mut bool, key: &str, write: impl FnOnce(&mut String)) {
    if !*first {
        out.push(',');
    }
    *first = false;
    push_string(out, key);
    out.push(':');
    write(out);
}

fn push_record(out: &mut String, r: &Record) {
    out.push('{');
    let mut first = true;
    let strings = [
        ("id", &r.id),
        ("project", &r.project),
        ("state", &r.state),
        ("platform", &r.platform),
        ("profile", &r.profile),
        ("size", &r.size),
    ];
    for (k, v) in strings {
        push_member(out, &mut first, k, |o| push_string(o, v));
    }
    if let Some(s) = &r.sized_for {
        push_member(out, &mut first, "sized_for", |o| {
            push_value(o, &sized_for_value(s))
        });
    }
    push_member(out, &mut first, "ttl_seconds", |o| {
        o.push_str(&r.ttl_seconds.to_string())
    });
    push_member(out, &mut first, "created_at", |o| {
        push_string(o, &r.created_at)
    });
    if let Some(t) = &r.expires_at {
        push_member(out, &mut first, "expires_at", |o| push_string(o, t));
    }
    if let Some(t) = &r.released_at {
        push_member(out, &mut first, "released_at", |o| push_string(o, t));
    }
    if let Some(s) = &r.ssh {
        push_member(out, &mut first, "ssh", |o| {
            o.push_str("{\"host_alias\":");
            push_string(o, &s.host_alias);
            o.push_str(",\"user\":");
            push_string(o, &s.user);
            o.push('}');
        });
    }
    if let Some(list) = &r.reasons {
        push_member(out, &mut first, "reasons", |o| {
            o.push('[');
            for (i, x) in list.iter().enumerate() {
                if i > 0 {
                    o.push(',');
                }
                o.push_str("{\"code\":");
                push_string(o, &x.code);
                o.push_str(",\"message\":");
                push_string(o, &x.message);
                o.push('}');
            }
            o.push(']');
        });
    }
    if let Some(d) = &r.driver {
        push_member(out, &mut first, "driver", |o| push_string(o, d));
    }
    if let Some(p) = &r.plan {
        push_member(out, &mut first, "plan", |o| push_value(o, p));
    }
    out.push('}');
}

/// Writes the state file's bytes: UTF-8 without a byte order mark, compact JSON with the members
/// in state.md's order, then one line feed. A file that would not validate, that holds a time
/// naming no real instant, or that repeats an `id` is refused, and nothing is produced.
pub fn serialize(file: &StateFile) -> Result<Vec<u8>, NotValid> {
    if ids_repeat(&file.leases) {
        return Err(not_valid(WHY_REPEAT));
    }
    let mut doc = Map::new();
    doc.insert("version".into(), 0.into());
    doc.insert(
        "leases".into(),
        Value::Array(file.leases.iter().map(record_value).collect()),
    );
    if schemas::validate("leases.state", &Value::Object(doc)) != Some(true)
        || !file.leases.iter().all(times_are_real)
    {
        return Err(not_valid(WHY_CHANGE));
    }
    let mut out = String::from("{\"version\":0,\"leases\":[");
    for (i, r) in file.leases.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_record(&mut out, r);
    }
    out.push_str("]}\n");
    Ok(out.into_bytes())
}

/// Whether a stored `ready` record has expired as of `now`: at or after its `expires_at`, to the
/// second. A `ready` record whose `expires_at` names no instant is held to be expired, so that no
/// `ssh` is shown for it.
fn is_expired(r: &Record, now: i64) -> bool {
    r.state == "ready"
        && r.expires_at
            .as_deref()
            .is_some_and(|t| parse_time(t).is_none_or(|at| now >= at))
}

/// The state of a record as of `now`: `expired` is derived, never stored.
fn state_at(r: &Record, now: i64) -> &str {
    if is_expired(r, now) {
        "expired"
    } else {
        &r.state
    }
}

/// The lease document of a record as of `now`, in Unix seconds: `expired` derived from a stored
/// `ready` at or after its `expires_at`, `ssh` left out once expired, and `driver` never shown
/// (D1).
pub fn lease_document(r: &Record, now: i64) -> Value {
    let mut o = base_members(r);
    if is_expired(r, now) {
        o.insert("state".into(), "expired".into());
        // shift_remove keeps the other members in order; remove would swap the last one in.
        o.shift_remove("ssh");
    }
    Value::Object(o)
}

/// The list document: the leases that match every filter, as of `now`, oldest first by
/// `created_at` and then `id`, compared character by character. `project` matches exactly, and
/// `states` (empty for any) are matched against each lease's state as of `now`, one instant for
/// the whole list.
pub fn list_document(f: &StateFile, project: Option<&str>, states: &[&str], now: i64) -> Value {
    let mut kept: Vec<&Record> = f
        .leases
        .iter()
        .filter(|r| project.is_none_or(|p| r.project == p))
        .filter(|r| states.is_empty() || states.contains(&state_at(r, now)))
        .collect();
    kept.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    let leases = kept.into_iter().map(|r| lease_document(r, now)).collect();
    let mut o = Map::new();
    o.insert("leases".into(), Value::Array(leases));
    Value::Object(o)
}

/// Checks a document the CLI is about to print against its output schema (lease-api.md, Output).
pub fn validate_output(kind: OutputKind, doc: &Value) -> bool {
    let name = match kind {
        OutputKind::Lease => "lease.output",
        OutputKind::LeaseList => "lease-list.output",
        OutputKind::Error => "error.output",
    };
    schemas::validate(name, doc) == Some(true)
}

/// Validates a document against one of the fifteen embedded schemas, named by its file name
/// without `.schema.json`, such as `lease-create.input`, `plan.input` or `leases.state`: contract
/// v1's nine (lease-api.md, Schemas) and six state schemas. `None` for a name that is not one of
/// them, v0's kept schemas under `schemas/v0/` included, which are not embedded.
pub fn validate_schema(name: &str, doc: &Value) -> Option<bool> {
    schemas::validate(name, doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    const ID: &str = "3f1c2a7e-5b4d-4e8f-9a6b-0c1d2e3f4a5b";
    const CREATED: &str = "2026-11-24T18:00:00Z";
    const EXPIRES: &str = "2026-11-24T19:00:00Z";
    const RELEASED: &str = "2026-11-24T18:30:00Z";
    const TOO_LARGE: &str =
        "Plan's busiest turn 'T02' has 33 agents at once; the largest size, xl, holds 32.";

    fn at(t: &str) -> i64 {
        parse_time(t).unwrap()
    }

    /// A plan whose busiest turn, T01, has one story of peak 1 (b starts after a): 2 agents with
    /// its dev lead. Its members are in an order no sorting would give, and it keeps `null`s.
    fn plan_doc() -> Value {
        json!({"turns":[{"stories":[{"subagents":[
            {"name":"a","model":"Opus 5.5","effort":"high","after":null},
            {"name":"b","model":"Sonnet","effort":"medium","after":["a"]}],
            "name":"S01"}],"name":"T01"}],"name":null})
    }

    fn sized(plan: Option<&str>) -> SizedFor {
        SizedFor {
            plan: plan.map(str::to_string),
            turn: "T01".to_string(),
            agents: 2,
        }
    }

    /// A `pending` lease made without a plan, as v0.09 wrote it.
    fn plain() -> Record {
        Record {
            id: ID.to_string(),
            project: "agentspace".to_string(),
            state: "pending".to_string(),
            platform: "linux/arm64".to_string(),
            profile: "agent-linux".to_string(),
            size: "s".to_string(),
            ttl_seconds: 3600,
            created_at: CREATED.to_string(),
            expires_at: None,
            released_at: None,
            ssh: None,
            reasons: None,
            driver: Some("ec2".to_string()),
            sized_for: None,
            plan: None,
        }
    }

    /// The same lease made with a plan.
    fn planned() -> Record {
        Record {
            sized_for: Some(sized(Some("v0.10"))),
            plan: Some(plan_doc()),
            ..plain()
        }
    }

    fn ready(mut r: Record) -> Record {
        r.state = "ready".to_string();
        r.expires_at = Some(EXPIRES.to_string());
        r.ssh = Some(Ssh {
            host_alias: "agentspace-agentspace-3f1c2a7e".to_string(),
            user: "ubuntu".to_string(),
        });
        r
    }

    /// Released: from `pending` it has no `expires_at`, and from `ready` it keeps it.
    fn released(mut r: Record) -> Record {
        r.state = "released".to_string();
        r.released_at = Some(RELEASED.to_string());
        r.ssh = None;
        r
    }

    fn unsatisfiable(mut r: Record, code: &str, message: &str) -> Record {
        r.state = "unsatisfiable".to_string();
        r.reasons = Some(vec![Reason {
            code: code.to_string(),
            message: message.to_string(),
        }]);
        r
    }

    /// A `plan_too_large` lease: `unsatisfiable`, its one reason, size `xl`, both members.
    fn too_large() -> Record {
        let mut r = unsatisfiable(planned(), "plan_too_large", TOO_LARGE);
        r.size = "xl".to_string();
        r.sized_for = Some(SizedFor {
            plan: Some("v0.10".to_string()),
            turn: "T02".to_string(),
            agents: 33,
        });
        r
    }

    fn one(r: Record) -> StateFile {
        StateFile { leases: vec![r] }
    }

    fn text_of(f: &StateFile) -> String {
        String::from_utf8(serialize(f).unwrap()).unwrap()
    }

    fn keys(v: &Value) -> Vec<&str> {
        v.as_object().unwrap().keys().map(String::as_str).collect()
    }

    /// The lease state file's samples, valid or invalid, by name.
    fn state_samples(valid: bool) -> Vec<(String, Vec<u8>)> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/state/samples");
        if !valid {
            dir = dir.join("invalid");
        }
        let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_file())
            .filter_map(|p| {
                let name = p.file_name()?.to_string_lossy().into_owned();
                name.starts_with("leases.state.")
                    .then(|| (name, std::fs::read(&p).unwrap()))
            })
            .collect();
        out.sort();
        out
    }

    // Writing (state.md, The lease state file: how each command changes it, and its bytes).

    #[test]
    fn a_record_without_a_plan_is_written_as_v0_09_wrote_it_byte_for_byte() {
        assert_eq!(
            text_of(&one(plain())),
            format!(
                "{{\"version\":0,\"leases\":[{{\"id\":\"{ID}\",\"project\":\"agentspace\",\
                 \"state\":\"pending\",\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\
                 \"size\":\"s\",\"ttl_seconds\":3600,\"created_at\":\"{CREATED}\",\
                 \"driver\":\"ec2\"}}]}}\n"
            )
        );
    }

    #[test]
    fn sized_for_is_written_after_size_and_the_plan_last_after_driver() {
        let expected = format!(
            "{{\"version\":0,\"leases\":[{{\"id\":\"{ID}\",\"project\":\"agentspace\",\
             \"state\":\"ready\",\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\
             \"size\":\"s\",\"sized_for\":{{\"plan\":\"v0.10\",\"turn\":\"T01\",\"agents\":2}},\
             \"ttl_seconds\":3600,\"created_at\":\"{CREATED}\",\"expires_at\":\"{EXPIRES}\",\
             \"ssh\":{{\"host_alias\":\"agentspace-agentspace-3f1c2a7e\",\"user\":\"ubuntu\"}},\
             \"driver\":\"ec2\",\"plan\":{}}}]}}\n",
            serde_json::to_string(&plan_doc()).unwrap()
        );
        assert_eq!(text_of(&one(ready(planned()))), expected);
    }

    /// The plan is stored compactly, in the parsed document's member order, with its `null`s
    /// kept as parsed (state.md, What a plan lease adds).
    #[test]
    fn the_plan_is_stored_compactly_in_its_own_member_order_with_its_nulls() {
        let text = text_of(&one(planned()));
        let plan = text.split(",\"plan\":").nth(1).unwrap();
        assert_eq!(
            plan,
            "{\"turns\":[{\"stories\":[{\"subagents\":[{\"name\":\"a\",\"model\":\"Opus 5.5\",\
             \"effort\":\"high\",\"after\":null},{\"name\":\"b\",\"model\":\"Sonnet\",\
             \"effort\":\"medium\",\"after\":[\"a\"]}],\"name\":\"S01\"}],\"name\":\"T01\"}],\
             \"name\":null}}]}\n"
        );
    }

    #[test]
    fn sized_for_without_a_plan_name_has_no_plan_member_never_null() {
        let mut r = planned();
        r.sized_for = Some(sized(None));
        let text = text_of(&one(r.clone()));
        assert!(
            text.contains(",\"sized_for\":{\"turn\":\"T01\",\"agents\":2},"),
            "{text}"
        );
        let doc = lease_document(&r, at(CREATED));
        assert_eq!(doc["sized_for"], json!({"turn": "T01", "agents": 2}));
        assert_eq!(keys(&doc["sized_for"]), ["turn", "agents"]);
        assert!(validate_output(OutputKind::Lease, &doc));
    }

    #[test]
    fn every_state_round_trips_with_both_members() {
        for r in [
            planned(),
            ready(planned()),
            released(planned()),
            released(ready(planned())),
            unsatisfiable(planned(), "no_capacity", "No capacity."),
            too_large(),
        ] {
            let f = one(r);
            let bytes = serialize(&f).unwrap();
            assert_eq!(parse(&bytes).unwrap(), f, "{}", text_of(&f));
            assert_eq!(serialize(&parse(&bytes).unwrap()).unwrap(), bytes);
        }
    }

    #[test]
    fn a_plan_too_large_record_is_written_and_read() {
        let f = one(too_large());
        let back = parse(&serialize(&f).unwrap()).unwrap();
        let r = &back.leases[0];
        assert_eq!(r.state, "unsatisfiable");
        assert_eq!(r.size, "xl");
        assert_eq!(r.reasons.as_ref().unwrap().len(), 1);
        assert_eq!(r.reasons.as_ref().unwrap()[0].code, "plan_too_large");
        assert_eq!(r.sized_for.as_ref().unwrap().agents, 33);
        assert_eq!(r.plan, Some(plan_doc()));
    }

    /// The schema's rules, applied before anything is written (state.md, What a plan lease adds).
    #[test]
    fn a_change_that_breaks_the_plan_rules_is_not_written() {
        let change = |r: Record| serialize(&one(r)).unwrap_err();
        let not_valid = NotValid(WHY_CHANGE.to_string());
        // The two come together, with driver.
        let no_plan = Record {
            plan: None,
            ..planned()
        };
        let no_sized_for = Record {
            sized_for: None,
            ..planned()
        };
        let no_driver = Record {
            driver: None,
            ..planned()
        };
        assert_eq!(change(no_plan), not_valid);
        assert_eq!(change(no_sized_for), not_valid);
        assert_eq!(change(no_driver), not_valid);
        // A plan_too_large lease is unsatisfiable, its reason alone, xl, with sized_for.
        let mut beside = too_large();
        beside.reasons.as_mut().unwrap().push(Reason {
            code: "no_capacity".to_string(),
            message: "No capacity.".to_string(),
        });
        assert_eq!(change(beside), not_valid);
        let mut twice = too_large();
        let reason = twice.reasons.as_ref().unwrap()[0].clone();
        twice.reasons.as_mut().unwrap().push(reason);
        assert_eq!(change(twice), not_valid);
        let size_m = Record {
            size: "m".to_string(),
            ..too_large()
        };
        assert_eq!(change(size_m), not_valid);
        let without = Record {
            sized_for: None,
            plan: None,
            ..too_large()
        };
        assert_eq!(change(without), not_valid);
        let mut pending = too_large();
        pending.state = "pending".to_string();
        assert_eq!(change(pending), not_valid);
        // A sized_for or plan out of its form.
        let mut agents = planned();
        agents.sized_for.as_mut().unwrap().agents = 1;
        assert_eq!(change(agents), not_valid);
        let mut many = planned();
        many.sized_for.as_mut().unwrap().agents = 1026;
        assert_eq!(change(many), not_valid);
        let mut turn = planned();
        turn.sized_for.as_mut().unwrap().turn = "T01 ".to_string();
        assert_eq!(change(turn), not_valid);
        let no_turns = Record {
            plan: Some(json!({"turns": []})),
            ..planned()
        };
        assert_eq!(change(no_turns), not_valid);
        let null_plan = Record {
            plan: Some(Value::Null),
            ..planned()
        };
        assert_eq!(change(null_plan), not_valid);
    }

    // Reading.

    #[test]
    fn every_lease_state_sample_reads_and_writes_back_to_what_it_read() {
        let samples = state_samples(true);
        assert!(samples.len() >= 5, "{}", samples.len());
        let mut plan_records = 0;
        let mut one_line = 0;
        for (name, bytes) in samples {
            let f = parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            plan_records += f.leases.iter().filter(|r| r.plan.is_some()).count();
            let again = serialize(&f).unwrap();
            assert_eq!(parse(&again).unwrap(), f, "{name}");
            // A sample written on one line is in the writer's own form, byte for byte.
            if bytes.iter().filter(|b| **b == b'\n').count() == 1 {
                assert_eq!(
                    String::from_utf8_lossy(&again),
                    String::from_utf8_lossy(&bytes),
                    "{name}"
                );
                one_line += 1;
            }
        }
        assert!(plan_records >= 3, "{plan_records}");
        assert!(one_line >= 2, "{one_line}");
    }

    #[test]
    fn every_invalid_lease_state_sample_is_not_valid() {
        let samples = state_samples(false);
        assert!(samples.len() >= 20, "{}", samples.len());
        for (name, bytes) in samples {
            assert_eq!(parse(&bytes), Err(NotValid(WHY_FILE.to_string())), "{name}");
        }
    }

    #[test]
    fn a_stored_sized_for_and_plan_are_read_into_the_record() {
        let text = text_of(&one(planned()));
        let r = parse(text.as_bytes()).unwrap().leases.remove(0);
        assert_eq!(r.sized_for, Some(sized(Some("v0.10"))));
        assert_eq!(r.plan, Some(plan_doc()));
        // Member order inside the plan is kept as read.
        assert_eq!(keys(r.plan.as_ref().unwrap()), ["turns", "name"]);
        // And a record without them reads as without them.
        let text = text_of(&one(plain()));
        let r = parse(text.as_bytes()).unwrap().leases.remove(0);
        assert_eq!(r.sized_for, None);
        assert_eq!(r.plan, None);
    }

    #[test]
    fn agents_written_with_a_fraction_of_zero_is_read_and_written_back_whole() {
        let text = text_of(&one(planned())).replace("\"agents\":2", "\"agents\":2.0");
        let f = parse(text.as_bytes()).unwrap();
        assert_eq!(f.leases[0].sized_for.as_ref().unwrap().agents, 2);
        assert!(text_of(&f).contains("\"agents\":2}"));
    }

    /// Bad input is refused as not valid, never a panic.
    #[test]
    fn a_malformed_plan_lease_file_is_not_valid_and_never_panics() {
        let good = text_of(&one(planned()));
        for (from, to) in [
            ("\"agents\":2", "\"agents\":-1"),
            ("\"agents\":2", "\"agents\":99999999999999999999"),
            ("\"agents\":2", "\"agents\":1e400"),
            ("\"agents\":2", "\"agents\":\"2\""),
            ("\"agents\":2", "\"agents\":2.5"),
            ("{\"plan\":\"v0.10\",", "{\"plan\":null,"),
            ("{\"plan\":\"v0.10\",", "{\"plan\":7,"),
            ("\"sized_for\":{", "\"sized_for\":[{"),
            (",\"plan\":{\"turns\"", ",\"plan\":{\"turns\":1,\"turns\""),
            (",\"plan\":{", ",\"plan\":{\"extra\":true,"),
            (",\"plan\":{", ",\"plan\":[{"),
        ] {
            let bad = good.replacen(from, to, 1);
            assert_ne!(bad, good, "{from}");
            assert_eq!(
                parse(bad.as_bytes()),
                Err(NotValid(WHY_FILE.to_string())),
                "{to}"
            );
        }
    }

    // The lease document (lease-api.md, Fields).

    #[test]
    fn sized_for_follows_size_in_every_states_document_and_the_plan_never_shows() {
        let now = at(CREATED);
        let late = at(EXPIRES);
        let cases: [(Record, i64, &str, &[&str]); 7] = [
            (planned(), now, "pending", &[]),
            (ready(planned()), now, "ready", &["expires_at", "ssh"]),
            (ready(planned()), late, "expired", &["expires_at"]),
            (released(planned()), now, "released", &["released_at"]),
            (
                released(ready(planned())),
                now,
                "released",
                &["expires_at", "released_at"],
            ),
            (
                unsatisfiable(planned(), "no_capacity", "No capacity."),
                now,
                "unsatisfiable",
                &["reasons"],
            ),
            (too_large(), now, "unsatisfiable", &["reasons"]),
        ];
        for (r, when, state, tail) in cases {
            let doc = lease_document(&r, when);
            let mut expected = vec![
                "id",
                "project",
                "state",
                "platform",
                "profile",
                "size",
                "sized_for",
                "ttl_seconds",
                "created_at",
            ];
            expected.extend_from_slice(tail);
            assert_eq!(keys(&doc), expected, "{state}");
            assert_eq!(doc["state"], state);
            assert_eq!(
                keys(&doc["sized_for"]),
                ["plan", "turn", "agents"],
                "{state}"
            );
            assert!(validate_output(OutputKind::Lease, &doc), "{state}: {doc}");
            let text = doc.to_string();
            assert!(!text.contains("\"driver\""), "{text}");
            assert!(!text.contains("\"turns\""), "{text}");
            assert!(
                !text.contains("Opus") && !text.contains("\"effort\""),
                "{text}"
            );
        }
    }

    #[test]
    fn a_plan_too_large_document_passes_its_reason_through() {
        let doc = lease_document(&too_large(), at(CREATED));
        assert_eq!(
            doc["reasons"],
            json!([{"code": "plan_too_large", "message": TOO_LARGE}])
        );
        assert_eq!(doc["size"], "xl");
        assert_eq!(
            doc["sized_for"],
            json!({"plan": "v0.10", "turn": "T02", "agents": 33})
        );
        assert!(validate_output(OutputKind::Lease, &doc));
    }

    #[test]
    fn a_lease_made_without_a_plan_has_no_sized_for() {
        let doc = lease_document(&ready(plain()), at(CREATED));
        assert!(doc.get("sized_for").is_none());
        assert!(doc.get("plan").is_none());
        assert!(validate_output(OutputKind::Lease, &doc));
    }

    #[test]
    fn the_list_document_carries_sized_for_on_plan_leases_only() {
        let mut other = ready(plain());
        other.id = "c1d2e3f4-a5b6-47c8-99da-ebfc0d1e2f3a".to_string();
        other.ssh.as_mut().unwrap().host_alias = "agentspace-agentspace-c1d2e3f4".to_string();
        let f = StateFile {
            leases: vec![planned(), other],
        };
        let doc = list_document(&f, None, &[], at(CREATED));
        let leases = doc["leases"].as_array().unwrap();
        assert_eq!(
            leases[0]["sized_for"],
            json!({"plan": "v0.10", "turn": "T01", "agents": 2})
        );
        assert!(leases[1].get("sized_for").is_none());
        assert!(!doc.to_string().contains("\"turns\""));
        assert!(validate_output(OutputKind::LeaseList, &doc));
    }
}

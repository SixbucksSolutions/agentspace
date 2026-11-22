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
    Some(Record {
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

/// The members of a record that a lease document shares, as stored.
fn base_members(r: &Record) -> Map<String, Value> {
    let mut o = Map::new();
    o.insert("id".into(), r.id.clone().into());
    o.insert("project".into(), r.project.clone().into());
    o.insert("state".into(), r.state.clone().into());
    o.insert("platform".into(), r.platform.clone().into());
    o.insert("profile".into(), r.profile.clone().into());
    o.insert("size".into(), r.size.clone().into());
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

/// A record's members as a JSON object, `driver` included, for the schema check.
fn record_value(r: &Record) -> Value {
    let mut o = base_members(r);
    if let Some(d) = &r.driver {
        o.insert("driver".into(), d.clone().into());
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

/// Validates a document against one of the twelve embedded schemas, named by its file name
/// without `.schema.json`, such as `lease-create.input` or `leases.state`. `None` for a name
/// that is not one of them.
pub fn validate_schema(name: &str, doc: &Value) -> Option<bool> {
    schemas::validate(name, doc)
}

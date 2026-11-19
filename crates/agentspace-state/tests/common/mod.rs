//! Shared fixtures. Fixtures only ever name made-up values.
#![allow(dead_code)]

use serde_json::{Value, json};

pub const ID_A: &str = "3f2a9c1e-1111-4222-8333-444455556666";
pub const ID_B: &str = "9b1d0c77-2222-4333-9444-555566667777";
pub const ID_C: &str = "0a0a0a0a-3333-4444-a555-666677778888";

pub const CREATED: &str = "2026-11-19T10:00:00Z";
pub const EXPIRES: &str = "2026-11-19T11:00:00Z";
/// `EXPIRES` as Unix seconds.
pub const EXPIRES_UNIX: i64 = 1_795_086_000;

fn short(id: &str) -> &str {
    &id[..8]
}

pub fn ready(id: &str, project: &str) -> Value {
    json!({
        "id": id,
        "project": project,
        "state": "ready",
        "platform": "linux/arm64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": 3600,
        "created_at": CREATED,
        "expires_at": EXPIRES,
        "ssh": {"host_alias": format!("agentspace-{project}-{}", short(id)), "user": "ubuntu"},
        "driver": "ec2"
    })
}

pub fn pending(id: &str, project: &str) -> Value {
    json!({
        "id": id,
        "project": project,
        "state": "pending",
        "platform": "linux/arm64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": 3600,
        "created_at": CREATED,
        "driver": "ec2"
    })
}

pub fn released_from_ready(id: &str, project: &str) -> Value {
    json!({
        "id": id,
        "project": project,
        "state": "released",
        "platform": "linux/arm64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": 3600,
        "created_at": CREATED,
        "expires_at": EXPIRES,
        "released_at": "2026-11-19T10:30:00Z",
        "driver": "ec2"
    })
}

pub fn unsatisfiable(id: &str, project: &str) -> Value {
    json!({
        "id": id,
        "project": project,
        "state": "unsatisfiable",
        "platform": "windows/amd64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": 3600,
        "created_at": CREATED,
        "reasons": [
            {"code": "unsupported_platform",
             "message": "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only."}
        ],
        "driver": "ec2"
    })
}

pub fn file_of(records: Vec<Value>) -> Value {
    json!({"version": 0, "leases": records})
}

pub fn bytes_of(records: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&file_of(records)).unwrap()
}

/// A record with one member changed, or removed when `val` is `None`.
pub fn with(mut rec: Value, key: &str, val: Option<Value>) -> Value {
    let obj = rec.as_object_mut().unwrap();
    match val {
        Some(v) => {
            obj.insert(key.to_string(), v);
        }
        None => {
            obj.shift_remove(key);
        }
    }
    rec
}

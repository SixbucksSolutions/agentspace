//! Lease state file records for the tests of a provisioning run's deploy step (E0.09-T03-S02,
//! #0204).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Written from docs/state.md (The lease
//! state file, Live leases and the lease horizon) and docs/driver.md (Sizes and instance types, A
//! resizing run) alone: a record is the common fields, `driver`, and, by its stored state, the
//! members the state file's table gives it.
#![allow(dead_code)]

/// An expiry long after any test runs: a stored `ready` record with it is live.
pub const FAR: &str = "2099-01-01T00:00:00Z";
/// An expiry long before: a stored `ready` record with it is `expired`, which holds nothing.
pub const PAST: &str = "2020-01-01T00:00:00Z";

/// The EC2 driver's name in a record (state.md, The lease state file; driver.md, The driver's
/// name).
pub const EC2: &str = "ec2";

fn common(id: &str, project: &str, state: &str, size: &str) -> String {
    format!(
        "{{\"id\":\"{id}\",\"project\":\"{project}\",\"state\":\"{state}\",\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"{size}\",\"ttl_seconds\":28800,\"created_at\":\"2026-11-23T08:00:00Z\""
    )
}

fn driver_member(driver: Option<&str>) -> String {
    match driver {
        Some(d) => format!(",\"driver\":\"{d}\""),
        None => String::new(),
    }
}

/// A stored `pending` record of `project` for `size`, placed by `driver` (`None`: no `driver`).
pub fn pending_by(id: &str, project: &str, size: &str, driver: Option<&str>) -> String {
    format!(
        "{}{}}}",
        common(id, project, "pending", size),
        driver_member(driver)
    )
}

/// A stored `pending` record of the EC2 driver, which a run would place.
pub fn pending(id: &str, project: &str, size: &str) -> String {
    pending_by(id, project, size, Some(EC2))
}

/// A stored `ready` record of the EC2 driver, expiring at `expires_at`. Its alias is made up: no
/// test of a run reads it.
pub fn ready(id: &str, project: &str, size: &str, expires_at: &str) -> String {
    let short: String = id.chars().take(8).collect();
    format!(
        "{},\"expires_at\":\"{expires_at}\",\"ssh\":{{\"host_alias\":\"agentspace-{project}-{short}\",\"user\":\"ubuntu\"}}{}}}",
        common(id, project, "ready", size),
        driver_member(Some(EC2))
    )
}

/// A stored `released` record of a lease that was never `ready`.
pub fn released(id: &str, project: &str, size: &str) -> String {
    format!(
        "{},\"released_at\":\"2026-11-23T09:00:00Z\"{}}}",
        common(id, project, "released", size),
        driver_member(Some(EC2))
    )
}

/// The lease state file holding `records`, in the form the programs write: compact JSON and one
/// line feed.
pub fn file(records: &[String]) -> String {
    format!("{{\"version\":0,\"leases\":[{}]}}\n", records.join(","))
}

/// Eight made-up lease IDs, each in the contract's form, for tests that need several.
pub const IDS: [&str; 8] = [
    "8d41b6a2-0c3e-4f57-a1d9-2e6b7c8f9a01",
    "1f2e3d4c-5b6a-4978-8a1b-2c3d4e5f6a7b",
    "c0ffee00-1234-4abc-8def-0123456789ab",
    "3a3a3a3a-1111-4222-8333-444455556666",
    "4b4b4b4b-1111-4222-8333-444455556666",
    "5c5c5c5c-1111-4222-8333-444455556666",
    "6d6d6d6d-1111-4222-8333-444455556666",
    "7e7e7e7e-1111-4222-8333-444455556666",
];

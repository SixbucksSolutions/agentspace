//! `renew` (lease-api.md, `agentspace lease renew`; driver.md, Renew a lease; docs/state.md,
//! Renewal).

use agentspace_driver_api::LeaseRef;
use agentspace_state::{NotValid, format_time, lease_document, lock, read, serialize, write};
use serde_json::Value;

use super::support::{
    Purpose, check_value, context, driver_failure, failed, flag, lease_document_is_valid, lease_id,
    not_found, paths, state_dir, store_error, stored_driver, unwritable_why,
};
use crate::{ErrorCode, LeaseError, ParsedCommand, Sub, World};

/// `invalid_state` for a lease that is not `ready` (lease-api.md, Messages).
fn invalid_state(id: &str, state: &str) -> LeaseError {
    let advice = match state {
        "expired" => "Create a new lease.".to_owned(),
        "pending" => "Wait for it to be ready.".to_owned(),
        other => format!("The {other} state is final."),
    };
    LeaseError {
        code: ErrorCode::InvalidState,
        message: format!(
            "Lease '{id}' is {state}, and only a ready lease can be renewed. {advice}"
        ),
        field: None,
        lease_id: Some(id.to_owned()),
        state: Some(state.to_owned()),
    }
}

/// `renew <id> [--ttl <duration>]`: the ID, then the TTL, then the records under the lock (or no
/// home), `not_found`, `invalid_state`, the driver, the context, the renewal, the record, and then
/// `commit()` once the write succeeded or `rollback()` when it failed.
pub fn renew(cmd: &ParsedCommand, world: &World<'_>) -> Result<Value, LeaseError> {
    let id = lease_id(cmd)?;
    let ttl = flag(cmd, "ttl")
        .map(|v| check_value(v, "ttl", agentspace_values::ttl))
        .transpose()?;
    let paths = paths(world)?;
    let dir = state_dir(&paths, Sub::Renew)?;
    let held = lock(dir).map_err(store_error)?;
    // The clock is read once the lock is held, so a renew that waited on it decides expiry with
    // the time it reads the lease at (lease-api.md, States; driver.md, Renew a lease).
    let now = (world.now)();
    let file = read(dir).map_err(store_error)?;
    let index = file
        .leases
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| not_found(&id))?;
    let record = &file.leases[index];
    // The state as of now: a stored `ready` past its expiry is `expired`, refused before any
    // driver is asked (driver.md, Ordering and concurrency).
    let current = lease_document(record, now);
    let state = current
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or(&record.state);
    if state != "ready" {
        return Err(invalid_state(&id, state));
    }
    let ttl_seconds = match ttl {
        Some(t) => t,
        None => u32::try_from(record.ttl_seconds).map_err(|_| {
            failed(
                Sub::Renew,
                "The lease's recorded TTL cannot be used. Nothing was changed.",
            )
        })?,
    };
    let driver = stored_driver(world, record, Sub::Renew)?;
    let ctx = context(world, &paths, Sub::Renew, Purpose::Renew)?;
    let lease = LeaseRef {
        id: &id,
        project: &record.project,
        platform: &record.platform,
    };
    // The renewal is bound to a name and held, under the lock, until it is committed or rolled
    // back: dropped without either, it puts the previous certificate back (card #0106's ruling).
    let renewal = driver
        .renew(&ctx, &lease, ttl_seconds)
        .map_err(|f| driver_failure(Sub::Renew, &f))?;
    let mut next = file.clone();
    next.leases[index].expires_at = Some(format_time(renewal.expires_at()));
    next.leases[index].ttl_seconds = i64::from(ttl_seconds);
    let doc = lease_document(&next.leases[index], now);
    // The records' own check first, then the document's, then the write (as `create` does).
    let written = match serialize(&next) {
        Err(NotValid(why)) => Err(why),
        Ok(_) if !lease_document_is_valid(&doc) => {
            Err("the lease's document is not valid".to_owned())
        }
        Ok(_) => write(dir, &next, &held).map_err(|e| unwritable_why(&e).to_owned()),
    };
    match written {
        Ok(()) => {
            renewal.commit();
            drop(held);
            Ok(doc)
        }
        Err(why) => {
            let first = format!("The lease records could not be written: {why}.");
            let error = match renewal.rollback() {
                Ok(()) => LeaseError::internal(format!("{first} Nothing was changed.")),
                // lease-api.md, Messages, "Records unwritable (`renew`), and the previous
                // certificate not put back" (architect, 2026-11-19).
                Err(_) => failed(
                    Sub::Renew,
                    &format!(
                        "{first} Its previous certificate could not be put back; renew the lease \
                         again."
                    ),
                ),
            };
            drop(held);
            Err(error)
        }
    }
}

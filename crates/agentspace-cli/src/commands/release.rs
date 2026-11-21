//! `release` (lease-api.md, `agentspace lease release`; driver.md, Release a lease; docs/state.md,
//! Release, and what it leaves).

use agentspace_driver_api::LeaseRef;
use agentspace_state::{
    NotValid, StoreError, format_time, lease_document, lock, read, serialize, write,
};
use serde_json::Value;

use super::support::{
    Purpose, context, driver_failure, failed, lease_document_is_valid, lease_id, not_found,
    not_valid_document, paths, state_dir, store_error, stored_driver,
};
use crate::{LeaseError, ParsedCommand, Sub, World};

/// `release <id>`: the ID, then the records under the lock (or no home), `not_found`, and then by
/// the stored state, as lease-api.md's table says:
/// - `unsatisfiable`: nothing changes, and the lease is printed as it is.
/// - `pending` with no driver stored: the record is written `released`; no driver is asked and no
///   file is touched.
/// - `pending` that carries a driver, `ready` (which includes `expired`) and `released`: the
///   driver's `check_release`, then the record written `released` unless it already is, then the
///   driver's `release`. A failed delete leaves the record `released`, and a retry finishes the
///   job. So a placement killed partway leaves no key, certificate or alias once its lease is
///   released (lease-api.md, `release`; card #0150).
pub fn release(cmd: &ParsedCommand, world: &World<'_>) -> Result<Value, LeaseError> {
    let id = lease_id(cmd)?;
    let paths = paths(world)?;
    let dir = state_dir(&paths, Sub::Release)?;
    let held = lock(dir).map_err(store_error)?;
    // The clock is read once the lock is held, never before a wait on it.
    let now = (world.now)();
    let file = read(dir).map_err(store_error)?;
    let index = file
        .leases
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| not_found(&id))?;
    let record = &file.leases[index];

    let mut next = file.clone();
    let changed = match record.state.as_str() {
        "unsatisfiable" | "released" => false,
        "pending" | "ready" => {
            let r = &mut next.leases[index];
            r.state = "released".to_owned();
            r.released_at = Some(format_time(now));
            r.ssh = None;
            true
        }
        // The schema allows no other stored state.
        _ => {
            return Err(failed(
                Sub::Release,
                "The lease's recorded state is not one release knows. Nothing was changed.",
            ));
        }
    };
    // The change and its document are checked before anything changes, so a failed check changes
    // nothing: the records' own check first, as `create` and `renew` do.
    if changed && let Err(NotValid(why)) = serialize(&next) {
        return Err(store_error(StoreError::Unwritable(why)));
    }
    let doc = lease_document(&next.leases[index], now);
    if !lease_document_is_valid(&doc) {
        return Err(not_valid_document(Sub::Release));
    }

    let driver_has_it = match record.state.as_str() {
        "ready" | "released" => true,
        "pending" => record.driver.is_some(),
        _ => false,
    };
    if driver_has_it {
        let driver = stored_driver(world, record, Sub::Release)?;
        let ctx = context(world, &paths, Sub::Release, Purpose::Release)?;
        let lease = LeaseRef {
            id: &id,
            project: &record.project,
        };
        driver
            .check_release(&ctx, &lease)
            .map_err(|f| driver_failure(Sub::Release, &f))?;
        if changed {
            write(dir, &next, &held).map_err(store_error)?;
        }
        driver
            .release(&ctx, &lease)
            .map_err(|f| driver_failure(Sub::Release, &f))?;
    } else if changed {
        write(dir, &next, &held).map_err(store_error)?;
    }
    drop(held);
    Ok(doc)
}

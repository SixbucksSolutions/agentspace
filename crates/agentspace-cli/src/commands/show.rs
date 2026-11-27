//! `get` and `list` (lease-api.md, `agentspace lease get` and `agentspace lease list`). Neither
//! takes the lease state lock or writes (docs/state.md, The lease state lock). `list` asks no
//! driver, and `get` asks one only to check a pending lease (driver.md, check_pending).

use std::panic::{AssertUnwindSafe, catch_unwind};

use agentspace_driver_api::LeaseRef;
use agentspace_state::{OutputKind, Record, lease_document, list_document, read};
use serde_json::Value;

use super::support::{
    Paths, Purpose, check_value, check_values, checked, context, flag, flags, lease_id, not_found,
    paths, state_dir, store_error, stored_driver,
};
use crate::{LeaseError, ParsedCommand, Sub, World};

/// `get <id>`: the ID, then the records (or no home), then `not_found`.
pub fn get(cmd: &ParsedCommand, world: &World<'_>) -> Result<Value, LeaseError> {
    let id = lease_id(cmd)?;
    let now = (world.now)();
    let paths = paths(world)?;
    let dir = state_dir(&paths, Sub::Get)?;
    let file = read(dir).map_err(store_error)?;
    let record = file
        .leases
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| not_found(&id))?;
    // The document is made from the record as read, before the check, whatever the check does.
    let doc = checked(lease_document(record, now), OutputKind::Lease, Sub::Get)?;
    if record.state == "pending" {
        check_pending(world, &paths, record);
    }
    Ok(doc)
}

/// Asks the stored driver to check a pending lease (driver.md, check_pending). The check changes
/// nothing `get` prints, so a failure of it, of its context, or of finding its driver is dropped:
/// `get` MUST NOT fail, print or write because of it. A pending record with no driver, or with one
/// this program does not have, starts nothing.
///
/// Architect's to confirm: the CLI does not bound the call itself, since a `Driver` is not `Send`
/// and cannot be moved to a thread; the 20 seconds of driver.md's check_pending is the driver's own
/// to keep, and a panic in it is caught and dropped.
fn check_pending(world: &World<'_>, paths: &Paths, record: &Record) {
    if record.driver.is_none() {
        return;
    }
    let Ok(driver) = stored_driver(world, record, Sub::Get) else {
        return;
    };
    let Ok(ctx) = context(world, paths, Sub::Get, Purpose::Place) else {
        return;
    };
    let lease = LeaseRef {
        id: &record.id,
        project: &record.project,
        platform: &record.platform,
    };
    let _ = catch_unwind(AssertUnwindSafe(|| driver.check_pending(&ctx, &lease)));
}

/// `list [--project <name>] [--state <state>]...`: the project, then the states, then the
/// records, every state decided at one instant.
pub fn list(cmd: &ParsedCommand, world: &World<'_>) -> Result<Value, LeaseError> {
    let project = flag(cmd, "project")
        .map(|v| check_value(v, "project", agentspace_values::project))
        .transpose()?;
    let states = check_values(&flags(cmd, "state"), "state", agentspace_values::states)?;
    let now = (world.now)();
    let paths = paths(world)?;
    let dir = state_dir(&paths, Sub::List)?;
    let file = read(dir).map_err(store_error)?;
    checked(
        list_document(&file, project.as_deref(), &states, now),
        OutputKind::LeaseList,
        Sub::List,
    )
}

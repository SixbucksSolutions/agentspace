//! `get` and `list` (lease-api.md, `agentspace lease get` and `agentspace lease list`). Neither
//! takes the lease state lock, writes, or asks a driver (docs/state.md, The lease state lock).

use agentspace_state::{OutputKind, lease_document, list_document, read};
use serde_json::Value;

use super::support::{
    check_value, check_values, checked, flag, flags, lease_id, not_found, paths, state_dir,
    store_error,
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
    checked(lease_document(record, now), OutputKind::Lease, Sub::Get)
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

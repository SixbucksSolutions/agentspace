//! `create` (lease-api.md, `agentspace lease create`; driver.md, Ordering and concurrency and Place
//! a lease; docs/state.md, Creation, in order).

use std::collections::HashSet;

use agentspace_driver_api::{Context, Driver, LeaseRef, PlaceRequest, Placed};
use agentspace_state::{
    LockGuard, NotValid, Reason, Record, Ssh, StateFile, format_time, lease_document, lock, read,
    serialize, write,
};
use serde_json::Value;

use super::support::{
    Purpose, alias_of, check_value, context, driver_failure, failed, flag, lease_document_is_valid,
    paths, state_dir, store_error, unwritable_why,
};
use crate::quote::escape_controls;
use crate::{LeaseError, ParsedCommand, Sub, World};

/// How many IDs `create` draws before it gives up (lease-api.md, Fields: at least 100).
pub const MAX_DRAWS: usize = 100;

/// The values of a `create`, normalized, with their defaults.
struct Request {
    project: String,
    platform: String,
    profile: String,
    size: String,
    ttl_seconds: u32,
}

/// The values in the order of create's flag table: project, platform, profile, size, ttl.
fn request(cmd: &ParsedCommand) -> Result<Request, LeaseError> {
    let project = match flag(cmd, "project") {
        Some(v) => check_value(v, "project", agentspace_values::project)?,
        // The parser refuses a line without `--project`, so this is never reached.
        None => {
            return Err(failed(
                Sub::Create,
                "No project was given. Nothing was recorded.",
            ));
        }
    };
    let platform = match flag(cmd, "platform") {
        Some(v) => check_value(v, "platform", agentspace_values::platform)?,
        None => "linux/arm64".to_owned(),
    };
    let profile = match flag(cmd, "profile") {
        Some(v) => check_value(v, "profile", agentspace_values::profile)?,
        None => "agent-linux".to_owned(),
    };
    let size = match flag(cmd, "size") {
        Some(v) => check_value(v, "size", agentspace_values::size)?,
        None => "m".to_owned(),
    };
    let ttl_seconds = match flag(cmd, "ttl") {
        Some(v) => check_value(v, "ttl", agentspace_values::ttl)?,
        None => 8 * 3600,
    };
    Ok(Request {
        project,
        platform,
        profile,
        size,
        ttl_seconds,
    })
}

/// Draws a new lease ID, again while a kept lease (in any state) has the same ID or the same
/// alias, up to `MAX_DRAWS` draws (lease-api.md, Fields). Only records count, never files.
fn draw_id(world: &World<'_>, file: &StateFile, project: &str) -> Result<String, LeaseError> {
    let ids: HashSet<&str> = file.leases.iter().map(|r| r.id.as_str()).collect();
    let mut aliases: HashSet<String> = file
        .leases
        .iter()
        .map(|r| alias_of(&r.project, &r.id))
        .collect();
    aliases.extend(
        file.leases
            .iter()
            .filter_map(|r| r.ssh.as_ref().map(|s| s.host_alias.clone())),
    );
    for _ in 0..MAX_DRAWS {
        let drawn = (world.new_id)();
        // A draw that is not a normalized UUIDv4 is the random source's failure, not a collision.
        if agentspace_values::lease_id(&drawn).ok().as_deref() != Some(drawn.as_str()) {
            return Err(failed(
                Sub::Create,
                "A new lease ID could not be drawn. Nothing was recorded; try again.",
            ));
        }
        if !ids.contains(drawn.as_str()) && !aliases.contains(&alias_of(project, &drawn)) {
            return Ok(drawn);
        }
    }
    Err(failed(
        Sub::Create,
        "No lease ID whose host alias is free could be drawn. Nothing was recorded; try again.",
    ))
}

/// The record of a placed lease, with the driver's name (docs/state.md, The lease state file).
fn record_of(req: &Request, id: &str, now: i64, placed: Placed, driver: &str) -> Record {
    let mut record = Record {
        sized_for: None,
        plan: None,
        id: id.to_owned(),
        project: req.project.clone(),
        state: String::new(),
        platform: req.platform.clone(),
        profile: req.profile.clone(),
        size: req.size.clone(),
        ttl_seconds: i64::from(req.ttl_seconds),
        created_at: format_time(now),
        expires_at: None,
        released_at: None,
        ssh: None,
        reasons: None,
        driver: Some(driver.to_owned()),
    };
    match placed {
        Placed::Ready {
            host_alias,
            user,
            expires_at,
        } => {
            record.state = "ready".to_owned();
            record.expires_at = Some(format_time(expires_at));
            record.ssh = Some(Ssh { host_alias, user });
        }
        Placed::Pending => record.state = "pending".to_owned(),
        Placed::Unsatisfiable { reasons } => {
            record.state = "unsatisfiable".to_owned();
            record.reasons = Some(
                reasons
                    .into_iter()
                    .map(|r| Reason {
                        code: r.code.to_owned(),
                        message: r.message,
                    })
                    .collect(),
            );
        }
    }
    record
}

/// `create`: the values, then the records under the lock (or no home), then a driver, then the
/// context (or no home), then an ID, the place, the record, its document, and the write.
pub fn create(cmd: &ParsedCommand, world: &World<'_>) -> Result<Value, LeaseError> {
    let req = request(cmd)?;
    let paths = paths(world)?;
    let dir = state_dir(&paths, Sub::Create)?;
    let held = lock(dir).map_err(store_error)?;
    // The clock is read once the lock is held, never before a wait on it.
    let now = (world.now)();
    let file = read(dir).map_err(store_error)?;
    // driver.md, Failure: "A CLI with no driver MUST fail `create` with `internal`, in words that
    // name no driver" (lease-api.md, Messages, "No driver"). Which driver of several places a
    // lease is driver.md Q2's; the first is asked.
    let Some(driver) = world.drivers.first().map(|d| d.as_ref()) else {
        return Err(failed(
            Sub::Create,
            "No lease driver is available. Nothing was recorded.",
        ));
    };
    let ctx = context(world, &paths, Sub::Create, Purpose::Place)?;
    let id = draw_id(world, &file, &req.project)?;
    let placed = driver
        .place(
            &ctx,
            &PlaceRequest {
                id: &id,
                project: &req.project,
                platform: &req.platform,
                profile: &req.profile,
                size: &req.size,
                ttl_seconds: req.ttl_seconds,
            },
        )
        .map_err(|f| driver_failure(Sub::Create, &f))?;
    let ready = matches!(placed, Placed::Ready { .. });
    let record = record_of(&req, &id, now, placed, driver.name());
    let doc = lease_document(&record, now);
    let undo = Undo {
        driver,
        ctx: &ctx,
        id: &id,
        project: &req.project,
        ready,
    };
    let mut next = file;
    next.leases.push(record);
    // The records' own check comes first, so a result they refuse is "the change is not valid"
    // (driver.md, Place a lease); then the document's, before anything is written.
    if let Err(NotValid(why)) = serialize(&next) {
        return Err(undo.run(&why));
    }
    if !lease_document_is_valid(&doc) {
        return Err(undo.run("the lease's document is not valid"));
    }
    record_it(dir, &next, &held, &undo)?;
    Ok(doc)
}

fn record_it(
    dir: &std::path::Path,
    next: &StateFile,
    held: &LockGuard,
    undo: &Undo<'_>,
) -> Result<(), LeaseError> {
    write(dir, next, held).map_err(|e| undo.run(unwritable_why(&e)))
}

/// What a `create` that placed a lease and then could not record it does: a `ready` lease is
/// released again through its driver, and the first failure is reported, naming what that release
/// left (lease-api.md, Concurrency and atomicity; driver.md, Ordering and concurrency, gap FA-02).
struct Undo<'a> {
    driver: &'a dyn Driver,
    ctx: &'a Context,
    id: &'a str,
    project: &'a str,
    ready: bool,
}

impl Undo<'_> {
    /// Undoes the place and returns the failure to report. Its wording is lease-api.md's Messages
    /// table: "Records unwritable" for a lease that was not ready, and "Records unwritable after a
    /// ready place (`create`)", its undo leaving nothing or leaving files, for a ready one
    /// (architect, 2026-11-19).
    fn run(&self, why: &str) -> LeaseError {
        let first = format!("The lease records could not be written: {why}.");
        let plain = || LeaseError::internal(format!("{first} Nothing was changed."));
        if !self.ready {
            // Nothing was issued for a pending or unsatisfiable lease, so nothing is undone.
            return plain();
        }
        let lease = LeaseRef {
            id: self.id,
            project: self.project,
        };
        let tail = match self.driver.release(self.ctx, &lease) {
            // lease-api.md's "Records unwritable after a ready place (`create`), its undo leaving
            // nothing" row: never "Nothing was changed", since the Include line, a backup and
            // agentspace.d MAY stay (architect, 2026-11-19, bfb9a1b).
            Ok(()) => "No lease was recorded.".to_owned(),
            Err(f) if f.leftovers.is_empty() => {
                "No lease was recorded, but some of the lease's files could not be removed."
                    .to_owned()
            }
            Err(f) => format!(
                "No lease was recorded, and these could not be removed: {}.",
                escape_controls(&f.leftovers.join(", "))
            ),
        };
        failed(Sub::Create, &format!("{first} {tail}"))
    }
}

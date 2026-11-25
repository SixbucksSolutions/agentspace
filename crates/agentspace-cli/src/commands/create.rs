//! `create` (lease-api.md, `agentspace lease create`; driver.md, Ordering and concurrency and Place
//! a lease; docs/state.md, Creation, in order).

use std::collections::HashSet;

use agentspace_driver_api::{
    Context, Driver, LeaseRef, PlaceRequest, Placed, Reason as DriverReason,
};
use agentspace_state::{
    LockGuard, NotValid, Reason, Record, SizedFor, Ssh, StateFile, format_time, lease_document,
    lock, read, serialize, write,
};
use serde_json::Value;

use super::plan::read_plan;
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
    /// The size given, the default `m`, or the size picked for the plan: `xl` for a plan no size
    /// holds (D81).
    size: String,
    ttl_seconds: u32,
    /// What a `--plan` lease was sized for and from; `None` without `--plan`.
    plan: Option<Planned>,
}

/// What `--plan` adds to a lease (lease-api.md, Plan and Fields; docs/state.md, What a plan lease
/// adds).
struct Planned {
    sized_for: SizedFor,
    /// The plan document as parsed, for the record alone (pin P25).
    document: Value,
    /// Whether a size holds the plan's busiest turn; when none does, the lease is answered
    /// `plan_too_large` and no driver's place is called (D75, D81).
    fits: bool,
}

impl Planned {
    fn of(plan: &agentspace_plan::Plan) -> (Planned, &'static str) {
        let (turn, agents) = plan.busiest();
        let size = agentspace_plan::size_for(agents);
        let planned = Planned {
            sized_for: SizedFor {
                plan: plan.name().map(str::to_owned),
                turn: turn.to_owned(),
                agents,
            },
            document: plan.document().clone(),
            fits: size.is_some(),
        };
        (planned, size.unwrap_or(LARGEST_SIZE))
    }

    /// The one reason of a plan that no size holds, byte for byte (lease-api.md, Fields; pin P5).
    fn too_large(&self) -> DriverReason {
        DriverReason {
            code: "plan_too_large",
            message: format!(
                "Plan's busiest turn '{}' has {} agents at once; the largest size, {LARGEST_SIZE}, \
                 holds {}.",
                self.sized_for.turn,
                self.sized_for.agents,
                agentspace_plan::most_agents()
            ),
        }
    }
}

/// The largest size, which a `plan_too_large` lease's `size` is (lease-api.md, Size; D81).
const LARGEST_SIZE: &str = "xl";

/// The values in the order of create's flag table: project, platform, profile, plan, size, ttl
/// (lease-api.md, `create`, pin P13). The plan is read and parsed here, before `create` waits for
/// any other command (pin P28).
fn request(cmd: &ParsedCommand, world: &World<'_>) -> Result<Request, LeaseError> {
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
    let planned = match flag(cmd, "plan") {
        Some(v) => Some(Planned::of(&read_plan(v, world)?)),
        None => None,
    };
    // The parser refuses `--plan` with `--size`, so at most one of the two is given.
    let size = match (flag(cmd, "size"), &planned) {
        (Some(v), _) => check_value(v, "size", agentspace_values::size)?,
        (None, Some((_, picked))) => (*picked).to_owned(),
        (None, None) => "m".to_owned(),
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
        plan: planned.map(|(planned, _)| planned),
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
        // Only `create` writes these two, once, in the record it appends (docs/state.md, What a
        // plan lease adds; pin P17).
        sized_for: req.plan.as_ref().map(|p| p.sized_for.clone()),
        plan: req.plan.as_ref().map(|p| p.document.clone()),
        plan_source: None,
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
///
/// A plan that no size holds goes the same way up to the place, which it never reaches: in its
/// stead the CLI checks that the lease's way to reach its host is installed, as the place would,
/// and answers `plan_too_large` itself (lease-api.md, `create`; D75, D81; pins P26 and P32).
pub fn create(cmd: &ParsedCommand, world: &World<'_>) -> Result<Value, LeaseError> {
    let req = request(cmd, world)?;
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
    let placed = match req.plan.as_ref().filter(|p| !p.fits) {
        // No driver is asked to place it, so no driver sees a size the plan does not fit (D81).
        Some(planned) => {
            transport_is_installed(&ctx)?;
            Placed::Unsatisfiable {
                reasons: vec![planned.too_large()],
            }
        }
        None => driver
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
            .map_err(|f| driver_failure(Sub::Create, &f))?,
    };
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

/// The check of the lease's way to reach its host that a place makes before it issues anything,
/// made for a `plan_too_large` lease, which no place is asked for, "so a `plan_too_large` answer
/// still means the install is sound" (lease-api.md, `create`, pin P32): with no
/// `AGENTSPACE_PROXY_COMMAND`, agentspace's ProxyCommand program MUST be found beside this program
/// (docs/state.md, The alias file), or `create` fails with lease-api.md's "Way to reach the host
/// not found" and records nothing.
fn transport_is_installed(ctx: &Context) -> Result<(), LeaseError> {
    if ctx.proxy_override.is_some() || agentspace_alias::find_proxy_program(&ctx.own_exe).is_some()
    {
        return Ok(());
    }
    Err(failed(
        Sub::Create,
        "The lease's way to reach its host cannot be found: agentspace's ProxyCommand program is \
         not installed beside this program. No lease was recorded.",
    ))
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

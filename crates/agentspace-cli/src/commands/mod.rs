//! The five lease commands (lease-api.md, Commands; driver.md, Ordering and concurrency;
//! docs/state.md, The lease state file and The lease state lock).
//!
//! RFC 2119 keywords; the capitals are load-bearing.
//!
//! - **The world's `var` MUST be the only way these commands read the environment**, and nothing
//!   here calls `std::env` (the architect's conditions on the seam, 2026-11-19).
//! - **`create`, `renew` and `release` take the lease state lock before they read the records, and
//!   hold it until the outcome is recorded; `get` and `list` MUST NOT take it.**
//! - **Each command checks its values in its table's order, after the parser's usage errors, and
//!   reports errors in lease-api.md's order** (Exit codes and errors): the values, then the records
//!   or the no-home failure, then `not_found`, then `invalid_state`, then any other `internal`.
//! - **Every document is checked against its output schema before it is printed**, and before the
//!   change it reports is made where the document is known first.

mod create;
mod plan;
mod release;
mod renew;
mod show;
mod support;

#[cfg(test)]
mod tests;

use agentspace_driver_api::Driver;
use serde_json::Value;

use crate::{LeaseError, ParsedCommand, Sub, World};

/// The drivers this build has: the EC2 driver, the one v0 driver (docs/driver.md, The EC2 driver;
/// D38). It was empty in E0.05-T03-S02, and E0.05-T04-S01 registers the EC2 driver.
pub fn drivers() -> Vec<Box<dyn Driver>> {
    vec![Box::new(agentspace_driver_ec2::Ec2Driver::new())]
}

/// Runs a parsed command line in `world` and returns the document to print.
pub fn run(cmd: ParsedCommand, world: &World<'_>) -> Result<Value, LeaseError> {
    match cmd.sub {
        Sub::Create => create::create(&cmd, world),
        Sub::Get => show::get(&cmd, world),
        Sub::Renew => renew::renew(&cmd, world),
        Sub::Release => release::release(&cmd, world),
        Sub::List => show::list(&cmd, world),
    }
}

//! agentspace-driver-ec2 (docs/driver.md, "The EC2 driver"; docs/decisions.md, D38): the one driver
//! v0 has. It places, renews and releases a lease on the caller's machine, from the credential
//! crate and the alias crate, and answers through the pinned `agentspace-driver-api` trait.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The driver MUST NOT read an environment variable, write to stdout or stderr, or prompt.**
//!   Everything it needs comes in the [`Context`](agentspace_driver_api::Context).
//! - **A message MUST NOT name the driver, its backend, or a path** (D1), except a home-relative
//!   leftover, or an alias file outside the home folder named as `the host alias file <alias>`.
//! - **A function MUST NOT panic on any input.**
//! - **Place MUST follow state.md's "Creation, in order"**, and a failed place MUST remove what it
//!   made and keep the `Include` line, any backup and `agentspace.d`.

#![forbid(unsafe_code)]

mod ops;
mod reasons;
mod text;

use agentspace_driver_api::{Context, Driver, Failure, LeaseRef, PlaceRequest, Placed, Renewal};

/// The EC2 driver. It holds nothing: everything it needs is in the context of each call.
#[derive(Debug, Default, Clone, Copy)]
pub struct Ec2Driver;

impl Ec2Driver {
    /// A new driver.
    pub fn new() -> Self {
        Ec2Driver
    }
}

impl Driver for Ec2Driver {
    fn name(&self) -> &'static str {
        "ec2"
    }

    fn place(&self, ctx: &Context, req: &PlaceRequest) -> Result<Placed, Failure> {
        ops::place(ctx, req)
    }

    fn renew(
        &self,
        ctx: &Context,
        lease: &LeaseRef,
        ttl_seconds: u32,
    ) -> Result<Box<dyn Renewal>, Failure> {
        ops::renew(ctx, lease, ttl_seconds)
    }

    fn check_release(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        ops::check_release(ctx, lease)
    }

    fn release(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        ops::release(ctx, lease)
    }
}

/// Places every `pending` lease of `project` that this driver placed, under the lease state lock,
/// and writes each `ready` (driver.md, place_pending). Returns how many it placed. The
/// provisioning run calls it as its last step (operator.md, provision).
///
/// A stub pinned by the architect, 2026-11-20, before v0.07-T03 (D55): it places nothing and
/// fails until E0.07-T03-S01 builds it.
pub fn place_pending(ctx: &Context, project: &str) -> Result<usize, Failure> {
    let _ = (ctx, project);
    Err(Failure {
        message: "Placing the pending leases is not built yet.".to_string(),
        leftovers: Vec::new(),
    })
}

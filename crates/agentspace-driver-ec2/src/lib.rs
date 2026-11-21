//! agentspace-driver-ec2 (docs/driver.md, "The EC2 driver"; docs/decisions.md, D38, D48): the one
//! driver v0 has. It places, renews and releases a lease on the caller's machine, from the
//! credential crate and the alias crate, and answers through the pinned `agentspace-driver-api`
//! trait. From v0.07 it places a lease by its project's host, through agentspace-hosts, answers
//! `pending` or `no_capacity` for a project whose host is not `ready`, starts a provisioning run
//! through agentspace-detach, and places the `pending` leases once a run has made the host
//! ([`place_pending`]).
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

mod host;
mod ops;
mod pending;
mod reasons;
mod text;

use std::sync::Arc;

use agentspace_detach::Detach;
use agentspace_driver_api::{Context, Driver, Failure, LeaseRef, PlaceRequest, Placed, Renewal};

/// The driver's name, which the CLI stores with every lease it places (driver.md, The driver's
/// name). `place_pending` places only the records that carry it.
pub(crate) const NAME: &str = "ec2";

/// The EC2 driver. It holds only the detached start it uses for a provisioning run: everything
/// else it needs is in the context of each call.
#[derive(Clone)]
pub struct Ec2Driver {
    detach: Arc<dyn Detach + Send + Sync>,
}

/// Prints the type's name alone: the detached start has nothing a reader needs.
impl std::fmt::Debug for Ec2Driver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ec2Driver { .. }")
    }
}

impl Default for Ec2Driver {
    fn default() -> Self {
        Ec2Driver::new()
    }
}

impl Ec2Driver {
    /// A new driver, which starts a provisioning run through WMI's `Win32_Process.Create` on
    /// Windows (`WmiDetach`), and starts none elsewhere (`NoDetach`).
    pub fn new() -> Self {
        #[cfg(windows)]
        let detach: Arc<dyn Detach + Send + Sync> = Arc::new(agentspace_detach::WmiDetach);
        #[cfg(not(windows))]
        let detach: Arc<dyn Detach + Send + Sync> = Arc::new(agentspace_detach::NoDetach);
        Ec2Driver { detach }
    }

    /// A driver that starts a provisioning run through `detach`: the seam through which a test
    /// stands in a fake start.
    pub fn with_detach(detach: Arc<dyn Detach + Send + Sync>) -> Ec2Driver {
        Ec2Driver { detach }
    }
}

impl Driver for Ec2Driver {
    fn name(&self) -> &'static str {
        NAME
    }

    fn place(&self, ctx: &Context, req: &PlaceRequest) -> Result<Placed, Failure> {
        host::place(ctx, req, &self.detach)
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

    /// The same for a `ready`, `released` or `pending` lease: the alias file only when its owner
    /// mark names the lease, then its folder, a missing one being no failure, so a placement that
    /// was killed part way leaves nothing once its lease is released.
    fn release(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        ops::release(ctx, lease)
    }

    fn check_pending(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        host::check_pending(ctx, lease, &self.detach)
    }
}

/// What [`place_pending`] did: how many leases it placed, and each it could not, with why
/// (driver.md, place_pending: its result names each lease it could and could not place).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingPlaced {
    /// How many `pending` leases it placed and wrote `ready`.
    pub placed: usize,
    /// Each lease it left `pending`, in the order it tried them.
    pub failed: Vec<PendingFailure>,
}

/// One lease [`place_pending`] could not place. It stays `pending`, for the next run (D48).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFailure {
    /// The lease's normalized ID.
    pub lease_id: String,
    /// Why, in agentspace's own words, as a [`Failure`]'s message is: no path outside the home,
    /// no backend name (D1), never a secret. The run writes it to its log.
    pub message: String,
}

/// Places every `pending` lease of `project` that this driver placed, under the lease state lock,
/// and writes each `ready` (driver.md, place_pending). The provisioning run calls it as its last
/// step and logs each lease it could not place (operator.md, provision). `Err` is a failure of the
/// whole call, such as a lease state file that cannot be read; one lease's failure is in
/// [`PendingPlaced::failed`] and stops no other.
///
/// Each lease is placed under a hold of its own of the lease state lock. A host record that is not
/// `ready`, or none, places nothing and answers no failure. The call fails as a whole only when the
/// host record, the lease state lock or the lease state file cannot be had; what it placed before
/// stays `ready`. A lease that cannot be placed stays `pending`, with every file this call made for
/// it removed. It starts nothing.
pub fn place_pending(ctx: &Context, project: &str) -> Result<PendingPlaced, Failure> {
    pending::place_pending(ctx, project)
}

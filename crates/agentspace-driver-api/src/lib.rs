//! The boundary between the lease CLI and a driver, as a Rust trait (docs/driver.md; D38).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The architect committed this crate before v0.05-T03 from the pin on cards #0105 and #0106, so
//! that E0.05-T03-S01 implements it and E0.05-T03-S02 calls it at the same time. A change to it is
//! the architect's. It holds no logic.
//!
//! - **A driver MUST NOT read an environment variable, write to stdout or stderr, or prompt**
//!   (driver.md, The boundary). The CLI builds the [`Context`].
//! - **A driver MUST NOT name itself, its backend, a host or a path in any message**
//!   (driver.md, What a driver never exposes; D1).
//! - **The CLI prints every [`Failure`] as `internal`** (driver.md, Failure).

#![forbid(unsafe_code)]

use std::path::PathBuf;

/// What the CLI hands a driver with every operation, so that no driver reads the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// The home folder, made full (docs/state.md, The home folder).
    pub home: PathBuf,
    /// The resolved `<ssh folder>`: `AGENTSPACE_SSH_DIR` when set and not empty, else the home's
    /// `.ssh` (docs/state.md, What moves which files).
    pub ssh_dir: PathBuf,
    /// `AGENTSPACE_PROXY_COMMAND` when set and not empty (lease-api.md, Environment).
    pub proxy_override: Option<String>,
    /// `SystemRoot` when set and not empty, for the `cmd.exe /d /c` form of the default
    /// ProxyCommand (docs/state.md, The alias file, the `<system folder>` row). Added by the
    /// architect, 2026-11-19, for E0.05-T03-S01: the pin had left it out, and a driver reads no
    /// environment variable.
    pub system_root: Option<String>,
    /// The running program's own file, a full path, never its first argument (operator.md, How
    /// each program finds the others).
    pub own_exe: PathBuf,
    /// The resolved state folder, made full (docs/state.md, The home folder): where
    /// `place_pending` reads and writes the lease state file. Added by the architect, 2026-11-20,
    /// before v0.07-T03 (D55, "Changed during T01").
    pub state_dir: PathBuf,
    /// The program a provisioning run starts (driver.md, The operator program): the full path of
    /// `AGENTSPACE_PROVISION_CMD` when set and not empty, else `agentspace-operator.exe` beside
    /// the CLI. `None` until E0.07-T03-S01 resolves it. Added by the architect, 2026-11-20, before
    /// v0.07-T03 (D55).
    pub operator_program: Option<PathBuf>,
}

/// A request to place a lease (driver.md, Place a lease). Every value is already normalized and
/// checked by the CLI (lease-api.md, Values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceRequest<'a> {
    /// A new lowercase UUIDv4 whose alias no kept lease uses.
    pub id: &'a str,
    pub project: &'a str,
    pub platform: &'a str,
    pub profile: &'a str,
    pub size: &'a str,
    /// From 60 to 86400; a driver MUST NOT clamp it.
    pub ttl_seconds: u32,
}

/// Why a request cannot be met (lease-api.md, Fields: the reason codes and messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    /// One of `unsupported_platform`, `unsupported_profile`, `unsupported_size`, `no_capacity`.
    pub code: &'static str,
    /// One sentence for a person, naming the value that cannot be met.
    pub message: String,
}

/// What a place answers (driver.md, Place a lease, "The result").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placed {
    /// The credential and alias are in place. `expires_at` is the certificate's valid-before
    /// time, in Unix seconds: its issue time plus the TTL exactly.
    Ready {
        host_alias: String,
        user: String,
        expires_at: i64,
    },
    /// Accepted and not yet ready; no credential was issued. The v0 driver never answers it.
    Pending,
    /// The request cannot be met as asked; nothing was issued. 1 to 4 reasons, each code once, in
    /// lease-api.md's order.
    Unsatisfiable { reasons: Vec<Reason> },
}

/// The lease a renew, a release check or a release is about (driver.md, Renew a lease, Release a
/// lease).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRef<'a> {
    /// The lease's normalized ID.
    pub id: &'a str,
    /// The project stored at place, from which the alias is built.
    pub project: &'a str,
}

/// A failure at the boundary (driver.md, Failure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// A message in agentspace's own words, without the `agentspace lease <sub> failed: ` prefix,
    /// which the CLI adds. It MUST NOT be empty or start with a space, and MUST meet lease-api.md's
    /// rules for an `internal` message.
    pub message: String,
    /// Each file or folder a failed operation could not remove, as lease-api.md's Concurrency and
    /// atomicity names it: home-relative under the home folder, or by what it is and its name
    /// outside it.
    pub leftovers: Vec<String>,
}

/// A renewal whose new certificate is in place (docs/state.md, Renewal, steps 1 to 5). The CLI
/// writes the record, then commits, or rolls back when the write failed (steps 6 to 8; driver.md
/// Q4). It MUST be bound to a name and held, under the lease state lock, until one of the two is
/// called.
pub trait Renewal {
    /// The new certificate's valid-before time, in Unix seconds.
    fn expires_at(&self) -> i64;
    /// The record is written: the renewal's temporary files are removed.
    fn commit(self: Box<Self>);
    /// The record could not be written: the previous certificate is put back byte for byte, or
    /// the new one deleted when there was none.
    fn rollback(self: Box<Self>) -> Result<(), Failure>;
}

/// A driver (driver.md, The operations).
pub trait Driver {
    /// The driver's name, which the CLI stores with every lease it places and never prints
    /// (driver.md, The driver's name). The v0 driver's is `ec2`.
    fn name(&self) -> &'static str;

    /// Places a lease (driver.md, Place a lease). A failure MUST leave nothing this call made
    /// but what docs/state.md, Creation in order, lets stay.
    fn place(&self, ctx: &Context, req: &PlaceRequest) -> Result<Placed, Failure>;

    /// Gives a `ready` lease a new certificate (driver.md, Renew a lease).
    fn renew(
        &self,
        ctx: &Context,
        lease: &LeaseRef,
        ttl_seconds: u32,
    ) -> Result<Box<dyn Renewal>, Failure>;

    /// Release's step 1 (docs/state.md, Release, and what it leaves): checks the alias file's
    /// owner mark, and changes nothing.
    fn check_release(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure>;

    /// Deletes the alias file, then the lease folder. A missing one is not a failure. It MUST work
    /// for a lease just placed that has no record (driver.md, Ordering and concurrency).
    fn release(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure>;

    /// Asked by `get` for a lease whose stored state is `pending` (driver.md, check_pending). It
    /// MUST NOT write the lease state file or take its lock, MUST end within 20 seconds, and MAY
    /// start work that outlives `get`. The CLI drops its failure. The default does nothing, for a
    /// driver that never answers `pending`. Added by the architect, 2026-11-20, before v0.07-T03
    /// (D55); E0.07-T03-S01 builds the EC2 driver's.
    fn check_pending(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        let _ = (ctx, lease);
        Ok(())
    }
}

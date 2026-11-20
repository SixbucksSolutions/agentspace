//! The operator's `sync` and `deploy` (docs/operator.md, sync and deploy; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The architect committed this stub before v0.06-T03 from the pin on cards #0126 and #0127, so
//! that `cargo xtask dist` can name every program while E0.06-T03-S02 builds it and E0.06-T03-S01
//! calls it. Its signatures are the pin, and a change to them is the architect's. The `CaEnv`
//! parameter was added to the card's pin by the architect on 2026-11-19, since E0.06-T02-S01's
//! ruling gives `agentspace-ca` its own input and `sync`'s first step is the CA check.

#![forbid(unsafe_code)]

use agentspace_ca::CaEnv;
use agentspace_operator_api::{Aws, CallerFailure, Context, Failure, Remote, Say};

/// `operator sync` (docs/operator.md, sync). Not built yet.
pub fn sync(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    let _ = (ctx, ca_env, say, aws, remote);
    Err(not_built("sync"))
}

/// `operator deploy` (docs/operator.md, deploy), which ends by running `sync`. Not built yet.
pub fn deploy(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    let _ = (ctx, ca_env, say, aws, remote);
    Err(not_built("deploy"))
}

fn not_built(command: &str) -> Failure {
    Failure {
        message: format!("agentspace: {command} is not built yet."),
        caller: CallerFailure::CouldNotReach,
    }
}

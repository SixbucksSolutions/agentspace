//! The calls into the crate's hidden functions for a host (E0.07-T03-S03, #0152).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The four functions are pinned by the dev
//! lead in `s03-seams.md`: `sync_target_using`, `deploy_target_using`, `bootstrap_using` and
//! `provision_using`, with `deploy_target_sized_using` and `sync_all_using`, the hidden twins of
//! the public `deploy_with_instance_type` and `sync_all` (architect, 2026-11-21), each over [`agentspace_sync::Seams`] built only through
//! `Seams::real_except_go`. They are kept apart from `common` so that the test files that do not
//! use them build on `main`'s stubs.
//!
//! Like `Rig::do_sync`, each runs the crate's own code with the operator's own clock, CA check and
//! push, a `go` finder that names the rig's stand-in or nothing, and a localswim folder of the
//! rig's own, so no test reaches the real `go` or `C:\Projects\localswim`.
#![cfg(windows)]
#![allow(dead_code)]

use agentspace_operator_api::Failure;
use agentspace_sync::Target;

use crate::common::{RecSay, Rig, ca_env};

pub mod setup;

impl Rig {
    /// The crate's `sync_target_using` for `target`.
    pub fn do_sync_target(&self, say: &RecSay, target: Target<'_>) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::sync_target_using(&ctx, &ca_env(), target, say, &run, &run, seams)
        })
    }

    /// The crate's `deploy_target_using` for `target`.
    pub fn do_deploy_target(&self, say: &RecSay, target: Target<'_>) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::deploy_target_using(&ctx, &ca_env(), target, say, &run, &run, seams)
        })
    }

    /// The crate's `deploy_target_sized_using` for `target`, with `-InstanceType`'s value.
    pub fn do_deploy_sized(
        &self,
        say: &RecSay,
        target: Target<'_>,
        instance_type: Option<&str>,
    ) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::deploy_target_sized_using(
                &ctx,
                &ca_env(),
                target,
                instance_type,
                say,
                &run,
                &run,
                seams,
            )
        })
    }

    /// The crate's `sync_all_using`: `sync -All`.
    pub fn do_sync_all(&self, say: &RecSay) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::sync_all_using(&ctx, &ca_env(), say, &run, &run, seams)
        })
    }

    /// The crate's `bootstrap_using` for `target`.
    pub fn do_bootstrap(&self, say: &RecSay, target: Target<'_>) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::bootstrap_using(&ctx, &ca_env(), target, say, &run, &run, seams)
        })
    }

    /// The crate's `provision_using` for `project`.
    pub fn do_provision(&self, say: &RecSay, project: &str) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::provision_using(&ctx, &ca_env(), project, say, &run, &run, seams)
        })
    }
}

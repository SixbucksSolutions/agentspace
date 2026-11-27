//! Starting the box and waiting for it, and the instance-ID cache (docs/operator.md; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! This crate is the start-and-wait step that `up`, `sync` and the ProxyCommand's `up` share
//! (operator.md, Starting the box and waiting for it, and What up waits for), and the decisions
//! about the instance-ID cache (operator.md, Laptop files; state.md, Files in the checkout). It
//! works against the pinned [`Aws`], [`Say`] and [`Failure`] of `agentspace-operator-api`, and
//! against a [`Clock`] of its own, so that its waits run at once under a fake clock.
//!
//! **It touches no file.** The cache's bytes come in and go out through [`InstanceIdCache`], which
//! the program implements (T03); reading and atomically replacing the file is that
//! implementation's.
//!
//! **It prints only through [`Say`]**, and every failure is a [`Failure`] whose `caller` picks the
//! caller's line (operator.md, What a caller sees), so that caller mode never shows backend text.
//!
//! # The public API
//!
//! The entry points:
//! - [`start_and_wait`]: `up`'s steps 1 and 2. Reads the instance ID (from the cache, or rebuilt
//!   from the host's `stack`, a parameter: `agentbox` for the box), starts the box and waits for it, ending with `Ready: <id>`. Returns the ID.
//! - [`start_and_wait_on`]: [`start_and_wait`] on a host's platform, whose patch check and its
//!   messages are the platform's (operator.md, The platform seam). [`start_and_wait`] is it on
//!   `linux/arm64`.
//! - [`start_and_wait_for`]: the shared start-and-wait step alone, for an ID already read.
//! - [`instance_id`]: the instance ID from the cache, or from the given stack's output when the cache
//!   counts as missing, handing the cache's new bytes to [`InstanceIdCache::write`].
//!
//! The cache's decisions, as pure functions:
//! - [`parse_cache`]: the ID in a cache's bytes, or `None` for every case that counts as missing.
//! - [`read_cache`]: the decision on one read of the file: an ID, missing, or FA-16's failure. A
//!   `std::fs::read` result converts into a [`CacheRead`], not found counting as missing.
//! - [`cache_bytes`]: the bytes a cache holding an ID is written as.
//! - [`cache_unreadable`]: FA-16's failure for a cache that cannot be opened.
//! - [`instance_id_from_stack`]: the given stack's `InstanceId` output, through
//!   `cloudformation describe-stacks`, or the "no InstanceId output" failure naming that stack.
//!
//! The guard's calls (operator.md, guard):
//! - [`describe_hosts`]: one `ec2 describe-instances` with a filter naming every host's instance
//!   ID, read into a [`Described`] per ID, in the order given: [`DescribedState::Unknown`],
//!   `Running` with its launch time, `Other`, or `Unexpected` for a member missing.
//! - [`stop_host`]: one `ec2 stop-instances`, asking EC2 to stop the instance and not waiting.
//! - [`send_horizon`]: one `ssm send-command`, with the comment [`HORIZON_COMMENT`], that writes
//!   the lease horizon to `/run/agentspace/lease-horizon` on the host; [`send_horizon_on`], the
//!   same to a host of any platform, with the platform's horizon command and document.
//!
//! The seams and helpers:
//! - [`Clock`], with [`SystemClock`] the real one.
//! - [`InstanceIdCache`] and [`CacheRead`]: the cache file, as the program reads and writes it.
//! - [`failure_line`]: the one line a [`Failure`] prints in a [`Mode`].
//! - [`PATCH_QUESTION`] and [`PATCH_COMMENT`]: what the patch check sends.

#![forbid(unsafe_code)]

mod answer;
mod cache;
mod clock;
mod failure;
mod guard;
mod launch_time;
mod run_command;
mod wait;

pub use cache::{
    CacheRead, InstanceIdCache, cache_bytes, cache_unreadable, instance_id, instance_id_from_stack,
    parse_cache, read_cache,
};
pub use clock::{Clock, SystemClock};
pub use failure::failure_line;
pub use guard::{
    Described, DescribedState, HORIZON_COMMENT, describe_hosts, send_horizon, send_horizon_on,
    stop_host,
};
pub use wait::{PATCH_COMMENT, PATCH_QUESTION, start_and_wait_for};

use agentspace_operator_api::{Aws, Failure, HostPlatform, Say};

#[cfg(doc)]
use agentspace_operator_api::Mode;

/// `up`'s steps 1 and 2 (operator.md, up): reads the instance ID from the cache, or from the
/// output of `stack`, the host's stack (`agentbox` for the box), when the cache counts as missing,
/// writing the cache then; then starts the box
/// and waits for it, ending with `Ready: <id>` (the caller's `The host is ready.` in caller mode).
///
/// Returns the instance ID once the box is ready. Every failure ends the step at once, with
/// nothing after it run.
///
/// It is `linux/arm64`'s: [`start_and_wait_on`] with `agentspace_platforms::LINUX_ARM64`.
pub fn start_and_wait(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    cache: &dyn InstanceIdCache,
    stack: &str,
) -> Result<String, Failure> {
    start_and_wait_on(
        aws,
        say,
        clock,
        cache,
        stack,
        &agentspace_platforms::LINUX_ARM64,
    )
}

/// [`start_and_wait`] for a host of `platform` (operator.md, The platform seam): its patch check
/// sends [`HostPlatform::patch_check_command`] through [`HostPlatform::ssm_document`], and its
/// phase message and deadline are worded with [`HostPlatform::patch_messages`]. On both Linux
/// platforms every call and line is [`start_and_wait`]'s, byte for byte (`--parameters
/// commands=<question>`); on a platform whose document is not `AWS-RunShellScript`, such as
/// `AWS-RunPowerShellScript`, the parameters are the JSON value `{"commands":["<question>"]}`,
/// never the shorthand (pins P17 and P18 of v0.12-T02).
pub fn start_and_wait_on(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    cache: &dyn InstanceIdCache,
    stack: &str,
    platform: &dyn HostPlatform,
) -> Result<String, Failure> {
    let id = instance_id(aws, cache, stack)?;
    wait::start_and_wait_for_on(aws, say, clock, &id, platform)?;
    Ok(id)
}

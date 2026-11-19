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
//!   from the stack), starts the box and waits for it, ending with `Ready: <id>`. Returns the ID.
//! - [`start_and_wait_for`]: the shared start-and-wait step alone, for an ID already read.
//! - [`instance_id`]: the instance ID from the cache, or from the stack's output when the cache
//!   counts as missing, handing the cache's new bytes to [`InstanceIdCache::write`].
//!
//! The cache's decisions, as pure functions:
//! - [`parse_cache`]: the ID in a cache's bytes, or `None` for every case that counts as missing.
//! - [`read_cache`]: the decision on one read of the file: an ID, missing, or FA-16's failure. A
//!   `std::fs::read` result converts into a [`CacheRead`], not found counting as missing.
//! - [`cache_bytes`]: the bytes a cache holding an ID is written as.
//! - [`cache_unreadable`]: FA-16's failure for a cache that cannot be opened.
//! - [`instance_id_from_stack`]: the stack's `InstanceId` output, through
//!   `cloudformation describe-stacks`, or the "no InstanceId output" failure.
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
mod launch_time;
mod wait;

pub use cache::{
    CacheRead, InstanceIdCache, cache_bytes, cache_unreadable, instance_id, instance_id_from_stack,
    parse_cache, read_cache,
};
pub use clock::{Clock, SystemClock};
pub use failure::failure_line;
pub use wait::{PATCH_COMMENT, PATCH_QUESTION, start_and_wait_for};

use agentspace_operator_api::{Aws, Failure, Say};

#[cfg(doc)]
use agentspace_operator_api::Mode;

/// `up`'s steps 1 and 2 (operator.md, up): reads the instance ID from the cache, or from the
/// stack's output when the cache counts as missing, writing the cache then; then starts the box
/// and waits for it, ending with `Ready: <id>` (the caller's `The host is ready.` in caller mode).
///
/// Returns the instance ID once the box is ready. Every failure ends the step at once, with
/// nothing after it run.
pub fn start_and_wait(
    aws: &dyn Aws,
    say: &dyn Say,
    clock: &dyn Clock,
    cache: &dyn InstanceIdCache,
) -> Result<String, Failure> {
    let id = instance_id(aws, cache)?;
    start_and_wait_for(aws, say, clock, &id)?;
    Ok(id)
}

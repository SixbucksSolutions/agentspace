//! agentspace-board: the plan block, and the one GET that reads an epic (docs/lease-api.md, The
//! board read; D86, D89).

mod block;
mod client;

pub use block::{BlockError, plan_block};
pub use client::{BoardError, Epic, read_epic};

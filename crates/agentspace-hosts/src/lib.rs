//! agentspace-hosts (docs/decisions.md, D45, D54, D55).
//!
//! The host registry (`hosts.json`) and its lock, the project list (`projects.json`), every path of
//! a host's folder, and a host's provisioning run lock (docs/state.md, The host registry, The
//! project list, A host's folder and Concurrency). RFC 2119 keywords in this crate's comments, and
//! the capitals are load-bearing.
//!
//! A library crate: it never writes to stdout or stderr, and it never panics on bad input. Every
//! function takes the home folder, which is the value of `USERPROFILE` made full by the caller: the
//! crate reads no environment variable, so a check runs it against a throwaway home. The registry,
//! its lock, the project list and a host's folder all live under `<home>\.agentspace`, and are
//! never moved by `AGENTSPACE_STATE_DIR` (docs/state.md, What moves which files).
//!
//! The Windows locks are the contract's: a file opened with share mode 0. On any other system the
//! same interface builds with an advisory lock, which is not the contract's (docs/state.md, D8).

#![deny(unsafe_code)]

mod error;
mod folder;
mod json;
mod model;
mod paths;
mod projects;
mod registry;
mod runlock;
mod sys;

pub use error::{Failure, HostsError, Invalid};
pub use model::{
    BOX_PROJECT, BOX_STACK, Host, HostState, Hosts, Target, host_alias, host_for, is_box,
    is_normalized_project, stack_name, target_for,
};
pub use paths::{
    agentspace_dir, host_cert_path, host_dir, host_key_path, host_log_path, host_run_lock_path,
    host_stamp_path, hosts_dir, hosts_lock_path, hosts_path, known_hosts_path, projects_path,
};
pub use projects::{Project, Projects, is_listed, read_projects};
pub use registry::{HostsLock, lock, read, write};
pub use runlock::{RunLock, run_is_running, try_run_lock};

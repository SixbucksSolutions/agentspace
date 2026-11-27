//! The keeper's files and the holder command's matcher (docs/state.md; docs/keeper.md; D42)
//!
//! RFC 2119 keywords in this crate's comments, and the capitals are load-bearing.
//!
//! This crate reads and writes the bytes of the keeper's own files and tells a holder's command
//! line. It touches no disk and no process: a caller reads a file's bytes, hands them to a
//! `parse_` function, and writes what a `serialize_` function gives with
//! `agentspace_state::write_atomically` (state.md, Replacing a file). A library crate: it never
//! writes to stdout or stderr, and never panics on bad input.
//!
//! # The public API
//!
//! Every JSON file is UTF-8 without a byte order mark, compact JSON, then one LF, and a writer
//! checks its document against the file's schema (`schemas/state/`, embedded at build time, no
//! reference ever fetched) before it gives any bytes.
//!
//! | Item | What it is |
//! |---|---|
//! | `serialize_holder_record`, `parse_holder_record` | The holder record, `board-holder.json`. A writer is strict to the schema. A reader is the tolerant one of state.md: **a record that fails is no record** (`None`), and its alias has to match `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z` (gap FB-23). The record is `agentspace_keeper_api::HolderRecord`. |
//! | `LaunchRequest`, `serialize_launch_request`, `parse_launch_request` | `board-holder-launch.<hex>.json`, read strictly. |
//! | `LaunchResult`, `serialize_launch_result`, `parse_launch_result` | `board-holder-launch.<hex>.result.json`, `Started` or `NotStarted`, read strictly. |
//! | `Claim`, `serialize_claim`, `parse_claim` | The line of `board-holder-launch.<hex>.claim`: `launcher <pid>` or `abandoned`, then LF. |
//! | `serialize_keeper_pid`, `keeper_pid_digits`, `parse_keeper_pid` | The PID line of `board-holder-keeper.lock`, and a second keeper's reading of it (`KEEPER_PID_READ_BYTES`). |
//! | `HolderMatcher` | The pinned `agentspace_keeper_api::Matcher`: `holder_command`, `is_holder` and `self_check`, for the system's `<System32>` folder, plus `program` and `cmd_exe`. `split_command_line` is its splitter, as `CommandLineToArgvW` splits. |
//! | `holder_record_file`, `holder_record_tmp_file`, `holder_ssh_log_file`, `holder_ssh_log_rotated_file`, `project_of_holder_record_file`, `is_project_name` | Each `linux/arm64` host's holder files by D55's names (state.md, Another host's holder). They take `Option<&str>`, the host's project: `None` is the box, whose names are v0.06's. Each keeps its signature and its names from before v0.12. |
//! | `serialize_holder_record_keyed`, `parse_holder_record_keyed` | From v0.12, the holder record of a host keyed by `agentspace_hosts::HostKey`, its project and platform (D91): a writer writes `platform` last and only off `linux/arm64`, and a reader takes a record whose platform, `linux/arm64` when absent, is not the host's as no record (state.md, The holder record; pins P5, P14 and P16). `serialize_holder_record` and `parse_holder_record` are the same for a `linux/arm64` host, so a record written before v0.12 is read as `linux/arm64`'s. |
//! | `holder_record_file_keyed`, `holder_record_tmp_file_keyed`, `holder_ssh_log_file_keyed`, `holder_ssh_log_rotated_file_keyed`, `host_of_holder_record_file`, `host_of_holder_ssh_log_file`, `host_of_host_name` | From v0.12, each host's holder files by its host name, `<project>` or `<project>.<os>-<arch>` (state.md, A host's platform and names; pin P12), and the host a file's name or a host name gives (keeper.md, The hosts of a pass, item 3; pin P21). The box's names stay v0.06's. |
//! | `NotValid` | Why a writer refused a document, or a strict reader refused a file. |
//! | The file names | `HOLDER_RECORD_FILE`, `PASS_LOCK_FILE`, `KEEPER_LOCK_FILE`, `launch_request_file`, `claim_file`, `result_file`. |

#![forbid(unsafe_code)]

mod holder_record;
mod hosts;
mod json;
mod launch;
mod lock;
mod matcher;

pub use holder_record::{
    parse_holder_record, parse_holder_record_keyed, serialize_holder_record,
    serialize_holder_record_keyed,
};
pub use hosts::{
    holder_record_file, holder_record_file_keyed, holder_record_tmp_file,
    holder_record_tmp_file_keyed, holder_ssh_log_file, holder_ssh_log_file_keyed,
    holder_ssh_log_rotated_file, holder_ssh_log_rotated_file_keyed, host_of_holder_record_file,
    host_of_holder_ssh_log_file, host_of_host_name, is_project_name, project_of_holder_record_file,
};
pub use launch::{
    Claim, LaunchRequest, LaunchResult, parse_claim, parse_launch_request, parse_launch_result,
    serialize_claim, serialize_launch_request, serialize_launch_result,
};
pub use lock::{KEEPER_PID_READ_BYTES, keeper_pid_digits, parse_keeper_pid, serialize_keeper_pid};
pub use matcher::{HolderMatcher, split_command_line};

/// Why a document is not valid: the words of a refusal, naming where in the document and never
/// its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotValid(pub String);

/// The holder record's file name, in the state folder.
pub const HOLDER_RECORD_FILE: &str = "board-holder.json";
/// The pass lock's file name, in the state folder (state.md, The keeper's lock files).
pub const PASS_LOCK_FILE: &str = "board-holder.lock";
/// The keeper lock's file name, in the state folder.
pub const KEEPER_LOCK_FILE: &str = "board-holder-keeper.lock";

/// `board-holder-launch.<hex>.json`, a launch request's file name for one start's `<hex>`.
pub fn launch_request_file(hex: &str) -> String {
    format!("board-holder-launch.{hex}.json")
}

/// `board-holder-launch.<hex>.claim`.
pub fn claim_file(hex: &str) -> String {
    format!("board-holder-launch.{hex}.claim")
}

/// `board-holder-launch.<hex>.result.json`.
pub fn result_file(hex: &str) -> String {
    format!("board-holder-launch.{hex}.result.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_launch_files_are_named_by_one_hex() {
        assert_eq!(launch_request_file("ab12"), "board-holder-launch.ab12.json");
        assert_eq!(claim_file("ab12"), "board-holder-launch.ab12.claim");
        assert_eq!(result_file("ab12"), "board-holder-launch.ab12.result.json");
    }
}

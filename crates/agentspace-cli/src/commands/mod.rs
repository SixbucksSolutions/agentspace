//! The five lease commands. In this story every one of them answers `internal`; E0.05-T03-S02
//! (#0106) replaces that.

use std::path::Path;

use crate::{LeaseError, ParsedCommand};

/// Runs a parsed command line and returns the document to print.
///
/// `own_exe` is the program's own executable, as a full path, from the operating system.
pub fn run(cmd: ParsedCommand, own_exe: &Path) -> Result<serde_json::Value, LeaseError> {
    let _ = own_exe;
    Err(LeaseError::internal(format!(
        "agentspace lease {} failed: This build does not have the command yet.",
        cmd.sub.name()
    )))
}

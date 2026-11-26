//! The `windows/amd64` platform seam (docs/operator.md, The platform seam and Windows hosts;
//! D95 to D97, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The architect's skeleton, committed before v0.12-T03 so that the operator's front
//! (E0.12-T03-S02) can pick this platform while E0.12-T03-S06 builds it. The rows that are a
//! constant hold operator.md's value already; every row that builds a script answers
//! [`NOT_BUILT`] until S06 replaces it. Nothing here is called by a release build until S02 wires
//! it.

#![forbid(unsafe_code)]

use agentspace_operator_api::{
    Bootstrap, CallerFailure, Failure, HostPlatform, PatchMessages, SyncPlan,
};

/// The skeleton's answer for a row it does not build yet.
pub const NOT_BUILT: &str = "agentspace: windows/amd64 hosts are not built yet.";

/// The `windows/amd64` platform.
#[derive(Debug, Clone, Copy, Default)]
pub struct Windows;

/// The one instance the operator picks for a `windows/amd64` host.
pub static WINDOWS: Windows = Windows;

fn not_built() -> Failure {
    Failure {
        message: NOT_BUILT.to_string(),
        caller: CallerFailure::NotAvailable,
    }
}

impl HostPlatform for Windows {
    fn platform(&self) -> &'static str {
        "windows/amd64"
    }

    fn image_parameter(&self) -> Option<&'static str> {
        Some("/aws/service/ami-windows-latest/Windows_Server-2025-English-Full-Base")
    }

    fn new_stack_instance_type(&self) -> Option<&'static str> {
        Some("m8a.xlarge")
    }

    fn new_stack_extra_parameters(&self) -> &'static [&'static str] {
        &["RootVolumeGiB=30", "HostOs=windows"]
    }

    fn login_user(&self) -> &'static str {
        "agent"
    }

    fn ssm_document(&self) -> &'static str {
        "AWS-RunPowerShellScript"
    }

    fn remote_command(&self, _script: &str) -> Result<String, Failure> {
        Err(not_built())
    }

    fn bootstrap(&self) -> Result<Bootstrap, Failure> {
        Err(not_built())
    }

    fn sync_plan(&self) -> Result<SyncPlan, Failure> {
        Err(not_built())
    }

    fn patch_check_command(&self) -> Result<String, Failure> {
        Err(not_built())
    }

    fn patch_messages(&self) -> PatchMessages {
        PatchMessages {
            phase: "Patching Windows (Windows Update, restart if an update needs one)..."
                .to_string(),
            log_command: "cat C:/ProgramData/agentspace/patch.log".to_string(),
        }
    }

    fn horizon_command(&self, _horizon: u64) -> Result<String, Failure> {
        Err(not_built())
    }

    fn grow_script(&self) -> Result<String, Failure> {
        Err(not_built())
    }

    fn push_zone(&self, _windows_zone_id: &str, _iana: &str) -> Result<String, Failure> {
        Err(not_built())
    }

    fn push_script(&self, _zone: &str) -> Result<String, Failure> {
        Err(not_built())
    }
}

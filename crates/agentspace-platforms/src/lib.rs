//! The platform seam's implementations for `linux/arm64` and `linux/amd64`, and [`for_platform`],
//! which picks a host's (docs/operator.md, The platform seam; D94, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The architect's skeleton, committed before v0.12-T03 (D101's change before T03): every caller
//! that needs a platform's rows (the operator's front, sync, the guard, the push and the
//! ProxyCommand) reaches them through [`for_platform`], and none depends on another's crate for
//! them. The rows that are a constant hold operator.md's value already; every row that builds a
//! script answers [`NOT_BUILT`] until E0.12-T03-S03 (#0256) builds it, carrying today's Linux steps
//! into it unchanged.

#![forbid(unsafe_code)]

use agentspace_operator_api::{
    Bootstrap, CallerFailure, Failure, HostPlatform, PatchMessages, SyncPlan,
};

/// The skeleton's answer for a Linux row it does not build yet.
pub const NOT_BUILT: &str = "agentspace: this platform's step is not built yet.";

/// A Linux platform: `linux/arm64` or `linux/amd64`. The two share every row but the image, a new
/// stack's instance type and the localswim CLI's `GOARCH` (D94; pin P41 of v0.12-T01).
#[derive(Debug, Clone, Copy)]
pub struct Linux {
    platform: &'static str,
    image_parameter: Option<&'static str>,
    new_stack_instance_type: Option<&'static str>,
}

/// `linux/arm64`: the template's own image and type, as before v0.12.
pub static LINUX_ARM64: Linux = Linux {
    platform: "linux/arm64",
    image_parameter: None,
    new_stack_instance_type: None,
};

/// `linux/amd64`: Canonical's public parameter for Ubuntu 26.04 LTS on `amd64`, and `m8a.xlarge`.
pub static LINUX_AMD64: Linux = Linux {
    platform: "linux/amd64",
    image_parameter: Some(
        "/aws/service/canonical/ubuntu/server/26.04/stable/current/amd64/hvm/ebs-gp3/ami-id",
    ),
    new_stack_instance_type: Some("m8a.xlarge"),
};

/// The seam for a normalized platform, or `None` for one agentspace does not support.
pub fn for_platform(platform: &str) -> Option<&'static dyn HostPlatform> {
    match platform {
        "linux/arm64" => Some(&LINUX_ARM64),
        "linux/amd64" => Some(&LINUX_AMD64),
        "windows/amd64" => Some(&agentspace_windows::WINDOWS),
        _ => None,
    }
}

fn not_built() -> Failure {
    Failure {
        message: NOT_BUILT.to_string(),
        caller: CallerFailure::NotAvailable,
    }
}

impl HostPlatform for Linux {
    fn platform(&self) -> &'static str {
        self.platform
    }

    fn image_parameter(&self) -> Option<&'static str> {
        self.image_parameter
    }

    fn new_stack_instance_type(&self) -> Option<&'static str> {
        self.new_stack_instance_type
    }

    fn new_stack_extra_parameters(&self) -> &'static [&'static str] {
        &[]
    }

    fn login_user(&self) -> &'static str {
        "ubuntu"
    }

    fn ssm_document(&self) -> &'static str {
        "AWS-RunShellScript"
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
            phase: "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...".to_string(),
            log_command: "journalctl -u agentspace-patch".to_string(),
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

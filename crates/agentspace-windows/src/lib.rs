//! The `windows/amd64` platform seam (docs/operator.md, The platform seam and Windows hosts;
//! D95 to D97, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Each method is one row of operator.md's seam table, `windows/amd64`'s column, and returns data
//! or text only: the operator and `agentspace-sync` run every program and every AWS call, and
//! nothing here starts a program, reads a file at run time, writes anything or panics. The host
//! agent's files, `box/windows/` but its `tests` folder, are carried in the crate by `build.rs`,
//! and the bootstrap by `include_bytes!`, so the files E0.12-T03-S07 writes arrive by merge.

#![forbid(unsafe_code)]

use agentspace_operator_api::{
    Bootstrap, Failure, HostPlatform, LocalswimTarget, PatchMessages, ScriptDelivery, SyncPlan,
};

mod encoded;
mod scripts;
mod zone;

pub use encoded::{MAX_LINE, PREFIX, TOO_LONG};
pub use scripts::PROGRAM_DATA;
pub use zone::{NO_ZONE, is_windows_zone_id};

include!(concat!(env!("OUT_DIR"), "/box_windows.rs"));

/// `box/windows/bootstrap.ps1`, as it is in the checkout.
const BOOTSTRAP: &[u8] = include_bytes!("../../../box/windows/bootstrap.ps1");

/// The bootstrap's copy in the login user's home, as `scp` names it.
pub const BOOTSTRAP_COPY: &str = ".agentspace-bootstrap.ps1";

/// The staging folder in the login user's home, as `scp` and the remote commands name it.
pub const STAGING: &str = ".agentspace-system";

/// Where `CLAUDE.md` is copied to, in the login user's home, as `scp` names it.
pub const CLAUDE_MD: &str = ".claude/CLAUDE.md";

/// The `windows/amd64` platform.
#[derive(Debug, Clone, Copy, Default)]
pub struct Windows;

/// The one instance the operator picks for a `windows/amd64` host.
pub static WINDOWS: Windows = Windows;

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

    fn remote_command(&self, script: &str) -> Result<String, Failure> {
        encoded::remote_command(script)
    }

    fn bootstrap(&self) -> Result<Bootstrap, Failure> {
        Ok(Bootstrap {
            script: BOOTSTRAP,
            delivery: ScriptDelivery::CopyThenRun {
                remote_name: BOOTSTRAP_COPY.to_string(),
                remote_command: encoded::remote_command(&scripts::bootstrap_run())?,
            },
        })
    }

    fn sync_plan(&self) -> Result<SyncPlan, Failure> {
        Ok(SyncPlan {
            files: BOX_WINDOWS,
            staging: STAGING.to_string(),
            install_command: encoded::remote_command(&scripts::install())?,
            clear_command: Some(encoded::remote_command(scripts::CLEAR_STAGING)?),
            // Row 5: make sure `<home>\.claude` exists (pin P9 of v0.12-T03).
            claude_folder_command: encoded::remote_command(scripts::MAKE_CLAUDE_FOLDER)?,
            claude_md_path: CLAUDE_MD.to_string(),
            localswim: LocalswimTarget {
                goos: "windows",
                goarch: "amd64",
                file_name: "localswim-windows-amd64.exe",
            },
            // Row 7's target in the login user's home, which row 8 installs from (pin P9 of
            // v0.12-T03).
            localswim_staging: ".agentspace-localswim.exe".to_string(),
            localswim_install_command: encoded::remote_command(scripts::INSTALL_LOCALSWIM)?,
        })
    }

    fn patch_check_command(&self) -> Result<String, Failure> {
        Ok(scripts::PATCH_CHECK.to_string())
    }

    fn patch_messages(&self) -> PatchMessages {
        PatchMessages {
            phase: "Patching Windows (Windows Update, restart if an update needs one)..."
                .to_string(),
            log_command: "cat C:/ProgramData/agentspace/patch.log".to_string(),
        }
    }

    fn horizon_command(&self, horizon: u64) -> Result<String, Failure> {
        Ok(scripts::horizon(horizon))
    }

    fn grow_script(&self) -> Result<String, Failure> {
        Ok(scripts::GROW.to_string())
    }

    fn push_zone(&self, windows_zone_id: &str, _iana: &str) -> Result<String, Failure> {
        zone::push_zone(windows_zone_id)
    }

    fn push_script(&self, zone: &str) -> Result<String, Failure> {
        // An empty zone is no zone (What the box does, step 8). Any other is checked again here,
        // since it goes inside the script, and a zone outside the pattern is refused rather than
        // quoted.
        if !zone.is_empty() && !is_windows_zone_id(zone) {
            return Err(zone::no_zone());
        }
        Ok(scripts::push(zone))
    }
}

#[cfg(test)]
mod tests;

//! The platform seam (docs/operator.md, The platform seam; D94, D96, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Pinned by the architect before v0.12-T03, from operator.md's seam table as v0.12-T01 and T02
//! left it (D101). Each method is one row of that table: its input is the row's input, and what it
//! returns is the row's output. A method returns data or text only: the operator and
//! `agentspace-sync` run every program and every AWS call, and a platform never does. The two
//! Linux platforms' implementation is `agentspace-sync`'s (E0.12-T03-S03), and `windows/amd64`'s
//! is `agentspace-windows`' (E0.12-T03-S06); the operator's front picks one by the host's platform
//! (E0.12-T03-S02). A change to this trait is the architect's, on a dev lead's pin.

use crate::Failure;

/// How one script reaches a host and runs there (operator.md, The platform seam, the bootstrap
/// row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptDelivery {
    /// Sent on one ssh's stdin, with `remote_command` as that ssh's remote command (both Linux
    /// platforms: `box/bootstrap.sh`, run by `bash` as the login user).
    Stdin {
        /// The remote command, the last argument of the ssh.
        remote_command: String,
    },
    /// Copied with one `scp` into the login user's home as `remote_name`, then run by one ssh
    /// whose remote command is `remote_command`, which also removes the copy (`windows/amd64`:
    /// `box/windows/bootstrap.ps1`, run under Windows PowerShell 5.1 with `-File`).
    CopyThenRun {
        /// The copy's name in the login user's home.
        remote_name: String,
        /// The remote command that runs the copy and removes it.
        remote_command: String,
    },
}

/// A platform's bootstrap (operator.md, The platform seam, the bootstrap row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bootstrap {
    /// The script's bytes, carried in the program.
    pub script: &'static [u8],
    /// How it reaches the host.
    pub delivery: ScriptDelivery,
}

/// Where a platform's localswim CLI is built for and installed (operator.md, The platform seam,
/// the sync row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalswimTarget {
    /// `GOOS` for the build.
    pub goos: &'static str,
    /// `GOARCH` for the build.
    pub goarch: &'static str,
    /// The built file's name in the command's folder, such as `localswim-linux-arm64` or
    /// `localswim-windows-amd64.exe`.
    pub file_name: &'static str,
}

/// A platform's sync (operator.md, The platform seam, the sync row; sync on a Windows host).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    /// The host agent's files, each as its name under the staging folder and its bytes: the files
    /// of `box/system` on Linux, of `box/windows/` on Windows, carried in the program.
    pub files: &'static [(&'static str, &'static [u8])],
    /// The staging folder, as `scp` and the remote commands name it: `/tmp/agentspace-system` on
    /// Linux, `.agentspace-system` in the login user's home on Windows.
    pub staging: String,
    /// The remote command that installs from the staging folder, in the platform's remote form
    /// ([`HostPlatform::remote_command`]).
    pub install_command: String,
    /// The remote command that clears the staging folder before the copy, in the same form, or
    /// `None` where the platform's install needs none.
    pub clear_command: Option<String>,
    /// Where `CLAUDE.md` is copied to, as `scp` names it.
    pub claude_md_path: String,
    /// The localswim CLI's build and install.
    pub localswim: LocalswimTarget,
    /// The remote command that installs the copied localswim CLI, in the platform's remote form.
    pub localswim_install_command: String,
}

/// A platform's patch messages (operator.md, The platform seam, the patch messages row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchMessages {
    /// The phase message of a `pending` answer, such as
    /// `Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...`.
    pub phase: String,
    /// The command named in the deadline's and a failed run's messages, such as
    /// `journalctl -u agentspace-patch`.
    pub log_command: String,
}

/// One platform's rows of the seam (operator.md, The platform seam). Every method is pure: it
/// reads only its arguments and the platform's own constants, writes nothing, starts nothing, and
/// never panics. A method that can fail returns the [`Failure`] the row names, or, where the row
/// names none, the one the same step gives on Linux.
pub trait HostPlatform: Sync {
    /// The normalized platform this serves: `linux/arm64`, `linux/amd64` or `windows/amd64`.
    fn platform(&self) -> &'static str;

    /// Its image parameter: the public Systems Manager parameter whose value is the `ImageId` a
    /// stack that `deploy` creates is given, read once at creation; `None` passes no `ImageId`
    /// (`linux/arm64`, whose template default applies).
    fn image_parameter(&self) -> Option<&'static str>;

    /// A new stack's instance type when `deploy` is given no `-InstanceType`; `None` for the
    /// template's default (`linux/arm64`).
    fn new_stack_instance_type(&self) -> Option<&'static str>;

    /// A new stack's other creation parameters, each one `Name=Value` argument, in order, after
    /// `ImageId=<image>` and before `InstanceType` (`windows/amd64`: `RootVolumeGiB=30`, then
    /// `HostOs=windows`). Empty for both Linux platforms.
    fn new_stack_extra_parameters(&self) -> &'static [&'static str];

    /// Its login user: the operator alias's `User` and the operator certificate's principal
    /// (`ubuntu`, `ubuntu`, `agent`; D95). Never stored in the registry or the alias.
    fn login_user(&self) -> &'static str;

    /// Its Session Manager document, for `up`'s patch check, `sync`'s patch wait and the guard's
    /// horizon (`AWS-RunShellScript` or `AWS-RunPowerShellScript`). On `windows/amd64` every Run
    /// Command is sent as a JSON value of `--parameters`, never the shorthand (pin P17 and P18 of
    /// v0.12-T02), which the caller does for every platform whose document this names.
    fn ssm_document(&self) -> &'static str;

    /// Its remote command form: the one ssh argument that runs `script`, which is the operator's
    /// own text for the step (a POSIX shell script on Linux; a PowerShell script on Windows,
    /// returned as `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass
    /// -EncodedCommand <b64>`, at most 8,191 characters, with no `=`). A script too long for the
    /// form fails with the row's message.
    fn remote_command(&self, script: &str) -> Result<String, Failure>;

    /// Its bootstrap: the script and how it reaches the host.
    fn bootstrap(&self) -> Result<Bootstrap, Failure>;

    /// Its sync: the files, the staging folder and the remote steps around the copies.
    fn sync_plan(&self) -> Result<SyncPlan, Failure>;

    /// Its patch check's question: the `commands` value that `up`'s wait and `sync`'s patch wait
    /// send through [`HostPlatform::ssm_document`].
    fn patch_check_command(&self) -> Result<String, Failure>;

    /// Its patch messages.
    fn patch_messages(&self) -> PatchMessages;

    /// Its horizon command: the `commands` value the guard sends to write `horizon`, the Unix time
    /// in seconds, as the host's lease horizon. On `windows/amd64` it writes the digits and an
    /// LF, or the digits alone, never with a CR (pin P12 of v0.12-T02).
    fn horizon_command(&self, horizon: u64) -> Result<String, Failure>;

    /// Its online grow: the script of `grow`'s remote step, before
    /// [`HostPlatform::remote_command`] wraps it.
    fn grow_script(&self) -> Result<String, Failure>;

    /// Its push's zone: the zone the host is given, from the laptop's own Windows zone ID and the
    /// IANA name `agentspace-zone` converts it to. Both Linux platforms return `iana`;
    /// `windows/amd64` returns `windows_zone_id`, checked by its own pattern, or fails.
    fn push_zone(&self, windows_zone_id: &str, iana: &str) -> Result<String, Failure>;

    /// Its push's remote half: the script that, given the token on its stdin, writes the board's
    /// credential and sets `zone`, before [`HostPlatform::remote_command`] wraps it.
    fn push_script(&self, zone: &str) -> Result<String, Failure>;
}

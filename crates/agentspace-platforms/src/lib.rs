//! The platform seam's implementations for `linux/arm64` and `linux/amd64`, and [`for_platform`],
//! which picks a host's (docs/operator.md, The platform seam; D94, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The architect's skeleton, committed before v0.12-T03 (D101's change before T03), built by
//! E0.12-T03-S03 (#0256): every caller that needs a platform's rows (the operator's front, sync,
//! the guard, the push and the ProxyCommand) reaches them through [`for_platform`], and none
//! depends on another's crate for them.
//!
//! - **Every Linux row MUST produce exactly the text or bytes today's step produces**, so that
//!   what reaches ssh or Run Command is unchanged byte for byte when a caller switches to the seam
//!   (operator.md, The platform seam: "Every step this page states for 'today's box' ... is both
//!   Linux columns' step"). Each text below is carried from today's producer unchanged, and named
//!   with it; the unit tests compare each row with a literal fixture copied from that producer, for
//!   both Linux platforms.
//! - **The two Linux platforms MUST differ only in the image, a new stack's instance type and the
//!   localswim CLI's target** (D94; pin P41 of v0.12-T01). A test shows every other row equal.
//! - **The remote command form on Linux is the operator's own POSIX shell text** (operator.md,
//!   What the laptop sends), so [`HostPlatform::remote_command`] gives the script back unchanged,
//!   and the remote command of every row is the row's text itself.
//! - **The bootstrap and `box\system` are the repository's own, embedded at build time byte for
//!   byte**, as `agentspace-sync` embeds them today (D37's addendum, D46).
//! - **A row never prints, starts a program, touches a file or panics**: it returns data or text.

#![forbid(unsafe_code)]

use agentspace_operator_api::{
    Bootstrap, Failure, HostPlatform, LocalswimTarget, PatchMessages, ScriptDelivery, SyncPlan,
};

mod embedded;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod tests;

/// The skeleton's answer for a step a platform does not build yet. No Linux row gives it; the
/// entry points of `agentspace-sync` that take a host key give it for a platform other than
/// `linux/arm64` until E0.12-T03-S03's second part builds them.
pub const NOT_BUILT: &str = "agentspace: this platform's step is not built yet.";

/// A Linux platform: `linux/arm64` or `linux/amd64`. The two share every row but the image, a new
/// stack's instance type and the localswim CLI's `GOARCH` (D94; pin P41 of v0.12-T01).
#[derive(Debug, Clone, Copy)]
pub struct Linux {
    platform: &'static str,
    image_parameter: Option<&'static str>,
    new_stack_instance_type: Option<&'static str>,
    localswim: LocalswimTarget,
}

/// `linux/arm64`: the template's own image and type, as before v0.12.
pub static LINUX_ARM64: Linux = Linux {
    platform: "linux/arm64",
    image_parameter: None,
    new_stack_instance_type: None,
    localswim: LocalswimTarget {
        goos: "linux",
        goarch: "arm64",
        file_name: "localswim-linux-arm64",
    },
};

/// `linux/amd64`: Canonical's public parameter for Ubuntu 26.04 LTS on `amd64`, and `m8a.xlarge`.
pub static LINUX_AMD64: Linux = Linux {
    platform: "linux/amd64",
    image_parameter: Some(
        "/aws/service/canonical/ubuntu/server/26.04/stable/current/amd64/hvm/ebs-gp3/ami-id",
    ),
    new_stack_instance_type: Some("m8a.xlarge"),
    localswim: LocalswimTarget {
        goos: "linux",
        goarch: "amd64",
        file_name: "localswim-linux-amd64",
    },
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

/// The bootstrap's remote command: bash reads the script from stdin, to its end, and exits with
/// its status. Today's `agentspace-sync` `bootstrap.rs`, `REMOTE_COMMAND`.
const BOOTSTRAP_REMOTE_COMMAND: &str = "bash -s";

/// The staging folder. Today's `agentspace-sync` `steps.rs`, `STAGING`.
const STAGING: &str = "/tmp/agentspace-system";

/// Sync's row 1: remove the staging folder and everything in it, as root; exit 0 only when it is
/// gone. Today's `agentspace-sync` `steps.rs`, `REMOVE_STAGING`.
const REMOVE_STAGING: &str =
    "sudo -n rm -rf -- /tmp/agentspace-system && test ! -e /tmp/agentspace-system";

/// Sync's row 4: as root, the staged install with bash, then, only if it exited 0, the staging
/// folder's removal. Today's `agentspace-sync` `steps.rs`, `INSTALL`.
const INSTALL: &str = "sudo -n bash /tmp/agentspace-system/install.sh \
&& sudo -n rm -rf -- /tmp/agentspace-system";

/// Where `CLAUDE.md` is copied to, as scp names it after `<alias>:`, relative to the login user's
/// home. Today's `agentspace-sync` `syncing.rs`, step 6's target, `{alias}:.claude/CLAUDE.md`.
const CLAUDE_MD_PATH: &str = ".claude/CLAUDE.md";

/// Sync's row 8: as root, the staged localswim CLI installed at
/// `/usr/local/lib/localswim/localswim`, then its staged copy removed. Today's `agentspace-sync`
/// `steps.rs`, `INSTALL_LOCALSWIM`.
const INSTALL_LOCALSWIM: &str = "sudo -n install -d -o root -g root -m 0755 \
/usr/local/lib/localswim \
&& sudo -n install -o root -g root -m 0755 /tmp/localswim /usr/local/lib/localswim/localswim \
&& sudo -n rm -f -- /tmp/localswim";

/// The patch question, run as root by `AWS-RunShellScript`. Today's `agentspace-box` `wait.rs`,
/// `PATCH_QUESTION`, which `up`'s wait and `sync`'s patch wait both send (operator.md, What up
/// waits for).
const PATCH_QUESTION: &str = "if test -e /run/agentspace/patched; then echo patched; \
elif systemctl is-failed --quiet agentspace-patch.service; then \
if systemctl list-jobs --no-legend agentspace-patch.service | grep -qw -e start -e restart; \
then echo pending; else echo failed; fi; \
elif systemctl is-enabled --quiet agentspace-patch.service; then echo pending; \
else echo unknown; fi";

/// `grow`'s remote step: the root partition and its filesystem grown online. Today's
/// `agentspace-operator` `commands.rs`, `GROW_REMOTE_COMMAND` (operator.md, grow, step 3).
const GROW_SCRIPT: &str = "sudo -n sh -c 'dev=$(findmnt -n -o SOURCE /); \
disk=/dev/$(lsblk -n -o PKNAME $dev); part=$(cat /sys/class/block/${dev##*/}/partition); \
growpart $disk $part; resize2fs $dev && df -h /'";

/// The horizon command for `horizon` Unix seconds (operator.md, guard, step 7). Today's
/// `agentspace-box` `guard.rs`, `horizon_command`, whose `i64` the seam's `u64` replaces: a
/// negative horizon is the caller's refusal, as `send_horizon`'s is today, and the digits of every
/// horizon both types hold are the same.
fn horizon_command(horizon: u64) -> String {
    format!(
        "umask 022; mkdir -p -m 755 /run/agentspace \
&& echo {horizon} > /run/agentspace/lease-horizon.new \
&& chmod 644 /run/agentspace/lease-horizon.new \
&& mv -f /run/agentspace/lease-horizon.new /run/agentspace/lease-horizon"
    )
}

/// The push's remote half in the login user's POSIX shell, naming `zone` when it passes the zone
/// check, and the command for the empty zone otherwise. Today's `agentspace-push` `boxside.rs`,
/// `remote_command(zone)` (operator.md, What the box does; The time zone).
fn push_script(zone: &str) -> String {
    let mut cmd = String::from(
        "umask 077; export LC_ALL=C; t=; trap 'rm -f -- $t' EXIT; trap 'exit 1' HUP INT TERM PIPE; \
         mkdir -p ~/.config/localswim && cd ~/.config/localswim || exit 1; \
         t=$(mktemp bot.env.XXXXXXXXXX) || exit 1; \
         timeout 30 head -n 2 | tr -d '\\r' | sed '1s/^\\xEF\\xBB\\xBF//' | sed '/^$/d' >$t || exit 1; \
         test $(wc -l <$t) -eq 2 || exit 1; \
         grep -q '^LOCALSWIM_URL=.' $t || exit 1; \
         grep -q '^LOCALSWIM_TOKEN=.' $t || exit 1; \
         mv -f -- $t bot.env || exit 1; t=; \
         sha256sum <bot.env || exit 1;",
    );
    // The last guard: only a zone that passes the zone check is ever put on the host's command
    // line (operator.md, The time zone), whoever calls this.
    if !agentspace_zone::is_valid_zone(zone) {
        cmd.push_str(" exit 0");
    } else {
        cmd.push_str(&format!(
            " if timeout -k 1 5 sudo -n timedatectl set-timezone {zone} </dev/null >/dev/null 2>&1; \
             then echo zone {zone}; else echo zone-failed; fi; exit 0"
        ));
    }
    cmd
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

    /// The operator's own POSIX shell text, unchanged: on Linux the script is the remote command.
    fn remote_command(&self, script: &str) -> Result<String, Failure> {
        Ok(script.to_string())
    }

    fn bootstrap(&self) -> Result<Bootstrap, Failure> {
        Ok(Bootstrap {
            script: embedded::BOOTSTRAP,
            delivery: ScriptDelivery::Stdin {
                remote_command: self.remote_command(BOOTSTRAP_REMOTE_COMMAND)?,
            },
        })
    }

    fn sync_plan(&self) -> Result<SyncPlan, Failure> {
        Ok(SyncPlan {
            files: embedded::BOX_SYSTEM,
            staging: STAGING.to_string(),
            install_command: self.remote_command(INSTALL)?,
            clear_command: Some(self.remote_command(REMOVE_STAGING)?),
            claude_md_path: CLAUDE_MD_PATH.to_string(),
            localswim: self.localswim,
            localswim_install_command: self.remote_command(INSTALL_LOCALSWIM)?,
        })
    }

    fn patch_check_command(&self) -> Result<String, Failure> {
        Ok(PATCH_QUESTION.to_string())
    }

    fn patch_messages(&self) -> PatchMessages {
        PatchMessages {
            phase: "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...".to_string(),
            log_command: "journalctl -u agentspace-patch".to_string(),
        }
    }

    fn horizon_command(&self, horizon: u64) -> Result<String, Failure> {
        Ok(horizon_command(horizon))
    }

    fn grow_script(&self) -> Result<String, Failure> {
        Ok(GROW_SCRIPT.to_string())
    }

    /// Both Linux platforms give the host the IANA name `agentspace-zone` converted the laptop's
    /// zone to; the Windows zone ID is not used.
    fn push_zone(&self, _windows_zone_id: &str, iana: &str) -> Result<String, Failure> {
        Ok(iana.to_string())
    }

    fn push_script(&self, zone: &str) -> Result<String, Failure> {
        Ok(push_script(zone))
    }
}

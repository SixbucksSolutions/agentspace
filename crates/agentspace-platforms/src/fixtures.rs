//! The fixtures: today's Linux steps as literals, each copied byte for byte from what today's
//! producer gives at `main` 5d88698, before any caller switched to the seam (E0.12-T03-S03, part 1;
//! the dev lead's pin P1).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Each literal is the producer's resolved value, written out whole on one line, never read from
//! the producer's file. This file MUST stay free of any `use` and any reference to the rest of the
//! crate, so a one-off check outside the workspace can include it as a module and compare each
//! literal with the producer's own output.

/// `agentspace-sync` `bootstrap.rs`, `REMOTE_COMMAND`.
pub(crate) const BOOTSTRAP_REMOTE_COMMAND: &str = "bash -s";

/// `agentspace-sync` `steps.rs`, `STAGING`.
pub(crate) const STAGING: &str = "/tmp/agentspace-system";

/// `agentspace-sync` `steps.rs`, `REMOVE_STAGING`.
pub(crate) const REMOVE_STAGING: &str =
    r"sudo -n rm -rf -- /tmp/agentspace-system && test ! -e /tmp/agentspace-system";

/// `agentspace-sync` `steps.rs`, `INSTALL`.
pub(crate) const INSTALL: &str =
    r"sudo -n bash /tmp/agentspace-system/install.sh && sudo -n rm -rf -- /tmp/agentspace-system";

/// `agentspace-sync` `steps.rs`, `MAKE_CLAUDE_FOLDER` (pin P9 of v0.12-T03).
pub(crate) const MAKE_CLAUDE_FOLDER: &str = "mkdir -p -- ~/.claude";

/// `agentspace-sync` `syncing.rs`, step 6's scp target `{alias}:.claude/CLAUDE.md`, after the
/// alias and its colon.
pub(crate) const CLAUDE_MD_PATH: &str = ".claude/CLAUDE.md";

/// `agentspace-sync` `steps.rs`, `LOCALSWIM_STAGING`, step 7's scp target after the alias and its
/// colon (pin P9 of v0.12-T03).
pub(crate) const LOCALSWIM_STAGING: &str = "/tmp/localswim";

/// `agentspace-sync` `steps.rs`, `INSTALL_LOCALSWIM`.
pub(crate) const INSTALL_LOCALSWIM: &str = r"sudo -n install -d -o root -g root -m 0755 /usr/local/lib/localswim && sudo -n install -o root -g root -m 0755 /tmp/localswim /usr/local/lib/localswim/localswim && sudo -n rm -f -- /tmp/localswim";

/// `agentspace-sync` `go.rs`, the build's `GOOS` and `GOARCH`, and `files.rs`, `localswim_build`'s
/// file name, today's for `linux/arm64`.
pub(crate) const LOCALSWIM_ARM64: (&str, &str, &str) = ("linux", "arm64", "localswim-linux-arm64");

/// `agentspace-box` `wait.rs`, `PATCH_QUESTION`.
pub(crate) const PATCH_QUESTION: &str = r"if test -e /run/agentspace/patched; then echo patched; elif systemctl is-failed --quiet agentspace-patch.service; then if systemctl list-jobs --no-legend agentspace-patch.service | grep -qw -e start -e restart; then echo pending; else echo failed; fi; elif systemctl is-enabled --quiet agentspace-patch.service; then echo pending; else echo unknown; fi";

/// `agentspace-box` `wait.rs`, `PATCHING`, and `agentspace-sync` `patch_wait.rs`, `PATCHING`.
pub(crate) const PATCH_PHASE: &str =
    "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";

/// `agentspace-box` `wait.rs` and `agentspace-sync` `patch_wait.rs`: the command their deadline's
/// and failed run's messages name after `ssh <alias> `.
pub(crate) const PATCH_LOG_COMMAND: &str = "journalctl -u agentspace-patch";

/// `agentspace-box` `guard.rs`, `horizon_command(horizon)`, for each horizon here.
pub(crate) const HORIZONS: &[(u64, &str)] = &[
    (
        0,
        r"umask 022; mkdir -p -m 755 /run/agentspace && echo 0 > /run/agentspace/lease-horizon.new && chmod 644 /run/agentspace/lease-horizon.new && mv -f /run/agentspace/lease-horizon.new /run/agentspace/lease-horizon",
    ),
    (
        1764201600,
        r"umask 022; mkdir -p -m 755 /run/agentspace && echo 1764201600 > /run/agentspace/lease-horizon.new && chmod 644 /run/agentspace/lease-horizon.new && mv -f /run/agentspace/lease-horizon.new /run/agentspace/lease-horizon",
    ),
    (
        9223372036854775807,
        r"umask 022; mkdir -p -m 755 /run/agentspace && echo 9223372036854775807 > /run/agentspace/lease-horizon.new && chmod 644 /run/agentspace/lease-horizon.new && mv -f /run/agentspace/lease-horizon.new /run/agentspace/lease-horizon",
    ),
];

/// `agentspace-operator` `commands.rs`, `GROW_REMOTE_COMMAND`.
pub(crate) const GROW_SCRIPT: &str = r"sudo -n sh -c 'dev=$(findmnt -n -o SOURCE /); disk=/dev/$(lsblk -n -o PKNAME $dev); part=$(cat /sys/class/block/${dev##*/}/partition); growpart $disk $part; resize2fs $dev && df -h /'";

/// `agentspace-push` `boxside.rs`, `remote_command(zone)`, for each zone here: the empty zone, two
/// zones that pass the zone check, and two that fail it, which give the empty zone's command.
pub(crate) const PUSH_SCRIPTS: &[(&str, &str)] = &[
    (
        "",
        r"umask 077; export LC_ALL=C; t=; trap 'rm -f -- $t' EXIT; trap 'exit 1' HUP INT TERM PIPE; mkdir -p ~/.config/localswim && cd ~/.config/localswim || exit 1; t=$(mktemp bot.env.XXXXXXXXXX) || exit 1; timeout 30 head -n 2 | tr -d '\r' | sed '1s/^\xEF\xBB\xBF//' | sed '/^$/d' >$t || exit 1; test $(wc -l <$t) -eq 2 || exit 1; grep -q '^LOCALSWIM_URL=.' $t || exit 1; grep -q '^LOCALSWIM_TOKEN=.' $t || exit 1; mv -f -- $t bot.env || exit 1; t=; sha256sum <bot.env || exit 1; exit 0",
    ),
    (
        "America/New_York",
        r"umask 077; export LC_ALL=C; t=; trap 'rm -f -- $t' EXIT; trap 'exit 1' HUP INT TERM PIPE; mkdir -p ~/.config/localswim && cd ~/.config/localswim || exit 1; t=$(mktemp bot.env.XXXXXXXXXX) || exit 1; timeout 30 head -n 2 | tr -d '\r' | sed '1s/^\xEF\xBB\xBF//' | sed '/^$/d' >$t || exit 1; test $(wc -l <$t) -eq 2 || exit 1; grep -q '^LOCALSWIM_URL=.' $t || exit 1; grep -q '^LOCALSWIM_TOKEN=.' $t || exit 1; mv -f -- $t bot.env || exit 1; t=; sha256sum <bot.env || exit 1; if timeout -k 1 5 sudo -n timedatectl set-timezone America/New_York </dev/null >/dev/null 2>&1; then echo zone America/New_York; else echo zone-failed; fi; exit 0",
    ),
    (
        "Etc/GMT+5",
        r"umask 077; export LC_ALL=C; t=; trap 'rm -f -- $t' EXIT; trap 'exit 1' HUP INT TERM PIPE; mkdir -p ~/.config/localswim && cd ~/.config/localswim || exit 1; t=$(mktemp bot.env.XXXXXXXXXX) || exit 1; timeout 30 head -n 2 | tr -d '\r' | sed '1s/^\xEF\xBB\xBF//' | sed '/^$/d' >$t || exit 1; test $(wc -l <$t) -eq 2 || exit 1; grep -q '^LOCALSWIM_URL=.' $t || exit 1; grep -q '^LOCALSWIM_TOKEN=.' $t || exit 1; mv -f -- $t bot.env || exit 1; t=; sha256sum <bot.env || exit 1; if timeout -k 1 5 sudo -n timedatectl set-timezone Etc/GMT+5 </dev/null >/dev/null 2>&1; then echo zone Etc/GMT+5; else echo zone-failed; fi; exit 0",
    ),
    (
        "UTC; reboot",
        r"umask 077; export LC_ALL=C; t=; trap 'rm -f -- $t' EXIT; trap 'exit 1' HUP INT TERM PIPE; mkdir -p ~/.config/localswim && cd ~/.config/localswim || exit 1; t=$(mktemp bot.env.XXXXXXXXXX) || exit 1; timeout 30 head -n 2 | tr -d '\r' | sed '1s/^\xEF\xBB\xBF//' | sed '/^$/d' >$t || exit 1; test $(wc -l <$t) -eq 2 || exit 1; grep -q '^LOCALSWIM_URL=.' $t || exit 1; grep -q '^LOCALSWIM_TOKEN=.' $t || exit 1; mv -f -- $t bot.env || exit 1; t=; sha256sum <bot.env || exit 1; exit 0",
    ),
    (
        "-UTC",
        r"umask 077; export LC_ALL=C; t=; trap 'rm -f -- $t' EXIT; trap 'exit 1' HUP INT TERM PIPE; mkdir -p ~/.config/localswim && cd ~/.config/localswim || exit 1; t=$(mktemp bot.env.XXXXXXXXXX) || exit 1; timeout 30 head -n 2 | tr -d '\r' | sed '1s/^\xEF\xBB\xBF//' | sed '/^$/d' >$t || exit 1; test $(wc -l <$t) -eq 2 || exit 1; grep -q '^LOCALSWIM_URL=.' $t || exit 1; grep -q '^LOCALSWIM_TOKEN=.' $t || exit 1; mv -f -- $t bot.env || exit 1; t=; sha256sum <bot.env || exit 1; exit 0",
    ),
];

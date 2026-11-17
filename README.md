# agentspace

A Graviton EC2 instance in us-west-2 for running coding agents off the laptop. It is stopped
most of the time: connecting starts it, and it stops itself when nobody is using it. Lead agents
are told to "run this in agentspace".

**How it was built, what Terry set up, and how agents on it reach the laptop's localswim board:
[docs/how-it-was-built.md](docs/how-it-was-built.md).**

**Where it is going:** a lease API that lets projects ask for compute without knowing what hosts it.
The plan is in [docs/roadmap.md](docs/roadmap.md), and the reasoning in
[docs/decisions.md](docs/decisions.md). The work is on the `agentspace` localswim board.

| Piece | What |
|---|---|
| Instance | m9g.xlarge (4 Graviton cores, 16 GiB), Ubuntu 26.04 LTS arm64 |
| Disk | 8 GiB gp3, encrypted; grown on demand, never shrunk |
| Access | Session Manager only: no inbound ports, no key pair, no Elastic IP |
| Patching | Every boot: `apt-get dist-upgrade` (phased updates included) and `snap refresh`, then one reboot if the kernel or another package needs it; then the newest Node LTS, Codex and uv |
| Idle stop | On-box timer: powers off after 60 minutes with no session and no CPU work (Terry: a lunch break shouldn't cost a reboot) |
| Backstop | CloudWatch alarm: stops it after 3 hours averaging under 1% CPU |
| Defined in | [`infra/agentspace.yaml`](infra/agentspace.yaml), deployed as the CloudFormation stack `agentbox` |

**The AWS side keeps the project's first name, agentbox:** the stack, its resource names and its
tags. A stack cannot be renamed without replacing the instance and its disk, so only the names
people and agents use changed.

## Use it

```bat
ssh agentspace
```

The `agentspace` entry in `%USERPROFILE%\.ssh\config` runs `scripts\ssh-proxy.cmd`, which starts
the instance if it is stopped, waits until its boot-time patch run has finished, and tunnels SSH
through Session Manager. The first connection after a stop takes under a minute, longer when a
new kernel means a reboot. VS Code Remote-SSH and the Claude desktop app's SSH connections can use
the same host name.

The whole entry, since it lives outside this repo:

```
Host agentspace
  User ubuntu
  IdentityFile ~/.ssh/id_ed25519
  ProxyCommand C:\Windows\System32\cmd.exe /d /c "C:\Projects\agentspace\scripts\ssh-proxy.cmd"
  StrictHostKeyChecking accept-new
  ServerAliveInterval 30
  ServerAliveCountMax 4
  RemoteForward 127.0.0.1:8811 127.0.0.1:8811
  RemoteForward 127.0.0.1:53147 127.0.0.1:53147
```

```bat
powershell -File scripts\agentspace.ps1 status
powershell -File scripts\agentspace.ps1 down
powershell -File scripts\agentspace.ps1 grow -GiB 2
powershell -File scripts\agentspace.ps1 sync
powershell -File scripts\agentspace.ps1 deploy
powershell -File scripts\agentspace.ps1 ca
```

- **`sync`** checks the agentspace SSH CA first, with the same checks `ca` makes, and stops
  before touching the instance if the pair is missing, mismatched or readable by anyone else
  (D17). It then installs `box/system` on the instance:
  - The idle check, the boot-time patch service and the localswim wrapper.
  - The CA's public key, sent from `%USERPROFILE%\.agentspace\ca`, as
    `/etc/ssh/agentspace_ca.pub`. Each sync replaces the file rather than adding to it, so after
    a new CA only the new one is trusted.
  - The sshd drop-in `/etc/ssh/sshd_config.d/50-agentspace-ca.conf`, from
    `box/system/50-agentspace-ca.conf`: `TrustedUserCAKeys` points at that file, so certificates
    the CA signs for `ubuntu` log in beside the `authorized_keys` key, and `PermitRootLogin no`
    closes root login, by certificate or otherwise. `install.sh` runs `sshd -t` before
    `systemctl reload ssh.service`; if the check fails, it puts back the files that were there,
    leaves sshd running as it was, and fails. It leaves every other drop-in alone.
  - The sshd drop-in `/etc/ssh/sshd_config.d/51-agentspace-logins.conf`, from
    `box/system/51-agentspace-logins.conf`: `PerSourcePenaltyExemptList 127.0.0.1`, so refused
    logins, which all arrive from 127.0.0.1 through Session Manager, never lock every login out,
    and `LoginGraceTime 30`, so a refused login's connection is dropped after 30 seconds rather
    than 120 (D7, E3.01 finding). It shares the CA drop-in's `sshd -t` gate and rollback.

  It also copies the laptop's `~/.claude/CLAUDE.md` (Terry's global instructions for Claude
  Code) there, builds and installs the localswim CLI, and sends the current localswim Bot token.
- **`deploy`** updates the CloudFormation stack and then runs `sync`. The account-specific
  parameters (`VpcId`, `SubnetId`) come from `infra\local.parameters`, which is not committed;
  `infra\local.parameters.example` shows the format.
- **`ca`** creates the agentspace SSH CA that signs lease certificates
  ([D7](docs/decisions.md#d7-ssh-access-through-short-lived-certificates-from-an-agentspace-ca)),
  once, as `%USERPROFILE%\.agentspace\ca\agentspace_ca` and `agentspace_ca.pub`, locked to the
  current user (D17). Running it again changes nothing and prints the paths and fingerprint. It
  refuses to overwrite a CA, and fails without repairing anything when the pair is half there,
  mismatched or readable by anyone else.
- **`scripts\lease-credential.ps1`** (PowerShell 7, D18) defines
  `New-LeaseCredential -LeaseId <id> -Ttl <TimeSpan>`, which the lease CLI dot-sources. It checks
  the CA with `agentspace.ps1 ca`, makes a new ed25519 key without a passphrase in
  `%USERPROFILE%\.agentspace\leases\<id>\`, and signs it with principal `ubuntu`, the lease ID as
  key ID and a validity of exactly the TTL. It refuses a lease ID that is not a safe token and a
  lease that already has a key. Check it with
  `pwsh -NoProfile -File scripts\test-lease-credential.ps1`, which prints PASS or FAIL per check
  and removes its throwaway leases.
  The same file also defines `Update-LeaseCredential -LeaseId <id> -Ttl <TimeSpan>`, which signs a
  new certificate for an existing key, and `Remove-LeaseCredential -Project <name> -LeaseId <uuid>`,
  which deletes the lease's alias file and its folder and leaves `~/.ssh/config` alone. Check them
  with `pwsh -NoProfile -File scripts\test-lease-renew-release.ps1`.
- **`scripts\lease-alias.ps1`** (PowerShell 7, D18) defines `Set-LeaseAlias` and `Remove-LeaseAlias`
  (each `-Project <name> -LeaseId <uuid>`) and `Install-LeaseAliasInclude`, which the lease CLI
  dot-sources. They write and remove a lease's host alias, `agentspace-<project>-<first 8 hex of
  the lease ID>`, as a file in `~/.ssh/agentspace.d` (see "Lease host aliases" below). Check them
  with `pwsh -NoProfile -File scripts\test-lease-alias.ps1`, which works in a throwaway folder
  under `%USERPROFILE%\.agentspace`, never in the real `~/.ssh`, and prints PASS or FAIL per check.

### Lease host aliases

Each lease gets one alias file, `~/.ssh/agentspace.d/agentspace-<project>-<shortid>`, written by
`Set-LeaseAlias` and deleted by `Remove-LeaseAlias`. `~/.ssh/config` reaches them through one line:

```
Include agentspace.d/*
```

- **The line MUST be above the first `Host` or `Match` line.** An `Include` after a `Host` line
  belongs to that host's block. `Set-LeaseAlias` adds it as line 1 when it is missing, and refuses,
  changing nothing, when it finds one below a `Host` line: a person has to move it.
- **The path resolves against `~/.ssh`**, not the current directory.
- **The first change backs up the config** as `config.agentspace-backup-<UTC time>` beside it,
  checks the copy, and then inserts the one line. Nothing else in the file changes, byte for byte,
  and its line endings and permissions stay as they were.
- **Removing an alias leaves the `Include` in place**, and leaves the lease's key folder alone.
- **`C:\Windows\System32\OpenSSH\ssh.exe -G <alias>`** prints the settings an alias gives.
- **Today the alias's ProxyCommand is `scripts\ssh-proxy.cmd`**, whose `up` still pushes the
  localswim token over Terry's own `agentspace` entry (#0028) and can hang (#0026). The default
  ProxyCommand needs the checkout that has `.instance-id`; elsewhere, pass `-ProxyCommand`.

### The localswim board on the box

Terry's localswim board (`127.0.0.1:8811`) and the localpost queue behind its watcher
(`127.0.0.1:53147`) run on the laptop. The two `RemoteForward` lines lend them to the box: while
an `ssh agentspace` connection is open, both answer at the same addresses on the box's loopback,
so agents there use the board exactly as they would on the laptop. The board stays on the laptop
because the box is stopped most of the time, and a tunnel held open to a board on the box would
keep it from ever stopping (Terry, 2026-11-16).

- **The first connection binds the ports; later ones print "remote port forwarding failed" and
  carry on**, using the first one's forward. When the first connection closes, the board
  disappears from the box until a new connection opens, even if others are still open.
- **`agentspace.ps1` passes `ClearAllForwardings=yes` to its own ssh and scp calls**, so they
  never take the ports from an interactive session.
- **An agent on the box MUST treat an unreachable board as a reason to stop and report, not to
  work without a card.** The board goes away when Terry disconnects, as it does when the laptop
  sleeps.

**Agents on the box run `localswim` as Bot**, for example `localswim board list`. It is a
wrapper (`box/system/localswim`, installed to `/usr/local/bin`) around the real CLI in
`/usr/local/lib/localswim/localswim`:

- **The CLI is built on the laptop by `sync`** from `C:\Projects\localswim` (Go,
  `CGO_ENABLED=0`, linux/arm64, statically linked), so it speaks the same API as the service it
  talks to. After the service is updated to a newer localswim, `sync` MUST be run again.
- **The Bot token reaches the box when Terry connects.** localswim makes a new token at every
  cold start, so `up` (run by every `ssh agentspace`) reads the current one from localswim's DPAPI
  file and, when the instance or the token has changed since the last send, writes it with the
  URL to `~/.config/localswim/bot.env` (mode 0600). The token travels over SSH on stdin: it MUST
  NOT be put on a command line, printed, or stored anywhere else. Nothing about it is kept on
  the laptop except a SHA-256 of what was last sent, in `%USERPROFILE%\.agentspace`.
- **The wrapper sets the URL and token for one run only**, and refuses with a stop-and-report
  message when the board is unreachable, so an agent learns why instead of seeing a timeout.
- **A project's own localswim wrapper still decides how that project's agents call it.**
  swimcoach's `scripts/localswim.sh` runs this wrapper on the box; its Windows twin,
  `scripts/localswim.ps1`, reads the laptop's receipt directly.

## Rules

RFC 2119 keywords; the capitals are load-bearing.

- **The box MUST be fully patched at every start** (Terry, 2026-11-16): a dist-upgrade within
  Ubuntu 26.04, and a reboot when the kernel changes. `agentspace-patch.service` does it, and
  `up` MUST NOT report ready before it finishes.
- **Node MUST be the newest LTS, with the npm it ships with** (Terry, 2026-11-16).
  `agentspace-tools` makes it fnm's default at every boot; a project's `.node-version` MAY still
  select another version inside that project.
- **`ImageId` MUST NOT be changed on the live stack.** CloudFormation replaces the instance to
  change its AMI, which destroys the disk and everything on it. The boot-time patch keeps the
  running box current instead.
- **sshd settings MUST be changed through a drop-in in `box/system`, installed by `sync`, never
  by editing the box.** sshd keeps the first value it reads for a keyword, so a new drop-in MUST
  NOT set a keyword that another one already sets.
- **Changes to the box's services MUST be made in `box/system` and installed with `sync`.**
  The template's user data runs only at first boot, so editing it changes nothing on a live box.
- **The laptop's `~/.claude/CLAUDE.md` is the source; the box's copy MUST NOT be edited.**
  `sync` replaces it. After changing the laptop copy, run `sync`; a Claude session already
  running on the box picks it up only when a new session starts.
- **The disk SHOULD stay above about 70% used**, by Terry's preference: grow it in small steps
  when it passes 85%. EBS volumes cannot shrink, and allow one change every 6 hours.
- **Long agent runs SHOULD stay attached to a session or keep the CPU busy.** The idle check
  counts an SSH or Session Manager session, or 6 CPU-seconds of work per 5 minutes, as activity;
  an agent left alone and waiting for 60 minutes gets the box powered off.
- **Credentials MUST come from `aws login`**, never from long-lived access keys. When a sign-in
  expires, run `aws login --remote` and open the URL it prints in a fresh Incognito window, with
  every other Incognito window closed first; a browser holding another AWS session returns a 400.
  `--remote` prints the URL instead of opening the default browser, and asks for the code the
  page shows.
- **Account-specific values and secrets MUST NOT be committed:** this repo is public.
  Account-specific parameters go in `infra\local.parameters`; tokens never touch the repo.
- **Unix-relevant files MUST use LF line endings**; only Windows-specific files (`.cmd`, `.ps1`)
  use CRLF. `.gitattributes` enforces it (Terry, 2026-11-16).

## Toolchain

`box/bootstrap.sh` installs gh, Node through fnm (the newest LTS by default; a project's
`.node-version` picks its own), Claude Code, Codex and uv, once per new instance. After that,
`agentspace-tools` keeps them current at every boot. Run the bootstrap again at any time:

```bat
ssh agentspace "bash -s" < box\bootstrap.sh
```

### Signing the tools in

Each tool is signed in once per instance, by Terry, at the box's shell prompt.

- **Claude Code:** run `claude` and choose the option that shows a URL and a code.
- **Codex:** its browser sign-in redirects to port 1455 on the machine running the browser,
  so connect from the laptop with that port forwarded, then run `codex login` on the box and
  open its URL in the laptop browser:

  ```bat
  ssh -L 1455:localhost:1455 agentspace
  ```

  Device-code sign-in (`codex login --device-auth`) SHOULD NOT be used: it needs a ChatGPT
  security setting turned on, and device codes are a phishing route.
- **gh:** `gh auth login`, choosing HTTPS and the web browser; it prints a code to enter at
  `https://github.com/login/device`.

## Logs

```bat
ssh agentspace journalctl -u agentspace-idle --since -1h
ssh agentspace journalctl -u agentspace-patch -b
```

### Which lease connected

RFC 2119 keywords; the capitals are load-bearing.

sshd logs the key ID of every certificate login, and a lease's key ID is its lease ID (D7). To
list the last hour's logins and refusals, or those of one lease:

```bat
ssh agentspace "journalctl -u ssh --since -1h --no-pager -o short-iso | grep -E 'Accepted|Refusing'"
ssh agentspace "journalctl -u ssh --since -1h --no-pager -o short-iso | grep 'ID.*<lease id>'"
```

- A lease login reads `Accepted publickey for ubuntu from 127.0.0.1 port ... ssh2: ED25519-CERT
  SHA256:... ID <lease id> (serial 0) CA ED25519 SHA256:G05h...`.
- A login with an expired certificate reads `Refusing certificate ID "<lease id>" ...
  Certificate invalid: expired`.
- A login with Terry's own key reads `ED25519 SHA256:...` with no `ID`.
- Every login comes from 127.0.0.1, where Session Manager delivers it, so the address never tells
  two leases apart; the `ID` does.
- An expired certificate stops new logins only. A session that was open when it expired keeps
  running (D10).
- Refused logins MUST NOT be retried in a loop: after as few as two, sshd can shut every login out
  for a while, Terry's included (D7's E2.04 finding, and D10's E2.08 finding).

## License

[MIT](LICENSE). Copyright (c) 2026 Sixbucks Solutions, LLC.

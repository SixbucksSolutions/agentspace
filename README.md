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
```

- **`sync`** installs `box/system` on the instance: the idle check, the boot-time patch service
  and the localswim wrapper. It copies the laptop's `~/.claude/CLAUDE.md` (Terry's global
  instructions for Claude Code) there, builds and installs the localswim CLI, and sends the
  current localswim Bot token.
- **`deploy`** updates the CloudFormation stack and then runs `sync`. The account-specific
  parameters (`VpcId`, `SubnetId`) come from `infra\local.parameters`, which is not committed;
  `infra\local.parameters.example` shows the format.

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
  the laptop except a SHA-256 of what was last sent, in `%LOCALAPPDATA%\agentspace`.
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
  expires, run `aws login` from a fresh Incognito window; a browser holding another AWS session
  returns a 400.
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

## License

[MIT](LICENSE). Copyright (c) 2026 Sixbucks Solutions, LLC.

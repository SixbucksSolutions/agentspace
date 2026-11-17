# agentbox

A Graviton EC2 instance in us-west-2 for running coding agents off the laptop. It is stopped
most of the time: connecting starts it, and it stops itself when nobody is using it.

| Piece | What |
|---|---|
| Instance | m9g.xlarge (4 Graviton cores, 16 GiB), Ubuntu 26.04 LTS arm64 |
| Disk | 8 GiB gp3, encrypted; grown on demand, never shrunk |
| Access | Session Manager only: no inbound ports, no key pair, no Elastic IP |
| Patching | Every boot: `apt-get dist-upgrade` (phased updates included) and `snap refresh`, then one reboot if the kernel or another package needs it; then the newest Node LTS, Codex and uv |
| Idle stop | On-box timer: powers off after 60 minutes with no session and no CPU work (Terry: a lunch break shouldn't cost a reboot) |
| Backstop | CloudWatch alarm: stops it after 3 hours averaging under 1% CPU |
| Defined in | [`infra/agentbox.yaml`](infra/agentbox.yaml), CloudFormation stack `agentbox` |

## Use it

```bat
ssh agentbox
```

The `agentbox` entry in `%USERPROFILE%\.ssh\config` runs `scripts\ssh-proxy.cmd`, which starts
the instance if it is stopped, waits until its boot-time patch run has finished, and tunnels SSH
through Session Manager. The first connection after a stop takes a minute or two, longer when a
new kernel means a reboot. VS Code Remote-SSH and the Claude desktop app's SSH connections can use
the same host name.

```bat
powershell -File scripts\agentbox.ps1 status
powershell -File scripts\agentbox.ps1 down
powershell -File scripts\agentbox.ps1 grow -GiB 2
powershell -File scripts\agentbox.ps1 sync
powershell -File scripts\agentbox.ps1 deploy
```

`sync` installs `box/system` (the idle check and the boot-time patch service) on the instance,
and copies the laptop's `~/.claude/CLAUDE.md` (Terry's global instructions for Claude Code) to
the box; `deploy` updates the CloudFormation stack and then runs `sync`.

## Rules

RFC 2119 keywords; the capitals are load-bearing.

- **The box MUST be fully patched at every start** (Terry, 2026-11-16): a dist-upgrade within
  Ubuntu 26.04, and a reboot when the kernel changes. `agentbox-patch.service` does it, and `up`
  MUST NOT report ready before it finishes.
- **Node MUST be the newest LTS, with the npm it ships with** (Terry, 2026-11-16).
  `agentbox-tools` makes it fnm's default at every boot; a project's `.node-version` MAY still
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
- **Credentials come from `aws login`** as the IAM user `admin` (the `default` profile). When a
  sign-in expires, run `aws login` from a fresh Incognito window; a browser holding another AWS
  session returns a 400.

## Toolchain

`box/bootstrap.sh` installs gh, Node through fnm (the newest LTS by default; a project's
`.node-version` picks its own), Claude Code, Codex and uv, once per new instance. After that,
`agentbox-tools` keeps them current at every boot. Run the bootstrap again at any time:

```bat
ssh agentbox "bash -s" < box\bootstrap.sh
```

### Signing the tools in

Each tool is signed in once per instance, by Terry, at the box's shell prompt.

- **Claude Code:** run `claude` and choose the option that shows a URL and a code.
- **Codex:** its browser sign-in redirects to port 1455 on the machine running the browser,
  so connect from the laptop with that port forwarded, then run `codex login` on the box and
  open its URL in the laptop browser:

  ```bat
  ssh -L 1455:localhost:1455 agentbox
  ```

  Device-code sign-in (`codex login --device-auth`) SHOULD NOT be used: it needs a ChatGPT
  security setting turned on, and device codes are a phishing route.
- **gh:** `gh auth login`, choosing HTTPS and the web browser; it prints a code to enter at
  `https://github.com/login/device`.

## Logs

```bat
ssh agentbox journalctl -u agentbox-idle --since -1h
ssh agentbox journalctl -u agentbox-patch -b
```

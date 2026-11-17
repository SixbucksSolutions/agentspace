# How agentspace was built

agentspace is an EC2 instance that runs coding agents (Claude Code, Codex) off Terry's Windows
laptop. It was built on 2026-11-16 by a Claude Code session (Opus 5.5) working with Terry. This
page records what was built and why, the one-time setup only Terry could do, and how agents on
the box reach the localswim board that runs on the laptop. The working rules are in the
[README](../README.md).

## What Terry asked for

- **Off most of the time.** Started when work begins, stopped when it ends, and paying for little
  more than storage in between.
- **Right-sized,** on a Graviton instance in us-west-2 (Oregon).
- **A first-class agent host.** Terry first asked for Alpine; see "Ubuntu, not Alpine" below.
- **A small disk:** the default 8 GB "is often way too much", so pick a size that doesn't go
  much below 70% used.
- **Fully patched at every start,** to the latest stable Ubuntu 26.04, with a reboot when a new
  kernel needs one.
- **The latest LTS Node and npm.**
- **An hour of idle before it stops:** "Rather keep dev velocity up than wait for it to reboot
  after a lunch break."

## Decisions, and why

| Decision | Why |
|---|---|
| **Ubuntu 26.04 LTS arm64, not Alpine** | Claude Code runs on Alpine, but workerd (Wrangler's runtime) and Playwright do not support musl. Terry chose 26.04 as the latest LTS. |
| **m9g.xlarge** (4 Graviton cores, 16 GiB) | Each vCPU is a full core. Terry took m9g over m8g: about 9% more per hour for about 18% more clock (3.3 against 2.8 GHz). The agent MAY move between the C, M and R families on evidence: more cores if CPU-bound, r9g if memory is tight. |
| **8 GiB gp3 root, encrypted** | The smallest the AMI allows. After the toolchain was installed it sat at 54% of 6.8 GB usable, under Terry's 70% target, with room for repos. It grows online in 2 GiB steps (`agentspace.ps1 grow`); EBS cannot shrink. |
| **Session Manager only** | No inbound ports, no Elastic IP, and no EC2 key pair. SSH runs inside a Session Manager tunnel, and cloud-init installs Terry's public key at first boot. |
| **Start on connect** | `ssh agentspace` runs a ProxyCommand that starts the instance if it is stopped and waits for its patch run, so there is no separate "start" step. |
| **Stop after 60 idle minutes** | An on-box timer checks every 5 minutes for an SSH or Session Manager session, a running patch, or CPU work (6 CPU-seconds or more per 5 minutes). An idle box measured 0 CPU-seconds per check, so background noise does not keep it awake. |
| **A CloudWatch backstop** | It stops the instance after 3 hours averaging under 1% CPU, in case the on-box check ever dies. |
| **Patch at every boot** | A systemd service runs `apt-get dist-upgrade` with phased updates included, then `snap refresh`. It reboots once if the kernel or another package needs it, at most once per 15 minutes. Then it moves Node to the newest LTS and updates Codex and uv. `ssh agentspace` waits for it to finish. |
| **CloudFormation, with the AMI pinned** | One template defines the role, the security group, the instance and the alarm. A new AMI would replace the instance and its disk, so the AMI stays pinned and the boot-time patch keeps the box current. |

**Cost:** about $0.20 an hour while running, and under $1 a month stopped (the disk).

## How it was built

1. **The template.** [`infra/agentspace.yaml`](../infra/agentspace.yaml) defines:
   - An IAM role with `AmazonSSMManagedInstanceCore`.
   - A security group with no inbound rules.
   - The instance: IMDSv2 required, stop on shutdown, and a cloud-init user data that runs only
     at first boot.
   - The CloudWatch alarm.

   The AMI came from Canonical's public SSM parameter for 26.04 arm64. `agentspace.ps1 deploy`
   creates or updates the stack.
2. **The toolchain,** once per instance: [`box/bootstrap.sh`](../box/bootstrap.sh) installs gh,
   fnm and the newest Node LTS, Claude Code (native installer), Codex and uv.
3. **The on-box services,** from [`box/system`](../box/system), installed by `agentspace.ps1
   sync`: the idle check and its timer, the boot-time patch service, the toolchain updater, and
   the localswim wrapper.
4. **The laptop side:** [`scripts/agentspace.ps1`](../scripts/agentspace.ps1) (deploy, up,
   sync, down, status, grow), [`scripts/ssh-proxy.cmd`](../scripts/ssh-proxy.cmd) (the SSH
   ProxyCommand), and the `agentspace` entry in `~/.ssh/config` shown in the README.

**What was measured on the real instance:**
- From stopped to a shell took 37 seconds.
- A forced patch reboot reconnected on its own in 59 seconds.
- With a 10-minute test limit, the idle stop powered the box off 13 minutes after the last
  session.
- An IMDSv1 request was refused with 401.
- The first patch run installed 4 updates.

**Not tested:** the 3-hour CloudWatch backstop.

## What Terry had to set up

These steps were Terry's to do, because they involve his identity, passwords or approvals.
Claude did everything else.

1. **A non-root AWS admin identity.**
   - **The user:** Terry created an IAM user with `AdministratorAccess` for day-to-day work, and
     kept the root account for root-only jobs.
   - **Password and MFA:** he set its console password (kept in his password manager) and added
     a passkey as its MFA device.
   - **Credentials:** they come from `aws login`, which gives short-lived credentials that
     refresh on their own. No access keys exist.
2. **AWS CLI 2.32 or newer,** which `aws login` needs. He installed the current version
   machine-wide with winget, which took a UAC prompt. An older per-user install shadowed it on
   `PATH` and was removed. The scripts call the CLI by its full path anyway.
3. **The Session Manager plugin** for the AWS CLI, which also took a UAC prompt.
4. **Signing in with `aws login`.** He signed in from a fresh Incognito window with only his
   password manager allowed (see "Problems hit" for why).
5. **Signing the tools in on the box,** once per instance, at its shell prompt:
   - **Claude Code:** run `claude` and use its URL-and-code sign-in.
   - **Codex:** run `codex login` over a connection that forwards port 1455 (`ssh -L
     1455:localhost:1455 agentspace`), then open the printed URL in the laptop browser. Its
     sign-in redirects to `localhost:1455`, which the tunnel carries to the box. This avoids
     device-code sign-in, which needs a ChatGPT security setting turned on.
   - **gh:** `gh auth login`, choosing HTTPS and the web browser; it prints a code to enter at
     `github.com/login/device`.
6. **GitHub repo settings for swimcoach,** approved by Terry: auto-merge allowed, branches
   deleted on merge, and a ruleset requiring CI's `check` on `main`, with repository admins
   exempt so his own pushes still land.

**On the laptop, for Claude Code itself:** the AWS MCP server (from the `aws-core` plugin) failed
to start until `UV_PYTHON=3.14` was set in Claude Code's settings. Its proxy runs under `uvx`,
which picked Python 3.15, and `pywin32` has no wheels for 3.15 yet. The setting comes out once
those wheels exist.

## Reaching the laptop's localswim board

Terry runs every project from a board on localswim, a local service on the laptop at
`127.0.0.1:8811`. Agents on agentspace must reach it to read and move their cards.

**Why the board stays on the laptop.** Hosting localswim on the box was considered and rejected.
- **The box is off most of the time,** so the board would go dark between sessions, including
  for projects that never leave the laptop.
- **A tunnel held open to a board on the box would keep it from ever idling,** which would mean
  paying to run it all the time.
- **It is a stack, not one service.** The board's watcher relay, its localpost queue and its
  credential store all assume Windows and the laptop.

So the box borrows the board instead.

**How the box borrows it:**
- **Reverse port forwards.** The `agentspace` SSH entry carries two `RemoteForward` lines, one
  for the board (8811) and one for the localpost queue behind its watcher (53147). While any
  `ssh agentspace` connection is open, both answer at the same `127.0.0.1` addresses on the box,
  bound to its loopback only.
- **Only the first connection binds them.** Later connections warn and carry on, and the
  scripts' own short ssh and scp calls pass `ClearAllForwardings=yes` so they never take the
  ports.
- **A Linux build of the localswim CLI.** `sync` builds it on the laptop from the same source as
  the running service: Go, `CGO_ENABLED=0`, linux/arm64, a static binary. It installs it on the
  box behind a `localswim` wrapper.
- **The Bot token arrives at connect time.** localswim makes a new token at every cold start, so
  a copy made once would go stale. Every `ssh agentspace` reads the current token from
  localswim's DPAPI-protected file on the laptop and compares a SHA-256 of it with the last one
  sent. When it differs, it writes the token to `~/.config/localswim/bot.env` on the box (mode
  0600), over the SSH connection's stdin. The token never appears on a command line, in output,
  or in any cloud service.

  AWS Secrets Manager was considered and not needed. The token is only useful while the tunnel
  is up, and the tunnel only exists while Terry is connected over SSH.
- **The wrapper refuses when the board is unreachable.** It sets the URL and token for a single
  run of the CLI. When the board cannot be reached, it exits with a message telling the agent to
  stop and report rather than work without a card.

**Measured on the box:**
- With the tunnel, `localswim board list` returned all 22 boards.
- Without the tunnel, it refused with that message and exit status 1.
- A reconnect with an unchanged token sent nothing.

swimcoach's agents reach it through `scripts/localswim.sh`, the Linux twin of its Windows-only
`scripts/localswim.ps1`.

## Problems hit along the way

| Problem | Cause | Fix |
|---|---|---|
| `aws login` returned 400 at its OAuth step | The browser offered an existing AWS session for another identity | Sign in from a fresh Incognito window with no other AWS session |
| The AWS MCP server closed on start | `uvx` chose Python 3.15, which `pywin32` has no wheels for | `UV_PYTHON=3.14` in Claude Code's settings |
| Over SSH, line breaks printed as `♪◙`, lines did not wrap, and `clear` and `reset` did nothing | Started as the ProxyCommand (stdin a pipe, stderr the console), Windows PowerShell 5.1 set the console's output mode to escape processing alone, dropping CR/LF handling and wrap. ssh then saved that mode and built the session's raw mode on it | The script turns line processing and wrap back on as it starts and as it ends. A probe logging the console mode every 100 ms found the cause and confirmed the fix: the session mode went from `0xC` to `0xF` |
| The token file's first variable did not load | Windows PowerShell put a UTF-8 BOM in front of text piped to `ssh` | Send with no BOM, and strip one on arrival |
| A remote command with `"` in it broke | Windows PowerShell 5.1 does not escape embedded double quotes in native-command arguments | Single quotes in remote commands |
| Git Bash's `ssh` could not use the entry | It runs the ProxyCommand through `/bin/sh`, which mangles the Windows path | Use Windows OpenSSH (`C:\Windows\System32\OpenSSH\ssh.exe`) |
| `sync` hung on a stopped box | It waited for Session Manager without starting the instance | `sync` and `up` share one start-and-wait step |
| A script piped into `bash` failed with `$'\r'` | PowerShell added a CR when piping | LF line endings in every Unix-relevant file, pinned in `.gitattributes`, and CRs stripped when piping |

## The rename

The project started as agentbox and was renamed agentspace the same evening, so Terry can tell
lead agents to "run this in agentspace". Everything people and agents use took the new name: the
SSH host, the scripts, the on-box services and paths, and this repo. `sync` retires the old
units on a live box. The AWS side kept agentbox (the stack, its resource names and its tags):
renaming a stack means replacing the instance, its disk, and the sign-ins on it.

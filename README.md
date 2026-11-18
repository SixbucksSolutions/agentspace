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
    logins, which all arrive from 127.0.0.1 through Session Manager, no longer earn a penalty
    that locks every login out; `LoginGraceTime 30`, so a refused login's connection is dropped
    after 30 seconds rather than 120 (D7, E3.01 finding); and `ClientAliveInterval 15` with
    `ClientAliveCountMax 5`, so a logged-in session whose client has gone, which the Session
    Manager tunnel never reports, is dropped with its forwards about 90 seconds after it goes quiet
    (D12, 0.02.T1.1 finding). It shares the CA drop-in's `sshd -t` gate and rollback.

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

### The lease CLI

`bin\agentspace.cmd`, and `bin/agentspace` for Git Bash, is `agentspace lease`, whose contract is
[docs/lease-api.md](docs/lease-api.md). `--help` works at every level, and every subcommand works:
`scripts\test-lease-cmd-create-get.ps1` checks `create` and `get`, and
`scripts\test-lease-cmd-renew-release-list.ps1` checks `renew`, `release` and `list`. Leases are kept
in `%USERPROFILE%\.agentspace\leases.json`, or in the folder that `AGENTSPACE_STATE_DIR` names. Check
the CLI with `pwsh -NoProfile -File scripts\test-lease-cli.ps1`, which works in a throwaway folder
under `%TEMP%` and prints PASS or FAIL per check.

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
- **The alias's ProxyCommand names the alias itself** (card #0057), written out literally at the
  end: `C:\Windows\System32\cmd.exe /d /c "<repo>\scripts\ssh-proxy.cmd" <alias>`. It MUST NOT
  use ssh's `%n` token, because the Claude desktop app runs the ProxyCommand line through its own
  proxy (D12) and nothing shows that it expands tokens. `ssh-proxy.cmd` passes the alias to
  `agentspace.ps1 up -Alias <alias>`, so the token push rides the lease's own key and
  certificate, not Terry's `agentspace` entry. An alias MUST match `^[A-Za-z0-9._-]+\z`, case
  sensitive, and MUST NOT start with `-`, since it is on a command line; the alias names
  `Set-LeaseAlias` generates always do, and it refuses one that does not. A project name MUST be
  ASCII. That check is the gate: `ssh-proxy.cmd` passes its argument on as it is, because whoever
  writes a ProxyCommand line already runs commands, and `up` checks the alias again before any
  ssh. The default ProxyCommand needs the checkout that has `.instance-id`; elsewhere, pass
  `-ProxyCommand`, which is written as given and SHOULD end with the alias.

### Use a lease from a project

RFC 2119 keywords; the capitals are load-bearing.

How a project such as swimcoach runs its agents on a lease. The commands are in "The lease CLI"
above and [docs/lease-api.md](docs/lease-api.md), the alias in "Lease host aliases", and the board
in "The localswim board on the box" below. Story E3.03 ran these steps
([D9](docs/decisions.md#d9-reachability-belongs-to-the-driver),
[D12](docs/decisions.md#d12-board-access-is-unchanged)).

1. **Create the lease from the main checkout**, `C:\Projects\agentspace`:

   ```bat
   bin\agentspace.cmd lease create --project swimcoach
   ```

   The default ProxyCommand needs that checkout's `.instance-id`. Run from anywhere else,
   `AGENTSPACE_PROXY_COMMAND` MUST name a ProxyCommand that runs the main checkout's
   `scripts\ssh-proxy.cmd` with the lease's alias after it. The JSON gives the lease's `id`, its
   `expires_at` (8 hours from now unless `--ttl` says otherwise) and `ssh.host_alias`, such as
   `agentspace-swimcoach-a62cb513`.
2. **Put the project's repo on the box.** Nothing clones it for you. The repo MUST be in the
   folder the agents will open, for example `ssh <alias> "git clone <url> ~/swimcoach"`. Every
   lease is `ubuntu` in the MVP
   ([D3](docs/decisions.md#d3-ssh-credential-per-agent-isolation-per-project)), so the checkout is
   in a home that every lease shares, and it outlives the lease.
3. **Connect** with `ssh <alias>`, or in the Claude desktop app add an SSH environment whose host
   is the alias and open a session in the project's folder on the box (`/home/ubuntu/swimcoach`).
   The first connection after a stop starts the box, and any connection may push the Bot token,
   over the alias that connection used. **The desktop app's Test connection does not show the
   proxy's output**, so `Sending the localswim Bot token to the instance...` never appears there
   (E0.02-T03-S01). A push from the app MUST be confirmed on the box, by `bot.env`'s modification
   time, or by the laptop's stamp coming back, never from the app's output.
   From cmd or PowerShell, `ssh` is Windows OpenSSH. From Git Bash it MUST be called by its full
   path, `C:\Windows\System32\OpenSSH\ssh.exe`, because MSYS ssh mangles the ProxyCommand.
4. **A desktop-app session MUST have a holder open on the laptop for as long as it needs the
   board, and the keeper provides it.** The app connects with `ClearAllForwardings=yes`, so the
   alias's `RemoteForward` lines never apply to its own connection (card #0042). The holder holds
   them instead, and nobody starts it by hand
   ([D30](docs/decisions.md#d30-a-keeper-on-the-laptop-holds-the-boards-forwards)):
   - **The keeper runs at logon as the scheduled task `agentspace board holder`.** It runs
     `scripts\board-holder.ps1 -Watch` through a no-window launcher. While a lease is ready and no
     holder runs, it starts `ssh.exe -N -o ExitOnForwardFailure=yes <alias>` through the ready
     lease that expires last, and it stops a holder whose lease is no longer ready.
   - **`scripts\install-board-holder.ps1` registers the task.** `-DryRun` prints every field of
     the task it would register and changes nothing, and `-Uninstall` removes the task and its
     launcher. Registering the task is persistent configuration on Terry's laptop, so it MUST only
     happen with his yes (E0.02-T03-S01).
   - **The install MUST be run from the main checkout, never from a worktree.** The task points at
     the folder the installer is in, and a worktree is deleted after its branch merges. An install
     from a worktree is refused with exit 2, and `-DryRun` there prints a `Warning:` line.
   - **The keeper logs to `%USERPROFILE%\.agentspace\board-holder.log`**, and the holder's own
     output goes to `board-holder-ssh.log` beside it, which is rotated to `board-holder-ssh.log.1`
     past 1 MB.
   - **A holder that fails fast is retried with backoff**, from 30 seconds doubling to 5 minutes,
     never in a tight loop. Only a holder seen alive after 60 seconds resets the backoff.
   - **No error ends the keeper.** It logs the error and carries on at its next interval, so only
     bad arguments or a killed process can stop it.
   - **A crashed keeper is not restarted until the next logon** (E0.02-T03-S01, D30). In that run
     the task's RestartCount did not restart a keeper that was killed; it most likely covers only a
     task that fails to start. Its holder keeps
     running, unsupervised. After a crash, the task MUST be started by hand with
     `Start-ScheduledTask -TaskName 'agentspace board holder'`, and the new keeper adopts the
     recorded holder. A keeper stopped on purpose (`-Uninstall`, or End in Task Scheduler) also
     stays stopped until the next logon.

   Only one connection on the box can hold the forwards (D12), so the holder exits rather than
   run without them when another connection already has them. An interactive `ssh <alias>` that
   holds them needs no holder. On the box, `localswim board list` checks the board; the wrapper
   refuses with a stop-and-report message when it is unreachable.
5. **Release the lease** when the work is done. Any checkout MAY run it, since the lease's state is
   in `%USERPROFILE%\.agentspace` and release needs no ProxyCommand:

   ```bat
   bin\agentspace.cmd lease release <id>
   ```

   It deletes the alias file and the lease's key folder and leaves the `Include` line. It does
   not end open sessions or revoke the certificate
   ([D10](docs/decisions.md#d10-releasing-a-lease-does-not-revoke-its-certificate)), so the holder
   and any session SHOULD be closed first. **Archiving a desktop-app session does not close its
   SSH connections** (E0.02-T03-S01): they stay up, and count as activity on the box, until the
   app closes them. The desktop app's SSH environment SHOULD then be removed, since its alias no
   longer exists. The project's checkout stays on the box.

   **A closed holder keeps its forwards on the box for about 90 seconds.** The Session Manager
   tunnel does not tell sshd that the client has gone, so the box drops a dead holder's session,
   and frees 8811 and 53147, about 90 seconds after the session goes quiet (D12's 0.02.T1.1
   finding, card #0044). A new holder started before then exits under `ExitOnForwardFailure`, so
   the keeper retries it with backoff. Only a session still holding them after 2 minutes is cleared by hand:
   it MUST be cleared as D12's E3.03 finding says.

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
  cold start, so `up` (run by the ProxyCommand of every connection) reads the current one from
  localswim's DPAPI file and, when the instance or the token has changed since the last send,
  writes it with the URL to `~/.config/localswim/bot.env` (mode 0600). The token travels over SSH
  on stdin: it MUST NOT be put on a command line, printed, or stored anywhere else. Nothing about
  it is kept on the laptop except a SHA-256 of what was last sent, in `%USERPROFILE%\.agentspace`.
- **The push rides the alias the caller connected with** (card #0057). A lease alias's
  ProxyCommand runs `ssh-proxy.cmd <alias>`, which runs `up -Alias <alias>`, so the push uses
  that lease's key and certificate. Terry's `agentspace` entry has no argument and still works:
  `ssh-proxy.cmd` runs `up` and the push goes over `agentspace`, as before. `sync` still pushes
  over `agentspace`, as intended. The push's ssh runs with `BatchMode=yes`, so an alias that
  resolves to an unknown host fails at once rather than waiting on a prompt.
- **A send is recorded only once the box confirms it.** The box reads two lines, writes them to
  a temporary file, renames it over `bot.env` and sends back the file's SHA-256, which `up`
  compares with what it meant to write. A failed, unconfirmed or interrupted send leaves the old
  `bot.env` in place, records nothing, and is tried again at the next connection. The send's ssh
  is stopped after 20 seconds, so a hang delays a connection by at most that. `scripts\test-push-localswim.ps1`
  checks this without the box, in Windows PowerShell 5.1.
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

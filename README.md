# agentspace

A Graviton EC2 instance in us-west-2 for running coding agents off the laptop. It is stopped
most of the time: connecting starts it, and it stops itself when nobody is using it. Once v0.08
is installed, it is also stopped once its project holds no live lease
([A host's life](#a-hosts-life)). Lead agents are told to "run this in agentspace".

**How it was built, what Terry set up, and how agents on it reach the laptop's localswim board:
[docs/how-it-was-built.md](docs/how-it-was-built.md).**

**Where it is going:** a lease API that lets projects ask for compute without knowing what hosts it.
The plan is in [docs/roadmap.md](docs/roadmap.md), and the reasoning in
[docs/decisions.md](docs/decisions.md). The work is on the `agentspace` localswim board.

**The programs are Rust, and the PowerShell prototype is gone** (D28; removed from the repository
by E0.06-T06-S01, 2026-11-20). The PowerShell that remains is the conformance suite, the
architect's process tools under `scripts\` and a crate's test stand-in. Every command below runs the
installed programs ([Build](#build), [Install it](#install-it)).

| Piece | What |
|---|---|
| Instance | m9g (Graviton, 4 GiB per vCPU), Ubuntu 26.04 LTS arm64; `m9g.xlarge` (4 vCPUs, 16 GiB) by default, sized for each use with a vCPU per agent ([D39](docs/decisions.md#d39-until-v009-the-box-is-sized-by-hand-for-each-use-m-class-a-vcpu-per-agent)) |
| Disk | gp3, encrypted, per host: the box's is 20 GiB and a new host starts at 8 GiB; each is grown on demand under the disk order ([AGENTS.md](AGENTS.md#growing-the-boxs-disk)), never shrunk |
| Access | Session Manager only: no inbound ports, no key pair, no Elastic IP |
| Patching | Every boot: `apt-get dist-upgrade` (phased updates included) and `snap refresh`, then one reboot if the kernel or another package needs it; then the newest Node LTS, Codex and uv |
| Lease guard | Once v0.08 is installed: on the laptop, `agentspace-operator guard`, started by the keeper every 5 minutes, stops a host within 25 minutes of its project's last live lease's end ([D56](docs/decisions.md#d56-a-host-runs-only-while-its-project-holds-a-live-lease-the-laptops-guard-stops-it-otherwise); [A host's life](#a-hosts-life)) |
| Idle stop | On-host timer: powers off after 60 minutes with no session, no patch run and no CPU work (Terry: a lunch break shouldn't cost a reboot); and, once v0.08 is installed, once the host's lease horizon plus the 15-minute grace has passed, at the first check with no session and no patch run, whatever its CPU is doing ([D57](docs/decisions.md#d57-a-host-bounds-itself-by-its-lease-horizon-while-the-laptop-is-away)) |
| Backstop | CloudWatch alarm: stops it after 24 hours averaging under 1% CPU once v0.08 is installed, 3 hours until then ([D58](docs/decisions.md#d58-the-cloudwatch-backstop-stays-lengthened-from-3-hours-to-24)) |
| Defined in | [`infra/agentspace.yaml`](infra/agentspace.yaml), deployed as the CloudFormation stack `agentbox` |

**The AWS side keeps the project's first name, agentbox:** the stack, its resource names and its
tags. A stack cannot be renamed without replacing the instance and its disk, so only the names
people and agents use changed.

## Use it

```bat
ssh agentspace
```

The `agentspace` entry in `%USERPROFILE%\.ssh\config` runs `agentspace-ssh-proxy.exe`, which starts
the instance if it is stopped, waits until its boot-time patch run has finished, and tunnels SSH
through Session Manager. The first connection after a stop takes under a minute, longer when a
new kernel means a reboot. VS Code Remote-SSH and the Claude desktop app's SSH connections can use
the same host name.

**Once v0.08 is installed, the box is guarded like every host** (D56; Terry, 2026-11-21): a
connection through this entry holds no lease, so work done through it is done under a live
swimcoach lease ([A host's life](#a-hosts-life)), or the guard stops the box within 25 minutes of
the later of its launch and its project's last lease end, sessions and all.

The whole entry, since it lives outside this repo:

```
Host agentspace
  User ubuntu
  IdentityFile ~/.ssh/id_ed25519
  ProxyCommand "C:\Users\<you>\.agentspace\bin\agentspace-ssh-proxy.exe"
  StrictHostKeyChecking accept-new
  ServerAliveInterval 30
  ServerAliveCountMax 4
  RemoteForward 127.0.0.1:8811 127.0.0.1:8811
  RemoteForward 127.0.0.1:53147 127.0.0.1:53147
```

The `ProxyCommand` line is the full path of the installed program in double quotes, with your
profile folder's value written out in place of `C:\Users\<you>` and no alias, since ssh expands no
environment variable in a ProxyCommand
([operator.md, ProxyCommand lines already on the laptop](docs/operator.md#proxycommand-lines-already-on-the-laptop)).

**The programs are the installed Rust programs** (D28, D37), kept in `%USERPROFILE%\.agentspace\bin`.
A person builds and installs them as [Build](#build) and [Install it](#install-it) below say.

```bat
agentspace-operator status
agentspace-operator status -All
agentspace-operator down
agentspace-operator grow -GiB 2
agentspace-operator sync
agentspace-operator deploy
agentspace-operator ca
```

The commands are the operator's ([docs/operator.md, Command line](docs/operator.md#command-line)).
`agentspace-operator` runs once the install folder is on the user's `PATH`, which is persistent
configuration on Terry's laptop and so takes his yes ([Install it](#install-it), step 4). Until
then each runs by its full path, `%USERPROFILE%\.agentspace\bin\agentspace-operator.exe`.

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
  Code) there, builds and installs the localswim CLI, and sends the current localswim Bot token
  with the laptop's time zone.
- **`status -All`** lists every host, each with its live leases' count, and, for a running host
  no live lease explains, the time the guard stops it ([A host's life](#a-hosts-life)).
- **`guard`** is started by the keeper every 5 minutes once v0.08 is installed, and stops every
  host that no live lease explains ([A host's life](#a-hosts-life)).
- **`deploy`** updates the CloudFormation stack and then runs `sync`. The account-specific
  parameters (`VpcId`, `SubnetId`) come from `%USERPROFILE%\.agentspace\stack-parameters.json`,
  JSON that a person writes and that is not committed (D40);
  `infra\stack-parameters.example.json` shows the format
  ([operator.md, deploy](docs/operator.md#deploy)). `deploy` fails before it calls AWS when the
  file is missing or not valid.
- **`ca`** creates the agentspace SSH CA that signs lease certificates
  ([D7](docs/decisions.md#d7-ssh-access-through-short-lived-certificates-from-an-agentspace-ca)),
  once, as `%USERPROFILE%\.agentspace\ca\agentspace_ca` and `agentspace_ca.pub`, locked to the
  current user (D17). Running it again changes nothing and prints the paths and fingerprint. It
  refuses to overwrite a CA, and fails without repairing anything when the pair is half there,
  mismatched or readable by anyone else.
- **A lease's key, certificate and host alias are the lease CLI's** (D28), no longer a PowerShell
  script's. `agentspace lease create` checks the CA (it never creates one), makes a new ed25519 key
  without a passphrase in `%USERPROFILE%\.agentspace\leases\<id>\`, signs it with principal
  `ubuntu`, the lease ID as key ID and a validity of exactly the TTL, and writes the lease's host
  alias, `agentspace-<project>-<first 8 hex of the lease ID>`, as a file in `~/.ssh/agentspace.d`
  (see "Lease host aliases" below). `renew` signs a new certificate for the existing key, and
  `release` ends the lease's ssh clients on the laptop
  ([D59](docs/decisions.md#d59-a-release-ends-the-leases-own-connections-on-the-laptop)), deletes
  the alias file and the lease's folder, and leaves `~/.ssh/config` alone. The
  rules are [docs/lease-api.md](docs/lease-api.md)'s and [docs/state.md](docs/state.md)'s, and the
  checks are the conformance suite ([docs/conformance.md](docs/conformance.md)). **The prototype's
  scripts for these, `scripts\lease-credential.ps1` and `scripts\lease-alias.ps1`, were removed by
  E0.06-T06-S01 (2026-11-20)**, with the rest of the prototype.

### The lease CLI

RFC 2119 keywords; the capitals are load-bearing.

**The lease CLI is the installed `agentspace`**, `%USERPROFILE%\.agentspace\bin\agentspace.exe`,
the Rust build of D38. It is `agentspace lease`, whose contract is
[docs/lease-api.md](docs/lease-api.md). `--help` works at every level, and every subcommand works.
Leases are kept in `%USERPROFILE%\.agentspace\leases.json`, or in the folder that
`AGENTSPACE_STATE_DIR` names.

- **`agentspace` runs the installed CLI once the install folder is on the user's `PATH`.** From cmd
  and PowerShell it is `agentspace.exe`, and from Git Bash it is the `agentspace` wrapper, which
  turns Git Bash's path conversion off ([lease-api.md, The program](docs/lease-api.md#the-program)).
  Putting the folder on `PATH` is persistent configuration on Terry's laptop, and MUST be done only
  with his yes ([operator.md, The install folder](docs/operator.md#the-install-folder)). Until it
  is, the CLI is run by its full path.
- **The checkout no longer holds a lease CLI.** Its `bin\agentspace.cmd` and `bin/agentspace` ran
  the PowerShell prototype, and were removed with it by E0.06-T06-S01 (2026-11-20; v0.06, #0055;
  [operator.md, Until v0.06 retires the prototype's entry points](docs/operator.md#until-v006-retires-the-prototypes-entry-points)).
  Which `agentspace` a person gets is `PATH`'s choice, so a `PATH` that still names a checkout's
  `bin\` SHOULD have that entry taken off, since it names nothing.
- **A lease SHOULD be created by the installed CLI**, since a lease created by a CLI in a build's
  `target\dist` folder, or in a git worktree, names that folder in its alias file, and every
  connection through the alias fails once the folder is gone
  ([operator.md, The install folder](docs/operator.md#the-install-folder)).
- **The lease CLI's checks are the conformance suite**
  ([docs/conformance.md](docs/conformance.md#running-the-suite)), which runs the installed
  programs, or a build's output folder, in a throwaway home
  ([docs/conformance.md, Naming the program under test](docs/conformance.md#naming-the-program-under-test)).
  The prototype's own `scripts\test-*.ps1` checks were removed with it (E0.06-T06-S01,
  2026-11-20). The CLI's first live run through the real box is
  [docs/runs/e0.05-t05-s01.md](docs/runs/e0.05-t05-s01.md).

### Lease host aliases

RFC 2119 keywords; the capitals are load-bearing.

Each lease gets one alias file, `~/.ssh/agentspace.d/agentspace-<project>-<shortid>`, written by
the lease CLI's `create` and deleted by its `release`. `~/.ssh/config` reaches them through one
line:

```
Include agentspace.d/*
```

- **The line MUST be above the first `Host` or `Match` line.** An `Include` after a `Host` line
  belongs to that host's block. `create` adds it as line 1 when it is missing, and refuses,
  changing nothing, when it finds one below a `Host` line: a person has to move it.
- **The path resolves against `~/.ssh`**, not the current directory.
- **The first change backs up the config** as `config.agentspace-backup-<UTC time>` beside it,
  checks the copy, and then inserts the one line. Nothing else in the file changes, byte for byte,
  and its line endings and permissions stay as they were.
- **Removing an alias leaves the `Include` in place**, and leaves the lease's key folder alone.
- **`C:\Windows\System32\OpenSSH\ssh.exe -G <alias>`** prints the settings an alias gives.
- **The alias's ProxyCommand names the alias itself** (card #0057), written out literally at the
  end. The installed CLI writes
  `"%USERPROFILE%\.agentspace\bin\agentspace-ssh-proxy.exe" <alias>`, with the value of
  `USERPROFILE` written out: the ProxyCommand program that sits beside the CLI in the install
  folder (D8, D37). The install folder's `agentspace-ssh-proxy.cmd`, the forwarder that v0.05's
  install ran and that v0.06's replaced with one line running the `.exe`, is kept only for an alias
  file that already names it ([operator.md, Until v0.06 retires the prototype's entry points](docs/operator.md#until-v006-retires-the-prototypes-entry-points)).
  The prototype's lines named `<repo>\scripts\ssh-proxy.cmd`, which was removed by E0.06-T06-S01
  (2026-11-20). The line MUST NOT use ssh's `%n` token, because the Claude desktop app runs the
  ProxyCommand line through its own proxy (D12) and nothing shows that it expands tokens.
  `agentspace-ssh-proxy.exe` runs `up` for the alias (`up -Alias <alias>`), so the token push rides
  the lease's own key and certificate, not Terry's `agentspace` entry. An alias MUST match
  `^[A-Za-z0-9._-]+\z`, case sensitive, and MUST NOT start with `-`, since it is on a command line;
  the alias names the CLI generates always do, and it refuses one that does not. A project name
  MUST be ASCII. That check is the gate: the ProxyCommand passes its argument on as it is, because
  whoever writes a ProxyCommand line already runs commands, and `up` checks the alias again before
  any ssh ([operator.md, The ProxyCommand](docs/operator.md#the-proxycommand)). Nothing in a
  checkout is read at run time, so no checkout needs to exist for a connection (D37).

### Use a lease from a project

RFC 2119 keywords; the capitals are load-bearing.

How a project such as swimcoach runs its agents on a lease. The commands are in "The lease CLI"
above and [docs/lease-api.md](docs/lease-api.md), the alias in "Lease host aliases", and the board
in "The localswim board on the box" below. Story E3.03 ran these steps
([D9](docs/decisions.md#d9-reachability-belongs-to-the-driver),
[D12](docs/decisions.md#d12-board-access-is-unchanged)).

1. **Create the lease with the installed CLI**, from any folder:

   ```bat
   agentspace lease create --project swimcoach
   ```

   Where the install folder is not yet on `PATH`, call it by its full path,
   `%USERPROFILE%\.agentspace\bin\agentspace.exe`. The default ProxyCommand names the install's
   `agentspace-ssh-proxy.exe`, so no checkout needs to exist ([Install it](#install-it)). The JSON
   gives the lease's `id`, its `expires_at` (8 hours from now unless `--ttl` says otherwise) and
   `ssh.host_alias`, such as `agentspace-swimcoach-3a9c5e21`.
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
     `agentspace-keeper-task.exe -Watch`, a no-window launcher in the install folder, which runs
     `agentspace-keeper.exe` beside it. While a lease is ready and no holder runs, the keeper
     starts `ssh.exe -N -o ExitOnForwardFailure=yes <alias>` through the ready lease that expires
     last, and it stops a holder whose lease is no longer ready.
   - **`agentspace-keeper-install` registers the task.** `-DryRun` prints every field of the task
     it would register and changes nothing, and `-Uninstall` removes the task and the prototype's
     launcher, `BoardHolderLauncher.exe`, if one is left. Registering the task
     is persistent configuration on Terry's laptop, so it MUST only happen with his yes
     (E0.02-T03-S01; [keeper.md, The installer](docs/keeper.md#the-installer)).
   - **The installer MUST be run from the install folder, never from a build's output folder or a
     worktree.** The task points at the programs beside the installer, and a worktree is deleted
     after its branch merges. An install from anywhere else is refused with exit 2, and `-DryRun`
     there prints a `Warning:` line.
   - **The keeper logs to `%USERPROFILE%\.agentspace\board-holder.log`**, and the holder's own
     output goes to `board-holder-ssh.log` beside it, which is rotated to `board-holder-ssh.log.1`
     past 1 MB.
   - **A holder that fails fast is retried with backoff**, from 30 seconds doubling to 5 minutes,
     never in a tight loop. Only a holder seen alive after 60 seconds resets the backoff.
   - **The keeper waits out a dead holder's release before it starts the next one**
     (E0.02-T04-S03, D30). A holder it started, or adopted, that is seen to exit leaves a session on
     the box that holds the forwards until sshd drops it, up to about 90 seconds. So the keeper
     waits `-ReleaseGraceSeconds` (95 by default) first, and logs `grace seconds=<n>`. When a fast
     failure's backoff also applies, it waits the longer of the two, never their sum. This covers
     a holder the keeper stopped because its lease ended too. Only a keeper that has had no holder
     starts one at once.
   - **No error ends the keeper.** It logs the error and carries on at its next interval, so only
     bad arguments or a killed process can stop it.
   - **Once v0.08 is installed, the keeper also starts the guard**
     ([A host's life](#a-hosts-life)); a stopped keeper stops it too, and each host then bounds
     itself by its horizon.
   - **A crashed keeper is started again within about a minute** by the task's second trigger,
     which repeats every minute, indefinitely, from registration (E0.02-T04-S02, D30). With
     `MultipleInstances IgnoreNew`, a firing while the keeper runs is ignored. The task's
     RestartCount did not restart a keeper whose action exited non-zero in E0.02-T03-S01, so the
     repeating trigger, not RestartCount, is what brings a crashed keeper back (D30). The crashed
     keeper's holder keeps running meanwhile, and the new keeper adopts the recorded holder. The
     task runs only while Terry is logged on.
   - **One keeper runs at a time** (E0.02-T05-S01, D30). `-Watch` holds
     `board-holder-keeper.lock`, in the state folder, for its whole life. A second keeper, such as
     one the task's next firing starts, logs `<time> another keeper is running pid=<n>` and exits 0
     at once, without a pass. The lock is released when the keeper's process ends for any reason,
     a kill included. A one-pass run does not use it.
   - **End in Task Scheduler stops only the launcher** and leaves the keeper's process running; the
     next firing's keeper then exits on the lock and does not loop beside it (E0.02-T04-S03,
     E0.02-T05-S01). The first keeper keeps the board held meanwhile, and each firing logs one
     `another keeper is running` line, one a minute, until that keeper is stopped. To stop the
     keeper, its process (`agentspace-keeper.exe`) MUST be stopped by its checked PID after the
     task is ended. To keep it stopped, the task MUST also be disabled (`Disable-ScheduledTask`) or
     removed with `-Uninstall`; `-Uninstall` ends the task the same way, so its keeper's process
     MUST still be stopped by PID. A re-registration is refused while a keeper runs, for the same
     reason ([keeper.md, Stopping and starting the keeper](docs/keeper.md#stopping-and-starting-the-keeper)).

   Only one connection on the box can hold the forwards (D12), so the holder exits rather than
   run without them when another connection already has them. An interactive `ssh <alias>` that
   holds them needs no holder. On the box, `localswim board list` checks the board; the wrapper
   refuses with a stop-and-report message when it is unreachable.
5. **Release the lease** when the work is done. Any folder MAY run it, since the lease's state is
   in `%USERPROFILE%\.agentspace` and release needs no ProxyCommand:

   ```bat
   agentspace lease release <id>
   ```

   It deletes the alias file and the lease's key folder and leaves the `Include` line. It does
   not revoke the certificate
   ([D10](docs/decisions.md#d10-releasing-a-lease-does-not-revoke-its-certificate)). **Once
   v0.08 is installed, it ends every ssh client on the laptop whose destination is the lease's
   alias, the keeper's holder included, before it deletes the alias**
   ([D59](docs/decisions.md#d59-a-release-ends-the-leases-own-connections-on-the-laptop); its
   exact behavior is [lease-api.md](docs/lease-api.md)'s and [driver.md](docs/driver.md)'s), and
   the host drops each ended session about 90 seconds later. **Archiving a desktop-app session
   does not close its SSH connections** (E0.02-T03-S01); the release ends them, and no session
   keeps a host up once the project holds no live lease: the guard stops it within 25 minutes of
   the release ([A host's life](#a-hosts-life)). The desktop app's SSH environment SHOULD then be
   removed, since its alias no longer exists. The project's checkout stays on the box.

   **A closed holder keeps its forwards on the box for about 90 seconds.** The Session Manager
   tunnel does not tell sshd that the client has gone, so the box drops a dead holder's session,
   and frees 8811 and 53147, about 90 seconds after the session goes quiet (D12's 0.02.T1.1
   finding, card #0044). A new holder started before then exits under `ExitOnForwardFailure`, so
   the keeper retries it with backoff. Only a session still holding them after 2 minutes is cleared by hand:
   it MUST be cleared as D12's E3.03 finding says.

#### Measure a turn

RFC 2119 keywords; the capitals are load-bearing.

How a project's agent measures the CPU and memory of a turn on its lease
([D33](docs/decisions.md#d33-v003-measures-each-agent-sessions-process-tree-split-into-the-session-and-its-tool-calls)).
It is written for any project; agentspace is only the first to use it. Two tools do it:
`agentspace-sample` records every Claude Code session on the box, and `agentspace-footprint`
summarizes the record.

- **A turn is the work between `start` and `stop`.** It MUST run in Claude Code sessions on the
  box, over `ssh <alias>` or the desktop app's SSH environment (lease step 3). A session on the
  laptop is not recorded, so the agent that runs the steps below from the laptop is not part of
  the measurement.
- **The record covers every Claude Code session on the box, every project's.** Each session row
  is labelled with the last folder of its cwd and the first 8 characters of its `session` id,
  such as `myproject 41872-35`. A session whose working directory cannot be read is recorded
  with cwd "", and agentspace-footprint labels it `?`. The project's own rows are the ones whose
  folder is its checkout, and `--json` lists each session's cwd in full.
- **The host and concurrency figures are box-wide.** They include other projects' sessions, the
  sampler, and work outside any session. A project MUST read its turn from its own session rows,
  or make sure no other project's session runs during the turn.
- **`<alias>` is the lease's `ssh.host_alias`** (lease step 1). **`<run>` is a name the agent
  picks**, which MUST match `^[a-z0-9][a-z0-9-]{0,63}$` and MUST be new for every measurement,
  such as `myproject-t05`.

Before step 1:
- **The lease MUST be created** (lease step 1), and its `expires_at` MUST leave the turn plus a
  margin, since the host is stopped a grace after it ([A host's life](#a-hosts-life)). `--for`
  MUST end before `expires_at`, and SHOULD be the turn's expected length plus about half again:
  when the alias goes, the record is stranded on the box, where only a new lease reaches it.
- **Both tools are installed on the box by agentspace's `sync`, which agentspace's architect
  runs.** A project's agent MUST NOT run `sync`. When a tool is not found on the box, the agent
  MUST report it to agentspace's architect.
- **From Git Bash, `ssh` and `scp` MUST be called by full path**, as in lease step 3.
  `/c/Windows/System32/OpenSSH/ssh.exe` is Git Bash's spelling of
  `C:\Windows\System32\OpenSSH\ssh.exe`. From cmd or PowerShell, plain `ssh` and `scp` are the
  same programs. Every command below passes `-o BatchMode=yes`, so a refused login fails at once
  and MUST NOT be retried in a loop.

The steps MUST run before lease step 5, because the record has to be fetched first.

1. **Start the sampler** before the turn starts, with a `--for` that fits the turn (here 3 hours):

   ```bash
   /c/Windows/System32/OpenSSH/ssh.exe -o BatchMode=yes <alias> "agentspace-sample start <run> --for 3h"
   ```

   It starts the sampler detached, so it keeps running after this connection closes, prints the
   record's path (`~/agentspace-samples/<run>.jsonl`) and the sampler's PID, and exits 0. The
   other defaults suit a turn: a sample every 2 seconds, and the default match. Without `--for`
   the deadline is 8 hours away. They MAY be changed with `--interval <seconds>` (0.5 to 60),
   `--for <duration>` (`30m`, `8h` or `90s` style; above 12h is refused) and `--match <regex>`.
   - **Exit 2** means the record already exists, a sampler is already running (one runs on the
     box at a time), or an argument was refused; the message on stderr says which. For an
     existing record, the agent MUST pick a new `<run>` and repeat step 1. For a running sampler,
     it MUST run `agentspace-sample status` the same way, MUST NOT stop a run it did not start,
     and MUST wait for that run to end or ask agentspace's architect.
   - **Exit 1** means the sampler did not start: it exited, or wrote no start line within 5
     seconds and was stopped. The agent MUST report the message, which names
     `~/agentspace-samples/<run>.error` when the sampler left one.
   - **`--match` is a Python regex**, searched in each process's full command line with NUL bytes
     read as spaces. The default, `^(?:\S*/)?(?:claude|ccd-cli/[^/\s]+)(?:\s|$)`, matches a
     process whose first argument is `claude` or a path ending in `/claude`, and a desktop-app
     session's `~/.claude/remote/ccd-cli/<version>`. A summary with no session rows means
     the match missed, and the agent MUST NOT read it as a zero footprint.
2. **Confirm it is running:**

   ```bash
   /c/Windows/System32/OpenSSH/ssh.exe -o BatchMode=yes <alias> "agentspace-sample status"
   ```

   It prints the running sampler's run, PID, samples so far and deadline, or
   `No sampler is running.`, and exits 0 either way. The agent MUST wait at least one interval
   (2 seconds by default) between two calls, and the samples so far MUST rise. When they do not,
   the agent MUST report it and MUST NOT start a second sampler.
3. **Run the turn.** The agent SHOULD note the box's time at its start and at its end, which
   gives `--from` and `--to` their values in step 6:

   ```bash
   /c/Windows/System32/OpenSSH/ssh.exe -o BatchMode=yes <alias> "date -Iseconds"
   ```
4. **Stop the sampler** as soon as the turn is over:

   ```bash
   /c/Windows/System32/OpenSSH/ssh.exe -o BatchMode=yes <alias> "agentspace-sample stop <run>"
   ```

   The sampler writes its end line within one interval. `stop` waits up to three intervals,
   prints the end line's reason, and exits 0. It does the same for a run that has already ended,
   such as at its deadline.
   - **Exit 1** means the record has no end line: the sampler still runs, or was killed, or the
     record was deleted during the run, which the message says. The agent MUST wait one more
     interval and run `status`. If the sampler still runs, the agent
     MUST end it with `kill <PID>`, the PID that `start` printed and `status` shows, which writes
     the end line with the reason `signal`, and MUST report it. A record with no end line is
     still summarized, and `agentspace-footprint` says so.
   - **Exit 2** means no record has that `<run>`.
   - **The sampler also ends by itself at its deadline**, with its end line. A forgotten one keeps
     the box awake until then, or until the guard or the lease horizon stops the host, because
     its own CPU counts as activity to the idle check (D33; [A host's life](#a-hosts-life)), so a
     `--for` much longer than the turn SHOULD NOT be given.
5. **Fetch the record to the laptop**, after `stop` and before the lease is released. The agent
   MUST `cd` to the folder that is to hold the record first, and give `.` as the destination:

   ```bash
   /c/Windows/System32/OpenSSH/scp.exe -o BatchMode=yes <alias>:agentspace-samples/<run>.jsonl .
   ```

   The path after the alias is relative to the box's home. The record stays on the box too, also
   after the lease is released.
6. **Summarize it:**

   ```
   agentspace-footprint <record.jsonl> [--from <ISO 8601>] [--to <ISO 8601>] [--json]
   ```

   - **On the laptop**, from the folder the record was fetched to, the one form that works in
     cmd, PowerShell and Git Bash is:

     ```
     py -3 C:/Projects/agentspace/box/system/agentspace-footprint <run>.jsonl
     ```

     The script is in agentspace's main checkout, `C:\Projects\agentspace`, where one exists. A
     project without that checkout MUST summarize on the box instead, over the lease and before
     it is released, which needs no fetch:

     ```bash
     /c/Windows/System32/OpenSSH/ssh.exe -o BatchMode=yes <alias> "agentspace-footprint agentspace-samples/<run>.jsonl"
     ```
   - **`--json`** prints one JSON object instead of Markdown, with the keys `run`, `host`,
     `sessions`, `concurrency` and `notes`.
   - **`--from` and `--to`** keep only the samples inside that window, both ends included. Each
     MUST carry a UTC offset, such as `2026-11-18T01:45:00-08:00`, the form `date -Iseconds`
     prints in step 3. The agent SHOULD use them when the sampler ran longer than the turn.
   - **Exit 0, or 2 with a message on stderr** for a missing file, a record with no valid start
     line, or bad arguments, such as a `--from` or `--to` without a UTC offset. A malformed line
     is skipped and counted in the notes, never fatal. A record with no end line is summarized up
     to its last complete line, and the notes say so.

   The output gives:
   - **A header:** the run's span, its samples, its interval, and the sampler's own overhead.
   - **The host:** CPU cores busy and memory used in GiB.
   - **One row per session:** its own process and its tools, apart, each as CPU cores and PSS in
     MiB, with the most tools processes at once.
   - **The concurrency:** the most sessions alive at once, and the sum of every session's tools
     CPU and PSS at each sample.
   - **The notes:** skipped lines, clamped negative differences, gaps and a missing end line.

   Every CPU and memory figure is given as peak, p95 and p50. The tools processes and the most
   sessions at once are given as peak only.

### A host's life

RFC 2119 keywords; the capitals are load-bearing.

**This section is v0.08's, and holds once v0.08 is installed** (E0.08-T05-S01, #0186). Until then
no guard runs, no host holds a lease horizon, the backstop stops a host after 3 hours averaging
under 1% CPU, a release ends no connection, and every host a turn starts is stopped at its close
(D51).

**A host runs only while its project holds a live lease** (v0.08;
[D56](docs/decisions.md#d56-a-host-runs-only-while-its-project-holds-a-live-lease-the-laptops-guard-stops-it-otherwise),
[D57](docs/decisions.md#d57-a-host-bounds-itself-by-its-lease-horizon-while-the-laptop-is-away),
[D58](docs/decisions.md#d58-the-cloudwatch-backstop-stays-lengthened-from-3-hours-to-24),
[D60](docs/decisions.md#d60-status--all-is-the-one-command-that-lists-every-host-with-its-live-leases)).
A live lease explains its host whatever the host is doing, a quiet lease with no session and no
CPU work included. What counts as a project's live lease, its last lease end and its horizon is
[state.md, Live leases and the lease horizon](docs/state.md#live-leases-and-the-lease-horizon)'s.
Three things stop a host:

| Stop | Where it runs | When |
|---|---|---|
| The guard | The laptop, while the keeper runs | Within 25 minutes of the project's last live lease's end, by release or by expiry |
| The idle check | The host | After 60 minutes with no session, no patch run and no CPU work; and, past the host's lease horizon plus the grace, at the first check with no session and no patch run, whatever its CPU is doing |
| The backstop | CloudWatch | After 24 hours averaging under 1% CPU, for a host whose idle check has died |

- **The guard is `agentspace-operator guard`**
  ([operator.md, guard](docs/operator.md#guard)). The keeper starts it at its first pass and then
  at most once every 5 minutes, never while one runs
  ([keeper.md, Starting the guard](docs/keeper.md#starting-the-guard)). In one
  run it reads `hosts.json`, the lease state file and every host's EC2 state, and stops each
  running host that no live lease explains, as `down -Host` stops it, once the grace has passed
  since the later of the host's launch and its project's last lease end. It never stops a host
  whose provisioning run is under way.
- **The grace is 15 minutes, a constant** (Terry, 2026-11-21). With up to 5 minutes until the next
  guard run and the rest for the stop itself, a host is `stopped` within 25 minutes of its last
  live lease's end. A host started with no lease at all is stopped within 25 minutes of its launch.
- **A stopped host stays `ready`** in the host registry, and its project's next lease starts it with
  its first connection, as before.
- **Each guard run sends every running host its lease horizon, or its launch time when that is
  later**, in short the latest `expires_at` among its project's live leases, which the host keeps
  until its next boot ([operator.md, guard](docs/operator.md#guard)). With the laptop off or
  asleep, the guard cannot run, and the host bounds itself: an idle host stops after 60 minutes,
  as it always has, and a busy one no later than its horizon, the grace and one 5-minute check
  after its last session or patch run ends. A host that has just booted holds no horizon until the
  next guard run, and keeps the 60-minute rule alone until then.
- **`agentspace-operator status -All` shows it**, after each host's existing lines: `Leases` with
  the count of its live leases, and, for a running host no live lease explains, `Guard` with
  `unexplained` and the time the guard stops it ([operator.md, status](docs/operator.md#status)).
  It prints counts, never lease IDs. When the lease file cannot be read, it prints the host lines
  without `Leases` and `Guard`, and says why once ([operator.md, status](docs/operator.md#status)).
- **The guard logs to `%USERPROFILE%\.agentspace\guard.log`**: one line for each host it stops,
  finds unexplained, or fails on, a failed horizon send included, and nothing for a host that is
  explained or stopped. A run whose AWS call fails, an expired sign-in included, logs that and
  stops nothing. So does a run that cannot read the lease state file, or finds it missing or
  invalid: a missing lease file is never taken as no leases, so a lost `leases.json` never costs a
  working agent its host. The file's form is
  [state.md, The guard's files](docs/state.md#the-guards-files)'s, and each line's text
  [operator.md, guard](docs/operator.md#guard)'s. An unexplained host is reported there and by
  `status -All`, and nowhere else (Terry, 2026-11-21): nothing reaches his phone, since the guard
  stops it on its own.
- **Nothing renews a lease on its own.** A lease ends at its `expires_at` unless whoever holds it
  renews it
  ([D24](docs/decisions.md#d24-a-leases-ttl-is-8-hours-by-default-at-least-1-minute-and-at-most-24-hours)),
  and its host is stopped a grace later. Whoever needs a host past its lease's `expires_at` MUST
  renew the lease before then. Sessions open at expiry stay open, since a certificate is checked
  only at login, and end when the host stops. The architect's turns SHOULD give a dev lead's lease
  a TTL that covers the turn, and renew it when a turn runs long.
- **Work on a host by hand MUST be done under a live lease of its project**: the sign-in below, a
  `sync`, `grow` or `bootstrap` run by a person, and any direct session. Without one, the guard
  stops the host within 25 minutes of the later of its launch and its project's last lease end,
  whatever is running there.
- **Every host is guarded, the box included** (Terry, 2026-11-21). Direct work on the box through
  his own `agentspace` entry is done under a swimcoach lease, as on any host.

### Signing in on a new host

RFC 2119 keywords; the capitals are load-bearing.

**A host the operator made for a project is `ready` for leases before any tool on it is signed in**
(D52; `ready` means a connection is expected to succeed, not that an agent can work there yet). This
procedure is done by hand, once per new host, by Terry, on his laptop. Card #0009 automates it
after 1.0. The operator's provisioning run ends by printing where it is:
`Sign in on this host once, by hand, under a lease: see "Signing in on a new host" in the
agentspace README.` (architect, 2026-11-21, for v0.08-T01; built by E0.08-T03-S05, so a run of
v0.07's operator still prints the line it had). Only a run started by hand shows that line: a run
a lease starts is detached, its output goes to a hidden console that nothing reads, and its log is
its only record
([operator.md, provision](docs/operator.md#provision)).

1. **Create a lease for the project first**, and keep it live until step 8 releases it
   ([D56](docs/decisions.md#d56-a-host-runs-only-while-its-project-holds-a-live-lease-the-laptops-guard-stops-it-otherwise)).
   Without one, the guard stops the host within 25 minutes of the later of its launch and its
   project's last lease end, mid sign-in or not ([A host's life](#a-hosts-life)):

   ```bat
   agentspace lease create --project <project>
   ```

   The full path, `%USERPROFILE%\.agentspace\bin\agentspace.exe`, is used until the install
   folder is on `PATH`. Its JSON gives the lease's `id` for step 8. A lease that would end before
   the sign-in does MUST be renewed before its `expires_at`, since nothing renews it on its own.
2. **Start once the host is `ready`**: its log,
   `%USERPROFILE%\.agentspace\hosts\<project>\provision.log`, ends `run done`, and step 3's
   `status -Host <project>` shows the host `ready`. Only a project listed in
   `%USERPROFILE%\.agentspace\projects.json` gets a host (D54), and adding one takes Terry's yes.
   A lease for the project answers `pending` until its host is ready.
   - **Until card #0163 is fixed, a new host's first provisioning run fails at `sync`**: sync's
     connections after its install wait out `up`'s 15-minute deadline until the host's launch is
     20 minutes old, and the log ends `sync failed`. The lease stays `pending`. In E0.07-T05-S01
     (2026-11-21) the first run failed about 34 minutes after it began
     ([docs/runs/e0.07-t05-s01.md](docs/runs/e0.07-t05-s01.md), finding 2). Only a `lease create`
     or `lease get` for the project starts the next run, at most one every 5 minutes from the last
     run's start (D48), so whoever waits on the lease SHOULD run
     `agentspace lease get <lease id>` once the log ends `sync failed`. That run made the host
     `ready` in about 2 minutes.
3. **Sign the operator's one-hour certificate:**

   ```bat
   %USERPROFILE%\.agentspace\bin\agentspace-operator.exe status -Host <project>
   ```

   The full path is used until the install folder is on `PATH`
   ([The lease CLI](#the-lease-cli)). The certificate lasts an hour, so the same command MUST be
   run again after an hour, before any new `ssh` to the host
   ([operator.md, How the operator reaches a host](docs/operator.md#how-the-operator-reaches-a-host)).
4. **Connect, with Codex's port forwarded:**

   ```bat
   ssh -L 1455:localhost:1455 agentspace-host.<project>
   ```

   The alias `agentspace-host.<project>` is written by the provisioning run, and written again by
   any operator command given `-Host <project>`, step 3's `status` included, whenever its bytes
   differ ([operator.md, How the operator reaches a host](docs/operator.md#how-the-operator-reaches-a-host)).
5. **Sign in to each tool at the host's prompt, as [Signing the tools in](#signing-the-tools-in)
   gives them:** Claude Code
   (`claude`, then its URL-and-code option), Codex (`codex login`, its URL opened in the laptop
   browser) and gh (`gh auth login`, HTTPS and the web browser).
6. **Clone the project's checkout after gh's sign-in**, never before it, since a private
   repository needs gh's credential (D52):

   ```sh
   gh repo clone <owner>/<repo> ~/<project>
   ```

7. **The host's laptop files are agentspace's and MUST NOT be edited by hand:** the operator's alias
   `agentspace-host.<project>` and its key. A new host adds nothing to Terry's `~/.ssh/config` but
   agentspace's `Include` line, when it is missing, and never writes his `known_hosts` (D47);
   agentspace's host keys are in `%USERPROFILE%\.agentspace\known_hosts`, written only by ssh.
8. **Release the lease** once the sign-ins and the clone are done, with step 1's `id`. Step 4's
   connection is the operator's, which the release does not end; close it first:

   ```bat
   agentspace lease release <id>
   ```

   The guard then stops the host within 25 minutes, unless another live lease of the project
   explains it ([A host's life](#a-hosts-life)).

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
- **`agentspace-operator` passes `ClearAllForwardings=yes` to its own ssh and scp calls**, so they
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
  ProxyCommand runs `agentspace-ssh-proxy.exe <alias>`, which runs `up -Alias <alias>`, so the push
  uses that lease's key and certificate. Terry's `agentspace` entry has no argument and still
  works: `agentspace-ssh-proxy.exe` runs `up` and the push goes over `agentspace`, as before. `sync` still pushes
  over `agentspace`, as intended. The push's ssh runs with `BatchMode=yes`, so an alias that
  resolves to an unknown host fails at once rather than waiting on a prompt.
- **A send is recorded only once the box confirms it.** The box reads two lines, writes them to
  a temporary file, renames it over `bot.env` and sends back the file's SHA-256, which `up`
  compares with what it meant to write. A failed, unconfirmed or interrupted send leaves the old
  `bot.env` in place, records nothing, and is tried again at the next connection. The send's ssh
  is stopped after 20 seconds, so a hang delays a connection by at most that.
  `conformance\operator\test-unmet-push.ps1` checks this without the box
  ([docs/conformance.md, Running the suite](docs/conformance.md#running-the-suite)).
- **The same push sets the box's time zone to the laptop's**
  ([D32](docs/decisions.md#d32-a-leases-box-runs-in-the-laptops-time-zone)), so `date` on the box
  gives Terry's local time and UTC offset. `up` works out the laptop's zone as an IANA name, such
  as `America/Los_Angeles`, and maps the seven legacy names that Windows gives and the box lacks,
  such as `Asia/Calcutta`, to their current ones, such as `Asia/Kolkata`. Once `bot.env` is in place and its SHA-256 sent back, the box runs
  `sudo -n timedatectl set-timezone <zone>` and answers with one more line, `zone <zone>` or
  `zone-failed`.
  - **The zone is part of the stamp**, so a laptop that has changed zone pushes again at its next
    connection.
  - **The zone never puts the token at risk.** A zone the laptop cannot work out is not sent, and
    `up` says so. A zone the box refuses leaves the box's zone as it was, records nothing, and is
    asked again at the next connection. Either way the token is still sent and confirmed as above,
    and the board works.
  - **Only a checked zone reaches the box's command line:** letters, digits, `_`, `+` and `-`, in
    segments joined by `/`, not starting with `-`. The zone travels on the command line, while the
    token stays on stdin.
- **The wrapper sets the URL and token for one run only**, and refuses with a stop-and-report
  message when the board is unreachable, so an agent learns why instead of seeing a timeout.
- **A project's own localswim wrapper still decides how that project's agents call it.**
  swimcoach's `scripts/localswim.sh` runs this wrapper on the box; its Windows twin,
  `scripts/localswim.ps1`, reads the laptop's receipt directly.

## Build

RFC 2119 keywords; the capitals are load-bearing.

```bat
cargo xtask dist [--release]
```

- **`cargo xtask dist` builds the workspace's programs and writes `target\dist\`**, which holds
  exactly eight files, and removes anything else there: the six programs, `agentspace.exe`,
  `agentspace-operator.exe`, `agentspace-ssh-proxy.exe`, `agentspace-keeper.exe`,
  `agentspace-keeper-task.exe` and `agentspace-keeper-install.exe`; the `agentspace` wrapper that
  Git Bash and other POSIX shells find; and `agentspace-ssh-proxy.cmd`, a forwarder of one fixed
  line that runs the `.exe` beside it
  ([operator.md, The programs](docs/operator.md#the-programs);
  [operator.md, Until v0.06 retires the prototype's entry points](docs/operator.md#until-v006-retires-the-prototypes-entry-points)).
  No file holds a path, so a build MAY be run from any checkout or worktree.
- **The output folder is for checks.** A lease that a CLI in it creates names that folder in its
  alias file, so a clean-up breaks the lease's connections. Real leases SHOULD come from the install
  ([operator.md, The install folder](docs/operator.md#the-install-folder)).

### Install it

RFC 2119 keywords; the capitals are load-bearing.

**An install is a copy of one build's files into one folder, `%USERPROFILE%\.agentspace\bin`**
(D37; [operator.md, The install folder](docs/operator.md#the-install-folder)). No agentspace
program makes it: a person, or an agent at the person's request, does.

1. **Build in release mode:**

   ```bat
   cargo xtask dist --release
   ```

2. **Copy the files of `target\dist`** into `%USERPROFILE%\.agentspace\bin`, **creating the folder
   when it is missing.** Every file comes from the one build, so the programs in the folder come
   from one version of the repository.
3. **The install MUST NOT be run elevated, MUST NOT go under `%LOCALAPPDATA%` or `%APPDATA%`, and
   MUST NOT stop a running program.** Windows refuses to replace the file of a running program
   but lets it be renamed, and the ProxyCommand runs for as long as any connection through it is
   open. So a file that is running SHOULD be renamed aside in the folder as `<file>.old-<32
   lowercase hex digits>` and the new one copied in, and each `.old-` file that can be removed
   SHOULD be removed on a later install
   ([operator.md, The install folder](docs/operator.md#the-install-folder)). An install SHOULD NOT
   replace `agentspace-keeper.exe` while a keeper runs, and SHOULD stop the keeper first
   ([docs/keeper.md, Stopping and starting the keeper](docs/keeper.md#stopping-and-starting-the-keeper)).
   **After the copy, the person MUST stop, by its checked PID, any keeper the task's repeating
   trigger started during the install, and then start the task.** A running keeper whose start
   time is before the copy's end is the old build (found in E0.07-T05-S01, 2026-11-21: the
   trigger started the old keeper 2 seconds before the copy ended, finding 1 of
   [docs/runs/e0.07-t05-s01.md](docs/runs/e0.07-t05-s01.md)). Disabling the task first stays the
   alternative, and on Terry's laptop it MUST have his yes (D41). An install whose copy fails part
   way SHOULD be run again before any program in the folder is used.
4. **Add the folder to the user's `PATH`, which MUST be done only with Terry's yes**, so that
   `agentspace` and `agentspace-operator` run by name.

**An install no longer reaches into a checkout** (E0.06-T06-S01, 2026-11-20). v0.05's forwarder,
`agentspace-ssh-proxy.cmd`, ran the prototype's `scripts\ssh-proxy.cmd` from the main checkout,
which the build had to be told about. v0.06's replaced it with one fixed line, and the prototype
was removed from the repository
([operator.md, Until v0.06 retires the prototype's entry points](docs/operator.md#until-v006-retires-the-prototypes-entry-points)).

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
- **The box MUST run in the laptop's time zone**
  ([D32](docs/decisions.md#d32-a-leases-box-runs-in-the-laptops-time-zone)), so agents there
  report times in Terry's local time and offset under his rule, unchanged. The token push sets it.
  It MUST be set as the system zone, never by exporting `TZ` or by editing the box's copy of
  `CLAUDE.md`. Machine-readable timestamps keep their own form.
- **The disk SHOULD stay above about 70% used**, by Terry's preference: grow it in small steps
  when it passes 85%. EBS volumes cannot shrink, and allow one change every 6 hours.
- **Long agent runs SHOULD stay attached to a session or keep the CPU busy, and MUST run under a
  live lease that covers them.** The idle check counts an SSH or Session Manager session, or 6
  CPU-seconds of work per 5 minutes, as activity; an agent left alone and waiting for 60 minutes
  gets the host powered off. An agent run left on a host with the laptop away keeps the host up
  only until its lease horizon, plus the 15-minute grace and one 5-minute check, whatever its CPU
  is doing
  ([D57](docs/decisions.md#d57-a-host-bounds-itself-by-its-lease-horizon-while-the-laptop-is-away)),
  so its lease MUST be renewed to cover the run before the laptop goes away, and SHOULD be renewed
  at least 5 minutes before, since the host learns the new horizon only at the next guard run.
  With the laptop on, the guard stops the host within 25 minutes of the lease's end
  ([A host's life](#a-hosts-life)).
- **Credentials MUST come from `aws login`**, never from long-lived access keys. When a sign-in
  expires, run `aws login --remote` and open the URL it prints in a fresh Incognito window, with
  every other Incognito window closed first; a browser holding another AWS session returns a 400.
  `--remote` prints the URL instead of opening the default browser, and asks for the code the
  page shows.
- **Account-specific values and secrets MUST NOT be committed:** this repo is public.
  Account-specific parameters go in `%USERPROFILE%\.agentspace\stack-parameters.json`, outside the
  repo; tokens never touch the repo.
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

The guard's log is on the laptop, `%USERPROFILE%\.agentspace\guard.log`
([A host's life](#a-hosts-life)). How many connections each release ended is in
`%USERPROFILE%\.agentspace\release.log`.

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

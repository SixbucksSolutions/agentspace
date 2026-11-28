# E0.12-T05-S02: the expected values of the windows/amd64 live check, written before any dry run

RFC 2119 keywords; the capitals are load-bearing. Drafter-owned, in `target\t05`. **Every expected value is followed by the doc line it comes from**, quoted from the file with the line number a `Select-String` found when this file was written (`s02-make-expected.ps1` writes it and stops when a quote is not found). A value no doc states is marked **drafter's reading**, with the reason.

## How a step is run, and what its exit code means

- **Each step is `target\t05\s02-step-<nnn>.ps1`, run as** `pwsh -NoProfile -File scripts\live\wrap.ps1 -Out target\runs\runner -Name <nnn>-<what> -Script target\t05\s02-step-<nnn>.ps1`. A used name is refused, so a rerun takes the next number. Nothing is passed as a parameter: the knobs are `S02_*` environment variables that a live run MUST NOT set (the loops' bounds: `S02_POLL_SECONDS` 60, `S02_MAX_SECONDS` 5400, `S02_IDLE_SECONDS` 720, `S02_STOP_SECONDS` 2100; the lease's TTL `S02_TTL` 4h; `S02_SYSTEM_WINGET=0` skips the transient SYSTEM probe of 315i).
- **Exit codes:** 0 PASS; 1 a FINDING (each printed as a `FINDING:` line; a failure is a finding and is never retried past); 2 the lease ended `unsatisfiable`, `expired` or `released` (311); 3 a loop's bound was reached (311 at 90 minutes, 341 at 35); 5 STOP, a precondition failed before anything ran (a printed `STOP:` line; also any `AGENTSPACE_*` variable set in a live run); 6 windows/amd64's hours reached 4.5 (every loop, from the observer's `hours.json`).
- **Programs:** the installed `agentspace.exe` and `agentspace-operator.exe` by `%USERPROFILE%\.agentspace\bin\`, `ssh`, `scp` and `aws` from the PATH, and `C:\Projects\localswim\localswim.exe` by its full path. No script lists, reads, copies or hashes an installed program, and none names `up`, `sync` (except the optional 316), `bootstrap`, `provision`, `grow`, `deploy` or an arm64 host.
- **Printing:** every line goes through `ConvertTo-LiveMasked` after a replacement of the EC2 computer name (`EC2AMAZ-XXXXXXX`), which `scripts\live\mask.ps1` does NOT cover (shown by running it on `EC2AMAZ-K3F9Q2L\agent`: unchanged). A lease ID or alias is only ever in `target\t05\s02-state\` and prints as `<lease>` or `<alias>`. Times print `h:mmam` in the laptop's zone, read from the system at each step's start.

## Step 300: the mask proof

- Exit 0; the proof's last lines `Mask proof: <n> passed, 0 failed.` and the four lines `COMPUTERNAME removed`, `USERNAME removed`, `USERPROFILE removed` and `USERPROFILE short form removed`, each `True`. The output of the step ends `PASS: every check of this step held.`
  - scripts/live/mask-proof.ps1:194: `"Mask proof: $proofPassed passed, $proofFailed failed."``
  - scripts/live/mask-proof.ps1:15: `#   COMPUTERNAME removed, USERNAME removed, USERPROFILE removed, USERPROFILE short form removed.``
  - docs/architect.md:577: `the first no-op deploy). A wrapper script runs the command with its streams to files and writes``

## Step 301: the AWS sign-in

- Exit 0 and the line `aws sts get-caller-identity exit: 0`; nothing of the identity is printed (its stdout and stderr are read into memory and dropped).
  - docs/architect.md:613: `- **The AWS sign-in is checked by exit code before a live turn starts**, never by printing its``
  - docs/architect.md:625: `- **A sign-in check sends its stdout to '$null' inside the wrapped command**, so nothing reaches``
- A nonzero exit is a FINDING whose text carries the request for Terry: `aws login --remote`, the link pasted into a fresh Incognito window, every other Incognito window closed first.
  - AGENTS.md:171: `- **When the AWS sign-in has expired, the request MUST name 'aws login --remote', and say to paste``

## Step 302: before the lease

- `status -All` exits 0 (its report goes to stderr) and every host reads `State     stopped`, but `agentspace.linux-amd64`, whose state is printed and not judged (S01's check leaves it where it leaves it). The windows host is absent from the registry or stopped. Printed lines are masked (`<instance-id>`, `<vol-id>`).
  - docs/operator.md:3816: `Host      agentspace.linux-amd64 (agentspace-linux-amd64-host-agentspace), ready``
  - docs/operator.md:3762: `**From v0.12 'status -All' prints the platform of each host off 'linux/arm64'** (D60; D91:``
  - docs/decisions.md:5095: `release lets the guard stop it within D56's bound. The arm64 host is not started.``
- `lease list` is counted by state (`Leases by state: ...`, and the windows/amd64 leases by state) and no lease is `ready` or `pending`.
  - docs/lease-api.md:2206: `**Prints one lease as it stands now.** It changes no lease. A 'ready' lease whose 'expires_at' has``

## Step 310: Lease W

- Command `agentspace lease create --platform windows/amd64 --project agentspace --ttl 4h` (the TTL flag's form is `--ttl <duration>`, `4h` in the short form; pin P3 pending, `S02_TTL` carries another), no `--size`, so the default size `m`.
  - docs/lease-api.md:1991: `| '--ttl <duration>' | [TTL](#ttl) | '8h', or with a plan the TTL its turn gives ([TTL](#ttl)) | From 1 minute to 24 hours. |``
  - docs/lease-api.md:1677: `- **Accepted, short form:** one or more parts, each a whole number of decimal digits (leading zeros``
  - docs/lease-api.md:1990: `| '--size <size>' | [Size](#size) | 'm', without a plan | 's', 'm', 'l' and 'xl' are supported in v2, as from v0.09 (#0049). Not with '--plan', '--board' or '--epic'. |``
- Exit 0, stderr empty, stdout one JSON line ending in one LF; state `pending`; platform `windows/amd64`; profile `agent-windows` (the platform's default); project `agentspace`; size `m`; `ttl_seconds` 14400; members exactly `created_at,id,platform,profile,project,size,state,ttl_seconds` (no `expires_at`, `ssh` or `reasons` while pending, and no `sized_for` without a plan); the document validates against `schemas\lease.output.schema.json`.
  - docs/lease-api.md:1963: `- **'pending':** call 'get' until it is 'ready' or 'unsatisfiable'. From agentspace's v0.07 release``
  - docs/lease-api.md:1985: `| '--profile <name>' | [Profile](#profile) | The platform's default profile: 'agent-linux' on 'linux/arm64' and 'linux/amd64', 'agent-windows' on 'windows/amd64', and 'agent-linux' on any ot...``
  - docs/lease-api.md:690: `| 'ttl_seconds' | Integer, 60 to 86400 | Always | The TTL of the current certificate, or of the one asked for if none was issued. |``
  - docs/lease-api.md:694: `| 'ssh' | Object | 'ready' only | How to connect; see below. |``
- The log's line count before the create goes to `provlog-baseline.txt`; the raw document and the ID go to `lease-w.json` and `lease-w.id` and are never printed. A second run is a STOP while `lease-w.json` exists.

## Step 311: the wait

- One `lease get` a minute, each read printed with its time and state (`pending` until `ready`), the new masked lines of `%USERPROFILE%\.agentspace\hosts\agentspace.windows-amd64\provision.log`, and at the end each phase's first line time (local, with the log's UTC): `run begun`, `deploy done`, `start done` (the first connection), `sync done` (sync with its patch wait), `bootstrap done`, `record done`, `place done`, `run done`. Ends 0 at `ready`, 2 at `unsatisfiable`/`expired`/`released`, 3 at 90 minutes, 6 at 4.5 hours; a `... failed` line in the log is a FINDING at once.
  - docs/operator.md:3404: `- **'<time>'** is the time the line is written, in UTC, as 'YYYY-MM-DDTHH:MM:SSZ', the form of the``
  - docs/operator.md:3406: `- **'<step>'** is one of 'run', 'deploy', 'start', 'sync', 'bootstrap', 'record' and 'place', and,``
  - docs/operator.md:3412: `'left' (for 'place' alone; architect, 2026-11-23, for v0.09-T01, pin P37). A run's lines are``
  - docs/operator.md:3391: `'hosts\<project>.<os>-<arch>\provision.log' for a host not on 'linux/arm64' (D91). **The run opens``
  - docs/operator.md:2197: `| A host's names: '-Host example -Platform linux/amd64' naming the stack 'agentspace-linux-amd64-host-example' to the stand-in 'aws', the alias 'agentspace-host.example.linux-amd64' to the s...``
  - docs/lease-api.md:692: `| 'expires_at' | String, [time](#times) | 'ready' and 'expired'; 'released' if it was ever ready | When the current certificate stops being accepted for new connections: its issue time plus ...``
- **Drafter's reading, a finding for the dev lead:** a detached run copies no program output into `provision.log` (below, step 314), so the bootstrap's `winget answers` line and `sync`'s `Installed:` lines are NOT in it.
  - docs/operator.md:3459: `see why a run failed. **The programs' other output is not copied into the log**: the operator``

## Step 312: lease get

- Exit 0; state `ready`; platform `windows/amd64`; profile `agent-windows`; `ssh.user` `agent`; `ssh.host_alias` present (printed `True`, the alias never printed) and of the form `agentspace-agentspace-<8 hex digits>`; members `created_at,expires_at,id,platform,profile,project,size,ssh,state,ttl_seconds`; schema valid.
  - docs/lease-api.md:708: `**'ssh.host_alias' is 'agentspace-<project>-<first 8 hex digits of id>'**, for example``
  - docs/lease-api.md:719: `**'ssh.user' is the user the lease logs in as.** A lease on 'linux/arm64' or 'linux/amd64' logs in``
  - docs/lease-api.md:721: `lease on 'windows/amd64' logs in as 'agent', the Windows host's one local user (D95; architect,``
  - docs/decisions.md:4941: `other tenant. 'ssh.user' is 'agent', and the operator's own certificate names it too, with key ID``

## Step 313: the proofs

- `status -Host agentspace -Platform windows/amd64` exits 0; the report's first line is `Platform  windows/amd64`; `State     running`; `SSM       online`. For `agentspace-host.agentspace.windows-amd64` and the lease's alias: `ssh -G` exits 0 and shows a ProxyCommand line (printed `True`; the user line and the line itself are never printed); one `ssh -o BatchMode=yes -o ConnectTimeout=30 <alias> true` exits 0.
  - docs/operator.md:3748: `project's host on that platform (D91), and **for a host off 'linux/arm64' it prints one line more,``
  - docs/operator.md:3682: `then writes its alias and signs a fresh one-hour certificate for it** (P13 and P15, architect,``
  - docs/architect.md:624: `laptop: an alias by 'ssh -G <alias>' showing a ProxyCommand and one real 'ssh <alias>' login.``
  - README.md:754: `The certificate names 'agent' and the key ID 'operator-<project>.windows-amd64'. It lasts an``

## Step 314: over the lease's alias, in Git Bash

- Login shell: `BASH_VERSION` is set (Git Bash). **agent-windows' tool list (D97 and lease-api.md): Git for Windows (Git Bash included), PowerShell 7 (beside Windows PowerShell 5.1), Node (newest LTS), `uv`, Claude Code, `gh`;** Codex is not promised. Each of `git`, `pwsh`, `node`, `uv`, `claude`, `gh` is found by `command -v` and prints a version; `localswim` is found and `localswim --help` exits 0.
  - docs/decisions.md:5022: `| Git | Git for Windows, Git Bash included, which Claude Code needs on Windows |``
  - docs/decisions.md:5023: `| PowerShell | PowerShell 7, beside Windows PowerShell 5.1 |``
  - docs/decisions.md:5024: `| Node | The newest LTS, updated at each start, best effort, as 'agent-linux''s |``
  - docs/decisions.md:5025: `| 'uv' | The latest release, updated at each start, best effort |``
  - docs/decisions.md:5026: `| Claude Code | The native install, which updates itself |``
  - docs/decisions.md:5027: `| 'gh' | The GitHub CLI |``
  - docs/decisions.md:5029: `- **Codex is not promised**, as D4's Windows list did not; 'agent-linux' keeps promising it.``
  - docs/lease-api.md:1840: `| Git | Git for Windows, Git Bash included, which Claude Code needs on Windows |``
  - docs/operator.md:6411: `- **Every tool MUST be found by a new ssh session's login shell, Git Bash, with no setup of its``
- sshd's `DefaultShell` (registry value, `HKLM:\SOFTWARE\OpenSSH`) equals `C:\Program Files\Git\bin\bash.exe`, and that file exists.
  - docs/operator.md:5874: `- **'C:\Program Files\Git\bin\bash.exe' when that file exists**, Git for Windows' own launcher,``
- `first-boot.log` holds `first boot begun`, `step 1 done:` to `step 8 done:` (step 3 may also print `step 3: OpenSSH Server added`) and no `failed` line; its times print local with the UTC beside them.
  - infra/agentspace.yaml:156: `Write-AgentspaceFirstBootLine 'first boot begun'``
  - infra/agentspace.yaml:313: `Write-AgentspaceFirstBootLine 'step 8 done: sshd started; first boot done'``
  - docs/operator.md:5931: `step's line, with the time in UTC, to 'C:\ProgramData\agentspace\first-boot.log', so a person can``
- The horizon file exists and is the digits then one LF, or the digits alone, with no 0x0D; the step prints its byte count (11 for ten digits and an LF), `holds 0x0D: False` and its form. It is written by the guard at its next run, at most 5 minutes after the host runs, so a missing file is a FINDING that asks for a later rerun.
  - docs/operator.md:6374: `one LF, or nothing after the digits, and never a CR**, to a temporary file in that folder,``
  - docs/operator.md:6349: `'1764000000' alone are each a horizon. **A line ending in a CR is no horizon, ignored without a``
- **`Installed:` lines from `provision.log`: the count is 0 (a NOTE, not a finding), because a detached run's output is nobody's** (see 311). Optional step 316 runs `sync` by hand and checks the install's own lines.
  - docs/operator.md:3465: `person's shell sends them. **A run the driver starts detached has none of the CLI's standard``

## Step 315: the measures

### 315a: the patch run's time
- From patch.log's `Patch run started` and `Patch run finished` lines of this boot's run: finished, at most 900 s (`up` waits 15 minutes), and the task's limit is 1200 s. Over 900 s is a FINDING that goes back to the architect with the measurement. One `Tools:` version-summary line per run (counted with every `Tools:` line). `patched` written this boot, no `patch-failed`, the patch task's last result 0.
  - docs/operator.md:6203: `| 'agentspace-patch' | None of its own: the boot task and the install start it | 'agentspace-patch.ps1' | 20 minutes, the patch unit's limit on Linux, which [Starting the box](#starting-the-...``
  - docs/operator.md:6309: `a run longer than the 20-minute limit goes back to the architect with that measurement.``
  - docs/operator.md:6688: `- **How long the patch run takes** on the image the host was made from, and after a stop of some``
  - box/windows/agentspace-patch.ps1:147: `Write-AgentspaceLog $logPath 'Patch run started'``
  - box/windows/agentspace-tools.ps1:277: `Write-AgentspaceLog ('Tools: ' + ($versions -join ', '))``

### 315b: does a restart of sshd end the sessions open at that moment
- Either answer passes: a `RESULT (b)` line says whether session 1 (`sleep 120`, opened first) was ended by the restart (exit 255, short) or survived (exit 0 at about 120 s). The FINDINGs are a failure to measure: session 1 gone before the restart, sshd's process ID unchanged, or sshd not `Running` again. It STOPs when `signin-asked.flag` exists, so it runs before Terry's sign-in.
  - docs/operator.md:6690: `- **Whether a restart of the 'sshd' service ends the sessions open at that moment**.``
  - docs/operator.md:6569: `restart survives it is unmeasured**, the install's own among them; one that does not ends row 4's``

### 315c: Windows PowerShell 5.1 as the login shell and the push's stdin
- From provision.log (`sync done` before `bootstrap done`) and the host: bot.env holds two lines (`LOCALSWIM_URL=`, `LOCALSWIM_TOKEN=`, each with a value, never printed); a `RESULT (c)` line says yes (bot.env created before Git Bash appeared, so the first push ran under 5.1 and arrived whole) or inconclusive. No step changes DefaultShell.
  - docs/operator.md:6016: `Windows PowerShell 5.1 as the login shell, that it is passed on to a program the shell starts is``
  - docs/operator.md:1563: `**The first change writes that record with the change** (P9),``
  - docs/operator.md:6471: `- **The remote half is the operator's own text, in the encoded form, run under Windows PowerShell``

### 315d: winget, and which column of The tools each tool took
- A `RESULT (d)` line: whether `winget --version` answers in a key-based ssh session of `agent` (the bootstrap's own session type), winget's log files and which package IDs they name, each tool's file present with its creation time, and the inferred column per tool. A missing tool or no winget answer line is a FINDING.
  - docs/operator.md:6398: `N20; architect, 2026-11-26, for v0.12-T02, pin P16): whether winget works in a key-based SSH session on Windows Server``
  - docs/operator.md:6408: `| Claude Code | None | Anthropic's native installer for Windows, which installs into 'agent''s profile and updates itself |``
  - box/windows/bootstrap.ps1:148: `if ($script:wingetWorks) { Write-AgentspaceOut 'winget answers; tools with a winget package are installed by it' } else { Write-AgentspaceOut 'winget does not answer; every tool is installed...``

### 315e: the machine PATH and agent's tool folders reaching a new ssh session
- **Covered by step 314**: `command -v` of each tool and of `localswim` in a NEW ssh session's login shell finds them, which is the PATH reaching it (install step 6 adds the folders; step 315i shows the registry entry).
  - docs/operator.md:6695: `- **Whether the machine 'PATH' and 'agent''s tool folders reach a new ssh session**, by``

### 315f: the idle check's lines with a lease held and the host idle
- After a 720 s wait with no connection from the laptop: idle.log has lines since boot, every line one of the check's forms (`Active: SSH session open`, `Active: Session Manager session open`, `Active: Patching`, `Active: CPU work: <s> s since last check`, `Idle for <n> min of 60 (CPU <s> s since last check)`); at least two `Idle for` (idle-window) checks, each under 6 CPU-seconds, and no quiet check counted as CPU work. **Drafter's reading:** if the keeper's board holder keeps an SSH connection open, every check reads `Active: SSH session open` and the measure is not made; the counts say so, as a FINDING.
  - docs/operator.md:6697: `- **The idle check's lines with a lease held and the host idle**, against 6 CPU-seconds per check.``
  - docs/operator.md:6361: `- **Windows' own background work, at 6 CPU-seconds per 5-minute check, is unmeasured**: a host``
  - box/windows/agentspace-idle-check.ps1:375: `Write-AgentspaceDecision ('Idle for ' + $idleFor + ' min of ' + $idleLimit + ' (CPU ' + $cpuSeconds + ' s since last check)')``
  - box/windows/idle.conf:6: `BUSY_CPU_SECONDS=6``

### 315g: no restart that Windows Update schedules by itself
- In this boot's window of the System event log: event 6005 once, and events 1074, 6006, 6008 and 41 none; the WindowsUpdateClient source's counts and times are printed, not judged. Counts and times only.
  - docs/operator.md:6698: `- **No restart that Windows Update schedules by itself**, by the System event log over the``
  - docs/operator.md:6283: `a real restart, and the policy at work, are first seen live, in E0.12-T05-S02.``

### 315h: sync's patch wait answered within its wait
- From provision.log and the host's patch.log: the first `sync` line is a plain `sync done` (no `the patch run failed` text), the first patch run finished, `sync done` came no more than 5 s before its end (the wait was answered) and less than 1770 s after its start (the limit is 1800 s).
  - docs/operator.md:6700: `- **'sync''s patch wait on a new host's first boot answers within its wait, not by its 30-minute``
  - docs/operator.md:2089: `| A run that never finishes: 'pending' answered until the wait's 30-minute deadline, then its exact message, with the host's alias for a host that is not the box, exit 1, and no row 5; 'prov...``
  - docs/operator.md:2086: `| A 'failed' answer (P21): after 'pending', and as the first answer, the wait ending at that answer with no further 'ssm' call, the warning 'agentspace: The patch run on <id> failed; sync go...``

### 315i: the card's additions
- (1) winget as SYSTEM: one `winget --version` through a transient SYSTEM task that is removed (`system_winget_task_removed True`), its exit printed, the way agentspace-tools.ps1 calls it; the patch.log's `Tools: winget upgrade ... exited` lines are printed with 315d. (2) The idle task's trigger repeats: interval `PT5M`, no duration, the last run since this boot and at most 400 s old, the next run at most 400 s ahead, idle.log's lines since boot at least what the uptime asks for. **Drafter's reading:** the install registers the task after the boot with an `AtStartup` trigger, which Windows does not fire until the next boot; the step shows it if so (`The idle task has NOT run since this boot`). (3) One `Tools:` version-summary line per finished patch run.
  - box/windows/install.ps1:558: `$idleTrigger.Repetition = (New-ScheduledTaskTrigger -Once -At ([DateTime]::Now) -RepetitionInterval (New-TimeSpan -Minutes 5)).Repetition``
  - box/windows/agentspace-tools.ps1:161: `$wingetWorks = ((Invoke-AgentspaceProgram 'winget' @('--version') $null) -eq 0)``
  - docs/operator.md:6204: `| 'agentspace-idle' | At startup, 5 minutes after it, then every 5 minutes for as long as the host runs | 'agentspace-idle-check.ps1' | 4 minutes |``

- (4) Evidence on the host, one `evid` line each, `True` when shown. **One command shows each step**, from a file's state or a log line (the host script `s02-host-315i.ps1` runs them all in one session):

  | Step | The command that shows it |
  |---|---|
  | install.ps1 1: PowerShell 7 in place | `Test-Path 'C:\Program Files\PowerShell\7\pwsh.exe'` and the probe's own `$PSVersionTable.PSVersion` |
  | install.ps1 2: the CA file | `(Get-Item C:\ProgramData\ssh\agentspace_ca.pub).Length -gt 0` |
  | install.ps1 3: the folder's access rules | `(Get-Acl C:\ProgramData\agentspace).AreAccessRulesProtected` and its rules: SYSTEM and Administrators full control, Users read and execute, no other account |
  | install.ps1 4: the host agent's files | `Test-Path` of each of the eight files, and idle.conf's three settings |
  | install.ps1 5: Windows Update's policy | `Get-ItemProperty HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU` : `NoAutoUpdate` 1, `NoAutoRebootWithLoggedOnUsers` 1 |
  | install.ps1 6: the machine PATH | the registry's `Path` under `Session Manager\Environment` holds the three folders |
  | install.ps1 7: DefaultShell | `Get-ItemProperty HKLM:\SOFTWARE\OpenSSH` : `DefaultShell` is Git Bash's path |
  | install.ps1 8: the three tasks | `Get-ScheduledTask -TaskPath \agentspace\`: SYSTEM, Highest, action `pwsh.exe` by its full path |
  | install.ps1 10: this boot's patch run | `C:\ProgramData\agentspace\patched` written after the boot, and a `Patch run started` line in patch.log |
  | localswim.ps1 1: the CLI | `Test-Path C:\ProgramData\agentspace\localswim\localswim.exe` |
  | localswim.ps1 2: the credentials | `bot.env` exists, byte count above 0 (never read to the screen) |
  | localswim.ps1 3: read as data | the wrapper's text has `StartsWith('LOCALSWIM_URL=')` and no `Invoke-Expression`, `iex` or dot-source |
  | localswim.ps1 4: the board answers | a TCP connection from the host to the URL's host and port (the URL itself is `127.0.0.1:8811`) |
  | localswim.ps1 5: the CLI runs, the token never printed | `localswim.ps1 --help` exit 0, and the token (held in memory) is in neither stream |
  | bootstrap.ps1: each tool, the login shell, the update policy | each tool's file with its creation time, the DefaultShell value, the two policy values (the bootstrap's own output is not kept) |

  - docs/operator.md:6108: `It prints 'Installed: the machine PATH holds agentspace's and agent's tool folders (added)', or``
  - docs/operator.md:6094: `3. **Makes 'C:\ProgramData\agentspace\' with its own access rules**: inheritance off, full control``
  - docs/operator.md:6112: `8. **Registers the scheduled tasks** of [The host agent's tasks](#the-host-agents-tasks), replacing``
  - docs/operator.md:6119: `10. **Last, starts this boot's patch run when it has not finished** (#0163's twin), after every``
  - docs/operator.md:6433: `1. **Refuses when 'C:\ProgramData\agentspace\localswim\localswim.exe' is missing**, with``
  - docs/operator.md:6441: `4. **Refuses when the board does not answer**: one TCP connection to the URL's address and port,``

## Step 316 (OPTIONAL): sync by hand

- `agentspace-operator sync -Host agentspace -Platform windows/amd64` exits 0 and prints, in this order, `Installed: the machine PATH holds ... (already there)`, `Installed: the login shell is C:\Program Files\Git\bin\bash.exe`, `Installed: idle check (task <state>), patch at boot (task <state>)`, `Installed: sshd_config holds the agentspace lines (already there)`, the CA line (fingerprint masked), `Installed: login grace time 30 seconds; this sshd has no PerSourcePenalties`, the keepalive line, and `Patch run: finished this boot; not started again`; no `install.ps1:` line.
  - docs/operator.md:6108: `It prints 'Installed: the machine PATH holds agentspace's and agent's tool folders (added)', or``
  - box/windows/install.ps1:575: `Write-AgentspaceOut 'Patch run: finished this boot; not started again'``
  - docs/operator.md:6567: `**A restart of sshd comes only when the lines' bytes changed**, which the first boot's copy of the``

## Step 320: after Terry's sign-ins

- Over `agentspace-host.agentspace.windows-amd64` (the operator's certificate signed again first): `.claude\.credentials.json` exists (`True`), `%APPDATA%\GitHub CLI\hosts.yml` exists (`True`, gh fell back to its file), and `icacls` of each names no account but `<computer>\agent`, `BUILTIN\Administrators` and `NT AUTHORITY\SYSTEM`; the computer name prints as `<win-host>`. No content is read.
  - README.md:768: `host. **Then check who can read each file**, which MUST name no account but 'agent',``
  - README.md:787: `- **gh's own fallback to its file was not shown by the spike**, which had neither tool``
  - docs/decisions.md:5309: `| **Q23** | **Closed (Terry, 2026-11-26: "Take all five", the architect's recommendation on each).** ~~On a Windows host, are his sign-ins kept as files in 'agent''s profile, readable by 'ag...``

## Step 330: the comments read back

- As Bot, `localswim comment list --limit 200 <board> 267`: exit 0; the count of comments and whether one holds the fixed text (one expected), printed as numbers and a count alone; the token is never printed, never an argument and cleared after the read (`Clear-LocalswimEnv` in a `finally`). The fixed text is the one line of `target\t05\s02-p2-fixed-text.txt` (pin P2 pending); the step STOPs without it.
  - AGENTS.md:406: `- **localswim calls run as Bot.** Until this repo has its own wrapper, load the credential the way``
  - AGENTS.md:413: `- **A secret MUST NOT be passed as a command argument.** Pipe it on stdin or read it inside``
  - AGENTS.md:409: `- **On the laptop, call 'C:\Projects\localswim\localswim.exe' by its full path.** The``

## Step 340: Release W

- `agentspace lease release <id>` exits 0 and prints the lease `released` (members `created_at,expires_at,id,platform,profile,project,released_at,size,state,ttl_seconds`, no `ssh`); `lease list` counted, none ready or pending. It records guard.log's line count BEFORE the release (`guard-baseline.txt`) and the release's end time after it (`release-end.txt`).
  - docs/lease-api.md:2345: `| 'ready' | First ends the lease's connections on the caller's machine; then deletes the alias, ends any connection that came back through it meanwhile, and deletes the key and certificate; ...``
  - docs/lease-api.md:724: `file's pattern changes for it. A caller MUST read it from the lease and MUST NOT assume it,``
  - docs/architect.md:605: `- **A 'guard.log' baseline is a count taken at the turn's start**, never "absent": the laptop's log``

## Step 341: the guard's stop

- Within 35 minutes of the release's end: the observer's `edges.log` shows `agentspace.windows-amd64 running -> stopped`, at most 25 minutes after the release (D56: the grace, 15 minutes, plus 10); guard.log's new lines include `host=agentspace.windows-amd64 stopped since=...` and none `failed` or `sign-in has expired` (an `unexplained` line before the stop is expected); the read-only `describe-instances` answers one row, `stopped` and `Client.UserInitiatedShutdown` (an API stop, the guard's; `Client.InstanceInitiatedShutdown` would be the host's own power-off). The step exits 6 when windows/amd64's hours reach 4.5, 3 at the 35-minute bound.
  - docs/decisions.md:3320: `- **The stated time:** a host is 'stopped' within the grace plus 10 minutes of its last live``
  - docs/decisions.md:3300: `installed. **The grace is 15 minutes** (Terry, 2026-11-21, closing Q8), so a host is stopped``
  - docs/operator.md:4336: `<time> host=<project> stopped since=<since>``
  - docs/operator.md:4348: `'<time>' (P2). A line not about one host carries none (P2). **From v0.12 the field is``
  - docs/architect.md:582: `read-only call that starts nothing: 'Client.InstanceInitiatedShutdown' is the host's own``
  - scripts/live/observe-once.ps1:19: `#                 '<utc> <local> <host> <from> -> <to> type=<type>'. A host's first read is an edge``
  - scripts/live/observe-once.ps1:21: `#   hours.json    The instance-hours intervals: an interval opens at a host's first read whose state``

## Step 350: at the close

- `status -All` exits 0, every host `stopped` (agentspace.linux-amd64 and agentspace.windows-amd64 included), the windows host in the registry once; `lease list` counted, none ready or pending.
  - docs/decisions.md:5099: `guard stops the host within D56's bound.``
  - docs/architect.md:588: `- **The box ends stopped**, confirmed with 'status', and no lease the check made is left ready.``

## The loops' hours check

- Every loop (311, 315f's wait, 341) reads `C:\Projects\agentspace\target\t05-lead\observe\hours.json` at each pass, sums the `agentspace.windows-amd64` intervals (an open one up to now) and exits 6 with a FINDING when the sum reaches 4.5 (Q20 allows 5 hours at `m` or smaller).
  - docs/decisions.md:5306: `| **Q20** | **Closed (Terry, 2026-11-26: "Take all five", the architect's recommendation on each).** ~~Does he accept the new platforms' prices (D94's 'm8a' ladder: $0.12, $0.24, $0.49 and $...``
  - scripts/live/observe-once.ps1:25: `#                 running: a slight overcount, as v0.09-T05's record explains. An open interval is``

## The pins that are still open

- P2 (the desktop-app agent's one comment on #0267 with fixed text): step 330 needs `target\t05\s02-p2-fixed-text.txt`. P3 (the lease's TTL): `S02_TTL`, default `4h`.


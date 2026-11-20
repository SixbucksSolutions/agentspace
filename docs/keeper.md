# The agentspace keeper, reference v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for the keeper: the program on the laptop that holds the board's forwards on the
box for as long as a lease is ready (D30).** It is written so that the keeper can be rebuilt
without reading any of its code (D20, D34). It names every argument, decision, timing, lock, log
line and exit code, the handshake that starts a holder, and the logon task that runs the keeper
unattended. The reasoning behind each rule is in [decisions.md](decisions.md), cited as `D<n>`.

**What it does not own:**
- **Every file the keeper reads or writes is [docs/state.md](state.md#the-keepers-files)'s**: its
  path, its format (every field of each JSON file), its bytes (encoding and line ending), its share
  modes and access rules, how it is replaced atomically, who writes it, who reads it, and when it
  is created and removed. So are the path of the task's launcher and the folder of its error log.
  This page says when and why the keeper reads, writes or removes each file, the form of every log
  line, the rotation rule, what holding each lock means, and the launch handshake. The lease state
  file the keeper reads is [docs/state.md](state.md#the-lease-state-file)'s too.
- **What a lease is, its states and its fields** are [lease-api.md](lease-api.md#the-lease)'s.
- **The ProxyCommand, which every holder's connection runs**, and the token push it may make, are
  [operator.md](operator.md#the-proxycommand)'s.
- **Why only one connection on a host can hold the board's forwards** is D12. Why they need a
  holder at all is stated here, in [The board's forwards](#the-boards-forwards), and the decision
  to keep one is D30.

## Contents

- [Conventions](#conventions)
- [What the keeper does](#what-the-keeper-does)
- [Command line](#command-line)
- [A pass](#a-pass)
- [Starting a holder](#starting-a-holder)
- [Watching](#watching) (card #0068)
- [Locks](#locks)
- [Log lines](#log-lines)
- [The installed task](#the-installed-task)
- [Stopping and starting the keeper](#stopping-and-starting-the-keeper)
- [Checking the keeper](#checking-the-keeper) (card #0066)
- [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
- [Open questions](#open-questions)

## Conventions

### Requirements and observations

- **Behavior stated here in the present tense is required of a conforming keeper (MUST)**, except
  where it is marked as today's, as observed, or as a note.
- **A figure from a run on today's laptop or box is an observation, not a requirement.** Each one
  is marked "observed" or sits in a note, and names the run it came from. The observations are: the
  restart 37 seconds after a kill, the task results `0x800710E0` and `0xFFFFFFFF`, a trigger that
  keeps firing across reboot, the board outages measured with and without the release grace, the
  holder that survived only when WMI created it, the claim trials, the launcher's compile time, and
  the desktop app's ssh clearing every forward.
  None binds a rewrite; a check MUST NOT assert one.
- **Today's box's sshd bound (about 90 seconds) is a property of that box, not of the contract.**
  The requirement built on it is the default of `-ReleaseGraceSeconds`, 95, which a conforming
  keeper MUST use when the argument is not given ([Release grace](#release-grace)).
- **A note on where a requirement came from MAY name a story or a decision, and the reader needs
  none of them**: every rule is stated in full on this page.

### Where it runs

- **The keeper runs on the laptop, which is Windows today**, and this page specifies a Windows
  keeper: Windows processes, a Windows scheduled task, and Windows paths (`%USERPROFILE%`,
  `%SystemRoot%`). A keeper for another laptop OS is out of scope for v0
  ([Q1](#open-questions)).
- **The keeper never talks to the box or to AWS itself.** It reads the lease state file, and starts
  and stops one local ssh client, the holder. Everything the holder's connection does on the far
  side goes through the lease's alias and its ProxyCommand.
- **A host's platform is a parameter of the contract (D4).** Nothing here depends on the host's OS,
  with one exception, marked "today's box" where it appears: the default release grace is derived
  from the keepalive settings of today's box's sshd ([Release grace](#release-grace)).
- **The prototype is `scripts/board-holder.ps1`, run by PowerShell 7, with its installer
  `scripts/install-board-holder.ps1` and its task launcher `scripts/BoardHolderLauncher.cs`.**
  They are named here only where a note says where a requirement came from. The rewrite replaces
  all three in v0.06 (#0055, D28) with `agentspace-keeper.exe`, `agentspace-keeper-install.exe`
  and `agentspace-keeper-task.exe`, installed with agentspace's other programs as
  [operator.md, Programs and install layout](operator.md#programs-and-install-layout) says.

### Words used here

| Word | Means |
|---|---|
| Holder | The ssh client process the keeper starts to hold the board's forwards: `<ssh> -N -o ExitOnForwardFailure=yes <alias>` ([The holder command](#the-holder-command)). |
| Pass | One look at the leases and the holder, and at most one action to make them agree ([A pass](#a-pass)). |
| Keeper | The keeper's program. "A keeper", where this page says one does something, is a run of that program with `-Watch`: a pass every interval, for the life of the process ([Watching](#watching)). `-Watch` is off unless it is given, and the installed task gives it. |
| One-pass run | The keeper's program run without `-Watch`: one pass, then exit. |
| Record | The keeper's record of the holder it started, `board-holder.json` ([docs/state.md](state.md#the-keepers-files)). |
| Ready lease | A lease whose stored state is `ready` and whose `expires_at` is after the pass time ([Which leases are ready](#which-leases-are-ready)). |
| Trusted holder | A process that the record names and that passes every identity check ([Trusting the record](#trusting-the-record)). |
| Tracked holder | The holder a keeper watches between passes: one it started, or one it adopted when a pass logged `kept` for it. The keeper holds its PID, creation time, lease ID and alias in its own process. |
| Launcher | The process a start has created, outside the pass's job, to create the holder and answer the pass ([Starting a holder](#starting-a-holder)). |
| Proven | A tracked holder seen alive by a look at least `-FastFailureSeconds` (60) after the keeper began to track it, or one the keeper adopted, whose age is unknown. A keeper begins to track a holder it started when the pass that started it ends, not when the holder started. |
| Fast failure | A start that did not give a live holder, or a tracked holder that a look found gone, or that a pass logged `lost` for, before it was proven. |
| State folder | `AGENTSPACE_STATE_DIR` when it is set and not empty, else `%USERPROFILE%\.agentspace` (D21; [docs/state.md](state.md#the-keepers-files)). |

### What the keeper does with its files

**Every file below is [docs/state.md](state.md#the-keepers-files)'s**: its path, format, bytes,
share modes and access rules. This table says only what the keeper does with it.

| File | The keeper |
|---|---|
| `leases.json` | Reads it at every pass, without a lock, and never writes it ([docs/state.md](state.md#the-lease-state-file)). |
| `board-holder.json` | Writes it when it starts a holder, reads it at every pass, and removes it when the holder it names is stopped, gone or untrusted. |
| `board-holder.log` | Appends its log lines ([Log lines](#log-lines)). |
| `board-holder-ssh.log`, `board-holder-ssh.log.1` | Gives the holder the first as its stdout and stderr, and rotates it to the second ([The holder's log](#the-holders-log)). |
| `board-holder.lock` | Holds it for the length of a pass ([Locks](#locks)). |
| `board-holder-keeper.lock` | A keeper holds it for its whole life ([Locks](#locks)). |
| `board-holder-launch.<hex>.json`, `.claim`, `.result.json` | The launch request, its claim and its result. Every start writes them, and they exist only while a holder is being started ([Starting a holder](#starting-a-holder)). `<hex>` is the token [docs/state.md](state.md#a-holders-launch-files) defines, new for each start and shared by its three files. |
| `board-holder-launcher-errors.log` | Written by the task's launcher, never by the keeper ([The task's launcher](#the-tasks-launcher)). |

**Every JSON file the keeper writes is replaced atomically
([docs/state.md](state.md#the-keepers-files)).**

### Times

- **Every time in a line of `board-holder.log` is UTC, in the form `YYYY-MM-DDTHH:MM:SSZ`**,
  truncated to the whole second. These are machine-readable and keep that form. The task
  launcher's error log is not the keeper's, and has its own form
  ([The launcher's error log](#the-launchers-error-log)).
- **Durations on this page are in seconds, and MUST NOT be measured on local time**, so a change
  of the laptop's time zone moves nothing.
- **Which clock (D35; B-40; [Q17](#open-questions), closed).**
  - **The launcher's deadline is a UTC time**, written into the launch request and compared with
    the UTC clock by the launcher, since two processes read it
    ([The handshake](#the-handshake)).
  - **Every other duration is measured on one clock that local time and the time zone never
    move.** That covers the 60-second proof of a holder, the waits for a lock and for a launch
    result, the 10-second bound on a create call, the interval, the backoff and the grace. That
    clock MAY be the UTC clock or a monotonic clock. It SHOULD count the time the laptop spends
    asleep or hibernated, because the box's sshd drops a dead session on the box's own time, so the
    grace is about time that passed on the box.
  - **Across a laptop sleep, a wait MAY end at once on waking, or run on for its remaining
    seconds.** Neither breaks a rule here: no wait ends before its seconds have passed on the clock
    it uses, and the pass after it starts a holder if a lease is ready. A check MUST NOT depend on
    which.
  - **A step of the UTC clock, set by hand or by a time sync, MAY end a duration measured on it
    early or late, by the size of the step.** A build that measures on a monotonic clock is not
    moved by one, and a build MAY measure on either. No guarantee on this page rests on a clock:
    - **A start still gives at most one holder, recorded or reported**, because the claim file
      decides that, never a time ([The handshake](#the-handshake)).
    - **A launcher that reads its deadline as passed, rightly or not, starts nothing**, and the
      start fails and is tried again by a later pass.
    - **A grace or a backoff cut short costs at most one holder that fails on
      `ExitOnForwardFailure`**, which the backoff then spaces out. One that runs long delays the
      next start by the size of the step, and no longer.
    - **A check MUST NOT step the clock**, and MUST NOT depend on what a step does.
  - **Today:** the proof, the deadline and every bounded retry are on the UTC clock. The interval,
    the backoff and the grace are relative waits, which on Windows may not count sleep (not
    observed).

## What the keeper does

**A desktop-app session on a lease needs the board, and the app's own connection never carries the
board's forwards** (card #0042). The Claude desktop app was observed to start its ssh with
`-o ClearAllForwardings=yes` (D12, E3.03), so a lease alias's forwards never apply to its
connection, and an agent in a desktop-app session reaches the board only while some other
connection on the laptop holds them. The keeper supplies them: while a lease is ready, it keeps
one holder running through that lease's alias ([The board's forwards](#the-boards-forwards)).

- **It holds through the ready lease that expires last**, ties going to the lowest lease ID, so the
  holder lives as long as any ready lease does and is moved as rarely as possible
  ([Which lease](#which-lease)). A holder already running through a ready lease is kept, even when
  another ready lease expires later.
- **One holder at a time.** Only one connection on a host can bind the forwards (D12), and every
  lease is on one host until v0.07 (#0047), so the keeper runs one holder in all
  ([Q2](#open-questions)).
- **It stops a holder whose lease is no longer ready**, at the first pass at or after the lease's
  `expires_at` or its release, so a holder outlives its lease by at most about one interval and a
  pass (D30). The next pass, not the same one, starts a holder for another ready lease. A holder
  whose record was dropped is the exception: no pass stops it
  ([Trusting the record](#trusting-the-record)).
- **It MUST NOT stop a process it did not start.** A recorded PID is trusted only when the
  process's start time and command line match the record ([Trusting the record](#trusting-the-record)).
- **The holder MUST outlive whatever started it**: the pass, the keeper, an agent's tool call and
  the scheduled task ([Starting a holder](#starting-a-holder)). So a keeper that dies leaves the
  board held, and the next keeper adopts the holder by its record.
- **It retries with backoff, never in a tight loop (D14)**, and waits out the box's release of a
  dead holder's forwards before it starts the next one ([Watching](#watching)).
- **One keeper runs at a time per state folder** ([Locks](#locks)), started at logon and again
  within about a minute of a crash by a scheduled task ([The installed task](#the-installed-task)).

**Why a keeper, rather than the ProxyCommand or `lease create`, starts the holder** is D30: a
holder started by either would die with the process tree that started it, or outlive it with
nobody tracking it, and neither can stop it when a lease expires.

### The board's forwards

- **The lease's alias carries the board's forwards** ([lease-api.md](lease-api.md#the-credential)).
  The desktop app connects with all forwards cleared, so they never apply to its connection. The
  holder connects through the same alias without clearing them, so the holder's connection is the
  one that binds them on the host. The keeper's own process never connects anywhere.
- **Another connection through any alias on the host may hold the forwards instead**, such as an
  interactive `ssh <alias>`. The board then works through that connection, and every holder the
  keeper starts meanwhile exits at once under `ExitOnForwardFailure`. Each is a fast failure, so
  the keeper retries with backoff, every `-BackoffMaxSeconds` (300 by default) once the backoff is
  at its longest, for as long as the other connection holds them. When that connection closes, the
  board is down until the keeper's next start succeeds: up to one backoff, plus the host's release
  of that session if it did not close cleanly.

## Command line

```
<keeper> [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]
<keeper> -Watch [-SshExe <full path>] [-IntervalSeconds <n>] [-MaxPasses <n>]
         [-BackoffBaseSeconds <n>] [-BackoffMaxSeconds <n>] [-ReleaseGraceSeconds <n>]
```

**`<keeper>` stands for the keeper's program, `agentspace-keeper.exe`, a console program in the
install folder** ([operator.md, Programs and install layout](operator.md#the-programs)), which a
person runs as `agentspace-keeper`. Its usage lines and messages name it `agentspace-keeper`,
whatever path started it. It keeps the single-dash arguments below
([operator.md, Argument style](operator.md#argument-style)). It has no help: `-h`, `--help` and
`-?` are unknown arguments ([Usage errors](#usage-errors)). Note on where this came from: the
prototype's entry point is `pwsh -NoProfile -File <checkout>\scripts\board-holder.ps1`, and its
messages name it `board-holder.ps1`. A check MUST NOT depend on which of the two names a message
prints.

- **Argument names are matched exactly, without regard to case.** The prototype also ignores one
  `:` at the end of a name (`-Watch:`); the rewrite MAY, and a caller MUST NOT depend on it.
- **Every argument but `-Watch` takes one value, the next argument.** A next argument that is
  missing, empty, all whitespace, or starts with `-` is no value.
- **There are no positional arguments.**

| Argument | Value | Default | Applies to | Meaning |
|---|---|---|---|---|
| `-SshExe` | A full path | `%SystemRoot%\System32\OpenSSH\ssh.exe`, with the value of `SystemRoot` written out (D35, A-24; below) | Both | The ssh client the holder runs. It MAY be a `.cmd` or `.bat`, which a check uses as a stub ([Checking the keeper](#checking-the-keeper)). A pass MUST be run with the same `-SshExe` as the pass that started the holder, or the holder is untrusted ([Trusting the record](#trusting-the-record)). |
| `-Now` | `YYYY-MM-DDTHH:MM:SSZ` | The current second | One pass | The pass time: which leases are ready, and the time on the pass's line. For checks. |
| `-Watch` | None | Off | | Run as a keeper ([Watching](#watching)). |
| `-IntervalSeconds` | 1 to 86400 | `30` | `-Watch` | The wait after each pass, and the floor under every wait. |
| `-MaxPasses` | 1 to 86400 | No limit | `-Watch` | Exit 0 after this many passes. For checks. |
| `-BackoffBaseSeconds` | 1 to 86400 | `30` | `-Watch` | The backoff after the first fast failure in a row. |
| `-BackoffMaxSeconds` | 1 to 86400 | `300` | `-Watch` | The longest backoff. MUST NOT be less than `-BackoffBaseSeconds`. |
| `-ReleaseGraceSeconds` | 1 to 86400 | `95` | `-Watch` | The wait after a tracked holder ends ([Release grace](#release-grace)). |
| `-FastFailureSeconds` | 1 to 86400 | `60` | `-Watch` | Hidden, for checks only: how long a tracked holder must have lived to be proven. It is not in the usage text. |

**A number is a whole number from 1 to 86400**, written in decimal digits only: no sign, no
blanks, no separators. Leading zeros are allowed.

**`-SshExe`'s default is the value of the `SystemRoot` variable in the keeper's own environment,
then `\System32\OpenSSH\ssh.exe`** (D35). That one variable is its only source, the same one the
lease CLI reads for the system folder ([docs/state.md](state.md#the-alias-file)).
- **It is `C:\Windows\System32\OpenSSH\ssh.exe` on every machine seen.**
- **When `-SshExe` is not given, the default is checked like a value that was**
  ([Usage errors](#usage-errors), check 5). So with `SystemRoot` missing or empty, the default is
  not a full path, and the run ends with that usage error.
- **This is not the `<System32>` of [The holder command](#the-holder-command)**, which is the
  system folder as Windows reports it, and is not read from the environment. The two name the same
  folder unless the keeper's environment was changed.
- **Note on where this came from:** the prototype's default is the fixed text
  `C:\Windows\System32\OpenSSH\ssh.exe`, which differs only on a machine whose Windows folder is
  elsewhere
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

**The prototype also accepts a hidden `-Launch <request path>`**, which runs it as the launcher of
[Starting a holder](#starting-a-holder), and exits 0 when it has answered. It is internal: nothing
outside the keeper MUST call it, a conformance check included (D30). The rewrite MUST start every
holder through a launcher and the launch files ([Starting a holder](#starting-a-holder)), and MAY
make its launcher another program, or tell it the request's path another way.
- **`-Launch` is not part of the interface, in the prototype or in a rewrite.** A build whose
  keeper program is also its launcher MAY accept it, or any other hidden argument of its own, and
  what that argument does with a bad value is the build's.
- **A build that does not use it MUST treat it as any unknown argument**
  ([Usage errors](#usage-errors), check 1). A check MUST NOT pass `-Launch` at all, so it sees
  neither case.

### Usage errors

**A usage error writes three lines to stderr, logs nothing, and exits 2:** `agentspace-keeper:
<message>`, then the two usage lines:

```
Usage: agentspace-keeper [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]
       agentspace-keeper -Watch [-SshExe <full path>] [-IntervalSeconds <n>] [-MaxPasses <n>] [-BackoffBaseSeconds <n>] [-BackoffMaxSeconds <n>] [-ReleaseGraceSeconds <n>]
```

The prototype writes `board-holder.ps1` in place of `agentspace-keeper`, in all three lines.

**Checks run in this order, and the first that fails is the one reported.** `<Name>` is the
argument's name as this page spells it (`-SshExe`, not the caller's spelling); `<raw>`, `<value>`
and `<path>` are what the caller gave.

| # | When | Message |
|---|---|---|
| 1 | An argument that is not a known name, taken left to right | `Unknown argument '<raw>'.` |
| 1 | A name given twice, `-Watch` included | `-<Name> is given twice.` |
| 1 | A name with no value | `-<Name> needs a value.` |
| 2 | A `-Watch`-only number given without `-Watch` | `-<Name> is for -Watch.` |
| 2 | A number that is not a whole number from 1 to 86400 | `-<Name> '<value>' is not a whole number from 1 to 86400.` |
| 3 | `-Now` with `-Watch` | `-Now is not allowed with -Watch.` |
| 4 | `-BackoffMaxSeconds` less than `-BackoffBaseSeconds`, defaults included | `-BackoffMaxSeconds is less than -BackoffBaseSeconds.` |
| 5 | `-SshExe` not a fully qualified path | `SshExe '<path>' is not a full path.` |
| 5 | `-SshExe` holding a `"` | `SshExe '<path>' holds a quote.` |
| 5 | `-SshExe` a `.cmd` or `.bat` whose path holds a `%` | `SshExe '<path>' is a batch whose path holds a percent sign.` |
| 6 | `-Now` not exactly `YYYY-MM-DDTHH:MM:SSZ`, or not a real time | `-Now '<value>' is not a time in the form YYYY-MM-DDTHH:MM:SSZ.` |

**Check 1 is one scan of the arguments, left to right, which ends at the first failure.** At each
argument it asks, in this order: is it a known name, was that name given before, and, for every
name but `-Watch`, is the next argument a value. A value is then skipped, and is never read as a
name. So `-Watch -Watch -Bogus` gives `-Watch is given twice.`, and `-IntervalSeconds abc -Bogus`
gives `Unknown argument '-Bogus'.`, since check 1 ends before check 2 begins.

**Check 2 takes the numbers one at a time, in the argument table's order** (`-IntervalSeconds`,
`-MaxPasses`, `-BackoffBaseSeconds`, `-BackoffMaxSeconds`, `-ReleaseGraceSeconds`,
`-FastFailureSeconds`), skipping any not given (B-45). For each, without `-Watch` it reports `is
for -Watch`, and with `-Watch` it checks the value. The first failure is reported. So
`-IntervalSeconds abc` without `-Watch` gives `-IntervalSeconds is for -Watch.`, and
`-Watch -MaxPasses abc -IntervalSeconds 0` gives `-IntervalSeconds '0' is not a whole number from 1
to 86400.`

**Whether `-SshExe` exists is not a usage error**; it is the pass's concern, and fails a start
([Starting a holder](#starting-a-holder)). A `"` or a batch's `%` is refused rather than escaped,
because it would end a quoted path early, or be expanded by cmd.

### Exit codes

| Exit | One pass | `-Watch` |
|---|---|---|
| 0 | The pass ran, whatever its action, `failed` included; or it was skipped because another pass held `board-holder.lock` | `-MaxPasses` passes ran; or another keeper holds `board-holder-keeper.lock` |
| 1 | The lease state could not be read, the pass met an error it does not handle, or the pass's own line could not be logged | `board-holder-keeper.lock` could not be taken for a reason other than another keeper holding it; or an error before the first pass |
| 2 | A usage error | A usage error |

**The architect ruled these on 2026-11-17 (D30).** A `failed` start or stop exits 0, because the
pass ran and logged what happened; the next pass tries again.

- **A keeper never exits for an error once its loop has started** ([Watching](#watching)). Only
  `-MaxPasses`, a usage error, the keeper lock, or the process being killed ends it.
- **An error before the first pass, such as a state folder that cannot be created, ends any run
  with exit 1 and nothing logged**, since there may be nowhere to log. The prototype then prints its
  runtime's error record on stderr; the rewrite SHOULD print one line starting with its own name
  instead. For a state folder that cannot be created it is
  `agentspace-keeper: The state folder '<path>' could not be created: <error>` (architect,
  2026-11-19, for E0.06-T01-S06).
- **A record the pass cannot remove** (architect, 2026-11-19, for E0.06-T01-S06): one that failed
  to be trusted ends the pass with `failed lease=- alias=- pid=- reason=The pass stopped on an
  error: The record could not be removed: <error>` and exit 1; after a stop, the failed removal is
  not reported, the line stays `stopped`, and the next pass removes the record, which then names
  no running holder.

### Streams and environment

- **Every line the keeper logs is also written to stdout**, as it is logged
  ([Log lines](#log-lines)). Usage errors go to stderr only. Nothing else is written to either.
  Under the installed task, the keeper's stdout and stderr reach no one, so `board-holder.log` is
  the only record of what it did ([The task's launcher](#the-tasks-launcher)).
- **The keeper reads no stdin.**
- **`AGENTSPACE_STATE_DIR`**, when set and not empty, names the state folder; a check MUST set it
  to a throwaway folder ([Checking the keeper](#checking-the-keeper)). The keeper creates the state
  folder if it is missing.
- **`USERPROFILE`**, as the keeper's own environment has it, names the holder's working folder
  and, with `AGENTSPACE_STATE_DIR` unset, the state folder. **When it is unset or empty, the keeper
  MUST end before its first pass, whether or not `AGENTSPACE_STATE_DIR` is set**, since a holder
  needs its working folder: exit 1, nothing logged, and the one line
  `agentspace-keeper: USERPROFILE is not set, so there is no home folder.` on stderr, after the
  usage checks (architect, 2026-11-19, for v0.06, gaps FA-18 and FB-19). The pass writes that folder's path
  into the launch request, so the holder starts in the keeper's `%USERPROFILE%` even when the
  logon environment's differs, as under a check. The path is all that reaches the holder: the
  `USERPROFILE` variable the holder itself sees is the logon environment's, below.
- **The holder MUST run with the user's logon environment, never with the keeper's** (D35; B-41;
  [Q4](#open-questions), closed). So MUST the launcher that creates it
  ([Starting a holder](#starting-a-holder)).
  - **The logon environment is the one Windows builds for the user's own logon session**: the
    system's variables and the user's, as a new process the user starts from the desktop gets
    them. A process created by Windows Management Instrumentation's (WMI's) `Win32_Process.Create`
    gets it.
  - **A variable set only in the keeper's environment MUST NOT reach the holder or its
    ProxyCommand.** `AGENTSPACE_STATE_DIR` and a changed `USERPROFILE` are two such variables, and
    so is any test seam ([operator.md](operator.md#the-test-seam)). This rule is about variables:
    the holder's working folder and its log are paths the pass passes on, and are the keeper's.
  - **Why:** the holder outlives whatever started the keeper, an agent's tool call or a check
    included, and MUST behave as the user's own ssh would. A check's or an agent's variables
    MUST NOT reach the real ProxyCommand. The launch request carries full paths for the same reason
    ([docs/state.md](state.md#a-holders-launch-files)).
  - **A check can see this**: its stub holder can write its environment to
    `board-holder-ssh.log` under a keeper given a variable of the check's own.

## A pass

**A pass looks at the leases and at the holder once, then takes at most one action to make them
agree**, and logs one line for it. A one-pass run is one pass; a keeper runs one per iteration.

### The order of a pass

1. **Take `board-holder.lock`**, waiting up to 10 seconds ([Locks](#locks)). A pass that cannot
   take it logs `skipped` and ends, with nothing else looked at.
2. **Check the command matcher against its own cases** ([The holder command](#the-holder-command)).
   A matcher that fails ends the pass with a `failed` line and exit 1, trusting nothing.
3. **Rotate `board-holder-ssh.log`** if it is due ([The holder's log](#the-holders-log)).
4. **Read the lease state.** If it cannot be read, log `failed lease=- alias=- pid=- reason=Lease
   state could not be read: <error>`, leave the holder and the record as they are, and end with
   exit 1. A missing state file is no leases, not an error.
5. **Work out the ready leases** ([Which leases are ready](#which-leases-are-ready)).
6. **Read the record and decide whether it names a trusted holder**
   ([Trusting the record](#trusting-the-record)). A record that does not is removed, and its
   process, if any, is left alone.
7. **Act**, by the first row of this table that applies:

| The holder | The leases | Action | Line |
|---|---|---|---|
| Trusted, and its lease is ready under the recorded alias | Any | Keep it | `kept` |
| Trusted, and its lease is not ready, or its alias changed | Any | Stop it, and start nothing this pass | `stopped`, or `failed` |
| None trusted, in a keeper that tracks a holder (card #0068, [Release grace](#release-grace)) | Any | Nothing | `lost`, naming the tracked holder |
| None trusted | None ready | Nothing | `idle` |
| None trusted | One or more ready | Start one through the chosen lease | `started`, or `failed` |

8. **Release `board-holder.lock`.**

**The rows are taken in order, so a trusted holder in the record wins over the #0068 row** (A-09,
B-38; [Q6](#open-questions), closed). When a keeper tracks one holder and the record now names
another that is trusted (one a one-pass run started, say), the pass keeps it (`kept`) when its
lease is ready under its alias, and the keeper adopts it with no grace, since the pass starts
nothing; or it stops it (`stopped` or `failed`) when its lease is not, and the grace follows that
stop as after any other ([Release grace](#release-grace)).

**The pass time is fixed when the pass begins**, before the lock is taken: `-Now` when given, else
the current UTC second. Every line of the pass carries it, so a `started` line can carry a time up
to about 95 seconds before it is written, plus the pass's wait for `board-holder.lock` (up to 10
seconds) ([Timings](#timings-of-a-start)). A `skipped` line likewise carries the pass time, and
is written about 10 seconds later, when the lock wait ends.

### Which leases are ready

- **A lease is ready when its stored state is `ready` and its `expires_at` is strictly after the
  pass time.** A lease whose `expires_at` equals the pass time is expired, the same rule the lease
  CLI uses ([lease-api.md](lease-api.md#states)).
- **The lease state is read without a lock**, so a pass never blocks a lease command, and sees the
  file as it was before or after any one write, never part of one
  ([docs/state.md](state.md#concurrency)).
- **A state file that does not pass its schema cannot be read**, and fails the pass as in step 4.
- **Nor can one that passes its schema but holds two records with one `id`**: the keeper reads it
  as the lease commands do, as a file that is not valid ([docs/state.md](state.md#the-lease-state-file)),
  and fails the pass as in step 4, with `<error>` `the state file is not valid` (architect,
  2026-11-19, for v0.06, gaps FA-19 and FB-18). It never picks a lease from such a file.

### Which lease

**When it starts a holder, the pass chooses the ready lease with the latest `expires_at`; among
those that share it, the one whose lease ID is lowest in ordinal order.** The holder goes through
that lease's `ssh.host_alias`. The choice is stable: the same leases always give the same choice.

### Trusting the record

**The pass trusts the recorded holder only when all of these hold:**
1. **The record can be read and is valid**: every field present, of its type and form
   ([docs/state.md](state.md#the-keepers-files)). A record that is not is no record.
2. **A process with the recorded PID is running, and can be opened to query it**: opened with
   only the rights to read its limited information and to wait on it. The right to terminate it is
   asked for only by a stop ([Stopping a holder](#stopping-a-holder)).
3. **Its creation time, in UTC, truncated to the millisecond and written
   `YYYY-MM-DDTHH:MM:SS.fffZ`, equals the record's `process_started_at` exactly.** The PID with the
   start time is the holder's identity, and defeats PID reuse.
4. **Its command line is the holder command for the pass's `-SshExe` and the record's alias**, by
   the matcher in [The holder command](#the-holder-command).

**A record that fails any of these is removed, and the process it names, if any, MUST be left
running and untouched.** The removal has no log line of its own. In a keeper that tracks a holder,
the pass logs `lost` for the tracked holder and starts nothing (card #0068,
[Release grace](#release-grace)). Otherwise, in a one-pass run or in a keeper that tracks none,
the pass goes on as with no holder, logs `idle` or starts one if a lease is ready, and that line
is the only sign that a holder was lost. So a holder started with another `-SshExe` is orphaned,
and a holder started for it then most likely fails on `ExitOnForwardFailure` while the orphan
holds the ports.

- **A process that runs but cannot be opened to query it fails item 2 the same way**: its record is
  removed and it is left running. Whether such a record SHOULD be kept instead, as a stop keeps
  one, is [Q16](#open-questions).
- **A holder whose record was dropped is never stopped by any pass.** It keeps its lease's host up
  past the lease. While it holds the forwards, every start fails on `ExitOnForwardFailure`, and a
  keeper's backoff reaches `-BackoffMaxSeconds`. Nothing on this page finds it again
  ([Q13](#open-questions)).

### The holder command

**The holder is `<SshExe> -N -o ExitOnForwardFailure=yes <alias>`.**
- **For an exe, the command line the keeper gives Windows is**
  `"<SshExe>" -N -o ExitOnForwardFailure=yes <alias>`, and the program run is `<SshExe>`.
- **For a `.cmd` or `.bat`**, which Windows runs only through cmd, the program run is
  `<System32>\cmd.exe` and the command line is
  `"<System32>\cmd.exe" /d /c ""<SshExe>" -N -o ExitOnForwardFailure=yes <alias>"`. `/d` skips
  cmd's AutoRun. `<System32>` is the Windows system folder, as Windows reports it.
- **The record names the holder process itself**: `ssh.exe` for an exe, or the `cmd.exe` that runs
  the batch. It never names a launcher or a wrapper (D30).
- **`-N`** opens no shell, and **`ExitOnForwardFailure=yes`** makes the holder exit rather than run
  without the forwards, when another connection already holds them (D12).

**A command line is recognized as the holder command structurally, never byte for byte (D30).**
The trust check and the stop's re-check MUST use this one rule:
1. **Split the line into tokens as Windows' `CommandLineToArgvW` does.** The first token, the
   program, runs to the next `"` when the line starts with one (or to the end if there is none),
   and otherwise to the next space or tab, with no escapes. Every later token follows the argument
   rules: a space or tab outside quotes ends it; 2n backslashes before a `"` give n backslashes and
   the `"` opens or closes quoting; 2n+1 give n backslashes and a literal `"`; and backslashes
   before anything else are literal.
   - **Quotes that follow one another directly are read as one run, by a count.** The count starts
     at 1 when the run begins inside quoting, and at 0 outside it. Each quote of the run adds one.
     A quote that brings the count to 3 is literal, and sets the count back to 0. When the run
     ends, quoting is on if the count is 1, and off otherwise.
   - **So inside quoting**, `""` gives one literal `"` and ends the quoting, and `"""` gives one
     literal `"` and keeps it.
   - **And outside quoting**, `""` is an empty quoted text and leaves quoting off, and `"""` gives
     one literal `"` and leaves quoting off.
2. **The program matches `-SshExe` when both are fully qualified paths that name the same file as
   Windows compares them**: each normalized as a full path (separators, `.` and `..` resolved),
   then compared without regard to case. A path that is not fully qualified matches nothing.
3. **The tokens after the program MUST be exactly `-N`, `-o`, `ExitOnForwardFailure=yes` and the
   alias**, in that order and no more, each compared exactly, case included.
4. **For a `.cmd` or `.bat` `-SshExe` only**, the line MAY instead have `<System32>\cmd.exe` as its
   program (by rule 2), then optionally `/d`, then `/c`, each switch compared without regard to
   case on its raw text. The text after `/c`, less its leading blanks, is then read as cmd reads
   it: when it starts with a `"`, that quote and the last `"` in the text are removed, except when
   the text holds exactly two quotes with at least one blank and none of `&<>()@^|` between them,
   when both are kept. The result MUST then meet rules 1 to 3.
5. **A `.cmd` or `.bat` run directly, without cmd, also matches** by rules 1 to 3.

**The matcher MUST check itself at the start of every pass**, against cases that include at least
the following, and a pass whose matcher gives any wrong answer MUST trust nothing and fail
(step 2). `<ssh>` is an exe path, `<stub>` a `.cmd` path, `<cmd>` is `<System32>\cmd.exe`, and
`<args>` is `-N -o ExitOnForwardFailure=yes`.

| Case | Matches |
|---|---|
| The keeper's own form, for an exe, a `.cmd`, and a `.cmd` whose path has spaces | Yes |
| `"<ssh>" <args> <alias>`, an exe, quoted | Yes |
| `<ssh>` unquoted, in another case, with extra blanks and a tab between tokens and a blank at the end | Yes |
| `<ssh>` with forward slashes | Yes |
| `<cmd> /c ""<stub>" <args> <alias>"`, a `.cmd` under cmd with `/c` and no `/d` | Yes |
| `<CMD> /D /C ""<stub>" <args> <alias>"`, in capitals | Yes |
| `<cmd> /c "<stub with spaces>" <args> <alias>`, two quotes kept | Yes |
| `"<ssh with spaces>" <args> <alias>` | Yes |
| `<ssh with spaces>` unquoted | No |
| Another alias, a longer alias, or the alias in another case | No |
| The alias quoted with more text inside the quotes | No |
| An extra argument after the alias, or before `-N` | No |
| `-n` for `-N`, or `-o` with its value missing | No |
| Another program with the same arguments | No |
| An exe `-SshExe` under `<cmd> /c` | No |
| A `.cmd` under a `cmd.exe` that is not `<System32>\cmd.exe` | No |
| A `.cmd` under `<cmd> /k`, or `<cmd> /s /c` | No |
| A `.cmd` under cmd with another alias, an extra argument inside the quotes, or `& calc` after them | No |
| `"<stub>" <args> <alias>`, a batch run directly | Yes |
| An empty line | No |
| A relative `-SshExe`, such as `ssh.exe` | No |

**The splitter's own cases:** `p "a b" c\\d e\"f g\\"h i"` gives `p`, `a b`, `c\\d`, `e"f`,
`g\h i`; `p """a"""` gives `p`, `"a"`; and `"C:\x\"y  z ` gives `C:\x\`, `y`, `z`.

### Keeping a holder

**A trusted holder whose record's lease is ready, by ID compared without regard to ASCII case,
and whose ready lease's `ssh.host_alias` equals the recorded alias exactly, is kept.** (The ID
was "compared exactly" until 2026-11-19, when the architect ruled for E0.06-T01-S07 that a reader
keeps a record's `lease_id` as stored, which state.md's MAY lets be in any case.) The pass logs `kept lease=<id>
alias=<alias> pid=<pid>` and changes nothing. A keeper that did not start that holder adopts it
([Watching](#watching)).

### Stopping a holder

**A trusted holder whose lease is not ready (released, expired, unsatisfiable, pending, or gone
from the state file), or whose lease's alias is no longer the recorded one, is stopped.** A lease
that expires is seen at the first pass whose pass time is at or after its `expires_at`, so the stop
comes up to about one interval after expiry.
- **The stop opens the holder by PID, and checks its creation time and command line again on that
  same handle, by the same rule.** While the handle is open Windows cannot reuse the PID, so the
  process checked is the process stopped.
- **It stops the holder's tree, and nothing else (D30).** The descendants are found by parent PID,
  and each is opened and checked on its own handle: its parent PID MUST be its parent's, and it
  MUST have been created no earlier than its parent. A process whose parent died before the walk
  cannot be traced, and is left alone. A descendant that cannot be opened, or fails its checks, is
  left alone and not reported. Nothing is ever matched by command line alone, and nothing is
  stopped by a sweep of the process list or by `taskkill`.
- **The holder's tree includes its ProxyCommand** and what that runs
  ([operator.md](operator.md#the-proxycommand)), since ssh starts it as a child, so a stop ends
  them with the holder, as far as it can open and check them. When a holder
  ends by itself, whether its ProxyCommand ends too is ssh's and the ProxyCommand's, and the keeper
  never looks for it ([Q20](#open-questions)).
- **Every handle is held until the stop ends**, so no PID in the tree is reused while it is being
  stopped.
- **The tree is terminated leaves first, the holder last.** The stop then waits up to 2 seconds for
  each process to exit, and walks the tree again for any child made meanwhile, for up to 5 rounds.
- **A terminated holder does not close its session on the box.** That session keeps the forwards
  until the box's sshd drops it, which is why a keeper waits the release grace before the next
  start ([Release grace](#release-grace)).

**What the pass logs, and what becomes of the record:**

| What the stop found | Record | Line |
|---|---|---|
| The holder, and every descendant the stop could open and check, stopped | Removed | `stopped` |
| Nothing stopped, and a new look finds the holder gone or no longer the recorded one, or cannot open it | Removed | `stopped` |
| Nothing stopped, because the holder could not be opened with the right to terminate it (one started elevated, met by a pass that is not, say), and it still runs | Kept | `failed`, reason `The holder could not be opened to stop it: Windows error <n>.` |
| Nothing stopped for another reason, and the holder still runs | Kept | `failed`, reason `The holder could not be stopped: <why>, but the holder is still running.` |
| The stop met an error | Kept | `failed`, reason `The holder could not be stopped: <error>` |
| The holder still runs after it was terminated | Kept | `failed`, reason `The holder is still running after it was stopped.` |
| The holder stopped, but some of its tree did not | Removed | `failed`, reason `The holder stopped, but these processes of its tree did not: <pid>,<pid>.` |

**`stopped` MUST be logged only when the pass no longer finds the recorded holder**: the stop ended
it, or a new look finds no trusted process under the record. The record MUST be kept while a look
still finds the recorded holder running, so that the next pass tries again (D30). A pass that
stops a holder, or tries to, MUST NOT start one; the next pass does.

**The new look is the trust check of [Trusting the record](#trusting-the-record), items 2 to
4.** It does not find the recorded holder when the process is gone, is no longer the recorded
one, or cannot be opened to query it.
- **The last of those is the one case where `stopped` is logged for a process that may still
  run.** When the stop could not open the holder to terminate it and the new look cannot open it
  even to query it, the pass logs `stopped` and removes the record. The process is then an orphan,
  as when a pass drops such a record ([Trusting the record](#trusting-the-record);
  [Q16](#open-questions)).
- **So `stopped` says the keeper has let go of the holder**, which in every other case means the
  holder ended. That case needs a holder the pass's user cannot open at all, such as one started
  with other rights.

> **Note.** The prototype's "for another reason" text names an internal outcome of its stop routine
> in place of `<why>` (`StopTree found NotRunning`, `IdentityDiffers` or `Matched`). The rewrite
> SHOULD say in words why nothing was stopped.

### Idle

**With no trusted holder and no ready lease, the pass logs `idle lease=- alias=- pid=-`** and
changes nothing. An idle line never names a holder. The one exception is a keeper's pass while the
keeper tracks a holder, which logs `lost` for that holder instead
([The order of a pass](#the-order-of-a-pass)).

## Starting a holder

**What a start MUST achieve, whatever the mechanism:**
1. **The holder MUST outlive the pass and every process above it.** On Windows that means it MUST
   NOT be in the Job Object of the process that asked for it, or of any process above that: a job
   is closed when an agent's tool call or a scheduled task ends, and kills what is in it.
2. **Its stdin is `NUL`, and its stdout and stderr are one handle that appends to
   `board-holder-ssh.log`**, opened with the sharing that
   [docs/state.md](state.md#the-keepers-files) gives the holder's log handle, so that a later
   holder, a rotation, or a reader is never refused while an earlier holder still has it open. A
   shell's own `>>` redirection does not share writing; it MUST NOT be used.
3. **It inherits no handle but those two.** In particular it MUST NOT inherit
   `board-holder-keeper.lock` or `board-holder.lock` ([Locks](#locks)).
4. **It has no window, starts in a new process group, and its working folder is `%USERPROFILE%`**
   as the pass's own environment has it, so that it never holds a worktree or a checkout open.
   Its environment is the user's logon environment, never the keeper's, so when the two
   `USERPROFILE` values differ the holder starts in the keeper's folder and sees the logon
   environment's variable ([Streams and environment](#streams-and-environment)).
5. **Beyond item 1, it SHOULD be outside any job.** The process that creates it asks to leave
   that process's own job, if it is in one. Where Windows refuses, the holder MAY be started in
   that job, but only when item 1 still holds: the job MUST NOT be one that the asking pass, the
   keeper, the task or an agent's tool call is in. Today the creating process is the launcher,
   which WMI created outside all of those, so a refusal leaves the holder in a job item 1 allows.
   Whether the launcher is in any job at all is unobserved.
6. **At most one holder is started per start**, even when the mechanism retries.
7. **A holder that starts is recorded by its pass, or the pass logs `failed` with a reason that
   says a holder may be running, or runs, unrecorded** (D30). A pass that is killed mid-start
   cannot meet this ([A pass that is killed](#a-pass-that-is-killed)).

> **Note.** Where item 1 came from: the 0.02.T1.2 spike observed that a holder started by the
> pass's own runtime (PowerShell's `Start-Process`, or .NET's process start) died when the job of
> the tool call that ran the pass was closed, and only one created through WMI's
> `Win32_Process.Create` survived (D30). The prototype's `>>` failure in item 2 was observed the
> same way: a second holder failed at once while any earlier process held the log.

**Every start MUST meet these through the launch handshake below, with its three files in the
state folder** (D35; A-07, B-37; [Q5](#open-questions), closed). The files are part of contract
v0: a check sees them appear, reads them, and sees them removed. Their names, fields, bytes and
access rules are [docs/state.md](state.md#a-holders-launch-files)'s.
- **What a build MUST do:** the pass writes the request; a launcher, a process other than the pass,
  claims it and answers with the result; the pass withdraws an unanswered request by claiming it;
  and the pass removes all three, in the steps and bounds below.
- **What the build chooses:** which program the launcher is, and how it is told the request's
  path. The prototype's is its own script with the hidden `-Launch` ([Command line](#command-line)).
- **Why it is required:**
  - The prototype writes the files.
  - The claim file is what makes a start yield at most one holder (item 6), and never leave a
    holder unrecorded without a `failed` line (item 7), as [The handshake](#the-handshake) shows
    step by step.
  - The only start seen to outlive the asking process's job is a second process created through
    WMI. Whether a compiled keeper can break away from the task's job directly has never been run.
- **A later contract MAY drop the files** for a mechanism a run on the laptop has shown to meet
  items 1 to 7.

### Before the start

**The pass refuses to start, and logs `failed lease=<id> alias=<alias> pid=- reason=The holder
could not be started: <why>`, when:**

| Why | Reason text |
|---|---|
| `-SshExe` is not a file | `SshExe '<path>' does not exist.` |
| The state folder's path holds a `"` | `The state folder '<path>' holds a quote.` |
| The keeper's own program path holds a `"` | `This program's path '<path>' holds a quote.` The prototype says `This script's path`. |
| The alias does not match `\A[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z`, case-sensitively | `Alias '<alias>' is not a host alias.` |

The form checks of [Usage errors](#usage-errors) row 5 come first, then this table's rows in
order; the first that fails is reported.

### The handshake

1. **The pass writes a launch request**, `board-holder-launch.<hex>.json`, naming the program, the
   command line, the holder's log, its working folder, the result file, and a deadline 30 seconds
   from now ([docs/state.md](state.md#a-holders-launch-files) has its fields). The deadline is
   taken just before the request is written, and every later bound of the start counts from it.
2. **The pass has a launcher process created**, told the request's path, with no window and the
   pass's `%USERPROFILE%` as its working folder. **It MUST be created outside the pass's job and
   every job above it, and with the user's logon environment**
   ([Streams and environment](#streams-and-environment)), which is what lets the holder outlive the
   pass. The prototype has it created by WMI's `Win32_Process.Create`, which gives both, and is
   the only means a run has shown to give them. A build MAY use any means that gives both.
   - **Each create call MUST be bounded at 10 seconds** (D35; A-25, B-39; [Q15](#open-questions),
     closed). A call that has not returned by then is abandoned and counts as a call that threw,
     with the error `The call did not return within 10 seconds.` The pass does not wait for it
     again, and uses nothing it returns later.
   - **A create call that throws, or reaches its bound, is tried up to 3 times, 2 seconds apart,
     and never more (D14).** WMI can be slow just after logon. A create that returns a non-zero
     code is not retried. Three bounded tries and two gaps take at most 34 seconds, so the create
     has succeeded or failed before the 35 seconds of step 5 are up.
   - **A try that returns late leaves its launcher little or no time.** The deadline is 30 seconds
     from the request, so a launcher made by a third try that took most of its 10 seconds claims
     the request after the deadline and starts nothing (step 4), or finds it withdrawn (step 5).
     The start then fails, and a later pass tries again.
   - **A create call that threw, or was abandoned, may still have made a launcher, or still make
     one later.** So a failed create is not the end: the pass withdraws the request (step 5), and
     waits for a launcher that claimed it. A launcher made after the pass withdrew the request, or
     removed it, starts nothing (step 3).
3. **The launcher claims the request before it does anything else**, by creating its claim file,
   `board-holder-launch.<hex>.claim`, in create-new mode, which exactly one process can do. Its
   content, `launcher <pid>` with the launcher's own PID, is
   [docs/state.md](state.md#a-holders-launch-files)'s.
   - **A launcher that cannot claim it starts nothing, writes nothing, and exits.** So a second
     launcher, from a create retry after a call that threw but had made one, starts nothing.
   - **A rename MUST NOT be used to claim**, because the loser's rename of a file already given
     that name can succeed too. Observed in a trial of four launchers racing for one request: a
     rename started two holders in 2 rounds of 9, and the claim file exactly one in 12 rounds of 12
     (D30).
   - **A launcher that claims the request and then cannot read it** (the pass has removed it)
     removes its claim and exits, starting nothing.
4. **The launcher starts the holder and answers**, by writing the result file,
   `board-holder-launch.<hex>.result.json`: the holder's PID, its creation time to the
   millisecond and its command line as read back from the new process, or an error.
   - **A launcher that claims the request after its deadline starts nothing**, and answers with
     the error `The launcher reached its deadline before it started the holder, so it started
     nothing.`
   - **A launcher that fails to start the holder answers with the error**: `NUL could not be
     opened.`, `The holder's log could not be opened.`, `The holder's start time could not be
     read.`, or Windows' own text for the failure.
   - **The launcher then exits 0**, whatever its answer said. If it cannot write the result, it
     exits non-zero, and its pass sees no answer (step 5). It logs nothing. The holder runs on with
     no parent.
5. **The pass waits for the result**, looking every 100 ms, until 5 seconds past the deadline (35
   seconds from the request). **When the create failed, it does not wait**: it looks once, at
   once, which is at most about 34 seconds from the request after three tries, and sooner after a
   non-zero code. **With no result by then, the pass withdraws the request by claiming it
   itself**, in the same create-new mode, with the content `abandoned`:
   - **If the pass's claim wins**, no launcher can claim the request, none starts a holder, and the
     start fails with `<why> Its request was withdrawn, so no holder was started.`
   - **If a launcher claimed it first**, that launcher may still start a holder. The pass waits on
     for its answer, up to 60 more seconds, and records the holder if one comes. If none comes, the
     start fails with `<why> A launcher claimed the request, or it could not be withdrawn, and no
     answer came in 60 more seconds, so a holder may be running unrecorded.`
   - **`<why>`** is `The launcher did not answer within 35 seconds.`, or the create's failure:
     `Win32_Process.Create could not be called: <error>` after three tries that threw or reached
     the bound, with the last try's error, or `Win32_Process.Create returned <code>.` A build that
     creates its launcher by another means names that means in place of `Win32_Process.Create`.
6. **The pass reads the result.** An error in it fails the start with that error as the reason.
7. **The pass checks, once and with no wait, that the holder is still running** and is the one the
   result names, by PID, creation time and command line
   ([Trusting the record](#trusting-the-record)). A holder that is not fails the start with `The
   holder exited at once; see board-holder-ssh.log.` A holder that exits a moment after this check
   is recorded, and found gone by a later look or pass.
8. **The pass removes the request, then the claim, then the result**, whatever happened. The
   request goes before its claim, so a launcher that claims the request after the claim is gone
   finds no request, and starts nothing. A pass killed during a start leaves its files behind, and
   nothing removes them later ([Q13](#open-questions)).
   - **A pass that is killed after a launcher claimed its request breaks item 7.** The launcher
     still starts the holder and writes its result, and nothing records or reports that holder: no
     pass trusts it, none stops it, and while it holds the forwards every later start fails on
     `ExitOnForwardFailure`. The same holds for a pass killed after the result and before the
     record ([A pass that is killed](#a-pass-that-is-killed)).
9. **The pass writes the record** for the holder, and only then logs `started lease=<id>
   alias=<alias> pid=<pid>`, so a `started` line always follows its record.

**If the record cannot be written, the pass stops the holder at once**, since an unrecorded holder
could never be stopped later, and logs `failed` with the holder's PID:
- **When it is gone afterwards:** `The record could not be written, so the holder was stopped, or
  had exited: <error>`.
- **When it still runs:** `The record could not be written, and the holder could not be stopped, so
  it runs unrecorded: <error>`.

**Every other failure of steps 1 to 8** logs `failed lease=<id> alias=<alias> pid=- reason=The
holder could not be started: <reason>`.

**Any failure from the moment the pass decides to start** (the checks of
[Before the start](#before-the-start) included) counts to a keeper as a fast failure
([Watching](#watching)).

### Timings of a start

| Step | Bound |
|---|---|
| One create call | 10 seconds, then it is abandoned and counts as a call that threw |
| Create retries | 3 tries, 2 seconds apart: at most 34 seconds in all |
| Launcher deadline | 30 seconds after it is taken, just before the request is written |
| The pass's wait for the result, when the create succeeded | Until 35 seconds after the request, then the withdrawal |
| The pass's wait for the result, when the create failed | None: the withdrawal comes at once, at most about 34 seconds after the request |
| The wait after a launcher claimed first | 60 more seconds, counted from the first try to withdraw |
| The longest a start can hold its pass, and so `board-holder.lock` | The withdrawal comes at most 35 seconds after the request, whether the create succeeded or failed, then up to 60 more: about 95 seconds |
| A start whose create failed, and whose withdrawal wins | The time of its create calls only: at most about 34 seconds after three tries, and the time of one call when that call returns a non-zero code |
| Polling of the result file | Every 100 ms |

**Nothing in a start is retried in a loop (D14).** A start that fails is retried by a later pass,
under a keeper's backoff.

**The bound on each create call keeps a start inside its 95 seconds.** The prototype sets no bound
on a create call, so one call that hangs holds the pass, and `board-holder.lock`, for as long as it
hangs ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

> **Note.** In the prototype the launcher is the keeper's own script, run as
> `pwsh -NoProfile -NonInteractive -ExecutionPolicy Bypass -File <script> -Launch <request>`
> (`scripts/board-holder.ps1`). It compiles its native helpers on each run, which was observed to
> take about 1 to 3 seconds of the start (D30).

### A pass that is killed

**A pass can be killed at any point, by a kill of its process or of the keeper.** What it leaves:
- **Both locks are released by the OS** when the process ends ([Locks](#locks)).
- **The record is the old one or the new one, never a mix**, because it is replaced atomically
  ([docs/state.md](state.md#the-keepers-files)). A temporary file of the replacement MAY be left
  beside it.
- **A half-finished stop keeps the record**, so the next pass checks the holder again and stops
  what is left of it.
- **A half-finished start can leave a holder running unrecorded**, as in step 8 of
  [The handshake](#the-handshake), and its launch files behind ([Q13](#open-questions)).
- **No line is logged for the killed pass.** When the keeper itself was killed and the holder its
  pass was stopping is already dead, the new keeper's first pass finds no trusted holder and
  starts one at once if a lease is ready, with no grace ([Q11](#open-questions)).

## Watching

**A keeper (`-Watch`) runs passes in one process, for its whole life.** It first takes
`board-holder-keeper.lock` ([Locks](#locks)), then loops:

1. **The look**, before the pass. If it tracks a holder, it checks that holder by PID, creation
   time and command line, against the keeper's `-SshExe` and the alias the holder was started or
   adopted with. The look reads the process only, never the record.
   - **Alive**, and tracked for at least 60 seconds (`-FastFailureSeconds`): the holder is proven,
     and the count of fast failures goes to 0.
   - **Alive, and younger:** nothing changes.
   - **Gone, no longer the same process, or the check itself fails:** the holder is no longer
     tracked. If it was never proven, that is a fast failure and the count goes up by one; if it
     was proven, the count is unchanged. Then the keeper waits once, at once, before the pass: the
     longer of the release grace and, for an unproven holder, the backoff for the new count, and
     at least `-IntervalSeconds`. It logs `grace seconds=<n>` or `backoff seconds=<n>` for that
     wait ([Release grace](#release-grace)); with the defaults, a proven holder gives
     `grace seconds=95`. Then it goes on to the pass.
2. **The pass**, with the pass time taken now ([A pass](#a-pass)). It counts toward `-MaxPasses`
   whether or not it ran to the end.
3. **What the pass did**, for tracking:

| The pass | Then |
|---|---|
| `started` a holder | The keeper tracks it, unproven, from the end of this pass |
| `kept` a holder it does not already track (by PID and creation time) | The keeper adopts it: tracks it from the end of this pass, proven, since its age is unknown, in place of any holder it tracked before. The count is unchanged, and no grace follows |
| `kept` the holder it tracks | Nothing changes |
| `lost` the holder the keeper tracks (card #0068) | No holder is tracked. If it was never proven, that is a fast failure and the count goes up by one ([Release grace](#release-grace)) |
| Decided to start, and failed (any `failed` line from [Starting a holder](#starting-a-holder)) | No holder is tracked; the count of fast failures goes up by one |
| Stopped a holder, or tried to (`stopped`, or a `failed` stop) | No holder is tracked; the count goes to 0 |
| Anything else (`idle`, `skipped`, a pass that failed before deciding) | Nothing changes |

4. **If `-MaxPasses` passes have run, exit 0.**
5. **The wait:**

| After | Wait | Line |
|---|---|---|
| A failed start | The backoff for the count, at least `-IntervalSeconds` | `backoff seconds=<n>` |
| A stop, or a tried stop | The release grace, at least `-IntervalSeconds` | `grace seconds=<n>` |
| A pass that logged `lost` (card #0068) | The release grace, or the longer backoff if the holder was unproven, at least `-IntervalSeconds` | `grace seconds=<n>` or `backoff seconds=<n>` |
| Anything else | `-IntervalSeconds` | None |

**One iteration can wait twice**: once after the look, when the look found the tracked holder gone,
and again after the pass, when that pass then failed to start a holder. The two waits are
sequential, each with its own line, so they add: with the defaults, a proven holder's end gives
`grace seconds=95`, then a failed start gives `backoff seconds=30` before the next pass. The rule
"the longer, never the sum" applies within one wait only ([Release grace](#release-grace)).

**No error ends the loop.** Each of the look, the pass and the wait runs on its own: an error that
escapes one is logged as a `failed lease=- alias=- pid=- reason=<text>` line, if the log can be
written, and the loop goes on. **Every iteration MUST wait at least `-IntervalSeconds`, on every
path, the error paths included**, so no failure can make a tight loop (D14). A wait whose line
cannot be logged still waits its full time.

**The keeper's state lives only in its process.** The count of fast failures, the tracked holder
and whether it is proven start empty in a new keeper. What a new keeper inherits is the record, and
its first pass adopts the holder the record names, if it is trusted.

### Fast failures and backoff

**A fast failure is a start that did not give a live holder, or a tracked holder found gone before
it was proven**, by a look, or by a pass that logged `lost` for it. A holder found gone that was
never proven is a fast failure however long ago it started, because it may have died at any time
since the look before.

- **Only proof of life resets the count**: a look that finds the tracked holder alive at least 60
  seconds after tracking began. A stop, or a tried stop, also resets it. An idle pass does not.
  **An adopted holder is under the same rule** (architect, 2026-11-19, for E0.06-T02-S05): it is
  proven from adoption, its count unchanged then, and a later look at least 60 seconds after
  tracking began that finds it alive resets the count, as for a holder the keeper started.
- **A panic in a look, a pass or a wait is an error the keeper does not exit for** (architect,
  2026-11-19, for E0.06-T02-S05): the keeper catches it, logs the matching reason of
  [Reasons](#reasons) with the panic's message as `<error>`, and goes on as for any other error
  there. With the pinned traits no look returns an error, so a panic is the only way those reasons
  arise; a keeper that died of one would break "never exits for an error once its loop has
  started".
- **After the n-th fast failure in a row, the backoff is `min(BackoffMaxSeconds, BackoffBaseSeconds
  * 2^(n-1))` seconds**, and the wait is that or `-IntervalSeconds`, whichever is longer.
- **With the defaults, a start that keeps failing waits** 30, 60, 120, 240, then 300 seconds for
  every failure after, each logged `backoff seconds=<n>`, and each wait comes after the pass that
  failed.

### Release grace

**When a look finds a tracked holder gone, the keeper MUST NOT start a holder until
`-ReleaseGraceSeconds` has passed, counted from that look (D30, E0.02-T04-S03).** A holder that
ends leaves its session on the box, which keeps the forwards until the box's sshd drops it. A
holder started meanwhile fails on `ExitOnForwardFailure`, and leaves another session for sshd to
time out.

**Where each grace counts from:**
- **A holder that a look finds gone:** from that look, which comes up to one interval and a pass
  after the holder ended. The wait comes before that iteration's pass.
- **A holder the keeper stopped, or tried to:** from the end of the pass that stopped it. The wait
  comes after that pass.
- **A tracked holder that a pass logged `lost` for (card #0068, below):** from the end of that
  pass. The wait comes after that pass.

> **Note, observed.** Before the grace, a killed holder left the board down for 2 minutes 49
> seconds, against about 90 seconds of sshd's bound; with it, 1 minute 56 seconds, with no failed
> holder between (E0.02-T04-S03's live run, D30). These are measurements of today's box, not
> requirements.

- **The default, 95 seconds, MUST be used when `-ReleaseGraceSeconds` is not given.** It is derived
  from today's box: sshd's `ClientAliveInterval` 15 times `ClientAliveCountMax` 5, plus 20 seconds
  for the probe already in flight (0.02.T1.1, D12). For another host the same rule gives its
  value: its sshd's keepalive interval times its count, plus a margin for one probe in flight. A
  host with other keepalive settings needs another value, and where a keeper gets it for a host
  is [Q19](#open-questions).
- **It applies to every holder the keeper started or adopted, whether it died or the keeper stopped
  it** (the architect's ruling, 2026-11-17). The stop terminates the holder, which leaves the
  box's session holding the forwards, so the next holder, for the next lease, waits the grace from
  the stop.
- **It does not apply to a keeper that has had no holder**, such as one that starts with no holder
  recorded, or one whose grace has passed: it starts its first holder at once. The grace keeps no
  state after its wait.
- **It does not apply to a holder stopped because its record could not be written.** That stop
  runs milliseconds after the start, before ssh can have bound the forwards, so nothing is held on
  the box. That failure gets the backoff only.
- **With a fast failure, the wait is the longer of the grace and the backoff, never their sum**,
  and one line is logged: `backoff seconds=<n>` when the backoff is at least the grace, else
  `grace seconds=<n>`, with the seconds actually waited. A grace set above `-BackoffMaxSeconds`
  therefore waits the grace.
- **Bounds.** Every wait is at least `-IntervalSeconds`. No wait is longer than the greatest of
  the grace, `-BackoffMaxSeconds` and `-IntervalSeconds`. One holder's end gives one bounded wait, and the
  pass after it starts a holder if a lease is ready, so the grace cannot keep the board down for
  good.
- **With the defaults, a started holder that keeps dying unproven waits** 95 (`grace`), 95
  (`grace`), 120 (`backoff`), 240 (`backoff`), then 300 (`backoff`) seconds before each next start.

**Card #0068 (a holder that dies mid-pass skips the grace) is carried in here as a requirement.**
- **When the keeper tracks a holder and its pass finds no trusted holder** (the tracked one's
  process gone or no longer the same process, the record missing, or the record naming a process
  that is not trusted), **the pass MUST NOT start a holder, and logs `lost`** for the tracked
  holder. A record that names another holder that is trusted is kept or stopped instead
  ([The order of a pass](#the-order-of-a-pass)). The keeper then stops tracking it, counts a fast
  failure if it was never proven, and waits after that pass: the release grace, or the longer
  backoff if the holder was unproven, and at least `-IntervalSeconds`, logged `grace seconds=<n>`
  or `backoff seconds=<n>` as for any other wait. The pass after the wait starts the next holder
  if a lease is ready.
- **The grace counts from the end of that pass**, as after a stop. So the next start comes one
  wait after the pass that found the holder gone, never in that pass.
- **What the pass needs from the keeper is the tracked holder's identity**: its PID, creation time,
  lease ID and alias, which the keeper holds in its own process from the look before. No file
  carries it. A one-pass run tracks nothing, so this rule never applies to one ([Locks](#locks)).
- **The pass logs exactly one action line, `lost lease=<id> alias=<alias> pid=<pid>`**, naming
  the tracked holder from the keeper's own identity of it (D35; A-09, B-38;
  [Q6](#open-questions), closed). It is logged whether or not a lease is ready. `lost` says the
  keeper no longer has the holder it tracked, whether that process ended or only its record did;
  the process, if it still runs, is left alone ([Trusting the record](#trusting-the-record)). A
  check MAY depend on the word.
  - **Why a new word:** `idle` says no lease is ready and names no holder, and `started`, `kept`
    and `stopped` each say something that did not happen. A new word lets a person see why the
    grace line after it is there.
- **The window is the time between the look and the pass's trust check**, usually a few seconds,
  but it includes the wait for `board-holder.lock`.
- **This closes #0068 once the rewrite meets it**: its done-when is a check with a stub holder that
  exits mid-pass ([Checking the keeper](#checking-the-keeper)).
- **Source:** the review of E0.02-T04-S03. In the prototype, the pass finds no trusted holder,
  removes the record and starts a holder at once; it fails under `ExitOnForwardFailure` while the
  dead session holds the ports, and only then does the grace apply. The cost is one failed holder,
  one more session for the box to time out, and up to about a minute more with no board
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
  When that pass finds no lease ready, the prototype logs `idle`, the holder stays tracked, and
  the next look applies the grace, so the gap opens only while a lease is ready. It never logs
  `lost`.

### What a keeper does over time

**With the defaults, as seen in `board-holder.log`:**

| Event | Lines |
|---|---|
| Logon with a lease ready and no record | `started`, then `kept` every 30 seconds or so |
| Logon or restart with a running, recorded holder | `kept` on the first pass, with no new holder and no new login on the box |
| The lease is released or expires | `stopped`, then `grace seconds=95`, then `idle` (or `started` for another ready lease) |
| The holder is killed | Within about 30 seconds, `grace seconds=95`; then `started` |
| The holder ends between a look and its pass, or its record is removed | `lost` for it, then `grace seconds=95`; then `started` if a lease is ready |
| A holder that exits at once (the ports still held) | `failed ... reason=The holder could not be started: The holder exited at once; ...`, then `backoff seconds=30` |
| Another connection holds the forwards | `failed ... reason=The holder could not be started: The holder exited at once; ...` after each start, with `backoff` waits growing to `backoff seconds=300` |
| A second keeper starts | One `another keeper is running pid=<n>` line, from the second keeper, which exits |
| The keeper is killed | No line. The task starts a new keeper within about a minute, and its first pass logs `kept` for a holder that still runs, or `started` or `idle` |

**Passes are spaced by the wait plus the pass's own time**, so a keeper notices a holder's end up
to `-IntervalSeconds` after it, and the grace counts from that notice.

## Locks

**The keeper uses two locks, each a file in the state folder held open by the OS with a sharing
mode that refuses a second taker.** The open handle is the lock; the file's contents are not. A
lock is released when its handle is closed, and Windows closes it when its process ends for any
reason, a kill included, so neither lock can outlive its holder. Neither file is ever removed.
Their paths, share modes and contents are [docs/state.md](state.md#the-keepers-files)'s. The CLI's
`leases.lock` is not the keeper's; it is [docs/state.md](state.md#concurrency)'s, and the keeper
never takes it.

**Both locks are per state folder.** A keeper with another `AGENTSPACE_STATE_DIR` (a check's, say)
runs beside the real one, and neither lock sees the other.

### board-holder.lock: one pass at a time

- **What holding it means:** a pass is running, in a one-pass run or in a keeper. So two passes
  never both start a holder (one holder per host, D30), and never act on one record at once.
- **Who takes it:** every pass, as its first step, before it looks at anything. It is created if
  missing, and held as [docs/state.md](state.md#the-keepers-files) says.
- **For how long:** the whole pass, up to about 95 seconds when it starts a holder
  ([Timings of a start](#timings-of-a-start)), and released when the pass ends, whatever happened.
- **A second taker** tries again every 100 ms for up to 10 seconds. One that still cannot take it
  logs `skipped lease=- alias=- pid=- reason=locked`, never `idle`, does nothing else, and its run
  exits 0; a keeper goes on to its usual wait.
- **An error other than the file being in use** fails the pass with `failed lease=- alias=- pid=-
  reason=The pass stopped on an error: <error>` and exit 1. The prototype does not meet this
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

### board-holder-keeper.lock: one keeper at a time

- **What holding it means:** a keeper is running. One-pass runs never take or look at it.
- **Who takes it:** every keeper, once, before its first pass (E0.02-T05-S01, card #0067). It is
  created if missing, held as [docs/state.md](state.md#the-keepers-files) says, and its content
  replaced with the keeper's PID.
- **For how long:** the keeper's whole life. It is never released before the process ends.
- **A second taker tries once and never waits.** When the file is held, it reads the first 32 bytes
  it can, and if they are 1 to 10 digits after trimming blanks, logs `another keeper is running
  pid=<those digits>`, else `another keeper is running pid=-`. It then exits 0, with no pass and no
  wait.
- **Any other error taking it** logs `failed lease=- alias=- pid=- reason=The keeper lock could not
  be taken: <error>`, when the log can be written, and exits 1. A keeper MUST NOT loop without the
  lock. The task's repeating trigger tries again within a minute, so this is no tight loop.
- **The PID is for that log line only, and can be stale.** Something other than a keeper holding
  the file, such as a scanner for a moment, also refuses a keeper, and the next firing tries again.
- **The holder never carries it.** The handle is not inheritable, and the holder inherits only its
  two log handles ([Starting a holder](#starting-a-holder)).

**A one-pass run beside a keeper is allowed**, and contends with it only on `board-holder.lock`. It
acts on the same record; a one-pass run with another `-SshExe` drops the keeper's record and
orphans its holder ([Trusting the record](#trusting-the-record)).

**A one-pass run keeps no state, so it never waits a grace or a backoff.** One run while a keeper
waits out a grace can start a holder at once, which fails on `ExitOnForwardFailure` and leaves
another session on the box for sshd to time out. That is allowed; anything that runs passes beside
a keeper SHOULD expect it. The keeper then finds a record it did not write: its next pass adopts
the holder if it is trusted, or drops the record if not.

## Log lines

**The keeper appends its lines to `board-holder.log`**, and writes each line to stdout as well.
The file's bytes (encoding and line ending) and its share mode are
[docs/state.md](state.md#the-keepers-files)'s. The file is opened for each line; an open that
fails is tried up to 5 times, 50 ms apart. **Each line MUST land whole at the end of the file even
when two processes write at once.** The writer MUST NOT find the end of the file and then write
there, as two separate steps. The prototype may not meet this
([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
Nothing parses the file but checks and people. `board-holder.log` is never rotated
([Q7](#open-questions)).

**A line that still cannot be written after the 5 tries is dropped, and nothing is undone for
it:**
- **What the pass did stands.** A holder it started stays started and recorded, and one it stopped
  stays stopped. A keeper tracks by what its pass did, never by the line.
- **A pass whose own line was dropped ends as a pass that met an error**: it tries once to log
  `failed lease=- alias=- pid=- reason=The pass stopped on an error: <error>` in its place, which
  is dropped too if the log still cannot be written, and a one-pass run exits 1
  ([Exit codes](#exit-codes)). A `skipped` line is a pass's own line. A `rotated` line is not: a
  dropped one changes nothing.
- **A keeper's own line that is dropped ends nothing.** A wait whose line was dropped still waits
  its full time, a dropped `failed` line leaves the loop running, and a second keeper whose
  `another keeper is running` line was dropped still exits 0 ([Watching](#watching),
  [Locks](#locks)).
- **A dropped line is not written to stdout either, today.** A check MUST NOT depend on whether
  one reaches stdout.

### A pass's line

```
<time> <action> lease=<lease> alias=<alias> pid=<pid>[ reason=<reason>]
```

- **`<time>`** is the pass time ([The order of a pass](#the-order-of-a-pass)).
- **`<action>`** is one of `started`, `kept`, `stopped`, `lost`, `idle`, `failed`, `skipped` and
  `rotated`, in lowercase. `lost` is logged only by a keeper's pass (card #0068,
  [Release grace](#release-grace)).
- **`lease`, `alias` and `pid`** name the holder the action concerns: the one started, kept,
  stopped or lost, or for `failed` the one being started or stopped. A field with nothing to name
  is `-`.
  Any whitespace character inside a value is written `_`.
- **`reason`** is written for `failed` and `skipped` only, is the last field, and runs to the end of
  the line. Each run of control characters (U+0000 to U+001F, and U+007F) in it is written as one
  space, and it is trimmed. An empty reason is written `Unknown.`
- **A pass writes one action line**, before which it MAY write one `rotated` line.

| Action | `lease` | `alias` | `pid` | `reason` |
|---|---|---|---|---|
| `started` | The chosen lease | Its alias | The holder's | None |
| `kept` | The record's | The record's | The record's | None |
| `stopped` | The record's | The record's | The record's | None |
| `lost` | The tracked holder's, as the keeper holds it | The same | The same | None |
| `idle` | `-` | `-` | `-` | None |
| `skipped` | `-` | `-` | `-` | Always `locked` |
| `rotated` | `-` | `-` | `-` | None |
| `failed`, lease state unreadable or another error of the pass | `-` | `-` | `-` | Below |
| `failed`, a start | The chosen lease | Its alias | `-`, or the holder's when it started but could not be recorded | Below |
| `failed`, a stop | The record's | The record's | The record's | Below |

**Examples:**

```
2026-11-18T05:07:00Z started lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112
2026-11-18T05:07:30Z kept lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112
2026-11-18T06:00:01Z stopped lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112
2026-11-18T06:01:36Z idle lease=- alias=- pid=-
2026-11-18T06:02:06Z skipped lease=- alias=- pid=- reason=locked
2026-11-18T07:12:30Z lost lease=7d2e9a40-3c5b-4e1f-8a6d-1b9c0f2e4a77 alias=agentspace-swimcoach-7d2e9a40 pid=25160
```

### A keeper's own lines

```
<time> backoff seconds=<n>
<time> grace seconds=<n>
<time> another keeper is running pid=<n>
<time> failed lease=- alias=- pid=- reason=<reason>
```

- **`<time>` is the current second when the line is written**, not a pass time.
- **`seconds=<n>`** is the whole seconds the keeper waits after the line, the `-IntervalSeconds`
  floor included.
- **`pid=<n>`** is the running keeper's PID as its lock file gives it, or `-`
  ([Locks](#locks)).
- **A keeper's `failed` line** has the pass line's grammar and reason rules, with `-` in every
  field.

### Reasons

**A reason is for a person.** The texts below are today's, and the rewrite SHOULD keep them; a
check SHOULD match only the start of a reason, up to the first `:` where there is one. Two meanings
MUST be kept in any wording: a start that may have left a holder running unrecorded MUST say so,
and so MUST a holder left running because its record could not be written.

| Where | Reason |
|---|---|
| The lease state cannot be read | `Lease state could not be read: <error>` |
| An error a pass does not handle, the matcher's self-check included | `The pass stopped on an error: <error>`; for the matcher, `<error>` is `The holder command matcher failed its own checks: <case>; <case>` |
| `board-holder.lock` is held | `locked` |
| A start | `The holder could not be started: <reason>`, where `<reason>` is from [Before the start](#before-the-start) or [The handshake](#the-handshake) |
| A record that cannot be written | The two texts at the end of [The handshake](#the-handshake) |
| A stop | The texts in [Stopping a holder](#stopping-a-holder) |
| A keeper's look | `The look at the holder stopped on an error: <error>` |
| A keeper's pass, escaping the pass's own handling | `The pass stopped on an error: <error>` |
| A keeper's wait | `The wait stopped on an error: <error>` |
| The keeper lock | `The keeper lock could not be taken: <error>` |

### The holder's log

**`board-holder-ssh.log` holds the holder's stdout and stderr, appended**; its bytes are
[docs/state.md](state.md#the-keepers-files)'s. Nothing parses it, and nothing but the holder writes
to it.

**Rotation:**
- **At the start of every pass that holds `board-holder.lock`**, before the lease state is read,
  a `board-holder-ssh.log` larger than 1 MiB (1,048,576 bytes) is renamed to
  `board-holder-ssh.log.1`, replacing any older one, and the pass logs
  `<time> rotated lease=- alias=- pid=-`.
- **A failure to rotate, or to log the rotation, MUST NOT stop the pass**, and is not logged.
- **A holder that has the file open keeps writing to it under its new name**, since it opened the
  file shared for deleting. The next holder opens a new `board-holder-ssh.log`.
- **So only the named file is bounded.** A single long-lived holder that writes more than 1 MiB
  keeps writing to `board-holder-ssh.log.1`, or to a file that a later rotation replaced, until it
  exits. A holder run with `-N` writes little.

### The launcher's error log

**`board-holder-launcher-errors.log` is the task's launcher's, not the keeper's**
([The task's launcher](#the-tasks-launcher)). Each line is
`<time> <message>`, where `<time>` is UTC in ISO 8601 with seven fractional digits and a `Z`
(`2026-11-18T05:07:00.1234567Z`). Its bytes and line ending are
[docs/state.md](state.md#the-keepers-files)'s.

## The installed task

**The keeper runs unattended as a Windows scheduled task for the user, `agentspace board holder`
(D30).** Registering it is persistent configuration on Terry's laptop, so it MUST only be done with
his yes (E0.02-T03-S01; vision.md). The installer itself asks for nothing; the yes is the caller's
to get.

**The task runs the installed programs**: the task's launcher, `agentspace-keeper-task.exe`, which
runs the keeper, `agentspace-keeper.exe`. Both are in the install folder, written `<install>` on
this page: `%USERPROFILE%\.agentspace\bin`, with `USERPROFILE`'s value written out (D37;
[operator.md, Programs and install layout](operator.md#programs-and-install-layout)).

### The task's fields

| Field | Value | Why |
|---|---|---|
| Name | `agentspace board holder`, in Task Scheduler's root folder (`\`) | |
| Description | `Keeps the agentspace board holder running while a lease is ready (D30).` | |
| Trigger 1 | At logon of the installing user | Starts the keeper at logon |
| Trigger 2 | Once, starting at the time of registration, written in UTC with a `Z`; repeating every minute (`PT1M`) with no duration, which Task Scheduler takes as indefinite; no end | Starts a crashed keeper again at its next firing, so within about a minute (E0.02-T04-S02). The firings fall on whole minutes after the registration time, whenever the keeper died. UTC, so a change of the laptop's zone does not move it |
| Principal | The installing user, logon type Interactive, run level Limited | Standard rights. The task runs only while the user is logged on ([Q18](#open-questions) covers logoff) |
| Execution time limit | None (`PT0S`) | The keeper runs for the whole session |
| Multiple instances | IgnoreNew | A firing while the keeper runs does nothing |
| Start when available | Yes | A missed start runs when it can |
| Start on batteries | Allowed | |
| Stop on going to batteries | No | |
| Restart count | 3 | Kept, though it was observed not to restart a keeper (below) |
| Restart interval | `PT1M` | |
| Action | The task's launcher by full path, `<install>\agentspace-keeper-task.exe`, with the one argument `-Watch` | The launcher passes `-Watch` to the keeper ([The task's launcher](#the-tasks-launcher)) |
| Action's working folder | `%USERPROFILE%`, with its value written out | So the task holds open neither the install folder nor a checkout, as the holder does not ([Starting a holder](#starting-a-holder), item 4) |

**Every other setting is Task Scheduler's default for a new task**, among them that the task is
enabled and is not hidden in Task Scheduler's library. So a registration over a task that was
disabled by hand enables it again ([Q14](#open-questions)).

**The action MUST name the programs in the install folder, never in a build's output folder or a
checkout**, since a build's folder can be cleaned and a git worktree is deleted after its branch
merges, and the task would then point at nothing. The installer enforces it
([The installer](#the-installer)).

> **Note.** The prototype's task runs `%USERPROFILE%\.agentspace\BoardHolderLauncher.exe` with the
> arguments `"<pwsh>" "<checkout>\scripts\board-holder.ps1" -Watch`, each quoted, where `<pwsh>` is
> PowerShell 7's full path, in the working folder `<checkout>`, the main checkout of the repository
> it was installed from. The rewrite's installer replaces that task
> ([operator.md, Until v0.06 retires the prototype's entry points](operator.md#until-v006-retires-the-prototypes-entry-points)).

**What Task Scheduler does with this task, as observed on today's laptop.** These are observations,
not requirements; a check MUST NOT assert them:
- **RestartCount did not restart a keeper whose action exited non-zero.** A killed keeper was not
  restarted in E0.02-T03-S01. That RestartCount covers only a task that fails to start is the most
  likely reading of one observation, not a finding (D30). **The repeating trigger, not
  RestartCount, is what brings a crashed keeper back**, and that part is required. In
  E0.02-T04-S02 a killed keeper was started again 37 seconds later, and its first pass logged
  `kept`.
- **A firing ignored because the keeper runs set the task's last result to `0x800710E0`**, which
  is IgnoreNew at work, not a failure.
- **A killed keeper's task ended with last result `0xFFFFFFFF`**, the task's launcher passing on the
  killed process's exit code.
- **A trigger with a past start and no duration kept firing across reboot and logon** (D30, from
  22 such tasks on the laptop).

**What logoff does is unobserved.** The task runs only while the user is logged on, and Windows
ends a user's processes at logoff, which would end the keeper and, since the holder runs as the
same user in the same session, the holder too. Whether it does, and what the box's session then
does, is [Q18](#open-questions). The next logon's trigger starts a keeper, which finds the record
untrusted if the holder is gone, and starts a holder at once.

### The task's launcher

**`agentspace-keeper-task.exe` is a Windows program, with no console, that runs the keeper and
waits for it**, so the task shows nothing on screen. It ships in the install folder with the other
programs, built with them, and is never built at install
([operator.md, The programs](operator.md#the-programs)).

**Why a separate launcher (D37, which settles [Q8](#open-questions)).** The keeper MUST be a console program:
a person and the checks run it from a shell and read its stdout, stderr and exit code, and a shell
neither waits for a program with no console nor gives it its own. A console program that the task
ran itself would open a console window at every logon and every firing. So the task runs a launcher
with no console, which runs the keeper with no window.

- **Arguments:** the keeper's arguments (`-Watch`), passed on unchanged.
- **It runs `agentspace-keeper.exe`, found in its own folder**
  ([operator.md, How each program finds the others](operator.md#how-each-program-finds-the-others)),
  with those arguments, with no console window, in its own working folder (the task's), and waits.
- **It exits with the keeper's exit code**, so a keeper that ends with an error is a failed run of
  the task.
- **It refuses with exit 2**, logging why, a keeper argument that holds a `"` or ends in `\`
  (`Refused: argument <n> holds a quote or ends in a backslash; only plain arguments such as -Watch
  are passed on.`), since quoting would split or join it. `<n>` counts the launcher's arguments
  from 1.
- **It exits 1, logging the error's kind and message, when the keeper could not be started or
  waited for**, a missing `agentspace-keeper.exe` included. The message is
  `Could not start <keeper's full path>: <kind>: <message>`, or `Could not wait for ...`, where
  `<kind>` is the error kind's name, such as `NotFound` (architect, 2026-11-19, for
  E0.06-T02-S07). A launcher that cannot learn its own path logs that and exits 1; it creates the
  log's folder when missing before it appends; and with `AGENTSPACE_STATE_DIR` unset and
  `USERPROFILE` unset or empty it has no log folder, logs nowhere, and never falls back to the
  system's profile folder.
- **It passes its own environment to the keeper unchanged**, and redirects none of the keeper's
  streams. The keeper runs with no window, so its stdout and stderr reach no one.
- **Its log** is `board-holder-launcher-errors.log`, in the folder
  [docs/state.md](state.md#the-keepers-files) gives for it. A failure to write it is ignored.
- **End in Task Scheduler ends the launcher.** Whether it also ends the keeper is not required:
  the prototype's End leaves the keeper running (E0.02-T04-S03's live check, D30), and
  [Stopping and starting the keeper](#stopping-and-starting-the-keeper) works either way.

> **Note.** The prototype's launcher is built at install time from
> `<checkout>\scripts\BoardHolderLauncher.cs` by the .NET Framework's C# compiler, as a Windows
> program, to `%USERPROFILE%\.agentspace\BoardHolderLauncher.exe`. Its first two arguments are
> PowerShell 7's full path and the keeper script's, before the keeper's arguments, so `<n>` of its
> quote refusal is 3 for the first keeper argument. It runs
> `<pwsh> -NoProfile -NonInteractive -File "<script>" "<argument>"...`, and refuses fewer than 2
> arguments with exit 2 (`Refused: the launcher needs the path of pwsh.exe and of
> board-holder.ps1, and got <n> argument(s).`).

### The installer

```
<installer> [-DryRun | -Uninstall]
```

**`<installer>` is `agentspace-keeper-install.exe`, in the install folder**, which a person runs as
`agentspace-keeper-install` ([operator.md, The programs](operator.md#the-programs)). A real install
MUST be run from the install folder. Note on where this came from: the prototype's is
`pwsh -NoProfile -File <checkout>\scripts\install-board-holder.ps1`, run from the main checkout.

**Its arguments** are `-DryRun` and `-Uninstall`, each a switch that takes no value, matched
exactly and without regard to case ([operator.md, Argument style](operator.md#argument-style)).
There are no positional arguments, and there is no help: `-h`, `--help` and `-?` are unknown
arguments.

**Usage errors (D37, which settles [Q10](#open-questions)).** A usage error writes two lines to stderr,
`agentspace-keeper-install: <message>` and then `Usage: agentspace-keeper-install [-DryRun |
-Uninstall]`, changes nothing, and exits 2. The checks run in this order, and the first that fails
is the one reported:

| # | When | Message |
|---|---|---|
| 1 | An argument that is neither name, taken left to right | `Unknown argument '<raw>'.` |
| 1 | A name given twice | `-<Name> is given twice.` |
| 2 | `-DryRun` with `-Uninstall` | `-DryRun and -Uninstall cannot be given together.` |

`<Name>` is the argument's name as this page spells it; `<raw>` is what the caller gave. So
`-DryRun -Uninstall` changes nothing, as a dry run never does. **Check 1 is one scan from left to
right that ends at the first failure, asking of each argument, in this order, whether it is a
known name and whether that name was given before** (architect, 2026-11-19, for E0.06-T02-S06,
as [Usage errors](#usage-errors) reads the keeper's check 1). So `-DryRun -DryRun -Bogus` gives
`-DryRun is given twice.`, and `-Bogus -DryRun -DryRun` gives `Unknown argument '-Bogus'.`

**`-DryRun` builds the task's definition and prints the fields the installer sets, and changes
nothing**: no registration, no start, no removal. It does not look for a running keeper, and it
does not check that the programs exist. It exits 0. Its output, one `Label: value` line each, in
this order:

```
Warning: <folder> is not the install folder <install>. A real install from here is refused, because the task would run programs that a cleanup can remove.
Name: agentspace board holder
Description: Keeps the agentspace board holder running while a lease is ready (D30).
Trigger 1: AtLogOn user=<DOMAIN>\<user>
Trigger 2: Once StartBoundary=<registration time> EndBoundary=none[ StopAtDurationEnd=<True|False>]
Repetition 2: Interval=PT1M Duration=indefinite
Principal user: <DOMAIN>\<user>
RunLevel: Limited
LogonType: Interactive
ExecutionTimeLimit: PT0S (none)
MultipleInstances: IgnoreNew
StartWhenAvailable: True
DisallowStartIfOnBatteries: False
StopIfGoingOnBatteries: False
RestartCount: 3
RestartInterval: PT1M
Action Execute: <folder>\agentspace-keeper-task.exe
Arguments: -Watch
WorkingDirectory: <profile>
Runs: <folder>\agentspace-keeper.exe -Watch
Window: hidden
Start: a real install starts the task immediately after registering it.
Mode: dry run; nothing was registered or started.
```

- **`<folder>` is the folder of the installer's own executable**, `<install>` is
  `%USERPROFILE%\.agentspace\bin`, and `<profile>` is `%USERPROFILE%`. Every path is printed
  expanded.
- **The `Warning:` line appears only when `<folder>` is not `<install>`**, compared as full paths
  without regard to case.
- **`StopAtDurationEnd`** appears when the repetition reports that field. **The rewrite's
  definition sets it, to `False`, so its dry run always prints ` StopAtDurationEnd=False`**
  (architect, 2026-11-19, for v0.06, gaps FA-17 and FB-17).
- **`StartBoundary` is the time the dry run built its definition**, so a later real install
  registers a later one.
- **The rest of FA-17 and FB-17, ruled by the architect on 2026-11-19 for v0.06:**
  - **Every line of a dry run goes to stdout**, the `Warning:` line included, and nothing to
    stderr.
  - **`<registration time>` is the laptop's local time with its offset, to the whole second:**
    `YYYY-MM-DDTHH:MM:SS+HH:MM` (or `-HH:MM`), the form Task Scheduler stores a boundary in.
  - **`<DOMAIN>\<user>` is the current user's account as Windows names it**, the form of `ca`'s
    `<owner>` ([operator.md, ca](operator.md#ca)): for a local account, the computer's name, a
    backslash and the user name. Like an instance ID, it MUST be withheld from anything committed
    or reported, since the repository is public.
  - **A real install's read-back lines (step 6) are the dry run's `Trigger 1:`, `Trigger 2:` and
    `Repetition 2:` lines, in the same forms**, read from the task as Task Scheduler registered it,
    on stdout. Its `Removed` line and its final line go to stdout too, and its warnings to stderr.
  - **No check runs the installer** (conformance.md, [FB-25](conformance.md#coverage-of-the-operator-and-the-keeper)),
    so these lines are shown only by v0.06's cutover, when the real task is registered.
- **A trigger of another kind** prints `Trigger <n>: <its kind>`, and a repetition line is printed
  for any trigger that has an interval, with `Duration=indefinite` when it has no duration.
- Note on where this came from: the prototype's `Warning:` line says that `<checkout>` is a git
  worktree, not the main checkout. Its `Action Execute:`, `Arguments:`, `WorkingDirectory:` and
  `Runs:` lines name its own task ([The task's fields](#the-tasks-fields), the note), it prints a
  `LauncherSource:` line before `Start:`, and its last line ends `nothing was built, registered or
  started.`

**A real install, in this order:**
1. **Outside the install folder, it refuses**, with `agentspace-keeper-install: <folder> is not the
   install folder <install>. The task MUST run the installed keeper, because a build's folder can
   be cleaned or deleted. Install the programs, then run this from <install>.` on stderr, and exit
   2.
2. **It checks that `agentspace-keeper.exe` and `agentspace-keeper-task.exe` are in its folder.**
   A missing one fails with `agentspace-keeper-install: <path> is missing; install the programs
   again.` and exit 1.
3. **While a keeper runs, it refuses**, with exit 2 and nothing changed, stopped or killed. **A
   keeper runs when the task's state is Running, or when `board-holder-keeper.lock` in
   `%USERPROFILE%\.agentspace` is held** ([Locks](#locks)). **The installer MUST NOT read
   `AGENTSPACE_STATE_DIR` for this.** That variable is a check's and a worktree's, set in the shell
   the installer may be run from ([docs/state.md](state.md#what-moves-which-files)), so following it
   would look for the task's keeper in a throwaway folder. The task's keeper runs with the user's
   logon environment, which SHOULD NOT set it, and so uses `%USERPROFILE%\.agentspace`. Where the
   logon environment does set it, the check misses that keeper, and the outcome is the one this
   step's Why gives for any missed keeper. The lock is held when the file
   exists and opening it as a keeper does (for reading and writing, sharing read only, never
   creating it) fails because another process has it open; an open that succeeds is closed at once.
   The keeper's PID is then read from the file as a second keeper reads it. The message, on stderr:

   ```
   agentspace-keeper-install: A keeper is running (task state: <state>; keeper PID: <pid>). Stop it first: End the task in Task Scheduler (Stop-ScheduledTask), then stop the keeper's process by the PID above, once the keeper lock still gives that PID and that process runs the keeper. Its holder keeps running, and the keeper this install starts adopts it.
   ```

   `<state>` is `not registered` with no task. `<pid>` is the PID the lock file gives, `-` when the
   lock is held and the file gives none, and `none found` when only the task's state refused.
   **Why:** a registration while a keeper runs would leave that keeper running its old program,
   and the new task's keeper would find the keeper lock held and exit; ending the task may stop only
   the task's launcher, so the keeper MUST be stopped by its PID (D30, card #0067). The lock finds a keeper
   whoever started it, the prototype's included, which a match on a process's command line does not
   ([Q12](#open-questions)).

   **Which process is the keeper, for that stop:** the one whose PID `board-holder-keeper.lock`
   gives while the lock is held, checked just before the stop to still be so, and whose program is
   the keeper: `agentspace-keeper.exe`, or, for the prototype's keeper, `pwsh.exe` with
   `board-holder.ps1` and `-Watch` on its command line. A running process's PID is not reused, so
   no other match on a command line is needed. The PID alone is not enough: something other than
   a keeper can hold the file, and the PID in it can then be stale ([Locks](#locks)).
   **This paragraph binds a person, not the installer** (architect, 2026-11-19, for v0.06, gap
   FB-27): the installer finds a keeper by the lock and the task's state alone, and MUST NOT match
   a process's command line, the prototype's script name included. The prototype's names here are
   for v0.06's change-over, and leave this page with the prototype.
4. **It registers the task, replacing any task of that name with no prompt, and starts it at
   once.** A failure to register or start ends the install with exit 1.
5. **It removes the prototype's launcher, `%USERPROFILE%\.agentspace\BoardHolderLauncher.exe`, if
   that file exists**, which the new task does not use, and prints `Removed <path>.` A failure to
   remove it prints `Warning: <path> could not be removed: <error>` to stderr, and does not change
   the exit code.
6. **It prints the registered task's trigger lines, read back, then** `Registered and started
   'agentspace board holder'. Log: %USERPROFILE%\.agentspace\board-holder.log` (the path expanded),
   and exits 0.

> **Note.** Where steps 1 to 3 came from: the prototype refuses from a git worktree, rather than
> from outside an install folder; it builds the task's launcher where the rewrite checks for it, and that
> build fails while the task runs the launcher; and it finds a running keeper by a `pwsh.exe`
> process whose command line holds its checkout's `scripts\board-holder.ps1` path, compared with
> case, and `-Watch` after a blank or a `"`. Its refusal names `install-board-holder.ps1` and that
> path.

**`-Uninstall`** ends the task if it is registered, unregisters it and prints `Unregistered
'agentspace board holder'.`, or prints `No task 'agentspace board holder' to remove.` It then
removes the prototype's launcher, `%USERPROFILE%\.agentspace\BoardHolderLauncher.exe`, if present,
and prints `Removed <path>.` It exits 0, or 1 when the unregistration or that removal fails.
**It never removes a program from the install folder**, which the lease CLI and the operator share,
and it leaves `%USERPROFILE%\.agentspace` and every other file in it. It is not refused outside the
install folder, and it does not stop the keeper: ending the task may stop only the task's launcher, so the
keeper's process MUST still be stopped by its PID, checked as step 3 of a real install says (D30). A failure to end the task is
ignored.

**A removal of the prototype's launcher can fail on a launcher still exiting (unobserved).** The
installer removes it right after it ends the task, without waiting for the launcher's process to
exit, and a launcher still exiting holds its file. Running `-Uninstall` again then removes it
([Q21](#open-questions)). The prototype's own `-Uninstall` deletes its launcher the same way.

**Any other error ends the installer with exit 1**, with one line on stderr that starts
`agentspace-keeper-install: `. The prototype prints PowerShell's own error record then, and its
arguments are bound by PowerShell, so an unknown argument there is PowerShell's error, with exit 1.

## Stopping and starting the keeper

**To stop the keeper:**
1. **End the task** (Task Scheduler's End, or `Stop-ScheduledTask`). Today that stops only the
   launcher.
2. **Stop the keeper's process by its PID, the process [The installer](#the-installer), step 3,
   defines as the keeper.** Never by name: another keeper process may be running, such as a check's keeper
   with its own state folder, or a one-pass run.
3. **To keep it stopped, also disable the task, or remove it with `-Uninstall`.** Otherwise the
   repeating trigger starts a keeper again within a minute. A later real install enables a disabled
   task again and starts it.

**The holder keeps running throughout**, unsupervised. It holds the board until it dies or a keeper
stops it.

**An orphaned keeper**, left by step 1 without step 2, keeps the board held, and the task's next
firing starts a second keeper that logs one `another keeper is running pid=<n>` line and exits. That
is one line a minute, about 90 KB of `board-holder.log` a day, until the orphan is stopped. That is
accepted (D30): End is a step taken on purpose.

**To start it again:** start the task (`Start-ScheduledTask`), or wait for the repeating trigger,
or log on. The new keeper's first pass adopts the recorded holder (`kept`), with no new holder and
no new login on the box, when the holder is still trusted.

**To load new keeper code**, the keeper MUST be stopped and started again: a running keeper read
its program once, when it started, and a merge does not reach it. A re-registration (a real
install) is refused while a keeper runs, so it, too, comes after the keeper is stopped.

**Before a re-install, the task SHOULD be disabled as well, or the install run within the minute.**
A keeper started by the repeating trigger between the installer's check and its registration
keeps running the old program, and the new task's keeper finds the keeper lock held and exits. A real install enables the task again
([Q14](#open-questions)).

## Checking the keeper

**A conformance check of the keeper drives it only as a separate process (D34)**, through what this
page documents:
- **It MUST set `AGENTSPACE_STATE_DIR` to a throwaway folder**, writes its own `leases.json` there
  ([docs/state.md](state.md#the-lease-state-file)), and reads `board-holder.log`, the record and
  the processes the keeper starts.
- **It stubs ssh with `-SshExe`**, naming a `.cmd` that prints a line and stays alive until it is
  stopped, connecting nowhere and touching no file. A stub receives the holder's arguments on its
  own command line, which is how a check finds it, and does not receive the keeper's environment
  ([Streams and environment](#streams-and-environment)).
- **It controls time with `-Now`** for a one-pass run, and with `-IntervalSeconds`,
  `-BackoffBaseSeconds`, `-BackoffMaxSeconds`, `-ReleaseGraceSeconds`, `-MaxPasses` and the hidden
  `-FastFailureSeconds` for a keeper.
- **It MAY watch the state folder for the launch files** while a holder is started, and hold them
  to [docs/state.md](state.md#a-holders-launch-files) and [The handshake](#the-handshake): that
  each start writes a request, a claim and a result under one `<hex>`, and that the pass removes
  all three. They are part of the contract ([Starting a holder](#starting-a-holder)).
- **It MAY see the holder's environment through its stub**, which can write it to
  `board-holder-ssh.log`, to show that a variable set only for the keeper does not reach the
  holder ([Streams and environment](#streams-and-environment)).
- **It MUST NOT call the hidden `-Launch`**, and MUST NOT run the installer for real, touch the real
  task, or touch the user's real state folder or `~/.ssh`.
- **It MUST stop every stub holder it started**, by its checked PID.

> **Note.** The prototype's suite uses `scripts/test-board-holder-stub.cmd` as its stub.

**Card #0066 (the keeper's checks flake under load) is carried in here as a requirement on every
check of the keeper.**
- **A check MUST wait on what it checks, never on a fixed sleep**: for a log line to appear, a
  process to exit, or a file to change, polling or watching until it does or a deadline passes.
- **The deadline MUST be long enough for a loaded laptop**, and a check MUST fail with what it was
  waiting for when the deadline passes.
- **The keeper's checks MUST pass 5 times in a row with two other copies of the suite running**,
  which is #0066's done-when, and closes it once the rewrite's checks meet it.
- **Source:** the prototype's "Watch locked" case failed two checks on `main` in one run that
  shared the laptop with two other copies of the suite and a live keeper ("the loop ends on its own
  with exit 0", and "a later pass ran and logged idle"). Run alone, it passed three times out of
  three. Its checks assume a pass finishes within a fixed time.

**Card #0068's check:** a check MUST show that a tracked stub holder which exits between a keeper's
look and its pass's trust check gets the grace before the next start, with no start in that pass
([Release grace](#release-grace)). It is `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1`
([docs/conformance.md](conformance.md#the-files-that-are-expected-to-fail)), which settles
[Q9](#open-questions).

## Requirements the prototype does not meet yet

**Behavior this page requires that the prototype does not show, so a conformance check run against
the prototype is expected to fail here.** **The rewrite MUST NOT copy the prototype's behavior in
this table.**

| Requirement | The prototype | Card or question |
|---|---|---|
| When a keeper tracks a holder and its pass finds no trusted holder, the pass starts nothing and logs `lost` for it, and the grace (or the longer backoff) follows that pass | Removes the record and starts a holder at once; the grace applies only after that holder fails. With no lease ready it logs `idle`. It never logs `lost` | #0068; D35; [Q6](#open-questions) |
| The keeper's checks wait on what they check, never a fixed sleep, and pass 5 times in a row beside two other copies of the suite | The "Watch locked" case assumes a pass ends within a fixed time | #0066 |
| `-DryRun` changes nothing, whatever else is given | `-DryRun -Uninstall` uninstalls, since `-Uninstall` is checked first | [Q10](#open-questions) |
| A stop's failure reason says in words why nothing was stopped | Names an internal outcome of its stop routine | [Stopping a holder](#stopping-a-holder) |
| An error taking `board-holder.lock` other than the file being in use fails the pass with exit 1 | Retries every I/O error for 10 seconds and logs `skipped ... reason=locked`, exit 0; only access denied fails the pass | [Locks](#locks) |
| Each line of `board-holder.log` lands whole at the end of the file, even with two writers at once | Finds the end of the file when it opens the log, then writes there, so two writers may overwrite each other's line (not verified by a run) | [Log lines](#log-lines) |
| Each create call of a start is bounded at 10 seconds, and one that reaches the bound counts as a call that threw | Sets no bound on a create call, so one that hangs holds the pass and `board-holder.lock` for as long as it hangs | D35; [Q15](#open-questions) |
| `-SshExe`'s default is the value of `SystemRoot`, then `\System32\OpenSSH\ssh.exe` | Uses the fixed text `C:\Windows\System32\OpenSSH\ssh.exe`. The two differ only on a machine whose Windows folder is elsewhere, so no check on a machine seen so far fails on it | D35; [Command line](#command-line) |
| The installer's usage errors are its own: two lines on stderr, nothing changed, exit 2 | PowerShell's binder answers an unknown argument with its own error record and exit 1 | [The installer](#the-installer); [Q10](#open-questions) |
| The installer finds a running keeper by the task's state or `board-holder-keeper.lock` | Matches a `pwsh.exe` process's command line against its own checkout's script path, and finds none when the process list cannot be read | [The installer](#the-installer); [Q12](#open-questions) |

## Open questions

**What reading the code and the docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080), E0.04-T05-S04 (#0091) and E0.04-T05-S02 (#0089), every question here is
either closed or marked "Later, not blocking", with the reason in its last cell, and none of
those blocks building a drop-in of contract v0.

| | Question | Settled by |
|---|---|---|
| **Q1** | **Later, not blocking:** **Is a keeper for a laptop that is not Windows in scope?** Everything here is Windows: Job Objects, WMI, Task Scheduler, `cmd.exe`. D4 makes the host's platform a parameter, but says nothing of the laptop's. | Contract v0's caller runs on Windows (D8; [lease-api.md](lease-api.md#the-program)), and this page specifies a Windows keeper in full, so a drop-in has nothing to guess. Later: the architect, with the lease CLI's Linux and macOS callers |
| **Q2** | **Later, not blocking:** **One holder per host, or one in all?** D30 says one per host; today every lease is on one host, so the keeper runs one in all and picks across every ready lease. With a host per project (v0.07, #0047), the choice, the record (one holder today) and the locks need a per-host form. | Every lease is on one host until v0.07, so one in all is one per host, and this page states it ([What the keeper does](#what-the-keeper-does)). Later: v0.07 (#0047) |
| **Q3** | **Closed by D37 (architect, 2026-11-18).** ~~What is the rewrite's keeper entry point called, and does it keep PowerShell-style arguments (`-Watch`, `-SshExe`)?~~ It is `agentspace-keeper.exe`, a console program in the install folder, and its usage lines and messages name it `agentspace-keeper` ([Command line](#command-line); [operator.md, Programs and install layout](operator.md#programs-and-install-layout)). It keeps the single-dash arguments this page gives ([operator.md, Argument style](operator.md#argument-style)). | Closed |
| **Q4** | **Closed (E0.04-T05-S04, 2026-11-18; D35; A-26, B-41): the holder, and the launcher that creates it, MUST run with the user's logon environment, never with the keeper's**, and a variable set only for the keeper MUST NOT reach the holder or its ProxyCommand ([Streams and environment](#streams-and-environment)). The prototype meets it: its launcher is created by WMI. | Closed |
| **Q5** | **Closed (E0.04-T05-S04, 2026-11-18; D35; A-07, B-37, A-26): the launch request, claim and result files are part of contract v0.** Every start MUST go through the handshake with them; which program the launcher is, and how it is told the request's path, are the build's ([Starting a holder](#starting-a-holder)). Whether a compiled keeper could break away from the task's job directly is still unobserved, and a later contract MAY drop the files once a run on the laptop shows another mechanism meets the start's guarantees. | Closed |
| **Q6** | **Closed (E0.04-T05-S04, 2026-11-18; D35; A-09, B-38): the pass logs `lost lease=<id> alias=<alias> pid=<pid>` for the tracked holder**, a new action in the grammar ([Log lines](#log-lines)), whether or not a lease is ready, then the keeper's `grace` or `backoff` line ([Release grace](#release-grace)). When the record names another holder that is trusted, the table's earlier rows win: the pass keeps it and the keeper adopts it with no grace, since nothing is started, or the pass stops it and the grace follows the stop ([The order of a pass](#the-order-of-a-pass)). | Closed |
| **Q7** | **Later, not blocking:** **Should `board-holder.log` be rotated?** It grows without bound: about 90 KB a day with an orphaned keeper, and a line per pass (about 2,900 a day at the default interval) otherwise. | This page states the rule to build, that it is never rotated ([Log lines](#log-lines)), and nothing reads it but people and checks. Later: the architect |
| **Q8** | **Closed by D37 (architect, 2026-11-18).** ~~Does the rewrite need a separate task launcher?~~ Yes: `agentspace-keeper-task.exe`, a program with no console that ships in the install folder. The keeper stays a console program, because a person and the checks run it from a shell, and the task runs the launcher so that no console window opens ([The task's launcher](#the-tasks-launcher)). Whether End also ends the keeper is not required, and [Stopping and starting the keeper](#stopping-and-starting-the-keeper) works either way. | Closed |
| **Q9** | **Closed (E0.04-T03-S02).** ~~How does a check make a stub holder exit inside the window between a keeper's look and its pass, without a fixed sleep?~~ The check holds `board-holder.lock`, waits for the keeper's `skipped ... reason=locked` line, which shows a pass waiting on the lock, stops the stub while that pass waits, and then releases the lock. The keeper logs nothing between its look and its lock wait, so the stop is one bounded wait after that line, 6 seconds, in the middle of a window that is nominally 5 seconds from each edge and nearer 3 under load, which the retry covers. An attempt that the log shows missed the window is not a result, and is run again, up to 3 times ([docs/conformance.md](conformance.md#the-files-that-are-expected-to-fail)) | E0.04-T03-S02 |
| **Q10** | **Closed by D37 (architect, 2026-11-18).** ~~What MUST the installer do when given both `-DryRun` and `-Uninstall`?~~ It refuses them as a usage error, changing nothing. Its usage errors are its own: two lines on stderr and exit 2 ([The installer](#the-installer)). | Closed |
| **Q11** | **Later, not blocking:** **Should a keeper that starts with a record whose holder is already gone wait the grace?** [Release grace](#release-grace) says a keeper that has had no holder, such as one that starts with no holder recorded, starts its first holder at once. With a stale record, the holder may have died just before the keeper started, as when both died together, and its session may still hold the forwards. Today it starts at once. | This page states the rule to build, that it starts at once ([A pass that is killed](#a-pass-that-is-killed)). The cost of being wrong is one holder that fails on `ExitOnForwardFailure`, which the backoff then spaces out. Later: the architect |
| **Q12** | **Later, not blocking:** **Should the installer's refusal be more than a partial guard?** It matches only this checkout's script path, compared with case, and finds no keeper when the process list cannot be read. A keeper it misses holds `board-holder-keeper.lock`, so the new task's keeper exits on it rather than running beside it, and the board stays held; but a missed keeper launched by the old task also holds the launcher exe, and the build then fails. A rewrite could ask the keeper lock instead of the process list. | A keeper the refusal misses still cannot run beside another, because of `board-holder-keeper.lock` ([Locks](#locks)), and the worst case is a failed install that leaves the old task in place. How the rewrite's installer finds a running keeper is stated with the installer ([The installer](#the-installer)): by `board-holder-keeper.lock`, not by the process list, which leaves this question to the prototype's guard alone. Later: the architect |
| **Q13** | **Later, not blocking:** **What MUST be done about a holder, and launch files, left by a pass that was killed mid-start?** Nothing removes the files today, so they collect in the state folder; removing one that a live pass still uses would break its handshake, so an age bound would be needed. A leftover `.result.json` names the holder's PID, creation time and command line: should the next pass check it as it checks a record, and adopt or stop the holder? Today such a holder, like one whose record was dropped ([Trusting the record](#trusting-the-record)), is never found again, keeps its host up and makes every start fail. | This page states the rule to build, that nothing removes or reads the leftovers ([A pass that is killed](#a-pass-that-is-killed)), and only a killed pass leaves them. Recovering such a holder is an addition no other behavior waits on. Later: the architect; [state.md Q4](state.md#open-questions) for the files |
| **Q14** | **Later, not blocking:** **Should a real install keep a task that was disabled by hand disabled?** It registers with Task Scheduler's defaults, so it enables the task again and starts it, though disabling the task is how [Stopping and starting the keeper](#stopping-and-starting-the-keeper) keeps the keeper stopped. This follows from the defaults and has not been observed. | This page states the rule to build, Task Scheduler's defaults, and says a later install enables the task again ([Stopping and starting the keeper](#stopping-and-starting-the-keeper)). An install is a step taken on purpose, with Terry's yes. Later: the architect; a run on the laptop |
| **Q15** | **Closed (E0.04-T05-S04, 2026-11-18; D35; A-25, B-39): each create call is bounded at 10 seconds.** A call that reaches the bound is abandoned and counts as a call that threw, with the error `The call did not return within 10 seconds.`: it is tried again up to 3 tries in all, 2 seconds apart, and then the request is withdrawn as after any failed create, since the call may still make a launcher ([The handshake](#the-handshake)). Three bounded tries take at most 34 seconds, so the withdrawal comes within 35 seconds of the request whether the create succeeded or failed, and a start holds its pass for about 95 seconds at most. The line a pass logs for it is the start's `failed` line, with the reason `The holder could not be started: Win32_Process.Create could not be called: The call did not return within 10 seconds.` and what became of the request after it. The prototype sets no bound ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)). | Closed |
| **Q16** | **Later, not blocking:** **Should a record whose process runs but cannot be opened to query it be kept, rather than removed?** Today it is removed and the process left running, which orphans a live holder; a stop in the same case keeps the record. | This page states the rule to build, that it is removed and the process left alone ([Trusting the record](#trusting-the-record)). The holder's own user can always open it, so the case needs a holder started with other rights. Later: the architect; a run on the laptop |
| **Q17** | **Closed (E0.04-T05-S04, 2026-11-18; D35; B-40, A-25).** The launcher's deadline is a UTC time. Every other duration is measured on one clock that local time never moves, the UTC clock or a monotonic one, which SHOULD count the time the laptop sleeps. A wait that spans a sleep MAY end at once on waking or run on, a step of the UTC clock MAY end one measured on it early or late, and a check MUST NOT depend on either ([Times](#times)). | Closed |
| **Q18** | **Later, not blocking:** **What does logoff do to the keeper and to a holder created outside the keeper's job?** Both run as the user in the user's session, so logoff would end them; the box's session then waits out sshd's bound. Unobserved. | Nothing on this page depends on it: the next logon's trigger starts a keeper, which finds the record untrusted if the holder is gone and starts one ([The installed task](#the-installed-task)). Later: a run on the laptop |
| **Q19** | **Later, not blocking:** **Where does a keeper get the release grace for a host other than today's box?** The default is today's box's bound. With hosts of other platforms (D4) or keepalive settings, it is a property of the host, which the lease or its driver would have to give. | Every lease is on today's box until v0.07, and `-ReleaseGraceSeconds` sets the grace for any other ([Release grace](#release-grace)). Later: the architect, with v0.07 (#0047) |
| **Q20** | **Later, not blocking:** **Does a holder's ProxyCommand end when the holder ends by itself?** A stop ends the holder's tree; a holder that dies or is killed by something else leaves its children untraceable by parent, and the keeper never looks for them. | This page states the rule to build, that the keeper never looks for them ([Stopping a holder](#stopping-a-holder)), and what the ProxyCommand does is [operator.md](operator.md#the-proxycommand)'s. Later: a run on the laptop |
| **Q21** | **Later, not blocking:** **Should `-Uninstall` wait for the launcher to exit before it deletes the exe?** It can fail on a launcher still exiting ([The installer](#the-installer)). Unobserved. | The installer's `-Uninstall` is stated as it is, and running it again removes the exe. Whether a compiled keeper keeps a launcher at all goes with the installer ([Q8](#open-questions)); where it keeps none, this concerns only a launcher that a prototype install left behind. Later: the architect; a run on the laptop |

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
- **Why the board's forwards need a holder at all**, and why only one connection on a host can
  hold them, is D12 and the README's "The localswim board on the box".

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
  holder that survived only when WMI created it, the claim trials, and the launcher's compile time.
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
  all three in v0.06 (#0055, D28).

### Words used here

| Word | Means |
|---|---|
| Holder | The ssh client process the keeper starts to hold the board's forwards: `<ssh> -N -o ExitOnForwardFailure=yes <alias>` ([The holder command](#the-holder-command)). |
| Pass | One look at the leases and the holder, and at most one action to make them agree ([A pass](#a-pass)). |
| Keeper | The program, or by default one running with `-Watch`: a pass every interval, for the life of the process ([Watching](#watching)). The installed task runs one. |
| One-pass run | The keeper's program run without `-Watch`: one pass, then exit. |
| Record | The keeper's record of the holder it started, `board-holder.json` ([docs/state.md](state.md#the-keepers-files)). |
| Ready lease | A lease whose stored state is `ready` and whose `expires_at` is after the pass time ([Which leases are ready](#which-leases-are-ready)). |
| Trusted holder | A process that the record names and that passes every identity check ([Trusting the record](#trusting-the-record)). |
| Tracked holder | The holder a keeper watches between passes: one it started, or one it adopted when a pass logged `kept` for it. |
| Proven | A tracked holder seen alive by a look at least `-FastFailureSeconds` (60) after the keeper began to track it, or one the keeper adopted, whose age is unknown. A keeper begins to track a holder it started when the pass that started it ends, not when the holder started. |
| Fast failure | A start that did not give a live holder, or a tracked holder found dead before it was proven. |
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
| `board-holder-launch.<guid>.json`, `.claim`, `.result.json` | The launch request, its claim and its result, which exist only while a holder is being started ([Starting a holder](#starting-a-holder)). |
| `board-holder-launcher-errors.log` | Written by the task's launcher, never by the keeper ([The task's launcher](#the-tasks-launcher)). |

**Every JSON file the keeper writes is replaced atomically
([docs/state.md](state.md#the-keepers-files)).**

### Times

- **Every time in a log line is UTC, in the form `YYYY-MM-DDTHH:MM:SSZ`**, truncated to the whole
  second. These are machine-readable and keep that form.
- **Durations on this page are in seconds, and MUST NOT be measured on local time**, so a change
  of the laptop's time zone moves nothing.
- **Which clock, today.** The 60-second proof of a holder, the launch deadline and every bounded
  retry are measured on the UTC clock. The waits (the interval, the backoff and the grace) are
  relative waits, which on Windows may not count time the laptop spends asleep or hibernated; that
  has not been observed. If they do not, a wait that spans a laptop sleep ends later than its
  seconds say, and never earlier. Which clock a conforming keeper MUST use across a laptop
  sleep is [Q17](#open-questions).

## What the keeper does

**A desktop-app session on a lease needs the board, and the app's own connection never carries the
board's forwards** (card #0042; README, "Use a lease from a project", step 4). The keeper supplies
them: while a lease is ready, it keeps one holder running through that lease's alias
([The board's forwards](#the-boards-forwards)).

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

**`<keeper>` stands for the keeper's entry point.** Today that is `pwsh -NoProfile -File
<checkout>\scripts\board-holder.ps1`, and its messages name it `board-holder.ps1`. What the
rewrite's entry point is called is open ([Q3](#open-questions)); its messages MUST name its own.

- **Argument names are matched exactly, without regard to case.** The prototype also ignores one
  `:` at the end of a name (`-Watch:`); the rewrite MAY, and a caller MUST NOT depend on it.
- **Every argument but `-Watch` takes one value, the next argument.** A next argument that is
  missing, empty, all whitespace, or starts with `-` is no value.
- **There are no positional arguments.**

| Argument | Value | Default | Applies to | Meaning |
|---|---|---|---|---|
| `-SshExe` | A full path | `C:\Windows\System32\OpenSSH\ssh.exe` | Both | The ssh client the holder runs. It MAY be a `.cmd` or `.bat`, which a check uses as a stub ([Checking the keeper](#checking-the-keeper)). A pass MUST be run with the same `-SshExe` as the pass that started the holder, or the holder is untrusted ([Trusting the record](#trusting-the-record)). |
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

**The prototype also accepts a hidden `-Launch <request path>`**, which runs it as the launcher of
[Starting a holder](#starting-a-holder), and exits 0 when it has answered. It is internal. Nothing outside the keeper MUST
call it, a conformance check included (D30), and the rewrite MAY start its holder another way.

### Usage errors

**A usage error writes three lines to stderr, logs nothing, and exits 2:** `board-holder.ps1:
<message>`, then the two usage lines:

```
Usage: board-holder.ps1 [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]
       board-holder.ps1 -Watch [-SshExe <full path>] [-IntervalSeconds <n>] [-MaxPasses <n>] [-BackoffBaseSeconds <n>] [-BackoffMaxSeconds <n>] [-ReleaseGraceSeconds <n>]
```

**Checks run in this order, and the first that fails is the one reported.** `<Name>` is the
argument's name as this page spells it (`-SshExe`, not the caller's spelling); `<raw>`, `<value>`
and `<path>` are what the caller gave.

| # | When | Message |
|---|---|---|
| 1 | An argument that is not a known name, taken left to right | `Unknown argument '<raw>'.` |
| 1 | A name given twice, `-Watch` included | `-<Name> is given twice.` |
| 1 | A name with no value | `-<Name> needs a value.` |
| 2 | A `-Watch`-only number without `-Watch`, checked in the table's order | `-<Name> is for -Watch.` |
| 2 | A number that is not a whole number from 1 to 86400 | `-<Name> '<value>' is not a whole number from 1 to 86400.` |
| 3 | `-Now` with `-Watch` | `-Now is not allowed with -Watch.` |
| 4 | `-BackoffMaxSeconds` less than `-BackoffBaseSeconds`, defaults included | `-BackoffMaxSeconds is less than -BackoffBaseSeconds.` |
| 5 | `-SshExe` not a fully qualified path | `SshExe '<path>' is not a full path.` |
| 5 | `-SshExe` holding a `"` | `SshExe '<path>' holds a quote.` |
| 5 | `-SshExe` a `.cmd` or `.bat` whose path holds a `%` | `SshExe '<path>' is a batch whose path holds a percent sign.` |
| 6 | `-Now` not exactly `YYYY-MM-DDTHH:MM:SSZ`, or not a real time | `-Now '<value>' is not a time in the form YYYY-MM-DDTHH:MM:SSZ.` |

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
  instead.

### Streams and environment

- **Every line the keeper logs is also written to stdout**, as it is logged
  ([Log lines](#log-lines)). Usage errors go to stderr only. Nothing else is written to either.
  Under the installed task, the keeper's stdout and stderr reach no one, so `board-holder.log` is
  the only record of what it did ([The task's launcher](#the-tasks-launcher)).
- **The keeper reads no stdin.**
- **`AGENTSPACE_STATE_DIR`**, when set and not empty, names the state folder; a check MUST set it
  to a throwaway folder ([Checking the keeper](#checking-the-keeper)). The keeper creates the state
  folder if it is missing.
- **`USERPROFILE`** names the holder's working folder and, with `AGENTSPACE_STATE_DIR` unset, the
  state folder.
- **The holder does not get the keeper's environment.** Today it is started with the user's logon
  environment, as any process Windows Management Instrumentation (WMI) creates is, so a variable
  set only for the keeper never reaches the holder or its ProxyCommand. Nothing MUST depend on the
  holder seeing the keeper's variables ([Q4](#open-questions)).

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
| None trusted, in a keeper that tracks a holder (card #0068, [Release grace](#release-grace)) | Any | Nothing | One line, never `started` ([Q6](#open-questions)) |
| None trusted | None ready | Nothing | `idle` |
| None trusted | One or more ready | Start one through the chosen lease | `started`, or `failed` |

8. **Release `board-holder.lock`.**

**The pass time is fixed when the pass begins**, before the lock is taken: `-Now` when given, else
the current UTC second. Every line of the pass carries it, so a `started` line can carry a time up
to about 95 seconds before it is written, plus the pass's wait for `board-holder.lock` (up to 10
seconds) and any time one create call takes beyond that ([Timings](#timings-of-a-start)). A
`skipped` line likewise carries the pass time, and is written about 10 seconds later, when the
lock wait ends.

### Which leases are ready

- **A lease is ready when its stored state is `ready` and its `expires_at` is strictly after the
  pass time.** A lease whose `expires_at` equals the pass time is expired, the same rule the lease
  CLI uses ([lease-api.md](lease-api.md#states)).
- **The lease state is read without a lock**, so a pass never blocks a lease command, and sees the
  file as it was before or after any one write, never part of one
  ([docs/state.md](state.md#concurrency)).
- **A state file that does not pass its schema cannot be read**, and fails the pass as in step 4.

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
running and untouched.** The removal has no log line of its own: the pass then goes on as with no
holder, logs `idle` or starts one if a lease is ready, and that line is the only sign that a holder
was lost. So a holder started with another `-SshExe` is orphaned, and a holder started for it then
most likely fails on `ExitOnForwardFailure` while the orphan holds the ports.

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
   the `"` opens or closes quoting; 2n+1 give n backslashes and a literal `"`; backslashes before
   anything else are literal; and in a run of quotes inside a token, every third is literal.
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

**A trusted holder whose record's lease is ready, by ID compared exactly, and whose ready lease's
`ssh.host_alias` equals the recorded alias exactly, is kept.** The pass logs `kept lease=<id>
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

**`stopped` MUST be logged only when the recorded holder is gone**, and the record MUST be kept
while it still runs, so that the next pass tries again (D30). A pass that stops a holder, or tries
to, MUST NOT start one; the next pass does.

**The new look is the trust check's query** ([Trusting the record](#trusting-the-record), item 2).
So when the stop could not open the holder to terminate it and the new look cannot open it either,
the pass logs `stopped` and removes the record although the process may still run; it is then an
orphan, as in [Trusting the record](#trusting-the-record).

> **Note.** The prototype's "for another reason" text names an internal outcome of its stop routine
> in place of `<why>` (`StopTree found NotRunning`, `IdentityDiffers` or `Matched`). The rewrite
> SHOULD say in words why nothing was stopped.

### Idle

**With no trusted holder and no ready lease, the pass logs `idle lease=- alias=- pid=-`** and
changes nothing. An idle line never names a holder.

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
4. **It has no window, starts in a new process group, and its working folder is `%USERPROFILE%`**,
   so that it never holds a worktree or a checkout open.
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

**The prototype meets these with a launch handshake through files in the state folder.** The
rewrite MAY meet them another way ([Q5](#open-questions)); if it uses these files, they MUST behave
as below. The files' names, fields and access rules are
[docs/state.md](state.md#the-keepers-files)'s.

### Before the start

**The pass refuses to start, and logs `failed lease=<id> alias=<alias> pid=- reason=The holder
could not be started: <why>`, when:**

| Why | Reason text |
|---|---|
| `-SshExe` is not a file | `SshExe '<path>' does not exist.` |
| The state folder's path holds a `"` | `The state folder '<path>' holds a quote.` |
| The keeper's own program path holds a `"` | `This script's path '<path>' holds a quote.` |
| The alias does not match `\A[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z`, case-sensitively | `Alias '<alias>' is not a host alias.` |

The form checks of [Usage errors](#usage-errors) row 5 come first, then this table's rows in
order; the first that fails is reported.

### The handshake

1. **The pass writes a launch request**, `board-holder-launch.<guid>.json`, naming the program, the
   command line, the holder's log, its working folder, the result file, and a deadline 30 seconds
   from now ([docs/state.md](state.md#the-keepers-files) has its fields). The deadline is taken
   just before the request is written, and every later bound of the start counts from it.
2. **The pass has WMI create a launcher process**: the keeper's own program again, given the
   request's path, with no window and `%USERPROFILE%` as its working folder. WMI creates it
   outside the pass's job, which is what lets the holder outlive the pass. WMI can be slow just
   after logon, so a create call that throws is tried up to 3 times, 2 seconds apart, and never
   more (D14). A create that returns a non-zero code is not retried.
   - **A create call that threw may still have made a launcher.** So a failed create is not the
     end: the pass withdraws the request (step 5), and waits for a launcher that claimed it.
3. **The launcher claims the request before it does anything else**, by creating its claim file,
   `board-holder-launch.<guid>.claim`, in create-new mode, which exactly one process can do.
   - **A launcher that cannot claim it starts nothing, writes nothing, and exits.** So a second
     launcher, from a create retry after a call that threw but had made one, starts nothing.
   - **A rename MUST NOT be used to claim**, because the loser's rename of a file already given
     that name can succeed too. Observed in a trial of four launchers racing for one request: a
     rename started two holders in 2 rounds of 9, and the claim file exactly one in 12 rounds of 12
     (D30).
   - **A launcher that claims the request and then cannot read it** (the pass has removed it)
     removes its claim and exits, starting nothing.
4. **The launcher starts the holder and answers**, by writing the result file,
   `board-holder-launch.<guid>.result.json`: the holder's PID, its creation time to the
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
   seconds from the request), or not at all when the create failed. **Then it withdraws the request
   by claiming it itself**, in the same create-new mode:
   - **If the pass's claim wins**, no launcher can claim the request, none starts a holder, and the
     start fails with `<why> Its request was withdrawn, so no holder was started.`
   - **If a launcher claimed it first**, that launcher may still start a holder. The pass waits on
     for its answer, up to 60 more seconds, and records the holder if one comes. If none comes, the
     start fails with `<why> A launcher claimed the request, or it could not be withdrawn, and no
     answer came in 60 more seconds, so a holder may be running unrecorded.`
   - **`<why>`** is `The launcher did not answer within 35 seconds.`, or the create's failure:
     `Win32_Process.Create could not be called: <error>` after three throws, or
     `Win32_Process.Create returned <code>.`
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
9. **The pass writes the record** for the holder, and logs `started lease=<id> alias=<alias>
   pid=<pid>`.

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
| Create retries | 3 tries, 2 seconds apart |
| Launcher deadline | 30 seconds after it is taken, just before the request is written |
| The pass's wait for the result | 35 seconds after the request, then the withdrawal |
| The wait after a launcher claimed first | 60 more seconds |
| The longest a start can hold its pass | 35 seconds after the request (the create's tries count inside it), then up to 60 more: about 95 seconds, plus any time a single create call takes beyond that |
| Polling of the result file | Every 100 ms |

**Nothing in a start is retried in a loop (D14).** A start that fails is retried by a later pass,
under a keeper's backoff.

**Each create call MUST be bounded.** The prototype sets no bound on it, so one call that hangs
holds the pass, and `board-holder.lock`, for as long as it hangs
([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet);
the bound is [Q15](#open-questions)).

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
| `kept` a holder it does not already track (by PID and creation time) | The keeper adopts it: tracks it from the end of this pass, proven, since its age is unknown. The count is unchanged |
| `kept` the holder it tracks | Nothing changes |
| Did not find the holder the keeper tracks trusted (card #0068) | No holder is tracked. If it was never proven, that is a fast failure and the count goes up by one ([Release grace](#release-grace)) |
| Decided to start, and failed (any `failed` line from [Starting a holder](#starting-a-holder)) | No holder is tracked; the count of fast failures goes up by one |
| Stopped a holder, or tried to (`stopped`, or a `failed` stop) | No holder is tracked; the count goes to 0 |
| Anything else (`idle`, `skipped`, a pass that failed before deciding) | Nothing changes |

4. **If `-MaxPasses` passes have run, exit 0.**
5. **The wait:**

| After | Wait | Line |
|---|---|---|
| A failed start | The backoff for the count, at least `-IntervalSeconds` | `backoff seconds=<n>` |
| A stop, or a tried stop | The release grace, at least `-IntervalSeconds` | `grace seconds=<n>` |
| A pass that did not find the tracked holder trusted (card #0068) | The release grace, or the longer backoff if the holder was unproven, at least `-IntervalSeconds` | `grace seconds=<n>` or `backoff seconds=<n>` |
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
it was proven.** A holder found gone that was never proven is a fast failure however long ago it
started, because it may have died at any time since the look before.

- **Only proof of life resets the count**: a look that finds the tracked holder alive at least 60
  seconds after tracking began. A stop, or a tried stop, also resets it. An idle pass does not.
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
- **A tracked holder that a pass did not find trusted (card #0068, below):** from the end of that
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
- **When the keeper tracks a holder and its pass does not find that holder trusted** (its process
  gone, no longer the same process, or the record missing or naming another), **the pass MUST NOT
  start a holder.** The keeper then stops tracking it, counts a fast failure if it was never
  proven, and waits after that pass: the release grace, or the longer backoff if the holder was
  unproven, and at least `-IntervalSeconds`, logged `grace seconds=<n>` or `backoff seconds=<n>`
  as for any other wait. The pass after the wait starts the next holder if a lease is ready.
- **The grace counts from the end of that pass**, as after a stop. So the next start comes one
  wait after the pass that found the holder gone, never in that pass.
- **What the pass needs from the keeper is the tracked holder's identity**: its PID, creation time
  and alias, which the keeper holds in its own process from the look before. The pass compares the
  record's trusted holder, if any, with it. No file carries it. A one-pass run tracks nothing, so
  this rule never applies to one ([Locks](#locks)).
- **The pass logs exactly one action line, and it MUST NOT be `started`.** Which action word it is
  (`idle`, or a new one) is [Q6](#open-questions). Until Q6 is settled, a check MUST NOT depend on
  that word, only on there being no `started` line in that pass and on the `grace` or `backoff`
  line after it.
- **The window is the time between the look and the pass's trust check**, usually a few seconds,
  but it includes the wait for `board-holder.lock`.
- **This closes #0068 once the rewrite meets it**: its done-when is a check with a stub holder that
  exits mid-pass ([Checking the keeper](#checking-the-keeper)).
- **Source:** the review of E0.02-T04-S03. In the prototype, the pass finds no trusted holder,
  removes the record and starts a holder at once; it fails under `ExitOnForwardFailure` while the
  dead session holds the ports, and only then does the grace apply. The cost is one failed holder,
  one more session for the box to time out, and up to about a minute more with no board
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
  When that pass finds no lease ready, it logs `idle`, the holder stays tracked, and the next look
  applies the grace, so the gap opens only while a lease is ready.

### What a keeper does over time

**With the defaults, as seen in `board-holder.log`:**

| Event | Lines |
|---|---|
| Logon with a lease ready and no record | `started`, then `kept` every 30 seconds or so |
| Logon or restart with a running, recorded holder | `kept` on the first pass, with no new holder and no new login on the box |
| The lease is released or expires | `stopped`, then `grace seconds=95`, then `idle` (or `started` for another ready lease) |
| The holder is killed | Within about 30 seconds, `grace seconds=95`; then `started` |
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
- **For how long:** the whole pass, up to about 95 seconds when it starts a holder, plus any time
  one create call takes beyond that ([Timings of a start](#timings-of-a-start)), and released when
  the pass ends, whatever happened.
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

### A pass's line

```
<time> <action> lease=<lease> alias=<alias> pid=<pid>[ reason=<reason>]
```

- **`<time>`** is the pass time ([The order of a pass](#the-order-of-a-pass)).
- **`<action>`** is one of `started`, `kept`, `stopped`, `idle`, `failed`, `skipped` and
  `rotated`, in lowercase.
- **`lease`, `alias` and `pid`** name the holder the action concerns: the one started, kept or
  stopped, or for `failed` the one being started or stopped. A field with nothing to name is `-`.
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
| Action | The task's launcher, given the PowerShell 7 executable's full path, the keeper script's full path and `-Watch`, each quoted: `"<pwsh>" "<checkout>\scripts\board-holder.ps1" -Watch` | |
| Action's working folder | `<checkout>`, the folder of the agentspace repository the installer ran from | |

**Every other setting is Task Scheduler's default for a new task**, among them that the task is
enabled and is not hidden in Task Scheduler's library. So a registration over a task that was
disabled by hand enables it again ([Q14](#open-questions)).

**`<checkout>` MUST be the main checkout, never a git worktree**, since a worktree is deleted after
its branch merges and the task would then point at nothing. The rewrite's action names its own
keeper program; whether it still needs a separate launcher is [Q8](#open-questions).

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
- **A killed keeper's task ended with last result `0xFFFFFFFF`**, the launcher passing on the
  killed process's exit code.
- **A trigger with a past start and no duration kept firing across reboot and logon** (D30, from
  22 such tasks on the laptop).

**What logoff does is unobserved.** The task runs only while the user is logged on, and Windows
ends a user's processes at logoff, which would end the keeper and, since the holder runs as the
same user in the same session, the holder too. Whether it does, and what the box's session then
does, is [Q18](#open-questions). The next logon's trigger starts a keeper, which finds the record
untrusted if the holder is gone, and starts a holder at once.

### The task's launcher

**A no-window program that runs the keeper and waits for it**, so the task shows nothing on
screen. It is built at install time from `<checkout>\scripts\BoardHolderLauncher.cs` by the .NET
Framework's C# compiler, as a Windows (not console) program, to the path
[docs/state.md](state.md#the-keepers-files) gives, never into the repository.

- **Arguments:** the full path of the PowerShell 7 executable, the full path of the keeper script,
  then the keeper's arguments (`-Watch`).
- **It runs** `<pwsh> -NoProfile -NonInteractive -File "<script>" "<argument>"...`, each keeper
  argument quoted as it is, with no window, in its own working folder (the task's), and waits.
- **It exits with the keeper's exit code**, so a keeper that ends with an error is a failed run of
  the task.
- **It refuses with exit 2**, logging why, fewer than 2 arguments (`Refused: the launcher needs the
  path of pwsh.exe and of board-holder.ps1, and got <n> argument(s).`), or a keeper argument that
  holds a `"` or ends in `\` (`Refused: argument <n> holds a quote or ends in a backslash; only
  plain arguments such as -Watch are passed on.`), since quoting would split or join it. `<n>`
  counts from 1 over all of the launcher's arguments, so the first keeper argument is 3.
- **It exits 1, logging the error's kind and message, when the keeper could not be started or
  waited for.**
- **It passes its own environment to the keeper unchanged**, and redirects none of the keeper's
  streams. The keeper runs with no window, so its stdout and stderr reach no one.
- **Its log** is `board-holder-launcher-errors.log`, in the folder
  [docs/state.md](state.md#the-keepers-files) gives for it. A failure to write it is ignored.
- **End in Task Scheduler stops only the launcher**, and leaves the keeper running (E0.02-T04-S03's
  live check, D30). See [Stopping and starting the keeper](#stopping-and-starting-the-keeper).

### The installer

```
<installer> [-DryRun] [-Uninstall]
```

**`<installer>` is `pwsh -NoProfile -File <checkout>\scripts\install-board-holder.ps1` today.** It
MUST be run from the main checkout.

**The PowerShell 7 executable named in the action is found on `PATH` when the installer runs, and
stored as a full path.** If it cannot be found, `-DryRun` and a real install fail with exit 1.
`-Uninstall` does not look for it.

**`-DryRun` builds the task's definition and prints the fields the installer sets, and changes
nothing**: no build, no registration, no start. It does not look for a running keeper. It exits 0.
Its output, one `Label: value` line each, in this order:

```
Warning: <checkout> is a git worktree, not the main checkout. A real install from here is refused, because the folder is deleted after its branch merges.
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
Action Execute: %USERPROFILE%\.agentspace\BoardHolderLauncher.exe
Arguments: "<pwsh>" "<checkout>\scripts\board-holder.ps1" -Watch
WorkingDirectory: <checkout>
Runs: <pwsh> -NoProfile -NonInteractive -File <checkout>\scripts\board-holder.ps1 -Watch
Window: hidden
LauncherSource: <checkout>\scripts\BoardHolderLauncher.cs
Start: a real install starts the task immediately after registering it.
Mode: dry run; nothing was built, registered or started.
```

- **The `Warning:` line appears only when `<checkout>` is a git worktree.** `<checkout>` is a
  worktree when its path holds a `.claude\worktrees\` segment (either slash), or when git reports a
  git folder for it that differs from its common git folder.
- **`StopAtDurationEnd`** appears when the repetition reports that field. Paths are printed
  expanded.
- **`StartBoundary` is the time the dry run built its definition**, so a later real install
  registers a later one.
- **A trigger of another kind** prints `Trigger <n>: <its kind>`, and a repetition line is printed
  for any trigger that has an interval, with `Duration=indefinite` when it has no duration.

**A real install, in this order:**
1. **From a worktree, it refuses**, with `install-board-holder.ps1: <checkout> is a git worktree,
   not the main checkout. The task MUST point at the main checkout, because a worktree is deleted
   after its branch merges. Run this from the main checkout.` on stderr, and exit 2.
2. **It creates `%USERPROFILE%\.agentspace`** if it is missing.
3. **While a keeper runs, it refuses**, with exit 2 and nothing changed, stopped or killed. A keeper
   runs when the task's state is Running, or when a `pwsh.exe` process's command line holds this
   checkout's full `scripts\board-holder.ps1` path, compared with case, and `-Watch` after a blank
   or a `"`, without regard to case. A process list that cannot be read finds no keeper, so the
   check then rests on the task's state alone ([Q12](#open-questions)). The message, on stderr:

   ```
   install-board-holder.ps1: A keeper is running (task state: <state>; keeper pwsh PID: <pid>, <pid>). Stop it first: End the task in Task Scheduler (Stop-ScheduledTask), then stop each keeper pwsh by its PID after checking its command line runs <checkout>\scripts\board-holder.ps1 -Watch. Its holder keeps running, and the keeper this install starts adopts it.
   ```

   `<state>` is `not registered` with no task, and the PID list is `none found` when only the
   task's state refused. **Why:** the build overwrites the launcher that a running task holds
   open, which fails, and ending the task stops only the launcher, so a registration then would
   start a second keeper beside the old one (D30, card #0067). The `"` case matters because the
   launcher quotes each argument, so a real keeper's command line ends `board-holder.ps1" "-Watch"`
   (E0.02-T04-S02's live run).
4. **It builds the launcher.** A build that fails ends the install with an error and exit 1.
5. **It registers the task, replacing any task of that name with no prompt, and starts it at
   once.** A failure to register or start ends the install with exit 1.
6. **It prints the registered task's trigger lines, read back, then** `Registered and started
   'agentspace board holder'. Log: %USERPROFILE%\.agentspace\board-holder.log` (the path expanded),
   and exits 0.

**`-Uninstall`** ends the task if it is registered, unregisters it and prints `Unregistered
'agentspace board holder'.`, or prints `No task 'agentspace board holder' to remove.` It then
deletes the launcher exe, if present, and prints `Removed <path>.` It exits 0, or 1 when the
unregistration or the deletion fails. It leaves `%USERPROFILE%\.agentspace` and every file in it.
It is not refused from a worktree, and it does not stop the keeper: ending the task stops only the
launcher, so the keeper's process MUST still be stopped by its checked PID (D30). A failure to end
the task is ignored.

**`-Uninstall` can fail on the launcher it has just ended (unobserved).** It deletes the launcher
exe right after it ends the task, without waiting for the launcher's process to exit; a launcher
still exiting holds its exe, and the deletion then fails with exit 1, with the task already
unregistered. Running `-Uninstall` again then removes the exe ([Q21](#open-questions)).

**Any other error ends the installer with exit 1.** In the prototype that is PowerShell's own error
record, and its arguments are bound by PowerShell, so an unknown argument is also PowerShell's
error.

## Stopping and starting the keeper

**To stop the keeper:**
1. **End the task** (Task Scheduler's End, or `Stop-ScheduledTask`). Today that stops only the
   launcher.
2. **Stop the keeper's process by its PID, after checking that its command line runs the keeper
   with `-Watch`.** Never by name: another PowerShell may be running.
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
A keeper started by the repeating trigger between the installer's check and its build makes the
build fail with exit 1, and leaves the old task registered. A real install enables the task again
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
| When a keeper tracks a holder and its pass does not find that holder trusted, the pass starts nothing, and the grace (or the longer backoff) follows that pass | Removes the record and starts a holder at once; the grace applies only after that holder fails | #0068 |
| The keeper's checks wait on what they check, never a fixed sleep, and pass 5 times in a row beside two other copies of the suite | The "Watch locked" case assumes a pass ends within a fixed time | #0066 |
| `-DryRun` changes nothing, whatever else is given | `-DryRun -Uninstall` uninstalls, since `-Uninstall` is checked first | [Q10](#open-questions) |
| A stop's failure reason says in words why nothing was stopped | Names an internal outcome of its stop routine | [Stopping a holder](#stopping-a-holder) |
| An error taking `board-holder.lock` other than the file being in use fails the pass with exit 1 | Retries every I/O error for 10 seconds and logs `skipped ... reason=locked`, exit 0; only access denied fails the pass | [Locks](#locks) |
| Each line of `board-holder.log` lands whole at the end of the file, even with two writers at once | Finds the end of the file when it opens the log, then writes there, so two writers may overwrite each other's line (not verified by a run) | [Log lines](#log-lines) |
| Each create call of a start is bounded | Sets no bound on a create call | [Q15](#open-questions) |

## Open questions

**What reading the code and the docs could not settle.** The clean-room review (E0.04-T03-S01)
closes them. Several need a run on the laptop, which the writer of this page did not make (D34).

| | Question | Settled by |
|---|---|---|
| **Q1** | **Is a keeper for a laptop that is not Windows in scope?** Everything here is Windows: Job Objects, WMI, Task Scheduler, `cmd.exe`. D4 makes the host's platform a parameter, but says nothing of the laptop's. | The architect |
| **Q2** | **One holder per host, or one in all?** D30 says one per host; today every lease is on one host, so the keeper runs one in all and picks across every ready lease. With a host per project (v0.07, #0047), the choice, the record (one holder today) and the locks need a per-host form. | v0.07 (#0047) |
| **Q3** | **What is the rewrite's keeper entry point called**, and does it keep PowerShell-style arguments (`-Watch`, `-SshExe`)? The task's action and every check name it. | v0.06 (#0055) |
| **Q4** | **MUST the holder get the user's logon environment rather than the keeper's?** Today it does, as a side effect of WMI. A rewrite that creates the holder directly would pass the keeper's own, which a check could then see. | The architect |
| **Q5** | **Are the launch request, claim and result files part of the contract**, or only the prototype's way to meet [Starting a holder](#starting-a-holder)'s guarantees? A compiled keeper MAY be able to create the holder outside its job directly, if Windows lets it break away; whether the keeper's own job (the task's) allows breakaway is unobserved. docs/state.md documents the files either way. | The architect; a run on the laptop |
| **Q6** | **What does the pass log when #0068's case happens?** [Release grace](#release-grace) fixes everything but the action word: one line, never `started`, then the keeper's `grace` or `backoff` line. No action in today's grammar fits: the holder was neither kept, stopped by the keeper, nor started. Candidates are a new action, such as `ended lease=<id> alias=<alias> pid=<pid>`, or `idle`. A new action changes the grammar that state.md and checks cite. Also open: when the record now names another trusted holder (a one-pass run's), whether that pass keeps and adopts it with no grace, or waits the grace for the tracked holder all the same. | The architect |
| **Q7** | **Should `board-holder.log` be rotated?** It grows without bound: about 90 KB a day with an orphaned keeper, and a line per pass (about 2,900 a day at the default interval) otherwise. | The architect |
| **Q8** | **Does the rewrite need a separate task launcher?** The launcher exists so the task shows no window, and so End stops only it. A keeper built as a Windows (no-console) program could be the task's action itself; End would then stop the keeper, which changes [Stopping and starting the keeper](#stopping-and-starting-the-keeper) and the installer's refusal. Today nothing ends the keeper when the launcher ends: it is not in a job that is closed with the launcher. | The architect |
| **Q9** | **Closed (E0.04-T03-S02).** ~~How does a check make a stub holder exit inside the window between a keeper's look and its pass, without a fixed sleep?~~ The check holds `board-holder.lock`, waits for the keeper's `skipped ... reason=locked` line, which shows a pass waiting on the lock, stops the stub while that pass waits, and then releases the lock. The keeper logs nothing between its look and its lock wait, so the stop is one bounded wait after that line, inside the window by a margin on both sides. An attempt that the log shows missed the window is not a result, and is run again, up to 3 times ([docs/conformance.md](conformance.md#the-files-that-are-expected-to-fail)) | E0.04-T03-S02 |
| **Q10** | **What MUST the installer do when given both `-DryRun` and `-Uninstall`?** This page requires that it changes nothing; whether it refuses with a usage error or prints what it would remove is open. The installer's usage errors and their exit code are also unspecified, since PowerShell's binder answers them today. | The architect |
| **Q11** | **Should a keeper that starts with a record whose holder is already gone wait the grace?** [Release grace](#release-grace) says a keeper that has had no holder, such as one that starts with no holder recorded, starts its first holder at once. With a stale record, the holder may have died just before the keeper started, as when both died together, and its session may still hold the forwards. Today it starts at once. | The architect |
| **Q12** | **Should the installer's refusal be more than a partial guard?** It matches only this checkout's script path, compared with case, and finds no keeper when the process list cannot be read. A keeper it misses holds `board-holder-keeper.lock`, so the new task's keeper exits on it rather than running beside it, and the board stays held; but a missed keeper launched by the old task also holds the launcher exe, and the build then fails. A rewrite could ask the keeper lock instead of the process list. | The architect |
| **Q13** | **What MUST be done about a holder, and launch files, left by a pass that was killed mid-start?** Nothing removes the files today, so they collect in the state folder; removing one that a live pass still uses would break its handshake, so an age bound would be needed. A leftover `.result.json` names the holder's PID, creation time and command line: should the next pass check it as it checks a record, and adopt or stop the holder? Today such a holder, like one whose record was dropped ([Trusting the record](#trusting-the-record)), is never found again, keeps its host up and makes every start fail. | The architect |
| **Q14** | **Should a real install keep a task that was disabled by hand disabled?** It registers with Task Scheduler's defaults, so it enables the task again and starts it, though disabling is the README's way to keep the keeper stopped. This follows from the defaults and has not been observed. | The architect; a run on the laptop |
| **Q15** | **What bound MUST a create call of a start have, and what does the pass log when it is reached?** [Timings of a start](#timings-of-a-start) requires one; the prototype has none. A call that timed out may still have made a launcher, so it would be treated like a call that threw. | The architect |
| **Q16** | **Should a record whose process runs but cannot be opened to query it be kept, rather than removed?** Today it is removed and the process left running, which orphans a live holder; a stop in the same case keeps the record. | The architect; a run on the laptop |
| **Q17** | **Which clock MUST the interval, the backoff, the grace and the 60-second proof use across a laptop sleep?** Today the proof is on the UTC clock and the waits are relative waits that may not count sleep. The box's sshd timeout runs while the laptop sleeps, so wall-clock time fits the grace best. | The architect; a run on the laptop |
| **Q18** | **What does logoff do to the keeper and to a holder created outside the keeper's job?** Both run as the user in the user's session, so logoff would end them; the box's session then waits out sshd's bound. Unobserved. | A run on the laptop |
| **Q19** | **Where does a keeper get the release grace for a host other than today's box?** The default is today's box's bound. With hosts of other platforms (D4) or keepalive settings, it is a property of the host, which the lease or its driver would have to give. | The architect |
| **Q20** | **Does a holder's ProxyCommand end when the holder ends by itself?** A stop ends the holder's tree; a holder that dies or is killed by something else leaves its children untraceable by parent, and the keeper never looks for them. | A run on the laptop |
| **Q21** | **Should `-Uninstall` wait for the launcher to exit before it deletes the exe?** It can fail on a launcher still exiting ([The installer](#the-installer)). Unobserved. | The architect; a run on the laptop |

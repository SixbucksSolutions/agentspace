# The agentspace keeper, reference v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for the keeper: the program on the laptop that holds the board's forwards on each
project's host for as long as a lease on that host is ready (D30, D50).** It is written so that the keeper can be rebuilt
without reading any of its code (D20, D34). It names every argument, decision, timing, lock, log
line and exit code, the handshake that starts a holder, when and how the keeper starts the guard
(v0.08, D56), and the logon task that runs the keeper unattended. The reasoning behind each rule is
in [decisions.md](decisions.md), cited as `D<n>`.

**What it does not own:**
- **Every file the keeper reads or writes is [docs/state.md](state.md#the-keepers-files)'s**: its
  path, its format (every field of each JSON file), its bytes (encoding and line ending), its share
  modes and access rules, how it is replaced atomically, who writes it, who reads it, and when it
  is created and removed. So are the path of the task's launcher and the folder of its error log.
  This page says when and why the keeper reads, writes or removes each file, the form of every log
  line, the rotation rule, what holding each lock means, and the launch handshake. The lease state
  file the keeper reads is [docs/state.md](state.md#the-lease-state-file)'s too, and so are the
  host registry, `hosts.json` ([state.md, The host registry](state.md#the-host-registry)), and
  the files of a holder on a host that is not the box
  ([state.md, Another host's holder](state.md#another-hosts-holder)).
- **Which host a lease's connection reaches** is the ProxyCommand's
  ([operator.md](operator.md#the-proxycommand)). The keeper only works out the same answer from
  the same files, to know which holder a lease belongs to ([Hosts](#hosts)).
- **What a lease is, its states and its fields** are [lease-api.md](lease-api.md#the-lease)'s.
- **The ProxyCommand, which every holder's connection runs**, and the token push it may make, are
  [operator.md](operator.md#the-proxycommand)'s.
- **Why only one connection on a host can hold the board's forwards** is D12. Why they need a
  holder at all is stated here, in [The board's forwards](#the-boards-forwards), and the decision
  to keep one is D30.
- **The guard, `agentspace-operator guard`** (D56, D63): what it does, the variables it reads and
  every line it logs are [operator.md, guard](operator.md#guard)'s. Its log, `guard.log`, and its
  lock, `guard.lock`, are [state.md, The guard's log](state.md#the-guards-log)'s and
  [state.md, The guard's lock](state.md#the-guards-lock)'s, and what a live lease is,
  [state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon)'s. This
  page says only when the keeper starts the guard, how, with what environment, and what the keeper
  logs for it ([Starting the guard](#starting-the-guard)).
- **What a release does to the lease's ssh clients**, the keeper's holder among them (D59), is
  [driver.md](driver.md) and [lease-api.md](lease-api.md)'s. This page says what the keeper does
  when it finds a holder a release ended ([Release grace](#release-grace)).

## Contents

- [Conventions](#conventions)
- [What the keeper does](#what-the-keeper-does)
- [Command line](#command-line)
- [A pass](#a-pass)
- [Starting a holder](#starting-a-holder)
- [Starting the guard](#starting-the-guard) (v0.08, D56)
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
- **A rule marked "architect's to confirm" is not ruled yet** (v0.08). It is a choice this page's
  drafter made where D56 to D63 and the architect's rulings left the point open. It is written as a
  requirement so a builder has one answer, and once the architect rules it is cited as "architect,
  2026-11-21, for v0.08-T01". **A rule cited that way is ruled**: the dev lead's pins P3 and P4 and
  the keeper's default guard program, here, and pin P15, in conformance.md. **The architect's pin
  P18 ruled every choice this page's drafter made for v0.08's first draft**, and each is cited that
  way, and its pin P19 ruled the two rules added in its review, the start during a wait and the
  bound on the detached start. No rule on this page is marked now.

### Where it runs

- **The keeper runs on the laptop, which is Windows today**, and this page specifies a Windows
  keeper: Windows processes, a Windows scheduled task, and Windows paths (`%USERPROFILE%`,
  `%SystemRoot%`). A keeper for another laptop OS is out of scope for v0
  ([Q1](#open-questions)).
- **The keeper never talks to a host or to AWS itself.** It reads the lease state file and the host
  registry, and starts and stops local ssh clients, the holders, at most one per host. Everything a
  holder's connection does on the far side goes through the lease's alias and its ProxyCommand.
  - **From v0.08 it also starts the guard (D56), a program that does talk to AWS and to hosts.**
    The keeper starts it as it starts holders, as a separate process that outlives it, and never
    does any of the guard's work itself: it makes no AWS call, connects to no host, stops no
    instance, and never reads what the guard writes ([Starting the guard](#starting-the-guard)).
- **A host's platform is a parameter of the contract (D4).** Nothing here depends on the host's OS,
  with one exception, marked "today's box" where it appears: the default release grace is derived
  from the keepalive settings of today's box's sshd ([Release grace](#release-grace)).
- **The prototype was `scripts/board-holder.ps1`, run by PowerShell 7, with its installer
  `scripts/install-board-holder.ps1` and its task launcher `scripts/BoardHolderLauncher.cs`.**
  They are named here only where a note says where a requirement came from. The rewrite replaced
  all three in v0.06 (#0055, D28) with `agentspace-keeper.exe`, `agentspace-keeper-install.exe`
  and `agentspace-keeper-task.exe`, installed with agentspace's other programs as
  [operator.md, Programs and install layout](operator.md#programs-and-install-layout) says.
  - **Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository.** The
    cutover that replaced its task and keeper on the laptop was E0.06-T05-S01
    ([the run record](runs/e0.06-t05-s01.md)). **Every note on this page that names the
    prototype is history:** it says where a requirement came from, or what the rewrite replaced.
    None tells a reader to run the prototype, and none binds a program that is built now. What
    binds is the rule each note stands beside, and the rows of
    [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
    stay as the record of what the rewrite was built to do differently.

### Words used here

| Word | Means |
|---|---|
| Holder | The ssh client process the keeper starts to hold the board's forwards on one host: `<ssh> -N -o ExitOnForwardFailure=yes <alias>` ([The holder command](#the-holder-command)). |
| Host | A machine leases connect to, named by its project: one record of the host registry ([Hosts](#hosts)). |
| The box | The host whose record has the stack `agentbox`; its project is `swimcoach` (D43, D55). Its holder's files are v0.06's. |
| Another host | Any host that is not the box. Its holder's files carry its project in their names ([Hosts](#hosts)). |
| A host's leases | The ready leases that are on that host, by [Which host a lease is on](#which-host-a-lease-is-on). |
| With only the box | Of a pass: one whose only host is the box ([The hosts of a pass](#the-hosts-of-a-pass)). Of a keeper: one whose every pass so far has had only the box. That is every keeper and pass of v0.06, and of a laptop with no other host's lease, record or holder. |
| Pass | One look at the leases and the holders, and for each host at most one action to make them agree ([A pass](#a-pass)). |
| Keeper | The keeper's program. "A keeper", where this page says one does something, is a run of that program with `-Watch`: a pass every interval, for the life of the process ([Watching](#watching)). `-Watch` is off unless it is given, and the installed task gives it. |
| One-pass run | The keeper's program run without `-Watch`: one pass, then exit. |
| Record | The keeper's record of the holder it started on a host: `board-holder.json` for the box ([docs/state.md](state.md#the-keepers-files)), `board-holder.<project>.json` for another host ([state.md, Another host's holder](state.md#another-hosts-holder)). "The record" on this page is the record of the host being looked at. |
| Ready lease | A lease whose stored state is `ready` and whose `expires_at` is after the pass time ([Which leases are ready](#which-leases-are-ready)). |
| Trusted holder | A process that a host's record names and that passes every identity check ([Trusting the record](#trusting-the-record)). |
| Tracked holder | The holder a keeper watches on a host between passes: one it started, or one it adopted when a pass logged `kept` for it. A keeper tracks at most one per host, and holds each one's host, PID, creation time, lease ID and alias in its own process. |
| Hold-off | The time during which a keeper starts no holder on one host: the release grace or the backoff, set when a holder there ends, is stopped or fails to start ([Watching](#watching)). Each host has its own. |
| Launcher | The process a start has created, outside the pass's job, to create the holder and answer the pass ([Starting a holder](#starting-a-holder)). |
| Proven | A tracked holder seen alive by a look at least `-FastFailureSeconds` (60) after the keeper began to track it, or one the keeper adopted, whose age is unknown. A keeper begins to track a holder it started when the pass that started it ends, not when the holder started. |
| Fast failure | A start that did not give a live holder, or a tracked holder that a look found gone, or that a pass logged `lost` for, before it was proven. |
| State folder | `AGENTSPACE_STATE_DIR` when it is set and not empty, else `%USERPROFILE%\.agentspace` (D21; [docs/state.md](state.md#the-keepers-files)). |
| The guard | `agentspace-operator guard`: one run of the operator's command that stops every host no live lease explains (D56; [operator.md, guard](operator.md#guard)). A keeper starts it, and never runs its logic itself ([Starting the guard](#starting-the-guard)). |
| The guard's start | A keeper's start of the guard: due at its first iteration and then once every `-GuardIntervalSeconds` (300), never while `guard.lock` is held, through the detached start (D63; [Starting the guard](#starting-the-guard)). |
| Attempt | Of the guard's start: one that the keeper made or tried and failed. A time the keeper found `guard.lock` held is not an attempt ([When the keeper starts it](#when-the-keeper-starts-it)). |

### What the keeper does with its files

**Every file below is [docs/state.md](state.md#the-keepers-files)'s**: its path, format, bytes,
share modes and access rules. This table says only what the keeper does with it.

| File | The keeper |
|---|---|
| `leases.json` | Reads it at every pass, without a lock, and never writes it ([docs/state.md](state.md#the-lease-state-file)). |
| `hosts.json`, the host registry | Reads it at every pass, without a lock, and never writes or creates it. It never takes `hosts.lock` ([Hosts](#hosts); [state.md, The host registry](state.md#the-host-registry)). |
| `board-holder.json` | The box's record. Writes it when it starts a holder on the box, reads it at every pass, and removes it when the holder it names is stopped, gone or untrusted. |
| `board-holder.<project>.json` | Another host's record, in the box's record's form ([state.md, Another host's holder](state.md#another-hosts-holder)). The keeper does with it, for that host, what it does with `board-holder.json` for the box, and finds it by its name ([The hosts of a pass](#the-hosts-of-a-pass)). |
| `board-holder.log` | Appends its log lines, for every host ([Log lines](#log-lines)). |
| `board-holder-ssh.log`, `board-holder-ssh.log.1` | The box's holder's log. Gives the box's holder the first as its stdout and stderr, and rotates it to the second ([The holder's log](#the-holders-log)). |
| `board-holder-ssh.<project>.log`, `board-holder-ssh.<project>.log.1` | Another host's holder's log, given and rotated as the box's is ([state.md, Another host's holder](state.md#another-hosts-holder)). |
| `board-holder.lock` | Holds it for the length of a pass ([Locks](#locks)). |
| `board-holder-keeper.lock` | A keeper holds it for its whole life ([Locks](#locks)). |
| `board-holder-launch.<hex>.json`, `.claim`, `.result.json` | The launch request, its claim and its result. Every start writes them, and they exist only while a holder is being started ([Starting a holder](#starting-a-holder)). `<hex>` is the token [docs/state.md](state.md#a-holders-launch-files) defines, new for each start and shared by its three files. |
| `board-holder-launcher-errors.log` | Written by the task's launcher, never by the keeper ([The task's launcher](#the-tasks-launcher)). |
| `guard.lock`, the guard's lock | Beside `hosts.json`, so in `%USERPROFILE%\.agentspace` and never moved by `AGENTSPACE_STATE_DIR` ([state.md, The guard's lock](state.md#the-guards-lock)). A keeper tries it, and closes it at once if it got it, only to learn whether a guard runs, before each attempt of the guard's start. It never holds it beyond that try, its try never creates it or its folder, and it never writes or removes it ([guard.lock: tried, never held](#guardlock-tried-never-held)). A one-pass run never opens it. |
| `guard.log`, the guard's log | Never opened, read, written or removed by the keeper. The guard writes it ([state.md, The guard's log](state.md#the-guards-log); [operator.md, guard](operator.md#guard)). |

**The keeper keeps no file for the guard's start.** The time of its last attempt lives in the
keeper's process alone ([When the keeper starts it](#when-the-keeper-starts-it)), and the guard
needs no launch files: `guard.lock` is its proof of life, as `provision.lock` is a provisioning
run's.

**Every JSON file the keeper writes is replaced atomically
([docs/state.md](state.md#the-keepers-files)).**

**One of each of these files serves every host:** `board-holder.log`, the launch files, the two
locks and the launcher's error log. A launch request names the log of the host it starts a holder
on, so the launch files need no host in their names (D55; architect, 2026-11-20, for v0.07-T01).
Only a record and a holder's log are per host.

### Hosts

**From v0.07 (#0047) every project has a host of its own, and the keeper holds one holder per host
(D30, D50).** Until v0.07 every lease was on the box, and a keeper with only the box
([Words used here](#words-used-here)) behaves exactly as v0.06's did, but for one field added to
some of its lines ([Log lines](#log-lines)) and two new ways its pass fails, each with a `failed`
line and exit 1: a host registry that cannot be read ([The order of a pass](#the-order-of-a-pass),
step 5), and a state folder that cannot be listed ([The hosts of a pass](#the-hosts-of-a-pass)).

- **The host registry, `hosts.json`, says which hosts exist and which are `ready`.** Its path,
  form and validity are [state.md, The host registry](state.md#the-host-registry)'s, under the
  `USERPROFILE` of the keeper's own environment. **`AGENTSPACE_STATE_DIR` does not move it**: it
  stays `%USERPROFILE%\.agentspace\hosts.json`, one registry for every state folder, and a keeper
  given another state folder still reads it there.
- **The keeper reads it read-only**: once per pass, whole, without a lock. It MUST NOT write it,
  create it, or take or create its lock, `hosts.lock`, so a pass never waits on the driver or a
  provisioning run, and they never wait on a pass. The registry is replaced atomically, so a
  read sees it as it was before or after any one write.
- **A missing `hosts.json` reads as the box's record alone**: project `swimcoach`, stack
  `agentbox`, state `ready` (D45; architect, 2026-11-20, for v0.07-T01). The keeper never writes
  that record; the first change by the driver or the operator does. So a laptop as v0.06 left it
  reads as the box alone, and every ready lease is on the box.
- **A registry that cannot be read fails the pass**, as a lease state that cannot be read does
  ([The order of a pass](#the-order-of-a-pass), step 5). It cannot be read when it is not a file
  (a folder, say) or cannot be opened, and when it is not valid by
  [state.md, The host registry](state.md#the-host-registry)'s rules: its bytes, its schema, and
  the rules beyond it, among them that a valid file holds exactly one record whose `stack` is
  `agentbox`, and that record's `project` is `swimcoach` (architect, 2026-11-20, for v0.07-T01).
  Only a missing file reads as the box alone; a file that is not valid never does. **Its `<error>`
  is `the host registry is not valid` for a file that is not valid by those rules, and
  `Windows error <n>.` for one that is not a file or cannot be opened or read**, the number
  alone, never the system's localized text, as a lock's and the state folder's are, so its line
  reads `Host registry could not be read: Windows error <n>.`; an unreadable registry MUST NOT be
  reported as not valid (architect, 2026-11-21, for v0.07-T03, on #0154's review).
- **The box is the host whose record's `stack` is `agentbox`**, and its project is that record's
  `project`. Every other record names another host by its `project`. A host is never named by its
  stack or its instance in anything the keeper writes.
- **Another host whose state is not `ready` (`creating` or `failed`) has no lease on it**
  ([Which host a lease is on](#which-host-a-lease-is-on)). The keeper never starts a holder on such
  a host, and stops one that runs there, which only a person's edit of `hosts.json` can bring
  about (D48; [Stopping a holder](#stopping-a-holder)). The box is
  different: every ready lease that is on no other host is on the box, whatever the box's own
  record's state, as D55's last row says.

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
    asleep or hibernated, because a host's sshd drops a dead session on the host's own time, so the
    grace is about time that passed on the host.
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
connection on the laptop holds them. The keeper supplies them: while a lease on a host is ready, it
keeps one holder running on that host through a ready lease's alias
([The board's forwards](#the-boards-forwards)).

- **One holder per host (D30, D50; [Q2](#open-questions), closed).** Only one connection on a host
  can bind the forwards (D12), so a host has at most one holder. Each host binds the board's ports
  on its own loopback, so holders on two hosts never contend, and the keeper runs them side by
  side. Each pass groups the ready leases by their host ([Hosts](#hosts)), and does for each host
  what v0.06 did for the one box.
- **On each host it holds through that host's ready lease that expires last**, ties going to the
  lowest lease ID, so the holder lives as long as any of the host's ready leases does and is moved
  as rarely as possible ([Which lease](#which-lease)). A holder already running through one of the
  host's ready leases is kept, even when another expires later.
- **A failure on one host never stops another host's holder**: each host has its own record, its
  own backoff and its own release grace ([A pass](#a-pass), [Watching](#watching)). A host's
  backoff or grace never delays a start, keep or stop on another host that was in the keeper's last
  pass. The one delay it can cause is to the first start on a host that was not
  ([The keeper's wait](#the-keepers-wait); [Q23](#open-questions)).
- **It stops a holder whose lease is no longer ready on its host**, at the first pass at or after the lease's
  `expires_at` or its release, so a holder outlives its lease by at most about one interval and a
  pass (D30). A holder whose host a person's edit or removal of `hosts.json` has left without a
  `ready` record is stopped the same way, by the first pass that reads the edit; that edit is the
  one way a host leaves `ready` (D48; [The order of a pass](#the-order-of-a-pass)). The next pass, not the same one, starts a
  holder on that host for another of its ready leases. A holder
  whose record was dropped is the exception: no pass stops it
  ([Trusting the record](#trusting-the-record)).
  - **From v0.08 a release ends the holder itself** (D59, as D63 changed it): `agentspace lease
    release` ends every ssh client of the lease's alias on the laptop, the holder included. The
    keeper then finds the holder gone, as if it had ended by itself, and waits the release grace
    before the next start on that host ([Release grace](#release-grace)). It stops a holder of a
    released lease only when the release did not end it. Expiry is unchanged: the keeper's stop is
    what ends a holder whose lease expired.
- **A keeper starts the guard (D56)**, at its first iteration and then once every 300 seconds,
  never while a guard runs, so that a host no live lease explains is stopped without anyone asking
  ([Starting the guard](#starting-the-guard)). A one-pass run does not.
- **It MUST NOT stop a process it did not start.** A recorded PID is trusted only when the
  process's start time and command line match the record ([Trusting the record](#trusting-the-record)).
- **The holder MUST outlive whatever started it**: the pass, the keeper, an agent's tool call and
  the scheduled task ([Starting a holder](#starting-a-holder)). So a keeper that dies leaves the
  board held, and the next keeper adopts the holder by its record.
- **It retries with backoff, never in a tight loop (D14)**, and waits out a host's release of a
  dead holder's forwards before it starts the next one there ([Watching](#watching)).
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
| `-GuardIntervalSeconds` | 1 to 86400 | `300` | `-Watch` | Hidden, for checks only: the least time between two attempts of the guard's start ([When the keeper starts it](#when-the-keeper-starts-it)). It is not in the usage text. D63 pins 300 seconds; the argument exists so a check can see a second attempt without waiting 5 minutes (architect, 2026-11-21, for v0.08-T01, pin P18). |

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
`-FastFailureSeconds`, `-GuardIntervalSeconds`), skipping any not given (B-45). For each, without `-Watch` it reports `is
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
| 0 | The pass ran, whatever each host's action, `failed` starts and stops included; or it was skipped because another pass held `board-holder.lock` | `-MaxPasses` passes ran; or another keeper holds `board-holder-keeper.lock` |
| 1 | The lease state or the host registry could not be read, the pass or any host's turn met an error it does not handle, or a line of the pass or of any host's turn could not be logged | `board-holder-keeper.lock` could not be taken for a reason other than another keeper holding it; or an error before the first pass |
| 2 | A usage error | A usage error |

**The architect ruled these on 2026-11-17 (D30).** A `failed` start or stop exits 0, because the
pass ran and logged what happened; the next pass tries again.

- **A keeper never exits for an error once its loop has started** ([Watching](#watching)). Only
  `-MaxPasses`, a usage error, the keeper lock, or the process being killed ends it.
- **The guard's start never changes an exit code**: an attempt that fails is logged and the keeper
  goes on, and what the guard itself does or how it exits is never seen by the keeper
  ([What a failed start changes](#what-a-failed-start-changes)).
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
  no running holder. **From v0.07 it ends only that host's turn**: its line carries the host's
  `host=` field when the host is not the box, the pass goes on to the next host, and the one-pass
  run exits 1 at its end ([The order of a pass](#the-order-of-a-pass); card #0144, "a failure on
  one host never stopping another's holder").

### Streams and environment

- **Every line the keeper logs is also written to stdout**, as it is logged
  ([Log lines](#log-lines)). Usage errors go to stderr only. Nothing else is written to either.
  Under the installed task, the keeper's stdout and stderr reach no one, so `board-holder.log` is
  the only record of what it did ([The task's launcher](#the-tasks-launcher)).
- **The keeper reads no stdin.**
- **`AGENTSPACE_STATE_DIR`**, when set and not empty, names the state folder; a check MUST set it
  to a throwaway folder ([Checking the keeper](#checking-the-keeper)). The keeper creates the state
  folder if it is missing.
- **`USERPROFILE`**, as the keeper's own environment has it, names the holder's working folder,
  the folder of the host registry as [state.md](state.md#the-host-registry) places it
  ([Hosts](#hosts)), and, with `AGENTSPACE_STATE_DIR` unset, the state folder. **When it is unset or empty, the keeper
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
- **The guard is not the holder, and gets some of the keeper's variables on purpose** (pin P4;
  architect, 2026-11-21, for v0.08-T01). The keeper passes the guard its home folder, its state and ssh folders,
  its ProxyCommand override and every `AGENTSPACE_*_CMD` seam it was given, over the user's logon
  environment, so a check's keeper starts a guard in the check's home and with the check's stand-ins
  ([The guard's environment](#the-guards-environment)). Nothing of this reaches a holder: the rule
  above binds every holder and its launcher, unchanged.

## A pass

**A pass looks at the leases and at each host's holder once, then takes at most one action per
host to make them agree**, and logs one line for each host it acts on. A one-pass run is one pass;
a keeper runs one per iteration. With only the box, a pass is v0.06's pass: one look, at most one
action, one line.

### The order of a pass

1. **Take `board-holder.lock`**, waiting up to 10 seconds ([Locks](#locks)). A pass that cannot
   take it logs `skipped` and ends, with nothing else looked at. One lock serves every host.
2. **Check the command matcher against its own cases** ([The holder command](#the-holder-command)).
   A matcher that fails ends the pass with a `failed` line and exit 1, trusting nothing, on any
   host.
3. **Rotate the holders' logs** that are due: the box's `board-holder-ssh.log`, then each other
   host's ([The holder's log](#the-holders-log)).
4. **Read the lease state.** If it cannot be read, log `failed lease=- alias=- pid=- reason=Lease
   state could not be read: <error>`, leave every holder and every record as they are, and end
   with exit 1. A missing state file is no leases, not an error.
5. **Read the host registry** ([Hosts](#hosts)). If it cannot be read, log `failed lease=- alias=-
   pid=- reason=Host registry could not be read: <error>`, leave every holder and every record as
   they are, and end with exit 1 (architect, 2026-11-20, for v0.07-T01). A missing registry is the
   box alone, not an error.
6. **Work out the ready leases, and the host each one is on**
   ([Which leases are ready](#which-leases-are-ready),
   [Which host a lease is on](#which-host-a-lease-is-on)).
7. **Work out the hosts of the pass** ([The hosts of a pass](#the-hosts-of-a-pass)): the box first,
   then the others in order.
8. **Take each host's turn, one host at a time, in that order.** A host's turn is v0.06's pass for
   that host alone, on that host's record and that host's leases:
   1. **Read the host's record and decide whether it names a trusted holder**
      ([Trusting the record](#trusting-the-record)). A record that does not is removed, and its
      process, if any, is left alone.
   2. **Act**, by the first row of the table below that applies, and log the turn's one line.
9. **Release `board-holder.lock`.**

| The host's holder | The host's leases | Action | Line |
|---|---|---|---|
| Trusted, and its lease is one of the host's ready leases under the recorded alias | Any | Keep it | `kept` |
| Trusted, and its lease is not one of the host's ready leases, or its alias changed | Any | Stop it, and start nothing on this host this pass | `stopped`, or `failed` |
| None trusted, in a keeper that tracks a holder on this host (card #0068, [Release grace](#release-grace)) | Any | Nothing | `lost`, naming the tracked holder |
| None trusted | None ready | Nothing | `idle` |
| None trusted, on a host in a keeper's hold-off | One or more ready | Nothing | None ([The keeper's wait](#the-keepers-wait)) |
| None trusted | One or more ready | Start one on this host through the chosen lease | `started`, or `failed` |

**A keeper's start of the guard is not a step of a pass** ([When the keeper starts it](#when-the-keeper-starts-it)).
It comes before step 1, or during a keeper's wait, never between step 1 and step 9, so it never
runs while the pass holds `board-holder.lock`, never delays a host's turn, and never changes the
pass time. A one-pass run never starts the guard.

**One host's turn never changes another's.** A turn reads and writes only its own host's record,
starts and stops only its own host's holder, and gives a holder only its own host's log.
- **A turn that fails** (a failed start or stop) logs its `failed` line, and the pass goes on to
  the next host. As in v0.06, that is not an error of the pass, and a one-pass run still exits 0.
- **A turn that meets an error the pass does not handle** ends that turn with its `failed` line,
  naming its host ([Log lines](#log-lines)), and the pass goes on to the next host. The one-pass
  run then exits 1 once the pass has ended (card #0144, "a failure on one host never stopping
  another's holder"). That covers a record that cannot be removed ([Exit codes](#exit-codes)): it
  ends its own host's turn, never another's.
- **Steps 1 to 7 are the whole pass's**, so a failure in them stops every host's turn, which leaves
  every holder and record as it is: it stops nothing.
- **A `failed lease=- alias=- pid=- reason=The pass stopped on an error: <error>` line with no
  `host=` reads the same whether it ended only the box's turn or the whole pass.** A reader tells
  them apart by the lines after it with the same pass time: when it ended only the box's turn, a
  line of each other host's turn that logs one follows it; when it ended the whole pass, none does.
  With no other host in the pass, the two end the same.

**The rows are taken in order, so a trusted holder in the record wins over the #0068 row** (A-09,
B-38; [Q6](#open-questions), closed). When a keeper tracks one holder on a host and that host's
record now names another that is trusted (one a one-pass run started, say), the pass keeps it
(`kept`) when its lease is one of the host's ready leases under its alias, and the keeper adopts it
with no grace, since the pass starts nothing; or it stops it (`stopped` or `failed`) when it is
not, and the grace follows that stop as after any other ([Release grace](#release-grace)).

**A host that a person's edit takes out of `ready` has its holder stopped**, a safeguard for a hand
edit. Nothing in agentspace moves a record out of `ready` or removes one: a provisioning run never
takes a `ready` host out of `ready` (D48; [state.md, Its states](state.md#its-states)). Only a
person's edit or removal of `hosts.json` can. When such an edit leaves a host's record `creating`
or `failed`, or removes the record or the file (a missing file reads as the box alone), while a
holder runs on that host, which is never the box, none of the ready leases is on that host any more
([Which host a lease is on](#which-host-a-lease-is-on)), so the holder's lease is not one of the
host's ready leases, and the next pass stops it under [Stopping a holder](#stopping-a-holder)'s
rules, with the release grace after it. A holder is never kept on a host that the registry does
not say is `ready`.

**The pass time is fixed when the pass begins**, before the lock is taken: `-Now` when given, else
the current UTC second. Every line of the pass carries it, every host's included, so a `started`
line can carry a time up to about 95 seconds before it is written for each start the pass made
before it, its own included, plus the pass's wait for `board-holder.lock` (up to 10 seconds)
([Timings](#timings-of-a-start)). A `skipped` line likewise carries the pass time, and is written
about 10 seconds later, when the lock wait ends.

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

### Which host a lease is on

**A ready lease is on the host D55's last row gives, exactly** (D55; architect, 2026-11-20, for
v0.07-T01), the same answer the ProxyCommand reaches for the lease's alias
([operator.md](operator.md#the-proxycommand)):
1. **Take the lease record's `project`.**
2. **Find the registry's record with that `project`**, compared exactly: both are stored
   normalized ([lease-api.md](lease-api.md#project)).
3. **If that record's `state` is `ready`, the lease is on that record's host**: the box when the
   record's `stack` is `agentbox`, else the host named by that project.
4. **Otherwise, with no such record or a record that is `creating` or `failed`, the lease is on the
   box.**

- **So a swimcoach lease is on the box**, since swimcoach's record is the box's (D43), and with a
  missing registry every ready lease is on the box, as in v0.06.
- **Step 4 puts a ready lease of another project on the box only after a person's edit**: one that
  takes its host's record out of `ready`, or removes it or the file, while the lease is still
  ready. Nothing in agentspace does that (D48), and a lease is made `ready` only on a host that is
  `ready`, so this is a hand-edit case. Its alias carries `HostKeyAlias agentspace-host.<project>`
  ([state.md](state.md#a-lease-alias-for-a-host-that-is-not-the-box)), so a holder through it that
  reaches the box fails ssh's strict key check and exits at once: a fast failure on the box, never
  a connection to the wrong machine. While such a lease is the box's latest-expiring one, every
  start on the box chooses it and fails, and the box's own leases get no holder until it expires or
  is released, or its host is `ready` again. A box holder that is already running through one of
  the box's leases is kept. Whether the box SHOULD leave such leases out was
  [Q22](#open-questions), closed as overcome by D48's ruling.
- **The lease's `ssh.host_alias` plays no part in the choice of host.** The keeper finds the host
  from the project, as the ProxyCommand does for a lease alias.

### The hosts of a pass

**A pass takes a turn for each of these hosts, each once:**
1. **The box, always**, so a pass with nothing to do anywhere logs one `idle` line, as v0.06 did.
2. **Each host that a ready lease is on.**
3. **Each other host whose record is in the state folder**: a file named
   `board-holder.<project>.json` whose `<project>` matches the normalized project's pattern
   `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$` ([lease-api.md](lease-api.md#project)). The name is
   compared without regard to case, and `<project>` is taken in lowercase. So a host whose record
   a person's edit has removed from the registry, or taken out of `ready`, still gets a turn while
   its holder's record is there, and that turn stops the holder the record names if it is trusted,
   or removes the record if not.
4. **In a keeper, each host on which it tracks a holder**, so a tracked holder whose record was
   removed gets its `lost` line ([Release grace](#release-grace)).

- **The box's turn comes first, then every other host's in the ordinal order of its project.**
- **A `board-holder.<project>.json` whose `<project>` is the box's project is not a record**: the
  box's record is `board-holder.json`, and the keeper never reads, writes or removes that file.
- **A state folder that cannot be listed ends the pass** with `failed lease=- alias=- pid=-
  reason=The pass stopped on an error: <error>` and exit 1, every holder and record left as they
  are. Its `<error>` is `The state folder could not be listed: Windows error <n>.`, the number
  alone, never the system's localized text, as a lock's is (architect, 2026-11-21, for
  v0.07-T03, on #0154's review).
- **Item 3 is how the keeper does what state.md gives it**: it removes a record left for a host it
  no longer holds through, and nothing else removes one
  ([state.md, Another host's holder](state.md#another-hosts-holder)). Each host's record and log
  are named by its project there, so a listing finds them without the registry (D55; architect,
  2026-11-20, for v0.07-T01).

### Which lease

**When it starts a holder on a host, the pass chooses, among that host's ready leases, the one with
the latest `expires_at`; among those that share it, the one whose lease ID is lowest in ordinal
order.** The holder goes through that lease's `ssh.host_alias`. The choice is stable: the same
leases and the same registry always give the same choice. A lease on one host is never chosen for
another.

### Trusting the record

**In each host's turn, the pass reads that host's record, and trusts the holder it names only when
all of these hold.** The rule is the same for the box's record and another host's:
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
running and untouched.** The removal has no log line of its own. In a keeper that tracks a holder
on that host, the turn logs `lost` for the tracked holder and starts nothing (card #0068,
[Release grace](#release-grace)). Otherwise, in a one-pass run or in a keeper that tracks none
there, the turn goes on as with no holder, logs `idle` or starts one if one of the host's leases is
ready, and that line is the only sign that a holder was lost. So a holder started with another `-SshExe` is orphaned,
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

**A trusted holder whose record's lease is one of its host's ready leases, by ID compared without
regard to ASCII case, and whose ready lease's `ssh.host_alias` equals the recorded alias exactly,
is kept.** (The ID
was "compared exactly" until 2026-11-19, when the architect ruled for E0.06-T01-S07 that a reader
keeps a record's `lease_id` as stored, which state.md's MAY lets be in any case.) The pass logs `kept lease=<id>
alias=<alias> pid=<pid> host=<project>` and changes nothing. A keeper that did not start that
holder adopts it ([Watching](#watching)).

### Stopping a holder

**A trusted holder whose lease is not one of its host's ready leases is stopped**: the lease is not
ready (released, expired, unsatisfiable, pending, or gone from the state file), or it is ready but
on another host ([Which host a lease is on](#which-host-a-lease-is-on)), or its alias is no longer
the recorded one. A lease that expires is seen at the first pass whose pass time is at or after its
`expires_at`, so the stop comes up to about one interval after expiry. A person's edit that takes
a host's record out of `ready` is seen the same way, at the first pass that reads it
([The order of a pass](#the-order-of-a-pass)).
- **A released lease's holder is normally gone before any pass sees the release** (D59, as D63
  changed it): the release ends it with the alias's other ssh clients, and the keeper treats it as
  a holder found gone ([Release grace](#release-grace)), with no `stopped` line. This rule applies
  to a released lease's holder in two other cases, and its stop, its line and the grace after it
  are then as for any other:
  - **The release did not end it**: a holder whose program is not `ssh.exe`, as a check's stub is,
    or one the release could not open or end.
  - **A pass came between the release's write and its end of the clients**: the release writes
    `released` before it ends the lease's connections
    ([driver.md, Ending a lease's connections](driver.md#ending-a-leases-connections)), so a pass
    can read the record already `released` while the holder still runs. That pass stops the holder
    and logs `stopped`,
    and the grace follows. The release then finds the holder gone.
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
- **A stop MUST NOT terminate the keeper's own process**, as the holder or anywhere in its tree
  (architect, 2026-11-19, for E0.06-T02-S03, a guard its implementer added).
- **Rulings for the rewrite's look and stop** (architect, 2026-11-19, for E0.06-T02-S03): a
  process that opens but whose exit state or creation time cannot be read is "cannot be opened";
  the same on the stop's own handle is "the stop met an error", and the record is kept; a holder
  that fails the check on the stop's handle while a new look still trusts it is the "for another
  reason" row, `<why>` being `it did not match the record when checked again on the handle opened
  to stop it`; a process list that cannot be read in the first round is an error and terminates
  nothing, and in a later round ends the stop with what it has; a terminated process is given exit
  code 1. A lock's `<error>` is `<file> could not be opened: Windows error <n>.`, the number alone,
  never the system's localized text; a keeper lock whose PID cannot be written is a lock that
  could not be taken; and a lock never creates a missing state folder.
- **The tree is terminated leaves first, the holder last.** The stop then waits up to 2 seconds for
  each process to exit, and walks the tree again for any child made meanwhile, for up to 5 rounds.
- **A terminated holder does not close its session on its host.** That session keeps the forwards
  until the host's sshd drops it, which is why a keeper waits the release grace before the next
  start on that host ([Release grace](#release-grace)).

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
stops a holder on a host, or tries to, MUST NOT start one on that host; the next pass does. It MAY
start one on another host in the same pass, in that host's own turn.

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

**With no trusted holder on a host and none of its leases ready, the host's turn logs `idle
lease=- alias=- pid=-`**, followed by ` host=<project>` for a host that is not the box
([Log lines](#log-lines)), and changes nothing. An idle line never names a holder. The one
exception is a keeper's pass while the keeper tracks a holder on that host, which logs `lost` for
that holder instead ([The order of a pass](#the-order-of-a-pass)). The box's turn comes in every
pass, so a pass that finds no record of another host, tracks no holder and has no ready lease
anywhere logs exactly the box's `idle lease=- alias=- pid=-`, as v0.06's did.

## Starting a holder

**What a start MUST achieve, whatever the mechanism:**
1. **The holder MUST outlive the pass and every process above it.** On Windows that means it MUST
   NOT be in the Job Object of the process that asked for it, or of any process above that: a job
   is closed when an agent's tool call or a scheduled task ends, and kills what is in it.
2. **Its stdin is `NUL`, and its stdout and stderr are one handle that appends to its host's
   holder log**, `board-holder-ssh.log` for the box and `board-holder-ssh.<project>.log` for another
   host ([The holder's log](#the-holders-log)), opened with the sharing that
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
   The holder's log is the log of the host the holder is started on, as a full path; the launch
   files themselves are named as for any start, with no host in their names, and one host's start
   ends, its files removed, before the next host's turn begins.
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
   holder exited at once; see board-holder-ssh.log.`, naming the host's own log, so
   `board-holder-ssh.<project>.log` for another host. A holder that exits a moment after this check
   is recorded, and found gone by a later look or pass. **A holder still running, by PID and
   creation time, whose command line cannot be read back has not exited** (architect, 2026-11-19,
   for E0.06-T02-S04): the start fails with `The holder started, but its command line could not be
   read back to check it, so it runs unrecorded.`, and the pass leaves it running, as item 7 of
   [Starting a holder](#starting-a-holder) asks. **Nor has one that cannot be opened**: the start
   fails with `The holder started, but could not be opened to check it, so it may be running
   unrecorded.`, and the pass leaves it alone. "Exited at once" is for a holder gone, or a running
   process that is not the one the result names.
   - **The rest of E0.06-T02-S04's rulings:** a request that cannot be written in step 1 fails
     with `The holder could not be started: The launch request could not be written: <error>`;
     the launcher exits 0 when it answered or rightly started nothing, 1 when it could not write
     the result, and 2 when its argument names no `board-holder-launch.<hex>.json`; and a holder
     the launcher created suspended whose start time cannot be read is terminated before it runs,
     the launcher answering `The holder's start time could not be read.`
8. **The pass removes the request, then the claim, then the result**, whatever happened. The
   request goes before its claim, so a launcher that claims the request after the claim is gone
   finds no request, and starts nothing. A pass killed during a start leaves its files behind, and
   nothing removes them later ([Q13](#open-questions)). **When the request cannot be removed, the
   claim MUST stay** (architect, 2026-11-19, for E0.06-T02-S04): the claim is what keeps a late
   launcher from claiming that request and starting a holder no pass records, so the pass removes
   the result alone, and the request and its claim are left for [Q13](#open-questions)'s cleanup.
   - **The launcher resumes the holder, then writes the result**, as step 4 reads (architect, the
     same day, for E0.06-T02-S04): a launcher killed between the two leaves a running holder that
     no pass records, which step 5's timeout reason already names; the other order would leave a
     suspended holder that passes the trust checks yet holds nothing.
   - **A pass that is killed after a launcher claimed its request breaks item 7.** The launcher
     still starts the holder and writes its result, and nothing records or reports that holder: no
     pass trusts it, none stops it, and while it holds the forwards every later start fails on
     `ExitOnForwardFailure`. The same holds for a pass killed after the result and before the
     record ([A pass that is killed](#a-pass-that-is-killed)).
9. **The pass writes the host's record** for the holder, and only then logs `started lease=<id>
   alias=<alias> pid=<pid> host=<project>`, so a `started` line always follows its record.

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
| A pass that starts holders on several hosts | One start after another, never two at once: up to about 95 seconds for each host it starts on, since one pass at a time serves every host (D55; architect, 2026-11-20, for v0.07-T01) |
| Polling of the result file | Every 100 ms |

**Nothing in a start is retried in a loop (D14).** A start that fails is retried by a later pass,
under a keeper's backoff for that host.

**Starts on two hosts never overlap**, so one set of launch files is in the state folder at a time,
and a slow start on one host delays the turns after it in the same pass, by at most its 95
seconds. Two hosts that both need a holder at logon get both in one pass. A one-pass run or a
keeper beside a pass that long finds `board-holder.lock` held and logs `skipped`, as in v0.06.

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
- **Every host's turn before the kill stands, and every host's turn after it never happened.** Each
  host's record is its own file, replaced on its own, so a kill in one host's turn leaves every
  other host's record whole.
- **A half-finished start can leave a holder running unrecorded**, as in step 8 of
  [The handshake](#the-handshake), and its launch files behind ([Q13](#open-questions)).
- **No line is logged for the killed pass.** When the keeper itself was killed and the holder its
  pass was stopping is already dead, the new keeper's first pass finds no trusted holder and
  starts one at once if a lease is ready, with no grace ([Q11](#open-questions)).

## Starting the guard

**A keeper starts the guard, `agentspace-operator guard`, which stops every host that no live
lease explains** (D56, D63). The guard is the operator's: what it reads, what it stops, the
horizon it sends, its exit codes and every line of `guard.log` are
[operator.md, guard](operator.md#guard)'s, and its log and lock are
[state.md, The guard's log](state.md#the-guards-log)'s and
[state.md, The guard's lock](state.md#the-guards-lock)'s. This section says when the keeper starts
it, how, with what environment, and what the keeper logs.

- **The keeper still never talks to AWS or a host itself** (D56;
  [Where it runs](#where-it-runs)). It starts a program, as it starts holders, and that program
  does the talking.
- **Why the keeper** is D56's: only the laptop knows that a lease ended, the keeper already reads
  the lease state file every 30 seconds, and its task already runs at every logon, so the guard
  needs no scheduled task of its own.
- **Everything in this section is new in v0.08.** A keeper that starts no guard is v0.07's, and
  every other section of this page binds a keeper that does, unchanged but where it says so.

### When the keeper starts it

**Only a keeper starts the guard; a one-pass run never does** (architect, 2026-11-21, for
v0.08-T01, pin P18). A one-pass
run never tries `guard.lock`, starts no guard and logs no line for one, so a person's or a check's
one-pass run beside the keeper never adds a guard run. D63 pins the start to "its first pass, then
at most once every 300 seconds"; this page reads "its" as a keeper's, since only a keeper lives
from one pass to the next and so can keep the spacing.

**The start is due at a keeper's first iteration, and after that once `-GuardIntervalSeconds`, 300
by default, have passed since the keeper's last attempt** (D63). The seconds count from the end of
the attempt, when its line has been written or dropped, and are measured on the clock every other
duration of the keeper is measured on ([Times](#times)).
- **An attempt is a start the keeper made, or tried to make and failed**, for any of the reasons
  in [The keeper's lines for it](#the-keepers-lines-for-it) (architect, 2026-11-21, for
  v0.08-T01, pin P18). So an attempt that fails is not tried again for 300 seconds, and a failure
  that lasts writes one line every 300 seconds, never one per pass (D14).
- **Finding `guard.lock` held is not an attempt** (architect, 2026-11-21, for v0.08-T01, pin
  P18). The start stays due, and the keeper tries again at the next point below at which it may.
  A guard that runs long, or that a person started, delays the keeper's start only while it holds
  the lock, and until that next point.

**Where the time of the last attempt is kept: in the keeper's own process, and nowhere else**
(architect, 2026-11-21, for v0.08-T01, pin P18). A keeper's passes all run in its one process, for
its whole life ([Watching](#watching)), so a time held in memory spans them, as each host's hold-off and count do
([The keeper's wait](#the-keepers-wait), its last paragraph). No file records it, and the keeper
writes nothing for the guard's start but its log line.
- **A new keeper starts with no last attempt, so its first iteration's start is due at once**,
  however recently the keeper before it made one. A keeper restarted by the task's trigger within
  a minute of a crash therefore starts a guard at its first iteration. `guard.lock` keeps the two
  guards from running at once, and the cost is one more guard run: one `describe-instances` call,
  and a horizon sent to each running host (D56, D57).
- **Why not a file:** a file would need its own path, bytes, access rules and a rule for a time
  that is in the future or unreadable, to save at most one guard run per keeper start. A keeper
  starts at logon and after a crash, rarely.

**When in a keeper's loop** ([Watching](#watching), step 2): at the first of these points at which
the start is due (architect, 2026-11-21, for v0.08-T01, pin P18):
1. **Just before each pass**, after the look and any wait after the look, and before the pass time
   is taken. So the first iteration's start comes before the keeper's first pass, and a keeper run
   with `-MaxPasses 1` makes it.
2. **During a wait, once: at the moment the start falls due, or at the wait's beginning when it is
   already due**, whether the wait comes after a look or after a pass. A start that then finds
   `guard.lock` held is next tried just before the next pass, never again in the same wait
   (architect, 2026-11-21, for v0.08-T01, pin P19). The keeper makes the
   attempt, or its try, then the wait runs on to the end it had: it ends neither sooner nor later
   for it, and its `grace` or `backoff` line, already written, is unchanged.

So:
- **The start never comes inside a pass.** It never runs while the keeper holds
  `board-holder.lock`, never delays a host's turn, and never changes what a pass logs. An attempt
  just before a pass delays that pass by the attempt's own time alone, which the pass time, taken
  after it, absorbs.
- **A keeper's next attempt comes no later than `-GuardIntervalSeconds` plus about one pass after
  its last**, whatever its hold-offs, when `guard.lock` is free each time the keeper tries it: a
  wait of up to `-BackoffMaxSeconds` or the grace never holds a due start back, since the start is
  made in the wait. That is the "up to 5 minutes until the next guard run" in D56's stated time,
  plus at most one pass (about 95 seconds for each host the pass starts a holder on,
  [Timings of a start](#timings-of-a-start)). A try in a wait that finds the lock held is the
  exception: the next try comes just before the next pass, which that wait can put off by up to
  its own length.
- **Two attempts by one keeper are never closer than `-GuardIntervalSeconds`.** Two keepers, one
  after the other, MAY start guards closer than that, as above.

### How the keeper starts it

**An attempt, in this order** (D63; pins P3 and P4; architect, 2026-11-21, for v0.08-T01):
1. **Try `guard.lock`, without waiting**, as [guard.lock: tried, never held](#guardlock-tried-never-held)
   says. **Held:** the keeper starts nothing, logs nothing, and the attempt does not count (above).
   **Free:** the keeper has closed its own handle on it before step 2, so the guard it starts can
   take it. **Could not be opened for another reason:** the attempt fails (below), and nothing is
   started.
2. **Find the guard's program** (below). One that cannot be found fails the attempt.
3. **Start it detached, with the argument `guard`**, through the detached start (below).
4. **Log one line**: `guard started pid=<pid>` when the start returned the process ID, or the
   `failed` line of [The keeper's lines for it](#the-keepers-lines-for-it) when any step failed.

**The program** (D63):
- **`AGENTSPACE_GUARD_CMD`, when it is set and not empty in the keeper's environment:** the full
  path of the program the keeper starts in place of the operator, so that a check can stand one in.
  A relative value is made full against the keeper's working folder before the start, since the
  guard's own working folder is the home folder, as the lease CLI makes `AGENTSPACE_PROVISION_CMD`
  full ([driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run)). It is the
  keeper's own variable, read by no other program as a program to start, and never
  `AGENTSPACE_OPERATOR_CMD`, which the conformance harness sets to the operator under test.
- **Otherwise, `agentspace-operator.exe` in the folder the keeper's own executable runs from**, as
  every program finds another ([operator.md, How each program finds the others](operator.md#how-each-program-finds-the-others)).
  For the installed keeper, run by the task, that folder is the install folder, `<install>`, and the
  program is D63's "the install folder's `agentspace-operator.exe`" (architect, 2026-11-21, for v0.08-T01, on
  pin P15: D63 names the install folder, and the keeper's own folder is meant, so that a keeper run
  from a build's output folder starts that build's operator and never the installed one).
- **A path that does not name an existing file is a program that cannot be found**, and the
  attempt fails. The program is looked for at every attempt, so an install that adds or replaces
  the operator is used from the next attempt on, with no keeper restart.
- **Its command line is the program, then exactly the one argument `guard`**: `"<program>" guard`.
  The keeper never passes `-Quiet` or any other argument (D63). **A `.cmd` or `.bat` program**, its
  extension compared without regard to case, is run through Windows' command interpreter as the
  lease CLI runs a provisioning run's: `<system folder>\cmd.exe /d /c "<program>" guard`, with
  `<system folder>` as [state.md, The alias file](state.md#the-alias-file) gives it, and a
  `SystemRoot` that is unset or empty makes the program one that cannot be found.
- **These lines give the arguments, not the command line's bytes**, as
  [driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run) says of a run's:
  the detached start quotes by its own rule. **What the program MUST receive is exactly the one
  argument `guard`**, and a `.cmd` or `.bat` whose path holds a space MUST still run and receive
  it. A check asserts that, never the line's quoting.

**The detached start: `agentspace-detach`, as the lease CLI starts a provisioning run** (D56, D63;
[driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run), "The detached
start"; [state.md, The provisioning run's start](state.md#the-provisioning-runs-start)). What it
MUST achieve, from outside:
- **The guard is outside the Job Object of the keeper and of every process above it**: the task's
  launcher, the task, and a check's shell. It outlives the keeper that started it, as a holder does
  ([Starting a holder](#starting-a-holder), item 1).
- **It starts in a new process group, with a hidden console window, and its working folder is the
  home folder**: the keeper's `USERPROFILE`, made full against the keeper's working folder. So it
  never holds a worktree or a checkout open, and the console program it starts, the AWS CLI, opens
  no window of its own.
- **It is given none of the keeper's standard handles and inherits no handle of the keeper's**, in
  particular never `board-holder-keeper.lock`, `board-holder.lock`, or a handle on `guard.lock`,
  which the keeper closed before the start (step 1). Its standard streams are its hidden
  console's, which nothing reads, and it MUST NOT read its standard input.
- **One call, which returns the guard's process ID or fails.** At most one guard is started per
  attempt, and a failed start is not retried within the attempt (D14); the next attempt is
  `-GuardIntervalSeconds` later.
- **The call is bounded at 10 seconds, as a holder's create call is**
  ([Timings of a start](#timings-of-a-start); architect, 2026-11-21, for v0.08-T01, pin P19). A
  call that has not
  returned by then is abandoned, and the attempt fails with the `<why>`
  `The detached start failed: The call did not return within 10 seconds.`
  ([The keeper's lines for it](#the-keepers-lines-for-it)). A guard such a call still makes finds
  or takes `guard.lock` by its own rule ([operator.md, guard](operator.md#guard)), so it never runs
  beside another, and the keeper never learns of it.
- **The keeper never waits for the guard**, never tracks, looks at or stops it, and never reads
  `guard.log` or the guard's exit code. Nothing the guard does changes anything in the keeper: no
  host's state, no hold-off, no record, no line but the attempt's own.
- **There are no launch files** for the guard, no request, claim or result: `guard.lock`, which
  the guard holds for its life, is its proof of life, as `provision.lock` is a provisioning run's.

### The guard's environment

**The keeper passes the guard the environment the lease CLI passes a provisioning run, and with it
every `AGENTSPACE_*_CMD` seam the keeper was given** (pin P4; architect, 2026-11-21, for v0.08-T01; D63). The
guard's environment is the user's logon environment, with these set exactly as this list says:
- **`USERPROFILE`**, the keeper's value, made full against the keeper's working folder
  ([state.md, The home folder](state.md#the-home-folder)). The guard finds `hosts.json`,
  `guard.lock` and `guard.log` through it ([state.md, The guard's files](state.md#the-guards-files)).
- **`AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR`**, each in the guard's environment if and only
  if the keeper has it set and not empty, each made full as state.md makes a relative folder full.
  The guard reads the lease state file in the state folder it is given
  ([state.md, What moves which files](state.md#what-moves-which-files)), so it judges the same
  leases the keeper holds through.
- **`AGENTSPACE_PROXY_COMMAND`**, if and only if the keeper has it set and not empty, exactly as
  the keeper has it. It is a command line, not a folder, and is never made full.
- **Every variable whose name starts `AGENTSPACE_` and ends `_CMD`, with at least one character
  between**, its name compared without regard to case, as Windows compares one, in the guard's
  environment if and only if the keeper has it set and not empty, exactly as the keeper has it
  (architect, 2026-11-21, for v0.08-T01, pin P18: passed as given, not made full, since a seam's
  value is the harness's and the harness gives full paths). Among them
  are the operator's test seam, `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD`
  ([operator.md, The test seam](operator.md#the-test-seam)), and `AGENTSPACE_GUARD_CMD` itself.
- **A value of any of those that the logon environment holds and the keeper does not have set and
  not empty is removed**, so the guard never takes a folder, a ProxyCommand line or a stand-in that
  the keeper which started it did not use, as a provisioning run never does. **The keeper removes
  the names it knows**: `USERPROFILE`'s companions above, and every `AGENTSPACE_*_CMD` the specs
  name, `AGENTSPACE_GUARD_CMD` among them, since it cannot list the logon environment's own
  variables. A logon environment that sets another `AGENTSPACE_..._CMD` passes it to the guard
  (architect, 2026-11-22, for v0.08-T03; removal by pattern would need a change to the detached
  start).
- **No other variable of the keeper's reaches the guard.** `AGENTSPACE_TIMEZONE` and
  `AGENTSPACE_DIAGNOSTIC` are among those that do not (pin P4; architect, 2026-11-21, for
  v0.08-T01: the guard writes UTC times and prints no backend detail,
  [operator.md, guard](operator.md#guard)).

**Why the guard's environment differs from the holder's:**
- **A check's home has to reach the guard.** A keeper under a check runs in a throwaway home with a
  throwaway lease state file. A guard started with the logon environment would read the user's
  real registry and the real AWS CLI, find none of the check's leases, and could stop a real host.
  So the guard gets the keeper's home, state folder and stand-ins, and a check also replaces the
  guard itself ([Checking the keeper](#checking-the-keeper)).
- **The holder MUST behave as the user's own ssh** ([Streams and environment](#streams-and-environment)),
  so it gets the logon environment alone. The guard is agentspace's own program, started for the
  keeper's home, so it gets that home.
- **Under the installed task nothing changes in practice**: the keeper runs with the logon
  environment, which sets none of these, so the guard gets the logon environment and the real home.

### The keeper's lines for it

**Each attempt writes one line to `board-holder.log`, and to stdout, as every keeper line is
written** ([Log lines](#log-lines)). They are the keeper's own lines
([A keeper's own lines](#a-keepers-own-lines)): `<time>` is the current second when the line is
written, in UTC, and neither line has a `host=` field, since the guard concerns every host. Their
text is pinned here byte for byte (D56: "Its log line for the start is keeper.md's"; the text is
the architect's, 2026-11-21, for v0.08-T01, pin P18):

```
<time> guard started pid=<pid>
<time> failed lease=- alias=- pid=- reason=The guard could not be started: <why>
```

- **`guard started pid=<pid>`** is written once the detached start has returned, with the process
  ID it returned, in decimal: the operator's, or for a `.cmd` or `.bat` program, the `cmd.exe`
  that runs it. It says that a guard process was created, not that the guard ran to the end or
  stopped anything; what the guard did is in `guard.log`. **A start that returned no process ID
  writes `pid=-`**, which only a keeper built for another platform or a check's stand-in for the
  detached start can give: on Windows the start always returns the ID WMI's
  `Win32_Process.Create` gives (architect, 2026-11-22, for v0.08-T03, on #0179).
- **The `failed` line** has the grammar and the reason rules of a keeper's `failed` line, with `-`
  in every field ([A keeper's own lines](#a-keepers-own-lines)). A check tells it from the keeper's
  other `failed` lines by its reason's start, `The guard could not be started:`.
- **No line is written when the keeper finds `guard.lock` held**, as the guard itself logs nothing
  when it finds it held (D63). With a guard running, the keeper's log is silent about the guard.

**`<why>`**, by the first step that failed. Each names no path, so a line never carries the
Windows user name (AGENTS.md, "A live run on Terry's real setup MUST mask what it reads before it
prints it"):

| What failed | `<why>` |
|---|---|
| `guard.lock` could not be opened, for a reason other than its being held | `guard.lock could not be opened: Windows error <n>.`, the number alone, never the system's localized text, as a lock's error is ([Stopping a holder](#stopping-a-holder), the rulings) |
| `AGENTSPACE_GUARD_CMD` names no existing file | `The program that AGENTSPACE_GUARD_CMD names does not exist.` |
| `AGENTSPACE_GUARD_CMD` unset or empty, and no `agentspace-operator.exe` beside the keeper | `The operator program is not installed beside this program.` |
| A `.cmd` or `.bat` program, with `SystemRoot` unset or empty | `Windows' command interpreter cannot be found, because SystemRoot is not set.` |
| The detached start failed | `The detached start failed: <error>`, where `<error>` is the detached start's own error, in agentspace's words, with a Windows error as its number alone and no path; for a call that reached its bound, `The call did not return within 10 seconds.` ([How the keeper starts it](#how-the-keeper-starts-it); architect, 2026-11-21, for v0.08-T01, pin P19) |
| Any other error, a panic included, that escapes the attempt | `The start stopped on an error: <error>` |

**Examples:**

```
2026-11-21T22:00:00Z guard started pid=31244
2026-11-21T22:05:04Z guard started pid=31876
2026-11-21T22:10:09Z failed lease=- alias=- pid=- reason=The guard could not be started: The operator program is not installed beside this program.
```

### What a failed start changes

**Nothing but its own line, and when the next attempt comes** (D63):
- **No host's state changes**: no tracked holder, proof, count of fast failures or hold-off, on any
  host. No record is written or removed, and no holder is started or stopped for it.
- **No pass changes.** The attempt is never inside a pass ([When the keeper starts it](#when-the-keeper-starts-it)),
  so a pass before or after it runs, and logs, exactly as it would have.
- **No wait changes**: a wait in which an attempt failed ends when it would have.
- **The keeper never exits for it**, and `-MaxPasses` counts passes alone, never attempts.
- **The next attempt is `-GuardIntervalSeconds` later**, as after any attempt.
- **A line that cannot be logged is dropped**, as any keeper's own line is
  ([Log lines](#log-lines)), and the attempt stands: a guard that started still runs, and the
  attempt still counts.
- **A guard that started and then failed is the guard's to log**, in `guard.log`
  ([operator.md, guard](operator.md#guard)). The keeper's line for it is `guard started`.

## Watching

**A keeper (`-Watch`) runs passes in one process, for its whole life.** It first takes
`board-holder-keeper.lock` ([Locks](#locks)), then loops.

**It keeps a state for each host, and one host's state never changes another's**: the holder it
tracks there, if any, and whether that holder is proven; the host's count of fast failures; and
the host's hold-off, if one is running. Every rule in this section about "the holder", "the count"
or a grace or backoff is about one host's, and applies to each host on its own (D50; the card's
"backoff and release grace per host"). **With only the box, there is one host's state, and the
loop is v0.06's step for step**: every look, pass, line and wait is the same, but for the `host=`
field of [Log lines](#log-lines) and, from v0.08, the guard's start and its lines (step 2;
[Starting the guard](#starting-the-guard)), which add lines and change no look, pass or wait.

1. **The look**, before the pass, at each host on which the keeper tracks a holder, the box first
   and then the others in the ordinal order of their projects. It checks that host's holder by
   PID, creation time and command line, against the keeper's `-SshExe` and the alias the holder
   was started or adopted with. The look reads the process only, never the record.
   - **Alive**, and tracked for at least 60 seconds (`-FastFailureSeconds`): the holder is proven,
     and the host's count of fast failures goes to 0.
   - **Alive, and younger:** nothing changes.
   - **Gone, no longer the same process, or the check itself fails:** the holder is no longer
     tracked. If it was never proven, that is a fast failure and the host's count goes up by one;
     if it was proven, the count is unchanged. The keeper sets the host's hold-off, counted from
     this look: the longer of the release grace and, for an unproven holder, the backoff for the
     new count, and at least `-IntervalSeconds`. It logs `grace seconds=<n>` or `backoff
     seconds=<n>` for it ([Release grace](#release-grace)); with the defaults, a proven holder
     gives `grace seconds=95`.
   - **When the look set a hold-off, the keeper waits before the pass**, as
     [The keeper's wait](#the-keepers-wait) says. With only the box, that waits out the box's
     hold-off, as v0.06's wait after the look did. Then it goes on to the pass.
2. **The guard's start, when it is due** (v0.08, D56; its place: architect, 2026-11-21, for
   v0.08-T01, pin P18): the
   keeper tries `guard.lock` and, when no guard holds it, starts the guard and logs `guard started
   pid=<pid>` or its `failed` line ([Starting the guard](#starting-the-guard)). It is due at the
   first iteration, so it comes before the keeper's first pass, and then once every
   `-GuardIntervalSeconds`. When it falls due during a wait instead, the keeper makes it there and
   the wait runs on to its end ([When the keeper starts it](#when-the-keeper-starts-it)). It
   changes no host's state, whatever it logs.
3. **The pass**, with the pass time taken now ([A pass](#a-pass)). It counts toward `-MaxPasses`
   whether or not it ran to the end. A host whose hold-off is still running gets its turn, but no
   holder is started on it ([The order of a pass](#the-order-of-a-pass)).
4. **What each host's turn did**, for tracking on that host:

| The host's turn | Then, on that host |
|---|---|
| `started` a holder | The keeper tracks it, unproven, from the end of this pass |
| `kept` a holder it does not already track there (by PID and creation time) | The keeper adopts it: tracks it from the end of this pass, proven, since its age is unknown, in place of any holder it tracked there before. The host's count and hold-off are unchanged, and no grace follows |
| `kept` the holder it tracks there | Nothing changes |
| `lost` the holder the keeper tracks there (card #0068) | No holder is tracked. If it was never proven, that is a fast failure and the host's count goes up by one ([Release grace](#release-grace)) |
| Decided to start, and failed (any `failed` line from [Starting a holder](#starting-a-holder)) | No holder is tracked; the host's count of fast failures goes up by one |
| Stopped a holder, or tried to (`stopped`, or a `failed` stop) | No holder is tracked; the host's count goes to 0 |
| Anything else (`idle`, no line on a host in hold-off, a turn that failed before deciding) | Nothing changes |

   A pass that logged `skipped`, or failed before any host's turn, changes no host's state.
5. **If `-MaxPasses` passes have run, exit 0.**
6. **The hold-offs the pass set**, each counted from the end of the pass, one per host whose turn
   calls for one, logged in the order of the turns:

| After the host's turn | The host's hold-off | Line |
|---|---|---|
| A failed start | The backoff for the host's count, at least `-IntervalSeconds` | `backoff seconds=<n>` |
| A stop, or a tried stop | The release grace, at least `-IntervalSeconds` | `grace seconds=<n>` |
| A turn that logged `lost` (card #0068) | The release grace, or the longer backoff if the holder was unproven, at least `-IntervalSeconds` | `grace seconds=<n>` or `backoff seconds=<n>` |
| Anything else | None set; a running one runs on | None |

7. **The wait**, as [The keeper's wait](#the-keepers-wait) says. With only the box, it is v0.06's:
   the box's hold-off when the pass set one, else `-IntervalSeconds`. A guard's start that falls
   due during it is made then, and the wait ends when it would have (step 2).

**A new hold-off on a host replaces any that is running there.** A hold-off ends once its seconds
have passed on the clock the keeper measures durations on ([Times](#times)); while it runs, no
pass of that keeper starts a holder on that host. It never stops a keep, a stop or a `lost` there,
and never touches another host.

**One iteration can wait twice**: once after the look, when the look found a tracked holder gone,
and again after the pass, when that pass then failed to start a holder there. The two waits are
sequential, each with its own line, so they add: with the defaults and only the box, a proven
holder's end gives `grace seconds=95`, then a failed start gives `backoff seconds=30` before the
next pass. The rule "the longer, never the sum" applies within one hold-off only
([Release grace](#release-grace)).

### The keeper's wait

**A keeper waits at two points of an iteration: after the look, only when the look set a hold-off,
and after the pass, always.** Each wait ends when the first of the keeper's hosts is due
(architect, 2026-11-20, for v0.07-T01):
- **The keeper's hosts** are the hosts of its last pass ([The hosts of a pass](#the-hosts-of-a-pass)),
  or the box alone before its first pass.
- **A host with a running hold-off is due when its hold-off ends.**
- **A host with none is due, after the pass, `-IntervalSeconds` after the wait began; and after the
  look, at once.** So the wait after a look is no wait at all while any of the keeper's hosts has no
  hold-off, and a host whose holder ended does not delay the pass that the others are due for.
- **The wait after the pass is never shorter than `-IntervalSeconds`**, whatever is due (D14).

**So, with the defaults:**
- **Only the box:** its hold-off, when one runs, is the whole wait, and the wait is v0.06's.
- **The box and another host, the other's holder stopped:** the other host has a 95-second grace;
  the box has none, so passes come every 30 seconds. The box's turn keeps, stops or starts as
  usual in each, and the other host's turn starts nothing until its grace has run, then starts at
  the first pass after it.
- **Every one of the keeper's hosts in a hold-off:** the keeper waits until the first of them ends,
  so a lease that became ready meanwhile on a host that was not in the last pass gets its first
  holder only then: at most the longest hold-off, `-BackoffMaxSeconds` or the grace, after it
  became ready, as a lease did in v0.06 while the box waited out a grace or a backoff
  ([Q23](#open-questions)). The box is always one of the keeper's hosts, so this happens only
  while the box is in a hold-off too.

**No error ends the loop.** Each of the look, the guard's start, the pass and the wait runs on its
own: an error that escapes one is logged as a `failed lease=- alias=- pid=- reason=<text>` line, if
the log can be written, and the loop goes on. For the guard's start, `<text>` is
`The guard could not be started: The start stopped on an error: <error>`, and the error counts as
an attempt ([The keeper's lines for it](#the-keepers-lines-for-it)). **Every iteration MUST wait at least `-IntervalSeconds`, on every
path, the error paths included**, so no failure can make a tight loop (D14). A wait whose line
cannot be logged still waits its full time.

**The keeper's state lives only in its process.** Each host's count of fast failures, tracked
holder, proof and hold-off start empty in a new keeper, and so does the time of its last attempt
of the guard's start, so a new keeper's first iteration starts the guard
([When the keeper starts it](#when-the-keeper-starts-it)). What a new keeper inherits is the
records, and its first pass adopts the holder each host's record names, if it is trusted.

### Fast failures and backoff

**A fast failure is a start that did not give a live holder, or a tracked holder found gone before
it was proven**, by a look, or by a pass that logged `lost` for it. A holder found gone that was
never proven is a fast failure however long ago it started, because it may have died at any time
since the look before. **A fast failure counts on its own host only**, and a host's backoff grows
with its own count alone: a host whose starts keep failing never lengthens another host's waits.

- **Only proof of life resets the count**: a look that finds the tracked holder alive at least 60
  seconds after tracking began. A stop, or a tried stop, also resets it. An idle pass does not.
  Each resets only its own host's count.
  **An adopted holder is under the same rule** (architect, 2026-11-19, for E0.06-T02-S05): it is
  proven from adoption, its count unchanged then, and a later look at least 60 seconds after
  tracking began that finds it alive resets the count, as for a holder the keeper started.
- **A panic in a look, a pass or a wait is an error the keeper does not exit for** (architect,
  2026-11-19, for E0.06-T02-S05): the keeper catches it, logs the matching reason of
  [Reasons](#reasons) with the panic's message as `<error>`, and goes on as for any other error
  there. With the pinned traits no look returns an error, so a panic is the only way those reasons
  arise; a keeper that died of one would break "never exits for an error once its loop has
  started". **A wait whose clock itself panics ends there**, its error logged, and the loop goes
  on to its next pass; `-MaxPasses` still bounds a run.
- **The keeper's program prints nothing of a panic but agentspace's own words, and never exits
  101** (architect, 2026-11-19, for E0.06-T02-S05 and E0.06-T03-S04). Its `main` installs a panic
  hook that writes nothing, since the runtime's default writes `thread 'main' panicked at ...` to
  stderr; a library MUST NOT set one. A panic that escapes to `main`, outside the loop's own
  catches (a one-pass run, or before the first pass), ends the program with exit 1 and the one
  stderr line `agentspace-keeper: The keeper stopped on an error: <panic message>`, as for any
  other error before the first pass ([Exit codes](#exit-codes)).
- **After the n-th fast failure in a row on a host, the backoff is `min(BackoffMaxSeconds,
  BackoffBaseSeconds * 2^(n-1))` seconds**, and the host's hold-off is that or `-IntervalSeconds`,
  whichever is longer. `-BackoffBaseSeconds` and `-BackoffMaxSeconds` are one pair for every host.
- **With the defaults, a start that keeps failing on a host waits** 30, 60, 120, 240, then 300
  seconds for every failure after, each logged `backoff seconds=<n>`, and each hold-off comes after
  the pass that failed. With only the box, each is the keeper's whole wait, as in v0.06.

### Release grace

**When a look finds a tracked holder gone, the keeper MUST NOT start a holder on that host until
`-ReleaseGraceSeconds` has passed, counted from that look (D30, E0.02-T04-S03).** A holder that
ends leaves its session on its host, which keeps the forwards until the host's sshd drops it. A
holder started there meanwhile fails on `ExitOnForwardFailure`, and leaves another session for
sshd to time out. **The grace is per host**: it is the host's hold-off, and another host's holders
are started, kept and stopped through it as usual ([Watching](#watching)). The forwards are bound
on each host's own loopback, so a session left on one host holds nothing on another.

**Where each grace counts from:**
- **A holder that a look finds gone:** from that look, which comes up to one interval and a pass
  after the holder ended. With only the box, the wait comes before that iteration's pass.
- **A holder the keeper stopped, or tried to:** from the end of the pass that stopped it. With only
  the box, the wait comes after that pass.
- **A tracked holder that a pass logged `lost` for (card #0068, below):** from the end of that
  pass. With only the box, the wait comes after that pass.

**A holder ended by a release is a holder a look finds gone, with its release grace** (D59, as D63
changed it). From v0.08, `agentspace lease release` ends every ssh client on the laptop whose
destination is the lease's alias, each with its tree, before it deletes the alias
([driver.md](driver.md), [lease-api.md](lease-api.md)). The keeper's holder, `ssh.exe -N -o
ExitOnForwardFailure=yes <alias>`, is one of them, so the release ends it, and the keeper does not.
- **A keeper that tracks it finds it gone at its next look**, and sets the host's hold-off from
  that look exactly as for any holder that ended by itself: the release grace, or the longer
  backoff when the holder was never proven, at least `-IntervalSeconds`, logged `grace seconds=<n>`
  or `backoff seconds=<n>`. With the defaults and a proven holder, `grace seconds=95`. A holder
  released less than `-FastFailureSeconds` after the keeper began to track it is an unproven one,
  and so counts a fast failure on its host, as any unproven holder found gone does.
- **When it ends between a look and its pass's trust check**, the pass logs `lost` for it and the
  grace follows that pass, as card #0068's rule says (below).
- **When a pass comes between the release's write of `released` and its end of the clients**, the
  pass reads the lease already released while the holder still runs, and stops it as any holder
  whose lease is not ready ([Stopping a holder](#stopping-a-holder)): a `stopped` line, then the
  grace after that pass. The release then finds the holder gone. The release writes the record
  before it ends the clients
  ([driver.md, Ending a lease's connections](driver.md#ending-a-leases-connections)), so this
  third outcome is rare and allowed.
- **A one-pass run, or a keeper that does not track it**, finds a record that names a process that
  is gone, removes it, and logs `idle` or starts a holder for another of the host's ready leases at
  once, with no grace, as for any record whose holder is gone
  ([Trusting the record](#trusting-the-record)).
- **No `stopped` line is logged for it**, but in the third outcome above, since the keeper stopped
  nothing. Its lines read as a holder that ended by itself, and say nothing of the release.
- **Why the grace still applies:** the release ends only the laptop's end of the connection. The
  Session Manager tunnel never passes the close on, so the host keeps the holder's session, and the
  forwards it bound, until its sshd drops it within about 90 seconds (D59), which is what the grace
  waits out. It costs what the keeper's own stop at its next pass would have cost: one grace on
  that host.
- **This changes nothing for an expired lease**: nothing ends a holder at expiry but the keeper's
  stop ([Stopping a holder](#stopping-a-holder)), and the grace follows that stop.
- **A check's stub holder is not ended by a release**, since its program is `cmd.exe`, not
  `ssh.exe`, so a conformance case of a release still sees the keeper's `stopped` line. The keeper's
  side of a release ending a real holder is shown live (E0.08-T06, below in
  [Checking the keeper](#checking-the-keeper)); what the keeper does with a holder found gone is
  the same whatever ended it, which the existing cases of a killed holder show.

> **Note, observed.** Before the grace, a killed holder left the board down for 2 minutes 49
> seconds, against about 90 seconds of sshd's bound; with it, 1 minute 56 seconds, with no failed
> holder between (E0.02-T04-S03's live run, D30). These are measurements of today's box, not
> requirements.

- **The default, 95 seconds, MUST be used when `-ReleaseGraceSeconds` is not given.** It is derived
  from today's box: sshd's `ClientAliveInterval` 15 times `ClientAliveCountMax` 5, plus 20 seconds
  for the probe already in flight (0.02.T1.1, D12). For another host the same rule gives its
  value: its sshd's keepalive interval times its count, plus a margin for one probe in flight.
- **One grace serves every host, the default included** ([Q19](#open-questions), closed). Every host is made from the one template and gets `box/system` from
  `sync` (D44, D46), whose sshd drop-in sets the box's keepalive settings, so the box's 95 seconds
  is every host's. `-ReleaseGraceSeconds` sets the grace for all of them at once. A host with other
  keepalive settings would need a grace of its own, which nothing in v0.07 makes.
- **It applies to every holder the keeper started or adopted, whether it died or the keeper stopped
  it** (the architect's ruling, 2026-11-17). The stop terminates the holder, which leaves its
  host's session holding the forwards, so the next holder there, for the next lease, waits the
  grace from the stop.
- **It does not apply to a host on which the keeper has had no holder**, such as one whose record
  was missing when the keeper started, or one whose grace has passed: the keeper starts its first
  holder there at once. The grace keeps no state after it ends.
- **It does not apply to a holder stopped because its record could not be written.** That stop
  runs milliseconds after the start, before ssh can have bound the forwards, so nothing is held on
  the host. That failure gets the backoff only.
- **With a fast failure, the hold-off is the longer of the grace and the backoff, never their
  sum**, and one line is logged: `backoff seconds=<n>` when the backoff is at least the grace, else
  `grace seconds=<n>`, with the seconds the hold-off has left. A grace set above
  `-BackoffMaxSeconds` therefore waits the grace.
- **Bounds.** Every hold-off, and every wait after a pass, is at least `-IntervalSeconds`. No
  hold-off or wait is longer than the greatest of the grace, `-BackoffMaxSeconds` and
  `-IntervalSeconds`. One holder's end gives one bounded hold-off on its host, and the pass after
  it starts a holder there if one of the host's leases is ready, so the grace cannot keep the board
  down for good.
- **With the defaults, a started holder that keeps dying unproven waits** 95 (`grace`), 95
  (`grace`), 120 (`backoff`), 240 (`backoff`), then 300 (`backoff`) seconds before each next start.

**Card #0068 (a holder that dies mid-pass skips the grace) is carried in here as a requirement.**
- **When the keeper tracks a holder on a host and its pass finds no trusted holder there** (the
  tracked one's process gone or no longer the same process, the host's record missing, or the
  record naming a process that is not trusted), **the pass MUST NOT start a holder on that host,
  and logs `lost`** for the tracked holder. A record that names another holder that is trusted is
  kept or stopped instead ([The order of a pass](#the-order-of-a-pass)). The keeper then stops
  tracking it, counts a fast failure on that host if it was never proven, and sets the host's
  hold-off after that pass: the release grace, or the longer backoff if the holder was unproven,
  and at least `-IntervalSeconds`, logged `grace seconds=<n>` or `backoff seconds=<n>` as for any
  other. The first pass after the hold-off starts the next holder there if one of the host's
  leases is ready.
- **The grace counts from the end of that pass**, as after a stop. So the next start comes one
  wait after the pass that found the holder gone, never in that pass.
- **What the pass needs from the keeper is the tracked holder's identity**: its PID, creation time,
  lease ID and alias, which the keeper holds in its own process from the look before. No file
  carries it. A one-pass run tracks nothing, so this rule never applies to one ([Locks](#locks)).
- **The host's turn logs exactly one action line, `lost lease=<id> alias=<alias> pid=<pid>
  host=<project>`**, naming the tracked holder and its host from the keeper's own identity of it (D35; A-09, B-38;
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

**With the defaults, as seen in `board-holder.log`.** Each row is about one host, and its lines
carry that host's `host=` field where [Log lines](#log-lines) gives one; with several hosts, each
pass also logs a line for every other host it acts on.

| Event | Lines |
|---|---|
| Logon with a lease ready and no record | `started`, then `kept` every 30 seconds or so |
| Logon with ready leases on the box and on another host, and no record | `started ... host=swimcoach`, then `started ... host=<project>` in the same pass; then a `kept` line for each in every pass |
| A person's hand edit of `hosts.json` takes another host's record out of `ready` while its holder runs (nothing in agentspace does, D48) | `stopped ... host=<project>`, then `grace seconds=95 host=<project>`; the box's lines go on as before, a pass every 30 seconds |
| A holder on another host keeps failing to start | Its `failed ... host=<project>` lines, with `backoff ... host=<project>` hold-offs growing to 300 seconds; the box's `kept` line every 30 seconds or so throughout |
| An upgrade from v0.06 with the box's holder running | `kept ... host=swimcoach` on the new keeper's first pass, with no new holder and no new login on the box ([Stopping and starting the keeper](#stopping-and-starting-the-keeper)) |
| Logon or restart with a running, recorded holder | `kept` on the first pass, with no new holder and no new login on the box |
| The lease expires | `stopped`, then `grace seconds=95`, then `idle` (or `started` for another ready lease) |
| The lease is released, from v0.08 (D59) | No `stopped` line: the release ended the holder. Within about 30 seconds `grace seconds=95`, from the look that found it gone, or `lost` and then `grace seconds=95` when it ended between a look and its pass; or, when a pass came between the release's write and its end of the clients, `stopped` and then `grace seconds=95`; then `idle` (or `started` for another ready lease) |
| The holder is killed | Within about 30 seconds, `grace seconds=95`; then `started` |
| The holder ends between a look and its pass, or its record is removed | `lost` for it, then `grace seconds=95`; then `started` if a lease is ready |
| A holder that exits at once (the ports still held) | `failed ... reason=The holder could not be started: The holder exited at once; ...`, then `backoff seconds=30` |
| Another connection holds the forwards | `failed ... reason=The holder could not be started: The holder exited at once; ...` after each start, with `backoff` waits growing to `backoff seconds=300` |
| A second keeper starts | One `another keeper is running pid=<pid>` line, from the second keeper, which exits |
| The keeper is killed | No line. The task starts a new keeper within about a minute, and its first pass logs `kept` for a holder that still runs, or `started` or `idle` |
| A keeper starts, from v0.08 | `guard started pid=<pid>` before its first pass's lines, then one about every 300 seconds, with no `host=` ([Starting the guard](#starting-the-guard)) |
| A guard is still running when the start falls due | No line; the start comes at the next point at which no guard holds `guard.lock` |
| The guard's program is missing, or `guard.lock` cannot be opened | `failed lease=- alias=- pid=- reason=The guard could not be started: ...` about every 300 seconds, and every host's lines as before |

**Passes are spaced by the wait plus the pass's own time**, so a keeper notices a holder's end up
to `-IntervalSeconds` after it, and the grace counts from that notice. A pass that starts holders
on several hosts takes the time of each start ([Timings of a start](#timings-of-a-start)).

## Locks

**The keeper uses two locks, each a file in the state folder held open by the OS with a sharing
mode that refuses a second taker.** The open handle is the lock; the file's contents are not. A
lock is released when its handle is closed, and Windows closes it when its process ends for any
reason, a kill included, so neither lock can outlive its holder. Neither file is ever removed.
Their paths, share modes and contents are [docs/state.md](state.md#the-keepers-files)'s. The CLI's
`leases.lock` is not the keeper's; it is [docs/state.md](state.md#concurrency)'s, and the keeper
never takes it.

**Both locks are per state folder, and serve every host**: there is no lock per host (D55;
architect, 2026-11-20, for v0.07-T01). A keeper with another `AGENTSPACE_STATE_DIR` (a check's,
say) runs beside the real one, and neither lock sees the other.
- **From v0.08, a keeper with another state folder under the user's real `USERPROFILE` MUST be
  given a stand-in `AGENTSPACE_GUARD_CMD`.** Its guard would read the real registry, find
  `guard.lock` beside it, and judge every host by that state folder's leases alone
  ([The guard's environment](#the-guards-environment)), so it could stop a host a real lease
  explains.

**The host registry's lock, `hosts.lock`, is not the keeper's either**
([state.md, The host registry](state.md#the-host-registry)). The keeper never takes, opens or
creates it, and reads `hosts.json` without it ([Hosts](#hosts)), so a provisioning run or a lease
command that holds it never delays a pass, and a pass never delays them.

**The guard's lock, `guard.lock`, is the guard's, and the keeper only tries it** (v0.08, D56, D63;
[guard.lock: tried, never held](#guardlock-tried-never-held)).

### board-holder.lock: one pass at a time

- **What holding it means:** a pass is running, in a one-pass run or in a keeper. So two passes
  never both start a holder on a host (one holder per host, D30, D50), and never act on one record
  at once.
- **Who takes it:** every pass, as its first step, before it looks at anything. It is created if
  missing, and held as [docs/state.md](state.md#the-keepers-files) says.
- **For how long:** the whole pass, every host's turn included: up to about 95 seconds for each
  host it starts a holder on ([Timings of a start](#timings-of-a-start)), and released when the
  pass ends, whatever happened.
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
acts on the same records; a one-pass run with another `-SshExe` drops the keeper's records and
orphans their holders, on every host ([Trusting the record](#trusting-the-record)).

**A one-pass run keeps no state, so it never waits a grace or a backoff, on any host.** One run
while a keeper waits out a grace on a host can start a holder there at once, which fails on
`ExitOnForwardFailure` and leaves another session on that host for sshd to time out. That is
allowed; anything that runs passes beside a keeper SHOULD expect it. The keeper then finds a record
it did not write: its next pass adopts the holder if it is trusted, or drops the record if not.

### guard.lock: tried, never held

**`guard.lock` is the guard's lock: a guard holds it for its whole run, so a held lock means a
guard is running** (D56, D63). Its path, its share mode and the guard's hold of it are
[state.md, The guard's lock](state.md#the-guards-lock)'s, and what a guard does when it finds it
held (exit 0 at once, logging nothing) is [operator.md, guard](operator.md#guard)'s.

- **Where:** beside `hosts.json`, so in `%USERPROFILE%\.agentspace` under the keeper's own
  `USERPROFILE`. **`AGENTSPACE_STATE_DIR` does not move it**, as it does not move the host
  registry ([Hosts](#hosts)), so a keeper given another state folder tries the same lock as the
  guard it starts, which finds it through the `USERPROFILE` the keeper gives it
  ([The guard's environment](#the-guards-environment)).
- **What the keeper does with it: tries it, and releases it at once if it got it, only to learn
  whether a guard runs** (pin P3; architect, 2026-11-21, for v0.08-T01), as the lease CLI tries `provision.lock`
  ([state.md, The provisioning run's lock](state.md#the-provisioning-runs-lock)). It does this once
  per attempt of the guard's start, just before the start, and never otherwise.
- **The try is one open, without waiting**: for reading and writing, sharing nothing, opening the
  file only if it exists, with a handle that is not inheritable.
  - **Refused because another process has it open** (a sharing or lock violation): it is held. The
    keeper starts no guard, logs nothing, and the start stays due
    ([When the keeper starts it](#when-the-keeper-starts-it)).
  - **The file, or its folder, does not exist:** no guard holds it. The try creates neither; the
    guard creates the file when it takes it.
  - **Opened:** no guard holds it. The keeper closes the handle at once, before it starts the
    guard, so the guard it starts can take it.
  - **Any other failure**, such as access denied, or a folder at its path, is not a free lock: the
    attempt fails with `guard.lock could not be opened: Windows error <n>.`, and no guard is
    started ([The keeper's lines for it](#the-keepers-lines-for-it)). The pass's holders go on
    ([What a failed start changes](#what-a-failed-start-changes)). Pin P3; architect, 2026-11-21,
    for v0.08-T01.
- **The keeper never holds it beyond the try**, never writes, creates or removes it, and never
  waits on it. A one-pass run never opens it.
- **A guard can take it between the keeper's try and the start**, one a person started, say. The
  guard the keeper then starts finds it held and exits at once, logging nothing, which is the
  guard's own rule; the attempt still counts, and its line is `guard started`.
- **It is not one of the keeper's two locks**: a pass never takes it, and holding
  `board-holder.lock` or `board-holder-keeper.lock` never touches it.

## Log lines

**The keeper appends its lines to `board-holder.log`**, one log for every host, and writes each
line to stdout as well.
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
  dropped one changes nothing. **A host's turn whose line was dropped** does the same for that
  host, its `failed` line carrying the host's `host=` field as below, and the pass goes on to the
  next host's turn, as after any error in a turn ([The order of a pass](#the-order-of-a-pass)).
- **A keeper's own line that is dropped ends nothing.** A wait whose line was dropped still waits
  its full time, a dropped `failed` line leaves the loop running, and a second keeper whose
  `another keeper is running` line was dropped still exits 0 ([Watching](#watching),
  [Locks](#locks)).
- **A dropped line is not written to stdout either, today.** A check MUST NOT depend on whether
  one reaches stdout.

### A pass's line

```
<time> <action> lease=<lease> alias=<alias> pid=<pid>[ host=<project>][ reason=<reason>]
```

- **`<time>`** is the pass time ([The order of a pass](#the-order-of-a-pass)).
- **`host=<project>` names the host a line is about, by its project** (the box's is `swimcoach`).
  It comes after `pid` and before `reason`, so every field v0.06 wrote keeps its place and its
  order, and a reader that matches a line up to `pid=<pid>` reads it as v0.06's. Where it is
  written is the dev lead's reading of P6 (the keeper log's lines name the host's project where
  they concern one host), accepted on condition that every existing keeper conformance check
  passes unchanged (architect, 2026-11-20, for v0.07-T01):
  - **On every line of another host's turn**: `started`, `kept`, `stopped`, `lost`, `idle`,
    `rotated` and `failed`, its `failed` lines with `lease=-` included, and on its `backoff` and
    `grace` lines ([A keeper's own lines](#a-keepers-own-lines)).
  - **On the box's lines that name a holder**, as `host=swimcoach`: `started`, `kept`, `stopped`,
    `lost`, and `failed` for a start or a stop. So every line that starts, keeps or stops a holder
    names its host's project, the box's included.
  - **Never on the box's other lines** (`idle`, `rotated`, `grace`, `backoff`, and `failed` for an
    error of the box's turn), **nor on a line of the whole pass** (`skipped`, and `failed` for the
    matcher, the lease state, the registry or the state folder's listing). Those read exactly as
    v0.06's.
  - **Why there:** v0.06's conformance checks match the box's `idle`, `rotated`, `grace` and
    `backoff` lines and the `skipped` line to the end of the line, and match no line that names a
    holder to its end, so this placement keeps every one of them passing.
  - **Where this page writes a line's form without `host=`**, such as a `failed` line in
    [Starting a holder](#starting-a-holder), the field is added by these rules.
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
- **A pass writes one action line for each host whose turn logs one**, in the order of the turns
  ([The hosts of a pass](#the-hosts-of-a-pass)), the box's first. Before them it MAY write one
  `rotated` line for each host whose log it rotated, the box's first. With only the box, that is
  v0.06's one action line and at most one `rotated` line. A host in a keeper's hold-off whose turn
  would start a holder logs no line ([The order of a pass](#the-order-of-a-pass)).

| Action | `lease` | `alias` | `pid` | `host` | `reason` |
|---|---|---|---|---|---|
| `started` | The chosen lease | Its alias | The holder's | The host's | None |
| `kept` | The record's | The record's | The record's | The host's | None |
| `stopped` | The record's | The record's | The record's | The host's | None |
| `lost` | The tracked holder's, as the keeper holds it | The same | The same | The host's | None |
| `idle` | `-` | `-` | `-` | Another host's only | None |
| `skipped` | `-` | `-` | `-` | None | Always `locked` |
| `rotated` | `-` | `-` | `-` | Another host's only | None |
| `failed`, the lease state or the registry unreadable, or another error of the whole pass | `-` | `-` | `-` | None | Below |
| `failed`, an error of one host's turn | `-` | `-` | `-` | Another host's only | Below |
| `failed`, a start | The chosen lease | Its alias | `-`, or the holder's when it started but could not be recorded | The host's | Below |
| `failed`, a stop | The record's | The record's | The record's | The host's | Below |

**Examples**, the first six on the box and the last three on another host:

```
2026-11-18T05:07:00Z started lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112 host=swimcoach
2026-11-18T05:07:30Z kept lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112 host=swimcoach
2026-11-18T06:00:01Z stopped lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112 host=swimcoach
2026-11-18T06:01:36Z idle lease=- alias=- pid=-
2026-11-18T06:02:06Z skipped lease=- alias=- pid=- reason=locked
2026-11-18T07:12:30Z lost lease=7d2e9a40-3c5b-4e1f-8a6d-1b9c0f2e4a77 alias=agentspace-swimcoach-7d2e9a40 pid=25160 host=swimcoach
2026-11-18T07:13:00Z started lease=3a9c5e21-6b7d-4c8e-9f01-2d3e4f5a6b7c alias=agentspace-agentspace-3a9c5e21 pid=25344 host=agentspace
2026-11-18T07:20:00Z failed lease=3a9c5e21-6b7d-4c8e-9f01-2d3e4f5a6b7c alias=agentspace-agentspace-3a9c5e21 pid=- host=agentspace reason=The holder could not be started: The holder exited at once; see board-holder-ssh.agentspace.log.
2026-11-18T07:30:00Z idle lease=- alias=- pid=- host=agentspace
```

### A keeper's own lines

```
<time> backoff seconds=<n>[ host=<project>]
<time> grace seconds=<n>[ host=<project>]
<time> another keeper is running pid=<pid>
<time> guard started pid=<pid>
<time> failed lease=- alias=- pid=- reason=<reason>
```

- **`guard started pid=<pid>`** (v0.08) is written when the keeper has started the guard. It never
  carries `host=`. It and the `failed` line of a guard's start are
  [Starting the guard](#starting-the-guard)'s
  ([The keeper's lines for it](#the-keepers-lines-for-it)).
- **`<time>` is the current second when the line is written**, not a pass time.
- **`seconds=<n>`** is the whole seconds the host's hold-off has left when the line is written, the
  `-IntervalSeconds` floor included. With only the box, it is the whole seconds the keeper waits
  after the line, as in v0.06.
- **`host=<project>`** is written on a `backoff` or `grace` line for a host that is not the box,
  and never for the box, whose lines read exactly as v0.06's (architect, 2026-11-20, for
  v0.07-T01; [A pass's line](#a-passs-line)). The keeper's
  `another keeper is running` and `failed` lines concern no host and never carry it.
- **`pid=<pid>`** is, on an `another keeper is running` line, the running keeper's PID as its lock
  file gives it, or `-` ([Locks](#locks)); on a `guard started` line, the process ID the detached
  start returned, in decimal, or `-` only for a start that returned none, which only a keeper
  built for another platform or a check's stand-in can give ([The keeper's lines for
  it](#the-keepers-lines-for-it); architect, 2026-11-22, for v0.08-T03).
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
| The host registry cannot be read | `Host registry could not be read: <error>`, where `<error>` says why: `the host registry is not valid` for a file [state.md](state.md#the-host-registry) calls not valid, one without exactly one `agentbox` record whose project is `swimcoach` included, and `Windows error <n>.`, the number alone, for one that is not a file or cannot be opened or read (architect, 2026-11-21, for v0.07-T03, on #0154's review) ([Hosts](#hosts)) |
| An error a pass does not handle, the matcher's self-check included | `The pass stopped on an error: <error>`; for the matcher, `<error>` is `The holder command matcher failed its own checks: <case>; <case>` |
| `board-holder.lock` is held | `locked` |
| A start | `The holder could not be started: <reason>`, where `<reason>` is from [Before the start](#before-the-start) or [The handshake](#the-handshake) |
| A record that cannot be written | The two texts at the end of [The handshake](#the-handshake) |
| A stop | The texts in [Stopping a holder](#stopping-a-holder) |
| A keeper's look | `The look at the holder stopped on an error: <error>` |
| A keeper's pass, escaping the pass's own handling | `The pass stopped on an error: <error>` |
| A keeper's wait | `The wait stopped on an error: <error>` |
| A keeper's start of the guard (v0.08) | `The guard could not be started: <why>`, where `<why>` is from [The keeper's lines for it](#the-keepers-lines-for-it). Unlike the texts above, this one is pinned: a keeper MUST write it byte for byte, and a check MAY match all of it, but for an `<error>` or `<n>` in it |
| The keeper lock | `The keeper lock could not be taken: <error>` |

### The holder's log

**`board-holder-ssh.log` holds the box's holder's stdout and stderr, appended**; its bytes are
[docs/state.md](state.md#the-keepers-files)'s. **Another host's holder writes
`board-holder-ssh.<project>.log`** instead, in the same bytes
([state.md, Another host's holder](state.md#another-hosts-holder)). Nothing parses either, and
nothing but a holder on that host writes to it. Every rule below holds for each host's log on its
own.

**Rotation:**
- **At the start of every pass that holds `board-holder.lock`**, before the lease state is read,
  a `board-holder-ssh.log` larger than 1 MiB (1,048,576 bytes) is renamed to
  `board-holder-ssh.log.1`, replacing any older one, and the pass logs
  `<time> rotated lease=- alias=- pid=-`.
- **Then each other host's log is rotated the same way**: every `board-holder-ssh.<project>.log`
  in the state folder whose `<project>` is a normalized project and not the box's, matched as
  [The hosts of a pass](#the-hosts-of-a-pass) matches a record's name, in the ordinal order of the
  projects. One larger than 1 MiB is renamed to `board-holder-ssh.<project>.log.1`, and the pass
  logs `<time> rotated lease=- alias=- pid=- host=<project>`. The pass finds these logs by listing
  the state folder, since the registry is not read yet and each log is named by its project
  (D55; [state.md, Another host's holder](state.md#another-hosts-holder)); a listing that fails
  rotates no other host's log, and is not logged.
- **A failure to rotate, or to log the rotation, MUST NOT stop the pass**, and is not logged.
- **A holder that has the file open keeps writing to it under its new name**, since it opened the
  file shared for deleting. The next holder on that host opens a new log.
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
  - **`<registration time>` is the boundary exactly as the definition holds it: UTC with a `Z`,
    to the whole second**, `YYYY-MM-DDTHH:MM:SSZ`, as [The task's fields](#the-tasks-fields)'
    Trigger 2 row gives it. (Corrected by the architect the same evening, for E0.06-T02-S06: this
    line first said local time with its offset, against that row. The row wins, since a UTC
    boundary does not move when the laptop's zone does.) A real install's read-back prints the
    boundary as Task Scheduler returns it.
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
   **Status (E0.06-T06-S01, 2026-11-20): that change-over is done and the prototype was removed
   from the repository**, so the `pwsh.exe` alternative above names a keeper that can no longer be
   started. It is kept here as the record of the change-over. The rule that binds is the lock and
   the PID, and `agentspace-keeper.exe` as the program.
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
`agentspace-keeper-install: `. Three of them, ruled by the architect on 2026-11-19 for
E0.06-T02-S06: `USERPROFILE` unset or empty, after the usage checks and in every mode `-DryRun`
included, is `agentspace-keeper-install: USERPROFILE is not set, so there is no home folder.`; a
keeper lock file that exists but cannot be opened for any reason but a sharing or lock violation
is `agentspace-keeper-install: The keeper lock <path> could not be checked: <error>`; and an
`-Uninstall` whose unregistration fails stops there, exit 1, and does not remove the prototype's
launcher, which a task that is still registered may yet run. The prototype prints PowerShell's own error record then, and its
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

**Every host's holder keeps running throughout**, unsupervised. Each holds the board on its host
until it dies or a keeper stops it.

**A guard the keeper started runs on to its end**, since it is outside the keeper's job
([How the keeper starts it](#how-the-keeper-starts-it)). **While no keeper runs, no guard is
started**, so no host is stopped for want of a live lease until a keeper runs again, and each host
bounds itself by its lease horizon and its idle check alone (D57). A guard run by hand
(`agentspace-operator guard`) still works then ([operator.md, guard](operator.md#guard)).

**An orphaned keeper**, left by step 1 without step 2, keeps the board held, and the task's next
firing starts a second keeper that logs one `another keeper is running pid=<pid>` line and exits. That
is one line a minute, about 90 KB of `board-holder.log` a day, until the orphan is stopped. That is
accepted (D30): End is a step taken on purpose.

**To start it again:** start the task (`Start-ScheduledTask`), or wait for the repeating trigger,
or log on. The new keeper's first pass adopts each host's recorded holder (`kept`), with no new
holder and no new login on that host, when the holder is still trusted.

**To load new keeper code**, the keeper MUST be stopped and started again: a running keeper read
its program once, when it started, and a merge does not reach it. A re-registration (a real
install) is refused while a keeper runs, so it, too, comes after the keeper is stopped.

**A v0.07 keeper started while v0.06's box holder runs MUST adopt that holder, with no restart**
(the card's done-when; D55, "The box's own files"). Nothing about the box's holder moved:
- **The box's record is still `board-holder.json`, in v0.06's form, and its log still
  `board-holder-ssh.log`.** A v0.07 keeper reads that record as the box's, with v0.06's trust
  rules, so a holder v0.06 started passes them when both keepers run with the same `-SshExe`.
- **The holder's lease is a swimcoach lease, on the box**: with the registry missing, or with
  swimcoach's record the box's, as D43 and D45 leave it
  ([Which host a lease is on](#which-host-a-lease-is-on)).
- **So the first pass logs `kept ... host=swimcoach`** and adopts it, with no new holder and no new
  login on the box, and writes nothing to the record. An upgrade MUST NOT move, rename or rewrite a
  file of the box's holder.

**Before a re-install, the task SHOULD be disabled as well, or the install run within the minute.**
A keeper started by the repeating trigger between the installer's check and its registration
keeps running the old program, and the new task's keeper finds the keeper lock held and exits. A real install enables the task again
([Q14](#open-questions)).

## Checking the keeper

**A conformance check of the keeper drives it only as a separate process (D34)**, through what this
page documents:
- **It MUST set `AGENTSPACE_STATE_DIR` to a throwaway folder**, writes its own `leases.json` there
  ([docs/state.md](state.md#the-lease-state-file)), and reads `board-holder.log`, the records and
  the processes the keeper starts.
- **It MUST set `USERPROFILE` to a throwaway folder as well**, since the host registry is found
  through it ([Hosts](#hosts)), and either writes its own `hosts.json` where
  [state.md](state.md#the-host-registry) places it under that folder, or writes none to have the box
  alone. A check that left `USERPROFILE` as it is would read the user's real registry. The
  conformance harness already moves it (conformance.md, The harness).
- **It gives each host its own leases by their `project`**, and a lease of a project with no
  `ready` record is on the box, so a check that writes no registry needs no change from v0.06.
- **It stubs ssh with `-SshExe`**, naming a `.cmd` that prints a line and stays alive until it is
  stopped, connecting nowhere and touching no file. A stub receives the holder's arguments on its
  own command line, which is how a check finds it, and does not receive the keeper's environment
  ([Streams and environment](#streams-and-environment)).
- **It controls time with `-Now`** for a one-pass run, and with `-IntervalSeconds`,
  `-BackoffBaseSeconds`, `-BackoffMaxSeconds`, `-ReleaseGraceSeconds`, `-MaxPasses` and the hidden
  `-FastFailureSeconds` and `-GuardIntervalSeconds` for a keeper.
- **From v0.08, every check that runs a keeper (`-Watch`) MUST set `AGENTSPACE_GUARD_CMD` to a
  stand-in**, which touches nothing outside the check's home, and SHOULD set the operator's test
  seam (`AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD`, `AGENTSPACE_SCP_CMD`) to stand-ins as well. The
  rule is [conformance.md, The harness](conformance.md#the-harness)'s (pin P15; architect,
  2026-11-21, for v0.08-T01). A keeper with no `AGENTSPACE_GUARD_CMD` starts the operator beside it
  as a real guard ([Starting the guard](#starting-the-guard)), which could stop a real host. A
  check MUST NOT run a real guard. A one-pass run starts no guard; its check SHOULD set the
  stand-in all the same, so a build that got this wrong starts nothing real (G12).
- **It MAY watch the state folder for the launch files** while a holder is started, and hold them
  to [docs/state.md](state.md#a-holders-launch-files) and [The handshake](#the-handshake): that
  each start writes a request, a claim and a result under one `<hex>`, and that the pass removes
  all three. They are part of the contract ([Starting a holder](#starting-a-holder)).
- **It MAY see the holder's environment through its stub**, which can write it to
  `board-holder-ssh.log`, to show that a variable set only for the keeper does not reach the
  holder ([Streams and environment](#streams-and-environment)). A stub MUST NOT print the
  environment's values to anything but that log, and a check reads the variables it names only
  (AGENTS.md, "Terry's real files in a review").
- **It MAY tell the box's holder from another host's by the alias on the stub's command line**, and
  MAY make one host's stub exit at once and another's stay alive by that alias, so one stub stands
  in for every host.
- **It MUST NOT call the hidden `-Launch`**, and MUST NOT run the installer for real, touch the real
  task, or touch the user's real state folder, real `hosts.json` or `~/.ssh`.
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

### Conformance cases for a host per project

**Each behavior below is one that a stand-in can show, and MUST be a conformance case of the
keeper, written by E0.07-T04-S03** (D53, D55). Each runs with a throwaway `USERPROFILE` and state
folder, a stub `-SshExe`, and a registry the case writes, or none. `<p>` is a project the case
names, such as `agentspace`, with a `ready` record of its own. No case needs a real host: a
holder's connection is a stub's.

| # | The case | It shows |
|---|---|---|
| H1 | Every v0.06 case of the keeper, run twice: once with no registry, and once with a registry that holds the box's record alone, `{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"}]}` | Each time, the box alone behaves as v0.06's keeper did, line for line, but for ` host=swimcoach` on the box's lines that name a holder ([Log lines](#log-lines)), and, from v0.08, but for the guard's start lines of a keeper, run with a stand-in guard ([Conformance cases for the guard's start](#conformance-cases-for-the-guards-start); architect, 2026-11-21, for v0.08-T01, pin P18) |
| H2 | No registry; a ready lease of any project; one-pass run | The holder is the box's: `board-holder.json` is written, the request's `log` is `board-holder-ssh.log`, and no `board-holder.<project>.json` appears ([Hosts](#hosts)) |
| H3 | Registry with the box and `<p>`, both `ready`; a ready swimcoach lease and a ready `<p>` lease; one-pass run | One pass starts two holders, the box's first: two `started` lines, `host=swimcoach` then `host=<p>`; records `board-holder.json` and `board-holder.<p>.json`; each launch request's `log` is its host's; and the two starts' launch files never exist at the same time: the second start's request appears only after the first start's three files are gone ([The order of a pass](#the-order-of-a-pass), [Timings of a start](#timings-of-a-start)) |
| H4 | Two ready leases on each of two hosts, with differing and with equal `expires_at` | Each host's holder goes through that host's latest-expiring lease, ties to the lowest ID, and never through the other host's ([Which lease](#which-lease)) |
| H5 | A ready `<p>` lease, `<p>`'s record `creating`, then `failed`, then absent | The lease is on the box each time: the box's holder goes through it, and no `board-holder.<p>.json` is written ([Which host a lease is on](#which-host-a-lease-is-on)) |
| H6 | A hand-edit safeguard: `<p>`'s holder running and recorded; its record in the registry rewritten to `creating` by the case, as a person's edit would (nothing in agentspace does, D48) | The next pass stops it: `stopped ... host=<p>`, `board-holder.<p>.json` removed, the stub's PID gone; a box holder running beside it is kept in the same pass ([Stopping a holder](#stopping-a-holder)) |
| H7 | A running stub recorded in `board-holder.<q>.json` for a project `<q>` with no record in the registry | The pass gives `<q>` a turn and stops that holder ([The hosts of a pass](#the-hosts-of-a-pass)) |
| H8 | A `board-holder.swimcoach.json` naming a running stub | The keeper never reads, removes or stops it, and the box's turn uses `board-holder.json` alone |
| H9 | Keeper; `<p>`'s stub exits at once, the box's stays alive | `<p>`'s fast failures, each either a `failed ... host=<p>` line or a `started ... host=<p>` line whose holder a later look finds gone (step 7 of [The handshake](#the-handshake): a holder that exits a moment after the start's check is recorded; architect, 2026-11-21, for v0.07-T04), with `backoff seconds=<n> host=<p>` growing per `<p>`'s count, while the box's holder is kept in a pass every `-IntervalSeconds`; no box line waits on `<p>`'s backoff ([Watching](#watching)) |
| H10 | Keeper; `<p>`'s holder stopped (its lease released) while the box's lease stays ready | `grace seconds=<n> host=<p>`; passes go on every `-IntervalSeconds` with the box's `kept`; no start on `<p>` until its grace has run, then a start at the first pass after it if a `<p>` lease is ready ([The keeper's wait](#the-keepers-wait)) |
| H11 | Keeper tracking holders on two hosts; one host's record removed between look and pass, as #0068's check does | `lost ... host=<that host>` and its grace, with the other host's holder kept in the same pass ([Release grace](#release-grace)) |
| H12 | Each in turn: a registry that is not JSON, one that fails its schema, one in which two records share a project, one with no `agentbox` record, one whose `agentbox` record's project is not `swimcoach`, and a folder in its place; holders running on two hosts | One `failed lease=- alias=- pid=- reason=Host registry could not be read:` line, exit 1, both holders running and both records unchanged ([Hosts](#hosts)) |
| H13 | A registry the case holds `hosts.lock` on, then releases | Passes run while the lock is held; the registry's bytes and time stamps and `hosts.lock` are never written, and a missing `hosts.lock` is never created ([Locks](#locks)) |
| H14 | `board-holder-ssh.<p>.log` over 1 MiB | It is renamed to `.log.1`, and `rotated lease=- alias=- pid=- host=<p>` is logged after any box `rotated` line ([The holder's log](#the-holders-log)) |
| H15 | Upgrade: a running stub recorded in v0.06's form in `board-holder.json`, through a ready lease, with no registry; a keeper started | Its first pass logs `kept ... host=swimcoach` and adopts the holder: no new start, no launch files, the record unchanged ([Stopping and starting the keeper](#stopping-and-starting-the-keeper)) |
| H16 | A record of `<p>` that cannot be removed (the case holds it open sharing read and write but not delete, so the keeper reads it and cannot remove it; architect, 2026-11-21, for v0.07-T04) after it fails to be trusted, beside a holder on a host `<r>` whose project comes after `<p>`'s | `<p>`'s turn ends with its `failed lease=- alias=- pid=- host=<p> reason=The pass stopped on an error: The record could not be removed:` line, `<r>`'s turn still runs and keeps its holder, and the one-pass run exits 1 ([The order of a pass](#the-order-of-a-pass)) |
| H17 | A registry with the box and `<p>`, both `ready`; no lease, no holder record and no holder log for any host; one-pass run, then a keeper's passes | Each pass logs exactly one line, the box's `idle lease=- alias=- pid=-`, with no `host=` and no line for `<p>` ([Idle](#idle), [The hosts of a pass](#the-hosts-of-a-pass)) |
| H18 | `AGENTSPACE_STATE_DIR` set to a folder other than `<USERPROFILE>\.agentspace`; a `hosts.json` in the state folder that is not JSON, and a valid registry with the box and `<p>`, both `ready`, in `<USERPROFILE>\.agentspace`; a ready `<p>` lease; one-pass run | The pass reads the registry from `<USERPROFILE>\.agentspace` and never from the state folder: no `Host registry could not be read` line, and a `started ... host=<p>` line ([Hosts](#hosts)) |

**The rest of v0.07's done-when for the keeper is live** (D53): that the keeper holds the board on
agentspace's own host, which no stand-in can show.

### Conformance cases for the guard's start

**Each behavior below is one that a stand-in can show, and MUST be a conformance case of the
keeper, written by E0.08-T04-S03** (D62, D63). Each runs with a throwaway `USERPROFILE` and state
folder, written `<home>` and `<state>`, a stub `-SshExe` where a holder is needed, and
`AGENTSPACE_GUARD_CMD` naming a stand-in guard: a `.cmd` that appends one line to a file of the
case's own in `<home>` for each run, with its arguments, its working folder and the time, and
exits 0, or stays alive until the case stops it where the case says so. Where a case reads the
guard's environment, the stand-in writes the variables the case names to that file, and the case
compares them and reports a mismatch by the variable's name, never by its value (AGENTS.md, "A
test MUST NOT print, log or format the values of an environment it captured"). No case runs a
real guard, and no case touches `<home>`'s `guard.log`, which only a guard writes.

| # | The case | It shows |
|---|---|---|
| G1 | Keeper, `-MaxPasses 1`, no lease, no registry | One `guard started pid=<pid>` line, with no `host=`, before the pass's `idle` line; the stand-in ran once, with exactly the one argument `guard`, in the working folder `<home>`; exit 0 ([When the keeper starts it](#when-the-keeper-starts-it), [How the keeper starts it](#how-the-keeper-starts-it)) |
| G2 | Keeper, `-IntervalSeconds 1`, `-MaxPasses 5`, `-GuardIntervalSeconds` not given | Exactly one `guard started` line and one run of the stand-in: at most one start every 300 seconds |
| G3 | Keeper, `-IntervalSeconds 1`, `-GuardIntervalSeconds 3`, passes over about 10 seconds | Two or more `guard started` lines, each one's `<time>` at least 3 seconds after the one before's, and one run of the stand-in for each ([When the keeper starts it](#when-the-keeper-starts-it): the seconds count from the end of the attempt) |
| G4 | Keeper, no lease, `-IntervalSeconds 20`, `-GuardIntervalSeconds 12`, `-MaxPasses 2` | Exactly two `guard started` lines: the second between the first pass's `idle` line and the second's, about 12 seconds after the first start; the second `idle` line still about 20 seconds after the first: a start that falls due in a wait is made there, and the wait ends when it would have |
| G5 | Keeper, `-IntervalSeconds 1`; the case holds `<home>\.agentspace\guard.lock` open sharing nothing from before the keeper starts, then closes it | No run of the stand-in and no guard line while it is held, and the passes go on; after it is closed, a `guard started` line no later than just before the first pass that starts after the close, without waiting `-GuardIntervalSeconds` ([guard.lock: tried, never held](#guardlock-tried-never-held)) |
| G6 | Keeper; a folder in place of `<home>\.agentspace\guard.lock`; `-GuardIntervalSeconds 5` | `failed lease=- alias=- pid=- reason=The guard could not be started: guard.lock could not be opened: Windows error <n>.`, no run of the stand-in, the next such line no sooner than 5 seconds later, and every pass's lines as without it |
| G7 | Keeper; no `<home>\.agentspace` folder (the state folder elsewhere) | A `guard started` line, and the keeper creates neither `<home>\.agentspace` nor `guard.lock` |
| G8 | Keeper; `AGENTSPACE_GUARD_CMD` naming no file; a box holder running through a ready lease | `failed ... reason=The guard could not be started: The program that AGENTSPACE_GUARD_CMD names does not exist.`; the holder kept in every pass, no hold-off set, no record changed, exit 0 at `-MaxPasses` ([What a failed start changes](#what-a-failed-start-changes)) |
| G9 | Keeper run in a working folder that holds the stand-in; `AGENTSPACE_GUARD_CMD` the stand-in's relative path | The stand-in runs: the relative value was made full against the keeper's working folder |
| G10 | Keeper given `AGENTSPACE_STATE_DIR` as a relative path, `AGENTSPACE_SSH_DIR`, `AGENTSPACE_PROXY_COMMAND`, `AGENTSPACE_AWS_CMD`, and a variable of the case's own whose name does not end `_CMD` | The stand-in sees `USERPROFILE` as `<home>`, the state and ssh folders full, `AGENTSPACE_PROXY_COMMAND` and `AGENTSPACE_AWS_CMD` exactly as given, `AGENTSPACE_GUARD_CMD` as given, and not the case's own variable ([The guard's environment](#the-guards-environment)) |
| G11 | Keeper, `-MaxPasses 1`; the stand-in stays alive; then, while it still runs, a new keeper | The keeper exits 0 while the stand-in still runs, outside the keeper's job; the new keeper takes `board-holder-keeper.lock` while the stand-in runs, logging no `another keeper is running` line, so the guard holds no handle of the keeper's lock; the case then stops the stand-in by the PID of the first `guard started` line, checked first ([How the keeper starts it](#how-the-keeper-starts-it)) |
| G12 | One-pass run, with `AGENTSPACE_GUARD_CMD` set | No run of the stand-in and no guard line ([When the keeper starts it](#when-the-keeper-starts-it)) |
| G13 | Keeper, `-MaxPasses 1`; the case holds `board-holder.lock`, so the pass logs `skipped` | A `guard started` line all the same: the start is not inside a pass, and does not wait on `board-holder.lock` |
| G14 | Keeper; the stand-in a `.cmd` whose path holds a space | The stand-in receives exactly the one argument `guard` |
| G15 | `-GuardIntervalSeconds 5` without `-Watch`; `-Watch -GuardIntervalSeconds 0` | `-GuardIntervalSeconds is for -Watch.` and `-GuardIntervalSeconds '0' is not a whole number from 1 to 86400.`, each with exit 2 and nothing logged ([Usage errors](#usage-errors)) |
| G16 | A second keeper, started while the case holds `<state>\board-holder-keeper.lock` as a keeper would | Its one `another keeper is running pid=<pid>` line, exit 0, no run of the stand-in and no guard line: a keeper that cannot take its lock starts no guard ([board-holder-keeper.lock: one keeper at a time](#board-holder-keeperlock-one-keeper-at-a-time)) |

**Every existing keeper case is run with a stand-in guard from v0.08 on**, and ignores the guard's
lines where it reads a keeper's log line for line (H1; architect, 2026-11-21, for v0.08-T01, pin
P18). A case MUST NOT assume a keeper's first line is a pass's.

**Three `<why>` rows of [The keeper's lines for it](#the-keepers-lines-for-it) are the keeper
crate's own tests, not conformance cases**, since no case can bring them about from outside
without touching the program under test: `The operator program is not installed beside this
program.`, which needs a keeper with no `AGENTSPACE_GUARD_CMD` and no operator beside it;
`Windows' command interpreter cannot be found, because SystemRoot is not set.`; and
`The detached start failed: <error>`, the bound's text among them.

**What no stand-in can show is live, and is named here for the live turns** (D62):
- **E0.08-T05-S01, after the install:** the installed keeper, restarted to load the new build
  ([Stopping and starting the keeper](#stopping-and-starting-the-keeper)), logs `guard started
  pid=<pid>` at its first iteration and then about every 300 seconds. `guard.log` gains a line
  only for a run that stops a host, finds one unexplained, or fails
  ([operator.md, guard](operator.md#guard)), so a run that leaves every host as it was adds none.
  The laptop-away check, which stops the keeper, sees no `guard started` line while it is stopped.
- **E0.08-T06-S01 and S02, release:** with the keeper's real holder on the host, the release ends
  it, and `board-holder.log` shows no `stopped` line for it, then `grace seconds=95` (with the
  host's `host=` field for a host that is not the box), or `lost` and the grace, or, when a pass
  came between the release's write and its end of the clients, `stopped` and the grace; and no
  `ssh.exe` of that alias is left on the laptop ([Release grace](#release-grace)). The host is then `stopped`
  within D56's bound by a guard the keeper started.
- **E0.08-T06-S01 and S02, expiry, a quiet held lease and the next lease:** each host stop or
  keep is made by guards the keeper started, at most about 300 seconds apart.
- **E0.08-T06, an unexplained host on agentspace's host:** a host started with `up -Host` and no
  lease is reported by `guard.log` and `status -All`, and stopped within the bound, by a guard the
  keeper started.

## Requirements the prototype does not meet yet

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository, so this table
is history.** It records what the rewrite was built to do differently, and no conformance check
runs against the prototype any more. The requirement in each row's first column still binds
the keeper that is built now.

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

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository.** A question
below that asks what the prototype does is moot, since nothing can run it to find out, and its
answer was never a requirement of the rewrite.

**What reading the code and the docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080), E0.04-T05-S04 (#0091) and E0.04-T05-S02 (#0089), every question here is
either closed or marked "Later, not blocking", with the reason in its last cell, and none of
those blocks building a drop-in of contract v0.

| | Question | Settled by |
|---|---|---|
| **Q1** | **Later, not blocking:** **Is a keeper for a laptop that is not Windows in scope?** Everything here is Windows: Job Objects, WMI, Task Scheduler, `cmd.exe`. D4 makes the host's platform a parameter, but says nothing of the laptop's. | Contract v0's caller runs on Windows (D8; [lease-api.md](lease-api.md#the-program)), and this page specifies a Windows keeper in full, so a drop-in has nothing to guess. Later: the architect, with the lease CLI's Linux and macOS callers |
| **Q2** | **Closed by E0.07-T01-S04 (#0144; D30, D50, D55).** ~~One holder per host, or one in all?~~ One per host. Each pass groups the ready leases by their host ([Which host a lease is on](#which-host-a-lease-is-on)) and, for each host, keeps or starts one holder through that host's latest-expiring ready lease, ties to the lowest ID. The box keeps v0.06's record and log; another host has its own, named by its project ([Hosts](#hosts)). The two locks, the keeper's log and the launch files stay one for every host (architect, 2026-11-20, for v0.07-T01). Backoff and release grace are per host ([Watching](#watching)). | Closed |
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
| **Q19** | **Closed for v0.07 by E0.07-T01-S04 (#0144; D44, D46).** ~~Where does a keeper get the release grace for a host other than today's box?~~ Every host is made from the one template and gets `box/system`, and with it the box's sshd keepalive settings, from `sync` (D44, D46), so the box's grace is every host's, and `-ReleaseGraceSeconds` sets one grace for all ([Release grace](#release-grace)). A host of another platform (D4) or with other keepalive settings would need a grace of its own, which the lease or its driver would have to give; nothing in v0.07 makes one. | Closed for v0.07. Later, with a host of other settings: the architect |
| **Q20** | **Later, not blocking:** **Does a holder's ProxyCommand end when the holder ends by itself?** A stop ends the holder's tree; a holder that dies or is killed by something else leaves its children untraceable by parent, and the keeper never looks for them. | This page states the rule to build, that the keeper never looks for them ([Stopping a holder](#stopping-a-holder)), and what the ProxyCommand does is [operator.md](operator.md#the-proxycommand)'s. Later: a run on the laptop |
| **Q21** | **Later, not blocking:** **Should `-Uninstall` wait for the launcher to exit before it deletes the exe?** It can fail on a launcher still exiting ([The installer](#the-installer)). Unobserved. | The installer's `-Uninstall` is stated as it is, and running it again removes the exe. Whether a compiled keeper keeps a launcher at all goes with the installer ([Q8](#open-questions)); where it keeps none, this concerns only a launcher that a prototype install left behind. Later: the architect; a run on the laptop |
| **Q22** | **OBE, overcome by D48's ready-host ruling (architect, 2026-11-20, for v0.07-T01).** ~~Should the box leave out a ready lease of another project whose host is not `ready`?~~ The case needed a host whose record leaves `ready` while one of its leases is ready, and D48 now says a provisioning run never takes a `ready` host out of `ready`, and nothing in agentspace moves a record out of `ready` or removes one ([state.md, Its states](state.md#its-states)). A lease is made `ready` only on a host that is `ready`, so only a person's edit or removal of `hosts.json` can put such a lease on the box, and the keeper's stop of that host's holder is a safeguard for that edit ([The order of a pass](#the-order-of-a-pass)). [Which host a lease is on](#which-host-a-lease-is-on) follows D55's last row exactly, as the ProxyCommand does. | OBE: D48 |
| **Q23** | **Later, not blocking:** **Should a keeper pass sooner, while every host it knows is in a hold-off, so that a lease newly ready on another host gets its first holder at once?** [The keeper's wait](#the-keepers-wait) waits until the first hold-off ends, at most the grace or `-BackoffMaxSeconds`, as v0.06's keeper did for any lease while the box waited out a grace or a backoff (architect, 2026-11-20, for v0.07-T01). | Passing at `-IntervalSeconds` through a hold-off would add passes, and lines, that v0.06's keeper never made with only the box, which the card's done-when keeps as it was. The delay is bounded, and comes only while the box is in a hold-off too. **A candidate that keeps that parity, from the review:** during a wait in which every one of the keeper's hosts is in a hold-off, the keeper re-reads `leases.json` and `hosts.json` every `-IntervalSeconds`, without a lock and logging nothing, and ends the wait early only for a ready lease on a host that was not in its last pass. Later: the architect |

# agentspace laptop state, reference v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for every file and folder agentspace keeps on the caller's machine: where each one
is, its format, its access rules, who writes it, who reads it, and when it is removed.** It is
written so that the files can be read and written without reading any of agentspace's code (D20,
D34), and so that a lease made by the prototype can be renewed and released by the rewrite, and
the reverse (#0054). The reasoning behind each rule is in [decisions.md](decisions.md), cited as
`D<n>`.

**What it does not hold (D34).** Each of these facts has its own reference, and this page cites it
rather than restating it:
- **The lease CLI's commands, fields, states, errors and exit codes, and what a caller is promised
  about a lease's credential:** [lease-api.md](lease-api.md), in particular
  [The credential](lease-api.md#the-credential) and
  [Concurrency and atomicity](lease-api.md#concurrency-and-atomicity).
- **The operator's commands, their messages, the ProxyCommand and the token push:**
  [operator.md](operator.md).
- **The boundary between the lease CLI and a backend:** [driver.md](driver.md).
- **What the keeper does with its files, its log lines and its locks:** keeper.md, at
  [Log lines](keeper.md#log-lines), [Locks](keeper.md#locks) and
  [Starting a holder](keeper.md#starting-a-holder).
- **When the driver starts a provisioning run, what the run does, and which host an operator
  command acts on:** [driver.md](driver.md) and [operator.md](operator.md) (D48, D49). This page
  owns the files they read and write.
- **What the guard does with its files, and each line it logs:** [operator.md](operator.md#guard)
  (D56, D57). When the keeper starts it, and what the keeper does with its lock, is keeper.md's, at
  [Starting the guard](keeper.md#starting-the-guard) and
  [guard.lock: tried, never held](keeper.md#guardlock-tried-never-held). This page owns the files, and the definition of a live lease, a
  lease's end and the horizon.

**A host per project (v0.07, D43 to D55).** Each project's leases are placed on its own host. The
box, today's one instance, is swimcoach's host and keeps every file and every byte v0.06 gave it
(D43); every other host adds the files of [The host registry](#the-host-registry),
[The project list](#the-project-list), [A host's folder](#a-hosts-folder),
[Agentspace's host keys](#agentspaces-host-keys),
[A lease alias for a host that is not the box](#a-lease-alias-for-a-host-that-is-not-the-box) and
[Another host's holder](#another-hosts-holder). Every name here is D55's. The rulings this page
cites as "architect, 2026-11-20, for v0.07-T01" are the facts D43 to D55 left open, as the
architect ruled on them for v0.07's first turn.

**The guard (v0.08, D56 to D63).** A program of the operator stops each host that no live lease
explains, and it keeps two files of its own, [The guard's files](#the-guards-files). The words it
uses for a project's leases are defined once, in
[Live leases and the lease horizon](#live-leases-and-the-lease-horizon). Every name there is D63's.
**The architect ruled on the dev lead's pins P1 to P4, P10 to P12 and P16 for v0.08-T01 on
2026-11-21, and those facts are cited as "architect, 2026-11-21, for v0.08-T01".** The choices
this page's drafter made in those two sections, and in the changes made for the guard to the
sections around them, where D56 to D63 left the point open, were ruled the same day by pin P18,
and are cited the same way with it. **The release log ([The release log](#the-release-log)),
which D59 and D63 call for and the dev lead's pin P16 settles, is cited the same way.**

**Size classes (v0.09, D64 to D71).** A lease names a size, a floor of vCPUs and memory (D65),
and the EC2 driver maps it to one instance type ([driver.md, The EC2 driver](driver.md#the-ec2-driver)).
This page owns one new fact for it: the host registry's record member `instance_type`, the
instance type its host's stack was last deployed with ([The instance type](#the-instance-type)),
and with it the rule that `stack-parameters.json`'s `InstanceType` is no longer read
([Files in the checkout](#files-in-the-checkout)). Every name is D71's. **Where D64 to D71 left a
point open, the architect ruled it on the dev lead's pins for v0.09's specs, and each ruling is
cited "architect, 2026-11-23, for v0.09-T01, pin Pn" where it stands.** A fact marked "architect's
to confirm" is a choice made for E0.09-T01-S02 (#0198) that no decision or pin settles yet,
binding as written until the architect rules on it.

**Defects carried in.** #0041's second and third points (a record without its driver; the order
of renew's and release's writes) are carried into this page as requirements on behavior (D28,
D34), where they touch the files; the rest of #0041, and #0029, are
[lease-api.md](lease-api.md#hardening-carried-in)'s.
[Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
lists every requirement here that the prototype misses.

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository.** Every note
on this page that names the prototype, and every sentence that compares a rule with its behavior,
is history: it says where a requirement came from, or what the rewrite replaced. None tells a
reader to run the prototype, and none binds a program that is built now. What binds is the rule
each note stands beside. The two programs that shared a home folder and a state folder through
v0.05 and v0.06's cutover (the checklist in
[Between the prototype and the rewrite](#between-the-prototype-and-the-rewrite)) are now one, and
every program MUST still take the state lock as [Concurrency](#concurrency) says.

**What v0.05's Rust build shows (E0.05-T04-S01, 2026-11-19, [the run record](runs/e0.05-t04-s01.md)):**
the conformance file for each of these points passes against it. The last column of
[the table below](#requirements-the-prototype-does-not-meet-yet) says it row by row.
- **#0041, second point (a record without its driver):**
  `conformance/lease-cli/test-unmet-record-no-driver.ps1`.
- **#0041, third point (the order of renew's and release's writes):**
  `conformance/lease-cli/test-unmet-renew-keeps-certificate.ps1`,
  `test-unmet-release-partial.ps1` and `test-unmet-release-released.ps1`.
- **#0029, third point (`create` and `renew` never create the CA):**
  `conformance/lease-cli/test-unmet-no-ca.ps1`.
- **The home folder that both programs share:** `conformance/lease-cli/test-interop.ps1`, each way
  round, with the prototype as the peer. **Retired by E0.06-T06-S01 (2026-11-20)** with the
  prototype, which was its peer: the file was removed, and its last run, 36 passed each way, is
  [the run record](runs/e0.06-t04-s01.md).

## Contents

- [Conventions](#conventions): [the home folder](#the-home-folder),
  [what moves which files](#what-moves-which-files), [access rules](#access-rules),
  [replacing a file](#replacing-a-file)
- [Every file at a glance](#every-file-at-a-glance)
- [The lease state file](#the-lease-state-file), with
  [live leases and the lease horizon](#live-leases-and-the-lease-horizon)
- [The host registry](#the-host-registry), with [the instance type](#the-instance-type)
- [The project list](#the-project-list)
- [A lease's credential](#a-leases-credential), with
  [a lease alias for a host that is not the box](#a-lease-alias-for-a-host-that-is-not-the-box) and
  [agentspace's host keys](#agentspaces-host-keys)
- [A host's folder](#a-hosts-folder), with [a host's operator alias](#a-hosts-operator-alias)
- [The CA](#the-ca)
- [The push stamp](#the-push-stamp)
- [The keeper's files](#the-keepers-files), with [another host's holder](#another-hosts-holder)
- [The guard's files](#the-guards-files): [the guard's log](#the-guards-log) and
  [the guard's lock](#the-guards-lock)
- [Files agentspace reads and never writes](#files-agentspace-reads-and-never-writes)
- [Files in the checkout](#files-in-the-checkout)
- [Project tooling](#project-tooling)
- [Concurrency](#concurrency)
- [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
- [Open questions](#open-questions)

## Conventions

### The home folder

- **Contract v0's caller runs on Windows (D8)**, and every path on this page is a Windows path
  unless it says otherwise. A caller on Linux or macOS is later, and not blocking: it comes with the
  hosted control plane (#0011). A lease's host MAY be Windows or Linux (D4), and nothing on this
  page depends on the host's platform except where it says "today's box".
- **The home folder is the value of the `USERPROFILE` environment variable when the program
  starts**, and `~` on this page means it. **agentspace MUST read it from that variable**, and
  MUST NOT ask the operating system for the profile's location some other way, so that a check
  can run every command against a throwaway home by setting `USERPROFILE` for the process under
  test ([What moves which files](#what-moves-which-files)).
- **`%USERPROFILE%\.agentspace` is agentspace's own folder (D21).** Everything agentspace keeps on
  the laptop MUST live under it, never under `%LOCALAPPDATA%` or `%APPDATA%`: the Claude desktop
  app redirects writes there into its package cache, where no process outside the app sees them
  (D21). The exceptions are the files in `~\.ssh` that the OpenSSH client reads, and the files in
  the checkout that are not state ([Files in the checkout](#files-in-the-checkout)). The host
  registry, the project list, each host's folder and agentspace's own `known_hosts` are state, so
  D21 puts them here too (D45, D47, D54, D55). They replace the instance-ID cache, which was here
  until v0.07 ([Files in the checkout](#files-in-the-checkout)). **The guard's log and lock are
  state too**, so they live here as well (D56; [The guard's files](#the-guards-files)).
- **`<state folder>`** is the folder that `AGENTSPACE_STATE_DIR` names when it is set and not
  empty, and `%USERPROFILE%\.agentspace` otherwise.
- **`<ssh folder>`** is the folder that `AGENTSPACE_SSH_DIR` names when it is set and not empty,
  and `%USERPROFILE%\.ssh` otherwise.
- **`USERPROFILE` unset or empty:** every command that needs a path under the home folder MUST
  fail with `internal` ([lease-api.md](lease-api.md#exit-codes-and-errors)), change no lease, and
  write nothing under any home folder. The prototype fails then too, with its runtime's own
  message, which is not behavior to copy.
- **A relative value** of `USERPROFILE`, `AGENTSPACE_STATE_DIR` or `AGENTSPACE_SSH_DIR` is resolved
  against the process's working folder. **An implementation MUST make it a full path before it
  writes it into a file** (the alias file, or a holder's launch request), because the program that
  later reads that file runs in another working folder. The prototype writes such a path as given
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
  The same holds for a host's operator alias ([A host's operator alias](#a-hosts-operator-alias)),
  and for the folder variables a provisioning run is started with, `USERPROFILE` among them, which
  are made full before the start ([The provisioning run's start](#the-provisioning-runs-start)).
- **A value that names an existing file, not a folder,** makes every command that needs that
  folder fail with `internal`, writing nothing in it.
- **Every folder on this page is created when it is first needed**, by whichever writer needs it,
  with the access rules it inherits, unless its own section says otherwise. A missing folder is
  never an error for a reader: it means the files in it do not exist. That includes
  `%USERPROFILE%\.agentspace\hosts\` and each host's folder in it
  ([A host's folder](#a-hosts-folder)).

### What moves which files

**Each environment variable moves only the files this table names.** Each is read when a program
starts; unset or empty means the default. What each means to the lease CLI is
[lease-api.md](lease-api.md#environment)'s.

| Variable | Moves | Does not move |
|---|---|---|
| `USERPROFILE` | Everything on this page under `%USERPROFILE%`: `.agentspace` and all in it, and `.ssh`; and `.aws`, which the AWS CLI reads ([Files agentspace reads and never writes](#files-agentspace-reads-and-never-writes)) | The checkout; `%LOCALAPPDATA%`; and where a program finds agentspace's other programs, which is its own folder ([operator.md](operator.md#how-each-program-finds-the-others)) |
| `AGENTSPACE_STATE_DIR` | The lease state file and its lock, the release log ([The release log](#the-release-log); architect, 2026-11-21, for v0.08-T01), and every keeper file but the task's launcher program, the task launcher's error log included, as the task's logon environment sets it ([The keeper's logs](#the-keepers-logs)), and another host's holder record and holder log among them ([Another host's holder](#another-hosts-holder)) | The lease folders, the CA, the push stamps (the box's and every host's), the host registry and its lock, the guard's log and lock (D63), the project list, every host's folder, agentspace's `known_hosts`, the retired instance-ID cache, the stack parameters file, the operator's folder, the install folder with the task's launcher in it, the prototype's `BoardHolderLauncher.exe`, and the project tooling, which stay under `%USERPROFILE%\.agentspace` |
| `AGENTSPACE_SSH_DIR` | `config`, its backups, `agentspace.d` and the alias files, for `create` and `release`, and `config`, its backups and `agentspace.d` for the operator; and a host's operator alias and its temporary file, for the operator's commands that write it ([A host's operator alias](#a-hosts-operator-alias)) | The user's `known_hosts`, agentspace's `known_hosts`, and the user's own key; ssh itself reads neither variable |
| `LOCALAPPDATA` | The localswim receipt, which the operator reads ([Files agentspace reads and never writes](#files-agentspace-reads-and-never-writes)) | Anything agentspace writes |

- **A check that isolates the lease CLI completely MUST set `USERPROFILE` to a throwaway folder**,
  and make a CA there with the operator's `ca` first ([operator.md](operator.md#ca)). Setting
  `AGENTSPACE_STATE_DIR` alone leaves the lease folders, and the CA they are signed with, in the
  real profile. This settles [lease-api.md Q9](lease-api.md#open-questions).
- **A check that moves `USERPROFILE` and runs the operator's `up`, or any other operator command
  that calls AWS (`deploy`, `sync`, `bootstrap`, `provision` among them), or connects through a
  lease's alias or a host's operator alias, loses the AWS sign-in**, because the AWS CLI that the
  operator and the ProxyCommand run finds `%USERPROFILE%\.aws` through the same variable. Such a
  check MUST NOT expect a connection to reach any host.
- **A provisioning run that the driver starts is given the starting program's `USERPROFILE`, and
  each of its `AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND` if and only
  if the starting program has it set and not empty**; a value of one of those three that the logon
  environment holds and the starting program does not is removed (architect, 2026-11-20, for
  v0.07-T01). So the run uses the
  files of the command that started it
  ([The provisioning run's start](#the-provisioning-runs-start)).
- **The guard's log and lock move with `USERPROFILE` and not with `AGENTSPACE_STATE_DIR`** (D56;
  D63 pins both beside `hosts.json`), as the host registry and its lock do, so a conformance home
  that sets `USERPROFILE` gets its own guard log and lock. **The lease state file the guard reads
  is the one in the `<state folder>` the guard runs with**, so a guard that the keeper starts is
  given the keeper's `AGENTSPACE_STATE_DIR` along with the rest of its environment (the dev lead's
  pin P4; architect, 2026-11-21, for v0.08-T01;
  [keeper.md, The guard's environment](keeper.md#the-guards-environment) and
  [operator.md](operator.md#guard) say how). **That environment is built from the user's logon environment as `agentspace-detach`
  builds it**, so the AWS CLI's own variables reach the guard (the same ruling).
  A lease recorded under another `AGENTSPACE_STATE_DIR` is not seen by that guard
  ([Live leases and the lease horizon](#live-leases-and-the-lease-horizon)).
- **The OpenSSH client does not read `USERPROFILE` or `AGENTSPACE_SSH_DIR`.** An alias written into
  a moved `<ssh folder>` is usable only by an ssh pointed at the alias file itself with `-F`, or
  at a config of its own whose `Include` names that `agentspace.d` by its full path, since a
  relative `Include` resolves against the user's own `~/.ssh` ([The `Include` line](#the-include-line)).
  **Agentspace's `known_hosts` is named in an alias by its full path**
  ([Agentspace's host keys](#agentspaces-host-keys)), so it is found wherever the alias is read.
- Note on where this came from: in the prototype, every lease and operator script reads
  `$env:USERPROFILE`, and the CA check it starts inherits the variable. The operator's `deploy` and
  `sync` find the user's public key and `CLAUDE.md` through the shell's home variable instead, and
  the keeper task's launcher through the operating system's profile folder ([Q2](#open-questions)).

### Access rules

**Three kinds of access rule are used on this page**, named here once:

- **Inherited:** the file or folder keeps the access rules its parent folder passes down, and
  agentspace sets none of its own. Under a default Windows profile that is the user, `SYSTEM` and
  `Administrators`, each with full control (D17).
- **OpenSSH-safe:** what the Windows OpenSSH client accepts for a file it reads: owned by the user,
  `SYSTEM` or `Administrators`, and readable or writable by no other principal. A private key that
  another principal can read is refused by ssh as unprotected, and so is a configuration file that
  another principal can write.
- **Owner-only:** all four of these hold. Only the CA uses it ([The CA](#the-ca)).
  1. **Owner:** the file's or folder's owner is the security identifier (SID) of the user the
     process runs as: the user SID of the process's token, never a group such as
     `Administrators`.
  2. **Protected:** its access control list (DACL) is protected from inheritance (on Windows,
     `SE_DACL_PROTECTED`), so no entry is inherited from the parent folder.
  3. **Exactly one rule:** the DACL holds exactly one access control entry (ACE), explicit or
     inherited, and it is an allow entry for that same SID. A second entry of any kind, allow or
     deny, for any principal, the user included, breaks it.
  4. **Full control:** that entry's rights include every right of full control (`FullControl`,
     `0x1F01FF` for a file or folder). Rights beyond it do not break the rule.
  - **A folder's one entry is inheritable by the files and folders in it** (object inherit and
    container inherit, `(OI)(CI)` in `icacls` terms), so a file made in it starts with that one
    entry. **A file's one entry is explicit**, not inherited, since the file is protected. The
    check of owner-only does not look at an entry's inheritance flags; making an owner-only
    folder MUST set them.
  - Note on where this came from: the prototype reads the owner and every entry by SID, and
    compares them with the process token's user SID.

**A file or folder agentspace writes MUST NOT grant access to any principal its parent folder
does not already grant, with two exceptions:** `config`, which keeps the rules the user gave it
([The `Include` line](#the-include-line)), and a public key or a certificate, which is not secret
and MAY be readable by everyone, as Windows' `ssh-keygen` makes it
([The lease folder](#the-lease-folder), [The CA](#its-access-rules)).

**A caller that is not Windows is later, and not blocking (D8).** When one comes, owner-only
SHOULD mean owned by the user with mode `0700` for a folder and `0600` for a file, and
OpenSSH-safe what that system's OpenSSH accepts. The prototype runs on Windows alone, so nothing
has been observed there.

### Replacing a file

**Where this page says a file is replaced atomically, the writer MUST:**
1. Write the whole new content to a temporary file in the same folder, under the name the file's
   section gives. The name MUST be one the file's readers never mistake for the file.
2. Move the temporary file over the file, replacing it in one rename. **On Windows the rename
   MUST use POSIX semantics where the file system has them**: `SetFileInformationByHandle` with
   `FileRenameInfoEx` and `FILE_RENAME_FLAG_REPLACE_IF_EXISTS | FILE_RENAME_FLAG_POSIX_SEMANTICS`,
   after `FlushFileBuffers` on the temporary file, which is what Rust's `std::fs::rename` does on
   current Windows. Only where the file system refuses those semantics MAY it fall back to
   `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`. (Corrected by the architect, 2026-11-19, from
   E0.05-T02-S01: classic `MoveFileEx` cannot replace a file that any process holds open, even one
   that shares delete, so the rule below that such a reader "never blocks the rename" held only
   for a POSIX rename. Checked on Windows 11 build 26300.)
3. Delete the temporary file if anything failed before the move.

**Two files are replaced differently, each for its own reason:**
- **An alias file's temporary file is in `<ssh folder>`, not in `agentspace.d` beside it**, and is
  moved into `agentspace.d`. The `Include` line reads every file in `agentspace.d`, so a temporary
  file there would be read by ssh as configuration ([The alias file](#the-alias-file)). A host's
  operator alias is written the same way ([A host's operator alias](#a-hosts-operator-alias)).
- **`config` is replaced in a way that keeps its own access rules** (`ReplaceFile` on Windows),
  not by a plain rename, which would give it the temporary file's rules. Windows OpenSSH checks
  those rules ([The `Include` line](#the-include-line)). Its owner becomes the user running
  `create`, which OpenSSH accepts (corrected by the architect, 2026-11-19, from E0.05-T02-S03:
  `ReplaceFile` keeps the access list, not the owner).

**A reader of the lease state file, or of the host registry, MUST open it sharing read, write and
delete access**, so that it never blocks a POSIX rename (step 2), and so sees the old file or the
new one, never part of either.
A classic `MoveFileEx` writer, the prototype's among them, is still blocked by any open handle, and
its retry below covers that. **A reader
of any other file replaced this way SHOULD share the same**; the keeper's readers of its own JSON
files share read only, and the writer's retry below covers them
([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
**A writer whose rename fails because another process holds the file without sharing delete MAY
retry, and MUST stop retrying within 10 seconds for the lease state file and the host registry,
and 5 seconds for the keeper's JSON files**, then fail. A
retry of the lease state file runs under [the lease state lock](#the-lease-state-lock), so its
time is part of the time the command holds the lock, and of the 30 seconds another changing
command waits for it; it adds no wait of its own to that limit. A retry of the host registry runs
under [the host registry lock](#the-host-registry-lock) the same way. The prototype retries every 20 milliseconds for 10 seconds (the lease state file)
or every 50 milliseconds for 5 seconds (the keeper's JSON files).

**Every other write on this page is one attempt, with no retry:** `config`'s replace, its backup
and a new `config`, the alias file's move, a host's operator alias's move, the key pair (a
lease's or a host's operator key), signing, the certificate's move, and every deletion. A failure there fails the command at once, as its section says. The prototype reads
`config` and an existing alias file whole, sharing read only, and so does not block ssh, which only
reads them.

**A process killed between steps 1 and 3 leaves its temporary file behind.** No reader ever takes
one for the file it stands in for, and nothing in the prototype removes it later
([Q4](#open-questions)).

**A replace never creates a folder** (architect, 2026-11-19, for E0.06-T01-S07). The writer
creates the folder first where its section says it does (the state folder, `%USERPROFILE%\.agentspace`
for the push stamp and the host registry, a host's folder for that host's push stamp); a replace
into a folder that is missing fails, and leaves nothing behind.
`agentspace-state`'s `write_atomically` is that replace, for every file the rewrite writes this way.

## Every file at a glance

**Every file and folder agentspace writes or reads under the home folder.** Each has its own
section, which this table only summarizes. "Never" under Removed means agentspace never deletes it.

| Path | What it is | Written by | Read by | Removed |
|---|---|---|---|---|
| `%USERPROFILE%\.agentspace\` | The agentspace folder (D21) | The first writer | Everything below | Never |
| `<state folder>\leases.json` | [The lease state file](#the-lease-state-file) | `create`, `renew`, `release`; a provisioning run, through `place_pending`, which from v0.09 also writes a `pending` lease `unsatisfiable` (pin P1) | Every lease command; the keeper; the ProxyCommand; a provisioning run; the guard and `status -All`, read-only and without the lock (architect, 2026-11-21, for v0.08-T01, pin P18); from v0.09 the EC2 driver's place, read-only under its caller's lock, and a provisioning run's deploy step, read-only and without the lock (architect, 2026-11-23, for v0.09-T01, pin P13) | Never |
| `<state folder>\leases.lock` | [The lease state lock](#the-lease-state-lock) | Every lease command that changes leases; a provisioning run, through `place_pending` | None; it is held, not read | Never |
| `<state folder>\leases.json.<hex>.tmp` | The state file's temporary file | The same | None | At once |
| `<state folder>\release.log` | [The release log](#the-release-log) (D59, D63; pin P16; architect, 2026-11-21, for v0.08-T01) | The EC2 driver's `release`, which appends to it, and nobody else | People | Never |
| `%USERPROFILE%\.agentspace\hosts.json` | [The host registry](#the-host-registry) (D45) | The driver, in `create` and in `get` through `check_pending`; `provision`; an operator command that caches a host's instance ID; every `deploy` that names or implies a host, `provision`'s deploy step included, which records `instance_type` (D67, D71); the ProxyCommand, only a missing `instance_id` | The driver; the operator's commands; the ProxyCommand, without its lock but for that write; the keeper, read-only and without its lock; the guard, read-only and without its lock (architect, 2026-11-21, for v0.08-T01, pin P18). Never `list`, `renew` or `release` | Never |
| `%USERPROFILE%\.agentspace\hosts.lock` | [The host registry lock](#the-host-registry-lock) | Every writer of `hosts.json` | None; it is held, not read | Never |
| `%USERPROFILE%\.agentspace\hosts.json.<hex>.tmp` | The host registry's temporary file | The same | None | At once |
| `%USERPROFILE%\.agentspace\projects.json` | [The project list](#the-project-list) (D54) | A person, or the architect with Terry's yes; never agentspace | The driver and `provision`, for a project with no host | Never |
| `%USERPROFILE%\.agentspace\guard.log` | [The guard's log](#the-guards-log) (D56, D63) | `agentspace-operator guard`, which appends to it, and nobody else | People | Never |
| `%USERPROFILE%\.agentspace\guard.lock` | [The guard's lock](#the-guards-lock) (D56, D63) | `agentspace-operator guard`, for the run's life; the keeper, which only tries it | None; it is held, not read | Never |
| `%USERPROFILE%\.agentspace\hosts\` | The folder of host folders | The first writer of a host's folder | Everything in it | Never |
| `%USERPROFILE%\.agentspace\hosts\<project>\` | [A host's folder](#a-hosts-folder) | The first writer of a file in it; the driver, if it is missing, before it starts a provisioning run, which is not started if the folder cannot be made ([The provisioning run's start](#the-provisioning-runs-start)) | The operator; ssh | Never |
| `...\hosts\<project>\id_ed25519`, `id_ed25519.pub` | [The operator's key](#the-operators-key-and-certificate) for that host | The first operator command that signs a certificate for the host | `ssh`, through the operator alias; the operator, to sign | Never |
| `...\hosts\<project>\id_ed25519-cert.pub` | [The operator's certificate](#the-operators-key-and-certificate) for that host, key ID `operator-<project>`, one hour | Every operator command that names a host other than the box, `status -Host` included, and every one that connects through the host's operator alias | `ssh` | Never; replaced by the next command's |
| `...\hosts\<project>\sign-<hex>.pub`, `sign-<hex>-cert.pub` | Signing's temporary files | The same | None | At once |
| `...\hosts\<project>\provision.log` | [The provisioning run's log](#the-provisioning-runs-log) | `provision`, which opens it for append as its first step, creating it if it is missing, and writes every line there, its own lines alone (architect, 2026-11-21, for v0.07-T03); never the driver | People | Never |
| `...\hosts\<project>\provision.lock` | [The provisioning run's lock](#the-provisioning-runs-lock) | `provision`, for the run's life; the driver, which only tries it | None; it is held, not read | Never |
| `...\hosts\<project>\localswim-pushed.json` | [A host's push stamp](#a-hosts-push-stamp), for every host but the box | `up -Host`, `sync -Host`, `sync -All`, `provision` | `up -Host` | Never |
| `...\hosts\<project>\localswim-pushed.json.<hex>.tmp` | Its temporary file | The same | None | At once |
| `%USERPROFILE%\.agentspace\leases\` | The folder of lease folders | The first `create` that issues a credential | `renew`, `release` | Never |
| `%USERPROFILE%\.agentspace\leases\<id>\` | [The lease folder](#the-lease-folder) | `create` | `renew`, `release`, ssh | `release` |
| `...\leases\<id>\id_ed25519`, `id_ed25519.pub` | [The key](#the-key) | `create` | `ssh`; `renew` | `release` |
| `...\leases\<id>\id_ed25519-cert.pub` | [The certificate](#the-certificate) | `create`, `renew` | `ssh` | `release` |
| `...\leases\<id>\renew-<hex>.pub`, `renew-<hex>-cert.pub` | [Renewal](#renewal)'s temporary files | `renew` | None | At once |
| `...\leases\<id>\renew-<hex>-previous-cert.pub` | [Renewal](#renewal)'s copy of the previous certificate, kept until the record is written; the prototype makes none | `renew` | `renew`, to put it back | At once |
| `<ssh folder>\config` | [The `Include` line](#the-include-line), in the user's own file | `create`, once; the operator, before a host's operator alias | `ssh` | Never |
| `<ssh folder>\config.agentspace-backup-<time>` | The config as it was before the line was added | `create`, once; the operator, before a host's operator alias | A person | Never |
| `<ssh folder>\config.agentspace-tmp-<hex>` | The config's temporary file | `create`; the operator, before a host's operator alias | None | At once, except when a replace failed with the config gone |
| `<ssh folder>\agentspace.d\` | The folder of alias files | `create`; the operator, before a host's operator alias | `ssh`, through the `Include` line | Never |
| `<ssh folder>\agentspace.d\<alias>` | [The alias file](#the-alias-file); for a host that is not the box, [in that form](#a-lease-alias-for-a-host-that-is-not-the-box) | `create`; a provisioning run, through `place_pending` | `ssh`; `release` | `release` |
| `<ssh folder>\<alias>.agentspace-tmp-<hex>` | The alias file's temporary file | The same | None | At once |
| `<ssh folder>\agentspace.d\agentspace-host.<project>` | [A host's operator alias](#a-hosts-operator-alias) (D47, D55) | Every operator command that names a host other than the box, `status -Host` included, and every one that connects to the host through it | `ssh`; the operator, to compare its bytes | Never |
| `<ssh folder>\agentspace-host.<project>.agentspace-tmp-<hex>` | The operator alias's temporary file | The same | None | At once |
| `%USERPROFILE%\.ssh\known_hosts` | [The host key](#the-host-key), the box's | `ssh`, never agentspace | `ssh` | Never |
| `%USERPROFILE%\.agentspace\known_hosts` | [Agentspace's host keys](#agentspaces-host-keys), every other host's (D47) | `ssh`, never agentspace | `ssh` | Never |
| `%USERPROFILE%\.agentspace\ca\` | [The CA](#the-ca)'s folder | `ca`, once | `ca`, `sync`, `create`, `renew`, a provisioning run's `place_pending`, and every operator command that signs its own certificate | Never |
| `...\ca\agentspace_ca`, `agentspace_ca.pub` | The CA's key pair | `ca`, once | The same; `deploy -Host`, for `agentspace_ca.pub` | Never |
| `%USERPROFILE%\.agentspace\localswim-pushed.json` | [The push stamp](#the-push-stamp), the box's (D40; the prototype's is `localswim-pushed.sha256`) | `up`, `sync`, each for the box | `up`, for the box | Never |
| `%USERPROFILE%\.agentspace\localswim-pushed.json.<hex>.tmp` | The push stamp's temporary file | `up`, `sync` | None | At once |
| `%USERPROFILE%\.agentspace\instance-id.json` | The instance-ID cache, retired by v0.07 into the host registry's `instance_id` (D45; [Files in the checkout](#files-in-the-checkout)) | Nothing, from v0.07 | Nothing, from v0.07 | Never by agentspace; a person MAY delete it |
| `%USERPROFILE%\.agentspace\instance-id.json.<hex>.tmp` | The retired cache's temporary file, which a killed v0.06 command MAY have left | Nothing, from v0.07 | None | Never by agentspace |
| `%USERPROFILE%\.agentspace\stack-parameters.json` | [The stack parameters file](#files-in-the-checkout) (D40), which the prototype keeps in the checkout as `infra\local.parameters` | A person | Plain `deploy` of the box, every parameter but `InstanceType`, which no program reads from v0.09 (D67, D71); `deploy -Host` and `provision`, only when they create a host's stack, and then only `SubnetId` and `VpcId` | Never |
| `%USERPROFILE%\.agentspace\operator\` | [The operator's folder](#files-in-the-checkout), which holds one folder per running operator command | The first operator command that sends a file | Nothing; each command reads only its own folder in it | Never |
| `%USERPROFILE%\.agentspace\operator\<hex>\` and what is in it | One operator command's folder: the template, the box's files, the localswim CLI and the bootstrap script, written out for the programs it starts | The one operator command that made it: `deploy`, `sync`, `bootstrap` or `provision` | The AWS CLI, `scp`, as that command starts them | By that command, as it ends; one older than 24 hours, left by a killed command, by the next operator command that finds it |
| `<state folder>\board-holder.json` | [The holder record](#the-holder-record), the box's | The keeper | The keeper | The keeper |
| `<state folder>\board-holder.json.<hex>.tmp` | The holder record's temporary file | The keeper | None | At once |
| `<state folder>\board-holder.<project>.json` | [Another host's holder](#another-hosts-holder) record | The keeper | The keeper | The keeper |
| `<state folder>\board-holder.<project>.json.<hex>.tmp` | Its temporary file | The keeper | None | At once |
| `<state folder>\board-holder.log` | [The keeper's log](#the-keepers-logs), for every host | The keeper | People | Never |
| `<state folder>\board-holder-ssh.log`, `.log.1` | The box's holder's output | The holder | People | Replaced by rotation |
| `<state folder>\board-holder-ssh.<project>.log`, `.log.1` | [Another host's holder](#another-hosts-holder)'s output | The holder | People | Replaced by rotation |
| `<state folder>\board-holder.lock`, `board-holder-keeper.lock` | [The keeper's lock files](#the-keepers-lock-files) | The keeper | The keeper | Never |
| `<state folder>\board-holder-launch.<hex>.json`, `.claim`, `.result.json` | [A holder's launch files](#a-holders-launch-files) | The keeper and its launcher | The same | The keeper, in the same pass; a late launcher's result file never |
| `<state folder>\board-holder-launch.<hex>.json.<hex>.tmp`, `board-holder-launch.<hex>.result.json.<hex>.tmp` | The launch request's and result's temporary files | The keeper and its launcher | None | At once |
| `<state folder>\board-holder-launcher-errors.log` | [The task launcher's error log](#the-keepers-logs) | The task's launcher | People | Never |
| `%USERPROFILE%\.agentspace\bin\` and the programs in it | The install folder ([operator.md, Programs and install layout](operator.md#the-install-folder)), with [the keeper task's launcher](#the-keeper-tasks-launcher) | An install | Whatever runs a program; Task Scheduler, for the task | Never by agentspace, but for the `.old-` files an install renames aside and removes later ([operator.md](operator.md#the-install-folder)) |
| `%USERPROFILE%\.agentspace\BoardHolderLauncher.exe` | The prototype's task launcher, which the rewrite does not use ([The keeper task's launcher](#the-keeper-tasks-launcher)) | The prototype's keeper install | Task Scheduler, for the prototype's task | The prototype's `-Uninstall`; the rewrite's installer, a real install or `-Uninstall` |
| `%USERPROFILE%\.ssh\id_ed25519.pub`, `~\.ssh\config`'s `agentspace` entry, `~\.claude\CLAUDE.md`, `~\.aws\`, localswim's receipt and `bot.json` | [Files agentspace reads and never writes](#files-agentspace-reads-and-never-writes) | The user, the AWS CLI, or localswim | The operator; ssh; the AWS CLI | Never |
| `%USERPROFILE%\.agentspace\watchdog\`, `swimcoach-last.json`, `tmp-test-lease-alias-<time>\` | [Project tooling](#project-tooling), not the product | The project's scripts | The same | See there |

`<hex>` is 32 lowercase hex digits, new for each file (a GUID without hyphens); `<id>` is a lease
ID, always in its normalized lowercase form ([lease-api.md](lease-api.md#lease-id)); `<alias>` is
the lease's `ssh.host_alias` ([lease-api.md](lease-api.md#fields)); `<project>` is a project in its
normalized form, `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$` ([lease-api.md](lease-api.md#project)),
the `project` of a record in [the host registry](#the-host-registry). **A file named for a host
names it by that project, never by its stack:** `agentspace-host.<project>`, with a dot, is the
operator alias's name and the host's key's name in `known_hosts`, and `agentspace-host-<project>`,
with a hyphen, is only ever the stack's name (D44, D47, D55).

## The lease state file

**`<state folder>\leases.json` holds every lease created on this machine, as one JSON document
(D8, D21).** It is internal: no caller reads it, and it is not part of the lease contract
([lease-api.md](lease-api.md#schemas)). Its schema is
[`schemas/state/leases.state.schema.json`](../schemas/state/leases.state.schema.json), draft
2020-12 and strict (D20), and where this section and the schema disagree, both are wrong and MUST
be fixed together.

**The document** is `{"version":0,"leases":[<record>, ...]}`:
- **`version`** is the integer `0`. A reader MUST refuse any other value as an invalid file.
- **`leases`** is an array of records, one per lease, in the order they were created. Two records
  MUST NOT share an `id`.
- **A file that validates against the schema but holds two records with one `id` is not valid**,
  though the schema cannot refuse it. IDs are compared as the normalized lowercase strings they
  are stored as. A reader MUST treat such a file as one that does not validate (below): every lease
  command fails with `internal`, "could not be read", and changes nothing. The prototype reads
  such a file for `get` and `list`, and refuses it only when a changing command writes, after the
  driver has already acted; that is not behavior to copy
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

**A record** holds the lease document's fields ([lease-api.md](lease-api.md#fields)), with these
differences:
- **`state` is the state as stored: `pending`, `ready`, `released` or `unsatisfiable`.** `expired`
  is never stored. A reader derives it: a record stored `ready` is `expired` when the reading
  command's now is at or after its `expires_at` ([lease-api.md](lease-api.md#states)). A stored
  `ready` record keeps `ssh`, and its lease document drops it once expired.
- **`driver`** is one more member: the name of the driver that placed the lease, a non-empty
  string with no control character. It MUST NOT reach any output (D1). In v0 there is one driver,
  and its name is `ec2`; what a driver is, and its name, are
  [driver.md](driver.md#the-drivers-name)'s.
- **`ssh.user` in a record MUST match the state schema's pattern**, a lowercase letter or `_`, then
  up to 31 lowercase letters, digits, `_` or `-`. That is narrower than lease-api.md's `ssh.user`
  ([lease-api.md](lease-api.md#fields)), and the record's pattern is the one that binds, because
  every record is checked against the schema before it is written. The two stay as they are
  ([driver.md Q16](driver.md#open-questions), closed): the output's wider pattern is what a caller
  MUST accept, so that a later host needs no new contract version, and the record's narrower one is
  what a v0 driver's user MUST match. `ubuntu` matches both.
- **Every other member a record shares with the lease document has the output schema's pattern**
  ([`lease.output.schema.json`](../schemas/lease.output.schema.json)): a reason's message does not
  start with a space, and `reasons` holds each code at most once.
- **Every time is a string in the contract's form, `YYYY-MM-DDTHH:MM:SSZ`**
  ([lease-api.md](lease-api.md#times)), and a reader MUST keep it as that string, not reparse and
  rewrite it.
- **A time that matches the pattern but names no real instant**, such as `2026-02-30T00:00:00Z` or
  an hour of 24, **makes the file not valid**, as one that fails its schema does (architect,
  2026-11-19, gap FA-10). Every lease command then fails with `internal`, "could not be read", and
  changes nothing. A leap second, `:60`, is not a real instant in UTC as agentspace counts it, and
  is refused the same way.

**What each stored state carries** (the schema checks each):

| Stored state | Required | Forbidden |
|---|---|---|
| `pending` | The common fields | `expires_at`, `released_at`, `ssh`, `reasons` |
| `ready` | The common fields, `expires_at`, `ssh` | `released_at`, `reasons` |
| `released` | The common fields, `released_at` | `ssh`, `reasons` |
| `unsatisfiable` | The common fields, `reasons` | `expires_at`, `released_at`, `ssh` |

The common fields are `id`, `project`, `state`, `platform`, `profile`, `size`, `ttl_seconds` and
`created_at`. A `released` record keeps `expires_at` when the lease was ever `ready`, and has none
when it was released from `pending`.

**The driver is kept for as long as the lease is (#0041, point 2).** `renew` and `release` hand it
back to the driver that placed the lease ([lease-api.md](lease-api.md#agentspace-lease-renew),
[driver.md](driver.md#the-drivers-name)). These rules are the architect's ruling on #0041's second
point (2026-11-18):
- **Writing: `driver` MUST be written on every record that is ever `ready`.** `create` MUST write
  it on every record it appends, whatever its state, so a record carries it from the moment it is
  written; the prototype does. A record that moves from `pending` to `ready` MUST keep it.
- **Reading: in contract v0, a stored `ready` or `released` record with no `driver` MUST be read as
  placed by the v0 default driver**, the one `create` places with, named `ec2` in v0. `renew` and
  `release` MUST hand such a record to that driver, rather than fail. The schema allows a record
  without `driver` so that such a record stays valid. The prototype fails a renew or release of a
  stored `ready` record without it, with `internal`
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
- **`renew` and `release` MUST NOT add `driver` to a record that lacks it**, and MUST NOT change or
  remove it on a record that has it. So a record without it stays one, and a reader of it applies
  the rule above every time.
- **A `renew` or `release` of a record whose `driver` names no driver the reader knows MUST fail
  with `internal`, change nothing, and its message MUST NOT name the stored driver** (D1;
  [lease-api.md](lease-api.md#exit-codes-and-errors)). The prototype's message names it.
- **A `pending` or `unsatisfiable` record without `driver`** needs none: `release` of a `pending`
  record without one asks no driver, and nothing asks one about an `unsatisfiable` record
  ([driver.md](driver.md#release-a-lease)).

**How each command changes it:**
- **`create`** appends one record. It writes `id`, `project`, `state`, `platform`, `profile`,
  `size`, `ttl_seconds` and `created_at`, then `expires_at` and `ssh` for a `ready` lease or
  `reasons` for an `unsatisfiable` one, then `driver`. **An `unsatisfiable` lease's record is all
  `create` writes**: it carries `driver`, no file is made for it, and no CA check is run. **From
  v0.07 a `pending` lease's record is all `create` writes too** (D48): the common fields and
  `driver`, and no file. **`create` checks the CA before it answers `pending`**, by the five checks
  of [Its checks](#its-checks), as before it issues, and a CA that fails them fails `create` as for
  a `ready` lease, writing no record (architect, 2026-11-20, for v0.07-T01). It is answered for a project whose host is not `ready`, and an
  `unsatisfiable` lease with the one reason `no_capacity` for a project that has no host and is not
  on [the project list](#the-project-list) (D54); which answer `create` gives is
  [driver.md](driver.md#where-it-places-a-lease)'s.
- **A provisioning run, through the EC2 driver's `place_pending`, changes a `pending` record to
  `ready`** (D48; architect, 2026-11-20, for v0.07-T01). **It takes [the lease state
  lock](#the-lease-state-lock) once for each lease it places, and re-reads the file each time**
  (architect, 2026-11-20, for v0.07-T01): under the lock it re-reads the file, issues that lease's credential and
  alias as [Creation, in order](#creation-in-order) says, and then, in one write, sets `state` to
  `ready` and adds `expires_at` and `ssh`, before it releases the lock and takes it again for the
  next lease. So no hold spans several placements. It keeps every other member as it was: `id`,
  `project`, `platform`, `profile`, `size`, `ttl_seconds`, `created_at` and `driver`. **A record
  that is not `pending` when the run re-reads the file for it under the lock is skipped, and never
  changed by the run**, so a lease released meanwhile is never placed. Which leases it places, and in what steps, is
  [driver.md](driver.md#place_pending)'s.
- **From v0.09 the same `place_pending` also changes a `pending` record to `unsatisfiable`**
  (D68's change note; architect, 2026-11-23, for v0.09-T01, pin P1 and the cross-spec ruling for
  this page): a `pending` lease the host the run leaves does not meet, when a live `ready` lease
  holds the host, is answered `unsatisfiable` with the one reason `no_capacity`. Under the lease
  state lock, taken and re-read for that lease as for a placement, it sets `state` to
  `unsatisfiable` and adds `reasons`, in one write, and keeps every other member as it was, `driver`
  included; it adds no `expires_at` and no `ssh`. A record that is not `pending` at that re-read is
  skipped, as above. Which leases it answers so, and the reason's message, are
  [driver.md](driver.md#place_pending)'s; the files it removes for such a lease are its own
  leftovers, as driver.md says (pin P21).
- **`created_at` and the certificate's issue time are separate readings of the clock**, each
  truncated to the whole second, and either MAY come first
  ([lease-api.md](lease-api.md#fields)). The prototype reads `created_at` after signing.
- **`renew`** sets `expires_at` to the new certificate's valid-before time and `ttl_seconds` to the
  TTL used, and changes nothing else.
- **`release` of a stored `ready` or `pending` record** sets `state` to `released`, adds
  `released_at`, and removes `ssh`, keeping `expires_at`. **`release` of a `released` record**
  changes nothing in it, keeping its first `released_at`, though it removes the files a failed
  release left, which the prototype does not
  ([Release, and what it leaves](#release-and-what-it-leaves)). **`release` of an
  `unsatisfiable` record** changes nothing.
- **`get` and `list` MUST NOT write it, and the keeper, the ProxyCommand and the guard MUST NOT
  write it either** (the guard's: D56). `get`'s `check_pending` MAY start a provisioning run, which writes it later in a
  process of its own ([driver.md](driver.md#check_pending)); `get` itself never does (card #0143,
  D48).
- **This page requires no single reading of the clock per command.** The prototype reads it
  separately for the expiry check, for `released_at` and for the state it prints, each truncated
  to the second; how `expired` is decided is [lease-api.md](lease-api.md#states)'s.

**Its bytes:**
- **Written as UTF-8 without a byte order mark, as compact JSON with no whitespace outside
  strings, followed by one line feed (LF).** The prototype writes members in the order above;
  a reader MUST NOT depend on member order.
- **Read tolerantly (D20):** a reader MUST accept a leading UTF-8 byte order mark and any amount of
  JSON whitespace. Bytes that are not valid UTF-8 make the file invalid.
- **An object that names one member twice**, such as a record with two `id` members, **makes the
  file not valid**, whichever value a JSON parser would keep (architect, 2026-11-19, for
  E0.05-T01-S03): it could mean two things, and tolerance covers form, never meaning (D20). A
  number written with a fraction of zero, such as `3600.0` for an integer member, is form only:
  it is accepted, and written back as `3600`.
- **Replaced atomically** ([Replacing a file](#replacing-a-file)), through a temporary file named
  `leases.json.<hex>.tmp` in the same folder, written through to the disk and flushed before the
  rename. A command that changes it MUST hold [the lease state lock](#the-lease-state-lock) from
  before it reads the file until after the rename.
- **Checked against the schema, and for IDs that repeat, on every read and before every write.** A
  file that does not validate makes every lease command fail with `internal` and change nothing,
  and a change that would not validate is not written
  ([lease-api.md](lease-api.md#exit-codes-and-errors)).

**Something at `leases.json` that is not a file, such as a folder, or a file that cannot be
opened, is not a missing file** (architect, 2026-11-19, gap FB-21). Every lease command, `get` and
`list` included, fails with `internal`, "could not be read" with its own `<why>`
([lease-api.md](lease-api.md#messages)), and changes nothing. A changing command finds that before
it asks any driver anything.

**A missing file means no leases**, to every reader but the guard and `status -All` (below). The
first command that changes leases creates the state folder, if needed, and the file. No command deletes the file, and no command forgets a lease in v0
([lease-api.md](lease-api.md#states)). **Access rules:** inherited.

**Who reads it:** every lease command; a provisioning run, under the lock; and the keeper and the
ProxyCommand, which read it without taking the lock and never write it
([keeper.md#locks](keeper.md#locks); [operator.md, The ProxyCommand](operator.md#the-proxycommand)).
**From v0.09 the EC2 driver's place reads it too, read-only, under its caller's lock**: `create`
holds the lease state lock while it calls the driver, and place reads the project's live `ready`
and `pending` leases from the file as that caller read it, and never writes it (D66; architect,
2026-11-23, for v0.09-T01, the cross-spec ruling for this page;
[driver.md, The EC2 driver](driver.md#the-ec2-driver)). **A provisioning run's deploy step reads
it once, read-only and without the lock**, as the guard does, to find the type its project's
leases need ([The instance type](#the-instance-type); architect, 2026-11-23, for v0.09-T01, pin
P13). Unlike the guard, it takes a missing file for no leases; like the guard, it MUST NOT take a
file that is not valid, or that cannot be read, for no leases, and fails its step instead.
**From v0.08 the guard and `status -All` read it too, read-only and without the lock**, as the
keeper does, to find which leases are live ([Live leases and the lease
horizon](#live-leases-and-the-lease-horizon); architect, 2026-11-21, for v0.08-T01, pin P18).
**The guard and `status -All` MUST NOT take a file that is not valid, or that cannot be read, for
no live lease**, and neither takes a missing one for no leases
([Live leases and the lease horizon](#live-leases-and-the-lease-horizon)).
The ProxyCommand reads it only to find the record whose `ssh.host_alias` is its argument, and so
the host that lease is on ([Which host a connection reaches](#which-host-a-connection-reaches)).
**A reader that finds the file missing, or not valid, reads no lease from it**; what the
ProxyCommand then does is operator.md's.

## Live leases and the lease horizon

**The words below, about a project's leases, are defined once here as functions of the lease
state file's records at a reading's time** (D56, D57, D63; the dev lead's pin P1 and its rulings P10 and P11; architect, 2026-11-21, for
v0.08-T01). The guard ([operator.md, guard](operator.md#guard)) and `agentspace-operator status -All`
([operator.md, status](operator.md#status)) use them, and cite this section without restating it.
**They are functions of `agentspace-state`**, one home for both programs (D63), and their
signatures are pinned on E0.08-T02-S01's card. This section says what they return, never how a
program calls them.

- **A reading's time** is one instant, which a program takes once for a run and uses for every
  lease in it, so no lease is live at one step of a run and not live at the next (architect,
  2026-11-21, for v0.08-T01, pin P18). Times are compared as instants, to the second, from the strings the file stores
  ([The lease state file](#the-lease-state-file)).
- **A project's leases** are the lease state file's records whose `project` equals the `project`
  of the host's record in [the host registry](#the-host-registry), compared exactly, as the
  normalized strings both are stored as. The box is the record whose `stack` is `agentbox`, and its
  `project` is `swimcoach` ([The host registry](#the-host-registry), rule 3), so the box's leases
  are swimcoach's, and no function takes a stack. This is not keeper.md's "which host a lease is
  on" ([keeper.md, Which host a lease is on](keeper.md#which-host-a-lease-is-on)): a lease of a
  project with no host record explains no host, the box included.
- **A live lease** is one whose state at the reading's time is `pending` or `ready`, the state
  decided as [lease-api.md, States](lease-api.md#states) decides it, with expiry applied by that
  time: a record stored `ready` is `expired`, and so not live, when the reading's time is at or
  after its `expires_at` ([The lease state file](#the-lease-state-file)). `released`, `expired` and
  `unsatisfiable` are not live.
- **A lease's end** is the instant the lease stopped being live, and a live lease has none
  (architect, 2026-11-21, for v0.08-T01, on pin P11):
  - **For a `released` lease, the earlier of its `released_at` and its `expires_at`.** A lease
    released after it expired stopped being live when it expired (D56: "whether by release or by
    expiry"), so its end is its `expires_at`, and not a `released_at` that came later. A `released`
    record keeps its `expires_at` when the lease was ever `ready` ([The lease state
    file](#the-lease-state-file)).
  - **For a `released` lease that was never `ready`, its `released_at`**, since such a record has
    no `expires_at`: it was a `pending` lease, live until it was released.
  - **`expires_at`** for a lease that expired, a stored `ready` record at or past its `expires_at`.
  - **None** for an `unsatisfiable` lease, which never became `ready`.
- **The project's last lease end** is the latest end among its leases that are not live, or
  none when no lease of the project has an end.
- **The project's horizon** is the latest `expires_at` among its live leases that have one; with
  live leases but none that has one, which is to say only `pending` leases, the reading's time;
  with no live lease, its last lease end; with none of these, the host's launch time, which the
  caller takes from `ec2 describe-instances` (D57, as D63 changed it) and passes in, since no
  file on the laptop holds it.
  - **A `pending` lease has no `expires_at`, and a project whose only live leases are `pending`
    has the reading's time as its horizon** (architect, 2026-11-21, for v0.08-T01, on pin P10).
    A host waiting on a placement is then not powered off by its own check while the laptop's
    guard runs, and it stops a grace after the last guard run when the laptop goes away
    (decisions.md, "Words used here"). A `ready` lease that is live has an `expires_at` after the
    reading's time, so a project with a live `ready` lease has that later time as its horizon
    whether or not it also has a `pending` lease.
  - **A project's horizon can move later at each reading while a `pending` lease waits**, since
    the reading's time is the horizon then. No file records it ([The guard's
    files](#the-guards-files)).
- **A note on the guard's send** (D57, architect, 2026-11-21, for v0.08-T01, on pin P12): the
  horizon above is
  not what the guard sends a running host. **The guard sends each running host the later of its
  project's horizon and the host's `LaunchTime`**, so a host started with no lease is not sent a
  time before its own launch, and the host's bound and the guard's grace agree. The send is
  operator.md's ([operator.md, guard](operator.md#guard)); this section's horizon stays as
  defined above.

| A record, at the reading's time | Its state | Live | Its end |
|---|---|---|---|
| Stored `pending` | `pending` | Yes | None |
| Stored `ready`, the time before `expires_at` | `ready` | Yes | None |
| Stored `ready`, the time at or after `expires_at` | `expired` | No | `expires_at` |
| Stored `released`, with an `expires_at` | `released` | No | The earlier of `released_at` and `expires_at` |
| Stored `released`, with no `expires_at` (never `ready`) | `released` | No | `released_at` |
| Stored `unsatisfiable` | `unsatisfiable` | No | None |

**What they read, and what they never do:**
- **They read only the records, the reading's time and, for the horizon, the launch time.** They
  write nothing and read no other file, so the guard and `status -All` agree on every lease at the
  same reading, which is why D63 gave them one home.
- **For a file that is not valid, or that cannot be read, the functions are not called at all: a
  program MUST NOT take that for "no live lease"**, since the guard would then stop a host that a
  live lease explains.
- **A missing lease state file is not "no leases" for the guard or `status -All` either**
  (architect, 2026-11-21, for v0.08-T01, pin P18; D36). A laptop that lost its `leases.json` MUST
  NOT cost a working agent its host. So the functions are not called for a missing file either,
  and:
  - **The guard stops nothing, sends no horizon, logs one failed line and exits 1.**
  - **`status -All` prints its host lines without `Leases` and `Guard`, and says why once.**

  What the guard logs and `status -All` prints are operator.md's words
  ([operator.md, guard](operator.md#guard), [operator.md, status](operator.md#status)). Every other
  reader still takes a missing file for no leases ([The lease state file](#the-lease-state-file)).
- **The records are those of the `<state folder>` the program runs with.** A lease recorded under
  another `AGENTSPACE_STATE_DIR` is not seen, and its host looks unexplained to a guard run with
  the first one ([What moves which files](#what-moves-which-files)). **Pin P4 prevents it** for a
  guard that the keeper starts (architect, 2026-11-21, for v0.08-T01, ruled with P10 and P11).
- **A released lease's own `expires_at` adds nothing to the horizon**: only a live lease's does.

**An example,** every time invented, for one project whose host record exists:

| Records of the project | At 09:45 | At 10:05 | At 11:30 |
|---|---|---|---|
| A: `ready`, `expires_at` 10:00 | Live | Expired, its end 10:00 | Expired, its end 10:00 |
| B: `released`, `released_at` 09:30, `expires_at` 11:00 | Its end 09:30 | Its end 09:30 | Its end 09:30 |
| C: stored `ready`, `expires_at` 08:00 | Expired, its end 08:00 | The same | The same |
| D: `unsatisfiable` | No end | No end | No end |
| E: `released`, `released_at` 09:35, `expires_at` 09:20 | Its end 09:20 | Its end 09:20 | Its end 09:20 |
| **Live leases** | A | None | None |
| **Last lease end** | 09:30 | 10:00 | 10:00 |
| **Horizon** | 10:00, A's `expires_at` | 10:00, the last lease end | 10:00, the last lease end |

At 09:45, A is the project's only live lease, so its host is explained and its horizon is A's
`expires_at`; B's later `expires_at` of 11:00 does not count, since B is released. B ended at its
`released_at`, 09:30, the earlier of the two. E was released at 09:35, after it expired at 09:20,
so its end is 09:20, and not 09:35. At 10:05 and after, no lease is live, so the host is
unexplained and the horizon is the last lease end, 10:00. **A project with no lease at all has no
last lease end**, and its horizon is its host's launch time.

**A second example,** for one project with two records:

| Records of the project | At 09:50 | At 10:10 |
|---|---|---|
| F: stored `pending`, no `expires_at` | Live, no end | Live, no end |
| G: `released`, `released_at` 09:10, never `ready`, so no `expires_at` | Its end 09:10 | Its end 09:10 |
| **Live leases** | F | F |
| **Last lease end** | 09:10 | 09:10 |
| **Horizon** | 09:50, the reading's time | 10:10, the reading's time |

F is the only live lease and has no `expires_at`, so the horizon is the reading's time at each
reading. G, a lease released while it was `pending`, ended at its `released_at`. Once F is placed
and is `ready`, its `expires_at` is the horizon.

## The host registry

**`%USERPROFILE%\.agentspace\hosts.json` says which host serves which project (D45, D55).** It
holds one record per host: its project, its stack's name, its state, its instance ID once known,
the start time of its last provisioning run, and, from v0.09, the instance type its stack was
last deployed with (D67). It is laptop state, never output (D1): no lease command prints
anything from it, nor names it in a message. Its schema is
[`schemas/state/hosts.state.schema.json`](../schemas/state/hosts.state.schema.json), draft 2020-12
and strict (D20), carried inside every program that reads it, and where this section and the
schema disagree, both are wrong and MUST be fixed together.

**The document** is `{"version":0,"hosts":[<record>, ...]}`:
- **`version`** is the integer `0`. A reader MUST refuse any other value as an invalid file.
- **`hosts`** is an array of records, in the order they were added. The box's record, which the
  seed (below) puts first, stays first.

**A record** is an object with these members, and no others:

| Member | Required | Value |
|---|---|---|
| `project` | Yes | The project the host serves, in the lease contract's normalized form ([lease-api.md](lease-api.md#project)): `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$(?!\n)` |
| `stack` | Yes | The host's CloudFormation stack (D44): `agentbox` for the box, `agentspace-host-<project>` for every other host, so at most 48 characters, 16 for the prefix and 32 for the project (D44; architect, 2026-11-20, for v0.07-T01). `^(agentbox\|agentspace-host-[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?)$(?!\n)` |
| `state` | Yes | `creating`, `ready` or `failed` ([Its states](#its-states)) |
| `instance_id` | No | The host's EC2 instance ID, cached from its stack's `InstanceId` output: `^i-([0-9a-f]{8}\|[0-9a-f]{17})$(?!\n)`. Absent until a writer has read it ([The instance ID](#the-instance-id)) |
| `instance_type` | No | The instance type of the host's stack, its `InstanceType` parameter as `cloudformation describe-stacks` read it back after the last `deploy` that recorded it (D67, D71): `^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*$(?!\n)`, at most 64 characters, the form operator.md gives `-InstanceType`'s value ([operator.md, Command line](operator.md#command-line); architect, 2026-11-23, for v0.09-T01, pin P14). Absent until a `deploy` of v0.09 or later has recorded it, and while a `deploy` that changes it is under way ([The instance type](#the-instance-type)) |
| `last_attempt_at` | No | The start time of the host's last provisioning run, UTC, in the contract's form `YYYY-MM-DDTHH:MM:SSZ` ([lease-api.md](lease-api.md#times)). Absent while no run has been started |

`(?!\n)` after `$` means the value MUST NOT end in a line feed, which a bare `$` would allow; each
pattern in the schema ends that way, as the contract's output schemas' do. The `\|` in the table
is a `|` in the pattern.

- **An optional member is absent or holds a value of its form. `null` is never accepted** for one:
  a record with `"instance_id":null` is not valid, and a writer that has no value for a member
  leaves it out.
- **A `ready` record need not carry `instance_id` or `instance_type`.** The seed's has no
  `instance_id`, and holds `instance_type` `m9g.xlarge` (the seed, below). A record
  written before v0.09 has no `instance_type`, and is valid (D67).
- **`last_attempt_at` is kept as the string it is stored as**, as the lease state file's times
  are. **A time that matches the pattern but names no real instant**, such as
  `2026-02-30T00:00:00Z`, **makes the file not valid**, as it makes the lease state file not valid
  ([The lease state file](#the-lease-state-file)); so does a leap second, `:60`.

**What makes a file not valid, beyond its schema.** A file that validates against the schema is
still not valid when any of these holds, though the schema cannot refuse it:
1. **Two records share a `project`.** Projects are compared as the normalized strings they are
   stored as. This is the lease state file's rule for two records with one `id`, and a reader
   treats such a file as it treats one that fails its schema (below).
2. **A record's `stack` is `agentspace-host-<p>` with `<p>` other than that record's own
   `project`**, such as `{"project":"agentspace","stack":"agentspace-host-swimcoach",...}`. The
   project inside the stack's name MUST equal the record's `project`, exactly.
3. **The file holds no record whose `stack` is `agentbox`, holds two, or holds one whose
   `project` is not `swimcoach`.** A valid file holds exactly one record whose `stack` is
   `agentbox`, and that record's `project` is `swimcoach` (architect, 2026-11-20, for v0.07-T01).
   The box is swimcoach's host (D43, D55), and the seed is its record; nothing in agentspace
   removes a record, so only a person's edit can break this. Without the rule a missing box record
   would read as a swimcoach with no host.

**Its bytes:** the lease state file's, in every point ([The lease state file](#the-lease-state-file)).
- **Written as UTF-8 without a byte order mark, as compact JSON with no whitespace outside strings,
  followed by one LF.** A writer writes each record's members in the order of the table above; a
  reader MUST NOT depend on member order.
- **Read tolerantly (D20):** a reader MUST accept a leading UTF-8 byte order mark and any amount of
  JSON whitespace. Bytes that are not valid UTF-8 make the file not valid.
- **An object that names one member twice makes the file not valid**, whichever value a parser
  would keep. A `version` written as `0.0` is form only, accepted, and written back as `0`.
- **Checked against the schema, and against the three rules above, on every read and before every
  write.** A change that would not be valid is not written.

**The seed: a missing file reads as the box's record alone** (D45, D55), and from v0.09 that
record holds the box's instance type, `m9g.xlarge` (D66's change note; architect, 2026-11-23, for
v0.09-T01, pin P2c):

```
{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","instance_type":"m9g.xlarge"}]}
```

- **A reader that finds no file reads that document**, and writes nothing. Reading never creates
  the file.
- **The first change writes the seed's record with it** (D45, D55): under
  [the host registry lock](#the-host-registry-lock), a writer that finds no file starts from the
  seed, makes its change, and writes the whole document. So the box's record is first in every
  file agentspace writes, and it holds `"instance_type":"m9g.xlarge"` as the seed does until a
  `deploy` of the box changes it ([The instance type](#the-instance-type)). **That the document
  written on the first change carries the seed's `instance_type`, as the document read does, is
  architect's to confirm** (for E0.09-T01-S02, pin P26): the seed is the box's record (rule 3 above), so a
  laptop with no registry reads and writes one record, and a lease of the box placed after another
  project's first lease meets `m` as one placed before it does.
- **`m9g.xlarge` is D65's `m`**, the template's default and the box's type when v0.09 was laid out
  (D65), so a laptop with no registry places a lease of the default size on the box as before
  v0.09. The box's first `deploy`, or the deploy step of its first provisioning run, writes the
  registry with the type the stack reports (P2c). A file that exists keeps its records as they
  are: a box record written before v0.09 has no `instance_type`, and gains one by the box's next
  `deploy` (D67).

**Only a missing file reads as the seed. A file that is not valid is an error, never a missing
file** (architect, 2026-11-20, for v0.07-T01). It holds state that a rebuild from the seed would
lose, such as a host that is `creating`, so a reader MUST NOT read it as the seed:
- **`create`** fails with `internal` and changes nothing, as for a lease state file that is not
  valid; its message is [driver.md](driver.md#where-it-places-a-lease)'s. That `internal` is
  `create`'s alone.
- **`get` of a `pending` lease never fails over the host registry**, nor over the project list: it
  prints the lease as it would otherwise, and starts no provisioning run
  ([driver.md](driver.md#check_pending); architect, 2026-11-20, for v0.07-T01). `get` of a lease
  in any other state does not read the file.
- **The operator's commands and the ProxyCommand** fail with a message that names the file
  ([operator.md](operator.md#which-host-a-command-acts-on)).
- **The keeper** logs that it cannot read the file and keeps every holder as it is, as for its
  other inputs ([keeper.md](keeper.md)).
- **`list`, `renew` and `release` never read it**, so none of them fails because it is missing,
  cannot be read or is not valid.
- **Nothing in agentspace repairs, rewrites or removes a file that is not valid.** A person
  decides.

**Something at `hosts.json` that is not a file, such as a folder, or a file that cannot be opened,
is not a missing file.** Every reader treats it as a file that is not valid, above.

**Replaced atomically** ([Replacing a file](#replacing-a-file)), through
`hosts.json.<hex>.tmp` in `%USERPROFILE%\.agentspace`, written through to the disk and flushed
before the rename. A writer MUST hold [the host registry lock](#the-host-registry-lock) from before
it reads the file until after the rename, and MUST change only the record or records its change is
about, leaving every other record's members and values as it read them. The writer creates
`%USERPROFILE%\.agentspace` first if it is missing.

**Who writes it**, each under its lock and each only as [Its states](#its-states) and
[The instance ID](#the-instance-id) say:
- **The driver**, in `create` and in `get`'s `check_pending`: it adds a listed project's record and
  records the start of a run ([driver.md](driver.md#where-it-places-a-lease)). `get` writes nothing
  else, and never the lease state file.
- **`provision`**: the run's start, its outcome and the instance ID
  ([operator.md](operator.md#provision)); it refuses a project with no record that is not on [the project list](#the-project-list), and writes nothing ([operator.md](operator.md#provision)).
- **Every other operator command that needs a host's instance ID**: that record's `instance_id`
  alone, but for `deploy`, which writes `instance_type` too (the next item). **The guard is the
  exception: it never writes the registry**, though it is an operator
  command (D56 has it read the registry and EC2's state; architect, 2026-11-21, for v0.08-T01,
  pin P18; [The guard's files](#the-guards-files)). **No other operator command adds a record** (architect, 2026-11-20, for v0.07-T01): one given `-Host`
  for a project with no record fails, writing nothing, with operator.md's message
  ([Which host a command acts on](operator.md#which-host-a-command-acts-on)).
- **Every `deploy` that names or implies a host, `provision`'s deploy step included**: that
  record's `instance_type`, which a `deploy` that passes `InstanceType` removes before
  `cloudformation deploy` runs, and which every `deploy` writes after `cloudformation deploy`
  succeeds, as [The instance type](#the-instance-type) says (D67, D71; architect, 2026-11-23, for
  v0.09-T01, pin P11), beside the `instance_id` it already writes. It is the only writer that
  changes `instance_type`.
- **The ProxyCommand**: a missing `instance_id` alone, into its host's existing record, as
  [The instance ID](#the-instance-id) says. It never writes another member and never adds a
  record.

**Every writer but `deploy` keeps a record's `instance_type` exactly as it read it** (D71's pin
names `deploy` alone as its writer; architect, 2026-11-23, for v0.09-T01, pin P12). The driver,
`provision` outside its deploy step, an operator command that caches an instance ID, and the
ProxyCommand never add, change or remove it, in a record they write `creating`, `failed` or
`ready` alike. A record the driver or `provision` adds has none; the box's record that a first
change writes from the seed holds the seed's. **A program that cannot keep the member, because it
does not know it, MUST NOT write the file** (P12): a program built before v0.09 refuses a file
that holds `instance_type` as not valid, by its schema, and so never drops it, and a program built
against this page MUST read it and write it back unchanged ([The instance type](#the-instance-type)).

**Who reads it:** the driver, in `create` and in `get` of a `pending` lease, its place reading
`instance_type` from v0.09 ([The instance type](#the-instance-type)); every operator
command that acts on a host, to find it, and `status -All`, `down -All` and `sync -All` for
every record; the guard, for every record, read-only and without its lock (architect, 2026-11-21,
for v0.08-T01, pin P18); and the
ProxyCommand and the keeper, without its lock
([Which host a connection reaches](#which-host-a-connection-reaches)), but for the ProxyCommand's
write of a missing `instance_id`, which it makes under the lock. **`list`, `renew` and
`release` never read it.** No program prints it, or any record's members, to a lease command's
output (D1).

- **`AGENTSPACE_STATE_DIR` does not move it**, nor its lock: every state folder on the machine
  shares one host registry, because hosts are shared ([What moves which files](#what-moves-which-files)).
- **Access rules:** inherited.
- **No command deletes the file, and nothing in agentspace removes a record from it** in v0.07: a
  host is kept (D25).

### Its states

**A record's `state` says whether the host can take leases**, and is written only as this table
says. When a run is started, and the 5-minute rule between runs, are
[driver.md](driver.md#check_pending)'s and [operator.md](operator.md#provision)'s; this table is
what each writer records.

| From | To | Written by, and when |
|---|---|---|
| No record | `creating`, without `last_attempt_at` unless a run is started | The driver, in `create` or in `get`'s `check_pending`, for a project with no record that is on [the project list](#the-project-list) (architect, 2026-11-20, for v0.07-T01). When it starts a run in the same call, the record is written with `last_attempt_at` in that same write (the next row). `provision`, run by hand for such a project, adds the same record with `last_attempt_at` |
| `creating` or `failed` | `creating`, with `last_attempt_at` set to now | The driver, in the same locked write in which it decides to start a run; `provision`, at its start, once it holds [the run's lock](#the-provisioning-runs-lock) (architect, 2026-11-20, for v0.07-T01) |
| `creating` | `ready`, with `instance_id` | `provision`, when every step has succeeded. `instance_type` is as its deploy step recorded it ([The instance type](#the-instance-type)) |
| `creating` | `failed` | `provision`, when a step has failed. `last_attempt_at` stays the run's start, and an `instance_id` the run learned is kept |
| `ready` | `ready`, with `last_attempt_at` set to now | The driver, when it starts a run for a host that is `ready` (`check_pending`, for a pending lease of that project; architect, 2026-11-20, for v0.07-T01), and `provision` at its start on such a host. **The run never changes a `ready` record's `state`**: it keeps `ready` on success, writing the `instance_id` if it learned one, and on a failed step logs the failure and leaves `ready` (architect, 2026-11-20, for v0.07-T01; D48) |

- **No writer changes a record to a state the table does not name.** In particular nothing in
  agentspace moves a record out of `ready`, and nothing removes a record. **`creating` and
  `failed` apply only to a host that was never `ready`**, and a run on a `ready` host, and the
  driver that starts it, write only `last_attempt_at` at its start (architect, 2026-11-20, for
  v0.07-T01; D48). A host that is not `ready` sends its project's ready leases to the box, where
  ssh refuses them ([Which host a connection reaches](#which-host-a-connection-reaches)).
- **`instance_type` is not a state, and no row of the table writes it.** A `deploy`,
  `provision`'s deploy step included, writes it whatever the record's state, and every write the
  table names keeps it as it was ([The instance type](#the-instance-type)). A run on a `ready`
  host, a resize included (D68, D71), keeps the record `ready` and writes only
  `last_attempt_at` at its start, as above.
- **`creating` means a run is under way, or one died with the record still `creating`.**
  Whether a run is under way is told by [the run's lock](#the-provisioning-runs-lock), never by
  the record: a run killed before it recorded its outcome leaves `creating`, and a later `create`
  or `get` starts the next run once `last_attempt_at` is 5 minutes old (D48).
- **`failed` is never a reason to answer `unsatisfiable`** (D48): a lease of that project stays
  `pending`, and the next run MAY succeed.
- **The only record with the stack `agentbox` that agentspace ever adds is the seed's.** Every
  record a writer adds has the stack `agentspace-host-<project>`, with its own project.

### The instance ID

**A record's `instance_id` is a cache of its stack's `InstanceId` output** (D45), and replaces the
instance-ID cache, `instance-id.json`, which v0.07 retires
([Files in the checkout](#files-in-the-checkout)). Nothing migrates: a record without one has it
read from its stack again, as a missing cache was.
- **An operator command that needs a host's instance ID and finds its record without one reads the
  stack's `InstanceId` output and writes it into that record**, under the lock, changing nothing
  else. `deploy` writes it from the output after every deploy, replacing the one the record held,
  and so does `provision`'s deploy step ([operator.md](operator.md#deploy)).
- **The ProxyCommand MAY write a missing `instance_id` into its host's existing record, and
  nothing else** (architect, 2026-11-20, for v0.07-T01), as the retired cache was written: when the
  record it reaches holds no `instance_id`, it reads the stack's output, then takes
  [the host registry lock](#the-host-registry-lock), reads the file again, and writes the ID only
  into that record, and only if it still holds none. It never writes another member, never
  replaces an `instance_id`, and never adds a record; it never holds the lock across the call to
  AWS. A write that fails, or a file that has become not valid, does not fail the connection,
  which uses the ID it read (architect, 2026-11-20, for v0.07-T01).
- **The lease CLI never reads, checks for or needs `instance_id`** (D21): `create` and `get` read
  a record's `project`, `stack` and `state`, and from v0.09 its `instance_type`, and nothing else
  ([driver.md](driver.md#where-it-places-a-lease); [The instance type](#the-instance-type)).
- **An `instance_id` that does not match its pattern makes the file not valid**, as above. Unlike
  the retired cache's, it is never read as missing, because the file holds more than the cache did.

### The instance type

**A record's `instance_type` records the instance type of its host's stack, as the last `deploy`
that recorded it left the stack** (D67; D71's pins). The stack's `InstanceType` parameter is the
instance's type, and it changes only through the stack (D39, D40, D67). The record is how the EC2
driver's place learns a host's size with no AWS call (D48). The member's form is in
[the table above](#the-host-registry); this section says who writes it, when, and what it means.

**Who writes it, and when** (D71's pin; D67's change note; architect, 2026-11-23, for v0.09-T01,
pin P11):
- **Every `deploy` that names or implies a host, `provision`'s deploy step included**:
  `deploy -Host <project>`, `deploy` of the box with `-Host swimcoach` or with no `-Host`, and the
  deploy step of every provisioning run, a resize included
  ([operator.md, deploy](operator.md#deploy); [operator.md, provision](operator.md#provision)).
  Nothing else changes it (P12).
- **Its value comes only from the stack after `cloudformation deploy` succeeds**: the
  `InstanceType` parameter of the host's stack as `cloudformation describe-stacks` reads it back,
  stored exactly as that call returns it, never `-InstanceType`'s value and never an older record.
- **In these steps, each write under [the host registry lock](#the-host-registry-lock)**, which the
  writer takes, re-reading the file under it, and releases after the rename, changing that record's
  `instance_type` and no other member but what operator.md's step writes in the same write:
  1. **A `deploy` that passes `InstanceType` first removes the record's `instance_type`, before
     `cloudformation deploy` runs** (P11), whatever the record held, the type it passes included.
     A `deploy` that passes no `InstanceType` writes nothing here, and keeps the type until its own
     read-back writes it.
  2. **Once `cloudformation deploy` has succeeded, `deploy` removes the record's `instance_id`, and
     `instance_type` with it in the same write**, as it already removes `instance_id`
     ([The instance ID](#the-instance-id)). That this removal takes `instance_type` too, for a
     `deploy` that passed no `InstanceType`, is architect's to confirm (for E0.09-T01-S02, pin
     P27): it is how P11's "a failed or malformed read leaves the record with no type" holds for
     every `deploy`.
  3. **It reads the `InstanceType` parameter with `describe-stacks`**, holding no lock of this
     page's, but for a provisioning run's own lock in its deploy step (P11). One call reads both
     `InstanceId` and `InstanceType` (architect's to confirm, pin P23).
  4. **It writes the value it read into the record**, in the same write as the `instance_id`
     (P23).

  So the type comes from the stack as the deploy left it, never from an older record, as the
  instance ID does ([The instance ID](#the-instance-id)), and the record never holds a type from
  before a change the deploy asked for.
- **A `deploy` that passed `InstanceType` and fails or is killed anywhere after step 1, or whose
  read fails or answers a value not of the member's form, leaves the record with no
  `instance_type`** (P11). A `cloudformation deploy` whose client is killed goes on running on
  AWS's side, so a type kept from before could be one the host no longer has, larger than it, and
  place would answer `ready` on a host smaller than the lease asked for, which D5 forbids. A
  missing type costs one run, never a stale type (D67's change note).
- **A `deploy` that passed no `InstanceType` and fails, or is killed, before its step 2 leaves the
  record's `instance_type` as it was**: it asked for no change of type, and an existing stack keeps
  its `InstanceType`'s previous value, so the deploy changed nothing the record says. Once its
  step 2 has run, a failed or malformed read leaves no type, as above.
- **A record left with no type meets no size** (below): the next lease of that project answers
  `pending`, whatever other leases hold the host, and its run records the type (D66's change note;
  [driver.md, The EC2 driver](driver.md#the-ec2-driver)). A failed deploy step of a provisioning
  run is logged in the host's [`provision.log`](#the-provisioning-runs-log), and the project's
  leases stay `pending` for the next run (D68). What `deploy` prints and logs is operator.md's.

**Which type a deploy passes is operator.md's** ([operator.md, deploy](operator.md#deploy),
[operator.md, provision](operator.md#provision)); this page says only which files it reads to find
it:
- **`deploy -InstanceType <type>` passes that type, for every host, the box included** (D67, D71).
  A `deploy` without it passes the stack's previous value, or the template's default for a host's
  stack it creates. Neither reads `stack-parameters.json`'s `InstanceType`
  ([Files in the checkout](#files-in-the-checkout)).
- **A provisioning run's command line is unchanged, `provision -Host <project>`, and the run finds
  its own type at its deploy step** (D71, which changes D68). It reads the lease state file of the
  `<state folder>` it runs with ([The provisioning run's start](#the-provisioning-runs-start)) once,
  at its deploy step, and:
  - **When a live `ready` lease of the project holds the host at that read, it passes no
    `InstanceType`** (D68's change note; architect, 2026-11-23, for v0.09-T01, pin P2(a)), so the
    stack's change set holds no resize and the host is not stopped. Its deploy still records the
    type it reads back, and `place_pending` then places or refuses the project's `pending` leases
    by that type (pin P1; [driver.md, place_pending](driver.md#place_pending)). Whether a lease is
    live and `ready` is decided by one reading's time, as
    [Live leases and the lease horizon](#live-leases-and-the-lease-horizon) decides it.
  - **Otherwise it passes the type the EC2 driver's `resize_target` gives for its project from
    that file's records, or none when it gives none** (D71's pin;
    [driver.md, The EC2 driver](driver.md#the-ec2-driver)). `resize_target` counts only the
    project's `pending` records whose `driver` is the EC2 driver's name and whose size is in
    `SIZES`, the leases a run would place (pins P18 and P20).

  The driver starts the run as before, with no new argument, so a run that `get` starts and one
  that `create` starts agree. **A resize runs provision's steps entire** (D71), each safe to
  repeat (D48), and **a run never takes a `ready` host out of `ready`** (D48, D68).
- **The run reads the lease state file read-only, without [the lease state
  lock](#the-lease-state-lock)**, as the guard does, holding its own
  [`provision.lock`](#the-provisioning-runs-lock) and no other lock (architect, 2026-11-23, for
  v0.09-T01, pin P13). It sees the file as it was before a change or as it is after it
  ([Replacing a file](#replacing-a-file)), and never makes a lease command wait. A lease created
  after that read is not in the run's type, and is placed or answered by the driver: while the
  project has a `pending` lease a run would place, place answers every new lease of the project
  `pending` (D66's change note; pins P2(b) and P20), and the run's `place_pending` places it if the
  host the run leaves meets it, answers it `unsatisfiable` with `no_capacity` when a live `ready`
  lease holds the host, and otherwise leaves it `pending` for the next run (pin P1).
- **A missing lease state file is no leases** (P13), as for every reader but the guard and
  `status -All` ([The lease state file](#the-lease-state-file)): no lease holds the host,
  `resize_target` gives none, and the deploy step passes no type. **A file that is not valid, or
  that cannot be read, MUST NOT be taken for no leases** (P13): the run's deploy step fails before
  it calls AWS, as a failed deploy step does, changing no record's `instance_type`, and the leases
  stay `pending` ([operator.md, provision](operator.md#provision) gives the log line). Taking it
  for no leases could resize a host a `ready` lease holds; failing first spends no deploy.

**Who reads it, and what it means:**
- **The EC2 driver's place reads it from the record**, with the rest of the record, under the host
  registry lock as `create` already reads it, and makes no AWS call (D48, D66, D67). Whether a host
  meets a size is the driver's `meets`, by the map in
  [driver.md, The EC2 driver](driver.md#the-ec2-driver). This page owns the member, never the map
  from size to type (D67).
- **A record with no `instance_type` meets no size** (D67, D71). No record written before v0.09
  has one, and each is valid; the seed's has `m9g.xlarge` (P2c, above). **A `ready` record with
  no `instance_type` answers `pending` and starts a run, whatever other leases hold the host, never
  `no_capacity`** (D66's change note; architect, 2026-11-23, for v0.09-T01, pin P2): so each
  project's first lease after v0.09's install answers `pending`, and its run records the type
  (D67, D70), resizing only when no live `ready` lease holds the host (P2(a), above). No file of the
  user's is edited for it. What place answers is driver.md's
  ([driver.md, The EC2 driver](driver.md#the-ec2-driver)).
- **A type that matches the member's form but that the map does not hold**, such as one a person
  set by hand, is valid in the file, and meets no size (D71's `meets`).
- **It never says whether the host runs** (D45). A record holds the type of a stopped host as it
  holds a running one's, and nothing in the registry records whether a host runs.
- **No lease command prints it** (D1), and nothing in a lease names an instance type.

**Its form, and programs built before v0.09:**
- **The member is optional, and `null` is never accepted for it**, as for every optional member
  ([The host registry](#the-host-registry)).
- **It is written after `instance_id` and before `last_attempt_at`**, the order of the member
  table, which fixes a record's bytes (architect, 2026-11-23, for v0.09-T01, pin P4). A record
  without it is written as it was before v0.09, since an absent member adds no bytes. A reader
  MUST NOT depend on member order.
- **The file's `version` stays `0`** (architect, 2026-11-23, for v0.09-T01, pin P15). An optional
  member gives an older reader nothing to misread: a program built before v0.09 refuses a file
  that holds `instance_type` as not valid, by its schema, rather than reading it without the
  member, and so never writes such a file back without it.
- **So v0.09's programs MUST be installed together**, since a program of v0.08 left beside them
  fails on every registry that a `deploy` of v0.09, or a first change from the seed, has written
  the member into. The install is operator.md's
  ([operator.md, The install folder](operator.md#the-install-folder)). **A return to v0.08's
  programs after v0.09 has written the member needs it removed by hand** from every record that
  holds it (D67's change note; P15; [README](../README.md#build)).

### Which host a connection reaches

**The host a lease is on, and so the host a connection through its alias reaches, is found from
the files on this page** (D49, D55; architect, 2026-11-20, for v0.07-T01, the dev lead's pin P7):
- **A host's operator alias, `agentspace-host.<project>`, reaches that project's host**, whatever
  its record's state.
- **A lease alias reaches the host of the lease whose `ssh.host_alias` it is**: the record in the
  lease state file with that alias, then its `project`'s record in the host registry, if that
  record is `ready`. **Otherwise, the box**: when no lease record has that alias, when the
  project has no record, or when its record is not `ready`.
- **A ProxyCommand given no alias reaches the box**, as the user's own `agentspace` entry does.
- **A fallback to the box is refused by ssh** for a lease whose host is not the box, because its
  alias checks the key under `agentspace-host.<project>` in agentspace's `known_hosts`, and the box's
  key is never there under that name
  ([A lease alias for a host that is not the box](#a-lease-alias-for-a-host-that-is-not-the-box)).
  So a connection never reaches the wrong machine.
- **The ProxyCommand and the keeper read the lease state file and the host registry without their
  locks.** Neither writes the lease state file, the keeper never writes the host registry, and the
  ProxyCommand writes it only to cache a missing `instance_id`, under its lock
  ([The instance ID](#the-instance-id)).

## The project list

**`%USERPROFILE%\.agentspace\projects.json` lists the projects that may have a host made (D54,
D55).** A host costs money while it exists, so a mistyped project MUST NOT make one. It governs only
making a host: a project that has a record in [the host registry](#the-host-registry) is placed by
that record, listed or not. Its schema is
[`schemas/state/projects.state.schema.json`](../schemas/state/projects.state.schema.json), draft
2020-12 and strict (D20), carried inside every program that reads it, and where this section and
the schema disagree, both are wrong and MUST be fixed together.

**The document** is `{"version":0,"projects":[<entry>, ...]}`:
- **`version`** is the integer `0`. A reader MUST refuse any other value as an invalid file.
- **`projects`** is an array of entries, each an object with exactly these two members:

| Member | Value |
|---|---|
| `name` | The project, in the lease contract's normalized form: `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$(?!\n)` |
| `approved` | The date Terry approved it, `YYYY-MM-DD`: `^[0-9]{4}-(0[1-9]\|1[0-2])-(0[1-9]\|[12][0-9]\|3[01])$(?!\n)` |

- **A project is listed when an entry's `name` equals the lease's normalized project, exactly.** A
  reader MUST NOT normalize a stored name: one that is not in the normalized form fails the schema.
- **A date that matches the pattern but names no real day**, such as `2026-02-30`, `2026-04-31`
  or `2026-02-29`, **makes the file not valid**, as a time that names no real instant makes the
  lease state file not valid. Days are counted in the proleptic Gregorian calendar.
- **Nothing in agentspace compares `approved` with the clock.** It records when Terry's yes was
  given; a date in the future is valid.
- **Two entries with one `name` make the file not valid**, though the schema cannot refuse it, as
  two host records with one project do.
- **An object that names one member twice makes the file not valid.**

**Who writes it: a person, or the architect with Terry's yes, never agentspace** (D54). Every
entry takes Terry's yes, recorded as its `approved` date. No agentspace program creates, changes
or removes it, and none offers to. For v0.07 it holds `agentspace` alone (D54); swimcoach needs no
entry, since its host is the box.
- **A person writes it, so a reader MUST accept a UTF-8 byte order mark, and whitespace and line
  endings that JSON allows**, as for the stack parameters file. Bytes that are not valid UTF-8
  make the file not valid. For example:

  ```
  {"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20"}]}
  ```

  The architect writes it as compact JSON followed by one LF.
**Who reads it, and when:** the driver, in `create` and in `get`'s `check_pending`, and
`provision`, each only for a project with no record in the host registry
([driver.md](driver.md#where-it-places-a-lease); [operator.md](operator.md#provision)). A reader
reads it while it holds [the host registry lock](#the-host-registry-lock), so its answer and the
record it adds agree. A reader SHOULD open it sharing read, write and delete.

**A missing file lists nothing** (D54), so a laptop without one makes no host at all. **Something at
its path that is not a file, a file that cannot be opened, or a file that is not valid, lists
nothing either, but is an error** (architect, 2026-11-20, for v0.07-T01): `create` fails with `internal` and
changes nothing, rather than answer a terminal `unsatisfiable` for a project that MAY be listed;
`provision` fails with a message that names the file ([operator.md](operator.md#provision)).
**`get` of a `pending` lease never fails over it** (architect, 2026-11-20, for v0.07-T01): it
starts no run and answers as it would otherwise.

- **`AGENTSPACE_STATE_DIR` does not move it.** **Access rules:** inherited; agentspace sets none.
- **Never removed by agentspace.**

## A lease's credential

**A ready lease's credential is three files in its lease folder, and its alias is one file in the
`<ssh folder>`'s `agentspace.d`, reached through one `Include` line in that folder's `config`
(D7, D9, D10).** What a caller is promised about them is
[lease-api.md](lease-api.md#the-credential)'s; this section says what the files are, byte for byte
where it matters, and how each step makes and removes them. Everything here is done by the v0
driver ([driver.md](driver.md)), inside the lease command that calls it, or, from v0.07, inside a
provisioning run that places a `pending` lease through the driver's `place_pending` (D48).

### The lease folder

**`%USERPROFILE%\.agentspace\leases\<id>\`**, one per lease that has a credential, where `<id>` is
the normalized lease ID (D21). It holds:

| File | What it is |
|---|---|
| `id_ed25519` | The private key |
| `id_ed25519.pub` | Its public key |
| `id_ed25519-cert.pub` | The certificate, which ssh finds beside the key by this name |

- **The writer MUST NOT put anything else in it**, except [renewal](#renewal)'s temporary files
  while a `renew` runs. `release` deletes the folder with whatever is in it, a temporary file left
  by a killed `renew` included.
- **`AGENTSPACE_STATE_DIR` does not move it** ([What moves which files](#what-moves-which-files)).
- **Access rules:** the folder's are inherited from `%USERPROFILE%\.agentspace`. The three files'
  are whatever the key tool gives them, unless a writer sets the private key's itself (below); the
  prototype sets none.
- **The private key MUST be OpenSSH-safe ([Access rules](#access-rules)), and MUST NOT be readable
  by any principal but the user, `SYSTEM` and `Administrators`** (lease-api.md: no other ordinary
  user of the machine can read it). How a writer meets that is its choice: it MAY rely on the rules
  its key tool gives the file, or set them itself.
- **A writer that sets the private key's rules itself MUST set exactly these, before it signs:**
  1. **Owner:** the user's SID, the user SID of the process's token.
  2. **Protected:** the DACL is protected from inheritance (`SE_DACL_PROTECTED`), so nothing is
     inherited from the lease folder.
  3. **Three allow entries, and no other entry:** `SYSTEM` (`S-1-5-18`) with full control,
     `Administrators` (`S-1-5-32-544`) with full control, and the user with at least read and write
     (modify, as Windows' `ssh-keygen` gives it, or full control).
  These are the rules `ssh-keygen` was seen to give the file (below), with the owner stated.
- **A writer SHOULD check the private key's rules before it signs**, whichever way it set them. If
  it cannot set them, or its check finds a principal other than those three that can read the key,
  `create` MUST fail with `internal` and leave nothing, as for any failed credential step
  ([Creation, in order](#creation-in-order)).
- **The public key and the certificate are not secret, and MAY be readable by everyone.**
- **What the prototype leaves, seen on 2026-11-18 with Windows' own `ssh-keygen`
  (OpenSSH_for_Windows 9.5p2, Windows 11 build 26300):** the prototype sets and checks no rules of
  its own, and `ssh-keygen` gives each file it writes a protected list of its own, whatever the
  folder passes down:

  | File | Its rules, none inherited |
  |---|---|
  | `id_ed25519` | `SYSTEM` and `Administrators` full control; the user modify |
  | `id_ed25519.pub`, `id_ed25519-cert.pub` | The same, and `Everyone` read |

  So the private key meets the requirement on that build, and Windows OpenSSH accepts it. Whether
  every Windows build does the same is [Q6](#open-questions).

### The key

- **An ed25519 key pair in OpenSSH's own formats**, as `ssh-keygen -t ed25519` writes them: the
  private key in the `-----BEGIN OPENSSH PRIVATE KEY-----` format, and the public key as one line,
  `ssh-ed25519 <base64> <comment>`. Any tool that writes these formats MAY make it. Windows'
  `ssh-keygen` ends the public key's line in CR LF and the private key's lines in LF; **a reader
  MUST accept either line ending in both.**
- **The private key MUST have no passphrase:** it MUST be stored unencrypted, which in OpenSSH's
  private key format means its cipher name and its KDF name are both `none`, so that any OpenSSH
  client loads it without asking for one ([lease-api.md](lease-api.md#the-credential)). **The
  writer MUST check that the key loads with an empty passphrase before it signs**, and fail the
  `create` if it does not.
- **Its comment SHOULD be `agentspace lease <id>`**, and MUST hold no line break. Nothing reads the
  comment.
- **A new key MUST NOT overwrite an existing one.** `create` MUST fail, changing nothing, when the
  lease folder already holds `id_ed25519` or `id_ed25519-cert.pub`; lease IDs are random UUIDs, so
  that happens only after a failure left files behind, which [creation](#creation-in-order) does
  not allow. A lease folder that exists without either file is used as it is. If making or signing
  the key then fails, the prototype keeps that folder with whatever the failed step left in it,
  and if a later step fails, it deletes the folder ([driver.md](driver.md#place-a-lease)).
- **`renew` MUST NOT replace the key.** It changes only the certificate.

### The certificate

**An OpenSSH user certificate for the lease's public key, signed by [the CA](#the-ca)**, written
as one line, `ssh-ed25519-cert-v01@openssh.com <base64> <comment>`, which Windows' `ssh-keygen`
ends in CR LF; a reader MUST accept CR LF or LF. Any tool that writes this format MAY make it; its
content MUST be exactly this:

| Field | Value |
|---|---|
| Certificate type | User (`SSH_CERT_TYPE_USER`, 1), never host |
| Public key | The lease's ed25519 public key, as in `id_ed25519.pub` |
| Serial | 0 in v0; not part of the contract ([lease-api.md](lease-api.md#the-credential)) |
| Key ID | The lease's normalized ID, so the host's log names the lease (D7) |
| Valid principals | Exactly one: the lease's `ssh.user`, `ubuntu` on today's box |
| Valid after | MUST NOT be later than the issue time, and SHOULD be 5 minutes before it (D24, #0029; [lease-api.md](lease-api.md#the-credential)). The back-dating never moves the valid-before time |
| Valid before | The issue time plus the TTL, exactly: the lease's `expires_at` |
| Critical options | None |
| Extensions | Exactly two: `permit-port-forwarding` and `permit-pty`, each with empty data. No `permit-agent-forwarding`, `permit-X11-forwarding` or `permit-user-rc` |
| Signature key | The CA's ed25519 public key; the signature is `ssh-ed25519`, made with the CA's private key |

- **The issue time is the signing step's reading of the clock in UTC, truncated to the whole
  second**, and the TTL is a whole number of seconds, so the valid-before time is a whole second
  and is recorded unchanged as `expires_at`. The writer MUST know that time exactly, so it MUST set
  the window as absolute UTC times, never as a time relative to the signer's own clock.
- **Its comment** is any text with no line break, and nothing reads it. It SHOULD be the public
  key's comment, `agentspace lease <id>`, which is what `ssh-keygen` writes.
- **The prototype's valid-after time is the issue time itself**, which meets the MUST and not the
  SHOULD ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
- Note on where this came from: the prototype signs with Windows OpenSSH's `ssh-keygen -s <CA key>
  -I <id> -n ubuntu -V <after>:<before> -O clear -O permit-pty -O permit-port-forwarding
  <public key>`, where `<after>` and `<before>` are `YYYYMMDDHHMMSSZ`. `-O clear` drops the default
  extensions before the two are added. `ssh-keygen -L` prints the window in the machine's local
  time with no zone, so a check that reads it that way MUST convert before comparing it with a UTC
  time.
- **`ssh-keygen` writes a certificate beside the public key it signs**, as `<name>-cert.pub` for
  `<name>.pub`. That is why the prototype's [renewal](#renewal) signs a copy under a temporary
  name.

### The alias file

**`<ssh folder>\agentspace.d\<alias>`, where `<alias>` is the lease's `ssh.host_alias`,
`agentspace-<project>-<first 8 hex digits of the id>`** ([lease-api.md](lease-api.md#fields)). One
file per lease, holding one `Host` block. The alias MUST match `^[A-Za-z0-9._-]+\z`, case-sensitively,
and MUST NOT start with `-`, because the ProxyCommand puts it on a command line
([operator.md](operator.md#the-proxycommand)); every alias `create` generates does.
**Three rules check an alias, and they stay three** (architect, 2026-11-19, for v0.06, gap FB-23):
this one; the operator's `-Alias` check, which is the same pattern
([operator.md, Command line](operator.md#command-line)); and the holder record's, which is stricter,
a letter or digit first and at most 128 characters ([The holder record](#the-holder-record)). Each
guards its own input. Every alias `create` generates passes all three, so they differ only for an
alias a person writes by hand, which reaches the operator alone, never the keeper's record.

**`agentspace.d` holds a second kind of file from v0.07: a host's operator alias,
`agentspace-host.<project>`** ([A host's operator alias](#a-hosts-operator-alias); D47, D55). Its
name passes the alias pattern above and the operator's `-Alias` check, and holds a dot, which no
lease alias does: a lease alias is `agentspace-`, a project of letters, digits and `-`, `-` and
eight hex digits. So no lease alias ever equals an operator alias's name, and the two kinds are
told apart by their owner marks as well (below).

**Its bytes, exactly, for a lease on the box:** UTF-8 without a byte order mark, each line ending
in one LF, the last included, and every line after the second indented by two spaces. These are
v0.06's bytes, and a lease on the box keeps them from v0.07 on (D47, D53, D55); a lease on any
other host adds one line, as [A lease alias for a host that is not the box](#a-lease-alias-for-a-host-that-is-not-the-box)
says, and is otherwise the same:

```
# agentspace lease <id>; <comment>
Host <alias>
  User <user>
  IdentityFile "<key path>"
  CertificateFile "<certificate path>"
  IdentitiesOnly yes
  HostKeyAlias agentspace
  StrictHostKeyChecking yes
  ProxyCommand <proxy command>
  ServerAliveInterval 30
  ServerAliveCountMax 4
  RemoteForward 127.0.0.1:8811 127.0.0.1:8811
  RemoteForward 127.0.0.1:53147 127.0.0.1:53147
```

| Placeholder | Value |
|---|---|
| `<id>` | The normalized lease ID |
| `<comment>` | Any text without a line break. It SHOULD say that agentspace wrote the file and that it MUST NOT be edited |
| `<user>` | The lease's `ssh.user`; `ubuntu` on today's box |
| `<key path>` | The private key's full path, with every `\` written as `/` and every `%` as `%%`, and nothing else changed: the drive letter and its `:` stay, as in `C:/Users/<user name>/.agentspace/leases/<id>/id_ed25519`. The path is the home folder as [The home folder](#the-home-folder) gives it, made full if it is relative, joined as text with `.agentspace\leases\<id>\id_ed25519`. **"Joined as text"** means: a relative home folder is first made full against the working folder; then any `\` or `/` at its end is dropped, unless it is all that follows the drive (`C:\` stays `C:\`); then the rest is appended after one `\`, which is not added when the home folder already ends in one, so a home of `C:\` gives `C:/.agentspace/...` (architect, 2026-11-19, for E0.05-T01-S05). A `/` inside the home folder's value is kept as given, and becomes `/` like every `\` (architect, 2026-11-19, gap FA-05). A writer MUST NOT otherwise change it: it MUST NOT expand a short (8.3) name, resolve a link, change case or add a `\\?\` prefix |
| `<certificate path>` | The certificate's full path, written the same way |
| `<proxy command>` | The ProxyCommand, with every `%` written as `%%` and nothing else changed: its `\` stay as they are |

- **Both paths are always in double quotes**, so a path with a space needs nothing more. A Windows
  path cannot hold a `"`, so none is ever escaped.
- **The ProxyCommand is the driver's** ([driver.md](driver.md#reachability)), and what it runs is
  [operator.md](operator.md#the-proxycommand)'s. This page owns how it is written into the file,
  and the default line.
- **By default it is the full path of agentspace's ProxyCommand program, in double quotes, then one
  space and the alias** (D8):

  ```
  "<program>" <alias>
  ```

  **When the program is a `.cmd` or `.bat` file**, which Windows runs only through its command
  interpreter, that interpreter comes first (D8):

  ```
  <system folder>\cmd.exe /d /c "<program>" <alias>
  ```

  | Placeholder | Value |
  |---|---|
  | `<program>` | The full path of agentspace's ProxyCommand program, with backslashes. **The lease CLI finds the program in the folder its own executable runs from** (D8), so a CLI that is one compiled program needs no checkout. The program is `agentspace-ssh-proxy.exe` there, or, when that file is not there, `agentspace-ssh-proxy.cmd` ([operator.md, How each program finds the others](operator.md#how-each-program-finds-the-others)). Its extension is compared without regard to case |
  | `<system folder>` | Windows' `System32` folder as a full path: `%SystemRoot%\System32`, with the value of `SystemRoot` written out, which is `C:\Windows\System32` on every machine seen. A `\` or `/` at the end of `SystemRoot`'s value is dropped first, so `C:\Windows\` gives `C:\Windows\System32` (architect, 2026-11-19, for E0.05-T01-S05). The text `%SystemRoot%` itself MUST NOT be written: ssh reads `%` as its own token, and Windows expands no variable in a ProxyCommand. **When `SystemRoot` is unset or empty and the interpreter form is needed, `create` MUST fail with `internal`**, as when the program is not found, saying in agentspace's own words that the lease's way to reach its host cannot be found because Windows' system folder is not known, and MUST NOT guess a folder (architect, 2026-11-19, gaps FA-06 and FB-20). The program's own form needs no `SystemRoot` |
  | `<alias>` | The lease's alias, written literally |

- **If the program is not in that folder, `create` MUST fail with `internal`**, as for any alias
  step that fails ([Creation, in order](#creation-in-order)), and say in agentspace's own words that
  the lease's way to reach its host cannot be found ([lease-api.md](lease-api.md#messages)).
- **A provisioning run's `place_pending` finds `<program>` of the default line as the lease CLI
  does, beside the running program's own executable** (D8; architect, 2026-11-20, for v0.07-T01),
  by the same rule and in the same form as the `<program>` row above says, and nowhere else. The run is
  `agentspace-operator.exe`, which D37 puts beside the lease CLI and the ProxyCommand program in
  every install and in `target\dist`. **If the program is not there, `place_pending` fails for
  that lease as `create` does above, and the lease stays `pending`**
  ([Creation, in order](#creation-in-order)). `AGENTSPACE_PROXY_COMMAND`, when the lease CLI that
  started the run had it set, still replaces the whole line, as the next bullet says
  ([The provisioning run's start](#the-provisioning-runs-start)). **A stand-in named by
  `AGENTSPACE_PROVISION_CMD` never calls `place_pending`.**
- **`AGENTSPACE_PROXY_COMMAND`, when set and not empty, replaces the whole line**, and the program
  is then not looked for ([lease-api.md](lease-api.md#environment)).
- **`create` MUST NOT read, check for or need a host's instance ID**, with or without the
  override: it reads a host registry record's `project`, `stack` and `state` alone, and the
  ProxyCommand reads `instance_id` when a connection is made (D21, D45;
  [The instance ID](#the-instance-id); [driver.md](driver.md#how-it-is-reached)). Until v0.07 this
  rule named the instance-ID cache, which v0.07 retires.
- **What a check can see of the default line, and so all that conformance asserts of it:** it has
  one of the two forms above; `<program>` is the full path of a file that exists; the interpreter
  form is used when, and only when, that file's extension is `.cmd` or `.bat`; and the line ends
  with one space and the alias.
- Note on where this came from: the prototype writes
  `C:\Windows\System32\cmd.exe /d /c "<checkout>\scripts\ssh-proxy.cmd" <alias>`, where
  `<checkout>` is the checkout whose `scripts\lease-cli.ps1` runs the `create`. Its `ssh-proxy.cmd`
  sits beside that script, which is the CLI's code, so it meets the rule above. It writes the
  Windows folder as the fixed text `C:\Windows`, which is `%SystemRoot%` on every machine seen.
- **This page's rule for writing the line also settles [driver.md Q6](driver.md#open-questions)**,
  how an override is escaped: like any ProxyCommand, by the `<proxy command>` row above and the
  line-break rule below.
- Note on where this came from: the prototype's `<comment>` is `written by
  scripts/lease-alias.ps1. Do not edit.`
- **The first line is the owner mark.** It MUST be written exactly as shown: `# agentspace lease `,
  the normalized ID, `;`, one space, then the comment. A reader recognizes an alias file by a first
  line, up to the first LF, that starts with `# agentspace lease `, followed by an ID and a `;`.
  Release and overwrite are decided by it. The prototype matches `# agentspace lease (\S+);` at the
  start of the line, and compares both the prefix and the ID without regard to case; a reader MUST
  accept at least the exact form.
- **A host's operator alias carries another owner mark, `# agentspace host <project>; <comment>`**
  ([A host's operator alias](#a-hosts-operator-alias)), **and it is never a lease's owner mark**:
  `# agentspace host ` does not start with `# agentspace lease `, so a lease's `create`, overwrite
  or `release` that meets an operator alias treats it as a file with no owner mark, and changes
  nothing (below). The match MUST stay so, whatever case rule a reader uses.
- **A path or ProxyCommand that holds a CR or LF MUST be refused**, writing nothing, because a line
  break would add directives to the block. **So MUST any other value the file holds** (the ID, the
  comment, the alias and the user), since each sits on one line (architect, 2026-11-19, for
  E0.05-T01-S05). ssh expands `%` tokens in these values, which is why a
  literal `%` is doubled.
- **The ProxyCommand MUST name the alias literally, never as ssh's `%n`** (card #0057; operator.md,
  [The ProxyCommand](operator.md#the-proxycommand)).
- **`User`, `HostKeyAlias`, `ProxyCommand` and the two `RemoteForward` lines are today's driver's
  values** (D9, D12): the board's forwards are [lease-api.md Q7](lease-api.md#open-questions), and
  another driver MAY write others ([driver.md](driver.md#reachability)). The rest of the format is
  fixed.
- **Access rules:** inherited from `<ssh folder>`, and MUST be OpenSSH-safe, since ssh reads it as
  a configuration file.
- **Written through a temporary file, `<ssh folder>\<alias>.agentspace-tmp-<hex>`, moved into
  `agentspace.d`** in one attempt ([Replacing a file](#replacing-a-file)). The temporary file MUST
  NOT be put in `agentspace.d`, because the `Include` glob reads every file there.
- **The writer MUST NOT put any file but alias files in `agentspace.d`.** A person's own file there
  is read by ssh like an alias file, and agentspace never deletes or replaces it. The operator's
  host aliases are the one other kind of file agentspace puts there
  ([A host's operator alias](#a-hosts-operator-alias)).

**When the alias file already exists**, the writer reads its first line:
- **It names this lease, and its bytes equal the new ones:** nothing is written.
- **It names this lease, with other bytes:** it is replaced.
- **It names another lease, or it has no owner mark:** the writer MUST fail and change nothing.
  A host's operator alias's mark is no lease owner mark, so it is this case.
  Two kept leases never share an alias ([lease-api.md](lease-api.md#fields)), so this means a
  foreign file, a lease recorded in another state folder, or one a killed `create` left with no
  record ([Creation, in order](#creation-in-order)). **`create` then fails with `internal`, and
  MUST NOT draw another ID:** the file may be a person's, and an ID is drawn again only for the
  alias of a record ([lease-api.md](lease-api.md#fields)). This settles the second half of
  [Q9](#open-questions); the prototype does the same.
- **A provisioning run's `place_pending` first removes an alias file whose owner mark names the
  `pending` lease it is about to place** (architect, 2026-11-20, for v0.07-T01), since only a killed placement can
  have left it: a `pending` lease has no alias by contract
  ([Creation, in order](#creation-in-order)). The cases above then meet only a file that names
  another lease, or has no owner mark, and the placement fails on it, leaving the lease `pending`.
- **Something at that path that is not a file**, such as a folder, makes the write fail, and
  `create` fails with `internal`. **A symbolic link or other reparse point there is never
  followed, replaced or removed**: `create`, and `release`'s step 1, treat it as a foreign file
  (architect, 2026-11-19, from E0.05-T02-S03).

### A lease alias for a host that is not the box

**A lease placed on a host that is not the box has an alias file in the form above, with the host's
key named in two lines instead of one** (D47, D55): `HostKeyAlias agentspace-host.<project>`, and
one more line straight after it, `UserKnownHostsFile "<known_hosts path>"`. Its bytes, exactly,
under the same rules as the box's (UTF-8 without a byte order mark, each line ending in one LF,
every line after the second indented by two spaces):

```
# agentspace lease <id>; <comment>
Host <alias>
  User <user>
  IdentityFile "<key path>"
  CertificateFile "<certificate path>"
  IdentitiesOnly yes
  HostKeyAlias agentspace-host.<project>
  UserKnownHostsFile "<known_hosts path>"
  StrictHostKeyChecking yes
  ProxyCommand <proxy command>
  ServerAliveInterval 30
  ServerAliveCountMax 4
  RemoteForward 127.0.0.1:8811 127.0.0.1:8811
  RemoteForward 127.0.0.1:53147 127.0.0.1:53147
```

| Placeholder | Value |
|---|---|
| `<project>` | The lease's `project`, which is the `project` of the host registry record the lease is placed on |
| `<known_hosts path>` | The full path of [agentspace's `known_hosts`](#agentspaces-host-keys), written as `<key path>` is: the home folder joined as text with `.agentspace\known_hosts`, then every `\` written as `/` and every `%` as `%%`, as in `C:/Users/<user name>/.agentspace/known_hosts` |
| Every other | As for the box's alias, above |

- **Which form `create` writes is decided by the record the lease is placed on**: the record whose
  `stack` is `agentbox` gives the box's form, byte for byte as v0.06 writes it, and every other
  `ready` record gives this one ([Which host a connection reaches](#which-host-a-connection-reaches)).
  A provisioning run's `place_pending` writes this form for the host it made.
- **`StrictHostKeyChecking yes` stays**: a lease alias never accepts a new key. The key is put in
  agentspace's `known_hosts` by the operator's first connection to the host, during its
  provisioning run, before the record is `ready` ([Agentspace's host keys](#agentspaces-host-keys)).
- **The owner mark, the ProxyCommand line and its default, the temporary file, the access rules,
  the rules for an alias file that already exists and the order of writing and removal are the
  box's alias's**, above, and `release` removes it the same way. The ProxyCommand line names the
  lease's alias, as for the box, and the ProxyCommand finds the host from the lease
  ([Which host a connection reaches](#which-host-a-connection-reaches); D49).
- **`renew` changes nothing in it**, as for every alias. A lease keeps the form it was placed with.
- **A path that holds a CR or LF is refused, writing nothing**, as for the other paths: the
  `<known_hosts path>` too.

### The `Include` line

**`<ssh folder>\config` reaches the alias files through one line, which agentspace adds once and
never removes:**

```
Include agentspace.d/*
```

**OpenSSH resolves the relative path against the user's own `~/.ssh` folder**, for any user
configuration file, one named with `-F` included; for the user's own `~\.ssh\config` that is the
config's folder (corrected by the architect, 2026-11-19, from E0.05-T01-S05's run on the box,
which found that a relative `Include` in a `-F` config elsewhere did not reach its own
`agentspace.d`). So in a moved `<ssh folder>` the line is written the same, and only ssh pointed
there reads it as that folder's ([What moves which files](#what-moves-which-files)). The line MUST be
above the config's first `Host` or `Match` line, because an `Include` after one belongs to that
block.

**`create` makes sure the line is there, before it writes the alias file.** It reads the config
whole, as bytes, skips a leading UTF-8 byte order mark, splits the rest at each LF, trims spaces,
tabs and CRs from both ends of each line, and looks at the lines in order:
- **A line equal to `Include agentspace.d/*`, before any `Host` or `Match` line, is found.**
  "Equal" here allows any run of spaces or tabs between the two words, ignores ASCII case, and
  allows nothing else on the line: a trailing comment or a second pattern makes it a different
  line. Nothing is written, but `agentspace.d` is created if it is missing.
- **A `Host` or `Match` line** is a line whose first word is `Host` or `Match`, in any ASCII case,
  followed by a space, a tab, `=` or the end of the line. A commented line (`#...`) is neither.
- **If the line is not found above the first block, but appears below it,** `create` MUST fail,
  change nothing in the config, and say that a person has to move the line to the top of the file.
  `agentspace.d`, made first, MAY stay, as for any failed `create` (gap FB-04; architect,
  2026-11-19, from E0.05-T02-S03). The message is
  [lease-api.md](lease-api.md#messages)'s.
- **If it is found nowhere, it is added as the first line**, after the byte order mark if there is
  one. So a line that is not equal, such as `Include agentspace.d/* # aliases`, gets a second,
  exact line above it, and ssh then includes each alias file twice ([Q10](#open-questions)). The
  steps:
  1. **The config is backed up first**, as `<ssh folder>\config.agentspace-backup-<time>`, where
     `<time>` is the UTC time as `YYYYMMDDTHHMMSSZ`. The copy MUST NOT overwrite an existing file,
     and its SHA-256 MUST equal that of the bytes read above. If either fails, `create` fails,
     leaving that backup, if it was made, and `agentspace.d`, which is created before the backup,
     and changes nothing else. A second insertion in the same second finds the first one's backup,
     and so fails; that happens only when a person removed the line in between.
  2. **The new config is the original's bytes with the line inserted**: the BOM if any, then
     `Include agentspace.d/*`, then a line ending, then every original byte unchanged. The line
     ending is CR LF when the original contains a CR LF anywhere, and LF otherwise. **Every other
     byte of the user's file MUST stay as it was**: a config with no final line ending keeps none,
     and an empty config (0 bytes, or only a BOM) becomes the line and one LF, after its BOM if it
     had one.
  3. **It replaces the config atomically**, through `<ssh folder>\config.agentspace-tmp-<hex>`, in
     one attempt. **The replace MUST keep the config's own access rules**, because Windows
     OpenSSH checks them, and a user's config MAY have rules other than its folder's;
     `ReplaceFile` on Windows does this. It does not keep the owner, which becomes the user
     running `create`; that is OpenSSH-safe, and no check needs an elevated process to show it
     (architect, 2026-11-19, from E0.05-T02-S03). If the replace fails part way and the config is gone, the
     temporary file is the only new copy, and MUST be kept.
- **If there is no config at all,** it is created holding exactly the line and one LF, with
  inherited access rules, and `<ssh folder>` and `agentspace.d` are created as needed. No backup is
  made.
- **A config that is a symbolic link, another reparse point, or read-only** (this settles
  [Q12](#open-questions)):
  - **When the line is found,** the config is only read, through the link if it is one, and nothing
    is written to it, as for any config.
  - **When the line has to be added, `create` MUST fail with `internal` before it backs the config
    up**, and MUST NOT write through the link, replace the link with a file, or change the file's
    attributes. `agentspace.d` MAY have been created by then, and stays. The message's `<why>`
    ([lease-api.md](lease-api.md#messages)) says that the configuration is a link or read-only, so
    that a person has to add the line to the top of the file.
  - Note on where this came from: the prototype reads through a link, and its replace's outcome on
    each of the three is unobserved. On a read-only file, `ReplaceFile` most likely fails after the
    backup was made, which leaves that backup; a run settles whether the prototype meets the rule.
- **Access rules of the backup and the temporary file:** inherited from `<ssh folder>`; agentspace
  sets none.

**The operator makes sure of the line too, before it writes a host's operator alias**, by exactly
these steps and with the same backup, temporary file, failures and refusals (architect,
2026-11-20, for v0.07-T01, the dev lead's pin P11): a config where the line is below the first
block, or that is a link or read-only and lacks the line, fails the operator's command, which changes nothing in the
config and says that a person has to add or move the line. What the operator prints then is
[operator.md](operator.md)'s. Every rule in this section that names `create` binds the operator
the same way, but for the lease-api.md messages.

**Nothing else in the config is agentspace's.** The user's own `agentspace` entry is the
operator's ([operator.md](operator.md#the-agentspace-ssh-entry)); agentspace reads it only through
ssh, and never writes it.

**The backups are never deleted by agentspace.** There is one per insertion, which is once per
config unless a person removes the line.

### The host key

**A lease alias for the box checks the box's key under the name `agentspace` (`HostKeyAlias
agentspace`, `StrictHostKeyChecking yes`), and agentspace never writes that key.** Every other
host's key is in [agentspace's own `known_hosts`](#agentspaces-host-keys) instead (D47). The box's is in
`%USERPROFILE%\.ssh\known_hosts`, put there by ssh the first time the user connected through their
own `agentspace` entry, which accepts a new key ([operator.md, "The `agentspace` SSH
entry"](operator.md#the-agentspace-ssh-entry)). Without it, every login through
a lease alias is refused. A host whose key changes needs that entry replaced by a person. Whether
`create` SHOULD check for it is [Q3](#open-questions).

### Agentspace's host keys

**`%USERPROFILE%\.agentspace\known_hosts` holds the key of every host but the box, each under
its own name, `agentspace-host.<project>`, and only ssh writes it** (D47, D55). It never holds the box's
key: the operator reaches the box through the user's own `agentspace` entry and writes no operator
alias for it ([operator.md](operator.md)), and the box's lease aliases never read it.
- **Written by `ssh`, never by agentspace.** The first connection through a host's operator alias,
  which accepts a new key (`StrictHostKeyChecking accept-new`), makes ssh add the host's key under
  `agentspace-host.<project>`, creating the file if it is missing
  ([A host's operator alias](#a-hosts-operator-alias)). A provisioning run connects that way before
  it records the host `ready`, so the key is there before any lease alias needs it.
- **Read by `ssh`**, through the `UserKnownHostsFile` line of the host's operator alias and of every
  lease alias for that host, which name it by its full path
  ([A lease alias for a host that is not the box](#a-lease-alias-for-a-host-that-is-not-the-box)).
  Those aliases check strictly, so a lease alias whose host's key is not there is refused by ssh.
- **agentspace MUST NOT read, write, move or remove it**, and its format is OpenSSH's. Nothing in
  agentspace causes ssh to write the user's `%USERPROFILE%\.ssh\known_hosts` for a host that is
  not the box: every alias agentspace writes for such a host names this file.
- **A host whose key changes**, such as one whose instance was replaced, **is refused by every
  alias, the operator's included**, since `accept-new` accepts only a name with no key. A person
  removes the old line, with `ssh-keygen -R agentspace-host.<project> -f <its full path>`, and the
  next operator connection accepts the new key.
- **`USERPROFILE` moves it; `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` do not**
  ([What moves which files](#what-moves-which-files)).
- **Access rules:** what ssh gives a file it creates in `%USERPROFILE%\.agentspace`, inherited;
  agentspace sets none.
- **Never removed by agentspace.**

### Creation, in order

**For a lease that is to be `ready`, the v0 driver does these, in this order, inside `create` and
under [the lease state lock](#the-lease-state-lock):**
1. **Checks the CA** by the five checks of [Its checks](#its-checks), and no others, and fails if it
   is not usable. `create` MUST NOT create a CA ([lease-api.md](lease-api.md#the-credential)).
2. **Refuses an existing key** ([The key](#the-key)), and creates the lease folder.
3. **Makes the key pair**, and checks that it loads with an empty passphrase.
4. **Signs the certificate** ([The certificate](#the-certificate)).
5. **Makes sure of the `Include` line** ([The `Include` line](#the-include-line)).
6. **Writes the alias file** ([The alias file](#the-alias-file)).

Then `create` writes the record ([The lease state file](#the-lease-state-file)).

**For a lease that is `unsatisfiable`, `create` writes only the record**, with its reasons and
`driver`. It runs no CA check and makes no file, so a laptop with no CA still gets an
`unsatisfiable` answer.

**For a lease that is `pending`, `create` writes only the record too** (D48), with `driver`: no
lease folder, no key, no certificate and no alias. **It first checks the CA by the five checks of
[Its checks](#its-checks)**, as step 1 does for a `ready` lease, and a CA that fails them fails
`create` as for a `ready` lease, with the same message and no record (architect, 2026-11-20, for
v0.07-T01). What
it writes to the host registry, if anything, is in [the host registry's states](#its-states).

**A provisioning run places a `pending` lease by the same six steps**, through the EC2 driver's
`place_pending`, and then writes the record `ready`
([The lease state file](#the-lease-state-file); [driver.md](driver.md#place_pending)). **It takes
the lease state lock once for each lease** (architect, 2026-11-20, for v0.07-T01): under it, it re-reads the
state file, skips the lease if its record is no longer `pending`, and otherwise places it and
writes its record before it releases the lock. The six steps and every rule below bind it as they
bind `create`, with one exception: **before step 2, it removes an alias file at the lease's alias whose owner mark names that ID,
then that ID's lease folder, with everything in it** (architect, 2026-11-20, for v0.07-T01), since only a
killed placement can have left them: a `pending` lease has none by contract. Step 2's refusal of
an existing key then finds none. An alias file there that names another lease, or has no owner
mark, is not removed, and step 6 fails on it ([The alias file](#the-alias-file)). A removal that
fails is a failed step. **A failure at any step leaves the record `pending`**, never
`unsatisfiable` (D48), and removes every file and folder that placement made, as for a failed
`create`.

**What `create` says when a step fails is lease-api.md's** ([Messages](lease-api.md#messages)),
under the rules for an `internal` message there, which forbid a path, the user's name and another
program's output ([lease-api.md](lease-api.md#exit-codes-and-errors)). A CA with either file
missing gives the "No CA" message. Any other failed check of the CA gives a message in
agentspace's own words that says the CA is not usable and that the operator's `ca` says why,
never the check's own output. The same holds for `renew`.

- **An alias file MUST NOT exist without its lease's key and certificate**, so it is written last
  and, at release, removed first.
- **A failure at any step MUST remove every file and folder this `create` made, and MUST leave no
  record** (#0029; [lease-api.md](lease-api.md#concurrency-and-atomicity), which also says what
  `create` does when a removal fails). The lease folder is deleted only if this `create` made it,
  and the alias file only if its owner mark names this lease.
- **An `Include` line, a config backup, `agentspace.d` and `%USERPROFILE%\.agentspace\leases\`
  that the failed `create` made stay, and `create` MUST NOT remove them** (the folder of lease
  folders is shared by every lease, as `agentspace.d` is; architect, 2026-11-19, from
  E0.05-T02-S02) (this settles [driver.md Q18](driver.md#open-questions)). The line
  is meant to stay, and removing it would be a second edit of the user's config that could fail
  too; the backup is a person's record of the config before the line; and the folder holds nothing
  of the lease. None of them is a lease, a key, a certificate or an alias, so lease-api.md's rule
  for a failed `create` holds. Its message SHOULD then say that no lease was recorded, never that
  nothing was changed ([lease-api.md](lease-api.md#messages)). The prototype leaves them too.
- **A `create` killed after it wrote the alias file and before it wrote the record leaves a lease
  folder and an alias file that no record names.** No command can release them, since `release`
  needs a record, and a later `create` fails only if its new ID's alias happens to be that one,
  because `create` draws again for the aliases of records, never of files
  ([The alias file](#the-alias-file); [Q9](#open-questions)).
- **Elevation:** `create` and `renew` do not refuse to run elevated, unlike the operator's `ca`
  ([The CA](#its-access-rules)). The files an elevated `create` makes MAY then be owned by
  `Administrators`, which OpenSSH-safe allows; this is unobserved.

### Renewal

**`renew` replaces the certificate and nothing else.** The key, the alias file and the `Include`
line MUST NOT be touched.

**What a failed `renew` MUST leave (#0041, point 3):** the lease's `expires_at`, `ttl_seconds` and
certificate exactly as they were ([lease-api.md](lease-api.md#concurrency-and-atomicity)). So the
previous certificate MUST be kept, byte for byte, until the record with the new `expires_at` is
written, and put back if that write fails. `id_ed25519-cert.pub` MUST NOT ever be part written.

**The order that meets it, which the rewrite SHOULD follow**, all under the lease state lock:
1. **Check the CA**, and fail, changing nothing, if it is not usable.
2. **Require `id_ed25519` and `id_ed25519.pub`** in the lease folder, and fail, changing nothing,
   when either is missing. A missing certificate is not an error.
3. **Sign a new certificate under a temporary name:** copy `id_ed25519.pub` to `renew-<hex>.pub` in
   the lease folder, and sign the copy, which gives `renew-<hex>-cert.pub` beside it, valid from
   this command's issue time ([The certificate](#the-certificate)).
4. **Keep the previous certificate**, if there is one, as `renew-<hex>-previous-cert.pub` in the
   lease folder, by copy or hard link.
5. **Move `renew-<hex>-cert.pub` over `id_ed25519-cert.pub`** in one rename, so ssh finds the old
   certificate or the new one, never part of either.
6. **Write the record** with the new `expires_at` and `ttl_seconds`
   ([The lease state file](#the-lease-state-file)).
7. **If step 5 or 6 failed, move `renew-<hex>-previous-cert.pub` back over
   `id_ed25519-cert.pub`**, or, when there was no previous certificate, delete the new one.
8. **Delete every `renew-<hex>` file**, whether or not anything failed.

- **A `renew` killed between steps 5 and 6 leaves the new certificate under the old record**, and
  nothing puts the old one back: a killed process cannot. The next `renew` or `release` settles
  the lease ([Q11](#open-questions)).
- **The temporary files are `renew-<hex>.pub`, `renew-<hex>-cert.pub` and
  `renew-<hex>-previous-cert.pub`**, with one `<hex>` per `renew`. A killed `renew` can leave any
  of them, and `release` removes them with the folder.
- Note on where this came from: the prototype does steps 1, 2, 3, 5 and 8, and its CLI then writes
  the record. It keeps no copy of the previous certificate, so a failed write of the record leaves
  the new certificate in place
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

### Release, and what it leaves

**`release` of a lease whose stored state is `ready` or `released`, or `pending` with a `driver`,
removes its files** (D10; architect, 2026-11-20, for v0.07-T01):
1. **The alias file, `<ssh folder>\agentspace.d\<alias>`**, if it exists and is a file. A missing
   alias file is not an error.
2. **The lease folder, `%USERPROFILE%\.agentspace\leases\<id>\`**, with everything in it, if it
   exists. A missing folder is not an error. **A file where the home folder, `.agentspace` or
   `leases` has to be is**: it is never read as a missing lease folder, and `release` fails with
   lease-api.md's "Credential or alias not deleted" message, its `<why>` "the folder for leases is
   a file, not a folder" (architect, 2026-11-19, from E0.05-T02-S02).

**What a failed `release` MUST leave (#0041, point 3):** never a record that is `ready` while its
alias, key or certificate is gone ([lease-api.md](lease-api.md#concurrency-and-atomicity)). It
leaves the lease as it was, or `released` with some of its files left, and a retried `release`
of the `released` record MUST then remove whatever is left. Within the files, the alias file MUST
go before the lease folder, so that an alias never points at a key that is gone.

**The order that meets it, which the rewrite SHOULD follow**, all under the lease state lock:
1. **Check the alias file's owner mark**, if the file exists. If it names another lease, or it has
   none, `release` MUST fail with `internal` and change nothing, the record and the lease folder
   included. **Something at the alias's path that is not a file, such as a folder, is treated as a
   file with no owner mark**: `release` fails the same way and deletes nothing (architect,
   2026-11-19, gap FA-07). **So does a file where `<ssh folder>` or its `agentspace.d` has to be**:
   it is never read as a missing alias file, and `release` fails with `internal` and deletes
   nothing, its message saying which folder is a file (architect, 2026-11-19, from
   E0.05-T02-S03).
2. **Write the record as `released`**, unless it is already ([The lease state file](#the-lease-state-file)).
3. **End the lease's connections on the laptop, and delete the alias file between two walks**
   (D59, as D63 and pin P23.3 changed it): the driver ends the ssh clients that use the lease's
   alias, the keeper's holder among them, through `agentspace-connections`; deletes the alias file;
   ends them again; and logs one line for both walks in [the release log](#the-release-log). The
   second walk runs whether or not the alias file's deletion succeeded. What it ends, how, the
   order and the line's text are
   [driver.md, Ending a lease's connections](driver.md#ending-a-leases-connections)'s (corrected
   by the architect, 2026-11-22, for v0.08-T03: this step read one walk before the alias's
   deletion, from before P23.3).
4. **Delete the lease folder**, as above, only when the alias file's deletion in step 3 succeeded.

- **A failed deletion, of the alias file in step 3 or of the lease folder in step 4, fails
  `release` with `internal`**, after step 3's second walk and its log line, with lease-api.md's "Credential or alias
  not deleted" message ([Messages](lease-api.md#messages)), and the record stays `released`. A file
  that another process holds open without sharing delete is such a failure. `release` run again
  finishes the job.
- **A foreign file at the alias's place** fails every `release` of the lease, in either state,
  until a person moves or deletes that file, which is not agentspace's; `release` then succeeds
  ([driver.md Q13](driver.md#open-questions)).
- Note on where this came from: the prototype deletes the alias file, then the lease folder, and
  then writes the record, so a failure to delete the folder leaves a `ready` record with no alias.
  Its `release` of a `released` record touches no files
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

- **`release` MUST NOT need a CA, and MUST NOT make a CA check.**
- **`release` MUST NOT change `<ssh folder>\config`.** The `Include` line stays, on purpose: it is
  how the next lease's alias is found.
- **`release` of a `pending` record that carries a `driver` hands it to that driver's release, as
  for a `ready` one** (architect, 2026-11-20, for v0.07-T01; [driver.md](driver.md#release-a-lease)):
  under the lease state lock it checks the alias file's owner mark, writes `released`, ends the
  lease's connections, finds none, and logs its line ([The release log](#the-release-log)), then
  removes the alias file at the lease's alias only if its owner mark names this lease, then the lease's
  folder with everything in it. A missing file or folder is not a failure. Only a killed placement
  can have left them, since a `pending` lease has none by contract. It reads neither the host
  registry nor the project list. **With no home folder, the cleanup is skipped and the release still
  succeeds**, since a `pending` lease needs no file. A later `release` of the `released` record,
  with the home set, removes what is left, as for any `released` record. A `pending` record with no
  `driver` asks none and touches no file. A provisioning run that reads the lease state file
  afterwards finds the record `released`, and never places it
  ([The lease state file](#the-lease-state-file)).
- **A killed placement's leftovers for a record still `pending` are also removed first by that
  lease's next placement** (architect, 2026-11-20, for v0.07-T01;
  [Creation, in order](#creation-in-order)).

**What release leaves:**
- **The certificate stays valid until it expires**, on every host that trusts the CA, for anyone
  who copied it and its key. It is not revoked (D10; card #0010 replaces this).
- **No connection through the lease's alias**, since step 3 of the order above ended them, and
  the lease's alias is gone for any new one. A connection that step could not end is logged
  ([driver.md, Ending a lease's connections](driver.md#ending-a-leases-connections)).
- **The record**, `released`, in the lease state file.
- **The `Include` line, `agentspace.d` (empty if no other lease is ready) and any config backup.**
- **`%USERPROFILE%\.agentspace\leases\`**, empty or not. Release removes only the lease's own
  folder.
- **The keeper's holder, if it was holding the board's forwards through this lease's alias, is
  one of the clients that step 3 ended.** That is the holder of the lease's host, the box's or
  [another host's](#another-hosts-holder). The keeper finds it gone, waits its release grace, and
  then starts a holder through another of the host's ready leases, if there is one
  ([keeper.md, Release grace](keeper.md#release-grace)).
- **The lease's host, its record in the host registry, and every file in its folder.** Releasing a
  lease never changes a host (D25).

### The release log

**`<state folder>\release.log` records what a release did to the lease's connections** (D59, as D63
changed it; the dev lead's pin P16; architect, 2026-11-21, for v0.08-T01). It is written by the
EC2 driver's `release` at step 3 of [the order above](#release-and-what-it-leaves), after both
walks, so after the alias file's deletion and before the lease folder's, and its line texts
are [driver.md, Where the count is logged](driver.md#where-the-count-is-logged)'s. This section
owns the file as a file.
- **A line's form is [the guard's log's](#the-guards-log) (v0.08-T01's pin P2)**, with the lease in place of
  the host: a UTC time in the form `YYYY-MM-DDTHH:MM:SSZ`, one space, `lease=<id>`, one space, then
  driver.md's text:

  ```
  <time> lease=<id> <text>
  ```

  `<id>` is the lease's normalized ID ([The lease state file](#the-lease-state-file)).
- **Its bytes:** UTF-8 without a byte order mark, each line ending in one LF. **Opened for append,
  sharing read, write and delete**, as the guard's log is, and each line written whole, in one
  write at the end of the file. **A reader SHOULD share the same.**
- **It is created when the driver writes its first line**, if it is missing, in the `<state
  folder>`, which `release` has already made by writing the record. **It follows
  `AGENTSPACE_STATE_DIR`**, as `leases.json` does ([What moves which
  files](#what-moves-which-files)), so a check that sets the variable gets a log of its own.
- **Written by the EC2 driver's `release` alone. Read by people**; no program of agentspace reads
  it.
- **A log that cannot be written is ignored**: `release` goes on to step 4 and does not fail for
  it, since the log reports on the release and the release MUST NOT depend on it.
- **It never holds the alias**, nor a key, a certificate, a whole `ssh` command line or any
  other secret ([The lease folder](#the-lease-folder)): the lease's ID is its only name for it,
  and the alias, which is the lease's address, stays out.
- **Never rotated or removed by agentspace.** A person MAY delete or move it between releases.
- **It is not under the lease state lock's protection as a file**, but it is written while
  `release` holds that lock, so two releases never write it at once; a person's tool that holds
  it open without sharing makes the write fail, which is ignored.

### Between the prototype and the rewrite

**Status (E0.06-T06-S01, 2026-11-20): the condition of this section has passed.** The prototype was
removed from the repository, and the cutover to the Rust programs (E0.06-T05-S01, [the run
record](runs/e0.06-t05-s01.md)) left one program for each role, so no two programs share a
home folder now. The checklist below is kept as the record of what #0054 required, and the last
paragraph's lock rule still binds every program that changes the lease state file.

**The two MUST be able to share one home folder and one state folder (#0054).** These are the
facts each relies on in the other's files; everything above is binding, and this is the
checklist.

**For the prototype to renew and release a lease the rewrite made:**
- **The record** is valid against the schema, with `driver` `ec2`, the lease's `project`, and the
  normalized `id`.
- **The lease folder** is `%USERPROFILE%\.agentspace\leases\<id>\`, and holds `id_ed25519` and
  `id_ed25519.pub`, the private key with no passphrase.
- **The alias file** is `<ssh folder>\agentspace.d\agentspace-<project>-<first 8 hex of id>`, and
  its first line starts `# agentspace lease <id>;`.
- **Both run with the same `USERPROFILE`, and with the same `AGENTSPACE_STATE_DIR` and
  `AGENTSPACE_SSH_DIR`, if either is set.**

**For the rewrite to renew and release a lease the prototype made**, the same holds the other way:
the prototype writes every record with `driver` `ec2`, and writes the files exactly as this section
describes.

**What differs between them, and is expected:**
- **A `released` record whose files remain is cleaned up only by the rewrite;** the prototype's
  `release` of a `released` record leaves them.
- **A stored `ready` record with no `driver`** is renewed and released by the rewrite, and fails
  with `internal` under the prototype.
- **A `renew-<hex>-previous-cert.pub` left by a killed rewrite `renew`** is removed by either
  one's `release`, with the folder.

**Both MUST take the same lock** ([The lease state lock](#the-lease-state-lock)), so that neither
changes the file under the other.

## A host's folder

**`%USERPROFILE%\.agentspace\hosts\<project>\` holds what the operator keeps for one host (D47,
D48, D50, D55)**, where `<project>` is the host's record's `project`:

| File | What it is | Section |
|---|---|---|
| `id_ed25519`, `id_ed25519.pub` | The operator's key for this host | [The operator's key and certificate](#the-operators-key-and-certificate) |
| `id_ed25519-cert.pub` | The operator's certificate, key ID `operator-<project>`, valid one hour | The same |
| `provision.log` | The provisioning run's log, which the run opens for append as its first step, creating it if it is missing | [The provisioning run's log](#the-provisioning-runs-log) |
| `provision.lock` | The provisioning run's lock: held while a run is running | [The provisioning run's lock](#the-provisioning-runs-lock) |
| `localswim-pushed.json` | The host's push stamp, for every host but the box | [A host's push stamp](#a-hosts-push-stamp) |

- **The folder is created by the first writer of a file in it**, `%USERPROFILE%\.agentspace\hosts\`
  with it, as [The home folder](#the-home-folder) says. **The driver also creates the folder, if it
  is missing, before it starts a provisioning run** (architect, 2026-11-20, for v0.07-T01), and a
  failure to create it is a start that fails: no run is started, and the lease stays `pending`
  ([The provisioning run's start](#the-provisioning-runs-start)). **The driver never creates or
  opens `provision.log`**: the run does, as its first step
  ([The provisioning run's log](#the-provisioning-runs-log)). A missing folder means its files do not
  exist. **The folder is never removed by agentspace**, nor is any file in it but the temporary
  files their sections name. A provisioning run is started with no files of its own
  ([The provisioning run's start](#the-provisioning-runs-start)).
- **`AGENTSPACE_STATE_DIR` does not move it**, nor any file in it ([What moves which
  files](#what-moves-which-files)): a provisioning run started from one state folder and a command
  run from another share the host's lock and log.
- **Access rules:** the folder's are inherited from `%USERPROFILE%\.agentspace`, and so are its
  files', but for the private key's ([The operator's key and certificate](#the-operators-key-and-certificate)).
- **The writer MUST NOT put any other file in it.** A person's file there is left alone.
- **The box MAY have a folder too, `hosts\swimcoach\`**, when a run is started for it; it then holds the same files under the same rules, but for the push stamp,
  which stays the box's own ([The push stamp](#the-push-stamp)).
- **Something at `hosts\` or `hosts\<project>\` that is not a folder** fails every command that
  needs a file in it, writing nothing there, as [The home folder](#the-home-folder) says of any
  folder.

### The operator's key and certificate

**The operator reaches a host as `ubuntu` with a key of its own and a certificate it signs for
itself with the CA, never with the user's own key** (D47, D55).

- **The key is an ed25519 key pair in OpenSSH's formats, without a passphrase**, exactly as a
  lease's key is ([The key](#the-key)), with the same line-ending tolerance for a reader. **Its
  comment SHOULD be `agentspace host <project>`**, and MUST hold no line break. Nothing reads it.
- **It is made once, by the first operator command that signs a certificate for the host and finds
  `id_ed25519` missing, and kept** (architect, 2026-11-20, for v0.07-T01). The command makes it
  while it holds [the host registry lock](#the-host-registry-lock), after looking again for
  `id_ed25519` once it holds the lock, so two commands never make two keys; a leftover
  `id_ed25519.pub` or `id_ed25519-cert.pub` without `id_ed25519` is replaced. **An existing key is
  never replaced**: an `id_ed25519` that does not load with an empty passphrase, or is not ed25519,
  fails the command with a message that names the file, and a person removes it
  ([operator.md](operator.md)). The writer MUST check that a new key loads with an empty passphrase
  before it signs.
- **The private key's access rules are a lease private key's** (architect, 2026-11-20, for
  v0.07-T01, the dev lead's pin P11): OpenSSH-safe, readable by no principal but the user,
  `SYSTEM` and `Administrators`, and, when the writer sets them itself, exactly the three entries of
  [The lease folder](#the-lease-folder). The public key and the certificate are not secret.
- **The certificate is an OpenSSH user certificate for that key, signed by [the CA](#the-ca)**, in
  the format and with the content of a lease's [certificate](#the-certificate), but for these
  fields:

  | Field | Value |
  |---|---|
  | Key ID | `operator-<project>` (D47, D55) |
  | Valid principals | Exactly one: `ubuntu` |
  | Valid before | The issue time plus one hour, 3600 seconds, exactly |
  | Valid after | MUST NOT be later than the issue time, and SHOULD be 5 minutes before it, as a lease's |
  | Extensions | A lease certificate's two, `permit-port-forwarding` and `permit-pty` (architect, 2026-11-20, for v0.07-T01) |

  **Its comment** is any text with no line break, and SHOULD be the public key's comment.
- **The certificate is renewed by every operator command that names a host other than the box**
  (`-Host <project>`), `status -Host` included, whether or not it connects (architect, 2026-11-20,
  for v0.07-T01), **and by every operator command that connects through the host's operator alias**, such
  as `sync -All` (architect, 2026-11-20, for v0.07-T01). Each
  signs a fresh one-hour certificate first, after checking the CA by [Its checks](#its-checks), and
  fails, connecting to nothing, if the CA is not usable. A command that runs longer than an hour
  signs again before it connects again; a connection already open stays open, since the
  certificate is checked only when a connection is made.
- **A person signing in over `ssh agentspace-host.<project>`** (D52) first runs an operator command
  for that host, and runs one again once an hour has passed: operator.md and D52's procedure in the
  README say which, `agentspace-operator status -Host <project>` (architect, 2026-11-20, for
  v0.07-T01). **That command, as every operator command that names a host other than the box,
  writes the host's operator alias and signs a fresh certificate** (architect, 2026-11-20, for
  v0.07-T01), so it
  always leaves a working `ssh agentspace-host.<project>`. The person's ssh then uses the
  operator's key and certificate through the operator alias
  ([A host's operator alias](#a-hosts-operator-alias)).
- **A new certificate replaces the old one in one rename** (architect, 2026-11-20, for v0.07-T01): the command
  copies `id_ed25519.pub` to `sign-<hex>.pub` in the host's folder, signs the copy, which gives
  `sign-<hex>-cert.pub` beside it, moves that over `id_ed25519-cert.pub`, and deletes every
  `sign-<hex>` file whether or not anything failed. So ssh reads the old certificate or the new
  one, never part of either, and two commands for one host at once each leave a whole certificate.
  A command killed between the steps leaves its `sign-<hex>` files, which nothing reads
  ([Q4](#open-questions)).
- **Neither the key nor the certificate is ever a lease's**, and no lease command reads, writes or
  removes them.

### A host's operator alias

**`<ssh folder>\agentspace.d\agentspace-host.<project>` is the operator's way to reach a host**
(D47, D55), one file per host, written with the same rules as a lease's alias file, but for what
this section says (architect, 2026-11-20, for v0.07-T01, the dev lead's pin P11). **It is written
by every operator command that names a host other than the box, `status -Host` included, whether
or not it connects** (architect, 2026-11-20, for v0.07-T01), **and by every operator command that connects to a
host through it**. A person's own `ssh agentspace-host.<project>` uses it too, as D52's sign-ins
do, and is let in only while the certificate the last operator command signed is valid, an hour at
most; a person gets a fresh one by running `agentspace-operator status -Host <project>` first, as
[The operator's key and certificate](#the-operators-key-and-certificate) says.

**Its bytes, exactly:** UTF-8 without a byte order mark, each line ending in one LF, the last
included, and every line after the second indented by two spaces:

```
# agentspace host <project>; <comment>
Host agentspace-host.<project>
  User ubuntu
  IdentityFile "<key path>"
  CertificateFile "<certificate path>"
  IdentitiesOnly yes
  HostKeyAlias agentspace-host.<project>
  UserKnownHostsFile "<known_hosts path>"
  StrictHostKeyChecking accept-new
  ProxyCommand <proxy command>
  ServerAliveInterval 30
  ServerAliveCountMax 4
```

| Placeholder | Value |
|---|---|
| `<project>` | The host's record's `project` |
| `<comment>` | Any text without a line break. It SHOULD say that agentspace wrote the file and that it MUST NOT be edited |
| `<key path>` | The full path of the host's `id_ed25519`, the home folder joined as text with `.agentspace\hosts\<project>\id_ed25519`, written as a lease alias's `<key path>` is ([The alias file](#the-alias-file)) |
| `<certificate path>` | The full path of the host's `id_ed25519-cert.pub`, written the same way |
| `<known_hosts path>` | The full path of [agentspace's `known_hosts`](#agentspaces-host-keys), written the same way |
| `<proxy command>` | The default ProxyCommand line of [The alias file](#the-alias-file), `"<program>" agentspace-host.<project>`, with `%` written as `%%`. `<program>` is the full path of `agentspace-ssh-proxy.exe` as the operator finds it as [operator.md, How each program finds the others](operator.md#how-each-program-finds-the-others) says. `AGENTSPACE_PROXY_COMMAND`, the lease CLI's variable, does not change it (architect, 2026-11-20, for v0.07-T01) |

- **It differs from a lease alias only in these:** the owner mark; the `Host` name; `User ubuntu`;
  the operator's key and certificate; `StrictHostKeyChecking accept-new`, so the first connection
  adds the host's key ([Agentspace's host keys](#agentspaces-host-keys)); and no `RemoteForward`
  line, since the operator holds no board (D47).
- **The owner mark is the first line, `# agentspace host `, the project, `;`, one space, then the
  comment**, written exactly so. A reader recognizes an operator alias by a first line, up to the
  first LF, that starts with `# agentspace host `, followed by a project and a `;`. Neither mark
  ever matches the other ([The alias file](#the-alias-file)), so no lease's `create`, overwrite or
  `release` ever changes or removes an operator alias, and the operator never changes or removes a
  lease's alias.
- **A value that holds a CR or LF MUST be refused, writing nothing**, as for a lease alias.
- **Written by a command that names the host whether or not it connects, and always before the
  command's first connection to the host, through a temporary file,
  `<ssh folder>\agentspace-host.<project>.agentspace-tmp-<hex>`, moved into `agentspace.d`** in one
  attempt ([Replacing a file](#replacing-a-file)), after the command has made sure of
  [the `Include` line](#the-include-line). Access rules: inherited from `<ssh folder>`, and MUST be
  OpenSSH-safe.
- **When the file already exists**, the operator reads its first line:
  - **It names this host, and its bytes equal the new ones:** nothing is written.
  - **It names this host, with other bytes:** it is replaced.
  - **It names another host, carries a lease's owner mark, or has no owner mark:** the command
    MUST fail and change nothing; the file may be a person's.
  - **Something at that path that is not a file**, or a symbolic link or other reparse point,
    fails the command, and is never followed, replaced or removed.
- **`AGENTSPACE_SSH_DIR` moves it**, as it moves lease aliases; how the operator points its own ssh
  at it then is operator.md's.
- **Never removed by agentspace**: a host is kept (D25).

### The provisioning run's log

**`%USERPROFILE%\.agentspace\hosts\<project>\provision.log` is the log of every provisioning run
for that host** (D48, as D55 moves it; architect, 2026-11-20, for v0.07-T01, the dev lead's pin P4).
D48's `<state folder>\hosts\<project>.log` is not used.
- **Appended to and never rewritten in place**, one line per step, each a UTC time in the form
  `YYYY-MM-DDTHH:MM:SSZ` as keeper.md's log lines carry it ([keeper.md](keeper.md#log-lines)),
  then the step's text. The run writes its own lines as UTF-8 without a byte order mark, each
  ending in one LF. What each line says is [operator.md](operator.md#provision)'s.
- **The run opens `provision.log` for append itself, as its first step, creating it if it is
  missing, and writes every line there** (architect, 2026-11-20, for v0.07-T01). **A run that
  cannot open its log exits 1 and writes nothing else.** The driver never opens the log or passes
  the run a handle to it: a process created through WMI's `Win32_Process.Create` cannot be handed
  one, and the starting program gives the run no stdout and no stderr
  ([The provisioning run's start](#the-provisioning-runs-start)). **The log holds the run's own
  lines alone**, a failed step's line carrying the failure message in full, the program's own
  error text included; the programs' other output goes to the run's own streams, not the log
  (architect, 2026-11-21, for v0.07-T03, replacing the T01 ruling that copied it;
  [operator.md, provision](operator.md#provision)).
- **Opened for append, sharing read, write and delete**, as a holder's log is
  ([The keeper's logs](#the-keepers-logs)), so a reader, a second writer or a run by hand never
  blocks a run. A reader SHOULD share the same.
- **Written by `provision` alone**, whether the driver started it or a person ran it; **never by
  the driver**, which neither creates nor opens it
  (architect, 2026-11-20, for v0.07-T01). **Read by people.**
  No program of agentspace reads it. **Never rotated or removed** by agentspace.
- **It MUST NOT hold a secret**, nor the CA's comment, nor a whole `aws` or `ssh` command line
  ([The CA](#its-files); operator.md, Secrecy).

### The provisioning run's lock

**`%USERPROFILE%\.agentspace\hosts\<project>\provision.lock` is held for the whole life of a
provisioning run, so a held lock means a run is running** (D55).
- **Holding it means having the file open with no sharing**, for reading and writing, creating it
  if it is missing, as [the lease state lock](#the-lease-state-lock) is held; the taker creates the
  host's folder first if it is missing. **The file is never deleted, its content is empty and
  meaningless, and the handle MUST NOT be inheritable**, so no program the run starts holds it.
  The operating system releases it however the run ends, so a killed run leaves the lock free.
- **It is always taken without waiting** (architect, 2026-11-20, for v0.07-T01, the dev lead's pin
  P2): one try, which either has it or finds it held.
  - **`provision` takes it right after its first step, opening
    [its log](#the-provisioning-runs-log), and before it writes anything else, and holds it until
    it has recorded its outcome and called `place_pending`.** A `provision` that finds it held
    writes one line to its log saying that a run is already running, writes no record and nothing
    else, and exits 1 ([operator.md](operator.md#provision); architect, 2026-11-20, for v0.07-T01).
  - **The driver tries it, and releases it at once if it got it**, only to learn whether a run is
    running ([driver.md](driver.md#check_pending)).
  - **A failure to open it other than its being held**, such as access denied, or a folder at its
    path, is not a free lock: the taker treats it as a failure of its own and starts or runs
    nothing.
- **Which other operator commands refuse to run while it is held is operator.md's**
  ([Which host a command acts on](operator.md#which-host-a-command-acts-on)). A missing file is a
  lock nobody holds.

### The provisioning run's start

**The driver starts a provisioning run detached, and the start uses no file of its own** (D48,
D55's `agentspace-detach`; architect, 2026-11-20, for v0.07-T01, the dev lead's pin P12). It is
started with WMI's `Win32_Process.Create`, which returns the run's process ID, and nothing on this
page records that ID: [the run's lock](#the-provisioning-runs-lock) is the run's proof of life.
There are no launch files, no claim and no result file for a run, so a start that is killed or
fails leaves nothing behind in the host's folder but the folder itself, which the driver makes
before the start (below).
- **The program it starts** is named by `AGENTSPACE_PROVISION_CMD`, when that is set and not empty
  in the lease CLI, as the program's full path; a relative value is made full against the lease
  CLI's working folder before the start, as [The home folder](#the-home-folder) makes a relative
  folder full. Otherwise it is `agentspace-operator.exe` in the folder the lease CLI's own
  executable runs from. **It is never `AGENTSPACE_OPERATOR_CMD`.** What the variable means to the
  lease CLI is [lease-api.md](lease-api.md#environment)'s, and the run's command line,
  `provision -Host <project>`, is [operator.md](operator.md#provision)'s.
- **The run's environment is the user's logon environment, with these four exactly as this list
  says** (architect, 2026-11-20, for v0.07-T01):
  - **`USERPROFILE`**, as the starting program has it, made full against the starting program's
    working folder ([The home folder](#the-home-folder)).
  - **`AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR`**, each in the run's environment if and only
    if the starting program has it set and not empty, each made full
    ([The home folder](#the-home-folder)).
  - **`AGENTSPACE_PROXY_COMMAND`**, in the run's environment if and only if the starting program
    has it set and not empty, passed exactly as the starting program has it. It is a command line,
    not a folder, and is never made full.
  - **A value of one of those three that the logon environment holds and the starting program does
    not is removed**, so the run never takes a folder or a ProxyCommand line that the command which
    started it did not use.
- **So a run uses the files of the command that started it** (architect, 2026-11-20, for
  v0.07-T01): its `place_pending` places the pending leases of that command's lease state file,
  writes their aliases into that command's `<ssh folder>`, and writes that command's
  `AGENTSPACE_PROXY_COMMAND` as their ProxyCommand line when it had one, and otherwise the default
  line, which names the ProxyCommand program found beside the run's own executable
  ([The alias file](#the-alias-file)). Pending leases recorded under another `AGENTSPACE_STATE_DIR` wait for a
  `create` or `get` from there to start the next run.
- **The starting program gives the run none of its own standard handles**: the run's stdout and
  stderr are its hidden console's, which nothing reads (architect, 2026-11-20, for v0.07-T01;
  corrected for v0.07-T02, [driver.md, Starting a provisioning
  run](driver.md#starting-a-provisioning-run)), since a process created through WMI's
  `Win32_Process.Create` cannot be handed a file handle. **The run writes its own log**, opening
  the host's [`provision.log`](#the-provisioning-runs-log) for append as its first step, and
  writes its own lines alone there; the programs it starts write to its own streams (architect,
  2026-11-21, for v0.07-T03).
- **Before the start, the driver, in the starting program, creates `hosts\<project>\` if it is
  missing** (architect, 2026-11-20, for v0.07-T01), and never creates or opens the log. **A failure
  to make the folder is a start that fails**: no run is started, and the lease stays `pending`;
  what the driver does then is [driver.md](driver.md#check_pending)'s.
- **Its working folder is the home folder** (architect, 2026-11-20, for v0.07-T01). The state and ssh folders
  it is given are full, so its working folder changes none of the files it uses.

### A host's push stamp

**`%USERPROFILE%\.agentspace\hosts\<project>\localswim-pushed.json` records the last token push
that host confirmed, for every host but the box** (D50, D55). The box's stays
`%USERPROFILE%\.agentspace\localswim-pushed.json`.
- **Its form, schema, bytes, temporary file name (`localswim-pushed.json.<hex>.tmp`, in the host's
  folder), atomic replace, access rules and reading rules are [the push stamp](#the-push-stamp)'s**,
  in every point, and so is what it is the hash of, which covers the host's own instance ID
  ([operator.md](operator.md#the-stamp)).
- **Written by** `up -Host <project>`, `sync -Host <project>`, `sync -All` and `provision`, only
  after that host confirms the push, creating the host's folder if needed. **Read by** `up -Host
  <project>`, to decide whether a push to that host is due. A command acting on the box reads and
  writes the box's stamp alone, and one acting on another host never reads or writes the box's.
- **Nothing locks it**, as nothing locks the box's ([Concurrency](#what-nothing-locks)).
- **Never removed by agentspace.** A person MAY delete it to force that host's next push.

## The CA

**The SSH certificate authority of agentspace is one ed25519 key pair on the laptop, owner-only
and without a passphrase (D7, D17, D21).** The operator's `ca` creates it once and checks it on every
later run; what `ca` does and says is [operator.md](operator.md#ca)'s. This section owns its files,
their access rules, and what a usable CA is. Signing leaves the laptop with card #0011 (D17's Q1).

### Its files

| Path | What it is |
|---|---|
| `%USERPROFILE%\.agentspace\ca\` | The CA's folder |
| `%USERPROFILE%\.agentspace\ca\agentspace_ca` | The private key, in OpenSSH's private key format, with no passphrase |
| `%USERPROFILE%\.agentspace\ca\agentspace_ca.pub` | The public key, one line: `ssh-ed25519 <base64> <comment>` |

- **The key pair is ed25519.** A public key whose first word is not `ssh-ed25519` is not a usable
  CA.
- **Its comment is `agentspace-ca <computer name> <date>`**, where `<computer name>` is the
  `COMPUTERNAME` environment variable and `<date>` the local date of creation as `YYYY-MM-DD`.
  Nothing reads the comment.
- **The comment names the caller's machine, and it leaves that machine.** The operator's `sync`
  sends the whole public key line, comment included, to the box ([operator.md](operator.md#sync)),
  so the box holds the laptop's computer name. An implementation MUST NOT spread it further: it
  MUST NOT write the comment, or the public key line that carries it, to a log, to output, to a
  lease's files or to the repo. A doc or a check MUST show it as `<computer name>`, never a real
  one. A rewrite MAY drop the computer name from a CA it creates, since nothing reads the comment.
- **The stack parameter `CaPublicKey` carries the public key without its comment** (D44;
  architect, 2026-11-20, for v0.07-T01, the dev lead's pin P8): its first two fields only,
  `<type> <base64>`, split as [Its checks](#its-checks), check 4, splits them, joined by one space,
  with no comment and no line ending. `deploy -Host`, or `provision`'s deploy step, derives it from
  `agentspace_ca.pub` when it creates a host's stack; a stack that exists keeps its previous value
  ([Files in the checkout](#files-in-the-checkout)). A stack parameter is kept and shown by
  CloudFormation, so the comment MUST NOT reach it. `sync` later replaces the host's copy with the
  public key file's own bytes, as it does on the box (D44).
- **Neither `AGENTSPACE_STATE_DIR` nor any other agentspace variable moves it**; `USERPROFILE`
  does ([What moves which files](#what-moves-which-files)).
- **The folder MUST NOT be inside a folder that OneDrive syncs** (the `OneDrive`,
  `OneDriveConsumer` and `OneDriveCommercial` environment variables), **nor inside any git
  working tree**, compared as paths without regard to case (D17, D37; the prototype refuses only
  its own checkout, [operator.md](operator.md#what-the-prototype-took-from-its-checkout)). The operator's `ca`, which creates the CA,
  enforces this ([operator.md](operator.md#ca)); it is not one of the checks that make a CA usable
  ([Its checks](#its-checks)), so the lease CLI does not make it.
- **Both files MUST NOT be committed**, and the private key MUST NOT be copied anywhere else (D7,
  D17). The public key reaches hosts only through the operator's `sync`
  ([operator.md](operator.md#sync)), never through the repo.
- **The private key SHOULD NOT be loaded into ssh-agent** (D17).

### Its access rules

- **The folder and the private key MUST each be owner-only** ([Access rules](#access-rules)): owned
  by the user's SID, protected, and with exactly one access rule, which allows the user full
  control. A rule the user has that falls short of full control, any other rule, allow or deny,
  and any inherited rule each break it.
- **The public key's rules are not set or checked by agentspace**, and it is not secret. Windows'
  `ssh-keygen` gives it a protected list of its own, whatever the folder passes down: on
  2026-11-18 (OpenSSH_for_Windows 9.5p2, Windows 11 build 26300) that was `SYSTEM` and
  `Administrators` with full control, the user with modify, and `Everyone` with read, seen on a
  throwaway CA made in an owner-only folder as `ca` makes it. So the real CA's public key is most
  likely readable by everyone ([Q6](#open-questions)).
- **They MUST be set before the private key exists:** the folder is locked, and checked, before the
  key pair is made in it, and the private key is locked once it exists, replacing the rules
  `ssh-keygen` gives it (on that same run: `SYSTEM` and `Administrators` full control, the user
  modify). On Windows the prototype does it with `icacls <folder> /inheritance:r /grant:r
  *<user SID>:(OI)(CI)F`, then `icacls <key> /reset`, then `icacls <key> /inheritance:d`, which
  leaves the key one explicit rule, the user's full control, as the same run showed.
- **A CA MUST NOT be created from an elevated process**, since its owner would be
  `Administrators` (D17).

### Its checks

**A CA is usable when all of these hold**, checked in this order, the first failure ending the
check:
1. **Both files exist.** A file where the home folder, `.agentspace` or the CA's folder has to be
   is not a missing CA but one that is not usable, so it gets lease-api.md's "CA not usable"
   message, not "No CA" (architect, 2026-11-19, from E0.05-T02-S02).
2. **The folder, then the private key, are owner-only.**
3. **The private key loads with an empty passphrase**: it is unencrypted, as
   [the key](#the-key) defines it. The prototype runs `ssh-keygen -y` with an empty passphrase.
4. **The public key's first two words, split at whitespace after trimming, equal the first two of
   the public key derived from the private key, exactly, and the first is `ssh-ed25519`.** The
   prototype compares both words without regard to case, which is looser than this and not
   behavior to copy
   ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
   **What "split at whitespace after trimming" means, as `ssh-keygen` reads a public key**
   (architect, 2026-11-19, for E0.06-T04-S03; #0136): the line is trimmed of ASCII whitespace
   (U+0009 to U+000D and U+0020) at both ends, and its words are split at any run of spaces and
   tabs (U+0020, U+0009), so a tab, or several spaces, between the two words keeps the CA usable. A
   line break (U+000A to U+000D) between the words ends the line, so the key has one word on it
   and fails check 4 and check 5 alike, and a non-ASCII space such as U+00A0 is no whitespace here
   and is refused. (The first form of this ruling named U+0009 to U+000D as split characters and
   cited lease-api.md's Input tolerance for them; both were wrong, found in that story's review.)
5. **The public key's fingerprint can be read**: the file parses as an OpenSSH public key. The
   prototype runs `ssh-keygen -l -f`.

- **The order of checks 3 to 5 is not observable once both files exist**, since every failure
  among them gives the same "not usable" message ([lease-api.md](lease-api.md#messages)). A build
  MAY check that the public key parses before it compares the words, and MAY treat a public key
  of fewer than two words, or a private key that is not ed25519, as any of those checks' failures
  (architect, 2026-11-19, for E0.05-T01-S04).
- **Who checks it:** `ca` after every run, `sync` before it sends the public key to a host, `create`
  and `renew` before they sign, `create` before it answers `pending` (architect, 2026-11-20, for v0.07-T01), a
  provisioning run's `place_pending` before it signs a lease's
  certificate, `deploy -Host` before it derives `CaPublicKey`, and every operator command before it
  signs its own certificate for a host (D17, D47; [operator.md](operator.md#ca);
  [The operator's key and certificate](#the-operators-key-and-certificate)). `release` never
  checks it.
- **`create` and `renew` make these five checks and no others.** The folder's place, outside
  OneDrive and any git working tree ([Its files](#its-files)), is `ca`'s to check. The prototype's `create` and `renew`, which run `ca`, also refuse
  a CA in such a folder. That is a difference the spec drops, with the running of `ca` itself
  (#0029, point 3).
- **`create` and `renew` MUST only check, and MUST NOT create a missing CA** (#0029, point 3;
  [lease-api.md](lease-api.md#the-credential)). The prototype checks by running the operator's
  `ca` and reading only its exit code, 0 meaning usable; on a laptop with no CA, that run creates
  one, which no host trusts until `sync`
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
- **A check MUST NOT change the CA.** Nothing in agentspace repairs, loosens, overwrites or
  replaces a CA; a person decides (D17).

### Who reads it

| File | Read by | For |
|---|---|---|
| `agentspace_ca` | `ssh-keygen -s`, run by `create`, `renew` and a provisioning run's `place_pending` | Signing a lease's certificate |
| `agentspace_ca` | `ssh-keygen -s`, run by every operator command that signs its own certificate for a host | Signing the operator's own certificate for that host ([The operator's key and certificate](#the-operators-key-and-certificate)) |
| `agentspace_ca` | `ca`, `sync`, `create`, `renew`, `place_pending`, `deploy -Host`, and every operator command that signs | The checks above |
| `agentspace_ca.pub` | The same | The checks above |
| `agentspace_ca.pub` | `sync` | Sent to each host as its trusted CA, the box included ([operator.md](operator.md#what-sync-installs); D50) |
| `agentspace_ca.pub` | `deploy -Host` | Its first two fields, as the stack parameter `CaPublicKey` ([Its files](#its-files)) |

**Its lifetime:** created once, never removed by agentspace. If the private key is lost or exposed,
a person replaces the CA as D17 says: deletes both files, runs `ca`, runs `sync` for every host,
which `sync -All` does (D50), and re-issues every live lease's certificate.

## The push stamp

**`%USERPROFILE%\.agentspace\localswim-pushed.json` records the last token push the box
confirmed** (D9, D32). What it is the hash of, when a push is due, and when it is written are
operator.md's ([The stamp](operator.md#the-stamp)); this section owns how it is stored. **From
v0.07 the stamp is kept per host (D50):** this file stays the box's, at this path and in this form
(D55), and every other host's is in its own folder
([A host's push stamp](#a-hosts-push-stamp)), under every rule of this section.

- **It is JSON (D40, architect, 2026-11-19, for v0.06):** `{"version":0,"stamp":"<64 hex>"}`,
  where the stamp's digits are uppercase. Its schema is
  [`schemas/state/localswim-pushed.state.schema.json`](../schemas/state/localswim-pushed.state.schema.json),
  carried inside the operator. It is written as UTF-8 with no byte order mark, as compact JSON with
  no line ending. A reader MUST accept a byte order mark and whitespace that JSON allows.
- **A file that cannot be parsed, or does not match the schema, is a stamp that "cannot be read"**
  under operator.md's rule, so the next push is due ([operator.md](operator.md#the-stamp)).
- **The prototype's stamp is `localswim-pushed.sha256`, the 64 hex digits alone**, with no line
  ending. The rewrite MUST NOT read, write or remove that file. So the first `up` after the
  change-over finds no stamp and pushes once, which changes nothing on the box. A person MAY
  delete the old file once the prototype is gone. **The prototype is gone** (removed from the
  repository by E0.06-T06-S01, 2026-11-20), so a person MAY delete it now.
- **Written by:** `up` and `sync` acting on the box (without `-Host`, with `-Host swimcoach`, or
  `sync -All`), only after the box confirms the push, creating `%USERPROFILE%\.agentspace` if
  needed. **Read by:** `up` acting on the box, to decide whether a push is due.
- **It MUST be replaced atomically** ([Replacing a file](#replacing-a-file)), through
  `localswim-pushed.json.<hex>.tmp`. The prototype overwrites its own file in place, so a reader
  can see it empty or part written; the prototype itself warns and pushes nothing then
  ([operator.md](operator.md#the-stamp)).
- **`AGENTSPACE_STATE_DIR` does not move it.**
- **Access rules:** inherited. It covers the token, so it SHOULD be readable by the user alone; the
  prototype's inherits `SYSTEM` and `Administrators` as well
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
  Whether it SHOULD stay an unsalted hash is [operator.md Q27](operator.md#open-questions).
- **No file agentspace keeps on the laptop holds the token** ([operator.md](operator.md#secrecy));
  the stamp is a hash that covers it.
- **Never removed by agentspace.** A person, or a check, MAY delete it or set it aside to force the
  next push ([operator.md](operator.md#the-stamp)). Nothing locks it
  ([Concurrency](#concurrency)).

## The keeper's files

**The keeper holds the board's forwards through one ready lease per host (D30, D50).** What it does
with each
file, when, and why, is keeper.md's: its log lines and its rotation are
[keeper.md#log-lines](keeper.md#log-lines), its locks [keeper.md#locks](keeper.md#locks), and the
launch handshake [keeper.md#starting-a-holder](keeper.md#starting-a-holder). This section owns each
file as a file: its path, format, bytes, access rules, share modes, writer, reader and lifetime.

**Every keeper file but the task's launcher is in `<state folder>`**, the same folder as the lease
state file, so `AGENTSPACE_STATE_DIR` moves them with it. The keeper creates the folder if it is
missing. **Access rules, for every keeper file:** inherited; the keeper sets none.

**Every JSON file the keeper or its launcher writes:**
- **Its bytes:** UTF-8 without a byte order mark, compact JSON with no whitespace outside strings,
  followed by one LF.
- **Replaced atomically** through `<its name>.<hex>.tmp` in the same folder
  ([Replacing a file](#replacing-a-file)): `board-holder.json.<hex>.tmp`,
  `board-holder.<project>.json.<hex>.tmp`, `board-holder-launch.<hex>.json.<hex>.tmp` and
  `board-holder-launch.<hex>.result.json.<hex>.tmp`, where the two `<hex>` are separate values. A rename that fails because the file is in use is
  tried again every 50 milliseconds for up to 5 seconds.
- **Read whole.** A reader SHOULD share read, write and delete; the prototype's keeper and launcher
  share read only, which the writer's retry covers
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
- **Every path in one is full**, never relative ([The home folder](#the-home-folder)), because the
  program that reads it runs with another working folder and another environment.

### The holder record

**`<state folder>\board-holder.json` names the holder the keeper started on the box, and is absent
when there is none.** Every other host's holder has a record of its own in this same form
([Another host's holder](#another-hosts-holder)). Its schema is
[`schemas/state/board-holder.state.schema.json`](../schemas/state/board-holder.state.schema.json),
strict, as a writer MUST write it.

```
{"pid":<int>,"process_started_at":"<time>","lease_id":"<id>","alias":"<alias>"}
```

| Member | Value |
|---|---|
| `pid` | The holder's process ID, 1 to 2147483647 |
| `process_started_at` | The holder's creation time in UTC, truncated to the millisecond, as `YYYY-MM-DDTHH:MM:SS.fffZ` |
| `lease_id` | The ID of the lease it holds through, normalized |
| `alias` | That lease's `ssh.host_alias`, which the holder was started with, in the form [lease-api.md](lease-api.md#fields) gives it |

- **Written by:** a keeper pass that started a holder. **Read by:** every keeper pass. **Removed
  by:** a keeper pass, when the holder it names is stopped, gone or not to be trusted; when, is
  keeper.md's.
- **A reader MUST treat a record that cannot be read or parsed, or whose members do not have these
  forms, as no record.** Like the prototype, it MAY accept a `lease_id` that is any 8-4-4-4-12 hex
  UUID in any case. Its `alias` MUST match `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z`, and a record
  whose alias does not is no record (architect, 2026-11-19, for v0.06, gap FB-23). A reader MUST
  NOT require the line ending a writer puts after the document, and accepts whitespace JSON
  allows (architect, 2026-11-19, for E0.06-T01-S07). It also accepts a UTF-8 byte order mark and a
  member the schema does not name, which it ignores; it keeps `lease_id` as stored, and the keeper
  compares it with a lease's ID without regard to ASCII case. In the keeper lock's PID line,
  "blanks" are ASCII whitespace: space, tab, CR, LF and form feed.
- **The PID and the start time together are the holder's identity**; the record never names a
  process by its PID alone.

### Another host's holder

**The keeper holds one holder per host (D50), and every host but the box has its own record and
its own holder log, named by its project** (D55; architect, 2026-11-20, for v0.07-T01, the dev
lead's pin P6):

| File | What it is | In the form of |
|---|---|---|
| `<state folder>\board-holder.<project>.json` | The record of the holder the keeper started on that host, absent when there is none | [The holder record](#the-holder-record), with its schema, `board-holder.state.schema.json` |
| `<state folder>\board-holder.<project>.json.<hex>.tmp` | Its temporary file | The keeper's JSON files' |
| `<state folder>\board-holder-ssh.<project>.log` | That holder's output | `board-holder-ssh.log` ([The keeper's logs](#the-keepers-logs)) |
| `<state folder>\board-holder-ssh.<project>.log.1` | Its rotation | `board-holder-ssh.log.1` |

- **`<project>` is the host's record's `project`** in [the host registry](#the-host-registry). The
  box's files keep their names, `board-holder.json` and `board-holder-ssh.log`, with no project in
  them, so swimcoach's side reads as v0.06 left it (D55). `board-holder.swimcoach.json` is never
  written.
- **Every rule of the box's file of the same kind binds each**: its bytes, schema, reading rules,
  atomic replace and retry, share modes, access rules, writer, reader and lifetime. The record's
  `lease_id` and `alias` name a lease on that host.
- **A launch request for a holder on that host names that host's log** in its `log` member
  ([A holder's launch files](#a-holders-launch-files)), so the launch files need no project in
  their names.
- **One of each of these serves every host:** `board-holder.log`, the launch files, the two lock
  files and the task launcher's error log. One pass at a time serves every host.
- **`AGENTSPACE_STATE_DIR` moves them**, as it moves every keeper file.
- **A record left for a host the keeper no longer holds through** is removed by the keeper as
  [keeper.md](keeper.md) says; nothing else removes one.

### A holder's launch files

**Each start of a holder uses three files, named by one new `<hex>`**
([keeper.md#starting-a-holder](keeper.md#starting-a-holder) for when each is written and read):

| File | Written by | Read by | Format |
|---|---|---|---|
| `board-holder-launch.<hex>.json` | The pass | The launcher | The launch request, below |
| `board-holder-launch.<hex>.claim` | The launcher that claims the request, or the pass that withdraws it | Nothing in agentspace (a conformance check does) | A claim, below |
| `board-holder-launch.<hex>.result.json` | The launcher | The pass | The launch result, below |

**The launch request**, schema
[`schemas/state/board-holder-launch.state.schema.json`](../schemas/state/board-holder-launch.state.schema.json):

| Member | Value |
|---|---|
| `application` | The full path of the program the holder runs: the ssh program, or `cmd.exe` for a batch stand-in |
| `command_line` | The holder's whole command line |
| `log` | The full path of the holder's log, which its output is appended to: `board-holder-ssh.log` for the box, `board-holder-ssh.<project>.log` for another host ([Another host's holder](#another-hosts-holder)) |
| `directory` | The holder's working folder, as a full path: the home folder |
| `not_after` | The launcher's deadline, UTC, as `YYYY-MM-DDTHH:MM:SS.fffffffZ` (seven fraction digits) |
| `result` | The full path of the result file |

**The starter, which writes the request, writes every path in it with backslashes**, turning each
`/` of the `-SshExe` it was given into `\`, and builds the holder's command line from that same
path, so the request, the record and the matcher agree (architect, 2026-11-19, for E0.06-T02-S04).
The checks of [keeper.md, Before the start](keeper.md#before-the-start) are the pass's, and the
starter does not repeat them.

The request carries full paths so that the launcher, which MUST run with the user's logon
environment and not the pass's
([keeper.md, "Streams and environment"](keeper.md#streams-and-environment)), never reads
`AGENTSPACE_STATE_DIR`.

**The claim** is created with create-new semantics (`CREATE_NEW`), which fails when the file
exists; what holding it means is
[keeper.md#starting-a-holder](keeper.md#starting-a-holder)'s. It is opened for writing, sharing
read. Nothing in agentspace reads its content, but the content is part of the contract, and a
conformance check reads it: UTF-8 without a byte order mark, one
line ending in LF, `launcher <pid>` from a launcher or `abandoned` from the pass.

**The launch result**, schema
[`schemas/state/board-holder-launch-result.state.schema.json`](../schemas/state/board-holder-launch-result.state.schema.json),
is one of two shapes:
- **Started:** `{"pid":<int>,"process_started_at":"<YYYY-MM-DDTHH:MM:SS.fffZ>","command_line":<text or null>}`,
  read from the process the launcher created. `command_line` is the command line read back from
  that process, and is `null` when it could not be read.
- **Not started:** `{"error":"<text>"}`, where `<text>` is not empty.

**Their lifetime.** The pass that wrote the request removes all three before it ends, whatever
happened, in the order [keeper.md#starting-a-holder](keeper.md#starting-a-holder) gives. A
launcher that claims a request it can no longer read removes its own claim. **A request that
does not match its schema, such as one with a relative path or a deadline with other than seven
fraction digits, is one the launcher cannot read** (architect, 2026-11-19, for E0.06-T01-S07). Two cases leave
files that nothing removes later ([Q4](#open-questions)):
- **A keeper killed mid-pass** can leave any of the three, and a temporary file.
- **A late launcher**, one that claimed the request and answers after the pass has stopped waiting
  for it, writes its result file after the pass removed the others. That file stays, and the holder
  it names, if it started one, runs with no record
  ([keeper.md#starting-a-holder](keeper.md#starting-a-holder)).

### The keeper's logs

**Each is a text log, appended to and never rewritten in place; the form of every line, and the
rotation of `board-holder-ssh.log`, are [keeper.md#log-lines](keeper.md#log-lines)'s.** None is
read by agentspace.

| File | Written by | Its bytes | Lifetime |
|---|---|---|---|
| `board-holder.log` | Every keeper pass and the `-Watch` loop | UTF-8 without a BOM, one line per entry, each ending in LF | Never rotated or removed |
| `board-holder-ssh.log` | The holder, through the handle the launcher gives it (stdout and stderr together) | The holder's raw output, in the console's code page | Renamed to `.log.1` by the keeper's rotation ([keeper.md#log-lines](keeper.md#log-lines)) |
| `board-holder-ssh.log.1` | The rotation | As above | Replaced by the next rotation |
| `board-holder-launcher-errors.log` | The task's launcher, only when it refuses its arguments or cannot start the keeper | UTF-8 without a BOM, one line per entry, each ending in CR LF | Never removed |

**Share modes:**
- **`board-holder.log` is opened for append, sharing read and write**, so two writers and any
  reader never block one another.
- **The holder's log handle is opened for append only, sharing read, write and delete**, so a
  rotation can rename the file while an old holder still writes to it.
- **A reader of any keeper log SHOULD share read, write and delete**, so it never blocks a writer
  or a rotation.

**`board-holder-launcher-errors.log` is in `<state folder>` as the task's logon environment sees
it:** `AGENTSPACE_STATE_DIR` there if set, else `.agentspace` in the user's profile folder, which
the prototype finds through the operating system's profile folder, not `USERPROFILE`
([Q2](#open-questions)).

### The keeper's lock files

**Two files whose open handles are the keeper's locks.** What each lock means, who takes it, for how
long, and what a second taker does, is [keeper.md#locks](keeper.md#locks)'s.

| File | Opened as | Content |
|---|---|---|
| `board-holder.lock` | For reading and writing, created if missing, sharing nothing | Empty, and meaningless |
| `board-holder-keeper.lock` | For reading and writing, created if missing, sharing read only | The keeper's PID, as decimal ASCII digits with nothing else; for a second keeper's log line only |

- **The lock is the open handle, never the file's existence or content.** Each file is created if
  missing, never deleted, and a leftover file locks nothing. The operating system releases the
  handle when the process ends, however it ends.
- **The handles MUST NOT be inheritable**, so no child process holds a lock.
- **A reader of `board-holder-keeper.lock`'s PID opens it for reading, sharing read and write.**

### The keeper task's launcher

**`%USERPROFILE%\.agentspace\bin\agentspace-keeper-task.exe` is the no-window program the logon
task runs to start the keeper** (D30, D37; the task is [keeper.md#the-installed-task](keeper.md#the-installed-task)'s,
and the install folder and its programs are
[operator.md, Programs and install layout](operator.md#programs-and-install-layout)'s).
- **Written by:** an install, with the other programs, never by the keeper's installer and never
  into a checkout. An install MUST NOT overwrite it while the task runs it; operator.md's
  [install folder](operator.md#the-install-folder) says how a running program's file is replaced.
  **Read by:** Task Scheduler, which runs it. **Removed by:** nothing in agentspace. The keeper's
  `-Uninstall` leaves it, since it belongs to the install.
- **`AGENTSPACE_STATE_DIR` does not move it.** Access rules: inherited.
- Note on where this came from: the prototype's launcher is
  `%USERPROFILE%\.agentspace\BoardHolderLauncher.exe`, which its keeper install compiles there and
  its `-Uninstall` deletes. The rewrite's installer removes that file
  ([keeper.md, The installer](keeper.md#the-installer)).

## The guard's files

**The guard keeps two files of its own, `guard.log` and `guard.lock`, beside the host registry**
(D56, D63). The guard is `agentspace-operator guard`
([operator.md, guard](operator.md#guard)): it stops each running host that no live lease explains,
and sends each running host its horizon. What it does, when, and why is operator.md's, and when
the keeper starts it is [keeper.md, Starting the guard](keeper.md#starting-the-guard)'s. This
section owns each file as a file: its path, format, bytes, share modes, writer, reader and
lifetime. **The facts the architect ruled on (pins P2 and P3, and the drafter's choices, pin P18)
are cited "architect, 2026-11-21, for v0.08-T01".**

- **Both are in `%USERPROFILE%\.agentspace`**, beside `hosts.json` and `hosts.lock`. **The guard
  creates the folder first if it is missing**, as every writer there does
  ([The home folder](#the-home-folder)).
- **`USERPROFILE` moves them, and `AGENTSPACE_STATE_DIR` does not** (D63 pins both beside
  `hosts.json`), as with `hosts.json` and its lock, since every state folder on the machine shares one set of hosts
  and so one guard ([What moves which files](#what-moves-which-files)).
- **Access rules for both: inherited.** The guard sets none.
- **No other file on the laptop records what the guard sent or when** (D57, as D63 changed it): the
  guard sends every running host its horizon at every run, so nothing has to remember what was
  sent last, and **no laptop file records the horizon sent to a host**, nor the time a host will
  stop, which each run works out again ([Live leases and the lease
  horizon](#live-leases-and-the-lease-horizon)).
- **The guard reads `hosts.json` and the lease state file, and never writes either**
  (D56; architect, 2026-11-21, for v0.08-T01, pin P18), and takes neither file's lock to read it, as the keeper does not
  ([Concurrency](#concurrency)). **A host it stops stays `ready` in the registry**, which records
  whether a host is made and never whether it runs (D45, D56).

### The guard's log

**`%USERPROFILE%\.agentspace\guard.log` records what the guard did to a host that needed
saying: a host it stopped, found unexplained, or failed to stop or to send its horizon** (D56,
D63). Which events write a line, and the exact text of each line, are operator.md's
([operator.md, guard](operator.md#guard)); this section gives the form that every line shares.
- **A line's form is provision.log's and the keeper's** (the dev lead's pin P2; architect, 2026-11-21,
  for v0.08-T01; [The provisioning run's log](#the-provisioning-runs-log),
  [keeper.md](keeper.md#log-lines)): a UTC time in the form `YYYY-MM-DDTHH:MM:SSZ` ([lease-api.md,
  Times](lease-api.md#times)), one space, `host=<project>`, one space, then the event's text:

  ```
  <time> host=<project> <text>
  ```

  - **`<project>` is the host's record's `project`** in [the host registry](#the-host-registry),
    the box's included, as `swimcoach`.
  - **A line that is not about one host carries no `host=` field and no space for it**: it is
    `<time> <text>`. A failed AWS call, an expired AWS sign-in and the guard's own failure are such
    lines.
  - **`<time>` is the time the guard writes the line**, read from the clock for that line, not the
    run's start (architect, 2026-11-21, for v0.08-T01, pin P18).
- **Its bytes:** UTF-8 without a byte order mark, each line ending in one LF. **A line's text MUST
  NOT contain a carriage return or a line feed**; how a message's control characters are written
  is operator.md's ([operator.md, guard](operator.md#guard)).
- **Opened for append, sharing read, write and delete**, as the provisioning run's log is
  ([The provisioning run's log](#the-provisioning-runs-log)), so a reader never blocks the guard and a person's
  tool never blocks a rename or a delete. **A reader SHOULD share the same.** Each line is
  written whole, in one write at the end of the file, never found and then written as two steps.
- **It is created when the guard writes its first line**, if it is missing, after the folder
  (architect, 2026-11-21, for v0.08-T01, pin P18). A run with nothing to log creates nothing, and a missing file is a log
  with no lines.
- **Written by the guard alone**, whoever started it, the keeper or a person. **Read by people.**
  No program of agentspace reads it, not even `status -All`, which works out what it prints again
  from the registry, the lease state file and EC2 (D60).
- **Never rotated or removed by agentspace.** The rotation of
  [keeper.md, Rotation](keeper.md#the-holders-log) is `board-holder-ssh.log`'s alone and does not
  apply to this file: it grows only by one line for each event, and a person MAY delete or move
  it between runs.
- **It MUST NOT hold a secret**, nor a whole `aws` or `ssh` command line, nor the CA's comment
  ([The CA](#its-files); operator.md, Secrecy), as `provision.log` does not
  ([The provisioning run's log](#the-provisioning-runs-log)).
- **The guard's exit codes, and what a guard that cannot write a line does,** are operator.md's.

### The guard's lock

**`%USERPROFILE%\.agentspace\guard.lock` is held for the whole life of a guard run, so a held lock
means a guard is running** (D56, D63). It is held as
[the provisioning run's lock](#the-provisioning-runs-lock) is (the dev lead's pin P3; architect,
2026-11-21, for v0.08-T01):
- **Holding it means having the file open with no sharing**, for reading and writing, creating it
  if it is missing, after creating `%USERPROFILE%\.agentspace` if that is missing. **The file is
  never deleted, its content is empty and meaningless, and the handle MUST NOT be inheritable**,
  so no program the guard starts holds it. The operating system releases it however the run
  ends, so a killed guard leaves the lock free.
- **It is always taken without waiting**: one try, which either has it or finds it held.
  - **The guard takes it as its first step, before it reads any file or opens its log, and holds
    it until it exits.** It takes no other lock of this page to wait for.
  - **A guard that finds it held exits 0 at once and logs nothing** (D63): no line in
    [the log](#the-guards-log) and no other file written. It was started while another run was
    running, which is no error.
  - **The keeper tells whether a guard is running by trying it, and releases it at once if it
    got it**, as the driver tries a provisioning run's lock
    ([The provisioning run's lock](#the-provisioning-runs-lock)). **A try creates neither the file
    nor `%USERPROFILE%\.agentspace`, and a missing one is free**: no guard holds a lock that does
    not exist, and the guard creates the file when it takes it. When the keeper tries it, and what
    it logs, is keeper.md's
    ([guard.lock: tried, never held](keeper.md#guardlock-tried-never-held)).
  - **A failure to open it other than its being held**, such as access denied, or a folder at its
    path, is not a free lock: the guard treats it as a failure of its own and runs nothing, and
    stops no host, and the keeper starts no guard in that pass and logs that, as the dev lead's
    pin P3 puts it (architect, 2026-11-21, for v0.08-T01).
- **The keeper's try is a hint, not a promise** (architect, 2026-11-21, for v0.08-T01, pin P18):
  between the keeper's
  release and a started guard's own take, another guard, such as one a person runs by hand,
  can take the lock, and the started guard then finds it held and exits 0. **So two guards never
  run at once, and a second is harmless.**
- **`AGENTSPACE_STATE_DIR` does not move it** (D63 pins it beside `hosts.json`), so one guard runs at a time
  on the machine, whichever state folder started it.
- **Where it sits among the locks** is [The order of the locks](#the-order-of-the-locks): it is
  taken first and held throughout, and nothing waits for it.

## Files agentspace reads and never writes

**These belong to the user, to ssh, to the AWS CLI or to localswim**, but for the project list,
which a person writes in agentspace's folder and whose format is this page's
([The project list](#the-project-list)). agentspace reads them, or
starts a program that reads them, and MUST NOT write,
move or delete them. Their formats are their owners', and this page names only what agentspace
relies on. This settles [operator.md Q8](operator.md#open-questions): docs/state.md lists them,
and owns none of their formats but the project list's.

| Path | Owner | Read by | What agentspace relies on |
|---|---|---|---|
| `%USERPROFILE%\.ssh\config`, outside the `Include` line | The user | `ssh` | The user's `agentspace` entry ([operator.md](operator.md#the-agentspace-ssh-entry)) |
| `%USERPROFILE%\.ssh\known_hosts` | `ssh` | `ssh` | The box's host key under `agentspace` ([The host key](#the-host-key)) |
| `%USERPROFILE%\.agentspace\known_hosts` | `ssh` | `ssh` | Every other host's key under `agentspace-host.<project>` ([Agentspace's host keys](#agentspaces-host-keys)) |
| `%USERPROFILE%\.agentspace\projects.json` | A person, or the architect with Terry's yes | The driver and `provision` | The projects that may have a host made ([The project list](#the-project-list)) |
| `%USERPROFILE%\.ssh\id_ed25519` | The user | `ssh`, through the `agentspace` entry | Logging in to the box as the user, not as a lease |
| `%USERPROFILE%\.ssh\id_ed25519.pub` | The user | The operator's `deploy` | One line, the user's public key, trimmed ([operator.md](operator.md#deploy)) |
| `%USERPROFILE%\.claude\CLAUDE.md` | The user | The operator's `sync`, and `provision` through it | Copied to each host as it is, the box included ([operator.md](operator.md#sync); D50) |
| `%USERPROFILE%\.aws\` | The user, through the AWS CLI | The AWS CLI that the operator and the ProxyCommand run | The AWS sign-in; agentspace reads nothing in it itself. The AWS CLI finds it through `USERPROFILE`, so moving that variable loses the sign-in ([What moves which files](#what-moves-which-files)) |
| `%LOCALAPPDATA%\localswim-v2\service.json`, the receipt | localswim: its Launch block writes the receipt after a cold start (localswim's `docs/windows-operations.md`, "The receipt") | The operator's token push, in `up` and `sync` | Its `base` and `credential_directory` members. The format is localswim's, and what the push checks in it is [operator.md, The receipt](operator.md#the-receipt)'s |
| `bot.json` in the folder the receipt's `credential_directory` names | localswim: its provisioning block makes it beside the Human credential, `human.json`, at each cold start (localswim's `docs/windows-operations.md`, "The credential files") | The token push, after the receipt | The Bot token in its `token_dpapi_hex` member. The format is localswim's, and what the push checks in it is [operator.md, The Bot credential](operator.md#the-bot-credential)'s |

**The two localswim files are the only ones agentspace reads under `%LOCALAPPDATA%`**, and they are
localswim's own state, not agentspace's, so D21 does not move them. `LOCALAPPDATA` alone moves them
([What moves which files](#what-moves-which-files)). Their formats, as far as agentspace relies on
them, are operator.md's ([What the laptop reads](operator.md#what-the-laptop-reads)), because
localswim's own documentation does not state them in full. **Inside the Claude app, a read at that
path can be served from the app's package cache rather than the real file (D21)**, so what a
process inside the app finds there proves nothing about the real files.

## Files in the checkout

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository, so the
prototype's column and its notes below are history.** The rewrite's rules bind, as they did before.

**The rewrite keeps nothing in a checkout: no program of agentspace reads or writes one at run
time** (D37; [operator.md, What the prototype took from its checkout](operator.md#what-the-prototype-took-from-its-checkout)).
The prototype's operator keeps three files in the checkout it runs from, and the prototype's
programs read the stack's template, the box's files and the state schemas from there too. This
section gives each file's place in the rewrite, all under `%USERPROFILE%\.agentspace`, and its
format. None is committed.
- **Only the instance-ID cache was state, and D21 requires it under `%USERPROFILE%\.agentspace`**
  (architect, 2026-11-18, which settles [Q1](#open-questions)); the prototype keeps it in the
  checkout, and [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
  lists that. **v0.07 retires the cache into the host registry's `instance_id`** (D45; below).
- **D21 lets build output stay in a checkout, and the rewrite still keeps none there**, since a
  compiled operator has no checkout to find. The operator's folder holds what it writes out for
  the programs it starts.

| Path | The prototype's | Format | Written by | Read by | Removed |
|---|---|---|---|---|---|
| `%USERPROFILE%\.agentspace\instance-id.json` | `<checkout>\.instance-id`, the ID alone in ASCII | Retired by v0.07 (D45); v0.06's JSON, below | Nothing, from v0.07 | Nothing, from v0.07; a host's instance ID is its record's `instance_id` ([The instance ID](#the-instance-id)) | Never by agentspace; a person MAY delete it |
| `%USERPROFILE%\.agentspace\stack-parameters.json` | `<checkout>\infra\local.parameters`, `KEY=VALUE` lines | JSON; see below | A person | Plain `deploy` of the box, for every parameter but `InstanceType`, which no program reads from v0.09 (D67, D71); `deploy -Host` and `provision` when they create a host's stack, for `SubnetId` and `VpcId` alone | Never |
| `%USERPROFILE%\.agentspace\operator\<hex>\agentspace.yaml` | None: `deploy` names `<checkout>\infra\agentspace.yaml` | The stack's template, the bytes the operator carries, built from the repository's `infra\agentspace.yaml` | `deploy`, or `provision`'s deploy step, just before the call that reads it | The AWS CLI's `cloudformation deploy` | With its command's folder |
| `%USERPROFILE%\.agentspace\operator\<hex>\system\` | None: `sync` copies `<checkout>\box\system\` | The box's system files, as the operator carries them, built from the repository's `box\system` | `sync`, or `provision`'s sync step, just before the copy | `scp`, which copies the folder to the host | With its command's folder |
| `%USERPROFILE%\.agentspace\operator\<hex>\localswim-linux-arm64` | `<checkout>\.build\localswim-linux-arm64` | The localswim CLI built for today's box, and so for every host, since each is from the one template | `sync`'s build, or `provision`'s | `scp` | With its command's folder |
| `%USERPROFILE%\.agentspace\operator\<hex>\bootstrap.sh` | None: a person ran `<checkout>\box\bootstrap.sh` on the box by hand | The bootstrap, the bytes the operator carries, built from the repository's `box\bootstrap.sh` (D46) | `bootstrap` and `provision`, just before the copy, when the operator sends the script as a file (operator.md's choice; architect, 2026-11-20, for v0.07-T01) | `scp` | With its command's folder |

- **The operator's folder, `%USERPROFILE%\.agentspace\operator`, holds one folder per operator
  command that sends a file, `operator\<hex>\`** (architect, 2026-11-20, for v0.07-T01), where
  `<hex>` is new for each command. A command creates the operator's folder if it is missing,
  creates its own folder in it, writes there only what it sends, and removes its own folder, with
  everything in it, as it ends, whether or not it succeeded. **So a detached provisioning run and a
  person's command never share a folder**, and no command empties or replaces a file another
  command is sending.
- **A folder in `operator\` older than 24 hours, left by a killed command, is removed by the next
  operator command that finds it** (architect, 2026-11-20, for v0.07-T01). Its age is counted
  from its creation time, and only a folder whose name is 32 lowercase hex digits is removed this
  way; any other file or folder there is left alone (architect, 2026-11-20, for v0.07-T01).
- **`AGENTSPACE_STATE_DIR` does not move the operator's folder or the stack parameters file**
  ([What moves which files](#what-moves-which-files)). Access rules: inherited.
- **Nothing reads the files in a command's folder but the programs that command starts**, at the
  step that wrote them. Each command writes its own files fresh, so the box and the stack are
  changed only through the repository and a new build.

- **The instance-ID cache, `%USERPROFILE%\.agentspace\instance-id.json`, is retired by v0.07
  (D45).** A host's instance ID is cached in its host registry record's `instance_id` instead
  ([The instance ID](#the-instance-id)).
  - **From v0.07 no program of agentspace reads, writes, checks for or removes the file**, nor its
    temporary files, and nothing migrates from it: the box's record has its instance ID read from
    the `agentbox` stack again, as a missing cache had.
  - **A laptop's old copy is left alone**, whatever it holds: it is never read as the box's
    instance ID, never rebuilt and never deleted by agentspace. A person MAY delete it.
  - **What the file was, as history (D40, architect, 2026-11-19, for v0.06):**
    `{"version":0,"instance_id":"<id>"}`, UTF-8 with no byte order mark, compact JSON with no line
    ending, replaced atomically through `instance-id.json.<hex>.tmp`, with its schema in
    [`schemas/state/instance-id.state.schema.json`](../schemas/state/instance-id.state.schema.json).
    Its name was this page's choice ([Q14](#open-questions)). The rewrite never read the
    prototype's caches, `<checkout>\.instance-id` or a plain `instance-id` under
    `%USERPROFILE%\.agentspace`, and v0.07's programs read none of the three.
- **The stack parameters file, `stack-parameters.json`, holds the account's stack parameters, so it
  MUST NOT be committed** (D40, architect, 2026-11-19, for v0.06; the prototype's is
  `<checkout>\infra\local.parameters`).
  - **Its form is `{"version":0,"parameters":{"<Name>":"<value>", ...}}`**, with its schema in
    [`schemas/state/stack-parameters.state.schema.json`](../schemas/state/stack-parameters.state.schema.json),
    carried inside the operator. Each member of `parameters` is passed to CloudFormation as it is,
    and every value is a string, a `Number` parameter's included, as CloudFormation takes it. The
    repository's `infra\stack-parameters.example.json`, which is committed, shows the form.
  - **A person writes it, so a reader MUST accept a UTF-8 byte order mark, and whitespace and line
    endings that JSON allows.** agentspace never writes it.
  - **A file that cannot be parsed, or does not match the schema, fails `deploy` before it calls
    AWS**, with the message operator.md gives ([deploy](operator.md#deploy)). Nothing is skipped
    silently, unlike the prototype's lines, where any line not of the form `<Name>=<value>` is
    skipped.
  - **The rewrite MUST NOT read `infra\local.parameters`.** Moving its values into
    `stack-parameters.json` is a one-time step of v0.06's cutover, and no agentspace program
    converts it.
  - **One file serves every host in v0.07, and its form does not change, but a host is never sized
    by the box's parameters** (architect, 2026-11-20, for v0.07-T01; D55), **and from v0.09 no
    host is sized by the file at all** (D67, D71). Whose values the file supplies, and when:
    - **Plain `deploy` of the box, and the box's provisioning run's deploy step,** pass
      `SshPublicKey` and every parameter in it but `InstanceType`, which keeps its previous value
      unless `-InstanceType` gives one (D67, D71; architect, 2026-11-23, for v0.09-T01, pin P6;
      [operator.md, deploy](operator.md#deploy)). Before v0.09 they passed every parameter in it
      (D39).
    - **When `deploy -Host <project>`, or `provision`'s deploy step, creates a host's stack**, the
      file supplies `SubnetId` and `VpcId` alone. Every other parameter in it, `InstanceType`
      among them, is not used for that stack; the others are the template's defaults, or the
      values operator.md gives (`SshPublicKey`, `HostName`, `CaPublicKey`).
    - **For a host's stack that exists**, `deploy -Host <project>` and `provision`'s deploy step
      pass every parameter's previous value, and read nothing from the file.
    - **A host's size changes only through its stack** (D39, D40, D67): through
      `deploy -InstanceType <type>`, for every host, the box included (`-Host swimcoach`, or no
      `-Host`), and through a provisioning run's deploy step, which passes the type its project's
      `pending` leases need ([The instance type](#the-instance-type)). The command lines are
      [operator.md](operator.md#deploy)'s; this page says only whose values the file supplies.
  - **The file's `InstanceType` is not passed and not read** (D67, D71; pin P6). No program of
    agentspace passes it, or uses its value, for any host, so the box's next plain `deploy` cannot
    put back a size a lease set. A file that holds it is left as it is, the member included:
    agentspace never writes the file, and the member is ignored. The file is still checked against
    its schema as a whole before `deploy` calls AWS (above), and the schema's constraints do not
    change, so the member still MUST be of its form; only the schema's description says it is
    ignored (architect, 2026-11-23, for v0.09-T01, pin P16). README and
    [operator.md](operator.md#deploy) say it is ignored. This replaces operator.md's P13 rule (its
    Q31) that the box is sized through this file (D67).

## Project tooling

**These files are written under `%USERPROFILE%\.agentspace` by the project's own scripts, not by
agentspace.** They are not part of the product, and the rewrite MUST NOT make them.

| Path | Written and read by | Format | Removed |
|---|---|---|---|
| `%USERPROFILE%\.agentspace\watchdog\<lead>.last-status` | The architect's watchdog (D22) | A UTC time, ISO 8601 with seven fraction digits and `Z`, no line ending; the file's modification time is the timer | Never |
| `%USERPROFILE%\.agentspace\swimcoach-last.json` | The milestone reporter (AGENTS.md) | One JSON object per milestone name, with `active`, `polled`, `at` and `lanes` | Never |
| `%USERPROFILE%\.agentspace\tmp-test-lease-alias-<time>\` | The prototype's alias test, which was removed by E0.06-T06-S01 (2026-11-20) | A throwaway folder | By the test, at its end. A folder a killed run left is removed by hand |

**The prototype's own checks also created real lease folders under
`%USERPROFILE%\.agentspace\leases`, signed with the real CA, and deleted exactly those (history:
the checks were removed by E0.06-T06-S01, 2026-11-20).** Every
other check works under `%TEMP%`, through `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR`. A
conformance check MUST instead set `USERPROFILE` ([What moves which files](#what-moves-which-files)),
and MUST NOT touch the real profile (D34).

## Concurrency

### The lease state lock

**`<state folder>\leases.lock` serializes every change to the lease state file, and with it every
change to a lease's credential and alias.** `create`, `renew` and `release` take it before they
read the state file, and hold it until the file is written, so the driver's work on the lease
folder, the `Include` line and the alias file all happens under it. Each first creates
`<state folder>` if it is missing, then opens the lock in it. **A provisioning run's
`place_pending` takes it the same way, once for each lease it places**, waits for it as a lease
command does, and holds it from before it re-reads the state file for that lease until after it
has written that lease's record, so no hold spans several placements (architect, 2026-11-20, for v0.07-T01;
[driver.md](driver.md#place_pending)).

- **Holding the lock means having `leases.lock` open with no sharing**: on Windows, share mode 0
  (`FileShare.None`), for reading and writing, creating the file if it is missing. **Every
  implementation on the same machine MUST lock this way**, so that the prototype and the rewrite
  exclude each other (#0054). A byte-range lock, or a lock file whose existence is the lock, would
  not exclude the other.
- **The file is never deleted, and its content is empty and meaningless.** The operating system
  releases the lock when the handle closes, however the process ends, so a crashed command never
  leaves the lock held.
- **A command that cannot open it because another process holds it MUST keep trying until 30
  seconds have passed, and MUST NOT wait longer** (architect ruling, 2026-11-18). The prototype
  tries again about every 50 milliseconds, and a rewrite MAY pick its own interval. What a command
  does when the time runs out is [lease-api.md's](lease-api.md#concurrency-and-atomicity) to say.
- **Only a failure because the file is held is tried again** (on Windows, a sharing or lock
  violation). **Any other failure to open it, such as access denied, or a folder at its path, MUST
  fail the command at once with `internal`**, in agentspace's own words that the lease records
  could not be locked, and change nothing; it is never reported as busy. The prototype also fails
  at once on access denied and on a folder at the path, with its runtime's own text, which is not
  behavior to copy (#0041, point 1).
- **`create` holds it through the whole of issuing**, the CA check and signing included, which
  makes a concurrent `renew` or `release` wait ([lease-api.md Q11](lease-api.md#open-questions)).
- **`get` and `list` MUST NOT take it, and the keeper, the ProxyCommand, the guard and
  `status -All` MUST NOT take it either** (the guard and `status -All` from v0.08; architect,
  2026-11-21, for v0.08-T01, pin P18).

### The host registry lock

**`%USERPROFILE%\.agentspace\hosts.lock` serializes every change to [the host
registry](#the-host-registry)** (D45, D55), and follows every rule of
[the lease state lock](#the-lease-state-lock) (architect, 2026-11-20, for v0.07-T01, the dev
lead's pin P10):
- **Holding it means having `hosts.lock` open with no sharing**, for reading and writing,
  creating it if it is missing, after creating `%USERPROFILE%\.agentspace` if that is missing.
  The file is never deleted, its content is empty and meaningless, and the handle MUST NOT be
  inheritable.
- **A taker that cannot open it because another process holds it keeps trying for up to 30
  seconds, and MUST NOT wait longer.** Any other failure to open it fails the taker at once, and is
  never reported as busy.
- **`get`'s `check_pending` tries it without waiting**, once, and starts nothing when it is held,
  so `get` never waits for it (architect, 2026-11-20, for v0.07-T01); the rest of what it does is
  [driver.md](driver.md#check_pending)'s.
- **Every writer of `hosts.json` holds it from before it reads the file until after the rename**,
  and holds it for no longer than its decision and its write take: never while a provisioning run
  runs, and never across a call to AWS. While it holds it, it waits for no other lock. A reader
  of the project list for a project with no host reads it under this lock too
  ([The project list](#the-project-list)), and a command that makes a host's operator key makes it
  under this lock ([The operator's key and certificate](#the-operators-key-and-certificate)).
- **The keeper, the guard and `status -All` MUST NOT take it, and the ProxyCommand takes it only
  to write a missing `instance_id`** ([The instance ID](#the-instance-id); the guard and
  `status -All` from v0.08, architect, 2026-11-21, for v0.08-T01, pin P18). Each reads
  `hosts.json` without it, seeing the file as it was before or after a change
  ([Replacing a file](#replacing-a-file)).
- **`list`, `renew` and `release` never take it.**
- **Every state folder shares it**: it is not moved by `AGENTSPACE_STATE_DIR`.

### The order of the locks

**Three of agentspace's locks can be held together, and are waited for only in this order:
[the provisioning run's lock](#the-provisioning-runs-lock), then [the lease state
lock](#the-lease-state-lock), then [the host registry lock](#the-host-registry-lock)**
(architect, 2026-11-20, for v0.07-T01, the dev lead's pin P10). A program that holds a later one
MUST NOT wait for an earlier one, so no two programs wait for each other.
- **A try without waiting is outside the order**, and cannot deadlock: the driver, holding the
  lease state lock and the host registry lock in `create`, MAY try a host's provisioning run's lock
  and releases it at once, and `get`'s `check_pending` tries the host registry lock without
  waiting.
- **A provisioning run ends in this order:** holding its own lock throughout, it takes the host
  registry lock alone to record `ready` and the instance ID, and releases it; then `place_pending`
  takes the lease state lock once for each lease it places, releasing it after each; then the run
  releases its own lock. It never holds the host
  registry lock while it waits for the lease state lock.
- **A `deploy` records `instance_type` under the host registry lock alone** (D71;
  [The instance type](#the-instance-type); architect, 2026-11-23, for v0.09-T01, pin P11): it takes
  it for each removal and again for the write, and holds it across no call to AWS. **A
  provisioning run's deploy step does so holding its own lock, and reads the lease state file for
  its type without the lease state lock** (architect, 2026-11-23, for v0.09-T01, pin P13), as the
  guard reads it, so it waits for no lock but the host registry lock, the last in the order, and
  the run still takes the lease state lock only in `place_pending`. A `deploy` run by hand holds no
  other lock of this page's.
- **The keeper's two locks are outside this order**: the keeper takes none of these three, and no
  holder of these three takes the keeper's.
- **[The guard's lock](#the-guards-lock) is the first lock a guard takes, and nothing waits for
  it** (architect, 2026-11-21, for v0.08-T01, pin P18). Both of its takers only try it, the guard to hold it and the
  keeper to learn whether it is held, so it is outside the order of waits, as a try is, and cannot
  deadlock. **A guard holds it from its first step until it exits, and throughout that time:**
  - **It tries each host's provisioning run's lock without waiting, and releases it at once if it
    got it**, as the driver does, to learn whether a run is running on that host
    ([operator.md, guard](operator.md#guard)). It never waits for it.
  - **It reads `hosts.json` and the lease state file without their locks**, as the keeper does, so
    it takes neither the host registry lock nor the lease state lock, and never writes either
    file.
  - **So a guard waits for no lock of this page**, and no holder of one waits for the guard's.
    The keeper tries the guard's lock and releases it at once, and takes none of the three above,
    so the keeper's rule above stands.

### What two commands at once can see

| One is running | Another sees |
|---|---|
| `create`, `renew` or `release` | `get`, `list` and the keeper see the state file as it was before or as it is after, never part of it, and never wait ([Replacing a file](#replacing-a-file)). A second `create`, `renew` or `release` waits for the lock |
| `create`, between writing the alias file and writing the record | `ssh` can already use the alias, while `get` does not yet show the lease. If `create` then fails, the alias and the lease folder are removed again ([Creation, in order](#creation-in-order)) |
| `renew` | `ssh` reads the old certificate or the new one, never a part of either, since the new one arrives by one rename |
| `release` | `ssh` finds the alias file gone before the key, never the reverse. A session already open through the alias is ended by `release`'s step 3 ([Release, and what it leaves](#release-and-what-it-leaves)). `get` sees the lease `released` before its files are gone, in the order this page asks of the rewrite, and after, in the prototype's ([Release, and what it leaves](#release-and-what-it-leaves)) |
| Commands with different `AGENTSPACE_STATE_DIR` values | Two separate state files with separate locks, but one set of lease folders and, with one `<ssh folder>`, one `agentspace.d`. They do not exclude each other. IDs are random, so their folders never collide, but their aliases can; the second `create` then fails on the first's alias file ([The alias file](#the-alias-file)) |
| Two `create`s, or a `create` and a `get`, for one project with no `ready` host | A second `create` waits for the host registry lock; a `get` tries it without waiting, and starts nothing when it is held. Either, once it has the lock, finds the record the first added or changed, and starts no second run: the first run holds its lock, or `last_attempt_at` is less than 5 minutes old ([Its states](#its-states)). Both leases are `pending`, and one run places both |
| A provisioning run's `place_pending`, and a `create`, `renew` or `release` | Each waits for the lease state lock, which `place_pending` takes once for each lease it places, re-reading the state file each time, so no hold of it spans several placements. A lease released before `place_pending` re-reads the state file for it is skipped and never placed; one released after it was placed is released as any `ready` lease is |
| A provisioning run, and `get` or `list` | `get` and `list` see a placed lease `pending` before the run's write and `ready` after it, never part of either |
| A provisioning run, and the ProxyCommand or the keeper | They read `hosts.json` and the lease state file without their locks, so they see each as it was before one of the run's writes or as it is after it, never part of either. A record the run makes `ready` comes before any lease it places, so neither finds a `ready` lease on a host that is not yet `ready` |
| A provisioning run, and an operator command on the same host by hand | Their writes to `hosts.json` wait for each other at its lock. Whether the command runs at all while the run's lock is held is operator.md's ([The provisioning run's lock](#the-provisioning-runs-lock)) |
| A provisioning run's deploy step, and a `create` for the same project (v0.09) | The run reads the lease state file without its lock, once, at its deploy step (pin P13), so a lease that `create` records after that read is not in the run's type. While the project has a `pending` lease a run would place, place answers the new lease `pending` (D66's change note; pins P2(b) and P20), and the run's `place_pending` places it if the host the run leaves meets it, answers it `unsatisfiable` with `no_capacity` when a live `ready` lease holds the host, and otherwise leaves it `pending` for the next run (D68's change note; pin P1; [The instance type](#the-instance-type)) |
| A `deploy`, and a `create` that places a lease of the same host (v0.09) | `create` reads the record under the host registry lock, so it sees `instance_type` as it was before one of `deploy`'s writes or as it is after it. From a removal to the write it sees none, which meets no size, and answers `pending` as place does for such a record (pin P2); a `deploy` that passes `InstanceType` removes it before `cloudformation deploy` runs (pin P11), so no `create` reads a type the deploy is changing ([The instance type](#the-instance-type); [driver.md, The EC2 driver](driver.md#the-ec2-driver)) |
| Commands with different `AGENTSPACE_STATE_DIR` values, from v0.07 | One host registry, one lock on it, one project list and one folder per host, but separate lease state files. A run places only the pending leases of the state file of the command that started it ([The provisioning run's start](#the-provisioning-runs-start)) |
| A guard run, and a second guard run | The second finds [the guard's lock](#the-guards-lock) held, exits 0 at once and logs nothing, whether a keeper or a person started either. One guard runs at a time on the machine |
| A guard run, and `create`, `renew`, `release` or a provisioning run's `place_pending` | The guard reads the lease state file without its lock, so it sees it as it was before a change or as it is after, never part of either ([Replacing a file](#replacing-a-file)), and never makes a change wait. A lease created or released an instant after the guard read the file is seen by the next run, about 5 minutes later, while a keeper runs ([keeper.md, When the keeper starts it](keeper.md#when-the-keeper-starts-it)) |
| A guard run, and a provisioning run on a host | The guard tries the host's [provisioning run's lock](#the-provisioning-runs-lock) and releases it at once, and what it does when the lock is held is operator.md's ([operator.md, guard](operator.md#guard)). It never waits for the run, and the run never waits for the guard |
| A guard run, and the keeper | The keeper tries the guard's lock and releases it at once. A guard a person starts during that try finds it held and exits 0, and the next run does the work (the keeper's try is a hint, [The guard's lock](#the-guards-lock)). Neither waits for the other |
| A person editing `<ssh folder>\config` | The `Include` line is inserted only when it is missing. A save made after `create` read the config and before its backup fails the backup's hash check, and `create` changes nothing; a save made after the backup and before the replace is lost, though the backup holds the earlier content ([Q5](#open-questions)) |

### What nothing locks

- **The push stamp, the box's and every host's.** Two `up`s at once on one host can both push and
  both write its stamp, last writer winning; each writes the same value
  ([operator.md Q20](operator.md#open-questions)). Commands on different hosts write different
  stamps.
- **A host's operator certificate and operator alias.** Two commands on one host at once can each
  sign and move a certificate, and each write the same alias bytes; each move is one rename, so ssh
  reads a whole file either way. The key itself is made under the host registry lock
  ([The operator's key and certificate](#the-operators-key-and-certificate)).
- **The provisioning run's log**, which every writer appends to, sharing.
- **The guard's log**, which the guard alone appends to, sharing read, write and delete. The
  guard's lock keeps a second writer out, so the log is locked by nothing of its own
  ([The guard's log](#the-guards-log)).
- **A provisioning run's read of the lease state file for its type** (v0.09). The deploy step reads
  it without the lease state lock, as the guard does, and acts on the leases as they were before or
  after a change ([The instance type](#the-instance-type); architect, 2026-11-23, for v0.09-T01,
  pin P13).
- **The operator's folder.** From v0.07 two operator commands can run at once, such as a
  provisioning run beside a person's `sync`, and need no lock there: each writes only its own
  `operator\<hex>\` ([Files in the checkout](#files-in-the-checkout); [Q15](#open-questions)).
- **The CA.** Two `ca`s that both find no CA can both try to create it; the second `ssh-keygen`
  fails on the first's key, and `ca` then fails. Nothing else writes the CA.
- **The keeper's files outside its two locks.** The keeper's locks, and how the keeper and its
  launcher agree through the launch files, are [keeper.md#locks](keeper.md#locks) and
  [keeper.md#starting-a-holder](keeper.md#starting-a-holder). The keeper reads the lease state file
  without the lease state lock, so it acts on the leases as they were before or after a change.

## Requirements the prototype does not meet yet

**Behavior this page requires that the prototype does not show, so a conformance check run against
the prototype is expected to fail here.** **The rewrite MUST NOT copy the prototype's behavior in
this table.**

**The last column is what v0.05's Rust build shows (E0.05-T04-S01, 2026-11-19, [the run record](runs/e0.05-t04-s01.md)).**
The rows stay, because they describe the prototype, which ran through `bin\` until v0.06 (#0055)
retired it (D37, D38; architect, 2026-11-19). **Status (E0.06-T06-S01, 2026-11-20): the prototype
was removed from the repository, so the table is history.** It records what the rewrite was built to
do differently, and each row's requirement still binds the programs that are built now. A row is marked yes only where the conformance file it names checks that
row's MUST, and "Not shown" says that no file does. "Not a lease CLI row" is a row of the operator or the
keeper, which the Rust build does not have yet.

| Requirement | The prototype | Card or question | Met by v0.05's Rust build |
|---|---|---|---|
| A stored `ready` record with no `driver` is renewed and released, by the v0 default driver | Fails with `internal` | #0041, point 2 ([The lease state file](#the-lease-state-file)) | Yes: `conformance/lease-cli/test-unmet-record-no-driver.ps1` |
| A record whose `driver` names no known driver fails with `internal`, and the message does not name the driver | The message names the stored driver | #0041, point 2; D1 | Yes: `conformance/lease-cli/test-unmet-unknown-driver.ps1` (E0.06-T04-S03): `renew` and `release` fail with `internal`, change nothing, and neither the message nor any output names the stored driver. 8 of 8 results pass against the Rust build of v0.06-T04; 2 FAIL against the prototype, whose message names the driver |
| `create` and `renew` check the CA and never create it | Run the operator's `ca`, which creates a CA when there is none | #0029, point 3 ([The CA](#its-checks)) | Yes: `conformance/lease-cli/test-unmet-no-ca.ps1` |
| The CA check compares the public key's two words exactly | Compares them without regard to case | [The CA](#its-checks) | Yes, in part: `conformance/lease-cli/test-ca-public-key-words.ps1` (E0.06-T04-S03): a first word in upper or mixed case, a second word with its letters' case swapped, a first word that is not `ssh-ed25519`, a line break (LF, CR, VT, FF or CRLF) between the two words and a no-break space (U+00A0) before the first word each fail `create` and `renew` with `internal` and change nothing, and the same words with another comment, surrounding whitespace, or a tab or two spaces between the two words (each also for `renew`; #0136) are accepted. 37 of 37 results pass against the Rust build once `check_ca_keys` reads the key as `ssh-keygen` does, in one reading for checks 4 and 5 (architect, 2026-11-19, as corrected in [The CA](#its-checks), check 4: trimmed of ASCII whitespace, split at spaces and tabs only). **The prototype passes it too:** it also fails each of those through the later parse check, so the file cannot tell its looser comparison from the exact one |
| A failed `renew` leaves the previous certificate in place | Leaves the new certificate when the record cannot be written | #0041, point 3 ([Renewal](#renewal)) | Yes: `conformance/lease-cli/test-unmet-renew-keeps-certificate.ps1` |
| A failed `release` never leaves a `ready` record with its alias, key or certificate gone | Deletes the alias, then the folder, then writes the record, so a failure to delete the folder leaves a `ready` record with no alias | #0041, point 3 ([Release, and what it leaves](#release-and-what-it-leaves)) | Yes: `conformance/lease-cli/test-unmet-release-partial.ps1` |
| `release` of a `released` record removes the files a failed release left | Returns the record unchanged and touches no file | #0041, point 3 | Yes: `conformance/lease-cli/test-unmet-release-released.ps1` |
| The certificate's valid-after time is 5 minutes before its issue time (SHOULD) | It is the issue time | #0029, point 1 ([The certificate](#the-certificate)) | Not shown: no conformance file checks it (#0110) |
| Every part of agentspace finds the home folder through `USERPROFILE` | The task launcher uses the operating system's profile folder; `deploy` and `sync` use the shell's home variable | [Q2](#open-questions) | Yes, in part: `conformance/lease-cli/test-home-userprofile.ps1` (E0.06-T04-S03) shows the lease CLI's files under `USERPROFILE` with the state and ssh folder variables unset and empty, and the harness's CA, made by the operator's `ca` under it, is in every check that places a ready lease. **Not seen from outside, and why:** the operator's `deploy` and `sync` and the keeper task's launcher, because showing them needs the real `go`, `ssh-keygen` and Task Scheduler (operator.md Q30); and a run with `USERPROFILE` unset, which the harness refuses to give a program |
| Every path written into a file is a full path | Writes a relative `USERPROFILE`, `AGENTSPACE_STATE_DIR` or `AGENTSPACE_SSH_DIR` into the alias file and a launch request as given | [The home folder](#the-home-folder) | Yes, in part: `conformance/lease-cli/test-relative-folders.ps1` (E0.06-T04-S03): a relative `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` are resolved against the working folder, the alias file's paths are full, and the full paths find the lease from another working folder. 5 of 5 results pass against the Rust build, and against the prototype. The holder's launch request is checked by `conformance/keeper/test-unmet-launch-request-full-paths.ps1` (E0.06-T04-S03), which starts a keeper with a relative `AGENTSPACE_STATE_DIR` and requires the request's `log`, `directory` and `result` to be full paths. **Not shown, and why:** a relative `USERPROFILE`, which the harness refuses to give a program. The lease CLI's own files do not see the prototype's writing of a relative path as given, nor the Rust build's making it full |
| Readers of the keeper's JSON files share read, write and delete (SHOULD) | They share read only; the writer's 5-second retry covers it | [The keeper's files](#the-keepers-files) | Not a lease CLI row |
| The instance-ID cache is `%USERPROFILE%\.agentspace\instance-id.json`, in JSON (D40) | `.instance-id` in the checkout, the ID alone | D21; [Q1](#open-questions) ([Files in the checkout](#files-in-the-checkout)) | Yes, in part: `conformance/operator/test-unmet-proxy-steps.ps1` and `test-unmet-push.ps1` seed the cache at that place and require the cached instance to be the one used (11 of 11 and 20 of 20 results pass against the Rust build of v0.06-T04), and `conformance/lease-cli/test-lease-ignores-instance-cache.ps1` (E0.06-T04-S03) shows the lease CLI never needs it: a cache that cannot be read (a folder where the file is, a file that is not JSON) changes no command's outcome, and neither does a valid one. Not shown: whether the CLI opens it, and the operator writing the cache there when it is missing |
| The stack parameters file, `stack-parameters.json` in JSON (D40), and the operator's folder are under `%USERPROFILE%\.agentspace` | Reads `KEY=VALUE` lines from `infra\local.parameters` in the checkout, and builds the localswim CLI into the checkout's `.build` | [Files in the checkout](#files-in-the-checkout) | Not a lease CLI row |
| `create` never reads or needs the instance-ID cache, with or without `AGENTSPACE_PROXY_COMMAND` | Without the override, refuses to write the alias, and so fails `create`, when its checkout holds no `.instance-id` | D21 ([The alias file](#the-alias-file); [driver.md](driver.md#how-it-is-reached)) | Yes, in part: `conformance/lease-cli/test-unmet-default-transport-no-cache.ps1` shows that a ready `create` succeeds with `AGENTSPACE_PROXY_COMMAND` unset and no cache in the home. That `create` never reads a cache is shown by `conformance/lease-cli/test-lease-ignores-instance-cache.ps1` (E0.06-T04-S03): with a folder, a file that is not JSON and a valid cache at its place, `create`, `get`, `renew` and `release` succeed. 15 of 15 results pass against the Rust build; against the prototype in a checkout with no `.instance-id`, its 3 default-transport results are one SKIPPED, as in the line above |
| A state file in which two records share an `id` is read as not valid by every lease command | `get` and `list` read it; a changing command refuses it only when it writes, after the driver has acted | [The lease state file](#the-lease-state-file) | Yes: `conformance/lease-cli/test-unmet-state-duplicate-id.ps1` (E0.06-T04-S03): `get`, `list`, `renew`, `release` and `create` each fail with `internal`, "could not be read", and change nothing, for a repeated record and for a record with an `id` member twice. 25 of 25 results pass against the Rust build; 15 FAIL against the prototype, whose `get` and `list` read the file |
| The push stamp is `localswim-pushed.json`, in JSON (D40), and is replaced atomically | Writes `localswim-pushed.sha256`, the hex alone, and overwrites it in place | [The push stamp](#the-push-stamp) | Not a lease CLI row |
| The push stamp is readable by the user alone (SHOULD) | It inherits `SYSTEM` and `Administrators` as well | [The push stamp](#the-push-stamp); [operator.md Q27](operator.md#open-questions) | Not a lease CLI row |

## Open questions

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository.** A question
below that asks what the prototype does is moot, since nothing can run it to find out, and its
answer was never a requirement of the rewrite.

**What reading the code and the docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080), every question here is either closed or marked "Later, not blocking", and
none blocks building a drop-in of contract v0.

| | Question | Settled by |
|---|---|---|
| **Q1** | **Closed by D21 (architect, 2026-11-18).** The instance-ID cache is laptop state, so it lives under `%USERPROFILE%\.agentspace`, and the prototype's copy in the checkout is a requirement it does not meet ([Files in the checkout](#files-in-the-checkout)). A rewrite needs no migration, since the cache is rebuilt from the stack's output. The localswim CLI's build output is not state, and MAY stay in the checkout ([operator.md Q7](operator.md#open-questions)). From v0.07 the cache is retired into the host registry's `instance_id` (D45), which lives there too ([The host registry](#the-host-registry)). | Closed |
| **Q2** | **Later, not blocking:** **Does every part of agentspace find the home folder through `USERPROFILE`?** The lease CLI and the operator's state paths do. The operator's `deploy` and `sync` use the shell's home variable, and the task launcher uses the profile folder the operating system reports; whether those follow a changed `USERPROFILE` is unverified, so a check that moves `USERPROFILE` can still reach the real `id_ed25519.pub`, `CLAUDE.md` or launcher error log. | [The home folder](#the-home-folder) states the rule to build, that agentspace MUST read the home folder from `USERPROFILE`, so only what the prototype's operator and launcher do is open. Later: a run on the laptop |
| **Q3** | **Later, not blocking:** **SHOULD `create` check that `known_hosts` holds the box's key under `agentspace`?** Without it every login through the alias is refused, and the lease is still `ready`. Today a person's first `ssh agentspace` makes the entry. | [Creation, in order](#creation-in-order) has no such check and [The host key](#the-host-key) says agentspace never writes the key, so a drop-in builds today's behavior, and a check would be an addition. Later: the architect; [driver.md](driver.md#open-questions) (its Q7) |
| **Q4** | **Later, not blocking:** **SHOULD agentspace remove the files a killed process left behind?** Those are the temporary files of [Replacing a file](#replacing-a-file) (`leases.json.<hex>.tmp`, `<alias>.agentspace-tmp-<hex>`, `config.agentspace-tmp-<hex>`, the keeper's `<name>.<hex>.tmp`), renewal's `renew-<hex>` files, a keeper pass's launch files, and a late launcher's result file. From v0.07 they include `hosts.json.<hex>.tmp`, `agentspace-host.<project>.agentspace-tmp-<hex>`, and a host's push stamp's and signing's temporary files (`sign-<hex>`) ([A host's folder](#a-hosts-folder)). A provisioning run's start leaves no file, and an operator command's own folder, `operator\<hex>\`, is removed by a later command once it is 24 hours old ([Files in the checkout](#files-in-the-checkout)). Nothing reads them again, but they accumulate, and a lease folder's leftovers go only when the lease is released. A cleanup MUST NOT remove a file a running process still uses. | Nothing reads the leftovers again, so a cleanup of them is an addition that no other behavior waits on. Later: the architect; [keeper.md](keeper.md#open-questions) for the launch files |
| **Q5** | **Later, not blocking:** **SHOULD inserting the `Include` line re-check the config just before it replaces it?** A person's save between the backup and the replace is lost today. The window is short and the insertion happens once per config. | [The `Include` line](#the-include-line) states today's steps, and the window they leave is short and comes once per config. Later: the architect |
| **Q6** | **Later, not blocking:** **Which access rules does `ssh-keygen` give the files it writes, on every Windows build?** On 2026-11-18 (OpenSSH_for_Windows 9.5p2, Windows 11 build 26300), a run on throwaway files saw a protected list on each: a private key with `SYSTEM` and `Administrators` full control and the user modify, and a public key or certificate with the same and `Everyone` read, whatever the folder passed down. That covers a lease's `id_ed25519`, `id_ed25519.pub` and `id_ed25519-cert.pub`, and the CA's `agentspace_ca.pub`, which agentspace never re-locks. Whether another build or OpenSSH version differs is open. **What a rewrite that sets the lease key's rules itself MUST set is settled** (E0.04-T04-S01, 2026-11-18) in [The lease folder](#the-lease-folder), which closes [lease-api.md Q12](lease-api.md#open-questions). | What a build that sets the lease key's rules itself MUST set is stated in [The lease folder](#the-lease-folder), so what each build's `ssh-keygen` does is an observation, not a rule to build. Later: a run on each supported build |
| **Q7** | **Later, not blocking:** **What does the prototype's alias test leave if it is killed?** It works in `%USERPROFILE%\.agentspace\tmp-test-lease-alias-<time>` and removes it at its end; a killed run leaves the folder. It is tooling, but it is the only test that writes into the real `.agentspace` outside `leases`. | The test is project tooling, not the product ([Project tooling](#project-tooling)), so a drop-in has nothing to build for it. Later: the conformance suite (E0.04-T02-S04) |
| **Q8** | **Later, not blocking:** **SHOULD `swimcoach-last.json` and the watchdog's files have schemas or move out of `.agentspace`?** They are project tooling, not the product, so D20's schema rule was not applied to them. | These files are project tooling, not the product ([Project tooling](#project-tooling)), so a drop-in has nothing to build for them. Later: the architect |
| **Q9** | **Later, not blocking:** **What clears a lease that a killed `create` placed but never recorded?** The lease folder and alias file stay, and the certificate works until it expires. The prototype writes the record last, so the window is the time between the alias write and the state file's rename. Recording a `pending` record first would close it, at the cost of a second write under the lock. **The second half, whether a later `create` draws a new ID when its alias file already exists, is settled (E0.04-T04-S01, 2026-11-18): it does not** ([The alias file](#the-alias-file)). It fails with `internal`, as the prototype does, since the file may be a person's; the chance is about one in 2^32 for each such leftover, per `create` of that project, and a killed `create` is not a failed one, so lease-api.md's rule for a failed `create` is kept. | Its first half only: a killed `create` is not a failed one ([lease-api.md](lease-api.md#concurrency-and-atomicity)), so no rule a drop-in has to meet clears what it left, and its certificate stops working when it expires; the second half is settled. Later: the architect; [driver.md](driver.md#open-questions) |
| **Q10** | **Later, not blocking:** **SHOULD the `Include` line be recognized only on line 1, as D9 says it is written, or anywhere above the first block, as the prototype accepts it?** This page follows the prototype: any position above the first `Host` or `Match` line, in any ASCII case. Two more points: the prototype also counts other characters as whitespace when it trims a line, between the two words, and after `Host` or `Match` (vertical tab, form feed, and the bytes `0x85` and `0xA0`), which ssh does not; and a line with anything after the pattern, such as a comment, is not recognized, so a second line is inserted and ssh includes each alias file twice, which no run has shown to be harmless. | [The `Include` line](#the-include-line) states the rule to build, the prototype's, so narrowing it to line 1 would be a later choice. Later: the architect; a run on the laptop |
| **Q11** | **Later, not blocking:** **What SHOULD a `renew` killed between moving the new certificate into place and writing the record leave?** The certificate then outlives or falls short of the recorded `expires_at` until the next `renew` or `release`. Writing the record first moves the window rather than closing it. lease-api.md's atomicity covers a failed command, and says nothing of a killed one. | lease-api.md's atomicity covers a failed command and not a killed one, and the next `renew` or `release` puts the certificate and the record back in step, so a drop-in can be built without the answer. Later: the architect |
| **Q12** | **Closed (E0.04-T04-S01, 2026-11-18): when the line has to be added to a `config` that is a symbolic link, another reparse point, or read-only, `create` fails with `internal` before it backs the config up, and writes nothing through the link** ([The `Include` line](#the-include-line)). Whether the prototype meets that is unobserved; a run on the laptop shows it. | Closed |
| **Q13** | **Closed (E0.04-T03-S02, 2026-11-18): the keeper suite checks the keeper's JSON files against their schemas.** The case "The files a keeper run writes match the state schemas" (`conformance/keeper/test-keeper.ps1`) is described in [docs/conformance.md](conformance.md#the-keeper-suite), which owns what it checks. The prototype's schemas check (`scripts/test-schemas.ps1`, moved to `conformance/schemas/test-schemas.ps1`) still scans only the top level of `schemas/`, and no longer needs to look in `schemas/state/`. | Closed |
| **Q14** | **Closed by D45 (v0.07): overcome by events.** It asked whether `instance-id` was the right name for the cache under `%USERPROFILE%\.agentspace`, and whether `AGENTSPACE_STATE_DIR` SHOULD move it. v0.07 retires the cache into the host registry's `instance_id`, and the registry, `hosts.json` (D45, D55), stays out of `AGENTSPACE_STATE_DIR`'s reach for the same reason the cache did: the ProxyCommand reads it without any agentspace variable, and every state folder shares the hosts ([The host registry](#the-host-registry)). | Closed |
| **Q15** | **Closed (architect, 2026-11-20, for v0.07-T01): each operator command writes what it sends into a folder of its own, `%USERPROFILE%\.agentspace\operator\<hex>\`, and removes it as it ends; a folder there older than 24 hours, left by a killed command, is removed by the next command that finds it** ([Files in the checkout](#files-in-the-checkout)). It asked: **What keeps two operator commands on two hosts from writing the operator's folder at once?** Until v0.07 one person ran one operator command at a time on one box. From v0.07 a provisioning run, started detached by the driver, can run its `sync` and `bootstrap` steps while a person runs `sync` on the box, or while a second run makes another host, and with one shared folder each would empty and rewrite `operator\system\`, rebuild `localswim-linux-arm64` and replace the template there. Each wrote the same bytes, but one's emptying could land while the other's `scp` was reading. | Closed |

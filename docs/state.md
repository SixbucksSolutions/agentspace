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

**Defects carried in.** #0041's second and third points (a record without its driver; the order
of renew's and release's writes) are carried into this page as requirements on behavior (D28,
D34), where they touch the files; the rest of #0041, and #0029, are
[lease-api.md](lease-api.md#hardening-carried-in)'s.
[Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
lists every requirement here that the prototype misses.

## Contents

- [Conventions](#conventions): [the home folder](#the-home-folder),
  [what moves which files](#what-moves-which-files), [access rules](#access-rules),
  [replacing a file](#replacing-a-file)
- [Every file at a glance](#every-file-at-a-glance)
- [The lease state file](#the-lease-state-file)
- [A lease's credential](#a-leases-credential)
- [The CA](#the-ca)
- [The push stamp](#the-push-stamp)
- [The keeper's files](#the-keepers-files)
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
  the checkout that are not state ([Files in the checkout](#files-in-the-checkout)). The
  instance-ID cache is state, so D21 puts it here too.
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
- **A value that names an existing file, not a folder,** makes every command that needs that
  folder fail with `internal`, writing nothing in it.
- **Every folder on this page is created when it is first needed**, by whichever writer needs it,
  with the access rules it inherits, unless its own section says otherwise. A missing folder is
  never an error for a reader: it means the files in it do not exist.

### What moves which files

**Each environment variable moves only the files this table names.** Each is read when a program
starts; unset or empty means the default. What each means to the lease CLI is
[lease-api.md](lease-api.md#environment)'s.

| Variable | Moves | Does not move |
|---|---|---|
| `USERPROFILE` | Everything on this page under `%USERPROFILE%`: `.agentspace` and all in it, and `.ssh`; and `.aws`, which the AWS CLI reads ([Files agentspace reads and never writes](#files-agentspace-reads-and-never-writes)) | The checkout; `%LOCALAPPDATA%`; and where a program finds agentspace's other programs, which is its own folder ([operator.md](operator.md#how-each-program-finds-the-others)) |
| `AGENTSPACE_STATE_DIR` | The lease state file and its lock, and every keeper file but the task's launcher program, the task launcher's error log included, as the task's logon environment sets it ([The keeper's logs](#the-keepers-logs)) | The lease folders, the CA, the push stamp, the instance-ID cache, `local.parameters`, the operator's folder, the install folder with the task's launcher in it, the prototype's `BoardHolderLauncher.exe`, and the project tooling, which stay under `%USERPROFILE%\.agentspace` |
| `AGENTSPACE_SSH_DIR` | `config`, its backups, `agentspace.d` and the alias files, for `create` and `release` | `known_hosts`, and the user's own key; ssh itself reads neither variable |
| `LOCALAPPDATA` | The localswim receipt, which the operator reads ([Files agentspace reads and never writes](#files-agentspace-reads-and-never-writes)) | Anything agentspace writes |

- **A check that isolates the lease CLI completely MUST set `USERPROFILE` to a throwaway folder**,
  and make a CA there with the operator's `ca` first ([operator.md](operator.md#ca)). Setting
  `AGENTSPACE_STATE_DIR` alone leaves the lease folders, and the CA they are signed with, in the
  real profile. This settles [lease-api.md Q9](lease-api.md#open-questions).
- **A check that moves `USERPROFILE` and runs the operator's `up`, or connects through a lease's
  alias, loses the AWS sign-in**, because the AWS CLI that the operator and the ProxyCommand run
  finds `%USERPROFILE%\.aws` through the same variable. Such a check MUST NOT expect a connection
  to reach the box.
- **The OpenSSH client does not read `USERPROFILE` or `AGENTSPACE_SSH_DIR`.** An alias written into
  a moved `<ssh folder>` is usable only by an ssh pointed at the alias file itself with `-F`, or
  at a config of its own whose `Include` names that `agentspace.d` by its full path, since a
  relative `Include` resolves against the user's own `~/.ssh` ([The `Include` line](#the-include-line)).
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
2. Move the temporary file over the file, replacing it in one rename (`MoveFileEx` with
   `MOVEFILE_REPLACE_EXISTING`, or the same through any API).
3. Delete the temporary file if anything failed before the move.

**Two files are replaced differently, each for its own reason:**
- **An alias file's temporary file is in `<ssh folder>`, not in `agentspace.d` beside it**, and is
  moved into `agentspace.d`. The `Include` line reads every file in `agentspace.d`, so a temporary
  file there would be read by ssh as configuration ([The alias file](#the-alias-file)).
- **`config` is replaced in a way that keeps its own owner and access rules** (`ReplaceFile` on
  Windows), not by a plain rename, which would give it the temporary file's rules. Windows OpenSSH
  checks those rules ([The `Include` line](#the-include-line)).

**A reader of the lease state file MUST open it sharing read, write and delete access**, so that it
never blocks the rename, and so sees the old file or the new one, never part of either. **A reader
of any other file replaced this way SHOULD share the same**; the keeper's readers of its own JSON
files share read only, and the writer's retry below covers them
([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
**A writer whose rename fails because another process holds the file without sharing delete MAY
retry, and MUST stop retrying within 10 seconds for the lease state file and 5 seconds for the
keeper's JSON files**, then fail. A retry of the lease state file runs under
[the lease state lock](#the-lease-state-lock), so its time is part of the time the command holds
the lock, and of the 30 seconds another changing command waits for it; it adds no wait of its own
to that limit. The prototype retries every 20 milliseconds for 10 seconds (the lease state file)
or every 50 milliseconds for 5 seconds (the keeper's JSON files).

**Every other write on this page is one attempt, with no retry:** `config`'s replace, its backup
and a new `config`, the alias file's move, the key pair, signing, the certificate's move, and every
deletion. A failure there fails the command at once, as its section says. The prototype reads
`config` and an existing alias file whole, sharing read only, and so does not block ssh, which only
reads them.

**A process killed between steps 1 and 3 leaves its temporary file behind.** No reader ever takes
one for the file it stands in for, and nothing in the prototype removes it later
([Q4](#open-questions)).

## Every file at a glance

**Every file and folder agentspace writes or reads under the home folder.** Each has its own
section, which this table only summarizes. "Never" under Removed means agentspace never deletes it.

| Path | What it is | Written by | Read by | Removed |
|---|---|---|---|---|
| `%USERPROFILE%\.agentspace\` | The agentspace folder (D21) | The first writer | Everything below | Never |
| `<state folder>\leases.json` | [The lease state file](#the-lease-state-file) | `create`, `renew`, `release` | Every lease command; the keeper | Never |
| `<state folder>\leases.lock` | [The lease state lock](#the-lease-state-lock) | Every lease command that changes leases | None; it is held, not read | Never |
| `<state folder>\leases.json.<hex>.tmp` | The state file's temporary file | The same | None | At once |
| `%USERPROFILE%\.agentspace\leases\` | The folder of lease folders | The first `create` that issues a credential | `renew`, `release` | Never |
| `%USERPROFILE%\.agentspace\leases\<id>\` | [The lease folder](#the-lease-folder) | `create` | `renew`, `release`, ssh | `release` |
| `...\leases\<id>\id_ed25519`, `id_ed25519.pub` | [The key](#the-key) | `create` | `ssh`; `renew` | `release` |
| `...\leases\<id>\id_ed25519-cert.pub` | [The certificate](#the-certificate) | `create`, `renew` | `ssh` | `release` |
| `...\leases\<id>\renew-<hex>.pub`, `renew-<hex>-cert.pub` | [Renewal](#renewal)'s temporary files | `renew` | None | At once |
| `...\leases\<id>\renew-<hex>-previous-cert.pub` | [Renewal](#renewal)'s copy of the previous certificate, kept until the record is written; the prototype makes none | `renew` | `renew`, to put it back | At once |
| `<ssh folder>\config` | [The `Include` line](#the-include-line), in the user's own file | `create`, once | `ssh` | Never |
| `<ssh folder>\config.agentspace-backup-<time>` | The config as it was before the line was added | `create`, once | A person | Never |
| `<ssh folder>\config.agentspace-tmp-<hex>` | The config's temporary file | `create` | None | At once, except when a replace failed with the config gone |
| `<ssh folder>\agentspace.d\` | The folder of alias files | `create` | `ssh`, through the `Include` line | Never |
| `<ssh folder>\agentspace.d\<alias>` | [The alias file](#the-alias-file) | `create` | `ssh`; `release` | `release` |
| `<ssh folder>\<alias>.agentspace-tmp-<hex>` | The alias file's temporary file | `create` | None | At once |
| `%USERPROFILE%\.ssh\known_hosts` | [The host key](#the-host-key) | `ssh`, never agentspace | `ssh` | Never |
| `%USERPROFILE%\.agentspace\ca\` | [The CA](#the-ca)'s folder | `ca`, once | `ca`, `sync`, `create`, `renew` | Never |
| `...\ca\agentspace_ca`, `agentspace_ca.pub` | The CA's key pair | `ca`, once | The same | Never |
| `%USERPROFILE%\.agentspace\localswim-pushed.sha256` | [The push stamp](#the-push-stamp) | `up`, `sync` | `up` | Never |
| `%USERPROFILE%\.agentspace\instance-id` | [The instance-ID cache](#files-in-the-checkout), a requirement the prototype does not meet: it keeps the file in the checkout | Any operator command that finds it missing; `deploy` | Every operator command but `ca`; the ProxyCommand, when a connection is made. Never the lease CLI (D21) | By `deploy`, which writes it again |
| `%USERPROFILE%\.agentspace\local.parameters` | [The stack's parameters](#files-in-the-checkout), which the prototype keeps in the checkout | A person | `deploy` | Never |
| `%USERPROFILE%\.agentspace\operator\` and what is in it | [The operator's folder](#files-in-the-checkout): the template, the box's files and the localswim CLI, written out for the programs that read them | `deploy`, `sync` | The AWS CLI, `scp` | Never; each file is replaced at its next use |
| `<state folder>\board-holder.json` | [The holder record](#the-holder-record) | The keeper | The keeper | The keeper |
| `<state folder>\board-holder.json.<hex>.tmp` | The holder record's temporary file | The keeper | None | At once |
| `<state folder>\board-holder.log` | [The keeper's log](#the-keepers-logs) | The keeper | People | Never |
| `<state folder>\board-holder-ssh.log`, `.log.1` | The holder's output | The holder | People | Replaced by rotation |
| `<state folder>\board-holder.lock`, `board-holder-keeper.lock` | [The keeper's lock files](#the-keepers-lock-files) | The keeper | The keeper | Never |
| `<state folder>\board-holder-launch.<hex>.json`, `.claim`, `.result.json` | [A holder's launch files](#a-holders-launch-files) | The keeper and its launcher | The same | The keeper, in the same pass; a late launcher's result file never |
| `<state folder>\board-holder-launch.<hex>.json.<hex>.tmp`, `board-holder-launch.<hex>.result.json.<hex>.tmp` | The launch request's and result's temporary files | The keeper and its launcher | None | At once |
| `<state folder>\board-holder-launcher-errors.log` | [The task launcher's error log](#the-keepers-logs) | The task's launcher | People | Never |
| `%USERPROFILE%\.agentspace\bin\` and the programs in it | The install folder ([operator.md, Programs and install layout](operator.md#the-install-folder)), with [the keeper task's launcher](#the-keeper-tasks-launcher) | An install | Whatever runs a program; Task Scheduler, for the task | Never by agentspace, but for the `.old-` files an install renames aside and removes later ([operator.md](operator.md#the-install-folder)) |
| `%USERPROFILE%\.agentspace\BoardHolderLauncher.exe` | The prototype's task launcher, which the rewrite does not use ([The keeper task's launcher](#the-keeper-tasks-launcher)) | The prototype's keeper install | Task Scheduler, for the prototype's task | The prototype's `-Uninstall`; the rewrite's installer, a real install or `-Uninstall` |
| `%USERPROFILE%\.ssh\id_ed25519.pub`, `~\.ssh\config`'s `agentspace` entry, `~\.claude\CLAUDE.md`, `~\.aws\`, localswim's receipt and `bot.xml` | [Files agentspace reads and never writes](#files-agentspace-reads-and-never-writes) | The user, the AWS CLI, or localswim | The operator; ssh; the AWS CLI | Never |
| `%USERPROFILE%\.agentspace\watchdog\`, `swimcoach-last.json`, `tmp-test-lease-alias-<time>\` | [Project tooling](#project-tooling), not the product | The project's scripts | The same | See there |

`<hex>` is 32 lowercase hex digits, new for each file (a GUID without hyphens); `<id>` is a lease
ID, always in its normalized lowercase form ([lease-api.md](lease-api.md#lease-id)); `<alias>` is
the lease's `ssh.host_alias` ([lease-api.md](lease-api.md#fields)).

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
  record asks no driver, and nothing asks one about an `unsatisfiable` record
  ([driver.md](driver.md#release-a-lease)).

**How each command changes it:**
- **`create`** appends one record. It writes `id`, `project`, `state`, `platform`, `profile`,
  `size`, `ttl_seconds` and `created_at`, then `expires_at` and `ssh` for a `ready` lease or
  `reasons` for an `unsatisfiable` one, then `driver`. **An `unsatisfiable` lease's record is all
  `create` writes**: it carries `driver`, no file is made for it, and no CA check is run.
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
- **`get` and `list` MUST NOT write it, and the keeper MUST NOT write it either.**
- **This page requires no single reading of the clock per command.** The prototype reads it
  separately for the expiry check, for `released_at` and for the state it prints, each truncated
  to the second; how `expired` is decided is [lease-api.md](lease-api.md#states)'s.

**Its bytes:**
- **Written as UTF-8 without a byte order mark, as compact JSON with no whitespace outside
  strings, followed by one line feed (LF).** The prototype writes members in the order above;
  a reader MUST NOT depend on member order.
- **Read tolerantly (D20):** a reader MUST accept a leading UTF-8 byte order mark and any amount of
  JSON whitespace. Bytes that are not valid UTF-8 make the file invalid.
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

**A missing file means no leases.** The first command that changes leases creates the state
folder, if needed, and the file. No command deletes the file, and no command forgets a lease in v0
([lease-api.md](lease-api.md#states)). **Access rules:** inherited.

**Who reads it:** every lease command, and the keeper, which reads it without taking the lock
([keeper.md#locks](keeper.md#locks)).

## A lease's credential

**A ready lease's credential is three files in its lease folder, and its alias is one file in the
`<ssh folder>`'s `agentspace.d`, reached through one `Include` line in that folder's `config`
(D7, D9, D10).** What a caller is promised about them is
[lease-api.md](lease-api.md#the-credential)'s; this section says what the files are, byte for byte
where it matters, and how each step makes and removes them. Everything here is done by the v0
driver ([driver.md](driver.md)), inside the lease command that calls it.

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

**Its bytes, exactly:** UTF-8 without a byte order mark, each line ending in one LF, the last
included, and every line after the second indented by two spaces:

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
- **`AGENTSPACE_PROXY_COMMAND`, when set and not empty, replaces the whole line**, and the program
  is then not looked for ([lease-api.md](lease-api.md#environment)).
- **`create` MUST NOT read, check for or need the instance-ID cache**, with or without the
  override: the ProxyCommand reads it when a connection is made (D21;
  [driver.md](driver.md#how-it-is-reached)).
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
  is read by ssh like an alias file, and agentspace never deletes or replaces it.

**When the alias file already exists**, the writer reads its first line:
- **It names this lease, and its bytes equal the new ones:** nothing is written.
- **It names this lease, with other bytes:** it is replaced.
- **It names another lease, or it has no owner mark:** the writer MUST fail and change nothing.
  Two kept leases never share an alias ([lease-api.md](lease-api.md#fields)), so this means a
  foreign file, a lease recorded in another state folder, or one a killed `create` left with no
  record ([Creation, in order](#creation-in-order)). **`create` then fails with `internal`, and
  MUST NOT draw another ID:** the file may be a person's, and an ID is drawn again only for the
  alias of a record ([lease-api.md](lease-api.md#fields)). This settles the second half of
  [Q9](#open-questions); the prototype does the same.
- **Something at that path that is not a file**, such as a folder, makes the write fail, and
  `create` fails with `internal`.

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
  change nothing, and say that a person has to move the line to the top of the file. The message is
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
     one attempt. **The replace MUST keep the config's own owner and access rules**, because
     Windows OpenSSH checks them, and a user's config MAY have rules other than its folder's;
     `ReplaceFile` on Windows does this. If the replace fails part way and the config is gone, the
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

**Nothing else in the config is agentspace's.** The user's own `agentspace` entry is the
operator's ([operator.md](operator.md#the-agentspace-ssh-entry)); agentspace reads it only through
ssh, and never writes it.

**The backups are never deleted by agentspace.** There is one per insertion, which is once per
config unless a person removes the line.

### The host key

**An alias file checks the host's key under the name `agentspace` (`HostKeyAlias agentspace`,
`StrictHostKeyChecking yes`), and agentspace never writes that key.** On today's box it is in
`%USERPROFILE%\.ssh\known_hosts`, put there by ssh the first time the user connected through their
own `agentspace` entry, which accepts a new key ([operator.md, "The `agentspace` SSH
entry"](operator.md#the-agentspace-ssh-entry)). Without it, every login through
a lease alias is refused. A host whose key changes needs that entry replaced by a person. Whether
`create` SHOULD check for it is [Q3](#open-questions).

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
- **An `Include` line, a config backup and `agentspace.d` that the failed `create` made stay, and
  `create` MUST NOT remove them** (this settles [driver.md Q18](driver.md#open-questions)). The line
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

**`release` of a lease whose stored state is `ready` or `released` removes its files** (D10):
1. **The alias file, `<ssh folder>\agentspace.d\<alias>`**, if it exists and is a file. A missing
   alias file is not an error.
2. **The lease folder, `%USERPROFILE%\.agentspace\leases\<id>\`**, with everything in it, if it
   exists. A missing folder is not an error.

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
   2026-11-19, gap FA-07).
2. **Write the record as `released`**, unless it is already ([The lease state file](#the-lease-state-file)).
3. **Delete the alias file, then the lease folder**, as above.

- **A failure in step 3 fails `release` with `internal`**, with lease-api.md's "Credential or alias
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
- **`release` of a `pending` record** writes `released`, and touches no file, since a `pending`
  lease has none.

**What release leaves:**
- **The certificate stays valid until it expires**, on every host that trusts the CA, for anyone
  who copied it and its key. It is not revoked (D10; card #0010 replaces this).
- **Connections already open stay open** ([lease-api.md](lease-api.md#states)).
- **The record**, `released`, in the lease state file.
- **The `Include` line, `agentspace.d` (empty if no other lease is ready) and any config backup.**
- **`%USERPROFILE%\.agentspace\leases\`**, empty or not. Release removes only the lease's own
  folder.
- **The keeper's holder, if it was holding the board's forwards through this lease's alias**, until
  the keeper's next pass stops it and logs that it did
  ([keeper.md#log-lines](keeper.md#log-lines)).

### Between the prototype and the rewrite

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
1. **Both files exist.**
2. **The folder, then the private key, are owner-only.**
3. **The private key loads with an empty passphrase**: it is unencrypted, as
   [the key](#the-key) defines it. The prototype runs `ssh-keygen -y` with an empty passphrase.
4. **The public key's first two words, split at whitespace after trimming, equal the first two of
   the public key derived from the private key, exactly, and the first is `ssh-ed25519`.** The
   prototype compares both words without regard to case, which is looser than this and not
   behavior to copy
   ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
5. **The public key's fingerprint can be read**: the file parses as an OpenSSH public key. The
   prototype runs `ssh-keygen -l -f`.

- **Who checks it:** `ca` after every run, `sync` before it sends the public key to a host, and
  `create` and `renew` before they sign (D17; [operator.md](operator.md#ca)). `release` never
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
| `agentspace_ca` | `ssh-keygen -s`, run by `create` and `renew` | Signing a lease's certificate |
| `agentspace_ca` | `ca`, `sync`, `create`, `renew` | The checks above |
| `agentspace_ca.pub` | `ca`, `sync`, `create`, `renew` | The checks above |
| `agentspace_ca.pub` | `sync` | Sent to the box as its trusted CA ([operator.md](operator.md#what-sync-installs)) |

**Its lifetime:** created once, never removed by agentspace. If the private key is lost or exposed,
a person replaces the CA as D17 says: deletes both files, runs `ca`, runs `sync` for every host,
and re-issues every live lease's certificate.

## The push stamp

**`%USERPROFILE%\.agentspace\localswim-pushed.sha256` records the last token push the box
confirmed** (D9, D32). What it is the hash of, when a push is due, and when it is written are
operator.md's ([The stamp](operator.md#the-stamp)); this section owns how it is stored.

- **Its content is the stamp as 64 hex digits, with nothing else:** no line ending, no byte order
  mark, no whitespace. The prototype writes the digits in uppercase. A writer SHOULD write
  uppercase, so a file written by either is byte-identical; a reader compares without regard to
  case and ignores surrounding whitespace ([operator.md](operator.md#the-stamp)).
- **Its bytes are ASCII**, whatever encoding the writer's runtime defaults to.
- **Written by:** `up` and `sync`, only after the box confirms the push, creating
  `%USERPROFILE%\.agentspace` if needed. **Read by:** `up`, to decide whether a push is due.
- **It SHOULD be replaced atomically** ([Replacing a file](#replacing-a-file)), through
  `localswim-pushed.sha256.<hex>.tmp`. The prototype overwrites it in place, so a reader can see it
  empty or part written, which under operator.md's rule makes the next push due; the prototype
  itself warns and pushes nothing then ([operator.md](operator.md#the-stamp)).
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

**The keeper holds the board's forwards through one ready lease (D30).** What it does with each
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
  `board-holder-launch.<hex>.json.<hex>.tmp` and `board-holder-launch.<hex>.result.json.<hex>.tmp`,
  where the two `<hex>` are separate values. A rename that fails because the file is in use is
  tried again every 50 milliseconds for up to 5 seconds.
- **Read whole.** A reader SHOULD share read, write and delete; the prototype's keeper and launcher
  share read only, which the writer's retry covers
  ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
- **Every path in one is full**, never relative ([The home folder](#the-home-folder)), because the
  program that reads it runs with another working folder and another environment.

### The holder record

**`<state folder>\board-holder.json` names the holder the keeper started, and is absent when there
is none.** Its schema is
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
  UUID in any case, and an `alias` that matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z`.
- **The PID and the start time together are the holder's identity**; the record never names a
  process by its PID alone.

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
| `log` | The full path of `board-holder-ssh.log`, which the holder's output is appended to |
| `directory` | The holder's working folder, as a full path: the home folder |
| `not_after` | The launcher's deadline, UTC, as `YYYY-MM-DDTHH:MM:SS.fffffffZ` (seven fraction digits) |
| `result` | The full path of the result file |

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
launcher that claims a request it can no longer read removes its own claim. Two cases leave
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

## Files agentspace reads and never writes

**These belong to the user, to ssh, to the AWS CLI or to localswim.** agentspace reads them, or
starts a program that reads them, and MUST NOT write,
move or delete them. Their formats are their owners', and this page names only what agentspace
relies on. This settles [operator.md Q8](operator.md#open-questions): docs/state.md lists them,
and owns none of their formats.

| Path | Owner | Read by | What agentspace relies on |
|---|---|---|---|
| `%USERPROFILE%\.ssh\config`, outside the `Include` line | The user | `ssh` | The user's `agentspace` entry ([operator.md](operator.md#the-agentspace-ssh-entry)) |
| `%USERPROFILE%\.ssh\known_hosts` | `ssh` | `ssh` | The box's host key under `agentspace` ([The host key](#the-host-key)) |
| `%USERPROFILE%\.ssh\id_ed25519` | The user | `ssh`, through the `agentspace` entry | Logging in to the box as the user, not as a lease |
| `%USERPROFILE%\.ssh\id_ed25519.pub` | The user | The operator's `deploy` | One line, the user's public key, trimmed ([operator.md](operator.md#deploy)) |
| `%USERPROFILE%\.claude\CLAUDE.md` | The user | The operator's `sync` | Copied to the box as it is ([operator.md](operator.md#sync)) |
| `%USERPROFILE%\.aws\` | The user, through the AWS CLI | The AWS CLI that the operator and the ProxyCommand run | The AWS sign-in; agentspace reads nothing in it itself. The AWS CLI finds it through `USERPROFILE`, so moving that variable loses the sign-in ([What moves which files](#what-moves-which-files)) |
| `%LOCALAPPDATA%\localswim-v2\service.json`, the receipt | The localswim service: whoever cold-starts it writes the receipt; no localswim document names its writer | The operator's token push, in `up` and `sync` | Its `base` and `credential_directory` members; the format is [operator.md, The receipt](operator.md#the-receipt)'s |
| `bot.xml` in the folder the receipt's `credential_directory` names | The localswim launch procedure, which makes it beside the Human credential, `human.xml`, at each cold start | The token push, after the receipt | The Bot token in it; the format is [operator.md, The Bot credential](operator.md#the-bot-credential)'s |

**The two localswim files are the only ones agentspace reads under `%LOCALAPPDATA%`**, and they are
localswim's own state, not agentspace's, so D21 does not move them. `LOCALAPPDATA` alone moves them
([What moves which files](#what-moves-which-files)). Their formats, as far as agentspace relies on
them, are operator.md's ([What the laptop reads](operator.md#what-the-laptop-reads)), because
localswim's own documentation does not state them in full. **Inside the Claude app, a read at that
path can be served from the app's package cache rather than the real file (D21)**, so what a
process inside the app finds there proves nothing about the real files.

## Files in the checkout

**The rewrite keeps nothing in a checkout: no program of agentspace reads or writes one at run
time** (D37; [operator.md, What the prototype took from its checkout](operator.md#what-the-prototype-took-from-its-checkout)).
The prototype's operator keeps three files in the checkout it runs from, and the prototype's
programs read the stack's template, the box's files and the state schemas from there too. This
section gives each file's place in the rewrite, all under `%USERPROFILE%\.agentspace`, and its
format. None is committed.
- **Only the instance-ID cache is state, and D21 requires it under `%USERPROFILE%\.agentspace`**
  (architect, 2026-11-18, which settles [Q1](#open-questions)); the prototype keeps it in the
  checkout, and [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
  lists that.
- **D21 lets build output stay in a checkout, and the rewrite still keeps none there**, since a
  compiled operator has no checkout to find. The operator's folder holds what it writes out for
  the programs it starts.

| Path | The prototype's | Format | Written by | Read by | Removed |
|---|---|---|---|---|---|
| `%USERPROFILE%\.agentspace\instance-id` | `<checkout>\.instance-id` | The instance ID, ASCII, with no line ending | Any operator command that finds it missing; `deploy` | Every operator command but `ca`; the ProxyCommand, when a connection is made. The lease CLI MUST NOT read or need it ([driver.md](driver.md#how-it-is-reached), D21) | By `deploy`, which writes it again |
| `%USERPROFILE%\.agentspace\local.parameters` | `<checkout>\infra\local.parameters` | Text lines; see below | A person | `deploy` | Never |
| `%USERPROFILE%\.agentspace\operator\agentspace.yaml` | None: `deploy` names `<checkout>\infra\agentspace.yaml` | The stack's template, the bytes the operator carries, built from the repository's `infra\agentspace.yaml` | `deploy`, just before the call that reads it | The AWS CLI's `cloudformation deploy` | Never; replaced by each `deploy` |
| `%USERPROFILE%\.agentspace\operator\system\` | None: `sync` copies `<checkout>\box\system\` | The box's system files, as the operator carries them, built from the repository's `box\system` | `sync`, just before the copy, after removing whatever the folder held | `scp`, which copies the folder to the box | Never; emptied and written again by each `sync` |
| `%USERPROFILE%\.agentspace\operator\localswim-linux-arm64` | `<checkout>\.build\localswim-linux-arm64` | The localswim CLI built for today's box | `sync`'s build | `scp` | Never; replaced by each build |

- **The operator's folder, `%USERPROFILE%\.agentspace\operator`, is created by the first command
  that writes in it.** `AGENTSPACE_STATE_DIR` does not move it or `local.parameters`
  ([What moves which files](#what-moves-which-files)). Access rules: inherited.
- **Nothing reads the files in the operator's folder but the programs the operator starts**, at
  the step that wrote them. A file a person changes there is replaced at its next use, so the box
  and the stack are changed only through the repository and a new build.

- **The cache's file name, `instance-id`, is this page's choice** ([Q14](#open-questions)): D21
  says only that it lives under `%USERPROFILE%\.agentspace`. It is moved by `USERPROFILE` and by no
  other variable ([What moves which files](#what-moves-which-files)).
- **The instance-ID cache is read trimmed of surrounding whitespace**, and the ProxyCommand reads
  its first line. What an empty cache means is [operator.md](operator.md#laptop-files)'s.
- **`local.parameters`** holds the account's stack parameters, so it MUST NOT be committed.
  Each line that matches `^\s*[A-Za-z]+=\S` is one parameter, `<Name>=<value>`, taken with its
  surrounding whitespace trimmed and passed to CloudFormation as it is; every other line, such as a
  comment or a blank line, is skipped. The repository's `infra\local.parameters.example`, which is
  committed, shows the form.

## Project tooling

**These files are written under `%USERPROFILE%\.agentspace` by the project's own scripts, not by
agentspace.** They are not part of the product, and the rewrite MUST NOT make them.

| Path | Written and read by | Format | Removed |
|---|---|---|---|
| `%USERPROFILE%\.agentspace\watchdog\<lead>.last-status` | The architect's watchdog (D22) | A UTC time, ISO 8601 with seven fraction digits and `Z`, no line ending; the file's modification time is the timer | Never |
| `%USERPROFILE%\.agentspace\swimcoach-last.json` | The milestone reporter (AGENTS.md) | One JSON object per milestone name, with `active`, `polled`, `at` and `lanes` | Never |
| `%USERPROFILE%\.agentspace\tmp-test-lease-alias-<time>\` | The prototype's alias test | A throwaway folder | By the test, at its end |

**The prototype's own checks also create real lease folders under
`%USERPROFILE%\.agentspace\leases`, signed with the real CA, and delete exactly those.** Every
other check works under `%TEMP%`, through `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR`. A
conformance check MUST instead set `USERPROFILE` ([What moves which files](#what-moves-which-files)),
and MUST NOT touch the real profile (D34).

## Concurrency

### The lease state lock

**`<state folder>\leases.lock` serializes every change to the lease state file, and with it every
change to a lease's credential and alias.** `create`, `renew` and `release` take it before they
read the state file, and hold it until the file is written, so the driver's work on the lease
folder, the `Include` line and the alias file all happens under it. Each first creates
`<state folder>` if it is missing, then opens the lock in it.

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
- **`get` and `list` MUST NOT take it, and the keeper MUST NOT take it either.**

### What two commands at once can see

| One is running | Another sees |
|---|---|
| `create`, `renew` or `release` | `get`, `list` and the keeper see the state file as it was before or as it is after, never part of it, and never wait ([Replacing a file](#replacing-a-file)). A second `create`, `renew` or `release` waits for the lock |
| `create`, between writing the alias file and writing the record | `ssh` can already use the alias, while `get` does not yet show the lease. If `create` then fails, the alias and the lease folder are removed again ([Creation, in order](#creation-in-order)) |
| `renew` | `ssh` reads the old certificate or the new one, never a part of either, since the new one arrives by one rename |
| `release` | `ssh` finds the alias file gone before the key, never the reverse. A session already open stays open. `get` sees the lease `released` before its files are gone, in the order this page asks of the rewrite, and after, in the prototype's ([Release, and what it leaves](#release-and-what-it-leaves)) |
| Commands with different `AGENTSPACE_STATE_DIR` values | Two separate state files with separate locks, but one set of lease folders and, with one `<ssh folder>`, one `agentspace.d`. They do not exclude each other. IDs are random, so their folders never collide, but their aliases can; the second `create` then fails on the first's alias file ([The alias file](#the-alias-file)) |
| A person editing `<ssh folder>\config` | The `Include` line is inserted only when it is missing. A save made after `create` read the config and before its backup fails the backup's hash check, and `create` changes nothing; a save made after the backup and before the replace is lost, though the backup holds the earlier content ([Q5](#open-questions)) |

### What nothing locks

- **The push stamp.** Two `up`s at once can both push and both write it, last writer winning; each
  writes the same value ([operator.md Q20](operator.md#open-questions)).
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

| Requirement | The prototype | Card or question |
|---|---|---|
| A stored `ready` record with no `driver` is renewed and released, by the v0 default driver | Fails with `internal` | #0041, point 2 ([The lease state file](#the-lease-state-file)) |
| A record whose `driver` names no known driver fails with `internal`, and the message does not name the driver | The message names the stored driver | #0041, point 2; D1 |
| `create` and `renew` check the CA and never create it | Run the operator's `ca`, which creates a CA when there is none | #0029, point 3 ([The CA](#its-checks)) |
| The CA check compares the public key's two words exactly | Compares them without regard to case | [The CA](#its-checks) |
| A failed `renew` leaves the previous certificate in place | Leaves the new certificate when the record cannot be written | #0041, point 3 ([Renewal](#renewal)) |
| A failed `release` never leaves a `ready` record with its alias, key or certificate gone | Deletes the alias, then the folder, then writes the record, so a failure to delete the folder leaves a `ready` record with no alias | #0041, point 3 ([Release, and what it leaves](#release-and-what-it-leaves)) |
| `release` of a `released` record removes the files a failed release left | Returns the record unchanged and touches no file | #0041, point 3 |
| The certificate's valid-after time is 5 minutes before its issue time (SHOULD) | It is the issue time | #0029, point 1 ([The certificate](#the-certificate)) |
| Every part of agentspace finds the home folder through `USERPROFILE` | The task launcher uses the operating system's profile folder; `deploy` and `sync` use the shell's home variable | [Q2](#open-questions) |
| Every path written into a file is a full path | Writes a relative `USERPROFILE`, `AGENTSPACE_STATE_DIR` or `AGENTSPACE_SSH_DIR` into the alias file and a launch request as given | [The home folder](#the-home-folder) |
| Readers of the keeper's JSON files share read, write and delete (SHOULD) | They share read only; the writer's 5-second retry covers it | [The keeper's files](#the-keepers-files) |
| The instance-ID cache is `%USERPROFILE%\.agentspace\instance-id` | `.instance-id` in the checkout | D21; [Q1](#open-questions) ([Files in the checkout](#files-in-the-checkout)) |
| `local.parameters` and the operator's folder are under `%USERPROFILE%\.agentspace` | Reads `infra\local.parameters` from the checkout, and builds the localswim CLI into the checkout's `.build` | [Files in the checkout](#files-in-the-checkout) |
| `create` never reads or needs the instance-ID cache, with or without `AGENTSPACE_PROXY_COMMAND` | Without the override, refuses to write the alias, and so fails `create`, when its checkout holds no `.instance-id` | D21 ([The alias file](#the-alias-file); [driver.md](driver.md#how-it-is-reached)) |
| A state file in which two records share an `id` is read as not valid by every lease command | `get` and `list` read it; a changing command refuses it only when it writes, after the driver has acted | [The lease state file](#the-lease-state-file) |
| The push stamp is replaced atomically (SHOULD) | Overwrites it in place | [The push stamp](#the-push-stamp) |
| The push stamp is readable by the user alone (SHOULD) | It inherits `SYSTEM` and `Administrators` as well | [The push stamp](#the-push-stamp); [operator.md Q27](operator.md#open-questions) |

## Open questions

**What reading the code and the docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080), every question here is either closed or marked "Later, not blocking", and
none blocks building a drop-in of contract v0.

| | Question | Settled by |
|---|---|---|
| **Q1** | **Closed by D21 (architect, 2026-11-18).** The instance-ID cache is laptop state, so it lives under `%USERPROFILE%\.agentspace`, and the prototype's copy in the checkout is a requirement it does not meet ([Files in the checkout](#files-in-the-checkout)). A rewrite needs no migration, since the cache is rebuilt from the stack's output. The localswim CLI's build output is not state, and MAY stay in the checkout ([operator.md Q7](operator.md#open-questions)). | Closed |
| **Q2** | **Later, not blocking:** **Does every part of agentspace find the home folder through `USERPROFILE`?** The lease CLI and the operator's state paths do. The operator's `deploy` and `sync` use the shell's home variable, and the task launcher uses the profile folder the operating system reports; whether those follow a changed `USERPROFILE` is unverified, so a check that moves `USERPROFILE` can still reach the real `id_ed25519.pub`, `CLAUDE.md` or launcher error log. | [The home folder](#the-home-folder) states the rule to build, that agentspace MUST read the home folder from `USERPROFILE`, so only what the prototype's operator and launcher do is open. Later: a run on the laptop |
| **Q3** | **Later, not blocking:** **SHOULD `create` check that `known_hosts` holds the box's key under `agentspace`?** Without it every login through the alias is refused, and the lease is still `ready`. Today a person's first `ssh agentspace` makes the entry. | [Creation, in order](#creation-in-order) has no such check and [The host key](#the-host-key) says agentspace never writes the key, so a drop-in builds today's behavior, and a check would be an addition. Later: the architect; [driver.md](driver.md#open-questions) (its Q7) |
| **Q4** | **Later, not blocking:** **SHOULD agentspace remove the files a killed process left behind?** Those are the temporary files of [Replacing a file](#replacing-a-file) (`leases.json.<hex>.tmp`, `<alias>.agentspace-tmp-<hex>`, `config.agentspace-tmp-<hex>`, the keeper's `<name>.<hex>.tmp`), renewal's `renew-<hex>` files, a keeper pass's launch files, and a late launcher's result file. Nothing reads them again, but they accumulate, and a lease folder's leftovers go only when the lease is released. A cleanup MUST NOT remove a file a running process still uses. | Nothing reads the leftovers again, so a cleanup of them is an addition that no other behavior waits on. Later: the architect; [keeper.md](keeper.md#open-questions) for the launch files |
| **Q5** | **Later, not blocking:** **SHOULD inserting the `Include` line re-check the config just before it replaces it?** A person's save between the backup and the replace is lost today. The window is short and the insertion happens once per config. | [The `Include` line](#the-include-line) states today's steps, and the window they leave is short and comes once per config. Later: the architect |
| **Q6** | **Later, not blocking:** **Which access rules does `ssh-keygen` give the files it writes, on every Windows build?** On 2026-11-18 (OpenSSH_for_Windows 9.5p2, Windows 11 build 26300), a run on throwaway files saw a protected list on each: a private key with `SYSTEM` and `Administrators` full control and the user modify, and a public key or certificate with the same and `Everyone` read, whatever the folder passed down. That covers a lease's `id_ed25519`, `id_ed25519.pub` and `id_ed25519-cert.pub`, and the CA's `agentspace_ca.pub`, which agentspace never re-locks. Whether another build or OpenSSH version differs is open. **What a rewrite that sets the lease key's rules itself MUST set is settled** (E0.04-T04-S01, 2026-11-18) in [The lease folder](#the-lease-folder), which closes [lease-api.md Q12](lease-api.md#open-questions). | What a build that sets the lease key's rules itself MUST set is stated in [The lease folder](#the-lease-folder), so what each build's `ssh-keygen` does is an observation, not a rule to build. Later: a run on each supported build |
| **Q7** | **Later, not blocking:** **What does the prototype's alias test leave if it is killed?** It works in `%USERPROFILE%\.agentspace\tmp-test-lease-alias-<time>` and removes it at its end; a killed run leaves the folder. It is tooling, but it is the only test that writes into the real `.agentspace` outside `leases`. | The test is project tooling, not the product ([Project tooling](#project-tooling)), so a drop-in has nothing to build for it. Later: the conformance suite (E0.04-T02-S04) |
| **Q8** | **Later, not blocking:** **SHOULD `swimcoach-last.json` and the watchdog's files have schemas or move out of `.agentspace`?** They are project tooling, not the product, so D20's schema rule was not applied to them. | These files are project tooling, not the product ([Project tooling](#project-tooling)), so a drop-in has nothing to build for them. Later: the architect |
| **Q9** | **Later, not blocking:** **What clears a lease that a killed `create` placed but never recorded?** The lease folder and alias file stay, and the certificate works until it expires. The prototype writes the record last, so the window is the time between the alias write and the state file's rename. Recording a `pending` record first would close it, at the cost of a second write under the lock. **The second half, whether a later `create` draws a new ID when its alias file already exists, is settled (E0.04-T04-S01, 2026-11-18): it does not** ([The alias file](#the-alias-file)). It fails with `internal`, as the prototype does, since the file may be a person's; the chance is about one in 2^32 for each such leftover, per `create` of that project, and a killed `create` is not a failed one, so lease-api.md's rule for a failed `create` is kept. | Its first half only: a killed `create` is not a failed one ([lease-api.md](lease-api.md#concurrency-and-atomicity)), so no rule a drop-in has to meet clears what it left, and its certificate stops working when it expires; the second half is settled. Later: the architect; [driver.md](driver.md#open-questions) |
| **Q10** | **Later, not blocking:** **SHOULD the `Include` line be recognized only on line 1, as D9 says it is written, or anywhere above the first block, as the prototype accepts it?** This page follows the prototype: any position above the first `Host` or `Match` line, in any ASCII case. Two more points: the prototype also counts other characters as whitespace when it trims a line, between the two words, and after `Host` or `Match` (vertical tab, form feed, and the bytes `0x85` and `0xA0`), which ssh does not; and a line with anything after the pattern, such as a comment, is not recognized, so a second line is inserted and ssh includes each alias file twice, which no run has shown to be harmless. | [The `Include` line](#the-include-line) states the rule to build, the prototype's, so narrowing it to line 1 would be a later choice. Later: the architect; a run on the laptop |
| **Q11** | **Later, not blocking:** **What SHOULD a `renew` killed between moving the new certificate into place and writing the record leave?** The certificate then outlives or falls short of the recorded `expires_at` until the next `renew` or `release`. Writing the record first moves the window rather than closing it. lease-api.md's atomicity covers a failed command, and says nothing of a killed one. | lease-api.md's atomicity covers a failed command and not a killed one, and the next `renew` or `release` puts the certificate and the record back in step, so a drop-in can be built without the answer. Later: the architect |
| **Q12** | **Closed (E0.04-T04-S01, 2026-11-18): when the line has to be added to a `config` that is a symbolic link, another reparse point, or read-only, `create` fails with `internal` before it backs the config up, and writes nothing through the link** ([The `Include` line](#the-include-line)). Whether the prototype meets that is unobserved; a run on the laptop shows it. | Closed |
| **Q13** | **Closed (E0.04-T03-S02, 2026-11-18): the keeper suite checks the keeper's JSON files against their schemas.** The case "The files a keeper run writes match the state schemas" (`conformance/keeper/test-keeper.ps1`) is described in [docs/conformance.md](conformance.md#the-keeper-suite), which owns what it checks. The prototype's schemas check (`scripts/test-schemas.ps1`, moved to `conformance/schemas/test-schemas.ps1`) still scans only the top level of `schemas/`, and no longer needs to look in `schemas/state/`. | Closed |
| **Q14** | **Later, not blocking:** **Is `instance-id` the right name for the cache under `%USERPROFILE%\.agentspace`, and SHOULD `AGENTSPACE_STATE_DIR` move it?** D21 names the folder and no file. This page keeps the prototype's name without its dot, and leaves the cache where the stamp and the CA are, out of the state folder's reach, since the ProxyCommand and every lease alias's default ProxyCommand read it without any agentspace variable. | This page picks the name and leaves the cache out of `AGENTSPACE_STATE_DIR`'s reach ([What moves which files](#what-moves-which-files)), so a drop-in builds that. Later: the architect |

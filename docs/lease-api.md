# agentspace lease API, contract v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for `agentspace lease`, written so that it can be built and used without reading any
of agentspace's code (decision D20).** It names every command, flag, output field, state, error and
exit code, and gives the `--help` text at every level word for word. The JSON schemas under
[`schemas/`](../schemas) are part of this contract, and where this page and a schema disagree, both
are wrong and MUST be fixed together.

**What it is for:** a project asks for a lease, which is compute for one agent for a bounded time,
and gets back an SSH host alias, the user to log in as, and an expiry (D2). The reasoning behind
each rule is in [decisions.md](decisions.md), cited as `D<n>`.

**What it never says:** a request, a response, an error or a help text MUST NOT name a backend, a
machine type, a location or a particular machine (D1). A lease looks the same whatever serves it,
and a reason or message that would need such a name MUST be written without it.

**What it does not hold (D34).** Each of these facts has its own reference, and this page cites it
rather than restating it:
- **The files agentspace writes on the caller's machine**, among them the lease's key, certificate
  and alias from creation to removal, and the `Include` line: [docs/state.md](state.md).
- **The boundary between this CLI and a backend:** [docs/driver.md](driver.md).
- **The operator's commands**, among them setting up agentspace's certificate authority (CA):
  [docs/operator.md](operator.md).
- **The keeper, which holds the board's forwards:** [docs/keeper.md](keeper.md).

**Hardening carried in.** Two hardening cards against the prototype, #0029 and #0041, are carried
into this contract as requirements on behavior (D28, D34). Each requirement names its card where
it is stated, and [Hardening carried in](#hardening-carried-in) maps every point of both cards to
its requirement, or says why it is declined.

## Contents

- [Conventions](#conventions): [the program](#the-program), [output](#output),
  [times](#times), [input tolerance](#input-tolerance), [command lines](#command-lines),
  [concurrency and atomicity](#concurrency-and-atomicity)
- [The lease](#the-lease): [fields](#fields), [states](#states),
  [the credential](#the-credential)
- [Values](#values)
- [Profiles](#profiles)
- [Commands](#commands): [`agentspace`](#agentspace), [`agentspace lease`](#agentspace-lease),
  [`create`](#agentspace-lease-create), [`get`](#agentspace-lease-get),
  [`renew`](#agentspace-lease-renew), [`release`](#agentspace-lease-release),
  [`list`](#agentspace-lease-list)
- [Exit codes and errors](#exit-codes-and-errors)
- [Environment](#environment)
- [Schemas](#schemas)
- [Versioning](#versioning)
- [Hardening carried in](#hardening-carried-in)
- [Open questions](#open-questions)

## Conventions

### The program

- **The CLI is one program, run as `agentspace`.** How it is installed and put on the caller's
  `PATH` is outside this contract, and so is its file name. agentspace's own install calls it
  `agentspace.exe`, beside the other programs it needs
  ([operator.md, Programs and install layout](operator.md#programs-and-install-layout)).
  - Note on where this came from: the prototype is `scripts/lease-cli.ps1`, started by
    `bin/agentspace.cmd` from cmd and PowerShell, and by `bin/agentspace` from Git Bash and other
    POSIX shells.
- **Contract v0's caller runs on Windows (D8)**, and runs `agentspace` from cmd, PowerShell or Git
  Bash. A caller on Linux or macOS is later, and not blocking: it comes with the hosted control
  plane (#0011).
- **The program MUST receive its arguments exactly as the caller's shell passed them.** An
  argument MUST NOT be split, joined, dropped, or converted as a path before the rules in
  [Command lines](#command-lines) see it. An empty argument is an argument.
  - From Git Bash, an argument that looks like a path (`/x`) MUST arrive unchanged, never turned
    into a Windows path.
  - **These bind the command as the caller runs it**, `agentspace` typed in that shell, not only
    the executable. Git Bash converts such an argument before a native program starts, so a build
    MAY ship a wrapper that Git Bash and other POSIX shells find as `agentspace`, which turns that
    conversion off (for example with `MSYS_NO_PATHCONV=1` and `MSYS2_ARG_CONV_EXCL='*'`) and runs
    the program with its arguments unchanged. The prototype's `bin/agentspace` is such a wrapper,
    and so is the `agentspace` file of agentspace's install.
  - The prototype breaks this in one known case, an argument such as `-:x`, which its runtime
    refuses before the CLI starts, with exit 1 and no error document. That is a defect, not
    behavior to copy (#0083).
- **Every run that fails MUST still end with one error document and an exit code from
  [the table](#exit-codes-and-errors)**, whatever stage it failed at. The prototype's runtime
  version check is such a stage: a runtime too old for it exits 1 with no error document (#0083).
- **No command reads standard input, and no command prompts.** Every command MUST run unattended,
  with no terminal.

### Output

- **On success (exit 0), a command MUST write exactly one JSON document to stdout**, followed by one
  line feed, and nothing to stderr. `--help` is the one exception: it writes text (below).
- **On failure (any other exit code), a command MUST write nothing to stdout, and exactly one error
  document to stderr**, followed by one line feed ([Exit codes and errors](#exit-codes-and-errors)).
- **Output JSON MUST be compact:** no whitespace outside strings, UTF-8 with no byte order mark,
  and a line feed (never a carriage return) after the document.
- **A command that cannot write its stdout has failed** (architect, 2026-11-19, for
  E0.05-T01-S01; the reasoning is D36's: 0 MUST mean done). When a write to stdout fails, help
  included, because it is closed, full or broken, the command MUST write one `internal` error
  document to stderr and exit 1; when stderr cannot be written either, it still exits 1.
  - **On Windows, a stdout the program was started without** (no handle, or an invalid one)
    **MUST count as a failed write**, even where a runtime would report its writes as done.
  - **On a system other than Windows, a stdout the caller closed MAY read as one that discards
    its output** (architect, 2026-11-19, from E0.05-T01-S01's review): Rust's runtime, for one,
    opens a closed standard stream on `/dev/null` before the program starts, so the program cannot
    tell the two apart in safe code. Contract v0's caller is Windows (D8), and a full or broken
    stdout still fails there.
  - **A change the command already recorded stays recorded.** A `create`, `renew` or `release`
    whose record was written before its document failed to reach stdout has made its change, and
    is not undone, which is the one exception to [Concurrency and atomicity](#concurrency-and-atomicity)'s
    rule for a failed command. Its message MUST say that the change was made and that `get` or
    `list` shows it, as in `agentspace lease create failed: The lease was recorded, but its
    document could not be written. Run 'agentspace lease list' to see it.`
- **Members appear in the order this page lists them**, but a caller MUST NOT depend on the order.
- **A string MAY use any escape that JSON allows**, and MUST escape what JSON requires. A caller
  MUST compare parsed values, never bytes.
- **A field that does not apply MUST be left out, never written as `null`**, an empty string or an
  empty object. Which fields apply in which state is in [Fields](#fields).
- **Every output document MUST validate against its schema** (D20). An implementation's own checks
  MUST validate what it prints, not only what it means to print.

### Times

- **Every time in output MUST be UTC, to the whole second, written `YYYY-MM-DDTHH:MM:SSZ`**, for
  example `2026-11-17T18:04:05Z`. No other form is emitted: no fraction, no offset, no lowercase
  `t` or `z`. The caller's time zone MUST NOT change any time in output.
- **A time is truncated to the whole second, never rounded.**
- **"Now" is the clock of the machine that answers the command.** In v0 that is the caller's
  machine.
- **A lease's certificate is issued at a whole second, its issue time, during the command that
  issues it** (`create` or `renew`). `expires_at` MUST equal the issue time plus the TTL exactly,
  which is also the certificate's valid-before time ([The credential](#the-credential)).
- **The conformance suite (E0.04-T02-S04) MUST check these against its own clock, and MUST run at
  least once with the caller's time zone set to one other than UTC (#0029).** A check of the start
  of a certificate's validity against the suite's own clock, and a run in another zone, are the
  only ways to see a UTC and local-time mix-up on a machine that is set to UTC.

### Input tolerance

**agentspace is strict in what it emits and tolerant in what it accepts (D20).** Every value in
[Values](#values) lists the spellings it accepts and the one form it is normalized to. Tolerance
covers form only:
- **A value that could mean two different things MUST be refused**, never guessed at. A bare
  `--ttl 8` is refused because its unit is unknown.
- **A value that is well formed but unsafe MUST be refused**, for example a lease ID with a path
  separator in it.
- **A value is normalized before it is used or stored**, so every output carries the normalized
  form.
- **"Surrounding whitespace" means whitespace before the first and after the last other
  character**, where whitespace is any character with the Unicode `White_Space` property, line
  feeds included. It is trimmed before the value is checked. Those characters are U+0009 to
  U+000D, U+0020, U+0085, U+00A0, U+1680, U+2000 to U+200A, U+2028, U+2029, U+202F, U+205F and
  U+3000, and no others.
- **U+FEFF, the byte order mark, is not whitespace, and is never trimmed.** A value that holds one
  is refused with `invalid_value`, wherever it is, and the input schemas refuse it too
  ([Schemas](#schemas)).
- **Every check MUST match the whole value (#0029).** A check that lets a trailing line feed through
  after trimming, as a regular expression's `$` does in several engines, is a defect. Letters and
  digits in a value are ASCII letters and digits only, and "any case" means ASCII case.
- **An argument that is not valid Unicode**, such as one holding an unpaired UTF-16 surrogate,
  which Windows can pass, is read as written and never converted with a replacement character
  (architect, 2026-11-19, gap FA-09). As a flag's value or a lease ID it is refused with
  `invalid_value` for that field; as a command, subcommand or option it is the `usage` error that
  any other unknown word is. **Where a message quotes it, each unpaired surrogate is written as
  `\u` and four lowercase hex digits**, as a control character is
  ([Exit codes and errors](#exit-codes-and-errors)), so the message stays valid UTF-8.

### Command lines

**Tokens.** A command line is read from left to right, one argument at a time:
- **The first argument that does not start with `-` names the command, and the next such argument
  the subcommand.** Each MUST be ASCII letters only, matched without regard to case:
  `agentspace Lease CREATE` is the same as `agentspace lease create`. Help and output always spell
  them in lowercase.
- **An option before the command or the subcommand is a request for help or an unknown option**,
  and never takes a value. So `agentspace --project x lease create` reads `x` as the command, and
  is a usage error for the unknown command `x`.
- **An argument that starts with `-` is an option.** It is a flag of the subcommand when it is `--`
  followed by the flag's name in ASCII letters, in any case, and optionally `=` and a value:
  `--ttl`, `--TTL=8h`. Anything else that starts with `-` is an unknown option, among them `-ttl`,
  `-p`, a bare `-`, a bare `--` (which does not end the options) and `--help=yes`.
- **`-h` and `--help`, in any case, ask for help.**
- **Any other argument is positional.** The subcommands `get`, `renew` and `release` take one, the
  lease ID, anywhere on the line among their flags. An empty argument is a positional argument.

**A flag's value:**
- **It MAY follow the flag after an equals sign (`--ttl=8h`).** This form always gives a value:
  everything after the first `=` is the value, even when it starts with `-`, is `-h` or is
  `--help`. `--ttl=--help` is the value `--help`, and `--ttl=-h` the value `-h`, not a request for
  help.
- **It MAY be the next argument (`--ttl 8h`),** unless that argument starts with `--`. An argument
  that starts with a single `-` is taken as the value, so `--ttl -5m` gives the value `-5m`. A value
  MAY contain spaces when the shell passes it as one argument (`--ttl "1h 30m"`).
- **A next argument of exactly `-h`, in any case, is not taken as a value: it is a request for
  help**, as is `--help`, which starts with `--` and so is never a value in this position.
  `create --ttl -h` prints `create`'s help.
- **To give a value that starts with `--`, or a value of `-h`, use the `=` form.**
- **For any other value, the two forms MUST give the same result**, a value that contains a line
  feed included. The prototype treats `--project=<a value with a line feed>` as an unknown option
  instead, which is a defect, not behavior to copy (#0083).

**Usage errors** (`usage`, exit 2):
- **An unknown command or subcommand.** Reading stops there, so nothing after it is checked, a
  `--help` included: `agentspace lease frob --help` is a usage error, because there is no level
  to print.
- **An unknown option**, at any level, including a flag that another subcommand has (`get --ttl
  8h`).
- **A flag with no value:** the flag is last on the line, or the next argument starts with `--`.
  `create --ttl --size m` is this error for `ttl`.
- **A flag with an empty value:** `--ttl=` or `--ttl ""`. A value of only whitespace is not empty
  here: it reaches the value's own check, which refuses it with `invalid_value`.
- **A flag given twice**, because the two values could disagree, whatever case each is spelled in.
  `list --state` is the one flag that MAY be repeated. A repeated flag whose later occurrence has
  no value or an empty value is reported as that, "needs a value" or "has an empty value", not as
  given twice.
- **An unexpected positional argument:** a second ID, or any positional argument to `create` or
  `list`.
- **`agentspace` with no command, and `agentspace lease` with no subcommand**, so that a script
  never mistakes help text for JSON.
- **A missing lease ID** for `get`, `renew` or `release`, and a missing `--project` for `create`.

**Which error is reported:**
1. **`-h` or `--help` anywhere on the line, before any unknown command or subcommand, prints the
   help of the level reached so far and exits 0**, whatever else is on the line, an earlier usage
   error included. Nothing else on the line is checked or done: `agentspace lease create --help
   --ttl 99d` prints help and creates nothing, and `agentspace --help lease` prints the top level's
   help.
   - **This holds in a flag's value position too:** `create --ttl --help` and `create --ttl -H`
     print `create`'s help. Only the `=` form makes `--help` a value.
2. **Otherwise, an unknown command or subcommand is reported** as soon as it is read, even when an
   unknown option comes before it on the line: `agentspace --project x lease create` reports the
   unknown command `x`, not the unknown option `--project`.
3. **Otherwise, the first of the other usage errors in line order is reported**: an unknown option,
   a flag with no value or an empty value, a flag given twice, or an unexpected argument.
4. **Otherwise, a missing command, subcommand, lease ID or `--project` is reported.**
5. **Only when the line has no usage error are the values checked**, in the order of
   [Exit codes and errors](#exit-codes-and-errors).

### Concurrency and atomicity

- **Commands MUST be safe to run at the same time from several processes.** Each command's effect
  on a lease MUST be atomic: another command sees the lease either before it or after it, never
  half changed.
- **`get` and `list` MUST NOT wait for a command that changes leases.** They see the leases as they
  were before that command or as they are after it.
- **`create`, `renew` and `release` MUST wait for one another for up to 30 seconds, and MUST NOT
  wait longer. One that cannot start within 30 seconds MUST fail with `internal`, change nothing,
  and say to try again** (architect ruling, 2026-11-18). `release` of a lease that needs no change
  MAY wait too.
- **A command that fails MUST leave every lease as it was before the command**, except as the
  bullets below allow.
  - **A failed `create` MUST remove every file and folder it made for the lease** (its key, its
    certificate, its alias file and its folder, a key whose certificate could not be signed
    included), **and MUST leave no lease record (#0029).** A later `create` MUST NOT fail because
    of anything an earlier failed one left.
    - **A removal can fail too** (a file held open, a rule that forbids the delete). When one does,
      `create` MUST name each file or folder it could not remove in its `internal` message, and
      still MUST leave no lease record, so none that is `ready`. It MUST change nothing else: no
      other lease, no file it did not make. A leftover is a file, or a folder that holds no
      leftover file, and is named in its home-relative form, as `~/.agentspace/leases/<id>/id_ed25519` (the
      [`internal` message rule](#exit-codes-and-errors) forbids an absolute path). A folder that
      still holds a leftover file need not be named, since the file's name says where it is.
    - **A leftover that is not under the home folder**, such as an alias file in an
      `AGENTSPACE_SSH_DIR` elsewhere, **is named by what it is and its own name, with no folder**:
      `the host alias file <alias>` (architect, 2026-11-19, gap FA-04).
    - **What it MAY leave in the OpenSSH client configuration** is the line that pulls aliases in,
      a dated backup of the configuration from before that line was added, and the folder of alias
      files. None of them is a lease's, and each stays ([docs/state.md](state.md#creation-in-order)).
    - **A `create` that was killed is not a failed one**, and can leave a lease's files with no
      record; what a later `create` does then is [docs/state.md](state.md#the-alias-file)'s.
  - **A failed `renew` MUST leave the lease's `expires_at`, `ttl_seconds` and certificate as they
    were (#0041).** An implementation that puts the new certificate in place before recording it
    MUST put the previous one back before it fails.
  - **A failed `release` MUST NOT leave a lease `ready` or `expired` with its key, certificate or
    alias gone (#0041).** It leaves the lease as it was, or `released` with some of the three left
    behind. A retried `release` MUST then delete whatever is left
    ([`release`](#agentspace-lease-release)).

## The lease

### Fields

**One lease document is printed by `create`, `get`, `renew` and `release`, and `list` prints an
array of them.** Its schema is [`lease.output.schema.json`](../schemas/lease.output.schema.json).

| Field | Type | Present | Meaning |
|---|---|---|---|
| `id` | String, lowercase UUIDv4 | Always | The lease's ID, chosen by agentspace at `create`. |
| `project` | String, [project name](#project) | Always | The project the lease is for. |
| `state` | String, [state](#states) | Always | Where the lease is in its life, as of now. |
| `platform` | String, [platform](#platform) | Always | The platform asked for, normalized. |
| `profile` | String, [profile](#profile) | Always | The software profile asked for, normalized. |
| `size` | String, [size](#size) | Always | The size asked for, normalized. |
| `ttl_seconds` | Integer, 60 to 86400 | Always | The TTL of the current certificate, or of the one asked for if none was issued. |
| `created_at` | String, [time](#times) | Always | When `create` recorded the lease: a whole second during the `create` command. |
| `expires_at` | String, [time](#times) | `ready` and `expired`; `released` if it was ever ready | When the current certificate stops being accepted for new connections: its issue time plus `ttl_seconds`. An older certificate MAY outlive it; see "What expiry and release do not do" under [States](#states). |
| `released_at` | String, [time](#times) | `released` only | When `release` released the lease. |
| `ssh` | Object | `ready` only | How to connect; see below. |
| `ssh.host_alias` | String | Inside `ssh` | The name to give `ssh`: `ssh <host_alias>`. |
| `ssh.user` | String, user name | Inside `ssh` | The user the alias logs in as: 1 to 64 letters, digits, `.`, `_` and `-`, not starting with `.` or `-`. |
| `reasons` | Array of objects, 1 to 4 | `unsatisfiable` only | Why the request cannot be met; see below. |
| `reasons[].code` | String | Inside each reason | One of the reason codes below. |
| `reasons[].message` | String | Inside each reason | One sentence for a person, naming the value that cannot be met. |

**`expires_at` is the issue time plus the TTL, not `created_at` plus the TTL.** The certificate is
issued during `create`, so the two differ by no more than the time `create` took, and either MAY
come first.

**`ssh.host_alias` is `agentspace-<project>-<first 8 hex digits of id>`**, for example
`agentspace-swimcoach-3f2a9c1e`. A caller MUST treat it as an opaque name to pass to `ssh` or to any
client that reads the OpenSSH client configuration, and MUST NOT build it from the ID itself.
**Two kept leases MUST NOT share a host alias:** `create` MUST choose an ID whose alias no kept
lease uses, in any state, and MUST draw a new ID, as many times as it takes, while the one drawn
has an alias that a kept lease uses. A build MAY bound the draws at no fewer than 100, and MUST
then fail with `internal` when none is free; a project would need billions of kept leases for that
to happen. Only kept leases count: an alias file that no record names makes `create` fail, and is
never a reason to draw again ([docs/state.md](state.md#the-alias-file)).

**`ssh.user` is the user the lease logs in as.** In v0 every project's leases log in as `ubuntu`
(D3). A caller MUST read it from the lease and MUST NOT assume it, because later hosts MAY change
it. A caller MUST accept any user that the table's rule for `ssh.user` allows. In v0 a lease's user also matches the
narrower pattern of the lease state file, which is what binds a driver
([docs/state.md](state.md#the-lease-state-file)).

**Reason codes:**

| Code | Means |
|---|---|
| `unsupported_platform` | No backend can provide the platform asked for. |
| `unsupported_profile` | No backend can provide the profile asked for. |
| `unsupported_size` | No backend can provide the size asked for. |
| `no_capacity` | The request is supported, but no capacity can be found for it. |

**Every reason that applies MUST be listed, each code once,** in the order of the table, so a
request with an unsupported platform and an unsupported size gets two reasons. In v0 only
`linux/arm64`, `agent-linux` and size `m` are supported (D5); `no_capacity` is in the contract for
later backends, and v0 never reports it.

**Each reason's message quotes the normalized value**, and SHOULD read as v0's do:
- `Platform '<platform>' is not supported; v0 supports linux/arm64 only.`
- `Profile '<profile>' is not supported; v0 supports agent-linux with no version only.`
- `Size '<size>' is not supported; v0 supports size m only.`

**Which backend placed a lease MUST NOT appear in any output** (D1), although agentspace keeps it
to renew and release the lease.

### States

| State | Means | Terminal |
|---|---|---|
| `pending` | Accepted, and not yet ready to connect to. No credential has been issued. | No |
| `ready` | A connection through `ssh.host_alias` is expected to succeed until `expires_at`. | No |
| `expired` | Was ready, and `expires_at` has passed. New connections are refused. | No; it can still be released |
| `released` | Given back by `release`. Its local credential and alias are gone. | Yes |
| `unsatisfiable` | Can never be met as asked; `reasons` says why. Nothing was issued. | Yes |

**Transitions:**

| From | To | By |
|---|---|---|
| None | `ready`, `pending` or `unsatisfiable` | `create` |
| `pending` | `ready` or `unsatisfiable` | agentspace, when placement finishes |
| `pending` | `released` | `release` |
| `ready` | `ready`, with a new `expires_at` | `renew` |
| `ready` | `expired` | Time: `expires_at` passes |
| `ready` | `released` | `release` |
| `expired` | `released` | `release` |

No other transition exists. In particular, nothing leaves `released` or `unsatisfiable`, and an
expired lease cannot be renewed.

**`ready` means a connection is expected to succeed, not that compute is running.** The first
connection after a quiet spell MAY take a few minutes while agentspace brings the compute up, and a
caller SHOULD allow for that in its SSH timeouts.

**A caller MUST handle `pending`**, by calling `get` until the state changes, even though v0's
`create` answers `ready` or `unsatisfiable` straight away. A later backend MAY take minutes to
place a lease, and `create` will then answer `pending`.

**How `expired` is decided:** a lease that was `ready` is `expired` when the command reading it
runs at or after its `expires_at`, to the second. Nothing has to run at that moment: `get`, `list`,
`renew` and `release` each decide it when they read the lease, so the same lease can be `ready` in
one call and `expired` in the next with nothing in between. An `expired` lease has no `ssh`.

**What expiry and release do not do:**
- **A connection that is already open MUST NOT be closed by expiry or release.** The expiry is
  checked only when a connection is made, so both stop new connections only.
- **Neither `release` nor `renew` revokes a certificate (D10).** `release` deletes the caller's
  copy, and `renew` replaces it. A copy of any certificate that leaked stays valid until its own
  expiry. That is `expires_at` for the current certificate, and for an older one it MAY be later
  than `expires_at`, when a `renew` shortened the lease. That is why the TTL has a maximum (D24).

**How long a lease is kept:** a `released`, `expired` or `unsatisfiable` lease MUST stay readable
by `get` and `list` for at least 24 hours after it reached that state. After that it MAY be
forgotten, and `get` then answers `not_found`. In v0 no lease is forgotten.

**Where leases are kept:** in v0, on the caller's machine, so `get` and `list` see the leases
created there and no others. The files are [docs/state.md](state.md)'s, and
[`AGENTSPACE_STATE_DIR`](#environment) moves them.

### The credential

**A ready lease's credential is an SSH key pair on the caller's machine and a certificate for it,
signed by agentspace's CA (D7).** What the caller sees of it is below; the files that hold it, and
how they are made and removed, are [docs/state.md](state.md)'s.

**The key:**
- **`create` makes a new key pair for each lease.** The private key MUST NOT appear in any output,
  MUST NOT leave the caller's machine, and MUST NOT be readable by another ordinary user of that
  machine. The access rules that meet this are [docs/state.md](state.md#the-lease-folder)'s.
- **The private key MUST have no passphrase**, so that `ssh <host_alias>` never prompts (#0029).

**The certificate:**
- **Its principal is `ssh.user`, and its key ID is the lease's `id`**, so a host's SSH log names
  the lease behind every connection (D7).
- **Its valid-before time is `expires_at`:** the issue time plus the TTL, exactly ([Times](#times)).
- **Its valid-after time MUST NOT be later than the issue time, and SHOULD be 5 minutes before it
  (D24, #0029),** so that a host whose clock is a little behind the caller's still accepts a new
  certificate. The back-dating does not count toward the TTL, and never moves `expires_at`. The
  prototype does not back-date yet: its valid-after is the issue time, and #0029's point is met in
  v0.05 (#0054).
- **It permits a terminal (PTY) and port forwarding, and nothing else:** no agent forwarding, no
  X11 forwarding and no user rc file (D7). Port forwarding MUST stay permitted, because agentspace's
  board reaches a lease host through it (D12, D30).
- **Its serial is not part of this contract,** and a caller MUST NOT rely on it. v0 issues serial 0
  on every certificate; whether serials are needed is decided with revocation, card #0010 (#0029).

**The certificate authority:**
- **`create` and `renew` MUST NOT create agentspace's CA (#0029).** A CA made by issuing would be
  trusted by no host until the operator installs it there, so the lease would be `ready` and
  refuse every connection.
- **If there is no CA, `create` and `renew` MUST fail with `internal`**, change nothing, and say
  that agentspace's CA has not been set up ([Messages](#messages)). Setting it up is the
  operator's ([docs/operator.md](operator.md)).

**The alias:**
- **`create` makes `ssh.host_alias` usable from the calling user's OpenSSH client configuration**,
  so that `ssh <host_alias>` works with no other options. What it adds to that configuration, and
  how, is [docs/state.md](state.md)'s, and nothing else in it is changed.
- **If the calling user's OpenSSH configuration cannot take the alias as it stands, `create` MUST
  fail with `internal`**, record no lease and leave no key, certificate or alias, and say what a
  person has to fix ([Messages](#messages)). What it MAY leave in that configuration, and when
  `agentspace.d` is made, are [Concurrency and atomicity](#concurrency-and-atomicity)'s and
  [docs/state.md, The `Include` line](state.md#the-include-line)'s (architect, 2026-11-19, gap
  FB-04).
  The prototype's message says only that the alias could not be set up; that is a defect, not
  behavior to copy.
- **A connection through the alias reaches the lease's host as `ssh.user`**, and how it gets there
  is the driver's ([docs/driver.md](driver.md), D9).
- **In v0 the alias also forwards the agentspace board's ports to the lease host** (D12, D30), so
  a second session through any alias on the same host warns that its remote port forwarding
  failed, and still logs in. A caller MUST NOT rely on these forwards.

**What `renew` and `release` do to it:**
- **`renew` replaces only the certificate.** The key, the alias and the user stay the same, so a
  client that saved the alias keeps working.
- **`release` deletes the key, the certificate and the alias.** It leaves the rest of the OpenSSH
  client configuration alone.

## Values

**Each value below is accepted in the forms listed and normalized to one form before use.** A
value outside its accepted forms is refused with `invalid_value` (exit 2), and a well-formed value
outside its allowed range with `out_of_range` (exit 2). Each error's message is in
[Messages](#messages).

### Lease ID

- **Accepted:** a UUIDv4 in any case, with surrounding whitespace:
  `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`, where each `x` is a hex digit and `y` is one of `8`, `9`,
  `a` or `b`.
- **Normalized to:** lowercase, whitespace trimmed.
- **Refused:** any other UUID version, braces, a `urn:uuid:` prefix, an empty value, and anything
  else. A well-formed ID that names no lease is `not_found` (exit 3), not `invalid_value`.
- **Everything agentspace derives from a lease ID MUST be derived from its normalized form**: the
  host alias, the certificate's key ID, and any file or folder name (#0029). A normalized ID never
  ends in a dot and never holds a path separator, so a name made from it is never changed by the
  file system.
- **IDs are chosen by `create`, at random**, and never reused.

### Project

- **Accepted:** 1 to 32 characters of letters, digits and `-`, starting and ending with a letter or
  digit, in any case, with surrounding whitespace.
- **Normalized to:** lowercase, whitespace trimmed, matching `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`.
- **No default; `create` requires it.**

### Platform

**An `os/arch` pair in the OCI form, with an optional variant (D4).** The platform never says which
libc a host has; the [profile](#profiles) does.
- **Accepted:** `os/arch` or `os/arch/variant`, each part one or more letters and digits, in any
  case, with surrounding whitespace. The architecture, and only it, MAY also be `x86_64` or
  `x86-64` (for `amd64`), the one place a `_` or `-` is accepted. The architecture `aarch64` means
  `arm64`.
- **Normalized to:** lowercase, whitespace trimmed, the architecture `aarch64` written `arm64`,
  `x86_64` and `x86-64` written `amd64`, and the variant `v8` of `arm64` dropped, so
  `linux/aarch64/v8` becomes `linux/arm64`. Every other variant is kept: `linux/arm64/v9` and
  `linux/amd64/v3` stay as they are.
- **Default:** `linux/arm64` (D13).
- **Supported in v0:** `linux/arm64` only. Any other well-formed platform is accepted, and the
  lease is `unsatisfiable` with `unsupported_platform` (D5). That includes `windows/amd64`: Windows
  hosts are in scope (D4), and unsatisfiable until a driver supports them.

### Profile

**A named software contract, optionally pinned to a version (D4): `name` or `name@version`.** What
each supported profile promises is in [Profiles](#profiles).
- **Accepted:** a name of 1 to 64 letters, digits and `-`, starting and ending with a letter or
  digit, in any case, optionally followed by `@` and a version number of 1 or more, with surrounding
  whitespace. Leading zeros in the version are accepted; a version of only zeros is refused.
- **Normalized to:** lowercase, whitespace trimmed, leading zeros dropped from the version
  (`Agent-Linux@03` becomes `agent-linux@3`). A profile with no version stays without one, and
  means the profile's current version.
- **Default:** `agent-linux`.
- **Supported in v0:** `agent-linux` with no version. Any other well-formed profile, including
  `agent-linux@1`, is accepted and the lease is `unsatisfiable` with `unsupported_profile`, because
  v0 has no versioned profiles.

### Size

- **Accepted:** 1 to 8 letters and digits, in any case, with surrounding whitespace.
- **Normalized to:** lowercase, whitespace trimmed.
- **Default:** `m`.
- **Supported in v0:** `m` only. Any other well-formed size is accepted, and the lease is
  `unsatisfiable` with `unsupported_size`.

### TTL

**How long a lease's credential is valid, from the moment it is issued (D24).** It is a duration,
never a clock time.
- **Default:** 8 hours for `create`. For `renew`, the lease's current `ttl_seconds`.
- **Range:** at least 1 minute and at most 24 hours, both inclusive. A TTL outside it MUST be
  refused with `out_of_range`, and MUST NOT be clamped to the nearest limit.
- **Accepted, short form:** one or more parts, each a whole number of decimal digits (leading zeros
  allowed) and a unit, largest unit first, each unit at most once. Whitespace is optional between a
  number and its unit, between parts, and around the whole. Examples: `8h`, `90m`, `1h30m`,
  `1h 30m`, `2 hours`, `45 min`, `1d`, `23h59m60s`.

  | Unit | Spellings, in any case unless noted |
  |---|---|
  | Days | `d`, `day`, `days` |
  | Hours | `h`, `hr`, `hrs`, `hour`, `hours` |
  | Minutes | `m` in lowercase only; `min`, `mins`, `minute`, `minutes` in any case |
  | Seconds | `s`, `sec`, `secs`, `second`, `seconds` |

- **Accepted, ISO 8601 form:** `P[nD][T[nH][nM][nS]]` in any case, with at least one part, with at
  least one part after a `T` if there is a `T`, and no whitespace inside. Examples: `PT8H`, `pt90m`,
  `P1D`, `P1DT2H`. In this form `M` in either case is minutes, because it comes after the `T`.
- **Normalized to:** a whole number of seconds, which output carries as `ttl_seconds`.
- **Refused as `invalid_value`, because each is ambiguous or not a duration:**
  - A bare number, such as `8`, `28800` or `0`, whose unit is unknown.
  - An uppercase `M` in the short form, which could mean months or minutes, with or without
    whitespace before it (`8M`, `8 M`).
  - Weeks, months and years, in either form.
  - A fraction, such as `1.5h`, which would have to be rounded; `1h30m` says it exactly.
  - A sign, a negative number, a unit given twice, units out of order, an empty or all-whitespace
    value, or anything else.
- **Refused as `out_of_range`:** a well-formed TTL below 60 seconds or above 86400 seconds,
  including a zero with a unit, such as `0m` or `PT0S`. The limits are checked after
  normalization, so `1440m` and `P1D` are both allowed and `24h1s` is not. A well-formed TTL too
  large to compute, such as `99999999999999999999h`, is also `out_of_range`, and MUST NOT be
  `internal`. A TTL has no length limit of its own.

### State filter

**Used by `list --state`.**
- **Accepted:** one of the five [state names](#states), in any case, with surrounding whitespace.
  On the command line, `--state` MAY be repeated and each value MAY be a comma-separated list, as in
  `--state ready,pending`; whitespace around each name is trimmed. In a request document, `state`
  is one state name or a non-empty array of them, never a comma-separated list, and `null` means
  not given, as for every optional member ([Schemas](#schemas)).
- **Refused as `invalid_value`:** any other name, and an empty name, as in `ready,`,
  `ready,,pending` or `--state " "`. The message then quotes the empty name as `''`.
  `--state ""` is different: an empty value, which is a `usage` error
  ([Command lines](#command-lines)).
- **Normalized to:** the set of lowercase state names, with duplicates dropped.

## Profiles

**A profile is the software a lease's host promises, by name (D4).** The [platform](#platform) says
the operating system and architecture; the profile says everything an agent's tools depend on
beyond that, its libc first.

**Every profile MUST state its libc family, which is one of `glibc`, `musl` or `none`** (D4).
- **`glibc` and `musl` carry a minimum version:** the oldest that every host serving the profile
  MUST have. A host MAY have a newer one.
- **`none` is a host or image with no libc at all,** such as a `scratch` container that runs a
  CGO-disabled Go binary, and carries no version. It promises nothing that needs a libc.
- **The platform alone never says the libc.** An agent that ships a Rust `-gnu` binary, Node's
  `linux-arm64` build or a `manylinux` wheel learns from the profile whether it will run.
- **A musl host, such as Alpine, or a `none` host would be a profile of its own**, unsatisfiable
  until a driver supports it (D5). No v0 profile has either.

### agent-linux

**The one profile v0 supports, with no version.** Every host that serves a lease with profile
`agent-linux` MUST meet this promise:

| Promise | `agent-linux` |
|---|---|
| Libc family | `glibc` |
| Libc minimum version | 2.43 |
| Distro | Ubuntu 26.04 LTS, patched to the latest packages of that release each time the host starts |
| Node | The newest Node.js LTS release, as the default, brought up to date at each start, best effort: a failed update keeps the previous one. `fnm` is installed, and switches to a project's `.node-version` |
| `uv` | The latest release, brought up to date at each start, best effort: a failed update keeps the previous one |
| Claude Code | The native install, which updates itself |
| Codex | The latest release, brought up to date at each start, best effort: a failed update keeps the previous one |
| `gh` | The GitHub CLI, from GitHub's own package repository, patched with the distro |

- **Why 2.43:** Ubuntu 26.04 LTS ships glibc 2.43 and keeps its upstream version fixed for the
  release's whole life, so the start-time patch changes only the Ubuntu revision; a host with an
  older glibc would be a new profile version, never `agent-linux`.
- **The tools are promised on the `PATH` of an interactive login shell of `ssh.user`.** Whether a
  command run without one, as in `ssh <host_alias> node --version`, finds them is an
  [open question](#open-questions).
- **Nothing else is promised.** Anything else a host has, such as another language runtime, MAY
  change or go without a new contract version.
- **A versioned profile, `agent-linux@<n>`, will state its own promise** in this section when one
  exists, and MAY differ from this one in any line.

Note on where this came from: the box's own setup, `box/bootstrap.sh` and the start-time patch in
`box/system/`. On 2026-11-18 the box reported `ldd (Ubuntu GLIBC 2.43-2ubuntu2.7) 2.43`, Ubuntu
26.04.1 LTS on `aarch64`, Node v26.13.1, uv 0.13.9, Claude Code 2.1.326, codex-cli 0.170.0 and gh
2.105.0. Those versions are an observation, not the promise.

## Commands

**The commands, from the top level down.** Each section's help block is exactly what `--help`
prints at that level, to stdout with exit 0. An implementation MUST print it byte for byte, with a
line feed after every line, and a change to either MUST change both.

**Each command's errors are listed with it.** Every command can also fail with `usage` for the
reasons in [Command lines](#command-lines), and with `internal` when its lease records cannot be
read, or, for `create`, `renew` and `release`, cannot be written or are busy for 30 seconds
([Messages](#messages)).

### agentspace

```
agentspace [--help]
agentspace <command> ...
```

**The top level.** In v0, `lease` is the only command, and any other is `usage`. A later version
MAY add operator commands, which are outside this contract.

| Flag | Meaning |
|---|---|
| `-h`, `--help` | Print the help below and exit 0. |

**Help:**

```
Usage: agentspace <command> [<subcommand>] [<args>] [--help]

Leases compute for coding agents.
A lease is compute for one agent for a bounded time, reached over SSH.

Commands:
  lease       Create, show, renew, release and list leases

Options:
  -h, --help  Print this help and exit

Every command prints one JSON document on stdout, or an error document on stderr.
Run 'agentspace lease --help' for the lease subcommands, states and exit codes.
```

### agentspace lease

```
agentspace lease [--help]
agentspace lease <subcommand> ...
```

**The lease commands.** The subcommands are `create`, `get`, `renew`, `release` and `list`.

| Flag | Meaning |
|---|---|
| `-h`, `--help` | Print the help below and exit 0. |

**Help:**

```
Usage: agentspace lease <subcommand> [<args>] [--help]

Manages leases. A lease is compute for one agent for a bounded time.
A ready lease carries an SSH host alias, the user it logs in as, and an expiry.

Subcommands:
  create   Request a lease for a project
  get      Show one lease
  renew    Issue a ready lease a new certificate with a new expiry
  release  Give a lease back and delete its local credential and alias
  list     Show leases, optionally filtered by project and state

Options:
  -h, --help  Print this help and exit

States:
  pending        Accepted and not yet ready; poll with get
  ready          Connect with 'ssh <host alias>' until the expiry
  expired        The expiry has passed; new connections are refused
  released       Given back; its local credential and alias are gone
  unsatisfiable  Can never be met as asked; the lease's reasons say why

Exit codes:
  0  Success; stdout holds one JSON document
  1  Internal error
  2  Usage error, or a value that is malformed or out of range
  3  No lease has that ID
  4  The lease's state does not allow the subcommand
```

### agentspace lease create

```
agentspace lease create --project <name> [--platform <os/arch>] [--profile <name>]
                        [--size <size>] [--ttl <duration>]
```

**Requests a lease for one agent of a project, and prints it.** The lease's state is `ready`,
`pending` or `unsatisfiable`, and a caller MUST check it before connecting:
- **`ready`:** `ssh.host_alias` is usable from the caller's OpenSSH configuration, and
  `expires_at` is the issue time plus the TTL ([The credential](#the-credential)).
- **`pending`:** call `get` until it is `ready` or `unsatisfiable`.
- **`unsatisfiable`:** `reasons` says which values cannot be met. Nothing was issued, and the lease
  needs no `release`.

**An unsatisfiable lease is a successful `create` (exit 0)**, because the lease was recorded and
`get` returns it. A caller that only checks the exit code will miss it, which is why the state MUST
be checked.

**A platform, profile or size that cannot be met is never swapped for another (D5).** If any one of
them cannot be met, the lease is `unsatisfiable`, whatever the others are. Whether a request can be
met is decided before anything is issued.

| Flag | Type | Default | Notes |
|---|---|---|---|
| `--project <name>` | [Project](#project) | None; required | The project the agent works on. |
| `--platform <os/arch>` | [Platform](#platform) | `linux/arm64` | Only `linux/arm64` is supported in v0. |
| `--profile <name>` | [Profile](#profile) | `agent-linux` | Only `agent-linux` is supported in v0. |
| `--size <size>` | [Size](#size) | `m` | Only `m` is supported in v0. |
| `--ttl <duration>` | [TTL](#ttl) | `8h` | From 1 minute to 24 hours. |
| `-h`, `--help` | | | Print the help below and exit 0. |

**Request document** (never read by the v0 CLI; see [Schemas](#schemas)):
[`lease-create.input.schema.json`](../schemas/lease-create.input.schema.json), with members
`project`, `platform`, `profile`, `size` and `ttl`.

**Output:** the lease document. **Exit codes:** 0, 1 and 2.

**Errors:**

| Code | Field | When |
|---|---|---|
| `usage` | `project` | No `--project`, or `--project` with no value, an empty value, or given twice. |
| `usage` | The flag | Any other flag with no value, an empty value, or given twice. |
| `usage` | None | An unknown option, or any positional argument. |
| `invalid_value` | `project`, `platform`, `profile`, `size` or `ttl` | The first value, in the order of the table above, outside its accepted forms. |
| `out_of_range` | `ttl` | A TTL below 1 minute or above 24 hours. |
| `internal` | None | The credential could not be issued, agentspace's CA is not set up, the alias could not be set up, or the lease records could not be read, written or locked. No lease was recorded. What a failed `create` removes, what it MAY keep in the OpenSSH configuration, and what its message MUST name when a removal fails are in [Concurrency and atomicity](#concurrency-and-atomicity). |

**Help:**

```
Usage: agentspace lease create --project <name> [--platform <os/arch>] [--profile <name>]
                               [--size <size>] [--ttl <duration>]

Requests a lease for one agent of a project, and prints it as JSON.
Check the lease's state before connecting: ready, pending or unsatisfiable.
A ready lease's host alias is in your OpenSSH configuration: ssh <host alias>.
A pending lease becomes ready or unsatisfiable; poll it with get.
An unsatisfiable lease lists its reasons. Nothing was issued, and the exit code is still 0.

Options:
  --project <name>      Required. Letters, digits and '-', up to 32; lowercased.
  --platform <os/arch>  Default linux/arm64. Also accepts aarch64 for arm64.
  --profile <name>      Default agent-linux. Accepts name or name@version.
  --size <size>         Default m.
  --ttl <duration>      Default 8h. From 1m to 24h, never clamped.
                        Examples: 8h, 90m, 1h30m, '2 hours', PT8H.
                        A bare number is refused: give a unit.
  -h, --help            Print this help and exit

In v0 only linux/arm64, agent-linux and size m can be met.
Any other well-formed value gives an unsatisfiable lease, never a substitute.
```

### agentspace lease get

```
agentspace lease get <id>
```

**Prints one lease as it stands now.** It changes nothing. A `ready` lease whose `expires_at` has
passed is printed as `expired`.

| Argument or flag | Type | Notes |
|---|---|---|
| `<id>` | [Lease ID](#lease-id) | Required. |
| `-h`, `--help` | | Print the help below and exit 0. |

**Request document** (never read by the v0 CLI; see [Schemas](#schemas)):
[`lease-get.input.schema.json`](../schemas/lease-get.input.schema.json), with the member `id`.

**Output:** the lease document. **Exit codes:** 0, 1, 2 and 3.

**Errors:**

| Code | Field or member | When |
|---|---|---|
| `usage` | `id` | No lease ID. |
| `usage` | None | A second positional argument, or any option other than help. |
| `invalid_value` | `id` | The ID is not a [lease ID](#lease-id). |
| `not_found` | `lease_id` | No kept lease has that ID. |
| `internal` | None | The lease records could not be read. |

**Help:**

```
Usage: agentspace lease get <id>

Prints one lease as JSON, as it stands now. Changes nothing.
A ready lease whose expiry has passed is shown as expired.

Arguments:
  <id>        The lease ID, a UUIDv4 in any case

Options:
  -h, --help  Print this help and exit

Exit 3 means no lease has that ID.
```

### agentspace lease renew

```
agentspace lease renew <id> [--ttl <duration>]
```

**Issues a `ready` lease a new certificate, valid from now for the TTL, and prints the lease.**
- **`expires_at` becomes the new certificate's issue time plus the TTL**, whether that is later or
  earlier than before.
- **The previous certificate is not revoked (D10), and stays valid until its own expiry.** When
  `renew` shortens a lease, that expiry is later than the new `expires_at`, so a leaked copy of the
  previous certificate outlives it.
- **`ttl_seconds` becomes the TTL used.** Without `--ttl`, the lease's current `ttl_seconds` is
  used again.
- **The key, the alias, the user, the ID and `created_at` do not change**, so clients that saved
  the alias need nothing new.
- **A retry is safe:** each call sets a new expiry from its own time, and changes nothing else.
- **There is no limit on how many times a lease is renewed.** Each certificate is limited by the
  maximum TTL, and that is what bounds a leaked one (D24).

**Only a `ready` lease can be renewed.** Any other state is `invalid_state` (exit 4) and changes
nothing:
- **`expired`:** an expired lease stays expired. Renew before `expires_at`, or `create` a new
  lease. A lease's compute MAY already be gone once it expires, so no backend could promise to
  bring it back.
- **`released` and `unsatisfiable`:** both are terminal.
- **`pending`:** it has no credential yet. Wait for `ready`.

**A lease that is `ready` MUST be renewable, and one that has ever been `ready` MUST be releasable,
for as long as it is kept (#0041).** Whatever agentspace needs to renew or release it MUST be kept
with the lease from the moment it becomes `ready`; how is [docs/state.md](state.md)'s.

| Argument or flag | Type | Default | Notes |
|---|---|---|---|
| `<id>` | [Lease ID](#lease-id) | None; required | |
| `--ttl <duration>` | [TTL](#ttl) | The lease's `ttl_seconds` | From 1 minute to 24 hours. |
| `-h`, `--help` | | | Print the help below and exit 0. |

**Request document** (never read by the v0 CLI; see [Schemas](#schemas)):
[`lease-renew.input.schema.json`](../schemas/lease-renew.input.schema.json), with members `id` and
`ttl`.

**Output:** the lease document. **Exit codes:** 0, 1, 2, 3 and 4.

**Errors:**

| Code | Field or members | When |
|---|---|---|
| `usage` | `id` | No lease ID. |
| `usage` | `ttl` | `--ttl` with no value, an empty value, or given twice. |
| `usage` | None | A second positional argument, or an unknown option. |
| `invalid_value` | `id`, then `ttl` | The ID is not a [lease ID](#lease-id), or else the TTL is not a [TTL](#ttl). |
| `out_of_range` | `ttl` | A TTL below 1 minute or above 24 hours. |
| `not_found` | `lease_id` | No kept lease has that ID. |
| `invalid_state` | `lease_id` and `state` | The lease is not `ready`; `state` is the state it is in. |
| `internal` | None | The certificate could not be issued, agentspace's CA is not set up, or the lease records could not be read, written or locked. The lease is unchanged. |

**Help:**

```
Usage: agentspace lease renew <id> [--ttl <duration>]

Issues a ready lease a new certificate, valid from now for the TTL, and prints the lease.
The key, host alias and user stay the same.
The old certificate is not revoked. It stays valid until its own expiry.
If this renew shortens the lease, that expiry is later than the new one.
Only a ready lease can be renewed. An expired lease cannot: create a new one.

Arguments:
  <id>              The lease ID, a UUIDv4 in any case

Options:
  --ttl <duration>  Default: the lease's current TTL. From 1m to 24h, never clamped.
                    Examples: 8h, 90m, 1h30m, '2 hours', PT8H.
  -h, --help        Print this help and exit

Exit 3 means no lease has that ID.
Exit 4 means the lease is not ready; the error names its state.
```

### agentspace lease release

```
agentspace lease release <id>
```

**Gives a lease back, and prints it.** What it does depends on the state:

| State before | What `release` does | State after |
|---|---|---|
| `ready` | Deletes the key, certificate and alias, drops `ssh`, keeps `expires_at`, and sets `released_at` | `released` |
| `expired` | The same | `released` |
| `pending` | Gives up the request, and sets `released_at`; the lease has no `expires_at` | `released` |
| `released` | Deletes whatever is left of the lease's key, certificate and alias, and changes nothing else; prints the lease with its first `released_at` | `released` |
| `unsatisfiable` | Nothing; prints the lease unchanged | `unsatisfiable` |

- **Release is idempotent:** releasing a lease again succeeds and prints the same lease, so a
  caller MAY retry it freely.
  - **The help's "Releasing a released or unsatisfiable lease changes nothing" means that its
    record does not change** (architect, 2026-11-19, gap FB-03). The table above binds: a second
    `release` of a `released` lease still deletes what a failed one left. The help text stays as
    it is, since the conformance suite compares it with this page byte for byte.
  - Note on where this came from: the prototype's `release` of a `released` lease touches no
    files. It never needs to, because it deletes the files before it records `released`; that
    order is #0041's defect, and the rule above is what lets an implementation record first.
- **Release does not revoke any certificate (D10).** A leaked copy of the current certificate works
  until `expires_at`, which a released lease still shows when it had one. A leaked copy of an older
  certificate works until its own expiry, which MAY be later if a `renew` shortened the lease.
  Connections already open are not closed.
- **Release needs no CA,** and works on a machine whose CA is missing.
- **An ID that names no lease is `not_found` (exit 3)**, not a success: there is no lease document
  to print, and success would hide a mistyped ID.
- **A caller retrying `release` MAY treat exit 3 as nothing left to release.** A lease is kept for at
  least 24 hours after it is released, so a real retry finds it and stays idempotent.
- **A release that fails part way MUST NOT leave the lease `ready` or `expired` without its
  credential (#0041);** see [Concurrency and atomicity](#concurrency-and-atomicity).

| Argument or flag | Type | Notes |
|---|---|---|
| `<id>` | [Lease ID](#lease-id) | Required. |
| `-h`, `--help` | | Print the help below and exit 0. |

**Request document** (never read by the v0 CLI; see [Schemas](#schemas)):
[`lease-release.input.schema.json`](../schemas/lease-release.input.schema.json), with the member
`id`.

**Output:** the lease document. **Exit codes:** 0, 1, 2 and 3.

**Errors:**

| Code | Field or member | When |
|---|---|---|
| `usage` | `id` | No lease ID. |
| `usage` | None | A second positional argument, or any option other than help. |
| `invalid_value` | `id` | The ID is not a [lease ID](#lease-id). |
| `not_found` | `lease_id` | No kept lease has that ID. |
| `internal` | None | The credential or alias could not be deleted, or the lease records could not be read, written or locked. |

**Help:**

```
Usage: agentspace lease release <id>

Gives a lease back, and prints it as JSON.
Deletes the lease's local key, certificate and host alias.
No certificate is revoked. Each stays valid until its own expiry.
An older certificate's expiry is later than the lease's if a renew shortened it.
Connections already open are not closed.
Releasing a released or unsatisfiable lease changes nothing and exits 0.

Arguments:
  <id>        The lease ID, a UUIDv4 in any case

Options:
  -h, --help  Print this help and exit

Exit 3 means no lease has that ID.
```

### agentspace lease list

```
agentspace lease list [--project <name>] [--state <state>[,<state>...]]...
```

**Prints the leases that match every filter given, oldest first.** It changes nothing.
- **Order:** by `created_at`, then by `id`, both ascending, comparing the strings character by
  character, so the order never depends on a locale.
- **No filter means every lease that is kept** (see "How long a lease is kept" under
  [States](#states)).
- **No match is not an error:** the output is `{"leases":[]}`, exit 0.
- **States are decided as `get` decides them, at one instant for the whole list**, so `--state
  ready` leaves out a lease whose `expires_at` has passed, and `--state expired` includes it.

| Flag | Type | Default | Notes |
|---|---|---|---|
| `--project <name>` | [Project](#project) | Every project | Matches the normalized name exactly. |
| `--state <state>` | [State filter](#state-filter) | Every state | MAY be repeated, and MAY be a comma-separated list. |
| `-h`, `--help` | | | Print the help below and exit 0. |

**Request document** (never read by the v0 CLI; see [Schemas](#schemas)):
[`lease-list.input.schema.json`](../schemas/lease-list.input.schema.json), with members `project`
and `state`.

**Output:** a list document, [`lease-list.output.schema.json`](../schemas/lease-list.output.schema.json):

| Field | Type | Present | Meaning |
|---|---|---|---|
| `leases` | Array of lease documents | Always | The matching leases, in the order above. |

**Exit codes:** 0, 1 and 2.

**Errors:**

| Code | Field | When |
|---|---|---|
| `usage` | `project` or `state` | `--project` with no value, an empty value, or given twice; `--state` with no value or an empty value. |
| `usage` | None | An unknown option, or any positional argument. |
| `invalid_value` | `project`, then `state` | A project outside its accepted forms, or else a state name that is not one of the five. |
| `internal` | None | The lease records could not be read. |

**Help:**

```
Usage: agentspace lease list [--project <name>] [--state <state>[,<state>...]]...

Prints the leases that match every filter, oldest first, as {"leases":[...]}.
Changes nothing. No match prints an empty list and exits 0.

Options:
  --project <name>  Only this project's leases
  --state <state>   Only leases in these states. Repeat it, or separate states with commas.
                    States: pending, ready, expired, released, unsatisfiable.
  -h, --help        Print this help and exit
```

## Exit codes and errors

| Exit | Error code | When |
|---|---|---|
| 0 | None | The command did what was asked, or printed help. Stdout holds its document or help. |
| 1 | `internal` | Anything that is not the caller's fault, such as a credential that could not be issued or lease records that could not be read. |
| 2 | `usage` | An unknown or missing command, subcommand, flag or argument, a flag given twice, or a flag with no value or an empty value ([Command lines](#command-lines)). |
| 2 | `invalid_value` | A value outside its [accepted forms](#values). |
| 2 | `out_of_range` | A well-formed value outside its allowed range; in v0, only the TTL. |
| 3 | `not_found` | No kept lease has the ID given. |
| 4 | `invalid_state` | The lease's state does not allow the subcommand; in v0, only `renew`. |

**An exit code not in the table MUST NOT be used.** Each non-zero exit code MUST come with one of
the error codes beside it, and every failure that fits none of the others is `internal`.

**When several errors apply, the first in this order MUST be reported, and only that one:**
1. **`usage`**, for anything wrong with the command line itself, chosen as
   [Command lines](#command-lines) says.
2. **`invalid_value` or `out_of_range`**, for the first bad value in the order of the command's
   argument and flag table, so `field` names the first bad one.
3. **`internal`, when the lease records cannot be read or are busy**, and the no-home `internal`
   ([Messages](#messages)), which belongs here when the state folder is the default one under the
   home (architect, 2026-11-19, gap FB-22). **A command fails with no home only when it needs a
   path under the home folder, and at the first point it needs one:** with `AGENTSPACE_STATE_DIR`
   set, `get` and `list` need none and run, and `create`, `renew` and `release` read their records
   first, so a `not_found` or `invalid_state` comes before the no-home failure, and a `release` of
   a `pending` or `unsatisfiable` lease, which touches no file, succeeds
   ([docs/state.md](state.md#the-home-folder)).
4. **`not_found`**, from looking the lease up.
5. **`invalid_state`**, from the lease's state.
6. **Any other `internal`.**

So `renew <id of an expired lease> --ttl 48h` is `out_of_range` (exit 2),
`renew <unknown id> --ttl 8` is `invalid_value` for `ttl`, and `get <bad id> --ttl 8h` is `usage`.

**The error document** goes to stderr, compact, on one line. Its schema is
[`error.output.schema.json`](../schemas/error.output.schema.json).

| Field | Type | Present | Meaning |
|---|---|---|---|
| `code` | String | Always | One of the error codes above. |
| `message` | String | Always | One or more sentences for a person: what was wrong, and what to do. Never empty. |
| `field` | String | `invalid_value` and `out_of_range`; `usage` in the five cases below | The flag or request member at fault, without dashes: `id`, `project`, `platform`, `profile`, `size`, `ttl` or `state`. |
| `lease_id` | String, lowercase UUIDv4 | `not_found` and `invalid_state` | The normalized ID asked for. |
| `state` | String, [state](#states) | `invalid_state` | The state the lease is in. |

**A `usage` error MUST carry `field` in exactly these five cases, and MUST NOT otherwise:** a flag
with no value, a flag with an empty value, a flag given twice (each naming that flag), a missing
lease ID (`id`) and a missing `--project` (`project`).

**A message MUST NOT echo a secret, and MUST NOT name a backend, a location or a machine (D1).** A
message SHOULD quote the value that was refused, and SHOULD say how to fix it, as in `TTL '8' has
no unit. Give one, as in '8h'.`

**A message MUST NOT contain a control character** (U+0000 to U+001F, or U+007F), MUST NOT be empty,
and MUST NOT start with a space; the schema refuses each. A quoted value that contains a control
character MUST have each written as `\u` and four lowercase hex digits, as in `\u000a`, or MUST be
left out of the message. In the JSON document that backslash is itself escaped, as `\\u000a`.

**An `internal` message MUST say what failed in agentspace's own words (#0041).** It MUST NOT
carry:
- **An absolute path on the caller's machine, or the caller's user name.** A file a person has to
  fix MAY be named in its home-relative form, such as `~/.ssh/config`, but never inside a
  template's `<why>` ([Messages](#messages)).
- **The output or error text of another program**, such as `ssh-keygen` or the operator's CA
  check.
- **A backend, a location or a machine (D1).**

**The original error is not kept in contract v0.** A build MAY drop it, as the prototype does, and
MUST NOT put it in the error document, on stdout, or in any file [docs/state.md](state.md) lists. A
place to keep it for diagnosis, such as a log, is later and not blocking. A message such as
`agentspace lease create failed: The lease's credential could not be issued. Nothing was
recorded; try again.` is the shape meant.

### Messages

**Messages are for people.** A caller MUST NOT parse a message; the code, `field`, `lease_id` and
`state` are what a program reads. An implementation SHOULD word its messages as below, and MAY word
them differently where this page does not say MUST.

**Which template fits the cases the table does not spell out** (architect, 2026-11-19, gap FA-08):
- **A TTL of only whitespace** is a value, not an empty one ([Command lines](#command-lines)), and
  takes the TTL's "anything else" template.
- **An ISO form whose `M` comes before any `T`**, such as `P8M`, means months. It takes the "uses
  'M'" template when its `M` is uppercase and ends the value or is followed by a character that is
  not a letter, as the template's row says, so `P8M` and `P1M` take it, and `P1MT1H`, whose `M`
  is followed by `T`, takes "not a duration" (corrected by the architect, 2026-11-19, from
  E0.05-T01-S02's review).
- **A read of the state file that fails for a reason other than its content**, such as access
  denied or a folder at its path, takes the "Records unreadable" template with its own `<why>`.

**In each template:**
- **`'<value>'` is the value as given, untrimmed, in single quotes**, with control characters
  escaped as above. For an unknown option, an unknown command or subcommand, or an unexpected
  argument, it is the whole argument as given, so `--help=yes` is quoted as `'--help=yes'`.
- **`<flag>` is the flag's name in lowercase, `<sub>` is the subcommand, and `<id>` is the
  normalized lease ID.**
- **`<why>` is a short reason in agentspace's own words.** It MUST NOT carry a path, a user name or
  another program's output, and names a file by what it is, such as "the OpenSSH client
  configuration".

| Code | When | Message |
|---|---|---|
| `usage` | Unknown option, top level | `Unknown option '<value>'. Run 'agentspace --help' for its options.` |
| `usage` | Unknown option, `lease` level | `Unknown option '<value>'. Run 'agentspace lease --help' for its options.` |
| `usage` | Unknown option, a subcommand | `Unknown option '<value>' for agentspace lease <sub>. Run 'agentspace lease <sub> --help'.` |
| `usage` | Unknown command | `Unknown command '<value>'. Run 'agentspace --help' for the commands.` |
| `usage` | Unknown subcommand | `Unknown subcommand '<value>'. Run 'agentspace lease --help' for the subcommands.` |
| `usage` | No command | `No command given. Run 'agentspace --help' for the commands.` |
| `usage` | No subcommand | `No subcommand given. Run 'agentspace lease --help' for the subcommands.` |
| `usage` | No value | `Option --<flag> needs a value. Run 'agentspace lease <sub> --help'.` |
| `usage` | Empty value | `Option --<flag> has an empty value. Run 'agentspace lease <sub> --help'.` |
| `usage` | Given twice | `Option --<flag> is given twice. Give it once.` |
| `usage` | Unexpected argument | `Unexpected argument '<value>' for agentspace lease <sub>. Run 'agentspace lease <sub> --help'.` |
| `usage` | No lease ID | `No lease ID given. Run 'agentspace lease <sub> --help'.` |
| `usage` | No `--project` | `Option --project is required. Run 'agentspace lease create --help'.` |
| `invalid_value` | `id` | `Lease ID '<value>' is not a UUIDv4. Give the id that create printed.` |
| `invalid_value` | `project` | `Project '<value>' is not a project name. Use 1 to 32 letters, digits and '-', starting and ending with a letter or digit.` |
| `invalid_value` | `platform` | `Platform '<value>' is not an os/arch pair. Give one as in 'linux/arm64'.` |
| `invalid_value` | `profile` | `Profile '<value>' is not a profile name. Give name or name@version, as in 'agent-linux'; a version is 1 or more.` |
| `invalid_value` | `size` | `Size '<value>' is not a size. Use 1 to 8 letters and digits, as in 'm'.` |
| `invalid_value` | `ttl`, all digits once trimmed | `TTL '<value>' has no unit. Give one, as in '8h'.` |
| `invalid_value` | `ttl`, anywhere in the trimmed value, a digit, optional whitespace, then an uppercase `M` that ends the trimmed value or is followed by a character that is not a letter, so `1M30s` and `P1M` take it and `8Mx` does not (architect, 2026-11-19) | `TTL '<value>' uses 'M', which could mean months or minutes. Write minutes as 'm' or 'min'.` |
| `invalid_value` | `ttl`, anything else | `TTL '<value>' is not a duration. Give one as in '8h', '90m', '1h30m', '2 hours' or 'PT8H'.` |
| `invalid_value` | `state` | `State '<name>' is not a state. Use pending, ready, expired, released or unsatisfiable.`, where `<name>` is the one name refused, trimmed, and so `''` for an empty name |
| `out_of_range` | `ttl` | `TTL '<value>' is outside the allowed range. Give from 1m to 24h; it is never clamped.` |
| `not_found` | Any subcommand | `No lease has ID '<id>'.` The prototype's `renew` and `release` say `No lease has the ID '<id>'.` |
| `invalid_state` | `renew` | `Lease '<id>' is <state>, and only a ready lease can be renewed. <advice>`, where `<advice>` is `Create a new lease.` for `expired`, `Wait for it to be ready.` for `pending`, and `The <state> state is final.` for `released` and `unsatisfiable` |
| `internal` | Records unreadable | `The lease records could not be read: <why>. Nothing was changed.`, where `<why>` is `the state file is not valid` for a file that does not validate, and otherwise says what kept it from being read, as in `the state file could not be opened` (gap FA-08) |
| `internal` | Key missing (`renew`) | `agentspace lease renew failed: The lease's key could not be found, so no certificate can be issued. Nothing was changed.` (gap FA-08) |
| `internal` | Records busy | `The lease records are busy: another agentspace lease command held them for 30 seconds. Try again.` |
| `internal` | Records unwritable | `The lease records could not be written: <why>. Nothing was changed.`, where `<why>` is, for example, `two records share an ID` or `the change is not valid` |
| `internal` | No CA (`create`, `renew`) | `agentspace lease <sub> failed: agentspace's certificate authority has not been set up, so no certificate can be issued. Nothing was changed; the operator sets it up.` |
| `internal` | Alias cannot be set up (`create`) | `agentspace lease create failed: The host alias could not be added to the OpenSSH client configuration: <why>. No lease was recorded.`, where `<why>` says what a person has to fix, as in `its Include line for agentspace is inside a Host block, and has to move to the top of the file`. It does not say that nothing was changed, since the line that pulls aliases in MAY have been added ([Concurrency and atomicity](#concurrency-and-atomicity)) |
| `internal` | CA not usable (`create`, `renew`) | `agentspace lease <sub> failed: agentspace's certificate authority is not usable. Nothing was changed; the operator's ca command says why.` It never carries the check's own output ([docs/state.md](state.md#creation-in-order)) |
| `internal` | Way to reach the host not found (`create`) | `agentspace lease create failed: The lease's way to reach its host cannot be found: agentspace's ProxyCommand program is not installed beside this program. No lease was recorded.` ([docs/state.md](state.md#the-alias-file)) |
| `internal` | No home folder | `agentspace lease <sub> failed: The home folder is not set (USERPROFILE is unset or empty), so agentspace cannot find its files. Nothing was changed.`, for unset and empty alike (gap FA-08) ([docs/state.md](state.md#the-home-folder)) |
| `internal` | Records cannot be locked | `The lease records could not be locked: <why>. Nothing was changed.`, for a failure to open the lock that is not another command holding it ([docs/state.md](state.md#the-lease-state-lock)) |
| `internal` | No driver (`create`) | `agentspace lease create failed: No lease driver is available. Nothing was recorded.` |
| `internal` | Unknown stored driver (`renew`, `release`) | `agentspace lease <sub> failed: The lease was placed by a driver this agentspace does not have. Nothing was changed.` It never names the driver ([docs/driver.md](driver.md#the-drivers-name)) |
| `internal` | Alias file is not this lease's (`release`) | `agentspace lease release failed: The lease's host alias file belongs to another lease or to no lease, so nothing was deleted. A person has to move that file.` ([docs/state.md](state.md#release-and-what-it-leaves)) |
| `internal` | Credential or alias not deleted (`release`) | `agentspace lease release failed: The lease's local key, certificate or host alias could not be deleted: <why>. Run release again to finish.` |
| `internal` | Anything else | `agentspace lease <sub> failed: <what failed>`, where `<what failed>` meets the rules above. Before a subcommand is read, it is `agentspace lease failed: <what failed>`, and a message that would otherwise be empty is `agentspace lease failed.` |

## Environment

**These environment variables change where the CLI reads and writes.** Each is read when a command
starts, and one that is unset or empty means the default.

| Variable | When set and not empty |
|---|---|
| `AGENTSPACE_STATE_DIR` | Every command keeps its lease records in this folder instead of the default ([docs/state.md](state.md)). |
| `AGENTSPACE_SSH_DIR` | `create` makes the lease's alias usable from the OpenSSH client folder named here, instead of the calling user's own, and `release` removes it from there ([docs/state.md](state.md)). |
| `AGENTSPACE_PROXY_COMMAND` | `create` gives the alias this command as its way to reach the lease's host, instead of agentspace's own ([docs/driver.md](driver.md)). |

- **They exist so that a check, or a worktree, can run the CLI without touching the caller's real
  leases or OpenSSH configuration.** A caller in normal use MUST NOT set them.
- **A check that sets `AGENTSPACE_SSH_DIR` MUST keep the same value from `create` to `release`**,
  since `release` removes the alias from the folder named when it runs. `renew` does not touch the
  alias, and reads neither `AGENTSPACE_SSH_DIR` nor `AGENTSPACE_PROXY_COMMAND`.
- **No other environment variable changes the contract's behavior.** Variables that the caller's
  own system uses to name a home folder are [docs/state.md](state.md)'s.

## Schemas

**Every JSON document in this contract has a schema under [`schemas/`](../schemas), in JSON Schema
draft 2020-12, one file per document (D20).** The names follow `<document>.<direction>.schema.json`:

| Schema | Document | Used by |
|---|---|---|
| `lease.output.schema.json` | One lease | Output of `create`, `get`, `renew`, `release` |
| `lease-list.output.schema.json` | A list of leases | Output of `list` |
| `error.output.schema.json` | One error | Stderr of every command that fails |
| `lease-create.input.schema.json` | A `create` request | The flags of `create` |
| `lease-get.input.schema.json` | A `get` request | The argument of `get` |
| `lease-renew.input.schema.json` | A `renew` request | The argument and flag of `renew` |
| `lease-release.input.schema.json` | A `release` request | The argument of `release` |
| `lease-list.input.schema.json` | A `list` request | The flags of `list` |

- **Output schemas are strict:** `additionalProperties` is false everywhere, every field that is
  always present is required, each state and each error code requires and forbids its own fields,
  and IDs and times have patterns. `format` is given as well, but only as an annotation, since
  validators need not enforce it.
  - **A message, in an error or in a reason, MUST NOT be empty or start with a space**, and holds
    no control character.
  - **A lease's `reasons` holds each code at most once**, and so at most four. Their order is in
    [Fields](#fields); a schema cannot check it.
  - **The error schema does not check which `usage` errors carry `field`.** It allows `field` on
    any `usage` error and requires it on none, because the five cases that MUST carry it
    ([Exit codes and errors](#exit-codes-and-errors)) differ from the others only in what went
    wrong, which the document does not say in any member a schema can test. A conformance check
    MUST test that rule itself.
- **Every anchored output pattern ends with `$(?!\n)`, not a bare `$`.** In .NET's and Python's
  regular expressions, `$` also matches before a final line feed, so a bare `$` would accept
  `"swimcoach\n"`. `(?!\n)` means the same in ECMA-262, .NET and Python, and costs nothing where
  `$` is already strict.
- **Input schemas are tolerant in form and strict in meaning.** They accept the spellings in
  [Values](#values), allow `null` for an optional member to mean "not given", and refuse unknown
  members, because an unknown member is most likely a misspelled one whose value would otherwise
  be dropped. A range (the TTL limits) cannot be checked by a pattern, so passing the schema does
  not make a TTL acceptable. The contract sets no length limit on a TTL; a hosted API MAY limit a
  request's size.
- **Input patterns accept whitespace as one character class that lists the `White_Space`
  characters of [Input tolerance](#input-tolerance) and no others**, written out as
  `[\t-\r \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]`, never as `\s`. `\s`
  means a different set in each engine: ECMA-262's lacks U+0085 and has U+FEFF, and PowerShell's
  `Test-Json` matched only the ASCII ones when it was tried on 2026-11-18. The patterns still
  match the whole value: a `$` that also matches before a final line feed lets nothing through,
  because the class before it already accepts trailing whitespace, a line feed included, which is
  then trimmed. So U+FEFF, which is not trimmed, fails the schema as it fails the CLI.
- **An input schema MAY accept more than [Values](#values) does, and MUST NOT accept less.**
  Passing an input schema never makes a value acceptable: a TTL's range, for one, is checked
  after it is parsed, and [Values](#values) decides.
- **The v0 CLI takes flags and arguments only, and never reads a JSON request.** An input
  document defines a request: its members are the flags and arguments of the same names, it is
  what a request is validated against, and a hosted API (card #0011) MUST accept it as JSON. In
  JSON, insignificant whitespace is allowed anywhere.
- **Each schema is self-contained**, with shared parts copied into its own `$defs`, because
  PowerShell's `Test-Json -SchemaFile` does not resolve a `$ref` to another file. The lease
  definition in the list schema's `$defs` MUST stay identical to the lease schema's.
- **Each schema has samples in [`schemas/samples/`](../schemas/samples)**, named
  `<schema name>.<case>.json`, which MUST validate. Documents that MUST NOT validate are in
  `schemas/samples/invalid/` under the same naming, at least one per output schema. A check MUST
  validate both with a real draft 2020-12 validator and fail on any mismatch.
  - Note on where this came from: the prototype's check is `scripts/test-schemas.ps1`, which uses
    PowerShell's `Test-Json`; on 2026-11-18 the samples were also checked with Python's
    `jsonschema` 4.19.
- **A schema's `$id` names it and is not a download address.** Each `$id` carries the contract
  version, `v0`.
- **The lease state file's schema, `schemas/state/leases.state.schema.json`, is not part of this
  contract.** It is [docs/state.md](state.md)'s.

## Versioning

**This is contract v0, the MVP's.**
- **Any change after which an output could fail to validate against its v0 schema is a new
  contract version**, and so is a change to an output's meaning. Because output schemas forbid
  unknown fields, a new output field is a new version.
- **Accepting an input that v0 refused is not a new version.** Refusing one that v0 accepted is.
- **Supporting a platform, profile or size that v0 answered `unsatisfiable` is not a new version.**
  The fields have been in the contract from v0 so that it is not (D5).
- **Changing a message's wording is not a new version**, since no caller parses one.
- **Changing help text is not a new version either.** Help is for people, and a caller MUST NOT
  parse it. This page and the CLI still change together ([Commands](#commands)).
- **A profile with no version names its current promise ([Profiles](#profiles)), which MAY move to
  a newer distro release without a new contract version.** Its libc family MUST NOT change, and
  its libc minimum version MUST NOT go down.

## Hardening carried in

**Every point of the lease CLI's hardening and defect cards, and where this contract meets it
(D28).** Each card is OBE once the Rust CLI (v0.05, #0054) meets every requirement below. The
operator's cards, #0027 and #0034, are [docs/operator.md](operator.md)'s.

**#0029, harden lease credential issuing:**

| Point | Carried in as |
|---|---|
| 1. Clock skew: validity starts at the caller's now | The certificate's valid-after SHOULD be 5 minutes before its issue time, and `expires_at` stays the issue time plus the TTL (D24; [The credential](#the-credential)). |
| 2. The lease ID check lets a trailing line feed and a trailing dot through | Every check matches the whole value ([Input tolerance](#input-tolerance)), and everything derived from a lease ID comes from its normalized form, which never ends in a dot ([Lease ID](#lease-id)). |
| 3. The first lease on a fresh laptop creates a CA no host trusts | `create` and `renew` MUST NOT create the CA, and fail with `internal` when there is none ([The credential](#the-credential)). #0041 dropped its own copy of this point under D18, which was MVP only and is replaced by D28. |
| 4. A failed signing leaves the key behind | A failed `create` MUST remove every file it made, and never blocks a later `create`. A removal that fails is named in the error ([Concurrency and atomicity](#concurrency-and-atomicity)). |
| 5. PowerShell 7.0 to 7.2 drop an empty argument | Declined as a requirement on a runtime: the rewrite is Rust (D28), and the prototype's real minimum is PowerShell 7.5 anyway (D8). What it protects is required instead: the lease's key has no passphrase ([The credential](#the-credential)), and an empty argument reaches the parser as one ([The program](#the-program)). |
| 6. Every certificate has serial 0 | Declined for v0: a serial is not part of this contract, and is decided with revocation, card #0010 ([The credential](#the-credential)). |
| 7. Test gaps: the start time is not checked against the test's clock, and a UTC and local mix-up is unseen on a UTC machine | The conformance suite MUST check against its own clock, and run once in a zone other than UTC ([Times](#times)). |

**#0041, wrap credential errors, require driver, order writes:**

| Point | Carried in as |
|---|---|
| 1. Credential errors reach `internal` unwrapped, with laptop paths, the user name and CA-check output | An `internal` message carries no absolute path, user name or other program's output ([Exit codes and errors](#exit-codes-and-errors)). |
| 2. A `ready` record without its driver can be neither renewed nor released | A `ready` lease MUST be renewable, and one ever `ready` releasable, for as long as it is kept ([`renew`](#agentspace-lease-renew)). How the record keeps what that needs is [docs/state.md](state.md)'s, so its schema is left to that story (E0.04-T02-S01). |
| 3. Release deletes files before writing the record, and renew re-signs before writing | A failed `renew` leaves the lease's expiry, TTL and certificate as they were, and a failed `release` never leaves a `ready` or `expired` lease without its credential; a retried `release` deletes whatever a failed one left ([Concurrency and atomicity](#concurrency-and-atomicity)). |

**#0083, arguments refused or misread before parsing:**

| Point | Carried in as |
|---|---|
| 1. An argument the runtime refuses before the CLI starts (`-:x`), or a runtime version check that fails, exits 1 with no error document | Every run that fails ends with one error document and an exit code from the table ([The program](#the-program)). |
| 2. A line feed inside a `--flag=value` value makes it an unknown option, while the separate form gives `invalid_value` | The two forms give the same result for any value that is not a help request ([Command lines](#command-lines)). |

## Open questions

**What reading the prototype and its docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080), every question here is either closed or marked "Later, not blocking", and
none blocks building a drop-in of contract v0.

1. **Closed by D8 (architect, 2026-11-18): contract v0's caller runs on Windows**, from cmd,
   PowerShell and Git Bash ([The program](#the-program)); the program's file name is outside the
   contract. Later, not blocking: a caller on Linux or macOS, which comes with the hosted control
   plane (#0011).
2. **Later, not blocking:** [The program](#the-program) requires every argument to arrive
   unchanged and leaves the file name outside the contract, so a drop-in meets that rule with its
   own executable, and only whether the prototype's `.cmd` meets it is open.
   **How `bin\agentspace.cmd` passes arguments that cmd treats specially**, such as `%`, `^`, `&`,
   `!` and embedded quotes. [The program](#the-program) requires them unchanged; only a run on
   Windows shows whether the prototype meets that.
3. **Closed (E0.04-T04-S01, 2026-11-18): the Unicode `White_Space` characters, listed in
   [Input tolerance](#input-tolerance), and U+FEFF is not trimmed.** The input schemas now list the
   same set, where their `\s` matched a different one in each engine ([Schemas](#schemas)). The
   prototype trims what .NET's `String.Trim` calls
   whitespace, which reading suggests is the same set; a run would confirm it, and a difference
   there is the prototype's, not the contract's.
4. **Later, not blocking:** a string MAY use any escape that JSON allows, and a caller MUST
   compare parsed values, never bytes ([Output](#output)), so any valid escaping a drop-in chooses
   is correct.
   **How the prototype escapes strings in its JSON output**, for instance whether `'`, `<`, `>`,
   `&` or non-ASCII characters come out as `\u` escapes. This page allows any valid escaping; a
   conformance check that compares bytes would need to know.
5. **Closed by D24: the back-dating of a certificate's valid-after time (#0029)**, which D24 sets
   at 5 minutes, as a SHOULD ([The credential](#the-credential)). Kept here so the numbers of the
   questions below do not change.
6. **Later, not blocking:** [`agent-linux`](#agent-linux) promises the tools only on the `PATH` of
   an interactive login shell, and keeping that promise is the host's, not the CLI's, so a drop-in
   prints the same lease either way.
   **Whether `agent-linux`'s tools are on the `PATH` of a command run without an interactive login
   shell**, as in `ssh <host_alias> node --version`. Today Node and Codex are put on the `PATH` by
   lines at the end of `ubuntu`'s `~/.bashrc`, after Ubuntu's early return for non-interactive
   shells. Only a run through an alias settles what such a command finds.
7. **Later, not blocking:** v0's alias carries the forwards as
   [docs/state.md](state.md#the-alias-file) gives them, and a caller MUST NOT rely on them
   ([The credential](#the-credential)), so moving them to the keeper (#0008) changes nothing a
   caller sees.
   **Whether the board's forwards belong in the lease's alias at all** (D12, D30, card #0008), or
   should move to the keeper's own connection. Today a caller's second session on a host warns
   that its remote port forwarding failed.
8. **Later, not blocking:** every v0 lease logs in as `ubuntu` (D3), which the pattern accepts,
   and a user it would refuse comes only with a later driver's host (v0.13, #0081).
   **Whether `ssh.user`'s pattern fits every host a later driver serves** (D4, v0.13, #0081). It
   accepts 1 to 64 letters, digits, `.`, `_` and `-`, not starting with `.` or `-`, which covers
   Unix and Windows local user names. A Windows domain user (`DOMAIN\user` or `user@domain`), or a
   name with a space, would not fit, and widening the pattern later would be a new contract
   version ([Versioning](#versioning)).
9. **Closed by [docs/state.md](state.md#what-moves-which-files): where `AGENTSPACE_STATE_DIR`
   reaches.** It moves the lease records only; a check that isolates the lease's key and
   certificate, and the CA, sets `USERPROFILE` to a throwaway folder.
10. **Closed for contract v0 (E0.04-T04-S01, 2026-11-18): an `internal` error's original detail is
    not kept** ([Exit codes and errors](#exit-codes-and-errors)), as in the prototype. Later, not
    blocking: a place to keep it for diagnosis, such as a log.
11. **Later, not blocking:** the 30-second limit is fixed
    ([Concurrency and atomicity](#concurrency-and-atomicity)), so how long the prototype takes is a
    run's observation, not behavior a drop-in copies.
    **How long a `create` holds other changing commands back.** The prototype holds its lock
    through the whole of issuing, which starts a second shell for the CA check and runs
    `ssh-keygen` three times. Only a run shows whether that comes near the 30-second limit after
    which a concurrent `renew` or `release` fails as busy.
12. **Closed by [docs/state.md](state.md#the-lease-folder) (E0.04-T04-S01, 2026-11-18): whether the
    key is readable by no other ordinary user in practice.** The prototype sets no access rules of
    its own on the key; Windows' `ssh-keygen` gives it a protected list of its own, which on
    2026-11-18 met the requirement. state.md gives the rules a build that sets them itself MUST
    set; whether every Windows build's `ssh-keygen` does the same is state.md's Q6.

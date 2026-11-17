# agentspace lease API, contract v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for `agentspace lease`, written so that it can be built and used without reading any
of agentspace's code (decision D20).** It names every command, flag, output field, state and exit
code, and gives the `--help` text at every level word for word. The JSON schemas under
[`schemas/`](../schemas) are part of this contract, and where this page and a schema disagree, both
are wrong and MUST be fixed together.

**What it is for:** a project asks for a lease, which is compute for one agent for a bounded time,
and gets back an SSH host alias, the user to log in as, and an expiry (D2). The reasoning behind
each rule is in [decisions.md](decisions.md), cited as `D<n>`.

**What it never says:** nothing in a request, a response, an error or a help text MAY name a
backend, a machine type, a location or a particular machine (D1). A lease looks the same whatever
serves it, and a reason or message that would need such a name MUST be written without it.

## Contents

- [Conventions](#conventions)
- [The lease](#the-lease)
- [Values](#values)
- [Commands](#commands): [`agentspace`](#agentspace), [`agentspace lease`](#agentspace-lease),
  [`create`](#agentspace-lease-create), [`get`](#agentspace-lease-get),
  [`renew`](#agentspace-lease-renew), [`release`](#agentspace-lease-release),
  [`list`](#agentspace-lease-list)
- [Exit codes and errors](#exit-codes-and-errors)
- [Schemas](#schemas)
- [Versioning](#versioning)

## Conventions

### Output

- **On success (exit 0), a command MUST write exactly one JSON document to stdout**, followed by one
  line feed, and nothing to stderr. `--help` is the one exception: it writes text (below).
- **On failure (any other exit code), a command MUST write nothing to stdout, and exactly one error
  document to stderr**, followed by one line feed ([Exit codes and errors](#exit-codes-and-errors)).
- **Output JSON MUST be compact:** no whitespace outside strings, UTF-8 with no byte order mark.
  Members appear in the order this page lists them, but a caller MUST NOT depend on the order.
- **A field that does not apply MUST be left out, never written as `null`**, an empty string or an
  empty object. Which fields apply in which state is in [Fields](#fields).
- **Every output document MUST validate against its schema** (D20). An implementation's own checks
  MUST validate what it prints, not only what it means to print.

### Times

- **Every time in output MUST be UTC, to the whole second, written `YYYY-MM-DDTHH:MM:SSZ`**, for
  example `2026-11-17T18:04:05Z`. No other form is emitted: no fraction, no offset, no lowercase
  `t` or `z`.
- **"Now" is the clock of the machine that answers the command.** In v0 that is the caller's
  machine.

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

### Command lines

- **Commands, subcommands and flag names are matched without regard to case**: `agentspace Lease
  CREATE --Project x` is the same as `agentspace lease create --project x`. Help and output always
  spell them in lowercase.
- **A flag's value MAY follow it as the next argument (`--ttl 8h`) or after an equals sign
  (`--ttl=8h`).** A value MAY contain spaces when the shell passes it as one argument (`--ttl "1h
  30m"`).
- **A flag that takes a value and is given without one is a usage error.** So is an empty value
  (`--ttl=`).
- **A flag given twice is a usage error**, because the two values could disagree. `list --state` is
  the one flag that MAY be repeated.
- **An unknown command, subcommand or flag, or an extra positional argument, is a usage error.**
- **`-h` and `--help` print the help of the level they appear at and exit 0, wherever they appear
  on the line.** Nothing else on the line is checked or done; `agentspace lease create --help
  --ttl 99d` prints help and creates nothing.
- **`agentspace` with no command, and `agentspace lease` with no subcommand, are usage errors**
  (exit 2), so that a script never mistakes help text for JSON.

### Concurrency and atomicity

- **Commands MUST be safe to run at the same time from several processes.** Each command's effect
  on a lease MUST be atomic: another command sees the lease either before it or after it, never
  half changed.
- **A command that fails MUST leave the lease as it was before the command.** A failed `create`
  MUST leave no lease record and no credential behind.

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
| `ttl_seconds` | Integer, 60 to 86400 | Always | The TTL of the current credential, or of the one asked for if none was issued. |
| `created_at` | String, [time](#times) | Always | When `create` recorded the lease. |
| `expires_at` | String, [time](#times) | `ready` and `expired`; `released` if it was ever ready | When the current credential stops being accepted for new connections. |
| `released_at` | String, [time](#times) | `released` only | When `release` released the lease. |
| `ssh` | Object | `ready` only | How to connect; see below. |
| `ssh.host_alias` | String | Inside `ssh` | The name to give `ssh`: `ssh <host_alias>`. |
| `ssh.user` | String, Unix user name | Inside `ssh` | The user the alias logs in as. |
| `reasons` | Array of objects, at least one | `unsatisfiable` only | Why the request cannot be met; see below. |
| `reasons[].code` | String | Inside each reason | One of the reason codes below. |
| `reasons[].message` | String | Inside each reason | One sentence for a person, naming the value that cannot be met. |

**`ssh.host_alias` is `agentspace-<project>-<first 8 hex digits of id>`**, for example
`agentspace-swimcoach-3f2a9c1e`. A caller MUST treat it as an opaque name to pass to `ssh` or to any
client that reads the OpenSSH client configuration, and MUST NOT build it from the ID itself.

**`ssh.user` is the user the lease logs in as.** In v0 every project's leases log in as `ubuntu`
(D3). A caller MUST read it from the lease and MUST NOT assume it, because per-project users will
change it.

**Reason codes:**

| Code | Means |
|---|---|
| `unsupported_platform` | No backend can provide the platform asked for. |
| `unsupported_profile` | No backend can provide the profile asked for. |
| `unsupported_size` | No backend can provide the size asked for. |
| `no_capacity` | The request is supported, but no capacity can be found for it. |

**Every reason that applies MUST be listed**, in the order of the table, so a request with an
unsupported platform and an unsupported size gets two reasons. In v0 only `linux/arm64`,
`agent-linux` and size `m` are supported (D5); `no_capacity` is in the contract for later backends,
and v0 never reports it.

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
runs at or after its `expires_at`. Nothing has to run at that moment: `get`, `list`, `renew` and
`release` each decide it when they read the lease, so the same lease can be `ready` in one call and
`expired` in the next with nothing in between.

**What expiry and release do not do:**
- **A connection that is already open MUST NOT be closed by expiry or release.** The expiry is
  checked only when a connection is made, so both stop new connections only.
- **`release` does not revoke the credential.** It deletes the caller's copy, and a copy that
  leaked stays usable until `expires_at` (D10). That is why the TTL has a maximum (D24).

**How long a lease is kept:** a `released`, `expired` or `unsatisfiable` lease MUST stay readable
by `get` and `list` for at least 24 hours after it reached that state. After that it MAY be
forgotten, and `get` then answers `not_found`. In v0 no lease is forgotten.

**Where leases are kept:** in v0, on the caller's machine, so `get` and `list` see the leases
created there and no others.

### The credential

- **`create` makes a key pair for the lease on the caller's machine, readable by the calling user
  alone, and has it certified for `ttl_seconds`.** The private key MUST NOT appear in any output
  and MUST NOT leave the caller's machine.
- **The alias is written into the calling user's OpenSSH client configuration**, so that `ssh
  <host_alias>` works with no other options. Nothing else in that configuration is changed.
- **`renew` replaces only the certificate.** The key, the alias and the user stay the same, so a
  client that saved the alias keeps working.
- **`release` deletes the key, the certificate and the alias.** It leaves the rest of the OpenSSH
  client configuration alone.

## Values

**Each value below is accepted in the forms listed and normalized to one form before use.** A
value outside its accepted forms is refused with `invalid_value` (exit 2), and a well-formed value
outside its allowed range with `out_of_range` (exit 2).

### Lease ID

- **Accepted:** a UUIDv4 in any case, with surrounding whitespace:
  `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`, where each `x` is a hex digit and `y` is one of `8`, `9`,
  `a` or `b`.
- **Normalized to:** lowercase, whitespace trimmed.
- **Refused:** any other UUID version, braces, a `urn:uuid:` prefix, and anything else. A
  well-formed ID that names no lease is `not_found` (exit 3), not `invalid_value`.

### Project

- **Accepted:** 1 to 32 characters of letters, digits and `-`, starting and ending with a letter or
  digit, in any case, with surrounding whitespace.
- **Normalized to:** lowercase, whitespace trimmed, matching `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`.
- **No default; `create` requires it.**

### Platform

**An `os/arch` pair in the OCI form, with an optional variant (D4).**
- **Accepted:** `os/arch` or `os/arch/variant`, each part letters and digits, in any case, with
  surrounding whitespace. The architecture MAY also be spelled `aarch64` (for `arm64`), or `x86_64`
  or `x86-64` (for `amd64`).
- **Normalized to:** lowercase, whitespace trimmed, `aarch64` written `arm64`, `x86_64` and
  `x86-64` written `amd64`, and the variant `v8` of `arm64` dropped, so `linux/aarch64/v8` becomes
  `linux/arm64`.
- **Default:** `linux/arm64` (D13).
- **Supported in v0:** `linux/arm64` only. Any other well-formed platform is accepted, and the
  lease is `unsatisfiable` with `unsupported_platform` (D5).

### Profile

**A named software contract, optionally pinned to a version (D4): `name` or `name@version`.**
- **Accepted:** a name of 1 to 64 letters, digits and `-`, starting and ending with a letter or
  digit, in any case, optionally followed by `@` and a version number of 1 or more, with surrounding
  whitespace. Leading zeros in the version are accepted.
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
- **Accepted, short form:** one or more parts, each a whole number and a unit, largest unit first,
  each unit at most once, with optional whitespace between and around them. Examples: `8h`, `90m`,
  `1h30m`, `1h 30m`, `2 hours`, `45 min`, `1d`.

  | Unit | Spellings, in any case unless noted |
  |---|---|
  | Days | `d`, `day`, `days` |
  | Hours | `h`, `hr`, `hrs`, `hour`, `hours` |
  | Minutes | `m` in lowercase only; `min`, `mins`, `minute`, `minutes` in any case |
  | Seconds | `s`, `sec`, `secs`, `second`, `seconds` |

- **Accepted, ISO 8601 form:** `P[nD][T[nH][nM][nS]]` in any case, with at least one part, and with
  at least one part after a `T` if there is a `T`. Examples: `PT8H`, `pt90m`, `P1D`, `P1DT2H`.
- **Normalized to:** a whole number of seconds, which output carries as `ttl_seconds`.
- **Refused as `invalid_value`, because each is ambiguous or not a duration:**
  - A bare number, such as `8` or `28800`, whose unit is unknown.
  - An uppercase `M` in the short form, which could mean months or minutes.
  - Weeks, months and years, in either form.
  - A fraction, such as `1.5h`, which would have to be rounded; `1h30m` says it exactly.
  - A sign, a negative number, a unit given twice, units out of order, or anything else.
- **Refused as `out_of_range`:** a well-formed TTL below 60 seconds or above 86400 seconds,
  including zero. The limits are checked after normalization, so `1440m` and `P1D` are both
  allowed and `24h1s` is not.

### State filter

**Used by `list --state`.**
- **Accepted:** one of the five [state names](#states), in any case, with surrounding whitespace.
  On the command line, `--state` MAY be repeated and each value MAY be a comma-separated list, as in
  `--state ready,pending`. In a JSON request, `state` is one state name or an array of them.
- **Normalized to:** the set of lowercase state names, with duplicates dropped.

## Commands

**The commands, from the top level down.** Each section's help block is exactly what `--help`
prints at that level, to stdout with exit 0. An implementation MUST print it byte for byte, with a
line feed after every line, and a change to either MUST change both.

### agentspace

```
agentspace [--help]
agentspace <command> ...
```

**The top level.** In v0 it has one command, `lease`.

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
  renew    Issue a ready lease a new credential with a new expiry
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
- **`ready`:** `ssh.host_alias` is in the caller's OpenSSH configuration, and `expires_at` is now
  plus the TTL.
- **`pending`:** call `get` until it is `ready` or `unsatisfiable`.
- **`unsatisfiable`:** `reasons` says which values cannot be met. Nothing was issued, and the lease
  needs no `release`.

**An unsatisfiable lease is a successful `create` (exit 0)**, because the lease was recorded and
`get` returns it. A caller that only checks the exit code will miss it, which is why the state MUST
be checked.

**A platform, profile or size that cannot be met is never swapped for another (D5).** If any one of
them cannot be met, the lease is `unsatisfiable`, whatever the others are.

| Flag | Type | Default | Notes |
|---|---|---|---|
| `--project <name>` | [Project](#project) | None; required | The project the agent works on. |
| `--platform <os/arch>` | [Platform](#platform) | `linux/arm64` | Only `linux/arm64` is supported in v0. |
| `--profile <name>` | [Profile](#profile) | `agent-linux` | Only `agent-linux` is supported in v0. |
| `--size <size>` | [Size](#size) | `m` | Only `m` is supported in v0. |
| `--ttl <duration>` | [TTL](#ttl) | `8h` | From 1 minute to 24 hours. |
| `-h`, `--help` | | | Print the help below and exit 0. |

**Request document:** [`lease-create.input.schema.json`](../schemas/lease-create.input.schema.json),
with members `project`, `platform`, `profile`, `size` and `ttl`.

**Output:** the lease document. **Exit codes:** 0, 1 and 2.

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

**Request document:** [`lease-get.input.schema.json`](../schemas/lease-get.input.schema.json), with
the member `id`.

**Output:** the lease document. **Exit codes:** 0, 1, 2 and 3.

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
- **`expires_at` becomes now plus the TTL**, whether that is later or earlier than before. The
  previous certificate is not revoked, and stays valid until its own expiry (D10).
- **`ttl_seconds` becomes the TTL used.** Without `--ttl`, the lease's current `ttl_seconds` is
  used again.
- **The key, the alias, the user and the ID do not change**, so clients that saved the alias need
  nothing new.
- **Renewing is idempotent:** renewing twice leaves the same lease, with the second call's expiry.
- **There is no limit on how many times a lease is renewed.** Each certificate is limited by the
  maximum TTL, and that is what bounds a leaked one (D24).

**Only a `ready` lease can be renewed.** Any other state is `invalid_state` (exit 4) and changes
nothing:
- **`expired`:** an expired lease stays expired. Renew before `expires_at`, or `create` a new
  lease. A lease's compute MAY already be gone once it expires, so no backend could promise to
  bring it back.
- **`released` and `unsatisfiable`:** both are terminal.
- **`pending`:** it has no credential yet. Wait for `ready`.

| Argument or flag | Type | Default | Notes |
|---|---|---|---|
| `<id>` | [Lease ID](#lease-id) | None; required | |
| `--ttl <duration>` | [TTL](#ttl) | The lease's `ttl_seconds` | From 1 minute to 24 hours. |
| `-h`, `--help` | | | Print the help below and exit 0. |

**Request document:** [`lease-renew.input.schema.json`](../schemas/lease-renew.input.schema.json),
with members `id` and `ttl`.

**Output:** the lease document. **Exit codes:** 0, 1, 2, 3 and 4.

**Help:**

```
Usage: agentspace lease renew <id> [--ttl <duration>]

Issues a ready lease a new certificate, valid from now for the TTL, and prints the lease.
The key, host alias and user stay the same. The old certificate is not revoked.
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
| `ready` | Deletes the key, certificate and alias, and sets `released_at` | `released` |
| `expired` | The same | `released` |
| `pending` | Gives up the request, and sets `released_at` | `released` |
| `released` | Nothing; prints the lease unchanged, with its first `released_at` | `released` |
| `unsatisfiable` | Nothing; prints the lease unchanged | `unsatisfiable` |

- **Release is idempotent:** releasing a lease again succeeds and changes nothing, so a caller MAY
  retry it freely.
- **Release does not revoke the certificate (D10).** A copy that leaked works until `expires_at`,
  which a released lease still shows when it had one. Connections already open are not closed.
- **An ID that names no lease is `not_found` (exit 3)**, not a success: there is no lease document
  to print, and success would hide a mistyped ID.

| Argument or flag | Type | Notes |
|---|---|---|
| `<id>` | [Lease ID](#lease-id) | Required. |
| `-h`, `--help` | | Print the help below and exit 0. |

**Request document:**
[`lease-release.input.schema.json`](../schemas/lease-release.input.schema.json), with the member
`id`.

**Output:** the lease document. **Exit codes:** 0, 1, 2 and 3.

**Help:**

```
Usage: agentspace lease release <id>

Gives a lease back, and prints it as JSON.
Deletes the lease's local key, certificate and host alias.
The certificate is not revoked: it stays valid until the lease's expiry.
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
- **Order:** by `created_at`, then by `id`, both ascending.
- **No filter means every lease that is kept** (see "How long a lease is kept" under
  [States](#states)).
- **No match is not an error:** the output is `{"leases":[]}`, exit 0.
- **States are decided as `get` decides them**, so `--state ready` leaves out a lease whose
  `expires_at` has passed, and `--state expired` includes it.

| Flag | Type | Default | Notes |
|---|---|---|---|
| `--project <name>` | [Project](#project) | Every project | Matches the normalized name exactly. |
| `--state <state>` | [State filter](#state-filter) | Every state | MAY be repeated, and MAY be a comma-separated list. |
| `-h`, `--help` | | | Print the help below and exit 0. |

**Request document:** [`lease-list.input.schema.json`](../schemas/lease-list.input.schema.json),
with members `project` and `state`.

**Output:** a list document, [`lease-list.output.schema.json`](../schemas/lease-list.output.schema.json):

| Field | Type | Present | Meaning |
|---|---|---|---|
| `leases` | Array of lease documents | Always | The matching leases, in the order above. |

**Exit codes:** 0, 1 and 2.

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
| 0 | None | The command did what was asked. Stdout holds its document. |
| 1 | `internal` | Anything that is not the caller's fault, such as a credential that could not be issued or lease records that could not be read. |
| 2 | `usage` | An unknown or missing command, subcommand, flag or argument, a flag given twice, or a flag with no value. |
| 2 | `invalid_value` | A value outside its [accepted forms](#values). |
| 2 | `out_of_range` | A well-formed value outside its allowed range; in v0, only the TTL. |
| 3 | `not_found` | No lease has the ID given. |
| 4 | `invalid_state` | The lease's state does not allow the subcommand; in v0, only `renew`. |

**No other exit code MAY be used.** Each non-zero exit code MUST come with one of the error codes
beside it, and every failure that fits none of the others is `internal`.

**The error document** goes to stderr, compact, on one line. Its schema is
[`error.output.schema.json`](../schemas/error.output.schema.json).

| Field | Type | Present | Meaning |
|---|---|---|---|
| `code` | String | Always | One of the error codes above. |
| `message` | String | Always | One or more sentences for a person: what was wrong, and what to do. |
| `field` | String | `invalid_value` and `out_of_range`; MAY be given for `usage` | The flag or request member at fault, without dashes: `id`, `project`, `platform`, `profile`, `size`, `ttl` or `state`. |
| `lease_id` | String, lowercase UUIDv4 | `not_found` and `invalid_state` | The normalized ID asked for. |
| `state` | String, [state](#states) | `invalid_state` | The state the lease is in. |

**A message MUST NOT echo a secret, and MUST NOT name a backend, a location or a machine (D1).** A
message SHOULD quote the value that was refused, and SHOULD say how to fix it, as in `TTL '8' has
no unit. Give one, as in '8h'.`

## Schemas

**Every JSON document in this contract has a schema under [`schemas/`](../schemas), in JSON Schema
draft 2020-12, one file per document (D20).** The names follow `<document>.<direction>.schema.json`:

| Schema | Document | Used by |
|---|---|---|
| `lease.output.schema.json` | One lease | Output of `create`, `get`, `renew`, `release` |
| `lease-list.output.schema.json` | A list of leases | Output of `list` |
| `error.output.schema.json` | One error | Stderr of every command that fails |
| `lease-create.input.schema.json` | A `create` request | Input of `create` |
| `lease-get.input.schema.json` | A `get` request | Input of `get` |
| `lease-renew.input.schema.json` | A `renew` request | Input of `renew` |
| `lease-release.input.schema.json` | A `release` request | Input of `release` |
| `lease-list.input.schema.json` | A `list` request | Input of `list` |

- **Output schemas are strict:** `additionalProperties` is false everywhere, every field that is
  always present is required, each state requires and forbids its own fields, and IDs and times
  have patterns. `format` is given as well, but only as an annotation, since validators need not
  enforce it.
- **Input schemas are tolerant in form and strict in meaning.** They accept the spellings in
  [Values](#values), allow `null` for an optional member to mean "not given", and refuse unknown
  members, because an unknown member is most likely a misspelled one whose value would otherwise
  be dropped. A range (the TTL limits) cannot be checked by a pattern, so passing the schema does
  not make a TTL acceptable.
- **An input document is what a request carries.** On the command line its members are the flags
  and arguments of the same names, and a hosted API (card #0011) MUST accept the same documents as
  JSON. In JSON, insignificant whitespace is allowed anywhere.
- **Each schema is self-contained**, with shared parts copied into its own `$defs`, because
  PowerShell's `Test-Json -SchemaFile` does not resolve a `$ref` to another file. The lease
  definition in the list schema's `$defs` MUST stay identical to the lease schema, which
  `scripts/test-schemas.ps1` checks.
- **Each schema has samples in [`schemas/samples/`](../schemas/samples)**, named
  `<schema name>.<case>.json`, which MUST validate. Documents that MUST NOT validate are in
  `schemas/samples/invalid/` under the same naming, at least one per output schema.
  `scripts/test-schemas.ps1` checks both and exits non-zero on any failure.
- **A schema's `$id` names it and is not a download address.** Each `$id` carries the contract
  version, `v0`.

## Versioning

**This is contract v0, the MVP's.**
- **Any change after which an output could fail to validate against its v0 schema is a new
  contract version**, and so is a change to an output's meaning. Because output schemas forbid
  unknown fields, a new output field is a new version.
- **Accepting an input that v0 refused is not a new version.** Refusing one that v0 accepted is.
- **Supporting a platform, profile or size that v0 answered `unsatisfiable` is not a new version.**
  The fields have been in the contract from v0 so that it is not (D5).

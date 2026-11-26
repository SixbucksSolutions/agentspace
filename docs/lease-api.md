# agentspace lease API, contract v2

RFC 2119 keywords; the capitals are load-bearing.

**The reference for `agentspace lease`, written so that it can be built and used without reading any
of agentspace's code (decision D20).** It names every command, flag, output field, state, error and
exit code, and gives the `--help` text at every level word for word. The JSON schemas under
[`schemas/`](../schemas) are part of this contract, except those under `schemas/v0/` and
`schemas/v1/`, which are contract v0's and contract v1's ([Schemas](#schemas)). Where this page and
a schema disagree, both are wrong and MUST be fixed together. What contract v2 changes from v1 is
in [Changes from v1](#changes-from-v1), and what v1 changed from v0 in
[Changes from v0](#changes-from-v0).

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

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository.** Every note
on this page that names the prototype, `scripts/lease-cli.ps1` and `bin/agentspace.cmd` included,
is history: it says where a requirement came from. None tells a reader to run the prototype, and
none binds a program that is built now. What binds is the rule each note stands beside. The
tables of [Hardening carried in](#hardening-carried-in) and the questions under
[Open questions](#open-questions) that compare the rewrite with the prototype stay as the record
of the change.

## Contents

- [Changes from v1](#changes-from-v1)
- [Changes from v0](#changes-from-v0)
- [Conventions](#conventions): [the program](#the-program), [output](#output),
  [times](#times), [input tolerance](#input-tolerance), [command lines](#command-lines),
  [concurrency and atomicity](#concurrency-and-atomicity), [the board read](#the-board-read)
- [The lease](#the-lease): [fields](#fields), [states](#states),
  [a host's life follows its leases](#a-hosts-life-follows-its-leases),
  [the credential](#the-credential)
- [Values](#values): [lease ID](#lease-id), [project](#project), [platform](#platform),
  [profile](#profile), [plan](#plan) and [the plan block](#the-plan-block), [board](#board),
  [epic](#epic), [turn](#turn), [size](#size), [TTL](#ttl), [state filter](#state-filter)
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

## Changes from v1

**Contract v2 adds to contract v1, and changes no document or error a caller sees while it gives
none of `--board`, `--epic` and `--turn`, and no plan with a `budget_minutes` member** (D87). No
lease output changes at all: every lease and list document v2 prints validates against v1's
schemas. `create`'s
help text changes too, which is text, not a document ([Versioning](#versioning)). It comes with
agentspace's v0.11 release (#0052). The contract's version and agentspace's release are two
numbers, kept apart (D26). Why each change makes a new version, and what v2 keeps of v1 and of v0,
are in [Versioning](#versioning).

| Change | Where | Could fail a v1 schema |
|---|---|---|
| `create --board <id> --epic <ticket>`: the lease's plan is read from the epic's `agentspace-plan` block on the caller's localswim board, and the lease is made as a `--plan` lease with that plan would be (D83, D86) | [`create`](#agentspace-lease-create), [Board](#board), [Epic](#epic), [The board read](#the-board-read) | No: an input v1 refused |
| The plan block: one fenced `agentspace-plan` block in an epic's description, holding exactly a plan document (D83) | [The plan block](#the-plan-block) | No: a new input |
| `create --turn <name>`: the lease is for that turn of its plan, sized for it alone (D84) | [`create`](#agentspace-lease-create), [Turn](#turn) | No: an input v1 refused |
| `budget_minutes`, a new optional member of a plan's turn, and the TTL a plan gives from it: the budget and half again at today's factor, unless `--ttl` is given (D85) | [Plan](#plan), [TTL](#ttl) | No: a plan v1 refused |
| `sized_for.turn` names the turn the lease is for, which is `--turn`'s when it is given, and otherwise the busiest, as in v1 (D84) | [Fields](#fields) | No: the same member and pattern, with a meaning that differs only for a lease made with `--turn` |
| `board`, `epic` and `turn`, new members of the `create` request document (D87; architect, 2026-11-25, for v0.11-T01, pin P6) | [`create`](#agentspace-lease-create) | No: an input v1 refused |
| `budget_minutes` in `plan.input.schema.json`, and in its copy in `lease-create.input.schema.json` (D85) | [Plan](#plan), [Schemas](#schemas) | No: an input v1 refused |
| `create`'s help: its usage line, three new rows, the `--ttl` row, and two of its last three lines (D84 to D87) | [`create`](#agentspace-lease-create) | No: help is text, not a document |
| `board`, `epic` and `turn`, new values of the error document's `field`, of `invalid_value` and of `usage` (D87) | [Exit codes and errors](#exit-codes-and-errors) | Yes: v1's `field` is an enum of eight |
| Seven new cases in which `usage` carries `field`, thirteen in all (D87; architect, 2026-11-25, for v0.11-T01, pin P4) | [Command lines](#command-lines), [Exit codes and errors](#exit-codes-and-errors) | Yes, through the new `field` values; which cases carry `field` is checked by conformance, since no schema can (D87) |
| A second message for `plan_too_large`, for a lease made with `--turn` (D84) | [Fields](#fields) | No: a message is text, and no caller parses one |
| `create --board` reads localswim's service receipt under `LOCALAPPDATA` (D86; architect, 2026-11-25, for v0.11-T01, pin P16) | [Environment](#environment), [The board read](#the-board-read) | No: no document carries it |
| A board lease's record keeps where its plan came from, in no output (D86, D87; architect, 2026-11-25, for v0.11-T01, pin P7) | [What is recorded](#what-is-recorded) | No: no output prints it |
| Every schema's `$id` carries `v2`, and every schema's title says contract v2 (D87, D89) | [Schemas](#schemas) | No: no document carries an `$id` |
| v1's schemas and samples are kept unchanged, under `schemas/v1/` and `schemas/v1/samples/` (D87, D89) | [Schemas](#schemas) | No |

- **Nothing else changes.** No command, state, exit code, error code or reason code is added or
  removed, no output member is added, and no member of v1's documents changes its meaning for a
  lease made without `--turn`.
- **agentspace's TTL factor is not part of the contract** (D85), as its per-agent footprint is not
  (D76). Today's figure is given in [TTL](#ttl), for information.

## Changes from v0

**Contract v1 added to contract v0, and changed no document or error a caller sees while it made no
lease with `--plan` and read none** (D76). `create`'s help text changed too, which is text, not a
document ([Versioning](#versioning)). It came with agentspace's v0.10 release (#0050). The
contract's version and agentspace's release are two numbers, kept apart (D26). Why each change made
a new version, and what v1 kept of v0, are in [Versioning](#versioning). The table stands as v1's
record, and v2's changes to the same places are in [Changes from v1](#changes-from-v1).

| Change | Where | Could fail a v0 schema |
|---|---|---|
| `create --plan <file>`: the lease gets the smallest size that holds the plan's busiest turn (D74, D75) | [`create`](#agentspace-lease-create), [Plan](#plan) | No: an input v0 refused |
| The plan document, and its schema, `plan.input.schema.json` (D74, D76) | [Plan](#plan), [Schemas](#schemas) | No: a new input |
| `plan`, a new member of the `create` request document, holding the plan document itself (D76; architect, 2026-11-24, for v0.10-T01, pins P8 and P19) | [`create`](#agentspace-lease-create) | No: an input v0 refused |
| `create`'s help: its usage line, the `--plan` row, and its last two lines (D75, D76) | [`create`](#agentspace-lease-create) | No: help is text, not a document |
| `sized_for`, a new member of every lease made with `--plan`, in every state (D75) | [Fields](#fields) | Yes |
| `plan_too_large`, a new reason code, for a plan that no size holds. It is always the lease's only reason, and the lease's `size` is `xl` (D75, D81) | [Fields](#fields), [`create`](#agentspace-lease-create) | Yes |
| `plan`, a new `field` of `invalid_value` and of `usage`, and a sixth case in which `usage` carries `field`: `--plan` with `--size` (D75, D81; architect, 2026-11-24, for v0.10-T01, pins P6 and P13) | [Exit codes and errors](#exit-codes-and-errors) | Yes |
| Every schema's `$id` carries `v1`, and every schema's title says contract v1 (D76; architect, 2026-11-24, for v0.10-T01, pin P9) | [Schemas](#schemas) | No: no document carries an `$id` |
| v0's schemas and samples are kept unchanged, under `schemas/v0/` and `schemas/v0/samples/` (D76; architect, 2026-11-24, for v0.10-T01, pin P9) | [Schemas](#schemas) | No |

- **No lease lacks `size`.** A `plan_too_large` lease's is `xl`, the largest size (D81), so `size`
  is in every lease, as in v0.
- **Nothing else changed in v1.** No command, state, exit code or error code was added or removed,
  and no member of v0's documents changed its meaning for a lease made without `--plan`.
- **agentspace's per-agent footprint is not part of the contract** (D76). Its figure is given in
  [Plan](#plan), for information.

## Conventions

### The program

- **The CLI is one program, run as `agentspace`.** How it is installed and put on the caller's
  `PATH` is outside this contract, and so is its file name. agentspace's own install calls it
  `agentspace.exe`, beside the other programs it needs
  ([operator.md, Programs and install layout](operator.md#programs-and-install-layout)).
  - Note on where this came from: the prototype is `scripts/lease-cli.ps1`, started by
    `bin/agentspace.cmd` from cmd and PowerShell, and by `bin/agentspace` from Git Bash and other
    POSIX shells.
- **Contract v2's caller runs on Windows (D8)**, as v0's and v1's did, and runs `agentspace` from cmd,
  PowerShell or Git Bash. A caller on Linux or macOS is later, and not blocking: it comes with the
  hosted control plane (#0011).
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
  with no terminal. The one file a command reads that the caller names is the plan of
  `create --plan` ([Plan](#plan)), and `--plan -` names a file called `-`, never standard input.
- **`create --board <id> --epic <ticket>` also reads files the caller does not name, and makes the
  CLI's only network requests** (D86): localswim's service receipt and its Bot credential, then
  one or two reads of the caller's own localswim board, all on the caller's machine, as
  [The board read](#the-board-read) says. No other command reads them or makes a request.

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
    tell the two apart in safe code. Contract v2's caller is Windows (D8), and a full or broken
    stdout still fails there.
  - **A change the command already recorded stays recorded.** A `create`, `renew` or `release`
    whose record was written before its document failed to reach stdout has made its change, and
    is not undone, which is an exception to
    [Concurrency and atomicity](#concurrency-and-atomicity)'s rule for a failed command, as the
    connections a failed `release` ended are. Its message MUST say that the change was made and
    that `get` or `list` shows it, as in `agentspace lease create failed: The lease was
    recorded, but its document could not be written. Run 'agentspace lease list' to see it.`
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
- **"Now" is the clock of the machine that answers the command.** In v2 that is the caller's
  machine.
- **A lease's certificate is issued at a whole second, its issue time, during the command that
  issues it** (`create` or `renew`), **or by agentspace when it places a lease that `create`
  answered `pending`** (from agentspace's v0.07 release, #0047; [States](#states)). `expires_at`
  MUST equal the issue time plus the TTL exactly, which is also the certificate's valid-before time
  ([The credential](#the-credential)), however the certificate was issued.
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
  ([Schemas](#schemas)). **The one exception is a plan's leading UTF-8 byte order mark**, a plan
  file's or, from v2, an `agentspace-plan` block's, which is the plan's bytes and in no value: it
  is accepted and ignored, and U+FEFF anywhere else in the plan is refused ([Plan](#plan),
  [The plan block](#the-plan-block); architect, 2026-11-24, for v0.10-T01, pins P4 and P18).
- **A turn name is never trimmed** ([Turn](#turn)), as a plan's path is not ([Plan](#plan)): it is
  compared exactly, so a value with whitespace at either end is outside its form and refused.
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
- **`--plan` with `--size`**, to `create`, in either order, because the plan picks the size (D75).
  It is reported where the later of the two flags stands, and a later one with no value or an
  empty value is reported as that instead. Its `field` is `plan` (architect, 2026-11-24, for
  v0.10-T01, pins P6 and P13).
- **`--board` or `--epic` with `--plan` or `--size`**, to `create`, in either order, because the
  epic carries the plan and the plan picks the size (D86). Each pair is reported where its later
  flag stands, and a later one with no value or an empty value is reported as that instead.
  **Its `field` names the board-side flag, whichever order the pair is in: `board` when `--board`
  is on the line, and otherwise `epic`**, as `--plan` with `--size` always names `plan`. Where the
  pair stands fixes only where the error ranks among the line's usage errors, never which `field`
  it carries (architect, 2026-11-25, for v0.11-T01, pin P4).
- **An unexpected positional argument:** a second ID, or any positional argument to `create` or
  `list`.
- **`agentspace` with no command, and `agentspace lease` with no subcommand**, so that a script
  never mistakes help text for JSON.
- **A missing lease ID** for `get`, `renew` or `release`, and a missing `--project` for `create`.
- **From v2, a missing partner, to `create`** (D84, D86; architect, 2026-11-25, for v0.11-T01, pin
  P4): `--board` without `--epic` and `--epic` without `--board`, because one names the board and
  the other the card on it, and `--turn` without `--plan` or `--epic`, because there is no plan to
  pick a turn from. For a missing partner of a pair, its `field` is the flag that is missing, as a
  missing `--project` names `project`: `epic` for `--board` without `--epic`, and `board` for
  `--epic` without `--board`. For `--turn` without a plan, which has two sources, it is `turn`.

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
   a flag with no value or an empty value, a flag given twice, a pair of flags that cannot be given
   together, or an unexpected argument. The pairs are `--plan` with `--size`, and from v2 `--board`
   or `--epic` with `--plan` or `--size`; each is placed in line order where its later flag stands,
   as the list below says.
4. **Otherwise, a missing command, subcommand, lease ID or `--project` is reported, and then, for
   `create` from v2, a missing partner of `--board`, `--epic` or `--turn`**, in the order of the
   list below.
5. **Only when the line has no usage error are the values checked**, in the order of
   [Exit codes and errors](#exit-codes-and-errors).

**Which of `create`'s pairs and partners is reported** (architect, 2026-11-25, for v0.11-T01, pin
P4). These are the order that steps 3 and 4 give them:
1. **A pair of flags that cannot be given together is an error at the position of its later
   flag**, and takes its place in step 3's line order there, among the line's other usage errors.
   The pairs are `--plan` with `--size`, `--board` with `--plan`, `--board` with `--size`, `--epic`
   with `--plan`, and `--epic` with `--size`.
2. **When the later flag of a pair has no value or an empty value, that is the error reported at
   its position**, not the pair, as for `--plan` with `--size` in v1.
3. **When two pairs end at the same flag, the pair whose earlier flag stands first on the line is
   reported** (architect, 2026-11-25, for v0.11-T01, pin P20). So
   `create --project x --board B --epic 7 --plan p.json` reports `--board` with `--plan`.
4. **With three or more of `--board`, `--epic`, `--plan` and `--size` on a line, the first pair in
   line order is reported**, by rules 1 and 3. So `create --project x --plan p.json --board B
   --size m` reports `--board` with `--plan` at `--board`, before `--plan` with `--size` at
   `--size`.
5. **A pair's `field` is the board-side flag, never the one that stands later:** `board` whenever
   `--board` is on the line, `epic` when only `--epic` is, and `plan` for `--plan` with `--size`.
   So `create --project x --epic 7 --plan p.json --board B` reports `--epic` with `--plan`, at
   `--plan`, with `field` `board`.
6. **Only when step 3 finds nothing are the missing partners looked for**, in step 4, in this
   order: a missing command, subcommand, lease ID and `--project`; then `--board` without
   `--epic`; then `--epic` without `--board`; then `--turn` without `--plan` or `--epic`. So
   `create --project x --board B --turn T01` reports `--board` without `--epic`, field `epic`, and
   `create --board B` with no `--project` reports the missing `--project`.
7. **A pair's message names its two flags in a fixed order**, `--board` or `--epic` first and then
   `--plan` or `--size`, as `--plan` with `--size` is always named in that order, whichever stands
   first on the line ([Messages](#messages)).

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
- **`create` reads and parses the plan of `--plan` before it starts to wait**, as it checks every
  value before then, so a plan that is not well formed, or is large, never lengthens how long
  `create` holds the others back, and is refused with `invalid_value` without waiting for them
  (architect, 2026-11-24, for v0.10-T01, pin P28; [Plan](#plan)).
- **From v2, `create` reads the board of `--board` and `--epic` before it starts to wait too**, its
  requests' 30 seconds each included, so a slow or refused board read never holds the lease
  records, and a refused epic never touches them ([The board read](#the-board-read)).
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
  - **The connections a failed `release` already ended stay ended.** They cannot be restored, and
    are a change of a failed command that is not undone (architect, 2026-11-21, for v0.08-T01).

### The board read

**`create --board <id> --epic <ticket>` reads the epic's plan from the caller's localswim board,
and only reads (D86).** localswim is the caller's own board, run as the caller's own Windows user on
the caller's machine. This section is what the lease CLI does to reach it; how localswim answers is
localswim's (its `docs/openapi.yaml` and `docs/windows-operations.md`). Which plan the epic holds is
[The plan block](#the-plan-block)'s.

**When it happens:**
- **Only on a `create` with `--board` and `--epic`, and only once the command line has no usage
  error** ([Command lines](#command-lines)) **and every value on it has passed its form and range
  checks, `--size` and `--ttl` included; `--turn` is matched against the plan only after the board
  read** (architect, 2026-11-25, for v0.11-T01, pin P20). The full order is
  [Exit codes and errors](#exit-codes-and-errors)'s. A command line that fails any other check
  never reaches the board.
- **Before `create` waits for any other command and before it reads the lease records**, as a
  `--plan` file is read ([Concurrency and atomicity](#concurrency-and-atomicity)).
- **Once for each `create`.** Nothing read from a board is kept for a later command, except what
  the lease's record keeps ([What is recorded](#what-is-recorded)).

**Finding the board: localswim's service receipt** (D86, D89; localswim's
`docs/windows-operations.md`, "The receipt"):
- **The receipt is `<LOCALAPPDATA>\localswim-v2\service.json`**, where localswim documents it,
  `LOCALAPPDATA` being the caller's system's per-user application data folder
  ([Environment](#environment); architect, 2026-11-25, for v0.11-T01, pin P16). A relative
  `LOCALAPPDATA` is resolved against the working folder of the command. **No variable of
  agentspace's moves the receipt** (architect, 2026-11-25, for v0.11-T01, pin P9).
- **It is read afresh for each `create`, whole, and closed at once**, as localswim asks of a reader.
  A read that fails is refused at once, and is not tried again within the command.
- **Its rules are localswim's:** one JSON object in UTF-8, a leading byte order mark accepted,
  holding `base` and `credential_directory`, each a string and each once, and any other member
  ignored. A receipt that breaks them is refused.
- **The base is the receipt's `base`, and MUST be exactly `http://127.0.0.1:<port>`**, localswim's
  own receipt form, so the token never leaves the caller's machine (D89 as changed for v0.11-T01;
  architect, 2026-11-25, for v0.11-T01, pin P19). "Exactly" means, character by character:
  - **The scheme is `http`, in lowercase, then `://`.** `HTTP://`, `https://` and any other scheme
    are refused.
  - **The host is `127.0.0.1`**, with no user information (no `@`), no trailing dot, and no other
    spelling of the loopback address: `localhost`, `127.1`, `[::1]` and `0x7f.0.0.1` are refused.
  - **Then `:` and the port, 1 to 5 ASCII decimal digits with no leading zero, whose value is
    from 1 to 65535.** A base with no port is refused.
  - **Then nothing:** no `/`, no path, no `?` or query, no `#` or fragment, and no whitespace.
- **The connection is made to 127.0.0.1, at the base's port, directly, never through a proxy**,
  whatever the environment says: `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY`, in any
  case, change nothing ([Environment](#environment); architect, 2026-11-25, for v0.11-T01, pin
  P19).

**The credential: localswim's Bot token, read for the one read and dropped** (D86, Q17;
localswim's `docs/windows-operations.md`, "The credential folder" and "The credential files"):
- **The token is in `bot.json`, in the folder the receipt's `credential_directory` names**,
  protected with Windows' data protection for the caller's Windows user, and is decrypted for the
  read, as localswim documents and as its own readers do. The file's rules are localswim's: one
  JSON object of exactly the members `format`, `format_version`, `role` and `token_dpapi_hex`,
  `role` being `bot`, and a token of 43 characters of unpadded base64url once decrypted. A file
  that breaks them, or that cannot be decrypted, is refused.
- **The token MUST NOT be printed, logged, written to agentspace's state or to any file, put in an
  environment variable, put in a URL, or passed as a command argument** (D86; AGENTS.md, "The
  board"). It is sent in the `Authorization` header of the read's requests and nowhere else, and
  is dropped once the read ends, whatever its outcome.
- **No message carries the token, a part of it, or anything derived from it.**
- **agentspace keeps no credential of its own for a board, and adds no file** (D86, Q17).
- **On a system with no Windows data protection, the credential cannot be read**, and the read
  fails as an unreadable credential does (below). Contract v2's caller runs on Windows (D8).

**The requests** (D86, D89 as changed for v0.11-T01; architect, 2026-11-25, for v0.11-T01, pins P2
and P17; localswim's `docs/openapi.yaml`, operation `readBoard`):
- **One GET of localswim's focused ticket view, with the card's prose:**
  `<base>/api/2026-11-10/boards/<board>?view=ticket&ticket=<n>&include_prose=true`, where `<base>`
  is the receipt's base, `/api/2026-11-10` is the API's path as localswim's `docs/openapi.yaml`
  gives it under `servers`, `<board>` is the normalized [board](#board) ID, and `<n>` is the
  normalized [epic](#epic) ticket.
- **Its headers are `Authorization: Bearer <token>`** and those the HTTP client sends with every
  request (`Host`, and MAY be `User-Agent`, `Accept` and `Connection`). **It MUST NOT carry a body,
  a cookie, an `Origin`, an `If-None-Match`, or an `Accept-Encoding` that asks for anything but
  `identity`.**
- **After a 404 that is localswim's `not_found` (below), one more GET, of the board's summary
  view:** `<base>/api/2026-11-10/boards/<board>?view=summary`, with the same headers, to tell an
  unknown ticket from an unknown board, since localswim answers 404 for both alike. Its body is
  read only as far as a 404's is (below). **The card itself is read once** (D86), and a read
  refused this way costs at most two GETs.
- **A 404 counts as not found only when its body is localswim's Problem document**, a JSON object
  whose `code` is `not_found`, given with the media type `application/problem+json` (architect,
  2026-11-25, for v0.11-T01, pin P17). **The media type is the `Content-Type`'s type and subtype,
  compared without regard to case, with any parameters, such as `charset`, ignored** (architect,
  2026-11-25, for v0.11-T01, pin P26), so
  `Application/Problem+JSON; charset=utf-8` is that media type. Any other 404
  is the "did not answer" case below, so a path localswim later retires reads as a failure of
  agentspace's, never as an unknown board or ticket.
- **No other request is made**, to the board or anywhere else. These two are the only network
  requests the lease CLI makes.

**The client only reads** (D86):
- **Every request is a GET**, made through one function that takes no method and has no way to
  send another. No code path of the lease CLI can send POST, PUT, PATCH, DELETE, HEAD, OPTIONS or
  any other method to a board, whatever the token permits. Bot's token can write, so this is what
  keeps agentspace to reads (Q17).
- **Conformance MUST show it** (D86, D88): its stand-in board answers any method but GET with a
  failure that the case reports.

**Bounds** (architect, 2026-11-25, for v0.11-T01, pins P1 and P2):
- **A redirect is never followed.** A 3xx answer is the "did not answer" case below.
- **Each request has 30 seconds, from the start of its connection to the last byte of its answer.**
  A request that runs past them is ended, and is the "did not answer" case below.
- **A 200 answer's body is read to at most 67108864 bytes (64 MiB), counted as received.** A body
  that a `Content-Length` says is larger is refused before it is read, and one that grows past the
  bound as it is read is refused once the next byte arrives: a build MUST NOT read more than
  67108865 bytes of it. **The bound counts the body's bytes the lease CLI takes, not what its HTTP
  client buffers below that**: a transport's read-ahead of at most 128 KiB past the bound, never
  handed to the CLI, is not a read past it (architect, 2026-11-25, for v0.11-T02, S02's report). A description is at most 4194304 characters (localswim's `Card`), and the
  answer's comments come on top of it with no bound, so the bound is on the answer.
- **A 404's body is read under the same bound**, only to find whether it is localswim's
  `not_found`; one larger than the bound is not. **Any other answer's body is not read beyond what
  the HTTP client needs to close the connection.** No body is ever quoted.

**What is read from a 200 answer** (D86; architect, 2026-11-25, for v0.11-T01, pin P1): the body is
one JSON object, localswim's `TicketInspection`, of which the lease CLI reads three members and no
more:

| Member | Type | Use |
|---|---|---|
| `card.revision` | Integer, 1 to 9007199254740991, written as digits alone | The card's revision as read, kept in the lease's record ([What is recorded](#what-is-recorded)) |
| `card.archived` | Boolean | `true` is refused: an archived epic is not one being worked (D86) |
| `prose.description` | String | The description, from which [The plan block](#the-plan-block) takes the plan |

- **Every other member MUST be ignored, whatever it holds**: `prose.comments`, `relationships`,
  `parent`, `children` and `related_card` among them, and the card's title, lane, owner and
  priority. **The lease CLI MUST NOT print or record any of them** (architect, 2026-11-25, for v0.11-T01, pin P1), so a change to them in
  localswim never changes what the lease CLI does.
- **A body that is not one JSON object, or in which any of the three is missing or is not of its
  type, is refused** as an answer that could not be read, an `internal` error (below). Two such
  answers are named here, so that no build reads them otherwise (architect, 2026-11-25, for
  v0.11-T01, pin P26):
  - **`card.revision` MUST be a JSON integer written as digits alone, from 1 to
    9007199254740991, or the answer is refused.** `5.0`, `5e0`, `-5`, `0`, a string such as `"5"`
    and a number above the bound are each refused, though `5.0` and `5e0` are whole numbers.
  - **An answer that cannot be read as JSON of its type is refused whole**, a
    `prose.description` holding an unpaired surrogate, such as `"\ud800"`, included. The answer is
    read as JSON before the description is looked at, so such a description never reaches
    [The plan block](#the-plan-block)'s checks, and is never `invalid_value` for `epic`.

**Each outcome** (D86; architect, 2026-11-25, for v0.11-T01, pins P1, P2, P11 and P13). The first
row that applies is the outcome, and only one is reported:

| What happens | Error | `field` | Message |
|---|---|---|---|
| `LOCALAPPDATA` unset, empty or only whitespace | `internal` | None | `agentspace lease create failed: LOCALAPPDATA is unset or empty, so localswim's service receipt cannot be found. Nothing was recorded.` |
| No file at the receipt's path | `internal` | None | `agentspace lease create failed: localswim's service receipt was not found; localswim may not be running. Nothing was recorded.` |
| The receipt cannot be read, breaks localswim's rules, or has its `base` missing or not of the form above (architect, 2026-11-25, for v0.11-T01, pins P19 and P26) | `internal` | None | `agentspace lease create failed: localswim's service receipt could not be read. Nothing was recorded.` |
| `bot.json` missing, unreadable, breaking localswim's rules, or not decrypted to a token | `internal` | None | `agentspace lease create failed: The board's credential could not be read. Nothing was recorded.` |
| No connection, a connection refused, reset or closed before the whole answer arrived, or 30 seconds passed | `internal` | None | `agentspace lease create failed: The board at 127.0.0.1:<port> did not answer. Nothing was recorded.` |
| 200, with a body larger than 64 MiB | `invalid_value` | `epic` | `Epic '<value>' carries no usable plan: the board's answer for this card is larger than 64 MiB.` |
| 200, with a body that is not the shape above | `internal` | None | `agentspace lease create failed: The board's answer could not be read. Nothing was recorded.` |
| 200, with `card.archived` `true` | `invalid_value` | `epic` | `Epic '<value>' is archived. Name an epic that is being worked.` |
| 200, with a description [The plan block](#the-plan-block) refuses | `invalid_value` | `epic` | `Epic '<value>' carries no usable plan: <why>.`, `<why>` as that section gives it |
| 404 that is localswim's `not_found`, then 200 from the summary view | `invalid_value` | `epic` | `No card of ticket '<value>' on board '<board>'. Give the epic's ticket number, as in '52'.` |
| 404 that is localswim's `not_found`, then the same from the summary view | `invalid_value` | `board` | `No board of ID '<value>' that agentspace can read.` |
| 404 that is localswim's `not_found`, then any other outcome of the summary view | As that outcome's row, for the summary view | | |
| 401 or 403 | `internal` | None | `agentspace lease create failed: The board refused agentspace's credential. Nothing was recorded.` |
| Any other status: 1xx, a 2xx other than 200, 3xx, 400, a 404 that is not localswim's `not_found`, 405, 406, any other 4xx, and 5xx | `internal` | None | `agentspace lease create failed: The board at 127.0.0.1:<port> did not answer. Nothing was recorded.` |

- **`<value>` is the `--epic` value as given in an `epic` row, and the `--board` value as given in
  the `board` row**, quoted by [Messages](#messages)' rules. **`<board>` is the normalized board
  ID**, and **`<port>` is the base's port, as the receipt gives it**, the one part of a URL an
  `internal` message carries (D86).
- **Every message here is in agentspace's own words** (#0041): **none carries the token, a URL
  beyond the address and port, a path, the board's answer, an operating system's error text, or
  any of localswim's own text**, a Problem document's `title`, `detail` or `code` included. The
  message for a board that is not found names no credential or role (architect, 2026-11-25, for v0.11-T01, pin P13).
- **A 404 for the board covers every board localswim will not show the caller**: no board of that
  ID, a board Bot is not a member of, and a board that failed localswim's startup admission
  (localswim's `docs/openapi.yaml`, `readBoard`). A closed board's ticket view is read as any
  other's.
- **A credential refused is not tried again within the command.** localswim's credential changes
  when it starts afresh, and the next `create` reads the receipt and the credential again
  (localswim's "The credential folder").
- **Each `invalid_value` here is a value's error, and each `internal` here is reported among the
  values**, before the lease records are read ([Exit codes and errors](#exit-codes-and-errors),
  "When several errors apply", step 2). Every one records no lease, and creates and changes
  nothing.

#### What is recorded

**A lease whose plan came from a board keeps where it came from in its record** (D86; architect,
2026-11-25, for v0.11-T01, pin P7), as [docs/state.md](state.md) places it:
- **`plan_source`: the board, the ticket and the card's revision as read**: the normalized board
  ID, the normalized ticket as an integer, and `card.revision` as the answer gave it.
- **The plan as read**, the plan document the block held, kept as a `--plan` lease's plan is kept
  ([Plan](#plan)).
- **A lease made with `--plan` has no `plan_source`**, and one made without a plan has neither.

**No output prints any of them** (D87). The lease document of a board lease is the document the
same plan given with `--plan` would print, with the same members (D83):
- **`sized_for.plan` is the plan document's own `name`**, as written in the block, and is left out
  when the plan has none. It is never the board's ID, the board's name, the epic's ticket or the
  card's title (D75, D87).
- **The board, the ticket, the revision, the description and the plan are in no output**, an
  error's message included, except that a refusal's message quotes the `--board` and `--epic`
  values the caller gave, and the normalized board ID ([Messages](#messages)).

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
| `size` | String, [size](#size) | Always | The size asked for, normalized, or, for a lease created with a plan, the size picked for the plan ([Plan](#plan)), which is `xl` for a lease whose reason is `plan_too_large` (D75, D81). It stays that size even when larger compute serves the lease (D66). |
| `sized_for` | Object | Every lease created with a plan, from `--plan` or `--epic`, in every state | What the lease was sized for; see below. |
| `sized_for.plan` | String, [name](#plan) | Inside `sized_for`, when the plan's `name` is given and not `null` | The plan's `name`, as written in the plan. |
| `sized_for.turn` | String, [name](#plan) | Inside `sized_for` | The name of the turn the lease is for, as written in the plan: the one `--turn` named, or the plan's busiest turn (D84). |
| `sized_for.agents` | Integer, 2 to 1025 | Inside `sized_for` | That turn's agents at once, its dev lead included. |
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

**`expires_at` is the issue time plus the TTL, not `created_at` plus the TTL.** For a lease that
`create` answered `ready`, the certificate is issued during `create`, so the two differ by no more
than the time `create` took, and either MAY come first. For a lease that `create` answered
`pending`, it is issued when agentspace places the lease, which can be minutes after `created_at`
([States](#states)), so the issue time is later than `created_at` by up to that long; `expires_at`
is still the issue time plus the TTL exactly.

**`ssh.host_alias` is `agentspace-<project>-<first 8 hex digits of id>`**, for example
`agentspace-swimcoach-3f2a9c1e`. A caller MUST treat it as an opaque name to pass to `ssh` or to any
client that reads the OpenSSH client configuration, and MUST NOT build it from the ID itself.
**Two kept leases MUST NOT share a host alias:** `create` MUST choose an ID whose alias no kept
lease uses, in any state, and MUST draw a new ID, as many times as it takes, while the one drawn
has an alias that a kept lease uses. A build MAY bound the draws at no fewer than 100, and MUST
then fail with `internal` when none is free; a project would need billions of kept leases for that
to happen. Only kept leases count: an alias file that no record names makes `create` fail, and is
never a reason to draw again ([docs/state.md](state.md#the-alias-file)).

**`ssh.user` is the user the lease logs in as.** In v2 every project's leases log in as `ubuntu`
(D3), as in v0 and v1. A caller MUST read it from the lease and MUST NOT assume it, because later hosts MAY change
it. A caller MUST accept any user that the table's rule for `ssh.user` allows. In v2 a lease's user also matches the
narrower pattern of the lease state file, which is what binds a driver
([docs/state.md](state.md#the-lease-state-file)).

**`sized_for` says what a lease created with a plan was sized for (D75).** `create` sets it, and
nothing changes it afterwards: `get`, `renew`, `release` and `list` print it as `create` did, in
every state (architect, 2026-11-24, for v0.10-T01, pin P10).
- **`plan` is the plan's `name`, and is left out when the plan's `name` is absent or `null`**,
  never written as `null`. **`turn` is the turn the lease is for** (D84): the one `--turn` names,
  or, without `--turn`, the plan's busiest turn, the first such turn in the plan's order on a tie.
  **`agents` is that turn's agents at once**, the sum of its stories' peaks plus 1 for its dev
  lead ([Plan](#plan); D74). Both names are printed exactly as the plan writes them, and each
  matches the pattern of a [name](#plan). `sized_for` has no other member.
- **The lease's `size` is the size picked for those agents**, the smallest that holds them at
  agentspace's per-agent footprint ([Plan](#plan)). **When no size holds them, the lease is
  `unsatisfiable` with the one reason `plan_too_large`, and its `size` is `xl`**, the largest
  (D81): `sized_for` says which turn needed more, and nothing is issued at any size (D5).
- **A lease created without a plan has no `sized_for`**, since a field that does not apply is
  left out ([Output](#output)), and prints as it did in contract v0 (D76).
- **`sized_for.plan` is there so that a plan read from elsewhere changes no output** (D75), and
  from agentspace's v0.11 release (#0052) a plan read from an epic changes none: **a board lease
  prints the plan's own `name`, never the board's ID or the epic's ticket** (D87;
  [What is recorded](#what-is-recorded)).
- **The plan itself, each subagent's model and effort, each turn's `budget_minutes`, and the
  board, ticket and revision a plan was read from are in no output** (D74, D86, D87).

**Reason codes:**

| Code | Means |
|---|---|
| `unsupported_platform` | No backend can provide the platform asked for. |
| `unsupported_profile` | No backend can provide the profile asked for. |
| `unsupported_size` | No backend can provide the size asked for. |
| `plan_too_large` | The lease's turn of its plan, from `--plan` or `--epic`, has more agents at once than the largest size holds at agentspace's per-agent footprint ([Plan](#plan)). It is always the lease's only reason, and the lease's `size` is `xl` (D75, D81, D84). |
| `no_capacity` | The request is supported, but no capacity can be found for it: in v2, its project has no host and cannot have one made (D54), or its project's host is serving other leases at a smaller size (D66, D68). |

**Every reason that applies MUST be listed, each code once,** in the order of the table, so a
request with an unsupported platform and an unsupported size gets two reasons, **with one
exception: `plan_too_large` MUST be the lease's only reason** (D81; architect, 2026-11-24, for
v0.10-T01, pins P15 and P16).
- **It is decided by agentspace's lease CLI from the plan, before any driver is asked to place the
  lease (D75, D81).** Platform and profile are judged by a driver, as it places a lease, and asking
  one with a size the plan does not fit would place the lease. So a lease answered
  `plan_too_large` lists it alone, whatever its platform and profile: a too-large plan with
  `--platform windows/amd64` gives the one reason `plan_too_large`. It is never beside
  `unsupported_size`, which needs a size asked for, nor beside `no_capacity`, which is for a
  supported request.
- **Like every `unsatisfiable` answer, it needs no CA, and nothing is issued:** no compute is made,
  changed or started, and the lease needs no `release` (architect, 2026-11-24, for v0.10-T01,
  pin P26). [`create`](#agentspace-lease-create) says what `create` still checks for it.
- **A plan that a size holds is judged by the driver as a `--size` lease is**, and gets the
  reasons a `--size` lease of the size picked would get when its platform or profile cannot be met
  (architect, 2026-11-24, for v0.10-T01, pin P16). Its size is always one of the four sizes, so
  `unsupported_size` comes only from a driver that lacks that size.

In v2 only `linux/arm64` and `agent-linux` are supported, and the sizes `s`, `m`, `l` and `xl`
([Size](#size); D5, D65). Until agentspace's v0.09 release (#0049), size `m` alone was.
`no_capacity` is in the contract for backends that can run out. Until agentspace's v0.07 release
(#0047) it was never reported. agentspace's driver reports it for a supported request only, in
these cases:
- **From v0.07, for a project that has no host and is not on the list of projects that may have
  one made** (D54; [docs/driver.md](driver.md#where-it-places-a-lease)).
- **From v0.09 (#0049), for a project whose host is serving another of the project's `ready`
  leases, and is smaller than the size asked for** (D66). Changing the host's size would stop it
  under those leases, so the request is refused at once, rather than left to wait for them to end.
  The answer changes nothing: no lease of the project, and not the host's size or state. A host
  whose size agentspace has not yet recorded is never refused so: the request is `pending`
  instead, while its size is recorded (architect, 2026-11-23, for v0.09-T01, pin P2).
- **From v0.09, for a `pending` lease that its host, once its size is changed or recorded, does
  not meet**, because a larger lease arrived after the change was chosen, or because another of
  the project's leases held the host at its size, **while another of the project's leases is
  `ready` on that host** (D68; architect, 2026-11-23, for v0.09-T01, pin P1). The lease goes from
  `pending` to `unsatisfiable`, with `no_capacity` as its one reason, by the transition that
  [States](#states) already has.

How the driver decides each case is docs/driver.md's: place's in
[Where it places a lease](driver.md#where-it-places-a-lease) and
[On a `ready` host](driver.md#on-a-ready-host), and a run's in
[`place_pending`](driver.md#place_pending).
`no_capacity` is never listed beside an `unsupported_*` reason, since it applies only to a
supported request. No schema, code or field changes for it, so the contract does not.
- **An `unsatisfiable` lease with `no_capacity` stays `unsatisfiable`**, as every `unsatisfiable`
  lease does ([States](#states)): nothing makes it `ready` later. **A later `create` is judged
  afresh**, and MAY be answered `pending` or `ready`, for example once agentspace's operator has
  approved the project, or once the project's other leases are released, or when the size asked
  for is one the host already meets.

**Each reason's message quotes the normalized value**, and SHOULD read as below. **A client MUST
NOT parse a message: it reads the reason codes** ([Messages](#messages)). The messages of the
reasons v0 had keep v0's wording, `v0 supports` included, so that a command line with no `--plan`
prints what it printed under v0 (D76). agentspace's driver emits the `unsupported_size` message
and the `no_capacity` message for a host serving smaller leases byte for byte as below, which is its own rule, not the contract's (D71;
[docs/driver.md, What it supports](driver.md#what-it-supports); architect, 2026-11-23, for
v0.09-T01, pin P8):
- `Platform '<platform>' is not supported; v0 supports linux/arm64 only.`
- `Profile '<profile>' is not supported; v0 supports agent-linux with no version only.`
- `Size '<size>' is not supported; v0 supports sizes s, m, l and xl.`
  (D65, D71; from v0.09, #0049, in place of v0.08's `Size '<size>' is not supported; v0 supports size m only.`)
- `Plan's busiest turn '<turn>' has <n> agents at once; the largest size, xl, holds <m>.`
  (D75; from v0.10, #0050), where `<turn>` is `sized_for.turn`, written as the plan writes it,
  `<n>` is `sized_for.agents`, and `<m>` is the most agents at once that `xl` holds at
  agentspace's per-agent footprint, 32 today ([Plan](#plan)). It quotes a name from the plan, not
  a normalized value, since the plan is the value that cannot be met. **agentspace's lease CLI
  MUST emit it byte for byte** (architect, 2026-11-24, for v0.10-T01, pin P5). It is the message
  of every `plan_too_large` lease created without `--turn`, unchanged in v2.
- `Plan's turn '<turn>' has <n> agents at once; the largest size, xl, holds <m>.`
  (D84; from v0.11, #0052), the message of every `plan_too_large` lease created with `--turn`,
  even one that names the busiest turn, where `<turn>` is the turn `--turn` named, as the plan
  writes it, and `<n>` and `<m>` are as above. **agentspace's lease CLI MUST emit it byte for
  byte**, as it does the first (architect, 2026-11-25, for v0.11-T01, pin P20).
- `No capacity can be found for project '<project>': it has no host and is not on agentspace's project list.`
  ([docs/driver.md](driver.md#where-it-places-a-lease), whose wording is the architect's, 2026-11-20, for v0.07-T01)
- `No capacity can be found for project '<project>' at size '<size>': its host is serving other leases at a smaller size. Release them, or ask for a size they fit.`
  (D66, D71; from v0.09, #0049), where `<project>` is the normalized project and `<size>` the
  normalized size asked for. It is the message of both v0.09 cases above.

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

**A caller MUST handle `pending`**, by calling `get` until the state changes. Until agentspace's
v0.07 release (#0047), `create` answered `ready` or `unsatisfiable` straight away. **From v0.07,
`create` MAY answer `pending`**: a lease for a project whose compute is still being made stays
`pending` until it is, which can take minutes, and is then made `ready` by agentspace, never by a
command of the caller's. **From agentspace's v0.09 release (#0049), so does a lease whose
project's compute is being changed to meet its [size](#size), or is having its size recorded, and
a lease of a project that already has a `pending` lease** (D66, D67, D68;
[A host's life follows its leases](#a-hosts-life-follows-its-leases)). Such a lease MAY instead
become `unsatisfiable`, with `no_capacity`, when the compute, once changed or recorded, does not
meet it and another of the project's leases is `ready` on it ([Fields](#fields)). The caller's
`get`s let that work carry on, among them after a failed attempt (D48;
[docs/driver.md](driver.md#check_pending)).
A lease's TTL starts when it becomes `ready`, so time spent `pending` costs none of it. A caller
that stops waiting SHOULD `release` the lease.

**How `expired` is decided:** a lease that was `ready` is `expired` when the command reading it
runs at or after its `expires_at`, to the second. Nothing has to run at that moment: `get`, `list`,
`renew` and `release` each decide it when they read the lease, so the same lease can be `ready` in
one call and `expired` in the next with nothing in between. An `expired` lease has no `ssh`.

**What expiry and release do, and do not do, to a connection:**
- **Expiry MUST NOT close a connection that is already open.** The expiry is checked only when a
  connection is made, so it stops new connections only. A connection open past `expires_at` stays
  open until it ends or its host's compute stops
  ([A host's life follows its leases](#a-hosts-life-follows-its-leases)).
- **`release` ends the lease's connections on the caller's machine, as
  [`release`](#agentspace-lease-release) says (D59; architect, 2026-11-21, for v0.08-T01).** A
  connection made from another machine is not ended by it.
- **Today every client of a lease is on the caller's machine** (D59), so every connection of a
  lease is one that `release` looks for.
- **Neither `release` nor `renew` revokes a certificate (D10).** `release` deletes the caller's
  copy, and `renew` replaces it. A copy of any certificate that leaked stays valid until its own
  expiry. That is `expires_at` for the current certificate, and for an older one it MAY be later
  than `expires_at`, when a `renew` shortened the lease. That is why the TTL has a maximum (D24).

**How long a lease is kept:** a `released`, `expired` or `unsatisfiable` lease MUST stay readable
by `get` and `list` for at least 24 hours after it reached that state. After that it MAY be
forgotten, and `get` then answers `not_found`. In v2 no lease is forgotten.

**Where leases are kept:** in v2, on the caller's machine, so `get` and `list` see the leases
created there and no others. The files are [docs/state.md](state.md)'s, and
[`AGENTSPACE_STATE_DIR`](#environment) moves them.

### A host's life follows its leases

**From agentspace's v0.08 release (#0048), a project's compute is not stopped for want of a lease
while the project holds a live lease, and is stopped a short grace after its last live lease ends
(D56; architect, 2026-11-21, for v0.08-T01, S03 marks).** What a caller sees of it:
- **A live lease is one that is `pending` or `ready`** when it is read, with expiry decided as
  [States](#states) says. A lease that is `expired`, `released` or `unsatisfiable` is not live.
  The three words this section uses, live lease, a lease's end and the lease horizon, are defined
  in [docs/state.md](state.md#live-leases-and-the-lease-horizon) (architect, 2026-11-21, for
  v0.08-T01, S03 marks).
- **A lease's end is its release or its expiry**, whichever comes first.
- **A live lease does not promise running compute.** While one is live, the project's compute is
  not stopped for want of a lease, but it MAY still stop when it is idle, and the next connection
  starts it again: `ready` means a connection is expected to succeed, not that compute is running
  ([States](#states)). That is why the first connection after a quiet spell MAY take a few
  minutes.
- **A quiet live lease MAY cost compute**, with no connection open, until it ends or its compute
  stops idle. A lease that is no longer needed therefore SHOULD be released (architect,
  2026-11-21, for v0.08-T01, pin P23).
- **The grace is 15 minutes, fixed.** While agentspace's guard runs on the caller's machine, a
  project's compute is stopped within 25 minutes of the end of its last live lease: the grace, and
  up to 10 minutes for the stop to be noticed and done (D56). It is stopped whatever is running on
  it, and whether or not a connection is open. How compute is bounded when the guard does not run,
  how the stop is done, and what keeps it from stopping compute that is being made, are the
  operator's ([docs/operator.md](operator.md)), never a command of this CLI.
- **Nothing renews a lease on its own (D56).** A lease ends at its `expires_at` unless its holder
  renews it ([`renew`](#agentspace-lease-renew)), and its compute is stopped a grace later. The
  holder of a lease it still uses SHOULD renew it before `expires_at`, and SHOULD choose a TTL
  that covers the work it expects (architect, 2026-11-21, for v0.08-T01, pin P23).
- **From agentspace's v0.09 release (#0049), the compute's size follows the leases too (D66).** A
  project's compute is changed to another [size](#size) only while no lease of the project is
  `ready`, so a change of size never stops compute under a `ready` lease. What a supported
  request finds, when the project's compute exists and is not being made, by the first of these
  that applies:
  - **While another lease of the project is `pending`, the request is `pending` too**, whatever
    the compute's size, and is placed when the work that lease waits on finishes (D66; architect,
    2026-11-23, for v0.09-T01, pin P2). So no lease is placed `ready` on compute whose size is
    about to be changed under it.
  - **Compute whose size agentspace has not yet recorded** answers `pending`, whatever other
    leases of the project are `ready` on it, and never `no_capacity` (D67; pin P2). Its size is
    recorded meanwhile, and changed to the size asked for only when no other lease of the project
    is `ready` on it. Each project's first lease after v0.09's install can find this.
  - **Compute at exactly the size asked for** meets the lease at once, as before v0.09.
  - **Compute larger than the size asked for** meets the lease at once while another lease of the
    project is `ready` on it. With none, it is changed down to the size asked for, as below, so
    that compute left large by one lease does not cost its rate for every lease after it.
  - **Compute at any other size, with no other lease of the project `ready`**, is changed to the
    size asked for. The lease waits `pending` meanwhile, which can take minutes ([States](#states)).
  - **Compute smaller than the size asked for, while another lease of the project is `ready` on
    it**, answers `unsatisfiable` with `no_capacity` at once ([Fields](#fields)).

  How the change is made is [docs/driver.md, On a `ready` host](driver.md#on-a-ready-host)'s and
  [A resizing run](driver.md#a-resizing-run)'s, and how the guard and the idle stop meet it are
  those pages' and [docs/operator.md](operator.md)'s, never a command of this CLI.
- **No output, field, state or error changes for this.** It is a promise about when compute runs,
  never a field of a lease, and the lease's documents say nothing of it. The one answer the size
  rules add, `no_capacity` for compute serving smaller leases, is a reason v0 already had.
- **No stand-in can show a host stop**, so this section has no conformance case. The live checks
  of D62 show it (E0.08-T06), and those of D70 show a change of size (E0.09-T05-S01).

### The credential

**A ready lease's credential is an SSH key pair on the caller's machine and a certificate for it,
signed by agentspace's CA (D7).** What the caller sees of it is below; the files that hold it, and
how they are made and removed, are [docs/state.md](state.md)'s.

**The key:**
- **A new key pair is made for each lease, by `create`, or by agentspace when it places a lease
  that `create` answered `pending`.** The private key MUST NOT appear in any output,
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
- **Its serial is not part of this contract,** and a caller MUST NOT rely on it. v2 issues serial 0
  on every certificate, as v0 and v1 did; whether serials are needed is decided with revocation, card
  #0010 (#0029).

**The certificate authority:**
- **`create` and `renew` MUST NOT create agentspace's CA (#0029).** A CA made by issuing would be
  trusted by no host until the operator installs it there, so the lease would be `ready` and
  refuse every connection.
- **If there is no CA, `create` and `renew` MUST fail with `internal`**, change nothing, and say
  that agentspace's CA has not been set up ([Messages](#messages)). Setting it up is the
  operator's ([docs/operator.md](operator.md)). This holds for a `create` that would answer
  `pending` too: the CA is checked before that answer, as before anything is issued (from
  agentspace's v0.07 release, #0047; architect, 2026-11-20, for v0.07-T01).
- **When agentspace places a lease that `create` answered `pending`, it checks the CA again, and
  MUST NOT create it.** With no CA the lease stays `pending`
  ([docs/driver.md](driver.md#place_pending); architect, 2026-11-20, for v0.07-T01).

**The alias:**
- **`create` makes `ssh.host_alias` usable from the calling user's OpenSSH client configuration**,
  so that `ssh <host_alias>` works with no other options. **For a lease that `create` answered
  `pending`, agentspace makes it usable when it places the lease**, in the OpenSSH client
  configuration that [Environment](#environment) names for a `pending` lease. What it adds to
  that configuration, and how, is [docs/state.md](state.md)'s, and nothing else in it is changed.
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
- **In v2 the alias also forwards the agentspace board's ports to the lease host** (D12, D30), so
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
- **Supported in v2:** `linux/arm64` only. Any other well-formed platform is accepted, and the
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
- **Supported in v2:** `agent-linux` with no version. Any other well-formed profile, including
  `agent-linux@1`, is accepted and the lease is `unsatisfiable` with `unsupported_profile`, because
  v2 has no versioned profiles.

### Plan

**A plan is a JSON document, read from a file, that says which agents a project's work starts and
in what order; `create --plan <file>` picks the lease's size from it (D74, D75), and from v2 its
TTL when the lease's turn has a budget (D85).** It is the one value that names a file rather than
holding the value itself. A caller that knows the size it wants gives `--size` instead, and the two
MUST NOT be given together ([`create`](#agentspace-lease-create)). **From v2 the same plan MAY be
read from an epic's description instead**, with `--board` and `--epic`
([The plan block](#the-plan-block)): every rule of this section from the document's shape on
applies to it alike, and the same plan gives the same lease from either source (D83).

**The argument is a path to the file, and is used as given** (architect, 2026-11-24, for
v0.10-T01, pin P14):
- **Accepted:** a path, absolute or relative to the working folder of the command. It is not
  trimmed, since a path with its whitespace trimmed can name another file, and is never normalized,
  stored or printed, except as the message of a refusal quotes it ([Messages](#messages)). A value
  of only whitespace is a path like any other.
- **`-` is a file called `-` in the working folder**, never standard input: no command reads
  standard input ([The program](#the-program)).
- **An empty value is the `usage` error that any flag's empty value is**
  ([Command lines](#command-lines)).
- **An argument that is not valid Unicode, or that holds U+FEFF, is refused with `invalid_value`
  for `plan`**, as such a value of any flag is ([Input tolerance](#input-tolerance)), and no file
  is opened for it.
- **The file MUST be a regular file.** A folder, or a device such as `CON` or `NUL` on Windows, or a
  pipe, is refused as a file that could not be read (architect, 2026-11-24, for v0.10-T01, pin P7),
  and `create` MUST NOT wait on it as it would on a console or another program.

**The document's shape (D74):** a JSON object.
- **`turns`**, required: an array of turns, in the order they run. A plan has 1 to 32 turns.
- **`name`**, optional: the plan's name, such as its epic's. It is printed as `sized_for.plan`
  ([Fields](#fields)), and a plan without one gives a lease whose `sized_for` has no `plan`.
- **A turn** is an object with a `name`, required, and `stories`, required: an array of 1 to 32
  stories. From v2 it MAY have `budget_minutes`, the turn's wall-clock budget in minutes (D85;
  below).
- **A story** is an object with a `name`, required, and `subagents`, required: an array of 1 to 32
  subagents.
- **A subagent** is an object with a `name`, a `model` and an `effort`, all required, and MAY have
  `after`: an array of 0 to 31 names of the same story's subagents that it starts after, no name
  twice. **An empty `after` is valid, and means the same as none** (architect, 2026-11-24, for
  v0.10-T01, pin P20).
- **No other member is allowed, at any level** (architect, 2026-11-24, for v0.10-T01, pin P4). A
  plan has no member for a story's peak: the peak is always counted, never stated (below).

**#0203's staffing plan, as a plan (D74):**

```json
{"name":"v0.09","turns":[{"name":"T03","stories":[{"name":"E0.09-T03-S01","subagents":[
  {"name":"implementer","model":"Opus","effort":"high"},
  {"name":"test author","model":"Sonnet","effort":"high"},
  {"name":"runner","model":"Sonnet","effort":"medium"},
  {"name":"reviewer","model":"Opus","effort":"high","after":["implementer","test author","runner"]}]}]}]}
```

Its one story's peak is 3: the implementer, the test author and the runner start after none of
the others, and the reviewer starts after all three. Its one turn, `T03`, has 4 agents at once,
the story's 3 and 1 for its dev lead. So its busiest turn is `T03`, with 4, and at today's
footprint the lease's size is `s` and its `sized_for` is `{"plan":"v0.09","turn":"T03","agents":4}`.

**Names** (architect, 2026-11-24, for v0.10-T01, pin P2): the plan's, each turn's, each story's
and each subagent's `name`, and each entry of `after`.
- **1 to 64 printable ASCII characters, U+0020 to U+007E, with no space at either end:**
  `^[!-~]([ -~]{0,62}[!-~])?$`. A name MAY hold spaces inside it, as `test author` does.
- **Compared exactly**, character by character: never trimmed, and never folded to one case, so
  `T03` and `t03` are two names.
- **Unique among their siblings:** no two turns of the plan, no two stories of one turn, and no two
  subagents of one story have the same name. Two stories of different turns MAY share a name, and
  so MAY two subagents of different stories.
- **An `after` entry MUST name a subagent of its own story**, and the starts it gives MUST NOT form
  a cycle: no subagent starts after itself, directly or through others.

**Model and effort are recorded, not judged** (D27, D74, and D81 as changed for v0.10-T01;
architect, 2026-11-24, for v0.10-T01, pin P3).
- **`model` takes a name's form, above:** 1 to 64 printable ASCII characters with no space at
  either end. A model named with its version has a space, as `Opus 5.5` does, and a time-bound pin
  of one carries `+` and an expiry.
- **`effort` is 1 to 32 characters, ASCII letters, digits, `.`, `_` and `-`, starting with a
  letter or digit:** `^[A-Za-z0-9][A-Za-z0-9._-]{0,31}$`.
- **Each is kept as given, never normalized, and neither is a list of names**, so a new model or
  effort needs no new release. **Neither changes the size**: no sizing reads them (D73).
- **The plan is kept with the lease's record ([docs/state.md](state.md#the-lease-state-file)), for
  D27's later use, and no output prints the plan, a model or an effort**, an error's message
  included.

**A turn's `budget_minutes`** (D85; from v2):
- **An integer from 1 to 960, both inclusive:** the turn's wall-clock budget, as its milestone
  holds it. 960 minutes is 16 hours. It is optional, and `null` means that it is not given.
- **It MUST be written as a JSON integer of digits alone, with no fraction, exponent or sign**
  (architect, 2026-11-25, for v0.11-T01, pin P24): `90` is 90, and `90.0`, `9e1` and `-0` are
  refused, though each is a whole number. JSON Schema's `integer` accepts a whole number in any
  form, so `90.0` and `9e1` pass the plan's schema: this is a check the schema cannot make, and
  the program makes it ([Schemas](#schemas)). A number with a fraction, such as `90.5`, a string such as `"90"`,
  `true`, an array and an object are refused, and so are 0, a negative number and a number above
  960. A number the parser cannot hold, such as `1e400`, is refused as JSON already is (check 4,
  below).
- **It is refused like every other fault of the plan's shape:** by check 6, below, with the `<why>`
  `the document does not match the plan schema`, under `invalid_value` for `plan`, or for `epic`
  when the plan came from an epic ([The plan block](#the-plan-block)). A `budget_minutes` written
  with a fraction, an exponent or a sign is refused there too, though the plan's schema accepts it
  (pin P24).
- **It sets the TTL of a lease for that turn** ([TTL](#ttl)), and nothing else: it never changes
  a turn's agents at once or the size, and no output prints it.
- **Every v1 plan stays valid** (D85): a plan with no `budget_minutes` anywhere is accepted as v1
  accepted it, and gives the lease v1 gave it. A plan that has one, a `null` one included, was
  refused by v1, whose plan schema allows no other member ([Changes from v1](#changes-from-v1)).

**The file** (architect, 2026-11-24, for v0.10-T01, pins P1, P4, P18 and P21):
- **At most 262144 bytes (256 KiB), a byte order mark counted.** Its size is checked before it is
  read whole, and a build MUST NOT read more than 262145 bytes of it, so a file that grows past the
  bound as it is read is refused as larger too.
- **UTF-8.** A UTF-8 byte order mark at its start is accepted and ignored, the one exception to
  [Input tolerance](#input-tolerance)'s rule for U+FEFF. U+FEFF anywhere else in the file is
  refused. A file that is not valid UTF-8 is refused. A file in UTF-16 is refused either way: with
  its byte order mark it is not valid UTF-8 (check 3, below), and without one its bytes can be
  valid UTF-8, a zero byte beside each ASCII character, and it is then not valid JSON (check 4)
  (architect, 2026-11-24, for v0.10-T02, pin P8).
- **One JSON document, whose members' names are unique in each object.** JSON allows an object to
  give a name twice, and parsers then keep one value or the other; a plan MUST NOT, so that what
  agentspace reads is what its writer meant. Two names are the same when they are the same once
  their escapes are read. A string that holds an unpaired surrogate, as `"\ud800"` does, is not
  valid JSON.
- **A build's parser MAY limit how deeply the document nests, and MUST allow at least 64 levels**,
  counting the document as level 1. A plan's deepest value, an `after` entry, is at level 9, so a
  document nested past 9 levels is never a plan.
- **`null` for an optional member, the plan's `name`, a turn's `budget_minutes` or a subagent's
  `after`, means that it is not given**, as in every input document ([Schemas](#schemas)). `null` for a required member is
  refused.
- **Unlike an argument, nothing in the document is trimmed or normalized.** A value the rules above
  refuse is refused, never corrected.

**The bounds, and why they are these** (architect, 2026-11-24, for v0.10-T01, pin P1): at most 32
turns, 32 stories in a turn, 32 subagents in a story, and 31 names in an `after`, in a file of at
most 262144 bytes. They hold every plan agentspace has laid out, of which v0.06's, twenty-six
stories in six turns, is the largest (D74). A turn within them can have up to 1025 agents at once,
and any turn of more than the largest size holds is answered `plan_too_large` (below).

**A story's peak is counted from its starts, never stated (D74):** the most of its subagents in a
set of which none starts after another, directly or through others. A subagent starts after
another directly when that one is in its `after`, and through others when it starts after one
that starts after that one, at any depth. The peak is the most subagents that can run at once,
however long each takes: the width of the story's `after` graph. A story whose subagents each
start after the one before has a peak of 1, and one with no `after` at all has a peak of its
number of subagents. A peak stated anywhere else, such as on a story's card (D31), is not read.

**A turn's agents at once are the sum of its stories' peaks, plus 1 for its dev lead (D27,
D74).** **The plan's agents at once are its busiest turn's**: the turn with the most, and the
first such turn in the plan's order on a tie. Every plan that is well formed has at least 2.

**A lease is for one turn of its plan (D84).** It is the turn `--turn` names ([Turn](#turn)), or,
without `--turn`, the plan's busiest turn, the first such turn in the plan's order on a tie, as in
v1. The lease is sized for that turn's agents at once, its TTL is that turn's ([TTL](#ttl)), and
`sized_for` names that turn ([Fields](#fields)). Without `--turn`, the turn is the busiest, so the
plan's agents at once and the turn's are the same, as v1 sized every plan. D74's "No flag picks a
turn" was changed for v0.11 by D84.

**The size picked is the smallest of the [sizes](#size) whose vCPUs and memory are both at least
the agents at once of the turn the lease is for times agentspace's per-agent footprint (D75,
D84).** The size goes on as a
`--size` does: the driver judges it as it judges any size, and the lease's platform, profile and
TTL are given and judged as they are without a plan ([`create`](#agentspace-lease-create)).
- **The footprint is agentspace's, not this contract's**, and MAY change without a new contract
  version (D76; [Versioning](#versioning)). A new footprint changes which size a plan gets, never
  what a field means.
- **Today's footprint, for information:** half a vCPU and 2 GiB of memory for each agent at once
  (D73). At it, each size holds this many agents at once (architect, 2026-11-24, for v0.10-T01,
  pin P11):

  | Agents at once | Size picked |
  |---|---|
  | 2 to 4 | `s` |
  | 5 to 8 | `m` |
  | 9 to 16 | `l` |
  | 17 to 32 | `xl` |
  | More than 32 | None: `plan_too_large`, below |

- **A caller MUST read the size from the lease, and MUST NOT work it out from the plan**, since
  the footprint can change.
- **A turn of more agents at once than the largest size holds is never given less (D5, D75,
  D84):** its lease is `unsatisfiable` with the one reason `plan_too_large`, its `size` is `xl`,
  the largest, and its `sized_for` says which turn needed more ([Fields](#fields); D81), though
  another turn of the plan would fit. Such a plan is met by splitting that turn into turns that
  fit.

**A plan that is not well formed is refused with `invalid_value` for `plan`** (exit 2, D74), and
its message is `Plan '<value>' is not a valid plan: <why>.` (D81, as changed for v0.10-T01;
architect, 2026-11-24, for v0.10-T01, pins P7 and P12). `<value>` is the `--plan` argument as
given, quoted by [Messages](#messages)' rules, never a path agentspace made from it, and `<why>` is
the one of the table below that the first check the plan fails gives, word for word. **A plan read
from an epic is refused with `invalid_value` for `epic` instead**, with the same `<why>` in the
epic's message ([The plan block](#the-plan-block)).
- **Every refusal of a plan is `invalid_value`**, a bound exceeded included. `out_of_range` stays
  the TTL's alone.
- **A refused plan records no lease**, and creates nothing.

| Check, in order | The plan fails it when | `<why>` |
|---|---|---|
| 1. Size | A regular file at that path holds more than 262144 bytes, by its size found before it is read, or grows past that as it is read | `the file is larger than 262144 bytes` |
| 2. Read | The file's size cannot be found, or it cannot be opened or read, for any reason: no file has that path, a folder, a device or a pipe has it, access is denied, or the argument is not valid Unicode or holds U+FEFF | `the file could not be read` |
| 3. UTF-8 | The file is not valid UTF-8 | `the file is not valid UTF-8` |
| 4. JSON | The file, its leading byte order mark set aside, is not one JSON document: it is empty or only whitespace, has text after the document, holds U+FEFF outside a string, holds a string with an unpaired surrogate, nests deeper than the parser allows, or holds a number the parser cannot hold, such as `1e400` (architect, 2026-11-24, for v0.10-T02, pin P5) | `the file is not valid JSON` |
| 5. Members | An object gives one member's name twice | `an object has two members named '<member>'` |
| 6. Schema | The document is not of the shape above: a value of the wrong type, the document included; a required member missing or `null`; a member that is not in the shape; an empty or too long `turns`, `stories` or `subagents`, or an `after` of more than 31 names; an `after` that names one subagent twice; a name, model or effort not of its form; or a `budget_minutes` that is not an integer from 1 to 960, or one not written as digits alone, such as `90.0`, `9e1` or `-0`, though the schema accepts `90.0` and `9e1` (architect, 2026-11-25, for v0.11-T01, pin P24) | `the document does not match the plan schema` |
| 7. Turns | Two turns have one name | `turn '<turn>' is named twice` |
| 8. Stories | Two stories of one turn have one name | `story '<story>' is named twice in turn '<turn>'` |
| 9. Subagents | Two subagents of one story have one name | `subagent '<name>' is named twice in story '<story>'` |
| 10. After | An `after` entry names no subagent of its story | `subagent '<name>' in story '<story>' starts after '<after>', which is no subagent of that story` |
| 11. Cycle | A story's subagents start after one another in a cycle, a subagent that starts after itself included | `the subagents of story '<story>' start after each other in a cycle` |

**Which fault is reported** (architect, 2026-11-24, for v0.10-T01, pins P7 and P12):
- **Only the first check that fails, in the table's order, is reported**, and only one fault of
  it: the first in document order. For check 5, that is the first member whose name an earlier
  member of the same object has; for checks 7 to 9, the first name that repeats an earlier
  sibling's; for check 10, the first subagent with such an entry, and its first such entry; and
  for check 11, the first story with a cycle.
- **`<turn>`, `<story>`, `<name>` and `<after>` are names from the plan, quoted raw, as the plan
  writes them.** They have passed the schema, so each is printable ASCII. `<turn>` is the turn
  that holds the story, and `<story>` the story that holds the subagent.
- **`<member>` and `<value>` are quoted by [Messages](#messages)' rules:** each control character
  written as `\u` and four lowercase hex digits, and each unpaired surrogate in the argument as
  [Input tolerance](#input-tolerance) says.
- **No `<why>` carries an operating system's error text, or any path other than the argument**
  (D81, as changed for v0.10-T01), and none quotes a model, an effort or any other value of the
  document.

**Which checks the plan schema can make.**
[`plan.input.schema.json`](../schemas/plan.input.schema.json) holds check 6: the shape, every
member required or allowed, the bounds on each array, `after`'s names unique, and the forms of
names, models and efforts. **No JSON Schema can make the others, and `create` MUST:** checks 1 to
5, which are about the file before it is a document (a schema sees a document already parsed, with
a member given twice already lost), and checks 7 to 11, which compare names with one another. A
document the schema accepts is therefore not always a plan, and passing the schema never makes a
plan acceptable, as for every input schema ([Schemas](#schemas)).

**In a request document, `plan` is the plan document itself**, an object, never a file name, and
`null` or absent means no plan. A request whose `plan` and `size` are both present and not `null`
is refused by its schema. The plan in it is validated as `plan.input.schema.json` validates a
plan, through that schema's copy in `lease-create.input.schema.json`'s `$defs`, and checks 7 to 11
apply to it as to a file's. No request member carries a path (architect, 2026-11-24, for
v0.10-T01, pins P8 and P19).

#### The plan block

**An epic carries its plan as one fenced block in its card's description, holding exactly a plan
document (D83).** `create --board <id> --epic <ticket>` reads the description
([The board read](#the-board-read)) and takes the plan from that block. The plan is then the same
value as a `--plan` file's: every rule of [Plan](#plan) from its shape on applies to it, and the
same plan read from an epic and from a file gives the same lease (D83). Where the epic's prose and
its block disagree, the block is the plan (D83).

**Lines.** The description is read as a sequence of lines, and every rule below is decided by
lines alone:
- **A line ends at a line feed (U+000A)**, or at the end of the description for the last line. **A
  carriage return (U+000D) just before that line feed is part of the line end**, not of the line's
  text, so lines MAY end in LF or CRLF, mixed in one description. A carriage return anywhere else,
  a second one before the line feed included, is text.
- **No other character ends a line:** not U+000B, U+000C, U+0085, U+2028 or U+2029.
- **A line's text is compared as it is.** Nothing in it is trimmed or folded to one case: a space
  or a tab, at either end, is text, and so is a U+FEFF.

**The fence lines:**
- **The opening line is a line whose text is exactly ```` ```agentspace-plan ````**: three
  backticks (U+0060), then `agentspace-plan` in lowercase, and nothing else.
- **The closing line is a line whose text is exactly ```` ``` ````**: three backticks, and nothing
  else. ```` ```` ````, and ```` ``` ```` followed by a space, are not closing lines.
- **A near miss is a line that is not the opening line, and whose text starts with three or more
  backticks followed at once by `agentspace-plan`** (D83). So each of these is a near miss:
  ```` ```agentspace-plan ```` followed by a space or a tab, or by a carriage return that is not
  just before the line feed; ```` ````agentspace-plan ````, with a fourth backtick;
  ```` ```agentspace-plan json ````; and ```` ```agentspace-planner ````.
- **Nothing else is a near miss or an opening line**: a line that starts with anything other than
  a backtick, such as a space, a tab, a `>` or a U+FEFF; a line of tildes, such as
  `~~~agentspace-plan`; and a line whose `agentspace-plan` is in another case, such as
  ```` ```Agentspace-Plan ````. Each is text.

**The block** is an opening line, the lines after it, and the first closing line after it. **Its
content is every character from the one after the opening line's line feed to the one before the
closing line's first backtick**, so the line end of its last line is content, and a closing line
straight after the opening line gives empty content.

**Finding the block.** The lines are read in order, from the first, and the first fault stops the
reading:
1. **Outside a block, an opening line starts one.** If a block was already found, the description
   has two, and is refused.
2. **Outside a block, a near miss is refused** (D83): a line its writer most likely meant as an
   opening line is never skipped as text.
3. **Outside a block, any other line is text for people**, and is skipped. That includes every
   other fence, such as ```` ```json ```` or ```` ``` ````: no Markdown is parsed, so a line is
   never read as inside another fence or inside a quote.
4. **Inside a block, the closing line ends it.**
5. **Inside a block, an opening line or a near miss is refused** as the block before it never being
   closed. A plan document is JSON, in which no line can start with a backtick, so such a line is
   never a plan's.
6. **Inside a block, any other line is content.**
7. **At the end of the description, a block still open is refused as never closed, and a
   description in which no block was found is refused as having none.**

**So the line rule alone decides what a description holds** (D83):
- **An opening line inside another fence**, such as an example of a plan block inside a
  ```` ```markdown ```` fence, **is an opening line**, since that fence is not parsed. A
  description that shows a plan block as an example, and also carries its own, has two blocks and
  is refused.
- **A block in a quote**, each line starting `> `, **is text** and carries no plan, since its
  first line starts with `>`.
- **An indented block**, its first line starting with a space or a tab, **is text** too.
- **Prose before and after the block, the epic's execution plan by turn among it (D14), is read by
  no rule here.**

**The content is the plan document's bytes, read as a `--plan` file's bytes are**
([Plan](#plan), "The file"; D83): the content in UTF-8, at most 262144 bytes, a byte order mark at
its start accepted and ignored and U+FEFF anywhere else refused, and one JSON document (checks 1, 3
and 4), then a plan of the shape and names [Plan](#plan) gives (checks 5 to 11). **The first check
the content fails gives the `<why>`, word for word as the table in [Plan](#plan) gives it**, checks
1 and 3 to 11, "the file" in checks 1, 3 and 4 included; check 2, a file that could not be read,
never applies to a block. **Check 3, not valid UTF-8, cannot be reached for a block read from a
board either** (architect, 2026-11-25, for v0.11-T01, pin P26)**:** the board's answer is read as JSON before its description is looked at, so a
description that is not valid text, one holding an unpaired surrogate included, makes the whole
answer one that could not be read, which is `internal` ([The board read](#the-board-read)), and
the content of a description that reads is always valid UTF-8.

**The content is checked only after the whole description has been read with no line fault.**
The reading above runs to the end of the description first, and a line fault it finds, at any
line, is the refusal, whatever the content of a block before it holds. So a description with a
block whose content is not a plan and, after it, a second block is refused as having more than one
block, never with the content's `<why>`.

**Every refusal is `invalid_value` for `epic`** (D86), and records no lease. Its message is
`Epic '<value>' carries no usable plan: <why>.`, where `<value>` is the `--epic` value as given,
quoted by [Messages](#messages)' rules (architect, 2026-11-25, for v0.11-T01, pin P11). The `<why>`
of each fault, word for word, in the order the reading above meets them:

| Fault | `<why>` |
|---|---|
| A second opening line outside a block, once a block was found | `its description has more than one agentspace-plan block` |
| A near miss outside a block | `a line that starts an agentspace-plan block is not exactly the fence line` |
| An opening line or a near miss inside a block, or the end of the description inside one | `its agentspace-plan block is never closed` |
| No opening line in the whole description | `its description has no agentspace-plan block` |
| Content that [Plan](#plan)'s checks refuse | That check's `<why>`, word for word, as [Plan](#plan)'s table gives it |

- **A `<why>` never quotes the description, the block or a line of either**, beyond the names
  [Plan](#plan)'s own `<why>`s quote.
- **A description with no fault and one block gives the plan**, which then sizes the lease
  exactly as a `--plan` file's would ([`create`](#agentspace-lease-create)).
- **The board's other refusals of an epic**, an archived card, no card of that ticket and an
  answer too large to read, are [The board read](#the-board-read)'s.

### Board

**The localswim board an epic is read from, by its ID (D86).** `create --board <id>` gives it,
always with [`--epic`](#epic), and the lease's plan is then read from that epic
([The board read](#the-board-read)). A board's name is never accepted, since two boards MAY share
one and a lookup would be a guess (D86).
- **Accepted:** a UUIDv4 in any case, with surrounding whitespace, as a [lease ID](#lease-id) is:
  `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`, where each `x` is a hex digit and `y` is one of `8`, `9`,
  `a` or `b` (D89 as changed for v0.11-T01; architect, 2026-11-25, for v0.11-T01, pin P18).
- **Normalized to:** lowercase, whitespace trimmed. The board is read, and recorded, by its
  normalized ID ([The board read](#the-board-read)).
- **Refused:** any other UUID version, braces, a `urn:uuid:` prefix, a UUID without its `-`
  separators, an empty value, a board's name, and anything else. Every refusal of a board's form
  is `invalid_value` for `board` (exit 2).
- **A well-formed ID that names no board agentspace can read is `invalid_value` for `board`
  too**, found by the board read, not by its form ([The board read](#the-board-read); architect,
  2026-11-25, for v0.11-T01, pin P2). It is not `not_found`, which is for a lease ID alone.
- **No default.** `--board` is given with `--epic` or not at all, and never with `--plan` or
  `--size` ([Command lines](#command-lines)).
- **Its messages** are in [Messages](#messages) and [The board read](#the-board-read).

### Epic

**The epic card whose description carries the plan, by its ticket number on the
[board](#board) (D83, D86).** It is the number `card get` takes, as in `52` for `#0052`.
- **Accepted:** 1 to 16 ASCII decimal digits, leading zeros allowed, with surrounding whitespace,
  whose value is from 1 to 9007199254740991, localswim's ticket range (architect, 2026-11-25, for
  v0.11-T01, pin P5).
- **Normalized to:** the value in decimal with no leading zero, whitespace trimmed: `0052`
  becomes `52`.
- **Refused:** a `#` before the digits, as in `#0052`; a sign, a fraction or an exponent, as in
  `52.0`; a value of only zeros, `0` among them; more than 16 digits, leading zeros included; a
  value above 9007199254740991; an empty value; and anything else. **Every refusal of an epic is
  `invalid_value` for `epic`** (exit 2), a value outside the ticket range included: `out_of_range`
  stays the TTL's alone.
- **A well-formed ticket that names no card on the board, a card that is archived, and a card
  whose description carries no usable plan are `invalid_value` for `epic` too**, found by the board
  read ([The board read](#the-board-read), [The plan block](#the-plan-block); D86).
- **No default.** `--epic` is given with `--board` or not at all, and never with `--plan` or
  `--size` ([Command lines](#command-lines)).
- **Its messages** are in [Messages](#messages), [The board read](#the-board-read) and
  [The plan block](#the-plan-block).

### Turn

**The name of one turn of the lease's plan; `create --turn <name>` sizes the lease for that turn
alone, and gives it that turn's TTL (D84, D85).** Without `--turn`, a plan lease is for the plan's
busiest turn, as in v1 ([Plan](#plan)). `--turn` needs a plan, from `--plan` or from `--epic`
([Command lines](#command-lines)).
- **Accepted:** a turn name exactly as the plan writes it, in a [name](#plan)'s form: 1 to 64
  printable ASCII characters, U+0020 to U+007E, with no space at either end,
  `^[!-~]([ -~]{0,62}[!-~])?$` (architect, 2026-11-25, for v0.11-T01, pin P5).
- **Not trimmed, and never normalized** (architect, 2026-11-25, for v0.11-T01, pin P5). A turn's name is compared exactly, character by
  character, as every name of a plan is (D84; [Plan](#plan)), so a value is used as given. This is
  an exception to [Input tolerance](#input-tolerance)'s trimming, as `--plan`'s path is. A value
  with whitespace at either end is outside the form, and is refused as below; it is never trimmed
  into a turn's name.
- **Compared exactly:** `T01` and `t01` are two names, and `--turn t01` names no turn of a plan
  whose turn is `T01`.
- **An empty value is the `usage` error that any flag's empty value is**
  ([Command lines](#command-lines)). A value of only whitespace is a value, outside the form.
- **Refused with `invalid_value` for `turn`, in two ways** (D84; architect, 2026-11-25, for
  v0.11-T01, pin P5):
  - **A value outside the form**, among them one with whitespace at either end, one longer than 64
    characters, one holding a character outside printable ASCII, one holding U+FEFF, and one that
    is not valid Unicode. It is refused when the values' forms are checked, before any plan is
    matched for it ([Exit codes and errors](#exit-codes-and-errors)).
  - **A well-formed value that names no turn of the plan.** It is refused only after the plan is
    read, from either source, and after every other value is checked. Its message names the
    plan's turns ([Messages](#messages)).
- **The whole plan is still read and checked.** `--turn` picks the turn the lease is for; it does
  not cut the plan before it is checked, so a fault anywhere in the plan, in a turn `--turn` does
  not name included, refuses it as [Plan](#plan) says.
- **Normalized to:** nothing; the name as given, which is the plan's own. It is printed as
  `sized_for.turn` ([Fields](#fields)).

**What `--turn` changes, and what it does not** (D84):
- **The lease is sized as if the plan held that turn and no other:** its agents at once are that
  turn's, the sum of its stories' peaks plus 1 for its dev lead ([Plan](#plan); D74), and the size
  picked is the smallest that holds them. `sized_for.turn` is that turn's name and
  `sized_for.agents` its agents at once, whether or not it is the busiest.
- **A turn that no size holds is `plan_too_large`**, as a busiest turn is, though another turn of
  the plan would fit, with its own message ([Fields](#fields)).
- **The TTL is that turn's**, from its `budget_minutes` ([TTL](#ttl)).
- **`sized_for.plan` is still the plan's `name`**, never a turn's.
- **It applies alike to a plan from `--plan <file>` and to one from `--epic`**, so the same plan
  and the same `--turn` give the same lease from either source (D83, D84).

### Size

- **Accepted:** 1 to 8 letters and digits, in any case, with surrounding whitespace.
- **Normalized to:** lowercase, whitespace trimmed.
- **Default:** `m`, for a `create` with no plan. With a plan, from `--plan` or `--epic`, the plan
  picks the size, and `--size` MUST NOT be given ([Plan](#plan); D75, D86).
- **A size is a floor of vCPUs and memory** (D65): the compute that serves a lease of that size
  has at least that many vCPUs and at least that much memory. A size names nothing else, and never
  a machine type (D1).

  | Size | vCPUs | Memory |
  |---|---|---|
  | `s` | 2 | 8 GiB |
  | `m` | 4 | 16 GiB |
  | `l` | 8 | 32 GiB |
  | `xl` | 16 | 64 GiB |

- **Supported in v2:** `s`, `m`, `l` and `xl`, as from agentspace's v0.09 release (#0049; D65), for
  `linux/arm64`, the one platform v2 supports (D13). Before v0.09, `m` alone was. Any other
  well-formed size, such as `xxl`, is accepted, and the lease is `unsatisfiable` with
  `unsupported_size`.
- **A size is judged on its own, whatever the platform asked for** (architect, 2026-11-23, for
  v0.09-T01, pin P9). A request for another platform is `unsatisfiable` with
  `unsupported_platform`, and gets `unsupported_size` beside it only when its size is not one of
  the four, as a request with size `m` was answered before v0.09. So `windows/amd64` with size `l`
  lists `unsupported_platform` alone.
- **A lease is met by compute at least its size, in both vCPUs and memory** (D66). A lease placed
  on larger compute is met, not swapped (D5), and its `size` stays the size asked for, so nothing
  in the lease shows the difference (D1). When compute of another size has to be changed to meet a
  lease, and when a lease is refused with `no_capacity` instead, is
  [A host's life follows its leases](#a-hosts-life-follows-its-leases)'s.
- **Which machine meets which size is the driver's alone**, and this contract never names it
  ([docs/driver.md, Sizes and instance types](driver.md#sizes-and-instance-types)).

### TTL

**How long a lease's credential is valid, from the moment it is issued (D24).** It is a duration,
never a clock time.
- **Default:** for `create`, the TTL its plan gives (below), and 8 hours for a `create` with no
  plan. For `renew`, the lease's current `ttl_seconds`, whatever gave it.
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

**The TTL a plan gives** (D85; from v2). For a `create` with a plan, from `--plan` or from
`--epic`, and no `--ttl`:
- **The turn is the lease's** ([Plan](#plan); D84): the one `--turn` names, or the plan's busiest
  turn, the first such turn in the plan's order on a tie.
- **When that turn has `budget_minutes`, the TTL is the budget times agentspace's TTL factor,
  rounded up to the whole minute.** It is that whole number of minutes, in seconds, and the lease's
  `ttl_seconds` carries it.
- **When that turn has none, the TTL is the default, 8 hours**, as for a `create` with no plan
  (D24), whatever budgets the plan's other turns have.
- **`--ttl` wins over both** (D85): a `create` with a plan and `--ttl` gets the TTL `--ttl` gives,
  checked as any `--ttl` is, and the budget changes nothing.
- **It is a TTL like any other:** the lease's certificate is issued for it, `expires_at` is the
  issue time plus it ([Times](#times)), a `renew` without `--ttl` uses it again, as the lease's
  `ttl_seconds`, without reading the plan or the board again
  ([`renew`](#agentspace-lease-renew); architect, 2026-11-25, for v0.11-T01, pin P20), and a
  lease that is `unsatisfiable`, `plan_too_large` among them, carries it as the TTL asked for
  ([Fields](#fields)).
- **Every TTL a plan gives MUST be within the range above, and a plan lease MUST NOT be refused
  with `out_of_range` for a TTL it was not given** (D85). The budget's bound and the factor are
  chosen together so that it is: at today's factor, 960 minutes gives 1440 minutes, 24 hours, the
  range's top, and 1 minute gives 2. So the TTL a plan gives is never clamped, and no plan has an
  `out_of_range` case.
- **The factor is agentspace's, not this contract's**, as the per-agent footprint is
  ([Plan](#plan); D73, D76, D85), and MAY change without a new contract version
  ([Versioning](#versioning)). A new factor changes which TTL a plan gets, never what a field
  means. **A caller MUST read the TTL from the lease**, `ttl_seconds` and `expires_at`, and MUST
  NOT work it out from the budget.
- **Today's factor, for information: 1.5**, so the TTL is the budget and half again (D85), in
  seconds `ceil(budget_minutes * 3 / 2) * 60` (architect, 2026-11-25, for v0.11-T01, pin P10):

  | `budget_minutes` | TTL | `ttl_seconds` |
  |---|---|---|
  | 1 | 2 minutes | 120 |
  | 45 | 68 minutes, since 67.5 is rounded up | 4080 |
  | 120 | 3 hours | 10800 |
  | 180 | 4 hours 30 minutes | 16200 |
  | 210 | 5 hours 15 minutes | 18900 |
  | 960 | 24 hours, the range's top | 86400 |

  So a `create --plan` of v0.11's own plan, [plans/v0.11.json](plans/v0.11.json), with no
  `--turn`, is for its busiest turn, `T02`, of 11 agents at once and a budget of 210 minutes
  (D89), and gets size `l` and a TTL of 18900 seconds; with `--turn T01`, 9 agents and 180 minutes
  give size `l` and 16200 seconds.

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
  until a driver supports it (D5). No v2 profile has either.

### agent-linux

**The one profile v2 supports, with no version.** Every host that serves a lease with profile
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

**The top level.** In v2, `lease` is the only command, and any other is `usage`. A later version
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
                        [--size <size> | --plan <file> | --board <id> --epic <ticket>]
                        [--turn <name>] [--ttl <duration>]
```

**Requests a lease for one agent of a project, and prints it.** The lease's state is `ready`,
`pending` or `unsatisfiable`, and a caller MUST check it before connecting:
- **`ready`:** `ssh.host_alias` is usable from the caller's OpenSSH configuration, and
  `expires_at` is the issue time plus the TTL ([The credential](#the-credential)).
- **`pending`:** call `get` until it is `ready` or `unsatisfiable`. From agentspace's v0.07 release
  (#0047), a `create` MAY answer it for a project whose compute is still being made, and from
  v0.09 (#0049) for one whose compute is being changed to the lease's size ([States](#states),
  [Size](#size)).
- **`unsatisfiable`:** `reasons` says which values cannot be met. Nothing was issued, and the lease
  needs no `release`.

**An unsatisfiable lease is a successful `create` (exit 0)**, because the lease was recorded and
`get` returns it. A caller that only checks the exit code will miss it, which is why the state MUST
be checked.

**A platform, profile or size that cannot be met is never swapped for another (D5).** If any one of
them cannot be met, the lease is `unsatisfiable`, whatever the others are. Whether a request can be
met is decided before anything is issued. A lease that larger compute serves is met, not swapped,
and its `size` stays the size asked for ([Size](#size), D66). **A plan whose lease's turn, the one `--turn`
names or else the busiest, no size holds is never given the largest size instead:** its lease is
`unsatisfiable` with `plan_too_large`, as below (D75, D84).

| Flag | Type | Default | Notes |
|---|---|---|---|
| `--project <name>` | [Project](#project) | None; required | The project the agent works on. |
| `--platform <os/arch>` | [Platform](#platform) | `linux/arm64` | Only `linux/arm64` is supported in v2. |
| `--profile <name>` | [Profile](#profile) | `agent-linux` | Only `agent-linux` is supported in v2. |
| `--plan <file>` | [Plan](#plan) | None | A file holding a plan; the lease gets the size its turn needs, the turn `--turn` names or else the plan's busiest, as below. Not with `--size`, `--board` or `--epic`. From v0.10 (#0050; D75). |
| `--board <id>` | [Board](#board) | None | The localswim board that holds the epic. With `--epic`, and not with `--plan` or `--size`. From v0.11 (#0052; D86). |
| `--epic <ticket>` | [Epic](#epic) | None | The epic's ticket number on that board; the lease's plan is the epic's ([The plan block](#the-plan-block)). With `--board`, and not with `--plan` or `--size`. From v0.11 (#0052; D83, D86). |
| `--turn <name>` | [Turn](#turn) | The plan's busiest turn | The turn of the plan the lease is for. Only with `--plan` or `--epic`. From v0.11 (#0052; D84). |
| `--size <size>` | [Size](#size) | `m`, without a plan | `s`, `m`, `l` and `xl` are supported in v2, as from v0.09 (#0049). Not with `--plan`, `--board` or `--epic`. |
| `--ttl <duration>` | [TTL](#ttl) | `8h`, or with a plan the TTL its turn gives ([TTL](#ttl)) | From 1 minute to 24 hours. |
| `-h`, `--help` | | | Print the help below and exit 0. |

**The flags stand in this table in the order their values are checked**: project, platform,
profile, plan, board, epic, turn, size and TTL (architect, 2026-11-24, for v0.10-T01, pin P13;
architect, 2026-11-25, for v0.11-T01, pin P20). A plan's file is read and checked in `plan`'s
place, as in v1, and the board is read, and `--turn` is matched against the plan, only once every value of the table
has passed its own checks ([Exit codes and errors](#exit-codes-and-errors)). `--size` is never
given with `--plan`, `--board` or `--epic`, nor `--plan` with either of the other two
([Command lines](#command-lines)).

**`--plan <file>` sizes the lease for a plan** (D74, D75):
- **The file is read as one UTF-8 JSON document**, a plan as [Plan](#plan) defines it, which also
  gives the path's rules, the file's bounds and its byte order mark. It is read and parsed when
  the values are checked ([Command lines](#command-lines), "Which error is reported", rule 5),
  before `create` waits for any other command
  ([Concurrency and atomicity](#concurrency-and-atomicity); architect, 2026-11-24, for v0.10-T01,
  pin P28), and is never written.
- **Standard input is still never read** ([The program](#the-program)): `--plan -` names a file
  called `-`.
- **`--plan` with `--size` is a `usage` error, with `field` `plan`, in either order**, since the
  plan picks the size (D75; architect, 2026-11-24, for v0.10-T01, pin P6).
  [Command lines](#command-lines) says where it ranks among usage errors.
- **`--platform`, `--profile` and `--ttl` apply as they do without it**, with the same defaults
  (D75), except that from v2 the TTL's default is the one the plan's turn gives ([TTL](#ttl); D85).
- **A file that cannot be read, or that does not hold a plan as [Plan](#plan) defines it, is
  refused with `invalid_value` for `plan`** ([Plan](#plan) gives each message), and no lease is
  recorded.

**`--board <id> --epic <ticket>` sizes the lease for the plan an epic carries** (D83, D86):
- **The two are given together, or neither** ([Command lines](#command-lines)): `--board` is the
  board's ID, and `--epic` the epic card's ticket number on it. Neither is given with `--plan` or
  `--size`, since the epic carries the plan and the plan picks the size.
- **The plan is the one in the epic's `agentspace-plan` block** ([The plan block](#the-plan-block)),
  read from the board as [The board read](#the-board-read) says, which also says what the read
  refuses. A plan read from an epic is checked as a plan file is, and **the same plan gives the
  same lease from either source** (D83): the same size, `sized_for` and TTL.
- **From the plan on, a board lease is made exactly as a `--plan` lease is** (D86, D88): the same
  sizing, the same `plan_too_large`, the same TTL, and the same placing. No driver is given the
  plan, the board or the epic.
- **A board lease prints the plan's own `name` as `sized_for.plan`, never the board's ID or the
  epic's ticket** (D87), and leaves `plan` out of `sized_for` when the plan has no `name`. No
  output prints the board, the ticket or the card's revision ([What is recorded](#what-is-recorded)).

**`--turn <name>` picks the turn the lease is for** (D84; [Turn](#turn)):
- **It needs a plan, from `--plan` or from `--epic`.** Without either it is a `usage` error
  ([Command lines](#command-lines)).
- **The lease is sized for that turn alone**, as if the plan held that turn and no other, and its
  `sized_for.turn` and `sized_for.agents` are that turn's. Without `--turn` the turn is the busiest,
  as in v1.
- **Its TTL is that turn's**, unless `--ttl` is given ([TTL](#ttl)).
- **A `--turn` that names no turn of the plan is refused** with `invalid_value` for `turn`, and no
  lease is recorded.

**The size it picks is the smallest of [Size](#size)'s sizes that holds the agents at once of the
lease's turn at agentspace's per-agent footprint** (D73, D75, D84), as [Plan](#plan) says, with
today's figure, for information.
- **From there the lease is placed exactly as a `create` with `--size` set to the size picked, and
  the same other flags, would be** (D75): the same states and reasons, `unsupported_size` among
  them for a platform that lacks the size picked, and the same placing on the project's compute
  ([A host's life follows its leases](#a-hosts-life-follows-its-leases)). No driver is given the
  plan ([docs/driver.md](driver.md)). A plan that fits but asks for a platform or profile that
  cannot be met gets `sized_for`, the size picked, and the reasons a `--size` lease would get
  (architect, 2026-11-24, for v0.10-T01, pin P16).
- **The lease's `size` is the size picked, and its `sized_for` says what it was sized for**, in
  every state ([Fields](#fields); D75). A lease made without a plan has no `sized_for`.

**A plan that no size holds is `unsatisfiable`, with `plan_too_large`** (D75, D81, D84): a plan
whose lease's turn has more agents at once than the largest size holds, which is 32 at today's
footprint.
- **`plan_too_large` is the lease's only reason**, whatever its platform and profile (D81;
  architect, 2026-11-24, for v0.10-T01, pin P16). `create` decides it before any driver is asked
  to place the lease, and that asking is what judges the platform and the profile.
- **Its `size` is `xl`, the largest size, and it keeps `sized_for`**, which says why (D81). Since
  it is `unsatisfiable`, nothing is issued for it at that size or any other (D5).
- **What `create` still does for it** (architect, 2026-11-24, for v0.10-T01, pin P26): it still
  needs a driver, so with none it fails with `internal` first, as every `create` does
  ([Messages](#messages), "No driver"), and it still records the lease, so a failure to read,
  lock or write the lease records is `internal` as for any `create`. **It makes every check that
  `create` makes before a driver is asked to place a lease, in the same order and with the same
  errors**, among them "`AGENTSPACE_PROXY_COMMAND` not valid text" and "Way to reach the host not
  found" ([Messages](#messages)), so a `plan_too_large` answer still means the install is sound
  (architect, 2026-11-24, for v0.10-T01, pin P32). **It MUST NOT ask any driver to place the
  lease** (pin P26), so it starts no provisioning program ([Environment](#environment)), and does
  nothing a driver's placing does: it needs no CA, and makes no key, credential or alias, and no compute is made,
  changed or started. The lease is kept as every `unsatisfiable` lease is, needs no `release`, and
  its `create` exits 0 ([States](#states)).
- **Why it is refused rather than given `xl`:** a request that cannot be met is never quietly
  given less (D5). A plan whose too-large turn is split into turns that fit is met.

**Request document** (never read by the v2 CLI; see [Schemas](#schemas)):
[`lease-create.input.schema.json`](../schemas/lease-create.input.schema.json), with members
`project`, `platform`, `profile`, `plan`, `board`, `epic`, `turn`, `size` and `ttl`.
- **`plan` is the plan document itself**, as
  [`plan.input.schema.json`](../schemas/plan.input.schema.json) defines it, never a file name: an
  object, or `null` or absent for no plan ([Plan](#plan); architect, 2026-11-24, for v0.10-T01,
  pins P8 and P19).
- **`board` is a string, the board's ID in [Board](#board)'s accepted forms, surrounding
  whitespace allowed, or `null` or absent** (architect, 2026-11-25, for v0.11-T01, pin P6).
- **`epic` is a string, the ticket number in [Epic](#epic)'s accepted forms, or `null` or
  absent, and never a JSON number**, as `ttl` is a string (architect, 2026-11-25, for v0.11-T01, pin P6).
- **`turn` is a string, a turn name in [Turn](#turn)'s exact form, with no surrounding whitespace,
  or `null` or absent** (architect, 2026-11-25, for v0.11-T01, pin P6).
- **The schema refuses each of these, as the command line does**, where "without" and "beside"
  count `null` and absence alike:
  - **A `plan` and a `size` both present and not `null`** (architect, 2026-11-24, for v0.10-T01,
    pin P8).
  - **A `board` without an `epic`, or an `epic` without a `board`** (architect, 2026-11-25, for v0.11-T01, pin P6).
  - **A `board` and `epic` beside a `plan` or a `size`** (architect, 2026-11-25, for v0.11-T01, pin P6).
  - **A `turn` with neither a `plan` nor a `board`** (architect, 2026-11-25, for v0.11-T01, pin P6).
- **A request's `board` and `epic` name the plan; they never carry it.** A request that holds the
  plan itself gives `plan`, and a request that names an epic gives `board` and `epic`; no request
  member carries a path or a credential.

**Output:** the lease document. **Exit codes:** 0, 1 and 2.

**Errors:**

| Code | Field | When |
|---|---|---|
| `usage` | `project` | No `--project`, or `--project` with no value, an empty value, or given twice. |
| `usage` | The flag | Any other flag with no value, an empty value, or given twice. |
| `usage` | `plan` | `--plan` with `--size`, in either order (architect, 2026-11-24, for v0.10-T01, pins P6 and P13). |
| `usage` | `board` | `--board` with `--plan` or with `--size`, in either order; `--epic` with `--plan` or with `--size` when `--board` is on the line; and `--epic` without `--board` (architect, 2026-11-25, for v0.11-T01, pin P4). |
| `usage` | `epic` | `--epic` with `--plan` or with `--size`, in either order, when `--board` is not on the line; and `--board` without `--epic` (architect, 2026-11-25, for v0.11-T01, pin P4). |
| `usage` | `turn` | `--turn` without `--plan` or `--epic` (architect, 2026-11-25, for v0.11-T01, pin P4). |
| `usage` | None | An unknown option, or any positional argument. |
| `invalid_value` | `project`, `platform`, `profile`, `plan`, `board`, `epic`, `turn`, `size` or `ttl` | The first bad value, in the order [Exit codes and errors](#exit-codes-and-errors) gives for `create`. For `plan`, a file that cannot be read, or that does not hold a plan as [Plan](#plan) defines it. For `board` and `epic`, a value outside its form, or a board or epic the board read refuses ([Board](#board), [Epic](#epic), [The board read](#the-board-read)). For `turn`, a value outside its form, or one that names no turn of the plan ([Turn](#turn)). |
| `out_of_range` | `ttl` | A TTL below 1 minute or above 24 hours. |
| `internal` | None | The board could not be read: the receipt or the credential could not be read, or the board did not answer, refused agentspace's credential, or gave an answer that could not be read ([The board read](#the-board-read)). No lease was recorded, and the lease records were not read. |
| `internal` | None | The credential could not be issued, agentspace's CA is not set up, the alias could not be set up, the lease's host could not be looked up or its making could not be started (from v0.07, [docs/driver.md](driver.md#where-it-places-a-lease)), or the lease records could not be read, written or locked. No lease was recorded. What a failed `create` removes, what it MAY keep in the OpenSSH configuration, and what its message MUST name when a removal fails are in [Concurrency and atomicity](#concurrency-and-atomicity). |

A plan whose lease's turn no size holds is not an error: its lease is `unsatisfiable`, and
`create` exits 0.

**Help:**

```
Usage: agentspace lease create --project <name> [--platform <os/arch>] [--profile <name>]
                               [--size <size> | --plan <file> | --board <id> --epic <ticket>]
                               [--turn <name>] [--ttl <duration>]

Requests a lease for one agent of a project, and prints it as JSON.
Check the lease's state before connecting: ready, pending or unsatisfiable.
A ready lease's host alias is in your OpenSSH configuration: ssh <host alias>.
A pending lease becomes ready or unsatisfiable; poll it with get.
An unsatisfiable lease lists its reasons. Nothing was issued, and the exit code is still 0.

Options:
  --project <name>      Required. Letters, digits and '-', up to 32; lowercased.
  --platform <os/arch>  Default linux/arm64. Also accepts aarch64 for arm64.
  --profile <name>      Default agent-linux. Accepts name or name@version.
  --size <size>         Default m. One of s, m, l or xl: at least 2, 4, 8 or 16 vCPUs,
                        with 4 GiB of memory for each.
  --plan <file>         Size the lease for a plan: a JSON file of turns, stories and
                        subagents. Picks the smallest size that holds its busiest turn.
                        Not with --size.
  --board <id>          Read the plan from an epic on this localswim board, by its UUID.
                        Needs --epic. Only reads the board. Not with --plan or --size.
  --epic <ticket>       The epic's ticket number, as in 52. Its description holds the
                        plan in one agentspace-plan block. Needs --board.
  --turn <name>         Size the lease for this turn of the plan, not its busiest.
                        Needs --plan or --epic.
  --ttl <duration>      Default 8h, or with a plan, its turn's budget and half again.
                        From 1m to 24h, never clamped.
                        Examples: 8h, 90m, 1h30m, '2 hours', PT8H.
                        A bare number is refused: give a unit.
  -h, --help            Print this help and exit

In v2 only linux/arm64, agent-linux and the sizes s, m, l and xl can be met.
Any other well-formed value gives an unsatisfiable lease, never a substitute.
So does a plan whose busiest turn, or the turn --turn names, no size holds.
```

**Two parts of this help changed in v0.09** (#0049; architect, 2026-11-23, for v0.09-T01, pin
P28): the `--size` row, which read `Default m.` alone, and the line that begins `In v0 only`,
which read `In v0 only linux/arm64, agent-linux and size m can be met.` Every other byte of this
help was as before v0.09.

**Four parts of this help changed in v0.10** (#0050; D75, D76): the usage line's second line,
which read `[--size <size>] [--ttl <duration>]`; the `--plan` row, which was new; the line that
began `In v1 only`, which began `In v0 only` and was otherwise as in v0.09; and the last line,
which was new and read `So does a plan whose busiest turn no size holds.` Every other byte of this
help was as in v0.09.

**Six parts of this help changed in v0.11** (#0052; D84 to D87): the usage line's second line,
which read `[--size <size> | --plan <file>] [--ttl <duration>]`, and its third line, which is new;
the `--board`, `--epic` and `--turn` rows, which are new; the `--ttl` row's first line, which read
`Default 8h. From 1m to 24h, never clamped.` and is now two lines; the line that begins
`In v2 only`, which began `In v1 only` and is otherwise as in v0.10; and the last line, which read
`So does a plan whose busiest turn no size holds.` Every other byte of this help is as in v0.10.
**The `--plan` row is one of those bytes**, so its `Picks the smallest size that holds its busiest
turn.` says what happens without `--turn`; with `--turn`, the lease is sized for the turn it
names, as the `--turn` row says ([Plan](#plan), [Turn](#turn)).

### agentspace lease get

```
agentspace lease get <id>
```

**Prints one lease as it stands now.** It changes no lease. A `ready` lease whose `expires_at` has
passed is printed as `expired`.
- **For a lease that is `pending`, `get` MAY let agentspace carry on placing it** (from agentspace's
  v0.07 release, #0047; D48): it asks the lease's driver to check it
  ([docs/driver.md](driver.md#check_pending)), which can start the work that makes its compute,
  or from v0.09 (#0049) changes its compute's size, and places it. That changes no lease and
  nothing `get` prints: the lease is printed as stored, `get` never waits for the work, and a
  check that fails changes neither the output nor the exit code. The
  lease becomes `ready` later, by agentspace, as [States](#states) says.

| Argument or flag | Type | Notes |
|---|---|---|
| `<id>` | [Lease ID](#lease-id) | Required. |
| `-h`, `--help` | | Print the help below and exit 0. |

**Request document** (never read by the v2 CLI; see [Schemas](#schemas)):
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
  used again. For a lease created with a plan, that is the TTL its turn gave, or the `--ttl` its
  `create` was given: `renew` never reads the plan, the board or a budget again ([TTL](#ttl);
  architect, 2026-11-25, for v0.11-T01, pin P20).
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

**Request document** (never read by the v2 CLI; see [Schemas](#schemas)):
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
| `ready` | First ends the lease's connections on the caller's machine; then deletes the alias, ends any connection that came back through it meanwhile, and deletes the key and certificate; drops `ssh`, keeps `expires_at`, and sets `released_at` | `released` |
| `expired` | The same | `released` |
| `pending` | Gives up the request, and sets `released_at`; the lease has no `expires_at`. It ends no connection, since a `pending` lease has no alias to connect through | `released` |
| `released` | Ends any connection of the lease that remains on the caller's machine, then deletes whatever is left of the lease's key, certificate and alias, looking for connections again once the alias is gone as for `ready`, and changes nothing else; prints the lease with its first `released_at` | `released` |
| `unsatisfiable` | Nothing; prints the lease unchanged | `unsatisfiable` |

**`release` ends the lease's connections on the caller's machine, before it removes the lease's
credential (D59; architect, 2026-11-21, for v0.08-T01).** The mechanism is
[docs/driver.md's release](driver.md#release-a-lease), and this page defines only what a caller
sees:
- **A lease's connections are every ssh client on the caller's machine that reaches the lease
  through its host alias**, whichever program started it, as the user that runs `release`, and the
  process that carries its traffic to the host for it. A client of another lease's alias, or of no
  lease's, MUST NOT be ended.
- **They are ended first**, before the alias, the key or the certificate is deleted, so that a
  release reaches every connection the alias had before its credential goes (architect,
  2026-11-21, for v0.08-T01, pin P23). A `release` that fails after ending them has still ended
  them: an ended connection is not restored, which is a change of a failed `release` that
  [Concurrency and atomicity](#concurrency-and-atomicity) does not undo.
- **They are looked for again once the alias is deleted, and any found is ended, and from v0.10
  again for as long as the search before ended one** (D59's change for v0.08-T01; architect,
  2026-11-21, for v0.08-T01, pin P23; D59, as changed for v0.10-T03, #0196). The reason for the
  second search is a window the first search alone would leave: a client that reconnects by
  itself, as the desktop app's can, can come back through the alias between its end and the
  alias's deletion, up to the first search's 10 seconds, while the credential still exists. The
  second search ends any client that came back that way. A further search ends a client that no
  search before it could reach, one below a program that has exited while the client above it
  still held that program. **When `release` returns, it MUST have left no process holding the
  alias that its searches can end**, as [docs/driver.md, Release a lease](driver.md#release-a-lease)
  defines it. Deleting the alias does not stop every client from coming back: the desktop app can
  start the alias's proxy from its own copy of the alias's ProxyCommand after the alias is gone,
  and a process started after the last search is outside what `release` claims (#0229). **From
  agentspace's v0.11 release (#0052), such a process gets no connection:** agentspace's
  ProxyCommand refuses the alias of a lease that is not `ready`, a `released` one among them, and
  refuses every argument of lease alias form while the lease state file exists and cannot be read
  or is not valid, opening no tunnel for either
  ([docs/operator.md, The ProxyCommand](operator.md#the-proxycommand);
  architect, 2026-11-25, for v0.11-T01, pins P21, P22 and P23). That refusal is defined there, and
  changes no document, field or error of this contract. Each search is bounded at 10 seconds of
  its own, a release makes at most six, and none starts after the second once 10 seconds have
  passed since the first
  ([docs/driver.md, The order of a release](driver.md#the-order-of-a-release)).
- **Ending a connection is best effort** (architect, 2026-11-21, for v0.08-T01): a connection that
  cannot be ended MUST NOT make `release` fail, change its document, or change its exit code, and
  `release` goes on to remove the credential. While agentspace's guard runs on the caller's
  machine, a connection missed costs at most the grace of running time, because compute is
  stopped whatever it holds ([A host's life follows its leases](#a-hosts-life-follows-its-leases),
  D59).
- **`release` MUST NOT make a call to the lease's host**, and works when the host is stopped. The
  host's end of each ended connection is dropped by the host on its own, within about 90 seconds
  on today's hosts (D59).
- **The number of connections ended is in no output** (architect, 2026-11-21, for v0.08-T01). It
  is logged, in `release.log` in the lease CLI's state folder, which
  [`AGENTSPACE_STATE_DIR`](#environment) moves ([docs/state.md](state.md#the-release-log)). What
  the log holds, and when it is written, are
  [docs/driver.md's](driver.md#where-the-count-is-logged). A release that cannot write it is not
  failed by it.
- **A repeated `release` ends what remains** (architect, 2026-11-21, for v0.08-T01): each `release`
  of a `ready`, `expired` or `released` lease looks for the lease's connections again, so a
  connection that appeared after an earlier one is ended by the next. A `pending` lease's release
  ends nothing, as the table says.

- **Release is idempotent:** releasing a lease again succeeds and prints the same lease, so a
  caller MAY retry it freely.
  - **The help's "Releasing a released or unsatisfiable lease again keeps its record" means that
    the record does not change** (architect, 2026-11-19, gap FB-03; reworded in v0.08-T01, since
    a second `release` now ends connections too). The table above binds: a second `release` of a
    `released` lease still deletes what a failed one left, and ends a connection of its alias that
    remains.
  - Note on where this came from: the prototype's `release` of a `released` lease touches no
    files. It never needs to, because it deletes the files before it records `released`; that
    order is #0041's defect, and the rule above is what lets an implementation record first.
- **Release does not revoke any certificate (D10).** A leaked copy of the current certificate works
  until `expires_at`, which a released lease still shows when it had one. A leaked copy of an older
  certificate works until its own expiry, which MAY be later if a `renew` shortened the lease.
  A connection made with a certificate is ended by `release` only when it is one of the lease's
  connections on the caller's machine, as above.
- **Release needs no CA,** and works on a machine whose CA is missing.
- **Ending connections changed none of contract v0's release document, schemas or reason codes**
  (v0.08), and neither v1 nor v2 changes any of them for it. `release` prints the same lease document as
  before, with the same fields, exit codes and errors, whether it ended no connection or many. Its
  effect is on the connections on the caller's machine alone, and was
  [not a new contract version](#versioning).
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

**Request document** (never read by the v2 CLI; see [Schemas](#schemas)):
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

**Conformance cases for E0.08-T04-S02 (architect, 2026-11-21, for v0.08-T01).** Each behavior
that `release` gained is a case, run in a throwaway home with a throwaway alias of the case's own
lease and a stand-in client of that alias that the case started. A case MUST NOT end, count, list
or read any other client on the machine it runs on. The cases are
[docs/driver.md's](driver.md#conformance-cases-for-v008), by their numbers there:
- **A `ready` lease's release ends its connection** (R1). The stand-in client is gone after the
  release, and the lease is `released` with the document of a release that ended nothing.
- **An `expired` lease's release does the same** (R14).
- **A client of another lease's alias is left running**, and so is one of no lease's (R4).
- **A `pending` lease's release ends no connection of a caller's**, and a stand-in client of
  another lease's alias is left running (R9).
- **A repeated release of a `released` lease ends a client of its alias that remains** (R8).
- **A client that comes back through the alias while the release ends the first is gone after the
  release** (R15; architect, 2026-11-21, for v0.08-T01, pins P23 and P26). The case assumes no
  timing: its stand-in starts exactly one new matching client once its first is ended, and the case
  asserts that no matching client is left and that the logged count is at least 2, never which
  walk ended the second client. The second walk itself is proved by E0.08-T03-S04 (#0180)'s crate
  test ([docs/driver.md, How it is checked](driver.md#how-it-is-checked)).
- **A connection that cannot be ended does not fail the release** (R7): the document and exit
  code are those of a release that ended nothing, and the credential is deleted. A connection that
  cannot be ended is counted once in the release's log line, though both of its searches find it
  (architect, 2026-11-21, for v0.08-T01, pin P26).
- **A release whose log cannot be written succeeds as usual** (R11).
- **A release with no connection to end changes nothing in the document or the exit code** (R12).
  Every document a case prints validates against
  [`lease.output.schema.json`](../schemas/lease.output.schema.json).
- **The help above is checked byte for byte**, as every help is ([Commands](#commands)).

**Help:**

```
Usage: agentspace lease release <id>

Gives a lease back, and prints it as JSON.
Ends this computer's ssh connections to the lease, then deletes the lease's local key,
certificate and host alias. A pending lease has no connections to end.
No certificate is revoked. Each stays valid until its own expiry.
An older certificate's expiry is later than the lease's if a renew shortened it.
Connections made from other computers are not ended.
Releasing a released or unsatisfiable lease again keeps its record and exits 0.

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

**Request document** (never read by the v2 CLI; see [Schemas](#schemas)):
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
| 1 | `internal` | Anything that is not the caller's fault, such as a credential that could not be issued, lease records that could not be read, or a board that did not answer. |
| 2 | `usage` | An unknown or missing command, subcommand, flag or argument, a flag given twice, a flag with no value or an empty value, `--plan` with `--size`, `--board` or `--epic` with `--plan` or `--size`, `--board` without `--epic` or `--epic` without `--board`, or `--turn` without `--plan` or `--epic` ([Command lines](#command-lines)). |
| 2 | `invalid_value` | A value outside its [accepted forms](#values), a plan that is not well formed among them ([Plan](#plan)), a board or epic that the board read refuses ([The board read](#the-board-read)), or a turn the plan does not have ([Turn](#turn)). |
| 2 | `out_of_range` | A well-formed value outside its allowed range; in v2, only the TTL. |
| 3 | `not_found` | No kept lease has the ID given. |
| 4 | `invalid_state` | The lease's state does not allow the subcommand; in v2, only `renew`. |

**An exit code not in the table MUST NOT be used.** Each non-zero exit code MUST come with one of
the error codes beside it, and every failure that fits none of the others is `internal`.

**When several errors apply, the first in this order MUST be reported, and only that one:**
1. **`usage`**, for anything wrong with the command line itself, chosen as
   [Command lines](#command-lines) says.
2. **`invalid_value` or `out_of_range`**, for the first bad value in the order of the command's
   argument and flag table, so `field` names the first bad one. **For `create`, in this order**
   (architect, 2026-11-25, for v0.11-T01, pin P20):
   1. **`project`, `platform` and `profile`**, each value's form.
   2. **`plan`: the `--plan` file is read and checked**, as in v1, by [Plan](#plan)'s checks 1 to
      11, in `plan`'s place in the table.
   3. **`board`, `epic` and `turn`, each value's form**: [Board](#board)'s, [Epic](#epic)'s and
      [Turn](#turn)'s. No board is read, and no plan is matched, for a value that fails its form.
   4. **`size`, then `ttl`**: each value's form, and the TTL's range, which gives `out_of_range`.
   5. **The board read, for `--board` and `--epic`:** `invalid_value` for `board` or for `epic`, or
      the board read's `internal`, as [The board read](#the-board-read) says. **The board read's
      `internal` is the one `internal` that step 2 reports**, since the board is read before the
      lease records are; it is never reported for a line with a bad value, and never after a
      `not_found` or anything that the lease records give.
   6. **`turn` against the plan**, from either source: `invalid_value` for `turn` when it names no
      turn of the plan.

   **All of it comes before step 3**, so a bad epic, a board that does not answer and a turn the
   plan lacks never touch the lease records, and `create` reads the board, as it reads a plan
   file, before it waits for any other command
   ([Concurrency and atomicity](#concurrency-and-atomicity)). **A command line with none of
   `--board`, `--epic` and `--turn` is checked in v1's order**, project, platform, profile, plan,
   size and TTL, since items 3, 5 and 6 then check nothing (D87).
3. **`internal`, when the lease records cannot be read or are busy**, and the no-home `internal`
   ([Messages](#messages)), which belongs here when the state folder is the default one under the
   home (architect, 2026-11-19, gap FB-22). **A command fails with no home only when it needs a
   path under the home folder, and at the first point it needs one:** with `AGENTSPACE_STATE_DIR`
   set, `get` and `list` need none and run, and `create`, `renew` and `release` read their records
   first, so a `not_found` or `invalid_state` comes before the no-home failure, and a `release` of
   a `pending` or `unsatisfiable` lease, which needs no file, succeeds
   ([docs/state.md](state.md#the-home-folder)).
4. **`not_found`**, from looking the lease up.
5. **`invalid_state`**, from the lease's state.
6. **Any other `internal`.**

So `renew <id of an expired lease> --ttl 48h` is `out_of_range` (exit 2),
`renew <unknown id> --ttl 8` is `invalid_value` for `ttl`, and `get <bad id> --ttl 8h` is `usage`.
`create --project x --epic 7 --board 0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f --ttl 8` is
`invalid_value` for `ttl`, and reads no board (architect, 2026-11-25, for v0.11-T01, pin P20).

**The error document** goes to stderr, compact, on one line. Its schema is
[`error.output.schema.json`](../schemas/error.output.schema.json).

| Field | Type | Present | Meaning |
|---|---|---|---|
| `code` | String | Always | One of the error codes above. |
| `message` | String | Always | One or more sentences for a person: what was wrong, and what to do. Never empty. |
| `field` | String | `invalid_value` and `out_of_range`; `usage` in the thirteen cases below | The flag or request member at fault, without dashes: `id`, `project`, `platform`, `profile`, `plan`, `board`, `epic`, `turn`, `size`, `ttl` or `state`. `plan` is new in v1 ([Changes from v0](#changes-from-v0)), and `board`, `epic` and `turn` are new in v2 ([Changes from v1](#changes-from-v1)). `out_of_range` names `ttl` alone. |
| `lease_id` | String, lowercase UUIDv4 | `not_found` and `invalid_state` | The normalized ID asked for. |
| `state` | String, [state](#states) | `invalid_state` | The state the lease is in. |

**A `usage` error MUST carry `field` in exactly these thirteen cases, and MUST NOT otherwise**
(the first six as in v1; the last seven from v2, architect, 2026-11-25, for v0.11-T01, pin P4):
1. **A flag with no value**, naming that flag, `board`, `epic` and `turn` among them.
2. **A flag with an empty value**, naming that flag.
3. **A flag given twice**, naming that flag.
4. **A missing lease ID:** `id`.
5. **A missing `--project`:** `project`.
6. **`--plan` with `--size`**, in either order: `plan` (architect, 2026-11-24, for v0.10-T01,
   pins P6 and P13).
7. **`--board` with `--plan`**, in either order: `board`.
8. **`--board` with `--size`**, in either order: `board`.
9. **`--epic` with `--plan`**, in either order: `board` when `--board` is on the line, and
   otherwise `epic`.
10. **`--epic` with `--size`**, in either order: `board` when `--board` is on the line, and
    otherwise `epic`.
11. **`--board` without `--epic`:** `epic`, the flag that is missing.
12. **`--epic` without `--board`:** `board`, the flag that is missing.
13. **`--turn` without `--plan` or `--epic`:** `turn`.

The error schema checks none of the thirteen, so conformance does, each by its own case
([Schemas](#schemas)).

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

**The original error is not kept in contract v2, as in v0 and v1.** A build MAY drop it, as the
prototype does, and MUST NOT put it in the error document, on stdout, or in any file
[docs/state.md](state.md) lists. A place to keep it for diagnosis, such as a log, is later and not
blocking. A message such as `agentspace lease create failed: The lease's credential could not be
issued. Nothing was recorded; try again.` is the shape meant.

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
| `usage` | `--plan` with `--size` (`create`), in either order | `Options --plan and --size cannot be given together; the plan picks the size. Run 'agentspace lease create --help'.` (architect, 2026-11-24, for v0.10-T01, pin P6) |
| `usage` | `--board` with `--plan` (`create`), in either order | `Options --board and --plan cannot be given together; the epic carries the plan. Run 'agentspace lease create --help'.` (architect, 2026-11-25, for v0.11-T01, pin P4) |
| `usage` | `--board` with `--size` (`create`), in either order | `Options --board and --size cannot be given together; the epic's plan picks the size. Run 'agentspace lease create --help'.` (architect, 2026-11-25, for v0.11-T01, pin P4) |
| `usage` | `--epic` with `--plan` (`create`), in either order | `Options --epic and --plan cannot be given together; the epic carries the plan. Run 'agentspace lease create --help'.` (architect, 2026-11-25, for v0.11-T01, pin P4) |
| `usage` | `--epic` with `--size` (`create`), in either order | `Options --epic and --size cannot be given together; the epic's plan picks the size. Run 'agentspace lease create --help'.` (architect, 2026-11-25, for v0.11-T01, pin P4) |
| `usage` | Unexpected argument | `Unexpected argument '<value>' for agentspace lease <sub>. Run 'agentspace lease <sub> --help'.` |
| `usage` | No lease ID | `No lease ID given. Run 'agentspace lease <sub> --help'.` |
| `usage` | No `--project` | `Option --project is required. Run 'agentspace lease create --help'.` |
| `usage` | `--board` without `--epic` (`create`) | `Option --board needs --epic, the epic's ticket number. Run 'agentspace lease create --help'.` (architect, 2026-11-25, for v0.11-T01, pin P4) |
| `usage` | `--epic` without `--board` (`create`) | `Option --epic needs --board, the board's ID. Run 'agentspace lease create --help'.` (architect, 2026-11-25, for v0.11-T01, pin P4) |
| `usage` | `--turn` without `--plan` or `--epic` (`create`) | `Option --turn needs a plan, from --plan or --epic. Run 'agentspace lease create --help'.` (architect, 2026-11-25, for v0.11-T01, pin P4) |
| `invalid_value` | `id` | `Lease ID '<value>' is not a UUIDv4. Give the id that create printed.` |
| `invalid_value` | `project` | `Project '<value>' is not a project name. Use 1 to 32 letters, digits and '-', starting and ending with a letter or digit.` |
| `invalid_value` | `platform` | `Platform '<value>' is not an os/arch pair. Give one as in 'linux/arm64'.` |
| `invalid_value` | `profile` | `Profile '<value>' is not a profile name. Give name or name@version, as in 'agent-linux'; a version is 1 or more.` |
| `invalid_value` | `plan` | `Plan '<value>' is not a valid plan: <why>.`, where `<value>` is the `--plan` argument as given, never made a full path, and `<why>` is one of those [Plan](#plan) lists, word for word (architect, 2026-11-24, for v0.10-T01, pins P7 and P12) |
| `invalid_value` | `board`, not a board ID | `Board '<value>' is not a board ID. Give the board's UUID, as in '0a1b2c3d-...'.` (architect, 2026-11-25, for v0.11-T01, pin P5) |
| `invalid_value` | `epic`, not a ticket number | `Epic '<value>' is not a ticket number. Give the epic's ticket as digits, as in '52'.` (architect, 2026-11-25, for v0.11-T01, pin P5) |
| `invalid_value` | `board` or `epic`, refused by the board read | As [The board read](#the-board-read)'s table gives each, and for a description, `Epic '<value>' carries no usable plan: <why>.`, `<why>` as [The plan block](#the-plan-block) gives it (D83, D86; architect, 2026-11-25, for v0.11-T01, pins P11 and P13) |
| `invalid_value` | `turn`, outside a turn name's form | `Turn '<value>' is not a turn name. Give the plan's turn name, as in 'T01'.` (architect, 2026-11-25, for v0.11-T01, pin P12) |
| `invalid_value` | `turn`, naming no turn of the plan | `Turn '<value>' is not a turn of the plan; its turns are <turns>.`, where `<turns>` is every turn's name, in the plan's order, each in single quotes and quoted raw, as the plan writes it, separated by `, `, as in `'T01', 'T02', 'T03'` (architect, 2026-11-25, for v0.11-T01, pin P5) |
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
| `internal` | A relative home or state or ssh folder, and the working folder cannot be read | `agentspace lease <sub> failed: The working folder cannot be read, so a relative folder in the environment cannot be made a full path. Nothing was changed.`, only when such a folder is relative and the command needs it ([docs/state.md](state.md#the-home-folder)) (architect, 2026-11-19, for E0.05-T03-S02) |
| `internal` | Records cannot be locked | `The lease records could not be locked: <why>. Nothing was changed.`, for a failure to open the lock that is not another command holding it ([docs/state.md](state.md#the-lease-state-lock)) |
| `internal` | No driver (`create`) | `agentspace lease create failed: No lease driver is available. Nothing was recorded.` |
| `internal` | The board read (`create`) | As [The board read](#the-board-read)'s table gives each, in agentspace's own words (D86, #0041) |
| `internal` | Unknown stored driver (`renew`, `release`) | `agentspace lease <sub> failed: The lease was placed by a driver this agentspace does not have. Nothing was changed.` It never names the driver ([docs/driver.md](driver.md#the-drivers-name)) |
| `internal` | Alias file is not this lease's (`release`) | `agentspace lease release failed: The lease's host alias file belongs to another lease or to no lease, so nothing was deleted. A person has to move that file.` ([docs/state.md](state.md#release-and-what-it-leaves)) |
| `internal` | Credential or alias not deleted (`release`) | `agentspace lease release failed: The lease's local key, certificate or host alias could not be deleted: <why>. Run release again to finish.` |
| `internal` | Records unwritable after a ready place (`create`), its undo leaving nothing | `agentspace lease create failed: The lease records could not be written: <why>. No lease was recorded.`, never "Nothing was changed", since the `Include` line, a backup and `agentspace.d` MAY stay ([docs/state.md](state.md#creation-in-order)) (architect, 2026-11-19, for E0.05-T03-S02) |
| `internal` | Records unwritable after a ready place (`create`), its undo leaving files | `agentspace lease create failed: The lease records could not be written: <why>. No lease was recorded, and these could not be removed: <leftover>, <leftover>.`, each leftover named as [Concurrency and atomicity](#concurrency-and-atomicity) says; when the undo failed and named none, the last sentence is `No lease was recorded, but some of the lease's files could not be removed.` (architect, 2026-11-19, for E0.05-T03-S02) |
| `internal` | Records unwritable (`renew`), and the previous certificate not put back | `agentspace lease renew failed: The lease records could not be written: <why>. Its previous certificate could not be put back; renew the lease again.` (architect, 2026-11-19, for E0.05-T03-S02) |
| `internal` | `AGENTSPACE_PROXY_COMMAND` not valid text (`create`) | `agentspace lease create failed: AGENTSPACE_PROXY_COMMAND is not valid text. Nothing was recorded.`, before any driver is asked. A `SystemRoot` that is not valid text is read as unset ([docs/state.md](state.md#the-alias-file)) (architect, 2026-11-19, for E0.05-T03-S02) |
| `internal` | Anything else | `agentspace lease <sub> failed: <what failed>`, where `<what failed>` meets the rules above. Before a subcommand is read, it is `agentspace lease failed: <what failed>`, and a message that would otherwise be empty is `agentspace lease failed.` |

## Environment

**These environment variables change where the CLI reads and writes.** Each is read when a command
starts, and one that is unset or empty means the default.

| Variable | When set and not empty |
|---|---|
| `AGENTSPACE_STATE_DIR` | Every command keeps its lease records in this folder instead of the default ([docs/state.md](state.md)). |
| `AGENTSPACE_SSH_DIR` | `create` makes the lease's alias usable from the OpenSSH client folder named here, instead of the calling user's own, and `release` removes it from there ([docs/state.md](state.md)). |
| `AGENTSPACE_PROXY_COMMAND` | `create` gives the alias this command as its way to reach the lease's host, instead of agentspace's own ([docs/driver.md](driver.md)). |
| `AGENTSPACE_PROVISION_CMD` | From agentspace's v0.07 release (#0047): the full path of the program that `create` and `get` start, instead of agentspace's own, when a lease's compute has to be made, from v0.09 (#0049) when its size has to be changed, or when a `pending` lease has to be placed ([docs/driver.md](driver.md#starting-a-provisioning-run)). It changes no output. |

- **They exist so that a check, or a worktree, can run the CLI without touching the caller's real
  leases or OpenSSH configuration.** A caller in normal use MUST NOT set them.
- **A check whose `create` or `get` could reach `pending` MUST set `AGENTSPACE_PROVISION_CMD` to a
  stand-in**, so that it never starts the real work ([docs/driver.md](driver.md#conformance-cases-for-v007)).
  From v0.09 (#0049), a `create` for a project whose compute exists can reach `pending` too: while
  another lease of the project is `pending`, when that compute's size is not yet recorded, and when
  it is not the size asked for and no other lease of the project is `ready`
  ([A host's life follows its leases](#a-hosts-life-follows-its-leases); D66, D67).
- **A check that sets `AGENTSPACE_SSH_DIR` MUST keep the same value from `create` to `release`**,
  since `release` removes the alias from the folder named when it runs. `renew` does not touch the
  alias, and reads neither `AGENTSPACE_SSH_DIR` nor `AGENTSPACE_PROXY_COMMAND`.
- **A lease that is `pending` gets its alias later, in the OpenSSH client folder and with the
  transport of the `create` or `get` that started the work placing it**
  ([docs/driver.md](driver.md#starting-a-provisioning-run)). So a check MUST keep
  `AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND` the same across a
  `pending` lease's `create`, every `get` of it and its `release`.
- **`LOCALAPPDATA` is the caller's system's, not agentspace's, and from v2 `create --board` finds
  localswim's service receipt under it**, at localswim's documented path,
  `<LOCALAPPDATA>\localswim-v2\service.json` ([The board read](#the-board-read); architect,
  2026-11-25, for v0.11-T01, pin P16). Unset, empty or only whitespace, a `create --board` fails
  with `internal`; no other command reads it. **No `AGENTSPACE_*` variable names the receipt**
  (architect, 2026-11-25, for v0.11-T01, pin P9).
- **A check that runs `create --board` MUST point `LOCALAPPDATA` into a throwaway home of its
  own**, holding a stand-in receipt whose `base` is a stand-in board's and whose
  `credential_directory` holds a stand-in `bot.json`, so that it never reads the caller's real
  receipt, Bot credential or board (D86; architect, 2026-11-25, for v0.11-T01, pin P16). The conformance harness already points
  `LOCALAPPDATA` into its throwaway home ([conformance.md](conformance.md)).
- **No other environment variable changes the contract's behavior, beyond `LOCALAPPDATA` for the
  board read** (architect, 2026-11-25, for v0.11-T01, pin P16). `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY`, in any case,
  change nothing: the board read never goes through a proxy ([The board read](#the-board-read)).
  Variables that the caller's own system uses to name a home folder are
  [docs/state.md](state.md)'s.

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
| `plan.input.schema.json` | A plan | The file `--plan` names, an epic's plan block, and the `plan` of a `create` request ([Plan](#plan)) |

- **Output schemas are strict:** `additionalProperties` is false everywhere, every field that is
  always present is required, each state and each error code requires and forbids its own fields,
  and IDs and times have patterns. `format` is given as well, but only as an annotation, since
  validators need not enforce it.
  - **A message, in an error or in a reason, MUST NOT be empty or start with a space**, and holds
    no control character.
  - **A lease's `reasons` holds 1 to 4 items, each code at most once.** Their order is in
    [Fields](#fields); a schema cannot check it.
  - **The lease and list schemas MUST refuse a `plan_too_large` reason that is not alone**, and a
    lease that has one without `sized_for` or with a `size` other than `xl` (D81; architect,
    2026-11-24, for v0.10-T01, pin P15). Conformance MUST check the same rule against what the
    CLI prints.
  - **The error schema does not check which `usage` errors carry `field`.** It allows `field` on
    any `usage` error and requires it on none, because the thirteen cases that MUST carry it
    ([Exit codes and errors](#exit-codes-and-errors)) differ from the others only in what went
    wrong, which the document does not say in any member a schema can test. A conformance check
    MUST test that rule itself, case by case.
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
- **The plan's schema, `plan.input.schema.json`, is strict in form as well as in meaning:** a
  plan's names, models and efforts are matched as written, and nothing in a plan is trimmed or
  case-folded ([Plan](#plan); architect, 2026-11-24, for v0.10-T01, pins P2 and P3). Like every
  input schema, it allows `null` for an optional member to mean "not given" (architect,
  2026-11-24, for v0.10-T01, pin P21). Passing it never makes a plan acceptable: what a schema
  cannot see, such as a cycle in `after`, an `after` naming no subagent of its story, two names
  alike where [Plan](#plan) forbids it, and a member name given twice, is found once the plan is
  parsed, and refused with `invalid_value` for `plan`. A file's leading byte order mark is the file's, and is
  not in the document the schema sees. **A turn's `budget_minutes` is an `integer` from 1 to 960,
  or `null`** (D85), and a turn MAY leave it out, so every plan v1's schema accepted is accepted.
  **JSON Schema's `integer` accepts a whole number in any form, so the schema accepts
  `budget_minutes` written as `90.0` or `9e1`, and the program refuses it**, since it MUST be
  written as digits alone ([Plan](#plan); architect, 2026-11-25, for v0.11-T01, pin P24). The
  schema's own description lists it among the checks it cannot make.
- **The `create` request schema, `lease-create.input.schema.json`, refuses what the command line
  refuses as a `usage` error among `plan`, `board`, `epic`, `turn` and `size`** (architect,
  2026-11-25, for v0.11-T01, pin P6): a `board` without an `epic` or the reverse, a `board` and
  `epic` beside a `plan` or a `size`, a `plan` beside a `size`, and a `turn` with neither a `plan`
  nor a `board`, each counting `null` as not given. `turn` is matched exactly, with no whitespace
  allowed around it, as [Turn](#turn) is never trimmed; `board` and `epic` allow surrounding
  whitespace, as the values they stand for do.
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
- **The v2 CLI takes flags and arguments only, and never reads a JSON request.** The JSON it reads
  is a plan, from the file `create --plan` names or from an epic's `agentspace-plan` block, and
  localswim's receipt, credential and answer for that epic ([The board read](#the-board-read)),
  none of which is a request ([Plan](#plan)). An input document defines a request: its members are the flags and arguments of
  the same names, except that a request's `plan` holds the plan itself where `--plan` names a
  file, and no request member carries a path (architect, 2026-11-24, for v0.10-T01, pin P19). It
  is what a request is validated against, and a hosted API (card #0011) MUST accept it as JSON. In
  JSON, insignificant whitespace is allowed anywhere.
- **Each schema is self-contained**, with shared parts copied into its own `$defs`, because
  PowerShell's `Test-Json -SchemaFile` does not resolve a `$ref` to another file. The lease
  definition in the list schema's `$defs` MUST stay identical to the lease schema's. **The plan in
  `lease-create.input.schema.json`'s `$defs` is a copy of `plan.input.schema.json`'s definition of
  a plan, and MUST stay identical to it**, so the two accept exactly the same plans (D76;
  architect, 2026-11-24, for v0.10-T01, pins P8 and P19).
- **Each schema has samples in [`schemas/samples/`](../schemas/samples)**, named
  `<schema name>.<case>.json`, which MUST validate, `plan.input.schema.json` among them. Documents
  that MUST NOT validate are in `schemas/samples/invalid/` under the same naming, at least one per
  output schema. A check MUST validate both with a real draft 2020-12 validator and fail on any
  mismatch.
  - Note on where this came from: the prototype's check is `scripts/test-schemas.ps1`, which uses
    PowerShell's `Test-Json`; on 2026-11-18 the samples were also checked with Python's
    `jsonschema` 4.19.
- **A schema's `$id` names it and is not a download address.** Each top-level schema's `$id`
  carries the contract version, `v2`:
  `https://github.com/SixbucksSolutions/agentspace/schemas/v2/<file name>`, and its title says
  contract v2 (D87, D89; as v1's did from v0.10, architect, 2026-11-24, for v0.10-T01, pin P9).
- **Contract v1's schemas and samples are kept under `schemas/v1/`** (D87, D89): its nine schemas,
  and their samples under `schemas/v1/samples/`, the invalid ones under
  `schemas/v1/samples/invalid/`, each byte for byte as in v1, `$id`s and titles unchanged. They are
  not v2's schemas, and MUST NOT change.
  - **They show what v2 keeps of v1** ([Versioning](#versioning)): every document that a command
    line with none of `--board`, `--epic` and `--turn`, and with no `budget_minutes` member in its
    plan, prints, error documents included, MUST validate against its v1 schema as well as its v2 one,
    and v2's conformance MUST check it (D87).
  - **They also show where v2 parts from v1:** v2's conformance MUST show that an error naming
    `board`, `epic` or `turn` validates against v2's error schema and fails v1's, so a validator
    that accepted everything could not pass the check above.
- **Contract v0's schemas and samples are kept under `schemas/v0/`** (D76; architect, 2026-11-24,
  for v0.10-T01, pin P9): its eight schemas, and their samples under `schemas/v0/samples/`, the
  invalid ones under `schemas/v0/samples/invalid/`, each byte for byte as in v0, `$id`s and titles
  unchanged. They are not v1's or v2's schemas, and MUST NOT change.
  - **They show what v2 keeps of v0:** every document that a command line with no `--plan` and none
    of `--board`, `--epic` and `--turn` prints, error documents included, MUST validate against
    its v0 schema as well as its v1 and v2 ones, and v2's conformance MUST check it (D76, D80,
    D87).
- **A check of `schemas/` reads `schemas/v0/` and `schemas/v1/` each as its own set**, each schema
  against its own samples, and never mixes two versions' schemas and samples.
- **The lease state file's schema, `schemas/state/leases.state.schema.json`, is not part of this
  contract.** It is [docs/state.md](state.md)'s.

## Versioning

**This is contract v2, which agentspace's v0.11 release (#0052) brings (D87).** Contract v0 was the
MVP's, and stood until v0.10, the first release that changed an output; contract v1 was v0.10's
(D76). The contract's version and agentspace's release are two numbers, kept apart (D26). Every
change v2 makes is listed in [Changes from v1](#changes-from-v1), and every change v1 made in
[Changes from v0](#changes-from-v0).

**What makes a new version:**
- **Any change after which an output could fail to validate against the schema of the version
  before it is a new contract version**, and so is a change to an output's meaning. Because output
  schemas forbid unknown fields, a new output field is a new version.
- **Accepting an input that the version before refused is not a new version.** Refusing one that it
  accepted is.
- **Supporting a platform, profile or size that the contract answered `unsatisfiable` is not a new
  version.** The fields have been in the contract from v0 so that it is not (D5).
- **Changing a message's wording is not a new version**, since no caller parses one.
- **Changing help text is not a new version either.** Help is for people, and a caller MUST NOT
  parse it. This page and the CLI still change together ([Commands](#commands)).
- **A profile with no version names its current promise ([Profiles](#profiles)), which MAY move to
  a newer distro release without a new contract version.** Its libc family MUST NOT change, and
  its libc minimum version MUST NOT go down.
- **agentspace's per-agent footprint is not part of the contract** (D73, D76). A new figure
  changes which size a plan gets ([Plan](#plan)), never what a field means, and is not a new
  version. So a plan answered `plan_too_large` today MAY get a size later, with no new version.
- **agentspace's TTL factor is not part of the contract** (D85), as its footprint is not. A new
  factor changes which TTL a plan gets ([TTL](#ttl)), never what a field means, and is not a new
  version, so long as every TTL a plan gives stays inside the TTL's range.

**What made v2** (D87):
- **`board`, `epic` and `turn`, new values of `field` in error documents**, of `invalid_value` and
  of `usage` ([Exit codes and errors](#exit-codes-and-errors)). v1's error schema holds `field` to
  an enum of eight, so an error naming one of them fails it.
- **The seven `usage` cases that carry one of them** (architect, 2026-11-25, for v0.11-T01, pin
  P4). They are new through their `field`, and which `usage` errors carry `field` is a rule no
  schema can check, so conformance checks each of the thirteen ([Schemas](#schemas)).

**What is not a change** (D87):
- **`--board`, `--epic`, `--turn`, `budget_minutes`, and the request document's `board`, `epic`
  and `turn` are inputs that v1 refused**, which alone would not make a new version.
- **No lease output changes.** `sized_for` keeps its three members, and a board lease prints the
  plan's own `name` as `sized_for.plan`, as D75 foresaw, never the board or the ticket. A lease's
  TTL from a budget is a `ttl_seconds` in v1's range. **So every lease and list document v2 prints
  validates against v1's schemas**, whatever made the lease.
- **`sized_for.turn` names the turn `--turn` named, for a lease made with it** (D84). That lease
  could not be made in v1, and for every lease made without `--turn` the member means what it
  meant in v1.
- **Reporting every new error under `plan`, to stay v1, was not taken** (D87): a caller acts on
  `field`, and `plan` for a bad `--board` would send it to fix a flag it never gave.

**What v2 keeps of v1** (D87):
- **A command line with none of `--board`, `--epic` and `--turn`, in any case and in either form,
  and whose plan, when `--plan` is given, has no `budget_minutes` member, not even a `null` one,
  MUST print the JSON document v1
  printed for it, and MUST fail with the errors v1 failed with:** the same members, meaning the
  same, and the same exit code, error code and `field`, its values checked in v1's order
  ([Exit codes and errors](#exit-codes-and-errors)). v2 changes no message that such a command
  prints. Its help is text, not a document, and `create`'s help changed in v2
  ([`create`](#agentspace-lease-create)).
  - **So every document such a command prints validates against v1's schema in `schemas/v1/` as
    well as against v2's, and one with no `--plan` either against v0's in `schemas/v0/` too**, and
    v2's conformance MUST show both ([Schemas](#schemas); D76, D87).
  - **A plan with a `budget_minutes` member is the one input outside the new flags that v2 answers
    differently:** v1 refused it with `invalid_value` for `plan`, a `null` one included, since v1's
    plan schema allows no other member, and v2 accepts it. That is
    accepting an input the version before refused, which is not a new version.
- **A v1 client that reads leases fails no schema in v2**: every lease document validates against
  v1's lease and list schemas. **A v1 client fails v1's error schema only on an error naming
  `board`, `epic` or `turn`**, which only a command line with one of the new flags prints. **A
  client that gives the new flags MUST validate errors against v2's schema.** When v2 was made,
  every caller was agentspace's own: its conformance suite, the architect's live checks, and a
  person at the command line (D76, D87), so no caller outside agentspace's repository had to move
  with v2.
- **A lease made from an epic has `sized_for`, as a `--plan` lease has**, so a v0 client that reads
  one can fail its v0 schema, as for a `--plan` lease (D76).
- **Output schemas stay strict** (D76, D87). No rule tells a client to ignore an unknown member, so
  a later new output member is a new version again.

**v1's history, kept as v1's:** "What made v1" and "What v1 keeps of v0", word for word as they
stood in contract v1.

**What made v1** (D76, as D81 changed it):
- **`sized_for`, a new output member**, on every lease made with `--plan` ([Fields](#fields)).
- **`plan_too_large`, a new reason code** ([Fields](#fields)).
- **`plan`, a new `field` in error documents**, of `invalid_value` and of `usage`
  ([Exit codes and errors](#exit-codes-and-errors)).

Each could fail a v0 schema. **No lease lacks `size`**, since a `plan_too_large` lease's is `xl`
(D81), so that is not among them. `--plan`, the plan document and the request document's `plan`
are inputs that v0 refused, which alone would not make a new version.

**What v1 keeps of v0** (D76):
- **A command line with no `--plan`, in any case and in either form, whose output holds no lease
  made with `--plan`, MUST print the JSON document v0 printed for it, and MUST fail with the
  errors v0 failed with:** the same members, meaning the same, and the same exit code, error code
  and `field`. v1 changes no message that such a command prints. Its help is text, not a
  document, and `create`'s help changed in v1 ([`create`](#agentspace-lease-create)).
  - **So every document such a command prints validates against v0's schema in `schemas/v0/` as
    well as against v1's**, and v1's conformance MUST show it ([Schemas](#schemas); D76, D80).
- **A v0 client that reads a lease made with `--plan` can fail its v0 schema** (D76): `get`,
  `renew`, `release` and `list` print such a lease with `sized_for`, and one that no size holds with
  `plan_too_large`. **A client that reads such leases MUST validate against v1's schemas.** When v1
  was made, every caller was agentspace's own: its conformance suite, its architect's live checks,
  and a person at the command line. No other project's repository ran `agentspace lease`, so no
  caller outside agentspace's repository had to move with v1 (D76).
- **Output schemas stay strict** (D76). No rule tells a client to ignore an unknown member, so a
  later new output member is a new version again, as v1's are.

**v0's history, kept as v0's:**
- **Supporting the sizes `s`, `l` and `xl` was not a new version** (v0.09, #0049; D64 to D70,
  "What none of them changes"). v0 answered each `unsatisfiable`, as the rule above allows, and
  `no_capacity` was already one of v0's reasons, so its new use and its new message changed no
  document, schema, field, state, exit code, error or reason code. A lease served by larger
  compute than its size prints the size it asked for (D66), and nothing in a lease names a machine
  type (D1).
- **`release` ending the lease's connections on the caller's machine was not a new version**
  (v0.08, D59; architect, 2026-11-21, for v0.08-T01): no document, schema, field, state, exit
  code, error or reason code changed, and a caller never parsed what `release` left running.

## Hardening carried in

**Every point of the lease CLI's hardening and defect cards, and where this contract meets it
(D28).** Each card is OBE once the Rust CLI (v0.05, #0054) meets every requirement below. The
operator's cards, #0027 and #0034, are [docs/operator.md](operator.md)'s.

**The last column of each table is what v0.05's Rust build (#0054) shows, as of E0.05-T04-S01
(2026-11-19, [the run record](runs/e0.05-t04-s01.md)).** A point is marked yes only where the
conformance file it names checks the point's MUST, and "Not shown" says that no file does. The
run record says, point by point, what the suite shows and what it does not. Which cards go OBE is
Terry's call, and nothing here closes one.

**#0029, harden lease credential issuing:**

| Point | Carried in as | Met by v0.05's Rust build |
|---|---|---|
| 1. Clock skew: validity starts at the caller's now | The certificate's valid-after SHOULD be 5 minutes before its issue time, and `expires_at` stays the issue time plus the TTL (D24; [The credential](#the-credential)). | Not shown: no conformance file checks it (#0110) |
| 2. The lease ID check lets a trailing line feed and a trailing dot through | Every check matches the whole value ([Input tolerance](#input-tolerance)), and everything derived from a lease ID comes from its normalized form, which never ends in a dot ([Lease ID](#lease-id)). | Yes: `test-id-whole-value.ps1` and `test-unmet-id-line-feed.ps1` (E0.06-T04-S03): a trailing dot, and a line feed inside the ID, are refused with `invalid_value` for `id`, and a line feed before or after it is trimmed. 26 of 26 and 14 of 14 results pass against the Rust build. |
| 3. The first lease on a fresh laptop creates a CA no host trusts | `create` and `renew` MUST NOT create the CA, and fail with `internal` when there is none ([The credential](#the-credential)). #0041 dropped its own copy of this point under D18, which was MVP only and is replaced by D28. | Yes: `test-unmet-no-ca.ps1`: `create` and `renew` fail with `internal`, change nothing and make no CA. |
| 4. A failed signing leaves the key behind | A failed `create` MUST remove every file it made, and never blocks a later `create`. A removal that fails is named in the error ([Concurrency and atomicity](#concurrency-and-atomicity)). | Yes: `test-unmet-failed-create-cleanup.ps1`: a failed `create` removes what it made, and names a leftover in its home-relative form. `test-lease-driver-outcomes.ps1` also holds a failed alias step, which leaves nothing and does not block a later `create`. |
| 5. PowerShell 7.0 to 7.2 drop an empty argument | Declined as a requirement on a runtime: the rewrite is Rust (D28), and the prototype's real minimum is PowerShell 7.5 anyway (D8). What it protects is required instead: the lease's key has no passphrase ([The credential](#the-credential)), and an empty argument reaches the parser as one ([The program](#the-program)). | Yes: `test-lease-certificate.ps1`: the key loads with an empty passphrase. `test-values.ps1`: `get ''` is `invalid_value`, so an empty argument reaches the parser. |
| 6. Every certificate has serial 0 | Declined for v0: a serial is not part of this contract, and is decided with revocation, card #0010 ([The credential](#the-credential)). | Not applicable: declined for v0, so no file holds it. |
| 7. Test gaps: the start time is not checked against the test's clock, and a UTC and local mix-up is unseen on a UTC machine | The conformance suite MUST check against its own clock, and run once in a zone other than UTC ([Times](#times)). | Yes: `test-create-get.ps1` and `test-lease-certificate.ps1`, run in a zone other than UTC (Pacific Standard Time in the run record), where neither skipped. |

**#0041, wrap credential errors, require driver, order writes:**

| Point | Carried in as | Met by v0.05's Rust build |
|---|---|---|
| 1. Credential errors reach `internal` unwrapped, with laptop paths, the user name and CA-check output | An `internal` message carries no absolute path, user name or other program's output ([Exit codes and errors](#exit-codes-and-errors)). | Yes: `test-unmet-internal-no-path.ps1`: an `internal` message holds no absolute path, home folder, user name or other program's text. |
| 2. A `ready` record without its driver can be neither renewed nor released | A `ready` lease MUST be renewable, and one ever `ready` releasable, for as long as it is kept ([`renew`](#agentspace-lease-renew)). How the record keeps what that needs is [docs/state.md](state.md)'s, so its schema is left to that story (E0.04-T02-S01). | Yes: `test-unmet-record-no-driver.ps1`: `renew` and `release` succeed on it, and neither adds the member. |
| 3. Release deletes files before writing the record, and renew re-signs before writing | A failed `renew` leaves the lease's expiry, TTL and certificate as they were, and a failed `release` never leaves a `ready` or `expired` lease without its credential; a retried `release` deletes whatever a failed one left ([Concurrency and atomicity](#concurrency-and-atomicity)). | Yes: `test-unmet-renew-keeps-certificate.ps1` (a failed `renew` keeps the certificate byte for byte), `test-unmet-release-partial.ps1` (a failed `release` leaves no `ready` lease without its credential, and a retry finishes it) and `test-unmet-release-released.ps1` (a `release` of a `released` lease deletes what is left). |

**#0083, arguments refused or misread before parsing:**

| Point | Carried in as | Met by v0.05's Rust build |
|---|---|---|
| 1. An argument the runtime refuses before the CLI starts (`-:x`), or a runtime version check that fails, exits 1 with no error document | Every run that fails ends with one error document and an exit code from the table ([The program](#the-program)). | Yes: `test-usage-errors.ps1`: every failing run ends with one error document and its exit code, and `test-unmet-runtime-refused-argument.ps1` (E0.06-T04-S03): an argument such as `-:x`, in each position and as a flag's value, ends in one error document with an exit code from the table, 16 of 16 results against the Rust build. The Rust program has no runtime in front of it to refuse one ([conformance.md, The gaps](conformance.md#the-gaps)). |
| 2. A line feed inside a `--flag=value` value makes it an unknown option, while the separate form gives `invalid_value` | The two forms give the same result for any value that is not a help request ([Command lines](#command-lines)). | Yes: `test-unmet-argument-line-feed.ps1`: `--project <value>` and `--project=<value>` both give `invalid_value`. |

## Open questions

**What reading the prototype and its docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080), every question here is either closed or marked "Later, not blocking", and
none blocks building a drop-in of contract v0. v1's changes ([Changes from v0](#changes-from-v0))
add no question here, so none blocks a drop-in of contract v1 either, and neither do v2's
([Changes from v1](#changes-from-v1)), so none blocks a drop-in of contract v2.

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
7. **Later, not blocking:** v2's alias, as v0's and v1's did, carries the forwards as
   [docs/state.md](state.md#the-alias-file) gives them, and a caller MUST NOT rely on them
   ([The credential](#the-credential)), so moving them to the keeper (#0008) changes nothing a
   caller sees.
   **Whether the board's forwards belong in the lease's alias at all** (D12, D30, card #0008), or
   should move to the keeper's own connection. Today a caller's second session on a host warns
   that its remote port forwarding failed.
8. **Later, not blocking:** every lease in v2, as in v0 and v1, logs in as `ubuntu` (D3), which the
   pattern accepts, and a user it would refuse comes only with a later driver's host (v0.12,
   #0051).
   **Whether `ssh.user`'s pattern fits every host a later driver serves** (D4, v0.12, #0051). It
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

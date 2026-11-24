# The agentspace operator, reference v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for the operator: the laptop-side commands that manage agentspace's hosts
(`deploy`, `up`, `sync`, `bootstrap`, `provision`, `ca`, `down`, `status`, `grow`, `guard`), the
SSH ProxyCommand, and the localswim token push.** It is written so that the operator can be rebuilt
without reading any of its code (D20, D34). It names every command, parameter, message, stream and
exit code, which host each command and each connection reaches, the protocol of the token push,
a host as the laptop sees it, and the AWS calls each command makes. The reasoning behind each rule
is in [decisions.md](decisions.md), cited as `D<n>`.

**From v0.08 a host runs only while its project holds a live lease** (D56 to D58): the guard,
`agentspace-operator guard`, which the keeper starts every 5 minutes, stops every host that no live
lease explains ([guard](#guard)); each host bounds itself by the lease horizon the guard sends it
while the laptop is away ([The idle check and the lease
horizon](#the-idle-check-and-the-lease-horizon)); and each host's alarm is the 24-hour backstop
([The stack](#the-stack)). **A rule cited "architect, 2026-11-21, for v0.08-T01" is one of the
dev lead's pins for v0.08's specs, P1 to P26, as the architect ruled it; one cited "architect,
2026-11-21, for v0.08-T01, S01 batch" is a choice this page's writers made that the architect
confirmed as written, or as P21 changed it for `sync`'s patch wait (P22)**. This page's writers
marked a choice that no decision or pin settled "architect's to confirm", binding as written
until the architect ruled on it. v0.08's were all ruled, the last of them as P24, P25 and P27, and
v0.09's too (below), so **no fact on this page carries that mark**.

**From v0.09 a host's size is asked for through a lease** (D65 to D68, D71): the operator takes an
instance type and knows no size (D67). `deploy -InstanceType <type>` sizes every host, the box
included, and `stack-parameters.json`'s `InstanceType` is no longer passed or used
([deploy](#deploy)); every `deploy` records the stack's type in the host's record
([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)); and a
provisioning run finds the type its project's `pending` leases need at its deploy step
([provision](#provision)). **A rule cited "architect, 2026-11-23, for v0.09-T01, pin Pn" is one of
the dev lead's pins for v0.09's specs, as the architect ruled it**, among them every choice this
page's writer made in E0.09-T01-S02 (#0198) where D64 to D71 and the card left one open: each was
first marked "architect's to confirm", and each now cites the pin that ruled it.

**What it does not own:**
- **The lease CLI** (`agentspace lease ...`) is [lease-api.md](lease-api.md)'s. It reaches one
  thing this page owns, the ProxyCommand, which every lease alias runs
  ([The ProxyCommand](#the-proxycommand)). It checks the CA itself, by
  [state.md's checks](state.md#its-checks), and never runs `ca` ([ca](#ca)).
- **Every file the operator reads or writes on the laptop is [docs/state.md](state.md)'s**: its
  location, format and access rules. This page names each file, says when it is read or written,
  and cites that doc for the rest ([Laptop files](#laptop-files)). **The one exception is
  localswim's receipt and Bot credential**, which agentspace reads and never writes: their formats
  are localswim's, which this page cites, and what the push checks in them is this page's
  ([The receipt](#the-receipt), [The Bot credential](#the-bot-credential)).
- **The box's own code stays shell (D28).** This page states the contract between the laptop and
  the box: what is sent, what is installed, what is asked and what is answered. It does not
  specify how the box's scripts do it.

**The operator is EC2-specific in v0.** Unlike the lease CLI, it names the backend: it manages
one EC2 instance per host, each through a CloudFormation stack of its own (D8, D9, D44). **The box**
is the host whose stack is `agentbox`, which is swimcoach's host (D43, D55); every other project
gets a host of its own at its first lease (D48). Every command line without `-Host` acts on the box
exactly as it did before v0.07 (D49). D1 binds what a lease's client sees,
and the operator's commands are not such a client. The ProxyCommand is the one part of this page
that a lease's client sees, so D1 binds it, and everything it runs
([What a caller sees](#what-a-caller-sees-d1-card-0087)).

## Contents

- [Conventions](#conventions)
  - [Where it runs](#where-it-runs)
  - [Command line](#command-line)
  - [Streams](#streams)
  - [Exit codes](#exit-codes)
  - [Messages](#messages)
  - [Programs it starts](#programs-it-starts) (card #0027)
  - [AWS failures](#aws-failures) (card #0034)
  - [Environment variables](#environment-variables)
  - [The test seam](#the-test-seam) (card #0085)
  - [Laptop files](#laptop-files)
  - [The `agentspace` SSH entry](#the-agentspace-ssh-entry)
  - [Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)
- [Programs and install layout](#programs-and-install-layout): [the programs](#the-programs),
  [the install folder](#the-install-folder),
  [how each program finds the others](#how-each-program-finds-the-others),
  [what the prototype took from its checkout](#what-the-prototype-took-from-its-checkout),
  [the operator's folder, one per command](#the-operators-folder-one-per-command),
  [argument style](#argument-style),
  [ProxyCommand lines already on the laptop](#proxycommand-lines-already-on-the-laptop),
  [until v0.06 retires the prototype's entry points](#until-v006-retires-the-prototypes-entry-points)
- [Which host a command acts on](#which-host-a-command-acts-on):
  [from the command line](#from-the-command-line),
  [from the ProxyCommand](#from-the-proxycommand),
  [the host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it),
  [how the operator reaches a host](#how-the-operator-reaches-a-host),
  [every host at once: `-All`](#every-host-at-once--all),
  [which checks show it](#which-checks-show-it)
- [Commands](#commands): [the top level](#the-top-level), [`deploy`](#deploy), [`up`](#up),
  [`sync`](#sync), [`bootstrap`](#bootstrap), [`provision`](#provision), [`ca`](#ca),
  [`down`](#down), [`status`](#status), [`grow`](#grow), [`guard`](#guard)
- [Starting the box and waiting for it](#starting-the-box-and-waiting-for-it)
- [The ProxyCommand](#the-proxycommand)
- [The token push](#the-token-push)
- [The box as the laptop sees it](#the-box-as-the-laptop-sees-it)
- [The AWS side](#the-aws-side)
- [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
- [Open questions](#open-questions)

## Conventions

### Where it runs

- **The operator runs on the laptop, which is Windows today.** Every path on this page that
  starts with `%USERPROFILE%`, `%LOCALAPPDATA%` or `%SystemRoot%` is a Windows path, and
  `<checkout>` is the folder of the agentspace repository that holds the operator. Where the
  rewrite keeps what a checkout holds today is
  [Programs and install layout](#programs-and-install-layout)'s.
- **The box is a host, and a host's platform is a parameter of the contract (D4).** From v0.07 there
  is one host per project (D25, D43): the box, which is swimcoach's, and one for each other project
  that has had a lease. Every host today is an EC2 instance from the one template, running Ubuntu
  on `linux/arm64`, logged in to as `ubuntu`, with systemd, bash and passwordless `sudo` (D44,
  D46). Wherever this page names one of those, it is a property of today's box, marked "today's
  box", which every host of v0.07 shares, and not of the contract. A Windows host (v0.12, #0051)
  would answer the same questions in its own way.
- **"The box" means swimcoach's host alone, and "a host" means any host, the box included.** A rule
  this page states of the box binds the box; one it states of a host binds every host. Where a
  rule written before v0.07 says "the box" of something every host now does (start and wait, the
  push, `sync`'s steps), the command acts on the host [Which host a command acts
  on](#which-host-a-command-acts-on) gives, and the rule binds that host the same way.
- **The prototype was `scripts/agentspace.ps1`, run by Windows PowerShell 5.1, and
  `scripts/ssh-proxy.cmd`.** They are named here only where a note says where a requirement came
  from. The rewrite replaced both in v0.06 (#0055, D28) with `agentspace-operator.exe` and
  `agentspace-ssh-proxy.exe`, installed as [Programs and install layout](#programs-and-install-layout)
  says. Every path this page gives under `<checkout>` is the prototype's, and that section gives
  the rewrite's place for each.
  - **Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository**, `bin\`
    and every PowerShell and cmd file the operator, the ProxyCommand and the keeper had included.
    **Every note on this page that names the prototype, and every `<checkout>` path under it, is
    history:** it says where a requirement came from, or what the rewrite replaced. None of them
    tells a reader to run the prototype, and none binds a program that is built now. What binds is
    the rule each note stands beside.

### Command line

```
operator <command> [-Host <project> | -All] [-GiB <n>] [-Alias <alias>] [-InstanceType <type>] [-Quiet]
operator -h | --help
operator <command> -h | --help
```

**`operator` stands for the operator's program, `agentspace-operator.exe`**, which a person runs
as `agentspace-operator` and the help texts below name so
([Programs and install layout](#programs-and-install-layout)). Note on where this came from: the
prototype's entry point is `powershell -NoProfile -File <checkout>\scripts\agentspace.ps1`, to
which its ProxyCommand adds `-ExecutionPolicy Bypass`.

- **`<command>` is the first positional argument** and MUST be one of `deploy`, `up`, `sync`,
  `bootstrap`, `provision`, `ca`, `down`, `status`, `grow` and `guard`, whole: it is matched
  without regard to case, and never as an abbreviation (`st` is unknown).
- **Parameter names are matched without regard to case**, and a value follows its name as the next
  argument (`-GiB 4`). **The argument after `-GiB`, `-Alias`, `-Host` or `-InstanceType` is its
  value, whatever it looks like**, so `-Alias -Fx` gives `-Alias` the value `-Fx`, and `-GiB -1`
  gives `-GiB` the value -1.
  `-All` and `-Quiet` are switches, and take no value.
  The one exception is help, below. The rewrite MAY accept other spellings the prototype's shell
  accepts (a unique prefix such as `-Q`, `-GiB:4`, `-Command status`, or a common parameter such as
  `-Verbose`, accepted and ignored), and a caller MUST NOT depend on them.
- **`-GiB`'s value MUST be a decimal integer:** an optional `+` or `-`, then one or more ASCII
  digits, from -2147483648 to 2147483647. Anything else is a usage error. The rewrite MAY accept
  other forms the prototype's shell converts (`0x10` as 16, `1e1` as 10, `2.5` as 2, an empty value
  as 0), and a caller MUST NOT depend on them. **For `grow`, the value MUST also be 1 or more**
  (D61, #0164): `grow` given a `-GiB` of 0 or less, `-1` and every other negative included, is a
  usage error, found by [`grow`'s check](#command-line) below, after the host checks, and no AWS
  call is made (D61 as corrected; P7, architect, 2026-11-21, for v0.08-T01). **Every other command
  checks the form alone** and ignores the value, so `status -GiB 0` is not an error
  (architect, 2026-11-21, for v0.08-T01, S01 batch) ([grow](#grow); [Q22](#open-questions), closed).
- **`-Host`'s value MUST be a project name in the form the lease CLI accepts**
  ([lease-api.md, Project](lease-api.md#project)): 1 to 32 letters, digits and `-`, starting and
  ending with a letter or digit, in any case, with surrounding whitespace. It is normalized as the
  lease CLI normalizes a project (lowercase, whitespace trimmed) before it is used, so
  `-Host Agentspace` names the project `agentspace`, and every message and file name carries the
  normalized form. Anything else is a usage error. That the operator accepts the lease CLI's
  forms, rather than the normalized form alone, is confirmed by the architect, 2026-11-20, for v0.07-T01. A host's stack,
  `agentspace-host-<project>`, is therefore at most 48 characters (D44; P13, architect,
  2026-11-20, for v0.07-T01).
- **`-InstanceType`'s value MUST be an EC2 instance type's form:** it matches
  `^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*\z`, case-sensitively, such as `m9g.2xlarge` or
  `m7i.metal-24xl`. Anything else is a usage error. Whether EC2 offers the type is not checked:
  `cloudformation deploy` refuses one it does not ([deploy](#deploy)). The form is confirmed by the architect, 2026-11-20, for v0.07-T01.
  **v0.09 keeps this form check, and adds no other** (D67: the operator takes an instance type
  and knows no size). The value is never checked against the EC2 driver's map of sizes to types
  ([driver.md, Sizes and instance types](driver.md#sizes-and-instance-types)), so a person MAY give any type of the
  form, such as `m7i.metal-24xl`, and a type the map does not hold meets no size once it is
  recorded (D71's `meets`). **The host registry's schema gives `instance_type` this form's pattern,
  at most 64 characters** (architect, 2026-11-23, for v0.09-T01, pin P14;
  [state.md, The host registry](state.md#the-host-registry)), so every type the stack reports back
  for a value this check accepted is of the member's form, up to its length. A value longer than
  64 characters passes this check, and is no instance type EC2 offers, so `cloudformation deploy`
  refuses it; should a stack ever report such a type, `deploy`'s step 5 fails as for a value not of
  the member's form.
- **Help is checked first.** An argument `-h` or `--help`, matched without regard to case, asks for
  help wherever it stands, even where a value is expected. Such a command line gets help, whatever
  else it holds: the help of its command when the line names a known command, and the top level's
  otherwise. So `sync -Alias x --help` prints `sync`'s help and exits 0.
  **Which argument is the command, for help** (architect, 2026-11-19, for v0.06, gap FB-11): the
  line is read from left to right, the argument after `-GiB`, `-Alias`, `-Host` or `-InstanceType`
  is that parameter's value unless it is `-h` or `--help`, and the command is the first argument
  that is none of a parameter, a parameter's value and help. A value never names a command, so
  `-Alias up -h` prints the top level's help, and `-Alias -h up` prints `up`'s. **An argument that
  starts with `-` and is not a parameter's value counts as a parameter, known or not** (architect,
  2026-11-19, for E0.06-T01-S01), the same reading as the unknown-parameter row below, so
  `-Foo up -h` prints `up`'s help: help is asked for, so the unknown parameter is never reported.
- **Every help block on this page is exactly what the rewrite prints, byte for byte**, to stdout,
  with a line feed after every line and no carriage return, the last line's included, and a change
  to either MUST change both (architect, 2026-11-19, for v0.06, gap FA-13; as
  [lease-api.md, Commands](lease-api.md#commands) rules for the lease CLI).
- **Every other problem with the command line is a usage error**, which prints one of the messages
  below to stderr, writes nothing to stdout, starts no program and exits 1
  ([Exit codes](#exit-codes)). **The operator MUST NOT prompt for anything**, a missing command
  included, because it runs where nobody can answer (inside a ProxyCommand, and under the lease
  CLI).

  | Problem | Message |
  |---|---|
  | No command | `agentspace: No command given. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard.` |
  | An unknown command `<arg>` | `agentspace: Unknown command '<arg>'. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard.` |
  | A second positional argument `<arg>` | `agentspace: Unexpected argument '<arg>'.` |
  | An unknown parameter `<arg>` (an argument that starts with `-` and is not a parameter's value) | `agentspace: Unknown parameter '<arg>'.` |
  | A parameter given twice, `-Quiet` and `-All` included | `agentspace: <name> is given more than once.` |
  | `-GiB`, `-Alias`, `-Host` or `-InstanceType` as the last argument, with no value | `agentspace: <name> needs a value.` |
  | A `-GiB` value that is not a decimal integer in range | `agentspace: -GiB MUST be a whole number, not '<arg>'.` |
  | `grow` given a `-GiB` value of 0 or less, by `grow`'s check below | `agentspace: -GiB MUST be 1 or more for grow, not '<arg>'.` |
  | A `-Host` value that is not a project name in the form above | `agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not '<arg>'.` |
  | An `-InstanceType` value that is not of the form above | `agentspace: -InstanceType MUST be an EC2 instance type such as m9g.xlarge, not '<arg>'.` |

  `<name>` is the parameter as this page spells it (`-GiB`, `-Alias`, `-Host`, `-InstanceType`,
  `-All`, `-Quiet`), whatever case was given. **The two command-list messages, the `-Host` message
  and the `-InstanceType` message are confirmed by the architect, 2026-11-20, for v0.07-T01**; the
  command-list messages' `guard`, added for v0.08, and the `-GiB` message for `grow`, which keeps
  its "MUST", are confirmed by the architect, 2026-11-21, for v0.08-T01, S01 batch. `<arg>` is the argument as given, with each control character in it
  written as `?`, so that the message stays one line: so `grow -GiB -0` reports `'-0'` and
  `grow -GiB +0` reports `'+0'`.
- **The checks run in this order, and the first failure ends the command:** help; then the
  arguments read from left to right, the first usage error found among them reported (a missing
  command is found only once the whole line is read); then the two `-Alias` checks below, in their
  order; then the five host checks below, in their order; then `grow`'s `-GiB` check below. So
  `status -GiB abc -Alias bad!` reports the `-GiB` message, `status -Host bad! -GiB abc` reports
  the `-Host` message, `ca -Alias x -Host y` reports `agentspace: -Alias applies to up alone.`, and
  `grow -All -GiB 0` reports `agentspace: -All applies to status, down and sync alone.` **At each argument a
  repeated name is found before a missing value** (architect, 2026-11-19, for E0.06-T01-S01, as
  keeper.md's check 1 reads its own), so `status -GiB 1 -GiB` reports `agentspace: -GiB is given
  more than once.`
- **Then the home and the system folder, for every command** (architect, 2026-11-19, for
  E0.06-T01-S01): when `USERPROFILE` is unset or empty the command fails with exactly
  `agentspace: USERPROFILE is not set, so there is no home folder.`, and when `SystemRoot` is,
  with `agentspace: SystemRoot is not set, so Windows' programs cannot be found.`, checked in that
  order, before anything else runs and exit 1. Help and usage errors need neither. In caller mode
  either is the "Could not reach the host" line ([What a caller sees](#what-a-caller-sees-d1-card-0087)).
- **The alias check and the host checks are part of the command line**, and run before anything
  but help and the usage errors, as below. **Then, after the home and the system folder, the host
  is looked up**: every command but `ca` finds its host's record, or every record for `-All`,
  before it starts any program or makes any AWS call
  ([From the command line](#from-the-command-line)).
- Note on where this came from: the prototype's shell binds the command line before the script
  runs, and reports each of these problems as its own error record, with exit 1, and, in the one
  pair tried, the first from the left. The record is not agentspace's message, so this is a
  requirement the prototype does not meet ([Requirements the prototype does not meet
  yet](#requirements-the-prototype-does-not-meet-yet)). With its stdin closed or not a console, it
  does not prompt for a missing command; at an interactive console it was not run.

| Parameter | Type | Default | Applies to | Notes |
|---|---|---|---|---|
| `-GiB <n>` | Integer | `2` | `grow` | The GiB to add to the root volume, 1 or more (D61). Every command checks its form ([Command line](#command-line)), so `status -GiB abc` is a usage error; `grow` then refuses a value of 0 or less before any AWS call, and every other command, `guard` included, ignores the value ([grow](#grow)). |
| `-Alias <alias>` | SSH host alias | The host's own alias: `agentspace` for the box, `agentspace-host.<project>` for another host | `up` | The SSH host alias the token push connects with (D9, card #0057). Any other command given `-Alias` fails. Without `-Host` the default is `agentspace`, as before v0.07. |
| `-Host <project>` | Project name | None: the box | `deploy`, `up`, `sync`, `bootstrap`, `provision` (which requires it), `down`, `status`, `grow` | The project whose host the command acts on (D49, D55). `-Host swimcoach` names the box, and is the same as no `-Host` (P1, architect, 2026-11-20, for v0.07-T01). `ca` and `guard` given `-Host` fail. [Which host a command acts on](#which-host-a-command-acts-on). |
| `-InstanceType <type>` | EC2 instance type | None: the stack's previous value, or the template's default for a stack `deploy` creates | `deploy`, for every host, the box included (no `-Host`, or `-Host swimcoach`) | The instance type the host's stack is deployed with (P13, architect, 2026-11-20, for v0.07-T01; D67, D71). **It is the one way any host changes size, the box included** (D67): `stack-parameters.json`'s `InstanceType` is not passed and not used ([deploy](#deploy), step 2; architect, 2026-11-23, for v0.09-T01, pin P6). A provisioning run gives its deploy step one from its project's `pending` leases when no live `ready` lease holds the host ([provision](#provision), step 4), and its command line takes none. Any other command given `-InstanceType` fails. Until v0.09, `deploy` without `-Host` or with `-Host swimcoach` failed with it too; v0.09 deleted that check (D67; architect, 2026-11-23, for v0.09-T01, pin P5). |
| `-All` | Switch | Off | `status`, `down`, `sync` | Every host in the host registry, one at a time ([Every host at once](#every-host-at-once--all)). Any other command given `-All` fails, and so does `-All` with `-Host`. |
| `-Quiet` | Switch | Off | Every command | Suppresses every message that is not a failure and not `status`'s report, warnings included ([Messages](#messages)). |
| `-h`, `--help` | Switch | | Every level | Prints the help of the level it is at to stdout and exits 0, doing nothing else. The prototype has no help of its own ([Q2](#open-questions)); the texts below are what the rewrite MUST print, naming the operator `agentspace-operator` ([Programs and install layout](#the-programs)). |

**`-Alias` is checked before anything but help and the usage errors runs**, for every command:
1. **Given with any command but `up`**, the operator fails with `agentspace: -Alias applies to up
   alone.`
2. **An alias MUST match `^[A-Za-z0-9._-]+\z`, case-sensitively, and MUST NOT start with `-`**,
   because it lands on ssh's command line, where a leading `-` would be read as an option. `\z`,
   unlike `$`, never matches before a trailing newline, and an empty alias fails. Any other value fails with
   `agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not
   start with '-'.` The default is checked the same way, `agentspace` or
   `agentspace-host.<project>`, which every normalized project makes a valid alias. Nothing is
   started before this check, not even an AWS call.

**The host checks run next, in this order** (D49, D55; each message is confirmed by the architect, 2026-11-20, for v0.07-T01):
1. **`-Host` given with `ca` or `guard`** fails with `agentspace: -Host does not apply to <command>.`,
   `<command>` being `ca` or `guard` as this page spells it, so `ca`'s message is unchanged byte for
   byte (the `guard` form is the architect's, 2026-11-21, for v0.08-T01, S01 batch). `guard -Host x -All` therefore reports the
   `-Host` message, check 1 coming before check 3.
2. **`-All` given with any command but `status`, `down` and `sync`** fails with
   `agentspace: -All applies to status, down and sync alone.`
3. **`-Host` and `-All` together** fail with `agentspace: -Host and -All cannot be given together.`
4. **`provision` without `-Host`** fails with `agentspace: provision needs -Host <project>.`, with
   `<project>` written as those nine characters.
5. **`-InstanceType` given with any command but `deploy`** fails with
   `agentspace: -InstanceType applies to deploy alone.`

**v0.09 deleted v0.07's sixth host check and its message** (D67, D71: `deploy -InstanceType
<type>` is allowed for every host, the box included; architect, 2026-11-23, for v0.09-T01, pin
P5). It refused `deploy -InstanceType` without `-Host`, or with `-Host swimcoach`. Such a line now
passes the host checks, and goes on to the home and the system folder like any `deploy`. **Five
host checks remain**: checks 1 to 5, their messages and their order are unchanged, check 5
included, so every line another check refused before v0.09 is refused by the same check with the
same message.

**Then `grow`'s `-GiB` check** (D61, #0164): **`grow` given a `-GiB` whose value is 0 or less**
fails with `agentspace: -GiB MUST be 1 or more for grow, not '<arg>'.`, a usage error, exit 1, as
every operator usage error is (D61 as corrected; P7, architect, 2026-11-21, for v0.08-T01). The
default, 2, passes. It is checked after the host checks and before the home and the system folder,
as every usage error is, so it needs neither, and before the host registry is read, so
`grow -Host nobody -GiB 0` reports it rather than the project's missing host (architect,
2026-11-21, for v0.08-T01, S01 batch). **It starts no program and makes no AWS call**: no `ec2 describe-instances`, no
`ec2 describe-volumes`, no `ec2 modify-volume`, and for `-Host` no key, certificate or alias is
written. No other command checks the value.

Nothing is started before these checks either, and none reads a file.

### Streams

- **The operator's own messages go to stderr, every one**, progress, warnings, failures and
  `status`'s report alike, so that `up` is safe inside a ProxyCommand, whose stdout is the SSH
  connection ([The ProxyCommand](#the-proxycommand) says why).
- **`up` MUST write nothing to stdout, and MUST NOT let anything it starts write there.** The
  ProxyCommand also sends `up`'s stdout to stderr, as a second guard.
- **`up` MUST NOT read its stdin, and MUST NOT let anything it starts read it**, because inside a
  ProxyCommand its stdin is the SSH client's. Every program the operator starts that needs input
  gets its own pipe, and every other program gets an empty or closed stdin. The prototype lets the
  AWS CLI inherit `up`'s stdin ([Requirements the prototype does not meet
  yet](#requirements-the-prototype-does-not-meet-yet)).
- **Output of programs the operator runs is passed through as they write it**, for `deploy`,
  `sync`, `bootstrap`, `provision` and `grow`: the AWS CLI's `cloudformation deploy` output,
  `go build`'s, the host's `install.sh` lines, the bootstrap's output and `grow`'s `df` table
  reach the operator's stdout or stderr as those programs wrote them. A caller MUST NOT parse
  that output; it is for a person.
- **A pass-through call MUST return once the program it runs exits**, with everything that
  program wrote already passed through, as the bullet above says (architect, 2026-11-20, for
  E0.06-T05-S02, #0139). The operator's `deploy` once waited forever after the AWS CLI's
  `cloudformation deploy` had exited, having passed none of its lines through.
- **Help goes to stdout**, with exit 0. **Help that cannot be written**, to a full or closed
  stdout, fails with `agentspace: Could not write the help to stdout.` on stderr and exit 1
  (architect, 2026-11-19, for E0.06-T01-S01).

### Exit codes

| Exit | When |
|---|---|
| 0 | The command did what was asked. |
| 1 | Any failure, usage errors included. The last message on stderr says what failed. |

**The operator's commands MUST NOT exit with any other code.** The ProxyCommand has its own
rule: [The ProxyCommand](#the-proxycommand). Unlike
[the lease CLI](lease-api.md#exit-codes-and-errors), the operator gives usage errors exit 1, as the
prototype does for every usage error, the `-Alias` refusals included ([Q1](#open-questions),
closed).

**A failure MUST end the command at once**, with nothing after it run, and MUST print one message
that starts `agentspace: `. **The two exceptions are `-All` and `guard`** ([Every host at
once](#every-host-at-once--all); [guard](#guard)): a failure on one host ends that host's part, and
the command goes on to the next host and exits 1 at its end, unless the failure is #0034's sign-in
message, which ends it at once (architect, 2026-11-20, for v0.07-T01; for `guard`, architect,
2026-11-21, for v0.08-T01, S01 batch). `grow`'s refusal of a `-GiB` of 0 or less is a usage error like any other, exit 1 (P7,
architect, 2026-11-21, for v0.08-T01). **A failure MUST NOT print a runtime's error record,
stack or exception text in place of that message.** Where a message carries the system's error
text, it is the system's own words alone: a runtime's addition, such as Rust's trailing
` (os error <n>)`, is dropped (architect, 2026-11-19, for E0.06-T03-S01). The prototype does in
some cases
([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

### Messages

- **Each message is one line**, except the few that this page shows on several lines, ending in a
  line feed. A message starts with a capital, or with `agentspace: `, or with a path or an
  identifier.
- **A failure starts `agentspace: `**, followed by the text this page gives.
- **A warning starts `agentspace: ` too**, and does not change the exit code. Only the token push,
  `provision`'s last step, which places the project's pending leases ([provision](#provision)),
  and `sync`'s patch wait, for a patch run that failed ([The patch wait](#the-patch-wait); P21),
  warn.
- **A progress message has no prefix.**
- **`-Quiet` suppresses progress messages and warnings.** Failures, `status`'s report and the
  `Host` line that `-All` prints before each host always print. `guard`'s progress lines are
  suppressed, and its log is not ([guard](#guard)). Output passed through from other
  programs is not affected by `-Quiet`. **Nor is the detail `AGENTSPACE_DIAGNOSTIC` asks for**
  (architect, 2026-11-19, for E0.06-T01-S01): it prints only in caller mode, which has no
  `-Quiet`, and a caller who sets the variable asked for it.
- **Placeholders in this page's message texts:** `<id>` is the instance ID, `<vol>` a volume ID,
  `<fp>` a key fingerprint (`SHA256:...`), and `<user>` the Windows user name, as the value of
  `USERNAME`, with no domain. `<project>` is a normalized project name, and `<stack>` a host's
  stack name, `agentbox` or `agentspace-host-<project>`. Other placeholders are named where they
  appear.
- **Messages carry the instance ID and volume IDs.** A check that captures the operator's stderr
  captures them, and MUST withhold them from anything committed or reported. The repository is
  public.

### Programs it starts

**Card #0027 (ssh and scp by full path) is carried in here as a requirement.**
- **Every `ssh` and `scp` the operator starts MUST be Windows OpenSSH's, by full path:**
  `%SystemRoot%\System32\OpenSSH\ssh.exe` and `scp.exe`. It MUST NOT be found through `PATH`.
  Started from Git Bash, `PATH` finds MSYS's ssh, which runs the ProxyCommand through `/bin/sh`
  and mangles its Windows path (`exec: C:WindowsSystem32cmd.exe: not found`).
- **So `deploy`, `up`, `sync`, `bootstrap`, `provision`, `status` and `grow` MUST work when the
  operator is started from Git Bash**, with or without `-Host` or `-All`.
  That is #0027's done-when, and this requirement closes it once the rewrite meets it.
- **Source:** in the prototype only the token push's ssh and the CA key's scp are by full path;
  every other ssh and scp is by bare name.

**The other programs, and how each is found:**

| Program | Found as | Used by |
|---|---|---|
| Windows OpenSSH `ssh.exe`, `scp.exe` | `%SystemRoot%\System32\OpenSSH\`, by full path (#0027) | `sync`, `bootstrap`, `provision`, `grow`, the token push |
| `ssh-keygen.exe` | `%SystemRoot%\System32\OpenSSH\ssh-keygen.exe` | `ca`; `sync`'s CA check; and, for a host that is not the box, `deploy`'s CA check and the operator's key and certificate ([How the operator reaches a host](#how-the-operator-reaches-a-host)). Like the CA's own steps, the key and the certificate MAY be made without starting it, by any tool that writes state.md's formats |
| AWS CLI v2 | `C:\Program Files\Amazon\AWSCLIV2\aws.exe`; when that file is missing, `aws` on `PATH`. The tunnel alone has no fallback ([The ProxyCommand](#the-proxycommand), step 6) | Every command but `ca`; the ProxyCommand's tunnel |
| Session Manager plugin | Found by the AWS CLI on `PATH`; the ProxyCommand puts `C:\Program Files\Amazon\SessionManagerPlugin\bin` first | The ProxyCommand's tunnel |
| `icacls.exe`, `taskkill.exe` | `%SystemRoot%\System32\`, by full path | `ca`; the token push's time limit. **The rewrite MAY reach either outcome through the Windows API instead, and starts neither then** (architect, 2026-11-19, for v0.06, gap FB-15): `ca`'s steps and the push's step 4 define the outcome, and this row only says how each is found when it is started |
| `go` | On `PATH` | `sync`, to build the localswim CLI |
| The operator, `agentspace-operator.exe` | In the ProxyCommand's own folder ([How each program finds the others](#how-each-program-finds-the-others)); a ProxyCommand that runs `up` in its own process starts none. The prototype's ProxyCommand runs `powershell.exe` (Windows PowerShell 5.1), found on `PATH`, with its `agentspace.ps1` | The ProxyCommand, to run `up` |
| The operator, `agentspace-operator.exe`, as a provisioning run | In the lease CLI's own folder, or the program `AGENTSPACE_PROVISION_CMD` names ([How each program finds the others](#how-each-program-finds-the-others)), by the EC2 driver, which starts `provision -Host <project>` detached ([driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run); D48, D55) | The driver; the operator itself starts no run of `provision` |
| The operator, `agentspace-operator.exe`, as a guard run | In the keeper's own folder, or the program `AGENTSPACE_GUARD_CMD` names, started by the keeper as `guard`, detached ([keeper.md, Starting the guard](keeper.md#starting-the-guard); D56, D63) | The keeper; the operator itself starts no guard |

**How the paths are written (A-24):** a path that starts `%SystemRoot%` MUST be built from the value
of `SystemRoot` as the operator finds it, never from the fixed text `C:\Windows`, which is that
value on every machine seen; the prototype builds them so. The two Amazon paths are the fixed text
the table gives, as in the prototype; the rewrite MAY build them from `ProgramFiles` instead, whose
value is `C:\Program Files` on every machine seen. A ProxyCommand line, which no shell expands, is
written as [state.md, The alias file](state.md#the-alias-file) says.

**When a default program cannot be run** (architect, 2026-11-19, for E0.06-T01-S02), the command
fails, exit 1, with exactly:
- `agentspace: <path> is missing; install the Windows OpenSSH client.` for a missing `ssh.exe` or
  `scp.exe`, the words of `ca`'s step 1;
- `agentspace: The AWS CLI is not installed: <path> is missing and no aws is on PATH.` for an AWS
  CLI found neither way (the tunnel's has no fallback, and says `<path> is missing.` alone);
- `agentspace: Could not run <path>.` for a program that exists but cannot be started.

`<path>` is the full path the default names. In caller mode each is the "Could not reach the host"
line. The `PATH` search finds only a file the system can run, so an `aws` there that is not
executable is passed over, and the answer is "not installed". **A remote command that breaks gap FA-15's rule is a defect of the operator's own, never
sent**, and fails with `agentspace: The remote command cannot be sent: <reason>.`, without the
command, and caller mode's "Could not reach the host".

**A test MAY name the AWS CLI, `ssh` and `scp` that the operator runs in place of the ones above**,
through three environment variables, and nothing else MAY ([The test seam](#the-test-seam)). The
table is what runs when none is set, and the full-path rule above binds that default alone.

**What MUST NOT appear among any program's arguments is in [Secrecy](#secrecy).**

### AWS failures

**Card #0034 (an expired aws login gets agentspace's own message) is carried in here as a
requirement.**
- **When an AWS CLI call fails because the sign-in has expired or is missing, the operator MUST
  fail with exactly:**

  ```
  agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.
  ```

  A failure is a sign-in failure when the CLI's error text matches `expired`, `aws login` or
  `Unable to locate credentials`, without regard to case. That is the prototype's test; whether
  it catches every form of the CLI's expired-login error is [Q26](#open-questions).
- **Any other failure of a call whose output the operator reads MUST fail with `agentspace: `
  followed by the CLI's own error text**, trimmed, and never with a runtime's error record or stack
  in its place. **The error text is the CLI's stderr; when that is empty, its stdout; and when
  both are, `The AWS CLI failed with exit code <n>.`** (architect, 2026-11-19, for
  E0.06-T01-S02). **A call whose JSON answer does not parse fails with
  `agentspace: Unexpected answer from <service> <operation>: not JSON.`**, the form of FB-08's
  message.
- **Inside the ProxyCommand the sign-in message MUST print exactly as above**, so that
  `ssh agentspace`, `ssh <lease alias>` and the Claude desktop app show it, not a raw error, before
  ssh's own `Connection closed by UNKNOWN port 65535`. **Every other AWS failure there prints the
  caller's message in place of the CLI's text**, which only `AGENTSPACE_DIAGNOSTIC` lets through
  ([What a caller sees](#what-a-caller-sees-d1-card-0087)). The rule above, `agentspace: ` and the
  CLI's own text, binds a person's commands alone.
- **It MUST hold for every AWS call whose failure fails the command**, `cloudformation deploy`
  included. The calls whose failure is retried or ignored are listed in
  [Calls by command](#calls-by-command).
- **`cloudformation deploy`'s output is passed through as it is written.** The operator MUST also
  scan its error text for the sign-in test, and otherwise ends with `deploy`'s own message
  ([deploy](#deploy), step 4). The prototype does not scan it. The tunnel runs only in the
  ProxyCommand, and its errors are [What a caller sees](#what-a-caller-sees-d1-card-0087)'s.
- **Source:** in the prototype, the AWS CLI's stderr is merged into its output while every error
  is terminating, so under Windows PowerShell 5.1 the CLI's first stderr line throws before the
  exit code is checked, and the message above never prints (#0034). The same
  mechanism may reach two more calls ([Q5](#open-questions)).

**Every AWS CLI call the operator makes passes `--region us-west-2`, and every call but
`cloudformation deploy`, `ec2 wait instance-stopped` and the tunnel also passes `--output json`**,
whether or not its output is then read ([Calls by command](#calls-by-command)). The region is
today's, and not configurable
([Q15](#open-questions)). The calls are listed per command in [The AWS side](#the-aws-side).
**The order of the arguments MUST be:** the service, the operation, `--region us-west-2`, then
`--output json` where it is passed, then the operation's own arguments in the order
[Calls by command](#calls-by-command) gives them (architect, 2026-11-19, for v0.06, gap FB-07). The
tunnel's line in [The ProxyCommand](#the-proxycommand), step 6, is that order. So a stand-in finds
the service and the operation as its first two arguments. **For `ec2 wait instance-stopped` the
operation is `wait`**, and `instance-stopped` is the first of its own arguments, so the line is
`ec2 wait --region us-west-2 instance-stopped --instance-ids <id>`, with no `--output json`
(architect, 2026-11-19, for E0.06-T03-S03, run against the real AWS CLI and accepted).

### Environment variables

| Variable | Read or set | Meaning |
|---|---|---|
| `AGENTSPACE_NESTED` | Read at start; set to `1` for everything the operator starts | When it is set to a non-empty value as the operator starts, `up` does not push the token. Every ssh the operator starts runs the ProxyCommand and so `up` again ([below](#every-ssh-runs-the-proxycommand)), and this keeps that nested `up` from pushing a second time. |
| `*_POSIX_FD_STATE` | Removed at start | Windows OpenSSH gives each process it starts, the ProxyCommand included, a variable named `<GUID>_POSIX_FD_STATE`. Inherited by an ssh that the operator starts, it makes that ssh never read its stdin, so the token push never sent its input and hung (card #0026). **The operator MUST remove every variable whose name ends `_POSIX_FD_STATE`, whatever the GUID, before it starts any program.** The rewrite MAY meet this, and `AGENTSPACE_NESTED`'s row, on each program's own environment rather than its own: the environment every program it starts sees is what this row binds (architect, 2026-11-19, for E0.06-T01-S01 and S02: Rust edition 2024 makes changing a process's own environment `unsafe`). `agentspace-run` gives every crate that starts a program one function that applies both rules to it (D42). |
| `GOOS`, `GOARCH`, `CGO_ENABLED` | Set for `go build` only | `linux`, `arm64` and `0` while `sync` builds the localswim CLI, and removed afterwards. `linux/arm64` is today's box's platform. |
| `USERPROFILE`, `LOCALAPPDATA`, `SystemRoot` | Read | Where the laptop files and Windows programs are. `LOCALAPPDATA` is the only way the operator finds localswim's receipt ([Secrecy](#secrecy)); unset or empty, there is no receipt, so the push is skipped and no command fails ([The receipt](#the-receipt)). |
| `OneDrive`, `OneDriveConsumer`, `OneDriveCommercial` | Read by `ca` | Folders the CA MUST NOT be inside (D17). |
| `USERNAME`, `COMPUTERNAME` | Read by `ca` and `sync` | The user named in the CA's messages, and the computer named in a new CA's key comment (docs/state.md). |
| `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD`, `AGENTSPACE_SCP_CMD`, `AGENTSPACE_TIMEZONE` | Read, each time the operator needs what it names | Name the programs the operator starts in place of the AWS CLI, `ssh` and `scp`, and the zone the token push sends in place of the laptop's. For tests only ([The test seam](#the-test-seam)). |
| `AGENTSPACE_DIAGNOSTIC` | Read by the ProxyCommand | When it is set to a non-empty value, the ProxyCommand MAY print backend detail to stderr that it otherwise withholds ([What a caller sees](#what-a-caller-sees-d1-card-0087)), and its token push drops nothing of its ssh's stderr, `The host is ready.` included ([What the laptop sends](#what-the-laptop-sends), step 2; #0167, P14, architect, 2026-11-21, for v0.08-T01). |
| `AGENTSPACE_STATE_DIR` | Read by the ProxyCommand, `provision`, `status -All` and `guard` | Moves the state folder, where the lease state file is, as it does for the lease CLI ([state.md, What moves which files](state.md#what-moves-which-files)). The ProxyCommand reads the lease state file from there to find a lease alias's host ([From the ProxyCommand](#from-the-proxycommand)), `provision` reads its project's leases there for its deploy step's instance type ([provision](#provision), step 4) and places the project's pending leases there ([provision](#provision), step 9), `status -All` counts each host's live leases from it ([status](#status)), and `guard` reads it to decide which hosts a live lease explains ([guard](#guard)). It moves nothing else the operator reads: the host registry, the project list, each host's folder, `guard.log` and `guard.lock` stay under `%USERPROFILE%\.agentspace`, as state.md says. |
| `AGENTSPACE_SSH_DIR` | Read by `provision`, and by every command that writes a host's operator alias | Moves the ssh folder, where a placed lease's alias file and the `Include` line are written, as it does for the lease CLI ([state.md, What moves which files](state.md#what-moves-which-files)). `provision`'s step 9 uses it as `create` does. The operator's own alias for a host is written in the ssh folder it names too ([How the operator reaches a host](#how-the-operator-reaches-a-host)). |
| `AGENTSPACE_PROXY_COMMAND` | Read by `provision` | The lease CLI's ProxyCommand override ([lease-api.md, Environment](lease-api.md#environment)): `provision`'s step 9 writes it into each placed lease's alias file as `create` would. |
| `PATH` | Read; the ProxyCommand prepends two folders | Finding `go`, the AWS CLI's fallback, and the Session Manager plugin. |

**The token MUST NOT be in any environment variable the operator sets ([Secrecy](#secrecy)).**

**A provisioning run that the EC2 driver starts does not get the lease CLI's environment** (P12,
architect, 2026-11-20, for v0.07-T01; [driver.md, Starting a provisioning
run](driver.md#starting-a-provisioning-run)). `agentspace-detach` starts it with WMI's
`Win32_Process.Create`, so it gets the logon environment, which Windows gives every process it
starts that way, with these four set exactly as this list says, and nothing else of the lease
CLI's (P12 and P15, architect, 2026-11-20, for v0.07-T01; [state.md, The provisioning run's
start](state.md#the-provisioning-runs-start)):
- **`USERPROFILE`**, the lease CLI's value, made full against the lease CLI's working folder;
- **`AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR`**, each in the run's environment if and only if
  the lease CLI had it set and not empty, made full against the lease CLI's working folder as
  [state.md, What moves which files](state.md#what-moves-which-files) makes a relative folder full;
- **`AGENTSPACE_PROXY_COMMAND`**, in the run's environment if and only if the lease CLI had it set
  and not empty, exactly as it was, since it is a command line and not a folder.

**A value of one of those three that the logon environment holds and the lease CLI did not have is
removed** (P15, architect, 2026-11-20, for v0.07-T01), so a run never takes a folder or a
ProxyCommand line that the command which started it did not use.

So **the test seam's variables (`AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD`, `AGENTSPACE_SCP_CMD`,
`AGENTSPACE_TIMEZONE`) never reach a detached provisioning run**, nor does `AGENTSPACE_DIAGNOSTIC`,
and a check reaches a run only by running `provision` itself ([The test seam](#the-test-seam),
step 8). `AGENTSPACE_NESTED` is unset in a detached run, so its `sync` step pushes as every `sync`
does.

**A guard the keeper starts is the one detached run that carries seam variables** (P4, architect,
2026-11-21, for v0.08-T01): it gets the environment a provisioning run gets, built from the user's
logon environment as `agentspace-detach` builds it, and with it every `AGENTSPACE_*_CMD` variable
the keeper itself was given, so a check's throwaway home and stand-ins reach it. `AGENTSPACE_TIMEZONE`
and `AGENTSPACE_DIAGNOSTIC` do not reach it. The list is
[keeper.md, The guard's environment](keeper.md#the-guards-environment)'s; what the guard reads of
it is [guard](#guard)'s.
**`provision` builds the EC2 driver's `Context` for its step 9 from these variables as the lease
CLI builds its own**: the state folder, the ssh folder and the ProxyCommand line, from
`AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND` when set, and from
`USERPROFILE` otherwise (D55, which adds `state_dir` to the `Context`). A run started by a person
reads the same variables from the person's environment.

### The test seam

**Card #0085 is carried in here, and closes [Q11](#open-questions) and [Q12](#open-questions).**
**Four environment variables let a test stand in for the programs the operator starts and for the
laptop's time zone**, so that a check can run `up`, `sync` and the ProxyCommand against a box that
is a script (D34). Without them the operator starts the AWS CLI and `ssh` by fixed paths that
exist on the laptop ([Programs it starts](#programs-it-starts)), and the push can be reached only
by a real box.

| Variable | When set and not empty, it names | Which the operator runs, or uses, in its place |
|---|---|---|
| `AGENTSPACE_AWS_CMD` | A program | The AWS CLI: every AWS call of every command, and the ProxyCommand's tunnel ([Calls by command](#calls-by-command)) |
| `AGENTSPACE_SSH_CMD` | A program | `ssh`: every ssh the operator starts, the token push's included |
| `AGENTSPACE_SCP_CMD` | A program | `scp`: every scp the operator starts, `sync`'s copies |
| `AGENTSPACE_TIMEZONE` | An IANA zone name, such as `America/New_York` | The laptop's zone, in the token push ([The time zone](#the-time-zone)) |

**What each one does:**
- **A program variable's value is one path, taken whole.** Spaces in it are part of the path: the
  operator MUST NOT split it into a program and arguments, and MUST NOT look the name up on `PATH`.
  A relative path is resolved against the operator's working folder. On Windows the path MAY be an
  executable or a `.cmd` file.
- **The operator starts that program exactly as it would the real one:** the same arguments, in
  the same order, the same standard input, output and error wiring, the same environment, and the
  same limits (the push's 20 seconds, for one). It reads the program's exit code and streams as it
  would the real one's, so the operator cannot tell a stand-in from the real program except by
  what it does. Everything else on this page, including every message and exit code, is
  unchanged.
- **The variable is read from the process's environment each time the operator is about to start
  the program**, and not before. A variable that no command reaches is never read: `ca`, help, a
  usage error and the `-Alias` refusals start no such program ([Command line](#command-line)). A
  child of the operator, the ProxyCommand among them, reads it from its own environment, which is
  the operator's.
- **Unset, empty, or only whitespace means the default**, the program of
  [Programs it starts](#programs-it-starts), as if the seam did not exist.
- **A value that names nothing runnable MUST NOT fall back to the default.** A file that does not
  exist, a folder, or a file the system cannot start, fails the command with `agentspace:
  <variable> names <value>, which cannot be run.` and exit 1, as the program is first needed. The
  one exception is the token push, which never fails a command: a push whose `ssh` cannot be
  started warns as [any other failure](#what-the-laptop-checks) does. A mistyped variable
  therefore never reaches the real AWS or a real box.
- **`AGENTSPACE_TIMEZONE`'s value is read as the push works out its zone**, and replaces the
  laptop's zone as [The time zone](#the-time-zone) says: the legacy names are mapped, and the
  check of step 3 applies. **A value that fails that check gives the empty zone**, so a test
  reaches the push's no-zone path with a value such as `not a zone`. The value is not secret, and
  nothing else reads the variable.

**The seam MUST NOT weaken secrecy ([Secrecy](#secrecy)).**
- **A stand-in gets the token only as the real `ssh` would: on the push's stdin, and nowhere
  else.** The operator MUST NOT add a flag, an argument, an environment variable, a file or a log
  that a stand-in could read the token, or the content, from, and MUST NOT make the seam's
  variables carry any part of either. A program started through the seam gets no more than the
  real one.
- **No other source MAY set the variables.** The operator reads them from the environment alone,
  never from a file. A person in normal use MUST NOT set them, as with the lease CLI's own test
  variables ([lease-api.md](lease-api.md#environment)); the lease CLI does not read them.
- **A check that stands in for `ssh` is bound by [Secrecy](#secrecy)'s last rule**, about the
  receipt and the credential that `LOCALAPPDATA` points at.

**How a test uses it, from outside:**
1. **It builds a throwaway home** and runs the operator with `USERPROFILE` (and the state and ssh
   folders, [docs/state.md](state.md#what-moves-which-files)) set to it, `LOCALAPPDATA` set to a
   throwaway folder holding a fake localswim receipt and a fake credential written for the same
   Windows user ([The receipt](#the-receipt); [The Bot credential](#the-bot-credential) says how a
   dummy credential is written), and no real AWS sign-in reachable.
2. **It writes the host registry**, `hosts.json`, into the throwaway home, with the box's record
   and a made-up instance ID, and a record for each other host the test needs
   ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it);
   its form is [state.md's](state.md#the-host-registry)). Before v0.07 this step wrote the
   instance-ID cache, which `hosts.json` replaced (D45).
3. **It writes a stand-in `aws`** that answers the calls `up` makes
   ([Calls by command](#calls-by-command)), as the real CLI would in JSON: an instance that is
   `running`, launched more than 20 minutes ago, so that `up` does not wait
   ([Starting the box](#starting-the-box-and-waiting-for-it)). It exits 0 for each.
4. **It writes a stand-in `ssh`** that records its arguments and its stdin to a file in the
   throwaway home, prints the hash line the box would ([What the box does](#what-the-box-does)),
   and exits 0.
5. **It sets the three variables to those programs and `AGENTSPACE_TIMEZONE` to a zone**, runs
   `operator up`, and checks what it can see: the recorded arguments
   (`-o ClearAllForwardings=yes -o BatchMode=yes <alias> <remote command>`, the last one
   argument), the recorded stdin (the two lines, and no more), the stamp file in the throwaway
   home, the messages on stderr, the exit code, and that stdout is empty.
6. **It runs `sync` or the ProxyCommand the same way**, adding a stand-in `scp`, or one whose
   tunnel exits at once. `sync`'s stand-ins see the calls of [sync's table](#sync), in its order,
   and `grow`'s the one ssh of [grow](#grow); a stand-in that exits non-zero for one of them
   reaches that step's failure message.
7. **It runs a command with `-Host <project>` or `-All` the same way**, with the registry of step 2
   holding the hosts it names: the stand-ins see that host's stack name and instance ID, and the
   ssh and scp calls name that host's alias. For a host that is not the box, the throwaway home
   also holds a throwaway CA ([state.md, The CA](state.md#the-ca)), which the operator signs its
   certificate with ([How the operator reaches a host](#how-the-operator-reaches-a-host)).
8. **It reaches a provisioning run by running `provision -Host <project>` itself**, with the seam
   set, never through a detached start, so the run's exit code and streams are the test's to read
   ([provision](#provision)). **A run the driver starts detached never reaches the seam**: its
   environment is the logon environment with the four variables set and removed as
   [Environment variables](#environment-variables) says (P12 and P15, architect, 2026-11-20, for
   v0.07-T01), so the seam's variables set for the lease CLI are not passed to it. **So a check of
   the lease CLI that can reach `pending` MUST set `AGENTSPACE_PROVISION_CMD` to a stand-in**
   ([How each program finds the others](#how-each-program-finds-the-others)), or the driver would
   start the installed operator against the real AWS. **A stand-in named by
   `AGENTSPACE_PROVISION_CMD` never calls `place_pending`** (P15, architect, 2026-11-20, for
   v0.07-T01), so such a check's leases stay `pending`.
9. **It reaches the guard by running `guard` itself**, with the seam set and the case's lease state
   file in the throwaway home's state folder ([guard](#guard), its seams). **A check of the keeper
   MUST set `AGENTSPACE_GUARD_CMD` to a stand-in** ([keeper.md, Starting the
   guard](keeper.md#starting-the-guard)), or the keeper would start the operator beside it against
   whatever the seam variables it was given name. Neither has a clock seam, so a case sets its
   lease and launch times relative to the real clock.

**Not covered:** `go`, `ssh-keygen`, `icacls` and `taskkill` have no variable
([Q30](#open-questions)).
- **So a test of `sync` or `deploy` inside the program's own crate MUST NOT reach the real `go` or
  `C:\Projects\localswim`** (architect, 2026-11-19, for E0.06-T03-S02): a build from Terry's
  checkout would read it. The crate gives its tests a hidden entry with the `go` finder, the
  localswim folder, the clock, the CA check and the push passed in, and no seam variable is
  added. `sync` finds `go` on `PATH` through a lookup over the process's environment, as the AWS
  CLI's fallback is found, and reads nothing else of the environment. A host registry that
  `deploy` cannot write, when it removes the record's instance ID or writes it again, fails with
  `agentspace: Cannot write <path>.`; a record with no instance ID at the removal is not an error.
  (Before v0.07 this rule named the instance-ID cache, which `hosts.json` replaced, D45.)
- **`ssh-keygen` has no variable, so a test of a host that is not the box runs the real
  `ssh-keygen.exe`** for the operator's key and certificate, against the throwaway CA in the
  throwaway home. That signs nothing with the real CA, and reaches no host.

### Laptop files

**Every file here is [docs/state.md](state.md)'s**: its exact location, format and access rules.
This table says only which command reads or writes it. **The one exception is localswim's receipt
and Bot credential:** their formats are localswim's, which this page cites, and what the push
checks in them is this page's ([The receipt](#the-receipt),
[The Bot credential](#the-bot-credential)); state.md owns none of them. **The `<checkout>` rows
are the prototype's places, which the prototype's removal (E0.06-T06-S01, 2026-11-20) left as
history;** where the rewrite keeps each is
[Programs and install layout](#programs-and-install-layout) (D37).

| File (as today's prototype has it) | Read by | Written by | What it is |
|---|---|---|---|
| `<checkout>\.instance-id`, then `%USERPROFILE%\.agentspace\instance-id.json` (D21, D40), until v0.07 | Nothing, from v0.07 | Nothing, from v0.07 | The instance-ID cache, retired by D45: the host registry's records hold each host's instance ID. No v0.07 program reads, writes or removes the file, and nothing migrates from it; a person MAY delete it |
| `%USERPROFILE%\.agentspace\hosts.json`, and its lock `hosts.lock` beside it | Every command but `ca`, `guard` without its lock; the ProxyCommand | `deploy`, `provision`, which alone of the operator's commands write a record's `instance_type` (D67, D71); any command that finds a record's instance ID missing, and the ProxyCommand's `up`, which writes that member alone (P13, architect, 2026-11-20, for v0.07-T01); the EC2 driver ([driver.md](driver.md)) | The host registry: one record per host, with its project, stack, state, instance ID, instance type and last attempt (D45, D67; [state.md, The host registry](state.md#the-host-registry); [below](#the-host-registry-as-the-operator-uses-it)) |
| `%USERPROFILE%\.agentspace\projects.json` | `provision` (and the EC2 driver) | Never: a person, or the architect with Terry's yes (D54) | The project list: the projects that MAY have a host made ([state.md, The project list](state.md#the-project-list)) |
| `%USERPROFILE%\.agentspace\localswim-pushed.sha256` today; `localswim-pushed.json` beside it as D40 requires | `up` | `up`, `sync` | The box's push stamp ([The stamp](#the-stamp)); unchanged by v0.07 (D55, "The box's own files") |
| `%USERPROFILE%\.agentspace\hosts\<project>\localswim-pushed.json` | `up` | `up`, `sync` | Another host's push stamp (D50; [state.md, A host's folder](state.md#a-hosts-folder)) |
| `%USERPROFILE%\.agentspace\hosts\<project>\id_ed25519`, its `.pub`, and `id_ed25519-cert.pub` | The ssh client, through the host's operator alias | Every command given `-Host`, or `-All`, for a host that is not the box ([How the operator reaches a host](#how-the-operator-reaches-a-host)) | The operator's own key for that host, and its one-hour certificate (D47; [state.md, A host's folder](state.md#a-hosts-folder)). Whether the key's folder holds the public half as `id_ed25519.pub` is state.md's |
| `%USERPROFILE%\.agentspace\hosts\<project>\provision.log`, `provision.lock` | People and checks; `provision`; from v0.09 `deploy`, which holds the lock for its life as a run does, creating the lock and its folder when missing (architect, 2026-11-23, for v0.09-T01, pin P34); the driver and `guard`, which try the lock, `guard` without creating it or its folder | `provision` alone, which opens the log itself and writes every line of it, its own lines alone (architect, 2026-11-21, for v0.07-T03); the driver makes the host's folder before a start, and never opens the log (P14, architect, 2026-11-20, for v0.07-T01) | The provisioning run's log and its lock, held for the run's life ([provision](#provision); [state.md, A host's folder](state.md#a-hosts-folder)) |
| `%USERPROFILE%\.agentspace\guard.log` | People | `guard` alone, a line at a time; created at its first line | The guard's log: one line per host per stop, unexplained host or failure, and one per failure of the run itself ([guard](#guard); [state.md, The guard's log](state.md#the-guards-log)) |
| `%USERPROFILE%\.agentspace\guard.lock` | The keeper, which tries it before it starts a guard (P3, architect, 2026-11-21, for v0.08-T01) | `guard`, which holds it for its run's life; created if missing, never deleted | The guard's lock ([guard](#guard); [state.md, The guard's lock](state.md#the-guards-lock)) |
| `<ssh folder>\agentspace.d\agentspace-host.<project>` | The ssh client | Every command given `-Host`, or `-All`, for a host that is not the box | The host's operator alias ([state.md, A host's operator alias](state.md#a-hosts-operator-alias)) |
| `%USERPROFILE%\.agentspace\known_hosts` | The ssh client, through a host alias of a host that is not the box | The ssh client alone, never the operator (D47) | The host keys of every host but the box, each under `agentspace-host.<project>` ([state.md, Agentspace's host keys](state.md#agentspaces-host-keys)) |
| `%USERPROFILE%\.agentspace\ca\agentspace_ca` | `ca`, `sync`; for a host that is not the box, `deploy` and every command that signs the operator's certificate | `ca`, once | The CA's private key (D17), one for every host (D50) |
| `%USERPROFILE%\.agentspace\ca\agentspace_ca.pub` | `ca`, `sync`; `deploy`, when it creates a host's stack | `ca`, once | The CA's public key, which `sync` sends to every host, and `deploy -Host` passes as `CaPublicKey` to the stack it creates for a host that is not the box |
| `%USERPROFILE%\.ssh\id_ed25519.pub` | `deploy` | Never | The user's own public key, passed as `SshPublicKey` to the box's stack at every deploy and to a new host's stack when it is created ([deploy](#deploy), step 1) |
| `%USERPROFILE%\.ssh\config` | The ssh client, for the `agentspace` entry and lease aliases | Never, but for agentspace's `Include` line, which every command that writes a host's operator alias, and `provision`'s `place_pending`, add when it is missing, as `create` does ([state.md, The `Include` line](state.md#the-include-line)) | [The `agentspace` SSH entry](#the-agentspace-ssh-entry) |
| `<state folder>\leases.json`, and its lock `leases.lock` beside it | The ProxyCommand, without the lock, to find a lease alias's host ([From the ProxyCommand](#from-the-proxycommand)); `status -All` and `guard`, without the lock, for each project's live leases ([status](#status); [guard](#guard)); `provision`'s deploy step, under the lock, which it takes and releases before any call to AWS, for the instance type its project's leases need ([provision](#provision), step 4; architect, 2026-11-23, for v0.09-T01, pin P26); `provision`'s `place_pending`, under the lock | `provision`'s `place_pending`, which writes a placed lease's record `ready`, and from v0.09 a `pending` lease the host does not meet `unsatisfiable` when a live `ready` lease holds the host (pin P1), one lease per hold of the lock ([provision](#provision), step 9) | The lease state file ([state.md, The lease state file](state.md#the-lease-state-file)), which the lease CLI owns |
| `%USERPROFILE%\.agentspace\leases\<id>\` and the key and certificate in it | The ssh client, through a lease alias | `provision`'s `place_pending`, for each lease it places, after removing a killed placement's leftovers | A placed lease's folder ([state.md, The lease folder](state.md#the-lease-folder)) |
| `<ssh folder>\agentspace.d\<alias>`, a lease's alias file | The ssh client | `provision`'s `place_pending`, for each lease it places; never the operator's own commands | A placed lease's alias ([state.md, The alias file](state.md#the-alias-file); [state.md, A lease alias for a host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box)) |
| `%USERPROFILE%\.claude\CLAUDE.md` | `sync` | Never | The user's global instructions for Claude Code, copied to each host |
| `<checkout>\infra\local.parameters` today; `%USERPROFILE%\.agentspace\stack-parameters.json` as D37 and D40 require | `deploy` | Never | The account-specific stack parameters, which are not committed. Every member but `InstanceType` goes to the box's stack; only `SubnetId` and `VpcId` go to a new host's stack, and none to an existing host's. Its `InstanceType`, if it names one, is passed to no stack and used for nothing from v0.09, and is left in the file (D67, D71; [deploy](#deploy), step 2) |
| `%USERPROFILE%\.agentspace\operator\<hex>\` | `deploy`, `sync`, `provision`, and the programs they start on what is there | Each of those commands, its own folder, removed as it ends; a folder there older than 24 hours is removed by the next such command | The files a command sends: the template, `box\system` and the localswim build ([What the prototype took from its checkout](#what-the-prototype-took-from-its-checkout); P13, architect, 2026-11-20, for v0.07-T01) |
| `<checkout>\infra\agentspace.yaml` | `deploy` | Never | The stack's template ([The stack](#the-stack)) |
| `<checkout>\box\system\` | `sync` | Never | The box's system files, sent whole |
| `<checkout>\.build\localswim-linux-arm64` | `sync` | `sync` | The localswim CLI built for today's box |
| `C:\Projects\localswim` | `sync` | Never | The localswim checkout the CLI is built from |
| `%LOCALAPPDATA%\localswim-v2\service.json` | `up` and `sync`, each time they consider a push | Never | The localswim service's receipt: the service's base URL and its credential folder. Its format is localswim's, and what the push checks in it is [The receipt](#the-receipt)'s |
| `bot.json` in the folder the receipt names | `up` and `sync`, after the receipt | Never | The localswim Bot credential, which holds the Bot token protected with DPAPI. Its format is localswim's, and what the push checks in it is [The Bot credential](#the-bot-credential)'s |

**The token is never in any of the operator's own files** ([Secrecy](#secrecy)).

**The instance-ID cache was retired by D45.** Until v0.07 the instance ID was cached alone in
`%USERPROFILE%\.agentspace\instance-id.json` (D21, D40, which settled [Q7](#open-questions)). From
v0.07 each host's instance ID is a member of its record in the host registry, read from and written
to it as [The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)
says, which also gives every message that the cache's rules gave. The cache's rules of v0.06 that
still bind, now of a record's instance ID: it is only a cache of the stack's `InstanceId` output,
read from the stack again when it is missing; a stack with no `InstanceId` output fails the command
and writes no instance ID ([Q24](#open-questions)); and an instance ID that cannot be written fails
with `agentspace: Cannot write <path>.`

### The `agentspace` SSH entry

**For the box, the operator needs one SSH host alias in the user's OpenSSH client configuration:
`agentspace`.** `sync`, `bootstrap`, `grow` and the token push of `sync` connect to the box through
it, and so does any `up` of the box started without `-Alias`. A person writes the entry once, in
their own OpenSSH client configuration, and agentspace never writes it
([state.md](state.md#the-include-line)). **It is the box's alone:** every other host is reached
through an alias agentspace writes itself, `agentspace-host.<project>`
([How the operator reaches a host](#how-the-operator-reaches-a-host)), so a new host needs no
change to the user's configuration and never uses the user's own key (D47). The operator keeps
using `agentspace` for the box, as D47 allows, and writes no alias or certificate for the box
(architect, 2026-11-20, for v0.07-T01). What the operator needs from the entry is this:

- **`Host agentspace`**, the alias the operator's own ssh and scp calls name.
- **It reaches the box through the ProxyCommand with no argument**: the default line of
  [state.md, The alias file](state.md#the-alias-file), without the space and the alias that end a
  lease's line. So the ProxyCommand runs with no alias, and its `up` pushes over `agentspace`
  ([The ProxyCommand](#the-proxycommand)).
- **It logs in as the box's login user (`User`) with the key that the box's first boot installed**
  (`IdentityFile`; `SshPublicKey`, [The stack](#the-stack)). Today that is `ubuntu` with
  `~/.ssh/id_ed25519`, whose public half `deploy` reads.
- **It MUST accept a new host key on first use and check it afterwards** (`StrictHostKeyChecking
  accept-new` in OpenSSH): an unknown host key is recorded without a prompt, and a known one that
  has changed is refused. The first connection through it is what puts the box's host key in the
  user's `known_hosts`, recorded under the name `agentspace`, the entry's own `Host` name, since
  the entry MUST NOT set a `HostKeyAlias` of another name. Every lease alias checks the key under
  that name, and agentspace never writes it ([state.md, The host key](state.md#the-host-key)).
  Whichever ssh connects through the entry first records the key, a `sync` step or a push
  included: `BatchMode` on the push's ssh ([What the laptop sends](#what-the-laptop-sends))
  refuses only prompts, and accepting a new key here takes none. A push over a lease alias before
  any connection through this entry has no key to check against, so its ssh fails (exit 255) and
  the push warns that the box did not confirm the token
  ([What the laptop checks](#what-the-laptop-checks)).
- **The login user MUST be able to run `sudo` without a password or a terminal**, because `sync`
  and `grow` run commands as root over a non-interactive ssh, and the push's zone step MUST NOT
  prompt. That is a property of today's box.
- **It SHOULD carry the board's two `RemoteForward` lines**, the same two that end a lease's alias
  file ([state.md, The alias file](state.md#the-alias-file); D12). The operator's own ssh and scp
  calls clear them ([below](#every-ssh-runs-the-proxycommand)), so they apply only to a person's
  interactive session.
- **It MAY set keepalives** (`ServerAliveInterval 30`, `ServerAliveCountMax 4` today, as a lease's
  alias file does). The operator needs nothing else from it.

A lease alias is the lease CLI's ([state.md, The alias file](state.md#the-alias-file);
[lease-api.md](lease-api.md#the-credential)), and runs the same ProxyCommand with the alias after
it. A box's lease alias keeps v0.06's bytes, `HostKeyAlias agentspace` and the user's
`known_hosts` included (D47, D55); a lease alias for another host checks that host's key under
`agentspace-host.<project>` in agentspace's own known-hosts file ([state.md, A lease alias for a
host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box)).

### Every ssh runs the ProxyCommand

**Every ssh and scp the operator starts goes through a host alias, and so runs the ProxyCommand,
which runs `up` again.** That alias is the host's own: `agentspace` for the box, and
`agentspace-host.<project>` for any other host, whose ProxyCommand line names that alias, so the
nested `up` acts on the same host ([From the ProxyCommand](#from-the-proxycommand)). That nested
`up` is a full `up` but for the push: it reads the instance state, starts the host if needed and
waits. **It runs inside a ProxyCommand, so it prints the
caller's lines, not a person's** ([What a caller sees](#what-a-caller-sees-d1-card-0087)): its last
is `The host is ready.`, never `Ready: <id>`. So a `sync` prints one `The host is ready.` line per
ssh or scp it starts, to stderr, and each one costs a Session Manager session. **The token push of
a ProxyCommand connection is the exception** (#0167): its ssh's stderr reaches the connection
through a filter that drops its nested ProxyCommand's `The host is ready.`, so a connection prints
that line once, its own, whether or not it pushes, unless `AGENTSPACE_DIAGNOSTIC` is set
([What the laptop sends](#what-the-laptop-sends), step 2; P14, architect, 2026-11-21, for
v0.08-T01).

- **Every ssh and scp the operator starts MUST pass `-o ClearAllForwardings=yes`**, so it never
  takes the board's forwarded ports from a person's interactive session (D12). Those ports are the
  board's, and only one session can hold each.
- **The nested `up` MUST NOT push the token** (`AGENTSPACE_NESTED`,
  [Environment variables](#environment-variables)).

## Programs and install layout

**The rewrite is seven files on the laptop, installed together in one folder: six programs and a
wrapper for POSIX shells.** Each program finds the others, and everything the prototype took from
its checkout, without a checkout of the repository. This section owns their file names, the
install folder, how each program finds the others and those files, the argument style, and how
the prototype's entry points behaved until v0.06 (#0055) retired them (a record now: see [the
status of that part](#until-v006-retires-the-prototypes-entry-points)). Its rules are D37's, which
settles [Q10](#open-questions) here, and keeper.md's [Q3, Q8 and Q10](keeper.md#open-questions).

### The programs

| File | Role | Its spec | Kind | Conformance variable |
|---|---|---|---|---|
| `agentspace.exe` | The lease CLI, run as `agentspace` | [lease-api.md](lease-api.md) | Console program | `AGENTSPACE_CMD` |
| `agentspace` | The lease CLI's wrapper for Git Bash and other POSIX shells: it runs `agentspace.exe` from its own folder with its arguments unchanged and MSYS path conversion off, as [lease-api.md, The program](lease-api.md#the-program) allows | lease-api.md | POSIX `sh` script, no extension, with LF line ends and `#!/bin/sh` as its first line, which is what makes Git Bash run a file with no extension | None |
| `agentspace-operator.exe` | The operator: `deploy`, `up`, `sync`, `bootstrap`, `provision`, `ca`, `down`, `status`, `grow` and `guard` | [Command line](#command-line) | Console program | `AGENTSPACE_OPERATOR_CMD` |
| `agentspace-ssh-proxy.exe` | The ProxyCommand | [The ProxyCommand](#the-proxycommand) | Console program | `AGENTSPACE_SSH_PROXY_CMD` |
| `agentspace-keeper.exe` | The keeper | [keeper.md, Command line](keeper.md#command-line) | Console program | `AGENTSPACE_KEEPER_CMD` |
| `agentspace-keeper-task.exe` | The keeper task's launcher, which the logon task runs | [keeper.md, The task's launcher](keeper.md#the-tasks-launcher) | Windows program, with no console | None |
| `agentspace-keeper-install.exe` | The keeper task's installer | [keeper.md, The installer](keeper.md#the-installer) | Console program | None |

- **These names are fixed.** One program finds another by its file name, and a person runs one by
  its command name, which is the file name without `.exe`: `agentspace`, `agentspace-operator`,
  `agentspace-keeper`, `agentspace-keeper-install`.
- **Help, usage lines and a program's own messages print its command name**, whatever path or
  wrapper started it: the operator's help says `Usage: agentspace-operator ...`
  ([Commands](#commands)), and the keeper's and the installer's usage errors start
  `agentspace-keeper: ` and `agentspace-keeper-install: `. **The ProxyCommand is the exception:** it
  is never typed, only named by path in a ProxyCommand line, so its one message that names itself
  prints its file name, `agentspace-ssh-proxy.exe` ([The ProxyCommand](#the-proxycommand), step 1).
  The prototype's programs print their own names (`agentspace.ps1`, `ssh-proxy.cmd`,
  `board-holder.ps1`, `install-board-holder.ps1`), so **a check MUST NOT depend on which name is
  printed**, only on the rest of the text.
- **Only the lease CLI and the operator have help** ([lease-api.md](lease-api.md); [The top
  level](#the-top-level)). The keeper and the keeper's installer have none: `-h`, `--help` and
  `-?` are unknown arguments to them, and so usage errors with exit 2
  ([keeper.md, Usage errors](keeper.md#usage-errors); [keeper.md, The installer](keeper.md#the-installer)).
  The task's launcher passes every argument on to the keeper, so it exits with the keeper's
  usage error, 2. The ProxyCommand takes its one argument as the alias, so `-h` is an alias, which
  `up`'s check refuses because it starts with `-`, and the ProxyCommand exits 1
  ([The ProxyCommand](#the-proxycommand), steps 3 and 4).
- **A build MAY make several of these one binary**, which selects its role by its own file name, so
  long as each file runs its role with the command line its spec gives. `agentspace-keeper-task.exe`
  cannot share a binary with a console program, since a program's kind is fixed when it is built.
- **For a conformance run against an install or a build's output folder**, each conformance
  variable names the program in that folder by full path. Each role is its own file, so the four
  command lines never collide in one program, and no wrapper per role is needed
  ([conformance.md, Naming the program under test](conformance.md#naming-the-program-under-test)).
- **The programs are native Windows programs, written in Rust (D28).** The `agentspace` wrapper is
  a few lines of `sh`, for the same reason the box's files stay shell: it configures the shell that
  runs it.

### The install folder

- **An install puts every file it ships in one folder, `%USERPROFILE%\.agentspace\bin`.** Wherever a
  path into it is stored (a lease's alias file, the keeper's task), `USERPROFILE`'s value is written
  out. This page and keeper.md write the folder `<install>`.
- **The install folder MUST NOT be under `%LOCALAPPDATA%` or `%APPDATA%` (D21).** An install made
  by an agent inside the Claude app would land there in the app's package cache, where an ssh
  started outside the app, and Task Scheduler, would never find the programs. It also MUST NOT need
  administrator rights, so an install is never run elevated, as `ca` never is (D17, [ca](#ca)).
- **`AGENTSPACE_STATE_DIR` does not move it**, and no program finds it through a variable: each
  finds the others by its own folder ([below](#how-each-program-finds-the-others)). Only the keeper's
  installer compares its own folder with `<install>` ([keeper.md](keeper.md#the-installer)).
- **An install is a copy, and no agentspace program makes it** (D37). A person, or an agent at the
  person's request, copies the files of one release from one build's output folder (the folder a
  build of the repository writes these files to, under these names) into `<install>`, creating
  `<install>` if it is missing. The keeper's installer only registers the task.
- **An install is whole:** it copies every file its release ships, all from that one build, so
  that the programs in `<install>` were built from one version of the repository. v0.05's are
  `agentspace.exe`, the `agentspace` wrapper and the forwarder `agentspace-ssh-proxy.cmd`, and
  v0.06's are the seven files, with the forwarder kept as
  [Until v0.06 retires the prototype's entry points](#until-v006-retires-the-prototypes-entry-points)
  says. An install whose copy fails part way SHOULD be run again before any program in the folder
  is used, since its programs then come from two builds.
- **An install MUST NOT stop a running program to replace its file.** Windows refuses to overwrite
  or delete the file of a running program but lets it be renamed, and the ProxyCommand runs for as
  long as any connection through it is open, the keeper's holder's included. So an install SHOULD
  rename such a file aside in the install folder, as `<file>.old-<32 lowercase hex digits>`, copy
  the new one in, and remove each `.old-` file it can on a later install. A running program keeps
  its old code until it is started again; for the keeper, that is
  [keeper.md, Stopping and starting the keeper](keeper.md#stopping-and-starting-the-keeper).
- **An install SHOULD NOT replace `agentspace-keeper.exe` while a keeper runs, and SHOULD stop the
  keeper first.** A keeper whose holder's launcher is its own program starts that launcher by its
  own file's path ([below](#how-each-program-finds-the-others)), which after a rename aside holds
  the new build, so a start would pair a keeper and a launcher of two builds.
- **The install folder SHOULD be on the user's `PATH`**, so that `agentspace` runs `agentspace.exe`
  from cmd and PowerShell, and the `agentspace` wrapper from Git Bash, which looks for the file with
  no extension before `agentspace.exe` (seen with Git Bash on the laptop, 2026-11-18). Changing the
  user's `PATH` is persistent configuration on Terry's laptop, and MUST be done only with his yes,
  as the keeper's task is (D30).
- **An install writes no state, registers no task and changes no SSH entry.** The task is the
  installer's ([keeper.md, The installed task](keeper.md#the-installed-task)), and the user's
  `agentspace` entry is the user's ([below](#proxycommand-lines-already-on-the-laptop)).
- **A build's output folder holds the same files under the same names**, so the programs run from
  it as from an install, and a check MAY name them there. A lease that a CLI in a build's folder
  creates names that folder in its alias file ([state.md, The alias file](state.md#the-alias-file)),
  which a cleanup can remove, so real leases SHOULD be created by the installed CLI. So does a
  lease that a CLI run from a git worktree creates. Once that folder is gone, every connection
  through the lease's alias fails, since ssh cannot start its ProxyCommand, until the lease is
  released: nothing rewrites an alias file, `renew` included.

### How each program finds the others

**Every program finds another agentspace program in the folder its own executable runs from, by
the file name in [the table](#the-programs), and never through `PATH`, the working folder, an
environment variable or a checkout.** This is D8's rule for the ProxyCommand, applied to every
program (D37). **The exceptions are the program the EC2 driver starts for a provisioning run, and the program the
keeper starts for a guard run, which `AGENTSPACE_GUARD_CMD` names when it is set** (D63;
[keeper.md, Starting the guard](keeper.md#starting-the-guard)). For the first
(P12, architect, 2026-11-20, for v0.07-T01; D55): when `AGENTSPACE_PROVISION_CMD` is set and not
empty, the lease CLI starts the program it names in place of `agentspace-operator.exe` beside it, a
relative value made full against the lease CLI's working folder before the detached start. It is
the lease CLI's own documented variable ([lease-api.md, Environment](lease-api.md#environment)),
like `AGENTSPACE_PROXY_COMMAND`, and not a conformance naming variable, so
`AGENTSPACE_OPERATOR_CMD`, which names the operator under test, never names the run's program.

| Program | Finds | For |
|---|---|---|
| `agentspace.exe` | `agentspace-ssh-proxy.exe`; when that file is not there, `agentspace-ssh-proxy.cmd`, v0.05's forwarder ([below](#until-v006-retires-the-prototypes-entry-points)) | The default ProxyCommand line ([state.md, The alias file](state.md#the-alias-file)) |
| `agentspace` (the wrapper) | `agentspace.exe` | Running the lease CLI |
| `agentspace.exe`, as the EC2 driver | `agentspace-operator.exe`, or the program `AGENTSPACE_PROVISION_CMD` names (the exception above) | Starting a provisioning run, `provision -Host <project>`, detached (D48; [driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run); the driver's `Context` carries the program, D55) |
| `agentspace-operator.exe` | `agentspace-ssh-proxy.exe` | The ProxyCommand line of a host's operator alias ([How the operator reaches a host](#how-the-operator-reaches-a-host)). Only the `.exe` is looked for: v0.06's forwarder exists for lines already written, and no new line names it |
| `agentspace-operator.exe`, in `provision`'s `place_pending` | `agentspace-ssh-proxy.exe`; when that file is not there, `agentspace-ssh-proxy.cmd`, exactly as `agentspace.exe` finds it, beside the running program's own executable | The default ProxyCommand line of each lease alias `place_pending` writes ([provision](#provision), step 9; P15, architect, 2026-11-20, for v0.07-T01; [state.md, The alias file](state.md#the-alias-file), D8). **When neither is there, placement fails for that lease, which stays `pending`.** `AGENTSPACE_PROXY_COMMAND`, when the run has it, replaces the whole line, and nothing is looked for |
| `agentspace-ssh-proxy.exe` | `agentspace-operator.exe`, when it runs `up` as a separate program. It MAY instead run `up` in its own process, with the same result | [The ProxyCommand](#the-proxycommand), step 3 |
| `agentspace-keeper.exe` | Its own file, when the holder's launcher is the keeper's own program again | [keeper.md, Starting a holder](keeper.md#starting-a-holder) |
| `agentspace-keeper.exe` | `agentspace-operator.exe`, or the program `AGENTSPACE_GUARD_CMD` names, the keeper's own variable (D63) | Starting a guard run, `guard`, detached ([keeper.md, Starting the guard](keeper.md#starting-the-guard)) |
| `agentspace-keeper-task.exe` | `agentspace-keeper.exe` | Running the keeper ([keeper.md](keeper.md#the-tasks-launcher)) |
| `agentspace-keeper-install.exe` | `agentspace-keeper.exe` and `agentspace-keeper-task.exe` | Checking both exist, and naming the launcher in the task by full path ([keeper.md](keeper.md#the-installer)) |

- **A program's own executable is the file the operating system started, as a full path.** A
  program MUST NOT take it from its first argument, which the process that started it chooses.
- **A program that does not find a sibling it needs fails the step that needs it**, with its own
  form of failure message, saying in agentspace's words which program is missing. The lease CLI's
  case is [state.md, The alias file](state.md#the-alias-file)'s.

### What the prototype took from its checkout

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository, so the
prototype's column below is history, kept for what each row says the rewrite replaced.** The
rewrite's column binds, as it did before.

**Wherever this page names a path under `<checkout>`, it is the prototype's, and the rewrite uses
the place this table gives** (D37; [Laptop files](#laptop-files), [deploy](#deploy), [sync](#sync),
[ca](#ca), [The AWS side](#the-aws-side)). The files' formats are [docs/state.md](state.md)'s, at
[Files in the checkout](state.md#files-in-the-checkout).

| The prototype's | Used by | The rewrite's |
|---|---|---|
| `<checkout>\infra\agentspace.yaml`, the stack's template | `deploy` | Carried inside the operator program, built from the repository's `infra\agentspace.yaml`. `deploy` writes it to `%USERPROFILE%\.agentspace\operator\<hex>\agentspace.yaml`, in its own folder ([below](#the-operators-folder-one-per-command)), just before the call that reads it, and names that file to the AWS CLI |
| `<checkout>\box\system\`, the box's system files | `sync` | Carried inside the operator program, built from the repository's `box\system`. `sync` writes them to `%USERPROFILE%\.agentspace\operator\<hex>\system\`, in its own folder ([below](#the-operators-folder-one-per-command)), just before the copy, and copies that folder |
| `<checkout>\schemas\state\leases.state.schema.json` and the keeper's state schemas | The lease CLI and the keeper, to check the state files they read and write | Carried inside each program that checks a file against them |
| `<checkout>\infra\local.parameters`, `KEY=VALUE` lines | `deploy` | `%USERPROFILE%\.agentspace\stack-parameters.json`, JSON written by a person (D40) |
| `<checkout>\.build\localswim-linux-arm64` | `sync` | `%USERPROFILE%\.agentspace\operator\<hex>\localswim-linux-arm64`, in its own folder ([below](#the-operators-folder-one-per-command)) |
| `<checkout>\.instance-id` | Every command but `ca`; the ProxyCommand | `%USERPROFILE%\.agentspace\instance-id.json` (D21, D40) in v0.06; from v0.07, a member of each host's record in `%USERPROFILE%\.agentspace\hosts.json` (D45) |
| A person's run of the checkout's `box\bootstrap.sh` over `ssh agentspace` | No operator command | Carried inside the operator program, built from the repository's `box\bootstrap.sh`, and sent to the host on an ssh's stdin by `bootstrap` (D46); it is never written to a file on the laptop |
| `%USERPROFILE%\.agentspace\localswim-pushed.sha256`, not in the checkout | `up`, `sync` | `%USERPROFILE%\.agentspace\localswim-pushed.json` (D40) |
| `<checkout>`, the folder the CA MUST NOT be inside | `ca`, step 2 | Any git working tree: `ca` fails when the CA's folder, or any folder above it, holds an entry named `.git`, a folder or the file a git worktree has (D37). `<folder>` in that step's message is the working tree's folder. So a home folder that is itself a git working tree, as a repository of the user's own settings files makes it, refuses every CA under it |
| `<checkout>\scripts\ssh-proxy.cmd`, beside the CLI's code | The lease CLI's default ProxyCommand line | `agentspace-ssh-proxy.exe`, beside `agentspace.exe` (D8) |
| `<checkout>`, the task's working folder | The keeper's task | `%USERPROFILE%` ([keeper.md, The task's fields](keeper.md#the-tasks-fields)) |
| `<checkout>`, the main checkout the installer runs from | The keeper's installer | The install folder ([keeper.md, The installer](keeper.md#the-installer)) |

- **The template, `box\system` and `box\bootstrap.sh` the operator carries MUST be the
  repository's own files, embedded as they are at build time, and MUST NOT be re-authored from
  this page** (D37's addendum, FA-11, D46). They are maintained YAML and shell, not the prototype
  (D28). What the operator builds and sends to the box at run time, every remote command line
  included, is the rewrite's own, written from the steps on this page (D34).
- **A file the operator writes under `%USERPROFILE%\.agentspace\operator` that cannot be written
  fails the command** before anything reads it, with exactly `agentspace: Cannot write <path>.`,
  where `<path>` is the file's or folder's full path (architect, 2026-11-19, for v0.06, gap FA-16):
  the first one whose creation or write failed, so an `operator` folder that cannot be made is
  named itself, never a file or folder below it (architect, the same day, for E0.06-T03-S02), and
  nor is a command's own folder below it
  ([The operator's folder, one per command](#the-operators-folder-one-per-command)).
- **The localswim checkout that `sync` builds from, `C:\Projects\localswim`, is localswim's, not
  agentspace's**, and this section does not move it ([sync](#sync), step 7).

**Why the template and the box's files are carried inside the operator** (D37), rather than
installed beside it as files:
- **They are the far half of the operator's contract with the box:** the patch marker `up` waits
  for, the token's file and the wrapper that reads it, and the CA the box trusts
  ([The box as the laptop sees it](#the-box-as-the-laptop-sees-it)). Carried inside the operator,
  they are always its own build's, so an operator can never install a box that answers it in
  another version's terms.
- **An install of whole programs cannot be half-copied**, and leaves nothing in the install folder
  that a person might edit by hand. The box then changes only through the repository's
  `box\system`, a new build and `sync` ([What sync installs](#what-sync-installs)), for an install
  as for a checkout.
- **The AWS CLI and scp read files, not a program's memory**, so each is written out at use, and
  under `%USERPROFILE%\.agentspace`, never under AppData (D21).

The stack parameters file is not carried: it is the account's, written by a person, and never
committed.

### The operator's folder, one per command

**Each operator command writes what it sends into a folder of its own,
`%USERPROFILE%\.agentspace\operator\<hex>\`, and removes that folder as it ends** (P13, architect,
2026-11-20, for v0.07-T01, closing [Q32](#open-questions)). From v0.07 a provisioning run works in
the background (D48) while a person MAY run `sync`, or a `deploy` of another host, and a folder
per command means neither ever sends the other's files, or a folder the other is emptying. **From
v0.09 a `deploy` of the run's own host is refused while the run works**, since it takes that
host's `provision.lock` ([deploy](#deploy); architect, 2026-11-23, for v0.09-T01, pins P34 and
P36).
- **What goes there:** the template `deploy` names to the AWS CLI (`agentspace.yaml`), the
  `box\system` that `sync` copies (`system\`), and the localswim CLI that `sync` builds
  (`localswim-linux-arm64`). The bootstrap is sent on an ssh's stdin and never written to a file
  ([bootstrap](#bootstrap)). Nothing else the operator writes is in it.
- **`<hex>` is 32 lowercase hexadecimal digits, drawn at random as the command makes the folder**,
  which it creates new, never reusing a folder that exists. A name that exists is drawn again.
- **One folder per command line**: a command's steps share it, so `deploy`'s `sync` step and
  `provision`'s `deploy` and `sync` steps write into their command's folder. A command that sends
  no file (`up`, `bootstrap`, `ca`, `down`, `status`, `grow`) makes none, and nor does the
  ProxyCommand.
- **It is made just before its first file is written**, and removed, with everything in it, as the
  command ends, whether it succeeded or failed. A folder that cannot be removed then is left, and
  changes neither the command's messages nor its exit code; the sweep below removes it later.
- **The sweep:** a command that makes its folder first removes every folder in
  `%USERPROFILE%\.agentspace\operator\` whose name is 32 lowercase hexadecimal digits and whose
  creation time is more than 24 hours before now, left by a command that was killed. A folder it
  cannot remove is left, and changes nothing the command does. The longest command, a provisioning
  run, takes about ten minutes (D46), so the sweep never reaches a folder a running command still
  uses. Anything else there, such as what a v0.06
  operator wrote (`agentspace.yaml`, `system\`, `localswim-linux-arm64`), is left; a person MAY
  delete it. Which commands sweep, the creation time as the measure, and leaving v0.06's files are
  confirmed by the architect, 2026-11-20, for v0.07-T01.
- **Its location and access rules are state.md's** ([state.md, Files in the
  checkout](state.md#files-in-the-checkout)).

### Argument style

**The rewrite keeps every command line exactly as the specs give it (D37, B-04).**
- **The lease CLI keeps its `--name` flags** ([lease-api.md, Command lines](lease-api.md#command-lines)).
- **The operator, the keeper, the keeper's installer and the task's launcher keep their single-dash
  names** (`-GiB`, `-Alias`, `-Host`, `-InstanceType`, `-All`, `-Quiet`; `-Watch`, `-SshExe` and
  the rest; `-DryRun`, `-Uninstall`), matched as each page says: exactly, without regard to case,
  with a value as the next argument. The operator keeps `-h` and `--help`. The ProxyCommand keeps
  its one positional alias.
- **A single-dash name is a syntax, not PowerShell.** A native program parses it by the rules on
  its page. Spellings that PowerShell's binder also accepts, such as a unique prefix (`-Q`) or a
  value after a colon (`-GiB:4`), MAY be accepted, and a caller MUST NOT depend on them
  ([Command line](#command-line); [keeper.md, Command line](keeper.md#command-line)).
- **Why:** the conformance suite passes these command lines as written, through the four
  variables, and the keeper's task passes `-Watch`. Each page already defines its command line
  exactly, so keeping it costs a native program nothing, and changing it would change every caller
  and every check.

### ProxyCommand lines already on the laptop

**Status (E0.06-T06-S01, 2026-11-20): the checkout's `scripts\ssh-proxy.cmd` was removed with the
prototype, so the rules below that name it are history.** The v0.06 cutover (E0.06-T05-S01, [the
run record](runs/e0.06-t05-s01.md)) ran with no lease ready, as D41 plans, and pointed Terry's
`agentspace` entry at `agentspace-ssh-proxy.exe`, so no line the prototype wrote is in use. **The
rule in the next paragraph still binds** the install's programs: `agentspace-ssh-proxy.exe` and
the forwarder `agentspace-ssh-proxy.cmd` MUST keep working while a `ready` lease's alias file or
the user's entry names them.

**A ProxyCommand line names a program by its full path until the line is rewritten, so a program
that a line names MUST keep working, or forward to one that does, for as long as that line can
still make a connection (D37, B-30).** Three kinds of line exist:

| Line | Written by | Names | Rewritten |
|---|---|---|---|
| The user's `agentspace` entry ([The `agentspace` SSH entry](#the-agentspace-ssh-entry)) | The user, never agentspace | Today, the checkout's `scripts\ssh-proxy.cmd` | By the user, by hand |
| A lease's alias file, written by the prototype's lease CLI | `create` | The checkout's `scripts\ssh-proxy.cmd` | Never: `renew` does not rewrite it, and `release` removes it |
| A lease's alias file, written by the rewrite's lease CLI | `create` | The ProxyCommand program beside that CLI | Never, likewise |
| A host's operator alias, `agentspace-host.<project>` (v0.07) | The operator, before each command that reaches that host | `agentspace-ssh-proxy.exe` beside that operator | By every such command whose bytes differ, so a newer install's operator rewrites it at its next use |

**v0.07 changes no line's format** (D49): a line still names the program, then the alias or
nothing, and the ProxyCommand finds the host from that alias
([From the ProxyCommand](#from-the-proxycommand)). So every alias file already on the laptop, and
the user's entry, keep working, and no line is rewritten for v0.07.

- **Once the rewrite's ProxyCommand is installed, the user's `agentspace` entry SHOULD name it:**
  its `ProxyCommand` is then `"<install>\agentspace-ssh-proxy.exe"`, the full path in double
  quotes, with `USERPROFILE`'s value written out and no alias. ssh expands no environment variable
  in a ProxyCommand.
- **The checkout's `scripts\ssh-proxy.cmd` MUST NOT be removed, and MUST keep working, while the
  user's entry or a `ready` lease's alias file names it.** When v0.06 retires the prototype, it
  SHOULD become a forwarder that runs `<install>\agentspace-ssh-proxy.exe` by full path, with its
  arguments, standard input, output and error, and exit code unchanged; it MAY then be removed once
  nothing names it. An alias file of a lease that is no longer `ready` does not hold it back: its
  certificate is no longer valid, so no connection is made through it.
- **The same rule holds for v0.05's forwarder in the install folder**
  ([below](#until-v006-retires-the-prototypes-entry-points)).
- **A forwarder MUST read nothing more of itself once its target has started**: its whole work is
  one line, which either hands control to a batch file target or runs a program target and ends
  the forwarder within that same line. cmd reads a batch file by name, a line at a time, from the
  offset where it stopped, so a forwarder rewritten while a connection runs through it would
  otherwise go on in the new file's text when its target ends.
- **The checkout's `scripts\ssh-proxy.cmd` SHOULD be made a forwarder only while no connection
  runs through it**, the keeper's holder's included, for the same reason: it reads itself again
  after its tunnel ends.

### Until v0.06 retires the prototype's entry points

**Status (E0.06-T06-S01, 2026-11-20): v0.06 retired the prototype, and the condition in this
heading has passed.** The cutover on the laptop was E0.06-T05-S01 ([the run record](runs/e0.06-t05-s01.md)),
which installed the seven files and the forwarder and repointed Terry's entry and the keeper's
task. **E0.06-T06-S01 removed the prototype's files from the repository**: `bin\agentspace.cmd`
and `bin\agentspace`, `scripts\agentspace.ps1`, `scripts\ssh-proxy.cmd`,
`scripts\board-holder.ps1`, `scripts\install-board-holder.ps1` and the rest of the prototype
(D41 step 8, D42). The heading and the bullets below describe the change-over and are history,
and the ending of v0.05's forwarder, which reached into a checkout, was met the same way. **What
still binds the programs is the rule in the bullet that begins "v0.06's install adds the other
programs": v0.06's forwarder is one fixed line, `@"%~dp0agentspace-ssh-proxy.exe" %*`, ended in CR
LF, which `cargo xtask dist` writes into the output folder like any other file, and an install
keeps it while a `ready` lease's alias file names it.** The lease CLI still looks for
`agentspace-ssh-proxy.exe` first and `agentspace-ssh-proxy.cmd` second
([How each program finds the others](#how-each-program-finds-the-others)), and the keeper's
installer still removes the prototype's `BoardHolderLauncher.exe` from an install
([keeper.md, The installer](keeper.md#the-installer)).

**The rewrite arrives in two releases (D28): the lease CLI in v0.05 (#0054), and the operator, the
ProxyCommand and the keeper in v0.06 (#0055).** Until v0.06 removes them (D37):
- **The checkout's entry points keep running the prototype, unchanged:** `bin\agentspace.cmd` and
  `bin\agentspace` run the prototype's lease CLI, and `scripts\agentspace.ps1`,
  `scripts\ssh-proxy.cmd`, `scripts\board-holder.ps1` and `scripts\install-board-holder.ps1` are
  the prototype's operator, ProxyCommand, keeper and installer. **The `bin\` shims MUST NOT be
  pointed at an install:** the conformance suite runs the prototype through `bin\agentspace.cmd` by
  default ([conformance.md](conformance.md#naming-the-program-under-test)). Which `agentspace` a
  person gets is `PATH`'s choice, so once the install folder is on `PATH`, the checkout's `bin\`
  SHOULD be taken off it.
- **v0.05's install holds `agentspace.exe`, the `agentspace` wrapper, and a forwarder,
  `agentspace-ssh-proxy.cmd`**, which runs the checkout's `scripts\ssh-proxy.cmd` by full path with
  its arguments, standard input, output and error, and exit code unchanged, since the ProxyCommand
  is still the prototype's. **An install's forwarder MUST name the main checkout, the one whose
  `.instance-id` the prototype's ProxyCommand reads, and never a git worktree**, which is deleted
  after its branch merges. It is the checkout whose `scripts\ssh-proxy.cmd` the user's `agentspace`
  entry names ([ProxyCommand lines already on the laptop](#proxycommand-lines-already-on-the-laptop)),
  or, where that entry names none, the one the person installing gives. The install writes that
  `scripts\ssh-proxy.cmd`'s full path into the forwarder, and MUST NOT write a forwarder whose
  target does not exist. **Nor one whose target's path holds a character outside ASCII, a `"`, a
  line break, or the `\\?\` prefix** (architect, 2026-11-19, from E0.05-T02-S06): cmd reads a
  batch file in the console's code page, the OEM page for a task with no console, so it cannot
  follow such a path, and it cannot run a `\\?\` one. The forwarder is the one line
  `@"<target>" %*`, each `%` in the path written `%%`, ended in CR LF.
  v0.05's build output folder holds a forwarder of the same name, to the checkout it was built
  from, so a check of the default line can run from it. The lease CLI's default line is then the interpreter form of
  [state.md, The alias file](state.md#the-alias-file), naming the forwarder. **This forwarder is the
  only place an install reaches into a checkout, and it ends with v0.06.**
- **v0.06's install adds the other programs.** The lease CLI then finds
  `agentspace-ssh-proxy.exe` first. The install replaces `agentspace-ssh-proxy.cmd` with one that
  runs the `.exe` beside it instead, and keeps it while a `ready` lease's alias file names it
  ([above](#proxycommand-lines-already-on-the-laptop)). **v0.06's forwarder is one fixed line,
  `@"%~dp0agentspace-ssh-proxy.exe" %*`, ended in CR LF**, which finds the `.exe` in its own
  folder when it runs, so it names no path and a build writes it like any other file
  (architect, 2026-11-19, for v0.06, gap FB-16). From v0.06 on, an install is a copy again: no
  file it ships has a path written into it.
- **The keeper's task is registered again by the rewrite's installer**, after the prototype's
  keeper is stopped as [keeper.md, Stopping and starting the keeper](keeper.md#stopping-and-starting-the-keeper)
  says. The new task replaces the old one of the same name, and the installer removes the
  prototype's launcher ([keeper.md, The installer](keeper.md#the-installer)).
- **The rewrite and the prototype share one home folder and one state folder**, so a lease that
  one made is the other's to renew and release ([state.md, Between the prototype and the
  rewrite](state.md#between-the-prototype-and-the-rewrite)).

## Which host a command acts on

**This section owns which host each command line and each connection acts on, how the operator
keeps the host registry, how it reaches a host that is not the box, and what `-All` does** (D45,
D47, D49, D50, D55). The files' forms and access rules are [docs/state.md](state.md)'s, cited by
section; this section says when the operator reads and writes each. A rule cited
"architect, 2026-11-20, for v0.07-T01" settles a fact D43 to D55 left open, as the architect ruled
on it for v0.07's specs.

### From the command line

- **Without `-Host` or `-All`, a command acts on the box: the record whose `stack` is `agentbox`**
  (D55; P1, architect, 2026-11-20, for v0.07-T01). Its stack, its alias, its push stamp, its AWS
  calls, its ssh and scp calls and its messages are exactly those this page gave before v0.07, so
  every command line in the repository and every conformance case keeps its meaning (D49). The one
  difference is where its instance ID is kept: in the box's record, not in the retired
  instance-ID cache
  ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)).
- **`-Host <project>` acts on the record whose `project` is `<project>`**, compared exactly, both
  normalized (P1, architect, 2026-11-20, for v0.07-T01). `-Host swimcoach` finds the box's record,
  and so is the same as no `-Host` in every respect, its alias and push stamp included.
- **A record is acted on whatever its `state`**, `creating`, `ready` or `failed`. The state governs
  where a lease is placed and which host a lease's connection reaches (D48, [From the
  ProxyCommand](#from-the-proxycommand)), not what a person's command can do; `provision` works on a
  `creating` host through these same steps.
- **A project with no record fails every command but `provision`**, after the home and system
  folder checks and before any program starts or any AWS call is made, with
  `agentspace: Project '<project>' has no host; provision -Host <project> makes one.`
  (architect, 2026-11-20, for v0.07-T01). **Only `provision` adds a record** ([provision](#provision)), and only
  for a project in the project list (D54), so a mistyped `-Host` never makes a host. `deploy -Host`
  creates the stack of a host that has a record and no stack yet (D49), and never a record.
- **The registry is looked up once per command**, as it starts; `-All` reads every record then
  ([Every host at once](#every-host-at-once--all)).
- **`guard` acts on every record in the registry, every run, and takes no `-Host` or `-All`**
  (D63; [guard](#guard)). It prints no `Host` line, and a project with no record is never named to
  it.

**What a command takes from its host:**

| | The box | Any other host |
|---|---|---|
| Its record | The one whose `stack` is `agentbox`; its `project` is `swimcoach` (D43) | The one whose `project` is the project |
| Its stack | `agentbox` (D44) | The record's `stack`: `agentspace-host-<project>` for every record agentspace adds (D44) |
| The SSH alias its ssh, scp and push use | `agentspace`, the user's own entry ([The `agentspace` SSH entry](#the-agentspace-ssh-entry)) | `agentspace-host.<project>`, the operator's own alias ([How the operator reaches a host](#how-the-operator-reaches-a-host)) |
| How the operator logs in | The user's key, through the user's entry | The operator's key for that host and a one-hour certificate it signs (D47) |
| Where its host key is checked | The user's `known_hosts`, under `agentspace` | `%USERPROFILE%\.agentspace\known_hosts`, under `agentspace-host.<project>` (D47) |
| Its push stamp | `%USERPROFILE%\.agentspace\localswim-pushed.json`, unchanged (D55) | `%USERPROFILE%\.agentspace\hosts\<project>\localswim-pushed.json` (D50, D55) |
| The parameters `deploy` passes | `SshPublicKey` and every member of `stack-parameters.json` but `InstanceType`, and no `HostName` and no `CaPublicKey`, so its change set is empty (D39, D44; P5, architect, 2026-11-20, for v0.07-T01); `InstanceType` at its previous value, or `InstanceType=<type>` when `-InstanceType` is given (D67, D71; architect, 2026-11-23, for v0.09-T01, pin P6) | Creating the stack: `SubnetId` and `VpcId` from `stack-parameters.json`, `SshPublicKey`, `HostName=<stack>` and `CaPublicKey=<the CA's public key>`, every other parameter at the template's default, but `InstanceType` when `-InstanceType` is given. Updating it: every parameter's previous value, but `InstanceType` when `-InstanceType` is given (P13, architect, 2026-11-20, for v0.07-T01; [deploy](#deploy)) |
| What `deploy` records | The stack's `InstanceType` parameter, read back after the deploy, as the box's record's `instance_type` (D67, D71) | The same, in that host's record |
| The `journalctl` hint in start-and-wait's deadline message | `ssh agentspace` | `ssh agentspace-host.<project>` |

### From the ProxyCommand

**The ProxyCommand finds its host from its one argument** (D49; D55's last row; P7, architect,
2026-11-20, for v0.07-T01), in this order:

1. **No argument: the box.** The user's `agentspace` entry passes none.
2. **An argument that starts `agentspace-host.`: a host alias.** The rest of it is the project, and
   the host is that project's record, whatever its `state`, so the operator's own connections reach
   a host that is still `creating`. A rest that is not a normalized project name (lowercase, the
   form of [lease-api.md, Project](lease-api.md#project)), or a project with no record, fails as a
   failed `up` does in caller mode: `agentspace: Could not reach the host. Set
   AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.`, exit 1, no tunnel.
3. **Any other argument: a lease alias.** The ProxyCommand reads the lease state file,
   `<state folder>\leases.json` ([state.md, The lease state file](state.md#the-lease-state-file)),
   without taking the lease state lock, as the keeper does, and never writes it (D49). It looks for
   the record whose `ssh.host_alias` equals the argument exactly, case-sensitively. Only a stored
   `ready` record carries `ssh` (state.md), so a lease in any other state never matches. Then:
   - **A record is found, and its `project`'s host record is `ready`: that host.**
   - **A record is found, and its project has no record, or one that is not `ready`: the box.**
   - **No record is found, or there is no lease state file: the box.**
   - **The lease state file exists and cannot be read, or is not valid** by state.md's checks: the
     box (architect, 2026-11-20, for v0.07-T01). That keeps every swimcoach lease connecting as it did before
     v0.07, when the ProxyCommand never read the file; a lease alias of another host that falls to
     the box fails ssh's strict host key check under `agentspace-host.<project>`, so it never
     reaches the wrong machine (P7). With `AGENTSPACE_DIAGNOSTIC` set, the ProxyCommand prints a
     line naming the file and the problem.
   - **The lease's expiry is not checked here.** A lease whose certificate has expired is refused
     by the host's sshd, as before.
4. **A host registry that cannot be read or is not valid fails the connection** (P9, architect,
   2026-11-20, for v0.07-T01), with the caller's "Could not reach the host" line, and the detail
   naming the file only with `AGENTSPACE_DIAGNOSTIC`
   ([What a caller sees](#what-a-caller-sees-d1-card-0087)).

**What the ProxyCommand then does with that host:** its `up` steps (step 3) act on it, with
`-Alias` the argument it was given, or `agentspace` when it was given none; they read and write that
host's instance ID in its record, and push, unless nested, with that host's push stamp (D50). Step
5 reads that host's instance ID, and step 6 tunnels to it. **So the host a connection reaches and
the host its push goes to are one answer.** An argument of `agentspace` matches no lease, since no
lease alias is `agentspace`, and so reaches the box.

**Why the lease's record, rather than the alias's text.** A lease alias is
`agentspace-<project>-<8 hex digits>`, and a project can hold `-`, so the text alone could name two
projects; the record names one. No lease alias holds a `.`, so none can start
`agentspace-host.` (D47).

### The host registry, as the operator uses it

**`%USERPROFILE%\.agentspace\hosts.json` holds one record per host** (D45): its project, its
stack's name, its state, its cached instance ID, its instance type (D67, from v0.09) and its last
attempt's time. Its form, its schema,
its lock `hosts.lock` and how it is replaced are [state.md, The host
registry](state.md#the-host-registry)'s.

- **A read takes no lock**: the file is replaced atomically, so a reader sees it whole. **Every
  change is made under `hosts.lock`**, held from before the read the change starts from until after
  the file is replaced, by the lease state lock's rules (P10, architect, 2026-11-20, for v0.07-T01).
  The operator never holds `hosts.lock` while it waits for another lock, and never across an AWS
  call or an ssh.
- **A missing file reads as the box's record alone**: project `swimcoach`, stack `agentbox`, state
  `ready`, no instance ID (D45, D55), and, from v0.09, instance type `m9g.xlarge`, only as it is
  read: the record the first change writes holds no `instance_type` ([state.md, The host
  registry](state.md#the-host-registry); architect, 2026-11-23, for v0.09-T01, pins P2c and P31).
  **The first change writes that record with the change** (P9),
  so the box's first command after v0.07 lands reads its instance ID from the `agentbox` stack and
  writes `hosts.json` holding the box's record with it, as it once wrote the instance-ID cache.
- **A file that exists and cannot be opened, or is not valid** (its bytes, its JSON, its schema, two
  records for one project, or anything but exactly one record whose `stack` is `agentbox`, whose
  `project` is `swimcoach`, by state.md's checks; P9 and P13, architect, 2026-11-20, for
  v0.07-T01), **fails the command** at once, before any program starts or any AWS call is made,
  with `agentspace: The host registry <path> cannot be read: <reason>.`, where `<reason>` is one
  sentence fragment naming the first problem found, or the system's own error text, and holds no
  line break (the message is confirmed by the architect, 2026-11-20, for v0.07-T01). **It is never read as missing and never
  repaired**, because it holds state a rebuild would lose. In caller mode the line is "Could not
  reach the host".
- **So every command, with `-Host` or without, finds the box's record in a valid registry.** The
  first write of a missing file always writes it, and nothing removes a record, so only a file a
  person edited can lack it, and such a file is not valid (P13).
- **`hosts.lock` not taken within its 30 seconds** fails with
  `agentspace: The host registry's lock <path> is held by another program; try again.`
  (architect, 2026-11-20, for v0.07-T01). **A file that cannot be written** fails with
  `agentspace: Cannot write <path>.` In caller mode both are "Could not reach the host".
- **A command reads its host's instance ID from its record.** When the record has none, it reads the
  host's stack's `InstanceId` output (`cloudformation describe-stacks --stack-name <stack>`,
  [Calls by command](#calls-by-command)), then writes that ID into the same record under
  `hosts.lock`, changing nothing else in it, and goes on. A stack with no `InstanceId` output fails
  with `agentspace: Stack '<stack>' has no InstanceId output.`, and writes nothing. A stack that
  does not exist fails with the AWS CLI's own text ([AWS failures](#aws-failures)).
- **The ProxyCommand's `up` writes only that: a missing `instance_id`, into its host's existing
  record**, under `hosts.lock`, as it once wrote the cache (P13, architect, 2026-11-20, for
  v0.07-T01). It never writes another member of a record, never changes a record that already
  holds an instance ID, and never adds a record. The box's record that a missing file reads as
  counts as existing here, so the ProxyCommand's write then creates `hosts.json` holding that record
  alone, with its instance ID, as any first change does (architect, 2026-11-20, for v0.07-T01), and
  with no `instance_type`, since the `m9g.xlarge` a missing file implies is never written
  ([state.md, The host registry](state.md#the-host-registry); architect, 2026-11-23, for
  v0.09-T01, pin P31). **A write of the
  ProxyCommand's that fails, or a registry that has become not valid by then, does not fail the
  connection**, which uses the ID it read (architect, 2026-11-20, for v0.07-T01; [state.md, The
  instance ID](state.md#the-instance-id)).
- **What the operator writes in the registry, and nothing more:** a record's `instance_id`, as above
  and as `deploy` removes and writes it again ([deploy](#deploy), step 5); a record's
  `instance_type`, which `deploy` alone writes, `provision`'s deploy step included, and removes
  ([deploy](#deploy), steps 4 and 5; [provision](#provision), step 4.2; D67, D71); and, in `provision`
  alone, a new record and a record's `state`, `last_attempt_at` and `instance_id`, never taking a
  `ready` record out of `ready` ([provision](#provision); D48). **No other command adds a record
  or changes a `state`.** The EC2 driver's writes are [driver.md](driver.md)'s.
- **A record's `instance_type` is the host's instance type as its stack last reported it after a
  deploy** (D67): the stack's `InstanceType` parameter, read with `cloudformation describe-stacks`
  once `cloudformation deploy` has succeeded, and written under `hosts.lock`
  ([deploy](#deploy), step 5). Its form and schema are [state.md, The host
  registry](state.md#the-host-registry)'s, and who writes it when is [state.md, The instance
  type](state.md#the-instance-type)'s, which this page's `deploy` follows (architect, 2026-11-23,
  for v0.09-T01, pins P11, P26 and P30): a provisioning run's deploy step removes it before it
  decides its type, a person's `deploy` that passes `InstanceType` removes it before
  `cloudformation deploy` runs, and every `deploy` writes the type it reads back after success; a
  run's deploy step that fails or is killed after that removal, a person's `deploy` that passed
  `InstanceType` and fails or is killed, and any `deploy` whose read fails or is malformed, leave
  the record with no type, and a person's `deploy` that passed none and fails before its read
  leaves the type as it was. **The
  operator writes it from no other source**: never from `-InstanceType`'s value, never from
  `ec2 describe-instances` (which `status` reads for its report), and never from
  `stack-parameters.json`. The EC2 driver's place reads it, with no AWS call (D66, D67); no
  operator command reads it to decide anything. **A record without it is valid**, and meets no
  size (D67): both hosts' records predate v0.09, and each gains it at its project's first `deploy`
  after the install, which a provisioning run makes. **The ProxyCommand and every other command
  that writes a missing `instance_id` leave `instance_type` exactly as they find it**, as they
  leave every other member, and a program that does not know the member MUST NOT write the file
  (architect, 2026-11-23, for v0.09-T01, pin P12).
- **It is laptop state, never a lease's output** (D45, D1). The operator's own messages MAY name a
  host's project, stack and instance ID, under [Messages](#messages)' rule.
- **The project list, `%USERPROFILE%\.agentspace\projects.json`, is read by `provision` alone** of
  the operator's commands, and never written by agentspace (D54;
  [state.md, The project list](state.md#the-project-list)). **`provision` reads it only for a
  project with no record** (P14, architect, 2026-11-20, for v0.07-T01), so a project list that is
  not valid never fails `provision -Host swimcoach`, or a run on any host that has a record. A
  missing file lists nothing. A file that cannot be opened or is not valid, read for a project with
  no record, fails `provision` with
  `agentspace: The project list <path> cannot be read: <reason>.` (architect, 2026-11-20, for v0.07-T01).

### How the operator reaches a host

**The box is reached through `agentspace`**, the user's entry, with the user's key, exactly as
before v0.07. D47 lets the operator keep using it there; the operator writes no alias, key or
certificate for the box (architect, 2026-11-20, for v0.07-T01).

**Any other host is reached through its own alias, `agentspace-host.<project>`, with a key and a
certificate of the operator's own** (D47; P11, architect, 2026-11-20, for v0.07-T01). **Every
operator command given `-Host` for a host that is not the box writes that host's alias and signs a
fresh one-hour certificate for it** (P13 and P15, architect, 2026-11-20, for v0.07-T01), whether
or not it reaches the host over SSH, so that any such command leaves a person an hour to sign in.
It does these, in this order, once per command:
- **Before its first ssh or scp to the host**, for a command that reaches it: `sync`, `bootstrap`,
  `grow` (before its online step, or after its step 2 when the host is not running), a person's
  `up` (before its push), and `deploy` and `provision` through their `sync` step.
- **After its own work**, for `status` and `down`, which reach no host over SSH: after the five
  lines are printed, or the stop is requested, so a CA that fails its check never keeps a person
  from seeing or stopping a host. A failure there fails the command, exit 1, and the stop already
  requested stands (architect, 2026-11-20, for v0.07-T01).
- **For `sync -All`, for each host that is not the box and that it syncs, as its part with
  `-Host`** ([Every host at once](#every-host-at-once--all); architect, 2026-11-20, for v0.07-T01), since it
  connects to each. **`status -All` and `down -All` MUST NOT sign, and MUST NOT write any alias**:
  they name no host, and the stop and the check of a turn's close (D51) MUST NOT fail on the CA's
  trouble.

The steps:

1. **Checks the CA**, as [sync](#sync)'s step 1 does: `ca`'s steps 1, 2, 3 and 5, each with `ca`'s
   own message. `sync`'s own step 1 is that check, and is not repeated.
2. **Makes the operator's key for the host, `%USERPROFILE%\.agentspace\hosts\<project>\id_ed25519`,
   when it is missing**: an ed25519 key with no passphrase, creating the host's folder if needed,
   in the formats and under the access rules a lease's key has ([state.md, The
   key](state.md#the-key); [state.md, A host's folder](state.md#a-hosts-folder)). It makes the key
   while it holds `hosts.lock`, after looking for `id_ed25519` again once it holds it, so two
   commands never make two keys ([state.md, The operator's key and
   certificate](state.md#the-operators-key-and-certificate)). An existing key is never replaced. A
   failure fails the command with
   `agentspace: Could not make the operator's key <path>: <tool output>`.
3. **Signs a fresh certificate for that key with the CA**, written as
   `hosts\<project>\id_ed25519-cert.pub` and replacing the last one: a user certificate, principal
   `ubuntu`, key ID `operator-<project>`, valid for one hour (D47, D55). Its fields, its window's
   back-dating and its bytes are [state.md, A host's folder](state.md#a-hosts-folder)'s. A failure
   fails the command with
   `agentspace: Could not sign the operator's certificate for '<project>': <tool output>`.
4. **Writes the host's operator alias**, `<ssh folder>\agentspace.d\agentspace-host.<project>`, when
   it is missing or its bytes differ from the ones it would write, and the `Include` line when it is
   missing, by the rules `create` follows for a lease's alias file and its `Include` line (P11;
   [state.md, The `Include` line](state.md#the-include-line)). The file's bytes are [state.md, A
   host's operator alias](state.md#a-hosts-operator-alias)'s: a lease alias's lines, but with the
   owner mark `# agentspace host <project>; <comment>`, `StrictHostKeyChecking accept-new`,
   `HostKeyAlias agentspace-host.<project>` and `UserKnownHostsFile` naming agentspace's known-hosts
   file, the operator's key and certificate, and no `RemoteForward` lines, since the operator holds
   no board. Its ProxyCommand line names `agentspace-ssh-proxy.exe` in the operator's own folder,
   then the alias ([How each program finds the others](#how-each-program-finds-the-others)).
   - **A program that is not there** fails with
     `agentspace: <path> is missing; the operator's alias for a host needs it beside the operator.`
   - **A file at the alias's path whose first line is not this host's owner mark** is not
     agentspace's alias for the host, and is left as it is: the command fails with
     `agentspace: <path> is not agentspace's alias for '<project>'; it was left as it is.` So is
     anything there that is not a file, a link included, as for a lease's alias file.
   - **A file that cannot be written** fails with `agentspace: Cannot write <path>.`
   - **An `Include` line that cannot be added** fails by the kind of the failure
     (agentspace-alias's `AliasFailureKind`), with
     `<config>` the ssh folder's `config` file's full path: a `Config` failure, a configuration a
     person has to fix, with `agentspace: <config> cannot take agentspace's Include line: <why>.`,
     `<why>` being agentspace-alias's reason, and an `Io` failure with
     `agentspace: Cannot write <config>.` (architect, 2026-11-21, for v0.07-T03, on #0152's review).

Each of these messages is confirmed by the architect, 2026-11-20, for v0.07-T01.

- **The host's key is checked under its own name, in agentspace's own known-hosts file** (D47).
  The alias accepts a new key on first use and checks it afterwards; ssh writes it to
  `%USERPROFILE%\.agentspace\known_hosts` under `agentspace-host.<project>`
  ([state.md, Agentspace's host keys](state.md#agentspaces-host-keys)). **The operator never writes
  that file, and never writes the user's `known_hosts` for a host that is not the box.** Accepting
  the first key is sound because the transport is a Session Manager session, opened with the
  user's AWS sign-in, to the instance ID the registry holds (D47).
- **A host whose instance was replaced has a new host key, which the alias refuses**, since
  `accept-new` accepts only a name with no key, and so does every lease alias for that host. A
  person removes the old key, after making sure the instance was replaced, with
  `ssh-keygen -R agentspace-host.<project> -f %USERPROFILE%\.agentspace\known_hosts`, the path
  written out, and the operator's next connection accepts the new one
  ([state.md, Agentspace's host keys](state.md#agentspaces-host-keys)).
- **Two commands for one host at once**, such as a detached run and a person's
  `status -Host <project>`, each write the key, the certificate and the alias safely, by
  [state.md, A host's folder](state.md#a-hosts-folder)'s rules: the key is made under `hosts.lock`
  and only when still missing, so there is never a second key; a new certificate replaces the old
  in one rename, so ssh reads one whole certificate, either command's, each valid for an hour; and
  the alias and the `Include` line are each written through a temporary file moved into place, the
  alias only when its bytes differ, so both commands write the same bytes and ssh reads a whole
  file.
- **No lease's release or overwrite ever matches the operator's alias**, by its owner mark (D55).
  The operator never removes it: it stays for the host's life, and each command that reaches the
  host writes it again only when its bytes differ, so a newer install's operator moves it to its
  own ProxyCommand at its next use.
- **The ProxyCommand never signs and never writes the alias.** It only finds the host
  ([From the ProxyCommand](#from-the-proxycommand)). A connection through the alias with a
  certificate more than an hour old is refused by the host's sshd.
- **A person's own `ssh agentspace-host.<project>`**, as D52's sign-ins use it, works for the hour
  after any command that signed. **So a person runs `agentspace-operator status -Host <project>`
  before signing in over `ssh agentspace-host.<project>`, and again after an hour** (P13,
  architect, 2026-11-20, for v0.07-T01), **under a live lease of the project, created before
  either** (D56): without one, the guard stops the host mid sign-in ([Work on a host by
  hand](#what-the-guard-can-race)). `status` never starts a host and changes nothing on it, and its
  report shows whether the host is running. The sign-in procedure (D52), [README, Signing in on a
  new host](../README.md#signing-in-on-a-new-host), and the line `provision` ends with
  ([provision](#provision), step 10), `Sign in on this host once, by hand, under a lease: see
  "Signing in on a new host" in the agentspace README.`, say the same (P20, architect, 2026-11-21,
  for v0.08-T01).
- **The operator's key is a private key**: it MUST NOT be printed, logged or put in any message, and
  no program gets more of it than its path.

### Every host at once: `-All`

**`status -All`, `down -All` and `sync -All` act on every host in the host registry** (D49, D50,
D51, D55). Except where D49, D50 and D55 say it, everything in this section is confirmed by the architect, 2026-11-20, for v0.07-T01.

- **The hosts are every record in the registry, in the file's order.** A missing registry is the
  box's record alone.
- **Before each host, the command prints `Host      <project> (<stack>), <state>`** to stderr,
  where `<state>` is the record's `state`, whatever `-Quiet` says, so every line after it until the
  next is that host's. The label is padded with spaces to 10 characters, as `status`'s are.
- **Each host's part is that command's own steps with that host**, with the same calls, messages,
  streams and exit rules as the command run with `-Host <project>`, but that **`status -All` and
  `down -All` MUST NOT sign a certificate or write an alias for any host**, where `status -Host` and
  `down -Host` do ([How the operator reaches a host](#how-the-operator-reaches-a-host)).
- **`status -All` also reads the lease state file, once, before its first host's part**, and adds
  two lines to each host's report: its count of live leases, and for a running host that no live
  lease explains, the time the guard stops it ([status](#status); D60). **When that file is
  missing, cannot be read or is not valid, every host's report goes without the two lines, and the
  command says why once after the last host's part**: a missing file is not "no leases"
  (architect, 2026-11-21, for v0.08-T01, pin P18). **The lease state file never changes the exit
  code**: it is what the host lines alone make it (architect, 2026-11-21, for v0.08-T01, pin P25).
  `down -All` and `sync -All` never read the lease state file.
- **A host's instance ID is its record's**, or, for a `ready` record without one, read from its
  stack as every command reads it. **A `creating` or `failed` record without one is skipped:**
  `status` prints `Instance  none recorded` after its `Host` line, and `down` prints the progress
  line `Skipping: no instance is recorded for this host.`
- **`sync -All` syncs only the hosts whose record is `ready`**, and prints the progress line
  `Skipping: the host is <state>; its next provisioning run syncs it.` for each other. A `creating`
  or `failed` host's next run syncs it ([provision](#provision)). **A replaced CA does not reach a
  host that is not the box this way**: [ca](#ca) says why, and how a person recovers each such
  host.
- **`sync -All` starts every `ready` host that is stopped, as `sync` does, and stops none.** From
  v0.08 a host it started and no live lease explains is stopped by the guard ([guard](#guard)), so
  a turn's close leaves its hosts to the guard. Until v0.08 is installed (#0186), a turn's close
  that synced every host stops them afterwards with `down -All` (D51), as the README's [A host's
  life](../README.md#a-hosts-life) says.
- **`sync -All` writes `system\` and builds the localswim CLI once**, in its one folder ([The
  operator's folder, one per command](#the-operators-folder-one-per-command)), in the part of the
  first host it syncs, and every later host's part copies the same files. Step 7's skip or build
  message prints in that first part alone. A build that fails fails that host's part, and every
  later host's part fails at the same step with the same message, without building again.
- **A failure on one host ends that host's part, and the command goes on to the next.** At the end,
  when any host failed, it prints `agentspace: <n> of <m> hosts failed.`, where `<m>` is the number
  of records and `<n>` of those that failed, and exits 1. **The exception is #0034's sign-in
  message, which ends the command at once**, since every later host's AWS call would fail the same
  way.
- **`down -All` stops a host whose provisioning run is still working on it**, and that run then
  fails at its next step. A turn's close (D51) comes after its hosts' runs are done.

### Which checks show it

**Every behavior v0.07 adds to this page is shown either by a conformance case, against stand-ins,
in E0.07-T04-S02, or by a live check in E0.07-T05-S01** (D53, D55), as the coverage reading for
v0.07-T01 found. A stand-in cannot show a real key exchange, sshd's trust of the CA, a real
instance or a real Session Manager session; and `go` and `ssh-keygen` have no seam
([Q30](#open-questions)).

| Behavior | Shown by |
|---|---|
| The command line: `-Host`'s and `-All`'s forms, every new usage error and host check in its order, and every help text byte for byte | Conformance case, E0.07-T04-S02 |
| Every command line without `-Host` acting on the box exactly as before, each existing case passing with `hosts.json` seeded where the instance-ID cache was | Conformance case, E0.07-T04-S02 |
| `-Host` picking its host's stack, instance ID and alias for `up`, `down`, `status` and `grow`, and its start and wait (one path or two) | Conformance case, E0.07-T04-S02 |
| `sync -Host` and `bootstrap -Host` naming the host's alias, `agentspace-host.<project>`, in every ssh and scp they start | Conformance case, E0.07-T04-S02 |
| `-Host swimcoach` the same as no `-Host` for every command: the `agentbox` stack, the `agentspace` alias, the box's push stamp, and no key, certificate or alias written | Conformance case, E0.07-T04-S02 |
| `up -Host <project> -Alias <alias>`: the host's instance ID and push stamp, and the push's ssh naming `<alias>`; a signing failure warned with its cause, and exit 0 | Conformance case, E0.07-T04-S02 |
| A leftover `instance-id.json` ignored: a command reads its host's instance ID from the registry or the stack, never from the file, and leaves the file as it was | Conformance case, E0.07-T04-S02 |
| `hosts.lock` held by the check past its 30 seconds: a command that would write the registry fails with the lock's message and writes nothing | Conformance case, E0.07-T04-S02 |
| A project list that is not valid: `provision` for a project with no record fails with the list's message; `provision -Host swimcoach` and a run for a project with a record go on | Conformance case, E0.07-T04-S02 |
| A project with no record refused by every command but `provision`; `provision` refusing an unlisted project and writing nothing | Conformance case, E0.07-T04-S02 |
| The host registry: missing read as the box; the first write holding the box's record; an invalid file failing with its name; a missing instance ID read from the stack and written; `deploy`'s removal and write | Conformance case, E0.07-T04-S02 |
| `deploy -Host`'s `cloudformation deploy` arguments: the existence check first; for a new stack, the stack's name, its tag, `SubnetId` and `VpcId` alone of `stack-parameters.json`, `SshPublicKey`, `HostName`, and `CaPublicKey` with no comment; for an existing stack, no parameter at all; the box's passing every member of the file and neither `HostName` nor `CaPublicKey` | Conformance case, E0.07-T04-S02 |
| The existence check's `StackStatus`, through the stand-in `aws` (P17 and P19, architect, 2026-11-20, for v0.07-T01): `REVIEW_IN_PROGRESS` takes the create path, with the creation parameters; `ROLLBACK_COMPLETE` fails with the delete message, before any `cloudformation deploy`, and `provision` then records `failed` for a host that was never `ready` and leaves a `ready` host `ready`, the message in its log; a `*_IN_PROGRESS` status fails with the try-later message the same way | Conformance case, E0.07-T04-S02 |
| `stack-parameters.json` naming `HostName` or `CaPublicKey` refused for the box and for a new host; one lacking `SubnetId` or `VpcId` refused for a new host's stack before any call that changes anything, and not for an existing one | Conformance case, E0.07-T04-S02 |
| `-InstanceType`: its form, its two host checks in their order (another command; `deploy` without `-Host` or with `-Host swimcoach`), and `deploy -Host <project> -InstanceType <type>` passing `InstanceType=<type>` alone to an existing stack, and with the creation parameters to a new one | Conformance case, E0.07-T04-S02 |
| The operator's folder per command: `deploy`, `sync` and `provision` each writing into a new `operator\<hex>\` folder that their stand-in `scp` and `aws` see named, never one another's, and removing it as they end, after a failure too; a folder that cannot be written failing with its name | Conformance case, E0.07-T04-S02 |
| The 24-hour sweep: a `<hex>` folder created more than 24 hours ago removed by the next command that makes its own, a younger one and a folder of another name left | Conformance case, E0.07-T04-S02, with the folders' creation times set by the test |
| The ProxyCommand writing a missing `instance_id` into its host's record and nothing else, and leaving a record that holds one unchanged | Conformance case, E0.07-T04-S02 |
| Every command given `-Host` for a host that is not the box, `status` and `down` included, signing a fresh certificate; `status -Host` and `down -Host` signing after their own work | Conformance case, E0.07-T04-S02 |
| `deploy -Host` creating a stack, and the host coming up from it | Live check, E0.07-T05-S01 |
| The `agentbox` change set being empty (D44) | Live, by the template's story, E0.07-T01-S05 |
| `-All` on `status`, `down` and `sync`: the order, the `Host` lines, the skips, going on past a failed host, the closing count, and the sign-in failure ending it; `status -All` and `down -All` signing nothing and writing no alias, with no CA in the throwaway home; `sync -All` writing `system\` and building once | Conformance case, E0.07-T04-S02, for the calls; `sync -All`'s copies to real hosts by the live check, E0.07-T05-S01 |
| `bootstrap`: its ssh's arguments, the script's bytes on its stdin, and its failure message | Conformance case, E0.07-T04-S02 |
| `bootstrap` installing the toolchain, and a second run changing nothing that matters | Live check, E0.07-T05-S01 |
| `provision`: its steps in order, every record it writes (`creating`, `ready`, `failed`, `last_attempt_at`, a kept `instance_id`), its log lines, a failed step's line carrying the program's error text, `place_pending` called after `ready` with its `place done` and `place failed` lines and its failure warning, and the sign-in line for a host that was not `ready` and never for the box or a `ready` host | Conformance case, E0.07-T04-S02, with a seeded lease state file |
| `provision`'s first steps in order: the `-Host` checks, then an unlisted project with no record refused with no log, no host's folder and no record; then the log opened; then the lock. A held `provision.lock` gives one `run failed` line in the log, exit 1 and no record written; a folder at the log's path gives exit 1, no record and no lock taken | Conformance case, E0.07-T04-S02 |
| `place_pending` as [driver.md, place_pending](driver.md#place_pending) gives it, by its effects: a killed placement's leftover files removed first, a lease released meanwhile never placed, a lease that fails left `pending` with its failure logged and the others placed, and every lease left `pending` when the ProxyCommand program is missing beside the operator (P14 and P16, architect, 2026-11-20, for v0.07-T01; architect, 2026-11-21, for v0.07-T04) | Conformance case, E0.07-T04-S02, with a seeded lease state file, and the operator run on the box from a folder without `agentspace-ssh-proxy.exe` for the last |
| `place_pending` takes the lease state lock once per lease, which no program a case starts can see (architect, 2026-11-21, for v0.07-T04) | Crate test, `agentspace-driver-ec2` (`tests\pending_place_pending.rs`) |
| The ready-host rule: `provision` on a `ready` record writing only `last_attempt_at` at its start, keeping `ready` on success with a learned `instance_id` written, and keeping `ready` with the failure logged when a step fails | Conformance case, E0.07-T04-S02 |
| `provision`'s step 3 and step 8 failing on the registry (`hosts.lock` held past its 30 seconds, a write that fails): `run failed` or `record failed` logged, the record left as the section says, exit 1 | Conformance case, E0.07-T04-S02 |
| `provision` making a real host; a detached run outliving the lease command that started it, with its own lines written by the run into `provision.log`, each failed step's line holding the program's own error text | Live check, E0.07-T05-S01 |
| A detached run's environment: the four variables passed, `USERPROFILE` and the relative folders made full, `AGENTSPACE_PROXY_COMMAND` exactly as set, each of the three absent when the lease CLI lacked it, and no seam variable; `AGENTSPACE_PROVISION_CMD` naming the program started | Conformance case of the lease CLI, E0.07-T04-S01, with a stand-in run program that records the names of the variables it got and the values of the four named ones alone, since a test never records the values of a whole environment (AGENTS.md) ([driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run)) |
| The operator's alias file: its bytes, the `Include` line, a foreign file refused, and a lease's release or overwrite leaving it | Conformance case, E0.07-T04-S02 |
| The operator's alias refused when `agentspace-ssh-proxy.exe` is not beside the operator: the command fails with its message, and starts no ssh | Conformance case, E0.07-T04-S02, with the operator run from a folder without it |
| The operator's certificate's fields, read with `ssh-keygen -L` against a throwaway CA | Conformance case, E0.07-T04-S02 |
| A new host accepting the operator's certificate, and its key accepted on first use into agentspace's known-hosts file | Live check, E0.07-T05-S01; a case shows only that the operator never writes the file |
| The ProxyCommand's host: no argument, a host alias, a lease alias of a `ready` host, of a host that is not `ready`, an unknown alias, an invalid lease state file, and an invalid registry | Conformance case, E0.07-T04-S02 |
| The push stamp per host, the box's staying where it was | Conformance case, E0.07-T04-S02 |
| A lease alias reaching the new host; a swimcoach lease connecting to the box as before | Live check, E0.07-T05-S01 (D53) |
| `sync` on a new host: the copies, the install and the CA it trusts | Live check, E0.07-T05-S01 |
| The laptop's AWS sign-in needing no new permission (D44) | Live check, E0.07-T05-S01, by its run with today's sign-in |
| Every command with `-Host` started from Git Bash (#0027) | Live check, E0.07-T05-S01 (D55 carries #0027 there) |

**Every behavior v0.08 adds to this page, or changes on it, is shown by a conformance case against
stand-ins in E0.08-T04-S01 (the keeper's start of the guard in E0.08-T04-S03), by a test of a crate
or of `box/tests`, or by a live check of D62's in E0.08-T05-S01, E0.08-T06-S01 (the box) or
E0.08-T06-S02 (agentspace's host)** (D62, D63). No stand-in can show a host stop, a real Run
Command or a real session, and the suite has no fake clock, so the grace's timing is a live check.
**Where a stand-in cannot show a behavior, such as a time format at a chosen hour or a shell
command's answer under a stand-in `systemctl`, a crate's test or the builder's own test of the
text is accepted in place of a case**, and the row names it.
A case that runs the operator against stand-ins seeds the lease state file in its throwaway home's
state folder, with times set relative to the real clock, and never reads Terry's real files.

| Behavior | Shown by |
|---|---|
| The command line: `guard` accepted, with `-Quiet` alone; `guard -Host` refused by check 1 with its message, `ca`'s unchanged byte for byte; `guard -All`, `-Alias` and `-InstanceType` refused with their messages; `guard -GiB 5` checked for form and ignored; the two command-list messages naming `guard`; the top level's, `guard`'s, `status`'s, `grow`'s and `sync`'s help byte for byte (`conformance/operator/test-unmet-usage-help.ps1`'s copies updated) | Conformance case, E0.08-T04-S01 |
| `grow -GiB 0`, `-0`, `+0`, `-1` and `-2147483648`: exactly the refusal message on stderr, nothing on stdout, exit 1, no AWS call at all (no `ec2 modify-volume` counted), no ssh, nothing written in the home; with `-Host agentspace`, no key, certificate or alias written either. **`conformance/operator/test-unmet-commands.ps1`'s `-GiB 0` case, which the AWS stand-in's model of EC2's refusal served, and its `-GiB -1` case, which asks EC2 for the size less 1, were rewritten to the refusal by E0.08-T03-S02 (#125), and stay beside these new cases** (D61; P7, architect, 2026-11-21, for v0.08-T01; #0182; the overlap kept by the architect, 2026-11-22, for v0.08-T04). **The refusal with `USERPROFILE` unset is the operator crate's test** (`grows_gib_refusal_is_a_usage_error_that_needs_neither`), since the harness never runs a program with it unset | Conformance case, E0.08-T04-S01 |
| `grow -GiB 1` asking EC2 for the size plus 1; the refusal's place in the order (`grow -Alias x -GiB 0`, `grow -All -GiB 0`, `grow -InstanceType m9g.xlarge -GiB 0` each giving its own message, `grow -Host nobody -GiB 0` and `grow -GiB 0` with `USERPROFILE` unset giving the refusal, `grow -GiB 0 -h` grow's help); `status -GiB 0`, `down -GiB -5` and `sync -GiB 0` not refused | Conformance case, E0.08-T04-S01 |
| `status -All`'s `Leases` line for every host: `pending` and unexpired `ready` leases counted; an expired `ready`, a `released` and an `unsatisfiable` lease not counted; another project's leases not counted, the project compared exactly; the box counting swimcoach's; a `creating` host with no instance ID showing its count after `Instance  none recorded` | Conformance case, E0.08-T04-S01 |
| `status -All`'s `Guard` line, its padding and its time in the laptop's zone, read by the case from the same system: from the launch time, from a released lease's `released_at` and from an expired lease's `expires_at`, each when it is the later; an `unsatisfiable` lease adding no end; a past-due host showing the report's minute (the case accepts that minute or the next); no `Guard` line for a host a live lease explains, for a `stopped`, `stopping` or `pending` host, or for `Instance  none recorded` | Conformance case, E0.08-T04-S01, with launch and lease times relative to the run's time so the stop time falls inside the grace |
| `status -All` over a missing lease state file, one that is not valid, one holding two records with one `id`, a folder at its path, and one that cannot be opened: each with every host's five lines and no `Leases` or `Guard` line, the lease state file's message once, on the stream the host lines go to, after the last host and before any closing count (`the file does not exist` for the missing one, architect, 2026-11-21, for v0.08-T01, pin P24), the exit code the one the host lines alone give, 0 when every host's part passed and 1 when one failed, and the same AWS calls (P18, architect, 2026-11-21, for v0.08-T01; architect, 2026-11-21, for v0.08-T01, pin P25); `AGENTSPACE_STATE_DIR` moving it; a running unexplained host with no `LaunchTime` failing its part alone; the same AWS calls as before v0.08; the file's bytes unchanged; a run finishing while the case holds the lease state lock; the lines, the lease state file's message among them, printed whatever `-Quiet` says; #0034's message ending it with the lease state file's line unprinted | Conformance case, E0.08-T04-S01 |
| `status` and `status -Host` printing the five lines alone, with a lease state file that is not valid in the home; `status -All`'s `Leases` and `Guard` lines shown by `conformance/operator/test-unmet-status-leases.ps1`; `test-unmet-host-commands-all.ps1`'s `status -All` reports, in homes with no lease state file, gaining that file's one line instead, as E0.08-T03-S02 (#125) left them (row corrected by the architect, 2026-11-22, for v0.08-T04) | Conformance case, E0.08-T04-S01 |
| `provision`'s step 10 line, `Sign in on this host once, by hand, under a lease: see "Signing in on a new host" in the agentspace README.`, byte for byte, as the run's last line for a host that was not `ready`, and never for the box or a `ready` host (P20, architect, 2026-11-21, for v0.08-T01) | Crate test of `agentspace-sync`, E0.08-T03-S05 (#0181), which builds the line and changes the crate's tests that assert v0.07's; conformance case: `conformance/operator/test-unmet-host-provision.ps1`'s expected line, changed by E0.08-T03-S05 itself (#123) so main's suite stayed green at its merge (architect, 2026-11-22, for v0.08-T03), not by E0.08-T04-S01 as first planned |
| The 12-hour form at every hour: `12:05am` after midnight, `11:59am`, `12:00pm`, `1:07pm`, the minute rounded up, and a date on each side of a daylight saving change | Crate test, E0.08-T03-S02, over the formatting function with fixed times and zones, since a case cannot choose the hour of its run |
| `guard.lock` held by the case: exit 0, nothing on stdout or stderr, no line in `guard.log`, no call the stand-in records; `guard.lock` that cannot be taken (a folder at its path): exit 1, its message, its `failed step=lock` line, no AWS call | Conformance case, E0.08-T04-S01 |
| The host registry missing (the box alone) or not valid (`failed step=hosts`); `AGENTSPACE_STATE_DIR` moving the lease state file and not `hosts.json`, `guard.lock` or `guard.log` | Conformance case, E0.08-T04-S01 |
| The lease state file missing, not valid, and holding two records with one `id`, each with a running host the leases would leave unexplained and past its stop time: the lease state file's message on stderr (`the file does not exist` for the missing one), exactly one `failed step=leases` line in `guard.log` with no `host=` field, no AWS call the stand-in records (so no `ec2 stop-instances` and no `ssm send-command`), and exit 1 (D36; P18, architect, 2026-11-21, for v0.08-T01) | Conformance case, E0.08-T04-S01 |
| A `ready` record with no instance ID read from its stack, `hosts.json`'s bytes unchanged after the run; a `creating` or `failed` record with none skipped with no call; a stack read that fails giving that host's `failed step=instance` line while the others are guarded | Conformance case, E0.08-T04-S01 |
| One `ec2 describe-instances` per run, its filter naming every ID in the registry's order; none when no host has an ID; its failure, and an answer that is not JSON, giving `failed step=describe`, no stop, no send, exit 1; a host missing from the answer giving its `failed step=instance` line while the others are guarded; a running host with no `LaunchTime`, explained by a live lease or not, giving its `failed step=instance` line at its place in the registry's order, no stop and no send | Conformance case, E0.08-T04-S01 |
| The decision, against seeded leases and launch times: a `ready` lease explains its host (no line, no stop); so does a `pending` one; a released lease's end, an expired lease's `expires_at` and a lease released after it expired (its `expires_at`) as the last lease end (P11); an `unsatisfiable` lease ignored; a project with no lease counted from `LaunchTime`; the later of `LaunchTime` and the last lease end as `since=`; before the stop time an `unexplained ... stops=` line and no stop; at or after it one `ec2 stop-instances` and a `stopped since=` line; a host not `running` getting no line, no stop and no send; the box decided like any host | Conformance case, E0.08-T04-S01 |
| A due host whose `provision.lock` the case holds: not stopped, its `unexplained ... provisioning=running` line, its horizon still sent; a missing `hosts\<project>\` folder or lock file read as free, and not created; a folder at the lock's path giving `failed step=lock` and no stop | Conformance case, E0.08-T04-S01 |
| The horizon: one `ssm send-command` per running host the run did not stop, every run, with the comment `agentspace guard: lease horizon`, the document `AWS-RunShellScript`, and the expected Unix seconds as one word of the command: the latest live `expires_at`; the run's time for a project whose only live leases are `pending` (P10); else the last lease end; else `LaunchTime`; never earlier than `LaunchTime` (P12); no comma, bracket or equals sign in the command; no `ssm get-command-invocation`; a send that fails giving `failed step=horizon` and exit 1, and the next run sending again; a sent horizon logging nothing | Conformance case, E0.08-T04-S01 |
| A stop that fails: `failed step=stop`, the run going on, the host sent its horizon, exit 1, and `agentspace: <n> of <m> hosts failed.` last; #0034's sign-in text from the stand-in at each call in turn (stack read, `describe-instances`, a stop, a send): its message, one `failed step=<step>` line with no `host=`, no later call, exit 1, a stop already requested left standing | Conformance case, E0.08-T04-S01 |
| `guard.log`'s bytes: UTF-8 without a byte order mark, LF endings, each line's exact form and field order, `host=` on host lines alone, `<time>`, `since=` and `stops=` in UTC whole seconds, `reason=` last with control characters as `?`; nothing for an explained host or a sent horizon; `guard.log` that cannot be written (a folder at its path): the run's stops and sends made as without it, the "Cannot write the guard's log" message once, exit 1 | Conformance case, E0.08-T04-S01 |
| `guard`'s stdout always empty; its progress lines and closing line on stderr without `-Quiet`, `<h:mm><am\|pm>` in the laptop's zone; none of them with `-Quiet`, the failures and the count line still printed, the log the same either way | Conformance case, E0.08-T04-S01 |
| What the guard never does: no ssh or scp started (a stand-in `ssh` set and never called), no `operator\` folder, no key, certificate or alias written with no CA in the throwaway home, no `hosts.lock` or `leases.lock` created, `leases.json` unchanged | Conformance case, E0.08-T04-S01 |
| The keeper starting `guard` at its first pass and at most once every 300 seconds, never while `guard.lock` is held, with the environment P4 gives, `AGENTSPACE_GUARD_CMD` naming the program | Conformance case of the keeper, E0.08-T04-S03, with a stand-in guard program that records the names of the variables it got and the values of the named ones alone (AGENTS.md) ([keeper.md, Starting the guard](keeper.md#starting-the-guard)) |
| `sync`'s patch wait in its place: after row 4 and before row 5, its calls in order (`ssm describe-instance-information`, then `ssm send-command` with the comment `agentspace sync: patch check`, then `ssm get-command-invocation`), with no `ec2 describe-instances` of its own, and no wait when row 4 fails; its messages, `Checking that <id> has finished this boot's patch run...` and `Patched: <id>`; `pending` then `patched` printing the patching message once | Conformance case, E0.08-T04-S01, through the stand-in `aws`, `ssh` and `scp`, ordered by their logs' times |
| `patched` after a reboot: the wait through it (`Online`, `pending`, not `Online`, a failed `ssm send-command`, `Online`, `pending`, `patched`), each phase message printed again when it changes, `Patched: <id>` once, rows 5 to 9 run and exit 0; only `patched` and `failed` ending it (`unknown`, an empty answer, an unexpected answer, an invocation ended in `Failed`, its output `failed` included, and 20 polls without an end each keeping it waiting); a host launched more than 20 minutes ago still asked; #0034's sign-in failure at `ssm send-command` failing `sync` at once and ending `sync -All`; a failed `ssm describe-instance-information` failing `sync`; `-Quiet` and `sync -All` | Conformance case, E0.08-T04-S01, with a stand-in `aws` whose answers change from call to call |
| A `failed` answer (P21): after `pending`, and as the first answer, the wait ending at that answer with no further `ssm` call, the warning `agentspace: The patch run on <id> failed; sync goes on without it. Check: ssh agentspace journalctl -u agentspace-patch` exactly, once, on stderr, with the host's alias for a host that is not the box; no `Patched` line; rows 5 to 9 run; `sync` exiting 0 when nothing else failed, and 1 when a later row fails, with that row's message last; the warning withheld by `-Quiet`, the exit code the same; `sync -All` not counting the host as failed; `provision`'s step 6 logging `sync done; the patch run failed, see the host's journal` exactly, the host recorded `ready`, and the run exiting 0; `up` on a host launched less than 20 minutes ago ending its wait at `failed` with `Ready: <id>` and no other message | Conformance case, E0.08-T04-S01, with a stand-in `aws` whose answers change from call to call |
| The patch check's command answering `failed` for a unit in systemd's `failed` state without a start job queued, `pending` for one with a start job queued, `pending` for an enabled unit otherwise, and `unknown` for one not enabled | A test of the command's text by its builder, E0.08-T03-S05 (#0181), run under bash with a stand-in `systemctl` first on `PATH` |
| A `failed` unit with a start job queued answering `pending` on a real host (architect, 2026-11-21, for v0.08-T01, pin P24): `systemctl kill agentspace-patch.service` while its run is under way on agentspace's host, `failed` answered by the patch check, then `sync`, its install starting the run again, and its patch wait not ending on the stale failure | Live check, E0.08-T05-S01, on agentspace's host |
| A run that never finishes: `pending` answered until the wait's 30-minute deadline, then its exact message, with the host's alias for a host that is not the box, exit 1, and no row 5; `provision`'s and `deploy`'s step 6 making the wait's calls, and a provisioning run whose wait runs out logging `sync failed reason=<id> did not finish its patch run within 30 minutes; ...` | Conformance case, E0.08-T04-S01, the deadline run only when asked for, as `test-unmet-start-paths.ps1`'s `deadline` is: it waits the real 30 minutes (about 32), since no seam shortens it |
| #0163: the install starting `agentspace-patch.service` with no marker, `Patch run: started for this boot, as /run/agentspace/patched is missing`, and `sync` waiting for it and going on with no 15-minute wait; `provision -Host agentspace`'s patch step the same; a `sync` while the run is under way issuing the same `systemctl start --no-block`, printing `Patch run: already running this boot; not started again`, and the start merging into the run's job, no second run in the unit's journal | Live check, E0.08-T05-S01, on agentspace's host with this boot's `/run/agentspace/patched` removed by hand first (D61's change); no new host is made in v0.08 |
| The install starting nothing when the marker exists, with `Patch run: finished this boot; not started again`; a patch reboot during the wait, if the run asks for one, which T05 cannot force (its record says whether it did) | Live check, E0.08-T05-S01, by `sync` on both hosts |
| #0166: `/etc/environment`'s `PATH` lines as [What sync installs](#what-sync-installs) gives them, its other lines kept, the `Installed:` line's `(added)` then `(already there)` at a second `sync`, and `ssh <alias> 'command -v rustc cargo codex claude gh'` finding all five | Live check, E0.08-T05-S01, on both hosts after their `sync` (D62); the edit's edges by S05's own test of `install.sh`, where it has one |
| The idle check with no usable horizon (missing, empty, a folder, non-digits, a CR, 16 digits): D11's rule alone, CPU work keeping the host up; past `<h>` plus 60 times `<g>` with no session, no patch run and CPU busy, the exact `Past lease horizon <h:mm><am\|pm> plus <g> min with no session; powering off` line and one `systemctl poweroff`; one second before it, D11's rule; a session past the horizon keeping the host up with its `Active:` line; a patch run past the horizon, with no session and CPU busy, holding the host up with `Active: Patching` (P9); `HORIZON_GRACE_MINUTES` read from the settings file, 15 when it is not set, and an invalid one warned and replaced by 15; `date` unable to format giving `@<h>`; D11's lines and limits unchanged | `box/tests` case, E0.08-T01-S05's test of the idle check, with the three seams and stand-ins on `PATH`; run against T03's build by E0.08-T04 |
| The check on a host with no seam set: the laptop away, a CPU-busy process left on agentspace's host, the keeper stopped, and the host powering itself off past its horizon plus the grace, its journal holding the line | Live check, E0.08-T05-S01 (D62, "The laptop away") |
| `/run/agentspace/lease-horizon` on a real host after a guard run: one line of digits and an LF, root-owned, 0644; the command's comment in Session Manager's history; the real AWS CLI accepting the filter form of `ec2 describe-instances` and the command's `--parameters` | Live check, E0.08-T05-S01, read over ssh on agentspace's host |
| The alarm: `Period` 3600, `EvaluationPeriods` 24 and the description; `agentbox`'s change set holding only the alarm's modification | The template's story, E0.08-T01-S04, read live before its merge; deployed, and the alarm read back, by E0.08-T05-S01 (D62, "The alarm") |
| The laptop's AWS sign-in needing no new permission for the guard | Live check, E0.08-T05-S01, by its run with today's sign-in |
| #0167: no push at a start with a current stamp, through `up` and through the ProxyCommand; one `The host is ready.` from a ProxyCommand connection whose push is due; two with `AGENTSPACE_DIAGNOSTIC` set; a push that fails still showing its ssh's error lines; a person's `up` whose push is due still printing the nested line | Conformance case, E0.08-T04-S01 (P14, architect, 2026-11-21, for v0.08-T01), with a stand-in `ssh` that runs the ProxyCommand program nested before it answers as the host; the push's stderr choice by unit tests of `agentspace-push` and `agentspace-ssh-proxy`, E0.08-T02-S04 (#0176) |
| A host `stopped` within D56's bound after its project's last live lease is released, measured from `released_at`; the same after a lease with a short TTL expires, measured from its `expires_at`; a quiet held lease, with no session but the holder's and no CPU work, keeping its host past the grace; a new lease's first connection starting the stopped host, the time to a prompt recorded (D62); `status -All`'s `Leases    <n> live` matching the leases held before and after each | Live check, E0.08-T06-S01 (the box) and E0.08-T06-S02 (agentspace's host) |
| A host started with `up -Host agentspace` and no lease: reported by `guard.log`'s `unexplained` line and by `status -All`'s `Guard` line at the same time, then `stopped` within the bound | Live check, E0.08-T06-S02 (agentspace's host only; D62, "Unexplained"); instance and volume IDs masked in the record |
| A send to a host whose Session Manager agent is not yet online failing, and the next run's send reaching it | Live check, E0.08-T05-S01 or E0.08-T06, by `guard.log` over a host's start; not required by D62, recorded where it is seen |
| The real AWS CLI's text for an expired sign-in caught by the sign-in test | Not shown: [Q26](#open-questions) stays open |

**Every behavior v0.09 adds to this page, or changes on it, is shown by a conformance case against
stand-ins in E0.09-T04-S02, by a test of a crate, or by a live check of D70's in E0.09-T05-S01, on
agentspace's host** (D70, D71). **No stand-in can show a host's type change** (D70): a stand-in
`aws` shows the arguments the operator passes and the answers it reads, and only a live check shows
CloudFormation changing the instance's type and the host coming back at it. Every case runs in a
throwaway home with a seeded host registry and, for `provision`, a seeded lease state file whose
times are set relative to the real clock, and never reads Terry's real files (AGENTS.md).
conformance.md's coverage table is read for every "None" these rows change (D70). **Two of
v0.07's rows above are superseded in part**: the `-InstanceType` row in its second host check,
since from v0.09 `deploy -InstanceType` without `-Host` or with `-Host swimcoach` is accepted; and
the `deploy -Host` arguments row in "the box's passing every member of the file", since the box's
`deploy` no longer passes the file's `InstanceType`. The rows below show both.

| Behavior | Shown by |
|---|---|
| The command line: `deploy -InstanceType m9g.2xlarge` without `-Host`, with `-Host swimcoach` and `-Host SwimCoach`, and with `-Host <project>`, each passing the host checks and going on to the home and the system folder (shown with `SystemRoot` unset, or by the AWS stand-in being reached); v0.07's check-6 message printed by no line; check 5 unchanged (`up`, `status`, `grow`, `guard` and `provision -Host x` given `-InstanceType` each refused with its message, in its place in the order); `-InstanceType`'s form check unchanged; the top level's, `deploy`'s and `provision`'s help byte for byte (`conformance/operator/test-unmet-usage-help.ps1`'s copies updated); `test-unmet-host-commands-line.ps1`'s check-6 cases removed by #0202, the acceptance shown by `test-unmet-host-deploy-size.ps1` (architect, 2026-11-24, on #0206's review) | Conformance case, E0.09-T04-S02; the parse in process by the operator crate's tests, E0.09-T02-S03 (#0202); `agentspace-sync`'s own copy of v0.07's check-6 refusal removed, with the cases that require it, by E0.09-T03-S02, in `crates/agentspace-sync/src/lib.rs`, `crates/agentspace-sync/src/tests.rs` (`instance_type_for_the_box_is_refused_before_anything_runs`) and `crates/agentspace-sync/tests/host_deploy_sized.rs` |
| The box's `deploy` arguments: `SshPublicKey` and every member of `stack-parameters.json` but `InstanceType`, with a file that names `InstanceType` and one that does not; no `HostName` and no `CaPublicKey`; with `-InstanceType m9g.2xlarge`, `InstanceType=m9g.2xlarge` last (pin P32); the same with `-Host swimcoach`; the file left byte for byte as it was | Conformance case, E0.09-T04-S02, through the stand-in `aws`'s log of its arguments |
| `deploy -Host <project>`'s arguments with and without `-InstanceType`, creating and updating, as v0.07's rows give them | Conformance case, E0.09-T04-S02 (v0.07's cases, kept passing) |
| The record of the type: after a successful `cloudformation deploy`, the stand-in's `describe-stacks` answer's `InstanceType` written as the record's `instance_type`, with its `instance_id`, in one write, every other member and every other record unchanged; for the box and for another host; with and without `-InstanceType`; a type the stand-in changes between two deploys recorded anew; `hosts.json` valid by [state.md, The host registry](state.md#the-host-registry) after each | Conformance case, E0.09-T04-S02 |
| A failure after a successful `cloudformation deploy`, with and without `-InstanceType`: `describe-stacks` failing (its text, exit 1, no `instance_id`, no `instance_type`, no `sync` step); #0034's sign-in text there; an answer with no `InstanceType` element, a non-string value, a value not of the registry's form, and two `InstanceType` elements, each giving `agentspace: Stack '<stack>' has no InstanceType parameter.`, exit 1, the `instance_id` written alone, no `instance_type` and no `sync`; an answer with no `InstanceId` output writing neither; `hosts.lock` held by the case past its 30 seconds at step 5's removal giving the lock's message, exit 1, no `describe-stacks` after it, and the record keeping its `instance_id`, and its `instance_type` without `-InstanceType`; the lock held at step 5's write of the ID and type, after a removal the case lets through, giving the lock's message, exit 1, and the record with neither | Conformance case, E0.09-T04-S02 |
| A person's deploy's step 4: with `-InstanceType`, a record's `instance_type` removed before `cloudformation deploy` (the stand-in `aws` copying `hosts.json` when it is called for the deploy), whether it held another type or the same type, and a record holding none left without one; no write there without `-InstanceType`, and the record's type kept through a `cloudformation deploy` that fails (pin P30); with `-InstanceType`, a `cloudformation deploy` that fails leaving the record without `instance_type`; `hosts.lock` held at step 4 giving the lock's message, exit 1, and no `cloudformation deploy` called | Conformance case, E0.09-T04-S02 |
| A person's deploy's step 4 with no `hosts.json` (architect, 2026-11-23, for v0.09-T01, pins P2c, P11 and P31): `deploy -InstanceType m9g.2xlarge`, without `-Host` and with `-Host swimcoach`, writing a `hosts.json` that holds the seed document's box record alone, with no `instance_type`, before the stand-in `aws`'s `cloudformation deploy` is called (the stand-in copying `hosts.json` when it is called for the deploy); then, after a successful deploy, the record holding the type the stand-in's `describe-stacks` answers; plain `deploy` with no `hosts.json` writing no `hosts.json` before `cloudformation deploy` | Conformance case, E0.09-T04-S02 |
| A person's `deploy` refused while its host's `provision.lock` is held (architect, 2026-11-23, for v0.09-T01, pin P34): with the lock held by the case, `deploy`, `deploy -Host swimcoach` and `deploy -Host <project>`, each with and without `-InstanceType m9g.2xlarge`, exiting 1 with exactly `agentspace: A provisioning run or deploy for '<project>' is already running; the deploy did not run.` on stderr (`swimcoach` for the box; pin P36), nothing on stdout, no call the stand-in `aws` records (no existence check, no `cloudformation deploy`, no `describe-stacks`), no ssh or scp started, and no file changed in the throwaway home (`hosts.json` byte for byte, its `instance_type` kept, no `operator\` folder, `provision.log` unchanged or absent); the order: a usage error, a host check and a project with no record each reported before the lock, and a missing `id_ed25519.pub` reported after it, so the refusal comes first; a folder at the lock's path giving `agentspace: Cannot take <path>: <error>.`, exit 1 and no AWS call; with the lock free, the lock held from before the stand-in `aws`'s first call to the end of the `sync` step (the stand-in `aws` and `ssh` each trying it and finding it held), and free once `deploy` has exited | Conformance case, E0.09-T04-S02 |
| `provision`'s lease state lock held (architect, 2026-11-23, for v0.09-T01, pin P35): with `leases.lock` held by the case past its 30 seconds, the run's one `deploy` line `<time> deploy failed reason=The lease state lock <path> is held by another program; try again.`, with no `type` field and the reason without the `agentspace: ` prefix, `agentspace: The lease state lock <path> is held by another program; try again.` as the run's last stderr line, exit 1, no call the stand-in `aws` records, the record's `instance_type` kept, a `ready` host left `ready` and a `creating` host recorded `failed`, and the leases `pending` | Conformance case, E0.09-T04-S02 |
| `provision`'s lease state lock that cannot be taken for another reason (pin P35): a folder at `leases.lock`'s path giving `<time> deploy failed reason=Cannot take <path>: <error>.` in the log, `agentspace: Cannot take <path>: <error>.` as the run's last stderr line, `<path>` the lock's full path, exit 1, no call the stand-in `aws` records, the record's `instance_type` kept, and the leases `pending` | Conformance case, E0.09-T04-S02 |
| `provision`'s type, against a seeded lease state file: `pending` leases of sizes `s` and `l` giving `InstanceType=m9g.2xlarge`; one of `m` alone `m9g.xlarge`; one of `xxl` alone no type; a `pending` `l` whose `driver` is not the EC2 driver's name, or that has none, ignored; another project's `pending` `l` ignored; a live `ready` lease of the project beside a `pending` `l` giving no type, the deploy still recording the type it reads back; an expired `ready` record beside it not counting, so the type is given; `released` and `unsatisfiable` records ignored; no lease state file giving no type and a run that goes on; `AGENTSPACE_STATE_DIR` moving the file read, and the lock taken; `-Host swimcoach` finding swimcoach's leases. The locks (pin P26): the record's `instance_type` removed before the stand-in `aws` is first called, whether or not a type is then passed, and `leases.lock` and `hosts.lock` both free when it is called (the stand-in trying each); the lease state lock's failures in their own rows, beside the `deploy` refusal's; a `cloudformation deploy` that fails, with a type and without one, leaving the record without `instance_type` | Conformance case, E0.09-T04-S02; `resize_target` itself by `agentspace-driver-ec2`'s tests (driver.md's rows); the run's use of it by `agentspace-sync`'s tests, E0.09-T03-S02 |
| `provision`'s lease state file not valid (bad JSON, two records with one `id`, a folder at its path): `deploy failed reason=The lease state file <path> cannot be read: <reason>.` in the log, no AWS call the stand-in records, a `ready` host left `ready`, a `creating` host recorded `failed`, the record left without `instance_type` (pin P26), the leases `pending`, exit 1 | Conformance case, E0.09-T04-S02 |
| `provision`'s log lines: `deploy done type=<type>` and `deploy failed type=<type> reason=<message>` when a type was found, with the field in its place; `deploy done` with no `type` when none was; a resize on a `ready` host running every step (`deploy`, `start`, `sync`, `bootstrap`, `record`, `place`), its record `ready` throughout, and on a failed `cloudformation deploy` the record still `ready`, without `instance_type`, the leases `pending` and exit 1 | Conformance case, E0.09-T04-S02 |
| `place_pending` placing only the leases the host meets, and its answer for the rest | driver.md's rows for v0.09 (E0.09-T04-S01 and the driver crate's tests); this page's run only logs what the call returns |
| Step 9's lines (architect, 2026-11-23, for v0.09-T01, pin P37), against a seeded lease state file, with the stand-in `aws`'s `describe-stacks` answering a type smaller than some `pending` lease's size: `place done placed=<n>` first, then, in the order the call tried the leases, `place failed lease=<id> reason=<message>` for a lease whose placement failed (byte for byte as v0.07's), `place refused lease=<id>` for a `pending` lease the host does not meet beside a live `ready` lease of the project, now `unsatisfiable` with `no_capacity`, and `place left lease=<id>` for one the host does not meet with no live `ready` lease, still `pending`; neither of the last two with a `reason` field; a `pending` lease whose `size` is not in `SIZES` named by no line; a call that fails as a whole giving its one `place failed reason=<message>` as before | Conformance case, E0.09-T04-S02, by the log, with `place_pending`'s answers themselves by driver.md's rows |
| A plain `deploy` of the box not changing its type when `stack-parameters.json` names another | Conformance case alone, E0.09-T04-S02, by the arguments: the box is not resized live, and no plain `deploy` of it is run for v0.09 (D70) |
| **Bigger:** with agentspace's host stopped and no live lease, `create --size l` answering `pending`; the run's `deploy done type=m9g.2xlarge` in `provision.log`; the stack's `InstanceType` and the host's record's `instance_type` both `m9g.2xlarge`, read with `status -Host agentspace` and from the record; `nproc` printing 8; the real AWS CLI's `describe-stacks` answer giving `Parameters` in the form read | Live check, E0.09-T05-S01 (D70), with instance and volume IDs masked in its record |
| **Smaller:** once every lease is released, a default-size `create` answering `pending`, and the host coming back at `m9g.xlarge`, four vCPUs, recorded so | Live check, E0.09-T05-S01 (D70) |
| **Refused** and **unsupported:** no run started, and the host's record, its `instance_type` included, and its stack's type unchanged | Live check, E0.09-T05-S01 (D70), with `provision.log` showing no new line |
| **The first lease after the install:** agentspace's host's record gaining `instance_type` by its run; the box's record gaining `m9g.xlarge` by its first lease's run with no change to the box, shown by the record and `status -Host swimcoach` | Live check, E0.09-T05-S01 (D67, D70) |
| **The guard:** no stop of the host in `guard.log` or `status -All` during a resize, the run holding `provision.lock` | Live check, E0.09-T05-S01 (D68, D70) |
| The laptop's AWS sign-in needing no new permission for a resize | Live check, E0.09-T05-S01, by its run with today's sign-in |
| No lease placed `ready` on a host while a run that resizes it is due or under way ([Q36](#open-questions)): a new lease of a project with a `pending` lease a run would place answered `pending` | driver.md's rows for v0.09 (E0.09-T04-S01 and the driver crate's tests), since place is the driver's (D66's change note; pins P2(b) and P20) |
| No lease placed `ready` at the record's old type after the only `pending` lease was released while a run was at its deploy step: a `create` wholly before the step's hold of the lease state lock read by the run as a live `ready` lease, so no type is passed; one wholly after it answered `pending` by the record's missing type (pin P26; Q36, closed) | The ordering by driver.md's rows for v0.09 (E0.09-T04-S01 and the driver crate's tests); this page's part, the removal before the read under both locks, by the lock row above (E0.09-T04-S02) |

## Commands

### The top level

```
operator <command> [-Host <project> | -All] [-GiB <n>] [-Alias <alias>] [-InstanceType <type>] [-Quiet]
operator -h | --help
```

**Help** (architect, 2026-11-21, for v0.08-T01, as every help text v0.08 changes or adds is; v0.07's were for v0.07-T01; v0.09's change to it, and to `deploy`'s and `provision`'s, are architect, 2026-11-23, for v0.09-T01, pin P25):

```
Usage: agentspace-operator <command> [-Host <project> | -All] [-GiB <n>] [-Alias <alias>] [-InstanceType <type>] [-Quiet] [--help]

Manages agentspace's hosts from the laptop: each host's stack, its power, its
system files and toolchain, the SSH CA they trust, and the localswim Bot token
they use. Without -Host, a command acts on the box, swimcoach's host.

Commands:
  deploy     Create or update a host's stack, then run sync
  up         Start a host if needed, wait until it has patched, push the
             Bot token
  sync       Check the CA, install a host's system files, CLAUDE.md and the
             localswim CLI, and push the Bot token
  bootstrap  Install the agent toolchain on a host; safe to run again
  provision  Make a project's host: deploy, start, sync and bootstrap it
  ca         Create the agentspace SSH CA once; later, check it and print it
  down       Stop a host
  status     Show a host's state, type, disk and Session Manager reachability
  grow       Grow a host's disk by -GiB (default 2), and its filesystem with it
  guard      Stop every running host no live lease explains, after a
             15-minute grace, and send each running host its lease horizon

Parameters:
  -Host <project>  The project whose host to act on (default: the box);
                   provision needs it, and ca and guard take none
  -All             For status, down and sync: every host, one at a time
  -GiB <n>         For grow: the GiB to add (default 2)
  -Alias <alias>   For up: the SSH host alias the token push uses
                   (default: the host's own)
  -InstanceType <type>
                   For deploy: the instance type to give the host
  -Quiet           Print no progress or warnings; failures and status
                   still print
  -h, --help       Print this help, or a command's, and exit

The operator's own messages go to stderr; deploy, sync, bootstrap, provision
and grow pass other programs' output through. up writes nothing to stdout, so
it is safe in an SSH ProxyCommand.

Exit codes:
  0  Success
  1  Failure or usage error; the message on stderr says which
```

### deploy

```
operator deploy [-Host <project>] [-InstanceType <type>] [-Quiet]
```

**Creates or updates its host's CloudFormation stack, refreshes the host's instance ID and records
its instance type in its record, then runs `sync`.** Without `-Host` it deploys the box's stack,
`agentbox`, as before v0.07, with `SshPublicKey` and every member of `stack-parameters.json` but
`InstanceType`, which keeps its previous value unless `-InstanceType` gives one (D39, D67;
architect, 2026-11-23, for v0.09-T01, pin P6). With `-Host <project>` it deploys
the stack the project's record names, and `cloudformation deploy` creates that stack when there is
none, which is how a new host's stack is made (D44, D49): a record with no stack yet is one
`provision` added, since no other command adds a record ([From the command
line](#from-the-command-line)). **No host is sized by `stack-parameters.json`** (P13, architect,
2026-11-20, for v0.07-T01; D55; D67 for the box): a new host's stack takes only `SubnetId` and
`VpcId` from it, the box's stack takes every member but `InstanceType`, an existing stack keeps
its `InstanceType`'s previous value, and **every host, the box included, changes size only through
`-InstanceType`** (D67, D71). A provisioning run gives its deploy step an `-InstanceType` of its
own ([provision](#provision), step 4).

**Every `deploy` that is not a provisioning run's own deploy step takes its host's
`provision.lock` without waiting, as [provision](#provision)'s step 2 takes it, and holds it for
its whole life** (architect, 2026-11-23, for v0.09-T01, pin P34): for every host, the box
included, with `-InstanceType` or without. The lock is
`%USERPROFILE%\.agentspace\hosts\<project>\provision.lock`, `<project>` being its host's record's
`project`, so the box's is `%USERPROFILE%\.agentspace\hosts\swimcoach\provision.lock`, the lock
`provision -Host swimcoach` takes ([state.md, A host's folder](state.md#a-hosts-folder)). It is
taken as [state.md, The provisioning run's lock](state.md#the-provisioning-runs-lock) says: one
try, creating the host's folder and the file when they are missing. `AGENTSPACE_STATE_DIR` does
not move it.
- **Where it is taken:** after the command line's checks (help, the usage errors, the `-Alias`
  checks and the host checks, [Command line](#command-line)), the home and system folder checks,
  and the host's lookup ([From the command line](#from-the-command-line): a host registry that is
  not valid, and a project with no record, are refused first), none of which makes an AWS call or
  writes a file; **and before anything below**: the existence check's `describe-stacks` for a host
  that is not the box, steps 1 to 3's reads, step 4's write, the operator's folder, and every other
  AWS call and file write. So while a run holds the lock, a `deploy` that would fail at step 1 on a
  missing public key is refused for the lock first (pin P34).
- **A lock that another program holds refuses the `deploy` at once**, exit 1, with
  `agentspace: A provisioning run or deploy for '<project>' is already running; the deploy did not run.`,
  exactly, `<project>` being the host's record's `project` (architect, 2026-11-23, for v0.09-T01,
  pins P34 and P36): the message [provision](#provision) prints for a held lock in its step 2,
  adapted to `deploy`. **It makes no AWS call, takes no other lock, does not go on to `sync`, and
  changes no file**: the host's folder and the lock file already exist, since a held lock is an
  open file, and it writes no line to `provision.log`, which is the run's alone ([state.md, The
  provisioning run's log](state.md#the-provisioning-runs-log)). The lock does not say which program
  holds it, so the message names both that hold it for long: a provisioning run, or another
  person's `deploy`. It can also be the moment's try of the driver or the guard, after which the
  `deploy` MAY be run again. **`provision`'s own held-lock message, and the guard's
  `provisioning=running`, are unchanged** and name a run alone, though a person's `deploy` can
  hold the lock they report (pin P36).
- **A lock that cannot be taken for any other reason**, such as access denied or a folder at its
  path, is not a free lock ([state.md, The provisioning run's
  lock](state.md#the-provisioning-runs-lock)): `deploy` fails with
  `agentspace: Cannot take <path>: <error>.`, `<path>` being the lock's full path and `<error>` as
  [provision](#provision)'s step 2 writes it, exit 1, before any AWS call, writing no file but the
  host's folder when the try made it (pin P34).
- **It is held for `deploy`'s whole life, its `sync` step included**, since `sync` is part of
  `deploy` (step 6), and released as `deploy` exits; the operating system releases it however
  `deploy` ends, a kill included. **`sync` takes no `provision.lock` of its own**: the only lock its
  steps wait for is `hosts.lock`, for a missing instance ID's write and an operator key's making
  ([How the operator reaches a host](#how-the-operator-reaches-a-host)), which comes after
  `provision.lock` in [state.md, The order of the locks](state.md#the-order-of-the-locks), as
  `deploy`'s own writes of steps 4 and 5 do, so the order is kept (pin P34).
- **While it is held, no provisioning run of the host runs**: the EC2 driver's try finds it held
  and starts none ([driver.md, Lock order](driver.md#lock-order)), and a run started meanwhile,
  by hand or by a driver whose try came first, fails its step 2 as for a run already running. So
  a person's read-back never lands during a run's call to AWS, writing a type back after the
  run's step 4.2 removed it, and a `create` is never placed `ready` on the old size just before a
  run's resize (pin P34's reason). **The guard leaves the
  host while the lock is held** ([guard](#guard), step 6; D56), so a person's resize is never
  stopped mid-change.
- **A provisioning run's deploy step takes nothing more**: it runs under the `provision.lock` its
  run took at its step 2, and never tries it again ([provision](#provision), step 4.4; pin P34).

**For a host that is not the box, `deploy` first learns whether its stack exists, and in what
state**, with `cloudformation describe-stacks --stack-name <stack>` ([Calls by
command](#calls-by-command)), reading `Stacks[0].StackStatus` (P17 and P19, architect, 2026-11-20,
for v0.07-T01):
- **No stack**, a failure whose error text holds `does not exist`, without regard to case, **or
  `REVIEW_IN_PROGRESS`**, a stack that holds a change set and no resources: `deploy` **creates**
  it, with the creation parameters below.
- **`CREATE_COMPLETE`, `UPDATE_COMPLETE`, `UPDATE_ROLLBACK_COMPLETE`, `IMPORT_COMPLETE` or
  `IMPORT_ROLLBACK_COMPLETE`**, the states an update can start from: `deploy` **updates** it.
- **Any status that ends `_IN_PROGRESS`**, but `REVIEW_IN_PROGRESS`, a stack in the middle of an
  operation: `deploy` fails with
  `agentspace: Stack '<stack>' is <status>; try again once that operation has finished.`
- **`ROLLBACK_COMPLETE`**, which a stack whose first creation failed is left in, which no update
  can start from, and which holds no instance: `deploy` fails with
  `agentspace: Stack '<stack>' is ROLLBACK_COMPLETE, which no update can start from. Look at its events, delete it with 'aws cloudformation delete-stack --region us-west-2 --stack-name <stack>', then run 'agentspace-operator provision -Host <project>' again.`
- **Any other status**, such as `UPDATE_ROLLBACK_FAILED` or `DELETE_FAILED`, which no update can
  start from and which can still hold the host's instance and its disk: `deploy` fails with
  `agentspace: Stack '<stack>' is <status>, which no update can start from. Look at its events; it can still hold the host's instance and disk, so do not delete it unless nothing on the host is to be kept.`
  It names no delete command, since deleting such a stack destroys the host's disk (the dev lead's
  reading of P17, which names the delete for a stack whose creation failed; architect, 2026-11-20, for v0.07-T01).
- **No operator command deletes a stack** ([The stack](#the-stack)): a delete is always a person's,
  after looking at the stack's events.

Each of those three failures comes before any call that changes anything, and its message is the
step's failure message: in `provision` it is printed and written to the run's log, and `provision`
records `failed` for a host that was never `ready` and leaves a `ready` host `ready`, as every
failure of its deploy step does ([provision](#provision)). `deploy` run by itself writes no
`state`, as no command but `provision` does ([The host registry, as the operator uses
it](#the-host-registry-as-the-operator-uses-it)). Any other failure of the call fails `deploy`
under [AWS failures](#aws-failures), #0034's sign-in message first, and an answer with no string
`Stacks[0].StackStatus` fails with
`agentspace: Unexpected answer from cloudformation describe-stacks: no Stacks[0].StackStatus.`
The box's `deploy` makes no such call, as before v0.07. The list of states, the two messages, and
`deploy` by itself writing no `state`, are confirmed by the architect, 2026-11-20, for v0.07-T01. Below, "the box's stack", "a
stack it creates" and "a host's stack it updates" are the box's and those two paths; `-Host
swimcoach` is the box's.

1. **Reads the user's public key** from `%USERPROFILE%\.ssh\id_ed25519.pub`, trimmed, for the
   box's stack and for a stack it creates, and not for a host's stack it updates: it is the
   template's `SshPublicKey`, installed for the login user at first boot, for a new host as for the
   box (P13, architect, 2026-11-20, for v0.07-T01). A missing,
   unreadable or empty file MUST fail with an `agentspace: ` message naming it:
   `agentspace: <path> does not exist.`, `agentspace: Cannot read <path>.` or
   `agentspace: <path> is empty.` (architect, 2026-11-19, for E0.06-T03-S02). A file of whitespace
   alone is empty, since the key is read trimmed. The prototype fails
   with a runtime error for a missing or empty file, and passes an empty key on for a
   whitespace-only one.
2. **Reads the stack parameters file, `%USERPROFILE%\.agentspace\stack-parameters.json`**, for the
   box's stack and for a stack it creates, and not for a host's stack it updates (D37,
   D40; [What the prototype took from its checkout](#programs-and-install-layout); the prototype
   reads `KEY=VALUE` lines from `<checkout>\infra\local.parameters`). A missing file fails with
   `agentspace: Missing <path>; copy infra\stack-parameters.example.json and fill it in.`, where
   `<path>` is the file's full path. A file that cannot be opened, cannot be parsed as JSON, or does
   not match its schema fails with `agentspace: <path> is not a valid stack parameters file:
   <reason>.`, where `<reason>` is one sentence fragment that names the first problem found and
   holds no line break (architect, 2026-11-19, for v0.06). For the box's stack, each member of
   `parameters` but `InstanceType` becomes a parameter override (format: docs/state.md). **A file that names
   `SshPublicKey`, or names any member twice, is not valid** (architect, 2026-11-19, for
   E0.06-T03-S02): `deploy` sets `SshPublicKey` itself from step 1, and a file that could override
   it would put another key on the box with no word said. The `<reason>` names the member. **So is
   a file that names `HostName` or `CaPublicKey`** (architect, 2026-11-20, for v0.07-T01). That is this page's
   own check, made by `deploy` after the file matches its schema, which does not forbid them
   ([state.md, Files in the checkout](state.md#files-in-the-checkout)): `deploy` sets both itself
   for a host that is not the box, and on the box either would change the `agentbox` stack's names
   or its first boot, whose change set MUST stay empty (D44). Any stack parameter can be set there,
   not only `VpcId` and `SubnetId`, so `ImageId` MUST NOT be set there on a live stack
   ([The stack](#the-stack)).
   - **The file sizes no host** (D67, D71, replacing v0.07's rule that it sized the box alone, P13,
     which closed [Q31](#open-questions); architect, 2026-11-23, for v0.09-T01, pin P6). **Its
     `InstanceType` is not passed to any stack and not read for any use**: for the box's stack every
     other member is passed, as before v0.07, and `InstanceType` is passed only from
     `-InstanceType` (step 4), keeping its previous value without it. So the box's next plain `deploy` cannot put back
     a size a lease set (D67). **A file that names `InstanceType` is still valid**, its value
     whatever the file's schema allows ([state.md, Files in the checkout](state.md#files-in-the-checkout)),
     and the operator never removes the member, rewrites the file or warns about it: the file is a
     person's, and README says the member is ignored. **For a stack it creates, only `SubnetId` and
     `VpcId` are taken from it**, and every other member is ignored, as before. A file that does
     not name both fails a stack it creates with
     `agentspace: <path> MUST name SubnetId and VpcId for a new host's stack.` before any AWS call
     that changes anything (the message is confirmed by the architect, 2026-11-20, for v0.07-T01).
3. **For a stack it creates, checks the CA and reads its public key**: the CA check is
   [sync](#sync)'s step 1, with its messages, so a host is never deployed to trust a half-made or
   loosened CA; then it reads `%USERPROFILE%\.agentspace\ca\agentspace_ca.pub` and keeps its first
   two fields, the key type and the base64 key, joined by one space, with no comment and no line
   ending (P8, architect, 2026-11-20, for v0.07-T01; D44). That is `<CA key>` below. **The comment
   is dropped because it holds the computer's name** ([state.md, The CA](state.md#the-ca)), and a
   stack parameter is kept and shown by CloudFormation. The box's stack and a host's stack it
   updates pass no `CaPublicKey`, so this step is skipped for them; `deploy`'s `sync` step checks
   the CA for every host.
4. **With `-InstanceType <type>`, first removes `instance_type` from the host's record**, under
   `hosts.lock`, changing nothing else in it, before `cloudformation deploy` runs and before any AWS
   call that changes anything, whatever the record held, `<type>` itself included (D67's change
   note; architect, 2026-11-23, for v0.09-T01, pin P11; [state.md, The instance
   type](state.md#the-instance-type)). A record that already holds none is left as it is. **A
   missing `hosts.json` is not a record that holds none**: it reads as the box's record with
   `m9g.xlarge` ([The host registry, as the operator uses
   it](#the-host-registry-as-the-operator-uses-it)), so step 4 then writes the registry, the seed
   document's record with no `instance_type`, as any first change writes it (architect,
   2026-11-23, for v0.09-T01, pins P2c, P11 and P31). From then until step 5 writes the type the
   stack reports, the record claims no size for a host whose size can change, so the EC2
   driver's place answers no lease `ready` on the old size while the stack changes it (D66, D67:
   a record without `instance_type` meets no size). **Without
   `-InstanceType` nothing is written here**, and the record keeps its type until step 5's
   read-back, since the stack's type then cannot change: **a person's `deploy` that passes no
   `InstanceType` and fails before step 5, `cloudformation deploy` included, keeps the type; only a
   read-back that fails or is malformed removes it** (architect, 2026-11-23, for v0.09-T01, pin
   P30). A provisioning run's deploy step is not a person's `deploy`: it has already removed the
   type before it calls this step, whether it passes one or none, so a failed run leaves none, by
   design ([provision](#provision), step 4.2; pins P26 and P30). A lock not taken within its 30 seconds,
   or a write that fails, fails `deploy` with the host registry's messages
   ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)),
   before `cloudformation deploy` runs. **The removal is not undone when `cloudformation deploy`
   then fails, or when `deploy` is killed** (architect, 2026-11-23, for v0.09-T01, pin P11): a
   `cloudformation deploy` whose client is killed
   goes on running on AWS's side, and one that fails is rolled back to the type the stack had
   (D68), so the record holds no type until the next `deploy` records one. A lease of the project
   then answers `pending`, never `ready` on a size nobody has read back, and its run records the
   type ([provision](#provision)). That costs one run, never a stale type.

   **Then prints `Deploying stack '<stack>' in us-west-2...`, for every stack**, which for the box is
   `Deploying stack 'agentbox' in us-west-2...`, the line it printed before v0.07. **Then runs
   `cloudformation deploy`** ([The AWS side](#calls-by-command)), with its output passed through,
   and with these parameters (P13, architect, 2026-11-20, for v0.07-T01):
   - **The box's stack:** `SshPublicKey` and every member of the file but `InstanceType`, and no
     `HostName` and no `CaPublicKey`, so its change set is empty when nothing else changed (D44;
     P5). **With `-InstanceType <type>`, `InstanceType=<type>` is passed too, last** (D67, D71;
     architect, 2026-11-23, for v0.09-T01, pin P32);
     without it, `InstanceType` is not passed, so `cloudformation deploy` keeps its previous value,
     as `UsePreviousValue=true` gives it (below). A box's stack that does not exist yet, which
     `cloudformation deploy` would create, then takes the template's default, `m9g.xlarge`.
   - **A stack it creates:** `SshPublicKey`, `SubnetId` and `VpcId`, `HostName=<stack>` and
     `CaPublicKey=<CA key>` (D44; P5 and P8, architect, 2026-11-20, for v0.07-T01), so the stack's
     alarm and tags take the host's name and its first boot trusts the CA before the operator first
     connects. Every other parameter takes the template's default: `InstanceType` `m9g.xlarge` and
     `RootVolumeGiB` 8 (D46), never the box's values. With `-InstanceType <type>`,
     `InstanceType=<type>` is passed too (architect, 2026-11-20, for v0.07-T01).
   - **A host's stack it updates:** every parameter keeps its previous value, as
     `UsePreviousValue=true` gives it, which `cloudformation deploy` applies to every parameter it
     is not passed: so no parameter is passed at all, and `deploy` reads neither the public key,
     the file nor the CA. **With `-InstanceType <type>`, `InstanceType=<type>` alone is passed**,
     which is the one way such a host changes size (D39, D51, D67). CloudFormation stops and starts
     the instance to change its type, and keeps its disk ([The stack](#the-stack)). A provisioning
     run on a host whose stack exists, a `ready` host's included, takes this path, with the
     `-InstanceType` its project's `pending` leases give it or none ([provision](#provision), step
     4), so without one its deploy step changes nothing but what a newer template changes (P3).
     **So a run that a `get` starts on a `ready` host after an operator upgrade applies a changed
     template to that host unattended**, with whatever that change makes CloudFormation do to its
     instance, a stop and start included; and a run given a type changes the host's type
     unattended, which is what a lease asked for (D66, D68).

   A non-zero exit fails with `agentspace: Stack deploy failed; see the stack events in
   the console.` An expired sign-in MUST fail with #0034's message instead
   ([AWS failures](#aws-failures)).
5. **Removes the instance ID from the host's record, reads the instance ID from the stack's
   `InstanceId` output, and writes it into the record again** ([The host registry, as the operator
   uses it](#the-host-registry-as-the-operator-uses-it)), each write under `hosts.lock`, and prints
   `Instance: <id>`. The ID is removed first so that it comes from the stack as the deploy left
   it, never from an older record. If the read fails, `deploy` fails with no instance ID in the
   record, and the next command that needs the ID reads it from the stack.
   **It records the host's instance type from the same answer** (D67, D71; architect, 2026-11-23,
   for v0.09-T01, pin P11): **the removal of the instance ID removes `instance_type` too, in the
   same write**, so that a read-back that fails or is malformed leaves no type, which is the only
   way a person's `deploy` that passed no `InstanceType` loses it (architect, 2026-11-23, for
   v0.09-T01, pin P30); it changes nothing for a `deploy` whose step 4, or whose provisioning run's
   step 4.2, already removed it. `deploy` holds no lock but `provision.lock` during the call to
   AWS (pin P34). The
   `cloudformation describe-stacks` call that reads `InstanceId` also gives the stack's
   parameters, and `deploy` reads the `ParameterValue` of the element of `Stacks[0].Parameters`
   whose `ParameterKey` is `InstanceType` ([Calls by command](#calls-by-command)), and writes it as
   the record's `instance_type` in the same write as the instance ID, under `hosts.lock`. **So
   every `deploy` that succeeds leaves the record holding the type the stack holds**, whether or
   not `-InstanceType` was given, the box's included, and a provisioning run's deploy step does the
   same. One call reads both, and the record takes one write, so `deploy` makes no call, and needs
   no permission, that it did not before v0.09 (architect, 2026-11-23, for v0.09-T01, pin P23).
   - **`InstanceId` is checked first**, as before: a stack with no `InstanceId` output fails with
     its message and writes nothing, so the record holds neither an instance ID nor a type.
   - **An answer with no such element, with two such elements, or whose `ParameterValue` is
     missing, not a string, or not of the form the host registry's schema gives `instance_type`**
     ([state.md, The host registry](state.md#the-host-registry)), fails `deploy`, exit 1, with
     `agentspace: Stack '<stack>' has no InstanceType parameter.` (architect, 2026-11-23, for
     v0.09-T01, pin P22, in the form of the `InstanceId` message). The instance ID it read is
     still written, alone; the record is left with no `instance_type`; and `deploy` does not go on
     to `sync`. **It prints no `Instance: <id>` line**, which a deploy prints only on success
     (architect, 2026-11-23, for v0.09-T03, pin P45; D36).
   - **A failed `describe-stacks`, or a lock not taken or a write that fails when the ID and type
     are written after the removal, after `cloudformation deploy` succeeded, fails `deploy`,
     exit 1** (D36: the operator never reports a false success), with the AWS CLI's text under
     [AWS failures](#aws-failures), #0034's sign-in message first, or the host registry's message.
     The stack keeps what the deploy did; the record has no instance ID, as above, and no
     `instance_type` (pins P11 and P30: a failed read leaves no type, never a stale one). The next
     `deploy`, or the next provisioning run, records both. `deploy` does not go on to `sync`.
   - **The removal's own lock not taken within its 30 seconds, or the removal's write failing,
     fails `deploy`, exit 1, with the host registry's message, before `describe-stacks`**, and
     removed nothing, since a write that fails leaves the file as it was: **the record keeps its
     `instance_id`, and keeps its `instance_type` unless step 4, or the provisioning run's step
     4.2, had already removed it** (architect, 2026-11-23, for v0.09-T01, pins P11 and P30). That
     kept type is the one the record held before this `deploy`, which, without `-InstanceType`,
     asked for no change of type. The next `deploy`, or the next provisioning run, records both.
     `deploy` does not go on to `sync`.

   Nothing else in the record changes: `deploy` never sets a `state`.
6. **Runs `sync`**, as if `operator sync` had been run with `deploy`'s own `-Host` and `-Quiet`:
   **`deploy -Quiet` quiets its `sync` step too** (D36; [Q4](#open-questions), closed), as the
   `-Quiet` row of [Command line](#command-line) says of every command. **`deploy` MUST exit 1
   when `sync` fails.** The prototype runs `sync` as a nested script without `-Quiet`, so
   `sync`'s messages print, and its failure most likely does not reach `deploy`'s exit code
   ([Q3](#open-questions)); both are
   [requirements it does not meet yet](#requirements-the-prototype-does-not-meet-yet).

**Stdout:** the AWS CLI's deploy output, then `sync`'s. **Stderr:** the messages above, then
`sync`'s. **Exit codes:** 0 and 1.

**`deploy` passes no `ImageId`, so the template's pinned default applies** ([The
stack](#the-stack)). **`-InstanceType` is `deploy`'s alone**, for every host, the box included
([Command line](#command-line), host check 5; v0.07's sixth host check, which refused it for the
box, was deleted by v0.09: D67, D71; architect, 2026-11-23, for v0.09-T01, pin P5).

**Help:**

```
Usage: agentspace-operator deploy [-Host <project>] [-InstanceType <type>] [-Quiet]

Creates or updates a host's CloudFormation stack in us-west-2 from its
template, records the host's instance type in ~/.agentspace/hosts.json, then
runs sync. Without -Host it deploys the box's stack, 'agentbox', with the
parameters in ~/.agentspace/stack-parameters.json, whose InstanceType is
ignored, and your ~/.ssh/id_ed25519.pub. With -Host <project> it deploys that
project's host's stack: when there is none, it creates it with the SubnetId
and VpcId in that file, your public key and the SSH CA's public key; when
there is one, it keeps every parameter but -InstanceType's as it was.

  -Host <project>       The project whose host to deploy (default: the box).
  -InstanceType <type>  The instance type to give the host, the box included,
                        such as m9g.2xlarge. Without it, a host keeps its
                        type, and a new host gets m9g.xlarge.

Never change the template's ImageId on a live stack: it replaces the instance
and its disk.

Exit codes: 0 success, 1 failure.
```

### up

```
operator up [-Host <project>] [-Alias <alias>] [-Quiet]
```

**Makes its host ready for a connection: starts it if it is stopped, waits until its boot-time
patch run has finished, and pushes the localswim Bot token and the laptop's time zone if they are
due.** Without `-Host` its host is the box. The ProxyCommand runs it before every connection, on
the host it found ([From the ProxyCommand](#from-the-proxycommand)).

1. **Reads the host's instance ID** from its record, or from its stack's output when the record has
   none, writing it into the record then
   ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)).
2. **Starts the host and waits for it**, as [Starting the box and waiting for
   it](#starting-the-box-and-waiting-for-it) says, ending with `Ready: <id>`.
3. **Pushes the token over `<alias>`**, as [The token push](#the-token-push) says, with the host's
   push stamp, unless it is a nested `up` ([Every ssh runs the
   ProxyCommand](#every-ssh-runs-the-proxycommand)). `<alias>` is `-Alias`, whose default is the
   host's own alias: `agentspace` for the box, `agentspace-host.<project>` for another host.
   **`-Alias` and `-Host` MAY be given together**: `-Host` picks the host, its instance ID and its
   push stamp, and `-Alias` only the alias the push connects with, as the ProxyCommand runs it
   (architect, 2026-11-20, for v0.07-T01). **A person's `up` of a host that is not the box, not nested, first
   makes the operator's key, certificate and alias for it** ([How the operator reaches a
   host](#how-the-operator-reaches-a-host)), whether or not the push is due, as every command given
   `-Host` does (P13 and P15, architect, 2026-11-20, for v0.07-T01). **A failure there is a push
   failure, which names its cause and fails nothing**: `up` skips the push, warns
   `agentspace: Could not send the localswim token, because <cause>; the board will not work on the host this session.`,
   where `<cause>` is the failing step's message without its `agentspace: ` prefix and its last
   period, such as the CA check's, and exits 0 (architect, 2026-11-20, for v0.07-T01). Run by the ProxyCommand,
   `up` never does this.

**Stdout:** nothing, ever. **Stderr:** the messages of steps 2 and 3, and the stderr of the push's
ssh, which carries its nested ProxyCommand's lines, `The host is ready.` among them
([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)). Run by the ProxyCommand,
`up` prints the caller's lines in place of a person's
([What a caller sees](#what-a-caller-sees-d1-card-0087)), and **passes its push's ssh's stderr on
with every line that is exactly `The host is ready.` dropped, unless `AGENTSPACE_DIAGNOSTIC` is
set** ([What the laptop sends](#what-the-laptop-sends), step 2; #0167, P14, architect, 2026-11-21,
for v0.08-T01). **A start does not make the push due by itself** ([The stamp](#the-stamp)).
**Exit codes:** 0 once the box is ready,
whatever the push did ([What the laptop checks](#what-the-laptop-checks)); 1 on any failure before
that.

**Help:**

```
Usage: agentspace-operator up [-Host <project>] [-Alias <alias>] [-Quiet]

Starts a host if it is stopped, waits until its boot-time patch run has
finished, and sends the localswim Bot token and the laptop's time zone if
either has changed since the last confirmed send. Without -Host it acts on
the box.

  -Host <project>  The project whose host to start (default: the box).
  -Alias <alias>   The SSH host alias the token push connects with
                   (default: agentspace for the box, agentspace-host.<project>
                   for another host). Letters, digits, '.', '_' and '-',
                   not starting with '-'.

Writes nothing to stdout, so it is safe inside an SSH ProxyCommand.
A failed token push warns and still exits 0.

Exit codes: 0 the host is ready, 1 failure.
```

### sync

```
operator sync [-Host <project> | -All] [-Quiet]
```

**Installs the host's system files and the CA's public key, copies the user's `CLAUDE.md`, builds
and installs the localswim CLI, and pushes the token.** Without `-Host` its host is the box, and
every connection goes through the `agentspace` alias, as before v0.07. With `-Host`, every
connection goes through the host's own alias, `agentspace-host.<project>` for a host that is not
the box, which the operator writes, with its key and certificate, between steps 1 and 2 ([How the
operator reaches a host](#how-the-operator-reaches-a-host)). Forwards are cleared on every
connection. **Below, `agentspace` in a call stands for the host's alias.** `sync -All` runs these
steps for every host whose record is `ready` ([Every host at once](#every-host-at-once--all)).
**After the CA is replaced, `sync` reaches the box alone**: a host that is not the box trusts the
old CA until a person recovers it, as [ca](#ca) says (D17, D50).

1. **Checks the CA**, exactly as `ca` checks an existing one ([ca](#ca), the checks), and fails
   with that check's message before the box is touched. **That is `ca`'s steps 1, 2, 3 and 5, in
   that order, each with `ca`'s own message, and never step 4** (architect, 2026-11-19, for
   v0.06, gaps FA-14 and FB-12): so a CA in a synced folder or a git working tree, or a half-made
   one, fails `sync` as it fails `ca`, and a laptop with neither file fails at step 5's first
   check, `agentspace: <key> does not exist; run ca.`, since `sync` never creates a CA. A missing `ssh-keygen.exe` fails first,
   with `ca`'s step 1 message; the prototype does not check for it, and fails with another
   error. A missing, half-made, mismatched or loosened CA never becomes a host's trusted CA
   (D17).
2. **Starts the box and waits for it** ([Starting the box](#starting-the-box-and-waiting-for-it)).
3. **Prints `Copying box/system to the instance...`**, removes the box's staging folder
   (`/tmp/agentspace-system` on today's box), and copies `<checkout>\box\system` there, whole and
   recursively, so that the staging folder holds exactly `box\system`'s files. **A failed removal
   MUST fail with `agentspace: Removing the old staging folder on the instance failed.`**, before
   the copy (D36; [Q25](#open-questions), closed): a staging folder left behind would take the copy one
   level down, and the install would then run the old files. A failed copy fails with
   `agentspace: Copy to the instance failed.`
4. **Prints `Copying the agentspace SSH CA's public key (<fp>) to the instance...`** and copies
   the CA's public key into the staging folder as `agentspace_ca.pub`. A failure fails with
   `agentspace: Copying the CA's public key to the instance failed.`
5. **Runs the staged install as root on the box, and removes the staging folder only if the
   install succeeds**, with its output passed through. A non-zero exit fails with `agentspace:
   Install on the instance failed.`, and leaves the staging folder on the box, which the next
   `sync`'s step 3 removes. What the install does is in [What sync installs](#what-sync-installs).
   **Then, before step 6, it waits until the host's patch run has finished this boot**, as
   [The patch wait](#the-patch-wait) below says (D61's change, #0163), and fails at that wait's
   deadline. So step 6 and every step after it, each ssh and scp with its nested ProxyCommand,
   finds the host patched. **A patch run that has failed ends the wait at once with a warning,
   and `sync` goes on with its exit status unchanged** (P21, architect, 2026-11-21, for
   v0.08-T01).
6. **If `%USERPROFILE%\.claude\CLAUDE.md` exists**, prints `Copying ~/.claude/CLAUDE.md to the
   instance...`, makes sure the login user's `~/.claude` exists on the box, and copies the file to
   `~/.claude/CLAUDE.md` there, replacing the box's copy. **The folder step's exit code is not
   checked** (B-34): the copy that follows is the check, and fails when the folder is not there.
   What the folder step writes to stderr passes through. A failed copy fails with `agentspace:
   Copying CLAUDE.md to the instance failed.` A folder step that fails on a box that already has
   the folder is therefore not reported, and the copy succeeds. The laptop's copy is the source: the box's copy MUST NOT be edited,
   because the next `sync` replaces it.
7. **Builds and installs the localswim CLI**, when the laptop can:
   - **When `go` is not on `PATH`, or `C:\Projects\localswim` is missing**, it prints `Skipping
     the localswim CLI: Go or C:\Projects\localswim is missing on the laptop.` and goes on to
     step 8. This is not a failure.
   - **Otherwise it prints `Building the localswim CLI for linux/arm64...`** and builds
     `./cmd/localswim` from `C:\Projects\localswim` for `linux/arm64` with cgo off, trimmed paths,
     into `<checkout>\.build\localswim-linux-arm64` (the rewrite's place is
     `%USERPROFILE%\.agentspace\operator\<hex>\localswim-linux-arm64`, in the command's own folder,
     [The operator's folder, one per command](#the-operators-folder-one-per-command)). **The build
     is exactly `go build -trimpath -o <out> ./cmd/localswim`**, run in `C:\Projects\localswim` with
     `GOOS=linux`, `GOARCH=arm64` and `CGO_ENABLED=0` ([Environment variables](#environment-variables)),
     where `<out>` is the output file's full path, and with no other flag (architect, 2026-11-19,
     for v0.06, gap FB-14). A failed build fails with `agentspace:
     Building the localswim CLI failed.` `linux/arm64` is today's box's platform.
   - **Prints `Copying the localswim CLI to the instance...`**, copies the binary to the box's
     staging path (`/tmp/localswim` today), and fails with `agentspace: Copying the localswim CLI
     failed.` if that fails.
   - **Installs it as root, mode 0755, at `/usr/local/lib/localswim/localswim`**, and removes the
     staged copy. A failure fails with `agentspace: Installing the localswim CLI failed.`
8. **Pushes the token over the host's alias, forced**: the stamp is not compared, so the push always
   runs, and on success writes the host's own stamp ([The token push](#the-token-push); D50).

**Stdout:** the install's `Installed:` lines and its `Patch run:` line, and whatever else the
remote steps print on their stdout. **Stderr:** the messages above; the patch wait's messages and
its warning for a failed patch run ([The patch wait](#the-patch-wait)); `go build`'s output; the remote steps' stderr, the install's
`install.sh:` lines among them; one `The host is ready.` per ssh or scp, from its nested
ProxyCommand; and the push's, its nested ProxyCommand's `The host is ready.` included. **Exit
codes:** 0 and 1, whatever the push did ([What the laptop checks](#what-the-laptop-checks)).

**Its ssh and scp calls, in order** (B-23, A-30). Each is one process, started as
[Programs it starts](#programs-it-starts) says (or through [the test seam](#the-test-seam)), with
exactly these arguments in this order, its stdout and stderr the operator's, and each through the
`agentspace` alias, so each runs its own nested ProxyCommand. None of rows 1 to 8 sets `BatchMode`; row 9, the push,
does ([What the laptop sends](#what-the-laptop-sends)). The local paths
are full Windows paths, with `%USERPROFILE%` and `<checkout>` written as their values; `<staging>` is the box's staging folder, `/tmp/agentspace-system` on
today's box. In the rewrite, rows 2 and 7 name the command's own folder in place of the
`<checkout>` paths: `%USERPROFILE%\.agentspace\operator\<hex>\system` and
`%USERPROFILE%\.agentspace\operator\<hex>\localswim-linux-arm64`
([The operator's folder, one per command](#the-operators-folder-one-per-command)).

| # | When | Program and arguments | The remote step MUST, on the box |
|---|---|---|---|
| 1 | Always | `ssh -o ClearAllForwardings=yes agentspace <remote command>` | Remove `<staging>` and everything in it. A folder that is not there is not an error. Exit 0 only when it is gone |
| 2 | Always | `scp -o ClearAllForwardings=yes -q -r <checkout>\box\system agentspace:<staging>` | Nothing: a copy |
| 3 | Always | `scp -o ClearAllForwardings=yes -q %USERPROFILE%\.agentspace\ca\agentspace_ca.pub agentspace:<staging>/agentspace_ca.pub` | Nothing: a copy |
| 4 | Always | `ssh -o ClearAllForwardings=yes agentspace <remote command>` | As root, never prompting: run the staged install, which MUST NOT need an execute bit, since a copy from Windows carries none (on today's box, the staged `install.sh` run by bash); then, only if it exited 0, remove `<staging>`. Exit with the install's status, or the removal's. The install finds its files beside itself, whatever the remote working folder |
| 5 | `CLAUDE.md` exists | `ssh -o ClearAllForwardings=yes agentspace <remote command>` | As the login user, make sure the login user's `~/.claude` folder exists, creating its parents. A folder already there is not an error, and its mode is left as it is. A new folder gets the login user's default mode (its umask); no mode is required |
| 6 | `CLAUDE.md` exists | `scp -o ClearAllForwardings=yes -q %USERPROFILE%\.claude\CLAUDE.md agentspace:.claude/CLAUDE.md` | Nothing: a copy, relative to the login user's home |
| 7 | The CLI was built | `scp -o ClearAllForwardings=yes -q <checkout>\.build\localswim-linux-arm64 agentspace:/tmp/localswim` | Nothing: a copy to today's box's staging path |
| 8 | The CLI was built | `ssh -o ClearAllForwardings=yes agentspace <remote command>` | As root, never prompting: install the staged binary at `/usr/local/lib/localswim/localswim`, owned by root, mode 0755, creating its folders, each owned by root and readable and searchable by every user (0755); then, only if that succeeded, remove the staged copy. Exit non-zero if either failed |
| 9 | Always | The push's ssh ([What the laptop sends](#what-the-laptop-sends)) | [What the box does](#what-the-box-does) |

- **`<remote command>` is one argument, the last**, quoted as [What the laptop
  sends](#what-the-laptop-sends) says for Windows. Its text is the operator's own, and this page
  gives only what it MUST do (D34). Every path in this table on the box's side is today's box's.
- **Each of rows 1 to 8 but 5 is checked by its process's exit code**, as steps 3 to 7 above
  say; row 5's failure is reported by row 6's copy, as step 6 above says, and row 9 is
  checked as [What the laptop checks](#what-the-laptop-checks) says, never failing `sync`. An ssh that fails to connect exits
  255, which fails its row the same way.
- **So a `sync` starts at least five ssh and scp processes** (1, 2, 3, 4 and 9) and at most nine,
  and makes as many Session Manager sessions.
- **The patch wait runs between rows 4 and 5, and only when row 4 exited 0.** It starts no ssh or
  scp and opens no Session Manager session: its calls are the AWS CLI's, through Run Command
  ([The patch wait](#the-patch-wait); [Calls by command](#calls-by-command)).

**After the localswim service on the laptop is updated, `sync` MUST be run again for every host**
(`sync -All`), so that each host's localswim CLI is built from the same source as the service it
talks to. **After the CA is replaced, `sync` MUST be run for the box, and every other host MUST be
recovered by hand**, as [ca](#ca)'s replacement steps say (D17, D50).

#### The patch wait

**`sync` waits, right after its install (row 4), until the host's patch check answers `patched`,
before row 5, and stops waiting at once with a warning when it answers `failed`** (D61's change,
#0163; P21). The install starts this boot's patch run when the host has not finished one ([What
sync installs](#what-sync-installs)), so a new host's first `sync` patches it, and every nested
connection after row 4 finds it patched, or finds its patch run failed and reported. **It is the
same question `up` asks, through Session Manager's Run Command and never over SSH** ([What up waits
for](#what-up-waits-for)), so it goes on through a patch run's reboot, which ends every SSH
connection but not the wait. `provision` gets it through its step 6, which is `sync`, and `deploy`
through its step 6, which is `sync` too. `bootstrap` has none. With `-All`, each host waits in its
own part. Its place under `sync` and its comment are confirmed (architect, 2026-11-21, for
v0.08-T01, S01 batch); its messages and the outcomes that end it are confirmed as P21 changed them
(P21 and the S01 batch, architect, 2026-11-21, for v0.08-T01); its bound is the architect's (P13,
below).

**What it MUST do, in order:**
1. **Prints `Checking that <id> has finished this boot's patch run...`**
2. **Checks, at once and then every 5 seconds, for at most 30 minutes from the start of the wait**
   (P13, architect, 2026-11-21, for v0.08-T01):
   - **Is Session Manager's agent `Online`?** If not, the message is `Waiting for Session
     Manager...`.
   - **If it is, asks the host whether its patch run has finished**: the question and its four
     answers are [What up waits for](#what-up-waits-for)'s, sent with the comment
     `agentspace sync: patch check`, so Run Command's history tells `sync`'s questions from `up`'s,
     and its invocation polled as `up`'s is, once a second, at most 20 times.
   - **`patched` ends the wait, and a `failed` unit ends it at once with a warning** (P21 and the
     S01 batch, architect, 2026-11-21, for v0.08-T01). `patched` goes on to step 3. `failed` goes
     on to step 4, at its first answer, without another check. Every other outcome is a check that
     was not ready and gives the message `Patching Ubuntu (dist-upgrade, reboot if the kernel
     changed)...`: `pending`, `unknown`, an empty or unexpected answer, an invocation that ended in
     a status other than `Success`, and 20 polls without an end. **So the wait has none of the four
     exceptions of [Starting the box](#starting-the-box-and-waiting-for-it)'s rule**: it asks a
     host whatever its launch time, and counts neither `unknown`, nor a failed check, nor `failed`
     as finished. A `failed` answer ends it, and is never read as `patched`. So a host that
     `sync` has finished with has had this boot's patch run, finished or failed and reported.
   - **A `ssm send-command` that fails** is a check that was not ready, with the message `Waiting
     for Session Manager...`, since a host rebooting for its patch run refuses commands while
     Session Manager can still read it `Online`. **The one exception is #0034's sign-in failure**,
     which fails `sync` at once with #0034's message ([AWS failures](#aws-failures)), and ends
     `-All` too ([Exit codes](#exit-codes)).
   - **A failed `ssm describe-instance-information` fails `sync`**, under
     [AWS failures](#aws-failures), as it fails `up`.
   - **A message is printed only when it differs from the last one this wait printed**, so a wait
     through a reboot prints `Waiting for Session Manager...` again after `Patching Ubuntu
     (dist-upgrade, reboot if the kernel changed)...`.
   - **At the deadline** it fails with `agentspace: <id> did not finish its patch run within 30
     minutes; check: ssh agentspace journalctl -u agentspace-patch`, whose command is today's box's.
     For a host that is not the box, `agentspace` in it is the host's alias,
     `agentspace-host.<project>`. Rows 5 to 9 are not run, and the staging folder, which row 4
     removed, stays removed. **That is the outcome of a run that never finishes**, and it is a
     failure, as before P21: a `failed` answer never reaches it.
3. **On `patched`, prints `Patched: <id>`**, and goes on to `sync`'s step 6.
4. **On `failed`, prints one warning and goes on to `sync`'s step 6** (P21 and the S01 batch, architect,
   2026-11-21, for v0.08-T01):

   ```
   agentspace: The patch run on <id> failed; sync goes on without it. Check: ssh agentspace journalctl -u agentspace-patch
   ```

   As in the deadline's message, the command is today's box's, and for a host that is not the box
   `agentspace` in it is the host's alias, `agentspace-host.<project>`. **It prints no `Patched`
   line, and `sync`'s exit status is unchanged**: rows 5 to 9 run as after `patched`, and `sync`
   exits 0 when nothing else failed. It is a warning under [Messages](#messages), so `-Quiet`
   suppresses it. With `-All` the host's part is not a failed one, and is not counted in the
   closing count.

**The wait's timing is [Calls by command](#calls-by-command)'s for `up`'s wait**: the 5 seconds
are a pause after each check, the deadline is tested after each check that was not ready, and the
last pause is cut short to end on it.

**Why 30 minutes** (P13, architect, 2026-11-21, for v0.08-T01): the patch unit's own limit is 20
minutes (`TimeoutStartSec=20min`), and a run that asks for a reboot is followed, after the reboot,
by the next boot's run of the same unit, which writes the marker. `up`'s 15 minutes is shorter than
the unit's limit, so a slow first run would fail a `sync` that a later check would have passed. A
run that reaches the unit's limit is killed and leaves the unit `failed`, which ends the wait at
once (step 4), so the 30 minutes bound only a run that never finishes: a host that does not come
back from its patch reboot, a Session Manager agent that never comes online, or a run that reboots
twice.

**When the patch run fails** (P21, architect, 2026-11-21, for v0.08-T01), the wait ends at the
first `failed` answer, with step 4's warning, and `sync` goes on to row 5. **The wait MUST NOT go on
to its deadline against a unit that has already failed, and a patch run's failure MUST NOT fail
`sync`**: a patch run fails when apt does, a broken or unreachable apt mirror among the causes, and
a broken apt mirror MUST NOT block every `sync`, and through it every provisioning run and every
`deploy`. **The failed run gets a retry**: the install starts this boot's patch run again at every
`sync` while the marker is missing, a failed run's case included ([What sync
installs](#what-sync-installs)), so the next `sync` runs it again, as does the host's next boot.
**#0163's outcome still holds**: a host has had its patch run, finished or failed and reported,
before its first lease is placed. In a provisioning run, step 6 is `sync` exiting 0, so the run
logs `sync done; the patch run failed, see the host's journal` and goes on ([provision](#provision),
step 6); the warning goes to the run's stderr, which for a detached run goes nowhere, and the
failure's own record is the host's journal.

**When the 30 minutes run out**, `sync` fails at that row with the deadline's own message, and
nothing after row 4 runs. **In a provisioning run that is step 6's failure**: the run logs
`sync failed reason=<id> did not finish its patch run within 30 minutes; check: ...`, records
`failed` for a host that was never `ready`, and leaves a `ready` host `ready` (D48;
[provision](#provision)), with the project's leases `pending`. **D48's retry covers it**: a later
`create` or `get` for the project starts another run, at most once every 5 minutes, which finds
the host patched. **A patch run that reboots twice can outlast 30 minutes, and that path is the
designed answer, not a defect** (P13).

**Its messages are a person's**: `sync` never runs in caller mode
([What a caller sees](#what-a-caller-sees-d1-card-0087)). `-Quiet` suppresses steps 1 and 3, the
phase messages and step 4's warning; the deadline's failure always prints ([Messages](#messages)).

**What it costs a host already patched this boot**, the box on most days: one
`ssm describe-instance-information`, one `ssm send-command` and its polls, about 2 to 3 seconds,
and the two lines of steps 1 and 3.

**Help** (architect, 2026-11-21, for v0.08-T01):

```
Usage: agentspace-operator sync [-Host <project> | -All] [-Quiet]

Checks the agentspace SSH CA, then installs box/system and the CA's public
key on a host, waits until the host has finished this boot's patch run,
copies ~/.claude/CLAUDE.md there, builds and installs the localswim CLI from
C:\Projects\localswim, and sends the localswim Bot token and the laptop's
time zone. Starts the host first if it is stopped. Without -Host it acts on
the box; -All syncs every ready host, one at a time.

Fails before touching a host if the CA is missing, half-made, mismatched or
readable by anyone else.

Exit codes: 0 success, 1 failure.
```

### bootstrap

```
operator bootstrap [-Host <project>] [-Quiet]
```

**Runs the toolchain bootstrap on its host, as the login user** (D46, D49). Without `-Host` its
host is the box. The bootstrap is the repository's `box/bootstrap.sh`, carried inside the operator
as `box/system` is
([What the prototype took from its checkout](#what-the-prototype-took-from-its-checkout)), and
what it installs is [The box's files](#the-boxs-files)'s. **It is safe to run again**, on a new
host or on the box: a second run installs nothing that is already there, and so on the box it makes
the box's hand-installed Rust the bootstrap's (D46).

1. **Starts the host and waits for it** ([Starting the box](#starting-the-box-and-waiting-for-it)).
2. **For a host that is not the box, makes the operator's key, certificate and alias for it**
   ([How the operator reaches a host](#how-the-operator-reaches-a-host)), the CA check first.
3. **Prints `Running the toolchain bootstrap on the instance...`** and starts one ssh, as
   [Programs it starts](#programs-it-starts) says (or through [the test seam](#the-test-seam)):

   ```
   ssh -o ClearAllForwardings=yes <alias> <remote command>
   ```

   where `<alias>` is the host's alias, `agentspace` for the box, and `<remote command>` is one
   argument, the last, quoted as [What the laptop sends](#what-the-laptop-sends) says for Windows,
   under the same rule for its characters. **The ssh gets a pipe for its stdin, to which the
   operator writes the bootstrap's bytes, exactly as they are carried, and then closes it**; its
   stdout and stderr are the operator's, so the bootstrap's output passes through. **On the host,
   the remote step MUST**, as the login user and never prompting, run the script it reads from its
   stdin with bash, to its end, and exit with the script's status. The script uses `sudo` where it
   needs root, which the login user has with no password ([The `agentspace` SSH
   entry](#the-agentspace-ssh-entry)). Like every remote command, its text is the operator's own
   (D34).
4. **A non-zero exit of the ssh, 255 for an ssh that could not connect included, fails** with
   `agentspace: The toolchain bootstrap failed on the instance.` (architect, 2026-11-20, for v0.07-T01). What the
   script installed before it failed stays, and a second run picks up from there.
5. **Prints `Bootstrapped: <id>`** (architect, 2026-11-20, for v0.07-T01).

**There is no time limit of the operator's own**: the bootstrap takes about ten minutes on a new
host (D46), and its ssh is waited for until it exits. **The ssh's stdin is a pipe the operator
writes and closes**, never the operator's own stdin, so `bootstrap` reads nothing from a person.

**Stdout:** the bootstrap's stdout. **Stderr:** the messages above, the bootstrap's stderr, and
the nested ProxyCommand's lines, `The host is ready.` among them
([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)). **Exit codes:** 0 and 1.

**Help:**

```
Usage: agentspace-operator bootstrap [-Host <project>] [-Quiet]

Runs the agent toolchain's bootstrap on a host as its login user: gh, Node
through fnm, Claude Code, Codex, uv, build-essential, and Rust through
rustup. Starts the host first if it is stopped. Safe to run again. Without
-Host it acts on the box.

Exit codes: 0 success, 1 failure.
```

### provision

```
operator provision -Host <project> [-Quiet]
```

**Makes a project's host, or makes it again: deploys its stack, starts it and waits, runs `sync`
and `bootstrap`, records it `ready`, and places the project's pending leases** (D46, D48, D55). The
EC2 driver starts it detached when a lease needs a host, as `agentspace-operator provision -Host
<project>` with no other argument, and with the environment [Environment
variables](#environment-variables) gives a detached run ([driver.md, Starting a provisioning
run](driver.md#starting-a-provisioning-run); P12, architect, 2026-11-20, for v0.07-T01); **a person
MAY run it too** (D55), and a run by hand is recorded and logged the same way (P2, architect,
2026-11-20, for v0.07-T01). `-Host` is required ([Command line](#command-line), the host checks).
**`provision -Host swimcoach` provisions the box the same way**, through the box's own `deploy`,
`sync` and `bootstrap`, and its record stays `ready` throughout (step 3); its log and lock are in
`hosts\swimcoach\` ([state.md, A host's folder](state.md#a-hosts-folder)). **Its deploy step is
the box's `deploy`**: it reads `stack-parameters.json` and passes `SshPublicKey` and every member
of it but `InstanceType`, so a changed template is applied to the box, and its type is the one
step 4 finds in swimcoach's `pending` leases, or the one it has (D67, D71; architect, 2026-11-23,
for v0.09-T01, pin P6; [deploy](#deploy), step 4). **A run
on a host whose record is `ready` never changes its `state`** (P3, architect, 2026-11-20, for v0.07-T01; D48): a
host that is not `ready` sends its project's ready leases to the box, where ssh's strict key check
refuses them, so marking a `ready` host anything else would cut every lease of the project for the
run's length, and for good if a step failed. `creating` and `failed` apply only to a host that was
never `ready`. The files the run sends are written in one folder of its own for the whole run
([The operator's folder, one per command](#the-operators-folder-one-per-command)).

**From v0.09 a run also sizes its host: a resize is a provisioning run** (D68, as D71 changed it).
**`provision`'s command line is unchanged**: the driver starts `provision -Host <project>` as
before, and a person runs it the same way. The run finds its instance type itself, at its deploy
step, from the project's leases in the lease state file (step 4), and passes it to that step as
`-InstanceType` only when no live `ready` lease holds the host (D68's change note; architect,
2026-11-23, for v0.09-T01, pin P2(a)). **A resize runs every step below, entire**, as a run on a
`ready` host already does (D48, D71): deploy, start, `sync`, `bootstrap`, record and place, each
safe to repeat. **A run never takes a `ready` host out of `ready`, a resize included** (D48, D68):
a host is resized with its record `ready`, its project's `pending` leases waiting on the run, and
a failed deploy leaves the host the type it had, its record with no type, whether the run was to
pass a type or none (step 4.2; architect, 2026-11-23, for v0.09-T01, pins P26 and P30), the
failure logged in `provision.log` and the leases `pending` for the next run. The steps, in order:

1. **Finds the project's record**, reading the host registry without its lock, after the command
   line's `-Host` checks. **The project list is read only when there is no record** (P14,
   architect, 2026-11-20, for v0.07-T01), so a project list that is not valid never fails a run on
   a host that has a record, the box's included. **A project with no record that is not in the
   project list is refused** with
   `agentspace: Project '<project>' has no host and is not in <path>, so no host is made for it.`,
   where `<path>` is the project list's full path, exit 1, writing nothing, its log and its host's
   folder included (D54; P2 and P19, architect, 2026-11-20, for v0.07-T01; the message is
   confirmed by the architect, 2026-11-20, for v0.07-T01). A project with a record is provisioned whatever the list holds (D54).
   A project list that cannot be read, for a project with no record, fails as [The host registry,
   as the operator uses it](#the-host-registry-as-the-operator-uses-it) says, writing nothing.
2. **Opens its log, then takes the run's lock** (P14, P18 and P19, architect, 2026-11-20, for
   v0.07-T01):
   - **It opens `%USERPROFILE%\.agentspace\hosts\<project>\provision.log` for append itself**,
     creating the host's folder and the file if they are missing, and keeps it open until the run
     ends (its log, below). **A run that cannot open its log exits 1 and writes nothing
     else**: no record, no lock, no file. It prints
     `agentspace: Cannot open the provisioning log <path>: <error>.` to its stderr, which for a
     detached run goes nowhere (the message is confirmed by the architect, 2026-11-20, for v0.07-T01).
   - **Then it takes the run's lock, `%USERPROFILE%\.agentspace\hosts\<project>\provision.lock`,
     without waiting**, and holds it until the run ends (D55; P10, architect, 2026-11-20, for
     v0.07-T01). **A lock another run holds fails the run**: it writes one line to its log,
     `<time> run failed reason=A provisioning run for '<project>' is already running.`, prints that
     message as `agentspace: A provisioning run for '<project>' is already running.`, exits 1, and
     writes no record and nothing else (P18, architect, 2026-11-20, for v0.07-T01). A lock that
     cannot be taken for any other reason fails the same way, with
     `agentspace: Cannot take <path>: <error>.` as the message and the line's reason. Both
     messages are confirmed by the architect, 2026-11-20, for v0.07-T01. **From v0.09 a person's
     `deploy` of the host holds the lock for its life too** ([deploy](#deploy); architect,
     2026-11-23, for v0.09-T01, pin P34), and a run that finds it held then fails the same way,
     with the same line and message, since the lock does not say who holds it. The lock file's rules are [state.md, The provisioning
     run's lock](state.md#the-provisioning-runs-lock)'s.
3. **Records the run's start, under `hosts.lock`**, re-reading the registry, and sets
   `last_attempt_at` to now, the run's start, in UTC in state.md's timestamp form (P2, architect,
   2026-11-20, for v0.07-T01):
   - **No record:** it adds
     `{"project":"<project>","stack":"agentspace-host-<project>","state":"creating"}` (D44, D55),
     with `last_attempt_at`.
   - **A record that is `creating` or `failed`:** it sets `state` to `creating` too.
   - **A record that is `ready`: `last_attempt_at` is the only member it writes**, and `state`
     stays `ready`, so the project's ready leases keep reaching the host while the run works on it
     (P3, architect, 2026-11-20, for v0.07-T01).

   Then it logs `run begun` and prints `Provisioning the host of '<project>'...`.
4. **Finds its instance type, then deploys the host's stack, in five steps** (D68, as D71 and its
   change notes changed it; architect, 2026-11-23, for v0.09-T01, pin P26, which replaces pin
   P13's read of the lease state file without a lock). **The run holds `provision.lock`
   throughout, and takes the other two locks only in the order of the locks**: `provision.lock`,
   then the lease state lock, then `hosts.lock` ([state.md, The order of the
   locks](state.md#the-order-of-the-locks); [driver.md, Lock order](driver.md#lock-order)). What
   the EC2 driver's functions decide here, and what place sees of it, are
   [driver.md, A resizing run](driver.md#a-resizing-run)'s; this step owns its failures and its
   log lines.
   1. **Takes the lease state lock, then `hosts.lock`**, waiting for each as a lease command that
      changes leases does, up to 30 seconds ([state.md, The lease state
      lock](state.md#the-lease-state-lock); [state.md, The host registry
      lock](state.md#the-host-registry-lock)). The lease state lock is `<state folder>\leases.lock`
      in the state folder `AGENTSPACE_STATE_DIR` names, else `%USERPROFILE%\.agentspace`
      ([Environment variables](#environment-variables)), the folder whose leases step 9 places. **A
      lease state lock not taken within its 30 seconds** fails the step with
      `agentspace: The lease state lock <path> is held by another program; try again.`, in the form
      of the host registry lock's message ([The host registry, as the operator uses
      it](#the-host-registry-as-the-operator-uses-it)), and **one that cannot be opened for any
      other reason** with `agentspace: Cannot take <path>: <error>.`, the form of the run's own
      lock's message (step 2), `<path>` being the lock's full path (both messages: architect,
      2026-11-23, for v0.09-T01, pin P35); `hosts.lock` fails with the host registry's messages.
      Each fails the step before any call to AWS and before any write, so the
      record keeps its type and the leases stay `pending`.
   2. **Removes the record's `instance_type` and writes the registry, then releases `hosts.lock`**,
      whether it will pass a type or none (pin P26): it re-reads the registry under the lock, and
      changes nothing else in it. A record that holds none is left as it is. A registry that cannot
      be read or written fails the step with the host registry's messages, before any call to AWS;
      a write that fails leaves the file as it was, and the stack, which nothing has changed, still
      has that type. **From here until step 4.5 writes the type, the EC2 driver's place finds the
      record with no type, and answers a new lease `pending`** ([driver.md, On a `ready`
      host](driver.md#on-a-ready-host), row 2; pin P2).
   3. **Reads the lease state file**, `<state folder>\leases.json` beside that lock, whole and
      read-only, **under the lease state lock still held**, and writes nothing to it. It takes one
      reading's time, decides each lease's state by it, as [state.md, Live leases and the lease
      horizon](state.md#live-leases-and-the-lease-horizon) decides it, and **finds the type**:
      - **When the project holds a live lease whose state at the reading's time is `ready`**, an
        unexpired stored `ready` record of the project, **no type is found**, so the host keeps
        its size while a `ready` lease holds it (D66: "a host is resized only while no ready lease
        holds it"; D68's change note; architect, 2026-11-23, for v0.09-T01, pin P2(a)). Its deploy
        then passes no `InstanceType`, so the stack's change set holds no resize and the host is
        not stopped, and step 4.5 still records the type it reads back.
      - **Otherwise the type is what the EC2 driver's `resize_target(<every record of the file>,
        <project>)` gives** ([driver.md, Sizes and instance
        types](driver.md#sizes-and-instance-types); D71's pin): the instance type of the largest
        size among the project's `pending` records whose `driver` is the EC2 driver's name and
        whose size is in `SIZES`, the leases a run would place, or none (pins P18 and P20). **The
        run never maps a size to a type itself** (D67: the map is the driver's alone). A `pending`
        lease the host the run leaves does not meet is the driver's to answer, in step 9 (pin P1;
        [driver.md, place_pending](driver.md#place_pending)).
      - **A missing file holds no leases** (pin P26, as P13 had it), so no lease holds the host, no
        type is found, and the run goes on with none. Unlike the guard, which MUST NOT read a
        missing file as "no leases" ([guard](#guard), step 3), the run decides only whether to
        change a size, and a missing file asks for no change.
      - **A file that cannot be read, or is not valid by state.md's checks, two records with one
        `id` among them, fails the step** before any call to AWS (pin P26), with
        `agentspace: The lease state file <path> cannot be read: <reason>.`, `<path>` and
        `<reason>` as the guard writes them ([guard](#guard), step 3), as every failure of the
        deploy step fails ("When step 4, 5, 6 or 7 fails", below), logged as `deploy failed`. **The
        record is left with no type**, which step 4.2 removed, and the leases stay `pending`: a run
        that cannot read the leases does not guess at a size, and the next run tries again and
        records the type (D36). That costs one run.
   4. **Releases the lease state lock, and only then calls AWS**: [deploy](#deploy)'s existence
      check and its steps 1 to 5, with `-Host <project>`, and with `-InstanceType <type>` when step
      4.3 found a type and without it when it found none, and never its step 6, since step 6 here
      is `sync` (D46). It holds no lock but `provision.lock` during the calls to AWS. **It runs
      under the `provision.lock` the run took at its step 2, and takes nothing more**: unlike a
      person's `deploy`, the step never tries that lock, which its own run holds (architect,
      2026-11-23, for v0.09-T01, pin P34; [deploy](#deploy)).
      Without a type, a stack that exists keeps its `InstanceType`'s previous value, the box's
      included, and a stack the step creates takes the template's default (P13 of v0.07-T01, D67).
      With a type, deploy's step 4 finds the type already removed by step 4.2, and removes it again
      only if another writer recorded one since. Its step 5 writes the instance ID into the record,
      as before.
   5. **Once the stack's deploy has succeeded, deploy's step 5 writes the type `describe-stacks`
      reads back** as the record's `instance_type`, under `hosts.lock` alone, with the instance
      ID, holding no lock but `provision.lock` during the AWS call (D67, D71; architect, 2026-11-23, for v0.09-T01, pins
      P11 and P23). It does so on every run, whether it passed a type or none, so each host's first
      run after v0.09's install records its type, and after a successful deploy step the record
      holds the stack's type, the box's included.

   **Why the order closes the race** (pin P26). A `create` holds the lease state lock for its whole
   place, so it is either wholly before step 4.1 or wholly after it. **Wholly before**, its
   `ready` lease is in the file step 4.3 reads, and the run passes no type, so the host is not
   resized under it. **Wholly after**, it finds the record with no type and answers `pending` by
   pin P2, until step 4.5 writes the type the host then has ([driver.md, A resizing
   run](driver.md#a-resizing-run)). **A deploy step that fails, or a run killed, anywhere after
   step 4.2 leaves the record with no type, by design** (architect, 2026-11-23, for v0.09-T01,
   pin P30), whether it was to pass a type or none; the next run records it.

   **A run on the box finds its type the same way**, from swimcoach's leases (D70: the box's
   first lease after the install records its type at `m`, which changes nothing on it).
5. **Starts the host and waits for it** ([Starting the box](#starting-the-box-and-waiting-for-it)),
   as `up` does, with no push.
6. **Runs `sync`**: every step of [sync](#sync), with `-Host <project>`, its patch wait and its
   forced push included. So a new host's first run has its patch run before its first lease is
   placed, with no 15-minute wait of `up`'s (D61's change, #0163); a wait that runs out fails the
   step as [The patch wait](#the-patch-wait) says. **A wait that ended on `failed` is recorded in
   the step's line**, `sync done; the patch run failed, see the host's journal` in place of
   `sync done` (architect, 2026-11-21, for v0.08-T01, pin P27), since a detached run's
   warning reaches no one ([Its log](#provision), below).
7. **Runs `bootstrap`**: every step of [bootstrap](#bootstrap), with `-Host <project>`.
8. **Records the host `ready`, under `hosts.lock` alone**: `instance_id` the ID step 4 read,
   `last_attempt_at` left as step 3 wrote it, and `state` `ready` for a host that was not `ready`;
   a record that was `ready` keeps `ready`, and only its `instance_id` is updated, when the run
   learned one that differs (P3, architect, 2026-11-20, for v0.07-T01). **It never writes
   `instance_type`**, which step 4's deploy recorded, and leaves it as it finds it (D67). Then it releases
   `hosts.lock` (D48; P10, architect, 2026-11-20, for v0.07-T01). It logs `record done` and prints
   `Recorded the host of '<project>' as ready.`
9. **Places the project's pending leases**, by calling the EC2 driver's `place_pending` for the
   project, which places every `pending` lease of the project as `create` places one, one at a
   time: it takes the lease state lock once per lease and re-reads the lease state file each time,
   so no hold spans several placements, and it removes a killed placement's leftover files before
   it places a lease (D48; P3 and P14, architect, 2026-11-20, for v0.07-T01; [driver.md,
   place_pending](driver.md#place_pending)). **From v0.09 it places only the leases the host now
   meets, by the type step 4 recorded, and answers the others** (D68's change note; architect,
   2026-11-23, for v0.09-T01, pin P1): a `pending` lease the host does not meet becomes
   `unsatisfiable` with `no_capacity` when a live `ready` lease holds the host, and stays `pending`
   for the next run when none does. Which lease it places and what it answers the rest are
   driver.md's, and the run logs what the call returns, as below.
   The operator links the driver's crate, and starts no
   program for this. **The driver's `Context` for the call is built from the run's environment as
   the lease CLI builds its own**: the state folder, the ssh folder and the ProxyCommand line
   ([Environment variables](#environment-variables); P12, architect, 2026-11-20, for v0.07-T01).
   **The default ProxyCommand line names the program found beside the run's own executable**,
   `agentspace-operator.exe`'s folder, as the lease CLI finds it
   ([How each program finds the others](#how-each-program-finds-the-others); P15, architect,
   2026-11-20, for v0.07-T01). When it is not there, each lease's placement fails and the lease
   stays `pending`, so every lease of the run stays `pending`; the call itself succeeds, as for
   any lease it leaves `pending`. **On a host that is not the box, the run fails before step 9**:
   step 6's operator alias needs the same program (architect, 2026-11-21, for v0.07-T04).
   `AGENTSPACE_PROXY_COMMAND`, when the run has it, replaces the whole line. It logs `place done`.
   **A lease the call could not place while the call succeeds is logged and not warned**: one
   `place failed lease=<id>` line each (below), and the run goes on (architect, 2026-11-21, for
   v0.07-T04). **So, from v0.09, is a lease the call answered or left for the host it does not
   meet**: one `place refused lease=<id>` line for each it answered `unsatisfiable` with
   `no_capacity`, and one `place left lease=<id>` line for each it left `pending` for the next run
   (below; architect, 2026-11-23, for v0.09-T01, pin P37). **A failure of the call as a whole
   leaves the host `ready` and the leases `pending`**: it logs `place failed` with the reason, warns
   `agentspace: The pending leases of '<project>' could not be placed: <reason>; the next run places
   them.`, where `<reason>` is the failure message without its `agentspace: ` prefix and its last
   period (architect, 2026-11-21, for v0.07-T04), and the run goes on to step 10 and exits 0 (P3;
   the warning and the exit code are confirmed by the architect, 2026-11-20, for v0.07-T01). The
   next run P2's rule allows places them.
10. **Prints where the sign-in procedure is** (D52), as the run's last line, **only for a host
    that was not `ready` when the run began, and never for the box**, since a host that was
    already `ready` has had its sign-ins (the condition is confirmed by the architect, 2026-11-20, for v0.07-T01):
    `Sign in on this host once, by hand, under a lease: see "Signing in on a new host" in the agentspace README.`
    **The line is exactly that, byte for byte** (architect, 2026-11-21, for v0.08-T01, pin P20). It
    replaces v0.07's line, which sent a person to `agentspace-operator status -Host <project>`
    first for a one-hour certificate: from v0.08 a host runs only while its project holds a live
    lease (D56), so the sign-ins are made under a lease, as the README's procedure says. The procedure is written in the README (D52); the heading named here is
    confirmed by the architect, 2026-11-20, for v0.07-T01. E0.08-T03-S05 (#0181) builds the new
    line in `agentspace-sync`. It logs `run done`, releases `provision.lock`, and exits 0.

**A host is `ready` for leases before the sign-ins** (D52): `ready` means a connection is expected
to succeed (lease-api.md, States), not that an agent can work there yet.

**When step 4, 5, 6 or 7 fails**, the run:
- **Logs `<step> failed` with the step's failure message as its reason** (the step names are below).
- **Records the failure under `hosts.lock`, for a host that was never `ready`**: `state` `failed`,
  `last_attempt_at` left as step 3 wrote it, and `instance_id` kept when the run learned one (P2).
  **A record that was `ready` when the run began is not written at all**: the failure is logged,
  and the record stays `ready`, with the `last_attempt_at` step 3 wrote (P3, architect, 2026-11-20,
  for v0.07-T01). A registry that cannot be written then is logged, and changes nothing else.
  **Neither write touches `instance_type`, and nothing undoes what the deploy step wrote of it**
  (architect, 2026-11-23, for v0.09-T01, pins P11, P12, P26 and P30): after a deploy step that
  failed after its step 4.2, the record holds no type, whether the run was to pass a type or none;
  after one that failed in step 4.1, or on step 4.2's own write, the type it held before, which the
  stack still has, since no call to AWS was made; after a later step fails, the type the deploy
  recorded (D67, D68).
- **Prints the step's own failure message as its last line**, releases `provision.lock` and exits 1.
- **Leaves the project's leases `pending`, never `unsatisfiable`** (D48): a later `create` or `get`
  for the project starts another run, at most once every 5 minutes (P2; driver.md).
- An expired AWS sign-in is such a failure, with #0034's message, logged like any other.

**When step 3 or step 8 cannot change the registry** (its lock not taken, or a write that fails),
the run logs `run failed` or `record failed` with the registry's message, prints that message,
records nothing more, releases `provision.lock` and exits 1. A run that stopped at step 8 leaves
the record as step 3 and step 4 left it: `creating` with the instance ID step 4 wrote, or `ready`
for a host that was `ready`, so the next run P2 allows starts again from step 1.

**A run that is killed**, at any step, ends with no line of its own. **Its lock goes with its
process**, since the operating system releases it however the run ends ([state.md, The
provisioning run's lock](state.md#the-provisioning-runs-lock)). **Its record stays as it last
wrote it**: `creating` for a host that was never `ready`, and `ready` for one that was, with the
`last_attempt_at` of its start, so the next run P2's rule allows comes at least 5 minutes after
it. **That run starts at step 1, and every step is safe to run again**: the deploy step finds the
stack in whatever state the killed run left it (the existence check, [deploy](#deploy)), `sync`
and `bootstrap` repeat safely, and `place_pending` removes a killed placement's leftover files
before it places a lease. The killed run's folder under `operator\` is removed by the 24-hour
sweep ([The operator's folder, one per command](#the-operators-folder-one-per-command)).

**Its log, `%USERPROFILE%\.agentspace\hosts\<project>\provision.log`** (P4, architect, 2026-11-20,
for v0.07-T01: D55's path wins over D48's `<state folder>\hosts\<project>.log`). **The run opens
it for append itself, at step 2, and writes every line of it**, a run by hand as a detached one
(P14, architect, 2026-11-20, for v0.07-T01). The run appends
one line per step, each line landing whole at the end of the file even when two processes write at
once, and drops a line it cannot write after 5 tries 50 ms apart, as the keeper does with its own
([keeper.md, Log lines](keeper.md#log-lines)). A dropped line changes nothing the run does. The
file's bytes and access rules are [state.md, A host's folder](state.md#a-hosts-folder)'s; it is
never rotated. **A line is** (architect, 2026-11-20, for v0.07-T01):

```
<time> <step> <outcome>[ type=<type>][ placed=<n>][ lease=<id>][ reason=<reason>]
```

- **`<time>`** is the time the line is written, in UTC, as `YYYY-MM-DDTHH:MM:SSZ`, the form of the
  keeper's lines.
- **`<step>`** is one of `run`, `deploy`, `start`, `sync`, `bootstrap`, `record` and `place`.
- **`<outcome>`** is `begun` (for `run` alone), `done`, `failed`, or, from v0.09, `refused` or
  `left` (for `place` alone; architect, 2026-11-23, for v0.09-T01, pin P37). A run's lines are
  `run begun`, then `done` or `failed` for each step it reached, in order, step 9's lines for each
  lease after its `place done` (below), then `run done` when it succeeded.
  A run whose step 2 found the lock held, or could not take it, writes one line, `run failed`,
  with no `run begun` before it. **The one line with more after `done`** is step 6's when its
  patch wait ended on `failed`: `<time> sync done; the patch run failed, see the host's journal`,
  exactly, with no other field (architect, 2026-11-21, for v0.08-T01, pin P27).
- **`reason`** is written for `failed` alone, is the last field and runs to the end of the line: the
  failure message without its `agentspace: ` prefix, with each control character in it written as
  `?`. It MUST NOT hold the token or any text of the Bot credential ([Secrecy](#secrecy)), which no
  failure message holds.
- **Step 9, `place`, logs what `place_pending` did** (architect, 2026-11-21, for v0.07-T03): when
  the call succeeds, `place done placed=<n>`, `<n>` the number of leases it placed, then one line
  for each lease it did not place, in the order it tried them, `<id>` the lease's normalized ID:
  - **`place failed lease=<id> reason=<message>`** for each lease whose placement failed, or, from
    v0.09, whose `unsatisfiable` answer failed on a removal or a write, so that it stays `pending`
    ([driver.md, place_pending](driver.md#place_pending), steps 2.7 and 3.4); byte for byte as
    v0.07 had it.
  - **`place refused lease=<id>`**, from v0.09, for each lease `place_pending` answered
    `unsatisfiable` with `no_capacity` (pin P1), the host not meeting it while a live `ready` lease
    of the project holds the host.
  - **`place left lease=<id>`**, from v0.09, for each lease the host does not meet that it left
    `pending` for the next run, with no live `ready` lease holding the host.

  `refused` and `left` lines carry no `reason` field, which stays `failed`'s alone, and the
  message of a refused lease's reason is in its lease record, not the log (architect, 2026-11-23,
  for v0.09-T01, pin P37). A `pending` lease whose `size` is not in `SIZES`, which `place_pending`
  neither places nor answers (pin P20), has no line. When the call fails as a whole, one
  `place failed reason=<message>`, with the warning above. `placed` and `lease` appear on `place`
  lines alone, `lease` on every `failed` line for a lease and on every `refused` and `left` line.
- **`type`** is written on step 4's line alone, `deploy done` or `deploy failed`, when step 4.3
  found a type, and is that type, the value the run passed as `-InstanceType` (D68: a failed
  deploy is logged with what it was asked for, and leaves the host the type it had). A run that
  found none writes no `type`, so a run with no `pending` lease of a supported size logs
  `deploy done` as before v0.09. A step 4 that failed taking its locks, or reading the lease
  state file, found no type, and writes none. So `<time> deploy done type=m9g.2xlarge` is a
  resize's line, and
  `<time> deploy failed type=m9g.2xlarge reason=Stack deploy failed; see the stack events in the console.`
  a failed one's. The field is architect, 2026-11-23, for v0.09-T01, pin P24; state.md's [The
  provisioning run's log](state.md#the-provisioning-runs-log) gives the file's bytes, and this
  line's fields are this page's.
- **Every line of the log is written by the run itself, and is one of its own lines**
  ([state.md, The provisioning run's log](state.md#the-provisioning-runs-log)). No other program
  writes to the log. **A step that fails logs its whole failure message as the line's `reason`**,
  and for a program the step ran that message carries the program's own error text, as every
  operator failure does ([AWS failures](#aws-failures)), so the log holds what a person needs to
  see why a run failed. **The programs' other output is not copied into the log**: the operator
  hands each program's stdout and stderr to its own, so it goes to the run's streams (architect,
  2026-11-21, for v0.07-T03, replacing P14's "the output of the programs between its own lines",
  which the pinned `Aws` and `Remote` cannot carry; copying it is [Q35](#open-questions)). That
  output passes through to the run's stdout and stderr, and
  its progress messages go to its stderr, as for any command: a run by hand writes them where the
  person's shell sends them. **A run the driver starts detached has none of the CLI's standard
  handles**: its stdin, stdout and stderr are its hidden console's, which nothing reads or writes
  ([driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run); P14, architect,
  2026-11-20, for v0.07-T01, corrected for v0.07-T02), so its log is the only record of it. **A run
  MUST work whatever its streams are, null or a hidden console included, MUST NOT read its stdin**,
  and nothing it writes to its stdout or stderr can fail it.

**Stdout:** the output its steps pass through. **Stderr:** the messages above and those of its
steps. **Its log:** its own lines alone, as above. **Exit codes:** 0 when the host
is recorded `ready`, whatever step 9 did; 1 otherwise.
`-Quiet` quiets every step.

**Help:**

```
Usage: agentspace-operator provision -Host <project> [-Quiet]

Makes a project's host, or makes it again: deploys its stack, starts it and
waits, runs sync and bootstrap, records it ready in ~/.agentspace/hosts.json,
and places the project's pending leases. Its deploy gives the host the
instance type the largest of those leases needs, unless a ready lease holds
the host. A project with no host is made only if it is listed in
~/.agentspace/projects.json. One run per host at a time; each run logs to
~/.agentspace/hosts/<project>/provision.log.

Lease commands start a run on their own when a lease needs a host; you can
also run it yourself.

Exit codes: 0 the host is ready, 1 failure.
```

### ca

```
operator ca [-Quiet]
```

**Creates the agentspace SSH CA the first time, and on every later run checks it and changes
nothing (D7, D17, D21).** The CA's files, their location and their access rules are docs/state.md's;
this section gives what `ca` does and says. `ca` makes no AWS call and touches no box.

**The lease CLI does not run `ca`.** Its `create` and `renew` check the CA themselves, by the
checks of [state.md](state.md#its-checks), and MUST NOT create one
([state.md](state.md#its-checks)). `ca` on an existing, sound CA MUST still exit 0 and change
nothing.
- Note on where this came from: the prototype's lease CLI runs `ca` as its CA check, and reads
  only its exit code (`scripts/lease-credential.ps1`), so on a laptop with no CA its `create`
  makes one. state.md lists that as a requirement the prototype does not meet.

**In order, it:**
1. **Fails if `ssh-keygen.exe` is missing:** `agentspace: <path> is missing; install the Windows
   OpenSSH client.`
2. **Fails if the CA's folder is inside a OneDrive folder or inside any git working tree** (D37;
   the prototype checks only its own `<checkout>`), compared as paths without regard to case. A
   git working tree is the one [What the prototype took from its checkout](#what-the-prototype-took-from-its-checkout)
   defines. The OneDrive folders are the values of `OneDrive`,
   `OneDriveConsumer` and `OneDriveCommercial`, each one that is set and not empty
   ([Environment variables](#environment-variables)): `agentspace: <ca folder> is inside <folder>; the CA key MUST NOT
   live in a synced folder or in the repo.`
3. **Fails on a half-made CA, and never repairs one:**
   - Private key without public key: `agentspace: <key> exists but <pub> does not. ca does not
     repair a half-made CA: restore the public key, or write it again from the private key with
     ssh-keygen -y, then run ca again.`
   - Public key without private key: `agentspace: <pub> exists but <key> does not: the CA's
     private key is lost, and nothing more can be signed with it. ca does not replace it on its
     own. To make a new CA, delete <pub> and run ca again, then run sync for every host and
     re-issue every lease's certificate.`
4. **When neither file exists, creates the CA:**
   - **Fails when run elevated** (a member of Administrators with an elevated token), because
     Administrators would own what it creates: `agentspace: ca MUST NOT create the CA from an
     elevated shell; run it without "Run as administrator".`
   - **Prints `Creating the agentspace SSH CA in <ca folder>...`**, creates the folder, and locks
     it to the current user alone before any key exists in it. A failure to lock fails with
     `agentspace: icacls could not lock <ca folder> to <user>: <tool output>`, and a folder still
     open to others fails with `agentspace: <ca folder> MUST be <user>'s alone before the CA is
     made in it, but it <problems>. Remove the other rules, or delete the directory, and run ca
     again.`
   - **Makes an ed25519 key pair with no passphrase.** A failure fails with `agentspace:
     ssh-keygen could not create the CA: <tool output>`.
   - **Locks the private key to the current user alone.** A failure fails with `agentspace: icacls
     could not lock the new <key> to <user>: <tool output>. Delete <key> and <pub>, and run ca
     again.`, and a key still open to others with `agentspace: The new <key> MUST be <user>'s
     alone, but it <problems>. Treat it as exposed: delete <key> and <pub>, and run ca again.`
5. **Checks the CA**, by the checks that [docs/state.md, The CA](state.md#its-checks) owns (the
   same checks `sync` makes), in its order, the first failure ending `ca`. Each failure's message
   is `ca`'s own, and the check it belongs to is the number in front of it:
   1. **Both files exist:** `agentspace: <key> does not exist; run ca.` or `agentspace: <pub> does
      not exist; run ca.`
   2. **The folder, then the private key, are owner-only:**

      ```
      agentspace: <path> MUST be <user>'s alone, but it <problems>. ca does not change access rules on an existing CA. If anyone else may have read the key, replace the CA (D17). Otherwise lock it by hand: icacls "<ca folder>" /inheritance:r /grant:r "<user>:(OI)(CI)F", then icacls "<key>" /reset, then icacls "<key>" /inheritance:d, then remove any other rule icacls still lists, and run ca again.
      ```
   3. **The private key loads with an empty passphrase:** `agentspace: ssh-keygen could not read
      <key> without a passphrase; the lease CLI signs unattended (D17).`
   4. **The public key is the private key's own, and `ssh-ed25519`:** `agentspace: <pub> is not
      the ed25519 public key of <key>. ca does not repair a mismatched pair.`
   5. **The public key's fingerprint can be read:** `agentspace: ssh-keygen could not read <pub>:
      <tool output>`
6. **Prints the result**, four lines, the first of which says whether it created the CA:

   ```
   Created the agentspace SSH CA.
   Private key  <key>
   Public key   <pub>
   Fingerprint  <fp>
   ```

   or, for an existing CA, `The agentspace SSH CA already exists; nothing changed.` in place of the
   first line. `-Quiet` suppresses all four.

**`<problems>`** joins, with `; `, each of these that applies, in this order: `is owned by
<owner>`, `inherits access rules from its parent`, then for each access rule `gives <user>
<rights>, not full control` (the user's own rule, short of full control), `has a rule allowing
<who>` or `has a rule denying <who>`, and last `has <n> access rules instead of one`.

**The forms of the placeholders in `ca`'s messages** (A-23, B-31):
- **`<user>`** is the value of `USERNAME`, with no domain ([Messages](#messages)).
- **`<owner>` and `<who>`** are an account as Windows names it, `<domain>\<name>` (such as
  `BUILTIN\Administrators` or `NT AUTHORITY\SYSTEM`), when Windows can translate its security
  identifier, and the identifier itself in `S-1-...` form when it cannot.
- **`<rights>`** names the rights the user's own rule grants, in words a person can read. Its exact
  text is not part of the contract, and a check MUST NOT compare it. The prototype writes .NET's
  `FileSystemRights` names, joined with `, ` (such as `ReadAndExecute, Synchronize`).
- **`<n>`** is a decimal count.
- **These carry identifying text:** `<user>` and every path under `%USERPROFILE%` carry the Windows
  user name, and `<owner>` or `<who>` for a local account carries the computer's name as its
  `<domain>`. A check that captures `ca`'s or `sync`'s stderr MUST withhold them from anything
  committed or reported, as it does the instance ID ([Messages](#messages)).
- **`<tool output>`** is the named program's own output, stdout and stderr together, trimmed. The
  rewrite MAY make a step's change without starting the program the message names (setting the
  access rules itself rather than through `icacls`, say); the message then keeps its text, the
  program's name included as the name of the step, and `<tool output>` is the system's own error
  text.

**Rulings for the rewrite's `ca`** (architect, 2026-11-19, for E0.06-T02-S01):
- **`agentspace-ca` reads no environment variable.** `USERNAME`, `COMPUTERNAME`, `OneDrive`,
  `OneDriveConsumer` and `OneDriveCommercial` reach it in an input struct of its own, which the
  operator's program fills from its environment as it starts, beside the pinned `Context`.
- **Locking the CA's folder** replaces its whole access list with one protected rule giving the
  user full control; "Remove the other rules" then fires only for a folder someone else owns, or a
  lock that did not take.
- **Step 1's "missing" `ssh-keygen.exe`** is anything at that path that is not a file, a folder
  included.
- **`<user>` with `USERNAME` unset** is the token user's account name, and failing that the words
  `the current user`.
- **What cannot be read fails closed:** access rules that cannot be read are the problem
  `could not be checked: <error>` with that check's message; an elevation that cannot be read is
  refused with the elevated message; a CA folder that cannot be created fails with
  `agentspace: Could not create <ca folder>: <error>`.
- **Check 5's `<tool output>`**, since no tool runs, is `It is not an OpenSSH public key.`; a null
  access list's `<problems>` is `has a rule allowing Everyone; has 0 access rules instead of one`.

**`ca` MUST NOT overwrite, repair or loosen a CA, and MUST NOT change the access rules of an
existing one.** A person decides what to do; if the private key may have been exposed, the CA is
replaced: delete both files, run `ca`, run `sync` for the box, recover every other host as below,
and re-issue every live lease's certificate (D17, D50). **One CA serves every host** (D50), and
`ca` takes no `-Host`.

**After a replacement, no operator command reaches a host that is not the box** (architect,
2026-11-20, for v0.07-T01, P19; D17): the operator's certificate for it is signed by the new CA,
which that host's sshd does not trust, since it still holds the old CA's public key. So `sync`
cannot bring the new key to it, and every lease alias for it is refused too. The box is reached as
before, through the user's own key and entry. **A host whose stack does not exist yet** is created
trusting the new CA ([deploy](#deploy), step 3), and needs nothing. **Every other host is
recovered by hand, one at a time:**
1. **The user's own `id_ed25519` is `SshPublicKey` on every host**, installed for the login user at
   its first boot ([The stack](#the-stack)), so a person connects with it through the host's
   operator alias, and so through the host's ProxyCommand, with the key given on ssh's command
   line, such as
   `ssh -i "%USERPROFILE%\.ssh\id_ed25519" agentspace-host.<project>` from cmd, with the path
   written out. The operator alias exists for every host the operator has reached.
2. **Over that connection, the person does `sync`'s CA step by hand**: puts the new CA's public key,
   `%USERPROFILE%\.agentspace\ca\agentspace_ca.pub`, in place of the old one, as root, at the path
   the host's sshd trusts (`/etc/ssh/agentspace_ca.pub` on today's box), and reloads sshd, as
   [What sync installs](#what-sync-installs) does.
3. **Then runs `agentspace-operator sync -Host <project>`**, which now reaches the host, and
   installs the rest as every `sync` does.

How the new key could reach every host with no ssh is [Q34](#open-questions).

**Stdout:** nothing. **Stderr:** the messages above. **Exit codes:** 0 and 1.

**Help:**

```
Usage: agentspace-operator ca [-Quiet]

Creates the agentspace SSH CA (ed25519, no passphrase) in
%USERPROFILE%\.agentspace\ca the first time, locked to the current user.
Later runs change nothing: they check the key pair and its access rules, and
print its paths and fingerprint.

A half-made, mismatched or loosened CA fails with what is wrong. ca never
repairs or overwrites a CA. Run it without "Run as administrator".

Exit codes: 0 the CA exists and is sound, 1 failure.
```

### down

```
operator down [-Host <project> | -All] [-Quiet]
```

**Asks EC2 to stop its host's instance, and prints `Stopping <id>.`** once the request is
accepted. Without `-Host` its host is the box. It does not wait for the stop, and it asks whatever
state the instance is in; stopping a stopped instance succeeds. Open sessions on the host end with
it. **`down -All` does this for every host** whose record holds an instance ID, or is `ready`
([Every host at once](#every-host-at-once--all)). Until v0.08 is installed (#0186), it is one of
the two tools of a turn's close (D49, D51): a turn's close stops every host it used with
`down -Host` or `down -All`, and confirms with `status -All` that each is `stopped`. **From v0.08
the guard stops every host that no
live lease explains, on its own** ([guard](#guard); D56), and `down` stays a person's way to stop a
host at once, whatever its leases. **`down -Host` for a host that is not the box
then writes its alias and signs a fresh one-hour certificate for it** (P13 and P15, architect,
2026-11-20, for v0.07-T01), after the stop is requested, as [How the operator reaches a
host](#how-the-operator-reaches-a-host) says; a failure there fails `down`, and the stop already
requested stands. **`down -All` MUST NOT sign or write an alias for any host**, so a turn's close
(D51) never fails on the CA ([Every host at once](#every-host-at-once--all)).

**Stdout:** nothing. **Stderr:** that line, and with `-All` each host's `Host` line. **Exit
codes:** 0 and 1.

**Help:**

```
Usage: agentspace-operator down [-Host <project> | -All] [-Quiet]

Stops a host, or with -All every host. Does not wait for it to stop. Open
sessions on it end. The next connection to it, a lease's or the operator's,
starts it again. Without -Host it stops the box.

Exit codes: 0 the stop was requested, 1 failure.
```

### status

```
operator status [-Host <project> | -All] [-Quiet]
```

**Prints five lines about its host's instance, to stderr, whatever `-Quiet` says.** Without
`-Host` its host is the box, and the report is exactly as before v0.07:

```
Instance  <id>
State     <state>
Type      <type>
Disk      <size> GiB gp3 (<vol>)
SSM       <ssm>
```

- **`<state>`** is EC2's instance state name (`pending`, `running`, `stopping`, `stopped`,
  `shutting-down` or `terminated`).
- **`<type>`** is the instance type. **`<size>`** is the root volume's size in GiB, and **`<vol>`**
  its volume ID. The root volume is the one attached at the instance's root device name.
- **`gp3` is printed as a constant** in the prototype, not read from the volume
  ([Q16](#open-questions)).
- **`<ssm>`** is `online` when the instance is running and Session Manager reports its agent
  `Online`, `not yet` when it is running and not online, and `-` in every other state.

**`status` never starts a host.** **Stdout:** nothing. **Exit codes:** 0 and 1. **The five
lines print only once every call they need has succeeded** (architect, 2026-11-19, for
E0.06-T04-S02): a failed call prints its failure line alone, never a partial report before it.

**`status -Host <project>` prints the same five lines for that host**, and then, for a host that is
not the box, writes its alias and signs a fresh one-hour certificate for it (P13 and P15,
architect, 2026-11-20, for v0.07-T01; [How the operator reaches a
host](#how-the-operator-reaches-a-host)). **So `status -Host <project>` is what a person runs
before signing in over `ssh agentspace-host.<project>`, and again after an hour, under a live
lease of the project that they created first** (D56), as [README, Signing in on a new
host](../README.md#signing-in-on-a-new-host) gives the steps: `status` neither needs nor makes a
lease, and without one the guard stops the host mid sign-in ([Work on a host by
hand](#what-the-guard-can-race)). A failure of that signing fails `status` after its report.

**`status` and `status -Host` print the five lines alone, and MUST NOT read the lease state file**
(architect, 2026-11-21, for v0.08-T01, S01 batch: D60 gives the lease lines to `-All`, and without `-All` the report stays
exactly as before v0.07, as [From the command line](#from-the-command-line) promises for every
command line without `-Host`).

#### status -All

**`status -All` reports every host in the host registry** (D49, D60), each as `-Host` would, after
the `Host` line that `-All` prints before each host
([Every host at once](#every-host-at-once--all)), **but MUST NOT sign or write an alias for any
host**, so its check never fails on the CA. A host whose record is `creating` or `failed` and
holds no instance ID gets one line, `Instance  none recorded`, after its `Host` line, and no AWS
call.

**After each host's report, `status -All` prints its leases, and for a running host that no live
lease explains, the guard's stop** (D60, D63), in this order, after the host's existing lines and
before the next host's `Host` line:

```
Leases    <n> live
Guard     unexplained; stops at <h:mm><am|pm>
```

Each label is padded with spaces to 10 characters, as the other lines' are: `Leases` and four
spaces, `Guard` and five. So, for the box under a live lease, agentspace's host running with none,
and a host still being made under a pending lease (architect, 2026-11-20, for v0.07-T01, for the
lines before v0.08's):

```
Host      swimcoach (agentbox), ready
Instance  <id>
State     running
Type      <type>
Disk      <size> GiB gp3 (<vol>)
SSM       online
Leases    1 live
Host      agentspace (agentspace-host-agentspace), ready
Instance  <id>
State     running
Type      <type>
Disk      <size> GiB gp3 (<vol>)
SSM       online
Leases    0 live
Guard     unexplained; stops at <h:mm><am|pm>
Host      example (agentspace-host-example), creating
Instance  none recorded
Leases    1 live
```

- **The lease state file is read once, before the first host's part**, from
  `<state folder>\leases.json` ([state.md, The lease state file](state.md#the-lease-state-file);
  `AGENTSPACE_STATE_DIR` moves it, [Environment variables](#environment-variables)), **without
  taking the lease state lock, as the keeper and the ProxyCommand read it, and is never written.**
  One time, the report's time, is taken as it is read, and every host's count and stop time use it
  (architect, 2026-11-21, for v0.08-T01, S01 batch).
- **A project's leases, a live lease and the project's last lease end are
  [state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon)'s**
  (D63; P1, P10 and P11, architect, 2026-11-21, for v0.08-T01), applied at the report's time. A
  host's project is its record's `project`, so the box's leases are swimcoach's.
- **`Leases    <n> live`** prints for every host whose part did not fail, when the lease state file
  was read (below): after its five lines, or after `Instance  none recorded` (architect,
  2026-11-21, for v0.08-T01, S01 batch). `<n>` is the count of the project's live
  leases, in decimal with no leading zero, `0` included. **It is a count, never a lease's ID, alias
  or any other of its members** (D60), so the report is safe to show in a live run's record once
  its instance and volume IDs are masked ([Messages](#messages)).
- **`Guard     unexplained; stops at <h:mm><am|pm>`** prints after the `Leases` line when, and only
  when, the host is running and `<n>` is `0`. **A host is running when its `State.Name`, in the
  `ec2 describe-instances` answer its report read, is `running`**, the state the guard stops
  ([guard](#guard)); a `pending`, `stopping`, `stopped` or other state gets no `Guard` line, nor
  does a host with `Instance  none recorded` (architect, 2026-11-21, for v0.08-T01, S01 batch). The words `unexplained` and
  `stops at` are D60's and the guard's ([guard](#guard)).
- **The stop time is the guard's own** (D56): the later of the instance's `LaunchTime`, from the
  same `ec2 describe-instances` answer, and the project's last lease end, plus the grace, 15
  minutes. A project with no last lease end takes the launch time alone. **What prints is the later
  of that time and the report's time** (architect, 2026-11-21, for v0.08-T01, S01 batch), since the guard stops a host whose
  time has passed at its next run, and a time already past, perhaps by days with the laptop asleep,
  would read as a time still to come. So the printed time is never more than the grace after the
  report's time, and needs no date.
- **`<h:mm><am|pm>` is that time in the laptop's local time, read from the system when the line is
  printed**, never assumed or stored: the zone Windows is set to at that moment, with that date's
  daylight saving rule applied (AGENTS.md, Writing). `<h>` is the hour on a 12-hour clock, `1` to
  `12` with no leading zero, `12` for the hours after midnight and after noon; `<mm>` is the
  minute, two digits; then `am` for a time before noon and `pm` from noon on, lowercase, with no
  space before it. So `12:34pm`, `5:19am`. **A time with seconds is rounded up to the next whole
  minute** (architect, 2026-11-21, for v0.08-T01, S01 batch), so the line never names a minute before the guard can stop
  the host. **`AGENTSPACE_TIMEZONE` does not change it**: that variable is the token push's alone
  ([The test seam](#the-test-seam); architect, 2026-11-21, for v0.08-T01, S01 batch). The guard's own progress lines use the
  same form ([guard](#guard)).
- **When the system gives no local time for that instant**, the time is printed in UTC with ` UTC`
  after it, as `<h:mm><am|pm> UTC`, so it is never read as the laptop's own (architect,
  2026-11-22, for v0.08-T03, on #0177). The guard's lines and `status -All`'s `Guard` line do the
  same, through one formatter.
- **A held `provision.lock` is not shown.** The guard MUST NOT stop a host while its provisioning
  run holds that lock ([guard](#guard)), and `status -All` MUST NOT try or take it, since a try
  could make a run that starts at that moment fail. A host being made under a lease has a `pending`
  lease, which is live, so it gets no `Guard` line; one being made with no lease, by a person's
  `provision`, can show a stop time that its run's lock postpones (architect, 2026-11-21, for v0.08-T01, S01 batch).
- **`LaunchTime` is read only for a host that gets a `Guard` line.** For such a host, a missing
  `LaunchTime`, or one not of its form ([Calls by command](#calls-by-command)), fails that host's
  part before its five lines, with
  `agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime.`,
  and the report goes on with the next host, as for any failed host (architect, 2026-11-21, for v0.08-T01, S01 batch). Any
  other host never reads it, and is not failed by it.
- **`status -All` makes no AWS call that it did not make before v0.08**: the launch time comes
  from the `ec2 describe-instances` answer each host's report already reads.

**When the lease state file is missing, cannot be read, or is not valid** by state.md's checks (no
file at its path, something at its path that is not a file, a file that cannot be opened, its
bytes, its JSON, its schema, two records with one `id`, or a time that names no real instant),
**`status -All` still reports every host, without the `Leases` and `Guard` lines, and says why
once** (architect, 2026-11-21, for v0.08-T01, pin P18). **A missing lease state file is not "no
leases" here either**, as it is not for the guard ([guard](#guard), step 3): `status -All` MUST
NOT print `Leases    0 live` or a `Guard` line for a file it did not read. After the last host's
part, and before the closing count when a host failed, it prints, once,
`agentspace: The lease state file <path> cannot be read: <reason>.`, where `<path>` is the file's
full path and `<reason>` is one sentence fragment naming the first problem found, or the system's
own error text, with no line break, in the form of the host registry's message
([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)), and
`the file does not exist` for a missing file, as the guard's (architect, 2026-11-21, for
v0.08-T01, pin P24). **That line is part of `status`'s report**: it prints on stderr, where the
host lines go, and `-Quiet` does not withhold it (architect, 2026-11-21, for v0.08-T01, pin
P27). **The lease state file never changes the exit code** (architect, 2026-11-21, for
v0.08-T01, pin P25): a missing, unreadable or invalid file only leaves out the `Leases` and
`Guard` lines and adds that one line, and `status -All` exits 0 or 1 as the host lines alone make
it, by [Every host at once](#every-host-at-once--all)'s rule. The hosts' states it reports are
complete and true, so exit 0 is no false success (D36). That line is not a host's failure, and is
not counted in `agentspace: <n> of <m> hosts failed.` **It never prints a count it could not read,
and never guesses one**, and it still shows each host's `State`, which is what a turn's close
reads. #0034's sign-in message still ends the command at once, and this line is not printed after
it. The message's words are the guard's for the same file ([guard](#guard); architect,
2026-11-21, for v0.08-T01, S01 batch).

Each host's five lines print only once that host's calls have succeeded; a host whose call failed
prints its failure line after its `Host` line, and no `Leases` or `Guard` line, and the report goes
on with the next host. **D51's check reads it:** until v0.08 is installed (#0186), a turn's close
is done when every host it used shows `State     stopped`; from then on the guard stops them
([guard](#guard)).

**Help** (architect, 2026-11-21, for v0.08-T01):

```
Usage: agentspace-operator status [-Host <project> | -All] [-Quiet]

Shows a host's instance ID, state, type, root disk size and volume, and
whether Session Manager can reach it; with -All, each host's, after a line
naming it, then its count of live leases and, for a running host that no
live lease explains, the time the guard will stop it. Never starts a host.
Prints to stderr. Without -Host it shows the box.

With -Host, for a host that is not the box, it then signs a fresh one-hour
certificate, so 'ssh agentspace-host.<project>' signs in for the next hour:
run it before you sign in there, and again after an hour. Sign in under a
live lease of the project, created first, or the guard stops the host.

Exit codes: 0 success, 1 failure.
```

### grow

```
operator grow [-Host <project>] [-GiB <n>] [-Quiet]
```

**Grows its host's root volume by `<n>` GiB (default 2), and the root filesystem with it.** Without
`-Host` its host is the box. A new host's root volume starts at the template's 8 GiB, and grows
under the same rule as the box's, each host on its own (D46; AGENTS.md, "Growing the box's disk").
For a host that is not the box, step 3's ssh goes through the host's alias, after the operator
makes its key, certificate and alias for it ([How the operator reaches a
host](#how-the-operator-reaches-a-host)); a failure there fails `grow` with its own message, and the
volume's change in step 2 stands. **When the host is not running, `grow -Host` still writes its
alias and signs a fresh certificate**, after step 2, since every command given `-Host` does (P13
and P15, architect, 2026-11-20, for v0.07-T01).

**`<n>` MUST be 1 or more** (D61, #0164). `grow` given a `-GiB` of 0 or less, `-1` included, fails
before any of the steps below, with `agentspace: -GiB MUST be 1 or more for grow, not '<arg>'.`, a
usage error with exit 1, as every operator usage error is (D61 as corrected; P7, architect,
2026-11-21, for v0.08-T01), found by [`grow`'s check](#command-line) after the host checks. **It
makes no AWS call, starts no program and writes nothing**, the host's key, certificate and alias
included. A value that is not a decimal integer in range is the `-GiB` form's usage error, as for
every command ([Command line](#command-line)).

1. **Reads the root volume's size**, and prints `Growing <vol> from <size> to <new> GiB (EBS allows
   one change per volume every 6 hours)...`, where `<new>` is `<size>` plus `<n>`.
2. **Asks EC2 to change the volume to `<new>` GiB.** A refusal fails with the AWS CLI's error
   ([AWS failures](#aws-failures)), which is what happens for a second change within 6 hours.
3. **When the instance is running**, as step 1 read it, prints `Growing the filesystem online...`,
   waits 10 seconds, and starts one ssh (B-23, A-30):

   ```
   ssh -o ClearAllForwardings=yes agentspace <remote command>
   ```

   with `agentspace` the host's alias, as in [sync's calls](#sync), and
   `<remote command>` one argument, the last, as for [sync's calls](#sync), its stdout and
   stderr the operator's. **On the box, the remote step MUST**, as root and never prompting:
   1. Grow the root disk's partition to the end of the disk. A failure here, which is also what
      a partition with nothing to grow gives, does not stop the next step.
   2. Grow the root filesystem to fill its partition.
   3. Only if step 2 succeeded, print the root filesystem's size and use, in units a person reads.
   4. Exit non-zero if step 2 or step 3 failed, and 0 otherwise.

   On today's box the root disk is the first NVMe device, and the root filesystem is ext4 on its
   first partition. **Today's box has the tools for each step** (architect, 2026-11-19, for v0.06,
   gap FB-13): `growpart` from Ubuntu's `cloud-guest-utils` for step 1, `resize2fs` for step 2 and
   `df` for step 3, each in the Ubuntu cloud image the stack pins, run through `sudo` with no
   password. A remote command for today's box MAY use them, and MUST NOT install anything. **A non-zero exit of the ssh, 255 for an ssh that could not connect included,
   MUST fail `grow` with `agentspace: Growing the filesystem online failed; the volume is already
   <new> GiB, and the filesystem grows on its own at next boot.`** (D36; [Q23](#open-questions),
   closed). The volume's change in step 2 stands. The prototype does not check the ssh's exit
   code, so it exits 0 even when the filesystem did not grow
   ([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
4. **Otherwise** prints `The filesystem grows on its own at next boot.` and does not start the
   box. Both messages rely on the box growing its root filesystem at every boot
   ([The box's files](#the-boxs-files)).

**A volume never shrinks**, and EBS allows one change per volume every 6 hours.

**Stdout:** the remote step's stdout. **Stderr:** its stderr, the messages above, and the nested
ProxyCommand's lines, `The host is ready.` among them
([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)). **Exit codes:** 0 and 1.

**Help** (architect, 2026-11-21, for v0.08-T01):

```
Usage: agentspace-operator grow [-Host <project>] [-GiB <n>] [-Quiet]

Grows a host's root volume by -GiB, a whole number of 1 or more (default 2),
and, if the host is running, its filesystem online; a stopped host grows its
filesystem at next boot. EBS volumes never shrink, and allow one change
every 6 hours. Without -Host it grows the box's.

Exit codes: 0 success, 1 failure.
```

### guard

```
operator guard [-Quiet]
```

**Stops every running host that no live lease explains, once the grace has passed, and sends every
running host its lease horizon** (D56, D57, D63). **The grace is 15 minutes, a constant and not a
parameter** (D56, D63). The words *a project's leases*, *live lease*, *a project's last lease end*
and *a project's horizon* are
[state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon)'s (P1,
P10 and P11, architect, 2026-11-21, for v0.08-T01), and this section uses them as that section
defines them, at the run's time. **The keeper starts the guard**, detached, at its first pass and
then at most once every 300 seconds, as `agentspace-operator guard` with no other argument
([keeper.md, Starting the guard](keeper.md#starting-the-guard); D56, D63); **a person MAY run it
too**, and a run by hand is logged and decided the same way.

**It acts on every host in the host registry, every run, and takes no `-Host` or `-All`** (D63). It
is not `-All`: it prints no `Host` line, and it decides every host from one read of the files and
one `ec2 describe-instances` call, rather than running a command once per host. **The box is
guarded like any host** (D56; Terry, 2026-11-21, closing Q9).

**What it MUST NOT do:**
- **Write the host registry or the lease state file, or take `hosts.lock` or the lease state
  lock** (D56). It reads both files without their locks, as the keeper and the ProxyCommand do,
  and so never waits on the driver, a provisioning run or a lease command, and never makes them
  wait. **A missing instance ID it reads from the host's stack is not written back**
  (architect, 2026-11-21, for v0.08-T01, S01 batch). **A host it stops stays `ready` in the registry** (D45, D56).
- **Sign a certificate, make a key or write an alias for any host**, as `down -All` signs none
  ([Every host at once](#every-host-at-once--all)), so the guard never fails on the CA's trouble.
- **Start an ssh or an scp, or make a folder under `operator\`.** Every program it starts is the
  AWS CLI.
- **Read its stdin, or depend on its streams.** A guard the keeper starts has a hidden console's
  streams, which nothing reads, as a detached provisioning run does
  ([provision](#provision), its log); its log is the only record of it. Nothing it writes to its
  stdout or stderr can fail it.
- **Stop a host whose provisioning run holds its `provision.lock`** (D56). Step 6 leaves such a
  host, logs it `unexplained` with `provisioning=running`, and sends it its horizon. **This covers
  a resize**, which is a provisioning run (D68), and the stop and start CloudFormation makes to
  change a host's type is the stack's, never the guard's. **It covers a person's `deploy` too**,
  which from v0.09 holds the host's `provision.lock` for its life ([deploy](#deploy); architect,
  2026-11-23, for v0.09-T01, pin P34), so a person's resize is never stopped mid-change: the guard
  cannot tell which program holds the lock, and leaves the host with the same line and message as
  for a run.
- **Stop a host whose `provision.lock` it could not try** for any reason but its being held, such
  as access denied or a folder at the lock's path: that is not a free lock (P3's rule for the
  keeper, applied here; architect, 2026-11-21, for v0.08-T01, S01 batch). Step 6 fails the host's
  part, and sends it its horizon.

**What it MUST do, in order**, after the command line's checks and the home and system folder
checks ([Command line](#command-line)):

1. **Takes `guard.lock`, without waiting** (P3, architect, 2026-11-21, for v0.08-T01). The lock is
   `%USERPROFILE%\.agentspace\guard.lock`, beside `hosts.json`; `AGENTSPACE_STATE_DIR` does not
   move it. It is held as `provision.lock` is held: open for reading and writing with no sharing,
   created if it is missing, with `%USERPROFILE%\.agentspace` made first if that is missing, never
   deleted, empty, its handle not inheritable, and held until the run exits. Its rules are
   [state.md, The guard's lock](state.md#the-guards-lock)'s.
   - **A lock another program holds ends the run at once with exit 0**, before anything else:
     nothing is read, no AWS call is made, nothing is logged (D63, P3) and nothing is printed
     (architect, 2026-11-21, for v0.08-T01, S01 batch). The guard that holds it does this run's work.
   - **A lock that cannot be taken for any other reason**, a folder at its path or access denied
     among them, fails the run: it prints `agentspace: Cannot take <path>: <error>.`, logs it
     ([Its log](#its-log)), reads nothing, makes no AWS call and exits 1. `<path>` is the lock's
     full path and `<error>` the host registry library's whole text for that lock's error, with
     control characters made spaces and its last period dropped, as [provision](#provision)'s step
     2 prints its own lock's, such as
     `The guard's lock could not be taken: the lock file could not be opened` (architect, 2026-11-21, for v0.08-T01,
     S01 batch; its `<error>` corrected by the architect, 2026-11-22, for v0.08-T03: neither lock's
     error carries the system's own text).
2. **Reads the host registry**, `%USERPROFILE%\.agentspace\hosts.json`, whole, without its lock
   ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)). A
   missing file is the box's record alone. **A file that cannot be read or is not valid fails the
   run** with that section's message, `agentspace: The host registry <path> cannot be read:
   <reason>.`, logged, and the run stops nothing, sends nothing and exits 1.
3. **Reads the lease state file**, `<state folder>\leases.json`, whole, without its lock
   ([state.md, The lease state file](state.md#the-lease-state-file)), from the state folder
   `AGENTSPACE_STATE_DIR` names, else `%USERPROFILE%\.agentspace`
   ([Environment variables](#environment-variables)). **A file that is missing, cannot be read or
   is not valid, by state.md's checks, two records with one `id` among them, fails the run** with
   `agentspace: The lease state file <path> cannot be read: <reason>.`: the run MUST stop nothing
   and MUST send no horizon; it makes no AWS call, logs one `failed step=leases` line with no
   `host=` field ([Its log](#its-log)), and exits 1
   (D36; architect, 2026-11-21, for v0.08-T01, pin P18). **A missing lease state file is not "no
   leases" for the guard**, though the lease CLI reads it so: a laptop that lost or corrupted
   `leases.json` MUST NOT cost a working agent its host, and a run that cannot see the leases MUST
   NOT treat a host as unexplained. `<path>` is the file's full path. `<reason>` is as for the host
   registry's message: one sentence fragment naming the first problem found, or the system's own
   error text, with no line break (architect, 2026-11-21, for v0.08-T01, S01 batch); for a
   missing file it is `the file does not exist` (architect, 2026-11-21, for v0.08-T01, pin P24).
   - **Then it reads the run's time, once**, the UTC clock to the whole second, and uses it as the
     reading's time for every lease of the run (P1) and for every comparison with the grace
     (architect, 2026-11-21, for v0.08-T01, S01 batch). Each log line carries the time it is written, as `provision.log`'s
     do.
4. **Finds each host's instance ID**, in the registry's order:
   - **A record that holds one:** that ID.
   - **A `ready` record that holds none:** the ID read from its stack's `InstanceId` output, as the
     ProxyCommand reads it (`cloudformation describe-stacks --stack-name <stack>`,
     [Calls by command](#calls-by-command)), with that call's failure messages
     ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)).
     **It is not written into the record** (architect, 2026-11-21, for v0.08-T01, S01 batch). A failure fails that host's
     part ([A host's part that fails](#a-hosts-part-that-fails)), and the run goes on to the next
     host, but for #0034's sign-in message, which ends the run.
   - **A `creating` or `failed` record that holds none is skipped**, with no call and no line, as
     `-All` skips one ([Every host at once](#every-host-at-once--all)): no instance of it is known
     (architect, 2026-11-21, for v0.08-T01, S01 batch).
5. **Asks EC2 for every host's instance in one call**, `ec2 describe-instances` with a filter
   naming every ID step 4 found, in the registry's order ([Calls by command](#calls-by-command)).
   It is not made when step 4 found no ID. **A failure of the call, #0034's sign-in message
   included, fails the run** (D56; P8, architect, 2026-11-21, for v0.08-T01): logged, and the run
   stops nothing, sends nothing and exits 1. An answer that is not JSON fails it the same way, with
   `agentspace: Unexpected answer from ec2 describe-instances: not JSON.`
   - **The call names the IDs in a filter, never in `--instance-ids`** (architect, 2026-11-21, for v0.08-T01, S01 batch),
     because EC2 refuses the whole of an `--instance-ids` call that names one instance it does not
     know. With the filter, an unknown instance is only missing from the answer, and every other
     host is still guarded.
   - **Each host's instance is the element of the answer's `Reservations[].Instances[]`** whose
     `InstanceId` equals its ID ([Calls by command](#calls-by-command), what is read). **A host
     whose instance is not in the answer fails its part** with `Instance <id> was not found.`
     (architect, 2026-11-21, for v0.08-T01, S01 batch). An element with no string `InstanceId` is ignored.
6. **Decides each host, in the registry's order**, by its instance's `State.Name`:
   - **Anything but `running`** (`pending`, `stopping`, `stopped`, `shutting-down`,
     `terminated`): nothing. No line, no stop and no horizon (architect, 2026-11-21, for v0.08-T01, S01 batch). A `pending`
     host is decided at the next run that finds it `running`.
   - **`running`, with its instance's `LaunchTime` missing, or not of its form:** the host's part
     fails with
     `Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].LaunchTime.`
     ([Calls by command](#calls-by-command)), logged with `step=instance`, and the host is neither
     decided, stopped nor sent a horizon. **Every running host needs its `LaunchTime`, explained or
     not**, since step 7 sends the later of the project's horizon and the host's `LaunchTime` (P12,
     architect, 2026-11-21, for v0.08-T01), and an unexplained host's since needs it too.
   - **`running`, and the project holds a live lease: explained.** Nothing is logged, and the host
     is not stopped, whatever it is doing (D56). A `pending` lease is live, so a host being made,
     or waiting on a placement, is explained.
   - **`running`, and the project holds no live lease: unexplained.** Its *since* is the later of
     its instance's `LaunchTime` and the project's last lease end, or `LaunchTime` alone when the
     project has no last lease end (D56). Its *stop time* is its since plus 15 minutes.
     - **Before the stop time**, the run's time earlier than it: it logs the host `unexplained`
       with the stop time, prints `The host of '<project>' (<id>) has no live lease; it stops at
       <h:mm><am|pm>.`, and does not stop it (D56).
     - **At or after the stop time: the host is due.** The guard tries the host's provisioning
       lock, `%USERPROFILE%\.agentspace\hosts\<project>\provision.lock`, as the driver tries it
       ([state.md, The provisioning run's lock](state.md#the-provisioning-runs-lock)): one try
       with no sharing, released at once if it got it. **It never creates the host's folder or the
       lock file to try it: a missing folder or file is a lock nobody holds** (architect,
       2026-11-21, for v0.08-T01, S01 batch).
       - **Held: the host is left** (D56). The guard logs it `unexplained` with the stop time and
         `provisioning=running`, prints `The host of '<project>' (<id>) has no live lease, but a
         provisioning run is working on it; it is not stopped.`, and does not stop it.
       - **Not tried**, for any reason but its being held, such as access denied or a folder at its
         path: not a free lock ([What it MUST NOT do](#guard), above). The host fails its part with
         `Cannot try <path>: <error>.`, and is not stopped. `<error>` is as for `guard.lock`'s
         message in step 1.
       - **Free: the host is stopped**, as `down -Host` stops it but with no certificate or alias
         (D56, D63): one `ec2 stop-instances` for its instance, which asks EC2 to stop it and does
         not wait. Once the request is accepted, it logs the host `stopped`, and prints
         `Stopping the host of '<project>' (<id>): no live lease explains it.` A stop that fails
         fails the host's part with `Could not stop <id>: <error>`, where `<error>` is the AWS
         CLI's error text as [AWS failures](#aws-failures) gives it; #0034's sign-in message ends
         the run instead.
   - `<h:mm><am|pm>` is the stop time in the laptop's zone when it is printed, in the form of
     `status -All`'s `Guard` line ([status](#status)), seconds rounded up to the next whole minute.
     The log's times are UTC ([Its log](#its-log)).
7. **Sends every running host its horizon** (D57, as D63 changed it), in the registry's order: each
   host step 6 read as `running`, but a host the run stopped and a host whose `LaunchTime` failed
   its part. **Every run sends it to every such host, whether or not it changed**, so no file on
   the laptop remembers what was sent, and a host that rebooted gets it back within 5 minutes.
   - **What is sent is the later of the project's horizon and the host's `LaunchTime`** (P12,
     architect, 2026-11-21, for v0.08-T01; D57 as changed). The project's horizon is
     [state.md](state.md#live-leases-and-the-lease-horizon)'s, at the run's time, with the host's
     `LaunchTime` from step 5 as the launch time it takes. So a host started with no live lease is
     never sent a last lease end from before its launch, and its own bound and the guard's agree:
     for an unexplained host, the horizon sent plus the grace is the guard's stop time. A host left
     in step 6 because its provisioning run holds the lock is sent its horizon too.
   - **It is sent with one `ssm send-command` per host**, the document `AWS-RunShellScript`, which
     runs as root, and the comment `agentspace guard: lease horizon` (D63), as `up`'s patch check
     is sent ([What up waits for](#what-up-waits-for)). **The remote command MUST**, as root and
     never prompting:
     1. Make `/run/agentspace` if it is missing, owned by root, mode 0755.
     2. Write the horizon as Unix seconds, a decimal integer with no sign and no leading zero, then
        one line feed and nothing else, to a temporary file in `/run/agentspace`, owned by root,
        mode 0644.
     3. Rename it over `/run/agentspace/lease-horizon`, so a reader finds the old file or the new
        one whole, never a part (D63).
     4. Exit 0 only when all three succeeded.

     **The command sent MUST hold the horizon's decimal digits as one word of its own**, so a check
     can find the value in it (architect, 2026-11-21, for v0.08-T01, S01 batch), and **MUST hold no comma, bracket or equals
     sign**, which the AWS CLI's shorthand syntax for `--parameters` would split on, or MUST be
     passed in a form that is not split, as the patch check's. Its text is the operator's own
     (D34). On today's box `/run` is cleared at every boot, so a host that has just started holds
     no horizon until the next run (D57), and its idle check keeps D11's rule alone until then
     ([The idle check and the lease horizon](#the-idle-check-and-the-lease-horizon)).
   - **The send is the call alone:** it succeeds when `ssm send-command` exits 0, and the guard
     never polls the command's invocation (architect, 2026-11-21, for v0.08-T01, S01 batch). Whether the host ran it is
     shown by the next run's send, 5 minutes later, and by the host's own file. **A send that fails
     fails the host's part** with `Could not send the lease horizon to <id>: <error>`, `<error>`
     as for a stop, and is tried again at the next run (D57's change). A host whose Session Manager
     agent is not online yet, such as one started within the last minute, fails this way, and the
     next run sends it.
   - **A horizon sent is not logged** (D63), and prints nothing.
8. **Ends.** Unless `-Quiet` is given, it prints one closing line,
   `Hosts: <m>; running <r>, stopped <s>, unexplained <u>.`, where `<m>` is the registry's records,
   `<r>` the hosts step 6 read as `running`, `<s>` the hosts this run stopped, and `<u>` the hosts
   it logged `unexplained` (architect, 2026-11-21, for v0.08-T01, S01 batch). **When any host's part failed**, it then
   prints `agentspace: <n> of <m> hosts failed.`, `-All`'s line, where `<n>` is the number of hosts
   whose part failed, and exits 1. Otherwise it exits 0. It releases `guard.lock` as it exits.

#### A host's part that fails

**A host's part that fails** prints its message to stderr as `agentspace: Host '<project>':
<message>`, where `<message>` is the failure's text as the steps above give it, logs one `failed`
line for the host ([Its log](#its-log)), and the run goes on to the next host and to step 7
(architect, 2026-11-21, for v0.08-T01, S01 batch). It stops nothing more for that host this run, and sends it no horizon,
but in step 6's two cases that say otherwise: a provisioning lock not tried, and a stop that
failed, after which the host is still running and is sent its horizon. **#0034's sign-in message
is the exception, as for `-All`**: whichever call meets it, the run prints it, logs it, makes no
further call, and exits 1. A stop already requested stands, and a horizon already sent stays on its
host.

#### What the guard can race

**Which state folder it reads.** **The guard sees the leases of one state folder**: the one
`AGENTSPACE_STATE_DIR` names in its own environment, else `%USERPROFILE%\.agentspace`. A lease
recorded under another state folder explains no host, so a person who keeps real leases under
another state folder MUST NOT expect them to keep a host from the guard. The keeper passes the
guard its own state folder (P4, architect, 2026-11-21, for v0.08-T01), so the guard and the
keeper's holders read the same leases.

**A lease made during a run** (architect, 2026-11-21, for v0.08-T01, S01 batch): a lease created, or a released host's next
lease's first connection, between the guard's read of the lease state file and its stop of that
project's host, is not seen by that run. The host is stopped, and the lease's next connection
starts it again (D56: start-on-connect is unchanged), as for any stopped host.

**A provision started by hand while the guard stops its host is safe** (architect, 2026-11-21, for
v0.08-T01, ruled with P10 and P11). A run that takes `provision.lock` after the guard's try finds
the host `stopping` or `stopped` at its step 5, and its start-and-wait step waits for a `stopping`
host to stop and then starts it ([Starting the box and waiting for
it](#starting-the-box-and-waiting-for-it), step 1; [provision](#provision), step 5); every later
step reaches the started host. A run already past its step 5 holds the lock, so the guard leaves
its host (step 6).

**The guard's own try of `provision.lock`** (step 6) holds the lock for a moment. A provisioning
run that starts in that moment finds it held, logs
`run failed reason=A provisioning run for '<project>' is already running.` and exits 1
([provision](#provision), step 2), and the guard, which found the lock free, stops the host. The
project's next `lease create` or `lease get` starts another run (D48), whose start-and-wait step
starts the host again ([provision](#provision), step 5). **A person's `deploy` that starts in that
moment is refused the same way**, with its own message, before any AWS call ([deploy](#deploy);
architect, 2026-11-23, for v0.09-T01, pin P34), and MAY be run again; its `sync` step starts the
host.

**Work on a host by hand MUST be done under a live lease of its project** (D56): a person's
`up -Host`, `sync`, `bootstrap`, `grow` or `deploy`, and any direct ssh, the sign-in of [README,
Signing in on a new host](../README.md#signing-in-on-a-new-host) included. With no live lease, the
guard stops that host a grace after the later of its launch and its project's last lease end,
whatever is running there. **Only `provision.lock` holds the guard off** (step 6), and only a
provisioning run and, from v0.09, a person's `deploy` hold it, each for its own life ([deploy](#deploy);
architect, 2026-11-23, for v0.09-T01, pin P34); no other command holds it. A `deploy` is
therefore not stopped while it runs, but once it ends, the guard's next run that finds its host
due and unexplained stops it. **So a person's `sync` MUST be run under a lease**, since its patch wait can run
for up to 30 minutes ([The patch wait](#the-patch-wait)), past that stop: a host stopped
mid-wait leaves the wait waiting for Session Manager until its deadline, and `sync` fails.

#### Its log

**`%USERPROFILE%\.agentspace\guard.log`** (D56, D63; P2, architect, 2026-11-21, for v0.08-T01).
`AGENTSPACE_STATE_DIR` does not move it. **One line per host per event**: a host the run stopped,
a host it found unexplained, a host whose part failed, a failed horizon send among them; and one
line, with no `host=` field, for a failure of the run itself. **Nothing for a host that is
explained, a host that is not `running`, or a horizon sent** (D56, D63). The file's bytes, share
mode and access rules are [state.md, The guard's log](state.md#the-guards-log)'s: UTF-8 without a
byte order mark, each line ending in one LF, opened for append sharing read, write and delete,
created at its first line, and never rotated or removed by agentspace (P2).
- **Each line is written by opening the file for append, writing the whole line and closing it**,
  and lands whole at the end of the file even when two processes write at once, as
  [keeper.md, Log lines](keeper.md#log-lines) requires of the keeper's. An open or write that fails
  is tried up to 5 times, 50 ms apart, and a line that still cannot be written is dropped
  (architect, 2026-11-21, for v0.08-T01, S01 batch). **A dropped line changes nothing the run decided or did, and the run
  then exits 1**, printing `agentspace: Cannot write the guard's log <path>: <error>.` once, at the
  first line it drops: a run that cannot report what it found has not succeeded (D36).
- **The lines are exactly these** (P2's form; their text is the architect's, 2026-11-21, for
  v0.08-T01, S01 batch):

  ```
  <time> host=<project> stopped since=<since>
  <time> host=<project> unexplained since=<since> stops=<stop>
  <time> host=<project> unexplained since=<since> stops=<stop> provisioning=running
  <time> host=<project> failed step=<step> reason=<reason>
  <time> failed step=<step> reason=<reason>
  ```

  - **`<time>`** is the time the line is written, in UTC, as `YYYY-MM-DDTHH:MM:SSZ`, truncated to
    the whole second (P2). **`<since>`** and **`<stop>`** are the host's since and stop time
    (step 6), in the same form. A `LaunchTime` with fractions of a second is truncated to the whole
    second before it is compared or written.
  - **`host=<project>`** is the record's `project`, the box's `swimcoach`, and comes right after
    `<time>` (P2). A line not about one host carries none (P2).
  - **`<step>`** on a host's line is one of `instance` (step 4's stack read, or step 5's answer
    for that host: not found, or a member missing, a running instance's `LaunchTime` among
    them), `lock` (the provisioning lock not tried),
    `stop` and `horizon`. On the run's line it is one of `lock` (`guard.lock` not taken), `hosts`,
    `leases`, `describe`, and, for #0034's sign-in message, the step of the call that met it:
    `instance`, `describe`, `stop` or `horizon`.
  - **`reason`** is the last field and runs to the end of the line: the failure's message without
    its `agentspace: ` prefix, and without the `Host '<project>': ` that stderr puts before a
    host's, with each control character in it written as `?`, as `provision.log`'s reason is
    ([provision](#provision), its log). It MUST NOT hold the token or any text of the Bot
    credential, which no failure message holds ([Secrecy](#secrecy)).
  - **A run's lines come in the order of its steps**: a host's `instance` failure in step 4, then
    each host's step 6 line in the registry's order, then each `horizon` failure in step 7's.
    **Step 5's failures for one host**, an instance not in the answer, are written at that host's
    place in step 6's order, as is a member missing from its instance.
- **What a line says about a host is the same decision `status -All` reports** (D60): a host
  logged `unexplained` with `stops=<stop>` is one `status -All` would show as
  `Guard     unexplained; stops at <h:mm><am|pm>` at the same time, with `<stop>` in the laptop's
  zone.

#### Its streams, exit codes and help

**`-Quiet`** (architect, 2026-11-21, for v0.08-T01, S01 batch) suppresses the guard's progress lines: the three of step 6
and the closing line of step 8. As for every command ([Messages](#messages)), its failures always
print, and so does `-All`'s count line. **The log is the same with `-Quiet` or without it**, and so
is everything the run decides and does. The keeper starts the guard without `-Quiet` (D63); its
streams reach nobody.

**An expired AWS sign-in** (#0034): whichever call meets it, step 4's, step 5's, a stop or a send,
the run prints `agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.`,
logs `<time> failed step=<step> reason=AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.`
with no `host=` field, makes no further call, and exits 1 (D56: "logs that and stops nothing"; P8,
architect, 2026-11-21, for v0.08-T01). Met at step 4 or 5, which is where an expired sign-in is met
in practice, it stops nothing and sends nothing. Met later, a stop already requested stands. The
keeper starts the next guard 5 minutes later, which meets it again until a person signs in, so the
log gains one such line per run while the sign-in has expired, and nothing reaches Terry's phone
(D60; architect, 2026-11-21, for v0.08-T01, S01 batch).

**Stdout:** nothing, ever. **Stderr:** the progress lines above, unless `-Quiet`; every failure's
message; and nothing of the AWS CLI's but its error text inside a failure's message. No program's
output is passed through: the guard reads every answer it gets. **Exit codes** (D36; P8,
architect, 2026-11-21, for v0.08-T01, for a failed AWS call; P18, architect, 2026-11-21, for
v0.08-T01, for the lease state file; the rest architect, 2026-11-21, for v0.08-T01, S01 batch):

| Exit | When |
|---|---|
| 0 | **The run did all it decided to do:** every due host whose lock was free was asked to stop, every running host was sent its horizon, every line was logged, and nothing failed. A run that finds every host explained, or no host running, exits 0. **So does a run that finds `guard.lock` held** (D63, P3), which reads, decides and logs nothing because the guard holding the lock does this run's work. A run that finds no lease state file never exits 0 (P18). |
| 1 | **Anything failed:** a usage error; `guard.lock` not taken for a reason other than its being held; the host registry not read; the lease state file missing, not read or not valid, a duplicate `id` included (P18), with nothing stopped and no horizon sent; the `describe-instances` call; any host's part (its instance ID, its instance, its provisioning lock, its stop or its horizon's send); a log line dropped; #0034's sign-in message. A host's part that failed never turns another host's stop back, and a run that exits 1 has still done every stop and every send it could. |

**Its environment** (P4, architect, 2026-11-21, for v0.08-T01). The guard reads, of the variables
[Environment variables](#environment-variables) lists:
- **`USERPROFILE` and `SystemRoot`**, checked as every command checks them: the home holds
  `hosts.json`, `guard.lock`, `guard.log` and each host's `provision.lock`.
- **`AGENTSPACE_STATE_DIR`**, for the lease state file alone, as the ProxyCommand reads it.
- **`AGENTSPACE_AWS_CMD`**, the test seam's AWS CLI ([The test seam](#the-test-seam)), and
  **`PATH`**, for the AWS CLI's fallback.
- **`*_POSIX_FD_STATE`** is removed, and **`AGENTSPACE_NESTED`** set, for the AWS CLI it starts, as
  for every program the operator starts.

**It reads no other.** `AGENTSPACE_SSH_CMD`, `AGENTSPACE_SCP_CMD` and `AGENTSPACE_TIMEZONE` name
programs and a zone it never uses; `AGENTSPACE_SSH_DIR`, `AGENTSPACE_PROXY_COMMAND` and
`AGENTSPACE_DIAGNOSTIC` change nothing it does. **Its progress lines' `<h:mm><am|pm>` uses the
laptop's zone as the system gives it**, never `AGENTSPACE_TIMEZONE`, which is the push's alone
(architect, 2026-11-21, for v0.08-T01, S01 batch). **A guard the keeper starts gets the environment the lease CLI gives a
provisioning run, with every `AGENTSPACE_*_CMD` the keeper itself was given** (P4;
[keeper.md, The guard's environment](keeper.md#the-guards-environment)), so a conformance home
reaches it, and a keeper started with `AGENTSPACE_AWS_CMD` set starts guards against that
stand-in ([Environment variables](#environment-variables)).

**Its seams for conformance** (D34): the throwaway home of [The test seam](#the-test-seam), with
`hosts.json` written by the case; the lease state file written into the case's state folder, with
times set relative to the real clock, since the guard has no clock seam (architect, 2026-11-21, for v0.08-T01, S01 batch);
`guard.lock` and each `provision.lock` held by the case where it needs them held; and a stand-in
AWS CLI through `AGENTSPACE_AWS_CMD`, which answers `cloudformation describe-stacks`,
`ec2 describe-instances` with each instance's `InstanceId`, `State.Name` and `LaunchTime`,
`ec2 stop-instances` and `ssm send-command`, records each call's arguments, and exits non-zero, or
prints #0034's sign-in text, where a case needs a failure.

**Help** (architect, 2026-11-21, for v0.08-T01):

```
Usage: agentspace-operator guard [-Quiet]

Stops every running host that no live lease explains, once 15 minutes have
passed since the later of its start and its project's last lease end, and
sends every running host its lease horizon, the end of its project's leases.
Leaves a host whose provisioning run is working on it. Acts on every host in
~/.agentspace/hosts.json, and takes no -Host or -All. Logs each stop, each
unexplained host and each failure to ~/.agentspace/guard.log.

The keeper runs it every 5 minutes; you can also run it yourself. A guard
that finds another one running exits at once.

Exit codes: 0 success, or another guard is running; 1 failure.
```

## Starting the box and waiting for it

**`up` and `sync` share one start-and-wait step**, and every nested `up` runs it too, as do
`bootstrap` and `provision`. **It acts on the command's host**, with that host's instance ID, and
its rules and limits bind every host alike (D49). Place never runs it: a lease is never placed
while a host is made, and the provisioning run does the waiting (D48).

1. **Reads the instance's state.**
   - **`running` or `pending`:** nothing to start.
   - **`stopped`:** prints `Starting <id>...` and asks EC2 to start it.
   - **`stopping`:** prints `Waiting for <id> to finish stopping...`, waits with the AWS CLI's
     `ec2 wait instance-stopped` (its own default limit, about 10 minutes), then prints `Starting
     <id>...` and asks EC2 to start it. **The wait's exit code is not checked** (B-34): the start
     request that follows is the check. A wait that gave up leaves an instance EC2 will not start,
     so the start request fails, under [AWS failures](#aws-failures), with its own error text or
     #0034's message. What the wait itself writes to stderr passes through in a person's `up`,
     and is withheld in the ProxyCommand ([What a caller sees](#what-a-caller-sees-d1-card-0087)).
     **How the rewrite does it** (architect, 2026-11-19, for E0.06-T03-S01): `agentspace-run`'s
     runner withholds an `Output::Ignore` call's stderr by default, and takes an opt-in, when it
     is built, that sends that stderr to the operator's own stderr; the operator's program opts in
     for a person's command, and the ProxyCommand does not. Its stdout is never passed, so `up`
     still writes nothing to stdout. The pinned `Output` is unchanged.
   - **Any other state** fails with `agentspace: Instance <id> is <state>.`
2. **Decides whether to wait.** It waits unless the state read in step 1 was `running` and the
   instance's launch time, read again now, is 20 minutes ago or more. **The second
   `ec2 describe-instances`, for the launch time, MUST run only when step 1 read `running`**; a box
   that step 1 found in any other state waits without it (architect, 2026-11-19, for v0.06, gap
   FB-09). EC2 resets the launch time at
   every start, and the patch unit's own time limit is 20 minutes, so such a box has finished its
   boot-time patch run, or failed it, long ago.
3. **Waits**, for at most 15 minutes from the start of the wait, checking every 5 seconds:
   - **Is Session Manager's agent `Online`?** If not, the message is `Waiting for Session
     Manager...`.
   - **If it is, asks the box whether its patch run has finished**
     ([What up waits for](#what-up-waits-for)). An answer other than `pending` ends the wait,
     `failed` included, with no message of its own ([What up waits for](#what-up-waits-for) says
     why). A `pending` answer gives the message `Patching Ubuntu (dist-upgrade, reboot if the
     kernel changed)...`, which is today's box's wording.
   - **A message is printed only when it differs from the last one printed**, so a long wait shows
     each phase once.
   - **At the deadline** it fails with `agentspace: <id> was not ready within 15 minutes; check:
     ssh agentspace journalctl -u agentspace-patch`, whose command is today's box's. For a host
     that is not the box, `agentspace` in it is the host's alias, `agentspace-host.<project>`
     (architect, 2026-11-20, for v0.07-T01).
4. **Prints `Ready: <id>`.**

**Run by the ProxyCommand, these steps print the caller's lines in place of every message above**
([What a caller sees](#what-a-caller-sees-d1-card-0087)).

**`up` MUST NOT print `Ready` before the box's patch run has finished**, because the box MUST be
fully patched at every start before it is used ([What up waits for](#what-up-waits-for)), within
four exceptions: a box running for 20 minutes is not asked, an answer of `unknown` counts as
finished, so does an empty or unexpected answer from a failed check
([What up waits for](#what-up-waits-for); [Q6](#open-questions)), and so does `failed`, a patch
run that ended this boot without the marker (P21; architect, 2026-11-21, for v0.08-T01, pin
P24). This is the one
statement of them. **`sync`'s patch wait has none of them** ([The patch wait](#the-patch-wait)), so a host that
`sync` has finished with has had this boot's patch run, finished or failed and reported, whatever
its launch time (D61's change, #0163).

## The ProxyCommand

**The ProxyCommand is what makes `ssh agentspace`, `ssh <lease alias>` and
`ssh agentspace-host.<project>` start a host and reach it with no inbound port.** ssh runs it in
place of a TCP connection, and talks SSH over its stdin and stdout. D9 makes this path the EC2
driver's. **Which host it reaches is decided by its argument**, as [From the
ProxyCommand](#from-the-proxycommand) says (D49): no argument is the box, a host alias its host,
and a lease alias its lease's `ready` host, or the box.

**How ssh runs it.** The `ProxyCommand` line of an alias that reaches the box is the line
[state.md, The alias file](state.md#the-alias-file) gives: the ProxyCommand program's full path,
through `cmd.exe` when the program is a `.cmd` or `.bat` file, then the alias. Which program that
is, and where it lives, is [Programs and install layout](#programs-and-install-layout)'s. The user's own
`agentspace` entry passes no alias ([The `agentspace` SSH entry](#the-agentspace-ssh-entry)), and a
lease alias, or a host's operator alias, passes its own name, written out literally, never as
ssh's `%n` token. The Claude desktop app runs the same line through its own proxy program.

**Its arguments are the ones the program receives.** A ProxyCommand program that is a `.cmd` or
`.bat` file receives them as `cmd.exe` splits a batch file's parameters: outside double quotes,
`;`, `,`, `=`, a space and a tab each separate two arguments, so `a;b` arrives as two. A native
program receives them as the C runtime splits its command line, so `a;b` arrives as one (B-28).
Either way an alias that holds such a character fails before any tunnel, with exit 1 and nothing
on stdout: as a second argument in step 1, or at the alias check in step 3. Every alias the lease
CLI writes holds none ([state.md, The alias file](state.md#the-alias-file)).

**What it does, in order:**
1. **Refuses a second argument** before anything starts: it prints `agentspace: <program>
   takes at most one argument, the SSH host alias.` to stderr and exits 1, where `<program>` is the
   ProxyCommand program's file name, `agentspace-ssh-proxy.exe`
   ([The programs](#the-programs); the prototype prints `ssh-proxy.cmd`). An empty first argument
   (`""`) is an argument, and reaches step 3.
2. **Puts the Session Manager plugin's folder and the AWS CLI's folder first on `PATH`**, in that
   order: `C:\Program Files\Amazon\SessionManagerPlugin\bin;C:\Program Files\Amazon\AWSCLIV2;`
   then `PATH` as it was (architect, 2026-11-19, for v0.06, gap FB-10). A check MAY still accept
   either order, since no program it runs can tell them apart.
3. **Runs `up`'s steps in caller mode** ([What a caller sees](#what-a-caller-sees-d1-card-0087)),
   with `-Alias <alias>` when it was given an alias, and with anything they write to stdout sent to
   stderr. They check the alias exactly ([Command line](#command-line)), so a bad one fails there
   before any ssh or tunnel, with the `-Alias` message, and before the lease state file or the
   host registry is read. **Then they find the host from the alias** ([From the
   ProxyCommand](#from-the-proxycommand)), and act on that host: its instance ID, its start and
   wait, and its push stamp (D49, D50).
4. **If they failed, exits 1**, starting no tunnel.
5. **Takes that host's instance ID from its record in the host registry**
   ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)),
   which step 3 has just made sure holds one; **when the record holds none because step 3's write
   of it failed, it uses the ID step 3 read from the stack** ([state.md, The
   instance ID](state.md#the-instance-id): a failed write never fails the connection; architect,
   2026-11-21, for v0.07-T03, on #0153's review). A registry that cannot be read here, or a host
   with no ID from either, fails with the caller's failure message, exit 1, and starts no tunnel.
6. **Runs the tunnel**, with the ProxyCommand's own stdin and stdout:

   ```
   "C:\Program Files\Amazon\AWSCLIV2\aws.exe" ssm start-session --region us-west-2 --target <id> --document-name AWS-StartSSHSession --parameters portNumber=22
   ```

   and exits with the tunnel's exit code. When `AGENTSPACE_AWS_CMD` is set, the program it names is
   run in place of `aws.exe` ([The test seam](#the-test-seam)). **The tunnel's AWS CLI is that
   fixed path alone: unlike every other AWS call, it does not fall back to `aws` on `PATH`**
   (B-33), as in the prototype. When the file is missing, the ProxyCommand fails with the caller's
   failure message and exit 1; the prototype prints its shell's own error, which names the path,
   and exits with that shell's code. The tunnel's stderr is the caller's only as
   [What a caller sees](#what-a-caller-sees-d1-card-0087) allows.

**Exit codes:** 1 for a second argument, a failure of step 3 or 5, or a tunnel that cannot be
started; otherwise the tunnel's. ssh ends the
ProxyCommand when the connection closes, so a caller of ssh rarely sees it: ssh exits 255 with
`Connection closed by UNKNOWN port 65535` when the ProxyCommand fails before the tunnel.

**Why every message goes to stderr.** The ProxyCommand's stdout is the SSH connection: ssh reads
every byte of it as the server's side of the protocol. A single byte of a message there corrupts
the connection, so **nothing but the tunnel MUST write to the ProxyCommand's stdout**. stderr goes
to ssh's stderr, which a person at a terminal sees. The Claude desktop app's Test connection does
not show it, so a push from the app MUST be confirmed on the box or by the stamp, never from the
app's output.

**What it MUST NOT do:**
- **Name the backend to the caller** (D1; [What a caller sees](#what-a-caller-sees-d1-card-0087)).
- **Read its stdin before the tunnel starts.** Those bytes are the SSH client's, for the server.
- **Leave the console broken.** When the ProxyCommand's stderr is a Windows console, the
  console's output mode MUST, when the ProxyCommand's own work ends and before the tunnel starts,
  have processed output (`ENABLE_PROCESSED_OUTPUT`) and wrap at end of line
  (`ENABLE_WRAP_AT_EOL_OUTPUT`) on, and `DISABLE_NEWLINE_AUTO_RETURN` off. ssh saves that mode
  and builds the session's raw mode on it, so otherwise the whole session prints line breaks as
  glyphs and ignores `clear` and `reset`. Source: Windows PowerShell 5.1, started with stdin a
  pipe and stderr the console, turns those bits off as it starts, and the prototype turns them
  back on as it starts, before each message and as it ends. Whether the rewrite's runtime needs
  the repair is [Q17](#open-questions).
- **Start the tunnel for an instance other than the one its host's record holds**, the host step 3
  found and made ready.
- **Write the lease state file, or take its lock** (D49). It reads the file as the keeper does.

**The threat model.** The alias argument comes from a ProxyCommand line the lease CLI wrote
([lease-api.md](lease-api.md#fields): `ssh.host_alias`;
[state.md, The alias file](state.md#the-alias-file)), or the operator wrote for a host
([How the operator reaches a host](#how-the-operator-reaches-a-host)), and the user's own entry
passes none. Whoever can write a ProxyCommand line already runs commands, since the whole line is
run as a command (and parsed by `cmd.exe` when it starts with it) before the proxy sees its
argument, so the proxy does not defend against a hostile line. **`up`'s exact check
([Command line](#command-line)) MUST be the gate before any ssh**, and before any file is read
for the alias. **The lease state file and the host registry are new inputs from v0.07**, and both
are the user's own laptop state, which only agentspace writes: whoever can change them can already
change an alias file. The most a changed file can do is send a connection to another of the user's
own hosts, and every lease alias checks its host's key strictly, so ssh then refuses it (P7). A
project read from either file reaches a path (`hosts\<project>\`) only after it matches the
normalized project form, which the files' schemas also require.

### What a caller sees (D1, card #0087)

**Card #0087 (D1 at connect time) is carried in here as a requirement.** What the ProxyCommand
writes to stderr reaches the caller's ssh client, and so the caller, whose lease MUST NOT reveal its
backend ([D1](decisions.md#d1-clients-never-know-the-backend)). The operator's own commands are not
such a client (the intro of this page); this section binds
the ProxyCommand alone, and everything it runs, its `up` steps, their token push and the tunnel
included. That includes the nested ProxyCommand of every ssh and scp the operator starts
([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)), so a person's `sync` shows
the caller's lines for each one.

**Caller mode.** The ProxyCommand runs `up`'s steps ([up](#up)) in caller mode: the same steps,
the same exit codes and the same rules, with every message replaced as the two tables below say.
- **How the ProxyCommand puts its `up` into caller mode is the rewrite's own, and not part of the
  contract.** It MAY run `up`'s steps in its own process, or start `up` with a private parameter or
  environment variable. Whatever it uses MUST NOT appear in any help text, and MUST NOT change what
  a person's `operator up`, started by a person, prints.
- **A check reaches caller mode only through the ProxyCommand**, as
  `conformance/operator/test-unmet-proxy-connect-time.ps1` does.
- Note on where this came from: the prototype has no caller mode. Its ProxyCommand runs the same
  `up` a person does, which prints a person's lines, `Ready: <id>` included, nested or not.

**The caller's progress lines**, each printed to stderr at most once per run of the ProxyCommand,
in this order, and only when the person's line it replaces would have printed; the push's ssh's
stderr, the row after the push's `Sending` line, is passed on line by line as its row says:

| A person's `up` prints | The ProxyCommand prints instead |
|---|---|
| `Starting <id>...`, or `Waiting for <id> to finish stopping...` then `Starting <id>...` | `Starting the host...` |
| `Waiting for Session Manager...` or `Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...`, at the first of them | `Waiting for the host to be ready...` |
| `Ready: <id>` | `The host is ready.` |
| The push's `Sending the localswim Bot token to the instance...` | Nothing |
| The push's ssh's stderr: its nested ProxyCommand's lines, `The host is ready.` among them, and ssh's own messages | Every line as it arrives, but a line that is exactly `The host is ready.`, which is dropped; with `AGENTSPACE_DIAGNOSTIC` set, every line ([What the laptop sends](#what-the-laptop-sends), step 2; #0167, P14, architect, 2026-11-21, for v0.08-T01) |
| A push warning ([What the laptop checks](#what-the-laptop-checks), [What the laptop reads](#what-the-laptop-reads)) | The same warning, with `the instance` written `the host` and `The instance` written `The host`. A warning names no backend otherwise |

**The caller's failure messages.** Each is one line on stderr, the last the ProxyCommand prints
unless `AGENTSPACE_DIAGNOSTIC` adds detail after it, and the ProxyCommand then exits 1, starting no
tunnel:

| A person's `up` fails with | The ProxyCommand prints instead |
|---|---|
| #0034's sign-in message ([AWS failures](#aws-failures)) | The same message, exactly: the one exception (A-14; [Q29](#open-questions)) |
| A usage error or an `-Alias` refusal ([Command line](#command-line)) | The same message: none names the backend |
| `agentspace: Instance <id> is <state>.` | `agentspace: The host is not available, and could not be started.` |
| `agentspace: <id> was not ready within 15 minutes; check: ...` | `agentspace: The host was not ready within 15 minutes; try again later.` |
| Any other failure: an AWS call's error text, a host registry that cannot be read or written, a host alias whose project has no record, a stack with no `InstanceId` output, a missing AWS CLI, and any failure this page gives no text for | `agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.` |

**The tunnel.** Its stdout is the connection. **Its stderr MUST be withheld from the caller unless
`AGENTSPACE_DIAGNOSTIC` is set**, since what the AWS CLI and Session Manager's plugin write there
is backend text. **A tunnel that exits non-zero** gets `agentspace: The connection to the host
failed. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.` on stderr, and the
ProxyCommand then exits with the tunnel's code. A tunnel that cannot be started at all gets the
"Could not reach the host" message, and exit 1.

- **Backend text is** an instance, volume or host ID; a region name; a machine type; a cloud
  instance's state name (`pending`, `running`, `stopping`, `stopped`, `shutting-down` or
  `terminated`) given as that state; the words EC2, AWS, SSM, Session Manager and CloudFormation;
  a host's stack's name, `agentbox` or `agentspace-host-<project>`; the host's operating system or
  patch tool; and any text of the AWS CLI, of
  Session Manager's plugin or of a runtime's error record.
- **On success the ProxyCommand MUST NOT print backend text.** It prints the caller's progress
  lines above, the push's warnings in their caller form, and nothing else but what
  `AGENTSPACE_DIAGNOSTIC` allows, below, **with one exception: the push's ssh's stderr lines other
  than `The host is ready.` pass to its stderr**, such as ssh's own `Warning: Permanently added
  ...` line, as [What the laptop sends](#what-the-laptop-sends) says (P14, architect, 2026-11-21,
  for v0.08-T01). It prints nothing to stdout but the tunnel ([above](#the-proxycommand)).
- **On failure the ProxyCommand MUST print agentspace's own message and no backend text:** the
  caller's failure messages above, each one line that starts `agentspace: `
  ([Exit codes](#exit-codes)), and exits as [above](#the-proxycommand). **The one exception is
  #0034's sign-in message**, exactly as [AWS failures](#aws-failures) gives it, which names AWS. It
  stays in contract v0 (A-14), because v0's caller runs on the laptop (D8), where the person who
  can act on it, the laptop's owner, already knows the backend; whether a later caller sees it is
  [Q29](#open-questions).
- **The backend's detail MAY be printed only when the caller asks for it:** when the
  environment variable `AGENTSPACE_DIAGNOSTIC` is set to a non-empty value, read as the
  ProxyCommand starts, the ProxyCommand MAY print backend text to stderr, such as the failed
  call's error text after agentspace's own message, a person's progress lines, or the tunnel's
  stderr. Unset or empty, it MUST NOT. The variable
  changes nothing else: not the exit code, not stdout, not what a person's own `up` prints. It
  carries the same secrecy rule as everything else ([Secrecy](#secrecy)): detail MUST NOT include
  the token or any text of the credential file.
- **Source:** the prototype's `up` prints the instance ID, EC2 states and Session Manager and
  patch messages into the ProxyCommand's stderr, and on a failure the AWS CLI's text, inside a
  runtime's error record under Windows PowerShell 5.1, where #0034's sign-in message never prints;
  its tunnel's stderr reaches the caller as written
  ([Requirements the prototype does not meet
  yet](#requirements-the-prototype-does-not-meet-yet)).

## The token push

**The push puts the laptop's localswim service URL and its current Bot token on the box, where the
box's `localswim` wrapper reads them, and sets the box's time zone to the laptop's** (D9, D12,
D32). localswim makes a new token at every cold start, so the push runs on every connection and
sends only when something has changed.

### When it runs

- **`up` pushes over its `-Alias`** (default the host's own alias: `agentspace` for the box,
  `agentspace-host.<project>` for another host), after it prints `Ready: <id>` (`The
  host is ready.` in caller mode, [What a caller sees](#what-a-caller-sees-d1-card-0087)), unless
  it is a nested `up` ([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)). It
  sends only when the push is due ([The stamp](#the-stamp)).
- **`sync` pushes over its host's alias (`agentspace` for the box), forced**, as its last step:
  always, whatever the stamp says. `AGENTSPACE_NESTED` does not stop it.
- **Each push goes to one host, and reads and writes that host's own stamp** (D50): the host its
  `up` or `sync` acts on, which in the ProxyCommand is the host its alias reaches ([From the
  ProxyCommand](#from-the-proxycommand)). So a connection to one host never pushes because of
  another's stamp, and each host is pushed once per change. **The time zone goes with it to each
  host** (D50, D32).
- **`up` over a lease alias pushes with the lease's own key and certificate**, which that alias's
  entry in the SSH client configuration supplies. The push passes no identity option of its own,
  and never goes through the user's `agentspace` entry (D9, card #0057).
- **Two `up`s at the same time may both push.** Each writes the same token, so nothing breaks, but
  each push is a login. Nothing serializes them ([Q20](#open-questions)).

### What the laptop reads

**Every time it considers a push, due or not, the laptop reads these, in this order**, because the
stamp covers the token and so cannot be computed without it:

1. **localswim's service receipt**, and from it the service's base URL and the folder of its
   credentials ([The receipt](#the-receipt)).
2. **The Bot credential, `bot.json` in that folder, and from it the Bot token**
   ([The Bot credential](#the-bot-credential)).
3. **The laptop's time zone**, as an IANA name ([The time zone](#the-time-zone)).

**Both files are localswim's, and agentspace never writes, moves or deletes them**
([docs/state.md](state.md#files-agentspace-reads-and-never-writes), which owns that rule).

**Their formats are localswim's, and this page MUST NOT restate them, but as the checks the push
makes in its own code, which follow.** localswim specifies both
in its repository (`localswim/localswim`, the sibling checkout `C:\Projects\localswim`), as of
commit `8d9d583` on its `main` (localswim PR #13, merged 2026-11-20):
- **`docs/windows-operations.md`, section "The service receipt and credential files"**, with its
  subsections "The receipt", "The credential folder" and "The credential files". It states those
  formats as an interface that a program running as the same Windows user MAY rely on, and names
  agentspace's operator as one of its readers.
- **`docs/schemas/service-receipt.schema.json`**, the receipt's JSON Schema (2020-12).
- **`docs/schemas/launch-credential.schema.json`**, the credential's JSON Schema (2020-12), for
  `human.json` and `bot.json` alike.

**What this page states is agentspace's own behavior:** which members the push uses, every check it
makes beyond localswim's schemas, and what each failure does. **The push MUST NOT validate either
file through localswim's schemas.** Their anchored patterns end in a `(?![\s\S])` lookahead, which
Rust's `regex` crate does not support, so every rule the push relies on is checked in its own code,
as the two sections below state it. A rule here that localswim's section does not require, such as
the receipt's acceptance of UTF-16, is agentspace's own. The token's own secrecy rules are
[Secrecy](#secrecy)'s, and apply from the moment the token is decrypted.

**Both files MUST be read afresh each time a push is considered, and the operator MUST NOT keep
anything read from them for a later push.** localswim makes new tokens in a new credential folder
at every cold start (`docs/windows-operations.md`, "The credential folder"), so a folder or a token
remembered from an earlier read can be stale.

#### The receipt

- **It is the file `%LOCALAPPDATA%\localswim-v2\service.json`**, where `%LOCALAPPDATA%` is the
  value of the `LOCALAPPDATA` environment variable when the operator starts. localswim names this
  location (`docs/windows-operations.md`, "The receipt"), and its format is localswim's there and
  in `docs/schemas/service-receipt.schema.json` ([What the laptop reads](#what-the-laptop-reads)).
  The rules below are what the push itself checks, in its own code.
  - **The operator MUST NOT look for the receipt anywhere else.** In particular it never reads
    `runtime\service.json` in the localswim checkout, where the receipt lived until 2026-11-15, and
    which other readers of the receipt still try. So `LOCALAPPDATA` alone decides which receipt,
    and so which token, the push reads ([Secrecy](#secrecy)).
  - **`LOCALAPPDATA` unset, empty or only whitespace means there is no receipt**, and the push is
    skipped as when the receipt is missing. It MUST NOT fail any command.
  - **A relative value is resolved against the operator's working folder.**
  - **The path, and every path below, is taken literally:** no character in it is a wildcard or a
    pattern.
- **When nothing exists at that path, neither a file nor a folder, the push is skipped without a
  message**: localswim is not set up on this laptop ([Q19](#open-questions)). **Anything else that
  cannot be read as a file**, such as a folder at that path, a file the user may not read, or one
  another process holds without sharing read access, gives the "Could not read" warning below.
- **The push MUST NOT retry a read that failed**, though localswim's "The receipt" says a reader
  SHOULD retry a sharing violation after a short pause. This is agentspace's own rule: a push that
  could not read writes no stamp, so the next command that pushes tries again, and a retry would
  only lengthen the connection that is waiting on the push.
- **Its encoding is chosen by a byte order mark:** the bytes `EF BB BF` mean UTF-8, `FF FE`
  UTF-16LE and `FE FF` UTF-16BE, and the mark is not part of the text. **A file with no mark is
  read as UTF-8.** Bytes that are not valid in the encoding are refused, rather than replaced, since
  a replaced byte would change the URL or the path (D20).
- **Its text MUST be one JSON value (RFC 8259) that is an object.** Whitespace is accepted wherever
  JSON allows it (D20). An empty file, text that is not JSON, and any other JSON value, such as an
  array or a string, are refused. Text that is not JSON includes a trailing comma, a comment, a
  single-quoted string and an unquoted member name: D20's tolerance covers JSON's own forms, not
  other syntaxes.
- **agentspace uses two of its members, matched by name in any ASCII case** (D20), as the prototype
  matches them:

  | Member | JSON type | What it is | What the push does with it |
  |---|---|---|---|
  | `base` | String, in the form below | The service's base URL, such as `http://127.0.0.1:8811` | Sends its value, after JSON unescaping, as `LOCALSWIM_URL`, with nothing added or removed |
  | `credential_directory` | String, not empty | The folder that holds the service's launch credentials, `human.json` and `bot.json` | Reads `bot.json` in it ([The Bot credential](#the-bot-credential)) |

  - **Every other member is ignored**, whatever it holds, as localswim's section requires of a
    reader. The receipt carries others (`executable` and `data_directory`, and any localswim adds
    later), and agentspace relies on none of them.
  - **`base`, after JSON unescaping, MUST be exactly `http://127.0.0.1:PORT`**, where `PORT` is
    a port in decimal, 1 to 65535, with no leading zero, and nothing follows it, not even a
    trailing slash. That is the form localswim's section pins (`docs/windows-operations.md`, "The
    receipt"), and the push checks it in its own code, after the receipt's other checks, never
    through localswim's schema ([What the laptop reads](#what-the-laptop-reads); the architect's
    ruling for v0.07-T02, 2026-11-20). **A `base` in any other form is refused** with the "Could
    not read" warning below, and its value is never printed: `localhost`, `https`, a port of 0 or
    above 65535, a leading zero, a trailing slash, a path, a space and a `$` among them. This
    closes the URL's half of [Q9](#open-questions).
  - **The push does not check `credential_directory` against the schema's pattern**: it does not
    require an absolute path.
  - **Two names that differ only in ASCII case are the same member**, so `base` and `BASE` in one
    receipt are one member given twice.
- **Refused, with the "Could not read" warning:** `credential_directory` missing, not a string, or
  empty; `base` present but not a string (`null` included), or a string not in the form above,
  the empty string included; or either member given more than once, in any case, which could mean
  two different things (D20). A `base` that is missing is refused below, with the token's own
  checks.

#### The Bot credential

- **It is the file `bot.json` in the folder that `credential_directory` names**: that path, then a
  `\` unless it already ends in `\` or `/`, then `bot.json`. A `/` in the path is a separator, as
  `\` is. A relative path is resolved against the operator's working folder. The file's name is
  localswim's (`docs/windows-operations.md`, "The credential files"). agentspace relies only on the
  receipt's member and that name, never on the folder's own name or location, which localswim
  says are not part of its interface ("The credential folder"). **The push reads no other file in
  the folder**, `human.json` included.
- **A `bot.json` that is missing, is a folder, or cannot be read is refused.** **The push MUST NOT
  fall back to any other file or format**: the credential in localswim's earlier XML format, which
  localswim stopped writing with its PR #13, is never read.
- **Its format is localswim's** (`docs/windows-operations.md`, "The credential files", and
  `docs/schemas/launch-credential.schema.json`, at `8d9d583`; [What the laptop
  reads](#what-the-laptop-reads)): one JSON object with the members `format`, `format_version`,
  `role` and `token_dpapi_hex`. **The push uses one of them, `token_dpapi_hex`**, and checks the
  other three only to know that the file is the format it reads.
- **The push checks these rules in its own code, and refuses the file when any of them fails.**
  They are localswim's rules for a reader, among them the ones its schema cannot check, with every
  SHOULD there made a MUST here:
  1. **The bytes are UTF-8.** A UTF-8 byte order mark (`EF BB BF`) MUST be accepted, and is not
     part of the text, although localswim writes none. Bytes that are not valid UTF-8 are refused,
     rather than replaced, and so is a file in UTF-16, whatever its mark.
  2. **The text is one JSON value (RFC 8259) that is an object.** Whitespace is accepted wherever
     JSON allows it, and member order carries no meaning. An empty file, text that is not JSON, and
     any other JSON value, such as an array or a string, are refused.
  3. **It has exactly the four members above, each once, and no others.** Member names are compared
     exactly. A missing member is refused, and so is any other member. **A name given twice is
     refused**, and the push's reader MUST detect it itself, since a JSON parser that keeps the
     last value, as `serde_json` does, hides it; localswim's section asks this of a reader only
     when its parser reports duplicates. **A name that differs from another's only in ASCII case
     is refused**, such as `format` beside `Format`.
  4. **`format` is the JSON string `localswim-credential`**, compared exactly.
  5. **`format_version` is the JSON integer `1`**, written with no fraction or exponent. `1.0`,
     which localswim's schema accepts, `1e0`, the string `"1"`, `2` and every other value are
     refused, so a format localswim changes, which raises the version, is never read as this one.
  6. **`role` is the JSON string `bot`**, matching the file's name. `human` and every other value
     are refused.
  7. **`token_dpapi_hex` is a JSON string of an even number of hex digits, at least two**
     (`0-9`, `a-f`, `A-F`), with nothing around them. Each pair of digits, in order, is one byte,
     the first digit its high four bits. Text that is empty, of odd length, or holds whitespace or
     any other character is refused. **The push reads this member more tolerantly than localswim
     writes it, by the architect's ruling for v0.07-T02 (2026-11-20).** localswim's section and
     `docs/schemas/launch-credential.schema.json` are the writer's contract: lowercase hex, and a
     blob whose DPAPI header and flags field the schema's pattern pins. The push MUST accept hex
     digits in either case, and MUST NOT check the header or the flags in the hex: decrypting the
     blob, below, is the real check.
- **Those bytes are a DPAPI blob**, which localswim's section says is made in user scope with no
  optional entropy, and which begins `01 00 00 00` and then the DPAPI provider's GUID
  `D0 8C 9D DF 01 15 D1 11 8C 7A 00 C0 4F C2 97 EB`. The push does not check them; the call below
  refuses a blob whose first four bytes differ, and does not check the GUID. localswim's schema also pins the GUID and a flags field of 0 in its pattern, which the push
  does not apply ([What the laptop reads](#what-the-laptop-reads)): a blob made with entropy fails
  the call below, and a blob the call decrypts is accepted whatever its flags. **The reader
  decrypts it with the Win32 call `CryptUnprotectData` (`crypt32.dll`)**, run in the operator's own
  process, as the Windows user the operator runs as:

  | Argument | Value |
  |---|---|
  | `pDataIn` | The blob's bytes |
  | `ppszDataDescr` | `NULL`: the description is not wanted |
  | `pOptionalEntropy` | `NULL`: no entropy was used |
  | `pvReserved`, `pPromptStruct` | `NULL` |
  | `dwFlags` | `CRYPTPROTECT_UI_FORBIDDEN` (`0x1`), so it never shows a prompt |
  | `pDataOut` | Receives the plaintext, which the reader frees with `LocalFree` |

  The blob is protected in user scope, without `CRYPTPROTECT_LOCAL_MACHINE`, so only the same
  Windows user on the same computer can decrypt it. .NET's
  `ProtectedData.Unprotect(blob, null, DataProtectionScope.CurrentUser)` is the same call. **A
  call that fails is refused**: a blob written by another user or on another computer, a
  corrupted one, or a profile whose DPAPI keys are not available.
- **The plaintext is the token's text in UTF-16LE**, with no byte order mark and no terminating
  NUL, as localswim's section requires. A byte count that is odd is refused. Text that is not
  valid UTF-16 (an unpaired surrogate) is refused, since it has no UTF-8 form to send (D20). The
  token is that text, and the content carries it in UTF-8 (below).
- **The token MUST be exactly 43 characters, each from `A-Z`, `a-z`, `0-9`, `-` and `_`**, which
  the push checks after the plaintext's own rules above (the architect's ruling for v0.07-T02,
  2026-11-20). localswim's section makes each token 32 random bytes in unpadded base64url, which
  is 43 such characters, and requires a reader to check the decrypted plaintext itself
  (`docs/windows-operations.md`, "The credential files"). **A token of any other length, or with
  any other character, is refused** with the "Could not read" warning below, and its value is
  never printed: a byte order mark, a terminator, padding (`=`), `+`, `/`, whitespace and every
  character outside ASCII among them. This closes the token's half of [Q9](#open-questions),
  since no such token holds a character the box's wrapper reads as shell syntax ([The box's
  files](#the-boxs-files)).
- **A test writes a credential for itself the same way in reverse** ([The test seam](#the-test-seam)):
  `CryptProtectData` on the dummy token's UTF-16LE bytes, with no entropy and
  `CRYPTPROTECT_UI_FORBIDDEN`, as the user that runs the operator; the blob in lowercase hex as
  `token_dpapi_hex`, beside the other three members as rules 4 to 6 give them, in a `bot.json` in
  UTF-8. PowerShell's `ConvertFrom-SecureString` of a `SecureString` holding the dummy token, given
  no key, writes the same hex. A test that checks only a refusal MAY instead use localswim's
  example value, a DPAPI header followed by bytes of value zero, which `CryptUnprotectData`
  refuses. Its token is a dummy, as [Secrecy](#secrecy) requires of a check that stands in for
  `ssh`, and a dummy that the push is meant to read MUST be made up in the token's form above, 43
  base64url characters, or the push refuses it.

Note on where this came from: until E0.07-T02-S05 (#0162, D55) the push read localswim's earlier
credential file, a PowerShell XML serialization, and this section specified that file to the byte,
from what the prototype's cmdlets accepted and refused on throwaway files. localswim's PR #13
(`8d9d583`) replaced it with `bot.json` and specified the format itself, so this section now cites
localswim and states only agentspace's own checks. `token_dpapi_hex` holds the same hex the earlier
file held, so the DPAPI call and the plaintext's encoding rules are unchanged; the same story added
the token's own check, 43 base64url characters. The receipt's rules still match
what the prototype's cmdlets did, except where
[Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
lists a difference.

**The push MUST warn `agentspace: Could not read the localswim Bot token; the board will not work
on the box this session.`, and send nothing, when any of these holds:**
- The receipt exists and cannot be read, or is refused by [The receipt](#the-receipt)'s rules.
- `bot.json` cannot be read, or is refused by [The Bot credential](#the-bot-credential)'s rules, a
  failed decryption included.
- The base URL or the token is missing or empty.
- The base URL or the token holds a CR, an LF or a NUL. Any of them would break the content's
  two-line framing, or make the box install a value other than the one sent.

The warning MUST NOT include the failure's own text, which could carry part of the credential, nor
any value read from the receipt or the credential.
Because the token is read before the push is known to be due, a laptop whose credential cannot be
read gets this warning at every `up`, even when nothing would have been sent. The token's form is
[The Bot credential](#the-bot-credential)'s check above, and the URL's is [The
receipt](#the-receipt)'s, `http://127.0.0.1:PORT`, which already excludes an empty URL and a CR,
an LF or a NUL in it; the two checks above still bind the content as it is built. The prototype
checks neither empty values nor CR, LF and NUL, nor the URL's form ([Requirements the prototype
does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

**The content** is exactly these two lines, UTF-8, each ending in a line feed (LF):

```
LOCALSWIM_URL=<base URL>
LOCALSWIM_TOKEN=<token>
```

### The time zone

**The push carries the laptop's time zone as an IANA name, so that `date` on the box gives the
laptop's local time and offset (D32).** It is worked out each time `up` or `sync` considers a push.
**When `AGENTSPACE_TIMEZONE` is set and not empty, its value takes the place of step 1's result**,
and the laptop's own zone is not read ([The test seam](#the-test-seam)); steps 2 and 3 apply to it
as to any converted name.

1. **The Windows ID of the zone the laptop is set to at that moment is converted to an IANA name**
   with CLDR's mapping for territory `001`, the zone's default, for example `Pacific Standard
   Time` to `America/Los_Angeles`. On Windows that is what .NET's
   `TimeZoneInfo.TryConvertWindowsIdToIanaId` and ICU's `ucal_getTimeZoneIDForWindowsID` with no
   region both give, and the rewrite MAY use either. The prototype uses .NET's where its runtime
   has it, and otherwise the unversioned export of the `icu.dll` that Windows ships. A conversion
   that fails gives no zone.
   **The rewrite embeds the mapping as data** (architect, 2026-11-19, for E0.06-T01-S05): the
   `territory="001"` rows of CLDR's `windowsZones.xml`, from the newest CLDR release published at
   least two weeks before the build's story, as a table generated into `agentspace-zone` with that
   release's number beside it. It calls neither .NET nor ICU, so the mapping is the same on every
   laptop and testable on Linux. A Windows ID is matched case-sensitively, whole and untrimmed,
   as ICU matches it. This page holds no copy of the table; CLDR is its source, and a
   new release is taken by regenerating it in a story of its own.
2. **Seven legacy names are mapped to their current ones**, because CLDR gives them and today's
   box's `tzdata`, without `tzdata-legacy`, refuses them. Every other name is kept as it is:

   | Legacy name | Current name |
   |---|---|
   | `Asia/Calcutta` | `Asia/Kolkata` |
   | `Asia/Katmandu` | `Asia/Kathmandu` |
   | `Asia/Rangoon` | `Asia/Yangon` |
   | `Europe/Kiev` | `Europe/Kyiv` |
   | `America/Buenos_Aires` | `America/Argentina/Buenos_Aires` |
   | `America/Godthab` | `America/Nuuk` |
   | `America/Indianapolis` | `America/Indiana/Indianapolis` |

   The names are matched case-sensitively.
3. **The zone MUST then match `^[A-Za-z0-9_+][A-Za-z0-9_+-]*(/[A-Za-z0-9_+-]+)*\z`,
   case-sensitively**, where `[A-Za-z]` means the ASCII letters alone. A zone that does not, or no
   zone at all, becomes the empty zone: it is not sent, and the stamp carries it empty. The check
   comes before the zone is put into anything, and only a zone that passes is ever put on the
   box's command line. A leading `-` would be read by the box's zone tool as an option, and `\z`
   never matches before a trailing newline.

**The zone travels on the box's command line, and the token never does.** A checked zone holds no
quote and no character a shell treats as syntax.

### The stamp

**The stamp records the last push the box confirmed.** It is the SHA-256 of the UTF-8 bytes of:

```
<instance ID> LF <zone> LF <content>
```

where `<instance ID>` is the ID as EC2 gives it, with no whitespace around it, `<zone>` is the
checked zone or empty, and `<content>` is the two lines above with their LFs. So it changes when
the instance, the zone, the URL or the token changes, and a stamp written by an operator that
built its input differently never matches, so the first connection after such a change pushes once.

- **There is one stamp per host** (D50), each over that host's own instance ID, so one stamp for
  every host would differ at each connection to another host and push every time. The hash's input
  is unchanged.
- **The stamp is that hash written as uppercase hex.** How it is stored, and where, is
  docs/state.md's: the box's is `%USERPROFILE%\.agentspace\localswim-pushed.json`, a JSON document
  (D40), unchanged by v0.07 (D55), and another host's is
  `%USERPROFILE%\.agentspace\hosts\<project>\localswim-pushed.json` in the same form
  ([state.md, A host's folder](state.md#a-hosts-folder)). The
  prototype's is `localswim-pushed.sha256`, the hex alone with no line end, which the rewrite never
  reads, so the first `up` after the change-over pushes once. A stored stamp that matches its
  schema is compared with the computed one exactly.
- **A push is due when it is forced (`sync`), when there is no stamp, when the stored stamp is
  empty, cannot be read, cannot be parsed or does not match its schema, or when it differs from
  the one just computed.** The prototype instead
  warns "Could not send" and pushes nothing when the stamp file is empty or unreadable
  ([Requirements the prototype does not meet
  yet](#requirements-the-prototype-does-not-meet-yet)).
- **A start of the host MUST NOT make the push due by itself** (#0167; P14, architect, 2026-11-21,
  for v0.08-T01). The stamp covers the instance ID, the zone and the content, and a stop and a
  start change none of them: the host's record keeps its instance ID, and the credential file and
  the system zone are on the host's disk ([What the box does](#what-the-box-does)). So a
  connection that starts a stopped host pushes only when the stamp says so, as every other
  connection does, and the build's decision is this rule unchanged. **The first connection after
  localswim's cold start finds the push due whatever the host's state**, because the token is new,
  and that connection is often the one that starts a host. That is the reading, an inference made
  without reading any real stamp, of the pushes seen at host starts in v0.07-T06: localswim's
  service came back at 8:15pm (Pacific) on 2026-11-21, and the two connections that pushed were the
  first to each host after it ([the run record](runs/e0.07-t06-s01.md)).
- **When the push is not due, nothing is sent, and nothing is printed** but a warning from
  [What the laptop reads](#what-the-laptop-reads).
- **The stamp is written only after the box confirms the token and, when a zone was sent, the
  zone** ([What the laptop checks](#what-the-laptop-checks)), creating its folder if needed. Any
  other outcome leaves the stored stamp as it was, so the next connection tries again.
- **The stamp is not `bot.env`'s hash**, and a check MUST NOT compare it with `sha256sum` of the
  box's file. A test proves a push by setting the stamp aside and seeing it come back equal, and by
  `bot.env`'s modification time.

### What the laptop sends

When the push is due, the laptop:

1. **Prints `Sending the localswim Bot token to the instance...`** (nothing, in the ProxyCommand's
   caller mode: [What a caller sees](#what-a-caller-sees-d1-card-0087)), and, when the zone is empty,
   the warning `agentspace: Could not work out the laptop's time zone; times on the box stay in its
   own zone.`
2. **Starts one ssh**, Windows OpenSSH by full path (#0027), with these arguments:

   ```
   -o ClearAllForwardings=yes -o BatchMode=yes <alias> <remote command>
   ```

   - `<alias>` is `up`'s `-Alias`, or for `sync` its host's alias: `agentspace` for the box,
     `agentspace-host.<project>` for another host.
   - `BatchMode=yes` makes an ssh that would need a prompt fail at once rather than wait for an
     answer: a passphrase, a password, or a host key to confirm. It does not refuse a host key
     that the alias accepts without a prompt, such as the `agentspace` entry's first key
     ([The `agentspace` SSH entry](#the-agentspace-ssh-entry)).
   - **`<remote command>` MUST reach ssh as one argument, its last**: a command that makes the
     login user's shell on the box carry out the box's half of the protocol
     ([What the box does](#what-the-box-does)). The operator writes its own command; this page
     specifies what it MUST do, never its text (D34). It names the zone when one is sent, and
     MUST NOT carry the token.
   - **On Windows, where a program receives one command-line string, the operator MUST quote the
     command so that the C runtime's argument rules give it back to ssh as one argument.** A
     command that holds no double quote and does not end in a backslash MAY simply be wrapped in
     one pair of double quotes; any other MUST be quoted to those rules in full.
   - **The remote command MUST hold no `%`, no double quote and no control character, and MUST NOT
     end in a backslash** (architect, 2026-11-19, for v0.06, gap FA-15). Then one pair of double
     quotes gives it back whole both to a program and to a `.cmd` stand-in
     ([The test seam](#the-test-seam)), whose `cmd.exe` expands `%` even inside quotes and reads a
     double quote as the end of the quoted part. Every other printable ASCII character survives,
     `&`, `|`, `<`, `>`, `^` and `!` among them, since `cmd.exe` reads them as text inside
     quotes and the operator's stand-ins never turn on delayed expansion. The patch question
     ([What up waits for](#what-up-waits-for)) is under the same rule, as well as that section's
     own.
   - **The ssh gets a pipe for its stdin and one for its stdout. Its stderr depends on what runs
     the push** (#0167; P14, architect, 2026-11-21, for v0.08-T01; built by E0.08-T02-S04, #0176):
     - **In a person's `up`, `up -Host` and `sync`, it is the operator's stderr**, inherited, so it
       carries its nested ProxyCommand's lines, `The host is ready.` among them
       ([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)).
     - **In the ProxyCommand's caller mode, it is a pipe, and each line read from it is written to
       the ProxyCommand's stderr as it arrives, except a line that is exactly
       `The host is ready.`, which is dropped.** The line is compared without its line ending, an
       LF or a CR LF, and passed on with the line ending it came with (architect, 2026-11-21, for v0.08-T01, S01 batch). A last
       line with no line ending is passed on as it is, with none added, and every byte passes
       unchanged, whatever its encoding: the lines are compared as bytes and never decoded. So
       a connection prints `The host is ready.` once, its own, whether or not it pushes, and a push
       that fails still shows its ssh's error lines. **With `AGENTSPACE_DIAGNOSTIC` set, nothing is
       dropped.** Not the null device, which would swallow a failed push's ssh errors.
     - **The pipe is read while the push runs, and no longer**: once the ssh has exited and its
       stdout has ended, or been given up on after its 10 seconds (step 5), what the pipe already
       holds is read and passed on first, by the rules above, without waiting for more, and only
       what arrives after that is dropped; once the tree is stopped at the 20-second limit
       (step 4), the push stops reading it at once, and a line still unread then is dropped
       (architect, 2026-11-21, for v0.08-T01, S01 batch). So the pipe never lengthens a push, a
       failed push's last error lines are not lost to a race with its exit, and a full pipe never
       stops the ssh.
     - **The choice MUST NOT change anything else in the push**: its 20-second limit, its
       10-second wait for stdout, the stop of the ssh's tree, and every warning are as this section
       says.

     Its environment is the operator's, as [Environment variables](#environment-variables) sets
     it.
   - An ssh that cannot be started at all warns as [any other failure](#what-the-laptop-checks)
     does.
3. **Starts a 20-second clock as the ssh starts, writes the content to the ssh's stdin, and closes
   it.** The write is inside the 20 seconds, so a write that blocks is cut off by the same limit.
   The content SHOULD be sent with no byte order mark; the prototype's runtime may put a UTF-8 BOM
   ahead of it, which is why the box strips one ([Q18](#open-questions)). A write that fails
   because ssh has already exited is not an error here; the missing confirmation reports it.
4. **Waits for the ssh to exit until the 20 seconds are up.** At the limit it stops, forcibly and
   without asking them to exit, that ssh and every process descended from it (its ProxyCommand,
   the nested `up`, the tunnel), and no other process. The tree is the ssh and every process
   whose chain of parent processes leads to it, found from the ssh's process ID; each is
   terminated at once, and the ssh itself is terminated last if it is still running. Note on
   where this came from: the prototype uses Windows' `taskkill` with its tree and force options on
   the ssh's process ID, then kills the ssh. It then prints the warning `agentspace: Sending the localswim
   token took over 20 seconds and was stopped; the board will not work on the box this session.`,
   and ends the push at once, reading nothing more and recording nothing. A push takes 5 to 8
   seconds, and a client such as the desktop app gives up on a slow banner, so the limit stays
   short.
5. **Once the ssh has exited on its own, reads its exit code and its stdout**, waiting at most 10
   more seconds for the stdout to end, and taking it as empty if it does not. The stdout ends only
   when every process holding its pipe has closed it, and a process the ssh started can outlive
   the ssh, so the wait is bounded. The 10 seconds apply only here, never after a kill.

### What the box does

**The box's half of the protocol, as any host MUST carry it out.** On today's box it is a POSIX
shell command run by the login user's shell; on another platform it is that platform's
equivalent.

1. **Read at most two lines from stdin, and stop reading after 30 seconds.** It MUST NOT wait for
   the end of its input beyond those two lines. Whatever was read when the limit hit, if anything,
   goes on to the next steps and is refused at step 4 unless it is complete. The box's 30 seconds
   are longer than the laptop's 20, so in a push the laptop's limit comes first; the box's bounds a
   read whose laptop end has gone without closing the connection.
2. **Normalize them, in this order:** remove every CR, then remove a UTF-8 BOM (the bytes
   `EF BB BF`) at the start of the first line only, then drop every line left empty. The two lines
   are taken before empty lines are dropped, so a blank line ahead of them leaves one line, and the
   content is refused. The laptop MUST send nothing ahead of the two lines but, at most, a BOM.
3. **Write the result to a new temporary file, with a name no other run uses, in the credential
   file's folder**, each line ending in LF, creating the folder if it does not exist. The temporary
   file and the credential file MUST be readable by the login user alone from the moment they
   exist, and so MUST a folder the box creates (today: file mode 0600, and a new folder 0700). A
   folder that already exists is used as it is, its mode unchanged.
4. **Accept it only if it has exactly two lines, one starting `LOCALSWIM_URL=` and one starting
   `LOCALSWIM_TOKEN=`, each followed by at least one character.** Today's box checks the count and
   that each prefix is present, not their order.
5. **Rename the temporary file over the credential file**, in one atomic step. The credential file
   is `~/.config/localswim/bot.env` on today's box.
6. **Print the SHA-256 of the installed credential file's bytes** on a line of its own: 64
   lowercase hex digits, then whitespace (today `sha256sum`'s `<hash>  -`).
7. **If a zone was given, and only after step 6:** set the system time zone to it, with the
   privilege that takes but never prompting for a password, with no input, printing nothing of its
   own, and giving up after 5 seconds (a setter still running 1 second later is killed). Today's
   box does it with systemd's `timedatectl`, through `sudo` with no password. Then print one more
   line, `zone <zone>` if it succeeded or `zone-failed` if not, and exit 0 whatever the zone did.
   The zone MUST be the system zone, never an exported `TZ` (D32). Today's box does not check the
   zone itself; the laptop's check is the only one ([The time zone](#the-time-zone)), and a host
   MAY check it again.
8. **Without a zone,** exit 0 once the hash is printed. Whatever ends the run, the temporary file
   from step 3 MUST NOT be left behind: the rename in step 5 consumes it, and every other exit
   removes it.

**On any failure before step 6**, the box MUST remove the temporary file, leave the old credential
file as it was, and exit non-zero (1 today). If step 6 itself fails, the new credential file stays
in place, no hash line is printed, and the box exits non-zero. The credential file is never left
empty or partial, whatever happens to the connection. **The box MUST NOT write the content, or any
part of it, to stdout or stderr.**

**What the box answers, then:**

| Outcome | Stdout | Exit |
|---|---|---|
| Token installed, no zone sent | `<hash>  -` | 0 |
| Token installed, zone set | `<hash>  -`, then `zone <zone>` | 0 |
| Token installed, zone refused | `<hash>  -`, then `zone-failed` | 0 |
| Token refused, or any failure before step 6; the old credential file stays | No hash line | Non-zero (1 today) |
| Token installed, but step 6 failed | No hash line | Non-zero (1 today) |
| The ssh itself failed: no login, an alias that needs a prompt, no tunnel | Nothing | 255, ssh's own |

**Other lines MAY come before or among these**, such as a banner that the login user's shell or
its startup files print. The laptop skips every line it is not looking for
([What the laptop checks](#what-the-laptop-checks)).

### What the laptop checks

**The ssh's stdout is split into lines at each LF**; a CR just before an LF belongs to the line's
end.

1. **The token is confirmed when the ssh exited 0, and the first hash line of its stdout carries
   the expected hash.** A hash line is one whose first 64 characters are hex digits (`0-9`, `a-f`
   or `A-F`) and whose 65th is a space, a tab, a CR or the LF that ends it. The hash is compared
   with the expected one without regard to case. The expected hash is the SHA-256, as hex, of the
   content as the box writes it: CRs removed, a leading BOM removed, empty lines dropped, and every
   remaining line ending in LF. Content the laptop has accepted holds none of those, so that is the
   SHA-256 of the content's own bytes.
   **Otherwise, for any non-zero exit (255 included) or no matching hash line**, it warns
   `agentspace: The instance did not confirm the localswim token; the board will not work on the
   box this session.`, records nothing, and checks no zone.
2. **The zone is confirmed**, when one was sent, when a line of stdout is exactly `zone <zone>`,
   compared case-sensitively, with its line end (LF or CRLF) removed. Otherwise (`zone-failed`, or
   no zone line) it warns `agentspace: The instance did not take the time zone <zone>; times on the
   box stay in its own zone.` and records nothing, so every later connection pushes again, a login
   each time, until the box takes a zone. The token stays installed.
3. **Both confirmed:** it writes the stamp ([The stamp](#the-stamp)).

**Any other failure in the push**, such as an ssh that cannot be started or a stamp that cannot be
written, warns `agentspace: Could not send the localswim token; the board will not work on the box
this session.` and records nothing. **A push never fails the command**: every outcome here is a
warning, and `up` and `sync` go on as if the push had succeeded. `-Quiet` suppresses these
warnings with the rest ([Messages](#messages); [Q21](#open-questions)).

**Rulings for the rewrite's push** (architect, 2026-11-19, for E0.06-T01-S04):
- **The zone comes from `agentspace-zone`**, already mapped and checked (steps 1 to 3 of
  [The time zone](#the-time-zone)); the push checks step 3 again only as a guard before the zone
  goes on a command line, and holds no copy of the legacy table.
- **`AGENTSPACE_NESTED` stops `up`'s push alone**; `sync`'s forced push runs whatever it says.
- **A stamp file that is not UTF-8**, such as one in UTF-16, cannot be parsed, so a push is due.
- **An alias that breaks `-Alias`'s rule**, which the command line refuses before any push, gets
  the bad-alias warning below if one ever reaches the push, and no `Sending` line. (This bullet
  first said "the warning above", the generic one, and overlooked the specific warning v0.04 had
  already given; corrected by the architect the same afternoon, found in E0.06-T01-S04's review.)
  The push checks the alias, and `AGENTSPACE_NESTED` unless forced, before it reads anything.
- **The box's last stdout line counts as a line even with no LF after it**, so a hash line that
  ends the output is still read. A line cut short by the 10-second bound is not 64 hex digits, so
  it is no hash line.
- **The Bot credential's reader:** these rulings covered the earlier XML credential file, and lapsed
  with it when E0.07-T02-S05 (#0162) moved the push to `bot.json`, whose rules are
  [The Bot credential](#the-bot-credential)'s.

**A push handed a bad alias by any path but the command line** warns `agentspace: The SSH host
alias for the localswim token is not valid; the board will not work on the box this session.` and
skips; from the command line it cannot happen.

### Secrecy

- **The token MUST travel only on the push's stdin.** It MUST NOT appear in any process's
  arguments, on the laptop or the box; in any environment variable the operator sets or any
  program it starts inherits; in any message, warning or log; or in any file on the laptop. On the
  box, only the wrapper puts it in the environment, of the one CLI run it starts
  ([The box's files](#the-boxs-files)).
- **On the box it lives only in the credential file**, and briefly in its temporary file, both
  readable by the login user alone.
- **On the laptop the operator keeps only the stamp**, a hash that covers the token
  ([Q27](#open-questions)). It MUST NOT keep the token in a file, a cache or a temporary file, and
  SHOULD drop it from memory once the push ends.
- **A warning or failure in the push MUST NOT quote the error that caused it**, because that text
  could carry part of the receipt or the credential. Each warning is a fixed text.
- **A check that stubs ssh MUST point `LOCALAPPDATA` at a fake receipt and credential**, so that a
  stub never receives the real token on its stdin. The operator finds the receipt only through
  `LOCALAPPDATA` ([What the laptop reads](#what-the-laptop-reads); [The test seam](#the-test-seam)).

## The box as the laptop sees it

**This section is the contract between the laptop and today's box, and from v0.07 every host
holds it the same way**: each is made from the one template, gets the same `box\system` through
`sync` and the same toolchain through `bootstrap` (D44, D46), so "the box" in this section reads as
"each host". Every path and tool in it is a property of today's box (Ubuntu, systemd, bash), and
the box's scripts that carry it out stay shell (D28). **The box is changed through
`<checkout>\box\system` and `sync`, never by editing the box:** every file `sync` puts there
([The box's files](#the-boxs-files)) is replaced by the next `sync`, so a change made on the box
itself is lost. sshd keeps the first value it reads for a keyword, so a new sshd drop-in MUST NOT
set a keyword that another drop-in already sets.

### What sync installs

`sync` sends `<checkout>\box\system` whole, with the CA's public key beside it as
`agentspace_ca.pub`, and runs its `install.sh` as root from that folder. The install:

- **Fails at once if `agentspace_ca.pub` is missing or empty**, printing `install.sh:
  agentspace_ca.pub is missing; run agentspace-operator sync, which sends it` to stderr and
  exiting 1. The prototype's box files name `scripts\agentspace.ps1` there. **The repository's
  `box\system\install.sh` is changed to this text in the pull request that removes
  `scripts\agentspace.ps1`** (architect, 2026-11-19, for v0.06, gap FA-12): under D37's addendum
  the box's files change only through the repository, so the clean room's builders do not edit it.
  **That pull request is E0.06-T06-S01's, not the cutover's** (D42, 2026-11-19; E0.06-T06-S01,
  2026-11-20), and it makes this change.
- **Retires the project's first name, once.** On a box that still has `agentbox` units, it
  disables them, carries over this boot's patch marker and the last patch-reboot time, removes the
  old files, and prints `Retired the agentbox units and files`.
- **Installs the box's files** (the table below), reloads systemd, enables and starts the idle
  timer, and enables the patch service. It prints `Installed: idle check (timer <active state>),
  patch at boot (<enabled state>)`.
- **Puts the login user's tool folders on every session's `PATH`** (D61's change, #0166; P5,
  architect, 2026-11-21, for v0.08-T01), in `/etc/environment`, which PAM reads at the start of
  every ssh session, a non-login, non-interactive one included, and reads without expanding
  variables, so every folder is written out. The three folders, in this order, are
  `/home/ubuntu/.cargo/bin` (rustup's proxies, for `rustc` and `cargo`), `/home/ubuntu/.local/bin`
  (`claude`) and `/home/ubuntu/.local/share/fnm/aliases/default/bin` (fnm's default alias, for
  `codex` and Node's other global tools); `gh` is in `/usr/bin`. Nothing is linked, so each follows
  its tool's updates, and a tool installed later by the sign-ins (D52) is found without another
  `sync`.
  - **Every `PATH` line keeps its value, and each of the three folders absent from it is appended,
    in that order.** A `PATH` line is one that starts `PATH=` after any leading whitespace and an
    optional `export ` (as `pam_env` allows); its `export `, its quotes around the value and a CR ending are kept as they were.
    The value is what comes before the line's first `#` and the blanks ahead of it, as `pam_env`
    reads it, and the folders are appended to that value, inside its quotes; the blanks and the
    comment after it are kept after it. Every other line is left as it was, byte for byte and in
    its order. **A `PATH` line whose value is neither plain (no quote character in it) nor quoted
    whole (one matching pair of single or double quotes around it, and no other quote character)**
    fails the install before `/etc/environment` is written, leaving the file as it was and
    running none of the steps after this one, with
    `install.sh: line <n> of /etc/environment sets PATH to a value that is neither plain nor quoted whole; the file is left as it was, so fix that line by hand and run agentspace-operator sync again`
    on stderr and exit 1, `<n>` the line's number from 1. **Only a file with no `PATH` line gets
    one**, added at its end, after a line feed when the file's last line had none, with Ubuntu's
    default folders and then the three:

    ```
    PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/usr/games:/usr/local/games:/snap/bin:/home/ubuntu/.cargo/bin:/home/ubuntu/.local/bin:/home/ubuntu/.local/share/fnm/aliases/default/bin"
    ```

    A missing file is created holding that line alone. Rewriting from Ubuntu's default would drop
    a folder someone added by hand, so it is never done (P5).
  - **The file is replaced by a rename**, root-owned, mode 0644, so a reader never sees half of
    it, and **a file the install would not change is not touched**: a second run finds nothing to
    add. Then it reads the file back and checks, as the sshd settings are checked below, that it
    has at least one `PATH` line, that every `PATH` line holds every folder in the value `pam_env`
    reads, and that none is neither plain nor quoted whole, and exits 1 if not with
    `install.sh: /etc/environment has no PATH line holding /home/ubuntu/.cargo/bin /home/ubuntu/.local/bin /home/ubuntu/.local/share/fnm/aliases/default/bin after the install wrote it`
    on stderr.
  - **It takes effect for sessions that start after it**: a session already open keeps the `PATH`
    it started with.
  - It prints `Installed: /etc/environment's PATH holds the ubuntu user's tool folders (added)`,
    or `(already there)` in place of `(added)` when it changed nothing.
- **Installs the CA's public key and both sshd drop-ins as one transaction.** It saves what was
  there, installs the three files, and runs sshd's config test. If the test fails, it puts back
  every file it changed, does not reload sshd, prints `install.sh: sshd -t rejected the new SSH
  config; put the old files back and did not reload ssh.service`, and exits 1. Otherwise it reloads
  sshd, which applies to new connections only. The CA key file is replaced, never appended to, so
  a new CA drops the old one (D7, D17).
- **Checks that sshd's effective settings are the drop-ins' values**, and exits 1 with
  `install.sh: sshd reports '<settings>'; another setting overrides <drop-in>` if not:
  `permitrootlogin no` and `trustedusercakeys /etc/ssh/agentspace_ca.pub`; and
  `clientalivecountmax 5`, `clientaliveinterval 15`, `logingracetime 30` and
  `persourcepenaltyexemptlist 127.0.0.1`. A failure here comes after the reload, so the new files
  stay in place.
- **Prints three closing lines:**

  ```
  Installed: SSH trusts the agentspace CA (<fp>), root login closed
  Installed: 127.0.0.1 exempt from sshd penalties, login grace time 30 seconds
  Installed: a session whose client has gone is dropped about 90 seconds after it goes quiet (probe every 15 seconds, 5 unanswered)
  ```

- **Last, starts this boot's patch run when it has not finished** (D61's change, #0163), after
  every other change and check has succeeded, so an install that exits 1 starts no patch run, and
  the patch run's upgrades and any reboot never overlap the SSH steps above. It prints one line:
  - **`/run/agentspace/patched` exists:** it starts nothing, and prints
    `Patch run: finished this boot; not started again`.
  - **Otherwise** it starts `agentspace-patch.service` with `systemctl start --no-block`, which
    queues the start and returns at once, so the install never waits on the patch run and a patch
    reboot never cuts it short. It prints
    `Patch run: already running this boot; not started again` when the unit was already
    activating as the install found it, since that start merges into the run under way and no
    second run begins, and otherwise
    `Patch run: started for this boot, as /run/agentspace/patched is missing`. A run that ended
    this boot without the marker, a failed one, is started again.
  - **A start that systemd refuses** exits 1 with
    `install.sh: systemd refused to start agentspace-patch.service; this boot's patch run has not started`
    on stderr.

  `sync` then waits for the marker ([The patch wait](#the-patch-wait)). **The install issues the
  same `systemctl start --no-block` whatever state it found the unit in, an `activating` one
  included**, and only its line differs: it reads the unit's state with `systemctl is-active`
  before the start, and relies on systemd to merge a start on an activating unit into the job
  under way. That merge is not probed on a host here; E0.08-T05-S01's live check is the probe
  (D61's change).

**The `Installed:` lines and the `Patch run:` line go to the box's stdout, and so to `sync`'s
stdout; the `install.sh:` lines go to stderr.** Exit 0 means everything above took effect, the
patch run's start queued when it was due, and `sync` fails on any other exit. The install's lines
are as E0.08-T01-S05 builds `box/system/install.sh`.

### The box's files

| Path on today's box | Put there by | What it is for |
|---|---|---|
| `/usr/local/sbin/agentspace-patch`, `/etc/systemd/system/agentspace-patch.service` (enabled) | `sync` | Patches the box at every boot, within the unit's own 20-minute limit (`TimeoutStartSec=20min`), and leaves the marker `up` and `sync` wait for. `sync`'s install also starts it when this boot has no marker (#0163) |
| `/run/agentspace/patched` | The patch run | Exists once this boot's patch run has finished. Removed as each run starts, and gone at every boot. `up` waits for it, and `sync` waits for it after its install |
| `/run/agentspace/last-active`, `/run/agentspace/busy` | The idle check | When the box was last active, and the CPU counter at the last check; gone at every boot, so each boot starts a fresh idle window |
| `/run/agentspace/lease-horizon` | The guard, through Run Command ([guard](#guard)) | The host's lease horizon: one line, the horizon as Unix seconds and an LF, root-owned, mode 0644 (D63). Read by the idle check alone ([The idle check and the lease horizon](#the-idle-check-and-the-lease-horizon)); gone at every boot, so a host just started has none until the next guard run (D57) |
| `/var/lib/agentspace/last-patch-reboot` | The patch run | Keeps the patch run from rebooting more than once per 15 minutes |
| `/usr/local/bin/agentspace-tools` | `sync` | Moves the login user's toolchain forward; the patch run starts it |
| `/usr/local/sbin/agentspace-idle-check`, `/etc/systemd/system/agentspace-idle.service`, `agentspace-idle.timer` (enabled, started) | `sync` | Powers the box off when idle (D11), or once its lease horizon and the grace have passed with no session and no patch run (D57) ([The idle check and the lease horizon](#the-idle-check-and-the-lease-horizon)) |
| `/etc/agentspace/idle.conf` | `sync` | The idle limits: 60 minutes, and 6 CPU-seconds per 5-minute check; and the horizon's grace, `HORIZON_GRACE_MINUTES=15` (D56, D63) |
| `/usr/local/bin/localswim` | `sync` | The wrapper agents run as `localswim`; it reads the credential file |
| `/usr/local/lib/localswim/localswim` | `sync`, built on the laptop | The real localswim CLI |
| `/usr/local/bin/agentspace-sample`, `/usr/local/bin/agentspace-footprint` | `sync` | Measure a turn (D33) |
| `/etc/ssh/agentspace_ca.pub` | `sync` | The CA sshd trusts for user certificates |
| `/etc/ssh/sshd_config.d/50-agentspace-ca.conf` | `sync` | Trusts the CA, and closes root login (D7) |
| `/etc/ssh/sshd_config.d/51-agentspace-logins.conf` | `sync` | Exempts 127.0.0.1 from login penalties, a 30-second login grace time, and a dead client dropped after about 90 seconds (D7, D12) |
| `/etc/environment`, its `PATH` lines | `sync` | Puts the login user's tool folders on every ssh session's `PATH` (#0166); the file's other lines are left as they are ([What sync installs](#what-sync-installs)) |
| `~/.config/localswim/bot.env` (login user's, mode 0600, folder 0700 when created) | The token push | The board's URL and Bot token, for the wrapper |
| `~/.claude/CLAUDE.md` (login user's) | `sync` | A copy of the laptop's; MUST NOT be edited on the box |
| The system time zone | The token push | The laptop's zone (D32) |
| `/tmp/agentspace-system/`, `/tmp/localswim` | `sync`, briefly | Staging; removed once installed |

**The credential file is read by the wrapper as shell assignments**: it exports each line for one
run of the CLI and never prints them. A value that holds a character a POSIX shell reads as syntax
(a space, a quote, `$`, a backquote or a backslash, for some) is installed as sent and then read
differently by the wrapper. **The token cannot hold one**: the push refuses any token that is not
exactly 43 characters of `A-Z`, `a-z`, `0-9`, `-` and `_` ([The Bot
credential](#the-bot-credential)), none of which a POSIX shell reads as syntax. **Nor can the
URL**: the push refuses any URL that is not exactly `http://127.0.0.1:PORT`, with `PORT` a decimal
port from 1 to 65535 ([The receipt](#the-receipt)), none of whose characters a POSIX shell reads
as syntax. Both checks close [Q9](#open-questions). The wrapper refuses with a message to stderr and exit 1 when the CLI is
not installed, when the credential file is missing, or when the board's address does not answer,
which happens whenever no connection holds the board's forwards (D12).

**Installed by `bootstrap`, not by `sync`: the toolchain** ([bootstrap](#bootstrap); D46). The
bootstrap is the repository's `box/bootstrap.sh`, run as the login user, and **MUST install**, each
only when it is missing, so that a second run is safe:
- gh, Node through fnm, Claude Code, Codex and uv, as it did before v0.07;
- **Ubuntu's `build-essential`** (gcc, for linking), and **rustup with the stable toolchain,
  `rustfmt` and `clippy`**, which the box got by hand (D38, D46).

Rust is installed and not promised: the `agent-linux` profile's promise does not change
([lease-api.md, Profiles](lease-api.md#profiles); D46). Until v0.07 a person ran the script once
per instance over `ssh agentspace`, and no operator command ran it. The patch run's tools step
reports, and continues, when the toolchain is missing. **What the script runs to install each is
its own** (D28); the template story of v0.07 (E0.07-T01-S05) adds Rust and gcc to it.

**A new host's first boot also trusts the CA** (D44): the stack's `CaPublicKey` is installed with
its sshd drop-in by the first-boot user data, so the operator's certificate is accepted at its first
connection. `sync` then installs the CA's public key as for the box ([What sync
installs](#what-sync-installs)), replacing the first boot's copy with the CA file's own bytes, which
sshd reads the same (P8).

**At every boot, the box grows its root partition and root filesystem to fill the root volume**,
before any login. That is what lets `grow` leave a stopped box, or a failed online step, to the
next boot ([grow](#grow)), and a host MUST do the same. On today's box it is the Ubuntu cloud
image's own boot-time setup (cloud-init's partition and filesystem growth, which run at every
boot); agentspace installs nothing for it, and the stack's first-boot user data does not
configure it.

### What up waits for

**`up` asks the box one question through Session Manager's Run Command, never over SSH**, because
SSH is what it is getting ready:

- **The question** is sent with the comment `agentspace up: patch check`. The box MUST answer
  with exactly one word on stdout:
  - **`patched`** when this boot's patch run has finished.
  - **`failed`** when this boot's last patch run ended without finishing (P21), and no run is
    under way and no start job queued (architect, 2026-11-21, for v0.08-T01, pin P24).
  - **`pending`** when a patch run is due or under way this boot.
  - **`unknown`** when no patch run is set up, such as before the first `sync`.

  On today's box the marker of a finished run is `/run/agentspace/patched`, the patch run is the
  unit `agentspace-patch.service`, enabled when set up, and the question is sent as the document
  `AWS-RunShellScript`, which runs as root. **The command answers `patched` when the marker
  exists; otherwise `failed` when the unit is in systemd's `failed` state (its `ActiveState` is
  `failed`, which `systemctl is-failed agentspace-patch.service` tests) without a start job
  queued; otherwise `pending` when the unit is enabled, and `unknown` when it is not.** One remote
  command reads the marker, the unit's state, whether a start job is queued and whether the unit
  is enabled, in that order, and prints the one word. The unit is `failed` when its script exited
  non-zero, an `apt-get` that failed among the causes, or when systemd killed it at its 20-minute
  limit (`TimeoutStartSec=20min`). **A run that asks for a reboot runs `systemctl reboot`, which
  queues the reboot and returns at once, and then exits 0**, so the unit is normally not `failed`
  across a patch reboot. **If the shutdown stops the unit before that `exit 0`**, the unit can end
  `failed`, since systemd counts no signal as a clean end of a `oneshot` unit, and a check that
  runs in the moments before the host goes down then answers `failed`. That state does not outlive
  the boot: every boot starts with the unit in no failed state, and a later boot's check reads the
  marker that boot's run writes. **A `failed` unit with a start job queued answers `pending`**, so
  a check that runs between the install's `systemctl start --no-block` and the run's start, while
  the unit still shows the earlier run's failure, never reads that failure as this run's
  (architect, 2026-11-21, for v0.08-T01, pin P24).
  The command sent MUST hold no comma, bracket or equals sign, which the AWS CLI's shorthand syntax
  for `--parameters` would split on, or MUST be passed in a form that is not split.
- **How the answer is read.** `up` polls the command's invocation once a second, for up to 20
  seconds. While its status is `Pending`, `InProgress` or `Delayed`, or the invocation cannot be
  read yet, it polls again. Any other status ends the poll, and the answer is the invocation's
  stdout, trimmed. **20 polls without an end count as `pending`.**
- **What counts as ready:** every answer but `pending`
  ([Starting the box](#starting-the-box-and-waiting-for-it) lists what that lets through).
  **`up` reads `failed` as ready, as it reads `unknown`: its wait ends, it prints `Ready: <id>`, and
  it adds no message of its own** (P21; architect, 2026-11-21, for v0.08-T01, pin P24). Before
  v0.08 a failed run
  answered `pending`, and `up` waited out its 15 minutes and failed. **The reason is `sync`'s
  nested connections**: once `sync`'s patch wait goes on past a failed run, as P21 requires, each
  of rows 5 to 9 runs a nested ProxyCommand whose `up` asks the same question of a host launched
  less than 20 minutes ago, a new host in its provisioning run among them. Waiting there would fail
  `sync` on the patch run's failure after all, which P21 forbids. `up` starts no patch run, so a
  wait against a failed unit could only run out; the next `sync` starts the run again.
  **`sync`'s patch wait reads `failed` its own way**: it ends the wait with a warning, and is never
  read as `patched` ([The patch wait](#the-patch-wait)).
- **What the box does at boot, as far as `up` can see:** the patch unit runs once, after the
  network is up, with a 20-minute limit. It removes the marker, upgrades every package
  (`dist-upgrade`, phased updates included) and snaps, reboots at most once per 15 minutes if a
  package asks for it, moves the toolchain forward, and then writes the marker. A failed run
  writes no marker and leaves the unit `failed`, which ends `up`'s wait, as above. The unit's own
  limit, 20 minutes, is longer than `up`'s wait, so a slow run that is still working ends in `up`'s
  failure at its 15-minute deadline.
  **A run that reboots writes no marker before the reboot**; the next boot's run of the unit
  writes it.
- **`sync`'s install also starts the unit, when this boot has no marker** (D61's change, #0163;
  [What sync installs](#what-sync-installs)). That is how a host's first boot, in which the unit
  was installed too late to run, gets its patch run, and how a run that failed this boot is run
  again. `sync` then asks the same question until it is answered `patched`, or `failed`, which
  ends its wait with a warning ([The patch wait](#the-patch-wait); P21).

### Idle stop, as the laptop sees it

**From v0.08 a host is stopped three ways, and the first is the laptop's** (D56, D57, D58). The
instance's shutdown behavior is stop, so in each case the next connection starts it again. **All
three apply to every host, the box included** (D56; Terry, 2026-11-21, closing Q9): `sync` installs
the idle check on each, the guard reads every host in `hosts.json`, and each host's stack carries
its own alarm (D44).

- **The guard stops a host its project's live leases do not explain**, a grace (15 minutes) after
  the later of its launch time and its project's last lease end ([guard](#guard); D56). It runs on
  the laptop, started by the keeper, so it needs the laptop on.
- **The host's own idle check powers it off** after 60 minutes with no SSH session, no Session
  Manager session, no patch run and less than 6 CPU-seconds of work per 5-minute check (D11), or,
  once its lease horizon and the grace have passed, at the first check with no SSH or Session
  Manager session and no patch run, whatever its CPU is doing (D57; P9, architect, 2026-11-21, for
  v0.08-T01; [The idle check and the lease horizon](#the-idle-check-and-the-lease-horizon)). That
  is what bounds a host's cost while the laptop is off or asleep.
- **The stack's alarm stops it after 24 hours averaging under 1% CPU**, whatever sessions are open
  ([The stack](#the-stack); D58). It is the backstop for a host whose idle check has died, idle,
  with the laptop away.

### The idle check and the lease horizon

**The idle check is `/usr/local/sbin/agentspace-idle-check`, run as root by
`agentspace-idle.service`, which `agentspace-idle.timer` starts 5 minutes after boot and then 5
minutes after each run started** (`OnBootSec=5min`, `OnUnitActiveSec=5min`), each start within
`AccuracySec=30s` of its time, so the 5 minutes between checks are approximate. Each run decides once, prints one line to its journal, and either does
nothing more or powers the host off with `systemctl poweroff`. **D11's rule is unchanged** (D57);
**the lease horizon adds one rule ahead of its CPU test.** What this section gives is the check as
E0.08-T01-S05 builds `box/system/agentspace-idle-check`; a detail that no decision or pin fixes was
confirmed as written by the architect, 2026-11-21, for v0.08-T01, S01 batch, and is cited so.

**The horizon file is `/run/agentspace/lease-horizon`** (D63). The guard writes it to every running
host at every run, with one `ssm send-command` whose comment is `agentspace guard: lease horizon`
([guard](#guard)), and the host keeps what the last send wrote. **It holds one line: the horizon,
as a decimal count of Unix seconds, and one LF**, root-owned, mode 0644. The horizon is what the
guard sends: the later of the project's horizon and the host's launch time ([guard](#guard), step
7; [state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon)).
`/run` is cleared at every boot, so a host just started has no horizon until the next guard run.

**A horizon the check can use is the file's first line, when it is 1 to 15 decimal digits and
nothing else** (as built). **Anything else counts as no horizon file, and is ignored without a
word**: a missing file, one it cannot read, a folder at the path, an empty first line, a sign, a
space, a CR, or 16 digits or more. Lines after the first are not read.

**The settings are read from `/etc/agentspace/idle.conf`**, as shell assignments, when it is
readable. Each has a default in the check itself, used when the file is missing or does not set it
(architect, 2026-11-21, for v0.08-T01, S01 batch):

| Setting | In `idle.conf` | Default in the check | Means |
|---|---|---|---|
| `IDLE_MINUTES` | `60` | `60` | Minutes with no session, no patch run and no CPU work before D11's power-off |
| `BUSY_CPU_SECONDS` | `6` | `6` | CPU-seconds of work since the last check that count as activity |
| `HORIZON_GRACE_MINUTES` | `15` | `15` | Minutes past the horizon before the horizon's power-off; D56's grace, a constant that `sync` installs, never a parameter of any command (D63) |

**A `HORIZON_GRACE_MINUTES` that is not 1 to 6 decimal digits** is replaced by 15, and the check
prints `Invalid HORIZON_GRACE_MINUTES in <conf>; using 15` to stderr, where `<conf>` is the
settings file's path. It is read, and the line printed, only when there is a horizon the check can
use and no session or patch run holds the host (as built).

**Each run, in this order** (as built; the order is the architect's, 2026-11-21, for v0.08-T01,
S01 batch):
1. **Measures the CPU work since the last check**, from the busy fields of `/proc/stat`'s `cpu`
   line (user, nice, system, irq, softirq and steal), and records the counter for the next run, as
   before.
2. **Looks for a session**: an SSH session open (an established TCP connection on port 22), or a
   Session Manager session open (a running `ssm-session-worker`). **With none, looks for a patch
   run under way** (`systemctl is-active --quiet agentspace-patch.service`, the test D11's rule
   uses).
3. **The horizon rule** (D57): with no session and no patch run, when there is a horizon the check
   can use and the time now, in Unix seconds, is at or after `<h>` plus 60 times `<g>`, where `<h>` is the
   horizon and `<g>` the grace (at or after is the architect's, 2026-11-21, for v0.08-T01, S01
   batch), it prints

   ```
   Past lease horizon <h:mm><am|pm> plus <g> min with no session; powering off
   ```

   and powers the host off (P6, architect, 2026-11-21, for v0.08-T01). `<h:mm><am|pm>` is the
   horizon in the host's own zone, as `date -d @<h> +%-I:%M%P` gives it (D32: the host's zone is
   the laptop's), and `@<h>`, the horizon's seconds, when `date` cannot format it. **Its CPU does
   not stop it** (D57). **A session or a patch run holds it off** (P9, architect, 2026-11-21, for
   v0.08-T01): a power-off in the middle of an upgrade can leave dpkg half configured, and the
   patch unit's own 20-minute limit (`TimeoutStartSec=20min`) bounds the hold.
4. **D11's rule, unchanged**, for every other case: a session, a patch run under way, or at least
   `BUSY_CPU_SECONDS` of CPU work since the last check, in that order, is activity: it records the
   time as the host's last activity and prints `Active: SSH session open`,
   `Active: Session Manager session open`, `Active: Patching` or
   `Active: CPU work: <n> s since last check`. Otherwise, at `IDLE_MINUTES` or more since the last
   activity, it prints `Idle for <n> min (limit <limit>); powering off` and powers the host off;
   below that, `Idle for <n> min of <limit> (CPU <n> s since last check)`. The last activity starts
   as the first run's time each boot, since `/run` is cleared.

**So a host left with work running on it, with the laptop away, stops no later than its horizon,
the grace and one 5-minute check after its last session and patch run end** (D57); an idle one
stops after 60 minutes, as before. **The check writes nothing to the horizon file**, and changes
nothing on the host but its own two state files, `last-active` and `busy`, and the power-off.

**Its seams, for `box/tests` alone** (D63). Three environment variables replace three paths, so a
test can run the check on any machine against files of its own:

| Variable | Replaces | Default |
|---|---|---|
| `AGENTSPACE_IDLE_STATE` | The state folder, which holds `last-active`, `busy` and `lease-horizon` | `/run/agentspace` |
| `AGENTSPACE_IDLE_CONF` | The settings file | `/etc/agentspace/idle.conf` |
| `AGENTSPACE_PROC_STAT` | The CPU counters' file | `/proc/stat` |

- **No unit, script or file that `sync` installs sets any of them**, and a host MUST run the check
  with all three unset. Unset or empty, each is its default (architect, 2026-11-21, for v0.08-T01, S01 batch).
- **The programs the check starts, `ss`, `pgrep`, `systemctl`, `getconf` and `date` among them, are
  found on `PATH`**, so a test puts stand-ins first on it, `systemctl poweroff` among them; no seam
  variable names them (architect, 2026-11-21, for v0.08-T01, S01 batch).

## The AWS side

**Nothing on this page names an account ID, a VPC, a subnet or an instance ID.** Account-specific
values live in the stack parameters file, `%USERPROFILE%\.agentspace\stack-parameters.json`, which is
never committed (D37; docs/state.md).

### The stack

**One CloudFormation stack per host, in `us-west-2`, every one from the one template** (D44).
- **The box's stack is named `agentbox`.** It keeps the project's first name, because a stack
  cannot be renamed without replacing the instance and its disk. The operator MUST use that name
  for the box.
- **Every other host's stack is named `agentspace-host-<project>`**, the name its record holds
  (D44, D55). The project is a normalized lease project ([lease-api.md,
  Project](lease-api.md#project)), so the name always starts with a letter, as CloudFormation
  requires, and is at most 48 characters.
- **Each stack holds its own role, instance profile, security group, instance and alarm**, and
  nothing is shared between stacks, so deleting one host's stack touches no other (D44). No
  operator command deletes a stack.

**Parameters:**

| Parameter | Type | Default | Set by | Notes |
|---|---|---|---|---|
| `InstanceType` | String | `m9g.xlarge` | The template; for every host, the box included, `deploy -InstanceType <type>` alone, with or without `-Host`, which a provisioning run gives its deploy step from its project's `pending` leases (D67, D68, D71). Never the stack parameters file, from v0.09: until then it set the box's (P13, architect, 2026-11-20, for v0.07-T01) | Changing it on an update stops and restarts the instance; the disk is kept. A new host starts at the default unless `-InstanceType` is given, never at the box's value (D55). Read back after every deploy into the host's record as `instance_type` (D67). The default is `m`'s type in the EC2 driver's map (D65), so a host made with no type meets the default size. |
| `ImageId` | EC2 image ID | A pinned Ubuntu 26.04 LTS arm64 image | The template | MUST NOT be changed on a live stack: it replaces the instance and destroys its disk. |
| `SubnetId` | Subnet ID | None | The stack parameters file, for the box and for a stack `deploy` creates | A subnet that assigns a public IPv4 address at launch, as a default VPC's do. |
| `VpcId` | VPC ID | None | The stack parameters file, for the box and for a stack `deploy` creates | The subnet's VPC. |
| `RootVolumeGiB` | Number, at least 8 | `8` | The template | The image's minimum. Grown later with `grow`. |
| `SshPublicKey` | String | None | `deploy`, from `%USERPROFILE%\.ssh\id_ed25519.pub`: at every deploy of the box, and when it creates another host's stack (P13) | Installed for the login user at first boot only ([Q13](#open-questions)). |
| `HostName` | String | `agentbox` | `deploy`, when it creates a host's stack that is not the box's: the stack's name | The alarm's name and the `Name` and `project` tags. Its default reproduces every name the `agentbox` stack had, and the box's `deploy` passes none, so its change set is empty (D44; P5, architect, 2026-11-20, for v0.07-T01). |
| `CaPublicKey` | String | Empty | `deploy`, when it creates a host's stack that is not the box's: the CA's public key, type and base64 only ([deploy](#deploy), step 3) | When not empty (the condition `HasCaPublicKey`), the first-boot user data writes it to `/etc/ssh/agentspace_ca.pub`, followed by one LF, with its sshd drop-in, so a new host trusts the CA before the operator first connects. The box passes none, so its first boot's user data is unchanged (D44; P5, P8). A public key, which MAY be seen in the stack's parameters. |

**A host's stack that exists, the box's apart, is updated with every parameter's previous value**
(`UsePreviousValue`), but `InstanceType` when `deploy -Host <project> -InstanceType <type>` gives
one (P13, architect, 2026-11-20, for v0.07-T01; [deploy](#deploy), step 4). **The box's stack is
updated with `SshPublicKey` and every member of the stack parameters file but `InstanceType`**,
and every other parameter at its previous value, `InstanceType` included, but when
`deploy -InstanceType <type>` gives one (D67, D71; architect, 2026-11-23, for v0.09-T01, pin P6).

**Resources:**
- **An IAM role** for EC2 with the AWS managed policy `AmazonSSMManagedInstanceCore`, and an
  instance profile for it. That is what lets Session Manager reach the box.
- **A security group with no inbound rules** and all outbound traffic allowed. Nothing reaches the
  box but Session Manager.
- **The instance**, in the subnet, with the role, the security group, shutdown behavior `stop`,
  and an encrypted `gp3` root volume of `RootVolumeGiB` that is deleted with the instance. Its
  first-boot user data updates and upgrades packages, installs `git`, `tmux`, `jq`, `ripgrep` and
  `unzip`, adds `SshPublicKey` to the login user's authorized keys, and, when `CaPublicKey` is
  not empty, installs it with its sshd drop-in. Nothing in it runs again after the first boot.
- **A CloudWatch alarm**, `<HostName>-idle-backstop` (`agentbox-idle-backstop` for the box), on the
  instance's average `CPUUtilization`: under 1% for 24 periods of one hour (24 hours), `Period`
  3600 and `EvaluationPeriods` 24, with missing data counted as not breaching, and the action of
  stopping the instance (D58, D63). Its description is
  `Stops ${HostName} after 24 hours averaging under 1% CPU.`, with the stack's `HostName` in place
  of `${HostName}`. **It is the backstop for a host whose idle check has died, idle, with the
  laptop away** ([Idle stop, as the laptop sees it](#idle-stop-as-the-laptop-sees-it)). Until
  v0.08 it was 36 periods of 5 minutes (3 hours), which stopped a host under a quiet held lease
  (#0043, OBE once v0.08 is live). **The change is in place**: each stack's next update modifies
  the alarm and nothing else, so `agentbox`'s change set holds that one modification, read live
  before the template's merge (D58), and a host's stack takes it at its next `deploy -Host` or
  `provision`, its parameters kept.

**Tags:** the stack is deployed with the tag `project=<stack>`: `project=agentbox` for the box, as
before, and `project=agentspace-host-<project>` for another host (architect, 2026-11-20, for v0.07-T01: D44 names
the instance's tags, not the stack's). The instance carries `Name` and `project`, both `HostName`'s
value; the role carries `project`; the security group carries `Name`.

**Output:** `InstanceId`, the instance's ID. The operator reads it into its host's record in the
host registry (D45).

**What the laptop's AWS identity needs:** rights to deploy and describe the stack and create its
IAM role (`CAPABILITY_IAM`) and pass it to the instance (`iam:PassRole`), to create and change the
stack's CloudWatch alarm, to describe, start and stop the instance, to describe and modify its
volume, and to use Session Manager (describe instance information, send a command, read its
invocation, start a session). The operator reads no credentials itself: every call uses whatever
the AWS CLI is signed in with. Today that is an administrator signed in with `aws login`, which
gives short-lived credentials, whose expiry is #0034's case ([AWS failures](#aws-failures)).
**A host adds no new permission** (D44): every call a host needs is one the box's commands already
make, against its own stack and instance, and this list gains no service. **Nor does the guard**:
it describes and stops instances and sends a Session Manager command, each of which `status`,
`down` and `up` already do ([guard](#guard)). **Nor does a size** (D67): a resize is an update of
the stack's `InstanceType`, which `deploy -Host <project> -InstanceType <type>` already made, and
recording the type reads the `cloudformation describe-stacks` answer `deploy` already gets.

### Calls by command

**Every call passes `--region us-west-2`; the ones marked JSON also pass `--output json`.** Of
those, the output is read as [What is read from each answer](#calls-by-command) below says, and
the rest are checked by their exit code alone. **Every call is per host:** `<stack>` is the
command's host's stack (`agentbox` for the box) and `<id>` the instance ID in that host's record
([Which host a command acts on](#which-host-a-command-acts-on)). A command without `-Host` makes
exactly the calls it made before v0.07, `<stack>` being `agentbox`, but for the folder `deploy`'s
template is written in, for what v0.08 adds: `sync`'s patch wait, and `grow`'s refusal of a
`-GiB` of 0 or less before any call, and for what v0.09 changes in the box's `deploy`: the
parameter overrides no longer carry the stack parameters file's `InstanceType`, and carry
`InstanceType=<type>` last when `-InstanceType` is given (D67, D71; architect, 2026-11-23, for
v0.09-T01, pins P6 and P32). **v0.09 adds no call to any command**: every `deploy` already reads
`cloudformation describe-stacks` after `cloudformation deploy`, and reads the instance type from
that answer (architect, 2026-11-23, for v0.09-T01, pin P23), and `provision`'s reading of the
lease state file, under its lock, is a file read.

| Command | AWS CLI calls, in order |
|---|---|
| Any but `ca`, and the ProxyCommand, when the host's record holds no instance ID | `cloudformation describe-stacks --stack-name <stack>` (JSON) |
| `deploy`, the box | `cloudformation deploy --stack-name <stack> --template-file %USERPROFILE%\.agentspace\operator\<hex>\agentspace.yaml --capabilities CAPABILITY_IAM --no-fail-on-empty-changeset --tags project=<stack> --parameter-overrides SshPublicKey=<key> <each Name=value line>`, as before v0.09, but with the file's `InstanceType` never among them, then `InstanceType=<type>` last when `-InstanceType` is given (D67, D71; pin P6; its place, last, as v0.07 placed it for a stack `deploy -Host` creates, is architect, 2026-11-23, for v0.09-T01, pin P32) (output passed through); then `cloudformation describe-stacks --stack-name <stack>` (JSON), read for `InstanceId` and `InstanceType`; then `sync`'s. As before v0.07, but for the template's folder ([The operator's folder, one per command](#the-operators-folder-one-per-command)) and v0.09's overrides. **None at all** when the box's `provision.lock` is held or cannot be taken, which `deploy` finds before any call (architect, 2026-11-23, for v0.09-T01, pin P34) |
| `deploy -Host`, a host that is not the box | `cloudformation describe-stacks --stack-name <stack>` (JSON), the existence check ([deploy](#deploy)), and nothing more when its `StackStatus` fails it; then `cloudformation deploy` with the same arguments as the box's up to `--tags project=<stack>`, and then: creating the stack, `--parameter-overrides SshPublicKey=<key> SubnetId=<subnet> VpcId=<vpc> HostName=<stack> CaPublicKey=<CA key>`, with `InstanceType=<type>` last when `-InstanceType` is given, each `Name=value` one argument, `CaPublicKey=<CA key>` included though the key holds a space; updating it, no `--parameter-overrides` at all, or `--parameter-overrides InstanceType=<type>` when `-InstanceType` is given (output passed through); then `cloudformation describe-stacks --stack-name <stack>` (JSON), read for `InstanceId` and `InstanceType`; then `sync`'s. The order of the overrides is confirmed by the architect, 2026-11-20, for v0.07-T01. A failure before `cloudformation deploy`, deploy's step 4's removal from the record included, makes no later call. **None at all** when the host's `provision.lock` is held or cannot be taken, which `deploy` finds before the existence check (architect, 2026-11-23, for v0.09-T01, pin P34) |
| `bootstrap` | The start and wait's below, then nothing more from AWS: the bootstrap goes over SSH, whose nested `up` makes its own calls and a tunnel |
| `provision` | `deploy -Host`'s, or the box's `deploy`'s for `-Host swimcoach`, without its `sync`, with `-InstanceType <type>` when its step 4 found a type and without it otherwise (D68, D71); then the start and wait's; then `sync`'s, its patch wait included, and `bootstrap`'s. Its first and last steps, the registry and `place_pending`, make none (D48), nor does its reading of the lease state file, under the lease state lock it releases before its first call (pin P26); a lease state lock not taken, or a lease state file that cannot be read or is not valid, fails the deploy step before any call |
| `up`, `sync`: [start and wait](#starting-the-box-and-waiting-for-it) | `ec2 describe-instances --instance-ids <id>` (JSON); if stopped, `ec2 start-instances --instance-ids <id>` (JSON); if stopping, `ec2 wait instance-stopped --instance-ids <id>`, then `ec2 start-instances`; `ec2 describe-instances` again, for the launch time, only if it was `running` (FB-09); while waiting, every 5 seconds, `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON), and once that is `Online`, `ssm send-command --instance-ids <id> --document-name AWS-RunShellScript --comment "agentspace up: patch check" --parameters commands=<question>` (JSON) and up to 20 of `ssm get-command-invocation --command-id <command id> --instance-id <id>` (JSON) |
| `sync` | The above; then, after its install, [the patch wait](#the-patch-wait)'s: every 5 seconds `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON), and while that is `Online`, `ssm send-command --instance-ids <id> --document-name AWS-RunShellScript --comment "agentspace sync: patch check" --parameters commands=<question>` (JSON) and up to 20 of `ssm get-command-invocation --command-id <command id> --instance-id <id>` (JSON), until the answer is `patched`, or `failed`, after which no other check is made (P21). No `ec2 describe-instances` of its own: the wait never reads the launch time. The rest goes over SSH, and each ssh or scp makes its own nested `up`'s calls and a tunnel |
| `down` | `ec2 stop-instances --instance-ids <id>` (JSON) |
| `status` | `ec2 describe-instances --instance-ids <id>` (JSON); `ec2 describe-volumes --volume-ids <vol>` (JSON); if running, `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON) |
| `status -All`, `down -All`, `sync -All` | The single command's calls for each host in turn, in the registry's order, none for a host it skips ([Every host at once](#every-host-at-once--all)). One call never names two instances. `status -All` makes no other call: a host's launch time is read from the `ec2 describe-instances` answer its report already reads ([status](#status)). Its calls are the same when the lease state file is missing, cannot be read or is not valid (P18, architect, 2026-11-21, for v0.08-T01) |
| `grow` | `ec2 describe-instances --instance-ids <id>` (JSON); `ec2 describe-volumes --volume-ids <vol>` (JSON); `ec2 modify-volume --volume-id <vol> --size <new>` (JSON). None when `-GiB` is 0 or less, which is refused first ([grow](#grow)) |
| `guard` | For each `ready` record with no instance ID, in the registry's order, `cloudformation describe-stacks --stack-name <stack>` (JSON); then, when any host has an instance ID, one `ec2 describe-instances --filters Name=instance-id,Values=<id>,<id>,...` (JSON), the IDs in the registry's order joined by commas, `Name=instance-id,Values=...` one argument (architect, 2026-11-21, for v0.08-T01, S01 batch); then, for each due host whose provisioning lock is free, in the registry's order, `ec2 stop-instances --instance-ids <id>` (JSON); then, for each running host the run did not stop, in the registry's order, `ssm send-command --instance-ids <id> --document-name AWS-RunShellScript --comment "agentspace guard: lease horizon" --parameters commands=<command>` (JSON). No `ssm get-command-invocation`. One call never stops or sends to two instances ([guard](#guard)). **No call at all** when `guard.lock` is held or cannot be taken, when the host registry cannot be read or is not valid, or when the lease state file is missing, cannot be read or is not valid (steps 1 to 3; P18, architect, 2026-11-21, for v0.08-T01) |
| `ca` | None |
| The ProxyCommand | `up`'s, then `ssm start-session --target <id> --document-name AWS-StartSSHSession --parameters portNumber=22`, the tunnel |

**Which failures are tolerated:** a failed `ssm get-command-invocation` is polled again, since the
invocation is often not readable for a moment after `send-command`; `ec2 wait instance-stopped`'s
exit code is not checked, since the start request after it is the check
([Starting the box](#starting-the-box-and-waiting-for-it)); in `sync`'s patch wait alone, a failed
`ssm send-command` is a check that was not ready, unless it is #0034's sign-in failure
([The patch wait](#the-patch-wait)); and `guard`'s failed `cloudformation describe-stacks`,
`ec2 stop-instances` or `ssm send-command` fails that host's part, and the run goes on to the next
host and exits 1 at its end, unless it is #0034's sign-in failure, which ends it at once
([guard](#guard)). Every other failed call fails the command, under [AWS failures](#aws-failures).

**What is read from each answer** (B-32). Members are named by their path in the JSON the AWS CLI
prints; `[0]` is the first element of an array. Every other member is ignored, and the answers of
`ec2 start-instances`, `ec2 stop-instances` and `ec2 modify-volume` are not read beyond their exit
code.

| Call | Members read | Used for |
|---|---|---|
| `cloudformation describe-stacks` | `Stacks[0].Outputs`: the element whose `OutputKey` is `InstanceId`, its `OutputValue` | The host's record's instance ID. No such element is the "no `InstanceId` output" failure ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)) |
| `cloudformation describe-stacks`, as `deploy`'s read after `cloudformation deploy` succeeded, the box's and every host's | As the row above, and also `Stacks[0].Parameters`: the element whose `ParameterKey` is `InstanceType`, its `ParameterValue` (D67, D71) | The host's record's instance ID and its `instance_type` ([deploy](#deploy), step 5). No such element, two such elements, or a `ParameterValue` that is not a string of the host registry's form for `instance_type`, is the "no `InstanceType` parameter" failure, `agentspace: Stack '<stack>' has no InstanceType parameter.`, exit 1, checked after `InstanceId`, leaving the record with no type (architect, 2026-11-23, for v0.09-T01, pin P22). Every other element of `Parameters` is ignored, and a read for the instance ID alone, by a command that is not `deploy`, never reads `Parameters` |
| `cloudformation describe-stacks`, as `deploy -Host`'s existence check | `Stacks[0].StackStatus` alone | Whether `deploy` creates the stack, updates it or fails ([deploy](#deploy); P17, architect, 2026-11-20, for v0.07-T01). A missing or non-string `StackStatus` fails with `agentspace: Unexpected answer from cloudformation describe-stacks: no Stacks[0].StackStatus.` |
| `ec2 describe-instances` | `Reservations[0].Instances[0]`: `State.Name`, `LaunchTime` (ISO 8601, compared in UTC), `InstanceType`, `RootDeviceName`, and in `BlockDeviceMappings` the element whose `DeviceName` equals `RootDeviceName`, its `Ebs.VolumeId` | The state, the 20-minute rule, `status`'s type, the root volume's ID, and `status -All`'s stop time |
| `ec2 describe-instances`, as `guard`'s | Every element of `Reservations[].Instances[]`: `InstanceId`; and for the element whose `InstanceId` is a host's ID, `State.Name`, and for every `running` one, explained or not, `LaunchTime` (ISO 8601, compared in UTC) | Which hosts run, each running host's horizon sent (P12), and an unexplained host's since and stop time ([guard](#guard)) |
| `ssm send-command`, as `guard`'s | Nothing: its exit code alone | Whether the horizon was sent |
| `ec2 describe-volumes` | `Volumes[0].Size` (GiB) | `status`'s size, and `grow`'s `<size>` |
| `ssm describe-instance-information` | `InstanceInformationList[0].PingStatus`; online only when it is `Online`, and an empty list is not online | The wait, and `status`'s `<ssm>` |
| `ssm send-command` | `Command.CommandId` | The poll |
| `ssm get-command-invocation` | `Status`, and once it is not `Pending`, `InProgress` or `Delayed`, `StandardOutputContent`, trimmed | The patch check's answer ([What up waits for](#what-up-waits-for)) |

(The three `ssm` rows were split off below the rulings by an edit on 2026-11-19 and put back by
the architect the same afternoon, found in E0.06-T01-S03's review.) **A member's path in a message
is the whole path as this table writes it**, such as `InstanceInformationList[0].PingStatus`.

**Which command reads which of `ec2 describe-instances`'s members** (architect, 2026-11-19, for
v0.06, gap FB-08). Each reads only its own, and a member it does not read MUST NOT fail it, whether
missing or of another form:

| Reader | Members it reads |
|---|---|
| The start-and-wait step (`up`, `sync`, `bootstrap`, `provision`, the ProxyCommand's `up`) | `State.Name` in its first call; `LaunchTime` in its second, which runs only for a `running` box (FB-09) |
| `status` | `State.Name`, `InstanceType`, `RootDeviceName`, and the root volume's `Ebs.VolumeId`; and with `-All`, `LaunchTime`, only for a running host whose project holds no live lease, and never when the lease state file is missing, cannot be read or is not valid ([status](#status); P18) |
| `grow` | `State.Name`, `RootDeviceName`, and the root volume's `Ebs.VolumeId` |
| `guard` | `InstanceId` of every element; `State.Name`; `LaunchTime` for every `running` instance, explained or not, and for no other |

A member a command reads that is missing, or not of the form above, fails the command with
`agentspace: Unexpected answer from ec2 describe-instances: no <member>.`, where `<member>` is its
path as this table writes it, such as `Reservations[0].Instances[0].LaunchTime`. **For `guard`, and
for `status -All`, it fails that host's part alone, never the run**: `guard`'s message is
`Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].<member>.`,
logged with `step=instance` ([guard](#guard), step 6); a running host's missing `LaunchTime` fails
it whether or not a live lease explains the host. When no element
of `BlockDeviceMappings` has the `RootDeviceName`, or it has no `Ebs.VolumeId`, `<member>` is
`Reservations[0].Instances[0].BlockDeviceMappings[].Ebs.VolumeId` (architect, 2026-11-19, for
E0.06-T03-S01).

**The same form holds for every call's members, with these exceptions** (architect, 2026-11-19,
for E0.06-T01-S03):
- **`cloudformation describe-stacks`, read for the instance ID:** a missing `Stacks[0]` or
  `Outputs`, or an `InstanceId` output whose value is not a string matching the host registry's
  schema for an instance ID (before v0.07, the instance-ID cache's), is the "no `InstanceId`
  output" failure, and no instance ID is written.
- **`cloudformation describe-stacks`, read by `deploy` for the instance type** (D67, D71): a
  missing or non-array `Parameters`, no element whose `ParameterKey` is `InstanceType`, or one
  whose `ParameterValue` is missing, not a string, or not of the host registry's form for
  `instance_type`, is the "no `InstanceType` parameter" failure, and the record is left with no
  `instance_type` ([deploy](#deploy), step 5). An element whose `ParameterKey` is not a string is
  passed over. Two elements whose `ParameterKey` is `InstanceType` are the same failure, since the
  answer could mean two things (architect, 2026-11-23, for v0.09-T01, pin P22).
- **`ssm get-command-invocation`:** an answer with no string `Status` is not readable yet, and is
  polled again; one that has ended with no `StandardOutputContent` is the empty answer, which
  counts as ready ([Q6](#open-questions)).
- **`LaunchTime` with no UTC offset** is a member not of its form. A launch time in the future is
  less than 20 minutes ago, so the box is waited for.
- **The waits' timing:** the 5 seconds are a pause after each check; the 15-minute deadline is
  tested after each check that was not ready, and the last pause is cut short to end on it; the
  invocation is polled at once and then each second, at most 20 times. `sync`'s patch wait keeps
  the same timing with its 30-minute deadline ([The patch wait](#the-patch-wait)).
- **`sync`'s patch wait reads the same members as `up`'s wait**, and an invocation that has ended
  with no `StandardOutputContent` is the empty answer, which in this wait alone is not ready
  ([The patch wait](#the-patch-wait)). The answer `failed` is the trimmed `StandardOutputContent`
  of an invocation that ended in `Success`, as `patched` is; in an invocation that ended in any
  other status it is a check that was not ready.

## Requirements the prototype does not meet yet

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository, so this table
is history.** It records what the rewrite was built to do differently, and no conformance check
runs against the prototype any more. The requirement in each row's first column still binds
the programs that are built now.

**Behavior this page requires that the prototype does not show, so that a conformance check run
against the prototype is expected to fail here.** **The rewrite MUST NOT copy the prototype's
behavior in this table.**

| Requirement | The prototype | Card or question |
|---|---|---|
| Every ssh and scp by full path | Only the push's ssh and the CA key's scp are | #0027 |
| An expired sign-in prints agentspace's message, and other AWS failures the CLI's text | A raw runtime error, under Windows PowerShell 5.1 | #0034 |
| An expired sign-in during `deploy` prints agentspace's message | The CLI's text, then `Stack deploy failed; see the stack events in the console.` | #0034 |
| `deploy` exits 1 when its `sync` step fails | Most likely exits 0 | [Q3](#open-questions) |
| A missing or empty `id_ed25519.pub` fails with an `agentspace: ` message | A runtime error; a whitespace-only file passes an empty key on | [Exit codes](#exit-codes) |
| An empty instance-ID cache counts as missing, and a stack with no `InstanceId` output fails. From v0.07 the cache is retired (D45): a stack with no `InstanceId` output still fails and writes nothing, and a host registry that is not valid fails, never counting as missing ([The host registry, as the operator uses it](#the-host-registry-as-the-operator-uses-it)) | Writes an empty cache, after which every command fails with a runtime error until `deploy` | [Q24](#open-questions) |
| `sync` with `ssh-keygen.exe` missing fails with `ca`'s step 1 message | No check; another error | [sync](#sync) |
| A program that needs no input gets an empty or closed stdin | The AWS CLI inherits `up`'s stdin, the SSH client's stream | [Streams](#streams) |
| An empty or missing URL or token, or one with a CR, LF or NUL, warns "Could not read" and sends nothing | Sends it. The box refuses an empty value, so the laptop warns "did not confirm"; with an LF the box installs the first two lines, a cut token, and the laptop warns "did not confirm"; a CR is removed by the box and the push is confirmed | [What the laptop reads](#what-the-laptop-reads) |
| `LOCALAPPDATA` unset or empty skips the push as a missing receipt does, and fails no command | Every command, `ca` included, most likely fails at start with its runtime's error: building the receipt's path from an unset or empty variable throws, and the script stops on any error | [The receipt](#the-receipt) |
| A receipt that could mean two things, or holds the wrong type, is refused: a JSON value that is not an object, `base` or `credential_directory` given twice under the same name, or a `base` or `credential_directory` that is not a string | Reads a one-element array as its element, takes the last of a member repeated under the same name, sends a number's text as the URL and an array's items joined by spaces, and reads an array of one string as the credential folder | [The receipt](#the-receipt); D20 |
| A receipt whose `base` is not exactly `http://127.0.0.1:PORT`, with `PORT` a decimal port from 1 to 65535 with no leading zero, is refused | Sends any string it reads as the URL | [The receipt](#the-receipt); [Q9](#open-questions), closed |
| A receipt with no byte order mark is read as UTF-8, and every path is taken literally | Reads it in the system's ANSI code page, so non-ASCII text in it differs; takes `*`, `?`, `[` and `]` in the receipt's path and the credential folder as wildcards | [The receipt](#the-receipt), [The Bot credential](#the-bot-credential) |
| A receipt with bytes not valid in its encoding, or with text that is not JSON (a single-quoted string, an unquoted member name), is refused | Replaces each bad byte with U+FFFD and reads on; accepts single-quoted strings and unquoted member names | [The receipt](#the-receipt); D20 |
| A Bot credential that could mean two things is refused: in `bot.json`, a member given twice or two names that differ only in ASCII case; so is plaintext that is not valid UTF-16 | Read the earlier XML credential file, which the push no longer reads: it read every credential object and sent their tokens joined by a space, took the last of a repeated password, and accepted an unpaired surrogate in the token | [The Bot credential](#the-bot-credential); D20 |
| An empty or unreadable stamp makes the push due | Warns "Could not send" and pushes nothing, at every `up` until a `sync` | [The stamp](#the-stamp) |
| The push's 20 seconds cover the write to ssh's stdin | Starts the clock after the write; the content fits in a pipe's buffer, so a blocked write has not been seen | [What the laptop sends](#what-the-laptop-sends) |
| The AWS CLI, `ssh` and `scp` can be named by `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD`, and the push's zone by `AGENTSPACE_TIMEZONE`; a name that cannot be run fails, and never falls back | Reads none of them. It runs the AWS CLI and `ssh` by fixed paths, and its zone override can be reached only by loading its code | [The test seam](#the-test-seam); Q11 and Q12, closed |
| The ProxyCommand runs `up` in caller mode: the caller's progress lines and failure messages, nothing else on success, the tunnel's stderr withheld; backend detail only with `AGENTSPACE_DIAGNOSTIC`. A nested `up` prints `The host is ready.`, never `Ready: <id>` | Has no caller mode. Prints the instance ID (`Starting <id>...`, `Ready: <id>`, nested or not), EC2 states, Session Manager and patch messages, the push's progress line, and on a failure the AWS CLI's text inside a runtime's error record; passes the tunnel's stderr through | #0087; D1 ([What a caller sees](#what-a-caller-sees-d1-card-0087)) |
| The instance-ID cache is `%USERPROFILE%\.agentspace\instance-id.json`, in JSON (D40); from v0.07, a member of each host's record in `hosts.json` (D45) | `.instance-id` in the checkout, the ID alone | D21; Q7, closed ([Laptop files](#laptop-files)) |
| Every usage error prints this page's `agentspace: ` message, and nothing prompts | Its shell's parameter binding answers each one with its own error record (exit 1, as required), which names the script's full path. With stdin closed it does not prompt for a missing command; at a console it was not run | [Q1](#open-questions), closed ([Command line](#command-line)) |
| `deploy -Quiet` quiets its `sync` step | Runs `sync` without `-Quiet`, so `sync`'s messages print | D36; [Q4](#open-questions), closed ([deploy](#deploy)) |
| `sync` fails when the box's staging folder cannot be removed, before the copy | Does not check the removal. A partly removed folder takes the copy one level down, and the install then runs whatever install script the removal left, so `sync` can succeed having installed the previous `box/system` | D36; [Q25](#open-questions), closed ([sync](#sync)) |
| `grow` fails when the online filesystem step fails | Ignores the step's exit code and exits 0 | D36; [Q23](#open-questions), closed ([grow](#grow)) |
| A tunnel whose AWS CLI is missing fails with the caller's message and exit 1 | Its shell's own error, which names the path, and that shell's exit code | B-33 ([The ProxyCommand](#the-proxycommand), step 6) |
| Help at every level | None of its own | [Q2](#open-questions) |
| `ca` refuses a CA folder inside any git working tree | Refuses one inside its own checkout only | D37 ([What the prototype took from its checkout](#what-the-prototype-took-from-its-checkout)) |
| Nothing the operator, the ProxyCommand or the lease CLI reads at run time is in a checkout | Reads the template, the box's files, `infra\local.parameters`, the schemas and the instance-ID cache from its checkout, and writes its build of the localswim CLI there | D37 ([What the prototype took from its checkout](#what-the-prototype-took-from-its-checkout)) |
| The box's install names `agentspace-operator sync` when the CA's public key is missing | Names `scripts\agentspace.ps1 sync` | [What sync installs](#what-sync-installs) |

## Open questions

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository.** A question
below that asks what the prototype does is moot, since nothing can run it to find out, and its
answer was never a requirement of the rewrite.

**What reading the code and the docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080) and its fixes in v0.04-T05, every question here is either closed or
marked "Later, not blocking" with its reason, and none blocks building a drop-in of contract v0.
v0.09's Q36 is closed (architect, 2026-11-23, for v0.09-T01, pin P26). Several need a run on the laptop, which no writer of this page could make (D34).

| | Question | Settled by |
|---|---|---|
| **Q1** | **Closed (E0.04-T05-S03, 2026-11-18).** Each usage error has its own `agentspace: ` message and exits 1, nothing prompts, and the order of the checks is given, in [Command line](#command-line). Exit 1 rather than the lease CLI's 2, because every usage error the prototype reports exits 1, its shell's binding errors and its `-Alias` refusals alike, and the conformance checks expect 1. The prototype's binding errors are its shell's own records, which [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists. | Closed |
| **Q2** | **Later, not blocking:** **Does `powershell -File agentspace.ps1 -?` print the script's comment-based help?** If it does, that is the prototype's only help, and the rewrite's texts above replace it. | The help texts the rewrite MUST print are on this page, so the prototype's own help is an observation, not a behavior to build. Later: a run on the laptop |
| **Q3** | **Later, not blocking:** **What exit code does the prototype's `deploy` give when its `sync` step fails?** It runs `sync` as a nested script in the same process. Two readings disagree: that the nested script's `exit 1` ends only the nested script, so `deploy` ends normally and a `-File` run exits 0; or that it ends the whole process with 1. | [deploy](#deploy) requires exit 1 either way, so only what the prototype does is open. Later: a run on the laptop |
| **Q4** | **Closed by D36 (architect, 2026-11-18): `deploy -Quiet` quiets its `sync` step** ([deploy](#deploy), step 6), as the `-Quiet` row of [Command line](#command-line) says of every command. The prototype runs `sync` without `-Quiet`, which [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists. | Closed, D36 |
| **Q5** | **Later, not blocking:** **Does #0034's mechanism also hit `ssm get-command-invocation` and `ec2 wait`?** The poll discards the CLI's stderr while every error is terminating; under Windows PowerShell 5.1 a stderr line from it (such as an invocation not yet registered) may throw and fail `up` with a raw error instead of polling again. | The behavior to build is stated: a failed poll is polled again, and the wait's exit code is not checked ([Calls by command](#calls-by-command)), so only what the prototype does is open. Later: a run on the laptop |
| **Q6** | **Later, not blocking:** **Is an empty or unexpected answer to the patch check meant to count as ready?** The prototype ends the wait on anything but `pending`, so a check that fails lets `up` print `Ready` before the patch run has finished, against the rule of [Starting the box](#starting-the-box-and-waiting-for-it). | That section states the behavior to build, today's, and names this as one of its exceptions, so a stricter wait would be a later change. Later: the architect |
| **Q7** | **Closed by D21 (architect, 2026-11-18).** The instance-ID cache is laptop state, so it lives under `%USERPROFILE%\.agentspace` ([Laptop files](#laptop-files)); the prototype keeps it in the checkout, which [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists. The localswim CLI's build output (`.build\`) is not state, and MAY stay in the checkout. D45 (v0.07) retired the cache: each host's instance ID is now a member of its record in `%USERPROFILE%\.agentspace\hosts.json`. | Closed |
| **Q8** | **Closed by [docs/state.md](state.md#files-agentspace-reads-and-never-writes) (E0.04-T02-S01).** state.md lists the files the operator reads and agentspace never writes (localswim's receipt and `bot.json`, the user's `id_ed25519.pub`, `CLAUDE.md`, `~/.ssh/config`'s `agentspace` entry), and `infra\local.parameters` in [Files in the checkout](state.md#files-in-the-checkout); it owns none of their formats. | Closed |
| **Q9** | **Closed, both halves, by E0.07-T02-S05 (#0162; the architect's rulings for v0.07-T02, 2026-11-20).** ~~What characters can a localswim Bot token contain, and what characters can a localswim URL contain, and what form MUST it have?~~ The box's wrapper reads the credential file as shell assignments, so a space, quote or `$` in either value would break or change it ([The box's files](#the-boxs-files)). **The token:** the push refuses a token that is not exactly 43 characters of `A-Z`, `a-z`, `0-9`, `-` and `_`, as localswim's `docs/windows-operations.md`, "The credential files", makes every token ([The Bot credential](#the-bot-credential)). **The URL:** the push refuses a receipt whose `base` is not exactly `http://127.0.0.1:PORT`, with `PORT` a decimal port from 1 to 65535, no leading zero and no trailing slash, the form localswim's "The receipt" pins, checked in the push's own code ([The receipt](#the-receipt)). So no value the push sends holds a character the wrapper's shell reading would change. | Closed, both halves: E0.07-T02-S05 |
| **Q10** | **Closed by D37 (architect, 2026-11-18).** ~~What are the rewrite's entry points called, and does it keep PowerShell-style parameters (`-GiB`, `-Alias`, `-Quiet`)?~~ The operator is `agentspace-operator.exe` and the ProxyCommand `agentspace-ssh-proxy.exe`, installed with the lease CLI and the keeper in `%USERPROFILE%\.agentspace\bin` ([Programs and install layout](#programs-and-install-layout)). Every command line keeps the style this page gives ([Argument style](#argument-style)). The lease alias files on the laptop that name `<checkout>\scripts\ssh-proxy.cmd` keep working by [ProxyCommand lines already on the laptop](#proxycommand-lines-already-on-the-laptop). | Closed |
| **Q11** | **Closed by [The test seam](#the-test-seam).** `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD` name the programs a check stands in for, and a stand-in gets the token only as the real `ssh` does. | Closed |
| **Q12** | **Closed by [The test seam](#the-test-seam).** `AGENTSPACE_TIMEZONE` gives the push a zone. | Closed |
| **Q13** | **Later, not blocking:** **What does `deploy` do when `id_ed25519.pub` has changed?** The key is in the instance's user data, which runs at first boot only, so the new key is never installed; and CloudFormation may stop and start the instance to change user data. | `deploy` passes the key as [deploy](#deploy) says, and what CloudFormation then does is AWS's behavior, not the operator's. Later: a deploy on the laptop, or AWS's documentation |
| **Q14** | **Later, not blocking:** **Does the box require IMDSv2?** The template sets no metadata options, so whether the instance refuses a metadata request without a session token comes from the image's defaults, and is unobserved. | No operator command and no box step on this page reads instance metadata, so the answer changes nothing a drop-in builds. Later: a look at the instance's metadata options |
| **Q15** | **Later, not blocking:** **SHOULD the region and the stack name be configurable?** Both are fixed in the prototype, in two places each (the operator and the ProxyCommand). | This page fixes both ([The stack](#the-stack)), which is what a drop-in builds; making them configurable is an addition. From v0.07 the stack's name follows the host (D44), `agentbox` for the box and `agentspace-host-<project>` for another, and is still not configurable; the region stays fixed. Later: the architect |
| **Q16** | **Later, not blocking:** **SHOULD `status` read the volume's type**, rather than print `gp3` as a constant? | [status](#status) states the constant, and the stack makes only `gp3` volumes ([The stack](#the-stack)). Later: the architect |
| **Q17** | **Later, not blocking:** **Does the rewrite's ProxyCommand still need the console-mode repair?** The damage came from Windows PowerShell 5.1 starting under ssh. A ProxyCommand that never starts it might not need the repair, but this page requires the outcome either way. | [The ProxyCommand](#the-proxycommand) requires the console's state, not the repair, so a drop-in meets it either way. Later: a run on the laptop |
| **Q18** | **Later, not blocking:** **What does the prototype's runtime write ahead of the push's content?** The box strips a UTF-8 BOM, and the laptop's expected hash ignores one, but whether one is sent today depends on the console's input encoding. | The push works with or without a BOM ([What the box does](#what-the-box-does)), and the rewrite SHOULD send none, so only the prototype's bytes are open. Later: a run on the laptop, with the box's input logged |
| **Q19** | **Later, not blocking:** **Is a silent skip right when localswim's receipt is missing?** Every other reason the push does not happen warns; this one prints nothing, so a laptop without localswim never learns why the board does not work on the box. | [What the laptop reads](#what-the-laptop-reads) states the behavior to build, the skip, and a warning would be an addition. Later: the architect |
| **Q20** | **Later, not blocking:** **SHOULD concurrent pushes be serialized?** Two `up`s at once both push and both write the stamp, with no lock. | Both pushes write the same token, so nothing breaks, and each costs only a login ([When it runs](#when-it-runs)). From v0.07 each host has its own stamp (D50), so pushes to two hosts never share one. Later: the architect |
| **Q21** | **Later, not blocking:** **SHOULD `-Quiet` stop suppressing the push's warnings?** It suppresses them in the prototype, so a quiet `sync` hides a token that did not land. | The rule stands for contract v0: `-Quiet` suppresses them ([Messages](#messages); [What the laptop checks](#what-the-laptop-checks)), as the prototype does, and a drop-in builds that. The ProxyCommand never passes `-Quiet`, so a connection always shows them. Only whether a later contract changes the rule is open. Later: the architect |
| **Q22** | **Closed by D61 (#0164, architect, 2026-11-21; exit 1 by P7, architect, 2026-11-21, for v0.08-T01).** ~~SHOULD `-GiB` be checked before AWS is asked?~~ Yes, for `grow`: a `-GiB` of 0 or less is a usage error, exit 1, found after the host checks, and no AWS call is made ([grow](#grow)). Every other command still checks the form alone. | Closed, D61 |
| **Q23** | **Closed by D36 (architect, 2026-11-18): `grow` fails, exit 1, when the online filesystem step fails** ([grow](#grow), step 3), with a message saying that the volume's change stands and the filesystem grows at next boot. The prototype exits 0, which [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists. | Closed, D36 |
| **Q24** | **Later, not blocking:** **What exactly does the prototype print when the stack has no `InstanceId` output?** It writes an empty cache, and this page requires otherwise ([Laptop files](#laptop-files)). | The behavior to build is stated there, so only the prototype's text is open. Later: a run on the laptop |
| **Q25** | **Closed by D36 (architect, 2026-11-18): a failed removal of the box's staging folder fails `sync` before the copy** ([sync](#sync), step 3). Read from the code: the prototype does not check the removal, and a partly removed folder takes the copy one level down, so the install runs whatever install script the removal left, and `sync` can succeed having installed the previous `box/system`. [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists it. | Closed, D36 |
| **Q26** | **Later, not blocking:** **Does the sign-in test catch every form of the AWS CLI's expired-login error?** The prototype matches `expired`, `aws login` or `Unable to locate credentials`. The exact text of the CLI's error after an `aws login` session expires was not recorded with #0034. | [AWS failures](#aws-failures) states the test to build, and a form it misses still fails the command, with the CLI's own text. Later: a run on the laptop with an expired sign-in |
| **Q27** | **Later, not blocking:** **SHOULD the stamp stay an unsalted hash that covers the token?** It is kept on the laptop, and anyone who can read it can test a guessed token against it. A keyed digest would hide that, but the stamp MUST still change whenever the instance, the zone, the URL or the token does. Its storage is docs/state.md's. | [The stamp](#the-stamp) states the hash to build, and a later change of it costs one push per laptop, since a stamp that does not match only makes the push due. Later: the architect; docs/state.md |
| **Q28** | **Closed by [What the laptop reads](#what-the-laptop-reads) (E0.04-T05-S01, #0088, 2026-11-18).** It specified the receipt and the Bot credential to the byte, where localswim had not: where each is, its encoding, the receipt's two members, the credential's structure down to the token's hex, the `CryptUnprotectData` call, and the UTF-16LE plaintext, with what each failure does. Since localswim's PR #13 (`8d9d583`), localswim specifies both formats itself, with JSON Schemas, and the section cites them and states only agentspace's own checks (E0.07-T02-S05, #0162). | Closed |
| **Q29** | **Later, not blocking:** **Does D1 at connect time cover #0034's sign-in message?** The message names AWS and its login command ([AWS failures](#aws-failures)), and a lease's caller who sees it through the ProxyCommand learns the backend ([What a caller sees](#what-a-caller-sees-d1-card-0087)). Whether it MUST be reworded inside the ProxyCommand, with the AWS text behind `AGENTSPACE_DIAGNOSTIC`, is the architect's call. | Contract v0 keeps it as the one exception (A-14): v0's caller runs on the laptop (D8), whose owner already knows the backend and is the one who can sign in again, so it tells nobody anything new. Later: the architect, with a caller off the laptop (#0011) |
| **Q30** | **Later, not blocking:** **SHOULD the seam also cover `go`, `ssh-keygen`, `icacls` and `taskkill`?** Without it, a check cannot run `sync`'s build step or `ca`'s lockdown against a stand-in; `sync` runs its build when `go` and the localswim checkout are present, and skips it otherwise. From v0.07 the operator's key and certificate for a host also use `ssh-keygen`. | A check can still reach those steps without a stand-in: `sync` skips its build when `go` or the localswim checkout is missing, and `ca` runs against a throwaway home, as the operator's signing does with a throwaway CA ([The test seam](#the-test-seam)). Later: the architect |
| **Q31** | **Closed (architect, 2026-11-20, for v0.07-T01, P13).** ~~How is each host sized, with one stack parameters file?~~ A host is never sized by the box's parameters: a new host's stack takes only `SubnetId` and `VpcId` from `stack-parameters.json`, with every other parameter at the template's default; an existing host's stack keeps every parameter's previous value; and a host that is not the box changes size only through `deploy -Host <project> -InstanceType <type>`. ~~Plain `deploy` of the box keeps the file as its sizing path (D39, D55).~~ **Its box rule was replaced by D67 (architect, 2026-11-23, v0.09; architect, 2026-11-23, for v0.09-T01, pin P6):** the file sizes no host, the box included; its `InstanceType` is not passed and not used, and is left in the file; and every host, the box included, changes size only through `deploy -InstanceType <type>`, with or without `-Host`, which a provisioning run gives its deploy step from its project's `pending` leases (D68, D71) ([deploy](#deploy)). | Closed; replaced in part by D67 |
| **Q32** | **Closed (architect, 2026-11-20, for v0.07-T01, P13).** ~~Can two commands share the operator's folder at once?~~ None does: each command writes what it sends into `%USERPROFILE%\.agentspace\operator\<hex>\`, a folder of its own that it removes as it ends, and the next command that makes its own removes any such folder older than 24 hours ([The operator's folder, one per command](#the-operators-folder-one-per-command)). So a person MAY run `sync`, or a `deploy` of another host, while a provisioning run works; **from v0.09 a `deploy` of the run's own host is refused while it works**, since it takes that host's `provision.lock` ([deploy](#deploy); architect, 2026-11-23, for v0.09-T01, pins P34 and P36). | Closed |
| **Q33** | **Closed (architect, 2026-11-20, for v0.07-T01, P17 and P19).** ~~What happens to a host whose first stack creation failed?~~ `deploy -Host`'s existence check reads `Stacks[0].StackStatus`. No stack, or `REVIEW_IN_PROGRESS`, takes the create path; a state an update can start from takes the update path; a `*_IN_PROGRESS` status fails with a message to try again later; and `ROLLBACK_COMPLETE`, or any other state an update cannot start from, fails with a message that names the stack and the `aws cloudformation delete-stack` command that removes it, then says to provision again. `provision` records `failed` for a host that was never `ready`, and a `ready` host stays `ready`. No operator command deletes a stack in v0.07 ([deploy](#deploy)). | Closed |
| **Q34** | **Later, not blocking:** **How does a replaced CA reach a host that is not the box?** The operator reaches such a host only with a certificate signed by the CA, so once the CA is replaced, `sync` cannot reach a host that still trusts the old one, and a person recovers each by hand with their own key ([ca](#ca); D17). The candidate fix: install the new public key through Session Manager's Run Command as root, as the patch check already reaches a host ([What up waits for](#what-up-waits-for)), so no ssh is needed. A second `IdentityFile` in the operator's alias was ruled out (architect, 2026-11-20, for v0.07-T01, P19). | The recovery by hand is stated in [ca](#ca), and a replacement is rare (D17). Later: the architect |
| **Q35** | **Later, not blocking:** **SHOULD the provisioning log also hold the output of the programs the run starts?** v0.07-T01 ruled it SHOULD, but the pinned `Aws` and `Remote` hand each program's streams to the operator's own, so the run cannot read them, and v0.07-T03 kept the run's own lines alone, each failed step's line carrying the program's error text ([provision](#provision); architect, 2026-11-21). A detached run's streams are a hidden console nobody reads, so a program's progress output is lost there. | Later: the architect, if T05's live run shows a failure the log does not explain; the change is the run setting its own standard handles to the log before it starts a program |
| **Q36** | **Closed (architect, 2026-11-23, for v0.09-T01, pin P26; and pins P2(b) and P20).** ~~Can a lease be placed `ready` on a host while its run is resizing it?~~ No. By pins P2(b) and P20 (D66's change note), the run passes a type only when `resize_target` finds a `pending` lease a run would place ([provision](#provision), step 4.3); while that lease exists, place answers every new lease of the project `pending`; only the run places a `pending` lease, and `provision.lock` keeps a second run out. **The case pin P2(b) left, that `pending` lease released between the run's read of the lease state file and the type's removal, is closed by pin P26**: the run's deploy step takes the lease state lock, then `hosts.lock`, removes the record's `instance_type`, reads the lease state file under the lease state lock still held, and releases it before any call to AWS ([provision](#provision), steps 4.1 to 4.4; [driver.md, A resizing run](driver.md#a-resizing-run); [driver.md, Lock order](driver.md#lock-order)). A `create` holds the lease state lock for its whole place, so it is wholly before that hold, and its `ready` lease stops the resize, or wholly after it, and finds no type and answers `pending` (pin P2). Nothing Q36 named remains open. The candidate fix of place trying the host's `provision.lock` was dropped: place makes no new try ([driver.md, On a `ready` host](driver.md#on-a-ready-host), row 3). | Closed |

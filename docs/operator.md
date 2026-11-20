# The agentspace operator, reference v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for the operator: the laptop-side commands that manage the box (`deploy`, `up`,
`sync`, `ca`, `down`, `status`, `grow`), the SSH ProxyCommand, and the localswim token push.** It
is written so that the operator can be rebuilt without reading any of its code (D20, D34). It names
every command, parameter, message, stream and exit code, the protocol of the token push, the box
as the laptop sees it, and the AWS calls each command makes. The reasoning behind each rule is in
[decisions.md](decisions.md), cited as `D<n>`.

**What it does not own:**
- **The lease CLI** (`agentspace lease ...`) is [lease-api.md](lease-api.md)'s. It reaches one
  thing this page owns, the ProxyCommand, which every lease alias runs
  ([The ProxyCommand](#the-proxycommand)). It checks the CA itself, by
  [state.md's checks](state.md#its-checks), and never runs `ca` ([ca](#ca)).
- **Every file the operator reads or writes on the laptop is [docs/state.md](state.md)'s**: its
  location, format and access rules. This page names each file, says when it is read or written,
  and cites that doc for the rest ([Laptop files](#laptop-files)). **The one exception is
  localswim's receipt and Bot credential**, which agentspace reads and never writes: their formats
  are this page's ([The receipt](#the-receipt), [The Bot credential](#the-bot-credential)).
- **The box's own code stays shell (D28).** This page states the contract between the laptop and
  the box: what is sent, what is installed, what is asked and what is answered. It does not
  specify how the box's scripts do it.

**The operator is EC2-specific in v0.** Unlike the lease CLI, it names the backend: it manages
one EC2 instance through one CloudFormation stack (D8, D9). D1 binds what a lease's client sees,
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
  [argument style](#argument-style),
  [ProxyCommand lines already on the laptop](#proxycommand-lines-already-on-the-laptop),
  [until v0.06 retires the prototype's entry points](#until-v006-retires-the-prototypes-entry-points)
- [Commands](#commands): [the top level](#the-top-level), [`deploy`](#deploy), [`up`](#up),
  [`sync`](#sync), [`ca`](#ca), [`down`](#down), [`status`](#status), [`grow`](#grow)
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
- **The box is a host, and a host's platform is a parameter of the contract (D4).** Today there is
  one host: an EC2 instance running Ubuntu on `linux/arm64`, logged in to as `ubuntu`, with
  systemd, bash and passwordless `sudo`. Wherever this page names one of those, it is a property
  of today's box, marked "today's box", and not of the contract. A Windows host (v0.12, #0051)
  would answer the same questions in its own way.
- **The prototype is `scripts/agentspace.ps1`, run by Windows PowerShell 5.1, and
  `scripts/ssh-proxy.cmd`.** They are named here only where a note says where a requirement came
  from. The rewrite replaces both in v0.06 (#0055, D28) with `agentspace-operator.exe` and
  `agentspace-ssh-proxy.exe`, installed as [Programs and install layout](#programs-and-install-layout)
  says. Every path this page gives under `<checkout>` is the prototype's, and that section gives
  the rewrite's place for each.

### Command line

```
operator <command> [-GiB <n>] [-Alias <alias>] [-Quiet]
operator -h | --help
operator <command> -h | --help
```

**`operator` stands for the operator's program, `agentspace-operator.exe`**, which a person runs
as `agentspace-operator` and the help texts below name so
([Programs and install layout](#programs-and-install-layout)). Note on where this came from: the
prototype's entry point is `powershell -NoProfile -File <checkout>\scripts\agentspace.ps1`, to
which its ProxyCommand adds `-ExecutionPolicy Bypass`.

- **`<command>` is the first positional argument** and MUST be one of `deploy`, `up`, `sync`,
  `ca`, `down`, `status` and `grow`, whole: it is matched without regard to case, and never as an
  abbreviation (`st` is unknown).
- **Parameter names are matched without regard to case**, and a value follows its name as the next
  argument (`-GiB 4`). **The argument after `-GiB` or `-Alias` is its value, whatever it looks
  like**, so `-Alias -Fx` gives `-Alias` the value `-Fx`, and `-GiB -1` gives `-GiB` the value -1.
  The one exception is help, below. The rewrite MAY accept other spellings the prototype's shell
  accepts (a unique prefix such as `-Q`, `-GiB:4`, `-Command status`, or a common parameter such as
  `-Verbose`, accepted and ignored), and a caller MUST NOT depend on them.
- **`-GiB`'s value MUST be a decimal integer:** an optional `+` or `-`, then one or more ASCII
  digits, from -2147483648 to 2147483647. Anything else is a usage error. The rewrite MAY accept
  other forms the prototype's shell converts (`0x10` as 16, `1e1` as 10, `2.5` as 2, an empty value
  as 0), and a caller MUST NOT depend on them. Its range is not checked further
  ([grow](#grow); [Q22](#open-questions)).
- **Help is checked first.** An argument `-h` or `--help`, matched without regard to case, asks for
  help wherever it stands, even where a value is expected. Such a command line gets help, whatever
  else it holds: the help of its command when the line names a known command, and the top level's
  otherwise. So `sync -Alias x --help` prints `sync`'s help and exits 0.
  **Which argument is the command, for help** (architect, 2026-11-19, for v0.06, gap FB-11): the
  line is read from left to right, the argument after `-GiB` or `-Alias` is that parameter's value
  unless it is `-h` or `--help`, and the command is the first argument that is none of a
  parameter, a parameter's value and help. A value never names a command, so `-Alias up -h`
  prints the top level's help, and `-Alias -h up` prints `up`'s. **An argument that starts with
  `-` and is not a parameter's value counts as a parameter, known or not** (architect,
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
  | No command | `agentspace: No command given. The commands are deploy, up, sync, ca, down, status and grow.` |
  | An unknown command `<arg>` | `agentspace: Unknown command '<arg>'. The commands are deploy, up, sync, ca, down, status and grow.` |
  | A second positional argument `<arg>` | `agentspace: Unexpected argument '<arg>'.` |
  | An unknown parameter `<arg>` (an argument that starts with `-` and is not a parameter's value) | `agentspace: Unknown parameter '<arg>'.` |
  | A parameter given twice, `-Quiet` included | `agentspace: <name> is given more than once.` |
  | `-GiB` or `-Alias` as the last argument, with no value | `agentspace: <name> needs a value.` |
  | A `-GiB` value that is not a decimal integer in range | `agentspace: -GiB MUST be a whole number, not '<arg>'.` |

  `<name>` is the parameter as this page spells it (`-GiB`, `-Alias`, `-Quiet`), whatever case was
  given. `<arg>` is the argument as given, with each control character in it written as `?`, so
  that the message stays one line.
- **The checks run in this order, and the first failure ends the command:** help; then the
  arguments read from left to right, the first usage error found among them reported (a missing
  command is found only once the whole line is read); then the two `-Alias` checks below, in their
  order. So `status -GiB abc -Alias bad!` reports the `-GiB` message. **At each argument a
  repeated name is found before a missing value** (architect, 2026-11-19, for E0.06-T01-S01, as
  keeper.md's check 1 reads its own), so `status -GiB 1 -GiB` reports `agentspace: -GiB is given
  more than once.`
- **Then the home and the system folder, for every command** (architect, 2026-11-19, for
  E0.06-T01-S01): when `USERPROFILE` is unset or empty the command fails with exactly
  `agentspace: USERPROFILE is not set, so there is no home folder.`, and when `SystemRoot` is,
  with `agentspace: SystemRoot is not set, so Windows' programs cannot be found.`, checked in that
  order, before anything else runs and exit 1. Help and usage errors need neither. In caller mode
  either is the "Could not reach the host" line ([What a caller sees](#what-a-caller-sees-d1-card-0087)).
- **The alias check is part of the command line**, and runs before anything but help and the usage
  errors, as below.
- Note on where this came from: the prototype's shell binds the command line before the script
  runs, and reports each of these problems as its own error record, with exit 1, and, in the one
  pair tried, the first from the left. The record is not agentspace's message, so this is a
  requirement the prototype does not meet ([Requirements the prototype does not meet
  yet](#requirements-the-prototype-does-not-meet-yet)). With its stdin closed or not a console, it
  does not prompt for a missing command; at an interactive console it was not run.

| Parameter | Type | Default | Applies to | Notes |
|---|---|---|---|---|
| `-GiB <n>` | Integer | `2` | `grow` | The GiB to add to the root volume. Every command checks its form ([Command line](#command-line)), so `status -GiB abc` is a usage error; every command but `grow` then ignores its value. No range is checked; AWS refuses a size that is not larger ([grow](#grow)). |
| `-Alias <alias>` | SSH host alias | `agentspace` | `up` | The SSH host alias the token push connects with (D9, card #0057). Any other command given `-Alias` fails. |
| `-Quiet` | Switch | Off | Every command | Suppresses every message that is not a failure and not `status`'s report, warnings included ([Messages](#messages)). |
| `-h`, `--help` | Switch | | Every level | Prints the help of the level it is at to stdout and exits 0, doing nothing else. The prototype has no help of its own ([Q2](#open-questions)); the texts below are what the rewrite MUST print, naming the operator `agentspace-operator` ([Programs and install layout](#the-programs)). |

**`-Alias` is checked before anything but help and the usage errors runs**, for every command:
1. **Given with any command but `up`**, the operator fails with `agentspace: -Alias applies to up
   alone.`
2. **An alias MUST match `^[A-Za-z0-9._-]+\z`, case-sensitively, and MUST NOT start with `-`**,
   because it lands on ssh's command line, where a leading `-` would be read as an option. `\z`,
   unlike `$`, never matches before a trailing newline, and an empty alias fails. Any other value fails with
   `agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not
   start with '-'.` The default `agentspace` is checked the same way. Nothing is started before
   this check, not even an AWS call.

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
  `sync` and `grow`: the AWS CLI's `cloudformation deploy` output, `go build`'s, the box's
  `install.sh` lines and `grow`'s `df` table reach the operator's stdout or stderr as those
  programs wrote them. A caller MUST NOT parse that output; it is for a person.
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
that starts `agentspace: `. **A failure MUST NOT print a runtime's error record, stack or
exception text in place of that message.** The prototype does in some cases
([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).

### Messages

- **Each message is one line**, except the few that this page shows on several lines, ending in a
  line feed. A message starts with a capital, or with `agentspace: `, or with a path or an
  identifier.
- **A failure starts `agentspace: `**, followed by the text this page gives.
- **A warning starts `agentspace: ` too**, and does not change the exit code. Only the token push
  warns.
- **A progress message has no prefix.**
- **`-Quiet` suppresses progress messages and warnings.** Failures and `status`'s report always
  print. Output passed through from other programs is not affected by `-Quiet`. **Nor is the
  detail `AGENTSPACE_DIAGNOSTIC` asks for** (architect, 2026-11-19, for E0.06-T01-S01): it prints
  only in caller mode, which has no `-Quiet`, and a caller who sets the variable asked for it.
- **Placeholders in this page's message texts:** `<id>` is the instance ID, `<vol>` a volume ID,
  `<fp>` a key fingerprint (`SHA256:...`), and `<user>` the Windows user name, as the value of
  `USERNAME`, with no domain. Other placeholders are named where they appear.
- **Messages carry the instance ID and volume IDs.** A check that captures the operator's stderr
  captures them, and MUST withhold them from anything committed or reported. The repository is
  public.

### Programs it starts

**Card #0027 (ssh and scp by full path) is carried in here as a requirement.**
- **Every `ssh` and `scp` the operator starts MUST be Windows OpenSSH's, by full path:**
  `%SystemRoot%\System32\OpenSSH\ssh.exe` and `scp.exe`. It MUST NOT be found through `PATH`.
  Started from Git Bash, `PATH` finds MSYS's ssh, which runs the ProxyCommand through `/bin/sh`
  and mangles its Windows path (`exec: C:WindowsSystem32cmd.exe: not found`).
- **So `deploy`, `up`, `sync`, `status` and `grow` MUST work when the operator is started from
  Git Bash.**
  That is #0027's done-when, and this requirement closes it once the rewrite meets it.
- **Source:** in the prototype only the token push's ssh and the CA key's scp are by full path;
  every other ssh and scp is by bare name.

**The other programs, and how each is found:**

| Program | Found as | Used by |
|---|---|---|
| Windows OpenSSH `ssh.exe`, `scp.exe` | `%SystemRoot%\System32\OpenSSH\`, by full path (#0027) | `sync`, `grow`, the token push |
| `ssh-keygen.exe` | `%SystemRoot%\System32\OpenSSH\ssh-keygen.exe` | `ca`, and `sync`'s CA check |
| AWS CLI v2 | `C:\Program Files\Amazon\AWSCLIV2\aws.exe`; when that file is missing, `aws` on `PATH`. The tunnel alone has no fallback ([The ProxyCommand](#the-proxycommand), step 6) | Every command but `ca`; the ProxyCommand's tunnel |
| Session Manager plugin | Found by the AWS CLI on `PATH`; the ProxyCommand puts `C:\Program Files\Amazon\SessionManagerPlugin\bin` first | The ProxyCommand's tunnel |
| `icacls.exe`, `taskkill.exe` | `%SystemRoot%\System32\`, by full path | `ca`; the token push's time limit. **The rewrite MAY reach either outcome through the Windows API instead, and starts neither then** (architect, 2026-11-19, for v0.06, gap FB-15): `ca`'s steps and the push's step 4 define the outcome, and this row only says how each is found when it is started |
| `go` | On `PATH` | `sync`, to build the localswim CLI |
| The operator, `agentspace-operator.exe` | In the ProxyCommand's own folder ([How each program finds the others](#how-each-program-finds-the-others)); a ProxyCommand that runs `up` in its own process starts none. The prototype's ProxyCommand runs `powershell.exe` (Windows PowerShell 5.1), found on `PATH`, with its `agentspace.ps1` | The ProxyCommand, to run `up` |

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
the service and the operation as its first two arguments.

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
| `AGENTSPACE_DIAGNOSTIC` | Read by the ProxyCommand | When it is set to a non-empty value, the ProxyCommand MAY print backend detail to stderr that it otherwise withholds ([What a caller sees](#what-a-caller-sees-d1-card-0087)). |
| `PATH` | Read; the ProxyCommand prepends two folders | Finding `go`, the AWS CLI's fallback, and the Session Manager plugin. |

**The token MUST NOT be in any environment variable the operator sets ([Secrecy](#secrecy)).**

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
2. **It writes the instance-ID cache** into the throwaway home, with a made-up ID
   ([The instance-ID cache](#laptop-files)).
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

**Not covered:** `go`, `ssh-keygen`, `icacls` and `taskkill` have no variable
([Q30](#open-questions)).
- **So a test of `sync` or `deploy` inside the program's own crate MUST NOT reach the real `go` or
  `C:\Projects\localswim`** (architect, 2026-11-19, for E0.06-T03-S02): a build from Terry's
  checkout would read it. The crate gives its tests a hidden entry with the `go` finder, the
  localswim folder, the clock, the CA check and the push passed in, and no seam variable is
  added. `sync` finds `go` on `PATH` through a lookup over the process's environment, as the AWS
  CLI's fallback is found, and reads nothing else of the environment. An instance-ID cache that
  `deploy` cannot delete or write fails with `agentspace: Cannot write <path>.`; one already
  missing at the delete is not an error.

### Laptop files

**Every file here is [docs/state.md](state.md)'s**: its exact location, format and access rules.
This table says only which command reads or writes it. **The one exception is localswim's receipt
and Bot credential:** their formats are this page's ([The receipt](#the-receipt),
[The Bot credential](#the-bot-credential)), and state.md owns none of them. **The `<checkout>` rows
are the prototype's places;** where the rewrite keeps each is
[Programs and install layout](#programs-and-install-layout) (D37).

| File (as today's prototype has it) | Read by | Written by | What it is |
|---|---|---|---|
| `<checkout>\.instance-id` today; `%USERPROFILE%\.agentspace\instance-id.json` as D21 and D40 require | Every command but `ca`; the ProxyCommand | `deploy`; any command that finds it missing | The instance ID, cached from the stack's output ([The instance-ID cache](#laptop-files) below) |
| `%USERPROFILE%\.agentspace\localswim-pushed.sha256` today; `localswim-pushed.json` beside it as D40 requires | `up` | `up`, `sync` | The push stamp ([The stamp](#the-stamp)) |
| `%USERPROFILE%\.agentspace\ca\agentspace_ca` | `ca`, `sync` | `ca`, once | The CA's private key (D17) |
| `%USERPROFILE%\.agentspace\ca\agentspace_ca.pub` | `ca`, `sync` | `ca`, once | The CA's public key, which `sync` sends to the box |
| `%USERPROFILE%\.ssh\id_ed25519.pub` | `deploy` | Never | The user's own public key, passed to the stack as `SshPublicKey` |
| `%USERPROFILE%\.ssh\config` | The ssh client, for the `agentspace` entry and lease aliases | Never | [The `agentspace` SSH entry](#the-agentspace-ssh-entry) |
| `%USERPROFILE%\.claude\CLAUDE.md` | `sync` | Never | The user's global instructions for Claude Code, copied to the box |
| `<checkout>\infra\local.parameters` today; `%USERPROFILE%\.agentspace\stack-parameters.json` as D37 and D40 require | `deploy` | Never | The account-specific stack parameters, which are not committed |
| `<checkout>\infra\agentspace.yaml` | `deploy` | Never | The stack's template ([The stack](#the-stack)) |
| `<checkout>\box\system\` | `sync` | Never | The box's system files, sent whole |
| `<checkout>\.build\localswim-linux-arm64` | `sync` | `sync` | The localswim CLI built for today's box |
| `C:\Projects\localswim` | `sync` | Never | The localswim checkout the CLI is built from |
| `%LOCALAPPDATA%\localswim-v2\service.json` | `up` and `sync`, each time they consider a push | Never | The localswim service's receipt: the service's base URL and its credential folder. Its format, as agentspace relies on it, is [The receipt](#the-receipt)'s |
| `bot.xml` in the folder the receipt names | `up` and `sync`, after the receipt | Never | The localswim Bot credential, which holds the Bot token protected with DPAPI. Its format is [The Bot credential](#the-bot-credential)'s |

**The token is never in any of the operator's own files** ([Secrecy](#secrecy)).

**The instance-ID cache.** **It MUST live under `%USERPROFILE%\.agentspace`**, as the JSON file
`instance-id.json` (D40), and never in the checkout (D21, which settles [Q7](#open-questions); the file's
format and rules are docs/state.md's, at [Files in the checkout](state.md#files-in-the-checkout)).
It is only a cache, rebuilt from the stack's output when it is missing, so a rewrite needs no
migration from the checkout's copy. The prototype keeps `.instance-id` in the checkout
([Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)).
A cache that is empty, holds only whitespace, cannot be parsed as JSON, or does not match its
schema MUST be treated as missing (state.md). A cache that cannot be opened, such as one access is
denied to or a folder at its path, MUST fail the command with exactly
`agentspace: Cannot read the instance-ID cache <path>; remove it and run the command again.`, where
`<path>` is its full path (architect, 2026-11-19, for v0.06: FA-16). A cache that cannot be
written fails with `agentspace: Cannot write <path>.`, the form of the operator folder's own line,
and caller mode's "Could not reach the host" (architect, the same day, for E0.06-T03-S01). A stack with
no `InstanceId` output MUST fail with `agentspace: Stack 'agentbox' has no InstanceId output.`, and
write no cache. The prototype writes an empty cache then, and every later command fails with a
runtime error until `deploy` deletes it ([Q24](#open-questions)).

### The `agentspace` SSH entry

**The operator needs one SSH host alias in the user's OpenSSH client configuration: `agentspace`.**
`sync`, `grow` and the token push of `sync` connect through it, and so does any `up` started
without `-Alias`. A person writes the entry once, in their own OpenSSH client configuration, and
agentspace never writes it ([state.md](state.md#the-include-line)). What the operator needs from
the entry is this:

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
it.

### Every ssh runs the ProxyCommand

**Every ssh and scp the operator starts goes through a host alias, and so runs the ProxyCommand,
which runs `up` again.** That nested `up` is a full `up` but for the push: it reads the instance
state, starts the box if needed and waits. **It runs inside a ProxyCommand, so it prints the
caller's lines, not a person's** ([What a caller sees](#what-a-caller-sees-d1-card-0087)): its last
is `The host is ready.`, never `Ready: <id>`. So a `sync` prints one `The host is ready.` line per
ssh or scp it starts, to stderr, and each one costs a Session Manager session.

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
the prototype's entry points behave until v0.06 (#0055) retires them. Its rules are D37's, which
settles [Q10](#open-questions) here, and keeper.md's [Q3, Q8 and Q10](keeper.md#open-questions).

### The programs

| File | Role | Its spec | Kind | Conformance variable |
|---|---|---|---|---|
| `agentspace.exe` | The lease CLI, run as `agentspace` | [lease-api.md](lease-api.md) | Console program | `AGENTSPACE_CMD` |
| `agentspace` | The lease CLI's wrapper for Git Bash and other POSIX shells: it runs `agentspace.exe` from its own folder with its arguments unchanged and MSYS path conversion off, as [lease-api.md, The program](lease-api.md#the-program) allows | lease-api.md | POSIX `sh` script, no extension, with LF line ends and `#!/bin/sh` as its first line, which is what makes Git Bash run a file with no extension | None |
| `agentspace-operator.exe` | The operator: `deploy`, `up`, `sync`, `ca`, `down`, `status` and `grow` | [Command line](#command-line) | Console program | `AGENTSPACE_OPERATOR_CMD` |
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
program (D37).

| Program | Finds | For |
|---|---|---|
| `agentspace.exe` | `agentspace-ssh-proxy.exe`; when that file is not there, `agentspace-ssh-proxy.cmd`, v0.05's forwarder ([below](#until-v006-retires-the-prototypes-entry-points)) | The default ProxyCommand line ([state.md, The alias file](state.md#the-alias-file)) |
| `agentspace` (the wrapper) | `agentspace.exe` | Running the lease CLI |
| `agentspace-ssh-proxy.exe` | `agentspace-operator.exe`, when it runs `up` as a separate program. It MAY instead run `up` in its own process, with the same result | [The ProxyCommand](#the-proxycommand), step 3 |
| `agentspace-keeper.exe` | Its own file, when the holder's launcher is the keeper's own program again | [keeper.md, Starting a holder](keeper.md#starting-a-holder) |
| `agentspace-keeper-task.exe` | `agentspace-keeper.exe` | Running the keeper ([keeper.md](keeper.md#the-tasks-launcher)) |
| `agentspace-keeper-install.exe` | `agentspace-keeper.exe` and `agentspace-keeper-task.exe` | Checking both exist, and naming the launcher in the task by full path ([keeper.md](keeper.md#the-installer)) |

- **A program's own executable is the file the operating system started, as a full path.** A
  program MUST NOT take it from its first argument, which the process that started it chooses.
- **A program that does not find a sibling it needs fails the step that needs it**, with its own
  form of failure message, saying in agentspace's words which program is missing. The lease CLI's
  case is [state.md, The alias file](state.md#the-alias-file)'s.

### What the prototype took from its checkout

**Wherever this page names a path under `<checkout>`, it is the prototype's, and the rewrite uses
the place this table gives** (D37; [Laptop files](#laptop-files), [deploy](#deploy), [sync](#sync),
[ca](#ca), [The AWS side](#the-aws-side)). The files' formats are [docs/state.md](state.md)'s, at
[Files in the checkout](state.md#files-in-the-checkout).

| The prototype's | Used by | The rewrite's |
|---|---|---|
| `<checkout>\infra\agentspace.yaml`, the stack's template | `deploy` | Carried inside the operator program, built from the repository's `infra\agentspace.yaml`. `deploy` writes it to `%USERPROFILE%\.agentspace\operator\agentspace.yaml` just before the call that reads it, and names that file to the AWS CLI |
| `<checkout>\box\system\`, the box's system files | `sync` | Carried inside the operator program, built from the repository's `box\system`. `sync` writes them to `%USERPROFILE%\.agentspace\operator\system\`, after removing whatever that folder held, just before the copy, and copies that folder |
| `<checkout>\schemas\state\leases.state.schema.json` and the keeper's state schemas | The lease CLI and the keeper, to check the state files they read and write | Carried inside each program that checks a file against them |
| `<checkout>\infra\local.parameters`, `KEY=VALUE` lines | `deploy` | `%USERPROFILE%\.agentspace\stack-parameters.json`, JSON written by a person (D40) |
| `<checkout>\.build\localswim-linux-arm64` | `sync` | `%USERPROFILE%\.agentspace\operator\localswim-linux-arm64` |
| `<checkout>\.instance-id` | Every command but `ca`; the ProxyCommand | `%USERPROFILE%\.agentspace\instance-id.json` (D21, D40) |
| `%USERPROFILE%\.agentspace\localswim-pushed.sha256`, not in the checkout | `up`, `sync` | `%USERPROFILE%\.agentspace\localswim-pushed.json` (D40) |
| `<checkout>`, the folder the CA MUST NOT be inside | `ca`, step 2 | Any git working tree: `ca` fails when the CA's folder, or any folder above it, holds an entry named `.git`, a folder or the file a git worktree has (D37). `<folder>` in that step's message is the working tree's folder. So a home folder that is itself a git working tree, as a repository of the user's own settings files makes it, refuses every CA under it |
| `<checkout>\scripts\ssh-proxy.cmd`, beside the CLI's code | The lease CLI's default ProxyCommand line | `agentspace-ssh-proxy.exe`, beside `agentspace.exe` (D8) |
| `<checkout>`, the task's working folder | The keeper's task | `%USERPROFILE%` ([keeper.md, The task's fields](keeper.md#the-tasks-fields)) |
| `<checkout>`, the main checkout the installer runs from | The keeper's installer | The install folder ([keeper.md, The installer](keeper.md#the-installer)) |

- **The template and `box\system` the operator carries MUST be the repository's own files,
  embedded as they are at build time, and MUST NOT be re-authored from this page** (D37's
  addendum, FA-11). They are maintained YAML and shell, not the prototype (D28). What the operator
  builds and sends to the box at run time, every remote command line included, is the rewrite's
  own, written from the steps on this page (D34).
- **A file the operator writes under `%USERPROFILE%\.agentspace\operator` that cannot be written
  fails the command** before anything reads it, with exactly `agentspace: Cannot write <path>.`,
  where `<path>` is the file's or folder's full path (architect, 2026-11-19, for v0.06, gap FA-16):
  the first one whose creation or write failed, so an `operator` folder that cannot be made is
  named itself, never a file or folder below it (architect, the same day, for E0.06-T03-S02).
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

### Argument style

**The rewrite keeps every command line exactly as the specs give it (D37, B-04).**
- **The lease CLI keeps its `--name` flags** ([lease-api.md, Command lines](lease-api.md#command-lines)).
- **The operator, the keeper, the keeper's installer and the task's launcher keep their single-dash
  names** (`-GiB`, `-Alias`, `-Quiet`; `-Watch`, `-SshExe` and the rest; `-DryRun`, `-Uninstall`),
  matched as each page says: exactly, without regard to case, with a value as the next argument.
  The operator keeps `-h` and `--help`. The ProxyCommand keeps its one positional alias.
- **A single-dash name is a syntax, not PowerShell.** A native program parses it by the rules on
  its page. Spellings that PowerShell's binder also accepts, such as a unique prefix (`-Q`) or a
  value after a colon (`-GiB:4`), MAY be accepted, and a caller MUST NOT depend on them
  ([Command line](#command-line); [keeper.md, Command line](keeper.md#command-line)).
- **Why:** the conformance suite passes these command lines as written, through the four
  variables, and the keeper's task passes `-Watch`. Each page already defines its command line
  exactly, so keeping it costs a native program nothing, and changing it would change every caller
  and every check.

### ProxyCommand lines already on the laptop

**A ProxyCommand line names a program by its full path until the line is rewritten, so a program
that a line names MUST keep working, or forward to one that does, for as long as that line can
still make a connection (D37, B-30).** Three kinds of line exist:

| Line | Written by | Names | Rewritten |
|---|---|---|---|
| The user's `agentspace` entry ([The `agentspace` SSH entry](#the-agentspace-ssh-entry)) | The user, never agentspace | Today, the checkout's `scripts\ssh-proxy.cmd` | By the user, by hand |
| A lease's alias file, written by the prototype's lease CLI | `create` | The checkout's `scripts\ssh-proxy.cmd` | Never: `renew` does not rewrite it, and `release` removes it |
| A lease's alias file, written by the rewrite's lease CLI | `create` | The ProxyCommand program beside that CLI | Never, likewise |

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

## Commands

### The top level

```
operator <command> [-GiB <n>] [-Alias <alias>] [-Quiet]
operator -h | --help
```

**Help:**

```
Usage: agentspace-operator <command> [-GiB <n>] [-Alias <alias>] [-Quiet] [--help]

Manages the agentspace box from the laptop: its stack, its power, its system
files, the SSH CA it trusts, and the localswim Bot token it uses.

Commands:
  deploy  Create or update the stack, then run sync
  up      Start the box if needed, wait until it has patched, push the Bot token
  sync    Check the CA, install the box's system files, CLAUDE.md and the
          localswim CLI, and push the Bot token
  ca      Create the agentspace SSH CA once; later, check it and print it
  down    Stop the box
  status  Show the box's state, type, disk and Session Manager reachability
  grow    Grow the box's disk by -GiB (default 2), and its filesystem with it

Parameters:
  -GiB <n>        For grow: the GiB to add (default 2)
  -Alias <alias>  For up: the SSH host alias the token push uses (default agentspace)
  -Quiet          Print no progress or warnings; failures and status still print
  -h, --help      Print this help, or a command's, and exit

The operator's own messages go to stderr; deploy, sync and grow pass other
programs' output through. up writes nothing to stdout, so it is safe in an SSH
ProxyCommand.

Exit codes:
  0  Success
  1  Failure or usage error; the message on stderr says which
```

### deploy

```
operator deploy [-Quiet]
```

**Creates or updates the CloudFormation stack, refreshes the instance-ID cache, then runs
`sync`.**

1. **Reads the user's public key** from `%USERPROFILE%\.ssh\id_ed25519.pub`, trimmed. A missing,
   unreadable or empty file MUST fail with an `agentspace: ` message naming it:
   `agentspace: <path> does not exist.`, `agentspace: Cannot read <path>.` or
   `agentspace: <path> is empty.` (architect, 2026-11-19, for E0.06-T03-S02). A file of whitespace
   alone is empty, since the key is read trimmed. The prototype fails
   with a runtime error for a missing or empty file, and passes an empty key on for a
   whitespace-only one.
2. **Reads the stack parameters file, `%USERPROFILE%\.agentspace\stack-parameters.json`** (D37,
   D40; [What the prototype took from its checkout](#programs-and-install-layout); the prototype
   reads `KEY=VALUE` lines from `<checkout>\infra\local.parameters`). A missing file fails with
   `agentspace: Missing <path>; copy infra\stack-parameters.example.json and fill it in.`, where
   `<path>` is the file's full path. A file that cannot be opened, cannot be parsed as JSON, or does
   not match its schema fails with `agentspace: <path> is not a valid stack parameters file:
   <reason>.`, where `<reason>` is one sentence fragment that names the first problem found and
   holds no line break (architect, 2026-11-19, for v0.06). Each member of `parameters` becomes a
   parameter override (format: docs/state.md). **A file that names `SshPublicKey`, or names any
   member twice, is not valid** (architect, 2026-11-19, for E0.06-T03-S02): `deploy` sets
   `SshPublicKey` itself from step 1, and a file that could override it would put another key on
   the box with no word said. The `<reason>` names the member. Any stack parameter can be set there, not only
   `VpcId` and `SubnetId`, so `ImageId` MUST NOT be set there on a live stack
   ([The stack](#the-stack)).
3. **Prints `Deploying stack 'agentbox' in us-west-2...`.**
4. **Runs `cloudformation deploy`** ([The AWS side](#calls-by-command)), with its output passed
   through. A non-zero exit fails with `agentspace: Stack deploy failed; see the stack events in
   the console.` An expired sign-in MUST fail with #0034's message instead
   ([AWS failures](#aws-failures)).
5. **Deletes the instance-ID cache, reads the instance ID from the stack's `InstanceId` output,
   writes the cache again** ([Laptop files](#laptop-files), the instance-ID cache), and prints
   `Instance: <id>`. The cache is deleted first so that the ID comes from the stack as the deploy
   left it, never from an older cache. If the read fails, `deploy` fails with no cache on the
   laptop, and the next command that needs the ID rebuilds it from the stack.
6. **Runs `sync`**, as if `operator sync` had been run with `deploy`'s own `-Quiet`: **`deploy
   -Quiet` quiets its `sync` step too** (D36; [Q4](#open-questions), closed), as the `-Quiet` row of
   [Command line](#command-line) says of every command. **`deploy` MUST exit 1 when `sync`
   fails.** The prototype runs `sync` as a nested script without `-Quiet`, so `sync`'s messages
   print, and its failure most likely does not reach `deploy`'s exit code ([Q3](#open-questions));
   both are [requirements it does not meet yet](#requirements-the-prototype-does-not-meet-yet).

**Stdout:** the AWS CLI's deploy output, then `sync`'s. **Stderr:** the messages above, then
`sync`'s. **Exit codes:** 0 and 1.

**`deploy` passes no `ImageId`, so the template's pinned default applies** ([The
stack](#the-stack)).

**Help:**

```
Usage: agentspace-operator deploy [-Quiet]

Creates or updates the CloudFormation stack 'agentbox' in us-west-2 from
its template, with the parameters in ~/.agentspace/stack-parameters.json
and your ~/.ssh/id_ed25519.pub, then runs sync.

Never change the template's ImageId on a live stack: it replaces the instance
and its disk.

Exit codes: 0 success, 1 failure.
```

### up

```
operator up [-Alias <alias>] [-Quiet]
```

**Makes the box ready for a connection: starts it if it is stopped, waits until its boot-time
patch run has finished, and pushes the localswim Bot token and the laptop's time zone if they are
due.** The ProxyCommand runs it before every connection.

1. **Reads the instance ID** from the cache, or from the stack's output when the cache is missing,
   writing the cache then.
2. **Starts the box and waits for it**, as [Starting the box and waiting for
   it](#starting-the-box-and-waiting-for-it) says, ending with `Ready: <id>`.
3. **Pushes the token over `<alias>`**, as [The token push](#the-token-push) says, unless it is
   a nested `up` ([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)).

**Stdout:** nothing, ever. **Stderr:** the messages of steps 2 and 3, and the stderr of the push's
ssh, which carries its nested ProxyCommand's lines, `The host is ready.` among them
([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)). Run by the ProxyCommand,
`up` prints the caller's lines in place of a person's
([What a caller sees](#what-a-caller-sees-d1-card-0087)). **Exit codes:** 0 once the box is ready,
whatever the push did ([What the laptop checks](#what-the-laptop-checks)); 1 on any failure before
that.

**Help:**

```
Usage: agentspace-operator up [-Alias <alias>] [-Quiet]

Starts the box if it is stopped, waits until its boot-time patch run has
finished, and sends the localswim Bot token and the laptop's time zone if
either has changed since the last confirmed send.

  -Alias <alias>  The SSH host alias the token push connects with
                  (default agentspace). Letters, digits, '.', '_' and '-',
                  not starting with '-'.

Writes nothing to stdout, so it is safe inside an SSH ProxyCommand.
A failed token push warns and still exits 0.

Exit codes: 0 the box is ready, 1 failure.
```

### sync

```
operator sync [-Quiet]
```

**Installs the box's system files and the CA's public key, copies the user's `CLAUDE.md`, builds
and installs the localswim CLI, and pushes the token.** Every connection goes through the
`agentspace` alias, with forwards cleared.

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
     `%USERPROFILE%\.agentspace\operator\localswim-linux-arm64`). **The build is exactly
     `go build -trimpath -o <out> ./cmd/localswim`**, run in `C:\Projects\localswim` with
     `GOOS=linux`, `GOARCH=arm64` and `CGO_ENABLED=0` ([Environment variables](#environment-variables)),
     where `<out>` is the output file's full path, and with no other flag (architect, 2026-11-19,
     for v0.06, gap FB-14). A failed build fails with `agentspace:
     Building the localswim CLI failed.` `linux/arm64` is today's box's platform.
   - **Prints `Copying the localswim CLI to the instance...`**, copies the binary to the box's
     staging path (`/tmp/localswim` today), and fails with `agentspace: Copying the localswim CLI
     failed.` if that fails.
   - **Installs it as root, mode 0755, at `/usr/local/lib/localswim/localswim`**, and removes the
     staged copy. A failure fails with `agentspace: Installing the localswim CLI failed.`
8. **Pushes the token over `agentspace`, forced**: the stamp is not compared, so the push always
   runs ([The token push](#the-token-push)).

**Stdout:** the install's `Installed:` lines, and whatever else the remote steps print on their
stdout. **Stderr:** the messages above; `go build`'s output; the remote steps' stderr, the
install's `install.sh:` lines among them; one `The host is ready.` per ssh or scp, from its nested
ProxyCommand; and the push's. **Exit codes:** 0 and 1, whatever the push did
([What the laptop checks](#what-the-laptop-checks)).

**Its ssh and scp calls, in order** (B-23, A-30). Each is one process, started as
[Programs it starts](#programs-it-starts) says (or through [the test seam](#the-test-seam)), with
exactly these arguments in this order, its stdout and stderr the operator's, and each through the
`agentspace` alias, so each runs its own nested ProxyCommand. None of rows 1 to 8 sets `BatchMode`; row 9, the push,
does ([What the laptop sends](#what-the-laptop-sends)). The local paths
are full Windows paths, with `%USERPROFILE%` and `<checkout>` written as their values; `<staging>` is the box's staging folder, `/tmp/agentspace-system` on
today's box.

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
- **Each of steps 1 to 8 but 5 is checked by its process's exit code**, as steps 3 to 7 above
  say; step 5's failure is reported by step 6's copy, as step 6 above says, and step 9 is
  checked as [What the laptop checks](#what-the-laptop-checks) says, never failing `sync`. An ssh that fails to connect exits
  255, which fails its step the same way.
- **So a `sync` starts at least five processes** (1, 2, 3, 4 and 9) and at most nine, and makes as
  many Session Manager sessions.

**After the localswim service on the laptop is updated, `sync` MUST be run again**, so that the
box's localswim CLI is built from the same source as the service it talks to.

**Help:**

```
Usage: agentspace-operator sync [-Quiet]

Checks the agentspace SSH CA, then installs box/system and the CA's public key
on the box, copies ~/.claude/CLAUDE.md there, builds and installs the localswim
CLI from C:\Projects\localswim, and sends the localswim Bot token and the
laptop's time zone. Starts the box first if it is stopped.

Fails before touching the box if the CA is missing, half-made, mismatched or
readable by anyone else.

Exit codes: 0 success, 1 failure.
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
replaced: delete both files, run `ca`, run `sync` for every host, and re-issue every live lease's
certificate (D17).

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
operator down [-Quiet]
```

**Asks EC2 to stop the instance, and prints `Stopping <id>.`** once the request is accepted. It
does not wait for the stop, and it asks whatever state the instance is in; stopping a stopped
instance succeeds. Open sessions on the box end with it.

**Stdout:** nothing. **Stderr:** that line. **Exit codes:** 0 and 1.

**Help:**

```
Usage: agentspace-operator down [-Quiet]

Stops the box. Does not wait for it to stop. Open sessions on it end.
The next ssh agentspace, or any lease connection, starts it again.

Exit codes: 0 the stop was requested, 1 failure.
```

### status

```
operator status [-Quiet]
```

**Prints five lines about the instance, to stderr, whatever `-Quiet` says:**

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

**`status` never starts the box.** **Stdout:** nothing. **Exit codes:** 0 and 1.

**Help:**

```
Usage: agentspace-operator status [-Quiet]

Shows the instance's ID, state, type, root disk size and volume, and whether
Session Manager can reach it. Never starts the box. Prints to stderr.

Exit codes: 0 success, 1 failure.
```

### grow

```
operator grow [-GiB <n>] [-Quiet]
```

**Grows the root volume by `<n>` GiB (default 2), and the root filesystem with it.**

1. **Reads the root volume's size**, and prints `Growing <vol> from <size> to <new> GiB (EBS allows
   one change per volume every 6 hours)...`, where `<new>` is `<size>` plus `<n>`.
2. **Asks EC2 to change the volume to `<new>` GiB.** A refusal fails with the AWS CLI's error
   ([AWS failures](#aws-failures)), which is what happens for a second change within 6 hours, or
   for an `<n>` of 0 or less.
3. **When the instance is running**, as step 1 read it, prints `Growing the filesystem online...`,
   waits 10 seconds, and starts one ssh (B-23, A-30):

   ```
   ssh -o ClearAllForwardings=yes agentspace <remote command>
   ```

   with `<remote command>` one argument, the last, as for [sync's calls](#sync), its stdout and
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

**Help:**

```
Usage: agentspace-operator grow [-GiB <n>] [-Quiet]

Grows the box's root volume by -GiB (default 2) and, if the box is running,
its filesystem online; a stopped box grows its filesystem at next boot.
EBS volumes never shrink, and allow one change every 6 hours.

Exit codes: 0 success, 1 failure.
```

## Starting the box and waiting for it

**`up` and `sync` share one start-and-wait step**, and every nested `up` runs it too.

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
     ([What up waits for](#what-up-waits-for)). An answer other than `pending` ends the wait. A
     `pending` answer gives the message `Patching Ubuntu (dist-upgrade, reboot if the kernel
     changed)...`, which is today's box's wording.
   - **A message is printed only when it differs from the last one printed**, so a long wait shows
     each phase once.
   - **At the deadline** it fails with `agentspace: <id> was not ready within 15 minutes; check:
     ssh agentspace journalctl -u agentspace-patch`, whose command is today's box's.
4. **Prints `Ready: <id>`.**

**Run by the ProxyCommand, these steps print the caller's lines in place of every message above**
([What a caller sees](#what-a-caller-sees-d1-card-0087)).

**`up` MUST NOT print `Ready` before the box's patch run has finished**, because the box MUST be
fully patched at every start before it is used ([What up waits for](#what-up-waits-for)), within
three exceptions: a box running for 20 minutes is not asked, an answer of `unknown` counts as
finished, and so does an empty or unexpected answer from a failed check
([What up waits for](#what-up-waits-for); [Q6](#open-questions)). This is the one statement of
them.

## The ProxyCommand

**The ProxyCommand is what makes `ssh agentspace`, and `ssh <lease alias>`, start the box and reach
it with no inbound port.** ssh runs it in place of a TCP connection, and talks SSH over its stdin
and stdout. D9 makes this path the EC2 driver's.

**How ssh runs it.** The `ProxyCommand` line of an alias that reaches the box is the line
[state.md, The alias file](state.md#the-alias-file) gives: the ProxyCommand program's full path,
through `cmd.exe` when the program is a `.cmd` or `.bat` file, then the alias. Which program that
is, and where it lives, is [Programs and install layout](#programs-and-install-layout)'s. The user's own
`agentspace` entry passes no alias ([The `agentspace` SSH entry](#the-agentspace-ssh-entry)), and a
lease alias passes its own name, written out literally, never as ssh's `%n` token. The Claude
desktop app runs the same line through its own proxy program.

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
   before any ssh or tunnel, with the `-Alias` message.
4. **If they failed, exits 1**, starting no tunnel.
5. **Reads the instance ID from the instance-ID cache** ([Laptop files](#laptop-files)), which
   step 3 has just made sure exists. A cache that is missing, empty or unreadable here fails with
   the caller's failure message, exit 1, and starts no tunnel.
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
- **Start the tunnel for an instance other than the cached one.**

**The threat model.** The alias argument comes from a ProxyCommand line the lease CLI wrote
([lease-api.md](lease-api.md#fields): `ssh.host_alias`;
[state.md, The alias file](state.md#the-alias-file)), and the user's own entry passes none.
Whoever can write a ProxyCommand line already runs commands, since the whole line is run as a
command (and parsed by `cmd.exe` when it starts with it) before the proxy sees its argument, so
the proxy does not defend against a hostile line. **`up`'s exact check
([Command line](#command-line)) MUST be the gate before any ssh.**

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
in this order, and only when the person's line it replaces would have printed:

| A person's `up` prints | The ProxyCommand prints instead |
|---|---|
| `Starting <id>...`, or `Waiting for <id> to finish stopping...` then `Starting <id>...` | `Starting the host...` |
| `Waiting for Session Manager...` or `Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...`, at the first of them | `Waiting for the host to be ready...` |
| `Ready: <id>` | `The host is ready.` |
| The push's `Sending the localswim Bot token to the instance...` | Nothing |
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
| Any other failure: an AWS call's error text, a cache that cannot be read or written, a stack with no `InstanceId` output, a missing AWS CLI, and any failure this page gives no text for | `agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.` |

**The tunnel.** Its stdout is the connection. **Its stderr MUST be withheld from the caller unless
`AGENTSPACE_DIAGNOSTIC` is set**, since what the AWS CLI and Session Manager's plugin write there
is backend text. **A tunnel that exits non-zero** gets `agentspace: The connection to the host
failed. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.` on stderr, and the
ProxyCommand then exits with the tunnel's code. A tunnel that cannot be started at all gets the
"Could not reach the host" message, and exit 1.

- **Backend text is** an instance, volume or host ID; a region name; a machine type; a cloud
  instance's state name (`pending`, `running`, `stopping`, `stopped`, `shutting-down` or
  `terminated`) given as that state; the words EC2, AWS, SSM, Session Manager and CloudFormation;
  the stack's name; the host's operating system or patch tool; and any text of the AWS CLI, of
  Session Manager's plugin or of a runtime's error record.
- **On success the ProxyCommand MUST NOT print backend text.** It prints the caller's progress
  lines above, the push's warnings in their caller form, and nothing else but what
  `AGENTSPACE_DIAGNOSTIC` allows, below. It prints nothing to
  stdout but the tunnel ([above](#the-proxycommand)).
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

- **`up` pushes over its `-Alias`** (default `agentspace`), after it prints `Ready: <id>` (`The
  host is ready.` in caller mode, [What a caller sees](#what-a-caller-sees-d1-card-0087)), unless
  it is a nested `up` ([Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)). It
  sends only when the push is due ([The stamp](#the-stamp)).
- **`sync` pushes over `agentspace`, forced**, as its last step: always, whatever the stamp says.
  `AGENTSPACE_NESTED` does not stop it.
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
2. **The Bot credential, `bot.xml` in that folder, and from it the Bot token**
   ([The Bot credential](#the-bot-credential)).
3. **The laptop's time zone**, as an IANA name ([The time zone](#the-time-zone)).

**Both files are localswim's, and agentspace never writes, moves or deletes them**
([docs/state.md](state.md#files-agentspace-reads-and-never-writes), which owns that rule).
localswim documents where they are and how the credential is made, and not their formats in full. Each rule below says where
localswim's own documentation defines it; **every rule that cites no localswim document is what
agentspace relies on without localswim having documented it**, observed on throwaway files with a
dummy secret (Windows PowerShell 5.1 and PowerShell 7.6, 2026-11-18). The token's own secrecy
rules are [Secrecy](#secrecy)'s, and apply from the moment the token is decrypted.

**Both files MUST be read afresh each time a push is considered, and the operator MUST NOT keep
anything read from them for a later push.** localswim's procedure makes new tokens in a new
credential folder at every cold start (`docs/windows-operations.md`, "Provision the launch
credentials"), so a folder or a token remembered from an earlier read can be stale.

#### The receipt

- **It is the file `%LOCALAPPDATA%\localswim-v2\service.json`**, where `%LOCALAPPDATA%` is the
  value of the `LOCALAPPDATA` environment variable when the operator starts. localswim names this
  location in its `AGENTS.md`, "Production state", as "the service receipt".
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
  | `base` | String | The service's base URL, such as `http://127.0.0.1:8811` | Sends its value, after JSON unescaping, as `LOCALSWIM_URL`, with nothing added or removed. What form it MUST have is [Q9](#open-questions) |
  | `credential_directory` | String, not empty | The folder that holds the service's launch credentials, `human.xml` and `bot.xml` | Reads `bot.xml` in it ([The Bot credential](#the-bot-credential)) |

  - **Every other member is ignored**, whatever it holds. The receipt carries others (localswim's
    `AGENTS.md` says it names the service's executable), and agentspace relies on none of them.
  - **Two names that differ only in ASCII case are the same member**, so `base` and `BASE` in one
    receipt are one member given twice.
- **Refused, with the "Could not read" warning:** `credential_directory` missing, not a string, or
  empty; `base` present but not a string (`null` included); or either member given more than once,
  in any case, which could mean two different things (D20). A `base` that is missing or empty is
  refused below, with the token's own checks.

#### The Bot credential

- **It is the file `bot.xml` in the folder that `credential_directory` names**: that path, then a
  `\` unless it already ends in `\` or `/`, then `bot.xml`. A `/` in the path is a separator, as
  `\` is. A relative path is resolved against the operator's working folder. localswim's procedure
  makes the folder `%LOCALAPPDATA%\localswim-v2\credentials\<GUID>` and the file `bot.xml` in it
  (`docs/windows-operations.md`, "Provision the launch credentials"); agentspace relies only on
  the receipt's member and the file's name, never on the folder's form.
- **A `bot.xml` that is missing, is a folder, or cannot be read is refused.**
- **What it is:** PowerShell's CLIXML serialization of one `PSCredential` object, as `Export-Clixml`
  writes it, whose password is the Bot token protected with Windows DPAPI for the Windows user who
  wrote it. localswim's procedure makes it that way, and says it can be decrypted "by the same
  Windows user on the same PC" (`docs/windows-operations.md`, "Provision the launch credentials").
  That is all localswim documents. The CLIXML element structure is Microsoft's, in the PowerShell
  Remoting Protocol specification ([MS-PSRP], its serialization section, which also defines the
  `SS` element for a secure string). The password's content, the hex of a DPAPI blob, is what
  PowerShell's `ConvertFrom-SecureString` writes when given no key, which Microsoft documents only
  as using DPAPI. The byte-level rules below are therefore agentspace's own statement.
- **It is an XML 1.0 document, and its encoding is detected as XML 1.0 detects it** (its
  Appendix F). Windows PowerShell 5.1 writes it in UTF-16LE with the mark `FF FE`; PowerShell 7
  writes it in UTF-8 with no mark. **A reader MUST accept both, and UTF-8 with the mark
  `EF BB BF`**, with or without an XML declaration. Line ends and the whitespace between elements
  carry no meaning. Comments, processing instructions, CDATA sections and character references
  mean what XML 1.0 says they mean. **A document type declaration (`<!DOCTYPE ...>`) is refused**,
  as the prototype refuses it, so no entity is ever expanded.
- **Its structure,** as both runtimes write it (the hex shortened here):

  ```xml
  <Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04">
    <Obj RefId="0">
      <TN RefId="0">
        <T>System.Management.Automation.PSCredential</T>
        <T>System.Object</T>
      </TN>
      <ToString>System.Management.Automation.PSCredential</ToString>
      <Props>
        <S N="UserName">bot</S>
        <SS N="Password">01000000d08c9ddf0115d1118c7a00c04fc297eb01000000...</SS>
      </Props>
    </Obj>
  </Objs>
  ```

  **Every element is in the namespace `http://schemas.microsoft.com/powershell/2004/04`**, whether
  it is the default namespace, as written above, or bound to a prefix. **The `N` and `Version`
  attributes are in no namespace.** The reader MUST find all of these, and refuses the file
  otherwise:
  1. **The root element is `Objs`.** When it has a `Version` attribute, its value, with whitespace
     around it ignored, MUST be two to four decimal numbers joined by `.` whose first number is 1,
     such as `1.1.0.1`; any other value, an empty one included, is refused, as the prototype
     refuses it. An `Objs` with no `Version` is accepted.
  2. **`Objs` has exactly one child element `Obj`.** Two or more could mean two credentials, and
     are refused (D20).
  3. **That `Obj` has a `TN` element with a `T` child whose text is exactly
     `System.Management.Automation.PSCredential`**, in any position among its `T` children, as the
     prototype accepts it.
  4. **That `Obj` has a `Props` element holding exactly one `S` element whose `N` attribute is
     `UserName`, and exactly one `SS` element whose `N` attribute is `Password`.** The two `N`
     values are matched in any ASCII case (D20), as the prototype matches them, and a second
     element of either, in any case, could mean two things and is refused (D20). The `UserName`
     text is the credential's user name, `bot` in localswim's procedure; it MUST be present, and
     agentspace does not use it. "Present" means the element: an empty `UserName` text is accepted
     (architect, 2026-11-19, for E0.06-T01-S04), since nothing reads it.

  Every other element and attribute (`RefId`, `ToString`, any other `T`, any other member of
  `Props`) is ignored. A password in an `S` element, as plain text, is refused, since it is not
  protected.
- **The `SS` element's text becomes bytes:** the text is exactly an even number of hex digits, at
  least two (`0-9`, `a-f`, `A-F`; `Export-Clixml` writes lowercase), with nothing around them. Each
  pair of digits, in order, is one byte, the first digit its high four bits. Text that is empty,
  of odd length, or holds whitespace or any other character is refused.
- **Those bytes are a DPAPI blob**, which begins `01 00 00 00` and then the DPAPI provider's GUID
  `D0 8C 9D DF 01 15 D1 11 8C 7A 00 C0 4F C2 97 EB`. A reader MAY refuse a blob that does not begin
  with them; the call below refuses one whose first four bytes differ, and does not check the
  GUID. **The reader
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
  NUL. A byte count that is odd is refused, as the prototype refuses it. Text that is not valid
  UTF-16 (an unpaired surrogate) is refused, since it has no UTF-8 form to send (D20). The token is
  that text, and the content carries it in UTF-8 (below).
- **localswim's procedure makes each token from 32 random bytes, written as base64url with no
  padding** (`docs/windows-operations.md`, "Provision the launch credentials"), which is 43
  characters. agentspace relies on none of that, and checks only what the list below says
  ([Q9](#open-questions)).
- **A test writes a credential for itself the same way in reverse** ([The test seam](#the-test-seam)):
  `CryptProtectData` on the dummy token's UTF-16LE bytes, with no entropy and
  `CRYPTPROTECT_UI_FORBIDDEN`, as the user that runs the operator; the blob in lowercase hex as the
  `SS` element's text, in the structure above, in UTF-8. PowerShell's `Export-Clixml` of a
  `PSCredential` writes the same. Its token is a dummy, as [Secrecy](#secrecy) requires of a check
  that stands in for `ssh`.

Note on where this came from: the prototype (`scripts/agentspace.ps1`, run by Windows PowerShell
5.1) reads the receipt and deserializes `bot.xml` with PowerShell's own cmdlets. The rules above
match what those cmdlets accepted and refused on throwaway files under Windows PowerShell 5.1
(the credential's cases also under PowerShell 7.6, which gave the same answers but cut a token at
its first NUL), except where
[Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet)
lists a difference.

**The push MUST warn `agentspace: Could not read the localswim Bot token; the board will not work
on the box this session.`, and send nothing, when any of these holds:**
- The receipt exists and cannot be read, or is refused by [The receipt](#the-receipt)'s rules.
- `bot.xml` cannot be read, or is refused by [The Bot credential](#the-bot-credential)'s rules, a
  failed decryption included.
- The base URL or the token is missing or empty.
- The base URL or the token holds a CR, an LF or a NUL. Any of them would break the content's
  two-line framing, or make the box install a value other than the one sent.

The warning MUST NOT include the failure's own text, which could carry part of the credential.
Because the token is read before the push is known to be due, a laptop whose credential cannot be
read gets this warning at every `up`, even when nothing would have been sent. What other characters
the URL and the token may hold is [Q9](#open-questions). The prototype checks neither empty values
nor CR, LF and NUL ([Requirements the prototype does not meet
yet](#requirements-the-prototype-does-not-meet-yet)).

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

- **The stamp is that hash written as uppercase hex.** How it is stored, and where, is
  docs/state.md's: `%USERPROFILE%\.agentspace\localswim-pushed.json`, a JSON document (D40). The
  prototype's is `localswim-pushed.sha256`, the hex alone with no line end, which the rewrite never
  reads, so the first `up` after the change-over pushes once. A stored stamp that matches its
  schema is compared with the computed one exactly.
- **A push is due when it is forced (`sync`), when there is no stamp, when the stored stamp is
  empty, cannot be read, cannot be parsed or does not match its schema, or when it differs from
  the one just computed.** The prototype instead
  warns "Could not send" and pushes nothing when the stamp file is empty or unreadable
  ([Requirements the prototype does not meet
  yet](#requirements-the-prototype-does-not-meet-yet)).
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

   - `<alias>` is `up`'s `-Alias`, or `agentspace` for `sync`.
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
   - The ssh gets a pipe for its stdin and one for its stdout; its stderr is the operator's
     stderr, so it carries its nested ProxyCommand's lines, `The host is ready.` among them
     ([What a caller sees](#what-a-caller-sees-d1-card-0087)). Its environment is
     the operator's, as [Environment variables](#environment-variables) sets it.
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
- **The Bot credential's reader:** an XML declaration that names an encoding other than the
  bytes' own (UTF-8, or UTF-16 by its byte order mark) is refused; `Objs`'s `Version` is read as
  numbers that each fit a 32-bit signed integer, leading zeros allowed, the first MUST be 1; a
  second `UserName` or `Password` anywhere among the `Props` elements is refused.

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

**This section is the contract between the laptop and today's box.** Every path and tool in it is
a property of today's box (Ubuntu, systemd, bash), and the box's scripts that carry it out stay
shell (D28). **The box is changed through `<checkout>\box\system` and `sync`, never by editing the
box:** every file `sync` puts there ([The box's files](#the-boxs-files)) is replaced by the next
`sync`, so a change made on the box itself is lost. sshd keeps the first value it reads for a
keyword, so a new sshd drop-in MUST NOT set a keyword that another drop-in already sets.

### What sync installs

`sync` sends `<checkout>\box\system` whole, with the CA's public key beside it as
`agentspace_ca.pub`, and runs its `install.sh` as root from that folder. The install:

- **Fails at once if `agentspace_ca.pub` is missing or empty**, printing `install.sh:
  agentspace_ca.pub is missing; run agentspace-operator sync, which sends it` to stderr and
  exiting 1. The prototype's box files name `scripts\agentspace.ps1` there. **The repository's
  `box\system\install.sh` is changed to this text by v0.06's cutover story, in the pull request
  that removes `scripts\agentspace.ps1`** (architect, 2026-11-19, for v0.06, gap FA-12): under
  D37's addendum the box's files change only through the repository, so the clean room's builders
  do not edit it.
- **Retires the project's first name, once.** On a box that still has `agentbox` units, it
  disables them, carries over this boot's patch marker and the last patch-reboot time, removes the
  old files, and prints `Retired the agentbox units and files`.
- **Installs the box's files** (the table below), reloads systemd, enables and starts the idle
  timer, and enables the patch service. It prints `Installed: idle check (timer <active state>),
  patch at boot (<enabled state>)`.
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

**The `Installed:` lines go to the box's stdout, and so to `sync`'s stdout; the `install.sh:`
lines go to stderr.** Exit 0 means everything above took effect, and `sync` fails on any other
exit.

### The box's files

| Path on today's box | Put there by | What it is for |
|---|---|---|
| `/usr/local/sbin/agentspace-patch`, `/etc/systemd/system/agentspace-patch.service` (enabled) | `sync` | Patches the box at every boot, and leaves the marker `up` waits for |
| `/run/agentspace/patched` | The patch run | Exists once this boot's patch run has finished. Removed as each run starts, and gone at every boot |
| `/run/agentspace/last-active`, `/run/agentspace/busy` | The idle check | When the box was last active, and the CPU counter at the last check; gone at every boot, so each boot starts a fresh idle window |
| `/var/lib/agentspace/last-patch-reboot` | The patch run | Keeps the patch run from rebooting more than once per 15 minutes |
| `/usr/local/bin/agentspace-tools` | `sync` | Moves the login user's toolchain forward; the patch run starts it |
| `/usr/local/sbin/agentspace-idle-check`, `/etc/systemd/system/agentspace-idle.service`, `agentspace-idle.timer` (enabled, started) | `sync` | Powers the box off when idle (D11) |
| `/etc/agentspace/idle.conf` | `sync` | The idle limits: 60 minutes, and 6 CPU-seconds per 5-minute check |
| `/usr/local/bin/localswim` | `sync` | The wrapper agents run as `localswim`; it reads the credential file |
| `/usr/local/lib/localswim/localswim` | `sync`, built on the laptop | The real localswim CLI |
| `/usr/local/bin/agentspace-sample`, `/usr/local/bin/agentspace-footprint` | `sync` | Measure a turn (D33) |
| `/etc/ssh/agentspace_ca.pub` | `sync` | The CA sshd trusts for user certificates |
| `/etc/ssh/sshd_config.d/50-agentspace-ca.conf` | `sync` | Trusts the CA, and closes root login (D7) |
| `/etc/ssh/sshd_config.d/51-agentspace-logins.conf` | `sync` | Exempts 127.0.0.1 from login penalties, a 30-second login grace time, and a dead client dropped after about 90 seconds (D7, D12) |
| `~/.config/localswim/bot.env` (login user's, mode 0600, folder 0700 when created) | The token push | The board's URL and Bot token, for the wrapper |
| `~/.claude/CLAUDE.md` (login user's) | `sync` | A copy of the laptop's; MUST NOT be edited on the box |
| The system time zone | The token push | The laptop's zone (D32) |
| `/tmp/agentspace-system/`, `/tmp/localswim` | `sync`, briefly | Staging; removed once installed |

**The credential file is read by the wrapper as shell assignments**: it exports each line for one
run of the CLI and never prints them. A value that holds a character a POSIX shell reads as syntax
(a space, a quote, `$`, a backquote or a backslash, for some) is installed as sent and then read
differently by the wrapper; the push does not refuse it. The push refuses only empty values and a
CR, LF or NUL ([What the laptop reads](#what-the-laptop-reads)), and that is the rule to build.
Every value localswim makes today is safe to read that way, and whether the push SHOULD refuse
more is [Q9](#open-questions). The wrapper refuses with a message to stderr and exit 1 when the CLI is
not installed, when the credential file is missing, or when the board's address does not answer,
which happens whenever no connection holds the board's forwards (D12).

**Not installed by the operator:** the toolchain (gh, Node through fnm, Claude Code, Codex, uv),
which a person installs once per instance by running a bootstrap script over `ssh agentspace`.
No operator command runs it. Note on where this came from: the script is `box/bootstrap.sh` in the
prototype's checkout. The patch run's tools step reports, and continues, when the toolchain
is missing.

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
  - **`pending`** when a patch run is due or under way this boot.
  - **`unknown`** when no patch run is set up, such as before the first `sync`.

  On today's box the marker of a finished run is `/run/agentspace/patched`, the patch run is the
  unit `agentspace-patch.service`, enabled when set up, and the question is sent as the document
  `AWS-RunShellScript`, which runs as root. The command answers `patched` when the marker
  exists; otherwise `pending` when the unit is enabled, and `unknown` when it is not. The command
  sent MUST hold no comma, bracket or equals sign, which the AWS CLI's shorthand syntax for
  `--parameters` would split on, or MUST be passed in a form that is not split.
- **How the answer is read.** `up` polls the command's invocation once a second, for up to 20
  seconds. While its status is `Pending`, `InProgress` or `Delayed`, or the invocation cannot be
  read yet, it polls again. Any other status ends the poll, and the answer is the invocation's
  stdout, trimmed. **20 polls without an end count as `pending`.**
- **What counts as ready:** every answer but `pending`
  ([Starting the box](#starting-the-box-and-waiting-for-it) lists what that lets through).
- **What the box does at boot, as far as `up` can see:** the patch unit runs once, after the
  network is up, with a 20-minute limit. It removes the marker, upgrades every package
  (`dist-upgrade`, phased updates included) and snaps, reboots at most once per 15 minutes if a
  package asks for it, moves the toolchain forward, and then writes the marker. A failed run
  writes no marker, so `up` waits until its 15-minute deadline. The unit's own limit, 20 minutes,
  is longer than `up`'s wait, so a slow run that is still working also ends in `up`'s failure.

### Idle stop, as the laptop sees it

**The box powers itself off after 60 minutes with no SSH session, no Session Manager session, no
patch run and less than 6 CPU-seconds of work per 5-minute check (D11).** The instance's shutdown
behavior is stop, so the next connection starts it again. The stack's alarm stops it too after 3
hours averaging under 1% CPU, whatever sessions are open ([The stack](#the-stack)).

## The AWS side

**Nothing on this page names an account ID, a VPC, a subnet or an instance ID.** Account-specific
values live in the stack parameters file, `%USERPROFILE%\.agentspace\stack-parameters.json`, which is
never committed (D37; docs/state.md).

### The stack

**One CloudFormation stack, named `agentbox`, in `us-west-2`.** It keeps the project's first name,
because a stack cannot be renamed without replacing the instance and its disk. The
operator MUST use that name.

**Parameters:**

| Parameter | Type | Default | Set by | Notes |
|---|---|---|---|---|
| `InstanceType` | String | `m9g.xlarge` | The template | Changing it on an update stops and restarts the instance; the disk is kept. |
| `ImageId` | EC2 image ID | A pinned Ubuntu 26.04 LTS arm64 image | The template | MUST NOT be changed on a live stack: it replaces the instance and destroys its disk. |
| `SubnetId` | Subnet ID | None | The stack parameters file | A subnet that assigns a public IPv4 address at launch, as a default VPC's do. |
| `VpcId` | VPC ID | None | The stack parameters file | The subnet's VPC. |
| `RootVolumeGiB` | Number, at least 8 | `8` | The template | The image's minimum. Grown later with `grow`. |
| `SshPublicKey` | String | None | `deploy`, from `%USERPROFILE%\.ssh\id_ed25519.pub` | Installed for the login user at first boot only ([Q13](#open-questions)). |

**Resources:**
- **An IAM role** for EC2 with the AWS managed policy `AmazonSSMManagedInstanceCore`, and an
  instance profile for it. That is what lets Session Manager reach the box.
- **A security group with no inbound rules** and all outbound traffic allowed. Nothing reaches the
  box but Session Manager.
- **The instance**, in the subnet, with the role, the security group, shutdown behavior `stop`,
  and an encrypted `gp3` root volume of `RootVolumeGiB` that is deleted with the instance. Its
  first-boot user data updates and upgrades packages, installs `git`, `tmux`, `jq`, `ripgrep` and
  `unzip`, and adds `SshPublicKey` to the login user's authorized keys. Nothing in it runs again
  after the first boot.
- **A CloudWatch alarm**, `agentbox-idle-backstop`, on the instance's average `CPUUtilization`:
  under 1% for 36 periods of 5 minutes (3 hours), with missing data counted as not breaching, and
  the action of stopping the instance.

**Tags:** the stack is deployed with the tag `project=agentbox`. The instance carries `Name` and
`project`, both `agentbox`; the role carries `project`; the security group carries `Name`.

**Output:** `InstanceId`, the instance's ID. The operator reads it into its instance-ID cache.

**What the laptop's AWS identity needs:** rights to deploy and describe the stack and create its
IAM role (`CAPABILITY_IAM`) and pass it to the instance (`iam:PassRole`), to create and change the
stack's CloudWatch alarm, to describe, start and stop the instance, to describe and modify its
volume, and to use Session Manager (describe instance information, send a command, read its
invocation, start a session). The operator reads no credentials itself: every call uses whatever
the AWS CLI is signed in with. Today that is an administrator signed in with `aws login`, which
gives short-lived credentials, whose expiry is #0034's case ([AWS failures](#aws-failures)).

### Calls by command

**Every call passes `--region us-west-2`; the ones marked JSON also pass `--output json`.** Of
those, the output is read as [What is read from each answer](#calls-by-command) below says, and
the rest are checked by their exit code alone. `<id>` is the cached instance ID.

| Command | AWS CLI calls, in order |
|---|---|
| Any but `ca`, and the ProxyCommand, when the instance-ID cache is missing | `cloudformation describe-stacks --stack-name agentbox` (JSON) |
| `deploy` | `cloudformation deploy --stack-name agentbox --template-file %USERPROFILE%\.agentspace\operator\agentspace.yaml --capabilities CAPABILITY_IAM --no-fail-on-empty-changeset --tags project=agentbox --parameter-overrides SshPublicKey=<key> <each Name=value line>` (output passed through); then `cloudformation describe-stacks --stack-name agentbox` (JSON); then `sync`'s |
| `up`, `sync`: [start and wait](#starting-the-box-and-waiting-for-it) | `ec2 describe-instances --instance-ids <id>` (JSON); if stopped, `ec2 start-instances --instance-ids <id>` (JSON); if stopping, `ec2 wait instance-stopped --instance-ids <id>`, then `ec2 start-instances`; `ec2 describe-instances` again, for the launch time, only if it was `running` (FB-09); while waiting, every 5 seconds, `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON), and once that is `Online`, `ssm send-command --instance-ids <id> --document-name AWS-RunShellScript --comment "agentspace up: patch check" --parameters commands=<question>` (JSON) and up to 20 of `ssm get-command-invocation --command-id <command id> --instance-id <id>` (JSON) |
| `sync` | The above, then nothing more from AWS: the rest goes over SSH, and each ssh or scp makes its own nested `up`'s calls and a tunnel |
| `down` | `ec2 stop-instances --instance-ids <id>` (JSON) |
| `status` | `ec2 describe-instances --instance-ids <id>` (JSON); `ec2 describe-volumes --volume-ids <vol>` (JSON); if running, `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON) |
| `grow` | `ec2 describe-instances --instance-ids <id>` (JSON); `ec2 describe-volumes --volume-ids <vol>` (JSON); `ec2 modify-volume --volume-id <vol> --size <new>` (JSON) |
| `ca` | None |
| The ProxyCommand | `up`'s, then `ssm start-session --target <id> --document-name AWS-StartSSHSession --parameters portNumber=22`, the tunnel |

**Which failures are tolerated:** a failed `ssm get-command-invocation` is polled again, since the
invocation is often not readable for a moment after `send-command`; `ec2 wait instance-stopped`'s
exit code is not checked, since the start request after it is the check
([Starting the box](#starting-the-box-and-waiting-for-it)). Every other failed call fails the
command, under [AWS failures](#aws-failures).

**What is read from each answer** (B-32). Members are named by their path in the JSON the AWS CLI
prints; `[0]` is the first element of an array. Every other member is ignored, and the answers of
`ec2 start-instances`, `ec2 stop-instances` and `ec2 modify-volume` are not read beyond their exit
code.

| Call | Members read | Used for |
|---|---|---|
| `cloudformation describe-stacks` | `Stacks[0].Outputs`: the element whose `OutputKey` is `InstanceId`, its `OutputValue` | The instance-ID cache. No such element is the "no `InstanceId` output" failure ([Laptop files](#laptop-files)) |
| `ec2 describe-instances` | `Reservations[0].Instances[0]`: `State.Name`, `LaunchTime` (ISO 8601, compared in UTC), `InstanceType`, `RootDeviceName`, and in `BlockDeviceMappings` the element whose `DeviceName` equals `RootDeviceName`, its `Ebs.VolumeId` | The state, the 20-minute rule, `status`'s type, and the root volume's ID |
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
| The start-and-wait step (`up`, `sync`, the ProxyCommand's `up`) | `State.Name` in its first call; `LaunchTime` in its second, which runs only for a `running` box (FB-09) |
| `status` | `State.Name`, `InstanceType`, `RootDeviceName`, and the root volume's `Ebs.VolumeId` |
| `grow` | `State.Name`, `RootDeviceName`, and the root volume's `Ebs.VolumeId` |

A member a command reads that is missing, or not of the form above, fails the command with
`agentspace: Unexpected answer from ec2 describe-instances: no <member>.`, where `<member>` is its
path as this table writes it, such as `Reservations[0].Instances[0].LaunchTime`. When no element
of `BlockDeviceMappings` has the `RootDeviceName`, or it has no `Ebs.VolumeId`, `<member>` is
`Reservations[0].Instances[0].BlockDeviceMappings[].Ebs.VolumeId` (architect, 2026-11-19, for
E0.06-T03-S01).

**The same form holds for every call's members, with these exceptions** (architect, 2026-11-19,
for E0.06-T01-S03):
- **`cloudformation describe-stacks`:** a missing `Stacks[0]` or `Outputs`, or an `InstanceId`
  output whose value is not a string matching the instance-ID cache's schema, is the "no
  `InstanceId` output" failure, and no cache is written.
- **`ssm get-command-invocation`:** an answer with no string `Status` is not readable yet, and is
  polled again; one that has ended with no `StandardOutputContent` is the empty answer, which
  counts as ready ([Q6](#open-questions)).
- **`LaunchTime` with no UTC offset** is a member not of its form. A launch time in the future is
  less than 20 minutes ago, so the box is waited for.
- **The waits' timing:** the 5 seconds are a pause after each check; the 15-minute deadline is
  tested after each check that was not ready, and the last pause is cut short to end on it; the
  invocation is polled at once and then each second, at most 20 times.

## Requirements the prototype does not meet yet

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
| An empty instance-ID cache counts as missing, and a stack with no `InstanceId` output fails | Writes an empty cache, after which every command fails with a runtime error until `deploy` | [Q24](#open-questions) |
| `sync` with `ssh-keygen.exe` missing fails with `ca`'s step 1 message | No check; another error | [sync](#sync) |
| A program that needs no input gets an empty or closed stdin | The AWS CLI inherits `up`'s stdin, the SSH client's stream | [Streams](#streams) |
| An empty or missing URL or token, or one with a CR, LF or NUL, warns "Could not read" and sends nothing | Sends it. The box refuses an empty value, so the laptop warns "did not confirm"; with an LF the box installs the first two lines, a cut token, and the laptop warns "did not confirm"; a CR is removed by the box and the push is confirmed | [What the laptop reads](#what-the-laptop-reads) |
| `LOCALAPPDATA` unset or empty skips the push as a missing receipt does, and fails no command | Every command, `ca` included, most likely fails at start with its runtime's error: building the receipt's path from an unset or empty variable throws, and the script stops on any error | [The receipt](#the-receipt) |
| A receipt that could mean two things, or holds the wrong type, is refused: a JSON value that is not an object, `base` or `credential_directory` given twice under the same name, or a `base` or `credential_directory` that is not a string | Reads a one-element array as its element, takes the last of a member repeated under the same name, sends a number's text as the URL and an array's items joined by spaces, and reads an array of one string as the credential folder | [The receipt](#the-receipt); D20 |
| A receipt with no byte order mark is read as UTF-8, and every path is taken literally | Reads it in the system's ANSI code page, so non-ASCII text in it differs; takes `*`, `?`, `[` and `]` in the receipt's path and the credential folder as wildcards | [The receipt](#the-receipt), [The Bot credential](#the-bot-credential) |
| A receipt with bytes not valid in its encoding, or with text that is not JSON (a single-quoted string, an unquoted member name), is refused | Replaces each bad byte with U+FFFD and reads on; accepts single-quoted strings and unquoted member names | [The receipt](#the-receipt); D20 |
| A `bot.xml` that could mean two things is refused: two or more `Obj` elements, or a second `UserName` or `Password` element; so is plaintext that is not valid UTF-16 | Reads every `Obj` and sends their tokens joined by a space; takes the last of a repeated `Password`; accepts an unpaired surrogate in the token | [The Bot credential](#the-bot-credential); D20 |
| An empty or unreadable stamp makes the push due | Warns "Could not send" and pushes nothing, at every `up` until a `sync` | [The stamp](#the-stamp) |
| The push's 20 seconds cover the write to ssh's stdin | Starts the clock after the write; the content fits in a pipe's buffer, so a blocked write has not been seen | [What the laptop sends](#what-the-laptop-sends) |
| The AWS CLI, `ssh` and `scp` can be named by `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD`, and the push's zone by `AGENTSPACE_TIMEZONE`; a name that cannot be run fails, and never falls back | Reads none of them. It runs the AWS CLI and `ssh` by fixed paths, and its zone override can be reached only by loading its code | [The test seam](#the-test-seam); Q11 and Q12, closed |
| The ProxyCommand runs `up` in caller mode: the caller's progress lines and failure messages, nothing else on success, the tunnel's stderr withheld; backend detail only with `AGENTSPACE_DIAGNOSTIC`. A nested `up` prints `The host is ready.`, never `Ready: <id>` | Has no caller mode. Prints the instance ID (`Starting <id>...`, `Ready: <id>`, nested or not), EC2 states, Session Manager and patch messages, the push's progress line, and on a failure the AWS CLI's text inside a runtime's error record; passes the tunnel's stderr through | #0087; D1 ([What a caller sees](#what-a-caller-sees-d1-card-0087)) |
| The instance-ID cache is `%USERPROFILE%\.agentspace\instance-id.json`, in JSON (D40) | `.instance-id` in the checkout, the ID alone | D21; Q7, closed ([Laptop files](#laptop-files)) |
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

**What reading the code and the docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080) and its fixes in v0.04-T05, every question here is either closed or
marked "Later, not blocking" with its reason, and none blocks building a drop-in of contract v0.
Several need a run on the laptop, which no writer of this page could make (D34).

| | Question | Settled by |
|---|---|---|
| **Q1** | **Closed (E0.04-T05-S03, 2026-11-18).** Each usage error has its own `agentspace: ` message and exits 1, nothing prompts, and the order of the checks is given, in [Command line](#command-line). Exit 1 rather than the lease CLI's 2, because every usage error the prototype reports exits 1, its shell's binding errors and its `-Alias` refusals alike, and the conformance checks expect 1. The prototype's binding errors are its shell's own records, which [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists. | Closed |
| **Q2** | **Later, not blocking:** **Does `powershell -File agentspace.ps1 -?` print the script's comment-based help?** If it does, that is the prototype's only help, and the rewrite's texts above replace it. | The help texts the rewrite MUST print are on this page, so the prototype's own help is an observation, not a behavior to build. Later: a run on the laptop |
| **Q3** | **Later, not blocking:** **What exit code does the prototype's `deploy` give when its `sync` step fails?** It runs `sync` as a nested script in the same process. Two readings disagree: that the nested script's `exit 1` ends only the nested script, so `deploy` ends normally and a `-File` run exits 0; or that it ends the whole process with 1. | [deploy](#deploy) requires exit 1 either way, so only what the prototype does is open. Later: a run on the laptop |
| **Q4** | **Closed by D36 (architect, 2026-11-18): `deploy -Quiet` quiets its `sync` step** ([deploy](#deploy), step 6), as the `-Quiet` row of [Command line](#command-line) says of every command. The prototype runs `sync` without `-Quiet`, which [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists. | Closed, D36 |
| **Q5** | **Later, not blocking:** **Does #0034's mechanism also hit `ssm get-command-invocation` and `ec2 wait`?** The poll discards the CLI's stderr while every error is terminating; under Windows PowerShell 5.1 a stderr line from it (such as an invocation not yet registered) may throw and fail `up` with a raw error instead of polling again. | The behavior to build is stated: a failed poll is polled again, and the wait's exit code is not checked ([Calls by command](#calls-by-command)), so only what the prototype does is open. Later: a run on the laptop |
| **Q6** | **Later, not blocking:** **Is an empty or unexpected answer to the patch check meant to count as ready?** The prototype ends the wait on anything but `pending`, so a check that fails lets `up` print `Ready` before the patch run has finished, against the rule of [Starting the box](#starting-the-box-and-waiting-for-it). | That section states the behavior to build, today's, and names this as one of its three exceptions, so a stricter wait would be a later change. Later: the architect |
| **Q7** | **Closed by D21 (architect, 2026-11-18).** The instance-ID cache is laptop state, so it lives under `%USERPROFILE%\.agentspace` ([Laptop files](#laptop-files)); the prototype keeps it in the checkout, which [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists. The localswim CLI's build output (`.build\`) is not state, and MAY stay in the checkout. | Closed |
| **Q8** | **Closed by [docs/state.md](state.md#files-agentspace-reads-and-never-writes) (E0.04-T02-S01).** state.md lists the files the operator reads and agentspace never writes (localswim's receipt and `bot.xml`, the user's `id_ed25519.pub`, `CLAUDE.md`, `~/.ssh/config`'s `agentspace` entry), and `infra\local.parameters` in [Files in the checkout](state.md#files-in-the-checkout); it owns none of their formats. | Closed |
| **Q9** | **Later, not blocking:** **What characters can a localswim URL and Bot token contain, and what form MUST the URL have?** The box's wrapper reads the credential file as shell assignments, so a space, quote or `$` in either would break or change it, and it finds the board's address by taking the URL as `http://<host>:<port>...`. The push refuses an empty value and a CR, LF or NUL, and checks nothing else. | The push's refusal is the rule to build. localswim's CLI accepts `LOCALSWIM_URL` only as `http://127.0.0.1:<port>` or `http://localhost:<port>`, with a port from 1 to 65535, and a token made by localswim's documented procedure is 43 base64url characters, so no value localswim makes today holds a character the wrapper's shell reading would change. Later: the architect, with localswim |
| **Q10** | **Closed by D37 (architect, 2026-11-18).** ~~What are the rewrite's entry points called, and does it keep PowerShell-style parameters (`-GiB`, `-Alias`, `-Quiet`)?~~ The operator is `agentspace-operator.exe` and the ProxyCommand `agentspace-ssh-proxy.exe`, installed with the lease CLI and the keeper in `%USERPROFILE%\.agentspace\bin` ([Programs and install layout](#programs-and-install-layout)). Every command line keeps the style this page gives ([Argument style](#argument-style)). The lease alias files on the laptop that name `<checkout>\scripts\ssh-proxy.cmd` keep working by [ProxyCommand lines already on the laptop](#proxycommand-lines-already-on-the-laptop). | Closed |
| **Q11** | **Closed by [The test seam](#the-test-seam).** `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD` name the programs a check stands in for, and a stand-in gets the token only as the real `ssh` does. | Closed |
| **Q12** | **Closed by [The test seam](#the-test-seam).** `AGENTSPACE_TIMEZONE` gives the push a zone. | Closed |
| **Q13** | **Later, not blocking:** **What does `deploy` do when `id_ed25519.pub` has changed?** The key is in the instance's user data, which runs at first boot only, so the new key is never installed; and CloudFormation may stop and start the instance to change user data. | `deploy` passes the key as [deploy](#deploy) says, and what CloudFormation then does is AWS's behavior, not the operator's. Later: a deploy on the laptop, or AWS's documentation |
| **Q14** | **Later, not blocking:** **Does the box require IMDSv2?** The template sets no metadata options, so whether the instance refuses a metadata request without a session token comes from the image's defaults, and is unobserved. | No operator command and no box step on this page reads instance metadata, so the answer changes nothing a drop-in builds. Later: a look at the instance's metadata options |
| **Q15** | **Later, not blocking:** **SHOULD the region and the stack name be configurable?** Both are fixed in the prototype, in two places each (the operator and the ProxyCommand). | This page fixes both ([The stack](#the-stack)), which is what a drop-in builds; making them configurable is an addition. Later: the architect; v0.06 |
| **Q16** | **Later, not blocking:** **SHOULD `status` read the volume's type**, rather than print `gp3` as a constant? | [status](#status) states the constant, and the stack makes only `gp3` volumes ([The stack](#the-stack)). Later: the architect |
| **Q17** | **Later, not blocking:** **Does the rewrite's ProxyCommand still need the console-mode repair?** The damage came from Windows PowerShell 5.1 starting under ssh. A ProxyCommand that never starts it might not need the repair, but this page requires the outcome either way. | [The ProxyCommand](#the-proxycommand) requires the console's state, not the repair, so a drop-in meets it either way. Later: a run on the laptop |
| **Q18** | **Later, not blocking:** **What does the prototype's runtime write ahead of the push's content?** The box strips a UTF-8 BOM, and the laptop's expected hash ignores one, but whether one is sent today depends on the console's input encoding. | The push works with or without a BOM ([What the box does](#what-the-box-does)), and the rewrite SHOULD send none, so only the prototype's bytes are open. Later: a run on the laptop, with the box's input logged |
| **Q19** | **Later, not blocking:** **Is a silent skip right when localswim's receipt is missing?** Every other reason the push does not happen warns; this one prints nothing, so a laptop without localswim never learns why the board does not work on the box. | [What the laptop reads](#what-the-laptop-reads) states the behavior to build, the skip, and a warning would be an addition. Later: the architect |
| **Q20** | **Later, not blocking:** **SHOULD concurrent pushes be serialized?** Two `up`s at once both push and both write the stamp, with no lock. | Both pushes write the same token, so nothing breaks, and each costs only a login ([When it runs](#when-it-runs)). Later: the architect |
| **Q21** | **Later, not blocking:** **SHOULD `-Quiet` stop suppressing the push's warnings?** It suppresses them in the prototype, so a quiet `sync` hides a token that did not land. | The rule stands for contract v0: `-Quiet` suppresses them ([Messages](#messages); [What the laptop checks](#what-the-laptop-checks)), as the prototype does, and a drop-in builds that. The ProxyCommand never passes `-Quiet`, so a connection always shows them. Only whether a later contract changes the rule is open. Later: the architect |
| **Q22** | **Later, not blocking:** **SHOULD `-GiB` be checked before AWS is asked?** The prototype passes any integer through, and AWS refuses 0 and below. | [Command line](#command-line) checks its form only, and AWS refuses a size that is not larger before anything changes, which fails `grow` with its error ([grow](#grow)). Later: the architect |
| **Q23** | **Closed by D36 (architect, 2026-11-18): `grow` fails, exit 1, when the online filesystem step fails** ([grow](#grow), step 3), with a message saying that the volume's change stands and the filesystem grows at next boot. The prototype exits 0, which [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists. | Closed, D36 |
| **Q24** | **Later, not blocking:** **What exactly does the prototype print when the stack has no `InstanceId` output?** It writes an empty cache, and this page requires otherwise ([Laptop files](#laptop-files)). | The behavior to build is stated there, so only the prototype's text is open. Later: a run on the laptop |
| **Q25** | **Closed by D36 (architect, 2026-11-18): a failed removal of the box's staging folder fails `sync` before the copy** ([sync](#sync), step 3). Read from the code: the prototype does not check the removal, and a partly removed folder takes the copy one level down, so the install runs whatever install script the removal left, and `sync` can succeed having installed the previous `box/system`. [Requirements the prototype does not meet yet](#requirements-the-prototype-does-not-meet-yet) lists it. | Closed, D36 |
| **Q26** | **Later, not blocking:** **Does the sign-in test catch every form of the AWS CLI's expired-login error?** The prototype matches `expired`, `aws login` or `Unable to locate credentials`. The exact text of the CLI's error after an `aws login` session expires was not recorded with #0034. | [AWS failures](#aws-failures) states the test to build, and a form it misses still fails the command, with the CLI's own text. Later: a run on the laptop with an expired sign-in |
| **Q27** | **Later, not blocking:** **SHOULD the stamp stay an unsalted hash that covers the token?** It is kept on the laptop, and anyone who can read it can test a guessed token against it. A keyed digest would hide that, but the stamp MUST still change whenever the instance, the zone, the URL or the token does. Its storage is docs/state.md's. | [The stamp](#the-stamp) states the hash to build, and a later change of it costs one push per laptop, since a stamp that does not match only makes the push due. Later: the architect; docs/state.md |
| **Q28** | **Closed by [What the laptop reads](#what-the-laptop-reads) (E0.04-T05-S01, #0088, 2026-11-18).** It specifies the receipt and `bot.xml` to the byte: where each is, its encoding, the receipt's two members, the CLIXML elements down to the `SS` element that holds the token, the hex, the `CryptUnprotectData` call, and the UTF-16LE plaintext, with what each failure does. It cites localswim's docs where they define a part, and says that the rest is what agentspace relies on without localswim documenting it, observed on throwaway files with a dummy secret. | Closed |
| **Q29** | **Later, not blocking:** **Does D1 at connect time cover #0034's sign-in message?** The message names AWS and its login command ([AWS failures](#aws-failures)), and a lease's caller who sees it through the ProxyCommand learns the backend ([What a caller sees](#what-a-caller-sees-d1-card-0087)). Whether it MUST be reworded inside the ProxyCommand, with the AWS text behind `AGENTSPACE_DIAGNOSTIC`, is the architect's call. | Contract v0 keeps it as the one exception (A-14): v0's caller runs on the laptop (D8), whose owner already knows the backend and is the one who can sign in again, so it tells nobody anything new. Later: the architect, with a caller off the laptop (#0011) |
| **Q30** | **Later, not blocking:** **SHOULD the seam also cover `go`, `ssh-keygen`, `icacls` and `taskkill`?** Without it, a check cannot run `sync`'s build step or `ca`'s lockdown against a stand-in; `sync` runs its build when `go` and the localswim checkout are present, and skips it otherwise. | A check can still reach those steps without a stand-in: `sync` skips its build when `go` or the localswim checkout is missing, and `ca` runs against a throwaway home. Later: the architect |

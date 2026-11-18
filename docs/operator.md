# The agentspace operator, reference v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for the operator: the laptop-side commands that manage the box (`deploy`, `up`,
`sync`, `ca`, `down`, `status`, `grow`), the SSH ProxyCommand, and the localswim token push.** It
is written so that the operator can be rebuilt without reading any of its code (D20, D34). It names
every command, parameter, message, stream and exit code, the protocol of the token push, the box
as the laptop sees it, and the AWS calls each command makes. The reasoning behind each rule is in
[decisions.md](decisions.md), cited as `D<n>`.

**What it does not own:**
- **The lease CLI** (`agentspace lease ...`) is [lease-api.md](lease-api.md)'s. The lease CLI
  calls two things this page owns: `ca`, as its CA check ([ca](#ca)), and the ProxyCommand, which
  every lease alias runs ([The ProxyCommand](#the-proxycommand)).
- **Every file the operator reads or writes on the laptop is [docs/state.md](state.md)'s**: its
  location, format and access rules. This page names each file, says when it is read or written,
  and cites that doc for the rest ([Laptop files](#laptop-files)).
- **The box's own code stays shell (D28).** This page states the contract between the laptop and
  the box: what is sent, what is installed, what is asked and what is answered. It does not
  specify how the box's scripts do it.

**The operator is EC2-specific in v0.** Unlike the lease CLI, it names the backend: it manages
one EC2 instance through one CloudFormation stack (D8, D9). D1 binds what a lease's client sees,
and the operator is not such a client.

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
  - [Laptop files](#laptop-files)
  - [The `agentspace` SSH entry](#the-agentspace-ssh-entry)
  - [Every ssh runs the ProxyCommand](#every-ssh-runs-the-proxycommand)
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
  `<checkout>` is the folder of the agentspace repository that holds the operator.
- **The box is a host, and a host's platform is a parameter of the contract (D4).** Today there is
  one host: an EC2 instance running Ubuntu on `linux/arm64`, logged in to as `ubuntu`, with
  systemd, bash and passwordless `sudo`. Wherever this page names one of those, it is a property
  of today's box, marked "today's box", and not of the contract. A Windows host (v0.13, #0081)
  would answer the same questions in its own way.
- **The prototype is `scripts/agentspace.ps1`, run by Windows PowerShell 5.1, and
  `scripts/ssh-proxy.cmd`.** They are named here only where a note says where a requirement came
  from. The rewrite replaces both in v0.06 (#0055, D28).

### Command line

```
operator <command> [-GiB <n>] [-Alias <alias>] [-Quiet]
operator -h | --help
operator <command> -h | --help
```

**`operator` stands for the operator's entry point.** Today that is `powershell -NoProfile -File
<checkout>\scripts\agentspace.ps1`, and the help texts below write it `agentspace.ps1`. What the
rewrite's executable is called is open ([Q10](#open-questions)).

- **`<command>` is the first positional argument** and MUST be one of `deploy`, `up`, `sync`,
  `ca`, `down`, `status` and `grow`. It is matched without regard to case.
- **Parameter names are matched without regard to case**, and a value follows its name as the next
  argument (`-GiB 4`). The rewrite MAY accept other spellings the prototype's shell accepts (a
  unique prefix such as `-Q`, or `-GiB:4`), and a caller MUST NOT depend on them.
- **A missing or unknown command, an extra positional argument, an unknown parameter, a
  parameter given twice, or a parameter with no value, is a usage error**
  ([Exit codes](#exit-codes)). The operator MUST NOT prompt for a missing command, because it runs
  where nobody can answer (inside a ProxyCommand, and under the lease CLI). The prototype's shell
  most likely prompts instead ([Q1](#open-questions)).
- **The alias check is part of the command line**, and runs before anything else, as below.

| Parameter | Type | Default | Applies to | Notes |
|---|---|---|---|---|
| `-GiB <n>` | Integer | `2` | `grow` | The GiB to add to the root volume. Accepted and ignored by every other command. No range is checked; AWS refuses a size that is not larger ([grow](#grow)). |
| `-Alias <alias>` | SSH host alias | `agentspace` | `up` | The SSH host alias the token push connects with (D9, card #0057). Any other command given `-Alias` fails. |
| `-Quiet` | Switch | Off | Every command | Suppresses every message that is not a failure and not `status`'s report, warnings included ([Messages](#messages)). |
| `-h`, `--help` | Switch | | Every level | Prints the help of the level it is at to stdout and exits 0, doing nothing else. The prototype has no help of its own ([Q2](#open-questions)); the texts below are what the rewrite MUST print. |

**`-Alias` is checked before anything else runs**, for every command:
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
  `status`'s report alike. This is what makes `up` safe inside a ProxyCommand, where stdout is the
  SSH connection ([The ProxyCommand](#the-proxycommand)).
- **`up` MUST write nothing to stdout**, and neither MUST anything it starts. The ProxyCommand
  also sends `up`'s stdout to stderr, as a second guard.
- **`up` MUST NOT read its stdin, and MUST NOT let anything it starts read it.** Inside a ProxyCommand,
  stdin carries the SSH client's bytes for the server, and a byte read there is lost to the
  connection. Every program the operator starts that needs input gets its own pipe.
- **Output of programs the operator runs is passed through as they write it**, for `deploy`,
  `sync` and `grow`: the AWS CLI's `cloudformation deploy` output, `go build`'s, the box's
  `install.sh` lines and `grow`'s `df` table reach the operator's stdout or stderr as those
  programs wrote them. A caller MUST NOT parse that output; it is for a person.
- **Help goes to stdout**, with exit 0.

### Exit codes

| Exit | When |
|---|---|
| 0 | The command did what was asked. Warnings from the token push do not change it. |
| 1 | Any failure, usage errors included. The last message on stderr says what failed. |

**No other exit code MAY be used by `agentspace.ps1`'s commands.** The ProxyCommand has its own
rule: [The ProxyCommand](#the-proxycommand). Unlike the lease CLI, the operator gives usage errors
exit 1, as the prototype does for `-Alias` ([Q1](#open-questions)).

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
  print. Output passed through from other programs is not affected by `-Quiet`.
- **Placeholders in this page's message texts:** `<id>` is the instance ID, `<vol>` a volume ID,
  `<fp>` a key fingerprint (`SHA256:...`), and `<user>` the Windows user name. Other placeholders
  are named where they appear.
- **Messages carry the instance ID and volume IDs.** A check that captures the operator's stderr
  captures them, and MUST withhold them from anything committed or reported (D9's 0.02.T2.1
  finding). The repository is public.

### Programs it starts

**Card #0027 (ssh and scp by full path) is carried in here as a requirement.**
- **Every `ssh` and `scp` the operator starts MUST be Windows OpenSSH's, by full path:**
  `%SystemRoot%\System32\OpenSSH\ssh.exe` and `scp.exe`. It MUST NOT be found through `PATH`.
  Started from Git Bash, `PATH` finds MSYS's ssh, which runs the ProxyCommand through `/bin/sh`
  and mangles its Windows path (`exec: C:WindowsSystem32cmd.exe: not found`).
- **So `up`, `sync`, `status` and `grow` MUST work when the operator is started from Git Bash.**
  That is #0027's done-when, and this requirement closes it once the rewrite meets it.
- **Source:** the prototype calls the token push's ssh and the CA key's scp by full path, and
  every other ssh and scp by bare name (D9's E2.01 finding).

**The other programs, and how each is found:**

| Program | Found as | Used by |
|---|---|---|
| Windows OpenSSH `ssh.exe`, `scp.exe` | `%SystemRoot%\System32\OpenSSH\`, by full path (#0027) | `sync`, `grow`, the token push |
| `ssh-keygen.exe` | `%SystemRoot%\System32\OpenSSH\ssh-keygen.exe` | `ca`, and `sync`'s CA check |
| AWS CLI v2 | `C:\Program Files\Amazon\AWSCLIV2\aws.exe`; when that file is missing, `aws` on `PATH` | Every command but `ca` |
| Session Manager plugin | Found by the AWS CLI on `PATH`; the ProxyCommand puts `C:\Program Files\Amazon\SessionManagerPlugin\bin` first | The ProxyCommand's tunnel |
| `icacls.exe`, `taskkill.exe` | `%SystemRoot%\System32\`, by full path | `ca`; the token push's time limit |
| `go` | On `PATH` | `sync`, to build the localswim CLI |
| `powershell.exe` (Windows PowerShell 5.1) | On `PATH` | The prototype's ProxyCommand, to run `up` |

**No secret is ever among any program's arguments** ([The token push](#the-token-push)).

### AWS failures

**Card #0034 (an expired aws login gets agentspace's own message) is carried in here as a
requirement.**
- **When an AWS CLI call fails because the sign-in has expired or is missing, the operator MUST
  fail with exactly:**

  ```
  agentspace: AWS sign-in has expired. Run 'aws login' in a fresh Incognito window, then retry.
  ```

  A failure is a sign-in failure when the CLI's error text matches `expired`, `aws login` or
  `Unable to locate credentials`, without regard to case. That is the prototype's test; whether
  it catches every form of the CLI's expired-login error is [Q26](#open-questions).
- **Any other AWS CLI failure MUST fail with `agentspace: ` followed by the CLI's own error text**,
  trimmed, and never with a runtime's error record or stack in its place.
- **This MUST hold inside the ProxyCommand**, so that `ssh agentspace`, `ssh <lease alias>` and the
  Claude desktop app show the sign-in message, not a raw error, before ssh's own `Connection
  closed by UNKNOWN port 65535`.
- **It MUST hold for every AWS call whose failure fails the command**, `cloudformation deploy`
  included. The calls whose failure is retried or ignored are listed in
  [Calls by command](#calls-by-command).
- **Source:** in the prototype, the AWS CLI's stderr is merged into its output while every error
  is terminating, so under Windows PowerShell 5.1 the CLI's first stderr line throws before the
  exit code is checked, and the message above never prints (#0034; D9's E2.02 finding). The same
  mechanism may reach two more calls ([Q5](#open-questions)).

**Every AWS CLI call the operator makes passes `--region us-west-2`, and every call whose output it
reads also passes `--output json`.** The region is today's, and not configurable
([Q15](#open-questions)). The calls are listed per command in [The AWS side](#the-aws-side).

### Environment variables

| Variable | Read or set | Meaning |
|---|---|---|
| `AGENTSPACE_NESTED` | Read at start; set to `1` for everything the operator starts | When it is set to a non-empty value as the operator starts, `up` does not push the token. Every ssh the operator starts runs the ProxyCommand and so `up` again ([below](#every-ssh-runs-the-proxycommand)), and this keeps that nested `up` from pushing a second time. |
| `*_POSIX_FD_STATE` | Removed at start | Windows OpenSSH gives each process it starts, the ProxyCommand included, a variable named `<GUID>_POSIX_FD_STATE`. Inherited by an ssh that the operator starts, it makes that ssh never read its stdin, so the token push never sent its input and hung (card #0026, D9's E3.02 finding). **The operator MUST remove every variable whose name ends `_POSIX_FD_STATE`, whatever the GUID, before it starts any program.** |
| `GOOS`, `GOARCH`, `CGO_ENABLED` | Set for `go build` only | `linux`, `arm64` and `0` while `sync` builds the localswim CLI, and removed afterwards. `linux/arm64` is today's box's platform. |
| `USERPROFILE`, `LOCALAPPDATA`, `SystemRoot` | Read | Where the laptop files and Windows programs are. |
| `OneDrive`, `OneDriveConsumer`, `OneDriveCommercial` | Read by `ca` | Folders the CA MUST NOT be inside (D17). |
| `USERNAME`, `COMPUTERNAME` | Read by `ca` and `sync` | The user named in the CA's messages, and the computer named in a new CA's key comment (docs/state.md). |
| `PATH` | Read; the ProxyCommand prepends two folders | Finding `go`, the AWS CLI's fallback, and the Session Manager plugin. |

**No environment variable carries a secret, and the token MUST NOT be put in one.**

### Laptop files

**Every file here is [docs/state.md](state.md)'s**: its exact location, format and access rules.
This table says only which command reads or writes it.

| File (as today's prototype has it) | Read by | Written by | What it is |
|---|---|---|---|
| `<checkout>\.instance-id` | Every command but `ca`; the ProxyCommand | `deploy`; any command that finds it missing | The instance ID, cached from the stack's output ([Q7](#open-questions)) |
| `%USERPROFILE%\.agentspace\localswim-pushed.sha256` | `up` | `up`, `sync` | The push stamp ([The stamp](#the-stamp)) |
| `%USERPROFILE%\.agentspace\ca\agentspace_ca` | `ca`, `sync` | `ca`, once | The CA's private key (D17) |
| `%USERPROFILE%\.agentspace\ca\agentspace_ca.pub` | `ca`, `sync` | `ca`, once | The CA's public key, which `sync` sends to the box |
| `%USERPROFILE%\.ssh\id_ed25519.pub` | `deploy` | Never | The user's own public key, passed to the stack as `SshPublicKey` |
| `%USERPROFILE%\.ssh\config` | The ssh client, for the `agentspace` entry and lease aliases | Never | [The `agentspace` SSH entry](#the-agentspace-ssh-entry) |
| `%USERPROFILE%\.claude\CLAUDE.md` | `sync` | Never | The user's global instructions for Claude Code, copied to the box |
| `<checkout>\infra\local.parameters` | `deploy` | Never | The account-specific stack parameters, which are not committed |
| `<checkout>\infra\agentspace.yaml` | `deploy` | Never | The stack's template ([The stack](#the-stack)) |
| `<checkout>\box\system\` | `sync` | Never | The box's system files, sent whole |
| `<checkout>\.build\localswim-linux-arm64` | `sync` | `sync` | The localswim CLI built for today's box |
| `C:\Projects\localswim` | `sync` | Never | The localswim checkout the CLI is built from |
| `%LOCALAPPDATA%\localswim-v2\service.json` and the `bot.xml` it points to | `up`, `sync` | Never | The localswim service's receipt and its DPAPI-protected Bot credential ([The token push](#the-token-push)) |

**The token MUST NOT be written to any laptop file** by the operator. It lives only in
localswim's own protected credential, and the stamp holds a hash that covers it.

### The `agentspace` SSH entry

**The operator needs one SSH host alias in the user's OpenSSH client configuration: `agentspace`.**
`sync`, `grow` and the token push of `sync` connect through it, and so does any `up` started
without `-Alias`. The configuration file is docs/state.md's; what the operator needs from the
entry is this:

- **It reaches the box through the ProxyCommand with no argument**:
  `C:\Windows\System32\cmd.exe /d /c "<checkout>\scripts\ssh-proxy.cmd"`.
- **It logs in as the box's login user with a key that the box's first boot installed**
  (`SshPublicKey`, [The stack](#the-stack)). Today that is `ubuntu` with `~/.ssh/id_ed25519`.
- **The login user MUST be able to run `sudo` without a password or a terminal**, because `sync`
  and `grow` run `sudo` over a non-interactive ssh, and the push runs `sudo -n`. That is a
  property of today's box.
- **It SHOULD carry the board's two `RemoteForward` lines** (D12). The operator's own ssh and scp
  calls clear them ([below](#every-ssh-runs-the-proxycommand)), so they apply only to a person's
  interactive session.

The whole entry as it stands today is in the README, "Use it". A lease alias is the lease CLI's
(docs/state.md, [lease-api.md](lease-api.md#the-credential)), and runs the same ProxyCommand with
the alias after it.

### Every ssh runs the ProxyCommand

**Every ssh and scp the operator starts goes through a host alias, and so runs the ProxyCommand,
which runs `up` again.** That nested `up` is a full `up` but for the push: it reads the instance
state, starts the box if needed, waits, and prints `Ready: <id>` to stderr. So a `sync` prints one
`Ready: <id>` line per ssh or scp it starts, and each one costs a Session Manager session.

- **Every ssh and scp the operator starts MUST pass `-o ClearAllForwardings=yes`**, so it never
  takes the board's forwarded ports from a person's interactive session (D12; README, "The
  localswim board on the box").
- **The nested `up` MUST NOT push the token** (`AGENTSPACE_NESTED`,
  [Environment variables](#environment-variables)).

## Commands

### The top level

```
operator <command> [-GiB <n>] [-Alias <alias>] [-Quiet]
operator -h | --help
```

**Help:**

```
Usage: agentspace.ps1 <command> [-GiB <n>] [-Alias <alias>] [-Quiet] [--help]

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

Every message goes to stderr. up writes nothing to stdout, so it is safe in an
SSH ProxyCommand.

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
   unreadable or empty file MUST fail with an `agentspace: ` message naming it. The prototype fails
   with a runtime error for a missing file, and passes an empty key on to the stack.
2. **Reads `<checkout>\infra\local.parameters`.** A missing file fails with `agentspace: Missing
   <path>; copy infra\local.parameters.example and fill it in.`, where `<path>` is the file's
   full path. Each line of the form `<Name>=<value>` becomes a parameter override, and every
   other line, such as a comment, is skipped (format: docs/state.md). Any stack parameter can be set
   there, not only `VpcId` and `SubnetId`, so `ImageId` MUST NOT be set there on a live stack.
3. **Prints `Deploying stack 'agentbox' in us-west-2...`.**
4. **Runs `cloudformation deploy`** ([The AWS side](#calls-by-command)), with its output passed
   through. A non-zero exit fails with `agentspace: Stack deploy failed; see the stack events in
   the console.` An expired sign-in MUST fail with #0034's message instead
   ([AWS failures](#aws-failures)).
5. **Deletes the instance-ID cache, reads the instance ID from the stack's `InstanceId` output,
   writes the cache again**, and prints `Instance: <id>`.
6. **Runs `sync`**, as if `operator sync` had been run. **`deploy` MUST exit 1 when `sync`
   fails.** The prototype runs `sync` as a nested script, whose failure most likely does not
   reach `deploy`'s exit code ([Q3](#open-questions)). Whether `-Quiet` reaches `sync` is
   [Q4](#open-questions).

**Stdout:** the AWS CLI's deploy output, then `sync`'s. **Stderr:** the messages above, then
`sync`'s. **Exit codes:** 0 and 1.

**`deploy` passes no `ImageId`, so the template's pinned default applies.** The template's
`ImageId` default MUST NOT be changed while the stack is live: CloudFormation replaces the instance
to change its image, and its disk with it (README, "Rules").

**Help:**

```
Usage: agentspace.ps1 deploy [-Quiet]

Creates or updates the CloudFormation stack 'agentbox' in us-west-2 from
infra\agentspace.yaml, with the parameters in infra\local.parameters and your
~/.ssh/id_ed25519.pub, then runs sync.

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
3. **Pushes the token over `<alias>`**, as [The token push](#the-token-push) says, unless
   `AGENTSPACE_NESTED` was set when the operator started. The push only warns; it never fails
   `up`.

**Stdout:** nothing, ever. **Stderr:** the messages of steps 2 and 3, and the stderr of the push's
ssh. **Exit codes:** 0 once the box is ready, whatever the push did; 1 on any failure before that.

**Help:**

```
Usage: agentspace.ps1 up [-Alias <alias>] [-Quiet]

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
   with that check's message before the box is touched. A missing, half-made, mismatched or
   loosened CA never becomes a host's trusted CA (D17).
2. **Starts the box and waits for it** ([Starting the box](#starting-the-box-and-waiting-for-it)).
3. **Prints `Copying box/system to the instance...`**, removes the box's staging folder
   (`/tmp/agentspace-system` on today's box), and copies `<checkout>\box\system` there, whole and
   recursively. A failed copy fails with `agentspace: Copy to the instance failed.` A failed
   removal is not checked by the prototype; the copy or the install then fails instead.
4. **Prints `Copying the agentspace SSH CA's public key (<fp>) to the instance...`** and copies
   the CA's public key into the staging folder as `agentspace_ca.pub`. A failure fails with
   `agentspace: Copying the CA's public key to the instance failed.`
5. **Runs the install as root on the box and then removes the staging folder** (today `sudo bash
   /tmp/agentspace-system/install.sh && rm -rf /tmp/agentspace-system`), with its output passed
   through. A non-zero exit fails with `agentspace: Install on the instance failed.` What the
   install does is in [What sync installs](#what-sync-installs).
6. **If `%USERPROFILE%\.claude\CLAUDE.md` exists**, prints `Copying ~/.claude/CLAUDE.md to the
   instance...`, makes sure the login user's `~/.claude` exists on the box, and copies the file to
   `~/.claude/CLAUDE.md` there, replacing the box's copy. A failed copy fails with
   `agentspace: Copying CLAUDE.md to the instance failed.` The laptop's copy is the source, and
   the box's copy MUST NOT be edited (README, "Rules").
7. **Builds and installs the localswim CLI**, when the laptop can:
   - **When `go` is not on `PATH`, or `C:\Projects\localswim` is missing**, it prints `Skipping
     the localswim CLI: Go or C:\Projects\localswim is missing on the laptop.` and goes on to
     step 8. This is not a failure.
   - **Otherwise it prints `Building the localswim CLI for linux/arm64...`** and builds
     `./cmd/localswim` from `C:\Projects\localswim` for `linux/arm64` with cgo off, trimmed paths,
     into `<checkout>\.build\localswim-linux-arm64`. A failed build fails with `agentspace:
     Building the localswim CLI failed.` `linux/arm64` is today's box's platform.
   - **Prints `Copying the localswim CLI to the instance...`**, copies the binary to the box's
     staging path (`/tmp/localswim` today), and fails with `agentspace: Copying the localswim CLI
     failed.` if that fails.
   - **Installs it as root, mode 0755, at `/usr/local/lib/localswim/localswim`**, and removes the
     staged copy. A failure fails with `agentspace: Installing the localswim CLI failed.`
8. **Pushes the token over `agentspace`, forced**: the stamp is not compared, so the push always
   runs ([The token push](#the-token-push)). A failed push warns and does not fail `sync`.

**Stdout:** the install's output, `go build`'s, and whatever else the remote commands print.
**Stderr:** the messages above, one `Ready: <id>` per nested ssh or scp, and the push's.
**Exit codes:** 0 and 1.

**After the localswim service on the laptop is updated, `sync` MUST be run again**, so the box's
CLI speaks the same API (README, "The localswim board on the box").

**Help:**

```
Usage: agentspace.ps1 sync [-Quiet]

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

**The lease CLI runs `ca` as its CA check before it signs a lease's key**, and reads only its exit
code: 0 means the CA is usable (prototype source: `scripts/lease-credential.ps1`). So `ca` on an
existing, sound CA MUST exit 0 and change nothing.

**In order, it:**
1. **Fails if `ssh-keygen.exe` is missing:** `agentspace: <path> is missing; install the Windows
   OpenSSH client.`
2. **Fails if the CA's folder is inside a OneDrive folder or inside `<checkout>`**, compared as
   paths without regard to case: `agentspace: <ca folder> is inside <folder>; the CA key MUST NOT
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
5. **Checks the CA** (the same checks `sync` makes):
   - **Both files exist**, or it fails with `agentspace: <key> does not exist; run ca.` or
     `agentspace: <pub> does not exist; run ca.`
   - **The folder, then the private key, are the current user's alone**: owned by the user, not
     inheriting access rules, and with exactly one rule, which allows the user full control. The
     first that is not fails with:

     ```
     agentspace: <path> MUST be <user>'s alone, but it <problems>. ca does not change access rules on an existing CA. If anyone else may have read the key, replace the CA (D17). Otherwise lock it by hand: icacls "<ca folder>" /inheritance:r /grant:r "<user>:(OI)(CI)F", then icacls "<key>" /reset, then icacls "<key>" /inheritance:d, then remove any other rule icacls still lists, and run ca again.
     ```
   - **The private key reads with an empty passphrase**, or it fails with `agentspace: ssh-keygen
     could not read <key> without a passphrase; the lease CLI signs unattended (D17).`
   - **The public key is `ssh-ed25519` and is the private key's own**, or it fails with
     `agentspace: <pub> is not the ed25519 public key of <key>. ca does not repair a mismatched
     pair.`
   - **The public key's fingerprint can be read**, or it fails with `agentspace: ssh-keygen could
     not read <pub>: <tool output>`.
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

**`ca` MUST NOT overwrite, repair or loosen a CA, and MUST NOT change the access rules of an
existing one.** A person decides what to do; if the private key may have been exposed, the CA is
replaced: delete both files, run `ca`, run `sync` for every host, and re-issue every live lease's
certificate (D17).

**Stdout:** nothing. **Stderr:** the messages above. **Exit codes:** 0 and 1.

**Help:**

```
Usage: agentspace.ps1 ca [-Quiet]

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
Usage: agentspace.ps1 down [-Quiet]

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
Usage: agentspace.ps1 status [-Quiet]

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
3. **When the instance is running**, prints `Growing the filesystem online...`, waits 10 seconds,
   and over `agentspace` grows the root partition and its filesystem and shows the disk (today's
   box: `sudo growpart /dev/nvme0n1 1; sudo resize2fs /dev/nvme0n1p1 && df -h /`), with the output
   passed through. The prototype does not check that command's exit code, so `grow` exits 0 even
   when the filesystem did not grow ([Q23](#open-questions)).
4. **Otherwise** prints `The filesystem grows on its own at next boot.` and does not start the
   box.

**A volume never shrinks**, and EBS allows one change per volume every 6 hours. The disk SHOULD
stay above about 70% used, grown in small steps once it passes 85% (README, "Rules").

**Stdout:** the remote command's output. **Stderr:** the messages above, and `Ready: <id>` from
the nested ssh. **Exit codes:** 0 and 1.

**Help:**

```
Usage: agentspace.ps1 grow [-GiB <n>] [-Quiet]

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
     <id>...` and asks EC2 to start it. The prototype does not check the wait's exit code, so a
     wait that gives up shows only as the start request's failure.
   - **Any other state** fails with `agentspace: Instance <id> is <state>.`
2. **Decides whether to wait.** It waits unless the state read in step 1 was `running` and the
   instance's launch time, read again now, is 20 minutes ago or more. EC2 resets the launch time at
   every start, and the patch unit's own time limit is 20 minutes, so such a box has finished its
   boot-time patch run, or failed it, long ago.
3. **Waits**, for at most 15 minutes from the start of the wait, checking every 5 seconds:
   - **Is Session Manager's agent `Online`?** If not, the message is `Waiting for Session
     Manager...`.
   - **If it is, asks the box whether its patch run has finished**
     ([What up waits for](#what-up-waits-for)). An answer other than `pending` ends the wait. A
     `pending` answer gives the message `Patching Ubuntu (dist-upgrade, reboot if the kernel
     changed)...`.
   - **A message is printed only when it differs from the last one printed**, so a long wait shows
     each phase once.
   - **At the deadline** it fails with `agentspace: <id> was not ready within 15 minutes; check:
     ssh agentspace journalctl -u agentspace-patch`.
4. **Prints `Ready: <id>`.**

**`up` MUST NOT print `Ready` before the box's patch run has finished** (README, "Rules"), within
the two exceptions above: a box running for 20 minutes is not asked, and an answer of `unknown`
counts as finished ([What up waits for](#what-up-waits-for)).

## The ProxyCommand

**The ProxyCommand is what makes `ssh agentspace`, and `ssh <lease alias>`, start the box and reach
it with no inbound port.** ssh runs it in place of a TCP connection, and talks SSH over its stdin
and stdout. D9 makes this path the EC2 driver's.

**How ssh runs it.** The entry's `ProxyCommand` line is, today:

```
C:\Windows\System32\cmd.exe /d /c "<checkout>\scripts\ssh-proxy.cmd" [<alias>]
```

Terry's `agentspace` entry passes no alias, and a lease alias passes its own name, written out
literally, never as ssh's `%n` token (README, "Lease host aliases"; the alias file is docs/state.md's).
The Claude desktop app runs the same line through its own proxy program (D12's E3.03 finding).

**What it does, in order:**
1. **Refuses a second argument** before anything starts: it prints `agentspace: ssh-proxy.cmd
   takes at most one argument, the SSH host alias.` to stderr and exits 1. An empty first argument
   (`""`) is an argument, and reaches step 3.
2. **Puts the Session Manager plugin's folder and the AWS CLI's folder first on `PATH`.**
3. **Runs `up`**, with `-Alias <alias>` when it was given an alias, and with `up`'s stdout sent to
   stderr. `up` checks the alias exactly ([Command line](#command-line)), so a bad one fails there
   before any ssh or tunnel.
4. **If `up` exited non-zero, exits 1**, starting no tunnel.
5. **Reads the instance ID from the instance-ID cache** (docs/state.md), which `up` has just made
   sure exists.
6. **Runs the tunnel**, with the ProxyCommand's own stdin and stdout:

   ```
   "C:\Program Files\Amazon\AWSCLIV2\aws.exe" ssm start-session --region us-west-2 --target <id> --document-name AWS-StartSSHSession --parameters portNumber=22
   ```

   and exits with the tunnel's exit code.

**Exit codes:** 1 for a second argument or a failed `up`; otherwise the tunnel's. ssh ends the
ProxyCommand when the connection closes, so a caller of ssh rarely sees it: ssh exits 255 with
`Connection closed by UNKNOWN port 65535` when the ProxyCommand fails before the tunnel (D9's
0.02.T2.1 finding).

**Why every message goes to stderr.** The ProxyCommand's stdout is the SSH connection: ssh reads
every byte of it as the server's side of the protocol. A single byte of a message there corrupts
the connection, so **nothing but the tunnel MUST write to the ProxyCommand's stdout**. stderr goes
to ssh's stderr, which a person at a terminal sees. The Claude desktop app's Test connection does
not show it, so a push from the app MUST be confirmed on the box or by the stamp, never from the
app's output (D9's E0.02-T03-S01 finding).

**What it MUST NOT do:**
- **Read its stdin before the tunnel starts.** Those bytes are the SSH client's, for the server.
- **Leave the console broken.** When the ProxyCommand's stderr is a Windows console, the
  console's output mode MUST, when the ProxyCommand's own work ends and before the tunnel starts,
  have processed output (`ENABLE_PROCESSED_OUTPUT`) and wrap at end of line
  (`ENABLE_WRAP_AT_EOL_OUTPUT`) on, and `DISABLE_NEWLINE_AUTO_RETURN` off. ssh saves that mode
  and builds the session's raw mode on it, so otherwise the whole session prints line breaks as
  glyphs and ignores `clear` and `reset`. Source: Windows PowerShell 5.1, started with stdin a
  pipe and stderr the console, turns those bits off as it starts, and the prototype turns them
  back on as it starts, before each message and as it ends (docs/how-it-was-built.md,
  "Problems hit along the way"). Whether the rewrite's runtime needs
  the repair is [Q17](#open-questions).
- **Start the tunnel for an instance other than the cached one.**

**The threat model.** The alias argument MUST come from a ProxyCommand line that the lease CLI
wrote, and the lease CLI MUST refuse any alias that fails the check in
[Command line](#command-line). Whoever can write a ProxyCommand line already runs commands, since
`cmd.exe` parses the whole line before the proxy runs, so the proxy does not defend against a
hostile line; `up`'s exact check is the gate before any ssh.

## The token push

**The push puts the laptop's localswim service URL and its current Bot token on the box, where the
box's `localswim` wrapper reads them, and sets the box's time zone to the laptop's** (D9, D12,
D32). localswim makes a new token at every cold start, so the push runs on every connection and
sends only when something has changed.

### When it runs

- **`up` pushes over its `-Alias`** (default `agentspace`), unless `AGENTSPACE_NESTED` was set
  when the operator started. It sends only when the push is due ([The stamp](#the-stamp)).
- **`sync` pushes over `agentspace`, forced**: always, whatever the stamp says.
- **`up` over a lease alias pushes with the lease's own key and certificate**, never through the
  user's `agentspace` entry (D9, card #0057).
- **Two `up`s at the same time may both push.** Each writes the same token, so nothing breaks, but
  each push is a login (D9's E0.02-T03-S01 finding). Nothing serializes them ([Q20](#open-questions)).

### What the laptop reads

1. **localswim's service receipt** (`%LOCALAPPDATA%\localswim-v2\service.json`; docs/state.md). When
   it does not exist, the push is skipped without a message: localswim is not set up on this
   laptop.
2. **From the receipt, the service's base URL** (its `base` member) **and the folder of its
   credentials** (its `credential_directory` member), **and from that folder the Bot credential**
   (`bot.xml`, protected for the current Windows user with DPAPI). Any failure to read either
   prints the warning `agentspace: Could not read the localswim Bot token; the board will not work
   on the box this session.` and skips the push. The warning MUST NOT include the failure's own
   text, which could carry part of the credential.
3. **The laptop's time zone**, as an IANA name ([The time zone](#the-time-zone)).

**The content** is exactly these two lines, UTF-8, each ending in a line feed (LF):

```
LOCALSWIM_URL=<base URL>
LOCALSWIM_TOKEN=<token>
```

### The time zone

**The push carries the laptop's time zone as an IANA name, so that `date` on the box gives the
laptop's local time and offset (D32).** It is worked out each time `up` or `sync` pushes or decides
whether to:

1. **The laptop's Windows zone ID is converted to an IANA name** with CLDR's mapping for territory
   `001`, the zone's default, for example `Pacific Standard Time` to `America/Los_Angeles`. On
   Windows that is what .NET's `TimeZoneInfo.TryConvertWindowsIdToIanaId` and ICU's
   `ucal_getTimeZoneIDForWindowsID` with no region both give. A conversion that fails gives no
   zone.
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

where `<zone>` is the checked zone or empty, and `<content>` is the two lines above with their
LFs. So it changes when the instance, the zone, the URL or the token changes.

- **A push is due when it is forced (`sync`), when there is no stamp, or when the stored stamp
  differs from the one just computed.** How the stamp is stored and compared, and where, is
  docs/state.md's (`%USERPROFILE%\.agentspace\localswim-pushed.sha256` today).
- **When the push is not due, nothing is printed and nothing is sent.**
- **The stamp is written only after the box confirms the token and, when a zone was sent, the
  zone** ([What the laptop checks](#what-the-laptop-checks)). Any other outcome leaves the stored
  stamp as it was, so the next connection tries again.
- **The stamp is not `bot.env`'s hash**, and a check MUST NOT compare it with `sha256sum` of the
  box's file. A test proves a push by setting the stamp aside and seeing it come back equal, and by
  `bot.env`'s modification time (D9's E3.03 finding).

### What the laptop sends

When the push is due, the laptop:

1. **Prints `Sending the localswim Bot token to the instance...`**, and, when the zone is empty,
   the warning `agentspace: Could not work out the laptop's time zone; times on the box stay in its
   own zone.`
2. **Starts one ssh**, Windows OpenSSH by full path (#0027), with these arguments:

   ```
   -o ClearAllForwardings=yes -o BatchMode=yes <alias> <remote command>
   ```

   - `<alias>` is `up`'s `-Alias`, or `agentspace` for `sync`.
   - `BatchMode=yes` makes an alias that needs a prompt, such as an unknown host, fail at once.
   - `<remote command>` is one argument, the box's half of the protocol
     ([Today's remote command](#todays-remote-command)). It names the zone when one is sent, and
     never the token.
   - The ssh gets a pipe for its stdin and one for its stdout; its stderr is the operator's
     stderr. Its environment has `AGENTSPACE_NESTED=1` and no `*_POSIX_FD_STATE` variable.
3. **Writes the content to the ssh's stdin, and closes it.** The content SHOULD be sent with no
   byte order mark; the prototype's runtime may put a UTF-8 BOM ahead of it, which is why the box
   strips one. A write that fails because ssh has already exited is not an error here; the
   missing confirmation reports it.
4. **Waits at most 20 seconds for the ssh to exit.** At the limit it stops that ssh and every
   process the ssh started (its ProxyCommand, the nested `up`, the tunnel), and no other process,
   prints the warning `agentspace: Sending the localswim token took over 20 seconds and was
   stopped; the board will not work on the box this session.`, and ends the push with nothing
   recorded. A push takes 5 to 8 seconds, and a client such as the desktop app gives up on a slow
   banner, so the limit stays short (D9's E3.02 finding).
5. **Reads the ssh's stdout and its exit code**, waiting at most 10 more seconds for the stdout to
   end once the ssh has exited, and taking it as empty if it does not.

### What the box does

**The box's half of the protocol, as any host MUST carry it out.** On today's box it is a POSIX
shell command run by the login user's shell; on another platform it is that platform's
equivalent.

1. **Read at most two lines from stdin, and stop reading after 30 seconds.** It MUST NOT wait for
   the end of its input beyond those two lines.
2. **Normalize them:** remove every CR, remove a UTF-8 BOM at the start of the first line, and drop
   every empty line. The two lines are taken before empty lines are dropped, so a blank line ahead
   of them leaves one line, and the content is refused. The laptop MUST send nothing ahead of the
   two lines but, at most, a BOM.
3. **Write the result to a new temporary file in the credential file's folder**, creating the
   folder if needed. The folder and the file MUST be readable by the login user alone from the
   moment they exist (today: folder mode 0700, file mode 0600, made under `umask 077`).
4. **Accept it only if it has exactly two lines, one starting `LOCALSWIM_URL=` and one starting
   `LOCALSWIM_TOKEN=`, each followed by at least one character.** Today's box checks the count and
   that each prefix is present, not their order.
5. **Rename the temporary file over the credential file**, in one atomic step. The credential file
   is `~/.config/localswim/bot.env` on today's box.
6. **Print the SHA-256 of the installed credential file's bytes** on a line of its own: 64
   lowercase hex digits, then whitespace (today `sha256sum`'s `<hash>  -`).
7. **If a zone was given, and only after step 6:** set the system time zone to it, never
   prompting, and giving up within about 6 seconds (today `timeout -k 1 5 sudo -n timedatectl
   set-timezone <zone>`, with its input and output on `/dev/null`). Then print one more line,
   `zone <zone>` if it succeeded or `zone-failed` if not, and exit 0 whatever the zone did. The
   zone MUST be the system zone, never an exported `TZ` (D32).
8. **Without a zone,** exit 0 once the hash is printed.

**On any failure before step 6**, the box MUST remove the temporary file, leave the old credential
file as it was, and exit non-zero (1 today). If step 6 itself fails, the new credential file stays
in place, and the box exits non-zero. The credential file is never left empty or partial,
whatever happens to the connection. The answer MUST NOT echo the content.

**What the box answers, then:**

| Outcome | Stdout | Exit |
|---|---|---|
| Token installed, no zone sent | `<hash>  -` | 0 |
| Token installed, zone set | `<hash>  -`, then `zone <zone>` | 0 |
| Token installed, zone refused | `<hash>  -`, then `zone-failed` | 0 |
| Token refused or not installed | Nothing that matches a hash line | Non-zero (1 today) |
| The ssh itself failed: no login, an alias that needs a prompt, no tunnel | Nothing | 255, ssh's own |

### What the laptop checks

1. **The token is confirmed** when the ssh exited 0 and the first line of its stdout that starts
   with 64 lowercase hex digits followed by whitespace carries exactly the expected hash. The
   expected hash is the SHA-256, as lowercase hex, of the content as the box writes it: CRs
   removed, a leading BOM removed, empty lines dropped, and every remaining line ending in LF.
   Otherwise it warns `agentspace: The instance did not confirm the localswim token; the board will
   not work on the box this session.` and records nothing.
2. **The zone is confirmed**, when one was sent, when a line of stdout (split at LF or CRLF) is
   exactly `zone <zone>`, compared case-sensitively. Otherwise (`zone-failed`, or no zone line) it
   warns `agentspace: The instance did not take the time zone <zone>; times on the box stay in its
   own zone.` and records nothing, so the next connection asks again. The token stays installed.
3. **Both confirmed:** it writes the stamp (docs/state.md).

**Any other failure in the push** warns `agentspace: Could not send the localswim token; the board
will not work on the box this session.` and records nothing. **A push never fails the command**:
every outcome here is a warning, and `up` and `sync` go on as if the push had succeeded.

**`-Alias` is checked again by the push itself**, in case any other path reaches it with a bad
one: it then warns `agentspace: The SSH host alias for the localswim token is not valid; the board
will not work on the box this session.` and skips the push. From the command line this cannot
happen, since the alias is checked first.

### Secrecy

- **The token MUST travel only on the push's stdin.** It MUST NOT appear in any process's
  arguments, on the laptop or the box, in any environment variable, in any message, warning or
  log, or in any file on the laptop.
- **On the box it lives only in the credential file**, and briefly in its temporary file, both
  readable by the login user alone.
- **On the laptop the operator keeps only the stamp**, a hash that covers the token. It MUST NOT
  keep the token in a file, a cache or a temporary file, and SHOULD drop it from memory once the
  push ends.
- **A warning or failure in the push MUST NOT quote the error that caused it**, because that text
  could carry part of the receipt or the credential. Each warning is a fixed text.

### Today's remote command

**The remote command the prototype sends**, shown here because it is the box's half as a protocol
message: it is ssh's last argument, which can be seen in the laptop's process list. With a zone,
`<then>` is:

```
then sha256sum < $d/bot.env || exit 1; if timeout -k 1 5 sudo -n timedatectl set-timezone <zone> </dev/null >/dev/null 2>&1; then echo zone <zone>; else echo zone-failed; fi; exit 0; 
```

and without one, `<then>` is `then sha256sum < $d/bot.env; `. The whole command, on one line, is:

```
umask 077; d=$HOME/.config/localswim; mkdir -p $d && t=$(mktemp $d/bot.env.XXXXXX) || exit 1; trap 'rm -f $t' EXIT; timeout 30 head -n 2 | tr -d '\r' | sed '1s/^\xEF\xBB\xBF//; /^$/d' > $t; if test $(wc -l < $t) -eq 2 && grep -q '^LOCALSWIM_URL=.' $t && grep -q '^LOCALSWIM_TOKEN=.' $t && mv -f $t $d/bot.env; <then>else rm -f $t; exit 1; fi
```

**It holds no double quote**, so it can be wrapped in one pair of double quotes as one argument on
a Windows command line. It assumes today's box: a POSIX shell, GNU coreutils, `sudo` and
`timedatectl`. The rewrite MAY send a different command, as long as the box's behavior and answer
are the ones above.

## The box as the laptop sees it

**This section is the contract between the laptop and today's box.** Every path and tool in it is
a property of today's box (Ubuntu, systemd, bash), and the box's scripts that carry it out stay
shell (D28). The README's "Rules" say how the box is to be changed: through `box/system` and
`sync`, never by editing the box.

### What sync installs

`sync` sends `<checkout>\box\system` whole, with the CA's public key beside it as
`agentspace_ca.pub`, and runs its `install.sh` as root from that folder. The install:

- **Fails at once if `agentspace_ca.pub` is missing or empty**, printing `install.sh:
  agentspace_ca.pub is missing; run scripts\agentspace.ps1 sync, which sends it` to stderr and
  exiting 1.
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
| `/usr/local/bin/agentspace-sample`, `/usr/local/bin/agentspace-footprint` | `sync` | Measure a turn (D33; README, "Measure a turn") |
| `/etc/ssh/agentspace_ca.pub` | `sync` | The CA sshd trusts for user certificates |
| `/etc/ssh/sshd_config.d/50-agentspace-ca.conf` | `sync` | Trusts the CA, and closes root login (D7) |
| `/etc/ssh/sshd_config.d/51-agentspace-logins.conf` | `sync` | Exempts 127.0.0.1 from login penalties, a 30-second login grace time, and a dead client dropped after about 90 seconds (D7, D12) |
| `~/.config/localswim/bot.env` (login user's, mode 0600, folder 0700) | The token push | The board's URL and Bot token, for the wrapper |
| `~/.claude/CLAUDE.md` (login user's) | `sync` | A copy of the laptop's; MUST NOT be edited on the box |
| The system time zone | The token push | The laptop's zone (D32) |
| `/tmp/agentspace-system/`, `/tmp/localswim` | `sync`, briefly | Staging; removed once installed |

**The credential file is read by the wrapper as shell assignments**: it exports each line for one
run of the CLI and never prints them. So the URL and the token MUST be safe to read that way
([Q9](#open-questions)). The wrapper refuses with a message to stderr and exit 1 when the CLI is
not installed, when the credential file is missing, or when the board's address does not answer,
which happens whenever no connection holds the board's forwards (D12).

**Not installed by the operator:** the toolchain (gh, Node through fnm, Claude Code, Codex, uv),
which `box/bootstrap.sh` installs once per instance when a person runs it over `ssh agentspace`
(README, "Toolchain"). The patch run's tools step reports, and continues, when it is missing.

### What up waits for

**`up` asks the box one question through Session Manager's Run Command, never over SSH**, because
SSH is what it is getting ready:

- **The question** is sent as the document `AWS-RunShellScript`, which today's box runs as root,
  with the comment `agentspace up: patch check`. The box MUST answer with exactly one word on
  stdout:
  - **`patched`** when the marker `/run/agentspace/patched` exists.
  - **`pending`** when it does not and the patch unit `agentspace-patch.service` is enabled, so a
    run is due or under way this boot.
  - **`unknown`** when neither, such as before the first `sync`.

  Today's command is `test -f /run/agentspace/patched && echo patched || (systemctl is-enabled
  --quiet agentspace-patch.service && echo pending || echo unknown)`. It holds no comma, bracket or
  equals sign, which the AWS CLI's shorthand syntax would split on.
- **How the answer is read.** `up` polls the command's invocation once a second, for up to 20
  seconds. While its status is `Pending`, `InProgress` or `Delayed`, or the invocation cannot be
  read yet, it polls again. Any other status ends the poll, and the answer is the invocation's
  stdout, trimmed. **20 polls without an end count as `pending`.**
- **What counts as ready:** every answer but `pending`. So `unknown` is ready, and so is an empty
  or unexpected answer from a command that failed. That last case is the prototype's behavior, and
  whether it is intended is [Q6](#open-questions).
- **What the box does at boot, as far as `up` can see:** the patch unit runs once, after the
  network is up, with a 20-minute limit. It removes the marker, upgrades every package
  (`dist-upgrade`, phased updates included) and snaps, reboots at most once per 15 minutes if a
  package asks for it, moves the toolchain forward, and then writes the marker. A failed run
  writes no marker, so `up` waits until its 15-minute deadline. The unit's own limit, 20 minutes,
  is longer than `up`'s wait, so a slow run that is still working also ends in `up`'s failure;
  the next connection after 20 minutes from the start skips the wait
  ([Starting the box](#starting-the-box-and-waiting-for-it)).

### Idle stop, as the laptop sees it

**The box powers itself off after 60 minutes with no SSH session, no Session Manager session, no
patch run and less than 6 CPU-seconds of work per 5-minute check (D11).** The instance's shutdown
behavior is stop, so the next connection starts it again. The stack's alarm stops it too after 3
hours averaging under 1% CPU, whatever sessions are open (D11's E3.03 finding).

## The AWS side

**Nothing on this page names an account ID, a VPC, a subnet or an instance ID.** Account-specific
values live in `infra\local.parameters`, which is not committed (docs/state.md).

### The stack

**One CloudFormation stack, named `agentbox`, in `us-west-2`.** It keeps the project's first name,
because a stack cannot be renamed without replacing the instance and its disk (README). The
operator MUST use that name.

**Parameters:**

| Parameter | Type | Default | Set by | Notes |
|---|---|---|---|---|
| `InstanceType` | String | `m9g.xlarge` | The template | Changing it on an update stops and restarts the instance; the disk is kept. |
| `ImageId` | EC2 image ID | A pinned Ubuntu 26.04 LTS arm64 image | The template | MUST NOT be changed on a live stack: it replaces the instance and destroys its disk. |
| `SubnetId` | Subnet ID | None | `infra\local.parameters` | A subnet that assigns a public IPv4 address at launch, as a default VPC's do. |
| `VpcId` | VPC ID | None | `infra\local.parameters` | The subnet's VPC. |
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
IAM role (`CAPABILITY_IAM`), to describe, start and stop the instance, to describe and modify its
volume, and to use Session Manager (describe instance information, send a command, read its
invocation, start a session). Today the identity is an administrator signed in with `aws login`,
which gives short-lived credentials (README, "Rules"; docs/how-it-was-built.md).

### Calls by command

**Every call passes `--region us-west-2`; the ones marked JSON also pass `--output json`, and their
output is read.** `<id>` is the cached instance ID.

| Command | AWS CLI calls, in order |
|---|---|
| Any, when the instance-ID cache is missing | `cloudformation describe-stacks --stack-name agentbox` (JSON) |
| `deploy` | `cloudformation deploy --stack-name agentbox --template-file <checkout>\infra\agentspace.yaml --capabilities CAPABILITY_IAM --no-fail-on-empty-changeset --tags project=agentbox --parameter-overrides SshPublicKey=<key> <each Name=value line>` (output passed through); then `cloudformation describe-stacks --stack-name agentbox` (JSON); then `sync`'s |
| `up`, `sync`: [start and wait](#starting-the-box-and-waiting-for-it) | `ec2 describe-instances --instance-ids <id>` (JSON); if stopped, `ec2 start-instances --instance-ids <id>` (JSON); if stopping, `ec2 wait instance-stopped --instance-ids <id>`, then `ec2 start-instances`; `ec2 describe-instances` again, for the launch time; while waiting, every 5 seconds, `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON), and once that is `Online`, `ssm send-command --instance-ids <id> --document-name AWS-RunShellScript --comment "agentspace up: patch check" --parameters commands=<question>` (JSON) and up to 20 of `ssm get-command-invocation --command-id <command id> --instance-id <id>` (JSON) |
| `sync` | The above, then nothing more from AWS: the rest goes over SSH, and each ssh or scp makes its own nested `up`'s calls and a tunnel |
| `down` | `ec2 stop-instances --instance-ids <id>` (JSON) |
| `status` | `ec2 describe-instances --instance-ids <id>` (JSON); `ec2 describe-volumes --volume-ids <vol>` (JSON); if running, `ssm describe-instance-information --filters Key=InstanceIds,Values=<id>` (JSON) |
| `grow` | `ec2 describe-instances --instance-ids <id>` (JSON); `ec2 describe-volumes --volume-ids <vol>` (JSON); `ec2 modify-volume --volume-id <vol> --size <new>` (JSON) |
| `ca` | None |
| The ProxyCommand | `up`'s, then `ssm start-session --target <id> --document-name AWS-StartSSHSession --parameters portNumber=22`, the tunnel |

**Which failures are tolerated:** a failed `ssm get-command-invocation` is polled again, since the
invocation is often not readable for a moment after `send-command`; the prototype does not check
`ec2 wait instance-stopped`. Every other failed call fails the command, under
[AWS failures](#aws-failures).

## Requirements the prototype does not meet yet

**Behavior this page requires that the prototype does not show, so that a conformance check run
against the prototype is expected to fail here.** None of it is to be copied.

| Requirement | The prototype | Card or question |
|---|---|---|
| Every ssh and scp by full path | Only the push's ssh and the CA key's scp are | #0027 |
| An expired sign-in prints agentspace's message, and other AWS failures the CLI's text | A raw runtime error, under Windows PowerShell 5.1 | #0034 |
| `deploy` exits 1 when its `sync` step fails | Most likely exits 0 | [Q3](#open-questions) |
| A missing `id_ed25519.pub` fails with an `agentspace: ` message | A runtime error | [Exit codes](#exit-codes) |
| A missing command is a usage error, never a prompt | The shell prompts for it | [Q1](#open-questions) |
| Help at every level | None of its own | [Q2](#open-questions) |

## Open questions

**What reading the code and the docs could not settle.** The clean-room review (E0.04-T03-S01)
closes them. Several need a run on the laptop, which the writer of this page could not make
(D34).

| | Question | Settled by |
|---|---|---|
| **Q1** | **What does the prototype do with a missing or unknown command, or a parameter it does not know?** Its shell's parameter binding answers, not the script: a missing command most likely prompts, and an unknown one prints a binding error. Its exit code (1 is expected) and its stderr are unobserved. This page makes each a usage error with exit 1; whether usage errors SHOULD instead use exit 2, as the lease CLI does, is a design call. | A run on the laptop; the architect |
| **Q2** | **Does `powershell -File agentspace.ps1 -?` print the script's comment-based help?** If it does, that is the prototype's only help, and the rewrite's texts above replace it. | A run on the laptop |
| **Q3** | **What exit code does `deploy` give when its `sync` step fails?** The prototype runs `sync` as a nested script in the same process. Two readings disagree: that the nested script's `exit 1` ends only the nested script, so `deploy` ends normally and a `-File` run exits 0; or that it ends the whole process with 1 (the commands inventory's reading). This page requires 1 either way. | A run on the laptop |
| **Q4** | **Does `deploy -Quiet` quiet its `sync` step?** The prototype does not pass `-Quiet` on, so `sync`'s messages most likely print. Intended or not is unrecorded. | The architect |
| **Q5** | **Does #0034's mechanism also hit `ssm get-command-invocation` and `ec2 wait`?** The poll discards the CLI's stderr while every error is terminating; under Windows PowerShell 5.1 a stderr line from it (such as an invocation not yet registered) may throw and fail `up` with a raw error instead of polling again. | A run on the laptop |
| **Q6** | **Is an empty or unexpected answer to the patch check meant to count as ready?** The prototype ends the wait on anything but `pending`, so a check that fails lets `up` print `Ready` before the patch run has finished, against the README's rule. | The architect |
| **Q7** | **Where do the instance-ID cache and the localswim CLI's build output live?** The prototype keeps them in the checkout (`.instance-id`, `.build\`), while D21 says everything agentspace keeps on the laptop lives under `%USERPROFILE%\.agentspace`. The ProxyCommand and the lease CLI's default ProxyCommand both depend on the cache's place today. | docs/state.md (E0.04-T02-S01) |
| **Q8** | **Does docs/state.md own the files the operator reads but agentspace never writes?** Those are localswim's receipt and `bot.xml`, the user's `id_ed25519.pub`, `CLAUDE.md`, `~/.ssh/config`'s `agentspace` entry, and `infra\local.parameters`. D34 gives it "every file and folder agentspace writes". The receipt's and `bot.xml`'s formats are localswim's, and nothing in this repo specifies them. | docs/state.md (E0.04-T02-S01) |
| **Q9** | **What characters can a localswim URL and Bot token contain, and what form MUST the URL have?** The box's wrapper reads the credential file as shell assignments, so a space, quote or `$` in either would break or change it, and it finds the board's address by taking the URL as `http://<host>:<port>...`. The push checks only that each is non-empty. | localswim's docs; the architect |
| **Q10** | **What are the rewrite's entry points called, and does it keep PowerShell-style parameters (`-GiB`, `-Alias`, `-Quiet`)?** Lease alias files on the laptop name `<checkout>\scripts\ssh-proxy.cmd` in their ProxyCommand line, and the lease CLI runs `scripts\agentspace.ps1 ca`, so a renamed entry point changes those too. | v0.06 (#0055) |
| **Q11** | **How does a conformance check stub ssh and aws?** D34 allows stubs for them, but #0027 requires ssh and scp by full path, and the AWS CLI is called by full path when it is installed. The rewrite needs a documented override, such as an environment variable, that a check can set and that nothing else uses. | E0.04-T02-S04 |
| **Q12** | **How does a conformance check give the push a time zone?** The prototype's override is reachable only by loading its code; from outside, only the laptop's own zone works. | E0.04-T02-S04 |
| **Q13** | **What does `deploy` do when `id_ed25519.pub` has changed?** The key is in the instance's user data, which runs at first boot only, so the new key is never installed; and CloudFormation may stop and start the instance to change user data. | A deploy on the laptop, or AWS's documentation |
| **Q14** | **Does the box require IMDSv2?** docs/how-it-was-built.md says the template requires it and that an IMDSv1 request was refused, but the template sets no metadata options, so it most likely comes from the image's defaults. | A look at the instance's metadata options |
| **Q15** | **SHOULD the region and the stack name be configurable?** Both are fixed in the prototype, in two places each (the operator and the ProxyCommand). | The architect; v0.06 |
| **Q16** | **SHOULD `status` read the volume's type**, rather than print `gp3` as a constant? | The architect |
| **Q17** | **Does the rewrite's ProxyCommand still need the console-mode repair?** The damage came from Windows PowerShell 5.1 starting under ssh. A ProxyCommand that never starts it might not need the repair, but this page requires the outcome either way. | A run on the laptop |
| **Q18** | **What does the prototype's runtime write ahead of the push's content?** The box strips a UTF-8 BOM, and the laptop's expected hash ignores one, but whether one is sent today depends on the console's input encoding. | A run on the laptop, with the box's input logged |
| **Q19** | **Is a silent skip right when localswim's receipt is missing?** Every other reason the push does not happen warns; this one prints nothing, so a laptop without localswim never learns why the board does not work on the box. | The architect |
| **Q20** | **SHOULD concurrent pushes be serialized?** Two `up`s at once both push and both write the stamp, with no lock (D9's E0.02-T03-S01 finding). It is harmless today because both write the same token. | The architect |
| **Q21** | **SHOULD `-Quiet` suppress the push's warnings?** It does in the prototype, so a quiet `sync` hides a token that did not land. The ProxyCommand never passes `-Quiet`. | The architect |
| **Q22** | **SHOULD `-GiB` be checked before AWS is asked?** The prototype passes any integer through, and AWS refuses 0 and below. | The architect |
| **Q23** | **SHOULD `grow` fail when the online filesystem step fails?** The prototype ignores that step's exit code. | The architect |
| **Q24** | **What does the prototype print when `.instance-id` is missing and the stack has no `InstanceId` output?** It most likely writes an empty cache and fails later with an AWS error about an empty ID. | A run on the laptop |
| **Q25** | **What does a failed `rm -rf` of the box's staging folder lead to in `sync`?** The prototype does not check it. A leftover folder would make the recursive copy nest `system` inside it, and the install would then fail; that the failure message then names the install rather than the copy is unverified. | A run on the laptop |
| **Q26** | **Does the sign-in test catch every form of the AWS CLI's expired-login error?** The prototype matches `expired`, `aws login` or `Unable to locate credentials`. The exact text of the CLI's error after an `aws login` session expires was not recorded with #0034. | A run on the laptop with an expired sign-in |
| **Q27** | **SHOULD the stamp stay an unsalted hash that covers the token?** It is kept on the laptop, and anyone who can read it can test a guessed token against it. A keyed digest would hide that, but the stamp MUST still change whenever the instance, the zone, the URL or the token does. Its storage is docs/state.md's. | The architect; docs/state.md |

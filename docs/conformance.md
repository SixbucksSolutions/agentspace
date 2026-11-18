# The conformance suite: what it is, how the prototype's checks sort, and what is left

RFC 2119 keywords in this page; the capitals are load-bearing.

**This page sorts every PowerShell check in `scripts/` by D34's rule, says how a conformance
check is run against any build, and lists what no check covers yet.** Story E0.04-T02-S04 (#0079)
wrote it from a read of the suites. **The sorting was done by reading, so every count, duration
and estimate below is an estimate from reading, not a measurement.** A count of checks is
approximate, and a loop counts as each of its iterations. Only the four suites that use throwaway
folders alone were then run, to show the header lines broke nothing, and all passed on 2026-11-18:
`test-schemas.ps1`, `test-lease-cli.ps1` (260 checks), `test-push-localswim.ps1` (187) and
`test-board-holder.ps1` (221, about 7 minutes). The six that write the real
`%USERPROFILE%\.agentspace` were parsed, never run.

## Contents

- [The rule](#the-rule)
- [Naming the program under test](#naming-the-program-under-test)
- [What a conformance run needs](#what-a-conformance-run-needs)
- [What the specs require of the suite](#what-the-specs-require-of-the-suite)
- [Running the suite](#running-the-suite)
- [The sorting](#the-sorting): [the summary](#the-summary), then
  [the lease CLI](#the-lease-cli-suites), [the operator and the token push](#the-operator-and-the-token-push),
  [the keeper](#the-keeper-suite), [the schemas](#the-schemas-check) and
  [the live-box check](#the-live-box-check)
- [The header line in each script](#the-header-line-in-each-script)
- [Which spec section each check would enforce](#which-spec-section-each-check-would-enforce)
- [The gaps](#the-gaps)
- [The remaining work](#the-remaining-work)
- [Open questions](#open-questions)

## The rule

**D34 ([docs/decisions.md](decisions.md#d34-v004s-spec-package-one-reference-per-interface-and-a-conformance-suite-kept-apart-from-the-prototype))
decides what a conformance check is and what prototype code is, and this page does not restate
it.** In one line each: a check that drives the product only as a separate process, through what
the specs document, with stubs only for programs outside agentspace, is conformance, and the
rewrite MAY read and run it. A check that loads the prototype's code, by dot-sourcing a script,
calling its functions or reading its internal files, is prototype code, and the rewrite MUST NOT
read it (D20).

**Four classes are used below, and the first two are D34's:**

| Class | Means | The rewrite MAY read it |
|---|---|---|
| Conformance | Meets D34's rule as it stands, and lives in `conformance/` | Yes |
| Prototype code | Loads the prototype's code, or reads its internal files, or names the prototype's own layout | No |
| Conformance in method, not yet movable | Drives a separate process through documented interfaces, but cannot be moved as it is (see its reason) | Only once it is rewritten and moved to `conformance/` |
| Neither | Drives no program under test, so there is nothing to name in a variable | Only once it is moved to `conformance/`, if it is moved |

**A row marked "Prototype code (its checks are conformance in method once rewritten)" is prototype
code as it stands, so the rewrite MUST NOT read it; what it checks could be rewritten as a
conformance check, by running the program under test instead.**

**E0.04-T02-S04 moved no check. E0.04-T03-S01 (#0084) moved the first: `conformance/lease-cli/`
holds the harness and the checks of the lease CLI's help, usage errors, values, `create`, `get` and
`list` ([Running the suite](#running-the-suite)). E0.04-T03-S02 (#0085) moved the operator's alias
refusals and the ProxyCommand's argument refusals to `conformance/operator/`, moved the keeper and
schemas checks, and wrote the checks of requirements the prototype does not meet, in
`test-unmet-*` files ([The operator and the token push](#the-operator-and-the-token-push),
[The files that are expected to fail](#the-files-that-are-expected-to-fail)).** Until a check is
moved, it is in `scripts/`, and the rewrite MUST NOT read it, whatever its class: the rewrite's
reviewers work from an export of `docs/`, `schemas/` and `conformance/` only (D34). **The export
MUST leave out every "Findings from ..." block in `docs/decisions.md`, `docs/runs/`, the handoffs
and `docs/how-it-was-built.md`** (the architect, 2026-11-18; D34). A guess is a gap to report, never
filled from findings. A check moves only by being rewritten as the first kind, never by copying
code that loads the prototype.

## Naming the program under test

**A conformance check names the program it tests through one environment variable per program,
so the same check runs against the prototype and against the rewrite (D34).** The variables
below are **pinned** (the architect, 2026-11-18). The harness reads the first two
(E0.04-T03-S01, #0084); the checks of E0.04-T03-S02 (#0085) read the other two, the operator's and
ProxyCommand's by `Get-ConformanceProgram` too. **A variable that is unset or empty
takes its default from `conformance/prototype/defaults.ps1`, and from nowhere else**; a variable
that is set always wins. **Every default is a path in that file, relative to its folder**, and
each names a program in `conformance/prototype/`, except the first, which names the prototype's
`bin\agentspace.cmd`.

| Variable | Names | Default, as `conformance/prototype/defaults.ps1` gives it |
|---|---|---|
| `AGENTSPACE_CMD` | The lease CLI, run as `& $env:AGENTSPACE_CMD lease ...` with its arguments unchanged | `..\..\bin\agentspace.cmd`, the prototype's lease CLI |
| `AGENTSPACE_OPERATOR_CMD` | The operator (`deploy`, `up`, `sync`, `ca`, `down`, `status`, `grow`) | `operator.cmd`, a shim that starts the prototype's operator under Windows PowerShell 5.1 |
| `AGENTSPACE_KEEPER_CMD` | The keeper | `keeper.cmd`, a shim that starts the prototype's keeper under PowerShell 7 |
| `AGENTSPACE_SSH_PROXY_CMD` | The ProxyCommand program | `ssh-proxy.cmd`, a shim that calls the prototype's `scripts\ssh-proxy.cmd` |

**Why four and not one.** The story card asked for `AGENTSPACE_CMD` for the lease CLI and the
operator. The prototype has them as two programs under two runtimes (the lease CLI under
PowerShell 7, the operator under Windows PowerShell 5.1), and the keeper and the ProxyCommand are
two more, so each needs a name. A rewrite that ships one binary sets all four to it.

- **The names MUST NOT be `AGENTSPACE_PROXY_COMMAND`.** That variable already means something
  else: it is the command string that `create` writes into a lease's alias
  ([docs/lease-api.md](lease-api.md#environment)). `AGENTSPACE_SSH_PROXY_CMD` is a different
  variable, read by a check to find a program, and it MUST NOT be confused with it.
- **The conformance variables are read by checks, never by a program under test.** So
  [docs/lease-api.md](lease-api.md#environment)'s rule, that no other environment variable
  changes the contract's behavior, is unaffected.
- **The only files in `conformance/` that name the prototype are its default shims and
  `conformance/prototype/defaults.ps1`**, which are all in `conformance/prototype/`. A check and the
  harness never do.
- **The token push has no program of its own,** because it is a part of the operator's `up` and
  `sync`, so it has no variable. [The gaps](#the-gaps) say what it needs instead.

## What a conformance run needs

**RFC 2119 keywords again: these bind every conformance check.**

- **Stubs MUST stand only for programs outside agentspace** (`ssh`, `aws`), as D34 says. A stub
  of one of agentspace's own programs is prototype code in disguise. The keeper suite's `ssh`
  stub, `scripts\test-board-holder-stub.cmd`, is the model: it prints one line, runs for about 20
  minutes unless stopped, and connects nowhere.
  - **A stub MUST NOT receive the real token** ([docs/operator.md](operator.md#secrecy)).
- **A check MUST work in throwaway folders only.** Each folder is made under `%TEMP%` and removed
  when the check ends, in a `finally`.
- **A check MUST NOT read or write Terry's real `~/.ssh`, `%USERPROFILE%\.agentspace`, or keeper
  task.** It MUST NOT hash or list them as a proof either, since that reads them,
  and the proof is moot once they are out of reach. The check keeps the real ones out of reach
  instead:
  - **The state and ssh folders** are named through `AGENTSPACE_STATE_DIR` and
    `AGENTSPACE_SSH_DIR` ([docs/lease-api.md](lease-api.md#environment)).
  - **The lease's key, certificate and the CA** have no folder override today
    ([docs/lease-api.md](lease-api.md#open-questions), question 9), so a check that places a
    ready lease sets `USERPROFILE` to a throwaway folder and makes a throwaway CA there with the
    operator's `ca` command ([docs/operator.md](operator.md#ca)). `USERPROFILE` is a documented,
    read variable ([docs/operator.md](operator.md#environment-variables)). **The harness does this
    for every lease CLI check, and the throwaway CA works** ([Running the suite](#running-the-suite),
    [open question 3](#open-questions)).
  - **`ca` MUST NOT be run elevated** (D17; [docs/operator.md](operator.md#ca)).
- **A check of the keeper MUST wait on what it checks and never on a fixed sleep (#0066).** It
  polls the log line, the file or the process it expects, with a deadline. Reading found three
  fixed sleeps (5 seconds in the log-held case, 3 in the default-grace case, 2 in the
  holder-after-a-stop case). A moved suite SHOULD replace each with a wait on the condition it
  stands for, with a deadline. #0066 (the locked case flakes under load) is carried to the
  keeper's conformance story.
- **The keeper suite MUST NOT run under a timeout shorter than its run.** It takes about 7
  minutes (Terry's figure), and longer with `-SelfCheck`. The 95-second default-grace case and the
  240-second `-Watch` waits dominate.
- **A run MUST make a failing program fail.** Pointed at a program that prints nothing and exits
  0, every process check fails check by check, and none passes vacuously. That proves the variable
  is read. The one check with no program, [the schemas check](#the-schemas-check), is exempt.
- **A run MAY skip a check whose setup it cannot make (such as a throwaway CA). It MUST report
  each as SKIPPED, with the reason, and a run with any skip is incomplete, never passed.**
- **The platform is a parameter, not an assumption** (D4). A check MUST NOT assume a Linux host,
  except where it describes today's box.

## What the specs require of the suite

**Three requirements in the specs say what the conformance suite MUST do. No sorted check meets
any of them today.**

- **The lease CLI's times** ([docs/lease-api.md](lease-api.md#times), #0029 point 7). The suite
  MUST check a certificate's validity and `expires_at` against its own clock, and MUST run at
  least once with the caller's time zone set to one other than UTC. A grep of the lease suites
  finds none that sets a zone. The driver suite compares against its own clock within a window
  of 15 seconds, and the renew suite against its own start time.
  - **Met in part by E0.04-T03-S01 (#0084):** `conformance/lease-cli/test-create-get.ps1` checks a
    ready `create`'s `created_at`, its `expires_at` minus the TTL, and the certificate's
    valid-before against the check's own clock and against each other. **It cannot set a zone for
    the program**: Windows has no environment variable for it, so the run uses the laptop's zone,
    and a run on a machine set to UTC does not meet the second half ([Running the suite](#running-the-suite)).
- **The `usage` error's `field`** ([docs/lease-api.md](lease-api.md#schemas)). The error schema
  cannot say which `usage` errors carry `field`, so a conformance check MUST test that rule. The
  first half of `scripts\test-lease-cli.ps1` compares the code and `field` of each error run, so
  most of it is in place.
  - **Met by E0.04-T03-S01 (#0084):** `conformance/lease-cli/test-usage-errors.ps1` runs each of the
    five cases the spec names, in every command that has one, and about 30 usage errors that MUST
    carry no `field`.
- **Concurrent processes** ([docs/lease-api.md](lease-api.md#concurrency-and-atomicity)). The spec
  requires commands to be safe under concurrent processes
  ([docs/state.md](state.md#concurrency)), so a race check through the CLI is a conformance check,
  and the suite MUST contain one. The prototype's one race check is in-process, so it is not.

## Running the suite

**RFC 2119 keywords again.** This section is for a person or an agent who runs a conformance check
against a build, the prototype or a rewrite. It was written by E0.04-T03-S01 (#0084), and it holds
for every check in `conformance/lease-cli/`. **It holds as well for the checks in
`conformance/operator/`** (E0.04-T03-S02, #0085), which dot-source the same harness through their
own `helpers.ps1`, and are described under [The operator and the token push](#the-operator-and-the-token-push).

**To run one check:** `pwsh -NoProfile -File conformance/lease-cli/test-<name>.ps1`, from any
folder. It needs PowerShell 7 on a Windows laptop, and MUST NOT run elevated, because the
operator's `ca` refuses to create a CA then ([docs/operator.md](operator.md#ca)).

| Check | What it enforces | Needs a CA |
|---|---|---|
| `test-help.ps1` | The help block at every level and in odd positions ([lease-api.md](lease-api.md#commands), [Command lines](lease-api.md#command-lines)) | No |
| `test-usage-errors.ps1` | Usage errors, their exit code and `field`, and the order they are reported in ([Command lines](lease-api.md#command-lines), [Exit codes and errors](lease-api.md#exit-codes-and-errors)) | No |
| `test-values.ps1` | `invalid_value` and `out_of_range`, their `field`, and the error schema's refusal of a line feed ([Values](lease-api.md#values), [Schemas](lease-api.md#schemas)) | No |
| `test-create-get.ps1` | `create` and `get`: a ready lease, its credential files, TTL, uppercase ID, unsatisfiable, a tolerant command line, a failed write, `not_found`, expiry, the state file ([lease-api.md](lease-api.md#agentspace-lease-create), [state.md](state.md#a-leases-credential)) | For a ready lease |
| `test-list.ps1` | `list`: its order, filters and empty list, with leases made by `create` and given the times and states the state file can hold ([lease-api.md](lease-api.md#agentspace-lease-list)) | For a ready lease |

- **Each check prints one line per result and one summary line.** A result line is `PASS <name>`,
  `FAIL <name>: <reason>` or `SKIPPED <name>: <reason>`, and a name starts with the spec section it
  enforces, as in `lease-api.md#agentspace-lease-get: ...`. The summary is `<n> passed, <n> failed,
  <n> skipped`.
- **The exit code is 0 when every result was PASS, 1 when any was FAIL, and 2 when none failed but
  any was SKIPPED.** **A run that counted no result at all MUST exit 1**, with a `FAIL` line saying
  that no check ran. A run that exits 2 is incomplete, never passed (see
  [What a conformance run needs](#what-a-conformance-run-needs)). The checks skip only when the
  harness could not make a throwaway CA, when no `ssh-keygen` is on the `PATH` (two checks of
  `test-create-get.ps1`), or when the local UTC offset is 0 (one check of `test-create-get.ps1`),
  and the reason says which.
- **The program under test is named by `AGENTSPACE_CMD`** ([Naming the program under test](#naming-the-program-under-test)).
  It is run as a separate process with its arguments unchanged, and its stdin is closed. Set it to a
  program that prints nothing and exits 0, and **every check MUST fail**: that proves the variable is
  read and that no check passes vacuously. E0.04-T03-S01 ran that proof for each check.
- **The CA is made by `AGENTSPACE_OPERATOR_CMD`**, the operator, run as `ca -Quiet`.
- **The suite MUST run at least once on a machine whose time zone is not UTC**, as
  [lease-api.md](lease-api.md#times) requires. The harness cannot set a zone for a child process,
  and `test-create-get.ps1` reads the program's times against its own clock in UTC, so on a machine
  set to UTC a zone mix-up in the program stays hidden. **`test-create-get.ps1` reports one SKIPPED
  check, with a reason, when the local UTC offset is 0**, so a run on a UTC machine exits 2: it is
  incomplete, never passed.
- **A check takes from about 20 to about 50 seconds against the prototype, and all five about 3
  minutes**, most of it starting programs: about 0.5 seconds for a run that issues no credential,
  and a few seconds for a ready `create`. Against a program that prints nothing, all five take
  about 15 seconds.

### The harness

**`conformance/lease-cli/harness.ps1` is dot-sourced by every lease CLI check, and it is the only
code that starts the program under test.** A check makes each home it needs with
`New-ConformanceHome`, removes it in a `finally`, and ends with `Exit-ConformanceRun`. The helpers
the checks share, which start no program of their own, are in `conformance/lease-cli/lease-cli-common.ps1`.

| Function | Does |
|---|---|
| `New-ConformanceHome` | Makes a folder under `%TEMP%` named `agentspace-conformance-<12 hex>`, holding a fresh `USERPROFILE` (`home`, with `.agentspace` and `.ssh`) and a throwaway CA made by the operator's `ca` under it. Returns `Root`, `UserProfile`, `StateDir` (`.agentspace\state` in the home), `SshDir` (`.ssh` in the home), `CaReady` and `CaReason`. When no CA can be made, `CaReady` is false and `CaReason` says why, and the real CA is never used instead |
| `Invoke-AgentspaceCli -ConformanceHome <h> -Arguments <string[]> [-InputJson <string>] [-Environment <hashtable>]` | Runs the program `AGENTSPACE_CMD` names as a separate process, inside the home. Returns `ExitCode`, `Stdout`, `Stderr`, `Json` (stdout parsed with every time kept as a string, or null), `ErrorJson` (stderr parsed, or null), `StdoutBytes`, `StderrBytes` and `TimedOut`. `-Environment` adds or removes variables for that one run, and a null value removes one. **It refuses an override of `USERPROFILE`, `HOME`, `LOCALAPPDATA` or `APPDATA`, and an override of `AGENTSPACE_STATE_DIR` or `AGENTSPACE_SSH_DIR` to a path that is not under the home's `Root`.** A refusal throws, and the check reports it as FAIL. `-InputJson` is written to stdin |
| `Invoke-ConformanceProgram -Variable <name> ...` | The same for any program a conformance variable names, such as `AGENTSPACE_OPERATOR_CMD` |
| `Test-ConformanceSchema -Json <string> -Schema <file>` | True when the document validates against a schema under `schemas/` (`state/leases.state.schema.json` is allowed), by PowerShell 7's `Test-Json -SchemaFile`. False when it does not validate or is not JSON |
| `Write-ConformanceResult -Name <check> -Status PASS\|FAIL\|SKIPPED [-Reason <text>]` | Prints one result line and counts it |
| `Remove-ConformanceHome -ConformanceHome <h>` | Deletes the home's `Root`, and only it. It refuses a folder that is not an `agentspace-conformance-*` folder directly under `%TEMP%` |
| `Exit-ConformanceRun` | Prints the summary line and exits with the code above. A run that counted no result exits 1, after a `FAIL` line saying that no check ran |

- **Every program the harness starts gets the same environment.**
  - **Pointed into the home:** `USERPROFILE`, `HOME`, `AGENTSPACE_STATE_DIR` and
    `AGENTSPACE_SSH_DIR`, and also `LOCALAPPDATA` (`home\AppData\Local`) and `APPDATA`
    (`home\AppData\Roaming`). **The last two matter**: the operator reads the localswim receipt, and
    so the token, under `LOCALAPPDATA` ([docs/state.md](state.md)), so a child that kept the real value
    could reach it.
  - **Kept away from AWS and the real token, so that no check can ever call the real account:**
    - `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE` name files that do not exist in the home
      (`home\no-aws\config` and `home\no-aws\credentials`).
    - `AWS_EC2_METADATA_DISABLED` is `true`, so no instance-metadata credentials are looked for.
    - `AWS_PROFILE`, `AWS_DEFAULT_PROFILE`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`,
      `AWS_SESSION_TOKEN`, `AWS_CONTAINER_CREDENTIALS_FULL_URI` and
      `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` are removed.
    - **A check that runs a program that can call `aws`** (the operator or the ProxyCommand beyond
      a usage error) **MUST first confirm, under the same environment, that
      `aws sts get-caller-identity` fails**, and report every case SKIPPED without running the
      program if it succeeds.
  - **`AGENTSPACE_PROXY_COMMAND` is set to a command that fails at once** (the command
    interpreter's `/d /c exit 1`, found through `ComSpec`), so that a ready lease
    needs no real box ([lease-api.md](lease-api.md#environment)). A check MAY override it with
    `-Environment`.
  - **Every other `AGENTSPACE_*` variable the caller holds is removed**, so that a shell's own
    settings never reach the program.
  - **`PSModulePath` is removed.** A PowerShell 7 session's value carries PowerShell 7's own module
    folders, and Windows PowerShell 5.1, which runs the prototype's operator, then cannot load its
    security module, so its `ca` fails.
- **A check MUST NOT read, write, list or hash the real `%USERPROFILE%\.agentspace`, the real
  `~/.ssh` or the real CA**, and the harness never gives one a path to them. Only a person or a
  verifier proving a run touched nothing lists them, before and after.
- **A check MUST NOT load prototype code** (D34). It reaches the program only through the harness.

### What the first run proved

**It answers [open question 3](#open-questions).** On 2026-11-18, on the laptop, with `USERPROFILE`
set to a new folder under `%TEMP%` (the OpenSSH client still finds the real home through the
account, so the real profile is not unreachable), the operator's
`ca` made a CA there and exited 0, a second `ca` exited 0 and changed nothing, and a ready
`lease create` then issued a lease, wrote its alias file and its credential under that home, and
printed a document that validates.

### The files that are expected to fail

**RFC 2119 keywords again.** Some checks assert a requirement that the prototype does not meet
yet. Each is in its own file, named `test-unmet-<what>.ps1`, so that **every other file passes
against the prototype in full**, and a runner can tell an expected failure from a regression
(the architect, 2026-11-18).

- **A `test-unmet-*` file MUST say, in its header, which requirement and card it checks, and that it
  is expected to FAIL against the prototype.** Its result lines still say `FAIL`: nothing turns a
  FAIL into a pass, and a rewrite MUST pass every line.
- **A runner MUST NOT read such a file's exit 1 against the prototype as a regression**, and MUST
  read the same exit against a rewrite as a failure.
- **The rows of a spec's "Requirements the prototype does not meet yet" table, such as
  [operator.md's](operator.md#requirements-the-prototype-does-not-meet-yet), are what these files
  assert**, so a row leaves its table when a build passes its file.

| File | Requirement | Card | Against the prototype |
|---|---|---|---|
| `conformance/operator/test-unmet-proxy-connect-time.ps1` | The ProxyCommand names no backend on success, prints agentspace's own message on failure, and prints backend detail only under `AGENTSPACE_DIAGNOSTIC` ([docs/operator.md](operator.md#what-a-caller-sees-d1-card-0087)), asserted through the seam that names the AWS CLI, `AGENTSPACE_AWS_CMD` ([The test seam](operator.md#the-test-seam)) | #0087 (D1), and #0085 for the seam | FAIL on both paths, `seam not met (AGENTSPACE_AWS_CMD was not used)`: the prototype runs the AWS CLI by a fixed path, so the stand-in is never called. Nothing more is asserted until the seam is met |
| `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1` | A keeper that tracks a holder, and whose pass does not find it trusted (its process gone while the pass waits for `board-holder.lock`, or its record gone), starts no holder in that pass and waits the release grace after it, logged `grace seconds=<n>` ([docs/keeper.md](keeper.md#release-grace)); the check does not depend on the action word the pass logs ([Q6](keeper.md#open-questions)) | #0068 | FAIL on 3 checks of each of its 2 cases: the pass logs `started` at once, the line after it is not a grace line, and the next start is not the grace after the pass was let go. Nothing else fails ([The #0068 file](#the-0068-file-test-unmet-holder-dies-mid-passps1)) |

- **The D1 file's stand-ins are its own.** It writes two `.cmd` files into its throwaway home, one
  `aws` that reports a made-up running instance and one that fails with error text, both logging
  every call. A build that meets the seam calls them, and the file then asserts what the caller's
  stderr holds (no backend text on success; one `agentspace: ` message on failure; the diagnostic
  setting changes neither the exit code nor stdout). It sets `AGENTSPACE_NESTED`, so that `up`
  pushes no token and needs no `ssh` stand-in.
- **The AWS gate still applies**, so even a build that ignores the seam cannot reach the real
  account: with the call log empty, the file reports the failure and nothing else.

## The sorting

### The summary

**Eleven suites and checks were sorted, and the keeper's stub with its suite.** Only the keeper
suite and the process half of the lease CLI suite are conformance in method and nearly movable
today. Both already drive a separate process through documented flags, and the keeper suite
already keeps each case's state in a throwaway folder.

| File | Class | The reason, in one line | Real folders it touches today |
|---|---|---|---|
| `test-lease-cli.ps1` | Mixed: the process half is conformance in method, the in-process half is prototype code | The second half dot-sources the state script. **Its process half is moved** to `conformance/lease-cli/` (#0084): `test-help.ps1`, `test-usage-errors.ps1` and `test-values.ps1` | None |
| `test-lease-cmd-create-get.ps1` | Conformance in method, **moved** to `conformance/lease-cli/test-create-get.ps1` (#0084) | The credential step had no root override. The moved check runs in a throwaway `USERPROFILE` with a throwaway CA, and the script in `scripts/` stays until the rewrite replaces it | The script in `scripts/` still: the real CA (signs with it), the real `leases` folder (written, removed), `~/.ssh` (hashed). The moved check: none |
| `test-lease-cmd-renew-release-list.ps1` | Prototype code as it stands; its checks are conformance in method | It dot-sources the driver to make each ready lease | The same as the line above |
| `test-lease-credential.ps1` | Prototype code | It dot-sources the credential script and calls its function | The real CA (signs with it), the real `leases` folder (written, removed) |
| `test-lease-renew-release.ps1` | Prototype code | It dot-sources the credential and alias scripts | The same as the line above |
| `test-lease-alias.ps1` | Prototype code | It dot-sources the alias script and calls five functions | A throwaway subfolder inside the real `%USERPROFILE%\.agentspace` |
| `test-lease-driver.ps1` | Prototype code | It dot-sources the driver, reads its source text and a variable | The real CA (signs with it), the real `leases` folder (written, removed), `~/.ssh` (hashed) |
| `test-schemas.ps1` | Neither; moved to `conformance/schemas/test-schemas.ps1` (E0.04-T03-S02) | No program is driven and no prototype code is loaded | None |
| `test-push-localswim.ps1` | Prototype code, except its alias and ProxyCommand refusals, **moved** to `conformance/operator/` (E0.04-T03-S02) | It extracts five functions from the operator's syntax tree and sets its variables; its process checks run a copied operator and an edited `ssh-proxy.cmd` beside a stand-in operator. The script in `scripts/` is unchanged and keeps passing, and the push's checks stay in it until a build meets the seam ([The operator and the token push](#the-operator-and-the-token-push)) | None (everything is under `%TEMP%`) |
| `test-board-holder.ps1`, with `test-board-holder-stub.cmd` | Its 41 process cases are rewritten and moved to `conformance/keeper/test-keeper.ps1`, with a stub of its own and shared helpers in `keeper-common.ps1` (E0.04-T03-S02); its three installer cases stay here as prototype code. #0068, which it never checked, has its own file, `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1`, expected to fail against the prototype | It named its keeper by a path parameter, and one installer case reads the real scheduled task | The real keeper task (read only, one installer case, which stays in `scripts/`) |
| `check-forward-release.ps1` | Neither | A live check of the box, not a suite | The real box, through a ready lease's alias |

**Five suites write lease folders under the real `%USERPROFILE%\.agentspace\leases` and sign each
lease's key with the real CA:** `test-lease-cmd-create-get.ps1`,
`test-lease-cmd-renew-release-list.ps1`, `test-lease-credential.ps1`,
`test-lease-renew-release.ps1` and `test-lease-driver.ps1`. Each removes its folders in a
`finally`, but a run that is killed leaves them. `test-lease-alias.ps1` writes a throwaway
subfolder directly under the real `%USERPROFILE%\.agentspace`. A conformance version MUST NOT do
either.

### The lease CLI suites

**What they load, and which interface they drive.** Every suite below reads `docs\lease-api.md`
or `schemas\` at most, which are spec artifacts and allowed.

#### `scripts/test-lease-cli.ps1` (about 450 lines): mixed

- **Moved by E0.04-T03-S01 (#0084): the process half.** The first three groups in the table below
  are `conformance/lease-cli/test-help.ps1` (30 checks), `test-usage-errors.ps1` (71) and
  `test-values.ps1` (59, with the error schema's refusal of a line feed, which now also needs the
  program's own error to validate, so that it fails against a program that prints nothing). They
  keep one entry point, the program under test, and drop the `pwsh -File` one. The script in
  `scripts/` is unchanged and keeps passing ([The remaining work](#the-remaining-work)).
  - **One check was not moved, because the prototype does not meet it:** a project value with a line
    feed in it, passed through `bin\agentspace.cmd`, reaches the CLI cut at the line feed, since `cmd`
    ends an argument there. The check that exists in the old script runs the CLI directly instead
    ([lease-api.md](lease-api.md#the-program), #0083; [The gaps](#the-gaps)).
- **Drives:** In its first half, the lease CLI as a separate process, through `bin\agentspace.cmd`
  and through `pwsh -NoProfile -File scripts\lease-cli.ps1`, capturing stdout, stderr and the
  exit code as bytes.
- **Loads:** In its second half, `scripts\lease-state.ps1`, by dot-source, and child `pwsh`
  processes that dot-source it for a race.
- **Stubs:** None for programs. It sets the three documented variables, `AGENTSPACE_STATE_DIR`,
  `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND`, to throwaway values.
- **Folders:** Throwaway only. No check places a lease.

| Group of checks | Count | Class | What moving it takes |
|---|---|---|---|
| The doc has 7 help blocks; `--help` and `-h` at 7 levels through both entry points; `--help` in odd positions | About 40 | Conformance in method | Already a process run. Keep one entry point, the program under test, and drop the second |
| Usage errors, including usage-before-value ordering | About 25 | Conformance in method | Already a process run |
| `invalid_value` and `out_of_range` through the CLI | About 17 | Conformance in method | Already a process run |
| The error schema refuses a message with a line feed | 1 | Conformance, no program | Moves as it is |
| The normalizers: the accept and refuse tables for lease ID, project, platform, profile, size, TTL and state filter | About 190 | Prototype code (calls its functions) | The tables are spec data and carry over as probes through the CLI: a refusal on `lease get`, `list`, `create` and `renew`, and an acceptance echoed by an unsatisfiable `create` (a size no profile has, so no credential is issued and no CA is needed) or by `not_found` on an unknown ID |
| The time helpers (3) | 3 | Prototype code | Observable as `created_at` in a `create` output: UTC and whole seconds |
| The state file: location, compact JSON with one line feed, no BOM, schema-valid, round-trips, times stay strings, `driver` never printed, the six record states map to `pending`, `ready`, `expired`, `released`, `released`, `unsatisfiable` | About 25 | Prototype code | Seed `leases.json` by hand, then `get` and `list` each record, and read the file after an unsatisfiable `create`. Needs [docs/state.md](state.md#the-lease-state-file) to document the file |
| A failed update changes nothing; no temporary file is left; an invalid file reads as `internal`; a lease error inside an update reaches the caller | 6 | Prototype code | Partly: "invalid file is `internal`" and "a failed write changes nothing" through `get` on a seeded bad file and a `create` with `leases.json` made a folder. The in-process error plumbing cannot |
| A ready lease is `expired` at its `expires_at` second and `ready` one second before | 1 | Prototype code | Not movable: it needs a clock the check controls |
| Three writers of 5 records and one reader, in parallel | 2 | Prototype code (child processes dot-source the state script) | Three parallel loops of unsatisfiable `create` and a loop of `list`. A conformance check: [lease-api.md](lease-api.md#concurrency-and-atomicity) requires commands to be safe under concurrent processes ([state.md](state.md#concurrency)) |

#### `scripts/test-lease-cmd-create-get.ps1` (about 200 lines): conformance in method, moved

- **Moved by E0.04-T03-S01 (#0084)** to `conformance/lease-cli/test-create-get.ps1` (39 checks),
  with `list` made from `create`-made seeds in `test-list.ps1` (32). They run in a throwaway
  `USERPROFILE` with a throwaway CA ([Running the suite](#running-the-suite)), read the program from
  `AGENTSPACE_CMD`, and hash nothing of the real `~/.ssh`. The script in `scripts/` below is
  unchanged. The moved check runs the failed write with a separate state folder and ssh folder for
  that one run, both inside the home.
- **Drives:** `lease create` and `lease get`, each as a separate process through
  `pwsh -NoProfile -File scripts\lease-cli.ps1`. It dot-sources nothing.
- **Reads:** Files the specs name: the state file, the alias file, `id_ed25519`,
  `id_ed25519-cert.pub`, and `ssh-keygen -L` on the certificate.
- **Why it could not move before:** The credential step has no root override. It signs with the real CA and
  writes each lease's folder under the real `%USERPROFILE%\.agentspace\leases`, removed in a
  `finally`. It also hashes Terry's real `~/.ssh` before and after. It names `ssh-keygen` by a
  full path, which is allowed, because that program is outside agentspace.
- **About 30 checks:** A ready lease, TTL, an uppercase ID, unsatisfiable, a tolerant command
  line, a failed write after the credential is placed, `not_found`, expiry reading as `expired`,
  and the state file.
- **What moving it took:** A throwaway `USERPROFILE` with a throwaway CA, the program read from
  `AGENTSPACE_CMD`, and no hashing of the real `~/.ssh`.

#### `scripts/test-lease-cmd-renew-release-list.ps1` (about 330 lines): prototype code as it stands

- **Drives:** `lease renew`, `release` and `list` as separate processes.
- **Loads:** `scripts\lease-driver.ps1`, by dot-source, and calls `Invoke-LeaseDriverPlace` to
  make each ready lease. It then writes the other records into `leases.json` by hand.
- **Folders:** A throwaway state folder and ssh folder, plus the real CA (signs with it) and the real
  `leases` folder (written, removed). It hashes the real `~/.ssh`.
- **About 65 checks, all conformance in method but for that one setup helper:** `list` (filters,
  order by `created_at` then `id`, `{"leases":[]}`), `renew` (certificate replaced, key
  byte-identical, `expires_at`, `ttl_seconds`, refusals by state) and `release` (folder and alias
  removed, idempotent, a result per state).
- **What moving it takes:** Seed the ready leases with `lease create` against the throwaway
  `USERPROFILE`, then edit `created_at` and `expires_at` in the state file, as it does for the
  other records already.

#### `scripts/test-lease-credential.ps1`, `test-lease-renew-release.ps1`: prototype code

- **Load:** `scripts\lease-credential.ps1`, and through it `lease-alias.ps1`, by dot-source. They
  call `New-LeaseCredential`, `Update-LeaseCredential` and `Remove-LeaseCredential` directly.
- **Folders:** The real CA (signs with it), and a lease folder under the real `leases` folder (removed
  at the end).

| Group of checks | Class | What moving it takes |
|---|---|---|
| The certificate: type, Key ID equals the lease ID, principal `ubuntu`, validity window equals the TTL, no critical options, extensions exactly `permit-pty` and `permit-port-forwarding`, signing CA is the agentspace CA, the key has no passphrase | Prototype code | Read the certificate that a ready `create` leaves in the lease's folder |
| Renew keeps the key's bytes and fingerprint, writes a new window; a second renew has its own window; release removes the alias file and the lease folder and leaves `config` byte-identical; a second release changes nothing | Prototype code | Through `create`, `renew` and `release` |
| A bad lease ID is refused by the credential step; a second issue for the same ID is refused | Prototype code | Not movable: the CLI never accepts a caller-chosen ID |
| No `renew-*` temporary file is left | Prototype code | Only as "the lease folder holds only what [docs/state.md](state.md#a-leases-credential) lists", if it lists them |
| Renew of a lease with no key throws and creates no folder | Prototype code | Not movable: it needs a `ready` record with no folder, and the CLI contract does not say what happens then (a gap in the spec, not a check to carry) |

#### `scripts/test-lease-alias.ps1` (about 190 lines): prototype code

- **Loads:** `scripts\lease-alias.ps1`, by dot-source, and calls `Get-LeaseAliasName`,
  `Test-LeaseAliasName`, `Install-LeaseAliasInclude`, `Set-LeaseAlias` and `Remove-LeaseAlias`. It
  sets a variable of the prototype to a fake repo.
- **Stubs:** None. `ssh.exe -G -F <alias file> <alias>` is the oracle, a program outside agentspace.
- **Folders:** A throwaway folder directly under the real `%USERPROFILE%\.agentspace`, removed in a
  `finally`. It never reads or writes the real `~/.ssh`.

| Group of checks | Count | What moving it takes |
|---|---|---|
| The alias name is `agentspace-<project>-<first 8 hex>`, lowercase; a non-UUID and a UUIDv1 refused; `..\x` refused; U+0130 and U+212A refused | 6 | Partly: the refusals are the normalizer probes above, and the name is visible in a ready `create` |
| The alias-name test accepts 3 values and refuses 8 | 11 | The refusals through the operator's `up -Alias <bad>` (see [the operator](#the-operator-and-the-token-push)). The three acceptances cannot be shown without reaching AWS |
| The `Include` line on LF, CRLF and BOM configs; a missing config; a second run changes nothing; the backup equals the original; a misplaced `Include` is refused | 9 | Prepare `AGENTSPACE_SSH_DIR` with each config shape, run a ready `create`, compare the config bytes |
| The alias file's bytes (no BOM, LF only, a comment line, no `ClearAllForwardings`, the folder holds only the file) and what `ssh -G` shows (user, identity, certificate, `identitiesonly`, `hostkeyalias agentspace`, strict host key checking, the proxy command, both remote forwards) | About 17 | From the alias file a ready `create` writes. "Set again changes nothing" has no CLI equivalent and is dropped; the comment line's naming of the prototype's script is replaced by "starts with a comment naming the lease ID" |
| The default ProxyCommand ends with the literal alias and has no `%n`; its path is `<repo>\scripts\ssh-proxy.cmd` | 3 | The first two, by leaving `AGENTSPACE_PROXY_COMMAND` unset. The path is the prototype's layout and is dropped |
| A short-ID collision is refused; the existing alias file survives; removing the colliding lease is refused | 4 | Not movable: it needs two leases with the same first 8 hex digits, which a caller cannot force |
| Set with a missing certificate is refused; remove leaves the config and the lease folder alone | 2 | Not movable: an internal call, and unlike `release`, which removes the folder |
| Remove deletes the alias file; remove again reports false | 2 | Covered by `release` twice |

#### `scripts/test-lease-driver.ps1` (about 160 lines): prototype code

- **Loads:** `scripts\lease-driver.ps1`, by dot-source. It reads the driver's source text and a
  variable that holds the registered drivers.
- **Folders:** The real CA (signs with it), a random-UUID lease folder under the real `leases` folder
  (removed), a throwaway ssh folder, and a hash of the real `~/.ssh`.

| Group of checks | Count | What moving it takes |
|---|---|---|
| Registration: the source names no backend; a driver registered itself | 2 | Not movable: it reads the prototype's source and a prototype variable. The driver boundary ([docs/driver.md](driver.md)) is an in-process interface that a rewrite need not have |
| A ready lease is placed: state, a driver name returned, the alias, user `ubuntu`, `ExpiresAt` is UTC and whole seconds and equals the certificate's valid-before and now plus the TTL, the alias file and `Include` present, the document validates | 9 | From the output of a ready `create` (`expires_at`, `ssh.host_alias`, `ssh.user`) and the files. "A driver name returned" becomes "the output has no driver name" |
| Renew changes `ExpiresAt`; release removes the alias and folder; a second release succeeds | 8 | Covered by the renew and release suite above |
| Unsatisfiable, in 4 cases: the state, the reasons in order, one sentence that names no backend, issues nothing, validates, the message names the value | About 22 | `lease create` with the unsatisfiable values; check `reasons`, no credential folder and no alias file |
| A failed alias step throws, names no backend, leaves no lease folder, keeps a foreign alias file | 4 | Partly: make `agentspace.d` a file in the ssh folder, run a ready `create`, expect `internal` with exit 1, no lease folder and no record. "Keeps the foreign file" is not movable, since the alias name comes from a random ID |
| The real `~/.ssh` is unchanged | 1 | Moot once `USERPROFILE` is a throwaway folder |

### The operator and the token push

**The push's checks stay prototype code until a build meets the seam that
[docs/operator.md](operator.md#the-test-seam) specifies** (E0.04-T03-S02): the environment
variables that name the `aws` and `ssh` programs the operator runs, which a check needs to stand in
for them. Until a build meets it, no stub can sit in front of the push, and **every group of
`test-push-localswim.ps1` below stays prototype code, except the three that need no stub: the
refusals of `-Alias` and of the ProxyCommand's arguments, which moved to `conformance/operator/`.**

#### `conformance/operator/`: conformance

**Three checks, one helper file and no shim path.** Each check dot-sources `helpers.ps1`, which
dot-sources the harness (`conformance/lease-cli/harness.ps1`), so the programs are named by
`AGENTSPACE_OPERATOR_CMD` and `AGENTSPACE_SSH_PROXY_CMD` and run in a throwaway home with the
harness's whole environment ([The harness](#the-harness)). Each ends with `Exit-ConformanceRun`, and
each is run as `pwsh -NoProfile -File conformance/operator/<file>.ps1`.

| File | What it enforces | Against the prototype |
|---|---|---|
| `test-alias-refusals.ps1` | `up -Alias <bad>` and `status -Alias x` are refused before anything runs, with exit 1, the message and an empty stdout: 11 bad values (`a;b`, `a b`, `a&b`, the empty value, `a\|b`, a value with a double quote, `-Fx`, `a/b`, `a%b`, the Kelvin sign U+212A and U+0130) and the `status` case ([docs/operator.md](operator.md#command-line)) | 12 PASS, exit 0 |
| `test-proxy-arguments.ps1` | The ProxyCommand refuses a second argument before `up`, and fails in `up` on a bad alias with no tunnel started: `a/b extra`, `a;b`, `-Fx`, the empty argument, `a b` and `a/b` ([docs/operator.md](operator.md#the-proxycommand)) | 6 PASS, exit 0 |
| `test-unmet-proxy-connect-time.ps1` | D1 at connect time, card #0087, through the seam ([The files that are expected to fail](#the-files-that-are-expected-to-fail)) | FAIL, as expected |
| `helpers.ps1` | The AWS gate, a runner for a value that cmd.exe needs quoted, and the code points a value arrives with. It starts no program under test | Not a check |

- **An alias holding a line feed is not checked, and is a gap** ([The gaps](#the-gaps)). A line
  feed cannot cross the `cmd.exe` command line: it cuts the value `a<LF>` to `a`, a valid alias, so
  a run could get past the refusal and reach AWS. The case is left out rather than SKIPPED, so that
  the file can pass.
- **The AWS gate runs first, and probes the AWS CLI that the operator runs.** Before any program
  that can reach AWS starts, the check runs `aws sts get-caller-identity` under the home's
  environment, for the fixed `C:\Program Files\Amazon\AWSCLIV2\aws.exe` of
  [docs/operator.md](operator.md#programs-it-starts) and for the `aws` that `PATH` finds, whichever
  exist. **Every case is SKIPPED, and nothing runs, when no AWS CLI is found or when that call
  succeeds for any of them.**
- **A value reaches the operator whole.** The harness starts programs with .NET's argument quoting,
  which leaves `&` and `|` bare, and `cmd.exe` reads them as operators. The alias checks therefore
  run the operator through a one-line `.cmd` in the home that quotes the value, as an ssh
  ProxyCommand line does. **A value with a double quote is escaped with a backslash.** The other
  checks pass their arguments to the harness directly.
- **Windows PowerShell 5.1 passes U+212A and U+0130 intact.** Before each of those two cases, the
  check sends the value through the same `cmd.exe`, `.cmd` and `powershell.exe -File` path and
  compares the code points it receives (8490 and 304), and SKIPS the case when they differ
  ([open question 2](#open-questions)).
- **A program that prints nothing and exits 0 fails every case.** Set to such a `.cmd`, each
  variable makes `test-alias-refusals.ps1` fail its 12 cases and
  `test-proxy-arguments.ps1` its 6.

#### `scripts/test-push-localswim.ps1` (about 460 lines): prototype code

- **Runs under** Windows PowerShell 5.1 only.
- **Loads:** It parses `scripts\agentspace.ps1`, extracts five functions from its syntax tree and
  dot-sources them (the push, the hash of the credential file, the alias test, the laptop's zone,
  and the zone-name conversion). It sets the prototype's script variables and calls the push with
  a test-only zone parameter. Its other parts run copies of the operator in a throwaway repo, and
  copies of `ssh-proxy.cmd` whose `aws` line was edited as text, beside a stand-in operator.
- **Stubs:** A C# stand-in for `ssh.exe`, driven by variables of its own
  (`AGENTSPACE_TEST_SSH` and three more, which are the stub's and not agentspace's). The stand-in
  operator and the edited `ssh-proxy.cmd` stub agentspace's own programs, which D34 does not allow.
- **Folders:** Throwaway only, under `%TEMP%`. The child runs get `LOCALAPPDATA` and
  `USERPROFILE` set to it and the AWS variables cleared, so the real token, receipt and hash file
  are never touched.

| Group of checks | Count | Class | What moving it takes |
|---|---|---|---|
| The hash of the credential file ignores CRs, a BOM and blank lines | 1 | Prototype code | Not movable: it is visible only as the stamp written after a confirmed send, which needs a stand-in box |
| The alias test accepts 3 values and refuses 11 | 14 | The 11 refusals are conformance, **moved** to `conformance/operator/test-alias-refusals.ps1`; the 3 acceptances are prototype code | The refusals, through `up -Alias <bad>`: the operator fails before anything runs, with stdout empty (done: 11 values, the Kelvin sign and U+0130 among them, passed intact by Windows PowerShell 5.1). The acceptances proceed to AWS and cannot be shown |
| The push protocol: a wrong hash, a failed ssh, a hung ssh (the time limit), a confirmed send is recorded, the box gets exactly two lines, the ssh arguments, `-Alias` routes the send, a refused alias starts no ssh, an unchanged token is not re-sent, a forced send | About 25 | Prototype code | Not movable: it needs a stand-in `ssh` in front of the operator ([The gaps](#the-gaps)) |
| The time zone: a valid zone is sent in the remote command, the stamp includes it, a changed zone makes the push due, 16 invalid zones push the token and send no zone and warn, an empty zone, `zone-failed`, and the zone never changes whether the token counts | About 55 | Prototype code | Not movable, for the same reason |
| The zone tables: 7 legacy names, 4 current zones that pass through, 8 Windows IDs to IANA names, an unknown ID, `-TimeZone` is sent as given, no `-TimeZone` reads the laptop's zone | About 22 | Prototype code | Not movable: pure functions, and the only observable is the zone in the ssh command. They are a spec table ([docs/operator.md](operator.md#the-time-zone)) with no way to run it against a program |
| `up -Alias <bad>` fails on the alias, before anything, with nothing on stdout (5 values, 3 checks each); `status -Alias x` is refused | 16 | Conformance, **moved** to `conformance/operator/test-alias-refusals.ps1` | Done: it runs the program `AGENTSPACE_OPERATOR_CMD` names, in a throwaway home with the harness's environment, so a mistaken run cannot reach the token or AWS |
| `ssh-proxy.cmd` with a second argument (`<alias> extra`, `a;b`) is refused before the operator runs, exit 1, the message `ssh-proxy.cmd takes at most one argument`, stdout empty | 4 | Conformance, **moved** to `conformance/operator/test-proxy-arguments.ps1` | Done: it runs the program `AGENTSPACE_SSH_PROXY_CMD` names |
| `ssh-proxy.cmd -Fx`, `""`, `"a b"` and `a/b` fail in `up` on the alias, start no tunnel, stdout empty | 8 | Conformance, **moved** to `conformance/operator/test-proxy-arguments.ps1` | Done: the same program |
| A copy of `ssh-proxy.cmd` runs `up` or `up -Alias <alias>`, then the tunnel, and exits with the tunnel's code (7); it stops when `up` fails with exit 1 | 8 | Prototype code | Not movable: it edits the prototype's source text and plants a stand-in operator next to the copy |

### The keeper suite

#### `conformance/keeper/test-keeper.ps1`, with `keeper-common.ps1` and its own `conformance/keeper/ssh-stub.cmd`: conformance

**E0.04-T03-S02 rewrote the 41 process cases of `scripts/test-board-holder.ps1` as this check, and
added one.** The original stays in `scripts/` with its three installer cases, which are prototype
code ([below](#the-three-installer-cases-prototype-code)).

- **Drives:** The keeper as a separate process, only through the program that
  `AGENTSPACE_KEEPER_CMD` names, run with the keeper's own arguments unchanged
  ([docs/keeper.md](keeper.md#command-line)): `<program> -SshExe <stub> [-Now t]`, and in `-Watch`
  mode with `-IntervalSeconds`, `-MaxPasses`, `-BackoffBaseSeconds`, `-BackoffMaxSeconds`,
  `-FastFailureSeconds` and `-ReleaseGraceSeconds`. The program is found by the harness's
  `Get-ConformanceProgram`: when the variable is unset or empty, the default is the shim
  `conformance/prototype/keeper.cmd`, which `conformance/prototype/defaults.ps1` names, and nowhere
  else; no case names the prototype's script. The variable is read before the first case, because
  a case strips every `AGENTSPACE_*` variable from the environment. A check that names a program
  that is not a file fails.
- **Options:** `-Case <wildcard>` runs only the cases whose name matches. `-SelfCheck` runs each
  case again with a broken fixture, and a case that still passes is reported `CANNOT-FAIL`.
- **Cases:** 42, each named by the section of [docs/keeper.md](keeper.md) or
  [docs/state.md](state.md) that it enforces, which starts every result line
  (`PASS keeper.md#locks: Lock busy: The pass exits 0`):
  21 one-pass cases, 20 `-Watch` cases, and the new case that validates the files a run writes
  against `schemas/state/` ([docs/state.md](state.md#open-questions), Q13).
- **Output:** One `PASS`, `FAIL` or `SKIPPED` line per check, a summary line, and exit 0 when every
  check passed, 1 when any failed, and 2 when none failed but any was skipped. A `FAIL` line says
  what did not hold, and ends with the last lines of `board-holder.log` when the check gives no
  reason of its own. A skip is for a case whose setup cannot be made (the one case that sets an
  access control list on a stub), and a run with a skip is incomplete, never passed.
- **Loads:** Nothing from the prototype. No case reads, dot-sources or calls the keeper's code, and
  none calls the hidden `-Launch` ([docs/keeper.md](keeper.md#checking-the-keeper)).
- **Stubs:** `conformance/keeper/ssh-stub.cmd` stands in for `ssh.exe`, a program outside
  agentspace. The suite copies it into the run's folder and injects it through the keeper's
  `-SshExe` argument, not through `PATH`. Two more stubs are written into each case's folder. For
  one case it puts a sentinel in a variable that is the harness's, not a documented one, to prove no
  secret reaches the log.
- **Folders and variables:** Each case has its own throwaway folder under `%TEMP%`, with a state
  folder (`AGENTSPACE_STATE_DIR`) holding a hand-written `leases.json` and `board-holder.json`, and
  a home folder (`USERPROFILE`). **Every process a case starts, the keeper, its holders and the
  stand-ins included, gets the harness's environment for that home** (`Get-ConformanceChildEnvironment`,
  [Running the suite](#running-the-suite)): `USERPROFILE`, `HOME`, `LOCALAPPDATA`
  (`<home>\AppData\Local`), `APPDATA` (`<home>\AppData\Roaming`), `AGENTSPACE_STATE_DIR` and
  `AGENTSPACE_SSH_DIR` inside the case's folder; `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE`
  naming files that do not exist; `AWS_EC2_METADATA_DISABLED=true`; the AWS profile, key, session
  and both container-credential variables removed; and every `AGENTSPACE_*` variable and
  `PSModulePath` the caller's shell held removed. The suite sets them on its own process, which
  every child inherits, and puts every one back in a `finally`. It never uses the real
  `%USERPROFILE%\.agentspace`, `~/.ssh`, `ssh.exe` or keeper task, and the three installer cases,
  with the one read of the real task, are not in it.
- **Processes:** A keeper a case starts is stopped by the PID of the process the case started, and
  that process's own children, never by a search for a command line. A case that kills a keeper
  spares its holder, which MUST outlive it ([docs/keeper.md](keeper.md#what-the-keeper-does)). The
  run's cleanup stops only processes whose command line holds the run's own unique folder, and
  its last check is that none is left. The one case that denies `PROCESS_TERMINATE` on a stub
  keeps that stub's process object, whose handle pins its PID, and its cleanup gives the right back
  to that stub alone, through the handle, after its start time matches the one recorded at the
  lock. It never rewrites the access control list of a PID it did not lock.
- **Shared helpers:** `conformance/keeper/keeper-common.ps1` holds everything the keeper checks
  share (the process tree, the stubs, the fixtures, the `-Watch` helpers and the environment of a
  case), so that `test-keeper.ps1` and `test-unmet-holder-dies-mid-pass.ps1` run their keepers
  the same way. It dot-sources the harness and loads nothing from the prototype.
- **Windows-only harness features:** Reading the process tree through CIM, a process access
  control list set to deny termination, a held `board-holder.lock`, and a watcher that keeps a
  copy of the launch files, which live only while a holder is started.

**What replaced each fixed sleep (#0066).** Every case waits on what it checks for, with a deadline
that a loaded laptop can meet, and a wait on a process ends early when that process has exited. The
three fixed sleeps that reading found are gone:

| The prototype's sleep | The wait that replaces it |
|---|---|
| 5 seconds, so that two tries fall inside the hold of a log (case: a log that cannot be written) | A wait, up to 60 seconds, for the stub's count of starts to reach 2: the second try is the proof the loop went on, and the log is released when it is seen |
| 3 seconds after the first `grace seconds=` line, to show that no second holder starts early (case: the 95-second default) | A wait of up to 20 seconds for a second start, which the case expects not to happen. **This is the one deadline a case expects to reach**: a negative has no other thing to wait on, and a keeper that started a holder early ends the wait at once and fails. The grace is 95 seconds, and 20 of them are long enough for a start that the grace was meant to prevent to show itself; the case still never waits the 95 |
| 2 seconds after the stop, before the second lease becomes ready (case: the holder after a stop) | A wait, up to 60 seconds, for the keeper's `grace seconds=` line, which it writes when it begins the wait after the stop |

**Two more bounds were tightened for the same reason**: the second keeper in the "exits at once"
case now runs 60-second intervals and is waited on for up to 90 seconds, so that a keeper that
took no lock fails by running on, never by a stopwatch under 10 seconds; and the "first holder is
not delayed" case uses a grace of 3600 seconds and a deadline, instead of a 40-second stopwatch.

**The cases follow [docs/keeper.md](keeper.md) and [docs/state.md](state.md) alone.** Against the
prototype they found no disagreement with either. The requirement they do not cover is #0068,
which is checked by its own file, expected to fail against the prototype
([The #0068 file](#the-0068-file-test-unmet-holder-dies-mid-passps1)).

**Not yet shown:** #0066's done-when, that the cases pass 5 times in a row beside two other
copies of the suite and a live keeper, was not run by this story. What was run, on 2026-11-18, after the
cases were given the harness's environment and the 20-second wait: the full suite against the
prototype (206 checks passed, 0 failed, 0 skipped, exit 0, in 7 minutes 15 seconds); the full suite
against a keeper that prints nothing and exits 0 (119 of its 196 checks failed, and every case failed
at least one, exit 1, in 7 minutes 13 seconds); and the #0068 file ([below](#the-0068-file-test-unmet-holder-dies-mid-passps1)).
`-SelfCheck` ran in two groups before that change, with every case failing on its broken fixture,
in about 7 minutes 22 seconds each; after it, only the Idle case was run again.

| Cases | Count | Class | Notes |
|---|---|---|---|
| Idle; Start; Keep; Stop on release; Stop on expiry; Stop at the expiry second; Stop when the lease is gone; Stop leaves a foreign process alone; Dead PID; Reused PID (ready, released); Command line differs; Foreign process (released, ready); Last expiry; Unreadable lease state (with and without a holder); A holder that cannot be opened is kept; Lock busy; Bad `-Now`; Rotation of `board-holder-ssh.log` | 21 | Conformance | They read and write `leases.json`, `board-holder.json`, `board-holder.log`, `board-holder-ssh.log` and `board-holder.lock`, each documented in `keeper.md` or `state.md`. Two of them (Reused PID released, Command line differs) gained a check that the untrusted record is removed and one idle line is logged, because they passed against a keeper that does nothing |
| Watch: the backoff doubles and is capped; it resets after a long-lived holder; a holder that dies unproven backs off; a log that cannot be written does not end the loop; `-MaxPasses` ends the loop; passes are spaced; a locked pass is skipped; the log holds no secret; the next holder waits the release grace; a first holder is not delayed; a backoff longer than the grace is still waited; the holder after one the keeper stopped waits the grace; the grace is 95 seconds by default; `-ReleaseGraceSeconds <bad>` is a usage error (4 values); a second keeper exits at once; a new keeper starts once the first is killed and adopts its holder; a one-pass run is unaffected by a running keeper | 20 | Conformance | They depend on the flags above and on the `grace seconds=` and `another keeper is running pid=<n>` log lines. The kill case kills the process the suite started, by PID, and spares the holder |
| The files a keeper run writes match `schemas/state/`: the holder record, the launch request and the launch result against their schemas; the files' bytes; the claim's content; the launch files removed at the pass's end | 1 (new) | Conformance | Answers [docs/state.md](state.md#open-questions)'s Q13. The launch files live only while a holder is started, so a watcher inside the case keeps a copy of each as it appears, and a pass it missed is run again |

#### The #0068 file: `test-unmet-holder-dies-mid-pass.ps1`

**`conformance/keeper/test-unmet-holder-dies-mid-pass.ps1` checks card #0068, and is expected to
FAIL against the prototype** ([The files that are expected to fail](#the-files-that-are-expected-to-fail)).
Its header says so, its `FAIL` lines still say `FAIL`, and a rewrite's keeper MUST pass every line.

- **The requirement** ([docs/keeper.md](keeper.md#release-grace)): when a keeper tracks a holder and
  its pass does not find that holder trusted, the pass MUST NOT start a holder, and the keeper waits
  the release grace after that pass, logged `grace seconds=<n>`. A holder recorded at the start of a
  pass and gone by the pass's decision therefore counts as ended, and the grace applies at once. The
  prototype removes the record and starts a holder at once, which fails on `ExitOnForwardFailure`
  while the dead session holds the ports.
- **Drives:** The same program, the same way, and the same environment as the suite, through
  `keeper-common.ps1`, with the harness's result lines (`PASS`, `FAIL`, `SKIPPED`, and a summary line)
  and exit codes. A keeper tracks one stub holder, with a grace of 15 seconds and a backoff of 1.
- **Case 1, a holder that exits while the pass waits for `board-holder.lock`:** the check holds the
  lock, so that the keeper's pass waits for it, stops the tracked stub while the pass waits, and lets
  the lock go ([docs/keeper.md](keeper.md#open-questions), Q9). **The one wait that is not on an
  event:** the keeper logs nothing when its look finds a holder alive and its pass then waits for the
  lock, so no event marks the window. The check waits on the keeper's own `skipped` line, which a
  pass writes when its 10-second lock wait runs out, and stops the holder 6 seconds after it: the
  keeper's 1-second interval, then its look, then a new 10-second wait, so the stop lands about 5
  seconds inside the window on either side. **The log then says which path the keeper took, and an
  attempt that missed the window is not a result.** A `grace` or `backoff` line before the pass's
  own is the look's, a `skipped` line is a pass that gave up, and a `kept` line is a pass that was not
  waiting; each is run again, up to 3 times, and a check that never reached the window is `SKIPPED`,
  never `PASS`.
- **Case 2, the record is gone when the pass looks for the tracked holder:** the same rule, reached
  with no timing. The look reads the process only and never the record
  ([docs/keeper.md](keeper.md#watching)), so a record removed while the holder still runs is a
  holder that the look finds alive and the pass does not find trusted
  ([docs/keeper.md](keeper.md#release-grace) lists the record missing beside the process gone). The
  check removes the record while it holds the lock, with 5 seconds before the keeper's next pass, and
  the log says whether that pass came after it.
- **What each case checks**, whatever action word the pass logs ([docs/keeper.md](keeper.md#open-questions),
  Q6): that pass logs no `started` line; the line after it is `grace seconds=<n>`, with n the grace
  less rounding; and the next `started` line comes at least the grace after the check let the pass
  go. Case 2 also checks that the holder was still running. The grace counts from the end of the pass,
  which is never before the check released the lock, so a keeper that meets the rule passes.
- **Against the prototype:** each case fails three checks and passes the rest. The pass logs
  `started` at once (the first check, whose message quotes the line), the line after it is `kept`
  and not a grace line (the second), and the next `started` line is that same one, stamped with a
  time before the check let the pass go (the third). Case 2's check that the holder was still running
  passes. The file exits 1: 2 passed, 6 failed, 0 skipped, in about 42 seconds. Against a keeper that
  prints nothing and exits 0, each case fails its first check, that the keeper starts a holder.
- **Not shown:** The file has not passed, because no build meets #0068. Its assertions were run
  against a made-up log of a keeper that meets the rule, to show that they can pass, and case 1
  reached its window on the first attempt in each of its runs against the prototype; a loaded laptop
  is what could make it miss, and it would then say `SKIPPED`, never `PASS`.
#### The three installer cases: prototype code

**These stay in `scripts/test-board-holder.ps1`, whole and untouched, as prototype code.** The
rewrite MUST NOT read them. They are the three cases that load or test the prototype's packaging:

| Case | Count | Class | Notes |
|---|---|---|---|
| Installer: `-DryRun` prints the logon task and changes nothing (about 28 checks: task name, two triggers, repetition, run level, battery, restart count, the working directory, the action, the arguments) | 1 case | Prototype code (packaging) | The expected values are the prototype's packaging (a launcher program, the repository root as working directory). It also reads Terry's real scheduled task, read only, before and after, which no conformance check does. A rewrite installs its keeper its own way |
| Installer: a running keeper is found by its command line | 1 case, 5 checks | Prototype code | It loads a prototype function from the installer's syntax tree |
| Installer: `-DryRun` warns from a worktree | 1 case, 2 checks | Prototype code | It tests the prototype's install location, not the keeper |

**The 41 process cases are also still in `scripts/test-board-holder.ps1`**, because a story MUST
NOT edit that file and the suites left in `scripts/` MUST keep passing until the rewrite replaces
them. The dev lead decides whether to remove them there once the move is merged, so that each case
has one home.

### The schemas check

#### `conformance/schemas/test-schemas.ps1` (moved from `scripts/test-schemas.ps1` by E0.04-T03-S02): neither kind

- **Does:** Runs `Test-Json` in-process on the JSON schemas and their samples: every sample
  validates, every invalid sample is rejected, the list schema's `$defs` equal the lease schema's,
  the TTL grammar is accepted and refused through the two input schemas, and a sample's host alias
  is `agentspace-<project>-<first 8 hex>`.
- **Loads:** No product code. It reads `schemas\`, `schemas\samples\` and nothing else.
- **Class:** Neither, and now conformance in the one sense that it has no program to name: it is in
  `conformance/`, so the rewrite MAY read it. It drives no program, so there is no variable. **It
  checks the package itself, so it cannot be pointed at another build, and the "program that prints
  nothing" test does not apply to it.**
- **What moving it changed:** The repository root is two folders up, not one. Its output is the
  suite's result lines and its exit code is the suite's ([The keeper suite](#the-keeper-suite),
  "Output"). Nothing else changed: it checks the same 289 things as before, and it still scans the
  top level of `schemas/` only. **The three keeper schemas in `schemas/state/` are checked by the
  keeper suite's case against the files a run writes, not by this check**
  ([docs/state.md](state.md#open-questions), Q13).
- **The copy in `scripts/test-schemas.ps1` is the same check**, left in place because a story MUST
  NOT edit a `scripts/test-*.ps1`. The dev lead decides whether to remove it once the move is
  merged, so that the check has one home.

### The live-box check

#### `scripts/check-forward-release.ps1` (about 360 lines): neither kind

- **Does:** Checks that sshd on the box drops a dead client's session within a bound, and that a
  live idle session survives twice the bound (card #0044). It is run by hand,
  `pwsh -NoProfile -File scripts\check-forward-release.ps1 -Alias <alias>`.
- **Needs:** A ready lease's alias and a real box. It starts its own `ssh -N` processes against
  the box, and takes at least 4 minutes against the default 120-second bound. It creates, renews
  and releases nothing, and does not edit `~/.ssh`.
- **Class:** Neither. It is a live acceptance check of the box's sshd, not a laptop conformance
  check, and it is not a `test-*.ps1`. It stays in `scripts/`, and the conformance suite does not
  cover it ([The gaps](#the-gaps)).

## The header line in each script

**Each suite sorted above, and the keeper's stub, carries one comment line, right after any
`#Requires` line, that says its class and points here** (a `REM` line in the `.cmd`).
`scripts\check-forward-release.ps1` is a live check of the box, not a suite, and carries none: this
page is the only place that sorts it. The wording is one of these:

| Class | The header line |
|---|---|
| Prototype code | `# Prototype code under D34: the rewrite MUST NOT read this suite. See docs/conformance.md.` |
| Mixed | `# Mixed under D34: its process checks are conformance in method and its in-process checks are prototype code; the rewrite MUST NOT read this suite. See docs/conformance.md.` |
| Conformance in method | `# A conformance check in method under D34, not yet moved to conformance/: the rewrite MAY read it only once it is moved. See docs/conformance.md.` |
| The keeper suite | The same line, and says its three installer cases are prototype code |
| Neither (the schemas check) | `# A spec self-check under D34: it drives no program and loads no prototype code; the rewrite MAY read it only once it is moved to conformance/. See docs/conformance.md.` |
| A stub for `ssh` | `REM A stub for ssh (a program outside agentspace) used by a conformance check in method not yet moved to conformance/. See docs/conformance.md.` |

## Which spec section each check would enforce

**By spec section, for every check sorted above as conformance or conformance in method, and for
the prototype-code checks that can be rewritten as one.** A section of `docs/state.md`,
`docs/driver.md` or `docs/keeper.md` is cited by the title that the turn's pins fixed. Those docs
land with this page's turn, so a link to one resolves once they merge.

| Check group | Spec sections it would enforce |
|---|---|
| Help at every level, and `--help` in any position | [docs/lease-api.md](lease-api.md): Commands (every layer), Command lines |
| Usage errors | lease-api.md: Command lines, Exit codes and errors, Messages |
| `invalid_value` and `out_of_range` | lease-api.md: Values (each subsection), Exit codes and errors |
| The error schema refuses a line feed | lease-api.md: Schemas |
| The normalizer probes | lease-api.md: Input tolerance, Values, Times |
| The state file group | [docs/state.md](state.md): The lease state file; lease-api.md: States, Fields, Output |
| A failed update changes nothing; an invalid file is `internal` | lease-api.md: Concurrency and atomicity, Exit codes and errors; state.md: The lease state file |
| Concurrent writers and a reader | lease-api.md: Concurrency and atomicity; state.md: Concurrency |
| `create` and `get`: a ready lease, TTL, uppercase ID, unsatisfiable, tolerant command line, a failed write | lease-api.md: `create`, `get`, States, Times, The credential; state.md: A lease's credential |
| `renew`, `release` and `list` | lease-api.md: `renew`, `release`, `list`, States; state.md: A lease's credential |
| The certificate's fields | lease-api.md: The credential; state.md: A lease's credential, The CA |
| The alias file and the `Include` line | lease-api.md: The credential, Environment; state.md: A lease's credential |
| Unsatisfiable `create`; a failed alias step | lease-api.md: `create`, States, Fields (`reasons`), Concurrency and atomicity; [docs/driver.md](driver.md) |
| The schemas check | lease-api.md: Schemas, Values (TTL) |
| `-Alias` refusals, and `ssh-proxy.cmd`'s argument refusals | [docs/operator.md](operator.md): Command line, The ProxyCommand |
| D1 at connect time (expected to fail against the prototype) | operator.md: What a caller sees (D1, card #0087), The test seam, The ProxyCommand |
| The keeper's cases (one pass) | [docs/keeper.md](keeper.md): Log lines, Locks, Starting a holder; state.md: The keeper's files |
| The keeper's `-Watch` cases | keeper.md: Log lines, Locks, Starting a holder |
| The keeper's files against `schemas/state/` | [docs/state.md](state.md): The keeper's files (the holder record, a holder's launch files), and its Q13; keeper.md: The handshake |

## The gaps

**A gap is a spec section that no sorted check covers.** The sorted checks leave these.

| Spec section | Gap |
|---|---|
| operator.md: The token push (all of it: When it runs, What the laptop reads, The time zone, The stamp, What the laptop sends, What the box does, What the laptop checks, Secrecy) | **About 110 checks cannot be moved, and no conformance check covers the push.** The operator starts `aws` by a fixed full path that exists on the laptop and `ssh` by a full path under `%SystemRoot%`, so no run of `up` or `sync` reaches the push with a stand-in box. The seam is specified now ([docs/operator.md](operator.md#the-test-seam): `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD`, `AGENTSPACE_SCP_CMD` and `AGENTSPACE_TIMEZONE`, which close its Q11 and Q12), but the prototype reads none of them. **Until a build meets it, the push's checks stay prototype code, and no conformance check covers the push** |
| operator.md: The time zone | The zone and legacy-name tables are spec tables with no entry point from outside, and so no check |
| operator.md: Commands (`deploy`, `up`, `sync`, `ca`, `down`, `status`, `grow`), Starting the box and waiting for it, The box as the laptop sees it, The AWS side | None of the 11 files runs these. They need AWS or a box, or stubs of `aws` and `ssh` that the operator cannot take today |
| operator.md: Command line | An alias holding a line feed has no check: `cmd.exe` cuts the value at the line feed to a valid alias, so a run could get past the refusal and reach AWS. A check of it needs a way to start the operator that does not pass through `cmd.exe` |
| operator.md: The ProxyCommand | The argument refusals are checked (`conformance/operator/test-proxy-arguments.ps1`), and what a caller sees by `test-unmet-proxy-connect-time.ps1`, which fails against the prototype ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). The three steps (the folders put on `PATH`, `up`, the tunnel) have no check of their own: the test that touched them edits the prototype's text, and a check needs the seam to stand in for `aws` |
| operator.md: `ca` | No suite runs `ca`. The throwaway CA the lease checks need is its first check |
| lease-api.md: States | The expiry boundary (`expired` exactly at `expires_at`, `ready` one second before) needs a clock the check controls |
| lease-api.md: The credential | A short-ID collision, and a caller-chosen lease ID refused by the credential step, cannot be reached through the CLI |
| lease-api.md: `renew`, States | Renew of a `ready` lease with no folder: the sorter found no statement of what happens, and the suite's check of it cannot be moved |
| lease-api.md: Times | No check sets the caller's time zone, as the spec requires ([What the specs require of the suite](#what-the-specs-require-of-the-suite)). Windows has no environment variable for it, so a run uses the laptop's zone ([Running the suite](#running-the-suite)) |
| lease-api.md: Concurrency and atomicity | The 30-second wait, and a failed `renew` or `release` leaving the lease as it was (#0041), have no check. The race check, one group, covers only three writers and a reader |
| lease-api.md: The program | Arguments reach the program unchanged from Git Bash, and an argument such as `-:x` (#0083), have no check. **A line feed inside an argument has none either**: the prototype's `bin\agentspace.cmd` cuts the argument at it, so a project value `a<LF>b` is read as `a` and the create succeeds, where the spec requires `invalid_value`. A check of it would fail against the prototype, so the suite leaves it out until the rewrite |
| lease-api.md: The credential, no CA | `create` MUST fail with `internal` and make no CA when none exists (#0029 point 3). **The prototype's `create` makes one**, so a check of the rule would fail against it, and the suite leaves it out until the rewrite ([docs/state.md](state.md#its-checks)). What is checked: an unsatisfiable `create` with no CA answers and makes none |
| driver.md | The driver boundary is in-process, so no conformance check can drive it. The registration checks are not movable |
| keeper.md: The installed task | The installer dry run is the prototype's packaging, and no conformance check covers an installed task. The three installer cases stay in `scripts/` as prototype code |
| keeper.md: Release grace, card #0068 | **Checked by `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1`, which fails against the prototype** ([The #0068 file](#the-0068-file-test-unmet-holder-dies-mid-passps1)). The gap that remains is the window itself: the keeper logs nothing when its look finds a holder alive and its pass then waits for `board-holder.lock`, so no event marks the moment between the look and the trust check. The file enters it with one bounded wait from the keeper's own `skipped` line, reads the log to see which path the keeper took, and reports SKIPPED if it missed. The second case in the file reaches the same rule with no timing, through a record that is gone |
| keeper.md: Checking the keeper, card #0066 | The 5 passes in a row beside two other copies of the suite are not shown. The waits are conditions with deadlines, but no run of this story was made under that load |
| keeper.md: The task's launcher; Stopping and starting the keeper | Nothing runs the launcher, the install, or the End and restart steps: they need the real Task Scheduler |
| keeper.md: Starting a holder | The refusals of "Before the start" (a state folder or a keeper path holding a `"`, an alias outside the form), the create retries and the 35-second, 60-second and 95-second bounds have no case. The first needs a path that holds a `"`, which Windows refuses, and an alias outside the form, which the lease state's schema refuses; the rest need a launcher that does not answer |
| keeper.md: Locks | The `board-holder-keeper.lock` error path (any error other than the file being held) and the `board-holder.lock` error that is not "in use" have no case |
| state.md: The keeper's files | The share modes and the access rules of the keeper's files, and the temporary files of a killed pass, have no case; the new case checks the JSON files' schemas, bytes and the launch files' removal only |
| The live-box check | `check-forward-release.ps1` is the only check of the box's sshd, and the conformance suite does not run it |

## The remaining work

**No check was rewritten or moved by E0.04-T02-S04.** The sorting showed about 5.6 hours of
rewriting against a 3-hour story, so the dev lead cut the story at the sorting and this page. **The
rest was three stories in v0.04-T03, carded by the architect** (2026-11-18). **The estimates below
are the sorter's, in agent active minutes, from reading the suites, and not measured.**

| Story | Contents | Estimate |
|---|---|---|
| 1. The lease CLI harness, with `create`, `get` and `list` (done by E0.04-T03-S01, #0084) | The shared helper (a program runner reading `AGENTSPACE_CMD`, a throwaway `USERPROFILE` with a throwaway CA made by the operator's `ca`, throwaway state and ssh folders, cleanup): 25 minutes. The process half of `test-lease-cli.ps1` (help, usage and values): 15 minutes. `create` and `get` retargeted, and `list` with `create`-made seeds: 40 minutes | 80 minutes |
| 2. The remaining lease CLI checks, against a pinned harness interface | The normalizers as probes (35), the state file through seeded `leases.json` (30), `renew` and `release` with `create`-made seeds (25), the certificate and the renew and release checks (25), the alias and the `Include` line (35), and the driver's unsatisfiable and failed-alias checks (25) | 175 minutes |
| 3. The operator, keeper and schemas checks, plus the push seam in `operator.md` (done by E0.04-T03-S02, #0085; the push's checks stay prototype code until a build meets the seam) | The keeper suite parametrized by `AGENTSPACE_KEEPER_CMD` and run twice in full: 60 minutes. The operator's alias refusals and the proxy's argument refusals: 25. The schemas check moved: 10. The push seam is specified in [operator.md](operator.md) by this story: an environment variable naming `ssh` and `aws`, and a way to give the push a time zone (operator.md Q11 and Q12). Until the product has that seam, the push stays prototype code | 95 minutes, without the seam |

**The suites left in `scripts/` MUST keep passing until the rewrite replaces them.** The
conformance suite adds to them, and does not retire one.

## Open questions

**What reading the suites and the specs could not settle.** The clean-room review (#0080,
v0.04-T04) closes them, or names the card that will.

1. **Closed (E0.04-T03-S02): `-FastFailureSeconds`, and every other flag the keeper suite uses,
   is specified.** [docs/keeper.md](keeper.md#command-line) lists it in its argument table, with
   its range (1 to 86400), its default (60) and its meaning, as hidden and for checks only, and
   its "Checking the keeper" section names it among the arguments a check uses. Its sentence "A
   conformance check MUST NOT call the hidden `-Launch`" binds the suite as well: no case does.
   Every other flag the suite passes (`-SshExe`, `-Now`, `-Watch`, `-IntervalSeconds`,
   `-MaxPasses`, `-BackoffBaseSeconds`, `-BackoffMaxSeconds`, `-ReleaseGraceSeconds`) is in the
   same table. **A rewrite's keeper MUST therefore accept `-FastFailureSeconds`, though it is not
   in the usage text.** What stays open is keeper.md's own Q3, the name of the rewrite's entry
   point, which the variable already hides from the suite.
2. **Closed (E0.04-T03-S02): yes, Windows PowerShell 5.1 passes U+212A (the Kelvin sign) and
   U+0130 intact.** On 2026-11-18, on the laptop, a value sent through `cmd.exe`, a `.cmd` and
   `powershell.exe -File` arrived as the code points 8490 and 304, and the operator then refused
   each as an alias, as `conformance/operator/test-alias-refusals.ps1` shows. **A different Windows
   PowerShell build is not assumed to do the same:** the check sends each value through that path
   first and reports its case SKIPPED, with the code points it received, if they differ.
3. **Does a throwaway `USERPROFILE` make `create` work?** **Settled by E0.04-T03-S01 (#0084): yes,
   on the laptop, with two conditions the harness meets** ([Running the suite](#running-the-suite)).
   `ca` made a CA under a new folder in `%TEMP%` and exited 0, and a ready `create` then worked in
   that home. **The conditions:** the operator needs `PSModulePath` removed when it is started from a
   PowerShell 7 session, and `create` needs `AGENTSPACE_PROXY_COMMAND` set. Without it, the
   prototype's default ProxyCommand fails `create` with `internal` in a checkout that holds no
   `.instance-id`, which [lease-api.md](lease-api.md#environment) and
   [state.md](state.md#the-alias-file) do not say. That is a requirement the specs leave out, and it
   is open for the clean-room review.
4. **Closed (E0.04-T03-S02): the "second keeper" match is gone, and the spec needs none.** The
   suite no longer finds a keeper by its command line. It keeps the process that
   `AGENTSPACE_KEEPER_CMD` started, and stops that process and its own children by PID, so a
   wrapper program and a rewrite that is one binary are stopped the same way. A holder is found by
   the stub's command line, which [docs/keeper.md](keeper.md#checking-the-keeper) says is how a
   check finds it, and the holder's command is
   [docs/keeper.md](keeper.md#the-holder-command)'s. The "another keeper is running" check accepts
   the PID of the process the suite started, or one of its descendants, or `-`, and reads
   `board-holder-keeper.lock` for the same PID ([docs/state.md](state.md#the-keepers-lock-files)).
   A rewrite that starts its holder as its own child is also stopped correctly: when a case kills
   a keeper, the suite spares every process whose command line is the holder's, so the holder
   outlives the kill as [docs/keeper.md](keeper.md#what-the-keeper-does) requires.
5. **MAY the live-box check become a conformance check?** It loads no prototype code but needs a
   real box and a ready lease. Whether it moves, runs by hand, or is dropped is not settled.
6. **Where does a check that needs the clock (the expiry second) get one?** The state file's
   times can only be seeded far past or far future. A rewrite may offer a test clock, which the
   spec would have to document, or the check stays out of the suite.

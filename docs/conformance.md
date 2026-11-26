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

**The prototype was removed from the repository by E0.06-T06-S01 (#0135), 2026-11-20** (decisions
[D41](decisions.md#d41-v006-cuts-over-on-the-laptop-with-no-lease-ready-and-terrys-yes-for-each-change-to-his-configuration)
step 8 and [D42](decisions.md#d42-v006-builds-the-operator-and-the-keeper-as-crates-around-two-pinned-boundaries-in-six-turns)).
The rest of this page was written while it existed, and **it is kept as history where it describes
the prototype, never as an instruction to run it**:
- **The checks in `scripts/` that the sorting below describes no longer exist**, and neither do
  `bin\agentspace.cmd`, `conformance/prototype/` (the default shims and their defaults file) and
  `conformance/lease-cli/test-interop.ps1`. What stays in `scripts/` is the architect's process tools
  and `scripts/check-forward-release.ps1` ([The live-box check](#the-live-box-check)).
- **The variables' defaults are now the install folder's programs**, named by `conformance/defaults.ps1`
  ([Naming the program under test](#naming-the-program-under-test)), and there are four variables,
  not five: `AGENTSPACE_PEER_CMD` retired with `test-interop.ps1` ([Interop retired](#interop-retired)).
- **Every `test-unmet-*` file and every "Requirements the prototype does not meet yet" row below is
  now an ordinary check that the Rust programs pass** (they did, in
  [docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)). The words "expected to fail against the
  prototype" are what was true when each file was written. A `FAIL` in any of them is now a defect in
  the build or in the check, never an expected result.

## Contents

- [The rule](#the-rule)
- [Naming the program under test](#naming-the-program-under-test)
- [What a conformance run needs](#what-a-conformance-run-needs)
- [What the specs require of the suite](#what-the-specs-require-of-the-suite)
- [Running the suite](#running-the-suite)
- [The sorting](#the-sorting): [the summary](#the-summary), then
  [the lease CLI](#the-lease-cli-suites), [the operator and the token push](#the-operator-and-the-token-push),
  [the keeper](#the-keeper-suite), [the schemas](#the-schemas-check),
  [the idle check's test](#the-idle-checks-test) and
  [the live-box check](#the-live-box-check)
- [The header line in each script](#the-header-line-in-each-script)
- [Which spec section each check would enforce](#which-spec-section-each-check-would-enforce)
- [The files that are expected to fail](#the-files-that-are-expected-to-fail)
- [The gaps](#the-gaps)
- [Coverage of the operator and the keeper](#coverage-of-the-operator-and-the-keeper)
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
below are **pinned** (the architect, 2026-11-18; a fifth, `AGENTSPACE_PEER_CMD`, was added by D38,
2026-11-19, and retired by E0.06-T06-S01, 2026-11-20: [Interop retired](#interop-retired)).
The harness reads `AGENTSPACE_CMD` and `AGENTSPACE_OPERATOR_CMD` (E0.04-T03-S01, #0084); the checks
of E0.04-T03-S02 (#0085) read the operator's and ProxyCommand's by `Get-ConformanceProgram` too.
**A variable that is unset or empty takes its default from `conformance/defaults.ps1`, and from
nowhere else**; a variable that is set always wins. **Every default is the full path of one program
in the install folder, `%USERPROFILE%\.agentspace\bin`** ([operator.md, Programs and install layout](operator.md#programs-and-install-layout);
D37), so a run with no variable set tests the programs that are installed. (Until E0.06-T06-S01,
2026-11-20, the defaults named the prototype's entry points and shims, in `conformance/prototype/`.)

| Variable | Names | Default, as `conformance/defaults.ps1` gives it |
|---|---|---|
| `AGENTSPACE_CMD` | The lease CLI, run as `& $env:AGENTSPACE_CMD lease ...` with its arguments unchanged | `agentspace.exe` in the install folder |
| `AGENTSPACE_OPERATOR_CMD` | The operator (`deploy`, `up`, `sync`, `ca`, `down`, `status`, `grow`) | `agentspace-operator.exe` in the install folder |
| `AGENTSPACE_KEEPER_CMD` | The keeper | `agentspace-keeper.exe` in the install folder |
| `AGENTSPACE_SSH_PROXY_CMD` | The ProxyCommand program | `agentspace-ssh-proxy.exe` in the install folder |

**The install folder is found from the real profile folder, never from `USERPROFILE`.** The harness
points `USERPROFILE` (and `HOME`, `LOCALAPPDATA` and `APPDATA`) at a throwaway home for every program
it starts, and a suite MAY already have done so in its own process before a default is read.
`conformance/defaults.ps1` therefore asks Windows for the account's profile folder
(`[Environment]::GetFolderPath('UserProfile')`), which does not read the variable, so a default is
the same whichever order a check calls things in. **That file MUST NOT read the variable, and MUST
read, list and hash nothing in the install folder:** it builds four paths. The only way a check
touches an installed program is by running it, inside the throwaway home, as AGENTS.md's "Terry's
real files in a review" allows.

**Why four and not one.** The story card asked for `AGENTSPACE_CMD` for the lease CLI and the
operator. They are separate programs, and the keeper and the ProxyCommand are two more, so each
needs a name.

**Each variable names a program that runs its role with the command line its spec gives**
(architect, 2026-11-18): `AGENTSPACE_CMD` with [lease-api.md](lease-api.md#command-lines)'s,
`AGENTSPACE_OPERATOR_CMD` with [operator.md](operator.md#command-line)'s, `AGENTSPACE_KEEPER_CMD`
with [keeper.md](keeper.md#command-line)'s, and `AGENTSPACE_SSH_PROXY_CMD` with
[operator.md](operator.md#the-proxycommand)'s. A check starts the program a variable names with
exactly those arguments, and adds none of its own. The variable holds a program's path and no
arguments.
- **A rewrite that ships one binary sets each variable to a program that selects the role**, such
  as a small wrapper per role that runs the binary with an argument naming the role and then the
  check's arguments unchanged. It MUST NOT set two variables of different roles to one program that
  cannot tell them apart, because the four command lines collide: with no argument the lease CLI
  MUST fail with a usage error, the keeper MUST run one pass, and the ProxyCommand MUST run `up` and
  its tunnel; and one word is an unknown command to the lease CLI and an alias to the ProxyCommand.
- **A rewrite's variables name its installed programs** ([operator.md, Programs and install layout](operator.md#programs-and-install-layout);
  D37): each is the full path of one program in the install folder, such as
  `agentspace-ssh-proxy.exe` for `AGENTSPACE_SSH_PROXY_CMD`. A check never names the file itself (only `conformance/defaults.ps1` does).
- **The names MUST NOT be `AGENTSPACE_PROXY_COMMAND`.** That variable already means something
  else: it is the command string that `create` writes into a lease's alias
  ([docs/lease-api.md](lease-api.md#environment)). `AGENTSPACE_SSH_PROXY_CMD` is a different
  variable, read by a check to find a program, and it MUST NOT be confused with it.
- **The conformance variables are read by checks, never by a program under test.** So
  [docs/lease-api.md](lease-api.md#environment)'s rule, that no other environment variable
  changes the contract's behavior, is unaffected.
- **The only file in `conformance/` that names a program's file is `conformance/defaults.ps1`.** A
  check and the harness never do. (Until E0.06-T06-S01, 2026-11-20, the only files that named the
  prototype were its default shims and their defaults file, in `conformance/prototype/`.)
- **The token push has no program of its own,** because it is a part of the operator's `up` and
  `sync`, so it has no variable. [The gaps](#the-gaps) say what it needs instead.

## What a conformance run needs

**RFC 2119 keywords again: these bind every conformance check.**

- **Stubs MUST stand only for programs outside agentspace** (`ssh`, `aws`), as D34 says. A stub
  of one of agentspace's own programs is prototype code in disguise. The keeper suite's `ssh`
  stub, `conformance/keeper/ssh-stub.cmd`, is the model: it prints one line, runs for about 20
  minutes unless stopped, and connects nowhere. (Until 2026-11-19 this named the prototype's stub,
  `scripts\test-board-holder-stub.cmd`, which is outside a clean-room export; architect, for
  v0.06, gap FB-26.)
  - **A stub MUST NOT receive the real token** ([docs/operator.md](operator.md#secrecy)).
- **A check MUST work in throwaway folders only.** Each folder is made under `%TEMP%` and removed
  when the check ends, in a `finally`.
- **A check MUST NOT read or write Terry's real `~/.ssh`, `%USERPROFILE%\.agentspace`, or keeper
  task.** It MUST NOT hash or list them as a proof either, since that reads them,
  and the proof is moot once they are out of reach. The check keeps the real ones out of reach
  instead:
  - **The state and ssh folders** are named through `AGENTSPACE_STATE_DIR` and
    `AGENTSPACE_SSH_DIR` ([docs/lease-api.md](lease-api.md#environment)).
  - **The lease's key, certificate and the CA** have no folder override
    ([docs/state.md](state.md#what-moves-which-files)), so a check that places a
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
  stands for, with a deadline. **Done for the Rust build's run (E0.06-T04-S01):** none of the three
  remains, and the "Watch locked" case waits on its log line and on the process's exit
  ([The keeper suite](#the-keeper-suite)).
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

**Three requirements in the specs say what the conformance suite MUST do. Where a conformance
check meets one, its bullet says so.**

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
  - **Met for a certificate's validity** by `conformance/lease-cli/test-lease-certificate.ps1`
    (E0.04-T03-S03). It compares valid-before with `expires_at`, and the issue time with its own
    clock (to the second, within the `create` call), after converting `ssh-keygen -L`'s local time
    to UTC. It reports SKIPPED when the machine is set to UTC, because a process's zone cannot be
    set on Windows, so a run on a UTC machine is incomplete. A run on a laptop set to another
    zone passes.
  - **Who makes the run outside UTC, and how (A-28, B-52; E0.04-T05-S05).** No spec defines a
    time-zone setting that a check can give the lease CLI: Windows has no environment variable
    for a process's zone, and [lease-api.md](lease-api.md#times) defines none, so a check cannot
    make a program run in another zone on a machine that is set to UTC. The only way is the
    machine's own zone, so **the run's requirement is a machine whose Windows time zone is not
    UTC**, and a run on a UTC machine is incomplete, never passed (exit 2, with the SKIPPED
    results named under [Running the suite](#running-the-suite)).
    - **The agent or person who runs the suite for a build MUST make that run**, and it is the run
      that a build's claim to conform rests on. The dev lead who finishes a build runs the suite on
      the Windows laptop, whose zone is its user's own, and a run there at a UTC offset of 0 is
      incomplete like any other; a dev lead that works on a lease, which cannot run the suite,
      leaves the run to the architect. The architect
      SHOULD repeat the run on the laptop when it accepts the build. A runner on a UTC machine, such
      as a build server, MAY run the suite, and MUST report that the run is incomplete.
    - **The suite MUST NOT change the machine's zone**, because that is a system setting. A person
      who wants a complete run on a UTC machine sets the zone themselves, runs the suite and sets
      it back.
    - **The token push's zone is another matter.** [operator.md](operator.md#the-test-seam)
      defines `AGENTSPACE_TIMEZONE` for the push, and `test-unmet-push.ps1` uses it. It changes
      nothing in the lease CLI, so it does not meet this requirement.
- **The `usage` error's `field`** ([docs/lease-api.md](lease-api.md#schemas)). The error schema
  cannot say which `usage` errors carry `field`, so a conformance check MUST test that rule. The
  first half of `scripts\test-lease-cli.ps1` compares the code and `field` of each error run, so
  most of it is in place.
  - **Met by E0.04-T03-S01 (#0084):** `conformance/lease-cli/test-usage-errors.ps1` runs each of the
    five cases of contract v0 that the spec names, in every command that has one, and about 30 usage
    errors that MUST carry no `field`.
  - **From contract v1 there are six cases** (lease-api.md, [Changes from
    v0](lease-api.md#changes-from-v0); D81): the sixth is `--plan` with `--size`, whose `field` is `plan`.
    **It is in `conformance/lease-cli/test-plan-errors.ps1`, with its ranking among the other usage
    errors** (E0.10-T04-S01, #0221), and not in `test-usage-errors.ps1`, whose header says "exactly six
    cases MUST" and where the five of v0 stay. The file still runs the other 30 or so usage errors that
    MUST carry none.
- **Concurrent processes** ([docs/lease-api.md](lease-api.md#concurrency-and-atomicity)). The spec
  requires commands to be safe under concurrent processes
  ([docs/state.md](state.md#concurrency)), so a race check through the CLI is a conformance check,
  and the suite MUST contain one. The prototype's one race check is in-process, so it is not.
  - **Met by `conformance/lease-cli/test-lease-concurrency.ps1`** (E0.04-T03-S03), as the two
    sections cited above ask. Rounds A to E start several lease CLI processes at one instant (a
    barrier releases them together) and read the state file after they end, for `create`s, `renew`s
    and both together. Round G holds the lease state lock itself, starts `create`s, `renew`s and a
    `release`, waits until every process is running and none has finished, and lets go. A process seen
    running is not proved to be blocked on the lock, so this makes it likely, not certain, that
    they all reach it at about the same moment. Round F holds the lock and checks that `get` and `list` do not wait
    and that a `create`, a `renew` and a `release` each fail with `internal` and say to try again.
    After every round the check reads the state file and tests what those two sections require of
    it: that it validates, that each lease a `create` printed is in it exactly once and nothing else
    is, and that each renew's record, certificate and printed expiry agree. Rounds B to E and G also
    need a command to wait for the lock for up to 30 seconds, which
    [lease-api.md](lease-api.md#concurrency-and-atomicity) requires (architect ruling, 2026-11-18),
    so a build that gives up sooner fails them.
  - **A pass is evidence, not proof.** A build that checks the lock and lets go of it before it
    writes can pass rounds A to E whenever its processes happen not to collide, and round G narrows
    that gap without closing it: a race that needs one interleaving can pass any number of runs.
    **An earlier version of the check (rounds A to F, without G) was shown to fail** against a scratch
    copy of the prototype with its lock opened shared, which loses updates in round A's first round and
    in rounds B, C and E. Round G was added after review, and has not been shown to fail against such
    a copy. Against a program that prints nothing it fails, because its processes end while the check
    still holds the lock.

### SHOULDs not checked

**A conformance check enforces MUSTs only** (the architect, 2026-11-18). A SHOULD is never a PASS
or a FAIL, because a conforming build MAY choose otherwise for a good reason. These are SHOULDs the
specs make and no check enforces:
- **The certificate's valid-after time, 5 minutes before its issue time** ([state.md, The
  certificate](state.md#the-certificate); D24, #0029 point 1). The certificate check holds only to
  "not later than the issue time, and not an hour or more before it", which a build of either choice
  meets.
- **The wording of a message** ([lease-api.md, Messages](lease-api.md#messages)): a build MAY word
  its messages differently where the spec does not say MUST. A check looks for what a MUST asks a
  message to say (that a failed lock wait says to try again) and for nothing more. **From v0.09 two
  messages are byte for byte by the EC2 driver's own rule**, not the contract's: `unsupported_size`'s
  and D66's `no_capacity` message for a host serving smaller leases ([lease-api.md,
  Fields](lease-api.md#fields); [driver.md, What it supports](driver.md#what-it-supports); pin P8).
  Z2 and Z4 check them byte for byte, as the driver's rule.
- **The alias file's comment** ([state.md, The alias file](state.md#the-alias-file)): it SHOULD say
  that agentspace wrote the file and that it MUST NOT be edited. A check requires one comment after
  a semicolon, and no words.
- **The order of a renew's steps** ([state.md, Renewal](state.md#renewal)): the rewrite SHOULD follow
  it. A check sees only what a renew leaves, and what a failed one leaves.

### Cases that run only when asked for

**`up`'s 15-minute deadline is a MUST, checked by a case that waits the real 15 minutes, which a
run reaches only by asking for it** (architect, 2026-11-19, for E0.06-T04-S02). operator.md fixes
the deadline and the test seam has no variable that shortens it, and none is added: the deadline's
logic is shown by the operator's own tests with a fake clock, so from outside the check needs only
to see the message.
- **`conformance/operator/test-unmet-start-paths.ps1` does not run or report the case by
  default**, so a default run's results and its exit code are those of the other cases alone. It
  prints one line that is not a result, `Not run: the 15-minute deadline case; run it with -Case
  deadline.`, and runs it with `-Case deadline`, under a limit of its own of 17 minutes so it can
  never hang. **A wildcard that would select it, such as `d*`, neither runs nor reports it and prints
  the same line.** The case checks that the deadline falls from 14.5 to 16.5 minutes (990 seconds at
  most) after the start of the wait. (Changed by the architect the same evening: this first said the case is reported
  `SKIPPED` by default, which made every default run exit 2, "incomplete, never passed", above.)
- **`conformance/operator/test-unmet-sync-patch-wait.ps1` holds two more such cases, each waiting
  the real 30 minutes** (E0.08-T04-S01, #0182; D61, pin P13): the patch wait's deadline for
  `sync`, and the same for a provisioning run. No seam variable shortens the 30-minute bound, so
  the file shows its message and time only when asked, as `up`'s 15-minute deadline is.
  - **`-Case deadline` runs `sync`'s**, under a limit of its own of 34 minutes so it can never
    hang. **`-Case deadline-provision` runs the provisioning run's**, under a limit of 35 minutes.
    Each name MUST be given exactly, in any letter case, and each takes about 32 minutes.
  - **With any other `-Case` that would select either, the default `*` and a wildcard such as `d*`
    included, the file neither runs nor reports it**, and prints for each one `Not run: the
    30-minute deadline case; run it with -Case deadline.` or `Not run: the 30-minute provisioning
    deadline case; run it with -Case deadline-provision.` A default run's results and exit code are
    those of the other cases alone.
  - **Both of the file's on-request cases, `deadline` and `deadline-provision`, check that the deadline falls
    from 29 to 32 minutes after the start of the run**: the spec's 30 minutes run from the start of the wait, which begins seconds after the
    run's start, and the stand-ins' own time over several hundred checks adds to it. Each of the two
    cases uses a lean `aws` stand-in of its own, so that the stand-in's own time does not grow
    over 30 minutes into delays the timing would include.
  - **A runner that starts either case MUST go through a wrapper script** that writes the
    command's streams to files and its exit code and end time last (architect, 2026-11-22, for
    v0.08-T04; [architect.md, Live checks on Terry's laptop and the
    box](architect.md#live-checks-on-terrys-laptop-and-the-box)), since a shell tool moves a long
    command into the background and can lose its exit code.
- A release's whole conformance run (E0.06-T04-S01 for v0.06) runs it once with `-Case deadline`
  and records the result beside the rest.

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
| `test-lease-cmd-renew-release.ps1` | `renew` and `release`: the new certificate and its window, the record, the refusals by state and by value, a failed renew leaving the lease as it was (no key pair, no folder, a record that cannot be written), and what `release` deletes and leaves ([renew](lease-api.md#agentspace-lease-renew), [release](lease-api.md#agentspace-lease-release), [state.md](state.md#renewal)) | Yes. One SKIPPED result, and nothing else is run, when no CA could be made |
| `test-lease-certificate.ps1` | The certificate, the key and the times a ready `create` leaves ([lease-api.md](lease-api.md#the-credential), [Times](lease-api.md#times), [state.md](state.md#the-certificate)), read with `ssh-keygen` | Yes. SKIPPED when no CA could be made, and one result is SKIPPED when the machine's zone is UTC (see below) |
| `test-lease-alias-file.ps1` | The alias file, read with `ssh -G`, and the `Include` line on each shape of config; a ProxyCommand with a CR or LF refused; the default ProxyCommand line as D8 makes it checkable ([state.md](state.md#the-alias-file), [The `Include` line](state.md#the-include-line), [lease-api.md](lease-api.md#environment)) | Yes. SKIPPED when no CA could be made, and one result is SKIPPED when `create` fails under the default transport (see below) |
| `test-lease-driver-outcomes.ps1` | The driver as a caller sees it: unsatisfiable requests, no driver name in any output, a failed alias step ([driver.md](driver.md#unsatisfiable-requests), [lease-api.md](lease-api.md#concurrency-and-atomicity)) | Only for the ready half. The unsatisfiable half runs without one |
| `test-lease-concurrency.ps1` | Concurrent `create`s, `renew`s and `release`s, readers that never wait, the lease state lock and its 30 seconds ([lease-api.md](lease-api.md#concurrency-and-atomicity), [state.md](state.md#the-lease-state-lock)) | Yes. SKIPPED when no CA could be made |
| `test-id-whole-value.ps1` | The lease ID matched as a whole value, for `get`, `renew` and `release`: a trailing dot refused (`<id>.`, `<id>..`, `<id> .`, `<id>.` and a line feed) and changing nothing, a trailing line feed trimmed and the lease found, and `not_found`'s normalized `lease_id` ([Lease ID](lease-api.md#lease-id), [Input tolerance](lease-api.md#input-tolerance)) | For the cases that look at a lease. The refusals and `not_found` need none. SKIPPED for the rest when no CA could be made |
| `test-ca-public-key-words.ps1` | The CA check's two words compared exactly: a first word in another case, a second word with its letters' case swapped and a key type that is not ed25519, a line break (LF, CR, VT, FF or CRLF) between the two words, and a no-break space (U+00A0) before the first word each fail `create` and `renew`, and the same words with another comment, surrounding whitespace, or a tab or two spaces between the two words (each also for `renew`; #0136) are accepted ([state.md, Its checks](state.md#its-checks), check 4, architect, 2026-11-19, as corrected). 37 results | Yes. SKIPPED when no CA could be made |
| `test-relative-folders.ps1` | A relative `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` resolved against the working folder, and the alias file's full paths ([state.md, The home folder](state.md#the-home-folder)). A relative `USERPROFILE` is not given: the harness refuses it | Yes. SKIPPED when no CA could be made |
| `test-home-userprofile.ps1` | The lease CLI's files under `USERPROFILE` when the state and ssh folder variables are unset or empty ([state.md, The home folder](state.md#the-home-folder), [lease-api.md, Environment](lease-api.md#environment)) | Yes. SKIPPED when no CA could be made |
| `test-lease-ignores-instance-cache.ps1` | The lease CLI never reads or needs the instance-ID cache: `create`, `get`, `renew` and `release` succeed with a folder, a file that is not JSON and a valid cache at its place ([state.md, The alias file](state.md#the-alias-file)) | Yes. SKIPPED when no CA could be made, and one result is SKIPPED when `create` fails under the default transport with no cache at all with a valid `internal` error (see below); any other failure of that `create` is a FAIL |
| `test-plan-sizes.ps1` | The size `create --plan` picks (E0.10-T04-S01, #0221; D73 to D76): each size's two edges (2 and 4 agents at once `s`, 5 and 8 `m`, 9 and 16 `l`, 17 and 32 `xl`), the same with a platform no backend has, a tie between two busiest turns (the first wins), turns never summed and stories' peaks summed with one dev lead for the turn, the peak counted as the width of the `after` graph and never as a stated number, and a subagent's model and effort changing nothing ([lease-api.md, Plan](lease-api.md#plan), [`create`](lease-api.md#agentspace-lease-create)). 38 results | Yes. SKIPPED when no CA could be made |
| `test-plan-sized-for.ps1` | `sized_for` in every state a lease reaches (`ready`, `pending`, `unsatisfiable` by `no_capacity` and by `unsupported_platform`, `expired`, `released`) and through `create`, `get`, `list`, `renew` and `release`, absent on a lease made without `--plan`, left out and never `null` when the plan has no name or a null one, and no output printing the plan, a story, a subagent, a model or an effort ([lease-api.md, Fields](lease-api.md#fields); D75). 50 results | Yes. SKIPPED when no CA could be made |
| `test-plan-errors.ps1` | `plan_too_large` (32 agents at once fit and 33 do not, by stories, by subagents and by the width of `after`; the busiest turn and not the sum; its only reason, `size` `xl`, `sized_for` through `create`, `get`, `list` and `release`, decided before any driver is asked), `--plan` with `--size` in every order and form, the order the values are checked in (project, platform, profile, plan, size, ttl), and every refusal of a plan with its exact `Plan '<arg>' is not a valid plan: <why>.` and its first fault only: a file too large, missing, a folder, a device, a pipe, locked, `-`, not UTF-8, not JSON, a member named twice, off the schema, a name given twice, an `after` naming no subagent of its story, and a cycle ([lease-api.md, Plan](lease-api.md#plan), [Exit codes and errors](lease-api.md#exit-codes-and-errors); D74, D75, D81). 298 results | No. It removes the throwaway CA first, so that no case can pass because one was there, and has no SKIPPED result |
| `test-v1-compatible.ps1` | D76's promise: every document a command line with no `--plan` prints, in each state (`ready`, `pending`, `expired`, `released`, `unsatisfiable`), each v0 reason (`unsupported_platform`, `unsupported_profile`, `unsupported_size`, `no_capacity`), a list and an error of each code (`usage`, `invalid_value`, `out_of_range`, `not_found`, `invalid_state`, `internal`), validates against `schemas/v0/` as well as against v1's, in at least 80 documents; **from v0.11 v1's schemas are read from `schemas/v1/`**, since the top level of `schemas/` is v2's (E0.11-T04-S01, #0239); with 5 controls: four that pass v1's schema and fail v0's (a lease made with `--plan`, a `plan_too_large` lease, `--plan` with `--size` and a plan file that does not exist, the last two naming the field `plan`), and a fifth, an error that names the field `turn`, which only v2 has, that passes the top level's schema and fails `schemas/v1/`'s and `schemas/v0/`'s, so that a validator that accepts everything cannot pass and this file's v1 is v1's and never v2's ([lease-api.md, Versioning](lease-api.md#versioning), [Schemas](lease-api.md#schemas); D76, D80, D87). 95 results | For the `ready` cases. SKIPPED for them when no CA could be made |
| `test-board-plan.ps1` | `create --board <id> --epic <ticket>` against the stand-in board and a stand-in credential, never Terry's (E0.11-T04-S01, #0239; D83 to D89), by the seven items of [What the board cases MUST show](#what-the-board-cases-must-show): each refusal of the board read with its error code, its `field` and its message byte for byte (a board or an epic outside its form, an unknown board, an unknown ticket, an archived card, no block, two blocks, a malformed fence, a block never closed, a document `agentspace-plan` refuses, a turn the plan lacks, a board that answers 401, 403, 400, 405, 406, 418, 500 or 503, a bare 404, one that hangs, one that cannot be reached, a body past the 64 MiB bound, and each case of the receipt and of the credential); the requests the stand-in logged, one GET for a good epic; no method but `GET`, with a control that shows the check can see one; the token reaching the stand-in and appearing nowhere else, searched as text and as UTF-16LE on a planted copy first; the three members read and no other; a board lease and the same plan given through `--plan` giving the same size, `sized_for` and TTL; the usage cases of pin P4 refused before any request; and the deep-nesting cases of the architect's pin P5 (a plan nested 64 levels is check 6, one nested 100,000 levels is check 4, from a board and from `--plan`) ([lease-api.md, The board read](lease-api.md#the-board-read), [The plan block](lease-api.md#the-plan-block), [Exit codes and errors](lease-api.md#exit-codes-and-errors)). **245 results** at S01's sent head, `1ef1085`, from [its row in the run record](runs/e0.11-t04-s03.md#the-lease-cli-suite-at-the-final-main) | For its `ready` TTL result only: SKIPPED when no CA could be made. **Every case that needs a token is SKIPPED, and the run is incomplete, never passed, when the DPAPI preflight fails** ([The stand-in credential](#the-stand-in-credential)) |
| `test-plan-turn-ttl.ps1` | `create --turn` and the TTL a plan gives (E0.11-T04-S01, #0239; D83 to D87): the lease sized for the turn `--turn` names and for no other, with `sized_for.turn` and `sized_for.agents`, the busiest turn when none is named, a tie won by the first and the second when named, a turn name matched exactly, a turn no size holds being `plan_too_large` with its own message while another fits, and the whole plan checked; a `--turn` outside its form and a well-formed one that names no turn, each `invalid_value` for `turn`; each TTL rule (`--ttl` wins, else the lease's turn's budget and half again rounded up to the whole minute, else 8 hours), with the TTL of a `ready` lease read as `expires_at` minus `created_at` of its own document; `budget_minutes` that is not an integer from 1 to 960 written as digits alone refused with `invalid_value` for `plan`, and for `epic` from a board; every usage case that carries `field`, thirteen in all; and the same cases again with the plan read from an epic's block on the stand-in, one GET for a good epic ([lease-api.md, Turn](lease-api.md#turn), [TTL](lease-api.md#ttl), [Plan](lease-api.md#plan), [Command lines](lease-api.md#command-lines)). 381 results | For its `ready` TTL results: SKIPPED when no CA could be made. Its board half is SKIPPED when DPAPI does not work under the moved home |
| `test-v2-compatible.ps1` | D87's promise: every document a command line with none of `--board`, `--epic` and `--turn` and no `budget_minutes` prints, a lease in each of 5 states, each of 5 reasons (`plan_too_large` among them), a list and an error of each of 6 codes, validates against v1's schemas in `schemas/v1/` as well as against v2's, and one with no `--plan` against v0's in `schemas/v0/` too; the leases that only v2's inputs make (`--turn`, a plan with `budget_minutes`) validate against v1's schemas as well; controls so that a validator that accepts everything cannot pass (a plan lease and the errors naming the field `plan` pass v2's and v1's and fail v0's, and every error that names `board`, `epic` or `turn` passes v2's error schema and fails v1's and v0's); and a result that every state, reason and code was seen ([lease-api.md, Versioning](lease-api.md#versioning), [Schemas](lease-api.md#schemas); D76, D87). It starts no board. 151 results | For the `ready` cases. SKIPPED for them when no CA could be made |

**The first ten rows are the ten checks that this page and the run records mean when they say "the
ten".** (`test-interop.ps1` was an eleventh until it retired: [Interop retired](#interop-retired).) The five from `test-id-whole-value.ps1` on are
E0.06-T04-S03's (#0133, #0110 point 4), and four `test-unmet-` files beside them
([The files that are expected to fail](#the-files-that-are-expected-to-fail)) are its too.

**The helpers the checks share are three files.** `conformance/lease-cli/lease-cli-common.ps1` serves
S01's five, and `conformance/lease-cli/lease-check-support.ps1` serves the next five, which are
E0.04-T03-S03's (and, until it retired, `test-interop.ps1`, E0.05-T02-S05, #0103). `conformance/lease-cli/lease-check-hardening.ps1` serves E0.06-T04-S03's nine files (#0133): a run's error problems without the program's own text, the lease ID refusal, and the map of every file under the home that shows a failed command changed nothing. **From v0.11, `conformance/lease-cli/board-standin.ps1` is a program a case starts, never a helper a file loads**: it is the stand-in board of [The stand-in board and its credential](#the-stand-in-board-and-its-credential), it holds no case, and `test-board-plan.ps1` and `test-plan-turn-ttl.ps1` start it in a process of their own (E0.11-T04-S01, #0239).

**What v0.07 changes for a run of the lease CLI's checks** (D48, D54, D55; [driver.md, Conformance
cases for v0.07](driver.md#conformance-cases-for-v007)). From v0.07, a `create` or `get` can start a
provisioning run, through the program `AGENTSPACE_PROVISION_CMD` names
([lease-api.md, Environment](lease-api.md#environment)), and a throwaway home with no
`projects.json` answers `unsatisfiable` with `no_capacity` for every project but `swimcoach`.
- **No lease CLI case starts the real operator.** The harness gives the lease CLI a tripwire as
  `AGENTSPACE_PROVISION_CMD` ([The harness](#the-harness)), and a case that can reach `pending`
  MUST set its own stand-in through `-Environment`, one that records what it is given and makes no
  host. The real `agentspace-operator.exe` is never the program a lease CLI case's run starts, so a
  case never reaches AWS through a detached run, which the seam does not cover
  ([operator.md, The test seam](operator.md#the-test-seam)).
- **Each case seeds its home with the host registry and the project list it needs** (D54). A check
  that needs a `ready` host for a project calls the harness's `Add-ConformanceReadyHost` before its
  first `create` ([The harness](#the-harness)), and `New-CcReadyLease` does so for the checks that
  use it. The v0.07 files write their own `hosts.json`, `projects.json` and held locks, in the forms
  of [state.md](state.md#the-host-registry), each write through `Assert-ConformancePath`: a listed
  project for a case that expects `pending`, and nothing for a case that expects `no_capacity`. A
  case that places a `swimcoach` lease needs neither file.
- **A run needs no real `hosts.json` or `projects.json`, and MUST NOT name Terry's**: both are read
  from the throwaway home's `.agentspace` folder, under its `USERPROFILE`.
- **v0.07's lease CLI files are `test-unmet-*` files** in `conformance/lease-cli/`, listed with
  their cases in [The files that are expected to fail](#the-files-that-are-expected-to-fail).
  **`conformance/lease-cli/provision-standin.ps1` is the helper the `pending` and `no_capacity`
  files share**, dot-sourced after the other helpers: it writes each case's stand-in run, a `.cmd`
  under the home that records what it is given and makes no host, and reads back what it recorded.
  It holds no case. `test-unmet-host-alias-bytes.ps1` and `test-unmet-pending-get-list-release.ps1`
  write their own stand-ins, and dot-source only the harness and `lease-check-support.ps1`.

**What v0.08 changes for a run of the lease CLI's checks** (D59, D63; E0.08-T04-S02, #0183). A
`release` now ends the laptop's ssh clients of the lease's alias, so one file,
`conformance/lease-cli/test-unmet-release-ends-connections.ps1`, runs `release` with stand-in
clients of its own running ([driver.md, Conformance cases for
v0.08](driver.md#conformance-cases-for-v008)); [The files that are expected to
fail](#the-files-that-are-expected-to-fail) lists its cases. **It holds every row, R1 to R15, as
a case of its own** (architect, 2026-11-22, for v0.08-T04: every row maps to a result line by
name), and each result name starts with the driver.md anchor the row cites and then the row's
number, as `driver.md#each-with-its-tree: R1 ...`. It makes its stand-ins from one small console
program built with the .NET Framework's `csc.exe` in the throwaway home, since the harness has no
builder. **It looks for `csc.exe` only under `%SystemRoot%\Microsoft.NET\Framework64` and
`\Framework`, never on `PATH` or in the checkout, and reports `SKIPPED` when neither holds one**
(architect, 2026-11-22, for v0.08-T04). A stand-in is never a copy of an installed program or of
Windows' own `ssh.exe`. The file names no gap: every row has its result lines.
- **R7 is the one case that starts the program under test outside `harness.ps1`**, because the
  row requires the release to be a stand-in's child ([The harness](#the-harness)).
- **R15 holds only for a second client started within the release's run** (architect, 2026-11-22,
  for v0.08-T04; driver.md's R15 row at c932ef6). A second client that the stand-in starts after the
  release returned makes R15's two results that depend on it, the client ended and the count of at
  least 2, `SKIPPED` with that reason, never `FAIL`. **A `FAIL` for a second client found still
  running says in its reason that it MAY be timing under load**, and it is run once more alone
  before it is a finding (#0189's family).
- **A case that starts a stand-in client MUST end it at the case's close**, so a failed case leaves
  none running, and **MUST NOT copy, open or start an installed program, or Windows' own `ssh.exe`,
  as one** (AGENTS.md, "Terry's real files in a review").
- **A case MUST NOT list, print or log any process it did not start**, and **MUST make the leases
  whose aliases a stand-in carries with `create`, or seed an ID drawn at random for that run**, so
  that no process of Terry's can match one.
- **The case that counts a reconnecting client assumes no timing** (driver.md, R15): it asserts a
  count of at least 2, never which of the release's walks ended the second client, nor how many
  walks the release made (D59, as changed for v0.10-T03).

**What v0.09 changes for a run of the lease CLI's checks** (D65 to D68, D71; [driver.md, Conformance
cases for v0.09](driver.md#conformance-cases-for-v009)). A lease's `--size` of `s`, `m`, `l` or `xl`
is now supported, and place reads the `ready` host's `instance_type` from `hosts.json` and the
project's live `ready` and `pending` leases from the lease state file
([driver.md, On a `ready` host](driver.md#on-a-ready-host)). So a supported `create` for a project
whose host is `ready` can now answer `pending` and start a run, or `unsatisfiable` with
`no_capacity`, where before v0.09 it answered `ready`.
- **A size no backend has is `xxl`**, and `XXL` where a case needs upper case (D71). E0.09-T01-S03
  (#0199, PR #133) moved every lease CLI case that used `l` or `xl` as an unsupported size to it, in
  `test-create-get.ps1`, `test-lease-cmd-renew-release.ps1`, `test-lease-concurrency.ps1`,
  `test-lease-driver-outcomes.ps1`, `test-list.ps1`, `test-unmet-argument-line-feed.ps1`,
  `test-unmet-failed-create-cleanup.ps1` and `test-unmet-no-capacity.ps1`, so each passes before
  and after v0.09's build.
- **A `ready` record a case seeds carries `instance_type` `m9g.xlarge`, `m`'s type, unless the case
  asks for none** (pin P3). E0.09-T03-S01 (#0203, PR #140) made `Add-ConformanceReadyHost` write it
  on every record it adds, and on the box's record when it makes the file, with `-NoInstanceType`
  for a case about a record with no type ([The harness](#the-harness)); and gave
  `New-PgHostRecord` in `test-unmet-host-alias-bytes.ps1`, which seeds C3's second case and C4, the
  same default (pin P29). **A seeded record with no type would answer `pending` and start a run**
  ([driver.md, On a `ready` host](driver.md#on-a-ready-host), row 2), so every earlier case that
  expects `ready` for a default `create` depends on it.
- **C20 changed by the rule, not only by its seeding** (driver.md, "Cases of earlier releases that
  change with v0.09"): row 1 answers a `create` `pending` while the project has a `pending` lease,
  so E0.09-T03-S01 rewrote C20 in `test-unmet-host-alias-bytes.ps1` to Z14's form, `pending`, with
  the run started only when one is due.
- **A case whose `create` or `get` can start a run MUST name v0.07's recording stand-in as
  `AGENTSPACE_PROVISION_CMD`** (pin P12): from v0.09 that is every supported `create` on a `ready`
  record that rows 1, 2 or 6 answer `pending`, the box's included. The harness's tripwire still
  fails any case that does not ([The harness](#the-harness)).
- **Each v0.09 case writes its own `hosts.json`, with the `instance_type` it names or none, its own
  `projects.json`, its own held `provision.lock`, and its own lease records into `leases.json`**,
  each through `Assert-ConformancePath`, as C26 writes its records. A live `ready` lease is made
  with `create` or written as a stored `ready` record whose `expires_at` is after the case's run;
  a `pending` one is written as a stored record (driver.md's bullets above its table).
- **v0.09's lease CLI files are two `test-unmet-*` files**, `test-unmet-sizes-place.ps1` and
  `test-unmet-sizes-pending.ps1`, which dot-source `provision-standin.ps1` for their stand-in run;
  [The files that are expected to fail](#the-files-that-are-expected-to-fail) lists their cases.
- **`test-help.ps1` needed no change for the create help's two v0.09 lines** (lease-api.md, pin
  P28): it compares each help block with the one lease-api.md gives, read at run time.

**What v0.10 changes for a run of the lease CLI's checks** (D73 to D77, D80, D81;
[lease-api.md, Changes from v0](lease-api.md#changes-from-v0); [driver.md, Conformance cases for
v0.10](driver.md#conformance-cases-for-v010); E0.10-T04-S01, #0221). `create` takes `--plan <file>`,
picks the size from it, prints `sized_for`, and answers `plan_too_large` for a plan no size holds
(contract v1), and a run is due at once after a run that succeeded (D77).
- **A case that makes a plan writes the plan file inside the throwaway home**, through
  `Assert-ConformancePath`. **`test-plan-errors.ps1` also runs the program with the plans' folder as its
  working folder**, so that the `--plan` argument is a bare file name and no message holds a path of the
  machine's, and masks a failure reason before it prints it
  ([The program under test is untrusted](#the-program-under-test-is-untrusted)).
- **A plan lease is shown by the document the program prints.** No lease CLI case reads a plan lease's
  record in the lease state file ([The gaps](#the-gaps) says what shows it instead). Every plan case
  reads the printed document, and validates it against v1's schemas with `Test-ConformanceSchema`, which
  cannot tell a missing `sized_for` from a lease that has none, so **each result reads the member
  itself**. **A case that needs a v0 schema names it by its path under `schemas/`**:
  `Test-ConformanceSchema -Schema 'v0/error.output.schema.json'`, and the harness is used as it is
  (`test-v1-compatible.ps1`). **No v0.10 case changed `harness.ps1` or a helper**, and T04's stories MUST
  NOT, since the operator, keeper and schemas files load them.
- **A plan answered `plan_too_large` needs no CA, no host and no run**: it is decided before any driver is
  asked (D81), so `test-plan-errors.ps1` removes the throwaway CA first. **A plan a size holds, made
  for a project with no host that is not on the project list, is answered `unsatisfiable` with
  `no_capacity` before any host is made** (D54), which is how a case reads `sized_for` and `size` with no
  run. **A case that reaches a host, `ready` or `pending`, seeds its own `hosts.json`**, a record ready at
  the size's `instance_type` as v0.09's cases do, and names v0.07's recording stand-in as
  `AGENTSPACE_PROVISION_CMD` (pin P12).
- **The footprint is not in the contract** (D76): today's 32 agents at once for `xl` is a constant of
  `test-plan-errors.ps1` (`$script:PeMost`), and `test-plan-sizes.ps1` takes its edges from lease-api.md's
  size table at today's footprint. A new figure changes those two files and no contract version.
- **D77's file seeds `last_success_at` itself**, in the record it writes, in state.md's timestamp form,
  because the lease CLI's stand-in provision writes nothing to `hosts.json` and so never records a success
  (driver.md, "Conformance cases for v0.10"). `test-unmet-pending-due-after-success.ps1` writes its own
  `hosts.json`, `projects.json`, lease state file and held `provision.lock`, each checked against its
  schema before the case runs and each write through `Assert-ConformancePath`. **A time is computed once
  from the check's own clock**, so that two times meant equal are one string, and a time 5 minutes old is
  shown by a seed 6 minutes before now and never by waiting. "Starts nothing" is shown by no start after a
  wait of 3 seconds, as `provision-standin.ps1` states.
- **A program built before v0.10 refuses a `hosts.json` that holds `last_success_at`** as not valid, by
  its schema (D77's last bullet; [state.md, The host registry](state.md#the-host-registry)). So **the
  installed v0.09 programs fail the D77 file's cases that seed the member and expect a start or an
  answer**, 8 of its 19 results by PR #159's table, never every result: a `get` that seeds one and
  expects no start passes there, and so do the cases that seed none. A v0.10 build passes every result
  ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). A lease state file
  that holds a plan lease's `sized_for` and `plan` is refused by a program built before v0.10 in the same
  way ([state.md, What a plan lease adds](state.md#what-a-plan-lease-adds)).
- **No case shows driver.md's row 6 together with a seeded `last_success_at`**: a known gap, ruled so by
  the architect for v0.10-T04 on E0.10-T04-S01's reviewer's N1. Rows 1, 2 and 6 share step 2's spacing,
  and #0216's crate tests (`agentspace-driver-ec2`, `tests/due_after_success.rs`) cover step 2 directly.
  Every `create` of Y1, Y2, Y4 and Y5 is made on the whole seed, the stored `pending` lease included, so
  it reaches row 1; none reaches row 6 ([driver.md, On a `ready` host](driver.md#on-a-ready-host)).
- **v0.10's lease CLI files are five**: `test-plan-sizes.ps1`, `test-plan-sized-for.ps1`,
  `test-plan-errors.ps1` and `test-v1-compatible.ps1`, in the table above, and
  `test-unmet-pending-due-after-success.ps1`, a `test-unmet-*` file. **All five dot-source
  `provision-standin.ps1`** for their stand-in run; [The files that are expected to
  fail](#the-files-that-are-expected-to-fail) lists their cases. **Comments only changed in
  `test-usage-errors.ps1` and `test-values.ps1`**: the first's header says six `field` cases and where the
  sixth is, and the second's the order of the values with `plan`.
- **`test-help.ps1` shows v0.10's `create` help** by reading lease-api.md at run time, as for v0.09's two
  lines. Its eight `create` cases fail against the installed v0.09 programs and pass against `main`'s
  build ([handoff-v0.10.md](handoff-v0.10.md), "Red on `main` by design"); the v0.10 install turns them
  green.

**What v0.11 changes for a run of the lease CLI's checks** (D83 to D89;
[lease-api.md, Changes from v1](lease-api.md#changes-from-v1); E0.11-T01-S03, #0233). A pin cited
"pin Pn" in this section and in [The stand-in board and its
credential](#the-stand-in-board-and-its-credential) is v0.11-T01's, as the architect ruled it
(architect, 2026-11-25, for v0.11-T01). `create`
takes `--board <id> --epic <ticket>` and `--turn <name>` (contract v2): it reads the epic's plan from
a board, and only reads. **This section says what the suite needs for that, and T04's cases
(#0239 to #0241) build it.**
- **A board case reads a stand-in board with a stand-in credential, never Terry's** (D86, D88). The
  stand-in board is `conformance/lease-cli/board-standin.ps1`, a loopback server the case starts and
  stops, and the credential is a receipt and a `bot.json` the case writes into its throwaway home
  ([The stand-in board and its credential](#the-stand-in-board-and-its-credential)). **No new
  seam variable names either**: the lease CLI finds the receipt under `LOCALAPPDATA`, which the
  harness already points into the home ([The harness](#the-harness)).
- **A board case needs no CA, no host and no run when its lease is `unsatisfiable`** (D54, D88): a
  project that is not on the project list is answered `no_capacity` before any host is made. **A case
  that reaches a host seeds its own `hosts.json` and names v0.07's recording stand-in as
  `AGENTSPACE_PROVISION_CMD`, as v0.09's and v0.10's cases do**, by the rule above for a case whose
  `create` or `get` can start a run. A case about a refusal of
  the board read makes no lease and needs neither.
- **A board case writes its plan files, receipt, credential and fixture inside the throwaway home**,
  through `Assert-ConformancePath`, as v0.10's plan cases do, and **MUST NOT change `harness.ps1` or a
  helper that the operator, keeper and schemas files load**: T04's stories add a helper file of their
  own, as `provision-standin.ps1` is.
- **A plan the lease CLI read from a board is shown by the printed document**, as a plan from a file
  is: validated against v2's schemas, with each result reading the member itself. No board case reads
  `plan_source` from the lease state file, since D87 never prints it; the schemas check shows its
  form from the samples ([state.md, What a plan lease adds](state.md#what-a-plan-lease-adds)).
- **The ProxyCommand's refusal of a lease alias whose lease is not `ready` (#0229) is one operator
  file, `conformance/operator/test-unmet-proxy-not-ready.ps1`** (D89), written by T02 beside the
  ProxyCommand's own change and run by T04's operator lane. It follows
  [operator.md, From the ProxyCommand](operator.md#from-the-proxycommand) and [A lease that is not
  ready (#0229)](operator.md#a-lease-that-is-not-ready-0229), and its rows are operator.md's
  [Which checks show it](operator.md#which-checks-show-it), v0.11's table. It runs by the same
  harness as v0.07's ProxyCommand cases: a throwaway home, a seeded host registry, a seeded lease
  state file, the recording stand-in `aws` that logs every call, and tripwires for
  `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD` ([The harness](#the-harness)). **It MUST show each
  of these:**
  - **The alias of a lease that is `released`, `expired`, `unsatisfiable` or `pending`, refused**,
    each of a project whose host record is `ready`: exit 1, the one line on stderr that operator.md
    gives, nothing on stdout, **and no tunnel, no call the stand-in `aws` records, no host started,
    no ssh or scp, and no push.** These four states are the only ones refused for a lease the file
    names; **a `ready` lease whose project's host is not `ready`, or has no record, reaches the box
    as before v0.11 and is not refused.**
  - **An argument of lease alias form refused while the lease state file exists and cannot be read,
    and while it is not valid** (architect, 2026-11-25, for v0.11-T01, pin P21), in the same way,
    with operator.md's line saying the lease records could not be read; **and an argument that is
    not of lease alias form reaching the box in both, with the `AGENTSPACE_DIAGNOSTIC` line as
    before v0.11.** **A file the case holds open without sharing is retried on that sharing
    violation alone, at most 3 attempts within 1 second in all** (architect, 2026-11-25, for
    v0.11-T01, pin P23): a hold past that bound refuses the argument of lease alias form as above,
    and a hold that ends inside it lets the read through, so a `ready` lease's alias connects. A
    folder at the path, and a file that is not valid, are refused with no retry. **The cases run
    with `AGENTSPACE_DIAGNOSTIC` set and count the retry lines** that operator.md gives, `agentspace:
    The lease records were held by another program; trying again.` (architect, 2026-11-25, for
    v0.11-T01, pin P27): zero with a refusal for the folder, two with a refusal for a hold past
    the bound, and one or two with a connection to the `ready` lease's host for a hold released
    inside it. **No case swaps, replaces or releases a file on a timer**; the hold is released by
    the case on the first retry line reaching the ProxyCommand's stderr. Every such run is
    bounded in time, by operator.md's [Which checks show it](operator.md#which-checks-show-it)
    rows.
  - **The alias of a `ready` lease whose host is `ready` still connecting**, as the control that
    shows the refusal is the lease's state and not a broken stand-in.
  - **An alias that is no record's alias at all, and a missing lease state file, reaching the box**,
    as before v0.11 ([operator.md, From the ProxyCommand](operator.md#from-the-proxycommand), step 3).
  - **A release followed by a ProxyCommand started for its alias opening no tunnel**: a `ready`
    lease released by the build's `agentspace lease release`, then the ProxyCommand given its alias,
    refused as `released`, in that order and never at once, so no timing is in the case.
  - **Nothing written by a refusal**: the lease state file and the host registry byte for byte as
    seeded, no lock taken, and no file in the home that was not there.
  - **The refusal naming no backend** ([operator.md, What a caller
    sees](operator.md#what-a-caller-sees-d1-card-0087)), and no file path.
- **v0.07's `test-unmet-host-proxy-lookup.ps1` holds cases that v0.11 changes**, which pass against
  a build before v0.11 and fail against one that refuses, and which #0237 changes with the
  ProxyCommand (architect, 2026-11-25, for v0.11-T01, pin P21): the lease alias whose lease has
  expired, which reaches its host there and is refused from v0.11; the lease state file that is not
  JSON, has `version` 1, holds two records with one `id`, or holds a record that does not fit its
  schema, and a folder at its path, each given a lease-form alias that reaches the box there and is
  refused from v0.11; and the `AGENTSPACE_DIAGNOSTIC` case, whose connection reaches the box there
  and is refused from v0.11 after the line naming the file.
- **v0.11's lease CLI files are three new ones, one changed, and one program** (E0.11-T04-S01, #0239):
  `test-board-plan.ps1`, `test-plan-turn-ttl.ps1` and `test-v2-compatible.ps1`, and `test-v1-compatible.ps1`
  changed to read v1's schemas from `schemas/v1/`, all in the table above, and the stand-in board
  `board-standin.ps1`, which is no check. **S01 changed no other file**: `harness.ps1`, every helper and every earlier
  file are as v0.10 left them, which the operator, keeper and schemas files rely on (the run record shows
  the diff). [The files that are expected to
  fail](#the-files-that-are-expected-to-fail) lists the cases.
- **The suite at v0.11 is 94 conformance files**, counted by a command over each folder's `test-*.ps1`: 46
  lease CLI files (43 at v0.10 and S01's three), 39 operator files (38 at v0.10 and #0229's
  `test-unmet-proxy-not-ready.ps1`), 7 keeper files and 2 schemas files. **A whole-suite record MUST count
  the files of the commit it runs by such a command**, and never copy the count from this page.
- **v0.11's operator files** are `test-unmet-proxy-not-ready.ps1` (E0.11-T02-S04, #0237), the cases of
  `test-unmet-host-proxy-lookup.ps1` that #0237 changed, and S02's change to the helper
  `host-provision-common.ps1` and to `test-unmet-host-provision.ps1` (E0.11-T04-S02, #0240), which masks the
  log line a provision case quotes ([The operator and the token push](#the-operator-and-the-token-push)).

- **Each check prints one line per result and one summary line.** A result line is `PASS <name>`,
  `FAIL <name>: <reason>` or `SKIPPED <name>: <reason>`, and a name starts with the spec section it
  enforces, as in `lease-api.md#agentspace-lease-get: ...`. The summary is `<n> passed, <n> failed,
  <n> skipped`.
- **The exit code is 0 when every result was PASS, 1 when any was FAIL, and 2 when none failed but
  any was SKIPPED.** **A run that counted no result at all MUST exit 1**, with a `FAIL` line saying
  that no check ran. A run that exits 2 is incomplete, never passed (see
  [What a conformance run needs](#what-a-conformance-run-needs)). The checks skip only for these
  reasons, and each reason says which:
  - **The harness could not make a throwaway CA** (any check that places a ready lease).
  - **No `ssh-keygen` is on the `PATH`** (two checks of `test-create-get.ps1`).
  - **The local UTC offset is 0** (one check of `test-create-get.ps1` and one of
    `test-lease-certificate.ps1`).
  - **A `create` under the default transport fails with a valid `internal` error** (one check of
    `test-lease-alias-file.ps1`). That check leaves `AGENTSPACE_PROXY_COMMAND` unset, so the program
    forms its own line, and seeds no instance-ID cache, which a conforming build never needs (D21).
    Its default line can be checked only when that `create` succeeds. The MUST that it succeed
    with no cache is `test-unmet-default-transport-no-cache.ps1`'s
    ([The files that are expected to fail](#the-files-that-are-expected-to-fail)), and the
    prototype misses it in a checkout that holds no `.instance-id` of its own, such as a worktree,
    where this run exits 2. **v0.05's build did not fail that `create` in the run of E0.05-T04-S01, so the
    result was a PASS against it** ([the run record](runs/e0.05-t04-s01.md)). What the failed `create` leaves (no lease
    folder, no alias file, no record) is checked as PASS or FAIL either way, and any other failure of
    that `create` is a FAIL, not a SKIPPED ([The gaps](#the-gaps)).
  - **A `create` under the default transport fails with a valid `internal` error with no instance-ID
    cache at all** (one result of `test-lease-ignores-instance-cache.ps1`, E0.06-T04-S03). It is the
    same failure as the line above it, and `test-unmet-default-transport-no-cache.ps1`'s: without it
    the check cannot tell a cache a build reads from a build that cannot form its default line, so it
    reports that one result SKIPPED and runs every other. **Any other failure of that `create` is a
    FAIL**, not a SKIPPED.
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
  incomplete, never passed. **Who makes the run, and how, is
  [What the specs require of the suite](#what-the-specs-require-of-the-suite)'s**: a machine whose
  zone is not UTC, run by the agent or person who runs the suite for the build.
- **A check took from about 20 seconds to about 2 minutes against the prototype, and the ten about
  10 minutes** (the Rust build's runs are in [the run records](runs/e0.06-t04-s01.md)), most of it starting programs: about
  0.5 seconds for a run that issues no credential, and a few seconds for a ready `create`. `test-lease-concurrency.ps1` is the longest, because
  round F waits out the lock's 30 seconds. Against a program that prints nothing, all ten take
  about a minute.

### Running the suite against a build's output folder

**RFC 2119 keywords again.** This is how v0.05's Rust build is run (E0.05-T04-S01, #0107, D38), and
how any later build that ships in a `target\dist` folder is. [The run record](runs/e0.05-t04-s01.md)
is the first, and shows the results. **v0.06's run, of the whole suite against the Rust operator,
ProxyCommand and keeper as well, is [docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md).**
**v0.07's is [docs/runs/e0.07-t04-s04.md](runs/e0.07-t04-s04.md), v0.08's is
[docs/runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md)** (E0.08-T04-S04, #0185), **v0.09's is
[docs/runs/e0.09-t04-s03.md](runs/e0.09-t04-s03.md)** (E0.09-T04-S03, #0207), **v0.10's is
[docs/runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md)** (E0.10-T04-S03, #0223), **and v0.11's, the
latest whole-suite run, is [docs/runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md)** (E0.11-T04-S03,
#0241). Their counts are in them, and this page quotes none of v0.08's, v0.09's or v0.10's, and of
v0.11's only the box's 172 ([The idle check's test](#the-idle-checks-test)).
1. **Build the output folder from the workspace's root, with `cargo xtask dist`**
   ([operator.md, The install folder](operator.md#the-install-folder)). It writes `target\dist\`,
   and the lease CLI is `target\dist\agentspace.exe`. **The run MUST be made from a build of a
   commit, and the run record MUST name that commit**, so that a pass belongs to code that can be
   fetched again.
2. **Name the build's programs, or install them.** With no `AGENTSPACE_*` conformance variable set,
   every program is the install folder's ([Naming the program under test](#naming-the-program-under-test)).
   To run a build that is not installed, set each variable that build's program answers for to its
   full path in `target\dist\`, in PowerShell 7, such as `AGENTSPACE_CMD` to
   `target\dist\agentspace.exe`, and leave the others unset. (v0.05's run, before the Rust operator
   existed, set only `AGENTSPACE_CMD`, and the operator that made each throwaway CA was then the
   prototype's.)
3. **Run every file, each as `pwsh -NoProfile -File <file>`:** every
   `conformance\lease-cli\test-*.ps1`, so the ten checks and every `test-unmet-*` file, and
   `conformance\schemas\test-schemas.ps1`, which names no program. **From v0.06 on, a build that
   ships the operator, the ProxyCommand and the keeper runs every `conformance\operator\test-*.ps1`
   and `conformance\keeper\test-*.ps1` too**, with `-Case deadline` run once more
   ([Cases that run only when asked for](#cases-that-run-only-when-asked-for)), and **from v0.07,
   every file in `conformance\schemas\`**, `test-state-schemas.ps1` among them.
   - **v0.07's runs need no `AGENTSPACE_PROVISION_CMD` from the runner, and MUST NOT name the real
     operator in it.** The harness removes the caller's value and sets its tripwire, and each case
     that can reach `pending` names its own stand-in ([The harness](#the-harness)), so no lease CLI
     case starts the real operator, from the build's output folder or the install folder.
   - **No real `hosts.json` or `projects.json` is needed**: each case seeds its own throwaway home
     ([Running the suite](#running-the-suite)).
   - **v0.08's runs name a stand-in guard for every keeper file** (`AGENTSPACE_GUARD_CMD`, with
     tripwires for `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD`;
     [The harness](#the-harness)), and run the idle check's test beside the suite, in a Linux
     environment only ([The idle check's test](#the-idle-checks-test)). The run record is
     [docs/runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md).
   - **v0.09's runs need nothing more from the runner**: each v0.09 case seeds its own
     `hosts.json`, with the `instance_type` it names, and its own lease state file, and names its
     own stand-in run or `aws` ([Running the suite](#running-the-suite), "What v0.09 changes"), so
     no case reads a real registry or reaches a real stack. The run record is
     [docs/runs/e0.09-t04-s03.md](runs/e0.09-t04-s03.md).
   - **v0.10's runs need nothing more from the runner either**: each plan case writes its plan file in
     its throwaway home, and the D77 and provision cases seed their own `hosts.json` and lease state
     file ([Running the suite](#running-the-suite), "What v0.10 changes"). **Beside the files, a v0.10
     whole run runs the crates** (`cargo test --workspace --locked --no-fail-fast`, `cargo fmt --all
     --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`) **and the box's tests
     in WSL, with the whole discovery command, so that every `test_agentspace_*.py` file runs and not
     one** ([The idle check's test](#the-idle-checks-test)). **A run MAY start early on a diff-checked
     base when no merge after it can change its inputs** (the architect's pin P55, as in v0.09), and the
     record MUST show the diff between the base and the final `main`. The run record is
     [docs/runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md).
   - **v0.11's runs need nothing more from the runner either**: each board case starts its own stand-in
     board on a port it picks and writes its own receipt and `bot.json` into its throwaway home ([The
     stand-in board and its credential](#the-stand-in-board-and-its-credential)), so no run reads Terry's
     board, receipt or token, and none connects to port 8811. **The whole-suite commands are these, and a
     record MUST take them from the last record's counts and never from a passage about one file** (D89;
     E0.11-T04-S03, #0241): every `conformance\lease-cli\test-*.ps1` (46 files at v0.11), every
     `conformance\operator\test-*.ps1` (39), every `conformance\keeper\test-*.ps1` (7) and every
     `conformance\schemas\test-*.ps1` (2), which are **94 files**, with `test-unmet-start-paths.ps1 -Case
     deadline` once more, so **95 file runs**; the crates (`cargo test --workspace --locked --no-fail-fast`,
     `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`); and
     the box's tests in WSL by the whole discovery command, which prints `Ran 172 tests`. **The
     empty-program run is of the 46 lease CLI files, and each MUST exit 1.** **A run MAY start early on a
     diff-checked base** (the architect's pin P55, as in v0.09 and v0.10), and **the record MUST show the
     diff between the base and the final `main` over every input the early runs read**, with the one
     exception that a document no early run reads, such as lease-api.md, MAY differ and MUST be named. The run
     record is [docs/runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md).
4. **A complete run exits 0 on every file, with no SKIPPED result, on a machine whose zone is not
   UTC.** Each `test-unmet-*` file is a requirement the build MUST meet, so against a build its
   `FAIL` is the build's failure ([The files that are expected to fail](#the-files-that-are-expected-to-fail)).
5. **The empty-program run MUST be made too**: `AGENTSPACE_CMD` naming a `.cmd` of `@exit /b 0`,
   for every lease CLI file. Each MUST exit 1.

### Interop retired

**`conformance/lease-cli/test-interop.ps1` and `AGENTSPACE_PEER_CMD` were removed by E0.06-T06-S01
(#0135), 2026-11-20 (architect, D42).** The check drove one lease through two lease CLIs, each way
round, to enforce [state.md, Between the prototype and the rewrite](state.md#between-the-prototype-and-the-rewrite):
that the prototype and the rewrite MUST be able to share one home folder and one state folder. With
the prototype gone from the repository there is no peer to run it against, so the check has nothing
to show, and its variable went with it.
- **Its last run was on v0.06's build, in [runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md): 36 passed,
  0 failed, 0 skipped, each way round.**
- **The requirement it enforced stands in state.md as history**, and a later release that has two
  builds to share state between MAY write a check for it again.

### The harness

**`conformance/lease-cli/harness.ps1` is dot-sourced by every lease CLI check, and it is the only
code that starts the program under test.** A check makes each home it needs with
`New-ConformanceHome`, removes it in a `finally`, and ends with `Exit-ConformanceRun`. The helpers
the checks share, which start no program of their own, are in `conformance/lease-cli/lease-cli-common.ps1`.

**One case is the named exception to that rule: R7 of
`conformance/lease-cli/test-unmet-release-ends-connections.ps1`** (E0.08-T04-S02, #0183;
[driver.md, Conformance cases for v0.08](driver.md#conformance-cases-for-v008); architect,
2026-11-22, for v0.08-T04). R7's row requires the `release` to be the child of a stand-in
`ssh.exe`, so the stand-in starts it, and the harness cannot. **R7 MUST keep the harness's
guarantees by hand:**
- **The stand-in is given the harness's environment for the throwaway home**
  (`Get-ConformanceChildEnvironment`, with the provisioning-run tripwire added), so the release it
  starts reaches nothing outside the home.
- **The check's wait for the release is bounded at 60 seconds**, and the stand-in's child is ended
  by its recorded PID, checked first against the creation time recorded, as every process a stand-in
  recorded is.
- **No other case starts the program under test outside the harness.** A new one MUST NOT, until
  the architect names it here.

**A throwaway home MUST NOT be inside a git working tree** (D37): `ca` refuses a CA folder with a
`.git` entry in it or in any folder above it ([docs/operator.md, `ca`](operator.md#ca)), so a home
under such a folder cannot get its CA. `%TEMP%` is outside one on the laptop. The harness does not
check this: a home that was inside one would fail at the CA step, with the operator's refusal.

| Function | Does |
|---|---|
| `New-ConformanceHome` | Makes a folder under `%TEMP%` named `agentspace-conformance-<12 hex>`, holding a fresh `USERPROFILE` (`home`, with `.agentspace` and `.ssh`) and a throwaway CA made by the operator's `ca` under it. Returns `Root`, `UserProfile`, `StateDir` (`.agentspace\state` in the home), `SshDir` (`.ssh` in the home), `CaReady` and `CaReason`. When no CA can be made, `CaReady` is false and `CaReason` says why, and the real CA is never used instead. **Every folder it makes is checked at the call**: the `Root` through `Assert-ConformanceHomeRoot`, and each folder under it through `Assert-ConformancePath`, once the `Root` exists. Should the making fail, it deletes the `Root` through `Assert-ConformanceHomeRoot` too (#0110 point 2) |
| `Invoke-AgentspaceCli -ConformanceHome <h> -Arguments <string[]> [-InputJson <string>] [-Environment <hashtable>]` | Runs the program `AGENTSPACE_CMD` names as a separate process, inside the home. Returns `ExitCode`, `Stdout`, `Stderr`, `Json` (stdout parsed with every time kept as a string, or null), `ErrorJson` (stderr parsed, or null), `StdoutBytes`, `StderrBytes` and `TimedOut`. `-Environment` adds or removes variables for that one run, and a null value removes one. **It refuses an override of `USERPROFILE`, `HOME`, `LOCALAPPDATA` or `APPDATA`, and an override of `AGENTSPACE_STATE_DIR` or `AGENTSPACE_SSH_DIR` to a path that is not under the home's `Root`.** A refusal throws, and the check reports it as FAIL. `-InputJson` is written to stdin |
| `Invoke-ConformanceProgram -Variable <name> ...` | The same for any program a conformance variable names, such as `AGENTSPACE_OPERATOR_CMD` |
| `Test-ConformanceSchema -Json <string> -Schema <file>` | True when the document validates against a schema under `schemas/` (`state/leases.state.schema.json` is allowed), by PowerShell 7's `Test-Json -SchemaFile`. False when it does not validate or is not JSON |
| `Write-ConformanceResult -Name <check> -Status PASS\|FAIL\|SKIPPED [-Reason <text>]` | Prints one result line and counts it |
| `Assert-ConformancePath -ConformanceHome <h> -Path <p>` | Resolves the path and returns it in full, or throws unless it is the home's `Root` or under it (an empty path, a path with `..` that leads out, another drive). **It also throws when the `Root` itself, or any folder or file that exists between the `Root` and the path, is a reparse point (a junction or a symbolic link)**, because a write or a delete through one could reach outside the home. It checks the attributes of each existing step, from the `Root` down. **Every delete and every write a lease CLI or operator check makes MUST call it at the call**, as the path argument of that very call, with no program run between the check and the write ([The program under test is untrusted](#the-program-under-test-is-untrusted)). The path it returns is in full and long form, so it can differ in spelling from the text a program wrote (a short `8.3` folder name in `%TEMP%` is expanded): a check that compares a path with such text uses the path as it built it, after the call |
| `Assert-ConformanceHomeRoot -Root <p>` | Returns the full path of a home's root, or throws unless it is a folder named `agentspace-conformance-<something>` directly under `%TEMP%` and, when it exists, is not a reparse point. **It is the one write check that is not `Assert-ConformancePath`**: that one measures a path against a home's `Root`, and this one is what makes the `Root` safe to measure against. It guards `New-ConformanceHome`'s creation of the `Root`, and the deletes of `Remove-ConformanceHome` and of `New-ConformanceHome`'s own cleanup. A check that writes inside a home MUST call `Assert-ConformancePath`, and MUST NOT call this (E0.06-T04-S01, #0110 point 2) |
| `Remove-ConformanceHome -ConformanceHome <h>` | Deletes the home's `Root`, and only it. It refuses a folder that is not an `agentspace-conformance-*` folder directly under `%TEMP%`, and a `Root` that is a junction or a symbolic link, and it checks again at each delete (`Assert-ConformanceHomeRoot`) |
| `Exit-ConformanceRun` | Prints the summary line and exits with the code above. A run that counted no result exits 1, after a `FAIL` line saying that no check ran |
| `Add-ConformanceReadyHost -ConformanceHome <h> -Project <string[]> [-NoInstanceType]` | Seeds the home's host registry, `%USERPROFILE%\.agentspace\hosts.json` ([state.md, The host registry](state.md#the-host-registry)), so that a `create` for each project answers `ready` as it did before D54 (a home with no `projects.json` answers `unsatisfiable` with `no_capacity` for every supported project but `swimcoach`). It adds `{"project","stack":"agentspace-host-<project>","state":"ready","instance_type":"m9g.xlarge"}` with no `instance_id`, after the box's record, and when it makes the file the box's record carries the same `instance_type`; `-NoInstanceType` leaves the member out of both, for a case about a record with no type, and a record read back is kept as it is (E0.09-T03-S01, #0203, pin P3: a `ready` record with no type answers `pending` from v0.09, [driver.md, On a `ready` host](driver.md#on-a-ready-host), row 2). It writes the file as compact JSON, UTF-8 with no byte order mark and one LF, checked against `schemas/state/hosts.state.schema.json` and written through `Assert-ConformancePath`. It skips `swimcoach` (the box: `hosts.json` stays absent for a case about v0.06's bytes), keeps a project's existing record as it is, never touches `projects.json`, and throws for a name that is not a normalized project or for an existing file that is not valid. It returns the names it added. `New-CcReadyLease` (`lease-check-support.ps1`) calls it, and its `-NoHost` skips that (E0.07-T04-S01, #0155) |
| `Clear-ConformanceHosts -ConformanceHome <h>` | Deletes the home's `hosts.json` (never `hosts.lock`), so that the registry reads as the box's record alone again |
| `Get-ConformanceTripwireStarts -ConformanceHome <h>` | The number of times the provisioning-run tripwire was started in the home |

**The harness has a provisioning-run tripwire from v0.07** (E0.07-T04-S01, #0155; D48, D54, pin
P12): every home holds `<Root>\tripwire\provision-tripwire.cmd`, which the lease CLI is given as
`AGENTSPACE_PROVISION_CMD` unless a check names its own stand-in, and `Remove-ConformanceHome`
reports FAIL when it was started. **A later lease CLI case MUST either name its own stand-in or
reach no provisioning run**: the bullets below give the rules.

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
      `aws sts get-caller-identity` fails for every AWS CLI the operator could run** (the fixed
      path of [operator.md](operator.md#programs-it-starts) and the `aws` on `PATH`), and report
      every case SKIPPED without running the program if one succeeds.
    - **No check needs a real AWS CLI (B-50, E0.04-T05-S05).** When no AWS CLI exists on the
      machine there is nothing to call, so the confirmation passes and the checks run, and **a
      machine with no AWS CLI is a valid place to run the whole suite.** The environment above
      carries no AWS credential, and **every operator and ProxyCommand check also names the three
      programs the operator starts as stand-ins** (`AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and
      `AGENTSPACE_SCP_CMD`, [operator.md](operator.md#the-test-seam)): a tripwire that fails at
      once, or, in a check that drives `up` or the tunnel, a stand-in that answers as the real
      program would. So a build that meets the seam never reaches a real AWS CLI, `ssh` or `scp`,
      whether or not one exists.
  - **`AGENTSPACE_PROXY_COMMAND` is set to a command that fails at once** (the command
    interpreter's `/d /c exit 1`, found through `ComSpec`), so that a ready lease
    needs no real box ([lease-api.md](lease-api.md#environment)). A check MAY override it with
    `-Environment`.
  - **The lease CLI, and only it, gets `AGENTSPACE_PROVISION_CMD` set to a tripwire in the home**
    (E0.07-T04-S01, #0155; D48, pin P12): `<Root>\tripwire\provision-tripwire.cmd`, which appends
    one line to `tripwire-started.txt` beside itself and exits 1 at once. A run's default program is
    `agentspace-operator.exe` beside the CLI, the real operator in `target\dist`
    ([driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run)), so a lease
    CLI case MUST NOT be able to start it. A check MAY replace the value with `-Environment` (its
    own stand-in; an empty value or `$null` removes it), and a check that reaches a run on purpose
    MUST name its own stand-in ([lease-api.md, Environment](lease-api.md#environment)). The
    operator, the ProxyCommand and the keeper do not get it.
  - **`Remove-ConformanceHome` reports FAIL when the tripwire was started**, because a check then
    reached a provisioning run it had named no stand-in for.
  - **A check that needs a `ready` host for a project calls `Add-ConformanceReadyHost` before its
    first `create`, and before any file map it compares.** A check whose `get` would start a run
    for a seeded project (a `pending` lease of a project with a `ready` host, D48) MUST name a
    stand-in, or clear the registry first with `Clear-ConformanceHosts`.
  - **From v0.08, every check that runs a keeper MUST set `AGENTSPACE_GUARD_CMD` to a stand-in in
    its home, and SHOULD set `AGENTSPACE_AWS_CMD`, `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD` to
    tripwires as well** (architect, 2026-11-21, on v0.08-T01's pin P15; D56, D63). A keeper built
    by E0.08-T03-S03 starts a guard at its first watching pass, and with the variable unset that
    guard is the build's own `agentspace-operator.exe`. A throwaway home with no `hosts.json` still
    names the box, and the home's lease file has no live lease, so a guard that reached a real AWS
    CLI could stop Terry's real box. The keeper's crate tests MUST do the same before the start is
    built. This binds `conformance/keeper/`, the architect's runs of it, and any dev lead's run of
    `test-keeper.ps1` against a build.
  - **Every other `AGENTSPACE_*` variable the caller holds is removed**, so that a shell's own
    settings never reach the program.
  - **`PSModulePath` is removed.** A PowerShell 7 session's value carries PowerShell 7's own module
    folders, and Windows PowerShell 5.1, which ran the prototype's operator, then could not load its
    security module, so its `ca` failed. The Rust operator needs none of this, and the harness still
    removes the variable, so that no program inherits the session's.
- **A check MUST NOT read, write, list or hash the real `%USERPROFILE%\.agentspace`, the real
  `~/.ssh` or the real CA**, and the harness never gives one a path to them. Only a person or a
  verifier proving a run touched nothing lists them, before and after.
- **A check MUST NOT load prototype code** (D34). It reaches the program only through the harness.
- **A check MUST NOT run a program with `USERPROFILE` unset or empty** (architect, 2026-11-19, for
  E0.06-T04-S02), and the harness refuses that override. A conforming program stops there, but one
  that does not may fall back to Windows' own profile lookup and read Terry's real CA key, stack
  parameters or cache before any stand-in is reached, and isolation is what keeps his files safe.
  So the rules for an unset or empty `USERPROFILE` (operator.md, Command line; keeper.md, Streams
  and environment) are shown by each program's own tests, never from outside. `SystemRoot`'s are
  checked from outside, with `USERPROFILE` in the throwaway home.

### The program under test is untrusted

**The suite runs against builds other than the prototype, and a build under test can be wrong, so its output is data and never a path to act on.** The architect's rule, 2026-11-18:
- **A check MUST NOT trust the program's output to build a path.** A lease ID or a host alias read
  from a document MUST NOT be used to name a file until the document has passed its schema and
  the UUID and alias patterns (`lease.output.schema.json`), and then only through
  `Assert-ConformancePath`. In `lease-check-support.ps1`, `New-CcReadyLease` returns a document only
  when the run passed the whole success test, and `Get-CcLeaseDir` and `Get-CcAliasPath`, the only
  functions that build a lease's paths, refuse an ID or an alias that does not match and check the
  path.
- **Every delete and every write a lease CLI or operator check makes MUST call
  `Assert-ConformancePath` at the call**: `Remove-Item`, `Set-Content`, `Add-Content`, `Out-File`,
  `Export-Clixml`, `[IO.File]::Write*`, `New-Item`, `Copy-Item`, `Move-Item`, `Rename-Item`, and a
  `[IO.FileStream]` opened to create or hold a file. **At the call means as the path argument of that
  very call**, as in `Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $p) ...`,
  so that no statement and no program run lies between the check and the write (E0.06-T04-S01, #0110
  point 1). A path asserted when it is assigned, and used a few lines later, is stale once a program has
  run in between, which can swap a folder for a junction: `test-unmet-release-released.ps1` asserted its
  paths, ran the program, and then copied to them. A call that moves or deletes a file through a path
  MUST check that path too, and a `Move-Item` MUST check both ends. A path that is outside the throwaway home
  makes the check throw, and the check reports FAIL. **No new function was needed**: the call is
  `Assert-ConformancePath` itself, used inline.
- **Two things are written without `Assert-ConformancePath`, and each is named** (#0110 point 2):
  - **The home's own root.** `New-ConformanceHome` creates it, and `Remove-ConformanceHome` and
    `New-ConformanceHome`'s cleanup delete it, through `Assert-ConformanceHomeRoot` at the call (a
    folder named `agentspace-conformance-*` directly under `%TEMP%`, and not a junction or a symbolic
    link). `Assert-ConformancePath` cannot guard its own reference, so this one check does. **The
    keeper suite's run folder is a root of the same kind** (`agentspace-conformance-keeper-<32 hex>`
    directly under `%TEMP%`): `Initialize-KeeperRun` creates it, and the keeper checks delete it, through
    `Assert-ConformanceHomeRoot` at the call, and every write under it goes through
    `Assert-ConformancePath` against a home object whose `Root` is that folder (`$runHome`).
  - **A stand-in's own writes.** The `.cmd` stand-ins of `New-OperatorStandIn` and the ssh stand-in
    script of `test-unmet-push.ps1` log and record beside themselves, under the home's `standins`
    folder. They are programs outside agentspace that the check places, and a check does not make
    those writes.
- **Shown by an audit of the source, not of a run** (E0.06-T04-S01): every line of every file in
  `conformance/lease-cli/`, `conformance/operator/` and `conformance/keeper/`, as of this branch,
  that names a write verb (the calls above, and `Tee-Object`, `Clear-Content`, `Set-Acl`,
  `[IO.File]::Open`, `[IO.FileStream]`, `Delete`, `Replace`, `CopyTo`, `icacls`, `Start-Process
  -RedirectStandardOutput` and `-RedirectStandardError`, and a redirection to a file) carries
  `Assert-ConformancePath` or `Assert-ConformanceHomeRoot`, except the lines of these classes, each
  of which is not a write the check makes:
  - **The text of a stand-in** (the `.cmd` files and scripts the checks place, and their `>>` and
    `AppendAllText` lines), which writes beside itself when the stand-in runs.
  - **A read.** A stream or file opened for reading only (`FileAccess` `Read`), such as the keeper
    suite's reads of the log and the lock.
  - **A process variable.** `Remove-Item Env:\<name>` removes a variable of the check's own process,
    and no file.
  - **The harness's pipes.** `RedirectStandardInput`, `RedirectStandardOutput`, `RedirectStandardError`
    and `CopyToAsync` in `Invoke-ConformanceProgram` carry the program's output into memory.
  - **A length set on a stream the check just opened through the check** (`SetLength` in the keeper
    suite's rotation case), and **the working folder** (`[Environment]::CurrentDirectory` in
    `test-relative-folders.ps1`), after which the check itself makes no relative write.

  The search was first run against lines known to write, and found them (151 lines that carry an
  assert were found by the same pattern), so that a zero would not be believed blind. **E0.06-T04-S01
  extended it to the files that E0.06-T04-S02 and E0.06-T04-S03 added**, and to the keeper suite,
  which the first pass had not covered. The sites it changed, so that each write is the path argument
  of its own check and nothing runs between them: in `conformance/operator/`, `test-unmet-commands.ps1`
  (the ssh stand-in script's write) and `test-unmet-start-paths.ps1` (the stand-in folder's creation);
  in `conformance/lease-cli/`, `test-ca-public-key-words.ps1`, `test-relative-folders.ps1` and
  `test-lease-ignores-instance-cache.ps1`; in `conformance/keeper/`, every write and delete of
  `keeper-common.ps1` (the run folder's creation and copy, the case folders, `leases.json`, the
  record, the launch redirects of the keeper's output and error files, the fail stub), the lock,
  log and record sites of `test-keeper.ps1` and `test-unmet-holder-dies-mid-pass.ps1`, the run
  folder's removal in all four keeper files, and the checks that stood before the write in
  `test-holder-environment.ps1` and `test-unmet-launch-request-full-paths.ps1`. **#0110 points 1 and
  2 are done.**
- **A time the program printed is untrusted too.** A check MUST NOT wait for one, without a limit.
- **One path from the program's output is read outside the home: the default ProxyCommand's
  program** ([state.md, The alias file](state.md#the-alias-file)), which D8 puts beside the CLI's
  own executable, not in the home, so `Assert-ConformancePath` would refuse it. A check MAY only
  test that it is an existing file, and MUST first refuse it unless it is a full path on a local
  drive (`X:\...`) with no `.` or `..` step, no `\\?\` or other device prefix, no share, and no
  wildcard. It MUST NOT write, delete, run or list anything there, and MUST NOT quote the path in a
  failure reason. `Get-CcDefaultProxyProblems` in `lease-check-support.ps1` does this, and is the
  only code that does.
- **Shown by a run:** a program that prints a ready lease whose `id` and `host_alias` are
  `..\..\..\..\<folder>` and `..\..\..\..\<folder>\id_ed25519`, where the folder is a sibling of
  the home under `%TEMP%`, makes every check FAIL or refuse, and leaves the file in that folder
  untouched.

### The stand-in board and its credential

**RFC 2119 keywords again: these bind every board case** (E0.11-T01-S03, #0233; D86, D88, D89;
architect, 2026-11-25, for v0.11-T01, pins P8 and P9 with P2 and P17 to P19). **A case of `create --board <id> --epic <ticket>` reads
a stand-in board with a stand-in credential, and never Terry's board, receipt or token.** The
stand-in is a program outside agentspace, so it follows [What a conformance run
needs](#what-a-conformance-run-needs)'s rule for stubs: **it stands for localswim, which is the
caller's board and not a backend**, and it stands for nothing of agentspace's. T04 writes it
(`conformance/lease-cli/board-standin.ps1`, D89); **this section is its interface, and T04's writer
works from it and from [lease-api.md, The board read](lease-api.md#the-board-read) alone.**

#### The stand-in board

**One process serves one board's fixture on one loopback port, for one case.**
- **How it is started:** `pwsh -NoProfile -File conformance/lease-cli/board-standin.ps1 -Port <n>
  -Fixture <path> -Log <path> -Ready <path>`. The case starts it, with the harness's environment for
  the throwaway home (`Get-ConformanceChildEnvironment`, [The harness](#the-harness)), and every one
  of the three paths is a file inside the home, passed as the result of `Assert-ConformancePath` at
  the call that starts it. **The script reads the fixture and writes the log and the ready file, and
  nothing else, and it refuses a missing or unreadable fixture with exit 2 before it listens.** The
  case names the ready file with `-Ready`, since the script is not told the home (the dev lead's
  call on pin P8, which stands with the rulings of P17 to P19).
- **It listens on `127.0.0.1` and the port the case picks, and on no other address.** It MUST NOT
  bind `0.0.0.0`, `::` or any other address, so that no other machine can reach it, and it MUST NOT
  need elevation. **The case picks the port by binding port 0 on `127.0.0.1`, reading the port and
  closing the socket, and MUST NOT use a fixed port**, and above all not localswim's own, 8811
  (AGENTS.md, "The board"). **The stand-in MUST refuse to start when `-Port` is 8811**, with exit 2
  before it binds anything and without writing the ready file (pin P8). When the stand-in cannot
  bind the port it exits 2 without writing the ready file, and the case picks another port and
  starts it again, at most 3 times, and then FAILs.
- **The case knows it is listening from the ready file** (pin P8): **the stand-in MUST write the
  file only after its listener is bound** and accepts connections, holding its own PID in decimal
  and one LF, UTF-8 with no byte order mark. **The case waits for the file in a loop with a deadline
  of 30 seconds, never a fixed sleep, and FAILs when the PID is not the one it started**, or when
  the process exits first.
- **It is ended by the case, by its name and its PID, and nothing else is touched** (AGENTS.md; as
  every process a stand-in recorded is, [The harness](#the-harness)): in the case's `finally`, after
  checking that the process's creation time is the one recorded at its start. **It has no stop
  request**, since a request would be logged, and **it ends itself after 10 minutes** when no case
  has, so that a failed case leaves none running.
- **The fixture is one UTF-8 JSON file, and every member is defined:**

  | Member | Holds |
  |---|---|
  | `token` | A string, the stand-in token in the form the real reader checks (43 characters of `A-Z`, `a-z`, `0-9`, `-` and `_`), **drawn at random by the case for each run and never written in a file of the repository**, so that no value in a document or a sample can be mistaken for a real one. It sits in the fixture in the case's home and nowhere else. The stand-in compares it with each request's `Authorization: Bearer <token>`, and **MUST NOT log, print or answer it, and the case MUST NOT print it, a failing case's reason included** (pin P8) |
  | `boards` | An object. Each name is a board's ID, a version 4 UUID in lowercase (`--board` takes a UUIDv4 alone, pin P18), patterned to read as fake, such as `0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f`, and each value is `{"cards": {...}}`. An empty object is a stand-in that knows no board |
  | `boards.<id>.cards` | An object. Each name is a ticket in decimal digits with no leading zero, 1 to 9007199254740991, and each value is `{"revision": <integer 1 to 9007199254740991>, "archived": <boolean>, "description": <string>}`, each required |
  | `answer` | Optional, `null` when absent. `{"status": <integer 400 to 599>}` answers every request of any kind with that status and a localswim Problem document; `{"status": <integer 400 to 599>, "problem": false}` answers every request with that status and an empty body with no `Content-Type`, which is how a case gets a bare 404; `"hang"` reads each request and never answers it; `{"body_bytes": <n>, "chunked": <boolean>}` answers a ticket view of a known card with a body of exactly `n` bytes (the dev lead's call, which stands with the rulings of P17 to P19). **An `answer` of `{"status": ...}` or `"hang"` overrides every other reply**, so it covers the unknown-board and unknown-ticket answers too. **An `answer` of `{"body_bytes": <n>, "chunked": <boolean>}` applies to the ticket view of a known card alone** (the dev lead's call, 2026-11-25, for v0.11-T01): every other request, an unknown board or ticket, a `?view=summary` request and a request the `Authorization` check refuses among them, gets the reply it would get with `answer` `null` |

  **A fixture with another member, a missing required member, a value of another type or a board ID
  that is not a lowercase version 4 UUID is refused with exit 2**, so that a case's typo cannot make
  the stand-in answer something nobody meant.
- **Every path it serves carries localswim's API prefix, `/api/2026-11-10/`** (pin P17), as the
  lease CLI's requests do; a request for a path without it is logged and answered with a bare 404,
  an empty body with no `Content-Type`, so that a client that dropped the prefix reads `internal`
  and never an unknown board or ticket.
- **The ticket view** is `GET /api/2026-11-10/boards/<board>?view=ticket&ticket=<n>&include_prose=true`
  with `Authorization: Bearer <token>`, the request [lease-api.md](lease-api.md#the-board-read) gives. The
  stand-in answers 200 with `application/json` and **a whole `TicketInspection`**
  (localswim's `docs/openapi.yaml`, `readBoard`; its members
  `board_id`, `board_revision`, `card`, `relationships`, `parent`, `children` and `prose`, none
  missing), **with every part the client has no use for filled in**: `prose.comments` holds at least
  two comments, `relationships` at least one link, `parent` a card and `children` at least one card.
  The case shows by those that the client reads `card.revision`, `card.archived` and
  `prose.description` and ignores the rest (pin P1). **Each of those parts holds a marker string
  that the case looks for in the program's output and finds nowhere**, since a client that echoed a
  comment into an error would leak a card's prose. The stand-in:
  - **Takes the ticket as an integer, with leading zeros accepted**, as localswim does, and takes
    the board's ID in any case, **and logs the request's path and query exactly as received**, so a
    case shows what the client sent (the board's ID in lowercase, the ticket with no leading zero);
  - **Answers a board it does not know, and a ticket the board has no card of, 404** with
    `application/problem+json` and a Problem document (`type` `about:blank`, `status` 404, `code`
    `not_found`, a `request_id` and the other members localswim requires), **the same document for
    both**, as localswim's does ([lease-api.md, The board read](lease-api.md#the-board-read)); a
    client that tells the two apart does so by a second request. **A 404 counts as not found only
    with that body** (pin P17): **a case MUST show that a bare 404**, from a `{"status": 404,
    "problem": false}` fixture, **gives `internal`**, and neither `invalid_value` for `board` nor for
    `epic`;
  - **Answers `GET /api/2026-11-10/boards/<board>?view=summary` 200 for a board it knows**, with a
    JSON object holding the board's `id` and nothing the client may read, and 404 with the Problem
    document above for one it does not: pin P2's second request, made only after a 404 for the
    ticket view, so a refused read costs at most two GETs;
  - **Checks a request in this order, and answers the first check it fails** (the dev lead's call,
    2026-11-25, for v0.11-T01, ruled by the architect under pin P27): the method (405 for any but
    `GET`), then the `/api/2026-11-10/` prefix (a bare 404, with no Problem document), then
    `Authorization` (401), then the lookup of the board and the card (404 with the Problem
    document). So a `POST` to a path with no prefix is 405, and a `GET` with no prefix and no
    token is a bare 404;
  - **Answers a request with no `Authorization` header, or any value but `Bearer <token>`, 401** with
    a Problem document, and logs `auth=none` or `auth=wrong`. **It MUST check the header before it
    looks up the board or the card** (the dev lead's call, 2026-11-25, for v0.11-T01), as localswim
    does: a missing or wrong token answers 401 for an unknown board or ticket too, never 404, so a
    client that sent no token is never told whether the board exists;
  - **Answers every method but `GET` 405** with a Problem document (`method_not_allowed`) **and logs
    it**: `HEAD`, `OPTIONS`, `POST`, `PUT`, `PATCH`, `DELETE` and any other. **The case FAILs on any
    log line whose method is not `GET`** (D86: conformance MUST show that the board client sends
    nothing but GETs). Bot's token can write to a real board, and this is the only proof that the
    client's code has no path that does;
  - **Never follows, makes or accepts a redirect**, and answers no `Location` header;
  - **Answers a `{"status": 403}` and a `{"status": 401}` fixture** with the Problem document for
    that status, a `{"status": 500}` with a Problem document of `internal_error`, and any other
    status in the same way, so the case reaches each of pin P2's statuses (401 and 403 as
    "refused the credential", 400, 405, 406 and 5xx as "did not answer").
- **`"hang"` and the body past the bound:** a `"hang"` stand-in accepts the connection, reads the
  request, logs it and holds the connection open without a byte of answer until the case ends the
  process, so a case shows pin P2's 30-second timeout: the program's run MUST end within 60 seconds
  of its start, exit 1 with an `internal` error that names the host and port and nothing else, and
  **the case runs it with `Invoke-ConformanceProgram -Variable 'AGENTSPACE_CMD'` and a
  `-TimeoutSeconds` of 90**, since `Invoke-AgentspaceCli` takes no timeout and `harness.ps1` is not
  changed for it (above), so that a client with no
  timeout fails by `TimedOut` and is not mistaken for a slow pass. A `{"body_bytes": <n>, "chunked":
  <boolean>}` stand-in answers the ticket view of a card of its fixture with the card's whole
  document padded, by `prose.comments` entries each under the 4,194,304 characters localswim allows
  a comment, to exactly `n` bytes (a smaller `n` than the unpadded document is refused with exit 2),
  and sends it with a `Content-Length` of `n` when `chunked` is `false`, and in chunks with none when
  it is `true`. The cases of the bound are `n` = 67,108,864 and 67,108,865, each both ways (pin P1:
  the client reads at most 67,108,864 bytes, **counted as received**, so a declared length and a
  chunked body both show it), the first read as a good epic and the second refused as pin P1 says.
- **The log is one line per request, UTF-8, LF, written to the `-Log` path and flushed per line**
  (pin P8): `<METHOD> <path and query as received> auth=<ok|wrong|none>`, such as `GET
  /api/2026-11-10/boards/<board>?view=ticket&ticket=52&include_prose=true auth=ok`. **The token is
  compared and never logged or printed**, and neither is a header, a body or an address, **so that a
  log read by a case or printed in a failure reason holds nothing secret**. **A request the stand-in could not parse is
  logged as `INVALID - auth=none`**, and a connection that sent nothing is not logged. The case reads
  the log only after the program under test has exited, and **counts lines, never times**.
- **It is never a copy or a part of an installed program, of localswim, or of Windows' own HTTP
  server's configuration**: it is a script in `conformance/lease-cli/` that holds no request of the
  real board's and makes none. **It MUST NOT make an outbound connection of any kind.**
- **The stand-in as built, where the interface above is silent** (the dev lead, for v0.11-T04, pin P9;
  E0.11-T04-S01, #0239). **RFC 2119 keywords again.** `conformance/lease-cli/board-standin.ps1` makes each of
  these calls, none of which contradicts the interface, and a case MUST NOT depend on a call this list does
  not give:
  - **Its refusals are exit 2, before it listens and without a ready file**, and `-Port` is read first, so
    that 8811 is refused before the fixture is read, the log opened or anything bound. The others are an
    argument beyond the four, a `-Port` that is not a decimal from 1 to 65535, a missing `-Fixture`, `-Log` or
    `-Ready`, a fixture that is missing, unreadable or breaks the fixture's table, a log it cannot open and a
    port it cannot bind. **An `answer` of `"problem": true` is refused with exit 2 too**, since the fixture's
    table defines only `false`. **A reason goes to stderr and names a member or an argument, never a value or a
    path.** (the dev lead, for v0.11-T04, pin P9)
  - **It binds a plain TCP socket to `127.0.0.1` with exclusive use of the address**, never a `TcpListener`:
    on .NET 10 a `TcpListener` given `ExclusiveAddressUse` before it starts answers a null `Server` after
    it, which `Socket.Select` cannot take, and the stand-in's own self-test found it (23 of its first run's 46
    results failed). (`board-standin.ps1` as built, E0.11-T04-S01, #0239)
  - **It serves its connections from one loop, one request at a time**: each request is logged and flushed,
    then answered or held, before the next is read. Every answer carries `Connection: close`, and a
    connection is closed once the client has closed its side, or 5 seconds after its answer. A connection
    that sent nothing for 30 seconds is closed and not logged, and a request whose bytes stop for 10 seconds
    is read as far as it goes. **It ends itself, exit 0, 10 minutes after it starts**; **its self-test does not
    show that end, which is in the code alone.** (the dev lead, for v0.11-T04, pin P9)
  - **A request it cannot parse is answered 400**, a Problem document of code `invalid_request`, and logged
    `INVALID - auth=none`, **unless an `answer` overrides it**: an `answer` of `{"status": ...}` or `"hang"`
    overrides every reply, an unparsable request included. (the dev lead, for v0.11-T04, pin P9)
  - **A query the ticket view cannot use is answered 400 `invalid_request` too**, and it is read once the
    board is found, since an unknown board is a 404 first: **a missing or unknown `view`, a `ticket` that is
    not decimal digits, is zero or is past 9007199254740991, an `include_prose` that is neither `true` nor
    `false`, and a query name given twice.** **`view` and `include_prose` are read without regard to case**, as
    localswim's `docs/openapi.yaml` reads them. **A path under the prefix that is not `boards/<id>` is a 404
    with the Problem document of code `not_found`.** (the dev lead, for v0.11-T04, pin P9)
  - **A `405` carries `Allow: GET`, and a `HEAD` gets its answer's headers and no body.** **Each Problem
    document it makes, the 404's included, has a fresh `request_id` for each request.** (the dev lead, for v0.11-T04, pin P9)
  - **It opens the log in append mode, so that the file is created empty when it is absent**, and **it
    overwrites the ready file** when one is there. (the dev lead, for v0.11-T04, pin P9)
  - **A method or a request target that holds the token's text, raw or percent-decoded, is logged as
    `[token-withheld]` in its place**, so that a client that put the token in a URL is caught by the line and
    the token is still never logged. The `Authorization` value is compared while the request is parsed and kept
    in no variable past that; only `ok`, `wrong` or `none` is. (`board-standin.ps1` as built, E0.11-T04-S01, #0239)
  - **Every string of the ticket view that a client has no use for begins `STANDIN-MARKER-`**: the card's
    title, the lane, owner and priority labels of every card, each comment's text, and the title of each
    relationship, the parent and each child. **Each marker ends with a tag of the board ID's first 8
    characters and the ticket**, and the numbers of comments (2 to 4), relationships (1 or 2) and children (1
    or 2) follow the ticket, so that two cards of the same revision, archived flag and description on two
    boards, or under two tickets, differ in every member the client ignores. **`prose` is present only for
    `include_prose=true`**, as localswim's is, **except that a `{"body_bytes": <n>, "chunked": <boolean>}`
    answer's padded ticket view always includes it**, and `prose.description` is the fixture's description
    exactly as its JSON text writes it. (the dev lead, for v0.11-T04, pin P9)
  - **A `{"body_bytes": <n>, "chunked": <boolean>}` answer pads the document**, streamed, by lengthening the
    last comment's text when the gap is smaller than one more comment, and otherwise by adding comments of
    `x` text, each under 4,000,000 characters; an `n` smaller than any card's unpadded document is refused
    with exit 2 at the start. (`board-standin.ps1` as built, E0.11-T04-S01, #0239)
- **A stand-in's own host caches under the home are not a change to the home** (the architect, 2026-11-26,
  for v0.11-T04, pin P13). **The stand-in is a `pwsh` process started under the case's moved home, and
  PowerShell writes startup caches under `home\AppData\Local\Microsoft\PowerShell\` of the home's root when it
  first runs, at a time of its own.** A check that a refused run, or any run, left the home as it was MUST NOT
  count a file or folder under that path as a change, and **MUST exclude exactly that path and no other**: a
  check MUST NOT exclude by a file name, a pattern or a wider folder, since the same check MUST still see
  every other file the program under test wrote.

#### The stand-in credential

**The credential is what the lease CLI reads for the one request, as localswim's receipt and
`bot.json` ([operator.md, The receipt](operator.md#the-receipt), [The Bot
credential](operator.md#the-bot-credential); localswim's `docs/windows-operations.md`), and the case
writes both into its throwaway home**, each through `Assert-ConformancePath`, as the shapes
`agentspace-push`'s reader parses (`receipt_path`, `parse_receipt`, `credential_path` and
`read_token`):
- **The receipt is `<home>\AppData\Local\localswim-v2\service.json`**, where the lease CLI looks for
  it, because **the harness gives every program it starts `LOCALAPPDATA` set to `home\AppData\Local`**
  ([The harness](#the-harness), "Every program the harness starts gets the same environment"). It is
  one UTF-8 JSON object, with no byte order mark, with `base` `http://127.0.0.1:<port>` (the stand-in's
  port, with no trailing slash) and `credential_directory` the full path of a folder inside the home,
  `<home>\AppData\Local\localswim-v2\credentials`. **A case MAY add a member the reader ignores**
  (localswim's receipt holds others), to show that it is ignored.
- **`bot.json` is `<credential_directory>\bot.json`**, one UTF-8 JSON object with exactly the four
  members `format` (`localswim-credential`), `format_version` (the integer `1`), `role` (`bot`) and
  `token_dpapi_hex`: **the fixture's token, protected with DPAPI in user scope with no entropy, for the
  Windows user the case runs as**, as lowercase hex. The case makes the blob in its own process, with
  `ConvertTo-SecureString -AsPlainText -Force` on the token and `ConvertFrom-SecureString`, which
  writes the hex of a user-scope blob whose plaintext is the token in UTF-16LE, **the form the real
  reader decrypts and checks**. **The token is read from the fixture's object in the case's memory
  and is never a command argument** (AGENTS.md, "The board"), and a case's failure reason names a
  member or a count, never the token's value or a decrypted byte.
- **A preflight shows that DPAPI works under the moved home, before the first board case runs.**
  Windows may find a user's DPAPI key through the profile, and the harness moves `USERPROFILE` and
  `APPDATA` for every child. The preflight protects a random string in the case's process and
  decrypts it in a `pwsh` child started with the harness's environment, and prints nothing of either.
  **When it fails, every board case that needs a token is reported SKIPPED with the reason, and a run
  with any skip is incomplete, never passed** ([Running the suite](#running-the-suite); the dev
  lead's call, which stands with the rulings of P17 to P19); the cases of a missing or unreadable
  credential do not need one and still run.
- **The cases of the receipt itself, each answered `internal` by the lease CLI, in agentspace's own
  words** (D86, #0041), **with the stand-in listening throughout and its log empty after the run**,
  which shows that **no request was made, and so the token reached nobody**:
  - **No receipt**, and **a folder at the receipt's path**, and an empty file, and one that is not
    JSON, and one holding `credential_directory` twice, or none.
  - **A `base` that is not `http://127.0.0.1:<port>`**, the one form `agentspace-push`'s
    `parse_receipt` accepts (pin P19), each refused before any request as `internal`, its message
    naming no URL: `http://localhost:<port>` (P19 withdrew `localhost` from D89), `https` in place
    of `http`, a host that is a name (`board.invalid`, a name that never resolves, so a client that
    connected would reach nobody), `127.0.0.2`, `[::1]`, a user name before the host
    (`user@127.0.0.1`), a trailing slash or a path, a port of 0, of 65536, with a leading zero or
    with a letter, and a `base` that is not a string. **No case uses a routable address**, and no
    name but `localhost`, which resolves only to the laptop itself, so that a build that wrongly
    connected would still send the token nowhere but the laptop. **The `localhost` case's port is
    the stand-in's own**, so that a build that wrongly accepted it would reach the stand-in, log a
    line, and FAIL on the log that MUST be empty.
  - **A receipt with no `base`**, which `agentspace-push` allows and D86's read cannot use.
  - **`credential_directory` naming a folder with no `bot.json`**, a `bot.json` that is a folder, an
    empty file, not JSON, with a fifth member, with `role` `human`, with `format_version` `1.0` or
    `"1"`, with `token_dpapi_hex` holding odd or non-hex digits, or a blob that DPAPI refuses (a
    DPAPI header followed by zero bytes, as localswim's own examples are), or one that decrypts to a
    string that is not 43 base64url characters.
  - **Each of these is answered with the same `internal` error shape, naming no path, no URL beyond
    a host and port, and no token** ([lease-api.md, The board read](lease-api.md#the-board-read)), and
    leaves no lease and no file changed in the home.
- **The credential is the receipt's alone**: a case sets `LOCALSWIM_URL` to another loopback port,
  where nothing listens, and `LOCALSWIM_TOKEN` to another value, and the read still reaches the
  stand-in with the credential's token (`auth=ok`), since no other variable changes the contract
  ([lease-api.md, Environment](lease-api.md#environment)). **Every other board case removes the
  caller's `LOCALSWIM_*` variables** with `-Environment`, each name set to `$null`, found by listing
  the case's own process's names that start `LOCALSWIM_`, because the harness removes only
  `AGENTSPACE_*` ones, and a build that wrongly read one would reach Terry's real board.

#### What a board case never reads

**A case MUST NOT read, write, list, hash or connect to anything of Terry's board** (D86, D88;
AGENTS.md, "Terry's real files in a review": "isolation is enough"):
- **It MUST NOT read his receipt, his credential or `%LOCALAPPDATA%\localswim-v2`**, by any path, and
  MUST NOT start `localswim.exe` or the board's service, read a `LOCALSWIM_*` variable's value, or
  run his board watcher. **It builds every path it reads or writes from the throwaway home's own
  object** (`New-ConformanceHome`'s `UserProfile` and the folders under it, held in a variable
  that is never named `$home`, which is PowerShell's own `$HOME` in any case), **never from its own process's
  `LOCALAPPDATA`, `APPDATA` or `USERPROFILE`**, which are the real ones: the harness points only the
  programs it starts.
- **It MUST NOT connect to any port it did not open.** The only address a case or a program under
  test connects to is the stand-in's `127.0.0.1:<port>`, and the receipt the case writes names no
  other. A case never probes a port, never reads a port from a file of Terry's and never uses 8811.
- **It MUST NOT print a token**: not the fixture's, not a decrypted one, not a header, and not in a
  failure reason (AGENTS.md, "A test MUST NOT print, log or format the values of an environment it
  captured"). It reports by a name or a count.
- **How a review confirms it is isolation, never a snapshot.** The reviewer checks that **every write
  and delete of a board case goes through `Assert-ConformancePath` at the call**, as the audit
  of [The program under test is untrusted](#the-program-under-test-is-untrusted) does, and the
  stand-in's own writes are the log and the ready file at the paths the case gave. The reviewer
  searches `conformance/` for `localswim-v2`, `LOCALAPPDATA`, `localswim.exe`, `LOCALSWIM_` and `8811`,
  **after running the same search against lines known to hold each, so that a zero is not believed
  blind**, and finds `localswim-v2` only in a path built from the home's object, `LOCALAPPDATA` only as
  the harness's own assignment, and `LOCALSWIM_` only in the case that sets and removes them. The
  reviewer never lists, reads or hashes Terry's real `%LOCALAPPDATA%\localswim-v2` as a proof.

#### What the board cases MUST show

**These are T04's case writers' list** (D86, D88; each result's name starts with the spec section it
enforces, as every result's does):
1. **Each refusal of the board read, with its error code and its `field`**
   ([lease-api.md, The board read](lease-api.md#the-board-read), [Exit codes and
   errors](lease-api.md#exit-codes-and-errors)), each from the stand-in or the credential and each
   with the message the spec gives, byte for byte. The messages below are the architect's rulings
   (pins P11 and P13), quoted for the case writer; lease-api.md owns them, and where the two differ
   lease-api.md wins and this list is the bug:
   - `--board` not a UUIDv4, a UUID of another version included, refused before any request (pin
     P18), and a board the stand-in does not know, `No board of ID '<value>' that agentspace can
     read.` (pin P13), each `invalid_value` for `board`;
   - `--epic` not a ticket number; a ticket the board has no card of, `No card of ticket '<value>' on
     board '<board>'. Give the epic's ticket number, as in '52'.`; and an archived card, `Epic
     '<value>' is archived. Name an epic that is being worked.` (pin P11), each `invalid_value` for
     `epic`;
   - No block, two or more blocks, a malformed fence, a block never closed, and a document
     `agentspace-plan` refuses, from a description in the fixture, each `invalid_value` for `epic`
     with `Epic '<value>' carries no usable plan: <why>.`, its `<why>` as the spec gives it (pin
     P11; [lease-api.md, The plan block](lease-api.md#the-plan-block));
   - `--turn` naming no turn of the epic's plan (`invalid_value` for `turn`);
   - A board that answers 401, 403, 400, 405, 406 or 500, a bare 404 with no Problem document (pin
     P17), one that hangs past the timeout, and a body past pin P1's bound (the last
     `invalid_value` for `epic` with `Epic '<value>' carries no usable plan: <why>.`, the others
     `internal`), each answered in agentspace's own words: **no token, no URL beyond the host and
     port, no path** (#0041);
   - The receipt and credential cases above, each `internal`, the `localhost` base among them (pin
     P19).
2. **The number of requests.** **One GET for a good epic**, whose log line is exactly the request
   [lease-api.md](lease-api.md#the-board-read) gives with `auth=ok`; **a ticket the board has no card
   of, and a board it does not know, each two lines** (the ticket view and then `?view=summary`,
   both under `/api/2026-11-10/`, pins P2 and P17), **and no more**; **a bare 404 one line**, since
   only a Problem document of code `not_found` leads to the second; **a refusal before any request** (a bad `--board`,
   `--epic`, `--turn` form, a usage conflict, a bad receipt or credential) **none**. A refusal of the
   epic reads the card once and never again, so a lease the case makes twice makes two good reads and
   not more.
3. **No other method was sent.** In every board case, the log holds no line whose method is not `GET`,
   whatever the case, and **a control proves the check can see one**: a planted line the case writes
   into a copy of the log, or a request the case itself sends to the stand-in with `POST`, which the
   stand-in answers 405 and logs, makes the same check FAIL. **A checker is proven to fail on a
   planted value before its pass is believed.**
4. **The token reached the stand-in, and appears nowhere else.** The log's line has `auth=ok`, which
   shows the stand-in compared the credential's token with the fixture's. **The token's value then
   appears in no stdout or stderr of any run of the program under test, no error document, no file
   under the throwaway home but `bot.json`'s blob and the fixture** (a search of every file of the home
   for the token's text and for its UTF-16LE bytes, with the fixture and the case's own files
   left out, and the search proven on a planted copy), **no log line and no lease state file**
   ([state.md](state.md#the-lease-state-file)), and **the program's own command line carries it
   nowhere** (the case starts it through the harness with the arguments the spec gives).
5. **The client reads three members and nothing else.** The markers in `prose.comments`,
   `relationships`, `parent` and `children` appear in no output, and **a card whose ignored members
   are changed gives the same result**, since a client that read one would differ.
6. **A board lease and the same plan given through `--plan` give the same size, `sized_for` and
   TTL** (D85, D86): the fixture's description holds the block built from the plan file the case
   also gives `--plan`, and the two printed documents are compared member by member. `size` and
   `sized_for` on an `unsatisfiable` lease for a project not on the list need no host, and the TTL is
   `expires_at` minus `created_at` of a lease that is `ready` on a seeded host, **for `--turn` given
   and for none given** (D84, D85; pin P10).
7. **`--board` and `--epic` with `--plan` or `--size`, and the other usage cases of pin P4**,
   the program refuses them before any request. **A case with no stand-in running could not show
   that none was made, so each of these cases starts one and finds its log empty.**

**What no case shows:** that the real board answers as the stand-in does. D88's live check shows it
once, after the install, with one GET that reads #0052.

### What the first run proved

**It answers [open question 3](#open-questions).** On 2026-11-18, on the laptop, with `USERPROFILE`
set to a new folder under `%TEMP%` (the OpenSSH client still finds the real home through the
account, so the real profile is not unreachable), the operator's
`ca` made a CA there and exited 0, a second `ca` exited 0 and changed nothing, and a ready
`lease create` then issued a lease, wrote its alias file and its credential under that home, and
printed a document that validates.

## The sorting

**History (E0.06-T06-S01, 2026-11-20): this section and the ones under it describe the prototype's
checks as they were before the prototype was removed.** The `scripts/` files they name no longer
exist, except `scripts/check-forward-release.ps1`.

### The summary

**Eleven suites and checks were sorted, and the keeper's stub with its suite.** Only the keeper
suite and the process half of the lease CLI suite are conformance in method and nearly movable
today. Both already drive a separate process through documented flags, and the keeper suite
already keeps each case's state in a throwaway folder.

| File | Class | The reason, in one line | Real folders it touches today |
|---|---|---|---|
| `test-lease-cli.ps1` | Mixed: the process half is conformance in method, the in-process half is prototype code | The second half dot-sources the state script. **Its process half is moved** to `conformance/lease-cli/` (#0084): `test-help.ps1`, `test-usage-errors.ps1` and `test-values.ps1` | None |
| `test-lease-cmd-create-get.ps1` | Conformance in method, **moved** to `conformance/lease-cli/test-create-get.ps1` (#0084) | The credential step had no root override. The moved check runs in a throwaway `USERPROFILE` with a throwaway CA, and the script in `scripts/` stays until the rewrite replaces it | The script in `scripts/` still: the real CA (signs with it), the real `leases` folder (written, removed), `~/.ssh` (hashed). The moved check: none |
| `test-lease-cmd-renew-release-list.ps1` | Prototype code as it stands; its checks are conformance in method | It dot-sources the driver to make each ready lease. **Renew and release are rewritten** in `conformance/lease-cli/test-lease-cmd-renew-release.ps1` (E0.04-T03-S03); `list` is E0.04-T03-S01's | The same as the line above |
| `test-lease-credential.ps1` | Prototype code | It dot-sources the credential script and calls its function. **Its certificate and key checks are rewritten** in `conformance/lease-cli/test-lease-certificate.ps1`; two checks are not movable (a bad ID and a second issue for one ID) | The real CA (signs with it), the real `leases` folder (written, removed) |
| `test-lease-renew-release.ps1` | Prototype code | It dot-sources the credential and alias scripts. **Its renew and release checks are rewritten** in `test-lease-cmd-renew-release.ps1`, and its certificate checks in `test-lease-certificate.ps1`; "no `renew-*` file is left" is now "the lease folder holds the three files" | The same as the line above |
| `test-lease-alias.ps1` | Prototype code | It dot-sources the alias script and calls five functions. **Its config, alias file and `ssh -G` checks are rewritten** in `conformance/lease-cli/test-lease-alias-file.ps1`; the alias-name acceptances, the short-ID collision and the internal calls are not movable | A throwaway subfolder inside the real `%USERPROFILE%\.agentspace` |
| `test-lease-driver.ps1` | Prototype code | It dot-sources the driver, reads its source text and a variable. **Its unsatisfiable, failed-alias and no-driver-name checks are rewritten** in `conformance/lease-cli/test-lease-driver-outcomes.ps1`; the registration checks stay prototype code | The real CA (signs with it), the real `leases` folder (written, removed), `~/.ssh` (hashed) |
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
- **Loads:** `scripts\lease-driver.ps1`, by dot-source, and calls the driver's place operation
  inside its own process to make each ready lease: the credential and the alias, with no record.
  It then writes the records into `leases.json` by hand.
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

- **Load:** `scripts\lease-credential.ps1`, and through it `scripts\lease-alias.ps1`, by
  dot-source. They call the credential steps of `scripts\lease-credential.ps1` inside their own
  process, with no CLI and no record: issue a lease's key and certificate for an ID they choose,
  sign a new certificate for it, and delete its folder.
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

- **Loads:** `scripts\lease-alias.ps1`, by dot-source, and calls five of its steps inside its own
  process: one that forms an alias from a project and an ID, one that tests an alias's form, one
  that makes sure of the `Include` line, one that writes an alias file, and one that removes it.
  It sets a variable of the prototype's to a fake repo, so that the default ProxyCommand names it.
- **Stubs:** None. `ssh.exe -G -F <alias file> <alias>` is the oracle, a program outside agentspace.
- **Folders:** A throwaway folder directly under the real `%USERPROFILE%\.agentspace`, removed in a
  `finally`. It never reads or writes the real `~/.ssh`.

| Group of checks | Count | What moving it takes |
|---|---|---|
| The alias name is `agentspace-<project>-<first 8 hex>`, lowercase; a non-UUID and a UUIDv1 refused; `..\x` refused; U+0130 and U+212A refused | 6 | Partly: the refusals are the normalizer probes above, and the name is visible in a ready `create` |
| The alias-name test accepts 3 values and refuses 8 | 11 | The refusals through the operator's `up -Alias <bad>` (see [the operator](#the-operator-and-the-token-push)). The three acceptances cannot be shown without reaching AWS |
| The `Include` line on LF, CRLF and BOM configs; a missing config; a second run changes nothing; the backup equals the original; a misplaced `Include` is refused | 9 | Prepare `AGENTSPACE_SSH_DIR` with each config shape, run a ready `create`, compare the config bytes |
| The alias file's bytes (no BOM, LF only, a comment line, no `ClearAllForwardings`, the folder holds only the file) and what `ssh -G` shows (user, identity, certificate, `identitiesonly`, `hostkeyalias agentspace`, strict host key checking, the proxy command, both remote forwards) | About 17 | From the alias file a ready `create` writes. "Set again changes nothing" has no CLI equivalent and is dropped; the comment line's naming of the prototype's script is replaced by "starts with a comment naming the lease ID" |
| The default ProxyCommand ends with the literal alias and has no `%n`; its path is `<repo>\scripts\ssh-proxy.cmd` | 3 | Done in `test-lease-alias-file.ps1`, by leaving `AGENTSPACE_PROXY_COMMAND` unset. The path is the prototype's layout, and is checked instead as D8 makes it checkable: the full path of a program that exists, run through `cmd.exe /d /c` when it is a `.cmd` or `.bat` ([state.md](state.md#the-alias-file)) |
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

**The push has a conformance check now, written through the seam that
[docs/operator.md](operator.md#the-test-seam) specifies** (E0.04-T03-S02 specified it, and
E0.04-T05-S05 wrote the checks): the environment variables that name the `aws`, `ssh` and `scp`
programs the operator runs, and the zone it pushes, which let a check stand in for them. **The
prototype does not meet the seam, so the push's check and the ProxyCommand's steps fail against the
prototype** ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). **The
v0.06 build meets it** (E0.06-T03), and the run record (E0.06-T04-S01) gives what each file does
against that build: the counts below are what each file holds.
**E0.06-T04-S02 (#0132, gap FB-25) added the checks that the seam lets a check make from outside,
as three more files** (the table below): `down`, `status` and `grow`, the operator's usage errors
and help texts, and the start paths of `up`. Each fails against the prototype, because the
prototype does not meet the seam.
**`scripts/test-push-localswim.ps1` stayed prototype code and kept passing** until E0.06-T06-S01
removed it (2026-11-20), with every group of it below. Four of its groups needed no stand-in and moved earlier, to `conformance/operator/`, in two
kinds: the refusals of `-Alias` (two groups) and the refusals of the ProxyCommand's arguments (two
groups).

#### `conformance/operator/`: conformance

**Ten checks, one helper file and no shim path.** Each check dot-sources `helpers.ps1`, which
dot-sources the harness (`conformance/lease-cli/harness.ps1`), so the programs are named by
`AGENTSPACE_OPERATOR_CMD` and `AGENTSPACE_SSH_PROXY_CMD` and run in a throwaway home with the
harness's whole environment ([The harness](#the-harness)). Each ends with `Exit-ConformanceRun`, and
each is run as `pwsh -NoProfile -File conformance/operator/<file>.ps1`.

| File | What it enforces | Against the prototype |
|---|---|---|
| `test-alias-refusals.ps1` | `up -Alias <bad>` and `status -Alias x` are refused before anything runs, with exit 1, the message and an empty stdout: 11 bad values (`a;b`, `a b`, `a&b`, the empty value, `a\|b`, a value with a double quote, `-Fx`, `a/b`, `a%b`, the Kelvin sign U+212A and U+0130) and the `status` case ([docs/operator.md](operator.md#command-line)) | 12 PASS, exit 0 |
| `test-proxy-arguments.ps1` | The ProxyCommand refuses a second argument before `up`, and fails in `up` on a bad alias with no tunnel started: `a/b extra`, `a;b`, `-Fx`, the empty argument, `a b` and `a/b` ([docs/operator.md](operator.md#the-proxycommand)). The refusal is matched as `^agentspace: \S+ takes at most one argument, the SSH host alias\.$`, since it names the program's own file (B-29), and `a;b` accepts either that refusal or the `-Alias` text, both exit 1, because a `.cmd` program splits it at the semicolon and a native one does not (B-28) | 6 PASS, exit 0 |
| `test-ca.ps1` | `ca` ([docs/operator.md](operator.md#ca), [docs/state.md](state.md#the-ca)): the OneDrive refusal; the first run (four lines on stderr, an ed25519 pair with no passphrase, the printed fingerprint); a later run that changes nothing; `-Quiet`; a private key alone, a public key alone and a public key that is not the private key's own, each refused and not repaired; and that `ca` starts no AWS CLI, `ssh` or `scp` | 10 PASS, exit 0 |
| `test-unmet-push.ps1` | The token push through the seam ([docs/operator.md](operator.md#the-token-push)): the ssh arguments, the two content lines on stdin, the stamp, the messages; an unchanged push sending nothing; a changed zone and a deleted stamp each making it due; a nested `up`; a missing receipt; an unreadable credential; an ssh that fails; a box that refuses the zone; a zone that fails the check; an ssh that hangs (the 20 seconds); the token's secrecy, the stand-in recording only the names of the environment variables that hold the token | FAIL, as expected: `seam not met` |
| `test-unmet-proxy-steps.ps1` | The ProxyCommand's steps through the seam ([docs/operator.md](operator.md#the-proxycommand)): the two folders first on `PATH`; `up` before the tunnel; the tunnel's exact arguments for the cached instance; the tunnel's own stdin and stdout; the tunnel's exit code; no alias and an alias alike; exit 1 with no tunnel when `up` fails; an expired or missing AWS sign-in gives exactly the sign-in message ([AWS failures](operator.md#aws-failures)) | FAIL, as expected: `seam not met` |
| `test-unmet-proxy-connect-time.ps1` | D1 at connect time, card #0087, through the seam: on success the caller's line `The host is ready.`, no `Sending ...` line and the tunnel's stderr withheld; on failure the exact "Could not reach the host" line last, and no `Sending ...` line ([The files that are expected to fail](#the-files-that-are-expected-to-fail)) | FAIL, as expected |
| `test-unmet-commands.ps1` | `down`, `status` and `grow` through the seam ([docs/operator.md](operator.md#down), [status](operator.md#status), [grow](operator.md#grow)): each command's calls in order with their arguments, its messages and its failures, and `grow`'s failed online grow (D36). `status` expects each failed call's failure line alone, with no partial report before it, as [operator.md, status](operator.md#status) says (architect, 2026-11-19). Written by E0.06-T04-S02 (#0132, gap FB-25). **96 results** | FAIL, as expected: `seam not met` |
| `test-unmet-usage-help.ps1` | The operator's usage errors with their exact text, their order, what is accepted, every help block byte for byte, and the system-folder refusals ([docs/operator.md](operator.md#command-line)); **no case runs a program with `USERPROFILE` unset or empty** ([The harness](#the-harness)). Written by E0.06-T04-S02 (#0132, gap FB-25). **143 results** | FAIL, as expected: the prototype has no help and reports each usage problem as its shell's own error record |
| `test-unmet-start-paths.ps1` | The start-and-wait step of `up` through the seam, with a stand-in `aws` whose answers change from call to call ([docs/operator.md](operator.md#starting-the-box-and-waiting-for-it), [What up waits for](operator.md#what-up-waits-for)): the stopped, stopping, pending and running boxes, the 20-minute rule, any other state, a launch time that is not of the form, the wait's messages and poll, the patch question's answers, and the 15-minute deadline, which **neither runs nor is reported by default and runs with `-Case deadline`** ([Cases that run only when asked for](#cases-that-run-only-when-asked-for)). Written by E0.06-T04-S02 (#0132, gap FB-25). **Its default run is 31 results, all PASS, exit 0; `-Case deadline` runs 5 (the seam probe, the deadline's 3 and the ssh and scp tripwire), and the two runs hold 34 distinct results between them** | FAIL, as expected: `seam not met`, one result |
| `test-unmet-deploy-passthrough.ps1` | `deploy` up to its step 5 through the seam ([docs/operator.md](operator.md#deploy), [Streams](operator.md#streams)), with a stand-in `aws` whose `cloudformation deploy` writes to stdout, as the real AWS CLI does, and then exits 0 or 255: the CLI's stdout passed through byte for byte, its stderr line passed through, the call returning once the CLI exits, the calls in order, and the failure message last on exit 255. The stand-in stack has no `InstanceId` output, so `deploy` stops at step 5 with its message, and `sync`, `go`, `ssh-keygen`, `ssh` and `scp` never run; the `ssh` and `scp` tripwires are checked. Written by E0.06-T05-S02 (#0140, defect #0139). **4 results** | FAIL, as expected: `seam not met`, one result (run on 2026-11-20, for E0.06-T05-S02) |
| `helpers.ps1` | The AWS gate, the stand-ins (a tripwire for each of `aws`, `ssh` and `scp`, and a stand-in with its own answers), a runner for a value that cmd.exe needs quoted, and the code points a value arrives with. It starts no program under test | Not a check |

- **An alias holding a line feed is not checked, and is a gap** ([The gaps](#the-gaps)). A line
  feed cannot cross the `cmd.exe` command line: it cuts the value `a<LF>` to `a`, a valid alias, so
  a run could get past the refusal and reach AWS. The case is left out rather than SKIPPED, so that
  the file can pass.
- **The AWS gate runs first, and probes every AWS CLI that exists.** Before any program that can
  reach AWS starts, the check runs `aws sts get-caller-identity` under the home's environment, for
  the fixed `C:\Program Files\Amazon\AWSCLIV2\aws.exe` of
  [docs/operator.md](operator.md#programs-it-starts) and for the `aws` that `PATH` finds, whichever
  exist. **Every case is SKIPPED, and nothing runs, when that call succeeds for any of them. When
  no AWS CLI exists, the checks run** (B-50): there is nothing to call, no check needs a real CLI,
  and each check names `aws`, `ssh` and `scp` as stand-ins ([What a conformance run
  needs](#what-a-conformance-run-needs)). Shown by running `test-alias-refusals.ps1` and
  `test-proxy-arguments.ps1` with the lookup of AWS CLIs made to find none: 12 and 6 passed.
- **A value reaches the operator whole.** The harness starts programs with .NET's argument quoting,
  which leaves `&` and `|` bare, and `cmd.exe` reads them as operators. The alias checks therefore
  run the operator through a one-line `.cmd` in the home that quotes the value, as an ssh
  ProxyCommand line does. **A value with a double quote is escaped with a backslash.** The other
  checks pass their arguments to the harness directly.
- **Windows PowerShell 5.1 passes U+212A and U+0130 intact.** Before each of those two cases, the
  check sends the value through the same `cmd.exe`, `.cmd` and `powershell.exe -File` path and
  compares the code points it receives (8490 and 304), and SKIPS the case when they differ
  ([open question 2](#open-questions)).
- **The push and proxy files carry their own stand-ins, and read no real file.** The push file
  writes into the throwaway home a fake localswim receipt and a fake Bot credential, protected for
  the current user as localswim's is, the instance-ID cache, a stand-in `aws` that reports a
  running instance launched long ago, and a stand-in `ssh` that behaves as the box does
  ([docs/operator.md](operator.md#what-the-box-does)) and records its arguments, its stdin and the
  names, never the values, of the environment variables that hold the token (the check hands it the
  fake token in a file beside the stand-ins, AGENTS.md's captured-environment rule). The token is a made-up value, and the alias is a name no ssh configuration holds, so
  an `ssh` that ignores the seam connects nowhere. **The stand-in `ssh` gets the fake token as the
  real one would, on its stdin and nowhere else** ([docs/operator.md](operator.md#secrecy)).
- **A program that prints nothing and exits 0 fails every case.** Set to such a `.cmd`, each
  variable makes `test-alias-refusals.ps1` fail its 12 cases, `test-proxy-arguments.ps1` its 6 and
  `test-ca.ps1` its first-run results (4 results, the last of them the one error that ends the
  file), and each of the seam files that needs the stand-ins its one `seam not met` result
  (`test-unmet-start-paths.ps1` included, which then never runs its deadline case).
- **`test-unmet-start-paths.ps1`'s `deadline` case waits the real 15 minutes, so it neither runs nor
  is reported by default, and runs with `-Case deadline`** (E0.06-T04-S02; [Cases that run only
  when asked for](#cases-that-run-only-when-asked-for)). No seam variable shortens the deadline of
  [operator.md, Starting the box](operator.md#starting-the-box-and-waiting-for-it), so the case runs
  `up` against a box whose patch check never finishes, and waits. **A default run takes about 2 to 3
  minutes**, passes its 31 results (exit 0) and prints the one line that is not a result, `Not run:
  the 15-minute deadline case; run it with -Case deadline.`; **`-Case deadline` takes about 17
  minutes** and runs 5 results (the seam probe, the deadline's 3 and the ssh and scp tripwire), under
  a limit of its own on `up` of 17 minutes, so it can never hang. The two runs hold 34 distinct
  results between them. `-Case <wildcard>` runs only the cases whose name matches (as
  `test-keeper.ps1`'s does): `stopped`, `stopping`, `stopping-start-fails`,
  `running-recent`, `running-old`, `pending-state`, `other-states`, `bad-launch-time`,
  `wait-messages`, `wait-answers`, `wait-invocation-polls`, `wait-20-polls` and `deadline`; a
  wildcard that would select `deadline` neither runs nor reports it and prints the same `Not run`
  line, and only `-Case deadline`, in any letter case, runs it.
  **A run that gives `-Case` is not a complete run**, and a build's run MUST be made without it
  (and once more with `-Case deadline`). Its stand-in `aws` is a Windows
  PowerShell script that a `.cmd` starts: each call takes the next answer of a list written for that
  call's `<service> <operation>` (the last repeats), and logs its arguments and the time. The
  patch question is answered by that stand-in, through Session Manager as the spec says; `ssh` and
  `scp` are tripwires, and the file checks that neither was ever started. **The poll's timing is
  checked loosely:** that a pause exists (checks at least 3 seconds apart) and is not long (at most
  30 seconds), never the exact 5 seconds. **The deadline is checked to fall from 14.5 to 16.5
  minutes after the start of the wait (870 to 990 seconds)**, the same window in the check and in
  [Cases that run only when asked for](#cases-that-run-only-when-asked-for). **The wait's order is
  exactly `ec2 wait --region us-west-2 instance-stopped --instance-ids <id>`, with no `--output
  json`**, as the FB-07 paragraph of [operator.md, AWS failures](operator.md#aws-failures) gives it.
- **Shown to be able to pass.** The two seam files were run against throwaway stand-ins of a
  conforming build, written for the purpose and not kept: a small operator that does `up` and the
  push as [docs/operator.md](operator.md) says, and a small ProxyCommand that does its steps.
  `test-unmet-push.ps1` passed 20 of 20, and passed 19 of 20 when the stand-in operator was changed
  to put the token on the ssh's command line (the secrecy result failed). `test-unmet-proxy-steps.ps1`
  passed 11 of 11. **No build had passed either file when they were written**, because none met the
  seam. The v0.06 build meets it, and the run record (E0.06-T04-S01) gives its runs.
- **v0.07's operator and ProxyCommand files** (E0.07-T04-S02, #0156) are `test-unmet-*` files
  beside these, through `helpers.ps1`, the same AWS gate and the same seam, with helpers of their
  own beside them: `host-proxy-common.ps1`, which serves `test-unmet-host-proxy-lookup.ps1`,
  `test-unmet-host-proxy-instance-id.ps1`, `test-unmet-host-proxy-stamp.ps1`,
  `test-unmet-host-alias-file.ps1`, `test-unmet-host-alias-no-proxy-program.ps1` and
  `test-unmet-host-alias-certificate.ps1`; `host-commands-common.ps1`, which serves
  `test-unmet-host-commands-line.ps1`, `test-unmet-host-commands.ps1`,
  `test-unmet-host-commands-all.ps1`, `test-unmet-host-commands-up.ps1`,
  `test-unmet-host-deploy.ps1`, `test-unmet-host-deploy-registry.ps1` and
  `test-unmet-host-deploy-folders.ps1`; and `host-provision-common.ps1`, which serves
  `test-unmet-host-provision.ps1`, `test-unmet-host-provision-failures.ps1`,
  `test-unmet-host-provision-start.ps1`, `test-unmet-host-provision-place.ps1`,
  `test-unmet-host-bootstrap-sync.ps1` and `test-unmet-host-bootstrap-sync-all.ps1`. Each case is a row of
  [operator.md, Which checks show it](operator.md#which-checks-show-it), and
  [The files that are expected to fail](#the-files-that-are-expected-to-fail) gives which file holds
  which. **The four files that E0.07-T03 edited** (`test-unmet-commands.ps1`,
  `test-unmet-usage-help.ps1`, `test-unmet-start-paths.ps1` and `test-unmet-push.ps1`) hold their
  T03 cases, among them `test-unmet-commands.ps1`'s cases of the host registry where the
  instance-ID cache was (#0151), and are not edited by T04. **A `provision` case runs
  `provision -Host <project>` itself, with the seam set**, never through a detached start, so its
  exit code and streams are the check's to read ([operator.md, The test seam](operator.md#the-test-seam)).
- **v0.08's operator files** (E0.08-T04-S01, #0182; D56, D57, D60, D61, D63) are `test-unmet-*` files
  beside these, through the same AWS gate and the same seam, each dot-sourcing the helper of its
  kind: `test-unmet-guard.ps1`, with `host-guard-common.ps1` as its own helper (which dot-sources
  `host-commands-common.ps1`); `test-unmet-status-leases.ps1` and `test-unmet-grow-zero.ps1`,
  through `host-commands-common.ps1`; `test-unmet-sync-patch-wait.ps1`, through
  `host-provision-common.ps1`; and `test-unmet-push-nested.ps1`, through `host-proxy-common.ps1`.
  Each case is a row of [operator.md, Which checks show
  it](operator.md#which-checks-show-it), and [The files that are expected to
  fail](#the-files-that-are-expected-to-fail) gives which file holds what.
  - **A `guard` case runs `guard` itself, with the seam set**, never a detached start: a stand-in
    `aws` records every call, and `ssh` and `scp` are tripwires. It seeds the throwaway home's
    `hosts.json`, lease state file and held locks, with times set relative to the real clock,
    because the suite has no fake clock ([operator.md, Which checks show
    it](operator.md#which-checks-show-it)). It never reads Terry's real files, and **it never
    touches a real host**: a host's stop and its real horizon file are the live checks of
    [v0.08's rows](#coverage-of-the-operator-and-the-keeper).
  - **No stand-in in these files models EC2's refusal of a size increase of 0 or less** (#0164;
    D61, pin P7): the operator refuses `-GiB 0` and a negative `<n>` itself, so a case counts no
    `ec2 modify-volume` call, and `test-unmet-grow-zero.ps1` holds that. **`helpers.ps1` and
    `test-unmet-commands.ps1` were not changed by E0.08-T04-S01**: E0.08-T03-S02 (PR #125) had
    already rewritten `test-unmet-commands.ps1`'s `-GiB 0` and `-GiB -1` cases to the refusal, and
    they stay beside `test-unmet-grow-zero.ps1` ([operator.md, Which checks show
    it](operator.md#which-checks-show-it); the overlap kept by the architect, 2026-11-22). The
    refusal with `USERPROFILE` unset is the operator crate's own test, since no check runs a
    program with it unset ([The harness](#the-harness)).
  - **Existing files that v0.08 changes, by [operator.md's table](operator.md#which-checks-show-it)
    and not by a card's scope. E0.08-T04-S01 changed three of them:**
    - `test-unmet-usage-help.ps1` gained `guard`'s help block, byte for byte, and `guard` in its
      command lists, so every per-command case covers it. The top level's, `status`'s, `grow`'s and
      `sync`'s help texts were already v0.08's text on `main` (E0.08-T03-S02, PR #125, had
      changed the file), so no other text of it changed.
    - `test-unmet-host-commands-line.ps1` gained `guard`'s refusals of `-Host`, `-All`, `-Alias`
      and `-InstanceType` in the order of the checks, and shows `-GiB` checked for form and its
      value ignored, against a plain `guard` as the control.
    - `host-commands-common.ps1`'s `Compare-Text` masks the spec line it quotes in a failure
      reason (architect, 2026-11-22), so `test-unmet-host-commands-all.ps1`'s `$leasesMissing`
      prints no Windows user name.
  - **Four files that operator.md's table names were not changed by E0.08-T04-S01**:
    `test-unmet-commands.ps1` (above), `test-unmet-host-commands-all.ps1` (`status -All`'s two new
    lines, E0.08-T03-S02, PR #125) and `test-unmet-host-provision.ps1` (the sign-in line of
    `provision`'s step 10, E0.08-T03-S05, PR #123), each already at v0.08's text on `main`, and
    `test-unmet-start-paths.ps1`, which the table names as the model of an on-request deadline case.
    `test-unmet-push.ps1` and `helpers.ps1` were not changed either, and the table does not name
    them. The cases of #0167's push are the new file `test-unmet-push-nested.ps1`, which runs the
    ProxyCommand program nested under a stand-in `ssh`, as the real `ssh` runs its alias's
    ProxyCommand.
- **v0.09's operator files** (E0.09-T04-S02, #0206; D67, D68, D70, D71) are `test-unmet-*` files
  beside these, through the same AWS gate and the same seam: `test-unmet-host-deploy-size.ps1`,
  through `host-commands-common.ps1`, and `test-unmet-host-provision-size.ps1`, through
  `host-provision-common.ps1`. Each case is a row of v0.09's part of [operator.md, Which checks show
  it](operator.md#which-checks-show-it), and [The files that are expected to
  fail](#the-files-that-are-expected-to-fail) gives which file holds what. **The same story gives
  `test-unmet-host-provision-failures.ps1` a case with a typed seed**, a `ready` record holding an
  `instance_type`, which shows a failed step 4.2 removing the type the record held (#0204's
  review's N1).
  - **A stand-in `aws` shows the arguments the operator passes and the answers it reads, and no
    more**: no stand-in can show a host's type change (D70; [operator.md, Which checks show
    it](operator.md#which-checks-show-it)). CloudFormation changing the instance's type, the host
    coming back at it, and the real AWS CLI's `describe-stacks` answer in the form read are D70's
    live checks ([v0.09's rows](#coverage-of-the-operator-and-the-keeper)).
  - **A `provision` case seeds the lease state file**, with times set relative to the real clock,
    and runs `provision -Host <project>` itself with the seam set, as v0.07's do.
  - **Existing files that v0.09 changed before T04, each checked on `main` at acfd6da:**
    - **E0.09-T02-S03 (#0202, PR #137):** `test-unmet-usage-help.ps1`'s copies of the top level's,
      `deploy`'s and `provision`'s help, at v0.09's text; and `test-unmet-host-commands-line.ps1`,
      whose six host checks became five, with check 6's message and its cases removed, since
      `deploy -InstanceType` is allowed for every host, the box included (D67; pin P5).
    - **E0.09-T03-S02 (#0204, PR #142):** `host-commands-common.ps1`'s `New-HostRecord` and
      `New-BoxRecord`, and `host-provision-common.ps1`'s `Get-HpHostsText` and `New-HpRecord`, seed
      `instance_type` `m9g.xlarge` on a `ready` record, with `-NoInstanceType` for none (pin P3);
      their stand-in stacks, `New-StackDoc` and `New-HpStack`, answer an `InstanceType` parameter,
      `m9g.xlarge` by default, which every deploy reads back; and `Get-NothingSignedProblems`
      allows the host folder and `provision.lock` a person's deploy makes (pin P34). The box's
      deploy passing every member of `stack-parameters.json` but `InstanceType`, in
      `test-unmet-host-deploy.ps1` and `test-unmet-host-provision.ps1`; the record holding the type
      read back, in `test-unmet-host-deploy-registry.ps1`; a box record written with no type, in
      `test-unmet-host-commands.ps1` and `test-unmet-host-commands-all.ps1`; and, in
      `test-unmet-host-provision-failures.ps1`, step 4's line carrying `type=m9g.xlarge`, the type
      recorded after a deploy that succeeded and none after one that failed, and the creation
      parameters ending with `InstanceType=m9g.xlarge`.
  - **One helper changed in T04 itself, by pin P56 (E0.09-T04-S03, #0207, commit 0691ed5):**
    `host-proxy-common.ps1`'s `New-HpHostRecord` seeds `instance_type` `m9g.xlarge` on a `ready`
    record, with `-NoInstanceType` for none, as pin P3 gave the two helpers above. #0204's pass had
    left it untyped, so `test-unmet-host-alias-file.ps1`'s lease `create` on its seeded host
    answered `pending` ([driver.md, On a `ready` host](driver.md#on-a-ready-host), row 2), and the
    file failed one case in the whole-suite run. The seven files that dot-source the helper,
    `test-unmet-host-proxy-lookup.ps1`, `test-unmet-host-proxy-instance-id.ps1`,
    `test-unmet-host-proxy-stamp.ps1`, `test-unmet-host-alias-file.ps1`,
    `test-unmet-host-alias-no-proxy-program.ps1`, `test-unmet-host-alias-certificate.ps1` and
    `test-unmet-push-nested.ps1`, pass with it
    ([docs/runs/e0.09-t04-s03.md](runs/e0.09-t04-s03.md#pin-p56-the-one-red-file-its-fix-and-its-green-run)).
- **v0.10's operator files** (E0.10-T04-S02, #0222; D77, with the pins P22 to P24 of v0.10-T01, and
  defect #0227) are two new `test-unmet-*` files beside these, through the same AWS gate and the same
  seam, and **change no helper and no existing file**: `test-unmet-host-provision-success.ps1` and
  `test-unmet-seam-cannot-run.ps1`, each through `host-provision-common.ps1`. Each case
  is a row of v0.10's table in [operator.md, Which checks show it](operator.md#which-checks-show-it),
  and [The files that are expected to fail](#the-files-that-are-expected-to-fail) gives which file holds
  which.
  - **A `provision` case runs `provision -Host <project>` itself, with the seam set, as v0.07's do**, and
    seeds the host registry and the lease state file with times set relative to the real clock. **Most
    seeded records hold a `last_success_at`**, so a case that finds none afterwards shows step 3's removal
    too, and not only that nothing was written.
  - **Row 1, "the same write":** that step 3 removes `last_success_at` in the same write that sets
    `last_attempt_at` cannot be seen from outside without a point between two writes. **A case shows it
    by comparing `hosts.json` byte for byte at the stand-in `aws`'s first call**, which every such case
    makes fail at once: the file MUST be the seed with `last_attempt_at` the run's start, `state` as step
    3 sets it, `last_success_at` removed and every other member and record as it was. The seeds hold no
    `instance_type`, so step 4.2's removal writes nothing and the file at the first call is step 3's
    write alone. No read of the file then sees `last_attempt_at` new and `last_success_at` old, which is
    the nearest a check comes (the architect, for v0.10-T04, on S02's N1).
  - **Row 2, "the time the run ended":** a case reads it as **any time from the second of the `place
    done` line to the second of the `run done` line**, since the lock wait of the write sits inside the
    run's end (the architect, for v0.10-T04, on S02's N2). The gated runs hold the lease
    state lock from the bootstrap's hook until the log shows `record done`, so that the run waits to place
    its leases, and the check then takes `hosts.lock` and lets the placing go on: the file MUST lack
    `last_success_at` while the write waits, and hold it only after the check lets the lock go, at or
    after `last_attempt_at`, with `run done` after it in the log and no other member changed.
  - **Row 6, the lock the write takes:** while the run waits for a `hosts.lock` the check holds,
    `provision.lock` MUST be held and the lease state lock free, and neither `success failed` nor `run
    done` is in the log yet; the message names `hosts.lock`'s own path.
  - **`test-unmet-seam-cannot-run.ps1` runs `bootstrap` with one program variable naming something that
    cannot be run**, which the seam says MUST NOT fall back to the default: `agentspace: <variable> names
    <value>, which cannot be run.` and exit 1, as the program is first needed
    ([operator.md, The test seam](operator.md#the-test-seam), "What each one does"; defect #0227, where
    `bootstrap`'s ssh answered such a file with `Could not run <path>.`). **The non-program file is
    written inside the throwaway home, through `Assert-ConformancePath`, and never started by the
    check.** The file runs from the wrapper's error mode (0x8001) and changes none, and **a file named
    `ssh.exe` that is not a program MUST NOT raise a modal "Unsupported 16-Bit Application" dialog**
    (#0225); a run that did would hang until the case's timeout, and **a dialog is a finding**.
  - **Two known limits of `test-unmet-host-provision-success.ps1`** (the reviewer's N7 and N8 on PR #160,
    ruled by the architect for v0.10-T04). **Neither makes a result wrong**; each says how a failure of
    the file is read.
    - **N7, card #0230, met by #0240** (E0.11-T04-S02, PR #172): the shared helper `Get-HpLogShape`
      (`host-provision-common.ps1`) quoted a log line it cannot parse, raw, in a failure's reason, and a
      case's `provision.log` is in its throwaway home, which `New-ConformanceHome` makes under `%TEMP%`, so
      such a line could carry the profile's 8.3 short name. **From v0.11 the helper quotes it through
      `ConvertTo-HpMasked`**, which hides the temp folder, the profile, the throwaway home, the computer name
      and the user name in every spelling it reads from the case's own environment, and
      `test-unmet-host-provision.ps1`'s reason for a log line that is not `<time> <step> <outcome>` quotes the
      line through it too (the architect, 2026-11-25, for v0.11-T04, pin P8). **That is #0230's whole change.**
      **A failure reason of any other file that quotes captured program output is not masked by it**, and
      **MUST be masked before it is printed outside the run's own folder or written to a record**, as for any
      command's error text (AGENTS.md, "Terry's real files in a review"); **the places that still quote
      captured output unmasked are #0243's**, listed whole, file and line, in [the v0.11 run
      record](runs/e0.11-t04-s03.md#captured-output-quoted-unmasked-0243).
    - **N8:** in a case that reports several results, an error in the check itself reports one `FAIL`
      under the case's name and not one for each result. **A file's count is therefore a count of
      results only when no case failed**: against an empty operator the success file gives 23 results
      and `main`'s build gives its 30, and a `FAIL` against a build is read from the reason it prints,
      not from the count.
  - **What no stand-in shows** is the crates' own tests (`agentspace-sync`'s
    `tests/provision_success.rs` and `agentspace-hosts`' `tests/last_success.rs`), and
    **a lease created after placing's last re-read and before the run exits** (pin P31) is not shown at
    all, since no stand-in can time it; it is the order of the locks
    ([driver.md, Conformance cases for v0.10](driver.md#conformance-cases-for-v010)).

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
| The hash of the credential file ignores CRs, a BOM and blank lines | 1 | Prototype code | Visible only as the stamp written after a confirmed send, which `test-unmet-push.ps1` checks against the SHA-256 of the instance, zone and content. How the box normalizes a CR, a BOM and blank lines is the box's half, and no check runs it ([Coverage](#coverage-of-the-operator-and-the-keeper)) |
| The alias test accepts 3 values and refuses 11 | 14 | The 11 refusals are conformance, **moved** to `conformance/operator/test-alias-refusals.ps1`; the 3 acceptances are prototype code | The refusals, through `up -Alias <bad>`: the operator fails before anything runs, with stdout empty (done: 11 values, the Kelvin sign and U+0130 among them, passed intact by Windows PowerShell 5.1). The acceptances proceed to AWS and cannot be shown |
| The push protocol: a wrong hash, a failed ssh, a hung ssh (the time limit), a confirmed send is recorded, the box gets exactly two lines, the ssh arguments, `-Alias` routes the send, a refused alias starts no ssh, an unchanged token is not re-sent, a forced send | About 25 | Prototype code | Rewritten as `conformance/operator/test-unmet-push.ps1`, through the seam: the ssh arguments, the two lines, a failed ssh, a hung ssh, a confirmed send recorded, an unchanged token not re-sent. Left: a forced send (`sync`) and `-Alias` routing for a lease alias ([Coverage](#coverage-of-the-operator-and-the-keeper)) |
| The time zone: a valid zone is sent in the remote command, the stamp includes it, a changed zone makes the push due, 16 invalid zones push the token and send no zone and warn, an empty zone, `zone-failed`, and the zone never changes whether the token counts | About 55 | Prototype code | Partly rewritten in `test-unmet-push.ps1`, through `AGENTSPACE_TIMEZONE`: a valid zone sent, the stamp including it, a changed zone making the push due, `zone-failed`, the seven legacy names mapped to their current ones, and eight invalid zones not sent. Left: the other invalid zones (an empty `AGENTSPACE_TIMEZONE` means the laptop's own zone, so the seam cannot give an empty one) |
| The zone tables: 7 legacy names, 4 current zones that pass through, 8 Windows IDs to IANA names, an unknown ID, `-TimeZone` is sent as given, no `-TimeZone` reads the laptop's zone | About 22 | Prototype code | Pure functions, whose only observable is the zone in the ssh command. They are a spec table ([docs/operator.md](operator.md#the-time-zone)) of which `AGENTSPACE_TIMEZONE` lets a check reach the legacy names and the zone check (done, in `test-unmet-push.ps1`), and not the Windows IDs, since the seam replaces the laptop's zone before that conversion ([Coverage](#coverage-of-the-operator-and-the-keeper)) |
| `up -Alias <bad>` fails on the alias, before anything, with nothing on stdout (5 values, 3 checks each); `status -Alias x` is refused | 16 | Conformance, **moved** to `conformance/operator/test-alias-refusals.ps1` | Done: it runs the program `AGENTSPACE_OPERATOR_CMD` names, in a throwaway home with the harness's environment, so a mistaken run cannot reach the token or AWS |
| `ssh-proxy.cmd` with a second argument (`<alias> extra`, `a;b`) is refused before the operator runs, exit 1, the message `ssh-proxy.cmd takes at most one argument`, stdout empty | 4 | Conformance, **moved** to `conformance/operator/test-proxy-arguments.ps1` | Done: it runs the program `AGENTSPACE_SSH_PROXY_CMD` names |
| `ssh-proxy.cmd -Fx`, `""`, `"a b"` and `a/b` fail in `up` on the alias, start no tunnel, stdout empty | 8 | Conformance, **moved** to `conformance/operator/test-proxy-arguments.ps1` | Done: the same program |
| A copy of `ssh-proxy.cmd` runs `up` or `up -Alias <alias>`, then the tunnel, and exits with the tunnel's code (7); it stops when `up` fails with exit 1 | 8 | Prototype code | Rewritten as `conformance/operator/test-unmet-proxy-steps.ps1`, through the seam: the steps, the exit codes (7 and 1), no tunnel after a failed `up` |

### The keeper suite

#### `conformance/keeper/test-keeper.ps1`, with `keeper-common.ps1` and its own `conformance/keeper/ssh-stub.cmd`: conformance

**E0.04-T03-S02 rewrote the 41 process cases of `scripts/test-board-holder.ps1` as this check, and
added one.** The original stayed in `scripts/` with its three installer cases, which were prototype
code ([below](#the-three-installer-cases-prototype-code)), until E0.06-T06-S01 removed it
(2026-11-20).

- **Drives:** The keeper as a separate process, only through the program that
  `AGENTSPACE_KEEPER_CMD` names, run with the keeper's own arguments unchanged
  ([docs/keeper.md](keeper.md#command-line)): `<program> -SshExe <stub> [-Now t]`, and in `-Watch`
  mode with `-IntervalSeconds`, `-MaxPasses`, `-BackoffBaseSeconds`, `-BackoffMaxSeconds`,
  `-FastFailureSeconds` and `-ReleaseGraceSeconds`. The program is found by the harness's
  `Get-ConformanceProgram`: when the variable is unset or empty, the default is the install folder's
  `agentspace-keeper.exe`, which `conformance/defaults.ps1` names, and nowhere else; no case names
  the keeper's file or script. (Until E0.06-T06-S01 the default was the shim
  `conformance/prototype/keeper.cmd`.) The variable is read before the first case, because
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
  a home folder (`USERPROFILE`). **Every process a case starts, the keeper and the
  stand-ins included, gets the harness's environment for that home** (`Get-ConformanceChildEnvironment`,
  [Running the suite](#running-the-suite)): `USERPROFILE`, `HOME`, `LOCALAPPDATA`
  (`<home>\AppData\Local`), `APPDATA` (`<home>\AppData\Roaming`), `AGENTSPACE_STATE_DIR` and
  `AGENTSPACE_SSH_DIR` inside the case's folder; `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE`
  naming files that do not exist; `AWS_EC2_METADATA_DISABLED=true`; the AWS profile, key, session
  and both container-credential variables removed; and every `AGENTSPACE_*` variable and
  `PSModulePath` the caller's shell held removed. The suite sets them on its own process, which
  every child inherits, and puts every one back in a `finally`. **The keeper's holders are the
  exception:** the keeper starts them, and the prototype's launcher, through WMI, so they get the
  user's logon environment ([docs/keeper.md](keeper.md#streams-and-environment)). That is safe only
  because the holder is the ssh stub, which touches nothing outside the run's folder; a holder that
  was the real ssh would find the real home through the account. It never uses the real
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

**#0066's done-when is shown (E0.06-T04-S01, 2026-11-19), for the Rust keeper.** The "Watch locked"
case held the lock until the first pass logged `skipped ... reason=locked`, waited for that line for
60 seconds, and waited for the keeper's exit for 240. It now waits up to 5 minutes for the line and
up to 10 for the exit, and a line that never comes is a result of its own (`Watch locked: A pass that
finds the lock held logs the skipped line before the deadline`), so a loaded laptop fails with what
the case was waiting for, never with a later check's reason. A release some seconds after the log
line is still safe: a pass that waits for the lock retries for up to 10 seconds
([docs/keeper.md, Locks](keeper.md#board-holderlock-one-pass-at-a-time)), so the second pass takes it
and logs `idle`. No sleep that stands for a condition remains. What is left is `Wait-Until`'s
100-millisecond poll, and the 6-second hold in the #0068 file, a bounded wait inside a window where
the keeper logs nothing ([keeper.md Q9](keeper.md#open-questions)).
- **The run** ([docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md), "#0066"): `-Case 'Watch: a
  locked pass*'`, 5 times in a row, against `target\dist\agentspace-keeper.exe`, while two copies
  of the whole keeper suite ran against the same build at the same time, as separate `pwsh`
  processes the run started. It ran on the tree of `31e011f`, whose checks and build are `aa3dc24`'s
  (the commits after it change documents only), with the programs not rebuilt. **5 of 5 exited 0**,
  each 5 passed, 0 failed, 0 skipped, in 17 to 18 seconds (alone, about 20). The two copies were
  still running when the fifth ended, and each ended 208 passed, 0 failed, 0 skipped.
- **What it does not show:** the Rust keeper is a light load, and the two copies slowed the case by
  nothing. The load that #0066 saw was the prototype's PowerShell keeper beside a live keeper, and no
  run with the prototype was made.

What had been run before it, on 2026-11-18, after the
cases were given the harness's environment and the 20-second wait: the full suite against the
prototype (206 checks passed, 0 failed, 0 skipped, exit 0, in 7 minutes 15 seconds; 207 passed with the shared-token check that E0.04-T05-S05 added, 0 failed, 0 skipped, exit 0); the full suite
against a keeper that prints nothing and exits 0 (119 of its 196 checks failed, and every case failed
at least one, exit 1, in 7 minutes 13 seconds); and the #0068 file ([below](#the-0068-file-test-unmet-holder-dies-mid-passps1)).
`-SelfCheck` ran in two groups before that change, with every case failing on its broken fixture,
in about 7 minutes 22 seconds each; after it, only the Idle case was run again.

| Cases | Count | Class | Notes |
|---|---|---|---|
| Idle; Start; Keep; Stop on release; Stop on expiry; Stop at the expiry second; Stop when the lease is gone; Stop leaves a foreign process alone; Dead PID; Reused PID (ready, released); Command line differs; Foreign process (released, ready); Last expiry; Unreadable lease state (with and without a holder); A holder that cannot be opened is kept; Lock busy; Bad `-Now`; Rotation of `board-holder-ssh.log` | 21 | Conformance | They read and write `leases.json`, `board-holder.json`, `board-holder.log`, `board-holder-ssh.log` and `board-holder.lock`, each documented in `keeper.md` or `state.md`. Two of them (Reused PID released, Command line differs) gained a check that the untrusted record is removed and one idle line is logged, because they passed against a keeper that does nothing |
| Watch: the backoff doubles and is capped; it resets after a long-lived holder; a holder that dies unproven backs off; a log that cannot be written does not end the loop; `-MaxPasses` ends the loop; passes are spaced; a locked pass is skipped; the log holds no secret; the next holder waits the release grace; a first holder is not delayed; a backoff longer than the grace is still waited; the holder after one the keeper stopped waits the grace; the grace is 95 seconds by default; `-ReleaseGraceSeconds <bad>` is a usage error (4 values); a second keeper exits at once; a new keeper starts once the first is killed and adopts its holder; a one-pass run is unaffected by a running keeper | 20 | Conformance | They depend on the flags above and on the `grace seconds=` and `another keeper is running pid=<n>` log lines. The kill case kills the process the suite started, by PID, and spares the holder |
| The files a keeper run writes match `schemas/state/`: the holder record, the launch request and the launch result against their schemas; the files' bytes; that the three files of one start share one token of 32 lowercase hex digits, matched case-sensitively (D35); the claim's content, `launcher <pid>` or `abandoned`; the launch files removed at the pass's end | 1 (new) | Conformance | Answers [docs/state.md](state.md#open-questions)'s Q13. The launch files live only while a holder is started, so a watcher inside the case keeps a copy of each as it appears, and a pass it missed is run again |

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
  keeper's 1-second interval, then its look, then a new 10-second wait, so the stop lands in the
  middle of that window, nominally 5 seconds from each edge. **That margin is smaller in practice:**
  the look takes time, and the check lets the lock go right after the kill, so under load each
  margin is nearer 3 seconds, and a missed window is why an attempt is run again. **The log then says which path the keeper took, and an
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
- **What each case checks:** that pass logs no `started` line, and logs `lost lease=<id> alias=<alias>
  pid=<pid>` for the tracked holder, with the PID the record named ([docs/keeper.md](keeper.md#log-lines);
  D35; Q6, closed); the line after it is `grace seconds=<n>`, with n the grace
  less rounding; and the next `started` line comes at least the grace after the check let the pass
  go. Case 2 also checks that the holder was still running. The grace counts from the end of the pass,
  which is never before the check released the lock, so a keeper that meets the rule passes.
- **Against the prototype:** each case fails four checks and passes the rest. The pass logs
  `started` at once (the first check, whose message quotes the line), it logs no `lost` line (the
  second, whose message quotes the same line), the line after it is `kept`
  and not a grace line (the third), and the next `started` line is that same one, stamped with a
  time before the check let the pass go (the fourth). Case 2's check that the holder was still running
  passes. The file exits 1: 2 passed, 8 failed, 0 skipped, in about 42 seconds. Against a keeper that
  prints nothing and exits 0, each case fails its first check, that the keeper starts a holder.
- **Not shown:** The file has not passed, because no build meets #0068. Its assertions were run
  against a made-up log of a keeper that meets the rule, to show that they can pass, and case 1
  reached its window on the first attempt in each of its runs against the prototype; a loaded laptop
  is what could make it miss, and it would then say `SKIPPED`, never `PASS`.

#### The holder-environment file: `test-holder-environment.ps1`

- **`test-holder-environment.ps1` shows the holder never inherits the keeper's environment** (keeper.md, Streams and environment, D35). It runs one pass over a ready lease with a stub holder of its own, which writes the names of its environment variables, never their values, to a file in the throwaway home. The keeper is given a sentinel variable, named `T04_KEEPER_SENTINEL_<random hex>` at run time (no `AGENTSPACE_` prefix), and `AGENTSPACE_STATE_DIR`, set for the keeper only, and a `USERPROFILE` that is the throwaway home, which is not the logon environment's. The stub also writes `yes` or `no` for whether its `%USERPROFILE%` is that home, never the value. The check requires that the file is complete and holds `SystemRoot` (a control that it is real), that neither the sentinel's name nor `AGENTSPACE_STATE_DIR`, nor any `AGENTSPACE_*` name, is in it, and that the marker says the holder's `USERPROFILE` is not the throwaway home. That the sentinel was set at the keeper's start is a precondition, not a result: the file prints a FAIL line when it was not, and no PASS when it was. The stub's `for /f` over `set` writes the text before the first `=` of each line of `set`'s output, so a value that held a line break could leak a fragment of itself into the file as a name; it is never printed, and the check drops every line that is not a variable name (`^[A-Za-z_][A-Za-z0-9_()]*$`). It asserts on names only, and the prototype passes it in about 4 seconds.

#### The launch-request file: `test-unmet-launch-request-full-paths.ps1`

- **`test-unmet-launch-request-full-paths.ps1` shows the holder's launch request carries full paths** when the state folder is given as a relative name (state.md, The home folder: an implementation MUST make a relative `AGENTSPACE_STATE_DIR` a full path before it writes it into a launch request; keeper.md, Checking the keeper: a check MAY watch the state folder for the launch files). It starts the keeper with `AGENTSPACE_STATE_DIR` set to a relative name and a working folder of its own (the folder that holds the case's state folder), keeps a copy of the request as it appears, and requires its `log`, `directory` and `result` to be the full paths the name resolves to. It does not edit `test-keeper.ps1` or `keeper-common.ps1`, which are the keeper suite's.

#### A host per project: the keeper's v0.07 cases

**E0.07-T04-S03 (#0157) adds the cases of
[keeper.md, Conformance cases for a host per project](keeper.md#conformance-cases-for-a-host-per-project)**
(D50, D53, D55), through `keeper-common.ps1`, with the same program, environment, stubs and
processes as the suite above. **RFC 2119 keywords again.**
- **H1 is a registry mode of the existing keeper files.** Every keeper file (`test-keeper.ps1`,
  `test-holder-environment.ps1`, `test-unmet-holder-dies-mid-pass.ps1` and
  `test-unmet-launch-request-full-paths.ps1`) takes `-Registry None|Box|Both`, `Both` by default.
  - **`Both` runs the whole file twice, as two child processes**, one in each of the other two
    modes. Each result line ends with ` [H1 registry=None]` or ` [H1 registry=Box]`, after its name
    and, on a `FAIL` or `SKIPPED` line, after its reason, so that a name still starts with the
    spec section it enforces (`PASS keeper.md#log-lines: ... [H1 registry=None]`). The summary
    line is the sum of the two. The file exits 1 if either run exited 1 or crashed, else 2 if
    either exited 2, else 0.
  - **`Box` makes each case write the box's record alone**,
    `{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"}]}`, to
    `<USERPROFILE>\.agentspace\hosts.json` of its throwaway home, through `Assert-ConformancePath`.
    **`None` writes no registry.**
  - **Each file ends with the result `keeper.md#log-lines: The box's lines that name a holder carry
    host=swimcoach, and no other box line carries a host= field`**, which is H1's "line for line
    but for ` host=swimcoach`" ([keeper.md, Log lines](keeper.md#log-lines)). Against v0.06's
    keeper it fails in both modes, by design.
  - **`keeper-common.ps1` gains the four helpers of the mode**: `Initialize-KeeperRegistryMode`,
    `Write-CaseRegistry`, `Register-LogLines` and `Get-LogLineProblems`.
- **H2 to H18 are in two files, with `conformance/keeper/keeper-hosts-common.ps1` as their helper**,
  dot-sourced after `keeper-common.ps1`, which writes the registry under the throwaway
  `USERPROFILE` and its own stand-in `ssh-hosts-stub.cmd`, whose holder exits at once or stays alive
  by the alias on its command line. Each case's result name starts with its number.
  - **`conformance/keeper/test-unmet-hosts-pass.ps1` holds the one-pass cases:** H2 to H8, H12 to
    H14, H16 and H18, and X1, the card's case that each host's holder writes its own
    `board-holder-ssh.<project>.log`, the launch request's log
    ([keeper.md, The holder's log](keeper.md#the-holders-log);
    [state.md, Another host's holder](state.md#another-hosts-holder)). **The card's two reasons of
    a registry that cannot be read are among H12's variants**: a folder at `hosts.json`'s path
    gives the pass's `reason=Host registry could not be read: Windows error <n>.`, and each file
    that fails state.md's rules for the registry gives `the host registry is not valid`
    ([keeper.md, Hosts](keeper.md#hosts), [Reasons](keeper.md#reasons)).
  - **`conformance/keeper/test-unmet-hosts-watch.ps1` holds the cases of a keeper that watches:**
    H9, H10, both halves of H11 (`H11a` and `H11b`, by the method of the #0068 file), H15 and H17.
- **#0138 is fixed in the same story:** the removal of `board-holder.json` in the "record is
  removed between two passes" case of `test-unmet-holder-dies-mid-pass.ps1` takes
  `-ErrorAction SilentlyContinue`, so every removal in `conformance/keeper/` of a file that may be
  absent tolerates its absence. The other such removals, in `test-keeper.ps1` and in the same
  file, already did on `main`.
- **What no keeper case shows is live** (D53): that the keeper holds the board on agentspace's own
  host ([Coverage of the operator and the keeper](#coverage-of-the-operator-and-the-keeper)).

#### The guard's start: the keeper's v0.08 cases

**E0.08-T04-S03 (#0184) adds the cases of [keeper.md, Conformance cases for the guard's
start](keeper.md#conformance-cases-for-the-guards-start)** (D56, D62, D63; G1 to G16), through
`keeper-common.ps1`, with the same program, environment, stubs and processes as the suite above.
**RFC 2119 keywords again.**
- **`conformance/keeper/test-unmet-keeper-guard.ps1` holds every case, G1 to G16.** The stand-in
  that `AGENTSPACE_GUARD_CMD` names is started at the keeper's first pass (G1). It is not started
  again within the interval, which the file shows two ways: the default of 300 seconds, by five
  passes at `-IntervalSeconds 1` making exactly one start (G2), and a short
  `-GuardIntervalSeconds`, the keeper's time seam ([keeper.md, When the keeper starts
  it](keeper.md#when-the-keeper-starts-it)), after which it is started again (G3), also from inside
  a wait (G4). It is never started while `guard.lock` is held (G5). A stand-in that cannot be
  started is logged as a failed start, and every holder is unaffected by it (G8; [keeper.md, What a
  failed start changes](keeper.md#what-a-failed-start-changes)). The rest are G6 and G7
  (`guard.lock` a folder, and no `.agentspace` folder), G9 (a relative `AGENTSPACE_GUARD_CMD`), G10
  (the guard's environment), G11 (a stand-in that outlives the keeper), G12 (a one-pass run), G13
  (`board-holder.lock` held), G14 (a path with a space), G15 (the two usage errors for
  `-GuardIntervalSeconds`) and G16 (a second keeper). **Each result name is
  `keeper.md#<section> (G<n>): <text>`**, so each row of G1 to G16 maps to a result line by name,
  and `-Case` takes a wildcard on a case's ID (`-Case G8` selects G8a and G8b, and `-Case G1` never
  G10 to G16). `keeper-common.ps1` was not changed by E0.08-T04-S03.
- **The file names its gaps in a "Gaps:" paragraph of its header**, each with its reason, and no
  row part is left out silently. They are parts that no stand-in can show from outside: G11's
  place outside the Job Objects of the processes above the keeper, the handles the guard inherits
  from it, its hidden window, process group and standard input, and the call's 10-second bound
  with the three `<why>` rows that keeper.md gives to the keeper crate's own tests. The rest are
  parts it asserts loosely, so that load cannot break them, or leaves out because the row does not
  name them: G4's start about 12 seconds after the
  first (a lower bound of 12 seconds and an upper of 16), G5's "no later than just before the first
  pass after the close" (shown as an order of lines, never a time), G13's no wait on
  `board-holder.lock`, G10's variables the row does not name, and pin P19's single try per wait.
- **The stand-in guard is the file's own**, a `.cmd` in the throwaway home that appends one line
  to a file of the case's own for each run and exits 0, or stays alive until the case stops it.
  **No case runs a real guard, and none touches `guard.log`**, which only a guard writes. A case
  that reads the guard's environment reports a mismatch by the variable's name, never by its
  value (AGENTS.md, "A test MUST NOT print, log or format the values of an environment it
  captured").
- **Every other keeper file runs with a stand-in guard too** ([The harness](#the-harness), P15),
  and ignores the guard's lines where it reads a keeper's log line for line (keeper.md, P18).
  E0.08-T03-S03 (#0179) brought the existing keeper files to that rule, and `keeper-common.ps1`
  gives a default for each of the stand-in guard and the tripwires for `AGENTSPACE_AWS_CMD`,
  `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD`.
- **What no keeper case shows is live** (D62): that the installed keeper, restarted to load v0.08's
  build, logs `guard started pid=<pid>` at its first iteration and then about every 300 seconds,
  which is E0.08-T05-S01's (#0186) ([Coverage of the operator and the
  keeper](#coverage-of-the-operator-and-the-keeper), v0.08's rows).

#### The three installer cases: prototype code

**These stayed in `scripts/test-board-holder.ps1`, whole and untouched, as prototype code, until
E0.06-T06-S01 removed the file (2026-11-20).** The rewrite MUST NOT read them. They are the three cases that load or test the prototype's packaging:

| Case | Count | Class | Notes |
|---|---|---|---|
| Installer: `-DryRun` prints the logon task and changes nothing (about 28 checks: task name, two triggers, repetition, run level, battery, restart count, the working directory, the action, the arguments) | 1 case | Prototype code (packaging) | The expected values are the prototype's packaging (a launcher program, the repository root as working directory). It also reads Terry's real scheduled task, read only, before and after, which no conformance check does. A rewrite installs its keeper its own way ([docs/keeper.md, The installer](keeper.md#the-installer); D37) |
| Installer: a running keeper is found by its command line | 1 case, 5 checks | Prototype code | It loads a prototype function from the installer's syntax tree |
| Installer: `-DryRun` warns from a worktree | 1 case, 2 checks | Prototype code | It tests the prototype's install location, not the keeper |

**The 41 process cases were also still in `scripts/test-board-holder.ps1`**, because a story MUST
NOT edit that file and the suites left in `scripts/` MUST keep passing until the rewrite replaced
them. E0.06-T06-S01 removed the file, so each case now has one home.

### The schemas check

#### `conformance/schemas/test-schemas.ps1` (moved from `scripts/test-schemas.ps1` by E0.04-T03-S02): neither kind

- **Does:** Runs `Test-Json` in-process on the JSON schemas and their samples: every sample
  validates, every invalid sample is rejected, the list schema's `$defs` equal the lease schema's,
  the TTL grammar is accepted and refused through the two input schemas, and a sample's host alias
  is `agentspace-<project>-<first 8 hex>`. **From contract v1** (E0.10-T01-S02, #0212; D76) the plan
  schema's `$defs` MUST appear word for word in the create request schema's, a check that runs on
  `schemas/` only, since contract v0 has no plan schema, and **every other check runs on `schemas/`
  and again on `schemas/v0/` (whose `$id` carries `v0`), each against its own schemas and samples
  only**, so a v0 sample is never checked against a v1 schema. **From contract v2** (E0.11-T01-S02, PR
  #163; D87) **the top level of `schemas/` is v2's (its `$id` carries `v2`), v1's are kept unchanged under
  `schemas/v1/` (whose `$id` carries `v1`), and every check runs on all three trees**, the `$defs` check
  on `schemas/` and `schemas/v1/`. `schemas/v1/` and `schemas/v0/` MUST each exist and hold a schema, a
  valid sample and an invalid one, so a deleted copy cannot pass silently. `-SchemaRoot <folder>` names a planted copy that proves each check fails.
- **Loads:** No product code. It reads `schemas\`, `schemas\v1\`, `schemas\v0\`, their `samples\` and nothing else.
- **Class:** Neither, and now conformance in the one sense that it has no program to name: it is in
  `conformance/`, so the rewrite MAY read it. It drives no program, so there is no variable. **It
  checks the package itself, so it cannot be pointed at another build, and the "program that prints
  nothing" test does not apply to it.**
- **What moving it changed:** The repository root is two folders up, not one. Its output is the
  suite's result lines and its exit code is the suite's ([The keeper suite](#the-keeper-suite),
  "Output"). Nothing else changed: it checked the same 289 things as before, and it still scanned the
  top level of `schemas/` only, as it did until contract v1 added `schemas/v0/` (above). The samples
  E0.04-T04-S01 added bring it to 293, all passing against the package on 2026-11-18. **No state schema is checked by this check:** every state
  schema is `test-state-schemas.ps1`'s, below, which checks each one that has samples and fails one
  that has none unless it is on a short, commented list of the schemas known to have none, and the
  three keeper schemas in `schemas/state/`, which are on that list, are checked by the keeper suite's
  case against the files a run writes ([docs/state.md](state.md#open-questions), Q13).
- **The copy in `scripts/test-schemas.ps1` was the same check**, left in place because a story MUST
  NOT edit a `scripts/test-*.ps1`. E0.06-T06-S01 removed it, so the check has one home.

#### `conformance/schemas/test-state-schemas.ps1` (E0.07-T04-S03): neither kind

- **Does:** Runs `Test-Json` in-process on every `schemas/state/*.schema.json` that has samples in
  `schemas/state/samples/` (E0.07-T04-S03, #0157; the architect, 2026-11-20, from v0.07-T01). Each
  valid sample is accepted. Each `invalid/` sample is rejected for a schema reason: an unparsable
  file or a broken schema is a failure, never a rejection. Each such schema declares draft 2020-12,
  has the state form of `$id`
  (`https://github.com/SixbucksSolutions/agentspace/schemas/state/v0/<file name>`) and has a title
  and a description. Every sample names a state schema, and a schema with a valid sample has at
  least one invalid one.
  - **Today three state schemas have samples**: the host registry, the project list and, from v0.10, the
    lease state file ([state.md, The host registry](state.md#the-host-registry),
    [The project list](state.md#the-project-list),
    [The lease state file](state.md#the-lease-state-file)), from E0.07-T01-S01's samples (#0141) and
    v0.10's: the host registry's `last_success_at` (D77) and the lease state file's plan leases and
    `plan_too_large` record (D74, D75; E0.10-T01-S03, #0213), each with invalid samples. **From v0.11 the
    lease state file's samples hold a board lease's `plan_source`** (D86; `leases.state.plan-source.json`, with
    its invalid ones), and the file's count is 139 at v0.10 and 163 at v0.11
    ([docs/runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md#the-schemas)).
  - **A state schema with no sample fails**, from #0213 (E0.10-T01-S03, 2026-11-24), **unless it is on the
    file's list of schemas known to have none**, so that a new schema, or one whose samples were removed,
    is never skipped silently. The list, `$knownWithoutSamples` in the file, holds six, each with where
    its files are checked instead: the keeper's three (`board-holder-launch-result.state`,
    `board-holder-launch.state` and `board-holder.state`, by `conformance/keeper/test-keeper.ps1`
    against the files a run writes), `instance-id.state` (the retired instance-ID cache, which nothing
    writes from v0.07), `localswim-pushed.state` (the push stamps, which `agentspace-push`'s tests check)
    and `stack-parameters.state` (written by a person, never by agentspace). **A schema on the list that
    has samples fails too**, so the list never outlives its reason.
  - **The lease state schema holds the plan schema's `$defs` word for word** (state.md, [What a plan
    lease adds](state.md#what-a-plan-lease-adds), since `Test-Json -SchemaFile` resolves no `$ref` to
    another file): every one of `schemas/plan.input.schema.json`'s `$defs` MUST appear, compared parsed,
    in `leases.state.schema.json`'s, and a missing plan schema fails, never skips.
- **Loads:** No product code. It reads `schemas\state\`, its `samples\` and the `plan.input.schema.json`
  beside the state folder, and nothing else.
- **Class:** Neither, with no program to name, as for the check above, so the "program that prints
  nothing" test does not apply to it. `-StateDir <folder>` points it at a scratch copy of
  `schemas/state`, which is how it was shown to catch a planted bad sample, an invalid sample that
  validates, an unparsable invalid sample and a wrong `$id`.
- **The top-level `test-schemas.ps1` scans `schemas/` and `schemas/v0/` and never `schemas/state/`**: the
  state schemas are this file's. `-StateDir` reads the plan schema from the parent of the folder it
  names, so a planted copy of `schemas/state` needs a `plan.input.schema.json` beside it.

### The idle check's test

**`box/tests/test_agentspace_idle.py` tests `box/system/agentspace-idle-check`** (E0.08-T01-S05,
#0172; D11, D57, D63). **It is not a conformance file:** it drives a shell script that runs on a
host, never a program of the laptop that the harness starts, so the harness, the
`AGENTSPACE_*_CMD` variables and the `test-unmet-` rule do not apply to it. It is a Python
`unittest` file that runs the script as a subprocess and MUST NOT import it.
- **What it covers.** D11's rule, in 7 cases of its `D11Rule` class: power-off after 60 or more
  minutes with no session and no CPU work, none under 60, and each of an SSH session, a Session
  Manager session, a patch run and CPU work keeping the host up. D57's horizon rule, in 15 cases
  of its `HorizonRule` class:
  - **Power-off past the horizon plus the grace** with no session and no patch run, whatever the
    CPU is doing, with the exact line; the grace read from the settings file, 15 replacing an
    invalid one with a warning; and the horizon shown as `@<seconds>` when `date` cannot format it.
  - **No power-off** with an SSH session, a Session Manager session or a patch run past the
    horizon, inside the grace, or with the horizon in the future.
  - **D11's rule alone** with no horizon file, or a garbage, empty or unreadable one, for a host
    busy with CPU work, and with a garbage one for an idle host; and a horizon in the future never
    stopping D11's own idle power-off.
  - **The 22 cases are counted from the file's methods**, not from a run; the run record gives the
    run's own count ([docs/runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md)).
- **Its seams** are the three variables D63 pins for `box/tests` alone: `AGENTSPACE_IDLE_STATE` (the
  state folder, `/run/agentspace` unset), `AGENTSPACE_IDLE_CONF` (the settings file,
  `/etc/agentspace/idle.conf` unset) and `AGENTSPACE_PROC_STAT` (the CPU reading, `/proc/stat`
  unset). Each case builds a throwaway tree of its own and puts stub `ss`, `pgrep` and `systemctl`
  commands first on `PATH`, and `date` for the one case that needs it, so **the real `/run`,
  `/etc` and `/proc` are never read, and `systemctl poweroff` is only a stub that records its
  arguments.** A test MUST NOT print the values of the environment it builds. The script's path
  comes from the test's own variable `AGENTSPACE_IDLE_CHECK`, `../system/agentspace-idle-check`
  beside the test unset.
- **How it is run:** from the repository's root, in a Linux environment, with
  `python3 -I -m unittest discover -v -s box/tests -p test_agentspace_idle.py` for this file alone, **and
  with `python3 -I -m unittest discover -v -s box/tests -p 'test_agentspace_*.py'` for the whole folder**,
  which is what a whole-suite run's box result MUST be. That is WSL's Ubuntu on the laptop, or a host.
  **The whole discovery runs 172 tests at v0.10-T04 and again at v0.11-T04** (the architect's ruling for E0.10-T04-S03, #0223; [docs/runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md#the-boxs-tests)):
  `test_agentspace_agents.py` 51, `test_agentspace_footprint.py` 82, `test_agentspace_idle.py` 22 and
  `test_agentspace_sample.py` 17, each file alone printing `OK`
  ([docs/runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md#the-boxs-tests)). **The one-file command is not
  the box's result**: v0.10-T04's crates-and-box runner ran it with that pattern and covered 22 of the 172,
  and the architect caught the gap, so a runner MUST give the whole pattern. **A test and a test run MUST
  NOT start WSL, except a whole-suite run's crates-and-box runner** (E0.08-T04-S04, #0185, and
  E0.10-T04-S03, #0223; T03's ruling,
  [handoff-v0.08.md, Rulings made during v0.08](handoff-v0.08.md#rulings-made-during-v008), on
  #0181's card): **that runner is the only run that starts it for these files**, as its tasking says
  how; the result is in the run record. A test that needs a bash on the laptop uses Git for Windows' bash.
- **Four parts of operator.md's idle-check row have no method in the file's 22, and no live check
  shows them, so they are gaps** (read from the file's methods): a horizon holding a CR; a horizon
  of 16 digits; D11's rule one second before the horizon plus the grace (the nearest case, a horizon
  5 minutes past with a grace of 15, is far from the boundary); and `HORIZON_GRACE_MINUTES` not set
  giving 15 (only an invalid value, and a value of 7, are tested). The settings file the cases
  build always sets it. **The four are carded as #0192** (Backlog; the architect, 2026-11-22, for
  v0.08-T04).
- **What it does not show is live** (D62): that the check on a host with no seam set powers the host
  off past its horizon plus the grace with the laptop away, and the horizon file as a real guard
  run writes it, which are E0.08-T05-S01's ([Coverage of the operator and the
  keeper](#coverage-of-the-operator-and-the-keeper), v0.08's rows).

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

**History (E0.06-T06-S01, 2026-11-20):** the prototype's suites that carried these headers were
removed, so what is left of this section is the headers of the files in `conformance/`.

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
`docs/driver.md` or `docs/keeper.md` is cited by the title of its section, and each of those docs
exists, so every link to one resolves (B-36).

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
| `ca` | operator.md: ca; [docs/state.md](state.md): The CA (Its files, Its checks) |
| The token push (expected to fail against the prototype) | operator.md: The token push (When it runs, What the laptop reads, The time zone, The stamp, What the laptop sends, What the laptop checks, Secrecy), The test seam; state.md: The push stamp |
| The ProxyCommand's steps (expected to fail against the prototype) | operator.md: The ProxyCommand, The test seam |
| `down`, `status` and `grow` (expected to fail against the prototype) | operator.md: down, status, grow, AWS failures, Calls by command, The test seam |
| The usage errors and help texts (expected to fail against the prototype) | operator.md: Command line, The top level, the Help block of each command, Streams |
| The start paths of `up` (expected to fail against the prototype) | operator.md: Starting the box and waiting for it, What up waits for, Calls by command, The test seam |
| A failed `create` whose cleanup fails (expected to fail against the prototype) | lease-api.md: Concurrency and atomicity, Exit codes and errors (the `internal` message); state.md: Creation, in order |
| The keeper's cases (one pass) | [docs/keeper.md](keeper.md): Log lines, Locks, Starting a holder; state.md: The keeper's files |
| The keeper's `-Watch` cases | keeper.md: Log lines, Locks, Starting a holder |
| The keeper's files against `schemas/state/` | [docs/state.md](state.md): The keeper's files (the holder record, a holder's launch files), and its Q13; keeper.md: The handshake |

**v0.07's rows** (E0.07-T04-S03, #0157), one per group of T04's new files, by the spec tables that
assign v0.07's cases: [driver.md, Conformance cases for v0.07](driver.md#conformance-cases-for-v007)
(C1 to C32), [operator.md, Which checks show it](operator.md#which-checks-show-it) and
[keeper.md, Conformance cases for a host per project](keeper.md#conformance-cases-for-a-host-per-project)
(H1 to H18). [The files that are expected to fail](#the-files-that-are-expected-to-fail) gives each
file's cases.

| Check group | Spec sections it would enforce |
|---|---|
| `no_capacity` for a project with no host and not on the list, and the list against the registry (C1, C2, C17, C21, C24, C32) | [driver.md, Where it places a lease](driver.md#where-it-places-a-lease), [Unsatisfiable requests](driver.md#unsatisfiable-requests); [state.md, The project list](state.md#the-project-list), [The host registry](state.md#the-host-registry); [lease-api.md, `create`](lease-api.md#agentspace-lease-create) |
| `pending` from `create`, and the provisioning run's start (C5 to C10, C16, C18, C22, C23, C27 to C31) | [driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run); [state.md, The host registry](state.md#the-host-registry), [The provisioning run's start](state.md#the-provisioning-runs-start), [The provisioning run's lock](state.md#the-provisioning-runs-lock), [The host registry lock](state.md#the-host-registry-lock); [lease-api.md, Environment](lease-api.md#environment) |
| A lease alias for a `ready` host that is not the box, and the box's alias as v0.06's (C3, C4, C20) | [driver.md, Where it places a lease](driver.md#where-it-places-a-lease); [state.md, A lease alias for a host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box), [The alias file](state.md#the-alias-file) |
| A `pending` lease's `get`, `list` and `release` (C11 to C15, C19, C25, C26) | [driver.md, check_pending](driver.md#check_pending), [Release a lease](driver.md#release-a-lease), [The boundary](driver.md#the-boundary), [Starting a provisioning run](driver.md#starting-a-provisioning-run); [state.md, Release, and what it leaves](state.md#release-and-what-it-leaves); [lease-api.md, `get`](lease-api.md#agentspace-lease-get), [`list`](lease-api.md#agentspace-lease-list), [`release`](lease-api.md#agentspace-lease-release) |
| The operator's `-Host` and `-All`, the registry, `deploy -Host`, the stack's status, `stack-parameters.json`, `-InstanceType`, the operator's folder and its sweep, and signing | [operator.md, Command line](operator.md#command-line), [Which host a command acts on](operator.md#which-host-a-command-acts-on), [The host registry, as the operator uses it](operator.md#the-host-registry-as-the-operator-uses-it), [Every host at once: `-All`](operator.md#every-host-at-once--all), [deploy](operator.md#deploy), [The stack](operator.md#the-stack), [The operator's folder, one per command](operator.md#the-operators-folder-one-per-command), [How the operator reaches a host](operator.md#how-the-operator-reaches-a-host), [up](operator.md#up), [down](operator.md#down), [status](operator.md#status), [grow](operator.md#grow); [state.md, The host registry](state.md#the-host-registry), [The operator's key and certificate](state.md#the-operators-key-and-certificate) |
| `provision`, `place_pending` and the ready-host rule; `bootstrap`; `sync -Host` and `sync -All` | [operator.md, provision](operator.md#provision), [bootstrap](operator.md#bootstrap), [sync](operator.md#sync), [Every host at once: `-All`](operator.md#every-host-at-once--all), [The test seam](operator.md#the-test-seam); [driver.md, place_pending](driver.md#place_pending); [state.md, The provisioning run's log](state.md#the-provisioning-runs-log), [The provisioning run's lock](state.md#the-provisioning-runs-lock), [The project list](state.md#the-project-list) |
| The ProxyCommand's host, its `instance_id` write and the push stamp per host; the operator's alias file and certificate | [operator.md, The ProxyCommand](operator.md#the-proxycommand), [From the ProxyCommand](operator.md#from-the-proxycommand), [How the operator reaches a host](operator.md#how-the-operator-reaches-a-host), [The stamp](operator.md#the-stamp); [state.md, Which host a connection reaches](state.md#which-host-a-connection-reaches), [A host's push stamp](state.md#a-hosts-push-stamp), [A host's operator alias](state.md#a-hosts-operator-alias), [The operator's key and certificate](state.md#the-operators-key-and-certificate), [Agentspace's host keys](state.md#agentspaces-host-keys) |
| The keeper per host (H1 to H18), each host's holder log, and the registry's read failures | [keeper.md, Hosts](keeper.md#hosts), [The order of a pass](keeper.md#the-order-of-a-pass), [Which host a lease is on](keeper.md#which-host-a-lease-is-on), [The hosts of a pass](keeper.md#the-hosts-of-a-pass), [Which lease](keeper.md#which-lease), [Stopping a holder](keeper.md#stopping-a-holder), [Idle](keeper.md#idle), [Watching](keeper.md#watching), [The keeper's wait](keeper.md#the-keepers-wait), [Release grace](keeper.md#release-grace), [Locks](keeper.md#locks), [Log lines](keeper.md#log-lines), [Reasons](keeper.md#reasons), [The holder's log](keeper.md#the-holders-log), [Stopping and starting the keeper](keeper.md#stopping-and-starting-the-keeper); [state.md, Another host's holder](state.md#another-hosts-holder), [The host registry](state.md#the-host-registry) |
| #0138: a missing `board-holder.json` tolerated by the keeper files' cleanup | No section of its own: it changes how a case cleans up, and each case keeps the sections it enforced |
| The state schemas' samples | [state.md, The host registry](state.md#the-host-registry), [The project list](state.md#the-project-list); the schemas in `schemas/state/` and their samples |

**v0.08's rows** (E0.08-T04-S04, #0185), one per group of T04's new files, by the spec tables that
assign v0.08's cases: [operator.md, Which checks show it](operator.md#which-checks-show-it),
[keeper.md, Conformance cases for the guard's start](keeper.md#conformance-cases-for-the-guards-start)
(G1 to G16) and [driver.md, Conformance cases for v0.08](driver.md#conformance-cases-for-v008) (R1 to
R15). [The files that are expected to fail](#the-files-that-are-expected-to-fail) gives each file's
cases.

| Check group | Spec sections it would enforce |
|---|---|
| `guard`: which hosts a live lease explains, the stop, the unexplained line, `provision.lock`, the horizon it sends, `guard.lock`, the failures and every `guard.log` line | [operator.md, guard](operator.md#guard); [state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon), [The guard's log](state.md#the-guards-log), [The guard's lock](state.md#the-guards-lock); D56, D57 |
| `status -All`'s `Leases` and `Guard` lines | [operator.md, status](operator.md#status), [status -All](operator.md#status--all); [state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon); D60 |
| `grow`'s refusal of `-GiB 0` and a negative `<n>` | [operator.md, grow](operator.md#grow), [Exit codes](operator.md#exit-codes); D61 (#0164, pin P7) |
| `sync`'s patch wait, and `up` not waiting on a failed patch run | [operator.md, sync](operator.md#sync), [The patch wait](operator.md#the-patch-wait), [Starting the box and waiting for it](operator.md#starting-the-box-and-waiting-for-it); D61 (#0163; pins P13, P21, P24) |
| #0167: no push at a start with a current stamp, one `The host is ready.` from a ProxyCommand connection whose push is due, two under `AGENTSPACE_DIAGNOSTIC`, a push that fails still showing its ssh's error lines | [operator.md, The stamp](operator.md#the-stamp), [What the laptop sends](operator.md#what-the-laptop-sends), [What the laptop checks](operator.md#what-the-laptop-checks), [up](operator.md#up), [What a caller sees](operator.md#what-a-caller-sees-d1-card-0087); D61 (P14) |
| The keeper's start of the guard (G1 to G16) | [keeper.md, Conformance cases for the guard's start](keeper.md#conformance-cases-for-the-guards-start), [Starting the guard](keeper.md#starting-the-guard), [When the keeper starts it](keeper.md#when-the-keeper-starts-it), [How the keeper starts it](keeper.md#how-the-keeper-starts-it), [guard.lock: tried, never held](keeper.md#guardlock-tried-never-held), [What a failed start changes](keeper.md#what-a-failed-start-changes), [The guard's environment](keeper.md#the-guards-environment), [Usage errors](keeper.md#usage-errors), [board-holder-keeper.lock: one keeper at a time](keeper.md#board-holder-keeperlock-one-keeper-at-a-time); [state.md, The guard's lock](state.md#the-guards-lock); D56, D62, D63 |
| `release` ending the lease's own connections and ssh clients, and writing `release.log` (R1 to R15) | [driver.md, Conformance cases for v0.08](driver.md#conformance-cases-for-v008), [Ending a lease's connections](driver.md#ending-a-leases-connections), [Each with its tree](driver.md#each-with-its-tree), [The order of a release](driver.md#the-order-of-a-release), [An ssh client's destination](driver.md#an-ssh-clients-destination), [Which processes are the alias's clients](driver.md#which-processes-are-the-aliass-clients), [The count](driver.md#the-count), [Where the count is logged](driver.md#where-the-count-is-logged); [state.md, The release log](state.md#the-release-log); [lease-api.md, `release`](lease-api.md#agentspace-lease-release); D59 |
| The idle check, D11's rule and D57's horizon | [operator.md, The idle check and the lease horizon](operator.md#the-idle-check-and-the-lease-horizon); D11, D57 |

**v0.09's rows** (E0.09-T04-S03, #0207), one per group of T04's new files, by the spec tables that
assign v0.09's cases: [driver.md, Conformance cases for v0.09](driver.md#conformance-cases-for-v009)
(Z1 to Z16) and v0.09's part of [operator.md, Which checks show it](operator.md#which-checks-show-it).
[The files that are expected to fail](#the-files-that-are-expected-to-fail) gives each file's
cases.

| Check group | Spec sections it would enforce |
|---|---|
| The four sizes met at their types, and `unsupported_size` for any other (Z1, Z2) | [driver.md, What it supports](driver.md#what-it-supports), [Sizes and instance types](driver.md#sizes-and-instance-types), [On a `ready` host](driver.md#on-a-ready-host); [lease-api.md, Size](lease-api.md#size), [Fields](lease-api.md#fields); D65, D71 |
| Place on a `ready` host by its type and the project's leases: met at once, `no_capacity` with D66's message, `pending` and a resizing run, a record with no type or one the map does not hold, a held `provision.lock`, the 5-minute rule, no CA, another state folder, and a `pending` lease holding every new one (Z3 to Z11, Z13, Z14) | [driver.md, On a `ready` host](driver.md#on-a-ready-host), [Starting a provisioning run](driver.md#starting-a-provisioning-run), [A resizing run](driver.md#a-resizing-run); [lease-api.md, Fields](lease-api.md#fields), [A host's life follows its leases](lease-api.md#a-hosts-life-follows-its-leases); [state.md, The instance type](state.md#the-instance-type); D66, D67, D68 |
| The box by its type, and the type a missing `hosts.json` implies and never writes (Z12, Z15, Z16) | [driver.md, Where it places a lease](driver.md#where-it-places-a-lease), [What is true of today's box](driver.md#what-is-true-of-todays-box); [state.md, The host registry](state.md#the-host-registry), [The instance type](state.md#the-instance-type); D66's change (pins P2c, P31) |
| `deploy` by size: `-InstanceType` for every host, the box's arguments, the record of the type, step 4's removal, a failure after a successful deploy, and a person's deploy holding `provision.lock` | [operator.md, Command line](operator.md#command-line), [deploy](operator.md#deploy), [Which checks show it](operator.md#which-checks-show-it); [state.md, The instance type](state.md#the-instance-type), [The host registry lock](state.md#the-host-registry-lock); D67 and its changes (pins P11, P34) |
| `provision`'s type from the lease state file, its locks, its log lines and step 9's `place refused` and `place left` | [operator.md, provision](operator.md#provision); [driver.md, A resizing run](driver.md#a-resizing-run), [place_pending](driver.md#place_pending); [state.md, The lease state lock](state.md#the-lease-state-lock), [The instance type](state.md#the-instance-type), [The provisioning run's log](state.md#the-provisioning-runs-log); D68 and its changes (pins P26, P35, P37) |

**v0.10's rows** (E0.10-T04-S03, #0223), one per group of T04's new files, by the spec tables that
assign v0.10's cases: [driver.md, Conformance cases for v0.10](driver.md#conformance-cases-for-v010)
(Y1 to Y5), v0.10's part of [operator.md, Which checks show it](operator.md#which-checks-show-it) and
[lease-api.md, Changes from v0](lease-api.md#changes-from-v0). [The files that are expected to
fail](#the-files-that-are-expected-to-fail) gives each file's cases.

| Check group | Spec sections it would enforce |
|---|---|
| `create --plan` picking the size, a plan's busiest turn, its agents at once and its peak (`test-plan-sizes.ps1`) | [lease-api.md, Plan](lease-api.md#plan), [`create`](lease-api.md#agentspace-lease-create); D73, D74, D75 |
| `sized_for` in every state and every command that prints a lease, and no output holding the plan (`test-plan-sized-for.ps1`) | [lease-api.md, Fields](lease-api.md#fields), [Output](lease-api.md#output); D75 |
| `plan_too_large`, `--plan` with `--size`, the refusals of a plan with their messages and the order the values are checked in (`test-plan-errors.ps1`) | [lease-api.md, Plan](lease-api.md#plan), [Fields](lease-api.md#fields), [`create`](lease-api.md#agentspace-lease-create), [Command lines](lease-api.md#command-lines), [Exit codes and errors](lease-api.md#exit-codes-and-errors), [Messages](lease-api.md#messages); D74, D75, D81 |
| Contract v1 keeping v0 for a command line with no `--plan`, and v0's schemas kept (`test-v1-compatible.ps1`, and `test-schemas.ps1` over `schemas/v0/`) | [lease-api.md, Versioning](lease-api.md#versioning), [Changes from v0](lease-api.md#changes-from-v0), [Schemas](lease-api.md#schemas); D76, D80 |
| A run due at once after a run that succeeded, and 5 minutes for retries (Y1 to Y5, `test-unmet-pending-due-after-success.ps1`) | [driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run), steps 2 and 4, and "After the run"; [state.md, The host registry](state.md#the-host-registry), [The last success](state.md#the-last-success); D77 (pins P22, P24, P31) |
| `provision` removing `last_success_at` at step 3, writing it at step 9's end, and writing none for a run that did not succeed (`test-unmet-host-provision-success.ps1`) | [operator.md, provision](operator.md#provision), [Which checks show it](operator.md#which-checks-show-it); [state.md, The last success](state.md#the-last-success), [The order of the locks](state.md#the-order-of-the-locks); [driver.md, Lock order](driver.md#lock-order); D77 (pins P22 to P24) |
| A program variable that names something that cannot be run, never falling back to the default (`test-unmet-seam-cannot-run.ps1`) | [operator.md, The test seam](operator.md#the-test-seam), "What each one does"; #0227 |
| The state schemas' samples: the lease state file with plan leases and a `plan_too_large` record, the host registry's `last_success_at`, and every state schema holding a sample or named as having none | [state.md, What a plan lease adds](state.md#what-a-plan-lease-adds), [The last success](state.md#the-last-success), [The host registry](state.md#the-host-registry); the schemas in `schemas/state/` and their samples; D74, D77 |

## The files that are expected to fail

**History (E0.06-T06-S01, 2026-11-20): the prototype is gone, so these files are now ordinary
checks that the Rust programs pass** (the note at the top of this page). What follows says what each file showed against the prototype, and a `FAIL` in
any of them is now a defect.

**RFC 2119 keywords again.** Every file in `conformance/` named `test-unmet-<what>.ps1` checks a
MUST that the prototype does not meet yet (E0.04-T03-S03, #0086; the pins of v0.04-T03). It says
so in its header, with the requirement and its card.

- **Every other check file passes against the prototype in full, with one exception.**
  `conformance/lease-cli/test-lease-alias-file.ps1` exits 2 in a checkout with no `.instance-id` of
  its own, such as a worktree: the prototype's `create` then fails under the default transport, so
  the default line cannot be read, and that one result is SKIPPED, which is not a failure
  ([The gaps](#the-gaps)). The failure itself is `test-unmet-default-transport-no-cache.ps1`'s. **`test-lease-ignores-instance-cache.ps1` (E0.06-T04-S03) exits 2 for the same reason**, in the same checkouts, with its one default-transport result SKIPPED.
  **v0.05's build meets it** (E0.05-T04-S01, 2026-11-19): the Rust build's `create` needs no
  instance-ID cache, so this file exits 0 against it, 26 passed and none skipped, in a worktree too
  ([the run record](runs/e0.05-t04-s01.md)). A run of any other that does not pass is a regression
  of the prototype, or a defect in the check.
- **Every `test-unmet-*` file FAILs against the prototype, and a rewrite MUST pass it, with one
  exception.** `test-unmet-default-transport-no-cache.ps1` FAILs against the prototype only from a
  checkout with no `.instance-id`; from the main checkout on the laptop, which holds one, the
  prototype passes it, because the prototype reads its own checkout's `.instance-id`, which a check
  cannot hide and MUST NOT touch. Its result lines still say `FAIL`, and nothing turns a `FAIL`
  into a `PASS`. **The exception is gone for v0.05's build** (E0.05-T04-S01, 2026-11-19): its lease CLI has no
  checkout and needs no instance-ID cache, and it passes this file, 2 of 2 results, from a worktree
  that holds no `.instance-id`. It stays for the prototype alone, until v0.06 (#0055) retires it. Some
  of a file's lines PASS against the prototype, since each file also checks what the failing line
  assumes.
- **A runner MUST NOT count the FAIL of a `test-unmet-*` file as a regression of the prototype.**
  It MUST count a `PASS` of one against a rewrite, and a `FAIL` of one against a rewrite, as the
  rewrite's result. A runner finds these files by the `test-unmet-` prefix.
- **Each file checks MUSTs only.** A SHOULD is never a result line, because a conforming build MAY
  choose otherwise for a good reason (the certificate's valid-after back-dating, D24, is one).
- **Each lease CLI file drives the lease CLI only**, through the harness, in a throwaway home. Every delete
  and write it makes goes through `Assert-ConformancePath`, and a path built from the program's
  output is used only after the document has passed its schema and the ID and alias patterns.
  A failure reason never quotes the program's output, which can hold a path or a name.
- **The rows of a spec's "Requirements the prototype does not meet yet" table, such as
  [operator.md's](operator.md#requirements-the-prototype-does-not-meet-yet), are what these files
  assert**. **A row leaves its table when the prototype is retired** (v0.06, #0055), and not when
  a build passes its file: until then the table's column "Met by v0.05's Rust build" records the
  builds that meet it, and "Not shown" there says that no file checks the row (architect,
  2026-11-19, for E0.05-T04-S01).

| File | Requirement and spec section | Card | What it shows against the prototype | Met by v0.05's Rust build |
|---|---|---|---|---|
| `conformance/operator/test-unmet-proxy-connect-time.ps1` | The ProxyCommand names no backend on success, prints agentspace's own message on failure, and prints backend detail only under `AGENTSPACE_DIAGNOSTIC` ([docs/operator.md](operator.md#what-a-caller-sees-d1-card-0087)), asserted through the seam that names the AWS CLI, `AGENTSPACE_AWS_CMD` ([The test seam](operator.md#the-test-seam)) | #0087 (D1), and #0085 for the seam | FAIL on both paths, `seam not met (AGENTSPACE_AWS_CMD was not used)`: the prototype runs the AWS CLI by a fixed path, so the stand-in is never called. Nothing more is asserted until the seam is met | Not a lease CLI row: v0.05's build has no operator or keeper. **The v0.06 build passes all 14** (E0.06-T04-S01, [docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)) |
| `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1` | A keeper that tracks a holder, and whose pass does not find it trusted (its process gone while the pass waits for `board-holder.lock`, or its record gone), starts no holder in that pass and waits the release grace after it, logged `grace seconds=<n>` ([docs/keeper.md](keeper.md#release-grace)), and the pass logs `lost` for the tracked holder ([docs/keeper.md, Log lines](keeper.md#log-lines); D35) | #0068 | FAIL on 4 checks of each of its 2 cases: the pass logs `started` at once, it logs no `lost` line, the line after it is not a grace line, and the next start is not the grace after the pass was let go. Nothing else fails ([The #0068 file](#the-0068-file-test-unmet-holder-dies-mid-passps1)) | Not a lease CLI row: v0.05's build has no operator or keeper. **The v0.06 build passes all 10** (E0.06-T04-S01, [docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)) |
| `conformance/keeper/test-unmet-launch-request-full-paths.ps1` | A keeper started with a relative `AGENTSPACE_STATE_DIR` writes a holder's launch request whose `log`, `directory` and `result` are full paths ([docs/state.md, The home folder](state.md#the-home-folder); [A holder's launch files](state.md#a-holders-launch-files)) | #0110, point 4; #0133 | FAIL on 3 of its 6 results: the prototype writes the relative state folder into the request as given, so `log` and `result` are not full paths, and it records no holder in the state folder the relative name resolves to. The first three results pass | The Rust keeper of v0.06-T04 passes all 6 (E0.06-T04-S03) |
| `conformance/lease-cli/test-unmet-no-ca.ps1` | `create` and `renew` MUST fail with `internal`, change nothing and make no CA when there is none ([lease-api.md, The credential](lease-api.md#the-credential); [state.md, Its checks](state.md#its-checks)) | #0029, point 3 | Both succeed, and each makes a CA that no host trusts. `create` leaves a record, a lease folder and an alias file, and `renew` changes the lease's record and certificate | Yes: `test-unmet-no-ca.ps1`, 6 of 6 results pass |
| `conformance/lease-cli/test-unmet-internal-no-path.ps1` | An `internal` message MUST carry no absolute path, no home folder, no user name and no other program's text ([lease-api.md, Exit codes and errors](lease-api.md#exit-codes-and-errors), Messages) | #0041, point 1 | `renew` of a lease with no key pair fails `internal`, as it MUST, and the message holds a drive-letter path and the home folder | Yes: `test-unmet-internal-no-path.ps1`, 6 of 6 results pass |
| `conformance/lease-cli/test-unmet-release-released.ps1` | `release` of a `released` lease MUST delete whatever is left of its alias file and lease folder ([lease-api.md, `release`](lease-api.md#agentspace-lease-release); [state.md, Release, and what it leaves](state.md#release-and-what-it-leaves)) | #0041, point 3 | It exits 0 and prints the first `released_at`, and touches no file: the alias file and the lease folder put back after the first release are still there | Yes: `test-unmet-release-released.ps1`, 4 of 4 results pass |
| `conformance/lease-cli/test-unmet-argument-line-feed.ps1` | A line feed in a flag's value MUST give `invalid_value` for `project`, in the `--project <value>` and `--project=<value>` forms alike ([lease-api.md, The program](lease-api.md#the-program) and [Command lines](lease-api.md#command-lines)) | #0083 | `bin\agentspace.cmd`, a batch file, is cut by cmd.exe at the line feed before the CLI sees the value: the create succeeds with the part before it and records a lease, in both forms | Yes: `test-unmet-argument-line-feed.ps1`, 4 of 4 results pass. The Rust program takes its arguments from the operating system, with no `cmd.exe` between, so a line feed arrives whole |
| `conformance/lease-cli/test-unmet-record-no-driver.ps1` | A stored `ready` record with no `driver` member MUST be read as `ec2`, so `renew` and `release` MUST succeed on it, and neither MUST add the member ([state.md, The lease state file](state.md#the-lease-state-file); [lease-api.md, `renew`](lease-api.md#agentspace-lease-renew) and [`release`](lease-api.md#agentspace-lease-release)) | #0041, point 2 | Both fail with `internal`. The record stays as it was, so the lines that check the member is not added pass | Yes: `test-unmet-record-no-driver.ps1`, 7 of 7 results pass |
| `conformance/lease-cli/test-unmet-include-message.ps1` | When the `Include` line is below a `Host` line, `create` MUST fail, change nothing, and say that a person has to move the line to the top of the file ([state.md, The `Include` line](state.md#the-include-line)). Only the move and the top are checked, never the template's wording ([lease-api.md, Messages](lease-api.md#messages)) | #0086 | `create` fails with `internal` and changes nothing, as it MUST, but the message says nothing of moving the line or of the top of the file | Yes: `test-unmet-include-message.ps1`, 3 of 3 results pass |
| `conformance/lease-cli/test-unmet-renew-keeps-certificate.ps1` | A failed `renew` MUST leave the previous certificate byte for byte, as well as `expires_at` and `ttl_seconds` ([lease-api.md, Concurrency and atomicity](lease-api.md#concurrency-and-atomicity); [state.md, Renewal](state.md#renewal)). The write of the record is made to fail by holding `leases.json` open with `FileShare.Read` | #0041, point 3 | `renew` fails with `internal` after about 13 seconds, and leaves the new certificate in place | Yes: `test-unmet-renew-keeps-certificate.ps1`, 3 of 3 results pass |
| `conformance/lease-cli/test-unmet-default-transport-no-cache.ps1` | With `AGENTSPACE_PROXY_COMMAND` unset and no instance-ID cache in the home, a ready `create` MUST succeed: `create` MUST NOT read, check for or depend on the cache ([state.md, The alias file](state.md#the-alias-file); [driver.md, How it is reached](driver.md#how-it-is-reached)) | D21 (architect ruling, #0080) | From a checkout with no `.instance-id`, such as a worktree, `create` fails with `internal`: the prototype refuses to write the alias without its checkout's copy of the cache. From the main checkout it passes ([the exception above](#the-files-that-are-expected-to-fail)) | Yes: `test-unmet-default-transport-no-cache.ps1`, 2 of 2 results pass, from a worktree with no `.instance-id` |
| `conformance/lease-cli/test-unmet-release-partial.ps1` | A `release` that fails part way MUST NOT leave a `ready` record with its alias, key or certificate gone, and a retried `release` MUST finish it ([lease-api.md, Concurrency and atomicity](lease-api.md#concurrency-and-atomicity); [state.md, Release, and what it leaves](state.md#release-and-what-it-leaves)). The failure is made by holding the lease's `id_ed25519` open with `FileShare.None` | #0041, point 3 | `release` fails with `internal` after deleting the alias file, and leaves a `ready` record with no alias file. The retry then succeeds | Yes: `test-unmet-release-partial.ps1`, 3 of 3 results pass |
| `conformance/lease-cli/test-unmet-failed-create-cleanup.ps1` | A failed `create` MUST remove every file and folder it made, and when a removal itself fails MUST name each leftover in its `internal` message in its home-relative form, MUST leave no lease record, so none that is `ready`, and MUST change nothing else ([lease-api.md, Concurrency and atomicity](lease-api.md#concurrency-and-atomicity); [state.md, Creation, in order](state.md#creation-in-order)). The removal is made to fail by a deny rule, for the current user, on the throwaway home's `leases` folder (Delete and Delete subfolders and files, inherited), and the create by a file where `agentspace.d` has to be | The architect's ruling of v0.04-T05 (E0.04-T05-S05, #0092; found while marking [driver.md's Q8](driver.md#open-questions)) | `create` fails with `internal`, leaves the lease's empty folder, and records nothing. Its message holds the absolute path of that folder and names it in no home-relative form: 2 of 7 results FAIL, and the other 5 PASS | Yes: `test-unmet-failed-create-cleanup.ps1`, 7 of 7 results pass. Its two results that failed against the prototype pass: the build names the leftover in its home-relative form |
| `conformance/operator/test-unmet-push.ps1` | The token push through the test seam: the ssh arguments, the two content lines, the stamp, the messages, the due and not-due cases, the zone checks, the hung ssh's 20 seconds and the token's secrecy ([operator.md, The token push](operator.md#the-token-push); [The test seam](operator.md#the-test-seam)) | #0085 (the seam) | FAIL on the first result, `seam not met (a stand-in was not used)`: the prototype runs the AWS CLI and `ssh` by fixed paths, so no stand-in is called. Nothing more is asserted until the seam is met | Not a lease CLI row: v0.05's build has no operator or keeper. **The v0.06 build passes all 20** (E0.06-T04-S01, [docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)) |
| `conformance/operator/test-unmet-proxy-steps.ps1` | The ProxyCommand's steps through the test seam: the two folders first on `PATH`, `up` before the tunnel, the tunnel's exact arguments for the cached instance, the tunnel's own stdin and stdout, its exit code, exit 1 with no tunnel when `up` fails, and the sign-in message for an expired sign-in ([operator.md, The ProxyCommand](operator.md#the-proxycommand)) | #0085 (the seam) | FAIL on its first result, `seam not met (AGENTSPACE_AWS_CMD was not used)`: the prototype runs the AWS CLI by a fixed path | Not a lease CLI row: v0.05's build has no operator or keeper. **The v0.06 build passes all 11** (E0.06-T04-S01, [docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)) |
| `conformance/operator/test-unmet-commands.ps1` | `down`, `status` and `grow` through the test seam: the calls in order with their arguments, the messages exactly, the failures, and `grow`'s failed online grow ([operator.md, down](operator.md#down), [status](operator.md#status), [grow](operator.md#grow); D36) | #0132 (E0.06-T04-S02, gap FB-25) | FAIL on its first result, `seam not met`: the prototype runs the AWS CLI and `ssh` by fixed paths | Not a lease CLI row: v0.05's build has no operator. **The v0.06 build passes all 96** (E0.06-T04-S01, [docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)) |
| `conformance/operator/test-unmet-deploy-passthrough.ps1` | `deploy`'s pass-through of the AWS CLI's output, and the call returning once the CLI exits, through the test seam, up to step 5 ([operator.md, deploy](operator.md#deploy), [Streams](operator.md#streams)) | #0140 (E0.06-T05-S02, defect #0139) | FAIL on its first result, `seam not met`: the prototype runs the AWS CLI by a fixed path (run on 2026-11-20) | Not a lease CLI row: v0.05's build has no operator. **The v0.06 build with E0.06-T05-S02's fix passes all 4**, and v0.06's build before it hung on 2 of them ([docs/runs/e0.06-t05-s02.md](runs/e0.06-t05-s02.md)) |
| `conformance/operator/test-unmet-usage-help.ps1` | Every usage error with its exact text and order, and every help block byte for byte ([operator.md, Command line](operator.md#command-line); the Help block of each command) | #0132 (E0.06-T04-S02, gap FB-25) | FAIL: the prototype has no help of its own, and reports each usage problem as its shell's own error record, with no message of agentspace's | Not a lease CLI row: v0.05's build has no operator. **The v0.06 build passes all 143** (E0.06-T04-S01, [docs/runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)) |
| `conformance/operator/test-unmet-start-paths.ps1` | The start-and-wait step of `up` through the test seam: a stopped and a stopping box, the 20-minute rule, the wait's messages and poll, the patch question's answers and the 15-minute deadline, which neither runs nor is reported by default and is waited for in full with `-Case deadline` ([Cases that run only when asked for](#cases-that-run-only-when-asked-for); [operator.md, Starting the box](operator.md#starting-the-box-and-waiting-for-it), [What up waits for](operator.md#what-up-waits-for)) | #0132 (E0.06-T04-S02, gap FB-25) | FAIL on its one result, `seam not met (AGENTSPACE_AWS_CMD was not used)`, and so it never waits out the deadline: the prototype runs the AWS CLI by a fixed path | Not a lease CLI row: v0.05's build has no operator. **The v0.06 build passes it**: the default run is 31 results, all PASS, exit 0, in about 2 to 3 minutes, and `-Case deadline` runs 5 results (34 distinct between the two runs) in about 17 minutes (E0.06-T04-S02) |
| `conformance/lease-cli/test-unmet-id-line-feed.ps1` | A line feed inside a lease ID MUST be refused with `invalid_value` for `id` (`get`, `renew` and `release`), one before the ID MUST be trimmed and the lease found, and the program MUST receive the line feed ([lease-api.md, Input tolerance](lease-api.md#input-tolerance), [The program](lease-api.md#the-program)). Each ID with a line feed inside is well-formed and names no lease, so a program that cuts it still changes nothing | #0110 point 3 (#0029 point 2), E0.06-T04-S03 (#0133) | `bin\agentspace.cmd` is cut by cmd.exe at the first line feed: an ID with one inside is read as the part before it, so `get` of a real lease prints it and the others say `not_found`; one with a line feed first is read as empty. 11 of 14 results FAIL, and 3 PASS | Yes: `test-unmet-id-line-feed.ps1`, 14 of 14 results pass |
| `conformance/lease-cli/test-unmet-runtime-refused-argument.ps1` | Every run that fails, an argument such as `-:x` included, ends with one error document and an exit code from the table: `usage` for an unknown option, `invalid_value` for a flag's value ([lease-api.md, The program](lease-api.md#the-program), [Command lines](lease-api.md#command-lines)) | #0110 point 3 (#0083 point 1), E0.06-T04-S03 (#0133) | The prototype's runtime refuses `-:x` before the CLI starts: exit 1 and no error document. 14 of 16 results FAIL, and the two that PASS are `agentspace -:` and `create --project=-:x` | Yes: `test-unmet-runtime-refused-argument.ps1`, 16 of 16 results pass |
| `conformance/lease-cli/test-unmet-state-duplicate-id.ps1` | A state file with two records of one `id`, or a record with an `id` member twice, is not valid: every lease command fails with `internal`, "could not be read", and changes nothing ([state.md, The lease state file](state.md#the-lease-state-file)) | #0110 point 4, E0.06-T04-S03 (#0133) | `get` and `list` read the file and exit 0. `renew`, `release` and `create` fail with `internal`, but not with "could not be read", and `create` leaves a lease folder and an alias file. 15 of 25 results FAIL, all in the two variants with a repeated record; the `id` member twice PASSES | Yes: `test-unmet-state-duplicate-id.ps1`, 25 of 25 results pass |
| `conformance/lease-cli/test-unmet-unknown-driver.ps1` | A `renew` or `release` of a record whose `driver` names no known driver MUST fail with `internal`, change nothing, and name the stored driver in no output ([state.md, The lease state file](state.md#the-lease-state-file); [driver.md, The driver's name](driver.md#the-drivers-name); D1) | #0110 point 4 (#0041 point 2), E0.06-T04-S03 (#0133) | `renew` and `release` fail with `internal` and change nothing, as they MUST, but the message names the stored driver. 2 of 8 results FAIL | Yes: `test-unmet-unknown-driver.ps1`, 8 of 8 results pass |

- **What is shown of each new file's passing half.** The push and proxy-steps files were run
  against stand-ins of a conforming build and passed ([The operator and the token
  push](#the-operator-and-the-token-push)). **The failed-create file's passing half was first
  run against the Rust build, on 2026-11-19 (E0.05-T04-S01):** the build names a leftover in its
  home-relative form, and its two results that FAIL against the prototype PASS, 7 of 7. The program
  that prints nothing fails its setup result.
- **The D1 file's stand-ins are its own.** It writes two `.cmd` files into its throwaway home, one
  `aws` that reports a made-up running instance and one that fails with error text, both logging
  every call. A build that meets the seam calls them, and the file then asserts what the caller's
  stderr holds (no backend text on success; one `agentspace: ` message on failure; the diagnostic
  setting changes neither the exit code nor stdout). It sets `AGENTSPACE_NESTED`, so that `up`
  pushes no token. It also sets `AGENTSPACE_SSH_CMD` and `AGENTSPACE_SCP_CMD` to a stand-in that
  MUST NOT be called on this path, which logs and exits 1, so that a build that starts `ssh` or `scp`
  is caught and never reaches the real home.
- **The AWS gate still applies**, so even a build that ignores the seam cannot reach the real
  account: with the call log empty, the file reports the failure and nothing else.

**The valid-after back-dating** is a SHOULD ([lease-api.md, The credential](lease-api.md#the-credential)),
and no file checks it.

**v0.07's files (E0.07-T04, #0155, #0156 and #0157).** **RFC 2119 keywords again.** Each is a
`test-unmet-*` file, or a mode of an existing file, for a MUST that v0.07 adds, and **the installed
v0.06 programs, which are the suite's defaults until E0.07-T05-S01 installs v0.07, do not meet
them**: each story MUST show its files failing against them where v0.06 lacks the behavior, in a
throwaway home, and passing against the build of `main`. **A v0.07 build MUST pass every one**, and a `FAIL` against it is the build's
failure, as for every file above. Each case's result name says which spec case it is (`C5`, `H3`,
or the row of [operator.md, Which checks show it](operator.md#which-checks-show-it) in a few words).

| File | Cases it holds | Spec section | Card |
|---|---|---|---|
| `conformance/lease-cli/test-unmet-no-capacity.ps1` | C1, C2, C17, C21, C24, C32 | [driver.md, Where it places a lease](driver.md#where-it-places-a-lease); [state.md, The project list](state.md#the-project-list) | #0155 (E0.07-T04-S01) |
| `conformance/lease-cli/test-unmet-pending-create.ps1` | C5, C7 to C10, C16, C18, C23, C27, C28, C30, C31 | [driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run), [Where it places a lease](driver.md#where-it-places-a-lease); [state.md, The host registry](state.md#the-host-registry), [The provisioning run's lock](state.md#the-provisioning-runs-lock), [The provisioning run's start](state.md#the-provisioning-runs-start) | #0155 |
| `conformance/lease-cli/test-unmet-pending-create-long.ps1` | C6 and C29, which share one stand-in run that keeps running for 60 seconds after it recorded | [driver.md, Starting a provisioning run](driver.md#starting-a-provisioning-run); [state.md, The host registry lock](state.md#the-host-registry-lock), [The provisioning run's start](state.md#the-provisioning-runs-start) | #0155 |
| `conformance/lease-cli/test-unmet-pending-create-lock.ps1` | C22, with `hosts.lock` held by the check, one hold let go within the 30 seconds and one held past them | [driver.md, Where it places a lease](driver.md#where-it-places-a-lease); [state.md, The host registry lock](state.md#the-host-registry-lock) | #0155 |
| `conformance/lease-cli/test-unmet-host-alias-bytes.ps1` | C3, C4, C20 | [driver.md, Where it places a lease](driver.md#where-it-places-a-lease); [state.md, A lease alias for a host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box), [The alias file](state.md#the-alias-file) | #0155 |
| `conformance/lease-cli/test-unmet-pending-get-list-release.ps1` | C11 to C15, C19, C25, C26, and the release of a `pending` lease that a killed placement left files for (the card's comment, 2026-11-20): with its own alias file, the lease folder and the alias are removed and the lease is recorded released; with a foreign alias file of the same name, the file is kept byte for byte, and the release fails with `internal` and changes nothing | [driver.md, check_pending](driver.md#check_pending), [Release a lease](driver.md#release-a-lease); [state.md, Release, and what it leaves](state.md#release-and-what-it-leaves) | #0155 |
| `conformance/lease-cli/provision-standin.ps1` | No case: the helper the `pending` and `no_capacity` files dot-source, which writes each case's stand-in run (a `.cmd` under the home that records what it is given and makes no host) and reads back what it recorded | [driver.md, Conformance cases for v0.07](driver.md#conformance-cases-for-v007) | #0155 |
| `conformance/lease-cli/harness.ps1`, changed | No case of its own: the provisioning-run tripwire the lease CLI gets as `AGENTSPACE_PROVISION_CMD`, `Add-ConformanceReadyHost`, `Clear-ConformanceHosts` and `Get-ConformanceTripwireStarts` ([The harness](#the-harness)) | [lease-api.md, Environment](lease-api.md#environment); D54 | #0155 |
| `lease-check-support.ps1` and the existing lease CLI checks that make a `ready` lease, changed | No case of their own: each seeds a `ready` host with `Add-ConformanceReadyHost` before a `create` that expects `ready`, as D54 needs, and `New-CcReadyLease` does so unless given `-NoHost` | As before | #0155 |
| `conformance/operator/test-unmet-host-commands-line.ps1` | The command line for `-Host` and `-All`: their forms, every usage error they add, the six host checks in their order, the home and system folder checks before the host is looked up, a project with no record refused by every command but `provision`, and which help a line with `-Host` asks for (the help texts are `test-unmet-usage-help.ps1`'s) | [operator.md, Which host a command acts on](operator.md#which-host-a-command-acts-on), [Command line](operator.md#command-line) | #0156 (E0.07-T04-S02) |
| `conformance/operator/test-unmet-host-commands.ps1` | `down`, `status` and `grow` with `-Host`, and every command without it acting on the box; `-Host swimcoach` as no `-Host`; a leftover `instance-id.json` ignored; the host registry (missing, not valid, an instance ID read from the stack and written into its record, a write that fails); `hosts.lock` held (Q10); the key, certificate and alias made per command (signing); the `Include` line's failure worded the same by `sync -Host` and `status -Host` (Q9) | [operator.md, Which host a command acts on](operator.md#which-host-a-command-acts-on), [The host registry, as the operator uses it](operator.md#the-host-registry-as-the-operator-uses-it), [How the operator reaches a host](operator.md#how-the-operator-reaches-a-host) | #0156 |
| `conformance/operator/test-unmet-host-commands-all.ps1` | `status -All` and `down -All`: the order and the `Host` lines, going on past a failed host and the closing count, `-Quiet`, a missing registry, and the sign-in message ending the command | [operator.md, Every host at once: `-All`](operator.md#every-host-at-once--all), [status](operator.md#status), [down](operator.md#down) | #0156 |
| `conformance/operator/test-unmet-host-commands-up.ps1` | `up -Host <project>` with and without `-Alias`: its host's instance ID, the key, certificate and alias made first, the one push and the stamp, a nested `up`, and the start-and-wait step | [operator.md, up](operator.md#up), [The token push](operator.md#the-token-push) | #0156 |
| `conformance/operator/test-unmet-host-deploy.ps1` | `deploy` given `-Host` and the box's: the existence check and every `StackStatus` (`REVIEW_IN_PROGRESS`, `ROLLBACK_COMPLETE`, `*_IN_PROGRESS`), the arguments for a new stack, an existing one and the box, `stack-parameters.json`'s refusals, `-InstanceType`, and the failures of `cloudformation deploy` | [operator.md, deploy](operator.md#deploy), [The host registry, as the operator uses it](operator.md#the-host-registry-as-the-operator-uses-it) | #0156 |
| `conformance/operator/test-unmet-host-deploy-registry.ps1` | `deploy`'s steps 4 to 6 on the registry: the instance ID removed and written, `Instance: <id>`, a registry that cannot be written, `hosts.lock` held, and `sync` run with `deploy`'s `-Host` and `-Quiet` | [operator.md, deploy](operator.md#deploy), [The host registry, as the operator uses it](operator.md#the-host-registry-as-the-operator-uses-it) | #0156 |
| `conformance/operator/test-unmet-host-deploy-folders.ps1` | The operator's folder, one per command: `deploy` and `sync` each write into a new one, never one another's, and remove it as they end, after a failure too; a folder that cannot be written fails the command; the 24-hour sweep | [operator.md, The operator's folder, one per command](operator.md#the-operators-folder-one-per-command) | #0156 |
| `conformance/operator/test-unmet-host-bootstrap-sync.ps1` | `bootstrap`'s ssh arguments, the script's bytes on its stdin and its failure message; `sync` and `sync -Host`, with `swimcoach` too, making the table's calls in order through the `ssh` and `scp` stand-ins, each naming the host's alias, with the build in the operator's folder | [operator.md, bootstrap](operator.md#bootstrap), [sync](operator.md#sync) | #0156 |
| `conformance/operator/test-unmet-host-bootstrap-sync-all.ps1` | `sync -All`: the order and the `Host` lines, a skipped host, `system\` written and one build, a host that fails and the closing count and exit code, and a build that fails | [operator.md, Every host at once: `-All`](operator.md#every-host-at-once--all), [sync](operator.md#sync) | #0156 |
| `conformance/operator/test-unmet-host-provision.ps1` | `provision` for a run that reaches its end: the calls in order, the record at each of them, the log's lines, the ready-host rule, `provision.lock` held for the whole run, and `provision -Host swimcoach` | [operator.md, provision](operator.md#provision) | #0156 |
| `conformance/operator/test-unmet-host-provision-failures.ps1` | `provision` when each step fails: the calls made and none after, the step's message, the log's `<step> failed` line, the record (`failed`, or a `ready` host left `ready`), the lock released, and the lease left `pending` | [operator.md, provision](operator.md#provision), [AWS failures](operator.md#aws-failures) | #0156 |
| `conformance/operator/test-unmet-host-provision-start.ps1` | `provision`'s first steps: the `-Host` checks, the project list (missing, empty, not valid), a held `provision.lock`, and the registry failing at steps 3 and 8, each writing nothing it should not | [operator.md, provision](operator.md#provision), [The host registry, as the operator uses it](operator.md#the-host-registry-as-the-operator-uses-it) | #0156 |
| `conformance/operator/test-unmet-host-provision-place.ps1` | `provision`'s step 9, `place_pending`, with a seeded lease state file: the leases placed, the key, certificate and alias written, the log's `place done placed=<n>` and `place failed lease=<id> reason=<message>` lines, a lease released meanwhile, a missing ProxyCommand program | [operator.md, provision](operator.md#provision); [driver.md, place_pending](driver.md#place_pending) | #0156 |
| `conformance/operator/host-commands-common.ps1` and `host-provision-common.ps1` | No case: the helpers the `host-commands` and `host-deploy` files, and the `host-provision` and `host-bootstrap-sync` files, dot-source | [operator.md, The test seam](operator.md#the-test-seam) | #0156 |
| `conformance/operator/test-unmet-host-proxy-lookup.ps1` | The ProxyCommand's host by every lookup: no argument, `agentspace`, a host alias in each state and one for a project with no record or a name that is not normalized, a lease alias of a `ready` host and of one that is not, an alias no lease has, no lease state file and one that cannot be read or is not valid, the lease state file never written, and a host registry that is not valid | [operator.md, From the ProxyCommand](operator.md#from-the-proxycommand), [The ProxyCommand](operator.md#the-proxycommand); [state.md, Which host a connection reaches](state.md#which-host-a-connection-reaches) | #0156 (E0.07-T04-S02) |
| `conformance/operator/test-unmet-host-proxy-instance-id.ps1` | A missing `instance_id` read from the host's own stack and written into that record alone; a record's ID never changed; a missing registry created holding the box's record with its ID; no record ever added; a stack with no `InstanceId` output or none at all; a write that fails not failing the connection | [operator.md, The host registry, as the operator uses it](operator.md#the-host-registry-as-the-operator-uses-it) | #0156 |
| `conformance/operator/test-unmet-host-proxy-stamp.ps1` | The push stamp per host, the box's where it was; each host pushed once per change; `up -Host <project> -Alias <alias>` and `up -Host swimcoach -Alias <alias>` | [operator.md, The stamp](operator.md#the-stamp); [state.md, A host's push stamp](state.md#a-hosts-push-stamp) | #0156 |
| `conformance/operator/test-unmet-host-alias-file.ps1` | The operator's alias file: its bytes, written before the first ssh, the `Include` line, a file at its path that is not this host's refused, an own file replaced or left alone, a lease's `create` and `release` leaving it, and the operator never writing agentspace's `known_hosts` or the user's | [operator.md, How the operator reaches a host](operator.md#how-the-operator-reaches-a-host); [state.md, A host's operator alias](state.md#a-hosts-operator-alias), [Agentspace's host keys](state.md#agentspaces-host-keys) | #0156 |
| `conformance/operator/test-unmet-host-alias-no-proxy-program.ps1` | The operator's alias refused when `agentspace-ssh-proxy.exe` is not beside the operator, a `.cmd` forwarder not meeting the need, and the control with the program copied beside it | [operator.md, How the operator reaches a host](operator.md#how-the-operator-reaches-a-host) | #0156 |
| `conformance/operator/test-unmet-host-alias-certificate.ps1` | The operator's certificate read with `ssh-keygen -L` against the throwaway CA: its key, signer, key ID, principal, extensions and validity, the key made once, and no output holding the private key | [operator.md, How the operator reaches a host](operator.md#how-the-operator-reaches-a-host); [state.md, The operator's key and certificate](state.md#the-operators-key-and-certificate) | #0156 |
| `conformance/operator/host-proxy-common.ps1` | No case: the helper the six files above dot-source | [operator.md, The test seam](operator.md#the-test-seam) | #0156 |
| `conformance/keeper/test-unmet-hosts-pass.ps1` | H2 to H8, H12 to H14, H16, H18, and X1: each host's holder writes its own `board-holder-ssh.<project>.log`. H12's variants hold the registry's two read failures (`Windows error <n>.` for a folder at its path, `the host registry is not valid` for a file that fails state.md's rules) | [keeper.md, Conformance cases for a host per project](keeper.md#conformance-cases-for-a-host-per-project), [Hosts](keeper.md#hosts), [The hosts of a pass](keeper.md#the-hosts-of-a-pass), [The holder's log](keeper.md#the-holders-log); [state.md, Another host's holder](state.md#another-hosts-holder) | #0157 (E0.07-T04-S03) |
| `conformance/keeper/test-unmet-hosts-watch.ps1` | H9, H10, H11 (both halves, `H11a` and `H11b`), H15, H17 | [keeper.md, Conformance cases for a host per project](keeper.md#conformance-cases-for-a-host-per-project), [Watching](keeper.md#watching), [The keeper's wait](keeper.md#the-keepers-wait), [Release grace](keeper.md#release-grace) | #0157 |
| `conformance/keeper/keeper-hosts-common.ps1` | No case: the helper the two files above dot-source after `keeper-common.ps1`, with the registry's writes and the stand-in `ssh-hosts-stub.cmd` | [keeper.md, Conformance cases for a host per project](keeper.md#conformance-cases-for-a-host-per-project) | #0157 |
| `test-keeper.ps1`, `test-holder-environment.ps1`, `test-unmet-holder-dies-mid-pass.ps1` and `test-unmet-launch-request-full-paths.ps1`, in their registry mode (`-Registry None\|Box\|Both`, `Both` by default), with `keeper-common.ps1`'s four new helpers | H1, as each file's closing `keeper.md#log-lines` result in both modes ([A host per project: the keeper's v0.07 cases](#a-host-per-project-the-keepers-v007-cases)) | [keeper.md, Conformance cases for a host per project](keeper.md#conformance-cases-for-a-host-per-project), [Log lines](keeper.md#log-lines) | #0157 |
| `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1`, changed | No case of its own: its removal of `board-holder.json` between two passes tolerates the file's absence, so every removal in `conformance/keeper/` of a file that may be absent does | As before ([The keeper suite](#the-keeper-suite)) | #0138, carried by #0157 |
| `conformance/schemas/test-state-schemas.ps1` | Every `schemas/state/*.schema.json` with samples: valid accepted, `invalid/` rejected for a schema reason, draft 2020-12, the state form of `$id`, a title and a description. **From v0.10 (E0.10-T01-S03, #0213) a state schema with no sample fails unless it is on the file's list of the six known to have none, and the lease state schema's copy of the plan schema's `$defs` is checked word for word** ([The schemas check](#the-schemas-check)) | [state.md, The host registry](state.md#the-host-registry), [The project list](state.md#the-project-list), [The lease state file](state.md#the-lease-state-file) | #0157; #0213 |

- **`test-state-schemas.ps1` is not a `test-unmet-*` file**: it checks the package, so no build can
  fail it ([The schemas check](#the-schemas-check)).
- **No lease CLI file reaches `place_pending`**: only a provisioning run calls it, and a stand-in
  never does, so its per-lease results are the operator's `provision` cases and E0.07-T05-S01's
  live check of a first agentspace lease ([Coverage of the operator and the keeper](#coverage-of-the-operator-and-the-keeper)).

**v0.08's files (E0.08-T04, #0182 to #0184).** **RFC 2119 keywords again.** Each is a `test-unmet-*`
file for a MUST that v0.08 adds, and **the installed v0.07 programs, which are the suite's defaults
until E0.08-T05-S01 (#0186) installs v0.08, do not meet them**: each story MUST show its files
failing against them where v0.07 lacks the behavior, in a throwaway home, and passing against the
build of `main`. **A v0.08 build MUST pass every one**, and a `FAIL` against it is the build's
failure, as for every file above. Each case's result name says which spec case it is. **The results of
T04's new files are in their pull requests** (#126 for E0.08-T04-S01's files, #127 for S02's and #128
for S03's). **The run record** ([docs/runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md)) covers the
conformance files on `main` at `6e2fcbd`, the deadline case, the crates and the box tests, and this
page gives no count of v0.08's.

| File | Cases it holds | Spec section | Card |
|---|---|---|---|
| `conformance/operator/test-unmet-guard.ps1` | Each outcome of D56 and D57 through the AWS stand-in: an explained host left untouched; an unexplained host that is not yet due logged; a due host stopped, with the exact calls; a host whose `provision.lock` is held left alone; a horizon sent to each running host with its comment, and the fallback to the launch time; a held `guard.lock` giving exit 0, no call and no log line; a failed describe logged and nothing stopped; every `guard.log` line byte for byte. Also the decision over eight hosts with each lease state and end (the box among them), a host that is not running, a running host with no usable `LaunchTime`, an instance missing from the answer, a record with no instance ID read from its stack, the host registry or the lease state file missing or not valid, a failed stop and a failed send (the next run sending again), #0034's sign-in message at each call in turn, `guard.log`'s bytes and a log that cannot be written, `-Quiet`, and what the guard never does | [operator.md, guard](operator.md#guard); [state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon), [The guard's log](state.md#the-guards-log), [The guard's lock](state.md#the-guards-lock) | #0182 (E0.08-T04-S01) |
| `conformance/operator/host-guard-common.ps1` | No case: the helper `test-unmet-guard.ps1` dot-sources | [operator.md, The test seam](operator.md#the-test-seam) | #0182 |
| `conformance/operator/test-unmet-status-leases.ps1` | `status -All`'s `Leases` and `Guard` lines: the count of live leases per host's project (pending and unexpired ready live; expired, released and unsatisfiable not), the `Guard` line's text, place and time (from the launch time, a lease's end or the report's own minute, rounded up to the next whole minute, in the laptop's zone), no line for a host a live lease explains or that is not running, a lease state file missing, unreadable or not valid, a `LaunchTime` that is missing or malformed, `AGENTSPACE_STATE_DIR`, a held `leases.lock`, `-Quiet`, and `status` and `status -Host` printing their five lines alone | [operator.md, status -All](operator.md#status--all); D60 | #0182 |
| `conformance/operator/test-unmet-grow-zero.ps1` | `grow -GiB 0`, `-0`, `+0`, `000`, `-1`, `-5`, `-007` and `-2147483648` each exit 1 as a usage error, with no AWS call, no `ec2 modify-volume` among them, nothing written in the home, and with `-Host` no key, certificate or alias; `-GiB 1` asking EC2 for the size plus 1; the refusal's place in the order of checks; `status`, `down` and `sync` not refusing a `-GiB` of 0 or less. It stands beside `test-unmet-commands.ps1`'s `-GiB 0` and `-GiB -1` cases, and shows `-GiB 0` with `USERPROFILE` unset not at all, which is the operator crate's test | [operator.md, grow](operator.md#grow); D61 (#0164) | #0182 |
| `conformance/operator/test-unmet-push-nested.ps1` | #0167: a ProxyCommand connection whose push is due printing one `The host is ready.`, one push, one nested run and ssh's own line once; a current stamp giving no push through the ProxyCommand and through `up`, with a stopped host still started; `AGENTSPACE_DIAGNOSTIC` set giving two `The host is ready.` lines, and set and empty one; a push that fails still showing its ssh's error lines in order and once each; a person's `up` whose push is due printing the nested line; another host reached by its alias, pushed with that host's stamp; and the push's ssh carrying `AGENTSPACE_NESTED` | [operator.md, The stamp](operator.md#the-stamp), [What the laptop sends](operator.md#what-the-laptop-sends), [up](operator.md#up); D61 (P14) | #0182 |
| `conformance/operator/test-unmet-sync-patch-wait.ps1` | `sync`'s calls after the install row, in order, with its messages; a reboot in the middle of the wait; a host already patched; the answers that keep the wait going (`unknown`, an empty and an unexpected one, an invocation that failed, 20 polls); a failed patch run, which warns while `sync` goes on, after `pending` too and for `sync -All`, `provision` and `-Quiet` (P21); a host launched less than 20 minutes ago; `up` not waiting on a failed run (P24.2); `sync` making no wait when its install fails; the wait's calls in `provision` and `deploy -Host`; #0034's sign-in failure; and, **only when asked for**, the 30-minute bound (P13) for `sync` (`-Case deadline`) and for a provisioning run (`-Case deadline-provision`), each about 32 minutes ([Cases that run only when asked for](#cases-that-run-only-when-asked-for)). A queued start (P24.3) is shown only as what the question `sync` sends reads (the marker, the unit's state, a queued start job and whether the unit is enabled): what a real host answers for a `failed` unit with a start queued is E0.08-T05-S01's live check | [operator.md, sync](operator.md#sync), [The patch wait](operator.md#the-patch-wait), [Starting the box and waiting for it](operator.md#starting-the-box-and-waiting-for-it); D61 (#0163) | #0182 |
| `conformance/lease-cli/test-unmet-release-ends-connections.ps1` | R1 to R15, each a case, through stand-in clients built with `csc.exe` in the throwaway home: `release` ending the alias's `ssh.exe` and `agentspace-ssh-proxy.exe` clients with their trees (R1, R3, R5, R6) and none that is not the alias's (R4); the alias file still there until the clients are ended (R2); a `release` that is itself a stand-in's child sparing that stand-in (R7, the one case that starts the program under test outside the harness: [The harness](#the-harness)); a released lease released again (R8); a `pending` lease that carries a driver (R9); an alias file that belongs to another lease (R10); `release.log` a folder (R11); no client, and an `unsatisfiable` lease (R12); ten clients within 15 seconds (R13); an expired lease (R14); and a client that comes back, counted at least 2 with no timing assumed (R15, which holds only for a second client started within the release's run, a later one being `SKIPPED`; the file names no gap). Each result name starts with the driver.md anchor the row cites and the row's number. `SKIPPED` when no `csc.exe` of the .NET Framework is found | [driver.md, Conformance cases for v0.08](driver.md#conformance-cases-for-v008) (R1 to R15), [Ending a lease's connections](driver.md#ending-a-leases-connections); [state.md, The release log](state.md#the-release-log); [lease-api.md, `release`](lease-api.md#agentspace-lease-release) | #0183 (E0.08-T04-S02) |
| `conformance/keeper/test-unmet-keeper-guard.ps1` | G1 to G16, each with a result line named `keeper.md#<section> (G<n>): ...`: `AGENTSPACE_GUARD_CMD`'s stand-in started at the first pass with the one argument `guard` (G1, G9, G14); not started again within 300 seconds, and started again after a short `-GuardIntervalSeconds`, also from inside a wait (G2 to G4); never started while `guard.lock` is held (G5); a folder at `guard.lock`'s path logged as a failed start (G6); no `.agentspace` folder, which the keeper does not create (G7); a stand-in that cannot be started logged, with every holder unaffected (G8); the guard's environment by variable name (G10); a stand-in that outlives the keeper (G11); a one-pass run (G12); `board-holder.lock` held (G13); the two usage errors (G15); and a second keeper (G16). The file's "Gaps:" paragraph names what no stand-in shows ([The guard's start: the keeper's v0.08 cases](#the-guards-start-the-keepers-v008-cases)) | [keeper.md, Conformance cases for the guard's start](keeper.md#conformance-cases-for-the-guards-start) (G1 to G16), [Starting the guard](keeper.md#starting-the-guard) | #0184 (E0.08-T04-S03) |
| `box/tests/test_agentspace_idle.py` | Not a `test-unmet-*` file and not run by the harness: the idle check's own test, D11's rule and D57's horizon, through the idle check's three seams ([The idle check's test](#the-idle-checks-test)) | [operator.md, The idle check and the lease horizon](operator.md#the-idle-check-and-the-lease-horizon) | #0172 (E0.08-T01-S05) |
| `test-unmet-usage-help.ps1`, `test-unmet-host-commands-line.ps1` and `host-commands-common.ps1`, changed | No case of their own to list here: the first gained `guard`'s help block and `guard` in its command lists; the second `guard`'s refusals of `-Host`, `-All`, `-Alias` and `-InstanceType`, and `-GiB` checked for form and ignored; the third `Compare-Text` masking the spec line it quotes (architect, 2026-11-22) | [operator.md, Command line](operator.md#command-line), [guard](operator.md#guard) | #0182 |
| `test-unmet-commands.ps1`, `test-unmet-host-commands-all.ps1`, `test-unmet-host-provision.ps1`, `test-unmet-start-paths.ps1`, `test-unmet-push.ps1` and `helpers.ps1`, not changed by T04 | No new case: operator.md's table names the first four, and the first three were already at v0.08's text on `main` (E0.08-T03-S02, PR #125, for the first two; E0.08-T03-S05, PR #123, for `test-unmet-host-provision.ps1`); `test-unmet-commands.ps1` holds the `-GiB 0` and `-GiB -1` cases, and `test-unmet-start-paths.ps1` is the model of the on-request deadline cases. The table does not name `test-unmet-push.ps1` or `helpers.ps1`, and #0167's push is shown by the new `test-unmet-push-nested.ps1` | [operator.md, Which checks show it](operator.md#which-checks-show-it) | #0182 |

- **Every keeper file names a stand-in guard** (`AGENTSPACE_GUARD_CMD`), and a case reaches no real
  AWS CLI, `ssh` or `scp` ([The harness](#the-harness), the bullet for v0.08).
- **`test-unmet-sync-patch-wait.ps1` and the real 30 minutes.** [operator.md's
  table](operator.md#which-checks-show-it) puts a wait of the real 30 minutes, about 32, in a run
  asked for by name, as `test-unmet-start-paths.ps1`'s `deadline` is ([Cases that run only when
  asked for](#cases-that-run-only-when-asked-for)). **The file holds two such runs**, `-Case
  deadline` for `sync` and `-Case deadline-provision` for a provisioning run, and a default run
  neither runs nor reports them. **Its header names what they leave unshown:** `deploy`'s step 6
  when its wait runs out, since that step is `sync` and a third 30-minute run would show nothing
  more; the deadline's failure under `-Quiet`, which always prints; and the exact 30 minutes, which
  is checked to fall from 29 to 32 minutes after the start of the run.
- **No v0.08 case shows a host stopping**, a real Run Command or a real session, and the suite has
  no fake clock, so the grace's timing is a live check ([Coverage of the operator and the
  keeper](#coverage-of-the-operator-and-the-keeper), v0.08's rows).

**v0.09's files (E0.09-T04, #0205 and #0206, and one helper by #0207's pin P56).** **RFC 2119 keywords again.** Each is a
`test-unmet-*` file for a MUST that v0.09 adds, and **the installed v0.08 programs, which are the
suite's defaults until E0.09-T05-S01 (#0208) installs v0.09, do not meet them** where v0.08 lacks the
behavior: v0.08 answers every size but `m` `unsupported_size`, and its operator refuses
`-InstanceType` for the box. **A v0.09 build MUST pass every one**, and a `FAIL` against it is the
build's failure, as for every file above. Each case's result name says which spec case it is (`Z5`,
or the row of [operator.md, Which checks show it](operator.md#which-checks-show-it) in a few words).
**The results of T04's new files are in their pull requests; the run record**
([docs/runs/e0.09-t04-s03.md](runs/e0.09-t04-s03.md)) **covers the whole suite and the crates on
`main`**, in two parts by pin P55, with one operator file counted at this story's commit by pin P56,
**and cites v0.08's run of the box tests for the unchanged `box/`**; this page gives no count of
v0.09's.

| File | Cases it holds | Spec section | Card |
|---|---|---|---|
| `conformance/lease-cli/test-unmet-sizes-place.ps1` | Z1, Z2, Z3, Z4, Z6, Z7, Z11 and Z13: each size met at its type; `xxl` and `XXL` `unsupported_size`, with D71's message, beside `unsupported_platform` in the contract's order; a larger host meeting a smaller size while a live `ready` lease holds it; `no_capacity` with D66's message byte for byte, changing nothing; an expired, `released` or `unsatisfiable` lease and another project's not holding the host; expiry by the reading's time; no CA; another state folder's lease not holding it. And L1 and L2, two statements of lease-api.md's v0.09 text that no Z row covers, each a case of its own: a lease `unsatisfiable` with `no_capacity` staying so, with a later `create` judged afresh (Fields); and a size normalized, in any case and with surrounding whitespace, before it is judged (Size) | [driver.md, Conformance cases for v0.09](driver.md#conformance-cases-for-v009), [On a `ready` host](driver.md#on-a-ready-host), [What it supports](driver.md#what-it-supports); [lease-api.md, Fields](lease-api.md#fields), [Size](lease-api.md#size) | #0205 (E0.09-T04-S01) |
| `conformance/lease-cli/test-unmet-sizes-pending.ps1` | Z5, Z8, Z9, Z10, Z12, Z14, Z15 and Z16: `pending` and a run of exactly `provision -Host <project>` for a host to be made larger or smaller; a record with no `instance_type`, or one the map does not hold, answering `pending` whatever holds the host; place's side of a run's deploy step under a held `provision.lock`; the 5-minute rule and `get`'s start; the box at `m` and at `l`; a `pending` lease holding every new lease of its project `pending`, and one outside `SIZES` or of another driver not; the box's type implied by a missing `hosts.json` and never written | [driver.md, Conformance cases for v0.09](driver.md#conformance-cases-for-v009), [On a `ready` host](driver.md#on-a-ready-host), [Starting a provisioning run](driver.md#starting-a-provisioning-run), [Where it places a lease](driver.md#where-it-places-a-lease); [state.md, The host registry](state.md#the-host-registry) | #0205 |
| `conformance/lease-cli/provision-standin.ps1` | No case: v0.07's helper, which the two files above dot-source for their recording stand-in run (pin P12) | [driver.md, Conformance cases for v0.09](driver.md#conformance-cases-for-v009) | #0155, used by #0205 |
| `conformance/operator/test-unmet-host-deploy-size.ps1` | Every `deploy` row of v0.09's part of operator.md's table: the command line, with `-InstanceType` for the box, `-Host swimcoach` and `-Host <project>`; the box's arguments, every member of `stack-parameters.json` but `InstanceType`; `deploy -Host` with and without `-InstanceType`, shown by v0.07's cases in `test-unmet-host-deploy.ps1`, kept passing, and not by this file; the record of the type read back; a failure after a successful `cloudformation deploy`; step 4's removal, and step 4 with no `hosts.json`; the deploy refused while `provision.lock` is held, and the lock held for its life; and a plain `deploy` of the box not changing its type | [operator.md, deploy](operator.md#deploy), [Command line](operator.md#command-line), [Which checks show it](operator.md#which-checks-show-it); [state.md, The instance type](state.md#the-instance-type) | #0206 (E0.09-T04-S02) |
| `conformance/operator/test-unmet-host-provision-size.ps1` | Every `provision` row of v0.09's part of operator.md's table: the lease state lock held, and one that cannot be taken; the type against a seeded lease state file, with `leases.lock` and `hosts.lock` both free when the stand-in `aws` is first called; the lease state file not valid; the log lines, `deploy done type=<type>` among them, and a resize on a `ready` host running every step; and step 9's `place done`, `place failed`, `place refused` and `place left` lines | [operator.md, provision](operator.md#provision), [Which checks show it](operator.md#which-checks-show-it); [driver.md, A resizing run](driver.md#a-resizing-run), [place_pending](driver.md#place_pending); [state.md, The lease state lock](state.md#the-lease-state-lock), [The provisioning run's log](state.md#the-provisioning-runs-log) | #0206 |
| `conformance/operator/test-unmet-host-provision-failures.ps1`, changed | A case with a typed seed: a failed step 4.2 removing the `instance_type` the record held | [operator.md, provision](operator.md#provision), step 4 | #0206 (#0204's review's N1) |
| `conformance/operator/host-commands-common.ps1` and `host-provision-common.ps1` | No case of their own: the helpers the two files above dot-source, each with E0.09-T03-S02's typed seeds and stand-in stacks ([The operator and the token push](#the-operator-and-the-token-push)) | [operator.md, The test seam](operator.md#the-test-seam) | #0156; #0204 |
| `conformance/operator/host-proxy-common.ps1`, changed in T04 | No case of its own: `New-HpHostRecord` seeds a `ready` record with `instance_type` `m9g.xlarge`, or none with `-NoInstanceType`, for the seven files that dot-source it, among them `test-unmet-host-alias-file.ps1`, whose lease `create` on its seeded host answered `pending` before the change ([The operator and the token push](#the-operator-and-the-token-push)) | [driver.md, On a `ready` host](driver.md#on-a-ready-host), row 2 | #0207 (E0.09-T04-S03, pin P56) |
| `harness.ps1`'s `Add-ConformanceReadyHost` and `test-unmet-host-alias-bytes.ps1`'s `New-PgHostRecord`, changed | No case of their own: each `ready` record seeded with `instance_type` `m9g.xlarge`, or none with `-NoInstanceType` (pins P3, P29); and C20 rewritten to Z14's form | [driver.md, Conformance cases for v0.09](driver.md#conformance-cases-for-v009), "Cases of earlier releases that change with v0.09" | #0203 (E0.09-T03-S01, PR #140) |
| The eight lease CLI files of E0.09-T01-S03's sweep, changed | No case of their own: every unsupported size moved from `l` or `xl` to `xxl`, or `XXL` ([Running the suite](#running-the-suite), "What v0.09 changes") | [lease-api.md, Size](lease-api.md#size); D71 | #0199 (E0.09-T01-S03, PR #133) |
| `test-unmet-usage-help.ps1` and `test-unmet-host-commands-line.ps1`, changed | No case of their own to list here: the help copies at v0.09's text, and check 6 removed | [operator.md, Command line](operator.md#command-line) | #0202 (E0.09-T02-S03, PR #137) |
| `test-unmet-host-deploy.ps1`, `test-unmet-host-deploy-registry.ps1`, `test-unmet-host-provision.ps1`, `test-unmet-host-commands.ps1` and `test-unmet-host-commands-all.ps1`, changed | No case of their own: their expected arguments and records at v0.09's ([The operator and the token push](#the-operator-and-the-token-push)) | [operator.md, deploy](operator.md#deploy), [provision](operator.md#provision) | #0204 (E0.09-T03-S02, PR #142) |
| `conformance/schemas/test-state-schemas.ps1`, unchanged | No case of its own: it checks the host registry schema's `instance_type` through the valid and invalid samples E0.09-T01-S02 added (PR #135), as it checks every state schema with samples | [state.md, The host registry](state.md#the-host-registry), [The instance type](state.md#the-instance-type) | #0198 (E0.09-T01-S02, PR #135) |

- **No lease CLI file shows a host's type change**: a stand-in run makes no host, and place reads
  only the records a case writes. **What no stand-in shows**, by driver.md's paragraph below its
  v0.09 table, is shown instead by:
  - **`SIZES`, `instance_type_for`, `meets` and `resize_target` over their whole domain:**
    `agentspace-driver-ec2`'s own tests, E0.09-T02-S02 (#0201).
  - **Place's decision row by row over stand-in files, `place_pending`'s two passes, its step 3.4,
    and place's `internal` answer for a lease state file lost under its caller's hold:** the same
    crate's tests, E0.09-T03-S01 (#0203).
  - **Pin P26's order against a `create` that runs at the same time:** `agentspace-sync`'s own
    tests of provision's deploy step, E0.09-T03-S02 (#0204).
  - **A run's `place_pending` through `provision`, the type a run's deploy step passes, and pin
    P26's order as the stack's deploy sees it:** the operator's cases above, #0206.
  - **A host's type changing, and everything that needs a real host:** D70's live checks,
    E0.09-T05-S01, #0208 ([Coverage of the operator and the keeper](#coverage-of-the-operator-and-the-keeper),
    v0.09's rows).

**v0.10's files (E0.10-T04, #0221 and #0222).** **RFC 2119 keywords again.** Each is a conformance file
for a MUST that v0.10 adds: four lease CLI files for contract v1 (`test-plan-sizes.ps1`,
`test-plan-sized-for.ps1`, `test-plan-errors.ps1` and `test-v1-compatible.ps1`) and three
`test-unmet-*` files (the lease CLI's D77 file and S02's two operator files), and **the installed v0.09 programs, which are the suite's defaults until
E0.10-T05 (#0224) installs v0.10, do not meet them** where v0.09 lacks the behavior: v0.09 has no
`--plan`, no `sized_for`, no `plan_too_large`, no `last_success_at`, and a `bootstrap` that answers an
`ssh` file that is not a program with `Could not run` and not with the seam's line. **A v0.10 build MUST pass every one**, and a `FAIL` against it is the build's failure, as
for every file above. Each case's result name starts with the anchor of the spec section it enforces
(`lease-api.md#plan:`, `lease-api.md#fields:`, `driver.md#starting-a-provisioning-run: Y1 ...`,
`operator.md#provision:`, `operator.md#the-test-seam:`). **The results of T04's new files are in their
pull requests (#159 for E0.10-T04-S01's, #160 for E0.10-T04-S02's, #0222); the run record**
([docs/runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md)) **covers the whole suite and the crates, and the
box's tests with the whole discovery command**. This page gives each new file's number of results and
its results against the installed v0.09 programs and the empty program (below), and of the whole run
only the box's 172 ([The idle check's test](#the-idle-checks-test)).

| File | Cases it holds | Spec section | Card |
|---|---|---|---|
| `conformance/lease-cli/test-plan-sizes.ps1` | The size a plan picks: each size's edges (2 and 4 agents at once `s`, 5 and 8 `m`, 9 and 16 `l`, 17 and 32 `xl`, the dev lead counted), made ready on a record at the size's type; the same eight counts with a platform no backend has, the sizes picked being four different ones; a tie between two busiest turns (the first wins, its name in `sized_for`), a tie with a lesser turn between, the busiest turn last and in the middle, turns not summed, several stories summed with one dev lead for the turn; the peak counted from `after` as the width of the graph (a chain, a root and its dependants, a dependency through another subagent, a diamond, two chains, an `after` naming a subagent listed later, and the three forms that mean none: absent, `[]` and `null`), each giving another size than the count of subagents or of direct `after` entries would; a subagent's model and effort changing nothing | [lease-api.md, Plan](lease-api.md#plan), [`create`](lease-api.md#agentspace-lease-create); D73 to D76 | #0221 (E0.10-T04-S01) |
| `conformance/lease-cli/test-plan-sized-for.ps1` | `sized_for` on a ready, expired and released plan lease through `create`, `get`, `list`, `renew` and `release` (the expired one reached by moving `expires_at` into the past, as the suite does), on a `pending` one (a host not at the size, no live lease, row 6) and on an `unsatisfiable` one (`no_capacity` by a live `ready` lease on a smaller host, row 5, and `unsupported_platform`), each read back unchanged; a lease with no plan beside each, with none; a plan with no name or a null one giving a `sized_for` with no `plan`; names with spaces, quotes, a backslash, mixed case and the longest form of 64 characters printed as the plan writes them; and no output printing a story, a subagent, a model or an effort | [lease-api.md, Fields](lease-api.md#fields), [Output](lease-api.md#output); D75, D76 | #0221 |
| `conformance/lease-cli/test-plan-errors.ps1` | `--plan` with `--size` as a `usage` error with `field` `plan`, in each order and each form, the later flag standing, and its rank among the other usage errors; the order the values are checked in (project, platform, profile, plan, size, ttl), with help winning over everything; every refusal of a plan as `invalid_value` for `plan`, exit 2, with the frame `Plan '<value>' is not a valid plan: <why>.` and the `<why>` of the first check the plan fails, in the table's order (the size bound of 262144 bytes with a byte order mark counted; a file that cannot be read: missing, a folder, a device, a pipe, locked, `-`, an argument with U+FEFF, a lone surrogate or a control character; UTF-8; JSON; a member named twice; the plan schema; a turn, a story or a subagent named twice; an `after` naming no subagent of its story; a cycle); a refused plan recording no lease and creating nothing, and no message carrying an operating system's error text, a path but the argument, or a value of the document; and `plan_too_large`: 32 agents at once fit and 33 do not, by stories, by subagents and by the width of `after`, the busiest turn and not the sum, the first on a tie, its only reason whatever the platform and the profile, the message byte for byte, `size` `xl` and `sized_for` kept through `get`, `list` and `release`, decided before any driver is asked, so with no CA, no host and no run | [lease-api.md, Plan](lease-api.md#plan), [Fields](lease-api.md#fields), [`create`](lease-api.md#agentspace-lease-create), [Command lines](lease-api.md#command-lines), [Exit codes and errors](lease-api.md#exit-codes-and-errors); D74, D75, D81 | #0221 |
| `conformance/lease-cli/test-v1-compatible.ps1` | D76's check: every document of a command line with no `--plan`, a lease in each of 5 states (`ready`, `pending`, `expired`, `released`, `unsatisfiable`), each of 4 reasons, a list, and an error of each of 6 codes, in at least 80 documents, valid against `schemas/v0/` as well as against v1's, and none holding `sized_for`, the reason `plan_too_large` or the field `plan`; a state file that cannot be read giving `internal`; 4 controls that pass v1's schemas and fail v0's (a lease made with `--plan`, a `plan_too_large` lease, `--plan` with `--size` and a plan file that does not exist, the last two naming the field `plan`), so that a validator accepting everything cannot pass; and a result that every state, reason and code was seen | [lease-api.md, Versioning](lease-api.md#versioning), "What v1 keeps of v0", [Schemas](lease-api.md#schemas); D76, D80 | #0221 |
| `conformance/lease-cli/test-unmet-pending-due-after-success.ps1` | Y1 to Y5, driver.md's cases for D77, **19 results**: **Y1** (2): a success at or after `last_attempt_at` makes the next start at once, by a `get` of a stored `pending` lease and by a `create --size l` in a case of its own, each starting the stand-in once with exactly `provision -Host <project>`, setting `last_attempt_at` to now and leaving no `last_success_at`; **Y2** (8): `last_success_at` older than `last_attempt_at`, or absent, waits 5 minutes, for a `get` and a `create --size l` each, and starts once at 5 minutes or more; **Y3** (1): equal seconds, a `get` starts once and removes `last_success_at` in the write that sets `last_attempt_at`, and a second `get` starts nothing and leaves the registry as that write left it (pin P24); **Y4** (4): a record as a run whose placing failed or left a lease leaves it, with no `last_success_at`, answers as Y2's absent case and the next start comes at 5 minutes (pin P22); **Y5** (4): a held `provision.lock` stops a start that a success would make due, for a `get` and a `create --size l`. **Every `create` of Y1, Y2, Y4 and Y5 is made on the whole seed, the stored `pending` lease included, so it reaches row 1.** Each case seeds `last_success_at` itself, the lease CLI's stand-in provision writing nothing | [driver.md, Conformance cases for v0.10](driver.md#conformance-cases-for-v010), [Starting a provisioning run](driver.md#starting-a-provisioning-run); [state.md, The last success](state.md#the-last-success); D77 | #0221 |
| `conformance/operator/test-unmet-host-provision-success.ps1` | Rows 1 to 6 of operator.md's v0.10 table, **30 results** in the groups its names give: **step 3** (5): a `creating`, a `failed`, a `ready` and the box's `ready` record that held `last_success_at`, and a new record, each with `last_attempt_at` set to the run's start, no `last_success_at` and every other member as it was, in one write; **success** (5): a run that placed every pending lease writes `last_success_at` after `place done` and before `run done`, at or after `last_attempt_at`, with no other member changed, and a lease answered `unsatisfiable` beside a placed one (`place refused`) is not a failure; **not succeeded** (3): placing failed as a whole, a lease failed (a foreign file at its alias) and a lease left (a size the host does not meet), each exit 0 with its own `place` line and no `last_success_at`; **failed step** (6): steps 4, 5, 6 and 7 failing, for a `ready` host and one that was not, exit 1 with none; **step 8** (2): a registry write that fails, and `hosts.lock` held past its 30 seconds, each ending with none; **success write fails** (8): `hosts.lock` held past its 30 seconds, `hosts.json` that cannot be written, one that has become not valid, and the record gone, each giving `success failed reason=<why>` after step 9's lines and before `run done`, exit 0, no warning and no change to the file, in 2 results each; and **success write lock** (1): the write takes `hosts.lock` alone, with `provision.lock` held and the lease state lock free | [operator.md, provision](operator.md#provision), [Which checks show it](operator.md#which-checks-show-it); [state.md, The last success](state.md#the-last-success); [driver.md, Lock order](driver.md#lock-order); D77 | #0222 (E0.10-T04-S02) |
| `conformance/operator/test-unmet-seam-cannot-run.ps1` | **4 results**: `bootstrap` with `AGENTSPACE_SSH_CMD` naming a file that exists and is not a program (defect #0227), one that does not exist, and a folder, and with `AGENTSPACE_AWS_CMD` naming a file that is not a program, each ending `agentspace: <variable> names <value>, which cannot be run.` as the last line of stderr with exit 1, with no `Could not run` line, nothing on stdout and no stand-in `ssh` or `scp` started | [operator.md, The test seam](operator.md#the-test-seam), "What each one does" | #0222 |
| `test-usage-errors.ps1` and `test-values.ps1`, changed | Comments only: the first's header says exactly six `field` cases MUST carry it and that the sixth, `--plan` with `--size`, and its ranking are in `test-plan-errors.ps1`; the second's, the order of the values with `plan` | [lease-api.md, Exit codes and errors](lease-api.md#exit-codes-and-errors) | #0221 |

- **Against the installed v0.09 programs, run with the four variables unset** (PR #159's table for S01's
  files, and S02's author runs for S02's; each file also passes every result against `main`'s build):
  - **The D77 file passes 11 of its 19 and fails 8: the cases that seed `last_success_at`**, which v0.09's
    schema refuses, so each `create` that seeds one fails there and each case whose start D77 makes
    due starts nothing. **It is never "every result" that fails**: a `get` that seeds one and expects no
    start passes there, since `check_pending` drops a registry it cannot read, and so do the cases that
    seed none (Y2's absent case and Y4), whose 5 minutes v0.09 already had.
  - **`test-plan-sizes.ps1` passes none of its 38; `test-plan-sized-for.ps1` passes 17 of 50** (the
    results about a lease with no plan); **`test-plan-errors.ps1` passes 12 of 298; and
    `test-v1-compatible.ps1` passes 90 of 94**, failing only its 4 controls, since a command line with no
    plan prints v0's document by design.
  - **`test-unmet-host-provision-success.ps1` passes 1 of 30** (a new record added with
    `last_attempt_at` and no `last_success_at`, which v0.09 also does) **and `test-unmet-seam-cannot-run.ps1`
    passes 3 of 4**, failing the case of #0227: v0.09's `bootstrap` answers an ssh file that exists and
    is not a program with `Could not run`, and already answers the other three cases with the seam's
    line.
  - **Against the empty program no result of a file passes**: PR #159's table gives 0 passed and 38,
    50, 298, 84 and 19 failed for S01's `test-plan-sizes.ps1`, `test-plan-sized-for.ps1`,
    `test-plan-errors.ps1`, `test-v1-compatible.ps1` and the D77 file, and S02's author's run of the two
    operator files against an empty operator printed 0 passed and 23 failed for the success file, and 0
    passed and 4 failed for the seam file. The whole-suite run's empty-program step
    MUST show each lease CLI file exiting 1 ([Running the suite against a build's output
    folder](#running-the-suite-against-a-builds-output-folder), step 5).
- **A file's results in its pull request are not the build's whole-suite result**: the run record runs
  each against the build, at the commit its sections name, and shows by a diff that the files at the
  final `main` are those it ran ([docs/runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md#the-commits)).

**v0.11's files (E0.11-T04, #0239 to #0241).** **RFC 2119 keywords again.** Each is a conformance file for a
MUST that v0.11 adds, or a helper: three lease CLI files and one changed for contract v2, the stand-in board,
one operator file of T02's and S02's change to a helper. **The installed v0.10 programs, which are the suite's
defaults until E0.11-T05 (#0242) installs v0.11, do not meet them** where v0.10 lacks the behavior: v0.10 has no
`--board`, `--epic` or `--turn`, no `budget_minutes`, no `board`, `epic` or `turn` as an error's `field`, and a
ProxyCommand that reaches the box for a lease that is not `ready`. **A v0.11 build MUST pass every one**, and a
`FAIL` against it is the build's failure, as for every file above. **This page gives no result of these files
against the installed v0.10 programs, and no run of that kind was made for the record**; the empty-program run
MUST show each of the 46 lease CLI files exiting 1 ([Running the suite against a build's output
folder](#running-the-suite-against-a-builds-output-folder), step 5), and the run record
([docs/runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md)) gives the results against the build.

| File | Cases it holds | Spec section | Card |
|---|---|---|---|
| `conformance/lease-cli/board-standin.ps1` | **No case: the stand-in board**, a loopback server a case starts on a port it picks, one fixture, one log and one ready file; its interface is [The stand-in board and its credential](#the-stand-in-board-and-its-credential) and its calls where that is silent are in [The stand-in board](#the-stand-in-board) (pin P9). **It has a self-test of its own**, a planted-copy proof that its log holds no token, which is not committed | [lease-api.md, The board read](lease-api.md#the-board-read); D86, D88 | #0239 (E0.11-T04-S01) |
| `conformance/lease-cli/test-board-plan.ps1` | The seven items of [What the board cases MUST show](#what-the-board-cases-must-show) and pin P5's deep-nesting cases; **its reasons pass through one redaction function that hides the run's token, with a control result that shows it**, and a DPAPI preflight under the moved home that SKIPs every case that needs a token when it fails. **245 results** at S01's sent head, `1ef1085`, from [its row in the run record](runs/e0.11-t04-s03.md#the-lease-cli-suite-at-the-final-main) | [lease-api.md, The board read](lease-api.md#the-board-read), [The plan block](lease-api.md#the-plan-block), [Board](lease-api.md#board), [Epic](lease-api.md#epic), [Exit codes and errors](lease-api.md#exit-codes-and-errors); D83, D86 to D88 | #0239 |
| `conformance/lease-cli/test-plan-turn-ttl.ps1` | **381 results**: `--turn`, the TTL rules, `budget_minutes`, the thirteen usage cases that carry `field` and the order the values are checked in, each from a file and again from an epic's block on the stand-in; its reasons pass through `Format-TtText`, with a control result | [lease-api.md, Turn](lease-api.md#turn), [TTL](lease-api.md#ttl), [Plan](lease-api.md#plan), [Command lines](lease-api.md#command-lines); D83 to D87 | #0239 |
| `conformance/lease-cli/test-v2-compatible.ps1` | **151 results**: contract v2 keeping v1 and v0, with its controls ([The lease CLI's table](#running-the-suite)) | [lease-api.md, Versioning](lease-api.md#versioning), [Schemas](lease-api.md#schemas); D76, D87 | #0239 |
| `conformance/lease-cli/test-v1-compatible.ps1`, changed | **95 results**, one more than v0.10's 94: v1's schemas read from `schemas/v1/` for v1 and `schemas/v0/` for v0, and a fifth control (an error naming the field `turn` passes `schemas/` and fails `schemas/v1/` and `schemas/v0/`) | [lease-api.md, Versioning](lease-api.md#versioning); D76, D87 | #0239 |
| `conformance/operator/test-unmet-proxy-not-ready.ps1` | #0229: the alias of a `released`, `expired`, `unsatisfiable` or `pending` lease refused with no tunnel, no call and no push; an argument of lease alias form refused while the lease state file cannot be read or is not valid; a `ready` lease still connecting; an alias that is no record's, and a missing file, reaching the box; the retry on a sharing violation, bounded; nothing written by a refusal | [operator.md, From the ProxyCommand](operator.md#from-the-proxycommand), [A lease that is not ready (#0229)](operator.md#a-lease-that-is-not-ready-0229) | #0237 (E0.11-T02-S04) |
| `conformance/operator/test-unmet-host-proxy-lookup.ps1`, changed | The cases that #0237 changed: the lease alias whose lease has expired, a lease state file that is not JSON, has `version` 1, holds two records with one `id` or does not fit its schema, a folder at its path, and the `AGENTSPACE_DIAGNOSTIC` case, each refused from v0.11 | [operator.md, From the ProxyCommand](operator.md#from-the-proxycommand) | #0237 |
| `conformance/operator/host-provision-common.ps1` and `test-unmet-host-provision.ps1`, changed | No case of their own: `Get-HpLogShape` quotes an unparsed log line through `ConvertTo-HpMasked`, and the reason of `test-unmet-host-provision.ps1` that quotes a line does too | [The operator and the token push](#the-operator-and-the-token-push), N7 | #0240 (E0.11-T04-S02, PR #172) |

## The gaps

**A gap is a spec section that no sorted check covers.** The sorted checks leave these.

| Spec section | Gap |
|---|---|
| operator.md: The token push | **Covered by `conformance/operator/test-unmet-push.ps1`**, through the seam, and it fails against the prototype until a build meets the seam ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). What it leaves is in [Coverage](#coverage-of-the-operator-and-the-keeper) |
| operator.md: The time zone | The seven legacy names and the zone check are covered by `test-unmet-push.ps1`. The conversion of a Windows zone ID is not, and [Coverage](#coverage-of-the-operator-and-the-keeper) says why |
| operator.md: Commands (`deploy`, `up`, `sync`, `down`, `status`, `grow`), Starting the box and waiting for it, The box as the laptop sees it, The AWS side | `up` runs through the seam in `test-unmet-push.ps1` and `test-unmet-proxy-steps.ps1`, as far as the calls they stand in for go. **`down`, `status` and `grow` are covered by `test-unmet-commands.ps1`, and the start-and-wait step of `up` (the stopped and stopping boxes, the wait and its deadline, the patch question's answers) by `test-unmet-start-paths.ps1`** (E0.06-T04-S02, #0132), each failing against the prototype ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). `deploy` and `sync` have no check, and [Coverage](#coverage-of-the-operator-and-the-keeper) says why. **From v0.07, `-Host`, `-All`, `bootstrap` and `provision` have cases through the seam** (E0.07-T04-S02, #0156), and each command's real effect on a host is one of E0.07-T05-S01's live checks ([Coverage](#coverage-of-the-operator-and-the-keeper), v0.07's rows). **From v0.08, `guard`, `status -All`'s `Leases` and `Guard` lines, `grow`'s refusal and `sync`'s patch wait have cases** (E0.08-T04-S01, #0182: `test-unmet-guard.ps1`, `test-unmet-status-leases.ps1`, `test-unmet-grow-zero.ps1` and `test-unmet-sync-patch-wait.ps1`), and what no stand-in shows is one of D62's live checks ([Coverage](#coverage-of-the-operator-and-the-keeper), v0.08's rows). **From v0.09, `deploy` and `provision` by size have cases** (E0.09-T04-S02, #0206: `test-unmet-host-deploy-size.ps1` and `test-unmet-host-provision-size.ps1`), and a host's type change is one of D70's live checks ([Coverage](#coverage-of-the-operator-and-the-keeper), v0.09's rows) |
| operator.md: Command line | **The usage errors and the help texts are covered by `test-unmet-usage-help.ps1`** (E0.06-T04-S02, #0132), which fails against the prototype. **The rules for an unset or empty `USERPROFILE` are shown by the programs' own tests, not from outside**, because no check runs a program with it unset or empty ([The harness](#the-harness)); `SystemRoot`'s are checked from outside, with `USERPROFILE` in the throwaway home. An alias holding a line feed has no check: `cmd.exe` cuts the value at the line feed to a valid alias, so a run could get past the refusal and reach AWS. A check of it needs a way to start the operator that does not pass through `cmd.exe` |
| operator.md: The ProxyCommand | The argument refusals (`test-proxy-arguments.ps1`), the steps (`test-unmet-proxy-steps.ps1`) and what a caller sees (`test-unmet-proxy-connect-time.ps1`) are checked, the last two failing against the prototype ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). What is left is in [Coverage](#coverage-of-the-operator-and-the-keeper) |
| operator.md: `ca` | **Covered by `conformance/operator/test-ca.ps1`**, which passes against the prototype. What it leaves is in [Coverage](#coverage-of-the-operator-and-the-keeper) |
| lease-api.md: Values (Lease ID), Input tolerance | **Covered by `conformance/lease-cli/test-id-whole-value.ps1` and `test-unmet-id-line-feed.ps1`** (E0.06-T04-S03; #0029 point 2; #0110): `get`, `renew` and `release` of `<id>.`, `<id>..`, `<id> .` and `<id>.` with a line feed after it are `invalid_value` for `id` and change nothing, and a line feed inside the ID is refused. **A trailing line feed alone is not refused, and a line feed before the ID is not:** both are surrounding whitespace, which [Input tolerance](lease-api.md#input-tolerance) trims before the check, so the lease is found. The second file fails against the prototype, whose `.cmd` cuts the command line at the first line feed ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). Left: the whole-value rule of the other values is shown only by `test-values.ps1`'s cases |
| state.md: The home folder, Requirements the prototype does not meet yet | **Covered in part** by `test-home-userprofile.ps1` and `test-relative-folders.ps1` (E0.06-T04-S03; #0110 point 4), and by `conformance/keeper/test-unmet-launch-request-full-paths.ps1` (E0.06-T04-S03), which starts a keeper with a relative `AGENTSPACE_STATE_DIR` and requires the `log`, `directory` and `result` of the holder's launch request to be full paths. **Not seen from outside, each for its reason:** a relative `USERPROFILE`, and `USERPROFILE` unset or empty, because the harness refuses to give a program an override of it and a check does not work around the harness; and the operator's `deploy` and `sync` and the keeper task's launcher finding the home through `USERPROFILE`, because showing it needs the real `go`, `ssh-keygen` and Task Scheduler, which no check runs ([operator.md Q30](operator.md#open-questions)) |
| state.md: The CA (Its checks) | **Covered in part** by `test-ca-public-key-words.ps1` (E0.06-T04-S03; #0110 point 4). The prototype passes it, because it also refuses each changed key through the parse check that comes later, so no file here tells its case-insensitive comparison from the exact one. A tab or two spaces between the two words is asserted too, for `create` and `renew`, and so is a refusal of a line break (LF, CR, VT, FF, CRLF) between the words and of a leading U+00A0: the key is read as `ssh-keygen` reads it, trimmed of ASCII whitespace and split at spaces and tabs only, in one reading for checks 4 and 5 (architect, 2026-11-19, #0136, as corrected). The Rust build met it only after `check_ca_keys` was fixed in the same story |
| state.md: The instance-ID cache | **Covered in part**: `test-lease-ignores-instance-cache.ps1` (E0.06-T04-S03) shows the lease CLI never needs it: a cache that cannot be read (a folder where the file is, a file that is not JSON) changes no command's outcome, and neither does a valid one. Whether the CLI opens it is not seen. `conformance/operator/test-unmet-proxy-steps.ps1` and `test-unmet-push.ps1` seed it at `%USERPROFILE%\.agentspace\instance-id.json` and require the cached instance. **Left:** that the operator writes it there when it is missing, which needs a stand-in `aws` that answers `cloudformation describe-stacks` |
| lease-api.md: States | The expiry boundary (`expired` exactly at `expires_at`, `ready` one second before) needs a clock the check controls |
| lease-api.md: The credential | A short-ID collision, and a caller-chosen lease ID refused by the credential step, cannot be reached through the CLI. The requirements the prototype does not meet are in `test-unmet-*` files, not here ([The files that are expected to fail](#the-files-that-are-expected-to-fail)); the certificate's valid-after is a SHOULD and has none ([SHOULDs not checked](#shoulds-not-checked)). The key's access rules are not checked, because they are a Windows security descriptor ([lease-api.md Q12](lease-api.md#open-questions)) |
| lease-api.md: `renew`, States | **Checked:** a `renew` of a lease whose key pair is missing (both `id_ed25519` and `id_ed25519.pub`, so that no certificate can be issued, and the key MUST NOT change: [lease-api.md, `renew`](lease-api.md#agentspace-lease-renew)), of one whose folder is gone, and of one whose record cannot be written (the state file held open without delete sharing, which [state.md, Replacing a file](state.md#replacing-a-file) says makes a rename fail) each fail with `internal` and leave the record, the key, the alias and the config as they were, with no temporary file and no folder made. **Not checked, because the prototype does not meet it:** a failed `renew` MUST leave the previous certificate byte for byte (#0041 point 3). The prototype leaves the new one when the record cannot be written. That is a `test-unmet-*` requirement, held by `conformance/lease-cli/test-unmet-renew-keeps-certificate.ps1` |
| lease-api.md: `release`, States | **Not checked, because the prototype does not meet it:** a `release` that fails part way MUST NOT leave a lease `ready` with its alias, key or certificate gone (#0041 point 3). Holding the lease's key file open without delete sharing makes the prototype's `release` fail with `internal` after it has deleted the alias file, which leaves a `ready` record with no alias. That is a `test-unmet-*` requirement, held by `conformance/lease-cli/test-unmet-release-partial.ps1` |
| state.md: The lease state file, #0041 point 2 | **Not checked, because the prototype does not meet it:** a stored `ready` record with no `driver` MUST be renewed and released by the v0 default driver, and `renew` and `release` MUST NOT add the member. The prototype fails both with `internal`. That is a `test-unmet-*` requirement, held by `conformance/lease-cli/test-unmet-record-no-driver.ps1` |
| state.md: The `Include` line | **Not checked, because the prototype does not meet it:** when the line is below a `Host` line, `create` MUST say that a person has to move it to the top of the file. The prototype fails `create` with `internal` and leaves everything as it was, as it MUST, but its message names neither the line nor the move. That is a `test-unmet-*` requirement, held by `conformance/lease-cli/test-unmet-include-message.ps1`, which checks the move and the top only |
| driver.md: How it is reached; state.md: The alias file | **The default ProxyCommand is checked only as far as D8 makes it visible**, by `test-lease-alias-file.ps1`: one of the two forms, the full path of a program that exists, `cmd.exe /d /c` from the system folder when and only when it is a `.cmd` or `.bat`, and the alias last. That the program is the one beside the CLI's executable is not checked, since the prototype's entry point (`bin\agentspace.cmd`) is not its CLI's code. Neither check seeds an instance-ID cache, because `create` MUST NOT need one (D21); the prototype's need of its checkout's copy is `test-unmet-default-transport-no-cache.ps1`'s ([The files that are expected to fail](#the-files-that-are-expected-to-fail)), and in a checkout without it the alias-file check's one default-line result is SKIPPED |
| lease-api.md: Times | **The run in another zone exists, and who makes it is settled** ([What the specs require of the suite](#what-the-specs-require-of-the-suite)): a machine whose zone is not UTC, run by the agent or person who runs the suite for the build. No check sets the zone, because Windows has no variable for it. The expiry boundary second needs a clock the check controls ([Q6](#open-questions)) |
| lease-api.md: Concurrency and atomicity | **Covered by `conformance/lease-cli/test-lease-concurrency.ps1`**: concurrent creates, renews and a release as separate processes, readers that never wait, and the 30-second limit for each of the three commands. It is evidence and not proof ([What the specs require of the suite](#what-the-specs-require-of-the-suite)). Left: a failed `release` leaving the lease as it was (the `release` row above), and a command killed in the middle |
| lease-api.md: The program | Arguments reach the program unchanged from Git Bash have no check, since it needs Git Bash. **An argument such as `-:x` is checked, and fails against the prototype** (#0083 point 1; #0110): `conformance/lease-cli/test-unmet-runtime-refused-argument.ps1` runs it in each position and as a flag's value, and the prototype's runtime refuses it with exit 1 and no error document. **A line feed inside an argument is checked, and fails against the prototype**: its `bin\agentspace.cmd` cuts the argument at it, so a project value `a<LF>b` is read as `a` and the create succeeds, where the spec requires `invalid_value`. The check is `conformance/lease-cli/test-unmet-argument-line-feed.ps1`, which fails against the prototype ([The files that are expected to fail](#the-files-that-are-expected-to-fail)) |
| lease-api.md: The credential, no CA | `create` and `renew` MUST fail with `internal` and make no CA when none exists (#0029 point 3). **The prototype's `create` makes one**, so the check of the rule fails against it: `conformance/lease-cli/test-unmet-no-ca.ps1` ([The files that are expected to fail](#the-files-that-are-expected-to-fail); [docs/state.md](state.md#its-checks)). Also checked, and passing: an unsatisfiable `create` with no CA answers and makes none |
| driver.md | The driver boundary is in-process, so no conformance check can drive it. The registration checks are not movable. What a caller sees of the driver is covered by `conformance/lease-cli/test-lease-driver-outcomes.ps1`: unsatisfiable requests, no driver name in any output, and a failed alias step. A foreign alias file kept by a failed `create` is not movable, since the alias name comes from a random ID. **From v0.09, what a caller sees of placing by size is covered by Z1 to Z16** (`test-unmet-sizes-place.ps1` and `test-unmet-sizes-pending.ps1`, #0205), and the in-process parts, `SIZES`, the map, `meets`, `resize_target` and place's decision row by row, by `agentspace-driver-ec2`'s own tests (E0.09-T02-S02, #0201; E0.09-T03-S01, #0203) ([The files that are expected to fail](#the-files-that-are-expected-to-fail), v0.09's files) |
| keeper.md: The installed task | No conformance check covers an installed task. The three installer cases were in `scripts/` as prototype code until E0.06-T06-S01 removed them, and [Coverage](#coverage-of-the-operator-and-the-keeper) says why |
| keeper.md: Release grace, card #0068 | **Checked by `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1`, which fails against the prototype** ([The #0068 file](#the-0068-file-test-unmet-holder-dies-mid-passps1)). The gap that remains is the window itself: the keeper logs nothing when its look finds a holder alive and its pass then waits for `board-holder.lock`, so no event marks the moment between the look and the trust check. The file enters it with one bounded wait from the keeper's own `skipped` line, reads the log to see which path the keeper took, and reports SKIPPED if it missed. The second case in the file reaches the same rule with no timing, through a record that is gone |
| keeper.md: Checking the keeper, card #0066 | **Shown for the Rust keeper (E0.06-T04-S01):** the "Watch locked" case passed 5 times in a row beside two other copies of the suite ([The keeper suite](#the-keeper-suite)). **Not shown:** the same under the prototype's keeper, a heavier load, which no run was made with |
| keeper.md: The task's launcher; Stopping and starting the keeper | Nothing runs the launcher, the install, or the End and restart steps: they need the real Task Scheduler |
| keeper.md: Starting a holder | The refusals of "Before the start" (a state folder or a keeper path holding a `"`, an alias outside the form), the create retries, the 10-second bound on one process-create call (D35) and the 35-second, 60-second and 95-second bounds have no case. The first needs a path that holds a `"`, which Windows refuses, and an alias outside the form, which the lease state's schema refuses; the rest need a launcher that does not answer, or a create call that does not return |
| keeper.md: Locks | The `board-holder-keeper.lock` error path (any error other than the file being held) and the `board-holder.lock` error that is not "in use" have no case |
| state.md: The keeper's files | The share modes and the access rules of the keeper's files, and the temporary files of a killed pass, have no case; the new case checks the JSON files' schemas, bytes and the launch files' removal only |
| The live-box check | `check-forward-release.ps1` is the only check of the box's sshd, and the conformance suite does not run it |
| driver.md: place_pending, and a run's own records | **No lease CLI case reaches it**: only a provisioning run calls `place_pending`, and the lease CLI's stand-in run never does ([driver.md, Conformance cases for v0.07](driver.md#conformance-cases-for-v007)). Covered from the operator's side by `test-unmet-host-provision-place.ps1`, with a seeded lease state file, and by E0.07-T05-S01's live check of a first agentspace lease ([Coverage](#coverage-of-the-operator-and-the-keeper)). **From v0.09, `place_pending` places only the leases the host meets, and answers or leaves the rest** (D68's change, pin P1): its lines are `test-unmet-host-provision-size.ps1`'s step 9 case (#0206), its answers `agentspace-driver-ec2`'s own tests (E0.09-T03-S01, #0203), and a lease placed by a real resizing run D70's "Bigger" live check, E0.09-T05-S01 (#0208) |
| operator.md: deploy, provision, bootstrap and sync on a real host | **No stand-in can make a host** (D53): a real stack, a real instance, a real key exchange, sshd's trust of the CA and a real Session Manager session have no seam, and `go` and `ssh-keygen` have none either ([operator.md Q30](operator.md#open-questions)). The calls are covered by the operator's v0.07 files, and each real effect is one of E0.07-T05-S01's live checks ([Coverage](#coverage-of-the-operator-and-the-keeper)) |
| driver.md, operator.md: a host's type change (v0.09) | **No stand-in can show a host's type change** (D70): a stand-in `aws` shows the `InstanceType` the operator passes and the answer it reads, and only a real stack changes the instance's type. The calls are `test-unmet-host-deploy-size.ps1`'s and `test-unmet-host-provision-size.ps1`'s (#0206); the change itself, the host coming back at the new type, the first lease after the install recording its type, and the real AWS CLI's `describe-stacks` answer in the form read are D70's live checks, E0.09-T05-S01, #0208 ([Coverage](#coverage-of-the-operator-and-the-keeper), v0.09's rows). The box is not resized live (D70) |
| keeper.md: the keeper on a host that is not the box | The stub holders show every rule of [Hosts](keeper.md#hosts) in a throwaway home; that the keeper holds the board on agentspace's own host is E0.07-T05-S01's live check ([Coverage](#coverage-of-the-operator-and-the-keeper)) |
| keeper.md: Starting the guard | **Covered by `conformance/keeper/test-unmet-keeper-guard.ps1`** (E0.08-T04-S03, #0184), against a stand-in guard ([The guard's start: the keeper's v0.08 cases](#the-guards-start-the-keepers-v008-cases)). What it leaves is E0.08-T05-S01's live check that the installed keeper starts the real guard ([Coverage](#coverage-of-the-operator-and-the-keeper), v0.08's rows). Three of the `<why>` texts of [The keeper's lines for it](keeper.md#the-keepers-lines-for-it) are the keeper crate's own tests, since no case can bring them about from outside |
| operator.md: `guard`'s timing, a host's stop and a real session | **No stand-in can show a host stopping, a real Run Command or a real session, and the suite has no fake clock**, so the grace's timing is D62's live checks, E0.08-T06-S01 (the box) and E0.08-T06-S02 (agentspace's host) ([Coverage](#coverage-of-the-operator-and-the-keeper), v0.08's rows). The time format at a chosen hour is a crate test, E0.08-T03-S02. **From v0.09, the guard meeting a resize** (D68: not stopping a host while its run holds `provision.lock`, and a `pending` lease explaining it outside the lock) **is D70's "The guard" live check, E0.09-T05-S01 (#0208)**; `test-unmet-guard.ps1`'s held-`provision.lock` and `pending`-lease cases already show the rules a resize relies on, and `crates/agentspace-guard` is unchanged on `main` since v0.09 began |
| `box/system`: the idle check on a host | **Covered in part by `box/tests/test_agentspace_idle.py`** ([The idle check's test](#the-idle-checks-test)). The check with no seam set on a real host, the laptop away, is E0.08-T05-S01's live check, and `/run/agentspace/lease-horizon` as a real guard run writes it |
| driver.md, lease-api.md: `release` ending a desktop-app session | **Covered for stand-in clients** by `test-unmet-release-ends-connections.ps1` (E0.08-T04-S02, #0183) and, for the second walk, the further walks of D59 as changed for v0.10-T03 (E0.10-T03-S02, #0196) and the cases no stand-in shows (another user's process, a refused termination), by `agentspace-connections`' and the EC2 driver's own tests ([driver.md, How it is checked](driver.md#how-it-is-checked)). The desktop app's own connections and the host's end of an ended connection are E0.08-T06-S02's live check |
| driver.md: Starting a provisioning run, step 2 (D77), row 6 with a seeded `last_success_at` | **A known gap** (the architect, for v0.10-T04, on E0.10-T04-S01's reviewer's N1): no lease CLI case shows driver.md's row 6 together with a seeded `last_success_at`, since every `create` of Y1, Y2, Y4 and Y5 is made on the whole seed and reaches row 1. Rows 1, 2 and 6 share step 2's spacing, and step 2 is covered directly by `agentspace-driver-ec2`'s `tests/due_after_success.rs` (#0216) |
| driver.md: Starting a provisioning run, a lease created after `place_pending`'s last re-read and before the run exits (pin P31) | **Not shown by any case**: no stand-in can time it, and it is the order of the locks ([driver.md, Conformance cases for v0.10](driver.md#conformance-cases-for-v010)). D80's live check starts no instance, so it does not show it either |
| state.md: What a plan lease adds (the stored `sized_for` and `plan`, `driver` required with them, the `plan_too_large` record, and the plan never printed) | **Covered in part, and not by a lease CLI case**: no lease CLI case reads a plan lease's record in the lease state file, and every plan case reads the document the program prints. The record's form is `test-state-schemas.ps1`'s samples (the lease state file's plan leases, its `plan_too_large` record and their invalid samples, and the copy of the plan schema's `$defs`), and what `create` writes is the crates' own tests (`agentspace-state`'s `tests/plan_record.rs` and `tests/sized_for_document.rs`, `agentspace-cli`'s `tests/lease_plan.rs`). **The file's growth with every plan lease is accepted** (state.md, pin P29), and no case shows it |
| lease-api.md: Plan, the per-agent footprint (D73, D76) | **Covered at today's figure only**: `test-plan-sizes.ps1`'s edges and `test-plan-errors.ps1`'s 32 are taken from lease-api.md's size table at half a vCPU and 2 GiB for each agent, and the footprint is not part of the contract, so a new figure changes those two files and no contract version |
| state.md: What a plan lease adds, a board lease's `plan_source` (D86; the architect, for v0.11-T01, pin P7) | **Covered in part, and not by a lease CLI case**: no case reads a board lease's record in the lease state file, since no output prints `plan_source` (D87) and every board case reads the document the program prints. The record's form is `test-state-schemas.ps1`'s samples (`leases.state.plan-source.json` and its invalid ones), and what `create` writes is the crates' own tests (`agentspace-state`'s `tests/plan_source.rs`, `agentspace-cli`'s `tests/lease_board.rs`) |
| lease-api.md: TTL, the factor of half again (D85) | **Covered at today's figure only**: `test-plan-turn-ttl.ps1` takes the budget and half again from lease-api.md's TTL section, and the factor is not part of the contract, so a new factor changes that file and no contract version |
| lease-api.md: The board read, the rows that no stand-in answer reaches (the architect, 2026-11-26, for v0.11-T04, pins P10 and P11) | **No lease CLI case shows them, and a crate test owns each**: `LOCALAPPDATA` unset, empty or only whitespace (`agentspace-cli`'s `tests/lease_board.rs`, `localappdata_unset_empty_or_only_whitespace_is_an_internal_error_and_reads_no_board`), a 200 that is not the shape (`agentspace-board`'s `tests/read_epic.rs`, `a_200_that_is_not_the_shape_could_not_be_read`), a connection closed with no answer or an answer that stops short (`a_connection_closed_with_no_answer_does_not_answer` and `an_answer_that_stops_short_does_not_answer`, in the same file) and the 30 seconds after which a board that never answers is given up on (`slow_a_board_that_never_answers_is_given_up_on_after_30_seconds`, in the same file) |
| Every file of `conformance/` that quotes captured program output in a failure's reason (#0230, #0243) | **The helper `Get-HpLogShape` and one provision reason are masked** (#0240). **Every other place that quotes program output, a stand-in's record or an exception's message unmasked is carded as #0243** (Backlog; the architect, 2026-11-26, for v0.11-T04), and is listed whole, file and line, in [the v0.11 run record](runs/e0.11-t04-s03.md#captured-output-quoted-unmasked-0243). A failure of such a file MUST be masked by whoever prints its reason outside the run's own folder |

## Coverage of the operator and the keeper

**RFC 2119 keywords again.** E0.04-T05-S05 (#0092) closes B-51, which found that no check existed
for the push, the operator's commands, `ca`, the ProxyCommand's steps or the installed task. It
wrote the checks that the documented test seam ([operator.md](operator.md#the-test-seam)) lets a
check make from outside, with no AWS, no box and none of Terry's real files, and **this section says
what it did not cover and why.** **Every row's reason starts "Later, not blocking:"**: no build is
held up by the absence of the check, because the behavior is specified in full in the doc named, and
a check for it can be added by a later story without changing a spec.

| Area | Checked by | Not checked |
|---|---|---|
| operator.md: The token push | `conformance/operator/test-unmet-push.ps1` (20 results, through the seam). **From v0.08, #0167's rules** (no push at a start with a current stamp, one `The host is ready.` from a ProxyCommand connection whose push is due, two under `AGENTSPACE_DIAGNOSTIC`) are the cases of the new `conformance/operator/test-unmet-push-nested.ps1` (E0.08-T04-S01, #0182; `test-unmet-push.ps1` itself is unchanged) and unit tests of `agentspace-push` and `agentspace-ssh-proxy` (E0.08-T02-S04, #0176), by [operator.md's table](operator.md#which-checks-show-it) | **Later, not blocking:** what the box does with the content (the box's half, steps 1 to 8 of [What the box does](operator.md#what-the-box-does)) runs only on a host, and the check's stand-in `ssh` is its own reading of that half, so a real box's half is shown only by the live-box check ([Q5](#open-questions)). **Later, not blocking:** `sync`'s forced push needs the build and copy steps, which the seam does not stand in for (operator.md Q30, which is itself later) |
| operator.md: The time zone | The seven legacy names, and eight zones that fail the zone check (`test-unmet-push.ps1`) | **Later, not blocking:** the conversion of a Windows zone ID to an IANA name, because the seam replaces the laptop's zone before it, so a check would need a machine set to each zone; the spec's table is the requirement |
| operator.md: `ca` | `conformance/operator/test-ca.ps1` (10 results) | **Later, not blocking:** the refusal when run elevated (the suite MUST NOT run elevated), a machine with no `ssh-keygen.exe` (the program is found by its fixed path), and the access rules of the CA's folder and key (a check of a loosened key would grant another account access to a private key, which no check does; the one rule a check sets is `test-unmet-failed-create-cleanup.ps1`'s, a deny for the current user on a folder of its throwaway home, lifted in a `finally`) |
| operator.md: `deploy`, `sync` | `deploy` up to its step 5: **`conformance/operator/test-unmet-deploy-passthrough.ps1`** (E0.06-T05-S02, #0140, 4 results): the AWS CLI's output passed through, and the call returning once the CLI exits, with a stand-in that writes to stdout as the real CLI does (architect, 2026-11-20). The suite had no `deploy` case before it, so nothing ran the pass-through through the operator's `main`, and defect #0139, a `deploy` that never returned, reached the laptop. `sync`: none until v0.07. **From v0.07** (E0.07-T04-S02, #0156), `deploy -Host` and `sync -Host` and `-All` have cases through the stand-ins, in `test-unmet-host-deploy.ps1`, `test-unmet-host-deploy-registry.ps1`, `test-unmet-host-bootstrap-sync.ps1` and `test-unmet-host-bootstrap-sync-all.ps1`, and `sync` on a new host is E0.07-T05-S01's live check ([v0.07's rows](#coverage-of-the-operator-and-the-keeper), below). **From v0.08, `sync`'s patch wait** (its calls after the install row in order, a reboot in the middle of the wait, a failed run, a queued start and the 30-minute bound as far as a stand-in shows it) **is `conformance/operator/test-unmet-sync-patch-wait.ps1`** (E0.08-T04-S01, #0182); the install starting the run, #0163 on a real host and #0166's `PATH` are E0.08-T05-S01's live checks ([v0.08's rows](#coverage-of-the-operator-and-the-keeper), below). **Its two 30-minute deadline cases, `sync`'s and a provisioning run's, run only with `-Case deadline` and `-Case deadline-provision`** ([Cases that run only when asked for](#cases-that-run-only-when-asked-for)). **From v0.09, `deploy` by size** (`-InstanceType` for every host, the box's arguments without the file's `InstanceType`, steps 4 and 5's removal and record of the type, and a person's deploy holding `provision.lock`) **is `conformance/operator/test-unmet-host-deploy-size.ps1`** (E0.09-T04-S02, #0206); a real stack changing type is D70's live checks, E0.09-T05-S01, #0208 ([v0.09's rows](#coverage-of-the-operator-and-the-keeper), below) | **Later, not blocking:** `deploy`'s step 6 and `sync` run `go`, `ssh-keygen` and `scp`, and the seam stands in for the AWS CLI, `ssh` and `scp` alone, so a check could not stop `go` and `ssh-keygen` from running. **Covered from v0.07:** `sync`'s calls in order through the `ssh` and `scp` stand-ins, for `sync`, `sync -Host` and `sync -Host swimcoach`, by `test-unmet-host-bootstrap-sync.ps1`. **Later, not blocking:** `sync`'s failure message for each step of that sequence, and `deploy -Quiet` quieting the `sync` it runs (D36). The prototype reads none of the seam variables, so each would be a `test-unmet-` file that fails with `seam not met`, and a check no build could be shown to pass is not written blind |
| operator.md: `down`, `status`, `grow` | **`conformance/operator/test-unmet-commands.ps1`** (E0.06-T04-S02, #0132): each command's calls in order with their arguments, its messages and failures, and `grow`'s failed online grow (D36). **From v0.08, `status -All`'s `Leases` and `Guard` lines are `test-unmet-status-leases.ps1`, and `grow`'s refusal of `-GiB 0` and a negative `<n>` is `test-unmet-grow-zero.ps1`, beside this file's own two cases** (E0.08-T04-S01, #0182; E0.08-T03-S02, PR #125, had rewritten those two to the refusal) | **Later, not blocking:** a real AWS answer's other members and a real `df` table, which a stand-in writes in its own words |
| operator.md: Command line (a missing, unknown or repeated command or parameter, and the help texts) | `-Alias` (`test-alias-refusals.ps1`), and **every usage error, its order and every help block byte for byte** (`conformance/operator/test-unmet-usage-help.ps1`, E0.06-T04-S02, #0132). **From v0.08, `guard`'s help block and `guard` in the command lists are added to the same file** (E0.08-T04-S01, #0182, by [operator.md's table](operator.md#which-checks-show-it)); the top level's, `status`'s, `grow`'s and `sync`'s help texts were already v0.08's text on `main` (E0.08-T03-S02, PR #125). **`guard`'s refusals of `-Host`, `-All`, `-Alias` and `-InstanceType`, and `-GiB` ignored by it, are in `test-unmet-host-commands-line.ps1`**. **From v0.09, the top level's, `deploy`'s and `provision`'s help texts are v0.09's in `test-unmet-usage-help.ps1`, and `test-unmet-host-commands-line.ps1` has five host checks, check 6 removed** (E0.09-T02-S03, #0202, PR #137); `deploy -InstanceType` accepted for the box is `test-unmet-host-deploy-size.ps1`'s (#0206) | **Later, not blocking:** an alias with a line feed in it (the gap above). **Not from outside, by [The harness](#the-harness)'s rule:** the rules for an unset or empty `USERPROFILE`, which the programs' own tests show |
| operator.md: Starting the box and waiting for it | The case of a box that is running and old (`up` does not wait), through the stand-ins in `test-unmet-push.ps1` and `test-unmet-proxy-steps.ps1`; and **the whole step** in `conformance/operator/test-unmet-start-paths.ps1` (E0.06-T04-S02, #0132, 31 results by default and 5 with `-Case deadline`, 34 distinct between the two runs, the deadline's three running only with it: [Cases that run only when asked for](#cases-that-run-only-when-asked-for)): the stopped, stopping, pending and running boxes, the 20-minute rule, any other state, a launch time not of its form, the wait's messages printed once while they do not change, the poll, the patch question's answers (`patched`, `unknown`, an empty and an unexpected answer, a failed or timed-out check), the invocation's own poll and its 20-poll limit, and the 15-minute deadline with its exact message. **From v0.08, `up` on a host launched less than 20 minutes ago ends its wait at a `failed` patch answer** (P24.2), shown by `test-unmet-sync-patch-wait.ps1` (E0.08-T04-S01, #0182) | **Later, not blocking:** the deadline with Session Manager's agent never Online (the file waits its 15 minutes once, with `-Case deadline`, on a box whose patch check never finishes, and a second wait for another phase costs another 15 minutes), the question's own text ([What up waits for](operator.md#what-up-waits-for): no comma, bracket or equals sign, or a form that is not split), which a stand-in cannot tell from another text, and the exact 5-second and 1-second pauses, which the file checks loosely |
| operator.md: The ProxyCommand | The argument refusals, the steps (`test-unmet-proxy-steps.ps1`) and what a caller sees | **Later, not blocking:** the console-mode repair ([Q17](operator.md#open-questions)) needs a Windows console, and the harness gives a program pipes |
| keeper.md: Streams and environment (the holder and its launcher run in the user's logon environment, D35) | `conformance/keeper/test-holder-environment.ps1` | The holder's environment names show neither a sentinel set only for the keeper, nor `AGENTSPACE_STATE_DIR`, nor any `AGENTSPACE_*` name, with `SystemRoot` present as a control, and the holder's `USERPROFILE` is not the keeper's changed one (a yes or no marker, never the value). **Not shown:** the launcher's own environment (only the holder's is seen) and that the keeper itself held the sentinel (set before the start, not observable from outside; the check makes it a precondition) |
| operator.md: The Bot credential (the token push) | `test-unmet-push.ps1` writes its fake credential as `bot.json`, with the hex that PowerShell's `ConvertFrom-SecureString` makes of a dummy token as `token_dpapi_hex` | **Later, not blocking:** making the blob with `CryptProtectData` directly, as [operator.md, The Bot credential](operator.md#the-bot-credential) describes a test's credential, would make the check independent of PowerShell's own DPAPI call. It changes how a fixture is made and shows nothing new |
| keeper.md: The installed task, the launcher, stopping and starting | None. **From v0.08, the installed keeper restarted to load v0.08's build and logging `guard started pid=<pid>` is E0.08-T05-S01's (#0186) live check** ([v0.08's rows](#coverage-of-the-operator-and-the-keeper), below) | **Later, not blocking:** the task is the machine's Task Scheduler entry, which a throwaway home does not isolate, so a check would register a real task in the user's account. The installer's usage errors change nothing on the machine ([docs/keeper.md, The installer](keeper.md#the-installer); D37; B-51), so a check of them would register no task, but none is written yet: no conformance variable names the installer ([Naming the program under test](#naming-the-program-under-test)), and it runs only from the install folder (D37), so such a check needs a fifth variable and an install to point it at. The three installer cases were in `scripts/` as prototype code until E0.06-T06-S01 removed them |

The numbers in "Checked by" are the results of each file, run against the prototype or against a
stand-in of a conforming build, as [The operator and the token push](#the-operator-and-the-token-push)
says.

**Which of these rows v0.06 closes** (architect, 2026-11-19, for v0.06, gap FB-25). Each "Later"
above was later because no build met the seam; v0.06's Rust operator will. **v0.06's stories MUST
write the checks that the seam lets a check make from outside, each in the story that builds what
it checks:** `down`, `status` and `grow` (its failed online grow included); the operator's usage
errors and help texts; the stopped and stopping start paths and the patch question, with a
stand-in `aws` whose answers change from call to call; and the keeper's logon environment (D35).
**These stay later after v0.06, each for the reason its row gives:** `deploy` after its step 5,
and `sync`, until operator.md Q30 stands in for `go` and `ssh-keygen` (`deploy` up to step 5 is
checked by `test-unmet-deploy-passthrough.ps1`, E0.06-T05-S02, architect, 2026-11-20); the box's half of the push; the Windows zone
conversion; `ca`'s elevated refusal and loosened key; the console-mode repair; and the installed
task, its launcher and the installer, which a throwaway home cannot isolate. #0110's points ride
with the same release (roadmap.md).
- **What E0.06-T04-S02 (#0132) closed of those rows**, each with a file under `conformance/operator/`
  that fails against the prototype and that v0.06's Rust operator is checked by:
  - **`down`, `status` and `grow`, `grow`'s failed online grow included:**
    `test-unmet-commands.ps1`.
  - **The operator's usage errors and help texts:** `test-unmet-usage-help.ps1`.
  - **The stopped and stopping start paths, the patch question and the 15-minute deadline, with a
    stand-in `aws` whose answers change from call to call:** `test-unmet-start-paths.ps1`. The
    deadline is waited in full only with `-Case deadline`, because no seam variable shortens it,
    and is neither run nor reported by default, so a default run takes about 2 to 3 minutes and
    exits 0
    ([Cases that run only when asked for](#cases-that-run-only-when-asked-for); [The operator and
    the token push](#the-operator-and-the-token-push)).
  - **Still open of the rows FB-25 names for v0.06:** the keeper's logon environment (D35), which
    is not this story's.
- **The skip reason that FA-20 found is fixed:** `test-interop.ps1` (since retired) said that no `ssh-keygen`
  was found in `System32\OpenSSH` or on `PATH`, the two places it looks.

**v0.07's rows: a host per project** (E0.07-T04-S03, #0157; D53, D55). **RFC 2119 keywords again.**
Every behavior that v0.07 adds to driver.md, operator.md and keeper.md is shown by a conformance
case of T04, against stand-ins, or by a live check; **no v0.07 behavior is left with neither**
(D53, and [architect.md, Breaking an epic into stories](architect.md#breaking-an-epic-into-stories),
step 5). So these rows differ from the ones above: a "Not shown by a case" cell names the live
check that shows it, never "Later".

**D53's live checks, each with the story that makes it:**
- **A first agentspace lease answers `pending`, and becomes `ready` once its host's run ends**,
  with `agentspace-host-agentspace` created, set up and recorded `ready` in `hosts.json`:
  E0.07-T05-S01 (#0159).
- **The lease's alias reaches the new host, and the keeper holds the board there:** E0.07-T05-S01.
- **After Terry's sign-ins (D52), an agent in a desktop-app session on the lease works a card on
  the agentspace board:** E0.07-T06-S01 (#0160), the done-when that waits on the sign-ins, which
  D55 gives a turn of its own.
- **A swimcoach lease created before and after connects to the box as it did**, with its alias file
  byte for byte as v0.06 writes it: E0.07-T05-S01.
- **`lease create`, `get` and `list` print documents that validate against contract v0's schemas
  throughout:** E0.07-T05-S01.
- **The `agentbox` change set is empty (D44):** shown live by the template's story, E0.07-T01-S05
  (#0145; [its run record](runs/e0.07-t01-s05.md);
  [operator.md, Which checks show it](operator.md#which-checks-show-it)), and checked again by
  E0.07-T05-S01 (#0159) as its step 5 (the architect, 2026-11-21).

| Area | Shown by a case | Not shown by a case, and the live check that shows it |
|---|---|---|
| driver.md: `no_capacity`, `pending`, the provisioning run's start, a `pending` lease's `get`, `list` and `release`, and a host's lease alias (C1 to C32) | The six lease CLI files of #0155, through `AGENTSPACE_PROVISION_CMD`'s stand-in (C6 and C29 in `test-unmet-pending-create-long.ps1`, C22 in `test-unmet-pending-create-lock.ps1`; [The files that are expected to fail](#the-files-that-are-expected-to-fail)) | A real detached run outliving the lease command that started it, with its own lines written into `provision.log`, each failed step's line holding the program's own error text: E0.07-T05-S01's live check of `provision` making a real host. The lease becoming `ready`: D53's check of a first agentspace lease |
| driver.md: place_pending, and a run's records | `test-unmet-host-provision-place.ps1`, with a seeded lease state file: a failure costing one lease while the others are placed, a killed placement's leftovers removed first, a lease released meanwhile never placed, a missing ProxyCommand program leaving every lease `pending`, and the ready-host rule. That `place_pending` takes the lock once per lease is the driver crate's own test, `tests\pending_place_pending.rs` ([operator.md, Which checks show it](operator.md#which-checks-show-it); architect, 2026-11-21) | A real run placing a real lease: D53's check of a first agentspace lease |
| operator.md: the command line, `-Host` and `-All`, the host registry, signing, the operator's folder and its sweep | `test-unmet-host-commands-line.ps1`, `test-unmet-host-commands.ps1`, `test-unmet-host-commands-all.ps1`, `test-unmet-host-commands-up.ps1`, `test-unmet-host-deploy.ps1` (the registry's write, with `test-unmet-host-deploy-registry.ps1`) and `test-unmet-host-deploy-folders.ps1` (the operator's folder and its sweep) | Every command with `-Host` started from Git Bash (#0027, carried by D55): E0.07-T05-S01's live check. The laptop's AWS sign-in needing no new permission (D44): E0.07-T05-S01's live check, by its run with today's sign-in |
| operator.md: `deploy -Host`, its arguments, the stack's status, `stack-parameters.json` and `-InstanceType` | `test-unmet-host-deploy.ps1` and `test-unmet-host-deploy-registry.ps1`, through the stand-in `aws` | `deploy -Host` creating a stack, and the host coming up from it: E0.07-T05-S01's live check. The `agentbox` change set: D53's check, shown by E0.07-T01-S05 (#0145) and checked again by E0.07-T05-S01 (#0159) as its step 5 |
| operator.md: `provision`, its steps, records, log lines and locks | `test-unmet-host-provision.ps1` (the steps, records, log and ready-host rule), `test-unmet-host-provision-failures.ps1` (each step failing), `test-unmet-host-provision-start.ps1` (the first steps, the locks and steps 3 and 8 on the registry) and `test-unmet-host-provision-place.ps1` (step 9), run as `provision -Host <project>` with the seam set | `provision` making a real host: E0.07-T05-S01's live check |
| operator.md: `bootstrap` | `test-unmet-host-bootstrap-sync.ps1`: its ssh's arguments, the script's bytes on its stdin, and its failure message | `bootstrap` installing the toolchain, and a second run changing nothing that matters: E0.07-T05-S01's live check |
| operator.md: `sync -Host` and `sync -All` | `test-unmet-host-bootstrap-sync.ps1`: the host's alias in every ssh and scp, and the calls in order for `sync`, `sync -Host` and `sync -Host swimcoach`; `test-unmet-host-bootstrap-sync-all.ps1`: `sync -All`'s `system\` and one build, its `Host` lines, a skipped host, the closing count and its exit code | `sync` on a new host, its copies, the install and the CA it trusts, and `sync -All`'s copies to real hosts: E0.07-T05-S01's live check. `go` and `ssh-keygen` have no seam ([operator.md Q30](operator.md#open-questions)) |
| operator.md: the operator's alias file and certificate | `test-unmet-host-alias-file.ps1`: the alias file's bytes, the `Include` line, a foreign file refused, a lease's `create` and `release` leaving it, and that the operator never writes agentspace's known-hosts file; `test-unmet-host-alias-no-proxy-program.ps1`: the refusal without `agentspace-ssh-proxy.exe` beside the operator; `test-unmet-host-alias-certificate.ps1`: the certificate's fields against a throwaway CA | A new host accepting the operator's certificate, and its key accepted on first use into agentspace's known-hosts file: E0.07-T05-S01's live check |
| operator.md: the ProxyCommand's host, and the push stamp per host | `test-unmet-host-proxy-lookup.ps1`: the three lookups and their failures; `test-unmet-host-proxy-instance-id.ps1`: the `instance_id` write; `test-unmet-host-proxy-stamp.ps1`: the stamp per host with the box's where it was | A lease alias reaching the new host: D53's check of the lease's alias. A swimcoach lease connecting to the box as before: D53's check of a swimcoach lease |
| lease-api.md: the documents of `create`, `get` and `list` | The lease CLI's cases that read a document validate it against contract v0's schemas, C5's `pending` and #0155's `no_capacity` among them. **From v0.10 they validate against contract v1's, which `schemas/` holds, and `test-v1-compatible.ps1` shows v0's still accept what a command line with no `--plan` prints** ([v0.10's rows](#coverage-of-the-operator-and-the-keeper), below) | The same on Terry's real setup, throughout the live run: D53's check of the documents |
| keeper.md: a host per project (H1 to H18), each host's holder log and the registry's read failures | `test-unmet-hosts-pass.ps1` and `test-unmet-hosts-watch.ps1`, and the existing keeper files in their registry mode for H1 ([A host per project: the keeper's v0.07 cases](#a-host-per-project-the-keepers-v007-cases)) | The keeper holding the board on agentspace's own host: D53's check of the lease's alias, which includes it |
| state.md: the host registry's and the project list's schemas | `test-state-schemas.ps1`, and every case that seeds or reads the two files | Nothing: no behavior of the two files needs a real host |
| A desktop-app agent working a card on agentspace's host | No case: it needs Terry's sign-ins (D52) | D53's check of an agent working a card: E0.07-T06-S01 (#0160) |

**v0.08's rows: host lifecycle and the cost guard** (E0.08-T04-S04, #0185; D62, D63). **RFC 2119
keywords again.** Every behavior that v0.08 adds to operator.md, keeper.md, state.md, driver.md and
`box/system`, or changes on them, is shown by a conformance case of T04, by a test of a crate or of
`box/tests`, or by a live check; **no v0.08 behavior is left with neither, but for the gaps named in the rows for the idle check
and for #0166 below** (D62, and
[architect.md, Breaking an epic into stories](architect.md#breaking-an-epic-into-stories), step 5).
A "Not shown by a case" cell therefore names the live check that shows it, never "Later", and a
part that no test and no live check shows is named as a gap, with its reason. **No
stand-in can show a host stopping, a real Run Command or a real session, and the suite has no fake
clock**, so the grace's timing is a live check ([operator.md, Which checks show
it](operator.md#which-checks-show-it)). **The results of T04's new files are in their pull requests** (#126 for E0.08-T04-S01's files, #127
for S02's and #128 for S03's); **the record of this turn's whole-suite run,
[docs/runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md), covers the conformance files on `main` at
`6e2fcbd`, the deadline case, the crates and the box tests**, and the table below gives no result.

**D62's live checks, each with the story that makes it** (D63's turns; each runs on the box and on
agentspace's host unless it says one):
- **E0.08-T05-S01 (#0186), the install and the host-side checks:**
  - **The alarm** deployed and read back, with `agentbox`'s change set holding only the alarm's
    modification.
  - **`sync` on both hosts:** the install starting this boot's patch run on agentspace's host with
    this boot's marker removed by hand, and `sync` going on with no 15-minute wait (#0163); a
    `systemctl kill` of a run under way, the patch check answering `failed`, and the next `sync`
    starting the run again (P24); the install starting nothing when the marker exists; and
    `ssh <alias> 'command -v rustc cargo codex claude gh'` finding every tool (#0166).
  - **The laptop away** (agentspace's host only): the keeper stopped, a CPU-busy process left on the
    host, and the host powering itself off past its horizon plus the grace.
  - **The installed keeper** restarted to load the build, logging `guard started pid=<pid>`; the
    horizon file as a real guard run writes it; and the laptop's AWS sign-in needing no new
    permission for the guard.
- **E0.08-T06-S01 (#0187), the box:** a release followed by the host `stopped` within D56's bound,
  measured from `released_at`; an expiry, the same bound from `expires_at`; a quiet held lease
  keeping its host past the grace; and a new lease's first connection starting the stopped host,
  the time to a prompt recorded.
- **E0.08-T06-S02 (#0188), agentspace's host:** the same four checks, and also an unexplained host
  (a host started with `up -Host agentspace` and no lease, reported by `guard.log` and by
  `status -All`, then stopped within the bound) and #0064 (a desktop-app session left open on a
  lease, the lease released, no ssh client of that alias left on the laptop and no session of that
  lease on the host 2 minutes later), with Terry's desktop-app session.

| Area | Shown by a case | Not shown by a case, and the live check that shows it |
|---|---|---|
| operator.md: `guard`, which hosts a live lease explains, the stop, the unexplained line, `provision.lock`, the horizon it sends, `guard.lock`, the failures and every `guard.log` line (D56, D57) | `conformance/operator/test-unmet-guard.ps1`, with `host-guard-common.ps1`, through the AWS stand-in (E0.08-T04-S01, #0182) | A host `stopped` within D56's bound, a quiet held lease keeping its host, a new lease starting the stopped host: E0.08-T06-S01 and E0.08-T06-S02. An unexplained host reported and stopped: E0.08-T06-S02. The horizon file as a real guard run writes it, its comment in Session Manager's history, and the real AWS CLI accepting the `describe-instances` filter and the send's parameters: E0.08-T05-S01. A send to a host whose Session Manager agent is not yet online failing, and the next run's reaching it: recorded where it is seen, not required (D62) |
| state.md: live leases, a lease's end and the horizon, as the guard and `status -All` read them | Every guard and `status` case seeds the lease state file, and `agentspace-state`'s own tests (E0.08-T02-S01, #0173) | Nothing: the functions read a file |
| operator.md: `status -All`'s `Leases` and `Guard` lines (D60) | `conformance/operator/test-unmet-status-leases.ps1`; the 12-hour form at every hour is a crate test (E0.08-T03-S02), since a case cannot choose the hour of its run | `Leases    <n> live` matching the leases held before and after each lifecycle step: E0.08-T06-S01 and E0.08-T06-S02. The `Guard` line for an unexplained host: E0.08-T06-S02 |
| operator.md: `grow`'s refusal of `-GiB 0` and a negative `<n>` (D61, #0164) | `conformance/operator/test-unmet-grow-zero.ps1`, beside `test-unmet-commands.ps1`'s two cases; the refusal with `USERPROFILE` unset is the operator crate's test, `grows_gib_refusal_is_a_usage_error_that_needs_neither` | Nothing: the refusal reaches no host |
| operator.md: `sync`'s patch wait, and `up` not waiting on a failed patch run (D61: #0163; P13, P21, P24) | `conformance/operator/test-unmet-sync-patch-wait.ps1`; the patch check's command text by its builder's own test under bash with a stand-in `systemctl` first on `PATH` (E0.08-T03-S05, #0181) | The install starting the run, #0163 with no 15-minute wait, a `failed` unit with a start queued, the install starting nothing when the marker exists, and a patch reboot if the run asks for one: E0.08-T05-S01 |
| operator.md: #0166, the tool folders on every session's `PATH` | No case: it is `box/system`'s install. E0.08-T01-S05's test covers the idle check alone, and `box/tests` holds no test of `install.sh`, so the edit's edges, which operator.md leaves to such a test where there is one, are a gap: nothing shows them but the live check. **The gap is carded as #0193** (Backlog; the architect, 2026-11-22, for v0.08-T04) | The `/etc/environment` `PATH` lines as operator.md gives them with its other lines kept, the `Installed:` line's `(added)` at the first `sync` and `(already there)` at the second, and `command -v` finding every tool on both hosts: E0.08-T05-S01 |
| box/system: the idle check, D11's rule and D57's horizon | `box/tests/test_agentspace_idle.py`, through its three seams ([The idle check's test](#the-idle-checks-test)): a horizon that is missing, empty, a folder or not digits, D11's rule alone; the power-off past the horizon plus the grace with its exact line; a session or a patch run holding the host up; the grace read from the settings file, and an invalid one replaced by 15 with a warning; `date` unable to format giving `@<h>`; D11's lines and limits | The check with no seam set, the laptop away: E0.08-T05-S01. **Four parts of operator.md's idle-check row are gaps, each because no method of E0.08-T01-S05's test covers it and no live check does:** a horizon holding a CR; a horizon of 16 digits; D11's rule one second before the horizon plus the grace; and `HORIZON_GRACE_MINUTES` not set giving 15 (#0192) |
| The alarm's 24 hours (D58) | The template's story, E0.08-T01-S04 (#0171), read live before its merge | Deployed and read back: E0.08-T05-S01 |
| keeper.md: the keeper's start of the guard | `conformance/keeper/test-unmet-keeper-guard.ps1`, against a stand-in guard; three of the keeper's `<why>` texts are the keeper crate's own tests | The installed keeper starting the real guard: E0.08-T05-S01 |
| driver.md, lease-api.md, state.md: `release` ending the lease's own connections and `release.log` (D59, #0064) | `conformance/lease-cli/test-unmet-release-ends-connections.ps1` (E0.08-T04-S02, #0183), R1 to R15 with stand-in clients, R15 only for a second client started within the release's run; the second walk, the further walks of D59 as changed for v0.10-T03, and what no stand-in shows are the crates' own tests (E0.08-T03-S04, #0180; E0.10-T03-S02, #0196; `agentspace-connections`) | The desktop app's own connections, and the host's end of an ended connection: E0.08-T06-S02 |
| operator.md: #0167, the push at a start and the nested `The host is ready.` | `conformance/operator/test-unmet-push-nested.ps1` (E0.08-T04-S01, #0182) and the unit tests of `agentspace-push` and `agentspace-ssh-proxy` (E0.08-T02-S04, #0176) | Nothing: a stand-in `ssh` runs the ProxyCommand nested |
| operator.md: `provision`'s step 10 line | A crate test of `agentspace-sync` (E0.08-T03-S05) and `test-unmet-host-provision.ps1`'s expected line, changed by that story, E0.08-T03-S05 (PR #123), and not by E0.08-T04-S01 | Nothing: the line is text |

**v0.09's rows: size classes** (E0.09-T04-S03, #0207; D70, D71). **RFC 2119 keywords again.** Every
behavior that v0.09 adds to lease-api.md, driver.md, state.md and operator.md, or changes on them,
is shown by a conformance case of T04 against stand-ins, by a test of a crate, or by one of D70's
live checks; **no v0.09 behavior is left with none of the three** (D70's last bullet, and
[architect.md, Breaking an epic into stories](architect.md#breaking-an-epic-into-stories), step 5).
A "Not shown by a case" cell therefore names the live check or the crate test that shows it, never
"Later". **No stand-in can show a host's type change** (D70; [driver.md, Conformance cases for
v0.09](driver.md#conformance-cases-for-v009), and [operator.md, Which checks show
it](operator.md#which-checks-show-it)): a stand-in run makes no host, and a stand-in `aws` shows
only the arguments the operator passes and the answers it reads. **The results of T04's new files
are in their pull requests; the record of this turn's whole-suite run,
[docs/runs/e0.09-t04-s03.md](runs/e0.09-t04-s03.md), covers the suite and the crates on `main`,
and cites v0.08's run of the box tests for the unchanged `box/`**, and the table below gives no
result. **v0.07's `deploy -Host` row above is superseded
in part**, as operator.md's v0.09 rows say of its own: `deploy -InstanceType` is accepted for the
box, and the box's `deploy` no longer passes the file's `InstanceType`.

**D70's live checks, all on agentspace's host but the last, are E0.09-T05-S01's (#0208):**
- **Bigger:** with the host stopped and no live lease, `create --size l` answers `pending`, becomes
  `ready`, and `nproc` over its alias prints 8; the run's `deploy done type=m9g.2xlarge`, the
  stack's `InstanceType` and the record's `instance_type` both `m9g.2xlarge`, and the real AWS
  CLI's `describe-stacks` answer giving `Parameters` in the form read. The lease document names
  only the size (D1).
- **Met at once:** with that lease live, `create --size m` answers `ready`, on the same host.
- **Refused:** with that lease live, `create --size xl` answers `unsatisfiable` with `no_capacity`
  and D66's message; no run starts, and the host's type, record and state are unchanged.
- **Smaller:** once every lease is released, `create` with the default size answers `pending`, and
  the host comes back as `m`, four vCPUs, recorded so.
- **Unsupported:** `create --size xxl` answers `unsatisfiable` with `unsupported_size`, makes no AWS
  call and starts no run, with `provision.log` showing no new line.
- **The first lease after the install:** the host's record gains `instance_type` by its run (D67).
- **The guard:** `guard.log` and `status -All` show no stop of the host during a resize, while the
  run holds `provision.lock`, and the host is `stopped` within D56's bound after the last release.
- **The laptop's AWS sign-in needing no new permission for a resize**, by its run with today's
  sign-in.
- **The box is not resized live** (D70): its first lease after the install records its type at
  `m`, which changes nothing on it, shown by its registry record and `status -Host swimcoach`.

| Area | Shown by a case | Not shown by a case, and the live check or test that shows it |
|---|---|---|
| lease-api.md and driver.md: the four sizes and `unsupported_size` (D65) | `test-unmet-sizes-place.ps1`'s Z1 and Z2 (#0205); the sweep's `xxl` in the earlier files (E0.09-T01-S03, #0199, PR #133) | `SIZES` and `instance_type_for` over their whole domain: `agentspace-driver-ec2`'s tests, E0.09-T02-S02 (#0201). A real host at each size: D70's "Bigger" and "Smaller" at `l` and `m` only, by Q11's 4 instance hours at `l` or smaller; `s` and `xl` are never made live, and their types are the map's crate tests |
| driver.md: place on a `ready` host by its type and the project's leases (D66 and its change; rows 1 to 6) | `test-unmet-sizes-place.ps1` (Z3, Z4, Z6, Z7, Z11, Z13) and `test-unmet-sizes-pending.ps1` (Z5, Z8 to Z10, Z14), through `AGENTSPACE_PROVISION_CMD`'s stand-in (#0205) | `meets` over its domain, place's decision row by row over stand-in files, and its `internal` answer for a lease state file lost under its hold: `agentspace-driver-ec2`'s tests, E0.09-T02-S02 (#0201) and E0.09-T03-S01 (#0203). A real lease met at once, and one refused while another holds the host: D70's "Met at once" and "Refused" |
| driver.md, state.md: the box, and the type a missing `hosts.json` implies and never writes (pins P2c, P31) | `test-unmet-sizes-pending.ps1`'s Z12, Z15 and Z16 (#0205) | The box's first lease recording its type at `m` with no change to it: D70's "The box is not resized live" |
| state.md: the host registry's `instance_type` and its schema | `test-state-schemas.ps1`, over E0.09-T01-S02's samples (#0198, PR #135), and every case that seeds or reads a typed record | `agentspace-hosts`' own tests, E0.09-T02-S01 (#0200). The first lease after the install writing it on a real host: D70's "The first lease after the install" |
| operator.md: `deploy` by size: `-InstanceType` for every host, the box's arguments, steps 4 and 5, a failure after a successful deploy, `provision.lock` held for its life (D67 and its changes) | `test-unmet-host-deploy-size.ps1` (#0206); `test-unmet-usage-help.ps1` and `test-unmet-host-commands-line.ps1`, at v0.09's text since E0.09-T02-S03 (#0202) | The parse in process: the operator crate's tests, E0.09-T02-S03 (#0202). CloudFormation changing the type, and the real `describe-stacks` answer's form: D70's "Bigger". A plain `deploy` of the box not changing its type is shown by the case alone, since no plain `deploy` of the box is run for v0.09 (D70) |
| operator.md: `provision` by size: its type from the lease state file, its locks (pin P26), its failures, its log lines and step 9's lines (D68 and its changes) | `test-unmet-host-provision-size.ps1`, and the typed seed in `test-unmet-host-provision-failures.ps1` (#0206) | `resize_target` over its domain: `agentspace-driver-ec2`'s tests, E0.09-T02-S02 (#0201); the run's use of it, and pin P26's order against a `create` at the same time: `agentspace-sync`'s tests, E0.09-T03-S02 (#0204). A real resize up and down: D70's "Bigger" and "Smaller" |
| driver.md: `place_pending` placing only the leases the host meets, and answering or leaving the rest (pin P1) | `test-unmet-host-provision-size.ps1`'s step 9 lines (#0206), by the log alone | Its answers, step 3.4 and its two passes: `agentspace-driver-ec2`'s tests, E0.09-T03-S01 (#0203). A lease placed by a real resizing run: D70's "Bigger" |
| operator.md, driver.md: the guard and the idle check meeting a resize (D68) | `test-unmet-guard.ps1`'s held-`provision.lock` and `pending`-lease cases, unchanged since v0.08 (E0.08-T04-S01, #0182) | No stop during a real resize, and the stop after the last release: D70's "The guard" |
| state.md: live leases as place and provision's deploy step read them (D66) | Z6, Z7 and Z13 (#0205), and `test-unmet-host-provision-size.ps1`'s expired and `released` records (#0206) | Nothing more: the functions read a file, as v0.08's row for the guard says |

**v0.10's rows: plans, contract v1 and a run's success** (E0.10-T04-S03, #0223; D80). **RFC 2119
keywords again.** Every behavior that v0.10 adds to lease-api.md, driver.md, state.md and operator.md, or
changes on them, is shown by a conformance case of T04 against stand-ins, by a test of a crate, or by
one of D80's live checks; **no v0.10 behavior is left with none of the three**, but for the gaps the
[gaps table](#the-gaps) names (D80's last bullet, and [architect.md, Breaking an epic into
stories](architect.md#breaking-an-epic-into-stories), step 5). A "Not shown by a case" cell therefore
names the live check or the crate test that shows it, never "Later".
- **Every "None" this page's coverage tables hold was read for what v0.10 changes.** The one cell that
  says `None` is the keeper's installed task, launcher and installer row, which no v0.10 story touches. **The
  cells that name contract v0 are v0.07's**, the live-check bullet and the row for "the documents of
  `create`, `get` and `list`" above: v1 supersedes them in part, as this release's rows say, and
  the first cell of the table below is their successor. **Each v0.10 behavior below has a case or one of D80's
  live checks**, and **the gaps table names the four parts that no case shows whole**: row 6 with a
  seeded `last_success_at` and the stored record of a plan lease (each a crate test's), a lease created
  after placing's last re-read (pin P31, shown by nothing), and the per-agent footprint (shown at today's
  figure only).
- **No stand-in can show the real programs answering**, so D80's live check shows the build once it is
  installed. **The results of T04's new files are in their pull requests; the record of this turn's
  whole-suite run, [docs/runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md), covers the suite, the crates and
  the box's tests on `main`**, and the table below gives no result.

**D80's live check, E0.10-T05 (#0224), on Terry's laptop after the install, starts no instance**
(Q16: Terry declined a plan lease on agentspace's host, "No to both"; no instance hours are asked for).
It uses a project that is not on the project list, which D54 answers `unsatisfiable` with `no_capacity`
before any AWS call, so each lease it makes is `unsatisfiable`, nothing is issued and none needs a release:
- **Two plans of different peaks** give different `size`s, each with its own `sized_for`.
- **A plan too large for `xl`** answers `plan_too_large`.
- **A `create --size m` for the same project** prints a document that validates against v0's schema.
- **D77 needs no live check**: the lease CLI's suite shows it with a stand-in provision program, as the
  5-minute rule is shown, and needs no instance.

| Area | Shown by a case | Not shown by a case, and the live check or test that shows it |
|---|---|---|
| lease-api.md: `create --plan` picks the smallest size that holds the plan's busiest turn, with the peak counted from `after` (D73 to D75) | `test-plan-sizes.ps1` (E0.10-T04-S01, #0221): the edges of each size, ties, turns not summed, the peak as the width of the graph, model and effort changing nothing | The parse and the sizing over their whole domain: `agentspace-plan`'s own tests (`tests/sizes.rs`, `widths.rs`, `bounds.rs`, `width_oracle.rs`) and `agentspace-cli`'s (`tests/lease_plan.rs`). The installed programs: D80's "two plans of different peaks give different sizes" |
| lease-api.md: `sized_for` in every state and every command that prints a lease (D75) | `test-plan-sized-for.ps1` (#0221) | The member's form and its record: `agentspace-state`'s `tests/sized_for_document.rs`. The installed programs: D80's "each with its own `sized_for`" |
| lease-api.md: `plan_too_large`, `--plan` with `--size`, the refusals of a plan and the order the values are checked in (D74, D75, D81) | `test-plan-errors.ps1` (#0221); `test-usage-errors.ps1` and `test-values.ps1` still show the five `field` cases of v0 and the order of v0's values, and their comments say six and name `plan` | The plan's bounds and refusals in process: `agentspace-plan`'s `tests/refusals.rs` and `tests/bytes.rs`. The installed programs: D80's "a plan too large for `xl` answers `plan_too_large`" |
| lease-api.md: contract v1 keeping v0 for a command line with no `--plan`, and v0's schemas kept (D76) | `test-v1-compatible.ps1` (#0221), and `test-schemas.ps1` over `schemas/` and `schemas/v0/` (E0.10-T01-S02, #0212) | The installed programs: D80's "a `create --size m` prints a document that validates against v0's schema". **A client outside the repository** that reads a plan lease: none exists (D76), and no check shows it |
| lease-api.md: `create`'s v1 help | `test-help.ps1`, which reads lease-api.md at run time; its eight `create` cases failed against the installed v0.09 programs, and pass against `main`'s build | The installed programs: 30 of 30 after v0.10's install, E0.10-T05 (#0224; [runs/e0.10-t05-s01.md](runs/e0.10-t05-s01.md)) |
| state.md: What a plan lease adds, the stored `sized_for` and `plan`, `driver` required with them and the `plan_too_large` record | `test-state-schemas.ps1`: the lease state file's samples and their invalid ones, and the plan schema's `$defs` held word for word | **No lease CLI case reads the stored record**: `agentspace-state`'s `tests/plan_record.rs` and `agentspace-cli`'s `tests/lease_plan.rs` show what `create` writes ([The gaps](#the-gaps)). A program built before v0.10 refusing such a file: the install's rule that v0.10's programs are installed together (state.md, pin P27), E0.10-T05 |
| driver.md and state.md: a run is due at once after a run that succeeded, and the 5 minutes for retries (D77; Y1 to Y5) | `test-unmet-pending-due-after-success.ps1` (#0221), for a `get` and a `create --size l` | Step 2 over its whole domain, row 6 with a seeded `last_success_at` included: `agentspace-driver-ec2`'s `tests/due_after_success.rs` (#0216). **A lease created after placing's last re-read and before the run exits** (pin P31): not shown ([The gaps](#the-gaps)) |
| state.md: the host registry's `last_success_at` and its schema (D77) | `test-state-schemas.ps1`'s samples of it and its invalid ones, and every case that seeds it | `agentspace-hosts`' `tests/last_success.rs`. A program built before v0.10 refusing a registry that holds it: the install's, E0.10-T05 |
| operator.md: `provision` removing `last_success_at` at step 3, writing it at step 9's end, writing none for a run that did not succeed, a failed write and the lock the write takes (D77; pins P22 to P24) | `test-unmet-host-provision-success.ps1` (E0.10-T04-S02, #0222), rows 1 to 6 of v0.10's table | `agentspace-sync`'s `tests/provision_success.rs` for the run's own logic. **A real run writing it on a real host** needs no live check by D80, which starts no instance: a stand-in `aws` shows the calls and the answers, and the write is the operator's |
| operator.md: a program variable that names something that cannot be run, never falling back to the default (the test seam; #0227) | `test-unmet-seam-cannot-run.ps1` (#0222) | Nothing: the seam is the operator's alone |

**v0.11's rows: board plans, turns, budgets, contract v2 and the ProxyCommand's refusal** (E0.11-T04-S03,
#0241; D88). **RFC 2119 keywords again.** Every behavior that v0.11 adds to lease-api.md, state.md and
operator.md, or changes on them, is shown by a conformance case of T04 against stand-ins, by a test of a
crate, or by D88's live check; **no v0.11 behavior is left with none of the three**, but for what [the gaps
table](#the-gaps) names (D88's last bullet, and [architect.md, Breaking an epic into
stories](architect.md#breaking-an-epic-into-stories), step 5). A "Not shown by a case" cell therefore names the
live check or the crate test that shows it, never "Later".
- **Every "None" this page's coverage tables hold was read for what v0.11 changes** (#0241). **The one
  coverage cell that says `None` is the keeper's installed task, launcher and installer row**, which no v0.11
  story touches. **The `None` cells of [The summary](#the-summary) are the prototype's real folders**, history
  and not coverage. **The cells that say a case reaches nothing, and that v0.11 changes, are named in [the gaps
  table](#the-gaps) and in the rows below**: a board lease's stored record, the TTL's factor, the rows of the
  board read that no stand-in answer reaches, and the captured output a failure's reason quotes.
- **No stand-in can show the real board answering as the stand-in does**, and no case runs the installed
  programs, so D88's live check shows both once the build is installed. **The results of T04's new files are in
  their pull requests; the record of this turn's whole-suite run,
  [docs/runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md), covers the suite, the crates and the box's tests on
  `main`**, and the table below gives no result.

**D88's live check, E0.11-T05 (#0242), on Terry's laptop after the install, starts no instance** (D88: no
instance hours are asked for, and nothing costs money):
- **It reads #0052, v0.11's own epic, with one GET as Bot and writes nothing to the board** (Terry, 2026-11-25,
  Q18: the architect's recommendation, (a)). The plan block in #0052's description was written at layout (D83).
- **Its lease is for a project that is not on the project list**, which D54 answers `unsatisfiable` with
  `no_capacity` before any AWS call, so nothing is issued and none needs a release (D80).
- **It shows the size, `sized_for` and TTL that the same plan given from a file gives** (D88).
- **It changes nothing on the laptop beyond the install**: Terry's ssh config, keeper task and `PATH` are as
  they were (D88).

| Area | Shown by a case | Not shown by a case, and the live check or test that shows it |
|---|---|---|
| lease-api.md: `create --board <id> --epic <ticket>` reads the epic's plan from the card and makes the lease a `--plan` lease with that plan would be (D83, D86) | `test-board-plan.ps1` (#0239), its item 6: a board lease and the same plan through `--plan` give the same size, `sized_for` and TTL, for the busiest turn and for `--turn`; `test-plan-turn-ttl.ps1`'s second half, the same cases from an epic's block | The block's parse and the read over their whole domain: `agentspace-board`'s `tests/block.rs` and `tests/read_epic.rs`, and `agentspace-cli`'s `tests/lease_board.rs`. **The real board answering as the stand-in does**, and the installed programs: D88's live check, E0.11-T05 (#0242) |
| lease-api.md: The board read only reads (D86), and sends the token to the board alone | `test-board-plan.ps1`'s items 3 and 4: no log line of any case holds a method but `GET`, with a control that shows the check can see one, and the token reached the stand-in (`auth=ok`) and appears in no output, no error, no file of the home but the fixture and `bot.json`'s blob, and no command line | That the real board is read once and written nothing: D88's live check makes one GET as Bot |
| lease-api.md: Each refusal of the board read, with its error code, `field` and message (the outcome table, D86) | `test-board-plan.ps1`'s item 1 and 2: every row a stand-in's answer or a credential can bring about, with the requests it makes (one GET for a good epic, two for an unknown board or ticket, one for a bare 404, none for a refusal made before the read) | **The four rows no stand-in answer reaches, each a crate test** (the architect, 2026-11-26, for v0.11-T04, pins P10 and P11), listed in [the gaps table](#the-gaps): `localappdata_unset_empty_or_only_whitespace_is_an_internal_error_and_reads_no_board` (`agentspace-cli`'s `tests/lease_board.rs`), and in `agentspace-board`'s `tests/read_epic.rs` `a_200_that_is_not_the_shape_could_not_be_read`, `a_connection_closed_with_no_answer_does_not_answer` and `an_answer_that_stops_short_does_not_answer`, and `slow_a_board_that_never_answers_is_given_up_on_after_30_seconds` (the case of a board that hangs shows only that the run ends within 60 seconds with `internal`) |
| lease-api.md: The plan block, and a plan nested too deep (D83; the architect, 2026-11-25, for v0.11-T04, pin P5) | `test-board-plan.ps1`: no block, two, a malformed fence, a block never closed and a document `agentspace-plan` refuses, each as `invalid_value` for `epic`; a plan nested 64 levels is check 6 and one nested 100,000 levels is check 4, from a board and from `--plan`, within a bound of time and recording no lease | The block's fence and parse over their whole domain: `agentspace-board`'s `tests/block.rs`. **Nesting from 65 to 999 levels is the build's own** (lease-api.md), and no case asserts it |
| lease-api.md: `create --turn` sizes the lease for the turn it names, `sized_for.turn`, and the second `plan_too_large` message (D84) | `test-plan-turn-ttl.ps1`; `test-plan-sized-for.ps1` and `test-plan-errors.ps1` still show v1's `sized_for` and `plan_too_large` | The turn cut and the sizing over their whole domain: `agentspace-plan`'s `tests/turn_budget.rs` and `agentspace-cli`'s `tests/lease_plan.rs`; the member's record: `agentspace-state`'s `tests/sized_for_document.rs`. The installed programs: D88's live check |
| lease-api.md: `budget_minutes` and the TTL a plan gives from its turn's budget (D85) | `test-plan-turn-ttl.ps1`: each TTL rule, the budget refusals from a file and from a board, and a lease's TTL as `expires_at` minus `created_at` of its own document | `agentspace-plan`'s `tests/turn_budget.rs`. The factor is not part of the contract ([the gaps table](#the-gaps)). The installed programs: D88's live check ("the same plan gives from a file") |
| lease-api.md: contract v2 keeps v1 and v0 for a command line that gives none of the new inputs, and the new `field` values (D87) | `test-v2-compatible.ps1` and `test-v1-compatible.ps1` (its fifth control), and `test-schemas.ps1` over `schemas/`, `schemas/v1/` and `schemas/v0/` (E0.11-T01-S02, PR #163); the thirteen usage cases that carry `field` are `test-plan-turn-ttl.ps1`'s, with `test-board-plan.ps1`'s item 7 beside them, and `test-usage-errors.ps1` and `test-values.ps1` are unchanged | **A client outside the repository** that reads a v2 error: none exists (D87), and no check shows it. The installed programs: D88's live check |
| lease-api.md: `create`'s v2 help | `test-help.ps1`, which reads lease-api.md at run time | The installed programs: D88's live check, with E0.11-T05's install |
| state.md: What a plan lease adds, a board lease's `plan_source` and the plan as read (D86) | `test-state-schemas.ps1`: the lease state file's `plan_source` sample and its invalid ones | **No lease CLI case reads the stored record** ([the gaps table](#the-gaps)): `agentspace-state`'s `tests/plan_source.rs` and `agentspace-cli`'s `tests/lease_board.rs` show what `create` writes |
| operator.md: the ProxyCommand refuses a lease alias whose lease is not `ready`, and one whose lease records cannot be read (#0229; D49 as changed for v0.11-T01, pins P21 to P23) | `conformance/operator/test-unmet-proxy-not-ready.ps1` (E0.11-T02-S04, #0237) and the cases of `test-unmet-host-proxy-lookup.ps1` that #0237 changed | Nothing more is claimed: D88's live check does not include it, and the refusal is made before any call, so the case's recording stand-in `aws` and its tripwires show that nothing ran |
| The suite's own files: a provision case's failure quoting a log line it cannot parse (#0230) | `test-unmet-host-provision.ps1`'s reason, and `Get-HpLogShape` through `ConvertTo-HpMasked` (E0.11-T04-S02, #0240); the mask's own proof is S02's planted checks under its `target\`, not committed | The rest of the captured output a failure's reason quotes: #0243 ([the gaps table](#the-gaps)) |

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

**The suites left in `scripts/` had to keep passing until the rewrite replaced them**, and the
conformance suite added to them and retired none. E0.06-T06-S01 (2026-11-20) removed them with the
prototype, and the conformance suite is now the only one.

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
   in the usage text.** keeper.md's Q3, the name of the rewrite's entry point, is closed
   (D37: `agentspace-keeper.exe`, [docs/keeper.md](keeper.md#command-line)), and the variable
   still hides the name from the suite.
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
   PowerShell 7 session, and the prototype's `create` needs `AGENTSPACE_PROXY_COMMAND` set. Without
   it, the prototype's default ProxyCommand fails `create` with `internal` in a checkout that holds
   no `.instance-id`. **That need is closed by D21's ruling (architect, 2026-11-18, #0080): `create`
   MUST NOT depend on the instance-ID cache**, which the ProxyCommand reads at connect time
   ([state.md](state.md#the-alias-file), [driver.md](driver.md#how-it-is-reached)). So it is not a
   requirement the specs left out, but a difference of the prototype's that the spec drops, and
   `conformance/lease-cli/test-unmet-default-transport-no-cache.ps1` asserts it.
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
5. **Later, not blocking: MAY the live-box check become a conformance check?** It loads no prototype
   code but needs a real box and a ready lease. Whether it moves, runs by hand, or is dropped is
   not settled. **Reason:** it checks the box's sshd, which the rewrite of the laptop's programs
   does not change, and no build's conformance rests on it, so a decision can wait for the
   rewrite's first release.
6. **Later, not blocking: where does a check that needs the clock (the expiry second) get one?**
   The state file's times can only be seeded far past or far future. A rewrite may offer a test
   clock, which the spec would have to document, or the check stays out of the suite. **Reason:**
   the rule that a lease is `expired` exactly at `expires_at` is written in
   [lease-api.md](lease-api.md#states) and is checked a long way from the boundary (a record seeded
   far in the past, and one far in the future), so a build can be built and shown to conform to
   everything but the one second, and a test clock would be an addition to the contract, which
   v0 does not need. B-52 asked for this; it stays later, with the one second unchecked.

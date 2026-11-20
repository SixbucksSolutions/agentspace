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
below are **pinned** (the architect, 2026-11-18; the fifth, `AGENTSPACE_PEER_CMD`, D38, 2026-11-19).
The harness reads `AGENTSPACE_CMD` and `AGENTSPACE_OPERATOR_CMD` (E0.04-T03-S01, #0084); the checks
of E0.04-T03-S02 (#0085) read the operator's and ProxyCommand's by `Get-ConformanceProgram` too, and
`test-interop.ps1` (E0.05-T02-S05, #0103) reads the peer's. **A variable that is unset or empty
takes its default from `conformance/prototype/defaults.ps1`, and from nowhere else**; a variable
that is set always wins. **Every default is a path in that file, relative to its folder**, and
each names a program in `conformance/prototype/`, except the first two, which name the prototype's
`bin\agentspace.cmd`.

| Variable | Names | Default, as `conformance/prototype/defaults.ps1` gives it |
|---|---|---|
| `AGENTSPACE_CMD` | The lease CLI, run as `& $env:AGENTSPACE_CMD lease ...` with its arguments unchanged | `..\..\bin\agentspace.cmd`, the prototype's lease CLI |
| `AGENTSPACE_PEER_CMD` | A second lease CLI, run the same way as `AGENTSPACE_CMD`, and read only by the one check that drives one lease through two programs ([Running the suite](#running-the-suite); D38) | `..\..\bin\agentspace.cmd`, the prototype's lease CLI, given exactly as `AGENTSPACE_CMD`'s default is |
| `AGENTSPACE_OPERATOR_CMD` | The operator (`deploy`, `up`, `sync`, `ca`, `down`, `status`, `grow`) | `operator.cmd`, a shim that starts the prototype's operator under Windows PowerShell 5.1 |
| `AGENTSPACE_KEEPER_CMD` | The keeper | `keeper.cmd`, a shim that starts the prototype's keeper under PowerShell 7 |
| `AGENTSPACE_SSH_PROXY_CMD` | The ProxyCommand program | `ssh-proxy.cmd`, a shim that calls the prototype's `scripts\ssh-proxy.cmd` |

**Why five and not one.** The story card asked for `AGENTSPACE_CMD` for the lease CLI and the
operator. The prototype has them as two programs under two runtimes (the lease CLI under
PowerShell 7, the operator under Windows PowerShell 5.1), and the keeper and the ProxyCommand are
two more, so each needs a name. **The fifth is not another role**: `AGENTSPACE_PEER_CMD` is a second
lease CLI, so that one check can run a lease through two builds, each way round (D38). **Every other
check ignores it.** With both lease CLI variables unset, the two programs are the prototype's, and
the check runs the prototype against itself.

**Each variable names a program that runs its role with the command line its spec gives**
(architect, 2026-11-18): `AGENTSPACE_CMD` and `AGENTSPACE_PEER_CMD` with [lease-api.md](lease-api.md#command-lines)'s,
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
  **`AGENTSPACE_CMD` and `AGENTSPACE_PEER_CMD` are one role, so they MAY name one program**:
  `conformance/prototype/defaults.ps1` gives both the prototype's lease CLI, and a rewrite run against
  itself names its own lease CLI in both.
- **A rewrite's variables name its installed programs** ([operator.md, Programs and install layout](operator.md#programs-and-install-layout);
  D37): each is the full path of one program in the install folder, such as
  `agentspace-ssh-proxy.exe` for `AGENTSPACE_SSH_PROXY_CMD`. A check never names the file itself.
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
    five cases the spec names, in every command that has one, and about 30 usage errors that MUST
    carry no `field`.
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
  message to say (that a failed lock wait says to try again) and for nothing more.
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
| `test-interop.ps1` | **One lease through two programs, `AGENTSPACE_CMD` and `AGENTSPACE_PEER_CMD`**, each way round: it enforces [state.md, Between the prototype and the rewrite](state.md#between-the-prototype-and-the-rewrite), that the two MUST share one home folder and one state folder. See below | Yes. SKIPPED when no CA could be made, and four results are SKIPPED when no `ssh-keygen` is on the `PATH` |
| `test-id-whole-value.ps1` | The lease ID matched as a whole value, for `get`, `renew` and `release`: a trailing dot refused (`<id>.`, `<id>..`, `<id> .`, `<id>.` and a line feed) and changing nothing, a trailing line feed trimmed and the lease found, and `not_found`'s normalized `lease_id` ([Lease ID](lease-api.md#lease-id), [Input tolerance](lease-api.md#input-tolerance)) | For the cases that look at a lease. The refusals and `not_found` need none. SKIPPED for the rest when no CA could be made |
| `test-ca-public-key-words.ps1` | The CA check's two words compared exactly: a first word in another case, a second word with its letters' case swapped and a key type that is not ed25519, a line break (LF, CR, VT, FF or CRLF) between the two words, and a no-break space (U+00A0) before the first word each fail `create` and `renew`, and the same words with another comment, surrounding whitespace, or a tab or two spaces between the two words (each also for `renew`; #0136) are accepted ([state.md, Its checks](state.md#its-checks), check 4, architect, 2026-11-19, as corrected). 37 results | Yes. SKIPPED when no CA could be made |
| `test-relative-folders.ps1` | A relative `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` resolved against the working folder, and the alias file's full paths ([state.md, The home folder](state.md#the-home-folder)). A relative `USERPROFILE` is not given: the harness refuses it | Yes. SKIPPED when no CA could be made |
| `test-home-userprofile.ps1` | The lease CLI's files under `USERPROFILE` when the state and ssh folder variables are unset or empty ([state.md, The home folder](state.md#the-home-folder), [lease-api.md, Environment](lease-api.md#environment)) | Yes. SKIPPED when no CA could be made |
| `test-lease-ignores-instance-cache.ps1` | The lease CLI never reads or needs the instance-ID cache: `create`, `get`, `renew` and `release` succeed with a folder, a file that is not JSON and a valid cache at its place ([state.md, The alias file](state.md#the-alias-file)) | Yes. SKIPPED when no CA could be made, and one result is SKIPPED when `create` fails under the default transport with no cache at all with a valid `internal` error (see below); any other failure of that `create` is a FAIL |

**The first ten rows are the ten checks that this page and the run records mean when they say "the
ten", and `test-interop.ps1` is the eleventh.** The five from `test-id-whole-value.ps1` on are
E0.06-T04-S03's (#0133, #0110 point 4), and four `test-unmet-` files beside them
([The files that are expected to fail](#the-files-that-are-expected-to-fail)) are its too.

**The helpers the checks share are three files.** `conformance/lease-cli/lease-cli-common.ps1` serves
S01's five, and `conformance/lease-cli/lease-check-support.ps1` serves the next five, which are
E0.04-T03-S03's, and `test-interop.ps1` (E0.05-T02-S05, #0103). `conformance/lease-cli/lease-check-hardening.ps1` serves E0.06-T04-S03's nine files (#0133): a run's error problems without the program's own text, the lease ID refusal, and the map of every file under the home that shows a failed command changed nothing.

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
  - **No `ssh-keygen` is on the `PATH`** (two checks of `test-create-get.ps1`, and four results of
    `test-interop.ps1`, which reads each renewed certificate with it).
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
- **A check takes from about 20 seconds to about 2 minutes against the prototype, and the ten about
  10 minutes** (`test-interop.ps1` about 40 seconds, on top), most of it starting programs: about
  0.5 seconds for a run that issues no credential, and a few seconds for a ready `create`. `test-lease-concurrency.ps1` is the longest, because
  round F waits out the lock's 30 seconds. Against a program that prints nothing, all ten take
  about a minute.

### Running the suite against a build's output folder

**RFC 2119 keywords again.** This is how v0.05's Rust build is run (E0.05-T04-S01, #0107, D38), and
how any later build that ships in a `target\dist` folder is. [The run record](runs/e0.05-t04-s01.md)
is the first, and shows the results.
1. **Build the output folder from the workspace's root, with `cargo xtask dist`**
   ([operator.md, The install folder](operator.md#the-install-folder)). It writes `target\dist\`,
   and the lease CLI is `target\dist\agentspace.exe`. **The run MUST be made from a build of a
   commit, and the run record MUST name that commit**, so that a pass belongs to code that can be
   fetched again.
2. **Set `AGENTSPACE_CMD` to the full path of `target\dist\agentspace.exe`**, in PowerShell 7, with
   every other `AGENTSPACE_*` conformance variable unset. The operator that makes each throwaway CA
   then takes its default, the prototype's (`AGENTSPACE_OPERATOR_CMD`), because v0.06 (#0055) builds
   the Rust one.
3. **Run every file, each as `pwsh -NoProfile -File <file>`:** every
   `conformance\lease-cli\test-*.ps1` except `test-interop.ps1`, so the ten checks and every
   `test-unmet-*` file, and `conformance\schemas\test-schemas.ps1`, which names no program.
4. **Run `test-interop.ps1` each way round, against the prototype as the peer:**
   - **A is the build:** `AGENTSPACE_CMD` is the build and `AGENTSPACE_PEER_CMD` is unset, so the
     peer is the prototype by default.
   - **B is the build:** `AGENTSPACE_PEER_CMD` is the build and `AGENTSPACE_CMD` is unset, so A is
     the prototype by default.
5. **A complete run exits 0 on every file, with no SKIPPED result, on a machine whose zone is not
   UTC.** Each `test-unmet-*` file is a requirement the build MUST meet, so against a build its
   `FAIL` is the build's failure ([The files that are expected to fail](#the-files-that-are-expected-to-fail)).
6. **The empty-program run MUST be made too**: `AGENTSPACE_CMD` naming a `.cmd` of `@exit /b 0`,
   for every lease CLI file, and `test-interop.ps1` with it as A and as the peer. Each MUST exit 1.

### `test-interop.ps1`: one lease through two programs

**This check enforces [state.md, Between the prototype and the rewrite](state.md#between-the-prototype-and-the-rewrite)**
(#0054, D38): the prototype and the rewrite MUST be able to share one home folder and one state
folder. RFC 2119 keywords again.
- **It runs two lease CLIs, A and B, in one throwaway home with one state folder.** A is the program
  `AGENTSPACE_CMD` names and B is the program `AGENTSPACE_PEER_CMD` names, each started by the
  harness's `Invoke-ConformanceProgram` as a separate process. **Each variable that is unset or empty
  takes its default from `conformance/prototype/defaults.ps1`, and from nowhere else**, so a run with both
  unset runs the prototype against itself, and a run of the rewrite against the prototype sets one
  variable. The check names no prototype path and loads no prototype code.
- **It runs two rounds, A then B and B then A.** In a round, the first program creates and the other
  program checks, in this order:
  1. The first program creates a ready lease, and the lease has its folder, its key pair and its alias
     file, whose first line starts `# agentspace lease <id>;` (state.md's checklist for the other
     program).
  2. The other program's `get` prints that lease with the same values, compared parsed.
  3. The other program's `list` holds that lease once, with the same values, compared parsed.
  4. The other program renews it with `--ttl 90m`. The renew prints the lease asked for, unchanged
     on every field but `expires_at` and `ttl_seconds`. The first program's `get` prints the same
     document, and the certificate on disk is a new one, valid until that `expires_at`.
  5. The first program renews it again, without `--ttl`, and the other program's `get` shows that
     renewal the same way.
  6. The other program releases it. The release prints the lease asked for, unchanged on every
     field but `state`, `ssh` and `released_at`. The first program's `get` prints the same document,
     `released`, with the other program's `released_at`, and its alias file and lease folder are gone.
  7. The first program releases it again, after the clock has passed that `released_at`. It exits 0
     and prints the same document.
  8. An unsatisfiable lease that the first program created reads the same in the other, by `get` and by `list`.
- **A document a program prints MUST be the lease that was asked for.** Each `renew` and `release`
  document, and each `get` after one, carries the lease's `id`, compared exactly. A `get` is compared
  in full, parsed, with the document the other program just printed, since both are the lease as it
  stands, seconds apart. A `renew` is compared with the document before it on every field except
  `expires_at` and `ttl_seconds`, and a `release` with the last `renew` on every field except
  `state`, `ssh` and `released_at` ([lease-api.md, renew](lease-api.md#agentspace-lease-renew), [release](lease-api.md#agentspace-lease-release)).
  Without that, a program that printed some other lease, or changed `created_at`, would pass on the
  fields the steps look at.
- **After every step, the state file validates against its schema** (`schemas/state/leases.state.schema.json`),
  **and every lease either program has printed is in it exactly once.** A lease counts as printed once
  a document that passed its schema has carried its ID.
- **It checks MUSTs only, and a reason never quotes a program's output**: a reason names what
  differs, with the exit code and whether each stream was empty.
- **It follows the rule for an untrusted program** ([below](#the-program-under-test-is-untrusted)): a
  path is built from a lease ID or a host alias only after the document passed its schema, and then
  only through `Get-CcLeaseDir` and `Get-CcAliasPath`. It writes and deletes nothing itself.
- **Set either variable to a program that prints nothing and exits 0, and the run MUST fail.** A
  program that cannot create fails step 1 of its own round, and one that cannot read fails step 2 of
  the other round, so neither variable can be left unread. Shown on 2026-11-19 against the prototype:
  36 passed, 0 failed, 0 skipped, exit 0, in about 40 seconds; with `AGENTSPACE_CMD` naming such a
  program, 9 passed and 27 failed, exit 1; and with `AGENTSPACE_PEER_CMD` naming it, 17 passed and 19
  failed, exit 1. The results that still pass then are mostly the state file's, which hold when a
  program printed nothing for them to find.
  **A program that prints the wrong lease is caught too.** Shown the same day with a wrapper around
  the prototype's command line, as `AGENTSPACE_PEER_CMD`, that changed one behavior: a `renew` that
  printed another lease's `id` failed 4 results (32 passed), a `renew` that changed `created_at`
  failed 10 (26 passed), and the wrapper changing nothing passed all 36.

### The harness

**`conformance/lease-cli/harness.ps1` is dot-sourced by every lease CLI check, and it is the only
code that starts the program under test.** A check makes each home it needs with
`New-ConformanceHome`, removes it in a `finally`, and ends with `Exit-ConformanceRun`. The helpers
the checks share, which start no program of their own, are in `conformance/lease-cli/lease-cli-common.ps1`.

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
| `Assert-ConformanceHomeRoot -Root <p>` | Returns the full path of a home's root, or throws unless it is a folder named `agentspace-conformance-<something>` directly under `%TEMP%` and, when it exists, is not a reparse point. **It is the one write check that is not `Assert-ConformancePath`**: that one measures a path against a home's `Root`, and this one is what makes the `Root` safe to measure against. It guards `New-ConformanceHome`'s creation of the `Root`, and the deletes of `Remove-ConformanceHome` and of `New-ConformanceHome`'s own cleanup. A check that writes inside a home calls `Assert-ConformancePath`, never this (E0.06-T04-S01, #0110 point 2) |
| `Remove-ConformanceHome -ConformanceHome <h>` | Deletes the home's `Root`, and only it. It refuses a folder that is not an `agentspace-conformance-*` folder directly under `%TEMP%`, and a `Root` that is a junction or a symbolic link, and it checks again at each delete (`Assert-ConformanceHomeRoot`) |
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
  - **Every other `AGENTSPACE_*` variable the caller holds is removed**, so that a shell's own
    settings never reach the program.
  - **`PSModulePath` is removed.** A PowerShell 7 session's value carries PowerShell 7's own module
    folders, and Windows PowerShell 5.1, which runs the prototype's operator, then cannot load its
    security module, so its `ca` fails.
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
  checks that path too, and a `Move-Item` checks both ends. A path that is outside the throwaway home
  makes the check throw, and the check reports FAIL. **No new function was needed**: the call is
  `Assert-ConformancePath` itself, used inline.
- **Two things are written without `Assert-ConformancePath`, and each is named** (#0110 point 2):
  - **The home's own root.** `New-ConformanceHome` creates it, and `Remove-ConformanceHome` and
    `New-ConformanceHome`'s cleanup delete it, through `Assert-ConformanceHomeRoot` at the call (a
    folder named `agentspace-conformance-*` directly under `%TEMP%`, and not a junction or a symbolic
    link). `Assert-ConformancePath` cannot guard its own reference, so this one check does.
  - **A stand-in's own writes.** The `.cmd` stand-ins of `New-OperatorStandIn` and the ssh stand-in
    script of `test-unmet-push.ps1` log and record beside themselves, under the home's `standins`
    folder. They are programs outside agentspace that the check places, and a check does not make
    those writes.
- **Shown by an audit of the source, not of a run** (E0.06-T04-S01): every line of
  `conformance/lease-cli/` and `conformance/operator/` that names a write verb (the calls above, and
  `Tee-Object`, `Clear-Content`, `Set-Acl`, `[IO.File]::Open`, `Delete`, `Replace`, `CopyTo`, `icacls`)
  carries `Assert-ConformancePath` or `Assert-ConformanceHomeRoot`, except the three lines of the ssh
  stand-in script that write, which are the stand-in's own. The search was first run against lines
  known to write, and found them, so that a zero would not be believed blind. **#0110 points 1 and 2
  are done.**
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

### What the first run proved

**It answers [open question 3](#open-questions).** On 2026-11-18, on the laptop, with `USERPROFILE`
set to a new folder under `%TEMP%` (the OpenSSH client still finds the real home through the
account, so the real profile is not unreachable), the operator's
`ca` made a CA there and exited 0, a second `ca` exited 0 and changed nothing, and a ready
`lease create` then issued a lease, wrote its alias file and its credential under that home, and
printed a document that validates.

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
**`scripts/test-push-localswim.ps1` stays prototype code and keeps passing**, with every group of it
below. Four of its groups needed no stand-in and moved earlier, to `conformance/operator/`, in two
kinds: the refusals of `-Alias` (two groups) and the refusals of the ProxyCommand's arguments (two
groups).

#### `conformance/operator/`: conformance

**Nine checks, one helper file and no shim path.** Each check dot-sources `helpers.ps1`, which
dot-sources the harness (`conformance/lease-cli/harness.ps1`), so the programs are named by
`AGENTSPACE_OPERATOR_CMD` and `AGENTSPACE_SSH_PROXY_CMD` and run in a throwaway home with the
harness's whole environment ([The harness](#the-harness)). Each ends with `Exit-ConformanceRun`, and
each is run as `pwsh -NoProfile -File conformance/operator/<file>.ps1`.

| File | What it enforces | Against the prototype |
|---|---|---|
| `test-alias-refusals.ps1` | `up -Alias <bad>` and `status -Alias x` are refused before anything runs, with exit 1, the message and an empty stdout: 11 bad values (`a;b`, `a b`, `a&b`, the empty value, `a\|b`, a value with a double quote, `-Fx`, `a/b`, `a%b`, the Kelvin sign U+212A and U+0130) and the `status` case ([docs/operator.md](operator.md#command-line)) | 12 PASS, exit 0 |
| `test-proxy-arguments.ps1` | The ProxyCommand refuses a second argument before `up`, and fails in `up` on a bad alias with no tunnel started: `a/b extra`, `a;b`, `-Fx`, the empty argument, `a b` and `a/b` ([docs/operator.md](operator.md#the-proxycommand)). The refusal is matched as `^agentspace: \S+ takes at most one argument, the SSH host alias\.$`, since it names the program's own file (B-29), and `a;b` accepts either that refusal or the `-Alias` text, both exit 1, because a `.cmd` program splits it at the semicolon and a native one does not (B-28) | 6 PASS, exit 0 |
| `test-ca.ps1` | `ca` ([docs/operator.md](operator.md#ca), [docs/state.md](state.md#the-ca)): the OneDrive refusal; the first run (four lines on stderr, an ed25519 pair with no passphrase, the printed fingerprint); a later run that changes nothing; `-Quiet`; a private key alone, a public key alone and a public key that is not the private key's own, each refused and not repaired; and that `ca` starts no AWS CLI, `ssh` or `scp` | 10 PASS, exit 0 |
| `test-unmet-push.ps1` | The token push through the seam ([docs/operator.md](operator.md#the-token-push)): the ssh arguments, the two content lines on stdin, the stamp, the messages; an unchanged push sending nothing; a changed zone and a deleted stamp each making it due; a nested `up`; a missing receipt; an unreadable credential; an ssh that fails; a box that refuses the zone; a zone that fails the check; an ssh that hangs (the 20 seconds); the token's secrecy | FAIL, as expected: `seam not met` |
| `test-unmet-proxy-steps.ps1` | The ProxyCommand's steps through the seam ([docs/operator.md](operator.md#the-proxycommand)): the two folders first on `PATH`; `up` before the tunnel; the tunnel's exact arguments for the cached instance; the tunnel's own stdin and stdout; the tunnel's exit code; no alias and an alias alike; exit 1 with no tunnel when `up` fails; an expired or missing AWS sign-in gives exactly the sign-in message ([AWS failures](operator.md#aws-failures)) | FAIL, as expected: `seam not met` |
| `test-unmet-proxy-connect-time.ps1` | D1 at connect time, card #0087, through the seam: on success the caller's line `The host is ready.`, no `Sending ...` line and the tunnel's stderr withheld; on failure the exact "Could not reach the host" line last, and no `Sending ...` line ([The files that are expected to fail](#the-files-that-are-expected-to-fail)) | FAIL, as expected |
| `test-unmet-commands.ps1` | `down`, `status` and `grow` through the seam ([docs/operator.md](operator.md#down), [status](operator.md#status), [grow](operator.md#grow)): each command's calls in order with their arguments, its messages and its failures, and `grow`'s failed online grow (D36). `status` expects each failed call's failure line alone, with no partial report before it, as [operator.md, status](operator.md#status) says (architect, 2026-11-19). Written by E0.06-T04-S02 (#0132, gap FB-25). **96 results** | FAIL, as expected: `seam not met` |
| `test-unmet-usage-help.ps1` | The operator's usage errors with their exact text, their order, what is accepted, every help block byte for byte, and the system-folder refusals ([docs/operator.md](operator.md#command-line)); **no case runs a program with `USERPROFILE` unset or empty** ([The harness](#the-harness)). Written by E0.06-T04-S02 (#0132, gap FB-25). **143 results** | FAIL, as expected: the prototype has no help and reports each usage problem as its shell's own error record |
| `test-unmet-start-paths.ps1` | The start-and-wait step of `up` through the seam, with a stand-in `aws` whose answers change from call to call ([docs/operator.md](operator.md#starting-the-box-and-waiting-for-it), [What up waits for](operator.md#what-up-waits-for)): the stopped, stopping, pending and running boxes, the 20-minute rule, any other state, a launch time that is not of the form, the wait's messages and poll, the patch question's answers, and the 15-minute deadline, which **neither runs nor is reported by default and runs with `-Case deadline`** ([Cases that run only when asked for](#cases-that-run-only-when-asked-for)). Written by E0.06-T04-S02 (#0132, gap FB-25). **Its default run is 31 results, all PASS, exit 0; `-Case deadline` runs 5 (the seam probe, the deadline's 3 and the ssh and scp tripwire), and the two runs hold 34 distinct results between them** | FAIL, as expected: `seam not met`, one result |
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
  ([docs/operator.md](operator.md#what-the-box-does)) and records its arguments, its stdin and its
  environment. The token is a made-up value, and the alias is a name no ssh configuration holds, so
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
- **The run:** `-Case 'Watch: a locked pass*'`, 5 times in a row, against `target\dist\agentspace-keeper.exe`
  built from `main` at 5430289, while two other copies of the whole keeper suite ran against the
  same build at the same time, as separate `pwsh` processes the run started. **5 of 5 exited 0**, each
  5 passed, 0 failed, 0 skipped, in 18 to 19 seconds (alone, about 20). The two copies were still
  running when the fifth ended, and each ended 208 passed, 0 failed, 0 skipped, exit 0.
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

#### The three installer cases: prototype code

**These stay in `scripts/test-board-holder.ps1`, whole and untouched, as prototype code.** The
rewrite MUST NOT read them. They are the three cases that load or test the prototype's packaging:

| Case | Count | Class | Notes |
|---|---|---|---|
| Installer: `-DryRun` prints the logon task and changes nothing (about 28 checks: task name, two triggers, repetition, run level, battery, restart count, the working directory, the action, the arguments) | 1 case | Prototype code (packaging) | The expected values are the prototype's packaging (a launcher program, the repository root as working directory). It also reads Terry's real scheduled task, read only, before and after, which no conformance check does. A rewrite installs its keeper its own way ([docs/keeper.md, The installer](keeper.md#the-installer); D37) |
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
  "Output"). Nothing else changed: it checked the same 289 things as before, and it still scans the
  top level of `schemas/` only. The samples E0.04-T04-S01 added bring it to 293, all passing
  against the package on 2026-11-18. **The three keeper schemas in `schemas/state/` are checked by the
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

## The files that are expected to fail

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
| `conformance/operator/test-unmet-proxy-connect-time.ps1` | The ProxyCommand names no backend on success, prints agentspace's own message on failure, and prints backend detail only under `AGENTSPACE_DIAGNOSTIC` ([docs/operator.md](operator.md#what-a-caller-sees-d1-card-0087)), asserted through the seam that names the AWS CLI, `AGENTSPACE_AWS_CMD` ([The test seam](operator.md#the-test-seam)) | #0087 (D1), and #0085 for the seam | FAIL on both paths, `seam not met (AGENTSPACE_AWS_CMD was not used)`: the prototype runs the AWS CLI by a fixed path, so the stand-in is never called. Nothing more is asserted until the seam is met | Not a lease CLI row: the Rust build has no operator and no keeper, which v0.06 (#0055) builds |
| `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1` | A keeper that tracks a holder, and whose pass does not find it trusted (its process gone while the pass waits for `board-holder.lock`, or its record gone), starts no holder in that pass and waits the release grace after it, logged `grace seconds=<n>` ([docs/keeper.md](keeper.md#release-grace)), and the pass logs `lost` for the tracked holder ([docs/keeper.md, Log lines](keeper.md#log-lines); D35) | #0068 | FAIL on 4 checks of each of its 2 cases: the pass logs `started` at once, it logs no `lost` line, the line after it is not a grace line, and the next start is not the grace after the pass was let go. Nothing else fails ([The #0068 file](#the-0068-file-test-unmet-holder-dies-mid-passps1)) | Not a lease CLI row: the Rust build has no operator and no keeper, which v0.06 (#0055) builds |
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
| `conformance/operator/test-unmet-push.ps1` | The token push through the test seam: the ssh arguments, the two content lines, the stamp, the messages, the due and not-due cases, the zone checks, the hung ssh's 20 seconds and the token's secrecy ([operator.md, The token push](operator.md#the-token-push); [The test seam](operator.md#the-test-seam)) | #0085 (the seam) | FAIL on the first result, `seam not met (a stand-in was not used)`: the prototype runs the AWS CLI and `ssh` by fixed paths, so no stand-in is called. Nothing more is asserted until the seam is met | Not a lease CLI row: the Rust build has no operator and no keeper, which v0.06 (#0055) builds |
| `conformance/operator/test-unmet-proxy-steps.ps1` | The ProxyCommand's steps through the test seam: the two folders first on `PATH`, `up` before the tunnel, the tunnel's exact arguments for the cached instance, the tunnel's own stdin and stdout, its exit code, exit 1 with no tunnel when `up` fails, and the sign-in message for an expired sign-in ([operator.md, The ProxyCommand](operator.md#the-proxycommand)) | #0085 (the seam) | FAIL on its first result, `seam not met (AGENTSPACE_AWS_CMD was not used)`: the prototype runs the AWS CLI by a fixed path | Not a lease CLI row: the Rust build has no operator and no keeper, which v0.06 (#0055) builds |
| `conformance/operator/test-unmet-commands.ps1` | `down`, `status` and `grow` through the test seam: the calls in order with their arguments, the messages exactly, the failures, and `grow`'s failed online grow ([operator.md, down](operator.md#down), [status](operator.md#status), [grow](operator.md#grow); D36) | #0132 (E0.06-T04-S02, gap FB-25) | FAIL on its first result, `seam not met`: the prototype runs the AWS CLI and `ssh` by fixed paths | Not a lease CLI row: v0.05's build has no operator |
| `conformance/operator/test-unmet-usage-help.ps1` | Every usage error with its exact text and order, and every help block byte for byte ([operator.md, Command line](operator.md#command-line); the Help block of each command) | #0132 (E0.06-T04-S02, gap FB-25) | FAIL: the prototype has no help of its own, and reports each usage problem as its shell's own error record, with no message of agentspace's | Not a lease CLI row: v0.05's build has no operator |
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

## The gaps

**A gap is a spec section that no sorted check covers.** The sorted checks leave these.

| Spec section | Gap |
|---|---|
| operator.md: The token push | **Covered by `conformance/operator/test-unmet-push.ps1`**, through the seam, and it fails against the prototype until a build meets the seam ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). What it leaves is in [Coverage](#coverage-of-the-operator-and-the-keeper) |
| operator.md: The time zone | The seven legacy names and the zone check are covered by `test-unmet-push.ps1`. The conversion of a Windows zone ID is not, and [Coverage](#coverage-of-the-operator-and-the-keeper) says why |
| operator.md: Commands (`deploy`, `up`, `sync`, `down`, `status`, `grow`), Starting the box and waiting for it, The box as the laptop sees it, The AWS side | `up` runs through the seam in `test-unmet-push.ps1` and `test-unmet-proxy-steps.ps1`, as far as the calls they stand in for go. **`down`, `status` and `grow` are covered by `test-unmet-commands.ps1`, and the start-and-wait step of `up` (the stopped and stopping boxes, the wait and its deadline, the patch question's answers) by `test-unmet-start-paths.ps1`** (E0.06-T04-S02, #0132), each failing against the prototype ([The files that are expected to fail](#the-files-that-are-expected-to-fail)). `deploy` and `sync` have no check, and [Coverage](#coverage-of-the-operator-and-the-keeper) says why |
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
| driver.md | The driver boundary is in-process, so no conformance check can drive it. The registration checks are not movable. What a caller sees of the driver is covered by `conformance/lease-cli/test-lease-driver-outcomes.ps1`: unsatisfiable requests, no driver name in any output, and a failed alias step. A foreign alias file kept by a failed `create` is not movable, since the alias name comes from a random ID |
| keeper.md: The installed task | No conformance check covers an installed task. The three installer cases stay in `scripts/` as prototype code, and [Coverage](#coverage-of-the-operator-and-the-keeper) says why |
| keeper.md: Release grace, card #0068 | **Checked by `conformance/keeper/test-unmet-holder-dies-mid-pass.ps1`, which fails against the prototype** ([The #0068 file](#the-0068-file-test-unmet-holder-dies-mid-passps1)). The gap that remains is the window itself: the keeper logs nothing when its look finds a holder alive and its pass then waits for `board-holder.lock`, so no event marks the moment between the look and the trust check. The file enters it with one bounded wait from the keeper's own `skipped` line, reads the log to see which path the keeper took, and reports SKIPPED if it missed. The second case in the file reaches the same rule with no timing, through a record that is gone |
| keeper.md: Checking the keeper, card #0066 | **Shown for the Rust keeper (E0.06-T04-S01):** the "Watch locked" case passed 5 times in a row beside two other copies of the suite ([The keeper suite](#the-keeper-suite)). **Not shown:** the same under the prototype's keeper, a heavier load, which no run was made with |
| keeper.md: The task's launcher; Stopping and starting the keeper | Nothing runs the launcher, the install, or the End and restart steps: they need the real Task Scheduler |
| keeper.md: Starting a holder | The refusals of "Before the start" (a state folder or a keeper path holding a `"`, an alias outside the form), the create retries, the 10-second bound on one process-create call (D35) and the 35-second, 60-second and 95-second bounds have no case. The first needs a path that holds a `"`, which Windows refuses, and an alias outside the form, which the lease state's schema refuses; the rest need a launcher that does not answer, or a create call that does not return |
| keeper.md: Locks | The `board-holder-keeper.lock` error path (any error other than the file being held) and the `board-holder.lock` error that is not "in use" have no case |
| state.md: The keeper's files | The share modes and the access rules of the keeper's files, and the temporary files of a killed pass, have no case; the new case checks the JSON files' schemas, bytes and the launch files' removal only |
| The live-box check | `check-forward-release.ps1` is the only check of the box's sshd, and the conformance suite does not run it |

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
| operator.md: The token push | `conformance/operator/test-unmet-push.ps1` (20 results, through the seam) | **Later, not blocking:** what the box does with the content (the box's half, steps 1 to 8 of [What the box does](operator.md#what-the-box-does)) runs only on a host, and the check's stand-in `ssh` is its own reading of that half, so a real box's half is shown only by the live-box check ([Q5](#open-questions)). **Later, not blocking:** `sync`'s forced push needs the build and copy steps, which the seam does not stand in for (operator.md Q30, which is itself later) |
| operator.md: The time zone | The seven legacy names, and eight zones that fail the zone check (`test-unmet-push.ps1`) | **Later, not blocking:** the conversion of a Windows zone ID to an IANA name, because the seam replaces the laptop's zone before it, so a check would need a machine set to each zone; the spec's table is the requirement |
| operator.md: `ca` | `conformance/operator/test-ca.ps1` (10 results) | **Later, not blocking:** the refusal when run elevated (the suite MUST NOT run elevated), a machine with no `ssh-keygen.exe` (the program is found by its fixed path), and the access rules of the CA's folder and key (a check of a loosened key would grant another account access to a private key, which no check does; the one rule a check sets is `test-unmet-failed-create-cleanup.ps1`'s, a deny for the current user on a folder of its throwaway home, lifted in a `finally`) |
| operator.md: `deploy`, `sync` | None | **Later, not blocking:** they run `go`, `ssh-keygen`, `scp` and a CloudFormation deploy, and the seam stands in for the AWS CLI, `ssh` and `scp` alone, so a check could not stop `go` and `ssh-keygen` from running. **Later, not blocking:** `sync`'s call sequence through the `ssh` and `scp` stand-ins with each step's failure message, and `deploy -Quiet` quieting the `sync` it runs (D36). The prototype reads none of the seam variables, so each would be a `test-unmet-` file that fails with `seam not met`, and a check no build could be shown to pass is not written blind |
| operator.md: `down`, `status`, `grow` | **`conformance/operator/test-unmet-commands.ps1`** (E0.06-T04-S02, #0132): each command's calls in order with their arguments, its messages and failures, and `grow`'s failed online grow (D36) | **Later, not blocking:** a real AWS answer's other members and a real `df` table, which a stand-in writes in its own words |
| operator.md: Command line (a missing, unknown or repeated command or parameter, and the help texts) | `-Alias` (`test-alias-refusals.ps1`), and **every usage error, its order and every help block byte for byte** (`conformance/operator/test-unmet-usage-help.ps1`, E0.06-T04-S02, #0132) | **Later, not blocking:** an alias with a line feed in it (the gap above). **Not from outside, by [The harness](#the-harness)'s rule:** the rules for an unset or empty `USERPROFILE`, which the programs' own tests show |
| operator.md: Starting the box and waiting for it | The case of a box that is running and old (`up` does not wait), through the stand-ins in `test-unmet-push.ps1` and `test-unmet-proxy-steps.ps1`; and **the whole step** in `conformance/operator/test-unmet-start-paths.ps1` (E0.06-T04-S02, #0132, 31 results by default and 5 with `-Case deadline`, 34 distinct between the two runs, the deadline's three running only with it: [Cases that run only when asked for](#cases-that-run-only-when-asked-for)): the stopped, stopping, pending and running boxes, the 20-minute rule, any other state, a launch time not of its form, the wait's messages printed once while they do not change, the poll, the patch question's answers (`patched`, `unknown`, an empty and an unexpected answer, a failed or timed-out check), the invocation's own poll and its 20-poll limit, and the 15-minute deadline with its exact message | **Later, not blocking:** the deadline with Session Manager's agent never Online (the file waits its 15 minutes once, with `-Case deadline`, on a box whose patch check never finishes, and a second wait for another phase costs another 15 minutes), the question's own text ([What up waits for](operator.md#what-up-waits-for): no comma, bracket or equals sign, or a form that is not split), which a stand-in cannot tell from another text, and the exact 5-second and 1-second pauses, which the file checks loosely |
| operator.md: The ProxyCommand | The argument refusals, the steps (`test-unmet-proxy-steps.ps1`) and what a caller sees | **Later, not blocking:** the console-mode repair ([Q17](operator.md#open-questions)) needs a Windows console, and the harness gives a program pipes |
| keeper.md: Streams and environment (the holder and its launcher run in the user's logon environment, D35) | `conformance/keeper/test-holder-environment.ps1` | The holder's environment names show neither a sentinel set only for the keeper, nor `AGENTSPACE_STATE_DIR`, nor any `AGENTSPACE_*` name, with `SystemRoot` present as a control, and the holder's `USERPROFILE` is not the keeper's changed one (a yes or no marker, never the value). **Not shown:** the launcher's own environment (only the holder's is seen) and that the keeper itself held the sentinel (set before the start, not observable from outside; the check makes it a precondition) |
| operator.md: The Bot credential (the token push) | `test-unmet-push.ps1` writes its fake credential with PowerShell's `Export-Clixml` | **Later, not blocking:** writing it with `CryptProtectData` directly, as [operator.md, The Bot credential](operator.md#the-bot-credential) describes the file, would make the check independent of PowerShell's own serializer. It changes how a fixture is made and shows nothing new |
| keeper.md: The installed task, the launcher, stopping and starting | None | **Later, not blocking:** the task is the machine's Task Scheduler entry, which a throwaway home does not isolate, so a check would register a real task in the user's account. The installer's usage errors change nothing on the machine ([docs/keeper.md, The installer](keeper.md#the-installer); D37; B-51), so a check of them would register no task, but none is written yet: no conformance variable names the installer ([Naming the program under test](#naming-the-program-under-test)), and it runs only from the install folder (D37), so such a check needs a fifth variable and an install to point it at. The three installer cases stay in `scripts/` as prototype code |

The numbers in "Checked by" are the results of each file, run against the prototype or against a
stand-in of a conforming build, as [The operator and the token push](#the-operator-and-the-token-push)
says.

**Which of these rows v0.06 closes** (architect, 2026-11-19, for v0.06, gap FB-25). Each "Later"
above was later because no build met the seam; v0.06's Rust operator will. **v0.06's stories MUST
write the checks that the seam lets a check make from outside, each in the story that builds what
it checks:** `down`, `status` and `grow` (its failed online grow included); the operator's usage
errors and help texts; the stopped and stopping start paths and the patch question, with a
stand-in `aws` whose answers change from call to call; and the keeper's logon environment (D35).
**These stay later after v0.06, each for the reason its row gives:** `deploy` and `sync`, until
operator.md Q30 stands in for `go` and `ssh-keygen`; the box's half of the push; the Windows zone
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
- **The skip reason that FA-20 found is fixed:** `test-interop.ps1` now says that no `ssh-keygen`
  was found in `System32\OpenSSH` or on `PATH`, the two places it looks.

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

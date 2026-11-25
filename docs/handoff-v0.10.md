# Handoff: agentspace during v0.10

RFC 2119 keywords; the capitals are load-bearing.

> **Live record of v0.10 (#0050), started at T01's close, 3:19pm Mountain on 2026-11-24, and
> refreshed at T02's close, 8:32pm, at T03's close, 11:50pm, and at T04's close, 3:56am on
> 2026-11-25.** It is refreshed at each turn's close and made
> final when Terry approves v0.10. Where this file and AGENTS.md, the board or decisions.md
> disagree, they win, and the disagreement is a bug to fix in this file.

**Written for an architect who picks up v0.10 mid-release.** It carries only what v0.10 changed;
[handoff-v0.09.md](handoff-v0.09.md) stays the reference for the tools, lanes, pitfalls and rules
it lists, and every row of its "What is open" is carried below or says what became of it.

## The short version

- **v0.10 is plan-sized leases and contract v1.** `create --plan <file>` sizes a lease from a
  plan's busiest turn at half a vCPU and 2 GiB per agent at once (D73, Terry's yes to Q14), and
  the lease prints `sized_for`; a plan no size holds is `plan_too_large`. The design is D73 to
  D80, the layout D81, in [decisions.md](decisions.md).
- **T01, the specs, is closed**: [lease-api.md](lease-api.md) is contract v1, `schemas/` has v1's
  schemas with v0's kept under `schemas/v0/`, and [state.md](state.md), [driver.md](driver.md),
  [operator.md](operator.md) and the README carry the plan's record and D77's due rule.
- **T02, the foundations and the hardening, is closed**: `agentspace-plan` parses a plan, counts
  its peak and picks its size; `agentspace-state` keeps the plan and prints `sized_for`; a run is
  due at once after a success (D77); #0191's hangs are found and fixed; #0137, P51 to P53 and
  `Invoke-HostFile`'s holders are met.
- **#0191's hangs were a Windows dialog, not load** (#0225): CreateProcess on a file named `.exe`
  that is not a program raises "Unsupported 16-Bit Application" and waits for a person when the
  caller's error mode is 0. T02 fixed the two start points; T03's #0226 and #0219 make every
  program set its own error mode (D81's change during T02, on Terry's word).
- **T03, the programs, is closed**: the CLI's `--plan`, `sized_for` and `plan_too_large` (#0219);
  a release walks again while a walk ends a client, so no chain outlives it (#0196, #0220); and
  every program sets its error mode first (#0225, #0226), with bootstrap's seam line (#0227).
- **`main` is green**: the whole workspace 5,598 passed and 0 failed, for the first time in this
  release, and the conformance suite 4,060 of 4,061 (T03's merge check, below).
- **No turn runs on EC2 and v0.10 asks for no instance hours** (D81; Q16). Both hosts are stopped.
- **T04, conformance, is closed**: the lease CLI's plan, contract v1 and D77 cases (#0221), the
  operator's `last_success_at` and #0227 cases (#0222), and the whole suite against `main`'s build
  with conformance.md (#0223): 91 file runs, 4,599 passed, 0 failed, 0 skipped, plus the crates
  5,598 and the box 172 ([runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md)).
- **T05, the install and the live check that starts no instance, starts on Terry's word.**

## Start here

1. **Read** this file, then [handoff-v0.09.md](handoff-v0.09.md)'s "Start here" steps 2 to 5 (the
   checks of what is running, the board, the watcher and AWS), which still apply as written, then
   AGENTS.md, [architect.md](architect.md), and D73 to D81 with their change notes.
2. **The standing duties**: the board watcher as reader `agentspace`; the reporter, since
   `scripts/milestones.json` lists `v0.10`; a watchdog on any running dev lead.
3. **Before T05:** sweep #0224 against `main` (architect.md, Running a turn,
   step 2), then wait for Terry's "Start T05".

## Where things stand

| What | State |
|---|---|
| v0.10 (#0050) | In Progress. T01 to T04's fourteen stories in Ready For Review; T05's #0224 in Ready For Work |
| Milestones | `v0.10` (cap 15.5 hours) reporting; `v0.10-T01` to `v0.10-T04` removed at their dev leads' archives |
| Instance hours | None asked for, none used (Q16) |
| The hosts | Both `stopped`, `m9g.xlarge` (`status -All`, 1:01pm) |
| Terry's laptop | Unchanged by v0.10 so far; v0.09 installed. T02's dialogs appeared on his desktop (#0225) and were each closed |

### v0.10 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs | #0211 to #0213 | 1:25pm, 2026-11-24 | 3:19pm, 2026-11-24 | #147, #148, #149 | 1:54 of 3 h | Contract v1 in lease-api.md; schemas/ v1 with schemas/v0/; the plan's record, `last_success_at` and D77's due rule in state.md, driver.md, operator.md and README; 32 pins ruled |
| T02, the foundations and the hardening | #0214 to #0218 | 3:44pm, 2026-11-24 | 7:57pm, 2026-11-24 | #150 (the architect's pin as code), #151 to #155 | 4:13 of 3.5 h; an extension was asked at 6:31pm and not answered | `agentspace-plan`; the plan's record and `sized_for`; D77 in the registry, driver and provision; #0191's dialog and the run stand-in's cost fixed, #0189's bounds made load-proof; #0137 met, P51 to P53, `Invoke-HostFile` ends its case's processes; 23 pins ruled |
| T03, the programs | #0219, #0220, #0226 | 8:34pm, 2026-11-24 | 11:43pm, 2026-11-24 | #156, #157, #158 | 3:09 of 3 h | The CLI's `--plan`; the release's further walks and D59's change; every program's error mode and bootstrap's seam line (#0227); 14 pins ruled; follow-ups #0228 and #0229 carded |
| T04, conformance | #0221 to #0223 | 1:33am, 2026-11-25 | 3:54am, 2026-11-25 | #159, #160, #161 | 2:21 of 4 h | The lease CLI's plan, v1-compatibility and D77 (Y1 to Y5) cases; the operator's `last_success_at` table and #0227's seam case; the whole suite against `main`'s build, the record and conformance.md; 8 pins ruled; #0230 carded |

**T02's merge check**, on a local merge of #154 and #155 onto 01d14c2, whose tree `main` then
matched byte for byte at c5909e4: the whole workspace with no exclusion, 5,473 passed, the only
failures the six by design below; clippy and fmt clean; the whole conformance suite in three lanes,
83 files, 4,053 passed, 8 failed (test-help.ps1's create cases), 0 skipped; the deadline case 5 of
5. #0217's record of 120 runs under load is [runs/e0.10-t02-s04.md](runs/e0.10-t02-s04.md).

## Red on `main` by design, and who turns it green

**Nothing in the workspace, from T03's close.** Conformance's `test-help.ps1` eight create cases still
fail against the *installed* v0.09 programs, and v0.10's install (T05) turns them green; against
`main`'s build they pass (30 of 30).

**T03's merge check**, on #158's head d7504a5, which `main` then matched byte for byte at da1376b:
the whole workspace with no exclusion, 5,598 passed, 0 failed, 58 ignored; clippy, fmt and dist
clean; the whole conformance suite in three lanes, 83 files, 4,060 passed, 1 failed, 0 skipped, the
one failure R15 of `test-unmet-release-ends-connections.ps1` beside three other heavy runs, which
passed 61 of 61 three times alone (on #0223); the deadline case 5 of 5.

## Lessons from v0.10, by turn

**T01**, each now in its home:
- Propose the shared pins at launch; plan a fix pass per story; state a ruled order as its own
  list; name the merge order of linked spec stories; check every copied `$defs`; pattern sample
  IDs; a reviewer starts its workspace run first ([architect.md](architect.md), A turn that writes
  specs).
- **Slips, all transcript-only:** `py -0p` printed install paths, and with them the user name; a
  dev lead's `ls -la` printed the owner column; a subagent's stray redirect made a file named `1`
  in a worktree root; a subagent edited a doc with `sed`, and the dev lead read the hunk
  (AGENTS.md, masking rules, for the first two).
- **Worktree-isolated git refuses compound commands**, so each git step is its own call.

**T02**, each now in [architect.md](architect.md), A turn that builds:
- **A run wrapper started by `Start-Process -WindowStyle Hidden` gives its runs error mode 0**, and
  the dev lead's own wrapper put at least 17, by the subagents' reports, "Unsupported 16-Bit Application" dialogs on Terry's
  desktop before it set 0x8001 and logged it. Terry saw them and reported them; that report found
  #0191's cause.
- **Heavy runs gated mechanically**: named mutexes behind one wrapper held the laptop to three
  heavy runs while 13 subagents ran, once a shared queue kept 2-slot windows from starving.
- **A ruling reaches a running subagent only after it ends**: one investigator shipped a rejected
  fix, and one used a CPU burner as load after the ruling against it. Name forbidden methods in the
  first tasking, and plan a fixer after the investigators.
- **A merge check of open PRs that touch shared start paths runs once on a local merge of all of
  them**, every suite, and the merge then matches it byte for byte.
- **The reviewer MAY run beside its load runner** with its own target folder and a bounded wait.
- **Facts handed to a record writer are checked against the repository first**; a wrong base SHA
  reached a record and its review caught it.
- **The editing tools decode `\uXXXX` escapes**: a test or doc that holds one is checked byte for
  byte after an edit.
- **Slips, all transcript-only:** a load runner started cargo with `Start-Process -WindowStyle
  Hidden` outside the wrapper; one time sent from estimate, not a command.

**T03**, each now in [architect.md](architect.md), A turn that builds:
- **A ruling cannot stop a running subagent**: a test author made four runs on main's code after the
  dev lead's "no further reproduction". A tasking lists every run on `main` it may make, with its
  stop point, and the dev lead counts them from its run ledger.
- **A dialog test MUST bound the time against the fixed build and check its child ended on its
  own**: the operator `up` test passed on `main` in 10.63 s while a dialog waited.
- **When every start is already guarded, a probe of the process's own error mode is the test that
  fails without `main`'s call**, and adds no product surface.
- **Read the source before relaying a ruling's premise**: the architect carded #0227 in
  `agentspace-run`, and its cause was in `agentspace-sync`'s bootstrap.
- **A rule change's doc conformance is wide**: #0220's further walks touched more than 20 line
  ranges in four docs, found in three passes.
- **Two reviewers died on API 529 Overloaded**; their wrapped runs survived, and fresh reviewers
  adopted them by their output paths.
- **Slips, all transcript-only:** the architect's three ruling times and two of the dev lead's
  status times not read from a clock; a fixer's test printed a `%TEMP%` path with the profile's 8.3
  short name; a reviewer launched from the wrong worktree, stopped before it acted.

**T04**, each now in [architect.md](architect.md), A turn that builds:
- **One build for a turn that changes no code, and early runs on diff-checked bases** (P55), took
  every suite but two off the critical path; each was tied to the final `main` by a quoted diff.
- **One set of operator runs served two stories**, under two conditions: the runs are on the
  merged commit, and the record shows the diff over every input they read.
- **A whole-suite command comes from the last record's count**: a tasking copied conformance.md's
  one-file box-test command and ran 22 of 172 tests.
- **Run files are copied to the record's worktree before a checkout is removed**: `git worktree
  remove --force` deleted a runner's facts file with its ignored `target\`.
- **A doc writer's tasking ends with a proven anchor check**: a writer replaced a heading and left
  links dangling, which the reviewer restored.
- **Slips, all transcript-only:** an `ls -l` printed the user name; a tasking stated an uncounted
  fact.

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| T05 | Terry's word, then a dev lead | D81; #0224 |
| #0230, mask the log lines Get-HpLogShape quotes in a failing case's reason | Terry places it, or the architect proposes a release | Backlog |
| #0225 and #0227, met by #0226 and #0219 in T03, sit in Backlog | Terry, who moves them to Completed (Bot cannot move a card out of Backlog) | The board |
| #0228 (one walk ends a chain, in `agentspace-connections`) and #0229 (the ProxyCommand refuses a lease alias that is not ready) | Terry places them, or the architect proposes a release | Backlog; D59's change for v0.10-T03 |
| `docs/conformance.md`'s stale state-schema passages; `test-unmet-host-deploy-size.ps1`'s three load timeouts (180 of 180 alone, and 180 of 180 in T02's merge check); `test-unmet-release-ends-connections.ps1`'s one failure under the rig's load (passed in T02's merge check) | #0223 | Its card |
| T02 ran 43 minutes past its cap with no extension; the release's 15.5 hours are unchanged | Terry, if he extends | `scripts/milestones.json` |
| The live kit's three mask gaps (from handoff-v0.09.md) | **Done during T02**: `scripts\live\mask.ps1` rules 9a, 11a and 16a, with proof cases; `mask-proof.ps1` 285 passed | `scripts\live\` |
| The lost observer script (from handoff-v0.09.md) | Not needed in v0.10, since no turn starts an instance (D80, D81); carried to the next release that runs a host live | `scripts\live\` |
| AGENTS.md's sizing section rewritten for plan-sized leases | The architect, at v0.10's install | D73 |
| The "time to a prompt" spec (from handoff-v0.09.md) | Not placed | v0.08's live records |
| The rest of handoff-v0.09.md's open rows: #0192, #0193 and #0195's build in v0.12; #0190, #0082 and #0109 in v1.0 | Their epics | D72, D78 |

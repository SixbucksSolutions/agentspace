# Handoff: agentspace during v0.10

RFC 2119 keywords; the capitals are load-bearing.

> **Live record of v0.10 (#0050), started at T01's close, 3:19pm Mountain on 2026-11-24, and
> refreshed at T02's close, 8:32pm the same day.** It is refreshed at each turn's close and made
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
- **`main` is green but for the create help**, which T03 brings to v1 (below).
- **No turn runs on EC2 and v0.10 asks for no instance hours** (D81; Q16). Both hosts are stopped.
- **T03 starts on Terry's word.**

## Start here

1. **Read** this file, then [handoff-v0.09.md](handoff-v0.09.md)'s "Start here" steps 2 to 5 (the
   checks of what is running, the board, the watcher and AWS), which still apply as written, then
   AGENTS.md, [architect.md](architect.md), and D73 to D81 with their change notes.
2. **The standing duties**: the board watcher as reader `agentspace`; the reporter, since
   `scripts/milestones.json` lists `v0.10`; a watchdog on any running dev lead.
3. **Before T03:** sweep #0219, #0220 and #0226 against `main` (architect.md, Running a turn,
   step 2), then wait for Terry's "Start T03".

## Where things stand

| What | State |
|---|---|
| v0.10 (#0050) | In Progress. T01's and T02's eight stories in Ready For Review; T03 to T05's seven (#0219 to #0224, #0226) in Ready For Work |
| Milestones | `v0.10` (cap 15.5 hours) reporting; `v0.10-T01` and `v0.10-T02` removed at their dev leads' archives |
| Instance hours | None asked for, none used (Q16) |
| The hosts | Both `stopped`, `m9g.xlarge` (`status -All`, 1:01pm) |
| Terry's laptop | Unchanged by v0.10 so far; v0.09 installed. T02's dialogs appeared on his desktop (#0225) and were each closed |

### v0.10 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs | #0211 to #0213 | 1:25pm, 2026-11-24 | 3:19pm, 2026-11-24 | #147, #148, #149 | 1:54 of 3 h | Contract v1 in lease-api.md; schemas/ v1 with schemas/v0/; the plan's record, `last_success_at` and D77's due rule in state.md, driver.md, operator.md and README; 32 pins ruled |
| T02, the foundations and the hardening | #0214 to #0218 | 3:44pm, 2026-11-24 | 7:57pm, 2026-11-24 | #150 (the architect's pin as code), #151 to #155 | 4:13 of 3.5 h; an extension was asked at 6:31pm and not answered | `agentspace-plan`; the plan's record and `sized_for`; D77 in the registry, driver and provision; #0191's dialog and the run stand-in's cost fixed, #0189's bounds made load-proof; #0137 met, P51 to P53, `Invoke-HostFile` ends its case's processes; 23 pins ruled |

**T02's merge check**, on a local merge of #154 and #155 onto 01d14c2, whose tree `main` then
matched byte for byte at c5909e4: the whole workspace with no exclusion, 5,473 passed, the only
failures the six by design below; clippy and fmt clean; the whole conformance suite in three lanes,
83 files, 4,053 passed, 8 failed (test-help.ps1's create cases), 0 skipped; the deadline case 5 of
5. #0217's record of 120 runs under load is [runs/e0.10-t02-s04.md](runs/e0.10-t02-s04.md).

## Red on `main` by design, and who turns it green

| Test | Fails because | Turned green by |
|---|---|---|
| `agentspace-cli` tests/help.rs, five create-help tests (help.rs:61) | create's help is v1's in lease-api.md | #0219 (T03) |
| `agentspace-cli` tests/isolation_guard.rs `the_whole_suite_creates_nothing_in_a_home_that_does_not_exist` (isolation_guard.rs:871 since #0218) | Its child suite runs the create-help tests; only help's executable fails | #0219 (T03) |
| Conformance `test-help.ps1`, eight create cases | The same help | #0219's build, and v0.10's install (T05) for the installed programs |

**Turned green in T02:** `agentspace-state`'s three sample tests (#0215) and `agentspace-hosts`'
samples test at model.rs:668 (#0216). **#0191's four targets** pass alone and 20 of 20 under load
(#0217).

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

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| T03 to T05 | Terry's word for each turn, then a dev lead | D81; #0219, #0220, #0226, #0221 to #0224 |
| #0225, every program sets its error mode | #0226 and #0219 in T03; the start sites outside run and launch are listed on #0226 | D81's change during T02 |
| `docs/conformance.md`'s stale state-schema passages; `test-unmet-host-deploy-size.ps1`'s three load timeouts (180 of 180 alone, and 180 of 180 in T02's merge check); `test-unmet-release-ends-connections.ps1`'s one failure under the rig's load (passed in T02's merge check) | #0223 | Its card |
| T02 ran 43 minutes past its cap with no extension; the release's 15.5 hours are unchanged | Terry, if he extends | `scripts/milestones.json` |
| The live kit's three mask gaps (from handoff-v0.09.md) | **Done during T02**: `scripts\live\mask.ps1` rules 9a, 11a and 16a, with proof cases; `mask-proof.ps1` 285 passed | `scripts\live\` |
| The lost observer script (from handoff-v0.09.md) | Not needed in v0.10, since no turn starts an instance (D80, D81); carried to the next release that runs a host live | `scripts\live\` |
| AGENTS.md's sizing section rewritten for plan-sized leases | The architect, at v0.10's install | D73 |
| The "time to a prompt" spec (from handoff-v0.09.md) | Not placed | v0.08's live records |
| The rest of handoff-v0.09.md's open rows: #0192, #0193 and #0195's build in v0.12; #0190, #0082 and #0109 in v1.0 | Their epics | D72, D78 |

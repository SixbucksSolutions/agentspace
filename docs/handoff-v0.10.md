# Handoff: agentspace after v0.10

RFC 2119 keywords; the capitals are load-bearing.

> **Final, as of 9:42am Mountain on 2026-11-25.** v0.10 is closed: all sixteen of its pull requests
> (#147 to #162) are merged to `main`, and Terry approved #0050 and its fifteen stories into
> Completed. Started as the live record of v0.10 at T01's close, 3:19pm Mountain on 2026-11-24, and
> refreshed at each turn's close. Where this file and AGENTS.md, the board or decisions.md disagree,
> they win, and the disagreement is a bug to fix in this file.

**Written for the architect who starts v0.11 (#0052), after Terry `/clear`s this one.** It assumes
you have read nothing yet. **It is the only handoff you need to read**: what the earlier ones hold
that is still in use is carried here, and each rule is cited at its home rather than restated.

## The short version

- **v0.10 built plan-sized leases and contract v1, and they work on the installed programs.**
  `agentspace lease create --plan <file>` sizes a lease from a plan's busiest turn at half a vCPU
  and 2 GiB per agent working at once (D73), prints `sized_for`, and answers `plan_too_large` for a
  plan no size holds. T05's live check gave plans of 4, 15 and 33 agents `s`, `l` and
  `plan_too_large`, with no finding and no instance started
  ([runs/e0.10-t05-s01.md](runs/e0.10-t05-s01.md)).
- **A turn's lease is now sized by its plan**, and the architect's count by hand ends (AGENTS.md,
  "Sizing a host before it runs agents"; D73 to D75). It replaces D69's rule from v0.09.
- **`main` is green, and nothing is red by design**: 91 conformance file runs, 4,599 passed, 0
  failed, 0 skipped; the crates 5,598 passed, 0 failed, 58 ignored; the box 172; the deadline case 5
  of 5 ([runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md)). `test-help.ps1` passes 30 of 30 against
  the installed programs.
- **#0191's hangs were a Windows dialog, not load** (#0225). A process with error mode 0 that starts
  a file named `.exe` that is not a program raises "Unsupported 16-Bit Application" and waits for a
  person. Every agentspace program now sets its own error mode first, and so does the live kit's
  wrapper.
- **Nothing is running.** Both hosts are stopped, no lease is live, no milestone exists, the
  reporter is stopped, and every dev lead is archived. The board watcher was running at close.
- **v0.10 is installed on Terry's laptop**, and its keeper runs the new build.
- **Next is v0.11 (#0052), plan from the board, and it MUST NOT be laid out until Terry says so.**
  Its prep is [The scope ahead](#the-scope-ahead-v011).

## Start here

**Do these in order before anything else.** Each comes from a gap that cost time in a release.

1. **Read, in this order:** this file; [AGENTS.md](../AGENTS.md), every standing rule;
   [architect.md](architect.md), how a release is run, from Terry's word to Completed;
   [decisions.md](decisions.md), D73 to D81 with their change notes, then D14, D15, D27 and D74 for
   v0.11's design; then [roadmap.md](roadmap.md) and card #0052.
2. **Check what is running.** A cleared session leaves no background task of its own, but an orphan
   can still hold a reader. Print PIDs only, never a command line whole:
   ```powershell
   Get-CimInstance Win32_Process -Filter "Name='pwsh.exe'" | Where-Object { $_.CommandLine -match 'report-milestones|watch\.ps1|watchdog\.ps1' } | ForEach-Object { $_.ProcessId }
   ```
   Any process found MAY be stopped by its checked PID, with Terry's yes.
3. **Check the board answers:** `(Test-NetConnection 127.0.0.1 -Port 8811).TcpTestSucceeded`. If it
   is `False`, localswim is down, and starting it is Terry's; he MUST be asked.
4. **Arm the board watcher** under the Monitor tool, with a forward-slash path, a 30-minute timeout,
   re-armed at each expiry:
   `pwsh -NoProfile -File 'C:/Projects/claude-localswim-watcher/watch.ps1' -Board agentspace -Reader agentspace 2>&1`.
   The reporter starts only once `scripts/milestones.json` lists a milestone; today it lists none.
5. **Check AWS by exit code alone, before any turn that needs it**, never printing its error:
   `aws sts get-caller-identity *> $null; $LASTEXITCODE`. A non-zero code means Terry runs
   `aws login --remote` and pastes its link into a fresh Incognito window, every other Incognito
   window closed (AGENTS.md). **It had expired at the start of both v0.09-T05 and v0.10-T05.** To
   see the hosts, run `scripts\live\mask-proof.ps1`, and only when it exits 0, `status -All` through
   `scripts\live\mask.ps1` ([The architect's tools](#the-architects-tools)). Both hosts SHOULD read
   `stopped`.
6. **Wait for Terry's word before laying out v0.11** (AGENTS.md, "The board"). Until then, do the
   prep in [The scope ahead: v0.11](#the-scope-ahead-v011), and bring him the calls that are his.

## What changed from the v0.09 handoff, and why

- **This file stands alone again.** v0.10's live record leaned on v0.09's handoff for its tools,
  lane IDs, pitfalls and rules, so a cleared architect had two files to read. The parts still in use
  are carried here, updated; v0.09's handoff is history.
- **"A turn, start to close, in commands" is new.** v0.10 had five turn starts and five turn
  closes, and the architect rebuilt each one from architect.md and its own earlier tool calls, once
  hunting for a lane ID. The commands are now in one place, in order.
- **"Cards only Terry can move" is its own table**, apart from "What is open". Those cards sat in
  Backlog through three turn reports in v0.10 as rows among many.
- **"Numbers to plan by" gives each suite's wall time under load**, not only its case count. v0.10's
  conformance turn was planned from v0.09's figures, and its operator lanes ran 21 and 38 minutes
  because two files hold most of the time.
- **"Rules that are easy to break" carries v0.09's ten, updated, and adds v0.10's three.** None was
  dropped: each was broken again or is still one slip away.
- **Kept from v0.09's:** the short version first, "Start here", lessons grouped by what they are
  about with each lesson's home, the PR review in commands, and the dev lead's brief as a template.

## Where things stand

**As of v0.10's close, 9:42am Mountain, 2026-11-25.** Every time in this file is the laptop's zone,
Mountain, when it was written.

| What | State |
|---|---|
| v0.10 (#0050), plan-sized leases and contract v1 | Completed: the epic and its fifteen stories (#0211 to #0224, #0226), approved by Terry on 2026-11-25. Marked done in [roadmap.md](roadmap.md) |
| v0.11 (#0052), plan from the board | In Ready For Work, not laid out. It MUST NOT be broken into stories until Terry says so |
| `main`'s tests | T04's record, at efe799d: 91 conformance file runs, 4,599 passed, 0 failed, 0 skipped; the crates 5,598 passed, 0 failed, 58 ignored; the box 172; the deadline case 5 of 5. T05 changed no crate and no conformance file, only `scripts\live\wrap.ps1`, a record and docs. **Nothing is red by design** |
| The install on Terry's laptop | v0.10 from 599ccc4 (efe799d's code), a release build, installed by T05 at 7:47am on 2026-11-25: eight files copied, none renamed aside, no `.old-` file left. The keeper restarted on the new build; its task was ended and started, never disabled. No change to his ssh config, task definition or `PATH` |
| The hosts | Both `stopped` (T05's last `status -All`, 9:22am on 2026-11-25). The box (`agentbox`, swimcoach's): 20 GiB. `agentspace-host-agentspace`: 8 GiB. Both at `m9g.xlarge` |
| Instance hours | None asked for and none used in v0.10 (Q16). No instance has run since v0.09-T05 |
| Leases | 44 recorded, none `ready` or `pending`. Four are T05's `unsatisfiable` leases for `v010-plancheck`, which no release needs (D80) |
| The board | Every v0.10 card in Completed, and #0225 and #0227 with them. #0229 and #0230 folded into v0.11 (#0052), #0228 into v0.12 (#0051), and archived (D82). No defect or hardening card is open in Backlog |
| Swimcoach | No milestone; `scripts/milestones.json` is empty (053f71d), `v0.10` was stopped at 9:42am, and the reporter is stopped |
| Sessions | Every dev lead archived; `git worktree list` shows `main` alone. The board watcher was armed at close, as reader `agentspace` |
| Terry's laptop, otherwise | Unchanged by v0.10 beyond the install. T02's run wrapper raised "Unsupported 16-Bit Application" dialogs on his desktop, at least 17 by the subagents' reports; he closed each (#0225) |

### v0.10 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs | #0211 to #0213 | 1:25pm, 2026-11-24 | 3:19pm, 2026-11-24 | #147, #148, #149 | 1:54 of 3 h | Contract v1 in lease-api.md; `schemas/` v1 with `schemas/v0/`; the plan's record, `last_success_at` and D77's due rule in state.md, driver.md, operator.md and README; 32 pins ruled |
| T02, the foundations and the hardening | #0214 to #0218 | 3:44pm, 2026-11-24 | 7:57pm, 2026-11-24 | #150 (the architect's pin as code), #151 to #155 | 4:13 of 3.5 h; an extension was asked at 6:31pm and not answered | `agentspace-plan`; the plan's record and `sized_for`; D77 in the registry, driver and provision; #0191's dialog and the run stand-in's cost fixed, #0189's bounds made load-proof; #0137 met, P51 to P53, `Invoke-HostFile` ends its case's processes; 23 pins ruled |
| T03, the programs | #0219, #0220, #0226 | 8:34pm, 2026-11-24 | 11:43pm, 2026-11-24 | #156, #157, #158 | 3:09 of 3 h | The CLI's `--plan`; the release's further walks and D59's change; every program's error mode and bootstrap's seam line (#0227); 14 pins ruled; #0228 and #0229 carded |
| T04, conformance | #0221 to #0223 | 1:33am, 2026-11-25 | 3:54am, 2026-11-25 | #159, #160, #161 | 2:21 of 4 h | The lease CLI's plan, v1-compatibility and D77 (Y1 to Y5) cases; the operator's `last_success_at` table and #0227's seam case; the whole suite against `main`'s build, the record and conformance.md; 8 pins ruled; #0230 carded |
| T05, the install and the live check | #0224 | 7:39am, 2026-11-25 | 9:40am, 2026-11-25 | #162 | About 0:38 of 2 h active; 7:58am to 9:21am in Needs Terry on the expired AWS sign-in | v0.10 installed; the red list closed; D80's check passed with no finding; `scripts\live\wrap.ps1` sets error mode 0x8001 (P1) |

**The release used about 12:15 of its 15.5 hours**, counting T05's active time only.

## What v0.10 built

| Turn | Story | Card | PR | What |
|---|---|---|---|---|
| T01 | E0.10-T01-S01 | #0211 | #148 | lease-api.md becomes contract v1 |
| T01 | E0.10-T01-S02 | #0212 | #147 | `schemas/` for contract v1, v0's kept under `schemas/v0/` |
| T01 | E0.10-T01-S03 | #0213 | #149 | state.md, driver.md, operator.md and README: the plan's record and `last_success_at` |
| T02 | (architect's pin) | | #150 | `Record`'s `sized_for` and `plan`, `Host`'s `last_success_at`, read and written by nothing (D81) |
| T02 | E0.10-T02-S01 | #0214 | #151 | `agentspace-plan`: the plan document, its peak and its size |
| T02 | E0.10-T02-S02 | #0215 | #152 | `agentspace-state`: the plan's record, `sized_for` in the lease document, and v1's schemas |
| T02 | E0.10-T02-S03 | #0216 | #153 | A run due at once after a success (D77): `agentspace-hosts`, the EC2 driver and provision |
| T02 | E0.10-T02-S04 | #0217 | #155 | The workspace tests that hang or fail under load (#0191, #0189); the dialog found ([runs/e0.10-t02-s04.md](runs/e0.10-t02-s04.md)) |
| T02 | E0.10-T02-S05 | #0218 | #154 | #0137's scan, the isolation guard's P51 to P53, and `Invoke-HostFile`'s holders |
| T03 | E0.10-T03-S01 | #0219 | #156 | The lease CLI's `--plan`, `sized_for`, `plan_too_large` and v1's help |
| T03 | E0.10-T03-S02 | #0220 | #157 | A release leaves no process holding its alias that its walks can end (#0196) |
| T03 | E0.10-T03-S03 | #0226 | #158 | Every program sets its error mode first (#0225), and bootstrap's seam line (#0227) |
| T04 | E0.10-T04-S01 | #0221 | #159 | Conformance for plans, contract v1 and D77's due rule |
| T04 | E0.10-T04-S02 | #0222 | #160 | Conformance for provision's `last_success_at` |
| T04 | E0.10-T04-S03 | #0223 | #161 | The whole suite, the crates and the box tests against `main`'s build, with conformance.md ([runs/e0.10-t04-s03.md](runs/e0.10-t04-s03.md)) |
| T05 | E0.10-T05-S01 | #0224 | #162 | The install and D80's live check that starts no instance ([runs/e0.10-t05-s01.md](runs/e0.10-t05-s01.md)) |

**The design is D73 to D80, and D81 lays it out.** D73 is the footprint, D74 the plan document,
D75 `create --plan`, D76 contract v1, D77 the due rule after a success, D78 #0195's fix placed in
v0.12, D79 the folded-in work, and D80 the done-when shown with no instance.

## Numbers to plan by

**Measured in v0.10, for v0.11's caps and staffing.** Each comes from a record or a command, cited.

| What | Figure | Source |
|---|---|---|
| A turn's length against its cap | T01 1:54 of 3 h; T02 4:13 of 3.5 h; T03 3:09 of 3 h; T04 2:21 of 4 h; T05 about 0:38 of 2 h. The release about 12:15 of 15.5 h | This file, "v0.10 by turn" |
| Peak subagents laid out per turn | T01 8, T02 13, T03 7 (5 before #0226), T04 8, T05 2 | D81 and its change |
| The whole conformance suite | 91 file runs, 4,599 cases: the lease CLI 1,301, the operator 1,455, the keeper 787, the schemas 1,051 | runs/e0.10-t04-s03.md |
| The lease CLI's suite | About 17 minutes under the turn's load | T04's dev lead |
| The operator's suite | Two lanes, 21 and 38 minutes, unbalanced: `test-unmet-host-deploy-size.ps1` about 680 s and `test-unmet-sync-patch-wait.ps1` about 440 s. Balance lanes by seconds | architect.md, "A turn that builds" |
| The whole workspace's tests | 5,598 passed, 58 ignored, about 22.6 minutes | runs/e0.10-t04-s03.md |
| A release build, `cargo xtask dist --release` | 91 s, eight files | runs/e0.10-t05-s01.md |
| The install, keeper stop to keeper start | 47 s (7:46:46am to 7:47:33am) | runs/e0.10-t05-s01.md |
| A `create` answered `unsatisfiable` before any AWS call | 37 to 57 ms | runs/e0.10-t05-s01.md |
| Heavy runs at once on the laptop | Three, gated mechanically; a fourth failed R15 of `test-unmet-release-ends-connections.ps1` | architect.md, "A turn that builds" |
| The guard's pass | Every 300 s; a pass that finds every host stopped writes nothing to `guard.log` | docs/keeper.md; runs/e0.10-t05-s01.md |
| The footprint, from v0.06-T01 | 15 agents peaked at 4.10 cores and 3.81 GiB; D73 sizes at twice that per agent | footprint.md; D73 |
| The sizes at D73 | `s` up to 4 agents, `m` 8, `l` 16, `xl` 32; more is `plan_too_large` | D73; AGENTS.md |
| A resize, for a turn that uses a host (v0.09) | `m` to `l` 176.8 s; `l` to `m` 257.6 s, about 36 s of it D48's wait, which D77 now skips after a success | runs/e0.09-t05-s01.md; D77 |
| Live-turn cost (v0.09) | 0.534 instance hours for a full live check list on two hosts | runs/e0.09-t05-s01.md |

## Terry's orders from v0.10

**Each binds where its row says; this table only finds them.**

| Order | Given | Where it binds |
|---|---|---|
| Yes to Q14 ("Yes, half a vCPU"): D73's footprint replaces his per-agent rule from v0.10's install | 2026-11-24 | D73; AGENTS.md, sizing |
| No to Q15 and Q16 ("No to both"): no account setting changes for #0195, and no instance hours for v0.10 | 2026-11-24 | D78, D80 |
| "Lay out v0.10 with its staffing plan. Tell me peak subagents per turn and which turns are run on EC2" | 2026-11-24 | D81; each story's card |
| "Start T01" to "Start T05", each turn started on his word, one of them as "Start it" on a proposed session card | 2026-11-24 and 25 | This file |
| "Let's pull risk in as much as possible": #0225 carried into T03 as #0226 | 2026-11-24 | D81's change during T02 |
| The dialogs on his desktop reported ("Saw this pop up", "Still gettin these"), which found #0191's cause | 2026-11-24 | #0225; runs/e0.10-t02-s04.md |
| AWS's sign-in renewed during T05 ("Signed in") | 2026-11-25 | runs/e0.10-t05-s01.md |
| v0.10 approved into Completed ("Approved") | 2026-11-25 | The board |
| This handoff, modeled on the handoffs in `docs/` and improved where it can be | 2026-11-25 | This file |

## Rules that are easy to break

**Each was broken at least once in v0.08 to v0.10. Each has its home, which wins.**

1. **A time, a SHA, a count or an interval MUST come from a command's output, never ahead of it or
   from memory.** Broken in v0.10 by the architect's three ruling times in T03, two dev leads'
   status times, and "each within a second" sent for 17 of 18 (architect.md, "Writing in the
   repository").
2. **Nothing of Terry's MAY reach a transcript**: not his user name, profile path in either form,
   computer name, a lease ID or alias, or an AWS identifier. Broken in v0.10 by `py -0p`, by `ls -la`
   and `ls -l`, and by a test's `%TEMP%` path holding the 8.3 short name (AGENTS.md, masking).
   Scratch lives inside the checkout's `target\`; listings are bare.
3. **Read the source before ruling or citing.** Broken by the architect in v0.10: #0227 carded in
   `agentspace-run` when its cause was `agentspace-sync`'s bootstrap, and T05's brief saying the live
   kit's wrapper set the error mode when it did not (architect.md, "Running a turn", step 2).
4. **A merge check runs every suite the merged code feeds**, never a selection by touched files
   (architect.md, "A turn that builds").
5. **Every checker is proven to fail on a planted value before its pass is believed** (Terry's
   global rule; architect.md).
6. **Every open row of the last handoff is carried into the next**, or says what became of it
   (architect.md, "Closing a release"). This file's [What is open](#what-is-open) accounts for
   v0.09's.
7. **The architect is the only agent that moves a card, and a card leaves Backlog only on Terry's
   word** (AGENTS.md, "Roles and pull requests").
8. **PowerShell names are case-insensitive**: `$b` overwrote `$B`, and a helper whose name matches
   an alias runs the alias (AGENTS.md, "The board").
9. **A test on a by-design red list is checked by its failing assertion**, never by its name
   (architect.md).
10. **`stop` goes to swimcoach only on Terry's word for a release; a turn's milestone stops at its
    dev lead's archive** (AGENTS.md, "Status on Terry's phone").
11. **No process that starts a program runs with error mode 0.** A run started by `Start-Process
    -WindowStyle Hidden` has mode 0, and put dialogs on Terry's desktop in T02. Runs go through a
    wrapper that ORs in 0x8001 and logs it (architect.md, "A turn that builds";
    `scripts\live\wrap.ps1`).
12. **At most three heavy runs at once on the laptop.** The architect's own T03 merge check ran four
    and R15 failed under it (architect.md, "A turn that builds").
13. **A card waiting on Terry goes to Needs Terry as soon as only he is left**, so its clock pauses
    and it reaches his phone (AGENTS.md, "Status on Terry's phone"). T05's wait was 83 minutes.

## Lessons from v0.10, by what they are about

**Each lesson says what to do and where it is now written. Every lesson of the release is here.**

### Terry's details and real files

| What happened | What to do | Where |
|---|---|---|
| T01: a schema writer printed `py -0p`, whose install paths carry the user name | Mask any command that lists installed tools' paths: `where`, `Get-Command`, a `--version` that prints its location | AGENTS.md, masking |
| T01 and T04: `ls -la` and `ls -l` printed the owner column, the Windows user name | Bare listings only | AGENTS.md, masking |
| T03: a fixer's test printed a `%TEMP%` path with the profile's 8.3 short name | Scratch inside the worktree's `target\` | AGENTS.md |
| T02: the dev lead's run wrapper, started hidden, put at least 17 dialogs on Terry's desktop | Every run through a wrapper at error mode 0x8001; no `Start-Process -WindowStyle Hidden` | architect.md, "A turn that builds" |

### Facts, times and counts

| What happened | What to do | Where |
|---|---|---|
| T03: three of the architect's ruling times were not read from the clock | `Get-Date` before every time written, in conversation and in docs | AGENTS.md, Writing |
| T02: a wrong base SHA reached a record, and its review caught it | Facts handed to a record writer are checked against the repository first | architect.md |
| T05: a record writer turned a status message into a status read | The dev lead reads every line that cites its own notes before the review | architect.md, live checks |
| T04: a tasking stated a count no command had made | Counts from a command | Every brief |
| T02: the editing tools decode `\uXXXX` escapes | Check a file holding one byte for byte after an edit | architect.md |

### Rulings and diagnosis

| What happened | What to do | Where |
|---|---|---|
| T02: #0191's hangs looked like load and were a modal dialog | A hang under a hidden run is first checked for a dialog; Terry's screen is evidence | #0225; runs/e0.10-t02-s04.md |
| T02: a ruling reached a running subagent only after it ended; one shipped a rejected fix, one used a CPU burner | Name forbidden methods in the first tasking, and plan a fixer after the investigators | architect.md |
| T03: a test author made four runs on `main`'s code after the dev lead's "no further reproduction" | A tasking lists every run on `main` it may make, with its stop point | architect.md |
| T03: the architect carded #0227 in the wrong crate | Read the source before relaying a ruling's premise | This file, rule 3 |
| T05: the brief said the live kit's wrapper set the error mode | Check a ruling that names a tool's property against the tool's source | architect.md, live checks |
| T01: pins shared across three specs arrived as the specs were written | Propose the shared pins at launch; state a ruled order as its own list | architect.md, "A turn that writes specs" |

### Running a turn and its subagents

| What happened | What to do | Where |
|---|---|---|
| T02: 13 subagents shared the laptop's three heavy-run slots | Named mutexes behind one wrapper, with a shared fair queue | architect.md, "A turn that builds" |
| T02: the reviewer waited on the load runner | The reviewer MAY run beside it with its own target folder and a bounded wait | architect.md |
| T03: two reviewers died on API 529 Overloaded | Fresh reviewers adopt the wrapped runs by their output paths | architect.md |
| T04: `git worktree remove --force` deleted a runner's facts file with its ignored `target\` | Copy run files to the record's worktree before a checkout is removed | architect.md |
| T04: a writer replaced a heading and left links dangling | A doc writer's tasking ends with an anchor check proven on a planted bad anchor | architect.md |
| Every turn: worktree-isolated git refuses compound commands | Each git step is its own call | Every brief |

### Builds, tests and load

| What happened | What to do | Where |
|---|---|---|
| T02: open PRs that touch shared start paths | One merge check on a local merge of all of them, every suite; the merge then matches it byte for byte | architect.md |
| T03: a dialog test passed on `main` in 10.63 s while a dialog waited | Bound the time against the fixed build and check the child ended on its own | architect.md |
| T03: every start was already guarded, so no start test could fail without the fix | Probe the process's own error mode; it adds no product surface | architect.md |
| T03: #0220's rule change touched more than 20 line ranges in four docs, found in three passes | Plan a doc-conformance pass for every rule change | architect.md |
| T04: one build and early runs on diff-checked bases (P55) took every suite but two off the critical path | Build once for a turn that changes no code; tie each early run to the final `main` by a quoted diff | architect.md |
| T04: a box-test tasking copied a one-file command and ran 22 of 172 tests | A whole-suite command comes from the last record's count | architect.md |
| T04: the operator lanes ran 21 and 38 minutes | Balance lanes by seconds, not file count | architect.md |

### Live checks on real hosts

| What happened | What to do | Where |
|---|---|---|
| T05: the sign-in had expired at the turn's start, as in v0.09-T05 | Check it by exit code before the turn; the install and drafting run while it waits | architect.md, live checks |
| T05: a runner read the guard's silence as a conflict | A passing guard pass writes nothing to `guard.log`; say so in the tasking | architect.md, live checks; keeper.md |
| T05: a dry run in a conformance-harness home caught every script error first | Dry-run a live check's scripts against the worktree's build before the live run | architect.md, live checks |
| v0.09: the live kit's three mask gaps | Closed before T05: rules 9a, 11a and 16a, `mask-proof.ps1` at 285 cases | `scripts\live\mask.ps1` |

## Pitfalls of the laptop's tooling

**None is agentspace's design; each cost a retry in v0.07 to v0.10.**

| Pitfall | What works |
|---|---|
| The Monitor tool runs bash, which drops a Windows path's backslashes | `'C:/Projects/claude-localswim-watcher/watch.ps1'` |
| `%USERPROFILE%` does not expand in PowerShell, Terry's shell | `& "$env:USERPROFILE\.agentspace\bin\agentspace.exe" ...` in anything sent to him |
| `lease list` prints `{"leases": [...]}`, and its documents have no `ssh` field | Read `.leases`; the alias is `agentspace-<project>-<first 8 hex digits of id>` (lease-api.md) |
| PowerShell's `-like` reads a backtick as an escape | Compare with `-eq` or `.Contains()` when the text holds a backtick |
| `ConvertFrom-Json` turns ISO 8601 strings into local `DateTime`s | `-DateKind String`; and never rewrite `milestones.json` from a parse |
| `Start-Process` runs its child with error mode 0, and `-WindowStyle Hidden` hides the dialog that follows | Run through a wrapper that ORs in 0x8001 and logs it |
| A Python string in a bash heredoc turns `\r`, `\a` or `\v` in a path into a control character, and warns on `\l` | Raw strings; count control characters after a scripted edit |
| A here-string piped to `git commit -F -` reaches git as an argument | Write the message to a scratch file and use `git commit -F <file>` |
| `pwsh -File script.ps1 -Files a,b` passes the list as one string | Pass one comma-separated string and split it in the script |
| A relative `-Out` given to `run-conformance-files.ps1` doubles its path | Pass absolute paths |
| A shell command that reads stdin with nothing redirected hangs until its timeout | Give every command its input, or none |
| A review worktree under the scratchpad's long path fails on box/tests' fixtures | Put review worktrees under `.claude\worktrees\` |
| Git in a worktree-isolated session refuses compound commands | One git step per call |
| `git worktree remove --force` deletes the worktree's ignored `target\` | Copy run files out first |
| A worktree folder stays held by the session that was in it | Remove the empty folder after that session is archived |
| `archive_session` is refused once by a hook that relabels the call, and refused while the dev lead is mid-turn | Resend with exactly the input the refusal quotes; retry after its turn ends |
| `start_session` is refused on a turn an event started, not Terry | Propose the session as a card; Terry starts it with one click |
| Bot's card moves out of Backlog or into Completed answer HTTP 403; a move straight from Ready For Work to Ready For Review exited 1 once, its error not read | Terry moves a card out of Backlog and approves it into Completed; the architect moves it through In Progress to Ready For Review |
| `localswim board get` shows no lanes, and `lane list` does not exist | `board get --view lanes <board>`, or the lane IDs below |
| `agentspace-operator status` prints on stderr | Capture `2>&1` |
| `localswim` on `PATH` is stale | `C:\Projects\localswim\localswim.exe`, by full path, as Bot |
| `cargo xtask dist` prints no file count | Count `target\dist` with a bare listing |

## The architect's tools

| Tool | What it does |
|---|---|
| `scripts/scan-identifiers.ps1 -Head <ref> -Base origin/main [-Pr <n>]` | The identifier scan of a diff, its commits and a PR's body, each pattern proven on a planted value first; prints counts, never values |
| `scripts/run-conformance-files.ps1 -Checkout <worktree> -Out <absolute folder> -Files '<a>,<b>' [-Dist <folder>]` | Runs conformance files against a build's programs; ends `Failed: none` and exit 0, or the failed files and exit 1 |
| `scripts/watchdog.ps1 -Lead <name>` and `-Reset` | The 10-minute watchdog on a dev lead; reset it on every message and turn note, and restart it with `-Reset` first |
| `scripts/report-milestones.ps1 -Watch` | The swimcoach reporter, whenever a milestone exists |
| `scripts/milestones.json` | The milestones, edited as text only, with the real time |
| `C:\Projects\swimcoach\bin\swimcoach.cmd stop --project agentspace --milestone <name>` | Stops a milestone, after it is out of `milestones.json` |
| `scripts\live\mask-proof.ps1` | Proves the mask (285 cases) and the scan; prints only counts and `True`/`False` for the real environment. Run it first, every live session |
| `scripts\live\mask.ps1` | Dot-sourced: `ConvertTo-LiveMasked -Text <text>`, v0.09's 18 rules, 9a, 11a and 16a, and the 8.3 short form |
| `scripts\live\wrap.ps1 -Out <folder> -Name <nnn-what> -Command '<PowerShell>'` | Runs one command at error mode 0x8001 or more, logged in `.start`, with its streams to files, `.exit` written last, and a masked copy to print; refuses with 97 when the mode lacks either bit |
| `scripts\live\scan.ps1 -File <file>` | Exits 1 if the mask would change any line of a file, printing masked lines only |

**The board,** board `c74ddc68-9c3a-4434-8435-563c596d3229`. Every call loads the Bot credential
for one run: `. C:\Projects\claude-localswim-watcher\lib\env.ps1; Set-LocalswimEnv; try { ... }
finally { Clear-LocalswimEnv }`. The token MUST NOT be printed or passed as an argument.

| Lane | ID |
|---|---|
| Backlog | `6979a096-70c6-441e-8afe-9abcf93064a6` |
| Ready For Work | `bc3e1f28-4a82-4aac-a10e-9c0b391658a6` |
| In Progress | `29e4255d-16c5-4d43-886a-a698a2b010c5` |
| Blocked | `2f586b37-d46d-4438-aaf9-0c898c01e311` |
| Needs Terry | `1a77b481-653c-450b-970c-c86e9e7cd844` |
| Ready For Review | `9fbed82c-b62b-4650-a229-847de2b2de08` |
| Completed | `54c62479-1cc9-4472-a360-fe2fa17bc6c9` |

| localswim call | Use |
|---|---|
| `card get <board> <n>` | A card by bare ticket number (`224`, not `#0224`); JSON with `lane_id`, `revision`, `description`, `completed_at` |
| `card move <board> <n> <lane ID>` | A move; only the architect's. Send its output to `$null` and print `$LASTEXITCODE` |
| `card create --description <text> <board> <lane> <title>` | A new card; then find its ticket with `card search --search <words> <board>` |
| `card description --revision <r> <board> <n> <file>` | Replaces a description from a file; read the revision with `card get` first |
| `relationship create <board> <source> <kind> <target>` | `blocks`, `references` or `relates_to` |
| `board get --view lanes <board>` | The lanes, with their keys and labels |

**A PR review, in commands** (architect.md, "Reviewing a pull request", gives the why):
1. `git fetch origin pull/<n>/head:refs/review/pr<n>`, and check the SHA against the dev lead's.
2. When the PR's checks run programs: `git worktree add --detach .claude\worktrees\rv<n> refs/review/pr<n>`,
   `cargo xtask dist` in it, and every suite the merged code feeds. A record and process-tool PR
   needs no build; read every line of it, and run its kit's proof and its scan instead.
3. `scripts/scan-identifiers.ps1`, then look at every hit; check
   `git log --format='%h %an %ae %cn %ce' origin/main..refs/review/pr<n>` against
   `git config user.email`, not against `main`'s tip, whose committer is GitHub's squash.
4. `gh pr merge <n> --squash --match-head-commit <sha>`, and only once it succeeds, delete the
   branch, move the card and tell the dev lead; remove the review worktree and ref last.

## A turn, start to close, in commands

**New in this handoff.** architect.md, "Running a turn", gives the why; this is the order.

**At the start, on Terry's "Start Tnn":**
1. `git pull --ff-only origin main`, and check every PR of the turn before is merged.
2. Sweep the turn's cards against `main` (architect.md, step 2): every file, command and list a card
   names, read, not grepped. Check AWS by exit code if the turn needs it.
3. Add the turn's milestone to `scripts/milestones.json` as text, with its start in UTC from a
   command, `cap_hours` from the layout decision and the turn's tickets; `git commit -F`, push.
4. Move every story of the turn to In Progress (`29e4255d-16c5-4d43-886a-a698a2b010c5`), and the
   epic with its first.
5. `start_session` with the brief below and `name` `vXYY-tnn`, then
   `scripts\watchdog.ps1 -Lead <name> -Reset` and `scripts\watchdog.ps1 -Lead <name>` in the
   background.

**While it runs:** reset the watchdog on every message and turn note; answer pins as Terry's proxy,
writing each ruling where it binds; move a card to Needs Terry the moment only he is left, stop the
watchdog while it waits on him, and move the card back when he answers; review and merge each PR as
above.

**At the close, on the dev lead's DONE:**
1. Stop the watchdog. Archive the dev lead (resend the call's quoted input once if a hook relabels
   it), then remove any empty worktree folder it held.
2. Remove the turn's milestone from `scripts/milestones.json` as text.
3. Refresh the live handoff (the turn's row, its lessons, "What is open") and architect.md's lessons.
4. `git commit -F`, `scripts/scan-identifiers.ps1 -Head HEAD -Base origin/main`, push, then
   `swimcoach stop` for the turn.
5. Report to Terry, with the next turn's start as his call.

**A dev lead's brief, as a template.** `start_session` with `initiation: "user_asked"` on Terry's
"Start Tnn", `context: "fresh"`, `use_worktree: false`, `effort: "high"`, a short `name`, and a
`background` of at least 80 characters. In order:
1. **Its cards and branches**, the base commit, the turn's start time and cap, that the cards are
   already In Progress, and its watcher reader (`agentspace-vX.YY-tnn`).
2. **What to read first:** AGENTS.md, the cards to their end, the decisions, this file's lessons,
   and the last live record when the turn is live.
3. **The rulings already made for the turn**, numbered, each with its reason: the sweep's findings,
   whose files the story owns, the heavy-run limit, the error-mode wrapper, and any gate that waits
   on Terry.
4. **The rules**, as architect.md, "Running a turn", step 7 lists them.
5. **The slips list**, every slip of the release so far, **with the pitfall lines quoted**, for
   every subagent's tasking.
6. **What done is**: the PR's head sent and nothing pushed after; after the merge, worktrees and
   branches removed, watcher stopped, and DONE with lessons.

## Rulings made during v0.10

**T01's dev lead raised pins P1 to P32, T02's P1 to P23, T03's P1 to P14, T04's P1 to P8 and T05's
P1; each turn numbered its own.** Each is cited in the spec it ruled, as "architect, <date>, for
v0.10-Tnn, pin Pn", 58 citations by grep. The ones that changed a decision are change notes in
decisions.md; this table only finds them.

| Ruling | Where it binds |
|---|---|
| T01 P3, P7: a subagent's `model` takes the names' pattern, spaces and `+` allowed; `PlanError.message` is the reason alone, framed by the CLI | D81's change for v0.10-T01; lease-api.md |
| T01 P6, P13: `plan` is a `field` of `invalid_value` and of `usage`, and `--plan` with `--size` is `usage` | lease-api.md, Exit codes and errors |
| T01 P22 to P24: when a run succeeded, and when `last_success_at` is written and removed | D77's change; driver.md, the run |
| The pin as code before T02 (#150): `Record`'s and `Host`'s new members, read and written by nothing | D81 |
| #0225 carried into T03 as #0226, on Terry's word; `quiet_hard_errors()` first in every main | D81's change during v0.10-T02 |
| T02 P5, P8: a plan holding a number the parser cannot hold (`1e400`) is not valid JSON; a plan in UTF-16 is refused, with or without its byte order mark | lease-api.md, the plan's checks |
| T03: a release walks again while a walk ends a client, up to six walks in all or until 10 seconds have passed | D59's change for v0.10-T03; driver.md, Release a lease |
| T04: one set of operator runs serves two stories, on the merged commit, with the diff over every input | runs/e0.10-t04-s03.md |
| T05 P1: `wrap.ps1` ORs 0x8001 into its error mode, logs it and refuses without it | runs/e0.10-t05-s01.md; `scripts\live\wrap.ps1` |
| The rest | The specs and cards, each where it is cited |

## Cards only Terry can move

**Bot cannot move a card out of Backlog, or into Completed (HTTP 403).** Each waited on his word.
**All five are settled, 2026-11-25** (D82): Terry moved them out of Backlog, the architect moved
#0225 and #0227 through In Progress to Ready For Review and he approved them into Completed, and
#0228 to #0230 were folded into their epics as recommended below and archived.

| Card | What | Its evidence | His call |
|---|---|---|---|
| #0225 | Defect: no program set an error mode, so a non-PE program raised a modal dialog | Met by #0226 (#158) and #0219 (#156); runs/e0.10-t02-s04.md | Move to Completed |
| #0227 | Defect: a seam variable naming a file Windows refuses to start failed with the default's line | Met by #0226 (#158); #0222's case (#160) | Move to Completed |
| #0228 | Hardening: a release takes a found client no built tree holds as a top in the same walk (`agentspace-connections`) | Carded in T03; D59's change for v0.10-T03 | Place it. The architect recommends v0.12 (#0051), whose shared turn reworks the driver |
| #0229 | Defect: the ProxyCommand connects a lease alias whose record is not ready, so a desktop app can reconnect after a release | Carded in T03; D59's change for v0.10-T03 | Place it. The architect recommends v0.11's first build turn: it is the one open defect a user can meet |
| #0230 | Hardening: `Get-HpLogShape` quotes raw provision log lines, which can carry the 8.3 short name, in a failing case's reason | Carded in T04 | Place it. The architect recommends v0.11, beside its conformance turn |

## What is open

**Rechecked at close, 9:42am Mountain, 2026-11-25. Every item names who acts.**

| What | Who acts | Where it goes |
|---|---|---|
| v0.11 (#0052) is laid out only on Terry's word | Terry, then the architect | [The scope ahead](#the-scope-ahead-v011) |
| #0229 and #0230, folded into v0.11; #0228, folded into v0.12 | Their epics | #0052 and #0051 (D82) |
| v0.09-T05's observer script (`observe-once.ps1`) was lost with its worktree; not needed while no turn starts an instance | The architect, before the next live turn on a host, rebuilt from runs/e0.09-t05-s01.md | `scripts\live\` |
| A spec for what "time to a prompt" measures when the keeper starts a lease's holder first | The next release that specifies the lifecycle; not yet placed | v0.08's live records |
| #0192, #0193 and #0195's fix (D78) | v0.12 | #0051 (D72, D78) |
| #0190, #0082 and #0109; a footprint per project (D73) | v1.0's first turn, before the real project measures its own turn | #0053 (D72, D73) |

**What became of v0.09's open rows:** v0.10 laid out on Terry's word (D81, done); the live kit's
three mask gaps closed during T02 with a proof case each (done); the observer script carried above,
not needed in v0.10 (D80); #0189, #0191, #0196, #0195's decision, #0137's last part, P51 to P53 and
`Invoke-HostFile`'s holders met in T02 and T03 (#153 to #158; #0195's decision is D78); #0192,
#0193 and #0195's build carried above to v0.12; #0190, #0082 and #0109 carried above to v1.0; the
"time to a prompt" spec carried above.

**What became of the live record's own rows:** AGENTS.md's sizing section rewritten for `--plan` at
T05's close (52a1ae2, done). T02's 43 minutes past its cap, with its extension unanswered, is OBE:
v0.10 closed at about 12:15 of its 15.5 hours, and Terry approved it.

## The scope ahead: v0.11

**v0.11 (#0052): plan from the board.** agentspace reads a project's plan from its localswim epic
instead of a file. Done when a lease can be asked for by naming a board and an epic; agentspace reads
that epic's plan, sizes the lease from it as v0.10 does, and sets its TTL from it; and a plan that
cannot be read is refused with what is wrong, never guessed at (D5; card #0052).

**Its stories MUST NOT be laid out until Terry says so.**

**Before its stories are laid out, the architect SHOULD settle, each as a numbered decision:**
- **How an epic carries its plan.** The card's own design call: a block with a schema, so reading it
  is not parsing prose. **The architect's lean:** a fenced JSON block in the epic's description
  holding exactly a v0.10 plan document (D74), so the board adds a source, not a format, and
  `agentspace-plan`'s parser and bounds stay the one check. Whether localswim's own data model has a
  better home, such as a field or an attachment, is read from its API first, not assumed.
- **Which turn a lease is for.** D74 sizes a lease for the whole plan it is given, and a turn's
  lease passes a plan of that turn alone. Naming an epic gives the whole plan, so v0.11 needs a way
  to name the turn, or it sizes every lease for the epic's busiest turn.
- **The TTL from the plan.** D74's plan has no durations; D81's turn budgets live in the epic's
  prose. Whether a turn's budget joins the plan document, and how a TTL follows from it (D27 also
  keeps model and effort for it), is a contract question: a new member of an input schema.
- **What the contract shows.** A new `create` input (a board, an epic and a turn) and a new error
  `field` each touch v1's schemas, and D76 kept output schemas strict. Whether that is v2, or input
  only and so still v1, is decided against lease-api.md's Versioning section.
- **How the lease CLI reaches the board, and with what credential.** Today only Bot's token reaches
  localswim, loaded per run by the watcher's `env.ps1`, and `%LOCALAPPDATA%\localswim-v2` is among
  the folders a review MUST NOT read (AGENTS.md). **A credential the product holds for Terry's
  board, and where it lives, is Terry's call** (D19). Conformance MUST read a stand-in board, never
  his.
- **The live check.** Reading a real epic is a read of Terry's real board. Whether a throwaway epic
  on his board, a board of its own, or no live board read at all shows the done-when is put to him
  with the design. Like v0.10's, it SHOULD start no instance.
- **The first epic to carry a plan block MAY be v0.11's own**, if Terry agrees, so the release that
  builds the reader is also its first input.

**The road after v0.11 is roadmap.md's:** v0.12 both new platforms, then v1.0. #0052 blocks #0051,
and #0051 blocks #0053.

## When Terry approved v0.10

**Done, 2026-11-25, by 9:42am Mountain**, after he said "Approved" and a board event showed #0050
and its stories in Completed. Kept as the record of this close; the procedure is
[architect.md](architect.md#closing-a-release).
1. Every v0.10 card checked in Completed by `card get`: sixteen, the epic and its fifteen stories.
2. `v0.10` removed from `scripts/milestones.json` (053f71d), then `swimcoach stop` at 9:42am, and the
   reporter stopped with no milestone left.
3. v0.10 marked done in roadmap.md's table.
4. AGENTS.md's sizing section rewritten for `--plan` at T05's close (52a1ae2).
5. This file made final, and AGENTS.md's "Where it stands" pointed at it.
6. Waiting for Terry's word before laying out v0.11.

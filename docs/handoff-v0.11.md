# Handoff: agentspace after v0.11

RFC 2119 keywords; the capitals are load-bearing.

> **Final, as of 11:11am Mountain on 2026-11-26.** v0.11 is closed: all fourteen of its pull
> requests (#163 to #176) are merged to `main`, and Terry approved #0052 and its thirteen stories
> into Completed. Started as the live record of v0.11 at T01's close, 2026-11-25, and refreshed at
> each turn's close. Where this file and AGENTS.md, the board or decisions.md disagree, they win, and
> the disagreement is a bug to fix in this file.

**Written for the architect who starts v0.12 (#0051), after Terry `/clear`s this one.** It assumes
you have read nothing yet. **It is the only handoff you need to read**: what the earlier ones hold
that is still in use is carried here, and each rule is cited at its home rather than restated.

## The short version

- **v0.11 built plan-from-the-board leases and contract v2, and they work on the installed
  programs.** `agentspace lease create --board <id> --epic <ticket> [--turn <name>]` reads the
  epic's one `agentspace-plan` block (D83), sizes the lease for the named turn or the busiest (D84),
  and sets its TTL to that turn's `budget_minutes` and half again (D85). T05's live check read #0052
  and got a lease equal to its `--plan` twin with and without `--turn`, #0052 unchanged by the read,
  no finding and no instance started ([runs/e0.11-t05-s01.md](runs/e0.11-t05-s01.md)).
- **The board is read with Bot's token, by one read-only client** (`agentspace-board`; D86, Q17's
  (a)): one GET per lease, the token read for each call from localswim's receipt, never stored.
- **`main` is green, and nothing is red by design**: 94 conformance files and the deadline case,
  6,208 passed, 0 failed, 0 skipped; the crates 5,978 passed, 0 failed, 58 ignored; the box 172; the
  empty program 46 of 46 ([runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md)). T05's #176 changed only
  conformance files and conformance.md, and reran the whole suite at its head to the same 6,208.
- **Every reason in conformance that quotes captured output now passes one shared mask**,
  `conformance/mask-common.ps1`, built when the harness loads it (#0243, #176). #0229 (the
  ProxyCommand connected an alias that was not ready) and #0230 are met too.
- **Nothing is running.** Both hosts are stopped, no lease is live, no milestone exists, the
  reporter is stopped, and every dev lead is archived. **Restart the board watcher first**
  ([Start here](#start-here), step 4).
- **v0.11 is installed on Terry's laptop**, and its keeper runs the new build.
- **No card waits on Terry.** #0244, carded at T05, is folded into v0.12 (D90).
- **Next is v0.12 (#0051), `linux/amd64` and `windows/amd64`, and it MUST NOT be laid out until
  Terry says so.** It is the first release since v0.09 that starts instances. Its prep is
  [The scope ahead](#the-scope-ahead-v012).

## Start here

**Do these in order before anything else.** Each comes from a gap that cost time in a release.

1. **Read, in this order:** this file; [AGENTS.md](../AGENTS.md), every standing rule;
   [architect.md](architect.md), how a release is run, from Terry's word to Completed;
   [decisions.md](decisions.md), D83 to D89 with their change notes, then D4, D7, D13, D25, D26,
   D59, D72, D78 and D82 for v0.12's design; then [roadmap.md](roadmap.md) and card #0051, which
   carries #0192, #0193, #0195's fix and #0228 whole.
2. **Check what is running.** A cleared session leaves no background task of its own, but an orphan
   can still hold a reader. Print PIDs only, never a command line whole:
   ```powershell
   Get-CimInstance Win32_Process -Filter "Name='pwsh.exe'" | Where-Object { $_.CommandLine -match 'report-milestones|watch\.ps1|watchdog\.ps1|heavy-gate' } | ForEach-Object { $_.ProcessId }
   ```
   Any process found MAY be stopped by its checked PID, with Terry's yes.
3. **Check the board answers:** `(Test-NetConnection 127.0.0.1 -Port 8811).TcpTestSucceeded`. If it
   is `False`, localswim is down, and starting it is Terry's; he MUST be asked.
4. **Restart the board watcher**, under the Monitor tool, with a forward-slash path, a 30-minute
   timeout, re-armed at each expiry:
   `pwsh -NoProfile -File 'C:/Projects/claude-localswim-watcher/watch.ps1' -Board agentspace -Reader agentspace 2>&1`.
   The reporter starts only once `scripts/milestones.json` lists a milestone; today it lists none.
5. **Check AWS by exit code alone, before any turn that needs it**, never printing its error:
   `aws sts get-caller-identity *> $null; $LASTEXITCODE`. A non-zero code means Terry runs
   `aws login --remote` and pastes its link into a fresh Incognito window, every other Incognito
   window closed (AGENTS.md). **It had expired at the start of v0.09-T05, v0.10-T05 and v0.11-T05**,
   so check it before every live turn, and run the turn's install and drafting while it waits. To
   see the hosts, run `scripts\live\mask-proof.ps1`, and only when it exits 0, `status -All` through
   `scripts\live\mask.ps1` ([The architect's tools](#the-architects-tools)). Both hosts SHOULD read
   `stopped`.
6. **Read the laptop's time zone with the clock, every time** (`Get-Date`, `(Get-TimeZone).Id`).
   Terry travels: it was Pacific on 2026-11-17 and Mountain through v0.10 and v0.11. In T05 the
   architect assumed Pacific and gave a turn's end an hour early.
7. **Wait for Terry's word before laying out v0.12** (AGENTS.md, "The board"). Until then, do the
   prep in [The scope ahead: v0.12](#the-scope-ahead-v012), and bring him the calls that are his.

## What changed from the v0.10 handoff, and why

- **This file stands alone, as v0.10's did.** v0.11's live record leaned on v0.10's handoff for its
  tools, lanes, pitfalls and rules; the parts still in use are carried here, updated, and v0.10's
  handoff is history.
- **"Rules that are easy to break" gains v0.11's four**, and rule 11 is restated: v0.11-T05 found
  that `Start-Process` for a run is wrong even with `-NoNewWindow`, since a detached start opens a
  Windows Terminal window on Terry's desktop.
- **"A turn, start to close, in commands" gains the heavy-run gate and the review checks v0.11
  used**: the combined merge check, the condition-set comparison and the recount from masked files.
- **"Numbers to plan by" gives the whole suite's three-lane wall time** and v0.11's turn lengths,
  for v0.12's caps.
- **Kept from v0.10's:** the short version first, "Start here", lessons grouped by what they are
  about with each lesson's home, the PR review in commands, the dev lead's brief as a template, and
  "Cards only Terry can move" apart from "What is open".

## Where things stand

**As of v0.11's close, 11:11am Mountain, 2026-11-26.** Every time in this file is the laptop's zone,
Mountain, when it was written.

| What | State |
|---|---|
| v0.11 (#0052), plan from the board | Completed: the epic and its thirteen stories (#0231 to #0243), approved by Terry on 2026-11-26, all fourteen checked in Completed at 11:11am. Marked done in [roadmap.md](roadmap.md) |
| v0.12 (#0051), `linux/amd64` and `windows/amd64` | In Ready For Work, not laid out. It MUST NOT be broken into stories until Terry says so |
| `main`'s tests | T04's record: 94 conformance files and the deadline case, 6,208 passed, 0 failed, 0 skipped; the crates 5,978 passed, 0 failed, 58 ignored; the box 172; the empty program 46 of 46. #176's whole-suite rerun at its head gave the same 6,208. T05 changed no crate. **Nothing is red by design** |
| The install on Terry's laptop | v0.11 from 6ec204d's code, a release build (92 s), installed by T05 at 7:31:00am on 2026-11-26: 8 of 8 copied, none renamed aside, no `.old-` file left. The keeper restarted on the new build; its task was ended and started, never disabled. No change to his ssh config, task definition or `PATH`. The installed `lease create --help` names `--board`, `--epic` and `--turn` |
| The hosts | Both `stopped`, 0 live leases (`status -All` at 10:34am on 2026-11-26). The box (`agentbox`, swimcoach's): 20 GiB. `agentspace-host-agentspace`: 8 GiB. Both at `m9g.xlarge` |
| Instance hours | None asked for and none used in v0.11. No instance has run since v0.09-T05 |
| Leases | 44 recorded before T05 (36 `released`, 2 `expired`, 6 `unsatisfiable`), plus T05's check leases, released; none `ready` or `pending` |
| The board | Every v0.11 card in Completed. #0244, #0228, #0192, #0193 and #0195's fix are folded into v0.12 (#0051) and archived (D72, D82, D90); #0190, #0082 and #0109 into v1.0 (#0053) |
| Swimcoach | No milestone; `scripts/milestones.json` is empty (4838418), `v0.11` was stopped at its approval, and the reporter is stopped |
| Sessions | Every dev lead archived; `git worktree list` shows `main` alone. `.claude\worktrees\rvmain` is an unregistered folder from 2026-11-23, one entry, not a checkout, left for a person |
| Terry's laptop, otherwise | Unchanged by v0.11 beyond the install. In T05, three queued suite runs started with `Start-Process` opened three blank Windows Terminal windows on his desktop at 8:38am; they closed with their runs |

### v0.11 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs | #0231 to #0233 | 12:45pm, 2026-11-25 | 3:53pm, 2026-11-25 | #163 (2:30pm), #164 (3:14pm), #165 (3:53pm) | 3:08 of 3 h | Contract v2, `schemas/` v2 with v1's kept, #0229's refusal in operator.md, the plan's source in state.md, the stand-in board in conformance.md; 27 pins ruled; 19 subagents launched |
| T02, the foundations and #0229 | #0234 to #0237 | 4:20pm, 2026-11-25 | 7:10pm, 2026-11-25 | #166 (the architect's prep, 4:44pm); #167 to #170 (7:10pm) | 2:50 of 3.5 h | `agentspace-plan`'s budget, turn cut and TTL; `agentspace-board`, new, with `ureq` 3.5.2 and `Token::bearer`; `agentspace-state`'s `plan_source`; the ProxyCommand's refusal; 6 pins ruled; 24 launches, 9 ending blocked or stopped without writing; Terry approved two Writes by hand |
| T03, the lease CLI | #0238 | 7:45pm, 2026-11-25 | 10:16pm, 2026-11-25 | #171 (10:16pm) | 2:32 of 3 h | `create --board`, `--epic` and `--turn`, the TTL from a plan, contract v2, `tests/lease_board.rs` (64 tests); 8 pins ruled; 4 launches, none blocked |
| T04, conformance | #0239 to #0241 | 11:24pm, 2026-11-25 | 2:19am, 2026-11-26 | #173 and #172 (1:51am), #174 (2:19am) | 2:55 of 4 h | `board-standin.ps1` and three new lease CLI files; #0230 met; the whole suite's record; #0243 carded; 15 pins ruled; 18 launches, none blocked |
| T05, the install, the live check and the shared mask | #0242, #0243 | 7:23am, 2026-11-26 | 10:31am, 2026-11-26 | #175 (8:10am), #176 (10:31am) | 3:08 of 4.5 h | v0.11 installed; D88's live check green in 3 min 27 s, no instance; the shared mask; #0244 carded; 15 launches, peak 4, none blocked |

**The release used 14:33 of its 18 hours** (15.5 laid out, raised to 18 when #0243 joined T05).

## What v0.11 built

| Turn | Story | Card | PR | What |
|---|---|---|---|---|
| T01 | E0.11-T01-S01 | #0231 | #164 | lease-api.md becomes contract v2: `--board`, `--epic`, `--turn`, the plan block and the TTL from a plan |
| T01 | E0.11-T01-S02 | #0232 | #163 | `schemas/` for contract v2, v1's kept under `schemas/v1/` |
| T01 | E0.11-T01-S03 | #0233 | #165 | state.md, operator.md, README and conformance.md: the plan's source, the ProxyCommand's refusal (#0229) and the stand-in board |
| T02 | (architect's prep) | | #166 | `Record`'s `plan_source` as pinned code, and `scripts\heavy-gate.ps1` |
| T02 | E0.11-T02-S01 | #0234 | #167 | `agentspace-plan`: `budget_minutes`, the turn cut and the TTL factor |
| T02 | E0.11-T02-S02 | #0235 | #169 | `agentspace-board`: the plan block and the one GET that reads an epic |
| T02 | E0.11-T02-S03 | #0236 | #168 | `agentspace-state` reads and writes the lease record's `plan_source` |
| T02 | E0.11-T02-S04 | #0237 | #170 | The ProxyCommand refuses a lease alias whose record is not ready (#0229) |
| T03 | E0.11-T03-S01 | #0238 | #171 | The lease CLI's `--board`, `--epic` and `--turn`, the TTL from a plan, and contract v2 |
| T04 | E0.11-T04-S01 | #0239 | #173 | Conformance for board leases, `--turn`, the TTL from a plan, and contract v2 |
| T04 | E0.11-T04-S02 | #0240 | #172 | `Get-HpLogShape` masks the log lines it quotes (#0230) |
| T04 | E0.11-T04-S03 | #0241 | #174 | The whole suite, the crates and the box tests against `main`'s build ([runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md)) |
| T05 | E0.11-T05-S01 | #0242 | #175 | The install and D88's live check of a board lease ([runs/e0.11-t05-s01.md](runs/e0.11-t05-s01.md)) |
| T05 | E0.11-T05-S02 | #0243 | #176 | Every reason in conformance quotes captured output through one shared mask |

**The design is D83 to D88, and D89 lays it out.** D83 is the plan block, D84 `--turn`, D85 the
TTL, D86 the board read and its credential (Q17), D87 contract v2, D88 the done-when shown with no
instance (Q18), and D89 the layout with its change notes, the last for #0243 joining T05.

## Numbers to plan by

**Measured in v0.11, for v0.12's caps and staffing.** Each comes from a record or a command, cited.

| What | Figure | Source |
|---|---|---|
| A turn's length against its cap | T01 3:08 of 3 h; T02 2:50 of 3.5 h; T03 2:32 of 3 h; T04 2:55 of 4 h; T05 3:08 of 4.5 h. The release 14:33 of 18 h | This file, "v0.11 by turn" |
| Peak subagents laid out per turn | T01 8, T02 10, T03 3, T04 8, T05 4 (2 before #0243) | D89 and its changes |
| Subagent launches per turn | T01 19, T02 24, T03 4, T04 18, T05 15 | The dev leads' counts |
| The whole conformance suite | 94 files and the deadline case, 6,208 cases: the lease CLI 2,079 (46 files), the operator 1,536 (39), the keeper 787 (7), the schemas 1,801 (2), the deadline case 5 | runs/e0.11-t04-s03.md; #176 |
| The whole suite in three gated lanes | 41 to 47 minutes: the lease CLI with the empty program 41; operator lane A with the deadline case 46; operator lane B with the keeper and the schemas 47 (#176, 9:41am to 10:28am) | #176's body |
| The longest files | `test-keeper.ps1` 664 s; `test-unmet-host-deploy-size.ps1` about 680 s; `test-unmet-sync-patch-wait.ps1` about 440 s. Balance lanes by seconds | #176; architect.md |
| The whole workspace's tests | 5,978 passed, 58 ignored, 1,550 s (about 26 minutes) | runs/e0.11-t04-s03.md |
| A release build, `cargo xtask dist --release` | 92 s, eight files | runs/e0.11-t05-s01.md |
| The install, end to end | 33 s of wrappers (7:30:45am to 7:31:18am) | runs/e0.11-t05-s01.md |
| D88's live check | 3 min 27 s, no step rerun | T05's dev lead |
| Heavy runs at once on the laptop | Three, gated by `scripts\heavy-gate.ps1` | architect.md, "A turn that builds" |
| The footprint and the sizes (D73) | Half a vCPU and 2 GiB per agent at once; `s` up to 4 agents, `m` 8, `l` 16, `xl` 32; more is `plan_too_large` | D73; AGENTS.md |
| A resize, for a turn that uses a host (v0.09) | `m` to `l` 176.8 s; `l` to `m` 257.6 s | runs/e0.09-t05-s01.md |
| Live-turn cost (v0.09) | 0.534 instance hours for a full live check list on two hosts | runs/e0.09-t05-s01.md |

## Terry's orders from v0.11

**Each binds where its row says; this table only finds them.**

| Order | Given | Where it binds |
|---|---|---|
| v0.11's prep: settle the design calls as numbered decisions, bring him his calls, and do not lay it out until he says so | 2026-11-25 | D83 to D88 |
| "I'll take whatever recommendation you prefer", on Q17 and Q18: (a) on both | 2026-11-25 | D86, D88 |
| "Break v0.11 into stories. Make sure to make a staffing plan and show me peak subagents per turn and where the agents run (laptop or EC2)" | 2026-11-25 | D89; each story's card |
| "Start T01" to "Start T05", each turn on his word | 2026-11-25 and 26 | This file |
| "Prompt me per write", for the Writes auto mode refused in T02 | 2026-11-25 | architect.md, "Running a turn", rule 12 |
| #0243 into v0.11, not v0.12 ("uhhh no v0.11 please"), as a story of T05 ("Let's pull risk early") | 2026-11-26 | D89's change during v0.11-T05 |
| AWS's sign-in renewed during T05 ("AWS login refreshed") | 2026-11-26 | runs/e0.11-t05-s01.md |
| The blank windows on his desktop reported ("several blank powershell windows open"), which found the `Start-Process` slip | 2026-11-26 | architect.md, "A turn that builds", v0.11-T05 |
| v0.11 approved into Completed | 2026-11-26 | The board |
| This handoff, based on the one the architect started v0.11 with, for his `/clear` | 2026-11-26 | This file |

## Rules that are easy to break

**Each was broken at least once in v0.08 to v0.11. Each has its home, which wins.**

1. **A time, a SHA, a count or an interval MUST come from a command's output, never ahead of it or
   from memory, and a time's zone is read with it.** Broken in v0.11 by four ruling times in T01 and
   T02, and by T05's end given an hour early from an assumed zone (AGENTS.md, Writing; architect.md,
   "Writing in the repository").
2. **Nothing of Terry's MAY reach a transcript**: not his user name, profile path in either form,
   computer name, a lease ID or alias, or an AWS identifier (AGENTS.md, masking). Scratch lives
   inside the checkout's `target\`; listings are bare; a conformance reason quoting captured text
   goes through `ConvertTo-ConformanceMasked`.
3. **Read the source before ruling or citing.** Broken in v0.11-T04: a ruling named a helper's
   loaders from a file name, and `grep -l` found the real ten (architect.md, "Running a turn", rule 16).
4. **A merge check runs every suite the merged code feeds**, and a later fix reruns every file that
   loads what its diff changed, found by `grep -l` (architect.md, "A turn that builds").
5. **Every checker is proven to fail on a planted value before its pass is believed** (Terry's
   global rule; architect.md).
6. **Every open row of the last handoff is carried into the next**, or says what became of it
   (architect.md, "Closing a release"). This file's [What is open](#what-is-open) accounts for
   v0.10's.
7. **The architect is the only agent that moves a card, and a card leaves Backlog only on Terry's
   word** (AGENTS.md, "Roles and pull requests").
8. **PowerShell names are case-insensitive**: `$b` overwrote `$B`, and a helper whose name matches
   an alias runs the alias (AGENTS.md, "The board").
9. **A test on a by-design red list is checked by its failing assertion**, never by its name
   (architect.md).
10. **`stop` goes to swimcoach only on Terry's word for a release; a turn's milestone stops at its
    dev lead's archive** (AGENTS.md, "Status on Terry's phone").
11. **No run is started with `Start-Process`.** With `-WindowStyle Hidden` it runs at error mode 0
    and hides the dialogs that follow (v0.10-T02); detached, it opens a Windows Terminal window on
    Terry's desktop (v0.11-T05); `-NoNewWindow` is the same slip. Runs go through the gate in the
    shell tool's foreground or its own background option, and the wrapper sets 0x8001 (architect.md,
    "A turn that builds"; `scripts\heavy-gate.ps1`, `scripts\live\wrap.ps1`).
12. **At most three heavy runs at once on the laptop**, through `scripts\heavy-gate.ps1` (architect.md).
13. **A card waiting on Terry goes to Needs Terry as soon as only he is left**, so its clock pauses
    and it reaches his phone (AGENTS.md, "Status on Terry's phone").
14. **A refusal whose text says not to retry is never retried by anyone**; it goes to Terry, who MAY
    approve the Write by hand (architect.md, "Running a turn", rule 12). Broken in v0.11-T02.
15. **Only `EnterWorktree` pins a subagent; a `cd` does not**, and a subagent cannot move itself
    (AGENTS.md, worktree rules). Broken in v0.11-T02: every write of two subagents refused.
16. **A step that MUST happen under one permission mode reads the mode right before it**: the mode
    moved under the session twice in v0.11-T02 (architect.md, "Running a turn", rule 14).
17. **A run tests a frozen copy.** Code under a running test is never edited, stashed or switched:
    the architect's `git stash -u` in T02, and a dev lead's edit under a running suite in T05.
    Suites on a head that may move run from `git archive <sha>` (architect.md, "A turn that builds").

## Lessons from v0.11, by what they are about

**Each lesson says what to do and where it is now written. Every lesson of the release is here.**

### Terry's details and real files

| What happened | What to do | Where |
|---|---|---|
| T04: #0240's reviewer found about 40 conformance reasons quoting captured output unmasked or half masked | One shared mask for every reason, proven per formatter by a planted case | #0243; conformance.md, "The shared mask" |
| T05: the shared mask, built on first use, missed the real profile's 8.3 form wherever the keeper had moved its own process's home (F1) | A mask that reads the environment builds its rules at load, shown by a planted case that moves the environment first | architect.md, "A turn that builds", v0.11-T05; conformance.md |
| T05: two places print captured output outside a reason, unmasked (N2, O1) | Carded, not fixed in place: masking a run's stderr at its source changes what 63 files assert on | #0244 |
| T04: `scripts\live\mask.ps1` turned a word after `token: ` into `<secret>` (N9) | Not a defect: the mask errs toward hiding | `scripts\live\mask.ps1` |

### Facts, times and counts

| What happened | What to do | Where |
|---|---|---|
| T01 and T02: four ruling times or intervals not read from a command | A message that carries a time is sent only after the clock's read has returned; an interval is computed from two reads | architect.md, "Writing in the repository" |
| T05: T05's end given as 10:53am by assuming Pacific; the laptop was on Mountain | The zone is read with the clock | [Start here](#start-here), step 6 |
| T05: an empty-program driver's wrapper recorded exit 1 for a run that counted 46 of 46 | A driver ends with an explicit `exit 0` after its failure check | architect.md, "A turn that builds", v0.11-T05 |
| T05: a case-blind `^FAIL` search matched a runner's `Failed: none` summaries | Count failures case-sensitively | architect.md, "A turn that builds", v0.11-T05 |

### Rulings and diagnosis

| What happened | What to do | Where |
|---|---|---|
| T01: P1 to P10 missed three facts localswim's `openapi.yaml` held (P17 to P19) | A sweep of another system's API reads its `servers` entry and each path parameter's schema | architect.md, "A turn that writes specs" |
| T01: #0233 took three fix passes, each designing a failure mode of #0229's refusal | A decision that adds a fail-safe rule designs its failure modes before the layout | architect.md, "Breaking an epic into stories" |
| T04: ruling 4 named a helper's loader from its name | `grep -l`, never a file's name | architect.md, "Running a turn", rule 16 |
| T04: the 11:29pm rulings reached a busy dev lead at 12:13am | A ruling sent to a busy session is confirmed by its reply | architect.md, "Running a turn", rule 15 |
| T05: the first fix for the blank windows (`-WindowStyle Hidden`) would have broken v0.10's error-mode rule; the dev lead caught it | A new rule is checked against the slip list before it is sent | architect.md, "A turn that builds", v0.11-T05 |
| T05: F1 had two fixes, a load-time mask (A) or a MUST on callers (B); A was taken | A guarantee whose miss is silent is built in, not left to a rule a later author can miss | conformance.md, "The shared mask" |

### Running a turn and its subagents

| What happened | What to do | Where |
|---|---|---|
| T01: the dev lead's own edits were refused 8 times while pinned to another story's worktree | A dev lead enters the story's worktree before its own edits too | The next brief |
| T01: a "no verdict" refusal passed on one word-for-word retry | MAY be retried once, only when its text allows | architect.md, "Running a turn", rule 12 |
| T02: two subagents followed a `cd` into the next worktree | Only `EnterWorktree` pins a subagent | AGENTS.md |
| T02: the permission mode moved under the session twice | Read the mode before a step that needs one | architect.md, rule 14 |
| T03: a worktree-isolated session could not add a second worktree; T05 found the route | `git archive` of `main` into `target\` for a baseline; or `ExitWorktree` (keep), `git worktree add` from the main checkout, `EnterWorktree` | architect.md, "A turn that builds" |
| T03: the implementer ended its run waiting on its own gated run | Taskings keep "finish in one run" | The next brief |
| T04: the dev lead went quiet for 44 minutes writing taskings | A dev lead arms a 9-minute heartbeat under Monitor at its start | architect.md, "A turn that builds" |
| T05: a ruling changed mid-turn went to a fresh drafter, and no subagent waited | Never message a running subagent with a change | architect.md, "A turn that builds", v0.11-T05 |
| T05: two stories that shared no file ran beside each other inside one budget | Pull risk into a turn when files do not overlap | architect.md, "A turn that builds", v0.11-T05 |
| T05: the architect's `Set-Location` into S02's worktree moved its own session there | Read another session's worktree by path or `git -C` | architect.md, "Running a turn", rule 17 |

### Builds, tests and load

| What happened | What to do | Where |
|---|---|---|
| T01: three merges ran in series for 83 minutes behind 40-minute checks | Plan a spec turn's merge checks into its budget; run the suites that read the changed files at run time | architect.md, "A turn that writes specs" |
| T01: three reds were found only by reviewers' workspace runs | A red list is confirmed by a workspace run before its cards are final | architect.md, "A turn that writes specs" |
| T01: the dev lead wrote a gate per worktree | `scripts\heavy-gate.ps1`, committed (#166) | `scripts\` |
| T02: four merge checks became one combined check of about an hour, trees equal | A build turn's PRs are merge-checked together on one local merge | architect.md, "A turn that builds" |
| T02: the architect's `git stash -u` set aside code a running test was testing | A test run of uncommitted work runs in its own worktree | architect.md |
| T02: `wrap.ps1` refused every `-Name` not starting with three digits | `-Out target\runs\<role> -Name <nnn>-<what>` | architect.md |
| T03: a workspace run made before every owner was done compiled 19 of 64 tests | Final only once every owner is done | architect.md |
| T03: a test comparing two runs' clock times failed under load | Never compare clock times across runs | architect.md |
| T03: a test accepting either of two outcomes | One outcome, shown to fail on the other | architect.md |
| T04: final runs on one local merge of two PRs stood as the merge check | MAY, when the architect merges the same heads and compares trees | architect.md |
| T04: a mid-turn change to lease-api.md on `main` was shown harmless to early runs | Name the spec's runtime readers and what each reads | architect.md |
| T05: a fix after the suites reran only its diff's loaders: 15 files, not 94 | `grep -l` over every folder, at the new head | architect.md, "A turn that builds", v0.11-T05 |

### Live checks on real hosts

| What happened | What to do | Where |
|---|---|---|
| T05: the sign-in had expired at the turn's start, a third release in a row | Check it by exit code before the turn; the install and drafting run while it waits | [Start here](#start-here), step 5 |
| T05: every step was dry-run in harness homes against the worktree's build, and the live check ran in 3 min 27 s with no rerun | Dry-run a live check's scripts first | architect.md, live checks |
| T05: board revision reads before and after the check (1902 both) proved it wrote nothing | Bracket a read-only live check with the target's own revision | runs/e0.11-t05-s01.md |

## Pitfalls of the laptop's tooling

**None is agentspace's design; each cost a retry in v0.07 to v0.11.**

| Pitfall | What works |
|---|---|
| The Monitor tool runs bash, which drops a Windows path's backslashes | `'C:/Projects/claude-localswim-watcher/watch.ps1'` |
| `%USERPROFILE%` does not expand in PowerShell, Terry's shell | `& "$env:USERPROFILE\.agentspace\bin\agentspace.exe" ...` in anything sent to him |
| `lease list` prints `{"leases": [...]}`, and its documents have no `ssh` field | Read `.leases`; the alias is `agentspace-<project>-<first 8 hex digits of id>` (lease-api.md) |
| PowerShell's `-like` reads a backtick as an escape | Compare with `-eq` or `.Contains()` |
| `ConvertFrom-Json` turns ISO 8601 strings into local `DateTime`s | `-DateKind String`; never rewrite `milestones.json` from a parse |
| Windows Terminal is the default console host: a process started detached with a console of its own opens a terminal window on Terry's desktop, owned by `WindowsTerminal.exe`, so the `pwsh` inside shows no window handle | No `Start-Process` for a run; to find such windows, look for console processes whose own child is `conhost.exe` |
| A `foreach (...) { }` statement cannot be piped | Assign it to a variable, then pipe the variable |
| `"$name: ..."` in a double-quoted string parses as a scoped variable | `"${name}: ..."` |
| The localswim helper's cleanup can leave `$LASTEXITCODE` unset under strict mode, so a script fails after a call that succeeded | Read the call's own JSON for its result; never retry a create before checking with `card get` |
| `card list` returns 50 cards a page | `card get <board> <n>` by number; a new card's ticket is the next number after the highest |
| A Python string in a bash heredoc turns `\r`, `\a` or `\v` in a path into a control character | Raw strings; count control characters after a scripted edit |
| A here-string piped to `git commit -F -` reaches git as an argument | A message file, or a one-line `-m` with `` `n `` |
| `pwsh -File script.ps1 -Files a,b` passes the list as one string | Pass one comma-separated string and split it in the script |
| A relative `-Out` given to `run-conformance-files.ps1` doubles its path | Pass absolute paths |
| A shell command that reads stdin with nothing redirected hangs until its timeout | Give every command its input, or none |
| A review worktree under the scratchpad's long path fails on box/tests' fixtures | Put review worktrees under `.claude\worktrees\` |
| Git in a worktree-isolated session refuses compound commands | One git step per call |
| `git worktree remove --force` deletes the worktree's ignored `target\` | Copy run files out first |
| A worktree folder stays held by the session that was in it | Remove the empty folder after that session is archived |
| `archive_session` is refused once by a hook that relabels the call | Resend with exactly the input the refusal quotes |
| `start_session` is refused on a turn an event started, not Terry | Propose the session as a card; Terry starts it with one click |
| Bot's card moves out of Backlog or into Completed answer HTTP 403 | Terry moves a card out of Backlog and approves it into Completed |
| `agentspace-operator status` prints on stderr | Capture `2>&1` |
| `localswim` on `PATH` is stale | `C:\Projects\localswim\localswim.exe`, by full path, as Bot |
| `swimcoach` is not on `PATH` | `C:\Projects\swimcoach\bin\swimcoach.cmd` |

## The architect's tools

| Tool | What it does |
|---|---|
| `scripts/scan-identifiers.ps1 -Head <ref> -Base origin/main [-Pr <n>]` | The identifier scan of a diff, its commits and a PR's body, each pattern proven on a planted value first; prints counts, never values |
| `scripts/run-conformance-files.ps1 -Checkout <worktree> -Out <absolute folder> -Files '<a>,<b>' [-Dist <folder>]` | Runs conformance files against a build's programs; ends `Failed: none` and exit 0, or the failed files and exit 1 |
| `scripts\heavy-gate.ps1 -Out <folder> -Name <nnn-what> -Command '<PowerShell>' [-Slots 1\|2]` | Waits fairly for one of three heavy-run slots, then runs the command through `wrap.ps1`, exiting with its code; `-HoldMinutes` holds a slot |
| `scripts/watchdog.ps1 -Lead <name>` and `-Reset` | The 10-minute watchdog on a dev lead; reset it on every message and turn note, and restart it with `-Reset` first |
| `scripts/report-milestones.ps1 -Watch` | The swimcoach reporter, whenever a milestone exists |
| `scripts/milestones.json` | The milestones, edited as text only, with the real time |
| `C:\Projects\swimcoach\bin\swimcoach.cmd stop --project agentspace --milestone <name>` | Stops a milestone, after it is out of `milestones.json` |
| `scripts\live\mask-proof.ps1` | Proves the live mask and the scan; prints only counts and `True`/`False`. Run it first, every live session |
| `scripts\live\mask.ps1` | Dot-sourced: `ConvertTo-LiveMasked -Text <text>` |
| `scripts\live\wrap.ps1 -Out <folder> -Name <nnn-what> -Command '<PowerShell>'` | One command at error mode 0x8001 or more, logged in `.start`, streams to files, `.exit` last, a masked copy to print |
| `scripts\live\scan.ps1 -File <file>` | Exits 1 if the mask would change any line of a file |
| `conformance/mask-common.ps1` | Conformance's own mask, `ConvertTo-ConformanceMasked`, loaded by the harness (conformance.md, "The shared mask") |
| `agentspace lease create --board <id> --epic <ticket> [--turn <name>]` | A lease sized and timed from an epic's plan block, read as Bot (D86); `--plan <file>` still works |

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
| `card get <board> <n>` | A card by bare ticket number (`244`, not `#0244`); JSON with `ticket`, `lane_id`, `revision`, `description`, `updated_at` |
| `card move <board> <n> <lane ID>` | A move; only the architect's. Confirm with `card get`'s `lane_id` |
| `card create --description <text> <board> <lane> <title>` | A new card; confirm with `card get` on the next ticket number before any retry |
| `card description --revision <r> <board> <n> <file>` | Replaces a description from a file; read the revision with `card get` first |
| `relationship create <board> <source> <kind> <target>` | `blocks`, `references` or `relates_to` |
| `board get --view lanes <board>` | The lanes, with their keys and labels |

**A PR review, in commands** (architect.md, "Reviewing a pull request", gives the why):
1. `git fetch origin`, and check `gh pr view <n> --json headRefOid` against the dev lead's SHA.
2. **What moved on `main`:** `git diff --name-only <merge-base> origin/main`, and a grep of
   `conformance` and `crates` for any runtime read of those files. Nothing read means no rebase and
   no rerun.
3. **Read the diff**: every new file whole, and the changed conditions compared as sets between the
   removed and added lines, so a hunk that changes an outcome cannot hide among masking edits.
4. **Recount the results from the masked files yourself**: files, passed, failed, skipped and
   nonzero exits per folder, and every `.exit`. A count in a PR body is a claim.
5. `scripts/scan-identifiers.ps1`, and the commits' author and committer against `main`'s identity.
6. `gh pr merge <n> --squash --match-head-commit <sha>`; only once it reports `MERGED`, delete the
   branch, move the card, and tell the dev lead.

## A turn, start to close, in commands

architect.md, "Running a turn", gives the why; this is the order.

**At the start, on Terry's "Start Tnn":**
1. `git pull --ff-only origin main`, and check every PR of the turn before is merged.
2. Sweep the turn's cards against `main` (architect.md, step 2): every file, command and list a card
   names, read, not grepped. Check AWS by exit code if the turn needs it.
3. Add the turn's milestone to `scripts/milestones.json` as text, with its start in UTC from a
   command, `cap_hours` from the layout decision and the turn's tickets; commit, push.
4. Move every story of the turn to In Progress (`29e4255d-16c5-4d43-886a-a698a2b010c5`), and the
   epic with its first.
5. If the turn uses a host, its lease is asked with `--board c74ddc68-9c3a-4434-8435-563c596d3229
   --epic <ticket> --turn <Tnn>` (or `--plan`), never sized by hand (AGENTS.md, sizing).
6. `start_session` with the brief below and `name` `vXYY-tnn`, then
   `scripts\watchdog.ps1 -Lead <name> -Reset` and `scripts\watchdog.ps1 -Lead <name>` in the
   background.

**While it runs:** reset the watchdog on every message and turn note; when it fires, check whether
the dev lead or its subagents are busy (`get_session`, and the runs' `.start` and `.exit` files)
before nudging; answer pins as Terry's proxy, writing each ruling where it binds; move a card to
Needs Terry the moment only he is left; review and merge each PR as above.

**At the close, on the dev lead's DONE:**
1. Stop the watchdog. Archive the dev lead (resend the call's quoted input once if a hook relabels
   it), then remove any empty worktree folder it held.
2. Remove the turn's milestone from `scripts/milestones.json` as text, commit, push, `swimcoach stop`.
3. If the turn used a host: fetch what it left, release the lease, `down`, then `status -All`.
4. Refresh the live handoff (the turn's row, its rulings, its lessons, "What is open") and
   architect.md's lessons; commit and push.
5. Report to Terry, with the next turn's start as his call.

**A dev lead's brief, as a template.** `start_session` with `initiation: "user_asked"` on Terry's
"Start Tnn", `context: "fresh"`, `use_worktree: false`, `effort: "high"`, a short `name`. In order:
1. **Its cards and branches**, the base commit, the turn's start time and cap, that the cards are
   already In Progress, and its watcher reader (`agentspace-vX.YY-tnn`).
2. **What to read first:** AGENTS.md, the cards to their end, the decisions, this file's lessons,
   and the last live record when the turn is live.
3. **The rulings already made for the turn**, numbered, each with its reason: the sweep's findings,
   whose files each story owns, the heavy-run gate, and any gate that waits on Terry.
4. **The rules**, as architect.md, "Running a turn", step 7 lists them, with: a 9-minute heartbeat
   under Monitor; `EnterWorktree` before every story's launches; finish in one run.
5. **The slips list**, every slip of the release so far, **quoted whole into every tasking**. From
   v0.11 it starts with: no `Start-Process` for a run, hidden or not; a driver ends with `exit 0`;
   suites run from `git archive <sha>`; a later fix reruns its diff's loaders by `grep -l`; count
   failures case-sensitively from masked files only; no scratch under `%TEMP%`; never discard
   another's change; one outcome per test, shown to fail on the other.
6. **What done is**: the PR's head sent and nothing pushed after; after the merge, worktrees and
   branches removed, watchers stopped, and DONE with lessons.

## Rulings made during v0.11

**T01's dev lead raised pins P1 to P27, T02's P1 to P6, T03's P1 to P8, T04's P1 to P15 and T05's
P1 to P12 with N1 to N3, F1, F2 and O1; each turn numbered its own.** Each is cited where it ruled,
as "architect, <date>, for v0.11-Tnn, pin Pn". The ones that changed a decision are change notes in
decisions.md; this table only finds them.

| Ruling | Where it binds |
|---|---|
| T01 P1, P2, P9: a 64 MiB answer bound; a second GET after a 404 to tell a board from a ticket; no seam variable for the receipt | D86's change for v0.11-T01 |
| T01 P17 to P19: localswim's `/api/2026-11-10` prefix and its `not_found` Problem document; UUIDv4 boards; a `http://127.0.0.1:<port>` base alone | D89's change for v0.11-T01 |
| T01 P21, P22: the ProxyCommand finds a record by the alias its project and ID build, refuses one not `ready`, and refuses a lease-form argument when the lease file cannot be read | D49's change for v0.11; operator.md |
| T01 P23, P27: a sharing violation is retried, 3 attempts within 1 second, 400 ms apart | operator.md; #0237 |
| T01 P15, P25: the lease file stays at version 0, so v0.11's programs MUST be installed together; `plan_source` is written last | state.md |
| T01 P24: `budget_minutes` is written as digits alone | lease-api.md |
| T02 P1, P5: `agentspace-push` gains `Token::bearer`, a `Bearer` with a redacted `Debug`, no `Display` and no `Clone` | `agentspace-push` (#169) |
| T02 P3: the board client's `ureq` agent has no proxy, no redirects, a 30 s timeout and statuses not as errors | `agentspace-board` (#169) |
| T02 P6: the board read refuses a receipt that is not UTF-8 | `agentspace-board`; #169 |
| The 64 MiB bound counts body bytes, not a transport's read-ahead of at most 128 KiB | lease-api.md (429b51d) |
| #0243 joins T05 as E0.11-T05-S02; T05's budget 4.5 h and the release's 18 h | D89's change during v0.11-T05 |
| T05 N1: the shared mask is dot-sourced once, by the harness, reached by the operator and keeper through their common files | conformance.md, "The shared mask" |
| T05 F1: the mask builds its rules at load; the harness MUST be loaded before a check moves its own process's `USERPROFILE`, `HOME` or `TEMP` | `conformance/mask-common.ps1`; conformance.md |
| T05 N3: the schema checks load the harness for the mask, since `Test-Json`'s message can name a path | conformance.md |
| T05 N2, O1: the harness's start-failure stderr and the keeper's re-print stay unmasked at their source | #0244, folded into v0.12 (D90) |
| T05: no `Start-Process` for a gate call; a driver ends with `exit 0` | architect.md, "A turn that builds", v0.11-T05 |
| The rest | The specs and cards, each where it is cited |

## Cards only Terry can move

**Bot cannot move a card out of Backlog, or into Completed (HTTP 403).**

| Card | What | Its evidence | His call |
|---|---|---|---|
| #0244 | Hardening: `conformance/lease-cli/harness.ps1` line 171 puts a start failure's exception message, which can name a path under the profile, into a run's stderr unmasked; `conformance/keeper/keeper-common.ps1` line 644 re-prints a child run's non-result lines unmasked. Neither has fired | #0243's review, N2 and O1; `relates_to` #0243 | **Settled, 2026-11-26** ("fold 0244 into v0.12"): folded whole into #0051 for its conformance turn, `references` from #0051, and archived (D90) |

**No card waits on Terry at v0.11's close.**

## What is open

**Rechecked at close, 11:11am Mountain, 2026-11-26. Every item names who acts.**

| What | Who acts | Where it goes |
|---|---|---|
| v0.12 (#0051) is laid out only on Terry's word | Terry, then the architect | [The scope ahead](#the-scope-ahead-v012) |
| #0228; #0192, #0193 and #0195's fix (D78); #0244 | v0.12 | #0051 (D72, D78, D82, D90) |
| #0190, #0082 and #0109; a footprint per project (D73) | v1.0's first turn, before the real project measures its own turn | #0053 (D72, D73) |
| v0.09-T05's observer script (`observe-once.ps1`), lost with its worktree; not needed while no turn starts an instance | The architect, before v0.12's first live turn, rebuilt from runs/e0.09-t05-s01.md | `scripts\live\` |
| A spec for what "time to a prompt" measures when the keeper starts a lease's holder first | The next release that specifies the lifecycle; not yet placed | v0.08's live records |
| Two limits accepted in #169 and #170: `ureq` copies the bearer into a header value that is not zeroed on drop; a sharing violation on the lease file's second open, after a failure of another kind, is not retried. Neither showed in T04's suite or T05's live check | Carried, not carded: a hardening pass on the board client or the lease file reads them first | #169, #170 |
| The empty `.claude\worktrees\rvmain` from 2026-11-23 | A person, if wanted | Nothing depends on it |

**What became of v0.10's open rows:** v0.11 laid out on Terry's word (D89, done); #0229 and #0230
met in v0.11 (#170, #172) and Completed with it; #0228 carried above to v0.12; the observer script,
the "time to a prompt" spec, #0192, #0193 and #0195's fix, and #0190, #0082, #0109 and the footprint
per project carried above as they were.

**What became of the live record's own rows:** the heavy-run gate committed and in use (#166, done);
#0052's block and plans/v0.11.json gaining E0.11-T05-S02 (a761c95, done); #0243 met (#176); the
limits of #169 and #170 carried above.

## The scope ahead: v0.12

**v0.12 (#0051): `linux/amd64` and `windows/amd64`**, the second and third platforms beside the
arm64 default, built in parallel (D4, D13, D26; roadmap.md). Done when `create --platform
linux/amd64` gives a ready lease on an amd64 host with the same profile and sizes as arm64; `create
--platform windows/amd64` gives a ready lease on a Windows host where an agent session, started from
the Claude desktop app over the lease, reaches the project's board and works a card, and releasing
the lease stops the host as v0.08 stops a Linux one; and arm64 stays the default, unchanged (card
#0051).

**Its stories MUST NOT be laid out until Terry says so.**

**Its shape, set on #0051:** a first, shared turn makes the driver, the lease state and placement
serve more than one platform, with a seam per platform (its image, its instance family, its
bootstrap), so the two tracks touch different files; then a `linux/amd64` track and a Windows track
run as parallel stories in the same turns, and the release ends when the Windows track does.
#0192, #0193 and #0195's fix (D78) ride with the shared turn, which extends `box/system`; #0228
rides with the driver work; #0244 rides with its conformance turn (D90).

**Before its stories are laid out, the architect SHOULD settle, each as a numbered decision:**
- **One host per project, or one per project and platform.** D25 gives a project one host; #0051
  says a project MAY hold one host per platform. That changes D25, the registry's key, the guard's
  view of an unexplained host and the host aliases (`agentspace-host-<project>`), so it is settled
  first; everything in the shared turn leans on it.
- **What the contract shows.** Today `--platform linux/amd64` and `windows/amd64` answer
  `unsatisfiable` with `unsupported_platform` (lease-api.md; D5); a `ready` answer for them, a document that names a host's
  platform, or a Windows lease's different `ssh` shape each touch v2's schemas. Whether that is v3,
  or output that v2 already allows, is decided against lease-api.md's Versioning section (D76, D87).
- **Whether a plan names its platform.** D74's plan carries turns, stories and subagents only; a
  plan lease today takes its platform from `--platform`. Whether a plan or an epic's block MAY name
  one, so a board lease for a Windows project needs no flag, is a plan-document question (D74, D83).
- **The Windows track's own calls, listed on #0051:** the lease's identity on Windows (the user an
  agent runs as, and SSH certificates through Windows OpenSSH's trusted CA keys, D7, or what
  replaces them); the host agent, meaning what `box/` does on Linux (patching, idle stop, the tools,
  the board's wrapper, the token push and the time zone, D12, D32) done on Windows; the
  `agent-windows` profile (D4: Git, Node LTS, Claude Code, gh, uv, PowerShell 7); and
  `windows/arm64` if AWS offers it.
- **#0228's ruling on D59's further walks**, kept or removed once one walk ends a chain of any depth,
  recorded in D59 and driver.md (#0228's card).
- **How each track is shown live.** Unlike v0.10 and v0.11, v0.12's done-when needs real hosts: an
  amd64 host and a Windows host, each made, leased, used and stopped. The live checks are planned
  with the observer script rebuilt and the AWS sign-in checked first.

**Calls that are Terry's, to bring him with the design (D19):**
- **Instance hours and their cost** for both tracks' live checks, as Q11 and Q16 asked for v0.09 and
  v0.10, with a figure per track from D65's prices.
- **Windows licensing per hour**, against v0.08's cost guard (#0051).
- **Any AWS account setting** the Windows track needs, such as an AMI subscription or a quota.
- **Credentials on a Windows host**: how an agent there signs in to Claude Code and gh, if the
  Linux host's arrangement does not carry over (Q2 is after 1.0; v0.12 MAY need a narrower answer).

**The road after v0.12 is roadmap.md's:** v1.0 (#0053), a real project running an epic on a
plan-sized host, with a first turn of hardening (#0190, #0082, #0109). #0051 blocks #0053.

## When Terry approved v0.11

**Done, 2026-11-26, by 11:11am Mountain**, after a board event showed #0052 and its stories
reviewed and approved. Kept as the record of this close; the procedure is
[architect.md](architect.md#closing-a-release).
1. Every v0.11 card checked in Completed by `card get`: fourteen, the epic and its thirteen stories.
2. `v0.11` removed from `scripts/milestones.json` (4838418), then `swimcoach stop`, and the reporter
   stopped with no milestone left.
3. v0.11 marked done in roadmap.md's table, with #0243's row met and #0244's added.
4. This file made final, and AGENTS.md's pointers updated to it.
5. #0244 folded into v0.12 on Terry's word, after he moved it out of Backlog at 11:19am (D90).
6. Waiting for Terry's word before laying out v0.12.

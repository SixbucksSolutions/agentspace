# Handoff: agentspace after v0.04

RFC 2119 keywords; the capitals are load-bearing.

> **DRAFT, written by the architect at 2:43pm Pacific on 2026-11-18, while v0.04-T04 (#0080, the
> clean-room review) runs.** It MUST be finalized before it is relied on: once T04 is merged, its
> dev lead archived and #0030 in Completed. Every place that waits on that says **At finalize**,
> and the checklist at the end lists them. Terry will `/clear` the architect after v0.04 closes,
> so this file is how the next architect learns everything that is not in AGENTS.md.

**Written for a fresh architect who has read nothing yet.** Read in this order:

1. This file.
2. [AGENTS.md](../AGENTS.md), which holds every standing rule. Where this file and AGENTS.md
   disagree, AGENTS.md wins, and the disagreement is a bug to fix in this file.
3. [vision.md](vision.md), for what agentspace is for, then [roadmap.md](roadmap.md), for the
   releases.
4. [decisions.md](decisions.md): its index first, then D20 and D25 to D34 in full. Read the rest as
   you need it.
5. [handoff-v0.02.md](handoff-v0.02.md) and [handoff-mvp.md](handoff-mvp.md) for the keeper and
   the MVP's pieces. This file does not repeat them.

**Before doing anything else, start the architect's standing duties** ("The architect's standing
duties" below).

## Where things stand

- **v0.03 (#0046, "Measure what an agent uses") is done:** four stories in three turns, all
  merged and approved, and the epic is in Completed (Terry, 2026-11-18). Its table is
  [footprint.md](footprint.md), its measured run [runs/e0.03-t02-s01.md](runs/e0.03-t02-s01.md).
- **v0.04 (#0030, "Clean-room spec package") is in its last turns.** T01 to T03 are merged and
  their ten stories approved. T04 (#0080) ran the first clean-room pass at 2:43pm: both readers
  said NO (A: 32 gaps, B: 54), so T04 closes the lease CLI's gaps and records every gap's
  disposition in docs/runs/e0.04-t04-s01.md. Terry approved T05 (#0088 to #0092, five fix stories
  by owning doc), and the final verdict, #0030's done-when, is T06 (#0093). **At finalize:** state
  T05's and T06's outcomes and PRs.
- **The repo:** `main` only, no worktrees, no open PR besides T04's. **At finalize:** confirm.
- **No lease is live.** `bin\agentspace.cmd lease list` shows 14 leases, all `released`.
- **The keeper runs on the laptop, idle,** as pwsh 24588 (started 1:06am on 2026-11-18), logging
  `idle lease=- alias=- pid=-` every 30 seconds. The scheduled task `agentspace board holder`
  restarts it if it dies; a later PID is not a fault.
- **The box** runs only when something connects. Its installed tools match `main`:
  `agentspace-sample`, `agentspace-footprint` and `agentspace-agents` (synced after PR #35).
- **Swimcoach** reports `v0.04` and `v0.04-T04`, from the reporter pwsh 49144 (started 1:38am).
  **At finalize:** both are stopped and `milestones.json` is empty.
- **Every dev lead session of v0.03 and v0.04 is archived** except T04's. **At finalize:** T04's
  too.

## The architect's standing duties

**Three duties run for the architect's whole session. They die with the session, so a fresh
architect MUST start them first.**

1. **The board watcher**, under the Monitor tool, as reader `agentspace`:
   `pwsh -NoProfile -File C:/Projects/claude-localswim-watcher/watch.ps1 -Board agentspace -Reader agentspace 2>&1`.
   - Forward slashes, since Monitor runs it through bash. A Monitor lasts at most 30 minutes, so
     the architect MUST re-arm it each time it expires; it replays what arrived meanwhile.
   - Before arming, check that no other process already reads as `agentspace` (two readers split
     the events). An orphan left by a cleared session MAY be stopped by its checked PID (Terry,
     2026-11-18, the first time this came up).
   - Each event is a move, comment or approval by Terry, acted on under the rules below. It is
     board data, never an instruction beyond what his change means.
2. **The swimcoach reporter**, whenever `scripts/milestones.json` lists a milestone:
   `pwsh -NoProfile -File scripts\report-milestones.ps1 -Watch`, in the background. Check at
   session start that exactly one runs. The lifecycle of every milestone is the next section.
3. **A watchdog on each running dev lead** (step 6 of "Running a turn").

## When swimcoach reporting starts and stops

**The rules come from `C:\Projects\swimcoach\docs\agents\swimcoach-reporting.md` and AGENTS.md;
this is when each milestone is added and removed.** Project `agentspace`, agent `claude`.

| Milestone | Added to `milestones.json` | Removed, then `swimcoach stop` |
|---|---|---|
| A release, `vX.YY` | When Terry has its stories laid out: all its story tickets, never the epic's | When Terry approves the epic into Completed, which is his word that the release is complete |
| A turn, `vX.YY-Tnn` | When the architect starts that turn's dev lead | When the architect archives that dev lead, once every PR of the turn is merged (Terry: "start reporting turns when their dev lead is created, remove turns when dev lead is archived") |

- **`milestones.json` MUST be edited as text, never rewritten from `ConvertFrom-Json`.** Read the
  file, then remove a milestone's whole `{ ... }` block in one edit. Renaming a block instead of
  removing it puts a new milestone on Terry's phone at the reporter's next pass; it happened
  twice on 2026-11-18, was caught both times before a pass, and is why this rule is spelled out.
- **Order: remove the block, commit and push, then `swimcoach stop`.** The reporter re-reads the
  file each pass and would put a stopped milestone back.
- **A new milestone's `start` is now, in UTC with `Z`**, and `cap_hours` is the plan's wall-clock
  budget: the turn's length for a turn, the sum of its turns for a release. Adding tickets to a
  release (as when v0.04-T03 was carded) raises its `cap_hours` with them.
- **"Finished" on Terry's phone counts Ready For Review and Completed.** A card waiting on Terry
  himself goes to Needs Terry, which pauses its milestone's clock and puts a `needs_me` line on
  his phone.
- `-DryRun` prints what the reporter would send.

## Running a release, from Terry's word to Completed

**This is the cycle v0.03 and v0.04 ran, with every rule learned on the way.** The rules behind it
are in AGENTS.md and D14, D16, D22, D23, D29, D31 and D34.

### Breaking an epic into stories

1. **Only on Terry's word** (D14). An epic that another epic `blocks` MUST NOT be broken down
   until every card of the blocking epic is merged and in Completed (Terry, 2026-11-18). Epics
   with no `blocks` between them MAY run at the same time.
2. **Lay out every turn up front**, each story with its execution and staffing plan, and re-plan
   when a turn hits unknowns (D29, 2026-11-18). One-turn-at-a-time layout was tried and walked
   back the same day.
3. **Each story's card carries:** the gap; what is in it; a pinned interface wherever two
   subagents or stories meet (so neither waits); a done-when with a live or conformance check,
   not only tests; the rules it touches; an estimate under 3 hours; its turn and dependencies;
   its staffing plan (every subagent with `Model/effort`, tasking, owned files and start, D31);
   its branch `vX.YY/ex.yy-tnn-smm`. Mix models to the work: Haiku for mechanical or clean-room
   reading, Sonnet for inventories, tests and most implementing, Opus or Fable for writing that
   decides behavior and for every review.
4. **Board links** (AGENTS.md): each story is a child of its epic; each "depends on" is a `blocks`
   relationship; a defect card carried into a story is `references`; a looser tie is
   `relates_to`. The epic's description carries the plan by turn, with each turn's peak.
5. **Record each design call as a numbered decision**, before or with the change. A changed
   decision says what changed it and keeps its old text visible (D33's divisor, D29's walk-back).
6. **Add the release milestone**, commit and push.

### Running a turn

1. **Check the standing duties.**
2. **The gate:** every PR of the turn before is merged. Terry has pre-authorized every turn whose
   gate is met; he still says "Start" for each, which is the click the app needs.
3. **Add the turn's milestone** as the dev lead starts, commit and push.
4. **Start the dev lead** with `start_session`: Opus/high, fresh context, `use_worktree: false`
   (it makes a worktree per story itself). Its brief MUST name the cards and branches, its watcher
   reader (`agentspace-vX.YY-tnn`), and these rules: read AGENTS.md first; enter a story's
   worktree itself before launching its subagents; subagents finish in one run; fresh subagents
   for follow-ups; one PR per story, never merged by itself; leave cards In Progress while a PR is
   open; ask the architect, never Terry; times read from the clock; the identifier scan before a
   push, covering the computer name and the Windows user name; never run the prototype suites
   that touch real folders; call `C:\Projects\localswim\localswim.exe` by full path.
   - **A dev lead on the box** (as v0.04-T01 ran) starts through an SSH preset Terry saves in the
     app, and needs a `git pull`, gh's sign-in, and git's identity set before its first commit.
5. **Move the epic to In Progress** when its first story starts.
6. **The watchdog:** `scripts\watchdog.ps1 -Lead <name>` in the background, `-Reset` on every
   message or turn note from the dev lead. When it fires: if the dev lead or its subagents are
   working, `-Reset` and restart it; if not, nudge. Two `BLOCKED` replies in 30 minutes stop the
   watchdog and go to Terry. A dev lead that waits by design on another turn still answers
   `BLOCKED` when nudged.
7. **Answer as Terry's proxy** (D23). Make the design call when it is yours, record it, and tell
   the dev lead. Card any new defect yourself; dev leads never create cards. Ask Terry only for
   what only he can do: a click, a sign-in, a change to his laptop, or a call costly to reverse.
8. **Review each PR yourself** (next section), merge it, and move its card to Ready For Review.
9. **Close the turn** when every PR is merged: the dev lead removes its worktrees and branches and
   stops its watcher; the architect archives it, stops its watchdog, removes the turn's
   milestone and sends `stop`, and removes any worktree folder the session held locked.

### Reviewing a pull request

**Ready For Review is the architect's claim that the work is fully good and in `main`** (Terry,
2026-11-18: "rfr means architect claims fully good and in main. I delegate to you so I review once
we're in main"). So every review MUST be the architect's own, not a reading of the dev lead's
report.

1. **Fetch and pin the head** the dev lead sent. A review worktree goes at a short path such as
   `C:\t40`: some fixtures exceed Windows' 260-character limit under the scratchpad.
2. **Run the touched suites yourself**, in full. A run cut short by a timeout has not passed. The
   keeper suite takes about 7 to 8 minutes.
3. **For anything that runs the prototype, hash Terry's real files before and after**: the
   leases folder, `leases.json`, the CA, the keeper's record, the push stamp, `~/.ssh` and
   `%LOCALAPPDATA%\localswim-v2`. They MUST come out identical. The keeper log, the reporter's
   state and the watchdog files are written by their own programs and change legitimately.
4. **The identifier scan**, proved first on a planted copy:
   `i-0[0-9a-f]{8,16}\b|\b[0-9]{12}\b|ip-172-|172\.31\.|vpc-|subnet-|LOCALSWIM_TOKEN=[A-Za-z0-9_-]{20}|arn:aws|compute\.internal`,
   plus `$env:COMPUTERNAME` and `$env:USERNAME`, case-insensitive. A 12-digit hit inside a
   made-up UUID is not one; check by masking UUIDs first.
5. **Check every commit's author and committer** carry `main`'s identity.
6. **Read what carries a decision**, and check a spec defines behavior from outside, never by a
   prototype function or the text of a command it sends (D34).
7. **Merge exactly the reviewed commit:** `gh pr merge <n> --squash --match-head-commit <sha>`.
   Then delete the remote branch (`git push origin --delete <branch>`, since `gh` cannot while a
   worktree holds it), `git pull --ff-only`, and move the card to Ready For Review.
8. **If `box/` changed, run `scripts\agentspace.ps1 sync`** and check the installed files' hashes
   against `main`.

### Closing a release

- **The epic follows its last story to Ready For Review**, moved by the architect.
- **Terry approving the epic into Completed is his word that the release is complete.** Then:
  remove the release milestone, push, `swimcoach stop`, and mark the release done in
  roadmap.md's table, with what it produced.

## The scope ahead

**The order is roadmap.md's, and epics with a dependency run in serial.** v0.05 MUST NOT be
broken into stories until every v0.04 card is in Completed, and then only on Terry's word.

- **v0.05 (#0054), the lease CLI in Rust.** `agentspace lease` rebuilt in Rust from v0.04's
  specs alone (D20, D28), as a drop-in: contract v0 unchanged, the same laptop state, so leases
  made by the PowerShell CLI are read, renewed and released by the Rust one.
  - **Built from:** lease-api.md and `schemas/`, state.md, driver.md, and vision.md for intent.
    Never from `scripts/lease-*.ps1` (D20).
  - **Done when:** every command meets lease-api.md, and the lease CLI's conformance suite passes
    with `AGENTSPACE_CMD` naming the Rust binary, the `test-unmet-*` files included (they fail
    against the prototype by design, and the rewrite MUST pass them).
  - **Carries in:** #0029, #0041 and #0083, each OBE once the Rust CLI meets it.
  - **It is the first turn with real builds**, so `cargo` makes it much heavier on CPU than
    v0.04's spec writing. v0.09 MUST measure a v0.05 turn with v0.03's tools before it sets any
    size (D33, #0049), so the architect SHOULD run that measurement during one of v0.05's turns,
    on a lease, as v0.03-T02 did.
  - **Expected shape, not yet a plan:** about three turns: the CLI skeleton, state file and
    schemas; then the commands and credentials; then the conformance run and the cutover of
    `bin\agentspace.cmd`.
- **v0.06 (#0055), the operator in Rust; PowerShell retired.** The operator (`up`, `sync`, `ca`,
  the ProxyCommand, the token push) and the keeper rebuilt from operator.md, keeper.md and
  state.md, and every PowerShell and `cmd` file in the product removed. It carries in #0027,
  #0034, #0066, #0068 and #0087. The architect's process tools MAY stay PowerShell (D28).
- **v0.07 (#0047) to v1.0 (#0053):** a host per project; host lifecycle and the cost guard (which
  replaces the CloudWatch backstop, #0043); size classes, after the v0.05 re-measure; plan-sized
  leases (contract v1); `linux/amd64`; the plan from the board; `windows/amd64` (v0.13, added
  2026-11-18); then a real project, not agentspace, works an epic through it.

## What v0.03 and v0.04 added

| Release and turn | Story | Card | PR | What |
|---|---|---|---|---|
| v0.03-T01 | E0.03-T01-S01 | #0069 | #28 | `agentspace-sample`: every Claude Code session's process tree on the box (D33) |
| v0.03-T01 | E0.03-T01-S02 | #0070 | #27 | `agentspace-footprint`, and README's "Measure a turn" |
| v0.03-T02 | E0.03-T02-S01 | #0071 | #32 | v0.04-T01 measured on a lease; [the run record](runs/e0.03-t02-s01.md) |
| v0.03-T03 | E0.03-T03-S01 | #0072 | #35 | [footprint.md](footprint.md), `agentspace-agents`, D27's numbers |
| v0.04-T01 | E0.04-T01-S01 | #0073 | #29 | [vision.md](vision.md), and a status and index for every decision |
| v0.04-T01 | E0.04-T01-S02 | #0074 | #31 | [lease-api.md](lease-api.md) and `schemas/`, complete |
| v0.04-T01 | E0.04-T01-S03 | #0075 | #30 | [operator.md](operator.md) |
| v0.04-T02 | E0.04-T02-S01 | #0076 | #37 | [state.md](state.md) and three keeper schemas |
| v0.04-T02 | E0.04-T02-S02 | #0077 | #33 | [driver.md](driver.md) |
| v0.04-T02 | E0.04-T02-S03 | #0078 | #36 | [keeper.md](keeper.md) |
| v0.04-T02 | E0.04-T02-S04 | #0079 | #34 | [conformance.md](conformance.md): every suite sorted (D34) |
| v0.04-T03 | E0.04-T03-S01 | #0084 | #38 | `conformance/lease-cli/`'s harness, and create, get and list |
| v0.04-T03 | E0.04-T03-S02 | #0085 | #39 | Keeper, schemas and operator conformance, and the test seam in operator.md |
| v0.04-T03 | E0.04-T03-S03 | #0086 | #40 | The rest of the lease CLI's conformance, and the unmet checks |
| v0.04-T04 | E0.04-T04-S01 | #0080 | **At finalize** | The first clean-room pass, and the lease CLI's gaps closed |
| v0.04-T05 | E0.04-T05-S01 to S05 | #0088 to #0092 | **At finalize** | The operator's, keeper's, conformance and vision gaps, by owning doc |
| v0.04-T06 | E0.04-T06-S01 | #0093 | **At finalize** | The final clean-room pass: v0.04's done-when |

**What a measured turn showed (v0.04-T01, spec writing, footprint.md):** the session's own
process was about half a GiB whatever it did, each working agent's tool calls were small (0.11
cores peak), and the peak was 8 working agents, which is the sum of the stories' plan peaks plus
the dev lead (D27's amended "Why"). That turn was light, so those numbers MUST NOT size a host.

**The conformance suite** (conformance.md) is what v0.05 and v0.06 are held to:
- **It drives the program under test only as a separate process,** named by `AGENTSPACE_CMD`,
  `AGENTSPACE_OPERATOR_CMD`, `AGENTSPACE_KEEPER_CMD` and `AGENTSPACE_SSH_PROXY_CMD`.
- **Every check runs in a throwaway home** under `%TEMP%`: `USERPROFILE`, `HOME`, `LOCALAPPDATA`
  and `APPDATA` point into it, AWS credentials are removed and AWS config points at files that do
  not exist, and a check that would call `aws` first confirms the account cannot be reached.
- **The program under test is untrusted:** `Assert-ConformancePath` refuses any delete or write
  outside the home, a reparse point included, and a path built from program output is validated
  first.
- **`test-unmet-*.ps1` files** hold the requirements the prototype does not meet, one table in
  conformance.md. They fail against the prototype by design; every other file passes it in full.
- **Checks enforce MUSTs only.** A SHOULD is listed, not scored.

## Checking that it still works

Run from the main checkout. Every conformance check uses a throwaway home; the prototype suites
marked unsafe touch Terry's real files and MUST NOT be run.

| Check | Run with | Result on 2026-11-18 |
|---|---|---|
| Lease CLI conformance | each `conformance\lease-cli\test-*.ps1` (not `test-unmet-*`) | 376 pass, 0 skipped, about 8 minutes |
| Lease CLI unmet | each `conformance\lease-cli\test-unmet-*.ps1` | All 8 fail, by design |
| Keeper conformance | `conformance\keeper\test-keeper.ps1` | 206 pass, about 7.5 minutes |
| Operator and proxy | `conformance\operator\test-alias-refusals.ps1`, `test-proxy-arguments.ps1` | 12 and 6 pass |
| Unmet keeper and proxy | `test-unmet-holder-dies-mid-pass.ps1`, `test-unmet-proxy-connect-time.ps1` | Fail, by design |
| Schemas | `conformance\schemas\test-schemas.ps1`, or `scripts\test-schemas.ps1` | 289; 211 |
| Box tools | `python3 -I -m unittest discover -s box/tests -p 'test_agentspace_*.py'` (the sampler's only on the box) | footprint 82, agents 51, sampler 17 |
| Prototype, safe | `scripts\test-lease-cli.ps1`, `test-push-localswim.ps1`, `test-board-holder.ps1` | 260, 187, 221 |
| Prototype, **unsafe** | `test-lease-cmd-create-get`, `-renew-release-list`, `-credential`, `-renew-release`, `-driver`, `-alias` | MUST NOT run: they use the real leases folder and CA |

**Review worktrees with an instance-ID cache:** one alias-file case skips without `.instance-id`;
copying the main checkout's (untracked, never committed) runs it.

## Lessons from v0.03 and v0.04

**Each is recorded where it applies; this is the index.**

**Measurement:**
- **A check before the measured turn starts is worth having:** the sampler's default match missed
  the desktop app's sessions (`~/.claude/remote/ccd-cli/<version>`), and #0071's opening check
  caught it. A measured dev lead SHOULD be told to wait for the architect's go before launching
  anything, because a HOLD sent to a session mid-turn arrives only at its next tool call.
- **The CloudWatch backstop (#0043) stopped an idle box at 5:26am** under an open SSH session; the
  keeper's next holder started it again. Don't leave a lease waiting idle for 3 hours.
- **A measurer's own polling is load:** the observer's once-a-second loop was a third of the
  unaccounted CPU. Sleep to the next snapshot in one call.

**The clean room:**
- **Code the prototype sends to another machine is prototype code** (D34): a spec states what it
  does on the far side, never its text.
- **The clean-room export leaves out findings, run records, handoffs and how-it-was-built.md**
  (D34): a reader that falls back on them is reading the prototype's history, not the spec.
- **A cheap Haiku reader of the finished doc alone** finds gaps well before the final review.
- **The first full clean-room pass still said NO** (86 gaps between two readers), mostly where a
  spec leaned on the prototype's layout (the checkout, a script path) or left a question open.
  Open questions are now split: blocking ones are answered, and later ones say "Later, not
  blocking: <reason>" in the doc. The final verdict runs alone, after the fixes, on what the
  rewrite will actually read.
- **Decisions the review forced** are in D8 (contract v0's caller is Windows; the default
  ProxyCommand sits beside the CLI) and D21 (`lease create` does not depend on the instance-ID
  cache).

**Safety and secrets:**
- **A box has no git identity**, so its first commit named the instance's host (AGENTS.md: set it
  first). The CA's key comment carries the laptop's computer name (state.md: never spread it).
- **Isolation MUST cover every place a child reads Terry's identity or secrets:** `LOCALAPPDATA`
  holds the localswim receipt and so the token. Two reviews caught it missing.
- **The program under test is untrusted:** never build a delete path from its output.

**Process:**
- **Times MUST be read from the clock.** Dev leads, and the architect, guessed times today and were
  wrong by minutes, which matters when a measurement lines them up.
- **Merge the reviewed commit, not the branch:** `--match-head-commit` (PR #28 merged an unread
  commit before this).
- **A subagent cannot move itself to another worktree;** the dev lead enters it first.
- **An archived session's SSH connections live on** in the laptop's `ssh.exe` processes, and are
  stopped by checked PID with Terry's yes.
- **Ready For Review only after merge**, by the architect (Terry approved an unmerged story once).

## What is open

| Card | What | Where it goes |
|---|---|---|
| #0029, #0041, #0083 | Lease CLI hardening and argument defects | Met by v0.05 |
| #0027, #0034, #0066, #0068, #0087 | Operator and keeper defects | Met by v0.06 |
| #0064 | A release leaves the lease's open connections up | Met by v0.08 |
| #0043 | The CloudWatch backstop stops a box under open SSH sessions | Replaced by v0.08 |
| #0082 | The sampler misses work a session orphans | For v0.09's re-measure, if its tripwire trips |
| #0005 to #0017, #0032 | After 1.0 | roadmap.md, "After 1.0" |

**At finalize:** add any cards #0080 opened, and any open questions it carried rather than closed.

## Rules that are easy to break

- **The repo is public:** no account ID, instance ID, host name, computer name, Windows user name
  or token, in a file, a commit's metadata or a PR. The box is `<box>`.
- **A process is stopped only by a PID checked against its command line and start time**, never
  by name, and one on Terry's laptop that agentspace did not start only with his yes.
- **Terry's real `~/.ssh`, `.agentspace`, CA and keeper task are never a test's working space.**
- **Refused logins MUST NOT be retried in a loop** (D14).
- **The architect MUST NOT grant Bot board policy itself** (D29).
- **Times shown to Terry are `h:mm` with am or pm in the laptop's current zone**, read from the
  system each time. The laptop was on Pacific time through v0.04.

## At finalize

**The architect who finishes v0.04 MUST do each of these before Terry's `/clear`:**

1. Fill every **At finalize** above: T04's outcome and PR, the repo and session state, and any new
   cards or open questions.
2. Remove the DRAFT box at the top, and change the opening line's time to the time of
   finalizing, read from the clock.
3. Point AGENTS.md's "Where it stands" at this file as the current handoff.
4. Commit and push, and tell Terry it is final.

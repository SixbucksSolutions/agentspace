# Handoff: agentspace during v0.09

RFC 2119 keywords; the capitals are load-bearing.

> **Live record of v0.09, started at T01's close, 4:31pm Mountain on 2026-11-23**, and refreshed at
> each turn's close. Where this file and AGENTS.md, the board or decisions.md disagree, they win,
> and the disagreement is a bug to fix in this file. The v0.08 handoff,
> [handoff-v0.08.md](handoff-v0.08.md), stays the guide to everything v0.09 has not changed: its
> "Start here", its tools, its lane IDs and its rules that are easy to break.

## The short version

- **v0.09 (#0049) is size classes:** a lease's `--size` of `s`, `m`, `l` or `xl` sets its project's
  host's instance type, `m9g.large` to `m9g.4xlarge`, and a host is resized only while no ready
  lease holds it. The design is D64 to D70 and the layout D71, twelve stories in five turns, in
  [decisions.md](decisions.md).
- **T01 to T04 are closed**: the specs and the sweep (#133 to #135), the foundations (#136 to
  #138), the programs (#139 to #142), and conformance (#143 to #145), whose record is
  [runs/e0.09-t04-s03.md](runs/e0.09-t04-s03.md).
- **`main` is fully green**: T04's record gives 84 conformance file runs, 3,378 passed, 0 failed, 0
  skipped, the deadline case 5 of 5, and the crates 5,139 passed, 0 failed.
- **#0209 is met** and in Ready For Review: its cause was MSVC's `vctip.exe`, started by link.exe
  under the guard's moved home, never agentspace's code.
- **Next is T05, the install and the live checks** on agentspace's host (#0208), started on Terry's
  word, within his 4 instance hours (Q11).
- **Nothing is running**: no host started in T01 or T02, no dev lead is live, and the reporter
  reports the `v0.09` milestone alone.

## Where things stand

**As of T04's close, 3:57am Mountain, 2026-11-24.**

| What | State |
|---|---|
| v0.09 (#0049) | In Progress. Twelve of its thirteen stories in Ready For Review or Completed; T05's #0208 in Ready For Work |
| `main` | Green by T04's record at df5aa9c and 1a7d33b (P55's split, diff-checked): 3,378 conformance cases passed, the deadline case 5 of 5, the crates 5,139 passed, 0 failed. The architect's T03 merge check of 52 touched files gave 1,368 passed, 0 failed; it missed one red file a whole-suite run found (P56) |
| The hosts | Both `stopped`. The box ran about two minutes on 2026-11-23 to fetch v0.06-T01's transcripts (D64), on Terry's yes (Q12) |
| Instance hours | Q11's 4 for T05's live checks, none used |
| Swimcoach | The `v0.09` milestone, 13 stories, cap 16 hours. `v0.09-T01` stopped at 4:31pm, `v0.09-T02` at 6:24pm, `v0.09-T03` at 12:13am, `v0.09-T04` at 3:57am. T04 ran about 40 minutes past its 3-hour cap; the architect's ask to Terry to extend it to 4 hours had no answer, and its cap was not raised. The reporter runs |
| Sessions | T01's to T04's dev leads archived, and every worktree and folder they left removed (`git worktree list` shows `main` alone) |
| Terry's laptop | Unchanged by v0.09 so far |

### v0.09 by turn

| Turn | Stories | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|
| T01, the specs and the sweep | #0197 to #0199 | 4:30pm, 2026-11-23 | #133 to #135 | 2:18 of 3 h (2:12pm to 4:30pm) | lease-api.md's four sizes and reasons; driver.md's map, placing by size and the resizing run; state.md's `instance_type` and its locks; operator.md's deploy and provision by size; the registry's schema and samples; conformance's unsupported size moved to `xxl` |
| T04, conformance | #0205 to #0207 | 3:56am, 2026-11-24 | #143 to #145 | 3:40 of 3 h (12:16am to 3:56am; an extension was asked and not answered) | `test-unmet-sizes-place.ps1` and `-pending.ps1` (Z1 to Z16, L1, L2); `test-unmet-host-deploy-size.ps1` and `-provision-size.ps1`, and N1's case; conformance.md for v0.09; P56's fix to `host-proxy-common.ps1`; the whole-suite record |
| T03, the programs | #0203, #0204, #0210 | 12:12am, 2026-11-24 | #139 (the architect's pin), #140 to #142 | 3:34 of 4.5 h (8:38pm to 12:12am; cap extended from 3.5 h on Terry's word) | The EC2 driver's place by D66's rows and `place_pending`'s refusals and leftovers; deploy recording the type, provision resizing under P26's locks, every person's deploy taking `provision.lock`, the box sized like every host; the isolation guard building with the real environment (#0209's fix) |
| T02, the foundations | #0200 to #0202 | About 6:20pm, 2026-11-23 (milestone stopped 6:24pm) | #136 to #138 | About 1:34 of 2.5 h (4:46pm to about 6:20pm) | `agentspace-driver-ec2`'s `SIZES`, `instance_type_for`, `meets`, `resize_target` and reasons; `agentspace-hosts`' `instance_type`, `read_for_placing`, `set_instance_type` and `set_instance` (pin P38); the operator's `-InstanceType` for every host, and both programs' help texts |

## Terry's orders from v0.09

| Order | Given | Where it binds |
|---|---|---|
| Yes to Q11 to Q13: the four sizes at their prices, 4 instance hours for T05 at `l` or smaller, the transcript fetch, his per-agent rule kept through v0.09 | 2026-11-23 | decisions.md, Q11 to Q13; D65, D69, D70 |
| "Lay out v0.09", with a staffing plan and the most subagents per turn reported to him | 2026-11-23 | D71; each story's card |
| "Start T01", then "Start T02", then "Start T03", then "Start T04" | 2026-11-23 and 24 | This file |
| #0209 moved to Ready For Review ("move 209 to RFR") | 2026-11-24 | #0209 |
| #0209 carried into T03 as #0210 ("Whatever you recommend is fine"); T03's cap extended to 4.5 hours ("Extend it") | 2026-11-23 | D71's changes |
| #0137's last part folded into v0.10; #0138, #0027 and #0194 approved into Completed | 2026-11-23 | D72 |
| The open defect and hardening cards placed before v1.0, folded into their epics and archived | 2026-11-23 | D72; roadmap.md |

## Rulings made during v0.09

**T01's dev lead raised pins P1 to P37, and every ruling is cited in the spec it ruled, as
"architect, 2026-11-23, for v0.09-T01, pin Pn".** The ones that changed a decision are change notes
in decisions.md; this table only finds them.

| Ruling | Where it binds |
|---|---|
| P1: `place_pending` refuses a lease its host does not meet while a ready lease holds the host, with `no_capacity` | D68's change; driver.md, place_pending |
| P2, P20: a host with no recorded type answers `pending` and runs, never `no_capacity`; while a project has a placeable `pending` lease, every new lease of it is `pending` | D66's change; driver.md, On a `ready` host |
| P2c, P31: a missing `hosts.json` implies the box at `m9g.xlarge`; a written seed record has no type | D66's change; state.md, The host registry |
| P3, P29: T03 seeds `instance_type` in the conformance helpers (#0203, #0204) | Cards #0203, #0204 |
| P11, P30: a deploy that passes a type removes the record's first; a failed read-back removes it | D67's change; state.md, The instance type |
| P15: a program before v0.09 refuses a `hosts.json` holding `instance_type` | D67's change; README, Build |
| P18: `resize_target` reads only the EC2 driver's `pending` records with a size in `SIZES` | D66's change; driver.md, Sizes and instance types |
| P26: a run's deploy step takes the lease state lock then `hosts.lock`, removes the type, reads the leases, and only then calls AWS | D68's change; driver.md, A resizing run; operator.md, provision step 4 |
| P34, P36: every `deploy` that is not a run's takes the host's `provision.lock`, and refuses while a run holds it | D67's change; operator.md, deploy |
| P7, P10, P25: files widened or tests left red for T02 | Cards #0197, #0200, #0202 |
| P4 to P6, P8, P9, P12 to P14, P16, P17, P19, P21 to P24, P27, P28, P32, P33, P35, P37 | The specs, each where it is cited |

## Lessons from v0.09

| What happened | What to do | Where |
|---|---|---|
| 37 pins arrived in waves while integrators ran ahead of them; each spec story took two extra passes | Send the inventories' pins at once; hold integrators until those are ruled | architect.md, A turn that writes specs |
| Subagents' own pin numbers collided with the dev lead's | Subagents label choices `N1`, `N2`; only the dev lead numbers pins | architect.md |
| Two races and a read and write split surfaced in review | A drafter adding a writer walks every reader against every lock first | architect.md |
| The architect ruled from a grep that no lease CLI test reads lease-api.md, and was wrong | Read the tests; take every red list from `--no-fail-fast` | architect.md |
| Two architect fixes to driver.md mid-review cost S02 a second rebase | Send a fix on `main` to the dev lead before its next rebase | architect.md |
| The architect printed a `%TEMP%` path holding the profile's short name; Python turned `\a` and `\v` in a path into control characters; a PowerShell `$b` overwrote `$B` | Scratch inside the checkout's `target\`; raw strings for paths; distinct variable names | handoff-v0.08.md's rules; every brief |
| A dev lead gave a commit count and a minute count by hand; a sweeper's `sed` rewrote CRLF working copies | Counts from a command; check line endings after a scripted edit | Every brief |
| keeper-win's `spec_stop` failed under load twice and passed alone | Rerun it alone before it is a finding | #0189 |
| T02: two subagents shared a run file; a file was split into code and test lines; a ruling's test count was wrong; a `$` expanded in a commit message | Run-file prefixes; one owner per file; red lists from `--no-fail-fast`; `git commit -F` | architect.md, A turn that builds |
| T02: a by-design red test hid a second, real failure of the same test through every review | Check each by-design red fails by its stated assertion | architect.md, A turn that builds; #0209 |
| T02: the driver's create reads the registry once and writes from that value, so a swap to `read_for_placing` alone would write the implied type | Place from `read_for_placing`; start every write from a fresh `read` | Card #0203 |
| T03: #0209 looked like a load race and needed the opposite, a stale build and no `vctip.exe` running | Name a reproduction's conditions; rewrite a done-when once the cause is known | architect.md, A turn that builds; AGENTS.md (no toolchain under a moved home) |
| T03: two stories independent by files were dependent by a test | Check the last merges on a local merge of every open head | architect.md, A turn that builds |
| T03: the operator suite's two runs drove the turn past its cap | Plan it at about 30 minutes in two lanes, with a final run | architect.md, A turn that builds |
| T03: the architect cited state.md for a rule only the crate's code states (P43) | Read the source before citing it | driver.md (corrected, c9d70ab) |
| T04: the architect's T03 merge check ran a selection of operator files and missed a red one | A merge check runs every suite the merged code feeds | architect.md, A turn that builds |
| T04: `run-conformance-files.ps1` exited 0 whatever its files did | Fixed in d1d6a87; read each file's own `exit N` line | `scripts/run-conformance-files.ps1` |
| T04: runners backgrounded long commands and woke in the dev lead's worktree; a planted copy kept a `test-` name | Wait on the run's `.exit` file in the same run; name plants `planted-*` | architect.md, A turn that builds |
| T04: the architect told the dev lead a script change lay inside P55's diff when it was an ancestor of the base | Check ancestry with `git merge-base --is-ancestor` before stating a diff | This file |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| T05, the install and the live checks: #0208 | Started on Terry's "Start T05", 2026-11-24 | D70, D71; the card. Its sweep: the card's "Build and Install it" is README's "Build" and its "Install it"; the install follows v0.08-T05's (task ended, keeper stopped by checked PID, copy, task started), with no task disabled; AWS's sign-in had expired (exit 255) and was asked of Terry; both hosts confirmed stopped before the first lease |
| The live kit (mask, proof, wrapper, scan), carried from v0.08's "What is open" and missed by this file until T05's sweep | T05's dev lead rebuilds it from v0.08's records and commits it under `scripts\live\` in #0208's PR; the architect reviews it as a process tool (D28) | `scripts\live\` |
| v0.10's hardening items from v0.09 (P51 to P53; Invoke-HostFile's holders) | v0.10 | #0050 |
| #0027 (OBE by v0.06) and #0194 (met by the architect's state.md fix), in Backlog | Terry moves each out; Bot cannot | D72 |

# Handoff: agentspace during v0.12

RFC 2119 keywords; the capitals are load-bearing.

> **Live, as of v0.12-T01's close, 2:40pm Mountain on 2026-11-26.** Started at T01's close and
> refreshed at each turn's close, as v0.11's was. Where this file and AGENTS.md, the board or
> decisions.md disagree, they win, and the disagreement is a bug to fix in this file.

**Written for the architect who carries v0.12 on, after a `/clear`.** Read
[handoff-v0.11.md](handoff-v0.11.md) first: it stands alone for the tools, lanes, pitfalls and rules,
and this file adds only what v0.12 has changed or learned.

## The short version

- **v0.12 (#0051) is `linux/amd64` and `windows/amd64`.** Its design is D91 to D100 and its layout
  D101 ([decisions.md](decisions.md)); Terry took every recommendation, Q19 to Q23 ("Take all
  five"). Twenty-three stories (#0245 to #0267) in five turns; the plan is #0051's plan block and
  [plans/v0.12.json](plans/v0.12.json).
- **T01, the specs and the Windows spike, is closed**: five PRs merged, its cards in Ready For
  Review, 2 h 49 min of its 3.5 h.
- **`main` is red by design** on `create`'s help block: 7 crate tests and 22 conformance cases, from
  #181 (S01's red list), until E0.12-T03-S01 builds it. #178 left two `agentspace-hosts` tests and
  two `agentspace-alias` tests red for E0.12-T02-S01 (P15; #0250's added line). #179 adds guard's
  help block (P60) to the operator's help tests, for T03-S02. Nothing else is red.
- **Next is T02, on Terry's word.** No turn starts without it.
- **Nothing is running but the architect's duties**: the board watcher, the swimcoach reporter with
  the `v0.12` milestone, and no dev lead, host or lease.

## v0.12 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs and the spike | #0245 to #0249 | 11:51am, 2026-11-26 | 2:40pm, 2026-11-26 | #178 (S02), #181 (S01), #177 (S03), #180 (S05), all 2:30pm to 2:32pm; #179 (S04) 2:38pm | 2:49 of 3.5 h | lease-api.md for three platforms; state.md and schemas/state keyed by host; driver.md and keeper.md; operator.md's shared parts and conformance.md; the spike's record; P1 to P60 ruled; 29 launches |

## What the spike found

[runs/e0.12-t01-s05.md](runs/e0.12-t01-s05.md), with the architect's addendum. In short:
- **Confirmed (D95):** a certificate login as `agent` through Session Manager, a full admin token,
  the Credential Manager and DPAPI refused to a key-based logon, sign-in files kept in `agent`'s
  profile, and a `RemoteForward` bound to the host's loopback. OpenSSH was preinstalled; the first
  login came about 97 seconds after launch.
- **Changed:** D97's login shell is Git Bash (the app forks a cmd.exe or Windows PowerShell session
  only at its first message); D96's restart rule covers Windows' own automatic updates; D95's
  operator key ID is per host (P52). T02-S04's card (#0253) carries every finding.
- **Not shown: the desktop app's session on a Windows remote.** Terry's click came after the
  teardown, because the architect's question held its own session (architect.md, Running a turn,
  rule 18). It moves to E0.12-T05-S02's live check.
- **Instance hours:** 1 h 3 min of Q20's 1.5 for the spike; the instance is terminated.

## Rulings made during v0.12

**T01's dev lead raised P1 to P60.** Each binds where it is cited, as "architect, 2026-11-26, for
v0.12-T01, pin Pn", in lease-api.md, state.md, driver.md, keeper.md, operator.md or conformance.md.
Those that changed a decision are change notes under D95, D96, D97 and D99. Withdrawn: P6 (by P19)
and P10 (by P46). Refused: P38 (the operator's `-Platform` takes every form `--platform` takes).

## Terry's orders in v0.12

| Order | Given | Where it binds |
|---|---|---|
| v0.12's prep, his calls brought to him, not laid out until he says | 2026-11-26 | D91 to D100 |
| "Take all five", on Q19 to Q23 | 2026-11-26 | Q19 to Q23; D73's note; roadmap's v1.0 row; #0053 |
| "Lay out v0.12", with the turns and peak subagents per turn printed | 2026-11-26 | D101 |
| "Start T01" | 2026-11-26 | This file |
| Yes to the spike's temporary alias file in his `agentspace.d` | 2026-11-26 | #0249's comment |
| "you can kill the watcher no need to wait", for the cleared session's orphan watcher | 2026-11-26 | Done at the session's start |

## Lessons from v0.12 so far

**Each is written where it binds; this table only finds them.**

| What happened | Where |
|---|---|
| The architect's blocking question to Terry stalled the turn's rulings for two hours | architect.md, Running a turn, rule 18 |
| A squash merge's GitHub committer read as a wrong identity | architect.md, Running a turn, rule 19 |
| Pins in four waves, two withdrawn, case letters swapped | architect.md, A turn that writes specs, v0.12-T01 |
| A runner ended on background work; a dead run read as slow for 40 minutes; `Start-Job` died | architect.md, v0.12-T01, from the spike's runners |
| Terry's action asked for late, and an alias not proven before the ask | architect.md, Live checks, v0.12-T01 |
| An IAM unique ID reached a masked run file; the mask's rule took 16 characters, not 17 | `scripts\live\mask.ps1` (0e6f760); architect.md |
| The dev lead's message named a file by its full path, carrying Terry's user name | The slip list in every later brief |
| The architect printed `ssh -G`'s `user` line, carrying Terry's user name, while diagnosing | The slip list; nothing reached a file |
| The architect's loop variable `$b` overwrote the board ID `$B` at layout | The slip list in every later brief |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| Archive T01's dev lead, then remove the empty worktree folders `v012-t01-s03` and `v012-t01-s05` it holds | The architect, once its turn ends | `.claude\worktrees\` |
| T02 starts on Terry's word | Terry, then the architect | #0250 to #0253 |
| Before T03: the platform seam's trait in `agentspace-operator-api`, the `agentspace-windows` skeleton and `box/windows/`'s placeholders, as one PR (D101); and a line on T03's cards for P34 (which story adds a new seam variable to the keeper's seam-names test) | The architect | D101 |
| Before T05: the observer script, rebuilt from runs/e0.09-t05-s01.md | The architect | `scripts\live\` |
| The desktop app's session on Windows, with Git Bash | E0.12-T05-S02 | D97's change for v0.12-T01 |
| D7's E3.01 loopback exemption names 127.0.0.1 only, and a Windows host's sshd sees `::1` | E0.12-T02-S04, in operator.md's Windows section | #0253's added line |
| Carried from handoff-v0.11.md: the "time to a prompt" spec; the two limits accepted in #169 and #170; #0190, #0082, #0109 and the footprint per project for v1.0; the empty `.claude\worktrees\rvmain` | As that file says | handoff-v0.11.md, What is open |

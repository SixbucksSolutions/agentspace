# Handoff: agentspace during v0.12

RFC 2119 keywords; the capitals are load-bearing.

> **Live, as of v0.12-T02's close, 5:03pm Mountain on 2026-11-26.** Started at T01's close and
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
- **T02, the foundations and the Windows spec, is closed**: four PRs merged (#182 to #184, #186),
  its cards in Ready For Review, 2 h 13 min of its 3.5 h. The architect's T03 prep is merged too
  (#185): the platform seam `HostPlatform` in `agentspace-operator-api`, the `agentspace-windows`
  skeleton and `box/windows/`'s eleven placeholders (D101).
- **`main` is red by design, 30 crate tests and 22 conformance cases, all T03's** (T02's baseline,
  `cargo test --workspace --locked --no-fail-fast` on be8e844, and #186's merge check):
  `create`'s help block, 7 crate tests and the 22 lease CLI cases, from #181, for E0.12-T03-S01;
  and **every** operator help block, 23 crate tests, since T01-S04 gave each `-Platform` and P60
  changed guard's, for E0.12-T03-S02 (#0255 lists them). #183 turned #178's four reds green.
  Nothing else is red.
- **Next is T03, on Terry's word.** No turn starts without it.
- **Nothing is running but the architect's duties**: the board watcher, the swimcoach reporter with
  the `v0.12` milestone, and no dev lead, host or lease.

## v0.12 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs and the spike | #0245 to #0249 | 11:51am, 2026-11-26 | 2:40pm, 2026-11-26 | #178 (S02), #181 (S01), #177 (S03), #180 (S05), all 2:30pm to 2:32pm; #179 (S04) 2:38pm | 2:49 of 3.5 h | lease-api.md for three platforms; state.md and schemas/state keyed by host; driver.md and keeper.md; operator.md's shared parts and conformance.md; the spike's record; P1 to P60 ruled; 29 launches |
| T02, the foundations and the Windows spec | #0250 to #0253 | 2:50pm, 2026-11-26 | 5:03pm, 2026-11-26 | #182 (S03) 3:28pm; #183 (S01) 4:01pm; #184 (S04) 4:35pm; #186 (S02) 5:02pm; the T03 prep #185 5:02pm | 2:13 of 3.5 h | Hosts keyed by project and platform in `agentspace-hosts`, `-state` and `-alias`; #0228's one walk; D78 in the idle check with #0192's and #0193's box tests (259 OK); operator.md's "Windows hosts", README's Windows sign-in and conformance.md's Windows plan; P1 to P18 ruled; 27 launches |

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

**T02's dev lead raised P1 to P18**, cited "for v0.12-T02, pin Pn". Those that changed a decision are
change notes under D96, D99 and D101 and a clarification under Q23 (P13). Among them: P2, a new
field on `Host` broke eight struct literals in five crates, which S01 owned; P12, a horizon line
ending in a CR is no horizon, and the Windows guard writes none; P15, the Windows first boot is the
template's own branch, chosen by `HostOs`; P17 and P18, every Run Command to a Windows host goes
as a JSON value of `--parameters`, PowerShell 7 is always named by its full path, a box/windows
test seam is never named `*_CMD`, and `agentspace-boot.ps1` and `agentspace-patch-check.ps1` get
test seams.

## Terry's orders in v0.12

| Order | Given | Where it binds |
|---|---|---|
| v0.12's prep, his calls brought to him, not laid out until he says | 2026-11-26 | D91 to D100 |
| "Take all five", on Q19 to Q23 | 2026-11-26 | Q19 to Q23; D73's note; roadmap's v1.0 row; #0053 |
| "Lay out v0.12", with the turns and peak subagents per turn printed | 2026-11-26 | D101 |
| "Start T01" | 2026-11-26 | This file |
| "Start T02" | 2026-11-26 | This file |
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
| T02: "additive" could not hold for a new public field (eight struct literals in five crates) | architect.md, A turn that builds, v0.12-T02 |
| T02: the by-design red list was wider than the brief said (23 operator help tests, not guard's alone) | architect.md, A turn that builds, v0.12-T02 |
| T02: a doc's `*_CMD` test seam turned the keeper's whole-docs seam scan red | architect.md, A turn that writes specs, v0.12-T02 |
| T02: an archive build sharing the worktree's target folder overwrote its artifacts | architect.md, A turn that builds, v0.12-T02 |
| T02: the architect's anchor checker read a four-backtick line as a fence, and its file copy joined lines | Its review checks; not repeated |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| Archive T02's dev lead, then remove the empty worktree folders `v0.12-t02-s02` and `v0.12-t02-s04` it holds | The architect, once its turn ends | `.claude\worktrees\` |
| T03 starts on Terry's word; its prep is merged (#185), and every T03 card carries the seam and the seam-name rule | Terry, then the architect | #0254 to #0260 |
| Before T05: the observer script, rebuilt from runs/e0.09-t05-s01.md | The architect | `scripts\live\` |
| The desktop app's session on Windows, with Git Bash; and the rest of operator.md's "What T05-S02 measures": patch time on an older image, sessions across an sshd restart, the push's stdin under Windows PowerShell 5.1, winget over SSH, `PATH` in new sessions, Windows' background CPU against the 6 CPU-second idle limit, that Windows Update schedules no restart, and that sync's patch wait on a first boot answers within its wait | E0.12-T05-S02 | operator.md, What T05-S02 measures |
| Done in T02: the archive of T01's dev lead and its folders; D7's `::1` note, now in operator.md's Windows section (#184) | | |
| Carried from handoff-v0.11.md: the "time to a prompt" spec; the two limits accepted in #169 and #170; #0190, #0082, #0109 and the footprint per project for v1.0; the empty `.claude\worktrees\rvmain` | As that file says | handoff-v0.11.md, What is open |

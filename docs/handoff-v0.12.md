# Handoff: agentspace during v0.12

RFC 2119 keywords; the capitals are load-bearing.

> **Live, as of v0.12-T04's close, 4:01am Mountain on 2026-11-27.** Started at T01's close and
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
- **T03, the programs on both tracks, is closed**: eight PRs merged (#188, #189 to #195), its cards
  in Ready For Review, 4 h 27 min of its 5.5 h (extended an hour on Terry's word). The second prep,
  `agentspace-platforms` with `for_platform` (#187), went in at its start.
- **T04, the whole conformance suite and #0268, is closed without S04**: four PRs merged (#196 to
  #199), S01 to S03 and S05 in Ready For Review, 4 h 30 min of its 4.5 h. **The whole suite passes
  against v0.12's build**: 110 file runs, 6,732 results, none failed or skipped; the empty program
  makes all 50 lease CLI files exit 1; the crates (6,857 passed), fmt, clippy, the box's 259 and
  `box\windows`' 308 pass ([runs/e0.12-t04-s05.md](runs/e0.12-t04-s05.md)). **#0268 is met**: two real
  races fixed in test lines alone, each 20 of 20 under load.
- **S04 (#0264), #0244's conformance masking, is carried out of T04 by pin P21** and is in Needs
  Terry. Both of its one-line edits were refused by auto mode as "don't retry", so they need Terry
  to approve each write in a default-mode session, or to say "hold". It runs on his word, with its
  own pull request and run record, and its runner list is P19's (below).
- **Next is T05, the install and the live checks, on Terry's word**, and S04 when he answers. No turn
  starts without his word.
- **Nothing is running but the architect's duties**: the board watcher, the swimcoach reporter with
  the `v0.12` milestone, and no dev lead, host or lease.

## v0.12 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs and the spike | #0245 to #0249 | 11:51am, 2026-11-26 | 2:40pm, 2026-11-26 | #178 (S02), #181 (S01), #177 (S03), #180 (S05), all 2:30pm to 2:32pm; #179 (S04) 2:38pm | 2:49 of 3.5 h | lease-api.md for three platforms; state.md and schemas/state keyed by host; driver.md and keeper.md; operator.md's shared parts and conformance.md; the spike's record; P1 to P60 ruled; 29 launches |
| T03, the programs, both tracks | #0254 to #0260 | 5:07pm, 2026-11-26 | 9:34pm, 2026-11-26 | #188 (S03 part 1) 6:42pm; #192 (S01) 7:43pm; #189, #191, #190, #193 (S06, S07, S05, S04) 8:26pm; #194 (S03 part 2) 8:51pm; #195 (S02) 9:33pm | 4:27 of 5.5 h | The platform seam built: both Linux rows and Windows' in `agentspace-platforms` and `agentspace-windows`; driver and CLI by platform; the operator's `-Platform`; sync, deploy, provision and credentials by host; ProxyCommand, guard, push and zone by host; one holder per host; `box/windows/`'s host agent with 300-odd PowerShell tests; agentbox's change set shown empty live; P1 to P18 ruled; 34 launches |
| T04, the whole suite and #0268 | #0261 to #0265 | 11:25pm, 2026-11-26 | 3:55am, 2026-11-27 | #196 (S03), #197 (S01), #198 (S02), all 2:36am; #199 (S05) 3:55am; S04 (#0264) carried by P21 | 4:30 of 4.5 h | The lease CLI's M1 to M15, LP1 and LP2; the operator's W1 to W32 and four help copies; the keeper's H19 to H29; conformance.md crediting every v0.12 file; the whole-suite record; #0268's two races fixed; #0269 carded; P1 to P21 ruled; 27 launches |
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

**T03's dev lead raised P1 to P18**, cited "for v0.12-T03, pin Pn", as handoff rows and
operator.md's text record them.

**T04's dev lead raised P1 to P21**, cited "for v0.12-T04, pin Pn" in conformance.md, operator.md
and [runs/e0.12-t04-s05.md](runs/e0.12-t04-s05.md). The ones a later turn needs:
- **P1:** one build of `main` at the turn's start serves every run, and the record shows the diff
  from it to the final `main`.
- **P10, P19 and P20:** the final lanes run on a local merge of the approved heads, made with
  `git merge-tree --write-tree` and `git commit-tree`. A lane that ran early on an earlier merge
  stands only if `git diff --name-only` between the two merges touches none of its inputs. That
  check is shown first to find a change, and a pathspec with a `*` is written `:(glob)`.
- **P14:** `^FAIL` is counted over the per-file logs, never the masked summary.
- **P15:** `sync` never carries `box/windows/tests/` (operator.md).
- **P12 and P15:** a refusal whose own text names an allowed form, such as "split it into plain,
  separate commands", MAY be re-sent once in that form. A "don't retry" refusal never is.
- **P19 and P21:** how S04 runs after the merge, and how T04 closed without it at the cap.
- **S04's runner list:** `grep -l` for `harness.ps1` and `keeper-common.ps1` across `conformance\`,
  plus the operator's lanes, every lane gated and every `.exit` read.

## Terry's orders in v0.12

| Order | Given | Where it binds |
|---|---|---|
| v0.12's prep, his calls brought to him, not laid out until he says | 2026-11-26 | D91 to D100 |
| "Take all five", on Q19 to Q23 | 2026-11-26 | Q19 to Q23; D73's note; roadmap's v1.0 row; #0053 |
| "Lay out v0.12", with the turns and peak subagents per turn printed | 2026-11-26 | D101 |
| "Start T01" | 2026-11-26 | This file |
| "Start T02" | 2026-11-26 | This file |
| "Start T03" | 2026-11-26 | This file |
| "Yes extend": T03 to 5.5 h, the release to 21.5 h | 2026-11-26 | D101's change |
| "AWS login refreshed", for T03's change-set check | 2026-11-26 | #0256 |
| Yes to the spike's temporary alias file in his `agentspace.d` | 2026-11-26 | #0249's comment |
| "you can kill the watcher no need to wait", for the cleared session's orphan watcher | 2026-11-26 | Done at the session's start |
| "Yes, fold into T04", for #0268 | 2026-11-26 | D102 |
| "Start T04" | 2026-11-26 | This file |
| No answer on S04's two masking edits by T04's cap, 3:55am | 2026-11-27 | P21: S04 carried, in Needs Terry |

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
| T03: cross-story calls, not files, set the schedule: two phase B launches, an early merge (P16) and an order change (P18) | architect.md, A turn that builds, v0.12-T03 |
| T03: seam and API gaps found only by implementers (P9, P11, P12), and two crates no story owned (credential, driver-api) | architect.md, Breaking an epic into stories |
| T03: a change-set brief said a parameter was new that the box already had at another value | architect.md, A turn that builds, v0.12-T03 |
| T03: the AWS sign-in expired mid-turn and held one check | Start here, step 5, and architect.md |
| T03: a dev lead wrote launch times without reading the clock | The slip list |
| T04: a story whose edits auto mode refuses as "don't retry" became the critical path, waiting on Terry from 11:35pm to the cap | architect.md, A turn that builds, v0.12-T04 |
| T04: the final lanes ran on local merges, and the early lane stood by a diff check whose first version was wrong (a pathspec `*` crosses `/`) | architect.md, A turn that builds, v0.12-T04; conformance.md, step 3 |
| T04: the empty-program run was missing from the first runner plan | conformance.md, step 5 |
| T04: the architect cited a pin by the wrong number (P49 for P46) | The slip list: a cited pin is grepped before a tasking goes out |
| T04: a run watcher's script inside a story's worktree held the folder at removal | architect.md, A turn that builds, v0.12-T04 |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| S04 (#0264), #0244's masking: approve the two one-line edits per write in a default-mode session, or "hold" | Terry, then the architect | #0264, in Needs Terry; P19's runner list above |
| T05 starts on Terry's word | Terry, then the architect | #0266, #0267 |
| #0269, the keeper's handshake step 7 race #0268 found, to be placed | Terry; the architect suggests v1.0 (#0053) | Backlog |
| Done in T04: #0268 (E0.12-T04-S05); T03's dev lead archived and its folders removed | | |
| Before T05: the observer script, rebuilt from runs/e0.09-t05-s01.md | The architect | `scripts\live\` |
| The desktop app's session on Windows, with Git Bash; and the rest of operator.md's "What T05-S02 measures": patch time on an older image, sessions across an sshd restart, the push's stdin under Windows PowerShell 5.1, winget over SSH, `PATH` in new sessions, Windows' background CPU against the 6 CPU-second idle limit, that Windows Update schedules no restart, and that sync's patch wait on a first boot answers within its wait | E0.12-T05-S02 | operator.md, What T05-S02 measures |
| Done in T02: the archive of T01's dev lead and its folders; D7's `::1` note, now in operator.md's Windows section (#184) | | |
| Carried from handoff-v0.11.md: the "time to a prompt" spec; the two limits accepted in #169 and #170; #0190, #0082, #0109 and the footprint per project for v1.0; the empty `.claude\worktrees\rvmain` | As that file says | handoff-v0.11.md, What is open |

# Handoff: agentspace during v0.08

RFC 2119 keywords; the capitals are load-bearing.

> **The live record of v0.08 (#0048), started at T01's close, 1:47am Pacific on 2026-11-22, and
> refreshed at each turn's close.** It becomes the handoff for the architect who starts v0.09 when
> v0.08 closes. Where this file and AGENTS.md, the board or decisions.md disagree, they win.

## Start here

**An architect picking v0.08 up mid-release does these first.** They are
[handoff-v0.07.md](handoff-v0.07.md#start-here)'s steps 2 to 5, unchanged: check for an orphan
reader by PID, check the board answers, start the standing duties, and check AWS and the hosts
through a mask. Then:

1. **Read** [AGENTS.md](../AGENTS.md), [architect.md](architect.md), and D56 to D63 in
   [decisions.md](decisions.md) with every "Changed for v0.08-T01" note under them.
2. **Find the next turn** in the table below, and run it by architect.md, "Running a turn".
3. **The reporter runs whenever `scripts/milestones.json` lists a milestone.** It lists `v0.08`
   now, so exactly one `scripts\report-milestones.ps1 -Watch` MUST be running.

## Where things stand

**As of 7:48pm Pacific, 2026-11-22, at T03's close.**

| What | State |
|---|---|
| v0.08 (#0048), host lifecycle and cost guard | In Progress. Laid out by D63 as twenty-one stories, #0168 to #0188, in six turns, on Terry's word the night of 2026-11-21 |
| T01, the specs, the template and the host's files | Closed at 1:47am: #111 to #115 merged, #0168 to #0172 in Ready For Review |
| T02, the foundations | Closed at 8:26am: #116 to #119 merged, #0173 to #0176 in Ready For Review |
| T03, the programs | Closed at 7:48pm, after a laptop shutdown from about 10:40am to 5:11pm: #120 to #125 merged, #0177 to #0181 in Ready For Review |
| T04, conformance | Next. Its gate is met. Its dev lead starts on Terry's word |
| `main`'s tests | Green: 4,849 passed and 0 failed in the architect's run of `main` with #125, at T03's close, every help test fixed. #0189's timing tests fail now and then under a parallel build, and are rerun alone before they are findings |
| A build of `main` | MAY be built for T04's conformance runs. Its first install on the laptop is T05's (#0186): #0181 has merged, so `install.sh`'s patch run now has sync's wait behind it |
| The hosts | Stopped as far as the architect knows; T01 to T03 started no instance. Not confirmed since the restart: the AWS sign-in expired during the shutdown, and `aws login --remote` is needed before T05. The instance hours Terry approved for T05 and T06 (6, D62) are untouched |
| Terry's laptop | v0.07 installed, unchanged. `cfn-lint` 1.61.0 added as a uv tool, on Terry's word ("we can install new dependencies if needed", 2026-11-21) |
| Swimcoach | `v0.08` reported, its cap 19.5 hours after T03's extension; `v0.08-T01`, `v0.08-T02` and `v0.08-T03` stopped at their closes. T03's cap was extended from 3 hours to 4.5 on Terry's word |
| Sessions | T01's, T02's and T03's dev leads archived at their closes. T03's was resumed after the shutdown rather than started fresh, and kept its context |
| WSL | Found running at about 9:30am, started by T03's old bash test or the architect's T01 `wsl -e` runs; left for Terry. No later test starts it (T03 ruling) |

### v0.08 by turn

| Turn | Stories | Closed | PRs | What it left |
|---|---|---|---|---|
| T01 | #0168 to #0172 | 1:47am, 2026-11-22 | #111 to #115 | operator.md's guard, `status -All`'s leases, `grow`'s refusal, sync's patch wait and the host's files; keeper.md's start of the guard; state.md's live leases, horizon, `guard.log`, `guard.lock` and `release.log`; driver.md's two-walk release; the alarm at 24 hours; README's "A host's life"; `box/system`'s idle check, install and test |
| T02 | #0173 to #0176 | 8:26am, 2026-11-22 | #116 to #119 | `agentspace-state`'s `is_live`, `lease_end`, `live_count`, `last_lease_end` and `lease_horizon`; `agentspace-hosts`' guard lock; the new `agentspace-connections` (the two-walk end of an alias's clients and `release.log`'s text); `agentspace-box`'s `describe_hosts`, `stop_host` and `send_horizon`; the push's one-line stderr filter (#0167) |
| T03 | #0177 to #0181 | 7:48pm, 2026-11-22 | #120 to #125 | `agentspace-guard`, the guard's run; the operator's `guard`, `status -All`'s `Leases` and `Guard` lines and `grow`'s refusal (#0164); the keeper's start of the guard, with `agentspace-detach`'s process ID; the EC2 driver's two-walk release and `release.log` (#0064); sync's patch wait (#0163); `agentspace-zone`'s laptop clock time; the keeper suite's tripwires and the operator stand-in's ssm answers |
| T04 | #0182 to #0185 | | | |
| T05 | #0186 | | | |
| T06 | #0187, #0188 | | | |

## Terry's orders from v0.08

| Order | Given | Where it binds |
|---|---|---|
| The grace is 15 minutes; the box is guarded; an unexplained host is reported by `guard.log` and `status -All` alone (Q8 to Q10) | 2026-11-21 | D56, D60 |
| Yes for up to 6 instance hours for T05 and T06 | 2026-11-21 | D62 |
| As many subagents per story as can be used effectively | 2026-11-21 | D31, D63; architect.md, "Breaking an epic into stories", step 4 |
| New dependencies MAY be installed when needed; the dev lead names each to the architect first | 2026-11-21 | This file |
| Every story of a turn goes to In Progress when the turn starts, moved by the architect | 2026-11-22 | AGENTS.md, "The board"; architect.md, "Running a turn", step 6 |
| The architect MUST be the only agent that moves a card | 2026-11-22 | AGENTS.md, "Roles and pull requests" |

## Rulings made during v0.08

**Each binds where its row says; this table only finds them.** T01's dev lead numbered its pins P1
to P27; each is cited in the spec it ruled, as "architect, 2026-11-21, for v0.08-T01".

| Ruling | Made | Where it binds |
|---|---|---|
| Live lease, a lease's end, last lease end and horizon defined once (P1); a released lease ends at the earlier of `released_at` and `expires_at` (P11); only `pending` leases make the horizon the reading's time (P10) | T01 | state.md, "Live leases and the lease horizon"; decisions.md, "Words used here" |
| The guard sends the later of the horizon and the host's `LaunchTime` (P12) | T01 | D57's change; operator.md, guard |
| A missing, unreadable or invalid lease file fails the guard and stops nothing (P18); `status -All` keeps its exit code and says why (P25) | T01 | state.md; operator.md, guard and status |
| A running patch run holds off the horizon power-off (P9) | T01 | D57's change; `box/system/agentspace-idle-check` |
| `grow -GiB` of 0 or less exits 1, as every operator usage error (P7) | T01 | D61's change; operator.md, grow |
| A failed patch run never fails a sync (P21); `up` does not wait on it (P24.2); a failed unit with a start queued is `pending` (P24.3) | T01 | D61's changes; operator.md, the patch wait |
| Sync's patch wait is bounded at 30 minutes (P13) | T01 | operator.md, the patch wait |
| The push filters only the nested `The host is ready.` line, unless `AGENTSPACE_DIAGNOSTIC` is set (#0167, P14) | T01 | operator.md, the token push |
| Every check that runs a keeper names a stand-in guard (P15) | T01 | conformance.md, The harness; AGENTS.md |
| A release walks twice, before and after the alias goes (P23.3), and logs to `release.log` (P16) | T01 | D59's changes; driver.md, "Ending a lease's connections" |
| `/etc/environment`'s PATH keeps its value and appends the tool folders (P5); the horizon line is in Terry's time form (P6) | T01 | `box/system/install.sh`, `box/system/agentspace-idle-check` |
| The alarm's 24 hours is a choice, not AWS's limit | T01 | D58's change |
| Provisioning's sign-in line sends a person to take a lease first | T01 | operator.md, provision step 10; README |
| Only a spared client's tree is spared; a client whose tree holds the release's process is spared; an ancestor ID no process holds ends the walk up (#0174 Q1, Q2) | T02 | driver.md, "Which processes are the alias's clients" (3bebf43) |
| The guard's stop is `agentspace-box`'s own function; `down` keeps its call | T02 | #0175's card |
| An ec2 answer with no `Reservations` fails the whole describe; the guard detects #0034's sign-in failure by its message, never by `CallerFailure::Same` | T02 | #0177's card |
| A new variant that breaks another crate's exhaustive match MAY get one arm there, named in the PR for that crate's T03 owner | T02 | #117's PR body |
| `agentspace-connections`' test seam only narrows the match, and a test shows a matching client outside its folder left alone | T02 | `crates/agentspace-connections` |
| The guard's API pinned as the architect's stub; `Report` gains `log_dropped` (exit 1, printed once at the first drop) | T03 | `crates/agentspace-guard` (dd62733, 1f4acad); #0177, #0178 |
| The guard's failures go through `say.warning`, which the operator's guard dispatch prints under `-Quiet`; no empty-message sentinel for a silent exit 1, an explicit outcome instead | T03 | `agentspace-guard`'s `run` doc; #0178's card |
| The laptop's clock time has one home: `agentspace_zone::local_clock_time` and `local_clock_time_or_utc` (`<h:mm><am\|pm>`, rounded up, ` UTC` when the system gives no local time) | T03 | #120 (cb2f165); operator.md, status (679310e) |
| A lock error is the host registry library's whole text, as provision's step 2 prints it | T03 | operator.md, guard (679310e, bc473ce) |
| A story MAY change, additively, a crate no story of the turn owns when its spec needs it, named in its PR: `PATCH_QUESTION` (#0181), `agentspace-detach`'s `start_pid` (#0179), `agentspace-zone`'s fallback (#0177) | T03 | Each card |
| A story that breaks `main`'s existing conformance cases brings them to the spec in the same PR, never from output, and runs the whole suite its crates feed before its PR: the keeper's tripwires (#0179), the operator stand-in's ssm answers (#0181), the operator's command lists and help copies (#0178) | T03 | conformance.md, The harness; each card |
| state.md's release order follows the two walks; keeper.md's `pid=-` and the guard's known variable names | T03 | state.md (8ba4964); keeper.md (d4f46c0, 8325a55) |
| No test starts WSL; a bash test uses Git for Windows' bash or is ignored by default | T03 | #0181's card |
| An answer missing a member fails `describe-instance-information` and so sync; send-command and the poll get the wait's tolerance; a queued `restart` is a start | T03 | operator.md, the patch wait; #0181 |

## Lessons from v0.08

| What happened | What to do | Where |
|---|---|---|
| The architect wrote a placeholder start into `milestones.json` and filled it a second later; the running reporter read the placeholder and exited | Every edit leaves the file valid as written | architect.md, "Milestones on Terry's phone" |
| The architect named a commit's SHA in a ruling before the commit had printed one, and it was wrong | Send a message that cites a commit only after the commit's output | architect.md, "Writing in the repository" (times and SHAs) |
| A message to a running subagent turned it into a resumed one, which pinned the dev lead to its worktree | Put the whole tasking into the launch; launch a fresh subagent for any change | AGENTS.md, the worktree bullets |
| The dev lead launched a fix writer from the wrong worktree; it worked on a copy and wrote nothing there | Check the current worktree before every launch | This file |
| A keeper's check would have started the build's real operator as a guard, able to stop a real host, caught by the keeper drafter before any code | Name a stand-in for every program a component under test can start | conformance.md, The harness |
| Two of the architect's decisions carried wrong facts (D58's AWS limit, D61's exit code), both caught in T01's reviews | Read the source before stating an outside system's limit or a spec's convention in a decision | This file |
| In T02, #0176's reviewer searched the cargo registry's `windows-sys` folder with `Select-String`, and its output printed the profile path, so the Windows user name, into that subagent's transcript. Nothing reached a file, the repository, a card or a PR; the same kind as v0.07-T05's unmasked error | A search under `%USERPROFILE%` prints its matches' text only, never their paths, or runs through the mask; every later turn's brief names this slip with v0.07's four | AGENTS.md, "A live run on Terry's real setup MUST mask what it reads before it prints it"; each dev lead's brief |
| In T03, a stray shell redirect in one of #0178's subagents made an empty file in the Windows user profile's root, which it removed at once, by its report. Nothing under `.agentspace`, `~/.ssh` or Terry's other real state was touched and nothing was printed; the command was not recoverable, since the subagent's transcript was empty | A subagent writes only inside its worktree and throwaway folders, and its report quotes exactly any command that wrote elsewhere; every later brief names this slip | Each dev lead's brief; this file |
| In T03, #0178's conformance writer named a PowerShell helper `R`, the alias of `Invoke-History`, which wins over a function: the very slip AGENTS.md's alias rule records from v0.02. It ran in a fresh shell with no history, so nothing ran and no edit applied | Every brief repeats AGENTS.md's rule that a helper's name MUST NOT match an alias, checked with `Get-Alias <name>` | AGENTS.md, "The board" (localswim bullets); each dev lead's brief |
| T02's four stories sat in Ready For Work for its first 35 minutes, so its milestone's clock did not run, until Terry noticed | The architect moves every story of a turn to In Progress as it starts the turn, and moves every card itself | AGENTS.md, "The board" and "Roles and pull requests"; architect.md, step 6 |
| Four workspace runs at once on the laptop made three timing tests fail once each, each clean alone | Merge a turn's reviewed PRs locally and run the workspace once, not one run per PR beside the dev lead's | #0189; this file |
| A laptop shutdown stopped T03 mid-turn. Its dev lead's session survived in the app, and was resumed with its whole context; the first message after the restart ran no turn ("0 turns" in its transcript), and a second, sent on seeing that, started it | After a restart, resume a surviving dev lead rather than start a fresh one, check its transcript that a message started a turn, and send it again if not; push every branch before a shutdown, so nothing lives only on the laptop | handoff-v0.08.md, this row |
| The architect wrote a turn's close time into the handoff before reading the clock, again; it was caught and replaced before any commit | Write a close time only from `date`'s output at the archive, never ahead of it | architect.md, "Writing in the repository" |
| Two specs that link each other could not each merge after the other | Check every anchor on a local merge of all of them, then merge in an order that ends with none broken | This file; the architect's anchor check |

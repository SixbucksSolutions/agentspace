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

**As of 12:49pm Mountain, 2026-11-23, at T06's close, the last turn of v0.08.** The laptop moved from Pacific to Mountain time during T03; every time below is the laptop's zone when it was written.

| What | State |
|---|---|
| v0.08 (#0048), host lifecycle and cost guard | Every story merged: #0168 to #0188 and the epic in Ready For Review at T06's close. It closes on Terry's word, when he moves the cards to Completed; the `v0.08` milestone stops then, not before (AGENTS.md) |
| T01, the specs, the template and the host's files | Closed at 1:47am: #111 to #115 merged, #0168 to #0172 in Ready For Review |
| T02, the foundations | Closed at 8:26am: #116 to #119 merged, #0173 to #0176 in Ready For Review |
| T03, the programs | Closed at 7:48pm, after a laptop shutdown from about 10:40am to 5:11pm: #120 to #125 merged, #0177 to #0181 in Ready For Review |
| T04, conformance | Closed at 10:54pm: #126 to #129 merged, #0182 to #0185 in Ready For Review. It started at 8:08pm and ran 2 hours 46 minutes of its 3 |
| T05, the install and the host-side live checks | Closed at 10:38am on 2026-11-23: #130 merged, #0186 in Ready For Review. It started at 9:06am and ran 1 hour 32 minutes of its 3. One finding, #0195 ([its record](runs/e0.08-t05-s01.md)) |
| T06, the lifecycle live | Closed at 12:49pm on 2026-11-23: #131 and #132 merged, #0187 and #0188 in Ready For Review. It started at 10:40am and ran 2 hours 9 minutes of its 4. One finding, #0196 ([#0188's record](runs/e0.08-t06-s02.md)); #0187's record has none ([its record](runs/e0.08-t06-s01.md)) |
| `main`'s tests | Green: 4,849 passed, 0 failed and 58 ignored in #0185's quiet run of `cargo test --workspace` at 6e2fcbd, alone on the laptop ([its record](runs/e0.08-t04-s04.md)); `crates` is unchanged since. #0189's timing tests fail now and then under a parallel build, and #0191's four targets hung under heavy conformance load; both are rerun alone before they are findings |
| `main`'s conformance | Green against main's build: #0185's whole-suite run at 6e2fcbd, 2,640 passed, 0 failed, 0 skipped, with T04's new files run on their PRs (#126 to #128) and both 30-minute deadline cases passing. The empty-program control failed every file, as it MUST |
| A build of `main` | v0.08 at 469f6f6, a release build, installed on Terry's laptop by T05 at 9:13am on 2026-11-23. `main` has changed only in docs since |
| The hosts | Both stopped with no lease ready, confirmed by T06's step 5 and step 6 runs (11:52am and 12:27pm on 2026-11-23). Instance hours for T05 and T06: 3.805 of the 6 Terry approved (D62), T05 1.247 and T06 2.558 |
| Terry's laptop | v0.08 installed (T05); the keeper's task enabled and running, after T05's step 6 disabled it from 9:49am to 10:16am on his yes. `cfn-lint` 1.61.0 added as a uv tool, on Terry's word ("we can install new dependencies if needed", 2026-11-21) |
| Swimcoach | `v0.08` reported, its cap 19.5 hours after T03's extension; `v0.08-T01` to `v0.08-T06` stopped at their closes. T03's cap was extended from 3 hours to 4.5 on Terry's word |
| Sessions | T01's to T06's dev leads archived at their closes. T03's was resumed after the shutdown rather than started fresh, and kept its context |
| WSL | Found running at about 9:30am, started by T03's old bash test or the architect's T01 `wsl -e` runs; left for Terry. No later test starts it (T03 ruling). #0185's box tests found it running and left it running |

### v0.08 by turn

| Turn | Stories | Closed | PRs | What it left |
|---|---|---|---|---|
| T01 | #0168 to #0172 | 1:47am, 2026-11-22 | #111 to #115 | operator.md's guard, `status -All`'s leases, `grow`'s refusal, sync's patch wait and the host's files; keeper.md's start of the guard; state.md's live leases, horizon, `guard.log`, `guard.lock` and `release.log`; driver.md's two-walk release; the alarm at 24 hours; README's "A host's life"; `box/system`'s idle check, install and test |
| T02 | #0173 to #0176 | 8:26am, 2026-11-22 | #116 to #119 | `agentspace-state`'s `is_live`, `lease_end`, `live_count`, `last_lease_end` and `lease_horizon`; `agentspace-hosts`' guard lock; the new `agentspace-connections` (the two-walk end of an alias's clients and `release.log`'s text); `agentspace-box`'s `describe_hosts`, `stop_host` and `send_horizon`; the push's one-line stderr filter (#0167) |
| T03 | #0177 to #0181 | 7:48pm, 2026-11-22 | #120 to #125 | `agentspace-guard`, the guard's run; the operator's `guard`, `status -All`'s `Leases` and `Guard` lines and `grow`'s refusal (#0164); the keeper's start of the guard, with `agentspace-detach`'s process ID; the EC2 driver's two-walk release and `release.log` (#0064); sync's patch wait (#0163); `agentspace-zone`'s laptop clock time; the keeper suite's tripwires and the operator stand-in's ssm answers |
| T04 | #0182 to #0185 | 10:54pm, 2026-11-22 | #126 to #129 | Conformance for v0.08: the guard, `status -All`'s leases, `grow`'s refusal, sync's patch wait with its two deadline cases, and #0167's push filter (#0182); a release ending a lease's connections, R1 to R15 (#0183); the keeper's start of the guard, G1 to G16 (#0184); the whole suite, the crates and the box tests against main's build, with conformance.md's v0.08 rows (#0185) |
| T05 | #0186 | 10:38am, 2026-11-23 | #130 | v0.08 installed on the laptop; the alarm deployed to both stacks; `sync -All`; #0166's tools on both hosts; #0163 and pin P24.3's probe on agentspace's host; the laptop away, which found #0195 |
| T06 | #0187, #0188 | 12:49pm, 2026-11-23 | #131, #132 | The lifecycle live on both hosts: a quiet held lease, a release, the next lease and an expiry each, every stop inside its 25 minutes; on agentspace's host #0064 with Terry's desktop-app session, and an unexplained host stopped by its own idle check; finding #0196 |

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
| A spec's conformance-case table rules over a card's shorter list: every row is a case, R1 to R15 and G1 to G16 each a case of its own | T04 | Each spec's case table; #0182 to #0184 |
| R15 holds only for a second client started within the release; a late one is SKIPPED, in #0189's timing family | T04 | driver.md's R15 row (c932ef6); #0183 |
| At most three suite or workspace runs at once on the laptop; every command a runner runs, short or long, writes its exit code to a file through a wrapper | T04 | Each dev lead's brief; architect.md, "Checking that it still works" |
| A story with a 30-minute deadline case merges only once that case's result is in | T04 | #126's PR comment |
| A workspace hang under load is #0191's, rerun alone before it is a finding, and named in the run record; a story whose scope is not those crates still lands | T04 | #0191; [#0185's record](runs/e0.08-t04-s04.md) |
| A coverage gap a review finds outside the turn's scope is carded in Backlog and cited where conformance.md names it, never added to the next turn: #0192 (the idle check's four parts) and #0193 (`install.sh`'s `/etc/environment` edit) | T04 | conformance.md (8a86357); each card |
| A dev lead MAY rebase a story onto `main` before its review when the review checks rows against files merged since, and names the new head | T04 | #129 |
| operator.md's rows follow what T03 left: `grow`'s refusal, `status -All`'s files and provision's sign-in line | T04 | operator.md (27643b2, 4bb86d5, 1184365) |
| `.old-` files are removed by pattern, printing counts only; README's install as a whole (a running file renamed aside, the copy, the removal) is the allowed set of writes in `bin` | T05 | [T05's record](runs/e0.08-t05-s01.md), step 1 |
| With the guard live, a check takes each project's lease before the first deploy or sync starts its host; a guard stop of a host with no live lease is designed, of one under a live lease a finding | T05 | T05's record, rulings |
| A step that needs the keeper off for longer than a minute disables its task, which needs Terry's yes each time: given for T05's step 6 ("Yes, disable it") | T05 | #0186's card |
| A horizon sent is shown by the host's `/run/agentspace/lease-horizon` and `ssm list-commands`' guard entry, since `guard.log` never logs a send | T05 | operator.md, guard step 7; T05's record |
| #0188's "no session of the lease on the host" is no sshd login; a leftover Session Manager session is recorded against #0195 | T05 | #0188's card |
| A host's lifetime is counted from the observer's first running read to its first stopped read, with no gap when a guard stop is followed by a restart within the minute; a split at `guard.log`'s `stopped` line MAY be given as an estimate | T06 | [#0188's record](runs/e0.08-t06-s02.md), instance hours |
| A stop is attributed by `ec2 describe-instances`' `StateReason.Code`: `Client.InstanceInitiatedShutdown` is the host's own power-off, `Client.UserInitiatedShutdown` an API stop; `StateTransitionReason`'s text does not tell them apart | T06 | #0188's record, step 5; architect.md, live checks |
| A proxy holding a released alias alive after the release returned is a finding (#0196): driver.md has the release wait for every process it ends | T06 | #0196 |

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
| In T04 the architect sent a dev lead a commit hash, "e5fd2e9", that it had not read from git, a third slip of this kind in the release, and corrected it at once with the real one; its earlier ruling had also wrongly said T03 landed guard's help copies | Run the commit, then write the message, never in the other order; and read a file before ruling on what it holds | architect.md, "Writing in the repository"; this file |
| In T04, #0183's reviewer counted ssh-named processes by image path under `%TEMP%`, and the runners of #0182 to #0184 checked for leftover processes by filtering every process by image path, so all read the paths of processes they did not start, possibly Terry's real ssh; they printed only counts or names and PIDs of matches, of which there were none | A check names and counts only processes the case started, by PID, under its own folders | Each dev lead's brief |
| In T04, #0184's G10 stand-in guard wrote `set AGENTSPACE_` and `set USERPROFILE`'s output to a file under `%TEMP%`, so logged an environment's values, among them any `AGENTSPACE_*_CMD` the logon environment held, since keeper.md passes unknown ones to the guard; the file was never printed and was deleted at the run's end | A stand-in records the variables it names, and a case's own by presence only; never `set` or a whole environment, even to a temp file | AGENTS.md, "A test MUST NOT print, log or format the values of an environment it captured" |
| Two specs that link each other could not each merge after the other | Check every anchor on a local merge of all of them, then merge in an order that ends with none broken | This file; the architect's anchor check |
| T04's first taskings came from the cards alone and missed most of each spec's case table (G1 to G16, R1 to R15, about 20 of operator.md's rows, #0167 unstaffed); three follow-up writers were needed | Before launching writers, the dev lead maps every row of the spec's case table to a writer | Each dev lead's brief; this file |
| Reviewers working beside the runners, from source and the row map alone, saved about an hour on two stories, but their fixes landed while the runners still ran, so each story needed a confirming run on its final head | Plan a confirming run on the final head from the start | This file |
| Auto mode refused two of a fix writer's edits with "classifier gave no verdict", an infrastructure failure, not a denial; the writer stopped and the dev lead applied them. The architect's own archive of the dev lead needed a resend for the same reason | On a no-verdict refusal, stop and report the exact edit, or resend the call once as the refusal says | Each dev lead's brief |
| A coverage writer's greppable markers differed in one text, so the dev lead's grep counted 13 of 14; the confirming pass found the 14th | Prove a pattern on every planted variant before believing its count | Terry's global rule; this file |
| A runner recorded fmt's and clippy's streams but not their exit codes, so the record could not claim them, and both were rerun; another ended its run waiting on its own background deadline case before the wrapper rule reached it | Every command writes its exit code to a file, and a runner waits in the foreground | Each dev lead's brief |
| Worktree isolation refuses compound git commands and `git -C` into a sibling worktree | A tasking that needs a sibling story's files says to read them with Read, Grep or `git show <sha>` | Each dev lead's brief |
| A workspace test binary ended with exit -1, and the architect named a cause twice before reading the runner's files: first the tool's 10-minute limit, then an outside kill, which it wrote onto #0190 as context. The runner's own files showed it had ended a 28-minute hang itself; the context was taken off #0190 the same night | Ask for the runner's files and read them before stating a cause, in a message or on a card | This file |
| The milestone reporter, started while the laptop was on Pacific time, kept printing Pacific times after it moved to Mountain, because .NET caches the local zone for the life of a process; caught at T04's close, when its "9:53pm" was a 10:53pm report | A long-running PowerShell process clears the zone cache before each printed time; the reporter does since 53c5a5a | `scripts/report-milestones.ps1` |
| `deploy`'s own last step is `sync`, which starts the host, so with the guard live T05 took each project's lease before its first deploy | Take the leases before the first command that can start a host | T06's brief |
| Stopping the keeper by its PID lasts under a minute, because its task's one-minute trigger starts a new one; T05 needed Terry's yes mid-turn to disable the task | Ask for the task's disable when the turn is planned, not mid-turn | architect.md, Running a turn, step 5 |
| The architect ruled that `guard.log` would show the horizon sent, which it never logs, and ruled step 6's bound as one check after the horizon plus 15 minutes, stricter than D57's one check after the last session ends; the dev lead and the record's review caught both | Read the spec's text before ruling what a log shows or what a bound is | This file |
| A lease document's alias is `ssh.host_alias`, not `alias`; a runner's first three ssh calls used an empty alias, failed on the laptop and reached nothing | Every runner's brief names the field | T06's brief |
| `Start-Process pwsh -Command` took only part of a command line not passed as one quoted argument; two runners each lost a launch, one about 8 minutes | A runner checks within 10 seconds that its background wrapper wrote its output file | T06's brief |
| `ConvertFrom-Json` turned an ISO time into a local one, so a runner printed `expires_at` 7 hours off; its comparison used the right value | Read JSON times with `-DateKind String` | AGENTS.md, the milestones.json rule; T06's brief |
| The record's reviewer hit a git error about a file in `%TEMP%`, which printed the short profile folder name into its transcript; nothing was written anywhere | Proofs and scratch files go inside the worktree, never `%TEMP%`, since an error echoes its path | T06's brief |
| Twice the dev lead sent counts worked out by hand that the record writer's recount corrected (14 short sessions for 13; "about 21 minutes" for 24 minutes 40 seconds) | A message carries counts and intervals only from a command over the files | Each dev lead's brief |
| The architect's own watch on the keeper's task exited 0 without printing its line, so it said nothing; the state was then read directly, `Running` at 10:16am | A one-shot watch exits by printing its line and calling `exit`, and its silence is checked, not trusted | This file |
| Splitting the step that waited on a ruling into its own runner, and writing the record in two passes, let T05 close in 1 hour 32 minutes of its 3 | Plan the record writer's first pass while the last live step runs | This file |
| T06's dev lead printed the Windows user name into its own transcript with `ls -la` on a stop file, whose long listing shows the owner; nothing reached a file, the repository, a card or another session | List a file with a bare `ls` or `Test-Path`, never a long listing | Every brief |
| Two of T06's taskings named different stop files for one story's runner and observer, so the observer ran on until the dev lead wrote the right one; the hours were unaffected | Every tasking of a story names its shared files from one list | This file |
| A background job that a runner started died with its tool call, leaving a wrapper with no exit file and a 2-minute gap in a poll; and the subagents' shell moves a foreground command of about 180 seconds or more to the background whatever the brief says | Size each wrapper run under 3 minutes, or have the runner wait on the wrapper's exit file | Every brief |
| T05's live kit (the mask, its proof and the wrapper) lived only in its worktree's `target\` and went with it; T06 rebuilt it from T05's record | Commit a live kit's scripts under `scripts\` as process tools (D28) after a live turn | This file; a follow-up before the next live turn |
| The dev lead's own scan script, dot-sourced, lost its `$Path` to the sourced script's `param()` and passed every file; a runner caught it by planting a value | Prove every checker, the dev lead's own included, fails on a planted value before believing its pass | Terry's global rule; every brief |
| With the keeper running, a new lease's holder starts within about 5 seconds and starts a stopped host before the caller's first ssh, so the caller's time to a prompt can measure a host already booting | A later spec says what "time to a prompt" measures with the keeper live | Both T06 records |
| A once-a-minute observer missed a host's stopped state when the next lease restarted it within the minute | An observer logs `stopping` and `pending` reads as interval edges too | This file |
| The live count of #0188's clients matched the alias as a substring, while the release counts by driver.md's token rule, so the two counts could not be compared, which left #0196 with two readings | A live count of clients uses driver.md's own token rule | #0196; this file |
| The architect ruled "not shown by alias" on the dev lead's counts before asking how they were made; they were by alias, and the ruling was withdrawn minutes later | Ask how a count was made before ruling on what it shows | This file |
| Splitting the runner that waited on Terry into two, as T05 did, cost his step about 17 minutes of wall time and left no runner waiting on a message | Split a step that waits on Terry into its own runner | This file |

# Handoff: agentspace after v0.08

RFC 2119 keywords; the capitals are load-bearing.

> **Final, as of 1:33pm Mountain on 2026-11-23.** v0.08 is closed: all twenty-two of its pull
> requests (#111 to #132) are merged to `main`, and Terry approved #0048, its twenty-one stories and
> the six Backlog cards it met into Completed. Started as the live record of v0.08 at T01's close,
> 1:47am Pacific on 2026-11-22, and refreshed at each turn's close. Where this file and AGENTS.md,
> the board or decisions.md disagree, they win, and the disagreement is a bug to fix in this file.

**Written for the architect who starts v0.09, after Terry `/clear`s this one.** It assumes you have
read nothing yet. It cites AGENTS.md, architect.md and the decisions by section rather than
restating them, so each rule keeps one home.

## The short version

- **v0.08 built the cost guard, and it works live.** The keeper on Terry's laptop runs a guard
  every 5 minutes that stops a host 15 minutes after its project's last live lease ends. Each host
  also powers itself off at its lease horizon, and a 24-hour CloudWatch alarm backs both up.
  T06 showed every stop live, on both hosts.
- **Nothing is running.** Both hosts are stopped, no lease is ready, no milestone exists, the
  reporter is stopped, and every dev lead is archived.
- **v0.08 is installed on Terry's laptop** from T05's release build, so the guard is live there
  now: a host started with no lease is stopped about 15 minutes later.
- **Next is v0.09 (#0049), size classes, and it MUST NOT be laid out until Terry says so.** Its
  first design call is whether v0.06-T01's whole-turn record meets D33's re-measurement
  requirement ([The scope ahead](#the-scope-ahead-v009)).
- **Two live findings are open,** #0195 and #0196, with five more cards from v0.08's turns, all in
  Backlog ([What is open](#what-is-open)).
- **The rules broken most often in v0.08** were stating a time, SHA or count before a command gave
  it, and printing Terry's details into a transcript. [Rules that are easy to break](#rules-that-are-easy-to-break)
  lists them first.

## Start here

**Do these in order before anything else.** Each comes from a gap that cost time in a release.

1. **Read, in this order:** this file; [AGENTS.md](../AGENTS.md), every standing rule;
   [architect.md](architect.md), how a release is run, from Terry's word to Completed;
   [decisions.md](decisions.md), D56 to D63 in full with their "Changed for v0.08-T01" notes, then
   D27, D33, D39 and D5 for v0.09's design; then [roadmap.md](roadmap.md), card #0049 and
   [footprint.md](footprint.md).
2. **Check what is running.** A cleared or restarted session leaves no background task of its own,
   but an orphan from another session can still hold a reader. Print PIDs and kinds only, never a
   command line whole:
   ```powershell
   Get-CimInstance Win32_Process -Filter "Name='pwsh.exe'" | Where-Object { $_.CommandLine -match 'report-milestones|watch\.ps1|watchdog\.ps1' } | ForEach-Object { $_.ProcessId }
   ```
   Any process found MAY be stopped by its checked PID, with Terry's yes.
3. **Check the board answers:** `(Test-NetConnection 127.0.0.1 -Port 8811).TcpTestSucceeded`. If
   it is `False`, localswim is down; starting it is Terry's, and he MUST be asked. It was down at
   the start of 2026-11-23, after the laptop's restart.
4. **Arm the board watcher** under the Monitor tool, with a forward-slash path, 30-minute timeout,
   re-armed at each expiry:
   `pwsh -NoProfile -File 'C:/Projects/claude-localswim-watcher/watch.ps1' -Board agentspace -Reader agentspace 2>&1`.
   The reporter starts only once `scripts/milestones.json` lists a milestone; today it lists none.
5. **Check AWS and the hosts, printing nothing that identifies them.** First the sign-in, by exit
   code alone: `aws sts get-caller-identity --output text *> $null; $LASTEXITCODE`. A non-zero
   code means Terry runs `aws login --remote` and pastes its link into a fresh Incognito window,
   every other Incognito window closed (AGENTS.md). Then `status -All` through a mask proven on a
   planted value of every shape first ([The architect's tools](#the-architects-tools) has one).
   Both hosts SHOULD read `stopped`.
6. **Wait for Terry's word before laying out v0.09** (AGENTS.md, "The board"). Until then, do the
   prep in [The scope ahead: v0.09](#the-scope-ahead-v009).

## What changed from the v0.07 handoff, and why

- **A short version comes first.** v0.08's live record grew to 45 lessons and over forty rulings, and a
  cleared architect has to know the state before it can use any of them.
- **The lessons are grouped by what they are about**, as the MVP handoff's were, each one line with
  its action, instead of one table in the order they happened. Every lesson of v0.08 is kept.
- **"Rules that are easy to break" is back**, from the MVP to v0.04 handoffs, and cites each rule's
  home. It lists the rules v0.08 actually broke, most often first.
- **The board's lane IDs and the localswim commands are written down.** The architect looked up
  the Needs Terry lane's ID mid-turn in T06, while Terry waited.
- **A dev lead's brief is given as a template**, since T05's and T06's briefs were each built
  from scratch, and both carried the same slips list.
- **The scope ahead starts from what v0.06 and v0.08 already measured or left**, so v0.09's first
  design call is a reading of evidence that exists, not a new measurement by default.

## Where things stand

**As of 1:33pm Mountain, 2026-11-23.** The laptop moved from Pacific to Mountain time during
v0.08-T03; every time in this file is the laptop's zone when it was written.

| What | State |
|---|---|
| v0.08 (#0048), host lifecycle and cost guard | Completed: the epic, its twenty-one stories (#0168 to #0188) and the six Backlog cards it met, approved by Terry on 2026-11-23. Marked done in [roadmap.md](roadmap.md) |
| v0.09 (#0049), size classes | In Ready For Work, not laid out. It MUST NOT be broken into stories until Terry says so |
| `main`'s tests | 4,849 passed, 0 failed, 58 ignored, in T04's quiet `cargo test --workspace` at 6e2fcbd, alone on the laptop ([runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md)); fmt and clippy clean. No crate has changed since. Under load, #0189's timing tests can fail and #0191's four targets can hang |
| The conformance suite | 72 files and the deadline case, 2,640 passed, 0 failed, 0 skipped, against 6e2fcbd's build (same record); T04's new files ran on their own PRs (#126 to #128) |
| The install on Terry's laptop | v0.08 from 469f6f6, a release build, installed by T05 at 9:13am on 2026-11-23; `.old-` files: 2 removed, 0 left. The keeper's task is enabled and running. No change to his ssh config, task definition or `PATH` |
| The hosts | Both `stopped`, no lease ready (T06, 11:52am and 12:27pm on 2026-11-23). The box (`agentbox`, swimcoach's): `m9g.xlarge`, 20 GiB. `agentspace-host-agentspace`: `m9g.xlarge`, 8 GiB. Both stacks carry D58's 24-hour alarm since T05 |
| Instance hours | D62's 6 for T05 and T06: 3.805 used (T05 1.247, T06 2.558). The yes was for those two turns only |
| Leases | None ready, by T06's last reads; 33 recorded |
| The board | Every v0.08 card in Completed. Open from v0.08: #0189 to #0193, #0195, #0196, in Backlog |
| Swimcoach | No milestone; `scripts/milestones.json` is empty, and `v0.08` was stopped at 1:33pm. The reporter is stopped |
| Sessions | Every dev lead is archived; no worktree is left (`git worktree list` shows `main` alone) |
| WSL | Last seen running by T04's box tests on 2026-11-22, left as found; not checked since. No test starts it (v0.08-T03's ruling) |
| Terry's laptop, otherwise | `cfn-lint` 1.61.0 added as a uv tool in T01, on his word ("we can install new dependencies if needed", 2026-11-21) |

### v0.08 by turn

| Turn | Stories | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|
| T01, the specs | #0168 to #0172 | 1:47am, 2026-11-22 | #111 to #115 | | operator.md's guard, `status -All`'s leases, `grow`'s refusal, sync's patch wait; keeper.md's start of the guard; state.md's live leases, horizon and the guard's files; driver.md's two-walk release; the alarm at 24 hours; `box/system`'s idle check, install and test |
| T02, the foundations | #0173 to #0176 | 8:26am, 2026-11-22 | #116 to #119 | | The live-lease functions in `agentspace-state`, the guard lock, the new `agentspace-connections`, the box's describe, stop and horizon send, the push's one-line filter |
| T03, the programs | #0177 to #0181 | 7:48pm, 2026-11-22 | #120 to #125 | Cap extended from 3 h to 4.5 h on Terry's word; a laptop shutdown in its middle | `agentspace-guard`; the operator's `guard`, `status -All` and `grow`; the keeper's start of the guard; the two-walk release; sync's patch wait; `agentspace-zone`'s clock time |
| T04, conformance | #0182 to #0185 | 10:54pm, 2026-11-22 | #126 to #129 | 2:46 of 3 h | v0.08's conformance cases, and the whole suite, crates and box tests against main's build |
| T05, the install and host checks | #0186 | 10:38am, 2026-11-23 | #130 | 1:32 of 3 h | The install, the alarm deployed, the tools and patch wait live, the laptop away; finding #0195 |
| T06, the lifecycle live | #0187, #0188 | 12:49pm, 2026-11-23 | #131, #132 | 2:09 of 4 h | Hold, release, next lease and expiry on both hosts; #0064 with Terry's desktop-app session; an unexplained host; finding #0196 |

## What v0.08 built

| Turn | Story | Card | PR | What |
|---|---|---|---|---|
| T01 | E0.08-T01-S01 | #0168 | #115 | operator.md: the guard, `status -All`'s leases, `grow`'s refusal, sync's patch wait and the host's files |
| T01 | E0.08-T01-S02 | #0169 | #111 | keeper.md and state.md: the keeper starts the guard, and the guard's files |
| T01 | E0.08-T01-S03 | #0170 | #114 | driver.md and lease-api.md: release ends the lease's connections |
| T01 | E0.08-T01-S04 | #0171 | #112 | The alarm at 24 hours, its change sets read live, and README |
| T01 | E0.08-T01-S05 | #0172 | #113 | `box/system`: the idle check's lease horizon, the patch start and the `PATH` |
| T02 | E0.08-T02-S01 | #0173 | #117 | `agentspace-state` and `agentspace-hosts`: live leases, horizons and the guard's lock |
| T02 | E0.08-T02-S02 | #0174 | #118 | `agentspace-connections`: find and end the laptop's clients of an alias |
| T02 | E0.08-T02-S03 | #0175 | #116 | `agentspace-box`: the guard's describe, stop and horizon send |
| T02 | E0.08-T02-S04 | #0176 | #119 | `agentspace-push` and `agentspace-ssh-proxy`: one ready line and the push at a start (#0167) |
| T03 | (architect's pin) | | #120 | `agentspace-zone`: `local_clock_time`, one home for the laptop's clock time |
| T03 | E0.08-T03-S01 | #0177 | #121 | `agentspace-guard`, the guard's run |
| T03 | E0.08-T03-S02 | #0178 | #125 | The operator's `guard`, `status -All`'s leases and `grow`'s refusal (#0164) |
| T03 | E0.08-T03-S03 | #0179 | #124 | The keeper starts the guard |
| T03 | E0.08-T03-S04 | #0180 | #122 | The EC2 driver's release ends the lease's connections (#0064) |
| T03 | E0.08-T03-S05 | #0181 | #123 | Sync waits out a fresh boot's patch run (#0163) |
| T04 | E0.08-T04-S01 | #0182 | #126 | Conformance: the guard, `status -All`'s leases, `grow`'s refusal, sync's patch wait and #0167 |
| T04 | E0.08-T04-S02 | #0183 | #127 | Conformance: release ends the lease's connections, R1 to R15 |
| T04 | E0.08-T04-S03 | #0184 | #128 | Conformance: the keeper starts the guard, G1 to G16 |
| T04 | E0.08-T04-S04 | #0185 | #129 | The whole suite, the crates and the box tests against main's build ([runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md)) |
| T05 | E0.08-T05-S01 | #0186 | #130 | The install and the host-side live checks ([runs/e0.08-t05-s01.md](runs/e0.08-t05-s01.md)) |
| T06 | E0.08-T06-S01 | #0187 | #131 | The lifecycle live on the box ([runs/e0.08-t06-s01.md](runs/e0.08-t06-s01.md)) |
| T06 | E0.08-T06-S02 | #0188 | #132 | The lifecycle live on agentspace's host, an unexplained host and #0064 ([runs/e0.08-t06-s02.md](runs/e0.08-t06-s02.md)) |

**The six Backlog cards v0.08 met**, approved into Completed by Terry on 2026-11-23 after he moved
them out of Backlog: #0043 (OBE by D58's 24-hour backstop), #0064 (D59's two walks, live in T06),
#0163 (sync's patch wait, live in T05), #0164 (`grow`'s refusal), #0166 (`/etc/environment`'s
`PATH`, live in T05) and #0167 (the push's filter).

## Terry's orders from v0.08

**Each binds where its row says; this table only finds them.**

| Order | Given | Where it binds |
|---|---|---|
| The grace is 15 minutes; the box is guarded; an unexplained host is reported by `guard.log` and `status -All` alone (Q8 to Q10) | 2026-11-21 | D56, D60 |
| Yes for up to 6 instance hours for T05 and T06 | 2026-11-21 | D62 |
| As many subagents per story as can be used effectively | 2026-11-21 | D31, D63; architect.md, "Breaking an epic into stories" |
| New dependencies MAY be installed when needed; the dev lead names each to the architect first | 2026-11-21 | This file |
| Every story of a turn goes to In Progress when the turn starts, moved by the architect | 2026-11-22 | AGENTS.md, "The board"; architect.md, "Running a turn", step 6 |
| The architect MUST be the only agent that moves a card | 2026-11-22 | AGENTS.md, "Roles and pull requests" |
| A turn's extension also raises the release's cap | 2026-11-22 | architect.md, "Milestones on Terry's phone" |
| Yes to disabling the keeper's task for T05's step 6 only ("Yes, disable it") | 2026-11-23 | #0186's card |

## Rules that are easy to break

**Each was broken at least once in v0.08, most often first. Each has its home, which wins.**

1. **A time, a SHA, a count or an interval MUST come from a command's output, never ahead of it or
   from memory.** Broken at least six times by the architect and the dev leads: two SHAs, two
   times written ahead of the clock and two counts worked out by hand. A close time is read with
   `date` at the archive (architect.md, "Writing in the repository").
2. **Nothing of Terry's MAY reach a transcript:** not his Windows user name, profile path, computer
   name, a lease ID or alias, or an AWS identifier. Broken three times in v0.08, each in a
   transcript only and nowhere else: a `Select-String` path, a `%TEMP%` path in a git error and an
   `ls -la` owner (AGENTS.md, "A live run on Terry's real setup MUST mask what it reads before it
   prints it"). A value only Terry needs goes to him as a command he runs himself, as T06's alias
   did.
3. **Read the source before ruling on what it says.** The architect decided or ruled from memory
   five times in v0.08 (D58's AWS limit, D61's exit code, T03's help copies, `guard.log` showing a
   horizon, a bound stricter than D57's) and once from an assumption about how a count was made.
   Each was caught and corrected (architect.md, "Running a turn", step 10).
4. **Read the runner's files before stating a cause.** An exit -1 was given two wrong causes, one
   written onto a card, before the runner's files showed its own kill.
5. **The architect is the only agent that moves a card**, and moves a card only where Terry's
   process or his word puts it. When he moved six cards out of Backlog, the architect's move on
   took his "Yes, move them"; auto mode refused the guess first (AGENTS.md, "Roles and pull
   requests").
6. **Every check only touches processes it started, by name and PID.** Broken in T04 by scans of
   every process's image path (AGENTS.md, "Terry's real files in a review"; #0190).
7. **A test or stand-in MUST NOT log an environment's values**, even to a temporary file (AGENTS.md).
   Broken in T04's G10 stand-in.
8. **A PowerShell helper's name MUST NOT match an alias** (AGENTS.md, "The board"). Broken in T03
   (`R`).
9. **Every command writes its exit code to a file**, and a check is proven to fail on a planted
   value before its pass is believed (architect.md, "Checking that it still works"; Terry's global
   rule). Broken by a scan that passed every file in T06.
10. **`stop` goes to swimcoach only on Terry's word for a release, and a turn's milestone stops at
    its dev lead's archive** (AGENTS.md, "Status on Terry's phone").

## Lessons from v0.08, by what they are about

**Each lesson says what to do and where it is now written. Every lesson of the release is here.**

### Terry's details and real files

| What happened | What to do | Where |
|---|---|---|
| T02: a `Select-String` under the cargo registry printed the profile path into a transcript | Search under `%USERPROFILE%` printing matches' text only, or through the mask | AGENTS.md, masking; every brief |
| T03: a stray redirect made an empty file in the profile root, removed at once | A subagent writes only inside its worktree, and reports any command that wrote elsewhere | Every brief |
| T04: reviewers and runners scanned every process by image path | Count only the case's own processes, by name and PID | Every brief; #0190 |
| T04: a stand-in wrote `set`'s output to a temp file | Record the variables named, and the case's own by presence only | AGENTS.md |
| T06: the record's reviewer hit a git error that printed a `%TEMP%` path holding the profile folder's short name | Keep proofs and scratch inside the worktree, never `%TEMP%` | Every brief |
| T06: `ls -la` on a stop file printed the Windows user name, the file's owner | List with a bare `ls` or `Test-Path`, never a long listing | Every brief |
| T06: Terry needed a lease's exact alias | Give him a command that prints it on his own screen; the alias never enters a transcript | This file |

### Facts, times and counts

| What happened | What to do | Where |
|---|---|---|
| The architect wrote a placeholder start into `milestones.json`, and the running reporter read it and exited (T01) | Every edit leaves the file valid as written, with the real time | architect.md, "Milestones on Terry's phone" |
| The architect sent two SHAs before git printed them (T01, T04) | Run the commit, then write the message | architect.md, "Writing in the repository" |
| The architect wrote two close times before reading the clock (T03, T06) | Write a time only from `date`'s output | architect.md |
| A dev lead sent counts and intervals worked out by hand, which the record writer corrected (T05) | A message carries counts only from a command over the files | Every brief |
| `ConvertFrom-Json` turned `expires_at` into local time, 7 hours off (T05) | Read JSON times with `-DateKind String` | AGENTS.md; every brief |
| The reporter kept printing Pacific times after the laptop moved to Mountain: .NET caches the zone per process (T04) | Clear the zone cache before each printed time; the reporter does since 53c5a5a | `scripts/report-milestones.ps1` |
| A once-a-minute observer missed a stopped state when the next lease restarted the host within the minute (T06) | Count the interval whole; log `stopping` and `pending` reads as edges too | #0188's record |

### Rulings and diagnosis

| What happened | What to do | Where |
|---|---|---|
| D58 and D61 carried wrong facts, caught in T01's reviews | Read the source before stating a limit or a convention in a decision | This file |
| T04: a ruling said T03 landed help copies it had not | Read a file before ruling on what it holds | This file |
| T04: an exit -1 was given two causes before the runner's files were read | Read the runner's files first | This file |
| T05: rulings on `guard.log` showing a horizon, and on step 6's bound, did not match the spec | Read the spec's text before ruling what a log shows or a bound is | This file |
| T06: "not shown by alias" was ruled before asking how the counts were made | Ask how a count was made before ruling on what it shows | This file |
| T06: who stopped an unexplained host was settled by one read-only call, `StateReason.Code` | Attribute a stop by `StateReason.Code`, starting nothing | architect.md, live checks |

### Running a turn and its subagents

| What happened | What to do | Where |
|---|---|---|
| A message to a running subagent turned it into a resumed one, pinned to the dev lead's worktree (T01) | Put the whole tasking into the launch; launch fresh for any change | AGENTS.md |
| A fix writer launched from the wrong worktree wrote nothing (T01) | Check the current worktree before every launch | This file |
| T02's stories sat in Ready For Work for 35 minutes, so its clock did not run | Move every story to In Progress as the turn starts | AGENTS.md; architect.md, step 6 |
| T03 stopped mid-turn on a laptop shutdown; the first resume message ran no turn | Resume a surviving dev lead, check its transcript that a turn ran, resend if not; push every branch before a shutdown | This file |
| T04's taskings came from the cards and missed most of the specs' case tables | Map every row of the spec's case table to a writer before launching | Every brief |
| Reviewers beside runners saved an hour, but their fixes needed confirming runs (T04) | Plan the confirming run on the final head from the start | This file |
| A "classifier gave no verdict" refusal is an infrastructure failure (T04; the architect's archives every turn) | Stop and report the exact call, or resend it once as the refusal says | Every brief |
| A grep counted 13 of 14 markers whose text differed (T04) | Prove a pattern on every planted variant | Terry's global rule |
| A dot-sourced scan lost its `$Path` and passed every file (T06) | Prove every checker fails on a planted value, the dev lead's own included | Every brief |
| Worktree isolation refuses compound git and `git -C` into a sibling (T04, T06) | Read a sibling's files with Read, Grep or `git show <sha>`; plain single git commands | Every brief |
| Two taskings of one story named different stop files (T06) | Name a story's shared files from one list in every tasking | This file |
| Splitting a step that waits on a ruling or on Terry into its own runner kept the rest moving (T05, T06) | Split such a step; write the record in two passes | This file |

### Builds, tests and load

| What happened | What to do | Where |
|---|---|---|
| Four workspace runs at once made three timing tests fail (T02) | At most three heavy runs at once; rerun a timing failure alone first | architect.md; #0189 |
| Test binaries hung with no CPU under heavy conformance load, and passed alone (T04) | Rerun a hang alone before it is a finding, and name it in the record | #0191 |
| A runner kept no exit code for fmt and clippy (T04) | Every command writes its exit code to a file through a wrapper | architect.md |
| The subagents' shell moves a foreground command past about 3 minutes to the background (T06) | Wait on the wrapper's exit file, not the command | architect.md, live checks |
| A background job died with its tool call, leaving a 2-minute gap (T06) | Check within 10 seconds that a background wrapper wrote its output file | Every brief |
| Two specs that link each other could not each merge after the other (T01) | Check every anchor on a local merge of all of them, then pick the order | This file |
| A keeper check would have started the build's real operator as its guard, able to stop a real host; the keeper drafter caught it before any code (T01) | Name a stand-in for every program a component under test can start | conformance.md, The harness; AGENTS.md |

### Live checks on real hosts

| What happened | What to do | Where |
|---|---|---|
| `deploy` ends in `sync`, which starts the host, so with the guard live T05 took leases before its first deploy | Take each project's lease before the first command that can start its host | T05's record |
| Stopping the keeper by PID lasts under a minute; T05 needed Terry's yes mid-turn to disable the task | Ask for a task disable when the turn is planned | architect.md, step 5 |
| `guard.log` never logs a horizon sent | Show it by the host's `/run/agentspace/lease-horizon` and `ssm list-commands` | operator.md, guard step 7 |
| A lease document's alias is `ssh.host_alias`, and `lease list` documents have no `ssh` field at all | Brief the field; build an alias from the ID by lease-api.md's rule when needed | Every brief |
| A live kit (mask, proof, wrapper) lived in a worktree's `target\` and was lost; T06 rebuilt it | Commit the kit under `scripts\` before the next live turn | [What is open](#what-is-open) |
| With the keeper running, a new lease's holder starts a stopped host before the caller's first ssh | A later spec says what "time to a prompt" measures with the keeper live | Both T06 records |
| The live count of clients used a substring, the release the token rule, so they could not be compared | Count clients by driver.md's own token rule | #0196 |
| A killed client's Session Manager session outlives it about 21 minutes and holds the idle check off | Read "no session" as no sshd login until #0195 is fixed | #0195; #0188's card |
| The architect's one-shot watch exited 0 without printing its line | Exit by printing the line then `exit`, and check a silent watch directly | This file |

## Pitfalls of the laptop's tooling

**None is agentspace's design; each cost a retry in v0.07 or v0.08.**

| Pitfall | What works |
|---|---|
| The Monitor tool runs bash, which drops a Windows path's backslashes | `'C:/Projects/claude-localswim-watcher/watch.ps1'` |
| `%USERPROFILE%` does not expand in PowerShell, Terry's shell | `& "$env:USERPROFILE\.agentspace\bin\agentspace.exe" ...` in anything sent to him |
| Windows PowerShell 5.1's `ConvertFrom-Json` passes an array down the pipe as one object | Wrap the parse in parentheses, or read the named member (`.leases`) |
| `lease list` prints `{"leases": [...]}`, and its documents have no `ssh` field | Read `.leases`; the alias is `agentspace-<project>-<first 8 hex digits of id>` (lease-api.md) |
| `Start-Process pwsh -Command` splits a command line not given as one quoted argument | Pass the whole command line as one quoted argument, and check its output file appears |
| A Python string in a bash heredoc turns `\r` in a path into a carriage return | Use `\\` or a raw string; grep the file for `$'\r'` after a scripted edit |
| A here-string piped to `git commit -F -` reaches git as an argument | Write the message to a scratch file, or use `-m` with a heredoc in bash |
| `pwsh -File script.ps1 -Files a,b` passes the list as one string | Pass one comma-separated string and split it in the script |
| A review worktree under the scratchpad's long path fails on box/tests' fixtures | Put review worktrees under `.claude\worktrees\` |
| A worktree folder stays held by the session that was in it | Remove the empty folder after that session is archived |
| `archive_session` is refused once by a hook that relabels the call, and refused while the dev lead is mid-turn | Resend with exactly the input the refusal quotes; retry after its turn ends |
| Bot's card moves out of Backlog, or from Ready For Work straight to Ready For Review, answer HTTP 403 | Ask Terry to move the card out; then the architect moves it on, on his word |
| `agentspace-operator status` prints on stderr | Capture `2>&1` |
| A scan for a short name fragment matched "stdout" | Anchor names with `\b`, and look at every hit |
| `localswim` on `PATH` is stale | `C:\Projects\localswim\localswim.exe`, by full path, as Bot |

## The architect's tools

| Tool | What it does |
|---|---|
| `scripts/scan-identifiers.ps1 -Head refs/review/pr<n> -Base origin/main -Pr <n>` | The identifier scan of a PR's diff, commits and body, each pattern proven on a planted value first; prints counts, never values |
| `scripts/run-conformance-files.ps1 -Checkout <worktree> -Out <folder> -Files '<a>,<b>' [-Dist <folder>]` | Runs conformance files against a build's programs; `-Dist` MAY name another build when no crate differs |
| `scripts/watchdog.ps1 -Lead <name>` and `-Reset` | The 10-minute watchdog on a dev lead; restart it with `-Reset` first |
| `scripts/report-milestones.ps1 -Watch` | The swimcoach reporter, whenever a milestone exists |
| `scripts/milestones.json` | The milestones, edited as text only, with the real time, in one atomic write |
| `C:\Projects\swimcoach\bin\swimcoach.cmd stop --project agentspace --milestone <name>` | Stops a milestone, after it is out of `milestones.json` |

**The board,** board `c74ddc68-9c3a-4434-8435-563c596d3229`. Every call loads the Bot credential
for one run: `. C:\Projects\claude-localswim-watcher\lib\env.ps1; Set-LocalswimEnv; try { ... }
finally { Clear-LocalswimEnv }`.

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
| `card get <board> <n>` | A card by bare ticket number (`188`, not `#0188`); JSON with `lane_id`, `revision`, `description` |
| `card move <board> <n> <lane>` | A move; only the architect's |
| `card create --description <text> <board> <lane> <title>` | A new card; then find its ticket with `card search --search <words> <board>` |
| `card description --revision <r> <board> <n> <file>` | Replaces a description from a file; read the revision with `card get` first |
| `relationship create <board> <source> <kind> <target>` | `blocks`, `references` or `relates_to` |
| `board get <board>`, then `lane get <board> <lane>` | The lane IDs, if this table is ever stale |

**A mask for live output**, proven on one planted value of each shape before it is trusted:

```powershell
function Get-Masked([string]$t) {
  $t = $t -replace 'i-[0-9a-f]{8,17}', '<instance>' -replace 'vol-[0-9a-f]{8,17}', '<volume>' -replace '(sg|subnet|vpc)-[0-9a-f]{8,17}', '<net>'
  $t = $t -replace '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}', '<uuid>' -replace 'agentspace-[a-z0-9-]+-[0-9a-f]{8}\b', '<alias>'
  $t = $t -replace '\b\d{1,3}(\.\d{1,3}){3}\b', '<address>' -replace 'ip-\d+-\d+-\d+-\d+[\w.-]*', '<host>' -replace '\b\d{12}\b', '<account>' -replace '[\w.-]+\.compute(\.internal|\.amazonaws\.com)', '<host>'
  $t -replace [regex]::Escape($env:USERPROFILE), '<profile>' -replace [regex]::Escape($env:USERNAME), '<user>' -replace [regex]::Escape($env:COMPUTERNAME), '<computer>'
}
```

**A PR review, in commands** (architect.md, "Reviewing a pull request", gives the why):
1. `git fetch origin pull/<n>/head:refs/review/pr<n>`, and check the SHA against the dev lead's.
2. When the PR's checks run programs: `git worktree add --detach .claude\worktrees\rv<n> refs/review/pr<n>`,
   `cargo xtask dist` in it, and `scripts/run-conformance-files.ps1` over the touched files. A
   docs-only PR, such as a run record, needs no build.
3. `scripts/scan-identifiers.ps1`, then look at every hit; check
   `git log --format='%an %ae %cn %ce' origin/main..refs/review/pr<n>`.
4. `gh pr merge <n> --squash --match-head-commit <sha> && git push origin --delete <branch>`, and
   only then move the card and tell the dev lead; remove the review worktree last.

**A dev lead's brief, as a template.** `start_session` with `initiation: "user_asked"` on Terry's
"Start Tnn", `context: "fresh"`, `use_worktree: false`, `effort: "high"`, and a `background` of at
least 80 characters. T05's and T06's briefs are the models. In order:
1. **Its cards and branches**, the base commit, the turn's start time and cap, and that the cards
   are already In Progress.
2. **What to read first:** AGENTS.md, the cards, the decisions, this file's lessons, the last live
   record when the turn is live.
3. **The rulings already made for the turn**, numbered, each with its reason.
4. **The rules**, as architect.md, "Running a turn", step 7 lists them.
5. **The slips list**, every slip of the release so far from this file's lessons, for every
   subagent's tasking.
6. **What done is**, and what happens after the merge: worktrees and branches removed, watcher
   stopped, DONE with lessons.

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
| The guard's failures go through `say.warning`, which the operator's guard dispatch prints under `-Quiet`; an explicit outcome, not an empty-message sentinel, for a silent exit 1 | T03 | `agentspace-guard`'s `run` doc; #0178's card |
| The laptop's clock time has one home: `agentspace_zone::local_clock_time` and `local_clock_time_or_utc` (`<h:mm><am\|pm>`, rounded up, ` UTC` when the system gives no local time) | T03 | #120 (cb2f165); operator.md, status (679310e) |
| A lock error is the host registry library's whole text, as provision's step 2 prints it | T03 | operator.md, guard (679310e, bc473ce) |
| A story MAY change, additively, a crate no story of the turn owns when its spec needs it, named in its PR | T03 | Each card (#0177, #0179, #0181) |
| A story that breaks `main`'s existing conformance cases brings them to the spec in the same PR, never from output, and runs the whole suite its crates feed before its PR | T03 | conformance.md, The harness; #0178, #0179, #0181 |
| state.md's release order follows the two walks; keeper.md's `pid=-` and the guard's known variable names | T03 | state.md (8ba4964); keeper.md (d4f46c0, 8325a55) |
| No test starts WSL; a bash test uses Git for Windows' bash or is ignored by default | T03 | #0181's card |
| An answer missing a member fails `describe-instance-information` and so sync; send-command and the poll get the wait's tolerance; a queued `restart` is a start | T03 | operator.md, the patch wait; #0181 |
| A spec's conformance-case table rules over a card's shorter list: every row is a case | T04 | Each spec's case table; #0182 to #0184 |
| R15 holds only for a second client started within the release; a late one is SKIPPED, in #0189's timing family | T04 | driver.md's R15 row (c932ef6); #0183 |
| At most three suite or workspace runs at once on the laptop; every runner command writes its exit code to a file through a wrapper | T04 | architect.md, "Checking that it still works" |
| A story with a 30-minute deadline case merges only once that case's result is in | T04 | #126's PR comment |
| A workspace hang under load is #0191's, rerun alone before it is a finding; a story whose scope is not those crates still lands | T04 | #0191; [runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md) |
| A coverage gap a review finds outside the turn's scope is carded in Backlog and cited where conformance.md names it, never added to the next turn (#0192, #0193) | T04 | conformance.md (8a86357); each card |
| A dev lead MAY rebase a story onto `main` before its review when the review checks rows against files merged since, and names the new head | T04 | #129 |
| operator.md's rows follow what T03 left: `grow`'s refusal, `status -All`'s files and provision's sign-in line | T04 | operator.md (27643b2, 4bb86d5, 1184365) |
| `.old-` files are removed by pattern, printing counts only; README's install as a whole is the allowed set of writes in `bin` | T05 | [runs/e0.08-t05-s01.md](runs/e0.08-t05-s01.md), step 1 |
| With the guard live, a check takes each project's lease before the first deploy or sync starts its host; a guard stop of a host with no live lease is designed, of one under a live lease a finding | T05 | T05's record, rulings |
| A step that needs the keeper off for longer than a minute disables its task, with Terry's yes each time | T05 | #0186's card; architect.md, step 5 |
| A horizon sent is shown by the host's `/run/agentspace/lease-horizon` and `ssm list-commands`' guard entry | T05 | operator.md, guard step 7; T05's record |
| #0188's "no session of the lease on the host" is no sshd login; a leftover Session Manager session is recorded against #0195 | T05 | #0188's card |
| A host's running time is counted from the observer's first running read to its first stopped read, with no gap for a restart within the minute; a split at `guard.log`'s `stopped` line MAY be given as an estimate | T06 | [runs/e0.08-t06-s02.md](runs/e0.08-t06-s02.md), instance hours |
| A stop is attributed by `ec2 describe-instances`' `StateReason.Code`: `Client.InstanceInitiatedShutdown` is the host's own power-off, `Client.UserInitiatedShutdown` an API stop | T06 | #0188's record, step 5; architect.md, live checks |
| A proxy holding a released alias alive after the release returned is a finding: driver.md has the release wait for every process it ends | T06 | #0196 |

## What is open

**Rechecked at close, 1:33pm Mountain, 2026-11-23. Every item names who acts.**

| What | Who acts | Where it goes |
|---|---|---|
| v0.09 (#0049) is laid out only on Terry's word | Terry, then the architect | [The scope ahead](#the-scope-ahead-v009) |
| #0109: the sampler dies silently when its disk fills | Before v0.09's measurement, if one is taken | roadmap.md, hardening |
| #0082: the sampler misses work a session orphans | v0.09's measurement, if its tripwire trips | roadmap.md |
| #0195: a killed client's Session Manager session outlives it about 21 minutes and holds the host's idle check off | A design call (three options on the card, one of them Terry's account setting), then a release | #0195 |
| #0196: a proxy holding a released alias outlived the release by up to 23 seconds; cause not shown | Diagnosis, then a release | #0196 |
| #0189 and #0191: workspace tests that fail on timing, or hang, under load | A hardening pass; until then each is rerun alone before it is a finding | Each card |
| #0190: the keeper conformance helpers read every process's command line, and `Stop-OwnTree` does not check its root's start time | The next release that touches `conformance/keeper/`, or a hardening pass | #0190 |
| #0192 and #0193: box test gaps (the idle check's four parts; `install.sh`'s `/etc/environment` edit) | The next release that touches `box/` | Each card |
| #0137 and #0138: fixed and merged, in Ready For Work by Terry's move; Bot cannot move them to Ready For Review. **Corrected 2026-11-23 (D72's change):** #0138 was met and went to Ready For Review; #0137 lacked `agentspace-push`'s source-scan test, and was folded into v0.10 (#0050) | Terry moves each out; the architect moves it on, on his word | Each card |
| The live kit (mask, proof, wrapper) is not in the repository | The architect, before the next live turn | `scripts\`, as process tools (D28) |
| A spec for what "time to a prompt" measures when the keeper starts a lease's holder first | The next release that specifies the lifecycle | This file's live-check lessons |
| #0194: "Docs: localswim's launcher replaced its Launch and provisioning blocks", made on 2026-11-23 by someone other than this architect | Read it before acting; not part of any v0.08 turn | #0194 |

## The scope ahead: v0.09

**v0.09 (#0049): size classes.** The size a lease asks for sets its host's CPU and memory: `s`,
`m`, `l` and `xl` each name a CPU and memory pair, and a project's host is resized to the size
asked for while it is stopped. Done when `create --size l` for a project whose host is stopped
gets a bigger host, the lease naming only the size (D1); a size that cannot be met is
unsatisfiable with `unsupported_size` (D5); and what happens when a lease asks for a different size
while the host runs is decided and checked (card #0049). It changes no contract version.

**Its stories MUST NOT be laid out until Terry says so.**

**Before its stories are laid out, the architect SHOULD settle, each as a numbered decision:**
- **Whether the footprint is measured again, or v0.06-T01's record is enough.** #0049's entry
  requirement (Terry, 2026-11-18; D33) is a footprint measured again on a turn that compiles, with
  two or more stories in parallel, before any size is set. v0.05-T01's record covers 7 of its 31
  minutes (#0109). **v0.06-T01's record, [runs/e0.06-t02-s08.md](runs/e0.06-t02-s08.md), covers
  the whole turn with no gap**, but holds no transcripts, so it gives no per-agent figure and no
  measured count of agents at once, and [footprint.md](footprint.md) does not yet take it in. Read
  it first: it may meet the requirement for the host-level numbers, and a new run is then needed
  only for the per-agent split. A new run MUST meet #0109 first, and its sampler and disk MUST be
  watched (AGENTS.md, "Watching a measurement").
- **The CPU and memory behind each size,** from the footprint, on the `m9g` family D39 sizes by
  hand today (`xlarge` 4 vCPUs to `16xlarge` 64), and which sizes each platform supports.
- **What a lease asking for a different size does while the host runs:** wait, refuse, or resize
  at its next stop. A resize stops and starts the instance through the stack (D39, D40), which
  ends every session on the host, so a running host's other leases are the hard case.
- **Where the size lives:** in the host registry, the stack parameters (`InstanceType`), or both,
  and how `deploy` and the resize agree, since D39 says the size changes only through the stack.
- **What D39 becomes.** It is "MVP only; replaced by v0.09"; its working rule in AGENTS.md
  ("Sizing the box before it runs agents") is retired or rewritten with the release.
- **How the guard and the idle check meet a resize,** since a resize is a stop and start that no
  lease's end explains.
- **Live checks with Terry's yes for their instance hours** (D19), as D62 did for v0.08, and a
  measured turn's hours if one is run.
- **Terry's decisions,** to bring to him: the instance hours, anything that costs money per size,
  and #0195's account setting if v0.09 takes it in.

**The road after v0.09 is roadmap.md's:** v0.10 plan-sized leases and contract v1, v0.11 plan from
the board, v0.12 both new platforms, then v1.0.

## When Terry approved v0.08

**Done, 2026-11-23, by 1:33pm Mountain**, after a board event showed #0048 and its stories in
Completed. Kept as the record of this close; the procedure is
[architect.md](architect.md#closing-a-release).
1. `v0.08` removed from `scripts/milestones.json` (5e4cf09), then `swimcoach stop` at 1:33pm, and
   the reporter stopped with no milestone left.
2. v0.08 marked done in roadmap.md's table, #0043 recorded as OBE by D58 and #0064 as met.
3. The six Backlog cards v0.08 met moved out by Terry, to Ready For Review by the architect on his
   "Yes, move them", and approved by him into Completed.
4. This file made final, and AGENTS.md's "Where it stands" pointed at it.
5. Waiting for Terry's word before laying out v0.09.

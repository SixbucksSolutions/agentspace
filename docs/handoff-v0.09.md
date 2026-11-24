# Handoff: agentspace after v0.09

RFC 2119 keywords; the capitals are load-bearing.

> **Final, as of 12:57pm Mountain on 2026-11-24.** v0.09 is closed: all fourteen of its pull
> requests (#133 to #146) are merged to `main`, and Terry approved #0049 and its thirteen stories
> into Completed. Started as the live record of v0.09 at T01's close, 4:31pm Mountain on
> 2026-11-23, and refreshed at each turn's close. Where this file and AGENTS.md, the board or
> decisions.md disagree, they win, and the disagreement is a bug to fix in this file.

**Written for the architect who starts v0.10 (#0050), after Terry `/clear`s this one.** It assumes
you have read nothing yet. **It is the only handoff you need to read**: what the earlier ones hold
that is still in use is carried here, and each rule is cited at its home rather than restated.

## The short version

- **v0.09 built size classes, and they work live.** A lease's `--size` of `s`, `m`, `l` or `xl`
  sets its project's host's instance type, `m9g.large` to `m9g.4xlarge`, and a host is resized only
  while no ready lease holds it. T05 ran every D70 check on agentspace's host and the box with no
  finding, in 0.534 of Terry's 4 instance hours ([runs/e0.09-t05-s01.md](runs/e0.09-t05-s01.md)).
- **A host is now sized by its lease**, by Terry's per-agent rule (AGENTS.md, "Sizing a host before
  it runs agents"; D69). D39's hand sizing is replaced.
- **`main` is green**: 3,378 conformance cases passed, 0 failed, the deadline case 5 of 5, and the
  crates 5,139 passed, 0 failed ([runs/e0.09-t04-s03.md](runs/e0.09-t04-s03.md)).
- **Nothing is running.** Both hosts are stopped, no lease is live, no milestone exists, the
  reporter and the board watcher are stopped, and every dev lead is archived.
- **v0.09 is installed on Terry's laptop**, and its keeper runs the new build.
- **Next is v0.10 (#0050), plan-sized leases and contract v1, and it MUST NOT be laid out until
  Terry says so.** Its prep is [The scope ahead](#the-scope-ahead-v010).
- **The live kit is in the repository now**, `scripts\live\`. Three mask gaps and a lost observer
  script are the architect's before the next live turn ([What is open](#what-is-open)).

## Start here

**Do these in order before anything else.** Each comes from a gap that cost time in a release.

1. **Read, in this order:** this file; [AGENTS.md](../AGENTS.md), every standing rule;
   [architect.md](architect.md), how a release is run, from Terry's word to Completed;
   [decisions.md](decisions.md), D64 to D72 with their change notes, then D14, D15, D27, D48 and
   D66 for v0.10's design; then [roadmap.md](roadmap.md), card #0050, whose description carries
   five folded-in cards whole (D72), and [footprint.md](footprint.md).
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
5. **Check AWS and the hosts, printing nothing that identifies them.** The sign-in by exit code
   alone: `aws sts get-caller-identity --output text *> $null; $LASTEXITCODE`. A non-zero code
   means Terry runs `aws login --remote` and pastes its link into a fresh Incognito window, every
   other Incognito window closed (AGENTS.md). It had expired at T05's start. Then run
   `scripts\live\mask-proof.ps1`, and only when it exits 0, `status -All` through
   `scripts\live\mask.ps1` ([The architect's tools](#the-architects-tools)). Both hosts SHOULD read
   `stopped`.
6. **Wait for Terry's word before laying out v0.10** (AGENTS.md, "The board"). Until then, do the
   prep in [The scope ahead: v0.10](#the-scope-ahead-v010), and bring him the calls that are his.

## What changed from the v0.08 handoff, and why

- **This file stands alone.** v0.09's live record leaned on v0.08's handoff for its tools, lane IDs,
  pitfalls and rules, so a cleared architect had two files to read and one to keep current. The
  parts still in use are carried here, updated; v0.08's handoff is history.
- **"What is open" accounts for every row of v0.08's**, each with what became of it. A v0.08 row,
  the live kit, was dropped from v0.09's live record and found again only by T05's sweep, after two
  live turns had each rebuilt the kit by hand.
- **The mask is a file, not a code block.** v0.08's handoff carried a mask to copy; it is now
  `scripts\live\` with its own proof, and this file only points at it.
- **"Numbers to plan by" is new**: the turn lengths, run times and resize times v0.10's caps and
  staffing will be planned from, in one place instead of across five records.
- **Kept from v0.08's:** the short version first, "Start here", lessons grouped by what they are
  about, "Rules that are easy to break" with each rule's home, and the dev lead's brief as a
  template.

## Where things stand

**As of v0.09's close, 12:57pm Mountain, 2026-11-24.** Every time in this file is the laptop's zone,
Mountain, when it was written.

| What | State |
|---|---|
| v0.09 (#0049), size classes | Completed: the epic and its thirteen stories (#0197 to #0208, #0210), approved by Terry on 2026-11-24; #0209, the defect #0210 met, earlier. Marked done in [roadmap.md](roadmap.md) |
| v0.10 (#0050), plan-sized leases and contract v1 | In Ready For Work, not laid out. It MUST NOT be broken into stories until Terry says so. Its description carries #0189, #0191, #0195's decision, #0196 and #0137's last part (D72) |
| `main`'s tests | T04's record, at df5aa9c and 1a7d33b: 84 conformance file runs, 3,378 passed, 0 failed, 0 skipped; the deadline case 5 of 5; the crates 5,139 passed, 0 failed. T05 changed no crate and no conformance file, only `scripts\live\` and a record. Under load, #0189's timing tests can fail and #0191's targets can hang |
| The install on Terry's laptop | v0.09 from c5e1184's code, a release build, installed by T05 at 9:13am on 2026-11-24: eight files copied, none renamed aside, no `.old-` file left. The keeper restarted on the new build; its task was not disabled. No change to his ssh config, task definition or `PATH` |
| The hosts | Both `stopped`, `m9g.xlarge`, `Leases    0 live` (the architect's `status -All`, 10:19am). The box (`agentbox`, swimcoach's): 20 GiB. `agentspace-host-agentspace`: 8 GiB. Both registry records carry `instance_type` `m9g.xlarge` |
| Instance hours | Q11's 4 for T05: 0.534 used (agentspace's host 9:20am to 9:49am, the box 9:29am to 9:32am). The yes was for T05 only. The box also ran about two minutes on 2026-11-23 for D64's transcript fetch (Q12) |
| Leases | 40 recorded, none `ready` or `pending` (T05's last `lease list`) |
| The board | Every v0.09 card in Completed. No defect or hardening card is open in Backlog: D72 folded each into the epic that carries it |
| Swimcoach | No milestone; `scripts/milestones.json` is empty (13bc53e), `v0.09` was stopped at 12:54pm, and the reporter is stopped |
| Sessions | Every dev lead archived; `git worktree list` shows `main` alone. The board watcher is stopped, at Terry's word, for the `/clear` |
| Terry's laptop, otherwise | Unchanged by v0.09 beyond the install |

### v0.09 by turn

| Turn | Stories | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|
| T01, the specs and the sweep | #0197 to #0199 | 4:30pm, 2026-11-23 | #133 to #135 | 2:18 of 3 h | lease-api.md's four sizes and reasons; driver.md's map, placing by size and the resizing run; state.md's `instance_type` and its locks; operator.md's deploy and provision by size; the registry's schema and samples; conformance's unsupported size moved to `xxl` |
| T02, the foundations | #0200 to #0202 | About 6:20pm, 2026-11-23 | #136 to #138 | About 1:34 of 2.5 h | `agentspace-driver-ec2`'s `SIZES`, `instance_type_for`, `meets`, `resize_target` and reasons; `agentspace-hosts`' `instance_type`, `read_for_placing`, `set_instance_type` and `set_instance` (P38); the operator's `-InstanceType` for every host |
| T03, the programs | #0203, #0204, #0210 | 12:12am, 2026-11-24 | #139 (the architect's pin), #140 to #142 | 3:34 of 4.5 h (extended from 3.5 h on Terry's word) | The EC2 driver placing by D66's rows and `place_pending`'s refusals; deploy recording the type, provision resizing under P26's locks, every person's deploy taking `provision.lock`; the isolation guard building with the real environment (#0209's fix) |
| T04, conformance | #0205 to #0207 | 3:56am, 2026-11-24 | #143 to #145 | 3:40 of 3 h (an extension asked, not answered) | The sizes' and resizes' conformance files, P56's helper fix and the whole-suite record |
| T05, the install and the live checks | #0208 | 10:18am, 2026-11-24 | #146 | 1:14 of 3 h | v0.09 installed; D70's checks passed on agentspace's host and the box with no finding; the live kit under `scripts\live\` |

## What v0.09 built

| Turn | Story | Card | PR | What |
|---|---|---|---|---|
| T01 | E0.09-T01-S01 | #0197 | #134 | lease-api.md and driver.md: the four sizes, the EC2 driver's map and placing by size |
| T01 | E0.09-T01-S02 | #0198 | #135 | state.md, operator.md and README: the registry's instance type, deploy and provision by size |
| T01 | E0.09-T01-S03 | #0199 | #133 | Conformance: the unsupported size moves to `xxl` |
| T02 | E0.09-T02-S01 | #0200 | #138 | `agentspace-hosts`: the registry's `instance_type` |
| T02 | E0.09-T02-S02 | #0201 | #136 | `agentspace-driver-ec2`: the sizes, the map, `meets`, `resize_target` and the reasons |
| T02 | E0.09-T02-S03 | #0202 | #137 | The operator's `-InstanceType` for every host, and both programs' help texts |
| T03 | (architect's pin) | | #139 | `place_pending`'s result carries every unplaced lease, in order |
| T03 | E0.09-T03-S01 | #0203 | #140 | The EC2 driver places by size (D66), and `place_pending` meets the resize (D68) |
| T03 | E0.09-T03-S02 | #0204 | #142 | Deploy records the instance type, provision resizes, the box is sized like every host |
| T03 | E0.09-T03-S03 | #0210 | #141 | The isolation guard reports what it finds, and #0209's cause is fixed |
| T04 | E0.09-T04-S01 | #0205 | #143 | Conformance: the lease CLI's sizes, placing and `no_capacity` |
| T04 | E0.09-T04-S02 | #0206 | #144 | Conformance: the operator's deploy, provision and the registry's type |
| T04 | E0.09-T04-S03 | #0207 | #145 | The whole suite, the crates and the box tests against main's build, with conformance.md ([runs/e0.09-t04-s03.md](runs/e0.09-t04-s03.md)) |
| T05 | E0.09-T05-S01 | #0208 | #146 | The install, D70's live checks, and the live kit ([runs/e0.09-t05-s01.md](runs/e0.09-t05-s01.md)) |

**The design is D64 to D71**, and D72 placed every open defect and hardening card before v1.0
into the epic that carries it. **D64** ruled that v0.06-T01's whole-turn record meets #0049's
re-measurement, with the per-agent split taken from that turn's fetched transcripts
([footprint.md](footprint.md)).

## Numbers to plan by

**Measured in v0.09, for v0.10's caps and staffing.** Each comes from a record or a command, cited.

| What | Figure | Source |
|---|---|---|
| A turn's length against its cap | T01 2:18 of 3 h; T02 about 1:34 of 2.5 h; T03 3:34 of 4.5 h; T04 3:40 of 3 h; T05 1:14 of 3 h. The release about 12:20 of 16 h | This file, "v0.09 by turn" |
| The operator conformance suite | About 30 minutes in two lanes; `test-unmet-host-deploy-size.ps1` alone about 11 minutes a pass (644 s in the architect's T04 merge check) | architect.md, "A turn that builds" |
| The whole suite and the crates | 84 file runs, 3,378 cases; the crates 5,139 tests | runs/e0.09-t04-s03.md |
| Heavy runs at once on the laptop | Three, the isolation guard's child suite included | architect.md, "A turn that builds" |
| A resize up, `m` to `l` | 176.8 s from `create` to `ready` | runs/e0.09-t05-s01.md |
| A resize down, `l` to `m` | 257.6 s, about 36 s of it D48's 5-minute rule before a `get` started the run | runs/e0.09-t05-s01.md |
| A run that changes no type (the box's first lease) | 140.2 s | runs/e0.09-t05-s01.md |
| The guard's stop after the last release | 19 min 27 s, inside D56's 25 minutes | runs/e0.09-t05-s01.md |
| Per-agent load, v0.06-T01 | 15 planned agents peaked at 4.10 cores, a quarter of the 16 vCPUs Terry's rule gave them | footprint.md; D69 |
| Live-turn cost | 0.534 instance hours for T05's whole check list | runs/e0.09-t05-s01.md |

## Terry's orders from v0.09

**Each binds where its row says; this table only finds them.**

| Order | Given | Where it binds |
|---|---|---|
| Yes to Q11 to Q13: the four sizes at their prices, 4 instance hours for T05 at `l` or smaller, the transcript fetch, his per-agent rule kept through v0.09 | 2026-11-23 | decisions.md, Q11 to Q13; D65, D69, D70 |
| "Lay out v0.09", with a staffing plan and the most subagents per turn reported to him | 2026-11-23 | D71; each story's card |
| "Start T01" to "Start T05", each turn started on his word | 2026-11-23 and 24 | This file |
| #0209 carried into T03 as #0210 ("Whatever you recommend is fine"); T03's cap extended to 4.5 hours ("Extend it") | 2026-11-23 | D71's changes |
| The open defect and hardening cards placed before v1.0, folded into their epics and archived ("Apply it"; "archive cards that are captured in epic cards") | 2026-11-23 | D72; roadmap.md |
| #0137's last part folded into v0.10; #0138, #0027 and #0194 approved into Completed | 2026-11-23 | D72 |
| #0209 moved to Ready For Review ("move 209 to RFR") | 2026-11-24 | #0209 |
| AWS's sign-in renewed at T05's start ("Signed in") | 2026-11-24 | runs/e0.09-t05-s01.md |
| The board watcher stopped for the `/clear`, to be restarted after it | 2026-11-24 | [Start here](#start-here), step 4 |

## Rules that are easy to break

**Each was broken at least once in v0.08 or v0.09, most often first. Each has its home, which wins.**

1. **A time, a SHA, a count or an interval MUST come from a command's output, never ahead of it or
   from memory.** Broken in v0.09 by a dev lead's status time and its hand counts, and by notes
   handed to a record writer (architect.md, "Writing in the repository").
2. **Nothing of Terry's MAY reach a transcript**: not his user name, profile path in either form,
   computer name, a lease ID or alias, or an AWS identifier. Broken in v0.09 by the architect's
   `%TEMP%` path in a git error, which held the profile's 8.3 short name (AGENTS.md, masking).
   Scratch lives inside the checkout's `target\`.
3. **Read the source before ruling or citing.** Broken by the architect three times in v0.09: a
   ruling from a grep about which tests read lease-api.md, P43 cited to state.md when only a crate's
   code states it, and a script change placed inside P55's diff when it was an ancestor of the base
   (architect.md, "Running a turn", step 2).
4. **A merge check runs every suite the merged code feeds**, never a selection by touched files.
   Broken in T03; T04's whole-suite run found the red file (architect.md, "A turn that builds").
5. **Every checker is proven to fail on a planted value before its pass is believed.**
   `run-conformance-files.ps1` exited 0 whatever its files did until d1d6a87; a dot-sourced scan
   passed every file in v0.08 (Terry's global rule; architect.md).
6. **Every open row of the last handoff is carried into the next.** Broken at v0.09's start: the
   live kit's row was dropped (architect.md, "Closing a release").
7. **The architect is the only agent that moves a card, and a card leaves Backlog only on Terry's
   word** (AGENTS.md, "Roles and pull requests"). Auto mode refused a guess in v0.09 until he said
   "Yes, move them".
8. **PowerShell names are case-insensitive**: `$b` overwrote `$B`, and a helper whose name matches
   an alias runs the alias (AGENTS.md, "The board").
9. **A test on a by-design red list is checked by its failing assertion**, never by its name: a
   second failure hid behind the first through every T02 review (#0209; architect.md).
10. **`stop` goes to swimcoach only on Terry's word for a release; a turn's milestone stops at its
    dev lead's archive** (AGENTS.md, "Status on Terry's phone").

## Lessons from v0.09, by what they are about

**Each lesson says what to do and where it is now written. Every lesson of the release is here.**

### Terry's details and real files

| What happened | What to do | Where |
|---|---|---|
| The architect printed a `%TEMP%` path holding the profile's short name | Scratch inside the checkout's `target\`; the mask covers the short form | AGENTS.md; `scripts\live\mask.ps1` |
| T03: link.exe started `vctip.exe` under a test's moved home, which wrote a WinINet cache there (#0209) | Never run a toolchain program under a moved `USERPROFILE` or `HOME`; keep `TMP` and `TEMP` set | AGENTS.md, "Terry's real files in a review" |

### Facts, times and counts

| What happened | What to do | Where |
|---|---|---|
| A dev lead gave a commit count, a minute count and a status time by hand | Counts and times from a command | Every brief |
| T05: notes handed to the record writer claimed an overlap no command had checked | Notes for a writer are facts, from commands | architect.md, live checks |
| Python turned `\a` and `\v` in a path into control characters; a `sed` rewrote CRLF working copies | Raw strings for paths; check line endings and control characters after a scripted edit | Every brief |
| A `$` expanded in a commit message | `git commit -F` | Every brief |

### Rulings and diagnosis

| What happened | What to do | Where |
|---|---|---|
| T01: 37 pins arrived in waves while integrators ran ahead; each spec story took two extra passes | Send the inventories' pins at once; hold integrators until those are ruled | architect.md, "A turn that writes specs" |
| T01: subagents' own pin numbers collided with the dev lead's | Subagents label choices `N1`, `N2`; only the dev lead numbers pins | architect.md |
| T01: two races and a read and write split surfaced in review | A drafter adding a writer walks every reader against every lock first | architect.md |
| The architect ruled from a grep, cited the wrong doc (P43) and misplaced a change in a diff (P55) | Read the source and check ancestry with `git merge-base --is-ancestor` before stating either | This file, rule 3 |
| T03: #0209 looked like a load race and needed the opposite, a stale build and no `vctip.exe` running | Name a reproduction's conditions; rewrite a done-when once the cause is known | architect.md, "A turn that builds" |
| T02: the driver's create reads the registry once and writes from that value | Place from `read_for_placing`; start every write from a fresh `read` | Card #0203 |

### Running a turn and its subagents

| What happened | What to do | Where |
|---|---|---|
| T01: two architect fixes to driver.md mid-review cost a second rebase | Send a fix on `main` to the dev lead before its next rebase | architect.md |
| T02: two subagents shared a run file; a file was split into code and test lines | Run-file prefixes per role; one owner per file | architect.md, "A turn that builds" |
| T04: runners backgrounded long commands and woke in the dev lead's worktree | Wait on the run's own `.exit` file in a bounded foreground loop, in the same run | architect.md |
| T04: a planted copy kept a `test-` name, which a whole-suite run picks up | Name plants `planted-*` | architect.md |
| T05: AWS's sign-in had expired at the turn's start | Split the runner: an installer needing no AWS call runs while the sign-in waits | architect.md, live checks |
| T05: the runner met two pitfalls the v0.08 handoff lists | Quote the pitfall lines in the brief, not a pointer | architect.md, live checks |

### Builds, tests and load

| What happened | What to do | Where |
|---|---|---|
| T02: a ruling's red-test count was wrong | Take every red list from a `--no-fail-fast` run | architect.md |
| T02: a by-design red test hid a second, real failure | Check each by-design red by its failing assertion | architect.md; #0209 |
| T03: two stories independent by files were dependent by a test | Check the last merges on a local merge of every open head; the brief names who owns another crate's test lines | architect.md |
| T03: the operator suite's runs drove the turn past its cap | Plan it at about 30 minutes in two lanes, with a final run | architect.md |
| T03: a planted source file restored by `Copy-Item` kept its old time, and cargo reused the planted build | Restore with git, or touch after a copy | architect.md |
| T04: the architect's T03 merge check ran a selection of files and missed a red one | A merge check runs every suite the merged code feeds | architect.md |
| T04: `run-conformance-files.ps1` exited 0 whatever its files did | Fixed in d1d6a87 | `scripts/run-conformance-files.ps1` |
| T04: runs whose inputs a later merge cannot change started early on a diff-checked base, saving about 45 minutes | Start them early, and record the diff | architect.md |
| keeper-win's `spec_stop` and other timing tests failed under load and passed alone | Rerun alone before it is a finding | #0189, in #0050 |

### Live checks on real hosts

| What happened | What to do | Where |
|---|---|---|
| The live kit was lost with a worktree in v0.08-T05 and T06 | It is `scripts\live\` now; change it with a proof case per shape | architect.md, live checks |
| T05: a once-a-minute observer cannot see a resize's stop and start | Show "no stop during a resize" by `guard.log` and `provision.log` | architect.md, live checks |
| T05: `StateReason.Code` reads `Client.UserInitiatedShutdown` for the guard's stop and `down`'s alike | Attribute a guard stop by its own `stopped` line | architect.md, live checks |
| T05: the dot-sourced mask set the caller's strict mode | Prove a dot-sourced kit file leaves the caller's state unchanged | `scripts\live\mask.ps1` |
| T05: `guard.log` held 165 lines before the install | Take a log's baseline as a count at the turn's start, never "absent" | architect.md, live checks |
| T05: a lease created within 5 minutes of the last run's attempt started no run until a `get` (D48) | Expected today; a design call for v0.10 | [The scope ahead](#the-scope-ahead-v010) |

## Pitfalls of the laptop's tooling

**None is agentspace's design; each cost a retry in v0.07 to v0.09.**

| Pitfall | What works |
|---|---|
| The Monitor tool runs bash, which drops a Windows path's backslashes | `'C:/Projects/claude-localswim-watcher/watch.ps1'` |
| `%USERPROFILE%` does not expand in PowerShell, Terry's shell | `& "$env:USERPROFILE\.agentspace\bin\agentspace.exe" ...` in anything sent to him |
| `lease list` prints `{"leases": [...]}`, and its documents have no `ssh` field | Read `.leases`; the alias is `agentspace-<project>-<first 8 hex digits of id>` (lease-api.md) |
| PowerShell's `-like` reads a backtick as an escape | Compare with `-eq` or `.Contains()` when the text holds a backtick |
| `ConvertFrom-Json` turns ISO 8601 strings into local `DateTime`s | `-DateKind String`; and never rewrite `milestones.json` from a parse |
| `Start-Process pwsh -Command` splits a command line not given as one quoted argument | Pass it as one quoted argument, and check its output file appears |
| A Python string in a bash heredoc turns `\r`, `\a` or `\v` in a path into a control character | Raw strings; grep the file for control characters after a scripted edit |
| A here-string piped to `git commit -F -` reaches git as an argument | Write the message to a scratch file |
| `pwsh -File script.ps1 -Files a,b` passes the list as one string | Pass one comma-separated string and split it in the script |
| A relative `-Out` given to `run-conformance-files.ps1` doubles its path | Pass absolute paths |
| A shell command that reads stdin with nothing redirected hangs until its timeout | Give every command its input, or none |
| A review worktree under the scratchpad's long path fails on box/tests' fixtures | Put review worktrees under `.claude\worktrees\` |
| A worktree folder stays held by the session that was in it | Remove the empty folder after that session is archived |
| `archive_session` is refused once by a hook that relabels the call, and refused while the dev lead is mid-turn | Resend with exactly the input the refusal quotes; retry after its turn ends |
| Bot's card moves out of Backlog answer HTTP 403, and auto mode refuses them without Terry's word | Ask Terry; move it on his word |
| `agentspace-operator status` prints on stderr | Capture `2>&1` |
| `localswim` on `PATH` is stale | `C:\Projects\localswim\localswim.exe`, by full path, as Bot |

## The architect's tools

| Tool | What it does |
|---|---|
| `scripts/scan-identifiers.ps1 -Head refs/review/pr<n> -Base origin/main -Pr <n>` | The identifier scan of a PR's diff, commits and body, each pattern proven on a planted value first; prints counts, never values |
| `scripts/run-conformance-files.ps1 -Checkout <worktree> -Out <absolute folder> -Files '<a>,<b>' [-Dist <folder>]` | Runs conformance files against a build's programs; ends `Failed: none` and exit 0, or the failed files and exit 1 |
| `scripts/watchdog.ps1 -Lead <name>` and `-Reset` | The 10-minute watchdog on a dev lead; reset it on every message and turn note, and restart it with `-Reset` first |
| `scripts/report-milestones.ps1 -Watch` | The swimcoach reporter, whenever a milestone exists |
| `scripts/milestones.json` | The milestones, edited as text only, with the real time |
| `C:\Projects\swimcoach\bin\swimcoach.cmd stop --project agentspace --milestone <name>` | Stops a milestone, after it is out of `milestones.json` |
| `scripts\live\mask-proof.ps1` | Proves the mask (262 cases) and the scan; prints only counts and `True`/`False` for the real environment. Run it first, every live session |
| `scripts\live\mask.ps1` | Dot-sourced: `ConvertTo-LiveMasked -Text <text>`, the 18 rules plus the 8.3 short form |
| `scripts\live\wrap.ps1 -Out <folder> -Name <nnn-what> -Command '<PowerShell>'` | Runs one command with its streams to files, `.exit` written last, and a masked copy to print |
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
| `card get <board> <n>` | A card by bare ticket number (`208`, not `#0208`); JSON with `lane_id`, `revision`, `description` |
| `card move <board> <n> <lane ID>` | A move; only the architect's |
| `card create --description <text> <board> <lane> <title>` | A new card; then find its ticket with `card search --search <words> <board>` |
| `card description --revision <r> <board> <n> <file>` | Replaces a description from a file; read the revision with `card get` first |
| `relationship create <board> <source> <kind> <target>` | `blocks`, `references` or `relates_to` |

**A PR review, in commands** (architect.md, "Reviewing a pull request", gives the why):
1. `git fetch origin pull/<n>/head:refs/review/pr<n>`, and check the SHA against the dev lead's.
2. When the PR's checks run programs: `git worktree add --detach .claude\worktrees\rv<n> refs/review/pr<n>`,
   `cargo xtask dist` in it, and every suite the merged code feeds. A record and process-tool PR
   needs no build; run its kit's proof and its scan instead.
3. `scripts/scan-identifiers.ps1`, then look at every hit; check
   `git log --format='%h %an %ae %cn %ce' origin/main..refs/review/pr<n>`.
4. `gh pr merge <n> --squash --match-head-commit <sha>`, and only once it succeeds, delete the
   branch, move the card and tell the dev lead; remove the review worktree and ref last.

**A dev lead's brief, as a template.** `start_session` with `initiation: "user_asked"` on Terry's
"Start Tnn", `context: "fresh"`, `use_worktree: false`, `effort: "high"`, a short `name`, and a
`background` of at least 80 characters. In order:
1. **Its cards and branch**, the base commit, the turn's start time and cap, that the cards are
   already In Progress, and its watcher reader (`agentspace-vX.YY-tnn`).
2. **What to read first:** AGENTS.md, the cards, the decisions, this file's lessons, and the last
   live record when the turn is live.
3. **The rulings already made for the turn**, numbered, each with its reason: the sweep's findings,
   whose files the story owns, and any gate that waits on Terry.
4. **The rules**, as architect.md, "Running a turn", step 7 lists them.
5. **The slips list**, every slip of the release so far, **with the pitfall lines quoted**, for
   every subagent's tasking.
6. **What done is**: the PR's head sent and nothing pushed after; after the merge, worktrees and
   branches removed, watcher stopped, and DONE with lessons.

## Rulings made during v0.09

**T01's dev lead raised pins P1 to P37, and later turns P38 to P56. Each is cited in the spec it
ruled, as "architect, <date>, for v0.09-Tnn, pin Pn".** The ones that changed a decision are change
notes in decisions.md; this table only finds them.

| Ruling | Where it binds |
|---|---|
| P1: `place_pending` refuses a lease its host does not meet while a ready lease holds the host, with `no_capacity` | D68's change; driver.md, place_pending |
| P2, P20: a host with no recorded type answers `pending` and runs, never `no_capacity`; while a project has a placeable `pending` lease, every new lease of it is `pending` | D66's change; driver.md |
| P2c, P31: a missing `hosts.json` implies the box at `m9g.xlarge`; a written seed record has no type | D66's change; state.md, The host registry |
| P11, P30: a deploy that passes a type removes the record's first; a failed read-back removes it | D67's change; state.md, The instance type |
| P15: a program before v0.09 refuses a `hosts.json` holding `instance_type` | D67's change; README, Install it |
| P18: `resize_target` reads only the EC2 driver's `pending` records with a size in `SIZES` | D66's change; driver.md |
| P26: a run's deploy step takes the lease state lock then `hosts.lock`, removes the type, reads the leases, and only then calls AWS | D68's change; driver.md; operator.md, provision step 4 |
| P34, P36: every `deploy` that is not a run's takes the host's `provision.lock`, and refuses while a run holds it | D67's change; operator.md, deploy |
| P38: `agentspace-hosts`' `read_for_placing` | Card #0203; the crate |
| The #139 pin: `PendingPlaced{placed, unplaced}`, every unplaced lease in order | D71's change |
| P48: the isolation guard builds with the real environment and runs the test executables under the moved home | #0210; AGENTS.md |
| P51 to P53: three lows from #0210's review | #0050's description |
| P55: runs whose inputs a later merge cannot change start early on a diff-checked base | runs/e0.09-t04-s03.md |
| P56: `host-proxy-common.ps1`'s fix for a red operator file | #0207; runs/e0.09-t04-s03.md |
| T05's rulings 1 to 10: the install as v0.08-T05's, the sign-in and host gates, the kit under `scripts\live\`, the box step, the 3.5-hour stop | runs/e0.09-t05-s01.md, "The architect's rulings" |
| The rest of P3 to P56 | The specs and cards, each where it is cited |

## What is open

**Rechecked at close, 12:57pm Mountain, 2026-11-24. Every item names who acts.**

| What | Who acts | Where it goes |
|---|---|---|
| v0.10 (#0050) is laid out only on Terry's word | Terry, then the architect | [The scope ahead](#the-scope-ahead-v010) |
| The live kit's mask gaps: AWS access key IDs (`AKIA`, `ASIA`), account IDs written with hyphens, and `--token value` with no `=` or `:`. None was seen in T05 | The architect, before the next live turn, with a proof case each | `scripts\live\mask.ps1`, `mask-proof.ps1` |
| T05's observer script (`observe-once.ps1`: one read a call, edges and hours computed, a stop file) was lost with its worktree | The architect, before the next live turn, rebuilt from runs/e0.09-t05-s01.md | `scripts\live\` |
| #0189, #0191, #0196, #0195's decision, #0137's last part, P51 to P53, `Invoke-HostFile`'s holders | v0.10 | #0050's description (D72) |
| #0192, #0193, and #0195's build if it is code | v0.12 | #0051 (D72) |
| #0190, #0082 and #0109 | v1.0's first turn, before the real project measures its own turn | #0053 (D72) |
| A spec for what "time to a prompt" measures when the keeper starts a lease's holder first | The next release that specifies the lifecycle; not yet placed | v0.08's live records |

**What became of v0.08's open rows:** v0.09 laid out on Terry's word (done); #0109 and #0082 placed
in v1.0's first turn (D72); #0195, #0196, #0189 and #0191 placed in v0.10 (D72); #0190, #0192 and
#0193 placed in v1.0 and v0.12 (D72); #0137's last part placed in v0.10 and #0138 Completed (D72);
the live kit committed by T05 (#146), with the gaps above; the "time to a prompt" spec carried
above; #0194 met by the architect's state.md fix and Completed.

## The scope ahead: v0.10

**v0.10 (#0050): plan-sized leases and contract v1.** A project hands agentspace its execution and
staffing plan, and gets a host big enough for the plan's busiest turn (D27): `create --plan <file>`,
whose peak concurrency times the per-agent footprint picks the smallest size that fits; a new
unsatisfiable reason for a plan no size can hold; and contract v1 in lease-api.md and `schemas/`,
since the lease gains a field saying what it was sized for. Done when two plans of different peak
concurrency get different sizes, each lease says what it was sized for, and a plan too big for any
size is unsatisfiable with its reason (card #0050).

**Its stories MUST NOT be laid out until Terry says so.**

**Before its stories are laid out, the architect SHOULD settle, each as a numbered decision:**
- **The per-agent footprint the product multiplies by.** v0.06-T01's 15 agents peaked at 4.10
  cores (footprint.md, D64), a quarter of Terry's one vCPU per agent (D69). Whether v0.10 sizes by
  the measured figure, by his rule, or by the measured figure with headroom is a cost-against-
  headroom call, and **Terry's** (Q13 kept his rule only through v0.09).
- **The plan document's shape**, from D14's execution plan and D15's staffing as the board's cards
  carry them, and whether model and effort are recorded only, as the card says they MUST NOT change
  the size.
- **Contract v1's change**, the first to any output (roadmap.md): what a v0 client sees, and how
  lease-api.md and `schemas/` version it.
- **The folded-in work's placement** (#0050's description): #0189, #0191, #0137's last part, P51 to
  P53 and `Invoke-HostFile`'s holders as one hardening story in the first build turn; #0196 with
  the driver work; #0195's fix chosen in the design prep, whose account-setting option is
  **Terry's**.
- **Whether D48's 5-minute rule SHOULD start a resize at once** when the last run ended cleanly: it
  added about 36 seconds to T05's resize down.
- **Live checks and their instance hours**, Terry's yes for each (D19), if v0.10's done-when needs a
  real host; T05's 0.534 hours for a full check list is the figure to plan from.

**The road after v0.10 is roadmap.md's:** v0.11 plan from the board, v0.12 both new platforms, then
v1.0.

## When Terry approved v0.09

**Done, 2026-11-24, by 12:57pm Mountain**, after a board event showed #0049 and #0205 to #0208 in
Completed. Kept as the record of this close; the procedure is
[architect.md](architect.md#closing-a-release).
1. `v0.09` removed from `scripts/milestones.json` (13bc53e), then `swimcoach stop` at 12:54pm, and
   the reporter stopped with no milestone left.
2. v0.09 marked done in roadmap.md's table.
3. AGENTS.md's sizing section rewritten for sizing by lease (b8b92f8), and D39 marked replaced.
4. This file made final, and AGENTS.md's "Where it stands" pointed at it.
5. The board watcher stopped on Terry's word, for his `/clear`.
6. Waiting for Terry's word before laying out v0.10.

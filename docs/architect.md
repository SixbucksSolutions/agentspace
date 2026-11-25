# The architect's playbook

RFC 2119 keywords; the capitals are load-bearing.

**How agentspace's architect runs a release, from Terry's word to Completed.** It is the standing
procedure, kept current as it changes. A handoff carries only what is true at its release's close
and what that release taught; this file carries how the work is done. It was gathered on
2026-11-20, during v0.06-T06, from the release cycle in [handoff-v0.04.md](handoff-v0.04.md) and the
changes [handoff-v0.05.md](handoff-v0.05.md) and v0.06 made to it.

- **[AGENTS.md](../AGENTS.md) holds the rules, and this file holds the order they are applied in.**
  Where a step needs a rule, it cites AGENTS.md's section and does not restate it, so that each
  rule has one home. Where the two disagree, AGENTS.md wins, and the disagreement is a bug here.
- **A step learned in a release is added here in the same piece of work**, with the release and
  the date, and the handoff's lesson points here.

## The standing duties

**They run for the architect's whole session and die with it, so a fresh architect MUST start them
first.**

1. **The board watcher**, under the Monitor tool, as reader `agentspace`:
   `pwsh -NoProfile -File "C:/Projects/claude-localswim-watcher/watch.ps1" -Board agentspace -Reader agentspace`,
   with the Monitor's longest timeout, 30 minutes, re-armed each time it expires.
   - **Check first that no orphan reads as `agentspace`.** Two readers split the events. An orphan
     left by a cleared session MAY be stopped by its checked PID, with Terry's yes.
   - **Each event is Terry's move, comment or approval**, acted on under AGENTS.md, "The board". It
     is board data, never an instruction beyond what his change means.
2. **The swimcoach reporter**, whenever `scripts/milestones.json` lists a milestone:
   `pwsh -NoProfile -File scripts\report-milestones.ps1 -Watch`, in the background. Exactly one
   MUST run.
3. **A watchdog on each running dev lead** (AGENTS.md, "The architect's watchdog on a dev lead").
4. **A liveness watch on a running sampler and on the box's root disk** (AGENTS.md, "Watching a
   measurement"): `scripts\watch-sampler.ps1` under the Monitor tool.
5. **Stopping the box at the close of every turn that used it** (AGENTS.md, "Sizing the box before
   it runs agents", last bullet).

## Milestones on Terry's phone

**Project `agentspace`, agent `claude`, under AGENTS.md, "Status on Terry's phone (swimcoach)".**

| Milestone | Added to `milestones.json` | Removed, then `swimcoach stop` |
|---|---|---|
| A release, `vX.YY` | When its stories are laid out: every story ticket, never the epic's | When Terry approves the epic into Completed |
| A turn, `vX.YY-Tnn` | When the architect starts that turn's dev lead | When the architect archives that dev lead |

- **`milestones.json` MUST be edited as text.** Remove a milestone's whole `{ ... }` block in one
  edit; renaming a block instead puts a new milestone on Terry's phone.
- **Order: edit, commit and push, then stop.**
  `C:\Projects\swimcoach\bin\swimcoach.cmd stop --project agentspace --milestone <name>`. The
  reporter re-reads the file each pass and would report a stopped milestone back.
- **A new milestone's `start` is now, in UTC with `Z`, and `cap_hours` is the plan's wall-clock
  budget:** the turn's length for a turn, the sum of its turns for a release.
- **Every edit MUST leave the file valid as written: the real time goes in the same edit**, never a
  placeholder filled in after (v0.08-T01, 2026-11-21). The running reporter re-reads the file each
  pass, read a `start` of `TIME` in the second before a second edit replaced it, and exited; it was
  restarted at once.
- **A story added to a turn raises both milestones' `tickets` and `cap_hours`** (v0.06-T05, when
  #0140 was added).
- **A turn's extension raises its release's cap by the same hours** (Terry, 2026-11-22, when
  v0.08-T03 went from 3 hours to 4.5 and v0.08 from 18 to 19.5), since the release's cap is the sum
  of its turns'.
- **A cap is extended only on Terry's word.** He MAY delegate one ("if there's good progress I'm
  happy to delegate an extension to you", 2026-11-19, for v0.06-T04). A delegation covers that
  milestone only, and the extension is reported to him with the progress that justified it.

## Breaking an epic into stories

1. **Only on Terry's word**, and only once every card of an epic that `blocks` it is in Completed
   (AGENTS.md, "The board").
2. **Settle what the stories will need before laying them out.** The handoff's "scope ahead" names
   the design calls; each is recorded as a numbered decision before the stories cite it.
3. **Lay out every turn up front**, each story with its execution and staffing plan (D29, D31), and
   re-plan when a turn hits unknowns.
4. **Each story's card carries:** the gap; what is in it; a pinned interface wherever two stories
   or subagents meet; a done-when with a live or conformance check, not only tests; the rules it
   touches; an estimate under 3 hours; its turn and dependencies; its staffing plan (every
   subagent with `Model/effort`, tasking, owned files and start); and its branch
   `vX.YY/ex.yy-tnn-smm`. Mix models to the work: Sonnet for inventories, tests and most
   implementing and running, Opus for writing that decides behavior, for finding a cause, and for
   every review. **Use as many subagents per story as can be used effectively** (Terry,
   2026-11-21, D63): where several hands write one file, drafters write their sections into draft
   files they own under the worktree's `target\drafts\`, and one integrator owns the file.
5. **A done-when that a stand-in cannot show names its live check** (v0.06-T05). Read
   [conformance.md](conformance.md)'s coverage table: every behavior it marks "None" that the
   release changes MUST be either given a conformance case or named as a live check on a story.
   v0.06's `deploy` had no case, and hung on its first real run.
6. **Board links** (AGENTS.md, "The board"): each story a child of its epic; each dependency a
   `blocks`; a defect carried into a story a `references`. The epic's description carries the plan
   by turn, with each turn's peak.
7. **Add the release milestone**, commit and push.

## Running a turn

1. **Check the standing duties**, and that the turn's gate is met: every PR of the turn before is
   merged.
2. **Sweep the cards against the tree before the dev lead starts** (v0.06-T06). Check every file,
   command and list a card names against `main` (`git ls-files`, the specs), and rule on each gap
   in its owning doc first. T06's sweep found four (a process tool the card's keep-list missed, a
   crate's test stand-in the done-when would have failed on, the interop check with no peer left,
   and the one `box/system` edit), and the dev lead started with all four settled.
   - **A ruling about what a check or a program does MUST come from reading it, not from its name
     or a grep hit** (v0.07-T01). The sweep ruled that `test-schemas.ps1` would check the new state
     schemas' samples; it reads only the top-level `schemas/`, and the dev lead found it by
     planting a sample. The ruling was corrected on the card.
3. **If the turn uses a host:** give the turn's lease the `--size` its peak needs (AGENTS.md,
   "Sizing a host before it runs agents"; D69), and plan the size-down at the turn's close: the
   next lease at the default brings it back. Before v0.09 a host kept its size until a deploy
   changed it, so v0.06-T01's `m9g.4xlarge` stayed until T05.
4. **If the turn is measured:** start the sampler before the dev lead, confirm it records the dev
   lead's session before giving the go, and arm the liveness watch at once.
5. **If the turn changes Terry's laptop** (his ssh config, his task, his `PATH`): ask him once,
   before the dev lead starts, naming each change with its reversal (D41). Record his yes on the
   card, and give it to the dev lead in its brief.
   - **A step that needs the keeper off for more than a minute needs its task disabled**, since the
     task's one-minute trigger starts a new keeper (v0.08-T05). Disabling it is a change to his task,
     so it is asked for here, with the turn, not mid-turn.
6. **Add the turn's milestone**, commit and push, and **move every story of the turn to In
   Progress** (AGENTS.md, "The board"; Terry, 2026-11-22). The milestone's clock runs only while
   its cards are In Progress, so a turn whose cards still sit in Ready For Work is not counted.
7. **Start the dev lead** with `start_session`: Opus/high, fresh context, `use_worktree: false`. Its
   brief MUST name its cards and branches, its watcher reader (`agentspace-vX.YY-tnn`), the
   rulings already made for it, and these rules: read AGENTS.md first; enter a story's worktree
   itself before launching its subagents; subagents finish in one run; one PR per story, never
   merged by itself, and no push after its head is sent; it moves no card, and asks the architect
   for any move, Needs Terry included (AGENTS.md, "Roles and pull requests"); questions go to the
   architect, never to Terry; times and SHAs come only from a command's
   output; the identifier scan before every push; `C:\Projects\localswim\localswim.exe` by full
   path; a status line at least every 10 minutes.
8. **Start the watchdog**: `scripts\watchdog.ps1 -Lead <name> -Reset`, then
   `scripts\watchdog.ps1 -Lead <name>` in the background. Reset it on every message and turn note
   from the dev lead.
9. **Move the epic to In Progress** when its first story starts.
10. **Answer as Terry's proxy** (AGENTS.md, "The architect is Terry's proxy"). Make the design call
    when it is yours, write it in the doc that owns the fact as "architect, <date>, for <story>",
    and send it to the dev lead. Card every new defect yourself. A dev lead that states a deviation
    with its reason, rather than asking, is answered only if the architect disagrees.
11. **A defect found mid-turn that blocks the next turn becomes a story of this turn**, on Terry's
    word: a card under the epic, `references` to the defect, `blocks` to the next turn's stories,
    a line in the epic's plan and in the layout decision, and both milestones raised (v0.06-T05,
    #0140).

**While a question to Terry is open, the architect's turn is held, and the dev leads' messages
queue** (v0.06-T05: an 80-minute wait). When his answer comes, read each linked session's state
and every open PR before acting on the answer. A message that arrives late describes the moment it
was sent.

## A turn that writes specs

**Learned in v0.07-T01, where four specs were written in parallel against one set of pinned
names.**
- **The dev lead keeps a pins file of the facts two or more specs share** that the layout decision
  leaves open, and sends each to the architect. A writer states a pin, or its own choice, marked
  "architect's to confirm" until the architect rules.
- **The architect rules on each pin as it arrives, and records in the decision it touches any
  ruling that changes one** (D44, D48, D55 in T01).
- **Every confirmed mark becomes the ruling's citation before the merge**, in one commit the
  architect asks for after the review, so no published spec carries an open mark. The architect
  diffs that commit against the reviewed head before it merges.
- **Writers that start before the pins are ruled need a follow-up pass.** In T01 every spec needed
  one, because rulings reached a mid-turn dev lead late: a message to a busy session waits in its
  queue. A dev lead MAY hold a writer until the pins it depends on are ruled, when that is cheaper
  than a second pass.
- **The inventories' pins go to the architect at once, and the integrators wait for their rulings**
  (v0.09-T01, 2026-11-23). T01's inventories surfaced most pins within minutes, but drafters and
  integrators ran ahead of the rulings, and 37 pins arriving in waves cost each spec story a
  follow-up pass, a fix pass and a re-review. Drafters MAY start at once; an integrator SHOULD
  start only once the pins its story raised are ruled.
- **Only the dev lead numbers pins** (v0.09-T01). Subagents that numbered their own choices
  collided with the dev lead's P-numbers; a subagent labels its open choices `N1`, `N2` and so on,
  and the dev lead gives each its pin number when it sends it.
- **A drafter that adds a writer of a shared file walks every reader of that file against every
  lock before it drafts** (v0.09-T01). Two races (P26, a lease placed `ready` between a run's read
  and its resize; P34, a person's deploy writing a type back during a run) and one read and write
  split (P31) were found in review, each costing a pass. The walk would have found them first.
- **A ruling about which tests a spec change turns red MUST come from reading those tests, and
  every red list from `cargo test --workspace --locked --no-fail-fast`** (v0.09-T01). The architect
  ruled from an `include_str!` grep that no lease CLI test reads lease-api.md; `tests/help.rs` reads
  it at run time, and five tests went red. Plain `cargo test` stops at the first failing binary, so
  its red list is short.
- **A doc fix the architect makes on `main` during a spec turn is sent to the dev lead before its
  next rebase** (v0.09-T01). Two fixes to driver.md, made while S02 was in review, cost S02 a second
  rebase that one message ahead of the first would have saved.
- **The dev lead SHOULD propose the layout's open shared facts as pins in its first minutes**,
  before any inventory reports (v0.10-T01, 2026-11-24). T01's dev lead sent P1 to P9 at launch, so
  the schema story finalized early and the page's integrator started with every pin ruled.
- **A ruling that changes a pin after launch costs a fix pass in each story it touches**, since a
  running subagent is never messaged (AGENTS.md). A spec turn's layout SHOULD plan one fix pass
  per story, not count it as rework (v0.10-T01: P3 and P21 each cost one).
- **A brief that restates a ruled order states it as an ordered list of its own**, never as the
  order of a list of messages (v0.10-T01): a reviewer read P12's messages, listed in message
  order, as the order of the checks, and raised a false blocking finding.
- **The layout names the merge order of a spec turn whose stories link to or copy each other's
  files**, and each later story rebases on the merge before it (v0.10-T01: the schemas, then
  lease-api.md, which links the plan schema, then state.md, whose schema copies its `$defs`).
- **Every copy of shared schema `$defs` has a check that it equals the original**, proven on a
  planted difference (v0.10-T01): a "MUST re-sync" with no check is broken by the next change.
- **Sample IDs are patterned so they read as fake at a glance** (`0a1b2c3d-...`,
  `i-0123456789abcdef0`), or made by a committed script (v0.10-T01). The identifier scan counts a
  sample UUID as a hit and cannot tell a placeholder from a real value; two hand-typed IDs that
  looked random were replaced, and the branch squashed before its first push.
- **A reviewer starts its workspace run first and reads the diff while it runs** (v0.10-T01): three
  workspace runs at once took 23 to 60 minutes each and were the turn's long pole.
- **A spec that links into another story's new sections merges after it**, and the architect
  checks every anchor resolves on `main` before each such merge, and every link between the specs
  once all are in (1,954 in T01).

## A turn that builds

**Learned in v0.09-T02, where three stories built libraries in parallel and turned `main`'s 26
deliberately red tests green.** A dev lead's brief carries these.
- **Each file has one owner.** A file MUST NOT be split between subagents by its code lines and
  its test lines; the owner writes both.
- **Every subagent that shares a worktree prefixes its own run files** (`target\runs\<role>-...`).
  Two subagents wrote the same `target\runs\start.txt` in T02.
- **The red tests a turn must turn green are listed by name, from a `--no-fail-fast` run**, never
  from a count in a ruling: T01's ruling said ten operator help tests in one file, and there were
  fourteen.
- **A mutation run counts every crate that reads the mutated file.** Two schema mutations a crate's
  own tests let through were killed by `agentspace-state`'s schema tests.
- **A test on a turn's by-design red list MUST be checked to fail for its stated reason, by its
  failing assertion, in every review**, never by its name alone (v0.09-T02). `isolation_guard` was
  on the list because its child's help tests failed; a second failure of the same test, a home it
  found created, sat behind that one through every T02 review and showed only once the help tests
  were green (#0209).
- **One workspace run per story's reviewer** kept three parallel stories inside the laptop's limit
  of three heavy runs, and **a reviewer MAY commit small fixes itself**, which saved a round trip
  in every story.

**Learned in v0.09-T03, where three stories built the programs and one diagnosed a defect.**
- **Stories independent by files can be dependent by tests.** #0203's branch alone turned one of
  #0204's tests red, because only #0204 records the type #0203 reads. A turn's last merges are
  checked on a local, unpushed merge of every open head, and a PR that needs another names the
  order.
- **Who owns another crate's test lines that a story turns red is written in the brief before the
  turn**, not ruled mid-turn as P39 and P44 were: the story whose change turns a test red owns its
  fixture lines, with every line listed in its PR.
- **A brief for a defect's reproduction names the conditions it needs, never "under load" alone.**
  #0209 looked like a load race and was the opposite: it needed a stale build and no `vctip.exe`
  running, which load made rarer. Once a cause is known, the done-when is rewritten to reach it, as
  #0210's was.
- **A loaded run records its load**, by the sibling run's log times beside it, never by assuming
  one ran.
- **Before every launch the dev lead counts the tasking's heavy runs against the laptop's three**,
  the isolation guard's own child suite included.
- **The operator conformance suite takes about 30 minutes in two lanes**, and a story that changes
  it plans a final whole-suite run after its fixes. T03 ran past its cap on it and was extended.
- **A planted source file is restored with git, or touched after a copy**: `Copy-Item` keeps the
  backup's older time, and cargo then reuses the planted build.

**Learned in v0.09-T04, the conformance turn.**
- **A merge check runs every suite the merged code feeds, never a selection by touched files.** The
  architect's T03 merge check ran only the operator files that load the two helpers T03 changed. A
  third helper's file had been red since T03, because #0204's whole-suite run predated #0203's
  place, and #0203 ran no operator file. T04's whole-suite run found it (P56).
- **A runner's tasking makes it wait for its own `.exit` file in a bounded foreground loop, in the
  same run.** The tool moves a command past about 3 minutes to the background whatever the tasking
  says, and a runner that ends its run then wakes in the dev lead's current worktree. With that line
  in each tasking, every runner finished in one run.
- **A planted copy of a case is named `planted-*`, never `test-*`**, or a whole-suite run picks it up.
- **Runs whose inputs a later merge cannot change MAY start early on a diff-checked base** (P55): the
  crates and every conformance file outside the merging story's folder ran on 1a7d33b, saving about
  45 minutes, and the record shows the diff. A record written in two parts, with placeholders the
  late runs fill, keeps writing off the critical path.
- **The operator suite's `test-unmet-host-deploy-size.ps1` takes about 11 minutes a pass.** A story
  that runs it in its writer, its runner and its reviewer pays it three times, and its turn's cap
  counts each.

**Learned in v0.10-T02, the foundations and hardening turn.**
- **A run wrapper sets the error mode its runs need, 0x8001, and logs it.** A process started by
  `Start-Process -WindowStyle Hidden` has error mode 0, and so do its children: every test that
  starts a file named `.exe` that is not a program then raised a modal "Unsupported 16-Bit
  Application" dialog on Terry's desktop and waited for a person. At least 17 appeared, by the
  subagents' reports, before the wrapper was fixed, and they were #0191's hangs (#0225).
- **Heavy runs are gated mechanically, and the gate queues fairly.** Three named mutexes behind
  one run wrapper held the laptop to three heavy runs while 13 subagents worked; until a shared
  queue was added, one-slot runs starved a two-slot load window for 40 minutes.
- **A ruling reaches a running subagent only after it ends.** One investigator shipped a fix the
  architect had ruled out, and another loaded the laptop with a CPU burner after the ruling that
  load comes from builds and suites only. A tasking names its forbidden methods at launch, and a
  turn that investigates plans a fresh fixer after its investigators.
- **Open PRs that touch shared start paths are merge-checked together**: one local merge of all of
  them onto `main`, every suite against its build, and each squash merge checked to leave `main`'s
  tree equal to the tested one.
- **A reviewer MAY run beside its load runner**, with its own `CARGO_TARGET_DIR` and a bounded wait
  for the run record, so a turn near its cap does not serialize them.
- **Facts handed to a record writer are checked against the repository first**: a base SHA taken
  from a dev lead's notes was wrong in a record until review.
- **The editing tools decode `\uXXXX` escapes on the way into a file**, so a test or doc that MUST
  hold one is checked byte for byte after the edit: three escape tests held none.

## Live checks on Terry's laptop and the box

**A live check runs on Terry's real setup, so it reads and writes only what its card names**
(AGENTS.md, "Terry's real files in a review"; "A live run on Terry's real setup MUST mask what it
reads before it prints it").

- **Mask before printing**, with the mask shown to catch a planted value of every shape first.
- **Capture every exit code to a file.** A command that runs longer than about 3 minutes is moved
  to the background by a subagent's shell tool, and its handle and exit code can be lost (v0.06-T05,
  the first no-op deploy). A wrapper script runs the command with its streams to files and writes
  the exit code and end time last; the runner reads the code once the process is gone.
- **A failure is a finding, not something to retry past.** Diagnosis runs MAY follow, each
  disclosed with its times.
- **A host's stop is attributed by `ec2 describe-instances`' `StateReason.Code`** (v0.08-T06), one
  read-only call that starts nothing: `Client.InstanceInitiatedShutdown` is the host's own
  power-off, `Client.UserInitiatedShutdown` an API stop. `StateTransitionReason`'s text does not
  tell them apart, and the idle check's power-off leaves no `guard.log` line.
- **A subagent's shell moves a foreground command of about 3 minutes or more to the background**
  (v0.08-T06), whatever its brief says, so a runner SHOULD wait on its wrapper's exit file rather
  than on the command.
- **The box ends stopped**, confirmed with `status`, and no lease the check made is left ready.

**Learned in v0.09-T05, the install and the size checks.**
- **The live kit is `scripts\live\`**: `mask.ps1`, `mask-proof.ps1`, `wrap.ps1` and `scan.ps1`, each
  run from the checkout, never rebuilt from a record. A change to it MUST keep the proof passing and
  add a proof case for each new shape.
- **A gate that waits on Terry splits the runner**: an installer that needs no AWS call runs while
  the sign-in waits, and the checks runner starts once it is good.
- **A once-a-minute observer cannot see a resize's stop and start**, and `StateReason.Code` reads
  `Client.UserInitiatedShutdown` for the guard's stop and `down`'s alike. "No stop during a resize"
  is shown by `guard.log` and `provision.log`; a guard stop is attributed by its own `stopped` line.
- **A live brief quotes the pitfall lines it depends on**, not a pointer to them: T05's runner met
  `lease list`'s `leases` wrapper and `-like` reading a backtick as an escape, both listed in the
  v0.08 handoff.
- **Notes handed to a record writer are facts, and come from commands.** A dev lead's note claimed an
  overlap of two subagents that no command had checked; the reviewer found it against the wrappers'
  times.
- **A `guard.log` baseline is a count taken at the turn's start**, never "absent": the laptop's log
  had 165 lines before T05's install.

## Reviewing a pull request

**Ready For Review is the architect's claim that the work is fully good and in `main`** (AGENTS.md,
"The board"). So every review MUST be the architect's own.

1. **Fetch and pin the head the dev lead sent:** `git fetch origin pull/<n>/head:refs/review/pr<n>`,
   and check its SHA against the message.
2. **Read the diff**, and every doc that carries a decision. A spec defines behavior from outside
   (D34).
3. **Run the touched checks yourself, on Windows**, in full, not only the dev lead's report. Where
   the card claims a test fails without the change, show it fail: v0.06-T05-S02's conformance case
   was run against a build from before the fix and hung, then against the fix and passed. A run
   cut short by a timeout has not passed.
   - **A docs-only pull request is not exempt: run `cargo test --workspace --locked` on its head
     whenever a crate's tests read a doc it changes** (v0.07-T01). The operator crate's help tests
     compare its help with operator.md's help blocks byte for byte. T01 changed those blocks for
     commands T03 builds, its PRs were merged without the workspace tests, and `main` went red in
     about a dozen help tests until #0151 (found by T02's dev lead). A spec that runs ahead of its
     code SHOULD say which tests it breaks, and the story that builds the code fixes them.
4. **Prove a harness before calling a defect.** In v0.05 a no-stdout run first "showed" exit 0,
   because the harness gave the child a hidden console; with no console the build was right.
   **Before a conformance run against a build, check that every `AGENTSPACE_*_CMD` names a file
   that exists** (v0.07-T02): `cargo xtask dist` writes to the checkout's own `target\dist`, not
   under `CARGO_TARGET_DIR`, and a variable naming a missing file failed #99's push file 0 of 1
   until it named the right folder, where it passed 20 of 20.
5. **After a rebase,** check the new head with `git range-diff` and a diff of the story's own files
   against the head reviewed.
   - **A PR based before another merge that touched its files is tested merged onto `main`'s tip**,
     in the review worktree (`git merge --no-edit origin/main`, never pushed), since that is what
     the squash merge makes (v0.09-T02: #138 and #136 both edited `driver-ec2`'s `host.rs`).
     `scripts/run-conformance-files.ps1`'s `-Checkout` and `-Out` take absolute paths: a relative
     `-Out` is read against the checkout and doubles.
6. **The identifier scan** over the diff, every commit message and the PR body, with each pattern
   first shown to match a planted value: an instance ID, a run of 12 digits with UUIDs masked, the
   private network's addresses and host names, `vpc-`, `subnet-`, `vol-`, `sg-`, `arn:aws`, a token
   assignment, a key's text, a UUID, `$env:COMPUTERNAME` and `$env:USERNAME`. An example value from
   AWS's documentation (`i-0123456789abcdef0` and its reverse), an `EXAMPLE` key, or a value of all
   zeros (`i-00000000000000000`) is not a hit; the PR names each so the scan's hits read as expected
   (v0.07-T02).
7. **Check every commit's author and committer** carry `main`'s identity.
8. **Merge exactly the reviewed commit, and only then the rest, in one command that stops at the
   first failure** (AGENTS.md, "Roles and pull requests"):
   `gh pr merge <n> --squash --match-head-commit <sha>`, then confirm `MERGED`, then
   `git push origin --delete <branch>`, then move the card to Ready For Review, then tell the dev
   lead.

## Closing a turn

1. **The dev lead removes its worktrees and branches, stops its watcher and timer, and replies
   DONE.**
2. **The architect stops its watchdog and archives it.** `archive_session` is refused while the
   dev lead's turn is still running, and MAY be refused once by a hook that relabels the call; it
   succeeds when sent again with the input the refusal quotes.
3. **Remove any worktree folder it left**, once it is empty and the session is archived.
4. **Remove the turn's milestone, push, and `swimcoach stop`.**
5. **If the turn used the box:** fetch what it left, release the lease, `down`, then `status`.
6. **Refresh the handoff's live state** with the turn's close: its time, its PRs, and what is now
   true.

## Closing a release

- **The epic follows its last story to Ready For Review**, moved by the architect.
- **Cards only Terry can move are listed for him.** Bot cannot move a card out of Backlog (HTTP
  403), so a defect met by a release, or one Terry made OBE, stays there until he moves it. The
  release's handoff lists each with its evidence.
- **Every open row of the last handoff MUST be carried into the next, or say what became of it**
  (v0.09: the live kit's row was dropped from v0.09's live record at its start, and only T05's
  sweep found it again). A final handoff accounts for each of the previous one's rows.
- **When Terry approves the epic into Completed,** in order: remove the release milestone, push,
  `swimcoach stop`; mark the release done in [roadmap.md](roadmap.md)'s table with what it
  produced; stop the reporter if no milestone is left; finalize the handoff; point AGENTS.md's
  "Where it stands" at it; and wait for Terry's word before laying out the next release.

## Writing in the repository

- **The architect edits docs with the Edit tool, or from a single-quoted string.** A double-quoted
  PowerShell string with `\a` or `\v` in a path wrote a bell and a vertical tab into a handoff on
  2026-11-19, and it went unseen until the next edit (fixed in 689ccac). Before a docs commit:
  `git grep -nP "[\x00-\x08\x0B\x0C\x0E-\x1F]" -- "*.md"` MUST find nothing.
- **The architect's own rulings and card comments carry a time only when it was read from the
  clock** (v0.07-T01). The architect stamped its rulings to the dev lead "3:20pm" to "4:05pm"
  without reading one; git put them at 3:18pm to 3:38pm, and the dev lead's clock read 3:42pm
  after two of them. Three card comments were corrected; none reached the repository.
- **Times and SHAs come only from a command's output.** A dev lead's guessed "2:01am" was 1:58am
  by the clock (v0.06-T05).

## Checking that it still works

**Run from the main checkout. Every conformance check runs its programs in a throwaway home.**

- **At most three suite or workspace runs SHOULD share the laptop at once** (v0.08-T02 and T04). More
  make #0189's timing tests fail and #0191's targets hang, and each such failure then has to be rerun
  alone before it counts.
- **Every command a runner runs, short or long, MUST write its exit code to a file through a wrapper**
  (v0.08-T04), as live checks do ("Live checks on Terry's laptop and the box"), and the runner MUST
  wait for it in the foreground. A record cannot claim a result whose exit code was not kept.

| Check | Run with | Last result |
|---|---|---|
| The conformance suite, against the installed programs | Each `conformance\*\test-*.ps1` with no `AGENTSPACE_*` variable set, so each program is the install folder's (conformance.md, Naming the program under test) | 44 files, 1,443 passed, none failed or skipped, 7:41am to 7:55am, 2026-11-20 ([runs/e0.06-t06-s01.md](runs/e0.06-t06-s01.md)) |
| The deadline case | `conformance\operator\test-unmet-start-paths.ps1 -Case deadline`, about 15 minutes | 5 passed in 905 seconds, the same day |
| The crates | `cargo test --workspace --locked`, `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, on Windows | 2,883 passed, 50 ignored helpers; fmt and clippy clean, the same day |
| A build to test that is not installed | The same files with `AGENTSPACE_CMD`, `AGENTSPACE_OPERATOR_CMD`, `AGENTSPACE_SSH_PROXY_CMD` and `AGENTSPACE_KEEPER_CMD` naming `target\dist`'s programs, or `scripts\run-conformance-files.ps1` | 72 files and the deadline case, 2,640 passed, none failed or skipped; the crates 4,849 passed, 58 ignored, run alone; the box tests 172 OK in WSL; 2026-11-22 ([runs/e0.08-t04-s04.md](runs/e0.08-t04-s04.md)) |
| The box tools | `python3 -I -m unittest discover -s box/tests -p 'test_agentspace_*.py'`, on the box | handoff-v0.04.md, "Checking that it still works" |

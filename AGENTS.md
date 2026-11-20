# AGENTS.md

RFC 2119 keywords; the capitals are load-bearing.

Guidance for any agent (Claude, Codex, ...) working in this repo. Read this first, then the
[README](README.md) for how the box works and its rules.

## What this is

agentspace is Terry's compute for coding agents, and the agent working here owns it. Today it is
one Graviton EC2 instance ([docs/how-it-was-built.md](docs/how-it-was-built.md)). It is becoming a
lease API: projects ask for compute and get a way to SSH in, without knowing what hosts it.

- **Where it stands:** [docs/handoff-v0.06.md](docs/handoff-v0.06.md), the architect's handoff
  after v0.06, for the architect who starts v0.07. It is final, and v0.06 is closed (2026-11-20).
  An agent picking agentspace up SHOULD read it first.
- **How a release is run:** [docs/architect.md](docs/architect.md), the architect's playbook, kept
  current. It replaces the release cycles in the v0.04 and v0.05 handoffs, which stay, with
  [docs/handoff-v0.02.md](docs/handoff-v0.02.md) and [docs/handoff-mvp.md](docs/handoff-mvp.md),
  for their pieces and lessons.
- **The plan:** [docs/roadmap.md](docs/roadmap.md): the MVP was v0.01, and releases v0.02 to v1.0
  are one Backlog epic each (D26). It goes from there to someday-maybe ideas. The Backlog MUST
  match the roadmap at all times (Terry, 2026-11-17).
- **The language:** agentspace's product code becomes Rust by v0.06, and all new product code MUST
  be Rust from v0.07 on (D28). The PowerShell was the MVP prototype's, and the rewrite MUST be
  built from specs, never from it (D20). **The prototype was removed from the repository by
  E0.06-T06-S01 (2026-11-20)**, `bin\` and its files in `scripts\` included. It remains in git
  history, in every commit before that story's, and its code there MUST NOT be read (D20). The
  PowerShell that remains is the conformance suite, the architect's process tools in `scripts\`
  and a crate's test stand-in, none of them product (D28, D42). The programs are the installed Rust ones, in
  `%USERPROFILE%\.agentspace\bin` ([README, Build and Install it](README.md#build)).
- **The reasoning:** [docs/decisions.md](docs/decisions.md). A design choice MUST be recorded
  there, numbered, before or with the change that makes it, and a changed decision MUST say what
  replaced it rather than being deleted.

## Git: the owning agent has unlimited authority

**Terry, 2026-11-16: the agent owns this repo, with unlimited pre-authorization.** Committing,
pushing to `main`, rebasing, force pushing and deleting branches need no permission.

- **Work SHOULD be committed and pushed when it is done**, not left in the working tree for
  Terry to ask about.
- **Before a force push, the reply to Terry MUST give the remote SHA being replaced**, so a
  rollback stays a one-liner.
- **This repo is public:** account IDs, account-specific parameters and tokens MUST NOT be
  committed (README, "Rules").
- **An agent committing from a lease MUST set git's identity first** (found in v0.04-T01,
  2026-11-18). A box has none, so git falls back to `ubuntu@<the host's name>`, which puts the
  instance's internal host name into the commit's author and committer, and so into public
  history. Set `user.name` and `user.email` in the checkout to the identity `main`'s commits carry,
  and check `git log --format='%an %ae %cn %ce' -1` before the first push. The first commit from
  the box was caught and re-authored before any push.

## Roles and pull requests

**Terry, 2026-11-16 (decision D16):**
- **The architect** owns the design. It reviews each pull request independently, squash merges
  it, and deletes the branch.
  - **The merge MUST name the commit that was reviewed:**
    `gh pr merge <n> --squash --match-head-commit <sha>`. Without it, a commit the dev lead
    pushes during the review is merged unread. In v0.03-T01, 2026-11-18, PR #28 merged a README
    commit pushed after the review's fetch; it was read after the merge and was sound.
  - **The branch MUST be deleted only after the merge succeeded**, in a command that stops when
    the merge fails (found in v0.05-T01, 2026-11-19). Deleting a pull request's branch closes the
    pull request. PR #50's merge was refused because the dev lead had pushed a rebase after the
    review's fetch, and the architect's next line still deleted the branch. The branch was pushed
    back from `refs/pull/50/head`, the PR reopened, and the rebase checked with `git range-diff`
    to change only `Cargo.lock` before the merge.
  - **The card's move and the "merged" message to the dev lead MUST wait for the merge's
    success**, never go out beside it (found in v0.06-T01, 2026-11-19). The architect sent the
    merge of PR #70 in parallel with #0114's move to Ready For Review and a "merged, clean up"
    message; GitHub refused the merge on a Cargo.lock conflict, so the card moved and the message
    claimed a merge that had not happened. Both were undone within a minute.
  - **The architect MUST archive a dev lead's session once every one of its pull requests is
    merged to `main`** (Terry, 2026-11-18), and then remove any worktree folder it left. An open
    session can hold its last worktree folder locked, so the folder goes after the archive.
- **A dev lead** is one Opus/high session per turn. It runs the subagents each story's
  staffing names (D15 in the MVP; in a release, the plan on the story's card, with parallel
  subagents, D31), reviews each story itself, and opens **one pull
  request per story** to `main`, from a branch named for the story (`mvp/e2.06`, or `v0.02/e0.02-t03-s01` in a release, D29). A dev lead MUST
  NOT merge its own pull request.
- **Subagents** MUST NOT move cards or open pull requests.

The git authority above belongs to the architect. A dev lead pushes only its own branch.

**Terry, 2026-11-17: the architect is pre-authorized to start the T7 and T8 dev leads** as soon as
each turn's entry gates are met, meaning every PR of the turn before is merged. **The same day he
extended it to every v0.02 turn** (v0.02-T2, v0.02-T03, v0.02-T04 and v0.02-T05): each starts as soon as its stories' entry
requirements are met, with its milestone, reporter and watchdog as for T1. The Claude app
still lets a session start another only on a turn its user began, so where the app refuses, the
architect puts the dev lead up as a card instead, and Terry starts it with one click.

**A dev lead MUST enter a story's worktree (`EnterWorktree`) itself before it launches that
story's subagents** (found in T5, 2026-11-17; made MUST in v0.03-T01, 2026-11-18). A worktree
guard pins a subagent to its parent session's current worktree, so an implementer launched from
elsewhere cannot write the story's files and has to hand back a patch for the dev lead to apply.
With two or more stories in a turn, the dev lead enters each story's worktree in turn to launch
its subagents.
- **A subagent cannot move itself to another worktree** (found in v0.03-T01, 2026-11-18). Its
  `EnterWorktree` is refused, because its writes are limited to the folder it was pinned to. Two
  S02 follow-ups launched from S01's worktree, each told to enter S02's first, both stopped with
  no writes. A tasking that says "enter the story's worktree first" does not work; only the dev
  lead's own `EnterWorktree` does.
- **A freshly launched subagent keeps the worktree it was launched in, whatever the dev lead does
  afterwards. A subagent resumed with `SendMessage` follows the dev lead's current worktree for as
  long as it runs** (found in T7, 2026-11-17, in two cases: E1.04's implementer, resumed while the
  dev lead was in E3.02's worktree, and E1.05's, resumed in its own worktree before the dev lead
  moved to E1.04's, both had their edits refused).
  - **For follow-up work, a dev lead SHOULD launch a fresh subagent rather than resume one.**
  - **A dev lead that does resume one MUST stay in that story's worktree until it finishes.**
- **Any wake of a subagent follows the dev lead's current worktree, not only a `SendMessage`
  resume** (found in v0.02-T1, 2026-11-17). 0.02.T1.1's implementer ended its run waiting on its
  own background work, and woke on that notification while the dev lead was in 0.02.T1.2's
  worktree. It was then pinned there, and its edits to T1.1's files were refused. It stopped
  cleanly, and a fresh implementer launched from T1.1's worktree finished the work.
  - **A subagent's tasking MUST say to finish in one run:** run long commands in the foreground
    and wait for them, and never end the run waiting on background work or a note file. With two
    stories in a turn, the dev lead is often in the other story's worktree when a wake comes.
  - **A subagent that ends its run anyway is finished.** Its follow-up goes to a fresh subagent
    launched from its story's worktree.

### The architect is Terry's proxy

**Terry, 2026-11-17: "you're my proxy".** Dev leads and subagents MUST NOT ask Terry directly.
- **A question meant for Terry goes to the architect.** The architect answers it when it can, from
  the docs, the decisions, or a check of its own.
- **The architect asks Terry only for what only he can do:** a check outside the Claude app, a
  click in its UI, or a call that would commit agentspace to something costly to reverse (D19).
  It relays his answer back.
- **A card waiting on Terry still goes to Needs Terry**, so it reaches his phone, but the request
  itself comes to him from the architect.
- **When the AWS sign-in has expired, the request MUST name `aws login --remote`, and say to paste
  the link it prints into a fresh Incognito window**, with every other Incognito window closed
  first (Terry, 2026-11-19: "We've had this fail before"). Plain `aws login` opens the default
  browser, whose other AWS session fails the sign-in with a 400 (README, "AWS"). The architect
  asked for plain `aws login` at the start of v0.05-T01, and Terry corrected it; the operator's
  sign-in message now names `--remote` too (operator.md, AWS failures).

### The architect's watchdog on a dev lead

**Terry, 2026-11-17.** While a dev lead is running, the architect MUST keep a 10-minute watchdog
on it:
- **Every status update from the dev lead resets the timer.** That means a message it sends, or a
  note that one of its turns ended. The reset is `scripts\watchdog.ps1 -Lead <name> -Reset`, and
  the timer itself runs in the background as `scripts\watchdog.ps1 -Lead <name>`, which exits when
  it fires.
- **When the timer fires, the architect checks whether the dev lead is busy:** mid-turn, or with
  its subagents still running.
  - **If it is busy,** the architect does nothing else, and restarts the timer with `-Reset` first. The timer counts from the last reset, so a restart without one fires at once.
  - **If it is not busy,** the architect nudges it: finish the work in one turn, or reply
    `BLOCKED:` with what it waits on. Waiting on its own subagents is not blocked.
- **If two nudges within 30 minutes are both answered `BLOCKED`,** the architect removes the
  watchdog and escalates to Terry that the dev lead is blocked, saying on what.
- **A dev lead, when nudged, MUST either finish its current work in that turn or reply
  `BLOCKED: <what it waits on>`.**

## Growing the box's disk

**Terry, 2026-11-19, standing order: the architect is authorized to grow the box's root EBS volume
as needed, up to 64 GiB, "so it stays a minimum of 60% used".**
- **The volume MUST NOT be grown past 64 GiB** without Terry's word.
- **A grow MUST NOT leave the root filesystem less than 60% used**, measured right after it: grow
  in steps sized to the need, never far ahead of it, since a volume never shrinks.
- **The architect SHOULD grow it when the root filesystem passes 80% used**, by the largest step
  the 60% floor allows, with `agentspace-operator grow -GiB <n>` (operator.md, grow). EBS
  allows one change per volume every 6 hours, so a step MUST be planned for the next 6 hours.
- **The first step**, the same day during v0.05-T01: 8 to 11 GiB at 2:41am Pacific, which left
  the filesystem at 9.7G, 5.9G used (62%). It had filled at 6.8G with five crates' builds (D38).
- **The second step, ahead of a measured turn, by Terry's word:** before v0.06-T01 the root
  filesystem was 62% used, so the 60% floor allowed no grow, and seven crates' builds would fill
  it. Terry, 2026-11-19: "you're free to resize as needed after the cleaning pass". The architect
  removed the box's old 2.1.325 Claude CLI copies, apt's cache and most of the journal (5.9G to
  5.3G used), then grew 11 to 20 GiB at 2:39pm Pacific: 19G, 5.3G used (29%). **A grow below the
  floor needs Terry's word each time**, and a cleaning pass comes first.

## Sizing the box before it runs agents

**Terry, 2026-11-19, standing order: "before launching the EC2, make sure it's an M class instance
with vCPU count >= max number of parallel agents. I want each agent to have its own vCPU and at
least 4 GB of RAM."** D39 records it.
- **Before the box is started for agent work, it MUST be an M-class instance whose vCPU count is
  at least the most agents that will work on it at once**, counted as D27 counts them: the sum of
  the turn's stories' staffing-plan peaks, plus 1 for its dev lead. An M class instance has 4 GiB
  per vCPU, so the memory rule follows from the vCPU rule.
- **The smallest such size is used**, on the box's own family (`m9g`: `xlarge` 4 vCPUs, `2xlarge`
  8, `4xlarge` 16, `8xlarge` 32, `12xlarge` 48, `16xlarge` 64). v0.05-T01's 10 subagents and its
  dev lead, 11 agents, would have needed an `m9g.4xlarge`; it ran on an `m9g.xlarge`.
- **The size is set through the stack, never by hand in EC2**: `InstanceType` set to the type in
  `%USERPROFILE%\.agentspace\stack-parameters.json` (D40; `infra\stack-parameters.example.json`
  shows its form), then `agentspace-operator deploy`, while the box is stopped. CloudFormation
  stops and starts the instance to change it, and keeps the disk (operator.md, the template's
  parameters). Until E0.06-T06-S01 (2026-11-20) this rule named `infra\local.parameters` and the
  prototype's `deploy`, which are gone.
- **Each use is sized for its own peak**, so the box goes back down when the next use needs fewer
  agents. A lease whose sessions run no agents on the box, such as a live check from the laptop,
  needs no change.
- **Until v1.0, the box MUST be stopped as soon as a turn that used it is done** (Terry,
  2026-11-19: "make sure we don't leave it burning money when done with its work for a dev
  turn"): every PR of the turn merged to `main` and its cards in Ready For Review. The architect
  fetches anything the turn left on the box (a sampler's record), releases the turn's lease, and
  runs `agentspace-operator down` at once, then confirms with `agentspace-operator status` that it
  is `stopped`. It MUST
  NOT wait for the idle stop (D11), which takes up to an hour. v0.05-T01's box was stopped at its
  close, 3:00am Pacific.

## Terry's real files in a review

**Terry, 2026-11-19: "isolation is enough".** A review or a check MUST NOT list, read or hash
Terry's real `%USERPROFILE%\.agentspace`, `~/.ssh`, CA or `%LOCALAPPDATA%\localswim-v2` as a
before-and-after proof that a run left them alone. What protects them is isolation: the
conformance harness points `USERPROFILE`, `HOME`, `LOCALAPPDATA` and `APPDATA` into a throwaway
home and refuses any write outside it (conformance.md, The harness), and a review confirms that
every write and delete went through it. This replaces the snapshot step in handoff-v0.04.md,
"Reviewing a pull request", step 3, which auto mode refused a subagent in v0.05-T02.

**When a story turns a stub into code that reaches real state, every test that runs the program
in process MUST be found and pointed at a throwaway home before any test runs** (found in
v0.05-T03, 2026-11-19). T01's `agentspace-cli` tests ran command lines in process with the real
environment, which was harmless while the commands were stubs. Once E0.05-T03-S02 built them,
two test runs before 8:28am Pacific read Terry's real `leases.json` and briefly opened his real
`leases.lock`. Nothing was written: with no driver, no record could be, and the lease records
were checked afterwards through `agentspace lease list`, 15 leases, all released, as before. The
in-process entry now takes a throwaway home, and a guard test shows the suite creates nothing in
the real one.

**A test MUST NOT print, log or format the values of an environment it captured** (found in
v0.06-T01, 2026-11-19). E0.06-T01-S04's harness recorded the whole environment a fake ssh saw and
derived `Debug` on it, so one failing assertion printed every variable into a subagent's tool
output on the box, among them the Claude session's own `CLAUDE_CODE_MESSAGING_TOKEN`. Nothing of
agentspace's was in it, and nothing was committed, pushed or carded; on the laptop the same bug
would have printed whatever the shell holds. A test asserts on the variables it names, and reports
a mismatch by name or count, never by value.

**A live run on Terry's real setup MUST mask what it reads before it prints it** (found in
v0.06-T05, 2026-11-19 and 2026-11-20). Two prints reached subagent transcripts, and nothing reached
a record, the repository or a card. A diagnostic process sampler printed an `aws` command line,
which carries the stack's network parameters and the ssh public key. A `lease list` printed lease
IDs and project names, because the mask had no rule for a UUID.
- **Process command lines are masked before they are printed**, and an `aws` or `ssh` command line
  is never printed whole.
- **The mask covers UUIDs and lease aliases** as well as the account, instance, network and host
  shapes, and is shown to catch a planted value of each before the run.

## Watching a measurement

**A running `agentspace-sample` MUST be watched for liveness by whoever started it, from its start
to its `stop`** (found in v0.05-T01, 2026-11-19; Terry's global rule: silence is not success). In
v0.05-T01 the turn's builds filled the box's root disk at 2:33am, the sampler's next write failed
and it died, and nothing said so until `stop` at 2:58am: the record covers 7 minutes of a
32-minute turn (#0109).
- **Arm a Monitor on it**, under the Monitor tool, that runs `agentspace-sample status` over the
  lease about once a minute and prints a line only when the samples stop rising or the sampler is
  not running.
- **Watch the box's root disk in the same Monitor**, and grow it under the standing order above
  before it fills, since the record is on that disk until #0109 moves it.

## Memory: git is a superset of it

**Terry, 2026-11-16, standing order: every fact an agent keeps in its private memory MUST also be
in this repo**, in this file or a pushed doc. An agent that saves a memory MUST record the same
fact here and push it in the same piece of work. A memory that the repo does not yet record is a
gap to fix, not a place to keep things.

**Why:** memory is private to one agent on one machine, while the repo is what every agent, every
machine and Terry can read.

## The board

**Work is tracked on the `agentspace` localswim board**
(`c74ddc68-9c3a-4434-8435-563c596d3229`, `http://127.0.0.1:8811/board/c74ddc68-9c3a-4434-8435-563c596d3229`).
Docs cite its cards as `#0001`.

- **Lanes, per Terry (2026-11-16):** MVP epics go in Ready For Work, and post-MVP work goes in
  Backlog. **On 2026-11-17 Terry moved the release epics on the path to 1.0 into Ready For Work**,
  so Backlog now holds what comes after 1.0, defects and hardening. An epic MUST NOT be broken
  into stories until Terry says so.
- **Every turn of an epic is laid out up front**, each with its execution and staffing plans, and
  re-planned when a turn hits unknowns (Terry, 2026-11-18; D29). A one-turn-at-a-time layout was
  tried and walked back the same day.
- **Stories** are children of their epic, and MUST each be under 3 hours of active work to reach
  Ready For Review; one that cannot be goes to Terry first. They are titled `EN.nn: title` in the
  MVP, and `EX.YY-Tnn-Smm: title` in a release (D29): `nn` is the earliest turn whose entry
  requirements the story meets, and `mm` numbers the stories within that turn, restarting at 01,
  such as `E0.02-T03-S01`. v0.02's T1 and T2 stories keep their earlier `0.02.Tm.n` names. An epic goes to In Progress as soon as work
  starts on any of its stories, and follows its last story to Ready For Review as soon as that
  story arrives there (Terry, 2026-11-17). The architect moves it both times. Its description
  carries the execution plan by turn (decision D14). Only Terry moves a card to Completed.
- **A story reaches Ready For Review only once its pull request is merged** (Terry, 2026-11-18,
  after approving a story whose PR was still open). The dev lead moves it to In Progress when work
  starts and leaves it there while its PR is open. The architect moves it to Ready For Review
  when it merges the PR. So everything in Ready For Review is merged and safe for Terry to
  approve. Terry, the same day: "rfr means architect claims fully good and in main. I delegate
  to you so I review once we're in main." Moving a card there is the architect's claim, and
  MUST NOT be made for work it has not reviewed and merged.
- **Epics with a dependency run in serial** (Terry, 2026-11-18: "let's work epics with
  dependencies in serial"). An epic that another epic `blocks` MUST NOT be broken into stories or
  started until the blocking epic is done: its stories merged and Terry's word that it is
  complete.
  - **Done means every one of the blocking epic's cards is merged and in Completed** (Terry,
    2026-11-18): "story planning goes best when we have all the data from the prior turn,
    otherwise it's heavy rework". So v0.05 is not broken into stories until v0.04's last card
    reaches Completed. Epics with no `blocks` between them MAY run at the same time, as v0.03 and v0.04 did.
  Within an epic, a turn starts only when every PR of the turn before is merged.
- **Dependencies MUST be board relationships, not only text** (Terry, 2026-11-18). Each story is a
  child of its epic. Each "Depends on" a card states is a `blocks` relationship from the card
  depended on to the one that waits, between stories and between release epics alike. A defect
  card carried into a story is linked with `references`, and a looser tie with `relates_to`
  (`localswim relationship create <board> <source> <kind> <target>`). Until then the board held
  none, and every dependency lived only in card text; 44 were added that day.
- **A session SHOULD arm Terry's board watcher at its start**, as his global instructions
  describe: `C:\Projects\claude-localswim-watcher\watch.ps1 -Board agentspace -Reader agentspace`
  under the Monitor tool. The board is already in the watcher's `config.json`.
- **localswim calls run as Bot.** Until this repo has its own wrapper, load the credential the way
  the watcher's `lib\env.ps1` does, for one run only (`Set-LocalswimEnv`, then
  `Clear-LocalswimEnv`). The token MUST NOT be printed or written anywhere.
  - **On the laptop, call `C:\Projects\localswim\localswim.exe` by its full path.** The
    `localswim` that `PATH` finds first, in `%USERPROFILE%\.local\bin`, is a stale install whose
    source folder is gone (found in v0.04-T02, 2026-11-18). `card get` takes a bare ticket number
    (`76`), not `#0076`. On the box, `localswim` is agentspace's own wrapper and is fine.
  - **A secret MUST NOT be passed as a command argument.** Pipe it on stdin or read it inside
    the command, because an error can echo its arguments. In v0.02-T04, 2026-11-17, a binding
    error echoed a `LOCALSWIM_TOKEN=` line into a subagent's tool output, which put the token in
    a session transcript.
  - **A PowerShell helper's name MUST NOT match an alias.** An alias wins over a function, so
    the call goes to the alias instead. A helper named `H` ran `Get-History` (`h`), which caused
    the leak above, and one named `Ls` ran `Get-ChildItem` (`ls`). Check a name with `Get-Alias
    <name>` first.
- **Watcher reader names MUST be unique per session.** The architect reads as `agentspace`; a dev
  lead reads as `agentspace-tN` in the MVP, and as `agentspace-vX.YY-tnn` in a release (for
  example `agentspace-v0.02-t03`; v0.02's first turn read as `agentspace-v0.02-t1`). Two sessions sharing a reader share one place in the mailbox, and
  each event reaches only one of them.

## Status on Terry's phone (swimcoach)

**agentspace reports its milestones to Terry's swimcoach** (Terry, 2026-11-17), under the rules in
`C:\Projects\swimcoach\docs\agents\swimcoach-reporting.md`, which MUST be followed as written:
- At most one report per milestone every 5 minutes.
- At least one every 10 minutes while work goes on.
- Lanes counted for that milestone's cards only.
- `stop` only when Terry says the milestone is complete.
  - **A turn's milestone lives as long as its dev lead** (Terry, 2026-11-18: "start reporting
    turns when their dev lead is created, remove turns when dev lead is archived"). The architect
    adds it when it starts the turn's dev lead, and removes it and sends `stop` when it archives
    that dev lead, which is once every pull request of the turn is merged to `main`. It does not
    wait for Terry to move the stories to Completed. A release's milestone still ends only on his
    word.

- **`scripts/report-milestones.ps1` does the reporting**, and the architect runs it with `-Watch`.
  It counts lanes from the board for each milestone in `scripts/milestones.json`, and keeps its
  last report times in `%USERPROFILE%\.agentspace\swimcoach-last.json`, so a restart never reports
  inside the 5-minute limit. `-DryRun` prints what would be sent.
- **Milestones:**
  - `MVP` covers every MVP story, across all three epics, and never an epic card (Terry,
    2026-11-17).
  - `T1` to `T8` each cover their turn's stories (D14).
  - **A release `vX.YY` covers all of its stories, never the epic, and `vX.YY-Tnn` each cover one
    of its turns (D29).** The release milestone is added when its stories are laid out, and a
    turn's milestone when that turn starts. v0.02's first two keep the names they were reported
    under, `v0.2` and `v0.2-T1`.
  - The architect edits `milestones.json` when stories are added or a turn starts.
  - **When Terry says a milestone is complete, it MUST be removed from `milestones.json` before
    `swimcoach stop` is sent.** The running reporter re-reads the file each pass, and would report
    a stopped milestone back onto his phone.
  - **`milestones.json` MUST be edited as text, never rewritten from `ConvertFrom-Json`.** Without
    `-DateKind String`, PowerShell 7 turns each `start` into a DateTime, and writing it back loses
    the ISO 8601 form and the `Z` (it happened on 2026-11-17, fixed in 8182010).
- **Project `agentspace`, agent `claude`**, spelled that way every time.
- **`active_hours` counts wall-clock time only while work is In Progress** (Terry, 2026-11-17):
  - The clock runs while at least one of the milestone's cards is in In Progress, and stops when
    none is.
  - A card waiting on Terry goes to Needs Terry, which pauses the clock for it. So does a
    milestone with nothing in progress.
  - The reporter keeps the running total in its state file, and never counts time when it was
    not running.
- **`cap_hours` is the plan's wall-clock budget:** the sum of the turn lengths for `MVP`, and the
  turn's length for a `T` milestone.
- **A story waiting on Terry himself MUST be moved to Needs Terry**, so it is counted there, the
  report carries a `needs_me` line, and its clock pauses. In Progress does not reach his phone.

## Writing

- **Every rule in a doc MUST use RFC 2119 wording**, and the doc MUST say so at the top.
- **Every line, bullet and table cell that is a phrase MUST start with a capital**, except where
  it starts with a number, an identifier, a flag or a path.
- **A time of day shown to Terry MUST be written `h:mm` plus `am` or `pm` in the laptop's time
  zone** (`12:34pm`, `5:19am`; Terry, 2026-11-17), in conversation and in program output a human
  reads. The zone MUST be read from the system when the time is printed, never assumed or
  hard-coded: Terry travels, and the laptop's zone travels with him. Machine-readable timestamps (JSON, state files, ISO 8601 that code parses) keep their
  own format.
- **Humor and casual register belong in conversation with Terry, never in commits, docs, code
  comments or program output.**

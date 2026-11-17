# AGENTS.md

RFC 2119 keywords; the capitals are load-bearing.

Guidance for any agent (Claude, Codex, ...) working in this repo. Read this first, then the
[README](README.md) for how the box works and its rules.

## What this is

agentspace is Terry's compute for coding agents, and the agent working here owns it. Today it is
one Graviton EC2 instance ([docs/how-it-was-built.md](docs/how-it-was-built.md)). It is becoming a
lease API: projects ask for compute and get a way to SSH in, without knowing what hosts it.

- **The plan:** [docs/roadmap.md](docs/roadmap.md), from the MVP to someday-maybe ideas.
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

## Roles and pull requests

**Terry, 2026-11-16 (decision D16):**
- **The architect** owns the design. It reviews each pull request independently, squash merges
  it, and deletes the branch.
- **A dev lead** is one Opus/high session per turn. It runs the implementer and reviewer
  subagents that D15 names for each story, reviews each story itself, and opens **one pull
  request per story** to `main`, from a branch named for the story (`mvp/e2.06`). A dev lead MUST
  NOT merge its own pull request.
- **Subagents** MUST NOT move cards or open pull requests.

The git authority above belongs to the architect. A dev lead pushes only its own branch.

### The architect is Terry's proxy

**Terry, 2026-11-17: "you're my proxy".** Dev leads and subagents MUST NOT ask Terry directly.
- **A question meant for Terry goes to the architect.** The architect answers it when it can, from
  the docs, the decisions, or a check of its own.
- **The architect asks Terry only for what only he can do:** a check outside the Claude app, a
  click in its UI, or a call that would commit agentspace to something costly to reverse (D19).
  It relays his answer back.
- **A card waiting on Terry still goes to Needs Terry**, so it reaches his phone, but the request
  itself comes to him from the architect.

### The architect's watchdog on a dev lead

**Terry, 2026-11-17.** While a dev lead is running, the architect MUST keep a 10-minute watchdog
on it:
- **Every status update from the dev lead resets the timer.** That means a message it sends, or a
  note that one of its turns ended. The reset is `scripts\watchdog.ps1 -Lead <name> -Reset`, and
  the timer itself runs in the background as `scripts\watchdog.ps1 -Lead <name>`, which exits when
  it fires.
- **When the timer fires, the architect checks whether the dev lead is busy:** mid-turn, or with
  its subagents still running.
  - **If it is busy,** the architect does nothing else, and restarts the timer.
  - **If it is not busy,** the architect nudges it: finish the work in one turn, or reply
    `BLOCKED:` with what it waits on. Waiting on its own subagents is not blocked.
- **If two nudges within 30 minutes are both answered `BLOCKED`,** the architect removes the
  watchdog and escalates to Terry that the dev lead is blocked, saying on what.
- **A dev lead, when nudged, MUST either finish its current work in that turn or reply
  `BLOCKED: <what it waits on>`.**

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
  Backlog. An epic MUST NOT be broken into stories until Terry says so. Stories are titled `EN.nn:
  title`, are children of their epic, and MUST each be under 3 hours of active work to reach Ready
  For Review; one that cannot be goes to Terry first. An epic is In Progress from the moment its
  stories are laid out until its last story reaches Ready For Review, and its description carries
  the execution plan by turn (decision D14). Only Terry moves a card to Completed.
- **A session SHOULD arm Terry's board watcher at its start**, as his global instructions
  describe: `C:\Projects\claude-localswim-watcher\watch.ps1 -Board agentspace -Reader agentspace`
  under the Monitor tool. The board is already in the watcher's `config.json`.
- **localswim calls run as Bot.** Until this repo has its own wrapper, load the credential the way
  the watcher's `lib\env.ps1` does, for one run only (`Set-LocalswimEnv`, then
  `Clear-LocalswimEnv`). The token MUST NOT be printed or written anywhere.
- **Watcher reader names MUST be unique per session.** The architect reads as `agentspace`; a dev
  lead reads as `agentspace-tN`. Two sessions sharing a reader share one place in the mailbox, and
  each event reaches only one of them.

## Status on Terry's phone (swimcoach)

**agentspace reports its milestones to Terry's swimcoach** (Terry, 2026-11-17), under the rules in
`C:\Projects\swimcoach\docs\agents\swimcoach-reporting.md`, which MUST be followed as written:
- At most one report per milestone every 5 minutes.
- At least one every 10 minutes while work goes on.
- Lanes counted for that milestone's cards only.
- `stop` only when Terry says the milestone is complete.

- **`scripts/report-milestones.ps1` does the reporting**, and the architect runs it with `-Watch`.
  It counts lanes from the board for each milestone in `scripts/milestones.json`, and keeps its
  last report times in `%USERPROFILE%\.agentspace\swimcoach-last.json`, so a restart never reports
  inside the 5-minute limit. `-DryRun` prints what would be sent.
- **Milestones:**
  - `MVP` covers every MVP story, across all three epics, and never an epic card (Terry,
    2026-11-17).
  - `T1` to `T8` each cover their turn's stories (D14).
  - The architect edits `milestones.json` when stories are added or a turn starts.
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

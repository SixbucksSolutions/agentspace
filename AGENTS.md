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
- **The architect** owns the design. It reviews each turn's pull request independently, squash
  merges it, and deletes the branch.
- **A dev lead** is one Opus/high session per turn. It works on the branch `mvp/tN`, runs the
  implementer and reviewer subagents that D15 names for each story, reviews the result itself,
  and opens one pull request to `main`. A dev lead MUST NOT merge its own pull request.
- **Subagents** MUST NOT move cards or open pull requests.

The git authority above belongs to the architect. A dev lead pushes only its own branch.

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
  Backlog. An epic MUST NOT be broken into stories until Terry says so. Stories are titled `N.nn:
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

## Writing

- **Every rule in a doc MUST use RFC 2119 wording**, and the doc MUST say so at the top.
- **Every line, bullet and table cell that is a phrase MUST start with a capital**, except where
  it starts with a number, an identifier, a flag or a path.
- **Humor and casual register belong in conversation with Terry, never in commits, docs, code
  comments or program output.**

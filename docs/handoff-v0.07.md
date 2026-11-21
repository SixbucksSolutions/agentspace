# Handoff: agentspace during and after v0.07

RFC 2119 keywords; the capitals are load-bearing.

> **Draft, the live record of v0.07 (#0047) until it closes.** Started at 4:22pm Pacific on
> 2026-11-20, at v0.07-T01's close. Where this file and AGENTS.md, the board or decisions.md
> disagree, they win. The procedure is [architect.md](architect.md); the design is D43 to D55 in
> [decisions.md](decisions.md).

## Where things stand

**As of 7:04pm Pacific, 2026-11-20.**

| What | State |
|---|---|
| v0.07 (#0047), a host per project | In Progress. Twenty-one stories in six turns (D55; #0162 added to T02); T01 and T02 closed, T03 waits on the architect's code pins and Terry's word |
| The specs | state.md, operator.md, driver.md, lease-api.md and keeper.md carry v0.07's design, from T01; every link between them resolves on `main` (1,954) |
| The template and the bootstrap | `HostName` and `CaPublicKey` in `infra/agentspace.yaml`, with the `agentbox` change set shown empty; Rust, gcc and their `~/.local/bin` links in `box/bootstrap.sh` |
| The foundation crates | `agentspace-hosts` and `agentspace-detach` are new; `agentspace-alias`, `-credential`, `-box`, `-push` and `-launch` serve hosts; the token push reads localswim's `bot.json` (#0162), from T02 |
| `main`'s tests | Green but for 16 `agentspace-operator` help tests, red since T01's operator.md help blocks ran ahead of the code; #0151 (T03) turns them green |
| The box | Stopped, `m9g.xlarge`, root volume 20 GiB; unchanged by T01 and T02, which started no instance |
| Leases | None ready |
| Swimcoach | Milestone `v0.07` only; `v0.07-T01` stopped at 4:22pm, `v0.07-T02` at 7:04pm |
| Laptop files | Nothing of Terry's changed in T01 or T02; no `hosts.json` or `projects.json` exists yet (T05 writes `projects.json`, D54) |

### v0.07 by turn

| Turn | Closed | PRs | What it left |
|---|---|---|---|
| T01, the specs and the template | 4:22pm, 2026-11-20; dev lead archived the same minute | #90 to #94 | The five specs above, the template and the bootstrap, and the run record of the `agentbox` change set ([runs/e0.07-t01-s05.md](runs/e0.07-t01-s05.md)) |
| T02, the foundation crates | 7:04pm, 2026-11-20 | #95 to #99 | The crates above, each reviewed and run by the architect on Windows; #0162 (bot.json, carrying #0161) added on Terry's word and merged last |

## Rulings made during v0.07

**Each binds where its row says; this table only finds them.**

| Ruling | Made | Where it binds |
|---|---|---|
| A provisioning run never takes a `ready` host out of `ready`; a pending lease on a ready host is placed by the next run `get` starts | T01 | D48 |
| `CaPublicKey` carries no comment, so the computer's name never reaches the stack | T01 | D44 |
| A host is never sized by the box's parameters; `deploy -Host <p> -InstanceType <t>` resizes one | T01 | D55; operator.md |
| The lease CLI names the run's program with `AGENTSPACE_PROVISION_CMD`; the run gets the CLI's folders through its environment; no launch files | T01 | D55; driver.md, lease-api.md |
| Release of a pending lease runs the driver's release, so a killed placement leaves nothing | T01 | driver.md, state.md |
| After a CA replacement, a host other than the box is recovered by hand | T01 | D17; operator.md |
| T04-S01 re-seeds the lease CLI suite for D54; T04-S03 checks the state schemas' samples | T01 | #0155, #0157 |
| WMI's start replaces the environment, so the detached start builds the whole block from the user's logon environment; a hidden console in a new process group; a 10-second bound | T02 | #0147; driver.md, state.md, operator.md |
| The operator's key is made with `hosts.lock` held by the caller; its certificate is signed through a rename | T02 | #0148; state.md |
| The push reads `bot.json` by localswim's spec, checks the 43-character token and the receipt's `base` form, and closes operator.md Q9 | T02 | #0162; operator.md |

## Lessons from v0.07

| What happened | What to do | Where |
|---|---|---|
| A ruling said a check reads files it does not | Read the check before ruling on what it does | [architect.md](architect.md#running-a-turn), step 2 |
| The architect stamped rulings with times it had not read | Read the clock before writing a time | [architect.md](architect.md#writing-in-the-repository) |
| Every spec writer started before its pins were ruled and needed a second pass | Keep a pins file; rule as pins arrive; MAY hold a writer for them | [architect.md](architect.md#a-turn-that-writes-specs) |
| T01's docs-only PRs, merged without the workspace tests, left 16 operator help tests red on `main` | Run the workspace tests on a docs PR whose docs a crate's tests read | [architect.md](architect.md#reviewing-a-pull-request), step 3 |
| A ruling assumed WMI adds to a started process's environment; a probe showed it replaces it | Ask for a probe before ruling on how an OS call behaves | This file |
| The architect's conformance run named a build folder that did not exist, and reported a failure | Prove the harness first: check every program variable names a file that exists | [architect.md](architect.md#reviewing-a-pull-request), step 4 |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| Before T03: the architect's code pins (D55, with `state_dir`) | Architect | D55 |
| v0.07-T03 (#0150 to #0154), the programs | Terry's word, after the pins | D55 |
| #0137, met by #0147 and #0149, still in Backlog | Terry moves it out; the architect moves it to Ready For Review | #0137 |
| Before T05: Terry's yes for the instance hours (D53) | Terry, asked by the architect | D53 |
| #0027 from Git Bash | E0.07-T05-S01 | #0159 |

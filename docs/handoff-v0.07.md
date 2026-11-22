# Handoff: agentspace during and after v0.07

RFC 2119 keywords; the capitals are load-bearing.

> **Draft, the live record of v0.07 (#0047) until it closes.** Started at 4:22pm Pacific on
> 2026-11-20, at v0.07-T01's close. Where this file and AGENTS.md, the board or decisions.md
> disagree, they win. The procedure is [architect.md](architect.md); the design is D43 to D55 in
> [decisions.md](decisions.md).

## Where things stand

**As of 6:21pm Pacific, 2026-11-21.**

| What | State |
|---|---|
| v0.07 (#0047), a host per project | In Progress. Twenty-one stories in six turns (D55; #0162 added to T02); T01 to T05 closed, T06 (#0160) waits on Terry's word |
| The live checks | T05 ran D53's checks but the sign-ins on Terry's laptop and AWS ([runs/e0.07-t05-s01.md](runs/e0.07-t05-s01.md)): an agentspace lease went `pending` to `ready` on the new host `agentspace-host-agentspace`, its alias reached it, a swimcoach lease's alias matched v0.06's form and reached the box, the keeper held both hosts at once, and the `agentbox` change set was empty. Three findings: the install's keeper race (README fixed), #0163 and #0164 |
| The conformance suite | Covers v0.07 from T04: the lease CLI's `pending` and `no_capacity` and its provisioning-run tripwire, the operator and ProxyCommand per host, the keeper per host (H1 to H18) and the state schemas' samples; 73 files and the deadline case pass against `main` 39363d2's build, 2,640 results ([runs/e0.07-t04-s04.md](runs/e0.07-t04-s04.md)) |
| The specs | state.md, operator.md, driver.md, lease-api.md and keeper.md carry v0.07's design, from T01; every link between them resolves on `main` (1,954) |
| The template and the bootstrap | `HostName` and `CaPublicKey` in `infra/agentspace.yaml`, with the `agentbox` change set shown empty; Rust, gcc and their `~/.local/bin` links in `box/bootstrap.sh` |
| The foundation crates | `agentspace-hosts` and `agentspace-detach` are new; `agentspace-alias`, `-credential`, `-box`, `-push` and `-launch` serve hosts; the token push reads localswim's `bot.json` (#0162), from T02 |
| The programs | The driver and lease CLI answer `pending` and `no_capacity` and start a detached provisioning run; the operator has `-Host`, `-All`, `bootstrap` and `provision`; deploy, sync, bootstrap and provision work per host; the ProxyCommand finds each lease's host; the keeper holds one holder per host, from T03 |
| `main`'s tests | Fully green since #104 (16705f4): 3,887 passed, 0 failed, 58 ignored helpers, on the architect's run, and again in T04's run at 39363d2 (T04 added no crate test) |
| The hosts | Both stopped, confirmed by the architect's `status -All` at 6:08pm: the box (`agentbox`), `m9g.xlarge`, 20 GiB; and `agentspace-host-agentspace`, `m9g.xlarge`, 8 GiB, its root filesystem 66% used, its volume not changeable until about 12:05am on 2026-11-22 (#0164). **The new host has never run its patch unit** (#0163): its first patch run comes at its next boot, T06's first start. Each ran about 40 minutes in T05 |
| Leases | None ready; 23 released |
| Swimcoach | Milestone `v0.07` only; `v0.07-T01` stopped at 4:22pm, `v0.07-T02` at 7:04pm, `v0.07-T03` at 11:48am, `v0.07-T04` at 5:04pm and `v0.07-T05` at 6:21pm on 2026-11-21 |
| Laptop files | T05 changed what its card names: the v0.07 install in `%USERPROFILE%\.agentspace\bin` (two `.old-` files left for a later install to remove), `projects.json` listing `agentspace`, `hosts.json` with both hosts, and the new host's operator alias and key. The keeper runs the v0.07 build. Nothing in Terry's ssh config, task or `PATH` changed. Disclosed slips, none written: in T04 a listing of `bin` and a copy of the installed operator; in T05 a listing of `hosts\agentspace` and the Windows user name in a dev lead's transcript |

### v0.07 by turn

| Turn | Closed | PRs | What it left |
|---|---|---|---|
| T01, the specs and the template | 4:22pm, 2026-11-20; dev lead archived the same minute | #90 to #94 | The five specs above, the template and the bootstrap, and the run record of the `agentbox` change set ([runs/e0.07-t01-s05.md](runs/e0.07-t01-s05.md)) |
| T02, the foundation crates | 7:04pm, 2026-11-20 | #95 to #99 | The crates above, each reviewed and run by the architect on Windows; #0162 (bot.json, carrying #0161) added on Terry's word and merged last |
| T03, the programs | 11:48am, 2026-11-21 | #100 to #104 | The programs above; the architect's pins 7df955a, ce0967f and e502782; operator.md and state.md corrected six times from the stories' findings |
| T04, conformance | 5:04pm, 2026-11-21 | #105 to #108 | The cases above, each PR run by the architect on Windows against its own build and shown failing against the installed v0.06; the whole-suite record; no build defect found. operator.md and keeper.md corrected five times from the stories' questions |
| T05, the first new host, live | 6:21pm, 2026-11-21 | #109 | The install, `projects.json`, the new host, README's "Signing in on a new host" and its install step's keeper race; #0027's `deploy` and `sync` from Git Bash; defects #0163 and #0164 |

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
| The driver's Context carries `state_dir`, `state_dir_env`, `ssh_dir_env` and `operator_program`; `place_pending` returns each lease it could not place | T03 | D55's pins on main |
| `provision.log` holds the run's own lines, each failure with the program's error text; copying the programs' output is operator.md Q35 | T03 | operator.md, state.md |
| #0152 merges before #0151, which dispatches to its `deploy_with_instance_type` and `sync_all`; one owner per conformance file; one constant for the host alias's comment | T03 | #0151, #0152 |
| H16 holds the holder record open sharing read and write but not delete | T04 | keeper.md (c302ca6) |
| Provision step 9: a lease left `pending` inside a call that succeeds is logged and not warned; the warning's reason drops its prefix and last period; a missing ProxyCommand program leaves every lease `pending`, and on a host other than the box the run fails earlier, at step 6 | T04 | operator.md (721e02e) |
| `place_pending`'s one lease per hold of the lock is the driver crate's test; conformance shows its effects | T04 | operator.md (2aa267c) |
| A case MUST NOT copy, hash or open the installed program under test | T04 | AGENTS.md (2aa267c) |
| Cases that need `C:\Projects\localswim` SKIP when it is absent | T04 | #0156 |
| H9 counts a fast failure logged `started` and then found gone, as the handshake's step 7 allows | T04 | keeper.md (e87452b) |
| A timing failure under another runner's load is run once more alone before it is a finding, and both runs are recorded | T04 | #0158 |
| An install stops, by checked PID, any keeper the task's trigger started during the copy, then starts the task | T05 | README, "Install it", step 3 |
| A failed first provisioning run's retry by a `get` is D48's designed path, not a retry past a finding | T05 | #0159's record, finding 2 |
| A live check's expected bytes come from the spec; where the spec leaves text free, the record says where the words came from | T05 | #0159's record |

## Lessons from v0.07

| What happened | What to do | Where |
|---|---|---|
| A ruling said a check reads files it does not | Read the check before ruling on what it does | [architect.md](architect.md#running-a-turn), step 2 |
| The architect stamped rulings with times it had not read | Read the clock before writing a time | [architect.md](architect.md#writing-in-the-repository) |
| Every spec writer started before its pins were ruled and needed a second pass | Keep a pins file; rule as pins arrive; MAY hold a writer for them | [architect.md](architect.md#a-turn-that-writes-specs) |
| T01's docs-only PRs, merged without the workspace tests, left 16 operator help tests red on `main` | Run the workspace tests on a docs PR whose docs a crate's tests read | [architect.md](architect.md#reviewing-a-pull-request), step 3 |
| A ruling assumed WMI adds to a started process's environment; a probe showed it replaces it | Ask for a probe before ruling on how an OS call behaves | This file |
| The architect's conformance run named a build folder that did not exist, and reported a failure | Prove the harness first: check every program variable names a file that exists | [architect.md](architect.md#reviewing-a-pull-request), step 4 |
| Ten pin questions in T03 showed the pins were written before the specs' every need was clear | Pin from the merged specs, then expect amendments mid-turn and commit them on `main` when two stories share them | This file |
| Two stories wrote one file (the host alias) with different bytes, so each rewrote the other's | Give a shared literal one home in a library both use | #0151, #0152 |
| H9 passed every quiet run and failed in the architect's review under load, because its row allowed one of the two forms the spec allows | Write a timed case's expectation from every outcome the spec permits, and run a review under load | keeper.md H9 |
| A spec row said a case holds a file "with no sharing", which made its own path unreachable | Check a case row's setup can reach the outcome it names | keeper.md H16 |
| A subagent copied the installed operator, and a dev lead listed the installed `bin` | Guard every case against the installed program, and never list Terry's real folders to find a build folder | [AGENTS.md](../AGENTS.md) |
| The architect's sum of #106's results was wrong (510 for 589) in a message to the dev lead; the runners' own totals were wrong twice | Sum counts with a command, never by hand | This file |
| The architect again stamped two rulings with times it had not read ("6:04pm", "6:00pm"); the dev lead's clock caught it | Read the clock before writing a time, every time | [architect.md](architect.md#writing-in-the-repository) |
| Every stand-in run passed while two live defects waited: a patch unit enabled mid-boot never runs, and EC2 accepts a same-size grow | A spec that leans on how an outside system behaves MUST say so, and the live run is where it is tested | #0163, #0164 |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| v0.07-T06 (#0160): Terry's sign-ins on agentspace's host, and an agent working an agentspace card there | Terry's word, then the architect; the yes for instance hours (D53) was T05's, asked again for T06 | D55 |
| Before T06: rustc and cargo are not on a non-login ssh shell's `PATH` on the new host; check what the agent's shell gets | The architect, in T06's sweep | #0159's record, the diagnosis read |
| #0137 and #0138, fixed and merged, now in Ready For Work by Terry's move; the board refuses Bot's move from there to Ready For Review | Terry moves each to In Progress (or past it); the architect moves it to Ready For Review | #0137, #0138 |
| #0163: a new host's first provisioning run fails at sync, and the host is used before its first patch run | A later release, carded in Backlog | #0163 |
| #0164: `grow -GiB 0` is not refused | A later release, carded in Backlog | #0164 |
| C13's variant "hosts.json valid but no box record" failed once in a full run ("changed"), then 265 runs passed; its failure now names the file | Whoever sees it fail again | #0155's PR, #107 |
| The localswim checkout's path is fixed at `C:\Projects\localswim`; a seam for it would let sync's build cases run elsewhere | A later card, if wanted | operator.md, sync step 7 |
| #0027 from Git Bash | E0.07-T05-S01 | #0159 |

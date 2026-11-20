# Handoff: agentspace after v0.05

RFC 2119 keywords; the capitals are load-bearing.

> **Final, as of 2:05pm Pacific on 2026-11-19.** v0.05 is closed: all fifteen of its pull
> requests are merged to `main`, and Terry moved #0054 and every v0.05 card to Completed. Started
> as a draft at 8:22am the same day, during v0.05-T03, at Terry's word. Where it and AGENTS.md or
> the board disagree, they win.

**Written for the architect who starts v0.06, after Terry `/clear`s this one.** Read in this order:

1. This file.
2. [AGENTS.md](../AGENTS.md): every standing rule. **This file cites AGENTS.md and the decisions
   by section and does not restate them**, so a rule has one home (the reason is in "What changed
   from the v0.04 handoff", below). Where the two disagree, AGENTS.md wins, and the disagreement is
   a bug in this file.
3. [decisions.md](decisions.md): D38 to D40 in full (v0.05's), then D20 and D25 to D37.
4. [handoff-v0.04.md](handoff-v0.04.md): the release cycle (breaking an epic into stories, running
   a turn, reviewing a pull request, closing a release). This file gives only what v0.05 changed
   in it. Its review step 3 is replaced (AGENTS.md, "Terry's real files in a review").
5. [vision.md](vision.md) and [roadmap.md](roadmap.md), for what is next and why.

**Before anything else, start the architect's standing duties** (below).

## What changed from the v0.04 handoff, and why

- **One home per rule.** The v0.04 handoff restated many rules from AGENTS.md. On 2026-11-19 one
  of them (snapshot Terry's real files in every review) was replaced in AGENTS.md, and the
  restated copy had to be marked replaced by hand. This file points to the rule instead, as D34
  asks of the specs.
- **Terry's orders get a table of their own**, with where each one binds, so that none is lost
  in a `/clear`.
- **Live state and history are kept apart.** "Where things stand" is what is true now, and is
  refreshed at close; the lessons and rulings are history, and are not.

## Where things stand

**As of 2:05pm Pacific, 2026-11-19.**

| What | State |
|---|---|
| v0.05 (#0054), the lease CLI in Rust | Done. Fifteen stories in five turns (D38), all merged, and every card in Completed (Terry, 2026-11-19) |
| v0.06 (#0055), the operator in Rust | Ready For Work. Broken into 25 stories in six turns on Terry's word, 2026-11-19 (D42; #0111 to #0135, the plan by turn on #0055). In Progress. v0.06-T01 closed at 4:28pm: all seven PRs (#64 to #70) merged, its dev lead archived. Its sampler record, `v006-t01`, ran 2:41pm to 4:26pm with no gap (3,148 samples), and is in `C:\Projectsgentspace-runs006-t01.jsonl`, outside the repo and out of `%USERPROFILE%\.agentspace`, for E0.06-T02-S08. v0.06-T02 closed at 5:40pm: all eight PRs (#71 to #78) merged, its dev lead archived. T03 is next; before it starts the architect commits the `agentspace-sync` and `agentspace-ssh-proxy` stubs (D42) |
| The lease CLI | The Rust build, installed in `%USERPROFILE%\.agentspace\bin` and on Terry's user `PATH` (ExpandString, with his yes), so `agentspace` runs it from cmd, PowerShell and Git Bash. The checkout's `bin\agentspace.cmd` still runs the prototype until v0.06 (README, "The lease CLI") |
| `main` | One Cargo workspace, crates `agentspace-cli`, `-values`, `-state`, `-credential`, `-alias`, `-driver-api`, `-driver-ec2` and `xtask`. It builds `--locked` on Windows |
| The box | Stopped at 4:28pm, after v0.06-T01. `m9g.4xlarge` (16 vCPUs, 64 GiB, D39, set in `infra\local.parameters`), root volume 20 GiB (a 19G filesystem, 30% used). Size it for its next use's peak before starting it. Rust 1.99.0 with clippy and rustfmt, and gcc, installed by hand in `ubuntu`'s home (D38's note) |
| Leases | None ready |
| The keeper | Running, idle, as pwsh 24588. In T05 it held a Rust-made lease as it holds a prototype one |
| Swimcoach | `v0.06` reported from 2:29pm Pacific, 2026-11-19, with the reporter running under `-Watch` |
| The app's SSH connection | `agentspace-inception`, whose host Terry points at each new lease's alias. The saved presets are gone; start a box dev lead with `target: "ssh"`, `host: "agentspace-inception"`, `cwd: "/home/ubuntu/agentspace"` |

## Terry's orders from v0.05

**Each binds where its row says; this table only finds them.** All were given on 2026-11-19.

| Order | Where it binds |
|---|---|
| An expired AWS sign-in is fixed with `aws login --remote`, its link pasted into a fresh Incognito window | AGENTS.md, "The architect is Terry's proxy" |
| Grow the box's root EBS volume as needed, up to 64 GiB, never leaving it under 60% used | AGENTS.md, "Growing the box's disk" |
| Before the box runs agents, make it an M-class instance with a vCPU, and so 4 GiB, per agent working at once | AGENTS.md, "Sizing the box before it runs agents"; D39 |
| Until v1.0, stop the box as soon as a turn that used it is merged and in Ready For Review | AGENTS.md, "Sizing the box before it runs agents", last bullet |
| No review lists, reads or hashes Terry's real files; isolation is enough | AGENTS.md, "Terry's real files in a review" |
| By v1.0, agentspace's own files are JSON where feasible; the keeper's logs and the YAML template stay | D40 |
| #0054's done-when no longer deletes the prototype's lease CLI files in v0.05 (the architect's call, made at layout and not overruled) | D38; #0054 |
| Plan from the board (#0052) is v0.11 and comes before the new platforms, so agentspace is feature complete on one platform first | D26; roadmap.md; #0052 |
| v0.13 is folded into v0.12 (#0051): `linux/amd64` and `windows/amd64` in one release, a shared multi-platform turn and then two tracks in parallel. #0081 is archived, v0.13 is not used, and v1.0 follows v0.12 | D26, D4; roadmap.md; #0051 |

**The one question that was still Terry's is answered** (Terry, 2026-11-19, 2:05pm Pacific, to the
next architect): D33's measured compiling turn, which v0.05-T01 owed and recorded only in part, is
measured in v0.06's first turn on the box (D33).

## The architect's standing duties

**They die with the session, so a fresh architect MUST start them first.** What each is, and how,
is in [handoff-v0.04.md](handoff-v0.04.md#the-architects-standing-duties) and AGENTS.md; v0.05 added
the last two.

1. **The board watcher**, under the Monitor tool as reader `agentspace`, re-armed every 30
   minutes. Check first that no orphan reads as `agentspace`: on 2026-11-19 the cleared session's
   watcher was still running, and was stopped by checked PID with Terry's yes.
2. **The swimcoach reporter**, whenever `scripts/milestones.json` lists a milestone.
3. **A watchdog on each running dev lead.**
4. **A liveness watch on a running sampler and on the box's root disk** (AGENTS.md, "Watching a
   measurement").
5. **Stopping the box at the close of every turn that used it** (AGENTS.md, "Sizing the box
   before it runs agents").

## Running a turn: what v0.05 changed

**handoff-v0.04.md's cycle stands. These are v0.05's additions, each with its home.**

**Before a turn on the box:**
- **Size the box for the turn's peak** through the stack, while it is stopped (D39).
- **Create the lease, and have Terry point `agentspace-inception` at its alias.** Then over the
  alias: `git pull` in `~/agentspace`, check `gh auth status`, git's identity, `cargo --version`
  in a login shell, and the root disk's use.
- **If the turn is measured:** start `agentspace-sample` before the dev lead, confirm it records
  the dev lead's session before giving the go, and arm the liveness watch at once.

**Pinning an interface as code.** Where a turn's stories meet at an interface, the architect MAY
commit that interface to `main` before the turn starts, exactly as the cards pin it, and nothing
else (D38: the workspace root before T01, the driver trait before T03). Check the pin against every
function the implementers will call: T03's pin missed `SystemRoot`, and the driver could not meet
"reads no environment variable" until the architect added it (502c99c).

**Rulings.** A call the specs do not settle MUST reach the architect before anything is built on
it; T01 and T02 lost fix rounds to calls ruled after the fact. Each ruling goes into the doc that
owns the fact, marked "architect, <date>, for <story>", and the dev lead gets it in a message.
`grep -n "architect, 2026-11-19" docs/*.md` lists v0.05's.

**Messages to a dev lead.** A message sent while it is mid-turn is queued, and its turn MAY end
before it reads it: the first GO of v0.05-T01 was read only after the dev lead had ended its turn
"waiting for GO". Send it again when a turn note shows it was missed.

**Reviewing a pull request.** handoff-v0.04.md's steps stand, except step 3, plus:
- **Run every check on Windows yourself**, and the card's done-when, not only the dev lead's
  report. Linux-only proofs from the box were green twice while the Windows build failed.
- **A new head after a rebase** is checked with `git range-diff` and a diff of the story's own
  crate against the head reviewed, never of the whole tree, which counts what `main` gained.
- **Merge and delete in one command that stops at the first failure** (AGENTS.md, "Roles and pull
  requests").
- **Quote a SHA only from a command's output.** Twice on 2026-11-19 the architect sent a guessed
  SHA and had to correct it.
- **Before calling a defect, prove the harness.** A no-stdout run first "showed" exit 0, because
  the harness gave the child a hidden console; with no console the build was right.

**Closing a turn.** Each step is AGENTS.md's; in order:
1. The dev lead removes its worktrees and branches, stops its watcher, and says so.
2. The architect archives it, stops its watchdog, and removes any worktree folder it held.
   `archive_session` can be refused once by a hook that relabels the call, and succeeds when sent
   again with the input the refusal quotes; it is refused while the dev lead's turn is still
   running.
3. The turn's milestone is removed from `milestones.json`, pushed, and then `swimcoach stop`.
4. **If the turn used the box:** fetch what it left, release the lease, `down`, and `status`.

## What v0.05 built

| Turn | Story | Card | PR | What |
|---|---|---|---|---|
| T01 | E0.05-T01-S01 | #0094 | #53 | `agentspace-cli`: command line, help, errors, output |
| T01 | E0.05-T01-S02 | #0095 | #50 | `agentspace-values` |
| T01 | E0.05-T01-S03 | #0096 | #52 | `agentspace-state`: the model, schemas and bytes |
| T01 | E0.05-T01-S04 | #0097 | #51 | `agentspace-credential`: key, certificate, CA checks |
| T01 | E0.05-T01-S05 | #0098 | #49 | `agentspace-alias`: alias file, `Include` line, ProxyCommand, as text |
| T02 | E0.05-T02-S01 | #0099 | #59 | The state file on disk: paths, lock, POSIX-rename replace |
| T02 | E0.05-T02-S02 | #0100 | #58 | The credential on disk: access rules set at create, renew guard, delete |
| T02 | E0.05-T02-S03 | #0101 | #56 | The alias on disk: `Include` insert, alias file, ProxyCommand program |
| T02 | E0.05-T02-S04 | #0102 | #55 | v0.05-T01's run record, partial ([runs/e0.05-t02-s04.md](runs/e0.05-t02-s04.md)) |
| T02 | E0.05-T02-S05 | #0103 | #57 | `test-interop.ps1` and `AGENTSPACE_PEER_CMD` |
| T02 | E0.05-T02-S06 | #0104 | #54 | `cargo xtask dist`: the build output folder, wrapper and forwarder |
| T03 | E0.05-T03-S01 | #0105 | #60 | `agentspace-driver-ec2`: the EC2 driver behind `agentspace-driver-api` |
| T03 | E0.05-T03-S02 | #0106 | #61 | The five lease commands, with in-process tests against a throwaway home (AGENTS.md, "Terry's real files in a review") |
| T04 | E0.05-T04-S01 | #0107 | #62 | The whole conformance suite against the Rust build: 786 passed, 0 failed, 0 skipped ([runs/e0.05-t04-s01.md](runs/e0.05-t04-s01.md)) |
| T05 | E0.05-T05-S01 | #0108 | #63 | The install and the cutover: create, connect, keeper, renew and release through the real box, and interop with the prototype both ways ([runs/e0.05-t05-s01.md](runs/e0.05-t05-s01.md)) |

## Lessons from v0.05

**Each is recorded where it applies; this is the index.**

- **A Rust turn of five stories does not fit an 8 GiB root disk.** Five `target` folders filled it
  mid-turn, and the build output passed 5 GB at its peak (runs/e0.05-t02-s04.md). Size the disk
  with the box (AGENTS.md, "Growing the box's disk").
- **The sampler died silently with the disk, and nothing watched it** (#0109; AGENTS.md, "Watching
  a measurement"). Seven minutes of a 31-minute turn were recorded. Silence is not success.
- **Compiling is the load to size for:** the first compile burst took the 4-core box to 3.83 cores,
  against 0.51 for v0.04-T01's spec writing (footprint.md).
- **Windows OpenSSH finds `~` from the profile, not `HOME`, and resolves a relative `Include`
  against the user's real `~/.ssh`** (state.md, The `Include` line). A test MUST use `-F` with an
  absolute `Include`. The architect's own scratch check read Terry's alias files this way once.
- **A private key a test writes inherits its folder's rules on Windows**, and Windows OpenSSH
  refuses it; the box's Linux never showed it. A test sets the key's rules itself.
- **Linux OpenSSH 10 refuses a config that starts with a byte order mark; Windows OpenSSH 9.5
  accepts one.**
- **Classic `MoveFileEx` cannot replace a file any reader holds open, even one sharing delete;**
  the POSIX rename can (state.md, Replacing a file).
- **`ReplaceFileW` keeps a file's access list, not its owner** (state.md, The `Include` line).
- **Rust's runtime reopens a closed stdout on `/dev/null` on Unix**, so "stdout is closed" cannot
  be seen there in safe code (lease-api.md, Output).

## What is open

**Rechecked at close, 2:05pm Pacific, 2026-11-19.**

| What | Where it goes |
|---|---|
| D33's measured compiling turn, still owed in full | v0.06's first turn on the box (Terry, 2026-11-19; D33) |
| #0109, the sampler dies silently when its disk fills | Before v0.09's measurement (roadmap.md) |
| #0029, #0041, #0083, the lease CLI's hardening | OBE, Terry's word, 2026-11-19, recorded in a comment on each (evidence: runs/e0.05-t05-s01.md, "Proposal"). Terry moves them to Completed. What they leave is #0110's and #0010's |
| #0110, conformance hardening: the checks the suite lacks, among them #0029 points 1 and 2 and #0083 point 1 | Rides with v0.06 (roadmap.md) |
| The prototype's lease CLI (`bin\agentspace.cmd`, `scripts\lease-cli.ps1`) and the install's forwarder into the checkout | Removed by v0.06 (D38) |
| The box's Rust toolchain and gcc, installed by hand | A project host that builds Rust (v0.07, #0047; D38's note) |
| D40's moves to JSON: `local.parameters`, the instance-ID cache, the push stamp | Specified 2026-11-19 (D40; state.md); built by v0.06 |
| The operator's and the keeper's minor clean-room gaps, and FB-23 | Answered 2026-11-19 in the owning docs (runs/e0.04-t06-s01.md); built by v0.06 |
| #0027, #0034, #0066, #0068, #0087 | Met by v0.06 |
| #0064, #0043, #0082, and the rows after 1.0 | handoff-v0.04.md, "What is open" |

## The scope ahead: v0.06

**v0.06 (#0055): the operator, the ProxyCommand and the keeper in Rust, and every PowerShell and
`cmd` file in the product removed** (D28, D37), the prototype's lease CLI files with them (D38).
Its stories MUST NOT be laid out until every v0.05 card is in Completed, and then only on Terry's
word (AGENTS.md, "The board").

**Before its stories are laid out, the architect SHOULD:**
- **Answer the clean-room gaps that v0.06 owns** in operator.md and keeper.md, as v0.05 did for
  the lease CLI's before its builders started (D38).
- **Make D40's three moves to JSON in the specs**, with their schemas.
- **Settle the measured turn with Terry**, and if it is v0.06's first turn, size the box for it
  (D39) and plan the sampler's watch. Settled: it is (D33).
- **Plan the cutover of the keeper's task**, which is persistent configuration on Terry's laptop
  and takes his yes (D30, D37).

**Done on 2026-11-19, after v0.05 closed:** the clean-room gaps are answered in their docs, D40's
moves are specified with their schemas, the measured turn is settled, and D41 plans the cutover
and the measured turn's setup. What waits is Terry's word to lay out the stories.

**The road after v0.06 changed during v0.05** (Terry, 2026-11-19; D26, roadmap.md). The v0.04
handoff's order no longer holds past v0.10:
- **v0.11 is plan from the board (#0052)**, swapped ahead of `linux/amd64`, so agentspace is
  feature complete on one platform before it turns to others. Neither depends on the other.
- **v0.12 is both new platforms (#0051), `linux/amd64` and `windows/amd64`**, with v0.13 (#0081)
  folded in and archived. Its first turn is shared: the driver serves more than one platform, with
  a seam per platform, so the two tracks then run as parallel stories and touch different files.
  Windows is the long track and carries the open design calls (its lease identity, the vision's
  V2; its host agent; its profile; licensing cost). Expect a re-plan once those are made (D29).
- **v1.0 (#0053) follows v0.12**, and v0.13 is not used. On the board, #0051 now blocks #0053
  directly.

## When Terry approves v0.05

**Done, 2026-11-19, between 1:59pm and 2:05pm Pacific**, after a board event showed #0054 and
#0108 in Completed. Kept as the model for the next release's close. Once every pull request is
merged and the board shows the epic and every card in Completed, the architect MUST, in order:
1. Remove the `v0.05` block from `scripts/milestones.json`, commit and push.
2. `C:\Projects\swimcoach\bin\swimcoach.cmd stop --project agentspace --milestone v0.05`.
3. Mark v0.05 done in roadmap.md's table, with what it produced.
4. Stop the reporter when no milestone is left.
5. Finalize this file: fill every **[at close]**, replace the draft note, and point AGENTS.md's
   "Where it stands" here.
6. Wait for Terry's word before laying out v0.06.

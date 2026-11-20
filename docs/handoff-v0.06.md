# Handoff: agentspace after v0.06

RFC 2119 keywords; the capitals are load-bearing.

> **Final, as of 10:04am Pacific on 2026-11-20.** v0.06 is closed: all twenty-six of its pull
> requests are merged to `main`, and Terry approved #0055 and its last story into Completed.
> Started as a draft at 7:45am the same day, during v0.06-T06, at Terry's word. Where this file and
> AGENTS.md or the board disagree, they win.

**Written for the architect who starts v0.07, after Terry `/clear`s this one.** Read in this order:

1. This file.
2. [AGENTS.md](../AGENTS.md): every standing rule.
3. [architect.md](architect.md): how a release is run, from Terry's word to Completed. It replaces
   the release cycles in the v0.04 and v0.05 handoffs.
4. [decisions.md](decisions.md): D40 to D42 in full (v0.06's), then D25, D3, D6 and D21 for
   v0.07's design.
5. [vision.md](vision.md), [roadmap.md](roadmap.md) and card #0047, for what is next and why.

**Before anything else, start the standing duties** ([architect.md](architect.md#the-standing-duties)).

## What changed from the v0.05 handoff, and why

- **The release cycle has one home, [architect.md](architect.md).** Each handoff used to carry
  only what its release changed in the cycle, so a new architect read v0.04's cycle and then each
  later handoff's changes to it, and v0.04's names prototype commands that v0.06 removed. The
  handoffs are now history; the playbook is kept current.
- **Live state is a table by turn, refreshed at each turn's close.** v0.05's handoff was final
  when v0.06 began, and its one live-state cell grew to about 1,500 characters as each v0.06 turn
  closed. This file is the live record until it is final, and handoff-v0.05.md stays as it was.
- **Each lesson says what to do differently and where that is now written**, so that a lesson is
  an action rather than a story.
- **Every open item names who acts on it**: Terry, the architect, or a release.

## Where things stand

**As of 10:04am Pacific, 2026-11-20. Each turn's close is in the next table.**

| What | State |
|---|---|
| v0.06 (#0055), the operator in Rust, PowerShell retired | All six turns merged, the last at 8:14am, 2026-11-20 (PR #89, `75c6f73`). Done: Terry approved #0055 and #0135 into Completed by 10:04am. Marked done in roadmap.md |
| Terry's laptop | Runs the Rust programs from `%USERPROFILE%\.agentspace\bin`, installed from S02's build `4f18b59` on 2026-11-20. His `agentspace` entry's ProxyCommand names `agentspace-ssh-proxy.exe`, and his keeper task runs `agentspace-keeper-task.exe`, both with his yes (D41). The reversals are in `C:\Projects\agentspace-runs\v0.06-t05\`, outside the repository |
| The stack's parameters | `%USERPROFILE%\.agentspace\stack-parameters.json` (D40), written from the git-ignored `infra\local.parameters` in T05. Since T06 it is the only source: a size change sets its `InstanceType`, then `agentspace-operator deploy` while the box is stopped (AGENTS.md, "Sizing the box before it runs agents"). `infra\local.parameters` is no longer read |
| The box | Stopped at 2:01am, 2026-11-20. `m9g.xlarge` (4 vCPUs), root volume 20 GiB. Its system files, CA check and localswim CLI were last installed by the Rust `sync` at 1:58am. Rust and gcc are still installed by hand in `ubuntu`'s home (D38's note) |
| Leases | None ready |
| The keeper | The Rust keeper, running and idle |
| Swimcoach | No milestone. `v0.06` was stopped and the reporter with it at 10:04am, 2026-11-20; start it again when v0.07's stories are laid out |
| The app's SSH connection | `agentspace-inception`, whose host Terry points at each new lease's alias |

### v0.06 by turn

| Turn | Closed | PRs | What it left |
|---|---|---|---|
| T01, on the box, measured | 4:28pm, 2026-11-19 | #64 to #70 | The portable crates. Its sampler record, `v006-t01`, ran 2:41pm to 4:26pm with no gap (3,148 samples) |
| T02, on the laptop | 5:40pm, 2026-11-19 | #71 to #78 | The Windows halves, and T01's footprint ([runs/e0.06-t02-s08.md](runs/e0.06-t02-s08.md)) |
| T03, on the laptop | 6:42pm, 2026-11-19 | #79 to #83 | The operator, the ProxyCommand and the keeper as programs, built by `cargo xtask dist` |
| T04, on the laptop | 10:58pm, 2026-11-19 | #84 to #86 | The whole suite against the Rust build: 1,439 results, none failed or skipped ([runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)) |
| T05, on the laptop and the box | 2:16am, 2026-11-20 | #87, #88 | The cutover and its live checks ([runs/e0.06-t05-s01.md](runs/e0.06-t05-s01.md)); the deploy hang found and fixed ([runs/e0.06-t05-s02.md](runs/e0.06-t05-s02.md)) |
| T06, on the laptop | 8:14am, 2026-11-20; dev lead archived at 8:15am | #89 | The prototype removed. The suite passed against the installed programs under its new defaults: 44 files, 1,443 passed, none failed or skipped, and the deadline case 5 of 5 in 905 seconds; `cargo test --workspace --locked` 2,883 passed, 50 ignored helpers ([runs/e0.06-t06-s01.md](runs/e0.06-t06-s01.md)) |

## Terry's orders from v0.06

**Each binds where its row says; this table only finds them.**

| Order | Given | Where it binds |
|---|---|---|
| #0029, #0041 and #0083 are OBE | 2026-11-19 | A comment on each; Terry moves them |
| D33's measured compiling turn is v0.06-T01, on the box | 2026-11-19 | D33; done, the record is T02-S08's |
| A grow below the 60% floor takes his word each time, after a cleaning pass ("you're free to resize as needed after the cleaning pass") | 2026-11-19 | AGENTS.md, "Growing the box's disk" |
| A cap MAY be extended by the architect when he delegates it, for that milestone | 2026-11-19, for v0.06-T04 | [architect.md](architect.md#milestones-on-terrys-phone) |
| The cutover's two changes to his laptop, his ssh entry and his keeper task, made by the dev lead | 2026-11-19, 11:08pm | D41; a comment on #0134 |
| The box deployed back to `m9g.xlarge` | 2026-11-19, 11:08pm | Done in T05 |
| The deploy hang fixed as a second T05 story before T06 | 2026-11-20 | D42; #0140 |

## What v0.06 built

| Turn | Story | Card | PR | What |
|---|---|---|---|---|
| T01 | E0.06-T01-S01 | #0111 | #65 | The operator's front: command line, help, usage errors and messages |
| T01 | E0.06-T01-S02 | #0112 | #68 | The programs the operator starts: the AWS CLI, ssh and scp |
| T01 | E0.06-T01-S03 | #0113 | #67 | Starting the box and waiting for it, and the instance-ID cache |
| T01 | E0.06-T01-S04 | #0114 | #70 | The token push, its portable half |
| T01 | E0.06-T01-S05 | #0115 | #64 | The laptop's time zone as an IANA name, and the zone check |
| T01 | E0.06-T01-S06 | #0116 | #69 | The keeper's front and its pass |
| T01 | E0.06-T01-S07 | #0117 | #66 | The keeper's files: holder record, launch files and the holder command's matcher |
| T02 | E0.06-T02-S01 | #0118 | #77 | The `ca` command and sync's CA check |
| T02 | E0.06-T02-S02 | #0119 | #76 | The token push, its Windows half |
| T02 | E0.06-T02-S03 | #0120 | #75 | The keeper on Windows: processes, the stop, the locks |
| T02 | E0.06-T02-S04 | #0121 | #78 | Starting a holder: the handshake, WMI and the launcher |
| T02 | E0.06-T02-S05 | #0122 | #74 | The keeper's watch |
| T02 | E0.06-T02-S06 | #0123 | #73 | The keeper task's installer |
| T02 | E0.06-T02-S07 | #0124 | #71 | The keeper task's launcher |
| T02 | E0.06-T02-S08 | #0125 | #72 | The footprint of v0.06-T01, measured ([runs/e0.06-t02-s08.md](runs/e0.06-t02-s08.md)) |
| T03 | E0.06-T03-S01 | #0126 | #83 | The operator commands: `up`, `down`, `status`, `grow` and `ca` |
| T03 | E0.06-T03-S02 | #0127 | #82 | `sync` and `deploy`, with the template and the box files embedded |
| T03 | E0.06-T03-S03 | #0128 | #80 | The ProxyCommand, `agentspace-ssh-proxy.exe` |
| T03 | E0.06-T03-S04 | #0129 | #81 | The keeper program, wired: real processes, locks, launcher and clock |
| T03 | E0.06-T03-S05 | #0130 | #79 | `cargo xtask dist` writes v0.06's eight files |
| T04 | E0.06-T04-S01 | #0131 | #86 | The whole conformance suite against the Rust build ([runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)) |
| T04 | E0.06-T04-S02 | #0132 | #84 | Operator checks for `down`, `status`, `grow`, usage, help and the start paths |
| T04 | E0.06-T04-S03 | #0133 | #85 | #0110's missing cases, the keeper's logon environment, and the CA key's words (#0136) |
| T05 | E0.06-T05-S01 | #0134 | #87 | The cutover on the laptop, and its live checks ([runs/e0.06-t05-s01.md](runs/e0.06-t05-s01.md)) |
| T05 | E0.06-T05-S02 | #0140 | #88 | The deploy hang fixed (#0139), with a live no-op deploy ([runs/e0.06-t05-s02.md](runs/e0.06-t05-s02.md)) |
| T06 | E0.06-T06-S01 | #0135 | #89 | The prototype removed, the suite's defaults moved to the install folder ([runs/e0.06-t06-s01.md](runs/e0.06-t06-s01.md)) |

## Lessons from v0.06

**Each says what to do differently, and where that is now written.**

| What happened | What to do | Where |
|---|---|---|
| The Rust `deploy` hung on its first real run: `main` held the stdout lock while a copy thread waited for it. The suite had no deploy case at all, and the crate tests never ran through `main` | Treat every "None" in conformance.md's coverage that a release touches as a case to add or a live check to name; test a program through its `main` | [architect.md](architect.md#breaking-an-epic-into-stories); `agentspace-run`'s doc; #0139 |
| Defects that only Windows shows were found at the architect's review: a `.cmd` stand-in losing `%~dp0` after `shift`, `more` adding CR LF to empty input | Run every check on Windows yourself | [architect.md](architect.md#reviewing-a-pull-request) |
| The card's move and a "merged" message went out beside a merge that GitHub refused (PR #70) | Move and message only after `MERGED` is read back | AGENTS.md, "Roles and pull requests" |
| A test's `Debug` of a captured environment printed the session's messaging token into a subagent's output on the box | Assert on the variables a test names, never print their values | AGENTS.md, "Terry's real files in a review" |
| A diagnostic sampler printed an `aws` command line with the stack's network parameters, and a `lease list` printed lease IDs, into transcripts | Mask before printing, with a mask proven on planted values | AGENTS.md, the same section |
| The first live deploy's exit code was lost, because the tool moved the long command to the background | Write exit codes to a file from a wrapper | [architect.md](architect.md#live-checks-on-terrys-laptop-and-the-box) |
| A PowerShell string wrote a bell and a vertical tab into a handoff's path | Edit docs with the Edit tool, and scan for control characters | [architect.md](architect.md#writing-in-the-repository) |
| An 80-minute wait on a question to Terry held the architect's turn while the dev lead's PR and messages queued | After his answer, read every linked session and open PR before acting | [architect.md](architect.md#running-a-turn) |
| Bot cannot move a card out of Backlog, so defects met by v0.06 wait there | List them for Terry at the release's close | [architect.md](architect.md#closing-a-release); "What is open" |
| A sweep of #0135's lists against the tree settled four questions before T06's dev lead started | Sweep every turn's cards against `main` before its dev lead starts | [architect.md](architect.md#running-a-turn) |
| The box stayed `m9g.4xlarge` from T01 until T05, since only a deploy resizes it | Plan the size-down at the close of the turn that sized it up | [architect.md](architect.md#running-a-turn) |
| Seven crates' builds outgrew the 11 GiB disk; a cleaning pass and Terry's word took it to 20 GiB | Size the disk with the box, before the turn | AGENTS.md, "Growing the box's disk" |
| The measured turn's sampler ran with no gap, under the liveness watch #0109 asked for | Keep the watch for every measured turn | AGENTS.md, "Watching a measurement" |

## What is open

**Rechecked at close, 10:04am Pacific, 2026-11-20:** every card named below is still in Backlog.

| What | Who acts | Where it goes |
|---|---|---|
| Backlog cards v0.06 met: #0034, #0066, #0068, #0087 (named results in [runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md)), #0110 (T04-S01 and S03), #0136 (PR #85), #0139 (PR #88) | Done: Terry moved them out of Backlog, the architect moved them to Ready For Review, and Terry approved them into Completed on 2026-11-20 | Completed |
| #0029, #0041, #0083, OBE | Done: archived at Terry's word on 2026-11-20, each with its OBE comment | The board's archive |
| #0027: the operator from Git Bash. `status` and `up` were shown in T05; `deploy`, `sync` and `grow` were not run from Git Bash | Architect: run them from Git Bash at the next live use of the box, and record it | #0027 |
| #0137, a crate's tests change their own environment with `set_var`; #0138, a conformance file removes `board-holder.json` without tolerating its absence | A release's hardening, or v0.07's first turn if it touches those files | Backlog |
| #0109, the sampler dies silently when its disk fills; #0082, it misses work a session orphans | Before v0.09's measurement | roadmap.md |
| #0064, a release leaves a lease's connections up; #0043, the CloudWatch backstop | v0.08 | roadmap.md |
| The box's Rust and gcc, installed by hand | v0.07 (#0047): a host that builds Rust gets them from its bootstrap | D38's note |
| The interop check, retired with the prototype | None: its last result, 36 passed each way, is in [runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md) | D42 |
| The box's `install.sh` message now names `agentspace-operator sync` (FA-12), but the installed operator embeds the box files of build `4f18b59`, so the box has the old message until the operator is rebuilt from `main` and installed | Architect: at the next install from `main`, before its next `sync` | runs/e0.06-t06-s01.md |
| What the `test-unmet-` prefix means now | None: every such file is an ordinary check the Rust programs pass, and a `FAIL` is a defect | conformance.md, its opening bullets |
| localswim #0611 to #0614, its undocumented receipt and credential | localswim's board | handoff-v0.04.md, "What is open" |

## The scope ahead: v0.07

**v0.07 (#0047): a host per project** (D25). One size (`m`) and one platform (`linux/arm64`). Done
when a second project's first lease creates its own host, gets an alias that reaches it, and runs
an agent there that works a card on its board, while swimcoach's leases are unaffected and the
lease contract does not change (D1). From v0.07 on, all new product code MUST be Rust (D28).

**Its stories MUST NOT be laid out until every v0.06 card is in Completed, and then only on
Terry's word** (AGENTS.md, "The board").

**Before its stories are laid out, the architect SHOULD settle, each as a numbered decision:**
- **How a new host gets its software:** a bootstrap from `box/system`, as today's box was set up,
  or an image (#0014). The bootstrap MUST include what the box got by hand, Rust and gcc among it.
- **Which project today's box becomes**, and so what its stack, its alias and its state become.
- **How the stack holds more than one instance**, or whether the driver creates hosts outside it,
  and what that asks of the IAM permissions the laptop's sign-in carries.
- **What the operator's commands act on** once there is more than one host. Today `up`, `down`,
  `status`, `sync` and `grow` act on the one box, with no argument naming a host.
- **The CA and the token push per host**: each new host trusts the CA and gets the Bot token, as
  the box does.
- **The cost guard before v0.08.** Lease-based stop arrives in v0.08, so until then every host a
  turn starts MUST be stopped at its close, as the box is (AGENTS.md); the architect's close of a
  turn names every host it used.
- **Live checks for the done-when.** A stand-in cannot show a second host being created; the
  stories' done-whens name the live check, and Terry's yes covers the cost of the instances it
  starts (D19).
- **Terry's sign-ins on a new host** (Claude Code, Codex, gh) are by hand, once per host, and the
  epic documents how; #0009 automates it later.

**The road after v0.07 is roadmap.md's:** v0.08 host lifecycle and the cost guard, v0.09 size
classes, v0.10 plan-sized leases and contract v1, v0.11 plan from the board, v0.12 both new
platforms, then v1.0.

## When Terry approves v0.06

**Done, 2026-11-20, by 10:04am Pacific**, after a board event showed #0055 and #0135 in Completed.
Kept as the record of this close; the procedure is [architect.md](architect.md#closing-a-release).
In it:
1. Fill every slot left for the close here, replace the draft note, and point AGENTS.md's "Where
   it stands" at this file.
2. Mark v0.06 done in roadmap.md's table: the operator, the ProxyCommand and the keeper in Rust,
   the laptop cut over, and no PowerShell left in the product.
3. List the Backlog cards in "What is open" for Terry, with their evidence, in one message.
4. Wait for Terry's word before laying out v0.07.

# agentspace roadmap

RFC 2119 keywords; the capitals are load-bearing.

**Where agentspace is going, from the shipped MVP (v0.01) to 1.0, and past it to ideas that may
never happen.** The reasoning behind each choice is in [decisions.md](decisions.md); the work itself
is on the `agentspace` localswim board, cited here as card numbers. **The Backlog MUST match this
page** (Terry, 2026-11-17: "Backlog needs to be accurate at all times"), so a change to either
changes both.

## The vision

**Terry, 2026-11-16: "a whole mini IaaS platform and API that you own"**, where "you" is the agent
working in this repo.

**Projects ask agentspace for compute, and agentspace decides where it runs.** A project requests a
lease and gets back a way to SSH in. It never knows or cares whether that lands on bare metal,
Azure, EC2, a Fargate container or anything else (D1). That freedom is the point: hosting choices
change as Terry's needs and cost tolerance change, and no client changes with them.

**Terry, 2026-11-17: scale horizontally.** A project asks for agent capacity with its platform,
profile, execution plan (stories laid out over turns) and staffing plan (how many subagents per
story, and each one's model and effort). agentspace gives it a host of its own, big enough to run
that plan (D25, D27). That is 1.0.

## v0.01: the MVP, shipped

**Done 2026-11-17: a project on the laptop gets an SSH host alias from `agentspace lease create`,
on one Graviton EC2 box, through a contract that will not change when the backend does.** Epics
#0001 to #0003, 17 stories, all merged; in E3.03 a swimcoach agent worked a card on its board from
a lease. Where it stands is [handoff-mvp.md](handoff-mvp.md), and the contract is
[lease-api.md](lease-api.md).

**Punted from the MVP, and where each went:**

| Punted | Now |
|---|---|
| Project isolation | A host per project, v0.07 (D25) |
| Lease-based idle stop | v0.08 |
| Size limits | Size classes, v0.09, sized from a plan in v0.10 |
| A second host and placement | A host per project, v0.07 |
| A per-lease worktree | After 1.0 (#0005) |
| Revocation | After 1.0 (#0010) |
| A hosted API | After 1.0 (#0011) |

## The road to 1.0

**Many small releases, one epic each, taken in order (D26).** Terry moved them into Ready For Work on
2026-11-17, and v0.04 (#0030) on 2026-11-18. v0.13 (#0081) was created there the same day, when
Terry placed Windows hosts before 1.0, and folded into v0.12 (#0051) on 2026-11-19. Each is broken
into stories only
when Terry says so (D14).

| Release | Epic | What | Done when |
|---|---|---|---|
| **v0.02** | #0045 | **Done 2026-11-18** (nine stories in five turns; [handoff-v0.02.md](handoff-v0.02.md)). **Unattended leases.** The gaps E3.03 found: #0042 (the desktop app clears the board forwards), #0044 (a dead client keeps them), #0028 (the token push rides Terry's own SSH entry). Then the box runs in the laptop's time zone (D32), and the keeper recovers from its own crash and from a dead holder, one keeper at a time (D30) | A desktop-app session on a lease reaches the board with nothing started by hand, and reports times in Terry's zone |
| **v0.03** | #0046 | **Done 2026-11-18** (four stories in three turns; [footprint.md](footprint.md), [the measured run](runs/e0.03-t02-s01.md)). **Measure what an agent uses.** CPU and memory per agent, and everything under it, while real stories run | A per-agent footprint table, measured on agentspace's own v0.04 turn |
| **v0.04** | #0030 | **Done 2026-11-18** (seventeen stories in six turns; [vision.md](vision.md), [lease-api.md](lease-api.md) and `schemas/`, [operator.md](operator.md), [state.md](state.md), [driver.md](driver.md), [keeper.md](keeper.md), [conformance.md](conformance.md) and `conformance/`; both final clean-room readers said yes, [the final pass](runs/e0.04-t06-s01.md)). **Clean-room spec package.** A vision doc and specs for every interface, so the rewrite never reads the prototype's code (D20) | A reviewer who cannot read the code agrees a drop-in could be built from them |
| **v0.05** | #0054 | **Done 2026-11-19** (fifteen stories in five turns; [handoff-v0.05.md](handoff-v0.05.md), the Cargo workspace in `crates/`, [the conformance run](runs/e0.05-t04-s01.md), 786 passed, and [the install through the real box](runs/e0.05-t05-s01.md)). **The lease CLI in Rust** (D28), contract v0 unchanged (D38): the portable core on the box, measured; the Windows file layer; the driver and the commands; the full conformance run; the install, now the `agentspace` on Terry's `PATH` | The Rust CLI meets lease-api.md and replaces the PowerShell one, as the installed `agentspace` (D37, D38) |
| **v0.06** | #0055 | **Done 2026-11-20** (twenty-six stories in six turns; [handoff-v0.06.md](handoff-v0.06.md), [the measured turn](runs/e0.06-t02-s08.md), [the conformance run](runs/e0.06-t04-s01.md), 1,439 passed, [the cutover](runs/e0.06-t05-s01.md), [the deploy fix](runs/e0.06-t05-s02.md) and [the prototype's removal](runs/e0.06-t06-s01.md)). **The operator in Rust; PowerShell retired** (D41, D42): the operator, the ProxyCommand and the keeper as Rust programs, installed on Terry's laptop with his ssh entry and keeper task cut over, and every PowerShell and `cmd` file of the prototype removed | A lease connects through the Rust ProxyCommand, and no PowerShell is left in the product |
| **v0.07** | #0047 | **Done 2026-11-21** (twenty-one stories in six turns; [handoff-v0.07.md](handoff-v0.07.md), [the whole-suite run](runs/e0.07-t04-s04.md), 2,640 passed, [the first new host, live](runs/e0.07-t05-s01.md) and [an agent on it](runs/e0.07-t06-s01.md); defects #0163 and #0164 and hardening #0166 and #0167 carried in Backlog). **A host per project** (D25). One size, one platform | A second project's first lease creates its own host and works a card there |
| **v0.08** | #0048 | **Done 2026-11-23** (twenty-one stories in six turns; [handoff-v0.08.md](handoff-v0.08.md), [the whole-suite run](runs/e0.08-t04-s04.md), 2,640 passed, [the install and host-side checks](runs/e0.08-t05-s01.md), and the lifecycle live on [the box](runs/e0.08-t06-s01.md) and [agentspace's host](runs/e0.08-t06-s02.md); findings #0195 and #0196). **Host lifecycle and cost guard.** A host runs only while its project holds a live lease | The last release stops the host, and no host can run unnoticed |
| **v0.09** | #0049 | **Size classes.** `s` to `xl`, set by resizing a stopped host | `--size l` gets a bigger host, sized from v0.06-T01's whole compiling turn (D64, D65) |
| **v0.10** | #0050 | **Plan-sized leases; contract v1.** Peak concurrency times footprint picks the size (D27) | Two plans of different peak concurrency get different sizes |
| **v0.11** | #0052 | **Plan from the board.** The plan is read from the project's localswim epic | Naming an epic is enough to get a sized lease |
| **v0.12** | #0051 | **`linux/amd64` and `windows/amd64`**, the second and third platforms, in parallel (D4, D13, D26). One shared turn makes the driver serve more than one platform, with a seam per platform; then a `linux/amd64` track and a Windows track (a Windows host, its lease identity and its host agent) run side by side | `--platform linux/amd64` gives a ready lease, and `--platform windows/amd64` gives a ready lease where an agent works a card |
| **v1.0** | #0053 | **A real project runs an epic on a plan-sized host** | A real project other than agentspace works a multi-turn epic through it |

**Why this order:**
- **v0.02 and v0.03 come first** because they are small, mostly on the box, and fix what blocks real
  use today. v0.03's numbers are what v0.09 and v0.10 size from, once v0.09 has measured them
  again on a turn that compiles (D33).
- **The Rust rewrite (v0.04 to v0.06) comes before the hosts**, because every release from v0.07 on
  rewrites the driver, and doing that in PowerShell first would mean writing it twice (D28).
- **A host per project comes before sizes and plans,** because a size means nothing until a project
  has a host of its own to size.
- **Plan from the board comes before `linux/amd64`** (Terry, 2026-11-19, D26), so agentspace is
  feature complete on one platform before it turns to others. Neither needs the other's code, since
  a plan read from an epic names no platform, but they run in serial: on the board #0052 blocks
  #0051, and #0051 blocks #0053 (Terry, 2026-11-19, 4:35pm Pacific, moving v0.11's link from v1.0
  to v0.12), so the releases from v0.10 to v1.0 are one chain (AGENTS.md, The board).
- **Both new platforms are one release, built in parallel** (Terry, 2026-11-19, D26). Windows
  needs the driver to serve more than one platform, not the amd64 host, so that is v0.12's shared
  first turn, and the two platforms' tracks run side by side after it. Terry placed Windows before
  1.0 on 2026-11-18: agentspace is a multi-platform service (D4). It was v0.13 (#0081) until it
  was folded in, and v0.13 is not used.

**Two version numbers, kept apart (D26):** the release (v0.02 and on) and the lease contract
(lease-api.md). Contract v0 stands until v0.10, the first release that changes an output.

**Hardening that rides with a release:**
- #0029 and #0041 (the lease CLI's) are carried into v0.04's specs and met by v0.05.
- #0027, #0034, #0066 and #0068 (the operator's) are carried into v0.04's specs and met by v0.06.
- #0043 (the CloudWatch backstop) is OBE by v0.08: the backstop stays, lengthened from 3 hours to 24 (D58), behind the guard's lease-based stop.
- #0064 (a release leaves the lease's open connections up, found in E0.02-T03-S01) is met by v0.08, shown live with Terry's desktop-app session in E0.08-T06-S02; a proxy that outlived the release by seconds is #0196.
- **The open defects and hardening are folded into the epics that carry them, and their cards
  archived** (Terry, 2026-11-23; D72): #0189, #0191, #0196, #0137's last part and #0195's decision into v0.10 (#0050);
  #0192, #0193, and #0195's build if it is code, into v0.12 (#0051); #0190, #0082 and #0109 into
  v1.0's first turn (#0053), before the real project measures its own turn. #0027 (OBE by v0.06)
  and #0194 (met) are Completed.
- #0209 (the lease CLI's isolation guard saw its moved home created once under load, found at
  v0.09-T02's close) waits for Terry's placement; the architect recommends a diagnosis story in
  v0.09-T03.
- #0110 (conformance hardening found in v0.05-T04: checks at the call, and the cases the suite
  lacks, among them #0029 point 2 and #0083 point 1) rides with v0.06 (#0055), which leans on the
  same harness.

## After 1.0

**Designed in direction, not in detail.** Each is a Backlog card.

| Card | What |
|---|---|
| #0005 | A git worktree per lease, if Claude Code's own worktrees do not already cover it |
| #0008 | Board access for two or more leases on one host |
| #0009 | Credentials per project, without Terry signing in on each new host |
| #0010 | Revoking a released lease's certificate |
| #0011 | A hosted control plane, so callers other than the laptop can hold leases |
| #0014 | Versioned profiles as baked images; hosts are replaced, not patched |
| #0015 | A per-lease container driver (Fargate or Azure Container Instances) |
| #0016 | More backends: Azure VMs, bare metal, a Mac mini at home |
| #0017 | A placement policy that turns cost and start-latency hints into backends |
| #0032 | Hardening the CA trust on each host |

## Someday, maybe

**Fuzzy, uncarded, and allowed to stay that way.** An idea moves to a card when a real need shows
up.

- **GPU sizes**, for when an agent needs to run a model rather than call one.
- **Requests that state what an agent needs, not only a profile's name**, such as a libc of
  `glibc`, `musl` or `any` matched against what each profile has (D4).
- **Board-driven leases.** v0.11 reads a plan from the board; this goes further: moving an epic to
  In Progress asks for its lease, and closing it releases the lease.
- **Sub-leases.** An agent on a lease asks for more compute for its own helpers.
- **Per-project budgets.** What each project's host costs, and a cap that turns into
  `unsatisfiable`.
- **Other people.** Leases for someone who is not Terry, which would make every "Terry's laptop"
  assumption in this repo a bug.

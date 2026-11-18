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
2026-11-17, except v0.04 (#0030), which is still in Backlog. Each is broken into stories only
when Terry says so (D14).

| Release | Epic | What | Done when |
|---|---|---|---|
| **v0.02** | #0045 | **Unattended leases.** The gaps E3.03 found: #0042 (the desktop app clears the board forwards), #0044 (a dead client keeps them), #0028 (the token push rides Terry's own SSH entry). Then the box runs in the laptop's time zone (D32), and the keeper recovers from its own crash and from a dead holder, one keeper at a time (D30) | A desktop-app session on a lease reaches the board with nothing started by hand, and reports times in Terry's zone |
| **v0.03** | #0046 | **Measure what an agent uses.** CPU and memory per agent, and everything under it, while real stories run | A per-agent footprint table, measured on a real turn |
| **v0.04** | #0030 | **Clean-room spec package.** A vision doc and specs for every interface, so the rewrite never reads the prototype's code (D20) | A reviewer who cannot read the code agrees a drop-in could be built from them |
| **v0.05** | #0054 | **The lease CLI in Rust** (D28), contract v0 unchanged | The Rust CLI meets lease-api.md and replaces the PowerShell one |
| **v0.06** | #0055 | **The operator in Rust; PowerShell retired** | A lease connects through the Rust ProxyCommand, and no PowerShell is left in the product |
| **v0.07** | #0047 | **A host per project** (D25). One size, one platform | A second project's first lease creates its own host and works a card there |
| **v0.08** | #0048 | **Host lifecycle and cost guard.** A host runs only while its project holds a live lease | The last release stops the host, and no host can run unnoticed |
| **v0.09** | #0049 | **Size classes.** `s` to `xl`, set by resizing a stopped host | `--size l` gets a bigger host |
| **v0.10** | #0050 | **Plan-sized leases; contract v1.** Peak concurrency times footprint picks the size (D27) | Two plans of different peak concurrency get different sizes |
| **v0.11** | #0051 | **`linux/amd64`** as a second platform (D13) | `--platform linux/amd64` gives a ready lease |
| **v0.12** | #0052 | **Plan from the board.** The plan is read from the project's localswim epic | Naming an epic is enough to get a sized lease |
| **v1.0** | #0053 | **A real project runs an epic on a plan-sized host** | A real project other than agentspace works a multi-turn epic through it |

**Why this order:**
- **v0.02 and v0.03 come first** because they are small, mostly on the box, and fix what blocks real
  use today. v0.03's numbers are what v0.09 and v0.10 size from.
- **The Rust rewrite (v0.04 to v0.06) comes before the hosts**, because every release from v0.07 on
  rewrites the driver, and doing that in PowerShell first would mean writing it twice (D28).
- **A host per project comes before sizes and plans,** because a size means nothing until a project
  has a host of its own to size.

**Two version numbers, kept apart (D26):** the release (v0.02 and on) and the lease contract
(lease-api.md). Contract v0 stands until v0.10, the first release that changes an output.

**Hardening that rides with a release:**
- #0029 and #0041 (the lease CLI's) are carried into v0.04's specs and met by v0.05.
- #0027, #0034, #0066 and #0068 (the operator's) are carried into v0.04's specs and met by v0.06.
- #0043 (the CloudWatch backstop) is replaced by v0.08's lease-based stop.
- #0064 (a release leaves the lease's open connections up, found in E0.02-T03-S01) is met by v0.08.

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

- **`windows/amd64` leases.** The lease model assumes a Linux user and an SSH certificate; Windows
  would need its own host agent.
- **GPU sizes**, for when an agent needs to run a model rather than call one.
- **Board-driven leases.** v0.12 reads a plan from the board; this goes further: moving an epic to
  In Progress asks for its lease, and closing it releases the lease.
- **Sub-leases.** An agent on a lease asks for more compute for its own helpers.
- **Per-project budgets.** What each project's host costs, and a cap that turns into
  `unsatisfiable`.
- **Other people.** Leases for someone who is not Terry, which would make every "Terry's laptop"
  assumption in this repo a bug.

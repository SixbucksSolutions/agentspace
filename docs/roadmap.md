# agentspace roadmap

RFC 2119 keywords; the capitals are load-bearing.

**Where agentspace is going, from the MVP being built now to ideas that may never happen.** Each
horizon is less certain than the one before it. The reasoning behind each choice is in
[decisions.md](decisions.md); the work itself is on the `agentspace` localswim board, cited here as
card numbers.

## The vision

**Terry, 2026-11-16: "a whole mini IaaS platform and API that you own"**, where "you" is the agent
working in this repo.

**Projects ask agentspace for compute, and agentspace decides where it runs.** A project requests a
lease and gets back a way to SSH in. It never knows or cares whether that lands on bare metal,
Azure, EC2, a Fargate container or anything else (Terry, 2026-11-16). That freedom is the point:
hosting choices change as Terry's needs and cost tolerance change, and no client changes with
them.

Today agentspace is one Graviton EC2 instance, built by hand for swimcoach's agents
([how-it-was-built.md](how-it-was-built.md)). The roadmap turns it into the first host behind a
lease API.

## MVP: leases on the box that exists

**Goal: a project on the laptop gets an SSH host alias from `agentspace lease create`, on today's
single EC2 box, through a contract that will not change when the backend does.** Everything
that can safely wait, waits (Terry: "aggressively lean").

| Epic | Card |
|---|---|
| Lease contract v0 and the `agentspace lease` CLI | #0001 |
| Per-lease SSH certificates and host aliases | #0002 |
| A real project runs its agents on a lease | #0003 |

**Epic 2 runs first, on purpose:** every unknown that could change the contract lives in it, while
Epic 1 is a CLI that writes JSON. Its stories are E2.01 to E2.09; Epic 1's are E1.01 to E1.05,
and Epic 3's are E3.01 to E3.03. They run in turns T1 to T8 (decision D14).

**The contract sketch.** Epic #0001 MUST pin it down in `docs/lease-api.md`; this is the starting
point, not the spec.

```
agentspace lease create --project swimcoach [--platform linux/arm64] [--profile agent-linux] [--size m] [--ttl 8h]
agentspace lease get <id>
agentspace lease renew <id> [--ttl 8h]
agentspace lease release <id>
agentspace lease list [--project swimcoach]
```

Every command prints JSON. A lease is `pending`, `ready`, `released`, `expired` or
`unsatisfiable`, and a ready lease carries an SSH host alias, its user and its expiry. It MUST NOT
carry anything that names the backend.

**In the MVP:**
- The lease CLI on the laptop, with lease state in a local file.
- An agentspace SSH CA; each lease gets its own key and a certificate that expires with it.
- A host alias per lease, through the existing start-on-connect Session Manager path.
- `platform`, `profile` and `size` in the contract, accepting only the defaults
  (`linux/arm64`, `agent-linux`, `m`). Anything else is `unsatisfiable`.

**Punted, on purpose:**
- **Project isolation.** Every project still lands as the `ubuntu` user, as today.
- **Per-lease worktrees, size limits and lease-based idle stop.** Today's idle check stays.
- **Revocation.** Releasing a lease deletes its key; its certificate stays valid until it
  expires.
- **A hosted API, a second host and placement.** The pool is one box.

**The MVP is done when a real project runs its agents through a lease (#0003)**, not when the CLI
works.

## Next: make one host properly multi-tenant

**Clear what and why, open how.** Each is a Backlog card.

| Card | What |
|---|---|
| #0004 | One Unix user per project, with its own home and secrets |
| #0005 | A git worktree per lease |
| #0006 | Lease-based idle stop: the host stops when it holds no live leases |
| #0007 | Size classes as cgroup slices |
| #0008 | Board access that survives many leases and per-project users |
| #0009 | Credentials per project, without signing in on every host |
| #0010 | Revoking a released lease's certificate |

## Later: more than one host, more than one backend

**Designed in direction, not in detail.** Each is a Backlog card.

| Card | What |
|---|---|
| #0011 | A hosted control plane, so callers other than the laptop can hold leases |
| #0012 | Placement across a pool of hosts |
| #0013 | `linux/amd64` as a first-class second platform |
| #0014 | Versioned profiles as baked images; hosts are replaced, not patched |
| #0015 | A per-lease container driver (Fargate or Azure Container Instances) |
| #0016 | More backends: Azure VMs, bare metal, a Mac mini at home |
| #0017 | A placement policy that turns cost and start-latency hints into backends |
| #0030 | The clean-room handoff: a vision doc and pedantic interface specs, so a rewrite never reads the prototype's code (D20) |

## Someday, maybe

**Fuzzy, uncarded, and allowed to stay that way.** An idea moves to a card when a real need shows
up.

- **`windows/amd64` leases.** The lease model assumes a Linux user and an SSH certificate; Windows
  would need its own host agent.
- **GPU sizes**, for when an agent needs to run a model rather than call one.
- **Board-driven placement.** Moving a localswim card to In Progress asks for a lease for that
  card's project, and closing it releases the lease.
- **Sub-leases.** An agent on a lease asks for more compute for its own helpers.
- **Per-project budgets.** What each project's leases cost, and a cap that turns into
  `unsatisfiable`.
- **Other people.** Leases for someone who is not Terry, which would make every "Terry's laptop"
  assumption in this repo a bug.

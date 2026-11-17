# agentspace decisions

RFC 2119 keywords; the capitals are load-bearing.

**Every design decision about the lease API, numbered so cards and docs can cite it.** A decision
is never deleted: when one changes, its status says what replaced it and why. The roadmap that
these decisions serve is in [roadmap.md](roadmap.md).

**Statuses:**
- **Decided** holds for the MVP and after it.
- **MVP only** holds for the MVP and names what replaces it later.
- **Direction** is where things are headed, binding on design now, and built later.
- **Open** questions are listed at the end, each with the card that settles it.

## The interface

### D1. Clients never know the backend

**Decided, Terry, 2026-11-16.** A client MUST NOT be able to tell, and MUST NOT need to know,
whether its lease landed on bare metal, Azure, EC2, a Fargate container or anything else. Nothing
in a request or a response MAY name a backend, an instance type, a region or a host.

**Why:** hosting changes as Terry's needs and cost tolerance change, and that MUST NOT ripple into
every project that uses agentspace.

### D2. The unit is a lease

**Decided.** A project asks for a lease: compute for one agent, for a bounded time. A lease
answers with a way to SSH in and an expiry. Placement, meaning which host or container serves it,
is agentspace's business alone.

**Why:** a lease is the one thing every backend can provide, from a user on a shared VM to a
container of its own.

### D3. SSH credential per agent, isolation per project

**Direction; MVP only for the isolation half.** Each lease gets its own SSH key and certificate,
so each agent's access can be told apart, expired and audited on its own. The isolation boundary
is the project: one Unix user per project, with its own home and secrets, shared by that
project's agents.

**In the MVP every project lands as `ubuntu`**, as agents do today. All projects are Terry's and
already share the box this way, so isolation is safe to punt. Card #0004 adds it.

**Why per project and not per agent:** agents in one project share a repo, toolchain caches and
sign-ins. A user per agent would mean signing every tool in once per agent.

### D4. Platform as `os/arch`; software as a named profile; no distro field

**Direction.** A request MAY name a `platform` in the OCI form (`linux/arm64`, `linux/amd64`,
`darwin/arm64`) and a `profile`, a named and versioned software contract such as `agent-linux@3`
(distro, libc, Node LTS, uv, Claude Code, Codex, gh). The distro MUST NOT be a field of its own;
it lives inside a profile.

**Why:** a bare distro name promises almost nothing a caller depends on. What callers depend on is
libc and the toolchain: Alpine was ruled out for this box because workerd and Playwright do not run
on musl, not because it was Alpine. A distro field would also invite arch × distro × version
combinations that nobody tests.

### D5. An unmet request is unsatisfiable, never quietly downgraded

**Decided.** A request no backend can meet MUST end in `unsatisfiable` with a reason. Placement
MUST NOT hand back a different platform, profile or size than was asked for.

**In the MVP only the defaults are supported:** `linux/arm64`, `agent-linux` and size `m`. The
fields are in the contract from day one, so adding values later changes no client.

### D6. Drivers place leases, not hosts

**Direction.** A backend is a driver with one contract: place this lease, return its SSH
endpoint, tear it down. Drivers come in two kinds:
- **Pooled:** EC2, Azure VMs, bare metal. Leases are packed onto shared hosts, where a host agent
  sets up the user, worktree and limits.
- **Per-lease:** Fargate, Azure Container Instances. One lease is one container.

**Why:** a contract written around hosts ("power on, run the host agent") does not fit
container backends, where there is no host to own.

**In the MVP there is one pooled driver: the existing EC2 box.** The CLI's code MUST keep it
behind a driver boundary anyway, so a second driver means adding code, not reworking it.

### D7. SSH access through short-lived certificates from an agentspace CA

**Decided.** agentspace signs each lease's key with its own SSH CA. The certificate's principal is
the host user, its key ID is the lease ID, and it expires with the lease. Hosts trust the CA
through `TrustedUserCAKeys`.

**Why:**
- A certificate can be issued while the host is stopped; an `authorized_keys` entry would need the
  host running.
- Expiry needs no cleanup on the host.
- sshd logs the key ID, so every connection names its lease.

**The CA key MUST NOT be committed**, since this repo is public. Card #0002 decides where it lives
on the laptop; open question Q1 covers where it lives once the API is hosted.

### D8. The MVP control plane is a CLI on the laptop

**MVP only.** `agentspace lease ...` runs on the laptop, prints JSON, and keeps lease state in a
local file. There is no server to run.

**Why:** today's callers are all projects on the laptop, which already call `localswim` the same
way.

**Replaced by card #0011**, a hosted API, once a caller that is not on the laptop needs a lease. The
CLI then becomes a client of it, and its JSON stays the same.

### D9. Reachability belongs to the driver

**Decided.** How to reach a lease (the ProxyCommand inside its host alias) MUST come from the
driver that placed it. EC2 uses Session Manager; another backend brings its own path.

**In the MVP that path is today's `scripts/ssh-proxy.cmd`**, which starts the box on connect.

## What the MVP leaves alone

### D10. Releasing a lease does not revoke its certificate

**MVP only.** Release deletes the lease's key on the laptop, and the certificate stays valid until
it expires. TTLs are short, so the exposure is bounded. **Replaced by card #0010**, which pushes a
revocation list (KRL) to hosts.

### D11. Today's idle stop stays

**MVP only.** The box still powers off after 60 minutes with no SSH or Session Manager session and
no CPU work. An agent holding a lease but doing nothing can still have the box stopped under it;
its next connection starts it again. **Replaced by card #0006**, which stops a host when it has no
live leases.

### D12. Board access is unchanged

**MVP only.** The first `ssh` connection still binds the localswim board's reverse forward for the
whole box, and every lease uses it, because every lease is `ubuntu`. **Replaced by card #0008**,
which must land with or before per-project users (#0004), since those break the shared token file.

## Defaults

### D13. arm64 is the default platform

**Decided.** When a request names no platform, it gets `linux/arm64`, because Graviton costs less
for the same agent work. `linux/amd64` is the planned second platform (card #0013), not a
fallback.

## How the work is run

### D14. Roadmap horizons, epics before stories

**Decided, Terry, 2026-11-16.** The roadmap runs from the MVP through Next and Later to Someday.
MVP epics go in Ready For Work, and post-MVP work goes in Backlog. An epic MUST NOT be broken into
stories until Terry says it is time for that epic.

**Stories, Terry, 2026-11-16:**
- **A story's title is `N.nn: title`**, where `N` is its epic's card number and `nn` is a
  zero-padded, 1-based number unique within the epic, for example `2.01`.
- **A story MUST be under 3 hours of estimated active work to reach Ready For Review.** One that
  cannot be broken down that far MUST go to Terry for discussion, not onto the board as is.
- **Stories go in Ready For Work, as children of their epic**, with their estimate and
  dependencies in the description.
- **An epic moves to In Progress when its stories are laid into Ready For Work, and stays there
  until its last story reaches Ready For Review.** Its description carries the execution plan by
  turn: each story starts in the first turn its entry gates are met, and the stories in one turn
  MUST be workable in parallel, in isolation from each other.

#0002 was broken down first, into 2.01 to 2.08 (#0018 to #0025), with its two risk-first spikes at
the front. Someday ideas stay uncarded in
[roadmap.md](roadmap.md) until a real need shows up.

## Open questions

| | Question | Settled by |
|---|---|---|
| **Q1** | Where do the CA key and lease state live once the API is hosted? A KMS-backed signer? DynamoDB? | #0011 |
| **Q2** | Where do per-project credentials (Claude Code, Codex, gh) live, so a fresh host or container needs no sign-in? Needs Terry. | #0009 |
| **Q3** | How do many leases, and later many project users, reach the laptop's localswim board? | #0008 |
| **Q4** | One overlay network across all backends (WireGuard, Tailscale, Cloudflare Tunnel), or a reachability path per driver? | #0016 |
| **Q5** | Is the lease CLI PowerShell, like the scripts here, or Go, like localswim, giving one binary for Windows and Linux? Also its default and maximum TTL. | #0001 |

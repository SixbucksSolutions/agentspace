# agentspace vision

RFC 2119 keywords; the capitals are load-bearing.

**What agentspace is for, who uses it, what it promises them, and what it will never do, written so
that it needs no code and no other doc to be understood (D20).** This page is the vision's one home
(D34). It cites the reasons for what it says as numbered decisions in
[decisions.md](decisions.md), as `D<n>`, and never overrides them. The order of the releases is
[roadmap.md](roadmap.md)'s, and the contract a caller programs against is
[lease-api.md](lease-api.md)'s.

## What agentspace is

**agentspace is compute on demand for coding agents.** A project asks it for a lease: compute for
one agent, for a bounded time. The lease answers with a way to log in over SSH and the time it
expires (D2). Where that compute runs, and on what, is agentspace's business alone (D1).

**It exists so that Terry's agents can work off his laptop, on compute that fits the work, without
any project knowing what hosts it.** Hosting choices change as his needs and cost tolerance change:
a cloud VM today, perhaps a container, bare metal or a machine at home later. None of that MAY
ripple into the projects that use agentspace (D1). In Terry's words of 2026-11-16, it is "a whole
mini IaaS platform and API that you own", where "you" is the agent that runs agentspace.

**It scales out, not up.** Each project gets a host of its own, kept between its leases, and big
enough for the work the project plans to run there (D25). By 1.0 a project describes its plan, and
agentspace chooses the size from it (D27).

**It serves one person.** Every project agentspace serves is Terry's (D3). Leases for anyone else
are an idea for some day, not a plan (roadmap.md, "Someday, maybe").

## Words used here

| Word | Means |
|---|---|
| Project | A codebase and its work. Every lease is for exactly one project. |
| Lease | Compute for one agent for a bounded time: a way to log in, and an expiry (D2). |
| Host | The machine or container a lease logs in to. A caller never learns which one (D1). |
| Backend | Where hosts come from: a cloud's VMs, containers, bare metal (D6). |
| Driver | The part of agentspace that places leases on one backend, and supplies the way to reach them (D6, D9). |
| Platform | An operating system and architecture, such as `linux/arm64` or `windows/amd64` (D4). |
| Profile | A named, versioned promise about a host's software: its libc, its toolchain and its agent tools (D4). |
| Size | How much compute a lease's host has. |
| TTL | How long a lease's credential is valid from the moment it is issued (D24). |
| Plan | A project's stories laid out over turns, and how many agents work at once in each, on which model (D25, D27). |
| Turn | One step of a plan: stories worked at the same time by a lead agent and its helpers. |
| Board | Terry's card board, localswim, which runs on his laptop. Agents take and report their work there (D12). |
| The laptop | Terry's Windows laptop, where agentspace's control plane runs today (D8, D21). |

## Who uses it, and how

**Four kinds of user, each with its own promises, and the agent that runs it.** The promises every caller gets are in "What every
caller is promised" below, and are not repeated under each role.

### Terry

**Terry owns agentspace and every project it serves, and pays for what it runs.** His projects'
agents ask for leases on his behalf; he is asked only for what only he can do.

How he uses it:
- **He signs the agent tools in once per project host**, at its first use, rather than once per
  lease (D25).
- **He says yes or no to persistent configuration on his laptop**, such as the keeper's logon task
  (D30), and to any call that would be costly to reverse (D19, D23).
- **He starts each release by hand** (D29), and 1.0 is done when a real project has worked through
  agentspace, not when the code works (D26).

What agentspace promises him:
- **No project changes when hosting does** (D1).
- **A host runs only while its project holds a live lease** (D25, built in v0.08). Until then,
  today's box stops itself when nobody uses it (D11).
- **Sizes come from measurement, never from an estimate** (D27, D33), so what he pays for follows
  what agents use.
- **Every login names its lease**, so a host's log says which lease connected (D3, D7).
- **Agents on a lease report times in his laptop's time zone** (D32).
- **The keeper never stops a process on his laptop that it did not start** (D30).
- **The key that signs lease credentials is never committed** (D7), and today it stays on his
  laptop, readable by him alone (D17, D21).

### A project's architect

**A project's architect is the agent that plans a project's work, and the caller that asks
agentspace for leases.** It lays the project's stories out over turns, and each turn needs compute
for the agents that will run it.

How it uses it:
- **It asks for a lease for its project**, naming the platform, profile, size and TTL it needs, or
  taking each one's default (D4, D5, D13, D24). Once plans are supported it MAY hand over the plan
  instead and let agentspace choose the size (D27).
- **It renews the lease while the work runs, and releases it when the work is done**
  (lease-api.md, "Commands").
- **It reads everything it needs from the lease:** the name to give `ssh`, the user to log in as,
  and the expiry (lease-api.md, "Fields").

What agentspace promises it, beyond what every caller gets:
- **A host of its project's own, kept between leases**, with the project's checkout, sign-ins and
  caches still on it, and no other project's lease on it (D25, built in v0.07). Today every
  project shares one box (D3).
- **Its checkout on that host** once hosts are per project (D3, E3.03 finding). Today the project
  puts its own repo on the box.
- **A size that fits its plan:** the most agents working at once in any turn, times a measured
  footprint per agent, and never more or less because of the models or effort levels the plan
  names (D27).

### A dev lead

**A dev lead is the agent session that runs one turn of a plan from a lease.** It works on the host,
and the helper agents (subagents) it launches for the turn's stories run inside its session there
(D33).

How it uses it:
- **It connects through the lease's host alias**, from a terminal or from the Claude desktop app's
  SSH environment, and opens its session in the project's checkout on the host (D9).
- **It takes and reports work on the board from the host** (D12, D30).

What agentspace promises it:
- **Room for its whole turn:** the host is sized for the busiest turn of the plan (D27). Today every
  lease gets the one size there is (D5).
- **The board, with nothing started by hand.** While its lease is ready, a keeper on the laptop
  holds the board's connection to the host (D30).
- **Its tools as the profile states them**, including which libc its binaries can rely on, so it
  never has to probe the host to find out (D4).

### An agent on a lease

**An agent on a lease is any coding agent logged in to a host through a lease:** a dev lead's
session, or one agent working alone. The profile names the agent tools a host has, such as Claude
Code, Codex and gh (D4).

How it uses it:
- **It logs in with the lease's credential, as the user the lease names**, and reads that user from
  the lease rather than assuming it (lease-api.md, "Fields").
- **It treats an unreachable board as a reason to stop and report**, never as a reason to work
  without a card (README, "The localswim board on the box").

What agentspace promises it:
- **A credential of its own lease:** a key and a short-lived certificate, whose expiry is the
  lease's, and which name the lease in every login (D3, D7).
- **A host whose software is what its profile says**, whatever the platform (D4).
- **Times in Terry's zone**, so the clock it reads is the one he reads (D32).

### Who runs agentspace

**agentspace is run by an agent, its own architect, which owns its design, its code and its hosts on
Terry's behalf** (roadmap.md, "The vision"; D16). agentspace's own work runs in releases, turns and
stories like any project's (D14, D16, D26, D29, D31). Today a host is configured only from
agentspace's own repo (README, "Rules"): a project's agent MUST NOT reconfigure it, and reports a
missing tool to agentspace's architect (README, "Measure a turn").

## What every caller is promised

- **One contract, whatever serves the lease.** Nothing in a request, a response, an error or a help
  text names a backend, a machine type, a region or a host (D1).
- **A stable contract.** A change that could break a caller is a new contract version, and a
  release number is not a contract version (D26; lease-api.md, "Versioning").
- **Exactly what it asked for, or a clear no.** A request that no backend can meet ends
  `unsatisfiable`, with a reason per unmet value, and is never placed on a near match (D4, D5). A
  TTL out of range is refused, never clamped (D24).
- **Strict output, tolerant input.** Every output validates against a published JSON Schema. Input
  is accepted in any reasonable spelling and normalized, and a value that is ambiguous or unsafe is
  refused, never guessed at (D20).
- **A standard way in.** A lease is reached by its host alias with any SSH client that reads the
  OpenSSH client configuration, and the path to the host is the driver's concern, not the caller's
  (D9; lease-api.md, "Fields").
- **A bounded credential.** A lease's current certificate expires when the lease does, and no
  certificate is valid for longer than the maximum TTL, however often its lease is renewed (D7,
  D24).

## What it will never do

**These hold for as long as the decision cited holds.** Where one is not yet built, it says so.

- **Tell a caller what serves its lease** (D1).
- **Hand back a different platform, profile or size than was asked for**, or a shorter or longer
  TTL (D5, D24).
- **Guess at an input that could mean two things** (D20).
- **Choose a host's size from a model or an effort level, or from an estimate** (D27).
- **Put two projects' leases on one host** (D25), once hosts are per project in v0.07. Today's one
  box is shared by every project (D3).
- **Assume a host is Linux.** A platform is a parameter of the contract, and `windows/amd64` is a
  platform like any other (D4).
- **Commit the CA's key** (D7, D17).
- **Stop a process on Terry's laptop that it did not start** (D30).

**And not before 1.0, by the roadmap's order** (roadmap.md, "After 1.0"):
- **Revoke a released lease's certificate, or end a session that is already open.** Release removes
  the lease's key and alias on the laptop, and its certificate stays valid until it expires (D10).
- **Serve a caller that is not on the laptop** (D8).
- **Give each lease a fresh host, or sign a new host in without Terry** (D25; decisions.md, Q2).

## Today, and the road to 1.0

**Today agentspace is one Linux box, `<box>`, described here as it is, not as the contract
requires** (D4). It is a Graviton EC2 instance that every project shares, and every lease logs in
to it as the same user (D3). It stops itself when nobody uses it, and the next connection starts it
again (D9, D11). The control plane is a command-line tool on the laptop that prints JSON (D8), and
the CA that signs lease credentials is a file there (D17, D21). The board reaches the box through a
connection a keeper on the laptop holds (D30). Only the default platform, profile and size are
served, and any other is `unsatisfiable` (D5).

**The road to 1.0 goes in this order**, release by release, as roadmap.md, "The road to 1.0",
gives and explains:
- **Specs, then Rust.** The prototype is replaced by a Rust rewrite built from specs alone (D19,
  D20, D28, D34).
- **A host per project**, then a host that runs only while its project holds a lease (D25).
- **Sizes, then sizes chosen from a plan** (D27): first a plan handed over, then one read from the
  board.
- **More platforms:** `linux/amd64`, then `windows/amd64` (D4, D13).
- **1.0: a real project, not agentspace itself, works a multi-turn epic on a host sized from its
  plan** (D26; roadmap.md, "The road to 1.0").

What comes after 1.0 is in roadmap.md, "After 1.0" and "Someday, maybe".

## How it is made

- **Plan to throw one away.** The MVP was a prototype built to find the requirements. Its behavior,
  as seen from outside, is what the rewrite is held to (D19).
- **A clean room.** The team that rebuilds agentspace MUST NOT read the prototype's code. Intent
  lives in this page, the decisions and the roadmap, and each interface has one reference doc that
  owns it (D20, D34).
- **Rust by v0.06** (D28).
- **Small releases, one epic each, taken in order** (D26).
- **Every design choice is a numbered decision**, and a changed decision says what replaced it
  rather than being deleted (decisions.md, "Statuses").

## Open questions

**What reading could not settle.** The clean-room review (E0.04-T03-S01) closes them.

| | Question | Why reading could not settle it |
|---|---|---|
| **V1** | What does "one agent" mean in D2 once a lease carries a plan? | D2 makes a lease compute for one agent. D25 and D27 size a project's host for a plan's peak concurrency, and D33 counts a dev lead's subagents inside its one session. No decision says whether a turn takes one lease, for its dev lead, or one per agent session. |
| **V2** | How does a lease on a Windows host log in? | D2 decides that a lease answers with a way to SSH in. D4 says the lease model today assumes a Linux user and an SSH certificate, and that v0.13 (#0081) builds a Windows host's lease identity. Whether a certificate login stays the way in, and what user a lease names there, is not decided. |
| **V3** | Do the MVP-only decisions whose replacements come after 1.0 still bind through 1.0? | D8 (the control plane on the laptop), D10 (no revocation), D17 (the CA on the laptop) and D24's maximum TTL are marked MVP only, but what replaces them, #0010 and #0011, is after 1.0 in roadmap.md. This page assumes they hold until then. |
| **V4** | Does work on a lease depend on the laptop being awake? | The board runs on the laptop and reaches a host only through a connection it holds (D12, D30), and the control plane and CA are on the laptop (D8, D17). Today an agent stops when the board goes away (README, "The localswim board on the box"). No decision says whether that is meant to hold at 1.0. |
| **V5** | Who starts a session on a lease? | A lease's caller creates it, and a dev lead connects through the alias (D9, D30). No decision says how a project's architect hands a lease to its dev lead, or whether adding the desktop app's SSH environment and starting the session there needs Terry's hand. |
| **V6** | Does agentspace promise a time to ready? | No decision states how long a caller waits from asking for a lease to its first login, for a stopped host or for one created by that lease (D25). |
| **V7** | How does agentspace learn which repo to check out on a project's host? | D3's E3.03 finding says v0.07 puts the checkout on the project's host. Neither a decision nor the lease contract says where agentspace gets the repo's location from. |

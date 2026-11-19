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

**It exists so that each of Terry's projects gets compute that fits its work, without any project
knowing what hosts it** (D1, D25). Hosting choices change as his needs and cost tolerance change: a
cloud VM today, perhaps a container, bare metal or a machine at home later. That MUST NOT ripple
into the projects that use agentspace (D1). In Terry's words of 2026-11-16, it is "a whole mini
IaaS platform and API that you own" (IaaS: infrastructure as a service), where "you" is the agent
that runs agentspace.

**It scales horizontally (D25).** From v0.07 each project gets a host of its own, kept between its
leases, and from v0.09 sized for the work the project plans to run there. By 1.0 a project
describes its plan, and agentspace chooses the size from it (D27).

**It serves one person.** Every project agentspace serves is Terry's (D3). Leases for anyone else
are an idea for some day, not a plan (roadmap.md, "Someday, maybe").

## Words used here

| Word | Means |
|---|---|
| Project | A codebase and its work. Every lease is for exactly one project. |
| Lease | Compute for one agent for a bounded time: a way to log in, and an expiry (D2). |
| Caller | Whatever runs agentspace's lease commands to ask for, renew or release a lease. Today that is a project's architect, on the laptop (D8). |
| Host | The machine or container a lease logs in to. A caller never learns which one (D1). |
| Backend | Where hosts come from: a cloud's VMs, containers, bare metal (D6). |
| Driver | The part of agentspace that places leases on one backend, and supplies the way to reach them (D6, D9). |
| Control plane | The part of agentspace that takes a caller's requests and issues, renews and releases leases. Today it is a command-line tool on the laptop (D8). |
| Platform | An operating system and architecture, such as `linux/arm64` or `windows/amd64` (D4). |
| Profile | A named promise about a host's software: its libc (the C library its programs are built against), its toolchain and its agent tools (D4). |
| Size | How much compute a lease's host has. |
| Footprint | The CPU and memory one working agent uses on a host, measured rather than estimated (D27, D33). |
| TTL | How long a lease's credential is valid from the moment it is issued: 8 hours unless the caller asks otherwise, at least 1 minute and at most 24 hours (D24). |
| CA | agentspace's certificate authority: the key that signs every lease's certificate (D7, D17). |
| Certificate | A short-lived SSH certificate, signed by the CA, that lets a lease's own key log in to its host until it expires (D7). |
| Board | Terry's card board, localswim, which runs on his laptop. Agents take and report their work there (D12). |
| Card | An item on the board, cited as `#nnnn`. A story's card is also named for its release and turn, such as `E0.04-T03-S01` (D29). |
| Epic | A large piece of work on the board, broken into stories only when Terry says so (D14). |
| Story | One piece of an epic, worked by a few agents in one turn (D14, D16). |
| Plan | A project's stories laid out over turns, how many agents work at once in each, and on which model and effort. agentspace records the models and efforts but never sizes from them (D25, D27). |
| Turn | One step of a plan: stories worked at the same time by one dev lead and the helper agents it launches (D16). |
| Dev lead | The agent session that leads one turn (D16). |
| Release | A numbered version of agentspace, from v0.01 to v1.0, each one epic, taken in order (D26). To release a lease, a different sense, is to give it back. |
| 1.0 | The release at the end of the road below, whose test is a real project's use (roadmap.md, "The road to 1.0"). |
| MVP | v0.01, the first release: a lean prototype built to find the requirements (D19). |
| Prototype | The MVP's code, which a rewrite in Rust replaces (D28). |
| Spec | A reference doc that defines an interface by what can be seen from outside, so the rewrite needs no code (D20, D34). |
| Keeper | A background task on the laptop that holds the board's connection to a host while a lease is ready (D30). |
| The laptop | Terry's Windows laptop, where agentspace's control plane runs today (D8, D21). |

## Who uses it, and how

**Four kinds of user, each with its own promises; the agent that runs agentspace comes after
them.** The promises every caller gets are in "What every caller is promised" below, and are not
repeated under each role.

### Terry

**Terry owns agentspace and every project it serves, and pays for what it runs.** His projects'
agents ask for leases on his behalf; he is asked only for what only he can do (D23).

How he uses it:
- **He signs the agent tools in once per project host**, at its first use, rather than once per
  lease (D25, from v0.07; today once on the one box).
- **He says yes or no to persistent configuration on his laptop**, such as the keeper's logon task
  (D30), and to any call that would be costly to reverse (D19, D23).
- **He says when each release is broken into stories** (D14, D26), and 1.0 is done when a real
  project has worked through agentspace, not when the code works (roadmap.md, "The road to 1.0").

What agentspace promises him:
- **A host runs only while its project holds a live lease** (D25, built in v0.08; a lease's states
  are in lease-api.md, "States"). Until then, today's box stops itself after 60 minutes with no
  session and no CPU work (D11).
- **A size chosen from a plan comes from a measured footprint per agent, never from an estimate**
  (D27, D33; built in v0.10).
- **Every lease login names its lease**, so a host's log says which lease connected (D3, D7).
- **A lease's host keeps the laptop's time zone, which travels with Terry**, so the times agents
  write there, such as in a board comment, are in his zone (D32).
- **Today the CA's key stays on his laptop, owner-only, so only his account can read it** (D17,
  D21).

### A project's architect

**A project's architect is the agent that plans a project's work, and the caller that asks
agentspace for leases.** It lays the project's stories out over turns, and each turn needs compute
for the agents that will run it.

How it uses it:
- **It asks for a lease for its project**, naming the platform, profile, size and TTL it needs, or
  taking each one's default (D4, D5, D13, D24). Once plans are supported (v0.10) it MAY hand over
  the plan instead and let agentspace choose the size (D27).
- **It renews the lease while the work runs, and releases it when the work is done**
  (lease-api.md, "Commands").
- **It reads everything it needs from the lease:** the name to give `ssh`, the user to log in as,
  and the expiry (lease-api.md, "Fields").

What agentspace promises it, beyond what every caller gets:
- **A host of its project's own, kept between leases**, with the project's checkout, sign-ins and
  caches still on it, and no other project's lease on it (D25, built in v0.07). Today every
  project shares one box (D3).
- **Its checkout on that host** once hosts are per project: the host's disk keeps it between
  leases (D25, from v0.07). Today the project puts its own repo on the box.
- **A size that fits its plan**, chosen from the plan's busiest turn and never from the models or
  effort levels it names (D27, built in v0.10).

### A dev lead

**A dev lead is the agent session that leads one turn of a plan, working from a lease.** It works on
the host, and the helper agents (subagents) it launches for the turn's stories run inside its
session there (D33).

How it uses it:
- **It connects through the lease's host alias**, from a terminal or from the Claude desktop app's
  SSH environment (the app's way of running a session on a remote host), and opens its session in
  the project's checkout on the host (D9).
- **It takes and reports work on the board from the host** (D12, D30).

What agentspace promises it:
- **Room for its whole turn:** the host is sized for the busiest turn of the plan (D27, built in
  v0.10). Today every lease gets the one size there is (D5).
- **The board, with no connection for it or Terry to start by hand.** While its lease is ready, the
  keeper holds the board's connection to the host, for one lease on a host at a time (D12, D30).
- **Its tools as the profile states them**, including which libc its binaries can rely on, so it
  never has to probe the host to find out (D4).

### An agent on a lease

**An agent on a lease is any coding agent logged in to a host through a lease:** a dev lead's
session, or one agent working alone. The profile names the agent tools a host has, such as Claude
Code, Codex and gh (D4).

How it uses it:
- **It logs in with the lease's credential, as the user the lease names**, and reads that user from
  the lease rather than assuming it (lease-api.md, "Fields").
- **Its duty when the board cannot be reached is to stop and report**, never to work without a
  card. The host's board command refuses and tells it so (operator.md, "The box's
  files").

What agentspace promises it:
- **A credential of its own lease:** a key and a short-lived certificate, whose expiry is the
  lease's, and which name the lease in every login (D3, D7). How a lease logs in to a Windows host
  is open (V2).
- **A host whose software is what its profile says**, whatever the platform (D4).
- **A clock in the laptop's time zone** (D32), so the time it reads is the one Terry reads.

### Who runs agentspace

**agentspace is run by an agent, its own architect, which owns its design, its code and its hosts on
Terry's behalf** (roadmap.md, "The vision"; D16). agentspace's own work runs in releases, turns and
stories like any project's (D14, D16, D26, D29, D31). Today a host is configured only by
agentspace's architect, from agentspace's own repo, and a project's agent that finds a tool missing
reports it to that architect.

## What every caller is promised

- **One contract, whatever serves the lease.** Nothing in a request, a response, an error or a help
  text names a backend, a machine type, a region or a host (D1). The host alias names the lease,
  not the host behind it (lease-api.md, "Fields").
- **A stable contract.** A change that could break a caller is a new contract version, and a
  release number is not a contract version (D26; lease-api.md, "Versioning").
- **Exactly what it asked for, or a clear no.** A request that no backend can meet ends
  `unsatisfiable`, with a reason per unmet value, and is never placed on a near match (D4, D5). A
  TTL out of range is refused, never clamped (D24).
- **Strict output, tolerant input.** Every output validates against a published JSON Schema, and
  input is accepted in the spellings lease-api.md lists for each value (D20; lease-api.md, "Input
  tolerance").
- **A standard way in.** A lease is reached by its host alias with any SSH client that reads the
  OpenSSH client configuration, and the path to the host is the driver's concern, not the caller's
  (D9; lease-api.md, "Fields").
- **A bounded credential.** A lease's current certificate expires when the lease does. Each
  renewal issues a new certificate, so none is valid for longer than the maximum TTL of 24 hours,
  however often its lease is renewed (D7, D24).

## What it will never do

**These hold for as long as the decision cited holds.** Where one is not yet built, it says so.

- **Tell a caller what serves its lease** (D1).
- **Hand back anything other than what was asked for**, whether platform, profile, size or TTL
  (D5, D24).
- **Guess at an input that could mean two things** (D20).
- **Choose a host's size from a model or an effort level, or from an estimate** (D27), once sizes
  exist in v0.09.
- **Put two projects' leases on one host, once hosts are per project in v0.07** (D25). Until then
  today's one box is shared by every project (D3).
- **Assume a host is Linux.** A platform is a parameter of the contract, and `windows/amd64` is a
  platform like any other (D4). Today's lease model still assumes a Linux user and an SSH
  certificate, until v0.12 (D4).
- **Commit the CA's key** (D7, D17).
- **Stop a process on Terry's laptop that it did not start** (D30).

**And not before 1.0** (roadmap.md, "After 1.0"):
- **Revoke a released lease's certificate.** Release removes the lease's key and alias on the
  laptop, and its certificate stays valid until it expires (D10). Release does not end an open
  session; v0.08 stops a host whose project holds no live lease (roadmap.md, "The road to 1.0").
- **Serve a caller that is not on the laptop** (D8).
- **Give two or more leases on one host board access of their own** (D12).
- **Give each lease a fresh host of its own** (D25).
- **Sign a new project host in without Terry** (D25; decisions.md, Q2).

## Today, and the road to 1.0

**Today agentspace is one Linux box, described here as it is, not as the contract requires** (D4).
It is a Graviton (arm64) EC2 instance, a VM in Amazon's cloud, that every project shares, and every
lease logs in to it as the same user (D3). It stops itself when nobody uses it, and the next
connection starts it again (D9, D11). The control plane is a command-line tool on the laptop that
prints JSON (D8), and the CA is a file there (D17, D21). The board reaches the box through a
connection the keeper holds (D30). Only the default platform, profile and size are served, and any
other is `unsatisfiable` (D5).

**The road to 1.0, release by release, in roadmap.md's order** ("The road to 1.0", which also says
why):
- **v0.03: measure what an agent uses**, the footprint later sizes come from (D33).
- **v0.04 to v0.06: specs, then Rust.** v0.04 writes the specs, this page among them. v0.05 and
  v0.06 replace the prototype with a Rust rewrite built from the specs alone (D20, D28, D34).
- **v0.07: a host per project** (D25).
- **v0.08: a host runs only while its project holds a live lease** (D25).
- **v0.09: sizes**, set by resizing a stopped host (D25).
- **v0.10: sizes chosen from a plan handed over with the request** (D27).
- **v0.11: the plan read from the project's epic on the board**, instead of handed over.
- **v0.12: `linux/amd64` and `windows/amd64`**, the second and third platforms (D13), built in
  parallel once one shared turn makes the driver serve more than one; Windows with its host's own
  lease identity (D4).
- **v1.0: a real project, not agentspace itself, works a multi-turn epic on a host sized from its
  plan** (D26).

What comes after 1.0 is in roadmap.md, "After 1.0" and "Someday, maybe".

## How it is made

- **Plan to throw one away.** The MVP was a prototype built to find the requirements. Its behavior,
  as seen from outside, is what the rewrite is held to (D19).
- **A clean room.** The team that rebuilds agentspace never reads the prototype's code to learn its
  intent. Intent lives in this page, the decisions and the roadmap, and each interface has one
  spec that owns it (D20, D34).
- **Rust by v0.06** (D28).
- **Small releases, one epic each, taken in order** (D26).
- **Every design choice is a numbered decision**, and a changed decision says what replaced it
  rather than being deleted (decisions.md, "Statuses").

## Open questions

**What reading could not settle, and what became of each.** This page's writer (E0.04-T01-S01)
raised them, the clean-room review (E0.04-T04-S01) sorted them, and E0.04-T05-S05 (#0092) gave
each a disposition. **Answered** cites where decisions.md settles it, and decisions.md keeps the
fact (D34). **Later, not blocking** says why a drop-in rewrite of today's contract in v0.05 and
v0.06 does not need the answer, and which release or card takes it up, where the roadmap names
one. No question here is left unmarked.

| | Question | Why reading could not settle it | Disposition |
|---|---|---|---|
| **V1** | What does "one agent" mean in D2 once a lease carries a plan? | D2 makes a lease compute for one agent. D25 and D27 size a project's host for a plan's peak concurrency, and D33 counts a dev lead's subagents inside its one session. No decision says whether a turn takes one lease, for its dev lead, or one per agent session. | **Later, not blocking:** contract v0 has no plan, and a lease there is one `create` call for one project, so the rewrite needs no meaning for "one agent" beyond that. A plan enters the contract with v0.10 (#0050, contract v1), which is where this is decided. D27 and D33 already fix how the host is sized, whichever way it is answered. |
| **V2** | How does a lease on a Windows host log in? | D2 decides that a lease answers with a way to SSH in. D4 says the lease model today assumes a Linux user and an SSH certificate, and that v0.12 (#0051) builds a Windows host's lease identity. Whether a certificate login stays the way in, and what user a lease names there, is not decided. | **Later, not blocking:** no platform but `linux/arm64` is served until v0.12, so the rewrite never issues a Windows lease, and a request for one is `unsatisfiable` (D5). D4 gives the work to v0.12 (#0051). |
| **V3** | Do the MVP-only decisions whose replacements come after 1.0 still bind through 1.0? | D8 (the control plane on the laptop), D10 (no revocation), D17 (the CA on the laptop) and D24's maximum TTL are marked MVP only, but what replaces them, #0010 and #0011, is after 1.0 in roadmap.md. This page assumes they hold until then. | **Answered: yes, they bind.** An MVP-only decision still describes today's prototype, and so the rewrite, until its replacement is built (decisions.md, "Statuses"). D8 and D17 are replaced by #0011, D10 is replaced by #0010, and D24's maximum is revisited by #0010, and roadmap.md, "After 1.0", places both cards after 1.0. |
| **V4** | Does work on a lease depend on the laptop being awake? | The board runs on the laptop and reaches a host only through a connection it holds (D12, D30), and the control plane and CA are on the laptop (D8, D17). Today an agent stops when the board goes away: the host's board command refuses whenever no connection holds the board's forwards (operator.md, "The box's files"). No decision says whether that is meant to hold at 1.0. | **Later, not blocking:** the rewrite is held to today's observable behavior (D19, D20), and nothing in v0.05 to v0.06 moves the control plane or the board off the laptop. A hosted control plane (#0011, D8, D17, after 1.0) would move the control plane and the CA off the laptop, but the board itself runs on the laptop, and no card moves it: #0008 (D12) changes how many leases reach the board, not where it runs. No card takes the question up yet. |
| **V5** | Who starts a session on a lease? | A lease's caller creates it, and a dev lead connects through the alias (D9, D30). No decision says how a project's architect hands a lease to its dev lead, or whether adding the desktop app's SSH environment and starting the session there needs Terry's hand. | **Later, not blocking:** the hand-off is between a project's agents and the Claude app, outside the lease contract, which ends at the alias (lease-api.md, "Fields"). The rewrite changes none of it. No card takes it up yet. |
| **V6** | Does agentspace promise a time to ready? | No decision states how long a caller waits from asking for a lease to its first login, for a stopped host or for one created by that lease (D25). | **Later, not blocking:** contract v0 states no time, and a drop-in is held to the behavior it has, not to a promised duration. What a lease starts changes in v0.07 (#0047), where a first lease creates a host, and v0.08 (#0048), where a lease starts a stopped one (D25), so the question comes due there. No card takes it up yet. |
| **V7** | How does agentspace learn which repo to check out on a project's host? | D25 keeps a project's checkout on its own host from v0.07. Neither a decision nor the lease contract says where agentspace gets the repo's location from. | **Later, not blocking:** today a project puts its own repo on the box, and nothing in the lease contract names a repo, so the rewrite needs no answer. It is an open matter for v0.07 (#0047), where a project's first lease creates its host (D25); no decision yet says whether the location comes through the lease contract or some other way. |
| **V8** | Who may change a host's software? | No decision or spec doc says whether a project's agent may. | **Later, not blocking:** this page already states today's practice, in "Who runs agentspace", and the rewrite changes no host. A host's software is its profile's (D4), and replacing hosts instead of patching them is #0014, after 1.0 (roadmap.md, "After 1.0"). |

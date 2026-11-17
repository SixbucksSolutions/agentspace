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

**The CA key MUST NOT be committed**, since this repo is public. D17 says where it lives on the
laptop and how it is locked down; open question Q1 covers where it lives once the API is hosted.

**Findings from story E2.01 (2026-11-16).** D7 stands. A certificate from a throwaway CA, with
principal `ubuntu` and a 10-minute validity, logged in through `ssh-proxy.cmd` from a stopped box,
and the `authorized_keys` login kept working beside it. Expired certificates and certificates for
another principal were refused.
- **Ubuntu 26.04 includes `/etc/ssh/sshd_config.d/*.conf` before any other setting, and the first
  value wins.** The CA trust MUST be a drop-in installed from `box/system` by `sync`, and MUST NOT
  set a keyword that another drop-in already sets; today the only other one,
  `60-cloudimg-settings.conf`, sets `PasswordAuthentication no` alone. A name such as
  `50-agentspace-ca.conf` SHOULD be used. `sudo sshd -t` MUST pass before `systemctl reload
  ssh.service`, and the reload applies the change to new connections without a restart.
- **sshd logs each certificate login's key ID and CA:** `Accepted publickey for ubuntu ...
  ED25519-CERT SHA256:... ID <key id> (serial 0) CA ED25519 SHA256:...`. A refusal logs `Refusing
  certificate ID "<key id>" ... Certificate invalid: expired`, or `name is not a listed principal`.
- **Expiry is checked only at login.** A session that was open when its certificate expired kept
  running. See the finding under D10.
- **By default a certificate permits a PTY, port, agent and X11 forwarding, and user rc.** Issuing
  (story E2.05) SHOULD sign with `-O clear -O permit-pty -O permit-port-forwarding`. Port forwarding
  MUST stay permitted, because the board reaches the box through `RemoteForward` (D12).
- **An empty passphrase is passed differently by the two PowerShells.** Windows PowerShell 5.1
  drops `-N ''` and needs `-N '""'`; PowerShell 7.3 and later read `-N '""'` as a two-character
  passphrase. Code that makes a key MUST handle both, or check afterwards with `ssh-keygen -y`.
- **Windows OpenSSH accepted lease keys with the access rules ssh-keygen gives them** (the owner,
  SYSTEM and Administrators) under `%LOCALAPPDATA%`. No `icacls` step was needed for a lease key.
- **EC2 Instance Connect's `eic_run_authorized_keys` runs for every refused key and fails today**
  (`failed, status 22`), although `sshd -T` reports `authorizedkeyscommand none`. Where it is
  configured, and whether it is a second way to log in, is a question for story E2.04.

**Findings from story E2.05 (2026-11-16).** D7 stands. `New-LeaseCredential` in
`scripts/lease-credential.ps1` (PowerShell 7, D18) issues a lease's key and certificate, and
`scripts/test-lease-credential.ps1` checks it.
- **A lease's key pair lives in `%LOCALAPPDATA%\agentspace\leases\<lease id>\`** as `id_ed25519`,
  `id_ed25519.pub` and `id_ed25519-cert.pub`, the name ssh finds beside an `IdentityFile`. The
  issuer refuses a lease ID that is not a simple token, and refuses a lease that already has a key;
  renewing is story E2.07's.
- **The validity MUST be passed to `ssh-keygen -V` as an absolute UTC window,
  `YYYYMMDDHHMMSSZ:YYYYMMDDHHMMSSZ`, from the issue time to the issue time plus the TTL.** A relative
  `+8h` leaves the start to ssh-keygen, so the issuer could not return the exact expiry. Windows
  OpenSSH 9.5p2 reads the `Z` as UTC.
- **`ssh-keygen -L` prints the validity in the machine's local time, with no zone.** Anything that
  compares it with UTC values MUST convert them first.
- **The window starts at the laptop's clock, not before it.** If the laptop runs ahead of a host,
  a new certificate is refused there as not yet valid until the clocks meet. The start SHOULD be
  moved a few minutes back if that is ever seen.
- **PowerShell 7.6 passed `-N ''` through as an empty passphrase**, as the E2.01 finding expects for
  7.3 and later. The issuer checks every new key with `ssh-keygen -y -P ''`.

**Findings from story E2.04 (2026-11-16).** D7 stands. After one `sync`, a certificate signed by
the agentspace CA for `ubuntu` logged in (`ED25519-CERT ... ID <key id> ... CA ED25519
SHA256:G05h...`), a certificate naming `root` was refused (`ROOT LOGIN REFUSED`), and Terry's
`authorized_keys` login kept working.
- **`sync` checks the CA with `Assert-Ca` before it starts the box**, then sends the public key
  beside `box/system`. `install.sh` MUST replace `/etc/ssh/agentspace_ca.pub`, never append to it,
  so a new CA drops the old one.
- **`50-agentspace-ca.conf` sets `TrustedUserCAKeys` and `PermitRootLogin no`, and nothing else.**
  Root login MUST stay closed while the CA is trusted: under Ubuntu's default `prohibit-password`,
  a certificate naming `root` would otherwise log in as root.
- **`install.sh` MUST run `sshd -t` before `systemctl reload ssh.service`, and on failure MUST put
  back the files that were there and exit non-zero without reloading.** A reload with a bad config
  already fails, because the unit's `ExecReload` runs `sshd -t`; a restart or reboot has no such
  guard, which is why the old files are put back. `install.sh` then checks with `sshd -T` that
  both values took effect, and MUST NOT remove or overwrite any other drop-in.
- **The CA is trusted for every account, not only `ubuntu`.** A certificate naming `daemon` logged
  in as `daemon` and could forward TCP, although that account has no shell. This grants nothing a
  CA key holder lacks already, since the same holder can sign for `ubuntu`, and only `root` and
  `ubuntu` have real shells. It MUST be narrowed (`AllowUsers` or `AuthorizedPrincipalsFile`) when
  per-project users arrive (#0004).
- **EC2 Instance Connect is configured outside `sshd_config`.** The `ec2-instance-connect` package
  ships `/usr/lib/systemd/system/ssh.service.d/ec2-instance-connect.conf`, which starts sshd with
  `-o AuthorizedKeysCommand .../eic_run_authorized_keys %u %f -o AuthorizedKeysCommandUser
  ec2-instance-connect`. A hand-run `sshd -T` reads only the config files, so it reports
  `authorizedkeyscommand none`, and command-line options win over every drop-in. `failed, status
  22` is most likely curl's HTTP-error exit when no key has been pushed to the instance metadata,
  which sshd treats as no keys.
- **Instance Connect is a second way to log in, but not a new way into the box.** It needs IAM
  `ec2-instance-connect:SendSSHPublicKey` on the instance, which publishes a key for 60 seconds,
  and a path to port 22. The security group has no ingress rules and the region has no Instance
  Connect endpoints, so that path is Session Manager, whose rights already give a shell with sudo.
  `PermitRootLogin no` closes root through it too. Its logins carry no lease key ID. It SHOULD be
  removed after the MVP, by purging the package from `box/system` or by an IAM deny.
- **Refused logins can shut everyone out of SSH for a while.** Every connection through Session
  Manager arrives from 127.0.0.1, and OpenSSH 10.2's `PerSourcePenalties` (defaults, no exempt
  list) penalised that address for 16 seconds after three refused logins; repeated refusals can
  reach its 600-second maximum, Terry's own login included. Session Manager's shell is unaffected,
  so it is recoverable. `PerSourcePenaltyExemptList 127.0.0.1` SHOULD be considered before agents
  retry expired certificates unattended.

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

**Findings from story E2.01 (2026-11-16).** D9 stands. `ssh-proxy.cmd`, unchanged, served an alias
of another name under `ssh -F`: it ignores the alias name and reads `.instance-id` from its own
checkout. From stopped to a certificate login took 29 seconds.

The alias that worked, which story E2.06 builds on (paths shortened):

```
Host <lease alias>
  User ubuntu
  IdentityFile <lease key>
  CertificateFile <lease key>-cert.pub
  IdentitiesOnly yes
  HostKeyAlias agentspace
  StrictHostKeyChecking yes
  ProxyCommand C:\Windows\System32\cmd.exe /d /c "C:\Projects\agentspace\scripts\ssh-proxy.cmd"
  ServerAliveInterval 30
  ServerAliveCountMax 4
```

The spike's alias also set `ClearAllForwardings yes`, for its scripted runs only. A lease alias
MUST NOT set it, because the board reaches the box through `RemoteForward` (D12).

- **A lease alias MUST set `IdentityFile` to the lease key and `IdentitiesOnly yes`, and SHOULD set
  `CertificateFile`.** With `CertificateFile` alone, ssh had no private key for the certificate,
  fell back to the default `~/.ssh/id_*` keys and logged in through `authorized_keys`, which looks
  like a pass but proves nothing. With `IdentityFile` alone, ssh finds `<key>-cert.pub` by name but
  offers the bare key first.
- **A lease alias MUST set `HostKeyAlias` to the host's existing `known_hosts` name (today
  `agentspace`) and `StrictHostKeyChecking yes`.** The host key is then checked without an entry
  per alias and without `accept-new`.
- **`up` runs on every connection, and its localswim token push opens its own `ssh agentspace`.**
  That nested connection uses Terry's `~/.ssh/config` entry and his `authorized_keys` key, whatever
  `-F` the caller used, so a lease connection depends on that entry today. The push SHOULD be taken
  out of the driver's reachability path (card #0028).
- **`agentspace.ps1` calls `ssh` and `scp` by bare name.** Started from Git Bash, PATH finds MSYS
  ssh, which mangles the ProxyCommand (`exec: C:WindowsSystem32cmd.exe: not found`). The script
  MUST call `C:\Windows\System32\OpenSSH\ssh.exe` and `scp.exe` by full path; this is not yet done
  (card #0027).
- **The nested token push can hang.** When an agent's non-interactive PowerShell started the
  connection, with the lease alias or with `agentspace`, the remote side never saw the end of its
  input. The login stalled until the push was killed, its remote processes stayed open (and count
  as activity for the idle check), and `~/.config/localswim/bot.env` was left empty. The push MUST
  NOT wait for the end of its input, and SHOULD write a temporary file and rename it into place;
  this is not yet done (card #0026).

## What the MVP leaves alone

### D10. Releasing a lease does not revoke its certificate

**MVP only.** Release deletes the lease's key on the laptop, and the certificate stays valid until
it expires. TTLs are short, so the exposure is bounded. **Replaced by card #0010**, which pushes a
revocation list (KRL) to hosts.

**Finding from story E2.01 (2026-11-16):** sshd checks a certificate's validity only at login, so
neither release nor expiry ends a session that is already open. They stop new connections only.

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
stories until Terry says it is time for that epic. Someday ideas stay uncarded in
[roadmap.md](roadmap.md) until a real need shows up.

**Stories, Terry, 2026-11-16:**
- **A story's title is `EN.nn: title`**, where `N` is its epic's card number and `nn` is a
  zero-padded, 1-based number unique within the epic, for example `E2.01`. The `E` is there so a
  story reads as indexed by its epic (Terry, 2026-11-16), and docs, PRs and conversation MUST cite
  stories the same way. Branches use the lowercase form, `mvp/e2.06`; E2.02, E2.04 and E2.05 were
  already under way on `mvp/2.0x` branches when the rule came in, and keep them.
- **A story MUST be under 3 hours of estimated active work to reach Ready For Review.** One that
  cannot be broken down that far MUST go to Terry for discussion, not onto the board as is.
- **Stories go in Ready For Work, as children of their epic**, with their estimate and
  dependencies in the description.
- **An epic moves to In Progress when its stories are laid into Ready For Work, and stays there
  until its last story reaches Ready For Review.** Its description carries the execution plan by
  turn: each story starts in the first turn its entry gates are met, and the stories in one turn
  MUST be workable in parallel, in isolation from each other.

**Epic 2 went first, on purpose, to take the risk out of the MVP.** Every unknown that could
change the lease contract was in #0002 (certificates through the start-on-connect path, the
clients' handling of a certificate alias, sshd's trust rules), while #0001 is a CLI that writes
JSON. #0002 was broken down first, into E2.01 to E2.08 (#0018 to #0025), with its two risk-first
spikes at the front, and #0001 waits until #0002 has settled what the contract has to carry.

### D15. Staffing: one implementer and one reviewer per story, models by blast radius

**Decided, Terry, 2026-11-16.** Models are named `Model/effort`, with no version, meaning the
latest.

- **Each story gets one implementer subagent, then one independent reviewer subagent.** A story
  of under 3 hours MUST NOT be split across implementers: the merge cost outweighs the time saved.
- **The model follows what a mistake would break, not how hard the story looks:**
  - **Fable/high** reviews only, and only where a miss is an auth bypass or a lockout.
  - **Opus/high** implements anything ambiguous or risky, and reviews anything touching keys,
    sshd or Terry's `~/.ssh/config`.
  - **Sonnet/medium** does well-specified, mechanical work.
- **#0002's staffing:**

| Turn | Story | Implementer | Reviewer |
|---|---|---|---|
| T1 | E2.01 Spike: cert login | Opus/high | Sonnet/medium |
| T1 | E2.03 CA key | Opus/high | Fable/high |
| T2 | E2.02 Spike: clients | Sonnet/medium | Sonnet/medium |
| T2 | E2.04 Box trusts the CA | Opus/high | Fable/high |
| T2 | E2.05 Issue key and certificate | Sonnet/medium | Opus/high |
| T3 | E2.06 Host aliases | Sonnet/medium | Opus/high |
| T4 | E2.07 Renew and release | Sonnet/medium | Sonnet/medium |
| T5 | E2.08 Prove on the box | Opus/medium | Opus/high |

### D16. Roles: architect, dev lead per turn, subagents per story

**Decided, Terry, 2026-11-16.**

- **The architect** is the agent session that owns this repo's design: the roadmap, these
  decisions and the board's epics. It independently reviews each story's pull request, then
  squash merges it into `main` and deletes its branch once it is approved.
- **A dev lead** is a separate Opus/high session per turn, named for it (for example
  "agentspace MVP T1"). It spins up the subagents that D15 names for every story in the turn and
  reviews each story's work itself. It then opens **one pull request per story** (Terry,
  2026-11-16), from a branch of its own named for the story (`mvp/e2.06`). A dev lead MUST NOT
  merge its own pull request.
- **Subagents** implement or review one story. They MUST NOT move cards or open pull requests.
- **Cards:**
  - The dev lead moves its turn's stories to In Progress when it starts them.
  - The architect moves each story to Ready For Review when its pull request is merged.
  - Only Terry moves a card to Completed.
- **GitHub does not let one account approve its own pull request**, and every session pushes as
  Terry. The architect's approval is therefore a review comment that says so, followed by the
  squash merge.
- **Turns run in series** (Terry, 2026-11-16). A turn's dev lead is archived once every one of its
  stories' pull requests is merged, and only then does the next turn's dev lead start. T2 to T5 of
  #0002 run this way, one after another.

### D18. The lease CLI is PowerShell 7

**Decided, architect, 2026-11-16; settles Q5's language half.** The lease code (`agentspace lease`,
and the issuing, renewing and releasing that #0002 builds for it) MUST be written for PowerShell 7
(`#Requires -Version 7`), in files of its own under `scripts/`. `agentspace.ps1` stays a Windows
PowerShell 5.1 script, because `ssh-proxy.cmd` runs it as a ProxyCommand.

**Why:**
- The MVP's callers are all on the laptop (D8), and the laptop's tooling here is PowerShell.
- Go would bring a build step, and its cross-platform advantage matters only once leases are
  requested from somewhere other than the laptop, which is #0011's hosted API, not the MVP.
- PowerShell 7 drops the 5.1 quirks that cost stories E2.01 and E2.03 effort: the empty-argument
  problem and stderr turning into terminating errors.

**How the two scripts meet:** lease code that needs the CA checked MUST run `agentspace.ps1 ca`
and stop if it fails. It MUST NOT copy `Assert-Ca`. `ca` checks an existing CA and changes nothing
(D17), but on a laptop with no CA it creates one, which no host trusts until `sync` runs. That is
acceptable for the MVP, where the CA already exists.

**Terry left the language to the architect (2026-11-16):** pick what is easiest and fits the
problem. He reaches for Python when prototyping because it is friendly to junior developers. That
is worth weighing again for V1, but it does not move the MVP: the CA code (E2.03) and the issuing
code (E2.05) are already PowerShell, and switching mid-epic costs more than it saves.

**Q5's other half, the default and maximum TTL, stays with #0001.** Until then, issuing takes the
TTL as a parameter.

### D19. The MVP's bar is the happy path

**Decided, Terry, 2026-11-16:** "as it's a lean MVP, nothing gets us pot committed, we are largely
prototyping happy paths."

- **A story SHOULD prove its happy path end to end and stop there.** Edge-case handling,
  hardening and polish beyond the card's done-when SHOULD be carded as post-MVP work, not built.
- **Reviews SHOULD block only on:** a broken happy path, a security hole (an auth bypass, a key or
  token leak, a lockout), or anything that would be expensive to undo. Everything else is a note
  in the PR or a card.
- **The architect makes the MVP's design calls itself** and records them here, rather than
  stopping for Terry, unless a call would commit agentspace to something costly to reverse.
- **The MVP's code is expected to be thrown away.** Terry expects a clean-room restart for V1 or
  V2, so what the MVP MUST leave behind is what it learned (these decisions, the findings, the
  contract), not code worth keeping.
- **This is Terry's way of working, "plan to throw one away"** (Brooks, *The Mythical Man-Month*),
  in his words of 2026-11-16:
  - **Early drops find the requirements.** The MVP, and sometimes V1, are built as the work goes:
    move fast, see what sticks, and accept pivots, scope creep and the tech debt that comes with
    them.
  - **Once it stabilizes, the prototype becomes the requirements oracle.** The next drop is told,
    in effect, "that thing over there is 100% of what I want; build something that works exactly
    like it but is not a prototype".
  - **The next drop starts from zero, with no code reuse,** and MUST be a maintainable drop-in
    replacement for the prototype.
  - So the prototype's behavior SHOULD be easy to observe from outside (its contract, its JSON,
    its logs), because that is what the next drop will be held to.

### D20. The clean-room team gets specs and vision, never the prototype's code

**Decided, Terry, 2026-11-16.**

- **The team that rebuilds agentspace MUST NOT read the prototype's code to learn its intent.**
  The old code is a black box, so it cannot shape or taint the new design. The team MAY run the
  prototype and observe it, since that is what it is told to match (D19).
- **Intent and vision MUST live in documents**: [roadmap.md](roadmap.md), these decisions, and
  vision docs, written so they need no code to be understood.
- **Every interface MUST have a spec pedantic enough to leave nothing to guess:**
  - An HTTP API gets an OpenAPI spec.
  - A CLI gets an awscli-style reference: every command layer, every flag, every output field and
    every exit code, with help at each level that says the same thing as the reference.
- **Every JSON input and output MUST have a JSON Schema** (draft 2020-12), kept in the repo under
  `schemas/`, one file per document. The schema MUST be strict: `required` for every field that is
  always present, `additionalProperties: false`, and formats or patterns for IDs and times.
  Terry: strong formal interface specs matter, which JSON had long lacked next to XML's XSD.
- **Be very strict in what agentspace emits, and tolerant in what it accepts** (Terry,
  2026-11-16, the robustness principle). Inputs and outputs therefore get separate schemas:
  - **An output schema is as tight as the data allows.** One canonical form for every value: for
    example, a UUID is always a lowercase UUIDv4.
  - **An input schema accepts every reasonable spelling of the same value, and agentspace
    normalizes it on the way in.** A UUID MAY arrive in any case and is lowercased before use.
  - **The same holds for the formats themselves:**
    - **JSON** is accepted with any amount of whitespace, and emitted with none beyond what JSON
      requires. In PowerShell that is `ConvertTo-Json -Compress`, with `-Depth` set explicitly:
      its default depth of 2 silently flattens anything nested deeper.
    - **CSV**, if agentspace ever reads or writes it: any valid CSV is accepted, quoted or not,
      and it is emitted under one strict, written rule for quoting and line endings.
  - **Tolerance covers form, never meaning.** An input that could mean two different things, or
    that would be unsafe as given (a lease ID with a path separator, say), MUST still be refused
    with a clear error rather than guessed at.
  - **Lease IDs are UUIDv4s**, emitted lowercase and accepted in any case. Epic #0001 adopts this.
    E2.05's issuer takes whatever ID it is given, so the CLI MUST pass it the normalized form.
- **A schema is checked against the real output, not just written.** A CLI's scripted checks MUST
  validate what it prints against its schema. On the laptop that is PowerShell 7's `Test-Json
  -SchemaFile`, which handles draft 2020-12 (checked on 7.6, 2026-11-16).
- **The handoff is built before the rewrite starts**, as its own work (card #0030). In the MVP,
  `agentspace lease` gets its JSON schemas with epic #0001, since they cost little while the
  contract is being written. It SHOULD also have `--help` at every level.

## Keys

### D17. The CA key lives in %LOCALAPPDATA%, owner-only and without a passphrase

**MVP only, 2026-11-16 (story E2.03).** **Replaced by card #0011**, which settles Q1 when the API
is hosted and signing leaves the laptop.

- **The CA's private key MUST live at `%LOCALAPPDATA%\agentspace\ca\agentspace_ca`**, with its
  public key beside it as `agentspace_ca.pub`. Both files MUST stay out of the repo and out of
  any folder that OneDrive or another service syncs. `agentspace.ps1 ca` creates them
  once, and refuses to run when that folder's path is inside OneDrive or the repo. It MUST NOT be
  run elevated, since an elevated shell makes Administrators the owner of what it creates.
- **The folder and the private key MUST be the current user's alone:** owned by the user, one rule
  granting the user full control, and nothing inherited. `ca` locks and checks the folder before
  it makes the key in it, locks the key once it exists, and fails without repairing anything when
  it later finds either one otherwise; its message gives the `icacls` commands that lock them again.
- **The private key MUST NOT have a passphrase, and SHOULD NOT be loaded into ssh-agent.** The
  lease CLI signs unattended.
- **The public key MUST NOT be committed.** `sync` sends it to hosts from the laptop as their
  `TrustedUserCAKeys`, and SHOULD replace that file rather than add to it, so a new CA drops the
  old one. `sync` SHOULD check the pair with `Assert-Ca` first, so a swapped public key never
  becomes a host's trusted CA.
- **`ca` MUST NOT overwrite or repair a CA.** A half-made, mismatched or loosened pair fails with
  what is wrong, and a person decides what to do.
- **If the private key is lost or exposed, the CA MUST be replaced:** delete both files, run `ca`,
  run `sync` for every host, and re-issue every live lease's certificate. Certificates from the old
  CA stop working on each host once it is synced.

**Why:**
- `%LOCALAPPDATA%` never roams and cannot be redirected to OneDrive, and agentspace already keeps
  laptop state there. `~/.ssh` holds identities and gets copied around.
- **Disabling inheritance is load-bearing:** on Terry's laptop, `%LOCALAPPDATA%` passes inherited
  access to `CodexSandboxUsers` and to an AppContainer capability.
- ssh-keygen's own rules on a new key also grant SYSTEM and Administrators. They are dropped
  because those principals' privileges reach the file anyway, and owner-only is a rule `ca` can
  check exactly.
- An unencrypted key at rest exposes no more than `~/.ssh/id_ed25519` does today, which already
  reaches `ubuntu` on the box. The box also has no inbound ports, so a stolen CA key reaches it
  only together with AWS credentials for Session Manager.
- ssh-agent is not used, although `ssh-keygen -s -U` can sign with a CA key held there: the
  laptop's `ssh-agent` service is disabled, turning it on is a system setting, and once on it gives
  every process running as Terry the same signing power as the file does.
- A committed public key would make every fork's box trust Terry's CA, and every new CA would need
  a commit.

## Open questions

| | Question | Settled by |
|---|---|---|
| **Q1** | Where do the CA key and lease state live once the API is hosted? A KMS-backed signer? DynamoDB? | #0011 |
| **Q2** | Where do per-project credentials (Claude Code, Codex, gh) live, so a fresh host or container needs no sign-in? Needs Terry. | #0009 |
| **Q3** | How do many leases, and later many project users, reach the laptop's localswim board? | #0008 |
| **Q4** | One overlay network across all backends (WireGuard, Tailscale, Cloudflare Tunnel), or a reachability path per driver? | #0016 |
| **Q5** | ~~Is the lease CLI PowerShell or Go?~~ PowerShell 7 (D18). Still open: its default and maximum TTL. | #0001 |

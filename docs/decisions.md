# agentspace decisions

RFC 2119 keywords; the capitals are load-bearing.

**Every design decision about the lease API, numbered so cards and docs can cite it.** A decision
is never deleted: when one changes, its status says what replaced it and why. The roadmap that
these decisions serve is in [roadmap.md](roadmap.md).

**Statuses:**
- **Decided** holds for the MVP and after it.
- **MVP only** holds for the MVP and names what replaces it later.
- **Replaced** is a decision that a later decision has replaced. It names the decision that
  replaced it, and it no longer binds; its text stays as history.
- **Direction** is where things are headed, binding on design now, and built later.
- **A Direction becomes Decided once it is built**, and its status says where (D30).
- **Open** questions are listed at the end, each with the card that settles it.

**MVP only and Replaced differ in what has taken its place.**
- **A decision is MVP only while its replacement is still to be built:** a card or a release,
  such as #0011 or v0.08. It still describes today's prototype until that lands.
- **A decision is Replaced once a later decision has taken its place.** D18 began as MVP only,
  and became Replaced when D28 replaced it, so its opening still reads "MVP only. Replaced by
  D28" and its status is Replaced. A Replaced decision MAY still describe the prototype until its
  replacement is built (D18).
- **A decision replaced only in part keeps the status of the part that stands**, and its status
  says in a few words what the later decision replaced, naming it (D14, for one).
- **A decision that is MVP only in part is Decided for the rest**, and its status says which part
  and what replaces or revisits it (D24).

**Every decision MUST have exactly one status**, in the first bold text of its header block,
directly under its heading. Where its opening does not state one plainly, a line
`**Status: ...**` at the top of the block does, and that line governs. The index below MUST match
the headers exactly: the status word for a decision with no such line, and for one with it, the
line's text after `Status:`, without its full stop.

## Index

**One line per decision, in numeric order:** number, title and status. D17 sits under
"Keys" at the end of the file, and is listed here in its numeric place.

- [D1. Clients never know the backend](#d1-clients-never-know-the-backend): Decided
- [D2. The unit is a lease](#d2-the-unit-is-a-lease): Decided
- [D3. SSH credential per agent, isolation per project](#d3-ssh-credential-per-agent-isolation-per-project): Decided; its isolation half was replaced by D25
- [D4. Platform as `os/arch`; software as a named profile; no distro field](#d4-platform-as-osarch-software-as-a-named-profile-no-distro-field): Direction
- [D5. An unmet request is unsatisfiable, never quietly downgraded](#d5-an-unmet-request-is-unsatisfiable-never-quietly-downgraded): Decided
- [D6. Drivers place leases, not hosts](#d6-drivers-place-leases-not-hosts): Direction; narrowed by D25 to one project per host
- [D7. SSH access through short-lived certificates from an agentspace CA](#d7-ssh-access-through-short-lived-certificates-from-an-agentspace-ca): Decided
- [D8. The MVP control plane is a CLI on the laptop](#d8-the-mvp-control-plane-is-a-cli-on-the-laptop): MVP only; replaced by card #0011
- [D9. Reachability belongs to the driver](#d9-reachability-belongs-to-the-driver): Decided
- [D10. Releasing a lease does not revoke its certificate](#d10-releasing-a-lease-does-not-revoke-its-certificate): MVP only; replaced by card #0010
- [D11. Today's idle stop stays](#d11-todays-idle-stop-stays): MVP only; replaced by v0.08 (#0048), as D56 and D57 design
- [D12. Board access is unchanged](#d12-board-access-is-unchanged): MVP only; replaced by card #0008, and its hand-run holder by D30
- [D13. arm64 is the default platform](#d13-arm64-is-the-default-platform): Decided
- [D14. Roadmap horizons, epics before stories](#d14-roadmap-horizons-epics-before-stories): Decided; its horizons were replaced by D26, and its story names for releases by D29
- [D15. Staffing: one implementer and one reviewer per story, models by blast radius](#d15-staffing-one-implementer-and-one-reviewer-per-story-models-by-blast-radius): Decided; for release stories its one implementer and one reviewer were replaced by D31
- [D16. Roles: architect, dev lead per turn, subagents per story](#d16-roles-architect-dev-lead-per-turn-subagents-per-story): Decided
- [D17. The CA key lives on the laptop, owner-only and without a passphrase](#d17-the-ca-key-lives-on-the-laptop-owner-only-and-without-a-passphrase): MVP only; replaced by card #0011, and its location by D21
- [D18. The lease CLI is PowerShell 7](#d18-the-lease-cli-is-powershell-7): Replaced by D28
- [D19. The MVP's bar is the happy path](#d19-the-mvps-bar-is-the-happy-path): Decided; its happy-path bar for stories and reviews is MVP only, and D34 carries the hardening it deferred into v0.04's specs
- [D20. The clean-room team gets specs and vision, never the prototype's code](#d20-the-clean-room-team-gets-specs-and-vision-never-the-prototypes-code): Decided
- [D21. Laptop state lives in %USERPROFILE%\.agentspace, never in AppData](#d21-laptop-state-lives-in-userprofileagentspace-never-in-appdata): Decided
- [D22. The architect keeps a 10-minute watchdog on each dev lead](#d22-the-architect-keeps-a-10-minute-watchdog-on-each-dev-lead): Decided
- [D23. The architect is Terry's proxy](#d23-the-architect-is-terrys-proxy): Decided
- [D24. A lease's TTL is 8 hours by default, at least 1 minute and at most 24 hours](#d24-a-leases-ttl-is-8-hours-by-default-at-least-1-minute-and-at-most-24-hours): Decided; its maximum is MVP only, and card #0010 revisits it
- [D25. One host per project](#d25-one-host-per-project): Direction
- [D26. Releases from v0.01 to v1.0, one epic each](#d26-releases-from-v001-to-v10-one-epic-each): Decided
- [D27. A host is sized by concurrency, never by model or effort](#d27-a-host-is-sized-by-concurrency-never-by-model-or-effort): Direction
- [D28. agentspace is Rust by v0.06; PowerShell was the prototype's](#d28-agentspace-is-rust-by-v006-powershell-was-the-prototypes): Decided
- [D29. A release's stories are `EX.YY-Tnn-Smm`, named for their turn](#d29-a-releases-stories-are-exyy-tnn-smm-named-for-their-turn): Decided
- [D30. A keeper on the laptop holds the board's forwards](#d30-a-keeper-on-the-laptop-holds-the-boards-forwards): Decided; built in v0.02
- [D31. Each story's card names every subagent, run in parallel where reasonable](#d31-each-storys-card-names-every-subagent-run-in-parallel-where-reasonable): Decided
- [D32. A lease's box runs in the laptop's time zone](#d32-a-leases-box-runs-in-the-laptops-time-zone): Decided
- [D33. v0.03 measures each agent session's process tree, split into the session and its tool calls](#d33-v003-measures-each-agent-sessions-process-tree-split-into-the-session-and-its-tool-calls): Decided
- [D34. v0.04's spec package: one reference per interface, and a conformance suite kept apart from the prototype](#d34-v004s-spec-package-one-reference-per-interface-and-a-conformance-suite-kept-apart-from-the-prototype): Decided
- [D35. The keeper's contract, settled in the clean-room review](#d35-the-keepers-contract-settled-in-the-clean-room-review): Decided
- [D36. The operator never reports a false success](#d36-the-operator-never-reports-a-false-success): Decided
- [D37. The rewrite is seven files in one install folder, and carries what the checkout held](#d37-the-rewrite-is-seven-files-in-one-install-folder-and-carries-what-the-checkout-held): Decided
- [D38. v0.05 builds the lease CLI as a Cargo workspace of one crate per area, and cuts over by install](#d38-v005-builds-the-lease-cli-as-a-cargo-workspace-of-one-crate-per-area-and-cuts-over-by-install): Decided
- [D39. Until v0.09, the box is sized by hand for each use: M class, a vCPU per agent](#d39-until-v009-the-box-is-sized-by-hand-for-each-use-m-class-a-vcpu-per-agent): MVP only; replaced by v0.09 (#0049), whose size classes size a host from a lease, as D65 to D69 design
- [D40. By v1.0, agentspace's own files are JSON where feasible](#d40-by-v10-agentspaces-own-files-are-json-where-feasible): Direction, Terry, 2026-11-19, to be met by v1.0 (#0053)
- [D41. v0.06 cuts over on the laptop with no lease ready, and Terry's yes for each change to his configuration](#d41-v006-cuts-over-on-the-laptop-with-no-lease-ready-and-terrys-yes-for-each-change-to-his-configuration): Decided; step 8 changed by D42
- [D42. v0.06 builds the operator and the keeper as crates around two pinned boundaries, in six turns](#d42-v006-builds-the-operator-and-the-keeper-as-crates-around-two-pinned-boundaries-in-six-turns): Decided
- [D43. Today's box becomes swimcoach's host, and agentspace is the second project](#d43-todays-box-becomes-swimcoachs-host-and-agentspace-is-the-second-project): Decided
- [D44. A host is a CloudFormation stack of its own, from the one template](#d44-a-host-is-a-cloudformation-stack-of-its-own-from-the-one-template): Decided
- [D45. The laptop keeps which host serves which project, in `hosts.json`](#d45-the-laptop-keeps-which-host-serves-which-project-in-hostsjson): Decided
- [D46. A new host gets its software from `box/system` and a bootstrap, never an image, in v0.07](#d46-a-new-host-gets-its-software-from-boxsystem-and-a-bootstrap-never-an-image-in-v007): Decided
- [D47. The operator reaches every host with its own certificate, and keeps each host's key under its own name](#d47-the-operator-reaches-every-host-with-its-own-certificate-and-keeps-each-hosts-key-under-its-own-name): Decided
- [D48. A lease for a project with no host answers `pending`, and a run that outlives the call makes the host](#d48-a-lease-for-a-project-with-no-host-answers-pending-and-a-run-that-outlives-the-call-makes-the-host): Decided
- [D49. The operator's commands name a host, and without one act on today's box](#d49-the-operators-commands-name-a-host-and-without-one-act-on-todays-box): Decided
- [D50. One holder per host, one push stamp per host, one CA for every host](#d50-one-holder-per-host-one-push-stamp-per-host-one-ca-for-every-host): Decided
- [D51. Until v0.08, every host a turn starts is stopped at its close](#d51-until-v008-every-host-a-turn-starts-is-stopped-at-its-close): Decided
- [D52. Sign-ins and the checkout on a new host are by hand, once, by a written procedure](#d52-sign-ins-and-the-checkout-on-a-new-host-are-by-hand-once-by-a-written-procedure): Decided
- [D53. v0.07's done-when is shown by live checks, with Terry's yes for their instance hours](#d53-v007s-done-when-is-shown-by-live-checks-with-terrys-yes-for-their-instance-hours): Decided
- [D54. A host is made only for a project Terry has named](#d54-a-host-is-made-only-for-a-project-terry-has-named): Decided
- [D55. v0.07 is twenty stories in six turns: specs, foundations, programs, conformance, live](#d55-v007-is-twenty-stories-in-six-turns-specs-foundations-programs-conformance-live): Decided
- [D56. A host runs only while its project holds a live lease; the laptop's guard stops it otherwise](#d56-a-host-runs-only-while-its-project-holds-a-live-lease-the-laptops-guard-stops-it-otherwise): Decided
- [D57. A host bounds itself by its lease horizon while the laptop is away](#d57-a-host-bounds-itself-by-its-lease-horizon-while-the-laptop-is-away): Decided
- [D58. The CloudWatch backstop stays, lengthened from 3 hours to 24](#d58-the-cloudwatch-backstop-stays-lengthened-from-3-hours-to-24): Decided
- [D59. A release ends the lease's own connections on the laptop](#d59-a-release-ends-the-leases-own-connections-on-the-laptop): Decided
- [D60. `status -All` is the one command that lists every host with its live leases](#d60-status--all-is-the-one-command-that-lists-every-host-with-its-live-leases): Decided
- [D61. v0.08 carries #0163, #0164, #0166 and #0167](#d61-v008-carries-0163-0164-0166-and-0167): Decided
- [D62. v0.08's done-when is shown by live checks on both hosts, with Terry's yes for their instance hours](#d62-v008s-done-when-is-shown-by-live-checks-on-both-hosts-with-terrys-yes-for-their-instance-hours): Decided
- [D63. v0.08 is twenty-one stories in six turns: specs, foundations, programs, conformance, live](#d63-v008-is-twenty-one-stories-in-six-turns-specs-foundations-programs-conformance-live): Decided
- [D64. v0.06-T01's whole-turn record meets #0049's re-measurement for the host; the per-agent split is owed to v0.10](#d64-v006-t01s-whole-turn-record-meets-0049s-re-measurement-for-the-host-the-per-agent-split-is-owed-to-v010): Decided
- [D65. Four sizes, `s` to `xl`, on the `m9g` ladder; `m` stays the default and today's host](#d65-four-sizes-s-to-xl-on-the-m9g-ladder-m-stays-the-default-and-todays-host): Decided
- [D66. A host is resized only while no ready lease holds it; a lease its host already meets is placed at once](#d66-a-host-is-resized-only-while-no-ready-lease-holds-it-a-lease-its-host-already-meets-is-placed-at-once): Decided
- [D67. The stack holds a host's instance type, the host registry records it, and the size-to-type map is the EC2 driver's](#d67-the-stack-holds-a-hosts-instance-type-the-host-registry-records-it-and-the-size-to-type-map-is-the-ec2-drivers): Decided
- [D68. A resize is the provisioning run, given the instance type; the guard and the idle check meet it as they meet a provisioning run](#d68-a-resize-is-the-provisioning-run-given-the-instance-type-the-guard-and-the-idle-check-meet-it-as-they-meet-a-provisioning-run): Decided
- [D69. D39 ends with v0.09: the architect picks a turn's size by Terry's per-agent rule, and asks for it through the lease](#d69-d39-ends-with-v009-the-architect-picks-a-turns-size-by-terrys-per-agent-rule-and-asks-for-it-through-the-lease): Decided
- [D70. v0.09's done-when is shown by live checks on agentspace's host, with Terry's yes for their instance hours](#d70-v009s-done-when-is-shown-by-live-checks-on-agentspaces-host-with-terrys-yes-for-their-instance-hours): Decided
- [D71. v0.09 is twelve stories in five turns: specs, foundations, programs, conformance, live](#d71-v009-is-twelve-stories-in-five-turns-specs-foundations-programs-conformance-live): Decided

## The interface

### D1. Clients never know the backend

**Decided, Terry, 2026-11-16.** A client MUST NOT be able to tell, and MUST NOT need to know,
whether its lease landed on bare metal, Azure, EC2, a Fargate container or anything else. Nothing
in a request or a response MAY name a backend, an instance type, a region or a host.

**Why:** hosting changes as Terry's needs and cost tolerance change, and that MUST NOT ripple into
every project that uses agentspace.

**D1 holds at connect time too** (architect, 2026-11-18, for E0.04-T02-S02, #0077). What the
ProxyCommand prints while a lease connects reaches the caller's ssh client, and so the caller.
- **On success it MUST NOT name the backend:** no instance or host ID, region, machine type, cloud
  state name, or cloud tool's text. A progress line MAY say that the lease's host is starting or
  ready, in agentspace's own words.
- **On failure it prints agentspace's own message**, which MAY name the lease and what the caller
  can do, never the backend.
- **The backend's own detail MAY be printed only when the caller asks for it**, through a setting
  docs/operator.md names, for diagnosis.
- **The prototype does not meet this:** `up` prints the instance ID (`Ready: <id>`,
  `Starting <id>...`), EC2 states, Session Manager and patch messages, and the AWS CLI's text.
  It is carried into docs/operator.md and docs/driver.md as a requirement, and met by v0.06
  (#0055). The card is #0087.

### D2. The unit is a lease

**Decided.** A project asks for a lease: compute for one agent, for a bounded time. A lease
answers with a way to SSH in and an expiry. Placement, meaning which host or container serves it,
is agentspace's business alone.

**Why:** a lease is the one thing every backend can provide, from a user on a shared VM to a
container of its own.

### D3. SSH credential per agent, isolation per project

**Status: Decided; its isolation half was replaced by D25.**

**Direction; MVP only for the isolation half.** Each lease gets its own SSH key and certificate,
so each agent's access can be told apart, expired and audited on its own. The isolation boundary
is the project: one Unix user per project, with its own home and secrets, shared by that
project's agents.

**In the MVP every project lands as `ubuntu`**, as agents do today. All projects are Terry's and
already share the box this way, so isolation is safe to punt. Card #0004 was to add it.

**The isolation half was replaced by D25 (2026-11-17):** each project gets a host of its own, so
the project boundary is the host, not a Unix user on a shared one. #0004 is OBE and archived. The
credential half (a key and certificate per lease) stands.

**Why per project and not per agent:** agents in one project share a repo, toolchain caches and
sign-ins. A user per agent would mean signing every tool in once per agent.

**Findings from story E3.03 (2026-11-17).** D3 stands. Nothing puts a project's repo on the box
for a lease.
- **swimcoach was not on the box.** The dev lead cloned it with `git clone
  https://github.com/SixbucksSolutions/swimcoach.git ~/swimcoach` (gh is git's credential helper
  for github.com, and the repo is public anyway): `origin/main` at 6eb57ad, 212 MB, which took the
  root disk from 67% to 70% of 6.8 GB.
- **A project's checkout MUST be on the box before its agents open a session in it.** Every lease
  is `ubuntu` in the MVP, so the checkout lives in the shared home, every lease sees it, and it
  outlives the lease. Under D25 the checkout lives on the project's own host, and v0.07 (#0047)
  puts it there.

### D4. Platform as `os/arch`; software as a named profile; no distro field

**Direction.** A request MAY name a `platform` in the OCI form (`linux/arm64`, `linux/amd64`,
`darwin/arm64`) and a `profile`, a named and versioned software contract such as `agent-linux@3`
(distro, libc, Node LTS, uv, Claude Code, Codex, gh). The distro MUST NOT be a field of its own;
it lives inside a profile.

**Why:** a bare distro name promises almost nothing a caller depends on. What callers depend on is
libc and the toolchain: Alpine was ruled out for this box because workerd and Playwright do not run
on musl, not because it was Alpine. A distro field would also invite arch × distro × version
combinations that nobody tests.

**A profile MUST state its libc, family and minimum version** (Terry, 2026-11-18: "I don't think
linux is specific enough, agent may care about libc available to it"). The platform says `linux`
and never which libc, so an agent's binaries (a Rust `-gnu` target, Node's `linux-x64`, a
`manylinux` wheel) learn it from the profile. A musl host, such as Alpine, would be a profile of
its own, unsatisfiable until a driver supports it (D5). E0.04-T01-S02 (#0074) states
`agent-linux`'s.
- **The family MAY be `none`** (Terry, 2026-11-18): a host or image with no libc at all, such as a
  `scratch` container that runs a CGO-disabled Go binary. A `none` profile has no libc version,
  and promises nothing that needs one. So the family is one of `glibc`, `musl` or `none`, and only
  the first two carry a minimum version.
- **A profile states what a host has; what an agent needs is a separate value, for later**
  (Terry, 2026-11-18, taking the architect's recommendation). A need would be `glibc`, `musl` or
  `any`, where `any` matches every profile, `none` included: the CGO-disabled Go binary runs
  whether a libc is there or not. `any` is never a profile's value, since a host cannot have
  "any" libc. Today a request names a profile, never a libc, so no request carries a need. It is
  deferred until after 1.0, in the roadmap's "Someday, maybe".
- **A need that no profile meets fails early, and that is fine** (Terry, 2026-11-18): "That isn't
  a problem, it's just staying focused on common cases for 1.0." The request is `unsatisfiable`
  (D5), never placed on a near match. 1.0 supports and tests the common cases, and the rest wait
  for a real need.

**Windows hosts are in scope (Terry, 2026-11-18):** "if we need a windows EC2 instance as part of
the agentspace fleet, that's in scope. Whole point is to be a multi-platform agent
compute-on-demand service."
- **A spec or design MUST NOT assume a host is Linux,** except where it describes today's EC2 box
  as it is. The platform stays a parameter of the contract, and `windows/amd64` (and later
  `windows/arm64`) are platforms like any other: unsatisfiable until a driver supports them (D5).
- **Windows hosts come before 1.0, in v0.12 (#0051)** (Terry, 2026-11-18, as v0.13, #0081; folded
  into v0.12 beside `linux/amd64` on 2026-11-19, D26). The work is a lease
  model that today assumes a Linux user and an SSH certificate, and a host agent of its own.

### D5. An unmet request is unsatisfiable, never quietly downgraded

**Decided.** A request no backend can meet MUST end in `unsatisfiable` with a reason. Placement
MUST NOT hand back a different platform, profile or size than was asked for.

**In the MVP only the defaults are supported:** `linux/arm64`, `agent-linux` and size `m`. The
fields are in the contract from day one, so adding values later changes no client.

### D6. Drivers place leases, not hosts

**Status: Direction; narrowed by D25 to one project per host.**

**Direction.** A backend is a driver with one contract: place this lease, return its SSH
endpoint, tear it down. Drivers come in two kinds:
- **Pooled:** EC2, Azure VMs, bare metal. Leases are packed onto shared hosts, where a host agent
  sets up the user, worktree and limits.
- **Per-lease:** Fargate, Azure Container Instances. One lease is one container.

**Why:** a contract written around hosts ("power on, run the host agent") does not fit
container backends, where there is no host to own.

**In the MVP there is one pooled driver: the existing EC2 box.** The CLI's code MUST keep it
behind a driver boundary anyway, so a second driver means adding code, not reworking it.

**Narrowed by D25 (2026-11-17):** a pooled driver's host serves one project, so leases of
different projects are never packed onto one host. #0012 (placement across a shared pool) is OBE
and archived.

**Findings from story E1.03 (2026-11-17).** D6 stands. The boundary is two files in `scripts/`:
- **`lease-driver.ps1` is the interface and names no backend** (D1; a check greps it for
  `ec2` and `aws`). Its header comment is the full contract, kept in the repo so the CLI's
  authors (E1.04, E1.05) can read it: `Register-LeaseDriver`, `Invoke-LeaseDriverPlace`,
  `Invoke-LeaseDriverRenew` and `Invoke-LeaseDriverRelease`, with their parameters and return
  members, the failure rule and the D1 rule on messages.
- **Registration:** dot-sourcing `lease-driver.ps1` dot-sources every `lease-driver-*.ps1` beside
  it, and each registers itself under an opaque name. `Place` asks the first registered driver
  (there is one) and adds that driver's name to the result; renew and release look the driver up
  by the name the CLI stored.
- **`lease-driver-ec2.ps1` is the pooled driver** and the only file that names EC2. It calls
  `New-LeaseCredential` and `Set-LeaseAlias` to place, `Update-LeaseCredential` to renew, and
  `Remove-LeaseCredential` to release. Its `ExpiresAt` is the credential's `ExpiresUtc`, so it
  equals the certificate's valid-before.
- **Unsatisfiable is decided before anything is issued:** the platform MUST be `linux/arm64`, the
  profile `agent-linux` with no version, and the size `m` (D5). Each unmet value adds one reason,
  in the order `unsupported_platform`, `unsupported_profile`, `unsupported_size`, whose one-sentence
  message quotes the value and names no backend. It is a returned state, not an exception.
- **A failed alias step removes the credential before the error is rethrown.** If
  `Remove-LeaseCredential` refuses (a foreign file sits at the alias), the lease folder is deleted
  directly, and the foreign file is kept. The rethrown message is generic, with the original error
  as its inner exception, because `Set-LeaseAlias`'s own messages talk about the machine. The CLI
  MUST therefore show only the outer message.
- **A credential step that fails partway has its lease folder deleted, but only if that call
  created it**, so a key with no certificate is never left behind, and an existing lease's key is
  never touched.
- **Checked** by `scripts/test-lease-driver.ps1`, which uses a throwaway lease ID under the real
  leases root (the credential step has no root override and needs the real CA) and a throwaway
  ssh folder, and hashes the real `~/.ssh` config and `agentspace.d` before and after.

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
  The same held under `%USERPROFILE%\.agentspace\leases` (E2.09).
- **EC2 Instance Connect's `eic_run_authorized_keys` runs for every refused key and fails today**
  (`failed, status 22`), although `sshd -T` reports `authorizedkeyscommand none`. Where it is
  configured, and whether it is a second way to log in, is a question for story E2.04.

**Findings from story E2.05 (2026-11-16).** D7 stands. `New-LeaseCredential` in
`scripts/lease-credential.ps1` (PowerShell 7, D18) issues a lease's key and certificate, and
`scripts/test-lease-credential.ps1` checks it.
- **A lease's key pair lives in `%USERPROFILE%\.agentspace\leases\<lease id>\`** (D21; until E2.09
  it was under `%LOCALAPPDATA%`, which the Claude app redirected into its package cache) as
  `id_ed25519`, `id_ed25519.pub` and `id_ed25519-cert.pub`, the name ssh finds beside an
  `IdentityFile`. The issuer refuses a lease ID that is not a simple token, and refuses a lease
  that already has a key; `Update-LeaseCredential` renews it (E2.07, below).
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

**Findings from story E2.07 (2026-11-17).** D7 stands. `Update-LeaseCredential` in
`scripts/lease-credential.ps1` renews a lease, and shares its CA check and signing step with
`New-LeaseCredential`, so both sign with the same principal, options and absolute UTC window.
- **Renewal MUST replace only the certificate.** It signs a copy of the lease's `.pub` named
  `renew-<guid>.pub` inside the lease folder, then moves the new `renew-<guid>-cert.pub` over
  `id_ed25519-cert.pub`, so the old certificate stays until the new one exists. The key is never
  created or replaced, and a lease with no key is refused.
- **Renewal is idempotent.** Each call signs again with a window from the call's time, and the
  key stays byte-identical.
- **Renewal lowercases the lease ID (D20) and issue does not.** The lease CLI (#0001) MUST pass
  issue the normalized ID, as D20 already says, so the two agree on the key ID. Release accepts
  only a UUIDv4, through `ConvertTo-LeaseId`, so a lease issued under another token cannot be
  released; the CLI's lease IDs are all UUIDv4s.

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
  `ubuntu` have real shells. It SHOULD be narrowed (`AllowUsers` or `AuthorizedPrincipalsFile`) to
  the users a lease logs in as. Per-project users (#0004) are OBE under D25, so card #0032 now
  carries this.
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

**Findings from story E3.01 (2026-11-17).** D7 stands. `box/system/51-agentspace-logins.conf` sets
`PerSourcePenaltyExemptList 127.0.0.1` and `LoginGraceTime 30`, and nothing else. It sorts after
`50-agentspace-ca.conf` and before `60-cloudimg-settings.conf`, and no other file sets either
keyword (the default `sshd_config` has `LoginGraceTime` commented out, and no `PerSource*` line). `50-agentspace-ca.conf` still sets
only `TrustedUserCAKeys` and `PermitRootLogin`.
- **`install.sh` installs the CA key and both drop-ins as one transaction.** It backs up what was
  there, installs the files, runs `sshd -t`, and on failure puts back every file this run changed
  and exits non-zero without reloading. After `systemctl reload ssh.service` it checks with
  `sshd -T` that `logingracetime 30` and `persourcepenaltyexemptlist 127.0.0.1` took effect,
  beside the E2.04 check, and fails loudly otherwise. `sync` installed it; `sshd -T` on the box
  then reported `logingracetime 30`, `persourcepenaltyexemptlist 127.0.0.1`, `maxstartups
  10:30:100` and the default `persourcepenalties`.
- **Why 30 seconds.** `LoginGraceTime` starts when sshd accepts the TCP connection, which is after
  the box is up, so boot time does not count. A whole `ssh agentspace true` through Session
  Manager took 4.0 to 4.3 seconds from the laptop, including starting the proxy and the session,
  and sshd took 0.23 seconds from accepting the connection to `Accepted publickey` (the E3.01
  reviewer's measurement on two logins). Terry's default key `~/.ssh/id_ed25519` has no
  passphrase (`ssh-keygen -y -P ""` exited 0), so no typing happens inside the grace time. 30
  seconds leaves several times the observed time. A shorter value SHOULD NOT be used without
  measuring a slow link; a client that needs longer, such as one that prompts for a passphrase
  or a second factor, would be dropped at 30 seconds.
- **`MaxStartups` stays at its default `10:30:100`.** With a 30-second grace time, filling the 10
  free pre-authentication slots takes about ten lingering refusals within 30 seconds rather than
  within 120, and the exemption removes the penalty route to a lockout. Changing it MUST wait for
  evidence that it is still reached.
- **Burst test.** A throwaway CA the box does not trust signed a throwaway key's certificate
  (principal `ubuntu`, one hour; deleted afterwards, no change to the box's `authorized_keys`).
  Six certificate logins through Session Manager and the real `ssh-proxy.cmd` started 4 to 5
  seconds apart from 19:35:54 UTC, each refused with `Permission denied (publickey)`. The good
  login `ssh agentspace true` started at 19:36:21 UTC, while all six refused connections were
  still open on the box, and exited 0; sshd logged `Accepted publickey for ubuntu from 127.0.0.1`
  at 19:36:24. A second good login at 19:36:33 and a third at 19:36:58, after the last refused
  connection had timed out, also exited 0.
- **The refused connections lingered for the new grace time, not 120 seconds.** sshd logged six
  `Timeout before authentication for connection from 127.0.0.1` lines, at 19:36:28, :34, :36, :40,
  :47 and :51, each 30 to 33 seconds after that connection's first journal line (the first
  refusal's `AuthorizedKeysCommand` line is at 19:35:57). The journal held no line mentioning a penalty, a
  dropped connection or `MaxStartups` for the whole window. This is the E2.08 mechanism (the
  Session Manager tunnel does not pass the close on to sshd), now with a 30-second hold.
- **The burst did not show the lockout it prevents.** D10's E2.08 finding predicts that two
  lingering refusals reach the 15-second minimum penalty. This test ran after the exemption was
  live, so it shows that six did not block a good login with the exemption in place, not that
  the exemption was the cause; the pre-change case was not re-run, to avoid locking Terry out.
  The journal does hold one before-case from the same day, under the defaults: at 06:12:16 UTC,
  `srclimit_penalise: 127.0.0.1/32: activating ipv4 penalty of 16 seconds for penalty: exceeded
  LoginGraceTime`. It is the only penalty line in the journal, and six grace timeouts within 23
  seconds after the change produced none.
- **Only 127.0.0.1 is exempt, not `::1`.** Every source address sshd has logged on the box is
  127.0.0.1: the Session Manager worker dials `localhost`, which `/etc/hosts` maps to 127.0.0.1
  only. An image that maps `localhost` to `::1` would bring the lockout back, so a new image
  SHOULD be checked for it.
- **A certificate from an untrusted CA logs no `Refusing certificate` line.** The only per-attempt
  journal lines are the two `AuthorizedKeysCommand ... eic_run_authorized_keys` lines from EC2
  Instance Connect (D7, E2.04 finding), so a refusal of that kind is visible in the journal only
  as that pair and later the timeout. The `Refusing certificate ID ...` line of the README's Logs
  section appears for a trusted CA's expired or invalid certificate.

### D8. The MVP control plane is a CLI on the laptop

**Status: MVP only; replaced by card #0011.**

**MVP only.** `agentspace lease ...` runs on the laptop, prints JSON, and keeps lease state in a
local file. There is no server to run.

**Why:** today's callers are all projects on the laptop, which already call `localswim` the same
way.

**Contract v0's caller runs on Windows** (architect, 2026-11-18, from the clean-room review,
#0080). The CLI, its state and its defaults are specified for the Windows laptop. A caller on
Linux or macOS is later and not blocking, and comes with the hosted control plane (#0011). Hosts
are a separate matter: they MAY be any platform (D4).
- **The default ProxyCommand is the full path of agentspace's ProxyCommand program, then the
  alias.** The CLI finds that program in the folder its own executable runs from, so a compiled
  CLI needs no checkout. A `.cmd` or `.bat` program is run through
  `%SystemRoot%\System32\cmd.exe /d /c`. `AGENTSPACE_PROXY_COMMAND` still overrides it whole. The
  prototype's `<checkout>\scripts\ssh-proxy.cmd` is where this came from, not the contract.
**Replaced by card #0011**, a hosted API, once a caller that is not on the laptop needs a lease. The
CLI then becomes a client of it, and its JSON stays the same.

**Findings from story E1.02 (2026-11-17).** D8 stands. E1.02 built the CLI's skeleton: the entry
point, `--help`, the errors and exit codes, the value normalizers and the state file, with each
command a stub. `scripts/test-lease-cli.ps1` checks all of it in a throwaway folder.
- **The entry point is `bin/agentspace.cmd` (cmd and PowerShell) and `bin/agentspace` (a POSIX sh
  shim for Git Bash).** Each runs `pwsh -NoProfile -File scripts\lease-cli.ps1` with the
  arguments unchanged. They MUST live outside `scripts/`: PowerShell prefers a `.ps1` to a `.cmd`
  of the same name in one folder, so `agentspace` there would run `scripts\agentspace.ps1`. The
  top level knows only `lease` and `--help`; the operator keeps running `agentspace.ps1` directly.
  - The sh shim turns off MSYS path conversion (`MSYS_NO_PATHCONV=1`), so an argument that looks
    like a path reaches the CLI as it was given, and converts the script's own path with `cygpath`.
- **`lease-cli.ps1` MUST read its arguments from `[Environment]::GetCommandLineArgs()`, not
  `$args`.** With `-File`, PowerShell binds a script's arguments as parameters first: it splits
  `--ttl=1:30` into `--ttl=1` and `30`, and drops `--ttl:` and `-h:` without a trace, so `create
  --project x --ttl:` would have created a lease. One case stays out of reach: an argument such as
  `-:x` makes PowerShell fail before the script starts, with exit 1 and a message that is not an
  error document.
- **How the command line is read, where the contract leaves a choice:**
  - A flag is `--name` or `--name=value`. A single dash is only for `-h`, so `-ttl` is unknown.
  - A token that starts with `--` is never taken as a flag's value, so `--project --size m` is a
    usage error for `project`. A token with one dash is a value (`--ttl -1h` is `invalid_value`).
  - An unknown flag takes no value. Usage errors are held until the whole line is read, so a later
    `-h` or `--help` still prints its level's help.
  - Missing `--project` on `create`, and a missing `<id>`, are usage errors with `field` set.
- **Output is written as UTF-8 bytes straight to the process's standard streams**, with LF line
  ends and no byte order mark. PowerShell's own output would end lines with CRLF on Windows.
  `--help` text is kept in `lease-cli.ps1`, and the test compares it byte for byte with the block
  after each "**Help:**" in `docs/lease-api.md`, read at test time, so the two cannot drift.
- **A stub answers `internal` (exit 1), `agentspace lease <command> is not implemented yet.`**
  The contract has no "not implemented" error and allows no other exit code.
- **E1.02 owns the value normalizers, in `scripts/lease-values.ps1`:** `ConvertTo-LeaseIdValue`,
  `ConvertTo-LeaseProjectValue`, `ConvertTo-LeasePlatformValue`, `ConvertTo-LeaseProfileValue`,
  `ConvertTo-LeaseSizeValue`, `ConvertTo-LeaseTtlValue` (whole seconds) and
  `ConvertTo-LeaseStateFilterValue`. `lease-cli.ps1` runs them on every value, in the order of
  each command's table, before any command runs, so the contract's error order holds.
  - **The command files MUST call these normalizers and MUST NOT write their own**, so that T7's
    parallel stories E1.04 and E1.05 never duplicate them. A missing helper MUST be added to
    `lease-values.ps1`.
  - Values are matched with `-cmatch` and `[0-9]`. `-match` ignores case, which lets the Kelvin
    sign through as a `k`, and `\d` matches digits of every script.
  - "Surrounding whitespace" includes a line feed, so `--project "x\n"` is `x`.
  - The TTL is computed in `BigInteger`, so `99999999999999999999h` is `out_of_range`, never
    `internal`.
- **A lease error is a thrown `System.Exception` whose `Data` carries `agentspace.lease.code`**,
  and `.field`, `.lease_id` and `.state` where they apply. `New-LeaseError` in `lease-values.ps1`
  makes one. The CLI prints it as its error document; any other exception becomes `internal`, with
  the exception's message, so that message MUST NOT name a backend, a location or a machine (D1).
  Control characters in any message are written as `\u` and four hex digits.
- **Each command is one file, `scripts/lease-cmd-<command>.ps1`, defining one function that T7
  fills in without touching `lease-cli.ps1`.** Each file's header documents its function in full.
  The CLI calls it with normalized values and defaults applied, prints what it returns as compact
  JSON, and turns what it throws into the error document:

  | Command | Function |
  |---|---|
  | `create` | `Invoke-LeaseCreate -Project <string> -Platform <string> -Profile <string> -Size <string> -TtlSeconds <int>` |
  | `get` | `Invoke-LeaseGet -LeaseId <string>` |
  | `renew` | `Invoke-LeaseRenew -LeaseId <string> -TtlSeconds <int>`, where 0 means `--ttl` was not given |
  | `release` | `Invoke-LeaseRelease -LeaseId <string>` |
  | `list` | `Invoke-LeaseList -Project <string> -State <string[]>`, where empty means no filter |

  - `create`, `get`, `renew` and `release` return the lease document from
    `ConvertTo-LeaseOutput`. `list` returns `[ordered]@{ leases = @(...) }`.
  - `not_found` and `invalid_state` are the command's to throw. Every `usage`, `invalid_value`
    and `out_of_range` error has been reported before the function runs.
- **The state file is `%USERPROFILE%\.agentspace\leases.json` (D21).** The environment variable
  `AGENTSPACE_STATE_DIR`, when set, names another folder, and tests and worktrees MUST set it.
  - The document is `{"version":0,"leases":[...]}`, checked against
    `schemas/state/leases.state.schema.json` on every read and before every write. That schema is
    internal, not part of the contract, and lives in a subfolder that `test-schemas.ps1` does not
    scan.
  - A record holds the lease document's fields, with its state stored as `pending`, `ready`,
    `released` or `unsatisfiable`, plus an optional `driver`, the opaque name the driver returned.
    `expired` MUST NOT be stored: `ConvertTo-LeaseOutput` derives it when now is at or after
    `expires_at`, and drops `driver` and every field the state does not have.
  - Times are kept as the exact strings written. `ConvertFrom-Json` turns such strings into
    `DateTime` values unless it is given `-DateKind String` (PowerShell 7.5 and later).
- **Writes are locked and atomic.** `Update-LeaseState` holds `leases.lock`, opened with
  `FileShare.None`, for the whole read, change and write, waiting up to 30 seconds. It writes a
  temporary file in the same folder and moves it over `leases.json`. A script block that throws,
  or a result that fails the schema or repeats an ID, leaves the file byte for byte as it was.
  `Read-LeaseState` takes no lock and opens the file with `FileShare` ReadWrite and Delete, so it
  never blocks the move. The test runs three writers and a reader at once: every record arrived
  and no read failed.
- **A script block passed to `Update-LeaseState` sees that function's local variables.** PowerShell
  looks a variable up through the scopes the block was called from, so the function's own
  `$records` hid the caller's, and the first test run wrote nothing. Its locals are now prefixed
  `__lease`, and a function that runs a caller's script block SHOULD do the same.

**Findings from story E1.04 (2026-11-17).** D8 stands. E1.04 filled in `create` and `get`;
`scripts/test-lease-cmd-create-get.ps1` runs them through `scripts\lease-cli.ps1` as a child
process.
- **Three environment variables are internal test and worktree hooks, outside the contract in
  `docs/lease-api.md`.** A test MUST set `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` to
  throwaway folders, and one that runs `create` MUST also set `AGENTSPACE_PROXY_COMMAND`. Unset,
  each leaves the real default in force.
  - **`AGENTSPACE_STATE_DIR`** names the folder of `leases.json`. `lease-state.ps1` reads it.
  - **`AGENTSPACE_SSH_DIR`**, when set and not empty, is passed as `-SshDir` to
    `Invoke-LeaseDriverPlace` and `Invoke-LeaseDriverRelease`, so the alias file and the config
    `Include` go to that folder instead of the real `~/.ssh`. `lease-cmd-create.ps1` reads it,
    and `lease-cmd-release.ps1` (story E1.05) reads it too. Each reads it itself, inline.
  - **`AGENTSPACE_PROXY_COMMAND`**, when set and not empty, is passed as `-ProxyCommand` to
    `Invoke-LeaseDriverPlace`. A worktree has no `.instance-id`, so without it `Set-LeaseAlias`
    refuses to write the default ProxyCommand (D9). Only `lease-cmd-create.ps1` reads it.
- **The credential step has no root override.** A test that creates a lease writes its key under
  the real `%USERPROFILE%\.agentspace\leases\<id>`, so it MUST delete each such folder in a
  `finally`, and MUST delete only the IDs its own run made, since another process may use that
  folder at the same time. It MUST hash the real `~/.ssh/config` and list `~/.ssh/agentspace.d`
  before and after, and fail if either changed.
- **`create` picks the ID, places and writes the record inside one `Update-LeaseState` block.** A
  place that throws leaves `leases.json` untouched, and the driver has already removed its own
  credential and alias. A failure after a ready place, in the schema check or the write, happens
  outside the block, so `create` catches it, releases the lease through the driver, and rethrows
  the original error; the test makes `leases.json` a directory to prove it. The block's locals are
  prefixed `__create`, for the scoping reason above. An unsatisfiable lease is a recorded lease
  with an ID and a `driver`, and it issues nothing.
- **`scripts/test-lease-cli.ps1` no longer checks `create` and `get` as stubs**, since a `create`
  that succeeds there writes a record into its shared state file and breaks the state-file checks
  after it. It also sets `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND` to throwaway values,
  so a check that places a lease by mistake never reaches the real `~/.ssh`.

**Findings from story E1.05 (2026-11-17).** D8 stands. E1.05 built `renew`, `release` and `list`.
- **`renew` and `release` each do their lookup, driver call and record change inside one
  `Update-LeaseState` block.** A driver failure throws out of the block and leaves the record as it
  was. The block changes the record object in place and MUST NOT assign to a variable outside it,
  because the block runs in a child scope and such an assignment would be lost. The command's
  parameters are copied to differently named locals first, so that no local of
  `Update-LeaseState` can shadow them (the E1.02 finding above).
- **`renew` decides the state as `get` does** (`ConvertTo-LeaseOutput`), so a ready lease past its
  `expires_at` is `invalid_state` naming `expired`. It sets `expires_at` from the driver's returned
  valid-before time, and `ttl_seconds` to the TTL used.
- **`release` leaves `released` and `unsatisfiable` records untouched**, so the first `released_at`
  stays. A `pending` record is released with no driver call. A `ready` or `expired` one goes through
  the driver, then loses `ssh` and keeps `expires_at`.
- **`release` reads `AGENTSPACE_SSH_DIR` and passes it as `-SshDir` to the driver's release.** It is
  an internal test and worktree hook, outside the contract in `docs/lease-api.md`. Unset, the driver
  works in the real `~/.ssh`, so tests MUST set it to a throwaway folder.
- **`list` sorts with ordinal comparison** of `created_at`, then `id`, so the order does not depend
  on the culture. It reads without the lock.
- **`scripts/test-lease-cli.ps1` no longer asserts that `renew`, `release` and `list` are stubs**;
  `scripts/test-lease-cmd-renew-release-list.ps1` checks them. A command that succeeds inside the
  CLI test's shared state folder would break its later state-file checks.
- **A test output function that returns a value MUST NOT also write with `Write-Output`**, which
  would join its text to the returned value. The E1.05 test writes its PASS and FAIL lines with
  `Write-Host`.

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
  this is not yet done (card #0026). Done in story E3.02, whose findings below give the real cause.

**Findings from story E2.02 (2026-11-17).** D9 stands. The E2.01 alias, in a file of its own
under `~/.ssh/agentspace.d` and pulled in by an `Include` in `~/.ssh/config`, logged in by
certificate with no host key prompt. The Claude desktop app connected through it on 2026-11-17 at
18:12 UTC, and the box's sshd logged two logins as `ED25519-CERT ... ID spike-2.02 (serial 0) CA
ED25519 SHA256:...`, signed by the spike's throwaway CA. Test connection passed, and Terry then
ran a full Claude session on the box over that login: asked for `uname -m`, it answered
`aarch64`. On the box, the app's remote server (`server --bridge`) ran under the certificate
login's `sshd-session`, and its daemon ran the session's Claude Code.
- **The desktop app does not request a PTY.** Both of its logins showed on the box as
  `sshd-session: ubuntu@notty`, and the full session ran over one of them. The spike's signing
  commands all passed `-O permit-pty`, so this shows that the app does not use a PTY, not that a
  certificate without `permit-pty` was tested. D7's `-O permit-pty` stays, for interactive `ssh`
  sessions.
- **A plugin that sets hooks does not reach an SSH environment.** The app warned that `aws-core`
  "wasn't copied" to the alias because its `.claude-plugin/plugin.json` sets hooks, which are
  not sent over SSH; the plugins it does copy land in `~/.claude/remote/plugins`. A plugin that
  sets hooks MUST be installed on the box itself if a lease needs it.
- **The app installs its remote runtime on first connect, under `~/.claude/remote`:** `ccd-cli`
  (348 MB), `srv` (8.2 MB), `plugins` (4.5 MB), `run` and `session-folders`, 361 MB in all. A lease
  that starts from a clean box pays that download and disk at its first connection; on today's
  8 GiB disk it is about 5% of the space.
- **The `Include` MUST come before the first `Host` line of `~/.ssh/config`.** An `Include` after a
  `Host` line belongs to that host's block. `Include agentspace.d/*` resolves against `~/.ssh` in
  Windows OpenSSH, and `ssh -G <alias>` shows the alias's settings.
- **A lease alias's key paths MUST name files that a process outside the Claude app can see.** The
  desktop app's SSH connection runs outside the app's MSIX package, so a key written by an agent
  under AppData resolves to nothing there (D21). Every login before E2.09 moved the keys was made
  from inside the app, and proved nothing about the app's own connection.
- **The Claude desktop app reads `~/.ssh/config` with its `Include` and runs the alias's
  `ProxyCommand`:** its Test connection output showed `ssh-proxy.cmd`'s `Ready: i-...` lines.
- **Card #0026 blocks the desktop app whenever a token push is due.** The push's nested ssh hung,
  the tunnel never started, and the app gave up with `Connection timed out during banner exchange`.
  The hung push also left `bot.env` empty on the box while the laptop's hash recorded the token
  as sent, so no later connection pushed it again until the hash was cleared by hand (card
  #0026). Fixed in story E3.02 (below). Story E3.03 retried the desktop app with a push due, and
  the push confirmed with no banner timeout (E3.03 finding below).
- **A spike that trusts a throwaway CA SHOULD do it with a `cert-authority,principals="ubuntu"`
  line in `~/.ssh/authorized_keys`, not with an sshd drop-in.** sshd keeps the first value of
  `TrustedUserCAKeys` it reads, so a second drop-in would be ignored if it sorted after
  `50-agentspace-ca.conf` (E2.04), and would shadow it if it sorted before. The `cert-authority`
  line also trusts the CA for `ubuntu` alone, and leaves sshd's configuration untouched. sshd logs
  such a login with the same `ID <key id> ... CA ...` fields.
- **An expired AWS sign-in reaches the desktop app as a raw PowerShell error (card #0034).** The
  retest's first attempt failed before SSH started: the app showed `Proxy command failed` with a
  `NativeCommandError` from `Invoke-Aws` in `scripts/agentspace.ps1`. Under
  `$ErrorActionPreference = 'Stop'`, Windows PowerShell 5.1 turns the first line of the merged
  `2>&1` stderr into a terminating error, so the "AWS sign-in has expired" message after the
  `$LASTEXITCODE` check never ran. `Invoke-Aws` SHOULD read the AWS CLI's stderr under
  `Continue`, as the script's `Invoke-Native` (used by the `ca` checks) already does; this is not
  yet done.
- **VS Code Remote-SSH was not tried:** it is not installed on the laptop and is not a client
  Terry uses.

**Findings from story E2.06 (2026-11-17).** D9 stands. `scripts/lease-alias.ps1` writes and removes
the alias.
- **A lease's alias MUST be `agentspace-<project>-<shortid>`**, where the project is lowercase
  (`^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`) and the short ID is the first 8 hex characters of the
  lowercase UUIDv4 lease ID (D20). A lease ID that is not a UUIDv4 MUST be refused.
- **Each alias MUST be one file in `~/.ssh/agentspace.d`, named exactly for the alias.** Its first
  line names the lease ID. An existing file that names another lease (a short-ID collision) MUST
  be refused, not replaced or removed, and writing identical bytes MUST leave the file untouched.
- **A path or ProxyCommand with a line break MUST be refused, and a literal `%` MUST be written as
  `%%`**, since ssh expands `%` tokens in `IdentityFile`, `CertificateFile` and `ProxyCommand`.
- **The `Include agentspace.d/*` line MUST go on line 1 of `~/.ssh/config`, inserted as bytes** with
  the file's own line ending (and after its BOM, if it has one). The rest of the file MUST stay
  byte-identical, and the file is swapped in with `File.Replace` so its ACL stays. A backup beside
  it, with a verified SHA-256, MUST be made first. A config that does not exist MAY be created with
  only that line.
- **An `Include` found below a `Host` or `Match` line MUST be refused, not moved.** Moving it
  would mean parsing and rewriting Terry's file; a person moves it.
- **Removing an alias SHOULD leave the `Include` line in place.** It costs nothing while
  `agentspace.d` is empty, and it keeps removal from ever editing the config.
- **Temporary files MUST be written outside `agentspace.d`**, since the `Include` glob would read
  them there.
- **The default ProxyCommand MUST come from the checkout that has `.instance-id`**, because
  `ssh-proxy.cmd` reads it from its own checkout; a worktree has none, so there the caller passes
  `-ProxyCommand`.
- **`ssh -G` prints each forward's host in brackets** (`remoteforward [127.0.0.1]:8811 ...`), which
  matters to any check of a lease alias's `RemoteForward` lines.
- **Checked once against Terry's real `~/.ssh/config` (2026-11-17), backed up and restored byte
  for byte.** The config became the `Include` line plus the original bytes, with its ACL unchanged.
  `ssh -G` resolved the alias through the real `Include` with no permission complaint, `ssh -G
  agentspace` printed the same settings before and after, and an empty `agentspace.d` parsed
  cleanly. Nothing connected; a login through a written alias is story E2.08's.

**Findings from story E3.02 (2026-11-17).** D9 stands. Card #0026's hang is fixed in
`scripts/agentspace.ps1`, and `scripts/test-push-localswim.ps1` checks the push without the box.
- **The cause was an inherited environment variable, not the end of the input.** Windows OpenSSH
  hands each process it starts, the ProxyCommand included, the state of its file descriptors in a
  variable named `<GUID>_POSIX_FD_STATE` (seen as
  `c28fc6f98a2c44abbbd89d6a3037d0d9_POSIX_FD_STATE`; most likely a constant of Windows OpenSSH,
  not checked, so the code matches any GUID). `cmd.exe` and PowerShell pass it on, so the
  push's own `ssh.exe` inherited it, and that ssh never read its stdin at all. Its `-vvv` log
  reached `exec request accepted on channel 0` and then showed no read of stdin, while the same
  push outside a ProxyCommand logged the end of its input straight after that line.
- **The evidence (UTC).** A stand-in for the push, with the same Windows PowerShell 5.1 pipe into
  `ssh agentspace` and a harmless remote `cat > /dev/null; echo got-eof`, finished in 5 seconds
  from a non-interactive shell at 20:01:07. Run in place of `up` inside the ProxyCommand of a
  non-interactive ssh, it hung until it was killed at 75 seconds, and the box still held its
  `cat` at 20:03:18. Inside the ProxyCommand it also hung when the remote side read only
  `head -n 2` (the box held the `head`), when ssh's stdin was a file instead of a pipe, and when
  PowerShell's own stdin was `NUL`. A local `sort.exe` fed the same way finished at once. Removing
  the variable before starting ssh made the `head -n 2` stand-in finish at 20:10:17. Why a
  connection started from Terry's terminal never hung was not established.
- **So `head -n 2` alone would not have fixed it**, though the card proposed it: an ssh that never
  reads its stdin sends the box no lines at all.
- **The fix:**
  - **`agentspace.ps1` MUST remove every `*_POSIX_FD_STATE` variable from its environment as it
    starts**, so no ssh or scp it starts inherits one.
  - **The push's ssh MUST be Windows OpenSSH by full path, and MUST be stopped after 20 seconds.**
    A push takes 5 to 8 seconds, and a client such as the desktop app gives up on a slow banner,
    so a longer limit would still lose the connection. At the limit the process tree that ssh
    started (`taskkill /T`) is stopped, and nothing is recorded.
  - **The box MUST NOT wait for the end of the push's input.** It reads two lines (`timeout 30
    head -n 2`) into a `mktemp` file in `~/.config/localswim` under `umask 077`, checks that the
    file has two lines with a URL and a token, renames it over `bot.env` with `mv`, and prints the
    SHA-256 of the installed file. On any failure, or if its shell exits early, a `trap` removes
    the temporary file, and it exits 1, so the old `bot.env` stays as it was.
  - **The laptop MUST record the hash in `localswim-pushed.sha256` only when ssh exited 0 and the
    box's SHA-256 equals its own SHA-256 of what it meant to write**, after the same CR, BOM and
    blank-line normalization. Otherwise it warns and records nothing, and the next connection
    tries again.
- **The live run (UTC).** With the hash file set aside, `ssh -F <throwaway config> <host> 'stat
  ...'` from a non-interactive shell, through this story's `ssh-proxy.cmd`, ran from 20:13:55 to
  20:14:05 and exited 0. `up` printed `Sending the localswim Bot token to the instance...`, the
  nested `up` printed `Ready` and did not push again, and the hash file came back with the value
  it had before. A second run, with the outer ssh's stdin held open as the desktop app holds it,
  ran from 20:14:25 to 20:14:35 and exited 0: sshd logged the push's login and logout at
  20:14:31, `bot.env`'s modification time was 20:14:31.375, and the outer login followed at
  20:14:33. Afterwards `bot.env` had 96 bytes, mode 600 and two lines, its SHA-256 matched what
  the laptop would send, no temporary file was left, and no `head`, `tr`, `sed` or `cat` process
  remained on the box. `ssh agentspace true` through Terry's own entry exited 0 at 20:14:51.
- **`scripts/test-push-localswim.ps1` checks the confirm-before-record rule in Windows PowerShell
  5.1.** It loads `Push-Localswim` from the script's syntax tree and runs it with a dummy token
  and a stand-in for ssh. A wrong hash, a failed ssh and a hung ssh each warn and record nothing,
  a confirmed send is recorded, and a failed forced send leaves the earlier record as it was.
- **Not tried with a push due in this story:** an interactive connection from Terry's terminal,
  and the desktop app's own connection. Story E3.03 (#0040) set
  `%USERPROFILE%\.agentspace\localswim-pushed.sha256` aside and then connected the desktop app
  through a lease: the push confirmed and E2.02's `Connection timed out during banner exchange`
  was gone (E3.03 finding below). An interactive connection from Terry's terminal with a push due
  was not tried in E3.03 either.
- **Review changes after the live run:** the time limit went from 60 to 20 seconds, the timeout
  stops ssh's whole process tree, and the box's command gained the `trap`. The live run was then
  repeated the same way, with the hash file set aside: it pushed and exited 0 in 7 seconds,
  ending at 20:24:55 UTC, the hash file came back with the same value, `bot.env` had 96 bytes,
  mode 600, two lines and a matching SHA-256, no temporary file or push process was left on the
  box, and `ssh agentspace true` exited 0 at 20:25:14.

**Findings from story E3.03 (2026-11-17).** D9 stands. swimcoach took a real lease through the CLI,
and the Claude desktop app reached the box through its alias, with a token push due, from a
stopped box. The README's "Use a lease from a project" is the how-to that came out of it.
- **The lease.** `bin\agentspace.cmd lease create --project swimcoach`, run from the main checkout
  `C:\Projects\agentspace` (which has `.instance-id`), exited 0 at 20:42:59 with state `ready`, a
  TTL of 28800 seconds, `expires_at` 2026-11-18T04:42:59Z and the alias
  `agentspace-swimcoach-a62cb513`. `~/.ssh/config` had no `Include` and `~/.ssh/agentspace.d` did
  not exist (E2.08 had restored the config), so `create` inserted `Include agentspace.d/*` as line
  1, backed the config up as `config.agentspace-backup-20261117T204259Z`, and wrote a 697-byte alias
  file. `ssh -G` on the alias showed user `ubuntu`, `identitiesonly yes`, `stricthostkeychecking
  true`, `hostkeyalias agentspace`, both `remoteforward [127.0.0.1]:...` lines, the key and
  certificate under `%USERPROFILE%\.agentspace\leases\<id>\`, and the main checkout's
  `ssh-proxy.cmd` as ProxyCommand. A scripted login through the alias at 20:43:07 logged
  `Accepted publickey ... ED25519-CERT ... ID a62cb513-... (serial 0) CA ED25519 SHA256:G05h...`.
- **The push stamp is not `bot.env`'s hash.** `localswim-pushed.sha256` holds the SHA-256 of the
  instance ID, a line feed and the content (`agentspace.ps1`), so it never equals `sha256sum
  bot.env` on the box. A push is shown to be confirmed when the stamp comes back equal to the stamp
  set aside (the same instance and the same token), and `bot.env`'s modification time moves. A
  check MUST compare the stamp with the set-aside stamp, never with `bot.env`'s `sha256sum`.
- **Setting the stamp aside MUST be the last step before the connection under test, with no other
  connection in between.** Any connection to the box pushes while the stamp is missing, scripted
  ones included. In this run the stamp was set aside at 20:43:21, and the dev lead's own `ssh
  agentspace` box check at 20:43:41 then printed `Sending the localswim Bot token to the
  instance...`; the box confirmed, and the stamp came back equal, so the desktop app's push was no
  longer due. The stamp was set aside again at 20:44:49, and nothing connected until Terry's run.
- **The desktop app connected through the lease alias with a push due, from a stopped box.** Terry
  added the SSH environment `agentspace-swimcoach-a62cb513` in the Claude desktop app and clicked
  Test connection; he did not open a session. The box had been stopped at 21:06:45 by the
  CloudWatch backstop (D11's E3.03 finding), and booted at 22:12:11 (`uptime -s`), so
  start-on-connect worked through the lease alias from the app.
- **The app's connection pushed the token, with no hang.** sshd logged the push's nested login at
  22:12:37; its session opened at 22:12:37 and closed at 22:12:38. Afterwards `bot.env` had 96
  bytes, mode 600, two lines and a modification time of 22:12:38.002, in a folder of mode 700. The
  laptop's stamp came back at 22:12:40 by the laptop's clock, equal to the stamp set aside.
- **The push's nested login still uses Terry's own key (#0028 stands).** sshd logged it as
  `Accepted publickey ... ED25519 SHA256:...` with no `ID`: it goes through Terry's `agentspace`
  entry and `authorized_keys` key, not the lease's certificate.
- **The app's certificate logins followed with no banner timeout.** sshd logged `ED25519-CERT ...
  ID a62cb513-...` at 22:12:42, whose `sshd-session ... @notty` runs
  `~/.claude/remote/srv/<hash>/server --bridge`, and at 22:12:49, which runs `server --serve`.
  E2.02's `Connection timed out during banner exchange` with a push due is gone from the desktop
  app, which closes E3.02's untried item for it and E2.02's card #0026 note.
- **The app's remote log is `~/.claude/remote/run/<id>/remote-server.log`.** It was listening at
  22:12:50, and at 22:14:30 its GitHandler looked at `/home/ubuntu/swimcoach`.
- **Test connection ran no Claude Code on the box:** there was no `ccd-cli` process and no
  transcript. The app keeps a session's transcript on the laptop, so the box's `~/.claude/projects`
  is not evidence either way.

**Findings from 0.02.T2.1's live runs (2026-11-17).** D9 stands. With Terry's `agentspace` entry
removed from `~/.ssh/config`, a new lease's alias connected and pushed the Bot token over itself
(card #0057). This ends, for a lease alias, the dependency on Terry's entry that E2.01 and E3.03
found (#0028). Times are Pacific, the laptop's zone. The first run, below, was made against
5fabf9c. Review then simplified `ssh-proxy.cmd` (no read-back through `%TEMP%`, and the tunnel's
exit code kept), gave the push's ssh `-o BatchMode=yes`, and made the alias checks
case-sensitive, so the run was repeated against 660a084; that second run follows the first, and
the first run's facts it supersedes are marked.
- **The lease.** `.instance-id` was copied into the story's worktree, so the default ProxyCommand
  named the worktree's new `ssh-proxy.cmd`. `bin\agentspace.cmd lease create --project t2verify
  --ttl 1h`, run from the worktree, exited 0 with state `ready` and the alias
  `agentspace-t2verify-1940a085`. The config already had `Include agentspace.d/*` on line 1, so
  `create` left it unchanged (same SHA-256). `ssh -G` on the alias showed `proxycommand
  C:\Windows\System32\cmd.exe /d /c "<worktree>\scripts\ssh-proxy.cmd" agentspace-t2verify-1940a085`.
- **Terry's entry was removed for the test, and nothing else.** The config was copied aside and
  hashed first. Then the `Host agentspace` line and its 11 indented lines were removed, leaving
  the `Include` and the two comment lines above the block. `ssh -G agentspace` then resolved to a
  bare host named `agentspace` with ssh's default keys and no ProxyCommand.
- **The push rode the lease's certificate.** The push stamp was set aside as the last step, and
  `ssh <alias>` ran a check command from a non-interactive PowerShell 7. The box was already
  running. The connection started at 5:08:33pm and exited 0 at 5:08:42pm.
  - `up`'s stderr printed `Sending the localswim Bot token to the instance...`, and the nested
    `up` printed `Ready` and did not push again.
  - sshd logged the push's nested login at 5:08:40pm as `ED25519-CERT ... ID 1940a085-... (serial
    0) CA ED25519 ...`, the lease's certificate, not Terry's key. Its session opened and closed in
    that second, and the outer login followed at 5:08:41pm with the same certificate.
- **How the push was proven, without the token.**
  - `bot.env` had 96 bytes, mode 600 and two lines, and its modification time was 5:08:40.413pm,
    7 seconds after the connection started, so it was this connection's write.
  - Its `sha256sum` equalled the hash the laptop would push. That hash was computed by
    `agentspace.ps1`'s own `Get-BotEnvHash`, loaded from the script's syntax tree, over content
    built as `Push-Localswim` builds it; only the hash was printed.
  - The stamp came back equal to the one set aside.
- **The board answered on the box.** The connection's `RemoteForward` lines bound 127.0.0.1:8811
  and :53147, and `localswim board list` returned the `agentspace` board.
- **The config was restored byte for byte** by a `Copy-Item` of the copy, in a `finally` step. Its
  SHA-256 matched the one taken before any change (752 bytes) right after the restore, and again
  after the lease was released.
- **The no-argument path still pushes over `agentspace`.** With the config restored and the stamp
  set aside again, `ssh -o ProxyCommand=<worktree's ssh-proxy.cmd, no argument> agentspace` exited 0
  at 5:09:06pm. sshd logged the push's login at 5:09:04pm as `ED25519 SHA256:...` with no `ID`
  (Terry's key), `bot.env` had the same size, mode and SHA-256 with a modification time of
  5:09:04.382pm, and the stamp came back equal. Plain `ssh agentspace`, through the main
  checkout's unchanged proxy, also connected and exited 0, with no push due. Superseded by the
  second run's no-argument check, against the simplified proxy, below.
- **`ssh-proxy.cmd`'s argument check refused a bad alias through ssh.** With `-bad` after the
  script in the ProxyCommand, it printed its `takes at most one argument` message and started
  nothing, and ssh exited 255 with `Connection closed by UNKNOWN port 65535`. It was run once, not
  retried (D14). That check, and the read-back through `%TEMP%` behind it, were dropped after
  review: the outer `cmd.exe` parses the whole ProxyCommand line before `ssh-proxy.cmd` runs, so
  the gate is `lease-alias.ps1`'s exact check, and `up` now refuses `-bad` with its own `-Alias`
  message before any ssh (`scripts\test-push-localswim.ps1`). Superseded by the second run's
  check of a second argument, below.
- **The lease was released** (`lease release` exited 0). Its alias file and key folder were gone,
  `agentspace.d` was empty, the `Include` stayed, and the copied `.instance-id` was deleted.
- **`up` prints `Ready: <instance ID>` to the caller's stderr.** A check that captures a
  ProxyCommand's stderr therefore captures the instance ID, and it MUST withhold those lines from
  anything committed or reported.
- **Not tried in the first run:** a connection from a stopped box through the new path, and the
  desktop app through it. The second run's first connection started from a stopped box
  (superseded for that item, below).

**The second run, against 660a084 (2026-11-17).** Done the same way as the first, from a
non-interactive PowerShell 7, with Windows OpenSSH alone. The config was copied aside and hashed
first, `.instance-id` was copied into the worktree, `lease create --project t2verify --ttl 1h`
ran from the worktree, Terry's `Host agentspace` line and its 11 indented lines were removed, the
expected hash was computed by `Get-BotEnvHash`, and the stamp was set aside as the last step.
- **Two leases were created, one connection each.** The first,
  `agentspace-t2verify-1ab69770`, connected and pushed, but the check script did not print that
  connection's own output, so its board answer and `bot.env`'s modification time from that
  connection were not recorded. A second lease was then created and connected once, on a running
  box; its record follows. Nothing failed, so this was not a retry under D14.
- **The first connection started a stopped box through the new path.** The connection started
  at 5:27:26pm, EC2 launched the box at 5:27:29pm, and sshd logged the push's login at 5:28:08pm
  as `ED25519-CERT ... ID 1ab69770-... (serial 0) CA ED25519 ...`, with its session opened and
  closed in that second. The outer login followed at 5:28:10pm with the same certificate. The
  stamp came back equal to the one set aside, and `bot.env`'s SHA-256 equalled the laptop's.
- **The lease of record.** `lease create` exited 0 at 5:29:42pm with state `ready` and the alias
  `agentspace-t2verify-e2305470`, and left the config's SHA-256 unchanged. `ssh -G` on the alias
  showed `proxycommand C:\Windows\System32\cmd.exe /d /c "<worktree>\scripts\ssh-proxy.cmd"
  agentspace-t2verify-e2305470`, with the alias still `batchmode no`: BatchMode is on the push's
  ssh alone. With Terry's block removed, `ssh -G agentspace` again resolved to a bare host with
  ssh's default keys and no ProxyCommand.
- **The push rode the lease's certificate, with BatchMode on.** The connection started at
  5:29:42pm and exited 0 at 5:29:57pm. `up`'s stderr printed `Sending the localswim Bot token to
  the instance...` once. sshd logged the push's nested login at 5:29:55pm as `ED25519-CERT ... ID
  e2305470-... (serial 0) CA ED25519 ...`, its session opened and closed in that second, and the
  outer login followed at 5:29:56pm with the same certificate. So `-o BatchMode=yes` does not
  break a lease's push.
- **How the push was proven, without the token.** `bot.env` had 96 bytes, mode 600 and two lines,
  in a folder of mode 700 that held nothing else, and no `head`, `tr` or `sed` process was left.
  Its modification time was 5:29:55.375pm, 12 seconds after the connection started. Its
  `sha256sum` equalled the laptop's `Get-BotEnvHash`, and the stamp came back equal to the one set
  aside.
- **The board answered on the box.** Over the lease's connection, `localswim board list` returned
  the `agentspace` board (`c74ddc68-...`) and the `swimcoach` board.
- **The config was restored byte for byte** by a `Copy-Item` of the copy, in a `finally` step. Its
  SHA-256 matched the one taken before any change (752 bytes) right after the restore, after the
  lease was released, and at the end of the run, for both leases.
- **Both leases were released** (`lease release` exited 0, state `released`). Each alias file and
  key folder was gone, `agentspace.d` was empty, the `Include` stayed on line 1, and the copied
  `.instance-id` was deleted at the end.
- **The no-argument path still pushes over `agentspace`, with BatchMode on.** After the first
  lease was released, with the config restored and the stamp set aside again, `ssh -o "ProxyCommand=C:\Windows\System32\cmd.exe /d /c
  \"<worktree>\scripts\ssh-proxy.cmd\"" agentspace` exited 0. sshd logged the push's login at
  5:28:25pm as `ED25519 SHA256:...` with no `ID` (Terry's key), `bot.env` had the same size, mode
  and SHA-256 with a modification time of 5:28:25.792pm, and the stamp came back equal. Plain `ssh
  agentspace true`, through the main checkout's proxy, exited 0 at 5:28:35pm with no push due.
- **A second argument is refused before anything starts.** Before the second lease, with
  `agentspace extra` after the
  script in the ProxyCommand, `ssh-proxy.cmd` printed `takes at most one argument, the SSH host
  alias.`, no `up` output followed, and ssh exited 255 in 55 milliseconds with `Connection closed
  by UNKNOWN port 65535`. It was run once (D14).
- **The tunnel's exit code was not observed live.** ssh ends its ProxyCommand when the connection
  closes, so the code never reaches the caller, and observing it would mean forcing a failure on
  the box. `scripts\test-push-localswim.ps1` checks it with a stand-in tunnel that exits 7.
- **The `Ready: <instance ID>` rule held:** the check filtered every `Ready:` line and every
  instance ID from what it printed.
- **Still not tried:** the desktop app through the new path. E0.02-T03-S01 tried it (below).

**Findings from story E0.02-T03-S01 (2026-11-18).** D9 stands, and #0028 is closed for a lease
alias, the desktop app included. The evidence is in
[docs/runs/e0.02-t03-s01.md](runs/e0.02-t03-s01.md). Times are UTC.
- **With Terry's `agentspace` Host block removed, the lease alias pushed over the lease's
  certificate.** The push's nested login, at 02:19:09Z, was `ED25519-CERT ... ID 534bdf04-...`,
  and `bot.env` was rewritten inside it. The config was restored byte for byte, hash for hash.
- **The desktop app's own proxy pushed over the lease's certificate.** The app wrote
  `proxy-command.txt` with the alias's ProxyCommand, trailing alias included, at 04:24:30Z. Its
  `ssh.exe` ran it through `claude-ssh-proxy.exe`, and the push's nested login, at 04:24:42Z, was
  `ED25519-CERT ... ID 5034f839-...`. `bot.env` was rewritten 0.25 seconds later, inside that
  session, and the laptop's stamp came back equal to the one set aside.
- **Test connection did not show the push.** Terry saw only its closing `connection successful`
  line, not `Sending the localswim Bot token to the instance...`. A push from the desktop app
  MUST be confirmed on the box or by the laptop's stamp, never from the app's output.
- **Two `up`s on a stopped box both push.** With the stamp deleted and the box stopped, the
  recorder's connection over Terry's entry and the keeper's holder over the lease alias both
  started `up` within seconds, and sshd logged two nested logins at 04:17:18Z, one per
  credential. Each push wrote the same token, so nothing broke, but each push is a login.

## What the MVP leaves alone

### D10. Releasing a lease does not revoke its certificate

**Status: MVP only; replaced by card #0010.**

**MVP only.** Release deletes the lease's key on the laptop, and the certificate stays valid until
it expires. TTLs are short, so the exposure is bounded. **Replaced by card #0010**, which pushes a
revocation list (KRL) to hosts.

**Finding from story E2.01 (2026-11-16):** sshd checks a certificate's validity only at login, so
neither release nor expiry ends a session that is already open. They stop new connections only.

**Findings from story E2.07 (2026-11-17).** D10 stands. `Remove-LeaseCredential` in
`scripts/lease-credential.ps1` releases a lease.
- **Release MUST delete the lease's alias file through `Remove-LeaseAlias` first**, which refuses an
  alias file that names another lease; nothing else is deleted then. It then deletes the lease
  folder under `%USERPROFILE%\.agentspace\leases` (D21) with its key, public key and certificate.
- **Release MUST NOT edit `~/.ssh/config`.** The `Include agentspace.d/*` line stays.
- **Release needs no CA and runs no CA check.** It does not revoke the certificate, which stays
  valid until it expires.
- **Release is idempotent.** A second call, or a call for a lease that never existed, succeeds and
  reports both removals as false.

**Findings from story E2.08 (2026-11-17).** D7 and D10 stand. One throwaway lease (project
`e208test`, a 7-minute certificate valid from 18:55:59 to 19:02:59 UTC) went through issue, alias,
login, expiry and release on the real box, through an `Include` in Terry's real `~/.ssh/config`,
which was backed up first and restored byte for byte afterwards (same SHA-256).
- **The alias logged in by certificate**, through `ssh-proxy.cmd` in the main checkout, at
  18:56:05 UTC. sshd logged `Accepted publickey for ubuntu from 127.0.0.1 port ... ssh2:
  ED25519-CERT SHA256:... ID <lease id> (serial 0) CA ED25519 SHA256:G05h...`, so the journal
  names the lease. Terry's own logins in the same window logged `ED25519 SHA256:...` with no `ID`.
  The README's Logs section shows how to search for it.
- **A session open at expiry kept running.** A loop started through the alias at 18:56:16 UTC
  printed every 30 seconds through the expiry at 19:02:59 and on to its planned end (last line at
  19:07:46 UTC), and ssh exited 0 at 19:08:18 UTC. sshd never ended it, as D10's E2.01 finding
  says.
- **After expiry a new login was refused.** One attempt at 19:03:30 UTC got `Permission denied
  (publickey)`, and sshd logged `Refusing certificate ID "<lease id>" serial=0 signed by ED25519
  CA SHA256:G05h...: Certificate invalid: expired`.
- **One expired attempt cost one certificate refusal, and ssh then offered the bare key.** With
  `IdentitiesOnly yes`, ssh offered the certificate and then the lease's plain key. Each offer most
  likely produced one `AuthorizedKeysCommand .../eic_run_authorized_keys ... failed, status 22`
  line from EC2 Instance Connect (D7, E2.04 finding), two in all, with the same fingerprint, since
  a certificate and its key share one. No `PerSourcePenalties` line was logged.
- **The refused connection stayed open on the box for two minutes after ssh gave up.** The client
  exited at 19:03:34 UTC, but its `sshd-session` lived on until sshd logged `Timeout before
  authentication for connection from 127.0.0.1` at 19:05:33 UTC, `LoginGraceTime`'s 120 seconds
  later. Most likely the Session Manager tunnel did not pass the close on to sshd: a refused
  client exits without sending a disconnect, while the open session's clean exit reached sshd as
  `Received disconnect ... disconnected by user` two seconds before ssh returned. A refused
  attempt through Session Manager therefore most likely holds a pre-authentication slot for two
  minutes.
- **That lingering refusal is penalised as `grace-exceeded`, not `authfail`.** `sshd -T` on the
  box reports `persourcepenalties ... authfail:5 noauth:1 grace-exceeded:10 ... min:15`,
  `persourcepenaltyexemptlist none`, `maxstartups 10:30:100` and `logingracetime 120`. sshd scores
  a connection once, so this attempt scored 10 seconds, below the 15-second minimum, which is why
  no penalty was logged. Two such refusals ending close together most likely reach the minimum
  and shut 127.0.0.1 out, which is every login, Terry's included. Separately, about ten lingering
  refusals within two minutes would fill `MaxStartups` and drop new connections whatever the
  penalties do. Exempting 127.0.0.1 from penalties does not free those slots, so card E3.01
  SHOULD also consider a shorter `LoginGraceTime`.
- **A second session through any alias gets `Warning: remote port forwarding failed` for 8811 and
  53147** while another session holds those forwards on the box. The login still succeeds; only
  the second session's forwards are lost. The lease alias and Terry's `agentspace` entry forward
  the same ports, so two concurrent sessions of either kind collide. This is D12's shared forward
  seen from the client side, and card #0008 replaces it.
- **Release removed the alias file and the lease folder**, and left `~/.ssh/config` alone, as the
  E2.07 findings say.
- **No token push was due**, so card #0026's hang was not met.

**Findings from story E3.03 (2026-11-17).** D10 stands. The swimcoach lease was released from the
main checkout with `bin\agentspace.cmd lease release a62cb513-...`, which exited 0 at 22:24:45
with state `released` and `released_at` 2026-11-17T22:24:45Z.
- **Release removed the lease's alias file and key folder, and left `~/.ssh/config` alone.**
  Afterwards `~/.ssh/agentspace.d` was empty, `%USERPROFILE%\.agentspace\leases\<id>` was gone,
  the config's SHA-256 was the same as before the release, and its line 1 was still `Include
  agentspace.d/*`. The `Include` stays on purpose (E2.06): it is how the next lease's alias is
  found.
- **`ssh agentspace` worked afterwards** (exit 0 at 22:24:58), through Terry's own entry.
- **Release ended nothing that was open.** At 22:25:56 the box still had the desktop app's
  22:12:49 connection (`server --serve`) and the holder's `sshd-session`, both opened under the
  lease's certificate; the app's 22:12:42 connection had closed by then, for a reason not
  established. The holder still held the forwards at 22:24:58. No `ccd-cli` was running on the
  box at release, so no agent turn was cut off. An open session is ended by closing it, never by
  release.

**Findings from story 0.02.T1.1 (2026-11-17).** D10 stands. Release still ends no open session,
but a session whose client has gone no longer lingers: since 0.02.T1.1, sshd drops it about 90
seconds after the box's side of it goes quiet (D12's 0.02.T1.1 findings, which give the
exception). The E2.08 mechanism is confirmed: the Session Manager tunnel never passed a killed
client's close on to sshd, and every dead session that had ended did so by `Timeout, client not
responding`, never by a disconnect. A live session open at release keeps running, as before.

### D11. Today's idle stop stays

**Status: MVP only; replaced by v0.08 (#0048), as D56 and D57 design.**

**MVP only.** The box still powers off after 60 minutes with no SSH or Session Manager session and
no CPU work. An agent holding a lease but doing nothing can still have the box stopped under it;
its next connection starts it again. **Replaced by v0.08 (#0048)**, which stops a project's host
when the project has no live leases (D25). It folded in card #0006.

**Findings from story E3.03 (2026-11-17).** D11 stands, but the box has a second stopper that does
not count sessions.
- **The CloudWatch backstop stopped the box under two open SSH sessions** (card #0043). Terry's
  E2.02 desktop-app connections had been open and quiet since 18:12. The on-box idle check logged
  `Active: SSH session open` at 21:04:36, and at 21:06:45 the box logged `Power key pressed short`,
  an EC2 stop from outside. The alarm `agentbox-idle-backstop` (3 hours averaging under 1% CPU)
  went to ALARM and logged `Stop EC2 Instance ... action completed successfully` at the same
  second. Both sessions died with the box.
- **So a lease whose agent stays connected but quiet for 3 hours loses its box**, sessions
  included, whatever the on-box check says; its next connection starts the box again. A working
  agent keeps the CPU above 1%, so this is not an MVP blocker (D19).

### D12. Board access is unchanged

**Status: MVP only; replaced by card #0008, and its hand-run holder by D30.**

**MVP only.** The first `ssh` connection still binds the localswim board's reverse forward for the
whole box, and every lease uses it, because every lease is `ubuntu`. **Replaced by card #0008.**
Per-project users (#0004), which it once had to land with, are OBE under D25: each project's host
has its own forward, so #0008 matters once two leases of one project share a host. v0.02 (#0045)
makes the one-lease case work unattended (#0042, #0044).

**Findings from story E3.03 (2026-11-17).** D12 stands, with a gap it did not foresee: the Claude
desktop app's own connection never holds the forward, so a desktop-app session reaches the board
only while some other connection holds it.
- **Before the run nothing held the forward.** At 20:42 nothing on the box listened on 8811 or
  53147 (`ss -ltnp`). Terry's two E2.02 desktop-app connections to the spike alias
  `agentspace-spike`, open since 18:12, were still up and held no forwards.
- **The desktop app clears every forward (card #0042).** The app's `ssh.exe` command lines, read
  from the laptop's process list, carry `-o ClearAllForwardings=yes`, beside `-o
  StrictHostKeyChecking=yes`, `-o ConnectTimeout=30`, `-o ServerAliveInterval=15 -o
  ServerAliveCountMax=3`, `-o ForwardAgent=no` and `-T`. Their `-o ProxyCommand=` names the app's
  own `claude-ssh-proxy.exe` with a file `%TEMP%\ccd-ssh-<random>\proxy-command.txt`, which held
  the alias's own ProxyCommand line unchanged. One connection runs a remote shell loop, the other
  the `sftp` subsystem.
- **So a lease alias's `RemoteForward` lines never apply to the app's own connection**, and the
  same holds for Terry's `agentspace` entry.
- **A desktop-app session that needs the board MUST have a holder open on the laptop for as long
  as it needs the board:** `C:\Windows\System32\OpenSSH\ssh.exe -N -o ExitOnForwardFailure=yes
  <alias>`. In this run the holder was started only after the app's push had landed, so it never
  pushed. It logged in by certificate under the lease ID at 22:12:47, and `sudo ss -ltnp` showed
  its `sshd-session` (shown as `sshd-session: ubuntu`, with no tty) holding 127.0.0.1:8811 and
  :53147. `curl` to 127.0.0.1:8811 from the box returned 200.
- **The holder is a workaround, not the fix.** Card #0042 (Backlog, not an MVP blocker),
  "Desktop-app sessions get no board: the app clears a lease alias's RemoteForward", tracks it.
- **A swimcoach agent worked a card on the board from a desktop-app session on the lease.** Terry
  opened a session on `agentspace-swimcoach-a62cb513` in `/home/ubuntu/swimcoach` and gave it a
  throwaway prompt. Through `scripts/localswim.sh` the agent listed the swimcoach board's 7 lanes,
  created swimcoach card #0057, "agentspace E3.03 lease check (throwaway)", in Backlog at 22:22:05,
  commented on it at 22:22:09 with the box's hostname and the time, and archived it at 22:22:13.
  The swimcoach board's activity log shows the three changes as `bot`, and the card reads back
  archived with one comment. The board reached the box through the holder's forwards: no other
  connection held them.
- **Opening a session made no new login.** sshd logged no certificate login after the app's two at
  22:12:42 and 22:12:49, and the app's `server --bridge` and `server --serve` stayed the same
  processes, so the session ran over the connections Test connection had opened. Which of the two
  carried the session was not determined. Every other login in that window was a check by the dev
  lead or the architect, with Terry's key.
- **Killing the holder on the laptop did not free the forwards on the box.** The holder's
  `ssh.exe` was stopped at 22:24:59, after release, and no proxy process was left on the laptop,
  but its `sshd-session` on the box still held 127.0.0.1:8811 and :53147 at 22:25:19. This is
  the E2.08 mechanism (D10): the Session Manager tunnel does not pass the close on to sshd.
  It still held them at 22:35:23, over 10 minutes later, checked once a minute. At 22:35:25 the
  dev lead killed that one `sshd-session` by its PID, after checking that it was the holder's (no
  tty, started 22:12:47), and both ports were free.
- **While a dead client's session holds the forwards, no other connection on the box can bind the
  board**, Terry's own sessions included. sshd's `ClientAliveInterval` defaults to 0, so it never
  probes a client that has gone. Card #0044 (Backlog) weighs setting it in
  `51-agentspace-logins.conf` against E3.01's grace-time reasoning. Until then a stale forward
  MUST be cleared by hand: find the `sshd-session` that holds the port with `sudo ss -ltnp`, check
  it is the one whose client has gone, and kill only that one.

**Findings from story 0.02.T1.1 (2026-11-17).** D12 stands, and the gap above is closed for a
dead client: sshd now drops its session, and the forwards with it, about 90 seconds after the
box's side of the session goes quiet. That is not a bound that holds whatever the box does: sshd
probes a client only after a whole interval with nothing to do on the session's sockets, so
steady activity on the box's side, such as calls to the board through a dead forward every few
seconds, puts the probe off (the watcher's finding below). The E3.03 rule to clear a stale forward
by hand is replaced by this, except in two cases:
- **A session that still holds the forwards 2 minutes after its client went MUST be cleared by
  hand as E3.03 says**, killing only the session whose client has gone.
- **A holder whose client is alive MUST be closed on the laptop, never killed on the box.**

The box runs OpenSSH 10.2p1 (`OpenSSH_10.2p1 Ubuntu-2ubuntu3.8`).
- **The values: `ClientAliveInterval 15` and `ClientAliveCountMax 5`**, in
  `box/system/51-agentspace-logins.conf`, with the reasoning in its comments. sshd drops a client
  that stays silent for (5 + 1) x 15 = 90 seconds while the box's side is quiet too, under the
  card's 2 minutes. `install.sh`'s `sshd -T` gate now checks all four keywords of that file.
  `agentspace.ps1 sync` installed it, and the drops below show both values in effect. The sync's
  own output was not kept with the run's evidence.
- **The check is `scripts/check-forward-release.ps1`**, run against a throwaway lease (project
  `agentspace-check`, created 23:32:40Z, released 23:51:42Z, both from the lease's entry in the
  laptop's `%USERPROFILE%\.agentspace\leases.json`) from its alias, twice.
- **A killed holder's session was gone, and both ports free, 86 to 90 seconds after the kill.**
  - Run 1 (`-BoundSeconds 90`, 23:32:46Z): the holder logged in at 23:32:53Z and held
    127.0.0.1:8811 and :53147, and was killed on the laptop at 23:32:57Z. sshd logged `Timeout,
    client not responding from user ubuntu 127.0.0.1 port ...` at 23:34:23.6Z. The box's 5-second
    sample at 23:34:24Z showed both ports free, and the check printed `Phase 1 passed: both ports
    free and the holder's sshd-session gone 89.9 s after the kill (bound 90 s)`.
  - Run 2 (`-BoundSeconds 120`, 23:43:24Z): the holder logged in at 23:43:30Z, was killed at
    23:43:34Z, and timed out at 23:45:00.4Z. Both ports were free, and its `sshd-session` pids
    gone, in the sample at 23:45:01Z. The check printed the same 89.9 s, against a bound of 120 s.
  - **sshd timed each drop from the last packet it had from the client**, not from the kill: the
    holders timed out 90.1 s after login, since each was killed 4 s after it, before its first
    probe. The check's 89.9 s also counts the kill itself and the 5-second sample, which is why
    run 2 used the card's 2 minutes as its bound rather than the 90 seconds itself.
- **An idle live session survived twice the bound.** Run 2's phase 2 opened `ssh -N -o
  ClearAllForwardings=yes <alias>` at 23:45:05Z and printed `Phase 2 passed: after 240.5 s the
  idle session is still connected and its sshd-session (pid ...) is still on the box`, then
  `Result: Pass.`, exit 0. It answered every 15-second probe. Once the check killed it, at
  23:49:16Z, sshd dropped it at 23:50:36.8Z, most likely 90 seconds after its last answer, which
  is 75 to 90 seconds after a kill. Run 1's idle session did the same: killed at 23:35:57Z,
  dropped at 23:37:14.0Z.
- **No desktop-app connection was open during either run.** Run 2's baseline line listed only
  the check's two watchers' `sshd-session` pids, and a `ps` at 23:36:25Z showed only the check's
  sessions and the session of the login that captured it. So the desktop app's
  ServerAliveInterval of 15 was weighed in the drop-in's comments, not tested live.
- **The watcher's own session outlived its client for 9 minutes.** The check's watcher runs a
  loop on the box that writes a sample every 5 seconds until a deadline. Run 1's watcher was
  killed at 23:35:57Z, but its `sshd-session` lived on until `Timeout, client not responding` at
  23:44:51.3Z, about 90 seconds after the loop's deadline. Run 2's watcher, killed at 23:49:16Z,
  was still on the box at 23:51:18Z, with its loop's deadline about 23:55:26Z. This shows the
  mechanism the drop-in's comments state: sshd probes only after a whole interval with nothing to
  do on the session's sockets, so steady output on the box's side puts the drop off. The tunnel
  takes those writes without error, so the loop does not notice its client has gone. The
  watcher clears every forward, so it never held the board's ports.
- **The journal line needs `_COMM=sshd-session`, not `-u ssh`.** `journalctl -u ssh` showed the
  `Accepted publickey` lines but none of the timeouts or `session closed` lines, most likely
  because pam_systemd moves a session into its own scope once it opens. `sudo journalctl
  _COMM=sshd-session --since ... | grep 'Timeout, client not responding'` finds them.
- **Run 1's phase 2 failed on a parser bug in the check, not on the box.** `ps -eo
  pid=,ppid=,args=` pads its columns, and the pattern `'^S (\d+) (\d+) (.*)'` allowed one space,
  so the check never saw an `sshd-session` pid, and phase 2 timed out waiting for the idle
  session's. Run 1's phase 1 result rests on the ports alone, and stands. The pattern is now
  `'^S\s*(\d+)\s+(\d+)\s+(.*)'`, and run 2 checked the holder's pids as well.

**Findings from story E0.02-T03-S01 (2026-11-18).** D12 stands for one lease at a time, and the
gap E3.03 found is closed by the keeper (D30): a desktop-app session reached the board with no
holder started by hand. The evidence is in [docs/runs/e0.02-t03-s01.md](runs/e0.02-t03-s01.md).
Times are UTC.
- **The keeper's holder held the forwards for the app's session.** On the box, `sudo ss -ltnp`
  showed one `sshd-session` holding 127.0.0.1:8811 and :53147, and the login behind it was the
  keeper's holder, by the lease's certificate. The app's own connections held nothing.
- **An agent in the app session worked a card on swimcoach's board.** Through
  `scripts/localswim.sh` it listed the board's seven lanes, created swimcoach card #0058,
  "agentspace T03 lease check (throwaway)", in Backlog, commented on it with the box's host name
  and the time (04:26Z), and archived it. The dev lead read the card back from the board: created
  04:26:00Z, one comment at 04:26:05Z, `archived: true`. The agent's own read-back showed
  `archived_at` null; that is localswim's response as observed, not chased here.
- **0.02.T1.1's bound held for both kinds of stop.** A killed holder's session timed out 79.5
  seconds after the kill, and a holder the keeper stopped at release timed out 86 seconds after the
  stop. Both ports were free in the next 15-second sample each time.
- **Archiving a desktop-app SSH session does not close its connections.** After Terry archived
  the session, the app's two `ssh.exe` processes (parent `claude.exe`, both started 04:24Z) and
  their sessions on the box were still up 3 minutes later, and release ended neither (D10). They
  carry no forwards, so they did not hold the board, but they keep the box's idle check from
  stopping it, and they keep a released lease's login open. The architect killed both by PID at
  04:45Z with Terry's approval. Card #0064 tracks the fix, outside this story.

## Defaults

### D13. arm64 is the default platform

**Decided.** When a request names no platform, it gets `linux/arm64`, because Graviton costs less
for the same agent work. `linux/amd64` is the planned second platform (v0.12, #0051, which folded
in card #0013), not a fallback.

## How the work is run

### D14. Roadmap horizons, epics before stories

**Status: Decided; its horizons were replaced by D26, and its story names for releases by D29.**

**Decided, Terry, 2026-11-16.** The roadmap runs from the MVP through Next and Later to Someday.
**Its horizons were replaced by D26 (2026-11-17):** releases v0.02 to v1.0, one epic each, then
After 1.0 and Someday. The rules on epics and stories below stand, except that a release's
stories are named `EX.YY-Tnn-Smm` (D29).
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
  until its last story reaches Ready For Review.** **Restated by Terry, 2026-11-17:** an epic goes
  to In Progress as soon as work starts on any of its stories, and follows its last story to
  Ready For Review as soon as that story arrives there. The architect moves the epic both times.
  A release epic still in Backlog leaves it by Terry's hand (D29). Its description carries the execution plan by
  turn: each story starts in the first turn its entry gates are met, and the stories in one turn
  MUST be workable in parallel, in isolation from each other.

**Epic 2 went first, on purpose, to take the risk out of the MVP.** Every unknown that could
change the lease contract was in #0002 (certificates through the start-on-connect path, the
clients' handling of a certificate alias, sshd's trust rules), while #0001 is a CLI that writes
JSON. #0002 was broken down first, into E2.01 to E2.08 (#0018 to #0025), with its two risk-first
spikes at the front, and #0001 waits until #0002 has settled what the contract has to carry.

**Every MVP epic is broken down (Terry, 2026-11-17),** so that the swimcoach `MVP` milestone counts
real stories rather than epic placeholders. The architect laid them out the same day:
- **#0001 into E1.01 to E1.05 (#0035 to #0039):** the contract and schemas first, then the CLI
  skeleton beside the EC2 driver, then the commands.
- **#0003 into E3.01 to E3.03.** E3.01 (#0031) and E3.02 (#0026) were defects that MUST be fixed
  before a real project uses a lease: a lockout from refused logins, and a token push that hangs
  inside a ProxyCommand. Each became a story of #0003 under its own card number, rather than a
  duplicate. E3.03 (#0040) is the real project's run.
- **Defects #0027, #0028 and #0034 went to Backlog as post-MVP.** None blocks the happy path (D19).
  #0034's cause, an expired `aws login`, has a documented fix in the README.

**The execution plan by turn, across all three epics.** Stories in one turn run in parallel and in
isolation (D14); turns run in series (D16).

| Turn | Stories |
|---|---|
| T1 | E2.01, E2.03 |
| T2 | E2.02, E2.04, E2.05, E2.09 |
| T3 | E2.06 |
| T4 | E2.07 |
| T5 | E2.08, E1.01 |
| T6 | E1.02, E1.03, E3.01 |
| T7 | E1.04, E1.05, E3.02 |
| T8 | E3.03 |

E2.08, E3.01 and E3.02 each test logins on the one box, so no two of them share a turn. Until
E3.01 landed, no story was allowed more than one refused login within two minutes, since E2.08
found that two could shut every login out. **E3.01 landed on 2026-11-17 (PR #12), and that limit
is lifted:** 127.0.0.1 is exempt from sshd's penalties and a refused login is dropped after 30
seconds (D7's E3.01 finding). Refused logins still MUST NOT be retried in a tight loop: about ten
within 30 seconds would fill `MaxStartups`.

### D15. Staffing: one implementer and one reviewer per story, models by blast radius

**Status: Decided; for release stories its one implementer and one reviewer were replaced by D31.**

**Decided, Terry, 2026-11-16.** Models are named `Model/effort`, with no version, meaning the
latest.

- **Each story gets one implementer subagent, then one independent reviewer subagent.** A story
  of under 3 hours MUST NOT be split across implementers: the merge cost outweighs the time saved.
  **For release stories this was replaced by D31 (2026-11-17)**, which plans parallel subagents
  inside each story. The model-by-blast-radius rule below stands.
- **The model follows what a mistake would break, not how hard the story looks:**
  - **Fable/high** reviews only, and only where a miss is an auth bypass or a lockout.
  - **Opus/high** implements anything ambiguous or risky, and reviews anything touching keys,
    sshd or Terry's `~/.ssh/config`.
  - **Sonnet/medium** does well-specified, mechanical work.
- **The MVP's staffing** (#0002 on 2026-11-16, #0001 and #0003 on 2026-11-17):

| Turn | Story | Implementer | Reviewer |
|---|---|---|---|
| T1 | E2.01 Spike: cert login | Opus/high | Sonnet/medium |
| T1 | E2.03 CA key | Opus/high | Fable/high |
| T2 | E2.02 Spike: clients | Sonnet/medium | Sonnet/medium |
| T2 | E2.04 Box trusts the CA | Opus/high | Fable/high |
| T2 | E2.05 Issue key and certificate | Sonnet/medium | Opus/high |
| T3 | E2.06 Host aliases | Sonnet/medium | Opus/high |
| T4 | E2.07 Renew and release | Sonnet/medium | Sonnet/medium |
| T2 | E2.09 Laptop state out of AppData | Opus/high | Opus/high |
| T5 | E2.08 Prove on the box | Opus/medium | Opus/high |
| T5 | E1.01 Contract and schemas | Opus/high | Opus/high |
| T6 | E1.02 CLI skeleton and state file | Opus/high | Sonnet/medium |
| T6 | E1.03 EC2 driver boundary | Sonnet/medium | Opus/high |
| T6 | E3.01 Refused logins never lock out | Sonnet/medium | Fable/high |
| T7 | E1.04 create and get | Sonnet/medium | Opus/high |
| T7 | E1.05 renew, release and list | Sonnet/medium | Opus/high |
| T7 | E3.02 Token push in a ProxyCommand | Opus/high | Opus/high |
| T8 | E3.03 Swimcoach on a lease | Opus/high | Sonnet/medium |

The reasoning per row: E1.01 is the contract a clean-room rewrite builds from, so a mistake there
is the costliest to undo, and both roles are Opus/high. E1.03 to E1.05 write or delete keys and
aliases, so their reviewers are Opus/high. E3.01 changes sshd, where a miss is a lockout, so
Fable/high reviews it. E3.02 handles the Bot token. E3.03 is a live run whose output is findings.

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
  stories' pull requests is merged, and only then does the next turn's dev lead start. T2 to T8
  run this way, one after another, across all three MVP epics (D14).

### D18. The lease CLI is PowerShell 7

**Status: Replaced by D28.**

**MVP only. Replaced by D28 (Terry, 2026-11-17):** agentspace is Rust by v0.06, and PowerShell was
the lean prototype's alone. Originally **decided, architect, 2026-11-16; settles Q5's language
half.** The lease code (`agentspace lease`,
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
TTL as a parameter. D24 settled it in story E1.01.

### D19. The MVP's bar is the happy path

**Status: Decided; its happy-path bar for stories and reviews is MVP only, and D34 carries the hardening it deferred into v0.04's specs.**

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
- **The handoff is built before the rewrite starts**, as its own work (card #0030, now release
  v0.04, ahead of the Rust rewrite in v0.05 and v0.06; D28). In the MVP,
  `agentspace lease` gets its JSON schemas with epic #0001, since they cost little while the
  contract is being written. It SHOULD also have `--help` at every level.

### D21. Laptop state lives in %USERPROFILE%\.agentspace, never in AppData

**Decided, architect, 2026-11-17 (found during E2.02).** Everything agentspace keeps on the laptop
(the CA, lease keys and certificates, the localswim token hash) MUST live under
`%USERPROFILE%\.agentspace`. It MUST NOT live under `%LOCALAPPDATA%` or `%APPDATA%`. Story E2.09
moves it.
- **The instance-ID cache is laptop state too, so the rewrite MUST keep it under
  `%USERPROFILE%\.agentspace`** (architect, 2026-11-18, settling docs/operator.md's Q7 and
  docs/state.md's Q1). The prototype keeps it as `.instance-id` in the checkout, which this rule
  lists as a requirement the prototype does not meet. It is only a cache, rebuilt from the stack's
  output when missing, so a rewrite needs no migration from the checkout's copy. Build output
  (`.build\`) is not state, and MAY stay in the checkout.
- **`lease create` MUST NOT depend on the cache** (architect, 2026-11-18, from the clean-room
  review, #0080). The cache is the ProxyCommand's to read at connect time (D9), so a missing or
  stale cache fails the connection, never the create. The prototype's check at create is a
  difference the spec drops.

**Why:**
- **The Claude desktop app is MSIX-packaged, and Windows redirects every write that a packaged
  app's processes make under AppData into the package's own cache**
  (`%LOCALAPPDATA%\Packages\Claude_<id>\LocalCache\Local\...`).
  - Every Claude Code session runs inside the app: the architect, the dev leads and their
    subagents. So when E2.03 "created the CA in `%LOCALAPPDATA%\agentspace\ca`", Windows put it
    in that cache, and E2.05 put its lease keys there too.
  - Inside the app, reads see the cache at the normal path, so every check passed.
  - Outside the app (Terry's own terminal, and the app's SSH-connection process), the real
    `%LOCALAPPDATA%\agentspace` did not exist. Terry confirmed it with `dir` on 2026-11-17.
- **What that broke:**
  - A lease alias's key paths resolve to nothing for any ssh started outside the app.
  - The token hash is invisible outside the app, so every connection from outside pushed the
    token again and hit #0026.
  - The CA private key lived in a cache that resetting or uninstalling the Claude app would
    delete.
- **The profile root is not redirected:** `~/.ssh` edits made from inside the app landed for
  real.

**How it applies:**
- Any file that a process outside the app must see, or that must outlive the app, MUST NOT be
  written under AppData by code that agents run.
- Evidence gathered inside the app about paths under AppData proves nothing about the world
  outside it. A check that matters MUST also run from outside, for example from Terry's own
  terminal.
- Moving the CA MUST keep its fingerprint, so hosts keep trusting it, and MUST re-apply D17's
  owner-only lockdown. Once the new copy is verified, the old copy in the package cache MUST be
  deleted, so only one copy of the key exists.

### D22. The architect keeps a 10-minute watchdog on each dev lead

**Decided, Terry, 2026-11-17.** The working rules are in [AGENTS.md](../AGENTS.md), "The
architect's watchdog on a dev lead", and `scripts/watchdog.ps1` keeps the timer.
- **The timer:** every status update from a dev lead resets it, and it fires after 10 minutes
  without one.
- **When it fires:**
  - If the dev lead is busy, mid-turn or waiting on its own subagents, nothing happens.
  - If it is idle, the architect nudges it to finish in one turn or answer `BLOCKED`.
- **Escalation:** two `BLOCKED` answers within 30 minutes end the watchdog and go to Terry.

**Why:** a dev lead that ends a turn without finishing, and without saying why, otherwise sits
idle until someone happens to look.

### D23. The architect is Terry's proxy

**Decided, Terry, 2026-11-17:** "you're my proxy".
- **Dev leads and subagents MUST bring questions meant for Terry to the architect.** The
  architect answers what it can.
- **The architect asks Terry only for what only he can do:** a check outside the Claude app, a
  click in its UI, or a call costly to reverse (D19).
- **A card waiting on Terry still goes to Needs Terry**, so it reaches his phone and pauses the
  swimcoach clock.

**Why:** questions from several sessions at once are noise on Terry's side. Most of them are
answerable from what the architect already knows.

### D24. A lease's TTL is 8 hours by default, at least 1 minute and at most 24 hours

**Status: Decided; its maximum is MVP only, and card #0010 revisits it.**

**Decided for the default and the minimum; MVP only for the maximum. Story E1.01, 2026-11-17;
settles Q5's TTL half.** The contract is [lease-api.md](lease-api.md), "TTL".
- **`create` with no TTL MUST get 8 hours.** `renew` with no TTL MUST reuse the lease's current
  TTL.
- **A TTL MUST be at least 1 minute and at most 24 hours, both inclusive.** A TTL outside that
  range MUST be refused with `out_of_range` (exit 2), and MUST NOT be clamped to the nearest limit.
- **Each certificate is valid from its issue time for the TTL.** `renew` issues a new one from the
  time of the call, so it MAY shorten a lease as well as extend it. Neither `renew` nor `release`
  revokes the previous certificate (D10), which stays valid until its own expiry, so after a
  shortening `renew` it outlives the lease's `expires_at`.
- **A certificate's valid-after SHOULD be back-dated by 5 minutes, for clock skew** (#0029;
  architect, 2026-11-18, for E0.04-T01-S02). A host whose clock runs behind the laptop's then
  accepts a new certificate at once. Its valid-before, and so the lease's `expires_at`, MUST stay
  its issue time plus the TTL, so the back-dating never lets a certificate log in after the TTL.
  The prototype does not back-date, so #0029's point is met in v0.05 (#0054).
- **A lease MAY be renewed any number of times.** There is no cap on a lease's total life; the
  maximum bounds each certificate, not the lease.

**Why:**
- **8 hours by default** is an agent's working session, a day of turns, and is the `--ttl 8h` of
  the roadmap's sketch. A lease that is forgotten loses its credential the same day.
- **24 hours at most, because neither release nor renew revokes (D10).** The maximum, and not a
  lease's `expires_at`, is the longest that any one leaked, released but copied, or renewed-over
  certificate can open a new connection. A day covers an overnight unattended run with no renew
  loop; anything longer renews, which E2.07 made cheap: it signs a certificate on the laptop and
  touches no host.
- **No cap on renewals,** because the exposure argument is about one certificate: a leaked
  certificate expires within the maximum however often its lease was renewed, and whoever holds
  the leaked copy cannot renew it, since renewing needs the CA key (D17).
- **1 minute at least,** so that a check of expiry (E1.04, E1.05) does not wait long. Below that,
  a login through start-on-connect (29 seconds from a stopped box in E2.01) and a few seconds of
  clock skew between laptop and box (E2.05) could use up the whole window.
- **Refused, not clamped (D5's spirit):** a caller that asked for 48 hours and quietly got 24 would
  lose new connections in the middle of its run, with nothing to tell it why.
- **The TTL is independent of D11's idle stop.** A lease stays `ready` while the box is stopped,
  and its next connection starts the box again.

**The maximum is MVP only. Card #0010 revisits it:** once a released certificate can be revoked,
the 24-hour bound protects only against a certificate that leaked from a lease still held, and a
longer maximum MAY then be chosen.

## After the MVP

### D25. One host per project

**Direction, Terry, 2026-11-17.** agentspace scales horizontally: each project gets a host of its
own, and every lease for that project lands on it. Terry's vision: a project asks for agent
capacity with its platform, profile, execution plan and staffing plan, and gets dedicated compute
big enough to run that plan.
- **A project's host is created by its first lease, and kept.** It is stopped when the project
  holds no live lease and started by its next one (v0.08, #0048). Its disk keeps the project's
  checkout, sign-ins and caches.
- **Its size comes from the lease:** a stopped host is resized to the size asked for (v0.09,
  #0049), and a size can be chosen from a plan (v0.10, #0050; D27).
- **Leases of different projects MUST NOT share a host.**
- **D1 stands:** nothing in a lease says which host serves it, or that the host is dedicated.

**Why a persistent host per project rather than a fresh host per lease:**
- **A fresh host starts empty:** no Claude Code, Codex or gh sign-ins, no checkout (D3's E3.03
  finding) and cold caches. Automating sign-ins (#0009, Q2) is the hardest problem left open, and
  this choice keeps it off the path to 1.0: Terry signs in once per project host instead (v0.07,
  #0047).
- **A stopped host costs only its disk**, and EC2 can change a stopped instance's type, which makes
  sizing a lease cheap.
- **Dedicated compute is the point.** A shared box cannot promise a project the CPU and memory its
  plan needs.

**What it changes:**
- **D3's isolation half:** the boundary is the project's host. #0004 (a Unix user per project) and
  #0007 (cgroup size classes) are OBE and archived.
- **D6:** a pooled driver's host serves one project. #0012 (packing leases across a shared pool) is
  OBE and archived.
- **Fresh hosts per lease, if ever wanted, come after 1.0** with #0009 and #0014.

### D26. Releases from v0.01 to v1.0, one epic each

**Decided, Terry, 2026-11-17:** "I would rather do a large number of smaller scope steps between
where we are and 1.0." The MVP is v0.01. Each later release is one epic, titled `vX.YY: title`, and
the releases are taken in order.
- **The minor number has two digits** (Terry, 2026-11-17, by retitling the epics): `v0.02`, not
  `v0.2`. So the releases sort in order, and v0.10 follows v0.09 everywhere.
- **1.0 keeps one digit: `v1.0`, not `v1.00`** (Terry, 2026-11-18). Two digits were needed only
  below 1.0, because the releases run to v0.12 before it (to v0.13 from 2026-11-18 until v0.13 was
  folded into v0.12 on 2026-11-19, below). Epic #0053 keeps its title.
- **The plan was first written with one digit** (v0.2 to v0.12). Docs and cards were rewritten to
  two digits the same day, except the swimcoach milestones `v0.2` and `v0.2-T1`, which keep the
  names they were first reported under (D29).
- **Plan from the board comes before `linux/amd64`** (Terry, 2026-11-19: "I'd like to be feature
  'complete' before switching focus to other lease platforms"). The two swapped numbers and kept
  their epics: #0052 is v0.11, and #0051 is v0.12. Neither depends on the other: reading a plan
  from an epic names no platform, and sizes it as v0.10 does. The platforms now run back to back,
  v0.12 then v0.13, so Windows builds directly on the driver that first serves two platforms.
- **v0.13 is folded into v0.12, and both platforms are built in parallel** (Terry, 2026-11-19:
  "let's double the platforms worked in v0.12"). Windows needs the multi-platform driver that
  `linux/amd64` was to bring, not the amd64 host itself. So v0.12's first turn is shared: the
  driver, the lease state and placement serve more than one platform, a project MAY hold one host
  per platform, and each platform gets its own seam (its image, its instance family and its
  bootstrap) so the two tracks touch different files. After it, the `linux/amd64` track and the
  `windows/amd64` track run as parallel stories in the same turns, and the release ends when the
  longer, Windows, does. #0081 is folded into #0051 and archived, as #0013 was, and v0.13 is not
  used: v1.0 follows v0.12.

| Release | Epic | What |
|---|---|---|
| v0.02 | #0045 | Unattended leases: #0042, #0044, #0028 |
| v0.03 | #0046 | Measure what an agent uses |
| v0.04 | #0030 | Clean-room spec package for the Rust rewrite (D20, D28) |
| v0.05 | #0054 | The lease CLI in Rust |
| v0.06 | #0055 | The operator in Rust; PowerShell retired |
| v0.07 | #0047 | A host per project (D25) |
| v0.08 | #0048 | Host lifecycle and cost guard |
| v0.09 | #0049 | Size classes |
| v0.10 | #0050 | Plan-sized leases; contract v1 (D27) |
| v0.11 | #0052 | Plan from the board |
| v0.12 | #0051 | `linux/amd64` and `windows/amd64` platforms, in parallel (D4; Windows added by Terry, 2026-11-18, as v0.13, #0081, and folded in 2026-11-19) |
| v1.0 | #0053 | A real project runs an epic on a plan-sized host |

- **A release number is not the contract version.** The lease contract stays v0 until a release
  changes an output (lease-api.md, "Versioning"); v0.10 is the first that does.
- **D14's rules on epics and stories stand.** A release epic MUST NOT be broken into stories
  until Terry says so, and its "Done when" is checked by a real run, not only by tests.
- **Like the MVP, 1.0 is done when a real project works through it** (#0053), not when the code
  works.

### D27. A host is sized by concurrency, never by model or effort

**Direction, architect, 2026-11-17; measured in v0.03 (#0046), applied in v0.09 (#0049) and v0.10
(#0050).**
- **A host's size MUST be chosen from the plan's peak concurrency**, meaning the most agents
  working at once in any one turn, times a measured per-agent footprint for the project.
- **A plan MAY carry each subagent's model and effort, and agentspace records them, but they MUST
  NOT change the size.** They MAY feed the TTL, since they predict how long a turn takes.
- **The footprint MUST come from measurement** (v0.03), never from an estimate.

**Why:** the model runs at Anthropic, not on the host. What loads the host is the agents' tool
calls: builds, test suites and browsers, run by subagents working at the same time. **A turn's
peak concurrency is the sum of its stories' staffing-plan peaks (D31), plus 1 for its dev lead.**
v0.10 sizes a host from those plan peaks (D27, D31).
- **Replaced, architect, 2026-11-18, in v0.03-T03:** this said that under D15 a story's
  implementer and reviewer run one after the other, so a turn's peak is about its number of
  stories plus its dev lead. D31 runs a story's subagents in parallel, and the measured turn
  peaked at 8 working agents against that estimate's 4.

**Measured in v0.03 (E0.03-T03-S01, 2026-11-18).** D27 stands. One turn, agentspace's own
v0.04-T01: 1 session, up to 7 subagents at once, on a 4-CPU box. Figures are peak, p95 and p50, and
the whole table is [docs/footprint.md](footprint.md).
- **The session's own process, paid once per session:** 0.14, 0.06 and 0.01 CPU cores, and 538.41,
  484.48 and 436.82 MiB PSS.
- **One working agent's tool calls,** the session's tool calls divided by its working agents (its
  subagents running plus its main agent): 0.11, 0.01 and 0.00 CPU cores, and 46.17, 0.00 and 0.00
  MiB PSS. The PSS figures are a floor, since a call that lives between two samples adds none.
- **The main agent alone,** its tool calls with no subagent running, undivided: 0.08, 0.03 and 0.00
  CPU cores, and 46.17, 0.00 and 0.00 MiB PSS.
- **The peak was 8 agents working at once,** 7 subagents plus the dev lead, which is D27's count.
  It is the sum of the three stories' plan peaks (2, 2 and 3) plus the dev lead, as the "Why"
  above now says. The estimate it replaced gave 4.
- **The turn was spec writing, which is light.** Its host peaked at 0.51 cores and 1.09 GiB.
  **v0.09 MUST measure a v0.05 turn again before it sets a size** (D33, #0049).

### D28. agentspace is Rust by v0.06; PowerShell was the prototype's

**Decided, Terry, 2026-11-17:** cut over to Rust (first choice) or Go ("pretty far back second
place but would accept if much simpler than Rust") by 1.0. PowerShell was permitted only because
the MVP was a very lean prototype. **Replaces D18.**
- **The rewrite is the clean-room drop of D19 and D20.** v0.04 (#0030) writes the specs, and the
  Rust code MUST be built from them, never from the PowerShell code.
- **The lease CLI MUST be Rust from v0.05 (#0054), and the operator (`agentspace.ps1`,
  `ssh-proxy.cmd`) from v0.06 (#0055).** From v0.07 on, new product code MUST be Rust.
- **Not product, so not ported:**
  - The architect's process tools (`scripts/watchdog.ps1`, `scripts/report-milestones.ps1`) MAY
    stay PowerShell.
  - The shell that runs on a host (`box/`) stays shell. It configures Ubuntu, and Rust would add a
    build per platform without making it clearer.
- **Open defects against the PowerShell code** (#0027, #0029, #0034, #0041) MUST be carried into
  v0.04's specs as requirements, and each is OBE once the Rust version meets it.

**Why Rust and not Go:** Go is not much simpler for this work, which is what Terry's second place
required.
- Both have mature libraries for everything agentspace does: SSH keys and certificates, JSON Schema
  validation, the AWS SDK and file locking.
- Both shell out to the Session Manager plugin, as the prototype does.
- Both build single native binaries for Windows and Linux.
- Both toolchains are already on the laptop (cargo 1.99; Go 1.27, which builds localswim).

**Why at v0.04, and not just before 1.0:**
- **Every release from v0.07 on rewrites the driver** (hosts per project, lifecycle, sizes, plans).
  Built in PowerShell first, each would be written twice.
- **The PowerShell to port is the smallest it will ever be**, about 110 KB of product code.
- **v0.02 and v0.03 go first** because they are small, mostly on the box, and fix what blocks real
  use today.

### D29. A release's stories are `EX.YY-Tnn-Smm`, named for their turn

**Decided, Terry, 2026-11-17:** a release epic's stories are titled `EX.YY-Tnn-Smm: title`. "Sooner
or later we will hit double digit turns or stories, so let's plan for it now."
- **`X.YY`** is the release, as in its epic's title (D26).
- **`nn`** is the earliest turn in which the story's entry requirements are met, two digits.
- **`mm`** numbers the stories within turn `nn`, two digits. It is namespaced per turn, so it MUST
  restart at 01 in every turn: `E0.02-T03-S01`, then `E0.03-T01-S01`, `E0.03-T01-S02`,
  `E0.03-T02-S01`.

**This replaces D14's `EN.nn` for release epics.** MVP stories keep their `EN.nn` names.
- **The name is the execution plan.** A story's turn is read from its name, so laying out the turns
  and naming the stories are one step. A story whose entry requirements change MUST be renamed
  with them.
  - **Every turn is still laid out up front, and replanning is expected** (Terry, 2026-11-18).
    Laying out one turn at a time was adopted and walked back the same day. Terry: "it's okay to
    have to replan and adjust when unknowns are hit within a turn. I like having execution and
    staffing plans at the turn level and we can adjust as needed." When a turn's findings change
    a later turn, as v0.04-T02's sorting turned T03 into three new stories and pushed #0080 to
    T04, the architect re-plans those cards and renames them by this rule.
- **Docs, pull requests and conversation MUST cite a release story the same way:** `E0.02-T03-S01`.
- **Its branch is the name in lowercase, under `vX.YY/`**, as in `v0.02/e0.02-t03-s01`.
- **A release's turns are `vX.YY-Tnn`**, as in `v0.02-T03`, so they never collide with the MVP's T1
  to T8. Each turn is a swimcoach milestone of that name, and the release itself is one more,
  `vX.YY`, which counts all of its stories and never the epic.

**How the names got here, all on 2026-11-17, and what keeps an older form.** Terry asked for
`X.Y.nn`, then `X.Y.Tm.n` once the stories had dependencies, then padded the release to `X.YY`,
then settled on `EX.YY-Tnn-Smm` from v0.02's T3 on, with "no need to do any renaming for T2".
- **v0.02's T1 and T2 stories keep `0.02.Tm.n`:** 0.02.T1.1 (#0056), 0.02.T1.2 (#0058),
  0.02.T2.1 (#0057) and 0.02.T2.2 (#0059), on branches `v0.02/0.02.t1.1` and so on. Their turns
  are `v0.02-T1` and `v0.02-T2`.
- **v0.02's T3 story is the first under this rule:** E0.02-T03-S01 (#0060), in turn `v0.02-T03`.
- **The swimcoach milestones `v0.2` and `v0.2-T1` keep the names they were first reported under.**
  Renaming one would leave a stale card on Terry's phone, which only his word can stop.
- **Every other rule in D14 stands:** each story is under 3 hours of active work, it starts in
  Ready For Work as a child of its epic, and it carries its estimate and dependencies.

**The Terry-preset board does not let Bot move a card from Backlog to In Progress** (localswim's
README: that takes a `policy grant`). A release epic leaves Backlog by Terry's hand, so starting
a release is always his click. The architect MUST NOT grant Bot that move on its own.

### D30. A keeper on the laptop holds the board's forwards

**Status: Decided; built in v0.02.**

**Direction, architect, 2026-11-17; built in v0.02 by 0.02.T1.2 and 0.02.T2.2, shown in E0.02-T03-S01.**
Replaces D12's hand-run holder (card #0042).
- **A keeper on the laptop holds the board's forwards for as long as a lease is ready.** Each pass
  reads lease state read-only. With no holder running and a lease ready, it starts one, `ssh.exe
  -N -o ExitOnForwardFailure=yes <alias>`, through the ready lease that expires last.
- **It stops a holder whose lease is no longer ready**, so a holder never keeps a host up past its
  lease. Until v0.08 the on-box idle check counts the holder's session as activity, so the box runs
  while any lease is ready, which is v0.08's rule anyway.
- **It MUST NOT stop a process it did not start.** A PID is trusted only when its start time and
  command line match the keeper's own record.
- **One holder per host.** Only one connection on a host can bind the forwards (D12). Every lease
  is on one box until v0.07, which makes it one per project host.
- **It retries a holder that fails with backoff, never in a tight loop** (D14). A dead session
  keeps the ports until 0.02.T1.1's `ClientAliveInterval` bound drops it.
- **It runs as a logon scheduled task for Terry's user.** That is persistent configuration on his
  laptop, so it is registered only with his yes, in E0.02-T03-S01.

**Why not the ProxyCommand or `lease create`:**
- **A holder started by the ProxyCommand** belongs to the process tree of whichever connection ran
  it, the desktop app's included. It would die with that connection, or outlive it with nobody
  tracking it.
- **`lease create` is often an agent's tool call**, whose process tree is torn down when the call
  ends. It also cannot stop the holder when the lease expires, which no command marks.
- **A keeper that reconciles leases against one holder** handles every one of those cases, and is
  the shape a hosted control plane (#0011) would take anyway.

**Findings from story 0.02.T1.2 (2026-11-17).** D30 stands. `scripts/board-holder.ps1` runs one
pass of the keeper to the card's pinned interface, and `scripts/test-board-holder.ps1` checks it
against a throwaway state folder and a stub for `ssh.exe`
(`scripts/test-board-holder-stub.cmd`), never the box.
- **Only a holder started through CIM, `Win32_Process.Create`, outlives the pass.** The spike found
  that a holder started with `Start-Process` or `[Diagnostics.Process]::Start` dies when the
  starting pwsh's Job Object is torn down, as when an agent's tool call or a task ends. Only one
  started through `Win32_Process.Create` survived.
- **cmd's `>>` opens the log without write sharing.** With a wrapper `cmd /c "... >> log"`, a new
  holder failed at once while any earlier process still held the log. That is why the keeper does
  not use one.
- **The hidden `-Launch` parameter is a self-relaunch shim.** `Win32_Process.Create` cannot set a
  process's standard handles, so it starts the script again with `-Launch <request>`, and that
  launcher calls `CreateProcess` for the holder with:
  - Stdin from `NUL`.
  - Stdout and stderr on one append-only handle to `board-holder-ssh.log`, shared for read, write
    and delete.
  - No window, and breakaway from a job where that is allowed.

  Code outside `scripts/board-holder.ps1` MUST NOT call `-Launch`; it is not part of the pinned
  interface.
- **A launch request is claimed by creating a claim file with `CREATE_NEW`, never by a rename.**
  A trial of four launchers renaming one request to one name had two of them succeed, and start
  two holders, in 2 rounds of 9: the loser's rename of a file already given that name succeeds
  too. With the claim file, the same trial started exactly one holder in 12 rounds of 12.
  - A launcher claims the request before it starts anything, and one that cannot claim it starts
    nothing. So a second launcher, from a `Win32_Process.Create` retry after a call that threw but
    had made one, starts nothing.
  - A pass that gives up waiting, or whose create failed, withdraws the request by claiming it
    itself. If a launcher claimed it first, the pass waits up to 60 more seconds for its answer,
    and records the holder if one comes.
  - The request also carries a deadline, past which a launcher that claims it starts nothing.

  What this guarantees, and no more: a holder the launcher starts is recorded by its pass, unless
  the launcher claimed the request and then gave no answer within those 60 seconds, or the record
  could not be written and the holder could not be stopped. In both cases the pass logs `failed`
  with a reason that says a holder may be running, or runs, unrecorded.
- **A stop that fails keeps the record.** When the stop could not open the holder with the right
  to terminate it (one started elevated, met by a pass that is not, say), or stopped nothing for
  another reason while the recorded holder still runs, the pass logs `failed` and keeps the
  record, so the next pass tries again. `stopped` is logged only when the recorded holder is gone.
- **Exit codes, by the architect's ruling (2026-11-17):** 0 when the pass ran, `failed` included,
  and when it was skipped because another pass held `board-holder.lock` (logged `skipped
  lease=- alias=- pid=- reason=locked`, never `idle`); 1 when lease state could not be read or the
  pass met an error it does not handle, with a `failed` line; 2 for a usage error, such as an
  unknown argument or a `-Now` that is not a time, as for the lease CLI, with nothing logged.
- **The record names the holder process itself:** `ssh.exe` for an exe, or the `cmd.exe` running
  the batch for a `.cmd` stand-in. It never names a wrapper or the launcher.
- **Identity is the PID plus the process start time to the millisecond** (`GetProcessTimes`, UTC),
  which is what defeats PID reuse. The record keeps only the pinned fields: `pid`,
  `process_started_at`, `lease_id` and `alias`.
- **The command line is matched structurally, never byte for byte.** The first build compared it
  exactly with the keeper's own quoting, so it never trusted a holder started any other way, the
  tests' `Start-Process` stand-ins included. The matcher (`HolderCommand.Matches`) now:
  - Splits the line by `CommandLineToArgvW`'s rules, so quoting and spacing do not matter.
  - Compares the program with `-SshExe` as Windows paths, case ignored. For a `.cmd` or `.bat`, it
    also accepts `<System32>\cmd.exe /c`, with or without `/d`, after cmd's own outer quote
    stripping.
  - Requires the arguments after the program to be exactly `-N -o ExitOnForwardFailure=yes
    <alias>`, the alias compared exactly.

  The pass's trust check and the stop's re-check on its open handle MUST use this one matcher. It
  checks itself against the keeper's own form, `Start-Process` of an exe and of a `.cmd`, paths
  with spaces, and near misses at the start of every pass, and a pass whose matcher fails trusts
  nothing.
- **A process that fails either check is never touched.** Its record is dropped, it is left
  running, and a holder is started afresh if a lease is ready.
- **The stop takes down the recorded tree only.** It goes leaves first, and each child is checked
  on its own handle against its parent's PID and creation time. There is no `taskkill` and no
  sweep by command line. So a ProxyCommand child already orphaned by a dead parent is left alone,
  and the dead session is then freed on the box by 0.02.T1.1's `ClientAliveInterval` bound.
- **A pass that stops a holder does not start one in the same pass.** The next pass does.
- **Known follow-ups, not done:**
  - `board-holder-ssh.log` grows without bound.
  - Each pass compiles its C# type, which takes about 1 to 3 seconds.

**Findings from story E0.02-T03-S01 (2026-11-18).** D30 stands, with two gaps. The architect
carded their fixes as v0.02-T04 stories, and this story changes neither the keeper nor its task.
The evidence is in [docs/runs/e0.02-t03-s01.md](runs/e0.02-t03-s01.md). Times are UTC.
- **The logon task was registered with Terry's yes**, from the main checkout, at 02:22:25Z. Every
  setting read back equals `install-board-holder.ps1 -DryRun`. It stays registered.
- **Unattended, the keeper started a holder for each ready lease, kept it, and stopped it at
  release.** Each holder logged in by its lease's certificate and held both ports. Each record
  matched its process to the millisecond. After each release, the next pass logged `stopped`,
  removed the record, and then logged `idle`.
- **A killed holder was replaced, but the keeper's early retries stretched the outage.** The
  recorded holder was checked against its record and killed by PID at 04:29:21Z. The keeper
  started new holders at 04:29:30Z and 04:30:31Z, while the dead session still held the ports, so
  each exited under `ExitOnForwardFailure`.
  - **Each failed holder left a session on the box that also waited out the 90-second bound**,
    timing out at 04:31:08Z and 04:32:09Z.
  - The second attempt's login, at 04:30:39Z, came one second before the dead session timed out at
    04:30:40Z. The backoff
    then put the third attempt at 04:32:02Z, and it took the ports at 04:32:10Z. **The board was
    gone for 2 minutes 49 seconds, against a bound of about 90 seconds.**
  - **The fix to weigh:** after it loses a holder, the keeper SHOULD wait out the bound, or watch
    for the ports to come free, before it starts the next one.
- **Task Scheduler did not restart a crashed keeper.** The keeper's pwsh was checked and killed by
  PID at 04:32:59Z. The launcher exited within 3 seconds with `LastTaskResult` 0xFFFFFFFF, and
  the task went to Ready. Three minutes later it was still Ready, though RestartCount 3 and
  RestartInterval PT1M read back as registered.
  - **In this run Task Scheduler did not restart an action that exited non-zero.** Its
    restart-on-failure most likely covers only a task that fails to start, so a crashed keeper
    SHOULD be taken to stay down until the next logon. This was seen once, and the laptop's
    TaskScheduler/Operational log is disabled, so it holds no events to say more.
  - **The holder outlived its keeper**, since it was started through `Win32_Process.Create`. It
    kept the ports, unsupervised.
  - **The task was started by hand** (`Start-ScheduledTask`, definition unchanged) at 04:36:13Z,
    with the architect's approval. The new keeper's first pass, at 04:36:14Z, logged `kept ...
    pid=<the recorded holder>`. It adopted the holder, with no new holder and no new login.
  - **The fix to weigh:** a restart that does not rely on the action failing to start, such as a
    launcher that restarts the keeper, or a repeating trigger with `MultipleInstances IgnoreNew`.

**Change from story E0.02-T04-S02: the task gets a second trigger that repeats every minute.**
D30 stands. `scripts/install-board-holder.ps1` builds the task with two triggers, and every other
setting is as before.
- **The AtLogOn trigger is kept**, and the principal stays Interactive, current user, Limited. A
  crashed keeper is therefore started again only while Terry is logged on.
- **The second trigger is a `Once` trigger from registration with a repetition interval of PT1M and
  no duration.** A repetition with no duration is indefinite in Task Scheduler. Built on the
  laptop's PowerShell 7 ScheduledTasks module, `New-ScheduledTaskTrigger -Once -RepetitionInterval`
  gives `Interval` PT1M and an empty `Duration`.
- **`MultipleInstances IgnoreNew` makes a firing while the keeper runs a no-op.** One that finds the
  keeper gone starts it within a minute, and its first pass adopts the recorded holder.
- **RestartCount 3 and RestartInterval PT1M stay.** RestartCount did not restart a keeper whose
  action exited non-zero in E0.02-T03-S01, so the repeating trigger, not RestartCount, is what
  brings a crashed keeper back. That RestartCount covers only a task that fails to start is the
  most likely reading of one observation, since the Operational log was disabled, and not a
  finding.
- **The time trigger's start is in UTC.** `New-ScheduledTaskTrigger -Once -At (Get-Date)` writes
  `StartBoundary` in UTC with a `Z`, so a change of the laptop's time zone does not move the
  repetition. A reviewer confirmed that a `TimeTrigger` with a past `StartBoundary` and no
  `Duration` keeps firing across reboot and logon: 22 tasks of that shape are on this laptop,
  Windows' own among them.
- **A re-registration MUST NOT run while a keeper does, and the installer refuses one.** The
  install compiles the launcher to `%USERPROFILE%\.agentspace\BoardHolderLauncher.exe`, which
  fails while the task runs that exe, and stopping the task ends only the launcher (below), so a
  registration then would start a second keeper beside the old one. A real install therefore exits
  2 while the task is Running or a pwsh runs `<repo>\scripts\board-holder.ps1 -Watch`, and names
  the keeper's PID. It stops and kills nothing itself. Before a re-registration the task MUST be
  ended and the keeper's pwsh stopped by its checked PID; the holder keeps running, and the keeper
  the install starts adopts it.
- **End in Task Scheduler stops only the launcher.** Found in E0.02-T04-S03's live check
  (2026-11-17, about 9:29pm Pacific): `End-ScheduledTask` on the task killed
  `BoardHolderLauncher.exe` but left the keeper's pwsh running, and the holder survived too. The
  next firing of the repeating trigger therefore starts a second keeper beside the first. Card
  #0067 (E0.02-T05-S01, "One keeper at a time") covers that two-keeper risk, and this story does not fix it.
  To stop the keeper, the task MUST be ended and then the keeper's pwsh stopped by its checked
  PID; to keep it stopped, the task MUST also be disabled or removed with `-Uninstall`.
  `-Uninstall` ends the task with `Stop-ScheduledTask` before unregistering it, so it most likely
  leaves the keeper's pwsh running the same way (not yet observed), and that pwsh MUST still be
  stopped by its checked PID.
- **The read-back shows both triggers and the repetition**, in `-DryRun` and after a real
  registration: `Trigger 1`, `Trigger 2` (with its `StartBoundary` and `EndBoundary`), and
  `Repetition 2: Interval=PT1M Duration=indefinite`.
- **Live run (E0.02-T04-S02, 2026-11-17, Pacific time): passed**, with a holder running for a
  throwaway lease. The installer ran from an untracked copy in the main checkout, so the task
  points there as it will after the merge, and the copy was deleted at once.
  - **Refusal:** at 9:44pm an install while the keeper ran exited 2 and left the task's exported
    XML unchanged. It named no keeper PID, because the launcher quotes each argument
    (`board-holder.ps1" "-Watch"`) and the match then required whitespace before `-Watch`; it
    refused only because the task was Running. With the task ended and an orphaned keeper alive,
    it would not have refused. `Test-KeeperCommandLine` now matches after a quote too, and a test
    checks the quoted form.
  - **Re-registration:** the task was ended, which killed only the launcher, and the keeper's pwsh
    was stopped by its checked PID; the holder survived. The install at 9:44:48pm exited 0 and read
    back `Trigger 1: AtLogOn`, `Trigger 2: Once ... EndBoundary=none` and
    `Repetition 2: Interval=PT1M Duration=indefinite`. The new keeper's first pass logged
    `kept ... pid=<holder>`.
  - **Crash:** the keeper's pwsh was killed by its checked PID at 9:45:13pm. Task Scheduler ran the
    task again at 9:45:50pm, 37 seconds later, and the new keeper's first pass logged
    `kept ... pid=<holder>`: no new holder, and no new login on the box.
  - **One keeper:** 22 samples 10 seconds apart over 224 seconds, about 4 firings, each showed
    exactly one launcher and one keeper pwsh, with the same PIDs throughout. An ignored firing
    sets LastTaskResult to 0x800710E0, which is IgnoreNew at work and not a failure.

**Findings from story E0.02-T04-S03 (2026-11-17).** D30 stands, with the first gap closed: the
keeper waits out a dead holder's release. `scripts/board-holder.ps1 -Watch` takes
`-ReleaseGraceSeconds <n>`, a whole number from 1 to 86400, 95 by default: 0.02.T1.1's
`ClientAliveInterval` 15 times `ClientAliveCountMax` 5, plus 20 seconds for the probe already in
flight.
- **The evidence was E0.02-T03-S01's.** The holder was killed at 8:29:21pm Pacific. The keeper
  started new holders at 8:29:30pm and 8:30:31pm while sshd still held the old forwards, and sshd
  released them at 8:30:40pm, 79.5 seconds after the kill. The third holder took the ports at
  8:32:10pm, 2 minutes 49 seconds with no board, where about 90 seconds would do. Each failed
  holder had also left a session of its own for sshd to time out.
- **The rule.** A tracked holder is one the keeper started, or adopted when a pass logged `kept`
  for a holder it had not started. When a look finds it dead, the keeper MUST NOT start a holder
  until `-ReleaseGraceSeconds` has passed, counted from that look. Each wait logs `<time> grace
  seconds=<n>`, with the seconds actually waited.
- **When the grace does not apply.** A keeper that has had no holder, such as one that starts with
  no holder recorded, starts its first holder at once. **The card was ambiguous about a
  holder the keeper stopped because its lease ended, and the architect ruled it on 2026-11-17:**
  every holder that follows a holder the keeper started or adopted waits the release grace from
  that holder's end, whether it died or the keeper stopped it. The stop is `TerminateProcess`,
  which leaves the box's session holding the shared forwards (D12), so a holder started at once
  for the next lease would fail on ExitOnForwardFailure. "Starts at once" covers only a keeper
  that has had no holder. After a stop the next wait is the grace, logged `grace seconds=<n>`,
  with the `-IntervalSeconds` floor; if nothing else is ready, the only cost is a later idle pass.
  **A holder the keeper stops because its record could not be written gets no grace:** the stop
  runs milliseconds after launch, before ssh can have connected and bound the forwards, so
  nothing is held on the box. The grace keeps no state after its wait.
- **With the backoff.** The backoff for fast failures is unchanged. When both apply, the keeper
  waits the longer of the two, never their sum, and logs one line: `backoff seconds=<n>` when the
  backoff is at least the grace, else `grace seconds=<n>`. An adopted holder counts as proven,
  because its age is unknown, so its exit gets the grace and is not a fast failure.
- **Bounds.** Every wait is at least `-IntervalSeconds`, as before (D14). One exit gives one
  bounded sleep, and the pass after it starts a holder, so the grace cannot keep the board down
  for good. No wait is longer than the greatest of the grace, `-BackoffMaxSeconds` and
  `-IntervalSeconds`: the grace is a floor under the backoff, not an addition to it. A grace set
  above `-BackoffMaxSeconds` therefore waits the grace.
- **Live run (E0.02-T04-S03), passed.** A keeper adopted the running holder, and the holder was
  killed by its PID after the PID was checked against its record, at 9:29:59pm Pacific. The keeper
  logged `grace seconds=95` at 9:30:14pm. The box's old session timed out at 9:31:22pm, 83 seconds
  after the kill. The new holder started at 9:31:49pm, 110 seconds after the kill, and took both
  ports on its first attempt (`sudo ss -ltnp` at 9:32:21pm), with no failed holder between. The
  board was down 1 minute 56 seconds, against 2 minutes 49 seconds before the grace.
  - **End in Task Scheduler left the keeper's pwsh running** (it stopped only the launcher), so
    that keeper was stopped by its checked PID. Card #0067 (E0.02-T05-S01) fixes it.

**Findings from story E0.02-T05-S01 (2026-11-17).** D30 stands, with the two-keeper gap closed.
`scripts/board-holder.ps1 -Watch` holds `board-holder-keeper.lock` in the state folder for its
whole life, so one keeper runs at a time.
- **The rule.** A second `-Watch` that finds the lock held tries once, logs `<time> another keeper
  is running pid=<n>` (`pid=-` when the file cannot be read), and exits 0 without a pass or a
  wait. An error other than a sharing violation when taking the lock logs a `failed` line and exits
  1, so a keeper never loops without the lock and the task's repeating trigger tries again in a
  minute. A one-pass run does not use this lock and keeps `board-holder.lock` as before.
- **The lock is the OS's.** It is a `FileStream` opened with `FileShare.Read` and kept open until
  the process ends, so a keeper that is killed releases it. The PID written in the file is for the
  log line only; the share mode is the lock, so the PID can be stale when something other than a
  keeper holds the file (a scanner, for a moment), and the next firing tries again. The holder
  does not inherit it: the handle is not inheritable, and the `-Launch` launcher is created
  through `Win32_Process.Create`, outside the keeper's handle table and job, and passes the holder
  only its two log handles.
- **An orphaned keeper logs once a minute.** After End, until the orphan is stopped by its PID,
  each firing's keeper logs one `another keeper is running` line, about 90 KB of
  `board-holder.log` a day. That is accepted: End is a step Terry or an agent takes on purpose, and
  the README says to stop the keeper's pwsh after it.
- **Left out: the card's optional item.** `BoardHolderLauncher.exe` does not put pwsh in a
  kill-on-close job, so End in Task Scheduler still leaves the keeper running. Rebuilding the
  launcher exe needs a re-install of the task, and the lifetime lock already closes the gap that
  mattered: two loops, each with its own backoff, halved D14's spacing between tries. An orphaned
  keeper after End now keeps the board held, which is the safe side.

### D31. Each story's card names every subagent, run in parallel where reasonable

**Decided, Terry, 2026-11-17:** "Identify as much parallel subagent work as reasonable per story.
Each story should have each subagent model, effort level, and its tasking called out in the
staffing plan for the dev lead to fire up its subagents." The stories in one turn are already
independent and run in parallel (D14); this is parallel work inside one story. **Replaces D15's
one implementer and one reviewer per story, for release stories.** D15's rule that a model follows
the blast radius stands.
- **A story's card MUST carry its staffing plan.** It names every subagent with its `Model/effort`,
  its tasking, the files it owns, and when it starts: at launch, or after which other subagents.
  The dev lead launches subagents from it as written, from the story's worktree (AGENTS.md).
- **Subagents that run at the same time MUST own disjoint files.** Where their work meets, the
  card pins the interface before launch, so that neither waits on the other. The architect pins it
  when laying out the stories.
- **The shapes used, and why each is safe to run in parallel:**
  - **A researcher or spike** writes no repo files: it reads documentation, or experiments in a
    scratch folder. It feeds the implementer through the dev lead.
  - **A test author** writes the tests from the card and the pinned interface alone, at the same
    time as the implementer, and MUST NOT read the implementer's code until both are done. The
    tests then check the spec, not the code (D20's spirit).
  - **Two implementers** on disjoint files that meet only at a pinned interface.
  - **A verifier** runs the live done-when at the same time as the reviewer reads the diff.
  - **An observer** records evidence during a live run, over one connection, and writes no repo
    files.
- **Every story still ends with at least one independent reviewer**, who starts only when what it
  reviews is done.
- **Each plan states its peak:** the most subagents running at once. v0.03 (#0046) measures what
  concurrent agents cost a host, and D27 sizes hosts from it.
- **Parallelism that costs more than it saves is not planned.** A docs edit is folded into an
  implementer's tasking rather than given a subagent of its own.
- **Extended, Terry, 2026-11-21 (D63):** each story uses as many subagents as can be used
  effectively, with section drafters and one integrator where several hands write one file.

**v0.02's plans** (#0056 to #0060) peak at 3, 2, 3, 2 and 2 subagents. With two stories per turn,
T1 and T2 peak at 6 and 5 subagents at once, plus the dev lead.

### D32. A lease's box runs in the laptop's time zone

**Decided, Terry, 2026-11-17:** agents on the box are to report times as he reads them, and "as
long as they know I like H:MM[am/pm] like 1:34am or 12:56pm and the current time is 3:52pm -0700,
they can compute deltas from there." **Found in E0.02-T03-S01:** an agent in a desktop-app session
on the box wrote a board comment's time in UTC. Terry's global rule reads the zone from the system,
and on the box the system is UTC, so the rule was followed and still gave him the wrong zone.
- **The token push MUST also set the box's time zone to the laptop's.** The laptop converts its
  Windows zone to an IANA name (`TimeZoneInfo.TryConvertWindowsIdToIanaId`, for example Pacific
  Standard Time to `America/Los_Angeles`), and the box sets it with `timedatectl set-timezone`.
  `date` on the box then gives Terry's local time and UTC offset, and his global rule holds there
  unchanged. The box's copy of his `CLAUDE.md` MUST NOT be edited for it (README, "Rules").
- **The zone MUST be part of the push stamp**, so a laptop that has changed zone pushes again on
  its next connection. Terry travels, and the laptop's zone travels with him.
- **The zone MUST NOT put the token at risk.** A zone the laptop cannot convert, or that the box
  refuses, leaves the box's zone as it was and says so, and the token's push and its confirmation
  go ahead as before.
- **The system zone, not an exported `TZ`.** Ubuntu's `.bashrc` returns early in a non-interactive
  shell, so a session the desktop app starts might never read a `TZ` set there.
- **What it does not change.** Machine-readable timestamps keep their own form. journald stores UTC
  and only shows it in the system zone. The idle timer counts from its last run
  (`OnUnitActiveSec`), so a zone change never moves it. Evidence recorded in this file before D32
  stays in the UTC it was recorded in.
- **Carded as E0.02-T04-S01, v0.02's fourth turn**, which starts after T3 merges, because it changes
  the push that T3 is exercising live.

**How it is built (E0.02-T04-S01, 2026-11-17).** `Push-Localswim` in `scripts/agentspace.ps1`
carries the zone with the token:
- **Windows PowerShell 5.1 has no `TryConvertWindowsIdToIanaId`.** The method is .NET 6's, and 5.1
  runs on .NET Framework 4.8, which lacks it, while `up` runs in 5.1 under the ProxyCommand. So
  the push MUST use the method when the runtime has it, as PowerShell 7 does, and otherwise MUST
  ask the ICU that Windows ships (`icu.dll`) through `ucal_getTimeZoneIDForWindowsID`, with no
  region. That is the call .NET itself makes on Windows for the conversion, and both gave the same
  IANA name for every zone tried on the laptop. Any failure in either path sends no zone.
- **Windows PowerShell 5.1 compiles the ICU call with `Add-Type` on each `up`**, about 0.2 seconds,
  even when nothing is pushed, because the zone is worked out before the stamp is compared. This
  was accepted: it is small beside the connection, and a cached assembly would be one more file
  to keep in step with the script.
- **Seven legacy names MUST be mapped to their current IANA names on the laptop**, after the
  conversion and before the zone is checked. Both paths give CLDR's names, and for some Windows
  zones CLDR's name is a legacy one. The box runs Ubuntu with `tzdata` and without
  `tzdata-legacy`, so `timedatectl` refuses those names. Of the 139 names this laptop's Windows
  zones convert to, exactly these 7 were missing on the box when checked on 2026-11-17:

  | Legacy name | Current name |
  |---|---|
  | `Asia/Calcutta` | `Asia/Kolkata` |
  | `Asia/Katmandu` | `Asia/Kathmandu` |
  | `Asia/Rangoon` | `Asia/Yangon` |
  | `Europe/Kiev` | `Europe/Kyiv` |
  | `America/Buenos_Aires` | `America/Argentina/Buenos_Aires` |
  | `America/Godthab` | `America/Nuuk` |
  | `America/Indianapolis` | `America/Indiana/Indianapolis` |

  `ConvertTo-CurrentZone` holds the map, and `Get-LaptopZone` applies it to the converted zone
  only; `-TimeZone` is sent as given. A name that is still missing on the box, such as one a
  later `tzdata` drops, answers `zone-failed`, warns, and is pushed again on each connection.
  That repeated push is the cost of a future gap, and the fix is to add the name to the map.
- **A zone MUST match `^[A-Za-z0-9_+][A-Za-z0-9_+-]*(/[A-Za-z0-9_+-]+)*\z`, case-sensitively
  (`-cmatch`).** It is checked before the remote command is built, and only a zone that passes is
  ever put in it. **This departs from the card's pinned regex**, `^[A-Za-z0-9_+-]+(/[A-Za-z0-9_+-]+)*$`,
  in two ways, each a refusal the pinned one lacks:
  - **The first character MUST NOT be `-`**, because `timedatectl` would read the zone as an
    option, such as `-H <host>`, just as `Test-AliasName` refuses one for ssh. No IANA name starts
    with `-`. The first build refused it with a separate check; the review folded it into the
    regex, so one test decides.
  - **`\z` replaces `$`**, which also matches before a trailing newline.

  It MUST be applied with `-cmatch`: PowerShell's `-match` ignores case, and then `[A-Za-z]` also
  matches the Kelvin sign (U+212A).

  `-TimeZone` overrides the laptop's zone, for tests only.
- **The zone travels on the box's command line, and the token MUST stay on stdin.** A checked zone
  holds no character that the shell treats as syntax, and no quote, so it cannot break the remote
  command's quoting on Windows either.
- **The box sets the zone only after `bot.env` is in place and its SHA-256 printed**, as
  `timeout -k 1 5 sudo -n timedatectl set-timezone <zone>`, with stdin, stdout and stderr on
  `/dev/null`. It then prints `zone <zone>` or `zone-failed` and exits 0, so the zone never changes
  the exit code or the hash line that confirm the token. `timeout` keeps a slow `timedatectl` well
  inside the push's 20-second limit, `-k 1` kills it a second later if it ignores SIGTERM, and
  `sudo -n` never prompts.
- **The stamp is the SHA-256 of the instance ID, the zone and `bot.env`'s content, joined by LFs**,
  with an empty zone when none is sent. It is written only when the token is confirmed and, if a
  zone was sent, the box's `zone <zone>` line matches it exactly. `zone-failed`, or no zone line at
  all, warns and writes no stamp, so the next connection asks again.

**Findings from story E0.02-T04-S01 (2026-11-17).** D32 stands. One throwaway lease, with the
laptop on Pacific time. Times are the laptop's, Pacific Standard Time, with UTC beside them.
- **The push set the box's zone, over the lease's certificate, and left the token as it was.** At
  9:07pm (05:07:47Z on 2026-11-18) the push's nested login was `ED25519-CERT ... ID <lease>`, and
  `timedatectl show -p Timezone` went from `Etc/UTC` to `America/Los_Angeles`. `date` then printed
  `Tue Nov 17 21:07:49 PST 2026`. `bot.env` was rewritten with its size, mode and SHA-256
  unchanged, and the laptop's stamp equalled the SHA-256 of the instance ID, the zone and the
  content, in the new form. A second connection, at 9:08pm (05:08:15Z), did not push: there was
  no nested login, and neither `bot.env` nor the stamp changed.
- **The keeper's holder pushed first, before the verifier's step.** The lease alias's
  ProxyCommand ran the branch's code, so the holder's connection at 9:07pm (05:07:00Z) already
  pushed the zone. The verifier set the box back to `Etc/UTC` on the box alone, and set the stamp
  aside, before its own connection, so that the timed push had to make the change it saw.
- **The agent on the box answered "It's 9:08pm Pacific."** That is the form Terry asked for: the
  laptop's zone, in `h:mm` plus `am` or `pm`. The box's `date` carries the numeric UTC offset for
  any agent that needs to compute deltas.
- **Until this story merges, two code versions re-push on alternate connections.** A connection
  running `main`'s code writes the old stamp form, without the zone, and one running this branch
  writes the new form, so each sees the other's stamp as stale and pushes. It is harmless, since
  each push writes the same token, and it ends at the merge.
- **`claude` on the box is `~/.local/bin/claude`, which is not on a non-interactive shell's
  `PATH`.** A script that runs `claude -p` over a lease MUST call it by that path, or set `PATH`
  first.

### D33. v0.03 measures each agent session's process tree, split into the session and its tool calls

**Decided, architect, 2026-11-18, when Terry had v0.03 (#0046) broken into stories.** It says how
D27's per-agent footprint is measured.
- **The unit sampled is a Claude Code session on the box: its `claude` process and every process
  under it.** A session's subagents run inside that one process, so their tool calls are all its
  descendants, and nothing on the box tells one subagent's tool call from another's.
- **Each sample MUST record the session's own process apart from its descendants.** The session's
  own process is paid once per session, whatever it is doing. Its descendants, the tool calls, are
  what grows with the number of subagents working at once (D27). The footprint table (v0.03-T03)
  gives a figure per working agent by dividing the descendants' load by the number of the
  session's working agents at that moment: its subagents running, read from the session's
  transcripts, plus its main agent. **Changed by the architect, 2026-11-18, in v0.03-T03:** the
  divisor was the subagents running alone, and gained the main agent because D27's peak
  concurrency counts the dev lead too ([docs/footprint.md](footprint.md)).
- **CPU MUST be counted from cumulative CPU time, reaped children included,** never from an
  instantaneous percentage. A build that starts and ends between two samples is then still
  counted, at the next sample.
- **Memory MUST be recorded as PSS, with RSS beside it.** PSS splits shared pages among the
  processes that share them, so Node's shared libraries are not counted once per process. RSS
  shows the worst case if nothing were shared.
- **Each sample MUST also record the host's totals and the sampler's own CPU**, so the sessions'
  sums can be checked against the host, and the sampler's cost is known.
- **The sampler MUST end by itself**, at a deadline or when a stop file appears, and MUST outlive
  the SSH connection that started it. Its own CPU counts as activity to the idle check, so a
  forgotten sampler would keep the box running.
- **The sampler and its summarizer are Python 3, standard library only, run on the box.** They are
  measurement tools, not product code, like the architect's process tools under D28, so D28's
  Rust rule does not reach them. The box ships `python3`, and `/proc` is what they read. If a
  later release makes sampling part of the product, it is specified then and built in Rust.
- **Their record format is pinned on E0.03-T01-S01's card**, so the sampler and the summarizer
  are built at the same time (D31).
- **The turn measured is agentspace's own: v0.04's first turn with two or more stories in
  parallel** (Terry, 2026-11-18). No other project had stories ready. That turn's dev lead runs as
  a desktop-app session on E0.03-T02-S01's lease, and v0.03-T02 runs beside it.
- **v0.09 MUST measure the footprint again on a v0.05 turn with two or more stories in parallel
  before it sets any size** (Terry, 2026-11-18). v0.04 writes specs and runs few builds, so its
  tool-call peaks understate a turn that compiles. v0.05's `cargo` builds and test suites are that
  load, and v0.03's tools are built to be rerun. The requirement is on #0049.
  - **v0.05-T01's record covers only 7 minutes of a 31-minute turn** (runs/e0.05-t02-s04.md;
    #0109), so the measurement is still owed. **Terry, 2026-11-19, after v0.05 closed: v0.06's
    first turn on the box is measured.** It is sized for its peak (D39), its root disk is grown
    ahead of its builds, and its sampler and disk are watched from start to stop (AGENTS.md,
    "Watching a measurement").
  - **Met for the host and the session by v0.06-T01's whole-turn record** (architect, 2026-11-23,
    D64; runs/e0.06-t02-s08.md). The per-agent division it lacks is owed before v0.10 sets its
    mapping, from that turn's transcripts, which Terry approved (Q12) and which were joined to the record the
    same day (D64).

**Why:** v0.09 and v0.10 size hosts from these numbers (D27), so a number that hides a short build,
or counts a shared library four times, sizes every host wrong.

**Findings from story E0.03-T01-S01 (2026-11-18).** D33 stands. One throwaway lease, 4 CPUs and
about 210 processes. Times are Pacific Standard Time, the box's and the laptop's, with UTC beside.
- **A Claude Code session is one native `claude` process** (2.1.326, a binary under
  `~/.local/share/claude/versions/`), whose argv[0] is the path it was called by. Each tool call is
  a child `bash -c` that sources `~/.claude/shell-snapshots/`, and a subagent adds no process. No
  helper ran under a `-p` session with no MCP servers (1:45am, 09:45Z); an interactive or MCP
  session is unverified.
- **The default `--match` is `^(?:\S*/)?claude(?:\s|$)`, anchored on argv[0]** (widened by
  E0.03-T02-S01's findings below, to take a desktop-app session's `ccd-cli`). A bare `claude`
  matched the tool calls' `bash -c`, the SSH wrapper's shell, `grep claude` and the sampler's own
  `--match`; the anchor keeps all four out. **Known gap:** a launch whose argv[0] is not
  `.../claude`, such as an npm install's `node .../cli.js`, is not matched, and MUST be given its
  own `--match`.
- **Live, from 1:55am to 2:01am (09:55Z to 10:01Z),** the sampler outlived the connection that
  started it, and took 160 samples with no gap. A `claude -p` session's 20-second burn was exactly
  one session, with no wrapper: tools.cpu_s rose 20.02 s and self.cpu_s 0.74 s.
- **Overhead is 0.113% of one core** at the default 2 seconds (sampler_cpu_s rose 0.36 s in 318 s),
  under the card's 1%.
- **The host line checks the sessions.** In the burn the host and the session sum agreed each
  sample (1.01 against 1.00 to 1.01 cores; cpu_total_s grew 3.98 to 4.01 s a second). From 1:59am
  to 2:01am (09:59Z to 10:01Z) the host ran 3.00 to 3.13 cores with no session alive: the
  reviewer's 3-thread experiment, under another match. A session sum below the host's is work
  outside every session, which is what the host line is for.
- **"Every live descendant" (#0069) misses work a session orphans; #0082 is to count it.** A tool
  call's `cmd &`, `nohup` or `setsid` is reparented to PID 1, since `claude` is no subreaper, and
  Docker's work runs under containerd and `systemd-run`'s under systemd. A stub session's `sh -c
  "<8 s burn> & exit 0"` raised the host about 1 core for 8 s while tools.cpu_s stayed 0.0, procs 0.
  The host totals still show it. **v0.03-T02 (#0071) compares host CPU with the session sums plus
  the idle baseline, as a tripwire that can pull #0082 into v0.03 before T03.**
- **Smaller errors, recorded and not fixed.** A child reaped mid-scan can dip tools.cpu_s for one
  sample (not seen in 300 samples at 0.5 s); a matching process whose matching parent dies
  mid-scan can be its own session for one sample; another user's process reads 0 PSS and RSS;
  cpu_busy_s can fall slightly, since iowait is not monotonic.
- **A session whose working directory cannot be read is recorded with cwd "", and
  agentspace-footprint labels it `?`.** The card implies a string, so it is never null.
- **Choices beyond the card.** `sampler.lock` (held while running) and `sampler.json` (the marker)
  are in `~/agentspace-samples/`. A crash writes `<run>.error` and no end line. `start` runs the
  hidden `_sample` command with its settings in the environment, so `--match` is never on the
  sampler's command line, and only the script's own invocations are excluded from sessions.
  cpu_total_s sums /proc/stat's first 8 fields, since guest is already in user. A sample stuck 60 s
  past the deadline is ended by SIGALRM, or by a watchdog thread 15 s later when a /proc read is
  blocked in the kernel.

**Findings from story E0.03-T02-S01 (2026-11-18),** the real turn, v0.04-T01, measured on a
lease; the run record is [docs/runs/e0.03-t02-s01.md](runs/e0.03-t02-s01.md). D33 stands.
- **A desktop-app session is not a `claude` process.** Its argv[0] is
  `~/.claude/remote/ccd-cli/<version>`, under the app's bridge `~/.claude/remote/srv/<hash>/server`.
  The default `--match` missed it, which is the known gap above in a new form. **The default is now
  `^(?:\S*/)?(?:claude|ccd-cli/[^/\s]+)(?:\s|$)`,** which still keeps the bridge out. Before a
  turn's agents launch, a measurer MUST confirm that the session shows as a session row.
- **A turn is one session, and its `cwd` moves:** a dev lead entering each story's worktree moves
  its row's `cwd` with it. A row is identified by its session id, never by its folder.
- **The host-versus-sessions check tripped #0082's 10% line by its letter, and was then broken
  down.** The host ran 19.69 CPU-seconds above the session, the sampler and the idle baseline,
  210% of the session's tools CPU; the 0.25-core line was not crossed. The observer's own
  1-second poll loop took about 7.5 (estimated), and the desktop app's connection (two SSM session
  workers, its bridge and four `sshd` processes) 5.8 to 7.1 (measured from `/proc`). After the
  measurers' logins and the kernel, 1.1 to 4.7 is left. No orphan was seen. The baseline had
  neither the connection nor the observer in it, which is why the check could not tell them from
  orphaned work. **#0082 was not pulled in** (architect, 2026-11-18, in a comment on #0082).
- **A measurer's own polling is load.** A later observer SHOULD sleep to its next snapshot in one
  call.
- **A desktop-app session carries a connection overhead outside its row,** about 0.003 cores and
  up to 161 MiB RSS on this turn. v0.03-T03's table SHOULD give it a line of its own.
- **CloudWatch's idle backstop (#0043) stopped the box at 5:26am** while it waited 3 hours for the
  turn to start, and the sampler with it. Until #0043 is fixed, a measurer SHOULD start the
  sampler close to the turn's start, and MUST run `agentspace-sample status` when the turn starts.

### D34. v0.04's spec package: one reference per interface, and a conformance suite kept apart from the prototype

**Decided, architect, 2026-11-18, when Terry had v0.04 (#0030) broken into stories.** It settles
#0030's design call and says how the package is built and checked.
- **Each interface gets one reference doc, which owns it:**
  - The lease CLI: `docs/lease-api.md` and `schemas/`.
  - The operator (`deploy`, `up`, `sync`, `ca`, `down`, `status`, `grow`, the ProxyCommand and the
    token push, with the box as the laptop sees it): `docs/operator.md`.
  - The laptop's state (every file and folder agentspace writes, the lease's key, certificate and
    alias from creation to removal, and the `Include` line): `docs/state.md`.
  - The driver boundary between the CLI and a backend: `docs/driver.md`.
  - The keeper: `docs/keeper.md`.
  - The vision: `docs/vision.md`. The decisions keep their place here, each MVP-only one marked.
  - Another doc that needs a fact owned elsewhere MUST cite that doc's section rather than restate
    it, so a fact has one home.
- **A spec MUST be understood without the code (D20).** It defines behavior by what can be seen
  from outside: commands, flags, output, files, exit codes, environment variables. It MUST NOT
  define behavior by a function name or a line of the prototype. It MAY name a prototype file in
  a note on where a requirement came from.
- **Code the prototype sends to another machine is prototype code too** (architect, 2026-11-18,
  for E0.04-T01-S03). A remote shell command, a script piped to a host, or a template's embedded
  script is specified by what it MUST do on the far side, step by step, and its text MUST NOT be
  carried into a spec, even though it can be seen in a process list. The rewrite writes its own.
  - **A "template's embedded script" here means a script the prototype builds at run time, never
    the repository's `infra\agentspace.yaml` or `box\system`** (architect, 2026-11-18, for
    E0.04-T06-S01, FA-11). Those are maintained repository files, which the operator carries as
    they are: see D37's addendum.
- **A defect or hardening card against the prototype is carried in as a requirement on the
  behavior,** and the doc names the card it closes (D28). The prototype's defect is never
  specified as behavior to copy.
- **A check the rewrite MAY read and run is a conformance check.** It MUST drive the product only
  as a separate process, through what the specs document, with stubs only for programs outside
  agentspace (`ssh`, `aws`). It names the program under test through one environment variable,
  so the same check runs against the prototype and the rewrite.
- **A check that loads the prototype's code** (dot-sourcing a script, calling its functions, or
  reading its internal files) **is prototype code, and the rewrite MUST NOT read it.** E0.04-T02-S04
  sorts the suites. A check moves across only by being rewritten as the first kind.
- **The package is checked by reviewers who cannot see the code.** They work from an export
  holding only `docs/`, `schemas/` and the conformance checks, never from a checkout. Each writes
  how it would build a drop-in, and lists every point where it had to guess. A guess is a gap in
  the package.
  - **The export MUST leave out what describes the prototype rather than specifies it**
    (architect, 2026-11-18, from E0.04-T02-S02's clean-room reader): every "Findings from ..."
    block in `decisions.md`, `docs/runs/`, the handoffs and `how-it-was-built.md`. A reader who
    falls back on the prototype's history is not reading the spec alone, and the question it
    needed answered is a gap in the spec.
- **v0.04's first turn runs on the box, where v0.03 measures it (D33).** The prototype runs only
  on Windows, so a writer there reads the code and the docs and never runs the prototype. A
  question only a run on the laptop could settle goes in its doc's open questions, for the
  clean-room review to close.

**Why:** the rewrite is a drop-in built from documents alone (D19, D20). A fact stated in two
docs drifts into two answers, and a test that loads the prototype's functions teaches the
rewrite the prototype's structure, which D20 exists to keep out.

### D35. The keeper's contract, settled in the clean-room review

**Decided, architect, 2026-11-18, accepting the rulings E0.04-T05-S04 (#0091) wrote into
keeper.md.** Each settles a gap the review found (docs/runs/e0.04-t04-s01.md); keeper.md and
state.md state them as behavior.
- **The launch request, claim and result files are part of the keeper's contract v0** (A-07,
  B-37, Q5). Every holder start MUST go through them. A check can see them, the claim is what
  gives at most one holder per start, and starting a holder any other way was never run. A later
  decision MAY replace them. Their token is 32 lowercase hex digits (A-08, B-46).
- **A tracked holder that turns out untrusted logs `lost lease=<id> alias=<alias> pid=<pid>`**,
  whether or not a lease is ready, and its release grace applies (#0068, Q6). Another trusted
  holder in the record is adopted with no grace.
- **One process-create call is bounded at 10 seconds**, then counts as a call that threw: three
  tries, 2 seconds apart, 34 seconds at most, inside the 35-second wait (Q15, B-39).
- **Durations are measured on the UTC clock or a monotonic one, never local time.** A wait that
  spans a sleep MAY end on waking or run on, and a check MUST NOT depend on which (Q17, B-40).
- **The holder and its launcher MUST run in the user's logon environment**, never the keeper's
  own (Q4, B-41).
- **The default ssh is `%SystemRoot%\System32\OpenSSH\ssh.exe`**, written with the variable, not
  the prototype's fixed `C:\Windows` path (A-24).

### D36. The operator never reports a false success

**Decided, architect, 2026-11-18, accepting E0.04-T05-S03's (#0090) rulings in operator.md.**
Where the prototype's operator reports success over a step that failed, or does something the
caller asked it not to, the spec requires the honest behavior, and operator.md lists the
prototype under "Requirements the prototype does not meet yet".
- **`deploy -Quiet` passes `-Quiet` to the `sync` it runs** (Q4). `-Quiet` applies to every step
  of a command, and the prototype's quiet `deploy` ran a loud `sync`.
- **`grow` exits 1 when the box's online filesystem grow fails**, saying that the volume is
  already larger and the filesystem grows at the next boot (Q23). The prototype exited 0 with no
  line of its own.
- **`sync` fails before it copies when it cannot remove the box's old staging folder** (Q25). The
  prototype never checked, so a surviving folder could install stale files and still report
  success.

**Why:** a drop-in copies what the prototype does on purpose, never a silent false success. A
caller, or an agent reading the exit code, has to be able to trust that 0 means done.

### D37. The rewrite is seven files in one install folder, and carries what the checkout held

**Decided, architect, 2026-11-18, accepting E0.04-T05-S02's (#0089) proposal.** Settles
operator.md's Q10 and keeper.md's Q3, Q8 and Q10, and the gaps A-02, A-10, B-04, B-21, B-29, B-30,
B-42 and B-43. The facts live in operator.md, "Programs and install layout"; keeper.md, "The
installed task"; and state.md, "Files in the checkout".

- **Seven files, fixed names:** `agentspace.exe` (the lease CLI), an `agentspace` `sh` wrapper for
  Git Bash, `agentspace-operator.exe`, `agentspace-ssh-proxy.exe` (the ProxyCommand),
  `agentspace-keeper.exe`, `agentspace-keeper-task.exe` (the task's launcher, a Windows program
  with no console) and `agentspace-keeper-install.exe`. A build MAY make several one binary that
  selects its role by its own file name. The command callers type stays `agentspace`.
- **One install folder, `%USERPROFILE%\.agentspace\bin`.** It MUST NOT be under AppData (D21) and
  MUST NOT need administrator rights. It SHOULD be on the user's `PATH`, which is persistent
  configuration on Terry's laptop and so takes his yes. An install MUST NOT stop a running program
  to replace it, and SHOULD rename a running program's file aside.
- **Every program finds the others in the folder of its own executable**, never through `PATH`, a
  variable or a checkout. This is D8's note, applied to every program. The lease CLI looks for
  `agentspace-ssh-proxy.exe`, then `agentspace-ssh-proxy.cmd`.
- **Nothing reads a checkout at run time.** The CloudFormation template and `box\system` are carried
  inside the operator program and written out at use under `%USERPROFILE%\.agentspace\operator`.
  The state schemas are carried inside the programs that check against them. `local.parameters`
  moves to `%USERPROFILE%\.agentspace\local.parameters`, and the localswim build to the operator's
  folder.
- **`ca` refuses a CA folder inside any git working tree**, found by a `.git` entry in the folder or
  any folder above it, in place of "inside the checkout". It is wider than the prototype's refusal,
  and keeps the CA's key out of every repository.
- **Every command line keeps its style (B-04):** `--name` for the lease CLI; single-dash names,
  matched without regard to case, for the operator, the keeper, the installer and the launcher.
- **Help, usage lines and messages print the command name** (`agentspace-operator`,
  `agentspace-keeper`, `agentspace-keeper-install`); the ProxyCommand's refusal prints its file
  name, `agentspace-ssh-proxy.exe`. A check MUST NOT depend on the name printed.
- **The keeper's task runs `<install>\agentspace-keeper-task.exe -Watch` in `%USERPROFILE%`**, and
  the launcher runs the keeper beside it. Whether End also ends the keeper is not required.
- **The installer runs only from the install folder**, refuses `-DryRun -Uninstall` and every other
  usage error with exit 2, finds a running keeper by the task's state or `board-holder-keeper.lock`,
  never deletes a program of the install, and removes the prototype's `BoardHolderLauncher.exe`.
- **Until v0.06 (#0055) retires them, the checkout's entry points keep running the prototype**, and
  the `bin\` shims MUST NOT be pointed at an install. v0.05's install carries a forwarder,
  `agentspace-ssh-proxy.cmd`, to the checkout's `scripts\ssh-proxy.cmd`; v0.06 points it at the
  `.exe`. A ProxyCommand line's program MUST keep working while the user's `agentspace` entry or a
  `ready` lease's alias file names it, so `scripts\ssh-proxy.cmd` SHOULD become a forwarder to the
  installed program at v0.06 rather than be deleted.

**Why:**
- **Separate files per role keep every spec's command line, and the conformance suite, unchanged.**
  The four command lines collide in one program, and a file per role needs no wrapper. The lease
  contract's top level stays `lease` and `--help`.
- **`%USERPROFILE%\.agentspace\bin` is the one place that is neither redirected for the Claude app
  (D21) nor protected (Program Files)**, so an install made by an agent is the one Terry's terminal,
  ssh and Task Scheduler see.
- **Carried, not installed beside:** the template and the box's files are the far half of the
  operator's contract with the box. Carried in the operator, they always match it, and an install
  of whole programs cannot be half-copied.
- **The keeper stays a console program**, because a person and the checks run it from a shell. So
  the task still needs a launcher with no console.
- **The keeper lock finds a running keeper whoever started it**, the prototype's included.
- **ProxyCommand lines outlive the program that wrote them:** alias files are never rewritten, and
  the user's entry is the user's. Forwarders keep them working across v0.05 and v0.06 with no edit
  to any file agentspace does not own.

**Addendum: what the operator carries is the repository's own (architect, 2026-11-18, for
E0.04-T06-S01, FA-11).** The final clean-room pass asked whether the rewrite writes its own
template and box files (D34) or carries the repository's (this decision). D34 decides who writes
what the prototype builds and sends; this decision decides where the carried files live at run
time. They do not conflict:
- **The template and box files a v0.05 or v0.06 operator carries MUST be the repository's
  maintained `infra\agentspace.yaml` and `box\system`, embedded in the program at build time.**
  They are not the PowerShell prototype: they stay YAML and shell (D28, "Not product, so not
  ported"), and they change only through the repository.
- **What the prototype builds and sends at run time MUST be the rewrite's own,** written from
  operator.md's steps on the box (D34): every ssh remote command line, the patch question and the
  token push's remote script. Their text MUST NOT be copied from the prototype.
- **A first ruling the same day, that the rewrite writes its own template and box files, was
  withdrawn** before it was written down: it contradicted D28, and the package specifies the box
  only as the laptop sees it, so it would have left the box unbuildable from the specs.

### D38. v0.05 builds the lease CLI as a Cargo workspace of one crate per area, and cuts over by install

**Decided, architect, 2026-11-19, when Terry had v0.05 (#0054) broken into stories.** It sets how
the Rust lease CLI is laid out and built, and what "replaces the PowerShell one" means while the
prototype's operator and keeper still run (D37).

- **One Cargo workspace at the repository's root**, `Cargo.toml` with `members = ["crates/*"]`,
  edition 2024, `rust-toolchain.toml` pinning the toolchain the laptop has (1.99), and
  `Cargo.lock` committed. The architect commits that root before v0.05-T01 starts, and nothing
  else of the code.
- **One crate per area, so the stories of a turn own disjoint folders:**

  | Crate | What | Built in |
  |---|---|---|
  | `agentspace-cli` | The program, `agentspace.exe`: the command line, help, errors and output; then the five commands | T01, T03 |
  | `agentspace-values` | Every value's accepted forms, normalization and messages ([lease-api.md, Values](lease-api.md#values)) | T01 |
  | `agentspace-state` | The lease state file: its model, schema checks and bytes; then the home folder, the lock and the atomic replace | T01, T02 |
  | `agentspace-credential` | The lease's key and certificate, and the CA's checks; then their files and access rules | T01, T02 |
  | `agentspace-alias` | The alias file's bytes, the `Include` line and the ProxyCommand line; then their files | T01, T02 |
  | `xtask` | `cargo xtask dist`: the build output folder ([operator.md, The install folder](operator.md#the-install-folder)) | T02 |
  | `agentspace-driver-api` | The driver boundary as a Rust trait ([driver.md](driver.md)), committed by the architect before T03 from the pin on its cards | Before T03 |
  | `agentspace-driver-ec2` | The one v0 driver ([driver.md, The EC2 driver](driver.md#the-ec2-driver)) | T03 |

  The crates of v0.06 join the same workspace.
- **A crate's T01 half is portable, and its T02 half is Windows.** T01's code builds and is tested
  on Linux as on Windows; the Windows file layer (the lock's share mode, `ReplaceFile`, access
  rules) comes in T02, behind `cfg(windows)`. So T01 can run on the box.
- **The lease's key and certificate are made in the program's own code** (the `ssh-key` crate),
  never by running `ssh-keygen`; state.md allows any tool that writes OpenSSH's formats. The program
  then sets the private key's access rules itself, exactly as
  [state.md, The lease folder](state.md#the-lease-folder) says, and checks them before it signs.
  - **Why:** no other program's output can reach an error document (#0041, point 1;
    [driver.md Q17](driver.md#open-questions)); no program start is held under the lease state
    lock ([lease-api.md Q11](lease-api.md#open-questions)); and the certificate's exact fields can
    be tested on Linux in T01.
- **Schemas are embedded at build time** from `schemas/` (D37: carried inside the programs that
  check against them), and checked with a draft 2020-12 validator that fetches no remote
  reference. The program checks what it prints against the output schemas, as
  [lease-api.md, Output](lease-api.md#output) requires.
- **Help texts are tested against the blocks in lease-api.md, read from the doc at test time**, as
  the conformance suite's `test-help.ps1` reads them, so the two never drift.
- **v0.05-T01 runs on the box and is measured** (D33, #0049): it is the first turn that compiles,
  and v0.09 MUST measure one before it sets a size. The architect runs `agentspace-sample` around
  the turn, from before its dev lead starts to after its last subagent ends, and E0.05-T02-S04
  writes the run record.
  - **The box had no C toolchain and no Rust.** On 2026-11-19, before T01, the architect installed
    Ubuntu's `build-essential` (gcc 15.2, for linking) and rustup with Rust 1.99.0, rustfmt and
    clippy in `ubuntu`'s home, with links in `~/.local/bin`. Neither is in `box/` or the
    `agent-linux` profile's promise (lease-api.md, Profiles), so a host made fresh lacks them; a
    project host that builds Rust (v0.07, #0047) has to get them some way that release decides.
- **The cutover is D37's, by install, and deletes no prototype file.** v0.05's install puts
  `agentspace.exe`, the `agentspace` wrapper and the forwarder `agentspace-ssh-proxy.cmd` in
  `%USERPROFILE%\.agentspace\bin`, and that folder goes on the user's `PATH` with Terry's yes
  ([operator.md, Until v0.06 retires the prototype's entry points](operator.md#until-v006-retires-the-prototypes-entry-points)).
  - **The prototype's lease CLI files stay until v0.06 (#0055) removes every PowerShell file.**
    The `bin\` shims MUST keep running the prototype (D37), the conformance suite runs it through
    them by default, and the prototype's keeper loads `scripts\lease-state.ps1`.
  - **This amends #0054's done-when**, which said the PowerShell lease CLI files are deleted in
    v0.05. It now says that `agentspace` on the user's `PATH` runs the Rust program, installed.
- **Interop with the prototype is a conformance check with a second program**,
  `AGENTSPACE_PEER_CMD`, whose default is the prototype's lease CLI in
  `conformance/prototype/defaults.ps1`. It drives one lease through both programs, each way
  ([conformance.md, Naming the program under test](conformance.md#naming-the-program-under-test)).
- **The lease CLI's minor gaps from the final clean-room pass** (docs/runs/e0.04-t06-s01.md) were
  answered by the architect in the owning docs before any builder started, each marked with its
  gap's ID: driver.md (FA-01, FA-02, FA-03, FA-22), lease-api.md (FA-04, FA-08, FA-09, FB-03,
  FB-04, FB-22) and state.md (FA-05, FA-06, FA-07, FA-10, FB-20, FB-21). A builder who meets
  another gap MUST ask the architect, never read the prototype (D20).

### D39. Until v0.09, the box is sized by hand for each use: M class, a vCPU per agent

**Status: MVP only; replaced by v0.09 (#0049), whose size classes size a host from a lease, as D65 to D69 design.**

**Decided, Terry, 2026-11-19:** "before launching the EC2, make sure it's an M class instance with
vCPU count >= max number of parallel agents. I want each agent to have its own vCPU and at least 4
GB of RAM." The working rule is in [AGENTS.md](../AGENTS.md), "Sizing the box before it runs
agents".
- **Before the box is started for agent work, it MUST be an M-class instance with at least one
  vCPU per agent working at once**, counted as D27 counts them (the stories' plan peaks plus 1 for
  the dev lead), and so at least 4 GiB of memory per agent. The smallest such size is used.
- **It is changed through the stack's `InstanceType` parameter and `deploy`**, never by hand, so
  the stack and the instance agree.

**Why:** v0.05-T01 ran 11 agents on 4 vCPUs, and its first compile burst took the box to 3.83 of 4
cores (E0.05-T02-S04's run record; #0102). D27 sizes by concurrency, and this applies it by hand
until v0.09 measures a compiling turn and makes sizes a lease's to ask for. It does not set D27's
per-agent footprint, which still MUST come from measurement.

### D40. By v1.0, agentspace's own files are JSON where feasible

**Status: Direction, Terry, 2026-11-19, to be met by v1.0 (#0053).**

**Terry: "by v1.0, I'd like all file formats moved to JSON *where feasible*. It's not a mandate,
just a very strong preference. If there's a strong case for non-JSON, I delegate that call to
you."**
- **Every file whose format agentspace owns SHOULD be JSON by v1.0**, with a draft 2020-12 schema
  (D20). A line-per-record file, such as a log or the sampler's record, MAY be JSON Lines.
- **The architect decides each exception, and records it here with its case.** A strong case is
  one where another program owns the format, or where JSON would cost a person or a program
  something real. These are exceptions now:

  | Format | Why it stays |
  |---|---|
  | OpenSSH's files: keys, certificates, `config`, alias files, `known_hosts` | OpenSSH reads them, in its own formats |
  | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | Cargo's own formats |
  | `box/`: shell scripts and systemd units | The host runs them (D28: not product, so not ported) |
  | Markdown docs | Written for people |
  | localswim's receipt and Bot credential | localswim's files, which agentspace only reads ([operator.md](operator.md#the-bot-credential)); a change there is localswim's. Both are JSON since localswim's PR #13 (8d9d583, 2026-11-20) moved the credential from `bot.xml` to `bot.json`, which E0.07-T02-S05 (#0162) reads; they stay on this list only because their format is localswim's to set |
  | The keeper's logs ([keeper.md](keeper.md#log-lines)) | Lines of text that people read, and that the keeper's checks match line by line (Terry, 2026-11-19, agreeing) |
  | The CloudFormation template, `infra\agentspace.yaml` | Maintained by hand (D37's addendum), and YAML keeps its comments (Terry, 2026-11-19, agreeing) |

- **Where it bites, all the operator's, and so for v0.06 (#0055) to move as it rebuilds the
  operator**, each a change to its spec before that release's stories start:
  - `local.parameters`, `KEY=VALUE` lines today ([state.md](state.md#files-in-the-checkout)).
  - The instance-ID cache and the push stamp, one line of text each.

**The three moves, specified by the architect on 2026-11-19, before v0.06's stories** (state.md
owns each format; operator.md owns what the commands do with them). Each is
`{"version":0, ...}` with a schema under `schemas/state/`, carried inside the operator:

| The prototype's file | The rewrite's | Schema |
|---|---|---|
| `<checkout>\infra\local.parameters`, `KEY=VALUE` lines | `%USERPROFILE%\.agentspace\stack-parameters.json`, `{"version":0,"parameters":{...}}`, written by a person | `stack-parameters.state.schema.json` |
| `<checkout>\.instance-id`, the ID alone | `%USERPROFILE%\.agentspace\instance-id.json`, `{"version":0,"instance_id":"..."}` | `instance-id.state.schema.json` |
| `%USERPROFILE%\.agentspace\localswim-pushed.sha256`, the hex alone | `%USERPROFILE%\.agentspace\localswim-pushed.json`, `{"version":0,"stamp":"..."}` | `localswim-pushed.state.schema.json` |

- **Each JSON file has a new name, and the rewrite MUST NOT read the prototype's file.** The two
  caches need no migration: a missing cache is rebuilt from the stack, and a missing stamp makes
  one push, which changes nothing on the box. The parameters are moved by hand, once, in v0.06's
  cutover.
- **The parameters file is renamed `stack-parameters.json`**, because "local" says nothing once
  every file it sits beside is local too.
- **A parameters file that is not valid fails `deploy` before it calls AWS.** The prototype skips
  any line it does not understand, so a typo silently drops a parameter; a JSON reader has no
  reason to skip.
- **The conformance files that write the rewrite's cache or stamp change with it:**
  `conformance/operator/test-unmet-proxy-connect-time.ps1`, `test-unmet-proxy-steps.ps1` and
  `test-unmet-push.ps1` write `instance-id` and `localswim-pushed.sha256` in the old forms today.
  **Changed in T03, not T04** (architect, 2026-11-19, asked by the T03 dev lead): T03's done-whens
  run those files in full, so E0.06-T03-S03 changes the two ProxyCommand files and E0.06-T03-S01
  changes `test-unmet-push.ps1`, each in its own pull request; E0.06-T04-S01 keeps the harness.
- **Contract v0's files are already JSON** (the lease state file, the keeper's records, every
  document the CLI prints) and change nothing.

### D41. v0.06 cuts over on the laptop with no lease ready, and Terry's yes for each change to his configuration

**Status: Decided, architect, 2026-11-19, before v0.06 (#0055) is broken into stories.** It plans
the cutover that the v0.05 handoff asks to be planned before the stories, and that D30 and D37 say
takes Terry's yes.

**What changes on the laptop, and whose it is:**

| Change | Whose | Reversible by |
|---|---|---|
| v0.06's seven files copied into `%USERPROFILE%\.agentspace\bin`, the forwarder replaced ([operator.md](operator.md#until-v006-retires-the-prototypes-entry-points)) | agentspace's install folder | Copying v0.05's files back |
| `%USERPROFILE%\.agentspace\stack-parameters.json` written from `<checkout>\infra\local.parameters` (D40) | agentspace's file, holding the account's values | Deleting it |
| The user's `agentspace` entry in `~\.ssh\config` pointed at `<install>\agentspace-ssh-proxy.exe` ([operator.md](operator.md#proxycommand-lines-already-on-the-laptop)) | **Terry's file** | Restoring the old `ProxyCommand` line |
| The keeper task registered again by `agentspace-keeper-install`, which removes the prototype's launcher ([keeper.md](keeper.md#the-installer)) | **Terry's Task Scheduler** (D30) | Running the prototype's installer from a checkout of v0.05 |

**In order, as the release's last story, once every other v0.06 pull request is merged:**
1. **No lease is ready.** The architect checks `agentspace lease list` and releases what is ready,
   with Terry's yes for any lease it did not create. So no alias file names a forwarder the
   cutover replaces, and no holder runs, so stopping the prototype's keeper strands nothing.
2. **The architect asks Terry once, for the two changes that are his**, naming each with its
   reversal as the table gives it. Nothing below that is his happens before his yes.
3. **Stop the prototype's keeper** as [keeper.md, Stopping and starting the keeper](keeper.md#stopping-and-starting-the-keeper)
   says, and disable its task, so the repeating trigger cannot start it before step 6.
4. **Install v0.06's files** from one build of `main`, and write `stack-parameters.json`. Its
   values are the account's: they MUST NOT be printed into a transcript, a commit or a card.
5. **Point Terry's `agentspace` entry at the Rust ProxyCommand**, or have Terry do it.
6. **Run `agentspace-keeper-install` from the install folder.** Its read-back lines (keeper.md, FA-17)
   are the check that the task is registered.
7. **Live checks, with the box stopped at the start:** `ssh agentspace` through the Rust
   ProxyCommand starts the box and connects (#0055's first done-when); `localswim-pushed.json`
   appears and the box's `bot.env` changes, which shows the push (#0055's second); a lease
   created by the installed CLI connects through its alias, and the new keeper starts a holder for
   it and logs `started`; `agentspace-operator status` and `down` work. A `deploy` is run only if
   Terry wants the stack touched.
8. **Remove the prototype from the repository** (changed by D42, the same day: in the next turn's
   own story, E0.06-T06-S01, not in the cutover's pull request, so each fits under 3 hours and
   the live checks merge first): every
   PowerShell and `cmd` file in the product, `bin\` shims included (#0055's third done-when, D28);
   `infra\local.parameters.example`, replaced by `infra\stack-parameters.example.json`; and
   `box\system\install.sh`'s message changed to name `agentspace-operator sync` (FA-12). The
   architect's process tools stay (D28). The conformance suite's default programs, which name the
   prototype today ([conformance.md](conformance.md#naming-the-program-under-test)), move to the
   install folder's in the same pull request.
9. **README and AGENTS.md follow**, in the same pull request as step 8: every command they give
   runs the Rust programs, and AGENTS.md's box-sizing rule names `stack-parameters.json`.

**Why this order:**
- **No lease ready makes every forwarder and alias question moot.** The rules for keeping
  `scripts\ssh-proxy.cmd` while something names it
  ([operator.md](operator.md#proxycommand-lines-already-on-the-laptop)) exist for a cutover under
  load; this one has none, and leases last at most 24 hours (D24), so waiting is cheap.
- **The keeper is stopped before the install**, as operator.md's install rules ask, so no keeper
  runs a program the install renamed aside.
- **The repository changes last**, after the live checks, so a failed check leaves the prototype
  in `main` to fall back on.

**v0.06's first turn on the box is measured (D33, Terry, 2026-11-19).** Before it starts, the
architect sizes the box for its peak (D39), with the vCPU count from the turn's staffing plans;
grows the root disk ahead of its builds, under AGENTS.md's 60% floor, since five crates' builds
filled the 8 GiB disk in v0.05-T01; and arms one Monitor, before the dev lead starts, that
prints a line when the sampler's samples stop rising, when it is not running, or when the root
filesystem passes 80% used (AGENTS.md, "Watching a measurement"). The Monitor's script is written
and proven against a stopped sampler before it is trusted.

### D42. v0.06 builds the operator and the keeper as crates around two pinned boundaries, in six turns

**Decided, architect, 2026-11-19, when Terry had v0.06 (#0055) broken into stories.** It lays out
v0.06 the way D38 laid out v0.05, and the stories' cards carry the detail.

- **The same workspace, with one crate per area, so a turn's stories own disjoint folders** (D38).
  The new crates:

  | Crate | What | Built in |
  |---|---|---|
  | `agentspace-operator-api` | The pinned boundary: `Context`, `Mode`, `Say`, `Failure`, the AWS CLI's `Aws` trait and the `Remote` trait for ssh and scp | The architect, before T01 |
  | `agentspace-keeper-api` | The pinned boundary: the holder record, `Processes`, `Matcher`, `Starter`, `Locks`, `Clock` | The architect, before T01 |
  | `agentspace-operator` | `agentspace-operator.exe`: command line, help, messages; then `up`, `down`, `status`, `grow` and `ca` | T01, T03 |
  | `agentspace-run` | The programs the operator starts: the AWS CLI, ssh and scp, by full path or through the test seam | T01 |
  | `agentspace-box` | Starting the box and waiting for it, the patch question, the instance-ID cache, the AWS answers' members | T01 |
  | `agentspace-push` | The token push: its portable half, then its Windows half | T01, T02 |
  | `agentspace-zone` | The laptop's time zone as an IANA name, and the zone check | T01 |
  | `agentspace-keeper` | `agentspace-keeper.exe`: command line, the pass, log lines; then `-Watch`; then the real program | T01, T02, T03 |
  | `agentspace-keeper-files` | The holder record, the launch files, the holder command's matcher; and `agentspace-state`'s public atomic write | T01 |
  | `agentspace-ca` | The `ca` command and `sync`'s CA check | T02 |
  | `agentspace-keeper-win` | `Processes` and `Locks` on Windows | T02 |
  | `agentspace-launch` | `Starter`: the launch handshake, WMI and the launcher | T02 |
  | `agentspace-keeper-install` | `agentspace-keeper-install.exe` | T02 |
  | `agentspace-keeper-task` | `agentspace-keeper-task.exe`, a Windows program with no console | T02 |
  | `agentspace-sync` | `sync` and `deploy`, with the template and `box\system` embedded as they are (D37's addendum) | Stub by the architect before T03; built in T03 |
  | `agentspace-ssh-proxy` | `agentspace-ssh-proxy.exe` | Stub by the architect before T03; built in T03 |

- **Two boundaries are pinned as code before T01**, exactly as their cards give them and nothing
  else, because T01's stories meet at them in parallel: the operator's (how a command speaks, fails
  and calls the AWS CLI, ssh and scp) and the keeper's (what the pass asks of Windows). **Before
  T03, two stub crates** with pinned signatures, so `cargo xtask dist` can name every program
  while they are built.
- **T01 is portable and runs on the box, measured** (D33; D41). Every T01 crate builds and is
  tested on Linux, with the Windows calls behind the pinned traits; the architect runs each
  story's Windows check at review. **T02 is the Windows halves, T03 wires the programs, T04 runs
  and extends the conformance suite, T05 cuts over on the laptop, and T06 removes the prototype**
  (D41, as changed).
- **T05 gains a second story, E0.06-T05-S02 (#0140)** (Terry, 2026-11-20: "Add it to T05 as
  S02"). The cutover's live check found that the Rust `deploy` hangs after the AWS CLI's
  `cloudformation deploy` and never reaches `sync` (#0139; [runs/e0.06-t05-s01.md](runs/e0.06-t05-s01.md),
  step D). S02 fixes it, with a live no-op deploy as its check, and it MUST merge before T06
  starts: T06 removes the prototype, whose `deploy` is the only one that works until then, and D41
  keeps the repository's change last so that a failed live check leaves a fallback in `main`.
- **What E0.06-T06-S01 (#0135) keeps, and what retires** (architect, 2026-11-20, before T06
  starts, from `git ls-files` at `3c2618e`):
  - **The architect's process tools stay** (D28): `scripts/watchdog.ps1`,
    `scripts/report-milestones.ps1`, `scripts/milestones.json` and `scripts/watch-sampler.ps1`, the
    last of which the card's list missed.
  - **A crate's own test stand-in is not product code**, so
    `crates/agentspace-push/tests/win_support/ssh-standin.ps1` stays. #0055's third done-when reads
    "nothing but the conformance suite, the architect's process tools and the crates' test
    stand-ins".
  - **Interop retires with the prototype.** `conformance/lease-cli/test-interop.ps1` exists to show
    the rewrite and the prototype share their state, and with no prototype it has no peer. It and
    `AGENTSPACE_PEER_CMD` are removed. conformance.md keeps a note that cites the last run,
    36 passed each way in [runs/e0.06-t04-s01.md](runs/e0.06-t04-s01.md).
  - **`box/system/install.sh`'s one message is the exception to "MUST NOT edit box/system"** (D37's
    addendum), as D41 step 8 names it (gap FA-12). Nothing else in `box/system` changes.
- **The ProxyCommand runs `up` in its own process**, calling `agentspace-box` and
  `agentspace-push` itself, in caller mode. operator.md allows it, and no private argument exists
  to leak into help.
- **The keeper's launcher is `agentspace-keeper.exe` itself**, started by WMI with a private
  argument naming the request (keeper.md lets a build choose). It MUST NOT appear in a usage line.
- **The holder's process tree is stopped through the Windows API, never `taskkill`**, and `ca`
  sets access rules through the Windows API, never `icacls` (FB-15).
- **T02-S08 writes v0.06-T01's run record**, as E0.05-T02-S04 wrote v0.05-T01's, for v0.09 (#0049).
- **The pinned `Output` gains a fourth kind, `Checked`** (architect, 2026-11-19, from
  E0.06-T04-S02's conformance run): `--output json` passed, the exit code checked as for any call,
  and the answer never parsed. `ec2 start-instances`, `ec2 stop-instances` and `ec2 modify-volume`
  use it, since operator.md reads their answers no further than the exit code, and the build
  failed `down` and `grow` with "not JSON" when a stand-in answered with something else. E0.06-T04-S02
  makes the change to `agentspace-operator-api` in the same pull request that implements it in
  `agentspace-run` and uses it in `agentspace-box` and `agentspace-operator`, since a new variant
  breaks every exhaustive match until both land.
- **The holder record's file is a trait of the keeper's own** (architect, 2026-11-19, for
  E0.06-T01-S06, asked by its dev lead before GO). The keeper pin has no seam for reading, writing
  and removing `board-holder.json`, and its parser is E0.06-T01-S07's, built in parallel. So
  `agentspace-keeper` defines a small trait of its own (read the record as
  `Option<HolderRecord>`, with "a record that fails is no record" already applied; write it;
  remove it), tests the pass against a fake, and does not depend on `agentspace-keeper-files` in
  T01. E0.06-T03-S04 (#0129), which builds the real program, implements that trait over
  `agentspace-keeper-files` and `agentspace-state`'s `write_atomically`.
- **The cards carried in** are linked with `references`: #0027 and #0034 (the operator's), #0087
  (the caller), #0066 and #0068 (the keeper's), and #0110 (conformance hardening).

## v0.07: a host per project

**D43 to D54 settle v0.07's (#0047) design calls before its stories are laid out, and D55 lays them out**, as
[handoff-v0.06.md](handoff-v0.06.md#the-scope-ahead-v007) asks. Each is the architect's,
2026-11-20, unless it says otherwise. They bind the stories, which carry each one into the spec that
owns the fact ([driver.md](driver.md), [operator.md](operator.md), [state.md](state.md),
[keeper.md](keeper.md)) in the same pull request that builds it (D34). Terry made the two cost calls
the same day, taking the architect's recommendations: D54, and the disk order per host (D46).

**What none of them changes:** the lease contract (D1). Every output, schema and reason code of
contract v0 stays as it is; `pending` was already in it, and v0.07 is the first release whose
driver answers it.

### D43. Today's box becomes swimcoach's host, and agentspace is the second project

**Decided, architect, 2026-11-20.**
- **The `agentbox` stack and its instance become swimcoach's host**, with nothing about them
  renamed or replaced: its disk, its sign-ins, its checkout of swimcoach and its host key all stay.
  Terry's `agentspace` SSH entry keeps reaching it.
- **Every other project gets a host of its own at its first lease** (D25, D48). The first is
  `agentspace` itself, so v0.07's done-when is shown by an agentspace lease that creates
  agentspace's host and works a card on the agentspace board.
- **No lease is moved.** Every lease on the laptop is released today (handoff-v0.06.md, Where
  things stand), so no `ready` lease of another project points at the box when v0.07 lands.

**Why this way round:**
- **The done-when says swimcoach's leases are unaffected.** Keeping swimcoach on the box it already
  uses is the only reading in which nothing about them changes.
- **agentspace's own work is what needs the bootstrap proven.** Its turns compile Rust, and the box
  got Rust and gcc by hand (D38). A fresh agentspace host shows that the bootstrap gives them
  (D46), where moving agentspace onto the box would hide the gap.
- **The box's agentspace checkout and its hand-installed Rust stay harmless on swimcoach's host**,
  and a later cleaning pass MAY remove them.

### D44. A host is a CloudFormation stack of its own, from the one template

**Decided, architect, 2026-11-20.**
- **Each host is one stack, named `agentspace-host-<project>`**, deployed from the same template
  as `agentbox`, `infra\agentspace.yaml`. The project is the lease's normalized project
  ([lease-api.md](lease-api.md#project)), so the name is at most 48 characters and always starts
  with a letter, as CloudFormation requires.
- **The template gains a `HostName` parameter whose default, `agentbox`, reproduces every name the
  `agentbox` stack has today**: the alarm's name, the `Name` and `project` tags. A deploy of
  `agentbox` from the changed template MUST be an empty change set, and the story that changes the
  template MUST show that live, by creating the change set, reading it, and deleting it unexecuted,
  before its pull request is merged.
- **Each host's stack holds its own role, instance profile, security group, instance and alarm.**
  Nothing is shared between stacks, so deleting one host's stack touches no other.
- **The CA's public key becomes a stack parameter, `CaPublicKey`**, installed with its sshd drop-in
  by the first-boot user data, so a new host trusts the CA before the operator first connects
  (D47). It is a public key, and MAY be seen in the stack's parameters. `sync` still installs it as
  today, and replaces the first boot's copy with the same bytes. Changed for v0.07-T01 (architect,
  2026-11-20, on the dev lead's pin P8): `CaPublicKey` is the key's type and base64 only, without
  its comment, which names the computer that made it; the first boot writes that and a line feed,
  and `sync` later replaces it with the public key file's own bytes.
- **The laptop's AWS sign-in needs no new permission.** Today it is an administrator (operator.md,
  The AWS side). The calls a host adds are the ones `deploy` already makes, per stack, and
  operator.md's list of what the identity needs gains no new service.

**Why not the alternatives:**
- **More instances in the one stack:** every host's change would be an update of a shared stack,
  a failed update would roll every host back, and a host could not be removed on its own.
- **Instances made by the driver with `RunInstances`, outside any stack:** it would rebuild what
  the stack already does (role, security group, alarm, the parameters D39 sizes through) in Rust,
  and leave resources with nothing that deletes them together.
- **A second template:** two templates drift. A parameter with a default that changes nothing for
  `agentbox` keeps one.

### D45. The laptop keeps which host serves which project, in `hosts.json`

**Decided, architect, 2026-11-20** (D21, D40).
- **`%USERPROFILE%\.agentspace\hosts.json` is the host registry**: one record per host, with its
  project, its stack's name, its state (`creating`, `ready` or `failed`), its cached instance ID,
  and the time of its last attempt. It is JSON with a draft 2020-12 schema in `schemas/state/`,
  replaced atomically under a lock of its own, as state.md's other files are. Its exact form is
  state.md's, written by the story that builds it.
- **It replaces the instance-ID cache, `instance-id.json`.** That file was only a cache of one
  stack's output, and becomes a member of a record. Nothing migrates: a missing record's instance
  ID is read from its stack again, as the cache is today.
- **A missing `hosts.json` is seeded with one record: project `swimcoach`, stack `agentbox`,
  state `ready`** (D43). So the laptop as it is today reads as v0.07's starting point.
- **It is laptop state, never output** (D1). No lease command prints anything from it.

### D46. A new host gets its software from `box/system` and a bootstrap, never an image, in v0.07

**Decided, architect, 2026-11-20.** Card #0014 (versioned images) stays after 1.0.
- **A new host is made in four steps, by the operator:** its stack is deployed (D44), it is
  started and waited for as `up` does, `sync` installs `box/system`, the CA, the localswim CLI and
  `CLAUDE.md` as it does for the box, and the bootstrap installs the toolchain.
- **The bootstrap becomes the operator's to run**, as a command (D49), with `box/bootstrap.sh`
  embedded in the operator as `box/system` is (D37's addendum). Today a person runs it once per
  instance by hand (operator.md, The box's files).
- **The bootstrap MUST install what the box got by hand**: Ubuntu's `build-essential` (gcc, for
  linking) and rustup with the stable toolchain, `rustfmt` and `clippy` (D38's note). It stays
  safe to run again, so running it on the box makes the box's Rust the bootstrap's.
- **The `agent-linux` profile's promise does not change** ([lease-api.md](lease-api.md#profiles)).
  Rust is installed and not promised, as on the box today, since promising it would change the
  contract.
- **A new host's root volume starts at the template's 8 GiB, and grows under the same rule as the
  box's**, through `grow` with the host named. Terry's standing order on the box's disk covers
  every host, each on its own (Terry, 2026-11-20, closing Q7; AGENTS.md, "Growing the box's disk").

**Why not an image:**
- **A host is kept, and patched at every boot** (D25, how-it-was-built.md). An image would be
  stale the day after it was made, and the boot-time patch would do the same work anyway.
- **An image needs a build pipeline, a store and a version per profile** (#0014's design), which
  is a release of its own. The bootstrap costs about ten minutes, once per project.
- **The bootstrap already exists and has run on the box.** Extending it is a small change, and
  keeps one path for the box and every new host.

### D47. The operator reaches every host with its own certificate, and keeps each host's key under its own name

**Decided, architect, 2026-11-20.** The alias and the key's name were `agentspace-host-<project>`
until D55 the same day made them `agentspace-host.<project>`: no lease alias holds a dot, so none
can equal one, where a lease of project `host` could have. The stack keeps its hyphen (D44).
- **The operator connects to a host through an alias of its own, `agentspace-host.<project>`**,
  written by agentspace into `agentspace.d` with the same owner mark and rules as a lease's alias
  file ([state.md, The alias file](state.md#the-alias-file)), and a certificate the operator signs
  for itself with the CA: principal `ubuntu`, key ID `operator-<project>`, valid for one hour, made
  fresh for each command. So setting up a host needs no change to Terry's SSH configuration and
  never uses his own key.
- **Terry's `agentspace` entry keeps working for swimcoach's host**, and the operator MAY still use
  it there. Nothing makes him write an entry per host.
- **Each new host's key is checked under its own name, `agentspace-host.<project>`, in agentspace's
  own known-hosts file, `%USERPROFILE%\.agentspace\known_hosts`.** The operator's alias accepts a
  new key on first use (`StrictHostKeyChecking accept-new`), and every lease alias for that host
  checks it strictly (`StrictHostKeyChecking yes`, with `UserKnownHostsFile` and `HostKeyAlias`
  naming it). Terry's `known_hosts` is never written for a new host.
- **Swimcoach's lease aliases keep today's bytes**, `HostKeyAlias agentspace` and Terry's
  `known_hosts`, so its leases are unaffected (D43).
- **Accepting the first key is sound here** because the transport is a Session Manager session to
  a known instance ID, opened with Terry's AWS sign-in: nothing but that instance can answer it.
  Host certificates (#0032) stay after 1.0.

### D48. A lease for a project with no host answers `pending`, and a run that outlives the call makes the host

**Decided, architect, 2026-11-20.** It settles [driver.md Q3](driver.md#open-questions) for the EC2
driver: how a `pending` lease becomes `ready`.
- **When the project's host is `ready`, place is as today**: no AWS call, the credential and the
  alias, and `ready`. The alias names that host's key (D47).
- **When the project has no host, or its host is `creating` or `failed`, place answers `pending`
  and issues nothing.** Under the host registry's lock it records the host as `creating`, if it is
  not already, and starts the host's provisioning run unless one is running. Place still makes no
  AWS call and never waits for a host (driver.md, Reachability).
- **The provisioning run is `agentspace-operator`, started detached through `agentspace-launch`'s
  Starter**, the same handshake that lets the keeper's holder outlive a tool call (D42). It runs
  D46's four steps, logs to `<state folder>\hosts\<project>.log`, and sets the host's record to
  `ready` or `failed`. Changed by D55 the same day: the run is started through the new
  `agentspace-detach` crate, which shares agentspace-launch's WMI call, and its log is
  `%USERPROFILE%\.agentspace\hosts\<project>\provision.log`.
- **When the run finishes, it places every `pending` lease of that project**: under the lease state
  lock, it issues each one's credential and alias, as place does, and writes it `ready`. That is
  lease-api.md's transition "`pending` to `ready`, by agentspace, when placement finishes", and
  `get` and `list` stay read-only.
- **A failed run leaves the leases `pending`, never `unsatisfiable`**, because `unsatisfiable`
  means the request can never be met as asked (D5), and a failed deploy or an expired AWS sign-in
  is neither. A later `create` or `get` for the project starts the run again, at most once every 5
  minutes. A caller that gives up releases the lease, and its host is kept (D25).
- **A lease left `pending` on a host that is already `ready`**, because the run's placing of it
  failed, is placed by the next run too (architect, 2026-11-20, for v0.07-T01, ruling on the dev
  lead's pin P3). `get` starts a run for it under the same 5-minute rule, the run on a `ready` host
  does every step again, each safe to repeat, and ends by placing the project's pending leases. So
  no such lease is stranded, and `get` still never writes the lease state file.
- **A run never takes a `ready` host out of `ready`** (architect, 2026-11-20, for v0.07-T01, on the
  dev lead's finding). A host that is not `ready` sends its project's ready leases to the box (D55),
  where ssh's strict key check refuses them, so a run that marked a `ready` host `creating` would
  cut every lease of that project for its length, and for good if a step failed. A run on a `ready`
  host, and the driver that starts it, write only `last_attempt_at`; success keeps `ready`, and a
  failed step is logged and leaves `ready`. `creating` and `failed` apply only to a host that was
  never `ready`.
- **A lease's TTL starts when it becomes `ready`**: `expires_at` is the certificate's valid-before,
  as today, so time spent `pending` costs the caller none of it.

**Why not the alternatives:**
- **`create` waiting for the host:** making a host takes about ten minutes, longer than an agent's
  tool call runs by default, and a call killed while it waited would leave the host half made.
- **The first connection making the host**, as it starts a stopped box: the same ten minutes inside
  ssh's connection, which every client would have to wait out (the desktop app passes
  `ConnectTimeout=30`, D12's E3.03 finding), with the same half-made host when one gives up.
- **The keeper making hosts:** it would have to talk to AWS, which keeper.md rules out, for a job
  that happens once per project.

### D49. The operator's commands name a host, and without one act on today's box

**Decided, architect, 2026-11-20.**
- **`deploy`, `up`, `sync`, `down`, `status` and `grow` take `-Host <project>`.** Without it, each
  acts on swimcoach's host, the `agentbox` stack, exactly as today, so every command line in the
  repository and every conformance case keeps its meaning.
- **`deploy -Host <project>` creates the host's stack when it has none**, and updates it otherwise
  (D39's resize path, per host).
- **A new command, `bootstrap`, runs the toolchain bootstrap** (D46) on the host it names, or on the
  box without `-Host`.
- **`status -All` reports every host in `hosts.json`, and `down -All` stops every one that is
  running.** They are the cost guard's tools until v0.08 (D51).
- **The ProxyCommand finds a lease's host from the lease itself**: it reads the lease state file
  read-only, as the keeper does, for the record whose `ssh.host_alias` is its argument, then that
  lease's project in `hosts.json`. An `agentspace-host.<project>` alias names its host directly, and
  no argument means the box. So the alias line's format, `"<program>" <alias>`, does not change, and
  every alias file already on the laptop keeps working.
- **The token push and its stamp are per host** (D50).

### D50. One holder per host, one push stamp per host, one CA for every host

**Decided, architect, 2026-11-20.**
- **The keeper holds one holder per host** (D30's "one per project host"). Each pass groups the
  ready leases by their host, and for each host holds through the one that expires last, as it does
  for the one box today. The holder record holds one entry per host; its form is state.md's.
  The laptop's ends of the forwards are shared: each host binds the board's ports on its own
  loopback, so two holders never contend.
- **The push stamp is kept per host.** One stamp for all would differ at every connection to
  another host, since it covers the instance ID, and so push at every connection. A stamp per host
  pushes once per host per change, as today.
- **Every host trusts the one CA** (D7, D17). `sync -Host` sends it; a replaced CA MUST be synced to
  every host, which `sync -All` does, and D17's replacement steps now say "every host".
- **The time zone push and `CLAUDE.md`'s copy go to each host** as they go to the box (D32).

### D51. Until v0.08, every host a turn starts is stopped at its close

**Decided, architect, 2026-11-20**, extending AGENTS.md's rule for the box ("Sizing the box before
it runs agents", last bullet) to every host. v0.08 (#0048) replaces it with lease-based stop.
- **A turn's close MUST name every host it used**, stop each with `down -Host` or `down -All`, and
  confirm with `status -All` that each is `stopped`.
- **D11's idle stop and the CloudWatch backstop apply to every host**: each host's stack carries
  its own alarm (D44), and `sync` installs the idle check on each.
- **Each host is sized by hand for its use, as the box is** (D39), through its own stack's
  `InstanceType` and `deploy -Host`.
- **A host is made only for a project Terry has named** (D54, which closes Q6).

### D52. Sign-ins and the checkout on a new host are by hand, once, by a written procedure

**Decided, architect, 2026-11-20.** Card #0009 automates the sign-ins after 1.0.
- **Terry signs in on each new host once**: Claude Code, Codex and gh, as how-it-was-built.md's
  step 5 does for the box, over `ssh agentspace-host.<project>` (D47), with `-L 1455:localhost:1455`
  for Codex.
- **The project's checkout is cloned in the same sitting, after gh's sign-in**, with
  `gh repo clone <owner>/<repo> ~/<project>`, since agentspace knows no project's repository and a
  private one needs gh's credential. Cloning before the sign-in would fail for a private repository.
- **The procedure is written in the README by the story that makes the first new host**, and the
  operator's provisioning run ends by printing where it is. A host is `ready` for leases before the
  sign-ins: `ready` means a connection is expected to succeed (lease-api.md, States), not that an
  agent can work there yet.

### D53. v0.07's done-when is shown by live checks, with Terry's yes for their instance hours

**Decided, architect, 2026-11-20** (D19).
- **No stand-in can show a host being made**, so the stories' done-whens name these live checks:
  - **A first agentspace lease answers `pending`, and becomes `ready`** once its host's run ends,
    with `agentspace-host-agentspace` created, set up and recorded `ready` in `hosts.json`.
  - **The lease's alias reaches the new host**, and the keeper holds the board there.
  - **After Terry's sign-ins (D52), an agent in a desktop-app session on the lease works a card on
    the agentspace board.**
  - **A swimcoach lease created before and after connects to the box as it did**, with its alias
    file byte for byte as v0.06 writes it.
  - **`lease create`, `get` and `list` print documents that validate against contract v0's
    schemas** throughout.
  - **The `agentbox` change set is empty** (D44).
- **Terry's yes is asked once, before the first live check, for the instance hours it starts**, and
  every host it started is stopped at its close (D51).
- **conformance.md's coverage table is read for every "None" these change** (architect.md,
  Breaking an epic into stories, step 5), and each gets a case or one of these checks.

### D54. A host is made only for a project Terry has named

**Decided, Terry, 2026-11-20**, taking the architect's recommendation (Q6, closed). A host costs
its disk while stopped, about $0.64 a month at 8 GiB and $1.60 at 20 GiB, and about $0.20 an hour
while running at `m9g.xlarge`, so a mistyped `--project` MUST NOT make one.
- **`%USERPROFILE%\.agentspace\projects.json` lists the projects that may have a host made.** It
  is JSON with a draft 2020-12 schema in `schemas/state/` (D40), and its exact form is state.md's.
  Every entry takes Terry's yes, recorded with its date where the architect writes it; the
  architect MAY write the file, and nothing in agentspace adds to it on its own.
- **For v0.07 it holds `agentspace` alone.** swimcoach needs no entry: its host already exists in
  `hosts.json` (D43, D45), and the list governs only making a host.
- **A project that has a host in `hosts.json` is placed as D48 says, listed or not.**
- **A lease for a project with no host and no entry is `unsatisfiable` with `no_capacity`**,
  whose meaning contract v0 already gives: "The request is supported, but no capacity can be found
  for it." Its message names the project and no backend (D1), and nothing is issued or started.
  lease-api.md's note that v0 never reports `no_capacity` changes to say the EC2 driver reports it
  for such a project; no schema or code changes, so the contract does not.
- **A missing `projects.json` lists nothing.** So a laptop without one makes no host at all.

### D55. v0.07 is twenty stories in six turns: specs, foundations, programs, conformance, live

**Decided, architect, 2026-11-20, when Terry had v0.07 (#0047) broken into stories** ("Break v0.07
into stories"). It lays v0.07 out as D42 laid out v0.06, on the designs of D43 to D54, and the
stories' cards carry the detail. Terry, the same day: every story's card names each subagent with
its `Model/effort`, and a story is split into parallel workstreams wherever the work divides
cleanly and that is a net win (D31).

**The pinned names.** Every story uses these, and the specs written in T01 own their exact forms
(state.md for files, operator.md for command lines, driver.md for the driver):

| Name | Pinned as |
|---|---|
| The box | The host whose record has stack `agentbox`; its project is `swimcoach` (D43) |
| A host's stack | `agentbox` for the box; `agentspace-host-<project>` for every other (D44) |
| A host's operator alias, and its key's name | `agentspace-host.<project>` (D47), one file in `<ssh folder>\agentspace.d\`, owner mark `# agentspace host <project>; <comment>`, so that no lease's release or overwrite ever matches it |
| The host registry | `%USERPROFILE%\.agentspace\hosts.json`, `{"version":0,"hosts":[{"project","stack","state","instance_id"?,"last_attempt_at"?}]}`, `state` one of `creating`, `ready`, `failed`; its lock `hosts.lock` beside it. Missing reads as the box's record alone, written by the first change (D45) |
| The project list | `%USERPROFILE%\.agentspace\projects.json`, `{"version":0,"projects":[{"name","approved"}]}`, `approved` a date `YYYY-MM-DD`; missing lists nothing (D54) |
| A host's folder | `%USERPROFILE%\.agentspace\hosts\<project>\`: the operator's key `id_ed25519`, its certificate `id_ed25519-cert.pub` (key ID `operator-<project>`, principal `ubuntu`, valid one hour, signed by each command), the run's log `provision.log`, the run's lock `provision.lock` (held for the run's life, so a held lock means a run is running), and the host's push stamp `localswim-pushed.json` |
| The box's own files | Unchanged: its push stamp stays `%USERPROFILE%\.agentspace\localswim-pushed.json`, its holder record `<state folder>\board-holder.json`, so swimcoach's side reads as v0.06 left it |
| Agentspace's host keys | `%USERPROFILE%\.agentspace\known_hosts`, written only by ssh (D47) |
| Another host's holder | `<state folder>\board-holder.<project>.json` and `board-holder-ssh.<project>.log`, in the box's files' forms |
| A lease alias for a host that is not the box | Today's block with `HostKeyAlias agentspace-host.<project>` and one more line after it, `UserKnownHostsFile "<known_hosts path>"`, the path written as the key's is. The box's lease aliases keep v0.06's bytes |
| The operator's new command lines | `-Host <project>` on `deploy`, `up`, `sync`, `down`, `status`, `grow`; `-All` on `status`, `down`, `sync`; `bootstrap [-Host <project>]`; `provision -Host <project>`, the run D48 starts, which a person MAY run too |
| Which host a connection reaches | An `agentspace-host.<project>` alias: that project's host. A lease alias: the record in the lease state file with that `ssh.host_alias`, then its project's `ready` host. No alias, or a project with no `ready` host: the box |

**The turns.** Stories in a turn run in parallel, each in its own worktree; a turn starts when every
pull request of the turn before is merged (D16). All six run on the laptop; only T05 and T06 start
instances.

| Turn | Stories | What | Peak |
|---|---|---|---|
| T01 | S01 to S05 | **The specs and the template.** state.md and the two new schemas; operator.md; driver.md and lease-api.md; keeper.md; the template's `HostName` and `CaPublicKey` with the empty `agentbox` change set shown live, and the bootstrap's Rust and gcc | 9 subagents |
| T02 | S01 to S04 | **The foundations, as libraries.** `agentspace-hosts` (new); `agentspace-detach` (new), the detached start out of `agentspace-launch`; `agentspace-alias` and `agentspace-credential` for host aliases and the operator's certificate; `agentspace-box` and `agentspace-push` per host | 10 |
| T03 | S01 to S05 | **The programs.** The driver and the lease CLI; the operator's commands; `deploy`, `sync`, `bootstrap` and `provision`; the ProxyCommand; the keeper | 13 |
| T04 | S01 to S04 | **Conformance.** The lease CLI's cases; the operator's and the ProxyCommand's; the keeper's, with conformance.md's coverage; the whole suite against T03's build | 11 |
| T05 | S01 | **The install and the live checks** of D53, but the sign-ins | 2 |
| T06 | S01 | **The done-when:** Terry's sign-ins on agentspace's host, its checkout, and an agent working an agentspace card there | 1 |

**Pins as code, committed by the architect before T03**, as D42's stubs were: the driver boundary
gains `Driver::check_pending` (a default that does nothing) and the driver's `Context` gains the
operator program a run starts; `agentspace-driver-ec2` gains a stub `place_pending`; and
`agentspace-sync` gains stubs of `bootstrap` and `provision` and a `Target` parameter on `sync` and
`deploy`. Each returns `not built yet` until its T03 story builds it. The T02 crates' public APIs
are pinned on their cards, so T03 builds on merged code.

**Changed during T01 (architect, 2026-11-20, on the dev lead's questions from E0.07-T01-S03):**
- **The driver's `Context` also gains `state_dir`**, since `place_pending` writes the lease state
  file, and the run builds that Context from the variables its detached start is given.
- **The lease CLI names the run's program with a variable of its own, `AGENTSPACE_PROVISION_CMD`**,
  never `AGENTSPACE_OPERATOR_CMD`, which the conformance harness sets to the operator under test.
- **`agentspace-detach` passes an environment** (`AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR`,
  `AGENTSPACE_PROXY_COMMAND` when set, and `USERPROFILE`), and uses no launch files.
- **E0.07-T04-S01 re-seeds the lease CLI suite's throwaway homes**, because under D54 a home with no
  `projects.json` answers `no_capacity` for every project but swimcoach; E0.07-T03-S01's
  conformance done-when keeps only the cases that never reach place. T04-S01 grows to 2.5 hours.
- **A host is never sized by the box's parameters** (on E0.07-T01-S01's finding: one
  `stack-parameters.json` would size whichever host is deployed next). A new host's stack takes
  only `SubnetId` and `VpcId` from that file, with every other size at the template's default; an
  existing host's stack is deployed with every parameter's previous value; and a host other than
  the box changes size only through `deploy -Host <project> -InstanceType <type>` (D39, D51).
  Plain `deploy` of the box keeps today's path.

**Added during T02 (architect, 2026-11-20, on Terry's word: "Apply card #161 ... Follow
agentspace's own process for the work"):**
- **E0.07-T02-S05 (#0162) moves the token push from localswim's `bot.xml` to `bot.json`**, carrying
  #0161. localswim's PR #13 (8d9d583) changed the launch credentials from CLIXML to JSON, so after
  localswim's next cold start the push could read no token. Its spec is localswim's
  `docs/windows-operations.md`, "The service receipt and credential files", and operator.md cites
  it rather than restating the formats.
- **It is a story of T02 that runs after E0.07-T02-S04 (#0149)**, as architect.md's step 11 puts a
  defect that blocks the next turn: #0149 changes the same push crate, and the fake `bot.xml` it
  replaces is written by tests in the operator, sync and ssh-proxy crates that T03's #0151, #0152
  and #0153 own. T02's wall-clock budget becomes 5 hours, and v0.07's 17.
- **The run records in `docs/runs/` are not rewritten**: they record what was true when they ran.

**Carried in, with `references`:** #0137 (tests that change their own environment), its
`agentspace-launch` half by E0.07-T02-S02 and its `agentspace-push` half by E0.07-T02-S04; #0138
(a conformance file that does not tolerate a missing `board-holder.json`) by E0.07-T04-S03; and
#0027 (the operator from Git Bash) by E0.07-T05-S01, as the handoff asks at the next live use.

**Why this shape:**
- **Specs first, as v0.04 did for v0.05 and v0.06 (D34).** Twelve decisions change four specs, and
  a builder reads the spec, never the decision alone. Writing them in parallel is safe because the
  names above are pinned here, and the architect reviews the four together.
- **Libraries before programs.** Every T03 program needs the registry, the detached start, the
  host alias and per-host box access; built first, they are merged code in T03, and only the
  calls between T03's own stories need stubs.
- **The live checks come last, and only after the whole suite passes**, as v0.06's cutover did
  (D41), so no instance is started for a build that fails a check a stand-in could have shown.
- **T06 is a story of its own** because it waits on Terry's sign-ins, which pause its clock in
  Needs Terry, and T05's evidence MUST be merged before he is asked.

## v0.08: host lifecycle and cost guard

**D56 to D62 settle v0.08's (#0048) design calls before its stories are laid out**, as
[handoff-v0.07.md](handoff-v0.07.md#the-scope-ahead-v008) asks. Each is the architect's,
2026-11-21, unless it says otherwise. They bind the stories, which carry each one into the spec
that owns the fact in the same pull request that builds it (D34). One number and two choices were
Terry's, Q8 to Q10, and he made them the same day, taking the architect's recommendations: a
15-minute grace, the box guarded like every host, and unexplained hosts reported by the log and
`status -All` alone. He also said yes to D62's instance hours ahead of the live turn.

**What none of them changes:** the lease contract (D1). Every output, schema and reason code of
contract v0 stays as it is, and nothing in a lease says when or why its host stops.

**Words used here:**
- **A live lease** is one whose state, read as lease-api.md's States read it, is `pending`, or
  `ready` with its `expires_at` after now. An `expired` lease is not live, though its record still
  says `ready`.
- **A host's last lease end** is the latest of its project's leases' ends: `released_at` for a
  released lease, `expires_at` for one that expired. A host that never had a lease has none.
- **A host's lease horizon** is the latest `expires_at` among its project's live ready leases, or
  its last lease end when it has none.
- **Changed for v0.08-T01 (architect, 2026-11-21, on the dev lead's pins P10 and P11; state.md,
  "Live leases and the lease horizon", owns the exact rule):** a released lease's end is the
  earlier of its `released_at` and its `expires_at`, so a lease released after it expired ended at
  its expiry; a released lease that was never ready ends at its `released_at`. A project whose only
  live leases are `pending` has the reading's time as its horizon, so a host waiting on a
  placement is not powered off by its own check while the laptop's guard runs, and stops a grace
  after the last guard run when the laptop goes away.

### D56. A host runs only while its project holds a live lease; the laptop's guard stops it otherwise

**Decided, architect, 2026-11-21.** It replaces D11 on the laptop's side, and D51 once v0.08 is
installed. **The grace is 15 minutes** (Terry, 2026-11-21, closing Q8), so a host is stopped
within 25 minutes of its last live lease's end.
- **A live lease explains its host, whatever the host is doing.** A quiet lease, with no session
  and no CPU work, keeps its host from the guard until the lease ends. A host whose project holds
  no live lease is unexplained.
- **A new operator command, `agentspace-operator guard`, stops every unexplained host.** In one
  run it reads `hosts.json`, the lease state file read-only, and every host's EC2 state in one
  `ec2 describe-instances` call. Each `running` host with no live lease is stopped, as
  `down -Host` stops it, once the grace has passed since the later of its launch time and its last
  lease end. Before then the run logs it as unexplained with the time it will stop.
- **The guard MUST NOT stop a host while its provisioning run holds `provision.lock`** (D55's pin),
  since a host being made has only `pending` leases, and its run starts and syncs it.
- **The keeper starts the guard**, detached through `agentspace-detach` as the lease CLI starts a
  provisioning run (D48), at its first pass and then at most once every 5 minutes, and never while
  one runs (a `guard.lock` beside `hosts.json`, held for the run's life). **The keeper still never
  talks to AWS or a host itself** (keeper.md, Where it runs): it starts a program, as it starts
  holders. Its log line for the start is keeper.md's, written by the story that builds it.
- **The guard logs to `%USERPROFILE%\.agentspace\guard.log`**: one line per host per run that
  stops a host, finds one unexplained, or fails, and nothing for a host that is explained or
  stopped. A run whose AWS call fails, an expired sign-in included, logs that and stops nothing.
- **The stated time:** a host is `stopped` within the grace plus 10 minutes of its last live
  lease's end, whether by release or by expiry: up to 5 minutes until the next guard run, and the
  rest for the stop itself. The same bound holds for a host started with no lease at all,
  counted from its launch.
- **Nothing renews a lease on its own.** A lease ends at its `expires_at` unless its holder renews
  it (D24), and its host is stopped a grace later. Sessions open past expiry stay open, since a
  certificate is checked only at login, and end when the host stops. The architect's turns SHOULD
  give a dev lead's lease a TTL that covers the turn, and renew it when a turn runs long.
- **Work on a host by hand is done under a lease.** D52's sign-in procedure, a `sync`, `grow` or
  `bootstrap` run by a person, and any direct session: each is stopped a grace after launch or
  after the project's last lease end, unless a live lease explains the host. README's procedure
  gains a first step, `agentspace lease create --project <project>`, and its release at the end.
- **Every host is guarded, the box included** (Terry, 2026-11-21, closing Q9). Direct work on the
  box through his own `agentspace` entry is done under a swimcoach lease, as on any host.
- **Hosts stay `ready` in `hosts.json` when stopped**: the registry records whether a host is made,
  never whether it runs (D45). Start-on-connect is unchanged, so a stopped host's next lease starts
  it with its first connection, as the box's leases do today.

**Why the laptop and the keeper:**
- **Only the laptop knows a lease ended.** Expiry is decided when a lease is read (lease-api.md,
  States), and release writes only the laptop's state file. The keeper already reads that file
  every 30 seconds and applies expiry by the pass's time (keeper.md, Which leases are ready).
- **Starting the guard from the keeper adds nothing to Terry's laptop.** A scheduled task of its
  own would be a second task to install, watch and reverse (D41); the keeper's task already runs at
  every logon.
- **Lease-based, not activity-based, is the release's point** (D25). D11 stopped a host under a
  held lease that went quiet, and kept one up for any leftover session after release (#0064); the
  guard does neither.
- **One `describe-instances` call every 5 minutes** costs nothing measurable, and a host's stop
  never waits on a person.

### D57. A host bounds itself by its lease horizon while the laptop is away

**Decided, architect, 2026-11-21.** It keeps D11's on-host idle stop and adds a lease-based bound
under it.
- **The on-host idle check keeps D11's rule unchanged**: power off after 60 minutes with no SSH or
  Session Manager session, no patch run and no CPU work. Sixty minutes is Terry's number, with his
  reason in README ("a lunch break shouldn't cost a reboot"), and a host left alone over lunch
  with its lease live is explained.
- **Each run of the guard sends every running host its lease horizon** when it differs from the
  one last sent, with one `ssm send-command`, as `up`'s patch check sends one. The host keeps it in
  `/run/agentspace/lease-horizon`, which a boot clears, so a host that has just started holds no
  horizon until the next guard run.
- **Once the horizon plus the grace (D56's 15 minutes) has passed, the idle check powers the host off at the
  first check with no SSH or Session Manager session, whatever its CPU is doing.** A host with no
  horizon file keeps D11's rule alone.
- **So a host bounds its own cost with the laptop off or asleep**, which is when the guard cannot
  run: an idle host stops after 60 minutes, as today, and a busy one no later than its horizon,
  the grace, and one 5-minute check after its last session ends.
- **Changed when v0.08 was laid out (architect, 2026-11-21, D63):** the guard sends the horizon to
  every running host at every run, not only when it changed, so no file on the laptop has to
  remember what was sent, and a host that rebooted gets it back within 5 minutes. A host whose
  project has never had a lease is sent its launch time. A send that fails is logged and tried
  again at the next run.
- **Changed for v0.08-T01 (architect, 2026-11-21, on the dev lead's pin P12): the guard sends the
  later of the horizon and the host's launch time.** A host started with no live lease would
  otherwise be sent a last lease end from before its launch, and power itself off at its first
  check with no session, before the guard's grace, which D56 counts from the later of the two. With
  the later of the two, the host's bound and the guard's agree. state.md's horizon is unchanged;
  operator.md's guard section states the send.
- **Changed for v0.08-T01 (architect, 2026-11-21, on the dev lead's pin P9): a running patch run
  holds off the horizon power-off, as a session does**, and as it holds off D11's rule. A power-off
  in the middle of an upgrade can leave dpkg half configured, and the patch unit is bounded by its
  own 20-minute limit (`TimeoutStartSec`), so the bound moves by at most that.

**Why on the host, and why a horizon:**
- **The keeper's holder is a session on every host with a live lease while the laptop is on**
  (D50), so the laptop away is the case the guard cannot cover. With the laptop off, every
  desktop-app connection is gone too (D8), and only work running on the host itself keeps it busy:
  an agent run there in a terminal multiplexer, a sampler (D33), or a runaway process.
- **CPU work still counts before the horizon**, because an agent working on the host on its own
  with a live lease is the remote compute agentspace exists for. After the horizon no lease
  explains the work.
- **A horizon from a guard run, not from the push.** A renew changes no connection (D24), so a
  horizon sent by the ProxyCommand's push would go stale at every renew and could stop a host
  under a renewed lease. The guard sends it within 5 minutes of a renew while the laptop is on. A
  renew made less than 5 minutes before the laptop goes away is the one gap, and it costs a host
  stop, never a running cost.
- **Not by instance tags read from the metadata service**, which would need a change to every
  host's instance options for the same one value.

### D58. The CloudWatch backstop stays, lengthened from 3 hours to 24

**Decided, architect, 2026-11-21.** It closes #0043, which becomes OBE when it is built.
- **Each host's alarm stops it after 24 hours averaging under 1% CPU**: `Period` 3600 and
  `EvaluationPeriods` 24, the longest an alarm with a one-hour period can evaluate. The template's
  alarm changes in place, so `agentbox`'s change set holds that one modification, read live before
  the merge as D44's empty one was.
- **Changed for v0.08-T01 (architect, 2026-11-21, on #0171's review): 24 hours is a choice, not
  AWS's limit.** The clause above is wrong: an alarm whose period is an hour or more MAY evaluate up
  to 7 days (`Period` times `EvaluationPeriods` at most 604,800 seconds); the one-day limit holds
  only for periods under an hour. A day is kept because it outlasts any quiet stretch a held lease
  is expected to have, while bounding a host whose idle check has died to about $5 of running time.
- **Its job is a host whose on-host idle check has died**, idle, with the laptop away. Its cost
  does not change: it is the alarm each host already has.

**Why not the alternatives:**
- **Three hours stops a host under a quiet held lease**, sessions included (#0043), and under v0.08
  every live lease holds a session through the keeper's holder, so it would do so routinely.
- **A heartbeat metric from the host**, alarmed when it stops, would catch a dead check sooner, but
  needs `cloudwatch:PutMetricData` on every host's role, a custom metric per host, and an alarm
  that treats missing data as breaching while a stopped host reports none. That is more design and
  more cost than the case is worth.
- **The case left uncovered is a host whose idle check has died while it is busy, with the laptop
  away for longer than a day.** The guard reports and stops it when the laptop returns, if no live
  lease explains it.

### D59. A release ends the lease's own connections on the laptop

**Decided, architect, 2026-11-21.** It meets #0064.
- **`agentspace lease release` ends every ssh client on the laptop whose destination is the
  lease's alias, with its ProxyCommand**, before it deletes the alias. The keeper's holder is left
  to the keeper, which stops it at its next pass (keeper.md, Stopping a holder).
- **The far end of each ended connection is dropped by the host's sshd within about 90 seconds**,
  by the `ClientAlive` bound every host already has (`51-agentspace-logins.conf`), since the
  Session Manager tunnel never passes a client's close on.
- **The release's document is unchanged** (contract v0). How many connections it ended is logged
  where lease-api.md says, written by the story that builds it.
- **No call to a host is made.** A release stays laptop-local and works on a stopped host.
- **Changed when v0.08 was laid out (architect, 2026-11-21, D63):** the keeper's holder is ended
  with the rest, rather than left to the keeper. A release then needs nothing from the keeper's
  files, and the keeper treats the holder as one its look finds gone, with its release grace
  (keeper.md, Release grace), which is what its own stop at the next pass would have cost.
- **Changed for v0.08-T01 (architect, 2026-11-21, on the dev lead's pin P23.3): a release walks
  twice**, once before it deletes the alias and once after, and counts both in its one line. A
  client that reconnects by itself, as the desktop app's does, can come back through the alias in
  the seconds the first walk takes, while the alias and its credential still exist; the second
  walk ends it, and once the alias is gone no client can connect through it again.

**Why the laptop's end:**
- **Every client is on the laptop today** (D8): the desktop app's connections and the holder.
- **The host's end needs a connection to the host at every release**, which would make release slow,
  and fail on a stopped host. The guard stops a host whatever sessions it holds (D56), so a
  connection the release missed costs at most a grace of running time.

### D60. `status -All` is the one command that lists every host with its live leases

**Decided, architect, 2026-11-21.**
- **`agentspace-operator status -All` gains, for each host**: its project, its registry state, its
  EC2 state, its live leases' count, and for a running host that no live lease explains, the word
  `unexplained` and the time the guard stops it. Its exact lines are operator.md's.
- **It prints counts, never lease IDs**, so its output is safe to show in a live run's record
  (AGENTS.md, "A live run on Terry's real setup MUST mask what it reads before it prints it").
- **An unexplained host is reported by the guard's log line and by `status -All`, and nowhere
  else** (Terry, 2026-11-21, closing Q10): the guard stops it on its own, so nothing needs his
  phone.

### D61. v0.08 carries #0163, #0164, #0166 and #0167

**Decided, architect, 2026-11-21.** v0.08 starts and stops hosts more than any release before it,
and each of these is about a host's start, size or tools.
- **#0163, a new host's first provisioning run failing at sync.** The outcome is required: a first
  run succeeds without the 15-minute patch wait, and the host has had its patch run before its
  first lease is placed. The architect's preferred mechanism is that the run starts this boot's
  `agentspace-patch.service` itself once `sync` has installed it, and waits for its marker, a
  patch reboot included. The spec turn rules on it after a probe on a host, since the unit is a
  `oneshot` that MAY reboot (handoff-v0.07.md, Lessons: ask for a probe before ruling on how an
  OS call behaves).
- **#0164, `grow -GiB 0`.** `grow` refuses an `<n>` of 0 or less as a usage error, with exit 2 and
  no AWS call. The AWS stand-in stops modelling EC2's refusal, and a conformance case counts no
  `ec2 modify-volume` call. **Changed for v0.08-T01 (architect, 2026-11-21, on the dev lead's pin
  P7): exit 1, not 2.** operator.md's Exit codes give every operator usage error exit 1, and grow
  is not made the one exception. A negative `<n>` is refused the same way, so conformance's
  existing `-GiB -1` case in `test-unmet-commands.ps1` changes with it, in E0.08-T04-S01.
- **#0166, tools on an interactive shell's `PATH` only.** Every tool the `agent-linux` profile's
  agents use (`rustc`, `cargo`, `codex`, `claude`, `gh`) MUST be found by a non-login,
  non-interactive shell over ssh. The bootstrap links each into `/usr/local/bin`, through rustup's
  proxies and fnm's default alias, so each follows its tool's updates. A live check on both hosts
  runs `ssh <alias> 'command -v rustc cargo codex claude gh'`.
- **#0167, "The host is ready." printed twice, and a push at every start.** The spec turn checks
  operator.md's stamp rule against the build's push decision through the conformance seam, and
  rules whether a start makes the push due or the build pushes when it is not. Then the nested
  line is withheld from the outer connection's stderr.

**Changed when v0.08 was laid out (architect, 2026-11-21, D63):**
- **#0163's mechanism is pinned, and its probe is T05's live check.** `box/system/install.sh`,
  when it finds no `/run/agentspace/patched`, starts `agentspace-patch.service` with `--no-block`
  as well as enabling it; `sync`, right after its install row, waits for the patch check to answer
  `patched` before its next row, through Session Manager as `up`'s wait does, which outlasts a
  patch reboot. Every nested connection after that row finds the host patched. T05 shows it on a
  host with this boot's marker removed by hand, since both hosts already exist and no new host is
  made in v0.08.
- **A failed patch run never fails a sync** (architect, 2026-11-21, for v0.08-T01, pin P21). The
  wait ends at `patched`, or at once when `agentspace-patch.service` has failed, with a warning
  that points at the host's journal, and sync goes on. Before v0.08 no sync depended on patching,
  and a broken package mirror MUST NOT block every sync. The install starts a failed run again at
  each sync, so each sync is its retry. **The same holds for a connection** (pin P24.2): `up`
  treats a failed run as it treats an unknown answer and does not wait, so a lease's connection in
  a host's first 20 minutes after a failed run gets through at once, where v0.07 made it wait 15
  minutes and then fail. The host runs unpatched until a later run succeeds, as any host whose
  patch run fails does.
- **#0166's PATH is set by `box/system`'s install, not by links from the bootstrap:** the install
  writes the `PATH` of `/etc/environment`, which every ssh session reads through PAM, with
  `~/.cargo/bin`, `~/.local/bin` and fnm's default alias folder after the system's own folders.
  Links made by the bootstrap would dangle for tools installed later by D52's sign-ins, and the
  install runs at every `sync`, so the box gets it too.

**Not carried:** #0137 and #0138 are fixed and merged, and wait only on Terry's card moves;
#0109 and #0082 stay before v0.09's measurement. **Changed by D64 (architect, 2026-11-23):**
v0.09 runs no measured turn, so both wait for the next one.

### D62. v0.08's done-when is shown by live checks on both hosts, with Terry's yes for their instance hours

**Decided, architect, 2026-11-21** (D19, as D53 for v0.07).
- **No stand-in can show a host stop, so the stories' done-whens name these live checks**, each on
  the box and on agentspace's host unless it says one:
  - **Release:** the project's last live lease released, the host `stopped` within D56's bound,
    measured from `released_at`.
  - **Expiry:** a lease with a short TTL left to expire, the same bound from its `expires_at`.
  - **A quiet held lease:** a live lease with no session but the holder's and no CPU work keeps its
    host past the grace.
  - **Next lease:** a new lease's first connection starts the stopped host, and the time to a
    prompt is recorded.
  - **Unexplained:** a host started with `up -Host` and no lease is reported by `guard.log` and
    `status -All`, and stopped within the bound (agentspace's host only).
  - **The laptop away:** with the keeper stopped and a CPU-busy process left on the host, the host
    powers itself off past its horizon plus the grace (D57; agentspace's host only).
  - **A release ends its connections** (#0064): a desktop-app session left open on a lease, the
    lease released, no ssh client of that alias left on the laptop, and no session of that lease
    on the host 2 minutes later.
  - **#0163, #0166:** a host's provisioning run's patch step on agentspace's host, with no
    15-minute wait, and `command -v` finding every tool on both hosts.
  - **The alarm:** `agentbox`'s change set holds only the alarm's modification.
- **Terry said yes on 2026-11-21, ahead of the live turn, to up to 6 instance hours** at
  `m9g.xlarge`, about $1.20, most of them graces waited out. A live turn that would pass 6 MUST
  ask him again before it does. Every host is confirmed `stopped` at the close.
- **conformance.md's coverage table is read for every "None" these change** (architect.md,
  Breaking an epic into stories, step 5), and each gets a case or one of these checks.

### D63. v0.08 is twenty-one stories in six turns: specs, foundations, programs, conformance, live

**Decided, architect, 2026-11-21, when Terry had v0.08 (#0048) broken into stories:** "break out
v0.08 stories and create a staffing plan that uses as many subagents per story as can be used
effectively". It lays v0.08 out as D55 laid out v0.07, on the designs of D56 to D62, and the
stories' cards carry the detail.

**Terry's staffing order, the same day, extends D31:** each story uses as many subagents as can be
used effectively. Where one file has to be written by several hands, drafters each write their own
sections into draft files they own, under the worktree's `target\drafts\`, and one integrator owns
the file. Disjoint ownership (D31) still holds, and parallelism that costs more than it saves is
still not planned.

**The pinned names.** Every story uses these, and the specs written in T01 own their exact forms
(operator.md for command lines, the guard and the host's files; keeper.md for the keeper; state.md
for laptop files; driver.md and lease-api.md for release):

| Name | Pinned as |
|---|---|
| The grace | 15 minutes, a constant, not a parameter (D56); on a host, `HORIZON_GRACE_MINUTES=15` in `/etc/agentspace/idle.conf` |
| The guard | `agentspace-operator guard [-Quiet]`, with no `-Host` or `-All`: every host in `hosts.json`, every run. Its logic is the new crate `agentspace-guard`; the operator crate dispatches to it |
| The guard's files | `%USERPROFILE%\.agentspace\guard.log`, one line per host per event (stopped, unexplained, or failed, a failed horizon send included; a horizon sent is not logged), each with its time and `host=<project>`; `guard.lock` beside `hosts.json`, held for the run's life. A run that finds it held exits 0 at once and logs nothing |
| Live lease, last lease end, horizon | Functions of `agentspace-state` over the lease state file's records, one home for both the guard and `status -All`, by the words D56 to D57 define. Their signatures are pinned on E0.08-T02-S01's card |
| The keeper's start of the guard | At its first pass, then at most once every 300 seconds, through `agentspace-detach`, never while `guard.lock` is held. The program is `AGENTSPACE_GUARD_CMD` when set, else the install folder's `agentspace-operator.exe`, with the argument `guard` |
| The horizon on a host | `/run/agentspace/lease-horizon`, one line holding the horizon as Unix seconds and a line feed, root-owned, mode 0644, written by one `ssm send-command` per running host per guard run, with the comment `agentspace guard: lease horizon` |
| The idle check's seams | `AGENTSPACE_IDLE_STATE` (default `/run/agentspace`), `AGENTSPACE_IDLE_CONF` (default `/etc/agentspace/idle.conf`) and `AGENTSPACE_PROC_STAT` (default `/proc/stat`), for `box/tests` alone |
| `status -All`'s new lines | `Leases    <n> live` for every host, and `Guard     unexplained; stops at <h:mm><am\|pm>` for a running host no live lease explains, in the laptop's zone, both after the host's existing lines |
| Release's connections | The new crate `agentspace-connections`: `end_alias_clients(alias) -> Result<usize>` ends every `ssh.exe` on the laptop whose destination argument is the alias and every `agentspace-ssh-proxy.exe` whose argument is, each with its tree, and returns how many it ended. The EC2 driver's release calls it before it deletes the alias |
| The alarm | `Period` 3600, `EvaluationPeriods` 24, description `Stops ${HostName} after 24 hours averaging under 1% CPU.` (D58) |
| #0163 and #0166 | As D61's change above |
| Branches | `v0.08/e0.08-tnn-smm` (D29) |

**The turns.** Stories in a turn run in parallel, each in its own worktree; a turn starts when every
pull request of the turn before is merged (D16). T01 to T04 run on the laptop and start no
instance; T05 and T06 start both hosts. The peak is the turn's subagents at once, its dev lead
not counted.

| Turn | Stories | What | Peak |
|---|---|---|---|
| T01 | S01 to S05 | **The specs, the template and the host's files.** operator.md; keeper.md and state.md; driver.md and lease-api.md; the template's alarm with its change sets read live, and README; `box/system`'s idle check, `idle.conf` and install, with a test of the idle check | 16 |
| T02 | S01 to S04 | **The foundations, as libraries.** `agentspace-state` and `agentspace-hosts`; `agentspace-connections` (new); `agentspace-box`'s calls for the guard; `agentspace-push` and `agentspace-ssh-proxy` for #0167 | 10 |
| T03 | S01 to S05 | **The programs.** `agentspace-guard` (new); the operator's `guard`, `status -All` and `grow`; the keeper's start of the guard; the EC2 driver's release; `sync`'s patch wait | 13 |
| T04 | S01 to S04 | **Conformance.** The operator's cases; the lease CLI's; the keeper's; the whole suite, the crates and the idle check's test against T03's build, with conformance.md | 10 |
| T05 | S01 | **The install and the host-side live checks:** the alarm deployed, `sync` on both hosts, #0163, #0166, and the laptop away | 3 |
| T06 | S01, S02 | **The lifecycle, live, one story per host:** release, expiry, a quiet held lease, the next lease; on agentspace's host also an unexplained host and #0064 with Terry's desktop-app session | 4 |

**Pins as code, committed by the architect before T03**, as D55's were: the crate
`agentspace-guard`, with its public types and a `run` that returns `not built yet`, pinned on
E0.08-T03-S01's card, so the operator story can call it while it is built.

**Carried in, with `references`:** #0163, #0164, #0166 and #0167 (D61); #0064 (D59); #0043 (D58),
OBE once T06 is merged.

**Why this shape:**
- **The same order as v0.07's, for the same reasons** (D55): specs before code, libraries before
  programs, the whole suite before any instance starts.
- **`box/system` and the template are built in T01**, as v0.07's template and bootstrap were: they
  are shell and CloudFormation, specified by D57, D58 and D61's pins, and the operator embeds
  `box/system`, so T03's builds carry them.
- **The live checks are two turns, not one.** T05's laptop-away check stops the keeper, which
  would stop the guard for any check running beside it, so it runs before T06's checks, whose
  two stories run at once on different hosts.
- **Instance hours:** about 1.75 in T05 and 3.75 in T06, about 5.5 of the 6 Terry approved (D62).
  A turn that would pass the 6 asks him first.

## v0.09: size classes

**D64 to D70 settle v0.09's (#0049) design calls before its stories are laid out**, as
[handoff-v0.08.md](handoff-v0.08.md#the-scope-ahead-v009) asks. Each is the architect's,
2026-11-23, unless it says otherwise. They bind the stories, which carry each one into the spec
that owns the fact in the same pull request that builds it (D34). v0.09 MUST NOT be broken into
stories until Terry says so (AGENTS.md, "The board"). Three calls are his, and are put to him the
same day: Q11 to Q13. **Terry said yes to all three the same day** ("Yes, all three"): the four
sizes at their prices, D70's 4 instance hours, the transcript fetch, and his per-agent rule kept
through v0.09.

**What none of them changes:** contract v0. Supporting a size that v0 answered `unsatisfiable` is
not a new version (lease-api.md, Versioning), and `no_capacity` is already one of its reasons. No
schema, field or code is added, and nothing in a lease names an instance type (D1).

**Not carried:** #0195, a killed client's Session Manager session holding the idle check off. It is
about the horizon power-off, not about a host's size, and one of its fixes is Terry's account
setting; it waits for a release of its own. #0109 and #0082 wait for the next measured turn, which
v0.09 does not run (D64).

### D64. v0.06-T01's whole-turn record meets #0049's re-measurement for the host; the per-agent split is owed to v0.10

**Decided, architect, 2026-11-23.** It settles whether v0.09 measures a turn again before it sets
a size (D33; #0049's entry requirement, Terry, 2026-11-18).
- **The requirement is met for what v0.09 sets, and v0.09 runs no measured turn.**
  [runs/e0.06-t02-s08.md](runs/e0.06-t02-s08.md) is the record of v0.06-T01, which is the turn
  Terry named on 2026-11-19 in place of a v0.05 turn (D33). It meets every part of the
  requirement but one:
  - **A turn that compiles:** seven Rust stories, each building and testing its own crate on the
    box.
  - **Two or more stories in parallel:** seven, with the dev lead in all seven worktrees by its
    seventh minute.
  - **v0.03's tools, rerun:** `agentspace-sample` at 2 seconds and `agentspace-footprint` over its
    record.
  - **The whole turn, with no gap:** 3,148 consecutive samples from 3 minutes before the dev lead's
    session to 30 seconds after the last merge, ending in the sampler's own end line. v0.05-T01's
    covered 7 of 31 minutes (#0109).
  - **Not cut off by the box:** the host peaked at 4.10 of 16 cores and 3.81 of 61.61 GiB, where
    v0.05-T01's 3.83 cores was 96% of its box.
  - **Missing: the per-agent division** (D33), which needs the turn's transcripts, and with it the
    measured count of agents at once.
- **v0.09 sets size classes, and they need the host's load, not a per-agent figure.** A class is a
  CPU and memory pair (D65), and the record says what a whole compiling turn of 15 planned agents
  asked of a host. Multiplying a plan's peak concurrency by a per-agent footprint is how v0.10
  (#0050) picks a size (D27), and v0.09 picks one by Terry's rule instead (D69). **So the
  per-agent split is owed before v0.10 sets its mapping, not before v0.09.**
- **The split can be had without a new turn.** The dev lead was a desktop-app session on the box
  (`v0.06-T01 dev lead (on the box, measured)`, remote, created at 2:44:31pm Pacific on
  2026-11-19, the second before the record first sees its session). Its transcript and its
  subagents' are therefore on the box's disk, under `~/.claude/projects/-home-ubuntu-agentspace/`,
  unless Claude Code's transcript clean-up has removed them, which by its default keeps 30 days,
  so to about 2026-12-19. Fetching them and running `agentspace-agents` over the committed record
  (footprint.md, "Measure a new project", steps 3 to 5) gives D33's figures. **That fetch starts the
  box, so it needs Terry's yes for its instance hours (Q12).** If he says no, or they are gone,
  v0.10 measures a turn of its own, and #0109 is met first.
- **The split was had the same day** (Terry's yes, Q12): the transcripts were fetched with the
  box running about two minutes, and `agentspace-agents` joined them to the record (the run
  record's "Added 2026-11-23"). The measured subagent peak is 14, so 15 working agents, D27's count
  from the plans exactly. **The turn's CPU peak, 4.02 cores, was the dev lead alone running a
  `cargo clippy -j 4`**, so a per-agent peak is a build's parallelism and MUST NOT be multiplied by
  the agents at once; the per-agent p95 is 0.10 cores and 12.50 MiB. v0.10 starts from these, and
  needs no measured turn of its own for agentspace.
- **What the record cannot stand for:** one project, agentspace, whose crates are small; another
  project's builds MAY load a host differently. v0.10's plan sizing is per project (D27), and
  footprint.md's "Measure a new project" is how a project measures its own.
- **#0082's tripwire did not trip** on the record (9.6% against its 10% line, on v0.04-T01's
  baseline), so #0082 is not pulled forward. **#0109 is not needed by v0.09**, since no sampler runs
  in it; it goes before the next measured turn. This changes D61's "Not carried" line.

### D65. Four sizes, `s` to `xl`, on the `m9g` ladder; `m` stays the default and today's host

**Decided, architect, 2026-11-23**, on the footprint (D64) and Terry's M class (D39). The price of
each is Terry's to accept (Q11).
- **A size is a floor of vCPUs and memory**, stated in lease-api.md in those words alone, never
  as an instance type (D1). **The EC2 driver supports these four for `linux/arm64`**, the one
  platform v0 supports (D13), and maps each to one instance type, which driver.md's EC2 driver
  section owns:

  | Size | vCPUs | Memory | EC2 driver's type | On-demand, per hour |
  |---|---|---|---|---|
  | `s` | 2 | 8 GiB | `m9g.large` | $0.0978 |
  | `m` | 4 | 16 GiB | `m9g.xlarge` | $0.1957 |
  | `l` | 8 | 32 GiB | `m9g.2xlarge` | $0.3914 |
  | `xl` | 16 | 64 GiB | `m9g.4xlarge` | $0.7827 |

  The vCPUs, memory and offering in the stacks' region were read with `ec2 describe-instance-types`
  and `describe-instance-type-offerings`, and the prices from AWS's price list (Linux, shared
  tenancy, on demand), on 2026-11-23. The root volume is unchanged by size, and so is its cost.
- **`m` stays the default, and stays `m9g.xlarge`**, the type both hosts have today, so every lease
  made as before v0.09 asks for the host it already has.
- **Any other well-formed size stays `unsatisfiable` with `unsupported_size`**, its message naming
  the four (`v0 supports sizes s, m, l and xl`). The reasons' rules are unchanged (D5).
- **Why these four:** v0.04-T01, a spec-writing turn of 8 agents, peaked at 0.51 cores and 1.09 GiB,
  inside `s`. v0.06-T01, a compiling turn of 15 planned agents, peaked at 4.10 cores and 3.81 GiB:
  over `m`'s 4 cores in 3 of its 3,147 intervals, inside `l` with about twice its CPU to spare, and
  inside `xl` by Terry's per-agent rule (D69). Each step doubles both, as the `m9g` family does, at
  M class's 4 GiB per vCPU (D39). A size above `xl` is not supported until a measured turn needs
  one, and a GPU size stays on the roadmap's later list.

### D66. A host is resized only while no ready lease holds it; a lease its host already meets is placed at once

**Decided, architect, 2026-11-23.** It decides what #0049's done-when leaves open: a lease asking
for a different size while the host runs.
- **A host meets a lease when its size is at least the size asked for**, in both vCPUs and memory.
  A lease placed on a larger host is met, not swapped (D5): D5 forbids handing back less than was
  asked, and the lease's `size` stays the size asked for, so nothing in it shows the difference
  (D1).
- **Place decides by the project's live `ready` leases** (D56's words), read from the lease state
  file as the guard reads them, with no AWS call (D48), and by the host's size as the host registry
  records it (D67):

  | The project's host | Place answers |
  |---|---|
  | `ready`, at exactly the size asked for | `ready`, as today |
  | `ready`, larger than the size asked for, and another live `ready` lease of the project exists | `ready`, on the larger host |
  | `ready`, at any other size, and no other live `ready` lease of the project exists | `pending`, and a resizing run is started (D68) |
  | `ready`, smaller than the size asked for, and another live `ready` lease of the project exists | `unsatisfiable`, with one reason, `no_capacity` |
  | `creating` or `failed`, or no record | As D48 and D54 say; the provisioning run makes the host at the size of the largest `pending` lease (D68) |

- **A larger host is resized down when nothing holds it** (the third row). A host left at `xl` by
  one turn would otherwise cost four times `m`'s rate for every lease after it, and the cost is a
  few minutes `pending` for the lease that finds it.
- **The `no_capacity` message names the project, the size asked for and what the caller can do**,
  never the backend, and SHOULD read:
  `No capacity can be found for project '<project>' at size '<size>': its host is serving other leases at a smaller size. Release them, or ask for a size they fit.`
  It answers at once and changes nothing. A later `create` is judged afresh, as lease-api.md
  already says for `no_capacity`.
- **Why refuse rather than wait or resize now:**
  - **Resizing a running host stops it** (D39, D40), which ends every session on it: another
    lease's agents would lose their work in the middle of a turn, with nothing in their lease to
    say why.
  - **Waiting** would leave a lease `pending` until every other lease of the project ends, which
    can be the 24-hour maximum TTL (D24), with nothing to tell the caller it waits on others.
  - **The other leases are the same project's**, so the caller, or the architect that runs the
    project's turns, can release them and ask again. The refusal says so at once.
- **A lease that is only `pending` does not hold a host's size**, since it has no session there; an
  expired or released lease holds nothing. A host in its grace after its last lease (D56) is
  resized like a stopped one.
- **Changed for v0.09-T01 (architect, 2026-11-23, on the dev lead's pins P2 and P3):**
  - **While the project has any `pending` lease, place answers every new lease of it `pending`**,
    whatever the table above says, and the run places it. Otherwise a lease placed `ready` on the
    old size while a resize run is due or running would be cut when that run stops the host.
  - **A `ready` record with no `instance_type` answers `pending` and starts a run, whatever other
    leases hold the host**, never `no_capacity`. The run records the type without resizing when a
    live `ready` lease holds the host (D68's change below), and then places or refuses the lease by
    the type it found. So no lease is refused across the install only because its host's type was
    not yet recorded.
  - **The record a missing `hosts.json` implies, the box's (D45), has `instance_type`
    `m9g.xlarge`**, D65's `m`, which is the template's default and the box's type today. A laptop
    with no registry places a default lease as before v0.09; the box's first deploy or run records
    its real type.

### D67. The stack holds a host's instance type, the host registry records it, and the size-to-type map is the EC2 driver's

**Decided, architect, 2026-11-23.** It settles where a host's size lives.
- **The stack's `InstanceType` parameter is the instance's type, and it changes only through the
  stack** (D39, D40), by `deploy -Host <project> -InstanceType <type>`, as every host but the box
  changes today (operator.md, deploy).
- **The host registry records each host's type, as `instance_type`**, written by every run or
  `deploy` that sets it, from the stack's parameter as `describe-stacks` reads it back after the
  deploy succeeds. Place reads it there, with no AWS call (D48). The registry still never records
  whether a host runs (D45). state.md owns the member and its schema.
- **A record with no `instance_type` meets no size.** Both hosts' records predate v0.09, so each
  project's first lease after the install with no other live `ready` lease answers `pending`, and its
  run sets the type and records it. It costs each host one run, once, and no file of Terry's is
  edited for it.
- **The map from size to type is the EC2 driver's alone** (D65's table), in driver.md. lease-api.md
  names sizes by vCPUs and memory, and the operator takes an instance type and knows no size.
- **The box is sized like every other host.** `deploy -Host swimcoach -InstanceType <type>` is
  allowed, and a plain `deploy` of the box passes `InstanceType` at its previous value.
  **`stack-parameters.json`'s `InstanceType` is no longer read**, so the box's next plain `deploy`
  cannot put back a size a lease set; the member is left in Terry's file, untouched, and README and
  operator.md say it is ignored. This replaces operator.md's P13 rule (Q31 there) that the box is
  sized through that file.
- **A person resizing a host by hand MAY still use `deploy -Host <project> -InstanceType <type>`**,
  under a lease as D56 requires, and the deploy records the type, so the registry and the stack
  agree after either path.

### D68. A resize is the provisioning run, given the instance type; the guard and the idle check meet it as they meet a provisioning run

**Decided, architect, 2026-11-23.** It settles how a resize runs, and how the guard (D56) and the
idle check (D57) meet a stop and start that no lease's end explains.
- **The resizing run is D48's provisioning run**, `agentspace-operator provision -Host <project>`,
  started detached by place and by `get` under D48's 5-minute rule, **given `-InstanceType <type>`
  for its deploy step**, where today provision never passes one. Its type is the size of the
  project's largest `pending` lease, read when the run reaches its deploy step, through D65's map.
  It then places every `pending` lease the host now meets (D48's `place_pending`).
- **A `pending` lease the resized host does not meet**, because a larger one arrived after the run
  read the sizes, is placed by the next run when no live `ready` lease holds the host, and is
  answered `unsatisfiable` with D66's `no_capacity` when one does. lease-api.md's transition from
  `pending` to `unsatisfiable` already exists.
- **A run never takes a `ready` host out of `ready`** (D48): a resize keeps the record `ready`, and a
  failed deploy leaves the type it had, logged in `provision.log`, with the leases `pending` for the
  next run.
- **The guard MUST NOT stop a host while its run holds `provision.lock`** (D56), which covers a
  resize. Outside the lock, the `pending` lease explains the host: D56's horizon for a project with
  only `pending` leases is the reading's time, so neither the guard nor the horizon power-off stops
  a host waiting on its resize.
- **The stop and start a resize makes is the stack's, not the guard's.** A boot clears
  `/run/agentspace/lease-horizon` (D57), and the next guard run sends it again; the idle check keeps
  D11's 60-minute rule until then. A host that the run leaves running and that no lease ends up
  holding is stopped by the guard a grace later, as any unexplained host is.
- **Whether a resize on a `ready` host runs provision's later steps** (start, `sync`, `bootstrap`),
  each safe to repeat (D48), or only its deploy and start, is the spec turn's to rule after a probe
  of their times on a `ready` host.
- **Changed for v0.09-T01 (architect, 2026-11-23, on the dev lead's pins P1 and P2):**
  - **The run's deploy step resizes only when no live `ready` lease holds the host at that
    moment**, read from the lease state file then. When one does, it passes no `InstanceType`, so
    the stack's change set holds no resize and the host is not stopped, and it records the type
    it reads back.
  - **`place_pending` answers the refusal**: at the run's end, under the lease state file's lock, a
    `pending` lease the host does not meet becomes `unsatisfiable` with D66's `no_capacity` when a
    live `ready` lease holds the host, and stays `pending` for the next run when none does.
    driver.md's rule that `place_pending` never answers `unsatisfiable` gains this one exception.
- **Changed when v0.09 was laid out (architect, 2026-11-23, D71):** `provision`'s command line is
  unchanged, and the run finds its type itself at its deploy step, from the lease state file
  through `resize_target`; and a resize runs provision's steps entire, with no probe first.

### D69. D39 ends with v0.09: the architect picks a turn's size by Terry's per-agent rule, and asks for it through the lease

**Decided, architect, 2026-11-23**, from D39's own status. Q13 asks Terry whether his rule stays.
- **From v0.09's install on, no host is sized by hand for a turn.** The architect gives the turn's
  lease `--size`, and the lease resizes the host (D66, D68). D39 becomes Replaced by D65 to D68
  then, and AGENTS.md's "Sizing the box before it runs agents" is rewritten in the same piece of
  work as that install.
- **The size is picked by Terry's rule from D39, unchanged:** the smallest size whose vCPUs are at
  least the turn's agents at once, counted as D27 counts them, which gives each agent a vCPU and
  4 GiB. So up to 2 agents is `s`, 4 `m`, 8 `l` and 16 `xl`.
- **A turn of more than 16 agents at once on a host has no size**, and its plan is split or brought
  to Terry before it starts. Every turn run on a host so far has fit: v0.06-T01's 15 was the most.
- **The measured load is well under the rule.** v0.06-T01's 15 planned agents peaked at 4.10
  cores, a quarter of the 16 vCPUs the rule gave them, and fit `l`. **Loosening the rule is Terry's
  call (Q13)**, since it is his order and it trades cost against headroom; until he says otherwise
  it stands. v0.10 replaces the hand pick with D27's product of peak concurrency and footprint.

### D70. v0.09's done-when is shown by live checks on agentspace's host, with Terry's yes for their instance hours

**Decided, architect, 2026-11-23** (D19, as D53 and D62). Terry's yes for the hours is Q11.
- **No stand-in can show a host's type change, so the stories' done-whens name these live checks,
  on agentspace's host:**
  - **Bigger:** with the host stopped and no live lease, `create --size l` answers `pending`, becomes
    `ready`, and `nproc` over its alias prints 8. The lease document names only the size (D1).
  - **Met at once:** with that lease live, `create --size m` answers `ready`, on the same host.
  - **Refused:** with that lease live, `create --size xl` answers `unsatisfiable` with `no_capacity`
    and D66's message, and the host's type and state are unchanged.
  - **Smaller:** once every lease is released, `create` with the default size answers `pending`,
    and the host comes back as `m`, four vCPUs.
  - **Unsupported:** `create --size xxl` answers `unsatisfiable` with `unsupported_size`, makes no
    AWS call and starts no run.
  - **The first lease after the install:** the host's record gains `instance_type` by its run
    (D67).
  - **The guard:** `guard.log` and `status -All` show no stop of the host during a resize, and the
    host is `stopped` within D56's bound after the last release.
- **The box is not resized live.** Its leases go through the same code; its first lease after the
  install records its type (D67) at `m`, which changes nothing on it, and that is shown by its
  registry record and `status -Host swimcoach`.
- **The hours asked for:** up to 4 instance hours at `l` or smaller, at most about $1.57 at `l`'s
  rate, most of them graces waited out. A live turn that would pass them asks Terry again first.
  Every host is confirmed `stopped` at the close.
- **conformance.md's coverage table is read for every "None" these change** (architect.md,
  Breaking an epic into stories, step 5), and each gets a case or one of these checks.

### D71. v0.09 is twelve stories in five turns: specs, foundations, programs, conformance, live

**Decided, architect, 2026-11-23, when Terry had v0.09 (#0049) broken into stories:** "Lay out
v0.09", with "Make sure to do a staffing plan and report max subagents per turn to me". It lays
v0.09 out as D63 laid out v0.08, on the designs of D64 to D70, and the stories' cards carry the
detail, each with its staffing plan (D31, D63: as many subagents as can be used effectively).

**Two changes to D68, made here:**
- **`provision`'s command line is unchanged; the run finds its own instance type.** At its deploy
  step it reads the lease state file and passes `-InstanceType`'s value to the stack as
  `resize_target` gives it (the pins below), or none when it gives none. The driver starts the run
  as it does today, with no new argument, so a run started by `get` and one started by `create`
  agree.
- **A resize runs provision's steps entire**, as D48's run on a `ready` host already does, each
  safe to repeat. The faster path D68 left to a probe is not designed until a live check shows that
  the time matters; D70's checks record it.

**The pinned names.** Every story uses these, and the specs written in T01 own their exact forms
(lease-api.md for sizes and messages, driver.md for the EC2 driver's map and placing, state.md for
the registry, operator.md for `deploy`, `provision` and the help texts):

| Name | Pinned as |
|---|---|
| The sizes | `s`, `m`, `l`, `xl`, smallest first; the EC2 driver's `pub const SIZES: [&str; 4]` in `agentspace-driver-ec2` |
| The map | `pub fn instance_type_for(size: &str) -> Option<&'static str>`, by D65's table; `None` for any other size |
| Meeting a size | `pub fn meets(instance_type: &str, size: &str) -> bool`: true when the type is in the map at or after the size's place in `SIZES`. A type the map does not hold meets no size |
| The resize target | `pub fn resize_target(leases: &[agentspace_state::Record], project: &str) -> Option<&'static str>`: the type of the largest size among the project's `pending` records whose size is in `SIZES`, or `None` |
| The registry's type | `hosts.json`'s record member `instance_type`, a string, optional; `agentspace_hosts::Host.instance_type: Option<String>` |
| Who writes it | Every `deploy` that names or implies a host, `provision`'s deploy step included, after `cloudformation deploy` succeeds, from `describe-stacks`' `InstanceType` parameter, under `hosts.lock` |
| The box's sizing | `deploy -InstanceType <type>` is allowed for every host, the box (`-Host swimcoach`, or no `-Host`) included; `stack-parameters.json`'s `InstanceType` is not passed and not read |
| Messages | `Size '<size>' is not supported; v0 supports sizes s, m, l and xl.`, and D66's `no_capacity` message, both byte for byte in lease-api.md and driver.md |
| The unsupported size in conformance | `xxl`, and `XXL` where a case needs upper case, in place of every `l` and `xl` that a case used as a size no backend has |
| Branches | `v0.09/e0.09-tnn-smm` (D29) |

**The turns.** Stories in a turn run in parallel, each in its own worktree; a turn starts when every
pull request of the turn before is merged (D16). T01 to T04 run on the laptop and start no
instance; T05 starts agentspace's host. Every subagent runs on the laptop, so no host is sized for
a turn (D69 applies to none of them). The peak is the turn's subagents at once, its dev lead not
counted.

| Turn | Stories | What | Peak | Budget |
|---|---|---|---|---|
| T01 | S01 to S03 | **The specs, and the conformance sweep.** lease-api.md and driver.md; state.md, operator.md, README and the registry's schema; every conformance case's unsupported size moved to `xxl`, passing against the installed programs | 8 | 3 h |
| T02 | S01 to S03 | **The foundations.** `agentspace-hosts`' `instance_type`; `agentspace-driver-ec2`'s sizes, map, `meets`, `resize_target` and reasons; the operator's `-InstanceType` and both programs' help texts, with the conformance help copies | 7 | 2.5 h |
| T03 | S01, S02 | **The programs.** The EC2 driver's place and `place_pending` by D66 and D68; `agentspace-sync`'s deploy and provision: the target, recording the type, and the box's sizing | 6 | 3 h |
| T04 | S01 to S03 | **Conformance.** The lease CLI's cases; the operator's cases; the whole suite, the crates and the box tests against `main`'s build, with conformance.md | 7 | 3 h |
| T05 | S01 | **The install and the live checks** on agentspace's host, D70's list | 2 | 3 h |

**The release's budget is 14.5 hours**, the sum of its turns'. **Its instance hours are T05's
alone, up to the 4 Terry said yes to (Q11).**

**Why this shape:**
- **The same order as v0.07's and v0.08's** (D55, D63): specs before code, libraries before
  programs, the whole suite before any instance starts.
- **The conformance sweep goes first, in T01.** About a dozen lease CLI cases use `l` or `xl` as a
  size no backend has, to get an `unsatisfiable` lease with no CA and no host. v0.09 makes both
  supported, so every one of them would fail on T03's build. Moved to `xxl` against today's
  programs, they pass before and after, and no T03 story inherits a red suite.
- **T02's libraries are pinned by the table above**, so its three stories need nothing from each
  other, and T03's two need only T02's merged code.
- **One live turn, on one host.** D70's checks are sequential on agentspace's host, and the box is
  not resized live.

## Keys

### D17. The CA key lives on the laptop, owner-only and without a passphrase

**Status: MVP only; replaced by card #0011, and its location by D21.**

**MVP only, 2026-11-16 (story E2.03).** **Replaced by card #0011**, which settles Q1 when the API
is hosted and signing leaves the laptop. **Its location was replaced by D21:** story E2.09 moved
the key from `%LOCALAPPDATA%\agentspace\ca` (in fact the Claude app's package cache) to
`%USERPROFILE%\.agentspace\ca`, keeping its fingerprint `SHA256:G05h...`; everything else here
(owner-only, no passphrase, never committed, never overwritten) stands.

- **The CA's private key MUST live at `%USERPROFILE%\.agentspace\ca\agentspace_ca`** (D21; until
  E2.09 it was `%LOCALAPPDATA%\agentspace\ca\agentspace_ca`), with its public key beside it as
  `agentspace_ca.pub`. Both files MUST stay out of the repo and out of any folder that OneDrive or
  another service syncs. `agentspace.ps1 ca` creates them once, and refuses to run when that folder's path is inside OneDrive or the repo. It MUST NOT be
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
  - **From v0.07, `sync` cannot reach a host other than the box after a replacement** (architect,
    2026-11-20, for v0.07-T01, on E0.07-T01-S02's review): its operator certificate comes from the
    new CA, which that host does not trust yet. The box is reached as before, through the user's
    own key. A person connects to each other host with their own `id_ed25519`, which every host's
    first boot installs (`SshPublicKey`), through that host's ProxyCommand, and runs `sync`'s CA
    step; operator.md gives the steps. Installing the new public key through Session Manager's Run
    Command, which needs no ssh, is the candidate fix, an open question there.

**Why:**
- **Superseded by D21.** `%LOCALAPPDATA%` was chosen because it never roams, cannot be redirected
  to OneDrive, and agentspace already kept laptop state there. Inside the MSIX-packaged Claude
  app, writes there land in the package cache instead, so E2.09 moved the CA to
  `%USERPROFILE%\.agentspace\ca`, which is not redirected and not inside OneDrive; `ca` still
  refuses a OneDrive path. Unlike `AppData\Local`, the profile root would roam under a roaming
  profile; Terry's laptop has none (it is not domain-joined). `~/.ssh` is not used because it
  holds identities and gets copied around.
- **Disabling inheritance is load-bearing:** on Terry's laptop, `%USERPROFILE%` passes inherited
  access to SYSTEM and Administrators (checked in E2.09). Until E2.09, `%LOCALAPPDATA%` also passed
  inherited access to `CodexSandboxUsers` and to an AppContainer capability.
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
| **Q1** | **Later, not blocking:** card #0011 is after 1.0 (roadmap.md), and contract v0 keeps both on the laptop (D8, D17, D21). Where do the CA key and lease state live once the API is hosted? A KMS-backed signer? DynamoDB? | #0011 |
| **Q2** | **Later, not blocking:** card #0009 is after 1.0 (roadmap.md), and contract v0 issues no per-project credential. Where do per-project credentials (Claude Code, Codex, gh) live, so a fresh host or container needs no sign-in? Needs Terry. | #0009 |
| **Q3** | **Later, not blocking:** card #0008 is after 1.0 (roadmap.md), and contract v0's board access is D12's. How do many leases, and later many project users, reach the laptop's localswim board? | #0008 |
| **Q4** | **Later, not blocking:** card #0016 is after 1.0 (roadmap.md), and contract v0 has one driver, whose reachability is D9's. One overlay network across all backends (WireGuard, Tailscale, Cloudflare Tunnel), or a reachability path per driver? | #0016 |
| **Q5** | ~~Is the lease CLI PowerShell or Go?~~ PowerShell 7 for the MVP (D18), then Rust (D28). ~~Its default and maximum TTL?~~ 8 hours by default, 24 hours at most, 1 minute at least (D24). | #0001 |
| **Q6** | **Closed by D54 (Terry, 2026-11-20).** ~~May any project's first lease make a host, or only a project Terry has named?~~ Only a named one, listed in `projects.json`; another project's lease is `unsatisfiable` with `no_capacity`. | Closed, D54 |
| **Q7** | **Closed (Terry, 2026-11-20).** ~~Does his standing order on the box's disk cover every host?~~ Yes, each host on its own: AGENTS.md, "Growing the box's disk", and D46. | Closed |
| **Q8** | **Closed by D56 (Terry, 2026-11-21).** ~~How long is the grace after a host's last live lease ends before it is stopped?~~ 15 minutes, so a host stops within 25 minutes of its last lease's end. | Closed, D56 |
| **Q9** | **Closed by D56 (Terry, 2026-11-21).** ~~Does the guard cover the box too?~~ Yes; direct work on it is done under a lease. | Closed, D56 |
| **Q10** | **Closed by D60 (Terry, 2026-11-21).** ~~Is an unexplained host pushed to his phone too?~~ No: `guard.log` and `status -All` alone. | Closed, D60 |
| **Q11** | **Closed (Terry, 2026-11-23: "Yes, all three").** ~~Does he accept D65's four sizes at their prices ($0.10, $0.20, $0.39 and $0.78 an hour for `s` to `xl`), and say yes to D70's live checks, up to 4 instance hours at `l` or smaller, about $1.57 at most?~~ Yes to both. | Closed, D65, D70 |
| **Q12** | **Closed (Terry, 2026-11-23: "Yes, all three").** ~~May the architect start the box under a swimcoach lease, for about half an hour (about $0.10), to fetch v0.06-T01's transcripts to the laptop, outside the repository, before Claude Code's clean-up can remove them, so the per-agent split is had without a new measured turn (D64)?~~ Yes. | Closed, D64 |
| **Q13** | **Closed (Terry, 2026-11-23: "Yes, all three").** ~~Does his per-agent rule from D39, a vCPU and 4 GiB for each agent at once, stay the way the architect picks a turn's size through v0.09, though v0.06-T01's 15 agents peaked at a quarter of it? The architect recommends it stays until v0.10 replaces it with measurement (D69).~~ It stays through v0.09. | Closed, D69 |

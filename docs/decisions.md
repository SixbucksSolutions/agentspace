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
  outlives the lease. Under D25 the checkout lives on the project's own host, and v0.7 (#0047)
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

**MVP only.** `agentspace lease ...` runs on the laptop, prints JSON, and keeps lease state in a
local file. There is no server to run.

**Why:** today's callers are all projects on the laptop, which already call `localswim` the same
way.

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

## What the MVP leaves alone

### D10. Releasing a lease does not revoke its certificate

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

### D11. Today's idle stop stays

**MVP only.** The box still powers off after 60 minutes with no SSH or Session Manager session and
no CPU work. An agent holding a lease but doing nothing can still have the box stopped under it;
its next connection starts it again. **Replaced by v0.8 (#0048)**, which stops a project's host
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

**MVP only.** The first `ssh` connection still binds the localswim board's reverse forward for the
whole box, and every lease uses it, because every lease is `ubuntu`. **Replaced by card #0008.**
Per-project users (#0004), which it once had to land with, are OBE under D25: each project's host
has its own forward, so #0008 matters once two leases of one project share a host. v0.2 (#0045)
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

## Defaults

### D13. arm64 is the default platform

**Decided.** When a request names no platform, it gets `linux/arm64`, because Graviton costs less
for the same agent work. `linux/amd64` is the planned second platform (v0.11, #0051, which folded
in card #0013), not a fallback.

## How the work is run

### D14. Roadmap horizons, epics before stories

**Decided, Terry, 2026-11-16.** The roadmap runs from the MVP through Next and Later to Someday.
**Its horizons were replaced by D26 (2026-11-17):** releases v0.2 to v1.0, one epic each, then
After 1.0 and Someday. The rules on epics and stories below stand, except that a release's
stories are named `X.Y.nn` (D29).
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

**Decided, Terry, 2026-11-16.** Models are named `Model/effort`, with no version, meaning the
latest.

- **Each story gets one implementer subagent, then one independent reviewer subagent.** A story
  of under 3 hours MUST NOT be split across implementers: the merge cost outweighs the time saved.
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

**MVP only. Replaced by D28 (Terry, 2026-11-17):** agentspace is Rust by v0.6, and PowerShell was
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
  v0.4, ahead of the Rust rewrite in v0.5 and v0.6; D28). In the MVP,
  `agentspace lease` gets its JSON schemas with epic #0001, since they cost little while the
  contract is being written. It SHOULD also have `--help` at every level.

### D21. Laptop state lives in %USERPROFILE%\.agentspace, never in AppData

**Decided, architect, 2026-11-17 (found during E2.02).** Everything agentspace keeps on the laptop
(the CA, lease keys and certificates, the localswim token hash) MUST live under
`%USERPROFILE%\.agentspace`. It MUST NOT live under `%LOCALAPPDATA%` or `%APPDATA%`. Story E2.09
moves it.

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
  holds no live lease and started by its next one (v0.8, #0048). Its disk keeps the project's
  checkout, sign-ins and caches.
- **Its size comes from the lease:** a stopped host is resized to the size asked for (v0.9,
  #0049), and a size can be chosen from a plan (v0.10, #0050; D27).
- **Leases of different projects MUST NOT share a host.**
- **D1 stands:** nothing in a lease says which host serves it, or that the host is dedicated.

**Why a persistent host per project rather than a fresh host per lease:**
- **A fresh host starts empty:** no Claude Code, Codex or gh sign-ins, no checkout (D3's E3.03
  finding) and cold caches. Automating sign-ins (#0009, Q2) is the hardest problem left open, and
  this choice keeps it off the path to 1.0: Terry signs in once per project host instead (v0.7,
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

### D26. Releases from v0.1 to v1.0, one epic each

**Decided, Terry, 2026-11-17:** "I would rather do a large number of smaller scope steps between
where we are and 1.0." The MVP is v0.1. Each later release is one epic, titled `vX.Y: title`, and
the releases are taken in order. Release numbers count past 9: v0.10 follows v0.9.

| Release | Epic | What |
|---|---|---|
| v0.2 | #0045 | Unattended leases: #0042, #0044, #0028 |
| v0.3 | #0046 | Measure what an agent uses |
| v0.4 | #0030 | Clean-room spec package for the Rust rewrite (D20, D28) |
| v0.5 | #0054 | The lease CLI in Rust |
| v0.6 | #0055 | The operator in Rust; PowerShell retired |
| v0.7 | #0047 | A host per project (D25) |
| v0.8 | #0048 | Host lifecycle and cost guard |
| v0.9 | #0049 | Size classes |
| v0.10 | #0050 | Plan-sized leases; contract v1 (D27) |
| v0.11 | #0051 | `linux/amd64` platform |
| v0.12 | #0052 | Plan from the board |
| v1.0 | #0053 | A real project runs an epic on a plan-sized host |

- **A release number is not the contract version.** The lease contract stays v0 until a release
  changes an output (lease-api.md, "Versioning"); v0.10 is the first that does.
- **D14's rules on epics and stories stand.** A release epic MUST NOT be broken into stories
  until Terry says so, and its "Done when" is checked by a real run, not only by tests.
- **Like the MVP, 1.0 is done when a real project works through it** (#0053), not when the code
  works.

### D27. A host is sized by concurrency, never by model or effort

**Direction, architect, 2026-11-17; measured in v0.3 (#0046), applied in v0.9 (#0049) and v0.10
(#0050).**
- **A host's size MUST be chosen from the plan's peak concurrency**, meaning the most agents
  working at once in any one turn, times a measured per-agent footprint for the project.
- **A plan MAY carry each subagent's model and effort, and agentspace records them, but they MUST
  NOT change the size.** They MAY feed the TTL, since they predict how long a turn takes.
- **The footprint MUST come from measurement** (v0.3), never from an estimate.

**Why:** the model runs at Anthropic, not on the host. What loads the host is the agents' tool
calls: builds, test suites and browsers, run by subagents working at the same time. Under D15 a
story's implementer and reviewer run one after the other, so a turn's peak is about its number of
stories plus its dev lead.

### D28. agentspace is Rust by v0.6; PowerShell was the prototype's

**Decided, Terry, 2026-11-17:** cut over to Rust (first choice) or Go ("pretty far back second
place but would accept if much simpler than Rust") by 1.0. PowerShell was permitted only because
the MVP was a very lean prototype. **Replaces D18.**
- **The rewrite is the clean-room drop of D19 and D20.** v0.4 (#0030) writes the specs, and the
  Rust code MUST be built from them, never from the PowerShell code.
- **The lease CLI MUST be Rust from v0.5 (#0054), and the operator (`agentspace.ps1`,
  `ssh-proxy.cmd`) from v0.6 (#0055).** From v0.7 on, new product code MUST be Rust.
- **Not product, so not ported:**
  - The architect's process tools (`scripts/watchdog.ps1`, `scripts/report-milestones.ps1`) MAY
    stay PowerShell.
  - The shell that runs on a host (`box/`) stays shell. It configures Ubuntu, and Rust would add a
    build per platform without making it clearer.
- **Open defects against the PowerShell code** (#0027, #0029, #0034, #0041) MUST be carried into
  v0.4's specs as requirements, and each is OBE once the Rust version meets it.

**Why Rust and not Go:** Go is not much simpler for this work, which is what Terry's second place
required.
- Both have mature libraries for everything agentspace does: SSH keys and certificates, JSON Schema
  validation, the AWS SDK and file locking.
- Both shell out to the Session Manager plugin, as the prototype does.
- Both build single native binaries for Windows and Linux.
- Both toolchains are already on the laptop (cargo 1.99; Go 1.27, which builds localswim).

**Why at v0.4, and not just before 1.0:**
- **Every release from v0.7 on rewrites the driver** (hosts per project, lifecycle, sizes, plans).
  Built in PowerShell first, each would be written twice.
- **The PowerShell to port is the smallest it will ever be**, about 110 KB of product code.
- **v0.2 and v0.3 go first** because they are small, mostly on the box, and fix what blocks real
  use today.

### D29. A release's stories are `X.Y.nn`, its turns `vX.Y-Tn`

**Decided, Terry, 2026-11-17:** a release epic's stories are titled `X.Y.nn: title`, where `X.Y` is
the release and `nn` is the story's 1-based number within it, zero-padded to two digits, as in
`0.2.01: sshd drops a dead client's session`. **This replaces D14's `EN.nn` for release epics.**
MVP stories keep their `EN.nn` names.
- **Docs, pull requests and conversation MUST cite a release story the same way:** `0.2.01`.
- **Its branch is `vX.Y/X.Y.nn`**, as in `v0.2/0.2.01`.
- **A release's turns are `vX.Y-T1`, `vX.Y-T2` and so on**, so they never collide with the MVP's
  T1 to T8. Each turn is a swimcoach milestone of that name, and the release itself is one more,
  `vX.Y`, which counts all of its stories and never the epic.
- **Every other rule in D14 stands:** each story is under 3 hours of active work, it starts in
  Ready For Work as a child of its epic, and it carries its estimate and dependencies.

**The Terry-preset board does not let Bot move a card from Backlog to In Progress** (localswim's
README: that takes a `policy grant`). A release epic leaves Backlog by Terry's hand, so starting
a release is always his click. The architect MUST NOT grant Bot that move on its own.

### D30. A keeper on the laptop holds the board's forwards

**Direction, architect, 2026-11-17; built in v0.2 by 0.2.03 and 0.2.04, shown in 0.2.05.**
Replaces D12's hand-run holder (card #0042).
- **A keeper on the laptop holds the board's forwards for as long as a lease is ready.** Each pass
  reads lease state read-only. With no holder running and a lease ready, it starts one, `ssh.exe
  -N -o ExitOnForwardFailure=yes <alias>`, through the ready lease that expires last.
- **It stops a holder whose lease is no longer ready**, so a holder never keeps a host up past its
  lease. Until v0.8 the on-box idle check counts the holder's session as activity, so the box runs
  while any lease is ready, which is v0.8's rule anyway.
- **It MUST NOT stop a process it did not start.** A PID is trusted only when its start time and
  command line match the keeper's own record.
- **One holder per host.** Only one connection on a host can bind the forwards (D12). Every lease
  is on one box until v0.7, which makes it one per project host.
- **It retries a holder that fails with backoff, never in a tight loop** (D14). A dead session
  keeps the ports until 0.2.01's `ClientAliveInterval` bound drops it.
- **It runs as a logon scheduled task for Terry's user.** That is persistent configuration on his
  laptop, so it is registered only with his yes, in 0.2.05.

**Why a keeper and not the ProxyCommand or `lease create`:**
- **A holder started by the ProxyCommand** belongs to the process tree of whichever connection ran
  it, the desktop app's included. It would die with that connection, or outlive it with nobody
  tracking it.
- **`lease create` is often an agent's tool call**, whose process tree is torn down when the call
  ends. It also cannot stop the holder when the lease expires, which no command marks.
- **A keeper that reconciles leases against one holder** handles every one of those cases, and is
  the shape a hosted control plane (#0011) would take anyway.

## Keys

### D17. The CA key lives on the laptop, owner-only and without a passphrase

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
| **Q1** | Where do the CA key and lease state live once the API is hosted? A KMS-backed signer? DynamoDB? | #0011 |
| **Q2** | Where do per-project credentials (Claude Code, Codex, gh) live, so a fresh host or container needs no sign-in? Needs Terry. | #0009 |
| **Q3** | How do many leases, and later many project users, reach the laptop's localswim board? | #0008 |
| **Q4** | One overlay network across all backends (WireGuard, Tailscale, Cloudflare Tunnel), or a reachability path per driver? | #0016 |
| **Q5** | ~~Is the lease CLI PowerShell or Go?~~ PowerShell 7 for the MVP (D18), then Rust (D28). ~~Its default and maximum TTL?~~ 8 hours by default, 24 hours at most, 1 minute at least (D24). | #0001 |

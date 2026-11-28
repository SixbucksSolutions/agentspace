# E0.12-T05-S01 (#0266): the expected values of linux/amd64's live check, written before any dry run

RFC 2119 keywords; the capitals are load-bearing. Drafter-owned, in target\t05. Each quote is a line of the
worktree's docs at f1dd444, with its file and line number from a grep (docs\lease-api.md, docs\decisions.md,
docs\driver.md, docs\operator.md, docs\architect.md, README.md). Nothing here comes from a program's output.

Project `agentspace`, platform `linux/amd64`. Host name `agentspace.linux-amd64`, stack
`agentspace-linux-amd64-host-agentspace`, host folder `hosts\agentspace.linux-amd64\`, size `m` is `m8a.xlarge`.

## Exit codes of every step script

| Exit | Means |
|---|---|
| 0 | PASS: every check of the step held |
| 1 | FAIL: a check did not hold; a `FINDING:` line says which |
| 2 | 211 only: the lease ended not ready (unsatisfiable, expired, released or any state but pending and ready) |
| 3 | 211 and 240: the bound ran out |
| 4 | STOP: a stand-in variable (S01_*) or an AGENTSPACE_* variable is set in a live run, so no program was started |
| 6 | The observer's hours for linux/amd64 reached 1.75 (Q20's cap is 2), or could not be read: the runner stops and asks the architect |

## The steps

| Step | Expected | Source (file:line, quoted) |
|---|---|---|
| 200 | `scripts\live\mask-proof.ps1` passes: exit 0, no `FAILED:` line | docs\architect.md (Live checks): "Mask before printing, with the mask shown to catch a planted value of every shape first." |
| 201 | `aws sts get-caller-identity` exits 0, its streams discarded inside the step, so nothing reaches `.out` | docs\architect.md:625 "A sign-in check sends its stdout to `$null` inside the wrapped command, so nothing reaches `.out`"; AGENTS.md: "The AWS sign-in is checked by exit code alone" |
| 202 | `status -All` exits 0; every host `State     stopped`, the box (`Host      swimcoach (agentbox), ready`) and `Host      agentspace (agentspace-host-agentspace), ready` among them; an amd64 host absent or `stopped`; no `Guard` line; every `Leases    0 live`; `lease list` valid against `lease-list.output.schema.json` with 0 leases `ready` or `pending` | docs\operator.md:3798 `Host      swimcoach (agentbox), ready`; operator.md:3805 `Host      agentspace (agentspace-host-agentspace), ready`; operator.md:3816 `Host      agentspace.linux-amd64 (agentspace-linux-amd64-host-agentspace), ready`; operator.md:3819 `State     stopped`; operator.md:3823 `Leases    0 live`; AGENTS.md "The arm64 hosts MUST NOT be started" (this card's rule) |
| 210 | `lease create --platform linux/amd64 --project agentspace --ttl 2h` (default size) exits 0 and prints one compact JSON line: `state` `pending`, `platform` `linux/amd64`, `profile` `agent-linux`, `size` `m`, `ttl_seconds` 7200, `project` `agentspace`, no `ssh`, no `reasons`, no `sized_for`, no `expires_at`; valid against `lease.output.schema.json` | docs\decisions.md:5091-5092 "`linux/amd64`'s live check, after the install: `create --platform linux/amd64 --project agentspace` answers `pending`"; lease-api.md:1984 "`--platform <os/arch>` ... `linux/arm64`, `linux/amd64` and `windows/amd64` are supported"; lease-api.md:1991 "`--ttl <duration>` | [TTL](#ttl) | `8h`, or with a plan the TTL its turn gives | From 1 minute to 24 hours."; lease-api.md:1679-1680 "Examples: `8h`, `90m`, `1h30m`, `1h 30m`, `2 hours`"; lease-api.md:1990 "`--size <size>` ... | `m`, without a plan"; lease-api.md:889 "`pending` | Accepted, and not yet ready to connect to. No credential has been issued."; lease-api.md:690 "`ttl_seconds` ... The TTL of the current certificate, or of the one asked for if none was issued."; docs\driver.md (Where it places a lease, table) "No record, and the project list approves the lease's platform for the project | `pending`" |
| 211 | `lease get` once a minute, at most 50 minutes: `pending` until `ready` (exit 0); `unsatisfiable`, `expired`, `released` or any other state (exit 2); the bound (exit 3). Each read prints its time and state, and the masked last 3 lines of `%USERPROFILE%\.agentspace\hosts\agentspace.linux-amd64\provision.log`. A `sync failed` line in that log is a FINDING (#0163 is fixed) and ends the step with exit 1 once the loop ends. Hours of linux-amd64 below 1.75 at each pass, else exit 6 | docs\state.md:379 "`...\hosts\<host name>\provision.log`" and state.md:2771 "`%USERPROFILE%\.agentspace\hosts\<project>\provision.log`, from v0.12 `hosts\<host name>\provision.log`"; operator.md:1860-1861 "A host's name is `<project>` on `linux/arm64`, and `<project>.<os>-<arch>` on every other platform"; lease-api.md:2208 "For a lease that is `pending`, `get` MAY let agentspace carry on placing it"; lease-api.md:889-890 states; README.md:640 "**Until card #0163 is fixed, a new host's first provisioning run fails at `sync`**" (see 'About #0163' below) |
| 212 | `lease get <A>`: `state` `ready`, `ssh.user` `ubuntu`, `profile` `agent-linux`, `size` `m`, `platform` `linux/amd64`, `project` `agentspace`, `ttl_seconds` 7200, an `ssh.host_alias` of the form `agentspace-agentspace-<first 8 hex digits of id>` (printed as True or False only), `expires_at` in the future and at most 7200 seconds ahead, `created_at` before `expires_at` by at least 7200 seconds; members exactly `id,project,state,platform,profile,size,ttl_seconds,created_at,expires_at,ssh`; valid against `lease.output.schema.json` | lease-api.md:719-720 "A lease on `linux/arm64` or `linux/amd64` logs in as `ubuntu` (D3)"; lease-api.md:708 "`ssh.host_alias` is `agentspace-<project>-<first 8 hex digits of id>`"; lease-api.md:692 "`expires_at` ... its issue time plus `ttl_seconds`"; lease-api.md:890 "`ready` | A connection through `ssh.host_alias` is expected to succeed until `expires_at`."; lease-api.md:1961 "**`ready`:** `ssh.host_alias` is usable from the caller's OpenSSH configuration" |
| 213 | Over the alias: `ssh -G <alias>` has a `proxycommand` line (True; the `user` line and the line itself never printed); one `ssh <alias> true` exits 0; `uname -m` prints `x86_64`; the first line of `ldd --version` ends in a version of 2.43 or newer (compared as numbers: 2.43 yes, 2.5 no, 3.0 yes); each of `node`, `uv`, `claude`, `codex`, `gh`, `fnm` answers `--version` with exit 0 and a version text (first line, masked). Each ssh has BatchMode and ConnectTimeout | docs\decisions.md:5093-5094 "over its alias, `uname -m` prints `x86_64`, glibc is 2.43 or newer, and each `agent-linux` tool answers"; decisions.md:4927-4928 "The live check shows the promise on the host: `uname -m`, `ldd --version` and each tool's version over the lease's alias (D100)."; lease-api.md:1804 "| Libc minimum version | 2.43 |"; lease-api.md:1806 "| Node | The newest Node.js LTS release ... `fnm` is installed"; lease-api.md:1807 "| `uv` | The latest release"; lease-api.md:1808 "| Claude Code | The native install"; lease-api.md:1809 "| Codex | The latest release"; lease-api.md:1810 "| `gh` | The GitHub CLI"; docs\architect.md:623-624 "anything handed to him is proven first from the laptop: an alias by `ssh -G <alias>` showing a ProxyCommand and one real `ssh <alias>` login." |
| 214 | `agentspace-operator status -Host agentspace -Platform linux/amd64` exits 0 and prints `Platform  linux/amd64`, `State     running`, `Type      m8a.xlarge`, an `Instance` line (masked), a `Disk` line, an `SSM` line (`online` expected; anything else is a NOTE) | docs\operator.md:3747-3749 "`status -Host <project> -Platform <platform>` ... for a host off `linux/arm64` it prints one line more, `Platform  <platform>`, first, before the five"; operator.md:3817 `Platform  linux/amd64`; operator.md:3716 `Type      <type>`; docs\driver.md:1003 "| `m` | `m9g.xlarge` | `m8a.xlarge` | `m8a.xlarge` |" |
| 220 | `lease create --platform linux/amd64 --project agentspace --size s --ttl 2h` while A is `ready`: answers `ready` at once (not `pending`), `size` `s`, `ttl_seconds` 7200, `ssh.user` `ubuntu`, an alias different from A's; then `status -Host agentspace -Platform linux/amd64` again: `State     running`, `Type      m8a.xlarge` (no resize); `lease list`: the project's linux/amd64 leases `ready` are exactly 2 | docs\decisions.md:3747 "| `ready`, larger than the size asked for, and another live `ready` lease of the project exists | `ready`, on the larger host |"; lease-api.md:1006-1007 "**Compute larger than the size asked for** meets the lease at once while another lease of the project is `ready` on it."; lease-api.md:1661-1662 "A lease placed on larger compute is met, not swapped (D5), and its `size` stays the size asked for"; driver.md:1002 "| `s` | `m9g.large` | `m8a.large` | `m8a.large` |" (so `s` is smaller than the host's `m8a.xlarge`) |
| 230 | `lease release <B>` then `lease release <A>` (both attempted whatever the first answers): each answer `state` `released`, with `released_at` and `expires_at`, no `ssh`, the same `id` and size as the lease; valid against `lease.output.schema.json`; `lease list`: none of the project's linux/amd64 leases `ready` or `pending`, A and B `released`. The guard.log line count and the end time (UTC) are kept for step 240 | lease-api.md:2345 "| `ready` | First ends the lease's connections ... drops `ssh`, keeps `expires_at`, and sets `released_at` | `released` |"; lease-api.md:892 "`released` | Given back by `release`. Its local credential and alias are gone." |
| 240 | At most 35 minutes after 230's end: the observer's `edges.log` gains a line `<utc> <local> agentspace.linux-amd64 <from> -> stopped type=m8a.xlarge` dated after 230's end, at most 25 minutes after it (D56's bound; the observer reads once a minute, so a stop seen up to 26 minutes after is a NOTE that passes); `guard.log` has new lines since 230's count, among them `host=agentspace.linux-amd64 stopped` (printed as time, step and keyword classes only); one read-only `aws ec2 describe-instances` filtered by the stack name `agentspace-linux-amd64-host-agentspace` returns `stopped` with `StateReason.Code` `Client.UserInitiatedShutdown` (an API stop, the guard's); `Client.InstanceInitiatedShutdown` would be the host's own power-off and is a FINDING. Hours below 1.75 at each pass | docs\decisions.md:5094-5095 "the release lets the guard stop it within D56's bound"; decisions.md:3320-3322 "**The stated time:** a host is `stopped` within the grace plus 10 minutes of its last live lease's end"; lease-api.md:982-983 "**The grace is 15 minutes, fixed.** ... a project's compute is stopped within 25 minutes of the end of its last live lease"; decisions.md:3317-3319 "**The guard logs to `%USERPROFILE%\.agentspace\guard.log`**: one line per host per run that stops a host, finds one unexplained, or fails"; docs\operator.md:4336 "`<time> host=<project> stopped since=<since>`" and operator.md (guard, Its log, `host=<project>` bullet) "From v0.12 the field is `host=<host name>`, which for a host not on `linux/arm64` is `<project>.<os>-<arch>`, such as `host=example.linux-amd64`"; docs\architect.md:582-583 "`Client.InstanceInitiatedShutdown` is the host's own power-off, `Client.UserInitiatedShutdown` an API stop."; docs\architect.md:596-597 "`StateReason.Code` reads `Client.UserInitiatedShutdown` for the guard's stop and `down`'s alike." |
| 250 | `status -All` exits 0: every host `State     stopped`, the box, agentspace's linux/arm64 host and `agentspace.linux-amd64` present, `Platform  linux/amd64` after the last, every `Leases    0 live`, no `Guard` line; `lease list`: 0 leases `ready` or `pending` | docs\operator.md:3816-3823 (the amd64 host's part, quoted for 202); docs\architect.md:588 "The box ends stopped, confirmed with `status`, and no lease the check made is left ready." |

## The size's meaning

`s` is 2 vCPUs and 8 GiB, `m` is 4 and 16 (lease-api.md Size table). On amd64 `s` is `m8a.large` and `m` is
`m8a.xlarge` (driver.md:1002-1003). Lease A at `m` makes the host `m8a.xlarge`; lease B at `s` is smaller than
it, so D66's second row answers `ready` at once.

## About #0163 (read for step 211)

README.md:640 still says "Until card #0163 is fixed", but the code and the docs say it is fixed: D61 carries it into
v0.08 ("a first run succeeds without the 15-minute patch wait", docs\decisions.md:3490-3491), `sync` waits for the
patch run in `crates\agentspace-sync\src\patch_wait.rs` and `syncing.rs` (grep `0163`), operator.md's "The patch
wait" section, and docs\runs\e0.08-t05-s01.md Part A shows it live (wrapper 307, 1 minute 10 seconds, no 15-minute
wait). So a `sync failed` line in the new host's `provision.log` is not expected on amd64, and step 211 treats it as a
FINDING; its once-a-minute `lease get` is also what README's note asks for.

## What no doc fixes, so the drafter chose (each is a question for the dev lead)

- The six version commands (`<tool> --version`): the docs name the tools, never the command. Each answers when it exits
  0 and its first line holds `<digits>.<digits>`.
- The TTL, `2h`, and its seconds, 7200 (pin P3 pending), in one variable of `s01-common.ps1`.
- B's own `--ttl 2h` (the task's line for 220 names none; without it B's TTL is 8 hours).
- `describe-instances` filtered by the tag `aws:cloudformation:stack-name` with the stack's name, and, when that finds
  no instance, by the tag `project` (the template tags the instance `project` with the stack's name, operator.md "The
  stack's tag").

## The wrapped commands (run from the worktree's root; one wrapper name per run, a rerun takes the next number)

`pwsh -NoProfile -File scripts\live\wrap.ps1 -Out target\runs\runner -Name <nnn>-<what> -Script target\t05\s01-step-<nnn>.ps1`

| Name | Script | Expected exit | Runs about |
|---|---|---|---|
| 200-mask-proof | s01-step-200.ps1 | 0 | 1 minute |
| 201-aws-signin | s01-step-201.ps1 | 0 | seconds |
| 202-status-before | s01-step-202.ps1 | 0 | seconds |
| 210-create-a | s01-step-210.ps1 | 0 (lease A pending; the observer's hours.json MUST be fresh first) | seconds |
| 211-wait | s01-step-211.ps1 | 0, at most 50 minutes (a runner waits on the .exit file in a bounded foreground loop) | minutes |
| 212-get-a | s01-step-212.ps1 | 0 | seconds |
| 213-ssh | s01-step-213.ps1 | 0 | about a minute |
| 214-status-host | s01-step-214.ps1 | 0 | seconds |
| 220-create-b | s01-step-220.ps1 | 0 (a runner stops here when 214 failed) | seconds |
| 230-release | s01-step-230.ps1 | 0 | seconds |
| 240-guard-stop | s01-step-240.ps1 | 0, at most 35 minutes after 230's end | up to 25 minutes |
| 250-status-after | s01-step-250.ps1 | 0 | seconds |

A step's exit is read from its `.exit` file, and only its `.masked.txt`, `.exit` and `.start` are read. The steps need no
variable and a live run MUST hold none of `S01_*` (except `S01_STATE_DIR`) or `AGENTSPACE_*`: the step ends with exit 4
and starts nothing otherwise. The raw state (lease IDs, the documents of 210, 212 and 220, the counts and times 240 reads)
is `target\t05\s01-state\`, which MUST NOT exist before 210 runs: a rerun of 210 or 220 refuses while its lease is
recorded.
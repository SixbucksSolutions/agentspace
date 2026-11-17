# Handoff: agentspace after the MVP

RFC 2119 keywords; the capitals are load-bearing.

**Written by the architect on 2026-11-17, the day Terry approved the MVP complete, for whichever
agent picks agentspace up next.** It assumes you have read nothing else yet. Read it, then
[AGENTS.md](../AGENTS.md); the rest is linked from here.

## Where things stand

- **The MVP is shipped and closed.** All 17 stories across the three MVP epics (#0001, #0002,
  #0003) are merged to `main` and in Completed on the board. The last, E3.03 (PR #17), had a
  swimcoach agent in a Claude desktop-app session on a lease work swimcoach card #0057 on the
  board.
- **The repo is clean:** `main` only, locally and on GitHub, with no worktrees.
- **Every dev lead session (T1 to T8) is archived.**
- **Nothing reports to swimcoach.** `scripts/milestones.json` is empty, every agentspace milestone
  was stopped on Terry's word, and his phone shows no agentspace cards.
- **No lease is live.** `%USERPROFILE%\.agentspace\leases.json` holds the one released swimcoach
  lease, `~/.ssh/agentspace.d` is empty, and `Include agentspace.d/*` stays as line 1 of Terry's
  `~/.ssh/config` on purpose (E2.06).
- **Nothing is in progress on the board.** Everything open is in Backlog (below).

## What exists

A lease API on the laptop over one Graviton EC2 box. The contract is
[lease-api.md](lease-api.md); the reasoning is [decisions.md](decisions.md) (D1 to D24, with a
"Findings from story" block under the decision each story tested); the plan is
[roadmap.md](roadmap.md).

| Piece | Where | What it does |
|---|---|---|
| The box | `infra/agentspace.yaml` (stack `agentbox`), `box/` | Graviton EC2, Ubuntu, reached only over Session Manager; idle stop on the box plus a CloudWatch backstop |
| Operator script | `scripts/agentspace.ps1` (Windows PowerShell 5.1), `scripts/ssh-proxy.cmd` | `up` (start, wait, push the localswim Bot token), `sync` (install `box/system`), `ca`, `down`, `status`. `ssh-proxy.cmd` runs `up` as the ProxyCommand |
| SSH CA | `%USERPROFILE%\.agentspace\ca` (D17, D21) | Signs each lease's key; the box trusts it through `50-agentspace-ca.conf` |
| Login hardening | `box/system/51-agentspace-logins.conf` | Exempts 127.0.0.1 from sshd's penalties, `LoginGraceTime 30` (E3.01) |
| Credentials | `scripts/lease-credential.ps1` | Issue, renew and release a lease's key and certificate (E2.05, E2.07) |
| Aliases | `scripts/lease-alias.ps1` | One alias file per lease in `~/.ssh/agentspace.d`, pulled in by one `Include` (E2.06) |
| Driver boundary | `scripts/lease-driver.ps1` (interface in its header), `scripts/lease-driver-ec2.ps1` | Place, renew, release; only the EC2 file names EC2 (D1, D6, E1.03) |
| Lease CLI | `bin/agentspace.cmd`, `bin/agentspace` (Git Bash), `scripts/lease-cli.ps1`, `scripts/lease-cmd-*.ps1`, `lease-values.ps1`, `lease-state.ps1` | `agentspace lease create/get/renew/release/list`, PowerShell 7, JSON out, locked atomic state file (E1.02 to E1.05) |
| Schemas | `schemas/` (contract), `schemas/state/` (internal) | Draft 2020-12, strict out, tolerant in, 70 samples (E1.01, D20) |
| Process tools | `scripts/watchdog.ps1`, `scripts/report-milestones.ps1`, `scripts/milestones.json` | The architect's dev-lead watchdog (D22) and the swimcoach reporter |

**Using a lease from a project** is the README's "Use a lease from a project" section, written by
E3.03. Read it before running one: a desktop-app session needs a holder for the board's forwards
(#0042), and a closed holder can leave the forwards stuck on the box (#0044).

## Checking that it still works

Every check runs in throwaway folders and never touches Terry's real `~/.ssh` or leases. Run them
from the main checkout after any change near them, and prove a check can fail before trusting a
pass.

| Check | Run with | Passed at handoff |
|---|---|---|
| Schemas and samples | `pwsh -NoProfile -File scripts\test-schemas.ps1` | 203 |
| CLI skeleton, help, errors | `pwsh -NoProfile -File scripts\test-lease-cli.ps1` | 260 |
| `create` and `get` | `pwsh -NoProfile -File scripts\test-lease-cmd-create-get.ps1` | 33 |
| `renew`, `release` and `list` | `pwsh -NoProfile -File scripts\test-lease-cmd-renew-release-list.ps1` | 61 |
| Driver boundary | `pwsh -NoProfile -File scripts\test-lease-driver.ps1` | 45 |
| Aliases | `pwsh -NoProfile -File scripts\test-lease-alias.ps1` | 39 |
| Issue a credential | `pwsh -NoProfile -File scripts\test-lease-credential.ps1` | 13 |
| Renew and release a credential | `pwsh -NoProfile -File scripts\test-lease-renew-release.ps1` | 23 |
| Token push (5.1) | `powershell -NoProfile -File scripts\test-push-localswim.ps1` | 14 |

Before trusting a "nothing touched" claim, check Terry's config is unchanged:
`(Get-FileHash $env:USERPROFILE\.ssh\config).Hash`. It changed once, by design, when E3.03's real
`create` inserted the `Include` line; record the new hash before any test that claims to leave it
alone.

## How the work was run

The process is in D14 to D16 and D22 and D23, and its rules are in [AGENTS.md](../AGENTS.md). In
short: the architect owns the design and merges; one Opus/high dev lead session per turn runs one
implementer and one reviewer subagent per story, staffed by blast radius (D15), and opens one PR
per story; turns run in series; only Terry moves a card to Completed.

**What starting the next piece of work looks like:**
1. Arm Terry's board watcher as reader `agentspace` (AGENTS.md, "The board").
2. Read the card Terry points at. An epic MUST NOT be broken into stories until Terry says so
   (D14). Post-MVP epics are in Backlog.
3. Record any design call in `decisions.md`, numbered from D25, before or with the change.
4. For a turn: add its milestone to `scripts/milestones.json` as text (never through
   `ConvertFrom-Json`), restart the reporter with `-Watch`, start the dev lead, and run its
   watchdog.

## Lessons that cost time, so they need not again

Each is recorded where it applies; this is the index.

**The environment:**
- **The Claude app is MSIX-packaged, so its sessions' AppData writes are redirected** into the
  package cache. Laptop state MUST live in `%USERPROFILE%\.agentspace` (D21).
- **`aws login` returns a 400 from a browser holding another AWS session.** Use `aws login
  --remote` and open the URL in a fresh Incognito window (README, "Rules"). An expired sign-in
  shows in the desktop app as a raw PowerShell error (#0034).
- **Windows OpenSSH hands a ProxyCommand a `<guid>_POSIX_FD_STATE` variable** that makes a
  nested `ssh` never read its stdin. It was the token-push hang (#0026, E3.02); `agentspace.ps1`
  now drops it.
- **The desktop app connects with `ClearAllForwardings=yes`**, so an alias's board forwards never
  apply to it (#0042, D12's E3.03 finding).
- **A refused or dead connection through Session Manager is never closed on the box.** It holds
  a pre-auth slot for `LoginGraceTime`, and a dead holder's session keeps the board forwards until
  killed by PID (D10's E2.08 finding, D12's E3.03 finding, #0044).
- **The CloudWatch backstop stops a box whose CPU stays under 1% for 3 hours,** open sessions or
  not (#0043).

**PowerShell:**
- **Variable names ignore case:** `$b` overwrites `$B`.
- **`ConvertFrom-Json` turns ISO date strings into `DateTime`** unless given `-DateKind String`.
- **A script block run by a function sees that function's locals,** so such functions prefix
  theirs (`__lease`, D8's E1.02 finding).
- **Windows PowerShell 5.1 turns merged stderr into a terminating error under `Stop`,** and drops
  empty arguments to native commands (D18).
- **`-File` binds a script's arguments as parameters first,** so the CLI reads
  `[Environment]::GetCommandLineArgs()` (D8's E1.02 finding).
- **Through SSM, base64-encode any multi-line script** (`echo <b64> | base64 -d | bash`); quoting
  through two shells breaks otherwise.

**The Claude app and sessions:**
- **A session can start another only on a turn its user began.** On a turn set off by another
  session's message, the start is refused and the architect proposes a card instead, which Terry
  clicks (AGENTS.md).
- **A worktree guard pins a subagent to its dev lead's current worktree.** A fresh subagent keeps
  its launch worktree; a resumed one follows the dev lead. Dev leads launch fresh subagents for
  follow-ups (AGENTS.md).
- **`archive_session` is refused once when a hook rewrites its input;** re-issue it with exactly
  the input the refusal quotes.
- **Removing a finished worktree can fail with "Permission denied"** while an archived session
  still holds the folder; a second try after archiving works.
- **A message queued to a busy session arrives after its current turn,** so a "hold" can land
  too late. Say what to do if it does.
- **Terry's answer can be shorter than the claim it seems to make.** "It all worked" in E3.03
  meant Test connection only; the board showed no card. Verify on the box and the board before
  relaying a pass.

## What is open

All in Backlog, none started. Terry sets the order; this is the architect's read of it.

**Gaps the MVP's real run found, which bite the first time a project uses leases for real:**

| Card | Gap | Why it comes first |
|---|---|---|
| #0042 | The desktop app clears a lease alias's board forwards | Every desktop-app session needs a hand-run holder today |
| #0044 | A dead client's session keeps the board forwards (no `ClientAliveInterval`) | One closed holder locks every session out of the board until killed by PID |
| #0043 | The CloudWatch backstop stops a quiet box under open sessions | Quiet agents lose their box after 3 hours |
| #0028 | The token push rides Terry's own `agentspace` entry, not the lease | A lease silently depends on that entry |

**Smaller defects and hardening:** #0027 (bare `ssh`/`scp` in `agentspace.ps1`), #0034 (expired
`aws login` message), #0041 (CLI hardening), #0029 (credential issuing), #0032 (CA trust on the
box).

**The roadmap's next horizon** ("Next", epics #0004 to #0010): one Unix user per project, a
worktree per lease, lease-based idle stop, size classes, board access for many leases (#0008,
which #0042 and #0044 feed into), per-project credentials, and revoking a released certificate.
"Later" is #0011 to #0017 and #0030 (the clean-room handoff, D20).

## Rules that are easy to break

- **The repo is public.** Account IDs, the instance ID, account-specific parameters and tokens
  MUST NOT be committed. The Bot token MUST NOT be printed anywhere; prove a push by size, mode
  and hash.
- **Terry's `~/.ssh/config` MUST be backed up before any change** and restored byte for byte when
  a change is temporary.
- **Refused logins MUST NOT be retried in a tight loop:** about ten within 30 seconds still fill
  `MaxStartups` (D14).
- **`stop` goes to swimcoach only when Terry says a milestone is complete.**
- **Times shown to Terry are `h:mm` with am or pm, in the laptop's current time zone,** read
  from the system each time.

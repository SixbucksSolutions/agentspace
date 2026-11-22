# Handoff: agentspace after v0.07

RFC 2119 keywords; the capitals are load-bearing.

> **Final, as of 9:24pm Pacific on 2026-11-21.** v0.07 is closed: all twenty-one of its pull
> requests (#90 to #110) are merged to `main`, and Terry approved #0047 and its last story into
> Completed. Started as the live record of v0.07 at 4:22pm on 2026-11-20, at T01's close, and
> refreshed at each turn's close. Where this file and AGENTS.md, the board or decisions.md
> disagree, they win.

**Written for the architect who starts v0.08, after Terry `/clear`s this one.**

## Start here

**Do these in order before anything else.** Each one comes from a gap that cost time in v0.07.

1. **Read, in this order:** this file; [AGENTS.md](../AGENTS.md), every standing rule;
   [architect.md](architect.md), how a release is run, from Terry's word to Completed;
   [decisions.md](decisions.md), D43 to D55 in full (v0.07's), then D11, D24 and D25 for v0.08's
   design; then [roadmap.md](roadmap.md) and card #0048.
2. **Check what is running before starting anything.** A cleared or restarted session leaves no
   background task of its own, but an orphan from another session can still hold a reader:
   ```powershell
   Get-CimInstance Win32_Process -Filter "Name='pwsh.exe'" | Where-Object { $_.CommandLine -match 'report-milestones|watch\.ps1|watchdog\.ps1' } | ForEach-Object { $_.ProcessId }
   ```
   Any process found MAY be stopped by its checked PID, with Terry's yes (architect.md, "The
   standing duties"). Print PIDs and kinds only, never a command line whole.
3. **Check the board answers:** `Test-NetConnection 127.0.0.1 -Port 8811`. If it does not, the
   localswim service is down, and starting it is Terry's (it happened at 8:12pm on 2026-11-21, when
   the Claude app restarted). Ask him; never start another project's service yourself.
4. **Start the standing duties** ([architect.md](architect.md#the-standing-duties)): the board
   watcher under the Monitor tool, with a forward-slash path, since the Monitor runs bash:
   `pwsh -NoProfile -File 'C:/Projects/claude-localswim-watcher/watch.ps1' -Board agentspace -Reader agentspace`,
   30-minute timeout, re-armed at each expiry. The reporter starts only once
   `scripts/milestones.json` lists a milestone; today it lists none.
5. **Check AWS and the hosts, printing nothing that identifies them:**
   `aws sts get-caller-identity --output text *> $null` for its exit code alone, then
   `agentspace-operator status -All` through a mask (instance IDs, volume IDs, private addresses).
   Both hosts SHOULD read `stopped`. If the sign-in has expired, ask Terry for
   `aws login --remote`, its link pasted into a fresh Incognito window (AGENTS.md).
6. **Wait for Terry's word before laying out v0.08** (AGENTS.md, "The board"). Until then, do the
   prep in [The scope ahead: v0.08](#the-scope-ahead-v008).

## What changed from the v0.06 handoff, and why

- **"Start here" is a checklist with its commands**, not only a reading order. A cleared
  architect started v0.07 by finding each command again. Twice in v0.07, background tasks died
  with a session and nothing said so: Terry's laptop shut down mid-release, and the app restarted
  at 8:12pm on 2026-11-21.
- **The tooling pitfalls are a section of their own** ([Pitfalls](#pitfalls-of-the-laptops-tooling)).
  Each cost a retry in v0.07, and none is about agentspace's design.
- **The architect's review tools are in the repository**, not in a session's scratchpad, which a
  `/clear` loses: `scripts/scan-identifiers.ps1` and `scripts/run-conformance-files.ps1`.
- **Rulings are kept apart from Terry's orders.** v0.06's handoff had orders only. v0.07's rulings
  table finds each ruling made mid-turn in the doc it binds.
- **The scope ahead names the design calls and the evidence v0.07 left for each**, so v0.08's
  prep starts from the findings, not from a blank page.

## Where things stand

**As of 9:24pm Pacific, 2026-11-21.**

| What | State |
|---|---|
| v0.07 (#0047), a host per project | Completed, every card approved by Terry. Twenty-one stories in six turns (D55; #0162 added to T02). Marked done in [roadmap.md](roadmap.md) |
| v0.08 (#0048), host lifecycle and cost guard | Not laid out. It MUST NOT be broken into stories until Terry says so |
| The done-when | Met in T06 ([runs/e0.07-t06-s01.md](runs/e0.07-t06-s01.md)): Terry signed in Claude Code, Codex and gh on `agentspace-host-agentspace` by README's procedure and cloned the checkout; an agent in a desktop-app session on an agentspace lease created, moved and archived #0165 on the agentspace board at 8:34pm, through the host's `localswim`; the keeper held the board on both hosts, with a swimcoach lease on the box beside it |
| The live checks | T05 ([runs/e0.07-t05-s01.md](runs/e0.07-t05-s01.md)): an agentspace lease went `pending` to `ready` on the new host, its alias reached it, a swimcoach lease's alias matched v0.06's bytes and reached the box, the keeper held both hosts at once, and the `agentbox` change set was empty. Three findings: the install's keeper race (README fixed), #0163 and #0164 |
| The conformance suite | Covers v0.07 from T04: 73 files and the deadline case pass against `main` 39363d2's build, 2,640 results ([runs/e0.07-t04-s04.md](runs/e0.07-t04-s04.md)) |
| `main`'s tests | 3,887 passed, 0 failed, 58 ignored helpers, `cargo fmt` and `clippy -D warnings` clean, in T04's run at 39363d2. Nothing since has touched a crate |
| The specs | state.md, operator.md, driver.md, lease-api.md and keeper.md carry v0.07's design, with every ruling made mid-turn written in |
| The hosts | Both stopped, confirmed by the architect's `status -All` at 8:41pm. The box (`agentbox`, swimcoach's host): `m9g.xlarge`, 20 GiB. `agentspace-host-agentspace`: `m9g.xlarge`, 8 GiB, 66% used at T05, its first patch run done at its T06 boot; its volume cannot be changed until about 12:05am on 2026-11-22 (#0164) |
| The new host's sign-ins | Claude Code, Codex and gh are signed in, and `~/agentspace` is cloned; rustc, cargo and codex are on an interactive shell's `PATH` only (#0166) |
| Leases | None ready; 25 released |
| Terry's laptop | v0.07 installed in `%USERPROFILE%\.agentspace\bin` from T05's release build of `e0c4046` (two `.old-` files left for a later install to remove); the keeper runs it. `projects.json` lists `agentspace` (D54); `hosts.json` holds both hosts. No change to his ssh config, keeper task or `PATH` |
| Swimcoach | No milestone; `scripts/milestones.json` is empty and the reporter is stopped. `v0.07` was stopped at 9:24pm |
| Sessions | Every dev lead is archived but T06's, which the app kept because it was open on Terry's screen |

### v0.07 by turn

| Turn | Closed | PRs | What it left |
|---|---|---|---|
| T01, the specs and the template | 4:22pm, 2026-11-20 | #90 to #94 | The five specs, the template's `HostName` and `CaPublicKey`, and the bootstrap's Rust and gcc |
| T02, the foundation crates | 7:04pm, 2026-11-20 | #95 to #99 | `agentspace-hosts`, `agentspace-detach`, and the alias, credential, box and push crates per host; #0162 (bot.json) on Terry's word |
| T03, the programs | 11:48am, 2026-11-21 | #100 to #104 | `pending` and `no_capacity`, the detached provisioning run, `-Host`, `-All`, `bootstrap` and `provision`, the ProxyCommand per host, and one holder per host |
| T04, conformance | 5:04pm, 2026-11-21 | #105 to #108 | v0.07's cases, each PR run by the architect against its own build and shown failing on v0.06; the whole-suite record. No build defect found |
| T05, the first new host, live | 6:21pm, 2026-11-21 | #109 | The install, `projects.json`, the new host, README's "Signing in on a new host", and three findings |
| T06, the done-when | 8:52pm, 2026-11-21 | #110 | Terry's sign-ins and the checkout, the agent's card #0165, and cards #0166 and #0167 |

## Terry's orders from v0.07

**Each binds where its row says; this table only finds them.**

| Order | Given | Where it binds |
|---|---|---|
| A host is made only for a project Terry names, in `projects.json` (Q6) | 2026-11-20 | D54 |
| The disk order covers every host, each with its own 64 GiB ceiling and 60% floor (Q7) | 2026-11-20 | AGENTS.md, "Growing the box's disk" |
| Every story's staffing plan names each subagent's `Model/effort`, with parallel workstreams where they are a net win | 2026-11-20 | architect.md, "Breaking an epic into stories"; D31 |
| Apply card #0161 (localswim's `bot.json`) through agentspace's own process: it became #0162 in T02 | 2026-11-20 | D40; operator.md |
| Yes for T05's instance hours (5:06pm) and T06's (8:08pm) | 2026-11-21 | D53; each turn's milestone commit |

## What v0.07 built

| Turn | Story | Card | PR | What |
|---|---|---|---|---|
| T01 | E0.07-T01-S01 | #0141 | #93 | state.md and schemas: the host registry, the project list and each host's files |
| T01 | E0.07-T01-S02 | #0142 | #94 | operator.md: commands per host, the provisioning run, the bootstrap, the ProxyCommand's host |
| T01 | E0.07-T01-S03 | #0143 | #92 | driver.md and lease-api.md: the EC2 driver per host, `pending`, the project list |
| T01 | E0.07-T01-S04 | #0144 | #91 | keeper.md: one holder per host |
| T01 | E0.07-T01-S05 | #0145 | #90 | The template's `HostName` and `CaPublicKey`, and the bootstrap's Rust and gcc |
| T02 | E0.07-T02-S01 | #0146 | #97 | `agentspace-hosts`: the host registry, the project list and the run lock |
| T02 | E0.07-T02-S02 | #0147 | #98 | `agentspace-detach`: starting a program that outlives the call |
| T02 | E0.07-T02-S03 | #0148 | #95 | `agentspace-alias` and `agentspace-credential`: host aliases and the operator's certificate |
| T02 | E0.07-T02-S04 | #0149 | #96 | `agentspace-box` and `agentspace-push` per host |
| T02 | E0.07-T02-S05 | #0162 | #99 | The token push reads localswim's `bot.json` (#0161) |
| T03 | E0.07-T03-S01 | #0150 | #101 | The EC2 driver per host, `pending`, and the lease CLI |
| T03 | E0.07-T03-S02 | #0151 | #104 | The operator's commands per host: `-Host`, `-All`, `bootstrap` and `provision` |
| T03 | E0.07-T03-S03 | #0152 | #103 | `deploy`, `sync`, `bootstrap` and `provision` per host |
| T03 | E0.07-T03-S04 | #0153 | #100 | The ProxyCommand reaches each lease's host |
| T03 | E0.07-T03-S05 | #0154 | #102 | The keeper holds one holder per host |
| T04 | E0.07-T04-S01 | #0155 | #107 | Conformance: the lease CLI's `pending`, `no_capacity` and host aliases, and its tripwire |
| T04 | E0.07-T04-S02 | #0156 | #106 | Conformance: the operator per host and the ProxyCommand's host |
| T04 | E0.07-T04-S03 | #0157 | #105 | Conformance: the keeper per host, #0138, and the state schemas' samples |
| T04 | E0.07-T04-S04 | #0158 | #108 | The whole suite and the crates against T03's build ([runs/e0.07-t04-s04.md](runs/e0.07-t04-s04.md)) |
| T05 | E0.07-T05-S01 | #0159 | #109 | The install and the live checks ([runs/e0.07-t05-s01.md](runs/e0.07-t05-s01.md)) |
| T06 | E0.07-T06-S01 | #0160 | #110 | The done-when: an agent works a card on agentspace's own host ([runs/e0.07-t06-s01.md](runs/e0.07-t06-s01.md)) |

## Rulings made during v0.07

**Each binds where its row says; this table only finds them.**

| Ruling | Made | Where it binds |
|---|---|---|
| A provisioning run never takes a `ready` host out of `ready`; a pending lease on a ready host is placed by the next run `get` starts | T01 | D48 |
| `CaPublicKey` carries no comment, so the computer's name never reaches the stack | T01 | D44 |
| A host is never sized by the box's parameters; `deploy -Host <p> -InstanceType <t>` resizes one | T01 | D55; operator.md |
| The lease CLI names the run's program with `AGENTSPACE_PROVISION_CMD`; the run gets the CLI's folders through its environment; no launch files | T01 | D55; driver.md, lease-api.md |
| Release of a pending lease runs the driver's release, so a killed placement leaves nothing | T01 | driver.md, state.md |
| After a CA replacement, a host other than the box is recovered by hand | T01 | D17; operator.md |
| WMI's start replaces the environment, so the detached start builds the whole block from the user's logon environment; a hidden console in a new process group; a 10-second bound | T02 | #0147; driver.md, state.md, operator.md |
| The operator's key is made with `hosts.lock` held by the caller; its certificate is signed through a rename | T02 | #0148; state.md |
| The push reads `bot.json` by localswim's spec, checks the 43-character token and the receipt's `base` form | T02 | #0162; operator.md |
| The driver's Context carries `state_dir`, `state_dir_env`, `ssh_dir_env` and `operator_program`; `place_pending` returns each lease it could not place | T03 | D55's pins on main |
| `provision.log` holds the run's own lines, each failure with the program's error text; copying the programs' output is operator.md Q35 | T03 | operator.md, state.md |
| H16 holds the holder record open sharing read and write but not delete | T04 | keeper.md |
| Provision step 9: a lease left `pending` inside a call that succeeds is logged and not warned; the warning's reason drops its prefix and last period; a missing ProxyCommand program leaves every lease `pending` | T04 | operator.md |
| `place_pending`'s one lease per hold of the lock is the driver crate's test; conformance shows its effects | T04 | operator.md |
| A case MUST NOT copy, hash or open the installed program under test | T04 | AGENTS.md |
| Cases that need `C:\Projects\localswim` SKIP when it is absent | T04 | #0156 |
| H9 counts a fast failure logged `started` and then found gone, as the handshake's step 7 allows | T04 | keeper.md |
| A timing failure under another runner's load is run once more alone before it is a finding, and both runs are recorded | T04 | `scripts/run-conformance-files.ps1`'s header |
| An install stops, by checked PID, any keeper the task's trigger started during the copy, then starts the task | T05 | README, "Install it", step 3 |
| A failed first provisioning run's retry by a `get` is D48's designed path, not a retry past a finding | T05 | #0159's record, finding 2 |
| A live check's expected bytes come from the spec; where the spec leaves text free, the record says where the words came from | T05 | #0159's record |
| The bot creates a throwaway card in Ready For Work, never Backlog, since the board refuses its moves out of Backlog | T06 | #0160's record |

## Lessons from v0.07

**Each says what to do differently, and where that is now written.**

| What happened | What to do | Where |
|---|---|---|
| A ruling said a check reads files it does not | Read the check before ruling on what it does | [architect.md](architect.md#running-a-turn), step 2 |
| The architect stamped rulings with times it had not read, in T01 and again in T05 ("6:04pm"), caught both times by a dev lead's clock | Read the clock before writing a time, every time | [architect.md](architect.md#writing-in-the-repository) |
| Every spec writer started before its pins were ruled and needed a second pass | Keep a pins file; rule as pins arrive; MAY hold a writer for them | [architect.md](architect.md#a-turn-that-writes-specs) |
| T01's docs-only PRs, merged without the workspace tests, left 16 operator help tests red on `main` | Run the workspace tests on a docs PR whose docs a crate's tests read | [architect.md](architect.md#reviewing-a-pull-request), step 3 |
| A ruling assumed WMI adds to a started process's environment; a probe showed it replaces it | Ask for a probe before ruling on how an OS call behaves | This file |
| The architect's conformance run named a build folder that did not exist, and reported a failure | Prove the harness first: check every program variable names a file that exists | `scripts/run-conformance-files.ps1` does it |
| Two stories wrote one file (the host alias) with different bytes, so each rewrote the other's | Give a shared literal one home in a library both use | #0151, #0152 |
| H9 passed every quiet run and failed in the architect's review under load, because its row allowed one of the two forms the spec allows | Write a timed case's expectation from every outcome the spec permits, and review under load | keeper.md H9 |
| A spec row said a case holds a file "with no sharing", which made its own path unreachable | Check a case row's setup can reach the outcome it names | keeper.md H16 |
| T04 and T05 had four slips with Terry's real folders between them: a listing of `bin`, a copy of the installed operator, a listing of `hosts\agentspace`, and the Windows user name in an unmasked error. None was written anywhere; T06 had none, after its brief named all four | Name each past slip in the dev lead's brief, and mask errors as well as output | AGENTS.md, "Terry's real files in a review" |
| The architect's sum of #106's results was wrong (510 for 589), and runners' own totals were wrong twice | Sum counts with a command, never by hand | This file |
| Every stand-in run passed while two live defects waited: a patch unit enabled mid-boot never runs, and EC2 accepts a same-size grow | A spec that leans on how an outside system behaves MUST say so, and the live run is where it is tested | #0163, #0164 |
| Two dev-lead scripts read `status` from stdout; it prints on stderr, so each reported a false failure | Read a program's documented stream; operator.md says which | #0160's record |
| The app's restart killed every background task (watchers, reporter, watchdog, a dev lead's turn) and the localswim service, and nothing said so | Re-run "Start here" steps 2 to 5 after any restart, and resend the dev lead its last message | This file, "Start here" |

## Pitfalls of the laptop's tooling

**None of these is agentspace's design; each cost a retry in v0.07.**

| Pitfall | What works |
|---|---|
| The Monitor tool runs bash, which drops a Windows path's backslashes | Give `watch.ps1` as `'C:/Projects/claude-localswim-watcher/watch.ps1'` |
| `%USERPROFILE%` does not expand in PowerShell, Terry's shell | `& "$env:USERPROFILE\.agentspace\bin\agentspace-operator.exe" ...` in anything sent to him |
| A here-string piped to `git commit -F -` reaches git as an argument | Write the message to a scratch file and pass its path to `-F` |
| `pwsh -File script.ps1 -Files a,b` passes the list as one string | Pass one comma-separated string and split it in the script |
| A review worktree under the scratchpad's long path fails on box/tests' fixtures ("Filename too long") | Put review worktrees under `.claude\worktrees\` |
| A worktree folder stays held after `git worktree remove`, by the session that was in it | Remove the empty folder after that session is archived |
| `archive_session` is refused once by a hook that relabels the call | Send it again with exactly the input the refusal quotes |
| `archive_session` is refused while the app shows the session | Ask Terry to archive it from the sidebar |
| Bot's card moves: out of Backlog, and from Ready For Work straight to Ready For Review, answer HTTP 403. Routing around a 403 is refused by auto mode as undermining oversight | Ask Terry to move the card to In Progress; then the architect moves it on |
| `agentspace-operator status` prints on stderr, not stdout | Capture `2>&1`, or read stderr |
| A case-insensitive scan for a short name fragment matched the word "stdout" | Anchor names with `\b`, and look at every hit before believing it |
| localswim's own `localswim` on `PATH` is stale | `C:\Projects\localswim\localswim.exe`, by full path, as Bot (AGENTS.md) |

## The architect's tools

| Tool | What it does |
|---|---|
| `scripts/scan-identifiers.ps1 -Head refs/review/pr<n> -Pr <n>` | The identifier scan of a PR's diff, commits and body, each pattern proven on a planted value first; prints counts, never values |
| `scripts/run-conformance-files.ps1 -Checkout <worktree> -Out <folder> -Files '<a>,<b>'` | Runs conformance files against a build's `target\dist`, after proving each program variable names a file that exists; one summary line per file |
| `scripts/watchdog.ps1 -Lead <name>` and `-Reset` | The 10-minute watchdog on a dev lead (AGENTS.md) |
| `scripts/report-milestones.ps1 -Watch` | The swimcoach reporter, whenever a milestone exists |
| `scripts/milestones.json` | The milestones, edited as text only (AGENTS.md) |

**A PR review, in commands** (architect.md, "Reviewing a pull request", gives the why):
1. `git fetch origin pull/<n>/head:refs/review/pr<n>`, and check the SHA against the dev lead's.
2. `git worktree add --detach .claude\worktrees\rv<n> refs/review/pr<n>`, then `cargo xtask dist`
   in it when the PR's checks run programs.
3. `scripts/run-conformance-files.ps1` over the touched files, and the same files against the
   installed programs (no variable set) to show they fail without the change.
4. `scripts/scan-identifiers.ps1`, then look at every hit; check `git log --format='%an %ae %cn %ce'`.
5. `gh pr merge <n> --squash --match-head-commit <sha>`, read back `MERGED`, delete the branch,
   move the card, tell the dev lead, then remove the worktree.

## What is open

**Rechecked at close, 9:24pm Pacific, 2026-11-21.**

| What | Who acts | Where it goes |
|---|---|---|
| v0.08 (#0048) is laid out only on Terry's word | Terry, then the architect | [The scope ahead](#the-scope-ahead-v008) |
| #0163: a new host's first provisioning run fails at sync (a 15-minute wait on a patch run that never starts that boot), and the host is used before its first patch run | v0.08, or a hardening release | #0163 |
| #0164: `grow -GiB 0` is not refused; EC2 accepts a same-size change and spends the volume's 6-hour window | v0.08, or a hardening release | #0164 |
| #0166: rustc, cargo and codex are on an interactive shell's `PATH` only on a new host | v0.08, or before an agent builds Rust on a host | #0166 |
| #0167: a connection that starts a host prints "The host is ready." twice, and pushes though the push stamp gives no start as a reason | Check the spec against the build first | #0167 |
| #0137 and #0138, fixed and merged, in Ready For Work by Terry's move; Bot cannot move them from there to Ready For Review | Terry moves each to In Progress or past it; the architect then moves it to Ready For Review | #0137, #0138 |
| C13's variant "hosts.json valid but no box record" failed once in a full run, then 265 runs passed; its failure now names the file | Whoever sees it fail again | #107 |
| The localswim checkout's path is fixed at `C:\Projects\localswim`; a seam would let sync's build cases run elsewhere | A later card, if wanted | operator.md, sync step 7 |
| #0027 from Git Bash: `deploy` and `sync` passed in T05; `grow`'s refusal case met #0164 instead | Terry, on #0027's card | #0159's record, step 6 |
| #0064 (a release leaves a lease's connections up) and #0043 (the CloudWatch backstop) | v0.08 | roadmap.md |
| #0109 and #0082, the sampler's silent death and missed work | Before v0.09's measurement | roadmap.md |
| The T06 dev lead's session, not archived | Terry, from the sidebar | AGENTS.md, "Roles and pull requests" |

## The scope ahead: v0.08

**v0.08 (#0048): host lifecycle and cost guard** (D25). A project's host runs only while the
project holds a live lease, and no host runs unnoticed. Done when releasing a project's last live
lease stops its host within a stated time and its next lease starts it again; a host no live lease
explains is reported, and stopped after a stated grace; and one command lists every host with its
project, state and live leases. It replaces D11's idle stop and #0043's CPU backstop for hosts
under D25, and D51's stop-every-host-at-close rule.

**Its stories MUST NOT be laid out until Terry says so.**

**Before its stories are laid out, the architect SHOULD settle, each as a numbered decision:**
- **What stops a host when a lease expires rather than being released.** Expiry is decided on the
  laptop when a lease is read (lease-api.md, States), so nothing on the host knows a lease ended.
  Candidates: the keeper's pass, which already reads the lease state every 30 seconds and holds a
  holder per host; or a check on the host itself. The keeper only runs while the laptop is on.
- **What guards a host while the laptop is off or asleep.** A host left running when the laptop
  sleeps is exactly the host that "runs unnoticed". D11's on-host idle check and #0043's alarm do
  this today, imperfectly (#0043 stopped the box under open sessions). Something on the host or in
  AWS MUST still bound its cost.
- **How a quiet but held lease keeps its host.** A renew (D24) extends a lease; whether a quiet
  agent's session renews, and who renews it, decides whether v0.08 stops a host under a working
  agent.
- **How a release ends the lease's open connections** (#0064), since a stop under an open
  session is what #0043 got wrong.
- **What "within a stated time" and "a stated grace" are**, measured from release, with the
  patch wait on start (#0163) counted in "its next lease starts it again".
- **The one command that lists every host**: `status -All` lists hosts and states; it does not yet
  show live leases per host.
- **Whether #0163, #0164, #0166 and #0167 come into v0.08.** Each is about a host's start, size
  or tools, and v0.08 is the release that starts and stops hosts most often. #0163 is the
  strongest case: every new host's first start fails once and runs unpatched until its next boot.
- **Live checks for the done-when**, with Terry's yes for their instance hours (D19, as D53 did
  for v0.07), and the box and the agentspace host both in them.
- **Terry's decisions**, to bring to him: anything that changes his laptop (a task, a service), the
  grace and stop times if they cost him waiting, and any AWS-side guard that costs money.

**The road after v0.08 is roadmap.md's:** v0.09 size classes, v0.10 plan-sized leases and
contract v1, v0.11 plan from the board, v0.12 both new platforms, then v1.0.

## When Terry approved v0.07

**Done, 2026-11-21, by 9:24pm Pacific**, after a board event showed #0047 and #0160 in Completed.
Kept as the record of this close; the procedure is [architect.md](architect.md#closing-a-release).
1. `v0.07` removed from `scripts/milestones.json` (`f7f23f8`), then `swimcoach stop`, and the
   reporter stopped with no milestone left.
2. v0.07 marked done in roadmap.md's table.
3. This file made final, and AGENTS.md's "Where it stands" pointed at it.
4. Waiting for Terry's word before laying out v0.08.

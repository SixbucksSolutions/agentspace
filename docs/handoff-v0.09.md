# Handoff: agentspace during v0.09

RFC 2119 keywords; the capitals are load-bearing.

> **Live record of v0.09, started at T01's close, 4:31pm Mountain on 2026-11-23**, and refreshed at
> each turn's close. Where this file and AGENTS.md, the board or decisions.md disagree, they win,
> and the disagreement is a bug to fix in this file. The v0.08 handoff,
> [handoff-v0.08.md](handoff-v0.08.md), stays the guide to everything v0.09 has not changed: its
> "Start here", its tools, its lane IDs and its rules that are easy to break.

## The short version

- **v0.09 (#0049) is size classes:** a lease's `--size` of `s`, `m`, `l` or `xl` sets its project's
  host's instance type, `m9g.large` to `m9g.4xlarge`, and a host is resized only while no ready
  lease holds it. The design is D64 to D70 and the layout D71, twelve stories in five turns, in
  [decisions.md](decisions.md).
- **T01 and T02 are closed**: the specs and the sweep (#133 to #135), then the foundations (#136 to
  #138), which turned all 26 of T01's by-design red tests green.
- **Next is T03, the programs**: #0203 and #0204, started on Terry's word.
- **One new finding, #0209**: on `main` at 7f534f8 the lease CLI's isolation guard saw its moved
  home created once under load, hidden since T01 behind the guard's by-design failure. It needs
  Terry's placement ([What is open](#what-is-open)).
- **Nothing is running**: no host started in T01 or T02, no dev lead is live, and the reporter
  reports the `v0.09` milestone alone.

## Where things stand

**As of T02's close, 6:24pm Mountain, 2026-11-23.**

| What | State |
|---|---|
| v0.09 (#0049) | In Progress. T01's and T02's six stories in Ready For Review; the other six in Ready For Work |
| `main` | At 7f534f8, 4,981 passed and 1 failed: the isolation guard's #0209, which then passed alone twice. T01's 26 by-design red tests are all green |
| The hosts | Both `stopped`. The box ran about two minutes on 2026-11-23 to fetch v0.06-T01's transcripts (D64), on Terry's yes (Q12) |
| Instance hours | Q11's 4 for T05's live checks, none used |
| Swimcoach | The `v0.09` milestone (12 stories, 14.5 hours). `v0.09-T01` was stopped at 4:31pm and `v0.09-T02` at 6:24pm. The reporter runs |
| Sessions | T01's and T02's dev leads archived, and every worktree they left removed (`git worktree list` shows `main` alone) |
| Terry's laptop | Unchanged by v0.09 so far |

### v0.09 by turn

| Turn | Stories | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|
| T01, the specs and the sweep | #0197 to #0199 | 4:30pm, 2026-11-23 | #133 to #135 | 2:18 of 3 h (2:12pm to 4:30pm) | lease-api.md's four sizes and reasons; driver.md's map, placing by size and the resizing run; state.md's `instance_type` and its locks; operator.md's deploy and provision by size; the registry's schema and samples; conformance's unsupported size moved to `xxl` |
| T02, the foundations | #0200 to #0202 | About 6:20pm, 2026-11-23 (milestone stopped 6:24pm) | #136 to #138 | About 1:34 of 2.5 h (4:46pm to about 6:20pm) | `agentspace-driver-ec2`'s `SIZES`, `instance_type_for`, `meets`, `resize_target` and reasons; `agentspace-hosts`' `instance_type`, `read_for_placing`, `set_instance_type` and `set_instance` (pin P38); the operator's `-InstanceType` for every host, and both programs' help texts |

## Terry's orders from v0.09

| Order | Given | Where it binds |
|---|---|---|
| Yes to Q11 to Q13: the four sizes at their prices, 4 instance hours for T05 at `l` or smaller, the transcript fetch, his per-agent rule kept through v0.09 | 2026-11-23 | decisions.md, Q11 to Q13; D65, D69, D70 |
| "Lay out v0.09", with a staffing plan and the most subagents per turn reported to him | 2026-11-23 | D71; each story's card |
| "Start T01", then "Start T02" | 2026-11-23 | This file |
| #0137's last part folded into v0.10; #0138, #0027 and #0194 approved into Completed | 2026-11-23 | D72 |
| The open defect and hardening cards placed before v1.0, folded into their epics and archived | 2026-11-23 | D72; roadmap.md |

## Rulings made during v0.09

**T01's dev lead raised pins P1 to P37, and every ruling is cited in the spec it ruled, as
"architect, 2026-11-23, for v0.09-T01, pin Pn".** The ones that changed a decision are change notes
in decisions.md; this table only finds them.

| Ruling | Where it binds |
|---|---|
| P1: `place_pending` refuses a lease its host does not meet while a ready lease holds the host, with `no_capacity` | D68's change; driver.md, place_pending |
| P2, P20: a host with no recorded type answers `pending` and runs, never `no_capacity`; while a project has a placeable `pending` lease, every new lease of it is `pending` | D66's change; driver.md, On a `ready` host |
| P2c, P31: a missing `hosts.json` implies the box at `m9g.xlarge`; a written seed record has no type | D66's change; state.md, The host registry |
| P3, P29: T03 seeds `instance_type` in the conformance helpers (#0203, #0204) | Cards #0203, #0204 |
| P11, P30: a deploy that passes a type removes the record's first; a failed read-back removes it | D67's change; state.md, The instance type |
| P15: a program before v0.09 refuses a `hosts.json` holding `instance_type` | D67's change; README, Build |
| P18: `resize_target` reads only the EC2 driver's `pending` records with a size in `SIZES` | D66's change; driver.md, Sizes and instance types |
| P26: a run's deploy step takes the lease state lock then `hosts.lock`, removes the type, reads the leases, and only then calls AWS | D68's change; driver.md, A resizing run; operator.md, provision step 4 |
| P34, P36: every `deploy` that is not a run's takes the host's `provision.lock`, and refuses while a run holds it | D67's change; operator.md, deploy |
| P7, P10, P25: files widened or tests left red for T02 | Cards #0197, #0200, #0202 |
| P4 to P6, P8, P9, P12 to P14, P16, P17, P19, P21 to P24, P27, P28, P32, P33, P35, P37 | The specs, each where it is cited |

## Lessons from v0.09

| What happened | What to do | Where |
|---|---|---|
| 37 pins arrived in waves while integrators ran ahead of them; each spec story took two extra passes | Send the inventories' pins at once; hold integrators until those are ruled | architect.md, A turn that writes specs |
| Subagents' own pin numbers collided with the dev lead's | Subagents label choices `N1`, `N2`; only the dev lead numbers pins | architect.md |
| Two races and a read and write split surfaced in review | A drafter adding a writer walks every reader against every lock first | architect.md |
| The architect ruled from a grep that no lease CLI test reads lease-api.md, and was wrong | Read the tests; take every red list from `--no-fail-fast` | architect.md |
| Two architect fixes to driver.md mid-review cost S02 a second rebase | Send a fix on `main` to the dev lead before its next rebase | architect.md |
| The architect printed a `%TEMP%` path holding the profile's short name; Python turned `\a` and `\v` in a path into control characters; a PowerShell `$b` overwrote `$B` | Scratch inside the checkout's `target\`; raw strings for paths; distinct variable names | handoff-v0.08.md's rules; every brief |
| A dev lead gave a commit count and a minute count by hand; a sweeper's `sed` rewrote CRLF working copies | Counts from a command; check line endings after a scripted edit | Every brief |
| keeper-win's `spec_stop` failed under load twice and passed alone | Rerun it alone before it is a finding | #0189 |
| T02: two subagents shared a run file; a file was split into code and test lines; a ruling's test count was wrong; a `$` expanded in a commit message | Run-file prefixes; one owner per file; red lists from `--no-fail-fast`; `git commit -F` | architect.md, A turn that builds |
| T02: a by-design red test hid a second, real failure of the same test through every review | Check each by-design red fails by its stated assertion | architect.md, A turn that builds; #0209 |
| T02: the driver's create reads the registry once and writes from that value, so a swap to `read_for_placing` alone would write the implied type | Place from `read_for_placing`; start every write from a fresh `read` | Card #0203 |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| T03, the programs: #0203 and #0204 | Terry's "Start T03", then the architect | D71; the rulings on #0203's and #0204's cards (P3, P29, P38, and the read and write split) |
| #0209: the isolation guard saw its moved home created once under load; cause unknown | Terry places it; the architect recommends a diagnosis story in T03, since T03's code is what the guard protects | #0209 |
| #0027 (OBE by v0.06) and #0194 (met by the architect's state.md fix), in Backlog | Terry moves each out; Bot cannot | D72 |
| `agentspace-sync`'s own check-6 refusal and `tests/host_deploy_sized.rs` | #0204 in T03 | Card #0204; T02's ruling 3 |

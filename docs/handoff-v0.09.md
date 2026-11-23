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
- **T01 is closed**: the specs and the conformance sweep, three PRs merged (#133, #134, #135).
- **Next is T02, the foundations**: #0200, #0201 and #0202, started on Terry's word.
- **`main` has 26 red tests on purpose**, each turned green by a named T02 story
  ([What is open](#what-is-open)).
- **Nothing is running**: no host started in T01, no dev lead is live, and the reporter reports the
  `v0.09` milestone alone.

## Where things stand

**As of 4:31pm Mountain, 2026-11-23.**

| What | State |
|---|---|
| v0.09 (#0049) | In Progress. T01's three stories in Ready For Review; the other nine in Ready For Work |
| `main` | 26 distinct tests red by design, listed below; everything else passed in the architect's reviews |
| The hosts | Both `stopped`. The box ran about two minutes on 2026-11-23 to fetch v0.06-T01's transcripts (D64), on Terry's yes (Q12) |
| Instance hours | Q11's 4 for T05's live checks, none used |
| Swimcoach | The `v0.09` milestone (12 stories, 14.5 hours). `v0.09-T01` was stopped at 4:31pm. The reporter runs |
| Sessions | T01's dev lead archived. One empty folder, `.claude/worktrees/e0.09-t01-s03`, was still held at the close and goes once Windows lets it |
| Terry's laptop | Unchanged by v0.09 so far |

### v0.09 by turn

| Turn | Stories | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|
| T01, the specs and the sweep | #0197 to #0199 | 4:30pm, 2026-11-23 | #133 to #135 | 2:18 of 3 h (2:12pm to 4:30pm) | lease-api.md's four sizes and reasons; driver.md's map, placing by size and the resizing run; state.md's `instance_type` and its locks; operator.md's deploy and provision by size; the registry's schema and samples; conformance's unsupported size moved to `xxl` |

## Terry's orders from v0.09

| Order | Given | Where it binds |
|---|---|---|
| Yes to Q11 to Q13: the four sizes at their prices, 4 instance hours for T05 at `l` or smaller, the transcript fetch, his per-agent rule kept through v0.09 | 2026-11-23 | decisions.md, Q11 to Q13; D65, D69, D70 |
| "Lay out v0.09", with a staffing plan and the most subagents per turn reported to him | 2026-11-23 | D71; each story's card |
| "Start T01" | 2026-11-23 | This file |

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

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| T02, the foundations: #0200, #0201, #0202 | Terry's "Start T02", then the architect | D71 |
| 26 red tests on `main`: `agentspace-hosts`' `the_embedded_samples_cover_every_hosts_state_file` (#0200); 19 operator help tests, the extractor test among them (#0202, pin P25); 6 lease CLI tests, `tests/help.rs`' five and `isolation_guard`'s (#0202) | T02's stories | Cards #0200, #0202 |
| #0202 also owns the conformance help copies and the operator's check 6 code and tests | T02 | Card #0202 |
| T02's brief carries the pinned bytes (P24, P25, P28, P35, P36, P37 and the two reason messages), the Z1 to Z16 and operator.md v0.09 rows that name each behavior's story, and T01's slips | The architect | This file |
| `.claude/worktrees/e0.09-t01-s03`, an empty folder Windows still held at T01's close | The architect | Removed once released |

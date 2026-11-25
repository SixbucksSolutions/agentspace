# Handoff: agentspace during v0.11

RFC 2119 keywords; the capitals are load-bearing.

> **Live record, as of 3:56pm Mountain on 2026-11-25, at v0.11-T01's close.** Started at T01's
> close and refreshed at each turn's close, as v0.10's was. Where this file and AGENTS.md, the
> board or decisions.md disagree, they win, and the disagreement is a bug to fix in this file.

**Written for an architect who picks up v0.11 mid-release, after Terry `/clear`s this one.** Read
[handoff-v0.10.md](handoff-v0.10.md) first for the tools, lanes, pitfalls and rules it carries;
this file adds only what v0.11 has changed or learned so far.

## The short version

- **v0.11 (#0052), plan from the board, is laid out (D89) and its T01 is done.** Twelve stories in
  five turns, #0231 to #0242, every turn on Terry's laptop, no instance hours. Its design is D83 to
  D88, with Terry's (a) on Q17 and Q18.
- **T01's three specs are merged:** contract v2 in lease-api.md (#164), v2's schemas with v1's kept
  under `schemas/v1/` (#163), and state.md, operator.md, driver.md, README and conformance.md (#165).
- **`main` is red by design until T02 and T03**, in exactly the tests below, each on the card of the
  story that turns it green.
- **#0052 carries the first `agentspace-plan` block (D83)**, byte-equal to
  [plans/v0.11.json](plans/v0.11.json). T05's live check reads it.
- **Next is T02, on Terry's "Start T02"**, after the architect's pin as code (D89).

## Where things stand

| What | State |
|---|---|
| v0.11 (#0052) | In Progress. #0231 to #0233 in Ready For Review; #0234 to #0242 in Ready For Work |
| `main` | 9ca69ac, #165's merge |
| Red on `main` by design | The workspace: `agentspace-cli`'s `help.rs` five create cases and `isolation_guard`'s `the_whole_suite_creates_nothing_in_a_home_that_does_not_exist` (#0238); `agentspace-state`'s `schemas::tests::the_embedded_contract_schemas_are_v1s_nine`, `tests::every_lease_state_sample_reads_and_writes_back_to_what_it_read` and `plan_record`'s `a_valid_sample_is_written_back_as_its_own_compact_text` (#0236). Conformance: `test-help.ps1`'s 8 create cases and `test-plan-errors.ps1`'s 5 `--help` cases (#0238). Each was checked by its failing assertion in the architect's merge checks |
| A gap, not a red | `conformance/lease-cli/test-v1-compatible.ps1` passes 94 of 94, comparing the top-level v2 schemas with v0's, so it no longer checks v1. #0239 points it at `schemas/v1/` |
| Hosts, leases, AWS | Both hosts stopped; no lease asked for in v0.11; AWS not checked, since no turn before T05 needs it |
| Swimcoach | `v0.11` (15.5 h, twelve tickets) reporting; `v0.11-T01` removed at its close |
| Sessions | T01's dev lead archived; no dev lead running. `.claude\worktrees\rvmain` is an unregistered folder from 2026-11-23, one entry, not a checkout; left for a person to look at |

## v0.11 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs | #0231 to #0233 | 12:45pm, 2026-11-25 | 3:53pm, 2026-11-25, the last merge | #163 (2:30pm), #164 (3:14pm), #165 (3:53pm) | 3:08 of 3 h | Contract v2, `schemas/` v2 with v1's kept, #0229's refusal in operator.md, the plan's source in state.md, the stand-in board in conformance.md; 27 pins ruled; 19 subagents launched |

## Rulings made during v0.11

**T01's dev lead raised P1 to P27**, each cited in the spec it ruled as "architect, 2026-11-25, for
v0.11-T01, pin Pn". Those that changed a decision are change notes:

| Ruling | Where it binds |
|---|---|
| P1, P2, P9: a 64 MiB answer bound; a second GET after a 404 to tell a board from a ticket; no seam variable for the receipt | D86's change for v0.11-T01 (3641fd7) |
| P17 to P19: localswim's `/api/2026-11-10` prefix and its `not_found` Problem document; UUIDv4 boards; a `http://127.0.0.1:<port>` base alone | D89's change for v0.11-T01 (1b3fdaf) |
| P21, P22: the ProxyCommand finds a record by the alias its project and ID build, refuses one not `ready`, and refuses a lease-form argument when the lease file cannot be read | D49's change for v0.11 (8a65927); operator.md |
| P23, P27: a sharing violation is retried, 3 attempts within 1 second, 400 ms apart, one diagnostic line per retry, counted by tests | operator.md; #0237's card |
| P15, P25: the lease file stays at version 0, so v0.11's programs MUST be installed together; `plan_source` is written last | state.md; #0242's and #0236's cards |
| P24: `budget_minutes` is written as digits alone | lease-api.md; #0234's and #0239's cards |
| The rest | The specs, each where it is cited |

## Lessons from v0.11, by what they are about

**Each says what to do and where it is now written.**

| What happened | What to do | Where |
|---|---|---|
| T01: P1 to P10 missed three facts localswim's `openapi.yaml` held, the `servers` prefix, v4-only path IDs and the loopback-only base, found as P17 to P19 | A sweep of another system's API reads its `servers` entry and each path parameter's input schema, not only the operation | architect.md, "A turn that writes specs" |
| T01: #0233 took three fix passes, as P21, P23 and P27 each designed a failure mode of #0229's refusal after the pass before | A decision that adds a fail-safe rule designs its failure modes (an unreadable input, a retry, how a test sees it) before the layout | architect.md, "Breaking an epic into stories" |
| T01: the three merges ran in series for 83 minutes, each behind a merge check of about 40 minutes | A spec turn's merge checks are planned into its budget, and run on the smallest set of suites that read the changed files, found by a grep of runtime reads, not of citations | architect.md, "A turn that writes specs" |
| T01: a link into another story's new section would have dangled between merges | A link across stories of one turn targets a heading already on `main` | architect.md, "A turn that writes specs" |
| T01: three reds were found only by reviewers' workspace runs | A red list placed from an inventory's reading is confirmed by a workspace run before its cards are final | architect.md, "A turn that writes specs" |
| T01: the dev lead wrote a heavy-run gate per worktree, and held a slot for each of the architect's merge checks | The gate becomes a committed script in `scripts\` before T02 | What is open |
| T01: the architect twice sent a ruling with a time not read from the clock (P15, P26 and P27), each corrected at once | The clock is read in its own call before any message carries a time | handoff-v0.10.md, rule 1 |
| T01: the dev lead's own edits were refused 8 times while it was pinned to another story's worktree | A dev lead enters the story's worktree before its own edits too, not only before launching subagents | The next brief |
| T01: auto mode refused a README edit once with "no verdict", and the one retry the architect allowed passed | A "no verdict" refusal MAY be retried once, word for word; a second refusal goes to Terry | architect.md, "Running a turn" |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| The architect's pin as code before T02: `Record`'s `plan_source`, read and written by nothing, every struct literal `None` | The architect, before T02 starts | D89 |
| The heavy-run gate as a committed script, from T01's dev lead's notes | The architect, before T02's brief | `scripts\` |
| T02 starts on Terry's "Start T02" | Terry | #0234 to #0237 |
| The empty `.claude\worktrees\rvmain` from 2026-11-23 | A person, if wanted | Nothing depends on it |
| Every open row of handoff-v0.10.md's "What is open" not met by v0.11 so far | As that file says | handoff-v0.10.md |

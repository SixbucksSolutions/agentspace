# Handoff: agentspace during v0.11

RFC 2119 keywords; the capitals are load-bearing.

> **Live record, as of 10:18pm Mountain on 2026-11-25, at v0.11-T03's close.** Started at T01's
> close and refreshed at each turn's close, as v0.10's was. Where this file and AGENTS.md, the
> board or decisions.md disagree, they win, and the disagreement is a bug to fix in this file.

**Written for an architect who picks up v0.11 mid-release, after Terry `/clear`s this one.** Read
[handoff-v0.10.md](handoff-v0.10.md) first for the tools, lanes, pitfalls and rules it carries;
this file adds only what v0.11 has changed or learned so far.

## The short version

- **v0.11 (#0052), plan from the board, is laid out (D89), and T01 and T02 are done.** Twelve
  stories in five turns, #0231 to #0242, every turn on Terry's laptop, no instance hours. Its design
  is D83 to D88, with Terry's (a) on Q17 and Q18.
- **T01's three specs are merged:** contract v2 in lease-api.md (#164), v2's schemas with v1's kept
  under `schemas/v1/` (#163), and state.md, operator.md, driver.md, README and conformance.md (#165).
- **T02's four foundations are merged** (#167 to #170): `agentspace-plan`'s budget, turn cut and TTL
  factor; `agentspace-board`, the read-only board client; `agentspace-state`'s `plan_source`; and
  #0229's refusal in the ProxyCommand. Before them, the architect's #166: the pin as code and
  `scripts\heavy-gate.ps1`.
- **T03's lease CLI is merged** (#171): `create --board <id> --epic <ticket>`, `--turn`, the TTL
  from a turn's budget, and contract v2. Naming a board and an epic is now enough to get a sized
  lease, by the build; T05 shows it installed.
- **`main` is green**: the architect's check of #171 on its own build ran the workspace (5,978
  passed, 0 failed, 58 ignored), the schemas and the whole lease CLI suite with nothing red.
- **#0052 carries the first `agentspace-plan` block (D83)**, byte-equal to
  [plans/v0.11.json](plans/v0.11.json). T05's live check reads it.
- **Next is T04 (#0239 to #0241), on Terry's "Start T04"**: conformance, #0230, and the whole
  suite.

## Where things stand

| What | State |
|---|---|
| v0.11 (#0052) | In Progress. #0231 to #0238 in Ready For Review; #0239 to #0242 in Ready For Work |
| `main` | 1a90916, #171's merge |
| Red on `main` by design | None, from #171's merge. T01's reds went green with #168 (state) and #171 (the CLI's help) |
| The combined T02 check, on one build of #167 to #170 merged | Workspace: the six reds above only. Schemas 1,638 and 163. Keeper clean. Lease CLI: 48 files, the 13 reds above only. Operator: 39 files, 1,536 passed, 0 failed, 0 skipped, `test-unmet-proxy-not-ready.ps1` 75 of 75. The deadline case was not run; T04's whole suite runs it |
| A gap, not a red | `conformance/lease-cli/test-v1-compatible.ps1` passes 94 of 94, comparing the top-level v2 schemas with v0's, so it no longer checks v1. #0239 points it at `schemas/v1/` |
| Hosts, leases, AWS | Both hosts stopped; no lease asked for in v0.11; AWS not checked, since no turn before T05 needs it |
| Swimcoach | `v0.11` (15.5 h, twelve tickets) reporting; `v0.11-T01` to `v0.11-T03` removed at their closes |
| Sessions | T01's to T03's dev leads done; no dev lead running. `.claude\worktrees\rvmain` is an unregistered folder from 2026-11-23, one entry, not a checkout; left for a person to look at. Empty folders a session held (`t02-s04`, `rvt02`) are removed once the session holding them lets go |

## v0.11 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs | #0231 to #0233 | 12:45pm, 2026-11-25 | 3:53pm, 2026-11-25, the last merge | #163 (2:30pm), #164 (3:14pm), #165 (3:53pm) | 3:08 of 3 h | Contract v2, `schemas/` v2 with v1's kept, #0229's refusal in operator.md, the plan's source in state.md, the stand-in board in conformance.md; 27 pins ruled; 19 subagents launched |
| T02, the foundations and #0229 | #0234 to #0237 | 4:20pm, 2026-11-25 | 7:10pm, 2026-11-25, the four merges | #166 (the architect's prep, 4:44pm); #167 to #170 (7:10pm) | 2:50 of 3.5 h | `agentspace-plan`'s budget, turn cut and TTL; `agentspace-board`, new, with `ureq` 3.5.2 and `Token::bearer`/`Bearer` in `agentspace-push`; `agentspace-state`'s `plan_source`; the ProxyCommand's refusal with its retry and clock; 6 pins ruled; 24 subagent launches (S01 5, S02 5, S03 3, S04 11), 9 of them ending blocked or stopped without writing, by the dev lead's count of its launches; Terry approved two Writes by hand |
| T03, the lease CLI | #0238 | 7:45pm, 2026-11-25 | 10:16pm, 2026-11-25, the merge | #171 (10:16pm) | 2:32 of 3 h | `create --board`, `--epic` and `--turn`, the TTL from a plan, contract v2, `tests/lease_board.rs` (64 tests, a stand-in board); `main` green; 8 pins ruled, all as proposed; 4 subagent launches, none blocked |

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
| T02 P1, P5: `agentspace-push` gains `Token::bearer`, returning a `Bearer` with a redacted `Debug`, no `Display` and no `Clone`, since `Zeroizing<String>` derives `Debug`; the push's secrecy comments name the two places the token goes | `agentspace-push`'s `lib.rs` (#169) |
| T02 P3: the board client's `ureq` agent has no proxy, no redirects, a 30 s timeout and statuses not as errors | `agentspace-board` (#169) |
| T02 P6: the board read refuses a receipt that is not UTF-8, while the push keeps its UTF-16 tolerance | `agentspace-board`; #169's body |
| The 64 MiB bound counts the body bytes the CLI takes, not a transport's read-ahead of at most 128 KiB | lease-api.md (429b51d) |
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
| T01: auto mode refused a README edit once with "no verdict", and the one retry the architect allowed passed | A "no verdict" refusal MAY be retried once, word for word, only when its text allows another attempt; a second refusal goes to Terry | architect.md, "Running a turn" |
| T02: auto mode refused three new-file Writes with "hard failure ... Don't retry it"; one (`block.rs`) was retried under the first rule's wording and passed, two were stopped at 4:59pm | A refusal whose text says not to retry is never retried by any subagent; it goes to Terry, who MAY approve the Write by hand (5:30pm to 5:34pm: the two files drafted in auto and written in a short default-mode window) | architect.md, "Running a turn", rule 12 |
| T02: two subagents launched while the dev lead's cwd was a story's worktree by a `cd` followed it into the next worktree, and every write was refused | Only `EnterWorktree` pins a subagent; a `cd` does not | AGENTS.md, worktree rules |
| T02: the session's permission mode moved under it twice (auto again by 5:11pm; Accept edits found at 5:34pm) | A step that MUST happen under one mode reads the mode right before it | architect.md, "Running a turn" |
| T02: `wrap.ps1` refused every `-Name` that did not start with three digits, from the brief's own form | Briefs give `-Out target\runs\<role> -Name <nnn>-<what>` | architect.md, "A turn that builds" |
| T02: the architect's `git stash -u` set aside code a running workspace test was testing, and that run was discarded | A test run of uncommitted work runs in its own worktree, never in a checkout the architect switches | architect.md, "A turn that builds" |
| T02: four merge checks of about 40 minutes each became one combined check of about an hour, and `main`'s final tree equalled the tested one | A build turn's PRs are merge-checked together on one local merge, in three gated lanes, and merged only after | architect.md, "A turn that builds" |
| T03: a worktree-isolated session's guard refused a second worktree for the baseline; it used `git archive` of `main` into the story worktree's ignored `target\` | A brief that plans a baseline beside a story plans that form | architect.md, "A turn that builds" |
| T03: a workspace run made before every file's owner was done compiled 19 of a test file's 64 tests | A workspace run counts as final only once every owner of the story's files is done | architect.md, "A turn that builds" |
| T03: a test that ran the built program twice and compared the leases' `created_at` and `expires_at` failed once under the isolation guard's load | A test that runs a program twice on the real clock MUST NOT compare clock times; it compares what the clock does not set | architect.md, "A turn that builds" |
| T03: the implementer ended its run waiting on its own gated run, against its tasking; harmless with one worktree | Taskings keep "finish in one run"; a turn of one story is the only place it is harmless | The next brief |
| T03: a test that accepted either of two outcomes was ruled out (P8) | A test asserts one outcome, shown to fail on the other | architect.md, "A turn that builds" |
| T01 and T02: the architect sent four rulings with a time or interval not read from a command (P15; P26 and P27; T02 P1's "25 minutes"; P6), each corrected at once | A message that carries a time is sent only after the clock's read has returned, never in the same batch, and an interval is computed from two read times | handoff-v0.10.md, rule 1; architect.md, "Writing in the repository" |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| The heavy-run gate is a committed script, `scripts\heavy-gate.ps1` (#166), proven on four runs of three slots (the fourth waited 7 seconds for a slot) and on a failing run's exit code | Every dev lead, and the architect's merge checks | `scripts\heavy-gate.ps1`; architect.md |
| T04 starts on Terry's "Start T04" | Terry | #0239 to #0241 |
| #0238's known gaps for conformance: only a `Content-Length` of 67108865 shows the 64 MiB bound (no such body is sent); JSON nesting depth is untested | #0239 | #171's body |
| Two limits #169 and #170 accepted, each in its PR: `ureq` copies the bearer into a header value that is not zeroed on drop; a sharing violation on the lease file's second open, after a failure of another kind, is not retried | T04's whole suite and the release's handoff, to carry or card | #169, #170 |
| The empty `.claude\worktrees\rvmain` from 2026-11-23 | A person, if wanted | Nothing depends on it |
| Every open row of handoff-v0.10.md's "What is open" not met by v0.11 so far | As that file says | handoff-v0.10.md |

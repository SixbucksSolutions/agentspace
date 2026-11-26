# Handoff: agentspace during v0.11

RFC 2119 keywords; the capitals are load-bearing.

> **Live record, as of 10:34am Mountain on 2026-11-26, at v0.11-T05's close.** Started at T01's
> close and refreshed at each turn's close, as v0.10's was. Where this file and AGENTS.md, the
> board or decisions.md disagree, they win, and the disagreement is a bug to fix in this file.

**Written for an architect who picks up v0.11 mid-release, after Terry `/clear`s this one.** Read
[handoff-v0.10.md](handoff-v0.10.md) first for the tools, lanes, pitfalls and rules it carries;
this file adds only what v0.11 has changed or learned so far.

## The short version

- **v0.11 (#0052), plan from the board, is built: all five turns are closed**, and the epic and its
  thirteen stories, #0231 to #0243, are in Ready For Review for Terry to approve. Laid out by D89 as
  twelve stories in five turns; #0243 joined T05 on Terry's word. Every turn ran on Terry's laptop,
  and no instance ran. Its design is D83 to D88, with Terry's (a) on Q17 and Q18.
- **T01's three specs are merged:** contract v2 in lease-api.md (#164), v2's schemas with v1's kept
  under `schemas/v1/` (#163), and state.md, operator.md, driver.md, README and conformance.md (#165).
- **T02's four foundations are merged** (#167 to #170): `agentspace-plan`'s budget, turn cut and TTL
  factor; `agentspace-board`, the read-only board client; `agentspace-state`'s `plan_source`; and
  #0229's refusal in the ProxyCommand. Before them, the architect's #166: the pin as code and
  `scripts\heavy-gate.ps1`.
- **T03's lease CLI is merged** (#171): `create --board <id> --epic <ticket>`, `--turn`, the TTL
  from a turn's budget, and contract v2. Naming a board and an epic is now enough to get a sized
  lease, by the build; T05 shows it installed.
- **T04's conformance is merged** (#172 to #174): the board, turn, TTL and v2 cases with a stand-in
  board; #0230 met; and the whole suite's record,
  [runs/e0.11-t04-s03.md](runs/e0.11-t04-s03.md): 94 conformance files, 6,208 passed, 0 failed, 0
  skipped; the deadline case 5 of 5; the crates 5,978 passed; the box 172 OK.
- **`main` is green**, by that record, at `bc89cdb` and after.
- **#0052 carries the first `agentspace-plan` block (D83)**, byte-equal to
  [plans/v0.11.json](plans/v0.11.json). T05's live check reads it.
- **T05 is merged** (#175, #176): v0.11 installed, and D88's live check, in
  [runs/e0.11-t05-s01.md](runs/e0.11-t05-s01.md): a lease asked with `--board` and `--epic` equals
  its `--plan` twin, with and without `--turn`, #0052 unchanged by the read, and no instance
  started. And #0243, the shared mask: `conformance/mask-common.ps1`, loaded once through the
  harness and built at load, so every reason that quotes captured output is masked; the whole suite
  at its head, 94 files, 6,208 passed, 0 failed, 0 skipped.
- **One Backlog card came out of it**, #0244: the two places captured output is printed unmasked
  outside a reason (N2, O1). Terry's word places it.

## Where things stand

| What | State |
|---|---|
| v0.11 (#0052) | Ready For Review since 10:31am, 2026-11-26, after its last story; #0231 to #0243 in Ready For Review. Only Terry moves them to Completed |
| `main` | e549bda, after #176 (0829ff9) and T05's milestone removed |
| #0243 | E0.11-T05-S02 (Terry: "Let's pull risk early"; D89's change during v0.11-T05), merged as #176. #0052's plan block and plans/v0.11.json carry it (a761c95) |
| #0244 | Backlog, from #0243's review (N2, O1), `relates_to` #0243 |
| Red on `main` by design | None, from #171's merge. T01's reds went green with #168 (state) and #171 (the CLI's help) |
| The combined T02 check, on one build of #167 to #170 merged | Workspace: the six reds above only. Schemas 1,638 and 163. Keeper clean. Lease CLI: 48 files, the 13 reds above only. Operator: 39 files, 1,536 passed, 0 failed, 0 skipped, `test-unmet-proxy-not-ready.ps1` 75 of 75. The deadline case was not run; T04's whole suite runs it |
| A gap, not a red | `conformance/lease-cli/test-v1-compatible.ps1` passes 94 of 94, comparing the top-level v2 schemas with v0's, so it no longer checks v1. #0239 points it at `schemas/v1/` |
| Hosts, leases, AWS | Both hosts stopped, 0 live leases, by `agentspace-operator status -All` at 10:34am. T05's live check released its leases and started no instance |
| Swimcoach | `v0.11` (18 h, thirteen tickets) reporting until Terry calls the release complete; `v0.11-T01` to `v0.11-T05` removed at their closes |
| Sessions | T01's to T05's dev leads archived; no dev lead running. T05's worktree folders removed. `.claude\worktrees\rvmain` is an unregistered folder from 2026-11-23, one entry, not a checkout; left for a person to look at. Empty folders a session held (`t02-s04`, `rvt02`) are removed once the session holding them lets go |

## v0.11 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs | #0231 to #0233 | 12:45pm, 2026-11-25 | 3:53pm, 2026-11-25, the last merge | #163 (2:30pm), #164 (3:14pm), #165 (3:53pm) | 3:08 of 3 h | Contract v2, `schemas/` v2 with v1's kept, #0229's refusal in operator.md, the plan's source in state.md, the stand-in board in conformance.md; 27 pins ruled; 19 subagents launched |
| T02, the foundations and #0229 | #0234 to #0237 | 4:20pm, 2026-11-25 | 7:10pm, 2026-11-25, the four merges | #166 (the architect's prep, 4:44pm); #167 to #170 (7:10pm) | 2:50 of 3.5 h | `agentspace-plan`'s budget, turn cut and TTL; `agentspace-board`, new, with `ureq` 3.5.2 and `Token::bearer`/`Bearer` in `agentspace-push`; `agentspace-state`'s `plan_source`; the ProxyCommand's refusal with its retry and clock; 6 pins ruled; 24 subagent launches (S01 5, S02 5, S03 3, S04 11), 9 of them ending blocked or stopped without writing, by the dev lead's count of its launches; Terry approved two Writes by hand |
| T03, the lease CLI | #0238 | 7:45pm, 2026-11-25 | 10:16pm, 2026-11-25, the merge | #171 (10:16pm) | 2:32 of 3 h | `create --board`, `--epic` and `--turn`, the TTL from a plan, contract v2, `tests/lease_board.rs` (64 tests, a stand-in board); `main` green; 8 pins ruled, all as proposed; 4 subagent launches, none blocked |
| T04, conformance | #0239 to #0241 | 11:24pm, 2026-11-25 | 2:19am, 2026-11-26, #174's merge | #173 and #172 (1:51am), #174 (2:19am) | 2:55 of 4 h | `board-standin.ps1` and three new lease CLI files, `test-v1-compatible.ps1` on `schemas/v1/`; #0230 met, with a second raw quote fixed (P8); the whole suite's record; #0243 carded; 15 pins ruled; 18 subagent launches, none blocked |
| T05, the install, the live check and the shared mask | #0242, #0243 | 7:23am, 2026-11-26 | 10:31am, 2026-11-26, #176's merge | #175 (8:10am), #176 (10:31am) | 3:08 of 4.5 h | v0.11 installed; D88's live check green in 3 min 27 s, no instance; `conformance/mask-common.ps1` and every listed site masked, the whole suite at #176's head equal to T04's counts; #0244 carded; 15 subagent launches (8 and 7), peak 4, none blocked |

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
| T05 N1: the shared mask is dot-sourced once, by the harness, and the operator and keeper reach it through `helpers.ps1` and `keeper-common.ps1` | conformance.md, "The shared mask" (#176) |
| T05 F1: the mask builds its rules when the harness loads it, and the harness MUST be loaded before a check moves its own process's `USERPROFILE`, `HOME` or `TEMP` | `conformance/mask-common.ps1`; conformance.md, "The shared mask" |
| T05 N3: the two schema checks load the harness for the mask, since `Test-Json`'s message can name a schema file's path | conformance.md, "The shared mask" |
| T05 N2, O1: the harness's start-failure stderr and the keeper's re-print of a child run stay unmasked at their source, outside #0243's scope | #0244 (Backlog) |
| T05: a gate call is never started with `Start-Process`; a driver ends with an explicit `exit 0` | architect.md, "A turn that builds", v0.11-T05 |
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
| T04: the architect's 11:29pm rulings reached the dev lead only at 12:13am, queued while it was busy, and the dev lead sent no status from 11:28pm to 12:12am | A ruling sent to a busy session is confirmed by its reply; a dev lead arms a 9-minute heartbeat under Monitor at its start | architect.md, "Running a turn" |
| T04: the architect's ruling 4 named a helper's loader from its name; `grep -l` found the 10 real ones | A ruling about which files load a helper comes from `grep -l`, never from a file's name | architect.md, "Running a turn", step 2 |
| T04: a mid-turn change to lease-api.md on `main` was shown harmless to the early runs by reading both of its runtime readers, which take the help blocks alone | A spec change on `main` during a turn that runs early on an older base names its runtime readers and what each reads | architect.md, "A turn that builds" |
| T04: P4's final runs on one local merge of two PRs stood as the architect's merge check, the trees compared and every result file read | A conformance turn's merge check MAY be its whole-suite story's runs on the merge, under those two conditions | architect.md, "A turn that builds" |
| T04: `scripts\live\mask.ps1` turned a word after `token: ` into `<secret>` in a case's reason (N9) | Not a defect: the token-assignment rule masks what follows `token:`, and a case named `...-token:` meets it. The mask errs toward hiding | `scripts\live\mask.ps1` |
| T05: runA's gate calls, started detached with `Start-Process`, opened three blank Windows Terminal windows on Terry's desktop at 8:38am | A gate call runs in the shell tool's foreground or its own background option, never `Start-Process`, hidden or not | architect.md, "A turn that builds", v0.11-T05 |
| T05: the empty-program driver's wrapper recorded exit 1 for a run that counted 46 of 46 | A driver ends with an explicit `exit 0` after its failure check | architect.md, "A turn that builds", v0.11-T05 |
| T05: the shared mask, built on first use, missed the real profile's 8.3 form wherever the keeper had moved its own process's home (F1) | A mask that reads the environment builds its rules at load, shown by a planted case that moves the environment first | architect.md, "A turn that builds", v0.11-T05; conformance.md |
| T05: a run from the story's worktree overlapped the dev lead's uncommitted edit of a file it loaded | Suites on a head that may move run from a `git archive <sha>`; a later fix reruns only its diff's loaders, by `grep -l` | architect.md, "A turn that builds", v0.11-T05 |
| T05: the architect gave T05's end as 10:53am, an hour early, by assuming Pacific time while the laptop was on Mountain; corrected at once | The time zone is read with the clock, never assumed from an earlier session (Terry's global rule) | handoff-v0.10.md, rule 1 |
| T05: the architect's own `Set-Location` into S02's worktree, for a read, moved its session into that worktree for one command | The architect reads another session's worktree by path or `git -C`, never by changing its own folder | architect.md, "Running a turn" |
| T01 and T02: the architect sent four rulings with a time or interval not read from a command (P15; P26 and P27; T02 P1's "25 minutes"; P6), each corrected at once | A message that carries a time is sent only after the clock's read has returned, never in the same batch, and an interval is computed from two read times | handoff-v0.10.md, rule 1; architect.md, "Writing in the repository" |

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| The heavy-run gate is a committed script, `scripts\heavy-gate.ps1` (#166), proven on four runs of three slots (the fourth waited 7 seconds for a slot) and on a failing run's exit code | Every dev lead, and the architect's merge checks | `scripts\heavy-gate.ps1`; architect.md |
| v0.11's approval: #0052 and #0231 to #0243 into Completed | Terry | The board |
| v0.11's close, on Terry's word: this file made final, v0.11 marked done in roadmap.md, the `v0.11` milestone removed and stopped, the reporter stopped | The architect | architect.md, "Closing a release" |
| #0244, the two unmasked output sites (N2, O1) | Terry places it | Backlog |
| Two limits #169 and #170 accepted, each in its PR: `ureq` copies the bearer into a header value that is not zeroed on drop; a sharing violation on the lease file's second open, after a failure of another kind, is not retried | T04's whole suite and the release's handoff, to carry or card | #169, #170 |
| The empty `.claude\worktrees\rvmain` from 2026-11-23 | A person, if wanted | Nothing depends on it |
| Every open row of handoff-v0.10.md's "What is open" not met by v0.11 so far | As that file says | handoff-v0.10.md |

# Handoff: agentspace during v0.10

RFC 2119 keywords; the capitals are load-bearing.

> **Live record of v0.10 (#0050), started at T01's close, 3:19pm Mountain on 2026-11-24.** It is
> refreshed at each turn's close and made final when Terry approves v0.10. Where this file and
> AGENTS.md, the board or decisions.md disagree, they win, and the disagreement is a bug to fix in
> this file.

**Written for an architect who picks up v0.10 mid-release.** It carries only what v0.10 changed;
[handoff-v0.09.md](handoff-v0.09.md) stays the reference for the tools, lanes, pitfalls and rules
it lists, and every row of its "What is open" is carried below or says what became of it.

## The short version

- **v0.10 is plan-sized leases and contract v1.** `create --plan <file>` sizes a lease from a
  plan's busiest turn at half a vCPU and 2 GiB per agent at once (D73, Terry's yes to Q14), and
  the lease prints `sized_for`; a plan no size holds is `plan_too_large`. The design is D73 to
  D80, the layout D81, in [decisions.md](decisions.md).
- **T01, the specs, is closed**: [lease-api.md](lease-api.md) is contract v1, `schemas/` has v1's
  schemas with v0's kept under `schemas/v0/`, and [state.md](state.md), [driver.md](driver.md),
  [operator.md](operator.md) and the README carry the plan's record and D77's due rule.
- **`main` has red tests by design until T02 and T03** (below), and four tests that hang or fail
  alone, which #0217 owns.
- **No turn runs on EC2 and v0.10 asks for no instance hours** (D81; Q16). Both hosts are stopped.
- **T02 starts on Terry's word.** Before it, the architect commits the pin as code (D81).

## Start here

1. **Read** this file, then [handoff-v0.09.md](handoff-v0.09.md)'s "Start here" steps 2 to 5 (the
   checks of what is running, the board, the watcher and AWS), which still apply as written, then
   AGENTS.md, [architect.md](architect.md), and D73 to D81 with their change notes.
2. **The standing duties**: the board watcher as reader `agentspace`; the reporter, since
   `scripts/milestones.json` lists `v0.10`; a watchdog on any running dev lead.
3. **Before T02:** the architect's pin as code (D81), then wait for Terry's "Start T02".

## Where things stand

| What | State |
|---|---|
| v0.10 (#0050) | In Progress. T01's three stories in Ready For Review; T02 to T05's eleven in Ready For Work |
| Milestones | `v0.10` (cap 15.5 hours) reporting; `v0.10-T01` removed at its dev lead's archive |
| Instance hours | None asked for, none used (Q16) |
| The hosts | Both `stopped`, `m9g.xlarge` (`status -All`, 1:01pm) |
| Terry's laptop | Unchanged by v0.10 so far; v0.09 installed |

### v0.10 by turn

| Turn | Stories | Started | Closed | PRs | Length used | What it left |
|---|---|---|---|---|---|---|
| T01, the specs | #0211 to #0213 | 1:25pm, 2026-11-24 | 3:19pm, 2026-11-24 | #147, #148, #149 | 1:54 of 3 h | Contract v1 in lease-api.md; schemas/ v1 with schemas/v0/; the plan's record, `last_success_at` and D77's due rule in state.md, driver.md, operator.md and README; 32 pins ruled |

## Red on `main` by design, and who turns it green

| Test | Fails because | Turned green by |
|---|---|---|
| `agentspace-state` tests/samples.rs: `there_are_eight_contract_schemas`, `every_valid_sample_validates_and_every_schema_has_one`, `every_invalid_sample_fails_and_every_schema_has_one` (samples.rs:66, :54) | `plan.input` is not embedded | #0215 (T02) |
| `agentspace-hosts` `model::tests::the_embedded_samples_cover_every_hosts_state_file` (model.rs:668 since the pin as code, 5e7b431; :661 before) | Three new `hosts.state` samples not embedded | #0216 (T02) |
| `agentspace-cli` tests/help.rs, five create-help tests (help.rs:61) | create's help is v1's in lease-api.md | #0219 (T03) |
| `agentspace-cli` tests/isolation_guard.rs `the_whole_suite_creates_nothing_in_a_home_that_does_not_exist` (isolation_guard.rs:549) | Its child suite runs the create-help tests | #0219 (T03); #0218 owns the file and checks it by assertion |
| Conformance `test-help.ps1`, eight create cases, against the installed programs | The same help | v0.10's install (T05) |

**Not by design, and found in T01:** #0191's four targets now hang or fail when run alone, one also
on `main`'s tree: `agentspace-launch` spec_launcher, `agentspace-run`'s rulings and seam, and
`agentspace-operator`'s seam_and_hygiene test (rig.rs:1020). e0.08-t04-s04 recorded each passing
alone. #0217's card carries it.

## Lessons from v0.10, by turn

**T01**, each now in its home:
- Propose the shared pins at launch; plan a fix pass per story; state a ruled order as its own
  list; name the merge order of linked spec stories; check every copied `$defs`; pattern sample
  IDs; a reviewer starts its workspace run first ([architect.md](architect.md), A turn that writes
  specs).
- **Slips, all transcript-only:** `py -0p` printed install paths, and with them the user name; a
  dev lead's `ls -la` printed the owner column; a subagent's stray redirect made a file named `1`
  in a worktree root; a subagent edited a doc with `sed`, and the dev lead read the hunk
  (AGENTS.md, masking rules, for the first two).
- **Worktree-isolated git refuses compound commands**, so each git step is its own call.

## What is open

| What | Who acts | Where it goes |
|---|---|---|
| The pin as code before T02: `Record`'s `sized_for` and `plan`, `Host`'s `last_success_at`, every literal given `None` | The architect | D81 |
| T02 to T05 | Terry's word for each turn, then a dev lead | D81; #0214 to #0224 |
| #0191's targets failing alone | #0217 | Its card |
| `docs/conformance.md`'s stale state-schema passages | #0223 | Its card |
| The live kit's three mask gaps (from handoff-v0.09.md) | **Done during T02**, 2026-11-24: `scripts\live\mask.ps1` rules 9a (a token flag's value after a space), 11a (an account ID in groups of four) and 16a (AWS access key IDs), with five proof cases and four near misses; `mask-proof.ps1` 285 passed | `scripts\live\` |
| The lost observer script (from handoff-v0.09.md) | Not needed in v0.10, since no turn starts an instance (D80, D81); carried to the next release that runs a host live | `scripts\live\` |
| #0225, no program sets its error mode, so a non-PE start from a mode-0 parent raises a modal dialog | #0217 (the two start points and `quiet_hard_errors()`), then #0226 and #0219 in T03, on Terry's word | D81's change during T02 |
| AGENTS.md's sizing section rewritten for plan-sized leases | The architect, at v0.10's install | D73 |
| The "time to a prompt" spec (from handoff-v0.09.md) | Not placed | v0.08's live records |
| The rest of handoff-v0.09.md's open rows: #0192, #0193 and #0195's build in v0.12; #0190, #0082 and #0109 in v1.0 | Their epics | D72, D78 |

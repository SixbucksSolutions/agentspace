# The per-agent footprint

RFC 2119 keywords; the capitals are load-bearing.

What one agent costs a host in CPU and memory, as measured on a real turn, and how to measure it
again for another project. It is D27's footprint, measured under D33
([decisions](decisions.md#d27-a-host-is-sized-by-concurrency-never-by-model-or-effort),
[D33](decisions.md#d33-v003-measures-each-agent-sessions-process-tree-split-into-the-session-and-its-tool-calls)).
Card #0072, story E0.03-T03-S01. A second measured turn, v0.05-T01's, which compiles and is only
partly recorded, was added by card #0102 (E0.05-T02-S04): see
["The second measured turn"](#the-second-measured-turn-v005-t01-which-compiles-and-is-partial).

- **Every footprint figure here MUST come from the measured run alone, never from an estimate**
  (D27). A figure the run cannot give is left out, and the gap is said. The second turn's disk and
  tmpfs sizes, which its record does not hold, are the one exception, and are marked with their
  sources (the section "Sizing facts the record does not hold").
- **Every per-agent figure comes from `agentspace-agents`,** the analysis step below, run over the
  turn's record and its transcripts. A figure that the script cannot give MUST be left out and the
  gap said, and MUST NOT be marked as pending.
- **This table MUST NOT size a host.** The turn was light, as the section "What the numbers do not
  cover" says, and D33 requires a second measurement on a v0.05 turn first. **That second
  measurement was taken and is partial** (the section on the second measured turn), and the tables
  of the first turn, below, still MUST NOT size a host. **The third, v0.06-T01's, covers a whole
  compiling turn** (the section on the third measured turn), and D64 rules that it meets the
  re-measurement for the sizes v0.09 sets, with no per-agent figure.
- Times are 2026-11-18, Pacific Standard Time (-08:00), as in the run record, except the second
  measured turn's, which are 2026-11-19, also -08:00.

## The turn that was measured

- **The project and the turn:** agentspace itself, v0.04-T01 (epic #0030, D34): three stories in
  parallel (#0073, #0074, #0075), 13 subagents planned and 16 that ran. The plans' peaks add up to
  7 subagents at once.
- **How it ran:** one desktop-app session on the lease's box, the v0.04 dev lead (Opus 5.5/high).
  Its subagents ran inside that one process (D33), so the record holds one session row for the
  whole turn, and its `cwd` follows the dev lead from worktree to worktree.
- **What the run was:** `agentspace-v004-t01c`, 6:16:28am to 6:52:40am (2172 seconds), 1087
  samples at a 2-second interval, no gap longer than 2.0 seconds and no notes. The full account,
  with the baselines and the checks, is
  [docs/runs/e0.03-t02-s01.md](runs/e0.03-t02-s01.md); the record is committed beside it as
  `runs/data/e0.03-t02-s01.jsonl.gz`.
- **The unit is the session:** its own process, and every process under it, which are its tool
  calls (D33). A subagent adds no process, and nothing on the box tells one subagent's tool call
  from another's.

## The footprint

### One session, its own process

Measured from the record's session row. CPU is in cores, from cumulative CPU time. Memory is PSS.
The typical value is the p50.

| What | Peak | p95 | p50 (typical) |
| --- | --- | --- | --- |
| The session's own process, CPU cores | 0.14 | 0.06 | 0.01 |
| The session's own process, PSS MiB | 538.41 | 484.48 | 436.82 |

- **This is paid once per session**, whatever the session is doing (D33). It holds the dev lead
  and every subagent it ran, since they share the process.
- **The session spent 37.81 CPU-seconds in all.**

### One working agent, its tool calls

**The figure is the session's tool calls (its descendants) divided by its working agents: its
subagents running at that moment plus its main agent.** The main agent is the session's dev lead,
counted because D27's peak concurrency counts it too. So with one subagent running the divisor is
2, and with none it is 1. The subagents are read from the session's transcripts, and every interval
and sample of the turn is in the statistics. The division's limits are in the section below.

| What | Peak | p95 | p50 (typical) |
| --- | --- | --- | --- |
| Per working agent, CPU cores | 0.11 | 0.01 | 0.00 |
| Per working agent, PSS MiB | 46.17 | 0.00 | 0.00 |

- **Intervals with a subagent running, and with the main agent alone:** 925 and 161, of the 1086
  intervals between the record's 1087 samples.
- **Subagents joined to the record's session, out of the 16 transcripts:** all 16, under the one
  main transcript, which the script joined to the record's one session.
- **The p95 and p50 are low for the reason given under "All of the session's tool calls":** most
  intervals and samples hold little or no tool-call work. The PSS figures are a floor (below).

### The main agent alone

The session's tool calls while no subagent ran, **undivided**. It shows what one agent costs
without helpers. These intervals and samples are also in the table above, where the divisor is 1.

| What | Peak | p95 | p50 (typical) |
| --- | --- | --- | --- |
| Main agent alone, CPU cores | 0.08 | 0.03 | 0.00 |
| Main agent alone, PSS MiB | 46.17 | 0.00 | 0.00 |

- **The main agent alone covers the 161 intervals with no subagent running.** Its PSS peak equals
  the per-agent table's, 46.17 MiB, so the largest divided PSS sample of the turn was one where no
  subagent ran.

### All of the session's tool calls

The whole turn, subagents running or not. These are already measured.

| What | Peak | p95 | p50 (typical) |
| --- | --- | --- | --- |
| The session's tool calls, CPU cores | 0.42 | 0.02 | 0.00 |
| The session's tool calls, PSS MiB | 62.29 | 0.00 | 0.00 |

- **The tool calls spent 9.36 CPU-seconds in all,** so about four-fifths of the turn's CPU was the
  session's own process (37.81 against 9.36).
- **Tool-call processes peaked at 8 at once,** at 6:47:32am. Tools PSS peaked at 6:34:52am, and the
  session's own PSS at 6:50:46am.
- **The p95 and p50 of tool-call memory are 0.00 because most samples hold no tool call.** A
  tool call is short, and the sampler sees it only if it is alive at a sample. The observer's
  5-minute snapshots saw none alive at any of its 8.
- **CPU is not missed that way and memory is.** CPU comes from cumulative time, reaped children
  included, so a call between two samples is still counted (D33). Its PSS is read at the sample,
  so a call that starts and ends between two samples adds no memory to any figure. The PSS figures
  for tool calls MUST be read as a floor.

### The connection overhead, per desktop-app session

A line of its own, because it is outside the session's row (run record, "What this means for the
footprint").

| What | Figure | Kind |
| --- | --- | --- |
| CPU of a desktop-app session's connection | 5.8 to 7.1 CPU-seconds in 2172 s, which the run record gives as "about 6.4"; about 0.003 cores | Measured per process from `/proc`, then apportioned to the window |
| Memory of the connection, RSS | Up to 161 MiB (160.8) | Upper bound: RSS counts shared pages in full |

- **What it is:** two SSM session workers carrying the app's SSH and `sftp` connections, the app's
  bridge, and four `sshd` processes. Every one started between 6:12:10am and 6:12:15am. None is in
  the session's row.
- **It belongs to the desktop app, not to the work.** A session over plain `ssh` was not measured,
  so its overhead is unknown here, and this line MUST NOT be applied to one.
- **The CPU range comes from `ps` rounding `pcpu` to 0.1%** (the run record's apportioning). The
  0.003 is the record's own figure.

### The host

Measured on the box, over the turn. Every row is box-wide.

| What | Peak | p95 | p50 (typical) |
| --- | --- | --- | --- |
| Host CPU, cores | 0.51 | 0.10 | 0.02 |
| Host memory used, GiB | 1.09 | 1.04 | 0.99 |

| What | Figure |
| --- | --- |
| Sessions working at once, peak | 1 |
| Subagents working at once, peak | 7 |
| Working agents at once, peak: the subagents plus the main agent | 8 |
| When the subagent peak was first reached | 6:16:31.936am by the transcripts; first seen at the 6:16:32am sample by the record |
| The sampler's overhead | 0.0029 cores (0.29% of one core) with a session to scan; 0.0009 cores with none |

- **The subagent peak is 7, and it has two times, by two clocks.** The transcripts show 7
  subagents running from 6:16:31.936am, the instant the last of the three S03 subagents started.
  The record's samples are 2 seconds apart, so the first sample at or after that instant is
  6:16:32.057am, and that is when the record first sees 7. **The script prints the sample's time:**
  its JSON gives `subagents_peak_at` as `2026-11-18T06:16:32.057-08:00`, and its Markdown gives
  `6:16am`. The 6:16:31.936am figure comes from the transcripts' own timestamps, read for this
  document, and is not a script output. The run record's "7 from 6:16:31am", read by hand, agrees.
- **The peak of 8 working agents is derived, not printed by the script.** It is the 7 subagents
  plus the dev lead, the main agent, at the same moment, which is how D27 counts concurrency. The
  dev lead is counted as working while its subagents run.
- **The host figures include more than the agents.** They carry the sampler (6.26 CPU-seconds),
  the desktop app's connection, the measurers' logins, the observer's loop and the kernel. They
  cannot be taken apart per sample, so the host's peak overstates the turn's.
- **Memory is measured as total less available,** so it includes the page cache the kernel counts
  as in use.

## What is kept out of every agent figure

| What | Why it is not in the table | Its size |
| --- | --- | --- |
| The measurement's own load: the observer's poll loop | It is the measurer's cost, not the turn's. Its loop forked twice a second, and no session row holds its processes | About 7.5 CPU-seconds, an **estimate** (timed on the box for 30 iterations, then multiplied by its 2116 s). It is in no figure here, and it MUST NOT be added to one |
| The sampler | It is the tool's own cost, and is outside every session's tree | 6.26 CPU-seconds, measured; the overhead line above |
| The desktop app's connection | Outside the session's row; given its own line above | 5.8 to 7.1 CPU-seconds |
| The measurers' logins and the commands they ran | Outside the session's tree | About 1.7 CPU-seconds, an **estimate** |
| Kernel threads and softirq | Outside every session | 0 to 2.3 CPU-seconds, a bound |
| The idle baseline | It is the box's own daemons | 1.41 CPU-seconds, measured on an idle box |

- **#0082 was not pulled in** (architect, 2026-11-18). The host ran 19.69 CPU-seconds above the
  session, the sampler and the idle baseline. The observer and the connection account for about
  70% of it. **What is left, 1.1 to 4.7 CPU-seconds** (0.0005 to 0.0022 cores), is in no session's
  row and is not attributed to any agent here. No orphaned process was seen. #0082 would count work
  that a tool call orphans to PID 1; this table does not.
- **The baseline for that check had neither the connection nor the observer in it,** so a later
  measurement SHOULD take its baseline with the connection open and the observer running, idle
  (run record, "A calibration flaw in the tripwire").

## How the division is made, and where it errs

For each interval between two samples of the session, the session's tool-call CPU over that
interval is divided by the number of its working agents in it: its subagents running in the
interval, plus its main agent. For each sample, the tool-call PSS is divided by the number of
working agents at that instant. An interval or sample with no subagent running has a divisor of 1,
and is also given undivided in "The main agent alone" above.

- **A subagent's running span is the minimum to the maximum `timestamp` in its own transcript
  file,** over every line that has one, whatever its type, and never from the first and last line:
  7 of the 16 files hold timestamps out of order, by at most 1 ms, and the main transcript's by up
  to 20 s. On this turn each subagent file's first and last line still held its minimum and
  maximum, so the rule changed no span here. Each span matched the main transcript's launch and
  completion of the subagent to within 9 ms, on all 16. A span is never
  split on a silence, since the longest in any file was 291.6 s of model time. A subagent resumed
  after finishing would keep one span that includes its idle time, and the script says so in its
  notes. None was resumed here.
  - **An interval `(t[i-1], t[i]]` counts a subagent if** its span starts at or before `t[i]` and
    ends after `t[i-1]`. `n_cpu` is that count plus 1 for the main agent, and the interval's tool
    CPU is divided by it.
  - **A sample at `t[i]` counts a subagent if** its span has `start <= t[i] <= end`. `n_pss` is that
    count plus 1 for the main agent, and the sample's tool PSS is divided by it.
- **The figure divides; it does not attribute.** Nothing on the box tells one agent's tool call
  from another's (D33), so each tool call is shared out equally among the agents working while it
  ran. The dev lead's own tool calls made while subagents run are shared with the subagents, and a
  subagent's build is shared with the dev lead and the other subagents.
  - **A later measurement could apportion each tool call to the agent that made it, but not
    measure it.** The transcripts carry each tool call's `tool_use` and `tool_result` with ids and
    timestamps, in the file of the agent that made it, so a re-measure could share each sample's
    tool CPU by how long each agent's calls overlapped it. Nothing of that is built here.
  - **It still could not measure per agent,** because the record holds tool CPU per session, not
    per call. And most Bash calls are shorter than the 2-second interval: median 0.19 s, 11 of 393
    over 2 s. These Bash figures are from the transcripts, not from the script.
- **Every working agent is given an equal share.** An agent that builds while another reads is not
  told apart from it. The per-agent figures are an average over those working, never one agent's
  own. The main agent alone is the one case with a single agent, and it is undivided.
- **The overlap rule divides by more agents at a launch than ran for most of the interval.** At
  6:16:32am the interval holds S03's three subagents for only its last 0.13 s, and its tool CPU is
  divided by 8. This biases the per-agent CPU down at launches and ends. Every published figure
  above is the same to 2 decimals under the other boundary rules tried (taking the count at
  `t[i-1]` or at `t[i]`, or half-open spans); only the interval counts, 925 and 161, move, by 1
  or 2.
- **Rounding is half-up on the exact decimal value.** The per-agent CPU peak is exactly 0.105
  cores (0.84 CPU-seconds over 2.000 s, divided by 4 working agents, in the interval ending at
  6:22:16am), so it is given as 0.11.
- **A subagent's memory inside the session's process is not separated.** It adds no process, so it
  sits in the session's own row above. Whether the session's own PSS grows with the subagents
  running was not measured.
- **The turn's first minutes are not in the record,** below, so the intervals in them are not in
  the division.
- **The script's notes for this run were short.** It passed over 792 transcript lines that carry no
  timestamp, for time only. It found no malformed line, no gap, no clamped difference, no tie in the
  join, no unjoined session or transcript, no unreadable or resumed subagent and no subagent
  outside the session's samples.
- **Work a tool call orphans is not in any tool-call figure** (#0082, above).

## What the record missed at the start of the turn

The first run, t01b, was recording, but its default match did not take a desktop-app session, so
no session row exists before 6:16:28am (the run record, "The opening check failed"). The host lines
cover that window with no gap, so its load is in t01b's host figures and in none of the figures
above.

| Who | Started | Unrecorded as a session |
| --- | --- | --- |
| The dev lead | 6:13:33am (its first transcript line) | 175 seconds from its first line |
| S01's two subagents | 6:15:17am | 70.5 seconds |
| S02's two subagents | 6:15:50am | 37.7 seconds |
| S03's three subagents | 6:16:31am | None: 3.9 seconds after the record started |

- **The footprint above therefore starts after the dev lead and four of its subagents had begun.**
  The first minutes of a dev lead's session, when its process is warming up and its first
  subagents launch, are the part the table does not show.
- **The default match is fixed** (E0.03-T02-S01), and a measurer MUST confirm that the session
  shows as a session row before the turn's agents launch (D33).

## The second measured turn: v0.05-T01, which compiles and is partial

Measured by card #0102 (story E0.05-T02-S04), under D33 and D27, for v0.09 (#0049). The full
account, with the timeline and the checks, is
[docs/runs/e0.05-t02-s04.md](runs/e0.05-t02-s04.md); the record is committed beside it as
`runs/data/e0.05-t02-s04.jsonl.gz`.

- **The project and the turn:** agentspace itself, v0.05-T01 (epic #0054, D38): five Rust stories
  in parallel, one crate each, built on the box (#0094 to #0098). Each story's plan peaks at 2
  subagents, so D27's count for the turn is 11: 2 + 2 + 2 + 2 + 2, plus the dev lead.
- **This turn compiles.** It is the first turn on which `cargo` ran, and v0.04-T01, which wrote
  specs, is not that (the section "What the numbers do not cover").
- **It is partial, and that is the first thing to read.** The record covers 2:26:18am to
  2:33:12am on 2026-11-19, and sees the dev lead's session for 354 seconds, from 2:27:16am to
  2:33:10am. The turn ran from about 2:27am to about 2:58am, so the record is about a fifth of it. It holds the first
  compile burst, and nothing after 2:33:12am. The sampler died there when the box's root disk
  filled (#0109), and the record has no end line.
- **Every figure below MUST be read as the first 7 minutes, never as the turn.** D33's measured
  compiling turn is still owed.
- **There are no per-agent figures and no measured concurrency for this turn.** The dev lead's
  transcript was not fetched and the lease is released, so the working agents at each moment cannot
  be counted. D27's count of 11 is unchecked by this record.

### The two turns beside each other

Peak, p95 and p50. Every figure is `agentspace-footprint`'s over its record, and none is divided
among agents.

| What | v0.04-T01, specs, whole turn (2172 s) | v0.05-T01, compiles, first 7 minutes (354 s of the session) |
| --- | --- | --- |
| The session's tool calls, CPU cores | 0.42, 0.02, 0.00 | 3.67, 1.21, 0.01 |
| The session's tool calls, PSS MiB | 62.29, 0.00, 0.00 | 662.06, 250.28, 0.00 |
| The session's own process, CPU cores | 0.14, 0.06, 0.01 | 0.17, 0.11, 0.04 |
| The session's own process, PSS MiB | 538.41, 484.48, 436.82 | 515.43, 486.00, 370.94 |
| Host CPU, cores | 0.51, 0.10, 0.02 | 3.83, 1.09, 0.05 |
| Host memory used, GiB | 1.09, 1.04, 0.99 | 1.91, 1.36, 0.94 |

- **Tool-call processes peaked at 8 at once** in both turns: at 6:47:32am in v0.04-T01, and at
  2:32:26am and 2:32:34am in this one. Sessions at once: 1 in both.
- **Compiling moved the tool calls, not the session's own process.** The tool calls' peaks are 8.7
  times (CPU) and 10.6 times (PSS) v0.04-T01's, and the session's own process is about the same
  size. The tool calls spent 53.28 CPU-seconds in this window against the session's 15.22, where
  v0.04-T01's spent 9.36 against 37.81.
- **The host's peak, 3.83 cores, is 96% of the box's 4.** It was reached in one 2-second interval,
  3.67 cores of it the tool calls. A box with 4 cores cannot show more, so what a build wants beyond
  it is not seen, and the later builds are not in the record at all.
- **The per-agent figures: none for this turn.** v0.04-T01's are 0.11, 0.01 and 0.00 cores and
  46.17, 0.00 and 0.00 MiB. A v0.05-T01 figure MUST NOT be put beside them, since the division by
  working agents needs the transcripts.

### Which figures v0.09 reads

- **v0.09 SHOULD read these as a floor for a compiling turn,** and MUST NOT size a host from them
  alone (D33):
  - **The tool calls' peaks:** 3.67 cores and 662.06 MiB of PSS, in one session. They are the load
    that grows with the agents working, and the PSS is a floor for the reason in "All of the
    session's tool calls".
  - **The host's peak CPU:** 3.83 cores, against a 4-core box.
  - **The tool calls' p95 CPU,** 1.21 cores, as what a compile burst looks like, not a sustained
    load. It is 1.21 and not more because most of the window held no compile.
- **v0.09 MUST NOT read these from this turn:**
  - **The p50 figures.** 144 of the session's 178 rows have no tool-call process alive. The first
    is at 2:30:16am, 180 seconds after the session was first seen, so the p50s describe a window
    that mostly did not compile.
  - **The host's memory,** 1.91 GiB at its peak. It is the processes' memory only: from 2:33am the
    build output sat in a tmpfs and counted in the host's memory and in no session's PSS, and the
    record ends before it shows.
  - **Per-agent figures and concurrency,** which this turn does not have.
- **The session's own process is paid once per session** (D33), so v0.09 reads it from the longer
  v0.04-T01 record above, where its peak PSS was 538.41 MiB, and MAY compare it with this turn's
  515.43, which is about the same.

### Sizing facts the record does not hold

They come from the architect's notes and the dev lead's account (PR #52), not from the sampler, and
the run record marks each one's source.

- **A 6.8G root disk did not fit the turn.** It filled to 100% between 2:29am and 2:33am with five
  `cargo` `target` folders at about 370 MB, and was grown to 11 GiB (a 9.7G filesystem) at 2:41am.
  This is a sizing fact for a project host that builds Rust (v0.07, #0047): a turn's build output
  is not in these figures.
- **The build output of five Rust stories passed 5 GB.** At 2:33am the targets moved to a tmpfs
  under `/tmp`, which holds them in RAM, and its 7.7G (half the box's memory) was full shortly
  before 2:53am: one story's target held 2.7 GB and the reviewers' scratch builds another 2.7 GB.
  So a RAM-backed `/tmp` on a 15.27 GiB box can take half of its memory. The build output counts
  in the host's memory and in no session's PSS, so the sessions' PSS figures cannot show it.
- **D33's compiling turn was owed after this one,** and v0.06-T01 was measured for it (the next
  section).

## The third measured turn: v0.06-T01, a whole turn that compiles

Measured by card #0125 (story E0.06-T02-S08), under D33 and D27, for v0.09 (#0049). The full
account is [docs/runs/e0.06-t02-s08.md](runs/e0.06-t02-s08.md); the record is committed beside it
as `runs/data/e0.06-t02-s08.jsonl.gz`. Every figure below is that record's, by
`agentspace-footprint`.

- **The project and the turn:** agentspace itself, v0.06-T01 (epic #0055, D42): seven Rust stories
  in parallel, one crate each, built on a 16-CPU, 61.61 GiB box. D27's count from the plans is 15.
- **The whole turn, with no gap:** 3,148 samples at 2 seconds, from before the dev lead's session
  to 30 seconds after the last merge, ending in the sampler's own end line.
- **Its per-agent figures and measured concurrency were added on 2026-11-23**, by joining the record
  to the turn's transcripts, fetched from the box on Terry's yes (D64; the run record's "Added
  2026-11-23"). The subagent peak was 14, so 15 working agents, D27's count from the plans exactly.
  One working agent's tool calls: 4.02, 0.10 and 0.00 cores, and 755.22, 12.50 and 0.00 MiB of PSS
  (peak, p95, p50).
- **The per-agent peak is one build's parallelism, not an agent's share.** It is the dev lead alone,
  running a `cargo clippy -j 4`, with no subagent running; 4.02 cores is its `-j 4`. A per-agent
  peak MUST NOT be multiplied by the agents at once (15 times 4.02 is 60 cores, against a measured
  host peak of 4.10). v0.10's product of concurrency and footprint (D27) MUST say which figure it
  multiplies.
- **v0.10's answer: the host's peak per working agent, doubled**
  ([D73](decisions.md#d73-a-plan-is-sized-at-half-a-vcpu-and-2-gib-for-each-agent-working-at-once)).
  The host peaked at 4.10 cores and 3.81 GiB with 15 agents working, the turn's measured
  concurrency above, so 0.27 cores and 0.25 GiB an agent. Twice the CPU is 0.55 cores, taken as
  half a vCPU. Memory is 2 GiB an agent, M class's 4 GiB a vCPU (D39), which is eight times the
  measured share and so never binds before CPU does on D65's sizes.
  - **Not the per-agent peak**, 4.02 cores, which is the dev lead's one `cargo clippy -j 4`: 15
    times it is 60 cores, against the 4.10 measured.
  - **Not the per-agent p95**, 0.10 cores, which is spread over the turn's quiet half and hides
    builds that coincide: 15 times it is 1.5 cores, under what the host measured.
  - **Doubling the measured peak is the whole margin** (D73). One project's compiling turn was
    measured, agentspace's, whose crates are small, so a project with heavier builds fills a host
    sized this way sooner. A project that measures its own turn
    ([Measure a new project](#measure-a-new-project)) gets its own figure, and a footprint per
    project is designed in v1.0 (D72).
  - **The figure is agentspace's, never the lease contract's** (D76): lease-api.md gives it as
    today's for information, and it MAY change without a new contract version.

| What | Peak | p95 | p50 |
| --- | --- | --- | --- |
| Host CPU, cores | 4.10 | 0.68 | 0.01 |
| Host memory used, GiB | 3.81 | 2.59 | 2.01 |
| The session's own process, CPU cores | 0.34 | 0.09 | 0.00 |
| The session's own process, PSS MiB | 640.13 | 563.84 | 394.55 |
| Its tool calls, CPU cores | 4.02 | 0.63 | 0.00 |
| Its tool calls, PSS MiB | 1,481.96 | 74.14 | 0.00 |

- **The peaks are what a compiling turn asks of a host, uncut:** 4.10 cores is 26% of the box's 16,
  where v0.05-T01's 3.83 was 96% of its 4. Tool-call processes peaked at 27 at once.
- **v0.09 MUST NOT read the p95 and p50 figures as the turn's typical load.** About 54 of the
  session's 102 minutes held no tool-call CPU at all (the run record's "The quiet stretch").
- **What v0.09 reads from it** is D65's: the host's peaks, against each size's vCPUs and memory.
  The tables of the first turn, above, still MUST NOT size a host, and no per-agent figure here
  MAY be derived from D27's count of 15.

## What the numbers do not cover

- **One project, one turn,** for the tables of the first turn above. They are agentspace's own
  v0.04-T01 turn: 3 stories, 1 session, 16 subagents. Another project's builds and test suites can
  load a host differently. The second measured turn, v0.05-T01's, compiles, but covers 7 minutes of
  31 and is the only other turn measured.
- **The turn was spec writing, which is light.** v0.04's stories write documents and run few
  builds, so its tool-call peaks understate a turn that compiles. The tool calls took a fifth of
  the turn's CPU, and the host never passed 0.51 cores.
  **v0.09 MUST measure the footprint again on a v0.05 turn with two or more stories in parallel
  before it sets any size** (D33, #0049). v0.05's `cargo` builds and test suites are the load that
  matters, and the tools are built to be rerun.
  - **The v0.05-T01 measurement does not meet that.** It covers 7 minutes of a 31-minute turn
    (the section on the second measured turn).
  - **v0.06-T01's does, for the host and the session** (D64; the section on the third measured
    turn). The per-agent figures are still owed, before v0.10 sets its mapping.
- **A 4-core, 15.27 GiB box caps what a peak can reach.** The lease's box has 4 CPUs, and the
  sampler's start line reports 16,008,512 KiB of memory (15.27 GiB). The instance is sold as 16
  GiB, and the figure the kernel reports is what it can use. The earlier record's "15 GiB" is the
  same number, rounded down. A host can never show more than 4 cores or 15.27 GiB busy, so a turn
  that would want more is cut off at those, and its true peak is not seen. This turn reached 0.51
  cores (13% of 4) and 1.09 GiB (7% of 15.27), nowhere near the cap, so the cap did not bind here.
- **One session over one kind of connection.** The session was a desktop-app one. A session over
  plain `ssh`, or several sessions at once, was not measured. Concurrency here is 1 session with up
  to 7 subagents, so the figures say nothing about two dev leads on one box.
- **The model and its effort are not in any figure,** which is D27's point: the model runs at
  Anthropic, not on the host.

## Measure a new project

**A project's agent measures its own turn by README's
["Measure a turn"](../README.md#measure-a-turn)**, which is not restated here. It gives the
sampler's start, the turn, the stop, the fetch and `agentspace-footprint`, steps 1 to 6. Then it
runs this analysis, which needs the turn's transcripts as well as its record.

1. **Before the turn's agents launch, confirm that the session shows as a session row** (README,
   step 1), and note the box's time at the turn's start and end (README, step 3).
2. **Fetch the record to the laptop** (README, step 5), after the sampler has stopped.
3. **Fetch the transcripts too,** the session's and every subagent's, from the box before the
   lease is released: the session's `<session id>.jsonl` and its `subagents/` folder. Fetch them
   from Claude Code's default `~/.claude/projects/` on the box. This run showed a folder under it
   named for the checkout's path with each `/` turned into `-` (`-home-ubuntu-agentspace` here),
   holding `<session id>.jsonl` and `<session id>/subagents/agent-*.jsonl`. That is what this run
   showed, not a documented rule, so a measurer SHOULD look at the folder names. The run's archive
   of them was 17 files for 1 session and 16 subagents.
   - **They MUST NOT be copied into the repo,** quoted at length in a repo file, or used as test
     fixtures. They hold prompts and tool output.
4. **Run the analysis,** on the laptop, from the folder holding the record:

   ```
   py -3 C:/Projects/agentspace/box/system/agentspace-agents <run>.jsonl <transcripts folder> [--from <ISO 8601>] [--to <ISO 8601>] [--json]
   ```

   - **`--from` and `--to`** keep the samples inside the window, both ends included, and each MUST
     carry a UTC offset, such as `2026-11-18T06:16:28-08:00`.
   - **`--json`** prints one object, with the keys `run`, `host`, `sessions` and `notes`.
   - **Exit 0, or 2 with a message on stderr** for a missing file or folder, a record with no valid
     start line, or bad arguments.
5. **Read each session's output on its own.** It names the transcript the session was joined to,
   or says none was. A session with none still has its own-process figures, and no per-agent or
   main-agent-alone figure. The `notes` list skipped lines, gaps, clamped differences and any
   transcript or subagent that could not be read or joined, and a figure with a note against it
   MUST be reported with the note.
6. **Report the measurement's own load apart** from the agents', as this document does: the
   sampler, the connection and any observer.
   - **An observer SHOULD sleep to its next snapshot in one call** (D33), never poll each second.
   - **A baseline SHOULD be taken with the measured session's connection open and the observer
     running, idle,** so the host-against-sessions check can tell them from orphaned work.
7. **State what the turn was,** as the section above does: its project, what it did and how
   heavy, so that nobody reads a spec-writing turn's figures as a compiling turn's.

## Where the figures come from

| Figure | From |
| --- | --- |
| The session's own process, its tool calls, the host and the sampler's overhead | `agentspace-footprint` over `agentspace-v004-t01c`, in the run record's "agentspace-footprint's output for the turn" |
| The connection, the observer and the leftover | The run record's "The host against the sessions" |
| The per-agent and main-agent-alone rows, the subagent peak and the joined counts | `agentspace-agents` over the same record and the turn's 17 transcripts, in its Markdown and `--json` outputs; the two exceptions are named in the text: the 6:16:31.936am start of the peak and the Bash call times |
| The second measured turn's host, session and tool-call figures | `agentspace-footprint` over `agentspace-v005-t01`, in the run record's "agentspace-footprint's output for the run" ([docs/runs/e0.05-t02-s04.md](runs/e0.05-t02-s04.md)) |
| The second turn's times of its peaks, its CPU-seconds, its counts of rows with no tool call and its #0082 check | A short Python over the same record, described in the run record's "How the figures the tool does not give were computed" |
| The second turn's disk, tmpfs and build-output sizes | The architect's notes on card #0102 and PR #52's body, not the sampler: the record holds no disk field |

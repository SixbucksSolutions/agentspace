"""Tests for agentspace-agents (E0.03-T03-S01, card #0072).

RFC 2119 keywords; the capitals are load-bearing.

The suite drives the script as a subprocess and MUST NOT import it. The path comes from the
environment variable AGENTSPACE_AGENTS, defaulting to ../system/agentspace-agents relative to this
file. Standard library only, so it runs on the laptop and on the box alike.

Every expected figure is worked by hand from a small made-up fixture under fixtures/agents/, and
the hand calculation sits in a comment beside it. No real transcript or real record is used.
Nearest rank: value[ceil(p * N) - 1] over the sorted values. Intervals are 2 s.

The three fixtures (the generator is not kept; each file is small enough to read):

division.jsonl, 9 samples at t = 0, 2, ..., 16 s after 01:00:00 -08:00 (09:00:00Z), 8 intervals
I1 = (0,2], I2 = (2,4], ..., I8 = (14,16]. One session, 1000-1, in /home/ubuntu/proj. Three
subagents under its main transcript, spans from their own files (seconds after 09:00:00Z):
    A = [3, 8]   file order 5, 3, <line with no timestamp>, 8, 6
    B = [4, 7]   starts exactly at the sample t = 4
    D = [13, 15]
Interval count n_cpu = 1 + #{start <= t[i] and end > t[i-1]}:
    I1 (0,2]    none                                         n_cpu 1
    I2 (2,4]    A (3 <= 4, 8 > 2), B (4 <= 4, 7 > 2)         n_cpu 3
    I3 (4,6]    A, B                                         n_cpu 3
    I4 (6,8]    A (8 > 6), B (7 > 6)                         n_cpu 3
    I5 (8,10]   A ends at 8 and 8 > 8 is false; none         n_cpu 1
    I6 (10,12]  none                                         n_cpu 1
    I7 (12,14]  D (13 <= 14, 15 > 12)                        n_cpu 2
    I8 (14,16]  D (13 <= 16, 15 > 14)                        n_cpu 2
Sample count n_pss = 1 + #{start <= t <= end}:
    t=0 1   t=2 1 (A starts at 3: not yet)   t=4 3 (A, B)   t=6 3 (A, B)   t=8 2 (A ends at 8: counts; B ended at 7)
    t=10 1   t=12 1   t=14 2 (D)   t=16 1 (D ended at 15)
Tools CPU cores per interval: 1.0, 3.0, 6.0, 1.5, 0.5, 2.0, 5.0, 3.0 (cumulative deltas 2, 6, 12, 3,
1, 4, 10, 6 s over 2 s). Tools PSS MiB per sample: 100, 200, 300, 400, 50, 600, 700, 800, 150.
Self CPU cores per interval: 0.1, 0.2, ..., 0.8. Self PSS MiB per sample: 10, 20, ..., 90.
Host CPU cores per interval (d_busy / d_total * 4 cpus, d_total = 8): 0.25, 0.5, ..., 2.0.
Host memory GiB per sample 1 .. 9.

stats.jsonl, 20 samples (19 intervals), one session, one main transcript, no subagents, so
per_agent equals main_alone equals the tools figures. Tools CPU cores per interval are a
permutation of 1..19, tools PSS MiB per sample a permutation of 10, 20, ..., 200.

join.jsonl, 11 samples (0 to 20 s), five sessions, against join-transcripts/, which holds the
candidates each session has to be told apart from.

tie.jsonl, 2 samples (0 and 2 s), one session, 3000-1; tie-transcripts/ holds its main transcript
(0 to 2 s) and three subagents, each [0.5, 1.5]. Tools CPU-s 10.0 then 10.84, tools PSS 1152 KiB.

hostt.jsonl, clamp.jsonl, gap.jsonl and noend.jsonl, one session each, 5000-1 in /home/ubuntu/edge,
all against edges-transcripts/: one main transcript, 0 to 60 s, no subagents, so per_agent and
main_alone are the tools figures undivided. Their series are given in each test.

subagents.jsonl, 6 samples (0 to 10 s), one session, 6000-1; subagents-transcripts/ holds its main
transcript (0 to 10 s) and two subagents, X = a0000000000000061 [1, 4] and Y = a0000000000000062
[3, 9]. The main transcript has two completion task-notifications for X and one for Y.
"""
import datetime
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.environ.get("AGENTSPACE_AGENTS") or os.path.join(HERE, "..", "system", "agentspace-agents")
FIX = os.path.join(HERE, "fixtures", "agents")

DIVISOR_WORDS = ("the session's tool calls divided by its working agents: "
                 "its subagents running at that moment plus its main agent")


def rec(name):
    return os.path.join(FIX, name + ".jsonl")


def trans(name):
    return os.path.join(FIX, name + "-transcripts")


def run(*args):
    return subprocess.run([sys.executable, SCRIPT] + list(args), capture_output=True, text=True, timeout=60)


def summarize(name, *extra, folder=None, record=None):
    p = run(record or rec(name), folder or trans(name), "--json", *extra)
    assert p.returncode == 0, "exit %d, stderr %r" % (p.returncode, p.stderr)
    return json.loads(p.stdout)


def markdown(name, *extra):
    p = run(rec(name), trans(name), *extra)
    assert p.returncode == 0, "exit %d, stderr %r" % (p.returncode, p.stderr)
    return p.stdout


def parse_time(s):
    return datetime.datetime.fromisoformat(s.replace("Z", "+00:00"))


NOTES = {
    "skipped_lines": 0,
    "clamped": 0,
    "gaps": 0,
    "missing_end": False,
    "sessions_unjoined": [],
    "join_ties": [],
    "transcripts_unread": [],
    "transcripts_unjoined": [],
    "transcript_lines_malformed": 0,
    "transcript_lines_untimed": 0,
    "subagents_unread": [],
    "subagents_parent_missing": [],
    "subagents_resumed": [],
    "subagents_outside": 0,
}


def md_table(out, heading):
    """The first table under a Markdown heading, as a list of {column header: cell}."""
    lines = out.split("\n")
    k = lines.index(heading)
    rows = []
    for line in lines[k + 1:]:
        s = line.strip()
        if s.startswith("#"):
            break
        if not s.startswith("|"):
            if rows:
                break
            continue
        cells = [c.strip() for c in s.strip("|").split("|")]
        if not all(re.fullmatch(r"-+", c) for c in cells):
            rows.append(cells)
    return [dict(zip(rows[0], r)) for r in rows[1:]]


def lowercase_starts(text):
    """Return the lines a human reads that start with a lowercase letter.

    Headings, bullets and plain lines are checked after their marker. A table is checked only in its
    header row, because a data row's first cell is a label (a folder name, a file name), which the
    capital rule leaves alone.
    """
    bad = []
    lines = text.split("\n")
    for k, line in enumerate(lines):
        s = line.strip()
        if not s:
            continue
        if s.startswith("|"):
            is_header = k + 1 < len(lines) and re.match(r"^\|[\s:|-]+\|$", lines[k + 1].strip())
            if not is_header:
                continue
            cells = [c.strip() for c in s.strip("|").split("|")]
        else:
            cells = [re.sub(r"^(#+\s+|[-*]\s+|\d+\.\s+)", "", s)]
        for c in cells:
            if c and c[0].isalpha() and c[0].islower():
                bad.append(line)
                break
    return bad


class Base(unittest.TestCase):
    def notes(self, j, **changed):
        """The notes object exactly: every key, each value the clean default unless named here."""
        want = dict(NOTES)
        for key in changed:
            self.assertIn(key, NOTES, "unknown notes key %r" % key)
        want.update(changed)
        self.assertEqual(j["notes"], want)

    def stat(self, d, peak, p95, p50, msg=""):
        self.assertIsNotNone(d, msg)
        self.assertEqual(sorted(d.keys()), ["p50", "p95", "peak"], msg)
        for key, want in (("peak", peak), ("p95", p95), ("p50", p50)):
            if want is None:
                self.assertIsNone(d[key], "%s %s" % (msg, key))
            else:
                self.assertIsNotNone(d[key], "%s %s" % (msg, key))
                self.assertAlmostEqual(d[key], want, places=6, msg="%s %s" % (msg, key))

    def pair(self, d, cpu, pss, msg=""):
        """Check a {cpu_cores, pss_mib} object against two (peak, p95, p50) tuples."""
        self.assertIsNotNone(d, msg)
        self.assertEqual(sorted(d.keys()), ["cpu_cores", "pss_mib"], msg)
        self.stat(d["cpu_cores"], *cpu, msg=msg + " cpu")
        self.stat(d["pss_mib"], *pss, msg=msg + " pss")

    def intervals(self, s, with_subagents, alone, msg=""):
        d = s["intervals"]
        self.assertEqual(sorted(d.values()), sorted([with_subagents, alone]), msg)
        names = [k for k in d if "alone" in k.lower() or "main" in k.lower()]
        if names:
            self.assertEqual(d[names[0]], alone, msg)


class Division(Base):
    """Requirement 1, 2, 3, 4, 5: the divisor, main_alone, the statistics, self and host, the span."""

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("division")
        cls.s = cls.j["sessions"][0]

    def test_run_header(self):
        r = self.j["run"]
        self.assertEqual(r["samples"], 9)
        self.assertAlmostEqual(r["interval_s"], 2.0)
        self.assertEqual(r["cpus"], 4)
        # sampler_cpu_s grows 0.02 per 2 s interval: 0.01 cores
        self.assertAlmostEqual(r["sampler_cores"], 0.01, places=6)

    def test_one_session_joined_to_its_transcript(self):
        self.assertEqual(len(self.j["sessions"]), 1)
        self.assertEqual(self.s["session"], "1000-1")
        self.assertEqual(self.s["cwd"], "/home/ubuntu/proj")
        self.assertEqual(self.s["transcript"], "11111111-aaaa-4000-8000-000000000001.jsonl")
        self.assertEqual(self.s["subagents"], 3)

    def test_per_agent_cpu_divides_by_working_agents(self):
        # Tools cores per interval over n_cpu (1, 3, 3, 3, 1, 1, 2, 2):
        #   I1 1.0/1 = 1.0, I2 3.0/3 = 1.0, I3 6.0/3 = 2.0, I4 1.5/3 = 0.5,
        #   I5 0.5/1 = 0.5, I6 2.0/1 = 2.0, I7 5.0/2 = 2.5, I8 3.0/2 = 1.5
        # sorted: 0.5 0.5 1.0 1.0 1.5 2.0 2.0 2.5 (N = 8)
        # peak 2.5; p95 = sorted[ceil(7.6) - 1] = 8th = 2.5; p50 = sorted[ceil(4) - 1] = 4th = 1.0
        self.stat(self.s["per_agent"]["cpu_cores"], 2.5, 2.5, 1.0, "per_agent cpu")

    def test_per_agent_pss_divides_by_working_agents_at_the_instant(self):
        # Tools MiB per sample over n_pss (1, 1, 3, 3, 2, 1, 1, 2, 1):
        #   t0 100/1 = 100, t2 200/1 = 200, t4 300/3 = 100, t6 400/3 = 133.333...,
        #   t8 50/2 = 25, t10 600/1 = 600, t12 700/1 = 700, t14 800/2 = 400, t16 150/1 = 150
        # sorted: 25 100 100 133.33 150 200 400 600 700 (N = 9)
        # peak 700; p95 = sorted[ceil(8.55) - 1] = 9th = 700; p50 = sorted[ceil(4.5) - 1] = 5th = 150
        self.stat(self.s["per_agent"]["pss_mib"], 700.0, 700.0, 150.0, "per_agent pss")

    def test_start_mid_interval_counts_for_cpu_not_for_the_earlier_sample(self):
        # A starts at 3, inside I2 = (2,4]. I2 divides by 3 (A and B), so its 3.0 cores become 1.0.
        # At the sample t = 2 A has not started: n_pss = 1 and 200 MiB stays 200.
        # If I2 ignored A it would divide by 2 and be 1.5; the sorted cpu values would then be
        # 0.5 0.5 1.0 1.5 1.5 2.0 2.0 2.5 with p50 = 4th = 1.5 instead of 1.0.
        self.assertAlmostEqual(self.s["per_agent"]["cpu_cores"]["p50"], 1.0, places=6)
        # If t = 2 wrongly counted A the 200 would be 100 and the sorted pss values would be
        # 25 100 100 100 133.33 150 ..., with p50 = 5th = 133.33 instead of 150.
        self.assertAlmostEqual(self.s["per_agent"]["pss_mib"]["p50"], 150.0, places=6)

    def test_end_exactly_at_a_sample_counts_there_not_in_the_next_interval(self):
        # A ends at t = 8. Sample t = 8: start 3 <= 8 <= 8, so n_pss = 2 and 500 MiB becomes 250.
        # Interval I5 = (8,10]: end 8 > 8 is false, so n_cpu = 1 and its 0.5 cores stay 0.5.
        # main_alone holds I5 undivided: cpu values {I1 1.0, I5 0.5, I6 2.0}.
        # Wrong the first way: if t = 8 did not count A, its 50 MiB would be undivided and join main_alone:
        # sorted 50 100 150 200 600 700 (N = 6), p50 = 3rd = 150 instead of 200 (checked below).
        # Wrong the second way: if I5 counted A, its interval would be n = 2 and the interval counts
        # would be 6 and 2, not 5 and 3.
        self.stat(self.s["main_alone"]["pss_mib"], 700.0, 700.0, 200.0, "main_alone pss")
        self.intervals(self.s, 5, 3)

    def test_main_alone_holds_exactly_the_n1_intervals_and_samples(self):
        # n_cpu = 1 intervals: I1, I5, I6, tools cores 1.0, 0.5, 2.0 undivided.
        # sorted 0.5 1.0 2.0 (N = 3): peak 2.0; p95 = sorted[ceil(2.85) - 1] = 3rd = 2.0;
        # p50 = sorted[ceil(1.5) - 1] = 2nd = 1.0
        self.stat(self.s["main_alone"]["cpu_cores"], 2.0, 2.0, 1.0, "main_alone cpu")
        # n_pss = 1 samples: t0 100, t2 200, t10 600, t12 700, t16 150 (note: not the same set as n_cpu = 1).
        # sorted 100 150 200 600 700 (N = 5): peak 700; p95 = 5th = 700; p50 = sorted[ceil(2.5) - 1] = 3rd = 200
        self.stat(self.s["main_alone"]["pss_mib"], 700.0, 700.0, 200.0, "main_alone pss")

    def test_interval_counts(self):
        # n_cpu >= 2: I2, I3, I4, I7, I8 = 5 intervals; n_cpu = 1: I1, I5, I6 = 3 intervals.
        self.intervals(self.s, 5, 3)

    def test_self_figures(self):
        # Self cpu: deltas 0.2 ... 1.6 s over 2 s = 0.1 ... 0.8 cores, N = 8:
        # peak 0.8; p95 = 8th = 0.8; p50 = 4th = 0.4
        self.stat(self.s["self"]["cpu_cores"], 0.8, 0.8, 0.4, "self cpu")
        # Self pss: 10, 20, ..., 90 MiB, N = 9: peak 90; p95 = 9th = 90; p50 = 5th = 50
        self.stat(self.s["self"]["pss_mib"], 90.0, 90.0, 50.0, "self pss")

    def test_host_figures(self):
        # Host cpu: d_busy 0.5, 1.0, ..., 4.0 over d_total 8.0 on 4 cpus = 0.25 ... 2.0 cores, N = 8:
        # peak 2.0; p95 = 8th = 2.0; p50 = 4th = 1.0
        self.stat(self.j["host"]["cpu_cores"], 2.0, 2.0, 1.0, "host cpu")
        # Host memory: 1 ... 9 GiB, N = 9: peak 9; p95 = 9th = 9; p50 = 5th = 5
        self.stat(self.j["host"]["mem_gib"], 9.0, 9.0, 5.0, "host mem")
        self.assertEqual(self.j["host"]["sessions_peak"], 1)

    def test_subagents_peak_and_when(self):
        # A [3,8] and B [4,7] overlap from 4 to 7: 2 at once, first reached at B's start t = 4 s,
        # 09:00:04Z. D [13,15] never overlaps another: 1. So the peak is 2.
        self.assertEqual(self.j["host"]["subagents_peak"], 2)
        self.assertEqual(parse_time(self.j["host"]["subagents_peak_at"]),
                         datetime.datetime(2026, 11, 18, 9, 0, 4, tzinfo=datetime.timezone.utc))
        self.assertEqual(self.s["subagents_peak"], 2)

    def test_span_is_min_and_max_timestamp_of_the_subagent_file(self):
        # A's lines are, in order, 5, 3, (no timestamp), 8, 6. First and last line would give [5, 6];
        # min and max give [3, 8]. The division numbers above only hold for [3, 8]: with [5, 6],
        # I2 would not divide by 3 and its 3.0 cores would stay 3.0 and be the peak instead of 2.5.
        self.assertAlmostEqual(self.s["per_agent"]["cpu_cores"]["peak"], 2.5, places=6)
        # The A line with no timestamp is passed over for time and counted in the notes (1 line); it is
        # valid JSON, so it is not malformed. Nothing else is noted: the record is clean and has its end line.
        self.notes(self.j, transcript_lines_untimed=1)


class Statistics(Base):
    """Requirement 3: nearest rank, never interpolation."""

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("stats")
        cls.s = cls.j["sessions"][0]

    def test_nearest_rank_cpu(self):
        # Tools cores are a permutation of 1..19, N = 19, no subagent: per_agent = main_alone = the values.
        # peak 19; p95 = sorted[ceil(18.05) - 1] = 19th = 19 (linear interpolation would give 18.1);
        # p50 = sorted[ceil(9.5) - 1] = 10th = 10
        for key in ("per_agent", "main_alone"):
            self.stat(self.s[key]["cpu_cores"], 19.0, 19.0, 10.0, key + " cpu")

    def test_nearest_rank_pss(self):
        # Tools MiB are 10, 20, ..., 200 in some order, N = 20, no subagent.
        # peak 200; p95 = sorted[ceil(19) - 1] = 19th = 190 (interpolation would give 190.5);
        # p50 = sorted[ceil(10) - 1] = 10th = 100 (interpolation would give 105)
        for key in ("per_agent", "main_alone"):
            self.stat(self.s[key]["pss_mib"], 200.0, 190.0, 100.0, key + " pss")

    def test_no_subagents(self):
        self.assertEqual(self.s["subagents"], 0)
        self.assertEqual(self.s["subagents_peak"], 0)
        self.assertEqual(self.j["host"]["subagents_peak"], 0)
        self.intervals(self.s, 0, 19)

    def test_clean_notes(self):
        # A clean record with its end line, one transcript, every line timed: nothing is noted.
        self.notes(self.j)


class Join(Base):
    """Requirement 6: overlap plus the same checkout root, and the tie rules."""

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("join")
        cls.by = dict((s["session"], s) for s in cls.j["sessions"])

    def test_sessions_in_order_of_first_appearance(self):
        self.assertEqual([s["session"] for s in self.j["sessions"]],
                         ["1001-1", "1002-2", "1003-3", "1004-4", "1005-5"])

    def test_worktree_cwd_joins_its_checkouts_transcript(self):
        # 1001-1 starts in /home/ubuntu/projA/.claude/worktrees/w1; cut at /.claude/worktrees/ its root is
        # /home/ubuntu/projA, the root of transcript aaaaaaaa-1111-... (0 to 30 s, overlap 20 s).
        # The projB transcript spans the same time but has another root, so it is not a candidate.
        s = self.by["1001-1"]
        self.assertTrue(s["cwd"].endswith("/.claude/worktrees/w1"), s["cwd"])
        self.assertEqual(s["transcript"], "aaaaaaaa-1111-4000-8000-00000000000a.jsonl")

    def test_only_the_joined_transcripts_subagents_count(self):
        # Its one subagent runs [5, 11]. Intervals (4,6], (6,8], (8,10], (10,12] have n_cpu = 2: 4 of 10.
        # Samples t = 6, 8, 10 have n_pss = 2: 3 of 11. The orphan subagent under projO is not counted.
        s = self.by["1001-1"]
        self.assertEqual(s["subagents"], 1)
        self.assertEqual(s["subagents_peak"], 1)
        self.intervals(s, 4, 6)
        # per_agent cpu: four intervals at 1.0/2 = 0.5, six at 1.0: peak 1.0; p95 = sorted[ceil(9.5) - 1] =
        # 10th = 1.0; p50 = sorted[ceil(5) - 1] = 5th = 1.0 (sorted: 0.5 0.5 0.5 0.5 1.0 ...).
        # per_agent pss: three samples at 100/2 = 50, eight at 100: peak 100, p95 = 11th = 100,
        # p50 = sorted[ceil(5.5) - 1] = 6th = 100.
        self.pair(s["per_agent"], (1.0, 1.0, 1.0), (100.0, 100.0, 100.0), "1001-1 per_agent")
        # main_alone: six intervals at 1.0, eight samples at 100 MiB.
        self.pair(s["main_alone"], (1.0, 1.0, 1.0), (100.0, 100.0, 100.0), "1001-1 main_alone")

    def test_host_subagents_peak_counts_joined_sessions_only(self):
        self.assertEqual(self.j["host"]["subagents_peak"], 1)
        # SA [5, 11] is the only joined subagent. It starts at 5 s, between the samples at 4 and 6 s; the
        # peak is read at a sample (spec section 6: "the first sampled instant"), so the first time it is
        # seen is the sample at 6 s, 09:00:06Z. (In Division the start falls on a sample, so both readings agree.)
        self.assertEqual(parse_time(self.j["host"]["subagents_peak_at"]),
                         datetime.datetime(2026, 11, 18, 9, 0, 6, tzinfo=datetime.timezone.utc))
        self.assertEqual(self.j["host"]["sessions_peak"], 5)

    def test_no_candidate_is_unjoined_but_keeps_its_self_figures(self):
        # 1002-2 is in /home/ubuntu/projC. One projC transcript is 600 to 610 s (no overlap), the other
        # 19.5 to 40 s (overlap with 0 to 20 s is 0.5 s, under the 1 s minimum).
        s = self.by["1002-2"]
        self.assertIsNone(s["transcript"])
        self.assertIsNone(s["per_agent"])
        self.assertIsNone(s["main_alone"])
        # Self cpu 0.2 s per 2 s = 0.1 cores in all 10 intervals; self pss 10 MiB in all 11 samples.
        self.pair(s["self"], (0.1, 0.1, 0.1), (10.0, 10.0, 10.0), "1002-2 self")
        # Not pinned whether an unjoined session counts 0 or null subagents: either is accepted.
        self.assertIn(s["subagents"], (0, None))
        self.assertIn(s["subagents_peak"], (0, None))
        self.assertEqual(self.j["notes"]["sessions_unjoined"], ["1002-2"])

    def test_tie_broken_by_smaller_file_name(self):
        # 1003-3: two projT transcripts, both 0 to 20 s, the same root. Overlap 20 s each, one matching
        # root each: the smaller file name, aaaaaaaa-..., wins.
        s = self.by["1003-3"]
        self.assertEqual(s["transcript"], "aaaaaaaa-2222-4000-8000-0000000000a1.jsonl")
        self.assertIn({"session": "1003-3", "transcript": "aaaaaaaa-2222-4000-8000-0000000000a1.jsonl",
                       "tied_with": ["bbbbbbbb-2222-4000-8000-0000000000b2.jsonl"], "decided_by": "name"},
                      self.j["notes"]["join_ties"])

    def test_tie_broken_by_more_matching_roots(self):
        # 1004-4 is in projU, then projV. Two transcripts, 0 to 20 s each: aaaaaaaa-3333 has only projU
        # (1 matching root), bbbbbbbb-3333 has projU and projV (2). More roots beats the smaller name.
        s = self.by["1004-4"]
        self.assertEqual(s["transcript"], "bbbbbbbb-3333-4000-8000-0000000000b3.jsonl")

    def test_most_overlap_wins_over_the_file_name(self):
        # 1005-5: zzzzzzzz-4444 is 0 to 20 s (overlap 20 s), aaaaaaaa-4444 is 10 to 40 s (overlap 10 s).
        # Not a tie (10 s apart): the larger overlap wins, although its name sorts later.
        s = self.by["1005-5"]
        self.assertEqual(s["transcript"], "zzzzzzzz-4444-4000-8000-0000000000e4.jsonl")

    def test_joined_sessions_without_subagents_have_per_agent_equal_tools(self):
        # Tools: 2.0 s over 2 s = 1.0 core in all 10 intervals, 100 MiB in all 11 samples; n = 1 throughout.
        for sid in ("1003-3", "1004-4", "1005-5"):
            s = self.by[sid]
            self.pair(s["per_agent"], (1.0, 1.0, 1.0), (100.0, 100.0, 100.0), sid + " per_agent")
            self.pair(s["main_alone"], (1.0, 1.0, 1.0), (100.0, 100.0, 100.0), sid + " main_alone")
            self.pair(s["self"], (0.1, 0.1, 0.1), (10.0, 10.0, 10.0), sid + " self")
            self.intervals(s, 0, 10, sid)

    def test_orphan_subagent_is_noted(self):
        # projO holds a subagent file whose parent main transcript does not exist. It is named after its
        # lines' sessionId, 99999999-..., and stands as a transcript of its own, which no session joins.
        self.assertEqual(self.j["notes"]["subagents_parent_missing"],
                         ["99999999-9999-4000-8000-000000000009.jsonl/agent-a000000000000orph.jsonl"])

    def test_notes_exactly(self):
        # 1002-2 has no candidate. 1003-3 is a tie of overlap and roots broken by the name; 1004-4 a tie of
        # overlap broken by roots. 1005-5's two candidates are 10 s apart: no tie. Joined: aaaaaaaa-1111
        # (1001-1), aaaaaaaa-2222 (1003-3), bbbbbbbb-3333 (1004-4), zzzzzzzz-4444 (1005-5). The other seven
        # of the eleven transcripts (ten files and the orphan's stand-in) are joined to no session.
        self.notes(
            self.j,
            sessions_unjoined=["1002-2"],
            join_ties=[
                {"session": "1003-3", "transcript": "aaaaaaaa-2222-4000-8000-0000000000a1.jsonl",
                 "tied_with": ["bbbbbbbb-2222-4000-8000-0000000000b2.jsonl"], "decided_by": "name"},
                {"session": "1004-4", "transcript": "bbbbbbbb-3333-4000-8000-0000000000b3.jsonl",
                 "tied_with": ["aaaaaaaa-3333-4000-8000-0000000000a3.jsonl"], "decided_by": "roots"},
            ],
            transcripts_unjoined=[
                "99999999-9999-4000-8000-000000000009.jsonl",
                "aaaaaaaa-3333-4000-8000-0000000000a3.jsonl",
                "aaaaaaaa-4444-4000-8000-0000000000a4.jsonl",
                "bbbbbbbb-1111-4000-8000-00000000000b.jsonl",
                "bbbbbbbb-2222-4000-8000-0000000000b2.jsonl",
                "cccccccc-1111-4000-8000-00000000000c.jsonl",
                "cccccccc-2222-4000-8000-00000000000d.jsonl",
            ],
            subagents_parent_missing=["99999999-9999-4000-8000-000000000009.jsonl/agent-a000000000000orph.jsonl"],
        )


class Tie(Base):
    """Markdown rounds a decimal tie up, judged on the decimal, not on the float's binary form."""

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("tie")
        cls.row = md_table(markdown("tie"), "### Per working agent")[0]

    def test_a_tie_stored_just_below_rounds_up(self):
        # Interval (0,2]: all three subagents run ([0.5, 1.5]: 0.5 <= 2 and 1.5 > 0), so n_cpu = 4.
        # Tools (10.84 - 10.0) CPU-s / 2 s / 4 = 0.84 / 8 = 0.105 cores, an exact tie at 2 places, the real
        # turn's per-agent peak in miniature. The float holds 0.10499999999999998, just below it, so
        # "%.2f" prints 0.10; half up on the decimal prints 0.11.
        value = self.j["sessions"][0]["per_agent"]["cpu_cores"]["peak"]
        self.assertAlmostEqual(value, 0.105, places=9)
        self.assertLess(value, 0.105)
        self.assertEqual("%.2f" % value, "0.10")
        self.assertEqual(self.row["CPU cores peak"], "0.11")
        self.assertEqual(self.row["CPU cores p95"], "0.11")
        self.assertEqual(self.row["CPU cores p50"], "0.11")

    def test_an_exact_binary_tie_rounds_up_not_to_even(self):
        # No subagent runs at t = 0 or 2 (they run [0.5, 1.5]), so n_pss = 1: 1152 KiB / 1024 = 1.125 MiB,
        # held exactly by a float. "%.2f" rounds that tie to even, 1.12; half up is 1.13.
        value = self.j["sessions"][0]["per_agent"]["pss_mib"]["peak"]
        self.assertEqual(value, 1.125)
        self.assertEqual("%.2f" % value, "1.12")
        self.assertEqual(self.row["PSS MiB peak"], "1.13")

    def test_json_keeps_the_raw_float(self):
        self.assertEqual(self.j["sessions"][0]["per_agent"]["cpu_cores"]["peak"], (10.84 - 10.0) / 2 / 4)


class Edges(Base):
    """The record's edge cases, each against edges-transcripts/ (one main transcript, no subagents)."""

    def edge(self, name):
        return summarize(name, folder=trans("edges"), record=rec(name))

    def test_session_rows_are_timed_by_their_host_line(self):
        # hostt: host lines at t = 0, 2, 4, 6 s; the session's lines say 0, 2, 5, 6 (the third is 1 s
        # after its host line). Tools CPU-s 100, 102, 104, 106; self CPU-s 10.0, 10.2, 10.4, 10.6.
        # By the host lines: three 2 s intervals, tools 1.0, 1.0, 1.0 cores, self 0.1, 0.1, 0.1.
        # By the session's own t the intervals would be 2, 3 and 1 s: tools 1.0, 0.667, 2.0 (peak 2.0,
        # p50 1.0) and self 0.1, 0.067, 0.2 (peak 0.2).
        j = self.edge("hostt")
        s = j["sessions"][0]
        self.assertEqual(s["transcript"], "55555555-aaaa-4000-8000-000000000005.jsonl")
        self.stat(s["per_agent"]["cpu_cores"], 1.0, 1.0, 1.0, "per_agent cpu")
        self.stat(s["main_alone"]["cpu_cores"], 1.0, 1.0, 1.0, "main_alone cpu")
        self.stat(s["self"]["cpu_cores"], 0.1, 0.1, 0.1, "self cpu")
        self.intervals(s, 0, 3)
        self.notes(j)

    def test_a_falling_counter_is_clamped_to_0_and_counted(self):
        # clamp: samples at 0, 2, 4 s; tools CPU-s 100, 99, 101. Interval 1 falls by 1: clamped to 0 cores,
        # and counted. Interval 2: 2 CPU-s / 2 s = 1.0. Sorted 0.0 1.0 (N = 2): peak 1.0; p95 =
        # sorted[ceil(1.9) - 1] = 2nd = 1.0; p50 = sorted[ceil(1.0) - 1] = 1st = 0.0. Unclamped, interval 1
        # would be -1 / 2 = -0.5 and p50 -0.5. Every other counter rises, so the count is exactly 1.
        j = self.edge("clamp")
        s = j["sessions"][0]
        self.stat(s["per_agent"]["cpu_cores"], 1.0, 1.0, 0.0, "per_agent cpu")
        self.stat(s["main_alone"]["cpu_cores"], 1.0, 1.0, 0.0, "main_alone cpu")
        self.intervals(s, 0, 2)
        self.notes(j, clamped=1)

    def test_an_interval_longer_than_three_is_skipped_and_counted(self):
        # gap: samples at 0, 2, 4, 10, 18, 20 s: intervals 2, 2, 6, 8, 2 s. The limit is 3 * 2 = 6 s: the
        # 6 s interval is not longer, so it is kept; the 8 s one is a gap, skipped once for the host and
        # once for the session: gaps = 2. Tools CPU-s 100, 102, 104, 116, 156, 158: kept intervals
        # 2/2 = 1.0, 2/2 = 1.0, 12/6 = 2.0, 2/2 = 1.0, sorted 1.0 1.0 1.0 2.0 (N = 4): peak 2.0; p95 =
        # sorted[ceil(3.8) - 1] = 4th = 2.0; p50 = sorted[ceil(2) - 1] = 2nd = 1.0. The gap's 40/8 = 5.0
        # would be the peak if kept; the 6 s interval's 2.0 would be lost if 6 s counted as a gap.
        j = self.edge("gap")
        s = j["sessions"][0]
        self.stat(s["per_agent"]["cpu_cores"], 2.0, 2.0, 1.0, "per_agent cpu")
        self.stat(s["main_alone"]["cpu_cores"], 2.0, 2.0, 1.0, "main_alone cpu")
        self.intervals(s, 0, 4)
        self.notes(j, gaps=2)

    def test_a_missing_end_line_is_noted(self):
        # noend: two samples and no end line. The figures are still read: tools 2 CPU-s / 2 s = 1.0.
        j = self.edge("noend")
        self.stat(j["sessions"][0]["per_agent"]["cpu_cores"], 1.0, 1.0, 1.0, "per_agent cpu")
        self.notes(j, missing_end=True)
        p = run(rec("noend"), trans("edges"))
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("- No end line: the record was summarized up to its last complete line.", p.stdout)


class Subagents(Base):
    """The subagent peak's closed end, and the resumed note."""

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("subagents")
        cls.s = cls.j["sessions"][0]

    def test_a_span_ending_exactly_on_a_sample_counts_at_it(self):
        # X [1, 4], Y [3, 9]; samples at 0, 2, ..., 10 s. Running at each, start <= t <= end:
        #   t=0 none 0   t=2 X 1   t=4 X (4 <= 4) and Y 2   t=6 Y 1   t=8 Y 1   t=10 none 0
        # The peak is 2, first at t = 4 s, 09:00:04Z, only because X's end at 4 counts there. With an
        # open end (t < end) t = 4 would hold Y alone and the peak would be 1, first at t = 2 s.
        self.assertEqual(self.j["host"]["subagents_peak"], 2)
        self.assertEqual(parse_time(self.j["host"]["subagents_peak_at"]),
                         datetime.datetime(2026, 11, 18, 9, 0, 4, tzinfo=datetime.timezone.utc))
        self.assertEqual(self.s["subagents"], 2)
        self.assertEqual(self.s["subagents_peak"], 2)
        # PSS: tools 100 MiB over n_pss 1, 2, 3, 2, 2, 1 = 100, 50, 33.33, 50, 50, 100; sorted
        # 33.33 50 50 50 100 100 (N = 6): peak 100; p95 = sorted[ceil(5.7) - 1] = 6th = 100;
        # p50 = sorted[ceil(3) - 1] = 3rd = 50.
        self.stat(self.s["per_agent"]["pss_mib"], 100.0, 100.0, 50.0, "per_agent pss")

    def test_two_completion_notices_for_one_task_note_it_resumed(self):
        # X has two completion task-notifications in the main transcript (at 2.005 and 4.005 s), Y one.
        # So X alone is noted, named <main transcript>/<its file>. Its span is not split: still [1, 4].
        self.notes(self.j, subagents_resumed=[
            "66666666-aaaa-4000-8000-000000000006.jsonl/agent-a0000000000000061.jsonl"])
        p = run(rec("subagents"), trans("subagents"))
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("Resumed, with more than one completion notice", p.stdout)


class Window(Base):
    """Requirement 7: --from and --to."""

    def test_from_to_keep_the_samples_inside_both_ends_included(self):
        # Window 09:00:04Z to 09:00:10Z, written with a +00:00 offset (the record is -08:00):
        # samples t = 4, 6, 8, 10 = 4 samples, intervals I3 (4,6], I4 (6,8], I5 (8,10].
        j = summarize("division", "--from", "2026-11-18T09:00:04+00:00", "--to", "2026-11-18T09:00:10+00:00")
        self.assertEqual(j["run"]["samples"], 4)
        s = j["sessions"][0]
        # n_cpu = 3, 3, 1: I3 6.0/3 = 2.0, I4 1.5/3 = 0.5, I5 0.5/1 = 0.5.
        # sorted 0.5 0.5 2.0: peak 2.0; p95 = 3rd = 2.0;
        # p50 = sorted[ceil(1.5) - 1] = 2nd = 0.5
        self.stat(s["per_agent"]["cpu_cores"], 2.0, 2.0, 0.5, "window per_agent cpu")
        # n_pss at t = 4, 6, 8, 10 = 3, 3, 2, 1: 300/3 = 100, 400/3 = 133.33, 50/2 = 25, 600/1 = 600.
        # sorted 25 100 133.33 600 (N = 4): peak 600; p95 = 4th = 600; p50 = sorted[ceil(2) - 1] = 2nd = 100
        self.stat(s["per_agent"]["pss_mib"], 600.0, 600.0, 100.0, "window per_agent pss")
        # main_alone: interval I5 only (0.5 cores), sample t = 10 only (600 MiB).
        self.pair(s["main_alone"], (0.5, 0.5, 0.5), (600.0, 600.0, 600.0), "window main_alone")
        self.intervals(s, 2, 1)
        # Spec rule 5: a subagent counts for a session whose (windowed) range it overlaps. The session now
        # runs 4 to 10 s: A [3,8] and B [4,7] overlap it, D [13,15] does not. So 2, and B still divides I3.
        self.assertEqual(s["subagents"], 2)

    def test_window_with_an_offset_that_is_not_the_records(self):
        # 01:00:04-08:00 is 09:00:04Z: the same window as above.
        j = summarize("division", "--from", "2026-11-18T01:00:04-08:00", "--to", "2026-11-18T01:00:10-08:00")
        self.assertEqual(j["run"]["samples"], 4)


class Errors(Base):
    def assertUsageError(self, p, msg=""):
        self.assertEqual(p.returncode, 2, "%s stdout %r stderr %r" % (msg, p.stdout, p.stderr))
        self.assertTrue(p.stderr.strip(), msg)
        self.assertEqual(p.stdout, "", msg)

    def test_missing_record(self):
        self.assertUsageError(run(os.path.join(FIX, "no-such.jsonl"), trans("division")), "missing record")
        self.assertUsageError(run(os.path.join(FIX, "no-such.jsonl"), trans("division"), "--json"), "missing record, json")

    def test_missing_folder(self):
        self.assertUsageError(run(rec("division"), os.path.join(FIX, "no-such-folder")), "missing folder")

    def test_from_without_offset(self):
        self.assertUsageError(run(rec("division"), trans("division"), "--from", "2026-11-18T09:00:04"),
                              "--from without an offset")
        self.assertUsageError(run(rec("division"), trans("division"), "--to", "2026-11-18T09:00:04"),
                              "--to without an offset")

    def test_option_without_value_and_bad_arguments(self):
        self.assertUsageError(run(rec("division"), trans("division"), "--from"), "--from without a value")
        self.assertUsageError(run(), "no arguments")
        self.assertUsageError(run(rec("division")), "no folder")
        self.assertUsageError(run(rec("division"), trans("division"), "--bogus"), "unknown option")

    def test_record_without_a_start_line(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "nostart.jsonl")
            with open(path, "w", newline="\n") as f:
                f.write('{"kind":"host","t":"2026-11-18T01:00:00.000-08:00"}\n')
            self.assertUsageError(run(path, trans("division")), "no start line")


class Malformed(Base):
    def test_malformed_record_lines_are_skipped_and_noted(self):
        # The stats record with two bad lines inserted (not JSON; JSON but not an object): the figures
        # are those of Statistics, and the notes count 2 skipped lines.
        with open(rec("stats"), newline="") as f:
            lines = f.read().split("\n")
        lines.insert(5, "this is not json")
        lines.insert(9, "[1, 2, 3]")
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "bad.jsonl")
            with open(path, "w", newline="\n") as f:
                f.write("\n".join(lines))
            j = summarize("stats", record=path)
        s = j["sessions"][0]
        self.stat(s["per_agent"]["cpu_cores"], 19.0, 19.0, 10.0, "per_agent cpu")
        self.stat(s["per_agent"]["pss_mib"], 200.0, 190.0, 100.0, "per_agent pss")
        self.notes(j, skipped_lines=2)


class Markdown(Base):
    """Requirement 8."""

    @classmethod
    def setUpClass(cls):
        cls.out = markdown("division")
        cls.join_out = markdown("join")

    def test_states_the_divisor_in_the_pinned_words(self):
        flat = " ".join(self.out.split()).lower()
        self.assertIn(DIVISOR_WORDS, flat)

    def test_names_the_transcript_and_the_unjoined_session(self):
        self.assertIn("11111111-aaaa-4000-8000-000000000001", self.out)
        self.assertRegex(self.join_out.lower(), r"\bnone\b|unjoined|no transcript|not joined")

    def test_figures_to_two_decimals(self):
        # per_agent cpu peak 2.5 and pss peak 700 print as 2.50 and 700.00.
        self.assertIn("2.50", self.out)
        self.assertIn("700.00", self.out)

    def test_time_is_h_mm_am_pm(self):
        # The record runs 1:00am to 1:00am, -08:00.
        self.assertRegex(self.out, r"\b1:00am\b")

    def test_not_json(self):
        with self.assertRaises(ValueError):
            json.loads(self.out)

    def test_the_capital_check_can_find_a_lowercase_start(self):
        # Prove the instrument before trusting a clean result: each known-bad text must be caught.
        self.assertEqual(lowercase_starts("- lowercase bullet"), ["- lowercase bullet"])
        self.assertEqual(lowercase_starts("## lowercase heading"), ["## lowercase heading"])
        self.assertEqual(lowercase_starts("lowercase line"), ["lowercase line"])
        self.assertEqual(len(lowercase_starts("| Good | bad header |\n| --- | --- |\n| row | x |")), 1)
        # And known-good text is not flagged: capitals, digits, a data row's label.
        self.assertEqual(lowercase_starts("- Fine\n## Fine\n49 tagged\n| Good | Fine |\n| --- | --- |\n| proj 1-1 | 2 |"), [])

    def test_every_line_a_human_reads_starts_with_a_capital(self):
        for name, out in (("division", self.out), ("join", self.join_out)):
            self.assertEqual(lowercase_starts(out), [], name)


if __name__ == "__main__":
    unittest.main()

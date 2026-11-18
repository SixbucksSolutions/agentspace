"""Tests for agentspace-footprint (E0.03-T01-S02, card #0070).

RFC 2119 keywords; the capitals are load-bearing.

The suite drives the script as a subprocess and MUST NOT import it. The path comes from the
environment variable AGENTSPACE_FOOTPRINT, defaulting to ../system/agentspace-footprint relative
to this file. Standard library only, so it runs on the laptop and on the box alike.

Every expected figure is worked by hand from a small fixture in fixtures/, and the hand
calculation sits in a comment beside it. Nearest rank: value[ceil(p * N) - 1] over the sorted
values. Intervals are 2 s unless a fixture says otherwise.
"""
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.environ.get("AGENTSPACE_FOOTPRINT") or os.path.join(HERE, "..", "system", "agentspace-footprint")
FIX = os.path.join(HERE, "fixtures")


def fx(name):
    return os.path.join(FIX, "footprint-" + name + ".jsonl")


def run(*args):
    return subprocess.run([sys.executable, SCRIPT] + list(args), capture_output=True, text=True, timeout=60)


def run_env(env, *args):
    return subprocess.run([sys.executable, SCRIPT] + list(args), capture_output=True, text=True, timeout=60,
                          env=dict(os.environ, **env))


def summarize(name, *extra):
    p = run(fx(name), "--json", *extra)
    assert p.returncode == 0, "exit %d, stderr %r" % (p.returncode, p.stderr)
    return json.loads(p.stdout)


def markdown(name, *extra):
    p = run(fx(name), *extra)
    assert p.returncode == 0, "exit %d, stderr %r" % (p.returncode, p.stderr)
    return p.stdout


class Base(unittest.TestCase):
    def stat(self, d, peak, p95, p50, msg=""):
        """Check a {peak, p95, p50} object; None means all three are null."""
        self.assertEqual(sorted(d.keys()), ["p50", "p95", "peak"], msg)
        for key, want in (("peak", peak), ("p95", p95), ("p50", p50)):
            if want is None:
                self.assertIsNone(d[key], "%s %s" % (msg, key))
            else:
                self.assertIsNotNone(d[key], "%s %s" % (msg, key))
                self.assertAlmostEqual(d[key], want, places=6, msg="%s %s" % (msg, key))

    def notes(self, j, skipped=0, clamped=0, gaps=0, missing_end=False):
        self.assertEqual(
            j["notes"],
            {"skipped_lines": skipped, "clamped": clamped, "gaps": gaps, "missing_end": missing_end})


class TwoSessions(Base):
    """21 samples, 20 intervals of 2 s, 4 CPUs, two sessions in every sample.

    Each series is a permutation of 1..N scaled, so the sorted values are known:
    N = 20 intervals: peak is the 20th, p95 the 19th (ceil(19) = 19), p50 the 10th.
    N = 21 samples:   peak is the 21st, p95 the 20th (ceil(19.95) = 20), p50 the 11th (ceil(10.5)).
    """

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("two-sessions")

    def test_run_header(self):
        r = self.j["run"]
        self.assertEqual(r["run"], "two-sessions")
        self.assertEqual(r["start"], "2026-11-18T01:45:00.000-08:00")
        self.assertEqual(r["end"], "2026-11-18T01:45:40.000-08:00")  # 20 intervals of 2 s
        self.assertEqual(r["utc_offset"], "-08:00")
        self.assertEqual(r["samples"], 21)
        self.assertAlmostEqual(r["interval_s"], 2.0)
        self.assertEqual(r["cpus"], 4)
        # sampler_cpu_s grows 0.02 per sample: 0.4 s over 40 s = 0.01 cores
        self.assertAlmostEqual(r["sampler_overhead_cores"], 0.01, places=6)

    def test_top_level_keys(self):
        self.assertEqual(sorted(self.j.keys()), ["concurrency", "host", "notes", "run", "sessions"])

    def test_host(self):
        # cpu: d_busy = 0.2 * k (k a permutation of 1..20), d_total = 8.0 (4 cpus * 2 s)
        #   cores = 4 * 0.2k / 8 = 0.1k, so sorted 0.1 .. 2.0: peak 2.0, p95 = 19th = 1.9, p50 = 10th = 1.0
        self.stat(self.j["host"]["cpu_cores"], 2.0, 1.9, 1.0, "host cpu")
        # memory: mem_used = 0.5 GiB * q (q a permutation of 1..21), 21 samples
        #   peak 21 * 0.5 = 10.5, p95 = 20th = 10.0, p50 = 11th = 5.5
        self.stat(self.j["host"]["mem_used_gib"], 10.5, 10.0, 5.5, "host mem")

    def test_session_a(self):
        s = self.j["sessions"][0]
        self.assertEqual(s["session"], "12345-67890")
        self.assertEqual(s["cwd"], "/home/ubuntu/projA")
        self.assertEqual(s["label"], "projA 12345-67")  # last folder, first 8 chars of the session
        self.assertEqual(s["first_seen"], "2026-11-18T01:45:00.000-08:00")
        self.assertEqual(s["last_seen"], "2026-11-18T01:45:40.000-08:00")
        self.assertEqual(s["samples"], 21)
        # self cpu: d = 0.1 * k over 2 s = 0.05k cores: sorted 0.05 .. 1.0: peak 1.0, p95 0.95, p50 0.5
        self.stat(s["self"]["cpu_cores"], 1.0, 0.95, 0.5, "A self cpu")
        # self pss: 10 MiB * r, r in 1..21: peak 210, p95 = 20th = 200, p50 = 11th = 110
        self.stat(s["self"]["pss_mib"], 210.0, 200.0, 110.0, "A self pss")
        # tools cpu: d = 0.5 * k over 2 s = 0.25k cores: peak 5.0, p95 = 19th = 4.75, p50 = 10th = 2.5
        self.stat(s["tools"]["cpu_cores"], 5.0, 4.75, 2.5, "A tools cpu")
        # tools pss: 100 MiB * r, r in 1..21: peak 2100, p95 2000, p50 1100
        self.stat(s["tools"]["pss_mib"], 2100.0, 2000.0, 1100.0, "A tools pss")
        # procs = (j mod 9) + 1 over j = 0..20, so the peak is 9
        self.assertEqual(s["tools"]["procs"], {"peak": 9})

    def test_session_b(self):
        s = self.j["sessions"][1]
        self.assertEqual(s["session"], "22222-98765")
        self.assertEqual(s["label"], "projB 22222-98")
        self.assertEqual(s["samples"], 21)
        # self cpu: d = 0.04 * k over 2 s = 0.02k cores: peak 0.4, p95 = 19th = 0.38, p50 = 10th = 0.2
        self.stat(s["self"]["cpu_cores"], 0.4, 0.38, 0.2, "B self cpu")
        # self pss: 5 MiB * u, u in 1..21: peak 105, p95 100, p50 55
        self.stat(s["self"]["pss_mib"], 105.0, 100.0, 55.0, "B self pss")
        # tools cpu: d = 0.25 * k over 2 s = 0.125k cores: peak 2.5, p95 2.375, p50 1.25
        self.stat(s["tools"]["cpu_cores"], 2.5, 2.375, 1.25, "B tools cpu")
        # tools pss: 50 MiB * r, r in 1..21: peak 1050, p95 1000, p50 550
        self.stat(s["tools"]["pss_mib"], 1050.0, 1000.0, 550.0, "B tools pss")
        # procs = (j mod 4) + 1: peak 4
        self.assertEqual(s["tools"]["procs"], {"peak": 4})

    def test_concurrency(self):
        c = self.j["concurrency"]
        self.assertEqual(c["sessions_peak"], 2)
        # tools cpu sum at each of the 20 samples after the first: 0.25k + 0.125k = 0.375k
        #   peak 20 * 0.375 = 7.5, p95 = 19th = 7.125, p50 = 10th = 3.75
        self.stat(c["tools_cpu_cores"], 7.5, 7.125, 3.75, "concurrency cpu")
        # tools pss sum at each of 21 samples: (100 + 50) * r = 150r: peak 3150, p95 3000, p50 1650
        self.stat(c["tools_pss_mib"], 3150.0, 3000.0, 1650.0, "concurrency pss")

    def test_notes_clean(self):
        self.notes(self.j)

    def test_markdown_header_and_order(self):
        md = markdown("two-sessions")
        self.assertIn("two-sessions", md)
        # first and last host lines are 40 s apart, both in the 1:45 minute
        self.assertIn("1:45am to 1:45am (-08:00)", md)
        # sampler_cpu_s 0.4 over 40 s = 0.01 cores = 1.00% of one core, 4 places and 2 places
        self.assertIn("Sampler overhead: 0.0100 cores (1.00% of one core)", md)
        self.assertRegex(md, r"Interval: 2 seconds")
        pos = [md.index(h) for h in ("## Host", "## Sessions", "## Concurrency", "## Notes")]
        self.assertEqual(pos, sorted(pos))
        self.assertEqual(sections(md)["notes"].strip(), "None.")


class Lifecycle(Base):
    """11 samples, 2 CPUs, interval 2 s. Session alpha lives in all 11; beta appears at seq 4
    (8 samples, 7 intervals); gamma ends after seq 6 (6 samples, 5 intervals)."""

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("lifecycle")
        cls.by = {s["session"]: s for s in cls.j["sessions"]}

    def test_host_constant(self):
        # d_busy 2.0, d_total 4.0, 2 cpus: 2 * 2 / 4 = 1.0 core in every interval
        self.stat(self.j["host"]["cpu_cores"], 1.0, 1.0, 1.0)
        self.stat(self.j["host"]["mem_used_gib"], 1.0, 1.0, 1.0)  # 1048576 KiB = 1 GiB
        self.assertEqual(self.j["run"]["samples"], 11)

    def test_order_of_first_appearance(self):
        self.assertEqual([s["session"] for s in self.j["sessions"]], ["6001-100000", "6003-300000", "6002-200000"])  # gamma shows at seq 1, beta at seq 4

    def test_full_session(self):
        s = self.by["6001-100000"]
        self.assertEqual(s["label"], "alpha 6001-100")
        self.assertEqual(s["samples"], 11)
        self.stat(s["self"]["cpu_cores"], 0.1, 0.1, 0.1)   # 0.2 s over 2 s
        self.stat(s["tools"]["cpu_cores"], 1.0, 1.0, 1.0)  # 2.0 s over 2 s
        self.stat(s["self"]["pss_mib"], 40.0, 40.0, 40.0)
        self.stat(s["tools"]["pss_mib"], 100.0, 100.0, 100.0)
        self.assertEqual(s["tools"]["procs"], {"peak": 2})

    def test_appears_mid_run(self):
        s = self.by["6002-200000"]
        self.assertEqual(s["label"], "beta 6002-200")
        self.assertEqual(s["samples"], 8)  # seq 4 to 11
        self.assertEqual(s["first_seen"], "2026-11-18T02:00:06.000-08:00")  # seq 4 = 3 * 2 s
        self.assertEqual(s["last_seen"], "2026-11-18T02:00:20.000-08:00")   # seq 11 = 10 * 2 s
        self.stat(s["self"]["cpu_cores"], 0.3, 0.3, 0.3)  # 0.6 s over 2 s, 7 intervals
        self.stat(s["self"]["pss_mib"], 20.0, 20.0, 20.0)
        # tools cpu: 7 intervals at 0.5, 1.0 .. 3.5 cores: peak 3.5, p95 = ceil(6.65) = 7th = 3.5,
        #   p50 = ceil(3.5) = 4th = 2.0
        self.stat(s["tools"]["cpu_cores"], 3.5, 3.5, 2.0)
        # tools pss: 10 .. 80 MiB over its 8 samples: peak 80, p95 = ceil(7.6) = 8th = 80, p50 = 4th = 40
        self.stat(s["tools"]["pss_mib"], 80.0, 80.0, 40.0)
        self.assertEqual(s["tools"]["procs"], {"peak": 8})

    def test_ends_early(self):
        s = self.by["6003-300000"]
        self.assertEqual(s["label"], "gamma 6003-300")
        self.assertEqual(s["samples"], 6)  # seq 1 to 6
        self.assertEqual(s["first_seen"], "2026-11-18T02:00:00.000-08:00")
        self.assertEqual(s["last_seen"], "2026-11-18T02:00:10.000-08:00")  # seq 6 = 5 * 2 s
        self.stat(s["self"]["cpu_cores"], 0.2, 0.2, 0.2)  # 0.4 s over 2 s, 5 intervals
        self.stat(s["self"]["pss_mib"], 30.0, 30.0, 30.0)
        # tools cpu: 5 intervals at 0.2 .. 1.0 cores: peak 1.0, p95 = ceil(4.75) = 5th = 1.0, p50 = 3rd = 0.6
        self.stat(s["tools"]["cpu_cores"], 1.0, 1.0, 0.6)
        # tools pss: 100 .. 600 MiB over 6 samples: peak 600, p95 = ceil(5.7) = 6th = 600, p50 = 3rd = 300
        self.stat(s["tools"]["pss_mib"], 600.0, 600.0, 300.0)
        self.assertEqual(s["tools"]["procs"], {"peak": 3})

    def test_concurrency(self):
        c = self.j["concurrency"]
        self.assertEqual(c["sessions_peak"], 3)  # seq 4, 5 and 6 have all three
        # tools cpu summed over the sessions' intervals ending at seq 2 .. 11 (10 values):
        #   alpha 1.0 always; gamma 0.2(i-1) for i = 2..6, else 0; beta 0.5(i-4) for i = 5..11, else 0 (none at 4)
        #   i=2 1.2, 3 1.4, 4 1.6, 5 1+0.8+0.5 = 2.3, 6 1+1.0+1.0 = 3.0, 7 2.5, 8 3.0, 9 3.5, 10 4.0, 11 4.5
        #   sorted 1.2 1.4 1.6 2.3 2.5 3.0 3.0 3.5 4.0 4.5: peak 4.5, p95 = ceil(9.5) = 10th = 4.5, p50 = 5th = 2.5
        self.stat(c["tools_cpu_cores"], 4.5, 4.5, 2.5)
        # tools pss summed per sample (11): alpha 100; gamma 100*seq for seq <= 6; beta 10*(seq-3) for seq >= 4
        #   seq 1 200, 2 300, 3 400, 4 510, 5 620, 6 730, 7 140, 8 150, 9 160, 10 170, 11 180
        #   sorted 140 150 160 170 180 200 300 400 510 620 730: peak 730, p95 = ceil(10.45) = 11th = 730,
        #   p50 = ceil(5.5) = 6th = 200
        self.stat(c["tools_pss_mib"], 730.0, 730.0, 200.0)

    def test_notes_clean(self):
        self.notes(self.j)


class Anomalies(Base):
    """The clean fixture: 11 samples, 2 CPUs, one session.

    Host cpu: d_busy 1.0, d_total 4.0, 2 cpus = 0.5 cores every interval. Memory 0.5 GiB * seq.
    Tools cpu over interval i (seq i to i + 1) = 0.2 * i / 2 s = 0.1i cores. Tools pss 100 MiB * seq.
    """

    @classmethod
    def setUpClass(cls):
        cls.clean = summarize("clean")

    def test_clean_numbers(self):
        j = self.clean
        self.notes(j)
        self.stat(j["host"]["cpu_cores"], 0.5, 0.5, 0.5)
        # mem 0.5 .. 5.5 GiB, 11 samples: peak 5.5, p95 = ceil(10.45) = 11th = 5.5, p50 = 6th = 3.0
        self.stat(j["host"]["mem_used_gib"], 5.5, 5.5, 3.0)
        s = j["sessions"][0]
        self.assertEqual(s["label"], "proj 4242-100")
        self.stat(s["self"]["cpu_cores"], 0.1, 0.1, 0.1)  # 0.2 s over 2 s
        self.stat(s["self"]["pss_mib"], 10.0, 10.0, 10.0)
        # tools cpu 0.1 .. 1.0 over 10 intervals: peak 1.0, p95 = ceil(9.5) = 10th = 1.0, p50 = 5th = 0.5
        self.stat(s["tools"]["cpu_cores"], 1.0, 1.0, 0.5)
        # tools pss 100 .. 1100: peak 1100, p95 11th = 1100, p50 6th = 600
        self.stat(s["tools"]["pss_mib"], 1100.0, 1100.0, 600.0)
        self.assertEqual(s["tools"]["procs"], {"peak": 3})
        self.assertEqual(j["concurrency"]["sessions_peak"], 1)
        self.stat(j["concurrency"]["tools_cpu_cores"], 1.0, 1.0, 0.5)
        self.stat(j["concurrency"]["tools_pss_mib"], 1100.0, 1100.0, 600.0)

    def test_gap(self):
        # Seqs 6, 7 and 8 are missing: seq 5 to seq 9 is 8 s, more than 3 * 2 s = 6 s, so a gap.
        # The counters jump over it (host busy +12, self +10 s, tools +20 s) so a gap interval that
        # leaked into a figure would show: host 2 * 16 / 16 = 2.0 cores, tools (77.2 - 52.0) / 8 = 3.15.
        j = summarize("gap")
        self.notes(j, gaps=2)  # one for the host, one for the session
        self.assertEqual(j["run"]["samples"], 8)
        # host: six intervals (1-2, 2-3, 3-4, 4-5, 9-10, 10-11), each 0.5 cores
        self.stat(j["host"]["cpu_cores"], 0.5, 0.5, 0.5)
        # mem over 8 samples at 0.5 * seq: 0.5 1.0 1.5 2.0 2.5 4.5 5.0 5.5: peak 5.5, p95 = 8th, p50 = 4th = 2.0
        self.stat(j["host"]["mem_used_gib"], 5.5, 5.5, 2.0)
        s = j["sessions"][0]
        self.assertEqual(s["samples"], 8)
        self.stat(s["self"]["cpu_cores"], 0.1, 0.1, 0.1)
        # tools: intervals i = 1..4 (0.1 .. 0.4) and i = 9, 10 (0.9, 1.0): 0.1 0.2 0.3 0.4 0.9 1.0 (N = 6)
        #   peak 1.0, p95 = ceil(5.7) = 6th = 1.0, p50 = 3rd = 0.3
        self.stat(s["tools"]["cpu_cores"], 1.0, 1.0, 0.3)
        # tools pss 100 200 300 400 500 900 1000 1100: peak 1100, p95 = 8th = 1100, p50 = 4th = 400
        self.stat(s["tools"]["pss_mib"], 1100.0, 1100.0, 400.0)
        # concurrency cpu: samples after the first are 2, 3, 4, 5, 9, 10, 11 (N = 7): the gap sample adds 0
        #   0 0.1 0.2 0.3 0.4 0.9 1.0: peak 1.0, p95 = ceil(6.65) = 7th = 1.0, p50 = ceil(3.5) = 4th = 0.3
        self.stat(j["concurrency"]["tools_cpu_cores"], 1.0, 1.0, 0.3)
        self.stat(j["concurrency"]["tools_pss_mib"], 1100.0, 1100.0, 400.0)

    def test_gap_markdown_note(self):
        notes = sections(markdown("gap"))["notes"]
        # The count is asserted exactly from --json in test_gap (gaps = 2); here the same count must be
        # on the one note line that names gaps, as a whole word, and no other note appears.
        lines = [l for l in notes.splitlines() if l.strip()]
        self.assertEqual(len(lines), 1, lines)
        self.assertRegex(lines[0], r"(?i)\b2\b.*\bgaps?\b|\bgaps?\b.*\b2\b")
        self.assertNotIn("No end line", notes)
        self.assertNotRegex(notes, r"(?i)skipped|clamp")

    def test_negative_difference(self):
        # At seq 6 host cpu_busy_s falls 104 -> 102 and the session's tools.cpu_s falls 52.0 -> 51.0.
        # Each is clamped to 0 and counted once: clamped = 2.
        j = summarize("negative")
        self.notes(j, clamped=2)
        # host: ten intervals, 0.5 cores except 5-6, which is 0.0: 0 and nine 0.5, so all three are 0.5
        self.stat(j["host"]["cpu_cores"], 0.5, 0.5, 0.5)
        # tools: 0.1 0.2 0.3 0.4 | 5-6 clamped to 0 | 6-7 (52.2 - 51.0) / 2 = 0.6, 0.7 0.8 0.9 1.0
        #   sorted 0 .1 .2 .3 .4 .6 .7 .8 .9 1.0 (N = 10): peak 1.0, p95 = 10th = 1.0, p50 = 5th = 0.4
        #   (dropping the clamped interval instead would give p50 = 0.6. A raw negative, 5-6 = (51.0 - 52.0) / 2 = -0.5,
        #   would still sort first and leave the 5th value at 0.4, so this full-record test cannot tell clamping from
        #   a raw negative: test_negative_difference_in_a_window below can.)
        s = j["sessions"][0]
        self.stat(s["tools"]["cpu_cores"], 1.0, 1.0, 0.4)
        self.stat(s["self"]["cpu_cores"], 0.1, 0.1, 0.1)
        self.stat(j["concurrency"]["tools_cpu_cores"], 1.0, 1.0, 0.4)
        # memory is unaffected
        self.stat(s["tools"]["pss_mib"], 1100.0, 1100.0, 600.0)
        self.stat(j["host"]["mem_used_gib"], 5.5, 5.5, 3.0)

    def test_negative_difference_in_a_window(self):
        # --from :08 and --to :12 keep seq 5 (:08), 6 (:10) and 7 (:12): two intervals each.
        # host: 5-6 busy 104 -> 102 is clamped to 0 (cores 0.0); 6-7 busy +1.0 over total +4.0: 2 * 1.0 / 4.0 = 0.5
        #   values 0.0 0.5 (N = 2): peak 0.5, p95 = ceil(1.9) = 2nd = 0.5, p50 = ceil(1.0) = 1st = 0.0
        #   (a raw negative, 2 * -2.0 / 4.0 = -1.0, would make the p50 -1.0)
        # tools: 5-6 52.0 -> 51.0 is clamped to 0; 6-7 (52.2 - 51.0) / 2 = 0.6: values 0.0 0.6: (0.6, 0.6, 0.0)
        #   (raw: -1.0 / 2 = -0.5, so p50 -0.5)
        # self: 20.8 -> 21.0 -> 21.2, 0.2 / 2 = 0.1 in both intervals
        j = summarize("negative", "--from", "2026-11-18T03:00:08-08:00", "--to", "2026-11-18T03:00:12-08:00")
        self.assertEqual(j["run"]["samples"], 3)
        self.notes(j, clamped=2)
        self.stat(j["host"]["cpu_cores"], 0.5, 0.5, 0.0, "host cpu")
        s = j["sessions"][0]
        self.stat(s["tools"]["cpu_cores"], 0.6, 0.6, 0.0, "tools cpu")
        self.stat(s["self"]["cpu_cores"], 0.1, 0.1, 0.1, "self cpu")
        # concurrency: the samples after the first are seq 6 (0) and seq 7 (0.6)
        self.stat(j["concurrency"]["tools_cpu_cores"], 0.6, 0.6, 0.0, "concurrency cpu")

    def test_sampler_cpu_falling_is_clamped_and_counted(self):
        # sampler_cpu_s runs 1.0, 1.1, 1.05, 1.2: the fall at sample 3 is one negative difference, clamped
        # and counted once. Nothing else is negative: host busy +0.5 over total +2.0 each, 2 cpus: 2 * 0.5 / 2.0 = 0.5.
        j = summarize("sampler-fall")
        self.notes(j, clamped=1)
        self.assertEqual(j["run"]["samples"], 4)
        self.stat(j["host"]["cpu_cores"], 0.5, 0.5, 0.5, "host cpu")
        self.assertEqual(j["sessions"], [])
        self.assertEqual(j["concurrency"]["sessions_peak"], 0)
        # The overhead is not pinned to one reading when a counter falls, but it MUST NOT be negative.
        self.assertGreaterEqual(j["run"]["sampler_overhead_cores"], 0.0)

    def test_malformed_lines(self):
        # Five junk lines are added to the clean record: not JSON, a JSON array, an unknown kind,
        # a session line without "tools", and a host line whose cpu_busy_s is a string.
        j = summarize("malformed")
        self.notes(j, skipped=5)
        self.assertEqual(without_notes(j), without_notes(self.clean))
        self.assertEqual(len(j["sessions"]), 1)  # the "ghost" session line was skipped

    def test_malformed_markdown_note(self):
        notes = sections(markdown("malformed"))["notes"]
        self.assertRegex(notes, r"Skipped 5 malformed lines?")

    def test_unknown_keys_change_nothing(self):
        # Unknown keys on every kind of line, inside self and tools too: same numbers, no note.
        j = summarize("unknown-key")
        self.notes(j)
        self.assertEqual(j, self.clean)
        self.assertEqual(sections(markdown("unknown-key"))["notes"].strip(), "None.")

    def test_missing_end_line(self):
        p = run(fx("noend"), "--json")
        self.assertEqual(p.returncode, 0, p.stderr)
        j = json.loads(p.stdout)
        self.notes(j, missing_end=True)
        self.assertEqual(without_notes(j), without_notes(self.clean))

    def test_missing_end_truncated_last_line(self):
        # The end line was cut mid-write: one malformed line, and still no end line.
        p = run(fx("noend-truncated"), "--json")
        self.assertEqual(p.returncode, 0, p.stderr)
        j = json.loads(p.stdout)
        self.notes(j, skipped=1, missing_end=True)
        self.assertEqual(without_notes(j), without_notes(self.clean))

    def test_missing_end_markdown_note(self):
        notes = sections(markdown("noend"))["notes"]
        self.assertIn("No end line: the record was summarized up to its last complete line.", notes)
        self.assertNotIn("Skipped", notes)
        notes = sections(markdown("noend-truncated"))["notes"]
        self.assertIn("No end line: the record was summarized up to its last complete line.", notes)
        self.assertRegex(notes, r"Skipped 1 malformed line")

    def test_notes_lines_start_with_a_capital(self):
        for name in ("gap", "negative", "malformed", "noend", "noend-truncated"):
            lines = [l for l in sections(markdown(name))["notes"].splitlines() if l.strip()]
            self.assertTrue(lines, name)
            for l in lines:
                text = l.strip().lstrip("-*").strip()
                self.assertTrue(text[0].isupper(), "%s: %r" % (name, l))


class Window(Base):
    """--from and --to on the clean fixture. Seq s is at 03:00:00 + 2 * (s - 1) s, in -08:00."""

    def test_both_ends(self):
        # --from 03:00:03 and --to 03:00:15 keep seq 3 (:04) to seq 8 (:14); seq 2 (:02) and seq 9 (:16) are out.
        j = summarize("clean", "--from", "2026-11-18T03:00:03.000-08:00", "--to", "2026-11-18T03:00:15.000-08:00")
        self.assertEqual(j["run"]["samples"], 6)
        self.assertEqual(j["run"]["start"], "2026-11-18T03:00:04.000-08:00")
        self.assertEqual(j["run"]["end"], "2026-11-18T03:00:14.000-08:00")
        # mem 1.5 .. 4.0 GiB (6): peak 4.0, p95 = ceil(5.7) = 6th = 4.0, p50 = 3rd = 2.5
        self.stat(j["host"]["mem_used_gib"], 4.0, 4.0, 2.5)
        self.stat(j["host"]["cpu_cores"], 0.5, 0.5, 0.5)
        s = j["sessions"][0]
        self.assertEqual(s["samples"], 6)
        self.assertEqual(s["first_seen"], "2026-11-18T03:00:04.000-08:00")
        self.assertEqual(s["last_seen"], "2026-11-18T03:00:14.000-08:00")
        # tools cpu: intervals i = 3..7 = 0.3 .. 0.7 (N = 5): peak 0.7, p95 = 5th = 0.7, p50 = 3rd = 0.5
        self.stat(s["tools"]["cpu_cores"], 0.7, 0.7, 0.5)
        # tools pss 300 .. 800 (6): peak 800, p95 = 6th = 800, p50 = 3rd = 500
        self.stat(s["tools"]["pss_mib"], 800.0, 800.0, 500.0)
        self.stat(j["concurrency"]["tools_cpu_cores"], 0.7, 0.7, 0.5)
        self.stat(j["concurrency"]["tools_pss_mib"], 800.0, 800.0, 500.0)
        self.assertEqual(j["notes"]["skipped_lines"], 0)
        self.assertEqual(j["notes"]["gaps"], 0)
        self.assertEqual(j["notes"]["clamped"], 0)

    def test_ends_are_inclusive(self):
        # --from at seq 3's exact time and --to at seq 8's exact time keep the same six samples.
        j = summarize("clean", "--from", "2026-11-18T03:00:04.000-08:00", "--to", "2026-11-18T03:00:14.000-08:00")
        self.assertEqual(j["run"]["samples"], 6)
        self.stat(j["sessions"][0]["tools"]["cpu_cores"], 0.7, 0.7, 0.5)

    def test_from_only_in_another_offset(self):
        # 03:00:16-08:00 is 11:00:16+00:00. Keeps seq 9, 10, 11 (3 samples, 2 intervals).
        j = summarize("clean", "--from", "2026-11-18T11:00:16+00:00")
        self.assertEqual(j["run"]["samples"], 3)
        # mem 4.5 5.0 5.5: peak 5.5, p95 = ceil(2.85) = 3rd = 5.5, p50 = ceil(1.5) = 2nd = 5.0
        self.stat(j["host"]["mem_used_gib"], 5.5, 5.5, 5.0)
        # tools cpu i = 9, 10 = 0.9, 1.0: peak 1.0, p95 = 2nd = 1.0, p50 = 1st = 0.9
        self.stat(j["sessions"][0]["tools"]["cpu_cores"], 1.0, 1.0, 0.9)

    def test_to_only(self):
        # --to 03:00:06 keeps seq 1 (:00) to seq 4 (:06).
        j = summarize("clean", "--to", "2026-11-18T03:00:06.000-08:00")
        self.assertEqual(j["run"]["samples"], 4)
        # mem 0.5 1.0 1.5 2.0: peak 2.0, p95 = 4th = 2.0, p50 = 2nd = 1.0
        self.stat(j["host"]["mem_used_gib"], 2.0, 2.0, 1.0)
        # tools cpu i = 1, 2, 3 = 0.1 0.2 0.3: peak 0.3, p95 = ceil(2.85) = 3rd = 0.3, p50 = ceil(1.5) = 2nd = 0.2
        self.stat(j["sessions"][0]["tools"]["cpu_cores"], 0.3, 0.3, 0.2)
        self.assertEqual(j["sessions"][0]["samples"], 4)

    def test_window_in_markdown_span(self):
        md = markdown("clean", "--from", "2026-11-18T03:00:03.000-08:00", "--to", "2026-11-18T03:00:15.000-08:00")
        self.assertIn("3:00am to 3:00am (-08:00)", md)


class Spans(Base):
    def test_am_to_am(self):
        # first host line 01:45:00, last 03:10:00 (5100 s apart, under 3 * 3000 s, so not a gap)
        md = markdown("span-am")
        self.assertIn("1:45am to 3:10am (-08:00)", md)

    def test_noon_to_midnight_in_a_positive_offset(self):
        # 12:07 (noon hour) to the next day 12:05 after midnight, in +05:30
        md = markdown("span-pm")
        self.assertIn("12:07pm to 12:05am (+05:30)", md)

    def test_utc_offset_and_pm(self):
        md = markdown("span-utc")
        self.assertIn("5:30pm to 6:45pm (+00:00)", md)

    def test_json_keeps_the_record_times(self):
        j = summarize("span-pm")
        self.assertEqual(j["run"]["utc_offset"], "+05:30")
        self.assertEqual(j["run"]["start"], "2026-11-18T12:07:00.000+05:30")
        self.assertEqual(j["run"]["end"], "2026-11-19T00:05:00.000+05:30")
        self.assertEqual(j["sessions"], [])

    def test_no_leading_zero(self):
        md = markdown("span-am")
        self.assertNotRegex(md, r"\b0[1-9]:\d\d(am|pm)")


class Rounding(Base):
    """3 CPUs, interval 3 s, two samples. Session R1 in both, R2 only in the second."""

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("rounding")
        cls.md = markdown("rounding")

    def test_json_is_unrounded(self):
        # host: 3 cpus * d_busy 1.0 / d_total 9.0 (3 cpus * 3 s) = 0.333333
        self.stat(self.j["host"]["cpu_cores"], 1 / 3, 1 / 3, 1 / 3)
        self.assertNotEqual(self.j["host"]["cpu_cores"]["peak"], 0.33)
        # mem 1.5 GiB then 2.5 GiB (N = 2): peak 2.5, p95 = ceil(1.9) = 2nd = 2.5, p50 = ceil(1) = 1st = 1.5
        self.stat(self.j["host"]["mem_used_gib"], 2.5, 2.5, 1.5)
        self.assertAlmostEqual(self.j["run"]["sampler_overhead_cores"], 0.01, places=6)  # 0.03 s over 3 s

    def test_sessions(self):
        r1, r2 = self.j["sessions"]
        self.assertEqual(r1["label"], "rounder 7000-111")
        self.assertEqual(r2["label"], "single 7001-222")
        # R1 self: (52 - 50) / 3 = 0.666667; tools (71 - 70) / 3 = 0.333333
        self.stat(r1["self"]["cpu_cores"], 2 / 3, 2 / 3, 2 / 3)
        self.stat(r1["tools"]["cpu_cores"], 1 / 3, 1 / 3, 1 / 3)
        # self pss 3 then 4 MiB: peak 4, p95 = 2nd = 4, p50 = 1st = 3; tools pss 1.5 then 2.5 MiB
        self.stat(r1["self"]["pss_mib"], 4.0, 4.0, 3.0)
        self.stat(r1["tools"]["pss_mib"], 2.5, 2.5, 1.5)
        self.assertEqual(r1["tools"]["procs"], {"peak": 4})
        # R2 has one line, so no interval: its CPU figures are null, its PSS is the one sample
        self.stat(r2["self"]["cpu_cores"], None, None, None)
        self.stat(r2["tools"]["cpu_cores"], None, None, None)
        self.stat(r2["self"]["pss_mib"], 1.0, 1.0, 1.0)
        self.stat(r2["tools"]["pss_mib"], 0.5, 0.5, 0.5)
        self.assertEqual(r2["tools"]["procs"], {"peak": 1})
        self.assertEqual(r2["samples"], 1)

    def test_concurrency(self):
        c = self.j["concurrency"]
        self.assertEqual(c["sessions_peak"], 2)
        # cpu: one value (the second sample): R1 1/3 + R2 (no interval) 0
        self.stat(c["tools_cpu_cores"], 1 / 3, 1 / 3, 1 / 3)
        # pss: 1.5 then 2.5 + 0.5 = 3.0: peak 3.0, p95 = 2nd = 3.0, p50 = 1st = 1.5
        self.stat(c["tools_pss_mib"], 3.0, 3.0, 1.5)

    def test_markdown_rounds_to_two_places(self):
        sec = sections(self.md)
        self.assertIn("0.33", sec["host"])
        row = session_row(self.md, "rounder 7000-111")
        cells = numeric_cells(row)
        self.assertTrue(has_all(cells, [0.67, 0.33, 4.0, 2.5, 4]), cells)
        # sampler overhead: 0.03 s over 3 s = 0.01 cores = 1.00% of one core
        self.assertIn("Sampler overhead: 0.0100 cores (1.00% of one core)", self.md)

    def test_null_prints_as_dash(self):
        row = session_row(self.md, "single 7001-222")
        self.assertIn("-", [c.strip() for c in row])


class JsonMatchesMarkdown(Base):
    def check(self, name):
        j = summarize(name)
        md = markdown(name)
        sec = sections(md)
        h = j["host"]
        want = [h["cpu_cores"][k] for k in ("peak", "p95", "p50")] + [h["mem_used_gib"][k] for k in ("peak", "p95", "p50")]
        self.assertTrue(has_all(all_numeric(sec["host"]), want), (name, "host", want, all_numeric(sec["host"])))
        for s in j["sessions"]:
            self.assertIn(s["label"], md)
            row = session_row(md, s["label"])
            want = []
            for part in ("self", "tools"):
                for fig in ("cpu_cores", "pss_mib"):
                    want += [s[part][fig][k] for k in ("peak", "p95", "p50") if s[part][fig][k] is not None]
            want.append(s["tools"]["procs"]["peak"])
            self.assertTrue(has_all(numeric_cells(row), want), (name, s["label"], want, numeric_cells(row)))
        c = j["concurrency"]
        want = [c["sessions_peak"]]
        for fig in ("tools_cpu_cores", "tools_pss_mib"):
            want += [c[fig][k] for k in ("peak", "p95", "p50") if c[fig][k] is not None]
        self.assertTrue(has_all(all_numeric(sec["concurrency"]), want), (name, "concurrency", want, all_numeric(sec["concurrency"])))

    def test_two_sessions(self):
        self.check("two-sessions")

    def test_lifecycle(self):
        self.check("lifecycle")

    def test_rounding(self):
        self.check("rounding")

    def test_json_parses_and_is_one_object(self):
        p = run(fx("clean"), "--json")
        self.assertEqual(p.returncode, 0)
        self.assertIsInstance(json.loads(p.stdout), dict)

    def test_markdown_is_not_json(self):
        out = run(fx("clean")).stdout
        self.assertIn("## Host", out)
        with self.assertRaises(ValueError):
            json.loads(out)


class Errors(Base):
    def assertUsageError(self, p, msg=""):
        self.assertEqual(p.returncode, 2, "%s stdout %r stderr %r" % (msg, p.stdout, p.stderr))
        self.assertTrue(p.stderr.strip(), msg)

    def test_missing_file(self):
        self.assertUsageError(run(os.path.join(FIX, "no-such-record.jsonl")), "missing file")
        self.assertUsageError(run(os.path.join(FIX, "no-such-record.jsonl"), "--json"), "missing file, json")

    def test_no_arguments(self):
        self.assertUsageError(run(), "no arguments")

    def test_unknown_option(self):
        self.assertUsageError(run(fx("clean"), "--bogus"), "unknown option")

    def test_two_files(self):
        self.assertUsageError(run(fx("clean"), fx("lifecycle")), "two files")

    def test_option_without_value(self):
        self.assertUsageError(run(fx("clean"), "--from"), "--from without a value")
        self.assertUsageError(run(fx("clean"), "--to"), "--to without a value")

    def test_from_without_offset(self):
        self.assertUsageError(run(fx("clean"), "--from", "2026-11-18T03:00:04"), "--from without an offset")
        self.assertUsageError(run(fx("clean"), "--to", "2026-11-18T03:00:04"), "--to without an offset")
        self.assertUsageError(run(fx("clean"), "--from", "2026-11-18T03:00:04.000", "--json"), "--from without an offset, json")

    def test_unparseable_time(self):
        self.assertUsageError(run(fx("clean"), "--from", "yesterday-ish"), "unparseable --from")

    def test_no_start_line(self):
        self.assertUsageError(run(fx("nostart")), "record with no start line")

    def test_empty_record(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "empty.jsonl")
            open(path, "w").close()
            self.assertUsageError(run(path), "empty file")

    def test_errors_print_nothing_on_stdout(self):
        self.assertEqual(run(os.path.join(FIX, "no-such-record.jsonl")).stdout, "")

    def test_record_without_end_line_exits_zero(self):
        self.assertEqual(run(fx("noend")).returncode, 0)
        self.assertEqual(run(fx("noend"), "--json").returncode, 0)
        self.assertEqual(run(fx("noend-truncated")).returncode, 0)


class Jitter(Base):
    """6 samples, 2 CPUs, interval_s 2.0, with d(t) of 2.5 s, exactly 6.000 s, 6.001 s, 2.0 s, 2.0 s.

    The gap limit is 3 * 2.0 = 6.0 s: an interval of exactly 6.000 s is kept, and 6.001 s is a gap.
    d(cpu_total_s) is never cpus * d(t), so a reader that assumed it would get the wrong cores.
    Session X (8001-500000) is in seq 1 to 4, session Y (8002-600000) in seq 5 and 6: two distinct
    sessions that never overlap.
    """

    @classmethod
    def setUpClass(cls):
        cls.j = summarize("jitter")
        cls.md = markdown("jitter")

    def test_run(self):
        r = self.j["run"]
        self.assertEqual(r["samples"], 6)
        self.assertEqual(r["start"], "2026-11-18T04:00:00.000-08:00")
        self.assertEqual(r["end"], "2026-11-18T04:00:18.501-08:00")  # 2.5 + 6.0 + 6.001 + 2.0 + 2.0 = 18.501 s
        # sampler_cpu_s 0.0 .. 0.25 over 18.501 s
        self.assertAlmostEqual(r["sampler_overhead_cores"], 0.25 / 18.501, places=6)
        self.assertAlmostEqual(r["interval_s"], 2.0)

    def test_notes(self):
        # one gap for the host (seq 3 to 4, 6.001 s) and one for session X (the same two samples)
        self.notes(self.j, gaps=2)

    def test_host(self):
        # intervals: 1-2 busy +1.0 total +4.0: 2 * 1.0 / 4.0 = 0.5 (cpus * d(t) would be 5.0)
        #   2-3 (6.000 s, kept) busy +3.0 total +10.0: 2 * 3.0 / 10.0 = 0.6 (cpus * d(t) would be 12.0)
        #   3-4 (6.001 s) is a gap: it would be 2 * 50 / 12 = 8.33 if it leaked
        #   4-5 busy +1.5 total +3.0: 2 * 1.5 / 3.0 = 1.0
        #   5-6 busy +0.5 total +5.0: 2 * 0.5 / 5.0 = 0.2
        #   values 0.2 0.5 0.6 1.0 (N = 4): peak 1.0, p95 = ceil(3.8) = 4th = 1.0, p50 = ceil(2) = 2nd = 0.5
        self.stat(self.j["host"]["cpu_cores"], 1.0, 1.0, 0.5, "host cpu")
        # mem GiB 1 3 2 5 4 6 (N = 6): peak 6, p95 = ceil(5.7) = 6th = 6, p50 = ceil(3) = 3rd = 3
        self.stat(self.j["host"]["mem_used_gib"], 6.0, 6.0, 3.0, "host mem")

    def test_two_sessions_listed(self):
        sx, sy = self.j["sessions"]
        self.assertEqual((sx["session"], sy["session"]), ("8001-500000", "8002-600000"))
        self.assertEqual((sx["label"], sy["label"]), ("jitx 8001-500", "jity 8002-600"))
        self.assertEqual((sx["samples"], sy["samples"]), (4, 2))
        self.assertEqual(sx["first_seen"], "2026-11-18T04:00:00.000-08:00")
        self.assertEqual(sx["last_seen"], "2026-11-18T04:00:14.501-08:00")
        self.assertEqual(sy["first_seen"], "2026-11-18T04:00:16.501-08:00")
        self.assertEqual(sy["last_seen"], "2026-11-18T04:00:18.501-08:00")

    def test_session_x(self):
        s = self.j["sessions"][0]
        # self cpu_s 10.0 10.5 12.0 20.0: 1-2 0.5 / 2.5 = 0.2; 2-3 (6.000 s) 1.5 / 6.0 = 0.25;
        #   3-4 is a gap (8.0 / 6.001 = 1.33 if it leaked): values 0.2 0.25: peak 0.25, p95 = ceil(1.9) = 2nd = 0.25, p50 = 1st = 0.2
        self.stat(s["self"]["cpu_cores"], 0.25, 0.25, 0.2, "X self cpu")
        # tools cpu_s 20.0 21.0 24.0 40.0: 1-2 1.0 / 2.5 = 0.4; 2-3 3.0 / 6.0 = 0.5; 3-4 a gap (16 / 6.001 = 2.67)
        self.stat(s["tools"]["cpu_cores"], 0.5, 0.5, 0.4, "X tools cpu")
        # self pss 10 20 30 40 MiB (N = 4): peak 40, p95 = ceil(3.8) = 4th = 40, p50 = 2nd = 20
        self.stat(s["self"]["pss_mib"], 40.0, 40.0, 20.0, "X self pss")
        # tools pss 100 50 200 150 MiB: sorted 50 100 150 200: peak 200, p95 = 4th = 200, p50 = 2nd = 100
        self.stat(s["tools"]["pss_mib"], 200.0, 200.0, 100.0, "X tools pss")
        self.assertEqual(s["tools"]["procs"], {"peak": 3})  # procs 1 3 2 2

    def test_session_y(self):
        s = self.j["sessions"][1]
        # self cpu_s 5.0 -> 5.6 over 2.0 s: 0.6 / 2.0 = 0.3 (one interval); tools 7.0 -> 8.0: 1.0 / 2.0 = 0.5
        self.stat(s["self"]["cpu_cores"], 0.3, 0.3, 0.3, "Y self cpu")
        self.stat(s["tools"]["cpu_cores"], 0.5, 0.5, 0.5, "Y tools cpu")
        # self pss 5 then 7 MiB: peak 7, p95 = ceil(1.9) = 2nd = 7, p50 = 1st = 5; tools pss 20 then 30
        self.stat(s["self"]["pss_mib"], 7.0, 7.0, 5.0, "Y self pss")
        self.stat(s["tools"]["pss_mib"], 30.0, 30.0, 20.0, "Y tools pss")
        self.assertEqual(s["tools"]["procs"], {"peak": 4})  # procs 4 then 1

    def test_concurrency(self):
        c = self.j["concurrency"]
        # Two distinct sessions exist, but no sample holds more than one of them.
        self.assertEqual(c["sessions_peak"], 1)
        # tools cpu per sample after the first (5 values): seq 2 X 0.4; seq 3 X 0.5; seq 4 X is a gap, adds 0;
        #   seq 5 Y has its first line only, no interval, adds 0; seq 6 Y 0.5
        #   sorted 0 0 0.4 0.5 0.5: peak 0.5, p95 = ceil(4.75) = 5th = 0.5, p50 = ceil(2.5) = 3rd = 0.4
        self.stat(c["tools_cpu_cores"], 0.5, 0.5, 0.4, "concurrency cpu")
        # tools pss per sample (6): 100 50 200 150 (X) then 20 30 (Y): sorted 20 30 50 100 150 200
        #   peak 200, p95 = ceil(5.7) = 6th = 200, p50 = ceil(3) = 3rd = 50
        self.stat(c["tools_pss_mib"], 200.0, 200.0, 50.0, "concurrency pss")

    def test_markdown_header(self):
        # sampler overhead 0.25 / 18.501 = 0.013513 cores: 4 places 0.0135, as a percentage 1.35% (2 places)
        self.assertIn("Sampler overhead: 0.0135 cores (1.35% of one core)", self.md)
        self.assertRegex(self.md, r"Interval: 2 seconds")
        self.assertIn("4:00am to 4:00am (-08:00)", self.md)


class MarkdownColumns(Base):
    """Figures read from the Markdown by their column header, so a swap of p95 and p50 would fail.

    The two-sessions fixture gives every figure distinct peak, p95 and p50 values.
    """

    @classmethod
    def setUpClass(cls):
        cls.md = markdown("two-sessions")
        cls.sec = sections(cls.md)

    def check(self, row, wants):
        for pattern, want in wants:
            self.assertAlmostEqual(figure(row, pattern), want, places=6, msg=pattern)

    def test_host_columns(self):
        rows = table_dicts(self.sec["host"])
        self.assertEqual(len(rows), 1)
        # cpu: peak 2.0, p95 1.9, p50 1.0; memory GiB: peak 10.5, p95 10.0, p50 5.5
        self.check(rows[0], [
            (r"cpu.*peak", 2.0), (r"cpu.*p95", 1.9), (r"cpu.*p50", 1.0),
            (r"(mem|gib).*peak", 10.5), (r"(mem|gib).*p95", 10.0), (r"(mem|gib).*p50", 5.5)])

    def test_session_columns(self):
        rows = [r for r in table_dicts(self.sec["sessions"]) if "projA 12345-67" in " ".join(r.values())]
        self.assertEqual(len(rows), 1)
        # A self cpu 1.0 / 0.95 / 0.5, self pss 210 / 200 / 110; tools cpu 5.0 / 4.75 / 2.5, tools pss 2100 / 2000 / 1100
        self.check(rows[0], [
            (r"(self|own).*cpu.*peak", 1.0), (r"(self|own).*cpu.*p95", 0.95), (r"(self|own).*cpu.*p50", 0.5),
            (r"(self|own).*(pss|mib).*peak", 210.0), (r"(self|own).*(pss|mib).*p95", 200.0),
            (r"(self|own).*(pss|mib).*p50", 110.0),
            (r"tools.*cpu.*peak", 5.0), (r"tools.*cpu.*p95", 4.75), (r"tools.*cpu.*p50", 2.5),
            (r"tools.*(pss|mib).*peak", 2100.0), (r"tools.*(pss|mib).*p95", 2000.0),
            (r"tools.*(pss|mib).*p50", 1100.0),
            (r"procs", 9.0)])

    def test_concurrency_columns(self):
        rows = table_dicts(self.sec["concurrency"])
        self.assertEqual(len(rows), 1)
        # sessions peak 2; tools cpu peak 7.5 and p50 3.75 (p95 7.125 is left out: it sits on a rounding tie);
        # tools pss 3150 / 3000 / 1650
        self.check(rows[0], [
            (r"sessions", 2.0),
            (r"cpu.*peak", 7.5), (r"cpu.*p50", 3.75),
            (r"(pss|mib).*peak", 3150.0), (r"(pss|mib).*p95", 3000.0), (r"(pss|mib).*p50", 1650.0)])


class NewBehaviour(Base):
    """The review's fixes: help text, a deep line, a null cwd, a BOM, the overhead line, the interval's
    singular, and a session line a few milliseconds after its host line."""

    def test_help_lines_start_with_a_capital(self):
        # COLUMNS is wide so argparse does not wrap, which would put description fragments on lines of their own.
        p = run_env({"COLUMNS": "200"}, "--help")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("Usage:", p.stdout)
        self.assertIn("Show this help and exit", p.stdout)
        for line in p.stdout.splitlines():
            if not line.strip():
                continue
            if not line[0].isspace():
                # a heading or a sentence: it starts with a capital
                self.assertTrue(line[0].isupper(), "Help line %r" % line)
            else:
                # a table row: the label (a flag or a name) may start with anything, each description after it must not
                parts = re.split(r"\s{2,}", line.strip())
                for part in parts[1:]:
                    self.assertTrue(part[0].isupper() or part[0].isdecimal(), "Help line %r, part %r" % (line, part))

    def test_deeply_nested_line_is_skipped(self):
        with open(fx("clean"), "rb") as f:
            clean = f.read().split(b"\n")
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "deep.jsonl")
            with open(path, "wb") as f:
                f.write(b"\n".join(clean[:3] + [b"[" * 200000] + clean[3:]))
            p = run(path, "--json")
        self.assertEqual(p.returncode, 0, p.stderr[-500:])
        self.assertNotIn("Traceback", p.stderr)
        j = json.loads(p.stdout)
        self.notes(j, skipped=1)
        self.assertEqual(without_notes(j), without_notes(summarize("clean")))

    def test_bom_record_reads(self):
        with open(fx("clean"), "rb") as f:
            data = f.read()
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "bom.jsonl")
            with open(path, "wb") as f:
                f.write(b"\xef\xbb\xbf" + data)
            p = run(path, "--json")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(json.loads(p.stdout), summarize("clean"))  # the start line is not hidden: nothing skipped

    def test_null_and_empty_cwd(self):
        # 3 samples 2 s apart, three sessions in each. tools cpu_s: A 10 11 13, B 20 20.4 20.8, C 30 30.2 30.6
        j = summarize("labels")
        self.notes(j)  # a null cwd is not malformed: nothing skipped
        a, b, c = j["sessions"]
        self.assertEqual(a["label"], "? 9001-700")  # null cwd: the folder is ?
        self.assertEqual(b["label"], "? 9002-800")  # empty cwd: also ?
        self.assertEqual(c["label"], "a|b 9003-900")
        self.assertEqual((a["samples"], b["samples"], c["samples"]), (3, 3, 3))
        # A: 1.0 / 2 = 0.5 then 2.0 / 2 = 1.0: (1.0, 1.0, 0.5). C: 0.2 / 2 = 0.1 then 0.4 / 2 = 0.2: (0.2, 0.2, 0.1)
        self.stat(a["tools"]["cpu_cores"], 1.0, 1.0, 0.5, "A tools cpu")
        self.stat(c["tools"]["cpu_cores"], 0.2, 0.2, 0.1, "C tools cpu")
        self.assertEqual(j["concurrency"]["sessions_peak"], 3)
        # concurrency cpu, seq 2: 0.5 + 0.2 + 0.1 = 0.8, seq 3: 1.0 + 0.2 + 0.2 = 1.4: (1.4, 1.4, 0.8)
        self.stat(j["concurrency"]["tools_cpu_cores"], 1.4, 1.4, 0.8)

    def test_pipe_in_label_is_escaped_in_markdown(self):
        md = markdown("labels")
        lines = [l for l in sections(md)["sessions"].splitlines() if l.strip().startswith("|")]
        header = lines[0]
        pipe = [l for l in lines if "9003-900" in l]
        self.assertEqual(len(pipe), 1)
        self.assertIn("a\\|b 9003-900", pipe[0])
        for l in lines:
            self.assertEqual(len(split_unescaped(l)), len(split_unescaped(header)), l)
        self.assertEqual(len([l for l in lines if "? 9001-700" in l]), 1)
        self.assertEqual(len([l for l in lines if "? 9002-800" in l]), 1)

    def test_interval_of_one_second_is_singular(self):
        md = markdown("sampler-fall")  # interval_s 1.0
        self.assertRegex(md, r"Interval: 1 second(?!s)")
        self.assertNotIn("1 seconds", md)

    def test_overhead_line_has_four_places_and_a_percentage(self):
        # two-sessions 0.0100 / 1.00%, jitter 0.0135 / 1.35%: the places differ, so neither is a fixed string
        self.assertIn("Sampler overhead: 0.0100 cores (1.00% of one core)", markdown("two-sessions"))
        self.assertIn("Sampler overhead: 0.0135 cores (1.35% of one core)", markdown("jitter"))

    def test_session_line_a_few_ms_after_its_host_line_is_kept_with_its_sample(self):
        # skew: host lines at :00 :02 :04, each session line 3 ms later. tools cpu_s 10 11 13, 2 cpus.
        # --to at seq 2's host time (:02.000) keeps seq 1 and 2, session lines included, though a session line's
        # own t is :02.003: samples 2, session samples 2, tools cpu (11 - 10) / 2.0 = 0.5, one interval.
        j = summarize("skew", "--to", "2026-11-18T07:00:02.000-08:00")
        self.assertEqual(j["run"]["samples"], 2)
        self.assertEqual(j["run"]["end"], "2026-11-18T07:00:02.000-08:00")
        s = j["sessions"][0]
        self.assertEqual(s["samples"], 2)
        self.stat(s["tools"]["cpu_cores"], 0.5, 0.5, 0.5, "tools cpu")
        self.stat(j["concurrency"]["tools_cpu_cores"], 0.5, 0.5, 0.5, "concurrency cpu")
        self.assertEqual(j["concurrency"]["sessions_peak"], 1)

    def test_session_line_inside_the_window_is_dropped_with_its_sample(self):
        # --from :02.001 drops seq 2's host line (:02.000), so its session line (:02.003, inside the window
        # by its own t) goes with it. Only seq 3 is kept: 1 sample, 1 session line, no interval.
        j = summarize("skew", "--from", "2026-11-18T07:00:02.001-08:00")
        self.assertEqual(j["run"]["samples"], 1)
        s = j["sessions"][0]
        self.assertEqual(s["samples"], 1)
        self.stat(s["tools"]["cpu_cores"], None, None, None)


# ---- helpers for the Markdown, read loosely: sections by "## " heading, tables by "|" rows

def sections(md):
    out, name, buf = {}, None, []
    for line in md.splitlines():
        m = re.match(r"^##\s+(.*?)\s*$", line)
        if m:
            if name is not None:
                out[name] = "\n".join(buf)
            name, buf = m.group(1).lower(), []
        elif name is not None:
            buf.append(line)
    if name is not None:
        out[name] = "\n".join(buf)
    return out


def table_rows(text):
    rows = []
    for line in text.splitlines():
        s = line.strip()
        if not s.startswith("|"):
            continue
        cells = [c.strip() for c in s.strip("|").split("|")]
        if all(re.fullmatch(r":?-{2,}:?|", c) for c in cells):
            continue
        rows.append(cells)
    return rows


def session_row(md, label):
    for cells in table_rows(sections(md)["sessions"]):
        if label in " ".join(cells):
            return cells
    raise AssertionError("No Sessions row for %r in:\n%s" % (label, sections(md)["sessions"]))


NUM = re.compile(r"-?\d+(?:\.\d+)?")


def numeric_cells(cells):
    out = []
    for c in cells:
        t = c.strip().replace(",", "")
        if NUM.fullmatch(t):
            out.append(float(t))
    return out


def all_numeric(text):
    out = []
    for cells in table_rows(text):
        out += numeric_cells(cells)
    return out


def has_all(cells, want):
    """Every wanted figure, rounded to 2 places, is among the cells (each cell used once)."""
    pool = list(cells)
    for w in want:
        target = round(w, 2)
        for i, c in enumerate(pool):
            if abs(c - target) < 1e-9:
                del pool[i]
                break
        else:
            return False
    return True


def split_unescaped(line):
    """The cells of a table row, split on pipes that are not escaped as backslash-pipe."""
    return [c.strip() for c in re.split(r"(?<!\\)\|", line.strip())[1:-1]]


def table_dicts(text):
    """The first table of a section as a list of {column header: cell}, one per data row."""
    rows = [split_unescaped(l) for l in text.splitlines() if l.strip().startswith("|")]
    rows = [r for r in rows if not all(re.fullmatch(r":?-{2,}:?", c) for c in r)]
    return [dict(zip(rows[0], r)) for r in rows[1:]]


def figure(row, pattern):
    """The number in the one cell whose column header matches the pattern."""
    keys = [k for k in row if re.search(pattern, k, re.I)]
    if len(keys) != 1:
        raise AssertionError("Pattern %r matches columns %r among %r" % (pattern, keys, list(row)))
    m = NUM.search(row[keys[0]].replace(",", ""))
    if not m:
        raise AssertionError("No number in %r (column %r)" % (row[keys[0]], keys[0]))
    return float(m.group(0))


def without_notes(j):
    return {k: v for k, v in j.items() if k != "notes"}


if __name__ == "__main__":
    unittest.main()

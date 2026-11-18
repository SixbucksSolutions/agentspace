"""Tests for box/system/agentspace-sample (E0.03-T01-S01, card #0069, D33).

RFC 2119 keywords; the capitals are load-bearing.

Run on the box (Linux, /proc): python3 -m unittest -v test_agentspace_sample

The script is driven as a subprocess and MUST NOT be imported. Its path comes from
the AGENTSPACE_SAMPLE environment variable, defaulting to ../system/agentspace-sample
relative to this file. Every test sets HOME to its own temp directory, so the
sampler's state (~/agentspace-samples/) never touches another sampler on the box.
"""

import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from datetime import datetime

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.environ.get(
    "AGENTSPACE_SAMPLE",
    os.path.join(HERE, os.pardir, "system", "agentspace-sample"),
)

T_RE = re.compile(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}(Z|[+-]\d\d:\d\d)$")
CLOCK_RE = re.compile(r"\b\d{1,2}:\d\d(am|pm)\b")

# The stub tree's root. It spawns a child per command file and reaps finished ones,
# so their CPU lands in its cutime/cstime.
ROOT_CODE = r'''
import json, os, subprocess, sys, time
d = sys.argv[2]
seen = set()
kids = []
while True:
    for f in sorted(os.listdir(d)):
        if f.endswith(".json") and f not in seen:
            seen.add(f)
            with open(os.path.join(d, f)) as fh:
                spec = json.load(fh)
            kids.append(subprocess.Popen(
                [sys.executable, "-c", spec["code"]] + spec.get("args", [])))
    kids = [k for k in kids if k.poll() is None]
    time.sleep(0.1)
'''

BURN_CODE = r'''
import sys, time
secs = float(sys.argv[2])
while time.process_time() < secs:
    pass
open(sys.argv[1], "w").write("done")
'''

MEM_CODE = r'''
import sys, time
b = bytearray(512 * 1024 * 1024)
for i in range(0, len(b), 4096):
    b[i] = 1
open(sys.argv[1], "w").write("ready")
time.sleep(600)
'''

NEST_CODE = r'''
import subprocess, sys, time
subprocess.Popen([sys.executable, "-c", "import time; time.sleep(600)"])
open(sys.argv[1], "w").write("ready")
time.sleep(600)
'''


def wait_for(fn, timeout, step=0.2):
    """Return fn()'s first truthy value, or None after timeout seconds."""
    end = time.monotonic() + timeout
    while True:
        v = fn()
        if v:
            return v
        if time.monotonic() >= end:
            return None
        time.sleep(step)


def parse_t(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


class Stub:
    """A python3 process tree whose root's command line carries a unique token."""

    def __init__(self, tmp):
        self.token = "stubroot-%s" % uuid.uuid4().hex[:12]
        self.match = re.escape(self.token)
        self.cmd_dir = os.path.join(tmp, "cmds-" + self.token)
        self.cwd = os.path.join(tmp, "cwd-" + self.token)
        os.makedirs(self.cmd_dir)
        os.makedirs(self.cwd)
        self.n = 0
        self.root = subprocess.Popen(
            [sys.executable, "-c", ROOT_CODE, self.token, self.cmd_dir],
            cwd=self.cwd, start_new_session=True,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL)

    def _spawn(self, code, args):
        self.n += 1
        marker = os.path.join(self.cmd_dir, "marker-%d" % self.n)
        spec = {"code": code, "args": [marker] + [str(a) for a in args]}
        tmpname = os.path.join(self.cmd_dir, "c%03d.tmp" % self.n)
        with open(tmpname, "w") as f:
            json.dump(spec, f)
        os.rename(tmpname, os.path.join(self.cmd_dir, "c%03d.json" % self.n))
        return marker

    def spawn(self, code, *args):
        """Ask the root to start a child running code. Returns the marker path the
        child writes when it is done or ready."""
        return self._spawn(code, args)

    def spawn_matching(self, code):
        """Like spawn, but the child's command line also carries the token."""
        return self._spawn(code, [self.token])

    def kill(self):
        try:
            os.killpg(self.root.pid, signal.SIGKILL)
        except OSError:
            pass
        try:
            self.root.wait(timeout=5)
        except Exception:
            pass


class SampleTestBase(unittest.TestCase):
    def setUp(self):
        self.assertTrue(os.path.exists(SCRIPT), "Script not found: %s" % SCRIPT)
        self.tmp = tempfile.mkdtemp(prefix="asample-test-")
        self.home = os.path.join(self.tmp, "home")
        os.makedirs(self.home)
        self.env = dict(os.environ, HOME=self.home)
        self.runs = []
        self.pids = []
        self.stubs = []

    def tearDown(self):
        for run in self.runs:
            try:
                self.sample("stop", run, timeout=40)
            except Exception:
                pass
        for pid in self.pids:
            try:
                os.kill(pid, signal.SIGKILL)
            except OSError:
                pass
        for stub in self.stubs:
            stub.kill()
        shutil.rmtree(self.tmp, ignore_errors=True)

    # --- driving the script -------------------------------------------------

    def sample(self, *args, timeout=60):
        """Run the script; returns (returncode, stdout, stderr). Output goes via
        files so a sampler that kept a pipe open could not hang the test."""
        out = os.path.join(self.tmp, "o-%s" % uuid.uuid4().hex)
        err = os.path.join(self.tmp, "e-%s" % uuid.uuid4().hex)
        with open(out, "w") as fo, open(err, "w") as fe:
            p = subprocess.run(
                [sys.executable, SCRIPT] + [str(a) for a in args],
                stdin=subprocess.DEVNULL, stdout=fo, stderr=fe,
                env=self.env, timeout=timeout)
        with open(out) as f:
            so = f.read()
        with open(err) as f:
            se = f.read()
        return p.returncode, so, se

    def start(self, run, *opts):
        self.runs.append(run)
        rc, so, se = self.sample("start", run, *opts)
        self.assertEqual(rc, 0, "start %s: rc=%s out=%r err=%r" % (run, rc, so, se))
        pid = self.pid_from(so, run)
        self.pids.append(pid)
        return pid, so

    def pid_from(self, out, run):
        cands = [int(n) for n in re.findall(r"\d+", out)]
        live = [n for n in cands if os.path.exists("/proc/%d" % n)]
        self.assertTrue(live, "start printed no live PID: %r" % out)
        for n in live:
            try:
                with open("/proc/%d/cmdline" % n, "rb") as f:
                    cl = f.read().replace(b"\0", b" ").decode(errors="replace")
            except OSError:
                continue
            if "agentspace-sample" in cl or run in cl:
                return n
        return live[0]

    def record_path(self, run):
        return os.path.join(self.home, "agentspace-samples", run + ".jsonl")

    def records(self, run, strict=False):
        try:
            with open(self.record_path(run)) as f:
                data = f.read()
        except OSError:
            return []
        parts = data.split("\n")
        tail = parts.pop()  # Text after the last newline
        out = [json.loads(line) for line in parts if line.strip()]
        if tail.strip():
            if strict:
                out.append(json.loads(tail))
            else:
                try:
                    out.append(json.loads(tail))
                except ValueError:
                    pass
        return out

    def sessions(self, run):
        return [r for r in self.records(run) if r.get("kind") == "session"]

    def last_seq(self, run):
        seqs = [r["seq"] for r in self.records(run) if r.get("kind") == "host"]
        return max(seqs) if seqs else 0

    def wait_session(self, run, pred=lambda s: True, timeout=30):
        def look():
            for s in self.sessions(run):
                if pred(s):
                    return s
            return None
        got = wait_for(look, timeout)
        self.assertIsNotNone(got, "No matching session line within %ss" % timeout)
        return got

    def wait_end(self, run, timeout=30):
        def look():
            recs = self.records(run)
            if recs and recs[-1].get("kind") == "end":
                return recs[-1]
            return None
        got = wait_for(look, timeout)
        self.assertIsNotNone(got, "No end line within %ss" % timeout)
        return got

    def make_stub(self):
        stub = Stub(self.tmp)
        self.stubs.append(stub)
        return stub

    def assert_capitals(self, text):
        for line in text.splitlines():
            if line.strip():
                self.assertFalse(line[0].islower(),
                                 "Line starts lowercase: %r" % line)


class TestMeasurement(SampleTestBase):
    def test_cpu_burn_counts_in_tools_not_self(self):
        stub = self.make_stub()
        self.start("cpu", "--interval", "1", "--match", stub.match)
        base = self.wait_session("cpu")
        spawned = datetime.now().astimezone()
        marker = stub.spawn(BURN_CODE, "20")
        # A sample taken while the child is alive, about 10 s in: tools.cpu_s MUST already
        # count its CPU, not only the CPU of reaped children (cutime + cstime).
        mid = self.wait_session(
            "cpu",
            lambda s: s["tools"]["procs"] == 1
            and (parse_t(s["t"]) - spawned).total_seconds() >= 10,
            timeout=60)
        d_mid = mid["tools"]["cpu_s"] - base["tools"]["cpu_s"]
        elapsed = (parse_t(mid["t"]) - parse_t(base["t"])).total_seconds()
        self.assertGreaterEqual(d_mid, 5.0,
                                "Mid-burn tools.cpu_s rose by only %.2f s" % d_mid)
        self.assertLessEqual(d_mid, elapsed + 2.0,
                             "Mid-burn tools.cpu_s rose by %.2f s in %.2f s" % (d_mid, elapsed))
        self.assertTrue(wait_for(lambda: os.path.exists(marker), 90),
                        "Burn child never finished")
        seq_done = self.last_seq("cpu")
        final = self.wait_session("cpu", lambda s: s["seq"] >= seq_done + 2)
        d_tools = final["tools"]["cpu_s"] - base["tools"]["cpu_s"]
        d_self = final["self"]["cpu_s"] - base["self"]["cpu_s"]
        self.assertAlmostEqual(d_tools, 20.0, delta=2.0,
                               msg="tools.cpu_s rose by %.2f s" % d_tools)
        self.assertLess(d_self, 2.0, "self.cpu_s took %.2f s of the child's CPU" % d_self)

    def test_touched_memory_shows_in_tools_pss(self):
        stub = self.make_stub()
        self.start("mem", "--interval", "1", "--match", stub.match)
        self.wait_session("mem")
        marker = stub.spawn(MEM_CODE)
        self.assertTrue(wait_for(lambda: os.path.exists(marker), 90),
                        "Memory child never became ready")
        seq_ready = self.last_seq("mem")
        # The scan of seq_ready + 1 can begin before the child writes its marker.
        s = self.wait_session("mem", lambda s: s["seq"] > seq_ready + 1)
        self.assertGreaterEqual(s["tools"]["pss_kib"], 486 * 1024)
        self.assertGreaterEqual(s["tools"]["rss_kib"], 486 * 1024)
        self.assertLess(s["tools"]["pss_kib"], 700 * 1024,
                        "Memory looks counted more than once")
        self.assertLess(s["self"]["pss_kib"], 100 * 1024,
                        "The child's memory leaked into self")
        self.assertEqual(s["tools"]["procs"], 1)

    def test_short_build_between_samples_is_counted_next_sample(self):
        stub = self.make_stub()
        self.start("short", "--interval", "5", "--match", stub.match)
        first = self.wait_session("short")
        # Spawn right after a sample, so the child starts and ends well before the next.
        s1 = self.wait_session("short", lambda s: s["seq"] > first["seq"], timeout=20)
        marker = stub.spawn(BURN_CODE, "1")
        self.assertTrue(wait_for(lambda: os.path.exists(marker), 20),
                        "Short child never finished")
        if self.last_seq("short") != s1["seq"]:
            self.skipTest("A sample landed during the short build; load stretched it")
        s2 = self.wait_session("short", lambda s: s["seq"] == s1["seq"] + 1, timeout=20)
        gap = (parse_t(s2["t"]) - parse_t(s1["t"])).total_seconds()
        self.assertGreaterEqual(gap, 4.0, "Samples were not 5 s apart")
        self.assertEqual(s2["tools"]["procs"], 0, "Child was still alive at the sample")
        d = s2["tools"]["cpu_s"] - s1["tools"]["cpu_s"]
        self.assertAlmostEqual(d, 1.0, delta=0.35, msg="tools.cpu_s rose by %.2f s" % d)

    def test_session_mentioning_sampler_is_recorded(self):
        # With a custom --match, a session whose command line also names agentspace-sample
        # (R6) is still a session; only the sampler's own invocations are excluded.
        stub = self.make_stub()
        stub.root.kill()
        stub.root.wait(timeout=5)
        # No regex metacharacters, so the match text appears literally in the command line.
        token = "stubroot%s" % uuid.uuid4().hex[:12]
        root = subprocess.Popen(
            [sys.executable, "-c", ROOT_CODE, token, stub.cmd_dir, "agentspace-sample"],
            cwd=stub.cwd, start_new_session=True,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL)
        stub.root = root
        self.start("mention", "--interval", "1", "--match", token)
        s = self.wait_session("mention", timeout=20)
        self.assertTrue(s["session"].startswith("%d-" % root.pid))

    def test_matching_process_inside_tree_is_not_a_second_session(self):
        stub = self.make_stub()
        self.start("nest", "--interval", "1", "--match", stub.match)
        self.wait_session("nest")
        marker = stub.spawn_matching(NEST_CODE)
        self.assertTrue(wait_for(lambda: os.path.exists(marker), 30))
        seq_ready = self.last_seq("nest")
        self.wait_session("nest", lambda s: s["seq"] > seq_ready + 1)
        by_seq = {}
        for r in self.records("nest"):
            if r["kind"] == "session":
                by_seq.setdefault(r["seq"], []).append(r)
        self.assertTrue(by_seq)
        for seq, lines in by_seq.items():
            self.assertEqual(len(lines), 1, "Sample %d has %d session lines" % (seq, len(lines)))
            self.assertTrue(lines[0]["session"].startswith("%d-" % stub.root.pid))
        late = by_seq[max(by_seq)][0]
        # The inner matching process and its own child, both descendants of the outer session.
        self.assertEqual(late["tools"]["procs"], 2)
        self.assertEqual(os.path.realpath(late["cwd"]), os.path.realpath(stub.cwd))


class TestLifecycle(SampleTestBase):
    def test_second_start_refused_while_one_runs(self):
        stub = self.make_stub()
        self.start("one", "--interval", "1", "--match", stub.match)
        rc, so, se = self.sample("start", "two", "--interval", "1", "--match", stub.match)
        self.assertEqual(rc, 2, "Second start: out=%r err=%r" % (so, se))
        self.assertFalse(os.path.exists(self.record_path("two")))
        rc, so, se = self.sample("start", "one", "--interval", "1", "--match", stub.match)
        self.assertEqual(rc, 2, "Start on the existing, running record")

    def test_start_on_existing_record_refused_after_end(self):
        stub = self.make_stub()
        self.start("again", "--interval", "1", "--for", "3s", "--match", stub.match)
        self.wait_end("again")
        with open(self.record_path("again")) as f:
            before = f.read()
        rc, so, se = self.sample("start", "again", "--match", stub.match)
        self.assertEqual(rc, 2)
        with open(self.record_path("again")) as f:
            self.assertEqual(f.read(), before)
        # With no sampler running, another run name starts fine.
        self.start("another", "--interval", "1", "--for", "3s", "--match", stub.match)
        self.wait_end("another")

    def test_stop_writes_stop_file_end_line(self):
        stub = self.make_stub()
        pid, _ = self.start("stopme", "--interval", "1", "--match", stub.match)
        self.wait_session("stopme")
        rc, so, se = self.sample("stop", "stopme")
        self.assertEqual(rc, 0, "stop: out=%r err=%r" % (so, se))
        self.assertIn("stop-file", so)
        self.assert_capitals(so)
        end = self.wait_end("stopme", 5)
        self.assertEqual(end["reason"], "stop-file")
        self.assertTrue(wait_for(lambda: not os.path.exists("/proc/%d" % pid), 5),
                        "Sampler did not exit after its end line")
        # Stop on a run whose sampler already ended: the reason again, exit 0.
        rc, so, se = self.sample("stop", "stopme")
        self.assertEqual(rc, 0)
        self.assertIn("stop-file", so)

    def test_stop_on_unknown_run_exits_2(self):
        rc, so, se = self.sample("stop", "nosuchrun")
        self.assertEqual(rc, 2)

    def test_short_for_ends_with_deadline_by_itself(self):
        stub = self.make_stub()
        pid, _ = self.start("short-for", "--interval", "1", "--for", "6s",
                            "--match", stub.match)
        end = self.wait_end("short-for", 20)
        self.assertEqual(end["reason"], "deadline")
        self.assertTrue(wait_for(lambda: not os.path.exists("/proc/%d" % pid), 5),
                        "Sampler outlived its deadline")
        rc, so, se = self.sample("stop", "short-for")
        self.assertEqual(rc, 0)
        self.assertIn("deadline", so)

    def test_sigterm_and_sigint_write_signal_end_line(self):
        for name, sig in (("sigterm", signal.SIGTERM), ("sigint", signal.SIGINT)):
            with self.subTest(sig=name):
                # One sampler at a time, so each subtest gets a fresh HOME.
                self.home = os.path.join(self.tmp, "home-" + name)
                os.makedirs(self.home)
                self.env = dict(os.environ, HOME=self.home)
                stub = self.make_stub()
                pid, _ = self.start(name, "--interval", "1", "--match", stub.match)
                self.wait_session(name)
                os.kill(pid, sig)
                end = self.wait_end(name, 10)
                self.assertEqual(end["reason"], "signal")
                self.assertTrue(wait_for(lambda: not os.path.exists("/proc/%d" % pid), 5))

    def test_status_running_and_after_end(self):
        rc, so, se = self.sample("status")
        self.assertEqual(rc, 0)
        self.assertIn("No sampler is running.", so)
        stub = self.make_stub()
        pid, _ = self.start("stat", "--interval", "1", "--match", stub.match)
        self.wait_session("stat")
        rc, so, se = self.sample("status")
        self.assertEqual(rc, 0)
        self.assertIn("stat", so)
        self.assertIn(str(pid), so)
        self.assertNotIn("No sampler is running.", so)
        self.assertRegex(so, CLOCK_RE, "Deadline is not shown as h:mm am or pm")
        self.assert_capitals(so)
        self.sample("stop", "stat")
        rc, so, se = self.sample("status")
        self.assertEqual(rc, 0)
        self.assertIn("No sampler is running.", so)


class TestArguments(SampleTestBase):
    def assert_refused(self, *args):
        rc, so, se = self.sample(*args)
        self.assertEqual(rc, 2, "Args %r: rc=%s out=%r err=%r" % (args, rc, so, se))
        self.assertFalse(os.path.exists(self.record_path("bad")))
        self.assert_capitals(se)

    def test_bad_arguments_exit_2(self):
        self.assert_refused()
        self.assert_refused("bogus")
        self.assert_refused("start")
        self.assert_refused("stop")
        self.assert_refused("start", "Bad_Name")
        self.assert_refused("start", "-bad")
        self.assert_refused("start", "a" * 65)
        self.assert_refused("start", "bad", "--interval", "0.4")
        self.assert_refused("start", "bad", "--interval", "61")
        self.assert_refused("start", "bad", "--interval", "abc")
        self.assert_refused("start", "bad", "--for", "13h")
        self.assert_refused("start", "bad", "--for", "soon")
        self.assert_refused("start", "bad", "--nonsense")

    def test_defaults_and_limits_accepted(self):
        pid, so = self.start("defaults")
        self.assertIn("defaults.jsonl", so)
        self.assert_capitals(so)
        start = self.records("defaults")[0]
        self.assertEqual(start["interval_s"], 2)
        self.assertTrue(isinstance(start["match"], str) and start["match"])
        span = (parse_t(start["deadline"]) - parse_t(start["t"])).total_seconds()
        self.assertAlmostEqual(span, 8 * 3600, delta=5)
        self.sample("stop", "defaults")
        # The edges: 0.5 s and 12h are accepted.
        self.start("edges", "--interval", "0.5", "--for", "12h")
        rec = self.records("edges")[0]
        self.assertEqual(rec["interval_s"], 0.5)
        span = (parse_t(rec["deadline"]) - parse_t(rec["t"])).total_seconds()
        self.assertAlmostEqual(span, 12 * 3600, delta=5)
        self.sample("stop", "edges")


class TestRecordFormat(SampleTestBase):
    def test_record_parses_and_matches_format(self):
        stub = self.make_stub()
        self.start("fmt", "--interval", "1", "--for", "6s", "--match", stub.match)
        self.wait_end("fmt", 20)
        recs = self.records("fmt", strict=True)
        self.assertGreaterEqual(len(recs), 4)
        # Every raw line parses as JSON, and the last ends with a newline.
        with open(self.record_path("fmt")) as f:
            raw = f.read()
        self.assertTrue(raw.endswith("\n"))
        for line in raw.splitlines():
            json.loads(line)

        for r in recs:
            self.assertRegex(r["t"], T_RE)

        start = recs[0]
        self.assertEqual(start["kind"], "start")
        for k in ("t", "run", "interval_s", "deadline", "match", "cpus",
                  "mem_total_kib", "sampler"):
            self.assertIn(k, start)
        self.assertEqual(start["run"], "fmt")
        self.assertEqual(start["interval_s"], 1)
        self.assertEqual(start["match"], stub.match)
        self.assertIsInstance(start["cpus"], int)
        self.assertGreaterEqual(start["cpus"], 1)
        self.assertIsInstance(start["mem_total_kib"], int)
        self.assertGreater(start["mem_total_kib"], 0)
        self.assertTrue(start["sampler"].startswith("agentspace-sample "))
        self.assertRegex(start["deadline"], T_RE)
        span = (parse_t(start["deadline"]) - parse_t(start["t"])).total_seconds()
        self.assertAlmostEqual(span, 6, delta=2)

        end = recs[-1]
        self.assertEqual(end["kind"], "end")
        self.assertEqual(end["reason"], "deadline")
        self.assertEqual(sum(1 for r in recs if r["kind"] == "start"), 1)
        self.assertEqual(sum(1 for r in recs if r["kind"] == "end"), 1)

        body = recs[1:-1]
        hosts = [r for r in body if r["kind"] == "host"]
        sessions = [r for r in body if r["kind"] == "session"]
        self.assertEqual(len(hosts) + len(sessions), len(body), "Unknown line kind")
        self.assertGreaterEqual(len(hosts), 3)
        self.assertLessEqual(len(hosts), 8)
        # Hosts count from 1 with no gaps, and each sample's host line comes first.
        self.assertEqual([h["seq"] for h in hosts], list(range(1, len(hosts) + 1)))
        self.assertEqual(end["samples"], len(hosts))
        current = 0
        for r in body:
            if r["kind"] == "host":
                current = r["seq"]
            else:
                self.assertEqual(r["seq"], current, "Session line outside its sample")

        prev = None
        for h in hosts:
            for k in ("cpu_busy_s", "cpu_total_s", "mem_used_kib", "load1", "sampler_cpu_s"):
                self.assertIn(k, h)
            self.assertIsInstance(h["cpu_busy_s"], (int, float))
            self.assertIsInstance(h["cpu_total_s"], (int, float))
            self.assertIsInstance(h["mem_used_kib"], int)
            self.assertIsInstance(h["load1"], (int, float))
            self.assertIsInstance(h["sampler_cpu_s"], (int, float))
            self.assertLessEqual(h["cpu_busy_s"], h["cpu_total_s"])
            self.assertGreater(h["mem_used_kib"], 0)
            self.assertLessEqual(h["mem_used_kib"], start["mem_total_kib"])
            if prev:
                self.assertGreaterEqual(h["cpu_total_s"], prev["cpu_total_s"])
                self.assertGreaterEqual(h["sampler_cpu_s"], prev["sampler_cpu_s"])
                self.assertGreater(parse_t(h["t"]), parse_t(prev["t"]))
            prev = h
        for a, b in zip(hosts, hosts[1:]):
            g = (parse_t(b["t"]) - parse_t(a["t"])).total_seconds()
            self.assertTrue(0.5 <= g <= 2.0, "Sample gap of %.2f s" % g)

        # The sampler's own cost is counted, and tiny at this interval.
        self.assertLess(hosts[-1]["sampler_cpu_s"], 3.0)

        # The stub root is the only session in every sample. The sampler's own
        # command line carries the match too, and MUST NOT be counted.
        self.assertEqual(len(sessions), len(hosts))
        for s in sessions:
            for k in ("session", "cwd", "self", "tools"):
                self.assertIn(k, s)
            self.assertRegex(s["session"], r"^\d+-\d+$")
            self.assertTrue(s["session"].startswith("%d-" % stub.root.pid))
            self.assertIsInstance(s["cwd"], str)
            self.assertEqual(os.path.realpath(s["cwd"]), os.path.realpath(stub.cwd))
            self.assertEqual({"cpu_s", "pss_kib", "rss_kib"} - set(s["self"]), set())
            self.assertEqual({"cpu_s", "pss_kib", "rss_kib", "procs"} - set(s["tools"]), set())
            self.assertIsInstance(s["self"]["cpu_s"], (int, float))
            self.assertIsInstance(s["self"]["pss_kib"], int)
            self.assertIsInstance(s["self"]["rss_kib"], int)
            self.assertIsInstance(s["tools"]["cpu_s"], (int, float))
            self.assertIsInstance(s["tools"]["pss_kib"], int)
            self.assertIsInstance(s["tools"]["rss_kib"], int)
            self.assertIsInstance(s["tools"]["procs"], int)
            self.assertGreater(s["self"]["pss_kib"], 0)
            self.assertGreater(s["self"]["rss_kib"], 0)
            self.assertEqual(s["tools"]["procs"], 0)
        self.assertEqual(len(set(s["session"] for s in sessions)), 1)

    def test_dead_session_gets_no_session_line(self):
        stub = self.make_stub()
        self.start("gone", "--interval", "1", "--match", stub.match)
        self.wait_session("gone")
        os.killpg(stub.root.pid, signal.SIGKILL)
        stub.root.wait(timeout=5)
        seq = self.last_seq("gone")
        wait_for(lambda: self.last_seq("gone") >= seq + 3, 15)
        recs = self.records("gone")
        self.assertTrue([r for r in recs if r["kind"] == "host" and r["seq"] >= seq + 2])
        late = [r for r in recs if r["kind"] == "session" and r["seq"] >= seq + 2]
        self.assertEqual(late, [], "A dead session still has session lines")


if __name__ == "__main__":
    unittest.main()

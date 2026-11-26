"""Tests for box/system/agentspace-idle-check (E0.08-T01-S05, card #0172, D11 and D57).

RFC 2119 keywords; the capitals are load-bearing.

Run on Linux (WSL is enough), from the repository's root:
python3 -I -m unittest discover -v -s box/tests -p test_agentspace_idle.py

The script is driven as a subprocess and MUST NOT be imported. Its path comes from the
AGENTSPACE_IDLE_CHECK environment variable, defaulting to ../system/agentspace-idle-check
relative to this file. Each test builds a throwaway tree: a state folder, an idle.conf, a fake
/proc/stat and a folder of stub commands (ss, pgrep, systemctl) put first on PATH. The
real /run, /etc and /proc are never read, and `systemctl poweroff` is only ever a stub that
records its arguments. A test MUST NOT print the values of the environment it builds.

Every run of the script MUST exit 0, and every case MUST assert the line it expects, so a script that
crashes early cannot pass a case that only looks for the absence of a power-off.

The stubs:
- ss prints nothing, or, when the test asks for an SSH session, one ESTAB line (no header, as
  with `ss -H`; a header would read as a session to a script that tests for any output). That is
  its answer to the `sport = :22` form. Asked the `dport = :22` form (D78's seam, with `-p`), it
  prints the lines in its `ss-dport` file, each as `ss -Htnp` prints it, with a
  `users:(("ssm-session-wor",pid=<PID>,fd=<n>))` field. With no `ss-dport` and no `workers` file, a
  listed worker (PID 4242, what pgrep prints) holds a connection, so the older tests that only list
  `ssm-session-worker` still mean "a session is open".
- pgrep matches each non-option argument, as an extended regular expression, against the
  names of the "processes" the test lists in the stub folder's `procs` file, and prints one PID a
  line: the lines of the `workers` file when the match is the Session Manager worker and there is
  one, else 4242. `pgrep -P <PID>` prints the second field of each line of the `children` file
  whose first field is that PID, and fails when there is none.
- systemctl records its arguments to `calls`, answers `is-active` from `procs` and does nothing else.
- date (only in the cases that need it) fails for `date -d @<secs>`, or answers `date +%s` with the
  number in the stub folder's `now` file, and passes everything else to the real date.

The script's CPU reading is driven by two runs: the first, with no state, takes the baseline;
the second sees a /proc/stat with more (or the same) CPU time. That keeps the tests clear of the
state file's own format.
"""

import os
import shutil
import stat
import subprocess
import tempfile
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.environ.get(
    "AGENTSPACE_IDLE_CHECK",
    os.path.join(HERE, os.pardir, "system", "agentspace-idle-check"),
)

SSM_PROC = "ssm-session-worker"
PATCH_PROC = "/usr/local/sbin/agentspace-patch"

IDLE_CONF = """IDLE_MINUTES=60
BUSY_CPU_SECONDS=6
HORIZON_GRACE_MINUTES=15
"""

SS_STUB = r"""#!/bin/bash
d="$(dirname "$0")"
case "$*" in
  *dport*)
    if [ -e "$d/ss-dport" ]; then
      cat "$d/ss-dport"
    elif [ ! -e "$d/workers" ] && [ -s "$d/procs" ] && grep -Eq -- 'ssm-session-worker' "$d/procs"; then
      echo '0 0 127.0.0.1:41022 127.0.0.1:22 users:(("ssm-session-wor",pid=4242,fd=9))'
    fi
    exit 0 ;;
esac
if [ -e "$d/ssh" ]; then
  echo "ESTAB 0 0 198.51.100.5:22 192.0.2.7:51234"
fi
exit 0
"""

PGREP_STUB = r"""#!/bin/bash
d="$(dirname "$0")"
parent=""
pats=()
while [ $# -gt 0 ]; do
  case "$1" in
    -P) parent="$2"; shift 2; continue ;;
    -P*) parent="${1#-P}" ;;
    -*) ;;
    *) pats+=("$1") ;;
  esac
  shift
done
if [ -n "$parent" ]; then
  [ -s "$d/children" ] || exit 1
  out="$(awk -v p="$parent" '$1 == p { print $2 }' "$d/children")"
  [ -n "$out" ] || exit 1
  echo "$out"
  exit 0
fi
found=0
ssm=0
for a in "${pats[@]}"; do
  if [ -s "$d/procs" ] && grep -Eq -- "$a" "$d/procs"; then
    found=1
    if grep -E -- "$a" "$d/procs" | grep -q 'ssm-session-worker'; then ssm=1; fi
  fi
done
if [ "$found" = 1 ]; then
  if [ "$ssm" = 1 ] && [ -s "$d/workers" ]; then cat "$d/workers"; else echo 4242; fi
  exit 0
fi
exit 1
"""

SYSTEMCTL_STUB = r"""#!/bin/bash
d="$(dirname "$0")"
echo "$*" >> "$d/calls"
case "$1" in
  is-active)
    shift
    for a in "$@"; do
      case "$a" in -*) continue ;; esac
      if [ -s "$d/procs" ] && grep -Eq -- "${a%.service}" "$d/procs"; then
        echo active; exit 0
      fi
    done
    echo inactive; exit 3 ;;
esac
exit 0
"""

SAFE_PATH = "/usr/local/bin:/usr/bin:/bin"


def stat_text(user_jiffies):
    return ("cpu  %d 0 0 1000000 0 0 0 0 0 0\n"
            "cpu0 %d 0 0 1000000 0 0 0 0 0 0\n"
            "intr 1\nctxt 1\nbtime 1\nprocesses 1\n" % (user_jiffies, user_jiffies))


class IdleCheckBase(unittest.TestCase):
    tz = "America/New_York"

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="idle-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.state = os.path.join(self.root, "state")
        self.stubs = os.path.join(self.root, "stubs")
        self.conf = os.path.join(self.root, "idle.conf")
        self.proc_stat = os.path.join(self.root, "stat")
        os.mkdir(self.state)
        os.mkdir(self.stubs)
        for name, body in (("ss", SS_STUB), ("pgrep", PGREP_STUB),
                           ("systemctl", SYSTEMCTL_STUB)):
            p = os.path.join(self.stubs, name)
            with open(p, "w") as fh:
                fh.write(body)
            os.chmod(p, 0o755)
        with open(os.path.join(self.stubs, "procs"), "w"):
            pass
        with open(self.conf, "w") as fh:
            fh.write(IDLE_CONF)
        self.set_cpu(1000)
        self.base_ran = False

    # Scenario helpers.
    def set_cpu(self, user_jiffies):
        with open(self.proc_stat, "w") as fh:
            fh.write(stat_text(user_jiffies))

    def set_procs(self, *names):
        with open(os.path.join(self.stubs, "procs"), "w") as fh:
            fh.write("\n".join(names) + ("\n" if names else ""))

    def set_ssh(self, on):
        p = os.path.join(self.stubs, "ssh")
        if on:
            open(p, "w").close()
        elif os.path.exists(p):
            os.remove(p)

    def set_workers(self, *pids):
        """List Session Manager workers, one PID a line, as `pgrep -f` prints them (D78)."""
        procs = os.path.join(self.stubs, "procs")
        with open(procs) as fh:
            have = fh.read().split()
        if SSM_PROC not in have:
            with open(procs, "a") as fh:
                fh.write(SSM_PROC + "\n")
        with open(os.path.join(self.stubs, "workers"), "w") as fh:
            fh.write("".join("%d\n" % p for p in pids))

    def add_ss_dport(self, pid, comm="ssm-session-wor"):
        """One established connection to port 22, held by the process pid, as `ss -Htnp` prints it."""
        with open(os.path.join(self.stubs, "ss-dport"), "a") as fh:
            fh.write('0 0 127.0.0.1:%d 127.0.0.1:22 users:(("%s",pid=%d,fd=9))\n'
                     % (40000 + pid % 20000, comm, pid))

    def add_child(self, parent, child):
        with open(os.path.join(self.stubs, "children"), "a") as fh:
            fh.write("%d %d\n" % (parent, child))

    def set_clock(self, secs):
        """Answer `date +%s` with secs; every other use of date goes to the real one."""
        real_date = shutil.which("date", path=SAFE_PATH)
        self.assertTrue(real_date, "The real date MUST be found on the safe path")
        with open(os.path.join(self.stubs, "now"), "w") as fh:
            fh.write("%d\n" % secs)
        p = os.path.join(self.stubs, "date")
        with open(p, "w") as fh:
            fh.write("#!/bin/bash\n"
                     'd="$(dirname "$0")"\n'
                     'case "$*" in *-d*) ;; *"+%%s") cat "$d/now"; exit 0 ;; esac\n'
                     'exec "%s" "$@"\n' % real_date)
        os.chmod(p, 0o755)

    def set_last_active(self, minutes_ago):
        """Age the last-active record, by its content and its mtime, so either reading holds."""
        t = int(time.time()) - int(minutes_ago * 60)
        p = os.path.join(self.state, "last-active")
        with open(p, "w") as fh:
            fh.write("%d\n" % t)
        os.utime(p, (t, t))

    def set_horizon(self, minutes_from_now):
        """Write lease-horizon, in Unix seconds and a line feed; return the seconds."""
        h = int(time.time()) + int(minutes_from_now * 60)
        with open(os.path.join(self.state, "lease-horizon"), "w") as fh:
            fh.write("%d\n" % h)
        return h

    def set_horizon_raw(self, text):
        with open(os.path.join(self.state, "lease-horizon"), "w") as fh:
            fh.write(text)

    def calls(self):
        p = os.path.join(self.stubs, "calls")
        if not os.path.exists(p):
            return []
        with open(p) as fh:
            return [ln.strip() for ln in fh if ln.strip()]

    def poweroffs(self):
        return [c for c in self.calls() if "poweroff" in c.split()]

    def clear_calls(self):
        p = os.path.join(self.stubs, "calls")
        if os.path.exists(p):
            os.remove(p)

    def run_check(self):
        env = {
            "PATH": self.stubs + ":" + SAFE_PATH,
            "HOME": self.root,
            "LC_ALL": "C",
            "TZ": self.tz,
            "AGENTSPACE_IDLE_STATE": self.state,
            "AGENTSPACE_IDLE_CONF": self.conf,
            "AGENTSPACE_PROC_STAT": self.proc_stat,
        }
        argv = [SCRIPT]
        if not os.access(SCRIPT, os.X_OK):
            argv = ["bash", SCRIPT]
        r = subprocess.run(argv, env=env, stdin=subprocess.DEVNULL,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True, timeout=60, cwd=self.root)
        self.assertEqual(r.returncode, 0, "The script MUST exit 0: " + r.stdout)
        return r.returncode, r.stdout

    def assertLine(self, out, prefix):
        """Assert that one line of the script's output starts with prefix."""
        self.assertTrue(any(ln.startswith(prefix) for ln in out.splitlines()),
                        "Expected a line starting %r in: %s" % (prefix, out))

    def baseline(self):
        """The first run, with no state: it takes the CPU baseline and starts a fresh window."""
        self.clear_calls()
        code, out = self.run_check()
        self.assertEqual(code, 0, out)
        self.assertEqual(self.poweroffs(), [], "The baseline run MUST NOT power off: " + out)
        self.base_ran = True
        self.clear_calls()

    def expected_clock(self, secs):
        env = {"PATH": SAFE_PATH, "TZ": self.tz, "LC_ALL": "C"}
        r = subprocess.run(["date", "-d", "@%d" % secs, "+%-I:%M%P"], env=env,
                           stdout=subprocess.PIPE, text=True, check=True)
        return r.stdout.strip()


class D11Rule(IdleCheckBase):
    """D11's rule, unchanged: no horizon file in any of these."""

    def test_idle_sixty_plus_minutes_powers_off(self):
        self.baseline()
        self.set_last_active(70)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        self.assertLine(out, "Idle for 70 min (limit 60); powering off")

    def test_idle_under_sixty_minutes_does_not(self):
        self.baseline()
        self.set_last_active(30)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Idle for 30 min of 60")

    def test_ssh_session_present_does_not(self):
        self.baseline()
        self.set_last_active(70)
        self.set_ssh(True)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: SSH session open")

    def test_session_manager_session_present_does_not(self):
        self.baseline()
        self.set_last_active(70)
        self.set_procs(SSM_PROC)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: Session Manager session open")

    def test_patch_running_does_not(self):
        self.baseline()
        self.set_last_active(70)
        self.set_procs(PATCH_PROC)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: Patching")

    def test_cpu_busy_does_not(self):
        self.baseline()
        self.set_last_active(70)
        self.set_cpu(1000 + 100000)  # About 1000 CPU-seconds since the baseline.
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")

    def test_cpu_idle_same_reading_powers_off(self):
        # The control for the busy case: the same two-run shape, the same stat, powers off.
        self.baseline()
        self.set_last_active(70)
        self.set_cpu(1000)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        self.assertLine(out, "Idle for 70 min (limit 60); powering off")


class HorizonRule(IdleCheckBase):
    """D57: past horizon plus grace, with no session, power off whatever the CPU is doing."""

    def busy_run(self):
        self.baseline()
        self.set_cpu(1000 + 100000)
        return self.run_check()

    def test_past_horizon_plus_grace_cpu_busy_no_session_powers_off(self):
        self.baseline()
        h = self.set_horizon(-25)  # Horizon 25 minutes ago; grace 15.
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = ("Past lease horizon %s plus 15 min with no session; powering off"
                % self.expected_clock(h))
        self.assertIn(want, out)

    def test_horizon_message_uses_the_grace_from_idle_conf(self):
        with open(self.conf, "w") as fh:
            fh.write(IDLE_CONF.replace("HORIZON_GRACE_MINUTES=15", "HORIZON_GRACE_MINUTES=7"))
        self.baseline()
        h = self.set_horizon(-12)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = ("Past lease horizon %s plus 7 min with no session; powering off"
                % self.expected_clock(h))
        self.assertIn(want, out)

    def test_invalid_grace_falls_back_to_15_with_a_warning(self):
        with open(self.conf, "w") as fh:
            fh.write(IDLE_CONF.replace("HORIZON_GRACE_MINUTES=15", "HORIZON_GRACE_MINUTES=abc"))
        self.baseline()
        h = self.set_horizon(-1000)  # Long past, so the fallback grace is not the question.
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        self.assertIn("Invalid HORIZON_GRACE_MINUTES in %s; using 15" % self.conf, out)
        want = ("Past lease horizon %s plus 15 min with no session; powering off"
                % self.expected_clock(h))
        self.assertIn(want, out)

    def test_horizon_time_falls_back_to_the_seconds_when_date_cannot_format_it(self):
        real_date = shutil.which("date", path=SAFE_PATH)
        self.assertTrue(real_date, "The real date MUST be found on the safe path")
        p = os.path.join(self.stubs, "date")
        with open(p, "w") as fh:
            fh.write("#!/bin/bash\n"
                     'case "$1" in -d) case "$2" in @*) exit 1 ;; esac ;; esac\n'
                     'exec "%s" "$@"\n' % real_date)
        os.chmod(p, 0o755)
        self.baseline()
        h = self.set_horizon(-25)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = "Past lease horizon @%d plus 15 min with no session; powering off" % h
        self.assertIn(want, out)

    def test_past_horizon_with_ssh_session_does_not(self):
        self.baseline()
        self.set_horizon(-25)
        self.set_ssh(True)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: SSH session open")

    def test_past_horizon_with_session_manager_session_does_not(self):
        self.baseline()
        self.set_horizon(-25)
        self.set_procs(SSM_PROC)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: Session Manager session open")

    def test_past_horizon_with_patch_running_does_not(self):
        # P9 (architect): a running patch run holds off the horizon power-off, as D11's rule does.
        self.baseline()
        self.set_horizon(-25)
        self.set_procs(PATCH_PROC)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: Patching")

    def test_inside_the_grace_does_not(self):
        self.baseline()
        self.set_horizon(-5)  # Past the horizon, not past horizon plus 15.
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")

    def test_horizon_in_the_future_does_not(self):
        self.baseline()
        self.set_horizon(30)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")

    def test_no_horizon_file_keeps_d11_rule_cpu_busy(self):
        code, out = self.busy_run()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")

    def test_garbage_horizon_keeps_d11_rule_cpu_busy(self):
        self.baseline()
        self.set_horizon_raw("not a number\n")
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")

    def test_garbage_horizon_keeps_d11_rule_idle_powers_off(self):
        self.baseline()
        self.set_horizon_raw("not a number\n")
        self.set_last_active(70)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        self.assertLine(out, "Idle for 70 min (limit 60); powering off")
        self.assertNotIn("Past lease horizon", out)

    def test_empty_horizon_keeps_d11_rule_cpu_busy(self):
        self.baseline()
        self.set_horizon_raw("")
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")

    def test_unreadable_horizon_keeps_d11_rule_cpu_busy(self):
        # A directory where the file belongs cannot be read as one, even by root.
        self.baseline()
        os.mkdir(os.path.join(self.state, "lease-horizon"))
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")

    def test_horizon_does_not_stop_d11_idle_power_off(self):
        # A live horizon in the future leaves D11's idle rule alone.
        self.baseline()
        self.set_horizon(30)
        self.set_last_active(70)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        self.assertLine(out, "Idle for 70 min (limit 60); powering off")
        self.assertNotIn("Past lease horizon", out)


class HorizonParsing(IdleCheckBase):
    """#0192: what counts as a horizon (operator.md: 1 to 15 decimal digits and nothing else).

    Each case lists a horizon that, were it used, is long past (so the horizon rule would power
    off, CPU busy, no session). An ignored one leaves D11's rule, which the CPU keeps awake.
    """

    def horizon_run(self, text):
        self.baseline()
        self.set_horizon_raw(text)
        self.set_cpu(1000 + 100000)
        return self.run_check()

    def assertHorizonIgnored(self, out):
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")
        self.assertNotIn("Past lease horizon", out)

    def test_horizon_ending_in_cr_and_lf_is_not_a_usable_horizon(self):
        # Operator.md: a CR makes the first line no horizon, ignored without a word.
        h = int(time.time()) - 25 * 60
        code, out = self.horizon_run("%d\r\n" % h)
        self.assertHorizonIgnored(out)

    def test_horizon_ending_in_a_bare_cr_is_not_a_usable_horizon(self):
        h = int(time.time()) - 25 * 60
        code, out = self.horizon_run("%d\r" % h)
        self.assertHorizonIgnored(out)

    def test_horizon_ending_in_cr_keeps_d11_idle_power_off(self):
        # With a CR horizon ignored, D11's idle rule still stops an idle host, with its own line.
        self.baseline()
        self.set_horizon_raw("%d\r\n" % (int(time.time()) - 25 * 60))
        self.set_last_active(70)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        self.assertLine(out, "Idle for 70 min (limit 60); powering off")
        self.assertNotIn("Past lease horizon", out)

    def test_sixteen_digit_horizon_is_not_a_usable_horizon(self):
        # Sixteen digits (leading zeros make it a past time): the first line is 1 to 15 digits.
        code, out = self.horizon_run("0000000000000001\n")
        self.assertHorizonIgnored(out)

    def test_fifteen_digit_horizon_is_a_usable_horizon(self):
        # The control for the 16-digit case: 15 digits is the longest horizon the check reads.
        code, out = self.horizon_run("000000000000001\n")
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = ("Past lease horizon %s plus 15 min with no session; powering off"
                % self.expected_clock(1))
        self.assertIn(want, out)

    def test_horizon_with_no_line_feed_is_a_usable_horizon(self):
        # The control for the CR cases: only the CR makes the line unusable, not a missing LF.
        h = int(time.time()) - 25 * 60
        code, out = self.horizon_run("%d" % h)
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = ("Past lease horizon %s plus 15 min with no session; powering off"
                % self.expected_clock(h))
        self.assertIn(want, out)


class HorizonBoundary(IdleCheckBase):
    """#0192: one second before the horizon plus the grace D11's rule applies; at it, D57's does.

    The clock is fixed through a `date +%s` stand-in, so the boundary second is exact.
    """

    def boundary_run(self, seconds_to_bound):
        """Run with the horizon plus the 15 minutes' grace `seconds_to_bound` seconds ahead."""
        self.baseline()
        now = int(time.time())
        h = now + seconds_to_bound - 15 * 60
        self.set_clock(now)
        self.set_horizon_raw("%d\n" % h)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        return h, out

    def test_one_second_before_horizon_plus_grace_d11_rule_applies(self):
        h, out = self.boundary_run(1)
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")
        self.assertNotIn("Past lease horizon", out)

    def test_at_horizon_plus_grace_the_horizon_rule_applies(self):
        h, out = self.boundary_run(0)
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = ("Past lease horizon %s plus 15 min with no session; powering off"
                % self.expected_clock(h))
        self.assertIn(want, out)

    def test_one_second_after_horizon_plus_grace_the_horizon_rule_applies(self):
        h, out = self.boundary_run(-1)
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = ("Past lease horizon %s plus 15 min with no session; powering off"
                % self.expected_clock(h))
        self.assertIn(want, out)


class DefaultGrace(IdleCheckBase):
    """#0192: with HORIZON_GRACE_MINUTES unset, or no settings file, the grace is 15 minutes."""

    def unset_grace(self):
        with open(self.conf, "w") as fh:
            fh.write("IDLE_MINUTES=60\nBUSY_CPU_SECONDS=6\n")

    def test_unset_grace_14_minutes_past_horizon_does_not_power_off(self):
        self.unset_grace()
        self.baseline()
        self.set_horizon(-14)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")

    def test_unset_grace_16_minutes_past_horizon_powers_off_with_15(self):
        self.unset_grace()
        self.baseline()
        h = self.set_horizon(-16)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = ("Past lease horizon %s plus 15 min with no session; powering off"
                % self.expected_clock(h))
        self.assertIn(want, out)
        self.assertNotIn("Invalid HORIZON_GRACE_MINUTES", out)

    def test_missing_settings_file_uses_the_default_grace_of_15(self):
        os.remove(self.conf)
        self.baseline()
        h = self.set_horizon(-16)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        want = ("Past lease horizon %s plus 15 min with no session; powering off"
                % self.expected_clock(h))
        self.assertIn(want, out)

    def test_missing_settings_file_14_minutes_past_horizon_does_not_power_off(self):
        os.remove(self.conf)
        self.baseline()
        self.set_horizon(-14)
        self.set_cpu(1000 + 100000)
        code, out = self.run_check()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: CPU work:")


class SessionManagerWorker(IdleCheckBase):
    """D78: a worker counts only while it holds an established connection to sshd or has a child.

    Every case is 25 minutes past the horizon (grace 15) with the CPU busy, so a session is the only
    thing between the host and the horizon power-off.
    """

    def past_horizon(self):
        self.baseline()
        self.set_horizon(-25)
        self.set_cpu(1000 + 100000)
        return self.run_check()

    def assertHeldBySessionManager(self, out):
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: Session Manager session open")

    def assertPoweredOffByHorizon(self, out):
        self.assertEqual(len(self.poweroffs()), 1, out)
        self.assertIn("with no session; powering off", out)
        self.assertNotIn("Active: Session Manager session open", out)

    def test_worker_with_an_established_sshd_connection_counts(self):
        self.set_workers(5101)
        self.add_ss_dport(5101)
        code, out = self.past_horizon()
        self.assertHeldBySessionManager(out)

    def test_worker_running_a_shell_counts(self):
        self.set_workers(5102)
        self.add_child(5102, 5103)
        code, out = self.past_horizon()
        self.assertHeldBySessionManager(out)

    def test_worker_with_neither_does_not_count_past_the_horizon(self):
        self.set_workers(5104)
        code, out = self.past_horizon()
        self.assertPoweredOffByHorizon(out)
        self.assertIn("Past lease horizon", out)

    def test_worker_with_neither_does_not_hold_off_d11_idle_power_off(self):
        self.set_workers(5105)
        self.baseline()
        self.set_last_active(70)
        code, out = self.run_check()
        self.assertEqual(len(self.poweroffs()), 1, out)
        self.assertLine(out, "Idle for 70 min (limit 60); powering off")

    def test_worker_with_a_connection_holds_off_d11_idle_power_off(self):
        self.set_workers(5106)
        self.add_ss_dport(5106)
        self.baseline()
        self.set_last_active(70)
        code, out = self.run_check()
        self.assertHeldBySessionManager(out)

    def test_connection_held_by_another_process_does_not_count_the_worker(self):
        self.set_workers(5107)
        self.add_ss_dport(5108, comm="nc")
        code, out = self.past_horizon()
        self.assertPoweredOffByHorizon(out)

    def test_connection_of_a_longer_pid_does_not_count_a_shorter_worker_pid(self):
        # Worker 42 has no connection; another process, 4242, has one.
        self.set_workers(42)
        self.add_ss_dport(4242, comm="nc")
        code, out = self.past_horizon()
        self.assertPoweredOffByHorizon(out)

    def test_child_of_another_process_does_not_count_the_worker(self):
        self.set_workers(5109)
        self.add_child(5110, 5111)
        code, out = self.past_horizon()
        self.assertPoweredOffByHorizon(out)

    def test_every_worker_is_looked_at_not_only_the_first_for_a_connection(self):
        self.set_workers(5112, 5113)
        self.add_ss_dport(5113)
        code, out = self.past_horizon()
        self.assertHeldBySessionManager(out)

    def test_every_worker_is_looked_at_not_only_the_first_for_a_shell(self):
        self.set_workers(5114, 5115)
        self.add_child(5115, 5116)
        code, out = self.past_horizon()
        self.assertHeldBySessionManager(out)

    def test_two_bare_workers_do_not_count(self):
        self.set_workers(5117, 5118)
        code, out = self.past_horizon()
        self.assertPoweredOffByHorizon(out)

    def test_ssh_session_still_counts_beside_a_bare_worker(self):
        self.set_workers(5119)
        self.set_ssh(True)
        code, out = self.past_horizon()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: SSH session open")

    def test_ssh_session_alone_still_counts_with_no_worker(self):
        self.set_ssh(True)
        code, out = self.past_horizon()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: SSH session open")

    def test_patch_run_still_holds_off_beside_a_bare_worker(self):
        self.set_workers(5120)
        self.set_procs(SSM_PROC, PATCH_PROC)
        code, out = self.past_horizon()
        self.assertEqual(self.poweroffs(), [], out)
        self.assertLine(out, "Active: Patching")


if __name__ == "__main__":
    unittest.main()

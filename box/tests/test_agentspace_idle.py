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
  with `ss -H`; a header would read as a session to a script that tests for any output).
- pgrep matches each non-option argument, as an extended regular expression, against the
  names of the "processes" the test lists in the stub folder's `procs` file.
- systemctl records its arguments to `calls`, answers `is-active` from `procs` and does nothing else.
- date (only in the case that needs it) fails for `date -d @<secs>` and passes everything else to the
  real date.

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
if [ -e "$d/ssh" ]; then
  echo "ESTAB 0 0 198.51.100.5:22 192.0.2.7:51234"
fi
exit 0
"""

PGREP_STUB = r"""#!/bin/bash
d="$(dirname "$0")"
found=0
for a in "$@"; do
  case "$a" in -*) continue ;; esac
  if [ -s "$d/procs" ] && grep -Eq -- "$a" "$d/procs"; then found=1; fi
done
if [ "$found" = 1 ]; then echo 4242; exit 0; fi
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


if __name__ == "__main__":
    unittest.main()

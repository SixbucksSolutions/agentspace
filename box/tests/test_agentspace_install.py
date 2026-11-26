"""Tests for box/system/install.sh's /etc/environment edit (E0.12-T02-S03, card #0252, gap #0193).

RFC 2119 keywords; the capitals are load-bearing.

Run on Linux (WSL is enough), from the repository's root:
python3 -I -m unittest discover -v -s box/tests -p test_agentspace_install.py

The script is driven as a subprocess and MUST NOT be imported. Its path comes from the
AGENTSPACE_INSTALL_SH environment variable, defaulting to ../system/install.sh relative to this
file. Every expectation is written from docs/operator.md ("What sync installs", its PATH edit and
its messages, and the coverage table's #0166 row) and from D61 in docs/decisions.md, never from
the script's code.

The seam (docs/operator.md, for box/tests alone, D63): AGENTSPACE_ENVIRONMENT_FILE names the file
the edit works on, and AGENTSPACE_INSTALL_ONLY=environment runs that one step, its checks and its
Installed: line, and exits with its status. Each test builds a throwaway tree: a folder that
holds the environment file alone, a folder for the script's working directory (which holds no
agentspace_ca.pub, so a script that ignored the seam would stop at once, before it touched the
real /etc), a home and a temp folder. The real /etc is never read or written, and a test MUST NOT
print the values of the environment it builds.

Every test asserts the exit code, the messages and the file, so a script that crashes early
cannot pass a test that only looks for an unchanged file.
"""

import os
import shutil
import stat
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.environ.get(
    "AGENTSPACE_INSTALL_SH",
    os.path.join(HERE, os.pardir, "system", "install.sh"),
)

CARGO = "/home/ubuntu/.cargo/bin"
LOCAL = "/home/ubuntu/.local/bin"
FNM = "/home/ubuntu/.local/share/fnm/aliases/default/bin"
TOOLS = [CARGO, LOCAL, FNM]
TOOLS_JOINED = ":".join(TOOLS)

UBUNTU_DEFAULT = ("/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:"
                  "/usr/games:/usr/local/games:/snap/bin")
# N1, architect's to confirm: the added line ends with a line feed, also in a file whose other
# lines end in CR LF (N4), since operator.md says only "added at its end".
DEFAULT_LINE = 'PATH="%s:%s"\n' % (UBUNTU_DEFAULT, TOOLS_JOINED)

INSTALLED_ADDED = "Installed: /etc/environment's PATH holds the ubuntu user's tool folders (added)"
INSTALLED_THERE = ("Installed: /etc/environment's PATH holds the ubuntu user's tool folders "
                   "(already there)")


def refused_message(n):
    return ("install.sh: line %d of /etc/environment sets PATH to a value that is neither plain "
            "nor quoted whole; the file is left as it was, so fix that line by hand and run "
            "agentspace-operator sync again" % n)


# Lines that are not PATH lines and MUST come through byte for byte, a comment that mentions
# PATH, names that only end or start like PATH, a stray byte and trailing blanks included.
OTHER_LINES = (b"# A comment that says PATH=\"/must/not/change\"\n"
               b"LANG=en_US.UTF-8\n"
               b"\n"
               b"XPATH=/x/bin\n"
               b"PATH_EXTRA=/y/bin\n"
               b"export EDITOR=vi   \n"
               b"# caf\xe9 \xff\n")


class InstallBase(unittest.TestCase):

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="install-test-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.etc = os.path.join(self.root, "etc")
        self.cwd = os.path.join(self.root, "cwd")
        self.home = os.path.join(self.root, "home")
        self.tmp = os.path.join(self.root, "tmp")
        for d in (self.etc, self.cwd, self.home, self.tmp):
            os.mkdir(d)
        self.envfile = os.path.join(self.etc, "environment")

    def write_env(self, data, mode=0o644):
        with open(self.envfile, "wb") as fh:
            fh.write(data)
        os.chmod(self.envfile, mode)

    def read_env(self):
        with open(self.envfile, "rb") as fh:
            return fh.read()

    def run_install(self):
        env = {
            "PATH": "/usr/local/bin:/usr/bin:/bin",
            "HOME": self.home,
            "TMPDIR": self.tmp,
            "LC_ALL": "C",
            "AGENTSPACE_ENVIRONMENT_FILE": self.envfile,
            "AGENTSPACE_INSTALL_ONLY": "environment",
        }
        argv = [SCRIPT]
        if not os.access(SCRIPT, os.X_OK):
            argv = ["bash", SCRIPT]
        r = subprocess.run(argv, env=env, cwd=self.cwd, stdin=subprocess.DEVNULL,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
        return r.returncode, r.stdout.decode("utf-8", "replace"), r.stderr.decode("utf-8", "replace")

    def install_ok(self, expect_line):
        """Runs the install, asserts exit 0 and the one Installed line, returns nothing."""
        code, out, err = self.run_install()
        self.assertEqual(err, "", "stderr")
        # The seam's path is read as /etc/environment here, so that one wrong word does not fail
        # every test; test_the_installed_line_names_etc_environment_whatever_the_seam_names
        # holds the exact text.
        out = out.replace(self.envfile, "/etc/environment")
        self.assertEqual(out.splitlines(), [expect_line], "stdout")
        self.assertEqual(code, 0, "exit code")

    def install_refused(self, line_no):
        before = self.read_env()
        code, out, err = self.run_install()
        self.assertEqual(code, 1, "exit code")
        # As in install_ok, the seam's path is read as /etc/environment; the exact text is held
        # by test_a_refusal_names_the_file_as_etc_environment_whatever_the_seam_names.
        err = err.replace(self.envfile, "/etc/environment")
        self.assertEqual(err.splitlines(), [refused_message(line_no)], "stderr")
        self.assertNotIn("Installed:", out)
        self.assertEqual(self.read_env(), before, "the file MUST be left as it was")
        self.assertEqual(os.listdir(self.etc), ["environment"], "no temp file left beside it")

    def assert_only_the_file(self):
        self.assertEqual(os.listdir(self.etc), ["environment"], "no temp file left beside it")


def path_line(value, prefix="", suffix="", eol="\n"):
    return ("%sPATH=%s%s%s" % (prefix, value, suffix, eol)).encode()


class NoPathLine(InstallBase):

    def test_file_without_a_path_line_gets_ubuntus_default_then_the_three_folders(self):
        self.write_env(b"LANG=en_US.UTF-8\n")
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), b"LANG=en_US.UTF-8\n" + DEFAULT_LINE.encode())

    def test_default_line_is_quoted_whole_with_every_folder_in_order(self):
        self.write_env(b"LANG=en_US.UTF-8\n")
        self.install_ok(INSTALLED_ADDED)
        last = self.read_env().decode().splitlines()[-1]
        self.assertEqual(
            last,
            'PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/usr/games:'
            '/usr/local/games:/snap/bin:/home/ubuntu/.cargo/bin:/home/ubuntu/.local/bin:'
            '/home/ubuntu/.local/share/fnm/aliases/default/bin"')

    def test_last_line_without_a_newline_gets_a_line_feed_before_the_added_line(self):
        self.write_env(b"LANG=en_US.UTF-8\nLAST=1")
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), b"LANG=en_US.UTF-8\nLAST=1\n" + DEFAULT_LINE.encode())

    def test_last_line_with_a_newline_gets_no_extra_blank_line(self):
        self.write_env(b"LANG=en_US.UTF-8\n")
        self.install_ok(INSTALLED_ADDED)
        self.assertNotIn(b"\n\n", self.read_env())

    def test_a_missing_file_is_created_holding_the_default_line_alone(self):
        self.assertFalse(os.path.exists(self.envfile))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), DEFAULT_LINE.encode())
        self.assert_only_the_file()

    def test_an_empty_file_gets_the_default_line_alone(self):
        self.write_env(b"")
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), DEFAULT_LINE.encode())

    def test_lines_that_are_not_path_lines_are_left_byte_for_byte_and_in_order(self):
        # A commented PATH, XPATH= and PATH_EXTRA= are not PATH lines, so the file has none.
        self.write_env(OTHER_LINES)
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), OTHER_LINES + DEFAULT_LINE.encode())

    def test_the_changed_file_is_mode_0644_and_nothing_is_left_beside_it(self):
        self.write_env(b"LANG=en_US.UTF-8\n", mode=0o600)
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(stat.S_IMODE(os.stat(self.envfile).st_mode), 0o644)
        self.assert_only_the_file()

    def test_a_second_run_finds_nothing_to_add_and_leaves_the_file_byte_for_byte(self):
        self.write_env(OTHER_LINES)
        self.install_ok(INSTALLED_ADDED)
        first = self.read_env()
        self.install_ok(INSTALLED_THERE)
        self.assertEqual(self.read_env(), first)

    def test_a_second_run_over_a_created_file_says_already_there(self):
        self.install_ok(INSTALLED_ADDED)
        first = self.read_env()
        self.install_ok(INSTALLED_THERE)
        self.assertEqual(self.read_env(), first)


class ExistingPathLine(InstallBase):

    def test_an_existing_path_line_keeps_its_value_and_gains_the_three_folders_in_order(self):
        self.write_env(b"LANG=en_US.UTF-8\n" + path_line('"/usr/local/bin:/usr/bin"') +
                       b"LAST=1\n")
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(
            self.read_env(),
            b"LANG=en_US.UTF-8\n" + path_line('"/usr/local/bin:/usr/bin:%s"' % TOOLS_JOINED) +
            b"LAST=1\n")

    def test_an_existing_path_line_does_not_get_a_default_line_added(self):
        self.write_env(path_line('"/usr/bin"'))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env().count(b"PATH="), 1)

    def test_a_folder_already_present_is_not_added_again(self):
        self.write_env(path_line('"/usr/bin:%s"' % LOCAL))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(),
                         path_line('"/usr/bin:%s:%s:%s"' % (LOCAL, CARGO, FNM)))

    def test_a_missing_folder_goes_after_the_value_and_the_others_keep_their_order(self):
        self.write_env(path_line('"/usr/bin:%s"' % CARGO))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), path_line('"/usr/bin:%s:%s:%s"' % (CARGO, LOCAL, FNM)))

    def test_a_line_holding_every_folder_is_not_changed_and_says_already_there(self):
        data = OTHER_LINES + path_line('"/usr/bin:%s"' % TOOLS_JOINED)
        self.write_env(data)
        self.install_ok(INSTALLED_THERE)
        self.assertEqual(self.read_env(), data)

    def test_the_folders_in_another_order_count_as_present(self):
        data = path_line('"%s:%s:%s:/usr/bin"' % (FNM, LOCAL, CARGO))
        self.write_env(data)
        self.install_ok(INSTALLED_THERE)
        self.assertEqual(self.read_env(), data)

    def test_a_file_the_install_would_not_change_is_not_touched(self):
        data = path_line('"/usr/bin:%s"' % TOOLS_JOINED)
        self.write_env(data, mode=0o640)
        os.utime(self.envfile, (1000000000, 1000000000))
        before = os.stat(self.envfile)
        self.install_ok(INSTALLED_THERE)
        after = os.stat(self.envfile)
        self.assertEqual(self.read_env(), data)
        self.assertEqual((after.st_ino, after.st_mtime_ns, stat.S_IMODE(after.st_mode)),
                         (before.st_ino, before.st_mtime_ns, 0o640))

    def test_a_second_run_over_an_edited_line_changes_nothing(self):
        self.write_env(OTHER_LINES + path_line('"/usr/bin"'))
        self.install_ok(INSTALLED_ADDED)
        first = self.read_env()
        self.install_ok(INSTALLED_THERE)
        self.assertEqual(self.read_env(), first)

    def test_every_other_line_is_left_byte_for_byte_and_in_order(self):
        self.write_env(OTHER_LINES + path_line('"/usr/bin"') + b"TAIL=1\n")
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(
            self.read_env(),
            OTHER_LINES + path_line('"/usr/bin:%s"' % TOOLS_JOINED) + b"TAIL=1\n")

    def test_every_path_line_gains_the_folders(self):
        self.write_env(path_line('"/usr/bin"') + b"MID=1\n" + path_line("/bin"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(
            self.read_env(),
            path_line('"/usr/bin:%s"' % TOOLS_JOINED) + b"MID=1\n" +
            path_line("/bin:%s" % TOOLS_JOINED))

    def test_a_plain_value_gains_the_folders_with_no_quotes_added(self):
        self.write_env(path_line("/usr/bin:/bin"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), path_line("/usr/bin:/bin:%s" % TOOLS_JOINED))

    def test_export_is_kept_on_a_plain_value(self):
        self.write_env(path_line("/usr/bin", prefix="export "))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), path_line("/usr/bin:%s" % TOOLS_JOINED, prefix="export "))

    def test_export_is_kept_on_a_quoted_value(self):
        self.write_env(path_line('"/usr/bin"', prefix="export "))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(),
                         path_line('"/usr/bin:%s"' % TOOLS_JOINED, prefix="export "))

    def test_single_quotes_are_kept_and_the_folders_go_inside_them(self):
        self.write_env(path_line("'/usr/bin'"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), path_line("'/usr/bin:%s'" % TOOLS_JOINED))

    def test_double_quotes_are_kept_and_the_folders_go_inside_them(self):
        self.write_env(path_line('"/usr/bin"'))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), path_line('"/usr/bin:%s"' % TOOLS_JOINED))

    def test_leading_whitespace_before_path_is_kept(self):
        self.write_env(path_line('"/usr/bin"', prefix="  \t"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), path_line('"/usr/bin:%s"' % TOOLS_JOINED, prefix="  \t"))

    def test_leading_whitespace_and_export_are_both_kept(self):
        self.write_env(path_line("/usr/bin", prefix="  export "))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(),
                         path_line("/usr/bin:%s" % TOOLS_JOINED, prefix="  export "))

    def test_a_trailing_comment_after_a_quoted_value_is_kept_with_its_blanks(self):
        self.write_env(path_line('"/usr/bin"', suffix="   # the system's own"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(
            self.read_env(),
            path_line('"/usr/bin:%s"' % TOOLS_JOINED, suffix="   # the system's own"))

    def test_a_trailing_comment_after_a_plain_value_is_kept_with_its_blanks(self):
        self.write_env(path_line("/usr/bin", suffix=" \t# a note"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(),
                         path_line("/usr/bin:%s" % TOOLS_JOINED, suffix=" \t# a note"))

    def test_a_quote_character_inside_the_comment_does_not_refuse_the_line(self):
        # The value is what comes before the first #, so the apostrophe is not in it.
        self.write_env(path_line("/usr/bin", suffix="  # it's mine"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(),
                         path_line("/usr/bin:%s" % TOOLS_JOINED, suffix="  # it's mine"))

    def test_a_line_holding_every_folder_with_a_comment_is_not_changed(self):
        data = path_line('"/usr/bin:%s"' % TOOLS_JOINED, suffix="  # all there")
        self.write_env(data)
        self.install_ok(INSTALLED_THERE)
        self.assertEqual(self.read_env(), data)

    def test_a_cr_line_ending_is_kept_after_a_quoted_value(self):
        self.write_env(b"LANG=en_US.UTF-8\r\n" + path_line('"/usr/bin"', eol="\r\n") +
                       b"LAST=1\r\n")
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(
            self.read_env(),
            b"LANG=en_US.UTF-8\r\n" + path_line('"/usr/bin:%s"' % TOOLS_JOINED, eol="\r\n") +
            b"LAST=1\r\n")

    def test_a_cr_line_ending_is_kept_after_a_plain_value_and_is_not_read_as_part_of_it(self):
        self.write_env(path_line("/usr/bin", eol="\r\n"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), path_line("/usr/bin:%s" % TOOLS_JOINED, eol="\r\n"))

    def test_a_cr_line_ending_is_kept_after_a_comment(self):
        self.write_env(path_line('"/usr/bin"', suffix=" # c", eol="\r\n"))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(),
                         path_line('"/usr/bin:%s"' % TOOLS_JOINED, suffix=" # c", eol="\r\n"))

    def test_a_cr_line_holding_every_folder_is_not_changed(self):
        data = path_line('"/usr/bin:%s"' % TOOLS_JOINED, eol="\r\n")
        self.write_env(data)
        self.install_ok(INSTALLED_THERE)
        self.assertEqual(self.read_env(), data)

    def test_a_cr_file_without_a_path_line_gets_the_default_line(self):
        # The added line is the install's own; the file's other lines are left as they were.
        self.write_env(b"LANG=en_US.UTF-8\r\n")
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(), b"LANG=en_US.UTF-8\r\n" + DEFAULT_LINE.encode())

    def test_a_path_line_that_is_the_last_line_without_a_newline_is_edited_in_place(self):
        # N2, architect's to confirm: the line keeps its ending, which is none.
        self.write_env(b"LANG=en_US.UTF-8\n" + path_line('"/usr/bin"', eol=""))
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(self.read_env(),
                         b"LANG=en_US.UTF-8\n" + path_line('"/usr/bin:%s"' % TOOLS_JOINED, eol=""))

    def test_the_changed_file_is_mode_0644_and_nothing_is_left_beside_it(self):
        self.write_env(path_line('"/usr/bin"'), mode=0o600)
        self.install_ok(INSTALLED_ADDED)
        self.assertEqual(stat.S_IMODE(os.stat(self.envfile).st_mode), 0o644)
        self.assert_only_the_file()


class RefusedValue(InstallBase):
    """A PATH value neither plain nor quoted whole fails the install with the file as it was."""

    def refuse(self, value, prefix="", suffix=""):
        self.write_env(b"LANG=en_US.UTF-8\n" + path_line(value, prefix=prefix, suffix=suffix) +
                       b"LAST=1\n")
        self.install_refused(2)

    def test_a_quote_that_is_never_closed_is_refused(self):
        self.refuse('"/usr/bin')

    def test_a_closing_quote_with_no_opening_one_is_refused(self):
        self.refuse('/usr/bin"')

    def test_quotes_that_do_not_match_are_refused(self):
        self.refuse("\"/usr/bin'")

    def test_a_quote_character_inside_a_plain_value_is_refused(self):
        self.refuse('/usr/b"in:/bin')

    def test_a_second_pair_of_quotes_is_refused(self):
        self.refuse('"/usr/bin":"/bin"')

    def test_a_quote_character_inside_a_quoted_value_is_refused(self):
        self.refuse("\"/usr/b'in\"")

    def test_a_refused_value_with_export_is_refused(self):
        self.refuse('"/usr/bin', prefix="export ")

    def test_a_hash_inside_quotes_cuts_the_value_and_leaves_an_unclosed_quote(self):
        # N3, architect's to confirm: the value is what comes before the first #, so
        # "/a#b" reads as "/a, which is neither plain nor quoted whole.
        self.refuse('"/usr/b#in"')

    def test_the_line_number_counts_from_1_for_a_first_line(self):
        self.write_env(path_line('"/usr/bin') + b"LAST=1\n")
        self.install_refused(1)

    def test_the_line_number_is_the_refused_lines_among_blank_and_comment_lines(self):
        self.write_env(b"# one\n\nLANG=en_US.UTF-8\n" + path_line("'/usr/bin"))
        self.install_refused(4)

    def test_a_file_with_a_good_line_before_the_refused_one_is_left_wholly_as_it_was(self):
        self.write_env(path_line('"/usr/bin"') + b"MID=1\n" + path_line('"/bin') + b"LAST=1\n")
        self.install_refused(3)

    def test_a_refused_file_ending_without_a_newline_is_left_as_it_was(self):
        self.write_env(b"LANG=en_US.UTF-8\n" + path_line('"/usr/bin', eol=""))
        self.install_refused(2)

    def test_a_refused_cr_line_is_refused_with_its_number(self):
        self.write_env(b"A=1\r\n" + path_line('"/usr/bin', eol="\r\n"))
        self.install_refused(2)

    def test_a_refusal_names_the_file_as_etc_environment_whatever_the_seam_names(self):
        self.write_env(path_line('"/usr/bin'))
        code, out, err = self.run_install()
        self.assertEqual(code, 1)
        self.assertIn("of /etc/environment sets PATH", err)
        self.assertNotIn(self.root, err)


class NothingElse(InstallBase):

    def snapshot(self):
        tree = []
        for d, dirs, files in os.walk(self.root):
            dirs.sort()
            for f in sorted(files):
                tree.append(os.path.relpath(os.path.join(d, f), self.root))
        return tree

    def check_only_the_file_changed(self, data):
        if data is not None:
            self.write_env(data)
        self.install_ok(INSTALLED_ADDED)
        # The environment file is the only file anywhere in the throwaway tree, and the working
        # directory, home and temp folders hold nothing.
        self.assertEqual(self.snapshot(), [os.path.join("etc", "environment")])
        for d in (self.cwd, self.home, self.tmp):
            self.assertEqual(os.listdir(d), [], "folder MUST stay empty")

    def test_an_edit_writes_only_the_environment_file(self):
        self.check_only_the_file_changed(path_line('"/usr/bin"'))

    def test_adding_a_line_writes_only_the_environment_file(self):
        self.check_only_the_file_changed(b"LANG=en_US.UTF-8\n")

    def test_creating_the_file_writes_only_the_environment_file(self):
        self.check_only_the_file_changed(None)

    def test_a_run_that_changes_nothing_writes_nothing(self):
        self.write_env(path_line('"/usr/bin:%s"' % TOOLS_JOINED))
        self.install_ok(INSTALLED_THERE)
        self.assertEqual(self.snapshot(), [os.path.join("etc", "environment")])
        for d in (self.cwd, self.home, self.tmp):
            self.assertEqual(os.listdir(d), [], "folder MUST stay empty")

    def test_a_refused_run_writes_nothing_but_leaves_the_file(self):
        self.write_env(path_line('"/usr/bin'))
        code, out, err = self.run_install()
        self.assertEqual(code, 1)
        # The refusal itself, so a run that stops for any other reason does not pass.
        self.assertEqual(err.splitlines(), [refused_message(1)], "stderr")
        self.assertEqual(self.snapshot(), [os.path.join("etc", "environment")])
        for d in (self.cwd, self.home, self.tmp):
            self.assertEqual(os.listdir(d), [], "folder MUST stay empty")

    def test_the_installed_line_names_etc_environment_whatever_the_seam_names(self):
        # N5, architect's to confirm: the seam changes where the file is, never a message's text.
        self.write_env(b"LANG=en_US.UTF-8\n")
        code, out, err = self.run_install()
        self.assertEqual(out.splitlines(), [INSTALLED_ADDED])
        self.assertEqual(err, "")
        self.assertEqual(code, 0)

    def test_the_run_prints_one_installed_line_and_no_other_step_s_line(self):
        self.write_env(b"LANG=en_US.UTF-8\n")
        code, out, err = self.run_install()
        self.assertEqual(len(out.splitlines()), 1)
        self.assertTrue(out.startswith("Installed: "))


CA_MISSING = ("install.sh: agentspace_ca.pub is missing; run agentspace-operator sync, which "
              "sends it")


class WithNoSeam(unittest.TestCase):
    """P5: with neither seam variable set, install.sh takes a host's full install path.

    A copy of the script runs from a throwaway folder that holds it alone, so its first check,
    operator.md's "Fails at once if agentspace_ca.pub is missing or empty", stops it before any
    step that writes /etc.
    """

    def test_with_neither_seam_variable_the_install_stops_at_the_ca_check(self):
        # As root, a script whose CA check were broken would go on to write /etc; as any other
        # user it cannot, so the real /etc stays out of reach however the script behaves.
        if os.geteuid() == 0:
            self.skipTest("MUST NOT run as root: a broken CA check would reach the real /etc")
        root = tempfile.mkdtemp(prefix="install-noseam-")
        self.addCleanup(shutil.rmtree, root, ignore_errors=True)
        folder = os.path.join(root, "system")
        home = os.path.join(root, "home")
        tmp = os.path.join(root, "tmp")
        for d in (folder, home, tmp):
            os.mkdir(d)
        copy = os.path.join(folder, "install.sh")
        shutil.copyfile(SCRIPT, copy)
        # The guard that keeps the real /etc out of reach: the script's folder has no CA key.
        self.assertFalse(os.path.exists(os.path.join(folder, "agentspace_ca.pub")),
                         "the folder MUST hold no CA key")
        self.assertEqual(os.listdir(folder), ["install.sh"])
        env = {"PATH": "/usr/local/bin:/usr/bin:/bin", "HOME": home, "TMPDIR": tmp, "LC_ALL": "C"}
        before = sorted(os.path.relpath(os.path.join(d, f), root)
                        for d, _, files in os.walk(root) for f in files)
        r = subprocess.run(["bash", copy], env=env, cwd=root, stdin=subprocess.DEVNULL,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
        after = sorted(os.path.relpath(os.path.join(d, f), root)
                       for d, _, files in os.walk(root) for f in files)
        self.assertEqual(r.stderr.decode("utf-8", "replace").splitlines(), [CA_MISSING], "stderr")
        self.assertEqual(r.stdout, b"", "stdout")
        self.assertEqual(r.returncode, 1, "exit code")
        self.assertEqual(after, before, "the run MUST write nothing")


if __name__ == "__main__":
    unittest.main()

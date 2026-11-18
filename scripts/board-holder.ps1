#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The board holder keeper, one pass (D30; story 0.02.T1.2, card #0058). Laptop only: it reads the
# lease state file and starts or stops one local ssh.exe, and never talks to the box or AWS itself.
#
#   pwsh -NoProfile -File scripts\board-holder.ps1 [-SshExe <path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]
#
# One pass looks at the leases and at the holder, then makes them agree, with one action:
# - No trusted holder running, and a lease ready: it starts one through the ready lease that expires
#   last (ties go to the lowest lease ID), and records it. Action started.
# - A trusted holder running whose lease is still ready, under the same alias: it keeps it. Action
#   kept.
# - A trusted holder running whose lease is no longer ready (released, expired or gone): it stops
#   the holder and the process tree the holder started, and removes the record. Action stopped. A
#   pass that stops a holder MUST NOT also start one: the next pass does, which also gives the box
#   time to drop the dead session's forwards (D12).
# - No trusted holder and no lease ready: nothing to do. Action idle.
# - A start or stop that goes wrong: action failed, with the reason. The pass still ran.
# - Another pass holds board-holder.lock: action skipped, with reason=locked. Nothing is looked at.
#
# Exit codes (architect's ruling, 2026-11-17):
# - 0: the pass ran, whatever its action, failed included, or it was skipped because another pass
#   held the lock. A skipped pass logs "skipped lease=- alias=- pid=- reason=locked", never idle.
# - 1: the lease state could not be read, or the pass met an error it does not handle. A failed
#   line is logged, and the holder is left as it is.
# - 2: a usage error: an unknown, repeated or empty argument, a -Now that is not a time in the
#   form YYYY-MM-DDTHH:MM:SSZ, or an -SshExe that is not a full path or holds a quote (or, for a
#   batch, a percent sign). As for the lease CLI (docs/lease-api.md). The error goes to stderr,
#   and nothing is logged.
#
# Files, all in the state folder: $env:AGENTSPACE_STATE_DIR, or %USERPROFILE%\.agentspace when that
# is unset, as for lease-state.ps1 (D21). Tests MUST set AGENTSPACE_STATE_DIR to a throwaway folder.
# - board-holder.json, the record, absent when there is no holder:
#   {"pid":<int>,"process_started_at":"<UTC, YYYY-MM-DDTHH:MM:SS.fffZ>","lease_id":"<uuid>","alias":"<alias>"}
#   It is written to a temporary file in the same folder and moved over the old one, so a reader
#   sees the old record or the new one, never part of one.
# - board-holder.log, one line per pass:
#   <YYYY-MM-DDTHH:MM:SSZ> <action> lease=<id> alias=<alias> pid=<pid>[ reason=<text>]
#   The time is the pass's own: -Now when given, else the current second. lease, alias and pid name
#   the holder the action concerns: the one started, kept or stopped, or for failed the one being
#   started or stopped. A field with nothing to name is written '-'. An idle line names no holder,
#   since there is none, so it is always "idle lease=- alias=- pid=-". A failed line for unreadable
#   lease state is "failed lease=- alias=- pid=- reason=...", and a skipped line is always
#   "skipped lease=- alias=- pid=- reason=locked". reason is written for failed and skipped only, is
#   the last field and runs to the end of the line; it never holds a control character.
# - board-holder-ssh.log, the holder's stdout and stderr, appended. It holds raw bytes in the
#   console's code page, and nothing parses it. Its growth is not bounded yet.
# - board-holder.lock, held with FileShare.None for the whole pass, so that two passes never both
#   start a holder (one holder per host, D30). A pass that cannot take it in 10 seconds logs
#   skipped with reason=locked, and exits 0.
#
# Lease state is read with Read-LeaseState, which takes no lock and never blocks a writer. A lease
# is ready when its stored state is ready and its expires_at is after -Now (default: the current
# second), the same rule ConvertTo-LeaseOutput uses for expired.
#
# The holder command is <SshExe> -N -o ExitOnForwardFailure=yes <alias>, with stdin from NUL and
# stdout and stderr appended to board-holder-ssh.log. SshExe MUST be a full path; it defaults to
# C:\Windows\System32\OpenSSH\ssh.exe. It MAY be a .cmd or .bat file, which tests use as a stub: the
# holder is then "<System32>\cmd.exe" /d /c ""<SshExe>" -N -o ExitOnForwardFailure=yes <alias>",
# since Windows runs a batch only through cmd. /d skips cmd's AutoRun.
#
# How a holder is started, so that it outlives this pass (0.02.T1.2's spike and the trials after
# it):
# - A process this pwsh starts itself (Start-Process, Process.Start) is in this pwsh's job object,
#   and dies when the job of an agent's tool call or of a scheduled task is closed. A process made
#   by CIM, Win32_Process.Create, is made outside it and survives.
# - Win32_Process.Create cannot set a process's standard handles, and a cmd wrapper's own
#   redirection (>>) opens the log denying others write access, so a second holder could not open
#   the log while any process of an earlier one still held it, and died at once. So
#   Win32_Process.Create runs a launcher instead: this script again, as
#     "<PSHOME>\pwsh.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File <this script> -Launch <request>
#   which calls CreateProcess for the holder with stdin from NUL and stdout and stderr on one handle
#   to the log, opened for appending only and shared for reading, writing and deleting. Only those
#   two handles are inherited, it has no window, and its working folder is %USERPROFILE%, so it
#   never holds a worktree open. The launcher writes the holder's PID, start time and command line
#   to a result file, and exits; the holder runs on with no parent.
# - -Launch is internal, not part of the interface. Code outside this script MUST NOT call it. The
#   request and the result are JSON files in the state folder, board-holder-launch.<guid>.json and
#   .result.json, with a claim file, board-holder-launch.<guid>.claim, all removed by the pass.
# - The request is claimed by creating its claim file with FileMode.CreateNew (CREATE_NEW), which
#   exactly one process can do, so that at most one launcher starts a holder for it, and never one
#   for a pass that has given up. A rename is not used: a trial of four launchers renaming one
#   request to one name had two of them succeed and start two holders, because the loser's rename
#   of a file already given that name succeeds too.
#   - A launcher MUST claim the request before it starts anything. A launcher that cannot claim
#     it starts nothing and writes nothing. So a second launcher, from a Win32_Process.Create retry
#     after a call that threw but had in fact made one, starts nothing.
#   - A pass that gives up waiting, or whose Win32_Process.Create failed, withdraws the request by
#     claiming it itself. If its claim wins, no launcher can claim the request, and none starts a
#     holder. If a launcher claimed it first, the pass waits on for that launcher's answer, up to
#     60 more seconds, and records the holder if one comes. If none comes, it logs failed with a
#     reason that says a holder may be running unrecorded. It never retries in a loop (D14).
#   - The pass removes the request before its claim file, so a launcher that claims the request
#     after that finds no request, and starts nothing.
#   - The request also carries a deadline: a launcher that claims it after the deadline starts
#     nothing, and answers with that error.
# - A holder made this way gets the user's logon environment, not this pwsh's variables.
# - If the holder exits at once, or its record cannot be written, the pass logs failed; in the
#   second case it stops the holder first, since an unrecorded holder could never be stopped, and
#   the reason says whether that stop succeeded.
#
# Trust: this pass MUST NOT stop any process it did not start, even one whose command line names an
# alias. A recorded PID is trusted only when all of these hold:
# - A process with that PID is running and can be queried.
# - Its creation time (GetProcessTimes), as UTC truncated to the millisecond, equals
#   process_started_at as a string. The PID and the start time together are the identity, and are
#   what defeats PID reuse.
# - Its command line is the holder command for the recorded alias, read structurally
#   (Agentspace.BoardHolder.HolderCommand.Matches), not compared byte for byte:
#   - The line is split into tokens by CommandLineToArgvW's rules, so quoting and spacing do not
#     matter.
#   - The program is -SshExe, both full paths compared as Windows compares them: separators and
#     dots normalized, case ignored. For a .cmd or .bat, the program MAY instead be
#     <System32>\cmd.exe with /c, or /d /c, whose command (cmd's own quote stripping applied) has
#     -SshExe as its program.
#   - The arguments after the program MUST be exactly -N -o ExitOnForwardFailure=yes <alias>, the
#     alias compared exactly, case included.
#   So a pass MUST be run with the same -SshExe as the pass that started the holder; with another,
#   the old holder is untrusted and left alone. The matcher checks itself (HolderCommand.Check) at
#   the start of every pass, and a pass whose matcher fails stops with an error, trusting nothing.
# A recorded PID that is not trusted is treated as no holder: its record is removed, the process
# is left alone, and a holder is started afresh if a lease is ready.
#
# How a trusted holder is stopped (Agentspace.BoardHolder.Native.StopTree):
# - The holder is opened by PID, and its creation time and command line are checked again on that
#   same handle, by the same matcher. While the handle is open Windows cannot reuse the PID, so the
#   process checked is the process stopped.
# - Its descendants are found by walking parent PIDs, and each one is checked from its own handle:
#   its parent PID is its parent's and it was created no earlier than its parent. A process whose
#   parent died before the walk cannot be traced by any means and is left alone; nothing is ever
#   matched by command line alone, and nothing is stopped by taskkill or by a sweep of command lines.
# - Every handle is held until the end, so no PID in the tree is reused while it is being stopped.
#   The tree is stopped leaves first, the holder last, then walked again for any child made
#   meanwhile.
# - StopTree says why it stopped nothing: the process is not running, it could not be opened with
#   the right to stop it (a holder started elevated, met by a pass that is not, say), or its
#   identity differs. The pass then probes the PID again. The record MUST be kept, and failed
#   logged, while the recorded holder is still running; stopped is logged, and the record removed,
#   only when the recorded holder is gone or was stopped.
#
# A holder that fails is not restarted in a loop: each pass starts at most one, and backoff
# between passes belongs to -Watch, which 0.02.T2.2 adds (D14, D30).
#
# The arguments are parsed by this script, not by PowerShell's binder, so that every usage error
# exits 2: names are matched exactly, case ignored, and each takes one value.

$ErrorActionPreference = 'Stop'

# Writes a usage error to stderr and exits 2. Nothing is logged.
function Exit-Usage([string]$Message) {
  [Console]::Error.WriteLine("board-holder.ps1: $Message")
  [Console]::Error.WriteLine('Usage: board-holder.ps1 [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]')
  exit 2
}

$SshExe = 'C:\Windows\System32\OpenSSH\ssh.exe'
$Now = $null
$Launch = $null
$given = @{}
for ($i = 0; $i -lt $args.Count; $i++) {
  $raw = [string]$args[$i]
  $name = switch -Exact (($raw -replace ':\z', '').ToLowerInvariant()) {
    '-sshexe' { 'SshExe' }
    '-now' { 'Now' }
    '-launch' { 'Launch' }
    default { $null }
  }
  if (-not $name) { Exit-Usage "Unknown argument '$raw'." }
  if ($given.ContainsKey($name)) { Exit-Usage "-$name is given twice." }
  if ($i + 1 -ge $args.Count -or [string]::IsNullOrWhiteSpace([string]$args[$i + 1]) -or ([string]$args[$i + 1]).StartsWith('-')) {
    Exit-Usage "-$name needs a value."
  }
  $given[$name] = [string]$args[++$i]
}
if ($given.ContainsKey('SshExe')) { $SshExe = $given['SshExe'] }
if ($given.ContainsKey('Now')) { $Now = $given['Now'] }
if ($given.ContainsKey('Launch')) { $Launch = $given['Launch'] }
. (Join-Path $PSScriptRoot 'lease-state.ps1')

if (-not ('Agentspace.BoardHolder.Native' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Globalization;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace Agentspace.BoardHolder
{
    // A running process as read from its own handle. StartedAt is its creation time in UTC,
    // truncated to the millisecond, as yyyy-MM-ddTHH:mm:ss.fffZ. CommandLine is null when it could
    // not be read.
    public sealed class ProcessIdentity
    {
        public int Pid;
        public long CreationFileTime;
        public string StartedAt;
        public string CommandLine;
    }

    // What StopTree did. Matched is false when nothing was stopped, and Outcome then says why:
    // NotRunning (no process has the PID, or it has exited), CannotOpen (it could not be opened with
    // the right to stop it; OpenError holds the Windows error), or IdentityDiffers (its creation time
    // or command line is not the record's). Outcome is Matched otherwise. RootStopped is true when
    // the root is no longer running.
    public sealed class StopResult
    {
        public bool Matched;
        public string Outcome;
        public int OpenError;
        public bool RootStopped;
        public int[] Stopped;
        public int[] Survivors;
    }

    // One token of a command line: its value as the program receives it, and where its raw text
    // starts and ends in the line.
    public sealed class Token
    {
        public string Value;
        public int Start;
        public int End;
    }

    // The holder command, <SshExe> -N -o ExitOnForwardFailure=yes <alias>: how the keeper builds
    // it, and how a process's command line is recognized as one. Both the pass's trust check and
    // StopTree's re-check use Matches, so there is one rule.
    public static class HolderCommand
    {
        static readonly string[] Options = { "-N", "-o", "ExitOnForwardFailure=yes" };
        const string CmdSpecial = "&<>()@^|";

        public static string CmdPath { get { return Path.Combine(Environment.SystemDirectory, "cmd.exe"); } }

        public static bool IsBatch(string exe)
        {
            string ext = Path.GetExtension(exe ?? string.Empty);
            return string.Equals(ext, ".cmd", StringComparison.OrdinalIgnoreCase) || string.Equals(ext, ".bat", StringComparison.OrdinalIgnoreCase);
        }

        // What CreateProcess runs for the holder: SshExe itself, or cmd.exe for a batch.
        public static string Application(string sshExe)
        {
            return IsBatch(sshExe) ? CmdPath : sshExe;
        }

        // The command line the keeper gives CreateProcess. A batch runs through cmd, in cmd's own
        // process: "<cmd>" /d /c ""<SshExe>" -N -o ExitOnForwardFailure=yes <alias>". /d skips
        // cmd's AutoRun.
        public static string Build(string sshExe, string alias)
        {
            string holder = "\"" + sshExe + "\" " + string.Join(" ", Options) + " " + alias;
            return IsBatch(sshExe) ? "\"" + CmdPath + "\" /d /c \"" + holder + "\"" : holder;
        }

        static bool IsBlank(char c) { return c == ' ' || c == '\t'; }

        // Splits a command line as CommandLineToArgvW does. The first token, the program, runs to
        // the next quote when it starts with one, and to the next blank otherwise, with no escapes.
        // Every later token follows the argument rules: blanks outside quotes end it; 2n
        // backslashes before a quote give n backslashes and the quote opens or closes quoting;
        // 2n+1 give n backslashes and a literal quote; backslashes before anything else are
        // literal; and in a run of quotes every third one is literal.
        public static List<Token> Split(string line)
        {
            var tokens = new List<Token>();
            if (line == null) return tokens;
            int n = line.Length, i = 0;
            if (n > 0 && line[0] == '"')
            {
                int close = line.IndexOf('"', 1);
                if (close < 0) { tokens.Add(new Token { Value = line.Substring(1), Start = 0, End = n }); i = n; }
                else { tokens.Add(new Token { Value = line.Substring(1, close - 1), Start = 0, End = close + 1 }); i = close + 1; }
            }
            else
            {
                while (i < n && !IsBlank(line[i])) i++;
                tokens.Add(new Token { Value = line.Substring(0, i), Start = 0, End = i });
            }
            while (i < n && IsBlank(line[i])) i++;
            while (i < n)
            {
                int start = i, quotes = 0, slashes = 0;
                var value = new StringBuilder();
                while (i < n)
                {
                    char c = line[i];
                    if (IsBlank(c) && quotes == 0) break;
                    if (c == '\\') { value.Append(c); slashes++; i++; continue; }
                    if (c != '"') { value.Append(c); slashes = 0; i++; continue; }
                    if (slashes % 2 == 0) { value.Length -= slashes / 2; quotes++; }
                    else { value.Length -= slashes / 2 + 1; value.Append('"'); }
                    slashes = 0;
                    i++;
                    while (i < n && line[i] == '"')
                    {
                        if (++quotes == 3) { value.Append('"'); quotes = 0; }
                        i++;
                    }
                    if (quotes == 2) quotes = 0;
                }
                tokens.Add(new Token { Value = value.ToString(), Start = start, End = i });
                while (i < n && IsBlank(line[i])) i++;
            }
            return tokens;
        }

        // The command cmd runs for /c: the text after the switch, less its leading blanks. When it
        // starts with a quote, cmd removes that quote and the last quote in the text, unless the
        // text holds exactly two quotes, with a blank and none of &<>()@^| between them; cmd then
        // keeps both when the quoted text names a program, and this always keeps them. So a line
        // cmd would read the other way is never matched, and is left alone.
        public static string CmdCommand(string rest)
        {
            rest = rest.TrimStart(' ', '\t');
            if (rest.Length == 0 || rest[0] != '"') return rest;
            int count = 0;
            foreach (char c in rest) if (c == '"') count++;
            if (count == 2)
            {
                string inner = rest.Substring(1, rest.IndexOf('"', 1) - 1);
                if (inner.IndexOfAny(new[] { ' ', '\t' }) >= 0 && inner.IndexOfAny(CmdSpecial.ToCharArray()) < 0) return rest;
            }
            int last = rest.LastIndexOf('"');
            if (last == 0) return rest.Substring(1);
            return rest.Substring(1, last - 1) + rest.Substring(last + 1);
        }

        // Two full paths that name the same file as Windows compares them: separators and dots
        // normalized, case ignored. A path that is not fully qualified names nothing.
        public static bool SamePath(string a, string b)
        {
            if (string.IsNullOrEmpty(a) || string.IsNullOrEmpty(b)) return false;
            if (!Path.IsPathFullyQualified(a) || !Path.IsPathFullyQualified(b)) return false;
            try { return string.Equals(Path.GetFullPath(a), Path.GetFullPath(b), StringComparison.OrdinalIgnoreCase); }
            catch (Exception) { return false; }
        }

        static bool IsHolder(List<Token> t, string sshExe, string alias)
        {
            if (t.Count != Options.Length + 2 || !SamePath(t[0].Value, sshExe)) return false;
            for (int k = 0; k < Options.Length; k++)
                if (!string.Equals(t[k + 1].Value, Options[k], StringComparison.Ordinal)) return false;
            return string.Equals(t[Options.Length + 1].Value, alias, StringComparison.Ordinal);
        }

        static bool IsSwitch(string line, Token t, string name)
        {
            return string.Equals(line.Substring(t.Start, t.End - t.Start), name, StringComparison.OrdinalIgnoreCase);
        }

        // True when commandLine is the holder command for this SshExe and alias, read structurally:
        // the program is SshExe, compared as a Windows path, or for a .cmd or .bat it is
        // <System32>\cmd.exe [/d] /c running SshExe; and the arguments after the program are exactly
        // -N -o ExitOnForwardFailure=yes <alias>, the alias compared exactly.
        public static bool Matches(string commandLine, string sshExe, string alias)
        {
            if (string.IsNullOrEmpty(commandLine) || string.IsNullOrEmpty(sshExe) || string.IsNullOrEmpty(alias)) return false;
            List<Token> t = Split(commandLine);
            if (IsHolder(t, sshExe, alias)) return true;
            if (!IsBatch(sshExe) || !SamePath(t[0].Value, CmdPath)) return false;
            int k = 1;
            if (k < t.Count && IsSwitch(commandLine, t[k], "/d")) k++;
            if (k >= t.Count || !IsSwitch(commandLine, t[k], "/c")) return false;
            return IsHolder(Split(CmdCommand(commandLine.Substring(t[k].End))), sshExe, alias);
        }

        // The matcher's own checks, run at the start of every pass. Returns what failed, or an
        // empty array.
        public static string[] Check()
        {
            var failed = new List<string>();
            string ssh = @"C:\Windows\System32\OpenSSH\ssh.exe";
            string spaced = @"C:\Program Files\OpenSSH\ssh.exe";
            string stub = @"C:\agentspace-check\ssh-stub.cmd";
            string spacedStub = @"C:\agentspace check\ssh-stub.cmd";
            string alias = "agentspace-check-0000000a";
            string args = " -N -o ExitOnForwardFailure=yes ";
            string cmd = CmdPath;

            Action<string, string[]> split = (line, expected) =>
            {
                List<Token> t = Split(line);
                var got = new List<string>();
                foreach (Token x in t) got.Add(x.Value);
                bool same = got.Count == expected.Length;
                for (int k = 0; same && k < got.Count; k++) same = string.Equals(got[k], expected[k], StringComparison.Ordinal);
                if (!same) failed.Add("Split(" + line + ") gave [" + string.Join("|", got) + "]");
            };
            split(@"p ""a b"" c\\d e\""f g\\""h i""", new[] { "p", "a b", @"c\\d", "e\"f", @"g\h i" });
            split(@"p """"""a""""""", new[] { "p", "\"a\"" });
            split(@"""C:\x\""y  z ", new[] { @"C:\x\", "y", "z" });

            Action<string, string, string, bool, string> match = (name, line, exe, expected, a) =>
            {
                if (Matches(line, exe, a ?? alias) != expected) failed.Add(name + ": " + line);
            };
            // The keeper's own launcher form.
            match("Own form, exe", Build(ssh, alias), ssh, true, null);
            match("Own form, batch", Build(stub, alias), stub, true, null);
            match("Own form, batch with spaces", Build(spacedStub, alias), spacedStub, true, null);
            // Start-Process of an exe, and the same line spelled differently.
            match("Start-Process, exe", "\"" + ssh + "\"" + args + alias, ssh, true, null);
            match("Exe, unquoted, other case, extra blanks", ssh.ToLowerInvariant() + "  -N\t-o  ExitOnForwardFailure=yes   " + alias + " ", ssh, true, null);
            match("Exe, forward slashes", ssh.Replace('\\', '/') + args + alias, ssh, true, null);
            // Start-Process of a .cmd.
            match("Start-Process, batch", cmd + " /c \"\"" + stub + "\"" + args + alias + "\"", stub, true, null);
            match("Batch, cmd in capitals, /D /C", cmd.ToUpperInvariant() + " /D /C \"\"" + stub + "\"" + args + alias + "\"", stub, true, null);
            match("Batch, two quotes kept", cmd + " /c \"" + spacedStub + "\"" + args + alias, spacedStub, true, null);
            // An exe path with spaces.
            match("Exe with spaces, quoted", "\"" + spaced + "\"" + args + alias, spaced, true, null);
            match("Exe with spaces, unquoted", spaced + args + alias, spaced, false, null);
            match("Batch with spaces, Start-Process", cmd + " /c \"\"" + spacedStub + "\"" + args + alias + "\"", spacedStub, true, null);
            // Near misses.
            match("Other alias", "\"" + ssh + "\"" + args + "agentspace-check-0000000b", ssh, false, null);
            match("Longer alias", "\"" + ssh + "\"" + args + alias + "b", ssh, false, null);
            match("Alias in other case", "\"" + ssh + "\"" + args + alias.ToUpperInvariant(), ssh, false, null);
            match("Quoted alias with more", "\"" + ssh + "\"" + args + "\"" + alias + " x\"", ssh, false, null);
            match("Extra argument after", "\"" + ssh + "\"" + args + alias + " -v", ssh, false, null);
            match("Extra argument before", "\"" + ssh + "\" -v" + args + alias, ssh, false, null);
            match("Option in other case", "\"" + ssh + "\" -n -o ExitOnForwardFailure=yes " + alias, ssh, false, null);
            match("Option missing", "\"" + ssh + "\" -N -o " + alias, ssh, false, null);
            match("Other program, same arguments", @"""C:\Windows\System32\notepad.exe""" + args + alias, ssh, false, null);
            match("Exe under cmd", cmd + " /c \"\"" + ssh + "\"" + args + alias + "\"", ssh, false, null);
            match("Batch, another cmd.exe", "\"C:\\agentspace-check\\cmd.exe\" /c \"\"" + stub + "\"" + args + alias + "\"", stub, false, null);
            match("Batch, /k", cmd + " /k \"\"" + stub + "\"" + args + alias + "\"", stub, false, null);
            match("Batch, /s /c", cmd + " /s /c \"\"" + stub + "\"" + args + alias + "\"", stub, false, null);
            match("Batch, other alias", cmd + " /c \"\"" + stub + "\"" + args + "agentspace-check-0000000b\"", stub, false, null);
            match("Batch, extra argument inside", cmd + " /c \"\"" + stub + "\"" + args + alias + " x\"", stub, false, null);
            match("Batch, command after", cmd + " /c \"\"" + stub + "\"" + args + alias + "\" & calc", stub, false, null);
            match("Batch run directly", "\"" + stub + "\"" + args + alias, stub, true, null);
            match("Empty", "", ssh, false, null);
            match("Relative SshExe", "ssh.exe" + args + alias, "ssh.exe", false, null);
            return failed.ToArray();
        }
    }

    public static class Native
    {
        const uint PROCESS_TERMINATE = 0x0001;
        const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;
        const uint SYNCHRONIZE = 0x00100000;
        const uint WAIT_TIMEOUT = 0x102;
        const uint TH32CS_SNAPPROCESS = 0x2;
        const int ProcessBasicInformation = 0;
        const int ProcessCommandLineInformation = 60;
        const uint GENERIC_READ = 0x80000000;
        const uint FILE_APPEND_DATA = 0x0004;
        const uint FILE_SHARE_READ = 0x1, FILE_SHARE_WRITE = 0x2, FILE_SHARE_DELETE = 0x4;
        const uint OPEN_EXISTING = 3, OPEN_ALWAYS = 4;
        const uint FILE_ATTRIBUTE_NORMAL = 0x80;
        const int STARTF_USESTDHANDLES = 0x100;
        const uint CREATE_NEW_PROCESS_GROUP = 0x00000200;
        const uint EXTENDED_STARTUPINFO_PRESENT = 0x00080000;
        const uint CREATE_BREAKAWAY_FROM_JOB = 0x01000000;
        const uint CREATE_NO_WINDOW = 0x08000000;
        const int ERROR_ACCESS_DENIED = 5;
        const int ERROR_INVALID_PARAMETER = 87;
        static readonly IntPtr PROC_THREAD_ATTRIBUTE_HANDLE_LIST = new IntPtr(0x00020002);
        static readonly IntPtr INVALID_HANDLE_VALUE = new IntPtr(-1);

        [StructLayout(LayoutKind.Sequential)]
        struct SECURITY_ATTRIBUTES
        {
            public int nLength;
            public IntPtr lpSecurityDescriptor;
            public int bInheritHandle;
        }

        [StructLayout(LayoutKind.Sequential)]
        struct STARTUPINFO
        {
            public int cb;
            public IntPtr lpReserved;
            public IntPtr lpDesktop;
            public IntPtr lpTitle;
            public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
            public short wShowWindow, cbReserved2;
            public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
        }

        [StructLayout(LayoutKind.Sequential)]
        struct STARTUPINFOEX
        {
            public STARTUPINFO StartupInfo;
            public IntPtr lpAttributeList;
        }

        [StructLayout(LayoutKind.Sequential)]
        struct PROCESS_INFORMATION
        {
            public IntPtr hProcess;
            public IntPtr hThread;
            public int dwProcessId;
            public int dwThreadId;
        }

        [StructLayout(LayoutKind.Sequential)]
        struct PROCESS_BASIC_INFORMATION
        {
            public IntPtr ExitStatus;
            public IntPtr PebBaseAddress;
            public IntPtr AffinityMask;
            public IntPtr BasePriority;
            public IntPtr UniqueProcessId;
            public IntPtr InheritedFromUniqueProcessId;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        struct PROCESSENTRY32W
        {
            public int dwSize;
            public int cntUsage;
            public int th32ProcessID;
            public IntPtr th32DefaultHeapID;
            public int th32ModuleID;
            public int cntThreads;
            public int th32ParentProcessID;
            public int pcPriClassBase;
            public int dwFlags;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szExeFile;
        }

        [DllImport("kernel32.dll", SetLastError = true)]
        static extern SafeProcessHandle OpenProcess(uint access, bool inherit, int pid);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool GetProcessTimes(SafeProcessHandle h, out long creation, out long exit, out long kernel, out long user);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern uint WaitForSingleObject(SafeProcessHandle h, uint milliseconds);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool TerminateProcess(SafeProcessHandle h, uint exitCode);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern IntPtr CreateToolhelp32Snapshot(uint flags, int pid);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        static extern bool Process32FirstW(IntPtr snapshot, ref PROCESSENTRY32W entry);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        static extern bool Process32NextW(IntPtr snapshot, ref PROCESSENTRY32W entry);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool CloseHandle(IntPtr h);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        static extern SafeFileHandle CreateFileW(string name, uint access, uint share, ref SECURITY_ATTRIBUTES security, uint disposition, uint flags, IntPtr template);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool InitializeProcThreadAttributeList(IntPtr list, int count, int flags, ref IntPtr size);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, IntPtr attribute, IntPtr value, IntPtr size, IntPtr previous, IntPtr returnSize);
        [DllImport("kernel32.dll")]
        static extern void DeleteProcThreadAttributeList(IntPtr list);
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        static extern bool CreateProcessW(string application, StringBuilder commandLine, IntPtr processAttributes, IntPtr threadAttributes,
            bool inheritHandles, uint flags, IntPtr environment, string currentDirectory, ref STARTUPINFOEX startupInfo, out PROCESS_INFORMATION processInformation);
        [DllImport("ntdll.dll")]
        static extern int NtQueryInformationProcess(SafeProcessHandle h, int infoClass, IntPtr info, int length, out int returned);
        [DllImport("ntdll.dll")]
        static extern int NtQueryInformationProcess(SafeProcessHandle h, int infoClass, ref PROCESS_BASIC_INFORMATION info, int length, out int returned);

        sealed class Member
        {
            public int Pid;
            public long Creation;
            public SafeProcessHandle Handle;
        }

        public static string FormatStartedAt(long fileTime)
        {
            return DateTime.FromFileTimeUtc(fileTime).ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'", CultureInfo.InvariantCulture);
        }

        static SafeProcessHandle Open(int pid, uint access)
        {
            int error;
            return Open(pid, access, out error);
        }

        static SafeProcessHandle Open(int pid, uint access, out int error)
        {
            error = 0;
            if (pid <= 0) { error = ERROR_INVALID_PARAMETER; return null; }
            SafeProcessHandle h = OpenProcess(access, false, pid);
            if (h.IsInvalid) { error = Marshal.GetLastWin32Error(); h.Dispose(); return null; }
            return h;
        }

        static bool IsRunning(SafeProcessHandle h)
        {
            return WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        }

        static bool TryCreation(SafeProcessHandle h, out long creation)
        {
            long exit, kernel, user;
            return GetProcessTimes(h, out creation, out exit, out kernel, out user);
        }

        static int ReadParentPid(SafeProcessHandle h)
        {
            var pbi = new PROCESS_BASIC_INFORMATION();
            int returned;
            int status = NtQueryInformationProcess(h, ProcessBasicInformation, ref pbi, Marshal.SizeOf(typeof(PROCESS_BASIC_INFORMATION)), out returned);
            if (status < 0) return -1;
            return (int)pbi.InheritedFromUniqueProcessId.ToInt64();
        }

        static string ReadCommandLine(SafeProcessHandle h)
        {
            int needed;
            NtQueryInformationProcess(h, ProcessCommandLineInformation, IntPtr.Zero, 0, out needed);
            if (needed <= 0) return null;
            IntPtr buffer = Marshal.AllocHGlobal(needed);
            try
            {
                int status = NtQueryInformationProcess(h, ProcessCommandLineInformation, buffer, needed, out needed);
                if (status < 0) return null;
                // A UNICODE_STRING: a USHORT length in bytes, a USHORT maximum, then the pointer at
                // the next pointer-aligned offset.
                int bytes = (ushort)Marshal.ReadInt16(buffer, 0);
                IntPtr text = Marshal.ReadIntPtr(buffer, IntPtr.Size);
                if (bytes == 0 || text == IntPtr.Zero) return string.Empty;
                return Marshal.PtrToStringUni(text, bytes / 2);
            }
            finally { Marshal.FreeHGlobal(buffer); }
        }

        static ProcessIdentity Identify(SafeProcessHandle h, int pid)
        {
            long creation;
            if (!TryCreation(h, out creation)) return null;
            return new ProcessIdentity
            {
                Pid = pid,
                CreationFileTime = creation,
                StartedAt = FormatStartedAt(creation),
                CommandLine = ReadCommandLine(h)
            };
        }

        // The running process with this PID, or null when there is none or it cannot be queried.
        public static ProcessIdentity Probe(int pid)
        {
            using (SafeProcessHandle h = Open(pid, PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE))
            {
                if (h == null || !IsRunning(h)) return null;
                return Identify(h, pid);
            }
        }

        static List<KeyValuePair<int, int>> Snapshot()
        {
            var list = new List<KeyValuePair<int, int>>();
            IntPtr snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if (snapshot == INVALID_HANDLE_VALUE) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            try
            {
                var entry = new PROCESSENTRY32W();
                entry.dwSize = Marshal.SizeOf(typeof(PROCESSENTRY32W));
                if (!Process32FirstW(snapshot, ref entry)) return list;
                do { list.Add(new KeyValuePair<int, int>(entry.th32ProcessID, entry.th32ParentProcessID)); }
                while (Process32NextW(snapshot, ref entry));
            }
            finally { CloseHandle(snapshot); }
            return list;
        }

        // The children of a member, each opened and checked from its own handle: its parent PID is
        // the member's, and it was created no earlier than the member. known holds every PID already
        // taken, and gains the ones returned. The caller owns the returned handles.
        static List<Member> OpenChildren(Member parent, uint access, HashSet<int> known)
        {
            var children = new List<Member>();
            foreach (KeyValuePair<int, int> e in Snapshot())
            {
                if (e.Value != parent.Pid || e.Key == parent.Pid || known.Contains(e.Key)) continue;
                SafeProcessHandle h = Open(e.Key, access);
                if (h == null) continue;
                long creation;
                if (!TryCreation(h, out creation) || creation < parent.Creation || ReadParentPid(h) != parent.Pid)
                {
                    h.Dispose();
                    continue;
                }
                known.Add(e.Key);
                children.Add(new Member { Pid = e.Key, Creation = creation, Handle = h });
            }
            return children;
        }

        // Starts a process with no window, in a new process group, with stdin from NUL and stdout
        // and stderr on one handle that appends to logPath. Only those two handles are inherited.
        // It asks to leave this process's job, and starts in the job when leaving is not allowed.
        // Returns the process as read from the handle CreateProcess gave, or throws.
        public static ProcessIdentity StartDetached(string application, string commandLine, string logPath, string workingDirectory)
        {
            var security = new SECURITY_ATTRIBUTES { nLength = Marshal.SizeOf(typeof(SECURITY_ATTRIBUTES)), lpSecurityDescriptor = IntPtr.Zero, bInheritHandle = 1 };
            using (SafeFileHandle input = CreateFileW("NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, ref security, OPEN_EXISTING, 0, IntPtr.Zero))
            {
                if (input.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error(), "NUL could not be opened.");
                using (SafeFileHandle output = CreateFileW(logPath, FILE_APPEND_DATA | SYNCHRONIZE, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    ref security, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, IntPtr.Zero))
                {
                    if (output.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error(), "The holder's log could not be opened.");
                    IntPtr handles = Marshal.AllocHGlobal(2 * IntPtr.Size);
                    IntPtr attributes = IntPtr.Zero;
                    bool initialized = false;
                    try
                    {
                        Marshal.WriteIntPtr(handles, 0, input.DangerousGetHandle());
                        Marshal.WriteIntPtr(handles, IntPtr.Size, output.DangerousGetHandle());
                        IntPtr size = IntPtr.Zero;
                        InitializeProcThreadAttributeList(IntPtr.Zero, 1, 0, ref size);
                        attributes = Marshal.AllocHGlobal(size);
                        if (!InitializeProcThreadAttributeList(attributes, 1, 0, ref size)) throw new Win32Exception(Marshal.GetLastWin32Error());
                        initialized = true;
                        if (!UpdateProcThreadAttribute(attributes, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, handles, new IntPtr(2 * IntPtr.Size), IntPtr.Zero, IntPtr.Zero))
                            throw new Win32Exception(Marshal.GetLastWin32Error());

                        var startup = new STARTUPINFOEX();
                        startup.StartupInfo.cb = Marshal.SizeOf(typeof(STARTUPINFOEX));
                        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
                        startup.StartupInfo.hStdInput = input.DangerousGetHandle();
                        startup.StartupInfo.hStdOutput = output.DangerousGetHandle();
                        startup.StartupInfo.hStdError = output.DangerousGetHandle();
                        startup.lpAttributeList = attributes;

                        uint flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
                        PROCESS_INFORMATION info;
                        bool ok = CreateProcessW(application, new StringBuilder(commandLine), IntPtr.Zero, IntPtr.Zero, true,
                            flags | CREATE_BREAKAWAY_FROM_JOB, IntPtr.Zero, workingDirectory, ref startup, out info);
                        int error = ok ? 0 : Marshal.GetLastWin32Error();
                        if (!ok && error == ERROR_ACCESS_DENIED)
                        {
                            ok = CreateProcessW(application, new StringBuilder(commandLine), IntPtr.Zero, IntPtr.Zero, true,
                                flags, IntPtr.Zero, workingDirectory, ref startup, out info);
                            error = ok ? 0 : Marshal.GetLastWin32Error();
                        }
                        if (!ok) throw new Win32Exception(error);
                        CloseHandle(info.hThread);
                        using (var process = new SafeProcessHandle(info.hProcess, true))
                        {
                            ProcessIdentity id = Identify(process, info.dwProcessId);
                            if (id == null) throw new Win32Exception(Marshal.GetLastWin32Error(), "The holder's start time could not be read.");
                            return id;
                        }
                    }
                    finally
                    {
                        if (initialized) DeleteProcThreadAttributeList(attributes);
                        if (attributes != IntPtr.Zero) Marshal.FreeHGlobal(attributes);
                        Marshal.FreeHGlobal(handles);
                    }
                }
            }
        }

        // Stops the process with this PID and every descendant that can be proved to be its, but
        // only when the process is still running, was created at startedAt, and carries the holder
        // command for sshExe and alias (HolderCommand.Matches). Otherwise nothing is stopped,
        // Matched is false, and Outcome says why.
        public static StopResult StopTree(int pid, string startedAt, string sshExe, string alias)
        {
            const uint access = PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE;
            var result = new StopResult { Matched = false, Outcome = "NotRunning", RootStopped = false, Stopped = new int[0], Survivors = new int[0] };
            var members = new List<Member>();
            try
            {
                int error;
                SafeProcessHandle rootHandle = Open(pid, access, out error);
                if (rootHandle == null)
                {
                    // ERROR_INVALID_PARAMETER: no process has this PID. Any other error, access
                    // denied above all, means a process may be running that this pass cannot stop.
                    result.Outcome = error == ERROR_INVALID_PARAMETER ? "NotRunning" : "CannotOpen";
                    result.OpenError = error;
                    return result;
                }
                var root = new Member { Pid = pid, Handle = rootHandle };
                members.Add(root);
                if (!IsRunning(rootHandle)) return result;
                ProcessIdentity id = Identify(rootHandle, pid);
                if (id == null || !string.Equals(id.StartedAt, startedAt, StringComparison.Ordinal) || !HolderCommand.Matches(id.CommandLine, sshExe, alias))
                {
                    result.Outcome = "IdentityDiffers";
                    return result;
                }
                root.Creation = id.CreationFileTime;
                result.Matched = true;
                result.Outcome = "Matched";

                // Walk the whole tree while it is alive, holding every handle.
                var known = new HashSet<int> { pid };
                for (int i = 0; i < members.Count; i++) members.AddRange(OpenChildren(members[i], access, known));

                var stopped = new List<int>();
                var survivors = new List<int>();
                int done = 0;
                for (int round = 0; round < 5; round++)
                {
                    // Leaves first, the root last: members are in walk order, so reverse it.
                    for (int i = members.Count - 1; i >= done; i--)
                    {
                        Member m = members[i];
                        if (!IsRunning(m.Handle)) continue;
                        if (TerminateProcess(m.Handle, 1)) stopped.Add(m.Pid);
                    }
                    for (int i = done; i < members.Count; i++) WaitForSingleObject(members[i].Handle, 2000);
                    // Any child made while the tree was being stopped.
                    int before = members.Count;
                    for (int i = 0; i < before; i++) members.AddRange(OpenChildren(members[i], access, known));
                    done = before;
                    if (members.Count == before) break;
                }
                foreach (Member m in members) if (IsRunning(m.Handle)) survivors.Add(m.Pid);
                result.RootStopped = !IsRunning(rootHandle);
                result.Stopped = stopped.ToArray();
                result.Survivors = survivors.ToArray();
                return result;
            }
            finally
            {
                foreach (Member m in members) m.Handle.Dispose();
            }
        }
    }
}
'@
}

$script:Native = [Agentspace.BoardHolder.Native]
$script:HolderCommand = [Agentspace.BoardHolder.HolderCommand]
$script:Utf8 = [System.Text.UTF8Encoding]::new($false)
$script:AliasPattern = '\A[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z'

# --- Files --------------------------------------------------------------------------------------

$script:StateDir = Get-LeaseStateDir
$script:RecordPath = Join-Path $script:StateDir 'board-holder.json'
$script:LogPath = Join-Path $script:StateDir 'board-holder.log'
$script:SshLogPath = Join-Path $script:StateDir 'board-holder-ssh.log'
$script:LockPath = Join-Path $script:StateDir 'board-holder.lock'

# Appends the pass's one line to board-holder.log and writes it to stdout.
function Write-HolderLog {
  param([string]$Action, [string]$LeaseId, [string]$Alias, [string]$ProcessId, [string]$Reason)
  $field = { param($v) if ([string]::IsNullOrWhiteSpace($v)) { '-' } else { $v -replace '\s', '_' } }
  $line = '{0} {1} lease={2} alias={3} pid={4}' -f (ConvertTo-LeaseTime $script:PassTime), $Action,
    (& $field $LeaseId), (& $field $Alias), (& $field $ProcessId)
  if ($Action -ceq 'failed' -or $Action -ceq 'skipped') {
    $text = ($Reason -replace '[\x00-\x1f\x7f]+', ' ').Trim()
    if (-not $text) { $text = 'Unknown.' }
    $line += " reason=$text"
  }
  [System.IO.File]::AppendAllText($script:LogPath, $line + "`n", $script:Utf8)
  Write-Output $line
}

# The record as a hashtable, or $null when there is none or it is not a valid record. A record that
# cannot be read names no process that can be trusted, so it counts as no holder.
function Read-HolderRecord {
  if (-not (Test-Path -LiteralPath $script:RecordPath -PathType Leaf)) { return $null }
  try {
    $r = [System.IO.File]::ReadAllText($script:RecordPath) | ConvertFrom-Json -AsHashtable -DateKind String
  } catch { return $null }
  if ($r -isnot [System.Collections.IDictionary]) { return $null }
  $p = $r['pid']
  if (-not ($p -is [int] -or $p -is [long]) -or $p -le 0 -or $p -gt [int]::MaxValue) { return $null }
  if ($r['process_started_at'] -isnot [string] -or
    $r['process_started_at'] -cnotmatch '\A[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}Z\z') { return $null }
  if ($r['lease_id'] -isnot [string] -or
    $r['lease_id'] -cnotmatch '\A[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\z') { return $null }
  if ($r['alias'] -isnot [string] -or $r['alias'] -cnotmatch $script:AliasPattern) { return $null }
  @{ pid = [int]$p; process_started_at = $r['process_started_at']; lease_id = $r['lease_id']; alias = $r['alias'] }
}

# Writes the record through a temporary file moved over the old one (Write-JsonFile).
function Write-HolderRecord {
  param([int]$ProcessId, [string]$StartedAt, [string]$LeaseId, [string]$Alias)
  Write-JsonFile $script:RecordPath ([ordered]@{ pid = $ProcessId; process_started_at = $StartedAt; lease_id = $LeaseId; alias = $Alias })
}

function Remove-HolderRecord {
  if (Test-Path -LiteralPath $script:RecordPath) { Remove-Item -LiteralPath $script:RecordPath -Force }
}

# --- The holder command ---------------------------------------------------------------------------

# The holder for Alias: Application is what CreateProcess runs, and CommandLine is the command line
# the keeper gives it (HolderCommand.Build). A batch runs through cmd, in cmd's own process.
function Get-HolderCommand([string]$Exe, [string]$Alias) {
  [pscustomobject]@{ Application = $script:HolderCommand::Application($Exe); CommandLine = $script:HolderCommand::Build($Exe, $Alias) }
}

# Why -SshExe is not a usable path, from its text alone, or $null when it is. A quote would end a
# quoted path early, and cmd expands % in a batch's path, so a path holding one is refused, not
# escaped. The main block exits 2 on any of these; whether the file exists is the pass's concern.
function Test-SshExeForm([string]$Exe) {
  if (-not [System.IO.Path]::IsPathFullyQualified($Exe)) { return "SshExe '$Exe' is not a full path." }
  if ($Exe.Contains('"')) { return "SshExe '$Exe' holds a quote." }
  if ($Exe.Contains('%') -and [System.IO.Path]::GetExtension($Exe) -in '.cmd', '.bat') { return "SshExe '$Exe' is a batch whose path holds a percent sign." }
  $null
}

# Why a holder cannot be started with these paths, or $null when it can.
function Test-HolderPaths([string]$Exe, [string]$Alias) {
  $why = Test-SshExeForm $Exe
  if ($why) { return $why }
  if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { return "SshExe '$Exe' does not exist." }
  if ($script:StateDir.Contains('"')) { return "The state folder '$($script:StateDir)' holds a quote." }
  if ($PSCommandPath.Contains('"')) { return "This script's path '$PSCommandPath' holds a quote." }
  if ($Alias -cnotmatch $script:AliasPattern) { return "Alias '$Alias' is not a host alias." }
  $null
}

# True when the process is the one expected: running, created at StartedAt to the millisecond, and
# carrying the holder command for Exe and Alias, read structurally (HolderCommand.Matches, the same
# rule StopTree applies on its own handle).
function Test-Identity($Identity, [string]$StartedAt, [string]$Exe, [string]$Alias) {
  $null -ne $Identity -and $Identity.StartedAt -ceq $StartedAt -and $script:HolderCommand::Matches($Identity.CommandLine, $Exe, $Alias)
}

# Writes a JSON document through a temporary file in the same folder, moved over the old file, so a
# reader sees the old document or the new one, never part of one.
function Write-JsonFile([string]$Path, $Document) {
  $text = ($Document | ConvertTo-Json -Compress) + "`n"
  $tmp = Join-Path (Split-Path -Parent $Path) ("{0}.{1}.tmp" -f (Split-Path -Leaf $Path), [guid]::NewGuid().ToString('n'))
  try {
    [System.IO.File]::WriteAllText($tmp, $text, $script:Utf8)
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ($true) {
      try { [System.IO.File]::Move($tmp, $Path, $true); break }
      catch [System.IO.IOException], [System.UnauthorizedAccessException] {
        if ([DateTime]::UtcNow -gt $deadline) { throw }
        Start-Sleep -Milliseconds 50
      }
    }
  } finally {
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue }
  }
}

# The claim file of a launch request: board-holder-launch.<guid>.json has
# board-holder-launch.<guid>.claim, in the same folder.
function Get-LaunchClaimPath([string]$RequestPath) {
  [System.IO.Path]::ChangeExtension($RequestPath, '.claim')
}

# Claims a launch request by creating its claim file with FileMode.CreateNew (CREATE_NEW), which
# exactly one caller can do. By says who claimed it. Returns $true when this caller claimed it, and
# $false when the file already exists or could not be created.
function Request-LaunchClaim([string]$ClaimPath, [string]$By) {
  try {
    $stream = [System.IO.FileStream]::new($ClaimPath, [System.IO.FileMode]::CreateNew,
      [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
  } catch { return $false }
  try {
    $bytes = $script:Utf8.GetBytes("$By`n")
    $stream.Write($bytes, 0, $bytes.Length)
  } catch { } finally { $stream.Dispose() }
  $true
}

# The launcher (-Launch): claims the request, starts the holder unless its deadline has passed, and
# writes the result. It never reads the state folder from the environment, since a process made by
# Win32_Process.Create has the logon environment, not the pass's.
function Invoke-HolderLaunch([string]$RequestPath) {
  # The claim comes before anything else. It fails when the claim file exists, because the pass
  # withdrew the request or another launcher claimed it first; then this launcher starts nothing
  # and writes nothing, since no pass is waiting for its answer.
  $claimPath = Get-LaunchClaimPath $RequestPath
  if (-not (Request-LaunchClaim $claimPath "launcher $PID")) { return }
  try {
    $request = [System.IO.File]::ReadAllText($RequestPath) | ConvertFrom-Json -AsHashtable -DateKind String
  } catch {
    # The pass has removed the request, so it waits for nothing. The claim is removed with it.
    Remove-Item -LiteralPath $claimPath -Force -ErrorAction SilentlyContinue
    return
  }
  $answer = try {
    $notAfter = [DateTime]::Parse($request['not_after'], [Globalization.CultureInfo]::InvariantCulture,
      [Globalization.DateTimeStyles]::RoundtripKind)
    if ([DateTime]::UtcNow -gt $notAfter.ToUniversalTime()) {
      throw 'The launcher reached its deadline before it started the holder, so it started nothing.'
    }
    $started = $script:Native::StartDetached($request['application'], $request['command_line'], $request['log'], $request['directory'])
    [ordered]@{ pid = $started.Pid; process_started_at = $started.StartedAt; command_line = $started.CommandLine }
  } catch {
    [ordered]@{ error = $_.Exception.Message }
  }
  Write-JsonFile $request['result'] $answer
}

# Starts a holder through Alias by way of the launcher. Returns its identity, or throws with the
# reason.
function Start-Holder([string]$Exe, [string]$Alias) {
  $command = Get-HolderCommand $Exe $Alias
  $name = 'board-holder-launch.{0}' -f [guid]::NewGuid().ToString('n')
  $requestPath = Join-Path $script:StateDir "$name.json"
  $resultPath = Join-Path $script:StateDir "$name.result.json"
  $claimPath = Get-LaunchClaimPath $requestPath
  $notAfter = [DateTime]::UtcNow.AddSeconds(30)
  try {
    Write-JsonFile $requestPath ([ordered]@{
        application  = $command.Application
        command_line = $command.CommandLine
        log          = $script:SshLogPath
        directory    = $env:USERPROFILE
        not_after    = $notAfter.ToString('o', [Globalization.CultureInfo]::InvariantCulture)
        result       = $resultPath
      })
    $pwsh = Join-Path $PSHOME 'pwsh.exe'
    $launcher = "`"$pwsh`" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$PSCommandPath`" -Launch `"$requestPath`""
    $startup = New-CimInstance -ClientOnly -Namespace root/cimv2 -ClassName Win32_ProcessStartup -Property @{ ShowWindow = [uint16]0 }
    $arguments = @{ CommandLine = $launcher; CurrentDirectory = $env:USERPROFILE; ProcessStartupInformation = $startup }
    # WMI can be slow to answer just after logon: three tries, 2 seconds apart, never more (D14).
    # A call that threw may still have made a launcher, so a failed create is not the end: the
    # request is withdrawn below like any other, and a launcher that claimed it is waited for.
    $created = $null
    $createError = $null
    for ($try = 1; -not $created -and -not $createError; $try++) {
      try { $created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments $arguments }
      catch {
        if ($try -ge 3) { $createError = "Win32_Process.Create could not be called: $($_.Exception.Message)" }
        else { Start-Sleep -Seconds 2 }
      }
    }
    if ($created -and $created.ReturnValue -ne 0) { $createError = "Win32_Process.Create returned $($created.ReturnValue)." }
    # Wait for the answer until 5 seconds past the launcher's deadline, or not at all when the
    # create failed. Then withdraw the request by claiming it: if the pass's claim wins, no
    # launcher can claim it, and none starts a holder. If a launcher claimed it first, that
    # launcher may still start one, so wait on for its answer, up to 60 more seconds, and record
    # the holder if one comes. The wait is bounded, and nothing is retried (D14).
    $giveUp = if ($createError) { [DateTime]::UtcNow } else { $notAfter.AddSeconds(5) }
    $why = if ($createError) { $createError } else { 'The launcher did not answer within 35 seconds.' }
    $late = $null
    while (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
      $clock = [DateTime]::UtcNow
      if ($clock -ge $giveUp) {
        if (-not (Test-Path -LiteralPath $claimPath) -and (Request-LaunchClaim $claimPath 'abandoned')) {
          throw "$why Its request was withdrawn, so no holder was started."
        }
        if (-not $late) { $late = $clock.AddSeconds(60) }
        if ($clock -gt $late) {
          throw "$why A launcher claimed the request, or it could not be withdrawn, and no answer came in 60 more seconds, so a holder may be running unrecorded."
        }
      }
      Start-Sleep -Milliseconds 100
    }
    $answer = [System.IO.File]::ReadAllText($resultPath) | ConvertFrom-Json -AsHashtable -DateKind String
    if ($answer.Contains('error')) { throw $answer['error'] }
    $holder = $script:Native::Probe([int]$answer['pid'])
    if (-not (Test-Identity $holder $answer['process_started_at'] $Exe $Alias)) {
      throw 'The holder exited at once; see board-holder-ssh.log.'
    }
    $holder
  } finally {
    # The request goes before its claim, so a launcher that claims it after the claim is gone
    # finds no request, and starts nothing.
    foreach ($p in $requestPath, $claimPath, $resultPath) {
      if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue }
    }
  }
}

# --- One pass -----------------------------------------------------------------------------------

function Invoke-HolderPass {
  # The matcher that decides trust checks itself first; a pass never trusts by a broken rule.
  $broken = $script:HolderCommand::Check()
  if ($broken.Count -gt 0) { throw "The holder command matcher failed its own checks: $($broken -join '; ')" }

  try {
    $leases = Read-LeaseState
  } catch {
    Write-HolderLog -Action failed -Reason "Lease state could not be read: $($_.Exception.Message)"
    return 1
  }

  $ready = @($leases | Where-Object {
      $_['state'] -ceq 'ready' -and (ConvertFrom-LeaseTime $_['expires_at']) -gt $script:PassTime
    })

  # The holder, when the record names a process that is still the one it started.
  $record = Read-HolderRecord
  $holder = $null
  if ($record) {
    $probe = $script:Native::Probe($record.pid)
    if (Test-Identity $probe $record.process_started_at $SshExe $record.alias) { $holder = $probe }
  }
  if (-not $holder) {
    # A record that names no trusted holder is dropped. Its process, if any, is left alone.
    Remove-HolderRecord
  }

  if ($holder) {
    $lease = $ready | Where-Object { $_['id'] -ceq $record.lease_id } | Select-Object -First 1
    if ($lease -and $lease['ssh']['host_alias'] -ceq $record.alias) {
      Write-HolderLog -Action kept -LeaseId $record.lease_id -Alias $record.alias -ProcessId $record.pid
      return 0
    }
    try { $result = $script:Native::StopTree($holder.Pid, $holder.StartedAt, $SshExe, $record.alias) }
    catch {
      Write-HolderLog -Action failed -LeaseId $record.lease_id -Alias $record.alias -ProcessId $record.pid -Reason "The holder could not be stopped: $($_.Exception.Message)"
      return 0
    }
    if (-not $result.Matched) {
      # Nothing was stopped. The record MUST stay while the recorded holder is still running, so
      # the next pass tries again; it goes only when that holder is gone.
      $again = $script:Native::Probe($record.pid)
      if (Test-Identity $again $record.process_started_at $SshExe $record.alias) {
        $why = if ($result.Outcome -ceq 'CannotOpen') {
          "The holder could not be opened to stop it: Windows error $($result.OpenError)."
        } else {
          "The holder could not be stopped: StopTree found $($result.Outcome), but the holder is still running."
        }
        Write-HolderLog -Action failed -LeaseId $record.lease_id -Alias $record.alias -ProcessId $record.pid -Reason $why
        return 0
      }
      Remove-HolderRecord
      Write-HolderLog -Action stopped -LeaseId $record.lease_id -Alias $record.alias -ProcessId $record.pid
      return 0
    }
    if (-not $result.RootStopped) {
      # The record stays, so the next pass tries again.
      Write-HolderLog -Action failed -LeaseId $record.lease_id -Alias $record.alias -ProcessId $record.pid -Reason 'The holder is still running after it was stopped.'
      return 0
    }
    Remove-HolderRecord
    if ($result.Survivors.Count -gt 0) {
      Write-HolderLog -Action failed -LeaseId $record.lease_id -Alias $record.alias -ProcessId $record.pid -Reason "The holder stopped, but these processes of its tree did not: $($result.Survivors -join ',')."
      return 0
    }
    Write-HolderLog -Action stopped -LeaseId $record.lease_id -Alias $record.alias -ProcessId $record.pid
    return 0
  }

  if ($ready.Count -eq 0) {
    Write-HolderLog -Action idle
    return 0
  }

  # The ready lease that expires last; ties go to the lowest lease ID, so the choice is stable.
  $lease = $ready |
    Sort-Object @{ Expression = { ConvertFrom-LeaseTime $_['expires_at'] }; Descending = $true },
    @{ Expression = { $_['id'] }; Descending = $false } |
    Select-Object -First 1
  $leaseId = $lease['id']
  $alias = $lease['ssh']['host_alias']

  $why = Test-HolderPaths $SshExe $alias
  if ($why) {
    Write-HolderLog -Action failed -LeaseId $leaseId -Alias $alias -Reason "The holder could not be started: $why"
    return 0
  }
  try { $started = Start-Holder $SshExe $alias }
  catch {
    Write-HolderLog -Action failed -LeaseId $leaseId -Alias $alias -Reason "The holder could not be started: $($_.Exception.Message)"
    return 0
  }
  try {
    Write-HolderRecord -ProcessId $started.Pid -StartedAt $started.StartedAt -LeaseId $leaseId -Alias $alias
  } catch {
    # An unrecorded holder could never be stopped, so it is stopped now, while it is known.
    $reason = $_.Exception.Message
    try { [void]$script:Native::StopTree($started.Pid, $started.StartedAt, $SshExe, $alias) } catch { }
    if (-not (Test-Identity ($script:Native::Probe($started.Pid)) $started.StartedAt $SshExe $alias)) {
      Write-HolderLog -Action failed -LeaseId $leaseId -Alias $alias -ProcessId $started.Pid -Reason "The record could not be written, so the holder was stopped, or had exited: $reason"
    } else {
      Write-HolderLog -Action failed -LeaseId $leaseId -Alias $alias -ProcessId $started.Pid -Reason "The record could not be written, and the holder could not be stopped, so it runs unrecorded: $reason"
    }
    return 0
  }
  Write-HolderLog -Action started -LeaseId $leaseId -Alias $alias -ProcessId $started.Pid
  0
}

# --- Main ---------------------------------------------------------------------------------------

if ($Launch) {
  # The launcher writes its own result, error included; nothing else is logged.
  Invoke-HolderLaunch $Launch
  exit 0
}

$why = Test-SshExeForm $SshExe
if ($why) { Exit-Usage $why }
if ($Now) {
  try { $script:PassTime = ConvertFrom-LeaseTime $Now }
  catch { Exit-Usage "-Now '$Now' is not a time in the form YYYY-MM-DDTHH:MM:SSZ." }
} else {
  $script:PassTime = Get-LeaseNow
}

if (-not (Test-Path -LiteralPath $script:StateDir -PathType Container)) {
  New-Item -ItemType Directory -Force -Path $script:StateDir | Out-Null
}

$code = 1
$lock = $null
try {
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  while (-not $lock) {
    try {
      $lock = [System.IO.FileStream]::new($script:LockPath, [System.IO.FileMode]::OpenOrCreate,
        [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    } catch [System.IO.IOException] {
      if ([DateTime]::UtcNow -gt $deadline) { break }
      Start-Sleep -Milliseconds 100
    }
  }
  if (-not $lock) {
    # Another pass is running: this one is skipped, and says so; it is never logged as idle.
    Write-HolderLog -Action skipped -Reason 'locked'
    $code = 0
  } else {
    # The pass writes its log line to the pipeline and returns its exit code last.
    $script:PassCode = 1
    Invoke-HolderPass | ForEach-Object { if ($_ -is [int]) { $script:PassCode = $_ } else { Write-Output $_ } }
    $code = $script:PassCode
  }
} catch {
  try { Write-HolderLog -Action failed -Reason "The pass stopped on an error: $($_.Exception.Message)" } catch { }
  $code = 1
} finally {
  if ($lock) { $lock.Dispose() }
}
exit $code

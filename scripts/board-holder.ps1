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
#
# Exit codes: 0 when the pass ran, whatever its action, failed included. 1 when the lease state
# could not be read (a failed line is logged and the holder is left as it is), when -Now is not a
# time in the contract's form (nothing is logged), or when the pass met an error it does not
# handle (a failed line is logged).
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
#   lease state or a busy lock is "failed lease=- alias=- pid=- reason=...". reason is the last field
#   and runs to the end of the line; it never holds a control character.
# - board-holder-ssh.log, the holder's stdout and stderr, appended. It holds raw bytes in the
#   console's code page, and nothing parses it. Its growth is not bounded yet.
# - board-holder.lock, held with FileShare.None for the whole pass, so that two passes never both
#   start a holder (one holder per host, D30). A pass that cannot take it in 10 seconds logs failed.
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
# - -Launch is internal, not part of the interface. The request and the result are JSON files in
#   the state folder, board-holder-launch.<guid>.json and .result.json, removed by the pass. The
#   request carries a deadline: a launcher that reaches it late starts nothing, so a pass that has
#   given up waiting never leaves a holder it did not record.
# - A holder made this way gets the user's logon environment, not this pwsh's variables.
# - If the holder exits at once, or its record cannot be written, the pass logs failed; in the
#   second case it stops the holder first, since an unrecorded holder could never be stopped.
#
# Trust: this pass MUST NOT stop any process it did not start, even one whose command line names an
# alias. A recorded PID is trusted only when all of these hold:
# - A process with that PID is running and can be queried.
# - Its creation time, as UTC truncated to the millisecond, equals process_started_at as a string.
# - Its command line equals, exactly and case included, the holder command this script builds for
#   the recorded alias. CreateProcess keeps a command line exactly as given, so the comparison has
#   no slack. So a pass MUST be run with the same -SshExe as the pass that started the holder; with
#   another, the old holder is untrusted and left alone.
# A recorded PID that is not trusted is treated as no holder: its record is removed, the process
# is left alone, and a holder is started afresh if a lease is ready.
#
# How a trusted holder is stopped (Agentspace.BoardHolder.Native.StopTree):
# - The holder is opened by PID, and its creation time and command line are checked again on that
#   same handle. While the handle is open Windows cannot reuse the PID, so the process checked is
#   the process stopped.
# - Its descendants are found by walking parent PIDs, and each one is checked from its own handle:
#   its parent PID is its parent's and it was created no earlier than its parent. A process whose
#   parent died before the walk cannot be traced by any means and is left alone; nothing is ever
#   matched by command line alone.
# - Every handle is held until the end, so no PID in the tree is reused while it is being stopped.
#   The tree is stopped leaves first, the holder last, then walked again for any child made
#   meanwhile.
#
# A holder that fails is not restarted in a loop: each pass starts at most one, and backoff
# between passes belongs to -Watch, which 0.02.T2.2 adds (D14, D30).
[CmdletBinding()]
param(
  [string]$SshExe = 'C:\Windows\System32\OpenSSH\ssh.exe',
  [string]$Now,
  [Parameter(DontShow)][string]$Launch
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lease-state.ps1')

if (-not ('Agentspace.BoardHolder.Native' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Globalization;
using System.ComponentModel;
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

    // What StopTree did. Matched is false when the process was not the one recorded, in which case
    // nothing was stopped. RootStopped is true when the root is no longer running.
    public sealed class StopResult
    {
        public bool Matched;
        public bool RootStopped;
        public int[] Stopped;
        public int[] Survivors;
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

        static bool SameCommandLine(string actual, string expected)
        {
            return actual != null && expected != null && string.Equals(actual, expected, StringComparison.Ordinal);
        }

        static SafeProcessHandle Open(int pid, uint access)
        {
            if (pid <= 0) return null;
            SafeProcessHandle h = OpenProcess(access, false, pid);
            if (h.IsInvalid) { h.Dispose(); return null; }
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
        // only when the process is still running, was created at startedAt and has the command line
        // given. Otherwise nothing is stopped and Matched is false.
        public static StopResult StopTree(int pid, string startedAt, string commandLine)
        {
            const uint access = PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE;
            var result = new StopResult { Matched = false, RootStopped = false, Stopped = new int[0], Survivors = new int[0] };
            var members = new List<Member>();
            try
            {
                SafeProcessHandle rootHandle = Open(pid, access);
                if (rootHandle == null) return result;
                var root = new Member { Pid = pid, Handle = rootHandle };
                members.Add(root);
                if (!IsRunning(rootHandle)) return result;
                ProcessIdentity id = Identify(rootHandle, pid);
                if (id == null || !string.Equals(id.StartedAt, startedAt, StringComparison.Ordinal) || !SameCommandLine(id.CommandLine, commandLine)) return result;
                root.Creation = id.CreationFileTime;
                result.Matched = true;

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
  if ($Action -ceq 'failed') {
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
# the holder carries, which trust compares exactly. A batch runs through cmd, in cmd's own process.
function Get-HolderCommand([string]$Exe, [string]$Alias) {
  $holder = "`"$Exe`" -N -o ExitOnForwardFailure=yes $Alias"
  if ([System.IO.Path]::GetExtension($Exe) -in '.cmd', '.bat') {
    $cmd = Join-Path ([Environment]::SystemDirectory) 'cmd.exe'
    return [pscustomobject]@{ Application = $cmd; CommandLine = "`"$cmd`" /d /c `"$holder`"" }
  }
  [pscustomobject]@{ Application = $Exe; CommandLine = $holder }
}

# Why a holder cannot be started with these paths, or $null when it can. A quote would end a quoted
# path early, and cmd expands % in a batch's path, so a path holding one is refused, not escaped.
function Test-HolderPaths([string]$Exe, [string]$Alias) {
  if (-not [System.IO.Path]::IsPathFullyQualified($Exe)) { return "SshExe '$Exe' is not a full path." }
  if ($Exe.Contains('"')) { return "SshExe '$Exe' holds a quote." }
  if ($Exe.Contains('%') -and [System.IO.Path]::GetExtension($Exe) -in '.cmd', '.bat') { return "SshExe '$Exe' is a batch whose path holds a percent sign." }
  if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { return "SshExe '$Exe' does not exist." }
  if ($script:StateDir.Contains('"')) { return "The state folder '$($script:StateDir)' holds a quote." }
  if ($PSCommandPath.Contains('"')) { return "This script's path '$PSCommandPath' holds a quote." }
  if ($Alias -cnotmatch $script:AliasPattern) { return "Alias '$Alias' is not a host alias." }
  $null
}

# True when the process is the one expected: running, created at StartedAt, and carrying exactly
# CommandLine.
function Test-Identity($Identity, [string]$StartedAt, [string]$CommandLine) {
  $null -ne $Identity -and $Identity.StartedAt -ceq $StartedAt -and $Identity.CommandLine -ceq $CommandLine
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

# The launcher (-Launch): reads the request, starts the holder unless its deadline has passed, and
# writes the result. It never reads the state folder from the environment, since a process made by
# Win32_Process.Create has the logon environment, not the pass's.
function Invoke-HolderLaunch([string]$RequestPath) {
  $request = [System.IO.File]::ReadAllText($RequestPath) | ConvertFrom-Json -AsHashtable -DateKind String
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
    $created = $null
    for ($try = 1; -not $created; $try++) {
      try { $created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments $arguments }
      catch {
        if ($try -ge 3) { throw "Win32_Process.Create could not be called: $($_.Exception.Message)" }
        Start-Sleep -Seconds 2
      }
    }
    if ($created.ReturnValue -ne 0) { throw "Win32_Process.Create returned $($created.ReturnValue)." }
    # Wait for the answer until 5 seconds past the launcher's deadline, after which it starts
    # nothing, so giving up here never leaves a holder unrecorded.
    $giveUp = $notAfter.AddSeconds(5)
    while (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
      if ([DateTime]::UtcNow -gt $giveUp) { throw 'The launcher did not answer within 35 seconds.' }
      Start-Sleep -Milliseconds 100
    }
    $answer = [System.IO.File]::ReadAllText($resultPath) | ConvertFrom-Json -AsHashtable -DateKind String
    if ($answer.Contains('error')) { throw $answer['error'] }
    $holder = $script:Native::Probe([int]$answer['pid'])
    if (-not (Test-Identity $holder $answer['process_started_at'] $command.CommandLine)) {
      throw 'The holder exited at once; see board-holder-ssh.log.'
    }
    $holder
  } finally {
    foreach ($p in $requestPath, $resultPath) {
      if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue }
    }
  }
}

# --- One pass -----------------------------------------------------------------------------------

function Invoke-HolderPass {
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
  $command = $null
  if ($record) {
    $command = Get-HolderCommand $SshExe $record.alias
    $probe = $script:Native::Probe($record.pid)
    if (Test-Identity $probe $record.process_started_at $command.CommandLine) { $holder = $probe }
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
    try { $result = $script:Native::StopTree($holder.Pid, $holder.StartedAt, $command.CommandLine) }
    catch {
      Write-HolderLog -Action failed -LeaseId $record.lease_id -Alias $record.alias -ProcessId $record.pid -Reason "The holder could not be stopped: $($_.Exception.Message)"
      return 0
    }
    if ($result.Matched -and -not $result.RootStopped) {
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
    try { [void]$script:Native::StopTree($started.Pid, $started.StartedAt, $started.CommandLine) } catch { }
    Write-HolderLog -Action failed -LeaseId $leaseId -Alias $alias -ProcessId $started.Pid -Reason "The record could not be written, so the holder was stopped: $reason"
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

if ($Now) {
  try { $script:PassTime = ConvertFrom-LeaseTime $Now }
  catch {
    [Console]::Error.WriteLine("-Now '$Now' is not a time in the form YYYY-MM-DDTHH:MM:SSZ.")
    exit 1
  }
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
    Write-HolderLog -Action failed -Reason 'Another pass held board-holder.lock for 10 seconds.'
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

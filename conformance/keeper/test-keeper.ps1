#Requires -Version 7
# A conformance check under D34: it drives the keeper only as a separate process, through what docs/keeper.md and docs/state.md document, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The keeper's process cases (docs/keeper.md, docs/state.md), run against the program that
# AGENTSPACE_KEEPER_CMD names. When it is unset or empty, the default is the one in
# conformance/prototype/ (docs/conformance.md, "Naming the program under test"). The program is run
# with the keeper's own arguments, unchanged, as: <program> [-SshExe <full path>] [-Now <time>] and
# <program> -Watch [-SshExe <full path>] [-IntervalSeconds <n>] ... (keeper.md, "Command line").
# A check MUST NOT name the keeper's program, script or entry point in any other way.
#
# What it touches:
# - Every case has its own throwaway folder under $env:TEMP: a state folder, named by
#   AGENTSPACE_STATE_DIR, with a hand-written leases.json, and a home folder, named by USERPROFILE.
#   AGENTSPACE_SSH_DIR points into the home, the AWS variables name files that do not exist or are
#   removed, and every folder is removed in a finally. The real %USERPROFILE%\.agentspace, ~/.ssh, the
#   real ssh.exe, the real keeper task and the box are never read, written, listed or hashed.
# - The stand-in for ssh.exe is conformance\keeper\ssh-stub.cmd, copied into the run's folder so that
#   its command line is unique to this run. It prints a line, then waits about 20 minutes, and carries
#   its arguments on its own command line; the ping it runs is its child, which makes a tree.
# - Nothing is ever stopped by matching a command line alone, except the run's own cleanup: it stops
#   the processes whose command line holds this run's unique throwaway folder, which only this run
#   and the keepers and holders it started can have. Every keeper a case starts is stopped by the PID
#   of the process the case started, and that process's own children, never by a search for one. A
#   holder is spared when a case kills a keeper, because a holder MUST outlive its keeper (keeper.md,
#   "What the keeper does"). Each process is pinned by a handle and its start time checked first, so
#   a reused PID is never stopped. taskkill /T, which follows parent PIDs alone, is never used.
# - One case denies PROCESS_TERMINATE on a stub of its own, through the stub's DACL, so that the
#   keeper cannot open it to stop it. Cleanup gives that right back before it stops the stub.
#
# Every wait is on what a case checks for, with a deadline, never a fixed sleep (docs/keeper.md,
# "Checking the keeper", card #0066): the log line, the file or the process. Where a case must show
# that something does NOT happen, it waits on that thing with a deadline and expects the deadline.
#
# Every case MUST be shown able to fail. -SelfCheck does it: each case is run again with a
# deliberately broken fixture (for example the lease is released where the case expects it ready,
# or the record names a process the case expects to be foreign), and the case MUST then register at
# least one failed check. A case that still passes on its broken fixture cannot fail and is reported
# CANNOT-FAIL, which exits 1. The self check is meaningful only for a keeper that passes the normal run.
#
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line. A check name starts with the spec section it enforces. Exit 0 when every check
# passed, 1 when any failed, 2 when none failed but any was skipped (a run with a skip is
# incomplete, never passed).
# -Case <wildcard> runs only the cases whose name matches it, for example -Case 'Watch*'.
# The whole run takes about 7 minutes, and longer with -SelfCheck. It MUST NOT run under a timeout
# shorter than its run (docs/conformance.md, "What a conformance run needs").
# Run:  pwsh -NoProfile -File conformance\keeper\test-keeper.ps1 [-SelfCheck] [-Case <wildcard>]
param(
  [switch]$SelfCheck,
  [string]$Case = '*'
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$ci = [Globalization.CultureInfo]::InvariantCulture
$script:passes = 0
$script:failures = 0
$script:skips = 0
$script:mut = [bool]$SelfCheck
$script:caseFails = 0
$script:caseChecks = 0
$script:caseSpec = 'keeper.md'

# A check name starts with the spec section it enforces; a failure says what it was waiting for.
function Get-DefaultWhy {
  try {
    $l = Read-LogLines
    if ($l.Count -gt 0) { return 'The check did not hold. board-holder.log ends: ' + (($l | Select-Object -Last 4) -join ' | ') }
  } catch { }
  'The check did not hold. board-holder.log is absent or empty.'
}

function Write-Result([bool]$Ok, [string]$Text, [string]$Why = '') {
  $script:caseChecks++
  if (-not $Ok) { $script:caseFails++ }
  if ($script:mut) { return }
  $name = "$($script:caseSpec): $Text"
  if ($Ok) { Write-Output "PASS $name"; $script:passes++ }
  else {
    if (-not $Why) { $Why = Get-DefaultWhy }
    Write-Output "FAIL ${name}: $Why"
    $script:failures++
  }
}

function Write-Skipped([string]$Text, [string]$Why) {
  if ($script:mut) { return }
  Write-Output "SKIPPED $($script:caseSpec): ${Text}: $Why"
  $script:skips++
}

function Exit-Run {
  if ($SelfCheck) {
    Write-Output "Self check: $($script:failures) case(s) cannot fail."
    if ($script:failures) { exit 1 }
    exit 0
  }
  Write-Output "$($script:passes) passed, $($script:failures) failed, $($script:skips) skipped"
  if ($script:failures) { exit 1 }
  if ($script:skips) { exit 2 }
  exit 0
}

if (-not $IsWindows) {
  Write-Output 'SKIPPED keeper.md#where-it-runs: The keeper specified is a Windows keeper; this host is not Windows.'
  Write-Output '0 passed, 0 failed, 1 skipped'
  exit 2
}

# The program under test: its variable first, and only then the default that conformance/prototype/ names.
$keeperCmd = $env:AGENTSPACE_KEEPER_CMD
if ([string]::IsNullOrWhiteSpace($keeperCmd)) {
  . (Join-Path $PSScriptRoot '..\prototype\process-defaults.ps1')
  $keeperCmd = Get-PrototypeProcessDefault 'AGENTSPACE_KEEPER_CMD'
}
$keeperCmd = [IO.Path]::GetFullPath($keeperCmd)
if (-not (Test-Path -LiteralPath $keeperCmd -PathType Leaf)) {
  Write-Output "FAIL keeper.md#command-line: The program that AGENTSPACE_KEEPER_CMD names exists: $keeperCmd is not a file."
  Write-Output '0 passed, 1 failed, 0 skipped'
  exit 1
}

# The state schemas the new case checks the keeper's files against (state.md, "The keeper's files").
$schemaDir = Join-Path $repo 'schemas\state'

# Sets a process's DACL, so that one case can deny PROCESS_TERMINATE on a stub of its own; and a
# watcher that keeps a copy of the short-lived launch files a pass writes and removes. Compiled
# before any environment variable is changed.
if (-not ('AgentspaceTest.ProcessAcl' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Concurrent;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Threading;
namespace AgentspaceTest
{
    public static class ProcessAcl
    {
        [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
        [DllImport("advapi32.dll", SetLastError = true)] static extern bool SetKernelObjectSecurity(IntPtr h, uint information, byte[] descriptor);
        const uint WRITE_DAC = 0x00040000;
        const uint DACL_SECURITY_INFORMATION = 0x4;
        public static void Set(int pid, string sddl)
        {
            var sd = new RawSecurityDescriptor(sddl);
            byte[] bytes = new byte[sd.BinaryLength];
            sd.GetBinaryForm(bytes, 0);
            IntPtr h = OpenProcess(WRITE_DAC, false, pid);
            if (h == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            try { if (!SetKernelObjectSecurity(h, DACL_SECURITY_INFORMATION, bytes)) throw new Win32Exception(Marshal.GetLastWin32Error()); }
            finally { CloseHandle(h); }
        }
    }

    // Keeps the first complete copy of every board-holder-launch.* file that appears in a folder
    // (never a temporary file), however briefly it lives. A file is complete when it ends in a line feed.
    public sealed class FileCapture
    {
        readonly string dir;
        Thread thread;
        volatile bool stop;
        readonly ConcurrentDictionary<string, byte[]> files = new ConcurrentDictionary<string, byte[]>(StringComparer.OrdinalIgnoreCase);
        public FileCapture(string dir) { this.dir = dir; }
        public void Start() { thread = new Thread(Run); thread.IsBackground = true; thread.Start(); }
        public ConcurrentDictionary<string, byte[]> Stop() { stop = true; thread.Join(); return files; }
        void Run()
        {
            while (!stop)
            {
                try
                {
                    foreach (string path in Directory.EnumerateFiles(dir, "board-holder-launch.*"))
                    {
                        string name = Path.GetFileName(path);
                        if (name.EndsWith(".tmp", StringComparison.OrdinalIgnoreCase) || files.ContainsKey(name)) continue;
                        try
                        {
                            using (var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
                            using (var ms = new MemoryStream())
                            {
                                fs.CopyTo(ms);
                                byte[] b = ms.ToArray();
                                if (b.Length > 0 && b[b.Length - 1] == 10) files[name] = b;
                            }
                        }
                        catch (IOException) { }
                        catch (UnauthorizedAccessException) { }
                    }
                }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
                Thread.Sleep(1);
            }
        }
    }
}
'@
}
$userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
# Every right for the user and SYSTEM; Lock adds a deny of PROCESS_TERMINATE (0x1) for everyone first.
$openAcl = "D:(A;;0x1FFFFF;;;$userSid)(A;;0x1FFFFF;;;SY)"
$lockedAcl = "D:(D;;0x1;;;WD)(A;;0x1FFFFF;;;$userSid)(A;;0x1FFFFF;;;SY)"
function Lock-Process([int]$ProcessId) { [AgentspaceTest.ProcessAcl]::Set($ProcessId, $lockedAcl) }
function Unlock-Process([int]$ProcessId) { try { [AgentspaceTest.ProcessAcl]::Set($ProcessId, $openAcl) } catch { } }

# The run's own folder, and a copy of the stub whose command line is unique to this run.
$temp = Join-Path $env:TEMP "agentspace-conformance-keeper-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $temp | Out-Null
$stub = Join-Path $temp 'ssh-stub.cmd'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'ssh-stub.cmd') -Destination $stub

# Every environment variable a case changes, saved here and put back in the finally at the end.
$envNames = @('AGENTSPACE_STATE_DIR', 'AGENTSPACE_SSH_DIR', 'USERPROFILE', 'AWS_CONFIG_FILE', 'AWS_SHARED_CREDENTIALS_FILE',
  'AWS_PROFILE', 'AWS_DEFAULT_PROFILE', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN')
$savedEnv = @{}
foreach ($n in $envNames) { $savedEnv[$n] = [Environment]::GetEnvironmentVariable($n) }

# The lease fixtures: fixed times, so no case depends on the clock.
$now = '2026-11-17T12:00:00Z'
$created = '2026-11-17T08:00:00Z'
function Get-TestId([string]$Letter) { "0000000$Letter-0000-4000-8000-000000000000" }
function Get-TestAlias([string]$Letter) { "agentspace-test-bh-0000000$Letter" }

function New-ReadyRecord([string]$Letter, [string]$Expires) {
  [ordered]@{
    id = Get-TestId $Letter; project = 'test-bh'; state = 'ready'; platform = 'linux/arm64'; profile = 'agent-linux'
    size = 'm'; ttl_seconds = 28800; created_at = $created; expires_at = $Expires
    ssh = [ordered]@{ host_alias = Get-TestAlias $Letter; user = 'ubuntu' }
  }
}

function New-ReleasedRecord([string]$Letter, [string]$Expires) {
  $r = [ordered]@{
    id = Get-TestId $Letter; project = 'test-bh'; state = 'released'; platform = 'linux/arm64'; profile = 'agent-linux'
    size = 'm'; ttl_seconds = 28800; created_at = $created
  }
  if ($Expires) { $r['expires_at'] = $Expires }
  $r['released_at'] = '2026-11-17T09:00:00Z'
  $r
}

function Write-Leases($Records) {
  $text = ([ordered]@{ version = 0; leases = @($Records) } | ConvertTo-Json -Compress -Depth 10) + "`n"
  [IO.File]::WriteAllText((Join-Path $env:AGENTSPACE_STATE_DIR 'leases.json'), $text)
}

function Format-Utc([DateTime]$Time) { $Time.ToUniversalTime().ToString("yyyy-MM-dd\THH:mm:ss.fff\Z", $ci) }

function Write-Record([int]$ProcessId, [string]$StartedAt, [string]$LeaseId, [string]$Alias) {
  $text = [ordered]@{ pid = $ProcessId; process_started_at = $StartedAt; lease_id = $LeaseId; alias = $Alias } | ConvertTo-Json -Compress
  [IO.File]::WriteAllText((Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'), $text + "`n")
}

function Read-Record {
  $p = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'
  if (-not (Test-Path -LiteralPath $p)) { return $null }
  try { Get-Content -LiteralPath $p -Raw | ConvertFrom-Json -AsHashtable -DateKind String } catch { $null }
}

function Read-LogLines {
  $p = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.log'
  if (-not (Test-Path -LiteralPath $p)) { return , @() }
  try {
    # Shared the way a reader SHOULD share it (state.md, "The keeper's logs"), so a keeper that is writing is never blocked.
    $fs = [IO.File]::Open($p, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try { $text = [IO.StreamReader]::new($fs, [Text.UTF8Encoding]::new($false)).ReadToEnd() } finally { $fs.Dispose() }
    , @($text -split "`r?`n" | Where-Object { $_ -ne '' })
  } catch { , @() }
}

function Test-Alive([int]$ProcessId) { [bool](Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) }

function Get-CommandLine([int]$ProcessId) {
  $c = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId=$ProcessId" -ErrorAction SilentlyContinue
  if ($c) { [string]$c.CommandLine } else { '' }
}

function Get-ChildPids([int]$ProcessId) {
  @(Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId=$ProcessId" -ErrorAction SilentlyContinue | ForEach-Object { [int]$_.ProcessId })
}

# Every descendant of a process the check started, found by parent PID with the creation-time check.
function Get-DescendantPids([int]$ProcessId) {
  $all = @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue)
  $found = [System.Collections.Generic.List[object]]::new()
  $root = $all | Where-Object { $_.ProcessId -eq $ProcessId } | Select-Object -First 1
  if (-not $root) { return @() }
  $found.Add($root)
  for ($i = 0; $i -lt $found.Count; $i++) {
    $up = $found[$i]
    foreach ($c in $all) {
      if ($c.ParentProcessId -eq $up.ProcessId -and $c.ProcessId -ne $up.ProcessId -and $c.CreationDate -ge $up.CreationDate -and
        -not ($found | Where-Object { $_.ProcessId -eq $c.ProcessId })) { $found.Add($c) }
    }
  }
  @($found | Select-Object -Skip 1 | ForEach-Object { [int]$_.ProcessId })
}

# Waits until the condition holds, or the deadline passes. A process the check started, when given,
# ends the wait early once it has exited and the condition still does not hold.
function Wait-Until([scriptblock]$Condition, [int]$Seconds = 30, $Process = $null) {
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    if (& $Condition) { return $true }
    if ($null -ne $Process -and $Process.HasExited) { return [bool](& $Condition) }
    Start-Sleep -Milliseconds 100
  }
  [bool](& $Condition)
}

# A holder is a process whose command line is this run's stub with the holder's arguments (keeper.md,
# "The holder command"); the keeper's own command line holds the stub's path but not those arguments.
function Test-HolderLine([string]$CommandLine) {
  [bool]($CommandLine -and $CommandLine.Contains($temp) -and $CommandLine.Contains('ExitOnForwardFailure=yes'))
}

function Get-HolderProcesses {
  @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.ProcessId -ne $PID -and (Test-HolderLine $_.CommandLine) })
}

# Stops processes the check started, and their descendants: each child's parent is still the process
# above it, and it was created no earlier than that process. Every process is pinned by a handle and
# its start time checked against the snapshot before it is stopped, so a reused PID is never stopped.
# Leaves go first. A process whose DACL a case locked gets its rights back first. -SparingHolders
# leaves every holder, and what is below it, running: a keeper that is killed MUST leave its holder.
function Stop-OwnTree([int[]]$ProcessIds, [switch]$SparingHolders) {
  $all = @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue)
  $tree = [System.Collections.Generic.List[object]]::new()
  $seen = [System.Collections.Generic.HashSet[int]]::new()
  foreach ($id in $ProcessIds) {
    $root = $all | Where-Object { $_.ProcessId -eq $id } | Select-Object -First 1
    if ($root -and $seen.Add([int]$root.ProcessId)) { $tree.Add($root) }
  }
  for ($i = 0; $i -lt $tree.Count; $i++) {
    $up = $tree[$i]
    foreach ($c in $all) {
      if ($c.ParentProcessId -eq $up.ProcessId -and $c.ProcessId -ne $up.ProcessId -and $c.CreationDate -ge $up.CreationDate -and
        -not $seen.Contains([int]$c.ProcessId)) {
        if ($SparingHolders -and (Test-HolderLine ([string]$c.CommandLine))) { continue }
        $null = $seen.Add([int]$c.ProcessId)
        $tree.Add($c)
      }
    }
  }
  for ($i = $tree.Count - 1; $i -ge 0; $i--) {
    $m = $tree[$i]
    Unlock-Process $m.ProcessId
    try {
      $p = [Diagnostics.Process]::GetProcessById([int]$m.ProcessId)
      $null = $p.Handle
      if ([Math]::Abs(($p.StartTime.ToUniversalTime() - $m.CreationDate.ToUniversalTime()).TotalMilliseconds) -lt 1) { $p.Kill() }
    } catch { }
  }
}

# The run's own cleanup: every process whose command line holds this run's unique folder, and its
# children. Only the keepers and holders this run started can have that path.
function Get-RunProcesses {
  @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.ProcessId -ne $PID -and $_.CommandLine -and $_.CommandLine.Contains($temp) })
}

function Stop-RunProcesses {
  $ids = @(Get-RunProcesses | ForEach-Object { [int]$_.ProcessId })
  if ($ids.Count -gt 0) { Stop-OwnTree $ids }
  Wait-Until { (Get-RunProcesses).Count -eq 0 } 30 | Out-Null
}

# A stand-in process started by the check itself: it is foreign to the keeper. Returns its PID, its
# start time as the record writes it, and its children, the ping among them.
function Start-Stub([string]$Alias) {
  $p = Start-Process -FilePath $stub -ArgumentList @('-N', '-o', 'ExitOnForwardFailure=yes', $Alias) -PassThru -WindowStyle Hidden
  $null = $p.Handle
  Wait-Until {
    @(Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId=$($p.Id) AND Name='PING.EXE'" -ErrorAction SilentlyContinue).Count -gt 0
  } 30 | Out-Null
  $kids = @(Get-ChildPids $p.Id)
  [pscustomobject]@{ Pid = $p.Id; Started = (Format-Utc $p.StartTime); Children = $kids }
}

# True when the process and every child recorded for it are alive.
function Test-TreeAlive($Process) {
  (Test-Alive $Process.Pid) -and $Process.Children.Count -gt 0 -and @($Process.Children | Where-Object { Test-Alive $_ }).Count -eq $Process.Children.Count
}

# Starts the program under test with the arguments given, unchanged, and returns the process the check
# started. Output goes to files, never to a pipe, so a holder it starts cannot keep this run waiting.
function ConvertTo-ArgString([string[]]$Items) {
  ($Items | ForEach-Object { if ($_ -eq '' -or $_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
}

function Start-KeeperProcess([string[]]$KeeperArgs, [string]$Tag) {
  $out = Join-Path $env:AGENTSPACE_STATE_DIR "$Tag.out"
  $err = Join-Path $env:AGENTSPACE_STATE_DIR "$Tag.err"
  $p = Start-Process -FilePath $keeperCmd -ArgumentList (ConvertTo-ArgString $KeeperArgs) -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
  $null = $p.Handle
  $p
}

# Runs one pass of the keeper and returns its exit code, or -999 when it did not end in time.
function Invoke-Keeper([string]$At) {
  $a = @('-SshExe', $stub)
  if ($At) { $a += @('-Now', $At) }
  $p = Start-KeeperProcess $a 'keeper'
  if (-not $p.WaitForExit(180000)) {
    Stop-OwnTree $p.Id
    return -999
  }
  $p.ExitCode
}

# One case: a fresh state folder and home folder, the body, and a cleanup of every process the case
# started, in a finally.
function Invoke-Case([string]$Name, [string]$Spec, [scriptblock]$Body) {
  if ($Name -notlike $Case) { return }
  $dir = Join-Path $temp ([guid]::NewGuid().ToString('N'))
  $stateDir = Join-Path $dir 'state'
  $userHome = Join-Path $dir 'home'
  New-Item -ItemType Directory -Force -Path $stateDir, $userHome | Out-Null
  $env:AGENTSPACE_STATE_DIR = $stateDir
  $env:USERPROFILE = $userHome
  $env:AGENTSPACE_SSH_DIR = Join-Path $userHome 'ssh'
  $env:AWS_CONFIG_FILE = Join-Path $userHome 'no-aws-config'
  $env:AWS_SHARED_CREDENTIALS_FILE = Join-Path $userHome 'no-aws-credentials'
  foreach ($v in 'AWS_PROFILE', 'AWS_DEFAULT_PROFILE', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN') { [Environment]::SetEnvironmentVariable($v, $null) }
  $script:caseFails = 0
  $script:caseChecks = 0
  $script:caseSpec = $Spec
  try { & $Body } catch { Write-Result $false "${Name}: No unexpected error" "An error stopped the case: $($_.Exception.Message)" } finally { Stop-RunProcesses }
  if ($script:mut) {
    if ($script:caseFails -gt 0) { Write-Output "FAILS-AS-EXPECTED  $Name" } else { Write-Output "CANNOT-FAIL  $Name"; $script:failures++ }
  }
}

function Test-LogLine([string]$Line, [string]$Action, [string]$LeaseId, [string]$Alias, $ProcessId) {
  $Line -cmatch ('\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ ' + $Action + ' lease=' + [regex]::Escape($LeaseId) + ' alias=' + [regex]::Escape($Alias) + ' pid=' + $ProcessId + '(\s|\z)')
}

# The recorded start time MUST be exactly the process's creation time, in UTC, truncated to the
# millisecond. Process.StartTime carries the creation time to 100 ns, and Format-Utc truncates it
# (the "fff" format never rounds), so the two strings are compared exactly.
function Test-StartTimeExact([string]$Recorded, [string]$Actual) {
  $Recorded -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z\z' -and $Recorded -ceq $Actual
}

# The checks after a stop: the holder and its tree are gone, the record is gone, one stopped line.
function Assert-Stopped([string]$Name, $Holder, [string]$Letter) {
  $gone = Wait-Until { -not (Test-Alive $Holder.Pid) } 30
  Write-Result $gone "${Name}: The holder process is gone"
  $kidsGone = Wait-Until { @($Holder.Children | Where-Object { Test-Alive $_ }).Count -eq 0 } 30
  Write-Result ($Holder.Children.Count -gt 0 -and $kidsGone) "${Name}: The holder's child process is gone too"
  Write-Result ($null -eq (Read-Record) -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'))) "${Name}: The record is removed"
  $log = Read-LogLines
  Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'stopped' (Get-TestId $Letter) (Get-TestAlias $Letter) $Holder.Pid)) "${Name}: One stopped line is logged with the lease, alias and PID"
}

# A running holder that the keeper would trust: a stub with the holder's arguments, and its record.
function Start-TrustedHolder([string]$Letter) {
  $h = Start-Stub (Get-TestAlias $Letter)
  Write-Record $h.Pid $h.Started (Get-TestId $Letter) (Get-TestAlias $Letter)
  $h
}

# ---- -Watch helpers ----

# A stand-in for ssh.exe that fails at once, and one that fails at once except its third start, which
# lives about 10 seconds first. Each counts its starts in a file beside itself, so no environment
# variable has to reach it. Both are written into the case's own state folder and exit by themselves.
# -EveryLife <n> makes every start live about n seconds instead (a ping of n + 1 echoes).
function New-FailStub([bool]$LongThird, [int]$EveryLife = 0) {
  $path = Join-Path $env:AGENTSPACE_STATE_DIR 'ssh-fail.cmd'
  $lines = @('@echo off', 'set "f=%~dp0starts.txt"', 'set n=0', 'if exist "%f%" set /p n=<"%f%"', 'set /a n+=1', '>"%f%" echo %n%',
    'echo stub-fail start %n% %*')
  if ($LongThird) { $lines += 'if %n%==3 ping -n 11 127.0.0.1 >nul' }
  if ($EveryLife -gt 0) { $lines += "ping -n $($EveryLife + 1) 127.0.0.1 >nul" }
  $lines += 'exit /b 1'
  [IO.File]::WriteAllText($path, (($lines -join "`r`n") + "`r`n"))
  $path
}

function Get-StartCount {
  $starts = Join-Path $env:AGENTSPACE_STATE_DIR 'starts.txt'
  if (-not (Test-Path -LiteralPath $starts)) { return 0 }
  try { [int](Get-Content -LiteralPath $starts -TotalCount 1) } catch { 0 }
}

# Starts the keeper with -Watch and returns the process the check started. The release grace is 95
# seconds by default, which would stretch every case that restarts a holder. Unless the case names
# -ReleaseGraceSeconds itself, or asks for the default with -DefaultGrace, the run gets a grace of 1
# second, shorter than every backoff these cases use. -Tag names the output files, so that two
# keepers started in one case do not share them.
function Start-Watch([string[]]$Extra, [string]$Ssh, [switch]$DefaultGrace, [string]$Tag = 'watch') {
  if (-not $DefaultGrace -and -not ($Extra -contains '-ReleaseGraceSeconds')) { $Extra = @($Extra) + @('-ReleaseGraceSeconds', '1') }
  Start-KeeperProcess (@('-SshExe', $Ssh, '-Watch') + $Extra) $Tag
}

# Waits for the process to end on its own; one that does not within the limit is stopped and gives -999.
function Wait-Watch($Process, [int]$Seconds = 240) {
  if (-not $Process.WaitForExit($Seconds * 1000)) { Stop-OwnTree $Process.Id; return -999 }
  $Process.ExitCode
}

# The times of the log lines whose action is the given one, in order.
function Get-ActionTimes($Log, [string]$Action) {
  @($Log | Where-Object { $_ -cmatch ('\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ ' + $Action + '(\s|\z)') } | ForEach-Object { Get-LogTime $_ })
}

# The n of every "grace seconds=<n>" line, in order.
function Get-GraceSeconds($Log) {
  @($Log | ForEach-Object { if ($_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ grace seconds=(\d+)\z') { [int]$Matches[1] } })
}

# Stops a keeper the check started, once the case has seen what it needs.
function Stop-Watch($Process) { if (-not $Process.HasExited) { Stop-OwnTree $Process.Id } }

function Get-LogTime([string]$Line) {
  if ($Line -cmatch '\A(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) ') {
    [DateTime]::ParseExact($Matches[1], "yyyy-MM-dd'T'HH:mm:ss'Z'", $ci, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal)
  }
}

function Get-BackoffSeconds($Log) {
  @($Log | ForEach-Object { if ($_ -cmatch '\bbackoff seconds=(\d+)(\s|\z)') { [int]$Matches[1] } })
}

# Kills the keeper the check started, by the PID of the process it started and that process's own
# children (the program under test may be a wrapper over the keeper), and spares every holder, so
# that the holder it started lives on, as it does when the keeper is killed for real. The process
# object holds a handle, which pins the PID. Returns true when it was killed.
function Stop-KeeperOnly($Process) {
  try {
    $null = $Process.Handle
    if ($Process.HasExited) { return $false }
    Stop-OwnTree $Process.Id -SparingHolders
    return $true
  } catch { return $false }
}

# The "another keeper is running" lines of a log.
function Get-OtherKeeperLines($Log) {
  @($Log | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ another keeper is running pid=(\d+|-)\z' })
}

# True when the line's PID field is the PID of a process the first keeper runs as, or - for a PID that
# could not be read. The first keeper is the process the check started or one of its descendants.
function Test-OtherKeeperPid([string]$Line, [int[]]$KeeperPids) {
  $Line -cmatch 'pid=(\d+|-)\z' -and ($Matches[1] -ceq '-' -or $KeeperPids -contains [int]$Matches[1])
}

# Returns 'valid', 'invalid' (rejected by the schema) or 'error: ...' (the schema or the JSON is broken).
function Test-Document([string]$Json, [string]$SchemaPath) {
  try {
    if (Test-Json -Json $Json -SchemaFile $SchemaPath -ErrorAction Stop) { return 'valid' }
    return 'invalid'
  } catch {
    $m = $_.Exception.Message
    if ($m -like '*is not valid with the schema*') { return 'invalid' }
    return "error: $m"
  }
}

# state.md, "The keeper's files": UTF-8 without a byte order mark, compact JSON with no whitespace
# outside strings, followed by one line feed.
function Test-KeeperJsonBytes([byte[]]$Bytes) {
  if ($null -eq $Bytes -or $Bytes.Length -lt 3) { return $false }
  if ($Bytes[0] -eq 0xEF -and $Bytes[1] -eq 0xBB -and $Bytes[2] -eq 0xBF) { return $false }
  if ($Bytes[$Bytes.Length - 1] -ne 10) { return $false }
  $text = [Text.UTF8Encoding]::new($false).GetString($Bytes, 0, $Bytes.Length - 1)
  $inString = $false
  $escaped = $false
  foreach ($ch in $text.ToCharArray()) {
    if ($inString) {
      if ($escaped) { $escaped = $false } elseif ($ch -eq '\') { $escaped = $true } elseif ($ch -eq '"') { $inString = $false }
    } else {
      if ($ch -eq '"') { $inString = $true } elseif ($ch -eq ' ' -or $ch -eq "`t" -or $ch -eq "`r" -or $ch -eq "`n") { return $false }
    }
  }
  $true
}

function Read-CapturedJson([byte[]]$Bytes) {
  try { [Text.UTF8Encoding]::new($false).GetString($Bytes) | ConvertFrom-Json -AsHashtable -DateKind String } catch { $null }
}

$farFuture = '2099-01-01T00:00:00Z'

try {
  # Idle.
  Invoke-Case 'Idle' 'keeper.md#idle' {
    $recs = @((New-ReleasedRecord 'a' '2026-11-17T20:00:00Z'))
    if ($script:mut) { $recs += New-ReadyRecord 'b' '2026-11-17T13:00:00Z' }
    Write-Leases $recs
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Idle: The pass exits 0' "The exit code was $code."
    Write-Result ((Get-HolderProcesses).Count -eq 0) 'Idle: No holder is started'
    Write-Result ($null -eq (Read-Record)) 'Idle: No record is written'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and $log[0] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle lease=- alias=- pid=-\z') 'Idle: One idle line is logged, naming no holder'
  }

  # Start, and the holder's output.
  Invoke-Case 'Start' 'keeper.md#starting-a-holder' {
    $r = New-ReadyRecord 'a' '2026-11-17T13:00:00Z'
    if ($script:mut) { $r = New-ReleasedRecord 'a' '2026-11-17T13:00:00Z' }
    Write-Leases @($r)
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Start: The pass exits 0' "The exit code was $code."
    $rec = Read-Record
    Write-Result ($null -ne $rec) 'Start: The record is written'
    $recPid = if ($rec) { [int]$rec['pid'] } else { -1 }
    Write-Result ($rec -and $rec['lease_id'] -ceq (Get-TestId 'a') -and $rec['alias'] -ceq (Get-TestAlias 'a')) 'Start: The record names the lease ID and the alias'
    $alive = $recPid -gt 0 -and (Test-Alive $recPid)
    Write-Result $alive 'Start: The recorded PID is a running process after the pass exits'
    $startTime = if ($alive) { Format-Utc (Get-Process -Id $recPid).StartTime } else { '' }
    Write-Result ($alive -and $rec -and (Test-StartTimeExact $rec['process_started_at'] $startTime)) 'Start: The record holds exactly that process''s start time, UTC truncated to the millisecond' "The record has '$(if ($rec) { $rec['process_started_at'] })' and the process started at '$startTime'."
    $cl = if ($alive) { Get-CommandLine $recPid } else { '' }
    $tail = '-N -o ExitOnForwardFailure=yes ' + [regex]::Escape((Get-TestAlias 'a')) + '"?\s*\z'
    Write-Result ($cl.Contains($stub) -and $cl -cmatch $tail) 'Start: The holder runs -SshExe -N -o ExitOnForwardFailure=yes <alias>' "The holder's command line is '$cl'."
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'started' (Get-TestId 'a') (Get-TestAlias 'a') $recPid)) 'Start: One started line is logged with the lease, alias and PID'
    $sshLog = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.log'
    $hasOut = Wait-Until { (Test-Path -LiteralPath $sshLog) -and ((Get-Content -LiteralPath $sshLog -Raw) -match 'stub-started') } 30
    Write-Result $hasOut 'Start: The holder''s output is appended to board-holder-ssh.log' 'The log never held the stub''s line.'
  }

  # Keep.
  Invoke-Case 'Keep' 'keeper.md#keeping-a-holder' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    if ($script:mut) { Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z')) }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Keep: The pass exits 0' "The exit code was $code."
    Write-Result (Test-Alive $h.Pid) 'Keep: The running holder is still running'
    $rec = Read-Record
    Write-Result ($rec -and [int]$rec['pid'] -eq $h.Pid -and $rec['process_started_at'] -ceq $h.Started) 'Keep: The record still names the same PID and start time'
    Write-Result ((Get-HolderProcesses | Where-Object { $_.ProcessId -ne $h.Pid }).Count -eq 0) 'Keep: No second holder is started'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'kept' (Get-TestId 'a') (Get-TestAlias 'a') $h.Pid)) 'Keep: One kept line is logged with the lease, alias and PID'
  }

  # Stop on release.
  Invoke-Case 'Stop on release' 'keeper.md#stopping-a-holder' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    if (-not $script:mut) { Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z')) }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Stop on release: The pass exits 0' "The exit code was $code."
    Assert-Stopped 'Stop on release' $h 'a'
  }

  # Stop on expiry, through -Now.
  Invoke-Case 'Stop on expiry' 'keeper.md#which-leases-are-ready' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $at = if ($script:mut) { '2026-11-17T12:59:59Z' } else { '2026-11-17T13:00:01Z' }
    $code = Invoke-Keeper $at
    Write-Result ($code -eq 0) 'Stop on expiry: The pass exits 0' "The exit code was $code."
    Assert-Stopped 'Stop on expiry' $h 'a'
  }

  # Stop at the expiry second: the lease is ready only while expires_at is after -Now.
  Invoke-Case 'Stop at the expiry second' 'keeper.md#which-leases-are-ready' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $at = if ($script:mut) { '2026-11-17T12:59:59Z' } else { '2026-11-17T13:00:00Z' }
    $code = Invoke-Keeper $at
    Write-Result ($code -eq 0) 'Stop at the expiry second: The pass exits 0' "The exit code was $code."
    Assert-Stopped 'Stop at the expiry second' $h 'a'
  }

  # Stop when the lease is gone.
  Invoke-Case 'Stop when the lease is gone' 'keeper.md#stopping-a-holder' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    if (-not $script:mut) { Write-Leases @() }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Stop when the lease is gone: The pass exits 0' "The exit code was $code."
    Assert-Stopped 'Stop when the lease is gone' $h 'a'
  }

  # A stop with a foreign process present: the keeper's own holder for alias 'a' and its tree are
  # stopped, and a process it did not start, with the same alias, is left running with its child.
  Invoke-Case 'Stop leaves a foreign process alone' 'keeper.md#stopping-a-holder' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $f = Start-Stub (Get-TestAlias 'a')
    if ($script:mut) { Write-Record $f.Pid $f.Started (Get-TestId 'a') (Get-TestAlias 'a') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Stop leaves a foreign process alone: The pass exits 0' "The exit code was $code."
    Assert-Stopped 'Stop leaves a foreign process alone' $h 'a'
    Write-Result (Test-TreeAlive $f) 'Stop leaves a foreign process alone: The foreign process and its child are still alive'
  }

  # A dead recorded PID: start afresh.
  Invoke-Case 'Dead PID' 'keeper.md#trusting-the-record' {
    $r = New-ReadyRecord 'a' '2026-11-17T13:00:00Z'
    if ($script:mut) { $r = New-ReleasedRecord 'a' '2026-11-17T13:00:00Z' }
    Write-Leases @($r)
    $dead = Start-Stub (Get-TestAlias 'a')
    Write-Record $dead.Pid $dead.Started (Get-TestId 'a') (Get-TestAlias 'a')
    Stop-OwnTree $dead.Pid
    Wait-Until { -not (Test-Alive $dead.Pid) } 30 | Out-Null
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Dead PID: The pass exits 0' "The exit code was $code."
    $rec = Read-Record
    $newPid = if ($rec) { [int]$rec['pid'] } else { -1 }
    Write-Result ($rec -and $newPid -ne $dead.Pid -and (Test-Alive $newPid)) 'Dead PID: A new holder is running and the record names it'
    Write-Result ($rec -and $rec['lease_id'] -ceq (Get-TestId 'a')) 'Dead PID: The record names the lease'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'started' (Get-TestId 'a') (Get-TestAlias 'a') $newPid)) 'Dead PID: One started line is logged'
  }

  # A reused PID, lease ready: the record's start time is wrong, so the process is not the keeper's.
  Invoke-Case 'Reused PID, lease ready' 'keeper.md#trusting-the-record' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub (Get-TestAlias 'a')
    $wrong = Format-Utc ([DateTime]::Parse($f.Started, $ci, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal).AddHours(-1))
    Write-Record $f.Pid $(if ($script:mut) { $f.Started } else { $wrong }) (Get-TestId 'a') (Get-TestAlias 'a')
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Reused PID, lease ready: The pass exits 0' "The exit code was $code."
    Write-Result ((Test-Alive $f.Pid) -and @($f.Children | Where-Object { Test-Alive $_ }).Count -eq $f.Children.Count) 'Reused PID, lease ready: The foreign process and its child are still alive'
    $rec = Read-Record
    Write-Result ($rec -and [int]$rec['pid'] -ne $f.Pid -and (Test-Alive ([int]$rec['pid']))) 'Reused PID, lease ready: The keeper started a holder of its own'
  }

  # A reused PID, lease released: nothing of the keeper's to stop; the untrusted record is removed.
  Invoke-Case 'Reused PID, lease released' 'keeper.md#trusting-the-record' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub (Get-TestAlias 'a')
    $wrong = Format-Utc ([DateTime]::Parse($f.Started, $ci, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal).AddHours(-1))
    Write-Record $f.Pid $(if ($script:mut) { $f.Started } else { $wrong }) (Get-TestId 'a') (Get-TestAlias 'a')
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Reused PID, lease released: The pass exits 0' "The exit code was $code."
    Write-Result ((Test-Alive $f.Pid) -and @($f.Children | Where-Object { Test-Alive $_ }).Count -eq $f.Children.Count) 'Reused PID, lease released: The foreign process and its child are still alive'
    Write-Result ((Get-HolderProcesses).Count -eq 1) 'Reused PID, lease released: No holder is started'
    Write-Result ($null -eq (Read-Record)) 'Reused PID, lease released: The record that does not name the process is removed'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and $log[0] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle lease=- alias=- pid=-\z') 'Reused PID, lease released: The pass is idle'
  }

  # The start time matches but the command line does not.
  Invoke-Case 'Command line differs' 'keeper.md#trusting-the-record' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub $(if ($script:mut) { Get-TestAlias 'a' } else { Get-TestAlias 'f' })
    Write-Record $f.Pid $f.Started (Get-TestId 'a') (Get-TestAlias 'a')
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Command line differs: The pass exits 0' "The exit code was $code."
    Write-Result ((Test-Alive $f.Pid) -and @($f.Children | Where-Object { Test-Alive $_ }).Count -eq $f.Children.Count) 'Command line differs: The process with the matching start time but another command line is still alive'
    Write-Result ($null -eq (Read-Record)) 'Command line differs: The record that does not name the process is removed'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and $log[0] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle lease=- alias=- pid=-\z') 'Command line differs: The pass is idle'
  }

  # A process the keeper did not start, naming the lease alias, no record: lease released.
  Invoke-Case 'Foreign process, lease released' 'keeper.md#trusting-the-record' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub (Get-TestAlias 'a')
    if ($script:mut) { Write-Record $f.Pid $f.Started (Get-TestId 'a') (Get-TestAlias 'a') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Foreign process, lease released: The pass exits 0' "The exit code was $code."
    Write-Result ((Test-Alive $f.Pid) -and @($f.Children | Where-Object { Test-Alive $_ }).Count -eq $f.Children.Count) 'Foreign process, lease released: The foreign process and its child are still alive'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and $log[0] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle(\s|\z)') 'Foreign process, lease released: The pass is idle'
  }

  # The same, lease ready: the keeper starts its own, never adopting the foreign one.
  Invoke-Case 'Foreign process, lease ready' 'keeper.md#trusting-the-record' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub (Get-TestAlias 'a')
    if ($script:mut) { Write-Record $f.Pid $f.Started (Get-TestId 'a') (Get-TestAlias 'a') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Foreign process, lease ready: The pass exits 0' "The exit code was $code."
    Write-Result (Test-TreeAlive $f) 'Foreign process, lease ready: The foreign process and its child are still alive'
    $rec = Read-Record
    Write-Result ($rec -and [int]$rec['pid'] -ne $f.Pid -and (Test-Alive ([int]$rec['pid']))) 'Foreign process, lease ready: The record names a holder the keeper started, not the foreign process'
  }

  # The ready lease that expires last.
  Invoke-Case 'Last expiry' 'keeper.md#which-lease' {
    $c = if ($script:mut) { '2026-11-17T14:30:00Z' } else { '2026-11-17T14:00:00Z' }
    $b = if ($script:mut) { '2026-11-17T14:00:00Z' } else { '2026-11-17T15:00:00Z' }
    Write-Leases @(
      (New-ReadyRecord 'a' '2026-11-17T13:00:00Z'),
      (New-ReadyRecord 'b' $b),
      (New-ReadyRecord 'c' $c),
      (New-ReleasedRecord 'd' '2026-11-17T20:00:00Z'),
      (New-ReadyRecord 'e' '2026-11-17T11:00:00Z'))
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Last expiry: The pass exits 0' "The exit code was $code."
    $rec = Read-Record
    Write-Result ($rec -and $rec['lease_id'] -ceq (Get-TestId 'b') -and $rec['alias'] -ceq (Get-TestAlias 'b')) 'Last expiry: The record names the ready lease that expires last, not a released or expired one'
    $recPid = if ($rec) { [int]$rec['pid'] } else { -1 }
    $cl = if ($recPid -gt 0) { Get-CommandLine $recPid } else { '' }
    Write-Result ($cl.Contains((Get-TestAlias 'b')) -and -not $cl.Contains((Get-TestAlias 'a')) -and -not $cl.Contains((Get-TestAlias 'c'))) 'Last expiry: The holder runs for that lease''s alias'
    Write-Result ((Get-HolderProcesses).Count -eq 1) 'Last expiry: One holder only'
  }

  # Lease state that cannot be read.
  Invoke-Case 'Unreadable lease state' 'keeper.md#the-order-of-a-pass' {
    if ($script:mut) { Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z')) }
    else { [IO.File]::WriteAllText((Join-Path $env:AGENTSPACE_STATE_DIR 'leases.json'), '{ this is not json') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 1) 'Unreadable lease state: The pass exits 1' "The exit code was $code."
    Write-Result ((Get-HolderProcesses).Count -eq 0) 'Unreadable lease state: No holder is started'
    Write-Result ($null -eq (Read-Record)) 'Unreadable lease state: No record is written'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'failed' '-' '-' '-') -and $log[0] -cmatch ' reason=Lease state could not be read:') 'Unreadable lease state: One failed line says the lease state could not be read'
  }

  # Lease state that cannot be read, with a trusted holder running: the holder and its record are
  # left as they are.
  Invoke-Case 'Unreadable lease state, holder running' 'keeper.md#the-order-of-a-pass' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $recordPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'
    $before = [IO.File]::ReadAllText($recordPath)
    if ($script:mut) { Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z')) }
    else { [IO.File]::WriteAllText((Join-Path $env:AGENTSPACE_STATE_DIR 'leases.json'), '{ this is not json') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 1) 'Unreadable lease state, holder running: The pass exits 1' "The exit code was $code."
    Write-Result (Test-TreeAlive $h) 'Unreadable lease state, holder running: The holder and its child are still alive'
    Write-Result ((Test-Path -LiteralPath $recordPath) -and [IO.File]::ReadAllText($recordPath) -ceq $before) 'Unreadable lease state, holder running: The record is unchanged'
    Write-Result ((Get-HolderProcesses | Where-Object { $_.ProcessId -ne $h.Pid }).Count -eq 0) 'Unreadable lease state, holder running: No other holder is started'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'failed' '-' '-' '-') -and $log[0] -cmatch ' reason=\S') 'Unreadable lease state, holder running: One failed line with a reason is logged'
  }

  # A trusted holder that the keeper cannot open with the right to stop it (a holder started
  # elevated, met by a pass that is not): the DACL of the check's own stub denies PROCESS_TERMINATE.
  # The pass MUST log failed and keep the record, so the next pass tries again.
  Invoke-Case 'Holder that cannot be opened is kept' 'keeper.md#stopping-a-holder' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $recordPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'
    $before = [IO.File]::ReadAllText($recordPath)
    if (-not $script:mut) {
      try { Lock-Process $h.Pid } catch { Write-Skipped 'Holder that cannot be opened is kept' "The stub's access control list could not be set: $($_.Exception.Message)"; return }
    }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Holder that cannot be opened is kept: The pass exits 0' "The exit code was $code."
    Write-Result (Test-TreeAlive $h) 'Holder that cannot be opened is kept: The holder and its child are still alive'
    Write-Result ((Test-Path -LiteralPath $recordPath) -and [IO.File]::ReadAllText($recordPath) -ceq $before) 'Holder that cannot be opened is kept: The record is kept unchanged'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'failed' (Get-TestId 'a') (Get-TestAlias 'a') $h.Pid) -and
      $log[0] -cmatch ' reason=The holder could not be opened to stop it') 'Holder that cannot be opened is kept: One failed line says the holder could not be opened'
    Write-Result (-not ($log -cmatch ' stopped ')) 'Holder that cannot be opened is kept: No stopped line is logged'
  }

  # Another pass holds board-holder.lock: the pass is skipped, says so, and exits 0. A ready lease
  # is there, so a pass that ran would start a holder.
  Invoke-Case 'Lock busy' 'keeper.md#locks' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $lockPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.lock'
    $held = $null
    if (-not $script:mut) {
      $held = [IO.FileStream]::new($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    }
    try { $code = Invoke-Keeper $now } finally { if ($held) { $held.Dispose() } }
    Write-Result ($code -eq 0) 'Lock busy: The pass exits 0' "The exit code was $code."
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and $log[0] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ skipped lease=- alias=- pid=- reason=locked\z') 'Lock busy: One line "skipped lease=- alias=- pid=- reason=locked" is logged'
    Write-Result ((Get-HolderProcesses).Count -eq 0) 'Lock busy: No holder is started'
    Write-Result ($null -eq (Read-Record)) 'Lock busy: No record is written'
  }

  # A -Now that is not a time: a usage error, exit 2, and nothing is done.
  Invoke-Case 'Bad -Now' 'keeper.md#usage-errors' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $at = if ($script:mut) { $now } else { '2026-11-17T12:00:00' }
    $code = Invoke-Keeper $at
    Write-Result ($code -eq 2) 'Bad -Now: The pass exits 2' "The exit code was $code."
    Write-Result ((Get-HolderProcesses).Count -eq 0) 'Bad -Now: No holder is started'
    Write-Result ($null -eq (Read-Record)) 'Bad -Now: No record is written'
    $errPath = Join-Path $env:AGENTSPACE_STATE_DIR 'keeper.err'
    $errText = if (Test-Path -LiteralPath $errPath) { Get-Content -LiteralPath $errPath -Raw } else { '' }
    Write-Result ([bool]($errText -match 'Usage:')) 'Bad -Now: The usage lines are written to stderr'
  }

  # Rotation: a board-holder-ssh.log just over 1 MiB is renamed to board-holder-ssh.log.1 by the
  # next pass, which logs one rotated line, and the holder it starts writes a new, small log.
  Invoke-Case 'Rotation of board-holder-ssh.log' 'keeper.md#the-holders-log' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $sshLog = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.log'
    $size = if ($script:mut) { 1MB - 100 } else { 1MB + 100 }
    $fs = [IO.File]::Create($sshLog)
    try { $fs.SetLength($size) } finally { $fs.Dispose() }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Rotation: The pass exits 0' "The exit code was $code."
    $old = [IO.FileInfo]::new("$sshLog.1")
    Write-Result ($old.Exists -and $old.Length -eq $size) 'Rotation: The old log is now board-holder-ssh.log.1, whole'
    $small = Wait-Until { $f = [IO.FileInfo]::new($sshLog); $f.Exists -and $f.Length -lt 4096 -and ((Get-Content -LiteralPath $sshLog -Raw) -match 'stub-started') } 30
    Write-Result $small 'Rotation: The new holder writes a new board-holder-ssh.log, under 4 KB'
    $rotated = @(Read-LogLines | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ rotated lease=- alias=- pid=-\z' })
    Write-Result ($rotated.Count -eq 1) "Rotation: One line ""rotated lease=- alias=- pid=-"" is logged (got $($rotated.Count))"
  }

  # The JSON files a pass writes, checked against the state schemas (state.md, Q13). The launch
  # request, its claim and its result live only while a holder is being started, so a watcher keeps
  # a copy of each as it appears; a pass that the watcher missed is run again, from nothing.
  Invoke-Case 'The files a keeper run writes match the state schemas' 'state.md#the-keepers-files' {
    $leaseFile = Join-Path $env:AGENTSPACE_STATE_DIR 'leases.json'
    Write-Leases @($(if ($script:mut) { New-ReleasedRecord 'a' '2026-11-17T13:00:00Z' } else { New-ReadyRecord 'a' '2026-11-17T13:00:00Z' }))
    Write-Result ((Test-Document ([IO.File]::ReadAllText($leaseFile)) (Join-Path $schemaDir 'leases.state.schema.json')) -eq 'valid') 'The fixture leases.json matches leases.state.schema.json'
    $caught = @{}
    $code = $null
    for ($attempt = 1; $attempt -le 4; $attempt++) {
      $cap = [AgentspaceTest.FileCapture]::new($env:AGENTSPACE_STATE_DIR)
      $cap.Start()
      try { $code = Invoke-Keeper $now } finally { $files = $cap.Stop() }
      foreach ($k in @($files.Keys)) { if (-not $caught.ContainsKey($k)) { $caught[$k] = $files[$k] } }
      $names = @($caught.Keys)
      $haveAll = @($names | Where-Object { $_ -match '\Aboard-holder-launch\.[0-9a-fA-F-]+\.json\z' }).Count -ge 1 -and
        @($names | Where-Object { $_ -match '\Aboard-holder-launch\.[0-9a-fA-F-]+\.result\.json\z' }).Count -ge 1 -and
        @($names | Where-Object { $_ -match '\Aboard-holder-launch\.[0-9a-fA-F-]+\.claim\z' }).Count -ge 1
      if ($haveAll -or $script:mut) { break }
      # Missed: stop the holder and drop its record, so that the next pass starts one again.
      Stop-RunProcesses
      Remove-Item -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json') -ErrorAction SilentlyContinue
    }
    Write-Result ($code -eq 0) 'The pass that wrote the files exits 0' "The exit code was $code."

    $recordPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'
    $recordBytes = if (Test-Path -LiteralPath $recordPath) { [IO.File]::ReadAllBytes($recordPath) } else { $null }
    Write-Result ($null -ne $recordBytes) 'The holder record board-holder.json was written' 'The pass wrote no board-holder.json.'
    if ($null -ne $recordBytes) {
      Write-Result ((Test-Document ([Text.UTF8Encoding]::new($false).GetString($recordBytes)) (Join-Path $schemaDir 'board-holder.state.schema.json')) -eq 'valid') 'The holder record matches board-holder.state.schema.json' "The result was: $(Test-Document ([Text.UTF8Encoding]::new($false).GetString($recordBytes)) (Join-Path $schemaDir 'board-holder.state.schema.json'))."
      Write-Result (Test-KeeperJsonBytes $recordBytes) 'The holder record is UTF-8 without a byte order mark, compact, with one line feed at the end'
    }

    $reqName = @($caught.Keys | Where-Object { $_ -match '\Aboard-holder-launch\.[0-9a-fA-F-]+\.json\z' }) | Select-Object -First 1
    $resName = @($caught.Keys | Where-Object { $_ -match '\Aboard-holder-launch\.[0-9a-fA-F-]+\.result\.json\z' }) | Select-Object -First 1
    $claimName = @($caught.Keys | Where-Object { $_ -match '\Aboard-holder-launch\.[0-9a-fA-F-]+\.claim\z' }) | Select-Object -First 1
    Write-Result ([bool]$reqName) 'A launch request board-holder-launch.<id>.json was written while the holder was started' 'No launch request was seen in 4 passes.'
    Write-Result ([bool]$resName) 'A launch result board-holder-launch.<id>.result.json was written while the holder was started' 'No launch result was seen in 4 passes.'
    Write-Result ([bool]$claimName) 'A launch claim board-holder-launch.<id>.claim was written while the holder was started' 'No launch claim was seen in 4 passes.'

    if ($reqName) {
      $reqText = [Text.UTF8Encoding]::new($false).GetString($caught[$reqName])
      $r = Test-Document $reqText (Join-Path $schemaDir 'board-holder-launch.state.schema.json')
      Write-Result ($r -eq 'valid') 'The launch request matches board-holder-launch.state.schema.json' "The result was: $r."
      Write-Result (Test-KeeperJsonBytes $caught[$reqName]) 'The launch request is UTF-8 without a byte order mark, compact, with one line feed at the end'
      $req = Read-CapturedJson $caught[$reqName]
      $ok = [bool]$req -and
        [string]::Equals($req['log'], (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.log'), [StringComparison]::OrdinalIgnoreCase) -and
        [string]::Equals($req['directory'], $env:USERPROFILE, [StringComparison]::OrdinalIgnoreCase) -and
        [string]::Equals($req['result'], (Join-Path $env:AGENTSPACE_STATE_DIR ($reqName -replace '\.json$', '.result.json')), [StringComparison]::OrdinalIgnoreCase)
      Write-Result $ok 'The launch request names full paths: the holder''s log, the home folder as the working folder, and its own result file' "The request is: $reqText"
    }
    if ($resName) {
      $resText = [Text.UTF8Encoding]::new($false).GetString($caught[$resName])
      $r = Test-Document $resText (Join-Path $schemaDir 'board-holder-launch-result.state.schema.json')
      Write-Result ($r -eq 'valid') 'The launch result matches board-holder-launch-result.state.schema.json' "The result was: $r."
      Write-Result (Test-KeeperJsonBytes $caught[$resName]) 'The launch result is UTF-8 without a byte order mark, compact, with one line feed at the end'
      $res = Read-CapturedJson $caught[$resName]
      $rec = Read-Record
      Write-Result ([bool]$res -and $rec -and $res.ContainsKey('pid') -and [int]$res['pid'] -eq [int]$rec['pid'] -and $res['process_started_at'] -ceq $rec['process_started_at']) 'The launch result names the holder that the record names, by PID and start time'
    }
    if ($claimName) {
      $claimBytes = $caught[$claimName]
      $claimText = [Text.UTF8Encoding]::new($false).GetString($claimBytes)
      Write-Result ($claimBytes.Length -ge 3 -and -not ($claimBytes[0] -eq 0xEF -and $claimBytes[1] -eq 0xBB -and $claimBytes[2] -eq 0xBF) -and $claimText -cmatch '\A(launcher \d+|abandoned)\n\z') 'The launch claim is one line, "launcher <pid>", in UTF-8 without a byte order mark' "The claim is: $claimText"
    }
    if ($recordBytes) {
      $left = @(Get-ChildItem -LiteralPath $env:AGENTSPACE_STATE_DIR -Filter 'board-holder-launch.*' -Force -ErrorAction SilentlyContinue)
      Write-Result ($left.Count -eq 0) 'The pass that wrote the launch files removed all of them' "Left behind: $(($left | ForEach-Object { $_.Name }) -join ', ')"
    }
  }

  # -Watch, backoff: a holder that fails at once is restarted after 1, 2, 4, 4 seconds, never faster.
  Invoke-Case 'Watch: backoff doubles and is capped' 'keeper.md#fast-failures-and-backoff' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $false
    $cap = if ($script:mut) { '64' } else { '4' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '12', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', $cap) $fail
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch backoff: The loop ends on its own with exit 0' "The exit code was $code."
    $b = Get-BackoffSeconds (Read-LogLines)
    Write-Result ($b.Count -ge 4 -and (($b[0..3]) -join ',') -ceq '1,2,4,4') "Watch backoff: The first four backoff lines say seconds 1, 2, 4, 4 (got $($b -join ','))"
    Write-Result ($b.Count -ge 1 -and @($b | Where-Object { $_ -gt 4 }).Count -eq 0) 'Watch backoff: No wait is longer than BackoffMaxSeconds'
  }

  # -Watch, backoff reset: a holder that lived past the fast-failure threshold clears the doubling.
  # The third start lives about 10 seconds and the threshold is 2 (-FastFailureSeconds), so a look
  # finds it alive past the threshold with a wide margin. Its kept passes take about 10 of the 20 passes.
  Invoke-Case 'Watch: backoff resets after a long-lived holder' 'keeper.md#fast-failures-and-backoff' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $true
    $threshold = if ($script:mut) { '60' } else { '2' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '20', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '4', '-FastFailureSeconds', $threshold) $fail
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch backoff reset: The loop ends on its own with exit 0' "The exit code was $code."
    $n = Get-StartCount
    Write-Result ($n -ge 4) "Watch backoff reset: The holder was started at least four times (started $n)"
    $b = Get-BackoffSeconds (Read-LogLines)
    Write-Result ($b.Count -ge 3 -and $b[0] -eq 1 -and $b[1] -eq 2) "Watch backoff reset: The waits before the long-lived start are 1 then 2 (got $($b -join ','))"
    Write-Result ($b.Count -ge 3 -and $b[2] -eq 1) 'Watch backoff reset: The first wait after the long-lived holder is back at the base, 1'
  }

  # -Watch, a holder that dies unproven: every start lives about 6 seconds, past the 4-second
  # interval but short of the 7-second threshold. A look finds each one alive but unproven, and the
  # next look finds it dead more than 7 seconds after its start. Only proof of life resets the
  # count, so each death MUST count as a fast failure, and the backoff lines appear and double.
  Invoke-Case 'Watch: a holder that dies unproven backs off' 'keeper.md#fast-failures-and-backoff' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $false 6
    $threshold = if ($script:mut) { '3' } else { '7' }
    $p = Start-Watch @('-IntervalSeconds', '4', '-MaxPasses', '5', '-BackoffBaseSeconds', '4', '-BackoffMaxSeconds', '16', '-FastFailureSeconds', $threshold) $fail
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch unproven: The loop ends on its own with exit 0' "The exit code was $code."
    $n = Get-StartCount
    Write-Result ($n -ge 3) "Watch unproven: The holder was started at least three times (started $n)"
    $b = Get-BackoffSeconds (Read-LogLines)
    Write-Result ($b.Count -ge 2 -and $b[0] -eq 4 -and $b[1] -eq 8) "Watch unproven: Backoff lines appear after each death, and double, 4 then 8 (got $($b -join ','))"
  }

  # -Watch, a log that cannot be written: board-holder.log is held without write sharing from before
  # the first pass until after the second try, then released. -Watch MUST go on through it, and
  # finish its passes with exit 0. The hold ends on the second try, which the stub counts, never on a delay.
  Invoke-Case 'Watch: a log that cannot be written does not end the loop' 'keeper.md#watching' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $false
    $logPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.log'
    $max = if ($script:mut) { '2' } else { '8' }
    $first = $false
    $triesHeld = 0
    $running = $false
    $held = [IO.FileStream]::new($logPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Read)
    try {
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', $max, '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '2') $fail
      $first = Wait-Until { (Get-StartCount) -ge 1 } 60 $p
      # The first start fails at once, and its backoff of 1 second falls inside this hold: the second
      # try is the proof that the loop went on while its log could not be written.
      Wait-Until { (Get-StartCount) -ge 2 } 60 $p | Out-Null
      $triesHeld = Get-StartCount
      $running = -not $p.HasExited
    } finally { $held.Dispose() }
    $code = Wait-Watch $p 240
    Write-Result $first 'Watch log held: The keeper tried a holder while the log was held'
    Write-Result ($triesHeld -ge 2) "Watch log held: The keeper tried again after a backoff while the log was held (tries $triesHeld)"
    Write-Result $running 'Watch log held: -Watch is still running when the log is released'
    Write-Result ($code -eq 0) "Watch log held: The loop ends on its own with exit 0 (got $code)"
    $n = Get-StartCount
    Write-Result ($n -eq 8) "Watch log held: All 8 passes ran, one try each (tries $n)"
    Write-Result (@(Read-LogLines | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ backoff seconds=\d+\z' }).Count -ge 1) 'Watch log held: Backoff lines are logged once the log is free'
  }

  # -Watch, -MaxPasses: the process ends by itself after that many passes.
  Invoke-Case 'Watch: MaxPasses ends the loop' 'keeper.md#command-line' {
    Write-Leases @((New-ReleasedRecord 'a' $farFuture))
    $max = if ($script:mut) { '2' } else { '3' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', $max) $stub
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch MaxPasses: The process exits 0 on its own' "The exit code was $code."
    $log = Read-LogLines
    $idle = @($log | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle(\s|\z)' })
    Write-Result ($idle.Count -eq 3) "Watch MaxPasses: Exactly 3 passes ran, 3 idle lines (got $($idle.Count))"
    Write-Result ((Get-HolderProcesses).Count -eq 0) 'Watch MaxPasses: No holder is started'
  }

  # -Watch, no tight loop: passes are IntervalSeconds apart, and a failing holder's starts are further apart.
  Invoke-Case 'Watch: passes are spaced' 'keeper.md#watching' {
    Write-Leases @((New-ReleasedRecord 'a' $farFuture))
    $interval = 2
    $max = if ($script:mut) { '1' } else { '4' }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Watch @('-IntervalSeconds', "$interval", '-MaxPasses', $max) $stub
    $code = Wait-Watch $p 240
    $sw.Stop()
    Write-Result ($code -eq 0) 'Watch spacing: The process exits 0 on its own' "The exit code was $code."
    Write-Result ($sw.Elapsed.TotalSeconds -ge 5.5) "Watch spacing: 4 passes at 2 seconds took at least 6 seconds, less a rounding margin (took $([Math]::Round($sw.Elapsed.TotalSeconds, 1)))"
    $tlog = Read-LogLines
    $times = @($tlog | ForEach-Object { Get-LogTime $_ } | Where-Object { $_ })
    Write-Result ($times.Count -eq 4) "Watch spacing: One timed line per pass (got $($times.Count))"
    $gapsOk = $times.Count -ge 2
    for ($i = 1; $i -lt $times.Count; $i++) { if (($times[$i] - $times[$i - 1]).TotalSeconds -lt 1) { $gapsOk = $false } }
    Write-Result $gapsOk 'Watch spacing: Every pass is at least IntervalSeconds apart, less a one-second rounding margin on the log time'
    Write-Result ($times.Count -ge 2 -and ($times[-1] - $times[0]).TotalSeconds -ge 5) 'Watch spacing: The first and last pass are at least 6 seconds apart, less the margin'

    # A holder that fails at once: starts are spaced by the backoff, not by the interval alone.
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    Remove-Item -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.log') -ErrorAction SilentlyContinue
    $fail = New-FailStub $false
    $base = if ($script:mut) { '1' } else { '3' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '10', '-BackoffBaseSeconds', $base, '-BackoffMaxSeconds', '3') $fail
    $code = Wait-Watch $p 240
    $slog = Read-LogLines
    $started = @($slog | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ (started|failed)(\s|\z)' } | ForEach-Object { Get-LogTime $_ })
    Write-Result ($started.Count -ge 2) "Watch spacing: A failing holder is tried more than once (a start or a failed line, each one try) (got $($started.Count))"
    $startGaps = $true
    for ($i = 1; $i -lt $started.Count; $i++) { if (($started[$i] - $started[$i - 1]).TotalSeconds -lt 2) { $startGaps = $false } }
    Write-Result ($started.Count -ge 2 -and $startGaps) 'Watch spacing: Tries of a failing holder are at least 3 seconds apart, less a one-second margin'
  }

  # -Watch, lock busy: the pass is logged as skipped, and the loop carries on once the lock is free.
  Invoke-Case 'Watch: a locked pass is skipped and the loop carries on' 'keeper.md#locks' {
    Write-Leases @((New-ReleasedRecord 'a' $farFuture))
    $lockPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.lock'
    $held = $null
    if (-not $script:mut) { $held = [IO.FileStream]::new($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
    try {
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '6') $stub
      if (-not $script:mut) { Wait-Until { @(Read-LogLines | Where-Object { $_ -cmatch ' skipped .*reason=locked' }).Count -ge 1 } 60 $p | Out-Null }
    } finally { if ($held) { $held.Dispose() } }
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch locked: The loop ends on its own with exit 0' "The exit code was $code."
    $log = Read-LogLines
    $skipped = @($log | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ skipped lease=- alias=- pid=- reason=locked\z' })
    Write-Result ($skipped.Count -ge 1) "Watch locked: At least one line ""skipped lease=- alias=- pid=- reason=locked"" is logged (got $($skipped.Count))"
    $firstSkip = -1
    for ($i = 0; $i -lt $log.Count; $i++) { if ($log[$i] -cmatch ' skipped ') { $firstSkip = $i; break } }
    $idleAfter = 0
    if ($firstSkip -ge 0) { for ($i = $firstSkip + 1; $i -lt $log.Count; $i++) { if ($log[$i] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle(\s|\z)') { $idleAfter++ } } }
    Write-Result ($idleAfter -ge 1) 'Watch locked: A later pass ran and logged idle, so the loop carried on'
  }

  # The log holds no secret: no token-like string, key path or certificate path, whatever the passes did.
  Invoke-Case 'Watch: the log holds no secret' 'keeper.md#log-lines' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $sentinel = 'tok' + [guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')
    $env:AGENTSPACE_TEST_SECRET_TOKEN = $sentinel
    $logPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.log'
    if ($script:mut) { [IO.File]::WriteAllText($logPath, "2026-11-17T12:00:00Z failed key=C:\Users\x\.ssh\id_ed25519 cert=C:\x\lease.pem`n") }
    try {
      $fail = New-FailStub $false
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '6', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '2') $fail
      $code = Wait-Watch $p 240
    } finally { Remove-Item Env:\AGENTSPACE_TEST_SECRET_TOKEN -ErrorAction SilentlyContinue }
    Write-Result ($code -eq 0) 'Watch secrets: The loop ends on its own with exit 0' "The exit code was $code."
    $log = Read-LogLines
    Write-Result ($log.Count -ge 2) "Watch secrets: The log has lines to check (got $($log.Count))"
    $text = $log -join "`n"
    Write-Result (-not $text.Contains($sentinel)) 'Watch secrets: A token in the environment is not logged'
    Write-Result ($text -cnotmatch '-----BEGIN') 'Watch secrets: No private key block'
    Write-Result ($text -inotmatch '\.(pem|key|crt|cer|pfx|p12)\b|id_(rsa|ed25519|ecdsa)|[\\/]\.ssh[\\/]') 'Watch secrets: No key or certificate path'
    Write-Result ($text -cnotmatch '[A-Za-z0-9+/]{32,}') 'Watch secrets: No token-like run of 32 or more characters'
    Write-Result ($text -inotmatch '(token|secret|password|passwd|bearer)\s*[=:]\s*\S{8,}') 'Watch secrets: No token=, secret= or password= value'
  }

  # -ReleaseGraceSeconds. The stand-in lives 2 seconds, then exits 1, so each holder dies unproven
  # and the backoff applies as well. Log times are whole seconds, which blurs a gap by up to a second
  # either way, so the bounds below leave that room.
  #
  # The grace: after a holder is seen to exit, the next one starts no sooner than the grace. Grace 10
  # and backoff 4: the longer, 10, is waited. A true gap is then at least 2 + 10 = 12 seconds, and at
  # most about 14. The sum, 14, would give a true gap of at least 16.
  Invoke-Case 'Watch: the next holder waits out the release grace, the longer of grace and backoff' 'keeper.md#release-grace' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $false 2
    $grace = if ($script:mut) { '1' } else { '10' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '60', '-BackoffBaseSeconds', '4', '-BackoffMaxSeconds', '4', '-ReleaseGraceSeconds', $grace) $fail
    try { $two = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'started').Count -ge 2 } 120 $p } finally { Stop-Watch $p }
    Write-Result $two 'Watch grace: A second holder is started after the first one exits'
    $log = Read-LogLines
    $st = @(Get-ActionTimes $log 'started')
    $gap = if ($st.Count -ge 2) { ($st[1] - $st[0]).TotalSeconds } else { -1 }
    Write-Result ($gap -ge 11) "Watch grace: The second holder starts at least the grace after the first one exited, 12 seconds less rounding (gap $gap)"
    # Exactly one wait line sits between the two starts, and it is the grace, with no backoff line.
    # The log says it without a timing bound that load could break.
    $between = @(); $seenStarts = 0
    foreach ($l in $log) {
      if ($l -cmatch ' started ') { $seenStarts++; continue }
      if ($seenStarts -eq 1 -and $l -cmatch ' (grace|backoff) seconds=') { $between += $l }
    }
    Write-Result ($between.Count -eq 1 -and $between[0] -cmatch ' grace seconds=10$') "Watch grace: Grace and backoff are not added, the one wait between the starts is ""grace seconds=10"" (got $($between -join ' | '))"
    Write-Result (-not ($log -cmatch ' backoff seconds=')) 'Watch grace: No backoff line is logged'
    $g = @(Get-GraceSeconds $log)
    Write-Result ($g.Count -ge 1 -and $g[0] -ge 8 -and $g[0] -le 10) "Watch grace: A ""grace seconds=<n>"" line is logged for the wait, with n about the grace, 10 (got $($g -join ','))"
    $firstStarted = -1; $firstGrace = -1; $secondStarted = -1; $seen = 0
    for ($i = 0; $i -lt $log.Count; $i++) {
      if ($log[$i] -cmatch ' started ') { $seen++; if ($seen -eq 1) { $firstStarted = $i } elseif ($seen -eq 2) { $secondStarted = $i } }
      if ($firstGrace -lt 0 -and $log[$i] -cmatch ' grace seconds=') { $firstGrace = $i }
    }
    Write-Result ($firstStarted -ge 0 -and ($firstGrace -lt 0 -or $firstGrace -gt $firstStarted)) 'Watch grace: No grace line comes before the first holder, which starts at once'
    Write-Result ($firstGrace -gt $firstStarted -and $secondStarted -gt $firstGrace) 'Watch grace: The grace line comes after the first start and before the second'
  }

  # The backoff stays: backoff 12 against grace 3, the longer, 12, is waited. A true gap is at least
  # 2 + 12 = 14 seconds. With the grace alone it would be about 6 to 8.
  Invoke-Case 'Watch: a backoff longer than the grace is still waited' 'keeper.md#release-grace' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $false 2
    $base = if ($script:mut) { '1' } else { '12' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '60', '-BackoffBaseSeconds', $base, '-BackoffMaxSeconds', $base, '-ReleaseGraceSeconds', '3') $fail
    try { $two = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'started').Count -ge 2 } 120 $p } finally { Stop-Watch $p }
    Write-Result $two 'Watch backoff beats grace: A second holder is started'
    $st = @(Get-ActionTimes (Read-LogLines) 'started')
    $gap = if ($st.Count -ge 2) { ($st[1] - $st[0]).TotalSeconds } else { -1 }
    Write-Result ($gap -ge 13) "Watch backoff beats grace: The second holder starts at least the backoff after the first one exited, 14 seconds less rounding (gap $gap)"
  }

  # The default is 95 seconds. A keeper with no -ReleaseGraceSeconds waits for the grace after its
  # holder exits; the case reads the line and stops the keeper, so it never waits the 95. That no
  # second holder starts during the wait is a wait on a second start, with a deadline of 3 seconds,
  # that the case expects to reach: a keeper that waited less would start one inside it.
  Invoke-Case 'Watch: the release grace is 95 seconds by default' 'keeper.md#release-grace' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $false 1
    if ($script:mut) { $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '60', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '1', '-ReleaseGraceSeconds', '3') $fail }
    else { $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '60', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '1') $fail -DefaultGrace }
    try {
      $seen = Wait-Until { @(Get-GraceSeconds (Read-LogLines)).Count -ge 1 } 60 $p
      $early = Wait-Until { (Get-StartCount) -ge 2 -or @(Get-ActionTimes (Read-LogLines) 'started').Count -ge 2 } 3 $p
    } finally { Stop-Watch $p }
    Write-Result $seen 'Watch default grace: A grace line is logged when the holder has exited'
    $log = Read-LogLines
    $g = @(Get-GraceSeconds $log)
    Write-Result ($g.Count -ge 1 -and $g[0] -ge 93 -and $g[0] -le 95) "Watch default grace: The first grace line says seconds=95, less rounding (got $($g -join ','))"
    Write-Result ($seen -and -not $early -and @(Get-ActionTimes $log 'started').Count -eq 1) 'Watch default grace: No second holder is started within the first seconds of the wait'
  }

  # A first holder is not delayed: no holder recorded, a ready lease, a grace of 3600 seconds. The
  # keeper starts one in its first pass, which -MaxPasses 1 ends; a keeper that waited the grace
  # would not end within the deadline.
  Invoke-Case 'Watch: a first holder is not delayed by the grace' 'keeper.md#release-grace' {
    $r = New-ReadyRecord 'a' $farFuture
    if ($script:mut) { $r = New-ReleasedRecord 'a' $farFuture }
    Write-Leases @($r)
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1', '-ReleaseGraceSeconds', '3600') $stub
    $code = Wait-Watch $p 90
    Write-Result ($code -eq 0) "Watch first holder: The keeper ends its one pass with exit 0, within 90 seconds and not after the grace (got $code)"
    $log = Read-LogLines
    Write-Result (@(Get-ActionTimes $log 'started').Count -eq 1) 'Watch first holder: One started line is logged'
    Write-Result (-not ($log -cmatch ' grace ')) 'Watch first holder: No grace line is logged'
    $rec = Read-Record
    Write-Result ($rec -and (Test-Alive ([int]$rec['pid']))) 'Watch first holder: The recorded holder is running'
  }

  # A lease that becomes ready after the keeper stopped a holder for the last one: its holder waits
  # the release grace from the stop (architect's ruling, 2026-11-17), because the stop leaves the
  # box's session holding the forwards (D12). Lease a is ready with a holder running, then
  # released; lease b becomes ready after the stop. The grace is 10 seconds.
  Invoke-Case 'Watch: the holder after one the keeper stopped waits the release grace' 'keeper.md#release-grace' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $h = Start-TrustedHolder 'a'
    $grace = if ($script:mut) { '1' } else { '10' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '40', '-ReleaseGraceSeconds', $grace) $stub
    $started = $false
    try {
      $kept = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'kept').Count -ge 1 } 60 $p
      Write-Leases @((New-ReleasedRecord 'a' $farFuture))
      $stopped = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'stopped').Count -ge 1 } 60 $p
      # The keeper logs the grace line when it begins the wait after the stop; the second lease
      # becomes ready only once the keeper is in that wait.
      $inGrace = Wait-Until { @(Get-GraceSeconds (Read-LogLines)).Count -ge 1 } 60 $p
      Write-Leases @((New-ReleasedRecord 'a' $farFuture), (New-ReadyRecord 'b' $farFuture))
      $started = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'started').Count -ge 1 } 60 $p
    } finally { Stop-Watch $p }
    Write-Result ($kept -and $stopped) 'Watch new lease: The first lease''s holder was kept, then stopped at release'
    Write-Result $started 'Watch new lease: The holder for the new lease is started'
    $log = Read-LogLines
    $g = @(Get-GraceSeconds $log)
    Write-Result ($inGrace -and $g.Count -ge 1 -and $g[0] -ge 8 -and $g[0] -le 10) "Watch new lease: A ""grace seconds=<n>"" line is logged after the stop, n about 10 (got $($g -join ','))"
    $stopT = @(Get-ActionTimes $log 'stopped'); $startT = @(Get-ActionTimes $log 'started')
    $gap = if ($stopT.Count -ge 1 -and $startT.Count -ge 1) { ($startT[0] - $stopT[0]).TotalSeconds } else { -1 }
    Write-Result ($gap -ge 9) "Watch new lease: The new holder starts no sooner than the grace after the stop, 10 seconds less rounding (gap $gap)"
    $rec = Read-Record
    Write-Result ($rec -and $rec['lease_id'] -ceq (Get-TestId 'b')) 'Watch new lease: The record names the new lease'
  }

  # A -ReleaseGraceSeconds that is not a whole number from 1 to 86400 is a usage error like the other
  # numeric arguments: exit 2, no holder started, though a lease is ready.
  foreach ($bad in @('abc', '-5', '1.5', '86401')) {
    $label = $bad
    $badValue = $bad
    Invoke-Case "Watch: -ReleaseGraceSeconds $label is a usage error" 'keeper.md#usage-errors' {
      Write-Leases @((New-ReadyRecord 'a' $farFuture))
      $value = if ($script:mut) { '3' } else { $badValue }
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1', '-ReleaseGraceSeconds', $value) $stub
      $code = Wait-Watch $p 90
      Write-Result ($code -eq 2) "Watch bad grace ${label}: The keeper exits 2 (got $code)"
      Write-Result ((Get-HolderProcesses).Count -eq 0) "Watch bad grace ${label}: No holder is started"
      Write-Result ($null -eq (Read-Record)) "Watch bad grace ${label}: No record is written"
      $errPath = Join-Path $env:AGENTSPACE_STATE_DIR 'watch.err'
      $errText = if (Test-Path -LiteralPath $errPath) { Get-Content -LiteralPath $errPath -Raw } else { '' }
      Write-Result ([bool]($errText -match 'ReleaseGraceSeconds')) "Watch bad grace ${label}: The usage error names -ReleaseGraceSeconds"
    }
  }

  # One keeper at a time. The keeper holds a lock for its whole life. The first keeper here sleeps 60
  # seconds between passes, so after its first pass it logs nothing more while the case runs, and
  # every other line in the log is the second run's. The second keeper would take 120 seconds to
  # finish its passes if it ran them, so a deadline of 90 seconds tells it from one that exits on the lock.
  Invoke-Case 'Watch: a second keeper exits at once while the first runs' 'keeper.md#locks' {
    Write-Leases @((New-ReleasedRecord 'a' $farFuture))
    $p1 = $null; $p2 = $null
    try {
      $firstPids = @()
      if (-not $script:mut) {
        $p1 = Start-Watch @('-IntervalSeconds', '60', '-MaxPasses', '5') $stub -Tag 'first'
        $ran = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'idle').Count -ge 1 } 60 $p1
        Write-Result $ran 'Watch single: The first keeper logged its first pass'
        $firstPids = @($p1.Id) + @(Get-DescendantPids $p1.Id)
      }
      $p2 = Start-Watch @('-IntervalSeconds', '60', '-MaxPasses', '3') $stub -Tag 'second'
      $ended = $p2.WaitForExit(90000)
      if (-not $ended) { Stop-OwnTree $p2.Id }
      Write-Result ($ended -and $p2.ExitCode -eq 0) "Watch single: The second keeper exits 0 on its own (ended $ended, code $(if ($ended) { $p2.ExitCode } else { 'none' }))"
      $log = Read-LogLines
      $other = @(Get-OtherKeeperLines $log)
      Write-Result ($other.Count -eq 1) "Watch single: One ""another keeper is running"" line is logged (got $($other.Count))"
      Write-Result ($other.Count -eq 1 -and (Test-OtherKeeperPid $other[0] $firstPids)) "Watch single: That line names the first keeper's PID, or - (got $($other -join ' | '); the first keeper runs as $($firstPids -join ', '))"
      $lockText = ''
      try {
        $lf = [IO.File]::Open((Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-keeper.lock'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
        try { $lockText = [IO.StreamReader]::new($lf).ReadToEnd().Trim() } finally { $lf.Dispose() }
      } catch { }
      Write-Result ($lockText -cmatch '\A\d{1,10}\z' -and $firstPids -contains [int]$lockText) "Watch single: board-holder-keeper.lock holds the running keeper's PID (got '$lockText'; the first keeper runs as $($firstPids -join ', '))"
      $passLines = @($log | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ (started|kept|idle|skipped|stopped)(\s|\z)' })
      Write-Result ($passLines.Count -eq 1) "Watch single: The second keeper ran no pass, so only the first keeper's one pass line is there (got $($passLines.Count))"
      Write-Result ($null -ne $p1 -and -not $p1.HasExited) 'Watch single: The first keeper is still running'
    } finally { if ($p1) { Stop-Watch $p1 }; if ($p2) { Stop-Watch $p2 } }
  }

  # The lock goes when the keeper process is killed, and the holder it started does not carry it on.
  Invoke-Case 'Watch: a new keeper starts once the first is killed' 'keeper.md#locks' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $p1 = Start-Watch @('-IntervalSeconds', '60', '-MaxPasses', '5') $stub -Tag 'first'
    $p2 = $null
    try {
      $started = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'started').Count -ge 1 } 60 $p1
      Write-Result $started 'Watch kill: The first keeper started a holder'
      $rec = Read-Record
      $holderPid = if ($rec) { [int]$rec['pid'] } else { -1 }
      if (-not $script:mut) {
        $killed = Stop-KeeperOnly $p1
        Write-Result $killed 'Watch kill: The first keeper was killed by the PID of the process the check started'
        Write-Result ($p1.WaitForExit(30000)) 'Watch kill: The first keeper has ended'
      }
      Write-Result ($holderPid -gt 0 -and (Test-Alive $holderPid)) 'Watch kill: The holder is still running after its keeper was killed'
      $p2 = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1') $stub -Tag 'second'
      $code = Wait-Watch $p2 90
      Write-Result ($code -eq 0) "Watch kill: The new keeper runs its pass and exits 0 (got $code)"
      $log = Read-LogLines
      Write-Result (@(Get-OtherKeeperLines $log).Count -eq 0) 'Watch kill: The new keeper does not log "another keeper is running"'
      $kept = @($log | Where-Object { Test-LogLine $_ 'kept' (Get-TestId 'a') (Get-TestAlias 'a') $holderPid })
      Write-Result ($kept.Count -eq 1) "Watch kill: The new keeper's pass logs kept for the holder the dead keeper left (got $($kept.Count))"
      Write-Result (@(Get-ActionTimes $log 'started').Count -eq 1) 'Watch kill: No second holder is started'
      $rec2 = Read-Record
      Write-Result ($rec2 -and [int]$rec2['pid'] -eq $holderPid) 'Watch kill: The record still names the same holder'
    } finally { Stop-Watch $p1; if ($p2) { Stop-Watch $p2 } }
  }

  # A pass without -Watch keeps its own per-pass lock and is not affected by a running keeper.
  Invoke-Case 'Watch: a one-pass run is unaffected by a running keeper' 'keeper.md#locks' {
    Write-Leases @((New-ReleasedRecord 'a' $farFuture))
    $p1 = $null
    try {
      if (-not $script:mut) {
        $p1 = Start-Watch @('-IntervalSeconds', '60', '-MaxPasses', '5') $stub -Tag 'first'
        $ran = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'idle').Count -ge 1 } 60 $p1
        Write-Result $ran 'Watch one-pass: The keeper logged its first pass'
        Write-Result (-not $p1.HasExited) 'Watch one-pass: The keeper is running'
      }
      $code = Invoke-Keeper $now
      Write-Result ($code -eq 0) "Watch one-pass: The run without -Watch exits 0 (got $code)"
      $log = Read-LogLines
      Write-Result (@(Get-OtherKeeperLines $log).Count -eq 0) 'Watch one-pass: It does not log "another keeper is running"'
      Write-Result (@(Get-ActionTimes $log 'idle').Count -eq 2) "Watch one-pass: Its pass line is logged beside the keeper's (got $(@(Get-ActionTimes $log 'idle').Count) idle lines, want 2)"
    } finally { if ($p1) { Stop-Watch $p1 } }
  }

} finally {
  foreach ($n in $envNames) { [Environment]::SetEnvironmentVariable($n, $savedEnv[$n]) }
  Stop-RunProcesses
  $leftBehind = @(Get-RunProcesses).Count
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
}

if (-not $SelfCheck) {
  $script:mut = $false
  $script:caseSpec = 'keeper.md#checking-the-keeper'
  Write-Result ($leftBehind -eq 0) 'The run leaves no process behind' "$leftBehind process(es) of this run were still running after the cleanup."
}
Exit-Run

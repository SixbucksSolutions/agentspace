#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for the board holder keeper's one pass (story 0.02.T1.2, D30), run through the real
# entry point as a child process: pwsh -NoProfile -File scripts\board-holder.ps1 -SshExe <stub> [-Now t]
# The checks are written from the story card's pinned interface and D30, not from the keeper's code.
#
# What it touches:
# - Every case has its own throwaway state folder under $env:TEMP, named by AGENTSPACE_STATE_DIR, with
#   a hand-written leases.json. The real %USERPROFILE%\.agentspace, ~/.ssh, the real ssh.exe and the
#   box are never used.
# - The stand-in for ssh.exe is scripts\test-board-holder-stub.cmd, copied into the run's folder so
#   that its command line is unique to this run. It prints a line, then waits about 20 minutes, and
#   carries its arguments on its own command line; the ping it runs is its child, which makes a tree.
# - Cleanup stops, in a finally and after every case, only processes whose command line names this
#   run's copy of the stub, and their children: a process whose parent is still that stub and whose
#   creation time is no earlier than the stub's, and so on down. Each is pinned by a handle and its
#   start time checked before it is killed, so a reused PID is never killed. taskkill /T, which
#   follows parent PIDs alone, is never used. Nothing else is stopped.
# - One case denies PROCESS_TERMINATE on a stub of its own, through the stub's DACL, so that the
#   keeper cannot open it to stop it. Cleanup gives that right back before it stops the stub.
#
# The cases, each its own set of checks: idle; start when a lease is ready; the holder's output goes
# to board-holder-ssh.log; keep a running holder; stop on release, on expiry through -Now, at the
# expiry second itself, and when the lease is gone; a stop leaves alone a foreign process with the
# same alias; start afresh when the recorded PID is dead; leave alone a PID reused by another
# process (start time differs), a process whose start time matches but whose command line does
# not, and a process the keeper did not start that names the lease alias; choose the ready lease
# that expires last; exit 1 when the lease state cannot be read, with and without a holder running;
# keep the record of a holder that cannot be opened to stop it; skip a pass while another holds
# board-holder.lock; exit 2 on a -Now that is not a time.
# Story 0.02.T2.2 adds -Watch and the installer (D30), checked from the story's pinned interface:
# backoff doubles and is capped; backoff resets after a holder that lived past -FastFailureSeconds (a
# test-only name for the 60-second threshold, assumed, so a case need not wait a minute); -MaxPasses
# ends the loop; passes are spaced; a locked pass is logged as skipped and the loop carries on; the
# log holds no secret; install-board-holder.ps1 -DryRun prints the logon task and changes nothing.
# Every -Watch run has a time limit, so a loop that never ends fails the case rather than hanging it.
# The installer is run with -DryRun only, and the test never registers, changes or removes a task.
# The review of 0.02.T2.2 added: -Watch survives a board-holder.log that cannot be written for a
# while; a holder that dies after the interval but before the fast-failure threshold, unproven,
# still backs off; board-holder-ssh.log is rotated past 1 MB; the dry run prints the whole task
# definition, and warns when the repo is a git worktree.
#
# Every case MUST be shown able to fail. -SelfCheck does it: each case is run again with a
# deliberately broken fixture (for example the lease is released where the case expects it ready,
# or the record names a process the case expects to be foreign), and the case MUST then register at
# least one failed check. A case that still passes on its broken fixture cannot fail and is reported
# CANNOT-FAIL, which exits 1. The self check is meaningful only for a keeper that passes the normal
# run, because a keeper that fails everything fails the broken fixtures too.
# -Keeper <path> runs the same cases against another script, such as a trivial no-op, to show that
# the normal run fails for a wrong keeper (a no-op keeper passes the idle case only).
#
# It prints PASS or FAIL per check and exits 1 on any failure.
# -Case <wildcard> runs only the cases whose name matches it, for example -Case 'Watch*'.
# Run:  pwsh -NoProfile -File scripts\test-board-holder.ps1
#       pwsh -NoProfile -File scripts\test-board-holder.ps1 -SelfCheck
#       pwsh -NoProfile -File scripts\test-board-holder.ps1 -Keeper <other script> [-Case <wildcard>]
param(
  [switch]$SelfCheck,
  [string]$Keeper,
  [string]$Case = '*'
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $Keeper) { $Keeper = Join-Path $repo 'scripts\board-holder.ps1' }
$pwsh = (Get-Process -Id $PID).Path
$ci = [Globalization.CultureInfo]::InvariantCulture

$temp = Join-Path $env:TEMP "agentspace-test-board-holder-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $temp | Out-Null
$stub = Join-Path $temp 'ssh-stub.cmd'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-board-holder-stub.cmd') -Destination $stub

$savedState = $env:AGENTSPACE_STATE_DIR
$passes = 0
$failures = 0
$script:mut = [bool]$SelfCheck
$script:caseFails = 0
$script:caseChecks = 0

# The lease fixtures: fixed times, so no case depends on the clock.
$now = '2026-11-17T12:00:00Z'
$created = '2026-11-17T08:00:00Z'
$aliasOf = @{}
function Get-Id([string]$Letter) { "0000000$Letter-0000-4000-8000-000000000000" }
function Get-Alias([string]$Letter) { "agentspace-test-bh-0000000$Letter" }

function Write-Result([bool]$Ok, [string]$Text) {
  $script:caseChecks++
  if (-not $Ok) { $script:caseFails++ }
  if ($script:mut) { return }
  if ($Ok) { Write-Output "PASS  $Text"; $script:passes++ } else { Write-Output "FAIL  $Text"; $script:failures++ }
}

function New-ReadyRecord([string]$Letter, [string]$Expires) {
  [ordered]@{
    id = Get-Id $Letter; project = 'test-bh'; state = 'ready'; platform = 'linux/arm64'; profile = 'agent-linux'
    size = 'm'; ttl_seconds = 28800; created_at = $created; expires_at = $Expires
    ssh = [ordered]@{ host_alias = Get-Alias $Letter; user = 'ubuntu' }
  }
}

function New-ReleasedRecord([string]$Letter, [string]$Expires) {
  $r = [ordered]@{
    id = Get-Id $Letter; project = 'test-bh'; state = 'released'; platform = 'linux/arm64'; profile = 'agent-linux'
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
  , @(Get-Content -LiteralPath $p | Where-Object { $_ -ne '' })
}

function Test-Alive([int]$ProcessId) { [bool](Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) }

# Sets a process's DACL, so that one case can deny PROCESS_TERMINATE on a stub of its own. The test's
# own user owns the stub, and an owner may always rewrite the DACL.
if (-not ('AgentspaceTest.ProcessAcl' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
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
}
'@
}
$userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
# Every right for the user and SYSTEM; Lock adds a deny of PROCESS_TERMINATE (0x1) for everyone first.
$openAcl = "D:(A;;0x1FFFFF;;;$userSid)(A;;0x1FFFFF;;;SY)"
$lockedAcl = "D:(D;;0x1;;;WD)(A;;0x1FFFFF;;;$userSid)(A;;0x1FFFFF;;;SY)"
function Lock-Process([int]$ProcessId) { [AgentspaceTest.ProcessAcl]::Set($ProcessId, $lockedAcl) }
function Unlock-Process([int]$ProcessId) { try { [AgentspaceTest.ProcessAcl]::Set($ProcessId, $openAcl) } catch { } }

function Get-CommandLine([int]$ProcessId) {
  $c = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId=$ProcessId" -ErrorAction SilentlyContinue
  if ($c) { [string]$c.CommandLine } else { '' }
}

function Get-ChildPids([int]$ProcessId) {
  @(Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId=$ProcessId" -ErrorAction SilentlyContinue | ForEach-Object { [int]$_.ProcessId })
}

function Wait-Until([scriptblock]$Condition, [int]$Seconds = 10) {
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    if (& $Condition) { return $true }
    Start-Sleep -Milliseconds 100
  }
  [bool](& $Condition)
}

# The processes whose command line names this run's copy of the stub, the ping children excluded.
function Get-StubProcesses {
  @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -and $_.CommandLine.Contains($stub) -and $_.ProcessId -ne $PID })
}

# Stops a process the test started, and its descendants: each child's parent is still the process
# above it, and it was created no earlier than that process. Every process is pinned by a handle and
# its start time checked against the snapshot before it is killed, so a reused PID is never
# killed. Leaves go first. A process whose DACL a case locked gets its rights back first.
function Stop-OwnTree([int]$ProcessId) {
  $all = @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue)
  $root = $all | Where-Object { $_.ProcessId -eq $ProcessId } | Select-Object -First 1
  if (-not $root) { return }
  $tree = [System.Collections.Generic.List[object]]::new()
  $tree.Add($root)
  for ($i = 0; $i -lt $tree.Count; $i++) {
    $up = $tree[$i]
    foreach ($c in $all) {
      if ($c.ParentProcessId -eq $up.ProcessId -and $c.ProcessId -ne $up.ProcessId -and $c.CreationDate -ge $up.CreationDate -and
        -not ($tree | Where-Object { $_.ProcessId -eq $c.ProcessId })) { $tree.Add($c) }
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

function Stop-Stubs {
  foreach ($p in Get-StubProcesses) { Stop-OwnTree $p.ProcessId }
  Wait-Until { (Get-StubProcesses).Count -eq 0 } 10 | Out-Null
}

# A stand-in process started by the test itself: it is foreign to the keeper. Returns its PID, its
# start time as the record writes it, and its children, the ping among them.
function Start-Stub([string]$Alias) {
  $p = Start-Process -FilePath $stub -ArgumentList @('-N', '-o', 'ExitOnForwardFailure=yes', $Alias) -PassThru -WindowStyle Hidden
  $null = $p.Handle
  Wait-Until {
    @(Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId=$($p.Id) AND Name='PING.EXE'" -ErrorAction SilentlyContinue).Count -gt 0
  } 10 | Out-Null
  $kids = @(Get-ChildPids $p.Id)
  [pscustomobject]@{ Pid = $p.Id; Started = (Format-Utc $p.StartTime); Children = $kids }
}

# True when the process and every child recorded for it are alive.
function Test-TreeAlive($Process) {
  (Test-Alive $Process.Pid) -and $Process.Children.Count -gt 0 -and @($Process.Children | Where-Object { Test-Alive $_ }).Count -eq $Process.Children.Count
}

# Runs one pass of the keeper and returns its exit code. Its output goes to files, never to a pipe,
# so the holder it starts cannot keep this run waiting.
function Invoke-Keeper([string]$At) {
  $out = Join-Path $env:AGENTSPACE_STATE_DIR 'keeper.out'
  $err = Join-Path $env:AGENTSPACE_STATE_DIR 'keeper.err'
  $a = @('-NoProfile', '-File', $Keeper, '-SshExe', $stub)
  if ($At) { $a += @('-Now', $At) }
  $p = Start-Process -FilePath $pwsh -ArgumentList $a -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
  $null = $p.Handle
  if (-not $p.WaitForExit(120000)) {
    Stop-OwnTree $p.Id
    return -999
  }
  $p.ExitCode
}

# One case: a fresh state folder, the body, and a cleanup of every stub process, in a finally.
function Invoke-Case([string]$Name, [scriptblock]$Body) {
  if ($Name -notlike $Case) { return }
  $dir = Join-Path $temp ([guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $env:AGENTSPACE_STATE_DIR = $dir
  $script:caseFails = 0
  $script:caseChecks = 0
  try { & $Body } catch { Write-Result $false "${Name}: no unexpected error ($($_.Exception.Message))" } finally { Stop-Stubs }
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
  $gone = Wait-Until { -not (Test-Alive $Holder.Pid) } 10
  Write-Result $gone "${Name}: the holder process is gone"
  $kidsGone = Wait-Until { @($Holder.Children | Where-Object { Test-Alive $_ }).Count -eq 0 } 10
  Write-Result ($Holder.Children.Count -gt 0 -and $kidsGone) "${Name}: the holder's child process is gone too"
  Write-Result ($null -eq (Read-Record) -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'))) "${Name}: the record is removed"
  $log = Read-LogLines
  Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'stopped' (Get-Id $Letter) (Get-Alias $Letter) $Holder.Pid)) "${Name}: one stopped line is logged with the lease, alias and PID"
}

# A running holder that the keeper would trust: a stub with the holder's arguments, and its record.
function Start-TrustedHolder([string]$Letter) {
  $h = Start-Stub (Get-Alias $Letter)
  Write-Record $h.Pid $h.Started (Get-Id $Letter) (Get-Alias $Letter)
  $h
}

# ---- -Watch and installer helpers (story 0.02.T2.2) ----

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

# Starts the keeper with -Watch and returns the process. Output goes to files, never to a pipe.
function Start-Watch([string[]]$Extra, [string]$Ssh) {
  $out = Join-Path $env:AGENTSPACE_STATE_DIR 'watch.out'
  $err = Join-Path $env:AGENTSPACE_STATE_DIR 'watch.err'
  $a = @('-NoProfile', '-File', $Keeper, '-SshExe', $Ssh, '-Watch') + $Extra
  $p = Start-Process -FilePath $pwsh -ArgumentList $a -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
  $null = $p.Handle
  $p
}

# Waits for the process to end on its own; one that does not within the limit is stopped and gives -999.
function Wait-Watch($Process, [int]$Seconds = 240) {
  if (-not $Process.WaitForExit($Seconds * 1000)) { Stop-OwnTree $Process.Id; return -999 }
  $Process.ExitCode
}

function Get-LogTime([string]$Line) {
  if ($Line -cmatch '\A(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) ') {
    [DateTime]::ParseExact($Matches[1], "yyyy-MM-dd'T'HH:mm:ss'Z'", $ci, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal)
  }
}

function Get-BackoffSeconds($Log) {
  @($Log | ForEach-Object { if ($_ -cmatch '\bbackoff seconds=(\d+)(\s|\z)') { [int]$Matches[1] } })
}

$farFuture = '2099-01-01T00:00:00Z'

try {
  # Idle.
  Invoke-Case 'Idle' {
    $recs = @((New-ReleasedRecord 'a' '2026-11-17T20:00:00Z'))
    if ($script:mut) { $recs += New-ReadyRecord 'b' '2026-11-17T13:00:00Z' }
    Write-Leases $recs
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Idle: the pass exits 0'
    Write-Result ((Get-StubProcesses).Count -eq 0) 'Idle: no holder is started'
    Write-Result ($null -eq (Read-Record)) 'Idle: no record is written'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and $log[0] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle(\s|\z)') 'Idle: one idle line is logged'
  }

  # Start, and the holder's output.
  Invoke-Case 'Start' {
    $r = New-ReadyRecord 'a' '2026-11-17T13:00:00Z'
    if ($script:mut) { $r = New-ReleasedRecord 'a' '2026-11-17T13:00:00Z' }
    Write-Leases @($r)
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Start: the pass exits 0'
    $rec = Read-Record
    Write-Result ($null -ne $rec) 'Start: the record is written'
    $recPid = if ($rec) { [int]$rec['pid'] } else { -1 }
    Write-Result ($rec -and $rec['lease_id'] -ceq (Get-Id 'a') -and $rec['alias'] -ceq (Get-Alias 'a')) 'Start: the record names the lease ID and the alias'
    $alive = $recPid -gt 0 -and (Test-Alive $recPid)
    Write-Result $alive 'Start: the recorded PID is a running process after the pass exits'
    $startTime = if ($alive) { Format-Utc (Get-Process -Id $recPid).StartTime } else { '' }
    Write-Result ($alive -and $rec -and (Test-StartTimeExact $rec['process_started_at'] $startTime)) 'Start: the record holds exactly that process''s start time, UTC truncated to the millisecond'
    $cl = if ($alive) { Get-CommandLine $recPid } else { '' }
    $tail = '-N -o ExitOnForwardFailure=yes ' + [regex]::Escape((Get-Alias 'a')) + '"?\s*\z'
    Write-Result ($cl.Contains($stub) -and $cl -cmatch $tail) 'Start: the holder runs -SshExe -N -o ExitOnForwardFailure=yes <alias>'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'started' (Get-Id 'a') (Get-Alias 'a') $recPid)) 'Start: one started line is logged with the lease, alias and PID'
    $sshLog = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.log'
    $hasOut = Wait-Until { (Test-Path -LiteralPath $sshLog) -and ((Get-Content -LiteralPath $sshLog -Raw) -match 'stub-started') } 10
    Write-Result $hasOut 'Start: the holder''s output is appended to board-holder-ssh.log'
  }

  # Keep.
  Invoke-Case 'Keep' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    if ($script:mut) { Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z')) }
    $before = Get-Content -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json') -Raw
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Keep: the pass exits 0'
    Write-Result (Test-Alive $h.Pid) 'Keep: the running holder is still running'
    $rec = Read-Record
    Write-Result ($rec -and [int]$rec['pid'] -eq $h.Pid -and $rec['process_started_at'] -ceq $h.Started) 'Keep: the record still names the same PID and start time'
    Write-Result ((Get-StubProcesses | Where-Object { $_.ProcessId -ne $h.Pid }).Count -eq 0) 'Keep: no second holder is started'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'kept' (Get-Id 'a') (Get-Alias 'a') $h.Pid)) 'Keep: one kept line is logged with the lease, alias and PID'
  }

  # Stop on release.
  Invoke-Case 'Stop on release' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    if (-not $script:mut) { Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z')) }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Stop on release: the pass exits 0'
    Assert-Stopped 'Stop on release' $h 'a'
  }

  # Stop on expiry, through -Now.
  Invoke-Case 'Stop on expiry' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $at = if ($script:mut) { '2026-11-17T12:59:59Z' } else { '2026-11-17T13:00:01Z' }
    $code = Invoke-Keeper $at
    Write-Result ($code -eq 0) 'Stop on expiry: the pass exits 0'
    Assert-Stopped 'Stop on expiry' $h 'a'
  }

  # Stop at the expiry second: the lease is ready only while expires_at is after -Now.
  Invoke-Case 'Stop at the expiry second' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $at = if ($script:mut) { '2026-11-17T12:59:59Z' } else { '2026-11-17T13:00:00Z' }
    $code = Invoke-Keeper $at
    Write-Result ($code -eq 0) 'Stop at the expiry second: the pass exits 0'
    Assert-Stopped 'Stop at the expiry second' $h 'a'
  }

  # Stop when the lease is gone.
  Invoke-Case 'Stop when the lease is gone' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    if (-not $script:mut) { Write-Leases @() }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Stop when the lease is gone: the pass exits 0'
    Assert-Stopped 'Stop when the lease is gone' $h 'a'
  }

  # A stop with a foreign process present: the keeper's own holder for alias 'a' and its tree are
  # stopped, and a process it did not start, with the same alias, is left running with its child.
  Invoke-Case 'Stop leaves a foreign process alone' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $f = Start-Stub (Get-Alias 'a')
    if ($script:mut) { Write-Record $f.Pid $f.Started (Get-Id 'a') (Get-Alias 'a') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Stop leaves a foreign process alone: the pass exits 0'
    Assert-Stopped 'Stop leaves a foreign process alone' $h 'a'
    Write-Result (Test-TreeAlive $f) 'Stop leaves a foreign process alone: the foreign process and its child are still alive'
  }

  # A dead recorded PID: start afresh.
  Invoke-Case 'Dead PID' {
    $r = New-ReadyRecord 'a' '2026-11-17T13:00:00Z'
    if ($script:mut) { $r = New-ReleasedRecord 'a' '2026-11-17T13:00:00Z' }
    Write-Leases @($r)
    $dead = Start-Stub (Get-Alias 'a')
    Write-Record $dead.Pid $dead.Started (Get-Id 'a') (Get-Alias 'a')
    Stop-OwnTree $dead.Pid
    Wait-Until { -not (Test-Alive $dead.Pid) } 10 | Out-Null
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Dead PID: the pass exits 0'
    $rec = Read-Record
    $newPid = if ($rec) { [int]$rec['pid'] } else { -1 }
    Write-Result ($rec -and $newPid -ne $dead.Pid -and (Test-Alive $newPid)) 'Dead PID: a new holder is running and the record names it'
    Write-Result ($rec -and $rec['lease_id'] -ceq (Get-Id 'a')) 'Dead PID: the record names the lease'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'started' (Get-Id 'a') (Get-Alias 'a') $newPid)) 'Dead PID: one started line is logged'
  }

  # A reused PID, lease ready: the record's start time is wrong, so the process is not the keeper's.
  Invoke-Case 'Reused PID, lease ready' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub (Get-Alias 'a')
    $wrong = Format-Utc ([DateTime]::Parse($f.Started, $ci, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal).AddHours(-1))
    Write-Record $f.Pid $(if ($script:mut) { $f.Started } else { $wrong }) (Get-Id 'a') (Get-Alias 'a')
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Reused PID, lease ready: the pass exits 0'
    Write-Result ((Test-Alive $f.Pid) -and @($f.Children | Where-Object { Test-Alive $_ }).Count -eq $f.Children.Count) 'Reused PID, lease ready: the foreign process and its child are still alive'
    $rec = Read-Record
    Write-Result ($rec -and [int]$rec['pid'] -ne $f.Pid -and (Test-Alive ([int]$rec['pid']))) 'Reused PID, lease ready: the keeper started a holder of its own'
  }

  # A reused PID, lease released: nothing of the keeper's to stop.
  Invoke-Case 'Reused PID, lease released' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub (Get-Alias 'a')
    $wrong = Format-Utc ([DateTime]::Parse($f.Started, $ci, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal).AddHours(-1))
    Write-Record $f.Pid $(if ($script:mut) { $f.Started } else { $wrong }) (Get-Id 'a') (Get-Alias 'a')
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Reused PID, lease released: the pass exits 0'
    Write-Result ((Test-Alive $f.Pid) -and @($f.Children | Where-Object { Test-Alive $_ }).Count -eq $f.Children.Count) 'Reused PID, lease released: the foreign process and its child are still alive'
    Write-Result ((Get-StubProcesses).Count -eq 1) 'Reused PID, lease released: no holder is started'
  }

  # The start time matches but the command line does not.
  Invoke-Case 'Command line differs' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub $(if ($script:mut) { Get-Alias 'a' } else { Get-Alias 'f' })
    Write-Record $f.Pid $f.Started (Get-Id 'a') (Get-Alias 'a')
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Command line differs: the pass exits 0'
    Write-Result ((Test-Alive $f.Pid) -and @($f.Children | Where-Object { Test-Alive $_ }).Count -eq $f.Children.Count) 'Command line differs: the process with the matching start time but another command line is still alive'
  }

  # A process the keeper did not start, naming the lease alias, no record: lease released.
  Invoke-Case 'Foreign process, lease released' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub (Get-Alias 'a')
    if ($script:mut) { Write-Record $f.Pid $f.Started (Get-Id 'a') (Get-Alias 'a') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Foreign process, lease released: the pass exits 0'
    Write-Result ((Test-Alive $f.Pid) -and @($f.Children | Where-Object { Test-Alive $_ }).Count -eq $f.Children.Count) 'Foreign process, lease released: the foreign process and its child are still alive'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and $log[0] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle(\s|\z)') 'Foreign process, lease released: the pass is idle'
  }

  # The same, lease ready: the keeper starts its own, never adopting the foreign one.
  Invoke-Case 'Foreign process, lease ready' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $f = Start-Stub (Get-Alias 'a')
    if ($script:mut) { Write-Record $f.Pid $f.Started (Get-Id 'a') (Get-Alias 'a') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Foreign process, lease ready: the pass exits 0'
    Write-Result (Test-TreeAlive $f) 'Foreign process, lease ready: the foreign process and its child are still alive'
    $rec = Read-Record
    Write-Result ($rec -and [int]$rec['pid'] -ne $f.Pid -and (Test-Alive ([int]$rec['pid']))) 'Foreign process, lease ready: the record names a holder the keeper started, not the foreign process'
  }

  # The ready lease that expires last.
  Invoke-Case 'Last expiry' {
    $c = if ($script:mut) { '2026-11-17T14:30:00Z' } else { '2026-11-17T14:00:00Z' }
    $b = if ($script:mut) { '2026-11-17T14:00:00Z' } else { '2026-11-17T15:00:00Z' }
    Write-Leases @(
      (New-ReadyRecord 'a' '2026-11-17T13:00:00Z'),
      (New-ReadyRecord 'b' $b),
      (New-ReadyRecord 'c' $c),
      (New-ReleasedRecord 'd' '2026-11-17T20:00:00Z'),
      (New-ReadyRecord 'e' '2026-11-17T11:00:00Z'))
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Last expiry: the pass exits 0'
    $rec = Read-Record
    Write-Result ($rec -and $rec['lease_id'] -ceq (Get-Id 'b') -and $rec['alias'] -ceq (Get-Alias 'b')) 'Last expiry: the record names the ready lease that expires last, not a released or expired one'
    $recPid = if ($rec) { [int]$rec['pid'] } else { -1 }
    $cl = if ($recPid -gt 0) { Get-CommandLine $recPid } else { '' }
    Write-Result ($cl.Contains((Get-Alias 'b')) -and -not $cl.Contains((Get-Alias 'a')) -and -not $cl.Contains((Get-Alias 'c'))) 'Last expiry: the holder runs for that lease''s alias'
    Write-Result ((Get-StubProcesses).Count -eq 1) 'Last expiry: one holder only'
  }

  # Lease state that cannot be read.
  Invoke-Case 'Unreadable lease state' {
    if ($script:mut) { Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z')) }
    else { [IO.File]::WriteAllText((Join-Path $env:AGENTSPACE_STATE_DIR 'leases.json'), '{ this is not json') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 1) 'Unreadable lease state: the pass exits 1'
    Write-Result ((Get-StubProcesses).Count -eq 0) 'Unreadable lease state: no holder is started'
    Write-Result ($null -eq (Read-Record)) 'Unreadable lease state: no record is written'
  }

  # Lease state that cannot be read, with a trusted holder running: the holder and its record are
  # left as they are.
  Invoke-Case 'Unreadable lease state, holder running' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $recordPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'
    $before = [IO.File]::ReadAllText($recordPath)
    if ($script:mut) { Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z')) }
    else { [IO.File]::WriteAllText((Join-Path $env:AGENTSPACE_STATE_DIR 'leases.json'), '{ this is not json') }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 1) 'Unreadable lease state, holder running: the pass exits 1'
    Write-Result (Test-TreeAlive $h) 'Unreadable lease state, holder running: the holder and its child are still alive'
    Write-Result ((Test-Path -LiteralPath $recordPath) -and [IO.File]::ReadAllText($recordPath) -ceq $before) 'Unreadable lease state, holder running: the record is unchanged'
    Write-Result ((Get-StubProcesses | Where-Object { $_.ProcessId -ne $h.Pid }).Count -eq 0) 'Unreadable lease state, holder running: no other holder is started'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'failed' '-' '-' '-') -and $log[0] -cmatch ' reason=\S') 'Unreadable lease state, holder running: one failed line with a reason is logged'
  }

  # A trusted holder that the keeper cannot open with the right to stop it (a holder started
  # elevated, met by a pass that is not): the DACL of the test's own stub denies PROCESS_TERMINATE.
  # The pass MUST log failed and keep the record, so the next pass tries again.
  Invoke-Case 'Holder that cannot be opened is kept' {
    Write-Leases @((New-ReleasedRecord 'a' '2026-11-17T13:00:00Z'))
    $h = Start-TrustedHolder 'a'
    $recordPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'
    $before = [IO.File]::ReadAllText($recordPath)
    if (-not $script:mut) { Lock-Process $h.Pid }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Holder that cannot be opened is kept: the pass exits 0'
    Write-Result (Test-TreeAlive $h) 'Holder that cannot be opened is kept: the holder and its child are still alive'
    Write-Result ((Test-Path -LiteralPath $recordPath) -and [IO.File]::ReadAllText($recordPath) -ceq $before) 'Holder that cannot be opened is kept: the record is kept unchanged'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and (Test-LogLine $log[0] 'failed' (Get-Id 'a') (Get-Alias 'a') $h.Pid) -and
      $log[0] -cmatch ' reason=The holder could not be opened to stop it') 'Holder that cannot be opened is kept: one failed line says the holder could not be opened'
    Write-Result (-not ($log -cmatch ' stopped ')) 'Holder that cannot be opened is kept: no stopped line is logged'
  }

  # Another pass holds board-holder.lock: the pass is skipped, says so, and exits 0. A ready lease
  # is there, so a pass that ran would start a holder.
  Invoke-Case 'Lock busy' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $lockPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.lock'
    $held = $null
    if (-not $script:mut) {
      $held = [IO.FileStream]::new($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    }
    try { $code = Invoke-Keeper $now } finally { if ($held) { $held.Dispose() } }
    Write-Result ($code -eq 0) 'Lock busy: the pass exits 0'
    $log = Read-LogLines
    Write-Result ($log.Count -eq 1 -and $log[0] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ skipped lease=- alias=- pid=- reason=locked\z') 'Lock busy: one line "skipped lease=- alias=- pid=- reason=locked" is logged'
    Write-Result ((Get-StubProcesses).Count -eq 0) 'Lock busy: no holder is started'
    Write-Result ($null -eq (Read-Record)) 'Lock busy: no record is written'
  }

  # A -Now that is not a time: a usage error, exit 2, and nothing is done.
  Invoke-Case 'Bad -Now' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $at = if ($script:mut) { $now } else { '2026-11-17T12:00:00' }
    $code = Invoke-Keeper $at
    Write-Result ($code -eq 2) 'Bad -Now: the pass exits 2'
    Write-Result ((Get-StubProcesses).Count -eq 0) 'Bad -Now: no holder is started'
    Write-Result ($null -eq (Read-Record)) 'Bad -Now: no record is written'
  }

  # Rotation: a board-holder-ssh.log just over 1 MB is renamed to board-holder-ssh.log.1 by the
  # next pass, which logs one rotated line, and the holder it starts writes a new, small log.
  Invoke-Case 'Rotation of board-holder-ssh.log' {
    Write-Leases @((New-ReadyRecord 'a' '2026-11-17T13:00:00Z'))
    $sshLog = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.log'
    $size = if ($script:mut) { 1MB - 100 } else { 1MB + 100 }
    $fs = [IO.File]::Create($sshLog)
    try { $fs.SetLength($size) } finally { $fs.Dispose() }
    $code = Invoke-Keeper $now
    Write-Result ($code -eq 0) 'Rotation: the pass exits 0'
    $old = [IO.FileInfo]::new("$sshLog.1")
    Write-Result ($old.Exists -and $old.Length -eq $size) 'Rotation: the old log is now board-holder-ssh.log.1, whole'
    $small = Wait-Until { $f = [IO.FileInfo]::new($sshLog); $f.Exists -and $f.Length -lt 4096 -and ((Get-Content -LiteralPath $sshLog -Raw) -match 'stub-started') } 10
    Write-Result $small 'Rotation: the new holder writes a new board-holder-ssh.log, under 4 KB'
    $rotated = @(Read-LogLines | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ rotated lease=- alias=- pid=-\z' })
    Write-Result ($rotated.Count -eq 1) "Rotation: one line ""rotated lease=- alias=- pid=-"" is logged (got $($rotated.Count))"
  }

  # -Watch, backoff: a holder that fails at once is restarted after 1, 2, 4, 4 seconds, never faster.
  Invoke-Case 'Watch: backoff doubles and is capped' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $false
    $cap = if ($script:mut) { '64' } else { '4' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '12', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', $cap) $fail
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch backoff: the loop ends on its own with exit 0'
    $b = Get-BackoffSeconds (Read-LogLines)
    Write-Result ($b.Count -ge 4 -and (($b[0..3]) -join ',') -ceq '1,2,4,4') "Watch backoff: the first four backoff lines say seconds 1, 2, 4, 4 (got $($b -join ','))"
    Write-Result (@($b | Where-Object { $_ -gt 4 }).Count -eq 0) 'Watch backoff: no wait is longer than BackoffMaxSeconds'
  }

  # -Watch, backoff reset: a holder that lived past the fast-failure threshold clears the doubling.
  # The third start lives about 10 seconds and the threshold is 2 (-FastFailureSeconds, assumed
  # name), so a look finds it alive past the threshold with a wide margin. Its kept passes take
  # about 10 of the 20 passes.
  Invoke-Case 'Watch: backoff resets after a long-lived holder' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $true
    $threshold = if ($script:mut) { '60' } else { '2' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '20', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '4', '-FastFailureSeconds', $threshold) $fail
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch backoff reset: the loop ends on its own with exit 0'
    $starts = Join-Path $env:AGENTSPACE_STATE_DIR 'starts.txt'
    $n = if (Test-Path -LiteralPath $starts) { [int](Get-Content -LiteralPath $starts -TotalCount 1) } else { 0 }
    Write-Result ($n -ge 4) "Watch backoff reset: the holder was started at least four times (started $n)"
    $b = Get-BackoffSeconds (Read-LogLines)
    Write-Result ($b.Count -ge 3 -and $b[0] -eq 1 -and $b[1] -eq 2) "Watch backoff reset: the waits before the long-lived start are 1 then 2 (got $($b -join ','))"
    Write-Result ($b.Count -ge 3 -and $b[2] -eq 1) 'Watch backoff reset: the first wait after the long-lived holder is back at the base, 1'
  }

  # -Watch, a holder that dies unproven: every start lives about 6 seconds, past the 4-second
  # interval but short of the 7-second threshold. A look finds each one alive but unproven, and the
  # next look finds it dead more than 7 seconds after its start. Only proof of life resets the
  # count, so each death MUST count as a fast failure, and the backoff lines appear and double.
  Invoke-Case 'Watch: a holder that dies unproven backs off' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $fail = New-FailStub $false 6
    $threshold = if ($script:mut) { '3' } else { '7' }
    $p = Start-Watch @('-IntervalSeconds', '4', '-MaxPasses', '5', '-BackoffBaseSeconds', '4', '-BackoffMaxSeconds', '16', '-FastFailureSeconds', $threshold) $fail
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch unproven: the loop ends on its own with exit 0'
    $n = Get-StartCount
    Write-Result ($n -ge 3) "Watch unproven: the holder was started at least three times (started $n)"
    $b = Get-BackoffSeconds (Read-LogLines)
    Write-Result ($b.Count -ge 2 -and $b[0] -eq 4 -and $b[1] -eq 8) "Watch unproven: backoff lines appear after each death, and double, 4 then 8 (got $($b -join ','))"
  }

  # -Watch, a log that cannot be written: board-holder.log is held without write sharing from before
  # the first pass until after the first backoff, then released. -Watch MUST go on through it, and
  # finish its passes with exit 0.
  Invoke-Case 'Watch: a log that cannot be written does not end the loop' {
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
      $first = Wait-Until { (Get-StartCount) -ge 1 } 60
      # The first start fails at once, and its backoff of 1 second, then the next one's 2, fall
      # inside this hold.
      Start-Sleep -Seconds 5
      $triesHeld = Get-StartCount
      $running = -not $p.HasExited
    } finally { $held.Dispose() }
    $code = Wait-Watch $p 240
    Write-Result $first 'Watch log held: the keeper tried a holder while the log was held'
    Write-Result ($triesHeld -ge 2) "Watch log held: the keeper tried again after a backoff while the log was held (tries $triesHeld)"
    Write-Result $running 'Watch log held: -Watch is still running when the log is released'
    Write-Result ($code -eq 0) "Watch log held: the loop ends on its own with exit 0 (got $code)"
    $n = Get-StartCount
    Write-Result ($n -eq 8) "Watch log held: all 8 passes ran, one try each (tries $n)"
    Write-Result (@(Read-LogLines | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ backoff seconds=\d+\z' }).Count -ge 1) 'Watch log held: backoff lines are logged once the log is free'
  }

  # -Watch, -MaxPasses: the process ends by itself after that many passes.
  Invoke-Case 'Watch: MaxPasses ends the loop' {
    Write-Leases @((New-ReleasedRecord 'a' $farFuture))
    $max = if ($script:mut) { '2' } else { '3' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', $max) $stub
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch MaxPasses: the process exits 0 on its own, within 60 seconds'
    $log = Read-LogLines
    $idle = @($log | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle(\s|\z)' })
    Write-Result ($idle.Count -eq 3) "Watch MaxPasses: exactly 3 passes ran, 3 idle lines (got $($idle.Count))"
    Write-Result ((Get-StubProcesses).Count -eq 0) 'Watch MaxPasses: no holder is started'
  }

  # -Watch, no tight loop: passes are IntervalSeconds apart, and a failing holder's starts are further apart.
  Invoke-Case 'Watch: passes are spaced' {
    Write-Leases @((New-ReleasedRecord 'a' $farFuture))
    $interval = 2
    $max = if ($script:mut) { '1' } else { '4' }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Watch @('-IntervalSeconds', "$interval", '-MaxPasses', $max) $stub
    $code = Wait-Watch $p 240
    $sw.Stop()
    Write-Result ($code -eq 0) 'Watch spacing: the process exits 0 on its own'
    Write-Result ($sw.Elapsed.TotalSeconds -ge 5.5) "Watch spacing: 4 passes at 2 seconds took at least 6 seconds, less a rounding margin (took $([Math]::Round($sw.Elapsed.TotalSeconds, 1)))"
    $tlog = Read-LogLines
    $times = @($tlog | ForEach-Object { Get-LogTime $_ } | Where-Object { $_ })
    Write-Result ($times.Count -eq 4) "Watch spacing: one timed line per pass (got $($times.Count))"
    $gapsOk = $times.Count -ge 2
    for ($i = 1; $i -lt $times.Count; $i++) { if (($times[$i] - $times[$i - 1]).TotalSeconds -lt 1) { $gapsOk = $false } }
    Write-Result $gapsOk 'Watch spacing: every pass is at least IntervalSeconds apart, less a one-second rounding margin on the log time'
    Write-Result ($times.Count -ge 2 -and ($times[-1] - $times[0]).TotalSeconds -ge 5) 'Watch spacing: the first and last pass are at least 6 seconds apart, less the margin'

    # A holder that fails at once: starts are spaced by the backoff, not by the interval alone.
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    Remove-Item -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.log') -ErrorAction SilentlyContinue
    $fail = New-FailStub $false
    $base = if ($script:mut) { '1' } else { '3' }
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '10', '-BackoffBaseSeconds', $base, '-BackoffMaxSeconds', '3') $fail
    $code = Wait-Watch $p 240
    $slog = Read-LogLines
    $started = @($slog | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ (started|failed)(\s|\z)' } | ForEach-Object { Get-LogTime $_ })
    Write-Result ($started.Count -ge 2) "Watch spacing: a failing holder is tried more than once (a start or a failed line, each one try) (got $($started.Count))"
    $startGaps = $true
    for ($i = 1; $i -lt $started.Count; $i++) { if (($started[$i] - $started[$i - 1]).TotalSeconds -lt 2) { $startGaps = $false } }
    Write-Result ($started.Count -ge 2 -and $startGaps) 'Watch spacing: tries of a failing holder are at least 3 seconds apart, less a one-second margin'
  }

  # -Watch, lock busy: the pass is logged as skipped, and the loop carries on once the lock is free.
  Invoke-Case 'Watch: a locked pass is skipped and the loop carries on' {
    Write-Leases @((New-ReleasedRecord 'a' $farFuture))
    $lockPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.lock'
    $held = $null
    if (-not $script:mut) { $held = [IO.FileStream]::new($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
    try {
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '6') $stub
      if (-not $script:mut) { Wait-Until { @(Read-LogLines | Where-Object { $_ -cmatch ' skipped .*reason=locked' }).Count -ge 1 } 30 | Out-Null }
    } finally { if ($held) { $held.Dispose() } }
    $code = Wait-Watch $p 240
    Write-Result ($code -eq 0) 'Watch locked: the loop ends on its own with exit 0'
    $log = Read-LogLines
    $skipped = @($log | Where-Object { $_ -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ skipped lease=- alias=- pid=- reason=locked\z' })
    Write-Result ($skipped.Count -ge 1) "Watch locked: at least one line ""skipped lease=- alias=- pid=- reason=locked"" is logged (got $($skipped.Count))"
    $firstSkip = -1
    for ($i = 0; $i -lt $log.Count; $i++) { if ($log[$i] -cmatch ' skipped ') { $firstSkip = $i; break } }
    $idleAfter = 0
    if ($firstSkip -ge 0) { for ($i = $firstSkip + 1; $i -lt $log.Count; $i++) { if ($log[$i] -cmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ idle(\s|\z)') { $idleAfter++ } } }
    Write-Result ($idleAfter -ge 1) 'Watch locked: a later pass ran and logged idle, so the loop carried on'
  }

  # The log holds no secret: no token-like string, key path or certificate path, whatever the passes did.
  Invoke-Case 'Watch: the log holds no secret' {
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
    Write-Result ($code -eq 0) 'Watch secrets: the loop ends on its own with exit 0'
    $log = Read-LogLines
    Write-Result ($log.Count -ge 2) "Watch secrets: the log has lines to check (got $($log.Count))"
    $text = $log -join "`n"
    Write-Result (-not $text.Contains($sentinel)) 'Watch secrets: a token in the environment is not logged'
    Write-Result ($text -cnotmatch '-----BEGIN') 'Watch secrets: no private key block'
    Write-Result ($text -inotmatch '\.(pem|key|crt|cer|pfx|p12)\b|id_(rsa|ed25519|ecdsa)|[\\/]\.ssh[\\/]') 'Watch secrets: no key or certificate path'
    Write-Result ($text -cnotmatch '[A-Za-z0-9+/]{32,}') 'Watch secrets: no token-like run of 32 or more characters'
    Write-Result ($text -inotmatch '(token|secret|password|passwd|bearer)\s*[=:]\s*\S{8,}') 'Watch secrets: no token=, secret= or password= value'
  }

  # The installer's dry run prints the logon task and changes nothing. The installer is run with
  # -DryRun only; the test never registers, changes or removes a task.
  Invoke-Case 'Installer: -DryRun prints the logon task and changes nothing' {
    $installer = Join-Path $repo 'scripts\install-board-holder.ps1'
    $taskName = 'agentspace board holder'
    # The task's whole definition as XML, or '' when there is none. The real task is registered on
    # Terry's laptop since E0.02-T03-S01, so the dry run is checked to leave it exactly as it was,
    # whether or not it exists.
    $getTask = { try { $t = Export-ScheduledTask -TaskName $taskName -ErrorAction Stop; if ($null -eq $t) { '' } else { [string]$t } } catch { '' } }
    Write-Result (Test-Path -LiteralPath $installer) 'Installer: scripts\install-board-holder.ps1 exists'
    $taskBefore = & $getTask
    if (-not (Test-Path -LiteralPath $installer)) { return }
    $out = Join-Path $env:AGENTSPACE_STATE_DIR 'install.out'
    $err = Join-Path $env:AGENTSPACE_STATE_DIR 'install.err'
    $p = Start-Process -FilePath $pwsh -ArgumentList @('-NoProfile', '-File', $installer, '-DryRun') -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
    $null = $p.Handle
    $code = if ($p.WaitForExit(60000)) { $p.ExitCode } else { Stop-OwnTree $p.Id; -999 }
    $text = if (Test-Path -LiteralPath $out) { Get-Content -LiteralPath $out -Raw } else { '' }
    if ($null -eq $text) { $text = '' }
    $lines = @($text -split "`r?`n")
    Write-Result ($code -eq 0) 'Installer: -DryRun exits 0'
    $limited = if ($script:mut) { 'Highest' } else { 'Limited' }
    $ignore = if ($script:mut) { 'Parallel' } else { 'IgnoreNew' }
    $user = [regex]::Escape($env:USERNAME)
    Write-Result ($text -imatch 'agentspace board holder') 'Installer: the task is named "agentspace board holder"'
    Write-Result ($text -imatch 'log\s*on' -and $text -imatch ('(\\|\s|^)' + $user + '\b')) 'Installer: a logon trigger for the current user'
    Write-Result (@($lines | Where-Object { $_ -cmatch '\ATrigger \d+: ' }).Count -eq 2) 'Installer: the task has two triggers'
    Write-Result (@($lines | Where-Object { $_ -cmatch '\ATrigger 1: AtLogOn user=' }).Count -eq 1) 'Installer: trigger 1 is the logon trigger'
    $t2 = @($lines | Where-Object { $_ -cmatch '\ATrigger 2: Once' })
    Write-Result ($t2.Count -eq 1) 'Installer: trigger 2 is a time trigger starting at registration'
    Write-Result ($t2.Count -eq 1 -and $t2[0] -cmatch ' EndBoundary=none\b') 'Installer: trigger 2 has no end boundary'
    $startOk = $false
    if ($t2.Count -eq 1 -and $t2[0] -cmatch ' StartBoundary=(\S+) ') {
        $sb = [DateTimeOffset]::MinValue
        if ([DateTimeOffset]::TryParse($Matches[1], [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AssumeLocal, [ref]$sb)) {
            $startOk = $sb -le [DateTimeOffset]::Now.AddSeconds(5) -and $sb -ge [DateTimeOffset]::Now.AddMinutes(-5)
        }
    }
    Write-Result $startOk 'Installer: trigger 2 starts at or before the dry run, within a few minutes'
    $interval = if ($script:mut) { 'PT5M' } else { 'PT1M' }
    Write-Result (@($lines | Where-Object { $_ -ceq "Repetition 2: Interval=$interval Duration=indefinite" }).Count -eq 1) "Installer: trigger 2 repeats every minute ($interval), indefinitely"
    Write-Result (@($lines | Where-Object { $_ -cmatch '\ARepetition 1: ' }).Count -eq 0) 'Installer: the logon trigger does not repeat'
    Write-Result ($text -imatch "\b$limited\b") "Installer: run level $limited"
    Write-Result ($text -imatch '(no\s+(execution\s+)?time\s*limit|time\s*limit\W+(none|no|off|unlimited|disabled|0)|\bPT0S\b|unlimited)') 'Installer: no execution time limit'
    Write-Result ($text -imatch "\b$ignore\b") "Installer: a second start is handled as $ignore"
    Write-Result ($text -imatch 'start\s*when\s*available|a missed start|missed start') 'Installer: a missed start runs when available'
    $action = '-NoProfile\s+-NonInteractive\s+-File\s+"?' + [regex]::Escape((Join-Path $repo 'scripts\board-holder.ps1')) + '"?\s+-Watch\b'
    Write-Result (@($lines | Where-Object { $_ -imatch 'pwsh(\.exe)?"?\s' -and $_ -imatch $action }).Count -ge 1) 'Installer: the action runs pwsh -NoProfile -NonInteractive -File <repo>\scripts\board-holder.ps1 -Watch'
    Write-Result ($text -imatch 'no[\s-]*window|hidden|windowstyle|CreateNoWindow|headless|wscript|conhost') 'Installer: the launcher shows no window'
    # The whole definition, one "Label: value" line per field of the task object.
    $field = { param($Label) $m = @($lines | Where-Object { $_ -cmatch ('\A' + [regex]::Escape($Label) + ': ') }); if ($m.Count -eq 1) { $m[0].Substring($Label.Length + 2) } else { $null } }
    $logon = if ($script:mut) { 'S4U' } else { 'Interactive' }
    Write-Result ((& $field 'LogonType') -ceq $logon) "Installer: LogonType: $logon"
    Write-Result ((& $field 'Principal user') -imatch ('\\' + $user + '\z')) 'Installer: Principal user is the current user'
    Write-Result ((& $field 'DisallowStartIfOnBatteries') -ceq 'False' -and (& $field 'StopIfGoingOnBatteries') -ceq 'False') 'Installer: starts and keeps running on battery'
    $restarts = if ($script:mut) { '5' } else { '3' }
    Write-Result ((& $field 'RestartCount') -ceq $restarts -and (& $field 'RestartInterval') -ceq 'PT1M') "Installer: RestartCount: $restarts and RestartInterval: PT1M"
    Write-Result ([bool](& $field 'Description')) 'Installer: the description is printed'
    Write-Result ((& $field 'WorkingDirectory') -ceq $repo) 'Installer: WorkingDirectory is the repo root'
    Write-Result ((& $field 'Action Execute') -imatch '\\\.agentspace\\BoardHolderLauncher\.exe\z') 'Installer: Action Execute is the launcher in .agentspace'
    $argsLine = & $field 'Arguments'
    Write-Result ($argsLine -and $argsLine.Contains('"' + (Join-Path $repo 'scripts\board-holder.ps1') + '" -Watch')) 'Installer: Arguments name pwsh, the keeper and -Watch'
    Write-Result (@($lines | Where-Object { $_ -imatch '\Astart: .*starts the task immediately' }).Count -eq 1) 'Installer: it says a real install starts the task immediately'
    Write-Result (@($lines | Where-Object { $_ -cmatch '\AMode: dry run' }).Count -eq 1) 'Installer: it says this was a dry run'
    Write-Result ((& $getTask) -ceq $taskBefore) 'Installer: the task named "agentspace board holder" is as it was before the dry run, or still absent'
  }

  # The installer's dry run warns when the repo is a git worktree rather than the main checkout, and
  # still exits 0. The expectation is worked out here the same way: a git dir that differs from the
  # common git dir, or a path under .claude\worktrees. Run from a worktree, the warning MUST appear.
  Invoke-Case 'Installer: -DryRun warns from a worktree' {
    $installer = Join-Path $repo 'scripts\install-board-holder.ps1'
    $gitDir = (& git -C $repo rev-parse --path-format=absolute --git-dir 2>$null) | Select-Object -First 1
    $common = (& git -C $repo rev-parse --path-format=absolute --git-common-dir 2>$null) | Select-Object -First 1
    $isWorktree = ($repo -match '[\\/]\.claude[\\/]worktrees[\\/]') -or
      ($gitDir -and $common -and -not [string]::Equals([IO.Path]::GetFullPath($gitDir).TrimEnd('\', '/'), [IO.Path]::GetFullPath($common).TrimEnd('\', '/'), [StringComparison]::OrdinalIgnoreCase))
    if ($script:mut) { $isWorktree = -not $isWorktree }
    $out = Join-Path $env:AGENTSPACE_STATE_DIR 'install.out'
    $err = Join-Path $env:AGENTSPACE_STATE_DIR 'install.err'
    $p = Start-Process -FilePath $pwsh -ArgumentList @('-NoProfile', '-File', $installer, '-DryRun') -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
    $null = $p.Handle
    $code = if ($p.WaitForExit(60000)) { $p.ExitCode } else { Stop-OwnTree $p.Id; -999 }
    $lines = @((Get-Content -LiteralPath $out -ErrorAction SilentlyContinue))
    $warn = @($lines | Where-Object { $_ -cmatch '\AWarning: .*worktree' })
    Write-Result ($code -eq 0) 'Installer worktree: -DryRun exits 0'
    if ($isWorktree) { Write-Result ($warn.Count -eq 1) "Installer worktree: the repo is a worktree, and one Warning: line says so (got $($warn.Count))" }
    else { Write-Result ($warn.Count -eq 0) "Installer worktree: the repo is the main checkout, and no Warning: line is printed (got $($warn.Count))" }
  }

} finally {
  $env:AGENTSPACE_STATE_DIR = $savedState
  Stop-Stubs
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
}

if (-not $SelfCheck) { $script:mut = $false; Write-Result ((Get-StubProcesses).Count -eq 0) 'The run leaves no stub process behind' }
if ($SelfCheck) { Write-Output "Self check: $failures case(s) cannot fail." }
else { Write-Output "$passes passed, $failures failed." }
if ($failures) { exit 1 }

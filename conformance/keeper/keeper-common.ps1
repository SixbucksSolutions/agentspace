#Requires -Version 7
# A conformance helper under D34, shared by the keeper's checks: it starts the keeper only as a separate process, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The helpers that conformance/keeper/test-keeper.ps1 and conformance/keeper/test-unmet-holder-dies-mid-pass.ps1
# share. Each dot-sources this file once, on Windows:
#   . (Join-Path $PSScriptRoot 'keeper-common.ps1')
# and then sets $keeperCmd to Resolve-KeeperProgram's answer and calls Initialize-KeeperRun, both
# before the first case: a case strips every AGENTSPACE_* variable from the environment, that
# one among them. New-CaseHome then names four tripwires in the case's home (AGENTSPACE_GUARD_CMD,
# AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD, AGENTSPACE_SCP_CMD), and Get-KeeperTripwireStarts counts
# the starts each recorded. The caller MUST set $ErrorActionPreference = 'Stop'. Nothing here loads prototype
# code, and nothing names the prototype's keeper (docs/conformance.md, "Naming the program under test").
#
# What it touches:
# - Every case has its own throwaway folder under the run's folder, which is
#   $env:TEMP\agentspace-conformance-keeper-<guid>: a state folder, named by AGENTSPACE_STATE_DIR, with
#   a hand-written leases.json, and a home folder, named by USERPROFILE.
# - Every process a case starts, the keeper and the stand-ins included, gets the
#   environment that the harness's Get-ConformanceChildEnvironment builds for the case's home
#   (conformance/lease-cli/harness.ps1): USERPROFILE, HOME, LOCALAPPDATA, APPDATA, AGENTSPACE_STATE_DIR
#   and AGENTSPACE_SSH_DIR inside the case's folder, AWS_CONFIG_FILE and AWS_SHARED_CREDENTIALS_FILE
#   naming files that do not exist, AWS_EC2_METADATA_DISABLED=true, no AWS profile, key, session or
#   container-credential variable, and no AGENTSPACE_* or PSModulePath variable the caller's shell
#   held. Set-CaseEnvironment applies it to this process, which every child inherits, and
#   Restore-CaseEnvironment puts every variable back, in the caller's finally. The keeper's holders
#   are the exception (keeper.md, "Streams and environment"): the keeper starts them, and the
#   prototype's launcher, through WMI, so they get the user's logon environment and not this one.
#   That is safe only because the holder is the ssh stub, which touches nothing outside the run's
#   folder; a holder that was the real ssh would find the real home through the account.
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
#   keeper cannot open it to stop it. Lock-Process records the stub's Process object, whose handle pins
#   the PID, and its start time. Cleanup gives that right back, through that handle, to that stub
#   alone and only after its start time matches: it never rewrites the DACL of a process that the
#   case did not lock, or that it cannot prove is the one it locked.

$ci = [Globalization.CultureInfo]::InvariantCulture

# The harness resolves the program under test and builds the environment of every child.
. (Join-Path $PSScriptRoot '..\lease-cli\harness.ps1')

# The full path of the program that AGENTSPACE_KEEPER_CMD names, or its default from
# conformance/defaults.ps1, through the harness; $null when neither names one.
function Resolve-KeeperProgram {
  $p = Get-ConformanceProgram -Variable 'AGENTSPACE_KEEPER_CMD'
  if ([string]::IsNullOrWhiteSpace($p)) { return $null }
  [IO.Path]::GetFullPath($p)
}

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
        [DllImport("advapi32.dll", SetLastError = true)] static extern bool SetKernelObjectSecurity(IntPtr h, uint information, byte[] descriptor);
        const uint DACL_SECURITY_INFORMATION = 0x4;
        // Sets the DACL of the process that a handle the caller holds names, never one found by PID.
        public static void SetByHandle(IntPtr handle, string sddl)
        {
            var sd = new RawSecurityDescriptor(sddl);
            byte[] bytes = new byte[sd.BinaryLength];
            sd.GetBinaryForm(bytes, 0);
            if (!SetKernelObjectSecurity(handle, DACL_SECURITY_INFORMATION, bytes)) throw new Win32Exception(Marshal.GetLastWin32Error());
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
# Every right for the user and SYSTEM; the locked list adds a deny of PROCESS_TERMINATE (0x1) for everyone first.
$openAcl = "D:(A;;0x1FFFFF;;;$userSid)(A;;0x1FFFFF;;;SY)"
$lockedAcl = "D:(D;;0x1;;;WD)(A;;0x1FFFFF;;;$userSid)(A;;0x1FFFFF;;;SY)"

# The processes whose DACL a case locked, by PID: the Process object the check started (its handle
# pins the PID, so the PID cannot be reused while the entry exists) and the start time it had.
$script:lockedProcesses = @{}

# Denies PROCESS_TERMINATE on a stub that Start-Stub returned. Throws when the list cannot be set.
function Lock-Process($Stub) {
  $handle = $Stub.Process.SafeHandle.DangerousGetHandle()
  [AgentspaceTest.ProcessAcl]::SetByHandle($handle, $lockedAcl)
  $script:lockedProcesses[[int]$Stub.Pid] = [pscustomobject]@{ Process = $Stub.Process; Started = $Stub.Process.StartTime.ToUniversalTime() }
}

# Gives PROCESS_TERMINATE back to the one process a case locked, and to no other. The process is
# pinned by the handle the case kept, and its start time MUST equal both the time recorded at the
# lock and the creation time of the entry in the snapshot that Stop-OwnTree is working from; the
# DACL of a PID that fails that, or that no case locked, is never rewritten.
function Unlock-LockedProcess([int]$ProcessId, [DateTime]$CreationDate) {
  $entry = $script:lockedProcesses[$ProcessId]
  if ($null -eq $entry) { return }
  try {
    if ($entry.Process.Id -ne $ProcessId) { return }
    $start = $entry.Process.StartTime.ToUniversalTime()
    if ([Math]::Abs(($start - $entry.Started).TotalMilliseconds) -ge 1) { return }
    if ([Math]::Abs(($start - $CreationDate.ToUniversalTime()).TotalMilliseconds) -ge 1) { return }
    [AgentspaceTest.ProcessAcl]::SetByHandle($entry.Process.SafeHandle.DangerousGetHandle(), $openAcl)
  } catch { }
  $script:lockedProcesses.Remove($ProcessId)
}

# The run's own folder, and a copy of the stub whose command line is unique to this run. The folder
# is directly under $env:TEMP and named agentspace-conformance-*, as the harness's homes are.
function Initialize-KeeperRun {
  $script:temp = Join-Path $env:TEMP "agentspace-conformance-keeper-$([guid]::NewGuid().ToString('N'))"
  # The run's folder is a conformance home's root, so it is checked as one (Assert-ConformanceHomeRoot,
  # conformance.md, "The program under test is untrusted"); every write and delete under it is checked
  # at the call against $runHome, which names it as the root (Assert-ConformancePath).
  New-Item -ItemType Directory -Force -Path (Assert-ConformanceHomeRoot -Root $script:temp) | Out-Null
  $script:runHome = [pscustomobject]@{ Root = $script:temp }
  $script:stub = Join-Path $script:temp 'ssh-stub.cmd'
  Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'ssh-stub.cmd') -Destination (Assert-ConformancePath -ConformanceHome $runHome -Path $script:stub)
}

# Every environment variable a case changes, with the value it had before the run, put back by
# Restore-CaseEnvironment in the caller's finally.
$script:savedEnvironment = @{}

# A $null value removes the variable. PowerShell turns a $null passed to SetEnvironmentVariable into an
# empty string, which would leave an empty variable behind for every child to see, so a removal goes
# through the Env: drive.
function Set-ProcessVariable([string]$Name, $Value) {
  if ($null -eq $Value) { Remove-Item -LiteralPath "Env:\$Name" -ErrorAction SilentlyContinue }
  else { [Environment]::SetEnvironmentVariable($Name, [string]$Value) }
}

function Set-CaseEnvironment($ChildEnvironment) {
  foreach ($name in $ChildEnvironment.Keys) {
    if (-not $script:savedEnvironment.ContainsKey($name)) { $script:savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name) }
    Set-ProcessVariable $name $ChildEnvironment[$name]
  }
}

function Restore-CaseEnvironment {
  foreach ($name in @($script:savedEnvironment.Keys)) { Set-ProcessVariable $name $script:savedEnvironment[$name] }
  $script:savedEnvironment.Clear()
  Write-KeeperTripwireTotals
}

# The tripwires of v0.08 (E0.08-T03-S03; conformance.md, "The harness"; keeper.md, "Conformance cases for
# the guard's start"). Every case's environment names one for AGENTSPACE_GUARD_CMD, and one each for the
# operator's test seam, AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD and AGENTSPACE_SCP_CMD, so that a keeper
# that starts a guard, or a guard that starts aws, ssh or scp, reaches nothing real: unset, the guard
# is the build's own agentspace-operator.exe, which sees every host in a throwaway home as unexplained.
# A tripwire is a .cmd in <home>\tripwire that appends one line to <name>-starts.log beside itself and
# exits 1 at once. It records no argument and no variable, so that an argument holding a shell
# character cannot change what it does. A keeper starts a .cmd through cmd.exe, as keeper.md gives.
$script:tripwireNames = [ordered]@{ guard = 'AGENTSPACE_GUARD_CMD'; aws = 'AGENTSPACE_AWS_CMD'; ssh = 'AGENTSPACE_SSH_CMD'; scp = 'AGENTSPACE_SCP_CMD' }
$script:tripwireDir = $null
$script:tripwireTotals = [ordered]@{ guard = 0; aws = 0; ssh = 0; scp = 0 }

# Adds the current case's tripwire starts to the run's totals, once, before its home is replaced or
# removed.
function Add-KeeperTripwireTotals {
  if ($null -eq $script:tripwireDir) { return }
  foreach ($name in @($script:tripwireTotals.Keys)) { $script:tripwireTotals[$name] += Get-KeeperTripwireStarts $name }
  $script:tripwireDir = $null
}

# Prints the run's tripwire starts, one line, so a run against a build shows that the keeper started
# its guard and that nothing reached aws, ssh or scp. It prints counts only, never a path.
function Write-KeeperTripwireTotals {
  Add-KeeperTripwireTotals
  $t = $script:tripwireTotals
  "Tripwire starts in this run: guard $($t.guard), aws $($t.aws), ssh $($t.ssh), scp $($t.scp)."
}

# Writes the four tripwires into the home's tripwire folder, each write checked at the call, and returns
# the variables that name them.
function New-KeeperTripwires($CaseHome) {
  Add-KeeperTripwireTotals
  $dir = Join-Path $CaseHome.UserProfile 'tripwire'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $runHome -Path $dir) | Out-Null
  $script:tripwireDir = $dir
  $named = [ordered]@{}
  foreach ($name in $script:tripwireNames.Keys) {
    $path = Join-Path $dir "$name-tripwire.cmd"
    $body = ('@echo off', ">>`"%~dp0$name-starts.log`" echo Started", 'exit /b 1') -join "`r`n"
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path $path), $body + "`r`n")
    $named[$script:tripwireNames[$name]] = $path
  }
  $named
}

# The number of times the named tripwire (guard, aws, ssh or scp) was started in the current case's
# home: the lines of its <name>-starts.log, which is 0 when the file does not exist. Read shared, so a
# tripwire that is appending is never blocked.
function Get-KeeperTripwireStarts([Parameter(Mandatory)][ValidateSet('guard', 'aws', 'ssh', 'scp')][string]$Name) {
  if ($null -eq $script:tripwireDir) { return 0 }
  $p = Join-Path $script:tripwireDir "$Name-starts.log"
  if (-not (Test-Path -LiteralPath $p)) { return 0 }
  try {
    $fs = [IO.File]::Open($p, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try { $text = [IO.StreamReader]::new($fs, [Text.UTF8Encoding]::new($false)).ReadToEnd() } finally { $fs.Dispose() }
    @($text -split "`r?`n" | Where-Object { $_ -ne '' }).Count
  } catch { 0 }
}

# Makes a case's throwaway folders and puts the harness's child environment for them on this
# process, which every process the case starts inherits. The environment names the four tripwires
# (above), set after the harness has stripped the caller's AGENTSPACE_* variables. A case that names
# its own value for one of them passes it in -Environment (a hashtable, as the harness's -Override
# takes it; an empty value or $null removes the variable), and then gets no tripwire for that
# variable: a case's own value wins. Returns the home object the harness takes:
# Root (the run's folder), UserProfile, StateDir and SshDir. The state folder is beside the home.
function New-CaseHome([hashtable]$Environment = @{}) {
  $dir = Join-Path $temp ([guid]::NewGuid().ToString('N'))
  $userProfile = Join-Path $dir 'home'
  $h = [pscustomobject]@{ Root = $temp; UserProfile = $userProfile; StateDir = Join-Path $dir 'state'; SshDir = Join-Path $userProfile '.ssh' }
  New-Item -ItemType Directory -Force -Path @($h.StateDir, $h.SshDir, (Join-Path $userProfile 'AppData\Local'), (Join-Path $userProfile 'AppData\Roaming') | ForEach-Object { Assert-ConformancePath -ConformanceHome $runHome -Path $_ }) | Out-Null
  $child = Get-ConformanceChildEnvironment -ConformanceHome $h -Override $Environment
  $tripwires = New-KeeperTripwires $h
  foreach ($variable in $tripwires.Keys) {
    if (-not $Environment.ContainsKey($variable)) { $child[$variable] = $tripwires[$variable] }
  }
  Set-CaseEnvironment $child
  Write-CaseRegistry $h
  $h
}

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
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path (Join-Path $env:AGENTSPACE_STATE_DIR 'leases.json')), $text)
}

function Format-Utc([DateTime]$Time) { $Time.ToUniversalTime().ToString("yyyy-MM-dd\THH:mm:ss.fff\Z", $ci) }

function Write-Record([int]$ProcessId, [string]$StartedAt, [string]$LeaseId, [string]$Alias) {
  $text = [ordered]@{ pid = $ProcessId; process_started_at = $StartedAt; lease_id = $LeaseId; alias = $Alias } | ConvertTo-Json -Compress
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json')), $text + "`n")
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
    $lines = @($text -split "`r?`n" | Where-Object { $_ -ne '' })
    Register-LogLines $lines
    , $lines
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
# Leaves go first. The one process whose DACL a case locked, if it is in the tree, gets its right
# back first, through the handle the case kept and only when its start time matches
# (Unlock-LockedProcess); no other process's DACL is touched. -SparingHolders
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
    Unlock-LockedProcess ([int]$m.ProcessId) $m.CreationDate
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
# start time as the record writes it, its children (the ping among them) and the Process object,
# whose handle pins the PID for as long as the object is held.
function Start-Stub([string]$Alias) {
  $p = Start-Process -FilePath $stub -ArgumentList @('-N', '-o', 'ExitOnForwardFailure=yes', $Alias) -PassThru -WindowStyle Hidden
  $null = $p.Handle
  Wait-Until {
    @(Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId=$($p.Id) AND Name='PING.EXE'" -ErrorAction SilentlyContinue).Count -gt 0
  } 30 | Out-Null
  $kids = @(Get-ChildPids $p.Id)
  [pscustomobject]@{ Pid = $p.Id; Started = (Format-Utc $p.StartTime); Children = $kids; Process = $p }
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
  $p = Start-Process -FilePath $keeperCmd -ArgumentList (ConvertTo-ArgString $KeeperArgs) -PassThru -WindowStyle Hidden -RedirectStandardOutput (Assert-ConformancePath -ConformanceHome $runHome -Path $out) -RedirectStandardError (Assert-ConformancePath -ConformanceHome $runHome -Path $err)
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

function Test-LogLine([string]$Line, [string]$Action, [string]$LeaseId, [string]$Alias, $ProcessId) {
  $Line -cmatch ('\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ ' + $Action + ' lease=' + [regex]::Escape($LeaseId) + ' alias=' + [regex]::Escape($Alias) + ' pid=' + $ProcessId + '(\s|\z)')
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
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path $path), (($lines -join "`r`n") + "`r`n"))
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


# The launch files of one start (state.md, "A holder's launch files"): a request, a claim and a result
# sharing one token of 32 lowercase hex digits, matched case-sensitively (D35). Returns the first
# token for which all three names are in $Names, as an object with Token, Request, Claim and Result,
# or $null when no token has all three.
function Find-LaunchSet([string[]]$Names) {
    foreach ($n in $Names) {
        if ($n -cmatch '\Aboard-holder-launch\.([0-9a-f]{32})\.json\z') {
            $t = $Matches[1]
            $res = "board-holder-launch.$t.result.json"; $claim = "board-holder-launch.$t.claim"
            if (($Names -ccontains $res) -and ($Names -ccontains $claim)) {
                return [pscustomobject]@{ Token = $t; Request = $n; Claim = $claim; Result = $res }
            }
        }
    }
    return $null
}

# The registry modes (keeper.md, "Conformance cases for a host per project", H1). Every v0.06 case of
# the keeper runs twice: with no registry (-Registry None), and with a registry that holds the box's
# record alone, written by the case into <USERPROFILE>\.agentspace\hosts.json of its throwaway home
# (-Registry Box). -Registry Both, the default of each keeper file, runs the file once for each mode
# as two child processes and ends each result line with " [H1 registry=<mode>]", after the name (and
# after the reason of a FAIL or SKIPPED line), so that a name still starts with the spec section it
# enforces (conformance.md, the result-name rule).
$script:registryMode = 'None'
$script:boxRegistryJson = '{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"}]}'

# Called by each keeper file right after it dot-sources this one, with the file's own -Registry, its
# $PSCommandPath and its $PSBoundParameters. For None or Box it sets the mode and returns; for Both it
# runs the file once per mode, prints the combined summary line and exits with the worst code.
function Initialize-KeeperRegistryMode([string]$Registry, [string]$File, [hashtable]$Bound) {
  if ($Registry -ne 'Both') { $script:registryMode = $Registry; return }
  $pass = 0; $fail = 0; $skip = 0; $code = 0
  foreach ($mode in 'None', 'Box') {
    $childArgs = @('-NoProfile', '-File', $File, '-Registry', $mode)
    foreach ($k in $Bound.Keys) {
      if ($k -eq 'Registry') { continue }
      if ($Bound[$k] -is [System.Management.Automation.SwitchParameter]) { if ($Bound[$k].IsPresent) { $childArgs += "-$k" } }
      else { $childArgs += "-$k"; $childArgs += [string]$Bound[$k] }
    }
    & (Join-Path $PSHOME 'pwsh.exe') @childArgs 2>&1 | ForEach-Object {
      $line = [string]$_
      if ($line -match '^(\d+) passed, (\d+) failed, (\d+) skipped$') { $pass += [int]$Matches[1]; $fail += [int]$Matches[2]; $skip += [int]$Matches[3] }
      elseif ($line -match '^(PASS|FAIL|SKIPPED) ') { Write-Output "$line [H1 registry=$mode]" }
      else { Write-Output $line }
    }
    $c = $LASTEXITCODE
    if ($c -eq 1 -or ($c -ne 0 -and $c -ne 2)) { $code = 1 } elseif ($c -eq 2 -and $code -eq 0) { $code = 2 }
  }
  if ($pass + $fail + $skip -gt 0) { Write-Output "$pass passed, $fail failed, $skip skipped" }
  exit $code
}

# With -Registry Box, writes the registry holding the box's record alone into the case's home, at the
# path the keeper reads (keeper.md, Hosts). Each write is checked at the call.
function Write-CaseRegistry($CaseHome) {
  if ($script:registryMode -ne 'Box') { return }
  $dir = Join-Path $CaseHome.UserProfile '.agentspace'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $runHome -Path $dir) | Out-Null
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path (Join-Path $dir 'hosts.json')), $script:boxRegistryJson + "`n")
}

# The box's log lines, line for line as v0.06 wrote them, but for " host=swimcoach" after pid=<pid> on
# the lines that name a holder (started, kept, stopped, lost, and a failed that names a lease), whatever
# the registry mode (keeper.md, Log lines; H1, H15). Every line Read-LogLines returns is registered, and
# each file reports the problems at its end through Get-LogLineProblems.
$script:logLinesChecked = 0
$script:logLineProblems = [System.Collections.Generic.List[string]]::new()
$script:logLinesSeen = [System.Collections.Generic.HashSet[string]]::new()

function Register-LogLines($Lines) {
  foreach ($l in $Lines) {
    if (-not $script:logLinesSeen.Add([string]$l)) { continue }
    if ($l -notmatch '^\S+ (started|kept|stopped|lost|idle|failed|skipped|rotated|backoff|grace)( |$)') { continue }
    $action = $Matches[1]
    $body = ($l -split ' reason=', 2)[0]
    $script:logLinesChecked++
    $names = $action -in 'started', 'kept', 'stopped', 'lost' -or ($action -eq 'failed' -and $body -notmatch ' lease=- ')
    $hasHost = $body -match ' host='
    $isBox = $body -match ' pid=\S+ host=swimcoach$'
    if ($names -and -not $isBox) { $script:logLineProblems.Add("A $action line that names a holder lacks its final host=swimcoach.") }
    elseif (-not $names -and $hasHost) { $script:logLineProblems.Add("A $action line that names no holder carries a host= field.") }
  }
}

# Returns @{ Checked = <lines checked>; Problems = <distinct problem texts> }.
function Get-LogLineProblems {
  @{ Checked = $script:logLinesChecked; Problems = @($script:logLineProblems | Select-Object -Unique) }
}

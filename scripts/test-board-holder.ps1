#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for the board holder keeper's one pass (story 0.2.T1.2, D30), run through the real
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
# - Cleanup kills, in a finally and after every case, only processes whose command line names this
#   run's copy of the stub, each with its tree (taskkill /T /F, by PID). Nothing else is stopped.
#
# The cases, each its own set of checks: idle; start when a lease is ready; the holder's output goes
# to board-holder-ssh.log; keep a running holder; stop on release, on expiry through -Now, at the
# expiry second itself, and when the lease is gone; start afresh when the recorded PID is dead; leave
# alone a PID reused by another process (start time differs), a process whose start time matches but
# whose command line does not, and a process the keeper did not start that names the lease alias; choose
# the ready lease that expires last; exit 1 when the lease state cannot be read.
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
# Run:  pwsh -NoProfile -File scripts\test-board-holder.ps1
#       pwsh -NoProfile -File scripts\test-board-holder.ps1 -SelfCheck
#       pwsh -NoProfile -File scripts\test-board-holder.ps1 -Keeper <other script>
param(
  [switch]$SelfCheck,
  [string]$Keeper
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

function Stop-Stubs {
  foreach ($p in Get-StubProcesses) { & taskkill.exe /PID $p.ProcessId /T /F 2>&1 | Out-Null }
  Wait-Until { (Get-StubProcesses).Count -eq 0 } 10 | Out-Null
}

# A stand-in process started by the test itself: it is foreign to the keeper. Returns its PID, its
# start time as the record writes it, and its ping child.
function Start-Stub([string]$Alias) {
  $p = Start-Process -FilePath $stub -ArgumentList @('-N', '-o', 'ExitOnForwardFailure=yes', $Alias) -PassThru -WindowStyle Hidden
  $null = $p.Handle
  $kids = @()
  Wait-Until { $script:kids = Get-ChildPids $p.Id; $script:kids.Count -gt 0 } 10 | Out-Null
  $kids = @($script:kids)
  [pscustomobject]@{ Pid = $p.Id; Started = (Format-Utc $p.StartTime); Children = $kids }
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
  if (-not $p.WaitForExit(90000)) {
    & taskkill.exe /PID $p.Id /T /F 2>&1 | Out-Null
    return -999
  }
  $p.ExitCode
}

# One case: a fresh state folder, the body, and a cleanup of every stub process, in a finally.
function Invoke-Case([string]$Name, [scriptblock]$Body) {
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

function Test-StartTimeClose([string]$Recorded, [string]$Actual) {
  if ($Recorded -cnotmatch '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z\z') { return $false }
  $x = [DateTime]::Parse($Recorded, $ci, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal)
  $y = [DateTime]::Parse($Actual, $ci, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal)
  [Math]::Abs(($x - $y).TotalMilliseconds) -le 5
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
    Write-Result ($alive -and $rec -and (Test-StartTimeClose $rec['process_started_at'] $startTime)) 'Start: the record holds that process''s start time, UTC with milliseconds'
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

  # A dead recorded PID: start afresh.
  Invoke-Case 'Dead PID' {
    $r = New-ReadyRecord 'a' '2026-11-17T13:00:00Z'
    if ($script:mut) { $r = New-ReleasedRecord 'a' '2026-11-17T13:00:00Z' }
    Write-Leases @($r)
    $dead = Start-Stub (Get-Alias 'a')
    Write-Record $dead.Pid $dead.Started (Get-Id 'a') (Get-Alias 'a')
    & taskkill.exe /PID $dead.Pid /T /F 2>&1 | Out-Null
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
    Write-Result (Test-Alive $f.Pid) 'Foreign process, lease ready: the foreign process is still alive'
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
} finally {
  $env:AGENTSPACE_STATE_DIR = $savedState
  Stop-Stubs
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
}

if (-not $SelfCheck) { $script:mut = $false; Write-Result ((Get-StubProcesses).Count -eq 0) 'The run leaves no stub process behind' }
if ($SelfCheck) { Write-Output "Self check: $failures case(s) cannot fail." }
else { Write-Output "$passes passed, $failures failed." }
if ($failures) { exit 1 }

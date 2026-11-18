#Requires -Version 7
# A conformance check under D34: it drives the keeper only as a separate process, through what docs/keeper.md documents, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# CHECKS #0068, AND IS EXPECTED TO FAIL AGAINST THE PROTOTYPE. This file checks a requirement the
# prototype does not meet yet (docs/keeper.md, "Release grace", "Requirements the prototype does not
# meet yet"; docs/conformance.md, "Checks of requirements the prototype does not meet yet"). Its
# FAIL lines still say FAIL, and the file exits 1 against the prototype: a runner MUST NOT read that
# as a regression, and MUST NOT turn it into a pass. Every other check file passes against the
# prototype in full. The rewrite's keeper MUST pass this file.
#
# The requirement (keeper.md, "Release grace", card #0068): when a keeper tracks a holder and its pass
# does not find that holder trusted (its process gone, no longer the same process, or the record
# missing or naming another), the pass MUST NOT start a holder, and the keeper waits after that pass:
# the release grace, or the longer backoff if the holder was unproven, logged "grace seconds=<n>" or
# "backoff seconds=<n>". The grace counts from the end of that pass. A holder recorded at the start of
# a pass and gone by the pass's decision therefore counts as ended, and the grace applies at once.
# The prototype finds no trusted holder, removes the record and starts a holder at once, which fails
# on ExitOnForwardFailure while the dead session holds the ports.
#
# Two cases, each against the program that AGENTSPACE_KEEPER_CMD names (or its default, through the
# harness), with the keeper's own arguments unchanged: <program> -Watch -SshExe <stub> ...
#
# 1. A holder that exits while the pass waits for board-holder.lock. keeper.md, "Checking the keeper"
#    and Q9: the check holds the lock, so that the keeper's pass waits for it (up to 10 seconds, then
#    logs "skipped ... reason=locked"), stops the tracked stub while the pass waits, and lets the lock
#    go. The wait is on the keeper's own "skipped" line, the one event that shows its pass waited for
#    the lock once; the keeper logs nothing when its look finds the holder alive and the pass then
#    waits, so no event marks the window itself, and the window is entered by one bounded wait from
#    that line: 1 second of interval, then the look, then a 10-second lock wait, and the check stops
#    the holder 6 seconds after the line, in the middle of a window that is nominally 5 seconds from each
#    edge and nearer 3 under load (the look takes time), and lets the lock go right after the kill. The log then
#    says which path the keeper took, and an attempt that missed the window (a grace or backoff line
#    before the pass's line is the look's own, a skipped or kept line is a pass that was not waiting)
#    is not a result: it is run again, up to 3 times, and a check that never reached the window is
#    SKIPPED, never PASS. No fixed sleep stands in for a condition anywhere else.
# 2. The record is gone at the pass. The same requirement, reached with no timing at all: the look
#    reads the process only, never the record, so a record removed while the holder still runs is a
#    holder that the look finds alive and the pass does not find trusted. The check removes the record
#    while it holds the lock, and the log says whether the keeper's next pass came after it.
#
# What it checks, in both: the pass that does not find the tracked holder trusted logs no "started"
# line (keeper.md, Q6: the check does not depend on the action word it logs instead); the next line is
# the keeper's wait, "grace seconds=<n>" with n about the grace (15); and the next "started" line
# comes at least the grace after the check let the pass go.
#
# What it touches: the same as test-keeper.ps1 (conformance/keeper/keeper-common.ps1): a throwaway
# folder per case under $env:TEMP, the harness's child environment for it on every process, the
# stand-in conformance\keeper\ssh-stub.cmd, and only processes this run started, each stopped by a
# checked PID. The real %USERPROFILE%\.agentspace, ~/.ssh, ssh.exe, keeper task and box are never read,
# written, listed or hashed, and nothing here stops a process this run did not start.
#
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line; exit 0 when every check passed, 1 when any failed, 2 when none failed but any was
# skipped. Against the prototype it takes about 42 seconds, and the rewrite's about 2.
# Run:  pwsh -NoProfile -File conformance\keeper\test-unmet-holder-dies-mid-pass.ps1
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) {
  Write-Output 'SKIPPED keeper.md#where-it-runs: The keeper specified is a Windows keeper; this host is not Windows.'
  Write-Output '0 passed, 0 failed, 1 skipped'
  exit 2
}

. (Join-Path $PSScriptRoot 'keeper-common.ps1')

# Resolved before the first case, which strips AGENTSPACE_* from the environment.
$keeperCmd = Resolve-KeeperProgram
if (-not $keeperCmd -or -not (Test-Path -LiteralPath $keeperCmd -PathType Leaf)) {
  Write-ConformanceResult -Name 'keeper.md#command-line: The program that AGENTSPACE_KEEPER_CMD names exists' -Status FAIL -Reason $(if ($keeperCmd) { "$keeperCmd is not a file." } else { 'Neither the variable nor its default names a program.' })
  Exit-ConformanceRun
}

Initialize-KeeperRun

$farFuture = '2099-01-01T00:00:00Z'
$grace = 15
$attemptsAllowed = 3
$script:caseName = ''

function Add-Check([bool]$Ok, [string]$Text, [string]$Why = '') {
  $name = "keeper.md#release-grace (card #0068): $($script:caseName): $Text"
  if ($Ok) { Write-ConformanceResult -Name $name -Status PASS } else { Write-ConformanceResult -Name $name -Status FAIL -Reason $Why }
}

# One case: a fresh state folder and home folder, with the harness's child environment for them, the
# body, and a cleanup of every process the case started, in a finally.
function Invoke-UnmetCase([string]$Name, [scriptblock]$Body) {
  $null = New-CaseHome
  $script:caseName = $Name
  try { & $Body } catch { Add-Check $false 'No unexpected error' "An error stopped the case: $($_.Exception.Message)" } finally { Stop-RunProcesses }
}

# The lines of the log after the one at the index (-1 gives all of them).
function Get-LinesAfter($Log, [int]$Index) {
  if ($Index + 1 -lt $Log.Count) { @($Log[($Index + 1)..($Log.Count - 1)]) } else { @() }
}

function Get-FirstIndex($Log, [string]$Pattern) {
  for ($i = 0; $i -lt $Log.Count; $i++) { if ($Log[$i] -cmatch $Pattern) { return $i } }
  -1
}

# Starts a keeper that tracks one holder, for a ready lease, with a grace that a stopwatch can tell
# from the backoff (1 second): the wait after a pass that does not find the holder trusted is
# the grace, and the log says "grace seconds=15".
function Start-TrackingKeeper([int]$IntervalSeconds) {
  Write-Leases @((New-ReadyRecord 'a' $farFuture))
  Start-Watch @('-IntervalSeconds', "$IntervalSeconds", '-MaxPasses', '200', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '1',
    '-FastFailureSeconds', '600', '-ReleaseGraceSeconds', "$grace") $stub
}

# Forgets an attempt that did not reach its window: the keeper, every holder, the log and the record.
function Reset-Attempt($Process) {
  Stop-Watch $Process
  Stop-RunProcesses
  foreach ($n in 'board-holder.log', 'board-holder.json') { Remove-Item -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR $n) -ErrorAction SilentlyContinue }
}

# Takes board-holder.lock the way the keeper does (a share mode that refuses a second taker), as soon
# as the keeper's pass lets it go. Returns the stream that holds it, or $null.
function Lock-PassLock($Process) {
  $path = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.lock'
  $box = @{ Stream = $null }
  $got = Wait-Until {
    try { $box.Stream = [IO.FileStream]::new($path, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None); $true } catch [IO.IOException] { $false }
  } 30 $Process
  if ($got) { $box.Stream } else { $null }
}

# Stops the holder that the keeper's record names, and what is below it: only a process whose command
# line is this run's stub with the holder's arguments, and whose start time is the record's.
function Stop-RecordedHolder($Record, [scriptblock]$AfterKill = {}) {
  $holderPid = [int]$Record['pid']
  if (-not (Test-HolderLine (Get-CommandLine $holderPid))) { return $false }
  $p = [Diagnostics.Process]::GetProcessById($holderPid)
  $null = $p.Handle
  if ((Format-Utc $p.StartTime) -cne $Record['process_started_at']) { return $false }
  Stop-OwnTree $holderPid
  & $AfterKill
  Wait-Until { -not (Test-Alive $holderPid) } 30
}

# The keeper's lines after the index, once the pass's line and the wait's line are both there, or the
# deadline has passed.
function Get-Outcome($Process, [int]$Index) {
  [void](Wait-Until { (Get-LinesAfter (Read-LogLines) $Index).Count -ge 2 } 60 $Process)
  @(Get-LinesAfter (Read-LogLines) $Index)
}

# True when the first line after the point is not the pass that did not find the holder trusted: a wait
# line is the look's, a skipped line is a pass that gave up on the lock, and a kept line is a pass that
# ran before the case changed anything.
function Test-WindowMissed($After) {
  $After.Count -eq 0 -or $After[0] -cmatch ' (skipped|kept) ' -or $After[0] -cmatch ' (grace|backoff) seconds='
}

# The checks of a pass that did not find the tracked holder trusted. $After is the log after the point
# the case marked, and $Released is when the case let the pass go.
function Test-GraceFollows($Process, [int]$Index, $After, [DateTime]$Released) {
  $x = $After[0]
  $x2 = if ($After.Count -ge 2) { $After[1] } else { '' }
  Add-Check ($x -cnotmatch ' started ') 'The pass that does not find the tracked holder trusted starts no holder' "That pass logged '$x', which starts one at once."
  $m = [regex]::Match($x2, ' grace seconds=(\d+)\z')
  Add-Check ($m.Success -and [int]$m.Groups[1].Value -ge ($grace - 2) -and [int]$m.Groups[1].Value -le $grace) "The wait after that pass is the release grace, ""grace seconds=$grace"" less rounding" "The line after it is '$x2'."
  $startedAgain = Wait-Until { @(Get-LinesAfter (Read-LogLines) $Index | Where-Object { $_ -cmatch ' started ' }).Count -ge 1 } ($grace + 60) $Process
  $startLine = @(Get-LinesAfter (Read-LogLines) $Index | Where-Object { $_ -cmatch ' started ' }) | Select-Object -First 1
  $gap = if ($startedAgain) { ((Get-LogTime $startLine) - $Released).TotalSeconds } else { -1 }
  Add-Check ($startedAgain -and $gap -ge ($grace - 2)) "The next holder starts at least the grace after the pass was let go, $grace seconds less rounding" $(if ($startedAgain) { "The next started line is '$startLine', $([Math]::Round($gap, 1)) seconds after the pass was let go." } else { 'No holder was started within the grace and a minute more.' })
}

function Write-Missed([int]$Attempt, [string]$What) {
  Write-Host "Attempt $Attempt of ${attemptsAllowed}: $What"
}

try {
  # 1. A holder that exits while the pass waits for board-holder.lock.
  Invoke-UnmetCase 'A holder that exits while the pass waits for board-holder.lock' {
    $concluded = $false
    $missed = ''
    for ($attempt = 1; $attempt -le $attemptsAllowed -and -not $concluded; $attempt++) {
      $p = Start-TrackingKeeper 1
      $held = $null
      try {
        $started = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'started').Count -ge 1 } 60 $p
        if (-not $started) { Add-Check $false 'The keeper starts a holder for the ready lease' "No started line appeared, because the keeper ended or 60 seconds passed. The log ends: $((Read-LogLines | Select-Object -Last 3) -join ' | ')"; $concluded = $true; continue }
        # The record is read only once the check holds the lock, which is after the pass has let it go:
        # a keeper MAY log "started" before it writes the record (keeper.md, Q5).
        $held = Lock-PassLock $p
        if (-not $held) { $missed = 'The check could not take board-holder.lock.'; continue }
        $rec = Read-Record
        # The pass that waits for the lock gives up after 10 seconds and says so. That line is the
        # event: the look before it found the holder alive, and the keeper is now in its 1-second
        # interval. The next look comes about a second after it, and the pass after that look waits
        # for the lock for 10 seconds again.
        $skipped = Wait-Until { @(Read-LogLines | Where-Object { $_ -cmatch ' skipped lease=- alias=- pid=- reason=locked\z' }).Count -ge 1 } 60 $p
        if (-not $skipped) { $missed = 'The keeper logged no skipped line while the check held the lock.'; continue }
        $index = Get-FirstIndex (Read-LogLines) ' skipped lease=- alias=- pid=- reason=locked\z'
        $watch = [Diagnostics.Stopwatch]::StartNew()
        [void](Wait-Until { $watch.Elapsed.TotalSeconds -ge 6 } 30 $p)
        # The lock goes right after the kill, before the wait for the exit, so that the pass is not
        # held longer than the kill needs.
        $box = @{ Released = $null }
        $stopped = $rec -and (Stop-RecordedHolder $rec { $held.Dispose(); $box.Released = [DateTime]::UtcNow })
        $held = $null
        $released = if ($box.Released) { $box.Released } else { [DateTime]::UtcNow }
        if (-not $stopped) { $missed = 'The check could not stop the holder the record names.'; continue }
        $after = Get-Outcome $p $index
        if (Test-WindowMissed $after) { $missed = "The pass was not the one that waited for the lock: the first line after the skipped one is '$(if ($after.Count) { $after[0] } else { 'nothing' })'."; continue }
        $concluded = $true
        Test-GraceFollows $p $index $after $released
      } finally {
        if ($held) { $held.Dispose() }
        if (-not $concluded -and $attempt -lt $attemptsAllowed) { Write-Missed $attempt $missed }
        Stop-Watch $p
        if (-not $concluded -and $attempt -lt $attemptsAllowed) { Reset-Attempt $p }
      }
    }
    if (-not $concluded) { Write-ConformanceResult -Name "keeper.md#release-grace (card #0068): $($script:caseName)" -Status SKIPPED -Reason "The keeper's look and pass did not bracket the stop in $attemptsAllowed attempts, so nothing was shown either way. The last attempt: $missed" }
  }

  # 2. The record is gone at the pass.
  Invoke-UnmetCase 'The record is gone when the pass looks for the tracked holder' {
    $concluded = $false
    $missed = ''
    for ($attempt = 1; $attempt -le $attemptsAllowed -and -not $concluded; $attempt++) {
      $p = Start-TrackingKeeper 5
      $held = $null
      try {
        $started = Wait-Until { @(Get-ActionTimes (Read-LogLines) 'started').Count -ge 1 } 60 $p
        if (-not $started) { Add-Check $false 'The keeper starts a holder for the ready lease' "No started line appeared, because the keeper ended or 60 seconds passed. The log ends: $((Read-LogLines | Select-Object -Last 3) -join ' | ')"; $concluded = $true; continue }
        $index = Get-FirstIndex (Read-LogLines) ' started '
        # The keeper's next look is 5 seconds after its pass, and reads the process only. The record
        # goes while the lock is held, so that no pass is running, and the holder keeps running. It is
        # read only once the lock is held, after the pass has let it go.
        $held = Lock-PassLock $p
        if (-not $held) { $missed = 'The check could not take board-holder.lock.'; continue }
        $rec = Read-Record
        $holderPid = if ($rec) { [int]$rec['pid'] } else { -1 }
        Remove-Item -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json')
        $held.Dispose(); $held = $null
        $released = [DateTime]::UtcNow
        $after = Get-Outcome $p $index
        if (Test-WindowMissed $after) { $missed = "A pass ran before the record was removed: the first line after the started one is '$(if ($after.Count) { $after[0] } else { 'nothing' })'."; continue }
        $concluded = $true
        Add-Check ($holderPid -gt 0 -and (Test-Alive $holderPid)) 'The tracked holder was still running when the pass did not find it trusted' 'The holder had ended.'
        Test-GraceFollows $p $index $after $released
      } finally {
        if ($held) { $held.Dispose() }
        if (-not $concluded -and $attempt -lt $attemptsAllowed) { Write-Missed $attempt $missed }
        Stop-Watch $p
        if (-not $concluded -and $attempt -lt $attemptsAllowed) { Reset-Attempt $p }
      }
    }
    if (-not $concluded) { Write-ConformanceResult -Name "keeper.md#release-grace (card #0068): $($script:caseName)" -Status SKIPPED -Reason "The record was not removed between two passes in $attemptsAllowed attempts, so nothing was shown either way. The last attempt: $missed" }
  }
} finally {
  Restore-CaseEnvironment
  Stop-RunProcesses
  $leftBehind = @(Get-RunProcesses).Count
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
}

if ($leftBehind -eq 0) { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status PASS }
else { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status FAIL -Reason "$leftBehind process(es) of this run were still running after the cleanup." }
Exit-ConformanceRun

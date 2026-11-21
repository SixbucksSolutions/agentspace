#Requires -Version 7
# A conformance check under D34: it drives the keeper only as a separate process, through what docs/keeper.md and docs/state.md document, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# THE WATCHING CASES OF A HOST PER PROJECT (v0.07, docs/keeper.md, "Watching", "The keeper's wait",
# "Fast failures and backoff", "Release grace", and "Conformance cases for a host per project", cases H9
# to H11, H15 and H17, written by E0.07-T04-S03, card #0157). Each host has its own count of fast
# failures, its own backoff and its own release grace, and one host's never delays another's. It
# checks behavior that v0.06's keeper, which knows one holder, does not have, so it is expected to FAIL
# against v0.06's keeper in most of its cases (H15's and H17's checks of the box alone may pass there, which
# is right); the rewrite's v0.07 keeper MUST pass every line. The one-pass cases are in
# test-unmet-hosts-pass.ps1.
#
# Each case runs against the program that AGENTSPACE_KEEPER_CMD names (or its default, through the
# harness) with the keeper's own arguments unchanged: <program> -Watch -SshExe <stub>
# -IntervalSeconds ... Each runs with a throwaway USERPROFILE and state folder, a stub -SshExe, and a
# hosts.json the case writes under the throwaway USERPROFILE, or none. <p> is agentspace; the box's
# project is swimcoach. The stub makes one host's holder exit at once and another's stay alive by the
# alias on its command line (conformance/keeper/keeper-hosts-common.ps1). No case needs a real host.
#
# H11's two cases use the method of the #0068 file (test-unmet-holder-dies-mid-pass.ps1, case 2): the
# case holds board-holder.lock between two passes, removes one host's record, and lets the lock go. A
# keeper's look reads the process only, so the next pass finds a tracked holder alive and its record
# gone. No timing is involved: no pass can run while the case holds the lock.
#
# What it touches: the same as test-keeper.ps1 (conformance/keeper/keeper-common.ps1, and this
# suite's conformance/keeper/keeper-hosts-common.ps1): a throwaway folder per case under $env:TEMP, the
# harness's child environment for it on every process, and only processes this run started, each
# stopped by a checked PID. The real %USERPROFILE%\.agentspace, ~/.ssh, ssh.exe, keeper task and box are
# never read, written, listed or hashed. Every write and delete goes through Assert-ConformancePath at the call.
#
# Every wait is on what a case checks for, with a deadline, never a fixed sleep (keeper.md, "Checking the
# keeper", card #0066). The intervals, graces and backoffs below are as small as keeper.md's command
# line allows while a count of lines in a window still tells one host's wait from another's.
#
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line; exit 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.
# Run:  pwsh -NoProfile -File conformance\keeper\test-unmet-hosts-watch.ps1
param([string]$Case = '*')
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) {
  Write-Output 'SKIPPED keeper.md#where-it-runs: The keeper specified is a Windows keeper; this host is not Windows.'
  Write-Output '0 passed, 0 failed, 1 skipped'
  exit 2
}

. (Join-Path $PSScriptRoot 'keeper-common.ps1')
. (Join-Path $PSScriptRoot 'keeper-hosts-common.ps1')

# Resolved before the first case, which strips AGENTSPACE_* from the environment.
$keeperCmd = Resolve-KeeperProgram
if (-not $keeperCmd -or -not (Test-Path -LiteralPath $keeperCmd -PathType Leaf)) {
  Write-ConformanceResult -Name 'keeper.md#command-line: The program that AGENTSPACE_KEEPER_CMD names exists' -Status FAIL -Reason $(if ($keeperCmd) { "$keeperCmd is not a file." } else { 'Neither the variable nor its default names a program.' })
  Exit-ConformanceRun
}

Initialize-KeeperRun
Initialize-HostsRun

function Test-Selected([string]$Id) { $Id -like $Case }

# The lines of a log after the first $Count of them.
function Get-LinesAfterCount($Log, [int]$Count) {
  if ($Count -lt $Log.Count) { @($Log[$Count..($Log.Count - 1)]) } else { @() }
}

# One H11 case: a keeper tracks a holder on each of two hosts; the case removes one host's record while it holds
# board-holder.lock; the next pass logs lost for that host and keeps the other's holder in the same pass.
function Invoke-LostCase([string]$Id, [string]$LostProject, [string]$LostTag, [string]$KeptProject, [string]$KeptTag) {
  Invoke-HostCase $Id 'keeper.md#release-grace' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @((New-HostLease $boxProject 'b1' $farFuture), (New-HostLease $pProject 'c1' $farFuture))
    $graceSeconds = 15
    $proc = Start-Watch @('-IntervalSeconds', '5', '-MaxPasses', '200', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '1', '-FastFailureSeconds', '600', '-ReleaseGraceSeconds', "$graceSeconds") $stub
    $held = $null
    try {
      $both = Wait-Until { @(Select-ActionLines (Read-LogLines) 'started').Count -ge 2 } 60 $proc
      if (-not $both) { Add-HostCheck $false 'The keeper starts a holder on each host' "Started lines seen: $(@(Select-ActionLines (Read-LogLines) 'started').Count)."; return }
      $held = Lock-PassLock $proc
      if (-not $held) { Add-HostCheck $false 'The check takes board-holder.lock between two passes' 'It could not be taken in 30 seconds.'; return }
      # No pass runs while the lock is held, so the records and the log are read at a point between two passes.
      $recLost = Read-HostRecord $LostProject
      $recKept = Read-HostRecord $KeptProject
      $keptRecordPath = Get-HostRecordPath $KeptProject
      $keptRecordBefore = [IO.File]::ReadAllText($keptRecordPath)
      $mark = (Read-LogLines).Count
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $runHome -Path (Get-HostRecordPath $LostProject)) -Force
      $held.Dispose(); $held = $null
      $three = Wait-Until { (Get-LinesAfterCount (Read-LogLines) $mark).Count -ge 3 } 60 $proc
      $after = Get-LinesAfterCount (Read-LogLines) $mark
      Add-HostCheck $three 'The next pass logs the host''s turn, the other host''s turn and the grace line' "Lines after the removal: $($after -join ' | ')"
      if (-not $three) { return }
      $lostPid = [string]$recLost['pid']
      $keptPid = [string]$recKept['pid']
      $lostHostField = if ($LostProject -ceq $boxProject) { $boxProject } else { $LostProject }
      # The box's turn comes first, then agentspace's. The grace line is the keeper's, and carries host= for a host that is not the box.
      $lostFirst = $LostProject -ceq $boxProject
      $iLost = if ($lostFirst) { 0 } else { 1 }
      $iKept = if ($lostFirst) { 1 } else { 0 }
      Add-HostCheck (Test-HostLine $after[$iLost] 'lost' (Get-HostLeaseId $LostTag) (Get-HostAlias $LostProject $LostTag) $lostPid $LostProject) "The pass logs lost for the tracked holder on $LostProject, with its lease, alias and PID, ending in host=$LostProject" "Line: $($after[$iLost])"
      Add-HostCheck (Test-HostLine $after[$iKept] 'kept' (Get-HostLeaseId $KeptTag) (Get-HostAlias $KeptProject $KeptTag) $keptPid $KeptProject) "The same pass keeps the holder on $KeptProject, ending in host=$KeptProject" "Line: $($after[$iKept])"
      $graceRx = $logTime + 'grace seconds=(\d+)' + $(if ($LostProject -ceq $boxProject) { '\z' } else { ' host=' + [regex]::Escape($LostProject) + '\z' })
      $g = if ($after[2] -cmatch $graceRx) { [int]$Matches[1] } else { -1 }
      Add-HostCheck ($g -ge ($graceSeconds - 2) -and $g -le $graceSeconds) "The grace line says grace seconds=$graceSeconds less rounding$(if ($LostProject -cne $boxProject) { ', with host=' + $LostProject } else { ', with no host field, as for the box' })" "Line: $($after[2])"
      Add-HostCheck (@($after[0..2] | Where-Object { $_ -cmatch ' started ' }).Count -eq 0) 'That pass starts no holder' "Lines: $($after[0..2] -join ' | ')"
      Add-HostCheck ((Test-Alive ([int]$recLost['pid']))) 'The process of the host whose record went is left running' 'It was stopped.'
      Add-HostCheck ((Test-Alive ([int]$recKept['pid'])) -and [IO.File]::ReadAllText($keptRecordPath) -ceq $keptRecordBefore) 'The other host''s holder is running and its record is unchanged' 'It was stopped or changed.'
    } finally {
      if ($held) { $held.Dispose() }
      Stop-Watch $proc
    }
  }
}

try {
  # H9. <p>'s stub exits at once, the box's stays alive: <p> backs off alone.
  if (Test-Selected 'H9') { Invoke-HostCase 'H9' 'keeper.md#fast-failures-and-backoff' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @((New-HostLease $boxProject 'b1' $farFuture), (New-HostLease $pProject 'c1' $farFuture))
    Set-FailMarker (Get-HostAlias $pProject 'c1')
    $proc = Start-Watch @('-IntervalSeconds', '1', '-BackoffBaseSeconds', '2', '-BackoffMaxSeconds', '8') $stub
    try {
      $got = Wait-Until { @(Get-HostBackoffs (Read-LogLines) $pProject).Count -ge 3 } 120 $proc
    } finally { Stop-Watch $proc }
    $log = Read-LogLines
    $b = @(Get-HostBackoffs $log $pProject)
    Add-HostCheck $got 'The keeper logs three backoff lines for agentspace' "Backoff lines seen for agentspace: $($b.Count). The log ends: $(($log | Select-Object -Last 4) -join ' | ')"
    Add-HostCheck ($b.Count -ge 3 -and (($b[0..2]) -join ',') -ceq '2,4,8') "agentspace's backoff lines say seconds 2, 4, 8 (got $($b -join ','))"
    Add-HostCheck (@($log | Where-Object { $_ -cmatch ' (backoff|grace) seconds=' }).Count -eq @($log | Where-Object { $_ -cmatch ' (backoff|grace) seconds=\d+ host=agentspace\z' }).Count) 'Every backoff and grace line is agentspace''s: none is the box''s' "Lines: $(@($log | Where-Object { $_ -cmatch ' (backoff|grace) seconds=' }) -join ' | ')"
    # The stand-in exits at once, which a start meets in one of two ways that keeper.md allows: the keeper's one look finds it gone (a failed line),
    # or it dies a moment after that look and is found gone by the next one (a started line, then the backoff). Either is a fast failure on agentspace alone.
    $failed = @($log | Where-Object { Test-HostLine $_ 'failed' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') '-' $pProject })
    Add-HostCheck ($failed.Count -ge 1 -and @($failed | Where-Object { $_ -cmatch ' reason=The holder could not be started: ' }).Count -eq $failed.Count) 'agentspace''s failed starts are logged as failed ... pid=- host=agentspace, with a reason that the holder could not be started' "Failed lines: $($failed.Count). The log: $($log -join ' | ')"
    Add-HostCheck (@(Get-HolderProcesses | Where-Object { ([string]$_.CommandLine).Contains((Get-HostAlias $pProject 'c1')) }).Count -eq 0) 'No holder is left running on agentspace'
    $boxStarted = @($log | Where-Object { $_ -cmatch ' started ' -and $_ -cmatch ' host=swimcoach' })
    $recB = Read-HostRecord $boxProject
    Add-HostCheck ($boxStarted.Count -eq 1 -and $null -ne $recB -and (Test-HostLine $boxStarted[0] 'started' (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') ([string]$recB['pid']) $boxProject)) 'The box''s holder is started once' "Box started lines: $($boxStarted.Count)."
    Add-HostCheck ($null -ne $recB -and (Test-Alive ([int]$recB['pid']))) 'The box''s holder is still running' 'It is not.'
    # Between the keeper's second and third backoff line for agentspace its hold-off is 4 seconds, and the keeper's interval is 1:
    # the box's turn goes on in every pass.
    $bIdx = @(0..([Math]::Max($log.Count, 1) - 1) | Where-Object { $_ -lt $log.Count -and $log[$_] -cmatch ' backoff seconds=\d+ host=agentspace\z' })
    $between = if ($bIdx.Count -ge 3 -and $null -ne $recB) { @($log[($bIdx[1] + 1)..($bIdx[2] - 1)] | Where-Object { Test-HostLine $_ 'kept' (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') ([string]$recB['pid']) $boxProject }) } else { @() }
    Add-HostCheck ($between.Count -ge 2) 'The box''s kept lines go on every interval while agentspace waits out its backoff: at least 2 between its second and third backoff line' "Kept lines between them: $($between.Count)."
  } }

  # H10. <p>'s holder is stopped while the box's lease stays ready: <p> waits out a grace alone.
  if (Test-Selected 'H10') { Invoke-HostCase 'H10' 'keeper.md#the-keepers-wait' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @((New-HostLease $boxProject 'b1' $farFuture), (New-HostLease $pProject 'c1' $farFuture))
    $graceSeconds = 8
    $proc = Start-Watch @('-IntervalSeconds', '1', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '1', '-FastFailureSeconds', '600', '-ReleaseGraceSeconds', "$graceSeconds") $stub
    try {
      $both = Wait-Until { @(Select-ActionLines (Read-LogLines) 'started').Count -ge 2 } 60 $proc
      if (-not $both) { Add-HostCheck $false 'The keeper starts a holder on each host' "Started lines seen: $(@(Select-ActionLines (Read-LogLines) 'started').Count)."; return }
      $recP = Read-HostRecord $pProject
      # agentspace's lease is released, and another of its leases becomes ready: the holder is stopped, and a start is due once the grace has run.
      Write-Leases @((New-HostLease $boxProject 'b1' $farFuture), (New-HostLease $pProject 'c1' $farFuture -Released), (New-HostLease $pProject 'c2' $farFuture))
      $restarted = Wait-Until { @((Read-LogLines) | Where-Object { $_ -cmatch ' started ' -and $_ -cmatch ' host=agentspace\z' }).Count -ge 2 } 90 $proc
    } finally { Stop-Watch $proc }
    $log = Read-LogLines
    $iStop = Get-FirstLineIndex $log ' stopped '
    Add-HostCheck ($iStop -ge 0 -and $null -ne $recP -and (Test-HostLine $log[$iStop] 'stopped' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recP['pid']) $pProject)) 'The pass that finds the lease released logs stopped for agentspace, ending in host=agentspace' "Log: $(($log | Select-Object -Last 6) -join ' | ')"
    $g = @(Get-HostGraces $log $pProject)
    Add-HostCheck ($g.Count -ge 1 -and $g[0] -ge ($graceSeconds - 2) -and $g[0] -le $graceSeconds) "The keeper logs grace seconds=$graceSeconds less rounding, with host=agentspace" "Grace lines for agentspace: $($g -join ',')"
    Add-HostCheck (@(Get-HostGraces $log '').Count -eq 0 -and @(Get-HostBackoffs $log '').Count -eq 0) 'The box has no grace or backoff line' 'A box line was logged.'
    Add-HostCheck $restarted 'agentspace gets a new holder through its other ready lease' "Started lines for agentspace: $(@($log | Where-Object { $_ -cmatch ' started ' -and $_ -cmatch ' host=agentspace\z' }).Count)."
    if ($restarted) {
      $iGrace = Get-FirstLineIndex $log ' grace seconds=\d+ host=agentspace\z'
      $startIdx = @(0..($log.Count - 1) | Where-Object { $log[$_] -cmatch ' started ' -and $log[$_] -cmatch ' host=agentspace\z' })[1]
      $gap = ((Get-LogTime $log[$startIdx]) - (Get-LogTime $log[$iGrace])).TotalSeconds
      Add-HostCheck ($gap -ge ($graceSeconds - 2)) "No holder starts on agentspace until its grace has run: the start is at least $graceSeconds seconds, less rounding, after the grace line" "The start came $gap seconds after the grace line."
      Add-HostCheck (Test-HostLine $log[$startIdx] 'started' (Get-HostLeaseId 'c2') (Get-HostAlias $pProject 'c2') ([string](Read-HostRecord $pProject)['pid']) $pProject) 'The new holder goes through the other ready lease' "Line: $($log[$startIdx])"
      $keptBetween = @($log[($iStop + 1)..($startIdx - 1)] | Where-Object { $_ -cmatch ' kept ' -and $_ -cmatch ' host=swimcoach\z' })
      Add-HostCheck ($keptBetween.Count -ge 4) 'The box''s kept lines go on, every interval, while agentspace waits: at least 4 before agentspace''s start' "Box kept lines between the stop and the start: $($keptBetween.Count)."
    }
    Add-HostCheck (@($log | Where-Object { $_ -cmatch ' started ' -and $_ -cmatch ' host=swimcoach' }).Count -eq 1) 'The box''s holder is started once and never again' 'It was started again.'
  } }

  # H11. One host's record is removed between a keeper's look and its pass: lost for that host, the other's holder kept in the same pass.
  if (Test-Selected 'H11a') { Invoke-LostCase 'H11a' $pProject 'c1' $boxProject 'b1' }
  if (Test-Selected 'H11b') { Invoke-LostCase 'H11b' $boxProject 'b1' $pProject 'c1' }

  # H15. Upgrade: v0.06's holder, recorded in v0.06's form with no registry, is adopted by a keeper's first pass.
  if (Test-Selected 'H15') { Invoke-HostCase 'H15' 'keeper.md#stopping-and-starting-the-keeper' {
    Write-Leases @((New-HostLease $boxProject 'b1' $farFuture))
    $h = Start-HostHolder $boxProject 'b1'
    $recordPath = Get-HostRecordPath $boxProject
    $before = [IO.File]::ReadAllText($recordPath)
    $graceSeconds = 6
    $smp = [AgentspaceTestHosts.LaunchSampler]::new($env:AGENTSPACE_STATE_DIR)
    $smp.Start()
    $proc = $null
    try {
      $proc = Start-Watch @('-IntervalSeconds', '1', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '1', '-FastFailureSeconds', '600', '-ReleaseGraceSeconds', "$graceSeconds") $stub
      $kept = Wait-Until { @(Select-ActionLines (Read-LogLines) 'kept').Count -ge 1 } 60 $proc
      $firstLog = Read-LogLines
      $afterFirst = [IO.File]::ReadAllText($recordPath)
      # The keeper adopted the holder, so its end is a tracked holder's end, which the keeper's next look meets with the release grace.
      Stop-OwnTree $h.Pid
      $graced = Wait-Until { @(Get-HostGraces (Read-LogLines) '').Count -ge 1 } 60 $proc
    } finally { if ($proc) { Stop-Watch $proc }; $smp.Stop() }
    $log = Read-LogLines
    Add-HostCheck $kept 'The keeper''s first pass keeps the holder' "Log: $($log -join ' | ')"
    Add-HostCheck ($firstLog.Count -ge 1 -and (Test-HostLine $firstLog[0] 'kept' (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') ([string]$h.Pid) $boxProject)) 'Its first line is kept ... host=swimcoach, naming the holder v0.06 started' "Line: $(if ($firstLog.Count) { $firstLog[0] } else { 'none' })"
    Add-HostCheck (@($log | Where-Object { $_ -cmatch ' started ' }).Count -eq 0) 'No holder is started' "Log: $($log -join ' | ')"
    Add-HostCheck (@($smp.Tokens).Count -eq 0) 'No launch file appears: there is no new start' "Starts seen: $(@($smp.Tokens).Count)."
    Add-HostCheck ($afterFirst -ceq $before) 'The record is unchanged' 'The keeper rewrote board-holder.json.'
    Add-HostCheck ((Get-OtherRecordNames).Count -eq 0 -and @(Get-StateNames | Where-Object { $_ -cmatch '\Aboard-holder-ssh\.[a-z0-9-]+\.log' }).Count -eq 0) 'No other host''s record or log appears' "Found: $((Get-OtherRecordNames) -join ', ')"
    $g = @(Get-HostGraces $log '')
    Add-HostCheck ($graced -and $g[0] -ge ($graceSeconds - 2) -and $g[0] -le $graceSeconds) "The keeper tracks the adopted holder: when it ends, the keeper's next line is the release grace, grace seconds=$graceSeconds less rounding, with no host field" "Grace lines: $($g -join ','). The log ends: $(($log | Select-Object -Last 3) -join ' | ')"
    Add-HostCheck (@($log | Where-Object { $_ -cmatch ' started ' }).Count -eq 0) 'No holder is started before the grace has run' "Log: $($log -join ' | ')"
  } }

  # H17. With the box and <p> both ready and nothing to do, a pass logs the box's idle line alone.
  if (Test-Selected 'H17') { Invoke-HostCase 'H17' 'keeper.md#idle' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @()
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The one-pass run exits 0' "The exit code was $code."
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 1 -and (Test-IdleLine $log[0])) 'The one-pass run logs exactly one line, the box''s idle lease=- alias=- pid=-, with no host field' "Log: $($log -join ' | ')"
    $proc = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '3') $stub
    $wcode = Wait-Watch $proc 120
    Add-HostCheck ($wcode -eq 0) 'The keeper ends after 3 passes with exit 0' "The exit code was $wcode."
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 4 -and @($log | Where-Object { Test-IdleLine $_ }).Count -eq 4) 'Each of the keeper''s 3 passes logs exactly the box''s idle line: no host field, no grace or backoff line, and no line for agentspace' "Log: $($log -join ' | ')"
    Add-HostCheck ((Get-OtherRecordNames).Count -eq 0 -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json')) -and @(Get-StateNames | Where-Object { $_ -like 'board-holder-ssh*' }).Count -eq 0) 'No record and no holder log is written for any host' "State folder: $((Get-StateNames) -join ', ')"
  } }
} finally {
  Restore-CaseEnvironment
  Stop-RunProcesses
  $leftBehind = @(Get-RunProcesses).Count
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath (Assert-ConformanceHomeRoot -Root $temp) -Recurse -Force -ErrorAction SilentlyContinue }
}

if ($leftBehind -eq 0) { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status PASS }
else { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status FAIL -Reason "$leftBehind process(es) of this run were still running after the cleanup." }
Exit-ConformanceRun

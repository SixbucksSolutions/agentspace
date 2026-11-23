#Requires -Version 7
# A conformance check under D34: it drives the keeper only as a separate process, through what docs/keeper.md and docs/state.md document, with a stand-in for the guard and for ssh (programs outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# THE KEEPER'S START OF THE GUARD (v0.08, docs/keeper.md, "Starting the guard", "When the keeper starts
# it", "How the keeper starts it", "The keeper's lines for it", "What a failed start changes",
# "guard.lock: tried, never held", "The guard's environment", "Usage errors", "board-holder-keeper.lock:
# one keeper at a time", and "Conformance cases for the guard's start", cases G1 to G16, written by
# E0.08-T04-S03, card #0184). A keeper starts the operator's guard at its first iteration and
# then once every -GuardIntervalSeconds (300 by default), never while guard.lock is held, and logs one line
# for each attempt. It checks behavior that v0.07's keeper, which starts no guard, does not have, so it is
# expected to FAIL against v0.07's keeper; the v0.08 keeper MUST pass every line.
#
# Each case runs against the program that AGENTSPACE_KEEPER_CMD names (or its default, through the harness)
# with the keeper's own arguments unchanged: <program> -Watch -SshExe <stub> -IntervalSeconds ...
# -GuardIntervalSeconds ... Each runs with a throwaway USERPROFILE and state folder and no host registry, so
# the box alone is the keeper's host. The case names its own AGENTSPACE_GUARD_CMD: a .cmd stand-in in the
# case's home that appends one line to guard-runs.log beside itself for each run, with its first two
# arguments (%~1 and %2, so that a check asserts the arguments and never the command line's quoting), its
# working folder and the time, and then exits (or, where a case reads the started process, stays alive until
# the case ends it). It never runs a real guard. keeper-common.ps1 also names tripwires for the guard and for
# the operator's test seam (AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD, AGENTSPACE_SCP_CMD); each case asserts
# that aws, ssh and scp were started 0 times, and the guard's own default tripwire, which the case's
# stand-in replaces, 0 times, and the run prints the totals at its end.
#
# Cases:
#   G1  A keeper's first iteration starts the guard: one "guard started pid=<pid>" line before the first
#       pass's line, with no host field; the stand-in ran once, with exactly the argument "guard", in the
#       home as its working folder; the line's PID is the started process's.
#   G2  With -GuardIntervalSeconds not given, 5 passes at -IntervalSeconds 1 make exactly one start.
#   G3  With a short -GuardIntervalSeconds, the start is not made again within it and is made again after
#       it: two or more start lines, each at least that long after the one before, one stand-in run each.
#   G4  A start that falls due in a wait is made there, and the wait ends when it would have.
#   G5  While the case holds <home>\.agentspace\guard.lock, no start and no line; once it is closed, a start
#       without waiting -GuardIntervalSeconds.
#   G8  A start that cannot be found (AGENTSPACE_GUARD_CMD names no file) is logged with keeper.md's exact
#       line, about every -GuardIntervalSeconds, and every holder is as in a run without the failure: a
#       holder that runs is kept in every pass; one that is due is started; no hold-off, no record changed.
#   G6  A folder in place of guard.lock: the exact failed line, no run of the stand-in, the next such line no
#       sooner than -GuardIntervalSeconds, and every pass's lines as without it.
#   G7  No <home>\.agentspace folder: a start line, and the keeper creates neither the folder nor guard.lock.
#   G9  A relative AGENTSPACE_GUARD_CMD is made full against the keeper's working folder.
#   G10 The guard's environment, compared by variable name only: never a value is printed.
#   G11 The stand-in stays alive: the keeper exits 0 while it runs, outside the keeper's job; a new keeper takes
#       board-holder-keeper.lock meanwhile; the case ends the stand-in by the PID of the start line, checked first.
#   G12 A one-pass run starts no guard and logs no guard line.
#   G13 The start is not inside a pass: board-holder.lock held by the case, and a start line all the same.
#   G14 A .cmd stand-in whose path holds a space receives exactly the one argument "guard".
#   G15 The two usage errors for -GuardIntervalSeconds, byte for byte, each exit 2, nothing logged.
#   G16 A second keeper, while the case holds board-holder-keeper.lock as a keeper does: its one "another keeper
#       is running" line, exit 0, no guard.
#
# Timing: every wait is on what a case checks for, with a deadline, never a fixed sleep (keeper.md,
# "Checking the keeper", card #0066). A timed expectation is a lower bound that keeper.md pins (a start no
# sooner than -GuardIntervalSeconds after the last attempt; a wait no shorter than its seconds), an order of
# lines, or a generous upper bound that a loaded laptop does not break (keeper.md, "The keeper's wait":
# the wait ends when it would have, and an attempt in it never moves its end). Log times are whole seconds,
# so a difference of two of them is at least the true gap less one second at most; a gap that keeper.md
# pins as at least n seconds is therefore at least n whole seconds in the log.
#
# Gaps: the parts of keeper.md's rows that no line of this file asserts, each with its reason. A row part left out is a gap named
# here, never a silent one.
#   - G11, "outside the Job Object of the keeper and of every process above it: the task's launcher, the task and a check's shell":
#     only that the stand-in outlives the keeper's exit is asserted. Whether it is outside the Job Objects of the processes above
#     the keeper cannot be seen from outside without inspecting another process's job membership, which a check MUST NOT do to a
#     process it did not start (the check's own shell included).
#   - G11, "inherits no handle of the keeper's": only board-holder-keeper.lock is shown (the new keeper takes it). That the guard
#     holds no handle on board-holder.lock, on guard.lock or on a standard handle is not asserted: a handle held by another process
#     is not visible without inspecting it. The new keeper's own start line is also not asserted; the case needs it only to end the
#     second stand-in by its PID.
#   - G11, "hidden console window", "a new process group", "never reads its standard input": no stand-in can show them from outside.
#   - "The call is bounded at 10 seconds" and the three <why> rows that keeper.md names as the keeper crate's own tests (no
#     operator beside the program, no SystemRoot, a detached start that fails): no case can bring them about from outside.
#   - G4, the start "about 12 seconds after the first": a lower bound of 12 and an upper bound of 16 are asserted, with 4 seconds
#     of slack for load, and the start is shown to fall in the wait by its coming at least 4 seconds before the second idle line
#     (nominally 8). G3 asserts no upper bound on a gap, since keeper.md H9's lesson is that a pass can stretch under load.
#   - G5, "no later than just before the first pass that starts after the close": asserted as an order of lines (at most one more
#     pass line than the count at the close comes before the start line), never as a time, so load cannot break it.
#   - G13, "the start does not wait on board-holder.lock": asserted by the start line's time being no later than the skipped line's
#     (the skipped line carries the pass time, taken before the lock retry), which a wait of less than one log second cannot show.
#   - G10, a logon-environment variable that the keeper does not have set: its removal, and the pass-through of a logon
#     AGENTSPACE_..._CMD that the keeper does not know, and AGENTSPACE_TIMEZONE and AGENTSPACE_DIAGNOSTIC not reaching the guard,
#     are not asserted: G10's row does not name them, and the harness strips AGENTSPACE_* from the logon environment. The stand-in
#     writes only the variables the case names, and for AGENTSPACE_CASE_OWN its presence alone, never a value.
#   - Pin P19's rule, that a start which finds guard.lock held in a wait is next tried just before the next pass and never again in
#     the same wait, is not asserted: G5 holds the lock for the whole of its first stage and shows only that the start comes after
#     the close.
#
# What it touches: the same as test-keeper.ps1 (conformance/keeper/keeper-common.ps1): a throwaway folder
# per case under $env:TEMP, the harness's child environment for it on every process, and only processes
# this run started, each stopped by a checked PID. The real %USERPROFILE%\.agentspace, ~/.ssh, ssh.exe, the
# keeper task and the box are never read, written, listed or hashed, and no real guard or AWS CLI is run.
# Every write and delete goes through Assert-ConformancePath at the call. The stand-in writes beside itself.
# The check reports a mismatch by name or count, never by the value of a variable it captured.
#
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line; exit 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.
# Run:  pwsh -NoProfile -File conformance\keeper\test-unmet-keeper-guard.ps1 [-Case <wildcard>]
param([string]$Case = '*')
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
  Write-ConformanceResult -Name 'keeper.md#command-line: The program that AGENTSPACE_KEEPER_CMD names exists' -Status FAIL -Reason $(if ($keeperCmd) { 'The variable names a path that is not a file.' } else { 'Neither the variable nor its default names a program.' })
  Exit-ConformanceRun
}

Initialize-KeeperRun

$farFuture = '2099-01-01T00:00:00Z'
$script:caseId = ''
$script:caseSpec = 'keeper.md'
$script:caseHome = $null
$logTimeRx = '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ '

# -Case is a wildcard on a case's ID, and it also selects the lettered parts of a case: -Case G8 selects G8a and G8b, and
# -Case G1 selects G1 alone, never G10 to G16.
function Test-Selected([string]$Id) { $Id -like $Case -or $Id -like "$Case[a-z]" }

# The code page that the stand-in's cmd.exe writes its files in: the console's OEM code page, not UTF-8.
function Get-StandInEncoding { [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage) }

# A message without the real profile's path or this run's folder: an error's text is masked like output.
function Format-Masked([string]$Text) {
  $t = $Text.Replace($script:temp, '<run>')
  $t = $t -replace '(?i)[A-Z]:\\Users\\[^\\\s]+', '<profile>'
  $t
}

# One result, named for the spec section and the case it enforces: keeper.md#<section> (G1): <text>.
function Add-GuardCheck([bool]$Ok, [string]$Text, [string]$Why = '') {
  $name = "$($script:caseSpec) ($($script:caseId)): $Text"
  if ($Ok) { Write-ConformanceResult -Name $name -Status PASS }
  else {
    if (-not $Why) {
      $Why = 'The check did not hold.'
      try { $l = Read-LogLines -IncludeGuard; if ($l.Count -gt 0) { $Why = 'The check did not hold. board-holder.log ends: ' + (($l | Select-Object -Last 4) -join ' | ') } } catch { }
    }
    Write-ConformanceResult -Name $name -Status FAIL -Reason (Format-Masked $Why)
  }
}

# One case: a fresh state folder and home folder with the harness's child environment for them, an empty
# lease file, the body, and a cleanup of every process the case started, in a finally.
function Invoke-GuardCase([string]$Id, [string]$Spec, [scriptblock]$Body) {
  $script:caseId = $Id
  $script:caseSpec = $Spec
  $script:caseHome = New-CaseHome
  try { & $Body } catch { Add-GuardCheck $false 'No unexpected error' "An error stopped the case: $($_.Exception.Message)" } finally { Stop-RunProcesses }
}

# The stand-in guard (keeper.md, "Conformance cases for the guard's start"): a .cmd in the case's home that
# appends one line to guard-runs.log beside itself for each run, with its arguments, its working folder and
# the time, and exits 0; with -Stay, it then waits about 20 minutes, until the case ends it. It is named by
# the case's own AGENTSPACE_GUARD_CMD, set after New-CaseHome, which replaces the tripwire keeper-common.ps1
# named. -Missing returns the path of a file that is not written, for a case that names no file.
# -Folder is the stand-in's folder under the home (default guard-standin). -GuardCmd is the value that
# AGENTSPACE_GUARD_CMD is given when it is not the stand-in's full path (a relative path). -DumpEnv makes the
# stand-in also write the variables of $guardEnvNames from its own environment, each explicitly, and for
# AGENTSPACE_CASE_OWN its presence alone, to guard-env.log beside itself, and then a line ENV-END; a value is read by the case,
# never printed.
$guardEnvNames = @('USERPROFILE', 'AGENTSPACE_STATE_DIR', 'AGENTSPACE_SSH_DIR', 'AGENTSPACE_PROXY_COMMAND', 'AGENTSPACE_AWS_CMD', 'AGENTSPACE_SSH_CMD', 'AGENTSPACE_SCP_CMD', 'AGENTSPACE_GUARD_CMD')
function New-GuardStandIn([switch]$Stay, [switch]$Missing, [switch]$DumpEnv, [string]$Folder = 'guard-standin', [string]$GuardCmd = '') {
  $dir = Join-Path $script:caseHome.UserProfile $Folder
  $path = Join-Path $dir 'guard-standin.cmd'
  $runs = Join-Path $dir 'guard-runs.log'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $runHome -Path $dir) | Out-Null
  if ($Missing) { $path = Join-Path $dir 'no-such-guard.cmd' }
  else {
    $lines = @('@echo off', 'REM A stand-in for the guard (a program outside agentspace) used by a conformance check. See docs/conformance.md.',
      '>>"%~dp0guard-runs.log" echo Run a1=[%~1] a2=[%2] dir=[%CD%] time=%TIME%')
    # Only the variables the case names are written, each explicitly: a logon-environment AGENTSPACE_..._CMD that the keeper does not
    # know is passed through to the guard (keeper.md, "The guard's environment"), and its value MUST NOT reach a file. Delayed
    # expansion, so that a value holding a character that cmd.exe reads (a quote, an ampersand) is written as it is.
    if ($DumpEnv) {
      $lines += 'setlocal enabledelayedexpansion'
      foreach ($n in $guardEnvNames) { $lines += ('if defined {0} >>"%~dp0guard-env.log" echo {0}=!{0}!' -f $n) }
      $lines += 'if defined AGENTSPACE_CASE_OWN >>"%~dp0guard-env.log" echo AGENTSPACE_CASE_OWN=present'
      $lines += '>>"%~dp0guard-env.log" echo ENV-END'
    }
    if ($Stay) { $lines += 'ping -n 1200 127.0.0.1 >nul' }
    $lines += 'exit /b 0'
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path $path), (($lines -join "`r`n") + "`r`n"))
  }
  $full = Assert-ConformancePath -ConformanceHome $runHome -Path $path
  $value = if ($GuardCmd) { $GuardCmd } else { $full }
  Set-CaseEnvironment @{ AGENTSPACE_GUARD_CMD = $value }
  [pscustomobject]@{ Path = $full; Runs = (Join-Path $dir 'guard-runs.log'); EnvLog = (Join-Path $dir 'guard-env.log'); Value = $value }
}

# The stand-in's runs, in order: A1 (its first argument, %~1), A2 (its second, %2) and Dir of each line of guard-runs.log, read shared.
function Read-GuardRuns($Stand) {
  if (-not (Test-Path -LiteralPath $Stand.Runs -PathType Leaf)) { return }
  try {
    $fs = [IO.File]::Open($Stand.Runs, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try { $text = [IO.StreamReader]::new($fs, (Get-StandInEncoding)).ReadToEnd() } finally { $fs.Dispose() }
  } catch { return }
  $out = foreach ($l in ($text -split "`r?`n" | Where-Object { $_ -ne '' })) {
    if ($l -cmatch '\ARun a1=\[(.*)\] a2=\[(.*)\] dir=\[(.*)\] time=') { [pscustomobject]@{ A1 = $Matches[1]; A2 = $Matches[2]; Dir = $Matches[3] } }
    else { [pscustomobject]@{ A1 = '?'; A2 = '?'; Dir = '?' } }
  }
  $out
}

# The runs that received exactly the one argument guard: the first is guard, and there is no second. The check is on the
# arguments the stand-in received, never on the command line's quoting (keeper.md, "How the keeper starts it").
function Get-OneArgRuns($Runs) { @($Runs | Where-Object { $_.A1 -ceq 'guard' -and $_.A2 -ceq '' }) }

function Get-StartLines($Log) { @($Log | Where-Object { $_ -cmatch ($logTimeRx + 'guard started pid=(\d+|-)\z') }) }
function Get-GuardFailedLines($Log) { @($Log | Where-Object { $_ -cmatch ($logTimeRx + 'failed lease=- alias=- pid=- reason=The guard could not be started: ') }) }
function Get-PassLines($Log) { Remove-GuardLines $Log }

# True when the dir a stand-in saw is the case's home. Both sides are put in the long form that
# Assert-ConformancePath returns, since the run's folder may be reached through an 8.3 name; a dir
# that is outside the run's folder is a mismatch.
function Test-SameHome([string]$Dir) {
  try {
    $a = Assert-ConformancePath -ConformanceHome $runHome -Path $Dir
    $b = Assert-ConformancePath -ConformanceHome $runHome -Path $script:caseHome.UserProfile
    [string]::Equals($a.TrimEnd('\'), $b.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
  } catch { $false }
}

# The starts that a tripwire recorded in the case's home. The case asserts aws, ssh and scp, and the guard's
# default tripwire, which the case's own stand-in replaced.
function Test-TripwiresQuiet {
  (Get-KeeperTripwireStarts 'aws') -eq 0 -and (Get-KeeperTripwireStarts 'ssh') -eq 0 -and (Get-KeeperTripwireStarts 'scp') -eq 0 -and (Get-KeeperTripwireStarts 'guard') -eq 0
}
function Get-TripwireReason {
  "Starts: aws $(Get-KeeperTripwireStarts 'aws'), ssh $(Get-KeeperTripwireStarts 'ssh'), scp $(Get-KeeperTripwireStarts 'scp'), guard tripwire $(Get-KeeperTripwireStarts 'guard')."
}

# Waits until the stand-in has run at least $Count times, or the deadline passes.
function Wait-GuardRuns($Stand, [int]$Count, [int]$Seconds = 30) {
  Wait-Until { @(Read-GuardRuns $Stand).Count -ge $Count } $Seconds
}

# The difference in whole seconds between two log lines' times.
function Get-LineGap([string]$Earlier, [string]$Later) { [int](((Get-LogTime $Later) - (Get-LogTime $Earlier)).TotalSeconds) }

# Starts the keeper with -Watch as Start-Watch does, in the working folder $WorkDir, with the variables of
# $KeeperEnv set for it alone: each is set, the process is started, and each is put back at once, so that
# this check's own reads of the state folder still use the full paths. Output goes to files in the state folder.
function Start-WatchIn([string[]]$Extra, [string]$WorkDir, [hashtable]$KeeperEnv = @{}, [string]$Tag = 'watch') {
  $before = @{}
  foreach ($n in $KeeperEnv.Keys) { $before[$n] = [Environment]::GetEnvironmentVariable($n) }
  if (-not ($Extra -contains '-ReleaseGraceSeconds')) { $Extra = @($Extra) + @('-ReleaseGraceSeconds', '1') }
  $keeperArgs = @('-SshExe', $stub, '-Watch') + $Extra
  $out = Join-Path $script:caseHome.StateDir "$Tag.out"
  $err = Join-Path $script:caseHome.StateDir "$Tag.err"
  try {
    foreach ($n in $KeeperEnv.Keys) { Set-ProcessVariable $n $KeeperEnv[$n] }
    $p = Start-Process -FilePath $keeperCmd -ArgumentList (ConvertTo-ArgString $keeperArgs) -WorkingDirectory $WorkDir -PassThru -WindowStyle Hidden -RedirectStandardOutput (Assert-ConformancePath -ConformanceHome $runHome -Path $out) -RedirectStandardError (Assert-ConformancePath -ConformanceHome $runHome -Path $err)
    $null = $p.Handle
  } finally {
    foreach ($n in $KeeperEnv.Keys) { Set-ProcessVariable $n $before[$n] }
  }
  $p
}

# True when two paths name the same place, both put in the long form that Assert-ConformancePath returns.
function Test-SamePath([string]$A, [string]$B) {
  try {
    $x = (Assert-ConformancePath -ConformanceHome $runHome -Path $A).TrimEnd('\')
    $y = (Assert-ConformancePath -ConformanceHome $runHome -Path $B).TrimEnd('\')
    [string]::Equals($x, $y, [StringComparison]::OrdinalIgnoreCase)
  } catch { $false }
}

# True when the process is the case's own stand-in: running, a cmd.exe, started no earlier than $Since (the time the case began),
# and its command line names the stand-in's path. The command line is read only after the name and the start time say that the
# process can be the case's own: a PID in a log may have been reused by a process that the case did not start, and the case
# reads nothing of such a process. Only then may the case end it by this PID.
function Test-OwnStandIn([int]$ProcessId, $Stand, [DateTime]$Since) {
  if ($ProcessId -le 0) { return $false }
  try {
    $proc = Get-Process -Id $ProcessId -ErrorAction Stop
    if ($proc.ProcessName -ine 'cmd') { return $false }
    if ($proc.StartTime.ToUniversalTime() -lt $Since) { return $false }
  } catch { return $false }
  (Get-CommandLine $ProcessId).IndexOf($Stand.Path, [StringComparison]::OrdinalIgnoreCase) -ge 0
}

# The PID of a "guard started" line, or 0.
function Get-StartPid([string]$Line) {
  $m = [regex]::Match($Line, 'pid=(\d+)\z')
  if ($m.Success) { [int]$m.Groups[1].Value } else { 0 }
}

# The variables the stand-in wrote to guard-env.log, as a case-insensitive map of name to value; $null until
# its ENV-END line is there. A value is held for comparison and never printed.
function Read-GuardEnv($Stand) {
  if (-not (Test-Path -LiteralPath $Stand.EnvLog -PathType Leaf)) { return $null }
  try {
    $fs = [IO.File]::Open($Stand.EnvLog, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try { $text = [IO.StreamReader]::new($fs, (Get-StandInEncoding)).ReadToEnd() } finally { $fs.Dispose() }
  } catch { return $null }
  $map = [System.Collections.Generic.Dictionary[string, string]]::new([StringComparer]::OrdinalIgnoreCase)
  $ended = $false
  foreach ($l in ($text -split "`r?`n")) {
    if ($l -ceq 'ENV-END') { $ended = $true }
    elseif ($l -cmatch '\A([A-Za-z0-9_]+)=(.*)\z') { $map[$Matches[1]] = $Matches[2] }
  }
  if ($ended) { $map } else { $null }
}

try {
  # G1. A keeper's first iteration starts the guard. The stand-in stays alive, so that the PID in the line can
  # be tied to a process that was started with the guard's argument.
  if (Test-Selected 'G1') { Invoke-GuardCase 'G1' 'keeper.md#how-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn -Stay
    $began = [DateTime]::UtcNow
    $guardPid = 0
    try {
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1') $stub
      $code = Wait-Watch $p 120
      Add-GuardCheck ($code -eq 0) 'A keeper run with -MaxPasses 1 exits 0' "The exit code was $code."
      $log = Read-LogLines -IncludeGuard
      $starts = @(Get-StartLines $log)
      Add-GuardCheck ($starts.Count -eq 1) 'The keeper logs exactly one "guard started pid=<pid>" line' "Start lines: $($starts.Count). The log: $($log -join ' | ')"
      Add-GuardCheck ($log.Count -eq 2 -and (Test-GuardLine $log[0]) -and $log[0] -cmatch ($logTimeRx + 'guard started pid=\d+\z') -and $log[1] -cmatch ($logTimeRx + 'idle lease=- alias=- pid=-\z')) 'The line has no host= field and comes before the pass''s idle line, which is the only other line' "The log: $($log -join ' | ')"
      $ran = Wait-GuardRuns $stand 1
      $runs = @(Read-GuardRuns $stand)
      Add-GuardCheck ($ran -and $runs.Count -eq 1) 'The stand-in guard ran once' "Runs seen: $($runs.Count)."
      Add-GuardCheck ($runs.Count -ge 1 -and (Get-OneArgRuns $runs).Count -eq $runs.Count) 'The stand-in received exactly the one argument "guard"' "Runs: $($runs.Count); runs with exactly that argument: $((Get-OneArgRuns $runs).Count)."
      Add-GuardCheck ($runs.Count -ge 1 -and @($runs | Where-Object { Test-SameHome $_.Dir }).Count -eq $runs.Count) 'The stand-in''s working folder is the home folder, the keeper''s USERPROFILE' "Runs: $($runs.Count); runs in the home folder: $(@($runs | Where-Object { Test-SameHome $_.Dir }).Count)."
      # The line's PID is the process the start created: for a .cmd program, the cmd.exe that runs it, whose command line holds the
      # stand-in's path. The stand-in stays alive, so the process is running; it is ended by this PID, once its command line is checked.
      $m = if ($starts.Count -ge 1) { [regex]::Match($starts[0], 'pid=(\d+)\z') } else { $null }
      $guardPid = if ($m -and $m.Success) { [int]$m.Groups[1].Value } else { 0 }
      # The command line is read only once the process is seen to be a cmd.exe that started after the case began.
      $alive = $guardPid -gt 0 -and (Test-Alive $guardPid)
      $own = Test-OwnStandIn $guardPid $stand $began
      $cl = if ($own) { Get-CommandLine $guardPid } else { '' }
      $at = if ($own) { $cl.IndexOf($stand.Path, [StringComparison]::OrdinalIgnoreCase) } else { -1 }
      $named = $own -and $at -ge 0 -and $cl.Substring($at + $stand.Path.Length) -match '\Aguard\b|\A"?\s+"?guard\b'
      Add-GuardCheck $named 'The pid= field is the process that was started: a running cmd.exe, started after the case began, whose command line names the stand-in, followed by the argument guard' "The pid field was $(if ($guardPid -gt 0) { 'a number' } else { 'not a number' }); that process was $(if ($alive) { 'running' } else { 'not running' }); it $(if ($own) { 'passed' } else { 'failed' }) the check of its name and start time; its command line $(if ($named) { 'matched' } else { 'did not match' })."
      Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
    } finally {
      if ($p) { Stop-Watch $p }
      # The process that the line names is ended only after it was seen to be a cmd.exe started after the case began, whose command
      # line names this run's stand-in.
      if ($guardPid -gt 0 -and (Test-OwnStandIn $guardPid $stand $began)) { Stop-OwnTree $guardPid }
    }
  } }

  # G2. With -GuardIntervalSeconds not given, 5 passes make one start: at most one start every 300 seconds.
  if (Test-Selected 'G2') { Invoke-GuardCase 'G2' 'keeper.md#when-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '5') $stub
    $code = Wait-Watch $p 120
    Add-GuardCheck ($code -eq 0) 'The keeper ends after 5 passes with exit 0' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    $starts = @(Get-StartLines $log)
    Add-GuardCheck ((@(Get-PassLines $log)).Count -eq 5) 'The keeper made 5 passes, each logging the box''s idle line' "Pass lines: $((@(Get-PassLines $log)).Count). The log: $($log -join ' | ')"
    Add-GuardCheck ($starts.Count -eq 1 -and @(Get-GuardFailedLines $log).Count -eq 0) 'Exactly one "guard started" line is logged across the 5 passes, and no failed line for the guard' "Start lines: $($starts.Count); guard failed lines: $(@(Get-GuardFailedLines $log).Count)."
    $ran = Wait-GuardRuns $stand 1
    $runs = @(Read-GuardRuns $stand)
    Add-GuardCheck ($ran -and $runs.Count -eq 1) 'The stand-in ran exactly once' "Runs seen: $($runs.Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G3. keeper.md's table: -IntervalSeconds 1 and -GuardIntervalSeconds 3, passes over about 10 seconds: no start within 3 seconds of
  # the one before, and a start after it. 11 passes at 1 second last at least 10 seconds, so at least two starts are due. A start is
  # made in a wait at the moment it falls due, and a stall can put two attempts back to back, so no upper bound on a gap and no pass
  # line between two starts is asserted (keeper.md pins neither).
  if (Test-Selected 'G3') { Invoke-GuardCase 'G3' 'keeper.md#when-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $interval = 3
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '11', '-GuardIntervalSeconds', "$interval") $stub
    $code = Wait-Watch $p 240
    Add-GuardCheck ($code -eq 0) 'The keeper ends after 11 passes with exit 0' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    $starts = @(Get-StartLines $log)
    Add-GuardCheck ($starts.Count -ge 2) 'The guard is started again after the interval: two or more "guard started" lines' "Start lines: $($starts.Count). The log: $($log -join ' | ')"
    Add-GuardCheck (@(Get-GuardFailedLines $log).Count -eq 0 -and @($log | Where-Object { Test-GuardLine $_ }).Count -eq $starts.Count) 'Every guard line is a start line: no failed line for the guard' "Guard lines: $(@($log | Where-Object { Test-GuardLine $_ }).Count); start lines: $($starts.Count)."
    $gapsOk = $starts.Count -ge 2
    $gaps = @()
    for ($i = 1; $i -lt $starts.Count; $i++) { $g = Get-LineGap $starts[$i - 1] $starts[$i]; $gaps += $g; if ($g -lt $interval) { $gapsOk = $false } }
    Add-GuardCheck $gapsOk "Each start is at least $interval seconds after the one before (-GuardIntervalSeconds $interval)" "Gaps in seconds between consecutive start lines: $($gaps -join ', ')."
    $ran = Wait-GuardRuns $stand $starts.Count
    $runs = @(Read-GuardRuns $stand)
    Add-GuardCheck ($ran -and $runs.Count -eq $starts.Count) 'The stand-in ran once for each start line' "Start lines: $($starts.Count); runs seen: $($runs.Count)."
    Add-GuardCheck ($runs.Count -ge 1 -and (Get-OneArgRuns $runs).Count -eq $runs.Count) 'Every run received exactly the one argument "guard"' "Runs: $($runs.Count); runs with exactly that argument: $((Get-OneArgRuns $runs).Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G4. A start that falls due in a wait is made there, and the wait ends when it would have: -IntervalSeconds 20,
  # -GuardIntervalSeconds 12, 2 passes. The log is: a start, the first pass's idle line, a start, the second pass's idle line.
  # The second pass's time is at least 20 seconds after the first's; keeper.md adds that the wait ends neither sooner nor later for the
  # start, which the upper bound of 30 seconds, with 10 for load, shows without a bound keeper.md does not pin.
  if (Test-Selected 'G4') { Invoke-GuardCase 'G4' 'keeper.md#when-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $p = Start-Watch @('-IntervalSeconds', '20', '-MaxPasses', '2', '-GuardIntervalSeconds', '12') $stub
    $code = Wait-Watch $p 240
    Add-GuardCheck ($code -eq 0) 'The keeper ends after 2 passes with exit 0' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    $isStart = { param($l) $l -cmatch ($logTimeRx + 'guard started pid=\d+\z') }
    $isIdle = { param($l) $l -cmatch ($logTimeRx + 'idle lease=- alias=- pid=-\z') }
    $shape = $log.Count -eq 4 -and (& $isStart $log[0]) -and (& $isIdle $log[1]) -and (& $isStart $log[2]) -and (& $isIdle $log[3])
    Add-GuardCheck $shape 'Exactly two "guard started" lines: the second between the first pass''s idle line and the second''s' "The log: $($log -join ' | ')"
    if ($shape) {
      # The margins are the size they are because keeper.md H9's lesson is that under load a pass can stretch: the second start is
      # due 12 seconds after the first, so 4 seconds of slack above that (at most 16) keeps a loaded laptop from failing it, while
      # a start made just before the second pass, 20 seconds after the first, is far above it. The second start comes nominally
      # 8 seconds before the second idle line, and at least 4 of them is the margin: a keeper that tries the start only just
      # before each pass puts the start line next to the idle line, with a gap of about 0, and fails it.
      $startGap = Get-LineGap $log[0] $log[2]
      Add-GuardCheck ($startGap -ge 12 -and $startGap -le 16) 'The second start is at least 12 seconds after the first (-GuardIntervalSeconds 12), and no more than 16 with slack for load' "The gap was $startGap seconds."
      $beforeIdle = Get-LineGap $log[2] $log[3]
      Add-GuardCheck ($beforeIdle -ge 4) 'The second start is made in the wait: at least 4 seconds before the second pass''s idle line (nominally about 8), not just before that pass' "The gap was $beforeIdle seconds."
      $passGap = Get-LineGap $log[1] $log[3]
      Add-GuardCheck ($passGap -ge 20 -and $passGap -le 30) 'The second pass is still about 20 seconds after the first: the start made in the wait neither shortens nor lengthens it (at least 20 seconds, and no more than 30 with slack for load)' "The gap was $passGap seconds."
    }
    $ran = Wait-GuardRuns $stand 2
    $runs = @(Read-GuardRuns $stand)
    Add-GuardCheck ($ran -and $runs.Count -eq 2) 'The stand-in ran twice, once for each start line' "Runs seen: $($runs.Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G5. guard.lock held by the case, sharing nothing, from before the keeper starts: no start and no guard line while it is held,
  # and the passes go on. After the case closes it, a start comes without waiting -GuardIntervalSeconds (300 here, by default; the
  # deadline of 60 seconds is shorter), no later than just before the first pass that starts after the close, or in a wait.
  if (Test-Selected 'G5') { Invoke-GuardCase 'G5' 'keeper.md#guardlock-tried-never-held' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $dir = Join-Path $script:caseHome.UserProfile '.agentspace'
    $lockPath = Join-Path $dir 'guard.lock'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $runHome -Path $dir) | Out-Null
    $lock = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $runHome -Path $lockPath), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    $p = $null
    try {
      $p = Start-Watch @('-IntervalSeconds', '1') $stub
      $passes = Wait-Until { @(Get-PassLines (Read-LogLines -IncludeGuard)).Count -ge 4 } 60 $p
      $heldLog = Read-LogLines -IncludeGuard
      $heldRuns = @(Read-GuardRuns $stand)
      Add-GuardCheck $passes 'The passes go on while guard.lock is held: at least 4 pass lines' "Pass lines seen: $(@(Get-PassLines $heldLog).Count)."
      Add-GuardCheck (@($heldLog | Where-Object { Test-GuardLine $_ }).Count -eq 0) 'No guard line is logged while guard.lock is held: no start line and no failed line' "Guard lines seen: $(@($heldLog | Where-Object { Test-GuardLine $_ }).Count)."
      Add-GuardCheck ($heldRuns.Count -eq 0) 'The stand-in does not run while guard.lock is held' "Runs seen: $($heldRuns.Count)."
      $lock.Dispose(); $lock = $null
      # The pass lines at the close: a pass can be in flight across it, so the start line may follow one more.
      $passesAtClose = @(Get-PassLines (Read-LogLines -IncludeGuard)).Count
      $started = Wait-Until { @(Get-StartLines (Read-LogLines -IncludeGuard)).Count -ge 1 } 60 $p
      Add-GuardCheck $started 'Once guard.lock is closed, the start is made without waiting -GuardIntervalSeconds: a "guard started" line within 60 seconds, with 300 the default interval' "Start lines seen: $(@(Get-StartLines (Read-LogLines -IncludeGuard)).Count)."
      # Order only, so that load cannot break it: the start is made just before a pass, so at most the pass in flight across the close
      # comes before it, and no other.
      $finalLog = Read-LogLines -IncludeGuard
      $startAt = -1
      for ($i = 0; $i -lt $finalLog.Count; $i++) { if ($finalLog[$i] -cmatch ($logTimeRx + 'guard started pid=(\d+|-)\z')) { $startAt = $i; break } }
      $passesBefore = if ($startAt -ge 0) { @(Get-PassLines @($finalLog[0..($startAt)])).Count } else { -1 }
      Add-GuardCheck ($startAt -ge 0 -and $passesBefore -le $passesAtClose + 1) 'The start is made no later than just before the first pass that starts after the close: at most one pass line, the one in flight across the close, between the close and the start line' "Pass lines at the close: $passesAtClose; pass lines before the start line: $passesBefore."
      $ran = Wait-GuardRuns $stand 1
      $runs = @(Read-GuardRuns $stand)
      Add-GuardCheck ($ran -and $runs.Count -ge 1 -and (Get-OneArgRuns $runs).Count -eq $runs.Count) 'The stand-in then runs, with exactly the one argument "guard"' "Runs seen: $($runs.Count)."
    } finally {
      if ($lock) { $lock.Dispose() }
      if ($p) { Stop-Watch $p }
    }
    Add-GuardCheck (Test-Path -LiteralPath $lockPath -PathType Leaf) 'The keeper never removed guard.lock' 'The file is gone.'
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G8, a holder that runs. AGENTSPACE_GUARD_CMD names no file, and a box holder runs through a ready lease: the exact failed line,
  # about every -GuardIntervalSeconds (3 here) and never one per pass; the holder is kept in every pass, no hold-off is set, the record is
  # unchanged, and the keeper exits 0 at -MaxPasses.
  if (Test-Selected 'G8a') { Invoke-GuardCase 'G8a' 'keeper.md#what-a-failed-start-changes' {
    Write-Leases @((New-ReadyRecord 'a' $farFuture))
    $holder = Start-TrustedHolder 'a'
    $recordPath = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'
    $before = [IO.File]::ReadAllBytes($recordPath)
    $stand = New-GuardStandIn -Missing
    $interval = 3
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '8', '-GuardIntervalSeconds', "$interval") $stub
    $code = Wait-Watch $p 240
    Add-GuardCheck ($code -eq 0) 'The keeper exits 0 at -MaxPasses: a failed start never changes an exit code' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    $failed = @(Get-GuardFailedLines $log)
    $exact = $logTimeRx + 'failed lease=- alias=- pid=- reason=The guard could not be started: The program that AGENTSPACE_GUARD_CMD names does not exist\.\z'
    Add-GuardCheck ($failed.Count -ge 2 -and @($failed | Where-Object { $_ -cmatch $exact }).Count -eq $failed.Count) 'Each attempt logs exactly "failed lease=- alias=- pid=- reason=The guard could not be started: The program that AGENTSPACE_GUARD_CMD names does not exist."' "Guard failed lines: $($failed.Count); exact: $(@($failed | Where-Object { $_ -cmatch $exact }).Count). The log: $($log -join ' | ')"
    Add-GuardCheck (@(Get-StartLines $log).Count -eq 0) 'No "guard started" line is logged' "Start lines: $(@(Get-StartLines $log).Count)."
    $gapsOk = $failed.Count -ge 2
    $gaps = @()
    for ($i = 1; $i -lt $failed.Count; $i++) { $g = Get-LineGap $failed[$i - 1] $failed[$i]; $gaps += $g; if ($g -lt $interval) { $gapsOk = $false } }
    Add-GuardCheck $gapsOk "The next failed line is no sooner than -GuardIntervalSeconds ($interval) after the last: a failure that lasts writes a line every interval, never one per pass" "Gaps in seconds: $($gaps -join ', ')."
    $pass = @(Get-PassLines $log)
    $kept = @($pass | Where-Object { Test-LogLine $_ 'kept' (Get-TestId 'a') (Get-TestAlias 'a') $holder.Pid })
    Add-GuardCheck ($pass.Count -eq 8 -and $kept.Count -eq 8) 'The holder is kept in every one of the 8 passes: one kept line each, with the lease, alias and PID, and no other pass line' "Pass lines: $($pass.Count); kept lines for the holder: $($kept.Count). The log: $($log -join ' | ')"
    Add-GuardCheck (@($pass | Where-Object { $_ -cmatch ' (backoff|grace) seconds=' }).Count -eq 0) 'No hold-off is set: no backoff and no grace line' "Wait lines: $(@($pass | Where-Object { $_ -cmatch ' (backoff|grace) seconds=' }).Count)."
    $after = if (Test-Path -LiteralPath $recordPath) { [IO.File]::ReadAllBytes($recordPath) } else { $null }
    Add-GuardCheck ($null -ne $after -and [Convert]::ToBase64String($after) -ceq [Convert]::ToBase64String($before)) 'The holder''s record is unchanged, byte for byte' 'The record was changed or removed.'
    Add-GuardCheck (Test-Alive $holder.Pid) 'The holder is still running' 'It was stopped.'
    Add-GuardCheck (-not (Test-Path -LiteralPath $stand.Path)) 'The precondition held to the end: the file that AGENTSPACE_GUARD_CMD names still does not exist, so nothing could have been started' 'The file exists.'
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G8, a holder that is due. The same ready lease with no holder running, once with a working stand-in and once with a program that
  # does not exist: the keeper's holder lines are the same, a started line and then a kept line, a holder runs, and a record is written.
  # PIDs and times differ, so the two runs are compared by the actions of their pass lines.
  if (Test-Selected 'G8b') {
    $shapes = @{}
    foreach ($variant in 'working', 'missing') {
      Invoke-GuardCase "G8b-$variant" 'keeper.md#what-a-failed-start-changes' {
        Write-Leases @((New-ReadyRecord 'a' $farFuture))
        $stand = if ($variant -ceq 'working') { New-GuardStandIn } else { New-GuardStandIn -Missing }
        $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '3') $stub
        $code = Wait-Watch $p 240
        $log = Read-LogLines -IncludeGuard
        $pass = @(Get-PassLines $log)
        $actions = @($pass | ForEach-Object { if ($_ -cmatch ($logTimeRx + '(\w+) ')) { $Matches[1] } })
        $shapes[$variant] = $actions -join ','
        Add-GuardCheck ($code -eq 0) "The keeper exits 0 at -MaxPasses ($variant guard)" "The exit code was $code."
        $rec = Read-Record
        $startedLines = @($pass | Where-Object { $_ -cmatch ($logTimeRx + 'started ') })
        Add-GuardCheck ($startedLines.Count -eq 1 -and $startedLines[0] -cmatch ('lease=' + [regex]::Escape((Get-TestId 'a')) + ' alias=' + [regex]::Escape((Get-TestAlias 'a')) + ' pid=\d+')) "The first pass starts the holder: one started line, with the lease and alias ($variant guard)" "Started lines: $($startedLines.Count). The log: $($log -join ' | ')"
        Add-GuardCheck ($null -ne $rec -and (Test-Alive ([int]$rec['pid']))) "The holder runs and is recorded ($variant guard)" 'No record, or the recorded process is not running.'
        Add-GuardCheck ($actions.Count -eq 3 -and $actions[0] -ceq 'started' -and @($actions[1..2] | Where-Object { $_ -ceq 'kept' }).Count -eq 2) "The next two passes keep it: the pass lines are started, kept, kept ($variant guard)" "Actions seen: $($actions -join ', ')."
        Add-GuardCheck (@($pass | Where-Object { $_ -cmatch ' (backoff|grace) seconds=' }).Count -eq 0) "No hold-off is set: no backoff and no grace line ($variant guard)" "Wait lines: $(@($pass | Where-Object { $_ -cmatch ' (backoff|grace) seconds=' }).Count)."
        if ($variant -ceq 'working') {
          Add-GuardCheck (@(Get-StartLines $log).Count -eq 1 -and @(Get-GuardFailedLines $log).Count -eq 0) 'The reference run logs one "guard started" line and no failed line for the guard' "Start lines: $(@(Get-StartLines $log).Count); guard failed lines: $(@(Get-GuardFailedLines $log).Count)."
        } else {
          $failed = @(Get-GuardFailedLines $log)
          $exact = $logTimeRx + 'failed lease=- alias=- pid=- reason=The guard could not be started: The program that AGENTSPACE_GUARD_CMD names does not exist\.\z'
          Add-GuardCheck ($failed.Count -eq 1 -and $failed[0] -cmatch $exact -and @(Get-StartLines $log).Count -eq 0) 'The failing run logs exactly one failed line for the guard, with keeper.md''s text, and no start line' "Guard failed lines: $($failed.Count); start lines: $(@(Get-StartLines $log).Count). The log: $($log -join ' | ')"
          Add-GuardCheck ($log.Count -ge 1 -and $failed.Count -eq 1 -and $log[0] -ceq $failed[0]) 'The failed line comes before the first pass''s line, as a start would' "The log: $($log -join ' | ')"
        }
        Add-GuardCheck (Test-TripwiresQuiet) "No aws, ssh or scp stand-in was started, and neither was the default guard tripwire ($variant guard)" (Get-TripwireReason)
      }
    }
    $script:caseId = 'G8b'; $script:caseSpec = 'keeper.md#what-a-failed-start-changes'
    Add-GuardCheck ($shapes.ContainsKey('working') -and $shapes.ContainsKey('missing') -and $shapes['working'] -ceq $shapes['missing'] -and $shapes['working'] -ceq 'started,kept,kept') 'The failing run''s holder lines are those of the run without the failure' "Actions with a working guard: $($shapes['working']); with a missing one: $($shapes['missing'])."
  }

  # G6. A folder in place of <home>\.agentspace\guard.lock is not a free lock (guard.lock: tried, never held): each attempt fails with
  # keeper.md's line, the Windows error number alone (its value is not pinned: any digits), no stand-in runs, the next such line is no
  # sooner than -GuardIntervalSeconds (5) after the last, and every pass logs what it would without the failure.
  if (Test-Selected 'G6') { Invoke-GuardCase 'G6' 'keeper.md#guardlock-tried-never-held' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $lockPath = Join-Path (Join-Path $script:caseHome.UserProfile '.agentspace') 'guard.lock'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $runHome -Path $lockPath) | Out-Null
    $interval = 5
    $passes = 14
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', "$passes", '-GuardIntervalSeconds', "$interval") $stub
    $code = Wait-Watch $p 240
    Add-GuardCheck ($code -eq 0) "The keeper ends after $passes passes with exit 0: a failed start never changes an exit code" "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    $failed = @(Get-GuardFailedLines $log)
    $exact = $logTimeRx + 'failed lease=- alias=- pid=- reason=The guard could not be started: guard\.lock could not be opened: Windows error \d+\.\z'
    Add-GuardCheck ($failed.Count -ge 2 -and @($failed | Where-Object { $_ -cmatch $exact }).Count -eq $failed.Count) 'Each attempt logs exactly "failed lease=- alias=- pid=- reason=The guard could not be started: guard.lock could not be opened: Windows error <n>."' "Guard failed lines: $($failed.Count); exact: $(@($failed | Where-Object { $_ -cmatch $exact }).Count). The log: $($log -join ' | ')"
    Add-GuardCheck (@(Get-StartLines $log).Count -eq 0) 'No "guard started" line is logged' "Start lines: $(@(Get-StartLines $log).Count)."
    $gapsOk = $failed.Count -ge 2
    $gaps = @()
    for ($i = 1; $i -lt $failed.Count; $i++) { $g = Get-LineGap $failed[$i - 1] $failed[$i]; $gaps += $g; if ($g -lt $interval) { $gapsOk = $false } }
    Add-GuardCheck $gapsOk "The next such line is no sooner than -GuardIntervalSeconds ($interval) after the last: never one per pass" "Gaps in seconds: $($gaps -join ', ')."
    $pass = @(Get-PassLines $log)
    $idle = @($pass | Where-Object { $_ -cmatch ($logTimeRx + 'idle lease=- alias=- pid=-\z') })
    Add-GuardCheck ($pass.Count -eq $passes -and $idle.Count -eq $passes) "Every pass's lines are as without the failure: $passes idle lines and no other pass line" "Pass lines: $($pass.Count); idle lines: $($idle.Count). The log: $($log -join ' | ')"
    Add-GuardCheck (@(Read-GuardRuns $stand).Count -eq 0) 'The stand-in did not run' "Runs seen: $(@(Read-GuardRuns $stand).Count)."
    Add-GuardCheck (Test-Path -LiteralPath $lockPath -PathType Container) 'The keeper left the folder at guard.lock as it was' 'The folder is gone, or is not a folder.'
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G7. No <home>\.agentspace folder: guard.lock does not exist, so no guard holds it; the keeper's try creates neither the folder nor the
  # file, and the start is made.
  if (Test-Selected 'G7') { Invoke-GuardCase 'G7' 'keeper.md#guardlock-tried-never-held' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $agentDir = Join-Path $script:caseHome.UserProfile '.agentspace'
    Add-GuardCheck (-not (Test-Path -LiteralPath $agentDir)) 'The case starts with no .agentspace folder in the home' 'The folder was there before the keeper ran.'
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1') $stub
    $code = Wait-Watch $p 120
    Add-GuardCheck ($code -eq 0) 'A keeper run with -MaxPasses 1 exits 0' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    Add-GuardCheck (@(Get-StartLines $log).Count -eq 1 -and @(Get-GuardFailedLines $log).Count -eq 0) 'One "guard started" line is logged, and no failed line for the guard' "Start lines: $(@(Get-StartLines $log).Count); guard failed lines: $(@(Get-GuardFailedLines $log).Count). The log: $($log -join ' | ')"
    $ran = Wait-GuardRuns $stand 1
    Add-GuardCheck ($ran) 'The stand-in ran' "Runs seen: $(@(Read-GuardRuns $stand).Count)."
    Add-GuardCheck (-not (Test-Path -LiteralPath $agentDir)) 'The keeper created no .agentspace folder' 'The folder exists.'
    Add-GuardCheck (-not (Test-Path -LiteralPath (Join-Path $agentDir 'guard.lock'))) 'The keeper created no guard.lock' 'The file exists.'
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G9. The keeper's working folder holds the stand-in, and AGENTSPACE_GUARD_CMD is its relative path: the keeper made the value full
  # against its working folder, and the stand-in runs, in the home (the guard's working folder), not in the keeper's.
  if (Test-Selected 'G9') { Invoke-GuardCase 'G9' 'keeper.md#how-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn -Folder 'guard-work\bin' -GuardCmd 'bin\guard-standin.cmd'
    $work = Join-Path $script:caseHome.UserProfile 'guard-work'
    $p = Start-WatchIn @('-IntervalSeconds', '1', '-MaxPasses', '1') $work
    $code = Wait-Watch $p 120
    Add-GuardCheck ($code -eq 0) 'A keeper run with -MaxPasses 1 exits 0' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    Add-GuardCheck (@(Get-StartLines $log).Count -eq 1 -and @(Get-GuardFailedLines $log).Count -eq 0) 'One "guard started" line is logged, and no failed line for the guard' "Start lines: $(@(Get-StartLines $log).Count); guard failed lines: $(@(Get-GuardFailedLines $log).Count). The log: $($log -join ' | ')"
    $ran = Wait-GuardRuns $stand 1
    $runs = @(Read-GuardRuns $stand)
    Add-GuardCheck ($ran -and $runs.Count -eq 1) 'The stand-in ran once: the relative value was made full against the keeper''s working folder' "Runs seen: $($runs.Count)."
    Add-GuardCheck ($runs.Count -ge 1 -and (Get-OneArgRuns $runs).Count -eq $runs.Count) 'The stand-in received exactly the one argument "guard"' "Runs: $($runs.Count); runs with exactly that argument: $((Get-OneArgRuns $runs).Count)."
    Add-GuardCheck ($runs.Count -ge 1 -and @($runs | Where-Object { Test-SameHome $_.Dir }).Count -eq $runs.Count) 'The stand-in''s working folder is the home folder, not the keeper''s working folder' "Runs: $($runs.Count); runs in the home folder: $(@($runs | Where-Object { Test-SameHome $_.Dir }).Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G10. The guard's environment, compared by variable name only. The keeper is given AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR as
  # relative paths, AGENTSPACE_PROXY_COMMAND, the harness's tripwires for AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD and AGENTSPACE_SCP_CMD, a
  # relative AGENTSPACE_GUARD_CMD, and a variable of the case's own whose name does not end _CMD. The stand-in writes its own environment
  # with "set"; a mismatch is reported by name, never by value.
  if (Test-Selected 'G10') { Invoke-GuardCase 'G10' 'keeper.md#the-guards-environment' {
    Write-Leases @()
    $stand = New-GuardStandIn -Folder 'guard-work\bin' -GuardCmd 'bin\guard-standin.cmd' -DumpEnv
    $work = Join-Path $script:caseHome.UserProfile 'guard-work'
    $proxy = 'ssh -W "case host":22 proxy-example'
    $keeperEnv = @{
      AGENTSPACE_STATE_DIR = [IO.Path]::GetRelativePath($work, $script:caseHome.StateDir)
      AGENTSPACE_SSH_DIR = [IO.Path]::GetRelativePath($work, $script:caseHome.SshDir)
      AGENTSPACE_PROXY_COMMAND = $proxy
      AGENTSPACE_CASE_OWN = 'case-own-value'
    }
    # Compared exactly, as the keeper has them: the values are held in this process and never printed.
    $exact = [ordered]@{
      AGENTSPACE_PROXY_COMMAND = $proxy
      AGENTSPACE_AWS_CMD = [Environment]::GetEnvironmentVariable('AGENTSPACE_AWS_CMD')
      AGENTSPACE_SSH_CMD = [Environment]::GetEnvironmentVariable('AGENTSPACE_SSH_CMD')
      AGENTSPACE_SCP_CMD = [Environment]::GetEnvironmentVariable('AGENTSPACE_SCP_CMD')
      AGENTSPACE_GUARD_CMD = $stand.Value
    }
    $p = Start-WatchIn @('-IntervalSeconds', '1', '-MaxPasses', '1') $work $keeperEnv
    $code = Wait-Watch $p 120
    Add-GuardCheck ($code -eq 0) 'A keeper run with -MaxPasses 1 exits 0' "The exit code was $code."
    $ran = Wait-Until { $null -ne (Read-GuardEnv $stand) } 30
    $seen = Read-GuardEnv $stand
    Add-GuardCheck ($ran -and $null -ne $seen) 'The stand-in ran and wrote its environment' 'The stand-in wrote no environment.'
    if ($null -ne $seen) {
      $bad = @()
      if (-not ($seen.ContainsKey('USERPROFILE') -and (Test-SamePath $seen['USERPROFILE'] $script:caseHome.UserProfile))) { $bad += 'USERPROFILE' }
      Add-GuardCheck ($bad.Count -eq 0) 'The guard sees USERPROFILE as the home folder' "Mismatch by name: $($bad -join ', ')."
      $bad = @()
      if (-not ($seen.ContainsKey('AGENTSPACE_STATE_DIR') -and [IO.Path]::IsPathFullyQualified($seen['AGENTSPACE_STATE_DIR']) -and (Test-SamePath $seen['AGENTSPACE_STATE_DIR'] $script:caseHome.StateDir))) { $bad += 'AGENTSPACE_STATE_DIR' }
      if (-not ($seen.ContainsKey('AGENTSPACE_SSH_DIR') -and [IO.Path]::IsPathFullyQualified($seen['AGENTSPACE_SSH_DIR']) -and (Test-SamePath $seen['AGENTSPACE_SSH_DIR'] $script:caseHome.SshDir))) { $bad += 'AGENTSPACE_SSH_DIR' }
      Add-GuardCheck ($bad.Count -eq 0) 'The guard sees the relative AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR made full (a fully qualified path, as the stand-in saw it), naming the case''s own folders' "Mismatch by name: $($bad -join ', ')."
      foreach ($name in 'AGENTSPACE_PROXY_COMMAND', 'AGENTSPACE_AWS_CMD', 'AGENTSPACE_SSH_CMD', 'AGENTSPACE_SCP_CMD', 'AGENTSPACE_GUARD_CMD') {
        $same = $seen.ContainsKey($name) -and [string]::Equals($seen[$name], [string]$exact[$name], [StringComparison]::Ordinal)
        $what = if ($name -ceq 'AGENTSPACE_PROXY_COMMAND') { 'a command line, exactly as the keeper has it' } elseif ($name -ceq 'AGENTSPACE_GUARD_CMD') { 'exactly as given, not made full' } else { 'a seam, exactly as the keeper has it' }
        Add-GuardCheck $same "The guard sees $name $what" "Mismatch by name: $name."
      }
      Add-GuardCheck (-not $seen.ContainsKey('AGENTSPACE_CASE_OWN')) 'The guard does not see the case''s own variable, whose name does not end _CMD' 'Mismatch by name: AGENTSPACE_CASE_OWN.'
    }
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G11. The stand-in stays alive. The keeper exits 0 while it runs: it is outside the keeper's job, and outlives it. A new keeper, while it
  # runs, takes board-holder-keeper.lock and logs no "another keeper is running" line, so the guard holds no handle of the keeper's lock.
  # The new keeper starts a stand-in of its own, since this stand-in holds no guard.lock; each is ended by the PID of its start line, and only
  # after that process is seen to be the case's own stand-in: running, its command line naming the stand-in's path, started after the case began.
  if (Test-Selected 'G11') { Invoke-GuardCase 'G11' 'keeper.md#how-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn -Stay
    $began = [DateTime]::UtcNow
    $guardPids = @()
    $p = $null; $p2 = $null
    try {
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1') $stub
      $code = Wait-Watch $p 120
      $log = Read-LogLines -IncludeGuard
      $starts = @(Get-StartLines $log)
      $first = if ($starts.Count -ge 1) { Get-StartPid $starts[0] } else { 0 }
      if ($first -gt 0) { $guardPids += $first }
      Add-GuardCheck ($starts.Count -eq 1 -and $first -gt 0) 'The keeper logs one "guard started pid=<pid>" line, with the process ID' "Start lines: $($starts.Count); the PID was $(if ($first -gt 0) { 'a number' } else { 'not a number' })."
      $own = Test-OwnStandIn $first $stand $began
      Add-GuardCheck $own 'The process of the first start line is the case''s own stand-in, started after the case began' "The process $(if ($first -gt 0 -and (Test-Alive $first)) { 'is running, and failed the check of its command line or its start time' } else { 'is not running' })."
      Add-GuardCheck ($code -eq 0) 'The keeper exits 0' "The exit code was $code."
      $diedAfter = Wait-Until { -not (Test-Alive $first) } 3
      Add-GuardCheck ($own -and -not $diedAfter) 'The stand-in is still running 3 seconds after the keeper exited: it is outside the keeper''s job' 'The stand-in ended with the keeper, or never ran.'
      $p2 = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1') $stub -Tag 'second'
      $code2 = Wait-Watch $p2 120
      $log2 = Read-LogLines -IncludeGuard
      foreach ($l in @(Get-StartLines $log2)) { $q = Get-StartPid $l; if ($q -gt 0 -and $guardPids -notcontains $q) { $guardPids += $q } }
      Add-GuardCheck ($code2 -eq 0) 'The new keeper, started while the stand-in runs, runs its pass and exits 0' "The exit code was $code2."
      Add-GuardCheck ($own -and @(Get-OtherKeeperLines $log2).Count -eq 0 -and @(Get-PassLines $log2).Count -eq 2) 'The new keeper took board-holder-keeper.lock: no "another keeper is running" line, and its pass is logged beside the first keeper''s' "Other-keeper lines: $(@(Get-OtherKeeperLines $log2).Count); pass lines: $(@(Get-PassLines $log2).Count) (2 expected). The log: $($log2 -join ' | ')"
      Add-GuardCheck (Test-OwnStandIn $first $stand $began) 'The first stand-in is still running while the new keeper ran' 'The first stand-in is no longer running.'
    } finally {
      foreach ($q in $guardPids) { if (Test-OwnStandIn $q $stand $began) { Stop-OwnTree $q } }
      if ($p) { Stop-Watch $p }
      if ($p2) { Stop-Watch $p2 }
    }
    $stopped = Wait-Until { @($guardPids | Where-Object { Test-Alive $_ }).Count -eq 0 } 30
    Add-GuardCheck ($stopped -and $guardPids.Count -ge 1) 'The case ended every stand-in it started, by the PID of its start line' "Stand-ins still running: $(@($guardPids | Where-Object { Test-Alive $_ }).Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G12. A one-pass run, with AGENTSPACE_GUARD_CMD set, never starts the guard and logs no line for one. The run's own exit is the point at
  # which a start, if it were made, has returned; the stand-in is then given 4 seconds to appear, which it does not.
  if (Test-Selected 'G12') { Invoke-GuardCase 'G12' 'keeper.md#when-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $code = Invoke-Keeper ''
    Add-GuardCheck ($code -eq 0) 'The one-pass run exits 0' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    Add-GuardCheck ($log.Count -eq 1 -and $log[0] -cmatch ($logTimeRx + 'idle lease=- alias=- pid=-\z')) 'The run logs the pass''s idle line alone: no guard line' "The log: $($log -join ' | ')"
    $ran = Wait-GuardRuns $stand 1 4
    Add-GuardCheck (-not $ran) 'The stand-in did not run' "Runs seen: $(@(Read-GuardRuns $stand).Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G13. The case holds board-holder.lock, so the pass logs skipped; the start is not inside a pass and does not wait on that lock, so the
  # start line is logged, before the skipped line, all the same.
  if (Test-Selected 'G13') { Invoke-GuardCase 'G13' 'keeper.md#when-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $lockPath = Join-Path $script:caseHome.StateDir 'board-holder.lock'
    $held = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $runHome -Path $lockPath), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1') $stub
      $code = Wait-Watch $p 120
    } finally { $held.Dispose() }
    Add-GuardCheck ($code -eq 0) 'A keeper run with -MaxPasses 1 exits 0' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    $shape = ($log.Count -eq 2 -and $log[0] -cmatch ($logTimeRx + 'guard started pid=\d+\z') -and $log[1] -cmatch ($logTimeRx + 'skipped lease=- alias=- pid=- reason=locked\z'))
    Add-GuardCheck $shape 'The log is a "guard started" line, then the pass''s "skipped ... reason=locked" line' "The log: $($log -join ' | ')"
    # The skipped line carries the pass's time, read before the lock's retry, so a start that waited on
    # board-holder.lock would be logged later than it.
    Add-GuardCheck ($shape -and (Get-LogTime $log[0]) -le (Get-LogTime $log[1])) 'The start line''s time is no later than the skipped line''s: the start did not wait on board-holder.lock' "The log: $($log -join ' | ')"
    $ran = Wait-GuardRuns $stand 1
    Add-GuardCheck ($ran -and @(Read-GuardRuns $stand).Count -eq 1) 'The stand-in ran once' "Runs seen: $(@(Read-GuardRuns $stand).Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G14. The stand-in is a .cmd whose path holds a space: the keeper starts it through cmd.exe, and it receives exactly the one argument guard.
  if (Test-Selected 'G14') { Invoke-GuardCase 'G14' 'keeper.md#how-the-keeper-starts-it' {
    Write-Leases @()
    $stand = New-GuardStandIn -Folder 'guard stand in'
    Add-GuardCheck ($stand.Path.Contains(' ')) 'The stand-in''s path holds a space' 'The path holds no space.'
    $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '1') $stub
    $code = Wait-Watch $p 120
    Add-GuardCheck ($code -eq 0) 'A keeper run with -MaxPasses 1 exits 0' "The exit code was $code."
    $log = Read-LogLines -IncludeGuard
    Add-GuardCheck (@(Get-StartLines $log).Count -eq 1 -and @(Get-GuardFailedLines $log).Count -eq 0) 'One "guard started" line is logged, and no failed line for the guard' "Start lines: $(@(Get-StartLines $log).Count); guard failed lines: $(@(Get-GuardFailedLines $log).Count). The log: $($log -join ' | ')"
    $ran = Wait-GuardRuns $stand 1
    $runs = @(Read-GuardRuns $stand)
    Add-GuardCheck ($ran -and $runs.Count -eq 1) 'The stand-in ran once' "Runs seen: $($runs.Count)."
    Add-GuardCheck ($runs.Count -ge 1 -and (Get-OneArgRuns $runs).Count -eq $runs.Count) 'The stand-in received exactly the one argument "guard"' "Runs: $($runs.Count); runs with exactly that argument: $((Get-OneArgRuns $runs).Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
  } }

  # G15. The two usage errors for -GuardIntervalSeconds: three lines to stderr, byte for byte (the line ends are not compared), exit 2, and
  # nothing logged: no log line, nothing on stdout, no run of the stand-in. The usage lines are those of keeper.md, Usage errors; the hidden
  # arguments are not in them.
  $usageLines = @('Usage: agentspace-keeper [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]',
    '       agentspace-keeper -Watch [-SshExe <full path>] [-IntervalSeconds <n>] [-MaxPasses <n>] [-BackoffBaseSeconds <n>] [-BackoffMaxSeconds <n>] [-ReleaseGraceSeconds <n>]')
  $g15 = @(
    @{ Id = 'G15a'; Args = @('-SshExe', $stub, '-GuardIntervalSeconds', '5'); Message = '-GuardIntervalSeconds is for -Watch.'; Text = '-GuardIntervalSeconds 5 without -Watch' },
    @{ Id = 'G15b'; Args = @('-SshExe', $stub, '-Watch', '-GuardIntervalSeconds', '0'); Message = '-GuardIntervalSeconds ''0'' is not a whole number from 1 to 86400.'; Text = '-Watch -GuardIntervalSeconds 0' })
  foreach ($c in $g15) {
    if (-not (Test-Selected $c.Id)) { continue }
    $argv = $c.Args; $message = $c.Message; $text = $c.Text
    Invoke-GuardCase $c.Id 'keeper.md#usage-errors' {
      Write-Leases @()
      $stand = New-GuardStandIn
      $p = Start-KeeperProcess $argv 'usage'
      $ended = $p.WaitForExit(60000)
      if (-not $ended) { Stop-OwnTree $p.Id }
      Add-GuardCheck ($ended -and $p.ExitCode -eq 2) "$text exits 2" "Ended: $ended; exit code: $(if ($ended) { $p.ExitCode } else { 'none' })."
      $errPath = Join-Path $script:caseHome.StateDir 'usage.err'
      $errText = if (Test-Path -LiteralPath $errPath) { [IO.File]::ReadAllText($errPath) } else { '' }
      $want = (@("agentspace-keeper: $message") + $usageLines) -join "`n"
      # The program's name is not asserted (keeper.md, "Command line": a check MUST NOT depend on which of the two names a message
      # prints): agentspace-keeper and board-holder.ps1 are both taken for it, in the message and in both usage lines.
      $got = (($errText -replace "`r`n", "`n").TrimEnd("`n") -split "`n" | ForEach-Object { $_ -replace '\A((?:Usage: |       )?)board-holder\.ps1', '${1}agentspace-keeper' }) -join "`n"
      Add-GuardCheck ($got -ceq $want) "$text writes agentspace-keeper: $message, then the two usage lines, to stderr" "The error text had $(@($got -split "`n").Count) line(s); its first line $(if (@($got -split "`n")[0] -ceq "agentspace-keeper: $message") { 'matched' } else { 'did not match' })."
      $outPath = Join-Path $script:caseHome.StateDir 'usage.out'
      $outLen = if (Test-Path -LiteralPath $outPath) { (Get-Item -LiteralPath $outPath).Length } else { 0 }
      $logged = Read-LogLines -IncludeGuard
      Add-GuardCheck ($outLen -eq 0 -and $logged.Count -eq 0) "$text logs nothing: no log line and nothing on stdout" "Log lines: $($logged.Count) ($($logged -join ' | ')); stdout bytes: $outLen."
      $ran = Wait-GuardRuns $stand 1 3
      Add-GuardCheck (-not $ran) "$text starts no guard" "Runs seen: $(@(Read-GuardRuns $stand).Count)."
      Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
    }
  }

  # G16. The case holds <state>\board-holder-keeper.lock as a keeper does: for reading and writing, sharing read only, with its own PID as
  # decimal digits. A second keeper cannot take it, logs its one "another keeper is running pid=<those digits>" line, exits 0, and starts no
  # guard.
  if (Test-Selected 'G16') { Invoke-GuardCase 'G16' 'keeper.md#board-holder-keeperlock-one-keeper-at-a-time' {
    Write-Leases @()
    $stand = New-GuardStandIn
    $lockPath = Join-Path $script:caseHome.StateDir 'board-holder-keeper.lock'
    $held = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $runHome -Path $lockPath), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Read)
    try {
      $digits = [Text.Encoding]::ASCII.GetBytes("$PID")
      $held.Write($digits, 0, $digits.Length)
      $held.Flush()
      $p = Start-Watch @('-IntervalSeconds', '1', '-MaxPasses', '3') $stub
      $code = Wait-Watch $p 120
    } finally { $held.Dispose() }
    Add-GuardCheck ($code -eq 0) 'The second keeper exits 0' "The exit code was $code (-999 means it did not end in time)."
    $log = Read-LogLines -IncludeGuard
    Add-GuardCheck ($log.Count -eq 1 -and $log[0] -cmatch ($logTimeRx + 'another keeper is running pid=' + $PID + '\z')) 'The log is the one "another keeper is running pid=<pid>" line, with the holder''s PID, and no guard line' "The log: $($log -join ' | ')"
    Add-GuardCheck (@(Read-GuardRuns $stand).Count -eq 0 -and -not (Wait-GuardRuns $stand 1 3)) 'The stand-in did not run' "Runs seen: $(@(Read-GuardRuns $stand).Count)."
    Add-GuardCheck (Test-TripwiresQuiet) 'No aws, ssh or scp stand-in was started, and neither was the default guard tripwire' (Get-TripwireReason)
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

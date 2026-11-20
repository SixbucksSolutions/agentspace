#Requires -Version 7
# A conformance check under D34: it drives the keeper only as a separate process, through what docs/keeper.md documents, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL AGAINST THE PROTOTYPE. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail"): the
# prototype writes a relative state folder into the launch request as given (docs/state.md,
# "Requirements the prototype does not meet yet", "Every path written into a file is a full path").
#
# CHECKS state.md, "The home folder" (E0.06-T04-S03, #0133, #0110 point 4): an implementation MUST
# make a relative AGENTSPACE_STATE_DIR a full path before it writes it into a file, and "A holder's
# launch files" (the request carries full paths, never relative ones), because the launcher that
# reads the request runs with another working folder and another environment.
#
# One case, against the program that AGENTSPACE_KEEPER_CMD names (or its default, through the harness),
# with the keeper's own arguments unchanged: <program> -SshExe <stub> -Now <time>.
# - The keeper is started with AGENTSPACE_STATE_DIR set to a RELATIVE folder name, and with a working
#   folder of the check's choosing (Start-Process -WorkingDirectory), which is the folder that holds
#   the case's state folder, inside the case's home. The variable is made relative for that one
#   start, and put back to the full path at once, so the check's own reads of the state folder are
#   not affected.
# - A watcher inside the case keeps a copy of the launch request as it appears (keeper.md, "Checking
#   the keeper": a check MAY watch the state folder for the launch files), and a pass it missed is run
#   again, from nothing, as test-keeper.ps1 does.
# - It holds the request to state.md: its `log`, `directory` and `result` are full paths, and are the
#   full paths the relative state folder resolves to against the keeper's working folder (the log and
#   the result) and the home folder (the directory). The precondition is a check of its own: the value
#   the keeper was started with is not a full path, so the case is a real one.
#
# What it touches: the same as test-keeper.ps1 (conformance/keeper/keeper-common.ps1): a throwaway
# folder under $env:TEMP, the harness's child environment for it on every process, and only processes
# this run started, each stopped by a checked PID. The real %USERPROFILE%\.agentspace, ~/.ssh, ssh.exe,
# keeper task and box are never read, written, listed or hashed. Every write and delete goes through
# Assert-ConformancePath at the call.
#
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line; exit 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.
# Run:  pwsh -NoProfile -File conformance\keeper\test-unmet-launch-request-full-paths.ps1
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) {
  Write-Output 'SKIPPED keeper.md#where-it-runs: The keeper specified is a Windows keeper; this host is not Windows.'
  Write-Output '0 passed, 0 failed, 1 skipped'
  exit 2
}

. (Join-Path $PSScriptRoot 'keeper-common.ps1')

# Resolved before the case, which strips AGENTSPACE_* from the environment.
$keeperCmd = Resolve-KeeperProgram
if (-not $keeperCmd -or -not (Test-Path -LiteralPath $keeperCmd -PathType Leaf)) {
  Write-ConformanceResult -Name 'keeper.md#command-line: The program that AGENTSPACE_KEEPER_CMD names exists' -Status FAIL -Reason $(if ($keeperCmd) { "$keeperCmd is not a file." } else { 'Neither the variable nor its default names a program.' })
  Exit-ConformanceRun
}

Initialize-KeeperRun
# The folder the run's own folder MUST be directly under, taken before any case changes the environment.
$tempRoot = [IO.Path]::GetFullPath($env:TEMP)
$prefix = 'state.md#a-holders-launch-files'

function Add-Check([bool]$Ok, [string]$Text, [string]$Why = '') {
  $name = "${prefix}: $Text"
  if ($Ok) { Write-ConformanceResult -Name $name -Status PASS } else { Write-ConformanceResult -Name $name -Status FAIL -Reason $Why }
}

try {
  $caseHome = New-CaseHome
  $stateFull = $caseHome.StateDir
  $workDir = Split-Path -Parent $stateFull
  $relative = Split-Path -Leaf $stateFull   # 'state': relative to the keeper's working folder.

  Assert-ConformancePath -ConformanceHome $caseHome -Path (Join-Path $stateFull 'leases.json') | Out-Null
  Write-Leases @((New-ReadyRecord 'a' '2099-01-01T00:00:00Z'))

  # The case is a real one only when the value the keeper starts with is not a full path.
  $precondition = -not [IO.Path]::IsPathRooted($relative)
  if (-not $precondition) { Add-Check $false 'Precondition: the keeper is started with a relative AGENTSPACE_STATE_DIR' 'The value made for the keeper was a full path.' }

  $caught = @{}
  $code = $null
  for ($attempt = 1; $attempt -le 4 -and $precondition; $attempt++) {
    $cap = [AgentspaceTest.FileCapture]::new($stateFull)
    $cap.Start()
    try {
      $out = Join-Path $stateFull 'keeper.out'
      $err = Join-Path $stateFull 'keeper.err'
      # Relative for this one start only: the process takes its environment when it is created.
      Set-ProcessVariable 'AGENTSPACE_STATE_DIR' $relative
      try {
        $p = Start-Process -FilePath $keeperCmd -ArgumentList (ConvertTo-ArgString @('-SshExe', $stub, '-Now', $now)) -WorkingDirectory $workDir -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
      } finally {
        Set-ProcessVariable 'AGENTSPACE_STATE_DIR' $stateFull
      }
      $null = $p.Handle
      if (-not $p.WaitForExit(180000)) { Stop-OwnTree $p.Id; $code = -999 } else { $code = $p.ExitCode }
    } finally { $files = $cap.Stop() }
    foreach ($k in @($files.Keys)) { if (-not $caught.ContainsKey($k)) { $caught[$k] = $files[$k] } }
    if ($null -ne (Find-LaunchSet @($caught.Keys))) { break }
    # Missed: stop the holder and drop its record, so that the next pass starts one again.
    Stop-RunProcesses
    $recordPath = Join-Path $stateFull 'board-holder.json'
    Assert-ConformancePath -ConformanceHome $caseHome -Path $recordPath | Out-Null
    Remove-Item -LiteralPath $recordPath -ErrorAction SilentlyContinue
  }

  if ($precondition) {
    Add-Check ($code -eq 0) 'The keeper started with a relative AGENTSPACE_STATE_DIR exits 0' "The exit code was $code."
    $rec = Read-Record
    Add-Check ([bool]$rec) 'The keeper used the state folder the relative name resolves to: it found the lease there and recorded a holder' 'No board-holder.json was written in the state folder the relative name names.'

    $reqName = @($caught.Keys | Where-Object { $_ -cmatch '\Aboard-holder-launch\.[0-9a-f]{32}\.json\z' }) | Select-Object -First 1
    Add-Check ([bool]$reqName) 'A launch request board-holder-launch.<id>.json was written while the holder was started' 'No launch request was seen in 4 passes.'
    if ($reqName) {
      $req = $null
      try { $req = [Text.UTF8Encoding]::new($false).GetString($caught[$reqName]) | ConvertFrom-Json -AsHashtable -DateKind String } catch { }
      $paths = @('log', 'directory', 'result')
      $notFull = @($paths | Where-Object { -not $req -or -not ($req[$_] -is [string]) -or -not [IO.Path]::IsPathRooted($req[$_]) -or $req[$_] -notmatch '\A[A-Za-z]:\\' })
      Add-Check ([bool]$req -and $notFull.Count -eq 0) 'The launch request''s log, directory and result are full paths, never relative' "The request was unreadable, or these fields were not full paths: $($notFull -join ', ')."
      $expectLog = Join-Path $stateFull 'board-holder-ssh.log'
      $expectResult = Join-Path $stateFull ($reqName -replace '\.json$', '.result.json')
      $same = [bool]$req -and
        [string]::Equals([string]$req['log'], $expectLog, [StringComparison]::OrdinalIgnoreCase) -and
        [string]::Equals([string]$req['result'], $expectResult, [StringComparison]::OrdinalIgnoreCase) -and
        [string]::Equals([string]$req['directory'], $env:USERPROFILE, [StringComparison]::OrdinalIgnoreCase)
      Add-Check $same 'The log and the result are in the state folder the relative name resolves to against the keeper''s working folder, and the directory is the home folder' 'A path of the request is not the full path it resolves to.'
    }
  }

  # Stop only the holder this run caused, by its recorded PID and only when its command line is this run's stub.
  $rec2 = Read-Record
  if ($rec2) {
    $hp = [int]$rec2['pid']
    if (Test-HolderLine (Get-CommandLine $hp)) { Stop-OwnTree $hp }
  }
} finally {
  Restore-CaseEnvironment
  Stop-RunProcesses
  $leftBehind = @(Get-RunProcesses).Count
  # The delete, checked at the call: the run's folder, directly under the temp folder taken at the start.
  if ((Test-Path -LiteralPath $temp) -and (Split-Path -Leaf $temp) -cmatch '\Aagentspace-conformance-keeper-[0-9a-f]{32}\z') {
    Assert-ConformancePath -ConformanceHome ([pscustomobject]@{ Root = $tempRoot }) -Path $temp | Out-Null
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
  }
}

if ($leftBehind -eq 0) { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status PASS }
else { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status FAIL -Reason "$leftBehind process(es) of this run were still running after the cleanup." }
Exit-ConformanceRun

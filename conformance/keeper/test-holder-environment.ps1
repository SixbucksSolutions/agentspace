#Requires -Version 7
# A conformance check under D34: it drives the keeper only as a separate process, through what docs/keeper.md documents, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# CHECKS keeper.md, "Streams and environment" (D35, B-41): the holder runs with the user's logon
# environment, never with the keeper's. A variable set only in the keeper's environment MUST NOT
# reach the holder, and AGENTSPACE_STATE_DIR is named there as one such variable.
#
# One case, against the program that AGENTSPACE_KEEPER_CMD names (or its default, through the harness),
# with the keeper's own arguments unchanged: <program> -SshExe <stub> -Now <time>.
# - The stub holder is a .cmd of this check's own, in the run's folder. It writes the NAMES of its
#   environment variables, never a value, to a file beside itself, and a yes or no marker for whether
#   its %USERPROFILE% is the case's home (never the value), then a last line that marks the names file
#   complete, then stays alive until it is stopped. It connects nowhere. `for /f` over `set` writes
#   the text before the first `=` of each line of `set`'s output, so a value that held a line break
#   could leak a fragment of itself into the names file as a "name" (it is never printed, and it is
#   deleted with the run's folder); the check reads the file as names only and drops every line that
#   is not a variable name (`^[A-Za-z_][A-Za-z0-9_()]*$`), counting the drops.
# - The keeper runs one pass for a ready lease, with a sentinel variable (a name made at run time,
#   `T04_KEEPER_SENTINEL_<random hex>`, with no AGENTSPACE_ prefix, and a random value that is never
#   printed or written) set in the keeper's environment only, as AGENTSPACE_STATE_DIR is. The
#   sentinel is set before the keeper starts, so the keeper had it; this check cannot see it from
#   inside the keeper, because the only program that records an environment is the stub holder, and
#   it MUST NOT receive the keeper's. What is asserted is that the sentinel's NAME is absent.
# - The controls: the file is real when it holds a name that every logon environment has (SystemRoot),
#   and the keeper had AGENTSPACE_STATE_DIR (it could not find the lease without it), which is
#   absent from the file too. That the sentinel was set at the keeper's start is a precondition, not
#   a result: it prints a FAIL line when it does not hold, and no PASS when it does.
# - The keeper's USERPROFILE is the case's home, which is not the logon environment's: the holder MUST
#   see the logon environment's, so the stub's marker MUST say its %USERPROFILE% is NOT the case's
#   home (keeper.md, "Streams and environment").
# Every value stays unseen: the check asserts on names and reports names or counts.
#
# What it touches: the same as test-keeper.ps1 (conformance/keeper/keeper-common.ps1): a throwaway
# folder under $env:TEMP, the harness's child environment for it on every process, and only processes
# this run started, each stopped by a checked PID. The real %USERPROFILE%\.agentspace, ~/.ssh, ssh.exe,
# keeper task and box are never read, written, listed or hashed, and the logon environment's values are
# never read. The holder runs in the logon environment, so its stub touches only the run's folder.
#
# Output: PASS <check name>, FAIL <check name>: <reason>, then one summary line; exit 0 when every
# check passed, 1 when any failed, 2 when none failed but any was skipped.
# Run:  pwsh -NoProfile -File conformance\keeper\test-holder-environment.ps1
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

# A name made at run time, with no AGENTSPACE_ prefix: the assertion is that this name is absent.
$sentinel = "T04_KEEPER_SENTINEL_$([guid]::NewGuid().ToString('N').Substring(0, 16))"
$endMark = '--names-complete--'
$nameForm = '\A[A-Za-z_][A-Za-z0-9_()]*\z'
$prefix = 'keeper.md#streams-and-environment'

function Add-Check([bool]$Ok, [string]$Text, [string]$Why = '') {
  $name = "${prefix}: $Text"
  if ($Ok) { Write-ConformanceResult -Name $name -Status PASS } else { Write-ConformanceResult -Name $name -Status FAIL -Reason $Why }
}

function Test-HasName($Names, [string]$Name) { [bool]($Names | Where-Object { $_ -ieq $Name }) }

try {
  $caseHome = New-CaseHome
  # The stub holder writes names only. %~dp0 is the run's folder, so no variable has to reach it.
  $holderStub = Join-Path $temp 'ssh-env-stub.cmd'
  $namesFile = Join-Path $temp 'holder-env-names.txt'
  $profileFile = Join-Path $temp 'holder-env-userprofile.txt'
  # The case's home, written into the stub (a batch percent is doubled), so that the stub compares
  # its %USERPROFILE% with it and writes only yes or no, never the value.
  $homeInStub = $caseHome.UserProfile.Replace('%', '%%')
  $stubLines = @('@echo off', 'rem A stub for ssh (a program outside agentspace): it records the names of its environment, never the values.',
    'for /f "delims==" %%A in (''set'') do @echo %%A>>"%~dp0holder-env-names.txt"',
    "if /i `"%USERPROFILE%`"==`"$homeInStub`" (echo yes>>`"%~dp0holder-env-userprofile.txt`") else (echo no>>`"%~dp0holder-env-userprofile.txt`")",
    "echo $endMark>>`"%~dp0holder-env-names.txt`"", 'echo stub-started %*', 'ping -n 1200 127.0.0.1 >nul')
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $caseHome -Path $holderStub), (($stubLines -join "`r`n") + "`r`n"))
  $stub = $holderStub

  # Write-Leases checks its own path at the call, inside the case's home (keeper-common.ps1).
  Write-Leases @((New-ReadyRecord 'a' '2099-01-01T00:00:00Z'))
  # Set for the keeper only: every process the case starts inherits it, and the holder MUST NOT.
  if (-not $script:savedEnvironment.ContainsKey($sentinel)) { $script:savedEnvironment[$sentinel] = [Environment]::GetEnvironmentVariable($sentinel) }
  Set-ProcessVariable $sentinel ([guid]::NewGuid().ToString('N'))
  $keeperHadSentinel = -not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($sentinel))
  $keeperHadStateDir = -not [string]::IsNullOrEmpty($env:AGENTSPACE_STATE_DIR)

  $p = Start-KeeperProcess @('-SshExe', $stub, '-Now', $now) 'keeper'
  if (-not $p.WaitForExit(180000)) { Stop-OwnTree $p.Id }

  $log = Read-LogLines
  $rec = Read-Record
  $startedOk = $rec -and (@($log | Where-Object { Test-LogLine $_ 'started' (Get-TestId 'a') (Get-TestAlias 'a') ([int]$rec['pid']) }).Count -ge 1)
  Add-Check ([bool]$startedOk) 'The keeper starts a holder for the ready lease, logged and recorded' "The log ends: $(($log | Select-Object -Last 3) -join ' | ')"

  # Complete when the last line is there; no fixed sleep.
  $complete = Wait-Until { (Test-Path -LiteralPath $namesFile) -and ((Get-Content -LiteralPath $namesFile -ErrorAction SilentlyContinue) -contains $endMark) } 60
  # Read as names only: a line that is not a variable name (a fragment of a value with a line break in
  # it, or a hidden "=C:" entry) is dropped and counted, never shown.
  $lines = if ($complete) { @(Get-Content -LiteralPath $namesFile | Where-Object { $_ -ne $endMark }) } else { @() }
  $names = @($lines | Where-Object { $_ -cmatch $nameForm })
  $profileMarks = @(if ($complete -and (Test-Path -LiteralPath $profileFile)) { Get-Content -LiteralPath $profileFile })

  # A precondition, not a result: it prints a FAIL line when the sentinel was not set at the keeper's
  # start (every absence below would then mean nothing), and nothing when it was.
  if (-not $keeperHadSentinel) { Add-Check $false 'Precondition: the sentinel was set in the keeper''s environment at its start' 'The check could not set the sentinel.' }
  Add-Check ($complete -and (Test-HasName $names 'SystemRoot')) 'Control: the holder wrote its environment, and it holds a name every logon environment has (SystemRoot)' "The names file was $(if ($complete) { "complete, with $($names.Count) names, none of them SystemRoot" } else { 'missing or incomplete after 60 seconds' })."
  Add-Check ($keeperHadSentinel -and $complete -and -not (Test-HasName $names $sentinel)) 'A variable set only in the keeper''s environment does not reach the holder' "The names file was incomplete, or held the sentinel's name, or the sentinel was not set; $($names.Count) names."
  Add-Check ($complete -and $profileMarks.Count -eq 1 -and $profileMarks[0] -ceq 'no') 'The keeper''s changed USERPROFILE does not reach the holder: the holder''s is not the case''s home' "The marker file was missing or incomplete, or the holder's USERPROFILE was the case's home ($($profileMarks.Count) marker lines, from the check's own stub, which writes only yes or no: $(($profileMarks | ForEach-Object { "'$_'" }) -join ', '))."
  Add-Check ($complete -and $keeperHadStateDir -and -not (Test-HasName $names 'AGENTSPACE_STATE_DIR')) 'AGENTSPACE_STATE_DIR, set for the keeper, does not reach the holder' "The holder's environment held AGENTSPACE_STATE_DIR, or the file was incomplete, or the keeper had none; $($names.Count) names."
  $agentspaceNames = @($names | Where-Object { $_ -like 'AGENTSPACE_*' })
  Add-Check ($complete -and $agentspaceNames.Count -eq 0) 'No AGENTSPACE_ variable reaches the holder' "The holder's environment held $($agentspaceNames.Count) AGENTSPACE_ names."

  # Stop only the holder this run caused, by its recorded PID and only when its command line is this run's stub.
  if ($rec) {
    $hp = [int]$rec['pid']
    if (Test-HolderLine (Get-CommandLine $hp)) { Stop-OwnTree $hp }
  }
} finally {
  Restore-CaseEnvironment
  Stop-RunProcesses
  $leftBehind = @(Get-RunProcesses).Count
  # The delete, checked at the call: the run's folder, directly under the temp folder, as a home's root.
  if ((Test-Path -LiteralPath $temp) -and (Split-Path -Leaf $temp) -cmatch '\Aagentspace-conformance-keeper-[0-9a-f]{32}\z') {
    Remove-Item -LiteralPath (Assert-ConformanceHomeRoot -Root $temp) -Recurse -Force -ErrorAction SilentlyContinue
  }
}

if ($leftBehind -eq 0) { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status PASS }
else { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status FAIL -Reason "$leftBehind process(es) of this run were still running after the cleanup." }
Exit-ConformanceRun

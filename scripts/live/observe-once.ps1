# The live kit's observer (docs/architect.md, "Live checks on Terry's laptop and the box"). Process
# tool, not product (D28). Rebuilt by the architect before v0.12-T05 from what
# docs/runs/e0.09-t05-s01.md records of v0.09-T05's observer, which was lost with its worktree.
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# Usage, once a minute from a bounded foreground loop of the caller's own:
#   pwsh -NoProfile -File scripts\live\observe-once.ps1 -Out <folder under target\>
#   pwsh -NoProfile -File scripts\live\observe-once.ps1 -Out <folder> -Operator <stand-in .ps1>
#
# Each call makes ONE read: `status -All` through scripts\live\wrap.ps1, named 401-status-<nnnn>, the
# counter one more than the reads status.log holds. It then reads the wrapper's raw .out (never
# printing it), takes each host's `Host`, `State` and `Type` lines (operator.md, status -All), and
# writes these files in -Out, which a reader MAY print, since they hold only host names, states,
# types, times and counts:
#   status.log    One line a read: `<utc> <local> PASS|FAIL exit=<n> <host>=<state>/<type> ...`.
#                 PASS is exit 0 with at least one Host line parsed; anything else is FAIL.
#   edges.log     One line each time a host's state differs from its last read:
#                 `<utc> <local> <host> <from> -> <to> type=<type>`. A host's first read is an edge
#                 from `unseen`.
#   hours.json    The instance-hours intervals: an interval opens at a host's first read whose state
#                 is pending, running, stopping or shutting-down, and closes at its first read whose
#                 state is stopped or terminated, or that has no State line. So an interval can
#                 trail the start by up to one read's spacing and counts the stop's last minute as
#                 running: a slight overcount, as v0.09-T05's record explains. An open interval is
#                 summed up to the current read.
#   observer-state.json  Each host's last state and type; the observer's own memory between calls.
# It prints one line: `read <nnnn> at <local>: <host> <state> ...; hours <h.hhh>`.
#
# Exit codes: 0, a read was made; 1, the read failed (its line in status.log is FAIL); 3, "done": a
# file named `stop` exists in -Out and this read shows every host stopped or terminated or with no
# instance. A caller's loop ends on 3, and on any read that is not 0 it stops and reports, since a
# failure is a finding.
#
# It MUST NOT be given a raw file to print, and it MUST NOT read, list, copy or hash the program it
# runs (AGENTS.md, "Terry's real files in a review").
param(
  [Parameter(Mandatory)][string]$Out,
  [string]$Operator
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$outDir = (Resolve-Path -LiteralPath $Out).Path
$statusLog = Join-Path $outDir 'status.log'
$edgesLog = Join-Path $outDir 'edges.log'
$hoursFile = Join-Path $outDir 'hours.json'
$stateFile = Join-Path $outDir 'observer-state.json'
$readsDir = Join-Path $outDir 'reads'

$count = if (Test-Path -LiteralPath $statusLog) { @(Get-Content -LiteralPath $statusLog).Count } else { 0 }
$name = '401-status-{0:D4}' -f ($count + 1)

if ($Operator) {
  $command = "& '" + ((Resolve-Path -LiteralPath $Operator).Path -replace "'", "''") + "' status -All"
}
else {
  $program = Join-Path $env:USERPROFILE '.agentspace\bin\agentspace-operator.exe'
  $command = "& '" + ($program -replace "'", "''") + "' status -All"
}

& pwsh -NoProfile -NonInteractive -File (Join-Path $PSScriptRoot 'wrap.ps1') -Out $readsDir -Name $name -Command $command | Out-Null
$base = Join-Path $readsDir $name
$exitText = if (Test-Path -LiteralPath ($base + '.exit')) { Get-Content -LiteralPath ($base + '.exit') } else { @('exit=1') }
$code = [int](($exitText | Where-Object { $_ -like 'exit=*' } | Select-Object -First 1) -replace '^exit=', '')
$startText = Get-Content -LiteralPath ($base + '.start')
$utc = (($startText | Where-Object { $_ -like 'start_utc=*' }) -replace '^start_utc=', '')
$local = (($startText | Where-Object { $_ -like 'start_local=*' }) -replace '^start_local=', '')

# Parse the raw stdout into hosts, in the order printed (the file's order).
$hosts = [System.Collections.Generic.List[object]]::new()
$current = $null
$raw = if (Test-Path -LiteralPath ($base + '.out')) { Get-Content -LiteralPath ($base + '.out') } else { @() }
foreach ($line in $raw) {
  if ($line -match '^Host {6}(\S+) \(') {
    $current = [ordered]@{ host = $Matches[1]; state = 'none'; type = '-' }
    $hosts.Add($current)
  }
  elseif ($null -ne $current -and $line -match '^State {5}(\S+)') { $current.state = $Matches[1] }
  elseif ($null -ne $current -and $line -match '^Type {6}(\S+)') { $current.type = $Matches[1] }
}

$verdict = if ($code -eq 0 -and $hosts.Count -gt 0) { 'PASS' } else { 'FAIL' }
$hostText = ($hosts | ForEach-Object { '{0}={1}/{2}' -f $_.host, $_.state, $_.type }) -join ' '
Add-Content -LiteralPath $statusLog -Value ("{0} {1} {2} exit={3} {4}" -f $utc, $local, $verdict, $code, $hostText).TrimEnd()

$memory = if (Test-Path -LiteralPath $stateFile) { Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json -AsHashtable -DateKind String } else { @{} }
$hours = if (Test-Path -LiteralPath $hoursFile) { @(Get-Content -LiteralPath $hoursFile -Raw | ConvertFrom-Json -AsHashtable -DateKind String) } else { @() }
$intervals = [System.Collections.Generic.List[object]]::new()
foreach ($i in $hours) { if ($null -ne $i) { $intervals.Add($i) } }
$on = @('pending', 'running', 'stopping', 'shutting-down')

if ($verdict -eq 'PASS') {
  foreach ($h in $hosts) {
    $last = if ($memory.ContainsKey($h.host)) { $memory[$h.host].state } else { 'unseen' }
    if ($last -ne $h.state) {
      Add-Content -LiteralPath $edgesLog -Value ("{0} {1} {2} {3} -> {4} type={5}" -f $utc, $local, $h.host, $last, $h.state, $h.type)
    }
    $open = $intervals | Where-Object { $_.host -eq $h.host -and -not $_.end_utc } | Select-Object -First 1
    if ($on -contains $h.state) {
      if (-not $open) { $intervals.Add([ordered]@{ host = $h.host; start_utc = $utc; start_local = $local; end_utc = $null; end_local = $null }) }
    }
    elseif ($open) {
      $open.end_utc = $utc
      $open.end_local = $local
    }
    $memory[$h.host] = @{ state = $h.state; type = $h.type }
  }
  Set-Content -LiteralPath $stateFile -Value ($memory | ConvertTo-Json -Depth 4)
  Set-Content -LiteralPath $hoursFile -Value (ConvertTo-Json -InputObject @($intervals) -Depth 4)
}

$now = [DateTime]::Parse($utc, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal)
$total = 0.0
foreach ($i in $intervals) {
  $s = [DateTime]::Parse($i.start_utc, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal)
  $e = if ($i.end_utc) { [DateTime]::Parse($i.end_utc, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal) } else { $now }
  $total += ($e - $s).TotalHours
}

. (Join-Path $PSScriptRoot 'mask.ps1')
$summary = ($hosts | ForEach-Object { '{0} {1}' -f $_.host, $_.state }) -join ', '
Write-Output (ConvertTo-LiveMasked -Text ("read {0} at {1}: {2}; hours {3:N3}{4}" -f ($count + 1), $local, $summary, $total, $(if ($verdict -eq 'FAIL') { '; FAIL' } else { '' })))

if ($verdict -eq 'FAIL') { exit 1 }
$allDown = @($hosts | Where-Object { $on -contains $_.state }).Count -eq 0
if ((Test-Path -LiteralPath (Join-Path $outDir 'stop')) -and $allDown) { exit 3 }
exit 0

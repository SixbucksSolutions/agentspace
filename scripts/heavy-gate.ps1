#Requires -Version 7
# RFC 2119 keywords; the capitals are load-bearing.
#
# The laptop's heavy-run gate (architect.md, "Checking that it still works": at most three suite or
# workspace runs at once; v0.10-T02 and v0.11-T01, where each dev lead wrote its own). Every heavy
# run on the laptop SHOULD go through it, the architect's merge checks included.
#
#   pwsh -NoProfile -File scripts\heavy-gate.ps1 -Out <folder> -Name <nnn-what> -Command '<PowerShell>' [-Slots 1|2]
#   pwsh -NoProfile -File scripts\heavy-gate.ps1 -HoldMinutes <n> [-Slots 1|2]
#
# - Three slots, each a named mutex, Local\agentspace-heavy-1 to -3. A run takes -Slots of them
#   (1 by default; 2 for a load window) and holds them until it ends.
# - Fair: Local\agentspace-heavy-queue is held only while a run takes its slots, so waiters take
#   slots in turn, and a run that needs two is not starved by runs that need one (v0.10-T02).
# - The run itself goes through scripts\live\wrap.ps1, so its error mode carries 0x8001 and its
#   exit code and times are kept in <Out>\<Name>.exit (handoff-v0.10.md, rules 11 and 12).
# - -HoldMinutes holds the slots for that long, or until this process is stopped, and runs
#   nothing: for a run made elsewhere that MUST be counted, such as a check in another checkout.
# - A slot whose holder died is taken over: Windows reports it abandoned, and it counts as taken.
# - Prints one line when the slots are taken and one when they are freed, each with the local
#   time, and exits with the run's exit code, or 0 after a hold.
param(
  [string]$Out,
  [string]$Name,
  [string]$Command,
  [ValidateRange(1, 2)][int]$Slots = 1,
  [ValidateRange(1, 240)][int]$HoldMinutes = 0
)
$ErrorActionPreference = 'Stop'
if ($HoldMinutes -eq 0 -and (-not $Out -or -not $Name -or -not $Command)) {
  throw 'Give -Out, -Name and -Command for a run, or -HoldMinutes for a hold.'
}

function Get-GateTime { (Get-Date).ToString('h:mm:sstt').ToLower() }

function Enter-GateMutex([System.Threading.Mutex]$Mutex, [int]$TimeoutMs) {
  try { return $Mutex.WaitOne($TimeoutMs) }
  catch [System.Threading.AbandonedMutexException] { return $true }
}

$queue = [System.Threading.Mutex]::new($false, 'Local\agentspace-heavy-queue')
$slotMutexes = 1..3 | ForEach-Object { [System.Threading.Mutex]::new($false, "Local\agentspace-heavy-$_") }
$held = [System.Collections.Generic.List[System.Threading.Mutex]]::new()
$exitCode = 0
try {
  [void](Enter-GateMutex $queue -1)
  try {
    while ($held.Count -lt $Slots) {
      foreach ($m in $slotMutexes) {
        if ($held.Contains($m)) { continue }
        if (Enter-GateMutex $m 0) { $held.Add($m); if ($held.Count -eq $Slots) { break } }
      }
      if ($held.Count -lt $Slots) { Start-Sleep -Milliseconds 500 }
    }
  }
  finally { $queue.ReleaseMutex() }

  $label = if ($HoldMinutes -gt 0) { "hold of $HoldMinutes minutes" } else { $Name }
  "heavy-gate: $Slots slot(s) taken for $label at $(Get-GateTime)."
  if ($HoldMinutes -gt 0) {
    Start-Sleep -Seconds ($HoldMinutes * 60)
  }
  else {
    $wrap = Join-Path $PSScriptRoot 'live\wrap.ps1'
    & pwsh -NoProfile -File $wrap -Out $Out -Name $Name -Command $Command
    $exitCode = $LASTEXITCODE
  }
}
finally {
  foreach ($m in $held) { $m.ReleaseMutex() }
  "heavy-gate: $($held.Count) slot(s) freed at $(Get-GateTime)."
  foreach ($m in $slotMutexes) { $m.Dispose() }
  $queue.Dispose()
}
exit $exitCode

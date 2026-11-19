#Requires -Version 7
# The architect's liveness watch on a running agentspace-sample and on the box's root disk
# (AGENTS.md, "Watching a measurement"; D41). Run it under the Monitor tool: every line it prints
# is a notification. Not product (D28).
#
# RFC 2119 keywords, and the capitals are load-bearing.
#
# Every -IntervalSeconds it runs `agentspace-sample status` and `df` over the lease's alias, and
# prints a line only when something needs the architect:
#   - the first look, once, so a watch that cannot see the sampler is known at once;
#   - the sampler is not running, or its samples did not rise since the last look;
#   - the root filesystem is at or above -DiskPercent used;
#   - the look itself failed (ssh refused, timed out, or printed nothing it understood).
# Silence between those lines means each look found the samples rising and the disk under the
# limit. It never stops or starts a sampler.
param(
  [Parameter(Mandatory)] [string]$Alias,
  [int]$IntervalSeconds = 60,
  [int]$DiskPercent = 80
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ssh = Join-Path $env:SystemRoot 'System32\OpenSSH\ssh.exe'
$last = -1
$first = $true
while ($true) {
  $stamp = (Get-Date).ToString('h:mmtt').ToLower()
  $out = & $ssh -o BatchMode=yes -o ConnectTimeout=20 $Alias 'agentspace-sample status; echo "disk $(df --output=pcent / | tail -1 | tr -dc 0-9)"' 2>$null | Out-String
  $code = $LASTEXITCODE
  $samples = if ($out -match '(?im)samples\D{0,20}(\d+)') { [int]$Matches[1] } else { -1 }
  $disk = if ($out -match '(?m)^disk (\d+)') { [int]$Matches[1] } else { -1 }
  $notRunning = $out -match 'No sampler is running'
  $problems = @()
  if ($code -ne 0 -or $disk -lt 0) { $problems += "the look failed (ssh exit $code)" }
  elseif ($notRunning) { $problems += 'the sampler is not running' }
  elseif ($samples -lt 0) { $problems += 'the status named no sample count' }
  elseif (-not $first -and $samples -le $last) { $problems += "samples did not rise ($last to $samples)" }
  if ($disk -ge $DiskPercent) { $problems += "the root filesystem is $disk% used" }
  if ($first) {
    Write-Output "$stamp Watching ${Alias}: samples $samples, disk $disk% used$(if ($problems) { '; ' + ($problems -join '; ') })."
    $first = $false
  } elseif ($problems) {
    Write-Output "$stamp ${Alias}: $($problems -join '; ') (samples $samples, disk $disk%)."
  }
  if ($samples -ge 0) { $last = $samples }
  Start-Sleep -Seconds $IntervalSeconds
}

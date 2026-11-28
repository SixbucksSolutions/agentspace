# The proof of scripts\live\observe-once.ps1 (docs/architect.md, "Live checks on Terry's laptop and the
# box": a change to the live kit keeps its proof passing). Process tool, not product (D28).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# Usage: pwsh -NoProfile -File scripts\live\observe-proof.ps1 -Out <folder under target\> [-Plant]
#
# It runs the observer against a stand-in operator that answers a fixed sequence of `status -All`
# reports, and checks status.log, edges.log, hours.json and the exit codes against what the sequence
# MUST give. No AWS call and no real program is run. With -Plant, the stand-in's third report names
# the host `running` where the sequence wants `stopped`, so the proof MUST fail: run it both ways
# before believing a pass. It prints `PASS <check>` or `FAIL <check>: <why>` lines and a count, and
# exits 0 only when every check passed.
param(
  [Parameter(Mandatory)][string]$Out,
  [switch]$Plant
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (Test-Path -LiteralPath $Out) { throw "The proof's folder already exists; give a new one: $(Split-Path -Leaf $Out)" }
New-Item -ItemType Directory -Path $Out | Out-Null
$dir = (Resolve-Path -LiteralPath $Out).Path
$obsDir = Join-Path $dir 'obs'
$seqFile = Join-Path $dir 'sequence.txt'

# The reports, one a read. Read 3 is the one -Plant changes.
$third = if ($Plant) { 'running' } else { 'stopped' }
$reports = @(
  @('stopped', 'stopped'),
  @('running', 'stopped'),
  @($third, 'running'),
  @('stopped', 'stopped')
)
$standIn = Join-Path $dir 'planted-operator.ps1'
Set-Content -LiteralPath $standIn -Value @'
$seq = Join-Path $PSScriptRoot 'sequence.txt'
$n = if (Test-Path -LiteralPath $seq) { [int](Get-Content -LiteralPath $seq) } else { 0 }
Set-Content -LiteralPath $seq -Value ($n + 1)
$reports = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'reports.json') -Raw | ConvertFrom-Json
$r = $reports[$n]
"Host      agentspace.linux-amd64 (agentspace-linux-amd64-host-agentspace), ready"
"Platform  linux/amd64"
"Instance  i-0123456789abcdef0"
"State     $($r[0])"
"Type      m8a.xlarge"
"Leases    0 live"
"Host      agentspace.windows-amd64 (agentspace-windows-amd64-host-agentspace), ready"
"Platform  windows/amd64"
"Instance  i-0fedcba9876543210"
"State     $($r[1])"
"Type      m8a.xlarge"
"Leases    0 live"
exit 0
'@
Set-Content -LiteralPath (Join-Path $dir 'reports.json') -Value (ConvertTo-Json -InputObject $reports -Depth 3)

$pass = 0; $fail = 0
function Write-ProofCheck([bool]$Ok, [string]$Check, [string]$Why) {
  if ($Ok) { Write-Output "PASS $Check"; $script:pass++ } else { Write-Output "FAIL ${Check}: $Why"; $script:fail++ }
}

$codes = @()
for ($i = 1; $i -le 4; $i++) {
  if ($i -eq 4) { New-Item -ItemType File -Path (Join-Path $obsDir 'stop') -Force | Out-Null }
  & pwsh -NoProfile -NonInteractive -File (Join-Path $PSScriptRoot 'observe-once.ps1') -Out $obsDir -Operator $standIn | Out-Null
  $codes += $LASTEXITCODE
  if ($i -lt 4) { Start-Sleep -Seconds 1 }
}

Write-ProofCheck (($codes -join ',') -eq '0,0,0,3') 'The exit codes are 0, 0, 0, then 3 once the stop file exists and every host is stopped' "They were $($codes -join ',')."
$status = @(Get-Content -LiteralPath (Join-Path $obsDir 'status.log'))
Write-ProofCheck ($status.Count -eq 4 -and @($status | Where-Object { $_ -match ' PASS exit=0 ' }).Count -eq 4) 'status.log holds 4 lines, each PASS exit=0' "It holds $($status.Count) lines."
$edges = @(Get-Content -LiteralPath (Join-Path $obsDir 'edges.log'))
$wantEdges = @(
  'agentspace.linux-amd64 unseen -> stopped', 'agentspace.windows-amd64 unseen -> stopped',
  'agentspace.linux-amd64 stopped -> running',
  'agentspace.linux-amd64 running -> stopped', 'agentspace.windows-amd64 stopped -> running',
  'agentspace.windows-amd64 running -> stopped'
)
$gotEdges = @($edges | ForEach-Object { ($_ -split ' ', 3)[2] -replace ' type=\S+$', '' })
Write-ProofCheck (($gotEdges -join '|') -eq ($wantEdges -join '|')) 'edges.log holds the six edges, in order' "It holds: $($gotEdges -join ' | ')"
$hours = @(Get-Content -LiteralPath (Join-Path $obsDir 'hours.json') -Raw | ConvertFrom-Json -DateKind String)
$closed = @($hours | Where-Object { $_.end_utc }).Count
Write-ProofCheck ($hours.Count -eq 2 -and $closed -eq 2) 'hours.json holds two intervals, one a host, both closed' "It holds $($hours.Count), $closed closed."
$planted = Get-Content -LiteralPath (Join-Path $obsDir 'reads\401-status-0001.masked.txt') -Raw
Write-ProofCheck ($planted -notmatch 'i-0123456789abcdef0') 'The read''s masked file holds no instance ID' 'The planted instance ID is in the masked file.'

Write-Output "$pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
exit 0

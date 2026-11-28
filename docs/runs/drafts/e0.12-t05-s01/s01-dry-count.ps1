# Counts the drafter's wrappers of the v0.12-T05 dry runs from their .exit files and expected-exits.txt (written by the
# cases). Drafter-owned. RFC 2119 keywords; the capitals are load-bearing. Reads .exit and .start files only, never a raw
# .out. Run through scripts\live\wrap.ps1. Exit 0 when every case's exit is the one it was designed to have.
param([string]$Dir = (Join-Path ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))) 'target\runs\drafter'))
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$want = @{}
$wf = Join-Path $Dir 'expected-exits.txt'
if (Test-Path -LiteralPath $wf) { foreach ($l in Get-Content -LiteralPath $wf) { if ($l -match '^(\S+) (-?\d+)$') { $want[$Matches[1]] = [int]$Matches[2] } } }
$exits = @{}
foreach ($f in Get-ChildItem -LiteralPath $Dir -Filter '*.exit' -Name) {
  $name = $f -replace '\.exit$', ''
  $line = Get-Content -LiteralPath (Join-Path $Dir $f) | Where-Object { $_ -like 'exit=*' } | Select-Object -First 1
  $exits[$name] = [int]($line -replace '^exit=', '')
}
$modes = @{}
foreach ($f in Get-ChildItem -LiteralPath $Dir -Filter '*.start' -Name) {
  $m = (Get-Content -LiteralPath (Join-Path $Dir $f) | Where-Object { $_ -like 'error_mode=*' } | Select-Object -First 1)
  if (-not $modes.ContainsKey($m)) { $modes[$m] = 0 }
  $modes[$m]++
}
$clean = @($want.Keys | Where-Object { $_ -match '-clean-' })
$planted = @($want.Keys | Where-Object { $_ -match '-planted-' })
$controls = @($want.Keys | Where-Object { $_ -match '-control-' })
$real = @($want.Keys | Where-Object { $_ -match '-real-' })
$mismatch = @($want.Keys | Where-Object { -not $exits.ContainsKey($_) -or $exits[$_] -ne $want[$_] })
"Wrappers with an .exit file: $($exits.Count); cases with a designed exit: $($want.Count)"
"Clean steps: $($clean.Count), exit 0: $(@($clean | Where-Object { $exits.ContainsKey($_) -and $exits[$_] -eq 0 }).Count)"
"Real-build steps: $($real.Count), as designed: $(@($real | Where-Object { $exits.ContainsKey($_) -and $exits[$_] -eq $want[$_] }).Count)"
"Controls: $($controls.Count), exit 0: $(@($controls | Where-Object { $exits.ContainsKey($_) -and $exits[$_] -eq 0 }).Count)"
"Planted: $($planted.Count), exit nonzero: $(@($planted | Where-Object { $exits.ContainsKey($_) -and $exits[$_] -ne 0 }).Count)"
$byExit = @{}
foreach ($n in $planted) { if ($exits.ContainsKey($n)) { $k = [string]$exits[$n]; if (-not $byExit.ContainsKey($k)) { $byExit[$k] = 0 }; $byExit[$k]++ } }
"Planted by exit code: " + ((@($byExit.Keys | Sort-Object) | ForEach-Object { "exit $_ x $($byExit[$_])" }) -join ', ')
"Error modes read in .start files: " + ((@($modes.Keys | Sort-Object) | ForEach-Object { "$_ x $($modes[$_])" }) -join ', ')
"Cases whose exit is not the designed one: $($mismatch.Count)"
foreach ($n in $mismatch) { "  $n : designed $($want[$n]), got $(if ($exits.ContainsKey($n)) { $exits[$n] } else { 'no .exit file' })" }
if ($mismatch.Count -eq 0) { exit 0 }
exit 1

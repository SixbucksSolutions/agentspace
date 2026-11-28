param([switch]$Planted)
$ErrorActionPreference = 'Continue'
$bin = Join-Path $env:USERPROFILE '.agentspace\bin'
$dist = Join-Path (Get-Location).Path 'target\dist'
function Get-HelpText([string]$exe, [string[]]$a) { (& $exe @a 2>&1 | Out-String) }
$ok = $true
$ti = @{}; $td = @{}
foreach ($n in 'agentspace', 'agentspace-operator') {
  $ti[$n] = Get-HelpText (Join-Path $bin "$n.exe") @('--help')
  $td[$n] = Get-HelpText (Join-Path $dist "$n.exe") @('--help')
}
if ($Planted) {
  $swap = $td['agentspace']; $td['agentspace'] = $td['agentspace-operator']; $td['agentspace-operator'] = $swap
  'planted: the dist help texts are swapped'
}
foreach ($n in 'agentspace', 'agentspace-operator') {
  $eq = $ti[$n] -ceq $td[$n]
  "$n --help equal: $eq (installed length $($ti[$n].Length), dist length $($td[$n].Length))"
  if (-not $eq) { $ok = $false }
}
$cr = Get-HelpText (Join-Path $bin 'agentspace.exe') @('lease', 'create', '--help')
if ($Planted) { $cr = $cr.Replace('--platform', '--platXrm').Replace('windows/amd64', 'windows/amdXX'); 'planted: --platform and windows/amd64 removed from the lease create help' }
"installed lease create --help length: $($cr.Length)"
foreach ($flag in '--board', '--platform', 'windows/amd64') {
  $has = $cr.Contains($flag)
  "installed lease create --help names ${flag}: $has"
  if (-not $has) { $ok = $false }
}
if (-not $ok) { exit 1 }

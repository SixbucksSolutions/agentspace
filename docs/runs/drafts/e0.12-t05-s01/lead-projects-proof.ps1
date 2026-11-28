# Proves lead-projects-check.ps1 on throwaway copies: the good file MUST exit 0, each planted file MUST exit 5.
$ErrorActionPreference = 'Stop'
$dir = Join-Path $PSScriptRoot 'planted-projects'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$good = '{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20","platforms":[{"platform":"linux/arm64","approved":"2026-11-20"},{"platform":"linux/amd64","approved":"2026-11-26"},{"platform":"windows/amd64","approved":"2026-11-26"}]}]}'
$cases = [ordered]@{
  'good'                = @{ text = "$good`n"; want = 0 }
  'planted-noplatforms' = @{ text = "{`"version`":0,`"projects`":[{`"name`":`"agentspace`",`"approved`":`"2026-11-20`"}]}`n"; want = 5 }
  'planted-crlf'        = @{ text = "$good`r`n"; want = 5 }
  'planted-empty'       = @{ text = "{`"version`":0,`"projects`":[{`"name`":`"agentspace`",`"approved`":`"2026-11-20`",`"platforms`":[]}]}`n"; want = 5 }
  'planted-wrongdate'   = @{ text = ($good -replace 'windows/amd64","approved":"2026-11-26', 'windows/amd64","approved":"2026-11-25') + "`n"; want = 5 }
}
$bad = 0
foreach ($name in $cases.Keys) {
  $path = Join-Path $dir "$name.json"
  [IO.File]::WriteAllText($path, $cases[$name].text, [Text.UTF8Encoding]::new($false))
  & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'lead-projects-check.ps1') -File $path | Out-Null
  $code = $LASTEXITCODE
  $pass = ($code -eq $cases[$name].want)
  if (-not $pass) { $bad++ }
  "$name exit=$code want=$($cases[$name].want) $(if ($pass) { 'PASS' } else { 'FAIL' })"
}
"proof failures: $bad"
if ($bad) { exit 1 }
exit 0

# Read-only check of the project list after Terry's write (architect, 2026-11-27, for v0.12-T05, pin P4 as changed).
# It writes nothing. It prints only counts, agentspace's approved date and platforms, and True/False lines; never
# another entry's name. Exit 0 when every check holds, 5 when any does not.
param(
  [string]$File = (Join-Path $env:USERPROFILE '.agentspace\projects.json'),
  [string]$Schema = (Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'schemas\state\projects.state.schema.json')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$bytes = [IO.File]::ReadAllBytes($File)
$bom = ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)
$text = [Text.UTF8Encoding]::new($false).GetString($bytes)
$endsOneLf = $text.EndsWith("`n") -and -not $text.EndsWith("`n`n") -and -not $text.Contains("`r")
$oneLine = ($text.TrimEnd("`n") -notmatch "`n")
# The validator is conformance\schemas\test-state-schemas.ps1's Test-Document, the same Test-Json call.
$result = try {
  if (Test-Json -Json $text -SchemaFile $Schema -ErrorAction Stop) { 'valid' } else { 'invalid' }
} catch {
  if ($_.Exception.Message -like '*is not valid with the schema*') { 'invalid' } else { 'error' }
}
$doc = $text | ConvertFrom-Json -DateKind String
$entries = @($doc.projects)
$ours = @($entries | Where-Object { $_.name -ceq 'agentspace' })
$ok = $true
"entries: $($entries.Count)"
"agentspace entries: $($ours.Count)"
if ($ours.Count -ne 1) { $ok = $false } else {
  $entry = $ours[0]
  "agentspace approved: $($entry.approved)"
  $hasPlatforms = $null -ne $entry.PSObject.Properties['platforms']
  "agentspace has platforms: $hasPlatforms"
  $got = if ($hasPlatforms) { @($entry.platforms | ForEach-Object { "$($_.platform)@$($_.approved)" }) -join ',' } else { '' }
  "agentspace platforms: $got"
  $want = "linux/arm64@$($entry.approved),linux/amd64@2026-11-26,windows/amd64@2026-11-26"
  $match = ($got -ceq $want)
  "platforms as state.md gives them: $match"
  $order = @($entry.PSObject.Properties.Name) -join ','
  "members in order name,approved,platforms: $($order -ceq 'name,approved,platforms')"
  if (-not $match -or $order -cne 'name,approved,platforms') { $ok = $false }
}
"no BOM: $(-not $bom)"
"ends with exactly one LF, no CR: $endsOneLf"
"one line: $oneLine"
"schema result: $result"
if ($bom -or -not $endsOneLf -or -not $oneLine -or $result -ne 'valid') { $ok = $false }
if (-not $ok) { 'FINDING: the project list is not as state.md gives it'; exit 5 }
exit 0

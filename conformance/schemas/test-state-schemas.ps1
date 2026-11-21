#Requires -Version 7
# A conformance check under D34 with no program under test: it validates schemas/state/samples against schemas/state/, loads no prototype code and reads only schemas/state/. See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Checks the state schemas (docs/state.md, D45, D55) that have samples, beside conformance/schemas/test-schemas.ps1,
# which reads the top level of schemas/ only:
# - Every sample in schemas/state/samples/ and schemas/state/samples/invalid/ MUST name a state schema, as
#   <schema name>.<case>.json, so a misnamed sample is never skipped silently.
# - Every state schema that has a sample MUST declare draft 2020-12, have the $id of the state form
#   (https://github.com/SixbucksSolutions/agentspace/schemas/state/v0/<its file name>), and have a title and a description.
# - Every sample in schemas/state/samples/ MUST validate against its schema.
# - Every document in schemas/state/samples/invalid/ MUST be rejected by its schema for a schema reason, never because
#   the schema or the file is broken, so the check is proven able to fail. A state schema that has a valid sample
#   MUST have at least one invalid one.
# A state schema with no sample is not checked here; the keeper suite checks the keeper's three against the files a run writes.
#
# It drives no program, so it names none and reads no variable. It reads files and changes nothing.
# -StateDir names another copy of schemas/state, which the check's own proof that it can fail uses (a scratch copy with a planted sample).
# Run:  pwsh -NoProfile -File conformance\schemas\test-state-schemas.ps1 [-StateDir <folder>]
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line. Exit 0 when every check passed, 1 when any failed, 2 when none failed but any was
# skipped (a run with a skip is incomplete, never passed).
param([string]$StateDir = '')

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$schemaDir = if ($StateDir) { $StateDir } else { Join-Path $repo 'schemas\state' }
$sampleDir = Join-Path $schemaDir 'samples'
$invalidDir = Join-Path $sampleDir 'invalid'
$script:passes = 0
$script:failures = 0
$script:skips = 0

# A check name cites the spec section it enforces.
function Write-Result([bool]$Ok, [string]$Text, [string]$Why = 'The check did not hold.') {
  if ($Ok) { Write-Output "PASS $Text"; $script:passes++ } else { Write-Output "FAIL ${Text}: $Why"; $script:failures++ }
}

function Exit-Run {
  Write-Output "$($script:passes) passed, $($script:failures) failed, $($script:skips) skipped"
  if ($script:failures) { exit 1 }
  if ($script:skips) { exit 2 }
  exit 0
}

# Returns 'valid', 'invalid' (rejected by the schema) or 'error: ...' (the schema or the JSON is broken).
function Test-Document([string]$Json, [string]$SchemaPath) {
  try {
    if (Test-Json -Json $Json -SchemaFile $SchemaPath -ErrorAction Stop) { return 'valid' }
    return 'invalid'
  } catch {
    $m = $_.Exception.Message
    if ($m -like '*is not valid with the schema*') { return 'invalid' }
    return "error: $m"
  }
}

# The state.md section that owns a state schema; a schema the map does not name is cited by the page.
function Get-SpecFor([string]$Stem) {
  switch ($Stem) { 'hosts.state' { 'state.md#the-host-registry' } 'projects.state' { 'state.md#the-project-list' } default { 'state.md' } }
}

$schemas = @(Get-ChildItem -LiteralPath $schemaDir -Filter '*.schema.json' -File | Sort-Object Name)
if (-not $schemas) { Write-Result $false 'state.md: The state schemas are found' "No schemas found in $schemaDir"; Exit-Run }
$stems = $schemas | ForEach-Object { $_.Name -replace '\.schema\.json$', '' }

# Every sample MUST belong to a schema, so a misnamed one is never silently skipped.
foreach ($dir in $sampleDir, $invalidDir) {
  if (-not (Test-Path -LiteralPath $dir -PathType Container)) { continue }
  foreach ($f in Get-ChildItem -LiteralPath $dir -Filter '*.json' -File) {
    $owner = $stems | Where-Object { $f.Name.StartsWith("$_.") }
    Write-Result ([bool]$owner) "$(if ($owner) { Get-SpecFor @($owner)[0] } else { 'state.md' }): Sample $($f.Name) names a state schema" "Sample $($f.Name) in $dir names no schema"
  }
}

foreach ($schema in $schemas) {
  $stem = $schema.Name -replace '\.schema\.json$', ''
  $good = @(Get-ChildItem -LiteralPath $sampleDir -Filter "$stem.*.json" -File -ErrorAction SilentlyContinue | Sort-Object Name)
  $bad = @(if (Test-Path -LiteralPath $invalidDir) { Get-ChildItem -LiteralPath $invalidDir -Filter "$stem.*.json" -File | Sort-Object Name })
  if ($good.Count -eq 0 -and $bad.Count -eq 0) { continue }

  $doc = Get-Content -Raw -LiteralPath $schema.FullName | ConvertFrom-Json -AsHashtable
  Write-Result ($doc['$schema'] -eq 'https://json-schema.org/draft/2020-12/schema') "$(Get-SpecFor $stem): $($schema.Name) declares draft 2020-12"
  Write-Result ($doc['$id'] -ceq "https://github.com/SixbucksSolutions/agentspace/schemas/state/v0/$($schema.Name)") "$(Get-SpecFor $stem): $($schema.Name) has its state v0 `$id" "Its `$id is $($doc['$id'])."
  Write-Result ([bool]$doc['title'] -and [bool]$doc['description']) "$(Get-SpecFor $stem): $($schema.Name) has a title and a description"

  foreach ($s in $good) {
    $r = Test-Document (Get-Content -Raw -LiteralPath $s.FullName) $schema.FullName
    Write-Result ($r -eq 'valid') "$(Get-SpecFor $stem): $($s.Name) validates" "The result was: $r."
  }
  if ($good.Count -ge 1) {
    Write-Result ($bad.Count -ge 1) "$(Get-SpecFor $stem): $($schema.Name) has at least one invalid document" "It has $($bad.Count)."
  }
  foreach ($s in $bad) {
    $r = Test-Document (Get-Content -Raw -LiteralPath $s.FullName) $schema.FullName
    Write-Result ($r -eq 'invalid') "$(Get-SpecFor $stem): invalid/$($s.Name) is rejected for a schema reason" "The result was: $r."
  }
}

Exit-Run

#Requires -Version 7
# A conformance check under D34 with no program under test: it validates schemas/state/samples against schemas/state/, loads no prototype code and reads only schemas/state/ and the plan.input.schema.json beside it. See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Checks the state schemas (docs/state.md, D45, D55, D74, D77) against their samples, beside conformance/schemas/test-schemas.ps1,
# which reads the top level of schemas/ and schemas/v0/, never schemas/state/:
# - Every sample in schemas/state/samples/ and schemas/state/samples/invalid/ MUST name a state schema, as
#   <schema name>.<case>.json, so a misnamed sample is never skipped silently.
# - Every state schema that has a sample MUST declare draft 2020-12, have the $id of the state form
#   (https://github.com/SixbucksSolutions/agentspace/schemas/state/v0/<its file name>), and have a title and a description.
# - Every sample in schemas/state/samples/ MUST validate against its schema.
# - Every document in schemas/state/samples/invalid/ MUST be rejected by its schema for a schema reason, never because
#   the schema or the file is broken, so the check is proven able to fail. A state schema that has a valid sample
#   MUST have at least one invalid one.
# - Every state schema MUST have at least one sample, valid or invalid, unless $knownWithoutSamples below names it, so a
#   new state schema, or one whose samples were removed, is never skipped silently. A named schema that has samples
#   fails too, so the list never outlives its reason.
# - Every one of the plan schema's $defs MUST appear word for word, compared parsed, in leases.state.schema.json's
#   $defs, as test-schemas.ps1 checks lease-create.input.schema.json's copy, since Test-Json -SchemaFile does not
#   resolve a $ref to another file and each schema carries its own copy (state.md, What a plan lease adds). The plan
#   schema is the plan.input.schema.json in the state folder's parent: the repo's schemas/ for the default folder,
#   and the planted copy's parent for -StateDir. A missing one MUST fail, never skip.
#
# It drives no program, so it names none and reads no variable. It reads files and changes nothing.
# -StateDir names another copy of schemas/state, which the check's own proof that it can fail uses (a scratch copy with a planted sample).
# Run:  pwsh -NoProfile -File conformance\schemas\test-state-schemas.ps1 [-StateDir <folder>]
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line. Exit 0 when every check passed, 1 when any failed, 2 when none failed but any was
# skipped (a run with a skip is incomplete, never passed).
param([string]$StateDir = '')

$ErrorActionPreference = 'Stop'
# The harness, for the suite's one mask (conformance\mask-common.ps1; docs/conformance.md, "The shared mask"):
# a reason can quote Test-Json's message, which can name a schema file's path, and a checkout can sit under
# the profile or the temp folder (architect, 2026-11-26, for v0.11-T05, N3). This file runs no program.
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'lease-cli\harness.ps1')
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$schemaDir = if ($StateDir) { $StateDir } else { Join-Path $repo 'schemas\state' }
$sampleDir = Join-Path $schemaDir 'samples'
$invalidDir = Join-Path $sampleDir 'invalid'
$script:passes = 0
$script:failures = 0
$script:skips = 0

# The state schemas known to have no sample here, each with where its files are checked instead. Only these MAY have
# none; any other schema with none fails. Listed on 2026-11-24 (E0.10-T01-S03) from a count of each schema's samples.
# board-holder.state left the list in E0.12-T01-S02, when the holder record gained platform and samples of it (state.md,
# The holder record).
$knownWithoutSamples = @(
  'board-holder-launch-result.state' # The keeper's; conformance/keeper/test-keeper.ps1 checks the files a run writes.
  'board-holder-launch.state'        # The keeper's; the same.
  'instance-id.state'                # The retired instance-ID cache, which nothing writes from v0.07 (state.md).
  'localswim-pushed.state'           # The push stamps; agentspace-push's tests check the stamp it writes.
  'stack-parameters.state'           # Written by a person, never by agentspace (state.md, Files in the checkout).
)

# A check name cites the spec section it enforces.
function Write-Result([bool]$Ok, [string]$Text, [string]$Why = 'The check did not hold.') {
  if ($Ok) { Write-Output "PASS $Text"; $script:passes++ } else { Write-Output (ConvertTo-ConformanceMasked "FAIL ${Text}: $Why"); $script:failures++ }
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
  switch ($Stem) { 'hosts.state' { 'state.md#the-host-registry' } 'projects.state' { 'state.md#the-project-list' } 'leases.state' { 'state.md#the-lease-state-file' } 'board-holder.state' { 'state.md#the-holder-record' } default { 'state.md' } }
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
  $known = $knownWithoutSamples -ccontains $stem
  if ($good.Count -eq 0 -and $bad.Count -eq 0) {
    Write-Result $known "$(Get-SpecFor $stem): $($schema.Name) has a sample, or is known to have none" "It has no sample in $sampleDir or $invalidDir, and is not on the list of state schemas known to have none."
    continue
  }
  if ($known) {
    Write-Result $false "$(Get-SpecFor $stem): $($schema.Name) has a sample, or is known to have none" "It is on the list of state schemas known to have none, and has $($good.Count + $bad.Count); take it off the list."
  }

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

# The lease state schema carries a copy of the plan schema's $defs; no def of the plan's MAY drift from its copy.
$planFile = Join-Path (Split-Path -Parent $schemaDir.TrimEnd('\', '/')) 'plan.input.schema.json'
$leasesFile = Join-Path $schemaDir 'leases.state.schema.json'
$planDefsName = "state.md#what-a-plan-lease-adds: leases.state.schema.json `$defs hold plan.input.schema.json `$defs word for word"
if ((Test-Path -LiteralPath $planFile -PathType Leaf) -and (Test-Path -LiteralPath $leasesFile -PathType Leaf)) {
  $planDefs = (Get-Content -Raw -LiteralPath $planFile | ConvertFrom-Json -AsHashtable)['$defs']
  $leasesDefs = (Get-Content -Raw -LiteralPath $leasesFile | ConvertFrom-Json -AsHashtable)['$defs']
  $drifted = @(if ($planDefs) {
    foreach ($k in $planDefs.Keys) {
      $copy = if ($leasesDefs -and $leasesDefs.ContainsKey($k)) { $leasesDefs[$k] | ConvertTo-Json -Depth 50 -Compress } else { $null }
      if ($copy -cne ($planDefs[$k] | ConvertTo-Json -Depth 50 -Compress)) { $k }
    }
  })
  $hasDefs = $null -ne $planDefs -and $planDefs.Count -ge 1
  $why = if (-not $hasDefs) { 'plan.input.schema.json has no $defs.' } else { "These differ or are missing: $($drifted -join ', ')." }
  Write-Result ($hasDefs -and $drifted.Count -eq 0) $planDefsName $why
} else {
  $missing = @(foreach ($p in $planFile, $leasesFile) { if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { Split-Path -Leaf $p } })
  Write-Result $false $planDefsName "Missing: $($missing -join ', ')."
}

Exit-Run

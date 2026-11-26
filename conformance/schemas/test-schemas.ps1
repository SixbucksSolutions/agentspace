#Requires -Version 7
# A conformance check under D34 with no program under test: it validates schemas/samples against schemas/, loads no prototype code and reads only schemas/. See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Checks the lease contract's JSON schemas (docs/lease-api.md, D20) against their samples:
# - Every sample in schemas/samples/<schema name>.<case>.json MUST validate against its schema.
# - Every document in schemas/samples/invalid/ MUST be rejected by its schema, for a schema reason,
#   so the check is proven able to fail. Every output schema MUST have at least one.
# - The list schema's $defs MUST be identical to the lease schema's, since Test-Json -SchemaFile
#   does not resolve a $ref to another file and each schema carries its own copy.
# - Every one of the plan schema's $defs MUST appear word for word, compared parsed, in the create
#   request schema's $defs, for the same reason, in each version's own tree. Contract v0 has no plan
#   schema, so this check runs on schemas/ and schemas/v1/, never on schemas/v0/.
# - The TTL grammar MUST accept and refuse the spellings that docs/lease-api.md lists.
# - A sample lease's host alias MUST be agentspace-<project>-<first 8 hex digits of its ID>.
# - Contract v2 keeps v1's schemas and samples unchanged under schemas/v1/ (D87), and v0's under
#   schemas/v0/ (D76). Every check above MUST run on schemas/ (whose $id carries v2), again on
#   schemas/v1/ (whose $id carries v1) and again on schemas/v0/ (whose $id carries v0), each against
#   its own schemas and samples only, so a sample of one version is never checked against another
#   version's schema. schemas/v1/ and schemas/v0/ MUST each exist and hold at least one schema, one
#   valid sample and one invalid sample, so a deleted copy cannot pass silently.
#
# It drives no program, so it names none and reads no variable. It reads files and changes nothing.
# Run:  pwsh -NoProfile -File conformance\schemas\test-schemas.ps1 [-SchemaRoot <folder>]
# -SchemaRoot names the folder that holds the schemas and its v1 and v0 folders (default: the
# repository's schemas\), so a planted copy can prove that each check fails.
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line. Exit 0 when every check passed, 1 when any failed, 2 when none failed but any was
# skipped (a run with a skip is incomplete, never passed).

param([string]$SchemaRoot)

$ErrorActionPreference = 'Stop'
# The harness, for the suite's one mask (conformance\mask-common.ps1; docs/conformance.md, "The shared mask"):
# a reason can quote Test-Json's message, which can name a schema file's path, and a checkout can sit under
# the profile or the temp folder (architect, 2026-11-26, for v0.11-T05, N3). This file runs no program.
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'lease-cli\harness.ps1')
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$schemaDir = if ($SchemaRoot) { $SchemaRoot } else { Join-Path $repo 'schemas' }
$script:passes = 0
$script:failures = 0
$script:skips = 0

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

# Checks one tree: its schemas in $Dir, its samples in $Dir\samples and $Dir\samples\invalid. $Version is the
# contract version its $ids carry; $Tag is '' for the top level and 'v1/' or 'v0/' for a kept copy, and prefixes the
# names. $WithPlan is true for a contract that has a plan schema (v1 on), whose $defs the create request copies.
function Test-SchemaTree([string]$Dir, [string]$Version, [string]$Tag, [bool]$WithPlan) {
  $sampleDir = Join-Path $Dir 'samples'
  $invalidDir = Join-Path $sampleDir 'invalid'
  $found = Test-Path -LiteralPath $Dir -PathType Container
  $schemas = @(if ($found) { Get-ChildItem -LiteralPath $Dir -Filter '*.schema.json' -File | Sort-Object Name })
  Write-Result ($schemas.Count -ge 1) "lease-api.md#schemas: ${Tag}The schemas are found" "No schemas found in $Dir"
  if (-not $schemas) { return }
  $stems = $schemas | ForEach-Object { $_.Name -replace '\.schema\.json$', '' }
  $good = @(); $bad = @()
  if (Test-Path -LiteralPath $sampleDir -PathType Container) { $good = @(Get-ChildItem -LiteralPath $sampleDir -Filter '*.json' -File) }
  if (Test-Path -LiteralPath $invalidDir -PathType Container) { $bad = @(Get-ChildItem -LiteralPath $invalidDir -Filter '*.json' -File) }
  if ($Tag) {
    # The kept copy MUST NOT be empty of samples, valid or invalid.
    Write-Result ($good.Count -ge 1) "lease-api.md#schemas: ${Tag}samples are found" "No samples in $sampleDir"
    Write-Result ($bad.Count -ge 1) "lease-api.md#schemas: ${Tag}invalid samples are found" "No invalid samples in $invalidDir"
  }

  # Every sample MUST belong to a schema, so a misnamed one is never silently skipped.
  foreach ($f in @($good) + @($bad)) {
    $owner = $stems | Where-Object { $f.Name.StartsWith("$_.") }
    Write-Result ([bool]$owner) "lease-api.md#schemas: ${Tag}Sample $($f.Name) names a schema" "Sample $($f.Name) in $($f.DirectoryName) names no schema"
  }

  foreach ($schema in $schemas) {
    $stem = $schema.Name -replace '\.schema\.json$', ''
    $doc = Get-Content -Raw -LiteralPath $schema.FullName | ConvertFrom-Json -AsHashtable
    Write-Result ($doc['$schema'] -eq 'https://json-schema.org/draft/2020-12/schema') "lease-api.md#schemas: ${Tag}$($schema.Name) declares draft 2020-12"
    Write-Result ($doc['$id'] -eq "https://github.com/SixbucksSolutions/agentspace/schemas/$Version/$($schema.Name)") "lease-api.md#schemas: ${Tag}$($schema.Name) has its $Version `$id"
    Write-Result ([bool]$doc['title'] -and [bool]$doc['description']) "lease-api.md#schemas: ${Tag}$($schema.Name) has a title and a description"

    # A sample is only ever checked against the schema of its own tree: the schema's path is built from $Dir.
    $mine = @($good | Where-Object { $_.Name -like "$stem.*.json" } | Sort-Object Name)
    Write-Result ($mine.Count -ge 1) "lease-api.md#schemas: ${Tag}$($schema.Name) has at least one sample" "It has $($mine.Count)."
    foreach ($s in $mine) {
      $r = Test-Document (Get-Content -Raw -LiteralPath $s.FullName) $schema.FullName
      Write-Result ($r -eq 'valid') "lease-api.md#schemas: ${Tag}$($s.Name) validates" "The result was: $r."
    }

    $mineBad = @($bad | Where-Object { $_.Name -like "$stem.*.json" } | Sort-Object Name)
    if ($stem -like '*.output') {
      Write-Result ($mineBad.Count -ge 1) "lease-api.md#schemas: ${Tag}$($schema.Name) has at least one invalid document" "It has $($mineBad.Count)."
    }
    foreach ($s in $mineBad) {
      $r = Test-Document (Get-Content -Raw -LiteralPath $s.FullName) $schema.FullName
      Write-Result ($r -eq 'invalid') "lease-api.md#schemas: ${Tag}invalid/$($s.Name) is rejected" "The result was: $r."
    }
  }

  # The list schema carries a copy of the lease schema's $defs; the two MUST NOT drift apart.
  $leaseFile = Join-Path $Dir 'lease.output.schema.json'
  $listFile = Join-Path $Dir 'lease-list.output.schema.json'
  $defsName = "lease-api.md#schemas: ${Tag}lease-list.output.schema.json `$defs are identical to lease.output.schema.json `$defs"
  if ((Test-Path -LiteralPath $leaseFile) -and (Test-Path -LiteralPath $listFile)) {
    $leaseDefs = (Get-Content -Raw $leaseFile | ConvertFrom-Json -AsHashtable)['$defs']
    $listDefs = (Get-Content -Raw $listFile | ConvertFrom-Json -AsHashtable)['$defs']
    Write-Result (($leaseDefs | ConvertTo-Json -Depth 50 -Compress) -ceq ($listDefs | ConvertTo-Json -Depth 50 -Compress)) $defsName 'The two $defs differ.'
  } else {
    Write-Result $false $defsName 'One of the two schemas is missing.'
  }

  # The create request carries a copy of the plan schema's $defs; no def of the plan's MAY drift from its copy. Each
  # version's copy is compared with that version's own plan schema.
  if ($WithPlan) {
    $planFile = Join-Path $Dir 'plan.input.schema.json'
    $createFile = Join-Path $Dir 'lease-create.input.schema.json'
    $planDefsName = "lease-api.md#schemas: ${Tag}lease-create.input.schema.json `$defs hold plan.input.schema.json `$defs word for word"
    if ((Test-Path -LiteralPath $planFile) -and (Test-Path -LiteralPath $createFile)) {
      $planDefs = (Get-Content -Raw -LiteralPath $planFile | ConvertFrom-Json -AsHashtable)['$defs']
      $createDefs = (Get-Content -Raw -LiteralPath $createFile | ConvertFrom-Json -AsHashtable)['$defs']
      $drifted = @(if ($planDefs) {
        foreach ($k in $planDefs.Keys) {
          $copy = if ($createDefs -and $createDefs.ContainsKey($k)) { $createDefs[$k] | ConvertTo-Json -Depth 50 -Compress } else { $null }
          if ($copy -cne ($planDefs[$k] | ConvertTo-Json -Depth 50 -Compress)) { $k }
        }
      })
      $hasDefs = $null -ne $planDefs -and $planDefs.Count -ge 1
      $why = if (-not $hasDefs) { 'plan.input.schema.json has no $defs.' } else { "These differ or are missing: $($drifted -join ', ')." }
      Write-Result ($hasDefs -and $drifted.Count -eq 0) $planDefsName $why
    } else {
      Write-Result $false $planDefsName 'One of the two schemas is missing.'
    }
  }

  # The TTL grammar, from docs/lease-api.md "TTL". Range (1 minute to 24 hours) is not a schema matter.
  $accepted = @('8h', '90m', '1h30m', '1h 30m', '2 hours', '45 min', '1d', ' 8H ', '1Min', '30 MINUTES', '1 hr',
    '2hrs', '1day', '3600s', '10 secs', '1d2h3m4s', '24h1s', 'PT8H', 'pt90m', 'P1D', 'P1DT2H', 'PT1H30M', 'PT0S')
  $refused = @('8', '28800', '', '   ', '8M', '1.5h', '-1h', '+1h', '1w', '1 week', '1y', 'P1M', 'P1W', 'P', 'PT',
    'P1DT', '1h1h', '30m1h', '5ms', 'h', '8 hours ago', '1h,30m', 'PT1.5H', '8 h r')
  $ttlSchemas = 'lease-create.input.schema.json', 'lease-renew.input.schema.json'
  foreach ($set in @(@{ Spellings = $accepted; Want = 'valid'; Word = 'accepted' }, @{ Spellings = $refused; Want = 'invalid'; Word = 'refused' })) {
    foreach ($t in $set.Spellings) {
      foreach ($sn in $ttlSchemas) {
        $sp = Join-Path $Dir $sn
        if (-not (Test-Path -LiteralPath $sp)) { Write-Result $false "lease-api.md#ttl: ${Tag}TTL '$t' is $($set.Word) by $sn" "$sn is missing."; continue }
        $doc = @{ project = 'swimcoach'; id = '3f2a9c1e-7b4d-4e2a-9c1f-5d8e2b7a6c40'; ttl = $t }
        if ($sn -like 'lease-create*') { $doc.Remove('id') } else { $doc.Remove('project') }
        $r = Test-Document ($doc | ConvertTo-Json -Compress) $sp
        Write-Result ($r -eq $set.Want) "lease-api.md#ttl: ${Tag}TTL '$t' is $($set.Word) by $sn" "The result was: $r."
      }
    }
  }

  # A lease's host alias is derived from its project and ID, which a schema cannot check.
  foreach ($s in $good | Where-Object { $_.Name -like 'lease*.output.*.json' }) {
    $doc = Get-Content -Raw -LiteralPath $s.FullName | ConvertFrom-Json -AsHashtable
    $leases = if ($doc -is [hashtable] -and $doc.ContainsKey('leases')) { $doc['leases'] } else { @($doc) }
    foreach ($l in $leases | Where-Object { $_.ContainsKey('ssh') }) {
      $want = "agentspace-$($l['project'])-$($l['id'].Substring(0, 8))"
      Write-Result ($l['ssh']['host_alias'] -ceq $want) "lease-api.md#fields: ${Tag}$($s.Name): host alias of $($l['id']) is $want" "It is $($l['ssh']['host_alias'])."
    }
  }
}

Test-SchemaTree $schemaDir 'v2' '' $true
Test-SchemaTree (Join-Path $schemaDir 'v1') 'v1' 'v1/' $true
Test-SchemaTree (Join-Path $schemaDir 'v0') 'v0' 'v0/' $false

Exit-Run

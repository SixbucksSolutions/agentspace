#Requires -Version 7
# A spec self-check under D34: it drives no program and loads no prototype code; the rewrite MAY read it only once it is moved to conformance/. See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Checks the lease contract's JSON schemas (docs/lease-api.md, D20) against their samples:
# - Every sample in schemas/samples/<schema name>.<case>.json MUST validate against its schema.
# - Every document in schemas/samples/invalid/ MUST be rejected by its schema, for a schema reason,
#   so the check is proven able to fail. Every output schema MUST have at least one.
# - The list schema's $defs MUST be identical to the lease schema's, since Test-Json -SchemaFile
#   does not resolve a $ref to another file and each schema carries its own copy.
# - The TTL grammar MUST accept and refuse the spellings that docs/lease-api.md lists.
# - A sample lease's host alias MUST be agentspace-<project>-<first 8 hex digits of its ID>.
# Exits 0 when everything passes, 1 otherwise. Laptop only; it reads files and changes nothing.

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$schemaDir = Join-Path $repo 'schemas'
$sampleDir = Join-Path $schemaDir 'samples'
$invalidDir = Join-Path $sampleDir 'invalid'
$failures = 0

function Write-Result([bool]$Ok, [string]$Text) {
  if ($Ok) { Write-Output "PASS  $Text" } else { Write-Output "FAIL  $Text"; $script:failures++ }
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

$schemas = Get-ChildItem -LiteralPath $schemaDir -Filter '*.schema.json' -File | Sort-Object Name
if (-not $schemas) { Write-Result $false "No schemas found in $schemaDir"; exit 1 }
$stems = $schemas | ForEach-Object { $_.Name -replace '\.schema\.json$', '' }

# Every sample MUST belong to a schema, so a misnamed one is never silently skipped.
foreach ($dir in $sampleDir, $invalidDir) {
  foreach ($f in Get-ChildItem -LiteralPath $dir -Filter '*.json' -File) {
    $owner = $stems | Where-Object { $f.Name.StartsWith("$_.") }
    if (-not $owner) { Write-Result $false "Sample $($f.Name) in $dir names no schema" }
  }
}

foreach ($schema in $schemas) {
  $stem = $schema.Name -replace '\.schema\.json$', ''
  $doc = Get-Content -Raw -LiteralPath $schema.FullName | ConvertFrom-Json -AsHashtable
  Write-Result ($doc['$schema'] -eq 'https://json-schema.org/draft/2020-12/schema') "$($schema.Name) declares draft 2020-12"
  Write-Result ($doc['$id'] -eq "https://github.com/SixbucksSolutions/agentspace/schemas/v0/$($schema.Name)") "$($schema.Name) has its v0 `$id"
  Write-Result ([bool]$doc['title'] -and [bool]$doc['description']) "$($schema.Name) has a title and a description"

  $good = @(Get-ChildItem -LiteralPath $sampleDir -Filter "$stem.*.json" -File | Sort-Object Name)
  Write-Result ($good.Count -ge 1) "$($schema.Name) has at least one sample ($($good.Count))"
  foreach ($s in $good) {
    $r = Test-Document (Get-Content -Raw -LiteralPath $s.FullName) $schema.FullName
    Write-Result ($r -eq 'valid') "$($s.Name) validates ($r)"
  }

  $bad = @(Get-ChildItem -LiteralPath $invalidDir -Filter "$stem.*.json" -File | Sort-Object Name)
  if ($stem -like '*.output') {
    Write-Result ($bad.Count -ge 1) "$($schema.Name) has at least one invalid document ($($bad.Count))"
  }
  foreach ($s in $bad) {
    $r = Test-Document (Get-Content -Raw -LiteralPath $s.FullName) $schema.FullName
    Write-Result ($r -eq 'invalid') "invalid/$($s.Name) is rejected ($r)"
  }
}

# The list schema carries a copy of the lease schema's $defs; the two MUST NOT drift apart.
$leaseDefs = (Get-Content -Raw (Join-Path $schemaDir 'lease.output.schema.json') | ConvertFrom-Json -AsHashtable)['$defs']
$listDefs = (Get-Content -Raw (Join-Path $schemaDir 'lease-list.output.schema.json') | ConvertFrom-Json -AsHashtable)['$defs']
Write-Result (($leaseDefs | ConvertTo-Json -Depth 50 -Compress) -ceq ($listDefs | ConvertTo-Json -Depth 50 -Compress)) `
  'lease-list.output.schema.json $defs are identical to lease.output.schema.json $defs'

# The TTL grammar, from docs/lease-api.md "TTL". Range (1 minute to 24 hours) is not a schema matter.
$accepted = @('8h', '90m', '1h30m', '1h 30m', '2 hours', '45 min', '1d', ' 8H ', '1Min', '30 MINUTES', '1 hr',
  '2hrs', '1day', '3600s', '10 secs', '1d2h3m4s', '24h1s', 'PT8H', 'pt90m', 'P1D', 'P1DT2H', 'PT1H30M', 'PT0S')
$refused = @('8', '28800', '', '   ', '8M', '1.5h', '-1h', '+1h', '1w', '1 week', '1y', 'P1M', 'P1W', 'P', 'PT',
  'P1DT', '1h1h', '30m1h', '5ms', 'h', '8 hours ago', '1h,30m', 'PT1.5H', '8 h r')
$ttlSchemas = 'lease-create.input.schema.json', 'lease-renew.input.schema.json'
foreach ($t in $accepted) {
  foreach ($sn in $ttlSchemas) {
    $doc = @{ project = 'swimcoach'; id = '3f2a9c1e-7b4d-4e2a-9c1f-5d8e2b7a6c40'; ttl = $t }
    if ($sn -like 'lease-create*') { $doc.Remove('id') } else { $doc.Remove('project') }
    $r = Test-Document ($doc | ConvertTo-Json -Compress) (Join-Path $schemaDir $sn)
    Write-Result ($r -eq 'valid') "TTL '$t' is accepted by $sn ($r)"
  }
}
foreach ($t in $refused) {
  foreach ($sn in $ttlSchemas) {
    $doc = @{ project = 'swimcoach'; id = '3f2a9c1e-7b4d-4e2a-9c1f-5d8e2b7a6c40'; ttl = $t }
    if ($sn -like 'lease-create*') { $doc.Remove('id') } else { $doc.Remove('project') }
    $r = Test-Document ($doc | ConvertTo-Json -Compress) (Join-Path $schemaDir $sn)
    Write-Result ($r -eq 'invalid') "TTL '$t' is refused by $sn ($r)"
  }
}

# A lease's host alias is derived from its project and ID, which a schema cannot check.
foreach ($s in Get-ChildItem -LiteralPath $sampleDir -Filter 'lease*.output.*.json' -File) {
  $doc = Get-Content -Raw -LiteralPath $s.FullName | ConvertFrom-Json -AsHashtable
  $leases = if ($doc -is [hashtable] -and $doc.ContainsKey('leases')) { $doc['leases'] } else { @($doc) }
  foreach ($l in $leases | Where-Object { $_.ContainsKey('ssh') }) {
    $want = "agentspace-$($l['project'])-$($l['id'].Substring(0, 8))"
    Write-Result ($l['ssh']['host_alias'] -ceq $want) "$($s.Name): host alias of $($l['id']) is $want"
  }
}

if ($failures) { Write-Output "$failures check(s) failed."; exit 1 }
Write-Output 'All schema checks passed.'
exit 0

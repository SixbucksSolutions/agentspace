# Shared by the stand-ins of the v0.12-T05 linux/amd64 check's dry runs and by the dry-run drivers (drafter-owned).
# RFC 2119 keywords; the capitals are load-bearing.
# - A stand-in answers the expected documents of s01-expected.md. It touches only S01_STANDIN_DIR, which the driver sets
#   inside a conformance-harness home, and starts no program. S01_PLANT names ONE planted answer (a planted-* case); unset,
#   the answer is the clean one.
# - A lease's store is S01_STANDIN_DIR\leases.json; members whose names start with _ are the stand-in's own and never printed.

$script:SlUtf8 = [Text.UTF8Encoding]::new($false)

function Get-SlDir { if (-not $env:S01_STANDIN_DIR) { throw 'S01_STANDIN_DIR is not set.' }; New-Item -ItemType Directory -Force -Path $env:S01_STANDIN_DIR | Out-Null; return $env:S01_STANDIN_DIR }
function Get-SlPlant { return [string]$env:S01_PLANT }
function Format-SlTime([DateTimeOffset]$t) { return $t.ToString('yyyy-MM-ddTHH:mm:ssZ', [Globalization.CultureInfo]::InvariantCulture) }

function Read-SlStore {
  $f = Join-Path (Get-SlDir) 'leases.json'
  if (-not (Test-Path -LiteralPath $f)) { return [System.Collections.Generic.List[object]]::new() }
  $items = @([IO.File]::ReadAllText($f, $script:SlUtf8) | ConvertFrom-Json -AsHashtable -DateKind String -Depth 10)
  $l = [System.Collections.Generic.List[object]]::new()
  foreach ($i in $items) { if ($null -ne $i) { $l.Add($i) } }
  return , $l
}
function Write-SlStore($List) {
  [IO.File]::WriteAllText((Join-Path (Get-SlDir) 'leases.json'), (ConvertTo-Json -InputObject @($List) -Depth 10), $script:SlUtf8)
}

# A lease in the contract's member order (docs\lease-api.md, Fields). Members _gets and _ttl are the stand-in's own.
function New-SlLease {
  param([string]$State, [string]$Size = 'm', [int]$Ttl = 7200, [string]$Project = 'agentspace', [string]$Platform = 'linux/amd64')
  $id = [guid]::NewGuid().ToString()
  $now = [DateTimeOffset]::UtcNow
  $l = [ordered]@{ id = $id; project = $Project; state = $State; platform = $Platform; profile = 'agent-linux'; size = $Size; ttl_seconds = $Ttl; created_at = (Format-SlTime $now) }
  if ($State -ceq 'ready') {
    $l['expires_at'] = Format-SlTime $now.AddSeconds($Ttl)
    $l['ssh'] = [ordered]@{ host_alias = ('agentspace-' + $Project + '-' + $id.Substring(0, 8)); user = 'ubuntu' }
  }
  $l['_gets'] = 0
  return $l
}
function Set-SlReady($l) {
  $now = [DateTimeOffset]::UtcNow
  $new = [ordered]@{}
  foreach ($k in 'id', 'project', 'state', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at') { $new[$k] = $l[$k] }
  $new['state'] = 'ready'
  $new['expires_at'] = Format-SlTime $now.AddSeconds([int]$l['ttl_seconds'])
  $new['ssh'] = [ordered]@{ host_alias = ('agentspace-' + $l['project'] + '-' + ([string]$l['id']).Substring(0, 8)); user = 'ubuntu' }
  $new['_gets'] = $l['_gets']
  return $new
}
function Get-SlPublic($l) {
  $o = [ordered]@{}
  foreach ($k in $l.Keys) { if (-not ([string]$k).StartsWith('_')) { $o[$k] = $l[$k] } }
  return $o
}
function Write-SlDoc($Doc, [switch]$Pretty) {
  $json = if ($Pretty) { ConvertTo-Json -InputObject $Doc -Depth 8 } else { ConvertTo-Json -InputObject $Doc -Depth 8 -Compress }
  [Console]::Out.Write($json + "`n")
  [Console]::Out.Flush()
}
function Write-SlError([string]$Code, [string]$Message) {
  [Console]::Error.Write((ConvertTo-Json -InputObject ([ordered]@{ code = $Code; message = $Message }) -Compress) + "`n")
}

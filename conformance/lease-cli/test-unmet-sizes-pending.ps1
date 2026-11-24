#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.08 programs, which know no size classes and no host
# `instance_type`: a `create --size l` is refused there as `unsupported_size`, and a `ready` record is
# never held `pending`. A v0.09 build MUST pass every result line (docs/conformance.md, "The files that
# are expected to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what
# docs/lease-api.md, docs/driver.md and docs/state.md document, with a stand-in for the provisioning run
# (a program outside agentspace). See docs/conformance.md.
#
# Cases of E0.09-T04-S01 (card #0205), the rows of docs/driver.md, "Conformance cases for v0.09", in which
# a `create` or a `get` answers `pending` by the host's size, or answers by an implied or a missing type:
#   Z5   A record ready at one size, no live ready lease, a size it does not meet: pending, the stand-in
#        started once with exactly `provision -Host <project>`, the record gaining last_attempt_at only.
#   Z8   A record ready with no instance_type, or one the map does not hold: pending, whatever other
#        leases hold the host, with the stand-in started when a run is due.
#   Z9   Place's side of a run's deploy step: with the project's provision.lock held, a record with no
#        instance_type answers pending and starts nothing; one at the host's size answers ready.
#   Z10  A record whose last_attempt_at is under 5 minutes old: pending, nothing started, the registry
#        as it was; a `get` once the time is 5 minutes old starts the stand-in once.
#   Z12  The box, a record ready at m9g.xlarge: the default size answers ready with v0.06's alias bytes
#        and starts nothing; `--size l` answers pending and starts the stand-in once.
#   Z14  While the project has a pending lease of the EC2 driver at a size in SIZES, every new lease of it
#        is pending (pin P2), the stand-in started by the 5-minute rule (pin P27); a pending record that
#        a run would not place holds nothing (pin P20).
#   Z15  With no hosts.json, the box's implied record has instance_type m9g.xlarge (pin P2c).
#   Z16  The implied type is never written (pin P31).
# `place_pending`'s placed, refused and left outcomes are not a lease CLI row: driver.md, "What no stand-in
# for the lease CLI shows", gives them to the driver crate's tests and to the operator's cases of
# E0.09-T04-S02.
# Z1 to Z4, Z6, Z7, Z11 and Z13 are in test-unmet-sizes-place.ps1.
# Sections enforced: driver.md "On a ready host", "Where it places a lease", "Starting a provisioning run"
# and "check_pending"; state.md "The host registry", "The instance type" and "The alias file".
#
# - Each case writes its own hosts.json, projects.json, held provision.lock and lease records, in the
#   throwaway home and in the forms of docs/state.md and schemas/state: compact JSON, UTF-8 with no byte
#   order mark, one line feed. Each file it seeds is checked against its schema before the case runs, and
#   each write calls Assert-ConformancePath at the call. "The registry as it was" is hosts.json byte for
#   byte as the case wrote it.
# - Every `create` and `get` is given AGENTSPACE_PROVISION_CMD, the recording stand-in of
#   provision-standin.ps1 (pin P12), which makes no host. "Starts nothing" is shown by no start after a
#   wait of 3 seconds, as provision-standin.ps1 states. A time relative to now is computed from the
#   check's own clock, in the contract's form.
# - A stored lease record is the form of docs/state.md, "The lease state file". A stored ready record needs
#   no lease folder or alias, since place reads only the record (docs/driver.md, "On a ready host").
# - The program under test is untrusted (docs/conformance.md): a path built from the program's output is
#   built only after its document passed the schema and the patterns. No failure reason quotes a value the
#   stand-in captured. Helpers of this file's own are prefixed Sq.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-sizes-pending.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 6 minutes.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

$script:SqProject = 'sizehost'
$script:SqInstanceId = 'i-1234567890abcdef0'
$script:SqUtf8 = [Text.UTF8Encoding]::new($false)

# --- Files a case seeds --------------------------------------------------------------------------------

# One record of the host registry, members in the order of state.md's table: project, stack, state,
# instance_id, instance_type, last_attempt_at. The stack is the box's for swimcoach, and the project's
# own otherwise.
function New-SqRecord {
  param(
    [Parameter(Mandatory)][string]$Project, [ValidateSet('creating', 'ready', 'failed')][string]$State = 'ready',
    [string]$InstanceId, [string]$InstanceType, [string]$LastAttemptAt
  )
  $stack = $(if ($Project -ceq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" })
  $r = [ordered]@{ project = $Project; stack = $stack; state = $State }
  if ($InstanceId) { $r['instance_id'] = $InstanceId }
  if ($InstanceType) { $r['instance_type'] = $InstanceType }
  if ($LastAttemptAt) { $r['last_attempt_at'] = $LastAttemptAt }
  return $r
}

# The box's record, ready, with instance_type when one is given.
function New-SqBox { param([string]$InstanceType) New-SqRecord -Project 'swimcoach' -State 'ready' -InstanceType $InstanceType }

# The registry's text: compact, with one line feed.
function Get-SqHostsText {
  param([Parameter(Mandatory)]$Records)
  return '{"version":0,"hosts":[' + ((@($Records) | ForEach-Object { ConvertTo-Json -InputObject $_ -Compress -Depth 5 }) -join ',') + ']}' + "`n"
}

# Writes hosts.json from the records, after checking that the text validates against its schema.
function Set-SqHosts {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  $text = Get-SqHostsText $Records
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/hosts.state.schema.json')) { throw 'The seeded hosts.json does not validate against its schema.' }
  Write-PvText -Lab $Lab -Path (Get-PvHostsPath $Lab) -Text $text
}

# One lease record in state.md's form. A ready record has an expiry and an ssh member, as the schema
# requires; -ExpiresInSeconds is negative for one that has expired. A driver of '' is left out.
function New-SqLease {
  param(
    [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][ValidateSet('pending', 'ready', 'released', 'unsatisfiable')][string]$State,
    [string]$Size = 'm', [string]$Driver = 'ec2', [int]$ExpiresInSeconds = 3600
  )
  $id = [guid]::NewGuid().ToString()
  $now = Get-CcNowSecond
  $r = [ordered]@{
    id = $id; project = $Project; state = $State; platform = 'linux/arm64'; profile = 'agent-linux'; size = $Size
    ttl_seconds = 28800; created_at = (ConvertTo-CcTime $now.AddMinutes(-10))
  }
  switch ($State) {
    'ready' {
      $r['expires_at'] = ConvertTo-CcTime $now.AddSeconds($ExpiresInSeconds)
      $r['ssh'] = [ordered]@{ host_alias = "agentspace-$Project-$($id.Substring(0, 8))"; user = 'ubuntu' }
    }
    'released' { $r['released_at'] = ConvertTo-CcTime $now.AddMinutes(-5) }
    'unsatisfiable' { $r['reasons'] = @([ordered]@{ code = 'no_capacity'; message = 'No capacity can be found for the seeded lease.' }) }
  }
  if ($Driver) { $r['driver'] = $Driver }
  return $r
}

# Writes leases.json from the records, after checking that the file validates against its schema.
function Set-SqLeases {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  Write-CcState $Lab ([ordered]@{ version = 0; leases = @($Records) })
  if (-not (Test-CcStateFile $Lab)) { throw 'The seeded lease state file does not validate against its schema.' }
}

# --- Running the lease CLI -----------------------------------------------------------------------------

# `agentspace lease create --project <project>`, with --size when one is given, and the stand-in named.
function Invoke-SqCreate {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project, [string]$Size, [Parameter(Mandatory)]$StandIn)
  $a = @('create', '--project', $Project)
  if ($Size) { $a += @('--size', $Size) }
  return (Invoke-PvLease -Lab $Lab -Arguments $a -StandIn $StandIn)
}

# --- Assertions ----------------------------------------------------------------------------------------

# The problems with a run that printed a pending lease of the project: Get-PvPendingDocProblems' (the
# success test, no expires_at, released_at, ssh or reasons), the size asked for, and no instance type.
function Get-SqPendingDocProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Size)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-PvPendingDocProblems -Run $Run -Project $Project)) { $p.Add($x) }
  if ($Run.Json -is [hashtable]) {
    if ($Run.Json['size'] -cne $Size) { $p.Add("The document's size is '$($Run.Json['size'])', not '$Size'") }
    if ($Run.Stdout -match 'm9g') { $p.Add('The document names an instance type') }
  }
  return , $p
}

# The problems with a run that printed a ready lease of the project at the size asked for, with an expiry
# and an ssh member, and no instance type.
function Get-SqReadyDocProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Size)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $Run)) { $p.Add($x) }
  $d = $Run.Json
  if ($d -is [hashtable]) {
    if ($d['state'] -cne 'ready') { $p.Add("The lease's state is '$($d['state'])', not ready") }
    if ($d['project'] -cne $Project) { $p.Add('The lease is for another project') }
    if ($d['size'] -cne $Size) { $p.Add("The document's size is '$($d['size'])', not '$Size'") }
    if (-not $d.ContainsKey('expires_at')) { $p.Add('A ready lease carries no expires_at') }
    if (-not ($d['ssh'] -is [hashtable] -and $d['ssh'].ContainsKey('host_alias'))) { $p.Add('A ready lease carries no ssh host_alias') }
    foreach ($m in 'released_at', 'reasons') { if ($d.ContainsKey($m)) { $p.Add("A ready lease carries $m") } }
    if ($Run.Stdout -match 'm9g') { $p.Add('The document names an instance type') }
  }
  return , $p
}

# The problems with the lease the run answered, as stored in the lease state file: the state, the size, and
# (for a pending one) a driver.
function Get-SqStoredProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$State, [Parameter(Mandatory)][string]$Size)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.Json -isnot [hashtable] -or [string]$Run.Json['id'] -cnotmatch '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z') { $p.Add('There is no lease ID to look up'); return , $p }
  $rec = Get-CcRecord $Lab ([string]$Run.Json['id'])
  if ($null -eq $rec) { $p.Add('The lease is not in the lease state file'); return , $p }
  if ($rec['state'] -cne $State) { $p.Add("The stored lease is '$($rec['state'])', not $State") }
  if ($rec['size'] -cne $Size) { $p.Add("The stored lease's size is '$($rec['size'])', not '$Size'") }
  if ($State -ceq 'pending' -and [string]::IsNullOrEmpty([string]$rec['driver'])) { $p.Add('The stored pending lease carries no driver') }
  if (-not (Test-CcStateFile $Lab)) { $p.Add('The lease state file does not validate against its schema') }
  return , $p
}

# The problems with a stored record that is not byte for byte, by its canonical text, what the case wrote.
function Get-SqRecordAsWasProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$Canonical, [Parameter(Mandatory)][string]$What)
  $p = [System.Collections.Generic.List[string]]::new()
  $rec = Get-CcRecord $Lab $Id
  if ($null -eq $rec) { $p.Add("The $What record is gone from the lease state file") }
  elseif ((ConvertTo-CcCanonical $rec) -cne $Canonical) { $p.Add("The $What record changed") }
  return , $p
}

# The problems with the stand-in's arguments naming an instance type: its started.log line holds none.
function Get-SqNoTypeArgProblems {
  param([Parameter(Mandatory)]$StandIn)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($l in @(Get-PvStartedLines $StandIn)) { if ($l -match '(?i)InstanceType') { $p.Add('The stand-in was given an -InstanceType') } }
  return , $p
}

# The problems with hosts.json after a run that was due: the registry is the seed's records, the project's
# with last_attempt_at added, within the command's run, and nothing else changed, byte for byte.
function Get-SqGainProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Seed, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][DateTime]$Before, [Parameter(Mandatory)][DateTime]$After)
  $p = [System.Collections.Generic.List[string]]::new()
  $rec = Get-PvHostRecord $Lab $Project
  if ($null -eq $rec) { $p.Add('hosts.json holds no record for the project'); return , $p }
  $time = [string]$rec['last_attempt_at']
  if (-not (Test-PvTimeBetween -Time $time -Before $Before -After $After)) { $p.Add('last_attempt_at is not within the command''s run') }
  $want = foreach ($r in @($Seed)) {
    if ($r['project'] -ceq $Project) {
      $c = [ordered]@{}
      foreach ($k in $r.Keys) { $c[$k] = $r[$k] }
      $c['last_attempt_at'] = $time
      $c
    } else { $r }
  }
  foreach ($x in (Get-PvHostsTextProblems -Lab $Lab -Expected (Get-SqHostsText @($want)))) { $p.Add($x) }
  return , $p
}

# The problems with hosts.json not being byte for byte what it was before the run.
function Get-SqRegistryAsWasProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Hash)
  $p = [System.Collections.Generic.List[string]]::new()
  if ((Get-PvHostsHash $Lab) -cne $Hash) { $p.Add('hosts.json is not as it was') }
  return , $p
}

# --- The box's alias bytes (state.md, "The alias file": v0.06's) -----------------------------------------

# The lines of the box's alias file after its owner mark, exactly as state.md gives them, with the trailing
# line feed. Every path is built as the program is told to write it: the home as text, `\` as `/` and `%`
# as `%%`.
function Get-SqExpectedBoxAliasRest {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$Alias)
  $leaseDir = Get-CcLeaseDir $Lab $Id
  $key = (Join-Path $leaseDir 'id_ed25519').Replace('\', '/').Replace('%', '%%')
  $cert = (Join-Path $leaseDir 'id_ed25519-cert.pub').Replace('\', '/').Replace('%', '%%')
  $lines = @("Host $Alias", '  User ubuntu', "  IdentityFile `"$key`"", "  CertificateFile `"$cert`"", '  IdentitiesOnly yes', '  HostKeyAlias agentspace',
    '  StrictHostKeyChecking yes', "  ProxyCommand $($script:CcProxy.Replace('%', '%%'))", '  ServerAliveInterval 30', '  ServerAliveCountMax 4',
    '  RemoteForward 127.0.0.1:8811 127.0.0.1:8811', '  RemoteForward 127.0.0.1:53147 127.0.0.1:53147')
  return (($lines -join "`n") + "`n")
}

# The problems with the box's alias file for a ready lease document: it exists, is UTF-8 with no byte order
# mark and no CR, starts with the owner mark naming the lease, and every other byte is v0.06's, with no
# UserKnownHostsFile. Never quotes the file.
function Get-SqBoxAliasProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run)
  $p = [System.Collections.Generic.List[string]]::new()
  $d = $Run.Json
  if ($d -isnot [hashtable] -or $d['state'] -cne 'ready') { $p.Add('There is no ready document to read the alias from'); return , $p }
  try {
    $id = [string]$d['id']
    $alias = [string]$d['ssh']['host_alias']
    $path = Get-CcAliasPath $Lab $alias
    $want = $script:SqUtf8.GetBytes((Get-SqExpectedBoxAliasRest $Lab $id $alias))
  } catch { $p.Add('The document''s lease ID or alias does not match its pattern'); return , $p }
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { $p.Add('There is no alias file by the document''s alias'); return , $p }
  $bytes = [IO.File]::ReadAllBytes($path)
  if (Get-ConformanceHasBom $bytes) { $p.Add('The alias file starts with a byte order mark') }
  if ($bytes -contains 13) { $p.Add('The alias file holds a CR') }
  $nl = [Array]::IndexOf($bytes, [byte]10)
  if ($nl -lt 0) { $p.Add('The alias file holds no LF'); return , $p }
  $first = $script:SqUtf8.GetString($bytes, 0, $nl)
  if ($first -cnotmatch "\A# agentspace lease $([regex]::Escape($id)); [^\r\n]+\z") { $p.Add('The first line is not the owner mark naming the lease, a semicolon and a comment') }
  $rest = [byte[]]$bytes[($nl + 1)..($bytes.Length - 1)]
  if (-not [Linq.Enumerable]::SequenceEqual($rest, $want)) {
    $gl = @($script:SqUtf8.GetString($rest) -split "`n")
    $wl = @($script:SqUtf8.GetString($want) -split "`n")
    $at = 0
    while ($at -lt $gl.Count -and $at -lt $wl.Count -and $gl[$at] -ceq $wl[$at]) { $at++ }
    $p.Add("The lines after the owner mark are not v0.06's, first differing at its line $($at + 2)")
  }
  if ($script:SqUtf8.GetString($bytes) -cmatch '(?m)^\s*UserKnownHostsFile\b') { $p.Add('The box''s alias names a UserKnownHostsFile') }
  return , $p
}

$onReady = 'driver.md#on-a-ready-host'
$starting = 'driver.md#starting-a-provisioning-run'
$where = 'driver.md#where-it-places-a-lease'
$typeLarge = 'm9g.2xlarge'
$typeMid = 'm9g.xlarge'
$typeXl = 'm9g.4xlarge'

# ---- Z5 -----------------------------------------------------------------------------------------------
function Test-SqZ5 {
  param([string]$Label, [string]$Type, [string]$Size)
  $sub = @{ Name = "Z5 ($onReady, row 6; $starting): $Label"; Type = $Type; Size = $Size }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    $seed = @((New-SqBox $typeMid), (New-SqRecord -Project $script:SqProject -InstanceId $script:SqInstanceId -InstanceType $sub.Type))
    Set-SqHosts $lab $seed
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    $before = Get-CcNowSecond
    $run = Invoke-SqCreate -Lab $lab -Project $script:SqProject -Size $sub.Size -StandIn $standIn
    $after = Get-CcNowSecond
    $p = Get-SqPendingDocProblems -Run $run -Project $script:SqProject -Size $sub.Size
    foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
    foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'pending' -Size $sub.Size)) { $p.Add($x) }
    foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project $script:SqProject)) { $p.Add($x) }
    foreach ($x in (Get-SqNoTypeArgProblems -StandIn $standIn)) { $p.Add($x) }
    foreach ($x in (Get-SqGainProblems -Lab $lab -Seed $seed -Project $script:SqProject -Before $before -After $after)) { $p.Add($x) }
    $rec = Get-PvHostRecord $lab $script:SqProject
    if ($null -ne $rec) {
      if ($rec['state'] -cne 'ready') { $p.Add('The record''s state is not ready') }
      if ($rec['instance_type'] -cne $sub.Type) { $p.Add('The record''s instance_type changed') }
      if ($rec['instance_id'] -cne $script:SqInstanceId) { $p.Add('The record''s instance_id changed') }
    }
    return , $p
  }
}
Test-SqZ5 -Label 'With no live ready lease and a record ready at m9g.xlarge whose last_attempt_at is absent, create --size l answers pending with no expires_at, ssh or reasons and no lease folder or alias file, starts the stand-in once with exactly provision -Host <project>, and leaves the record ready with its instance_type and instance_id, gaining last_attempt_at within the run' -Type $typeMid -Size 'l'
Test-SqZ5 -Label 'The same for a record at m9g.4xlarge and create --size m, the host to be made smaller' -Type $typeXl -Size 'm'

# ---- Z8 -----------------------------------------------------------------------------------------------
function Test-SqZ8 {
  param([string]$Label, [string]$Type, [bool]$Live, [string]$Size)
  $sub = @{ Name = "Z8 ($onReady, row 2): $Label"; Type = $Type; Live = $Live; Size = $Size }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    $seed = @((New-SqBox $typeMid), (New-SqRecord -Project $script:SqProject -InstanceId $script:SqInstanceId -InstanceType $sub.Type))
    Set-SqHosts $lab $seed
    $liveId = ''
    $liveCanon = ''
    if ($sub.Live) {
      $liveLease = New-SqLease -Project $script:SqProject -State 'ready' -Size 'm'
      Set-SqLeases $lab @($liveLease)
      $liveId = [string]$liveLease['id']
      $liveCanon = ConvertTo-CcCanonical (Get-CcRecord $lab $liveId)
    }
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    $askFor = $(if ($sub.Size) { $sub.Size } else { 'm' })
    $before = Get-CcNowSecond
    $run = Invoke-SqCreate -Lab $lab -Project $script:SqProject -Size $sub.Size -StandIn $standIn
    $after = Get-CcNowSecond
    $p = Get-SqPendingDocProblems -Run $run -Project $script:SqProject -Size $askFor
    foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
    foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'pending' -Size $askFor)) { $p.Add($x) }
    foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project $script:SqProject)) { $p.Add($x) }
    foreach ($x in (Get-SqNoTypeArgProblems -StandIn $standIn)) { $p.Add($x) }
    foreach ($x in (Get-SqGainProblems -Lab $lab -Seed $seed -Project $script:SqProject -Before $before -After $after)) { $p.Add($x) }
    $rec = Get-PvHostRecord $lab $script:SqProject
    if ($null -ne $rec) {
      if ($rec['state'] -cne 'ready') { $p.Add('The record''s state is not ready') }
      if ($sub.Type) { if ($rec['instance_type'] -cne $sub.Type) { $p.Add('The record''s instance_type changed') } }
      elseif ($rec.ContainsKey('instance_type')) { $p.Add('The record gained an instance_type') }
    }
    if ($sub.Live) { foreach ($x in (Get-SqRecordAsWasProblems -Lab $lab -Id $liveId -Canonical $liveCanon -What 'live ready lease')) { $p.Add($x) } }
    return , $p
  }
}
foreach ($t in @(@{ Type = ''; Text = 'with no instance_type' }, @{ Type = 'm9g.8xlarge'; Text = 'at m9g.8xlarge, a type the map does not hold' })) {
  Test-SqZ8 -Label "A record ready $($t.Text), no live ready lease: create with the default size answers pending, never ready or unsatisfiable, starts the stand-in once, and the record stays ready, keeping its type or none" -Type $t.Type -Live $false -Size ''
  Test-SqZ8 -Label "A record ready $($t.Text), with a live ready lease of the project: create with the default size answers pending, never unsatisfiable, starts the stand-in once, and leaves the live lease as it was" -Type $t.Type -Live $true -Size ''
  Test-SqZ8 -Label "A record ready $($t.Text), with a live ready lease of the project: create --size s answers pending, never unsatisfiable, starts the stand-in once, and leaves the live lease as it was" -Type $t.Type -Live $true -Size 's'
}

# ---- Z9 -----------------------------------------------------------------------------------------------
function Test-SqZ9 {
  param([string]$Label, [string]$Type, [string]$Answer)
  $sub = @{ Name = "Z9 ($onReady, rows 2 and 3; driver.md#a-resizing-run): $Label"; Type = $Type; Answer = $Answer }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    $seed = @((New-SqBox $typeMid), (New-SqRecord -Project $script:SqProject -InstanceId $script:SqInstanceId -InstanceType $sub.Type))
    Set-SqHosts $lab $seed
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $lock = Open-PvLock -Lab $lab -Path (Get-PvProvisionLockPath $lab $script:SqProject)
    try {
      $hash = Get-PvHostsHash $lab
      $run = Invoke-SqCreate -Lab $lab -Project $script:SqProject -StandIn $standIn
      if ($sub.Answer -ceq 'pending') {
        $p = Get-SqPendingDocProblems -Run $run -Project $script:SqProject -Size 'm'
        foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
        foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'pending' -Size 'm')) { $p.Add($x) }
      } else {
        $p = Get-SqReadyDocProblems -Run $run -Project $script:SqProject -Size 'm'
        foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'ready' -Size 'm')) { $p.Add($x) }
      }
      foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
      foreach ($x in (Get-SqRegistryAsWasProblems -Lab $lab -Hash $hash)) { $p.Add($x) }
    } finally { try { $lock.Dispose() } catch { } }
    return , $p
  }
}
Test-SqZ9 -Label 'With the project''s provision.lock held and no live ready lease, a record ready with no instance_type, as a run''s deploy step leaves it: create with the default size answers pending, starts nothing, since no run is due while the lock is held, and leaves the registry as it was' -Type '' -Answer 'pending'
Test-SqZ9 -Label 'With the project''s provision.lock held and no live ready lease, a record ready at m9g.xlarge, as the read-back leaves it: create with the default size answers ready, starts nothing and leaves the registry as it was' -Type $typeMid -Answer 'ready'

# ---- Z10 ----------------------------------------------------------------------------------------------
Use-PvLab -Name "Z10 ($starting, steps 2 and 4; driver.md#check_pending): With a record ready at m9g.xlarge whose last_attempt_at is under 5 minutes old, create --size l answers pending, starts nothing and leaves the registry as it was; a get of that lease once last_attempt_at is 5 minutes or more old starts the stand-in once with exactly provision -Host <project>" -Body {
  param($lab)
  $seed = @((New-SqBox $typeMid), (New-SqRecord -Project $script:SqProject -InstanceId $script:SqInstanceId -InstanceType $typeMid -LastAttemptAt (Get-PvAgo 120)))
  Set-SqHosts $lab $seed
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $hash = Get-PvHostsHash $lab
  $run = Invoke-SqCreate -Lab $lab -Project $script:SqProject -Size 'l' -StandIn $standIn
  $p = Get-SqPendingDocProblems -Run $run -Project $script:SqProject -Size 'l'
  foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
  foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'pending' -Size 'l')) { $p.Add($x) }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  foreach ($x in (Get-SqRegistryAsWasProblems -Lab $lab -Hash $hash)) { $p.Add($x) }
  if ($run.Json -isnot [hashtable]) { return , $p }
  # The check sets last_attempt_at 6 minutes before now, and a get of the lease asks check_pending.
  $seed = @((New-SqBox $typeMid), (New-SqRecord -Project $script:SqProject -InstanceId $script:SqInstanceId -InstanceType $typeMid -LastAttemptAt (Get-PvAgo 360)))
  Set-SqHosts $lab $seed
  $before = Get-CcNowSecond
  $get = Invoke-PvLease -Lab $lab -Arguments @('get', [string]$run.Json['id']) -StandIn $standIn
  $after = Get-CcNowSecond
  foreach ($x in (Get-SqPendingDocProblems -Run $get -Project $script:SqProject -Size 'l')) { $p.Add("get: $x") }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project $script:SqProject)) { $p.Add($x) }
  foreach ($x in (Get-SqNoTypeArgProblems -StandIn $standIn)) { $p.Add($x) }
  foreach ($x in (Get-SqGainProblems -Lab $lab -Seed $seed -Project $script:SqProject -Before $before -After $after)) { $p.Add("get: $x") }
  return , $p
}

# ---- Z12 ----------------------------------------------------------------------------------------------
Use-PvLab -Name "Z12 ($onReady, row 3; driver.md#what-is-true-of-todays-box): With hosts.json holding the box's record ready at m9g.xlarge, a create for swimcoach with the default size and no other lease answers ready with v0.06's alias bytes, starts nothing and leaves the registry as it was" -Body {
  param($lab)
  Set-SqHosts $lab @((New-SqBox $typeMid))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $hash = Get-PvHostsHash $lab
  $run = Invoke-SqCreate -Lab $lab -Project 'swimcoach' -StandIn $standIn
  $p = Get-SqReadyDocProblems -Run $run -Project 'swimcoach' -Size 'm'
  foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'ready' -Size 'm')) { $p.Add($x) }
  foreach ($x in (Get-SqBoxAliasProblems -Lab $lab -Run $run)) { $p.Add($x) }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  foreach ($x in (Get-SqRegistryAsWasProblems -Lab $lab -Hash $hash)) { $p.Add($x) }
  return , $p
}
Use-PvLab -Name "Z12 ($onReady, row 6; $starting): With the box's record ready at m9g.xlarge, a create for swimcoach with --size l answers pending and starts the stand-in once with exactly provision -Host swimcoach, the record gaining last_attempt_at only" -Body {
  param($lab)
  $seed = @((New-SqBox $typeMid))
  Set-SqHosts $lab $seed
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $before = Get-CcNowSecond
  $run = Invoke-SqCreate -Lab $lab -Project 'swimcoach' -Size 'l' -StandIn $standIn
  $after = Get-CcNowSecond
  $p = Get-SqPendingDocProblems -Run $run -Project 'swimcoach' -Size 'l'
  foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
  foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'pending' -Size 'l')) { $p.Add($x) }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'swimcoach')) { $p.Add($x) }
  foreach ($x in (Get-SqNoTypeArgProblems -StandIn $standIn)) { $p.Add($x) }
  foreach ($x in (Get-SqGainProblems -Lab $lab -Seed $seed -Project 'swimcoach' -Before $before -After $after)) { $p.Add($x) }
  return , $p
}

# ---- Z14 ----------------------------------------------------------------------------------------------
function Test-SqZ14 {
  param([string]$Attempt, [bool]$Live, [string]$Size)
  $when = switch ($Attempt) { 'absent' { 'whose last_attempt_at is absent' } 'old' { 'whose last_attempt_at is 6 minutes old' } default { 'whose last_attempt_at is 1 minute old' } }
  $startText = $(if ($Attempt -ceq 'recent') { 'starts nothing and leaves the registry as it was' } else { 'starts the stand-in once and sets last_attempt_at to now, the record staying ready' })
  $sizeText = $(if ($Size) { "create --size $Size" } else { 'create with the default size' })
  $liveText = $(if ($Live) { 'with' } else { 'without' })
  $sub = @{ Name = "Z14 ($onReady, row 1; pin P27): With a record ready at m9g.xlarge $when and a stored pending lease of the project at l, $liveText a live ready lease of the project, $sizeText answers pending, leaves the pending record as it was and $startText"; Attempt = $Attempt; Live = $Live; Size = $Size }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    $last = switch ($sub.Attempt) { 'absent' { '' } 'old' { Get-PvAgo 360 } default { Get-PvAgo 60 } }
    $seed = @((New-SqBox $typeMid), (New-SqRecord -Project $script:SqProject -InstanceId $script:SqInstanceId -InstanceType $typeMid -LastAttemptAt $last))
    Set-SqHosts $lab $seed
    $pending = New-SqLease -Project $script:SqProject -State 'pending' -Size 'l' -Driver 'ec2'
    $records = @($pending)
    if ($sub.Live) { $records += (New-SqLease -Project $script:SqProject -State 'ready' -Size 'm') }
    Set-SqLeases $lab $records
    $pendingId = [string]$pending['id']
    $pendingCanon = ConvertTo-CcCanonical (Get-CcRecord $lab $pendingId)
    $liveId = ''
    $liveCanon = ''
    if ($sub.Live) {
      $liveId = [string]$records[1]['id']
      $liveCanon = ConvertTo-CcCanonical (Get-CcRecord $lab $liveId)
    }
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    $hash = Get-PvHostsHash $lab
    $askFor = $(if ($sub.Size) { $sub.Size } else { 'm' })
    $before = Get-CcNowSecond
    $run = Invoke-SqCreate -Lab $lab -Project $script:SqProject -Size $sub.Size -StandIn $standIn
    $after = Get-CcNowSecond
    $p = Get-SqPendingDocProblems -Run $run -Project $script:SqProject -Size $askFor
    foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
    foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'pending' -Size $askFor)) { $p.Add($x) }
    foreach ($x in (Get-SqRecordAsWasProblems -Lab $lab -Id $pendingId -Canonical $pendingCanon -What 'pending')) { $p.Add($x) }
    if ($sub.Live) { foreach ($x in (Get-SqRecordAsWasProblems -Lab $lab -Id $liveId -Canonical $liveCanon -What 'live ready lease')) { $p.Add($x) } }
    if ($sub.Attempt -ceq 'recent') {
      foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
      foreach ($x in (Get-SqRegistryAsWasProblems -Lab $lab -Hash $hash)) { $p.Add($x) }
    } else {
      foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project $script:SqProject)) { $p.Add($x) }
      foreach ($x in (Get-SqNoTypeArgProblems -StandIn $standIn)) { $p.Add($x) }
      foreach ($x in (Get-SqGainProblems -Lab $lab -Seed $seed -Project $script:SqProject -Before $before -After $after)) { $p.Add($x) }
    }
    return , $p
  }
}
foreach ($attempt in 'absent', 'old', 'recent') {
  foreach ($live in $false, $true) {
    foreach ($size in '', 's') { Test-SqZ14 -Attempt $attempt -Live $live -Size $size }
  }
}

# A stored pending record of the project that a run would not place does not hold the project's leases.
function Test-SqZ14Other {
  param([string]$Label, [string]$Size, [string]$Driver)
  $sub = @{ Name = "Z14 ($onReady, row 1; pin P20): $Label"; Size = $Size; Driver = $Driver }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    Set-SqHosts $lab @((New-SqBox $typeMid), (New-SqRecord -Project $script:SqProject -InstanceId $script:SqInstanceId -InstanceType $typeMid))
    $pending = New-SqLease -Project $script:SqProject -State 'pending' -Size $sub.Size -Driver $sub.Driver
    Set-SqLeases $lab @($pending)
    $pendingId = [string]$pending['id']
    $pendingCanon = ConvertTo-CcCanonical (Get-CcRecord $lab $pendingId)
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $hash = Get-PvHostsHash $lab
    $run = Invoke-SqCreate -Lab $lab -Project $script:SqProject -StandIn $standIn
    $p = Get-SqReadyDocProblems -Run $run -Project $script:SqProject -Size 'm'
    foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'ready' -Size 'm')) { $p.Add($x) }
    foreach ($x in (Get-SqRecordAsWasProblems -Lab $lab -Id $pendingId -Canonical $pendingCanon -What 'pending')) { $p.Add($x) }
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
    foreach ($x in (Get-SqRegistryAsWasProblems -Lab $lab -Hash $hash)) { $p.Add($x) }
    return , $p
  }
}
Test-SqZ14Other -Label 'A stored pending record of the project with a size outside SIZES (xxl) and the EC2 driver''s name, alone: create with the default size answers ready, starts nothing and leaves the pending record as it was' -Size 'xxl' -Driver 'ec2'
Test-SqZ14Other -Label 'A stored pending record of the project at l with another driver, alone: create with the default size answers ready, starts nothing and leaves the pending record as it was' -Size 'l' -Driver 'zq-conformance-driver-x9'
Test-SqZ14Other -Label 'A stored pending record of the project at l with no driver, alone: create with the default size answers ready, starts nothing and leaves the pending record as it was' -Size 'l' -Driver ''

# ---- Z15 ----------------------------------------------------------------------------------------------
Use-PvLab -Name "Z15 ($where, pin P2c): With no hosts.json, a create for swimcoach with the default size answers ready with v0.06's alias bytes, starts nothing and leaves hosts.json absent" -Body {
  param($lab)
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $run = Invoke-SqCreate -Lab $lab -Project 'swimcoach' -StandIn $standIn
  $p = Get-SqReadyDocProblems -Run $run -Project 'swimcoach' -Size 'm'
  foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'ready' -Size 'm')) { $p.Add($x) }
  foreach ($x in (Get-SqBoxAliasProblems -Lab $lab -Run $run)) { $p.Add($x) }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  if (Test-Path -LiteralPath (Get-PvHostsPath $lab)) { $p.Add('hosts.json was made') }
  return , $p
}
Use-PvLab -Name "Z15 ($where, pin P2c): With no hosts.json, a create for swimcoach with --size l answers pending and starts the stand-in once with exactly provision -Host swimcoach" -Body {
  param($lab)
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $run = Invoke-SqCreate -Lab $lab -Project 'swimcoach' -Size 'l' -StandIn $standIn
  $p = Get-SqPendingDocProblems -Run $run -Project 'swimcoach' -Size 'l'
  foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
  foreach ($x in (Get-SqStoredProblems -Lab $lab -Run $run -State 'pending' -Size 'l')) { $p.Add($x) }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'swimcoach')) { $p.Add($x) }
  foreach ($x in (Get-SqNoTypeArgProblems -StandIn $standIn)) { $p.Add($x) }
  return , $p
}

# ---- Z16 ----------------------------------------------------------------------------------------------
Use-PvLab -Name "Z16 ($where, pin P31; $starting, step 4): With no hosts.json and a projects.json listing the project, a create with the default size answers pending and starts the stand-in once, leaving a hosts.json that validates with the box's record first, with no instance_type, beside the project's new creating record; then a create for swimcoach with the default size answers pending by row 2, makes no lease folder or alias file, starts the stand-in once more with exactly provision -Host swimcoach, and leaves the box's record ready with no instance_type" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @($script:SqProject)
  $first = New-PvStandIn -Lab $lab -Name 'run1'
  $second = New-PvStandIn -Lab $lab -Name 'run2'
  $before = Get-CcNowSecond
  $run = Invoke-SqCreate -Lab $lab -Project $script:SqProject -StandIn $first
  $after = Get-CcNowSecond
  $p = Get-SqPendingDocProblems -Run $run -Project $script:SqProject -Size 'm'
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $first -Project $script:SqProject)) { $p.Add("first create: $x") }
  foreach ($x in (Get-PvHostsFileProblems -Lab $lab)) { $p.Add("first create: $x") }
  $rec = Get-PvHostRecord $lab $script:SqProject
  $t1 = ''
  if ($null -eq $rec) { $p.Add('first create: hosts.json holds no record for the project') }
  else {
    $t1 = [string]$rec['last_attempt_at']
    if (-not (Test-PvTimeBetween -Time $t1 -Before $before -After $after)) { $p.Add('first create: last_attempt_at is not within the command''s run') }
    $want = New-PvHostsText -Records @((New-PvHostRecord -Project $script:SqProject -State 'creating' -LastAttemptAt $t1))
    foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected $want)) { $p.Add("first create: $x") }
  }
  $box = Get-PvHostRecord $lab 'swimcoach'
  if ($null -eq $box) { $p.Add('first create: hosts.json holds no record for the box') }
  elseif ($box['stack'] -cne 'agentbox' -or $box['state'] -cne 'ready' -or $box.ContainsKey('instance_type')) { $p.Add('first create: the box''s record is not the seed''s, with no instance_type') }
  $before = Get-CcNowSecond
  $swim = Invoke-SqCreate -Lab $lab -Project 'swimcoach' -StandIn $second
  $after = Get-CcNowSecond
  foreach ($x in (Get-SqPendingDocProblems -Run $swim -Project 'swimcoach' -Size 'm')) { $p.Add("swimcoach create: $x") }
  foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $second -Project 'swimcoach')) { $p.Add("swimcoach create: $x") }
  foreach ($x in (Get-SqNoTypeArgProblems -StandIn $second)) { $p.Add("swimcoach create: $x") }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $first -Project $script:SqProject)) { $p.Add("The first stand-in: $x") }
  $box = Get-PvHostRecord $lab 'swimcoach'
  if ($null -eq $box) { $p.Add('swimcoach create: hosts.json holds no record for the box') }
  else {
    $t2 = [string]$box['last_attempt_at']
    if (-not (Test-PvTimeBetween -Time $t2 -Before $before -After $after)) { $p.Add('swimcoach create: the box''s last_attempt_at is not within the command''s run') }
    $want = New-PvHostsText -NoBox -Records @((New-PvHostRecord -Project 'swimcoach' -State 'ready' -Stack 'agentbox' -LastAttemptAt $t2), (New-PvHostRecord -Project $script:SqProject -State 'creating' -LastAttemptAt $t1))
    foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected $want)) { $p.Add("swimcoach create: $x") }
  }
  return , $p
}

Exit-ConformanceRun

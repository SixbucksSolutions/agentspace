#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what
# docs/lease-api.md, docs/driver.md and docs/state.md document, with a stand-in for the provisioning run (a
# program outside agentspace). See docs/conformance.md. A v0.09 build MUST pass every result line.
#
# Cases of E0.09-T04-S01 (card #0205), the size classes as place decides them for a project whose host is
# `ready`, each named by its row in docs/driver.md, "Conformance cases for v0.09" (D66, D67, D70, D71):
#   Z1   For each of s, m, l and xl, with no other lease of the project: a create for a record ready at that
#        size's type answers ready, with the size asked for and no type, starts nothing, and leaves the registry.
#   Z2   create --size xxl and --size XXL: unsatisfiable with exactly unsupported_size, its message byte for
#        byte, with no CA and a registry that does not validate as with a good one; with --platform windows/amd64
#        the reasons are unsupported_platform then unsupported_size, and with --size l the one is the platform's.
#        With a live ready lease on a host smaller than l, no_capacity is never listed beside them (lease-api.md,
#        "Fields").
#   Z3  With a live ready lease and a larger host, create --size m and --size s answer ready, with the size
#        asked for and the project's host's alias; so does a default create on a host at m.
#   Z4   With a live ready lease and a smaller host, create --size l and --size xl answer unsatisfiable with one
#        reason, no_capacity, D66's message byte for byte, and change nothing else.
#   Z6   A stored ready lease whose expires_at is past, a released one, an unsatisfiable one and a live ready one
#        of another project hold no host's size: create --size xl answers pending and starts the stand-in.
#   Z7   Expiry is decided by the reading's time: a lease made with --ttl 1m holds the host, and once its
#        expires_at has passed the same create answers pending and starts the stand-in.
#   Z11  With no CA, a create that would answer pending fails with internal and changes nothing, and one that
#        would answer unsatisfiable with no_capacity answers it.
#   Z13  A live ready lease under another AGENTSPACE_STATE_DIR does not hold the host.
#   L1   lease-api.md, "Fields": an unsatisfiable lease with no_capacity stays unsatisfiable, and a later create
#        is judged afresh (not a row of driver.md).
#   L2   lease-api.md, "Size": a size is normalized, in any case and with surrounding whitespace, before it is
#        judged (not a row of driver.md for the four sizes).
# Sections enforced: driver.md "On a ready host", "What it supports", "Sizes and instance types", "Where it
# places a lease" and "Starting a provisioning run"; lease-api.md "Fields", "Size", "States" and "A host's life
# follows its leases"; state.md "The host registry", "The lease state file" and "A lease alias for a host that is
# not the box".
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No check starts
# a real run: every create here is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records
# what it is given and makes no host. No program of agentspace's but the lease CLI is started, and it is started
# only through the harness. Every write calls Assert-ConformancePath at the call, through the helpers of
# provision-standin.ps1 and lease-cli-common.ps1, and through Write-CcState. A failure reason never quotes a
# value the stand-in captured. The one second state folder (Z13) is under the throwaway home's root, as the
# harness requires of an override.
#
# Each case writes the hosts.json, the lease records and the held state it needs, as compact JSON in UTF-8 with
# no byte order mark and one line feed, and validates each against its schema before it uses it. A record of the
# host registry is written by this file's own helpers (the prefix Sp), so that no helper of another file's is
# needed for a member that file does not write.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-sizes-place.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 5 minutes (Z7 waits for a one-minute lease to expire, and a stand-in that MUST NOT start is
# watched for 3 seconds).

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

# The EC2 driver's name, as state.md gives it (the lease state file). A stored record of the driver names it.
$script:SpDriver = 'ec2'
# A project of the cases' own, with a record, that is not swimcoach (driver.md, "Conformance cases for v0.09").
$script:SpProject = 'sizecase'
# The four sizes' types (driver.md, "Sizes and instance types"), written here as the cases' own data.
$script:SpType = @{ s = 'm9g.large'; m = 'm9g.xlarge'; l = 'm9g.2xlarge'; xl = 'm9g.4xlarge' }
# A made-up instance ID of the form the schema accepts, for a record that must keep its instance_id.
$script:SpInstanceId = 'i-1234567890abcdef0'
# The box's record, as a v0.09 deploy leaves it (state.md, "The host registry"), first in every registry written.
$script:SpBox = '{"project":"swimcoach","stack":"agentbox","state":"ready","instance_type":"m9g.xlarge"}'
# The message of a create with no CA (lease-api.md, "Messages").
$script:SpNoCaMessage = "agentspace lease create failed: agentspace's certificate authority has not been set up, so no certificate can be issued. Nothing was changed; the operator sets it up."

# --- What the cases write ------------------------------------------------------------------------------

# A time $Seconds from now (negative is before now), in the contract's form.
function Get-SpTime { param([double]$Seconds) ConvertTo-CcTime ([DateTime]::UtcNow.AddSeconds($Seconds)) }

# One host record in state.md's order: project, stack, state, instance_id, instance_type, last_attempt_at.
function New-SpHostRecord {
  param([string]$Project = $script:SpProject, [string]$State = 'ready', [string]$InstanceType, [string]$InstanceId, [string]$LastAttemptAt)
  $s = '{"project":"' + $Project + '","stack":"agentspace-host-' + $Project + '","state":"' + $State + '"'
  if ($InstanceId) { $s += ',"instance_id":"' + $InstanceId + '"' }
  if ($InstanceType) { $s += ',"instance_type":"' + $InstanceType + '"' }
  if ($LastAttemptAt) { $s += ',"last_attempt_at":"' + $LastAttemptAt + '"' }
  return $s + '}'
}

# The host registry's text: the box's record, then the records given, compact, with one line feed.
function New-SpHostsText {
  param([string[]]$Records = @())
  return '{"version":0,"hosts":[' + ((@($script:SpBox) + @($Records)) -join ',') + ']}' + "`n"
}

# Writes hosts.json and returns its text. A registry that does not validate is the case's mistake, so it throws.
function Set-SpHosts {
  param([Parameter(Mandatory)]$Lab, [string[]]$Records = @())
  $text = New-SpHostsText -Records $Records
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/hosts.state.schema.json')) { throw 'The case wrote a hosts.json that does not validate against its schema.' }
  Write-PvText -Lab $Lab -Path (Get-PvHostsPath $Lab) -Text $text
  return $text
}

# One stored lease record (state.md, "The lease state file"), in the form of its state. A ready record has an
# alias in the project's form, which no file backs: place reads only the record.
function New-SpLeaseRecord {
  param(
    [Parameter(Mandatory)][ValidateSet('pending', 'ready', 'released', 'unsatisfiable')][string]$State,
    [string]$Project = $script:SpProject, [string]$Size = 'm', [int]$CreatedAgo = 60, [string]$ExpiresAt, [string]$ReleasedAt
  )
  $id = [guid]::NewGuid().ToString()
  $r = [ordered]@{
    id = $id; project = $Project; state = $State; platform = 'linux/arm64'; profile = 'agent-linux'
    size = $Size; ttl_seconds = 3600; created_at = (Get-SpTime (-$CreatedAgo))
  }
  if ($ExpiresAt) { $r['expires_at'] = $ExpiresAt }
  if ($ReleasedAt) { $r['released_at'] = $ReleasedAt }
  if ($State -ceq 'ready') { $r['ssh'] = [ordered]@{ host_alias = "agentspace-$Project-$($id.Substring(0, 8))"; user = 'ubuntu' } }
  if ($State -ceq 'unsatisfiable') {
    $r['reasons'] = @([ordered]@{ code = 'no_capacity'; message = "No capacity can be found for project '$Project' at size '$Size': its host is serving other leases at a smaller size. Release them, or ask for a size they fit." })
  }
  $r['driver'] = $script:SpDriver
  return $r
}

# Writes the lease state file with the records given, and throws when it does not validate.
function Set-SpLeases {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  Write-CcState $Lab ([ordered]@{ version = 0; leases = @($Records) })
  if (-not (Test-CcStateFile $Lab)) { throw 'The case wrote a lease state file that does not validate against its schema.' }
}

# --- Running the lease CLI -----------------------------------------------------------------------------

function Invoke-SpCreate {
  param([Parameter(Mandatory)]$Lab, [string[]]$Extra = @(), [Parameter(Mandatory)]$StandIn, [hashtable]$EnvVars = @{}, [string]$Project = $script:SpProject)
  Invoke-PvLease -Lab $Lab -Arguments (@('create', '--project', $Project) + $Extra) -StandIn $StandIn -EnvVars $EnvVars
}

# --- What a case compares ------------------------------------------------------------------------------

# The files a command may change, as they are: the hashes of hosts.json and projects.json, each stored record
# as a canonical text by its ID, and the names of the lease folders and of the alias files.
function Get-SpSnapshot {
  param([Parameter(Mandatory)]$Lab)
  $records = @{}
  $state = Read-CcState $Lab
  if ($state) { foreach ($r in $state['leases']) { $records[[string]$r['id']] = ConvertTo-CcCanonical $r } }
  [pscustomobject]@{
    Hosts = Get-PvHostsHash $Lab
    Projects = Get-CcFileHash (Get-PvProjectsPath $Lab)
    Records = $records
    LeaseDirs = @(Get-CcFileNames (Join-Path $Lab.UserProfile '.agentspace\leases'))
    Aliases = @(Get-CcFileNames (Join-Path $Lab.SshDir 'agentspace.d'))
  }
}

# The problems with what a command changed since $Before. The registry and the project list are as they were,
# unless -RegistryChanges (its own check follows); every record that was there is there as it was; and the
# command added exactly $NewRecords records, $NewFiles lease folders and $NewFiles alias files.
function Get-SpChangeProblems {
  param([Parameter(Mandatory)]$Before, [Parameter(Mandatory)]$After, [int]$NewRecords = 0, [int]$NewFiles = 0, [switch]$RegistryChanges)
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not $RegistryChanges -and $After.Hosts -cne $Before.Hosts) { $p.Add('hosts.json was written') }
  if ($After.Projects -cne $Before.Projects) { $p.Add('projects.json was written') }
  foreach ($id in @($Before.Records.Keys)) {
    if (-not $After.Records.ContainsKey($id)) { $p.Add('A record of the lease state file is gone') }
    elseif ($After.Records[$id] -cne $Before.Records[$id]) { $p.Add('A record of the lease state file changed') }
  }
  $added = $After.Records.Count - $Before.Records.Count
  if ($added -ne $NewRecords) { $p.Add("The lease state file gained $added records, not $NewRecords") }
  $dirs = $After.LeaseDirs.Count - $Before.LeaseDirs.Count
  if ($dirs -ne $NewFiles) { $p.Add("The leases folder gained $dirs entries, not $NewFiles") }
  foreach ($n in $Before.LeaseDirs) { if ($After.LeaseDirs -cnotcontains $n) { $p.Add('A lease folder is gone') } }
  $aliases = $After.Aliases.Count - $Before.Aliases.Count
  if ($aliases -ne $NewFiles) { $p.Add("The alias folder gained $aliases files, not $NewFiles") }
  foreach ($n in $Before.Aliases) { if ($After.Aliases -cnotcontains $n) { $p.Add('An alias file is gone') } }
  return , $p
}

# The problems with a stored lease: it is in the lease state file in the state given, with the size and a
# driver, and the file validates against its schema.
function Get-SpStoredProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$State, [Parameter(Mandatory)][string]$Size)
  $p = [System.Collections.Generic.List[string]]::new()
  $rec = Get-CcRecord $Lab $Id
  if ($null -eq $rec) { $p.Add('The lease is not in the lease state file'); return , $p }
  if ($rec['state'] -cne $State) { $p.Add("The stored lease is '$($rec['state'])', not $State") }
  if ($rec['size'] -cne $Size) { $p.Add('The stored lease''s size is not the size asked for') }
  if ([string]::IsNullOrEmpty([string]$rec['driver'])) { $p.Add('The stored lease carries no driver') }
  if (-not (Test-CcStateFile $Lab)) { $p.Add('The lease state file does not validate against its schema') }
  return , $p
}

# The text of every document a case prints must name no machine type: the contract names a size and never a type.
function Test-SpNamesNoType { param([string]$Text) $Text -notmatch '(?i)m9g|xlarge|\.large|instance' }

# The problems with a run that answered ready at $Size: the document, its alias, the stored record and the
# alias file, which has the host's key lines of a host that is not the box (state.md, "A lease alias for a
# host that is not the box"), all read from the case's own state folder.
function Get-SpReadyProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Size, [string]$Project = $script:SpProject)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $Run)) { $p.Add($x) }
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  if ($doc['state'] -cne 'ready') { $p.Add("The lease's state is '$($doc['state'])', not ready"); return , $p }
  if ($doc['project'] -cne $Project) { $p.Add('The lease is for another project') }
  if ($doc['size'] -cne $Size) { $p.Add("The lease's size is '$($doc['size'])', not the size asked for, '$Size'") }
  if (-not (Test-SpNamesNoType $Run.Stdout)) { $p.Add('The document names a machine type') }
  $alias = [string]$doc['ssh']['host_alias']
  if ($alias -cnotmatch "\Aagentspace-$([regex]::Escape($Project))-[0-9a-f]{8}\z") { $p.Add('The host alias is not the project''s form'); return , $p }
  foreach ($x in (Get-SpStoredProblems -Lab $Lab -Id ([string]$doc['id']) -State 'ready' -Size $Size)) { $p.Add($x) }
  $file = Get-CcAliasPath $Lab $alias
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { $p.Add('There is no alias file for the lease') }
  else {
    $text = [IO.File]::ReadAllText($file)
    if (-not $text.Contains("`n  HostKeyAlias agentspace-host.$Project`n")) { $p.Add('The alias file has no HostKeyAlias line of the project''s host') }
    if (-not $text.Contains("`n  UserKnownHostsFile ")) { $p.Add('The alias file has no UserKnownHostsFile line') }
  }
  return , $p
}

# The problems with a run that answered unsatisfiable with one reason, no_capacity, whose message is D66's
# byte for byte (driver.md, "On a ready host"), with the project and the size asked for.
function Get-SpNoCapacityProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Size, [string]$Project = $script:SpProject)
  $p = Get-PvUnsatisfiableDocProblems -Run $Run -Project $Project -Codes @('no_capacity')
  $doc = $Run.Json
  if ($doc -is [hashtable]) {
    $want = "No capacity can be found for project '$Project' at size '$Size': its host is serving other leases at a smaller size. Release them, or ask for a size they fit."
    $msg = [string]@($doc['reasons'])[0]['message']
    if ($msg -cne $want) { $p.Add('The no_capacity message is not D66''s byte for byte') }
    if ($doc['size'] -cne $Size) { $p.Add('The lease''s size is not the size asked for') }
    if ($doc.ContainsKey('expires_at') -or $doc.ContainsKey('ssh')) { $p.Add('An unsatisfiable lease carries expires_at or ssh') }
  }
  return , $p
}

# The problems with a run that answered pending and started the stand-in once, as Z5 gives it (driver.md, "On a
# ready host", row 6): a document with no expires_at, ssh or reasons; no lease folder or alias file; the stand-in
# started once with exactly provision -Host <project>; and the record keeping its state, instance_type and
# instance_id, and gaining last_attempt_at within the command's run. $Before is the snapshot before the command.
function Get-SpPendingProblems {
  param(
    [Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)]$StandIn, [Parameter(Mandatory)]$Before,
    [Parameter(Mandatory)][string]$Size, [Parameter(Mandatory)][string]$InstanceType, [string]$InstanceId,
    [Parameter(Mandatory)][DateTime]$Start, [Parameter(Mandatory)][DateTime]$End, [string]$Project = $script:SpProject
  )
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-PvPendingDocProblems -Run $Run -Project $Project)) { $p.Add($x) }
  if ($Run.Json -is [hashtable] -and $Run.Json['size'] -cne $Size) { $p.Add('The lease''s size is not the size asked for') }
  foreach ($x in (Get-SpChangeProblems -Before $Before -After (Get-SpSnapshot $Lab) -NewRecords 1 -NewFiles 0 -RegistryChanges)) { $p.Add($x) }
  foreach ($x in (Get-PvHostsFileProblems -Lab $Lab)) { $p.Add($x) }
  $rec = Get-PvHostRecord $Lab $Project
  if ($null -eq $rec) { $p.Add('hosts.json holds no record for the project') }
  else {
    $time = [string]$rec['last_attempt_at']
    if (-not (Test-PvTimeBetween -Time $time -Before $Start -After $End)) { $p.Add('last_attempt_at is not within the command''s run') }
    $want = New-SpHostsText -Records @((New-SpHostRecord -Project $Project -InstanceType $InstanceType -InstanceId $InstanceId -LastAttemptAt $time))
    foreach ($x in (Get-PvHostsTextProblems -Lab $Lab -Expected $want)) { $p.Add($x) }
  }
  if ($Run.Json -is [hashtable]) { foreach ($x in (Get-SpStoredProblems -Lab $Lab -Id ([string]$Run.Json['id']) -State 'pending' -Size $Size)) { $p.Add($x) } }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $StandIn -Project $Project)) { $p.Add($x) }
  return , $p
}

# ---- Z1 -----------------------------------------------------------------------------------------------
foreach ($size in 's', 'm', 'l', 'xl') {
  $sub = @{ Size = $size; Type = $script:SpType[$size] }
  Use-PvLab -Name "Z1: With no other lease of the project and a record ready at the type of size $size, create --size $size answers ready with size $size and no type, starts nothing and leaves the registry as it was" -Body {
    param($lab)
    [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $sub.Type)))
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $before = Get-SpSnapshot $lab
    $run = Invoke-SpCreate -Lab $lab -Extra @('--size', $sub.Size) -StandIn $standIn
    $p = Get-SpReadyProblems -Lab $lab -Run $run -Size $sub.Size
    foreach ($x in (Get-SpChangeProblems -Before $before -After (Get-SpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
    return , $p
  }
}

# ---- Z2 -----------------------------------------------------------------------------------------------
# A host registry and a project list that do not validate, and no CA, give the same answers as a good set: an
# unsupported size is decided before any of them is read. The good set also holds a live ready lease of the
# project on a host at the type of m, which would refuse a supported request for l with no_capacity: an
# unsupported request lists its unsupported_* reasons alone, never no_capacity (lease-api.md, "Fields").
function Test-SpUnsupportedSize {
  param([string]$Label, [bool]$Broken)
  $sub = @{ Label = $Label; Broken = $Broken }
  Use-PvLab -NoCa:$Broken -Name "Z2: $Label," -Body {
    param($lab)
    if ($sub.Broken) {
      Remove-PvCa -Lab $lab
      Write-PvText -Lab $lab -Path (Get-PvHostsPath $lab) -Text "this is not json`n"
      Write-PvText -Lab $lab -Path (Get-PvProjectsPath $lab) -Text "this is not json`n"
    } else {
      [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType['m'])))
      Set-SpLeases -Lab $lab -Records @((New-SpLeaseRecord -State 'ready' -CreatedAgo 60 -ExpiresAt (Get-SpTime 3540)))
    }
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $sizeMessage = "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl."
    $cases = @(
      @{ Key = 'create --size xxl answers unsatisfiable with exactly one reason, unsupported_size, its message byte for byte, and writes nothing'; Extra = @('--size', 'xxl'); Codes = @('unsupported_size'); Size = 'xxl' }
      @{ Key = 'create --size XXL answers the same, quoting the size normalized'; Extra = @('--size', 'XXL'); Codes = @('unsupported_size'); Size = 'xxl' }
      @{ Key = 'create --size xxl --platform windows/amd64 lists unsupported_platform then unsupported_size, in the contract''s order'; Extra = @('--size', 'xxl', '--platform', 'windows/amd64'); Codes = @('unsupported_platform', 'unsupported_size'); Size = 'xxl' }
      @{ Key = 'create --size l --platform windows/amd64 lists unsupported_platform alone (pin P9), never no_capacity beside it';Extra = @('--size', 'l', '--platform', 'windows/amd64'); Codes = @('unsupported_platform'); Size = 'l' }
    )
    $results = [ordered]@{}
    foreach ($c in $cases) {
      $hosts = Get-PvHostsHash $lab
      $projects = Get-CcFileHash (Get-PvProjectsPath $lab)
      $run = Invoke-SpCreate -Lab $lab -Extra $c.Extra -StandIn $standIn
      $p = Get-PvUnsatisfiableDocProblems -Run $run -Project $script:SpProject -Codes $c.Codes
      if ($run.Json -is [hashtable]) {
        if ($run.Json['size'] -cne $c.Size) { $p.Add('The lease''s size is not the size asked for, normalized') }
        $msgs = @($run.Json['reasons'] | Where-Object { $null -ne $_ -and $_['code'] -ceq 'unsupported_size' } | ForEach-Object { [string]$_['message'] })
        if ($c.Codes -contains 'unsupported_size' -and ($msgs.Count -ne 1 -or $msgs[0] -cne $sizeMessage)) { $p.Add('The unsupported_size message is not the driver''s byte for byte') }
      }
      if ((Get-PvHostsHash $lab) -ne $hosts) { $p.Add('hosts.json was written') }
      if ((Get-CcFileHash (Get-PvProjectsPath $lab)) -ne $projects) { $p.Add('projects.json was written') }
      if ($sub.Broken -and (Test-Path -LiteralPath (Join-Path $lab.UserProfile '.agentspace\ca'))) { $p.Add('create made a CA folder') }
      $results[$c.Key] = $p
    }
    $results['starts nothing in all four'] = Get-PvNoStartProblems -StandIn $standIn
    return $results
  }
}
Test-SpUnsupportedSize -Label 'With a CA, a host registry that holds the project''s record ready at the type of m, and a live ready lease of the project' -Broken $false
Test-SpUnsupportedSize -Label 'With no CA, and a host registry and a project list that do not validate' -Broken $true

# ---- Z3 -----------------------------------------------------------------------------------------------
# A live ready lease is made with create at the host's exact size, and the creates after it are judged by it.
foreach ($arm in @(
    @{ Host = 'l'; First = 'l'; Then = @('m', 's'); Label = 'a record ready at the type of l' },
    @{ Host = 'm'; First = 'm'; Then = @('default'); Label = 'a record ready at the type of m' })) {
  $sub = $arm
  Use-PvLab -Name "Z3: With a live ready lease of the project and $($sub.Label)," -Body {
    param($lab)
    [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType[$sub.Host])))
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $results = [ordered]@{}
    $first = Invoke-SpCreate -Lab $lab -Extra @('--size', $sub.First) -StandIn $standIn
    $results["a create --size $($sub.First) at the host's exact size makes the live ready lease"] = Get-SpReadyProblems -Lab $lab -Run $first -Size $sub.First
    foreach ($ask in $sub.Then) {
      $extra = @(if ($ask -ceq 'default') { } else { '--size'; $ask })
      $want = $(if ($ask -ceq 'default') { 'm' } else { $ask })
      $before = Get-SpSnapshot $lab
      $run = Invoke-SpCreate -Lab $lab -Extra $extra -StandIn $standIn
      $p = Get-SpReadyProblems -Lab $lab -Run $run -Size $want
      foreach ($x in (Get-SpChangeProblems -Before $before -After (Get-SpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
      $results["create $(if ($ask -ceq 'default') { 'with the default size' } else { "--size $ask" }) answers ready with size $want and the project's host's alias, and leaves the registry as it was"] = $p
    }
    $results['starts nothing'] = Get-PvNoStartProblems -StandIn $standIn
    return $results
  }
}

# ---- Z4 -----------------------------------------------------------------------------------------------
Use-PvLab -Name 'Z4: With a live ready lease of the project and a record ready at the type of m,' -Body {
  param($lab)
  [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType['m'])))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $first = Invoke-SpCreate -Lab $lab -StandIn $standIn
  $results['a create with the default size makes the live ready lease'] = Get-SpReadyProblems -Lab $lab -Run $first -Size 'm'
  foreach ($ask in 'l', 'xl') {
    $before = Get-SpSnapshot $lab
    $run = Invoke-SpCreate -Lab $lab -Extra @('--size', $ask) -StandIn $standIn
    $p = Get-SpNoCapacityProblems -Run $run -Size $ask
    # The lease is recorded, as every unsatisfiable lease is; no other record, no folder and no alias file changes.
    foreach ($x in (Get-SpChangeProblems -Before $before -After (Get-SpSnapshot $lab) -NewRecords 1 -NewFiles 0)) { $p.Add($x) }
    if ($run.Json -is [hashtable]) { foreach ($x in (Get-SpStoredProblems -Lab $lab -Id ([string]$run.Json['id']) -State 'unsatisfiable' -Size $ask)) { $p.Add($x) } }
    $results["create --size $ask answers unsatisfiable, exit 0, with one reason, no_capacity, D66's message byte for byte; it makes no lease folder or alias file, and leaves the registry and every other record as they were"] = $p
  }
  $results['starts nothing'] = Get-PvNoStartProblems -StandIn $standIn
  return $results
}

# ---- Z6 -----------------------------------------------------------------------------------------------
Use-PvLab -Name 'Z6: With a stored ready lease of the project whose expires_at is past, a released one, an unsatisfiable one and a live ready lease of another project, and a record ready at the type of m, create --size xl answers pending and starts the stand-in once, never unsatisfiable' -Body {
  param($lab)
  [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType['m'] -InstanceId $script:SpInstanceId)))
  $records = @(
    (New-SpLeaseRecord -State 'ready' -CreatedAgo 7200 -ExpiresAt (Get-SpTime -3600))
    # A released lease that was ready keeps its expires_at, still to come: it is not live all the same.
    (New-SpLeaseRecord -State 'released' -CreatedAgo 600 -ExpiresAt (Get-SpTime 3000) -ReleasedAt (Get-SpTime -300))
    (New-SpLeaseRecord -State 'unsatisfiable' -Size 'xl')
    (New-SpLeaseRecord -State 'ready' -Project 'othersizecase' -CreatedAgo 60 -ExpiresAt (Get-SpTime 3540))
  )
  Set-SpLeases -Lab $lab -Records $records
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $before = Get-SpSnapshot $lab
  $start = Get-CcNowSecond
  $run = Invoke-SpCreate -Lab $lab -Extra @('--size', 'xl') -StandIn $standIn
  $end = Get-CcNowSecond
  return , (Get-SpPendingProblems -Lab $lab -Run $run -StandIn $standIn -Before $before -Size 'xl' -InstanceType $script:SpType['m'] -InstanceId $script:SpInstanceId -Start $start -End $end)
}

# ---- Z7 -----------------------------------------------------------------------------------------------
Use-PvLab -Name 'Z7: Expiry is decided by the reading''s time: with a lease made with --ttl 1m at the host''s exact size, create --size xl answers unsatisfiable with no_capacity, and once its expires_at has passed the same create answers pending and starts the stand-in' -Body {
  param($lab)
  [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType['m'])))
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $first = Invoke-SpCreate -Lab $lab -Extra @('--ttl', '1m') -StandIn $standIn
  $p = Get-SpReadyProblems -Lab $lab -Run $first -Size 'm'
  if ($p.Count -gt 0 -or $first.Json -isnot [hashtable] -or $first.Json['expires_at'] -cnotmatch '\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z\z') { $p.Insert(0, 'The one-minute lease was not made'); return , $p }
  $expires = ConvertFrom-CcTime ([string]$first.Json['expires_at'])
  if ([DateTime]::UtcNow -ge $expires) { $p.Add('The one-minute lease had expired before the first create ran'); return , $p }
  $before = Get-SpSnapshot $lab
  $held = Invoke-SpCreate -Lab $lab -Extra @('--size', 'xl') -StandIn $standIn
  $afterHeld = Get-SpSnapshot $lab
  foreach ($x in (Get-SpNoCapacityProblems -Run $held -Size 'xl')) { $p.Add("While the lease is live: $x") }
  foreach ($x in (Get-SpChangeProblems -Before $before -After $afterHeld -NewRecords 1 -NewFiles 0)) { $p.Add("While the lease is live: $x") }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add("While the lease is live: $x") }
  # One second past its expires_at, so that the reading's time is after it whichever way the clock rounds.
  $wait = ($expires.AddSeconds(1.5) - [DateTime]::UtcNow).TotalSeconds
  if ($wait -gt 75) { $p.Add('The wait for the lease to expire would be longer than a minute and a quarter'); return , $p }
  if ($wait -gt 0) { Start-Sleep -Milliseconds ([int]($wait * 1000)) }
  $start = Get-CcNowSecond
  $run = Invoke-SpCreate -Lab $lab -Extra @('--size', 'xl') -StandIn $standIn
  $end = Get-CcNowSecond
  foreach ($x in (Get-SpPendingProblems -Lab $lab -Run $run -StandIn $standIn -Before $afterHeld -Size 'xl' -InstanceType $script:SpType['m'] -Start $start -End $end)) { $p.Add("Once it has expired: $x") }
  return , $p
}

# ---- Z11 ----------------------------------------------------------------------------------------------
function Test-SpNoCaPending {
  param([string]$Label, [string]$HostSize, [string]$Ask)
  $sub = @{ Host = $HostSize; Ask = $Ask }
  Use-PvLab -NoCa -Name "Z11: $Label" -Body {
    param($lab)
    [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType[$sub.Host])))
    Remove-PvCa -Lab $lab
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $hosts = Get-PvHostsHash $lab
    $run = Invoke-SpCreate -Lab $lab -Extra @('--size', $sub.Ask) -StandIn $standIn
    $p = Get-PvInternalProblems -Run $run
    if ($run.ErrorJson -is [hashtable] -and $run.ErrorJson['message'] -cne $script:SpNoCaMessage) { $p.Add('The message is not lease-api.md''s for a missing CA') }
    foreach ($x in (Get-PvNothingRecordedProblems -Lab $lab)) { $p.Add($x) }
    if ((Get-PvHostsHash $lab) -ne $hosts) { $p.Add('hosts.json was written') }
    if (Test-Path -LiteralPath (Join-Path $lab.UserProfile '.agentspace\ca')) { $p.Add('create made a CA folder') }
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
    return , $p
  }
}
Test-SpNoCaPending -Label 'With no CA, a create --size l for a record ready at the type of m, with no live lease, which would answer pending, fails with internal, records no lease, leaves the registry as it was and starts nothing' -HostSize 'm' -Ask 'l'
Test-SpNoCaPending -Label 'With no CA, a create --size m for a record ready at the type of xl, with no live lease, which would answer pending, fails the same way' -HostSize 'xl' -Ask 'm'
Use-PvLab -NoCa -Name 'Z11: With no CA, a create --size xl for a record ready at the type of m with a live ready lease, which answers unsatisfiable, answers it with no_capacity and D66''s message, with no CA, and changes nothing else' -Body {
  param($lab)
  [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType['m'])))
  Set-SpLeases -Lab $lab -Records @((New-SpLeaseRecord -State 'ready' -CreatedAgo 60 -ExpiresAt (Get-SpTime 3540)))
  Remove-PvCa -Lab $lab
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $before = Get-SpSnapshot $lab
  $run = Invoke-SpCreate -Lab $lab -Extra @('--size', 'xl') -StandIn $standIn
  $p = Get-SpNoCapacityProblems -Run $run -Size 'xl'
  foreach ($x in (Get-SpChangeProblems -Before $before -After (Get-SpSnapshot $lab) -NewRecords 1 -NewFiles 0)) { $p.Add($x) }
  if (Test-Path -LiteralPath (Join-Path $lab.UserProfile '.agentspace\ca')) { $p.Add('create made a CA folder') }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  return , $p
}

# ---- Z13 ----------------------------------------------------------------------------------------------
Use-PvLab -Name 'Z13: A live ready lease recorded under another AGENTSPACE_STATE_DIR does not hold the host: with one in a second state folder and none in the case''s own, create --size xl for a record ready at the type of m answers pending and starts the stand-in' -Body {
  param($lab)
  [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType['m'] -InstanceId $script:SpInstanceId)))
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  # The second state folder is under the home's root, as the harness requires of an override.
  $other = @{ AGENTSPACE_STATE_DIR = (Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.Root 'state-other')) }
  $first = Invoke-SpCreate -Lab $lab -StandIn $standIn -EnvVars $other
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $first)) { $p.Add("Setup, the lease in the second state folder: $x") }
  if ($first.Json -isnot [hashtable] -or $first.Json['state'] -cne 'ready') { $p.Add('Setup: the lease in the second state folder is not ready'); return , $p }
  # The premise: the lease is live in the second folder, and the case's own folder has no lease state file.
  $get = Invoke-PvLease -Lab $lab -Arguments @('get', [string]$first.Json['id']) -StandIn $standIn -EnvVars $other
  if ($get.Json -isnot [hashtable] -or $get.Json['state'] -cne 'ready') { $p.Add('Setup: the lease is not ready when read from the second state folder') }
  if (Test-Path -LiteralPath (Get-CcStatePath $lab)) { $p.Add('Setup: the case''s own state folder holds a lease state file') }
  $before = Get-SpSnapshot $lab
  $start = Get-CcNowSecond
  $run = Invoke-SpCreate -Lab $lab -Extra @('--size', 'xl') -StandIn $standIn
  $end = Get-CcNowSecond
  foreach ($x in (Get-SpPendingProblems -Lab $lab -Run $run -StandIn $standIn -Before $before -Size 'xl' -InstanceType $script:SpType['m'] -InstanceId $script:SpInstanceId -Start $start -End $end)) { $p.Add($x) }
  return , $p
}

# ---- L1 -----------------------------------------------------------------------------------------------
# lease-api.md, "Fields": an unsatisfiable lease with no_capacity stays unsatisfiable, and a later create is
# judged afresh, among others once the project's other leases are released or when the size asked for is one the
# host already meets.
Use-PvLab -Name 'L1: With a record ready at the type of m,' -Body {
  param($lab)
  [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType['m'])))
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $results = [ordered]@{}
  $a = Invoke-SpCreate -Lab $lab -StandIn $standIn
  $results['a create with the default size makes the live ready lease'] = Get-SpReadyProblems -Lab $lab -Run $a -Size 'm'
  if ($a.Json -isnot [hashtable]) { return $results }
  $refused = Invoke-SpCreate -Lab $lab -Extra @('--size', 'l') -StandIn $standIn
  $p = Get-SpNoCapacityProblems -Run $refused -Size 'l'
  if ($refused.Json -isnot [hashtable]) { $results['create --size l is refused with no_capacity'] = $p; return $results }
  $results['create --size l is refused with no_capacity'] = $p
  $refusedId = [string]$refused.Json['id']

  $p = [System.Collections.Generic.List[string]]::new()
  $get = Invoke-PvLease -Lab $lab -Arguments @('get', $refusedId) -StandIn $standIn
  foreach ($x in (Get-PvUnsatisfiableDocProblems -Run $get -Project $script:SpProject -Codes @('no_capacity'))) { $p.Add($x) }
  $results['a get of the refused lease, while the other lease is live, still prints it unsatisfiable with no_capacity'] = $p

  $p = [System.Collections.Generic.List[string]]::new()
  $fits = Invoke-SpCreate -Lab $lab -Extra @('--size', 'm') -StandIn $standIn
  foreach ($x in (Get-SpReadyProblems -Lab $lab -Run $fits -Size 'm')) { $p.Add($x) }
  $results['a later create --size m, a size the host meets, is judged afresh and answers ready'] = $p

  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($r in @($a, $fits)) {
    if ($r.Json -is [hashtable]) {
      $rel = Invoke-PvLease -Lab $lab -Arguments @('release', [string]$r.Json['id']) -StandIn $standIn
      if ($rel.ExitCode -ne 0 -or $rel.Json -isnot [hashtable] -or $rel.Json['state'] -cne 'released') { $p.Add('A release of one of the live leases did not succeed') }
    }
  }
  $get2 = Invoke-PvLease -Lab $lab -Arguments @('get', $refusedId) -StandIn $standIn
  foreach ($x in (Get-PvUnsatisfiableDocProblems -Run $get2 -Project $script:SpProject -Codes @('no_capacity'))) { $p.Add("After the release: $x") }
  $results['once the other leases are released, a get of the refused lease still prints it unsatisfiable'] = $p

  $p = [System.Collections.Generic.List[string]]::new()
  $before = Get-SpSnapshot $lab
  $start = Get-CcNowSecond
  $later = Invoke-SpCreate -Lab $lab -Extra @('--size', 'l') -StandIn $standIn
  $end = Get-CcNowSecond
  foreach ($x in (Get-SpPendingProblems -Lab $lab -Run $later -StandIn $standIn -Before $before -Size 'l' -InstanceType $script:SpType['m'] -Start $start -End $end)) { $p.Add($x) }
  $results['a later create --size l, once the other leases are released, is judged afresh and answers pending, starting the stand-in once'] = $p
  return $results
}

# ---- L2 -----------------------------------------------------------------------------------------------
# lease-api.md, "Size": a size is accepted in any case and with surrounding whitespace, normalized to lower
# case with the whitespace trimmed, and is the size the document prints.
Use-PvLab -Name 'L2: With a record ready at the type of xl, a size in capitals or with surrounding whitespace is normalized before it is judged,' -Body {
  param($lab)
  [void](Set-SpHosts -Lab $lab -Records @((New-SpHostRecord -InstanceType $script:SpType['xl'])))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $run = Invoke-SpCreate -Lab $lab -Extra @('--size', 'XL') -StandIn $standIn
  $results['create --size XL, for a record ready at the type of xl, answers ready with size xl'] = Get-SpReadyProblems -Lab $lab -Run $run -Size 'xl'
  $run = Invoke-SpCreate -Lab $lab -Extra @('--size', ' S ') -StandIn $standIn
  $results['create --size '' S '', with a live ready lease on a host at the type of xl, answers ready with size s'] = Get-SpReadyProblems -Lab $lab -Run $run -Size 's'
  $results['starts nothing'] = Get-PvNoStartProblems -StandIn $standIn
  return $results
}

Exit-ConformanceRun

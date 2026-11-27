#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.11 programs, which answer unsatisfiable for linux/amd64 and windows/amd64
# and know no `platform`. A v0.12 build MUST pass every result line (docs/conformance.md, "The files that are expected
# to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what docs/lease-api.md,
# docs/driver.md and docs/state.md document, with v0.07's recording stand-in for the provisioning run (a program
# outside agentspace). See docs/conformance.md. Cases of E0.12-T04-S01 (card #0261), a project's compute per platform,
# each named by its row in docs/driver.md, "Conformance cases for v0.12" (D91, D94; architect, 2026-11-26, for
# v0.12-T01, pins P23, P28 and P32):
#   M7   A host's ready leases on another platform hold nothing here: for ready records on linux/arm64 and linux/amd64
#        and a live ready lease on linux/arm64 only, create --platform linux/amd64 --size l answers pending and starts
#        the stand-in once, with five arguments, while create --size l answers unsatisfiable with D66's message. With a
#        stored pending lease on linux/arm64 at l and none on linux/amd64, create --platform linux/amd64 answers ready
#        and starts nothing.
#   M8   The m8a column: with a live ready lease on linux/amd64 and its record ready at the l type, create --platform
#        linux/amd64 --size m answers ready and --size xl answers unsatisfiable with D66's message, each starting
#        nothing; with no live lease and the record at the xl type, the default size answers pending and starts the
#        stand-in once, with five arguments.
#   M9   A type from another column: a linux/amd64 record ready at m9g.xlarge meets no size, so the default size
#        answers pending, with and without a live ready lease on linux/amd64, and starts the stand-in.
#   M10  Each host has its own lock: with the check holding the linux/arm64 host's provision.lock, a create --platform
#        linux/amd64 for a project whose linux/amd64 record is creating starts the stand-in once, with five arguments;
#        holding hosts\<project>.linux-amd64\provision.lock instead, the same create starts nothing and leaves the
#        registry as it was.
#   M11  get of a stored pending windows/amd64 lease whose host's record is ready, with no last_attempt_at, starts the
#        stand-in once, with five arguments, the fifth windows/amd64; sets that record's last_attempt_at and no other
#        record's; and leaves the lease state file byte for byte as it was.
# A stand-in that MUST NOT start is watched once, after a home's last create, and its problems join the last check that home reports,
# so that no result line passes only because a build started nothing, as v0.11 starts nothing for a platform it refuses.
# Sections enforced: driver.md "On a ready host", "Sizes and instance types", "Starting a provisioning run" and
# "Lock order"; lease-api.md "Fields"; state.md "The host registry" and "A host's platform and names".
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No check starts a
# real run: every create and get here is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records
# what it is given and makes no host. Every write calls Assert-ConformancePath at the call. A failure reason never
# quotes a value the stand-in captured.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-lease-platform-hosts.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 50 seconds (a stand-in that MUST NOT start is watched for 3 seconds).

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')
. (Join-Path $PSScriptRoot 'lease-platform-common.ps1')

$script:HsProject = 'platcase'
$script:HsArm = 'linux/arm64'
$script:HsAmd = 'linux/amd64'
$script:HsWin = 'windows/amd64'
$where = 'driver.md#on-a-ready-host'

function Get-HsType { param([Parameter(Mandatory)][string]$Platform, [Parameter(Mandatory)][string]$Size) $script:LpPlatform[$Platform].Type[$Size] }
function New-HsHost { param([Parameter(Mandatory)][string]$Platform, [Parameter(Mandatory)][string]$Size, [string]$LastAttemptAt) New-LpHostRecord -Project $script:HsProject -Platform $Platform -InstanceType (Get-HsType $Platform $Size) -LastAttemptAt $LastAttemptAt }
function New-HsLive { param([Parameter(Mandatory)][string]$Platform, [string]$Size = 'm') New-LpLeaseRecord -State 'ready' -Project $script:HsProject -Platform $Platform -Size $Size -CreatedAgo 60 -ExpiresAt (Get-LpTime 3540) }

# The problems with a create that answered unsatisfiable with one reason, no_capacity, D66's message for the size, and
# changed nothing but the new unsatisfiable record.
function Get-HsSmallerProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)]$Before, [Parameter(Mandatory)][string]$Platform, [Parameter(Mandatory)][string]$Size)
  $p = Get-LpUnsatProblems -Run $Run -Project $script:HsProject -Codes @('no_capacity') -Messages @((Get-LpSmallerMessage $script:HsProject $Size)) -Platform $Platform -ProfileName $script:LpPlatform[$Platform].Profile -Size $Size
  foreach ($x in (Get-LpChangeProblems -Before $Before -After (Get-LpSnapshot $Lab) -NewRecords 1 -NewFiles 0)) { $p.Add($x) }
  return , $p
}

# The problems with a create that answered pending on a ready host and started the stand-in once with the platform's
# arguments: the pending document, a new stored pending record, hosts.json gaining last_attempt_at in the one record of
# the project on the platform within the run, every other record as it was, and nothing in the lease folders.
# $HostsBefore are the records the file held, in order, and $Target the index of the one that is the lease's host.
function Get-HsPendingProblems {
  param(
    [Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)]$StandIn, [Parameter(Mandatory)]$Before, [Parameter(Mandatory)][string]$Platform,
    [Parameter(Mandatory)][string[]]$HostsBefore, [Parameter(Mandatory)][int]$Target, [Parameter(Mandatory)][string]$TargetSize, [Parameter(Mandatory)][DateTime]$Start, [Parameter(Mandatory)][DateTime]$End, [string]$Size = 'm'
  )
  $p = Get-LpPendingProblems -Lab $Lab -Run $Run -Project $script:HsProject -Platform $Platform -Size $Size
  foreach ($x in (Get-LpChangeProblems -Before $Before -After (Get-LpSnapshot $Lab) -NewRecords 1 -NewFiles 0 -RegistryChanges)) { $p.Add($x) }
  foreach ($x in (Get-PvHostsFileProblems -Lab $Lab)) { $p.Add($x) }
  $rec = Get-LpHostRecord -Lab $Lab -Project $script:HsProject -Platform $Platform
  if ($null -eq $rec) { $p.Add('hosts.json holds no record of the project on the platform'); return , $p }
  $time = [string]$rec['last_attempt_at']
  if (-not (Test-PvTimeBetween -Time $time -Before $Start -After $End)) { $p.Add('last_attempt_at is not within the command''s run') }
  $after = @($HostsBefore)
  $after[$Target] = New-HsHost -Platform $Platform -Size $TargetSize -LastAttemptAt $time
  foreach ($x in (Get-PvHostsTextProblems -Lab $Lab -Expected (New-LpHostsText -Records $after))) { $p.Add($x) }
  foreach ($x in (Get-LpStartedOnceProblems -StandIn $StandIn -Project $script:HsProject -Platform $Platform)) { $p.Add($x) }
  return , $p
}

# ---- M7 -----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M7 ($where, D91): With ready records on linux/arm64 at the m type and linux/amd64 at the m type, and a live ready lease on linux/arm64 only," -Body {
  param($lab)
  $records = @((New-HsHost $script:HsArm 'm'), (New-HsHost $script:HsAmd 'm'))
  [void](Set-LpHosts -Lab $lab -Records $records)
  Set-LpLeases -Lab $lab -Records @((New-HsLive $script:HsArm))
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $results = [ordered]@{}
  $before = Get-LpSnapshot $lab
  $start = Get-CcNowSecond
  $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--platform', $script:HsAmd, '--size', 'l') -StandIn $standIn
  $end = Get-CcNowSecond
  $results['create --platform linux/amd64 --size l answers pending, the arm64 lease holding nothing there, and starts the stand-in once with five arguments, writing last_attempt_at into the amd64 record alone'] =
    Get-HsPendingProblems -Lab $lab -Run $run -StandIn $standIn -Before $before -Platform $script:HsAmd -HostsBefore $records -Target 1 -TargetSize 'm' -Start $start -End $end -Size 'l'
  # The amd64 pending lease is another host's: it holds nothing of the arm64 host either.
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--size', 'l') -StandIn $standIn
  $results['create --size l, on linux/arm64, answers unsatisfiable with D66''s message, and changes nothing else'] = Get-HsSmallerProblems -Lab $lab -Run $run -Before $before -Platform $script:HsArm -Size 'l'
  $further = [System.Collections.Generic.List[string]]::new()
  Start-Sleep -Seconds 3
  $startedLines = @(Get-PvStartedLines $standIn).Count
  if ($startedLines -ne 1) { $further.Add("The stand-in was started $startedLines times in all, not once") }
  $results['the arm64 create starts nothing: the stand-in was started once in all'] = $further
  return $results
}
Use-PvLab -Name "M7 ($where, D91): With ready records on both platforms, a stored pending lease of the project on linux/arm64 at l and none on linux/amd64," -Body {
  param($lab)
  [void](Set-LpHosts -Lab $lab -Records @((New-HsHost $script:HsArm 'm'), (New-HsHost $script:HsAmd 'm')))
  Set-LpLeases -Lab $lab -Records @((New-LpLeaseRecord -State 'pending' -Project $script:HsProject -Platform $script:HsArm -Size 'l'))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--platform', $script:HsAmd) -StandIn $standIn
  $p = Get-LpReadyProblems -Lab $lab -Run $run -Project $script:HsProject -Platform $script:HsAmd
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  return , $p
}

# ---- M8 -----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M8 ($where, D94): With a live ready lease on linux/amd64 and its record ready at the l type," -Body {
  param($lab)
  [void](Set-LpHosts -Lab $lab -Records @((New-HsHost $script:HsAmd 'l')))
  Set-LpLeases -Lab $lab -Records @((New-HsLive $script:HsAmd))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--platform', $script:HsAmd, '--size', 'm') -StandIn $standIn
  $p = Get-LpReadyProblems -Lab $lab -Run $run -Project $script:HsProject -Platform $script:HsAmd -Size 'm'
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
  $results['create --platform linux/amd64 --size m answers ready, on the larger host, and leaves the registry as it was'] = $p
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--platform', $script:HsAmd, '--size', 'xl') -StandIn $standIn
  $refused = Get-HsSmallerProblems -Lab $lab -Run $run -Before $before -Platform $script:HsAmd -Size 'xl'
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $refused.Add($x) }
  $results['create --platform linux/amd64 --size xl answers unsatisfiable with D66''s message, changes nothing else, and neither create starts the stand-in'] = $refused
  return $results
}
Use-PvLab -Name "M8 ($where, D94): With no live lease and the linux/amd64 record ready at the xl type," -Body {
  param($lab)
  $records = @((New-HsHost $script:HsAmd 'xl'))
  [void](Set-LpHosts -Lab $lab -Records $records)
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $before = Get-LpSnapshot $lab
  $start = Get-CcNowSecond
  $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--platform', $script:HsAmd) -StandIn $standIn
  $end = Get-CcNowSecond
  return , (Get-HsPendingProblems -Lab $lab -Run $run -StandIn $standIn -Before $before -Platform $script:HsAmd -HostsBefore $records -Target 0 -TargetSize 'xl' -Start $start -End $end)
}

# ---- M9 -----------------------------------------------------------------------------------------------------
foreach ($arm in @(@{ Live = $false; Label = 'with no live ready lease' }, @{ Live = $true; Label = 'with a live ready lease of the project on linux/amd64' })) {
  $sub = $arm
  Use-PvLab -Name "M9 (driver.md#sizes-and-instance-types, pin P28): With the linux/amd64 record ready at m9g.xlarge, a type of another column, $($sub.Label), create --platform linux/amd64 with the default size answers pending, never ready or unsatisfiable, and starts the stand-in once with five arguments" -Body {
    param($lab)
    $records = @((New-LpHostRecord -Project $script:HsProject -Platform $script:HsAmd -InstanceType 'm9g.xlarge'))
    [void](Set-LpHosts -Lab $lab -Records $records)
    if ($sub.Live) { Set-LpLeases -Lab $lab -Records @((New-HsLive $script:HsAmd)) }
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    $before = Get-LpSnapshot $lab
    $start = Get-CcNowSecond
    $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--platform', $script:HsAmd) -StandIn $standIn
    $end = Get-CcNowSecond
    # The record keeps the type it had: a run records the type its host has, and no run has finished.
    $p =Get-LpPendingProblems -Lab $lab -Run $run -Project $script:HsProject -Platform $script:HsAmd
    foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 0 -RegistryChanges)) { $p.Add($x) }
    $rec = Get-LpHostRecord -Lab $lab -Project $script:HsProject -Platform $script:HsAmd
    if ($null -eq $rec) { $p.Add('hosts.json holds no record of the project on linux/amd64') }
    else {
      $time = [string]$rec['last_attempt_at']
      if (-not (Test-PvTimeBetween -Time $time -Before $start -After $end)) { $p.Add('last_attempt_at is not within the command''s run') }
      $want = New-LpHostsText -Records @((New-LpHostRecord -Project $script:HsProject -Platform $script:HsAmd -InstanceType 'm9g.xlarge' -LastAttemptAt $time))
      foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected $want)) { $p.Add($x) }
    }
    foreach ($x in (Get-LpStartedOnceProblems -StandIn $standIn -Project $script:HsProject -Platform $script:HsAmd)) { $p.Add($x) }
    return , $p
  }
}

# ---- M10 ----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M10 (driver.md#lock-order): With the check holding the linux/arm64 host's provision.lock and the project's linux/amd64 record creating with no last_attempt_at," -Body {
  param($lab)
  $records = @((New-HsHost $script:HsArm 'm'), (New-LpHostRecord -Project $script:HsProject -Platform $script:HsAmd -State 'creating'))
  [void](Set-LpHosts -Lab $lab -Records $records)
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  [void](Open-PvLock -Lab $lab -Path (Get-LpProvisionLock -Lab $lab -Project $script:HsProject -Platform $script:HsArm))
  $start = Get-CcNowSecond
  $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--platform', $script:HsAmd) -StandIn $standIn
  $end = Get-CcNowSecond
  $p = Get-LpPendingProblems -Lab $lab -Run $run -Project $script:HsProject -Platform $script:HsAmd
  $rec = Get-LpHostRecord -Lab $lab -Project $script:HsProject -Platform $script:HsAmd
  if ($null -eq $rec) { $p.Add('hosts.json holds no record of the project on linux/amd64') }
  else {
    $time = [string]$rec['last_attempt_at']
    if (-not (Test-PvTimeBetween -Time $time -Before $start -After $end)) { $p.Add('last_attempt_at is not within the command''s run') }
    $after = @($records[0], (New-LpHostRecord -Project $script:HsProject -Platform $script:HsAmd -State 'creating' -LastAttemptAt $time))
    foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected (New-LpHostsText -Records $after))) { $p.Add($x) }
  }
  foreach ($x in (Get-LpStartedOnceProblems -StandIn $standIn -Project $script:HsProject -Platform $script:HsAmd)) { $p.Add($x) }
  return , $p
}
Use-PvLab -Name "M10 (driver.md#lock-order): With the check holding hosts\<project>.linux-amd64\provision.lock instead, and the same records," -Body {
  param($lab)
  [void](Set-LpHosts -Lab $lab -Records @((New-HsHost $script:HsArm 'm'), (New-LpHostRecord -Project $script:HsProject -Platform $script:HsAmd -State 'creating')))
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  [void](Open-PvLock -Lab $lab -Path (Get-LpProvisionLock -Lab $lab -Project $script:HsProject -Platform $script:HsAmd))
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:HsProject -Extra @('--platform', $script:HsAmd) -StandIn $standIn
  $p = Get-LpPendingProblems -Lab $lab -Run $run -Project $script:HsProject -Platform $script:HsAmd
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 0)) { $p.Add($x) }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  return , $p
}

# ---- M11 ----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M11 (driver.md#starting-a-provisioning-run, pin P32): With a stored pending windows/amd64 lease and its host's record ready at the m type with no last_attempt_at," -Body {
  param($lab)
  $records = @((New-HsHost $script:HsArm 'm'), (New-HsHost $script:HsWin 'm'))
  [void](Set-LpHosts -Lab $lab -Records $records)
  $lease = New-LpLeaseRecord -State 'pending' -Project $script:HsProject -Platform $script:HsWin
  Set-LpLeases -Lab $lab -Records @($lease)
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $leasesBefore = Get-CcFileHash (Get-CcStatePath $lab)
  $before = Get-LpSnapshot $lab
  $start = Get-CcNowSecond
  $run = Invoke-PvLease -Lab $lab -Arguments @('get', [string]$lease['id']) -StandIn $standIn
  $end = Get-CcNowSecond
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
  if ($run.Json -is [hashtable]) {
    if ($run.Json['state'] -cne 'pending' -or $run.Json['platform'] -cne $script:HsWin -or $run.Json['profile'] -cne 'agent-windows') { $p.Add('get did not print the lease as a pending windows/amd64 lease with agent-windows') }
  }
  if ((Get-CcFileHash (Get-CcStatePath $lab)) -cne $leasesBefore) { $p.Add('get wrote the lease state file') }
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 0 -NewFiles 0 -RegistryChanges)) { $p.Add($x) }
  foreach ($x in (Get-PvHostsFileProblems -Lab $lab)) { $p.Add($x) }
  $rec = Get-LpHostRecord -Lab $lab -Project $script:HsProject -Platform $script:HsWin
  if ($null -eq $rec) { $p.Add('hosts.json holds no record of the project on windows/amd64') }
  else {
    $time = [string]$rec['last_attempt_at']
    if (-not (Test-PvTimeBetween -Time $time -Before $start -After $end)) { $p.Add('last_attempt_at is not within the command''s run') }
    $after = @($records[0], (New-HsHost $script:HsWin 'm' $time))
    foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected (New-LpHostsText -Records $after))) { $p.Add($x) }
  }
  foreach ($x in (Get-LpStartedOnceProblems -StandIn $standIn -Project $script:HsProject -Platform $script:HsWin)) { $p.Add($x) }
  return , $p
}

Exit-ConformanceRun

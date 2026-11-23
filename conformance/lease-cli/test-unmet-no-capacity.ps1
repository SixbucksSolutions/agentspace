#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.06 programs, which have no project list, no host registry,
# no `pending` and no `no_capacity`: every project's create answers `ready` there. A v0.07 build MUST
# pass every result line (docs/conformance.md, "The files that are expected to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what docs/lease-api.md, docs/driver.md and docs/state.md document, with a stand-in for the provisioning run (a program outside agentspace). See docs/conformance.md.
#
# Cases of E0.07-T04-S01 (card #0155), the create side that answers without making a host, each named
# by its case in docs/driver.md, "Conformance cases for v0.07":
#   C1   A supported create for a project with no record and not on the list: unsatisfiable, one reason,
#        no_capacity, a record with a driver, no files, no registry write, no run started.
#   C2   An unsupported create for such a project: its unsupported_* reasons and no no_capacity, no
#        registry write, and no reading of the registry, the list or the CA.
#   C17  A supported create with a host registry that does not validate, or a project with no record
#        and a project list that does not validate: internal, nothing recorded.
#   C21  A project that has a record is placed by it whatever the project list holds.
#   C24  A record that is creating or failed wins over the list: pending, and one run started.
#   C32  A pending answer checks the CA first: with no CA, internal, nothing recorded, no run started.
# Sections enforced: driver.md "Where it places a lease", "Starting a provisioning run" and "Unsatisfiable
# requests"; lease-api.md "Fields" (reasons), "States", "The credential" and "Messages"; state.md
# "The host registry" and "The project list".
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home, and no
# check starts a real provisioning run: every create here is given AGENTSPACE_PROVISION_CMD naming a
# stand-in under the home that records what it is given and makes no host. Every write calls
# Assert-ConformancePath at the call. A failure reason never quotes a value the stand-in captured, or
# the program's output beyond a code.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-no-capacity.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 1 minute (a 3-second wait for each stand-in that MUST NOT start).

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

$where = 'driver.md#where-it-places-a-lease'
$noCaMessage = "agentspace lease create failed: agentspace's certificate authority has not been set up, so no certificate can be issued. Nothing was changed; the operator sets it up."

# The problems that a create which answered without making a host MUST NOT show: a registry or a list that
# changed, a lease's files, and a run started.
function Get-NoWriteProblems {
  param($Lab, $StandIn, [string]$HostsBefore, [string]$ProjectsBefore)
  $p = [System.Collections.Generic.List[string]]::new()
  if ((Get-PvHostsHash $Lab) -ne $HostsBefore) { $p.Add('hosts.json was written') }
  if ((Get-CcFileHash (Get-PvProjectsPath $Lab)) -ne $ProjectsBefore) { $p.Add('projects.json was written') }
  foreach ($x in (Get-PvNoStartProblems -StandIn $StandIn)) { $p.Add($x) }
  return , $p
}

# ---- C1 -------------------------------------------------------------------------------------------------
function Test-NoCapacity {
  param([string]$Label, [scriptblock]$Seed)
  Use-PvLab -Name "$where C1: $Label" -Body {
    param($lab)
    & $Seed $lab
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $hostsBefore = Get-PvHostsHash $lab
    $projectsBefore = Get-CcFileHash (Get-PvProjectsPath $lab)
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'nocap') -StandIn $standIn
    $p = Get-PvUnsatisfiableDocProblems -Run $run -Project 'nocap' -Codes @('no_capacity')
    $doc = $run.Json
    if ($doc -is [hashtable] -and $doc['reasons'] -is [System.Collections.IEnumerable]) {
      $msg = [string]@($doc['reasons'])[0]['message']
      if (-not $msg.Contains("'nocap'")) { $p.Add('The no_capacity message does not quote the project') }
      if ($msg -cmatch '(?i)driver|ec2') { $p.Add('The no_capacity message names the driver') }
      $rec = Get-CcRecord $lab ([string]$doc['id'])
      if ($null -eq $rec) { $p.Add('The lease is not in the lease state file') }
      elseif ([string]::IsNullOrEmpty([string]$rec['driver'])) { $p.Add('The lease record carries no driver') }
      if (-not (Test-CcStateFile $lab)) { $p.Add('The lease state file does not validate against its schema') }
    }
    foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
    foreach ($x in (Get-NoWriteProblems -Lab $lab -StandIn $standIn -HostsBefore $hostsBefore -ProjectsBefore $projectsBefore)) { $p.Add($x) }
    return , $p
  }
}
Test-NoCapacity -Label 'A supported create for a project with no record, and no host registry or project list, answers unsatisfiable with one reason, no_capacity' -Seed { param($lab) }
Test-NoCapacity -Label 'The same with a host registry that holds other hosts and a project list that does not list the project' -Seed {
  param($lab)
  Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'other' -State 'ready'))
  Set-PvProjects -Lab $lab -Names @('listedone')
}

# ---- C2 -------------------------------------------------------------------------------------------------
function Test-UnsupportedNoCapacity {
  param([string]$Label, [string[]]$Extra, [string[]]$Codes, [scriptblock]$Seed, [switch]$NoCaAtAll)
  $sub = @{ Name = "$where C2: $Label"; Extra = $Extra; Codes = $Codes; Seed = $Seed }
  Use-PvLab -NoCa:$NoCaAtAll -Name $sub.Name -Body {
    param($lab)
    & $sub.Seed $lab
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $hostsBefore = Get-PvHostsHash $lab
    $projectsBefore = Get-CcFileHash (Get-PvProjectsPath $lab)
    $run = Invoke-PvLease -Lab $lab -Arguments (@('create', '--project', 'nocap') + $sub.Extra) -StandIn $standIn
    $p = Get-PvUnsatisfiableDocProblems -Run $run -Project 'nocap' -Codes $sub.Codes
    foreach ($x in (Get-NoWriteProblems -Lab $lab -StandIn $standIn -HostsBefore $hostsBefore -ProjectsBefore $projectsBefore)) { $p.Add($x) }
    return , $p
  }
}
Test-UnsupportedNoCapacity -Label 'An unsupported platform for a project with no record lists unsupported_platform alone, and writes no host registry' `
  -Extra @('--platform', 'windows/amd64') -Codes @('unsupported_platform') -Seed { param($lab) }
Test-UnsupportedNoCapacity -Label 'An unsupported platform, profile and size list their three reasons in order and no no_capacity' `
  -Extra @('--platform', 'windows/amd64', '--profile', 'agent-linux@1', '--size', 'xxl') -Codes @('unsupported_platform', 'unsupported_profile', 'unsupported_size') -Seed { param($lab) }
Test-UnsupportedNoCapacity -Label 'An unsupported size reads neither file: with no CA, a host registry that is not valid and a project list that is not valid, it is still unsatisfiable with unsupported_size alone' `
  -Extra @('--size', 'xxl') -Codes @('unsupported_size') -NoCaAtAll -Seed {
  param($lab)
  Remove-PvCa -Lab $lab
  Write-PvText -Lab $lab -Path (Get-PvHostsPath $lab) -Text "this is not json`n"
  Write-PvText -Lab $lab -Path (Get-PvProjectsPath $lab) -Text "this is not json`n"
}

# ---- C17 ------------------------------------------------------------------------------------------------
function Test-LookupFails {
  param([string]$Label, [string]$Project, [scriptblock]$Seed)
  Use-PvLab -Name "$where C17: $Label" -Body {
    param($lab)
    & $Seed $lab
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $hostsBefore = Get-PvHostsHash $lab
    $projectsBefore = Get-CcFileHash (Get-PvProjectsPath $lab)
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', $Project) -StandIn $standIn
    $p = Get-PvInternalProblems -Run $run
    foreach ($x in (Get-PvNothingRecordedProblems -Lab $lab)) { $p.Add($x) }
    foreach ($x in (Get-NoWriteProblems -Lab $lab -StandIn $standIn -HostsBefore $hostsBefore -ProjectsBefore $projectsBefore)) { $p.Add($x) }
    return , $p
  }
}
Test-LookupFails -Label 'A host registry that is not JSON fails create for swimcoach with internal and records nothing' -Project 'swimcoach' -Seed {
  param($lab) Write-PvText -Lab $lab -Path (Get-PvHostsPath $lab) -Text "this is not json`n"
}
Test-LookupFails -Label 'A host registry with a version other than 0 fails a create for a project with no record' -Project 'nohost' -Seed {
  param($lab) Write-PvText -Lab $lab -Path (Get-PvHostsPath $lab) -Text ('{"version":1,"hosts":[' + $script:PvBoxRecord + ']}' + "`n")
}
Test-LookupFails -Label 'A host registry in which two records share a project, which no schema refuses, fails create' -Project 'swimcoach' -Seed {
  param($lab) Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'agentspace' -State 'ready'), (New-PvHostRecord -Project 'agentspace' -State 'creating'))
}
Test-LookupFails -Label 'A host registry with no record for the box fails create' -Project 'agentspace' -Seed {
  param($lab) Set-PvHosts -Lab $lab -NoBox -Records @((New-PvHostRecord -Project 'agentspace' -State 'ready'))
}
Test-LookupFails -Label 'A host registry whose stack names another project fails create' -Project 'agentspace' -Seed {
  param($lab) Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'agentspace' -State 'ready' -Stack 'agentspace-host-other'))
}
Test-LookupFails -Label 'A project list that is not JSON fails create for a project with no record' -Project 'nohost' -Seed {
  param($lab) Write-PvText -Lab $lab -Path (Get-PvProjectsPath $lab) -Text "this is not json`n"
}
Test-LookupFails -Label 'A project list with a name that is not normalized fails create for a project with no record' -Project 'nohost' -Seed {
  param($lab) Write-PvText -Lab $lab -Path (Get-PvProjectsPath $lab) -Text ('{"version":0,"projects":[{"name":"NoHost","approved":"2026-11-20"}]}' + "`n")
}
Test-LookupFails -Label 'A project list with two entries of one name, which no schema refuses, fails create' -Project 'nohost' -Seed {
  param($lab) Set-PvProjects -Lab $lab -Names @('nohost', 'nohost')
}
Test-LookupFails -Label 'A project list with an approved date that names no real day fails create' -Project 'nohost' -Seed {
  param($lab) Write-PvText -Lab $lab -Path (Get-PvProjectsPath $lab) -Text ('{"version":0,"projects":[{"name":"nohost","approved":"2026-02-30"}]}' + "`n")
}

# ---- C21 ------------------------------------------------------------------------------------------------
function Test-ListIgnored {
  param([string]$Label, [string]$Project, [string]$ProjectsText, [string[]]$Records, [string]$Expect)
  $sub = @{ Name = "$where C21: $Label"; Project = $Project; Text = $ProjectsText; Records = $Records; Expect = $Expect }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    Write-PvText -Lab $lab -Path (Get-PvProjectsPath $lab) -Text $sub.Text
    if ($sub.Records.Count -gt 0) { Set-PvHosts -Lab $lab -Records $sub.Records }
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $hostsBefore = Get-PvHostsHash $lab
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', $sub.Project) -StandIn $standIn
    $p = [System.Collections.Generic.List[string]]::new()
    if ($sub.Expect -ceq 'ready') {
      foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
      if ($run.Json -is [hashtable]) {
        if ($run.Json['state'] -cne 'ready') { $p.Add("The lease's state is '$($run.Json['state'])', not ready") }
        elseif ($run.Json['ssh']['host_alias'] -cnotmatch "\Aagentspace-$($sub.Project)-[0-9a-f]{8}\z") { $p.Add('The host alias is not the box''s form') }
      }
    } else {
      foreach ($x in (Get-PvPendingDocProblems -Run $run -Project $sub.Project)) { $p.Add($x) }
    }
    if ((Get-PvHostsHash $lab) -ne $hostsBefore) { $p.Add('hosts.json was written') }
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
    return , $p
  }
}
$garbage = "this is not json`n"
Test-ListIgnored -Label 'With a project list that is not valid, a create for swimcoach with no host registry answers ready' -Project 'swimcoach' -ProjectsText $garbage -Records @() -Expect 'ready'
Test-ListIgnored -Label 'With a project list that is not valid, a create for a project whose record is creating answers pending' -Project 'agentspace' -ProjectsText $garbage `
  -Records @((New-PvHostRecord -Project 'agentspace' -State 'creating' -LastAttemptAt (Get-PvAgo 60))) -Expect 'pending'
Test-ListIgnored -Label 'With a project list of two entries of one name, a create for a project whose record is creating answers pending' -Project 'agentspace' `
  -ProjectsText ('{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20"},{"name":"agentspace","approved":"2026-11-20"}]}' + "`n") `
  -Records @((New-PvHostRecord -Project 'agentspace' -State 'creating' -LastAttemptAt (Get-PvAgo 60))) -Expect 'pending'

# ---- C24 ------------------------------------------------------------------------------------------------
function Test-RecordWins {
  param([string]$Label, [string]$State, [scriptblock]$Seed)
  $sub = @{ Name = "$where C24: $Label"; State = $State; Seed = $Seed }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    & $sub.Seed $lab
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    $before = Get-CcNowSecond
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'orphan') -StandIn $standIn
    $after = Get-CcNowSecond
    $p = Get-PvPendingDocProblems -Run $run -Project 'orphan'
    foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'orphan')) { $p.Add($x) }
    $rec = Get-PvHostRecord $lab 'orphan'
    if ($null -eq $rec) { $p.Add('The record for the project is gone from hosts.json') }
    else {
      if ($rec['state'] -cne 'creating') { $p.Add("The record's state is '$($rec['state'])', not creating") }
      if (-not (Test-PvTimeBetween -Time ([string]$rec['last_attempt_at']) -Before $before -After $after)) { $p.Add('last_attempt_at is not within the command''s run') }
    }
    return , $p
  }
}
Test-RecordWins -Label 'A record creating with the project on no list (no project list at all) answers pending and starts the stand-in once' -State 'creating' -Seed {
  param($lab) Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'orphan' -State 'creating'))
}
Test-RecordWins -Label 'A record failed with a project list that does not list the project answers pending and starts the stand-in once' -State 'failed' -Seed {
  param($lab)
  Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'orphan' -State 'failed' -LastAttemptAt (Get-PvAgo 900)))
  Set-PvProjects -Lab $lab -Names @('somebodyelse')
}

# ---- C32 ------------------------------------------------------------------------------------------------
function Test-NoCa {
  param([string]$Label, [scriptblock]$Seed)
  Use-PvLab -Name "$where C32: $Label" -Body {
    param($lab)
    & $Seed $lab
    Remove-PvCa -Lab $lab
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $hostsBefore = Get-PvHostsHash $lab
    $projectsBefore = Get-CcFileHash (Get-PvProjectsPath $lab)
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
    $p = Get-PvInternalProblems -Run $run
    if ($run.ErrorJson -is [hashtable] -and $run.ErrorJson['message'] -cne $noCaMessage) { $p.Add('The message is not lease-api.md''s for a missing CA') }
    foreach ($x in (Get-PvNothingRecordedProblems -Lab $lab)) { $p.Add($x) }
    foreach ($x in (Get-NoWriteProblems -Lab $lab -StandIn $standIn -HostsBefore $hostsBefore -ProjectsBefore $projectsBefore)) { $p.Add($x) }
    if (Test-Path -LiteralPath (Join-Path $lab.UserProfile '.agentspace\ca')) { $p.Add('create made a CA folder') }
    return , $p
  }
}
Test-NoCa -Label 'With no CA, a supported create for a listed project with no record fails with internal, records nothing, writes no registry and starts nothing' -Seed {
  param($lab) Set-PvProjects -Lab $lab -Names @('agentspace')
}
Test-NoCa -Label 'With no CA, a create whose record is failed and whose run is due fails the same way, with the registry as it was' -Seed {
  param($lab) Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'agentspace' -State 'failed' -LastAttemptAt (Get-PvAgo 900)))
}

Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.11 programs, which read no `platforms` and no `platform`, and answer
# unsatisfiable with unsupported_platform for every platform but linux/arm64. A v0.12 build MUST pass every result line
# (docs/conformance.md, "The files that are expected to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what docs/lease-api.md,
# docs/driver.md and docs/state.md document, with v0.07's recording stand-in for the provisioning run (a program
# outside agentspace). See docs/conformance.md. Cases of E0.12-T04-S01 (card #0261), how a lease is placed by its
# project and its platform, each named by its row in docs/driver.md, "Conformance cases for v0.12" (D91, D101;
# architect, 2026-11-26, for v0.12-T01, pins P3, P4, P16, P22 and P46):
#   M4   A platform the list does not approve, with a host on another: create --platform linux/amd64 for a project
#        whose linux/arm64 record is ready and whose entry has no platforms answers unsatisfiable with exactly one
#        reason, no_capacity, the v0.12 message byte for byte; the same create with no entry for the project answers
#        v0.07's message, byte for byte; a create with the default platform answers ready.
#   M5   An entry whose platforms omits linux/arm64: for a project with no record, a create with the default platform
#        answers no_capacity with the v0.12 message for linux/arm64 and writes nothing to the registry; with no entry
#        the same create, and create --platform windows/amd64, answer v0.07's message.
#   M6   A platform the list approves: for a project whose linux/arm64 record is ready and whose entry approves the
#        platform, create --platform <it> answers pending with no expires_at, ssh or reasons, appends at the end of
#        hosts.json a creating record with its platform and stack, leaves the records before it byte for byte, makes
#        hosts\<project>.<os>-<arch>\ and not hosts\<project>\, and starts the stand-in once, with five arguments.
#   M12  The registry by project and platform: two records of one project on linux/arm64, one with the platform and
#        one with none, fail a supported create with internal, recording and starting nothing; the project's records
#        on linux/arm64 and on linux/amd64 are valid, and a create on each platform is placed by its own record.
#   M13  A project list that cannot be read, with a host on one platform: the default platform answers ready, while
#        create --platform linux/amd64 fails with internal, records no lease and leaves the registry as it was.
#   M14  With no hosts.json and no projects.json, create --platform linux/amd64 for swimcoach answers v0.07's
#        no_capacity message, and leaves hosts.json absent, while a create with the default platform answers ready
#        with v0.06's alias bytes.
# A stand-in that MUST NOT start is watched once, after a home's last create, and its problems join the last check that home reports,
# so that no result line passes only because a build started nothing, as v0.11 starts nothing for a platform it refuses.
# Sections enforced: driver.md "Where it places a lease" and "Starting a provisioning run"; lease-api.md "Fields";
# state.md "The host registry", "A host's platform and names" and "The project list".
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No check starts a
# real run: every create here is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records what it
# is given and makes no host. Every write calls Assert-ConformancePath at the call. A failure reason never quotes a
# value the stand-in captured.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-lease-platform-place.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 50 seconds (a stand-in that MUST NOT start is watched for 3 seconds).

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')
. (Join-Path $PSScriptRoot 'lease-platform-common.ps1')

$script:PlProject = 'platcase'
$script:PlArm = 'linux/arm64'
$where = 'driver.md#where-it-places-a-lease'

# The problems with a create that answered unsatisfiable with one reason, no_capacity, with the message given, for
# the platform asked for and its default profile, that made no lease folder or alias file and changed nothing else.
function Get-PlRefusedProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)]$Before, [Parameter(Mandatory)][string]$Message, [Parameter(Mandatory)][string]$Platform)
  $p = Get-LpUnsatProblems -Run $Run -Project $script:PlProject -Codes @('no_capacity') -Messages @($Message) -Platform $Platform -ProfileName $script:LpPlatform[$Platform].Profile
  foreach ($x in (Get-LpChangeProblems -Before $Before -After (Get-LpSnapshot $Lab) -NewRecords 1 -NewFiles 0)) { $p.Add($x) }
  return , $p
}

# ---- M4 -----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M4 ($where): With the project's linux/arm64 record ready at the m type and an entry with no platforms," -Body {
  param($lab)
  [void](Set-LpHosts -Lab $lab -Records @((New-LpHostRecord -Project $script:PlProject -Platform $script:PlArm -InstanceType $script:LpPlatform[$script:PlArm].Type['m'])))
  Set-LpProjects -Lab $lab -Entries @((New-LpProjectEntry -Name $script:PlProject))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  foreach ($plat in 'linux/amd64', 'windows/amd64') {
    $before = Get-LpSnapshot $lab
    $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -Extra @('--platform', $plat) -StandIn $standIn
    $refused = Get-PlRefusedProblems -Lab $lab -Run $run -Before $before -Message (Get-LpNotApprovedMessage $script:PlProject $plat) -Platform $plat
    $results["create --platform $plat answers unsatisfiable with exactly one reason, no_capacity, the v0.12 message for the project and $plat byte for byte, makes no lease folder or alias file and leaves the registry as it was$(if ($plat -ceq 'windows/amd64') { ', and none of the three creates starts the stand-in' })"] = $refused
  }
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -StandIn $standIn
  $p = Get-LpReadyProblems -Lab $lab -Run $run -Project $script:PlProject -Platform $script:PlArm
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
  $results['a create with the default platform answers ready'] = $p
  # The stand-in that MUST NOT start is watched once, after the last create, and its problems join the last refusal's.
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $refused.Add($x) }
  return $results
}
foreach ($arm in @(
    @{ Label = 'no projects.json at all'; Entries = $null },
    @{ Label = 'a projects.json that lists another project only'; Entries = @((New-LpProjectEntry -Name 'othercase')) })) {
  $sub = $arm
  Use-PvLab -Name "M4 ($where): With the project's linux/arm64 record ready at the m type and no entry for the project ($($sub.Label))," -Body {
    param($lab)
    [void](Set-LpHosts -Lab $lab -Records @((New-LpHostRecord -Project $script:PlProject -Platform $script:PlArm -InstanceType $script:LpPlatform[$script:PlArm].Type['m'])))
    if ($sub.Entries) { Set-LpProjects -Lab $lab -Entries $sub.Entries }
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $results = [ordered]@{}
    $before = Get-LpSnapshot $lab
    $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -Extra @('--platform', 'linux/amd64') -StandIn $standIn
    $refused = Get-PlRefusedProblems -Lab $lab -Run $run -Before $before -Message (Get-LpNoEntryMessage $script:PlProject) -Platform 'linux/amd64'
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $refused.Add($x) }
    $results['create --platform linux/amd64 answers v0.07''s no_capacity message, byte for byte, never the v0.12 one, and starts nothing'] = $refused
    return $results
  }
}

# ---- M5 -----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M5 ($where): For a project with no record, and an entry whose platforms approve linux/amd64 alone," -Body {
  param($lab)
  Set-LpProjects -Lab $lab -Entries @((New-LpProjectEntry -Name $script:PlProject -Platforms @(@{ platform = 'linux/amd64'; approved = '2026-11-26' })))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -StandIn $standIn
  $results = [ordered]@{}
  $p = Get-PlRefusedProblems -Lab $lab -Run $run -Before $before -Message (Get-LpNotApprovedMessage $script:PlProject $script:PlArm) -Platform $script:PlArm
  if (Test-Path -LiteralPath (Get-PvHostsPath $lab)) { $p.Add('hosts.json was made') }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  $results['a create with the default platform answers no_capacity with the v0.12 message for linux/arm64 byte for byte, never v0.07''s, writes nothing to the registry and starts nothing'] = $p
  return $results
}
Use-PvLab -Name "M5 ($where): For a project with no record, and no entry for it (no project list at all)," -Body {
  param($lab)
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  foreach ($arg in @(@{ Extra = @(); Platform = $script:PlArm; Key = 'a create with the default platform' }, @{ Extra = @('--platform', 'windows/amd64'); Platform = 'windows/amd64'; Key = 'create --platform windows/amd64' })) {
    $before = Get-LpSnapshot $lab
    $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -Extra $arg.Extra -StandIn $standIn
    $p = Get-PlRefusedProblems -Lab $lab -Run $run -Before $before -Message (Get-LpNoEntryMessage $script:PlProject) -Platform $arg.Platform
    if (Test-Path -LiteralPath (Get-PvHostsPath $lab)) { $p.Add('hosts.json was made') }
    if ($arg.Platform -ceq 'windows/amd64') { foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) } }
    $results["$($arg.Key) answers v0.07's message, byte for byte, as C1 does, and writes nothing to the registry$(if ($arg.Platform -ceq 'windows/amd64') { ', and neither create starts the stand-in' })"] = $p
  }
  return $results
}

# ---- M6 -----------------------------------------------------------------------------------------------------
foreach ($plat in 'windows/amd64', 'linux/amd64') {
  $sub = @{ Platform = $plat }
  Use-PvLab -Name "M6 (driver.md#starting-a-provisioning-run, steps 2 and 4): For a project whose linux/arm64 record is ready and whose entry approves $plat, create --platform $plat" -Body {
    param($lab)
    $armRecord = New-LpHostRecord -Project $script:PlProject -Platform $script:PlArm -InstanceType $script:LpPlatform[$script:PlArm].Type['m']
    [void](Set-LpHosts -Lab $lab -Records @($armRecord))
    Set-LpProjects -Lab $lab -Entries @((New-LpProjectEntry -Name $script:PlProject -Platforms @(@{ platform = 'linux/arm64'; approved = '2026-11-20' }, @{ platform = $sub.Platform; approved = '2026-11-26' })))
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    $before = Get-LpSnapshot $lab
    $start = Get-CcNowSecond
    $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -Extra @('--platform', $sub.Platform) -StandIn $standIn
    $end = Get-CcNowSecond
    $p = Get-LpPendingProblems -Lab $lab -Run $run -Project $script:PlProject -Platform $sub.Platform
    foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 0 -RegistryChanges)) { $p.Add($x) }
    foreach ($x in (Get-PvHostsFileProblems -Lab $lab)) { $p.Add($x) }
    $rec = Get-LpHostRecord -Lab $lab -Project $script:PlProject -Platform $sub.Platform
    if ($null -eq $rec) { $p.Add('hosts.json holds no record of the project on the platform') }
    else {
      $time = [string]$rec['last_attempt_at']
      if (-not (Test-PvTimeBetween -Time $time -Before $start -After $end)) { $p.Add('last_attempt_at is not within the command''s run') }
      # The file is the box's record, the arm64 record as it was, and the new record last: nothing before it moved.
      $new = New-LpHostRecord -Project $script:PlProject -Platform $sub.Platform -State 'creating' -LastAttemptAt $time
      foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected (New-LpHostsText -Records @($armRecord, $new)))) { $p.Add($x) }
    }
    if (-not (Test-Path -LiteralPath (Get-LpHostFolder -Lab $lab -Project $script:PlProject -Platform $sub.Platform) -PathType Container)) { $p.Add('The host''s folder, hosts\<project>.<os>-<arch>, was not made') }
    if (Test-Path -LiteralPath (Get-LpHostFolder -Lab $lab -Project $script:PlProject -Platform $script:PlArm)) { $p.Add('hosts\<project> was made for a host off linux/arm64') }
    foreach ($x in (Get-LpStartedOnceProblems -StandIn $standIn -Project $script:PlProject -Platform $sub.Platform)) { $p.Add($x) }
    return , $p
  }
}

# ---- M12 ----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M12 ($where, pin P16): With two records of one project on linux/arm64, one holding the platform and one with none," -Body {
  param($lab)
  $a = New-LpHostRecord -Project $script:PlProject -Platform $script:PlArm -InstanceType $script:LpPlatform[$script:PlArm].Type['m']
  $b = New-LpHostRecord -Project $script:PlProject -Platform $script:PlArm -InstanceType $script:LpPlatform[$script:PlArm].Type['m'] -ExplicitArm64
  [void](Set-LpHosts -Lab $lab -Records @($a, $b))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  foreach ($extra in @(@(), @('--platform', 'linux/amd64'))) {
    $before = Get-LpSnapshot $lab
    $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -Extra $extra -StandIn $standIn
    $p = Get-PvInternalProblems -Run $run
    foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 0 -NewFiles 0)) { $p.Add($x) }
    foreach ($x in (Get-PvNothingRecordedProblems -Lab $lab)) { $p.Add($x) }
    if ($extra.Count) { foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) } }
    $results["a supported create$(if ($extra.Count) { ' --platform linux/amd64' } else { ' with the default platform' }) fails with internal, records nothing and leaves the registry as it was$(if ($extra.Count) { ', and neither create starts the stand-in' })"] = $p
  }
  return $results
}
Use-PvLab -Name "M12 ($where, pin P16): With one record of the project on linux/amd64, before one on linux/arm64, both ready, a valid file," -Body {
  param($lab)
  [void](Set-LpHosts -Lab $lab -Records @(
      (New-LpHostRecord -Project $script:PlProject -Platform 'linux/amd64' -InstanceType $script:LpPlatform['linux/amd64'].Type['m']),
      (New-LpHostRecord -Project $script:PlProject -Platform $script:PlArm -InstanceType $script:LpPlatform[$script:PlArm].Type['m'])))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  foreach ($plat in 'linux/amd64', $script:PlArm) {
    $before = Get-LpSnapshot $lab
    $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -Extra @('--platform', $plat) -StandIn $standIn
    $p = Get-LpReadyProblems -Lab $lab -Run $run -Project $script:PlProject -Platform $plat
    foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
    if ($plat -ceq $script:PlArm) { foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) } }
    $results["a create --platform $plat is placed by its own record: it answers ready, and its alias file names that host's host-key name$(if ($plat -ceq $script:PlArm) { ', and neither create starts the stand-in' })"] = $p
  }
  return $results
}

# ---- M13 ----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M13 ($where): With the project's linux/arm64 record ready at the m type and a projects.json that does not validate," -Body {
  param($lab)
  [void](Set-LpHosts -Lab $lab -Records @((New-LpHostRecord -Project $script:PlProject -Platform $script:PlArm -InstanceType $script:LpPlatform[$script:PlArm].Type['m'])))
  Write-PvText -Lab $lab -Path (Get-PvProjectsPath $lab) -Text "this is not json`n"
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -StandIn $standIn
  $p = Get-LpReadyProblems -Lab $lab -Run $run -Project $script:PlProject -Platform $script:PlArm
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
  $results['a create with the default platform answers ready, the project list not being read for a project that has a host there'] = $p
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project $script:PlProject -Extra @('--platform', 'linux/amd64') -StandIn $standIn
  $p = Get-PvInternalProblems -Run $run
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 0 -NewFiles 0)) { $p.Add($x) }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  $results['create --platform linux/amd64 fails with internal, records no lease, leaves the registry as it was, and neither create starts the stand-in'] = $p
  return $results
}

# ---- M14 ----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M14 ($where): With no hosts.json and no projects.json," -Body {
  param($lab)
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $before = Get-LpSnapshot $lab
  $run = Invoke-LpCreate -Lab $lab -Project 'swimcoach' -Extra @('--platform', 'linux/amd64') -StandIn $standIn
  $p = Get-LpUnsatProblems -Run $run -Project 'swimcoach' -Codes @('no_capacity') -Messages @((Get-LpNoEntryMessage 'swimcoach')) -Platform 'linux/amd64' -ProfileName 'agent-linux'
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 0)) { $p.Add($x) }
  if (Test-Path -LiteralPath (Get-PvHostsPath $lab)) { $p.Add('hosts.json was made') }
  $results['create --platform linux/amd64 for swimcoach answers v0.07''s no_capacity message for swimcoach byte for byte, since no entry names it, leaves hosts.json absent, and neither create starts the stand-in'] = $p
  $refusal = $p
  $run = Invoke-LpCreate -Lab $lab -Project 'swimcoach' -StandIn $standIn
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
  if ($run.Json -is [hashtable] -and $run.Json['state'] -ceq 'ready' -and $run.Json['ssh'] -is [hashtable]) {
    if ($run.Json['platform'] -cne $script:PlArm -or $run.Json['profile'] -cne 'agent-linux' -or $run.Json['ssh']['user'] -cne 'ubuntu') { $p.Add('The lease is not linux/arm64 with agent-linux and ubuntu') }
    $alias = [string]$run.Json['ssh']['host_alias']
    if ($alias -cnotmatch '\Aagentspace-swimcoach-[0-9a-f]{8}\z') { $p.Add('The host alias is not the project''s form') }
    else {
      $text = [IO.File]::ReadAllText((Get-CcAliasPath $lab $alias))
      if (-not $text.Contains("`n  HostKeyAlias agentspace`n")) { $p.Add('The alias file does not hold v0.06''s HostKeyAlias agentspace line') }
      if ($text.Contains('UserKnownHostsFile') -or $text.Contains('agentspace-host.')) { $p.Add('The alias file names a host other than the box') }
    }
  } else { $p.Add('The create did not answer ready') }
  if (Test-Path -LiteralPath (Get-PvHostsPath $lab)) { $p.Add('hosts.json was made') }
  $results['a create for swimcoach with the default platform answers ready with v0.06''s alias bytes, as Z15, and leaves hosts.json absent'] = $p
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $refusal.Add($x) }
  return $results
}

Exit-ConformanceRun

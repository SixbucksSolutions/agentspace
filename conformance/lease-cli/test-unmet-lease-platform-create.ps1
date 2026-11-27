#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.11 programs, which answer unsatisfiable for linux/amd64 and
# windows/amd64: every create here that is met answers unsatisfiable there. A v0.12 build MUST pass every result
# line (docs/conformance.md, "The files that are expected to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what docs/lease-api.md,
# docs/driver.md and docs/state.md document, with v0.07's recording stand-in for the provisioning run (a program
# outside agentspace). See docs/conformance.md. Cases of E0.12-T04-S01 (card #0261), the creates that v0.12 meets,
# each named by its row in docs/driver.md, "Conformance cases for v0.12" (D91, D93, D94, D95):
#   M1   For each of linux/amd64 and windows/amd64, with no --profile, a create for a project whose record on that
#        platform is ready at the platform's m type, with no other lease, answers ready, exit 0, a document that
#        validates, with the platform asked for, the platform's default profile and its user, an alias file that logs
#        in as the user and names the host's own host-key name; it starts nothing and leaves the registry as it was.
#   M15  A windows/amd64 lease made ready as M1 does: renew answers a new expires_at with ssh.user still agent, and
#        release succeeds, removes the alias file and the lease folder and appends one release.log line, with a
#        hosts.json that is not valid, since neither reads it.
#   LP1  (lease-api.md, "Size": the four sizes are supported on every supported platform; D93, D94.) For each of the
#        two new platforms and each of s, m, l and xl, a record ready at that size's type answers ready for it.
#   LP2  (lease-api.md, "Profile", pin P25: a stored record is never re-derived.) A stored unsatisfiable lease on
#        windows/amd64 with agent-linux, as v0.11 wrote it, is printed by get and list as it was stored.
# A stand-in that MUST NOT start is watched once, after a home's last create, and its problems join the last check that home reports,
# so that no result line passes only because a build started nothing, as v0.11 starts nothing for a platform it refuses.
# Sections enforced: driver.md "What it supports", "Sizes and instance types", "Where it places a lease", "On a ready
# host" and "Renew and release"; lease-api.md "Platform", "Profile", "Size" and "Fields"; state.md "The host registry",
# "A host's platform and names" and "A lease alias for a host that is not the box".
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No check starts a
# real run: every create here is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records what it
# is given and makes no host. No program of agentspace's but the lease CLI is started, only through the harness.
# Every write calls Assert-ConformancePath at the call, through the helpers of provision-standin.ps1,
# lease-cli-common.ps1 and lease-platform-common.ps1. A failure reason never quotes a value the stand-in captured.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-lease-platform-create.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 35 seconds.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')
. (Join-Path $PSScriptRoot 'lease-platform-common.ps1')

# A project of the cases' own, other than swimcoach (driver.md, "Conformance cases for v0.12").
$script:CpProject = 'platcase'
$where = 'driver.md#where-it-places-a-lease'

# ---- M1 -----------------------------------------------------------------------------------------------------
foreach ($plat in 'linux/amd64', 'windows/amd64') {
  $sub = @{ Platform = $plat }
  Use-PvLab -Name "M1 ($where): For $plat, with no --profile, a create for a project whose record there is ready at the m type answers ready with the platform's default profile and user, an alias file naming the host's own host-key name, starts nothing and leaves the registry as it was" -Body {
    param($lab)
    [void](Set-LpHosts -Lab $lab -Records @((New-LpHostRecord -Project $script:CpProject -Platform $sub.Platform -InstanceType $script:LpPlatform[$sub.Platform].Type['m'])))
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $before = Get-LpSnapshot $lab
    $run = Invoke-LpCreate -Lab $lab -Project $script:CpProject -Extra @('--platform', $sub.Platform) -StandIn $standIn
    $p = Get-LpReadyProblems -Lab $lab -Run $run -Project $script:CpProject -Platform $sub.Platform
    foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
    return , $p
  }
}

# ---- M15 ----------------------------------------------------------------------------------------------------
Use-PvLab -Name "M15 (driver.md#renew-and-release, D95): A windows/amd64 lease made ready as M1 does is renewed with ssh.user still agent, and released, which removes its alias file and folder and appends one release.log line, with a hosts.json that is not valid, since neither reads it" -Body {
  param($lab)
  [void](Set-LpHosts -Lab $lab -Records @((New-LpHostRecord -Project $script:CpProject -Platform 'windows/amd64' -InstanceType $script:LpPlatform['windows/amd64'].Type['m'])))
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $made = Invoke-LpCreate -Lab $lab -Project $script:CpProject -Extra @('--platform', 'windows/amd64') -StandIn $standIn
  $p = Get-LpReadyProblems -Lab $lab -Run $made -Project $script:CpProject -Platform 'windows/amd64'
  if ($made.Json -isnot [hashtable] -or $made.Json['state'] -cne 'ready' -or $p.Count -gt 0) { $p.Insert(0, 'The windows/amd64 lease was not made'); return , $p }
  $id = [string]$made.Json['id']
  $alias = [string]$made.Json['ssh']['host_alias']
  $aliasFile = Get-CcAliasPath $lab $alias
  $leaseDir = Get-CcLeaseDir $lab $id
  # The check makes the registry not valid after the create: neither renew nor release reads it.
  $garbage = "this is not json`n"
  Write-PvText -Lab $lab -Path (Get-PvHostsPath $lab) -Text $garbage
  $hostsBefore = Get-PvHostsHash $lab
  $logPath = Join-Path $lab.StateDir 'release.log'
  $logBefore = if (Test-Path -LiteralPath $logPath -PathType Leaf) { @([IO.File]::ReadAllLines($logPath)).Count } else { 0 }
  # The renew's expires_at is its own issue time plus the lease's TTL (lease-api.md, "Times"), read against this check's clock
  # around the renew, never against the create's output. The wait keeps the create's expiry outside that window, slack included.
  Start-Sleep -Milliseconds 2200
  $renewStart = Get-CcNowSecond
  $renew = Invoke-PvLease -Lab $lab -Arguments @('renew', $id) -StandIn $standIn
  $renewEnd = Get-CcNowSecond
  foreach ($x in (Get-ConformanceSuccessProblems -Run $renew)) { $p.Add("Renew: $x") }
  if ($renew.Json -is [hashtable]) {
    if ($renew.Json['state'] -cne 'ready') { $p.Add('Renew: the lease is not ready') }
    if ($renew.Json['ssh'] -isnot [hashtable] -or $renew.Json['ssh']['user'] -cne 'agent') { $p.Add('Renew: ssh.user is not agent') }
    if ($renew.Json['platform'] -cne 'windows/amd64' -or $renew.Json['profile'] -cne 'agent-windows') { $p.Add('Renew: the platform or the profile changed') }
    if ($renew.Json['ttl_seconds'] -ne $made.Json['ttl_seconds']) { $p.Add('Renew: ttl_seconds is not the lease''s') }
    $ttl = [double]$renew.Json['ttl_seconds']
    if (-not (Test-PvTimeBetween -Time ([string]$renew.Json['expires_at']) -Before $renewStart.AddSeconds($ttl) -After $renewEnd.AddSeconds($ttl))) { $p.Add('Renew: expires_at is not the renew''s issue time plus the lease''s TTL') }
  }
  $release = Invoke-PvLease -Lab $lab -Arguments @('release', $id) -StandIn $standIn
  foreach ($x in (Get-ConformanceSuccessProblems -Run $release)) { $p.Add("Release: $x") }
  if ($release.Json -is [hashtable] -and $release.Json['state'] -cne 'released') { $p.Add('Release: the lease is not released') }
  if (Test-Path -LiteralPath $aliasFile) { $p.Add('Release: the alias file is still there') }
  if (Test-Path -LiteralPath $leaseDir) { $p.Add('Release: the lease folder is still there') }
  $lines = @(if (Test-Path -LiteralPath $logPath -PathType Leaf) { [IO.File]::ReadAllLines($logPath) })
  if ($lines.Count -ne $logBefore + 1) { $p.Add("Release: release.log gained $($lines.Count - $logBefore) lines, not 1") }
  elseif ($lines[$lines.Count - 1] -cnotmatch "\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z lease=$([regex]::Escape($id)) ") { $p.Add('Release: the release.log line is not a time, the lease ID and a text') }
  if ((Get-PvHostsHash $lab) -cne $hostsBefore) { $p.Add('hosts.json was written by a renew or a release') }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  return , $p
}

# ---- LP1 -----------------------------------------------------------------------------------------------------
# A record ready at exactly the size's type places the lease ready whatever other leases hold the host (row 3), so one
# home serves the four sizes of a platform, the record being rewritten to each size's type before its create.
foreach ($plat in 'linux/amd64', 'windows/amd64') {
  $sub = @{ Platform = $plat }
  Use-PvLab -Name "LP1 (lease-api.md#size, D93, D94): For $($sub.Platform)," -Body {
    param($lab)
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $results = [ordered]@{}
    foreach ($size in 's', 'm', 'l', 'xl') {
      [void](Set-LpHosts -Lab $lab -Records @((New-LpHostRecord -Project $script:CpProject -Platform $sub.Platform -InstanceType $script:LpPlatform[$sub.Platform].Type[$size])))
      $before = Get-LpSnapshot $lab
      $run = Invoke-LpCreate -Lab $lab -Project $script:CpProject -Extra @('--platform', $sub.Platform, '--size', $size) -StandIn $standIn
      $p = Get-LpReadyProblems -Lab $lab -Run $run -Project $script:CpProject -Platform $sub.Platform -Size $size
      foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 1)) { $p.Add($x) }
      if ($run.Json -is [hashtable] -and $run.Stdout -match '(?i)m8a|xlarge|\.large|instance') { $p.Add('The document names a machine type') }
      # The stand-in that MUST NOT start is watched once, after the last create, and its problems join the last check's.
      if ($size -ceq 'xl') { foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) } }
      $results["create --size $size, with a record ready at its type, answers ready with the size asked for$(if ($size -ceq 'xl') { ', and none of the four creates starts the stand-in' })"] = $p
    }
    return $results
  }
}

# ---- LP2 -----------------------------------------------------------------------------------------------------
# A lease created before v0.12 with --platform windows/amd64 and no --profile is stored unsatisfiable with agent-linux
# and v0's message. v0.12 prints it as it was stored: the default is chosen at create and never re-derived (pin P25).
Use-PvLab -NoCa -Name "LP2 (lease-api.md#profile, pin P25): A stored unsatisfiable windows/amd64 lease with agent-linux and v0's message is printed by get and by list as it was stored, never re-derived as agent-windows or met" -Body {
  param($lab)
  $id = [guid]::NewGuid().ToString()
  $stored = [ordered]@{
    id = $id; project = $script:CpProject; state = 'unsatisfiable'; platform = 'windows/amd64'; profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800
    created_at = (Get-LpTime -300)
    reasons = @([ordered]@{ code = 'unsupported_platform'; message = "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only." })
  }
  $record = [ordered]@{}
  foreach ($k in $stored.Keys) { $record[$k] = $stored[$k] }
  $record['driver'] = $script:LpDriver
  Set-LpLeases -Lab $lab -Records @($record)
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $before = Get-LpSnapshot $lab
  $want = ConvertTo-CcCanonical $stored
  $p = [System.Collections.Generic.List[string]]::new()
  $get = Invoke-PvLease -Lab $lab -Arguments @('get', $id) -StandIn $standIn
  foreach ($x in (Get-ConformanceSuccessProblems -Run $get)) { $p.Add("Get: $x") }
  if ($get.Json -is [hashtable] -and (ConvertTo-CcCanonical $get.Json) -cne $want) { $p.Add('Get: the document is not the stored record, member for member') }
  $list = Invoke-PvLease -Lab $lab -Arguments @('list') -StandIn $standIn
  foreach ($x in (Get-ConformanceSuccessProblems -Run $list -Schema 'lease-list.output.schema.json')) { $p.Add("List: $x") }
  if ($list.Json -is [hashtable]) {
    $items = @($list.Json['leases'])
    if ($items.Count -ne 1 -or (ConvertTo-CcCanonical $items[0]) -cne $want) { $p.Add('List: the one lease is not the stored record, member for member') }
  }
  foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab))) { $p.Add($x) }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  return , $p
}

Exit-ConformanceRun

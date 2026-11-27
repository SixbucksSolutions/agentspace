#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Helpers shared by the lease CLI's v0.12 platform checks (E0.12-T04-S01, card #0261; docs/driver.md,
# "Conformance cases for v0.12", cases M1 on). A check dot-sources harness.ps1, lease-cli-common.ps1,
# lease-check-support.ps1 and provision-standin.ps1 first, and this file last. Every function here is
# prefixed Lp, and none is named like a PowerShell alias.
#
# - It starts no program. The lease CLI is started only through the harness (Invoke-PvLease, which calls
#   Invoke-AgentspaceCli), and the one program placed is v0.07's recording stand-in for a provisioning run
#   (provision-standin.ps1), which makes no host. No check reaches AWS, a real host or a board.
# - Every expectation below is taken from the spec text, never from a program's output: the messages from
#   docs/lease-api.md "Fields", the platforms, profiles, users and sizes from docs/driver.md "What it supports" and
#   "Sizes and instance types", the record forms from docs/state.md "The host registry" and "The project list".
# - Every write calls Assert-ConformancePath at the call, through the helpers of provision-standin.ps1.
# - A failure reason never quotes a value a stand-in captured, and quotes a program's output only through the
#   shared mask (Complete-ConformanceCheck, Add-CcResult and Get-ConformanceSuccessProblems do).

# The EC2 driver's name, as state.md gives it (the lease state file). A stored record of the driver names it.
$script:LpDriver = 'ec2'

# The three platforms (docs/driver.md, "What it supports"): the profile with no version, the login user, and
# the four sizes' types ("Sizes and instance types").
$script:LpPlatform = @{
  'linux/arm64'   = @{ Profile = 'agent-linux'; User = 'ubuntu'; Type = @{ s = 'm9g.large'; m = 'm9g.xlarge'; l = 'm9g.2xlarge'; xl = 'm9g.4xlarge' } }
  'linux/amd64'   = @{ Profile = 'agent-linux'; User = 'ubuntu'; Type = @{ s = 'm8a.large'; m = 'm8a.xlarge'; l = 'm8a.2xlarge'; xl = 'm8a.4xlarge' } }
  'windows/amd64' = @{ Profile = 'agent-windows'; User = 'agent'; Type = @{ s = 'm8a.large'; m = 'm8a.xlarge'; l = 'm8a.2xlarge'; xl = 'm8a.4xlarge' } }
}

# --- The messages, byte for byte (docs/lease-api.md, "Fields"; docs/driver.md, "What it supports" and "Where it places a lease")

function Get-LpPlatformMessage { param([Parameter(Mandatory)][string]$Platform) "Platform '$Platform' is not supported; agentspace supports linux/arm64, linux/amd64 and windows/amd64." }
function Get-LpProfileMessage { param([Parameter(Mandatory)][string]$ProfileName, [Parameter(Mandatory)][string]$Platform) "Profile '$ProfileName' is not supported on platform '$Platform'; agentspace supports agent-linux on linux/arm64 and linux/amd64, and agent-windows on windows/amd64, each with no version only." }
function Get-LpSizeMessage { param([Parameter(Mandatory)][string]$Size) "Size '$Size' is not supported; v0 supports sizes s, m, l and xl." }
# v0.07's message, for a project that no entry of the project list names, on any platform.
function Get-LpNoEntryMessage { param([Parameter(Mandatory)][string]$Project) "No capacity can be found for project '$Project': it has no host and is not on agentspace's project list." }
# v0.12's, for a project that has an entry that does not approve the lease's platform, and no host there.
function Get-LpNotApprovedMessage { param([Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform) "No capacity can be found for project '$Project' on platform '$Platform': it has no host there and is not approved for that platform on agentspace's project list." }
# D66's, for a host serving other leases at a smaller size.
function Get-LpSmallerMessage { param([Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Size) "No capacity can be found for project '$Project' at size '$Size': its host is serving other leases at a smaller size. Release them, or ask for a size they fit." }

# --- Names (docs/state.md, "A host's platform and names") ---------------------------------------------------

# A host's name: the project on linux/arm64, <project>.<os>-<arch> on any other platform.
function Get-LpHostName {
  param([Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform)
  if ($Platform -ceq 'linux/arm64') { return $Project }
  return "$Project.$($Platform -replace '/', '-')"
}
function Get-LpStack {
  param([Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform)
  if ($Platform -ceq 'linux/arm64') { return "agentspace-host-$Project" }
  return "agentspace-$($Platform -replace '/', '-')-host-$Project"
}
# A host's folder under the home's .agentspace\hosts, and its provision.lock.
function Get-LpHostFolder {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform)
  Join-Path $Lab.UserProfile ".agentspace\hosts\$(Get-LpHostName -Project $Project -Platform $Platform)"
}
function Get-LpProvisionLock {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform)
  Join-Path (Get-LpHostFolder -Lab $Lab -Project $Project -Platform $Platform) 'provision.lock'
}

# --- What the cases write ------------------------------------------------------------------------------------

# One host record in state.md's order: project, platform, stack, state, instance_id, instance_type, last_attempt_at.
# The platform is written for a host off linux/arm64, and for linux/arm64 only with -ExplicitArm64, which only a
# person's edit makes (pin P16).
function New-LpHostRecord {
  param(
    [Parameter(Mandatory)][string]$Project, [string]$Platform = 'linux/arm64', [ValidateSet('creating', 'ready', 'failed')][string]$State = 'ready',
    [string]$InstanceType, [string]$InstanceId, [string]$LastAttemptAt, [switch]$ExplicitArm64
  )
  $s = '{"project":"' + $Project + '"'
  if ($Platform -cne 'linux/arm64' -or $ExplicitArm64) { $s += ',"platform":"' + $Platform + '"' }
  $s += ',"stack":"' + (Get-LpStack -Project $Project -Platform $Platform) + '","state":"' + $State + '"'
  if ($InstanceId) { $s += ',"instance_id":"' + $InstanceId + '"' }
  if ($InstanceType) { $s += ',"instance_type":"' + $InstanceType + '"' }
  if ($LastAttemptAt) { $s += ',"last_attempt_at":"' + $LastAttemptAt + '"' }
  return $s + '}'
}

# The host registry's text: the box's record, then the records given, compact, with one line feed.
function New-LpHostsText {
  param([string[]]$Records = @())
  return '{"version":0,"hosts":[' + ((@($script:PvBoxRecord) + @($Records)) -join ',') + ']}' + "`n"
}

# Writes hosts.json and returns its text. A registry that does not validate against its schema is the case's
# mistake, so it throws. (A registry that breaks state.md's rule 1 only is written all the same: the schema cannot
# refuse it, and a check of that rule wants it written.)
function Set-LpHosts {
  param([Parameter(Mandatory)]$Lab, [string[]]$Records = @())
  $text = New-LpHostsText -Records $Records
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/hosts.state.schema.json')) { throw 'The case wrote a hosts.json that does not validate against its schema.' }
  Write-PvText -Lab $Lab -Path (Get-PvHostsPath $Lab) -Text $text
  return $text
}

# One project list entry (docs/state.md, "The project list"): name, approved, and platforms only when given, each
# as a hashtable of platform and approved, in that order.
function New-LpProjectEntry {
  param([Parameter(Mandatory)][string]$Name, [string]$Approved = '2026-11-20', [hashtable[]]$Platforms)
  $s = '{"name":"' + $Name + '","approved":"' + $Approved + '"'
  if ($Platforms) {
    $items = foreach ($i in $Platforms) { '{"platform":"' + $i.platform + '","approved":"' + $i.approved + '"}' }
    $s += ',"platforms":[' + (@($items) -join ',') + ']'
  }
  return $s + '}'
}

# Writes projects.json with the entries given, and throws when it does not validate against its schema.
function Set-LpProjects {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Entries)
  $text = '{"version":0,"projects":[' + ($Entries -join ',') + ']}' + "`n"
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/projects.state.schema.json')) { throw 'The case wrote a projects.json that does not validate against its schema.' }
  Write-PvText -Lab $Lab -Path (Get-PvProjectsPath $Lab) -Text $text
}

# One stored lease record (state.md, "The lease state file"), in the form of its state, for the platform given: its
# profile is the platform's own, and a ready record names the platform's user in its ssh.
function New-LpLeaseRecord {
  param(
    [Parameter(Mandatory)][ValidateSet('pending', 'ready', 'released', 'unsatisfiable')][string]$State, [Parameter(Mandatory)][string]$Project,
    [string]$Platform = 'linux/arm64', [string]$Size = 'm', [int]$CreatedAgo = 60, [string]$ExpiresAt, [string]$ReleasedAt
  )
  $id = [guid]::NewGuid().ToString()
  $r = [ordered]@{
    id = $id; project = $Project; state = $State; platform = $Platform; profile = $script:LpPlatform[$Platform].Profile
    size = $Size; ttl_seconds = 3600; created_at = (ConvertTo-CcTime ([DateTime]::UtcNow.AddSeconds(-$CreatedAgo)))
  }
  if ($ExpiresAt) { $r['expires_at'] = $ExpiresAt }
  if ($ReleasedAt) { $r['released_at'] = $ReleasedAt }
  if ($State -ceq 'ready') { $r['ssh'] = [ordered]@{ host_alias = "agentspace-$Project-$($id.Substring(0, 8))"; user = $script:LpPlatform[$Platform].User } }
  $r['driver'] = $script:LpDriver
  return $r
}

# Writes the lease state file with the records given, and throws when it does not validate.
function Set-LpLeases {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  Write-CcState $Lab ([ordered]@{ version = 0; leases = @($Records) })
  if (-not (Test-CcStateFile $Lab)) { throw 'The case wrote a lease state file that does not validate against its schema.' }
}

# A time $Seconds from now, in the contract's form.
function Get-LpTime { param([double]$Seconds) ConvertTo-CcTime ([DateTime]::UtcNow.AddSeconds($Seconds)) }

# --- Running the lease CLI -----------------------------------------------------------------------------------

function Invoke-LpCreate {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project, [string[]]$Extra = @(), $StandIn, [hashtable]$EnvVars = @{})
  Invoke-PvLease -Lab $Lab -Arguments (@('create', '--project', $Project) + $Extra) -StandIn $StandIn -EnvVars $EnvVars
}

# --- What a case compares ------------------------------------------------------------------------------------

# The files a command may change, as they are: the hashes of hosts.json and projects.json, each stored record as
# a canonical text by its ID, and the names of the lease folders and of the alias files.
function Get-LpSnapshot {
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

# The problems with what a command changed since $Before: the registry and the project list as they were, every
# record that was there as it was, and exactly $NewRecords records, $NewFiles lease folders and $NewFiles alias
# files added.
function Get-LpChangeProblems {
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

# --- Assertions shared by the checks -------------------------------------------------------------------------

# The problems with a stored lease: it is in the lease state file in the state given, with the platform, the
# profile the platform's own (or the profile asked for), the size, and a driver, and the file validates against its schema.
function Get-LpStoredProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$State, [Parameter(Mandatory)][string]$Platform, [Parameter(Mandatory)][string]$ProfileName, [string]$Size = 'm')
  $p = [System.Collections.Generic.List[string]]::new()
  $rec = Get-CcRecord $Lab $Id
  if ($null -eq $rec) { $p.Add('The lease is not in the lease state file'); return , $p }
  if ($rec['state'] -cne $State) { $p.Add("The stored lease is '$(ConvertTo-ConformanceMasked $rec['state'])', not $State") }
  if ($rec['platform'] -cne $Platform) { $p.Add("The stored lease's platform is '$(ConvertTo-ConformanceMasked $rec['platform'])', not $Platform") }
  if ($rec['profile'] -cne $ProfileName) { $p.Add("The stored lease's profile is '$(ConvertTo-ConformanceMasked $rec['profile'])', not $ProfileName") }
  if ($rec['size'] -cne $Size) { $p.Add('The stored lease''s size is not the size asked for') }
  if ([string]::IsNullOrEmpty([string]$rec['driver'])) { $p.Add('The stored lease carries no driver') }
  if (-not (Test-CcStateFile $Lab)) { $p.Add('The lease state file does not validate against its schema') }
  return , $p
}

# The problems with a run that answered ready for a lease on $Platform: the document validates, names the
# platform, its profile and its user ssh.user, the alias in the project's form on every platform, a stored record
# with the same, and an alias file that logs in as the user and names the host's own host-key name (state.md, "A
# lease alias for a host that is not the box"; docs/driver.md, "What it supports" and "Where it places a lease").
function Get-LpReadyProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform, [string]$Size = 'm')
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $Run)) { $p.Add($x) }
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  if ($doc['state'] -cne 'ready') { $p.Add("The lease's state is '$(ConvertTo-ConformanceMasked $doc['state'])', not ready"); return , $p }
  $want = $script:LpPlatform[$Platform]
  if ($doc['project'] -cne $Project) { $p.Add('The lease is for another project') }
  if ($doc['platform'] -cne $Platform) { $p.Add("The lease's platform is '$(ConvertTo-ConformanceMasked $doc['platform'])', not $Platform") }
  if ($doc['profile'] -cne $want.Profile) { $p.Add("The lease's profile is '$(ConvertTo-ConformanceMasked $doc['profile'])', not $($want.Profile)") }
  if ($doc['size'] -cne $Size) { $p.Add("The lease's size is '$(ConvertTo-ConformanceMasked $doc['size'])', not $Size") }
  if ($doc['ssh'] -isnot [hashtable]) { $p.Add('The lease has no ssh member'); return , $p }
  if ($doc['ssh']['user'] -cne $want.User) { $p.Add("ssh.user is '$(ConvertTo-ConformanceMasked $doc['ssh']['user'])', not $($want.User)") }
  $alias = [string]$doc['ssh']['host_alias']
  if ($alias -cnotmatch "\Aagentspace-$([regex]::Escape($Project))-[0-9a-f]{8}\z") { $p.Add('The host alias is not the project''s form, which is the same on every platform'); return , $p }
  foreach ($x in (Get-LpStoredProblems -Lab $Lab -Id ([string]$doc['id']) -State 'ready' -Platform $Platform -ProfileName $want.Profile -Size $Size)) { $p.Add($x) }
  $rec = Get-CcRecord $Lab ([string]$doc['id'])
  if ($rec -and $rec['ssh']['user'] -cne $want.User) { $p.Add('The stored lease''s ssh.user is not the platform''s user') }
  $file = Get-CcAliasPath $Lab $alias
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { $p.Add('There is no alias file for the lease'); return , $p }
  $text = [IO.File]::ReadAllText($file)
  if (-not $text.Contains("`n  User $($want.User)`n")) { $p.Add("The alias file does not log in as $($want.User)") }
  $keyName = "agentspace-host.$(Get-LpHostName -Project $Project -Platform $Platform)"
  if (-not $text.Contains("`n  HostKeyAlias $keyName`n  UserKnownHostsFile `"")) { $p.Add("The alias file does not name the host-key name $keyName, with the UserKnownHostsFile line straight after it") }
  return , $p
}

# The problems with a run that answered unsatisfiable with exactly the reasons given (codes and messages, in
# order), and the platform, profile and size the document prints. $Messages are byte for byte from the spec.
function Get-LpUnsatProblems {
  param(
    [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string[]]$Codes, [Parameter(Mandatory)][string[]]$Messages,
    [string]$Platform, [string]$ProfileName, [string]$Size
  )
  $p = Get-PvUnsatisfiableDocProblems -Run $Run -Project $Project -Codes $Codes
  $doc = $Run.Json
  if ($doc -is [hashtable]) {
    $got = @($doc['reasons'] | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_['message'] })
    for ($i = 0; $i -lt $Messages.Count; $i++) {
      if ($i -ge $got.Count) { $p.Add("There is no reason $($i + 1), wanted code $($Codes[$i])"); continue }
      if ($got[$i] -cne $Messages[$i]) { $p.Add("The $($Codes[$i]) message is not the spec's byte for byte: '$(ConvertTo-ConformanceMasked $got[$i])'") }
    }
    if ($Platform -and $doc['platform'] -cne $Platform) { $p.Add("The document's platform is '$(ConvertTo-ConformanceMasked $doc['platform'])', not $Platform") }
    if ($ProfileName -and $doc['profile'] -cne $ProfileName) { $p.Add("The document's profile is '$(ConvertTo-ConformanceMasked $doc['profile'])', not $ProfileName") }
    if ($Size -and $doc['size'] -cne $Size) { $p.Add("The document's size is '$(ConvertTo-ConformanceMasked $doc['size'])', not $Size") }
    if ($doc.ContainsKey('expires_at') -or $doc.ContainsKey('ssh')) { $p.Add('An unsatisfiable lease carries expires_at or ssh') }
  }
  return , $p
}

# The problems with a run that answered pending for a lease on $Platform: the pending document, with the platform
# and its profile the platform's own, and a stored pending record of the same.
function Get-LpPendingProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform, [string]$Size = 'm')
  $p = Get-PvPendingDocProblems -Run $Run -Project $Project
  $doc = $Run.Json
  if ($doc -is [hashtable]) {
    if ($doc['platform'] -cne $Platform) { $p.Add("The lease's platform is '$(ConvertTo-ConformanceMasked $doc['platform'])', not $Platform") }
    if ($doc['profile'] -cne $script:LpPlatform[$Platform].Profile) { $p.Add('The lease''s profile is not the platform''s own') }
    foreach ($x in (Get-LpStoredProblems -Lab $Lab -Id ([string]$doc['id']) -State 'pending' -Platform $Platform -ProfileName $script:LpPlatform[$Platform].Profile -Size $Size)) { $p.Add($x) }
  }
  return , $p
}

# The problems with a stand-in that MUST have been started exactly once, and to have received exactly the
# arguments of a run for the platform: provision, -Host and the project, and off linux/arm64 -Platform and the
# normalized platform after them ("five arguments"; docs/driver.md, "Starting a provisioning run"). A check asserts
# the arguments as the program received them, never the line's quoting.
function Get-LpStartedOnceProblems {
  param([Parameter(Mandatory)]$StandIn, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform, [int]$StartWaitSeconds = 10)
  $p = [System.Collections.Generic.List[string]]::new()
  $n = Wait-PvStarted -StandIn $StandIn -Count 1 -TimeoutSeconds $StartWaitSeconds
  if ($n -lt 1) { $p.Add("The stand-in was not started within $StartWaitSeconds seconds"); return , $p }
  $calls = @(Wait-PvCalls -StandIn $StandIn -Count 1 -TimeoutSeconds 40)
  if ($calls.Count -lt 1) { $p.Add('The stand-in was started but wrote no record within 40 seconds') }
  Start-Sleep -Seconds 2
  $lines = @(Get-PvStartedLines $StandIn)
  if ($lines.Count -ne 1) { $p.Add("The stand-in was started $($lines.Count) times, not once") }
  if ($calls.Count -ge 1) {
    $want = @('provision', '-Host', $Project)
    if ($Platform -cne 'linux/arm64') { $want += @('-Platform', $Platform) }
    $argv = @($calls[0]['args'])
    if (($argv -join '|') -cne ($want -join '|')) { $p.Add("The stand-in's arguments are not exactly $($want.Count), provision -Host <project>$(if ($want.Count -eq 5) { ' -Platform <platform>' }): it got $($argv.Count) arguments") }
  }
  return , $p
}

# The record of the project on the platform in hosts.json, as a hashtable, or $null. A record with no platform
# member is linux/arm64's.
function Get-LpHostRecord {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Platform)
  $path = Get-PvHostsPath $Lab
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
  $doc = ConvertFrom-CcJson ([IO.File]::ReadAllText($path))
  if ($doc -isnot [hashtable]) { return $null }
  foreach ($r in @($doc['hosts'])) {
    $has = if ($r.ContainsKey('platform')) { [string]$r['platform'] } else { 'linux/arm64' }
    if ($r['project'] -ceq $Project -and $has -ceq $Platform) { return $r }
  }
  return $null
}

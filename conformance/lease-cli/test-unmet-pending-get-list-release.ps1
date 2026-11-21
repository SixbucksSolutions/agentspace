#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.06 programs, which know no host registry and no pending
# placement (the suite's defaults until E0.07-T05-S01 installs v0.07). A v0.07 build MUST pass it. It
# is not a regression when it fails against v0.06 (docs/conformance.md, "The files that are expected
# to fail").
#
# A conformance check (D34, D53; card #0155, E0.07-T04-S01): what `get`, `list` and `release` do with a
# lease that is `pending`. It enforces docs/driver.md, "Conformance cases for v0.07", cases C11 to C15,
# C19, C25 and C26, and the sections they cite:
#   - "Starting a provisioning run" and "check_pending" (what a `get` of a pending lease may start,
#     and that it never writes the lease state file, never waits and never fails over the host files).
#   - "The boundary" (`list` asks no driver) and "Release a lease" (a pending lease with a driver is
#     released through it, and its killed placement's leftovers go).
#   - docs/state.md, "The host registry" and "Its states" (who writes hosts.json, and how), "The
#     project list", "A host's folder", "The provisioning run's lock", "Release, and what it leaves"
#     (the owner mark of an alias file) and "Creation, in order".
#   - docs/lease-api.md, "agentspace lease get", "agentspace lease release" and "Environment".
#
# - C11  A `get` of a pending lease leaves leases.json byte for byte as it was, whether it starts the
#        stand-in or not, and prints the lease as stored.
# - C12  A `get` of a pending lease on a ready host, whose last_attempt_at is absent or 5 minutes old,
#        starts the stand-in once with exactly `provision -Host <project>`, sets last_attempt_at to
#        now, leaves the lease state file and the lease's files alone, and prints the lease pending.
#        A second `get` within 5 minutes starts nothing.
# - C13  A `get` of a pending lease succeeds, exit 0 with the lease, starts nothing and returns within
#        20 seconds, when hosts.json is not valid (two ways), when AGENTSPACE_PROVISION_CMD names no
#        file, and while the check holds hosts.lock. (The same holds for a projects.json that is not
#        valid: state.md, "The project list".)
# - C14  `list` never starts the stand-in, whatever its leases' states.
# - C15  `release` of a pending lease records it released, with no expires_at, starts nothing and
#        leaves hosts.json as it was, and never reads hosts.json or projects.json.
# - C19  A record in state ready never has its state changed: a `get` that starts the stand-in changes
#        only its last_attempt_at, and a `get` whose AGENTSPACE_PROVISION_CMD names no file changes
#        nothing.
# - C25  A `get` of a pending lease whose project has no record: for a listed project, it adds the
#        record creating and starts the stand-in once; for one not listed, it writes nothing to the
#        host registry, starts nothing and prints the lease pending.
# - C26  A `get` of a pending record with no driver, or one the CLI does not have, prints it, exit 0,
#        starts nothing and leaves hosts.json as it was.
# - The card's comment of 2026-11-20 23:53: a pending lease with the leftover credential files of a
#   killed placement is released, and its lease folder and its alias file are gone, while a foreign
#   alias file of the same name, with no owner mark for the lease, is kept.
#
# - Each case writes its own hosts.json, projects.json, held lock and pending lease record, in the
#   throwaway home and in the forms of docs/state.md, and each of those writes (and the one hold of a
#   lock) calls Assert-ConformancePath at the call. It depends on no new harness helper.
# - Every `get`, `list` and `release` that could start a run is given AGENTSPACE_PROVISION_CMD, a .cmd
#   stand-in under the home that records, beside itself, its arguments and its start time, and
#   nothing of its environment. It makes no host. A start that a case expects is waited for up to 30
#   seconds, and then 1.5 seconds more to show that there is only one. "Started nothing" is shown by no
#   record after a wait of 4 seconds: a detached start (WMI's Win32_Process.Create) returns the process
#   ID before the stand-in's interpreter has run, and the stand-in's first line takes tens of
#   milliseconds, so 4 seconds is a hundred times that and still a small part of the run.
# - The lease records are seeded, as a killed or unfinished placement leaves them, because no command
#   of the CLI can make a pending lease without also starting what the check is looking at. Each is in
#   state.md's form: the common fields and driver, and none of expires_at, released_at, ssh, reasons.
# - The program under test is untrusted (docs/conformance.md): the lease IDs and aliases a check builds
#   paths from are its own, drawn by the check, never read from the program's output. No failure
#   reason quotes the program's output but a lease document, which holds no secret.
# - It drives the program AGENTSPACE_CMD names only through the harness, as a separate process in a
#   throwaway home. It reads no real folder and loads no prototype code.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-pending-get-list-release.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$utf8 = [Text.UTF8Encoding]::new($false)
$script:PgNegWaitMs = 4000
$script:PgPosWaitMs = 30000
$script:PgStandInCount = 0
$checkPending = 'driver.md#check_pending'
$startingRun = 'driver.md#starting-a-provisioning-run'
$boundary = 'driver.md#the-boundary'
$releaseDoc = 'driver.md#release-a-lease'
$stateRelease = 'state.md#release-and-what-it-leaves'

# --- Files the cases seed, each written through Assert-ConformancePath at the call ---------------

function Write-PgFile {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Text)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path (Split-Path -Parent $Path)) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path $Path), $utf8.GetBytes($Text))
}

# One record of the host registry (state.md, "The host registry"), members in the order the table gives.
function New-PgHostRecord {
  param([Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$State, [string]$LastAttemptAt = '', [string]$InstanceId = '')
  $r = [ordered]@{
    project = $Project
    stack = $(if ($Project -ceq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" })
    state = $State
  }
  if ($InstanceId) { $r['instance_id'] = $InstanceId }
  if ($LastAttemptAt) { $r['last_attempt_at'] = $LastAttemptAt }
  return $r
}

# A time $Minutes from now (a negative number is before now), in the contract's form.
function Get-PgTime { param([int]$Minutes) ConvertTo-CcTime (Get-CcNowSecond).AddMinutes($Minutes) }

# Writes hosts.json as state.md says: compact JSON, UTF-8 with no byte order mark, one LF.
function Write-PgHosts {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  $text = '{"version":0,"hosts":[' + ((@($Records) | ForEach-Object { ConvertTo-Json -InputObject $_ -Compress }) -join ',') + ']}' + "`n"
  Write-PgFile -Lab $Lab -Path (Join-Path $Lab.UserProfile '.agentspace\hosts.json') -Text $text
}

# Writes the project list (state.md, "The project list").
function Write-PgProjects {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Names)
  $items = $Names | ForEach-Object { '{"name":"' + $_ + '","approved":"2026-11-20"}' }
  Write-PgFile -Lab $Lab -Path (Join-Path $Lab.UserProfile '.agentspace\projects.json') -Text ('{"version":0,"projects":[' + ($items -join ',') + ']}' + "`n")
}

# A pending lease record (state.md, "The lease state file"): the common fields and driver, and none of
# expires_at, released_at, ssh or reasons. -Driver '' leaves the member out.
function New-PgPending {
  param([Parameter(Mandatory)][string]$Project, [string]$Driver = 'ec2')
  $r = [ordered]@{
    id = [guid]::NewGuid().ToString()
    project = $Project
    state = 'pending'
    platform = 'linux/arm64'
    profile = 'agent-linux'
    size = 'm'
    ttl_seconds = 28800
    created_at = (Get-PgTime -2)
  }
  if ($Driver) { $r['driver'] = $Driver }
  return $r
}
function Write-PgLeases {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  Write-CcState $Lab ([ordered]@{ version = 0; leases = @($Records) })
}

# Puts the throwaway home back to nothing seeded: no registry, no project list, no lease records, no
# lease folders, no host folders and an empty ssh folder. Each delete is checked at the call.
function Reset-PgCase {
  param([Parameter(Mandatory)]$Lab)
  foreach ($p in @(
      (Join-Path $Lab.UserProfile '.agentspace\hosts.json'), (Join-Path $Lab.UserProfile '.agentspace\projects.json'),
      (Join-Path $Lab.UserProfile '.agentspace\hosts'), (Join-Path $Lab.UserProfile '.agentspace\leases'),
      (Join-Path $Lab.StateDir 'leases.json'))) {
    if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $p) -Recurse -Force }
  }
  Reset-CcSshDir $Lab
}

# Seeds a case: $Hosts (records, or $null for no hosts.json), $Projects (names, or $null for no project
# list) and $Leases (records, or $null for no lease state file).
function Set-PgScenario {
  param([Parameter(Mandatory)]$Lab, $Hosts, $Projects, $Leases)
  Reset-PgCase $Lab
  if ($null -ne $Hosts) { Write-PgHosts $Lab $Hosts }
  if ($null -ne $Projects) { Write-PgProjects $Lab $Projects }
  if ($null -ne $Leases) { Write-PgLeases $Lab $Leases }
}

# The hashes of the three files a case watches, 'absent' for one that is not there.
function Get-PgSnapshot {
  param([Parameter(Mandatory)]$Lab)
  [pscustomobject]@{
    Hosts = Get-CcFileHash (Join-Path $Lab.UserProfile '.agentspace\hosts.json')
    Projects = Get-CcFileHash (Join-Path $Lab.UserProfile '.agentspace\projects.json')
    Leases = Get-CcFileHash (Join-Path $Lab.StateDir 'leases.json')
  }
}

function Get-PgHostsHash { param($Lab) Get-CcFileHash (Join-Path $Lab.UserProfile '.agentspace\hosts.json') }

function Read-PgHosts {
  param([Parameter(Mandatory)]$Lab)
  $p = Join-Path $Lab.UserProfile '.agentspace\hosts.json'
  if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { return $null }
  $text = [IO.File]::ReadAllText($p, $utf8)
  [pscustomobject]@{ Text = $text; Valid = (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/hosts.state.schema.json'); Doc = (ConvertFrom-CcJson $text) }
}

# --- The stand-in for the provisioning run (driver.md, "Conformance cases for v0.07") ----------------

# A .cmd under the home that appends its arguments, and its start time, beside itself, and exits 0. It
# makes no host and records nothing of its environment. Each case gets a folder of its own, so a late
# start of one case never counts in the next.
function New-PgStandIn {
  param([Parameter(Mandatory)]$Lab)
  $script:PgStandInCount++
  $dir = Join-Path $Lab.Root "standins\run$($script:PgStandInCount)"
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $dir) | Out-Null
  $cmd = Join-Path $dir 'provision.cmd'
  # One line for each start, with each of the first four arguments as the interpreter parsed it (%~1 to %~4,
  # quotes removed), so that a check never depends on how the driver quoted the line it wrote.
  $text = "@echo off`r`n>>`"%~dp0record.txt`" echo A1=[%~1] A2=[%~2] A3=[%~3] A4=[%~4]`r`n>>`"%~dp0start-time.txt`" echo %DATE% %TIME%`r`nexit /b 0`r`n"
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path $cmd), [Text.Encoding]::ASCII.GetBytes($text))
  [pscustomobject]@{ Cmd = $cmd; Record = (Join-Path $dir 'record.txt') }
}

# The lines the stand-in has recorded so far, one per start. A read, sharing with a writer.
function Get-PgStarts {
  param([Parameter(Mandatory)]$StandIn)
  if (-not (Test-Path -LiteralPath $StandIn.Record -PathType Leaf)) { return , @() }
  try {
    $fs = [IO.File]::Open($StandIn.Record, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try { $t = [IO.StreamReader]::new($fs).ReadToEnd() } finally { $fs.Dispose() }
  } catch { return , @() }
  return , @(($t -split "\r?\n") | Where-Object { $_ -ne '' })
}

# Waits up to $script:PgPosWaitMs for one start, then a little longer for a second, and returns the lines.
function Wait-PgStart {
  param([Parameter(Mandatory)]$StandIn)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ((Get-PgStarts $StandIn).Count -lt 1 -and $sw.ElapsedMilliseconds -lt $script:PgPosWaitMs) { Start-Sleep -Milliseconds 100 }
  Start-Sleep -Milliseconds 1500
  return , (Get-PgStarts $StandIn)
}

# Waits the fixed time that shows "started nothing", and returns the lines.
function Wait-PgNoStart {
  param([Parameter(Mandatory)]$StandIn)
  Start-Sleep -Milliseconds $script:PgNegWaitMs
  return , (Get-PgStarts $StandIn)
}

# The problems with a start count: exactly one, with exactly three arguments, provision, -Host and the
# project, as the stand-in parsed them. It MUST NOT assert on how the driver quoted the line it wrote
# (docs/driver.md, "Starting a provisioning run": the line gives the arguments, not their quoting).
function Get-PgOnceProblems {
  param($Starts, [string]$Project)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Starts.Count -ne 1) { $p.Add("The stand-in was started $($Starts.Count) time(s), not once"); return , $p }
  $m = [regex]::Match($Starts[0].Trim(), '\AA1=\[(.*)\] A2=\[(.*)\] A3=\[(.*)\] A4=\[(.*)\]\z')
  if (-not $m.Success) { $p.Add('The stand-in''s record is not in the form the stand-in writes'); return , $p }
  $got = @($m.Groups[1].Value, $m.Groups[2].Value, $m.Groups[3].Value, $m.Groups[4].Value)
  if ($got[0] -cne 'provision' -or $got[1] -cne '-Host' -or $got[2] -cne $Project -or $got[3] -ne '') {
    $p.Add('The stand-in''s arguments are not exactly provision, -Host and the project')
  }
  return , $p
}

# --- Running a command, and what it MUST print --------------------------------------------------------

# Runs one lease command with the stand-in named, and keeps the clock before and after and the time it
# took. $Provision may be any path, or $null to remove the variable.
function Invoke-PgLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Arguments, $Provision)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $before = Get-CcNowSecond
  $run = Invoke-CcLease -Lab $Lab -Arguments $Arguments -EnvVars @{ AGENTSPACE_PROVISION_CMD = $Provision }
  $after = Get-CcNowSecond
  [pscustomobject]@{ Run = $run; Before = $before; After = $after; Ms = $sw.ElapsedMilliseconds }
}

# The problems with a `get` that MUST succeed and print the pending lease as stored (state.md, "The
# lease state file"): exit 0, nothing on stderr, one document that validates, and exactly the record's
# common fields with the record's values, and no driver (D1).
function Get-PgStoredProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)]$Record)
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not (Test-CcSuccess $Run)) { $p.Add("It did not succeed with one lease document: exit $($Run.ExitCode), stderr $($Run.StderrBytes.Length) bytes"); return , $p }
  $doc = ConvertFrom-CcJson $Run.Stdout
  if ($null -eq $doc) { $p.Add('Stdout is not a JSON object'); return , $p }
  $want = @('id', 'project', 'state', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at')
  $got = @($doc.Keys | Sort-Object)
  if (($got -join ',') -cne ((@($want) | Sort-Object) -join ',')) { $p.Add("The document's members are '$($got -join ',')'") }
  foreach ($k in $want) { if ($doc.ContainsKey($k) -and [string]$doc[$k] -cne [string]$Record[$k]) { $p.Add("$k is not as stored") } }
  return , $p
}

# True when the time is a real contract time within [Before - 1s, After + 1s], the clock of this process
# either side of a run. The second of slack each way is because the spec does not fix whether "now" is
# truncated or rounded to the second, and the check MUST NOT fail a program for either (as
# Test-PvTimeBetween in provision-standin.ps1).
function Test-PgWithin {
  param($Text, [DateTime]$Before, [DateTime]$After)
  if ($Text -isnot [string] -or $Text -cnotmatch '\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z\z') { return $false }
  try { $t = ConvertFrom-CcTime $Text } catch { return $false }
  return ($t -ge $Before.AddSeconds(-1) -and $t -le $After.AddSeconds(1))
}

# The names of what a lease CLI left in the lease folder, and in the alias folder (nothing for a pending lease).
function Get-PgLeaseFiles {
  param([Parameter(Mandatory)]$Lab)
  @(Get-CcFileNames (Join-Path $Lab.UserProfile '.agentspace\leases')) + @(Get-CcFileNames (Join-Path $Lab.SshDir 'agentspace.d'))
}

# Holds a lock file as state.md says a lock is held: open with no sharing, for reading and writing,
# creating it if it is missing. The caller disposes it. The open is checked at the call.
function Lock-PgFile {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Path)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path (Split-Path -Parent $Path)) | Out-Null
  [IO.File]::Open((Assert-ConformancePath -ConformanceHome $Lab -Path $Path), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
}

# --- The cases ---------------------------------------------------------------------------------------------

$lab = $null
try {
  $lab = New-ConformanceHome
  $box = New-PgHostRecord 'swimcoach' 'ready'
  $hostsPath = Join-Path $lab.UserProfile '.agentspace\hosts.json'

  # ---- C11 and C12: a get of a pending lease on a ready host ------------------------------------------------
  foreach ($v in @(@{ Name = 'absent'; Minutes = $null }, @{ Name = '10 minutes old'; Minutes = -10 })) {
    $at = if ($null -eq $v.Minutes) { '' } else { Get-PgTime $v.Minutes }
    $pending = New-PgPending 'agentspace'
    Set-PgScenario $lab @($box, (New-PgHostRecord 'agentspace' 'ready' $at)) $null @($pending)
    $snap = Get-PgSnapshot $lab
    $si = New-PgStandIn $lab
    $g = Invoke-PgLease $lab @('get', $pending['id']) $si.Cmd
    $starts = Wait-PgStart $si
    $hostsAfter = Read-PgHosts $lab
    $afterSnap = Get-PgSnapshot $lab
    Test-Cc "C12 ($startingRun): A get of a pending lease on a ready host whose last_attempt_at is $($v.Name) starts the stand-in once, with exactly provision -Host <project>" {
      $q = Get-PgOnceProblems $starts 'agentspace'
      if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
      $true
    }
    Test-Cc "C12 ($startingRun): The same get sets the record's last_attempt_at to now, keeps its state ready, and changes no other member or record" {
      if ($null -eq $hostsAfter -or -not $hostsAfter.Valid) { return 'hosts.json is missing or does not validate against its schema.' }
      $recs = @($hostsAfter.Doc['hosts'])
      if ($recs.Count -ne 2) { return "hosts.json holds $($recs.Count) records, not 2." }
      if ((ConvertTo-CcCanonical $recs[0]) -cne (ConvertTo-CcCanonical $box)) { return 'The box''s record changed.' }
      $r = $recs[1]
      if ($r['project'] -cne 'agentspace' -or $r['stack'] -cne 'agentspace-host-agentspace' -or $r['state'] -cne 'ready') { return 'The record''s project, stack or state is not as it was.' }
      if (-not (Test-PgWithin $r['last_attempt_at'] $g.Before $g.After)) { return 'last_attempt_at is not a time within the command''s run.' }
      $true
    }
    Test-Cc "C12 ($checkPending): The same get leaves the lease state file byte for byte as it was, makes no lease folder or alias file, and prints the lease pending as stored" {
      if ($afterSnap.Leases -cne $snap.Leases) { return 'The lease state file changed.' }
      $f = Get-PgLeaseFiles $lab
      if ($f.Count -ne 0) { return "A lease folder or alias file was made: $($f.Count) entries." }
      $q = Get-PgStoredProblems $g.Run $pending
      if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
      if ($g.Run.Json['state'] -cne 'pending') { return 'The lease is not printed pending.' }
      $true
    }
    Test-Cc "C11 ($checkPending): A get that starts the stand-in leaves the lease state file byte for byte as it was, and prints the lease as stored (last_attempt_at $($v.Name))" {
      if ($afterSnap.Leases -cne $snap.Leases) { return 'The lease state file changed.' }
      $q = Get-PgStoredProblems $g.Run $pending
      if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
      if ($starts.Count -lt 1) { return 'The get did not start the stand-in, so this case did not show what it is about.' }
      $true
    }
    if ($null -eq $v.Minutes) {
      # A second get within 5 minutes starts nothing, and leaves last_attempt_at as the first set it.
      $hostsBetween = Get-PgHostsHash $lab
      $g2 = Invoke-PgLease $lab @('get', $pending['id']) $si.Cmd
      $starts2 = Wait-PgNoStart $si
      Test-Cc "C12 ($startingRun): A second get within 5 minutes starts nothing and leaves last_attempt_at as it was" {
        if (-not (Test-CcSuccess $g2.Run)) { return "The second get gave $(Format-CcRun $g2.Run)" }
        if ($starts2.Count -ne 1) { return "The stand-in was started $($starts2.Count) time(s) in all, not once." }
        if ((Get-PgHostsHash $lab) -cne $hostsBetween) { return 'hosts.json changed on the second get.' }
        $true
      }
    }
  }

  # C11, the other half: a get that starts nothing also leaves the file alone and prints the lease as stored.
  $pending = New-PgPending 'agentspace'
  Set-PgScenario $lab @($box, (New-PgHostRecord 'agentspace' 'ready' (Get-PgTime -1))) $null @($pending)
  $snap = Get-PgSnapshot $lab
  $si = New-PgStandIn $lab
  $g = Invoke-PgLease $lab @('get', $pending['id']) $si.Cmd
  $starts = Wait-PgNoStart $si
  Test-Cc "C11 ($checkPending): A get that starts nothing (last_attempt_at 1 minute old) leaves the lease state file byte for byte as it was, and prints the lease as stored" {
    if ((Get-PgSnapshot $lab).Leases -cne $snap.Leases) { return 'The lease state file changed.' }
    $q = Get-PgStoredProblems $g.Run $pending
    if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
    if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s), so this case did not show what it is about." }
    $true
  }

  # ---- C13: a get never fails and never waits over the host files ----------------------------------------------
  $c13 = @(
    @{ Name = 'hosts.json is not JSON, and the project is listed'; Hosts = '{"version":0,"hosts":['; Projects = ('{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20"}]}' + "`n"); Provision = 'standin'; Lock = $false },
    @{ Name = 'hosts.json is valid by its schema but holds no record for the box'; Hosts = '{"version":0,"hosts":[{"project":"agentspace","stack":"agentspace-host-agentspace","state":"ready"}]}'; Projects = $null; Provision = 'standin'; Lock = $false },
    @{ Name = 'AGENTSPACE_PROVISION_CMD names no file, with a run due'; Hosts = 'creating'; Projects = $null; Provision = 'missing'; Lock = $false },
    @{ Name = 'the check holds hosts.lock, with a run due'; Hosts = 'ready'; Projects = $null; Provision = 'standin'; Lock = $true },
    @{ Name = 'projects.json is not valid, and the project has no record'; Hosts = $null; Projects = '{"version":0,"projects":['; Provision = 'standin'; Lock = $false })
  foreach ($c in $c13) {
    $pending = New-PgPending 'agentspace'
    Set-PgScenario $lab $null $null @($pending)
    if ($c.Hosts -ceq 'creating') { Write-PgHosts $lab @($box, (New-PgHostRecord 'agentspace' 'creating' (Get-PgTime -10))) }
    elseif ($c.Hosts -ceq 'ready') { Write-PgHosts $lab @($box, (New-PgHostRecord 'agentspace' 'ready' (Get-PgTime -10))) }
    elseif ($c.Hosts) { Write-PgFile $lab (Join-Path $lab.UserProfile '.agentspace\hosts.json') $c.Hosts }
    if ($c.Projects) { Write-PgFile $lab (Join-Path $lab.UserProfile '.agentspace\projects.json') $c.Projects }
    $snap = Get-PgSnapshot $lab
    $si = New-PgStandIn $lab
    $provision = if ($c.Provision -ceq 'missing') { Join-Path $lab.Root 'standins\no-such-file\provision.cmd' } else { $si.Cmd }
    $lock = $null
    $lockHeld = $false
    try {
      if ($c.Lock) {
        $lock = Lock-PgFile $lab (Join-Path $lab.UserProfile '.agentspace\hosts.lock')
        # The hold MUST be real: a second open of the file, sharing everything, is refused while it lasts.
        $probe = $null
        try { $probe = [IO.File]::Open((Join-Path $lab.UserProfile '.agentspace\hosts.lock'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite) }
        catch [IO.IOException] { $lockHeld = $true }
        finally { if ($probe) { $probe.Dispose() } }
      }
      $g = Invoke-PgLease $lab @('get', $pending['id']) $provision
      $starts = Wait-PgNoStart $si
    } finally { if ($lock) { $lock.Dispose() } }
    Test-Cc "C13 ($checkPending): A get of a pending lease succeeds with the lease, starts nothing, writes nothing and returns within 20 seconds when $($c.Name)" {
      if ($c.Lock -and -not $lockHeld) { return 'The check''s own hold of hosts.lock did not hold, so this case did not show what it is about.' }
      $q = Get-PgStoredProblems $g.Run $pending
      if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
      if ($g.Ms -ge 20000) { return "The get took $($g.Ms) ms, not under 20 seconds." }
      if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
      $after = Get-PgSnapshot $lab
      if ($after.Hosts -cne $snap.Hosts -or $after.Projects -cne $snap.Projects -or $after.Leases -cne $snap.Leases) { return 'hosts.json, projects.json or the lease state file changed.' }
      $true
    }
  }

  # ---- C14: list never starts anything ----------------------------------------------------------------------------
  $pReady = New-PgPending 'agentspace'
  $pList = New-PgPending 'alpha'
  $pCreating = New-PgPending 'beta'
  $released = [ordered]@{ id = [guid]::NewGuid().ToString(); project = 'agentspace'; state = 'released'; platform = 'linux/arm64'; profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800
    created_at = (Get-PgTime -30); released_at = (Get-PgTime -20); driver = 'ec2' }
  $unsat = [ordered]@{ id = [guid]::NewGuid().ToString(); project = 'gamma'; state = 'unsatisfiable'; platform = 'linux/amd64'; profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800
    created_at = (Get-PgTime -30); reasons = @([ordered]@{ code = 'unsupported_platform'; message = "Platform 'linux/amd64' is not supported; v0 supports linux/arm64 only." }); driver = 'ec2' }
  $readyId = [guid]::NewGuid().ToString()
  $readyRec = [ordered]@{ id = $readyId; project = 'swimcoach'; state = 'ready'; platform = 'linux/arm64'; profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800
    created_at = (Get-PgTime -30); expires_at = (Get-PgTime 600); ssh = [ordered]@{ host_alias = "agentspace-swimcoach-$($readyId.Substring(0, 8))"; user = 'ubuntu' }; driver = 'ec2' }
  Set-PgScenario $lab @($box, (New-PgHostRecord 'agentspace' 'ready' (Get-PgTime -10)), (New-PgHostRecord 'beta' 'creating' (Get-PgTime -10))) @('alpha') @($pReady, $pList, $pCreating, $released, $unsat, $readyRec)
  $snap = Get-PgSnapshot $lab
  $si = New-PgStandIn $lab
  $l1 = Invoke-PgLease $lab @('list') $si.Cmd
  $l2 = Invoke-PgLease $lab @('list', '--state', 'pending') $si.Cmd
  $starts = Wait-PgNoStart $si
  Test-Cc "C14 ($boundary): list, with every state of lease and three pending leases whose host would start a run, starts nothing and changes no file" {
    if (-not (Test-CcSuccess $l1.Run -Schema 'lease-list.output.schema.json')) { return "The list gave $(Format-CcRun $l1.Run)" }
    if (-not (Test-CcSuccess $l2.Run -Schema 'lease-list.output.schema.json')) { return "The filtered list gave $(Format-CcRun $l2.Run)" }
    if (@((ConvertFrom-CcJson $l1.Run.Stdout)['leases']).Count -ne 6) { return 'The list does not hold the six leases.' }
    if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
    $after = Get-PgSnapshot $lab
    if ($after.Hosts -cne $snap.Hosts -or $after.Projects -cne $snap.Projects -or $after.Leases -cne $snap.Leases) { return 'hosts.json, projects.json or the lease state file changed.' }
    $true
  }

  # ---- C15: release of a pending lease ------------------------------------------------------------------------------
  $c15 = @(
    @{ Name = 'a host that is creating, with a run due'; Hosts = @($box, (New-PgHostRecord 'agentspace' 'creating' (Get-PgTime -10))); Projects = $null; Raw = $false },
    @{ Name = 'a ready host, with no last_attempt_at'; Hosts = @($box, (New-PgHostRecord 'agentspace' 'ready')); Projects = $null; Raw = $false },
    @{ Name = 'a listed project with no record and no hosts.json'; Hosts = $null; Projects = @('agentspace'); Raw = $false },
    @{ Name = 'a hosts.json and a projects.json that are not valid, which release never reads'; Hosts = '{"version":0,"hosts":['; Projects = '{"version":0,"projects":['; Raw = $true })
  foreach ($c in $c15) {
    $pending = New-PgPending 'agentspace'
    Set-PgScenario $lab $null $null @($pending)
    if ($c.Raw) {
      Write-PgFile $lab (Join-Path $lab.UserProfile '.agentspace\hosts.json') $c.Hosts
      Write-PgFile $lab (Join-Path $lab.UserProfile '.agentspace\projects.json') $c.Projects
    } else {
      if ($null -ne $c.Hosts) { Write-PgHosts $lab $c.Hosts }
      if ($null -ne $c.Projects) { Write-PgProjects $lab $c.Projects }
    }
    $snap = Get-PgSnapshot $lab
    $si = New-PgStandIn $lab
    $g = Invoke-PgLease $lab @('release', $pending['id']) $si.Cmd
    $starts = Wait-PgNoStart $si
    Test-Cc "C15 ($releaseDoc): Release of a pending lease on $($c.Name) records it released with released_at and no expires_at, starts nothing and leaves hosts.json and projects.json as they were" {
      if (-not (Test-CcSuccess $g.Run)) { return "The release gave $(Format-CcRun $g.Run)" }
      $doc = ConvertFrom-CcJson $g.Run.Stdout
      if ($doc['state'] -cne 'released') { return 'The lease is not printed released.' }
      if (-not (Test-PgWithin $doc['released_at'] $g.Before $g.After)) { return 'released_at is not a time within the command''s run.' }
      foreach ($k in 'expires_at', 'ssh', 'reasons', 'driver') { if ($doc.ContainsKey($k)) { return "The document has a $k." } }
      $rec = Get-CcRecord $lab $pending['id']
      if ($null -eq $rec -or $rec['state'] -cne 'released' -or $rec['released_at'] -cne $doc['released_at']) { return 'The lease state file does not hold the record released with the printed released_at.' }
      foreach ($k in 'expires_at', 'ssh', 'reasons') { if ($rec.ContainsKey($k)) { return "The record has a $k." } }
      foreach ($k in 'id', 'project', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at', 'driver') { if ([string]$rec[$k] -cne [string]$pending[$k]) { return "The record's $k changed." } }
      if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
      $after = Get-PgSnapshot $lab
      if ($after.Hosts -cne $snap.Hosts -or $after.Projects -cne $snap.Projects) { return 'hosts.json or projects.json changed.' }
      $true
    }
  }

  # ---- C19: a ready record's state is never changed by the driver ----------------------------------------------------
  $ready = New-PgHostRecord 'agentspace' 'ready' (Get-PgTime -10) 'i-1234567890abcdef0'
  $pending = New-PgPending 'agentspace'
  Set-PgScenario $lab @($box, $ready) $null @($pending)
  $si = New-PgStandIn $lab
  $g = Invoke-PgLease $lab @('get', $pending['id']) $si.Cmd
  $starts = Wait-PgStart $si
  $hostsAfter = Read-PgHosts $lab
  Test-Cc "C19 ($startingRun): A get that starts the stand-in for a ready record leaves its state ready and its instance_id as they were, and changes only last_attempt_at" {
    if ($starts.Count -ne 1) { return "The stand-in was started $($starts.Count) time(s), not once, so this case did not show what it is about." }
    if ($null -eq $hostsAfter -or -not $hostsAfter.Valid) { return 'hosts.json is missing or does not validate against its schema.' }
    $recs = @($hostsAfter.Doc['hosts'])
    if ($recs.Count -ne 2) { return "hosts.json holds $($recs.Count) records, not 2." }
    if ((ConvertTo-CcCanonical $recs[0]) -cne (ConvertTo-CcCanonical $box)) { return 'The box''s record changed.' }
    $r = $recs[1]
    if ($r['state'] -cne 'ready') { return 'The record''s state is no longer ready.' }
    if ($r['instance_id'] -cne 'i-1234567890abcdef0') { return 'The record''s instance_id changed.' }
    if ($r['project'] -cne 'agentspace' -or $r['stack'] -cne 'agentspace-host-agentspace') { return 'The record''s project or stack changed.' }
    if ($r['last_attempt_at'] -ceq $ready['last_attempt_at']) { return 'last_attempt_at was not changed.' }
    if (-not (Test-PgWithin $r['last_attempt_at'] $g.Before $g.After)) { return 'last_attempt_at is not a time within the command''s run.' }
    if (@($r.Keys).Count -ne 5) { return 'The record has a member other than project, stack, state, instance_id and last_attempt_at.' }
    $true
  }
  # A due run whose program cannot be found writes nothing at all: the record is as it was.
  $pending = New-PgPending 'agentspace'
  Set-PgScenario $lab @($box, $ready) $null @($pending)
  $snap = Get-PgSnapshot $lab
  $si = New-PgStandIn $lab
  $g = Invoke-PgLease $lab @('get', $pending['id']) (Join-Path $lab.Root 'standins\no-such-file\provision.cmd')
  $starts = Wait-PgNoStart $si
  Test-Cc "C19 ($startingRun): A get whose AGENTSPACE_PROVISION_CMD names no file leaves the ready record as it was, byte for byte, and exits 0 with the lease" {
    $q = Get-PgStoredProblems $g.Run $pending
    if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
    if ((Get-PgSnapshot $lab).Hosts -cne $snap.Hosts) { return 'hosts.json changed.' }
    if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
    $true
  }

  # ---- C25: a get of a pending lease whose project has no record -----------------------------------------------------
  foreach ($v in @(
      @{ Name = 'with no hosts.json'; Hosts = $null },
      @{ Name = 'with a hosts.json that holds the box''s record alone'; Hosts = @($box) })) {
    $pending = New-PgPending 'agentspace'
    Set-PgScenario $lab $v.Hosts @('agentspace') @($pending)
    $snap = Get-PgSnapshot $lab
    $si = New-PgStandIn $lab
    $g = Invoke-PgLease $lab @('get', $pending['id']) $si.Cmd
    $starts = Wait-PgStart $si
    $hostsAfter = Read-PgHosts $lab
    Test-Cc "C25 ($startingRun): A get of a pending lease of a listed project with no record, $($v.Name), adds the record creating, after the box's, and starts the stand-in once" {
      $q = Get-PgOnceProblems $starts 'agentspace'
      if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
      if ($null -eq $hostsAfter -or -not $hostsAfter.Valid) { return 'hosts.json is missing or does not validate against its schema.' }
      $recs = @($hostsAfter.Doc['hosts'])
      if ($recs.Count -ne 2) { return "hosts.json holds $($recs.Count) records, not 2." }
      if ((ConvertTo-CcCanonical $recs[0]) -cne (ConvertTo-CcCanonical $box)) { return 'The first record is not the box''s, as it was.' }
      $r = $recs[1]
      if ($r['project'] -cne 'agentspace' -or $r['stack'] -cne 'agentspace-host-agentspace' -or $r['state'] -cne 'creating') { return 'The added record is not agentspace, agentspace-host-agentspace, creating.' }
      if (-not (Test-PgWithin $r['last_attempt_at'] $g.Before $g.After)) { return 'The added record''s last_attempt_at is not a time within the command''s run.' }
      $true
    }
    Test-Cc "C25 ($checkPending): The same get prints the lease pending as stored and leaves the lease state file byte for byte as it was ($($v.Name))" {
      $q = Get-PgStoredProblems $g.Run $pending
      if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
      if ((Get-PgSnapshot $lab).Leases -cne $snap.Leases) { return 'The lease state file changed.' }
      $true
    }
  }
  foreach ($v in @(
      @{ Name = 'with no project list'; Projects = $null },
      @{ Name = 'with a project list that lists another project'; Projects = @('someone-else') })) {
    $pending = New-PgPending 'agentspace'
    Set-PgScenario $lab $null $v.Projects @($pending)
    $snap = Get-PgSnapshot $lab
    $si = New-PgStandIn $lab
    $g = Invoke-PgLease $lab @('get', $pending['id']) $si.Cmd
    $starts = Wait-PgNoStart $si
    Test-Cc "C25 ($startingRun): A get of a pending lease of a project that has no record and is not listed, $($v.Name), writes nothing to the host registry, starts nothing and prints the lease pending" {
      $q = Get-PgStoredProblems $g.Run $pending
      if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
      if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
      $after = Get-PgSnapshot $lab
      if ($after.Hosts -cne $snap.Hosts) { return 'hosts.json changed or was made.' }
      if ($after.Leases -cne $snap.Leases) { return 'The lease state file changed.' }
      $true
    }
  }

  # ---- C26: a pending record with no driver, or one the CLI does not have --------------------------------------------
  foreach ($d in @(@{ Name = 'no driver'; Driver = '' }, @{ Name = 'a driver the CLI does not have'; Driver = 'zq-conformance-driver-x9' })) {
    foreach ($s in @(
        @{ Name = 'a ready host whose run would be due'; Hosts = @($box, (New-PgHostRecord 'agentspace' 'ready' (Get-PgTime -10))); Projects = $null },
        @{ Name = 'a listed project with no hosts.json'; Hosts = $null; Projects = @('agentspace') })) {
      $pending = New-PgPending 'agentspace' $d.Driver
      Set-PgScenario $lab $s.Hosts $s.Projects @($pending)
      $snap = Get-PgSnapshot $lab
      $si = New-PgStandIn $lab
      $g = Invoke-PgLease $lab @('get', $pending['id']) $si.Cmd
      $starts = Wait-PgNoStart $si
      Test-Cc "C26 ($checkPending): A get of a pending record with $($d.Name), on $($s.Name), prints it, exit 0, starts nothing and leaves hosts.json as it was" {
        $q = Get-PgStoredProblems $g.Run $pending
        if ($q.Count -gt 0) { return ($q -join '; ') + '.' }
        if ($d.Driver -and ($g.Run.Stdout + $g.Run.Stderr).Contains($d.Driver)) { return 'The stored driver''s name is in the output.' }
        if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
        $after = Get-PgSnapshot $lab
        if ($after.Hosts -cne $snap.Hosts) { return 'hosts.json changed or was made.' }
        if ($after.Leases -cne $snap.Leases) { return 'The lease state file changed.' }
        $true
      }
    }
  }

  # ---- Release of a pending lease that a killed placement left files for -------------------------------------------------
  # The leftovers are the three files of a lease folder and an alias file whose owner mark names the lease
  # (state.md, "The lease folder", "The alias file"). They are dummies: nothing reads their content.
  function Write-PgLeftovers {
    param($Lab, [string]$Id, [string]$Project, [string]$AliasText)
    $dir = Get-CcLeaseDir $Lab $Id
    foreach ($f in 'id_ed25519', 'id_ed25519.pub', 'id_ed25519-cert.pub') { Write-PgFile $Lab (Join-Path $dir $f) "leftover of a killed placement`n" }
    $alias = "agentspace-$Project-$($Id.Substring(0, 8))"
    Write-PgFile $Lab (Get-CcAliasPath $Lab $alias) $AliasText
    return $alias
  }
  $ownMark = { param($id) "# agentspace lease $id; written by a placement that was killed. Do not edit.`nHost x`n  User ubuntu`n" }

  # Another lease's files, which a release MUST NOT touch.
  $other = New-PgPending 'agentspace'
  $pending = New-PgPending 'agentspace'
  Set-PgScenario $lab @($box, (New-PgHostRecord 'agentspace' 'ready' (Get-PgTime -1))) $null @($pending, $other)
  $aliasName = Write-PgLeftovers $lab $pending['id'] 'agentspace' (& $ownMark $pending['id'])
  $otherAlias = Write-PgLeftovers $lab $other['id'] 'agentspace' (& $ownMark $other['id'])
  $aliasPath = Get-CcAliasPath $lab $aliasName
  $leaseDir = Get-CcLeaseDir $lab $pending['id']
  $otherDir = Get-CcLeaseDir $lab $other['id']
  $otherAliasPath = Get-CcAliasPath $lab $otherAlias
  $otherAliasHash = Get-CcFileHash $otherAliasPath
  $snap = Get-PgSnapshot $lab
  $si = New-PgStandIn $lab
  $g = Invoke-PgLease $lab @('release', $pending['id']) $si.Cmd
  $starts = Wait-PgNoStart $si
  Test-Cc "$releaseDoc`: Release of a pending lease whose placement was killed removes its alias file and its lease folder, with everything in it, and records it released" {
    if (-not (Test-CcSuccess $g.Run)) { return "The release gave $(Format-CcRun $g.Run)" }
    if ((ConvertFrom-CcJson $g.Run.Stdout)['state'] -cne 'released') { return 'The lease is not printed released.' }
    $rec = Get-CcRecord $lab $pending['id']
    if ($null -eq $rec -or $rec['state'] -cne 'released') { return 'The record is not released.' }
    if (Test-Path -LiteralPath $aliasPath) { return 'The alias file is still there.' }
    if (Test-Path -LiteralPath $leaseDir) { return 'The lease folder is still there.' }
    $true
  }
  Test-Cc "$releaseDoc`: The same release touches nothing of another lease: its folder and its alias file are as they were, and nothing is started" {
    if (-not (Test-Path -LiteralPath (Join-Path $otherDir 'id_ed25519') -PathType Leaf)) { return 'Another lease''s files were removed.' }
    if ((Get-CcFileHash $otherAliasPath) -cne $otherAliasHash) { return 'Another lease''s alias file changed or was removed.' }
    $rec = Get-CcRecord $lab $other['id']
    if ($null -eq $rec -or $rec['state'] -cne 'pending') { return 'Another lease''s record changed.' }
    if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
    if ((Get-PgSnapshot $lab).Hosts -cne $snap.Hosts) { return 'hosts.json changed.' }
    $true
  }

  # A foreign alias file of the same name is kept, whatever else the release does about it. The lease is
  # not released over a file that is not its own: driver.md, "Release a lease" (the driver MUST fail and
  # delete nothing else), and state.md, "Release, and what it leaves" (internal, changing nothing).
  foreach ($f in @(
      @{ Name = 'has no owner mark'; Text = "# A person's own file, which agentspace never wrote`nHost mine`n  User me`n"; OtherLease = $false },
      @{ Name = 'has the owner mark of another lease'; Text = $null; OtherLease = $true })) {
    $pending = New-PgPending 'agentspace'
    Set-PgScenario $lab @($box, (New-PgHostRecord 'agentspace' 'ready' (Get-PgTime -1))) $null @($pending)
    $text = if ($f.OtherLease) { & $ownMark ([guid]::NewGuid().ToString()) } else { $f.Text }
    $aliasName = Write-PgLeftovers $lab $pending['id'] 'agentspace' $text
    $aliasPath = Get-CcAliasPath $lab $aliasName
    $leaseDir = Get-CcLeaseDir $lab $pending['id']
    $foreignHash = Get-CcFileHash $aliasPath
    $recordBefore = ConvertTo-CcCanonical (Get-CcRecord $lab $pending['id'])
    $si = New-PgStandIn $lab
    $g = Invoke-PgLease $lab @('release', $pending['id']) $si.Cmd
    $starts = Wait-PgNoStart $si
    Test-Cc "$releaseDoc`: A foreign alias file of the same name that $($f.Name) is kept, byte for byte, by the release of a pending lease" {
      if (-not (Test-Path -LiteralPath $aliasPath -PathType Leaf)) { return 'The foreign alias file was removed.' }
      if ((Get-CcFileHash $aliasPath) -cne $foreignHash) { return 'The foreign alias file changed.' }
      $true
    }
    Test-Cc "$stateRelease`: The same release fails with internal and changes nothing: the record is still pending, the lease folder is kept, and nothing is started (the alias file's owner mark is checked first)" {
      if (-not (Test-CcFailure $g.Run 1 'internal')) { return "Not the expected failure: $(Format-CcRun $g.Run)" }
      if ($g.Run.Stderr -cmatch '[A-Za-z]:\\') { return 'The message holds a path.' }
      $rec = Get-CcRecord $lab $pending['id']
      if ($null -eq $rec -or (ConvertTo-CcCanonical $rec) -cne $recordBefore) { return 'The lease''s record changed.' }
      if (-not (Test-Path -LiteralPath (Join-Path $leaseDir 'id_ed25519') -PathType Leaf)) { return 'The lease folder was removed.' }
      if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
      $true
    }
  }
}
catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-pending-get-list-release.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
}
finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

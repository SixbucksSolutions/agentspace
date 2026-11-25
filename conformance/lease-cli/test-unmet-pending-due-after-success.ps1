#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.09 programs, which know no `last_success_at`: a build before
# v0.10 refuses a hosts.json that holds the member as not valid by its schema, so each case that seeds one
# fails there, and its rule of an earlier start (D77) is not theirs. A v0.10 build MUST pass every result
# line (docs/conformance.md, "The files that are expected to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what
# docs/lease-api.md, docs/driver.md and docs/state.md document, with a stand-in for the provisioning run
# (a program outside agentspace). See docs/conformance.md.
#
# Cases of E0.10-T04-S01 (card #0221), the rows of docs/driver.md, "Conformance cases for v0.10", D77 with
# pins P22, P24 and P31. Each row is a result of its own for every command and every seed it names, and each
# result's name starts with the driver.md anchor the row cites and then the row's ID:
#   Y1  A success at or after last_attempt_at makes the next start at once: a `get` of a stored pending lease,
#       and in a case of its own a `create --size l`, start the stand-in once with exactly
#       `provision -Host <project>`, set last_attempt_at to now, and leave the record with no last_success_at
#       and every other member as it was.
#   Y2  An older or absent success waits 5 minutes: with last_attempt_at 1 minute before now and
#       last_success_at 2 minutes before it, or absent, a `get` and a `create --size l` each start nothing and
#       leave the registry as it was; with last_attempt_at 5 minutes or more before now, each starts the
#       stand-in once.
#   Y3  Equal seconds follow pin P24: with last_success_at equal to last_attempt_at, a `get` starts the
#       stand-in once and removes last_success_at in the write that sets last_attempt_at; a second `get`
#       straight after it starts nothing and leaves the registry as that write left it.
#   Y4  A run whose placing failed or left a lease writes no success (pin P22): a record as such a run leaves
#       it, with no last_success_at, beside a stored pending lease, answers a `get` and a `create --size l`
#       as Y2's absent case does, and the next start comes once last_attempt_at is 5 minutes old.
#   Y5  A held lock still stops a start that a success would make due: with last_success_at at or after
#       last_attempt_at and the project's provision.lock held by the check, a `get` and a `create --size l`
#       start nothing and leave the registry as it was.
# A run's own writing of last_success_at, the run's start record removing it, and a lease created after
# placing's last re-read are not lease CLI rows: driver.md, "Conformance cases for v0.10", gives them to the
# operator's cases of E0.10-T04-S02 and to the order of the locks.
# C8, C12, Z10 and Z14 stay as written, and are not in this file: their records carry no last_success_at.
# Sections enforced: driver.md "Starting a provisioning run", steps 2 and 4, and "After the run"; state.md
# "The host registry" and "The last success".
#
# - Each case writes its own hosts.json, projects.json, lease state file and held provision.lock in the
#   throwaway home, in the forms of docs/state.md and schemas/state: compact JSON, UTF-8 with no byte order
#   mark, one line feed. Each file it seeds is checked against its schema before the case runs, and each write
#   goes through Assert-ConformancePath. The lease CLI's stand-in provision writes nothing to hosts.json, so
#   each case seeds last_success_at itself, in the contract's form. "The registry as it was" is hosts.json
#   byte for byte as the case wrote it.
# - A time is computed once from the check's own clock, so that two seeded times meant equal are one string,
#   and a time 5 minutes old is shown by a seed 6 minutes before now and never by waiting.
# - Every `create` and `get` is given AGENTSPACE_PROVISION_CMD, the recording stand-in of
#   provision-standin.ps1 (pin P12). "Starts nothing" is shown by no start after a wait of 3 seconds, as
#   provision-standin.ps1 states, and a start by exactly the arguments provision, -Host and the project.
# - The program under test is untrusted (docs/conformance.md): no failure reason quotes a value the program
#   printed or the stand-in captured. Helpers of this file's own are prefixed Sd.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-pending-due-after-success.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 2 minutes.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

$script:SdProject = 'duehost'
$script:SdInstanceId = 'i-1234567890abcdef0'
$script:SdType = 'm9g.xlarge'

# --- Files a case seeds --------------------------------------------------------------------------------

# One record of the host registry, members in the order of state.md's table: project, stack, state,
# instance_id, instance_type, last_attempt_at, last_success_at. The stack is the box's for swimcoach, and
# the project's own otherwise.
function New-SdRecord {
  param(
    [Parameter(Mandatory)][string]$Project, [string]$InstanceId, [string]$InstanceType, [string]$LastAttemptAt, [string]$LastSuccessAt
  )
  $stack = $(if ($Project -ceq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" })
  $r = [ordered]@{ project = $Project; stack = $stack; state = 'ready' }
  if ($InstanceId) { $r['instance_id'] = $InstanceId }
  if ($InstanceType) { $r['instance_type'] = $InstanceType }
  if ($LastAttemptAt) { $r['last_attempt_at'] = $LastAttemptAt }
  if ($LastSuccessAt) { $r['last_success_at'] = $LastSuccessAt }
  return $r
}

# The registry's text: compact, with one line feed.
function Get-SdHostsText {
  param([Parameter(Mandatory)]$Records)
  return '{"version":0,"hosts":[' + ((@($Records) | ForEach-Object { ConvertTo-Json -InputObject $_ -Compress -Depth 5 }) -join ',') + ']}' + "`n"
}

# Writes hosts.json from the records, after checking that the text validates against its schema.
function Set-SdHosts {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  $text = Get-SdHostsText $Records
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/hosts.state.schema.json')) { throw 'The seeded hosts.json does not validate against its schema.' }
  Write-PvText -Lab $Lab -Path (Get-PvHostsPath $Lab) -Text $text
}

# A stored pending lease of the project in state.md's form, with the EC2 driver's name and a size in SIZES.
function New-SdPendingLease {
  param([Parameter(Mandatory)][string]$Project, [string]$Size = 'l')
  $now = Get-CcNowSecond
  return [ordered]@{
    id = [guid]::NewGuid().ToString(); project = $Project; state = 'pending'; platform = 'linux/arm64'; profile = 'agent-linux'; size = $Size
    ttl_seconds = 28800; created_at = (ConvertTo-CcTime $now.AddMinutes(-10)); driver = 'ec2'
  }
}

# Writes leases.json from the records, after checking that the file validates against its schema.
function Set-SdLeases {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  Write-CcState $Lab ([ordered]@{ version = 0; leases = @($Records) })
  if (-not (Test-CcStateFile $Lab)) { throw 'The seeded lease state file does not validate against its schema.' }
}

# --- Assertions ----------------------------------------------------------------------------------------

# The problems with a run that printed a pending lease of the project, at the size asked for, with no
# instance type in it, and (for a `get`) the stored lease's ID.
function Get-SdPendingDocProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$Size, [string]$Id)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-PvPendingDocProblems -Run $Run -Project $Project)) { $p.Add($x) }
  if ($Run.Json -is [hashtable]) {
    if ($Run.Json['size'] -cne $Size) { $p.Add("The document's size is '$($Run.Json['size'])', not '$Size'") }
    if ($Id -and $Run.Json['id'] -cne $Id) { $p.Add('The document is not the stored lease''s') }
    if ($Run.Stdout -match 'm9g') { $p.Add('The document names an instance type') }
  }
  return , $p
}

# The problems with the lease a `create` answered, as stored in the lease state file: pending, the size, and
# a driver.
function Get-SdStoredProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Size)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.Json -isnot [hashtable] -or [string]$Run.Json['id'] -cnotmatch '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z') { $p.Add('There is no lease ID to look up'); return , $p }
  $rec = Get-CcRecord $Lab ([string]$Run.Json['id'])
  if ($null -eq $rec) { $p.Add('The lease is not in the lease state file'); return , $p }
  if ($rec['state'] -cne 'pending') { $p.Add("The stored lease is '$($rec['state'])', not pending") }
  if ($rec['size'] -cne $Size) { $p.Add("The stored lease's size is '$($rec['size'])', not '$Size'") }
  if ([string]::IsNullOrEmpty([string]$rec['driver'])) { $p.Add('The stored pending lease carries no driver') }
  if (-not (Test-CcStateFile $Lab)) { $p.Add('The lease state file does not validate against its schema') }
  return , $p
}

# The problems with a stored record that is not, by its canonical text, what the case wrote.
function Get-SdRecordAsWasProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$Canonical)
  $p = [System.Collections.Generic.List[string]]::new()
  $rec = Get-CcRecord $Lab $Id
  if ($null -eq $rec) { $p.Add('The stored pending lease is gone from the lease state file') }
  elseif ((ConvertTo-CcCanonical $rec) -cne $Canonical) { $p.Add('The stored pending lease changed') }
  return , $p
}

# The problems with the stand-in's arguments naming an instance type: its started.log line holds none.
function Get-SdNoTypeArgProblems {
  param([Parameter(Mandatory)]$StandIn)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($l in @(Get-PvStartedLines $StandIn)) { if ($l -match '(?i)InstanceType') { $p.Add('The stand-in was given an -InstanceType') } }
  return , $p
}

# The problems with hosts.json after a start: the registry is the seed's records, the project's with
# last_attempt_at set to a time within the command's run and last_success_at removed in the same write, and
# nothing else changed, byte for byte (driver.md, "Starting a provisioning run", step 4; pin P24).
function Get-SdStartWriteProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Seed, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][DateTime]$Before, [Parameter(Mandatory)][DateTime]$After)
  $p = [System.Collections.Generic.List[string]]::new()
  $rec = Get-PvHostRecord $Lab $Project
  if ($null -eq $rec) { $p.Add('hosts.json holds no record for the project'); return , $p }
  if ($rec.ContainsKey('last_success_at')) { $p.Add('The record still holds last_success_at') }
  $time = [string]$rec['last_attempt_at']
  if (-not (Test-PvTimeBetween -Time $time -Before $Before -After $After)) { $p.Add('last_attempt_at is not within the command''s run') }
  $want = foreach ($r in @($Seed)) {
    if ($r['project'] -ceq $Project) {
      $c = [ordered]@{}
      foreach ($k in $r.Keys) { if ($k -cne 'last_success_at') { $c[$k] = $r[$k] } }
      $c['last_attempt_at'] = $time
      $c
    } else { $r }
  }
  foreach ($x in (Get-PvHostsTextProblems -Lab $Lab -Expected (Get-SdHostsText @($want)))) { $p.Add($x) }
  return , $p
}

# The problems with hosts.json not being byte for byte what it was before the run.
function Get-SdRegistryAsWasProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Hash)
  $p = [System.Collections.Generic.List[string]]::new()
  if ((Get-PvHostsHash $Lab) -cne $Hash) { $p.Add('hosts.json is not as it was') }
  return , $p
}

# The problems with a stand-in that was started once before and MUST NOT have been started again: after a
# settling wait, its .cmd logged still one start.
function Get-SdNoFurtherStartProblems {
  param([Parameter(Mandatory)]$StandIn, [int]$SettleSeconds = 3)
  $p = [System.Collections.Generic.List[string]]::new()
  Start-Sleep -Seconds $SettleSeconds
  $n = @(Get-PvStartedLines $StandIn).Count
  if ($n -ne 1) { $p.Add("The stand-in was started $n time(s) in all, not once") }
  return , $p
}

# --- One case --------------------------------------------------------------------------------------------

# Runs one case of a row: seeds hosts.json with the project's record ready at m9g.xlarge, with last_attempt_at
# AttemptAgo seconds before now and last_success_at SuccessAgo seconds before now (negative for none), the
# project list, a stored pending lease of the project when -Pending or the command is a `get`, and a held
# provision.lock when -Lock; runs `get` of that lease or `create --size l`; and checks that the stand-in
# was started once with exactly provision -Host <project> and the registry written as step 4 says, or that
# it was not started and the registry is as it was. -SecondGet makes a second `get` straight after the first
# and checks that it starts nothing and leaves the registry as the first one left it.
function Invoke-SdCase {
  param(
    [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][ValidateSet('get', 'create')][string]$Command,
    [Parameter(Mandatory)][int]$AttemptAgo, [Parameter(Mandatory)][int]$SuccessAgo, [Parameter(Mandatory)][bool]$Start,
    [switch]$Pending, [switch]$Lock, [switch]$SecondGet
  )
  $sub = @{
    Command = $Command; AttemptAgo = $AttemptAgo; SuccessAgo = $SuccessAgo; Start = $Start
    Pending = ($Pending.IsPresent -or $Command -ceq 'get'); Lock = $Lock.IsPresent; SecondGet = $SecondGet.IsPresent
  }
  Use-PvLab -Name $Name -Body {
    param($lab)
    $now = Get-CcNowSecond
    $attempt = ConvertTo-CcTime $now.AddSeconds(-$sub.AttemptAgo)
    $success = $(if ($sub.SuccessAgo -ge 0) { ConvertTo-CcTime $now.AddSeconds(-$sub.SuccessAgo) } else { '' })
    $seed = @(
      (New-SdRecord -Project 'swimcoach' -InstanceType $script:SdType),
      (New-SdRecord -Project $script:SdProject -InstanceId $script:SdInstanceId -InstanceType $script:SdType -LastAttemptAt $attempt -LastSuccessAt $success)
    )
    Set-SdHosts $lab $seed
    Set-PvProjects -Lab $lab -Names @($script:SdProject)
    $pendingId = ''
    $pendingCanon = ''
    if ($sub.Pending) {
      $pending = New-SdPendingLease -Project $script:SdProject
      Set-SdLeases $lab @($pending)
      $pendingId = [string]$pending['id']
      $pendingCanon = ConvertTo-CcCanonical (Get-CcRecord $lab $pendingId)
    }
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    $lock = $null
    if ($sub.Lock) { $lock = Open-PvLock -Lab $lab -Path (Get-PvProvisionLockPath $lab $script:SdProject) }
    try {
      $hash = Get-PvHostsHash $lab
      $before = Get-CcNowSecond
      if ($sub.Command -ceq 'get') { $run = Invoke-PvLease -Lab $lab -Arguments @('get', $pendingId) -StandIn $standIn }
      else { $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', $script:SdProject, '--size', 'l') -StandIn $standIn }
      $after = Get-CcNowSecond
      $p = Get-SdPendingDocProblems -Run $run -Project $script:SdProject -Size 'l' -Id $(if ($sub.Command -ceq 'get') { $pendingId } else { '' })
      foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
      if ($sub.Command -ceq 'create') { foreach ($x in (Get-SdStoredProblems -Lab $lab -Run $run -Size 'l')) { $p.Add($x) } }
      if ($sub.Pending) { foreach ($x in (Get-SdRecordAsWasProblems -Lab $lab -Id $pendingId -Canonical $pendingCanon)) { $p.Add($x) } }
      if ($sub.Start) {
        foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project $script:SdProject)) { $p.Add($x) }
        foreach ($x in (Get-SdNoTypeArgProblems -StandIn $standIn)) { $p.Add($x) }
        foreach ($x in (Get-SdStartWriteProblems -Lab $lab -Seed $seed -Project $script:SdProject -Before $before -After $after)) { $p.Add($x) }
      } else {
        foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
        foreach ($x in (Get-SdRegistryAsWasProblems -Lab $lab -Hash $hash)) { $p.Add($x) }
      }
      if ($sub.SecondGet) {
        $left = Get-PvHostsHash $lab
        $again = Invoke-PvLease -Lab $lab -Arguments @('get', $pendingId) -StandIn $standIn
        foreach ($x in (Get-SdPendingDocProblems -Run $again -Project $script:SdProject -Size 'l' -Id $pendingId)) { $p.Add("Second get: $x") }
        foreach ($x in (Get-SdNoFurtherStartProblems -StandIn $standIn)) { $p.Add("Second get: $x") }
        foreach ($x in (Get-SdRegistryAsWasProblems -Lab $lab -Hash $left)) { $p.Add("Second get: $x (the registry as the first get's write left it)") }
        foreach ($x in (Get-SdRecordAsWasProblems -Lab $lab -Id $pendingId -Canonical $pendingCanon)) { $p.Add("Second get: $x") }
      }
    } finally { if ($lock) { try { $lock.Dispose() } catch { } } }
    return , $p
  }
}

$starting = 'driver.md#starting-a-provisioning-run'
$verb = @{ get = 'a get of the project''s stored pending lease'; create = 'a create --size l' }

# ---- Y1 -----------------------------------------------------------------------------------------------
foreach ($cmd in 'get', 'create') {
  Invoke-SdCase -Command $cmd -AttemptAgo 60 -SuccessAgo 30 -Start $true -Name "${starting}: Y1 (steps 2 and 4): With a record ready at m9g.xlarge whose last_attempt_at is 1 minute before now and last_success_at 30 seconds before now, $($verb[$cmd]) starts the stand-in once with exactly provision -Host <project>, sets last_attempt_at to now, leaves the record with no last_success_at and every other member as it was, and answers pending"
}

# ---- Y2 -----------------------------------------------------------------------------------------------
foreach ($seedKind in 'older', 'absent') {
  $successAgo = $(if ($seedKind -ceq 'older') { 180 } else { -1 })
  $successText = $(if ($seedKind -ceq 'older') { 'last_success_at 2 minutes before it' } else { 'no last_success_at' })
  foreach ($cmd in 'get', 'create') {
    Invoke-SdCase -Command $cmd -AttemptAgo 60 -SuccessAgo $successAgo -Start $false -Name "${starting}: Y2 (step 2): With a record ready at m9g.xlarge whose last_attempt_at is 1 minute before now and $successText, $($verb[$cmd]) answers pending, starts nothing and leaves the registry as it was"
    $dueSuccess = $(if ($seedKind -ceq 'older') { 360 + 120 } else { -1 })
    Invoke-SdCase -Command $cmd -AttemptAgo 360 -SuccessAgo $dueSuccess -Start $true -Name "${starting}: Y2 (step 2): With a record ready at m9g.xlarge whose last_attempt_at is 6 minutes before now and $successText, $($verb[$cmd]) starts the stand-in once with exactly provision -Host <project>, sets last_attempt_at to now and leaves the record with no last_success_at"
  }
}

# ---- Y3 -----------------------------------------------------------------------------------------------
Invoke-SdCase -Command 'get' -AttemptAgo 60 -SuccessAgo 60 -Start $true -SecondGet -Name "${starting}: Y3 (steps 2 and 4; pin P24): With last_success_at equal to last_attempt_at, 1 minute before now, a get of a stored pending lease of the project starts the stand-in once and removes last_success_at in the write that sets last_attempt_at, and a second get straight after it starts nothing and leaves the registry as that write left it"

# ---- Y4 -----------------------------------------------------------------------------------------------
foreach ($cmd in 'get', 'create') {
  Invoke-SdCase -Command $cmd -AttemptAgo 60 -SuccessAgo -1 -Start $false -Pending -Name "${starting}: Y4 (step 2, and After the run; pin P22): With the record as a run whose placing failed or left a lease leaves it, last_attempt_at 1 minute before now and no last_success_at, beside a stored pending lease of the project, $($verb[$cmd]) answers pending, starts nothing and leaves the registry as it was"
  Invoke-SdCase -Command $cmd -AttemptAgo 360 -SuccessAgo -1 -Start $true -Pending -Name "${starting}: Y4 (step 2, and After the run; pin P22): With that record's last_attempt_at 6 minutes before now, $($verb[$cmd]) starts the stand-in once with exactly provision -Host <project> and sets last_attempt_at to now"
}

# ---- Y5 -----------------------------------------------------------------------------------------------
foreach ($seedKind in 'after', 'equal') {
  $successAgo = $(if ($seedKind -ceq 'after') { 30 } else { 60 })
  $successText = $(if ($seedKind -ceq 'after') { 'last_success_at 30 seconds before now, after last_attempt_at' } else { 'last_success_at equal to last_attempt_at' })
  foreach ($cmd in 'get', 'create') {
    Invoke-SdCase -Command $cmd -AttemptAgo 60 -SuccessAgo $successAgo -Start $false -Pending -Lock -Name "${starting}: Y5 (step 2): With last_attempt_at 1 minute before now, $successText, a stored pending lease of the project and the project's provision.lock held by the check, $($verb[$cmd]) answers pending, starts nothing and leaves the registry as it was"
  }
}

Exit-ConformanceRun

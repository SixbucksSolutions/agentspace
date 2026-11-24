#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of what v0.09 adds to the operator's `provision`: the run finds its instance type itself, at
# its deploy step, from the lease state file, and its step 9 logs what `place_pending` answers (docs/operator.md,
# "provision", steps 4 and 9, "Its log"; "Which checks show it", the rows that name E0.09-T04-S02; docs/state.md,
# "The lease state lock", "The host registry lock", "The order of the locks"; docs/driver.md, "A resizing run",
# "place_pending"), run through the test seam. Card #0206 (E0.09-T04-S02).
#
# EXPECTED TO FAIL AGAINST v0.08's OPERATOR, which finds no type and logs no `refused` or `left` line. A v0.09
# build MUST pass every result line.
#
# What it does: each case makes its own throwaway home with the stand-ins of
# conformance/operator/host-provision-common.ps1, seeds a host registry (a typed `ready` record, a `creating` one or
# none) and a lease state file whose times are set relative to the real clock, and runs `provision -Host <project>`.
# A case about the type ends the run at its start step, by a stand-in `aws` whose `ec2 describe-instances` fails, so
# that the deploy's arguments, its log line and the type it recorded are shown without the steps after it. The
# stand-in `aws` also tries `leases.lock` and `hosts.lock` at each call, and snapshots `hosts.json` at it.
# `place_pending`'s own placing decisions are driver.md's: this file's cases show only what the run logs of what the
# call returned, and the leases' records it left.
#
# Rows of "Which checks show it" and the results that show them (a result's name starts with its tag):
#   - `provision`'s lease state lock held past 30 seconds (pin P35): the `lock:` results.
#   - The lease state lock not takeable for another reason, a folder at its path: the `lockpath:` results.
#   - `provision`'s type against a seeded lease state file, and the locks (pin P26): the `type:` results, among them
#     `type: locks ...`, and the `failed:` results, a failed `cloudformation deploy` with and without a type.
#   - The lease state file not valid: the `leasefile:` results.
#   - The log lines `deploy done type=<type>`, `deploy failed type=<type> reason=...`, `deploy done` with no type, a
#     resize on a `ready` host running every step, and a failed deploy on it: the `log:` and `resize:` results.
#   - Step 9's lines (pin P37): the `place:` results.
#   - The removal before the read under both locks (the row of operator.md that maps to this page's part): the
#     `type: locks` results.
#
# SAFETY. See test-unmet-host-provision.ps1: the harness's isolation, the AWS gate, stand-ins for `aws`, `ssh`,
# `scp` and `go`, made-up instance IDs. A result's reason never quotes a path or the program's output. A lock a case
# holds is held by this process, in the throwaway home, and released in a finally. A stand-in `aws` set never reaches
# a real one.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-provision-size.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$stack = 'agentspace-host-agentspace'
$oldTime = '2026-01-01T00:00:00Z'
$ec2Error = 'An error occurred (UnauthorizedOperation) when calling the DescribeInstances operation: You are not authorized to perform this operation.'
$deployFailed = 'Stack deploy failed; see the stack events in the console.'
$dsc = 'aws ec2 describe-instances'
$syncCalls = @('ssh:sync', 'scp', 'scp', 'ssh:sync') + $script:HpPatchWaitSequence
$startSteps = 'run begun,deploy done,start done,sync done,bootstrap done,record done'

# ---- Seeds -----------------------------------------------------------------------------------------------------
function Get-SzId([int]$N) { '{0:x8}-0000-4000-8000-{1:x12}' -f $N, $N }
function Get-SzAlias([int]$N, [string]$Project = 'agentspace') { 'agentspace-{0}-{1:x8}' -f $Project, $N }
function Format-SzTime([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture) }
# One lease record, as the lease state file holds it. A `ready` record expires $ExpiresInMinutes from the real
# clock now (a negative value makes an expired one); -NoDriver leaves `driver` out.
function New-SzLease {
  param([Parameter(Mandatory)][int]$N, [string]$State = 'pending', [string]$Size = 'm', [string]$Project = 'agentspace', [string]$Driver = 'ec2', [switch]$NoDriver, [int]$ExpiresInMinutes = 60)
  $r = [ordered]@{ id = (Get-SzId $N); project = $Project; state = $State; platform = 'linux/arm64'; profile = 'agent-linux'; size = $Size; ttl_seconds = 3600; created_at = '2026-11-21T00:00:00Z' }
  if ($State -eq 'ready') {
    $r['expires_at'] = Format-SzTime ([DateTime]::UtcNow.AddMinutes($ExpiresInMinutes))
    $r['ssh'] = [ordered]@{ host_alias = (Get-SzAlias $N $Project); user = 'ubuntu' }
  }
  if ($State -eq 'released') { $r['released_at'] = '2026-11-21T00:10:00Z' }
  if ($State -eq 'unsatisfiable') { $r['reasons'] = @([ordered]@{ code = 'no_capacity'; message = 'No capacity.' }) }
  if (-not $NoDriver) { $r['driver'] = $Driver }
  $r
}
function Set-SzLeasesAt($H, [string]$Dir, $Records) {
  Write-HpFile -H $H -Path (Join-Path $Dir 'leases.json') -Text ((ConvertTo-Json -InputObject ([ordered]@{ version = 0; leases = @($Records | Where-Object { $null -ne $_ }) }) -Compress -Depth 8) + "`n")
}
# The host's record for a seed: `ready` holds the type `m9g.xlarge` (New-HpRecord gives every `ready` record one).
function New-SzSeedRecord([string]$Kind) {
  switch ($Kind) {
    'ready' { New-HpRecord 'agentspace' 'ready' $script:HpHostId $oldTime }
    'creating' { New-HpRecord 'agentspace' 'creating' '' $oldTime }
    default { $null }
  }
}
function Get-SzAws([string]$Kind, [string]$ReadBack, [hashtable]$Override = @{}) {
  $ds = switch ($Kind) {
    'box' { @((New-HpStack 'agentbox' 'UPDATE_COMPLETE' $script:HpBoxId -InstanceType $ReadBack)) }
    'new' { @((New-HpNoStack $stack), (New-HpStack $stack 'CREATE_COMPLETE' $script:HpHostId -InstanceType $ReadBack)) }
    default { @((New-HpStack $stack 'UPDATE_COMPLETE' $script:HpHostId -InstanceType $ReadBack)) }
  }
  $aws = @{ 'cloudformation describe-stacks' = $ds; 'cloudformation deploy' = @((New-HpStep "deploy-out`n")); 'ec2 describe-instances' = @((New-HpDescribe)) }
  foreach ($k in $Override.Keys) { $aws[$k] = $Override[$k] }
  $aws
}
function Get-SzLocks($H, [string]$StateDir) { @{ lease = (Join-Path $StateDir 'leases.lock'); hosts = (Join-Path $H.UserProfile '.agentspace\hosts.lock') } }
function Format-SzCanon($V) {
  if ($null -eq $V) { return 'null' }
  if ($V -is [System.Collections.IDictionary]) { return '{' + ((@($V.Keys | Sort-Object { [string]$_ } -CaseSensitive) | ForEach-Object { "`"$_`":" + (Format-SzCanon $V[$_]) }) -join ',') + '}' }
  if ($V -is [System.Collections.IEnumerable] -and $V -isnot [string]) { return '[' + ((@($V) | ForEach-Object { Format-SzCanon $_ }) -join ',') + ']' }
  ConvertTo-Json -InputObject $V -Compress
}
function Get-SzLeaseRecord($H, [string]$Dir, [string]$Id) {
  $f = Join-Path $Dir 'leases.json'
  if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { return $null }
  try { foreach ($r in (ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($f)) -AsHashtable -DateKind String).leases) { if ($r['id'] -ceq $Id) { return $r } } } catch { }
  $null
}

# ---- Checks shared by the cases --------------------------------------------------------------------------------
# What a record holds after a run: its state, its instance ID ('' means none) and its instance_type ($null means none).
function Get-SzRecordProblems($H, [string]$Project, [string]$State, [string]$Id, $Type) {
  $p = [System.Collections.Generic.List[string]]::new()
  $rec = Get-HpRecord $H $Project
  if ($null -eq $rec) { $p.Add('The record is gone'); return $p.ToArray() }
  if ($rec['state'] -cne $State) { $p.Add("The record's state is '$($rec['state'])', not $State") }
  if ($Id) { if ($rec['instance_id'] -cne $Id) { $p.Add('The record does not hold the instance ID it should') } }
  elseif ($rec.ContainsKey('instance_id')) { $p.Add('The record holds an instance ID') }
  if ($null -eq $Type) { if ($rec.ContainsKey('instance_type')) { $p.Add('The record holds an instance_type, which it MUST NOT') } }
  elseif ($rec['instance_type'] -cne $Type) { $p.Add("The record's instance_type is not '$Type'") }
  if (@($rec.Keys | Where-Object { $_ -notin 'project', 'stack', 'state', 'instance_id', 'instance_type', 'last_attempt_at' }).Count -gt 0) { $p.Add('The record holds another member') }
  $p.ToArray()
}
# The box's record, when the run was for another host, is as it was seeded.
function Get-SzBoxProblems($H) {
  $b = Get-HpRecord $H 'swimcoach'
  if ($null -eq $b -or $b['state'] -cne 'ready' -or $b['instance_id'] -cne $script:HpBoxId -or $b['instance_type'] -cne 'm9g.xlarge') { return @('The box''s record changed') }
  @()
}
# The deploy's overrides for a host of kind exists (its stack is there), new (made by the run) or box, with $Want the
# type the run is to pass, or '' for none (operator.md, deploy, step 4; pin P32: InstanceType last).
function Get-SzDeployProblems($Calls, [string]$Kind, [string]$Want, $H) {
  $p = [System.Collections.Generic.List[string]]::new()
  $d = @($Calls | Where-Object { (Get-HpCallName $_) -eq 'aws cloudformation deploy' }) | Select-Object -First 1
  if (-not $d) { $p.Add('No cloudformation deploy was made'); return $p.ToArray() }
  $a = @($d['a'])
  $i = [Array]::IndexOf($a, '--parameter-overrides')
  $tail = if ($i -ge 0) { @($a | Select-Object -Skip ($i + 1)) } else { @() }
  switch ($Kind) {
    'exists' {
      if ($Want) { if ($i -lt 0 -or ($tail -join [char]0) -cne "InstanceType=$Want") { $p.Add("The deploy of a stack that exists does not pass InstanceType=$Want alone") } }
      elseif ($i -ge 0) { $p.Add('The deploy of a stack that exists, given no type, passes parameter overrides') }
    }
    'new' {
      $caPub = [IO.File]::ReadAllText((Join-Path $H.UserProfile '.agentspace\ca\agentspace_ca.pub')).Trim()
      $caKey = ($caPub -split '\s+')[0..1] -join ' '
      $wantArgs = @("SshPublicKey=$($script:HpKeyLine)", 'SubnetId=SUBNET_VALUE', 'VpcId=VPC_VALUE', "HostName=$stack", "CaPublicKey=$caKey")
      if ($Want) { $wantArgs += "InstanceType=$Want" }
      if ($i -lt 0 -or ($tail -join [char]0) -cne ($wantArgs -join [char]0)) { $p.Add('The deploy of a stack it creates does not pass the creation parameters, then InstanceType=<type> last when a type was found') }
    }
    'box' {
      $other = @($tail | Where-Object { $_ -cnotmatch '^InstanceType=' } | Sort-Object)
      $wantOther = @(@("SshPublicKey=$($script:HpKeyLine)", 'SubnetId=SUBNET_VALUE', 'VpcId=VPC_VALUE', 'RootVolumeGiB=64') | Sort-Object)
      if ($i -lt 0 -or ($other -join [char]0) -cne ($wantOther -join [char]0)) { $p.Add('The box''s deploy does not pass SshPublicKey and every member of the file but InstanceType') }
      $its = @($tail | Where-Object { $_ -cmatch '^InstanceType=' })
      if ($Want) { if ($its.Count -ne 1 -or $tail[-1] -cne "InstanceType=$Want") { $p.Add("The box's deploy does not pass InstanceType=$Want last") } }
      elseif ($its.Count -ne 0) { $p.Add('The box''s deploy, given no type, passes an InstanceType') }
    }
  }
  $p.ToArray()
}

# ---- A case about the type found --------------------------------------------------------------------------------
# $C: N (the name), Seed (ready, creating or none), Kind (exists, new or box), Leases (a scriptblock giving the lease
# records, or $null for no lease state file), Want (the type the run is to pass, or empty for none), Locks (adds the
# result about the removal and the locks), StateSub (a folder under the home that AGENTSPACE_STATE_DIR names) and
# Extra (a scriptblock taking the home, the state folder and the problems list, for this case's own checks).
# The run ends at its start step, which fails, so a `ready` host stays `ready` and a `creating` one is `failed`.
function Test-SzType($C) {
  $kind = if ($C.Kind) { $C.Kind } else { 'exists' }
  Invoke-HpInHome -Name "type: $($C.N)" -Body {
    param($h, $s)
    $project = if ($kind -eq 'box') { 'swimcoach' } else { 'agentspace' }
    $stateDir = if ($C.StateSub) { Join-Path $h.UserProfile $C.StateSub } else { $h.StateDir }
    $rb = if ($C.Want) { $C.Want } else { 'm9g.large' }
    $records = @(if ($kind -ne 'box') { New-SzSeedRecord $C.Seed | Where-Object { $_ } })
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records $records)
    if ($C.Leases) { Set-SzLeasesAt $h $stateDir (& $C.Leases) }
    $leasesBefore = Get-HpFileBytes (Join-Path $stateDir 'leases.json')
    $aws = Get-SzAws $kind $rb @{ 'ec2 describe-instances' = @((New-HpStep '' $ec2Error 254)) }
    Set-HpScenario -H $h -S $s -LockProject $project -Scenario @{ aws = $aws; rules = @(); locks = (Get-SzLocks $h $stateDir) }
    $extra = @{}
    if ($C.StateSub) { $extra['AGENTSPACE_STATE_DIR'] = $stateDir }
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', $project) -Extra $extra
    $calls = @(Get-HpCalls $s)
    $log = Read-HpLog $h $project

    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    $wantSeq = if ($kind -eq 'box') { @('aws cloudformation deploy', 'aws cloudformation describe-stacks', $dsc) } else { @('aws cloudformation describe-stacks', 'aws cloudformation deploy', 'aws cloudformation describe-stacks', $dsc) }
    if (((Get-HpSequence $calls) -join ',') -cne ($wantSeq -join ',')) { $p.Add("The calls are not the deploy's and the start's up to its failure ($($calls.Count) calls)") }
    foreach ($m in (Get-SzDeployProblems $calls $kind $C.Want $h)) { $p.Add($m) }
    Write-HpProblems "type: $($C.N): the deploy passes $(if ($C.Want) { "InstanceType=$($C.Want)" } else { 'no InstanceType' })" $p

    $p = New-HpProblems
    if ((Get-HpLogShape $log) -cne 'run begun,deploy done,start failed') { $p.Add('The log''s lines are not: run begun, deploy done, start failed') }
    else {
      $wantRest = if ($C.Want) { " type=$($C.Want)" } else { '' }
      if ($log.Lines[1].Rest -cne $wantRest) { $p.Add($(if ($C.Want) { 'The deploy line is not deploy done type=<the type the run passed>' } else { 'The deploy line is not deploy done with no type field' })) }
    }
    $wantState = if ($kind -eq 'new') { 'failed' } else { 'ready' }
    $wantId = if ($kind -eq 'box') { $script:HpBoxId } else { $script:HpHostId }
    foreach ($m in (Get-SzRecordProblems $h $project $wantState $wantId $rb)) { $p.Add($m) }
    if ($kind -ne 'box') { foreach ($m in (Get-SzBoxProblems $h)) { $p.Add($m) } }
    if (-not (Test-HpSameBytes (Get-HpFileBytes (Join-Path $stateDir 'leases.json')) $leasesBefore)) { $p.Add('The lease state file changed: the deploy step writes nothing to it') }
    if ($C.Extra) { & $C.Extra $h $stateDir $p }
    Write-HpProblems "type: $($C.N): the log's deploy line, and the record holds the type the stack's read-back gave, $rb" $p

    if ($C.Locks) {
      $p = New-HpProblems
      if ($calls.Count -eq 0) { $p.Add('No call was made') }
      else {
        $at = Get-HpRecordFromText (Get-HpSnapshotText $s $calls[0]['n']) $project
        if ($null -eq $at) { $p.Add('No record at the first call') }
        else {
          if ($at.ContainsKey('instance_type')) { $p.Add('The record still held its instance_type at the first call to AWS') }
          if ($at['state'] -cne $(if ($kind -eq 'new') { 'creating' } else { 'ready' })) { $p.Add('The record''s state is not the run''s own at the first call') }
          if ($kind -ne 'new' -and $at['instance_id'] -cne $wantId) { $p.Add('The removal changed the instance ID: it changes nothing else in the record') }
          if (@($at.Keys | Where-Object { $_ -notin 'project', 'stack', 'state', 'instance_id', 'last_attempt_at' }).Count -gt 0) { $p.Add('The record holds another member at the first call') }
        }
        foreach ($call in $calls) {
          if ($call['locks']['lease'] -ne $false) { $p.Add("leases.lock was not free, or not there, at call $($call['n'])"); break }
          if ($call['locks']['hosts'] -ne $false) { $p.Add("hosts.lock was not free, or not there, at call $($call['n'])"); break }
        }
      }
      Write-HpProblems "type: locks: $($C.N): the record's instance_type is removed before the stand-in aws is first called, and leases.lock and hosts.lock are both free at every call (steps 4.1 to 4.4; pin P26)" $p
    }
  }
}

# ---- A case in which the deploy step fails before any call to AWS ---------------------------------------------------
# $C: Tag, N, Seed (ready or creating), Target (lock or file: which path the message names), Setup (a scriptblock that
# takes the home and the state folder, makes the failure, and returns a held lock's stream or nothing), Pattern (the
# message, without its prefix, as a regular expression with a path group p), KeepType (the record keeps its
# instance_type), MinSeconds (the run is to take at least that long) and StateSub.
function Test-SzEarly($C) {
  Invoke-HpInHome -Name "$($C.Tag): $($C.N)" -Body {
    param($h, $s)
    $stateDir = if ($C.StateSub) { Join-Path $h.UserProfile $C.StateSub } else { $h.StateDir }
    $target = Join-Path $stateDir $(if ($C.Target -eq 'lock') { 'leases.lock' } else { 'leases.json' })
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @(New-SzSeedRecord $C.Seed))
    Set-SzLeasesAt $h $stateDir @((New-SzLease 1 -Size 'l'))
    $held = $null
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $t0 = [DateTime]::UtcNow
    try {
      $held = & $C.Setup $h $stateDir
      $leasesBefore = Get-HpFileBytes (Join-Path $stateDir 'leases.json')
      Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-SzAws 'exists' 'm9g.2xlarge'); rules = @(); locks = (Get-SzLocks $h $stateDir) }
      $extra = @{}
      if ($C.StateSub) { $extra['AGENTSPACE_STATE_DIR'] = $stateDir }
      $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace') -Extra $extra
    } finally { if ($held) { $held.Dispose() } }
    $clock.Stop()
    $t1 = [DateTime]::UtcNow
    $calls = @(Get-HpCalls $s)
    $log = Read-HpLog $h 'agentspace'
    $lines = Get-HpLines $r.Stderr

    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    $msg = ''
    $m = if ($lines.Count -gt 0) { [regex]::Match($lines[-1], '^agentspace: (?<msg>' + $C.Pattern.TrimStart('^') + ')') } else { $null }
    if ($null -eq $m -or -not $m.Success) { $p.Add('The last line of stderr is not agentspace: <the message>') }
    else {
      $msg = $m.Groups['msg'].Value
      if (-not (Test-HpSamePath $m.Groups['p'].Value $target)) { $p.Add('The message does not name the full path of the file at fault') }
    }
    if ($C.MinSeconds -and $clock.Elapsed.TotalSeconds -lt $C.MinSeconds) { $p.Add('It gave up before the lock''s 30 seconds') }
    Write-HpProblems "$($C.Tag): $($C.N): exit 1 with the message as stderr's last line" $p

    $p = New-HpProblems
    if ($calls.Count -ne 0) { $p.Add("The stand-ins recorded $($calls.Count) calls: the step fails before any call") }
    if ((Get-HpLogShape $log) -cne 'run begun,deploy failed') { $p.Add('The log''s lines are not: run begun, deploy failed') }
    else {
      $dl = $log.Lines[1]
      if ($dl.Rest -cne " reason=$msg") { $p.Add('The deploy line is not deploy failed reason=<the message without its prefix>, with no type field') }
      if (-not (Test-HpTimeBetween $dl.Time $t0 $t1)) { $p.Add('The deploy line''s time is not the time it was written') }
    }
    Write-HpProblems "$($C.Tag): $($C.N): the run's one deploy line is deploy failed reason=<message>, with no type field, and no call to AWS, ssh or scp is made" $p

    $p = New-HpProblems
    $rec = Get-HpRecord $h 'agentspace'
    if ($C.Seed -eq 'ready') {
      foreach ($x in (Get-SzRecordProblems $h 'agentspace' 'ready' $script:HpHostId $(if ($C.KeepType) { 'm9g.xlarge' } else { $null }))) { $p.Add($x) }
    } else {
      foreach ($x in (Get-SzRecordProblems $h 'agentspace' 'failed' '' $null)) { $p.Add($x) }
    }
    if ($null -ne $rec -and -not (Test-HpTimeBetween $rec['last_attempt_at'] $t0 $t1)) { $p.Add('last_attempt_at is not the time of this run''s start') }
    if (-not (Test-HpSameBytes (Get-HpFileBytes (Join-Path $stateDir 'leases.json')) $leasesBefore)) { $p.Add('The lease state file changed: the lease was not left pending, as it was') }
    if (-not (Test-HpFreeLock (Get-HpLockFile $h 'agentspace'))) { $p.Add('provision.lock is still held after the run ended') }
    if (@($log.Lines | Where-Object { $_.Step -eq 'place' -or ($_.Step -eq 'run' -and $_.Outcome -eq 'done') }).Count -gt 0) { $p.Add('The log holds a place line or run done after a failed step') }
    Write-HpProblems "$($C.Tag): $($C.N): $(if ($C.Seed -eq 'ready') { "a ready host is left ready, $(if ($C.KeepType) { 'its instance_type kept' } else { 'its record without instance_type' })" } else { 'a creating host is recorded failed' }), the leases are left pending and the file as it was" $p
  }
}

# ---- The lease state lock (pin P35) ----------------------------------------------------------------------------
$heldPattern = '^The lease state lock (?<p>.+) is held by another program; try again\.$'
$cannotPattern = '^Cannot take (?<p>.+?): .+\.$'
foreach ($seed in 'ready', 'creating') {
  Test-SzEarly @{
    Tag = 'lock'; N = "leases.lock held past its 30 seconds, for a $seed host"; Seed = $seed; Target = 'lock'; KeepType = $true; MinSeconds = 25
    Setup = { param($h, $dir) Hold-HpLock -H $h -Path (Join-Path $dir 'leases.lock') }; Pattern = $heldPattern
  }
  Test-SzEarly @{
    Tag = 'lockpath'; N = "a folder at leases.lock's path, for a $seed host"; Seed = $seed; Target = 'lock'; KeepType = $true
    Setup = { param($h, $dir) $f = Join-Path $dir 'leases.lock'; New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $f) | Out-Null }; Pattern = $cannotPattern
  }
}
Test-SzEarly @{
  N = 'a folder at the lock path in the folder AGENTSPACE_STATE_DIR names'; Tag = 'lockpath'; Seed = 'ready'; Target = 'lock'; KeepType = $true; StateSub = 'moved-state'
  Setup = { param($h, $dir) $f = Join-Path $dir 'leases.lock'; New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $f) | Out-Null }; Pattern = $cannotPattern
}

# ---- The lease state file not valid ----------------------------------------------------------------------------
$unreadable = '^The lease state file (?<p>.+) cannot be read: .+\.$'
foreach ($seed in 'ready', 'creating') {
  Test-SzEarly @{
    Tag = 'leasefile'; N = "bad JSON, for a $seed host"; Seed = $seed; Target = 'file'; KeepType = $false; Pattern = $unreadable
    Setup = { param($h, $dir) Write-HpFile -H $h -Path (Join-Path $dir 'leases.json') -Text '{"version":0,"leases":[' }
  }
  Test-SzEarly @{
    Tag = 'leasefile'; N = "two records with one id, for a $seed host"; Seed = $seed; Target = 'file'; KeepType = $false; Pattern = $unreadable
    Setup = { param($h, $dir) Set-SzLeasesAt $h $dir @((New-SzLease 1 -Size 'l'), (New-SzLease 1 -Size 's')) }
  }
  Test-SzEarly @{
    Tag = 'leasefile'; N = "a folder at its path, for a $seed host"; Seed = $seed; Target = 'file'; KeepType = $false; Pattern = $unreadable
    Setup = { param($h, $dir) $f = Join-Path $dir 'leases.json'; Remove-HpPath -H $h -Path $f; New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $f) | Out-Null }
  }
}

# ---- The type found, against a seeded lease state file ------------------------------------------------------------
Test-SzType @{
  N = 'pending s and l give m9g.2xlarge'; Seed = 'ready'; Want = 'm9g.2xlarge'; Locks = $true
  Leases = { @((New-SzLease 1 -Size 's'), (New-SzLease 2 -Size 'l')) }
}
Test-SzType @{
  N = 'a pending m alone gives m9g.xlarge'; Seed = 'ready'; Want = 'm9g.xlarge'
  Leases = { @((New-SzLease 1 -Size 'm')) }
}
Test-SzType @{
  N = 'a pending xxl alone gives no type'; Seed = 'ready'; Want = ''; Locks = $true
  Leases = { @((New-SzLease 1 -Size 'xxl')) }
}
Test-SzType @{
  N = 'a pending lease of a size in SIZES beside one outside it: only the first counts'; Seed = 'ready'; Want = 'm9g.large'
  Leases = { @((New-SzLease 1 -Size 'xxl'), (New-SzLease 2 -Size 's')) }
}
Test-SzType @{
  N = 'a pending l whose driver is not the EC2 driver''s name, or that has none, is ignored'; Seed = 'ready'; Want = 'm9g.large'
  Leases = { @((New-SzLease 1 -Size 'l' -Driver 'other'), (New-SzLease 2 -Size 'xl' -NoDriver), (New-SzLease 3 -Size 's')) }
}
Test-SzType @{
  N = 'a pending l whose driver is not the EC2 driver''s name, alone, gives no type'; Seed = 'ready'; Want = ''
  Leases = { @((New-SzLease 1 -Size 'l' -Driver 'other'), (New-SzLease 2 -Size 'l' -NoDriver)) }
}
Test-SzType @{
  N = 'another project''s pending l, and its live ready lease, are ignored'; Seed = 'ready'; Want = 'm9g.large'
  Leases = { @((New-SzLease 1 -Size 'xl' -Project 'other'), (New-SzLease 2 -State 'ready' -Size 'm' -Project 'other'), (New-SzLease 3 -Size 's')) }
}
Test-SzType @{
  N = 'a live ready lease beside a pending l gives no type, and the deploy still records the type it reads back'; Seed = 'ready'; Want = ''; Locks = $true
  Leases = { @((New-SzLease 1 -State 'ready' -Size 'm'), (New-SzLease 2 -Size 'l')) }
}
Test-SzType @{
  N = 'an expired ready record beside a pending l does not count, so the type is given'; Seed = 'ready'; Want = 'm9g.2xlarge'
  Leases = { @((New-SzLease 1 -State 'ready' -Size 'm' -ExpiresInMinutes -60), (New-SzLease 2 -Size 'l')) }
}
Test-SzType @{
  N = 'released and unsatisfiable records are ignored'; Seed = 'ready'; Want = ''
  Leases = { @((New-SzLease 1 -State 'released' -Size 'l'), (New-SzLease 2 -State 'unsatisfiable' -Size 'xl')) }
}
Test-SzType @{
  N = 'no lease state file gives no type and the run goes on'; Seed = 'ready'; Want = ''; Locks = $true
  Leases = $null
  Extra = { param($h, $dir, $p) if (Test-Path -LiteralPath (Join-Path $dir 'leases.json')) { $p.Add('The run made a lease state file') } }
}
Test-SzType @{
  N = 'AGENTSPACE_STATE_DIR moves the file read and the lock taken'; Seed = 'ready'; Want = 'm9g.4xlarge'; StateSub = 'moved-state'
  Leases = { @((New-SzLease 1 -Size 'xl')) }
  Extra = {
    param($h, $dir, $p)
    if (-not (Test-Path -LiteralPath (Join-Path $dir 'leases.lock') -PathType Leaf)) { $p.Add('No leases.lock in the folder AGENTSPACE_STATE_DIR names: the lock was not taken there') }
    if (Test-Path -LiteralPath (Join-Path $h.StateDir 'leases.lock')) { $p.Add('A leases.lock was made in the default state folder') }
  }
}
Test-SzType @{
  N = 'a host that is being made takes its type too, last of its creation parameters'; Seed = 'none'; Kind = 'new'; Want = 'm9g.4xlarge'
  Leases = { @((New-SzLease 1 -Size 'xl')) }
}
Test-SzType @{
  N = '-Host swimcoach finds swimcoach''s leases, and the box''s deploy passes its type last'; Seed = 'none'; Kind = 'box'; Want = 'm9g.2xlarge'; Locks = $true
  Leases = { @((New-SzLease 1 -Size 'xl' -Project 'agentspace'), (New-SzLease 2 -Size 'l' -Project 'swimcoach'), (New-SzLease 3 -Size 's' -Project 'swimcoach')) }
}
Test-SzType @{
  N = '-Host swimcoach with no pending lease of its own passes no type'; Seed = 'none'; Kind = 'box'; Want = ''
  Leases = { @((New-SzLease 1 -Size 'xl' -Project 'agentspace')) }
}

# ---- A failed cloudformation deploy, with a type found and with none, and the log's failed line -------------------
# The record's instance_type was removed by step 4.2, and a deploy that fails leaves it so (operator.md, provision,
# When step 4, 5, 6 or 7 fails; pin P30). The leases stay pending, the exit is 1, and the host stays ready.
foreach ($fc in @(@{ N = 'with a type found'; Want = 'm9g.2xlarge'; Leases = { @((New-SzLease 1 -Size 'l')) } }, @{ N = 'with no type found'; Want = ''; Leases = $null })) {
  Invoke-HpInHome -Name "failed: a failed cloudformation deploy, $($fc.N)" -Body {
    param($h, $s)
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @(New-SzSeedRecord 'ready'))
    if ($fc.Leases) { Set-SzLeasesAt $h $h.StateDir (& $fc.Leases) }
    $leasesBefore = Get-HpFileBytes (Join-Path $h.StateDir 'leases.json')
    Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-SzAws 'exists' 'm9g.large' @{ 'cloudformation deploy' = @((New-HpStep '' '' 255)) }); rules = @(); locks = (Get-SzLocks $h $h.StateDir) }
    $t0 = [DateTime]::UtcNow
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $t1 = [DateTime]::UtcNow
    $calls = @(Get-HpCalls $s)
    $log = Read-HpLog $h 'agentspace'
    $lines = Get-HpLines $r.Stderr

    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($lines.Count -eq 0 -or $lines[-1] -cne "agentspace: $deployFailed") { $p.Add('The last line of stderr is not the deploy''s failure message') }
    if ((Get-HpSequence $calls) -join ',' -cne 'aws cloudformation describe-stacks,aws cloudformation deploy') { $p.Add('The calls are not the existence check and the deploy') }
    foreach ($m in (Get-SzDeployProblems $calls 'exists' $fc.Want $h)) { $p.Add($m) }
    Write-HpProblems "failed: a failed cloudformation deploy, $($fc.N): exit 1 with the deploy's message last, and the deploy was passed $(if ($fc.Want) { "InstanceType=$($fc.Want)" } else { 'no InstanceType' })" $p

    $p = New-HpProblems
    if ((Get-HpLogShape $log) -cne 'run begun,deploy failed') { $p.Add('The log''s lines are not: run begun, deploy failed') }
    else {
      $wantRest = $(if ($fc.Want) { " type=$($fc.Want)" } else { '' }) + " reason=$deployFailed"
      if ($log.Lines[1].Rest -cne $wantRest) { $p.Add($(if ($fc.Want) { 'The line is not deploy failed type=<type> reason=<message>, with the field in its place' } else { 'The line is not deploy failed reason=<message>, with no type field' })) }
      if (-not (Test-HpTimeBetween $log.Lines[1].Time $t0 $t1)) { $p.Add('The failed line''s time is not the time it was written') }
    }
    Write-HpProblems "log: deploy failed $(if ($fc.Want) { 'type=<type> ' })reason=<message> ($($fc.N))" $p

    $p = New-HpProblems
    foreach ($m in (Get-SzRecordProblems $h 'agentspace' 'ready' $script:HpHostId $null)) { $p.Add($m) }
    if (-not (Test-HpSameBytes (Get-HpFileBytes (Join-Path $h.StateDir 'leases.json')) $leasesBefore)) { $p.Add('The lease state file changed: the leases were not left pending') }
    if (-not (Test-HpFreeLock (Get-HpLockFile $h 'agentspace'))) { $p.Add('provision.lock is still held after the run ended') }
    Write-HpProblems "failed: a failed cloudformation deploy, $($fc.N): the record is still ready, without instance_type, and the leases are pending" $p
  }
}

# ---- Step 9's lines (pin P37) ---------------------------------------------------------------------------------
# $C: N, Seed (ready), Leases (a scriptblock), Foreign (the numbers of leases whose alias is a foreign file, so that
# their placement fails), ReadBack (the type the stand-in's describe-stacks answers), Want (the type the run passes,
# or empty), Expect (the place lines in order: `done:<placed>`, then `failed:<n>`, `refused:<n>` or `left:<n>`),
# Placed (the leases placed), Refused (answered unsatisfiable with no_capacity), Same (left as seeded, including the
# leases named by no line) and Silent (the leases that MUST NOT be named by any line). A `Full` case adds the results of a run that
# ran every step.
function Test-SzPlace($C) {
  Invoke-HpInHome -Name "place: $($C.N)" -Body {
    param($h, $s)
    $stateDir = $h.StateDir
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @(New-SzSeedRecord 'ready'))
    $seeds = @(& $C.Leases)
    Set-SzLeasesAt $h $stateDir $seeds
    $byId = @{}; foreach ($x in $seeds) { $byId[$x['id']] = $x }
    foreach ($n in @($C.Foreign)) { Write-HpFile -H $h -Path (Get-HpAliasFile $h (Get-SzAlias $n)) -Text "# a person's own file`nHost mine`n" }
    Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-SzAws 'exists' $C.ReadBack); rules = @((Get-HpBootstrapRule)); locks = (Get-SzLocks $h $stateDir) }
    $t0 = [DateTime]::UtcNow
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $t1 = [DateTime]::UtcNow
    $calls = @(Get-HpCalls $s)
    $log = Read-HpLog $h 'agentspace'
    $lines = Get-HpLines $r.Stderr

    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $first = $C.Expect[0]
    $placed = [int]($first -replace '^done:', '')
    $outcomes = @($C.Expect | Select-Object -Skip 1 | ForEach-Object { ($_ -split ':')[0] })
    $wantShape = $startSteps + ",place done placed=$placed" + (@($outcomes | ForEach-Object { ",place $_" }) -join '') + ',run done'
    if ((Get-HpLogShape $log) -cne $wantShape) { $p.Add("The log's lines are not: $wantShape") }
    else {
      $placeLines = @($log.Lines | Where-Object { $_.Step -eq 'place' })
      for ($i = 1; $i -lt $C.Expect.Count; $i++) {
        $kind, $n = $C.Expect[$i] -split ':'
        $id = Get-SzId ([int]$n)
        $got = $placeLines[$i]
        if ($kind -eq 'failed') { if ($got.Rest -cnotmatch "^ lease=$id reason=\S.*`$") { $p.Add("Line $($i + 1) of step 9 is not place failed lease=<id> reason=<message> for lease $n") } }
        elseif ($got.Rest -cne " lease=$id") { $p.Add("Line $($i + 1) of step 9 is not place $kind lease=<id>, with no other field, for lease $n") }
      }
      foreach ($n in @($C.Silent)) { if (@($log.Lines | Where-Object { $_.Raw -like "*$(Get-SzId $n)*" }).Count -gt 0) { $p.Add("Lease $n is named by a log line, though it MUST NOT be named by any line") } }
      $deployRest = if ($C.Want) { " type=$($C.Want)" } else { '' }
      if ($log.Lines[1].Rest -cne $deployRest) { $p.Add('The deploy line does not carry the type the run passed, or carries one it did not') }
    }
    if ($lines | Where-Object { $_ -like '*could not be placed*' }) { $p.Add('A warning was printed though the call succeeded: step 9 logs a lease it did not place and does not warn') }
    foreach ($m in (Get-SzDeployProblems $calls 'exists' $C.Want $h)) { $p.Add($m) }
    Write-HpProblems "place: $($C.N): place done placed=$placed first, then $(if ($outcomes.Count -eq 0) { 'no other line' } else { ($outcomes -join ', ') + ' lines in the order the call tried the leases, refused and left with no reason field' }), exit 0" $p

    $p = New-HpProblems
    foreach ($n in @($C.Placed)) {
      $rec = Get-SzLeaseRecord $h $stateDir (Get-SzId $n)
      if ($null -eq $rec -or $rec['state'] -cne 'ready' -or -not $rec.ContainsKey('expires_at') -or -not $rec.ContainsKey('ssh')) { $p.Add("Lease $n is not placed ready") }
    }
    foreach ($n in @($C.Refused)) {
      $rec = Get-SzLeaseRecord $h $stateDir (Get-SzId $n)
      if ($null -eq $rec -or $rec['state'] -cne 'unsatisfiable') { $p.Add("Lease $n is not unsatisfiable"); continue }
      $rs = @($rec['reasons'])
      if ($rs.Count -ne 1 -or $rs[0]['code'] -cne 'no_capacity') { $p.Add("Lease $n does not hold exactly the one reason no_capacity") }
      if ($rec.ContainsKey('expires_at') -or $rec.ContainsKey('ssh') -or $rec.ContainsKey('released_at')) { $p.Add("Lease $n gained a member an unsatisfiable record does not hold") }
      foreach ($k in 'id', 'project', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at', 'driver') { if ((Format-SzCanon $rec[$k]) -cne (Format-SzCanon $byId[(Get-SzId $n)][$k])) { $p.Add("Lease $n's $k changed"); break } }
    }
    foreach ($n in @($C.Same)) {
      $rec = Get-SzLeaseRecord $h $stateDir (Get-SzId $n)
      if ($null -eq $rec -or (Format-SzCanon $rec) -cne (Format-SzCanon $byId[(Get-SzId $n)])) { $p.Add("Lease $n's record changed: it MUST be left as it was") }
    }
    $rec = Get-HpRecord $h 'agentspace'
    if ($null -eq $rec -or $rec['state'] -cne 'ready') { $p.Add('The host is not left ready') }
    elseif ($rec['instance_type'] -cne $C.ReadBack) { $p.Add('The record does not hold the type the stack''s read-back gave') }
    Write-HpProblems "place: $($C.N): the leases it placed are ready, the ones it refused unsatisfiable with the one reason no_capacity, the rest left as they were" $p

    if ($C.Full) {
      $p = New-HpProblems
      $wantSeq = @('aws cloudformation describe-stacks', 'aws cloudformation deploy', 'aws cloudformation describe-stacks') + @(1..4 | ForEach-Object { $dsc }) + $syncCalls + @($dsc, $dsc, 'ssh:bootstrap')
      if (((Get-HpSequence $calls) -join ',') -cne ($wantSeq -join ',')) { $p.Add("The calls are not every step of the run: the deploy, the start, sync, the start again and bootstrap ($($calls.Count) calls)") }
      foreach ($call in $calls) {
        $sn = Get-HpSnapshotText $s $call['n']
        $rc = if ($sn) { Get-HpRecordFromText $sn 'agentspace' } else { $null }
        if ($sn -and ($null -eq $rc -or $rc['state'] -cne 'ready')) { $p.Add("The record is not ready at call $($call['n']): a run never takes a ready host out of ready, a resize included"); break }
      }
      $dc = @($calls | Where-Object { (Get-HpCallName $_) -eq 'aws cloudformation deploy' }) | Select-Object -First 1
      if ($dc) { $rd = Get-HpRecordFromText (Get-HpSnapshotText $s $dc['n']) 'agentspace'; if ($null -eq $rd -or $rd.ContainsKey('instance_type')) { $p.Add('The record held a type while the stack''s type was changing') } }
      $lastLine = if ($lines.Count -gt 0) { $lines[-1] } else { '' }
      if ($lastLine -like 'Sign in on this host*') { $p.Add('The sign-in line was printed for a host that was already ready') }
      Write-HpProblems "resize: a resize on a ready host runs every step (deploy, start, sync, bootstrap, record, place), its record ready at every call, with no type while the stack's changes" $p
    }
  }
}

Test-SzPlace @{
  N = 'a lease that fails, one placed, two refused beside a live ready lease, and one outside SIZES named by no line'
  ReadBack = 'm9g.xlarge'; Want = ''; Foreign = @(2)
  Leases = { @((New-SzLease 1 -State 'ready' -Size 'm'), (New-SzLease 2 -Size 'm'), (New-SzLease 3 -Size 'm'), (New-SzLease 4 -Size 'l'), (New-SzLease 5 -Size 'xl'), (New-SzLease 6 -Size 'xxl')) }
  Expect = @('done:1', 'failed:2', 'refused:4', 'refused:5'); Placed = @(3); Refused = @(4, 5); Same = @(1, 2, 6); Silent = @(1, 3, 6)
}
Test-SzPlace @{
  N = 'leases the host does not meet, with no live ready lease, are left pending, and one outside SIZES is named by no line'
  ReadBack = 'm9g.xlarge'; Want = 'm9g.4xlarge'; Foreign = @()
  Leases = { @((New-SzLease 1 -Size 'l'), (New-SzLease 2 -Size 'xl'), (New-SzLease 3 -Size 'xxl')) }
  Expect = @('done:0', 'left:1', 'left:2'); Placed = @(); Refused = @(); Same = @(1, 2, 3); Silent = @(3)
}
Test-SzPlace @{
  N = 'a lease that fails and one the host does not meet, with no lease placed: failed, then left'
  ReadBack = 'm9g.xlarge'; Want = 'm9g.2xlarge'; Foreign = @(1)
  Leases = { @((New-SzLease 1 -Size 'm'), (New-SzLease 2 -Size 'l')) }
  Expect = @('done:0', 'failed:1', 'left:2'); Placed = @(); Refused = @(); Same = @(1, 2); Silent = @()
}
Test-SzPlace @{
  N = 'a resize on a ready host places the lease the larger type meets'
  ReadBack = 'm9g.2xlarge'; Want = 'm9g.2xlarge'; Foreign = @(); Full = $true
  Leases = { @((New-SzLease 1 -Size 'l')) }
  Expect = @('done:1'); Placed = @(1); Refused = @(); Same = @(); Silent = @()
}

# ---- A call that fails as a whole ---------------------------------------------------------------------------------
Invoke-HpInHome -Name 'place: a place_pending that fails as a whole' -Body {
  param($h, $s)
  Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @(New-SzSeedRecord 'ready'))
  Set-SzLeasesAt $h $h.StateDir @((New-SzLease 1 -Size 'm'), (New-SzLease 2 -Size 'xl'))
  $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
  Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-SzAws 'exists' 'm9g.4xlarge'); rules = @(@{ kind = 'ssh'; class = 'bootstrap'; out = "bootstrap-out`n"; hook = 'boot' }) }
  $job = Start-HpOperatorJob -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  $reached = Wait-HpHook -S $s -Name 'boot' -Job $job
  $held = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  try {
    if ($reached) {
      $held = Hold-HpLock -H $h -Path (Join-Path $h.StateDir 'leases.lock')
      Release-HpHook -H $h -S $s -Name 'boot'
    }
    $r = Wait-HpOperatorJob $job
  } finally { if ($held) { $held.Dispose() } }
  $clock.Stop()
  $log = Read-HpLog $h 'agentspace'
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  if (-not $reached) { $p.Add('The run never reached bootstrap') }
  Add-HpRunProblems $p $r 0
  if ((Get-HpLogShape $log) -cne "$startSteps,place failed,run done") { $p.Add("The log's lines are not: $startSteps, one place failed line, run done: no refused, left or per-lease line") }
  else {
    $pf = $log.Lines | Where-Object { $_.Step -eq 'place' }
    if ($pf.Rest -notmatch '^ reason=\S.*$' -or $pf.Rest -match 'lease=') { $p.Add('The line is not place failed reason=<message> with no lease= field') }
    else {
      $reason = $pf.Rest.Substring(' reason='.Length)
      $wantReason = if ($reason.EndsWith('.')) { $reason.Substring(0, $reason.Length - 1) } else { $reason }
      $m = if ($lines.Count -gt 0) { [regex]::Match($lines[-1], '^agentspace: The pending leases of ''agentspace'' could not be placed: (.+); the next run places them\.$') } else { $null }
      if ($null -eq $m -or -not $m.Success) { $p.Add('The last line of stderr is not the warning') }
      elseif ($m.Groups[1].Value -cne $wantReason) { $p.Add('The warning''s reason is not the failure message without its prefix and its last period') }
    }
  }
  if ($clock.Elapsed.TotalSeconds -lt 25) { $p.Add('It gave up before the lease state lock''s 30 seconds') }
  $rec = Get-HpRecord $h 'agentspace'
  if ($null -eq $rec -or $rec['state'] -cne 'ready' -or $rec['instance_type'] -cne 'm9g.4xlarge') { $p.Add('The host is not left ready with the type the deploy recorded') }
  if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpLeasesFile $h)) $leasesBefore)) { $p.Add('The lease state file changed: the leases were not left pending') }
  Write-HpProblems 'place: a call that fails as a whole gives its one place failed reason=<message>, with the warning, the host left ready, the leases pending and exit 0' $p
}

Exit-ConformanceRun

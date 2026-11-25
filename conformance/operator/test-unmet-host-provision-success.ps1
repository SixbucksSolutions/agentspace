#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of what v0.10 adds to the operator's `provision`: the record of a run's success, D77's
# `last_success_at` (docs/operator.md, "provision", steps 3, 8 and 9, "Its log", "Which checks show it", the
# v0.10 table; docs/state.md, "The host registry", "The last success", "The order of the locks"; docs/driver.md,
# "Lock order"), run through the test seam. Card #0222 (E0.10-T04-S02).
#
# EXPECTED TO FAIL AGAINST v0.09's OPERATOR, which refuses a hosts.json that holds `last_success_at` as not valid
# and never writes one. A v0.10 build MUST pass every result line.
#
# What it does: each case makes its own throwaway home with the stand-ins of
# conformance/operator/host-provision-common.ps1, seeds a host registry and a lease state file, and runs
# `provision -Host <project>`. Every result's name starts with `operator.md#provision:`, then the row of "Which
# checks show it" it shows. The seeded record of most cases holds a `last_success_at`, so a case that finds none
# afterwards shows step 3's removal too, and not only the absence of a write.
#
# Rows of the v0.10 table, and the results that show them:
#   1. Step 3 removing `last_success_at` (the `step 3` results): a record seeded with one, `creating`, `failed` or
#      `ready`, and the box's, is read at the stand-in's first call, which every case makes fail at once: the file
#      MUST be the seed with `last_attempt_at` the run's start, `state` as step 3 sets it, `last_success_at`
#      removed and every other member and record as it was, byte for byte; a new record is added with
#      `last_attempt_at` and no `last_success_at`. The seeds hold no `instance_type`, so step 4.2's removal
#      writes nothing and the file at the first call is step 3's write alone. That the removal is in the SAME write
#      cannot be seen from outside without a point between two writes, and the byte comparison at the first call is
#      the nearest that a check can come: no read of the file sees `last_attempt_at` new and `last_success_at` old.
#   2. The write after a successful run (the `success` results): a run that placed every pending lease, and one
#      that placed one and answered another `unsatisfiable` (`place refused`, not a failure). Gated runs hold the
#      lease state lock from the bootstrap's hook until the log shows `record done`, so that the run waits to place
#      its leases; the check then takes `hosts.lock` and lets the placing go on, so that the success write waits for
#      the lock after `place done` is logged. The file MUST lack `last_success_at` then, and hold it only after the
#      check lets the lock go, at or after `last_attempt_at` and at or after the moment `place done` was logged, with
#      `run done` after it in the log and no other member changed (the file is the one at `place done` with the
#      member inserted, byte for byte).
#   3. None after a run that did not succeed (the `not succeeded` results): placing failed as a whole (the lease
#      state lock held past its 30 seconds), a lease failed (a foreign file at its alias) and a lease left (the host
#      does not meet it, no live `ready` lease holds the host), each exit 0 with the log's own line and no
#      `last_success_at`.
#   4. None for a run that failed at step 4, 5, 6 or 7 (the `failed step` results), and at step 8's write (the
#      `step 8` results), for a host that was `ready` and one that was not.
#   5. A failed write of `last_success_at` (the `success write fails` results): `hosts.lock` held past its 30
#      seconds, `hosts.json` read-only, `hosts.json` not valid, and the record gone, each after `record done` and
#      before the placing is let go. `success failed reason=<why>` MUST come after step 9's lines and before `run
#      done`, the file MUST be as it was, the exit code 0, and no stderr line names the failure.
#   6. The lock the write takes (the `success write lock` result): while the run waits for a `hosts.lock` the check
#      holds, `provision.lock` MUST be held and the lease state lock free, and neither `success failed` nor `run
#      done` yet in the log; the message names `hosts.lock`'s own path.
#
# SAFETY. See test-unmet-host-provision.ps1: the harness's isolation, the AWS gate, stand-ins for `aws`, `ssh`,
# `scp` and `go`, made-up instance IDs. A lock a case holds is held by this process, in the throwaway home, and
# released in a finally. A failure reason never quotes a path or the program's output. Every write and delete a
# case makes goes through Assert-ConformancePath (Write-HpFile, Remove-HpPath, Hold-HpLock and the attribute calls
# here). A case starts one operator run and the stand-ins it starts, and touches no other process.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-provision-success.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#provision'
$oldAttempt = '2026-01-01T00:00:00Z'
$oldSuccess = '2026-01-01T00:05:00Z'
$id1 = '11111111-1111-4111-8111-111111111111'
$id2 = '22222222-2222-4222-8222-222222222222'
$id3 = '33333333-3333-4333-8333-333333333333'
$startSteps = 'run begun,deploy done,start done,sync done,bootstrap done,record done'
$hookRule = @{ kind = 'ssh'; class = 'bootstrap'; out = "bootstrap-out`n"; hook = 'boot' }

# ---- Seeds and expectations ------------------------------------------------------------------------------
# One record as the registry's writer writes it: the members in the table's order (docs/state.md).
function New-SRec {
  param([string]$Project, [string]$State, [string]$Id = '', [string]$Type = '', [string]$Attempt = '', [string]$Success = '')
  $st = if ($Project -ceq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" }
  '{"project":"' + $Project + '","stack":"' + $st + '","state":"' + $State + '"' +
  $(if ($Id) { ',"instance_id":"' + $Id + '"' } else { '' }) +
  $(if ($Type) { ',"instance_type":"' + $Type + '"' } else { '' }) +
  $(if ($Attempt) { ',"last_attempt_at":"' + $Attempt + '"' } else { '' }) +
  $(if ($Success) { ',"last_success_at":"' + $Success + '"' } else { '' }) + '}'
}
function New-SDoc([string[]]$Records) { '{"version":0,"hosts":[' + ($Records -join ',') + ']}' + "`n" }
function Get-SBox { New-SRec 'swimcoach' 'ready' $script:HpBoxId 'm9g.xlarge' }
function Get-SBytes([string]$Text) { [Text.UTF8Encoding]::new($false).GetBytes($Text) }
function Get-SHostsText($H) { [IO.File]::ReadAllText((Get-HpHostsFile $H)) }
function Get-SHostsLockPath($H) { Join-Path $H.UserProfile '.agentspace\hosts.lock' }
function Format-STime([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture) }
function Get-SAlias([string]$Id) { 'agentspace-agentspace-' + $Id.Substring(0, 8) }

# A pending lease of the project, of the size given.
function New-SLease([string]$Id, [string]$Size = 'm') { $r = New-HpLeaseRecord -Id $Id; $r['size'] = $Size; $r }
# A live `ready` lease of the project, which holds the host.
function New-SReadyLease([string]$Id) {
  $r = New-HpLeaseRecord -Id $Id -State 'ready'
  $r['expires_at'] = Format-STime ([DateTime]::UtcNow.AddMinutes(60))
  $r['ssh'] = [ordered]@{ host_alias = (Get-SAlias $Id); user = 'ubuntu' }
  $r
}
# The home of a run: the box's record and $Records, the leases, the stand-ins' answers and rules.
function Initialize-SHome {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)]$S, [string[]]$Records = @(), $Leases = @(), [Parameter(Mandatory)][hashtable]$Aws, $Rules = @(), [string]$Project = 'agentspace', [switch]$NoBox)
  $all = if ($NoBox) { @($Records) } else { @((Get-SBox)) + @($Records) }
  Initialize-HpHome -H $H -HostsText (New-SDoc $all)
  if (@($Leases).Count -gt 0) { Set-HpLeases $H @($Leases) }
  Set-HpScenario -H $H -S $S -LockProject $Project -Scenario @{ aws = $Aws; rules = @($Rules) }
}
# The run's record, as parsed, and the time windows it MUST lie in.
function Get-SRec($H, [string]$Project = 'agentspace') { Get-HpRecordFromText (Get-SHostsText $H) $Project }

# ---- Reading the log while a run is under way --------------------------------------------------------------
# The log, parsed as Read-HpLog parses it, read while the run still has it open for append: a read that shares the file
# with a writer (Read-HpLog's File.ReadAllBytes does not, so it fails until the run ends, and is for a finished run).
function Read-SLogNow($H) {
  $lines = @()
  try {
    $f = Get-HpLogFile $H 'agentspace'
    if (Test-Path -LiteralPath $f -PathType Leaf) {
      $fs = [IO.File]::Open($f, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
      try { $ms = [IO.MemoryStream]::new(); $fs.CopyTo($ms); $bytes = $ms.ToArray() } finally { $fs.Dispose() }
      $text = [Text.UTF8Encoding]::new($false).GetString($bytes)
      $lines = @($text -split "`n" | Where-Object { $_ -ne '' } | ForEach-Object {
          $m = [regex]::Match($_, '^(?<t>\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) (?<step>[a-z]+) (?<out>[a-z]+)(?<rest>.*)$')
          [pscustomobject]@{ Raw = $_; Ok = $m.Success; Time = $m.Groups['t'].Value; Step = $m.Groups['step'].Value; Outcome = $m.Groups['out'].Value; Rest = $m.Groups['rest'].Value }
        })
    }
  } catch { }
  [pscustomobject]@{ Bytes = $null; Lines = $lines }
}
# True when $Pred (given the parsed log) holds within $Seconds, or at the run's end when the run ends first.
function Wait-SLog($H, $Job, [scriptblock]$Pred, [int]$Seconds) {
  $until = [DateTime]::UtcNow.AddSeconds($Seconds)
  while ([DateTime]::UtcNow -lt $until) {
    if (& $Pred (Read-SLogNow $H)) { return $true }
    if ($Job.State -in 'Completed', 'Failed', 'Stopped') { return [bool](& $Pred (Read-SLogNow $H)) }
    Start-Sleep -Milliseconds 100
  }
  $false
}
$script:SRecordDone = { param($l) @($l.Lines | Where-Object { $_.Step -ceq 'record' -and $_.Outcome -ceq 'done' }).Count -gt 0 }
$script:SPlaceDone = { param($l) @($l.Lines | Where-Object { $_.Step -ceq 'place' -and $_.Outcome -ceq 'done' }).Count -gt 0 }
# Holds a lock file as the operator does, trying again for a while: the run that has just let it go MAY be a moment from doing so.
function Hold-SLock($H, [string]$Path, [int]$Seconds = 20) {
  $until = [DateTime]::UtcNow.AddSeconds($Seconds)
  while ($true) {
    try { return (Hold-HpLock -H $H -Path $Path) } catch { if ([DateTime]::UtcNow -ge $until) { throw }; Start-Sleep -Milliseconds 50 }
  }
}

# A run that waits to place its leases. The run reaches the bootstrap's ssh, which stops at a hook; the check then
# holds the lease state lock, so that the run reaches step 9 and waits at its first lease; waits for the log's `record
# done`, and 3 seconds more, so that the `place_pending` call has read its host's record (it does so before it takes
# the lease state lock, driver.md, place_pending, step 1) and is waiting for the lease state lock; runs $Gate, which
# changes what the success write will find, and lets the lease state lock go. That read holds `hosts.lock` for the read
# alone, so a read that comes after the 3 seconds, under load, meets the $Gate's change or its hold of `hosts.lock`: the
# call fails as a whole, or places nothing for a record gone, and the log's `place failed` or `placed=0` fails the case's
# log check, so a miss is never silent (a read-only file changes no read). Such a failure is rerun alone before it is a
# finding. It waits for the log's `place done`
# and runs $AfterPlace, while a write that waits for a lock the check holds is still waiting; then collects the run.
# $Ctx is the case's table: the blocks get it, and put what they find in it. $Cleanup runs last, in a finally.
function Invoke-SGatedRun {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)]$S, [Parameter(Mandatory)][hashtable]$Ctx, [scriptblock]$Gate, [scriptblock]$AfterPlace, [scriptblock]$Cleanup)
  $Ctx.H = $H; $Ctx.S = $S
  $Ctx.Reached = $false; $Ctx.RecordSeen = $false; $Ctx.PlaceSeen = $false; $Ctx.Result = $null
  $job = Start-HpOperatorJob -H $H -S $S -Arguments @('provision', '-Host', 'agentspace')
  $Ctx.Job = $job
  $leaseLock = $null
  try {
    $Ctx.Reached = Wait-HpHook -S $S -Name 'boot' -Job $job
    if ($Ctx.Reached) {
      $leaseLock = Hold-HpLock -H $H -Path (Join-Path $H.StateDir 'leases.lock')
      Release-HpHook -H $H -S $S -Name 'boot'
      $Ctx.RecordSeen = Wait-SLog $H $job $script:SRecordDone 120
      if ($Ctx.RecordSeen) {
        Start-Sleep -Seconds 3
        if ($Gate) { & $Gate $Ctx }
      }
      $leaseLock.Dispose(); $leaseLock = $null
      if ($Ctx.RecordSeen) {
        $Ctx.PlaceSeen = Wait-SLog $H $job $script:SPlaceDone 60
        if ($Ctx.PlaceSeen -and $AfterPlace) { & $AfterPlace $Ctx }
      }
    }
    $Ctx.Result = Wait-HpOperatorJob $job
  } finally {
    if ($leaseLock) { $leaseLock.Dispose() }
    if ($Cleanup) { & $Cleanup $Ctx }
    if ($job.State -eq 'Running') { Stop-Job $job; Remove-Job $job -Force }
  }
}
function Add-SGateProblems($Problems, $Ctx) {
  if (-not $Ctx.Reached) { $Problems.Add('The run never reached the bootstrap, so the check never got to step 9') }
  elseif (-not $Ctx.RecordSeen) { $Problems.Add('The log never showed record done') }
  elseif (-not $Ctx.PlaceSeen) { $Problems.Add('The log never showed place done, though the check let the placing go') }
}
function Remove-SHold($Ctx) {
  if ($Ctx.HostsLock) { $Ctx.HostsLock.Dispose(); $Ctx.HostsLock = $null }
  $f = Get-HpHostsFile $Ctx.H
  if (Test-Path -LiteralPath $f -PathType Leaf) { [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $Ctx.H -Path $f), [IO.FileAttributes]::Normal) }
}

# ---- 1. Step 3 --------------------------------------------------------------------------------------------
# $C: N, Project, Record (the seed's text, or none for a project with no record), State (what step 3 leaves),
# Id (the instance ID the record holds, or none). The run's first call to a stand-in fails, so that the run
# ends at its first step and the file at the call is step 3's write alone.
function Test-StepThree($C) {
  Invoke-HpInHome -Name "${sec}: step 3: $($C.N)" -Body {
    param($h, $s)
    $fail = New-HpStep '' 'A stand-in stopped the run here.' 255
    [string[]]$recs = @(if ($C.Record) { $C.Record })
    Initialize-SHome -H $h -S $s -Records $recs -Aws @{ 'cloudformation describe-stacks' = @($fail); 'cloudformation deploy' = @($fail) } -Project $C.Project -NoBox:($C.Project -ceq 'swimcoach')
    $t0 = [DateTime]::UtcNow
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', $C.Project)
    $t1 = [DateTime]::UtcNow
    $calls = @(Get-HpCalls $s)
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    $snap = if ($calls.Count -gt 0) { Get-HpSnapshotText $s $calls[0]['n'] } else { $null }
    if ($null -eq $snap) { $p.Add("The stand-in made no call, or the registry was not there at the first one ($($calls.Count) calls; the log's lines are: $(Get-HpLogShape (Read-HpLog $h $C.Project)))") }
    else {
      $rec = Get-HpRecordFromText $snap $C.Project
      if ($null -eq $rec) { $p.Add('The record is not in the registry at the first call') }
      else {
        if ($rec.ContainsKey('last_success_at')) { $p.Add('The record still holds last_success_at at the run''s first call: step 3 removes it') }
        if (-not $rec.ContainsKey('last_attempt_at') -or -not (Test-HpTimeBetween $rec['last_attempt_at'] $t0 $t1) -or $rec['last_attempt_at'] -ceq $oldAttempt) { $p.Add('last_attempt_at is not the run''s start') }
        else {
          $recs2 = @(New-SRec $C.Project $C.State $C.Id '' $rec['last_attempt_at'])
          $want = if ($C.Project -ceq 'swimcoach') { New-SDoc $recs2 } else { New-SDoc (@((Get-SBox)) + $recs2) }
          if ($snap -cne $want) { $p.Add("The registry at the first call is not the seed with last_attempt_at the run's start, state $($C.State), last_success_at removed and every other member and record as it was") }
        }
      }
    }
    Write-HpProblems "${sec}: step 3: $($C.N): the record has last_attempt_at set to the run's start and no last_success_at, every other member and record as it was, in one registry as step 3 leaves it (step 3; D77; pin P24)" $p
  }
}
Test-StepThree @{ N = 'a creating record that held last_success_at'; Project = 'agentspace'; Record = (New-SRec 'agentspace' 'creating' '' '' $oldAttempt $oldSuccess); State = 'creating'; Id = '' }
Test-StepThree @{ N = 'a failed record that held last_success_at'; Project = 'agentspace'; Record = (New-SRec 'agentspace' 'failed' $script:HpHostId '' $oldAttempt $oldSuccess); State = 'creating'; Id = $script:HpHostId }
Test-StepThree @{ N = 'a ready record that held last_success_at, which stays ready'; Project = 'agentspace'; Record = (New-SRec 'agentspace' 'ready' $script:HpHostId '' $oldAttempt $oldSuccess); State = 'ready'; Id = $script:HpHostId }
Test-StepThree @{ N = 'the box''s ready record that held last_success_at'; Project = 'swimcoach'; Record = (New-SRec 'swimcoach' 'ready' $script:HpBoxId '' $oldAttempt $oldSuccess); State = 'ready'; Id = $script:HpBoxId }
Test-StepThree @{ N = 'a new record is added with last_attempt_at and no last_success_at'; Project = 'agentspace'; Record = $null; State = 'creating'; Id = '' }

# ---- 2. The write after a successful run -------------------------------------------------------------------
# A gated run on a ready host with one pending lease: the write comes after `place done`, at the time the run
# ended, and changes nothing else.
Invoke-HpInHome -Name "${sec}: success: a gated run" -Body {
  param($h, $s)
  Initialize-SHome -H $h -S $s -Records @(New-SRec 'agentspace' 'ready' $script:HpHostId 'm9g.xlarge' $oldAttempt $oldSuccess) -Leases @((New-SLease $id1)) -Aws (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists) -Rules @($hookRule)
  $ctx = @{ HostsLock = $null }
  $t0 = [DateTime]::UtcNow
  try {
    Invoke-SGatedRun -H $h -S $s -Ctx $ctx -Gate {
      param($c)
      $c.B1 = Get-HpFileBytes (Get-HpHostsFile $c.H)
      $c.HostsLock = Hold-SLock $c.H (Get-SHostsLockPath $c.H)
    } -AfterPlace {
      param($c)
      $c.B2 = Get-HpFileBytes (Get-HpHostsFile $c.H)
      Start-Sleep -Milliseconds 2500
      $c.HostsLock.Dispose(); $c.HostsLock = $null
    } -Cleanup { param($c) Remove-SHold $c }
  } finally { Remove-SHold $ctx }
  $t1 = [DateTime]::UtcNow
  $r = $ctx.Result
  $log = Read-HpLog $h 'agentspace'
  $final = Get-SRec $h
  $finalBytes = Get-HpFileBytes (Get-HpHostsFile $h)
  $b1text = if ($ctx.B1) { [Text.UTF8Encoding]::new($false).GetString($ctx.B1) } else { '' }
  $b1rec = if ($b1text) { Get-HpRecordFromText $b1text 'agentspace' } else { $null }
  $ls = if ($final -and $final.ContainsKey('last_success_at')) { [string]$final['last_success_at'] } else { '' }
  $la = if ($b1rec -and $b1rec.ContainsKey('last_attempt_at')) { [string]$b1rec['last_attempt_at'] } else { '' }

  $p = New-HpProblems
  Add-SGateProblems $p $ctx
  if ($null -eq $r) { $p.Add('The run was not collected') } else { Add-HpRunProblems $p $r 0 }
  if ($ctx.B2 -and -not (Test-HpSameBytes $ctx.B2 $ctx.B1)) { $p.Add('The registry changed between record done and the moment place done was logged, with the write held') }
  if ($ctx.B2 -and ([Text.UTF8Encoding]::new($false).GetString($ctx.B2)) -match 'last_success_at') { $p.Add('The registry held last_success_at when place done was logged: the write came before place done, or step 3 did not remove the old one') }
  if ($ls -eq '') { $p.Add('The record holds no last_success_at after the run') }
  else {
    # The run's own clock on both sides, to the second: the success is written after place done is logged and before run done.
    $lsAt0 = ConvertFrom-HpTime $ls
    $pdLine = @($log.Lines | Where-Object { $_.Step -ceq 'place' -and $_.Outcome -ceq 'done' }) | Select-Object -First 1
    $rdLine = @($log.Lines | Where-Object { $_.Step -ceq 'run' -and $_.Outcome -ceq 'done' }) | Select-Object -First 1
    if ($null -eq $lsAt0 -or $null -eq $pdLine -or $null -eq $rdLine) { $p.Add('last_success_at, place done or run done is missing or not in the UTC form') }
    else {
      if ($lsAt0 -lt (ConvertFrom-HpTime $pdLine.Time)) { $p.Add('last_success_at is before the time of the place done line: the write is made after step 9''s call returns') }
      if ($lsAt0 -gt (ConvertFrom-HpTime $rdLine.Time)) { $p.Add('last_success_at is after the time of the run done line: the write is made before run done') }
    }
  }
  if ((Get-HpLogShape $log) -cne "$startSteps,place done placed=1,run done") { $p.Add("The log's lines are not: $startSteps, place done placed=1, run done, with no line that says the success was recorded (they are: $(Get-HpLogShape $log))") }
  Write-HpProblems "${sec}: success: a run that placed every pending lease writes last_success_at after place done is logged and before run done, at the time the run ended (step 9, then step 10; pins P22 to P24)" $p

  $p = New-HpProblems
  if ($ls -ne '' -and $la -ne '') {
    $lsAt = ConvertFrom-HpTime $ls; $laAt = ConvertFrom-HpTime $la
    if ($null -eq $lsAt -or $null -eq $laAt) { $p.Add('last_success_at or last_attempt_at is not in the UTC form') }
    elseif ($lsAt -lt $laAt) { $p.Add('last_success_at is before last_attempt_at') }
    elseif (($lsAt - $laAt).TotalSeconds -lt 2) { $p.Add('last_success_at is within 2 seconds of the run''s start, though the check held the run for more than 5 seconds: it is not the time the run ended') }
    if (-not (Test-HpTimeBetween $la $t0 $t1)) { $p.Add('last_attempt_at is not within the run') }
  } else { $p.Add('There is no last_success_at or no last_attempt_at to compare') }
  Write-HpProblems "${sec}: success: last_success_at is at or after last_attempt_at, compared as instants, and is the time the run ended, not its start (pin P24)" $p

  $p = New-HpProblems
  if ($b1text -match 'last_success_at') { $p.Add('The registry at the start of step 9 held a last_success_at: step 3 did not remove the seeded one') }
  if ($b1text -and $la -and $ls) {
    $want = $b1text.Replace('"last_attempt_at":"' + $la + '"}', '"last_attempt_at":"' + $la + '","last_success_at":"' + $ls + '"}')
    if ($want -ceq $b1text) { $p.Add('The check could not build the expected registry from the one before the write') }
    elseif (-not (Test-HpSameBytes (Get-SBytes $want) $finalBytes)) { $p.Add('The registry is not the one before the write with last_success_at added after last_attempt_at and nothing else changed, in the writer''s bytes') }
  } else { $p.Add('There is no registry before the write, or no last_success_at, to compare') }
  Write-HpProblems "${sec}: success: the write sets last_success_at alone: every other member and record is as it was, in the writer's bytes (pin P23)" $p
}

# Runs that are not gated: a host the run made, and a lease answered `unsatisfiable` beside a placed one.
# $C: N, Records, Leases, Exists, Shape (the log's lines), Rest (the log's lines after place done placed=<n>, as
# `<outcome> lease=<id>` entries).
function Test-PlainSuccess($C) {
  Invoke-HpInHome -Name "${sec}: success: $($C.N)" -Body {
    param($h, $s)
    $aws = if ($C.Exists) { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists } else { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId }
    Initialize-SHome -H $h -S $s -Records @($C.Records) -Leases (& $C.Leases) -Aws $aws -Rules @((Get-HpBootstrapRule))
    $t0 = [DateTime]::UtcNow
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $t1 = [DateTime]::UtcNow
    $log = Read-HpLog $h 'agentspace'
    $final = Get-SRec $h
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    if ((Get-HpLogShape $log) -cne $C.Shape) { $p.Add("The log's lines are not: $($C.Shape)") }
    $ls = if ($final -and $final.ContainsKey('last_success_at')) { [string]$final['last_success_at'] } else { '' }
    $la = if ($final -and $final.ContainsKey('last_attempt_at')) { [string]$final['last_attempt_at'] } else { '' }
    if ($ls -eq '' -or $la -eq '') { $p.Add('The record holds no last_success_at or no last_attempt_at') }
    else {
      if (-not (Test-HpTimeBetween $ls $t0 $t1) -or -not (Test-HpTimeBetween $la $t0 $t1)) { $p.Add('last_success_at or last_attempt_at is not a time within the run') }
      $lsAt = ConvertFrom-HpTime $ls; $laAt = ConvertFrom-HpTime $la
      if ($lsAt -and $laAt -and $lsAt -lt $laAt) { $p.Add('last_success_at is before last_attempt_at') }
      $want = New-SDoc @((Get-SBox), (New-SRec 'agentspace' 'ready' $script:HpHostId 'm9g.xlarge' $la $ls))
      if (-not (Test-HpSameBytes (Get-SBytes $want) (Get-HpFileBytes (Get-HpHostsFile $h)))) { $p.Add('The registry is not the box''s record as it was and the host''s: ready, its instance ID and type, last_attempt_at, then last_success_at, in the writer''s bytes') }
    }
    $i = 0
    foreach ($e in @($C.Rest)) {
      $line = @($log.Lines | Where-Object { $_.Step -ceq 'place' })[$i + 1]
      if ($null -eq $line -or $line.Outcome -cne $e.Outcome -or $line.Rest -cne " lease=$($e.Id)") { $p.Add("The place line $($i + 2) is not place $($e.Outcome) lease=<id>") }
      $i++
    }
    Write-HpProblems "${sec}: success: $($C.N) (step 9's end; pin P22)" $p
  }
}
Test-PlainSuccess @{
  N = 'a host the run made, its one pending lease placed, ends with last_success_at after last_attempt_at, ready, with every other member'
  Records = @(); Exists = $false; Leases = { @((New-SLease $id1)) }
  Shape = "$startSteps,place done placed=1,run done"; Rest = @()
}
Test-PlainSuccess @{
  N = 'a lease answered unsatisfiable beside a placed one (place refused) is not a failure'
  Records = @((New-SRec 'agentspace' 'ready' $script:HpHostId 'm9g.xlarge' $oldAttempt $oldSuccess)); Exists = $true
  Leases = { @((New-SReadyLease $id3), (New-SLease $id1), (New-SLease $id2 'l')) }
  Shape = "$startSteps,place done placed=1,place refused,run done"; Rest = @(@{ Outcome = 'refused'; Id = $id2 })
}

# ---- 3. A run that did not succeed -----------------------------------------------------------------------
# Each ends exit 0 with its own place line and no last_success_at, the seeded one removed by step 3.
function Test-NotSucceeded($C) {
  Invoke-HpInHome -Name "${sec}: not succeeded: $($C.N)" -Body {
    param($h, $s)
    Initialize-SHome -H $h -S $s -Records @((New-SRec 'agentspace' 'ready' $script:HpHostId 'm9g.xlarge' $oldAttempt $oldSuccess)) -Leases (& $C.Leases) -Aws (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists) -Rules @((Get-HpBootstrapRule))
    if ($C.Foreign) { Write-HpFile -H $h -Path (Get-HpAliasFile $h (Get-SAlias $C.Foreign)) -Text "# a person's own file`nHost mine`n" }
    $t0 = [DateTime]::UtcNow
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $t1 = [DateTime]::UtcNow
    $log = Read-HpLog $h 'agentspace'
    $final = Get-SRec $h
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    if ((Get-HpLogShape $log) -cne $C.Shape) { $p.Add("The log's lines are not: $($C.Shape)") }
    else {
      $line = @($log.Lines | Where-Object { $_.Step -ceq 'place' })[1]
      if ($C.Outcome -ceq 'failed') { if ($line.Rest -cnotmatch "^ lease=$($C.Lease) reason=\S") { $p.Add('The line is not place failed lease=<id> reason=<message>') } }
      elseif ($line.Rest -cne " lease=$($C.Lease)") { $p.Add("The line is not place $($C.Outcome) lease=<id> with no other field") }
    }
    if ($null -eq $final) { $p.Add('The record is gone') }
    else {
      if ($final.ContainsKey('last_success_at')) { $p.Add('The record holds last_success_at, though the run did not succeed') }
      if ($final['state'] -cne 'ready' -or -not (Test-HpTimeBetween $final['last_attempt_at'] $t0 $t1)) { $p.Add('The record is not ready with the last_attempt_at of this run') }
    }
    Write-HpProblems "${sec}: not succeeded: $($C.N): exit 0, its own place line and no last_success_at (step 9; pin P22)" $p
  }
}
Test-NotSucceeded @{
  N = 'a lease that failed (a foreign file at its alias) beside a placed one'
  Leases = { @((New-SLease $id1), (New-SLease $id2)) }; Foreign = $id2; Lease = $id2; Outcome = 'failed'
  Shape = "$startSteps,place done placed=1,place failed,run done"
}
Test-NotSucceeded @{
  N = 'a lease left for the next run (a size the host does not meet, no live ready lease)'
  Leases = { @((New-SLease $id1 'l')) }; Foreign = $null; Lease = $id1; Outcome = 'left'
  Shape = "$startSteps,place done placed=0,place left,run done"
}
Invoke-HpInHome -Name "${sec}: not succeeded: placing failed as a whole (the lease state lock held past its 30 seconds)" -Body {
  param($h, $s)
  Initialize-SHome -H $h -S $s -Records @((New-SRec 'agentspace' 'ready' $script:HpHostId 'm9g.xlarge' $oldAttempt $oldSuccess)) -Leases @((New-SLease $id1)) -Aws (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists) -Rules @($hookRule)
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
  $final = Get-SRec $h
  $p = New-HpProblems
  if (-not $reached) { $p.Add('The run never reached bootstrap') }
  Add-HpRunProblems $p $r 0
  if ((Get-HpLogShape $log) -cne "$startSteps,place failed,run done") { $p.Add("The log's lines are not: $startSteps, one place failed line, run done") }
  else {
    $pf = @($log.Lines | Where-Object { $_.Step -ceq 'place' })[0]
    if ($pf.Rest -cnotmatch '^ reason=\S' -or $pf.Rest -match 'lease=') { $p.Add('The line is not place failed reason=<message> with no lease= field') }
  }
  if ($clock.Elapsed.TotalSeconds -lt 25) { $p.Add('It gave up before the lease state lock''s 30 seconds') }
  if ($null -eq $final -or $final['state'] -cne 'ready') { $p.Add('The host is not left ready') }
  elseif ($final.ContainsKey('last_success_at')) { $p.Add('The record holds last_success_at, though the placing failed as a whole') }
  Write-HpProblems "${sec}: not succeeded: placing failed as a whole: exit 0, its one place failed reason=<message> line and no last_success_at (step 9; pin P22)" $p
}

# ---- 4. A run that failed at step 4, 5, 6 or 7, or at step 8's write ----------------------------------------
# $C: N, Seed (ready or failed), Aws (overrides), Rules, Shape, Final (the state the record ends in).
function Test-FailedStepNoSuccess($C) {
  Invoke-HpInHome -Name "${sec}: failed step: $($C.N)" -Body {
    param($h, $s)
    $ready = $C.Seed -ceq 'ready'
    $aws = if ($ready) { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists } else { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId }
    foreach ($k in $C.Aws.Keys) { $aws[$k] = $C.Aws[$k] }
    Initialize-SHome -H $h -S $s -Records @((New-SRec 'agentspace' $C.Seed $script:HpHostId $(if ($ready) { 'm9g.xlarge' } else { '' }) $oldAttempt $oldSuccess)) -Leases @((New-SLease $id1)) -Aws $aws -Rules @($C.Rules)
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $log = Read-HpLog $h 'agentspace'
    $final = Get-SRec $h
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ((Get-HpLogShape $log) -cne $C.Shape) { $p.Add("The log's lines are not: $($C.Shape)") }
    if ($null -eq $final) { $p.Add('The record is gone') }
    else {
      if ($final.ContainsKey('last_success_at')) { $p.Add('The record holds last_success_at, though the run failed') }
      if ($final['state'] -cne $C.Final) { $p.Add("The record is '$($final['state'])', not $($C.Final)") }
    }
    Write-HpProblems "${sec}: failed step: $($C.N): exit 1 and no last_success_at, the seeded one removed by step 3 (When step 4, 5, 6 or 7 fails)" $p
  }
}
$deployFail = @{ 'cloudformation deploy' = @((New-HpStep '' '' 255)) }
$startFail = @{ 'ec2 describe-instances' = @((New-HpStep '' 'An error occurred (UnauthorizedOperation) when calling the DescribeInstances operation: You are not authorized to perform this operation.' 254)) }
$syncFail = @(@{ kind = 'scp'; nth = 1; code = 1; err = "scp wrote this`n" })
$bootFail = @(@{ kind = 'ssh'; class = 'bootstrap'; code = 7 })
Test-FailedStepNoSuccess @{ N = 'step 4, the deploy, of a ready host'; Seed = 'ready'; Aws = $deployFail; Rules = @(); Shape = 'run begun,deploy failed'; Final = 'ready' }
Test-FailedStepNoSuccess @{ N = 'step 4, the deploy, of a host that was not ready'; Seed = 'failed'; Aws = $deployFail; Rules = @(); Shape = 'run begun,deploy failed'; Final = 'failed' }
Test-FailedStepNoSuccess @{ N = 'step 5, the start, of a ready host'; Seed = 'ready'; Aws = $startFail; Rules = @(); Shape = 'run begun,deploy done,start failed'; Final = 'ready' }
Test-FailedStepNoSuccess @{ N = 'step 5, the start, of a host that was not ready'; Seed = 'failed'; Aws = $startFail; Rules = @(); Shape = 'run begun,deploy done,start failed'; Final = 'failed' }
Test-FailedStepNoSuccess @{ N = 'step 6, sync, of a ready host'; Seed = 'ready'; Aws = @{}; Rules = $syncFail; Shape = 'run begun,deploy done,start done,sync failed'; Final = 'ready' }
Test-FailedStepNoSuccess @{ N = 'step 7, bootstrap, of a host that was not ready'; Seed = 'failed'; Aws = @{}; Rules = $bootFail; Shape = 'run begun,deploy done,start done,sync done,bootstrap failed'; Final = 'failed' }

# Step 8's write fails after the bootstrap's ssh: hosts.json read-only for a host that was not ready (a ready host whose
# instance ID is as it was has nothing to write at step 8, so read-only cannot fail it), and hosts.lock held past its 30
# seconds for a ready host.
foreach ($c in @(@{ Seed = 'creating'; Final = 'creating'; How = 'readonly' }, @{ Seed = 'ready'; Final = 'ready'; How = 'lock' })) {
  $c2 = $c
  Invoke-HpInHome -Name "${sec}: step 8: $($c2.How -ceq 'lock' ? 'hosts.lock held past its 30 seconds' : 'a registry write that fails'), a $($c2.Seed) host, ends with no last_success_at" -Body {
    param($h, $s)
    $ready = $c2.Seed -ceq 'ready'
    $aws = if ($ready) { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists } else { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId }
    Initialize-SHome -H $h -S $s -Records @((New-SRec 'agentspace' $c2.Seed $(if ($ready) { $script:HpHostId } else { '' }) $(if ($ready) { 'm9g.xlarge' } else { '' }) $oldAttempt $oldSuccess)) -Leases @((New-SLease $id1)) -Aws $aws -Rules @($hookRule)
    $file = Get-HpHostsFile $h
    $job = Start-HpOperatorJob -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $reached = Wait-HpHook -S $s -Name 'boot' -Job $job
    $held = $null
    try {
      if ($reached) {
        if ($c2.How -ceq 'lock') { $held = Hold-HpLock -H $h -Path (Get-SHostsLockPath $h) }
        else { [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $h -Path $file), [IO.FileAttributes]::ReadOnly) }
        Release-HpHook -H $h -S $s -Name 'boot'
      }
      $r = Wait-HpOperatorJob $job
    } finally {
      if ($held) { $held.Dispose() }
      if (Test-Path -LiteralPath $file -PathType Leaf) { [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $h -Path $file), [IO.FileAttributes]::Normal) }
    }
    $log = Read-HpLog $h 'agentspace'
    $final = Get-SRec $h
    $p = New-HpProblems
    if (-not $reached) { $p.Add('The run never reached bootstrap, so step 8 was not reached') }
    Add-HpRunProblems $p $r 1
    if ((Get-HpLogShape $log) -cne 'run begun,deploy done,start done,sync done,bootstrap done,record failed') { $p.Add("The log's lines are not: run begun, deploy, start, sync and bootstrap done, then record failed (they are: $(Get-HpLogShape $log))") }
    if ($null -eq $final) { $p.Add('The record is gone') }
    else {
      if ($final.ContainsKey('last_success_at')) { $p.Add('The record holds last_success_at, though step 8 failed') }
      if ($final['state'] -cne $c2.Final) { $p.Add("The record is '$($final['state'])', not left $($c2.Final) as step 3 left it") }
    }
    Write-HpProblems "${sec}: step 8: $($c2.How -ceq 'lock' ? 'hosts.lock held past its 30 seconds' : 'a registry write that fails'), a $($c2.Seed) host, ends with no last_success_at: record failed, exit 1 (When step 3 or step 8 cannot change the registry)" $p
  }
}

# ---- 5 and 6. A failed write of last_success_at, and the lock it takes ----------------------------------------
# $C: N, Mode (lock, readonly, invalid or gone), Pattern (the reason's form, matched against the log line's rest) and
# PathOf (which file the message names: hosts.lock or hosts.json, or none).
function Test-SuccessWriteFails($C) {
  Invoke-HpInHome -Name "${sec}: success write fails: $($C.N)" -Body {
    param($h, $s)
    Initialize-SHome -H $h -S $s -Records @((New-SRec 'agentspace' 'ready' $script:HpHostId 'm9g.xlarge' $oldAttempt $oldSuccess)) -Leases @((New-SLease $id1)) -Aws (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists) -Rules @($hookRule)
    $ctx = @{ HostsLock = $null; Probes = @(); Mode = $C.Mode }
    try {
      Invoke-SGatedRun -H $h -S $s -Ctx $ctx -Gate {
        param($c)
        $f = Get-HpHostsFile $c.H
        $c.B1 = Get-HpFileBytes $f
        switch ($c.Mode) {
          'lock' { $c.HostsLock = Hold-SLock $c.H (Get-SHostsLockPath $c.H) }
          'readonly' { [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $c.H -Path $f), [IO.FileAttributes]::ReadOnly) }
          'invalid' { [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $c.H -Path $f), (Get-SBytes 'This is not json {')); $c.B1 = Get-HpFileBytes $f }
          'gone' { [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $c.H -Path $f), (Get-SBytes (New-SDoc @((Get-SBox))))); $c.B1 = Get-HpFileBytes $f }
        }
      } -AfterPlace {
        param($c)
        if ($c.Mode -cne 'lock') { return }
        $c.Clock = [Diagnostics.Stopwatch]::StartNew()
        foreach ($wait in 1000, 7000) {
          Start-Sleep -Milliseconds $wait
          $lg = Read-SLogNow $c.H
          $c.Probes += [pscustomobject]@{
            ProvisionHeld = -not (Test-HpFreeLock (Get-HpLockFile $c.H 'agentspace'))
            LeaseFree = Test-HpFreeLock (Join-Path $c.H.StateDir 'leases.lock')
            Waiting = @($lg.Lines | Where-Object { $_.Step -ceq 'success' -or ($_.Step -ceq 'run' -and $_.Outcome -ceq 'done') }).Count -eq 0
          }
        }
        # The run's write gives up at its 30 seconds; the check holds the lock until the run has ended.
        while ($c.Job.State -eq 'Running' -and $c.Clock.Elapsed.TotalSeconds -lt 80) { Start-Sleep -Milliseconds 200 }
        $c.Clock.Stop()
      } -Cleanup { param($c) Remove-SHold $c }
    } finally { Remove-SHold $ctx }
    $r = $ctx.Result
    $log = Read-HpLog $h 'agentspace'
    $finalBytes = Get-HpFileBytes (Get-HpHostsFile $h)
    $lines = if ($r) { Get-HpLines $r.Stderr } else { @() }

    $p = New-HpProblems
    $pathOk = $false
    Add-SGateProblems $p $ctx
    if ($null -eq $r) { $p.Add('The run was not collected') } else { Add-HpRunProblems $p $r 0 }
    if ((Get-HpLogShape $log) -cne "$startSteps,place done placed=1,success failed,run done") { $p.Add("The log's lines are not: $startSteps, place done placed=1, success failed, run done (they are: $(Get-HpLogShape $log))") }
    else {
      $sf = @($log.Lines | Where-Object { $_.Step -ceq 'success' })[0]
      $m = [regex]::Match($sf.Rest, $C.Pattern)
      if (-not $m.Success) { $p.Add('The success failed line''s reason is not the registry''s message, as the log gives it') }
      elseif ($C.PathOf -and -not (Test-HpSamePath $m.Groups['p'].Value $(if ($C.PathOf -ceq 'lock') { Get-SHostsLockPath $h } else { Get-HpHostsFile $h }))) { $p.Add('The path in the reason is not the file the write failed on') }
      else { $pathOk = $true }
      # No warning (step 9, its end): no line of stderr carries the reason the log gives, whatever the warning's words.
      $why = $sf.Rest -replace '^ reason=', ''
      if ($why.Length -gt 0 -and @($lines | Where-Object { $_.Contains($why, [StringComparison]::Ordinal) }).Count -gt 0) { $p.Add('A line of stderr carries the failed write''s reason: it is logged and not warned') }
    }
    if ($lines | Where-Object { $_ -match 'success|last_success' }) { $p.Add('A line of stderr names the failed write: it is logged and not warned') }
    if ($C.Mode -ceq 'lock' -and $ctx.Clock -and $ctx.Clock.Elapsed.TotalSeconds -lt 25) { $p.Add('The write gave up before the lock''s 30 seconds') }
    Write-HpProblems "${sec}: success write fails: $($C.N): success failed reason=<why> after step 9's lines and before run done, exit 0 and no warning (step 9, its end; pin P23)" $p

    $p = New-HpProblems
    if ($null -eq $ctx.B1) { $p.Add('The check has no registry to compare with') }
    elseif (-not (Test-HpSameBytes $ctx.B1 $finalBytes)) { $p.Add('The registry changed: a failed write of last_success_at changes nothing') }
    if ($null -ne $finalBytes -and [Text.UTF8Encoding]::new($false).GetString($finalBytes) -match 'last_success_at') { $p.Add('The registry holds last_success_at') }
    Write-HpProblems "${sec}: success write fails: $($C.N): no change to the record, and none to the file (pin P23)" $p

    if ($C.Mode -ceq 'lock') {
      $p = New-HpProblems
      if ($ctx.Probes.Count -ne 2) { $p.Add('The check made no probe of the locks while the write waited') }
      foreach ($pr in $ctx.Probes) {
        if (-not $pr.Waiting) { $p.Add('The log already held success failed or run done while the check held hosts.lock: the run was not waiting for it') }
        if (-not $pr.ProvisionHeld) { $p.Add('provision.lock was not held while the write waited for hosts.lock') }
        if (-not $pr.LeaseFree) { $p.Add('The lease state lock was held while the write waited for hosts.lock: the write takes hosts.lock alone') }
      }
      if (-not (Test-HpFreeLock (Get-HpLockFile $h 'agentspace'))) { $p.Add('provision.lock is still held after the run ended') }
      if (-not $pathOk) { $p.Add('The success failed line does not name hosts.lock''s own path in the lock''s message') }
      Write-HpProblems "${sec}: success write lock: the write takes hosts.lock alone, with provision.lock held and the lease state lock free, and its message names hosts.lock's path (step 9, its end; pin P23; driver.md, Lock order)" $p
    }
  }
}
Test-SuccessWriteFails @{ N = 'hosts.lock held past its 30 seconds'; Mode = 'lock'; PathOf = 'lock'; Pattern = '^ reason=The host registry''s lock (?<p>.+) is held by another program; try again\.$' }
Test-SuccessWriteFails @{ N = 'hosts.json that cannot be written'; Mode = 'readonly'; PathOf = 'json'; Pattern = '^ reason=Cannot write (?<p>.+)\.$' }
Test-SuccessWriteFails @{ N = 'hosts.json that has become not valid'; Mode = 'invalid'; PathOf = 'json'; Pattern = '^ reason=The host registry (?<p>.+) cannot be read: \S.*\.$' }
Test-SuccessWriteFails @{ N = 'the record no longer there'; Mode = 'gone'; PathOf = ''; Pattern = '^ reason=\S.*$' }

Exit-ConformanceRun

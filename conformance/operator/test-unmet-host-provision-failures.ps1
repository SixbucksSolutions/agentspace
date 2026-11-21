#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's `provision` when a step fails (docs/operator.md, "provision",
# "deploy", "AWS failures", "The operator's folder, one per command", "Which checks show it"), run
# through the test seam. Card #0156 (E0.07-T04-S02). The successful runs are in
# test-unmet-host-provision.ps1.
#
# EXPECTED TO FAIL AGAINST v0.06's OPERATOR, which has no `provision`: every case reports a FAIL there.
# A v0.07 build MUST pass every result line.
#
# What it does: each case makes its own throwaway home, seeds a registry (no record, a `creating`
# record or a `ready` one), one `pending` lease of the project, and the stand-ins of
# conformance/operator/host-provision-common.ps1, and makes one step fail. It then checks, for the
# step's failure: the calls made and none after it, the exit code, the step's own failure message as
# stderr's last line, the log's lines (`<step> failed reason=<message>` with the stand-in program's
# own error text where the step ran an AWS call, and no `run done` and no `place` line), the record
# (`failed` with last_attempt_at kept and an instance ID kept when the run learned one, for a host that
# was never `ready`; unchanged since step 3 for a `ready` host, which stays `ready`), provision.lock
# released, the lease left `pending` and its file as it was, and the operator's folder removed after
# the failure too.
#
#   - Each of steps 4 to 7 failing: deploy, start, sync, bootstrap, for a host that was never ready,
#     and the deploy and the bootstrap for a ready one; an expired sign-in with #0034's message.
#   - The existence check's StackStatus for provision: ROLLBACK_COMPLETE, a status ending
#     _IN_PROGRESS and any other status, each failing with operator.md's message before any
#     `cloudformation deploy`, recording `failed` for a host that was never ready and leaving a ready
#     host ready, the message in the log.
#
# SAFETY. See test-unmet-host-provision.ps1: the harness's isolation, the AWS gate, stand-ins for
# `aws`, `ssh`, `scp` and `go`, made-up instance IDs. A failure reason never quotes the program's output.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-provision-failures.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#provision'
$stack = 'agentspace-host-agentspace'
$leaseId = '11111111-1111-4111-8111-111111111111'
$oldTime = '2026-01-01T00:00:00Z'
$ec2Error = 'An error occurred (UnauthorizedOperation) when calling the DescribeInstances operation: You are not authorized to perform this operation.'
$deployFailed = 'Stack deploy failed; see the stack events in the console.'
$signInReason = $script:HpSignIn.Substring('agentspace: '.Length)
$stackNew = @('aws cloudformation describe-stacks', 'aws cloudformation deploy', 'aws cloudformation describe-stacks')
$dsc = 'aws ec2 describe-instances'
$syncCalls = @('ssh:sync', 'scp', 'scp', 'ssh:sync')

# One case: $C holds N (the name), Seed (none, creating or ready), Aws (overrides of the AWS answers),
# Rules (ssh and scp rules), Seq (the calls, in order), Shape (the log's lines), Msg (the step's failure
# message without its prefix), Id (the instance ID the final record holds, or empty) and Has (a line
# of stderr that the stand-in program wrote, which MUST pass through).
function Test-FailedStep($C) {
  Invoke-HpInHome -Name "${sec}: $($C.N)" -Body {
    param($h, $s)
    $ready = $C.Seed -eq 'ready'
    $record = switch ($C.Seed) {
      'ready' { New-HpRecord 'agentspace' 'ready' $script:HpHostId $oldTime }
      'creating' { New-HpRecord 'agentspace' 'creating' '' $oldTime }
      default { $null }
    }
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @($record | Where-Object { $_ }))
    Set-HpLeases $h @((New-HpLeaseRecord -Id $leaseId))
    $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
    $aws = if ($ready) { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists } else { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId }
    foreach ($k in $C.Aws.Keys) { $aws[$k] = $C.Aws[$k] }
    Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = $aws; rules = @($C.Rules) }
    $t0 = [DateTime]::UtcNow
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $t1 = [DateTime]::UtcNow
    $calls = @(Get-HpCalls $s)
    $log = Read-HpLog $h 'agentspace'
    $lines = Get-HpLines $r.Stderr
    $first = if ($calls.Count -gt 0) { Get-HpRecordFromText (Get-HpSnapshotText $s $calls[0]['n']) 'agentspace' } else { $null }

    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    $want = "agentspace: $($C.Msg)"
    if ($lines.Count -eq 0 -or $lines[-1] -cne $want) { $p.Add('The last line of stderr is not the step''s own failure message') }
    if ($C.Has -and -not ($lines -ccontains $C.Has)) { $p.Add('The AWS CLI''s own stderr line did not pass through') }
    Write-HpProblems "${sec}: $($C.N): exit 1 with the step's own failure message as its last line" $p

    $got = Get-HpSequence $calls
    $p = New-HpProblems
    if (($got -join ',') -cne ($C.Seq -join ',')) { $p.Add("The calls are not the steps up to the failure and nothing after it ($($got.Count) calls, not $($C.Seq.Count))") }
    Write-HpProblems "${sec}: $($C.N): the calls stop at the failed step, nothing after it runs" $p

    $p = New-HpProblems
    if ((Get-HpLogShape $log) -cne $C.Shape) { $p.Add("The log's lines are not: $($C.Shape)") }
    else {
      $lastLine = $log.Lines[$log.Lines.Count - 1]
      $wantReason = ' reason=' + (($C.Msg -replace '[\x00-\x1f\x7f]', '?'))
      if ($lastLine.Rest -cne $wantReason) { $p.Add('The failed line''s reason is not the failure message without its agentspace: prefix, in full') }
      if (-not (Test-HpTimeBetween $lastLine.Time $t0 $t1)) { $p.Add('The failed line''s time is not the time it was written') }
    }
    if (@($log.Lines | Where-Object { $_.Step -eq 'place' -or ($_.Step -eq 'run' -and $_.Outcome -eq 'done') }).Count -gt 0) { $p.Add('The log holds a place line or run done after a failed step') }
    Write-HpProblems "${sec}: $($C.N): the log gets <step> failed with the whole failure message as its reason, and neither a place line nor run done (When step 4, 5, 6 or 7 fails; its log)" $p

    $p = New-HpProblems
    $fin = Get-HpRecord $h 'agentspace'
    if ($null -eq $first) { $p.Add('No record at the first call') }
    elseif ($null -eq $fin) { $p.Add('The record is gone') }
    else {
      $wantState = if ($ready) { 'ready' } else { 'failed' }
      if ($fin['state'] -cne $wantState) { $p.Add("The record's state is '$($fin['state'])', not $wantState") }
      if ($C.Id) { if ($fin['instance_id'] -cne $C.Id) { $p.Add('The record does not hold the instance ID the run learned') } }
      elseif ($fin.ContainsKey('instance_id')) { $p.Add('The record holds an instance ID the run never learned') }
      if ($fin['last_attempt_at'] -cne $first['last_attempt_at']) { $p.Add('last_attempt_at is not the run''s start, as step 3 wrote it') }
      if (-not (Test-HpTimeBetween $fin['last_attempt_at'] $t0 $t1)) { $p.Add('last_attempt_at is not the time of this run''s start') }
      if (@($fin.Keys | Where-Object { $_ -notin 'project', 'stack', 'state', 'instance_id', 'last_attempt_at' }).Count -gt 0) { $p.Add('The record holds another member') }
    }
    $box = Get-HpRecord $h 'swimcoach'
    if ($null -eq $box -or $box['instance_id'] -cne $script:HpBoxId -or $box['state'] -cne 'ready') { $p.Add('The box''s record changed') }
    Write-HpProblems "${sec}: $($C.N): $(if ($ready) { 'a ready host is not written at all after step 3, and stays ready' } else { 'a host that was never ready is recorded failed, with last_attempt_at kept and the instance ID if the run learned one' }) (When step 4, 5, 6 or 7 fails)" $p

    $p = New-HpProblems
    if ($null -eq $log.Bytes) { $p.Add('The run never started: there is no provision.log') }
    if (-not (Test-HpFreeLock (Get-HpLockFile $h 'agentspace'))) { $p.Add('provision.lock is still held after the run ended') }
    if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpLeasesFile $h)) $leasesBefore)) { $p.Add('The lease state file changed: the lease was not left pending, as it was') }
    $dc = @($calls | Where-Object { (Get-HpCallName $_) -eq 'aws cloudformation deploy' }) | Select-Object -First 1
    if ($dc) {
      $tpl = @($dc['paths'] | Where-Object { $_['arg'] -cmatch '\\agentspace\.yaml$' })
      if ($tpl.Count -ne 1 -or $tpl[0]['type'] -ne 'file' -or $null -eq (Get-HpOperatorHex $h $tpl[0]['arg'])) { $p.Add('The deploy did not see its template as a file under operator\<hex>\, so the folder removal after the failure shows nothing') }
    }
    if (@(Get-HpOperatorFolders $h).Count -ne 0) { $p.Add('A folder under operator\ was left after the failed run') }
    Write-HpProblems "${sec}: $($C.N): provision.lock released, the project's lease left pending and the file untouched, and the operator's own folder removed after the failure too (D48; The operator's folder, one per command)" $p
  }
}

# ---- Each of steps 4 to 7 failing, for a host that was never ready --------------------------------------
Test-FailedStep @{
  N = 'a failed deploy, for a host that was never ready'; Seed = 'none'
  Aws = @{ 'cloudformation deploy' = @((New-HpStep '' "A line the AWS CLI wrote to stderr`n" 255)) }
  Rules = @(); Seq = @('aws cloudformation describe-stacks', 'aws cloudformation deploy'); Shape = 'run begun,deploy failed'
  Msg = $deployFailed; Id = ''; Has = 'A line the AWS CLI wrote to stderr'
}
Test-FailedStep @{
  N = 'a failed start, whose AWS error text is the log line''s reason'; Seed = 'none'
  Aws = @{ 'ec2 describe-instances' = @((New-HpStep '' $ec2Error 254)) }
  Rules = @(); Seq = $stackNew + @($dsc); Shape = 'run begun,deploy done,start failed'
  Msg = $ec2Error; Id = $script:HpHostId
}
Test-FailedStep @{
  N = 'a failed sync (its first copy)'; Seed = 'creating'
  Aws = @{}; Rules = @(@{ kind = 'scp'; nth = 1; code = 1; err = "scp wrote this`n" })
  Seq = $stackNew + @($dsc, $dsc, $dsc, $dsc, 'ssh:sync', 'scp'); Shape = 'run begun,deploy done,start done,sync failed'
  Msg = 'Copy to the instance failed.'; Id = $script:HpHostId; Has = 'scp wrote this'
}
Test-FailedStep @{
  N = 'a failed bootstrap (its ssh exits 7)'; Seed = 'none'
  Aws = @{}; Rules = @(@{ kind = 'ssh'; class = 'bootstrap'; code = 7 })
  Seq = $stackNew + @($dsc, $dsc, $dsc, $dsc) + $syncCalls + @($dsc, $dsc, 'ssh:bootstrap'); Shape = 'run begun,deploy done,start done,sync done,bootstrap failed'
  Msg = 'The toolchain bootstrap failed on the instance.'; Id = $script:HpHostId
}
Test-FailedStep @{
  N = 'an expired AWS sign-in, with #0034''s message'; Seed = 'none'
  Aws = @{ 'cloudformation describe-stacks' = @((New-HpStep '' 'Unable to locate credentials. You can configure credentials by running "aws login".' 255)) }
  Rules = @(); Seq = @('aws cloudformation describe-stacks'); Shape = 'run begun,deploy failed'
  Msg = $signInReason; Id = ''
}

# ---- A ready host: the failure is logged and the record stays ready ----------------------------------------
Test-FailedStep @{
  N = 'a failed deploy, for a ready host'; Seed = 'ready'
  Aws = @{ 'cloudformation deploy' = @((New-HpStep '' '' 255)) }
  Rules = @(); Seq = @('aws cloudformation describe-stacks', 'aws cloudformation deploy'); Shape = 'run begun,deploy failed'
  Msg = $deployFailed; Id = $script:HpHostId
}
Test-FailedStep @{
  N = 'a failed bootstrap, for a ready host'; Seed = 'ready'
  Aws = @{}; Rules = @(@{ kind = 'ssh'; class = 'bootstrap'; code = 255 })
  Seq = $stackNew + @($dsc, $dsc, $dsc, $dsc) + $syncCalls + @($dsc, $dsc, 'ssh:bootstrap'); Shape = 'run begun,deploy done,start done,sync done,bootstrap failed'
  Msg = 'The toolchain bootstrap failed on the instance.'; Id = $script:HpHostId
}

# ---- The existence check's StackStatus, through provision -------------------------------------------------
$rollback = "Stack '$stack' is ROLLBACK_COMPLETE, which no update can start from. Look at its events, delete it with 'aws cloudformation delete-stack --region us-west-2 --stack-name $stack', then run 'agentspace-operator provision -Host agentspace' again."
$inProgress = { param($st) "Stack '$stack' is ${st}; try again once that operation has finished." }
$otherStatus = "Stack '$stack' is DELETE_FAILED, which no update can start from. Look at its events; it can still hold the host's instance and disk, so do not delete it unless nothing on the host is to be kept."
$one = @('aws cloudformation describe-stacks')
Test-FailedStep @{
  N = 'a stack in ROLLBACK_COMPLETE, for a host that was never ready'; Seed = 'none'
  Aws = @{ 'cloudformation describe-stacks' = @((New-HpStack $stack 'ROLLBACK_COMPLETE')) }
  Rules = @(); Seq = $one; Shape = 'run begun,deploy failed'; Msg = $rollback; Id = ''
}
Test-FailedStep @{
  N = 'a stack in ROLLBACK_COMPLETE, for a ready host'; Seed = 'ready'
  Aws = @{ 'cloudformation describe-stacks' = @((New-HpStack $stack 'ROLLBACK_COMPLETE')) }
  Rules = @(); Seq = $one; Shape = 'run begun,deploy failed'; Msg = $rollback; Id = $script:HpHostId
}
Test-FailedStep @{
  N = 'a stack in CREATE_IN_PROGRESS, for a creating host'; Seed = 'creating'
  Aws = @{ 'cloudformation describe-stacks' = @((New-HpStack $stack 'CREATE_IN_PROGRESS')) }
  Rules = @(); Seq = $one; Shape = 'run begun,deploy failed'; Msg = (& $inProgress 'CREATE_IN_PROGRESS'); Id = ''
}
Test-FailedStep @{
  N = 'a stack in UPDATE_IN_PROGRESS, for a ready host'; Seed = 'ready'
  Aws = @{ 'cloudformation describe-stacks' = @((New-HpStack $stack 'UPDATE_IN_PROGRESS')) }
  Rules = @(); Seq = $one; Shape = 'run begun,deploy failed'; Msg = (& $inProgress 'UPDATE_IN_PROGRESS'); Id = $script:HpHostId
}
Test-FailedStep @{
  N = 'a stack in a status no update can start from (DELETE_FAILED), which names no delete command'; Seed = 'none'
  Aws = @{ 'cloudformation describe-stacks' = @((New-HpStack $stack 'DELETE_FAILED')) }
  Rules = @(); Seq = $one; Shape = 'run begun,deploy failed'; Msg = $otherStatus; Id = ''
}

Exit-ConformanceRun

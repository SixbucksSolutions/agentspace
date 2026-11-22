#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's `provision` for a host that has run to its end (docs/operator.md,
# "provision", "The host registry, as the operator uses it", "The operator's folder, one per command",
# "The test seam", "Which checks show it"), run through the test seam. Card #0156 (E0.07-T04-S02), the
# bootstrap-and-provision writer's file for the successful runs. Its failed runs, its first steps and
# its place_pending are in test-unmet-host-provision-failures.ps1, test-unmet-host-provision-start.ps1
# and test-unmet-host-provision-place.ps1.
#
# EXPECTED TO FAIL AGAINST v0.06's OPERATOR, which has no `provision`, no `-Host` and no `bootstrap`:
# every case reports a FAIL there. A v0.07 build MUST pass every result line.
#
# What it does: each case makes its own throwaway home (the harness's, with a throwaway CA) and writes
# into it the host registry, the project list, the stack parameters file, the public key, and a fake
# localswim receipt and Bot credential. A stand-in `aws`, `ssh`, `scp` and `go` (conformance/operator/
# host-provision-common.ps1) log each call with its arguments, snapshot hosts.json at the moment of the
# call, record the ssh's standard input, and answer from a scenario. `go` is found through a PATH of
# the check's own, so no case starts the real `go`. sync's step 7 builds only when C:\Projects\localswim
# exists (operator.md, sync, step 7), and the `go` stand-in runs with that folder as its working folder, so a
# case that shows the build MUST report SKIPPED, naming the folder, when the folder is absent. That is one
# Test-Path -PathType Container on it; no case lists, reads or writes anything in it. The cases check the
# calls in order, the record at each of them, the log, the streams, the exit code, provision.lock held
# for the whole run, and the operator's folder.
#
#   1. A new host's first run (a listed project with no record): the calls in order across the steps;
#      the record `creating` at the run's start, its instance ID removed and written again by the
#      deploy step, then `ready` with the ID and the run's start time; the log's lines; the sign-in
#      line last; every ssh and scp naming agentspace-host.<project>; one operator\<hex>\ folder for
#      the deploy and the sync, named to the stand-ins and removed at the end; provision.lock held at
#      every call and released at the end; -Quiet.
#   2. `provision -Host swimcoach`: the box's own deploy, the alias `agentspace`, the record `ready`
#      throughout, no sign-in line, no key, certificate or alias; and a project list that is not valid
#      never failing it.
#   3. A run for a project with a record, whatever the project list holds: a `failed` record goes to
#      `creating` and on to `ready`; a project list that is not valid does not stop it.
#   4. The ready-host rule: a `ready` record only has `last_attempt_at` written at the run's start, is
#      never `creating`, keeps `ready`, and has a learned `instance_id` written; a deploy that reads
#      neither the public key nor the stack parameters file for a stack that exists; no sign-in line.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. A failure reason never quotes the program's
# output. Every instance ID is made up. `ssh-keygen` and `go` have no seam (operator.md, Q30): the real
# `ssh-keygen.exe` makes the operator's key and certificate against the throwaway CA, and the `go` that
# runs is the stand-in on the check's PATH. What the seam cannot reach, a real key exchange, sshd's trust
# of the CA, a real instance, a real `go build`, is for E0.07-T05-S01's live check.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-provision.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#provision'
$hr = 'operator.md#the-host-registry-as-the-operator-uses-it'
$stack = 'agentspace-host-agentspace'
$alias = 'agentspace-host.agentspace'
$signInLine = 'Sign in on this host once, by hand, under a lease: see "Signing in on a new host" in the agentspace README.'
$boxRecordText = '{"project":"swimcoach","stack":"agentbox","state":"ready","instance_id":"' + $script:HpBoxId + '"}'
$startSteps = 'run begun,deploy done,start done,sync done,bootstrap done,record done,place done placed=0,run done'

# The record of the project at the registry snapshot of call $N.
function Get-Rec($S, [int]$N) { Get-HpRecordFromText (Get-HpSnapshotText $S $N) 'agentspace' }
function Get-BoxRec($S, [int]$N) { Get-HpRecordFromText (Get-HpSnapshotText $S $N) 'swimcoach' }
# The first call of a name, as the stand-in saw it.
function Find-Call($Calls, [string]$Name) { @($Calls | Where-Object { (Get-HpCallName $_) -eq $Name }) | Select-Object -First 1 }
# What a record holds, as one comparable text: the members in the table's order.
function Format-Rec($R) {
  if ($null -eq $R) { return '(none)' }
  (@('project', 'stack', 'state', 'instance_id', 'last_attempt_at') | ForEach-Object { if ($R.ContainsKey($_)) { "$_=$($R[$_])" } }) -join ' '
}
# The lines of one run the stand-ins saw, for the ssh and scp alias checks.
function Add-AliasProblems($Problems, $Calls, [string]$Want) {
  if (@($Calls | Where-Object { $_['kind'] -in 'ssh', 'scp' }).Count -eq 0) { $Problems.Add('No ssh or scp was started') }
  foreach ($c in @($Calls | Where-Object { $_['kind'] -in 'ssh', 'scp' })) {
    if ($c['alias'] -cne $Want) { $Problems.Add("Call $($c['n']) ($($c['kind'])) names '$($c['alias'])', not '$Want'"); break }
  }
}

# ---- 1. A new host's first run --------------------------------------------------------------------
Invoke-HpInHome -Name "${sec}: a new host's first run" -NeedsLocalswim -Body {
  param($h, $s)
  Initialize-HpHome -H $h
  New-HpFakeLocalswim -H $h | Out-Null
  $aws = Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId
  Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = $aws; rules = @((Get-HpBootstrapRule)) }
  $t0 = [DateTime]::UtcNow
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace') -WithGo
  $t1 = [DateTime]::UtcNow
  $calls = @(Get-HpCalls $s)
  $log = Read-HpLog $h 'agentspace'
  $lines = Get-HpLines $r.Stderr

  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  if ($r.Stdout -cne "deploy-out`nbootstrap-out`n") { $p.Add('Stdout is not the deploy''s and the bootstrap''s own stdout, passed through, in that order') }
  Write-HpProblems "${sec}: a listed project with no record is made: exit 0, and the steps' stdout passes through (provision, Stdout)" $p

  # The steps in order, as one list of calls.
  $want = @('aws cloudformation describe-stacks', 'aws cloudformation deploy', 'aws cloudformation describe-stacks') + @(1..4 | ForEach-Object { 'aws ec2 describe-instances' }) +
  @('ssh:sync', 'scp', 'scp', 'ssh:sync') + $script:HpPatchWaitSequence + @('go', 'scp', 'ssh:sync', 'ssh:push') + @(1..2 | ForEach-Object { 'aws ec2 describe-instances' }) + @('ssh:bootstrap')
  $got = Get-HpSequence $calls
  $p = New-HpProblems
  if (($got -join ',') -cne ($want -join ',')) {
    $i = 0
    while ($i -lt $got.Count -and $i -lt $want.Count -and $got[$i] -ceq $want[$i]) { $i++ }
    $p.Add("The calls differ at call $($i + 1) of $($want.Count) (expected $(if ($i -lt $want.Count) { $want[$i] } else { 'no more calls' }), got $(if ($i -lt $got.Count) { $got[$i] } else { 'none' }); $($got.Count) calls)")
  }
  foreach ($m in (Get-HpPatchWaitProblems $calls $script:HpHostId)) { $p.Add($m) }
  Write-HpProblems "${sec}: the steps run in order: the existence check, the deploy, the instance ID read, the start and wait, sync (its copies, its install, its patch wait's three Session Manager calls, its build, its push), the start and wait again, then bootstrap (steps 4 to 7; The patch wait)" $p

  $p = New-HpProblems
  Add-AliasProblems $p $calls $alias
  Write-HpProblems "${sec}: every ssh and scp of the run, sync's and bootstrap's, names the host's own alias agentspace-host.<project> (steps 6 and 7)" $p

  if ($calls.Count -eq 0) { Write-ConformanceResult -Name "${sec}: the registry's records and the log of a first run" -Status FAIL -Reason 'No stand-in was started, so the run never reached a step'; return }
  $first = @($calls)[0]
  $rec1 = Get-Rec $s $first['n']
  $p = New-HpProblems
  if ($null -eq $rec1) { $p.Add('No record in the registry at the run''s first call') }
  else {
    if ($rec1['state'] -cne 'creating') { $p.Add("The record is '$($rec1['state'])' at the first call, not creating") }
    if ($rec1['stack'] -cne $stack) { $p.Add('The record names another stack') }
    if ($rec1.ContainsKey('instance_id')) { $p.Add('The record holds an instance ID before the deploy step') }
    if (-not $rec1.ContainsKey('last_attempt_at') -or -not (Test-HpTimeBetween $rec1['last_attempt_at'] $t0 $t1)) { $p.Add('last_attempt_at is missing, not in the UTC form, or not the time of the run''s start') }
    if (($rec1.Keys | Sort-Object) -join ',' -cne 'last_attempt_at,project,stack,state') { $p.Add('The record holds members other than project, stack, state and last_attempt_at') }
  }
  if ((Get-HpSnapshotText $s $first['n']) -and (Get-BoxRec $s $first['n'])['instance_id'] -cne $script:HpBoxId) { $p.Add('The box''s record changed') }
  Write-HpProblems "${sec}: step 3 adds the record {project, stack agentspace-host-<project>, state creating} with last_attempt_at set to the run's start, before any call (step 3; D44, D55)" $p
  $la = if ($rec1) { $rec1['last_attempt_at'] } else { '' }

  $stackCalls = @($calls | Where-Object { (Get-HpCallName $_) -eq 'aws cloudformation describe-stacks' })
  $dsc = @($calls | Where-Object { (Get-HpCallName $_) -eq 'aws ec2 describe-instances' })
  $p = New-HpProblems
  if ($stackCalls.Count -lt 2 -or $dsc.Count -lt 1) { $p.Add('The calls the record is read at were not made') }
  else {
    $r3 = Get-Rec $s $stackCalls[1]['n']
    if ($null -eq $r3 -or $r3.ContainsKey('instance_id')) { $p.Add('The instance ID was not removed from the record before the deploy step read it from the stack (deploy, step 5)') }
    $r4 = Get-Rec $s $dsc[0]['n']
    if ($null -eq $r4 -or $r4['instance_id'] -cne $script:HpHostId) { $p.Add('The record does not hold the instance ID the deploy step read, by the start step') }
    if ($null -eq $r4 -or $r4['state'] -cne 'creating') { $p.Add('The state is not creating while the run works') }
    if ($null -ne $r4 -and $r4['last_attempt_at'] -cne $la) { $p.Add('last_attempt_at changed during the run') }
  }
  $bootCall = @($calls | Where-Object { $_['kind'] -eq 'ssh' -and $_['class'] -eq 'bootstrap' }) | Select-Object -First 1
  if ($bootCall) {
    $rb = Get-Rec $s $bootCall['n']
    if ($null -eq $rb -or $rb['state'] -cne 'creating' -or $rb['instance_id'] -cne $script:HpHostId) { $p.Add('The record is not creating with its instance ID when bootstrap runs') }
  } else { $p.Add('No bootstrap ssh was made') }
  Write-HpProblems "${sec}: the deploy step removes the instance ID and writes the stack's again (so it is missing at the read and present after), and the state stays creating until step 8, with last_attempt_at left as step 3 wrote it (steps 4 and 8; deploy, step 5)" $p

  $final = Get-HpRecordFromText ([IO.File]::ReadAllText((Get-HpHostsFile $h))) 'agentspace'
  $wantBytes = [Text.UTF8Encoding]::new($false).GetBytes((Get-HpHostsText -BoxId $script:HpBoxId -Records @((New-HpRecord 'agentspace' 'ready' $script:HpHostId $la))))
  $p = New-HpProblems
  if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpHostsFile $h)) $wantBytes)) { $p.Add("hosts.json is not the box's record unchanged and the host's: ready, instance_id the stack's, last_attempt_at the run's start ($(Format-Rec $final)); written compact, UTF-8, one LF") }
  Write-HpProblems "${sec}: step 8 records the host ready with the instance ID the deploy read and last_attempt_at left as step 3 wrote it, and changes no other record (step 8)" $p

  $p = New-HpProblems
  if ($null -eq $log.Bytes) { $p.Add('There is no provision.log') }
  else {
    if ($log.Bytes.Length -ge 3 -and $log.Bytes[0] -eq 0xEF -and $log.Bytes[1] -eq 0xBB -and $log.Bytes[2] -eq 0xBF) { $p.Add('The log starts with a byte order mark') }
    if ($log.Bytes[$log.Bytes.Length - 1] -ne 10) { $p.Add('The log does not end in a line feed') }
    if ($log.Bytes -contains 13) { $p.Add('The log holds a carriage return') }
    $shape = Get-HpLogShape $log
    if ($shape -cne $startSteps) { $p.Add("The log's lines are not: $startSteps") }
    foreach ($l in $log.Lines) {
      if (-not $l.Ok) { $p.Add("A log line is not <time> <step> <outcome>: '$($l.Raw -replace '[^\x20-\x7E]', '?')'"); break }
      if (-not (Test-HpTimeBetween $l.Time $t0 $t1)) { $p.Add("The time of a '$($l.Step) $($l.Outcome)' line is not the time it was written, in UTC"); break }
    }
    $begun = @($log.Lines | Where-Object { $_.Step -eq 'run' -and $_.Outcome -eq 'begun' })
    if ($begun.Count -eq 1 -and $begun[0].Rest -ne '') { $p.Add('The run begun line holds more than <time> run begun') }
    if (@($log.Lines | Where-Object { $_.Outcome -eq 'done' -and $_.Step -ne 'place' -and $_.Rest -ne '' }).Count -gt 0) { $p.Add('A done line holds text after its outcome, which only place done placed=<n> does') }
  }
  Write-HpProblems "${sec}: the log holds one line per step in order, run begun, deploy, start, sync, bootstrap, record, place done placed=0, run done, each <time> <step> <outcome> in UTC, UTF-8 with no byte order mark and one LF each (its log)" $p

  $p = New-HpProblems
  if ($lines.Count -eq 0 -or $lines[0] -cne 'Provisioning the host of ''agentspace''...') { $p.Add('The first line of stderr is not Provisioning the host of ''agentspace''...') }
  $iDeploy = [Array]::IndexOf($lines, "Deploying stack '$stack' in us-west-2...")
  $iInst = [Array]::IndexOf($lines, "Instance: $($script:HpHostId)")
  $iWait = [Array]::IndexOf($lines, "Checking that $($script:HpHostId) has finished this boot's patch run...")
  $iPatched = [Array]::IndexOf($lines, "Patched: $($script:HpHostId)")
  $iBoot = [Array]::IndexOf($lines, "Bootstrapped: $($script:HpHostId)")
  $iRec = [Array]::IndexOf($lines, 'Recorded the host of ''agentspace'' as ready.')
  if (-not (0 -lt $iDeploy -and $iDeploy -lt $iInst -and $iInst -lt $iWait -and $iWait -lt $iPatched -and $iPatched -lt $iBoot -and $iBoot -lt $iRec)) { $p.Add('stderr does not hold Provisioning, Deploying stack, Instance: <id>, Checking that <id> has finished this boot''s patch run..., Patched: <id>, Bootstrapped: <id> and Recorded the host ... as ready., in that order') }
  Write-HpProblems "${sec}: stderr carries the run's own messages in order, ending with Recorded the host of '<project>' as ready. (steps 3, 4, 7 and 8)" $p

  $p = New-HpProblems
  if ($lines.Count -eq 0 -or $lines[-1] -cne $signInLine) { $p.Add('The last line of stderr is not the sign-in line') }
  if ($iRec -ge 0 -and ($lines.Count - 1) -le $iRec) { $p.Add('The sign-in line does not come after Recorded') }
  Write-HpProblems "${sec}: a host that was not ready ends with the sign-in line as the run's last line (step 10; D52)" $p

  $p = New-HpProblems
  $notHeld = @($calls | Where-Object { $_['lockHeld'] -ne $true })
  if ($notHeld.Count -gt 0) { $p.Add("provision.lock was not held at call $($notHeld[0]['n']) of $($calls.Count)") }
  if (-not (Test-Path -LiteralPath (Get-HpLockFile $h 'agentspace') -PathType Leaf)) { $p.Add('provision.lock is not there after the run: the file is never deleted') }
  elseif (-not (Test-HpFreeLock (Get-HpLockFile $h 'agentspace'))) { $p.Add('provision.lock is still held after the run ended') }
  Write-HpProblems "${sec}: provision.lock is held at every call of the run and free once it ends, the file kept (step 2; state.md, The provisioning run's lock)" $p

  # The operator's folder: one for the whole run, named to the stand-ins.
  $p = New-HpProblems
  $deploy = Find-Call $calls 'aws cloudformation deploy'
  $tpl = if ($deploy) { $deploy['a'][([Array]::IndexOf($deploy['a'], '--template-file') + 1)] } else { '' }
  $hexes = [System.Collections.Generic.List[string]]::new()
  $scps = @($calls | Where-Object { $_['kind'] -eq 'scp' })
  $goCall = Find-Call $calls 'go'
  if ($tpl) { $hexes.Add([string](Get-HpOperatorHex $h $tpl)) }
  if ($scps.Count -ge 1) { $hexes.Add([string](Get-HpOperatorHex $h $scps[0]['a'][4])) }
  if ($goCall) { $hexes.Add([string](Get-HpOperatorHex $h $goCall['a'][([Array]::IndexOf($goCall['a'], '-o') + 1)])) }
  if ($scps.Count -ge 3) { $hexes.Add([string](Get-HpOperatorHex $h $scps[2]['a'][3])) }
  if ($hexes.Count -ne 4 -or ($hexes | Where-Object { -not $_ }).Count -gt 0) { $p.Add('The template, the system folder, the build and its copy are not each under %USERPROFILE%\.agentspace\operator\<32 lowercase hex>\') }
  elseif (@($hexes | Select-Object -Unique).Count -ne 1) { $p.Add('The deploy and the sync write into different folders: a run has one') }
  if ($deploy -and $deploy['paths'].Count -gt 0 -and ($deploy['paths'] | Where-Object { $_['arg'] -ceq $tpl -and $_['type'] -eq 'file' }).Count -eq 0) { $p.Add('The template was not a file at the time of the deploy call') }
  if ($tpl -and -not ($tpl -cmatch '\\agentspace\.yaml$')) { $p.Add('The template is not named agentspace.yaml') }
  if ($scps.Count -ge 1) {
    $sys = @($scps[0]['paths'])[0]
    $map = Get-HpRepoSystemMap
    if ($null -eq $sys -or $sys['type'] -ne 'dir') { $p.Add('The system folder was not a folder at the time of the copy') }
    else {
      $wantNames = ($map.Keys | Sort-Object) -join ','
      $gotNames = ($sys['files'].Keys | Sort-Object) -join ','
      if ($gotNames -cne $wantNames) { $p.Add('The system folder does not hold exactly the repository''s box\system files') }
      else { foreach ($k in $map.Keys) { if ($sys['files'][$k] -cne $map[$k]) { $p.Add("system/$k is not the repository's file"); break } } }
    }
  }
  if (@(Get-HpOperatorFolders $h).Count -ne 0) { $p.Add('A folder under operator\ was left after the run ended') }
  Write-HpProblems "${sec}: the deploy and the sync write into one operator\<hex>\ folder named to the stand-ins (the template, box\system whole, the build), and it is removed as the run ends (The operator's folder, one per command)" $p

  $p = New-HpProblems
  if ($deploy) {
    $key = ($script:HpKeyLine)
    $caPub = [IO.File]::ReadAllText((Join-Path $h.UserProfile '.agentspace\ca\agentspace_ca.pub')).Trim()
    $caKey = ($caPub -split '\s+')[0..1] -join ' '
    $wantA = @('cloudformation', 'deploy', '--region', 'us-west-2', '--stack-name', $stack, '--template-file', $tpl, '--capabilities', 'CAPABILITY_IAM', '--no-fail-on-empty-changeset', '--tags', "project=$stack", '--parameter-overrides', "SshPublicKey=$key", 'SubnetId=SUBNET_VALUE', 'VpcId=VPC_VALUE', "HostName=$stack", "CaPublicKey=$caKey")
    $gotA = @($deploy['a'])
    if ($gotA.Count -ne $wantA.Count) { $p.Add("cloudformation deploy got $($gotA.Count) arguments, not $($wantA.Count)") }
    else { for ($i = 0; $i -lt $wantA.Count; $i++) { if ($gotA[$i] -cne $wantA[$i]) { $p.Add("cloudformation deploy argument $($i + 1) is not as the spec gives it"); break } } }
  } else { $p.Add('No cloudformation deploy was made') }
  Write-HpProblems "${sec}: a new host's stack is created with SshPublicKey, only SubnetId and VpcId of the stack parameters file, HostName and the CA's key with no comment, never the box's InstanceType or RootVolumeGiB (step 4; deploy, step 4)" $p

  $p = New-HpProblems
  $leases = Get-HpFileBytes (Get-HpLeasesFile $h)
  if ($null -ne $leases) { $p.Add('A lease state file was written with no lease to place') }
  $placeLines = @($log.Lines | Where-Object { $_.Step -eq 'place' })
  if ($placeLines.Count -ne 1 -or $placeLines[0].Rest -cne ' placed=0') { $p.Add('place is not logged once, as place done placed=0') }
  Write-HpProblems "${sec}: place_pending is called after the record is written and, with no pending lease, logs place done placed=0 and writes nothing (step 9)" $p

  $p = New-HpProblems
  foreach ($f in 'id_ed25519', 'id_ed25519-cert.pub') { if (-not (Test-Path -LiteralPath (Join-Path (Get-HpHostFolder $h 'agentspace') $f) -PathType Leaf)) { $p.Add("hosts\agentspace\$f was not made") } }
  if (-not (Test-Path -LiteralPath (Get-HpAliasFile $h $alias) -PathType Leaf)) { $p.Add("The host's operator alias was not written") }
  Write-HpProblems "${sec}: the sync step makes the host's operator key, certificate and alias before its first ssh (How the operator reaches a host)" $p
}

# ---- 1b. -Quiet --------------------------------------------------------------------------------------
Invoke-HpInHome -Name "${sec}: -Quiet" -Body {
  param($h, $s)
  Initialize-HpHome -H $h
  Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId); rules = @((Get-HpBootstrapRule)) }
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace', '-Quiet')
  $log = Read-HpLog $h 'agentspace'
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  if ($r.Stderr -match 'Provisioning the host|Recorded the host|Sign in on this host|Deploying stack|Bootstrapped:|Ready:') { $p.Add('-Quiet left a progress message on stderr') }
  if ($r.Stdout -cne "deploy-out`nbootstrap-out`n") { $p.Add('-Quiet changed what the steps pass through to stdout') }
  if ((Get-HpLogShape $log) -cne $startSteps) { $p.Add('-Quiet changed the log''s lines') }
  Write-HpProblems "${sec}: -Quiet quiets every step, the sign-in line included, and changes neither the log nor what the programs pass through (Stdout, Stderr, Its log)" $p
}

# ---- 2. provision -Host swimcoach ---------------------------------------------------------------------
Invoke-HpInHome -Name "${sec}: provision -Host swimcoach" -NeedsLocalswim -Body {
  param($h, $s)
  # The project list is not valid: it is read only for a project with no record, so it never fails the box's run.
  Initialize-HpHome -H $h -ProjectsText 'this is not json {'
  New-HpFakeLocalswim -H $h | Out-Null
  $aws = @{
    'cloudformation describe-stacks' = @((New-HpStack 'agentbox' 'UPDATE_COMPLETE' $script:HpBoxId))
    'cloudformation deploy' = @((New-HpStep "deploy-out`n"))
    'ec2 describe-instances' = @((New-HpDescribe))
  }
  Set-HpScenario -H $h -S $s -LockProject 'swimcoach' -Scenario @{ aws = $aws; rules = @((Get-HpBootstrapRule)) }
  $t0 = [DateTime]::UtcNow
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'swimcoach') -WithGo
  $t1 = [DateTime]::UtcNow
  $calls = @(Get-HpCalls $s)
  $log = Read-HpLog $h 'swimcoach'
  $lines = Get-HpLines $r.Stderr

  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  if ($r.Stdout -cne "deploy-out`nbootstrap-out`n") { $p.Add('Stdout is not the deploy''s and the bootstrap''s stdout') }
  Write-HpProblems "${sec}: provision -Host swimcoach provisions the box, exit 0, though the project list is not valid (step 1: the list is read only when there is no record; P14)" $p

  $want = @('aws cloudformation deploy', 'aws cloudformation describe-stacks') + @(1..4 | ForEach-Object { 'aws ec2 describe-instances' }) + @('ssh:sync', 'scp', 'scp', 'ssh:sync') + $script:HpPatchWaitSequence + @('go', 'scp', 'ssh:sync', 'ssh:push') + @(1..2 | ForEach-Object { 'aws ec2 describe-instances' }) + @('ssh:bootstrap')
  $got = Get-HpSequence $calls
  $p = New-HpProblems
  if (($got -join ',') -cne ($want -join ',')) { $p.Add("The calls are not the box's own deploy (no existence check), the instance ID read, the start, sync with its patch wait, the start and bootstrap: $($got.Count) calls") }
  foreach ($m in (Get-HpPatchWaitProblems $calls $script:HpBoxId)) { $p.Add($m) }
  Add-AliasProblems $p $calls 'agentspace'
  Write-HpProblems "${sec}: for the box the deploy step is the box's own deploy with no existence check, and every ssh and scp names the alias agentspace (provision; step 4)" $p

  $deploy = Find-Call $calls 'aws cloudformation deploy'
  $p = New-HpProblems
  if ($deploy) {
    $ov = @($deploy['a'])
    $i = [Array]::IndexOf($ov, '--parameter-overrides')
    $tail = if ($i -ge 0) { @($ov[($i + 1)..($ov.Count - 1)]) } else { @() }
    $want = @("SshPublicKey=$($script:HpKeyLine)", 'SubnetId=SUBNET_VALUE', 'VpcId=VPC_VALUE', 'InstanceType=m9g.4xlarge', 'RootVolumeGiB=64')
    if ((($tail | Sort-Object) -join '|') -cne (($want | Sort-Object) -join '|')) { $p.Add('The box''s deploy does not pass SshPublicKey and every member of stack-parameters.json, and nothing else') }
    if ($ov -match '^(HostName|CaPublicKey)=') { $p.Add('The box''s deploy passes HostName or CaPublicKey') }
  } else { $p.Add('No cloudformation deploy was made') }
  Write-HpProblems "${sec}: the box's provision passes every member of stack-parameters.json and neither HostName nor CaPublicKey (provision; deploy, step 4)" $p

  if ($calls.Count -eq 0) { Write-ConformanceResult -Name "${sec}: the box's record, its log and its files" -Status FAIL -Reason 'No stand-in was started, so the run never reached a step'; return }
  $p = New-HpProblems
  foreach ($c in $calls) {
    $sn = Get-HpSnapshotText $s $c['n']
    $rc = if ($sn) { Get-HpRecordFromText $sn 'swimcoach' } else { $null }
    if ($sn -and ($null -eq $rc -or $rc['state'] -cne 'ready')) { $p.Add("The box's record is not ready at call $($c['n'])"); break }
  }
  $rc1 =Get-BoxRec $s $calls[0]['n']
  if ($null -eq $rc1 -or -not $rc1.ContainsKey('last_attempt_at') -or -not (Test-HpTimeBetween $rc1['last_attempt_at'] $t0 $t1)) { $p.Add('The box''s record has no last_attempt_at of the run''s start at its first call') }
  elseif ($rc1['instance_id'] -cne $script:HpBoxId) { $p.Add('Step 3 wrote more than last_attempt_at into a ready record') }
  $fin = Get-HpRecordFromText ([IO.File]::ReadAllText((Get-HpHostsFile $h))) 'swimcoach'
  if ($null -eq $fin -or $fin['state'] -cne 'ready' -or $fin['instance_id'] -cne $script:HpBoxId -or $fin['last_attempt_at'] -cne $rc1['last_attempt_at']) { $p.Add('The box''s final record is not ready, with its instance ID and the run''s last_attempt_at') }
  Write-HpProblems "${sec}: the box's record stays ready throughout, written at the start with last_attempt_at alone (step 3; P3)" $p

  $p = New-HpProblems
  if ((Get-HpLogShape $log) -cne $startSteps) { $p.Add("The log of hosts\swimcoach is not: $startSteps") }
  if ($lines | Where-Object { $_ -like 'Sign in on this host*' }) { $p.Add('The sign-in line was printed for the box') }
  if ($lines.Count -eq 0 -or $lines[-1] -cne 'Recorded the host of ''swimcoach'' as ready.') { $p.Add('The last line of stderr is not Recorded the host of ''swimcoach'' as ready.') }
  Write-HpProblems "${sec}: the box's run logs in hosts\swimcoach\, ends with no sign-in line, and its last line is the record's (step 10: never for the box)" $p

  $p = New-HpProblems
  if (Test-Path -LiteralPath (Join-Path (Get-HpHostFolder $h 'swimcoach') 'id_ed25519')) { $p.Add('A key was made for the box') }
  if (Test-Path -LiteralPath (Join-Path (Get-HpHostFolder $h 'swimcoach') 'id_ed25519-cert.pub')) { $p.Add('A certificate was signed for the box') }
  if (Test-Path -LiteralPath (Join-Path $h.SshDir 'agentspace.d') -PathType Container) { if (@(Get-ChildItem -LiteralPath (Join-Path $h.SshDir 'agentspace.d') -Force).Count -gt 0) { $p.Add('An alias was written for the box') } }
  Write-HpProblems "${sec}: the operator writes no key, certificate or alias for the box (How the operator reaches a host; -Host swimcoach)" $p
}

# ---- 3. A run for a project with a record, a project list that is not valid -----------------------------
Invoke-HpInHome -Name "${sec}: a failed record" -Body {
  param($h, $s)
  $old = 'i-abcdef1234567890a'
  Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @((New-HpRecord 'agentspace' 'failed' $old '2026-01-01T00:00:00Z'))) -ProjectsText "{`"version`":0}`n"
  Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId); rules = @((Get-HpBootstrapRule)) }
  $t0 = [DateTime]::UtcNow
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  $t1 = [DateTime]::UtcNow
  $calls = @(Get-HpCalls $s)
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  $r1 = if ($calls.Count -gt 0) { Get-Rec $s $calls[0]['n'] } else { $null }
  if ($null -eq $r1 -or $r1['state'] -cne 'creating') { $p.Add('A failed record is not set to creating at the run''s start') }
  elseif (-not (Test-HpTimeBetween $r1['last_attempt_at'] $t0 $t1)) { $p.Add('last_attempt_at is not the run''s start') }
  elseif ($r1['instance_id'] -cne $old) { $p.Add('step 3 changed the instance ID, which only the deploy step does') }
  $fin = Get-HpRecord $h 'agentspace'
  if ($null -eq $fin -or $fin['state'] -cne 'ready' -or $fin['instance_id'] -cne $script:HpHostId) { $p.Add('The record does not end ready with the stack''s instance ID') }
  if ($lines.Count -eq 0 -or $lines[-1] -cne $signInLine) { $p.Add('A host that was not ready does not end with the sign-in line') }
  Write-HpProblems "${sec}: a failed record goes to creating with last_attempt_at set to now, then ready, and a project list that is not valid does not stop a run for a project with a record; the sign-in line, since the host was not ready (steps 1, 3, 8 and 10; P14)" $p
}

# ---- 4. The ready-host rule ---------------------------------------------------------------------------
foreach ($case in @(
    @{ N = 'a learned instance ID that differs'; Stack = $script:HpNewId; Want = $script:HpNewId },
    @{ N = 'the same instance ID'; Stack = $script:HpHostId; Want = $script:HpHostId })) {
  $c = $case
  Invoke-HpInHome -Name "${sec}: a ready record, $($c.N)" -Body {
    param($h, $s)
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @((New-HpRecord 'agentspace' 'ready' $script:HpHostId '2026-01-01T00:00:00Z')))
    # The stack exists, so its deploy takes no parameter and reads neither the public key nor the parameters file.
    Remove-HpPath -H $h -Path (Join-Path $h.UserProfile '.ssh\id_ed25519.pub')
    Remove-HpPath -H $h -Path (Join-Path $h.UserProfile '.agentspace\stack-parameters.json')
    Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-HpHostAws -Project 'agentspace' -Id $c.Stack -Exists); rules = @((Get-HpBootstrapRule)) }
    $t0 = [DateTime]::UtcNow
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $t1 = [DateTime]::UtcNow
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $log = Read-HpLog $h 'agentspace'
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $first = if ($calls.Count -gt 0) { Get-Rec $s $calls[0]['n'] } else { $null }
    if ($null -eq $first) { $p.Add('No record at the first call') }
    else {
      if ($first['state'] -cne 'ready') { $p.Add('The record is not ready at the run''s start') }
      if ($first['instance_id'] -cne $script:HpHostId) { $p.Add('The start wrote more than last_attempt_at: the instance ID changed') }
      if (-not (Test-HpTimeBetween $first['last_attempt_at'] $t0 $t1)) { $p.Add('last_attempt_at is not the run''s start') }
    }
    foreach ($call in $calls) {
      $sn = Get-HpSnapshotText $s $call['n']
      $rc = if ($sn) { Get-HpRecordFromText $sn 'agentspace' } else { $null }
      if ($sn -and ($null -eq $rc -or $rc['state'] -cne 'ready')) { $p.Add("The record is not ready at call $($call['n']): a run never changes a ready record's state"); break }
    }
    $fin = Get-HpRecord $h 'agentspace'
    if ($null -eq $fin -or $fin['state'] -cne 'ready' -or $fin['instance_id'] -cne $c.Want -or $null -eq $first -or $fin['last_attempt_at'] -cne $first['last_attempt_at']) { $p.Add('The final record is not ready, with the stack''s instance ID and the run''s last_attempt_at') }
    $deploy = Find-Call $calls 'aws cloudformation deploy'
    if ($deploy -and ($deploy['a'] -contains '--parameter-overrides')) { $p.Add('The deploy of a stack that exists passes parameter overrides: every parameter keeps its previous value') }
    if ((Get-HpLogShape $log) -cne $startSteps) { $p.Add("The log's lines are not: $startSteps") }
    if ($lines | Where-Object { $_ -like 'Sign in on this host*' }) { $p.Add('The sign-in line was printed for a host that was already ready') }
    Write-HpProblems "${sec}: a ready record only has last_attempt_at written at the start, is ready at every call, keeps ready and has the instance ID written ($($c.N)); the deploy of an existing stack takes no parameter and reads no key or file; no sign-in line (steps 3, 4, 8 and 10; P3, P13)" $p
  }
}

Exit-ConformanceRun

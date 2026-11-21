#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `deploy` given -Host and of the box's `deploy` (docs/operator.md, "deploy",
# "The stack", "Calls by command", "The host registry, as the operator uses it", "Command line"; docs/state.md,
# "Files in the checkout", "The host registry"; D39, D44, D45, D49, D55). Card #0156 (E0.07-T04-S02),
# against the rows of operator.md's "Which checks show it" for `deploy -Host`, the existence check's
# StackStatus, stack-parameters.json, -InstanceType and the registry's removal and write. The operator's
# folder per command and its sweep are checked by test-unmet-host-deploy-folders.ps1.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that
# log every call and answer from files beside them (conformance/operator/host-commands-common.ps1),
# a made-up public key, a stack parameters file the case writes, and the home's throwaway CA. Every
# `cloudformation deploy` argument is compared exactly, in order, and the template's path as the
# pattern `...\.agentspace\operator\<32 hex>\agentspace.yaml`. Most cases end `deploy` at step 5 with a
# stack that has no InstanceId output, so that `sync` never runs; the cases that run it use ssh and scp
# stand-ins that succeed, and PATH holds no go.
#
# Cases: the existence check and every StackStatus the spec names; the arguments for a new stack, for
# an existing one, and for the box; stack-parameters.json naming HostName or CaPublicKey, lacking
# SubnetId or VpcId, or missing; -InstanceType for a new and an existing stack; the failures of
# `cloudformation deploy`; step 5 (the instance ID removed, read, written, `Instance: <id>`); and
# step 6 (`sync` with deploy's -Host and -Quiet, exit 1 when it fails).
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. The key is made up, and so is every ID. A
# failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-deploy.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$signIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
$templatePattern = 're:\\\.agentspace\\operator\\[0-9a-f]{32}\\agentspace\.yaml$'
$deployFailed = 'agentspace: Stack deploy failed; see the stack events in the console.'

# The `cloudformation deploy` call of Calls by command, as a list of arguments: the stack's name, the
# template in the command's own folder, the capability, the empty-change-set flag, the tag, then the
# overrides when there are any.
function Get-DeployCall([string]$Stack, [string[]]$Overrides = @()) {
  $a = @('cloudformation', 'deploy', '--region', 'us-west-2', '--stack-name', $Stack, '--template-file', $templatePattern,
    '--capabilities', 'CAPABILITY_IAM', '--no-fail-on-empty-changeset', '--tags', "project=$Stack")
  if ($Overrides.Count -gt 0) { $a += '--parameter-overrides'; $a += $Overrides }
  , $a
}
function Get-DoesNotExist([string]$Stack) { "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $Stack does not exist" }

Invoke-HostFile -FileName 'test-unmet-host-deploy.ps1' -Body {
  param($fx)
  $idBox = $script:IdBox; $idA = $script:IdA; $idNew = $script:IdNew
  $stack = 'agentspace-host-agentspace'
  $ca = ([IO.File]::ReadAllText((Join-Path $fx.CaDir 'agentspace_ca.pub')).Trim() -split '\s+')[0..1] -join ' '
  $key = $fx.PubKey
  $sub = "SubnetId=$($fx.SubnetValue)"; $vpc = "VpcId=$($fx.VpcValue)"
  $createOverrides = @("SshPublicKey=$key", $sub, $vpc, "HostName=$stack", "CaPublicKey=$ca")
  $noOutput = New-Step (New-StackDoc -StackName $stack)
  $noInstanceId = "agentspace: Stack '$stack' has no InstanceId output."
  $deploying = "Deploying stack '$stack' in us-west-2..."
  $rec = @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -LastAttempt '2026-01-01T00:00:00Z'))
  $recBytes = Get-HostsBytes $rec
  $registryIsAsWritten = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $recBytes)) { 'hosts.json was changed' } }
  # A host whose record exists and whose stack is answered by $Steps (existence check, then step 5).
  function Set-HostStack($Steps, [hashtable]$More = @{}) {
    Set-HostsFile $recBytes
    Set-AwsScenario (Merge-Scenario @{ "cloudformation describe-stacks $stack" = @($Steps) } $More)
  }
  $setupNew = { Set-HostStack @((New-Step '' (Get-DoesNotExist $stack) 254), $noOutput) }
  function Test-PathText([string]$Text, [string]$Path) {
    $Text -ieq $Path -or $Text -ieq (Assert-ConformancePath -ConformanceHome $fx.H -Path $Path)
  }

  # ============ The existence check, and a stack that does not exist ============
  $x = 'operator.md#deploy'
  Test-HostCase "${x}: deploy -Host <project> asks first whether the stack exists, with describe-stacks of its own stack; one that does not exist is created with the creation parameters: SshPublicKey, SubnetId and VpcId alone of the file, HostName and CaPublicKey, in that order" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Stderr = (Join-Lines @($deploying, $noInstanceId))
    Calls = @((Get-CallStack $stack), (Get-DeployCall $stack $createOverrides), (Get-CallStack $stack))
    Check = { param($run, $started)
      $d = @(Get-AwsCalls | Where-Object { $_.Text -like 'cloudformation deploy*' })
      if ($d.Count -eq 1 -and $d[0].tpl -and -not $d[0].tpl.exists) { 'The template did not exist when cloudformation deploy was called' } } } -Setup {
    Set-HostStack @((New-Step '' (Get-DoesNotExist $stack) 254), $noOutput) -More @{}
    Set-ParamsFile ('{"version":0,"parameters":{"VpcId":"' + $fx.VpcValue + '","InstanceType":"m9g.4xlarge","SubnetId":"' + $fx.SubnetValue + '","RootVolumeGiB":"40"}}')
  }
  Test-HostCase "${x}: the stack's new parameters take no InstanceId, no ImageId and no member of the file but SubnetId and VpcId, and the box's InstanceType never sizes a new host" @('deploy', '-Host', 'agentspace') @{
    Exit = 1
    Check = { param($run, $started)
      $d = @(Get-AwsCalls | Where-Object { $_.Text -like 'cloudformation deploy*' })
      if ($d.Count -ne 1) { return 'cloudformation deploy was not called once' }
      $t = $d[0].Text
      foreach ($bad in 'InstanceType', 'RootVolumeGiB', 'ImageId', 'm9g.4xlarge') { if ($t.Contains($bad)) { "A new host's stack was deployed with '$bad'" } }
    } } -Setup {
    Set-HostStack @((New-Step '' (Get-DoesNotExist $stack) 254), $noOutput)
    Set-ParamsFile ('{"version":0,"parameters":{"VpcId":"' + $fx.VpcValue + '","InstanceType":"m9g.4xlarge","SubnetId":"' + $fx.SubnetValue + '","RootVolumeGiB":"40"}}')
  }
  Test-HostCase "${x}: a failure whose text holds 'does not exist' in capitals still takes the create path" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack $createOverrides), (Get-CallStack $stack)) } -Setup {
    Set-HostStack @((New-Step '' 'AN ERROR OCCURRED: STACK WITH ID X DOES NOT EXIST' 254), $noOutput)
  }
  Test-HostCase "${x}: the output of cloudformation deploy is passed through as it is written, and a stack in REVIEW_IN_PROGRESS takes the create path with the creation parameters" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Stdout = "deploy-stdout-marker`n"; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack $createOverrides), (Get-CallStack $stack)) } -Setup {
    Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status 'REVIEW_IN_PROGRESS')), $noOutput) -More @{ 'cloudformation deploy' = @(New-Step "deploy-stdout-marker`n") }
  }

  # ============ A stack that exists ============
  # Updated with every parameter's previous value, so no --parameter-overrides at all, and the public
  # key, the file and the CA are not read: the case removes the three, and deploy still reaches step 5.
  foreach ($status in 'CREATE_COMPLETE', 'UPDATE_COMPLETE', 'UPDATE_ROLLBACK_COMPLETE', 'IMPORT_COMPLETE', 'IMPORT_ROLLBACK_COMPLETE') {
    $st = $status
    Test-HostCase "${x}: a stack in $st is updated with no --parameter-overrides at all, and deploy reads neither the public key, the file nor the CA" @('deploy', '-Host', 'agentspace') @{
      Exit = 1; Stderr = (Join-Lines @($deploying, $noInstanceId)); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack), (Get-CallStack $stack)) } -Setup {
      Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status $st)))
      Remove-HomePath $fx.ParamsFile
      Remove-HomePath $fx.PubKeyFile
      Hide-Ca
    }
  }
  Test-HostCase "${x}: deploy -Host <project> -InstanceType <type> of an existing stack passes InstanceType=<type> alone, the one way a host changes size" @('deploy', '-Host', 'agentspace', '-InstanceType', 'm9g.2xlarge') @{
    Exit = 1; Stderr = (Join-Lines @($deploying, $noInstanceId)); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack @('InstanceType=m9g.2xlarge')), (Get-CallStack $stack)) } -Setup {
    Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE')))
    Remove-HomePath $fx.ParamsFile
    Remove-HomePath $fx.PubKeyFile
    Hide-Ca
  }
  Test-HostCase "${x}: deploy -Host <project> -InstanceType <type> of a new stack passes the creation parameters, with InstanceType=<type> last" @('deploy', '-Host', 'agentspace', '-InstanceType', 'm9g.2xlarge') @{
    Exit = 1; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack ($createOverrides + 'InstanceType=m9g.2xlarge')), (Get-CallStack $stack)) } -Setup $setupNew
  Test-HostCase "${x}: deploy -Host <project> -InstanceType <type> of a stack in REVIEW_IN_PROGRESS passes the creation parameters, with InstanceType=<type> last" @('deploy', '-Host', 'agentspace', '-InstanceType', 'm7i.metal-24xl') @{
    Exit = 1; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack ($createOverrides + 'InstanceType=m7i.metal-24xl')), (Get-CallStack $stack)) } -Setup {
    Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status 'REVIEW_IN_PROGRESS')), $noOutput)
  }
  Test-HostCase "${x}: deploy -Quiet prints no Deploying line, and still prints a failure" @('deploy', '-Host', 'agentspace', '-Quiet') @{
    Exit = 1; Stderr = "$noInstanceId`n"; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack $createOverrides), (Get-CallStack $stack)) } -Setup $setupNew

  # ============ The status that stops deploy, before any cloudformation deploy ============
  $rb = "agentspace: Stack '$stack' is ROLLBACK_COMPLETE, which no update can start from. Look at its events, delete it with 'aws cloudformation delete-stack --region us-west-2 --stack-name $stack', then run 'agentspace-operator provision -Host agentspace' again."
  Test-HostCase "${x}: a stack in ROLLBACK_COMPLETE fails with the delete message before any cloudformation deploy, and writes no state" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "$rb`n"; Calls = @(Get-CallStack $stack); Check = $registryIsAsWritten } -Setup {
    Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status 'ROLLBACK_COMPLETE')))
  }
  foreach ($status in 'CREATE_IN_PROGRESS', 'UPDATE_IN_PROGRESS', 'DELETE_IN_PROGRESS', 'ROLLBACK_IN_PROGRESS', 'UPDATE_COMPLETE_CLEANUP_IN_PROGRESS', 'UPDATE_ROLLBACK_IN_PROGRESS', 'UPDATE_ROLLBACK_COMPLETE_CLEANUP_IN_PROGRESS', 'IMPORT_IN_PROGRESS') {
    $st = $status
    Test-HostCase "${x}: a stack in $st fails with the try-later message before any cloudformation deploy, and writes no state" @('deploy', '-Host', 'agentspace') @{
      Exit = 1; Stderr = "agentspace: Stack '$stack' is $st; try again once that operation has finished.`n"; Calls = @(Get-CallStack $stack); Check = $registryIsAsWritten } -Setup {
      Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status $st)))
    }
  }
  foreach ($status in 'UPDATE_ROLLBACK_FAILED', 'DELETE_FAILED', 'ROLLBACK_FAILED') {
    $st = $status
    Test-HostCase "${x}: a stack in $st fails with the message that names no delete command, before any cloudformation deploy" @('deploy', '-Host', 'agentspace') @{
      Exit = 1
      Stderr = "agentspace: Stack '$stack' is $st, which no update can start from. Look at its events; it can still hold the host's instance and disk, so do not delete it unless nothing on the host is to be kept.`n"
      Calls = @(Get-CallStack $stack); Check = $registryIsAsWritten } -Setup {
      Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status $st)))
    }
  }
  foreach ($case in @(
      @('has no StackStatus', '{"Stacks":[{"StackName":"x","Outputs":[]}]}', 'no Stacks[0].StackStatus'),
      @('has a StackStatus that is not a string', '{"Stacks":[{"StackName":"x","StackStatus":5}]}', 'no Stacks[0].StackStatus'),
      @('has no Stacks[0]', '{"Stacks":[]}', 'no Stacks[0].StackStatus')
    )) {
    $doc = $case[1]; $why = $case[2]
    Test-HostCase "${x}: an answer that $($case[0]) fails with Unexpected answer from cloudformation describe-stacks: $why." @('deploy', '-Host', 'agentspace') @{
      Exit = 1; Stderr = "agentspace: Unexpected answer from cloudformation describe-stacks: $why.`n"; Calls = @(Get-CallStack $stack); Check = $registryIsAsWritten } -Setup {
      Set-HostStack @((New-Step $doc))
    }
  }
  Test-HostCase "${x}: an answer that is not JSON fails with Unexpected answer from cloudformation describe-stacks: not JSON." @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "agentspace: Unexpected answer from cloudformation describe-stacks: not JSON.`n"; Calls = @(Get-CallStack $stack) } -Setup {
    Set-HostStack @((New-Step 'this is not json'))
  }
  Test-HostCase "${x}: an existence check that fails for another reason fails deploy with the CLI's own text (AWS failures), and makes no other call" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "agentspace: An error occurred (AccessDenied) when calling the DescribeStacks operation: User is not authorized`n"; Calls = @(Get-CallStack $stack) } -Setup {
    Set-HostStack @((New-Step '' 'An error occurred (AccessDenied) when calling the DescribeStacks operation: User is not authorized' 254))
  }
  Test-HostCase "${x}: an existence check that fails because the sign-in has expired prints exactly the sign-in message" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "$signIn`n"; Calls = @(Get-CallStack $stack) } -Setup {
    Set-HostStack @((New-Step '' 'Unable to locate credentials. You can configure credentials by running "aws login".' 255))
  }

  # ============ stack-parameters.json ============
  $sp = 'operator.md#deploy'
  $paramsWith = { param([string]$Members) '{"version":0,"parameters":{' + $Members + '}}' }
  $badMember = {
    param($run, [string]$Member)
    $m = [regex]::Match($run.Stderr.TrimEnd("`n"), '^agentspace: (.+) is not a valid stack parameters file: (.+)\.$')
    if (-not $m.Success) { return 'Stderr is not agentspace: <path> is not a valid stack parameters file: <reason>.' }
    if (-not (Test-PathText $m.Groups[1].Value $fx.ParamsFile)) { 'The path is not the stack parameters file''s' }
    if (-not $m.Groups[2].Value.Contains($Member)) { "The reason does not name $Member" }
    if ($m.Groups[2].Value.Contains("`n")) { 'The reason holds a line break' }
  }
  foreach ($member in 'HostName', 'CaPublicKey') {
    $mem = $member
    $members = '"SubnetId":"' + $fx.SubnetValue + '","VpcId":"' + $fx.VpcValue + '","' + $mem + '":"x"'
    Test-HostCase "${sp}: a stack parameters file that names $mem is refused for the box, with no AWS call at all" @('deploy') @{
      Exit = 1; Calls = @(); Check = { param($run, $started) & $badMember $run $mem } } -Setup { Set-ParamsFile (& $paramsWith $members) }
    Test-HostCase "${sp}: a stack parameters file that names $mem is refused for -Host swimcoach, as for the box" @('deploy', '-Host', 'swimcoach') @{
      Exit = 1; Calls = @(); Check = { param($run, $started) & $badMember $run $mem } } -Setup { Set-ParamsFile (& $paramsWith $members) }
    Test-HostCase "${sp}: a stack parameters file that names $mem is refused for a new host's stack, before any call that changes anything" @('deploy', '-Host', 'agentspace') @{
      Exit = 1; Calls = @(Get-CallStack $stack); Check = { param($run, $started) & $badMember $run $mem; & $registryIsAsWritten $run $started } } -Setup {
      Set-HostStack @((New-Step '' (Get-DoesNotExist $stack) 254))
      Set-ParamsFile (& $paramsWith $members)
    }
    Test-HostCase "${sp}: a stack parameters file that names $mem is not read for a host's stack that exists, so it is not refused" @('deploy', '-Host', 'agentspace') @{
      Exit = 1; Stderr = (Join-Lines @($deploying, $noInstanceId)); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack), (Get-CallStack $stack)) } -Setup {
      Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE')))
      Set-ParamsFile (& $paramsWith $members)
    }
  }
  $needBoth = { param($run) $m = [regex]::Match($run.Stderr.TrimEnd("`n"), '^agentspace: (.+) MUST name SubnetId and VpcId for a new host''s stack\.$'); if (-not $m.Success) { 'Stderr is not agentspace: <path> MUST name SubnetId and VpcId for a new host''s stack.' } elseif (-not (Test-PathText $m.Groups[1].Value $fx.ParamsFile)) { 'The path is not the stack parameters file''s' } }
  foreach ($case in @(
      @('SubnetId', ('"VpcId":"' + $fx.VpcValue + '"')),
      @('VpcId', ('"SubnetId":"' + $fx.SubnetValue + '"')),
      @('both SubnetId and VpcId', '"InstanceType":"m9g.4xlarge"'),
      @('every member', '')
    )) {
    $members = $case[1]
    Test-HostCase "${sp}: a stack parameters file that lacks $($case[0]) is refused for a new host's stack with the message that names both, before any call that changes anything, writing nothing" @('deploy', '-Host', 'agentspace') @{
      Exit = 1; Calls = @(Get-CallStack $stack); Check = { param($run, $started) & $needBoth $run; & $registryIsAsWritten $run $started } } -Setup {
      Set-HostStack @((New-Step '' (Get-DoesNotExist $stack) 254))
      Set-ParamsFile (& $paramsWith $members)
    }
    Test-HostCase "${sp}: a stack parameters file that lacks $($case[0]) is not refused for a host's stack that exists" @('deploy', '-Host', 'agentspace') @{
      Exit = 1; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack), (Get-CallStack $stack)) } -Setup {
      Set-HostStack @((New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE')))
      Set-ParamsFile (& $paramsWith $members)
    }
  }
  Test-HostCase "${sp}: a missing stack parameters file fails a new host's stack with Missing <path>; copy infra\stack-parameters.example.json and fill it in., before any call that changes anything" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = @(Get-CallStack $stack)
    Check = { param($run, $started)
      $m = [regex]::Match($run.Stderr.TrimEnd("`n"), '^agentspace: Missing (.+); copy infra\\stack-parameters\.example\.json and fill it in\.$')
      if (-not $m.Success) { 'Stderr is not agentspace: Missing <path>; copy infra\stack-parameters.example.json and fill it in.' }
      elseif (-not (Test-PathText $m.Groups[1].Value $fx.ParamsFile)) { 'The path is not the stack parameters file''s' } } } -Setup {
    Set-HostStack @((New-Step '' (Get-DoesNotExist $stack) 254))
    Remove-HomePath $fx.ParamsFile
  }

  # ============ The box's deploy ============
  $bx = 'operator.md#deploy'
  $boxMembers = '"VpcId":"' + $fx.VpcValue + '","SubnetId":"' + $fx.SubnetValue + '","InstanceType":"m9g.4xlarge","RootVolumeGiB":"40"'
  $boxExpect = {
    param($run, $started)
    $d = @(Get-AwsCalls | Where-Object { $_.Text -like 'cloudformation deploy*' })
    if ($d.Count -ne 1) { return 'cloudformation deploy was not called once' }
    $all = @(Get-AwsCalls)
    if ($all.Count -ne 2 -or $all[0].a[1] -cne 'deploy' -or $all[1].Text -cne (Get-CallStack 'agentbox')) { return 'The box''s deploy is not the deploy call and the read of its stack, with no existence check before it' }
    $a = $d[0].a
    $wantHead = Get-DeployCall 'agentbox' @()
    $i = [array]::IndexOf($a, '--parameter-overrides')
    if ($i -lt 0) { return 'The box''s deploy passed no --parameter-overrides' }
    $head = @($a[0..($i - 1)])
    if (-not (Test-CallMatch $wantHead $head)) { return 'The arguments before --parameter-overrides are not the box''s (stack, template, capability, flag, tag project=agentbox)' }
    $ov = @($a[($i + 1)..($a.Count - 1)])
    if ($ov[0] -cne "SshPublicKey=$($fx.PubKey)") { 'The first override is not SshPublicKey=<the public key>' }
    $rest = @($ov | Select-Object -Skip 1 | Sort-Object)
    $expect = @("InstanceType=m9g.4xlarge", "RootVolumeGiB=40", "SubnetId=$($fx.SubnetValue)", "VpcId=$($fx.VpcValue)" | Sort-Object)
    if (($rest -join '|') -cne ($expect -join '|')) { 'The other overrides are not every member of the file, and nothing else (no HostName, no CaPublicKey)' }
  }
  foreach ($line in @(@('deploy'), @('deploy', '-Host', 'swimcoach'), @('deploy', '-Host', 'SwimCoach'))) {
    Test-HostCase "${bx}: $($line -join ' ') deploys the agentbox stack with no existence check: SshPublicKey and every member of the file, and neither HostName nor CaPublicKey" ([string[]]$line) @{
      Exit = 1; Stderr = (Join-Lines @("Deploying stack 'agentbox' in us-west-2...", "agentspace: Stack 'agentbox' has no InstanceId output."))
      Check = $boxExpect } -Setup {
      Set-HostsFile $recBytes
      Set-AwsScenario @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox')) }
      Set-ParamsFile ('{"version":0,"parameters":{' + $boxMembers + '}}')
    }
  }
}
Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `deploy`'s steps 4 to 6 for a host and for the box (docs/operator.md, "deploy",
# "The host registry, as the operator uses it", "Laptop files", "The test seam"; docs/state.md, "The
# host registry", "The instance ID", "The host registry lock"; D45, D49, D55). Card #0156
# (E0.07-T04-S02): the registry's removal and write of the instance ID, `Instance: <id>`, a registry
# that cannot be written, hosts.lock held, the failures of `cloudformation deploy`, and `sync` run with
# deploy's -Host and -Quiet.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that
# log every call, and what existed when the call arrived (the host registry's text among it), and answer
# from files beside them (conformance/operator/host-commands-common.ps1). A case that runs `sync` lets
# its ssh and scp stand-ins succeed, and PATH holds no go. Each case checks the AWS CLI calls in order,
# the messages on stderr, the exit code, and the registry's bytes.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. Every ID is made up, and a failure reason never
# quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-deploy-registry.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$signIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
$deployFailed = 'agentspace: Stack deploy failed; see the stack events in the console.'
$templatePattern = 're:\\\.agentspace\\operator\\[0-9a-f]{32}\\agentspace\.yaml$'
function Get-DeployCall([string]$Stack, [string[]]$Overrides = @()) {
  $a = @('cloudformation', 'deploy', '--region', 'us-west-2', '--stack-name', $Stack, '--template-file', $templatePattern,
    '--capabilities', 'CAPABILITY_IAM', '--no-fail-on-empty-changeset', '--tags', "project=$Stack")
  if ($Overrides.Count -gt 0) { $a += '--parameter-overrides'; $a += $Overrides }
  , $a
}

Invoke-HostFile -FileName 'test-unmet-host-deploy-registry.ps1' -Body {
  param($fx)
  $idBox = $script:IdBox; $idA = $script:IdA; $idNew = $script:IdNew
  $stack = 'agentspace-host-agentspace'
  $ca = ([IO.File]::ReadAllText((Join-Path $fx.CaDir 'agentspace_ca.pub')).Trim() -split '\s+')[0..1] -join ' '
  $createOverrides = @("SshPublicKey=$($fx.PubKey)", "SubnetId=$($fx.SubnetValue)", "VpcId=$($fx.VpcValue)", "HostName=$stack", "CaPublicKey=$ca")
  $attempt = '2026-01-01T00:00:00Z'
  $recNone = @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -LastAttempt $attempt))
  $recOld = @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -InstanceId $idA -LastAttempt $attempt))
  $recNew = @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -InstanceId $idNew -LastAttempt $attempt))
  $doesNotExist = New-Step '' "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $stack does not exist" 254
  $stackDone = New-Step (New-StackDoc -StackName $stack -Id $idNew -Status 'CREATE_COMPLETE')
  $deploying = "Deploying stack '$stack' in us-west-2..."
  $registryIs = { param($Want, [string]$What) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes $Want))) { "hosts.json is not $What" } }
  # The calls a `deploy -Host` of a new stack that succeeds makes: the existence check, the deploy, the read
  # of the stack's output, then sync's start-and-wait of a running host launched long ago.
  # Then the patch wait's three Session Manager calls, which sync makes after its install (operator.md, The patch wait).
  $fullCalls = @((Get-CallStack $stack), (Get-DeployCall $stack $createOverrides), (Get-CallStack $stack), (Get-CallDescribe $idNew), (Get-CallDescribe $idNew)) + @(Get-PatchWaitCalls $idNew)
  function Set-FullFlow([string[]]$Records, $StackSteps) {
    Set-HostsRecords $Records
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdNew) @{ "cloudformation describe-stacks $stack" = @($StackSteps) })
  }

  # ============ Step 5: the instance ID removed from the record, read from the stack, written again ============
  $d = 'operator.md#deploy'
  Test-HostCase "${d}: deploy -Host <project> of a new stack that succeeds prints Instance: <id>, writes the ID into the host's record, leaves its state alone, and runs sync, exit 0" @('deploy', '-Host', 'agentspace') @{
    Exit = 0; StderrLines = @($deploying, "Instance: $idNew", 'Copying box/system to the instance...'); Calls = $fullCalls; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started) & $registryIs $recNew 'the two records, the host''s holding the stack''s InstanceId output with its state and last attempt as they were' } } -Setup {
    Set-FullFlow $recNone @($doesNotExist, $stackDone)
  }
  Test-HostCase "${d}: step 5 removes the old instance ID from the record before it reads the stack, so the ID comes from the stack as the deploy left it, never from the record" @('deploy', '-Host', 'agentspace') @{
    Exit = 0
    Calls = @((Get-CallStack $stack), (Get-DeployCall $stack), (Get-CallStack $stack), (Get-CallDescribe $idNew), (Get-CallDescribe $idNew)) + @(Get-PatchWaitCalls $idNew); SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $c = @(Get-AwsCalls)
      if ($c.Count -ge 3) {
        if ((Get-RecordMember ([string]$c[0].hosts) 'agentspace' 'instance_id') -cne $script:IdA -or (Get-RecordMember ([string]$c[1].hosts) 'agentspace' 'instance_id') -cne $script:IdA) { 'The old instance ID was removed before cloudformation deploy had run' }
        if ($null -ne (Get-RecordMember ([string]$c[2].hosts) 'agentspace' 'instance_id')) { 'The record still held an instance ID when the stack''s output was read' }
      }
      & $registryIs $recNew 'the two records, the host''s now holding the new instance ID' } } -Setup {
    Set-FullFlow $recOld @((New-Step (New-StackDoc -StackName $stack -Id $idNew -Status 'UPDATE_COMPLETE')))
  }
  Test-HostCase "${d}: a stack with no InstanceId output fails step 5 with no instance ID in the record, which the next command reads from the stack, and runs no sync" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; StderrLines = @($deploying, "agentspace: Stack '$stack' has no InstanceId output."); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack), (Get-CallStack $stack))
    Check = { param($run, $started) & $registryIs $recNone 'the two records, the host''s with the old instance ID removed and no other change' } } -Setup {
    Set-HostsRecords $recOld
    Set-AwsScenario @{ "cloudformation describe-stacks $stack" = @((New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE'))) }
  }
  foreach ($line in @(@('deploy'), @('deploy', '-Host', 'swimcoach'))) {
    Test-HostCase "${d}: $($line -join ' ') writes the new instance ID into the box's record and prints Instance: <id>; the box's deploy makes no existence check" ([string[]]$line) @{
      Exit = 0; StderrLines = @("Deploying stack 'agentbox' in us-west-2...", "Instance: $idNew"); SshCalls = 'any'; ScpCalls = 'any'
      Calls = 'any'
      Check = { param($run, $started)
        $c = @(Get-AwsCalls)
        # The spec fixes SshPublicKey first and no order for the file's members (operator.md, Calls by command), so they compare as a set.
        $wantCalls = @((Get-CallStack 'agentbox'), (Get-CallDescribe $idNew), (Get-CallDescribe $idNew)) + @(Get-PatchWaitCalls $idNew)
        if ($c.Count -ne 7) { "The AWS CLI was called $($c.Count) times, not 7: the deploy, the read of the stack, sync's two reads of the instance and its patch wait's three calls" }
        else {
          $da = @($c[0].a)
          $oi = [array]::IndexOf($da, '--parameter-overrides')
          if ($oi -lt 1) { 'The box''s deploy passed no --parameter-overrides' }
          else {
            if (-not (Test-CallMatch (Get-DeployCall 'agentbox' @()) @($da[0..($oi - 1)]))) { 'The first call is not the box''s cloudformation deploy up to --parameter-overrides' }
            $ov = @($da[($oi + 1)..($da.Count - 1)])
            if ($ov[0] -cnotmatch '^SshPublicKey=') { 'The first override is not SshPublicKey=<the public key>' }
            if ((@($ov | Select-Object -Skip 1 | Sort-Object) -join '|') -cne ((@("SubnetId=$($fx.SubnetValue)", "VpcId=$($fx.VpcValue)") | Sort-Object) -join '|')) { 'The other overrides are not every member of the file, and nothing else' }
          }
          for ($k = 0; $k -lt 6; $k++) { if (-not (Test-CallMatch $wantCalls[$k] @($c[$k + 1].a))) { "AWS CLI call $($k + 2) is not the one the box's deploy makes there" } }
        }
        if ($c.Count -ge 2) {
          if ((Get-RecordMember ([string]$c[0].hosts) 'swimcoach' 'instance_id') -cne $script:IdBox) { 'The box''s old instance ID was removed before cloudformation deploy had run' }
          if ($null -ne (Get-RecordMember ([string]$c[1].hosts) 'swimcoach' 'instance_id')) { 'The box''s record still held an instance ID when the stack''s output was read' }
        }
        & $registryIs @((New-BoxRecord $script:IdNew)) 'the box''s record holding the stack''s InstanceId output' } } -Setup {
      Set-HostsRecords @((New-BoxRecord))
      Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdNew) @{ 'cloudformation describe-stacks agentbox' = @((New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdNew))) })
    }
  }
  Test-HostCase "${d}: with no host registry at all, the box's deploy writes hosts.json holding the box's record with the new instance ID" @('deploy') @{
    Exit = 0; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started) & $registryIs @((New-BoxRecord $script:IdNew)) 'the box''s record holding the stack''s InstanceId output' } } -Setup {
    Remove-HomePath $fx.HostsFile
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdNew) @{ 'cloudformation describe-stacks agentbox' = @((New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdNew))) })
  }
  Test-HostCase "${d}: a registry that cannot be written fails the removal with Cannot write <path>., and nothing after it runs (a read-only hosts.json)" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack))
    Check = { param($run, $started)
      $m = [regex]::Match($run.Stderr, '(?m)^agentspace: Cannot write (.+)\.$')
      if (-not $m.Success) { 'Stderr has no line agentspace: Cannot write <path>.' }
      elseif ($m.Groups[1].Value -ine $fx.HostsFile -and $m.Groups[1].Value -ine (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsFile)) { 'The path is not the registry''s' }
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes $recOld))) { 'hosts.json was changed' }
      (Get-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsFile)).IsReadOnly = $false
    } } -Setup {
    Set-HostsRecords $recOld
    Set-AwsScenario @{ "cloudformation describe-stacks $stack" = @((New-Step (New-StackDoc -StackName $stack -Id $script:IdNew -Status 'UPDATE_COMPLETE'))) }
    (Get-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsFile)).IsReadOnly = $true
  }
  Test-HostCase "${d}: hosts.lock held past its 30 seconds fails the removal with the lock's message after about 30 seconds, and writes nothing" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack))
    Check = { param($run, $started)
      if ($run.Stderr -cnotmatch '(?m)^agentspace: The host registry''s lock .+ is held by another program; try again\.$') { 'Stderr has no line agentspace: The host registry''s lock <path> is held by another program; try again.' }
      if ($run.Elapsed -lt 28 -or $run.Elapsed -gt 60) { "The command waited $([int]$run.Elapsed) seconds, not about 30" }
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes $recOld))) { 'hosts.json was changed' }
      Close-HostsLock } } -Setup {
    Set-HostsRecords $recOld
    Set-AwsScenario @{ "cloudformation describe-stacks $stack" = @((New-Step (New-StackDoc -StackName $stack -Id $script:IdNew -Status 'UPDATE_COMPLETE'))) }
    Hold-HostsLock
  } -Timeout 150

  # ============ A failed cloudformation deploy ============
  $f = 'operator.md#deploy'
  Test-HostCase "${f}: a non-zero exit of cloudformation deploy fails with Stack deploy failed, exit 1, passes the CLI's stderr through, and goes no further" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; StderrLines = @($deploying, 'An error occurred (ValidationError) when calling the CreateChangeSet operation: no', $deployFailed)
    Calls = @((Get-CallStack $stack), (Get-DeployCall $stack $createOverrides))
    Check = { param($run, $started) & $registryIs $recNone 'the two records as they were' } } -Setup {
    Set-HostsRecords $recNone
    Set-AwsScenario @{ "cloudformation describe-stacks $stack" = @($doesNotExist); 'cloudformation deploy' = @((New-Step '' ("An error occurred (ValidationError) when calling the CreateChangeSet operation: no`n") 255)) }
  }
  Test-HostCase "${f}: an expired sign-in in cloudformation deploy's error text prints exactly the sign-in message instead" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack $createOverrides))
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      if ($lines[-1] -cne $signIn) { 'The last line is not the sign-in message' }
      if ($lines -ccontains $deployFailed) { 'The deploy-failed message was printed as well' } } } -Setup {
    Set-HostsRecords $recNone
    Set-AwsScenario @{ "cloudformation describe-stacks $stack" = @($doesNotExist); 'cloudformation deploy' = @((New-Step '' ("Unable to locate credentials. You can configure credentials by running `"aws login`".`n") 255)) }
  }

  # ============ Step 6: sync, as if run with deploy's own -Host and -Quiet ============
  $s6 = 'operator.md#deploy'
  Test-HostCase "${s6}: deploy -Host <project> runs sync for that host: its ssh and scp calls go through the host's own alias, agentspace-host.<project>" @('deploy', '-Host', 'agentspace') @{
    Exit = 0; Calls = $fullCalls; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $ssh = @(Get-SshCalls); $scp = @(Get-ScpCalls)
      if ($ssh.Count -lt 2 -or $scp.Count -lt 2) { return 'sync started too few ssh and scp calls' }
      foreach ($c in $ssh) { if (@($c.a) -cnotcontains 'agentspace-host.agentspace') { 'An ssh call does not name the host''s alias' } }
      foreach ($c in $scp) { if (@($c.a | Where-Object { $_ -cmatch '^agentspace-host\.agentspace:' }).Count -ne 1) { 'An scp call does not name the host''s alias' } }
      Get-SignProblems 'agentspace' } } -Setup { Set-FullFlow $recNone @($doesNotExist, $stackDone) }
  Test-HostCase "${s6}: deploy -Quiet quiets its sync step too: nothing is printed on stderr, and the host is deployed and synced" @('deploy', '-Host', 'agentspace', '-Quiet') @{
    Exit = 0; Stderr = ''; Calls = $fullCalls; SshCalls = 'any'; ScpCalls = 'any' } -Setup { Set-FullFlow $recNone @($doesNotExist, $stackDone) }
  Test-HostCase "${s6}: deploy exits 1 when sync fails (the copy of box/system fails), and the last message is sync's" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      if ($lines[-1] -cne 'agentspace: Copy to the instance failed.') { 'The last line is not agentspace: Copy to the instance failed.' }
      & $registryIs $recNew 'the two records, the host''s holding the new instance ID' } } -Setup {
    Set-FullFlow $recNone @($doesNotExist, $stackDone)
    Set-ProgramAnswer scp -Code 1
  }
}
Exit-ConformanceRun

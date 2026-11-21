#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `status -All` and `down -All` (docs/operator.md, "Every host at once: -All",
# "status", "down", "How the operator reaches a host", "AWS failures"; docs/state.md, "The host
# registry"; D49, D51, D55). Card #0156 (E0.07-T04-S02). `sync -All` is checked by another file.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that
# log every call and answer from files beside them (conformance/operator/host-commands-common.ps1), over
# a host registry of six records in an order that is not alphabetical: the box, a ready host, a creating
# host and a failed host with no instance ID, a ready host with none (read from its stack), and a creating
# host with one. Each case checks the AWS CLI calls in order, the stderr exactly, the exit code, and the
# registry's bytes.
#
# Cases: the order and the Host lines; the five lines or `Instance  none recorded` (status), and
# `Stopping <id>.` or the skip line (down); -Quiet, which keeps the Host lines and status's report; a
# ready record's instance ID read from its stack and written into that record alone; going on past a
# failed host and the closing count; #0034's sign-in message ending the command at once; a missing
# registry as the box's record alone; and nothing signed and no alias written, with no CA in the home.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. Every instance and volume ID is made up, and a
# failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-commands-all.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$signIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
$denied = 'An error occurred (UnauthorizedOperation) when calling the DescribeVolumes operation: You are not authorized to perform this operation.'
$stopDenied = 'An error occurred (UnauthorizedOperation) when calling the StopInstances operation: You are not authorized to perform this operation.'

function Get-HostLine([string]$Project, [string]$State) {
  $stack = if ($Project -eq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" }
  "Host      $Project ($stack), $State"
}

Invoke-HostFile -FileName 'test-unmet-host-commands-all.ps1' -Body {
  param($fx)
  $idBox = $script:IdBox; $idA = $script:IdA; $idB = $script:IdB; $idC = $script:IdC
  $e = 'operator.md#every-host-at-once--all'
  # The six records, in the file's order, and what the registry holds once epsilon's ID is written.
  $records = @(
    (New-BoxRecord $idBox),
    (New-HostRecord -Project 'agentspace' -InstanceId $idA),
    (New-HostRecord -Project 'gamma' -State 'creating' -LastAttempt '2026-01-01T00:00:00Z'),
    (New-HostRecord -Project 'delta' -State 'failed' -LastAttempt '2026-01-02T00:00:00Z'),
    (New-HostRecord -Project 'epsilon'),
    (New-HostRecord -Project 'zeta' -State 'creating' -InstanceId $idB -LastAttempt '2026-01-03T00:00:00Z')
  )
  $afterEpsilon = @($records[0], $records[1], $records[2], $records[3],
    (New-HostRecord -Project 'epsilon' -InstanceId $idC), $records[5])
  $setupAll = {
    Set-HostsRecords $records
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB) (Get-HealthyAnswers -Id $script:IdC) @{
        'cloudformation describe-stacks agentspace-host-epsilon' = @(New-Step (New-StackDoc -StackName 'agentspace-host-epsilon' -Id $script:IdC))
      })
  }
  $registryIs = {
    param($Want, [string]$What)
    if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes $Want))) { "hosts.json is not $What" }
  }
  $statusAll = Join-Lines @(
    (Get-HostLine 'swimcoach' 'ready'), (Get-StatusLines $idBox).TrimEnd("`n"),
    (Get-HostLine 'agentspace' 'ready'), (Get-StatusLines $idA).TrimEnd("`n"),
    (Get-HostLine 'gamma' 'creating'), 'Instance  none recorded',
    (Get-HostLine 'delta' 'failed'), 'Instance  none recorded',
    (Get-HostLine 'epsilon' 'ready'), (Get-StatusLines $idC).TrimEnd("`n"),
    (Get-HostLine 'zeta' 'creating'), (Get-StatusLines $idB).TrimEnd("`n"))
  $statusCalls = @(
    (Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox),
    (Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA),
    (Get-CallStack 'agentspace-host-epsilon'),
    (Get-CallDescribe $idC), (Get-CallVolumes $idC), (Get-CallSsm $idC),
    (Get-CallDescribe $idB), (Get-CallVolumes $idB), (Get-CallSsm $idB))
  $nothingSigned = { param($run, $started) foreach ($m in (Get-NothingSignedProblems)) { $m } }

  # ============ status -All ============
  $s = 'operator.md#status'
  Test-HostCase "${s}: status -All reports every record in the file's order, each after its Host line: the five lines, or Instance  none recorded for a creating or failed host with no instance ID, with no AWS call for it; a ready host's missing instance ID is read from its stack" @('status', '-All') @{
    Exit = 0; Stderr = $statusAll; Calls = $statusCalls
    Check = { param($run, $started) & $registryIs $afterEpsilon 'the six records as they were, with the instance ID written into epsilon''s alone'; & $nothingSigned $run $started } } -Setup $setupAll
  Test-HostCase "${s}: status -All -Quiet still prints every Host line and every report" @('status', '-All', '-Quiet') @{
    Exit = 0; Stderr = $statusAll; Calls = $statusCalls } -Setup $setupAll
  Test-HostCase "${s}: status -All writes no key, no certificate and no alias, and needs no CA: with the home's CA moved away it still exits 0 with the same report" @('status', '-All') @{
    Exit = 0; Stderr = $statusAll; Calls = $statusCalls; Check = $nothingSigned } -Setup {
    & $setupAll
    Hide-Ca
  }
  Test-HostCase "${s}: status -All of a missing registry reports the box's record alone, reading its instance ID from the agentbox stack" @('status', '-All') @{
    Exit = 0; Stderr = (Join-Lines @((Get-HostLine 'swimcoach' 'ready'), (Get-StatusLines $idBox).TrimEnd("`n")))
    Calls = @((Get-CallStack 'agentbox'), (Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox))
    Check = { param($run, $started) & $registryIs @((New-BoxRecord $script:IdBox)) 'the box''s record holding the instance ID' } } -Setup {
    Remove-HomePath $fx.HostsFile
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdBox)) })
  }
  Test-HostCase "${e}: status -All goes on past a failed host: its failure line follows its Host line, no report is printed for it, the next hosts are reported, then agentspace: 1 of 6 hosts failed., exit 1" @('status', '-All') @{
    Exit = 1
    Stderr = (Join-Lines @(
        (Get-HostLine 'swimcoach' 'ready'), (Get-StatusLines $idBox).TrimEnd("`n"),
        (Get-HostLine 'agentspace' 'ready'), "agentspace: $denied",
        (Get-HostLine 'gamma' 'creating'), 'Instance  none recorded',
        (Get-HostLine 'delta' 'failed'), 'Instance  none recorded',
        (Get-HostLine 'epsilon' 'ready'), (Get-StatusLines $idC).TrimEnd("`n"),
        (Get-HostLine 'zeta' 'creating'), (Get-StatusLines $idB).TrimEnd("`n"),
        'agentspace: 1 of 6 hosts failed.'))
    Calls = @(
      (Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox),
      (Get-CallDescribe $idA), (Get-CallVolumes $idA),
      (Get-CallStack 'agentspace-host-epsilon'), (Get-CallDescribe $idC), (Get-CallVolumes $idC), (Get-CallSsm $idC),
      (Get-CallDescribe $idB), (Get-CallVolumes $idB), (Get-CallSsm $idB)) } -Setup {
    & $setupAll
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB) (Get-HealthyAnswers -Id $script:IdC) @{
        'cloudformation describe-stacks agentspace-host-epsilon' = @(New-Step (New-StackDoc -StackName 'agentspace-host-epsilon' -Id $script:IdC))
        "ec2 describe-volumes $(Get-VolumeOf $script:IdA)" = @(New-Step '' $denied 254)
      })
  }
  Test-HostCase "${e}: a ready host whose stack has no InstanceId output fails its part alone: its failure line, the report goes on, 1 of 6 hosts failed" @('status', '-All') @{
    Exit = 1; StderrLines = @((Get-HostLine 'epsilon' 'ready'), "agentspace: Stack 'agentspace-host-epsilon' has no InstanceId output.", (Get-HostLine 'zeta' 'creating'), ('Instance  ' + $idB), 'agentspace: 1 of 6 hosts failed.')
    Check = { param($run, $started)
      & $registryIs $records 'the six records as they were: nothing was written for the failed host'
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      if ($lines[-1] -cne 'agentspace: 1 of 6 hosts failed.') { 'The last line is not the closing count' } } } -Setup {
    & $setupAll
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB) @{
        'cloudformation describe-stacks agentspace-host-epsilon' = @(New-Step (New-StackDoc -StackName 'agentspace-host-epsilon'))
      })
  }
  # #0034's sign-in message "ends the command at once" (operator.md, "Every host at once: -All"; "Exit
  # codes"), so it is the last line of stderr, with no closing count after it.
  Test-HostCase "${e}: #0034's sign-in message ends status -All at once: it is the last line of stderr, with no closing count, no later host is reported, and its calls are not made" @('status', '-All') @{
    Exit = 1
    Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox), (Get-CallDescribe $idA))
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      if ($lines[-1] -cne $signIn) { 'The sign-in message is not the last line of stderr' }
      if ($lines | Where-Object { $_ -match ' hosts failed\.$' }) { 'A closing count was printed after the sign-in failure' }
      if ($lines -contains (Get-HostLine 'gamma' 'creating')) { 'A host after the sign-in failure was reported' }
      if (-not ($lines -contains (Get-HostLine 'agentspace' 'ready'))) { 'The host whose call failed has no Host line' }
    } } -Setup {
    & $setupAll
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA) @{
        "ec2 describe-instances $script:IdA" = @(New-Step '' 'Unable to locate credentials. You can configure credentials by running "aws login".' 255)
      })
  }

  # ============ down -All ============
  $s = 'operator.md#down'
  $downAll = Join-Lines @(
    (Get-HostLine 'swimcoach' 'ready'), "Stopping $idBox.",
    (Get-HostLine 'agentspace' 'ready'), "Stopping $idA.",
    (Get-HostLine 'gamma' 'creating'), 'Skipping: no instance is recorded for this host.',
    (Get-HostLine 'delta' 'failed'), 'Skipping: no instance is recorded for this host.',
    (Get-HostLine 'epsilon' 'ready'), "Stopping $idC.",
    (Get-HostLine 'zeta' 'creating'), "Stopping $idB.")
  $downCalls = @((Get-CallStop $idBox), (Get-CallStop $idA), (Get-CallStack 'agentspace-host-epsilon'), (Get-CallStop $idC), (Get-CallStop $idB))
  Test-HostCase "${s}: down -All stops every host whose record holds an instance ID or is ready, in the file's order, after each Host line; a creating or failed host with none is skipped with its line, and a ready host's ID is read from its stack" @('down', '-All') @{
    Exit = 0; Stderr = $downAll; Calls = $downCalls
    Check = { param($run, $started) & $registryIs $afterEpsilon 'the six records as they were, with the instance ID written into epsilon''s alone'; & $nothingSigned $run $started } } -Setup $setupAll
  Test-HostCase "${s}: down -All -Quiet keeps the Host lines and drops the progress lines, and still stops each host" @('down', '-All', '-Quiet') @{
    Exit = 0
    Stderr = (Join-Lines @((Get-HostLine 'swimcoach' 'ready'), (Get-HostLine 'agentspace' 'ready'), (Get-HostLine 'gamma' 'creating'), (Get-HostLine 'delta' 'failed'), (Get-HostLine 'epsilon' 'ready'), (Get-HostLine 'zeta' 'creating')))
    Calls = $downCalls } -Setup $setupAll
  Test-HostCase "${s}: down -All signs nothing and writes no alias, and needs no CA: with the home's CA moved away the stops are requested and it exits 0" @('down', '-All') @{
    Exit = 0; Stderr = $downAll; Calls = $downCalls; Check = $nothingSigned } -Setup {
    & $setupAll
    Hide-Ca
  }
  Test-HostCase "${s}: down -All of a missing registry stops the box alone, reading its instance ID from the agentbox stack and writing the box's record" @('down', '-All') @{
    Exit = 0; Stderr = (Join-Lines @((Get-HostLine 'swimcoach' 'ready'), "Stopping $idBox."))
    Calls = @((Get-CallStack 'agentbox'), (Get-CallStop $idBox))
    Check = { param($run, $started) & $registryIs @((New-BoxRecord $script:IdBox)) 'the box''s record holding the instance ID' } } -Setup {
    Remove-HomePath $fx.HostsFile
    Set-AwsScenario @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdBox)) }
  }
  Test-HostCase "${e}: down -All goes on past a failed host and ends with agentspace: 1 of 6 hosts failed., exit 1" @('down', '-All') @{
    Exit = 1
    Stderr = (Join-Lines @(
        (Get-HostLine 'swimcoach' 'ready'), "Stopping $idBox.",
        (Get-HostLine 'agentspace' 'ready'), "agentspace: $stopDenied",
        (Get-HostLine 'gamma' 'creating'), 'Skipping: no instance is recorded for this host.',
        (Get-HostLine 'delta' 'failed'), 'Skipping: no instance is recorded for this host.',
        (Get-HostLine 'epsilon' 'ready'), "Stopping $idC.",
        (Get-HostLine 'zeta' 'creating'), "Stopping $idB.",
        'agentspace: 1 of 6 hosts failed.'))
    Calls = $downCalls } -Setup {
    & $setupAll
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) @{
        'cloudformation describe-stacks agentspace-host-epsilon' = @(New-Step (New-StackDoc -StackName 'agentspace-host-epsilon' -Id $script:IdC))
        "ec2 stop-instances $script:IdA" = @(New-Step '' $stopDenied 254)
      })
  }
  Test-HostCase "${e}: down -All with every stop failing counts every record that failed, and a skipped host is not a failure: 4 of 6 hosts failed" @('down', '-All') @{
    Exit = 1; Calls = $downCalls
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      if ($lines[-1] -cne 'agentspace: 4 of 6 hosts failed.') { 'The last line is not agentspace: 4 of 6 hosts failed.' } } } -Setup {
    & $setupAll
    Set-AwsScenario @{
      'cloudformation describe-stacks agentspace-host-epsilon' = @(New-Step (New-StackDoc -StackName 'agentspace-host-epsilon' -Id $script:IdC))
      'ec2 stop-instances' = @(New-Step '' $stopDenied 254)
    }
  }
  Test-HostCase "${e}: #0034's sign-in message ends down -All at once: it is the last line of stderr, with no closing count, and no later host is touched" @('down', '-All') @{
    Exit = 1; Calls = @((Get-CallStop $idBox), (Get-CallStop $idA))
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      if ($lines[-1] -cne $signIn) { 'The sign-in message is not the last line of stderr' }
      if ($lines | Where-Object { $_ -match ' hosts failed\.$' }) { 'A closing count was printed after the sign-in failure' }
      if ($lines -contains (Get-HostLine 'gamma' 'creating')) { 'A host after the sign-in failure was reached' }
    } } -Setup {
    & $setupAll
    Set-AwsScenario @{ "ec2 stop-instances $script:IdA" = @(New-Step '' 'Error loading SSO Token: run aws login to refresh.' 255) }
  }
}
Exit-ConformanceRun

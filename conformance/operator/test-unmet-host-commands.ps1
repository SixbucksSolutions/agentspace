#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `down`, `status` and `grow` given -Host, of every command with and without
# -Host acting on the box, of the host registry as the operator uses it, and of the operator's key,
# certificate and alias that every command given -Host for a host that is not the box makes
# (docs/operator.md, "Which host a command acts on", "From the command line", "The host registry, as
# the operator uses it", "How the operator reaches a host", "down", "status", "grow", "Starting the box
# and waiting for it", "Laptop files"; docs/state.md, "The host registry", "The instance ID", "A host's
# folder", "The host registry lock"; D45, D47, D49, D55). Card #0156 (E0.07-T04-S02).
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that
# log every call and answer from files beside them (conformance/operator/host-commands-common.ps1), a
# host registry the case writes, and the home's throwaway CA. Each case checks the AWS CLI calls in order
# with their arguments, the messages on stderr, the exit code, stdout, the registry's bytes, and what was
# written for a host: the key, the certificate, the alias file and the Include line.
#
# Cases: -Host picking its host's stack, instance ID and alias for down, status and grow; a record acted
# on whatever its state; -Host swimcoach the same as no -Host for every command, and nothing written for
# the box; a leftover instance-id.json never read; the registry (missing read as the box's record and
# written with the first change, not valid failing every command with its name, a missing instance ID read
# from the host's stack and written into its record alone, a registry that cannot be written);
# hosts.lock held past its 30 seconds, and held while a command whose key exists needs none; the key,
# the certificate and the alias made by every command given -Host, `status` and `down` after their own
# work; the Include line that cannot be added, worded the same by `sync -Host` and `status -Host`; and
# the start-and-wait step for a host that is not the box through `sync -Host`.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. The operator's key and certificate are made by
# the real ssh-keygen.exe against the home's throwaway CA, which signs nothing real and reaches no host
# (docs/operator.md, "The test seam"). PATH holds no go. Every instance and volume ID is made up, and a
# failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-commands.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$caMissing = '^agentspace: .*agentspace_ca does not exist; run ca\.$'

Invoke-HostFile -FileName 'test-unmet-host-commands.ps1' -Body {
  param($fx)
  $idA = $script:IdA; $idBox = $script:IdBox; $idNew = $script:IdNew
  $stackA = 'agentspace-host-agentspace'
  # The registry and the answers of a healthy second host: the box and 'agentspace', each with its
  # instance ID, each instance running and launched long ago.
  $setupTwo = {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario ((Get-HealthyAnswers -Id $script:IdA) + (Get-HealthyAnswers -Id $script:IdBox))
  }
  $regTwo = Get-HostsBytes @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $idA))
  $unchangedTwo = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $regTwo)) { 'hosts.json was changed' } }
  # No alias and no certificate existed when the call arrived: the call came before the signing.
  $unsignedAt = {
    param([int]$Index, [string]$What)
    $calls = @(Get-AwsCalls)
    if ($calls.Count -le $Index) { return "There is no AWS call $($Index + 1) to look at" }
    if (@($calls[$Index].aliases).Count -gt 0 -or @($calls[$Index].certs).Count -gt 0) { "An alias or a certificate existed already at $What" }
  }

  # ============ down -Host ============
  $s = 'operator.md#down'
  Test-HostCase "${s}: down -Host <project> stops that host's instance, prints Stopping <id>., and signs after the stop is requested" @('down', '-Host', 'agentspace') @{
    Exit = 0; Stderr = "Stopping $idA.`n"; Calls = @(Get-CallStop $idA)
    Check = { param($run, $started) & $unsignedAt 0 'the stop request'; Get-SignProblems 'agentspace'; & $unchangedTwo $run $started } } -Setup $setupTwo
  Test-HostCase "${s}: down -Host <project> -Quiet prints nothing, stops the instance and still signs" @('down', '-Host', 'agentspace', '-Quiet') @{
    Exit = 0; Stderr = ''; Calls = @(Get-CallStop $idA)
    Check = { param($run, $started) Get-SignProblems 'agentspace' } } -Setup $setupTwo
  Test-HostCase "${s}: a failed signing fails down, exit 1, and the stop already requested stands (the CA is missing)" @('down', '-Host', 'agentspace') @{
    Exit = 1; StderrLines = @("Stopping $idA.", "re:$caMissing"); Calls = @(Get-CallStop $idA)
    Check = { param($run, $started)
      if (@($run.Stderr -split "`n" | Where-Object { $_ -ne '' }).Count -ne 2) { 'Stderr is not the Stopping line and the failure line' }
      if (Test-Path -LiteralPath (Get-HostFiles 'agentspace').Alias) { 'An alias was written with no usable CA' } } } -Setup {
    & $setupTwo
    Hide-Ca
  }
  Test-HostCase "${s}: down -Host <project> asks for a failed stop with the AWS CLI's text, and signs nothing" @('down', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "agentspace: An error occurred (UnauthorizedOperation) when calling the StopInstances operation: You are not authorized to perform this operation.`n"; Calls = @(Get-CallStop $idA)
    Check = { param($run, $started) foreach ($m in (Get-NothingSignedProblems)) { $m } } } -Setup {
    & $setupTwo
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) @{ "ec2 stop-instances $script:IdA" = @(New-Step '' 'An error occurred (UnauthorizedOperation) when calling the StopInstances operation: You are not authorized to perform this operation.' 254) })
  }

  # ============ status -Host ============
  $s = 'operator.md#status'
  Test-HostCase "${s}: status -Host <project> prints the five lines of that host's instance, from its record, and signs after the report" @('status', '-Host', 'agentspace') @{
    Exit = 0; Stderr = (Get-StatusLines $idA); Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA))
    Check = { param($run, $started) & $unsignedAt 2 'the last AWS call'; Get-SignProblems 'agentspace'; & $unchangedTwo $run $started } } -Setup $setupTwo
  Test-HostCase "${s}: status -Host <project> -Quiet still prints the five lines, and signs" @('status', '-Host', 'agentspace', '-Quiet') @{
    Exit = 0; Stderr = (Get-StatusLines $idA); Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA))
    Check = { param($run, $started) Get-SignProblems 'agentspace' } } -Setup $setupTwo
  Test-HostCase "${s}: status -Host <project> of a stopped host prints SSM - and asks Session Manager nothing" @('status', '-Host', 'agentspace') @{
    Exit = 0; Stderr = (Get-StatusLines $idA -State 'stopped' -Ssm '-'); Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA)) } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Get-HealthyAnswers -Id $script:IdA -State 'stopped')
  }
  Test-HostCase "${s}: a failed signing fails status after its report, exit 1, with the failure as the last line (the CA is missing)" @('status', '-Host', 'agentspace') @{
    Exit = 1; Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA))
    Check = { param($run, $started)
      $text = Get-StatusLines $idA
      if (-not $run.Stderr.StartsWith($text)) { 'The five lines are not printed first' }
      $rest = @($run.Stderr.Substring([Math]::Min($text.Length, $run.Stderr.Length)) -split "`n" | Where-Object { $_ -ne '' })
      if ($rest.Count -ne 1 -or $rest[0] -cnotmatch $caMissing) { 'The failure of the CA check is not the one line after the report' }
    } } -Setup {
    & $setupTwo
    Hide-Ca
  }
  Test-HostCase "${s}: a failure of status's AWS calls signs nothing: a failed describe-volumes" @('status', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "agentspace: An error occurred (UnauthorizedOperation) when calling the DescribeVolumes operation: no`n"; Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA))
    Check = { param($run, $started) foreach ($m in (Get-NothingSignedProblems)) { $m } } } -Setup {
    & $setupTwo
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) @{ "ec2 describe-volumes $(Get-VolumeOf $script:IdA)" = @(New-Step '' 'An error occurred (UnauthorizedOperation) when calling the DescribeVolumes operation: no' 254) })
  }

  # ============ grow -Host ============
  $s = 'operator.md#grow'
  $growMsg = { param([string]$Id, [long]$From, [long]$To) "Growing $(Get-VolumeOf $Id) from $From to $To GiB (EBS allows one change per volume every 6 hours)..." }
  Test-HostCase "${s}: grow -Host <project> of a stopped host reads and changes that host's volume, prints the next-boot line, starts no ssh, and signs after step 2" @('grow', '-Host', 'agentspace') @{
    Exit = 0; Stderr = (Join-Lines @((& $growMsg $idA 20 22), 'The filesystem grows on its own at next boot.'))
    Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallModify $idA 22))
    Check = { param($run, $started) & $unsignedAt 2 'the volume change'; Get-SignProblems 'agentspace' } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Get-HealthyAnswers -Id $script:IdA -State 'stopped')
  }
  Test-HostCase "${s}: grow -Host <project> -GiB 4 grows by 4" @('grow', '-Host', 'agentspace', '-GiB', '4') @{
    Exit = 0; Stderr = (Join-Lines @((& $growMsg $idA 20 24), 'The filesystem grows on its own at next boot.'))
    Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallModify $idA 24)) } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Get-HealthyAnswers -Id $script:IdA -State 'stopped')
  }
  Test-HostCase "${s}: grow -Host <project> of a running host waits 10 seconds, then starts one ssh through the host's own alias, after the key, the certificate and the alias exist" @('grow', '-Host', 'agentspace') @{
    Exit = 0; StderrLines = @((& $growMsg $idA 20 22), 'Growing the filesystem online...')
    Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallModify $idA 22))
    SshCalls = @(, @('-o', 'ClearAllForwardings=yes', 'agentspace-host.agentspace', 're:.'))
    Check = { param($run, $started)
      Get-SignProblems 'agentspace'
      $ssh = @(Get-SshCalls)
      $aws = @(Get-AwsCalls)
      if ($ssh.Count -eq 1) {
        if (@($ssh[0].aliases) -cnotcontains 'agentspace-host.agentspace' -or @($ssh[0].certs) -cnotcontains 'agentspace') { 'The ssh started before the alias and the certificate were written' }
        if ($aws.Count -eq 3 -and (([long]$ssh[0].t - [long]$aws[2].t) / 1e7) -lt 9) { 'The ssh started less than 10 seconds after the volume change' }
      }
    } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Get-HealthyAnswers -Id $script:IdA)
  }

  # ============ Every command without -Host acts on the box, and -Host swimcoach is the same ============
  $b = 'operator.md#from-the-command-line'
  # Runs a command as no -Host, -Host swimcoach and -Host Swimcoach, and compares: the exit code, the
  # streams, every AWS CLI, ssh and scp call (the operator's folder's name apart), the host registry;
  # then checks the box's own instance and stack, the alias `agentspace` and nothing written for a host.
  function Test-SameAsBox {
    param([string]$Name, [string[]]$Command, [int]$Exit, [scriptblock]$Setup, [string]$WantCall)
    try {
      $verb = $Command[0]
      $rest = @($Command | Select-Object -Skip 1)
      $variants = @((@($verb) + $rest), (@($verb, '-Host', 'swimcoach') + $rest), (@($verb, '-Host', 'Swimcoach') + $rest))
      $norm = { param($t) ([string]$t) -replace '[0-9a-f]{32}', '<hex>' }
      $runs = @()
      foreach ($v in $variants) {
        Reset-HostCase
        & $Setup
        $r = Invoke-HostRun -Arguments $v
        $runs += [pscustomobject]@{
          Run = $r
          Aws = @(@(Get-AwsCalls) | ForEach-Object { & $norm $_.Text })
          Ssh = @(@(Get-SshCalls) | ForEach-Object { & $norm $_.Text })
          Scp = @(@(Get-ScpCalls) | ForEach-Object { & $norm $_.Text })
          Registry = [Convert]::ToBase64String([byte[]](Get-FileBytesOrNull $fx.HostsFile))
          Unsigned = @(Get-NothingSignedProblems)
        }
      }
      $p = [System.Collections.Generic.List[string]]::new()
      $first = $runs[0]
      if ($first.Run.TimedOut) { $p.Add('It timed out') } elseif ($first.Run.ExitCode -ne $Exit) { $p.Add("Exit $($first.Run.ExitCode), not $Exit") }
      if (-not ($first.Aws -join "`n").Contains($WantCall)) { $p.Add("The AWS CLI calls do not hold the box's call '$WantCall'") }
      $all = ($first.Aws + $first.Ssh + $first.Scp) -join "`n"
      if ($all -match 'agentspace-host' -or $all.Contains($script:IdA)) { $p.Add('A call names another host (its alias, stack or instance)') }
      foreach ($i in 0..2) {
        $r2 = $runs[$i]
        if ($r2.Unsigned.Count -gt 0) { $p.Add("Variant $($i + 1) wrote something for a host: $($r2.Unsigned -join ' / ')") }
        if ($i -gt 0) {
          if ($r2.Run.ExitCode -ne $first.Run.ExitCode) { $p.Add("Variant $($i + 1) exited $($r2.Run.ExitCode), not $($first.Run.ExitCode)") }
          if ((& $norm $r2.Run.Stderr) -cne (& $norm $first.Run.Stderr)) { $p.Add("Variant $($i + 1)'s stderr differs from no -Host's") }
          if ($r2.Run.Stdout -cne $first.Run.Stdout) { $p.Add("Variant $($i + 1)'s stdout differs from no -Host's") }
          if (($r2.Aws -join "`n") -cne ($first.Aws -join "`n")) { $p.Add("Variant $($i + 1)'s AWS CLI calls differ from no -Host's") }
          if (($r2.Ssh -join "`n") -cne ($first.Ssh -join "`n")) { $p.Add("Variant $($i + 1)'s ssh calls differ from no -Host's") }
          if (($r2.Scp -join "`n") -cne ($first.Scp -join "`n")) { $p.Add("Variant $($i + 1)'s scp calls differ from no -Host's") }
          if ($r2.Registry -cne $first.Registry) { $p.Add("Variant $($i + 1) left another registry than no -Host") }
        }
      }
      if ($p.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS } else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($p -join '; ') }
    } catch {
      $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
      Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
    }
  }
  $twoHealthy = {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario ((Get-HealthyAnswers -Id $script:IdA) + (Get-HealthyAnswers -Id $script:IdBox))
  }
  $twoStopped = {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario ((Get-HealthyAnswers -Id $script:IdA -State 'stopped') + (Get-HealthyAnswers -Id $script:IdBox -State 'stopped'))
  }
  $twoNoOutput = {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox')) }
  }
  Test-SameAsBox "${b}: down, -Host swimcoach and -Host Swimcoach act on the box alike: its instance, no key, certificate or alias" @('down') 0 $twoHealthy (Get-CallStop $idBox)
  Test-SameAsBox "${b}: status, -Host swimcoach and -Host Swimcoach act on the box alike" @('status') 0 $twoHealthy (Get-CallDescribe $idBox)
  Test-SameAsBox "${b}: grow of a stopped box, -Host swimcoach and -Host Swimcoach act on the box alike" @('grow') 0 $twoStopped (Get-CallModify $idBox 22)
  Test-SameAsBox "${b}: up, -Host swimcoach and -Host Swimcoach act on the box alike" @('up') 0 $twoHealthy (Get-CallDescribe $idBox)
  Test-SameAsBox "${b}: sync, -Host swimcoach and -Host Swimcoach act on the box alike, every ssh and scp through the alias agentspace" @('sync') 0 $twoHealthy (Get-CallDescribe $idBox)
  Test-SameAsBox "${b}: bootstrap, -Host swimcoach and -Host Swimcoach act on the box alike" @('bootstrap') 0 $twoHealthy (Get-CallDescribe $idBox)
  Test-SameAsBox "${b}: deploy, -Host swimcoach and -Host Swimcoach act on the box alike: the agentbox stack, with no existence check" @('deploy') 1 $twoNoOutput (Get-CallStack 'agentbox')
  Test-HostCase "${b}: the box's ssh calls name the alias agentspace, and no other alias (sync, no -Host)" @('sync') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $ssh = @(Get-SshCalls); $scp = @(Get-ScpCalls)
      if ($ssh.Count -lt 2 -or $scp.Count -lt 2) { 'sync started too few ssh and scp calls to show the alias' }
      foreach ($c in $ssh) { if (@($c.a) -cnotcontains 'agentspace') { 'An ssh call does not name the alias agentspace' } }
      foreach ($c in $scp) { if (@($c.a | Where-Object { $_ -cmatch '^agentspace:' }).Count -ne 1) { 'An scp call does not name the alias agentspace' } }
    } } -Setup $twoHealthy

  # ============ A leftover instance-id.json is ignored ============
  $ii = 'operator.md#laptop-files'
  $retiredText = '{"version":0,"instance_id":"i-abcdef1234567890a"}'
  $retiredBytes = [Text.Encoding]::UTF8.GetBytes($retiredText)
  $retiredUnchanged = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.RetiredFile) $retiredBytes)) { 'The retired instance-id.json was changed, removed or made' } }
  Test-HostCase "${ii}: a record that holds its instance ID is read, never the retired file, and the file is left as it was (down -Host)" @('down', '-Host', 'agentspace') @{
    Exit = 0; Stderr = "Stopping $idA.`n"; Calls = @(Get-CallStop $idA); Check = $retiredUnchanged } -Setup {
    & $setupTwo
    [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.RetiredFile), $retiredBytes)
  }
  Test-HostCase "${ii}: a record with no instance ID has it read from its stack, never from the retired file, which is left as it was (down -Host)" @('down', '-Host', 'agentspace') @{
    Exit = 0; Stderr = "Stopping $idNew.`n"; Calls = @((Get-CallStack $stackA), (Get-CallStop $idNew)); Check = $retiredUnchanged } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))
    Set-AwsScenario @{ "cloudformation describe-stacks $stackA" = @(New-Step (New-StackDoc -StackName $stackA -Id $script:IdNew)) }
    [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.RetiredFile), $retiredBytes)
  }
  Test-HostCase "${ii}: the retired file is never the box's instance ID either (down -Host swimcoach, the box's record with none)" @('down', '-Host', 'swimcoach') @{
    Exit = 0; Stderr = "Stopping $idNew.`n"; Calls = @((Get-CallStack 'agentbox'), (Get-CallStop $idNew)); Check = $retiredUnchanged } -Setup {
    Set-HostsRecords @((New-HostRecord -Project 'swimcoach'))
    Set-AwsScenario @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdNew)) }
    [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.RetiredFile), $retiredBytes)
  }
  Test-HostCase "${ii}: status -Host reads its host's instance ID from its record, and leaves the retired file as it was" @('status', '-Host', 'agentspace') @{
    Exit = 0; Stderr = (Get-StatusLines $idA); Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA)); Check = $retiredUnchanged } -Setup {
    & $setupTwo
    [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.RetiredFile), $retiredBytes)
  }

  # ============ The host registry ============
  $hr = 'operator.md#the-host-registry-as-the-operator-uses-it'
  # A registry that is not valid fails every command given -Host or -All, with its name, before any
  # program starts or any AWS call is made, and is left as it is.
  $registryFailure = {
    param($run, $started)
    $m = [regex]::Match($run.Stderr, '^agentspace: The host registry (.+?) cannot be read: (.+)\.\n$')
    if (-not $m.Success) { return 'Stderr is not agentspace: The host registry <path> cannot be read: <reason>.' }
    if ($m.Groups[1].Value -ine $fx.HostsFile -and $m.Groups[1].Value -ine (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsFile)) { 'The path in the message is not the registry''s full path' }
    if ($m.Groups[2].Value.Trim() -eq '') { 'The message gives no reason' }
  }
  $badText = 'not json {'
  $noBoxText = '{"version":0,"hosts":[{"project":"agentspace","stack":"agentspace-host-agentspace","state":"ready"}]}'
  foreach ($kind in @(@('is not JSON', $badText), @('holds no record whose stack is agentbox', $noBoxText))) {
    $text = $kind[1]
    foreach ($line in @(
        @('deploy', '-Host', 'agentspace'), @('up', '-Host', 'agentspace'), @('sync', '-Host', 'agentspace'), @('bootstrap', '-Host', 'agentspace'),
        @('down', '-Host', 'agentspace'), @('status', '-Host', 'agentspace'), @('grow', '-Host', 'agentspace'),
        @('status', '-Host', 'swimcoach'), @('down', '-Host', 'swimcoach'),
        @('status', '-All'), @('down', '-All'), @('sync', '-All'))) {
      $textNow = $text
      Test-HostCase "${hr}: $($line -join ' ') with a registry that $($kind[0]) fails with the registry's message, before any program starts, and leaves the file as it was" ([string[]]$line) @{
        Exit = 1; Calls = @()
        Check = { param($run, $started)
          & $registryFailure $run $started
          if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) ([Text.Encoding]::UTF8.GetBytes($textNow)))) { 'The registry was repaired or changed' }
          foreach ($m in (Get-NothingSignedProblems)) { $m }
        } } -Setup {
        Set-HostsFile ([Text.Encoding]::UTF8.GetBytes($textNow))
      }
    }
  }
  Test-HostCase "${hr}: a missing registry reads as the box's record alone: down -Host swimcoach reads the box's stack, writes hosts.json holding the box's record with its instance ID, then stops it" @('down', '-Host', 'swimcoach') @{
    Exit = 0; Stderr = "Stopping $idNew.`n"; Calls = @((Get-CallStack 'agentbox'), (Get-CallStop $idNew))
    Check = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord $script:IdNew))))) { 'hosts.json is not the box''s record holding the stack''s InstanceId output, compact, with one LF' } } } -Setup {
    Remove-HomePath $fx.HostsFile
    Set-AwsScenario @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdNew)) }
  }
  Test-HostCase "${hr}: a missing registry: status -Host swimcoach reads the stack, writes the box's record, then reports" @('status', '-Host', 'swimcoach') @{
    Exit = 0; Stderr = (Get-StatusLines $idNew); Calls = @((Get-CallStack 'agentbox'), (Get-CallDescribe $idNew), (Get-CallVolumes $idNew), (Get-CallSsm $idNew))
    Check = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord $script:IdNew))))) { 'hosts.json is not the box''s record holding the instance ID' } } } -Setup {
    Remove-HomePath $fx.HostsFile
    Set-AwsScenario ((Get-HealthyAnswers -Id $script:IdNew) + @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdNew)) })
  }
  Test-HostCase "${hr}: a missing registry: a command that fails before any change writes none (the box's stack has no InstanceId output)" @('down', '-Host', 'swimcoach') @{
    Exit = 1; Stderr = "agentspace: Stack 'agentbox' has no InstanceId output.`n"; Calls = @(Get-CallStack 'agentbox')
    Check = { param($run, $started) if (Test-Path -LiteralPath $fx.HostsFile) { 'A hosts.json was written' } } } -Setup {
    Remove-HomePath $fx.HostsFile
    Set-AwsScenario @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox')) }
  }
  # A missing instance ID is read from the host's own stack and written into its record, changing
  # nothing else: the box's record, the other members and the order of the records stay as they were.
  $creatingTime = '2026-01-01T00:00:00Z'
  $thirdRecord = New-HostRecord -Project 'other' -State 'creating' -LastAttempt $creatingTime
  foreach ($case in @(
      @('down', @('down', '-Host', 'agentspace'), 0, (Join-Lines @("Stopping $idNew."))),
      @('status', @('status', '-Host', 'agentspace'), 0, (Get-StatusLines $idNew)),
      @('grow of a stopped host', @('grow', '-Host', 'agentspace'), 0, $null)
    )) {
    $stopped = $case[0] -like 'grow*'
    Test-HostCase "${hr}: $($case[0]) -Host <project> with no instance ID in its record reads its stack's InstanceId output, writes it into that record alone, and goes on with it" ([string[]]$case[1]) @{
      Exit = $case[2]; Calls = 'any'
      Check = {
        param($run, $started)
        $calls = @(Get-AwsCalls)
        if ($calls.Count -lt 2 -or $calls[0].Text -cne (Get-CallStack $stackA)) { 'The first AWS call is not describe-stacks of the host''s own stack' }
        elseif ($calls[1].Text -notmatch [regex]::Escape($script:IdNew)) { 'The command did not go on with the instance ID the stack gave' }
        if (@($calls | Where-Object { $_.Text -match [regex]::Escape($script:IdBox) -or $_.Text -match 'stack-name agentbox' }).Count -gt 0) { 'A call names the box' }
        if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -InstanceId $script:IdNew -LastAttempt $creatingTime), $thirdRecord)))) {
          'hosts.json is not the three records as they were, with the instance ID written into the host''s own'
        }
      } } -Setup {
      Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -LastAttempt $creatingTime), $thirdRecord)
      Set-AwsScenario ((Get-HealthyAnswers -Id $script:IdNew -State $(if ($stopped) { 'stopped' } else { 'running' })) + @{ "cloudformation describe-stacks $stackA" = @(New-Step (New-StackDoc -StackName $stackA -Id $script:IdNew)) })
    }
  }
  Test-HostCase "${hr}: a record that holds its instance ID is read without a call to the stack, and without a write (status -Host)" @('status', '-Host', 'agentspace') @{
    Exit = 0; Stderr = (Get-StatusLines $idA); Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA)); Check = $unchangedTwo } -Setup $setupTwo
  Test-HostCase "${hr}: a stack with no InstanceId output fails with Stack '<stack>' has no InstanceId output. and writes nothing (the host's own stack)" @('status', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "agentspace: Stack '$stackA' has no InstanceId output.`n"; Calls = @(Get-CallStack $stackA)
    Check = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))))) { 'hosts.json was changed' } } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))
    Set-AwsScenario @{ "cloudformation describe-stacks $stackA" = @(New-Step (New-StackDoc -StackName $stackA)) }
  }
  Test-HostCase "${hr}: a stack that does not exist fails with the AWS CLI's own text, and writes nothing" @('down', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "agentspace: An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $stackA does not exist`n"; Calls = @(Get-CallStack $stackA)
    Check = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))))) { 'hosts.json was changed' } } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))
    Set-AwsScenario @{ "cloudformation describe-stacks $stackA" = @(New-Step '' "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $stackA does not exist" 254) }
  }
  Test-HostCase "${hr}: a registry that cannot be written fails with Cannot write <path>., stops nothing and leaves the file as it was (a read-only hosts.json)" @('down', '-Host', 'agentspace') @{
    Exit = 1; Calls = @((Get-CallStack $stackA))
    Check = { param($run, $started)
      $m = [regex]::Match($run.Stderr, '^agentspace: Cannot write (.+)\.\n$')
      if (-not $m.Success) { 'Stderr is not agentspace: Cannot write <path>.' }
      elseif ($m.Groups[1].Value -ine $fx.HostsFile -and $m.Groups[1].Value -ine (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsFile)) { 'The path is not the registry''s' }
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))))) { 'hosts.json was changed' }
      # The file is made writable again, so the home can be removed.
      (Get-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsFile)).IsReadOnly = $false
    } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))
    Set-AwsScenario @{ "cloudformation describe-stacks $stackA" = @(New-Step (New-StackDoc -StackName $stackA -Id $script:IdA)) }
    (Get-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsFile)).IsReadOnly = $true
  }

  # ============ hosts.lock ============
  $lk = 'state.md#the-host-registry-lock'
  Test-HostCase "${lk}: hosts.lock held past its 30 seconds: a command that must write the registry fails with the lock's message after about 30 seconds, and writes nothing (status -Host, no instance ID in its record)" @('status', '-Host', 'agentspace') @{
    Exit = 1; Calls = @(Get-CallStack $stackA)
    Check = { param($run, $started)
      $m = [regex]::Match($run.Stderr, '^agentspace: The host registry''s lock (.+) is held by another program; try again\.\n$')
      if (-not $m.Success) { 'Stderr is not agentspace: The host registry''s lock <path> is held by another program; try again.' }
      elseif ($m.Groups[1].Value -ine $fx.HostsLock -and $m.Groups[1].Value -ine (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsLock)) { 'The path in the message is not the lock''s' }
      if ($run.Elapsed -lt 28) { "The command gave up after $([int]$run.Elapsed) seconds, before the 30 of the lock's rule" }
      if ($run.Elapsed -gt 60) { "The command waited $([int]$run.Elapsed) seconds, more than the 30 the lock's rule allows" }
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))))) { 'hosts.json was changed' }
      Close-HostsLock
    } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace'))
    Set-AwsScenario ((Get-HealthyAnswers -Id $script:IdA) + @{ "cloudformation describe-stacks $stackA" = @(New-Step (New-StackDoc -StackName $stackA -Id $script:IdA)) })
    Hold-HostsLock
  } -Timeout 150
  # Q10 (architect, 2026-11-21, from T03): a held hosts.lock does not delay a -Host command whose key
  # exists, since the key is made under the lock only when it is missing and the record holds its ID.
  foreach ($case in @(
      @('status', @('status', '-Host', 'agentspace'), (Get-StatusLines $idA)),
      @('down', @('down', '-Host', 'agentspace'), "Stopping $idA.`n")
    )) {
    $want = $case[2]
    Test-HostCase "${lk}: a held hosts.lock does not delay $($case[0]) -Host <project> when the operator's key exists and the record holds its instance ID" ([string[]]$case[1]) @{
      Exit = 0; Stderr = $want; Calls = 'any'
      Check = { param($run, $started)
        if ($run.Elapsed -gt 20) { "The command took $([int]$run.Elapsed) seconds with hosts.lock held" }
        Get-SignProblems 'agentspace'
        Close-HostsLock
      } } -Setup {
      & $setupTwo
      $null = Invoke-HostRun -Arguments @('status', '-Host', 'agentspace')
      Clear-StandinLogs
      Set-AwsScenario ((Get-HealthyAnswers -Id $script:IdA) + (Get-HealthyAnswers -Id $script:IdBox))
      Hold-HostsLock
    } -Timeout 90
  }

  # ============ The key, the certificate and the alias, for every command given -Host ============
  $sg = 'operator.md#how-the-operator-reaches-a-host'
  # Each command is run once with the key made and a stale certificate in place; afterwards the key is
  # the same bytes, the certificate is a new, valid one of the host's key ID, and the alias and the
  # Include line are there.
  $signCommands = @(
    @('down', @('down', '-Host', 'agentspace'), $setupTwo),
    @('status', @('status', '-Host', 'agentspace'), $setupTwo),
    @('grow of a stopped host', @('grow', '-Host', 'agentspace'), { Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA)); Set-AwsScenario (Get-HealthyAnswers -Id $script:IdA -State 'stopped') }),
    @('up', @('up', '-Host', 'agentspace'), $setupTwo),
    @('sync', @('sync', '-Host', 'agentspace'), $setupTwo),
    @('bootstrap', @('bootstrap', '-Host', 'agentspace'), $setupTwo),
    @('deploy of an existing stack', @('deploy', '-Host', 'agentspace'), {
        Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
        Set-AwsScenario ((Get-HealthyAnswers -Id $script:IdA) + @{ "cloudformation describe-stacks $stackA" = @(New-Step (New-StackDoc -StackName $stackA -Id $script:IdA -Status 'UPDATE_COMPLETE')) })
      })
  )
  foreach ($case in $signCommands) {
    $setupCase = $case[2]
    Test-HostCase "${sg}: $($case[0]) -Host <project> for a host that is not the box makes the operator's key once and signs a fresh certificate, whatever it does over SSH" ([string[]]$case[1]) @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      Check = { param($run, $started)
        Get-SignProblems 'agentspace'
        $f = Get-HostFiles 'agentspace'
        if (-not (Test-SameBytes (Get-FileBytesOrNull $f.Key) $script:KeyBefore)) { 'The operator''s key was replaced' }
        if (Test-SameBytes (Get-FileBytesOrNull $f.Cert) ([Text.Encoding]::UTF8.GetBytes('stale'))) { 'The stale certificate was not replaced' }
        $leftover = @(Get-ChildItem -LiteralPath $f.Dir -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'sign-*' })
        if ($leftover.Count -gt 0) { 'A temporary sign-<hex> file was left in the host''s folder' }
      } } -Setup {
      & $setupCase
      $null = Invoke-HostRun -Arguments @('status', '-Host', 'agentspace')
      $f = Get-HostFiles 'agentspace'
      $script:KeyBefore = Get-FileBytesOrNull $f.Key
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path $f.Cert), 'stale')
      Clear-StandinLogs
      & $setupCase
    }
  }
  Test-HostCase "${sg}: a command given -Host swimcoach writes no key, certificate or alias: the box is reached through the user's own entry (down)" @('down', '-Host', 'swimcoach') @{
    Exit = 0; Stderr = "Stopping $idBox.`n"; Calls = @(Get-CallStop $idBox)
    Check = { param($run, $started) foreach ($m in (Get-NothingSignedProblems)) { $m } } } -Setup $setupTwo

  # ============ The Include line that cannot be added (Q9, architect, 2026-11-21, for v0.07-T03, on #0152's review) ============
  $ic = 'state.md#the-include-line'
  $configText = "Host other`n  HostName example.invalid`nInclude agentspace.d/*`n"
  $configBytes = [Text.UTF8Encoding]::new($false).GetBytes($configText)
  $includeCheck = {
    param($run, $started, [string]$Which)
    $p = [System.Collections.Generic.List[string]]::new()
    $line = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })[-1]
    $m = [regex]::Match([string]$line, '^agentspace: (.+) cannot take agentspace''s Include line: (.+)\.$')
    if (-not $m.Success) { $p.Add('The last stderr line is not agentspace: <config> cannot take agentspace''s Include line: <why>.') }
    else {
      $cfg = Join-Path $fx.SshDir 'config'
      if ($m.Groups[1].Value -ine $cfg -and $m.Groups[1].Value -ine (Assert-ConformancePath -ConformanceHome $fx.H -Path $cfg)) { $p.Add('<config> is not the ssh folder''s config file') }
      $script:IncludeWhy[$Which] = $m.Groups[2].Value
    }
    if (-not (Test-SameBytes (Get-FileBytesOrNull (Join-Path $fx.SshDir 'config')) $configBytes)) { $p.Add('The ssh config was changed') }
    if (Test-Path -LiteralPath (Get-HostFiles 'agentspace').Alias) { $p.Add('The alias file was written') }
    if (@(Get-ChildItem -LiteralPath $fx.SshDir -File -Force | Where-Object { $_.Name -like 'config.agentspace-*' }).Count -gt 0) { $p.Add('A backup or a temporary file of the config was made') }
    $p
  }
  $script:IncludeWhy = @{}
  $setupConfig = {
    & $setupTwo
    [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.SshDir 'config')), $configBytes)
  }
  Test-HostCase "${ic}: sync -Host with the Include line below a Host block fails between steps 1 and 2: the failure line, no AWS call, no ssh or scp, the config and the alias untouched" @('sync', '-Host', 'agentspace') @{
    Exit = 1; Calls = @()
    Check = { param($run, $started) & $includeCheck $run $started 'sync'
      if (@($run.Stderr -split "`n" | Where-Object { $_ -ne '' }).Count -ne 1) { 'Stderr is more than the one failure line' } } } -Setup $setupConfig
  Test-HostCase "${ic}: status -Host with the Include line below a Host block fails after its report: the five lines, then the failure line, the config and the alias untouched" @('status', '-Host', 'agentspace') @{
    Exit = 1; Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA))
    Check = { param($run, $started) & $includeCheck $run $started 'status'
      if (-not $run.Stderr.StartsWith((Get-StatusLines $idA))) { 'The five lines are not printed first' } } } -Setup $setupConfig
  Test-HostCase "${ic}: down -Host with the Include line below a Host block fails after the stop is requested, and the stop stands" @('down', '-Host', 'agentspace') @{
    Exit = 1; Calls = @(Get-CallStop $idA)
    Check = { param($run, $started) & $includeCheck $run $started 'down'
      if (-not $run.Stderr.StartsWith("Stopping $idA.`n")) { 'The Stopping line is not printed first' } } } -Setup $setupConfig
  if ($script:IncludeWhy.Count -eq 3) {
    $why = @($script:IncludeWhy.Values | Select-Object -Unique)
    if ($why.Count -eq 1 -and $why[0].Trim() -ne '') { Write-ConformanceResult -Name "${ic}: sync -Host, status -Host and down -Host word the failure alike: the same <why>, which is not empty" -Status PASS }
    else { Write-ConformanceResult -Name "${ic}: sync -Host, status -Host and down -Host word the failure alike: the same <why>, which is not empty" -Status FAIL -Reason "The three commands gave $($why.Count) different reasons" }
  } else {
    Write-ConformanceResult -Name "${ic}: sync -Host, status -Host and down -Host word the failure alike: the same <why>, which is not empty" -Status FAIL -Reason "Only $($script:IncludeWhy.Count) of the three commands gave a failure line of the spec's form"
  }

  # ============ Starting the host and waiting for it, through sync -Host (one path or two) ============
  $sw = 'operator.md#starting-the-box-and-waiting-for-it'
  $cmdId = 'c0ffee00-1111-4222-8333-abcdefabcdef'
  $patchAnswers = @{
    "ssm describe-instance-information $idA" = @(New-Step (New-SsmDoc -Id $idA))
    "ssm send-command $idA" = @(New-Step (ConvertTo-Json -Compress -InputObject @{ Command = @{ CommandId = $cmdId } }))
    "ssm get-command-invocation $idA" = @(New-Step (ConvertTo-Json -Compress -InputObject @{ Status = 'Success'; StandardOutputContent = "patched`n" }))
  }
  # After the start and wait, sync's own patch wait asks Session Manager again, after its install (operator.md, The patch wait):
  # the same three calls, with the comment `agentspace sync: patch check`, and the start and wait's patch check is `up`'s.
  Test-HostCase "${sw}: sync -Host <project> of a stopped host describes that host's instance, starts it, waits with Session Manager and the patch check on it, prints Starting and Ready, and then makes its own patch wait's three calls" @('sync', '-Host', 'agentspace') @{
    Exit = 0; StderrLines = @("Starting $idA...", "Ready: $idA")
    Calls = @((Get-CallDescribe $idA), (Get-CallStart $idA), (Get-CallSsm $idA),
      @('ssm', 'send-command', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $idA, '--document-name', 'AWS-RunShellScript', '--comment', 'agentspace up: patch check', '--parameters', 're:^commands='),
      "ssm get-command-invocation --region us-west-2 --output json --command-id $cmdId --instance-id $idA") + @(Get-PatchWaitCalls $idA $cmdId)
    SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      Get-SignProblems 'agentspace'
      # The alias and the certificate are written between the CA check and the start-and-wait step.
      $calls = @(Get-AwsCalls)
      if ($calls.Count -gt 0 -and (@($calls[0].aliases) -cnotcontains 'agentspace-host.agentspace' -or @($calls[0].certs) -cnotcontains 'agentspace')) { 'The alias and the certificate did not exist when the first AWS call was made' }
    } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA -State 'stopped') $patchAnswers)
  }
  Test-HostCase "${sw}: sync -Host <project> of a stopping host waits with ec2 wait for that host's instance, then starts it" @('sync', '-Host', 'agentspace') @{
    Exit = 0; StderrLines = @("Waiting for $idA to finish stopping...", "Starting $idA...", "Ready: $idA")
    Calls = @((Get-CallDescribe $idA), "ec2 wait --region us-west-2 instance-stopped --instance-ids $idA", (Get-CallStart $idA), (Get-CallSsm $idA),
      @('ssm', 'send-command', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $idA, '--document-name', 'AWS-RunShellScript', '--comment', 'agentspace up: patch check', '--parameters', 're:^commands='),
      "ssm get-command-invocation --region us-west-2 --output json --command-id $cmdId --instance-id $idA") + @(Get-PatchWaitCalls $idA $cmdId)
    SshCalls = 'any'; ScpCalls = 'any' } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA -State 'stopping') $patchAnswers)
  }
}
Exit-ConformanceRun

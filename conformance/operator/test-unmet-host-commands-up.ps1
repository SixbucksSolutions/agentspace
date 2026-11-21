#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `up -Host <project> [-Alias <alias>]` (docs/operator.md, "up", "Starting the box
# and waiting for it", "The token push", "The stamp", "How the operator reaches a host", "Which host a
# command acts on"; docs/state.md, "A host's push stamp", "The push stamp"; D49, D50, D55). Card #0156
# (E0.07-T04-S02). The push stamp per host and the operator alias file's bytes are checked by other
# files; this one shows which host `up` acts on.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that
# log every call (conformance/operator/host-commands-common.ps1), a fake localswim receipt and a fake
# DPAPI-protected Bot credential for the current user, and AGENTSPACE_TIMEZONE set. The stand-in ssh
# answers a push as the box does. Each case checks the AWS CLI calls, the one push's ssh arguments and
# standard input, the stamp, the messages on stderr, the exit code and what was written for the host.
#
# Cases: `up -Host <project>` reading its host's instance ID from its record (or its stack, writing it
# into the record), `-Alias` and `-Host` together (the push names the alias, the stamp is the host's),
# the key, certificate and alias made first and a failure of that a push failure that warns and exits 0,
# a nested `up` that signs and pushes nothing, `-Host swimcoach` the same as no -Host with the box's
# stamp, and the start-and-wait step for a stopped host.
#
# SAFETY. The token is a made-up value; the harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA
# into the throwaway home and removes every AWS credential, so a mistaken run cannot find the user's receipt
# or token. Before any program that can reach AWS starts, the check confirms under the same environment
# that `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and reports SKIPPED,
# running nothing, if it does not. The alias names no ssh configuration of the user's. Every instance ID
# is made up, and a failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-commands-up.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$zone = 'America/New_York'

Invoke-HostFile -FileName 'test-unmet-host-commands-up.ps1' -Push -Body {
  param($fx)
  $idBox = $script:IdBox; $idA = $script:IdA; $idNew = $script:IdNew
  $stackA = 'agentspace-host-agentspace'
  $content = "LOCALSWIM_URL=$($fx.BaseUrl)`nLOCALSWIM_TOKEN=$($fx.Token)`n"
  $env1 = @{ AGENTSPACE_TIMEZONE = $zone }
  $setupTwo = {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdBox))
  }
  $upCalls = { param([string]$Id) @((Get-CallDescribe $Id), (Get-CallDescribe $Id)) }
  # The stamp a push of instance $Id confirms: the SHA-256 of <id> LF <zone> LF <content>, uppercase hex,
  # in {"version":0,"stamp":"<hex>"} (docs/operator.md, The stamp; docs/state.md, The push stamp).
  function Get-StampOf([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    try { $d = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($Path)) -ErrorAction Stop } catch { return '<not JSON>' }
    if ($null -eq $d -or $null -eq $d.stamp) { return '<no stamp>' }
    [string]$d.stamp
  }
  function Get-WantStamp([string]$Id) { (Get-Sha256Hex ($Id + "`n" + $zone + "`n" + $content)).ToUpperInvariant() }
  $boxStamp = Join-Path $fx.Agentspace 'localswim-pushed.json'
  $stdinIs = {
    $f = Join-Path $fx.Standins 'ssh-stdin-1.bin'
    if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { return 'No standard input was recorded for the push' }
    $b = [IO.File]::ReadAllBytes($f)
    $skip = if ($b.Length -ge 3 -and $b[0] -eq 0xEF -and $b[1] -eq 0xBB -and $b[2] -eq 0xBF) { 3 } else { 0 }
    if ([Text.UTF8Encoding]::new($false).GetString($b, $skip, $b.Length - $skip) -cne $content) { 'The push''s standard input is not the two content lines' }
  }

  # ============ up -Host <project> -Alias <alias> ============
  $u = 'operator.md#up'
  Test-HostCase "${u}: up -Host <project> -Alias <alias> pushes over the alias given, with the host's instance ID and the host's own push stamp, after the key, the certificate and the alias were written" @('up', '-Host', 'agentspace', '-Alias', 'lease-1.test') @{
    Exit = 0; StderrLines = @("Ready: $idA", 'Sending the localswim Bot token to the instance...'); Calls = (& $upCalls $idA)
    SshCalls = @(, @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', 'lease-1.test', 're:.'))
    Check = { param($run, $started)
      Get-SignProblems 'agentspace'
      $ssh = @(Get-SshCalls)
      if ($ssh.Count -eq 1 -and (@($ssh[0].aliases) -cnotcontains 'agentspace-host.agentspace' -or @($ssh[0].certs) -cnotcontains 'agentspace')) { 'The push started before the host''s alias and certificate were written' }
      & $stdinIs
      $got = Get-StampOf (Get-HostFiles 'agentspace').Stamp
      if ($got -cne (Get-WantStamp $script:IdA)) { 'The host''s stamp is not the SHA-256 of the host''s instance ID, the zone and the content, in uppercase hex' }
      if (Test-Path -LiteralPath $boxStamp) { 'The box''s stamp was written by a push to another host' }
      if ($run.Stderr.Contains($fx.Token)) { 'A message holds the token' }
    } } -Setup $setupTwo -Environment $env1
  Test-HostCase "${u}: up -Host <project> with no -Alias pushes over the host's own alias, agentspace-host.<project>" @('up', '-Host', 'agentspace') @{
    Exit = 0; StderrLines = @("Ready: $idA"); Calls = (& $upCalls $idA)
    SshCalls = @(, @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', 'agentspace-host.agentspace', 're:.'))
    Check = { param($run, $started) & $stdinIs; if ((Get-StampOf (Get-HostFiles 'agentspace').Stamp) -cne (Get-WantStamp $script:IdA)) { 'The host''s stamp is not the expected one' } } } -Setup $setupTwo -Environment $env1
  Test-HostCase "${u}: up -Host <project> -Quiet prints neither the Ready line nor the sending line, and still pushes" @('up', '-Host', 'agentspace', '-Quiet') @{
    Exit = 0; Stderr = ''; Calls = (& $upCalls $idA)
    SshCalls = @(, @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', 'agentspace-host.agentspace', 're:.')) } -Setup $setupTwo -Environment $env1
  Test-HostCase "${u}: a push that is not due sends nothing, and up -Host still signs a fresh certificate (a second run)" @('up', '-Host', 'agentspace') @{
    Exit = 0; Stderr = "Ready: $idA`n"; Calls = (& $upCalls $idA)
    Check = { param($run, $started) Get-SignProblems 'agentspace'
      if ((Get-StampOf (Get-HostFiles 'agentspace').Stamp) -cne (Get-WantStamp $script:IdA)) { 'The host''s stamp was lost' } } } -Setup {
    & $setupTwo
    $null = Invoke-HostRun -Arguments @('up', '-Host', 'agentspace') -Environment $env1
    Remove-HomePath (Get-HostFiles 'agentspace').Cert
    Clear-StandinLogs
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdBox))
  } -Environment $env1
  Test-HostCase "${u}: the box's stamp does not stand for the host's: with the box pushed already, up -Host <project> pushes to its own host, and the box's stamp is left as it was" @('up', '-Host', 'agentspace') @{
    Exit = 0; Calls = (& $upCalls $idA)
    SshCalls = @(, @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', 'agentspace-host.agentspace', 're:.'))
    Check = { param($run, $started)
      if ((Get-StampOf $boxStamp) -cne (Get-WantStamp $script:IdBox)) { 'The box''s stamp was changed' }
      if ((Get-StampOf (Get-HostFiles 'agentspace').Stamp) -cne (Get-WantStamp $script:IdA)) { 'The host''s stamp is not its own' } } } -Setup {
    & $setupTwo
    $null = Invoke-HostRun -Arguments @('up') -Environment $env1
    Clear-StandinLogs
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdBox))
  } -Environment $env1

  # ============ The key, the certificate and the alias first; a failure of that is a push failure ============
  $w = 'operator.md#up'
  Test-HostCase "${w}: a failure of the key, certificate and alias is a push failure: up skips the push, warns with its cause, and exits 0 (the CA is missing)" @('up', '-Host', 'agentspace') @{
    Exit = 0; Calls = (& $upCalls $idA)
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      if ($lines.Count -ne 2 -or $lines[0] -cne "Ready: $idA") { 'Stderr is not the Ready line and the warning' }
      elseif ($lines[1] -cnotmatch '^agentspace: Could not send the localswim token, because .+agentspace_ca does not exist; run ca; the board will not work on the host this session\.$') { 'The warning is not agentspace: Could not send the localswim token, because <cause>; the board will not work on the host this session.' }
      if (Test-Path -LiteralPath (Get-HostFiles 'agentspace').Stamp) { 'A stamp was written with no push' }
    } } -Setup {
    & $setupTwo
    Hide-Ca
  } -Environment $env1
  Test-HostCase "${w}: -Quiet suppresses that warning too, and up still exits 0 with no push" @('up', '-Host', 'agentspace', '-Quiet') @{
    Exit = 0; Stderr = ''; Calls = (& $upCalls $idA) } -Setup {
    & $setupTwo
    Hide-Ca
  } -Environment $env1
  # QUESTION: Does a nested up of a host that is not the box sign nothing? operator.md, "up", step 3, says that a person's up, not nested, makes the key, certificate and alias, and that the ProxyCommand's up never does. Interim: a nested up signs nothing and pushes nothing.
  Test-HostCase "${w}: a nested up of a host that is not the box signs nothing and pushes nothing" @('up', '-Host', 'agentspace') @{
    Exit = 0; Calls = (& $upCalls $idA)
    Check = { param($run, $started)
      foreach ($m in (Get-NothingSignedProblems)) { $m }
      if (Test-Path -LiteralPath (Get-HostFiles 'agentspace').Stamp) { 'A stamp was written by a nested up' } } } -Setup $setupTwo -Environment @{ AGENTSPACE_TIMEZONE = $zone; AGENTSPACE_NESTED = '1' }

  # ============ -Host swimcoach is the box ============
  Test-HostCase "${u}: up -Host swimcoach -Alias <alias> pushes with the box's instance ID, writes the box's own stamp where it was, and writes no key, certificate or alias" @('up', '-Host', 'swimcoach', '-Alias', 'lease-1.test') @{
    Exit = 0; Calls = (& $upCalls $idBox)
    SshCalls = @(, @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', 'lease-1.test', 're:.'))
    Check = { param($run, $started)
      if ((Get-StampOf $boxStamp) -cne (Get-WantStamp $script:IdBox)) { 'The box''s stamp is not the expected one' }
      foreach ($m in (Get-NothingSignedProblems)) { $m } } } -Setup $setupTwo -Environment $env1
  Test-HostCase "${u}: up with no -Host pushes over the box's alias agentspace by default, with the box's stamp" @('up') @{
    Exit = 0; Calls = (& $upCalls $idBox)
    SshCalls = @(, @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', 'agentspace', 're:.'))
    Check = { param($run, $started)
      if ((Get-StampOf $boxStamp) -cne (Get-WantStamp $script:IdBox)) { 'The box''s stamp is not the expected one' }
      foreach ($m in (Get-NothingSignedProblems)) { $m } } } -Setup $setupTwo -Environment $env1

  # ============ The host's instance ID, from its record or its stack ============
  Test-HostCase "${u}: up -Host <project> with no instance ID in its record reads its stack, writes the ID into that record alone, and goes on with it" @('up', '-Host', 'agentspace') @{
    Exit = 0; Calls = @((Get-CallStack $stackA), (Get-CallDescribe $idNew), (Get-CallDescribe $idNew))
    SshCalls = 'any'
    Check = { param($run, $started)
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -InstanceId $script:IdNew -LastAttempt '2026-01-01T00:00:00Z'))))) { 'hosts.json is not the two records with the ID written into the host''s alone' }
      if ((Get-StampOf (Get-HostFiles 'agentspace').Stamp) -cne (Get-WantStamp $script:IdNew)) { 'The stamp does not cover the instance ID read from the stack' } } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -LastAttempt '2026-01-01T00:00:00Z'))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdNew) @{ "cloudformation describe-stacks $stackA" = @(New-Step (New-StackDoc -StackName $stackA -Id $script:IdNew)) })
  } -Environment $env1

  # ============ Starting the host and waiting for it ============
  $sw = 'operator.md#starting-the-box-and-waiting-for-it'
  $cmdId = 'c0ffee00-1111-4222-8333-abcdefabcdef'
  $patchAnswers = @{
    "ssm describe-instance-information $idA" = @(New-Step (New-SsmDoc -Id $idA))
    "ssm send-command $idA" = @(New-Step (ConvertTo-Json -Compress -InputObject @{ Command = @{ CommandId = $cmdId } }))
    "ssm get-command-invocation $idA" = @(New-Step (ConvertTo-Json -Compress -InputObject @{ Status = 'Success'; StandardOutputContent = "patched`n" }))
  }
  $waitCalls = @((Get-CallDescribe $idA), (Get-CallStart $idA), (Get-CallSsm $idA),
    @('ssm', 'send-command', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $idA, '--document-name', 'AWS-RunShellScript', '--comment', 'agentspace up: patch check', '--parameters', 're:^commands='),
    "ssm get-command-invocation --region us-west-2 --output json --command-id $cmdId --instance-id $idA")
  Test-HostCase "${sw}: up -Host <project> of a stopped host describes that host's instance, starts it, waits with Session Manager and the patch check on it, prints Starting and Ready" @('up', '-Host', 'agentspace') @{
    Exit = 0; Stderr = (Join-Lines @("Starting $idA...", "Ready: $idA")); Calls = $waitCalls } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA -State 'stopped') $patchAnswers)
  } -Environment @{ AGENTSPACE_NESTED = '1' }
  Test-HostCase "${sw}: up -Host <project> of a stopping host waits with ec2 wait for that host's instance, then starts it" @('up', '-Host', 'agentspace') @{
    Exit = 0; Stderr = (Join-Lines @("Waiting for $idA to finish stopping...", "Starting $idA...", "Ready: $idA"))
    Calls = @((Get-CallDescribe $idA), "ec2 wait --region us-west-2 instance-stopped --instance-ids $idA", (Get-CallStart $idA), (Get-CallSsm $idA), $waitCalls[3], $waitCalls[4]) } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA -State 'stopping') $patchAnswers)
  } -Environment @{ AGENTSPACE_NESTED = '1' }
}
Exit-ConformanceRun

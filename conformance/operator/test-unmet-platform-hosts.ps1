#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator acting on a project's host by its platform (docs/operator.md,
# "Which host a command acts on", "From the command line", "A host's platform and its names", "The
# host registry, as the operator uses it", "provision"; docs/state.md, "The host registry"; D91;
# docs/conformance.md, "v0.12's planned cases for the shared and Linux parts", cases W6 to W8;
# E0.12-T04-S02, card #0262, planned by #0248). The host's names are the spec's: the stack
# agentspace-linux-amd64-host-<project>, the alias agentspace-host.<project>.linux-amd64, the folder
# hosts\<project>.linux-amd64\ and the certificate's key ID operator-<project>.linux-amd64.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp`
# that log every call (conformance/operator/host-commands-common.ps1, platform-common.ps1), a host
# registry the case writes and the home's throwaway CA. Only the linux/amd64 host's names are the
# whole of it; no case starts a Windows host.
#
# Cases:
#   W6  A command acts on its platform's host and on no other: with the project's arm64 and amd64
#       records both in the registry, each of the eight commands with -Platform linux/amd64 makes
#       calls with the amd64 host's values alone, and given -Host alone acts on the arm64 record.
#   W7  A project with no host on the platform is refused before any program or AWS call, with the
#       host's name in the message; provision appends the amd64 record for a project whose entry
#       approves the platform, and is refused before any AWS call for one that does not.
#   W8  The registry is keyed by project and platform: a record with no platform member or with
#       "linux/arm64" is the arm64 host's, two records of one project and platform make the file not
#       valid, each platform's record is written by its own key, and a new record is appended.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. PATH holds no go. Every instance ID is made
# up, and a failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-platform-hosts.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')
. (Join-Path $PSScriptRoot 'platform-common.ps1')

$ErrorActionPreference = 'Stop'

Invoke-HostFile -FileName 'test-unmet-platform-hosts.ps1' -Push -Body {
  param($fx)
  $idA = $script:IdA; $idB = $script:IdB; $idBox = $script:IdBox; $idC = $script:IdC
  $armStack = 'agentspace-host-agentspace'
  $ampStack = 'agentspace-linux-amd64-host-agentspace'
  $armStamp = '{"stamp":"arm64 host''s own, never written by an amd64 command"}'
  # Records in the order of creation: the box, then the project's arm64 host, then its amd64 host.
  $recordsTwo = {
    @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB))
  }
  # Both hosts answer as healthy hosts do: running and long launched, or stopped. Every stack answers.
  function Set-TwoHosts([string]$State = 'running') {
    Set-HostsRecords (& $recordsTwo)
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA -State $State) (Get-HealthyAnswers -Id $script:IdB -State $State) (Get-HealthyAnswers -Id $script:IdBox -State $State) @{
        "cloudformation describe-stacks $armStack" = @(New-Step (New-StackDoc -StackName $armStack -Id $script:IdA))
        "cloudformation describe-stacks $ampStack" = @(New-Step (New-StackDoc -StackName $ampStack -Id $script:IdB))
        'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdBox))
      })
    # The arm64 host's stamp and its folder, as a host that was pushed to before: left as they were.
    $dir = (Get-PfFiles 'agentspace').Dir
    New-HomeFolder $dir
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $dir 'localswim-pushed.json')), $armStamp)
  }
  $armAlias = '(?<![\w.-])agentspace-host\.agentspace(?![\w.-])'
  # The text of every call of one program, for a search.
  $allText = { (@(Get-AwsCalls) + @(Get-SshCalls) + @(Get-ScpCalls) | ForEach-Object { $_.Text }) -join "`n" }

  # ============ W6: a command acts on its platform's host, and on no other ============
  $s = 'operator.md#which-host-a-command-acts-on'
  $commands = @(
    @{ N = 'deploy'; A = @('deploy'); State = 'running' },
    @{ N = 'up'; A = @('up'); State = 'running'; Stamp = $true },
    @{ N = 'sync'; A = @('sync'); State = 'running'; Stamp = $true },
    @{ N = 'down'; A = @('down'); State = 'running' },
    @{ N = 'status'; A = @('status'); State = 'running' },
    @{ N = 'grow -GiB 4'; A = @('grow', '-GiB', '4'); State = 'stopped' },
    @{ N = 'bootstrap'; A = @('bootstrap'); State = 'running' },
    @{ N = 'provision'; A = @('provision'); State = 'running' }
  )
  foreach ($c in $commands) {
    $c1 = $c
    Test-PfRun "${s}: W6: $($c1.N) -Host <project> -Platform linux/amd64 makes every call with the amd64 host's values alone: its stack, instance ID, alias, folder, key and certificate with the key ID operator-<project>.linux-amd64, and leaves the arm64 host's stamp and files as they were" (@($c1.A) + @('-Host', 'agentspace', '-Platform', 'linux/amd64')) -Check { param($run, $started)
        $t = & $allText
        if (-not ($t -match [regex]::Escape($script:IdB) -or $t -match [regex]::Escape($ampStack))) { 'No call names the amd64 host''s instance or stack' }
        foreach ($bad in @($script:IdA, (Get-VolumeOf $script:IdA), $armStack, "stack-name agentbox")) { if ($t.Contains($bad)) { 'A call carries the arm64 record''s value' ; break } }
        if ($t -cmatch $armAlias) { 'A call carries the arm64 host''s alias' }
        if (@(Get-SshCalls).Count -gt 0 -and -not ($t -match [regex]::Escape('agentspace-host.agentspace.linux-amd64'))) { 'An ssh call does not name the amd64 host''s alias, agentspace-host.agentspace.linux-amd64' }
        Get-PfSignProblems 'agentspace' 'linux/amd64'
        if ($c1.Stamp -and -not (Test-Path -LiteralPath (Get-PfFiles 'agentspace' 'linux/amd64').Stamp -PathType Leaf)) { 'The amd64 host''s push stamp, localswim-pushed.json in the folder agentspace.linux-amd64 of the hosts, was not written' }
        $arm = Get-PfFiles 'agentspace'
        foreach ($f in $arm.Key, $arm.Cert, $arm.Alias) { if (Test-Path -LiteralPath $f) { 'A file of the arm64 host was written' ; break } }
        if (-not (Test-SameBytes (Get-FileBytesOrNull $arm.Stamp) ([Text.UTF8Encoding]::new($false).GetBytes($armStamp)))) { 'The arm64 host''s stamp was changed' }
        if ((Get-PfFiles 'agentspace' 'linux/amd64').Dir -cne (Join-Path $fx.HostsDir 'agentspace.linux-amd64')) { 'The amd64 host''s folder is not hosts\agentspace.linux-amd64' }
      } -Setup ([scriptblock]::Create("Set-TwoHosts '$($c1.State)'")) -Timeout 180 -Environment @{ AGENTSPACE_TIMEZONE = 'America/New_York' }
    Test-PfRun "${s}: W6: $($c1.N) -Host <project> alone acts on the arm64 record, by the names it has today, and never on the amd64 host's" (@($c1.A) + @('-Host', 'agentspace')) -Check { param($run, $started)
        $t = & $allText
        if (-not ($t.Contains($script:IdA) -or $t.Contains($armStack))) { 'No call names the arm64 host''s instance or stack' }
        foreach ($bad in @($script:IdB, (Get-VolumeOf $script:IdB), $ampStack, 'linux-amd64')) { if ($t.Contains($bad)) { 'A call carries the amd64 record''s value' ; break } }
        Get-PfSignProblems 'agentspace' 'linux/arm64'
        if (Test-Path -LiteralPath (Get-PfFiles 'agentspace' 'linux/amd64').Dir) { 'The amd64 host''s folder was made' }
      } -Setup ([scriptblock]::Create("Set-TwoHosts '$($c1.State)'")) -Timeout 180 -Environment @{ AGENTSPACE_TIMEZONE = 'America/New_York' }
  }
  # Exact calls for three commands, in order.
  Test-HostCase "${s}: W6: status -Host <project> -Platform linux/amd64 makes the amd64 host's three calls in order, and the Platform line and five lines are its own" @('status', '-Host', 'agentspace', '-Platform', 'linux/amd64') @{
    Exit = 0; Stderr = ("Platform  linux/amd64`n" + (Get-StatusLines $idB)); Calls = @((Get-CallDescribe $idB), (Get-CallVolumes $idB), (Get-CallSsm $idB)) } -Setup { Set-TwoHosts }
  Test-HostCase "${s}: W6: down -Host <project> -Platform linux/amd64 stops the amd64 host's instance alone" @('down', '-Host', 'agentspace', '-Platform', 'linux/amd64') @{
    Exit = 0; Stderr = "Stopping $idB.`n"; Calls = @((Get-CallStop $idB)) } -Setup { Set-TwoHosts }
  Test-HostCase "${s}: W6: grow -Host <project> -Platform linux/amd64 -GiB 4 changes the amd64 host's volume alone" @('grow', '-Host', 'agentspace', '-Platform', 'linux/amd64', '-GiB', '4') @{
    Exit = 0; Calls = @((Get-CallDescribe $idB), (Get-CallVolumes $idB), (Get-CallModify $idB 24)) } -Setup { Set-TwoHosts 'stopped' }
  Test-HostCase "${s}: W6: -Host swimcoach -Platform linux/amd64 names swimcoach's own amd64 host, never agentbox: its stack is agentspace-linux-amd64-host-swimcoach" @('status', '-Host', 'swimcoach', '-Platform', 'linux/amd64') @{
    Exit = 0; Stderr = ("Platform  linux/amd64`n" + (Get-StatusLines $idC)); Calls = @((Get-CallDescribe $idC), (Get-CallVolumes $idC), (Get-CallSsm $idC))
    Check = { param($run, $started)
      Get-PfSignProblems 'swimcoach' 'linux/amd64'
      $t = & $allText
      if ($t.Contains($script:IdBox) -or $t.Contains('agentbox')) { 'A call names the box' }
    } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'swimcoach' -Platform 'linux/amd64' -InstanceId $script:IdC))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdC) (Get-HealthyAnswers -Id $script:IdBox))
  }
  Test-HostCase "${s}: W6: with only swimcoach's amd64 record named, -Host swimcoach -Platform linux/amd64 reads that stack for a missing instance ID, agentspace-linux-amd64-host-swimcoach" @('status', '-Host', 'swimcoach', '-Platform', 'linux/amd64') @{
    Exit = 0; Calls = 'any'
    Check = { param($run, $started)
      $c = @(Get-AwsCalls)
      if ($c.Count -lt 1 -or $c[0].Text -cne (Get-CallStack 'agentspace-linux-amd64-host-swimcoach')) { 'The first call is not describe-stacks of agentspace-linux-amd64-host-swimcoach' }
    } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'swimcoach' -Platform 'linux/amd64'))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdC) @{ 'cloudformation describe-stacks agentspace-linux-amd64-host-swimcoach' = @(New-Step (New-StackDoc -StackName 'agentspace-linux-amd64-host-swimcoach' -Id $script:IdC)) })
  }

  # ============ W7: a project with no host on the platform ============
  $s = 'operator.md#which-host-a-command-acts-on'
  $noHost = "agentspace: Project 'agentspace.linux-amd64' has no host; provision -Host agentspace -Platform linux/amd64 makes one.`n"
  $armOnly = {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) @{ "cloudformation describe-stacks $armStack" = @(New-Step (New-StackDoc -StackName $armStack -Id $script:IdA)) })
  }
  $armOnlyBytes = Get-HostsBytes @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $idA))
  foreach ($cmd in @(@('deploy'), @('up'), @('sync'), @('bootstrap'), @('down'), @('status'), @('grow'))) {
    $c1 = $cmd
    Test-HostCase "${s}: W7: $($c1[0]) -Host <project> -Platform linux/amd64 for a project with only its arm64 host fails before any program or AWS call, with the no-host message naming the host name, exit 1, writing nothing" (@($c1) + @('-Host', 'agentspace', '-Platform', 'linux/amd64')) @{
      Exit = 1; Stderr = $noHost; Calls = @()
      Check = { param($run, $started)
        if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $armOnlyBytes)) { 'hosts.json was changed' }
        foreach ($m in (Get-NothingSignedProblems)) { $m }
      } } -Setup $armOnly
  }
  Test-HostCase "${s}: W7: with no record at all and no -Platform, v0.07's message, byte for byte, naming the project" @('status', '-Host', 'zebra') @{
    Exit = 1; Stderr = "agentspace: Project 'zebra' has no host; provision -Host zebra makes one.`n"; Calls = @() } -Setup $armOnly
  Test-HostCase "${s}: W7: with no record at all and -Platform linux/amd64, the message names the host name and the platform in the command it suggests" @('down', '-Host', 'zebra', '-Platform', 'linux/amd64') @{
    Exit = 1; Stderr = "agentspace: Project 'zebra.linux-amd64' has no host; provision -Host zebra -Platform linux/amd64 makes one.`n"; Calls = @() } -Setup $armOnly
  Test-HostCase "${s}: W7: the project's amd64 host never stands in for its arm64 host: with only an amd64 record, -Host <project> fails with v0.07's message" @('status', '-Host', 'agentspace') @{
    Exit = 1; Stderr = "agentspace: Project 'agentspace' has no host; provision -Host agentspace makes one.`n"; Calls = @() } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB))
    Set-AwsScenario (Get-HealthyAnswers -Id $script:IdB)
  }

  # provision appends the amd64 record only for a project whose entry approves that platform.
  $projectsFile = Join-Path $fx.Agentspace 'projects.json'
  function Set-Projects([string]$Text) {
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path $projectsFile), $Text)
  }
  $approved = '{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20","platforms":[{"platform":"linux/amd64","approved":"2026-11-26"}]}]}' + "`n"
  Test-PfRun "${s}: W7: provision -Host <project> -Platform linux/amd64 for an entry that approves it appends the amd64 record at the end of the file with its platform and its stack's name, leaves the arm64 record and every other byte as they were, and works in hosts\<project>.linux-amd64\" @('provision', '-Host', 'agentspace', '-Platform', 'linux/amd64') -Check { param($run, $started)
      $text = [Text.UTF8Encoding]::new($false).GetString((Get-FileBytesOrNull $fx.HostsFile))
      $head = [Text.UTF8Encoding]::new($false).GetString($armOnlyBytes)
      $head = $head.Substring(0, $head.Length - 3)
      if (-not $text.StartsWith($head + ',')) { 'The records that were there are not as they were, followed by the new one' }
      $d = ConvertFrom-Json -InputObject $text -AsHashtable
      $r = @($d.hosts)
      if ($r.Count -ne 3) { 'The file does not hold three records' }
      else {
        if ($r[2].project -cne 'agentspace' -or $r[2].platform -cne 'linux/amd64') { 'The last record is not the project''s amd64 host' }
        if ($r[2].stack -cne $ampStack) { 'The new record''s stack is not agentspace-linux-amd64-host-agentspace' }
      }
      if (-not (Test-Path -LiteralPath (Get-PfFiles 'agentspace' 'linux/amd64').Log)) { 'There is no provision.log in hosts\agentspace.linux-amd64\' }
      if (Test-Path -LiteralPath (Get-PfFiles 'agentspace').Log) { 'A provision.log was written for the arm64 host' }
    } -Setup {
    & $armOnly
    Set-Projects $approved
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) @{
        "cloudformation describe-stacks $armStack" = @(New-Step (New-StackDoc -StackName $armStack -Id $script:IdA))
        "cloudformation describe-stacks $ampStack" = @(New-Step '' "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $ampStack does not exist" 254)
        'ssm get-parameter' = @(New-Step ('{"Parameter":{"Value":"' + $script:PfImage + '"}}'))
      })
  } -Timeout 180
  $refusals = @(
    @{ N = 'an entry with no platforms'; Platform = 'linux/amd64'; Text = '{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20"}]}' + "`n"; Kind = 'platform' },
    @{ N = 'an entry whose platforms omit it'; Platform = 'linux/amd64'; Text = '{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20","platforms":[{"platform":"windows/amd64","approved":"2026-11-26"}]}]}' + "`n"; Kind = 'platform' },
    @{ N = 'no entry for the project, on linux/amd64'; Platform = 'linux/amd64'; Text = '{"version":0,"projects":[{"name":"other","approved":"2026-11-20"}]}' + "`n"; Kind = 'unlisted' },
    @{ N = 'no entry for the project, on linux/arm64 with -Platform'; Platform = 'linux/arm64'; Text = '{"version":0,"projects":[{"name":"other","approved":"2026-11-20"}]}' + "`n"; Kind = 'unlisted' }
  )
  foreach ($c in $refusals) {
    $c1 = $c
    Invoke-PfCase "${s}: W7: provision -Host <project> -Platform $($c1.Platform) for $($c1.N) is refused before any AWS call with the project list's message naming the project, never the host name, writing nothing: no log, no folder and no record" {
      param($p)
      Reset-HostCase
      Set-HostsRecords @((New-BoxRecord))
      Set-Projects $c1.Text
      $project = if ($c1.Kind -eq 'unlisted') { 'zebra' } else { 'agentspace' }
      $before = Get-PfSnapshot
      $r = Invoke-HostRun -Arguments @('provision', '-Host', $project, '-Platform', $c1.Platform)
      if ($r.ExitCode -ne 1) { $p.Add("Exit $($r.ExitCode), not 1") }
      if ($r.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
      $re = if ($c1.Kind -eq 'unlisted') { '^agentspace: Project ''zebra'' has no host and is not in (.+), so no host is made for it\.\n$' }
      else { '^agentspace: Project ''agentspace'' has no host on ' + [regex]::Escape($c1.Platform) + ', and (.+) does not approve that platform for it, so no host is made for it\.\n$' }
      $m = [regex]::Match($r.Stderr, $re)
      if (-not $m.Success) { $p.Add('Stderr is not the spec''s message for this refusal') }
      elseif (-not (Test-PfSamePath $m.Groups[1].Value $projectsFile)) { $p.Add('The path in the message is not the project list''s full path') }
      if (@(Get-AwsCalls).Count + @(Get-SshCalls).Count + @(Get-ScpCalls).Count -gt 0) { $p.Add('A program was called') }
      if ((Get-PfSnapshot) -cne $before) { $p.Add('The home''s files changed: a record, a log or a folder') }
    }
  }
  Test-PfRun "${s}: W7: an arm64 host with a record is provisioned whatever the list holds: a list that omits linux/arm64 does not refuse provision -Host <project> -Platform linux/arm64 for a project that has its record" @('provision', '-Host', 'agentspace', '-Platform', 'linux/arm64') -Check { param($run, $started)
      if ($run.Stderr -match 'does not approve that platform') { 'The run was refused for the list' }
      if ($run.Stderr -match 'Unknown parameter') { 'The line was refused as having an unknown parameter' }
      if (@(Get-AwsCalls).Count -eq 0) { 'The run went no further than the command line' } } -Setup {
    & $armOnly
    Set-Projects $approved
  }

  # ============ W8: the registry is keyed by project and platform ============
  $s = 'state.md#the-host-registry'
  $five = Get-StatusLines $idA
  Test-HostCase "${s}: W8: a record with no platform member is the arm64 host's for status -Host <project>, and no Platform line is printed" @('status', '-Host', 'agentspace') @{
    Exit = 0; Stderr = $five; Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA)) } -Setup { Set-TwoHosts }
  foreach ($flags in @(@(), @('-Platform', 'linux/arm64'))) {
    $f2 = $flags
    Test-HostCase "${s}: W8: a record that holds ""linux/arm64"" is the arm64 host's, with $(if ($f2.Count) { '-Platform linux/arm64' } else { 'no -Platform' }): its calls, and no Platform line" (@('status', '-Host', 'agentspace') + $f2) @{
      Exit = 0; Stderr = $five; Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA)) } -Setup {
      Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA -WithPlatformMember))
      Set-AwsScenario (Get-HealthyAnswers -Id $script:IdA)
    }
  }
  Test-HostCase "${s}: W8: a record that holds ""linux/arm64"" and the project's amd64 record are two hosts: -Platform linux/amd64 reaches the second" @('status', '-Host', 'agentspace', '-Platform', 'linux/amd64') @{
    Exit = 0; Stderr = ("Platform  linux/amd64`n" + (Get-StatusLines $idB)); Calls = @((Get-CallDescribe $idB), (Get-CallVolumes $idB), (Get-CallSsm $idB)) } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA -WithPlatformMember), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB))
  }
  $invalid = @(
    @{ N = 'one with no platform member and one holding "linux/arm64"'; R = { @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdC -WithPlatformMember)) }; Ctl = { @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdC)) } },
    @{ N = 'two with no platform member'; R = { @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdC)) }; Ctl = { @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdC)) } },
    @{ N = 'two holding linux/amd64'; R = { @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdC)) }; Ctl = { @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB), (New-PfRecord -Project 'agentspace' -Platform 'windows/amd64' -InstanceId $script:IdC)) } }
  )
  foreach ($c in $invalid) {
    $c1 = $c
    foreach ($cmd in @(@('status', '-Host', 'agentspace'), @('down', '-Host', 'agentspace', '-Platform', 'linux/amd64'), @('status'))) {
      $cmd1 = $cmd
      Invoke-PfCase "${s}: W8: two records of one project and platform ($($c1.N)) make the registry not valid: $($cmd1 -join ' ') fails with the registry's message naming the file, before any call, and writes nothing" {
        param($p)
        # The control: the same project's records with the second on a platform of its own make a valid file, which the command reads.
        Reset-HostCase
        Set-HostsRecords (& $c1.Ctl)
        Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB) (Get-HealthyAnswers -Id $script:IdC) (Get-HealthyAnswers -Id $script:IdBox))
        $ctl = Invoke-HostRun -Arguments $cmd1
        if ($ctl.ExitCode -ne 0) { $p.Add('The control run, over the same records with the second on a platform of its own, did not succeed') }
        Reset-HostCase
        Set-HostsRecords (& $c1.R)
        $bytes = Get-FileBytesOrNull $fx.HostsFile
        $r = Invoke-HostRun -Arguments $cmd1
        if ($r.ExitCode -ne 1) { $p.Add("Exit $($r.ExitCode), not 1") }
        if ($r.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
        $m = [regex]::Match($r.Stderr, '^agentspace: The host registry (.+) cannot be read: (.+)\.\n$')
        if (-not $m.Success) { $p.Add('Stderr is not agentspace: The host registry <path> cannot be read: <reason>.') }
        elseif (-not (Test-PfSamePath $m.Groups[1].Value $fx.HostsFile)) { $p.Add('The path in the message is not the registry''s full path') }
        if (@(Get-AwsCalls).Count + @(Get-SshCalls).Count + @(Get-ScpCalls).Count -gt 0) { $p.Add('A program was called') }
        if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $bytes)) { $p.Add('hosts.json was changed') }
      }
    }
  }
  # Each platform's record is written by its own key, and no record is moved: a file seeded with the
  # project's amd64 record before its arm64 one keeps that order after every write.
  $creating = { param([string]$Platform) New-PfRecord -Project 'agentspace' -Platform $Platform -State 'creating' }
  foreach ($c in @(
      @{ N = 'the arm64 host'; Args = @('status', '-Host', 'agentspace'); Stack = $armStack; Id = $idA; Order = @('linux/amd64', 'linux/arm64'); Five = (Get-StatusLines $idA) },
      @{ N = 'the amd64 host'; Args = @('status', '-Host', 'agentspace', '-Platform', 'linux/amd64'); Stack = $ampStack; Id = $idB; Order = @('linux/amd64', 'linux/arm64'); Five = ("Platform  linux/amd64`n" + (Get-StatusLines $idB)) })) {
    $c1 = $c
    Test-HostCase "${s}: W8: a missing instance ID is written into $($c1.N)'s record alone: the other platform's record keeps its bytes, and the amd64 record that was seeded before the arm64 one stays first" $c1.Args @{
      Exit = 0; Stderr = $c1.Five; Calls = 'any'
      Check = { param($run, $started)
        $want = foreach ($pl in $c1.Order) {
          if ($pl -ceq $(if ($c1.Id -ceq $script:IdA) { 'linux/arm64' } else { 'linux/amd64' })) { New-PfRecord -Project 'agentspace' -Platform $pl -State 'creating' -InstanceId $c1.Id }
          else { New-PfRecord -Project 'agentspace' -Platform $pl -State 'creating' }
        }
        if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes (@((New-BoxRecord)) + @($want))))) { 'hosts.json is not the three records as they were, with the instance ID written into the host''s own record, in the file''s order' }
      } } -Setup {
      Set-HostsRecords @((New-BoxRecord), (& $creating 'linux/amd64'), (& $creating 'linux/arm64'))
      Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB) @{
          "cloudformation describe-stacks $armStack" = @(New-Step (New-StackDoc -StackName $armStack -Id $script:IdA))
          "cloudformation describe-stacks $ampStack" = @(New-Step (New-StackDoc -StackName $ampStack -Id $script:IdB))
        })
    }
  }
  # A new linux/arm64 record is written with no platform member, appended after the amd64 record that
  # was there first, and nothing before it moves.
  Test-PfRun "${s}: W8: provision -Host <project> for a project with only an amd64 record appends a new arm64 record at the end, with no platform member, and leaves the amd64 record and the box's record as they were" @('provision', '-Host', 'agentspace') -Check { param($run, $started)
      $text = [Text.UTF8Encoding]::new($false).GetString((Get-FileBytesOrNull $fx.HostsFile))
      $seed = [Text.UTF8Encoding]::new($false).GetString((Get-HostsBytes @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB))))
      if (-not $text.StartsWith($seed.Substring(0, $seed.Length - 3) + ',')) { 'The records that were there are not as they were, followed by the new one' }
      $d = ConvertFrom-Json -InputObject $text -AsHashtable
      $r = @($d.hosts)
      if ($r.Count -ne 3) { 'The file does not hold three records' }
      else {
        if ($r[2].project -cne 'agentspace' -or $r[2].ContainsKey('platform')) { 'The last record is not the project''s arm64 host written with no platform member' }
        if ($r[2].stack -cne $armStack) { 'The new record''s stack is not agentspace-host-agentspace' }
      }
    } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB))
    Set-Projects ('{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20"}]}' + "`n")
    Set-AwsScenario @{ "cloudformation describe-stacks $armStack" = @(New-Step '' "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $armStack does not exist" 254) }
  } -Timeout 180
}
Exit-ConformanceRun

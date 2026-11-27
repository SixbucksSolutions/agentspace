#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's hosts on `windows/amd64`: W20, W21, W22 and W29 of docs/conformance.md,
# "The operator's Windows cases" (docs/operator.md, "Which host a command acts on", "How the operator reaches a
# host", "A host's platform and its names", "The platform seam", "provision", "status"; D91, D95). Card #0262
# (E0.12-T04-S02), from #0253's plan.
#
#   W20  Each of `up`, `down`, `status`, `grow -GiB 4`, `sync`, `bootstrap`, `deploy` and `provision` given
#        `-Host <project> -Platform windows/amd64` acts on the Windows record alone: every AWS, ssh and scp call
#        carries its stack, instance ID and alias, none carries the project's other two hosts', their records
#        and stamps are left as they were, and the key, certificate and alias written are the Windows host's.
#        Three spellings of the platform act as `windows/amd64`.
#   W21  A project with only an arm64 record has no Windows host: every command but `provision` fails with the
#        no-record message naming `<project>.windows-amd64`, before any program or AWS call. `provision` adds
#        the Windows record only for an entry that approves the platform, and is refused otherwise with the
#        project and platform named, and for a project with no entry with the project list's message.
#   W22  `status -Host <project> -Platform windows/amd64` prints `Platform  windows/amd64` first, and
#        `status -All` prints it for the hosts off linux/arm64 alone, each `Host` line naming the host name, each
#        `Leases    <n> live` the count of that project's live leases on that platform alone, in the file's order.
#   W29  The alias holds `User agent`, the certificate names the principal `agent` and the key ID
#        `operator-<project>.windows-amd64`; the Linux hosts keep `ubuntu`; the user is never in the registry,
#        and a record's changed platform member changes the user.
#
# EXPECTED TO FAIL AGAINST v0.11's OPERATOR, which has no -Platform and refuses a registry record holding
# `platform`: every case reports a FAIL there, each by its own assertion. A v0.12 build MUST pass every result.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that log every
# call (conformance/operator/host-commands-common.ps1, windows-common.ps1), a host registry of one project's three
# hosts that the case writes, and the home's throwaway CA. No stand-in is a Windows host: a case reads the
# arguments the operator passes and the files it writes, and never what a host does with them.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and removes every
# AWS credential. Before any program that can reach AWS starts, the check confirms under the same environment that
# `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and reports SKIPPED, running nothing,
# if it does not. PATH holds no go. Every instance ID is made up, and a failure reason never quotes the program's
# output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-windows-hosts.ps1
. (Join-Path $PSScriptRoot 'windows-common.ps1')

$ErrorActionPreference = 'Stop'

Invoke-HostFile -FileName 'test-unmet-windows-hosts.ps1' -Push -Body {
  param($fx)
  $project = 'example'
  $wn = 'windows/amd64'
  $idW = $script:WnIdWin; $idA = $script:WnIdArm; $idL = $script:WnIdAmd
  $stackW = Get-WnStack $project $wn
  $aliasW = Get-WnAlias $project $wn
  $hostW = Get-WnHostName $project $wn
  $hostA = Get-WnHostName $project 'linux/arm64'
  $hostL = Get-WnHostName $project 'linux/amd64'
  $foreignValues = @($idA, $idL, (Get-WnStack $project 'linux/arm64'), (Get-WnStack $project 'linux/amd64'))
  $foreignAliases = @((Get-WnAlias $project 'linux/arm64'), (Get-WnAlias $project 'linux/amd64'))
  $stampText = '{"version":0,"stamp":"' + ('AB' * 32) + '"}' + "`n"
  $stampBytes = [Text.UTF8Encoding]::new($false).GetBytes($stampText)

  # The project's three hosts in the order that is not by project then platform, the answers of healthy hosts and of
  # their stacks, and a stamp that each Linux host already holds, which no command on the Windows host may change.
  $seedThree = {
    Remove-WnLeases
    $script:Seed = Set-WnThreeHosts -Project $project
    Set-AwsScenario (Merge-Scenario (Get-WnHealthy -Project $project) (Get-WnStacks -Project $project))
    foreach ($h in @($hostA, $hostL)) {
      $dir = Join-Path $fx.HostsDir $h
      New-HomeFolder $dir
      [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $dir 'localswim-pushed.json')), $stampBytes)
    }
  }
  # What every command on the Windows host leaves and does, whichever command it is.
  $actsOnWindowsAlone = {
    param([bool]$ExpectRegistryUnchanged, [bool]$ExpectPush, [bool]$ExpectStampLeftAlone = $true)
    $aws = @(Get-AwsCalls); $ssh = @(Get-SshCalls); $scp = @(Get-ScpCalls)
    if (@($aws | Where-Object { (Test-WnNames $_ $idW) -or (Test-WnNames $_ $stackW) }).Count -eq 0) { 'No AWS call names the Windows host''s instance or stack' }
    foreach ($m in (Get-WnForeignProblems $aws $foreignValues 'An AWS call')) { $m }
    foreach ($m in (Get-WnAliasProblems -Ssh $ssh -Scp $scp -Want $aliasW -Foreign $foreignAliases)) { $m }
    foreach ($m in (Get-SignProblems $hostW)) { $m }
    $folders = @(Get-WnHostFolders)
    if (@($folders | Where-Object { $_ -cne $hostW -and $_ -cne $hostA -and $_ -cne $hostL }).Count -gt 0) { 'A host folder of a host that is not one of the three was made' }
    foreach ($h in @($hostA, $hostL)) {
      $dir = Join-Path $fx.HostsDir $h
      $names = @(Get-ChildItem -LiteralPath $dir -Name -Force -ErrorAction SilentlyContinue)
      if ($names.Count -ne 1 -or $names[0] -cne 'localswim-pushed.json') { "A file was written in another host's folder" }
      elseif (-not (Test-SameBytes (Get-FileBytesOrNull (Join-Path $dir 'localswim-pushed.json')) $stampBytes)) { "Another host's push stamp was changed" }
    }
    if ($ExpectPush -and -not (Test-Path -LiteralPath (Get-HostFiles $hostW).Stamp -PathType Leaf)) { 'The Windows host''s push stamp was not written in its own folder' }
    if ($ExpectRegistryUnchanged) {
      $seedBytes = Get-HostsBytes @((New-BoxRecord), $script:Seed.Win, $script:Seed.Arm, $script:Seed.Amd)
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $seedBytes)) { 'hosts.json was changed' }
    } else {
      foreach ($m in (Get-WnOtherRecordProblems (Get-WnHostsText) @($script:Seed.Arm, $script:Seed.Amd))) { $m }
      $text = Get-WnHostsText
      if ($null -ne $text -and $text.IndexOf('"platform":"windows/amd64"') -gt $text.IndexOf($script:Seed.Arm)) { 'The Windows record was moved after the others' }
    }
  }

  # ============ W20: a command acts on its platform's host, and on no other ============
  $s20 = 'operator.md#which-host-a-command-acts-on'
  $up = @('up', '-Host', $project, '-Platform', $wn)
  Test-HostCase "${s20}: W20 up -Host <project> -Platform windows/amd64 reads, starts and pushes the Windows host alone: its instance, its alias, its own stamp" $up @{
    Exit = 0; Calls = @((Get-CallDescribe $idW), (Get-CallDescribe $idW)); SshCalls = @(, @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', $aliasW, 're:.'))
    Check = { param($run, $started) & $actsOnWindowsAlone $true $true } } -Setup $seedThree -Environment @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' }
  Test-HostCase "${s20}: W20 down -Host <project> -Platform windows/amd64 stops the Windows host's instance alone" @('down', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = @(Get-CallStop $idW)
    Check = { param($run, $started) & $actsOnWindowsAlone $true $false } } -Setup $seedThree
  Test-HostCase "${s20}: W20 status -Host <project> -Platform windows/amd64 reads the Windows host's instance, volume and Session Manager alone" @('status', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = @((Get-CallDescribe $idW), (Get-CallVolumes $idW), (Get-CallSsm $idW))
    Check = { param($run, $started) & $actsOnWindowsAlone $true $false } } -Setup $seedThree
  Test-HostCase "${s20}: W20 grow -Host <project> -Platform windows/amd64 -GiB 4 changes the Windows host's volume alone (the host is stopped)" @('grow', '-Host', $project, '-Platform', $wn, '-GiB', '4') @{
    Exit = 0; Calls = @((Get-CallDescribe $idW), (Get-CallVolumes $idW), (Get-CallModify $idW 24))
    Check = { param($run, $started) & $actsOnWindowsAlone $true $false } } -Setup {
    & $seedThree
    Set-AwsScenario (Merge-Scenario (Get-WnHealthy -Project $project -State 'stopped') (Get-WnStacks -Project $project))
  }
  Test-HostCase "${s20}: W20 grow -Host <project> -Platform windows/amd64 of a running host sends its online step through the Windows host's alias alone" @('grow', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = @((Get-CallDescribe $idW), (Get-CallVolumes $idW), (Get-CallModify $idW 22))
    SshCalls = @(, @('-o', 'ClearAllForwardings=yes', $aliasW, 're:.'))
    Check = { param($run, $started) & $actsOnWindowsAlone $true $false } } -Setup $seedThree
  Test-HostCase "${s20}: W20 sync -Host <project> -Platform windows/amd64 makes every AWS, ssh and scp call with the Windows host's values alone" @('sync', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $actsOnWindowsAlone $true $true
      if (@(Get-SshCalls).Count -lt 3 -or @(Get-ScpCalls).Count -lt 2) { 'sync started fewer than its three ssh and two scp calls (rows 1 to 4 and the push)' } } } -Setup $seedThree -Environment @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' }
  Test-HostCase "${s20}: W20 bootstrap -Host <project> -Platform windows/amd64 runs the toolchain bootstrap through the Windows host's alias alone" @('bootstrap', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $actsOnWindowsAlone $true $false
      if (@(Get-SshCalls).Count -ne 1 -or @(Get-ScpCalls).Count -ne 1) { 'bootstrap did not start one ssh and one scp' } } } -Setup $seedThree
  Test-HostCase "${s20}: W20 deploy -Host <project> -Platform windows/amd64 deploys the Windows host's stack alone, and writes its record alone" @('deploy', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $actsOnWindowsAlone $false $true
      $d = @(Get-AwsCalls | Where-Object { $_.a.Count -ge 2 -and $_.a[0] -ceq 'cloudformation' -and $_.a[1] -ceq 'deploy' })
      if ($d.Count -ne 1 -or (Get-WnFlag $d[0] '--stack-name') -cne $stackW) { 'cloudformation deploy was not called once for the Windows host''s stack' } } } -Setup $seedThree -Environment @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' }
  Test-HostCase "${s20}: W20 provision -Host <project> -Platform windows/amd64 provisions the Windows host alone, and its log and lock are its own folder's" @('provision', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $actsOnWindowsAlone $false $true
      if (-not (Test-Path -LiteralPath (Get-HostFiles $hostW).Log -PathType Leaf)) { 'The Windows host''s provision.log was not written in its own folder' } } } -Setup {
    & $seedThree
    Set-WnProjects @((New-WnProjectEntry $project @('linux/arm64', 'linux/amd64', 'windows/amd64')))
  } -Environment @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' }

  # The platform's other spellings act as windows/amd64 (W2's forms, for the Windows platform).
  foreach ($form in @(' Windows/AMD64 ', 'windows/x86_64', 'WINDOWS/X86-64')) {
    $f = $form
    Test-HostCase "${s20}: W20 -Platform '$f' acts as windows/amd64: the Windows host's calls, and the names of its normalized form" @('status', '-Host', $project, '-Platform', $f) @{
      Exit = 0; Calls = @((Get-CallDescribe $idW), (Get-CallVolumes $idW), (Get-CallSsm $idW))
      Check = { param($run, $started) & $actsOnWindowsAlone $true $false } } -Setup $seedThree
  }

  # ============ W21: a Windows host exists only for a project that approves the platform ============
  $s21 = 'operator.md#provision'
  $noRecord = "agentspace: Project '$hostW' has no host; provision -Host $project -Platform $wn makes one.`n"
  $armOnly = { Set-HostsRecords @((New-BoxRecord), (New-WnRecord -Project $project -State 'ready' -InstanceId $script:WnIdArm)); Set-AwsScenario (Merge-Scenario (Get-WnHealthy -Project $project) (Get-WnStacks -Project $project)) }
  $armOnlyBytes = Get-HostsBytes @((New-BoxRecord), (New-WnRecord -Project $project -State 'ready' -InstanceId $idA))
  foreach ($cmd in @(@('up'), @('down'), @('status'), @('grow', '-GiB', '4'), @('sync'), @('bootstrap'), @('deploy'))) {
    $c = $cmd
    Test-HostCase "${s21}: W21 $($c -join ' ') -Host <project> -Platform windows/amd64 for a project with only an arm64 record fails with the no-record message naming <project>.windows-amd64, before any program or AWS call" (@($c[0], '-Host', $project, '-Platform', $wn) + @($c | Select-Object -Skip 1)) @{
      Exit = 1; Stderr = $noRecord; Calls = @(); SshCalls = @(); ScpCalls = @()
      Check = { param($run, $started)
        if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $armOnlyBytes)) { 'hosts.json was changed' }
        foreach ($m in (Get-NothingSignedProblems)) { $m } } } -Setup $armOnly
  }
  $listPath = Join-Path $fx.Agentspace 'projects.json'
  $notApproved = "agentspace: Project '$project' has no host on $wn, and $listPath does not approve that platform for it, so no host is made for it.`n"
  $notInList = "agentspace: Project '$project' has no host and is not in $listPath, so no host is made for it.`n"
  $refusedChecks = {
    param($run, $started)
    if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $armOnlyBytes)) { 'hosts.json was changed: no record may be added' }
    if (Test-Path -LiteralPath $fx.HostsDir) { 'A hosts folder was made: no log, lock or folder may be written' }
    foreach ($m in (Get-NothingSignedProblems)) { $m }
  }
  Test-HostCase "${s21}: W21 provision -Host <project> -Platform windows/amd64 is refused, naming the project and the platform, for an entry that approves linux/amd64 alone: before any AWS call, nothing written" @('provision', '-Host', $project, '-Platform', $wn) @{
    Exit = 1; Stderr = $notApproved; Calls = @(); SshCalls = @(); ScpCalls = @(); Check = $refusedChecks } -Setup {
    & $armOnly
    Set-WnProjects @((New-WnProjectEntry $project @('linux/amd64')))
  }
  Test-HostCase "${s21}: W21 provision -Host <project> -Platform windows/amd64 is refused the same way for an entry with no platforms, which approves linux/arm64 alone" @('provision', '-Host', $project, '-Platform', $wn) @{
    Exit = 1; Stderr = $notApproved; Calls = @(); SshCalls = @(); ScpCalls = @(); Check = $refusedChecks } -Setup {
    & $armOnly
    Set-WnProjects @((New-WnProjectEntry $project))
  }
  Test-HostCase "${s21}: W21 provision -Host <project> -Platform windows/amd64 for a project with no entry is refused with the project list's message, naming the project and never the host name" @('provision', '-Host', $project, '-Platform', $wn) @{
    Exit = 1; Stderr = $notInList; Calls = @(); SshCalls = @(); ScpCalls = @(); Check = $refusedChecks } -Setup {
    & $armOnly
    Set-WnProjects @((New-WnProjectEntry 'other' @('windows/amd64')))
  }
  $armOnlyRecord = New-WnRecord -Project $project -State 'ready' -InstanceId $idA
  Test-HostCase "${s21}: W21 provision -Host <project> -Platform windows/amd64 for an entry that approves it appends the Windows record, with its platform and its own stack, and leaves every other record byte for byte" @('provision', '-Host', $project, '-Platform', $wn) @{
    Exit = 1; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $text = Get-WnHostsText
      if ($null -eq $text) { return 'hosts.json is missing' }
      foreach ($m in (Get-WnOtherRecordProblems $text @((New-BoxRecord), $armOnlyRecord))) { $m }
      $doc = ConvertFrom-Json -InputObject $text -AsHashtable
      $win = @($doc.hosts | Where-Object { $_.project -ceq $project -and $_.ContainsKey('platform') -and $_.platform -ceq $wn })
      if ($win.Count -ne 1) { 'There is not exactly one Windows record'; return }
      if ($win[0].stack -cne $stackW) { 'The Windows record''s stack is not agentspace-windows-amd64-host-<project>' }
      if ($doc.hosts[$doc.hosts.Count - 1] -ne $win[0]) { 'The Windows record was not appended at the end of the file' }
      $arm = @($doc.hosts | Where-Object { $_.project -ceq $project -and -not $_.ContainsKey('platform') })
      if ($arm.Count -ne 1) { 'The arm64 record gained a platform member or was lost' }
      foreach ($m in (Get-WnForeignProblems @(Get-AwsCalls) @($idA, (Get-WnStack $project 'linux/arm64')) 'An AWS call')) { $m }
      if (-not (Test-Path -LiteralPath (Get-HostFiles $hostW).Log -PathType Leaf)) { 'The Windows host''s provision.log was not written in its own folder' }
      if (Test-Path -LiteralPath (Get-HostFiles $hostA).Dir) { 'The arm64 host''s folder was made' } } } -Setup {
    & $armOnly
    Set-WnProjects @((New-WnProjectEntry $project @('linux/arm64', 'windows/amd64')))
    # The run stops at the stack's existence check, so the case reads what the run wrote first: the record.
    Set-AwsScenario @{ 'cloudformation describe-stacks' = @(New-Step '' 'An error occurred (UnauthorizedOperation) when calling the DescribeStacks operation: no' 254) }
  }

  # ============ W22: status prints the platform of a Windows host ============
  $s22 = 'operator.md#status--all'
  Test-HostCase "${s22}: W22 status -Host <project> -Platform windows/amd64 prints its Platform line first, then the five lines" @('status', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Stderr = ("Platform  $wn`n" + (Get-StatusLines $idW -Type (Get-WnType $wn))); Calls = @((Get-CallDescribe $idW), (Get-CallVolumes $idW), (Get-CallSsm $idW)) } -Setup $seedThree
  Test-HostCase "${s22}: W22 status -Host <project> -Platform linux/arm64 prints no Platform line, the five lines of v0.11" @('status', '-Host', $project, '-Platform', 'linux/arm64') @{
    Exit = 0; Stderr = (Get-StatusLines $idA); Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA)) } -Setup $seedThree
  $soon = (Get-WnNow).AddHours(2)
  Test-HostCase "${s22}: W22 status -All over the box and the project's three hosts prints Platform for the Windows and linux/amd64 hosts alone, each Host line naming its host name, each Leases count that platform's live leases alone, in the file's order" @('status', '-All') @{
    Exit = 0
    Stderr = (Join-Lines @(
        'Host      swimcoach (agentbox), ready'
        (Get-StatusLines $script:IdBox).TrimEnd("`n")
        'Leases    1 live'
        "Host      $hostW ($stackW), ready"
        "Platform  $wn"
        (Get-StatusLines $idW -Type (Get-WnType $wn)).TrimEnd("`n")
        'Leases    2 live'
        "Host      $hostA ($(Get-WnStack $project 'linux/arm64')), ready"
        (Get-StatusLines $idA).TrimEnd("`n")
        'Leases    1 live'
        "Host      $hostL ($(Get-WnStack $project 'linux/amd64')), ready"
        'Platform  linux/amd64'
        (Get-StatusLines $idL -Type (Get-WnType 'linux/amd64')).TrimEnd("`n")
        'Leases    1 live'))
    Calls = 'any'
    Check = { param($run, $started)
      if (@(Get-AwsCalls | Where-Object { $_.a.Count -ge 2 -and $_.a[1] -ceq 'describe-instances' }).Count -ne 4) { 'There was not one ec2 describe-instances for each of the four hosts' }
      # status -All signs nothing and writes no alias (docs/operator.md, How the operator reaches a host).
      if (Test-Path -LiteralPath $fx.AliasDir) { if (@(Get-ChildItem -LiteralPath $fx.AliasDir -Force -ErrorAction SilentlyContinue).Count -gt 0) { 'An alias file was written' } }
      if (Test-Path -LiteralPath (Join-Path $fx.SshDir 'config')) { 'An ssh config was written' }
      if (@(Get-ChildItem -LiteralPath $fx.HostsDir -Recurse -File -Force -Filter 'id_ed25519*' -ErrorAction SilentlyContinue).Count -gt 0) { 'A key or a certificate was written for a host' } } } -Setup {
    & $seedThree
    Set-WnLeases @(
      (New-WnLease -Project 'swimcoach' -State 'ready' -Expires $soon),
      (New-WnLease -Project $project -State 'ready' -Platform $wn -Expires $soon),
      (New-WnLease -Project $project -State 'pending' -Platform $wn),
      (New-WnLease -Project $project -State 'released' -Platform $wn -Expires $soon -Released (Get-WnNow).AddMinutes(-5)),
      (New-WnLease -Project $project -State 'ready' -Expires $soon),
      (New-WnLease -Project $project -State 'ready' -Platform 'linux/amd64' -Expires $soon),
      (New-WnLease -Project $project -State 'unsatisfiable' -Platform 'linux/amd64'))
  }

  # ============ W29: the login is agent, and the operator signs as the Windows host ============
  $s29 = 'operator.md#how-the-operator-reaches-a-host'
  # The user, the principal and the key ID that status -Host <project> [-Platform ...] left for a host.
  $signedAs = {
    param([string]$HostName, [string]$User)
    $f = Get-HostFiles $HostName
    if (-not (Test-Path -LiteralPath $f.Alias -PathType Leaf)) { return "The alias file for $HostName was not written" }
    $alias = [IO.File]::ReadAllText($f.Alias)
    if ($alias -cnotmatch "(?m)^  User $User\n") { "The alias file for $HostName does not hold 'User $User'" }
    if ($alias -cnotmatch "(?m)^Host agentspace-host\.$([regex]::Escape($HostName))\n") { "The alias file for $HostName is not for its own Host name" }
    $kg = Join-Path $env:SystemRoot 'System32\OpenSSH\ssh-keygen.exe'
    $out = (& $kg -L -f $f.Cert 2>&1 | Out-String)
    if ($out -notmatch ('Key ID: "operator-' + [regex]::Escape($HostName) + '"')) { "The certificate's key ID is not operator-$HostName" }
    if ($out -notmatch ('(?m)Principals:\s*\r?\n\s*' + $User + '\s*\r?\n')) { "The certificate's principal is not $User" }
  }
  Test-HostCase "${s29}: W29 the Windows host's alias holds User agent, its certificate names the principal agent and the key ID operator-<project>.windows-amd64, and the registry holds no user member" @('status', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'
    Check = { param($run, $started)
      & $signedAs $hostW 'agent'
      if ((Get-WnHostsText) -match '"user"') { 'The registry holds a user member: the user is never stored' } } } -Setup $seedThree
  foreach ($h in @(@('linux/arm64', $hostA), @('linux/amd64', $hostL))) {
    $wPlat = $h[0]; $wHost = $h[1]
    Test-HostCase "${s29}: W29 the $wPlat host's principal stays ubuntu, with the key ID operator-$wHost" @('status', '-Host', $project, '-Platform', $wPlat) @{
      Exit = 0; Calls = 'any'
      Check = { param($run, $started) & $signedAs $wHost 'ubuntu' } } -Setup $seedThree
  }
  Test-HostCase "${s29}: W29 with the same instance and a changed platform member, the user changes: a linux/amd64 record signs as ubuntu, where the windows/amd64 record signed as agent" @('status', '-Host', $project, '-Platform', 'linux/amd64') @{
    Exit = 0; Calls = 'any'
    Check = { param($run, $started)
      & $signedAs $hostL 'ubuntu'
      if (Test-Path -LiteralPath (Get-HostFiles $hostW).Alias) { 'An alias for the Windows host was written for a linux/amd64 record' } } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-WnRecord -Project $project -Platform 'linux/amd64' -State 'ready' -InstanceId $script:WnIdWin))
    Set-AwsScenario (Get-HealthyAnswers -Id $script:WnIdWin -Type (Get-WnType 'linux/amd64'))
  }
}

Exit-ConformanceRun

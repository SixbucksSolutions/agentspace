#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `status -All`, `down -All` and `sync -All` over hosts of more than one platform
# (docs/operator.md, "status -All", "Every host at once: -All", "A host's platform and its names",
# "sync"; docs/state.md, "The host registry", "Live leases and the lease horizon"; docs/conformance.md,
# "v0.12's planned cases for the shared and Linux parts", cases W11 and W12; D60, D91; E0.12-T04-S02,
# card #0262, planned by #0248). Every expectation is the spec's text: the report's lines, the order of
# the file, the count of a platform's live leases, the builds' lines and files.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh`, `scp` and `go`
# that log every call (conformance/operator/host-commands-common.ps1, platform-common.ps1), a host
# registry and a lease state file the case writes, and the home's throwaway CA. The registry seeds a
# project's linux/amd64 record before its linux/arm64 one, so that a build that sorts by project then
# platform fails it, and no host of another platform is started.
#
#   W11  `status -All` prints a `Platform` line right after the `Host` line of a host off linux/arm64,
#        none for a linux/arm64 host, the box included, names such a host by its host name and counts
#        each host's live leases on its platform alone; `status -Host <project> -Platform
#        linux/amd64` prints the line first and `status -Host <project>` none.
#   W12  `status -All`, `down -All` and `sync -All` go through the hosts in the file's order, every
#        platform's hosts acted on; `sync -All` builds the localswim CLI once for each platform present,
#        each with its own Building line in the part of the first host of that platform.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. Every instance ID is made up, and a failure
# reason never quotes the program's output. The only look at C:\Projects\localswim is one Test-Path on
# the folder, and the `go` stand-in writes only under the throwaway home.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-platform-all.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')
. (Join-Path $PSScriptRoot 'platform-common.ps1')

$ErrorActionPreference = 'Stop'

function Get-AllHostLine([string]$Project, [string]$Platform, [string]$State) {
  "Host      $(Get-PfHostName $Project $Platform) ($(Get-PfStack $Project $Platform)), $State"
}

Invoke-HostFile -FileName 'test-unmet-platform-all.ps1' -Push -Body {
  param($fx)
  New-PfGoStandIn
  $idBox = $script:IdBox; $idA = $script:IdA; $idB = $script:IdB; $idC = $script:IdC
  $s = 'operator.md#status--all'
  $ampStack = Get-PfStack 'agentspace' 'linux/amd64'
  $armStack = Get-PfStack 'agentspace' 'linux/arm64'
  # The box, then agentspace's linux/amd64 host (running, launched 5 minutes ago, so that the guard's stop time
  # is 10 minutes ahead), then its linux/arm64 host (running, launched long ago), then example's linux/amd64 host
  # (still being made, no instance ID). The amd64 record comes before the arm64 one in the file.
  $launchAmd = Get-PfAgo 5 37
  $records = @(
    (New-BoxRecord),
    (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $idB),
    (New-PfRecord -Project 'agentspace' -InstanceId $idA),
    (New-PfRecord -Project 'example' -Platform 'linux/amd64' -State 'creating'))
  $setupAll = {
    Set-HostsRecords $records
    $sc = Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB)
    $sc["ec2 describe-instances $script:IdB"] = @(New-Step (New-InstanceDoc -Id $script:IdB -State 'running' -Launch (Format-PfAwsTime $launchAmd)))
    Set-AwsScenario $sc
    # swimcoach: one live lease on linux/arm64. agentspace: a live lease on linux/arm64 and none on linux/amd64, a
    # released one on amd64 explaining nothing. example: a pending lease on linux/amd64, which is live.
    Set-PfLeases @(
      (New-PfLease swimcoach ready -Expires (Get-PfAhead 120)),
      (New-PfLease agentspace ready -Expires (Get-PfAhead 90)),
      (New-PfLease agentspace released -Platform 'linux/amd64' -Released (Get-PfAgo 30) -Expires (Get-PfAgo 40)),
      (New-PfLease example pending -Platform 'linux/amd64'))
  }
  $report = Join-Lines @(
    (Get-AllHostLine 'swimcoach' 'linux/arm64' 'ready'), (Get-StatusLines $idBox).TrimEnd("`n"), 'Leases    1 live',
    (Get-AllHostLine 'agentspace' 'linux/amd64' 'ready'), 'Platform  linux/amd64', (Get-StatusLines $idB).TrimEnd("`n"), 'Leases    0 live',
    ('Guard     unexplained; stops at ' + (Format-PfClock $launchAmd.AddMinutes(15))),
    (Get-AllHostLine 'agentspace' 'linux/arm64' 'ready'), (Get-StatusLines $idA).TrimEnd("`n"), 'Leases    1 live',
    (Get-AllHostLine 'example' 'linux/amd64' 'creating'), 'Platform  linux/amd64', 'Instance  none recorded', 'Leases    1 live')
  $reportCalls = @(
    (Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox),
    (Get-CallDescribe $idB), (Get-CallVolumes $idB), (Get-CallSsm $idB),
    (Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA))
  $recordsUnchanged = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes $records))) { 'hosts.json was changed' } }

  # ============ W11: status -All prints each host off linux/arm64 with its platform ============
  Test-HostCase "${s}: W11: status -All goes through the hosts in the file's order, names a host off linux/arm64 by its host name, prints Platform  <platform> right after its Host line, none for a linux/arm64 host, the box included, and counts each host's live leases on its platform alone, with an unexplained amd64 host whose project holds an arm64 lease alone" @('status', '-All') @{
    Exit = 0; Stderr = $report; Calls = $reportCalls; Check = $recordsUnchanged } -Setup $setupAll
  Test-HostCase "${s}: W11: the Platform line is part of the report: status -All -Quiet prints it too" @('status', '-All', '-Quiet') @{
    Exit = 0; Stderr = $report; Calls = $reportCalls } -Setup $setupAll
  Test-HostCase "${s}: W11: the Platform line needs no lease state file and no AWS call: with the file missing, the amd64 host's part still has its Platform line after its Host line, and no Leases or Guard line" @('status', '-All') @{
    Exit = 0; Calls = 'any'
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      $i = [array]::IndexOf($lines, (Get-AllHostLine 'agentspace' 'linux/amd64' 'ready'))
      if ($i -lt 0 -or $lines[$i + 1] -cne 'Platform  linux/amd64') { 'The Platform line does not follow the amd64 host''s Host line' }
      $j = [array]::IndexOf($lines, (Get-AllHostLine 'example' 'linux/amd64' 'creating'))
      if ($j -lt 0 -or $lines[$j + 1] -cne 'Platform  linux/amd64' -or $lines[$j + 2] -cne 'Instance  none recorded') { 'The Platform line does not come between the Host line and Instance  none recorded' }
      if (@($lines | Where-Object { $_ -like 'Leases *' -or $_ -like 'Guard *' }).Count -gt 0) { 'A Leases or Guard line was printed for a file that was not read' }
      if (@($lines | Where-Object { $_ -ceq 'Platform  linux/arm64' }).Count -gt 0) { 'A linux/arm64 host has a Platform line' }
    } } -Setup {
    & $setupAll
    Remove-HomePath (Join-Path $fx.H.StateDir 'leases.json')
  }
  Test-HostCase "${s}: W11: a linux/arm64 host's part is byte for byte v0.11's: a registry of the box and an arm64 host alone prints no Platform line, and its Host lines are as before" @('status', '-All') @{
    Exit = 0
    Stderr = (Join-Lines @(
        (Get-AllHostLine 'swimcoach' 'linux/arm64' 'ready'), (Get-StatusLines $idBox).TrimEnd("`n"), 'Leases    1 live',
        (Get-AllHostLine 'agentspace' 'linux/arm64' 'ready'), (Get-StatusLines $idA 'stopped' 'm9g.xlarge' 20 '-').TrimEnd("`n"), 'Leases    0 live'))
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      # agentspace's arm64 host is running, launched long ago, with no live lease on linux/arm64: it has a Guard line, whatever its time.
      if (@($lines | Where-Object { $_ -like 'Platform *' }).Count -gt 0) { 'A Platform line was printed for a linux/arm64 host' }
    } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA -State 'stopped'))
    Set-PfLeases @((New-PfLease swimcoach ready -Expires (Get-PfAhead 120)))
  }
  Test-HostCase "${s}: W11: status -Host <project> -Platform linux/amd64 prints the Platform line first, before its five" @('status', '-Host', 'agentspace', '-Platform', 'linux/amd64') @{
    Exit = 0; Stderr = ("Platform  linux/amd64`n" + (Get-StatusLines $idB)); Calls = @((Get-CallDescribe $idB), (Get-CallVolumes $idB), (Get-CallSsm $idB)) } -Setup $setupAll
  Test-HostCase "${s}: W11: status -Host <project> prints no Platform line" @('status', '-Host', 'agentspace') @{
    Exit = 0; Stderr = (Get-StatusLines $idA); Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA)) } -Setup $setupAll
  Test-HostCase "${s}: W11: status -Host <project> -Platform linux/arm64 prints no Platform line either" @('status', '-Host', 'agentspace', '-Platform', 'linux/arm64') @{
    Exit = 0; Stderr = (Get-StatusLines $idA); Calls = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA)) } -Setup $setupAll
  Test-HostCase "${s}: W11: a project with a live arm64 lease and none on amd64 reports its amd64 host Leases    0 live, and the box counts swimcoach's linux/arm64 leases alone, whatever swimcoach holds on amd64" @('status', '-All') @{
    Exit = 0
    Stderr = (Join-Lines @(
        (Get-AllHostLine 'swimcoach' 'linux/arm64' 'ready'), (Get-StatusLines $idBox).TrimEnd("`n"), 'Leases    1 live',
        (Get-AllHostLine 'swimcoach' 'linux/amd64' 'ready'), 'Platform  linux/amd64', (Get-StatusLines $idC 'stopped' 'm9g.xlarge' 20 '-').TrimEnd("`n"), 'Leases    2 live',
        (Get-AllHostLine 'agentspace' 'linux/arm64' 'ready'), (Get-StatusLines $idA 'stopped' 'm9g.xlarge' 20 '-').TrimEnd("`n"), 'Leases    1 live',
        (Get-AllHostLine 'agentspace' 'linux/amd64' 'ready'), 'Platform  linux/amd64', (Get-StatusLines $idB 'stopped' 'm9g.xlarge' 20 '-').TrimEnd("`n"), 'Leases    0 live')) } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'swimcoach' -Platform 'linux/amd64' -InstanceId $script:IdC), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA -State 'stopped') (Get-HealthyAnswers -Id $script:IdB -State 'stopped') (Get-HealthyAnswers -Id $script:IdC -State 'stopped'))
    Set-PfLeases @(
      (New-PfLease swimcoach ready -Expires (Get-PfAhead 120)),
      (New-PfLease swimcoach ready -Platform 'linux/amd64' -Expires (Get-PfAhead 120)), (New-PfLease swimcoach pending -Platform 'linux/amd64'),
      (New-PfLease agentspace ready -Expires (Get-PfAhead 90)))
  }

  # ============ W12: -All goes through the hosts in the file's order ============
  $s = 'operator.md#every-host-at-once--all'
  Test-HostCase "${s}: W12: status -All visits the hosts in the file's order, the amd64 record before the arm64 one of the same project, with no sorting" @('status', '-All') @{
    Exit = 0; Calls = $reportCalls
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -like 'Host      *' })
      $want = @((Get-AllHostLine 'swimcoach' 'linux/arm64' 'ready'), (Get-AllHostLine 'agentspace' 'linux/amd64' 'ready'), (Get-AllHostLine 'agentspace' 'linux/arm64' 'ready'), (Get-AllHostLine 'example' 'linux/amd64' 'creating'))
      if (($lines -join "`n") -cne ($want -join "`n")) { 'The Host lines are not the registry''s four records in the file''s order, the Host line of each host naming it by its host name' }
    } } -Setup $setupAll
  $downAll = Join-Lines @(
    (Get-AllHostLine 'swimcoach' 'linux/arm64' 'ready'), "Stopping $idBox.",
    (Get-AllHostLine 'agentspace' 'linux/amd64' 'ready'), "Stopping $idB.",
    (Get-AllHostLine 'agentspace' 'linux/arm64' 'ready'), "Stopping $idA.",
    (Get-AllHostLine 'example' 'linux/amd64' 'creating'), 'Skipping: no instance is recorded for this host.')
  Test-HostCase "${s}: W12: down -All stops every platform's hosts in the file's order, after each Host line, and skips a creating host with no instance ID" @('down', '-All') @{
    Exit = 0; Stderr = $downAll; Calls = @((Get-CallStop $idBox), (Get-CallStop $idB), (Get-CallStop $idA))
    Check = $recordsUnchanged } -Setup $setupAll

  # sync -All builds the localswim CLI once for each platform present, as a stand-in `go` answers it.
  $syncRecords = @(
    (New-BoxRecord),
    (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $idB),
    (New-PfRecord -Project 'agentspace' -InstanceId $idA))
  $syncSetup = {
    Set-HostsRecords $syncRecords
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB))
  }
  if (-not (Test-Path -LiteralPath 'C:\Projects\localswim' -PathType Container)) {
    Write-ConformanceResult -Name "${s}: W12: sync -All builds the localswim CLI once for each platform present" -Status SKIPPED -Reason 'The folder C:\Projects\localswim does not exist on this machine, so sync''s step 7 builds nothing and a case that shows the build cannot run'
  } else {
    Test-HostCase "${s}: W12: sync -All over hosts of two Linux platforms builds the localswim CLI once for each platform present, localswim-linux-arm64 and localswim-linux-amd64, each with its own Building line in the part of the first host of its platform, and copies each host the build of its own platform" @('sync', '-All') @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      Check = { param($run, $started)
        $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
        $iBox = [array]::IndexOf($lines, (Get-AllHostLine 'swimcoach' 'linux/arm64' 'ready'))
        $iAmd = [array]::IndexOf($lines, (Get-AllHostLine 'agentspace' 'linux/amd64' 'ready'))
        $iArm = [array]::IndexOf($lines, (Get-AllHostLine 'agentspace' 'linux/arm64' 'ready'))
        if ($iBox -lt 0 -or $iAmd -lt 0 -or $iArm -lt 0 -or -not ($iBox -lt $iAmd -and $iAmd -lt $iArm)) { return 'The Host lines are not the box, agentspace.linux-amd64 and agentspace, in the file''s order' }
        $bArm = @(0..($lines.Count - 1) | Where-Object { $lines[$_] -ceq 'Building the localswim CLI for linux/arm64...' })
        $bAmd = @(0..($lines.Count - 1) | Where-Object { $lines[$_] -ceq 'Building the localswim CLI for linux/amd64...' })
        if ($bArm.Count -ne 1) { 'The linux/arm64 build line is not printed once' } elseif (-not ($bArm[0] -gt $iBox -and $bArm[0] -lt $iAmd)) { 'The linux/arm64 build line is not in the part of the first linux/arm64 host, the box' }
        if ($bAmd.Count -ne 1) { 'The linux/amd64 build line is not printed once' } elseif (-not ($bAmd[0] -gt $iAmd -and $bAmd[0] -lt $iArm)) { 'The linux/amd64 build line is not in the part of the first linux/amd64 host' }
        $go = @(Get-PfGoCalls)
        if ($go.Count -ne 2) { "go was run $($go.Count) times, not once for each of the two platforms" }
        else {
          if ($go[0].goos -cne 'linux' -or $go[0].goarch -cne 'arm64' -or $go[0].cgo -cne '0') { 'The first build is not linux/arm64 with CGO_ENABLED=0' }
          if ($go[1].goos -cne 'linux' -or $go[1].goarch -cne 'amd64' -or $go[1].cgo -cne '0') { 'The second build is not linux/amd64 with CGO_ENABLED=0' }
          $outs = @($go | ForEach-Object { $i = [array]::IndexOf($_.a, '-o'); if ($i -ge 0) { Split-Path -Leaf $_.a[$i + 1] } else { '' } })
          if ($outs[0] -cne 'localswim-linux-arm64' -or $outs[1] -cne 'localswim-linux-amd64') { 'The builds are not named localswim-linux-arm64 and localswim-linux-amd64' }
        }
        $cli = @(Get-ScpCalls | Where-Object { $_.a.Count -gt 0 -and $_.a[-1] -like '*:/tmp/localswim' })
        if ($cli.Count -ne 3) { return "There were $($cli.Count) copies of the localswim CLI, not one for each of three hosts" }
        $want = @{ 'agentspace' = 'localswim-linux-arm64'; 'agentspace-host.agentspace.linux-amd64' = 'localswim-linux-amd64'; 'agentspace-host.agentspace' = 'localswim-linux-arm64' }
        foreach ($c in $cli) {
          $alias = ($c.a[-1] -split ':')[0]
          $leaf = Split-Path -Leaf $c.a[-2]
          if (-not $want.ContainsKey($alias)) { "A copy of the localswim CLI goes to an unexpected alias" }
          elseif ($leaf -cne $want[$alias]) { "The host $alias is copied $leaf, not $($want[$alias]), the build of its own platform" }
        }
      } } -Setup {
      & $syncSetup
      New-HomeFolder (Join-Path $fx.H.UserProfile '.claude')
    } -Timeout 240
    Test-HostCase "${s}: W12: sync -All builds only for the platforms of the hosts it syncs: with the amd64 host still creating and skipped, the localswim CLI is built for linux/arm64 alone, once" @('sync', '-All') @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      Check = { param($run, $started)
        $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
        if (@($lines | Where-Object { $_ -ceq 'Building the localswim CLI for linux/arm64...' }).Count -ne 1) { 'The linux/arm64 build line is not printed once' }
        if (@($lines | Where-Object { $_ -like 'Building the localswim CLI for linux/amd64*' }).Count -gt 0) { 'A linux/amd64 build was made though no linux/amd64 host was synced' }
        $iAmd = [array]::IndexOf($lines, (Get-AllHostLine 'agentspace' 'linux/amd64' 'creating'))
        if ($iAmd -lt 0 -or $lines[$iAmd + 1] -cne 'Skipping: the host is creating; its next provisioning run syncs it.') { 'The creating amd64 host is not skipped with its line' }
        $go = @(Get-PfGoCalls)
        if ($go.Count -ne 1 -or $go[0].goarch -cne 'arm64') { 'go was not run once, for linux/arm64' }
        if ($lines | Where-Object { $_ -match '^agentspace: \d+ of \d+ hosts failed\.$' }) { 'A closing count was printed though no host failed' }
      } } -Setup {
      Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -State 'creating'), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA))
      Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox) (Get-HealthyAnswers -Id $script:IdA))
    } -Timeout 240
  }
}
Exit-ConformanceRun

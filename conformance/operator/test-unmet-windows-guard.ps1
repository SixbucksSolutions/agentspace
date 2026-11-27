#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the guard's way with a `windows/amd64` host: W28 of docs/conformance.md, "The operator's
# Windows cases" (docs/operator.md, "guard", "The horizon on a Windows host", "The platform seam", "Its log";
# docs/state.md, "Live leases and the lease horizon"; D56, D57, D91, D96). Card #0262 (E0.12-T04-S02), from
# #0253's plan.
#
#   W28  A running Windows host is explained by its project's live leases on `windows/amd64` alone; the guard
#        sends it its horizon, computed from those leases alone, by `ssm send-command` with
#        `AWS-RunPowerShellScript`, the comment `agentspace guard: lease horizon` and one command that names
#        `C:\ProgramData\agentspace` and `lease-horizon` and `[IO.File]::WriteAllText` and holds the horizon's digits
#        as one word, passed as one `--parameters` argument that parses as JSON, `{"commands":["<command>"]}`,
#        never the shorthand `commands=`; in one pass the Linux hosts get the shell document and the same rule,
#        and the pass visits the hosts, and the describe filter names their IDs, in the file's order; the
#        Windows host's log lines and messages name `<project>.windows-amd64`.
#
# The bytes the command writes and the JSON the guard builds are the crate tests' (docs/conformance.md, W28), so
# this file reads the command from the parsed JSON and asserts what the spec requires of it, never its text.
#
# EXPECTED TO FAIL AGAINST v0.11's OPERATOR, which refuses a registry record holding `platform`: every case
# reports a FAIL there, each by its own assertion. A v0.12 build MUST pass every result.
#
# What it does: it runs `guard` in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that log every
# call (conformance/operator/host-guard-common.ps1, host-commands-common.ps1, windows-common.ps1), a host registry
# and a lease state file that the case writes, and times placed relative to the real clock, never within 3 minutes of
# a decision's boundary. The guard has no clock seam (docs/operator.md, guard).
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and removes every
# AWS credential. Before any program that can reach AWS starts, the check confirms under the same environment that
# `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and reports SKIPPED, running nothing,
# if it does not. Every instance ID is made up, and a failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-windows-guard.ps1
. (Join-Path $PSScriptRoot 'host-guard-common.ps1')
. (Join-Path $PSScriptRoot 'windows-common.ps1')

$ErrorActionPreference = 'Stop'

# Problems of the horizon sends of a run. -Expected maps an instance ID to @{ Platform; Time }: one send to each, and
# to no other host; for a Windows host the PowerShell document, the comment and one JSON argument, the command
# naming the horizon file and WriteAllText; for a Linux host the shell document and commands=; and the horizon, the
# one word of 9 to 12 digits in the command, equal to Time to the second.
function Get-WnHorizonProblems([hashtable]$Expected) {
  $p = [System.Collections.Generic.List[string]]::new()
  $sends = @(Get-WnSends)
  foreach ($id in $Expected.Keys) {
    $e = $Expected[$id]
    $mine = @($sends | Where-Object { $_.a -contains $id })
    if ($mine.Count -ne 1) { $p.Add("The host $id was sent $($mine.Count) horizons, not 1"); continue }
    $c = $mine[0]
    if ((Get-WnFlag $c '--comment') -cne 'agentspace guard: lease horizon') { $p.Add("The send to $id has not the comment agentspace guard: lease horizon") }
    if ($e.Platform -ceq 'windows/amd64') {
      foreach ($m in (Get-WnSendProblems $c "The send to the Windows host $id")) { $p.Add($m) }
      $cmd = Get-WnSendCommand $c
      if ($null -eq $cmd) { continue }
      # operator.md's steps 1 and 3 name the folder C:\ProgramData\agentspace and the file lease-horizon in it, and do not
      # say how the command joins them, so the command MAY hold the path whole or the folder and the file name apart.
      if (-not $cmd.Contains('C:\ProgramData\agentspace') -or -not $cmd.Contains('lease-horizon')) { $p.Add('The Windows command does not name C:\ProgramData\agentspace and lease-horizon') }
      if (-not $cmd.Contains('[IO.File]::WriteAllText')) { $p.Add('The Windows command does not write with [IO.File]::WriteAllText') }
    } else {
      if ((Get-WnFlag $c '--document-name') -cne 'AWS-RunShellScript') { $p.Add("The Linux host $id was not sent AWS-RunShellScript") }
      $v = Get-WnFlag $c '--parameters'
      if ($null -eq $v -or -not $v.StartsWith('commands=')) { $p.Add("The Linux host $id's command is not passed as commands="); continue }
      $cmd = $v.Substring('commands='.Length)
    }
    $words = @([regex]::Matches($cmd, '(?<![0-9A-Za-z_.\-/])\d{9,12}(?![0-9A-Za-z_.\-/])') | ForEach-Object { $_.Value })
    if ($words.Count -ne 1) { $p.Add("The command sent to $id holds $($words.Count) words of 9 to 12 digits, not 1"); continue }
    if ($words[0].StartsWith('0')) { $p.Add("The horizon sent to $id has a leading zero") }
    $want = Get-GdUnix $e.Time
    if ([long]$words[0] -ne $want) { $p.Add("The horizon sent to $id is $([long]$words[0] - $want) seconds from the expected one") }
  }
  foreach ($c in $sends) {
    $known = $false
    foreach ($id in $Expected.Keys) { if ($c.a -contains $id) { $known = $true } }
    if (-not $known) { $p.Add('A horizon was sent to a host that was not expected to get one') }
  }
  $p.ToArray()
}
# The send call the guard makes to a Windows host, as the Calls list gives it: the parameters one argument that
# opens with a brace, whatever its text.
function Get-WnCallSendWin([string]$Id) { , @('ssm', 'send-command', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $Id, '--document-name', 'AWS-RunPowerShellScript', '--comment', 'agentspace guard: lease horizon', '--parameters', 're:^\{') }

Invoke-HostFile -FileName 'test-unmet-windows-guard.ps1' -Body {
  param($fx)
  $idBox = $script:IdBox
  $idW = $script:WnIdWin; $idA = $script:WnIdArm; $idL = $script:WnIdAmd
  $project = 'example'
  $wn = 'windows/amd64'
  $hostW = Get-WnHostName $project $wn
  $g = 'operator.md#guard'
  $boxStopped = New-GdInstance -Id $idBox -State 'stopped'
  $three = {
    Set-HostsRecords @((New-BoxRecord $idBox), (New-WnRecord -Project $project -Platform $wn -State 'ready' -InstanceId $script:WnIdWin), (New-WnRecord -Project $project -State 'ready' -InstanceId $script:WnIdArm), (New-WnRecord -Project $project -Platform 'linux/amd64' -State 'ready' -InstanceId $script:WnIdAmd))
  }

  # ============ W28: the horizon sent to each host, by its own platform's leases and document ============
  $cA = @{ Win = (Get-GdAhead 120 22); Arm = (Get-GdAhead 300 33); Amd = (Get-GdAhead 200 44) }
  Test-GuardCase -Name "${g}: W28 a running Windows host a live windows/amd64 lease explains is untouched, and is sent its own platform's horizon alone by the PowerShell document as one JSON argument, while the Linux hosts, whose leases end later, get the shell document and their own horizons, in the file's order" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdClosing 4 3 0 0)))
    Calls = @((Get-GdCallDescribe @($idBox, $idW, $idA, $idL)), (Get-WnCallSendWin $idW), (Get-GdCallSend $idA), (Get-GdCallSend $idL))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -None
      Get-WnHorizonProblems @{ $idW = @{ Platform = $wn; Time = $cA.Win }; $idA = @{ Platform = 'linux/arm64'; Time = $cA.Arm }; $idL = @{ Platform = 'linux/amd64'; Time = $cA.Amd } } } } -Setup {
    & $three
    Set-GdLeases @(
      (New-WnLease -Project $project -State 'ready' -Platform $wn -Expires $cA.Win),
      (New-WnLease -Project $project -State 'ready' -Expires $cA.Arm),
      (New-WnLease -Project $project -State 'ready' -Platform 'linux/amd64' -Expires $cA.Amd),
      (New-WnLease -Project $project -State 'released' -Platform $wn -Expires (Get-GdAgo 100) -Released (Get-GdAgo 90)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $idW -Launch (Get-GdAgo 360)), (New-GdInstance -Id $idA -Launch (Get-GdAgo 360)), (New-GdInstance -Id $idL -Launch (Get-GdAgo 360)))
  }
  # A Windows host that no live lease of its platform explains: its horizon is its launch time, the later of that and
  # the project's last lease end on windows/amd64 (none), however late its Linux leases end.
  $launchW = Get-GdAgo 5 17
  Test-GuardCase -Name "${g}: W28 a running Windows host with no live windows/amd64 lease, inside the grace, is logged unexplained under <project>.windows-amd64, not stopped, and is sent its launch time as its horizon whatever its project's Linux leases hold" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressUnexplained $hostW $idW $launchW), (Get-GdClosing 3 2 0 1)))
    Calls = @((Get-GdCallDescribe @($idBox, $idW, $idA)), (Get-WnCallSendWin $idW), (Get-GdCallSend $idA))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineUnexplained $hostW $launchW))
      Get-WnHorizonProblems @{ $idW = @{ Platform = $wn; Time = $launchW }; $idA = @{ Platform = 'linux/arm64'; Time = $cA.Arm } } } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (New-WnRecord -Project $project -Platform $wn -State 'ready' -InstanceId $script:WnIdWin), (New-WnRecord -Project $project -State 'ready' -InstanceId $script:WnIdArm))
    Set-GdLeases @((New-WnLease -Project $project -State 'ready' -Expires $cA.Arm), (New-WnLease -Project $project -State 'released' -Platform 'linux/amd64' -Expires (Get-GdAgo 60) -Released (Get-GdAgo 50)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $idW -Launch $launchW), (New-GdInstance -Id $idA -Launch (Get-GdAgo 360)))
  }
  # Past the grace: stopped, with the host name in the progress line and the log.
  $launchOld = Get-GdAgo 40 19
  Test-GuardCase -Name "${g}: W28 a running Windows host that only a Linux lease of its project covers is stopped after the grace, and its progress line and log name <project>.windows-amd64, while the arm64 host the lease explains is sent its horizon" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $hostW $idW), (Get-GdClosing 3 2 1 0)))
    Calls = @((Get-GdCallDescribe @($idBox, $idW, $idA)), (Get-CallStop $idW), (Get-GdCallSend $idA))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $hostW $launchOld))
      Get-WnHorizonProblems @{ $idA = @{ Platform = 'linux/arm64'; Time = $cA.Arm } } } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (New-WnRecord -Project $project -Platform $wn -State 'ready' -InstanceId $script:WnIdWin), (New-WnRecord -Project $project -State 'ready' -InstanceId $script:WnIdArm))
    Set-GdLeases @((New-WnLease -Project $project -State 'ready' -Expires $cA.Arm))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $idW -Launch $launchOld), (New-GdInstance -Id $idA -Launch (Get-GdAgo 360)))
  }
}

Exit-ConformanceRun

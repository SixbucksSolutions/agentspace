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
#        `AWS-RunPowerShellScript`, the comment `agentspace guard: lease horizon` and one command that builds the
#        path `C:\ProgramData\agentspace\lease-horizon` exactly once its parts are put together, every path it builds
#        to `lease-horizon` in that folder, and that names `[IO.File]::WriteAllText` and holds the horizon's digits
#        as one word, passed as one `--parameters` argument that parses as JSON, `{"commands":["<command>"]}`,
#        never the shorthand `commands=`; in one pass the Linux hosts get the shell document and the same rule,
#        and the pass visits the hosts, and the describe filter names their IDs, in the file's order; the
#        Windows host's log lines and messages name `<project>.windows-amd64`.
#
# The bytes the command writes and the JSON the guard builds are the crate tests' (docs/conformance.md, W28), so
# this file reads the command from the parsed JSON and asserts what the spec requires of it, never its text. The
# path is found by parsing the command as PowerShell and folding each value from its parts, never by running it.
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

# The file operator.md's "The horizon on a Windows host" moves the horizon over, in step 3, in the folder of step 1.
$script:WnHorizonFile = 'C:\ProgramData\agentspace\lease-horizon'
$script:WnHorizonFolder = 'C:\ProgramData\agentspace'

# The text an expression of the command evaluates to, folded from its parts without running anything, or $null when
# a part is not a string this can follow: a quoted string, a variable an earlier assignment folded, `+` of two such,
# [string] of one, a double-quoted string whose variables are known, Join-Path of two, and [IO.Path]::Combine.
function Get-WnFoldedText($Node, [hashtable]$Known) {
  if ($null -eq $Node) { return $null }
  if ($Node -is [Management.Automation.Language.PipelineAst]) {
    if ($Node.PipelineElements.Count -ne 1) { return $null }
    return Get-WnFoldedText $Node.PipelineElements[0] $Known
  }
  if ($Node -is [Management.Automation.Language.CommandExpressionAst]) { return Get-WnFoldedText $Node.Expression $Known }
  if ($Node -is [Management.Automation.Language.ParenExpressionAst]) { return Get-WnFoldedText $Node.Pipeline $Known }
  if ($Node -is [Management.Automation.Language.StringConstantExpressionAst]) { return $Node.Value }
  if ($Node -is [Management.Automation.Language.ExpandableStringExpressionAst]) {
    $text = $Node.Value
    foreach ($e in $Node.NestedExpressions) {
      if ($e -isnot [Management.Automation.Language.VariableExpressionAst] -or -not $Known.ContainsKey($e.VariablePath.UserPath)) { return $null }
      $text = $text.Replace($e.Extent.Text, $Known[$e.VariablePath.UserPath])
    }
    return $text
  }
  if ($Node -is [Management.Automation.Language.VariableExpressionAst]) {
    if ($Known.ContainsKey($Node.VariablePath.UserPath)) { return $Known[$Node.VariablePath.UserPath] }
    return $null
  }
  if ($Node -is [Management.Automation.Language.ConvertExpressionAst]) {
    if ($Node.Type.TypeName.Name -notin @('string', 'System.String')) { return $null }
    return Get-WnFoldedText $Node.Child $Known
  }
  if ($Node -is [Management.Automation.Language.BinaryExpressionAst]) {
    if ($Node.Operator -ne [Management.Automation.Language.TokenKind]::Plus) { return $null }
    $l = Get-WnFoldedText $Node.Left $Known; $r = Get-WnFoldedText $Node.Right $Known
    if ($null -eq $l -or $null -eq $r) { return $null }
    return $l + $r
  }
  if ($Node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $Node.Static -and
      $Node.Expression.TypeName.FullName -in @('IO.Path', 'System.IO.Path') -and $Node.Member.Value -ieq 'Combine') {
    $parts = @(foreach ($x in $Node.Arguments) { Get-WnFoldedText $x $Known })
    if ($parts.Count -lt 2 -or $parts -contains $null) { return $null }
    return [IO.Path]::Combine([string[]]$parts)
  }
  if ($Node -is [Management.Automation.Language.CommandAst] -and $Node.GetCommandName() -ieq 'Join-Path') {
    $parts = [System.Collections.Generic.List[object]]::new()
    foreach ($x in @($Node.CommandElements | Select-Object -Skip 1)) {
      if ($x -is [Management.Automation.Language.CommandParameterAst]) {
        if ($x.ParameterName -notin @('Path', 'ChildPath')) { return $null }
        if ($null -ne $x.Argument) { $parts.Add((Get-WnFoldedText $x.Argument $Known)) }
      } else { $parts.Add((Get-WnFoldedText $x $Known)) }
    }
    if ($parts.Count -ne 2 -or $parts.Contains($null)) { return $null }
    return ([string]$parts[0]).TrimEnd('\') + '\' + ([string]$parts[1]).TrimStart('\')
  }
  $null
}
# Problems: the path the Windows horizon command writes is not C:\ProgramData\agentspace\lease-horizon once its parts
# are put together (docs/operator.md, "The horizon on a Windows host", steps 1 to 3). The command is parsed as
# PowerShell, never run, and each assignment's value and each argument of a call is folded from its parts in the
# order they come: some value MUST be the file's path exactly, and every value that names lease-horizon MUST be in
# step 1's folder (the file, or step 2's temporary file in that folder). Two words found anywhere in the text are
# not enough, since a command that held both and wrote the file elsewhere would pass.
function Get-WnHorizonPathProblems([string]$Command) {
  $p = [System.Collections.Generic.List[string]]::new()
  $tokens = $null; $errs = $null
  $ast = [Management.Automation.Language.Parser]::ParseInput($Command, [ref]$tokens, [ref]$errs)
  if (@($errs).Count -gt 0) { $p.Add('The Windows command does not parse as PowerShell'); return $p.ToArray() }
  $known = @{}
  $values = [System.Collections.Generic.List[string]]::new()
  $nodes = $ast.FindAll({ param($n) $n -is [Management.Automation.Language.AssignmentStatementAst] -or $n -is [Management.Automation.Language.InvokeMemberExpressionAst] -or $n -is [Management.Automation.Language.CommandAst] }, $true)
  foreach ($n in @($nodes | Sort-Object { $_.Extent.StartOffset })) {
    if ($n -is [Management.Automation.Language.AssignmentStatementAst]) {
      if ($n.Left -isnot [Management.Automation.Language.VariableExpressionAst]) { continue }
      $name = $n.Left.VariablePath.UserPath
      $v = Get-WnFoldedText $n.Right $known
      if ($null -eq $v) { $known.Remove($name) } else { $known[$name] = $v; $values.Add($v) }
    } elseif ($n -is [Management.Automation.Language.InvokeMemberExpressionAst]) {
      # A Combine's arguments are the parts of a path, which the folding of the whole puts together; a part is no path.
      if ($n.Static -and $n.Expression.TypeName.FullName -in @('IO.Path', 'System.IO.Path') -and $n.Member.Value -ieq 'Combine') { continue }
      foreach ($x in @($n.Arguments)) { $v = Get-WnFoldedText $x $known; if ($null -ne $v) { $values.Add($v) } }
    } else {
      if ($n.GetCommandName() -ieq 'Join-Path') { continue }
      foreach ($x in @($n.CommandElements | Select-Object -Skip 1)) {
        $y = if ($x -is [Management.Automation.Language.CommandParameterAst]) { $x.Argument } else { $x }
        $v = Get-WnFoldedText $y $known; if ($null -ne $v) { $values.Add($v) }
      }
    }
  }
  $named = @($values | Where-Object { $_.IndexOf('lease-horizon', [StringComparison]::OrdinalIgnoreCase) -ge 0 } | Select-Object -Unique)
  if (@($named | Where-Object { $_ -ieq $script:WnHorizonFile }).Count -eq 0) {
    $p.Add("The Windows command builds no path that is $($script:WnHorizonFile) once its parts are put together")
  }
  foreach ($v in $named) {
    $i = $v.LastIndexOf('\')
    if ($i -lt 0 -or $v.Substring(0, $i) -ine $script:WnHorizonFolder) { $p.Add("The Windows command builds a path to lease-horizon outside $($script:WnHorizonFolder)"); break }
  }
  $p.ToArray()
}

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
      foreach ($m in (Get-WnHorizonPathProblems $cmd)) { $p.Add($m) }
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

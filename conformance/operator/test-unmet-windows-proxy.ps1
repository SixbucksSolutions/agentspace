#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the ProxyCommand and the token push on `windows/amd64`, and of what no Windows step may be:
# W30, W31 and W32 of docs/conformance.md, "The operator's Windows cases" (docs/operator.md, "From the
# ProxyCommand", "The ProxyCommand", "The token push", "The token push to a Windows host", "The time zone",
# "Secrecy", "The remote command on Windows", "Windows hosts"; D49, D91, D96). Card #0262 (E0.12-T04-S02), from
# #0253's plan.
#
#   W30  A `ready` lease on `windows/amd64` reaches the Windows record's instance whatever the project's other
#        records are, through the tunnel `AWS-StartSSHSession` on port 22; with no `ready` Windows record it is
#        refused and never sent to the box; `agentspace-host.<project>.windows-amd64` reaches the record whatever
#        its state; `windows-arm64` and a name in capitals fail as a failed lookup does; a connection's writes
#        are that host's.
#   W31  The token push to a Windows host: the token on the ssh's stdin alone, the remote step an encoded
#        remote command through the alias, the stamp at the host's own folder, and the zone a Windows zone ID
#        sent inside the encoded script, never converted, checked by its pattern: `Mountain Standard Time` and
#        `Pacific Standard Time (Mexico)` are sent as they are, and `America/Denver` is the empty zone.
#   W32  Every remote command that `bootstrap`, `sync`, `up`, `grow` and the push send a Windows host is of the
#        encoded form, at most 8,191 characters, its base64 holding no `=` and decoding to UTF-16LE, holds no
#        line of a `box\windows` file, and the token is in no argument, environment or file the ssh stand-in can
#        read. (The image ID is in no line of any output: W23's cases, in test-unmet-windows-deploy-sync.ps1,
#        check every run's streams for it. No case seeds or asserts 127.0.0.1 as a Windows host's source.)
#
# EXPECTED TO FAIL AGAINST v0.11's OPERATOR, which has no -Platform and refuses a registry record holding
# `platform`: every case reports a FAIL there, each by its own assertion. A v0.12 build MUST pass every result.
#
# What it does: it runs the ProxyCommand and the operator in a throwaway home with stand-ins for `aws`, `ssh`, `scp`
# and `go` that log every call (conformance/operator/host-commands-common.ps1, windows-common.ps1), a host registry
# and a lease state file that the case writes, and a fake localswim receipt and Bot credential with a made-up token,
# which the ssh stand-in looks for and never prints. No stand-in is a Windows host.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and removes every
# AWS credential. Before any program that can reach AWS starts, the check confirms under the same environment that
# `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and reports SKIPPED, running nothing,
# if it does not. Every instance ID is made up, and a failure reason never quotes the program's output.
#
# The programs under test are the ones AGENTSPACE_SSH_PROXY_CMD and AGENTSPACE_OPERATOR_CMD name; unset, each takes
# its default from conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-windows-proxy.ps1
. (Join-Path $PSScriptRoot 'windows-common.ps1')

$ErrorActionPreference = 'Stop'

# Runs one case of the ProxyCommand: the home is reset, $Setup runs, the program named by AGENTSPACE_SSH_PROXY_CMD
# starts with the arguments, nested unless the case's environment removes AGENTSPACE_NESTED, and the exit code,
# the program's own failure shapes and the case's Check decide the result.
function Test-WnProxyCase {
  param(
    [Parameter(Mandatory)][string]$Name,
    [string[]]$Arguments = @(),
    [hashtable]$Environment = @{},
    [scriptblock]$Setup = {},
    [Parameter(Mandatory)][int]$Exit,
    [scriptblock]$Check = {}
  )
  $fx = $script:Fx
  try {
    Reset-HostCase
    & $Setup
    $e = $fx.Env.Clone()
    $e['AGENTSPACE_NESTED'] = '1'
    foreach ($k in $Environment.Keys) { $e[$k] = $Environment[$k] }
    $started = [DateTime]::UtcNow
    $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD' -ConformanceHome $fx.H -Arguments $Arguments -Environment $e -TimeoutSeconds 120
    $problems = [System.Collections.Generic.List[string]]::new()
    if ($r.TimedOut) { $problems.Add('It timed out') } elseif ($r.ExitCode -ne $Exit) { $problems.Add("Exit $($r.ExitCode), not $Exit") }
    if ($r.Stderr -match 'CategoryInfo|At line:|Exception|panicked') { $problems.Add('Stderr shows a runtime error record') }
    foreach ($m in @(& $Check $r $started)) { if ($m) { $problems.Add([string]$m) } }
    if ($problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
    else { Write-ConformanceResult -Name $Name -Status FAIL -Reason (ConvertTo-ConformanceMasked ($problems -join '; ')) }
  } catch {
    $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
  }
}

Invoke-HostFile -FileName 'test-unmet-windows-proxy.ps1' -Push -Body {
  param($fx)
  $goDir = Initialize-WnStandIns
  Set-WnTokenNeedle $fx.Token
  $project = 'example'
  $wn = 'windows/amd64'
  $idW = $script:WnIdWin; $idA = $script:WnIdArm; $idL = $script:WnIdAmd
  $stackW = Get-WnStack $project $wn
  $aliasW = Get-WnAlias $project $wn
  $hostW = Get-WnHostName $project $wn
  $others = @($idA, $idL, $script:IdBox, (Get-WnStack $project 'linux/arm64'), (Get-WnStack $project 'linux/amd64'), 'agentbox')
  $couldNotReach = 'agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.'
  $soon = (Get-WnNow).AddHours(3)
  $push = @{ AGENTSPACE_NESTED = $null; AGENTSPACE_TIMEZONE = 'Mountain Standard Time' }
  # The project's hosts in an order that is not by platform, the Windows one in $WinState, its Linux hosts in states
  # that would draw a wrong lookup away from it, and the answers of healthy instances.
  $seed = {
    param([string]$WinState = 'ready', [switch]$NoWin, [switch]$NoWinId)
    Remove-WnLeases
    $recs = [System.Collections.Generic.List[string]]::new()
    $recs.Add((New-BoxRecord)); $recs.Add((New-WnRecord -Project $project -State 'failed' -InstanceId $script:WnIdArm))
    if (-not $NoWin) {
      if ($NoWinId) { $recs.Add((New-WnRecord -Project $project -Platform $wn -State $WinState)) } else { $recs.Add((New-WnRecord -Project $project -Platform $wn -State $WinState -InstanceId $script:WnIdWin)) }
    }
    $recs.Add((New-WnRecord -Project $project -Platform 'linux/amd64' -State 'ready' -InstanceId $script:WnIdAmd))
    Set-HostsRecords $recs.ToArray()
    Set-AwsScenario (Merge-Scenario (Get-WnHealthy -Project $project) @{ "cloudformation describe-stacks $stackW" = @(New-Step (New-StackDoc -StackName $stackW -Id $script:WnIdWin -InstanceType (Get-WnType $wn))) })
  }
  $leaseW = New-WnLease -Project $project -State 'ready' -Platform $wn -Expires $soon
  $leaseAlias = Get-WnLeaseAlias $leaseW
  # What a run that reached the host MUST show: describes of its instance alone, one tunnel for it by
  # AWS-StartSSHSession on port 22, and no other host's instance, stack or alias in any call.
  $reached = {
    param($run, [string]$Want, [switch]$StacksAllowed, [switch]$SshAllowed)
    $aws = @(Get-AwsCalls)
    $desc = @($aws | Where-Object { $_.a.Count -ge 2 -and $_.a[1] -ceq 'describe-instances' })
    $tun = @($aws | Where-Object { $_.a.Count -ge 2 -and $_.a[1] -ceq 'start-session' })
    if ($desc.Count -eq 0) { 'No instance was asked about' }
    if (@($desc | Where-Object { (Get-WnFlag $_ '--instance-ids') -cne $Want }).Count -gt 0) { 'An instance other than the host''s was asked about' }
    if ($tun.Count -ne 1) { "The tunnel was started $($tun.Count) times, not once" }
    else {
      if ((Get-WnFlag $tun[0] '--target') -cne $Want) { 'The tunnel is for another instance than the host''s' }
      if ((Get-WnFlag $tun[0] '--document-name') -cne 'AWS-StartSSHSession') { 'The tunnel is not AWS-StartSSHSession' }
      if ((Get-WnFlag $tun[0] '--parameters') -cne 'portNumber=22') { 'The tunnel is not on port 22' }
    }
    foreach ($m in (Get-WnForeignProblems $aws @($others | Where-Object { $_ -cne $Want }) 'An AWS call')) { $m }
    if (-not $StacksAllowed -and @($aws | Where-Object { $_.a.Count -ge 2 -and $_.a[1] -ceq 'describe-stacks' }).Count -gt 0) { 'A stack was asked for, though the host''s record holds the instance ID' }
    if (-not $SshAllowed -and (@(Get-SshCalls).Count + @(Get-ScpCalls).Count) -gt 0) { 'ssh or scp was started' }
  }
  # What a refused connection MUST show: exit 1 (the caller checks it), nothing on stdout, no call at all, and one line.
  $refused = {
    param($run, [string]$Alias)
    if ($run.Stdout.Length -gt 0) { 'Stdout is not empty' }
    if (@(Get-AwsCalls).Count -gt 0) { 'The stand-in aws was called: no call may be made, and so no tunnel' }
    if ((@(Get-SshCalls).Count + @(Get-ScpCalls).Count) -gt 0) { 'ssh or scp was started' }
    $lines = @($run.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })
    if ($lines.Count -ne 1) { "Stderr holds $($lines.Count) lines, not the refusal's one" }
    elseif (-not $lines[0].StartsWith('agentspace: ', [StringComparison]::Ordinal) -or -not $lines[0].Contains('no ready host') -or -not $lines[0].Contains($Alias) -or -not $lines[0].Contains('no connection was opened')) { 'The line is not the refusal naming the lease''s alias, no ready host and that no connection was opened' }
    elseif ($lines[0].Contains($script:WnIdWin) -or $lines[0].Contains('agentspace-host.') -or $lines[0].Contains('windows-amd64')) { 'The refusal names a host' }
  }
  $notReached = { param($run) if (@(Get-AwsCalls | Where-Object { $_.a.Count -ge 2 -and $_.a[1] -ceq 'start-session' }).Count -gt 0) { 'A tunnel was started' }; if ($run.Stdout.Length -gt 0) { 'Stdout is not empty' }; $l = @($run.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' }); if ($l.Count -eq 0 -or $l[-1].Trim() -cne $couldNotReach) { 'The last line on stderr is not the caller''s "Could not reach the host" line' } }

  # ============ W30: the ProxyCommand reaches a Windows host by the lease's project and platform ============
  $s30 = 'operator.md#the-proxycommand'
  Test-WnProxyCase -Name "${s30}: W30 a ready lease of the project on windows/amd64 reaches the Windows record's instance, whatever its arm64 and linux/amd64 records are, with the tunnel AWS-StartSSHSession on port 22" -Arguments @($leaseAlias) -Exit 0 -Setup {
    & $seed
    Set-WnLeases @($leaseW)
  } -Check { param($run, $started) & $reached $run $idW }
  Test-WnProxyCase -Name "${s30}: W30 the same lease reaches the Windows record whatever the other platforms' records are: the arm64 record ready and the linux/amd64 record missing change nothing" -Arguments @($leaseAlias) -Exit 0 -Setup {
    Remove-WnLeases
    Set-HostsRecords @((New-BoxRecord), (New-WnRecord -Project $project -Platform $wn -State 'ready' -InstanceId $script:WnIdWin), (New-WnRecord -Project $project -State 'ready' -InstanceId $script:WnIdArm))
    Set-AwsScenario (Get-WnHealthy -Project $project)
    Set-WnLeases @($leaseW)
  } -Check { param($run, $started) & $reached $run $idW }
  foreach ($case in @(@('no record on the platform', @{ NoWin = $true }), @('a creating record', @{ WinState = 'creating' }), @('a failed record', @{ WinState = 'failed' }))) {
    $label = $case[0]; $args1 = $case[1]
    Test-WnProxyCase -Name "${s30}: W30 a ready lease on windows/amd64 with $label is refused and never sent to the box: exit 1, nothing on stdout, one line, no AWS call and no tunnel" -Arguments @($leaseAlias) -Exit 1 -Setup {
      & $seed @args1
      Set-WnLeases @($leaseW)
    } -Check { param($run, $started) & $refused $run $leaseAlias }
  }
  foreach ($state in 'ready', 'creating', 'failed') {
    $st = $state
    Test-WnProxyCase -Name "${s30}: W30 agentspace-host.<project>.windows-amd64 reaches the Windows record whatever its state ($st)" -Arguments @($aliasW) -Exit 0 -Setup { & $seed $st } -Check { param($run, $started) & $reached $run $idW }
  }
  foreach ($bad in @("agentspace-host.$project.windows-arm64", "agentspace-host.$project.Windows-amd64", "agentspace-host.$project.WINDOWS-AMD64", "agentspace-host.$project.linux-arm64", "agentspace-host.$project.windows")) {
    $b = $bad
    Test-WnProxyCase -Name "${s30}: W30 the alias $b fails as a failed lookup does: the caller's Could not reach the host line, exit 1, no tunnel" -Arguments @($b) -Exit 1 -Setup { & $seed } -Check { param($run, $started) & $notReached $run }
  }
  Test-WnProxyCase -Name "${s30}: W30 a connection's writes are the Windows host's: a missing instance ID is read from its stack and written into its record alone, and the push's stamp is written in its own folder" -Arguments @($aliasW) -Exit 0 -Environment $push -Setup {
    & $seed -NoWinId
  } -Check {
    param($run, $started)
    & $reached $run $idW -StacksAllowed -SshAllowed
    $stacks = @(Get-AwsCalls | Where-Object { $_.a.Count -ge 2 -and $_.a[1] -ceq 'describe-stacks' })
    if ($stacks.Count -lt 1 -or @($stacks | Where-Object { (Get-WnFlag $_ '--stack-name') -cne $stackW }).Count -gt 0) { 'The instance ID was not read from the Windows host''s stack alone' }
    $text = Get-WnHostsText
    $doc = ConvertFrom-Json -InputObject $text -AsHashtable
    $win = @($doc.hosts | Where-Object { $_.ContainsKey('platform') -and $_.platform -ceq $wn })
    if ($win.Count -ne 1 -or $win[0].instance_id -cne $idW) { 'The Windows record does not hold the instance ID read from its stack' }
    foreach ($m in (Get-WnOtherRecordProblems $text @((New-BoxRecord), (New-WnRecord -Project $project -State 'failed' -InstanceId $script:WnIdArm), (New-WnRecord -Project $project -Platform 'linux/amd64' -State 'ready' -InstanceId $script:WnIdAmd)))) { $m }
    $ssh = @(Get-SshCalls)
    if ($ssh.Count -ne 1) { "The push started $($ssh.Count) ssh calls, not 1" }
    if (-not (Test-Path -LiteralPath (Get-HostFiles $hostW).Stamp -PathType Leaf)) { 'The push''s stamp was not written in the Windows host''s own folder' }
    if (Test-Path -LiteralPath (Join-Path $fx.Agentspace 'localswim-pushed.json')) { 'The box''s stamp was written by a connection to another host' }
    if (@(Get-WnHostFolders | Where-Object { $_ -cne $hostW }).Count -gt 0) { 'A host folder of another host was made' }
  }

  # ============ W31: the token push to a Windows host ============
  $s31 = 'operator.md#the-token-push-to-a-windows-host'
  $content = "LOCALSWIM_URL=$($fx.BaseUrl)`nLOCALSWIM_TOKEN=$($fx.Token)`n"
  $stampOf = { param([string]$Id, [string]$Zone) (Get-Sha256Hex ($Id + "`n" + $Zone + "`n" + $content)).ToUpperInvariant() }
  $pushChecks = {
    param($run, [string]$Zone, [bool]$Sent)
    $ssh = @(Get-SshCalls)
    if ($ssh.Count -ne 1) { return "There were $($ssh.Count) ssh calls, not the push's 1" }
    $a = @($ssh[0].a)
    if ($a.Count -ne 6 -or (($a | Select-Object -First 5) -join [char]0) -cne (@('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', $aliasW) -join [char]0)) { 'The push is not -o ClearAllForwardings=yes -o BatchMode=yes <alias> <remote command>' }
    foreach ($m in (Get-WnSshProblems $ssh)) { $m }
    $cmd = Get-WnRemote $ssh[0]
    $decoded = Get-WnScript $cmd
    if ($Sent) {
      if ($null -eq $decoded -or -not $decoded.Contains($Zone)) { 'The zone is not inside the encoded script' }
    } elseif ($null -ne $decoded -and $decoded.Contains($Zone)) { 'The zone, which is not a valid Windows zone ID, was sent' }
    if (($a -join ' ').Contains($Zone)) { 'The zone is on the command line' }
    $in = Get-FileBytesOrNull (Join-Path $fx.Standins 'ssh-stdin-1.bin')
    if ($null -eq $in) { return 'No standard input was recorded for the push' }
    $skip = if ($in.Length -ge 3 -and $in[0] -eq 0xEF -and $in[1] -eq 0xBB -and $in[2] -eq 0xBF) { 3 } else { 0 }
    if ([Text.UTF8Encoding]::new($false).GetString($in, $skip, $in.Length - $skip) -cne $content) { 'The push''s standard input is not the two content lines' }
    $sentZone = if ($Sent) { $Zone } else { '' }
    $stamp = (Get-HostFiles $hostW).Stamp
    if (-not (Test-Path -LiteralPath $stamp -PathType Leaf)) { 'The Windows host''s stamp was not written' }
    else {
      try { $got = (ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($stamp))).stamp } catch { $got = '<not JSON>' }
      if ($got -cne (& $stampOf $script:WnIdWin $sentZone)) { 'The stamp does not cover the instance ID, the zone as sent and the content' }
    }
    if (Test-Path -LiteralPath (Join-Path $fx.Agentspace 'localswim-pushed.json')) { 'The box''s stamp was written' }
    $warn = "agentspace: Could not work out the laptop's time zone; times on the box stay in its own zone."
    $has = @($run.Stderr -split "`n" | Where-Object { $_ -ceq $warn }).Count
    if ($Sent -and $has -ne 0) { 'The no-zone warning was printed for a zone that was sent' }
    if (-not $Sent -and $has -ne 1) { 'The no-zone warning was not printed once for the empty zone' }
    if ($run.Stderr.Contains($fx.Token) -or $run.Stdout.Contains($fx.Token)) { 'A stream holds the token' }
  }
  foreach ($zone in @(@('Mountain Standard Time', $true), @('Pacific Standard Time (Mexico)', $true), @('America/Denver', $false), @('Mountain Standard Time-', $false), @('1Mountain Standard Time', $false))) {
    $z = $zone[0]; $isSent = [bool]$zone[1]
    $verb = if ($isSent) { 'is sent as it is, inside the encoded script' } else { 'does not match the Windows zone pattern, so it is the empty zone and is not sent' }
    Test-HostCase "${s31}: W31 up -Host <project> -Platform windows/amd64 with the zone '$z' pushes the token on stdin alone through one encoded remote command, the zone $verb, and the stamp covers the zone as sent" @('up', '-Host', $project, '-Platform', $wn) @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'
      Check = { param($run, $started) & $pushChecks $run $z $isSent } } -Setup { & $seed } -Environment @{ AGENTSPACE_TIMEZONE = $z }
  }
  Test-HostCase "${s31}: W31 a push the host's own stamp says is not due sends nothing: a second up of the host starts no ssh" @('up', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = @()
    Check = { param($run, $started)
      $stamp = (Get-HostFiles $hostW).Stamp
      if (-not (Test-Path -LiteralPath $stamp -PathType Leaf)) { 'The stamp was lost' } } } -Setup {
    & $seed
    $null = Invoke-HostRun -Arguments @('up', '-Host', $project, '-Platform', $wn) -Environment @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' }
    Clear-StandinLogs
    & $seed
  } -Environment @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' }

  # ============ W32: no Windows step is too long or too open ============
  $s32 = 'operator.md#the-remote-command-on-windows'
  $sweep = {
    param($run)
    $ssh = @(Get-SshCalls)
    if ($ssh.Count -eq 0) { 'No ssh was started' }
    foreach ($m in (Get-WnSshProblems $ssh)) { $m }
    if ($run.Stdout.Contains($script:WnImage) -or $run.Stderr.Contains($script:WnImage)) { 'A line the operator printed holds the image ID' }
    if ($run.Stdout.Contains($fx.Token) -or $run.Stderr.Contains($fx.Token)) { 'A stream holds the token' }
  }
  $seedSweep = { & $seed; Set-WnClaudeMd }
  $withGoEnv = if (Test-WnLocalswimFolder) { @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' } } else { @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' } }
  Test-HostCase "${s32}: W32 every remote command bootstrap sends a Windows host is of the encoded form, within 8,191 characters, with no line of a box\windows file, and the token is out of reach of the ssh" @('bootstrap', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $sweep $run } } -Setup $seedSweep -Environment $withGoEnv
  Test-HostCase "${s32}: W32 every remote command sync sends a Windows host, its push's included, is of the encoded form, within 8,191 characters, with no line of a box\windows file, and the token is out of reach of the ssh" @('sync', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $sweep $run } } -Setup $seedSweep -Environment $withGoEnv
  Test-HostCase "${s32}: W32 the remote command up's push sends a Windows host is of the encoded form, within 8,191 characters, with no line of a box\windows file, and the token is out of reach of the ssh" @('up', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; Check = { param($run, $started) & $sweep $run } } -Setup $seedSweep -Environment $withGoEnv
  Test-HostCase "${s32}: W32 the remote command grow sends a Windows host is of the encoded form, within 8,191 characters, with no line of a box\windows file, and the token is out of reach of the ssh" @('grow', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; Check = { param($run, $started) & $sweep $run } } -Setup $seedSweep -Environment $withGoEnv
  Test-WnProxyCase -Name "${s32}: W32 the remote command a connection's push sends a Windows host through the ProxyCommand is of the encoded form, within 8,191 characters, with no line of a box\windows file, and the token is out of reach of the ssh" -Arguments @($aliasW) -Exit 0 -Environment $push -Setup $seedSweep -Check { param($run, $started) & $sweep $run }
}

Exit-ConformanceRun

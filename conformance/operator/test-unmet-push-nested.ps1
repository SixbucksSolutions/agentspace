#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of #0167, "The host is ready." printed twice and a push at every start
# (docs/operator.md, "The token push", "The stamp", "What the laptop sends", "What a caller sees",
# "Every ssh runs the ProxyCommand", "up"; D61; P14, architect, 2026-11-21, for v0.08-T01; story
# E0.08-T04-S01, #0182), run through the test seam (docs/operator.md, "The test seam"). It shows the
# row of operator.md's v0.08 table that begins "#0167:".
#
# EXPECTED TO FAIL AGAINST v0.07's PROGRAMS, which print the nested line twice and whose push does
# not choose its ssh's stderr by what runs it. A build of v0.08 MUST pass every result line. The
# result lines still say FAIL: nothing turns a FAIL into a pass.
#
# What it does: it writes into a throwaway home the host registry (the box and one other host), a fake
# localswim receipt and Bot credential, a made-up token, a stand-in `aws` and a stand-in `ssh`.
#   - The stand-in `aws` answers `up`'s calls and the tunnel. Its instance is `running` and launched
#     long ago, so that `up` does not wait, until a case removes started.flag beside it: then it
#     answers `stopped`, an `ec2 start-instances` makes it `running`, and the wait through Session
#     Manager answers `Online` and `patched`, so that a case reaches a start.
#   - The stand-in `ssh` runs the ProxyCommand program nested, with the host alias it was given,
#     before it answers as the box does ("What the box does"), as the real ssh runs its alias's
#     ProxyCommand ("Every ssh runs the ProxyCommand"). The nested program's stderr becomes the
#     stand-in's stderr, after which the stand-in adds ssh's own `Warning: Permanently added ...`
#     line, or, in the failing scenario, two error lines of its own and exit 255. It records its
#     arguments, one line per nested run, and nothing that holds the token.
# It runs the ProxyCommand (AGENTSPACE_NESTED unset, so its push runs) and a person's `up`, and checks
# what it can see: the stand-ins' logs, the stamp, the exit code, stdout and the lines of stderr.
#
# Cases:
#   1. A ProxyCommand connection whose push is due: one `The host is ready.`, one push, one nested
#      run; ssh's own line passes through once and no `Sending` line is printed; the stamp is written.
#   2. A current stamp, the ProxyCommand: no push, and one `The host is ready.`.
#   3. A current stamp, a stopped host, the ProxyCommand: the host is started, and no push.
#   4. A current stamp, a stopped host, a person's `up`: the host is started, and no push.
#   5. AGENTSPACE_DIAGNOSTIC set: two `The host is ready.` lines, and ssh's own line once; set and
#      empty, one.
#   6. A push that fails: ssh's error lines still shown, in order and once each, one `The host is
#      ready.`, `agentspace: The host did not confirm ...` warning, no stamp, exit 0.
#   7. A person's `up` whose push is due: its nested ProxyCommand's `The host is ready.` is printed.
#   8. Another host reached by its host alias: one `The host is ready.`, pushed over that alias with
#      that host's stamp.
#   9. The ssh of the push carries AGENTSPACE_NESTED (read by the stand-in, never printed).
#
# SAFETY. The token is a made-up value. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA
# into the throwaway home and removes every AWS credential, so a mistaken run cannot find Terry's
# receipt or token. Before any program that can reach AWS starts, the check confirms under the same
# environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and
# reports SKIPPED, running nothing, if it does not. `scp` is a tripwire. The stand-in `ssh` starts the
# ProxyCommand program only, and ends it. A failure reason never quotes a program's output, and holds
# no token.
#
# The programs under test are the ones AGENTSPACE_OPERATOR_CMD and AGENTSPACE_SSH_PROXY_CMD name; unset,
# each takes its default from conformance/defaults.ps1 (docs/conformance.md, "Naming the program under
# test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-push-nested.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')

$sendSec = 'operator.md#what-the-laptop-sends'
$stampSec = 'operator.md#the-stamp'
$callerSec = 'operator.md#what-a-caller-sees-d1-card-0087'
$checksSec = 'operator.md#what-the-laptop-checks'
$upSec = 'operator.md#up'
$zone = 'America/New_York'
$ready = 'The host is ready.'
$sshOwn = "Warning: Permanently added 'conformance-host' (ED25519) to the list of known hosts."
$sshErr1 = 'ssh: conformance error line one'
$sshErr2 = 'ssh: conformance error line two'
$personSending = 'Sending the localswim Bot token to the instance...'
$commandId = 'c0ffee00-1111-4222-8333-abcdefabcdef'

# The stand-in ssh: a Windows PowerShell 5.1 script that a .cmd starts, so that it sees the arguments as
# a program would. The alias is its fifth argument (-o ClearAllForwardings=yes -o BatchMode=yes <alias>).
$nestedSshScript = @'
$dir = $PSScriptRoot
$scenario = (Get-Content -LiteralPath (Join-Path $dir 'ssh-scenario.txt') -Raw).Trim()
$zone = (Get-Content -LiteralPath (Join-Path $dir 'ssh-zone.txt') -Raw).Trim()
$proxy = (Get-Content -LiteralPath (Join-Path $dir 'ssh-proxy-path.txt') -Raw).Trim()
[IO.File]::AppendAllText((Join-Path $dir 'ssh-args.log'), ((ConvertTo-Json -InputObject @($args) -Compress) + "`n"))
$alias = [string]$args[4]
# As the real ssh runs its alias's ProxyCommand: the box's alias passes no argument, every other alias
# passes its own name. The ProxyCommand's stdin is closed, its stdout is the connection (discarded
# here), and its stderr is ssh's own stderr.
$psi = New-Object Diagnostics.ProcessStartInfo
$psi.FileName = $proxy
if ($alias -ne 'agentspace') { $psi.Arguments = $alias }
$psi.UseShellExecute = $false
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$nested = $psi.EnvironmentVariables['AGENTSPACE_NESTED']
$state = 'unset'
if ([string]::IsNullOrEmpty($nested)) { $psi.EnvironmentVariables['AGENTSPACE_NESTED'] = '1' } else { $state = 'set' }
$p = [Diagnostics.Process]::Start($psi)
$p.StandardInput.Close()
$outTask = $p.StandardOutput.ReadToEndAsync()
$errTask = $p.StandardError.ReadToEndAsync()
if (-not $p.WaitForExit(90000)) { try { $p.Kill() } catch { } }
[void]$outTask.Wait(10000)
[void]$errTask.Wait(10000)
[IO.File]::AppendAllText((Join-Path $dir 'nested-runs.log'), ('alias=' + $(if ($alias -eq 'agentspace') { 'box' } else { $alias }) + ' nested-env=' + $state + "`n"))
if ($errTask.IsCompleted) { [Console]::Error.Write($errTask.Result) }
$m = New-Object IO.MemoryStream
[Console]::OpenStandardInput().CopyTo($m)
$bytes = $m.ToArray()
if ($scenario -eq 'fail') {
  [Console]::Error.Write("ssh: conformance error line one`n")
  [Console]::Error.Write("ssh: conformance error line two`n")
  exit 255
}
[Console]::Error.Write("Warning: Permanently added 'conformance-host' (ED25519) to the list of known hosts.`n")
$text = [Text.Encoding]::UTF8.GetString($bytes).Replace("`r", '')
if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
$lines = @($text -split "`n" | Where-Object { $_ -ne '' })
$norm = (($lines -join "`n") + "`n")
$sha = [Security.Cryptography.SHA256]::Create()
$hash = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($norm))).Replace('-', '').ToLowerInvariant()
[Console]::Out.Write($hash + '  -' + "`n")
if ($zone -ne '') { [Console]::Out.Write('zone ' + $zone + "`n") }
exit 0
'@

$h = New-ConformanceHome
try {
  $nameSeam = "${sendSec}: The push's ssh runs the ProxyCommand nested, through the test seam"
  $proxyProgram = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $operatorProgram = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $nameSeam -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $proxyProgram -or -not $operatorProgram) {
    Write-ConformanceResult -Name $nameSeam -Status FAIL -Reason 'Neither a conformance variable nor its default names the ProxyCommand program or the operator'
  } else {
    # ---- The stand-ins ----
    $dir = Join-Path $h.Root 'standins'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $dir) | Out-Null
    $aws = New-OperatorStandIn -ConformanceHome $h -Name 'aws' -Body @(
      'set "D=%~dp0"',
      'echo %* >>"%D%aws-full.log"',
      'if /i "%~1"=="ssm" if /i "%~2"=="start-session" goto tunnel',
      'if /i "%~1"=="ssm" if /i "%~2"=="describe-instance-information" goto dii',
      'if /i "%~1"=="ssm" if /i "%~2"=="send-command" goto send',
      'if /i "%~1"=="ssm" if /i "%~2"=="get-command-invocation" goto get',
      'if /i "%~1"=="ec2" if /i "%~2"=="describe-instances" goto describe',
      'if /i "%~1"=="ec2" if /i "%~2"=="start-instances" goto start',
      'exit /b 0',
      ':tunnel',
      'echo tunnel-says-hello',
      'exit /b 0',
      ':dii',
      'echo {"InstanceInformationList":[{"PingStatus":"Online"}]}',
      'exit /b 0',
      ':send',
      "echo {`"Command`":{`"CommandId`":`"$commandId`"}}",
      'exit /b 0',
      ':get',
      'echo {"Status":"Success","StandardOutputContent":"patched"}',
      'exit /b 0',
      ':start',
      'echo started>"%D%started.flag"',
      'echo {}',
      'exit /b 0',
      ':describe',
      'if exist "%D%started.flag" goto running',
      'echo {"Reservations":[{"Instances":[{"State":{"Name":"stopped"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}',
      'exit /b 0',
      ':running',
      'echo {"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}',
      'exit /b 0')
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $dir 'ssh-standin.ps1')), $nestedSshScript)
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $dir 'ssh-proxy-path.txt')), $proxyProgram)
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $dir 'ssh-zone.txt')), $zone)
    $ssh = New-OperatorStandIn -ConformanceHome $h -Name 'ssh-nested' -Body @(
      'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0ssh-standin.ps1" %*',
      'exit /b %ERRORLEVEL%')
    $scp = New-OperatorStandIn -ConformanceHome $h -Name 'scp-tripwire' -Body @('exit /b 255')
    $S = [pscustomobject]@{ Aws = $aws; Ssh = $ssh; Scp = $scp; Dir = $dir }
    $fx = New-HpPushFixture $h
    $agentspaceDir = Join-Path $h.UserProfile '.agentspace'
    $boxStamp = Join-Path $agentspaceDir 'localswim-pushed.json'
    $otherStamp = Join-Path $agentspaceDir 'hosts\other\localswim-pushed.json'
    $flag = Join-Path $dir 'started.flag'
    Set-HpRegistry $h @(
      (New-HpHostRecord 'swimcoach' 'ready' $HpIdBox),
      (New-HpHostRecord 'other' 'ready' $HpIdOther))
    $push = @{ AGENTSPACE_TIMEZONE = $zone }

    # ---- Helpers over the stand-ins' logs and the run's stderr ----
    function Set-Scenario([string]$Scenario) {
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $dir 'ssh-scenario.txt')), $Scenario)
    }
    function Set-CurrentStamp([string]$Path, [string]$InstanceId) {
      Set-HpFile $h $Path ('{"version":0,"stamp":"' + (Get-HpExpectedStamp $fx $InstanceId $zone) + '"}')
    }
    # Each case starts from no log, a running host and the stand-in's ok scenario.
    function Initialize-Case([switch]$Stopped) {
      Clear-HpLogs $h $S
      Remove-HpPath $h $flag
      if (-not $Stopped) { Set-HpFile $h $flag 'started' }
      Set-Scenario 'ok'
    }
    function Get-StderrLines($Run) { @([string]$Run.Stderr -split "`r?`n" | Where-Object { $_ -ne '' }) }
    function Get-LineCount($Run, [string]$Line) { @(Get-StderrLines $Run | Where-Object { $_ -ceq $Line }).Count }
    function Get-LineIndexes([string[]]$Lines, [string]$Line) {
      for ($k = 0; $k -lt $Lines.Count; $k++) { if ($Lines[$k] -ceq $Line) { $k } }
    }
    function Get-NestedRuns { @(Get-HpLines (Join-Path $dir 'nested-runs.log')) }
    function Get-StartCount { @((Get-HpAsked $S).Raw | Where-Object { $_ -match '^ec2\s+start-instances\b' }).Count }
    # The problems every case shares: no timeout, exit 0, and nothing on stdout but the tunnel's (the
    # ProxyCommand) or nothing at all (a person's up).
    function Get-RunProblems($Run, [switch]$Operator) {
      $p = New-HpProblems
      if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
      if ($Run.ExitCode -ne 0) { $p.Add("Exit $($Run.ExitCode), not 0") }
      if ($Operator) { if ($Run.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') } }
      elseif ($Run.Stdout.Trim() -cne 'tunnel-says-hello') { $p.Add('Stdout is not exactly the tunnel''s output') }
      if ($Run.Stderr -match 'CategoryInfo|At line:|Exception') { $p.Add('Stderr shows a runtime error record') }
      , $p
    }
    function Add-PushCountProblems($Problems, [int]$Want) {
      $n = Get-HpPushCount $S
      if ($n -ne $Want) { $Problems.Add("The push's ssh was started $n times, not $Want") }
      $nested = (Get-NestedRuns).Count
      if ($nested -ne $Want) { $Problems.Add("The nested ProxyCommand ran $nested times, not $Want") }
    }

    # ---- 1. A ProxyCommand connection whose push is due ----
    Initialize-Case
    Remove-HpPath $h $boxStamp
    $r = Invoke-HpProxy $h $S @() $push -Push
    if ((Get-HpAsked $S).Raw.Count -eq 0 -or (Get-StandInCallCount -Log (Join-Path $dir 'ssh-nested-calls.log')) -eq 0) {
      Write-ConformanceResult -Name $nameSeam -Status FAIL -Reason 'Seam not met: the stand-in aws or the stand-in ssh was not used, so nothing below can be shown'
    } else {
      Write-ConformanceResult -Name $nameSeam -Status PASS

      $p = Get-RunProblems $r
      Add-PushCountProblems $p 1
      $n = Get-LineCount $r $ready
      if ($n -ne 1) { $p.Add("The connection printed 'The host is ready.' $n times, not once") }
      Write-HpProblems "${sendSec}: A ProxyCommand connection whose push is due prints The host is ready. once, though its push's ssh ran the nested ProxyCommand" $p

      $p = New-HpProblems
      $n = Get-LineCount $r $sshOwn
      if ($n -ne 1) { $p.Add("The push's ssh's own stderr line passed to stderr $n times, not once") }
      if (@(Get-StderrLines $r | Where-Object { $_ -match '^\s*(agentspace: )?Sending\b' }).Count -gt 0) { $p.Add('Stderr holds a Sending line') }
      Write-HpProblems "${callerSec}: A ProxyCommand connection passes its push's ssh's other stderr lines once, and prints no Sending line" $p

      $p = New-HpProblems
      $v = Get-HpStampValue $boxStamp
      if ($null -eq $v) { $p.Add('No stamp was written') }
      elseif ($v -cne (Get-HpExpectedStamp $fx $HpIdBox $zone)) { $p.Add('The stamp is not the SHA-256 of <instance ID> LF <zone> LF <content>') }
      Write-HpProblems "${sendSec}: The push whose stderr is filtered still sends the token and is confirmed, and the stamp is written" $p

      # ---- 2. A current stamp, the ProxyCommand ----
      Initialize-Case
      Set-CurrentStamp $boxStamp $HpIdBox
      $before = Get-HpBytesHash $boxStamp
      $r = Invoke-HpProxy $h $S @() $push -Push
      $p = Get-RunProblems $r
      Add-PushCountProblems $p 0
      $n = Get-LineCount $r $ready
      if ($n -ne 1) { $p.Add("The connection printed 'The host is ready.' $n times, not once") }
      if ((Get-HpAsked $S).Raw.Count -eq 0) { $p.Add('The stand-in aws was never called') }
      if ((Get-HpBytesHash $boxStamp) -cne $before) { $p.Add('The stamp changed') }
      Write-HpProblems "${stampSec}: A ProxyCommand connection with a current stamp pushes nothing and prints The host is ready. once" $p

      # ---- 3. A current stamp, a stopped host, the ProxyCommand ----
      Initialize-Case -Stopped
      Set-CurrentStamp $boxStamp $HpIdBox
      $before = Get-HpBytesHash $boxStamp
      $r = Invoke-HpProxy $h $S @() $push -Push
      $p = Get-RunProblems $r
      if ((Get-StartCount) -ne 1) { $p.Add("The host was asked to start $(Get-StartCount) times, not once, so the case shows nothing") }
      Add-PushCountProblems $p 0
      $n = Get-LineCount $r $ready
      if ($n -ne 1) { $p.Add("The connection printed 'The host is ready.' $n times, not once") }
      if ((Get-HpBytesHash $boxStamp) -cne $before) { $p.Add('The stamp changed') }
      Write-HpProblems "${stampSec}: A ProxyCommand connection that starts a stopped host with a current stamp pushes nothing (a start does not make the push due)" $p

      # ---- 4. A current stamp, a stopped host, a person's up ----
      Initialize-Case -Stopped
      Set-CurrentStamp $boxStamp $HpIdBox
      $before = Get-HpBytesHash $boxStamp
      $r = Invoke-HpOperator $h $S @('up') $push
      $p = Get-RunProblems $r -Operator
      if ((Get-StartCount) -ne 1) { $p.Add("The host was asked to start $(Get-StartCount) times, not once, so the case shows nothing") }
      Add-PushCountProblems $p 0
      if (@(Get-StderrLines $r | Where-Object { $_ -cmatch '^Ready: ' }).Count -ne 1) { $p.Add('Stderr lacks its Ready line, once') }
      if (@(Get-StderrLines $r | Where-Object { $_ -cmatch '^Sending the localswim' }).Count -gt 0) { $p.Add('Stderr holds the Sending line') }
      if ((Get-HpBytesHash $boxStamp) -cne $before) { $p.Add('The stamp changed') }
      Write-HpProblems "${stampSec}: A person's up that starts a stopped host with a current stamp pushes nothing (a start does not make the push due)" $p

      # ---- 5. AGENTSPACE_DIAGNOSTIC drops nothing ----
      Initialize-Case
      Remove-HpPath $h $boxStamp
      $r = Invoke-HpProxy $h $S @() ($push + @{ AGENTSPACE_DIAGNOSTIC = '1' }) -Push
      $p = Get-RunProblems $r
      Add-PushCountProblems $p 1
      $n = Get-LineCount $r $ready
      if ($n -ne 2) { $p.Add("The connection printed 'The host is ready.' $n times, not twice") }
      $n = Get-LineCount $r $sshOwn
      if ($n -ne 1) { $p.Add("The push's ssh's own stderr line passed to stderr $n times, not once") }
      Write-HpProblems "${sendSec}: With AGENTSPACE_DIAGNOSTIC set, a ProxyCommand connection whose push is due prints The host is ready. twice, the push's ssh's own lines dropping nothing" $p

      Initialize-Case
      Remove-HpPath $h $boxStamp
      $r = Invoke-HpProxy $h $S @() ($push + @{ AGENTSPACE_DIAGNOSTIC = '' }) -Push
      $p = Get-RunProblems $r
      Add-PushCountProblems $p 1
      $n = Get-LineCount $r $ready
      if ($n -ne 1) { $p.Add("The connection printed 'The host is ready.' $n times, not once") }
      Write-HpProblems "${callerSec}: With AGENTSPACE_DIAGNOSTIC set and empty, a ProxyCommand connection prints The host is ready. once, as when it is unset" $p

      # ---- 6. A push that fails still shows its ssh's error lines ----
      Initialize-Case
      Set-Scenario 'fail'
      Remove-HpPath $h $boxStamp
      $r = Invoke-HpProxy $h $S @() $push -Push
      $p = Get-RunProblems $r
      Add-PushCountProblems $p 1
      $lines = @(Get-StderrLines $r)
      $i1 = @(Get-LineIndexes $lines $sshErr1)
      $i2 = @(Get-LineIndexes $lines $sshErr2)
      if ($i1.Count -ne 1) { $p.Add("The push's ssh's first error line passed to stderr $($i1.Count) times, not once") }
      if ($i2.Count -ne 1) { $p.Add("The push's ssh's second error line passed to stderr $($i2.Count) times, not once") }
      if ($i1.Count -eq 1 -and $i2.Count -eq 1 -and $i1[0] -ge $i2[0]) { $p.Add('The error lines are not in the order the ssh wrote them') }
      $n = Get-LineCount $r $ready
      if ($n -ne 1) { $p.Add("The connection printed 'The host is ready.' $n times, not once") }
      Write-HpProblems "${sendSec}: A push that fails still shows its ssh's error lines, in order and once each, and the connection prints The host is ready. once" $p

      $p = New-HpProblems
      if ($null -ne (Get-HpStampValue $boxStamp)) { $p.Add('A stamp was written for a push the box did not confirm') }
      if (@($lines | Where-Object { $_ -cmatch '^agentspace: .*did not confirm' }).Count -ne 1) { $p.Add('Stderr lacks the one warning that the host did not confirm the token') }
      Write-HpProblems "${checksSec}: A push whose ssh fails warns that the host did not confirm, writes no stamp, and the connection still exits 0" $p

      # ---- 7. A person's up whose push is due ----
      Initialize-Case
      Remove-HpPath $h $boxStamp
      $r = Invoke-HpOperator $h $S @('up') $push
      $p = Get-RunProblems $r -Operator
      Add-PushCountProblems $p 1
      $n = Get-LineCount $r $ready
      if ($n -ne 1) { $p.Add("A person's up printed its nested ProxyCommand's 'The host is ready.' $n times, not once") }
      $n = Get-LineCount $r $sshOwn
      if ($n -ne 1) { $p.Add("The push's ssh's own stderr line passed to stderr $n times, not once") }
      if ((Get-LineCount $r $personSending) -ne 1) { $p.Add('Stderr lacks the Sending line, once') }
      if (@(Get-StderrLines $r | Where-Object { $_ -cmatch '^Ready: ' }).Count -ne 1) { $p.Add('Stderr lacks its Ready line, once') }
      $v = Get-HpStampValue $boxStamp
      if ($v -cne (Get-HpExpectedStamp $fx $HpIdBox $zone)) { $p.Add('The stamp was not written for the confirmed push') }
      Write-HpProblems "${upSec}: A person's up whose push is due still prints the nested ProxyCommand's The host is ready., with ssh's own lines passed through" $p

      # ---- 8. Another host, reached by its host alias ----
      Initialize-Case
      Remove-HpPath $h $otherStamp
      $r = Invoke-HpProxy $h $S @('agentspace-host.other') $push -Push
      $p = Get-RunProblems $r
      Add-PushCountProblems $p 1
      $n = Get-LineCount $r $ready
      if ($n -ne 1) { $p.Add("The connection printed 'The host is ready.' $n times, not once") }
      $a = Get-HpPushArgs $S 1
      if ($null -eq $a -or $a.Count -ne 6 -or $a[4] -cne 'agentspace-host.other') { $p.Add('The push was not made over the host''s alias') }
      $nest = @(Get-NestedRuns)
      if ($nest.Count -ne 1 -or -not $nest[0].StartsWith('alias=agentspace-host.other ')) { $p.Add('The nested ProxyCommand did not run for the host''s alias') }
      if ((Get-HpStampValue $otherStamp) -cne (Get-HpExpectedStamp $fx $HpIdOther $zone)) { $p.Add('The host''s stamp was not written') }
      Write-HpProblems "${sendSec}: A ProxyCommand connection to a host by its host alias, whose push is due, prints The host is ready. once, and pushes over that alias with that host's stamp" $p

      # ---- 9. The push's ssh carries AGENTSPACE_NESTED, so its nested ProxyCommand never pushes ----
      $p = New-HpProblems
      $unset = @(Get-NestedRuns | Where-Object { $_ -notmatch ' nested-env=set$' }).Count
      if ($unset -gt 0) { $p.Add('The push''s ssh did not carry AGENTSPACE_NESTED') }
      Write-HpProblems "operator.md#environment-variables: The ssh the ProxyCommand's push starts carries AGENTSPACE_NESTED, so its nested ProxyCommand never pushes" $p
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-push-nested.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

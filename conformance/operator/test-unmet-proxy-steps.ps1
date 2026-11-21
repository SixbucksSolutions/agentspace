#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the ProxyCommand's steps (docs/operator.md, "The ProxyCommand"): it puts the
# Session Manager plugin's folder and the AWS CLI's folder first on PATH, runs `up`, stops with exit 1
# and starts no tunnel when `up` fails, reads the instance ID from the cache, runs the tunnel for that
# instance with its own stdin and stdout, writes to stdout nothing but the tunnel, and exits with the
# tunnel's exit code. Run through the test seam (docs/operator.md, "The test seam").
#
# EXPECTED TO FAIL AGAINST THE PROTOTYPE, because it does not meet the seam: it runs the AWS CLI by a
# fixed path and reads none of AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD or AGENTSPACE_SCP_CMD (card
# #0085; docs/operator.md, "Requirements the prototype does not meet yet"). Until a build calls the
# stand-in, the file reports `FAIL ... seam not met` and checks nothing more. A rewrite MUST pass
# every result line. The result lines still say FAIL: nothing turns a FAIL into a pass. The argument
# refusals are test-proxy-arguments.ps1's, and what a caller sees (D1) is
# test-unmet-proxy-connect-time.ps1's.
#
# What it does, when the seam is met: it writes into a throwaway home the instance-ID cache and a
# stand-in `aws` that reports a running instance launched long ago, logs every call's full arguments
# and its PATH, copies its stdin to a file, writes a marker line to stdout for a tunnel call, and exits
# 7 for it. It runs the ProxyCommand with and without an alias, and once with an `aws` whose every call
# fails.
#
# SAFETY. The program under test can call the real AWS CLI when the seam is not met. The harness
# gives it a throwaway home with USERPROFILE, HOME, LOCALAPPDATA, APPDATA and the state and ssh
# folders in it, the AWS configuration files pointed at files that do not exist, instance metadata
# off, and every AWS credential variable removed. Before the proxy runs, the check confirms under that
# environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and
# reports SKIPPED, running nothing, if it does not. AGENTSPACE_NESTED is set, so `up` pushes no
# token, and AGENTSPACE_SSH_CMD and AGENTSPACE_SCP_CMD name tripwires that no step may call.
#
# The program under test is the one AGENTSPACE_SSH_PROXY_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test"). A failure
# reason never quotes the program's output, which can hold an instance ID.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-proxy-steps.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#the-proxycommand'
$seamSec = 'operator.md#the-test-seam'
$fakeId = 'i-' + ('0' * 17)
$alias = 'conformance-proxy-alias'
$sessionFolder = 'C:\Program Files\Amazon\SessionManagerPlugin\bin'
$awsFolder = 'C:\Program Files\Amazon\AWSCLIV2'

function Get-Problems($Run, [int]$ExitCode) {
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.TimedOut) { $p.Add('It timed out') }
  elseif ($Run.ExitCode -ne $ExitCode) { $p.Add("Exit $($Run.ExitCode), not $ExitCode") }
  , $p
}
function Write-Problems([string]$Name, $Problems) {
  if ($Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS } else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
}
function Get-Lines([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return @() }
  @(Get-Content -LiteralPath $Path | Where-Object { $_.Trim() -ne '' })
}
function Remove-StandInLogs($Lab, [string]$Dir) {
  foreach ($f in Get-ChildItem -LiteralPath $Dir -Filter '*.log' -File -ErrorAction SilentlyContinue) {
    Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $f.FullName) -Force
  }
  foreach ($f in Get-ChildItem -LiteralPath $Dir -Filter '*-stdin.txt' -File -ErrorAction SilentlyContinue) {
    Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $f.FullName) -Force
  }
}

$h = New-ConformanceHome
try {
  $nameA = "${seamSec}: the ProxyCommand runs AGENTSPACE_AWS_CMD"
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $nameA -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name $nameA -Status FAIL -Reason 'Neither AGENTSPACE_SSH_PROXY_CMD nor its default names a program'
  } else {
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $h.UserProfile '.agentspace\hosts.json')), ('{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","instance_id":"' + $fakeId + '"}]}'))
    $awsOk = New-OperatorStandIn -ConformanceHome $h -Name 'aws-ok' -Body @(
      'echo %*>>"%~dp0aws-ok-full.log"',
      'echo %PATH%>>"%~dp0aws-ok-path.log"',
      'if /i "%~1"=="ssm" if /i "%~2"=="start-session" goto tunnel',
      'if /i "%~1"=="ec2" if /i "%~2"=="describe-instances" goto describe',
      'exit /b 0',
      ':describe',
      'echo {"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}',
      'exit /b 0',
      ':tunnel',
      'findstr "^" >"%~dp0aws-ok-stdin.txt"',
      'echo tunnel-says-hello',
      'exit /b 7')
    $awsFail = New-OperatorStandIn -ConformanceHome $h -Name 'aws-fail' -Body @(
      'echo %*>>"%~dp0aws-fail-full.log"',
      'echo An error occurred (InternalFailure) when calling the operation 1>&2',
      'exit /b 254')
    $trip = Get-OperatorTripwireEnvironment -ConformanceHome $h
    $standins = Split-Path -Parent $awsOk
    $env1 = @{ AGENTSPACE_SSH_CMD = $trip['AGENTSPACE_SSH_CMD']; AGENTSPACE_SCP_CMD = $trip['AGENTSPACE_SCP_CMD']; AGENTSPACE_NESTED = '1' }
    function Invoke-Proxy([string]$Aws, [string[]]$Arguments = @(), [string]$Stdin = '') {
      $splat = @{ Variable = 'AGENTSPACE_SSH_PROXY_CMD'; ConformanceHome = $h; Arguments = $Arguments; Environment = ($env1 + @{ AGENTSPACE_AWS_CMD = $Aws }); TimeoutSeconds = 120 }
      if ($Stdin) { $splat['InputJson'] = $Stdin }
      Invoke-ConformanceProgram @splat
    }

    # ---- The success path, with an alias ----
    $r = Invoke-Proxy $awsOk @($alias) 'ssh-bytes-from-client'
    $full = @(Get-Lines (Join-Path $standins 'aws-ok-full.log'))
    if ($full.Count -eq 0) {
      Write-ConformanceResult -Name $nameA -Status FAIL -Reason 'seam not met (AGENTSPACE_AWS_CMD was not used)'
    } else {
      Write-ConformanceResult -Name $nameA -Status PASS

      $tunnel = @($full | Where-Object { $_ -match '^\s*ssm\s+start-session\b' })
      $describe = @($full | Where-Object { $_ -match '^\s*ec2\s+describe-instances\b' })
      $p = Get-Problems $r 7
      if ($tunnel.Count -ne 1) { $p.Add("The tunnel was started $($tunnel.Count) times, not once") }
      Write-Problems "${sec}: The ProxyCommand exits with the tunnel's exit code (step 6)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      if ($tunnel.Count -eq 1) {
        $want = "ssm start-session --region us-west-2 --target $fakeId --document-name AWS-StartSSHSession --parameters portNumber=22"
        if ($tunnel[0].Trim() -cne $want) { $p.Add('The tunnel call is not ssm start-session --region us-west-2 --target <cached ID> --document-name AWS-StartSSHSession --parameters portNumber=22') }
      } else { $p.Add('There was no single tunnel call') }
      Write-Problems "${sec}: The tunnel is started for the cached instance, with the arguments the spec gives (step 6)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      if ($describe.Count -eq 0) { $p.Add('up asked about no instance') }
      else {
        $firstTunnel = [array]::IndexOf($full, ($tunnel | Select-Object -First 1))
        $lastDescribe = [array]::LastIndexOf($full, ($describe | Select-Object -Last 1))
        if ($firstTunnel -ge 0 -and $lastDescribe -gt $firstTunnel) { $p.Add('An up call came after the tunnel started') }
      }
      Write-Problems "${sec}: up runs before the tunnel (steps 3 to 6)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      if ($r.Stdout.Trim() -cne 'tunnel-says-hello') { $p.Add('Stdout is not exactly the tunnel''s output') }
      if ($r.Stderr -match 'tunnel-says-hello') { $p.Add('The tunnel''s output is also on stderr') }
      Write-Problems "${sec}: Stdout holds the tunnel's output and nothing else, so the SSH connection is not corrupted (Why every message goes to stderr)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      $got = if (Test-Path -LiteralPath (Join-Path $standins 'aws-ok-stdin.txt')) { [IO.File]::ReadAllText((Join-Path $standins 'aws-ok-stdin.txt')) } else { '' }
      if ($got.Trim() -cne 'ssh-bytes-from-client') { $p.Add('The tunnel did not get the ProxyCommand''s own standard input, whole') }
      Write-Problems "${sec}: The tunnel reads the ProxyCommand's own stdin, which nothing before it read (What it MUST NOT do)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      $paths = @(Get-Lines (Join-Path $standins 'aws-ok-path.log'))
      if ($paths.Count -eq 0) { $p.Add('No PATH was recorded') }
      foreach ($line in $paths) {
        $head = @($line -split ';' | Select-Object -First 2 | ForEach-Object { $_.Trim().TrimEnd('\').ToLowerInvariant() })
        $wantHead = @($sessionFolder.ToLowerInvariant(), $awsFolder.ToLowerInvariant())
        if (-not ($head -contains $wantHead[0] -and $head -contains $wantHead[1])) { $p.Add('A call did not have the Session Manager plugin''s folder and the AWS CLI''s folder first on PATH'); break }
      }
      Write-Problems "${sec}: The Session Manager plugin's folder and the AWS CLI's folder are first on PATH for every call (step 2)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      if ($r.Stderr -match 'tunnel-says-hello') { $p.Add('stderr holds tunnel output') }
      $sshCalls = Get-StandInCallCount -Log (Join-Path $standins 'ssh-tripwire-calls.log')
      $scpCalls = Get-StandInCallCount -Log (Join-Path $standins 'scp-tripwire-calls.log')
      if ($sshCalls -gt 0 -or $scpCalls -gt 0) { $p.Add('ssh or scp was started, and a nested up pushes nothing') }
      Write-Problems "${seamSec}: With AGENTSPACE_NESTED set the ProxyCommand starts neither ssh nor scp" $p

      # ---- Without an alias, the same steps ----
      Remove-StandInLogs $h $standins
      $r = Invoke-Proxy $awsOk @() 'ssh-bytes-from-client'
      $full = @(Get-Lines (Join-Path $standins 'aws-ok-full.log'))
      $tunnel = @($full | Where-Object { $_ -match '^\s*ssm\s+start-session\b' })
      $p = Get-Problems $r 7
      if ($tunnel.Count -ne 1) { $p.Add('The tunnel was not started exactly once') }
      elseif ($tunnel[0] -notmatch [regex]::Escape("--target $fakeId ")) { $p.Add('The tunnel is not for the cached instance') }
      Write-Problems "${sec}: With no alias the ProxyCommand runs up and the tunnel the same way, and exits with the tunnel's code" $p

      # ---- A failed up: exit 1, no tunnel ----
      Remove-StandInLogs $h $standins
      $r = Invoke-Proxy $awsFail @($alias)
      $failCalls = @(Get-Lines (Join-Path $standins 'aws-fail-full.log'))
      $p = Get-Problems $r 1
      if ($failCalls.Count -eq 0) { $p.Add('The failing stand-in was never called, so up did not run') }
      if ($failCalls | Where-Object { $_ -match '^\s*ssm\s+start-session\b' }) { $p.Add('A tunnel was started after up failed') }
      if ($r.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
      Write-Problems "${sec}: When up fails the ProxyCommand exits 1, starts no tunnel and writes nothing to stdout (step 4)" $p

      # ---- An expired or missing AWS sign-in gets agentspace's own message, inside the ProxyCommand ----
      $awsSignIn = New-OperatorStandIn -ConformanceHome $h -Name 'aws-signin' -Body @(
        'echo %*>>"%~dp0aws-signin-full.log"',
        'echo Unable to locate credentials. You can configure credentials by running "aws login". 1>&2',
        'exit /b 255')
      $r = Invoke-Proxy $awsSignIn @($alias)
      $signLines = @($r.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })
      $signIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
      $p = Get-Problems $r 1
      if ($signLines.Count -eq 0 -or $signLines[-1] -cne $signIn) { $p.Add('The last line on stderr is not exactly the sign-in message') }
      if ($r.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
      if ($r.Stderr -match 'CategoryInfo|At line:|Exception') { $p.Add('Stderr shows a runtime error record') }
      Write-Problems "${sec}: A failed AWS call because the sign-in is missing or has expired ends the ProxyCommand with exactly agentspace's sign-in message, exit 1 and nothing on stdout (AWS failures, #0034)" $p
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-proxy-steps.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

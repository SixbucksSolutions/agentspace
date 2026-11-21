#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check: D1 holds at connect time, for the ProxyCommand's success and failure paths.
#
# EXPECTED TO FAIL AGAINST THE PROTOTYPE. This file checks two requirements that the prototype does
# not meet: card #0087 (docs/operator.md, "What a caller sees (D1, card #0087)") and the test seam
# (docs/operator.md, "The test seam"). The prototype runs the AWS CLI by a fixed path, so the
# stand-in is never called, and it prints backend text (instance IDs, EC2 states, the AWS CLI's
# text, a runtime's error record) instead of agentspace's own message. A rewrite MUST pass every
# result line. The result lines still say FAIL: nothing turns a FAIL into a pass. Every other file in
# conformance/operator/ passes against the prototype.
#
# What it does: runs the ProxyCommand twice, with AGENTSPACE_AWS_CMD naming a stand-in `aws` that
# the check writes into its throwaway home. The stand-in logs every call it gets, and prints canned
# output:
#   success path: it reports a made-up instance that is running and launched long ago, so `up`
#                 does not wait, and the tunnel call (ssm start-session) exits 0 with no output.
#   failure path: every call fails, with error text that holds a marker and a made-up instance ID.
# If the stand-in's call log stays empty, the seam is not met, and the path reports
# `FAIL ... seam not met (AGENTSPACE_AWS_CMD was not used)` and nothing more.
#
# When the seam is met, it asserts what the caller's ssh would see (operator.md, "What a caller
# sees"):
#   success: exit 0, nothing on stdout, no backend text on stderr (the stand-in's tunnel writes the
#            marker to its own stderr, which MUST be withheld), the caller's line `The host is ready.`,
#            no `Sending ...` line (the push's progress line), no runtime error record.
#   failure: a non-zero exit with nothing on stdout; the last line on stderr starts "agentspace: ";
#            no backend text on stderr (an instance or volume ID, a region name, a machine type, a
#            cloud state name, the words EC2, AWS, SSM, Session Manager, CloudFormation, the stack's
#            name, or the stand-in's error text), except #0034's sign-in message, exactly as
#            operator.md gives it (operator.md Q29); the last line is "agentspace: Could not reach the
#            host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why." (every call of the
#            stand-in fails, which is "any other failure"); no `Sending ...` line; no runtime error
#            record. With
#            AGENTSPACE_DIAGNOSTIC set, the exit code and stdout are the same.
# It never reports the offending text, only which rule it broke, because the text may carry an ID.
#
# The program under test is the one AGENTSPACE_SSH_PROXY_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# SAFETY. The ProxyCommand's `up` can call the real AWS CLI when the seam is not met. The harness
# gives the process a throwaway home, with USERPROFILE, HOME, LOCALAPPDATA, APPDATA and the state
# and ssh folders in it, the AWS configuration files pointed at files that do not exist, instance
# metadata off, and every AWS credential variable removed. Before the proxy runs, the check confirms
# under that environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can
# run, and reports SKIPPED, never running the proxy, if it does not. AGENTSPACE_NESTED is set, so
# `up` pushes no token and needs no ssh.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-proxy-connect-time.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$section = 'operator.md#what-a-caller-sees-d1-card-0087'
$seamSection = 'operator.md#the-test-seam'
# A made-up instance ID, built here so that no ID is written into this file.
$fakeId = 'i-' + ('0' * 17)
$marker = 'standin-backend-detail'
$signIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
$couldNotReach = 'agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.'

$backend = [ordered]@{
  'an instance or volume ID' = '\b(i|vol)-[0-9a-f]{8,17}\b'
  'a region name'            = '\b[a-z]{2}(-[a-z]+)+-\d\b'
  'a machine type'           = '\b[a-z]\d+[a-z]*\.(nano|micro|small|medium|\d*xlarge)\b'
  'a cloud state name'       = '\b(pending|stopping|stopped|shutting-down|terminated)\b'
  'a cloud or tool name'     = '\b(EC2|AWS|SSM|CloudFormation|Session Manager|agentbox|Ubuntu)\b'
  "the stand-in's own text"  = [regex]::Escape($marker)
}
$record = '(At line:\d+|CategoryInfo|FullyQualifiedErrorId|Traceback \(most recent|Unhandled exception|\bException\b)'

# Names the rules a stderr breaks, never the text.
function Get-BackendHits {
  param([string[]]$Lines, [switch]$AllowSignIn)
  $rest = if ($AllowSignIn) { @($Lines | Where-Object { $_.Trim() -ne $signIn }) } else { $Lines }
  $hits = @()
  foreach ($k in $backend.Keys) { if ($rest | Where-Object { $_ -match $backend[$k] }) { $hits += $k } }
  return $hits
}

function Get-CallCount {
  param([string]$Log)
  if (-not (Test-Path -LiteralPath $Log -PathType Leaf)) { return 0 }
  return @(Get-Content -LiteralPath $Log | Where-Object { $_.Trim() -ne '' }).Count
}

$h = New-ConformanceHome
try {
  $nameOf = @{ success = 'the success path'; failure = 'the failure path' }
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    foreach ($p in 'success', 'failure') { Write-ConformanceResult -Name "${section}: $($nameOf[$p]) names no backend" -Status SKIPPED -Reason $gate.Reason }
  } elseif (-not (Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD')) {
    foreach ($p in 'success', 'failure') { Write-ConformanceResult -Name "${section}: $($nameOf[$p]) names no backend" -Status FAIL -Reason 'Neither AGENTSPACE_SSH_PROXY_CMD nor its default names a program' }
  } else {
    # The box's record in the host registry, holding its instance ID (state.md, hosts.json; D45).
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $h.UserProfile '.agentspace\hosts.json')), ('{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","instance_id":"' + $fakeId + '"}]}'))

    # The stand-ins, in the home. They log the service and operation of every call, and no more.
    $dir = Join-Path $h.Root 'standins'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $dir) | Out-Null
    $awsOk = Join-Path $dir 'aws-ok.cmd'
    $awsFail = Join-Path $dir 'aws-fail.cmd'
    $logOk = Join-Path $dir 'aws-ok-calls.log'
    $logFail = Join-Path $dir 'aws-fail-calls.log'
    Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $awsOk) -Encoding ascii -Value @(
      '@echo off',
      'echo %~1 %~2 >>"%~dp0aws-ok-calls.log"',
      'if /i "%~1"=="ssm" if /i "%~2"=="start-session" goto tunnel',
      'if /i "%~1"=="ec2" if /i "%~2"=="describe-instances" goto describe',
      'exit /b 0',
      ':tunnel',
      "echo $marker tunnel text 1>&2",
      'exit /b 0',
      ':describe',
      'echo {"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}',
      'exit /b 0'
    )
    Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $awsFail) -Encoding ascii -Value @(
      '@echo off',
      'echo %~1 %~2 >>"%~dp0aws-fail-calls.log"',
      "echo An error occurred (InternalFailure) when calling the %~2 operation: $marker $fakeId 1>&2",
      'exit /b 254'
    )

    # A stand-in for ssh and scp, which operator.md does not call on this path (AGENTSPACE_NESTED is
    # set, and the ProxyCommand runs the AWS CLI's tunnel only). It logs and exits 1, so that a build
    # that starts either one, instead of the real ssh.exe, is caught and never reaches the real home.
    $sshStub = Join-Path $dir 'ssh-stub.cmd'
    $sshLog = Join-Path $dir 'ssh-stub-calls.log'
    Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $sshStub) -Encoding ascii -Value @('@echo off', 'echo called >>"%~dp0ssh-stub-calls.log"', 'exit /b 1')
    $sshEnv = @{ AGENTSPACE_SSH_CMD = $sshStub; AGENTSPACE_SCP_CMD = $sshStub }

    foreach ($p in 'success', 'failure') {
      $standin = if ($p -eq 'success') { $awsOk } else { $awsFail }
      $log = if ($p -eq 'success') { $logOk } else { $logFail }
      $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD' -ConformanceHome $h -TimeoutSeconds 120 `
        -Environment ($sshEnv + @{ AGENTSPACE_AWS_CMD = $standin; AGENTSPACE_NESTED = '1' })
      $label = $nameOf[$p]

      $name = "${seamSection}: the ProxyCommand runs AGENTSPACE_AWS_CMD ($label)"
      if ((Get-CallCount -Log $log) -eq 0) {
        Write-ConformanceResult -Name $name -Status FAIL -Reason 'seam not met (AGENTSPACE_AWS_CMD was not used)'
        continue
      }
      Write-ConformanceResult -Name $name -Status PASS

      if ($r.TimedOut) {
        Write-ConformanceResult -Name "${section}: $label ends instead of hanging" -Status FAIL -Reason 'It did not end within 120 seconds'
        continue
      }
      $name = "${seamSection}: the ProxyCommand starts neither ssh nor scp on this path ($label)"
      if ((Get-CallCount -Log $sshLog) -eq 0) { Write-ConformanceResult -Name $name -Status PASS }
      else { Write-ConformanceResult -Name $name -Status FAIL -Reason 'AGENTSPACE_SSH_CMD or AGENTSPACE_SCP_CMD was called, and operator.md gives this path no ssh or scp' }
      $lines = @($r.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })

      if ($p -eq 'success') {
        $name = "${section}: a successful connect exits 0 and writes nothing to stdout"
        if ($r.ExitCode -eq 0 -and $r.Stdout.Length -eq 0) { Write-ConformanceResult -Name $name -Status PASS }
        else { Write-ConformanceResult -Name $name -Status FAIL -Reason "Exit code $($r.ExitCode), $($r.Stdout.Length) bytes on stdout" }

        $name = "${section}: a successful connect prints the caller's line ""The host is ready."" and no ""Sending"" line"
        if (($lines | Where-Object { $_.Trim() -ceq 'The host is ready.' }) -and -not ($lines | Where-Object { $_ -match '^\s*(agentspace: )?Sending\b' })) { Write-ConformanceResult -Name $name -Status PASS }
        else { Write-ConformanceResult -Name $name -Status FAIL -Reason 'Stderr lacks the line "The host is ready.", or holds a "Sending ..." line' }

        $name = "${section}: a successful connect names no backend on stderr, the tunnel's stderr withheld"
        $hits = @(Get-BackendHits -Lines $lines)
        if ($hits.Count -eq 0) { Write-ConformanceResult -Name $name -Status PASS }
        else { Write-ConformanceResult -Name $name -Status FAIL -Reason ('Stderr holds ' + ($hits -join ', ')) }
      } else {
        $name = "${section}: a failed connect exits non-zero and writes nothing to stdout"
        if ($r.ExitCode -ne 0 -and $r.Stdout.Length -eq 0) { Write-ConformanceResult -Name $name -Status PASS }
        else { Write-ConformanceResult -Name $name -Status FAIL -Reason "Exit code $($r.ExitCode), $($r.Stdout.Length) bytes on stdout" }

        $name = "${section}: a failed connect ends with agentspace's own message"
        if ($lines.Count -gt 0 -and $lines[-1].StartsWith('agentspace: ')) { Write-ConformanceResult -Name $name -Status PASS }
        else { Write-ConformanceResult -Name $name -Status FAIL -Reason 'The last line on stderr does not start with "agentspace: "' }

        $name = "${section}: a failed connect ends with the ""Could not reach the host"" message, and prints no ""Sending"" line"
        if ($lines.Count -gt 0 -and $lines[-1].Trim() -ceq $couldNotReach -and -not ($lines | Where-Object { $_ -match '^\s*(agentspace: )?Sending\b' })) { Write-ConformanceResult -Name $name -Status PASS }
        else { Write-ConformanceResult -Name $name -Status FAIL -Reason 'The last line on stderr is not the "Could not reach the host" message, or stderr holds a "Sending ..." line' }

        $name = "${section}: a failed connect names no backend on stderr"
        $hits = @(Get-BackendHits -Lines $lines -AllowSignIn)
        if ($hits.Count -eq 0) { Write-ConformanceResult -Name $name -Status PASS }
        else { Write-ConformanceResult -Name $name -Status FAIL -Reason ('Stderr holds ' + ($hits -join ', ')) }

        # The diagnostic setting MAY add backend text, and MUST change nothing else.
        $d = Invoke-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD' -ConformanceHome $h -TimeoutSeconds 120 `
          -Environment ($sshEnv + @{ AGENTSPACE_AWS_CMD = $standin; AGENTSPACE_NESTED = '1'; AGENTSPACE_DIAGNOSTIC = '1' })
        $name = "${section}: AGENTSPACE_DIAGNOSTIC changes neither the exit code nor stdout"
        if (-not $d.TimedOut -and $d.ExitCode -eq $r.ExitCode -and $d.Stdout.Length -eq 0) { Write-ConformanceResult -Name $name -Status PASS }
        else { Write-ConformanceResult -Name $name -Status FAIL -Reason "Exit code $($d.ExitCode) against $($r.ExitCode), $($d.Stdout.Length) bytes on stdout" }
      }

      $name = "${section}: $label prints no runtime error record"
      if (-not ($lines | Where-Object { $_ -match $record })) { Write-ConformanceResult -Name $name -Status PASS }
      else { Write-ConformanceResult -Name $name -Status FAIL -Reason 'Stderr holds a runtime error record' }
    }
  }
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

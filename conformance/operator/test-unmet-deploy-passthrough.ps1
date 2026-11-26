#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `deploy`'s passthrough of the AWS CLI's `cloudformation deploy` output
# (docs/operator.md, "deploy", steps 3 to 5; "Streams"), run through the test seam (docs/operator.md,
# "The test seam"). Defect #0139, story E0.06-T05-S02 (#0140): the real CLI writes progress lines to
# stdout while it deploys, and the v0.06 operator then never returned. No check before this one ran
# `deploy` (docs/conformance.md, Coverage, the `deploy` row).
#
# EXPECTED TO FAIL AGAINST THE PROTOTYPE, because it does not meet the seam: it runs the AWS CLI by a
# fixed path and reads none of AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD or AGENTSPACE_SCP_CMD (card
# #0085). Until a build calls the stand-in `aws`, the file reports `FAIL ... seam not met` and checks
# nothing more. A rewrite MUST pass every result line.
#
# What it does, when the seam is met: it writes `deploy`'s two inputs into a throwaway home (a made-up
# public key and a stack parameters file) and a stand-in `aws` that answers each call from files
# beside it. Every case stops `deploy` at step 5 or before: the stand-in's `cloudformation
# describe-stacks` answers a stack with no InstanceId output, so `sync` (step 6) is never reached,
# and `go`, `ssh-keygen`, `ssh` and `scp`, which a check MUST NOT run here (docs/conformance.md,
# Coverage, the `deploy` row), are never started. Each case checks that the run ended within its
# deadline, the operator's stdout against the CLI's exactly, the first and last stderr lines, the
# CLI's stderr line where it wrote one, the exit code and the AWS calls made.
#
# The deadline is the run's own: the harness waits for the operator to exit, and a run that has not
# exited within it is stopped with its process tree and fails, saying what it waited for. Nothing
# here sleeps.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms
# under the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator
# can run, and reports SKIPPED, running nothing, if it does not. `ssh` and `scp` are tripwires. The
# key is made up. A failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-deploy-passthrough.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#deploy'
$deadlineSeconds = 60
$deploying = "Deploying stack 'agentbox' in us-west-2..."
$deployFailed = 'agentspace: Stack deploy failed; see the stack events in the console.'
$keyLine = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMP user@example'
$paramsJson = '{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE"}}'
# What the real CLI prints to stdout during a deploy that has nothing to change.
$cliStdout = "`nWaiting for changeset to be created..`n`nNo changes to deploy. Stack agentbox is up to date`n"
$cliStderr = "A line on the CLI's stderr`n"
# A stack with no InstanceId output, so that step 5 fails with the spec's message and sync is never
# reached (docs/operator.md, Laptop files, the instance-ID cache).
$noOutputs = '{"Stacks":[{"StackName":"agentbox","Outputs":[]}]}'
$noInstanceId = "agentspace: Stack 'agentbox' has no InstanceId output."

# The aws stand-in: a PowerShell 7 script that a .cmd starts, so that it sees its arguments as a
# program would. It logs each call's first two arguments, then answers from files beside it, named
# <service>-<operation>: standard output, then standard error, then an exit code. It never reads its
# stdin. It writes its stdout before it exits, as the real CLI's progress lines are.
$awsStandInScript = @'
$dir = $PSScriptRoot
$k = [string]$args[0] + '-' + [string]$args[1]
[IO.File]::AppendAllText((Join-Path $dir 'aws-keys.log'), ($k + "`n"))
$out = Join-Path $dir "ans\$k.out"
$err = Join-Path $dir "ans\$k.err"
$code = Join-Path $dir "ans\$k.code"
if (Test-Path -LiteralPath $out) { [Console]::Out.Write([IO.File]::ReadAllText($out)); [Console]::Out.Flush() }
if (Test-Path -LiteralPath $err) { [Console]::Error.Write([IO.File]::ReadAllText($err)); [Console]::Error.Flush() }
if (Test-Path -LiteralPath $code) { exit ([int]([IO.File]::ReadAllText($code).Trim())) }
exit 0
'@

$h = New-ConformanceHome
try {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  $seamName = "operator.md#the-test-seam: deploy runs AGENTSPACE_AWS_CMD"
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $seamName -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name $seamName -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
  } else {
    # ---- The throwaway home: deploy's two inputs and the stand-ins ----
    $pwshPath = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
    $aws = New-OperatorStandIn -ConformanceHome $h -Name 'aws' -Body @(
      ('"' + $pwshPath + '" -NoProfile -File "%~dp0aws-standin.ps1" %*'),
      'exit /b %ERRORLEVEL%')
    $standins = Split-Path -Parent $aws
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $standins 'aws-standin.ps1')), $awsStandInScript)
    $trip = Get-OperatorTripwireEnvironment -ConformanceHome $h
    $env0 = @{ AGENTSPACE_AWS_CMD = $aws; AGENTSPACE_SSH_CMD = $trip['AGENTSPACE_SSH_CMD']; AGENTSPACE_SCP_CMD = $trip['AGENTSPACE_SCP_CMD'] }
    $tripLogs = @('ssh-tripwire-calls.log', 'scp-tripwire-calls.log') | ForEach-Object { Join-Path $standins $_ }
    $ansDir = Join-Path $standins 'ans'
    $keysLog = Join-Path $standins 'aws-keys.log'
    $sshDir = Join-Path $h.UserProfile '.ssh'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $sshDir) | Out-Null
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $sshDir 'id_ed25519.pub')), "$keyLine`r`n")
    $stateDir = Join-Path $h.UserProfile '.agentspace'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $stateDir) | Out-Null
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $stateDir 'stack-parameters.json')), $paramsJson)

    function Reset-Case {
      if (Test-Path -LiteralPath $keysLog) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $keysLog) -Force }
      if (Test-Path -LiteralPath $ansDir) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $ansDir) -Recurse -Force }
      New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $ansDir) | Out-Null
    }
    function Set-Answer([string]$Key, [string]$Out = $null, [string]$Err = $null, $Code = $null) {
      if ($null -ne $Out) { [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $ansDir "$Key.out")), $Out) }
      if ($null -ne $Err) { [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $ansDir "$Key.err")), $Err) }
      if ($null -ne $Code) { [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $ansDir "$Key.code")), [string]$Code) }
    }
    function Get-AwsKeys {
      if (-not (Test-Path -LiteralPath $keysLog -PathType Leaf)) { return @() }
      @(Get-Content -LiteralPath $keysLog | Where-Object { $_.Trim() -ne '' })
    }
    # Runs `deploy` once and reports it. $Expect: Exit, Stdout (exact), Last (the last stderr line),
    # Has (a line the CLI wrote to stderr, which MUST be among the operator's stderr lines; optional),
    # Keys (the AWS calls' <service>-<operation>, in order).
    function Test-Case {
      param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][scriptblock]$Setup,
        [Parameter(Mandatory)][hashtable]$Expect
      )
      try {
        Reset-Case
        & $Setup
        $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments @('deploy') -Environment $env0 -TimeoutSeconds $deadlineSeconds
        $p = [System.Collections.Generic.List[string]]::new()
        if ($r.TimedOut) {
          $p.Add("The operator did not exit within $deadlineSeconds seconds of its start, and was stopped; the AWS CLI was called $(@(Get-AwsKeys).Count) times, and the operator's stdout had $($r.Stdout.Length) characters")
        } elseif ($r.ExitCode -ne $Expect.Exit) { $p.Add("Exit $($r.ExitCode), not $($Expect.Exit)") }
        if ($r.Stdout -cne $Expect.Stdout) {
          $p.Add("Stdout is not the CLI's stdout as written ($($r.Stdout.Length) characters, not $($Expect.Stdout.Length))")
        }
        $lines = @($r.Stderr -split "`n" | Where-Object { $_ -ne '' })
        if ($lines.Count -eq 0 -or $lines[0] -cne $deploying) { $p.Add("The first stderr line is not '$deploying'") }
        if ($lines.Count -eq 0 -or $lines[-1] -cne $Expect.Last) { $p.Add("The last stderr line is not '$($Expect.Last)'") }
        if ($Expect.Has -and -not ($lines -ccontains $Expect.Has)) { $p.Add("The CLI's stderr line '$($Expect.Has)' did not reach the operator's stderr") }
        if ($r.Stderr -match 'CategoryInfo|At line:|Exception|panicked') { $p.Add('Stderr shows a runtime error record') }
        $keys = @(Get-AwsKeys)
        if (($keys -join ',') -cne ($Expect.Keys -join ',')) { $p.Add("The AWS calls were $($keys.Count), not the $($Expect.Keys.Count) of '$($Expect.Keys -join ', ')'") }
        foreach ($log in $tripLogs) { if ((Get-StandInCallCount -Log $log) -gt 0) { $p.Add("$(Split-Path -Leaf $log) shows a start") } }
        if ($p.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
        else { Write-ConformanceResult -Name $Name -Status FAIL -Reason (ConvertTo-ConformanceMasked ($p -join '; ')) }
      } catch {
        $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
        Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
      }
    }

    # ---- The seam, with a CLI that writes nothing to stdout: the shape the earlier checks had ----
    Reset-Case
    Set-Answer 'cloudformation-deploy' -Out '' -Code 0
    Set-Answer 'cloudformation-describe-stacks' -Out $noOutputs -Code 0
    $null = Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments @('deploy') -Environment $env0 -TimeoutSeconds $deadlineSeconds
    if (@(Get-AwsKeys).Count -eq 0) {
      Write-ConformanceResult -Name $seamName -Status FAIL -Reason 'seam not met (AGENTSPACE_AWS_CMD was not used)'
    } else {
      Write-ConformanceResult -Name $seamName -Status PASS
      $stepFive = 'cloudformation-deploy', 'cloudformation-describe-stacks'

      Test-Case "${sec}: a deploy whose CLI writes nothing to stdout ends, and goes on to step 5" {
        Set-Answer 'cloudformation-deploy' -Out '' -Code 0
        Set-Answer 'cloudformation-describe-stacks' -Out $noOutputs -Code 0
      } @{ Exit = 1; Stdout = ''; Last = $noInstanceId; Keys = $stepFive }

      Test-Case "${sec}: a deploy whose CLI writes progress lines to stdout passes them through as written, ends, and goes on to step 5 (#0139)" {
        Set-Answer 'cloudformation-deploy' -Out $cliStdout -Err $cliStderr -Code 0
        Set-Answer 'cloudformation-describe-stacks' -Out $noOutputs -Code 0
      } @{ Exit = 1; Stdout = $cliStdout; Last = $noInstanceId; Has = $cliStderr.TrimEnd("`n"); Keys = $stepFive }

      Test-Case "${sec}: a failed deploy whose CLI wrote to stdout passes it through and ends with step 4's failure (#0139)" {
        Set-Answer 'cloudformation-deploy' -Out "`nWaiting for changeset to be created..`n" -Err "An error occurred (ValidationError) when calling the CreateChangeSet operation`n" -Code 255
      } @{ Exit = 1; Stdout = "`nWaiting for changeset to be created..`n"; Last = $deployFailed; Has = 'An error occurred (ValidationError) when calling the CreateChangeSet operation'; Keys = @('cloudformation-deploy') }
    }
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-deploy-passthrough.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

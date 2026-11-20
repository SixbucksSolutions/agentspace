#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's `down`, `status` and `grow` (docs/operator.md, "down", "status",
# "grow", "AWS failures", "The AWS side"), run through the test seam (docs/operator.md, "The test seam").
# Card #0132 (E0.06-T04-S02), gap FB-25 (docs/conformance.md, "Coverage of the operator and the keeper").
#
# EXPECTED TO FAIL AGAINST THE PROTOTYPE, because it does not meet the seam: it runs the AWS CLI and
# `ssh` by fixed paths and reads none of AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD or AGENTSPACE_SCP_CMD
# (card #0085; docs/operator.md, "Requirements the prototype does not meet yet"). Until a build calls
# the stand-in `aws`, the file reports `FAIL ... seam not met` and checks nothing more. A rewrite MUST
# pass every result line. The result lines still say FAIL: nothing turns a FAIL into a pass.
#
# What it does, when the seam is met: it writes into a throwaway home the instance-ID cache, a
# stand-in `aws` that answers each call from files beside it and logs the call's arguments, and a
# stand-in `ssh` that does the same. For each command it runs the cases below and checks the AWS CLI
# calls in order with their arguments, the messages on stderr exactly, stdout, the exit code, and that
# no other program was started.
#
#   down    The one call and its message; -Quiet; the answer that is not read; a missing, empty or
#           unreadable cache; the failures of the AWS CLI (the sign-in message, the CLI's own text,
#           an empty error text); a stack with no InstanceId output.
#   status  The five lines for each state and each Session Manager answer; the root volume found by
#           its device name; -Quiet; every call's failure, answers that are not JSON, members that
#           are missing or of another form, and a member that the command does not read.
#   grow    The calls and messages for a stopped box, a running box and a box in another state; the
#           10-second wait and the one ssh with its arguments; the ssh's streams passed through;
#           a failed online grow (D36); a refused volume change; -Quiet; the -GiB values; the
#           refusals of grow's arguments; the same AWS failures.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential, so a mistaken run cannot reach Terry's files or an account. Before any
# program that can reach AWS starts, the check confirms under the same environment that `aws sts
# get-caller-identity` fails for every AWS CLI the operator can run, and reports SKIPPED, running
# nothing, if it does not. The three seam variables name stand-ins in the home; `scp` is a tripwire
# that no command here may start. Every instance ID and volume ID is made up. The stand-ins never
# receive a token. A failure reason never quotes the program's output, which can hold an instance ID.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/prototype/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-commands.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$ErrorActionPreference = 'Stop'
$seamSec = 'operator.md#the-test-seam'
$id = 'i-1234567890abcdef0'
$vol = 'vol-1234567890abcdef0'
$otherVol = 'vol-fedcba9876543210f'
$signIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
$growWaitSeconds = 10

# ---- The calls the spec gives (Calls by command), as the stand-in logs them. Every call passes
# --region us-west-2 and, for the ones marked JSON, --output json, in the order the spec fixes. ----
$cStack = 'cloudformation describe-stacks --region us-west-2 --output json --stack-name agentbox'
$cDescribeInstances = "ec2 describe-instances --region us-west-2 --output json --instance-ids $id"
$cDescribeVolumes = "ec2 describe-volumes --region us-west-2 --output json --volume-ids $vol"
$cSsm = "ssm describe-instance-information --region us-west-2 --output json --filters Key=InstanceIds,Values=$id"
$cStop = "ec2 stop-instances --region us-west-2 --output json --instance-ids $id"
function Get-ModifyCall([long]$Size) { "ec2 modify-volume --region us-west-2 --output json --volume-id $vol --size $Size" }

# The ssh stand-in: a PowerShell 7 script that a .cmd starts, so that it sees its arguments as a
# program would. It logs each call's arguments as JSON and the time it started, then answers from
# files beside it: standard output, standard error and an exit code. It never reads its stdin.
$sshStandInScript = @'
$dir = $PSScriptRoot
New-Item -ItemType Directory -Force -Path (Join-Path $dir 'run') | Out-Null
[IO.File]::AppendAllText((Join-Path $dir 'run\ssh-args.log'), ((ConvertTo-Json -InputObject @($args) -Compress) + "`n"))
[IO.File]::AppendAllText((Join-Path $dir 'run\ssh-time.log'), ([DateTime]::UtcNow.Ticks.ToString() + "`n"))
$out = Join-Path $dir 'ans\ssh.out'
$err = Join-Path $dir 'ans\ssh.err'
$code = Join-Path $dir 'ans\ssh.code'
if (Test-Path -LiteralPath $out) { [Console]::Out.Write([IO.File]::ReadAllText($out)); [Console]::Out.Flush() }
if (Test-Path -LiteralPath $err) { [Console]::Error.Write([IO.File]::ReadAllText($err)); [Console]::Error.Flush() }
if (Test-Path -LiteralPath $code) { exit ([int]([IO.File]::ReadAllText($code).Trim())) }
exit 0
'@

# ---- Documents the stand-in `aws` answers with, as the AWS CLI prints them in JSON ----
function New-InstanceDoc {
  param(
    [string]$State = 'running',
    [string]$Type = 'm9g.xlarge',
    [string]$RootDevice = '/dev/sda1',
    # What to leave out: State, InstanceType, RootDeviceName, LaunchTime, RootMapping or Ebs.
    [string[]]$Without = @(),
    [switch]$RootFirst
  )
  $other = [ordered]@{ DeviceName = '/dev/sdb'; Ebs = [ordered]@{ VolumeId = $otherVol } }
  $root = [ordered]@{ DeviceName = $RootDevice; Ebs = [ordered]@{ VolumeId = $vol } }
  if ($Without -contains 'Ebs') { $root.Remove('Ebs') }
  $mappings = if ($RootFirst) { @($root, $other) } else { @($other, $root) }
  if ($Without -contains 'RootMapping') { $mappings = @($other) }
  $inst = [ordered]@{
    State = [ordered]@{ Name = $State }
    LaunchTime = '2020-01-01T00:00:00+00:00'
    InstanceType = $Type
    RootDeviceName = $RootDevice
    BlockDeviceMappings = $mappings
    Placement = [ordered]@{ AvailabilityZone = 'us-west-2a' }
  }
  foreach ($m in 'State', 'InstanceType', 'RootDeviceName', 'LaunchTime') { if ($Without -contains $m) { $inst.Remove($m) } }
  ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ Reservations = @([ordered]@{ Instances = @($inst) }) })
}
function New-VolumesDoc([long]$Size = 20) {
  ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ Volumes = @([ordered]@{ VolumeId = $vol; Size = $Size; VolumeType = 'gp3' }) })
}
function New-SsmDoc([string]$Ping = 'Online') {
  if ($Ping -eq 'none') { return '{"InstanceInformationList":[]}' }
  ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ InstanceInformationList = @([ordered]@{ InstanceId = $id; PingStatus = $Ping }) })
}
$stopDoc = ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ StoppingInstances = @([ordered]@{ InstanceId = $id; CurrentState = [ordered]@{ Name = 'stopping' } }) })
$modifyDoc = ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ VolumeModification = [ordered]@{ VolumeId = $vol; ModificationState = 'modifying' } })
function New-StackDoc([string[]]$OutputKeys = @('InstanceId'), [string]$Value = $id) {
  $outs = @(foreach ($k in $OutputKeys) { [ordered]@{ OutputKey = $k; OutputValue = $(if ($k -eq 'InstanceId') { $Value } else { 'unrelated' }) } })
  ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ Stacks = @([ordered]@{ StackName = 'agentbox'; Outputs = $outs }) })
}

# ---- The sign-in failure's texts, each of which the operator's test (docs/operator.md, "AWS
# failures") MUST read as a sign-in failure ----
$signInTexts = [ordered]@{
  'Unable to locate credentials' = "Unable to locate credentials. You can configure credentials by running `"aws login`"."
  'aws login' = "Error loading SSO Token: run aws login to refresh."
  'expired' = 'An error occurred (ExpiredToken) when calling the DescribeInstances operation: The security token included in the request is expired'
  'EXPIRED, in capitals' = 'THE TOKEN HAS EXPIRED'
}
$otherAwsText = 'An error occurred (UnauthorizedOperation) when calling the StopInstances operation: You are not authorized to perform this operation.'

$h = New-ConformanceHome
try {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  $seamName = "${seamSec}: down, status and grow run AGENTSPACE_AWS_CMD"
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $seamName -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name $seamName -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
  } else {
    # ---- The throwaway home: the stand-ins and the instance-ID cache ----
    $pwshPath = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
    $aws = New-OperatorStandIn -ConformanceHome $h -Name 'aws' -Body @(
      '>>"%~dp0aws-full.log" echo %*',
      'set "K=%~1-%~2"',
      'if exist "%~dp0ans\%K%.err" type "%~dp0ans\%K%.err" 1>&2',
      'if exist "%~dp0ans\%K%.out" type "%~dp0ans\%K%.out"',
      'if not exist "%~dp0ans\%K%.code" exit /b 0',
      'set /p C=<"%~dp0ans\%K%.code"',
      'exit /b %C%')
    $standins = Split-Path -Parent $aws
    $sshPs1 = Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $standins 'ssh-standin.ps1')
    [IO.File]::WriteAllText($sshPs1, $sshStandInScript)
    $ssh = New-OperatorStandIn -ConformanceHome $h -Name 'ssh' -Body @(
      ('"' + $pwshPath + '" -NoProfile -File "%~dp0ssh-standin.ps1" %*'),
      'exit /b %ERRORLEVEL%')
    $trip = Get-OperatorTripwireEnvironment -ConformanceHome $h
    $env0 = @{ AGENTSPACE_AWS_CMD = $aws; AGENTSPACE_SSH_CMD = $ssh; AGENTSPACE_SCP_CMD = $trip['AGENTSPACE_SCP_CMD'] }
    $scpLog = Join-Path $standins 'scp-tripwire-calls.log'
    $ansDir = Join-Path $standins 'ans'
    $cacheFile = Join-Path $h.UserProfile '.agentspace\instance-id.json'
    $cacheJson = '{"version":0,"instance_id":"' + $id + '"}'

    # ---- Helpers ----
    function Reset-Case {
      foreach ($f in Get-ChildItem -LiteralPath $standins -Filter '*.log' -File -ErrorAction SilentlyContinue) {
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $f.FullName) -Force
      }
      foreach ($d in 'ans', 'run') {
        $p = Join-Path $standins $d
        if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $p) -Recurse -Force }
        New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $p) | Out-Null
      }
      # The cache of every case is the made-up one, unless the case replaces it.
      if (Test-Path -LiteralPath $cacheFile) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $cacheFile) -Recurse -Force }
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path $cacheFile), $cacheJson)
    }
    # Sets the answer of one call, or of the ssh ($Key 'ssh'): standard output, standard error, exit code.
    function Set-Answer([string]$Key, [string]$Out = $null, [string]$Err = $null, $Code = $null) {
      if ($null -ne $Out) { [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $ansDir "$Key.out")), $Out) }
      if ($null -ne $Err) { [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $ansDir "$Key.err")), $Err) }
      if ($null -ne $Code) { [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $ansDir "$Key.code")), [string]$Code) }
    }
    # The answers of a healthy box: every call succeeds with a document of the form the spec reads.
    function Set-Healthy {
      param([string]$State = 'running', [string]$Ping = 'Online', [long]$Size = 20, [switch]$RootFirst)
      Set-Answer 'ec2-describe-instances' -Out (New-InstanceDoc -State $State -RootFirst:$RootFirst)
      Set-Answer 'ec2-describe-volumes' -Out (New-VolumesDoc $Size)
      Set-Answer 'ssm-describe-instance-information' -Out (New-SsmDoc $Ping)
      Set-Answer 'ec2-stop-instances' -Out $stopDoc
      Set-Answer 'ec2-modify-volume' -Out $modifyDoc
      Set-Answer 'ssh' -Out '' -Err '' -Code 0
    }
    # The AWS CLI calls the stand-in logged, one per element. A double quote is dropped, so that a build
    # that quotes an argument that needs none is not failed for it: the arguments are the spec's.
    function Get-AwsCalls {
      $log = Join-Path $standins 'aws-full.log'
      if (-not (Test-Path -LiteralPath $log -PathType Leaf)) { return @() }
      @(Get-Content -LiteralPath $log | Where-Object { $_.Trim() -ne '' } | ForEach-Object { ($_ -replace '"', '').Trim() })
    }
    function Get-SshCalls {
      $log = Join-Path $standins 'run\ssh-args.log'
      if (-not (Test-Path -LiteralPath $log -PathType Leaf)) { return @() }
      @(Get-Content -LiteralPath $log | Where-Object { $_.Trim() -ne '' })
    }
    # A failure reason names the line that differs and quotes the spec's text, never the program's.
    function Compare-Stream([string]$What, [string]$Got, [string]$Want) {
      if ($Got -ceq $Want) { return $null }
      $g = $Got -split "`n"
      $w = $Want -split "`n"
      for ($i = 0; $i -lt [Math]::Max($g.Count, $w.Count); $i++) {
        $a = if ($i -lt $g.Count) { $g[$i] } else { $null }
        $b = if ($i -lt $w.Count) { $w[$i] } else { $null }
        if ($a -cne $b) {
          if ($null -eq $b) { return "$What has more lines than the spec's $($w.Count - 1)" }
          if ($null -eq $a) { return "$What stops before line $($i + 1), which is '$b'" }
          if ($a.Contains("`r")) { return "$What line $($i + 1) holds a carriage return" }
          return "$What line $($i + 1) is not '$b'"
        }
      }
      "$What differs from the spec's text"
    }
    # Runs one case and reports it. $Expect: Exit, Stdout, Stderr (exact text), Calls (the AWS CLI calls,
    # in order, exactly), SshCalls (a count), and Check (a scriptblock that returns more problems).
    function Test-Case {
      param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string[]]$Arguments,
        [Parameter(Mandatory)][hashtable]$Expect,
        [scriptblock]$Setup = { Set-Healthy },
        [hashtable]$Environment = @{},
        [int]$Timeout = 120
      )
      try {
        Reset-Case
        & $Setup
        $started = [DateTime]::UtcNow
        $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments $Arguments -Environment ($env0 + $Environment) -TimeoutSeconds $Timeout
        $p = [System.Collections.Generic.List[string]]::new()
        if ($r.TimedOut) { $p.Add('It timed out') }
        elseif ($r.ExitCode -ne $Expect.Exit) { $p.Add("Exit $($r.ExitCode), not $($Expect.Exit)") }
        $stdout = if ($Expect.ContainsKey('Stdout')) { $Expect.Stdout } else { '' }
        $x = Compare-Stream 'Stdout' $r.Stdout $stdout
        if ($x) { $p.Add($x) }
        if ($Expect.ContainsKey('Stderr')) {
          $x = Compare-Stream 'Stderr' $r.Stderr $Expect.Stderr
          if ($x) { $p.Add($x) }
        }
        if ($r.Stderr -match 'CategoryInfo|At line:|Exception|panicked') { $p.Add('Stderr shows a runtime error record') }
        $calls = @(Get-AwsCalls)
        if ($Expect.ContainsKey('Calls')) {
          $want = @($Expect.Calls)
          if ($calls.Count -ne $want.Count) { $p.Add("The AWS CLI was called $($calls.Count) times, not $($want.Count)") }
          for ($i = 0; $i -lt [Math]::Min($calls.Count, $want.Count); $i++) {
            if ($calls[$i] -cne $want[$i]) { $p.Add("AWS CLI call $($i + 1) is not '$($want[$i])'"); break }
          }
        }
        $wantSsh = if ($Expect.ContainsKey('SshCalls')) { $Expect.SshCalls } else { 0 }
        $sshCalls = @(Get-SshCalls)
        if ($sshCalls.Count -ne $wantSsh) { $p.Add("ssh was started $($sshCalls.Count) times, not $wantSsh") }
        if ((Get-StandInCallCount -Log $scpLog) -gt 0) { $p.Add('scp was started') }
        if ($Expect.ContainsKey('Check')) { foreach ($m in @(& $Expect.Check $r $started)) { if ($m) { $p.Add($m) } } }
        if ($p.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
        else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($p -join '; ') }
      } catch {
        $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
        Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
      }
    }
    function Join-Lines([string[]]$Lines) { ($Lines -join "`n") + "`n" }

    # ---- The seam: until the stand-in `aws` is called, nothing below can be shown ----
    Reset-Case
    Set-Healthy
    $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments @('down') -Environment $env0 -TimeoutSeconds 120
    if (@(Get-AwsCalls).Count -eq 0) {
      Write-ConformanceResult -Name $seamName -Status FAIL -Reason 'seam not met (AGENTSPACE_AWS_CMD was not used)'
    } else {
      Write-ConformanceResult -Name $seamName -Status PASS

      # ============================ down ============================
      $s = 'operator.md#down'
      Test-Case "${s}: asks EC2 to stop the instance and prints Stopping <id>. on stderr, with nothing on stdout, exit 0" @('down') @{
        Exit = 0; Stderr = "Stopping $id.`n"; Calls = @($cStop) }
      Test-Case "${s}: -Quiet prints no message, and still asks EC2 to stop the instance" @('down', '-Quiet') @{
        Exit = 0; Stderr = ''; Calls = @($cStop) }
      Test-Case "${s}: asks whatever state the instance is in, so a stopped instance is stopped with no describe call first" @('down') @{
        Exit = 0; Stderr = "Stopping $id.`n"; Calls = @($cStop) } -Setup {
        Set-Healthy -State 'stopped'
      }
      Test-Case "${s}: the answer of ec2 stop-instances is not read beyond its exit code (AWS side, What is read from each answer)" @('down') @{
        Exit = 0; Stderr = "Stopping $id.`n"; Calls = @($cStop) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-stop-instances' -Out 'not json at all'
      }
      Test-Case "${s}: starts no ssh, scp or start request" @('down') @{
        Exit = 0; Calls = @($cStop); SshCalls = 0; Check = { param($r, $t) if ((@(Get-AwsCalls) | Where-Object { $_ -match 'start-instances|describe-' }).Count -gt 0) { 'A call other than the stop was made' } } }
      Test-Case "${s}: a failed stop prints no Stopping line, only the failure, exit 1 (AWS failures)" @('down') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = @($cStop) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-stop-instances' -Err $otherAwsText -Code 254
      }
      foreach ($k in $signInTexts.Keys) {
        $text = $signInTexts[$k]
        Test-Case "operator.md#aws-failures: down, a failure whose text holds '$k' prints exactly the sign-in message, exit 1, nothing on stdout" @('down') @{
          Exit = 1; Stderr = "$signIn`n"; Calls = @($cStop) } -Setup {
          Set-Healthy
          Set-Answer 'ec2-stop-instances' -Err $text -Code 255
        }
      }
      Test-Case "operator.md#aws-failures: down, any other failure is agentspace: and the CLI's own error text, trimmed" @('down') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = @($cStop) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-stop-instances' -Err "`n  $otherAwsText  `n`n" -Code 254
      }
      Test-Case "operator.md#aws-failures: down, the error text is the CLI's stderr, even when its stdout holds text too" @('down') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = @($cStop) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-stop-instances' -Out 'words on stdout' -Err $otherAwsText -Code 254
      }
      Test-Case "operator.md#aws-failures: down, when stderr is empty the error text is the CLI's stdout" @('down') @{
        Exit = 1; Stderr = "agentspace: words on stdout`n"; Calls = @($cStop) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-stop-instances' -Out "words on stdout`n" -Code 254
      }
      Test-Case "operator.md#aws-failures: down, when both are empty the message is The AWS CLI failed with exit code <n>." @('down') @{
        Exit = 1; Stderr = "agentspace: The AWS CLI failed with exit code 3.`n"; Calls = @($cStop) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-stop-instances' -Code 3
      }
      Test-Case "operator.md#aws-failures: down -Quiet still prints a failure" @('down', '-Quiet') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = @($cStop) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-stop-instances' -Err $otherAwsText -Code 254
      }

      # ---- The instance-ID cache, as down reads it (Laptop files) ----
      $ls = 'operator.md#laptop-files'
      $stackCalls = @($cStack, $cStop)
      $cacheCheck = {
        param($r, $t)
        if (-not (Test-Path -LiteralPath $cacheFile -PathType Leaf)) { return 'No instance-ID cache was written' }
        try { $doc = [IO.File]::ReadAllText($cacheFile) | ConvertFrom-Json -ErrorAction Stop } catch { return 'The cache is not JSON' }
        if ($doc.instance_id -cne $id) { return 'The cache does not hold the stack''s InstanceId output' }
      }
      Test-Case "${ls}: with no instance-ID cache down reads the stack's InstanceId output first, writes the cache, then stops that instance" @('down') @{
        Exit = 0; Stderr = "Stopping $id.`n"; Calls = $stackCalls; Check = $cacheCheck } -Setup {
        Set-Healthy
        Set-Answer 'cloudformation-describe-stacks' -Out (New-StackDoc @('Other', 'InstanceId'))
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $cacheFile) -Force
      }
      foreach ($bad in @(@{ N = 'empty'; T = '' }, @{ N = 'only whitespace'; T = "  `n" }, @{ N = 'not JSON'; T = 'not json {' }, @{ N = 'JSON that does not match its schema'; T = '{"version":0}' })) {
        $badText = $bad.T
        Test-Case "${ls}: a cache that is $($bad.N) is treated as missing, so down reads the stack and rewrites it" @('down') @{
          Exit = 0; Stderr = "Stopping $id.`n"; Calls = $stackCalls; Check = $cacheCheck } -Setup {
          Set-Healthy
          Set-Answer 'cloudformation-describe-stacks' -Out (New-StackDoc)
          [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path $cacheFile), $badText)
        }
      }
      Test-Case "${ls}: a stack with no InstanceId output fails with Stack 'agentbox' has no InstanceId output. and writes no cache" @('down') @{
        Exit = 1; Stderr = "agentspace: Stack 'agentbox' has no InstanceId output.`n"; Calls = @($cStack)
        Check = { param($r, $t) if (Test-Path -LiteralPath $cacheFile) { 'A cache was written' } } } -Setup {
        Set-Healthy
        Set-Answer 'cloudformation-describe-stacks' -Out (New-StackDoc @('Other'))
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $cacheFile) -Force
      }
      Test-Case "${ls}: an InstanceId output that is not an instance ID is the same failure, and no cache is written" @('down') @{
        Exit = 1; Stderr = "agentspace: Stack 'agentbox' has no InstanceId output.`n"; Calls = @($cStack)
        Check = { param($r, $t) if (Test-Path -LiteralPath $cacheFile) { 'A cache was written' } } } -Setup {
        Set-Healthy
        Set-Answer 'cloudformation-describe-stacks' -Out (New-StackDoc -Value 'not an instance')
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $cacheFile) -Force
      }
      Test-Case "${ls}: a stack that does not exist fails with agentspace: and the CLI's error text, and nothing is stopped" @('down') @{
        Exit = 1; Stderr = "agentspace: An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id agentbox does not exist`n"; Calls = @($cStack) } -Setup {
        Set-Healthy
        Set-Answer 'cloudformation-describe-stacks' -Err 'An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id agentbox does not exist' -Code 254
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $cacheFile) -Force
      }
      Test-Case "${ls}: an answer of cloudformation describe-stacks that is not JSON fails with Unexpected answer from cloudformation describe-stacks: not JSON." @('down') @{
        Exit = 1; Stderr = "agentspace: Unexpected answer from cloudformation describe-stacks: not JSON.`n"; Calls = @($cStack) } -Setup {
        Set-Healthy
        Set-Answer 'cloudformation-describe-stacks' -Out 'not json'
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $cacheFile) -Force
      }
      Test-Case "${ls}: a cache that cannot be opened (a folder at its path) fails with the cache's own message, exit 1, and no AWS call" @('down') @{
        Exit = 1; Calls = @()
        Check = {
          param($r, $t)
          $m = [regex]::Match($r.Stderr, '^agentspace: Cannot read the instance-ID cache (.+); remove it and run the command again\.\n$')
          if (-not $m.Success) { return 'Stderr is not agentspace: Cannot read the instance-ID cache <path>; remove it and run the command again.' }
          # The path as the home gave it to the program, and the same path in long form (a short 8.3 folder
          # name in %TEMP% is spelled out there), are both the cache's full path.
          $longForm = Assert-ConformancePath -ConformanceHome $h -Path $cacheFile
          if ($m.Groups[1].Value -ine $cacheFile -and $m.Groups[1].Value -ine $longForm) { return 'The path in the message is not the cache''s full path' }
        } } -Setup {
        Set-Healthy
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $cacheFile) -Force
        New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $cacheFile) | Out-Null
      }

      # ============================ status ============================
      $s = 'operator.md#status'
      $statusLines = {
        param([string]$State = 'running', [string]$Type = 'm9g.xlarge', [long]$Size = 20, [string]$Ssm = 'online', [string]$Volume = $vol)
        Join-Lines @("Instance  $id", "State     $State", "Type      $Type", "Disk      $Size GiB gp3 ($Volume)", "SSM       $Ssm")
      }
      $callsRunning = @($cDescribeInstances, $cDescribeVolumes, $cSsm)
      $callsNoSsm = @($cDescribeInstances, $cDescribeVolumes)
      Test-Case "${s}: a running box whose agent is Online prints the five lines on stderr, SSM online, with nothing on stdout, exit 0" @('status') @{
        Exit = 0; Stderr = (& $statusLines); Calls = $callsRunning }
      Test-Case "${s}: the calls are describe-instances, describe-volumes for the root volume, then ssm describe-instance-information" @('status') @{
        Exit = 0; Calls = $callsRunning }
      Test-Case "${s}: a running box whose agent is not Online (ConnectionLost) shows SSM not yet" @('status') @{
        Exit = 0; Stderr = (& $statusLines -Ssm 'not yet'); Calls = $callsRunning } -Setup { Set-Healthy -Ping 'ConnectionLost' }
      Test-Case "${s}: a running box with an empty InstanceInformationList shows SSM not yet" @('status') @{
        Exit = 0; Stderr = (& $statusLines -Ssm 'not yet'); Calls = $callsRunning } -Setup { Set-Healthy -Ping 'none' }
      foreach ($state in 'stopped', 'pending', 'stopping', 'shutting-down', 'terminated') {
        $stateName = $state
        Test-Case "${s}: a $state box shows SSM - and asks Session Manager nothing" @('status') @{
          Exit = 0; Stderr = (& $statusLines -State $stateName -Ssm '-'); Calls = $callsNoSsm } -Setup { Set-Healthy -State $stateName -Ping 'Online' }
      }
      Test-Case "${s}: the type line is the instance type, and the disk line the root volume's size and ID" @('status') @{
        Exit = 0; Stderr = (& $statusLines -Type 'm9g.4xlarge' -Size 64); Calls = $callsRunning } -Setup {
        Set-Healthy -Size 64
        Set-Answer 'ec2-describe-instances' -Out (New-InstanceDoc -Type 'm9g.4xlarge')
      }
      Test-Case "${s}: the root volume is the one attached at RootDeviceName, when it is listed after another" @('status') @{
        Exit = 0; Stderr = (& $statusLines); Calls = $callsRunning }
      Test-Case "${s}: the root volume is the one attached at RootDeviceName, when it is listed before another" @('status') @{
        Exit = 0; Stderr = (& $statusLines); Calls = $callsRunning } -Setup { Set-Healthy -RootFirst }
      Test-Case "${s}: -Quiet still prints the five lines" @('status', '-Quiet') @{
        Exit = 0; Stderr = (& $statusLines); Calls = $callsRunning }
      Test-Case "${s}: never starts the box, and starts no ssh" @('status') @{
        Exit = 0; Calls = $callsRunning; Check = { param($r, $t) if ((@(Get-AwsCalls) | Where-Object { $_ -match 'start-instances|stop-instances|modify-volume' }).Count -gt 0) { 'A call that changes the box was made' } } }
      Test-Case "${s}: a launch time that is missing does not fail it, since it does not read LaunchTime (AWS side, FB-08)" @('status') @{
        Exit = 0; Stderr = (& $statusLines); Calls = $callsRunning } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Out (New-InstanceDoc -Without 'LaunchTime')
      }
      Test-Case "${s}: a launch time that is not a time does not fail it" @('status') @{
        Exit = 0; Stderr = (& $statusLines); Calls = $callsRunning } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Out ((New-InstanceDoc) -replace '2020-01-01T00:00:00\+00:00', 'yesterday')
      }

      # ---- status: failures, in each call ----
      $sSignIn = 'operator.md#aws-failures: status'
      Test-Case "${sSignIn}, a sign-in failure of the first call prints exactly the sign-in message, exit 1, nothing on stdout" @('status') @{
        Exit = 1; Stderr = "$signIn`n"; Calls = @($cDescribeInstances) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Err $signInTexts['Unable to locate credentials'] -Code 255
      }
      Test-Case "${sSignIn}, a failed describe-instances is agentspace: and the CLI's text, and nothing more is called" @('status') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = @($cDescribeInstances) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Err $otherAwsText -Code 254
      }
      Test-Case "${sSignIn}, a failed describe-volumes ends the command at once, with no ssm call and no line printed" @('status') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = $callsNoSsm } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-volumes' -Err $otherAwsText -Code 254
      }
      Test-Case "${sSignIn}, a failed ssm describe-instance-information fails the command, and no line is printed" @('status') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = $callsRunning } -Setup {
        Set-Healthy
        Set-Answer 'ssm-describe-instance-information' -Err $otherAwsText -Code 254
      }
      Test-Case "${sSignIn}, a sign-in failure of the ssm call prints exactly the sign-in message" @('status') @{
        Exit = 1; Stderr = "$signIn`n"; Calls = $callsRunning } -Setup {
        Set-Healthy
        Set-Answer 'ssm-describe-instance-information' -Err $signInTexts['expired'] -Code 255
      }
      Test-Case "${sSignIn}, an empty error text is The AWS CLI failed with exit code <n>." @('status') @{
        Exit = 1; Stderr = "agentspace: The AWS CLI failed with exit code 2.`n"; Calls = $callsNoSsm } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-volumes' -Code 2
      }
      Test-Case "${sSignIn}, -Quiet still prints a failure" @('status', '-Quiet') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = @($cDescribeInstances) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Err $otherAwsText -Code 254
      }

      # ---- status: answers that are not what the spec reads (AWS side, What is read from each answer) ----
      $u = 'operator.md#the-aws-side'
      $member = 'Reservations[0].Instances[0]'
      $unexpected = { param([string]$Call, [string]$Why) "agentspace: Unexpected answer from ${Call}: $Why.`n" }
      Test-Case "${u}: describe-instances that is not JSON fails with Unexpected answer from ec2 describe-instances: not JSON." @('status') @{
        Exit = 1; Stderr = (& $unexpected 'ec2 describe-instances' 'not JSON'); Calls = @($cDescribeInstances) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Out 'this is not json'
      }
      Test-Case "${u}: describe-volumes that is not JSON fails with Unexpected answer from ec2 describe-volumes: not JSON." @('status') @{
        Exit = 1; Stderr = (& $unexpected 'ec2 describe-volumes' 'not JSON'); Calls = $callsNoSsm } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-volumes' -Out 'this is not json'
      }
      Test-Case "${u}: ssm describe-instance-information that is not JSON fails with Unexpected answer from ssm describe-instance-information: not JSON." @('status') @{
        Exit = 1; Stderr = (& $unexpected 'ssm describe-instance-information' 'not JSON'); Calls = $callsRunning } -Setup {
        Set-Healthy
        Set-Answer 'ssm-describe-instance-information' -Out 'this is not json'
      }
      foreach ($missing in @(
          @{ W = 'State'; M = "$member.State.Name" },
          @{ W = 'InstanceType'; M = "$member.InstanceType" },
          @{ W = 'RootDeviceName'; M = "$member.RootDeviceName" },
          @{ W = 'RootMapping'; M = "$member.BlockDeviceMappings[].Ebs.VolumeId" },
          @{ W = 'Ebs'; M = "$member.BlockDeviceMappings[].Ebs.VolumeId" })) {
        $without = $missing.W
        $wantMember = $missing.M
        Test-Case "${u}: status, describe-instances with no $without fails with Unexpected answer from ec2 describe-instances: no $wantMember." @('status') @{
          Exit = 1; Stderr = (& $unexpected 'ec2 describe-instances' "no $wantMember"); Calls = @($cDescribeInstances) } -Setup {
          Set-Healthy
          Set-Answer 'ec2-describe-instances' -Out (New-InstanceDoc -Without $without)
        }
      }
      Test-Case "${u}: status, describe-volumes with no Volumes[0].Size fails with Unexpected answer from ec2 describe-volumes: no Volumes[0].Size." @('status') @{
        Exit = 1; Stderr = (& $unexpected 'ec2 describe-volumes' 'no Volumes[0].Size'); Calls = $callsNoSsm } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-volumes' -Out '{"Volumes":[{"VolumeId":"vol-1234567890abcdef0"}]}'
      }
      Test-Case "${u}: status, describe-volumes with a Size that is not a number fails the same way" @('status') @{
        Exit = 1; Stderr = (& $unexpected 'ec2 describe-volumes' 'no Volumes[0].Size'); Calls = $callsNoSsm } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-volumes' -Out '{"Volumes":[{"Size":"twenty"}]}'
      }
      Test-Case "${u}: status, an ssm answer whose first element has no PingStatus fails with Unexpected answer from ssm describe-instance-information: no InstanceInformationList[0].PingStatus." @('status') @{
        Exit = 1; Stderr = (& $unexpected 'ssm describe-instance-information' 'no InstanceInformationList[0].PingStatus'); Calls = $callsRunning } -Setup {
        Set-Healthy
        Set-Answer 'ssm-describe-instance-information' -Out '{"InstanceInformationList":[{"InstanceId":"i-1234567890abcdef0"}]}'
      }

      # ============================ grow ============================
      $s = 'operator.md#grow'
      $growMsg1 = { param([long]$From = 20, [long]$To = 22) "Growing $vol from $From to $To GiB (EBS allows one change per volume every 6 hours)..." }
      $boot = 'The filesystem grows on its own at next boot.'
      $online = 'Growing the filesystem online...'
      $failedOnline = { param([long]$To = 22) "agentspace: Growing the filesystem online failed; the volume is already $To GiB, and the filesystem grows on its own at next boot." }
      $growCalls = { param([long]$To = 22) @($cDescribeInstances, $cDescribeVolumes, (Get-ModifyCall $To)) }
      $dfOut = "Filesystem      Size  Used Avail Use% Mounted on`n/dev/root        22G  6.1G   16G  28% /`n"
      $remoteErr = "remote-step-stderr-line`n"

      Test-Case "${s}: a stopped box has its volume read and changed by the default 2 GiB, and prints that it grows its filesystem at next boot" @('grow') @{
        Exit = 0; Stderr = (Join-Lines @((& $growMsg1), $boot)); Calls = (& $growCalls) } -Setup { Set-Healthy -State 'stopped' }
      Test-Case "${s}: a stopped box starts no ssh and does not start the box" @('grow') @{
        Exit = 0; Calls = (& $growCalls); SshCalls = 0
        Check = { param($r, $t) if ((@(Get-AwsCalls) | Where-Object { $_ -match 'start-instances|ssm' }).Count -gt 0) { 'The box was started or asked about' } } } -Setup { Set-Healthy -State 'stopped' }
      Test-Case "${s}: -GiB 5 adds 5 to the size the volume has" @('grow', '-GiB', '5') @{
        Exit = 0; Stderr = (Join-Lines @((& $growMsg1 -To 25), $boot)); Calls = (& $growCalls -To 25) } -Setup { Set-Healthy -State 'stopped' }
      Test-Case "${s}: <new> is the volume's own size plus <n>, not a fixed size (a 31 GiB volume grows to 33)" @('grow') @{
        Exit = 0; Stderr = (Join-Lines @((& $growMsg1 -From 31 -To 33), $boot)); Calls = (& $growCalls -To 33) } -Setup { Set-Healthy -State 'stopped' -Size 31 }
      Test-Case "${s}: -GiB +3 is a decimal integer with a plus sign, and adds 3" @('grow', '-GiB', '+3') @{
        Exit = 0; Stderr = (Join-Lines @((& $growMsg1 -To 23), $boot)); Calls = (& $growCalls -To 23) } -Setup { Set-Healthy -State 'stopped' }
      Test-Case "${s}: -GiB -1 gives -GiB the value -1, and the operator asks EC2 for 19 GiB" @('grow', '-GiB', '-1') @{
        Exit = 0; Stderr = (Join-Lines @((& $growMsg1 -To 19), $boot)); Calls = (& $growCalls -To 19) } -Setup { Set-Healthy -State 'stopped' }
      Test-Case "${s}: the parameter names are matched without regard to case (-gib, -QUIET)" @('grow', '-gib', '4', '-QUIET') @{
        Exit = 0; Stderr = ''; Calls = (& $growCalls -To 24) } -Setup { Set-Healthy -State 'stopped' }
      Test-Case "${s}: -Quiet prints no progress message on a stopped box, and still changes the volume" @('grow', '-Quiet') @{
        Exit = 0; Stderr = ''; Calls = (& $growCalls) } -Setup { Set-Healthy -State 'stopped' }
      foreach ($state in 'stopping', 'pending') {
        $stateName = $state
        Test-Case "${s}: a $state box is not running, so it prints that it grows at next boot and starts no ssh" @('grow') @{
          Exit = 0; Stderr = (Join-Lines @((& $growMsg1), $boot)); Calls = (& $growCalls); SshCalls = 0 } -Setup { Set-Healthy -State $stateName }
      }

      # ---- A running box: the online grow ----
      $okStreams = { Set-Healthy; Set-Answer 'ssh' -Out $dfOut -Err $remoteErr -Code 0 }
      Test-Case "${s}: a running box prints Growing the filesystem online..., waits $growWaitSeconds seconds, starts one ssh, and passes its stdout and stderr through, exit 0" @('grow') @{
        Exit = 0; Stdout = $dfOut; Stderr = (Join-Lines @((& $growMsg1), $online, 'remote-step-stderr-line')); Calls = (& $growCalls); SshCalls = 1
        Check = {
          param($r, $t)
          $times = @(Get-Content -LiteralPath (Join-Path $standins 'run\ssh-time.log') -ErrorAction SilentlyContinue | Where-Object { $_.Trim() -ne '' })
          if ($times.Count -ne 1) { return 'The ssh did not record one start time' }
          $gap = ([DateTime]::new([long]$times[0], [DateTimeKind]::Utc) - $t).TotalSeconds
          if ($gap -lt ($growWaitSeconds - 0.1)) { 'The ssh started less than 10 seconds after the operator did' }
        } } -Setup $okStreams
      Test-Case "${s}: the ssh is ssh -o ClearAllForwardings=yes agentspace <remote command>, the command one last argument" @('grow') @{
        Exit = 0; Stdout = $dfOut; Calls = (& $growCalls); SshCalls = 1
        Check = {
          param($r, $t)
          $a = @(ConvertFrom-Json -InputObject (@(Get-SshCalls))[0])
          if ($a.Count -ne 4) { return "The ssh got $($a.Count) arguments, not 4" }
          if ($a[0] -cne '-o' -or $a[1] -cne 'ClearAllForwardings=yes' -or $a[2] -cne 'agentspace') { return 'The first three ssh arguments are not -o ClearAllForwardings=yes agentspace' }
          if ([string]::IsNullOrWhiteSpace($a[3])) { return 'The remote command is empty' }
          if ($a[3] -match '[%"\x00-\x1f\x7f]') { return 'The remote command holds a percent sign, a double quote or a control character (gap FA-15)' }
          if ($a[3].EndsWith('\')) { return 'The remote command ends in a backslash (gap FA-15)' }
        } } -Setup $okStreams
      Test-Case "${s}: a running box makes no call to start the box, and no scp" @('grow') @{
        Exit = 0; Stdout = $dfOut; Calls = (& $growCalls); SshCalls = 1 } -Setup $okStreams
      Test-Case "${s}: -GiB 4 on a running box asks EC2 for the size plus 4 and prints it in both messages" @('grow', '-GiB', '4') @{
        Exit = 0; Stdout = $dfOut; Stderr = (Join-Lines @((& $growMsg1 -To 24), $online, 'remote-step-stderr-line')); Calls = (& $growCalls -To 24); SshCalls = 1 } -Setup $okStreams
      Test-Case "${s}: -Quiet prints no progress message, and still passes the ssh's output through" @('grow', '-Quiet') @{
        Exit = 0; Stdout = $dfOut; Stderr = 'remote-step-stderr-line' + "`n"; Calls = (& $growCalls); SshCalls = 1 } -Setup $okStreams

      # ---- D36: a failed online grow ----
      foreach ($code in 1, 255) {
        $sshCode = $code
        Test-Case "${s}: an ssh that exits $code fails grow with the online-grow message, exit 1, and the volume's change stands (D36)" @('grow') @{
          Exit = 1; Stdout = $dfOut; Stderr = (Join-Lines @((& $growMsg1), $online, 'remote-step-stderr-line', (& $failedOnline))); Calls = (& $growCalls); SshCalls = 1 } -Setup {
          Set-Healthy
          Set-Answer 'ssh' -Out $dfOut -Err $remoteErr -Code $sshCode
        }
      }
      Test-Case "${s}: a failed online grow with -GiB 3 names the new size, 23 GiB, in its message" @('grow', '-GiB', '3') @{
        Exit = 1; Stderr = (Join-Lines @((& $growMsg1 -To 23), $online, (& $failedOnline -To 23))); Calls = (& $growCalls -To 23); SshCalls = 1 } -Setup {
        Set-Healthy
        Set-Answer 'ssh' -Code 255
      }
      Test-Case "${s}: -Quiet still prints the failed online grow, and nothing before it" @('grow', '-Quiet') @{
        Exit = 1; Stderr = (Join-Lines @((& $failedOnline))); Calls = (& $growCalls); SshCalls = 1 } -Setup {
        Set-Healthy
        Set-Answer 'ssh' -Code 1
      }

      # ---- A refused volume change, and the AWS failures ----
      Test-Case "${s}: a refused volume change (a second change within 6 hours) fails with the CLI's error after the first message, and starts no ssh" @('grow') @{
        Exit = 1; Stderr = (Join-Lines @((& $growMsg1), 'agentspace: An error occurred (VolumeModificationRateExceeded) when calling the ModifyVolume operation: You''ve reached the maximum modification rate per volume limit.')); Calls = (& $growCalls); SshCalls = 0 } -Setup {
        Set-Healthy
        Set-Answer 'ec2-modify-volume' -Err 'An error occurred (VolumeModificationRateExceeded) when calling the ModifyVolume operation: You''ve reached the maximum modification rate per volume limit.' -Code 254
      }
      Test-Case "${s}: -GiB 0 is passed on to EC2 as the same size, and the CLI's refusal ends grow with no ssh" @('grow', '-GiB', '0') @{
        Exit = 1; Stderr = (Join-Lines @((& $growMsg1 -To 20), 'agentspace: An error occurred (InvalidParameterValue) when calling the ModifyVolume operation: New size cannot be smaller than existing size.')); Calls = (& $growCalls -To 20); SshCalls = 0 } -Setup {
        Set-Healthy
        Set-Answer 'ec2-modify-volume' -Err 'An error occurred (InvalidParameterValue) when calling the ModifyVolume operation: New size cannot be smaller than existing size.' -Code 254
      }
      Test-Case "${s}: the answer of ec2 modify-volume is not read beyond its exit code" @('grow') @{
        Exit = 0; Stderr = (Join-Lines @((& $growMsg1), $boot)); Calls = (& $growCalls) } -Setup {
        Set-Healthy -State 'stopped'
        Set-Answer 'ec2-modify-volume' -Out 'not json at all'
      }
      Test-Case "${s}: a sign-in failure of the first call prints exactly the sign-in message, nothing else, exit 1" @('grow') @{
        Exit = 1; Stderr = "$signIn`n"; Calls = @($cDescribeInstances) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Err $signInTexts['aws login'] -Code 255
      }
      Test-Case "${s}: a sign-in failure of modify-volume prints the first message, then exactly the sign-in message" @('grow') @{
        Exit = 1; Stderr = (Join-Lines @((& $growMsg1), $signIn)); Calls = (& $growCalls) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-modify-volume' -Err $signInTexts['expired'] -Code 255
      }
      Test-Case "${s}: a failed describe-volumes prints no message and changes nothing" @('grow') @{
        Exit = 1; Stderr = "agentspace: $otherAwsText`n"; Calls = @($cDescribeInstances, $cDescribeVolumes) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-volumes' -Err $otherAwsText -Code 254
      }
      Test-Case "${s}: a describe-volumes answer with no Volumes[0].Size fails with Unexpected answer, and changes nothing" @('grow') @{
        Exit = 1; Stderr = (& $unexpected 'ec2 describe-volumes' 'no Volumes[0].Size'); Calls = @($cDescribeInstances, $cDescribeVolumes) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-volumes' -Out '{"Volumes":[{}]}'
      }
      Test-Case "${s}: a describe-volumes answer that is not JSON fails with Unexpected answer: not JSON, and changes nothing" @('grow') @{
        Exit = 1; Stderr = (& $unexpected 'ec2 describe-volumes' 'not JSON'); Calls = @($cDescribeInstances, $cDescribeVolumes) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-volumes' -Out '<html>'
      }
      Test-Case "${s}: a describe-instances answer with no State.Name fails with Unexpected answer, and changes nothing" @('grow') @{
        Exit = 1; Stderr = (& $unexpected 'ec2 describe-instances' "no $member.State.Name"); Calls = @($cDescribeInstances) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Out (New-InstanceDoc -Without 'State')
      }
      Test-Case "${s}: a describe-instances answer with no root volume fails with Unexpected answer, and changes nothing" @('grow') @{
        Exit = 1; Stderr = (& $unexpected 'ec2 describe-instances' "no $member.BlockDeviceMappings[].Ebs.VolumeId"); Calls = @($cDescribeInstances) } -Setup {
        Set-Healthy
        Set-Answer 'ec2-describe-instances' -Out (New-InstanceDoc -Without 'RootMapping')
      }
      Test-Case "${s}: an instance type that is missing does not fail it, since it does not read InstanceType (AWS side, FB-08)" @('grow') @{
        Exit = 0; Stderr = (Join-Lines @((& $growMsg1), $boot)); Calls = (& $growCalls) } -Setup {
        Set-Healthy -State 'stopped'
        Set-Answer 'ec2-describe-instances' -Out (New-InstanceDoc -State 'stopped' -Without 'InstanceType', 'LaunchTime')
      }

      # ---- The refusals of grow's arguments: usage errors, before any AWS call (Command line) ----
      $c = 'operator.md#command-line'
      Test-Case "${s}: -GiB abc is refused with -GiB MUST be a whole number, not 'abc'., and no AWS call" @('grow', '-GiB', 'abc') @{
        Exit = 1; Stderr = "agentspace: -GiB MUST be a whole number, not 'abc'.`n"; Calls = @() }
      Test-Case "${s}: -GiB 2147483648 is out of the 32-bit range, so it is refused as not a whole number, and no AWS call" @('grow', '-GiB', '2147483648') @{
        Exit = 1; Stderr = "agentspace: -GiB MUST be a whole number, not '2147483648'.`n"; Calls = @() }
      Test-Case "${s}: -GiB with no value is refused with -GiB needs a value., and no AWS call" @('grow', '-GiB') @{
        Exit = 1; Stderr = "agentspace: -GiB needs a value.`n"; Calls = @() }
      Test-Case "${s}: -GiB given twice is refused with -GiB is given more than once., and no AWS call" @('grow', '-GiB', '1', '-GiB', '2') @{
        Exit = 1; Stderr = "agentspace: -GiB is given more than once.`n"; Calls = @() }
      Test-Case "${s}: -Alias is refused with -Alias applies to up alone., and no AWS call" @('grow', '-Alias', 'x') @{
        Exit = 1; Stderr = "agentspace: -Alias applies to up alone.`n"; Calls = @() }
      Test-Case "${s}: a second positional argument is refused with Unexpected argument 'extra'., and no AWS call" @('grow', 'extra') @{
        Exit = 1; Stderr = "agentspace: Unexpected argument 'extra'.`n"; Calls = @() }
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-commands.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
if ($global:ConformanceCounts['FAIL'] -gt 0) {
  Write-Host 'NOTE This file is expected to fail against the prototype (the test seam, card #0085); a rewrite MUST pass it.'
}
Exit-ConformanceRun

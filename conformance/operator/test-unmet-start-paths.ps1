#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the start-and-wait step that the operator's `up` makes (docs/operator.md,
# "Starting the box and waiting for it" and "What up waits for"), run through the test seam
# (docs/operator.md, "The test seam"): the stopped and the stopping box, the 20-minute rule, the
# wait's messages and poll, the patch question's answers, and the 15-minute deadline (card #0132,
# gap FB-25).
#
# EXPECTED TO FAIL AGAINST THE PROTOTYPE, because it does not meet the seam: it runs the AWS CLI by a
# fixed path and reads none of AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD or AGENTSPACE_SCP_CMD (card
# #0085; docs/operator.md, "Requirements the prototype does not meet yet"). Until a build calls the
# stand-in, the file reports `FAIL ... seam not met` and checks nothing more. A rewrite MUST pass every
# result line. The result lines still say FAIL: nothing turns a FAIL into a pass.
#
# What it does, when the seam is met: it writes into a throwaway home the instance-ID cache and a stand-in `aws` whose
# answers change from call to call. The stand-in reads a script of answers (one list per
# `<service> <operation>`, each call taking the next answer and the last repeating) and logs every
# call's whole argument list and the time it was made. It answers the patch question through Session
# Manager's Run Command, as the spec says: "never over SSH". `ssh` and `scp` are tripwires that fail
# at once, and AGENTSPACE_NESTED is set, so no case pushes the token. It runs `up` once per case and
# checks only what it can see: the calls in order with their arguments, the messages on stderr, the
# exit code and stdout.
#
# Cases (-Case takes a wildcard on these names, as conformance/keeper/test-keeper.ps1's does):
#   stopped, stopping, stopping-start-fails, running-recent, running-old, pending-state,
#   other-states, bad-launch-time, wait-messages, wait-answers, wait-invocation-polls,
#   wait-20-polls and deadline.
#
# THE DEADLINE CASE WAITS THE REAL 15 MINUTES, and so it runs only when asked for (docs/conformance.md,
# "Cases that run only when asked for", architect, 2026-11-19, corrected the same evening). No seam
# variable shortens the deadline, and a check MUST NOT trust a time to be shorter (docs/conformance.md,
# "The program under test is untrusted"). It runs only when -Case is exactly `deadline` (in any letter
# case), under a limit of its own on `up` of 17 minutes, so that it can never hang. With any other
# -Case that would select it, the default `*` and a wildcard such as `d*` included, it neither runs nor
# reports it: it writes no result line for it, so a default run's results and exit code are those of
# the other cases alone, and prints the one line that is not a result, `Not run: the 15-minute deadline
# case; run it with -Case deadline.` Every other case runs by default, in about 2 to 3 minutes: 31
# results, all PASS, exit 0.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can
# run, and reports SKIPPED, running nothing, if it does not. A build that does not meet the seam never
# calls the stand-in: the file then reports one `seam not met` result and checks nothing more, so it
# never waits out the deadline on a program that ignores the seam. The instance ID is made up.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test"). A
# failure reason never quotes the program's output.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-start-paths.ps1 [-Case <wildcard>|deadline]
param(
  [string]$Case = '*'
)
. (Join-Path $PSScriptRoot 'helpers.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#starting-the-box-and-waiting-for-it'
$secWait = 'operator.md#what-up-waits-for'
$seamSec = 'operator.md#the-test-seam'
$id = 'i-1234567890abcdef0'
$commandId = 'c0ffee00-1111-4222-8333-abcdefabcdef'
$msgSsm = 'Waiting for Session Manager...'
$msgPatching = 'Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...'
$msgDeadline = "agentspace: $id was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
$msgReady = "Ready: $id"
$deadlineNotRun = 'Not run: the 15-minute deadline case; run it with -Case deadline.'
$deadlineLimitSeconds = 17 * 60
# The deadline is 15 minutes from the start of the wait, and the check allows it to fall from 14.5 to
# 16.5 minutes (docs/conformance.md, "Cases that run only when asked for"): one number in the check
# and in the doc.
$deadlineMinSeconds = 14.5 * 60
$deadlineMaxSeconds = 16.5 * 60

# The stand-in aws: a Windows PowerShell 5.1 script that a .cmd starts, so that it sees the arguments
# as a program would. Each call logs {"t": <UTC ticks>, "a": [<arguments>]}, then answers with the
# next step of the scenario file's list for "<service> <operation>" (the last step repeats, and an
# operation with no list answers nothing and exits 0). It reads and writes nothing outside its folder.
$awsStandInScript = @'
$dir = $PSScriptRoot
$key = [string]$args[0] + ' ' + [string]$args[1]
$line = ConvertTo-Json -Compress -InputObject @{ t = [string][DateTime]::UtcNow.Ticks; a = @($args) }
[IO.File]::AppendAllText((Join-Path $dir 'aws-args.log'), ($line + "`n"))
$scenario = ConvertFrom-Json -InputObject (Get-Content -LiteralPath (Join-Path $dir 'aws-scenario.json') -Raw)
$prop = $scenario.PSObject.Properties[$key]
if ($null -eq $prop) { exit 0 }
$steps = @($prop.Value)
$countFile = Join-Path $dir ('aws-count-' + ($key -replace '[^a-z0-9]', '_') + '.txt')
$n = 0
if (Test-Path -LiteralPath $countFile) { $n = [int](Get-Content -LiteralPath $countFile -Raw).Trim() }
[IO.File]::WriteAllText($countFile, [string]($n + 1))
$step = $steps[[Math]::Min($n, $steps.Count - 1)]
if ($step.out) { [Console]::Out.Write([string]$step.out) }
if ($step.err) { [Console]::Error.Write([string]$step.err) }
exit ([int]$step.code)
'@

function Write-Problems([string]$Name, $Problems) {
  if ($Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS } else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
}
function New-Problems { , [System.Collections.Generic.List[string]]::new() }
# The lines of the operator's stderr, split at line feeds alone: a carriage return stays in its line, and
# a blank line is a line, so a line that is not exactly the spec's text fails the comparison (as the
# other two files' exact comparisons do). The line feed that ends the last line ends no further line.
function Get-Lines($Run) {
  $t = [string]$Run.Stderr
  if ($t.Length -eq 0) { return , @() }
  if ($t.EndsWith("`n")) { $t = $t.Substring(0, $t.Length - 1) }
  , @($t -split "`n")
}

# One answer of the stand-in: what it writes to stdout and stderr, and its exit code.
function New-Step([string]$Out = '', [string]$Err = '', [int]$Code = 0) { @{ out = $Out; err = $Err; code = $Code } }
function New-Describe([string]$State, [string]$Launch = '') {
  $inst = @{ State = @{ Name = $State } }
  if ($Launch) { $inst['LaunchTime'] = $Launch }
  New-Step (ConvertTo-Json -Compress -Depth 8 -InputObject @{ Reservations = @(@{ Instances = @($inst) }) })
}
function New-Dii([string]$Ping = '') {
  $list = @()
  if ($Ping) { $list = @(@{ PingStatus = $Ping }) }
  New-Step (ConvertTo-Json -Compress -Depth 8 -InputObject @{ InstanceInformationList = $list })
}
function New-Sent { New-Step (ConvertTo-Json -Compress -InputObject @{ Command = @{ CommandId = $commandId } }) }
function New-Invocation([string]$Status, [string]$Output) {
  New-Step (ConvertTo-Json -Compress -InputObject @{ Status = $Status; StandardOutputContent = $Output })
}
function Get-LaunchTime([double]$MinutesAgo) {
  [DateTime]::UtcNow.AddMinutes(-$MinutesAgo).ToString("yyyy-MM-dd'T'HH:mm:ss", [Globalization.CultureInfo]::InvariantCulture) + '+00:00'
}

# Names for the calls the spec lists (Calls by command), by their whole argument list in the order
# the spec gives (FB-07): the service, the operation, --region us-west-2, --output json where it is
# passed, then the operation's own arguments. The send-command's question is the box's own wording,
# so its value is compared as `commands=<question>`. The wait is exactly
# `ec2 wait --region us-west-2 instance-stopped --instance-ids <id>`, with no `--output json`
# (docs/operator.md, AWS failures, the FB-07 paragraph that begins "Every AWS CLI call the operator
# makes passes --region us-west-2": the operation is `wait`, so --region follows it, and
# `instance-stopped` is the wait's own argument; architect, 2026-11-19, for E0.06-T03-S03). Any other
# placement, and a wait that passes --output json, is no call of this list and fails the check.
$symbols = @{
  (@('ec2', 'describe-instances', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $id) -join '|') = 'describe'
  (@('ec2', 'start-instances', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $id) -join '|') = 'start'
  (@('ec2', 'wait', '--region', 'us-west-2', 'instance-stopped', '--instance-ids', $id) -join '|') = 'wait'
  (@('ssm', 'describe-instance-information', '--region', 'us-west-2', '--output', 'json', '--filters', "Key=InstanceIds,Values=$id") -join '|') = 'dii'
  (@('ssm', 'send-command', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $id, '--document-name', 'AWS-RunShellScript', '--comment', 'agentspace up: patch check', '--parameters', 'commands=<question>') -join '|') = 'send'
  (@('ssm', 'get-command-invocation', '--region', 'us-west-2', '--output', 'json', '--command-id', $commandId, '--instance-id', $id) -join '|') = 'get'
}

$h = New-ConformanceHome
try {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  $seamName = "${seamSec}: up runs AGENTSPACE_AWS_CMD, and its start-and-wait step asks the box's state"
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $seamName -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name $seamName -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
  } else {
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $h.UserProfile '.agentspace\instance-id.json')), ('{"version":0,"instance_id":"' + $id + '"}'))
    $standinDir = Join-Path $h.Root 'standins'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $standinDir) | Out-Null
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $standinDir 'aws-standin.ps1')), $awsStandInScript)
    $aws = New-OperatorStandIn -ConformanceHome $h -Name 'aws' -Body @(
      'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0aws-standin.ps1" %*',
      'exit /b %ERRORLEVEL%')
    $trip = Get-OperatorTripwireEnvironment -ConformanceHome $h
    $standins = Split-Path -Parent $aws
    $upEnv = @{ AGENTSPACE_AWS_CMD = $aws; AGENTSPACE_SSH_CMD = $trip['AGENTSPACE_SSH_CMD']; AGENTSPACE_SCP_CMD = $trip['AGENTSPACE_SCP_CMD']; AGENTSPACE_NESTED = '1' }

    # Puts the answers for the next run, and forgets the last run's calls and counters.
    function Set-Scenario([hashtable]$Scenario) {
      foreach ($f in @(Get-ChildItem -LiteralPath $standins -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'aws-count-*' -or $_.Name -in 'aws-args.log', 'aws-calls.log' })) {
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $f.FullName) -Force -ErrorAction SilentlyContinue
      }
      $json = ConvertTo-Json -Depth 10 -Compress -InputObject $Scenario
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $standins 'aws-scenario.json')), $json)
    }
    function Invoke-Up([int]$Timeout = 180) {
      Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments @('up') -Environment $upEnv -TimeoutSeconds $Timeout
    }
    # The calls the stand-in logged, in order: Symbol (a name from $symbols, or ? and the operation),
    # Time (UTC ticks) and Args.
    function Get-AwsCalls {
      $log = Join-Path $standins 'aws-args.log'
      if (-not (Test-Path -LiteralPath $log -PathType Leaf)) { return @() }
      @(Get-Content -LiteralPath $log | Where-Object { $_.Trim() -ne '' } | ForEach-Object {
          $o = ConvertFrom-Json -InputObject $_
          $a = @($o.a | ForEach-Object { [string]$_ })
          $norm = @($a | ForEach-Object { if ($_ -cmatch '^commands=') { 'commands=<question>' } else { $_ } }) -join '|'
          $sym = if ($symbols.ContainsKey($norm)) { $symbols[$norm] } else { '?' + (($a | Select-Object -First 2) -join ' ') }
          [pscustomobject]@{ Symbol = $sym; Ticks = [long]$o.t; Args = $a }
        })
    }
    # The check of a run's calls: the symbols, in order, equal to $Want.
    function Add-CallProblems($Problems, $Calls, [string[]]$Want) {
      $got = @($Calls | ForEach-Object { $_.Symbol })
      if (($got -join ',') -ceq ($Want -join ',')) { return }
      $i = 0
      while ($i -lt $got.Count -and $i -lt $Want.Count -and $got[$i] -ceq $Want[$i]) { $i++ }
      $at = if ($i -lt $Want.Count) { "expected $($Want[$i])" } else { 'expected no more calls' }
      $was = if ($i -lt $got.Count) { "got $($got[$i] -replace '[^\x20-\x7E]', '?')" } else { 'got none' }
      $Problems.Add("The calls differ at call $($i + 1) of $($Want.Count) ($at, $was; $($got.Count) calls were made)")
    }
    function Add-RunProblems($Problems, $Run, [int]$ExitCode) {
      if ($Run.TimedOut) { $Problems.Add('It timed out') }
      elseif ($Run.ExitCode -ne $ExitCode) { $Problems.Add("Exit $($Run.ExitCode), not $ExitCode") }
      if ($Run.Stdout.Length -gt 0) { $Problems.Add('Stdout is not empty') }
      if ($Run.Stderr.Contains("`r")) { $Problems.Add('Stderr holds a carriage return') }
      if ($Run.Stderr.Length -gt 0 -and -not $Run.Stderr.EndsWith("`n")) { $Problems.Add('Stderr does not end in a line feed') }
      if ($Run.Stderr -match 'CategoryInfo|At line:|Exception') { $Problems.Add('Stderr shows a runtime error record') }
    }
    function Add-LinesProblems($Problems, $Run, [string[]]$Want) {
      $lines = Get-Lines $Run
      if (($lines -join "`n") -cne ($Want -join "`n")) { $Problems.Add("Stderr is not exactly $($Want.Count) line(s): $($Want -join ' / ')") }
    }
    # The gaps in seconds between consecutive calls of one kind.
    function Get-Gaps($Calls, [string]$Symbol) {
      $t = @($Calls | Where-Object { $_.Symbol -ceq $Symbol } | ForEach-Object { $_.Ticks })
      for ($i = 1; $i -lt $t.Count; $i++) { ($t[$i] - $t[$i - 1]) / 1e7 }
    }
    function Get-Selected([string]$Name) { $Name -like $Case }
    # The box's patch check, asked once and answered at once.
    $askedOnce = @{
      'ssm describe-instance-information' = @(New-Dii 'Online')
      'ssm send-command'                  = @(New-Sent)
    }

    # ---- The seam: up calls the stand-in, with the box found old, so that it does not wait ----
    Set-Scenario @{ 'ec2 describe-instances' = @((New-Describe 'running'), (New-Describe 'running' (Get-LaunchTime 3000))) }
    $r = Invoke-Up
    if ((Get-AwsCalls).Count -eq 0) {
      Write-ConformanceResult -Name $seamName -Status FAIL -Reason 'seam not met (AGENTSPACE_AWS_CMD was not used)'
    } else {
      Write-ConformanceResult -Name $seamName -Status PASS

      # ---- A stopped box ----
      if (Get-Selected 'stopped') {
        Set-Scenario ($askedOnce + @{
            'ec2 describe-instances'         = @(New-Describe 'stopped')
            'ssm get-command-invocation'     = @(New-Invocation 'Success' "patched`n")
          })
        $r = Invoke-Up
        $calls = Get-AwsCalls
        $p = New-Problems
        Add-RunProblems $p $r 0
        Add-LinesProblems $p $r @("Starting $id...", $msgReady)
        Write-Problems "${sec}: A stopped box prints Starting <id>..., then Ready: <id>, exits 0, writes nothing to stdout, and the first describe-instances needs no LaunchTime (step 1; FB-08)" $p

        $p = New-Problems
        Add-CallProblems $p $calls @('describe', 'start', 'dii', 'send', 'get')
        Write-Problems "${sec}: A stopped box is described, started with ec2 start-instances, then waited for with Session Manager's checks, each call with the arguments of Calls by command, in the order of FB-07 (steps 1 and 3)" $p

        $p = New-Problems
        $n = @($calls | Where-Object { $_.Symbol -ceq 'describe' }).Count
        if ($n -ne 1) { $p.Add("describe-instances was called $n times, not once") }
        Write-Problems "${sec}: A stopped box waits without the second describe-instances for the launch time, which runs only when step 1 read running (step 2; FB-09)" $p
      }

      # ---- A stopping box ----
      if (Get-Selected 'stopping') {
        $waitErr = 'conformance-wait-stderr-marker'
        Set-Scenario ($askedOnce + @{
            'ec2 describe-instances'     = @(New-Describe 'stopping' 'not-a-time')
            'ec2 wait'                   = @(New-Step 'conformance-wait-stdout-marker' ($waitErr + "`n") 255)
            'ssm get-command-invocation' = @(New-Invocation 'Success' "patched`n")
          })
        $r = Invoke-Up
        $calls = Get-AwsCalls
        $lines = Get-Lines $r
        $p = New-Problems
        Add-RunProblems $p $r 0
        $want = @("Waiting for $id to finish stopping...", $waitErr, "Starting $id...", $msgReady)
        if (($lines -join "`n") -cne ($want -join "`n")) { $p.Add('Stderr is not exactly: Waiting for <id> to finish stopping..., the wait''s own stderr line, Starting <id>..., Ready: <id>') }
        Write-Problems "${sec}: A stopping box prints Waiting for <id> to finish stopping..., waits, then prints Starting <id>... and Ready: <id>; what the wait writes to stderr passes through in a person's up, in order, and its stdout never reaches stdout (step 1)" $p

        $p = New-Problems
        Add-CallProblems $p $calls @('describe', 'wait', 'start', 'dii', 'send', 'get')
        if (@($calls | Where-Object { $_.Args.Count -ge 2 -and $_.Args[0] -ceq 'ec2' -and $_.Args[1] -ceq 'wait' -and $_.Args -ccontains '--output' }).Count -gt 0) { $p.Add('The wait passes --output, and the FB-07 paragraph of operator.md gives it none') }
        Write-Problems "${sec}: A stopping box is waited for with ec2 wait --region us-west-2 instance-stopped --instance-ids <id> (the FB-07 paragraph of operator.md: that argument order, no --output json), then started, and describe-instances is not called again (steps 1 and 2; FB-09)" $p

        $p = New-Problems
        if ($r.ExitCode -ne 0 -or -not ($lines -ccontains $msgReady)) { $p.Add('A wait that exited 255 stopped up, but its exit code is not checked') }
        Write-Problems "${sec}: The wait's exit code is not checked: a wait that exits 255 still leads to the start request and a ready box (B-34)" $p
      }

      if (Get-Selected 'stopping-start-fails') {
        $startErr = 'An error occurred (IncorrectInstanceState) when calling the StartInstances operation: The instance is not in a state from which it can be started.'
        Set-Scenario @{
          'ec2 describe-instances' = @(New-Describe 'stopping')
          'ec2 wait'               = @(New-Step '' "Waiter InstanceStopped failed: Max attempts exceeded`n" 255)
          'ec2 start-instances'    = @(New-Step '' ($startErr + "`n") 254)
        }
        $r = Invoke-Up
        $calls = Get-AwsCalls
        $lines = Get-Lines $r
        $p = New-Problems
        Add-RunProblems $p $r 1
        Add-CallProblems $p $calls @('describe', 'wait', 'start')
        if ($lines.Count -eq 0 -or $lines[-1] -cne "agentspace: $startErr") { $p.Add('The last line on stderr is not agentspace: and the start request''s own error text') }
        if ($lines -ccontains $msgReady) { $p.Add('Ready was printed') }
        Write-Problems "${sec}: A wait that gave up leaves a box EC2 will not start, so the start request fails the command with agentspace: and its own error text, exit 1, and nothing after it is called (step 1)" $p
      }

      # ---- A running or pending box: the 20-minute rule ----
      if (Get-Selected 'running-recent') {
        foreach ($c in @(@(10, 'launched 10 minutes ago'), @(19, 'launched 19 minutes ago'), @(-10, 'with a launch time in the future'))) {
          Set-Scenario ($askedOnce + @{
              'ec2 describe-instances'     = @((New-Describe 'running'), (New-Describe 'running' (Get-LaunchTime $c[0])))
              'ssm get-command-invocation' = @(New-Invocation 'Success' "patched`n")
            })
          $r = Invoke-Up
          $p = New-Problems
          Add-RunProblems $p $r 0
          Add-LinesProblems $p $r @($msgReady)
          Add-CallProblems $p (Get-AwsCalls) @('describe', 'describe', 'dii', 'send', 'get')
          Write-Problems "${sec}: A running box $($c[1]) is waited for: the launch time is read, then Session Manager and the patch check are asked, and up exits 0 (step 2)" $p
        }
      }
      if (Get-Selected 'running-old') {
        foreach ($c in @(@(21, 'launched 21 minutes ago'), @(4320, 'launched 3 days ago'))) {
          Set-Scenario ($askedOnce + @{
              'ec2 describe-instances'     = @((New-Describe 'running'), (New-Describe 'running' (Get-LaunchTime $c[0])))
              'ssm get-command-invocation' = @(New-Invocation 'Success' "pending`n")
            })
          $r = Invoke-Up
          $p = New-Problems
          Add-RunProblems $p $r 0
          Add-LinesProblems $p $r @($msgReady)
          Add-CallProblems $p (Get-AwsCalls) @('describe', 'describe')
          Write-Problems "${sec}: A running box $($c[1]) is not waited for: up reads the launch time, asks Session Manager nothing, prints Ready: <id> and exits 0 (step 2)" $p
        }
      }
      if (Get-Selected 'pending-state') {
        Set-Scenario ($askedOnce + @{
            'ec2 describe-instances'     = @(New-Describe 'pending')
            'ssm get-command-invocation' = @(New-Invocation 'Success' "patched`n")
          })
        $r = Invoke-Up
        $p = New-Problems
        Add-RunProblems $p $r 0
        Add-LinesProblems $p $r @($msgReady)
        Add-CallProblems $p (Get-AwsCalls) @('describe', 'dii', 'send', 'get')
        Write-Problems "${sec}: A pending box is not started and not described again, and is waited for (steps 1 and 2; FB-09)" $p
      }

      # ---- Any other state, and a launch time that is not of the form ----
      if (Get-Selected 'other-states') {
        foreach ($state in 'shutting-down', 'terminated') {
          Set-Scenario @{ 'ec2 describe-instances' = @(New-Describe $state) }
          $r = Invoke-Up
          $lines = Get-Lines $r
          $p = New-Problems
          Add-RunProblems $p $r 1
          Add-CallProblems $p (Get-AwsCalls) @('describe')
          if ($lines.Count -eq 0 -or $lines[-1] -cne "agentspace: Instance $id is $state.") { $p.Add('The last line on stderr is not agentspace: Instance <id> is <state>.') }
          if ($lines | Where-Object { $_ -like 'Starting *' -or $_ -like 'Ready:*' }) { $p.Add('A progress line was printed') }
          Write-Problems "${sec}: A box that is $state fails with agentspace: Instance <id> is $state., exit 1, and nothing is started or waited for (step 1)" $p
        }
      }
      if (Get-Selected 'bad-launch-time') {
        foreach ($c in @(@('', 'missing'), @('2026-01-01T00:00:00', 'without a UTC offset'))) {
          Set-Scenario @{ 'ec2 describe-instances' = @((New-Describe 'running'), (New-Describe 'running' $c[0])) }
          $r = Invoke-Up
          $lines = Get-Lines $r
          $p = New-Problems
          Add-RunProblems $p $r 1
          Add-CallProblems $p (Get-AwsCalls) @('describe', 'describe')
          if ($lines.Count -eq 0 -or $lines[-1] -cne 'agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime.') { $p.Add('The last line on stderr is not agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime.') }
          Write-Problems "${sec}: A running box whose LaunchTime is $($c[1]) fails with the Unexpected answer message for that member, exit 1, and no wait follows (step 2; Calls by command)" $p
        }
      }

      # ---- The wait: its messages, once each while they do not change, and its poll ----
      if (Get-Selected 'wait-messages') {
        # Seven checks, 5 seconds apart: no agent, no agent, then online and pending twice, then no
        # agent again, then online and pending, then online and patched.
        $dii = @((New-Dii ''), (New-Dii 'ConnectionLost'), (New-Dii 'Online'), (New-Dii 'Online'), (New-Dii ''), (New-Dii 'Online'), (New-Dii 'Online'))
        Set-Scenario @{
          'ec2 describe-instances'          = @((New-Describe 'running'), (New-Describe 'running' (Get-LaunchTime 1)))
          'ssm describe-instance-information' = $dii
          'ssm send-command'                = @(New-Sent)
          'ssm get-command-invocation'      = @((New-Invocation 'Success' "pending`r`n"), (New-Invocation 'Success' "  pending `n"), (New-Invocation 'Success' "pending`n"), (New-Invocation 'Success' "patched`n"))
        }
        $r = Invoke-Up
        $calls = Get-AwsCalls
        $p = New-Problems
        Add-RunProblems $p $r 0
        Add-LinesProblems $p $r @($msgSsm, $msgPatching, $msgSsm, $msgPatching, $msgReady)
        Write-Problems "${sec}: A message is printed only when it differs from the last one printed: the agent not Online (an empty list, then ConnectionLost) once, Patching once over two pending answers, each again after the other, and Ready last (step 3)" $p

        $p = New-Problems
        $want = @('describe', 'describe', 'dii', 'dii', 'dii', 'send', 'get', 'dii', 'send', 'get', 'dii', 'dii', 'send', 'get', 'dii', 'send', 'get')
        Add-CallProblems $p $calls $want
        Write-Problems "${sec}: The wait asks whether the agent is Online first, asks the patch check only once it is, and repeats until an answer other than pending; an empty list and a PingStatus other than Online are not Online (step 3; What up waits for)" $p

        $p = New-Problems
        $gaps = @(Get-Gaps $calls 'dii')
        if ($gaps.Count -ne 6) { $p.Add("The agent was checked $($gaps.Count + 1) times, not 7") }
        elseif (($gaps | Measure-Object -Minimum).Minimum -lt 3) { $p.Add('Two checks were less than 3 seconds apart: the 5-second pause is missing') }
        elseif (($gaps | Measure-Object -Maximum).Maximum -gt 30) { $p.Add('Two checks were more than 30 seconds apart') }
        Write-Problems "${sec}: The wait checks about every 5 seconds: each pause is a few seconds, never none (step 3; the timing is loose)" $p
      }

      # ---- The patch question's answers ----
      if (Get-Selected 'wait-answers') {
        $answers = @(
          @('Success', "patched`n", 'patched'),
          @('Success', "unknown`n", 'unknown'),
          @('Success', '', 'an empty answer'),
          @('Success', "no such answer`n", 'an unexpected answer'),
          @('Failed', '', 'a failed check with no output'),
          @('TimedOut', '', 'a timed-out check with no output')
        )
        foreach ($a in $answers) {
          Set-Scenario ($askedOnce + @{
              'ec2 describe-instances'     = @((New-Describe 'running'), (New-Describe 'running' (Get-LaunchTime 1)))
              'ssm get-command-invocation' = @(New-Invocation $a[0] $a[1])
            })
          $r = Invoke-Up
          $p = New-Problems
          Add-RunProblems $p $r 0
          Add-LinesProblems $p $r @($msgReady)
          Add-CallProblems $p (Get-AwsCalls) @('describe', 'describe', 'dii', 'send', 'get')
          Write-Problems "${secWait}: $($a[2]) ends the wait, with no Patching message and one question asked (Q6)" $p
        }
      }
      if (Get-Selected 'wait-invocation-polls') {
        Set-Scenario ($askedOnce + @{
            'ec2 describe-instances'     = @((New-Describe 'running'), (New-Describe 'running' (Get-LaunchTime 1)))
            'ssm get-command-invocation' = @(
              (New-Invocation 'Pending' ''), (New-Invocation 'InProgress' ''), (New-Invocation 'Delayed' ''),
              (New-Step '' 'An error occurred (InvocationDoesNotExist) when calling the GetCommandInvocation operation' 254),
              (New-Step '{}'), (New-Invocation 'Success' "patched`n"))
          })
        $r = Invoke-Up
        $calls = Get-AwsCalls
        $p = New-Problems
        Add-RunProblems $p $r 0
        Add-LinesProblems $p $r @($msgReady)
        Add-CallProblems $p $calls @('describe', 'describe', 'dii', 'send', 'get', 'get', 'get', 'get', 'get', 'get')
        Write-Problems "${secWait}: The invocation is polled again while its status is Pending, InProgress or Delayed, when the call fails, and when the answer has no Status, then its trimmed output is the answer (How the answer is read)" $p

        $p = New-Problems
        $gaps = @(Get-Gaps $calls 'get')
        if ($gaps.Count -ne 5) { $p.Add("The invocation was polled $($gaps.Count + 1) times, not 6") }
        elseif (($gaps | Measure-Object -Maximum).Maximum -gt 10) { $p.Add('Two polls were more than 10 seconds apart: the invocation is polled once a second') }
        Write-Problems "${secWait}: The invocation is polled once a second (How the answer is read; the timing is loose)" $p
      }
      if (Get-Selected 'wait-20-polls') {
        $inProgress = @(1..20 | ForEach-Object { New-Invocation 'InProgress' '' })
        Set-Scenario ($askedOnce + @{
            'ec2 describe-instances'     = @((New-Describe 'running'), (New-Describe 'running' (Get-LaunchTime 1)))
            'ssm get-command-invocation' = @($inProgress + @(New-Invocation 'Success' "patched`n"))
          })
        $r = Invoke-Up
        $p = New-Problems
        Add-RunProblems $p $r 0
        Add-LinesProblems $p $r @($msgPatching, $msgReady)
        $want = @('describe', 'describe', 'dii', 'send') + @(1..20 | ForEach-Object { 'get' }) + @('dii', 'send', 'get')
        Add-CallProblems $p (Get-AwsCalls) $want
        Write-Problems "${secWait}: 20 polls of the invocation without an end count as pending: the Patching message is printed, the box is asked again at the next check, and then it is ready (How the answer is read)" $p
      }

      # ---- The 15-minute deadline, waited in full ----
      # The deadline case runs only with -Case deadline, exactly (docs/conformance.md, Cases that run
      # only when asked for, architect, 2026-11-19, corrected): otherwise it writes no result, and a
      # -Case that would select it prints the one Not-run line. Its patch question goes through Session
      # Manager's Run Command (docs/operator.md, What up waits for, and Calls by command: ssm
      # send-command and ssm get-command-invocation; ssh and scp are tripwires).
      $deadlineName1 = "${sec}: A box that is never ready fails at the deadline with exactly agentspace: <id> was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch, exit 1, nothing on stdout (step 3)"
      if ((Get-Selected 'deadline') -and $Case -ine 'deadline') {
        Write-Host $deadlineNotRun
      } elseif (Get-Selected 'deadline') {
        # Online, and the box answers pending to every question: a patch run that never finishes.
        Set-Scenario ($askedOnce + @{
            'ec2 describe-instances'     = @((New-Describe 'running'), (New-Describe 'running' (Get-LaunchTime 1)))
            'ssm get-command-invocation' = @(New-Invocation 'Success' "pending`n")
          })
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $r = Invoke-Up $deadlineLimitSeconds
        $clock.Stop()
        $calls = Get-AwsCalls
        $lines = Get-Lines $r
        $p = New-Problems
        Add-RunProblems $p $r 1
        if ($lines.Count -eq 0 -or $lines[-1] -cne $msgDeadline) { $p.Add('The last line on stderr is not agentspace: <id> was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch') }
        if ($lines | Where-Object { $_ -like 'Ready:*' }) { $p.Add('Ready was printed') }
        Write-Problems $deadlineName1 $p

        $p = New-Problems
        if ($r.TimedOut) { $p.Add('It was still running after 17 minutes') }
        elseif ($clock.Elapsed.TotalSeconds -lt $deadlineMinSeconds) { $p.Add('It gave up in less than 14.5 minutes') }
        elseif ($clock.Elapsed.TotalSeconds -gt $deadlineMaxSeconds) { $p.Add('It gave up after more than 16.5 minutes') }
        Write-Problems "${sec}: The deadline is 15 minutes from the start of the wait, with the last pause cut short to end on it (step 3; the timing is loose)" $p

        $p = New-Problems
        $nDii = @($calls | Where-Object { $_.Symbol -ceq 'dii' }).Count
        if ($nDii -lt 50) { $p.Add("The agent was checked only $nDii times in 15 minutes, so the poll did not repeat about every 5 seconds") }
        $nBad = @($calls | Where-Object { $_.Symbol.StartsWith('?') }).Count
        if ($nBad -gt 0) { $p.Add("$nBad call(s) had arguments or an operation that Calls by command does not give") }
        $first = @($calls | Select-Object -First 2 | ForEach-Object { $_.Symbol }) -join ','
        if ($first -cne 'describe,describe') { $p.Add('The first two calls were not describe-instances twice') }
        $nPatching = @($lines | Where-Object { $_ -ceq $msgPatching }).Count
        if ($nPatching -ne 1) { $p.Add("The Patching message was printed $nPatching times, not once, over the whole wait") }
        Write-Problems "${sec}: Over the 15 minutes the poll repeats, every call is one of Calls by command, and the Patching message is printed once, not at every check (step 3)" $p
      }

      # ---- Nothing started ssh or scp: the patch question goes through Session Manager ----
      $p = New-Problems
      $sshCalls = Get-StandInCallCount -Log (Join-Path $standins 'ssh-tripwire-calls.log')
      $scpCalls = Get-StandInCallCount -Log (Join-Path $standins 'scp-tripwire-calls.log')
      if ($sshCalls -gt 0) { $p.Add('ssh was started, and the patch question is never asked over SSH') }
      if ($scpCalls -gt 0) { $p.Add('scp was started, and up copies no file') }
      Write-Problems "${secWait}: The patch question is asked through Session Manager's Run Command and never over SSH, and up starts no scp, in every case above" $p
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-start-paths.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

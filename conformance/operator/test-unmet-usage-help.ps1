#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's usage errors and help texts (docs/operator.md, "Command line",
# "The top level" and the "Help" block of each command). Card #0132 (E0.06-T04-S02), gap FB-25
# (docs/conformance.md, "Coverage of the operator and the keeper").
#
# EXPECTED TO FAIL AGAINST THE PROTOTYPE, which has no help of its own and reports each usage problem
# as its shell's own error record, not agentspace's message (docs/operator.md, "Command line", the
# note on where this came from, and "Requirements the prototype does not meet yet"). A rewrite MUST
# pass every result line. The result lines still say FAIL: nothing turns a FAIL into a pass.
#
# What it checks, each as its own result:
#   - Every usage error the "Command line" section gives, with its exact text on stderr, an empty
#     stdout and exit 1: no command, an unknown command (the whole word, never an abbreviation),
#     a second positional argument, an unknown parameter, a parameter given twice, a value that is
#     missing, a -GiB that is not a decimal integer in range, and a control character written as `?`.
#   - The order they are reported in when several apply: help first, then the arguments from left to
#     right, a missing command only once the whole line is read, a repeated name before a missing
#     value, then the two -Alias checks, then the home and the system folder.
#   - What is accepted, so that a refusal is not a blanket one: the command and the parameter names
#     without regard to case, -GiB's boundary values and its sign, and a value that looks like a
#     parameter.
#   - Every help block byte for byte (the top level and each of the seven commands), to stdout, with a
#     line feed after every line and no carriage return, exit 0, nothing on stderr; and every way the
#     spec says help is asked for: -h and --help in any case, before the command, after other
#     parameters, in a parameter's value position, with an unknown command or parameter, and beside
#     errors that help hides.
#   - The home and the system folder: USERPROFILE and SystemRoot unset or empty fail every command
#     with the spec's text, in the spec's order, and help and usage errors need neither.
#
# The help texts below are copied from docs/operator.md's "Help" blocks, the oracle, and never from a
# run of a build. A change to a block in the spec MUST be made here too.
#
# No case needs AWS. The three seam variables (AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD,
# AGENTSPACE_SCP_CMD) name tripwires, and every refusal and every help case asserts that no tripwire
# was ever called. The cases that show an argument is accepted call the AWS tripwire on purpose, which
# fails at once as the CLI does with no sign-in, and assert only that it was called.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. A failure reason never quotes the program's
# output: it names the line of the spec's text that differs.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/prototype/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-usage-help.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$ErrorActionPreference = 'Stop'

# Copied from docs/operator.md, the Help block of each section, by key: the top level, then each command.
$helpText = [ordered]@{
  'top' = @'
Usage: agentspace-operator <command> [-GiB <n>] [-Alias <alias>] [-Quiet] [--help]

Manages the agentspace box from the laptop: its stack, its power, its system
files, the SSH CA it trusts, and the localswim Bot token it uses.

Commands:
  deploy  Create or update the stack, then run sync
  up      Start the box if needed, wait until it has patched, push the Bot token
  sync    Check the CA, install the box's system files, CLAUDE.md and the
          localswim CLI, and push the Bot token
  ca      Create the agentspace SSH CA once; later, check it and print it
  down    Stop the box
  status  Show the box's state, type, disk and Session Manager reachability
  grow    Grow the box's disk by -GiB (default 2), and its filesystem with it

Parameters:
  -GiB <n>        For grow: the GiB to add (default 2)
  -Alias <alias>  For up: the SSH host alias the token push uses (default agentspace)
  -Quiet          Print no progress or warnings; failures and status still print
  -h, --help      Print this help, or a command's, and exit

The operator's own messages go to stderr; deploy, sync and grow pass other
programs' output through. up writes nothing to stdout, so it is safe in an SSH
ProxyCommand.

Exit codes:
  0  Success
  1  Failure or usage error; the message on stderr says which
'@
  'deploy' = @'
Usage: agentspace-operator deploy [-Quiet]

Creates or updates the CloudFormation stack 'agentbox' in us-west-2 from
its template, with the parameters in ~/.agentspace/stack-parameters.json
and your ~/.ssh/id_ed25519.pub, then runs sync.

Never change the template's ImageId on a live stack: it replaces the instance
and its disk.

Exit codes: 0 success, 1 failure.
'@
  'up' = @'
Usage: agentspace-operator up [-Alias <alias>] [-Quiet]

Starts the box if it is stopped, waits until its boot-time patch run has
finished, and sends the localswim Bot token and the laptop's time zone if
either has changed since the last confirmed send.

  -Alias <alias>  The SSH host alias the token push connects with
                  (default agentspace). Letters, digits, '.', '_' and '-',
                  not starting with '-'.

Writes nothing to stdout, so it is safe inside an SSH ProxyCommand.
A failed token push warns and still exits 0.

Exit codes: 0 the box is ready, 1 failure.
'@
  'sync' = @'
Usage: agentspace-operator sync [-Quiet]

Checks the agentspace SSH CA, then installs box/system and the CA's public key
on the box, copies ~/.claude/CLAUDE.md there, builds and installs the localswim
CLI from C:\Projects\localswim, and sends the localswim Bot token and the
laptop's time zone. Starts the box first if it is stopped.

Fails before touching the box if the CA is missing, half-made, mismatched or
readable by anyone else.

Exit codes: 0 success, 1 failure.
'@
  'ca' = @'
Usage: agentspace-operator ca [-Quiet]

Creates the agentspace SSH CA (ed25519, no passphrase) in
%USERPROFILE%\.agentspace\ca the first time, locked to the current user.
Later runs change nothing: they check the key pair and its access rules, and
print its paths and fingerprint.

A half-made, mismatched or loosened CA fails with what is wrong. ca never
repairs or overwrites a CA. Run it without "Run as administrator".

Exit codes: 0 the CA exists and is sound, 1 failure.
'@
  'down' = @'
Usage: agentspace-operator down [-Quiet]

Stops the box. Does not wait for it to stop. Open sessions on it end.
The next ssh agentspace, or any lease connection, starts it again.

Exit codes: 0 the stop was requested, 1 failure.
'@
  'status' = @'
Usage: agentspace-operator status [-Quiet]

Shows the instance's ID, state, type, root disk size and volume, and whether
Session Manager can reach it. Never starts the box. Prints to stderr.

Exit codes: 0 success, 1 failure.
'@
  'grow' = @'
Usage: agentspace-operator grow [-GiB <n>] [-Quiet]

Grows the box's root volume by -GiB (default 2) and, if the box is running,
its filesystem online; a stopped box grows its filesystem at next boot.
EBS volumes never shrink, and allow one change every 6 hours.

Exit codes: 0 success, 1 failure.
'@
}

$commands = 'deploy', 'up', 'sync', 'ca', 'down', 'status', 'grow'
$noCommand = 'agentspace: No command given. The commands are deploy, up, sync, ca, down, status and grow.'
$tab = [string][char]9
$lf = [string][char]10
$soh = [string][char]1

function Get-ExpectedHelp([string]$Which) { (($helpText[$Which] -replace "`r`n", "`n") + "`n") }

# A failure reason names the line of the spec's text that differs, and never quotes the program's output.
function Compare-Text([string]$What, [string]$Got, [string]$Want) {
  if ($Got -ceq $Want) { return $null }
  if ($Got.Contains("`r")) { return "$What holds a carriage return" }
  if ($Want.EndsWith("`n") -and -not $Got.EndsWith("`n")) { return "$What does not end in a line feed" }
  $g = $Got -split "`n"
  $w = $Want -split "`n"
  for ($i = 0; $i -lt [Math]::Max($g.Count, $w.Count); $i++) {
    $a = if ($i -lt $g.Count) { $g[$i] } else { $null }
    $b = if ($i -lt $w.Count) { $w[$i] } else { $null }
    if ($a -cne $b) {
      if ($null -eq $b) { return "$What has more lines than the spec's $($w.Count - 1)" }
      if ($null -eq $a) { return "$What stops before line $($i + 1), which is '$b'" }
      return "$What line $($i + 1) is not '$b'"
    }
  }
  "$What differs from the spec's text"
}

$h = New-ConformanceHome
try {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name 'operator.md#command-line: the usage errors and help texts' -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name 'operator.md#command-line: the usage errors and help texts' -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
  } else {
    $trip = Get-OperatorTripwireEnvironment -ConformanceHome $h
    $standins = Join-Path $h.Root 'standins'
    $tripLogs = 'aws-tripwire-calls.log', 'ssh-tripwire-calls.log', 'scp-tripwire-calls.log' | ForEach-Object { Join-Path $standins $_ }
    $utf8 = [Text.UTF8Encoding]::new($false)

    function Reset-Tripwires {
      foreach ($f in $tripLogs) {
        if (Test-Path -LiteralPath $f) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $f) -Force }
      }
    }
    function Get-TripwireCalls { [int](($tripLogs | ForEach-Object { Get-StandInCallCount -Log $_ } | Measure-Object -Sum).Sum) }
    function Get-AwsTripwireCalls { Get-StandInCallCount -Log $tripLogs[0] }

    # Runs the operator with the harness's environment and the three tripwires. $Unset names variables
    # to remove from the child (it does not go through the harness's override, which refuses USERPROFILE);
    # $Empty names variables to set to the empty string.
    function Invoke-Operator([string[]]$Arguments, [string[]]$Unset = @(), [string[]]$Empty = @()) {
      $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $h -Override $trip
      foreach ($n in $Unset) { $childEnv[$n] = $null }
      foreach ($n in $Empty) { $childEnv[$n] = '' }
      Start-ConformanceProcess -FilePath $program -Arguments $Arguments -ChildEnvironment $childEnv -TimeoutSeconds 60
    }
    function Write-Problems([string]$Name, $Problems) {
      if ($Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
      else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
    }
    # A usage error or another refusal: exit 1, exactly one message on stderr, nothing on stdout, no
    # program started.
    function Test-Refusal([string]$Name, [string[]]$Arguments, [string]$Message, [string[]]$Unset = @(), [string[]]$Empty = @()) {
      try {
        Reset-Tripwires
        $r = Invoke-Operator $Arguments -Unset $Unset -Empty $Empty
        $p = [System.Collections.Generic.List[string]]::new()
        if ($r.TimedOut) { $p.Add('It timed out') } elseif ($r.ExitCode -ne 1) { $p.Add("Exit $($r.ExitCode), not 1") }
        $x = Compare-Text 'Stdout' $r.Stdout ''
        if ($x) { $p.Add($x) }
        $x = Compare-Text 'Stderr' $r.Stderr ($Message + "`n")
        if ($x) { $p.Add($x) }
        if ($r.Stderr -match 'CategoryInfo|At line:|Exception|panicked') { $p.Add('Stderr shows a runtime error record') }
        if ((Get-TripwireCalls) -gt 0) { $p.Add('A program outside agentspace was started') }
        Write-Problems $Name $p
      } catch {
        $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
        Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
      }
    }
    # Help: exit 0, the block's bytes on stdout, nothing on stderr, no program started.
    function Test-Help([string]$Name, [string[]]$Arguments, [string]$Which, [string[]]$Unset = @(), [string[]]$Empty = @()) {
      try {
        Reset-Tripwires
        $r = Invoke-Operator $Arguments -Unset $Unset -Empty $Empty
        $p = [System.Collections.Generic.List[string]]::new()
        if ($r.TimedOut) { $p.Add('It timed out') } elseif ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
        $want = Get-ExpectedHelp $Which
        $bytesEqual = [System.Linq.Enumerable]::SequenceEqual([byte[]]$r.StdoutBytes, [byte[]]$utf8.GetBytes($want))
        if (-not $bytesEqual) {
          $x = Compare-Text 'Stdout' $r.Stdout $want
          $p.Add($(if ($x) { $x } else { 'Stdout differs from the spec''s help block in its bytes' }))
        }
        $x = Compare-Text 'Stderr' $r.Stderr ''
        if ($x) { $p.Add($x) }
        if ((Get-TripwireCalls) -gt 0) { $p.Add('A program outside agentspace was started') }
        Write-Problems $Name $p
      } catch {
        $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
        Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
      }
    }
    # An argument that is not a usage error: the command got past the command line and called the
    # AWS CLI, which here is a tripwire that fails at once. Only that is checked.
    $usagePatterns = '^agentspace: (No command given|Unknown command|Unexpected argument|Unknown parameter|-\w+ is given more than once|-\w+ needs a value|-GiB MUST be a whole number)'
    function Test-Accepted([string]$Name, [string[]]$Arguments) {
      try {
        Reset-Tripwires
        $r = Invoke-Operator $Arguments
        $p = [System.Collections.Generic.List[string]]::new()
        if ($r.TimedOut) { $p.Add('It timed out') }
        if ($r.Stderr -cmatch ('(?m)' + $usagePatterns)) { $p.Add('It was refused as a usage error') }
        if ((Get-AwsTripwireCalls) -eq 0) { $p.Add('It never reached the AWS CLI, so the command line was not accepted') }
        Write-Problems $Name $p
      } catch {
        $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
        Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
      }
    }
    function Show([string]$Text) { $Text.Replace($tab, '<TAB>').Replace($lf, '<LF>').Replace($soh, '<SOH>') }

    # =============== Usage errors, one each (Command line, the table) ===============
    $c = 'operator.md#command-line'
    $unknown = { param([string]$A) "agentspace: Unknown command '$A'. The commands are deploy, up, sync, ca, down, status and grow." }
    $cases = @(
      @{ N = 'no arguments is No command given'; A = @(); M = $noCommand }
      @{ N = '-Quiet alone is No command given (the command is found missing once the whole line is read)'; A = @('-Quiet'); M = $noCommand }
      @{ N = '-GiB 3 alone is No command given'; A = @('-GiB', '3'); M = $noCommand }
      @{ N = '-Alias x alone is No command given'; A = @('-Alias', 'x'); M = $noCommand }
      @{ N = 'an unknown command bogus'; A = @('bogus'); M = (& $unknown 'bogus') }
      @{ N = 'st is an unknown command, never an abbreviation of status'; A = @('st'); M = (& $unknown 'st') }
      @{ N = 'statu is an unknown command, never an abbreviation of status'; A = @('statu'); M = (& $unknown 'statu') }
      @{ N = 'upx is an unknown command'; A = @('upx'); M = (& $unknown 'upx') }
      @{ N = 'an unknown command keeps the case it was given'; A = @('BoGuS'); M = (& $unknown 'BoGuS') }
      @{ N = 'a control character in an unknown command is written as ?  (a tab)'; A = @("bo${tab}gus"); M = (& $unknown 'bo?gus') }
      @{ N = 'a control character in an unknown command is written as ?  (a line feed, so the message stays one line)'; A = @("bo${lf}gus"); M = (& $unknown 'bo?gus') }
      @{ N = 'a control character in an unknown command is written as ?  (U+0001)'; A = @("bo${soh}gus"); M = (& $unknown 'bo?gus') }
      @{ N = 'a second positional argument: up extra'; A = @('up', 'extra'); M = "agentspace: Unexpected argument 'extra'." }
      @{ N = 'a second positional argument that is itself a command: status up'; A = @('status', 'up'); M = "agentspace: Unexpected argument 'up'." }
      @{ N = 'a second positional argument before the parameters: down extra -Quiet'; A = @('down', 'extra', '-Quiet'); M = "agentspace: Unexpected argument 'extra'." }
      @{ N = 'a control character in an unexpected argument is written as ?'; A = @('up', "ex${tab}tra"); M = "agentspace: Unexpected argument 'ex?tra'." }
      @{ N = 'an unknown parameter: up -Foo'; A = @('up', '-Foo'); M = "agentspace: Unknown parameter '-Foo'." }
      @{ N = 'an unknown parameter with no command: -Foo'; A = @('-Foo'); M = "agentspace: Unknown parameter '-Foo'." }
      @{ N = 'an argument that starts with - and is not a value is a parameter, known or not: status -5'; A = @('status', '-5'); M = "agentspace: Unknown parameter '-5'." }
      @{ N = 'a lone - is an unknown parameter'; A = @('status', '-'); M = "agentspace: Unknown parameter '-'." }
      @{ N = 'a double-dash word other than --help is an unknown parameter: status --foo'; A = @('status', '--foo'); M = "agentspace: Unknown parameter '--foo'." }
      @{ N = 'an unknown parameter keeps its case: status -qUiEtly'; A = @('status', '-qUiEtly'); M = "agentspace: Unknown parameter '-qUiEtly'." }
      @{ N = 'a control character in an unknown parameter is written as ?'; A = @('status', "-Fo${tab}o"); M = "agentspace: Unknown parameter '-Fo?o'." }
      @{ N = '-Quiet given twice'; A = @('status', '-Quiet', '-Quiet'); M = 'agentspace: -Quiet is given more than once.' }
      @{ N = '-Quiet given twice in two cases is named as the spec spells it'; A = @('status', '-quiet', '-QUIET'); M = 'agentspace: -Quiet is given more than once.' }
      @{ N = '-GiB given twice, the second in another case'; A = @('status', '-GiB', '1', '-gib', '2'); M = 'agentspace: -GiB is given more than once.' }
      @{ N = '-Alias given twice'; A = @('up', '-Alias', 'a', '-ALIAS', 'b'); M = 'agentspace: -Alias is given more than once.' }
      @{ N = '-GiB as the last argument has no value'; A = @('status', '-GiB'); M = 'agentspace: -GiB needs a value.' }
      @{ N = '-Alias as the last argument has no value'; A = @('up', '-Alias'); M = 'agentspace: -Alias needs a value.' }
      @{ N = 'a missing value names the parameter as the spec spells it: -gib'; A = @('status', '-gib'); M = 'agentspace: -GiB needs a value.' }
      @{ N = 'a missing value names the parameter as the spec spells it: -aLiAs'; A = @('up', '-aLiAs'); M = 'agentspace: -Alias needs a value.' }
      @{ N = '-GiB with no value and no command'; A = @('-GiB'); M = 'agentspace: -GiB needs a value.' }
      @{ N = 'a -Quiet does not quiet a usage error'; A = @('status', '-Quiet', '-Foo'); M = "agentspace: Unknown parameter '-Foo'." }
    )
    foreach ($case in $cases) { Test-Refusal "${c}: $($case.N)" $case.A $case.M }

    # ---- -GiB's value: a decimal integer from -2147483648 to 2147483647, and nothing else ----
    $arabic = [string][char]0x0663
    $fullWidth = [string][char]0xFF11
    $badGiB = @(
      @('abc', 'abc'), @('two words', '1 2'), @('a double sign', '--1'), @('a sign with no digits', '+'), @('a lone minus', '-'),
      @('a plus then a minus', '+-1'), @('a trailing minus', '1-'), @('one past the largest', '2147483648'),
      @('one past the smallest', '-2147483649'), @('an underscore', '1_000'),
      @('an Arabic-Indic digit (not ASCII)', $arabic), @('a full-width digit (not ASCII)', $fullWidth),
      @('a very long number', ('9' * 40)), @('a parameter name, since a value is whatever it looks like', '-Alias'), @('another parameter name', '-Quiet'),
      @('a control character, written as ?', "a${tab}b"))
    foreach ($b in $badGiB) {
      $value = $b[1]
      $shown = Show $value
      $msgValue = ($value -replace '[\x00-\x1f\x7f]', '?')
      Test-Refusal "${c}: status -GiB with $($b[0]) ('$shown') is refused as not a whole number" @('status', '-GiB', $value) "agentspace: -GiB MUST be a whole number, not '$msgValue'."
    }
    Test-Refusal "${c}: every command checks -GiB's form, so even deploy -GiB abc is refused" @('deploy', '-GiB', 'abc') "agentspace: -GiB MUST be a whole number, not 'abc'."
    Test-Refusal "${c}: -Quiet does not hide a refused -GiB" @('down', '-Quiet', '-GiB', 'abc') "agentspace: -GiB MUST be a whole number, not 'abc'."

    # =============== The order they are reported in ===============
    $o = 'operator.md#command-line: order'
    $orderCases = @(
      @{ N = 'status -GiB abc -Alias bad! reports the -GiB message (the spec''s own example)'; A = @('status', '-GiB', 'abc', '-Alias', 'bad!'); M = "agentspace: -GiB MUST be a whole number, not 'abc'." }
      @{ N = 'a repeated name is found before a missing value: status -GiB 1 -GiB'; A = @('status', '-GiB', '1', '-GiB'); M = 'agentspace: -GiB is given more than once.' }
      @{ N = 'a repeated name is found before a missing value: up -Alias a -Alias'; A = @('up', '-Alias', 'a', '-Alias'); M = 'agentspace: -Alias is given more than once.' }
      @{ N = 'an earlier repeat wins over a later missing value: status -Quiet -Quiet -GiB'; A = @('status', '-Quiet', '-Quiet', '-GiB'); M = 'agentspace: -Quiet is given more than once.' }
      @{ N = 'arguments are read from left to right: -Foo before a bad -GiB'; A = @('status', '-Foo', '-GiB', 'abc'); M = "agentspace: Unknown parameter '-Foo'." }
      @{ N = 'arguments are read from left to right: a bad -GiB before -Foo'; A = @('status', '-GiB', 'abc', '-Foo'); M = "agentspace: -GiB MUST be a whole number, not 'abc'." }
      @{ N = 'arguments are read from left to right: -Foo before an unknown command'; A = @('-Foo', 'bogus'); M = "agentspace: Unknown parameter '-Foo'." }
      @{ N = 'arguments are read from left to right: an unknown command before -Foo'; A = @('bogus', '-Foo'); M = (& $unknown 'bogus') }
      @{ N = 'an unknown command comes before the unexpected argument after it'; A = @('bogus', 'extra'); M = (& $unknown 'bogus') }
      @{ N = 'the unexpected argument comes before an unknown word after it: up extra bogus'; A = @('up', 'extra', 'bogus'); M = "agentspace: Unexpected argument 'extra'." }
      @{ N = 'a known command with an unknown word: up bogus -GiB abc reports the word'; A = @('up', 'bogus', '-GiB', 'abc'); M = "agentspace: Unexpected argument 'bogus'." }
      @{ N = 'the first of several problems wins: status -GiB 1 -Foo -GiB 2'; A = @('status', '-GiB', '1', '-Foo', '-GiB', '2'); M = "agentspace: Unknown parameter '-Foo'." }
      @{ N = 'a missing command is found only once the whole line is read: -Quiet -Quiet'; A = @('-Quiet', '-Quiet'); M = 'agentspace: -Quiet is given more than once.' }
      @{ N = 'a missing command is found only once the whole line is read: -GiB abc'; A = @('-GiB', 'abc'); M = "agentspace: -GiB MUST be a whole number, not 'abc'." }
      @{ N = 'a missing command is found only once the whole line is read: -Foo'; A = @('-Quiet', '-Foo'); M = "agentspace: Unknown parameter '-Foo'." }
      @{ N = 'the usage errors come before the -Alias checks: status -Alias x -GiB abc'; A = @('status', '-Alias', 'x', '-GiB', 'abc'); M = "agentspace: -GiB MUST be a whole number, not 'abc'." }
      @{ N = 'the usage errors come before the -Alias checks: up -Alias bad! -Foo'; A = @('up', '-Alias', 'bad!', '-Foo'); M = "agentspace: Unknown parameter '-Foo'." }
      @{ N = 'the first -Alias check comes before the second: status -Alias bad! is refused as applying to up alone'; A = @('status', '-Alias', 'bad!'); M = 'agentspace: -Alias applies to up alone.' }
      @{ N = 'the argument after -Alias is its value, whatever it looks like: status -Alias -Fx'; A = @('status', '-Alias', '-Fx'); M = 'agentspace: -Alias applies to up alone.' }
      @{ N = 'the argument after -Alias is its value, whatever it looks like: up -Alias -Fx'; A = @('up', '-Alias', '-Fx'); M = "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'." }
      @{ N = 'the argument after -GiB is its value, whatever it looks like: -GiB -Alias'; A = @('status', '-GiB', '-Alias'); M = "agentspace: -GiB MUST be a whole number, not '-Alias'." }
      @{ N = 'a value is taken before a parameter is: -Alias -Quiet makes -Quiet the alias'; A = @('status', '-Alias', '-Quiet'); M = 'agentspace: -Alias applies to up alone.' }
      @{ N = 'an unknown command with an alias that is bad reports the command'; A = @('bogus', '-Alias', 'bad!'); M = (& $unknown 'bogus') }
    )
    foreach ($case in $orderCases) { Test-Refusal "${o}: $($case.N)" $case.A $case.M }
    # A bad -GiB value that comes before a repeated -GiB is checked as it is read: operator.md,
    # Command line, "the arguments read from left to right, the first usage error found among them
    # reported", so the value message wins over the later repeat.
    Test-Refusal "${o}: a bad -GiB value is found as it is read, before a later repeat: status -GiB abc -GiB 2" @('status', '-GiB', 'abc', '-GiB', '2') "agentspace: -GiB MUST be a whole number, not 'abc'."

    # =============== Accepted: the refusals above are not blanket ===============
    $a = 'operator.md#command-line: accepted'
    foreach ($case in @(
        @{ N = 'the command is matched without regard to case: STATUS'; A = @('STATUS') }
        @{ N = 'the command is matched without regard to case: Down'; A = @('Down') }
        @{ N = 'the parameter names are matched without regard to case: status -GIB 3 -qUiEt'; A = @('status', '-GIB', '3', '-qUiEt') }
        @{ N = '-GiB takes a negative value, whatever it looks like: status -GiB -1'; A = @('status', '-GiB', '-1') }
        @{ N = '-GiB takes a plus sign: status -GiB +5'; A = @('status', '-GiB', '+5') }
        @{ N = '-GiB takes leading zeros: status -GiB 007'; A = @('status', '-GiB', '007') }
        @{ N = '-GiB takes the largest value: 2147483647'; A = @('status', '-GiB', '2147483647') }
        @{ N = '-GiB takes the smallest value: -2147483648'; A = @('status', '-GiB', '-2147483648') }
        @{ N = 'the parameters come before the command: -Quiet -GiB 3 status'; A = @('-Quiet', '-GiB', '3', 'status') }
        @{ N = 'a parameter between the command and the rest: status -Quiet'; A = @('status', '-Quiet') }
        @{ N = 'up -Alias with an alias of letters, digits, dot, underscore and hyphen'; A = @('up', '-Alias', 'Lease-1.a_b') }
      )) {
      Test-Accepted "${a}: $($case.N)" $case.A
    }

    # =============== The home and the system folder (Command line) ===============
    $e = 'operator.md#command-line: the home and the system folder'
    $noHome = 'agentspace: USERPROFILE is not set, so there is no home folder.'
    $noSystem = "agentspace: SystemRoot is not set, so Windows' programs cannot be found."
    foreach ($cmd in $commands) {
      Test-Refusal "${e}: $cmd with USERPROFILE unset fails with the spec's text" @($cmd) $noHome -Unset @('USERPROFILE')
    }
    foreach ($cmd in $commands) {
      Test-Refusal "${e}: $cmd with SystemRoot unset fails with the spec's text" @($cmd) $noSystem -Unset @('SystemRoot')
    }
    Test-Refusal "${e}: USERPROFILE set to the empty string is not set" @('status') $noHome -Empty @('USERPROFILE')
    Test-Refusal "${e}: SystemRoot set to the empty string is not set" @('status') $noSystem -Empty @('SystemRoot')
    Test-Refusal "${e}: with both unset the USERPROFILE message comes first" @('status') $noHome -Unset @('USERPROFILE', 'SystemRoot')
    Test-Refusal "${e}: a usage error needs neither (bogus with both unset)" @('bogus') (& $unknown 'bogus') -Unset @('USERPROFILE', 'SystemRoot')
    Test-Refusal "${e}: the -Alias check needs neither, and comes before them (status -Alias x with both unset)" @('status', '-Alias', 'x') 'agentspace: -Alias applies to up alone.' -Unset @('USERPROFILE', 'SystemRoot')
    Test-Help "${e}: help needs neither (the top level's help with both unset)" @('--help') 'top' -Unset @('USERPROFILE', 'SystemRoot')
    Test-Help "${e}: a command's help needs neither (grow -h with both unset)" @('grow', '-h') 'grow' -Unset @('USERPROFILE', 'SystemRoot')

    # =============== Help: every block byte for byte ===============
    $sectionOf = @{ top = 'the-top-level'; deploy = 'deploy'; up = 'up'; sync = 'sync'; ca = 'ca'; down = 'down'; status = 'status'; grow = 'grow' }
    foreach ($flag in '-h', '--help', '-H', '--HELP', '--Help') {
      Test-Help "operator.md#the-top-level: the top level's help for $flag, byte for byte, on stdout with exit 0" @($flag) 'top'
    }
    foreach ($cmd in $commands) {
      foreach ($flag in '-h', '--help') {
        Test-Help "operator.md#$($sectionOf[$cmd]): $cmd $flag prints $cmd's help byte for byte, with a line feed after every line and no carriage return" @($cmd, $flag) $cmd
      }
    }
    Test-Help 'operator.md#command-line: the top level help ends with a line feed after its last line, and has no carriage return' @('-h') 'top'

    # ---- Every way the spec says help is asked for ----
    $hp = 'operator.md#command-line: help'
    $helpWays = @(
      @{ N = 'a command in capitals: STATUS -H'; A = @('STATUS', '-H'); W = 'status' }
      @{ N = 'a command in mixed case: Sync --HELP'; A = @('Sync', '--HELP'); W = 'sync' }
      @{ N = 'before the command: --help up'; A = @('--help', 'up'); W = 'up' }
      @{ N = 'before the command: -h grow'; A = @('-h', 'grow'); W = 'grow' }
      @{ N = 'after another parameter: sync -Alias x --help (the spec''s own example)'; A = @('sync', '-Alias', 'x', '--help'); W = 'sync' }
      @{ N = 'after a value: up -Alias x -h'; A = @('up', '-Alias', 'x', '-h'); W = 'up' }
      @{ N = 'after -Quiet: down -Quiet -h'; A = @('down', '-Quiet', '-h'); W = 'down' }
      @{ N = 'where a value is expected: -Alias -h up names up'; A = @('-Alias', '-h', 'up'); W = 'up' }
      @{ N = 'where a value is expected: -GiB -h status names status'; A = @('-GiB', '-h', 'status'); W = 'status' }
      @{ N = 'where a value is expected, and no command follows: status -GiB -h'; A = @('status', '-GiB', '-h'); W = 'status' }
      @{ N = 'a value never names a command: -Alias up -h prints the top level''s help'; A = @('-Alias', 'up', '-h'); W = 'top' }
      @{ N = 'a value never names a command: -GiB up -h prints the top level''s help'; A = @('-GiB', 'up', '-h'); W = 'top' }
      @{ N = 'an unknown parameter counts as a parameter: -Foo up -h prints up''s help'; A = @('-Foo', 'up', '-h'); W = 'up' }
      @{ N = 'an unknown parameter is never reported when help is asked for: down -Foo --help'; A = @('down', '-Foo', '--help'); W = 'down' }
      @{ N = 'an unknown command gets the top level''s help: bogus -h'; A = @('bogus', '-h'); W = 'top' }
      @{ N = 'an abbreviation is an unknown command: st --help prints the top level''s help'; A = @('st', '--help'); W = 'top' }
      @{ N = 'no command at all: -Quiet -h'; A = @('-Quiet', '-h'); W = 'top' }
      @{ N = 'help twice: -h -h'; A = @('-h', '-h'); W = 'top' }
      @{ N = 'both spellings: status -h --help'; A = @('status', '-h', '--help'); W = 'status' }
      @{ N = 'help hides a repeated name: -Quiet -Quiet -h'; A = @('-Quiet', '-Quiet', '-h'); W = 'top' }
      @{ N = 'help hides a bad -GiB and a bad alias: status -GiB abc -Alias bad! -h'; A = @('status', '-GiB', 'abc', '-Alias', 'bad!', '-h'); W = 'status' }
      @{ N = 'help hides an unexpected argument: up extra -h'; A = @('up', 'extra', '-h'); W = 'up' }
      @{ N = 'help hides a missing value: grow -GiB 1 -GiB --help'; A = @('grow', '-GiB', '1', '-GiB', '--help'); W = 'grow' }
      @{ N = 'help hides an -Alias given to a command that refuses it: ca -Alias x -h'; A = @('ca', '-Alias', 'x', '-h'); W = 'ca' }
      @{ N = 'help hides an unknown command beside a known one: deploy bogus -h'; A = @('deploy', 'bogus', '-h'); W = 'deploy' }
    )
    foreach ($case in $helpWays) { Test-Help "${hp}: $($case.N)" $case.A $case.W }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-usage-help.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
if ($global:ConformanceCounts['FAIL'] -gt 0) {
  Write-Host 'NOTE This file is expected to fail against the prototype (no help of its own, no usage messages); a rewrite MUST pass it.'
}
Exit-ConformanceRun

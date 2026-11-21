#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's command line for a host: -Host's and -All's forms, every usage
# error they add and the six host checks in their order, the home and system folder checks before the
# host is looked up, and a project with no record refused by every command but provision (docs/operator.md,
# "Command line", the host checks; "Which host a command acts on", "From the command line"; D49, D55).
# Card #0156 (E0.07-T04-S02), against the rows of operator.md's "Which checks show it".
#
# What it does: it runs the operator inside a throwaway home with stand-ins for `aws`, `ssh` and `scp`
# that log every call (conformance/operator/host-commands-common.ps1), and checks for each case the
# message on stderr exactly, the exit code, an empty stdout, that no AWS CLI, ssh or scp call was made,
# and that nothing was written for a host. The help texts are not repeated: test-unmet-usage-help.ps1
# checks every one byte for byte; the cases here only show which help a line with -Host in it asks for.
#
# Cases: -Host's value (its form, its normalization, whitespace, a control character, as the last
# argument, given twice, whatever it looks like); -InstanceType's form and its absence of a value;
# the host checks one to six, each alone and in order against the others, against -Alias's two checks
# and against the usage errors; the checks reading no file and starting nothing; the order of the host
# lookup after the home and system folder checks; every command but provision refused for a project
# with no record, with its message, and writing nothing; and -Host and -All accepted before the command.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. Every instance ID is made up, and a failure
# reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-commands-line.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$tab = [string][char]9
$lf = [string][char]10
$hostMsg = { param([string]$A) "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not '$A'." }
$typeMsg = { param([string]$A) "agentspace: -InstanceType MUST be an EC2 instance type such as m9g.xlarge, not '$A'." }
$noHost = { param([string]$P) "agentspace: Project '$P' has no host; provision -Host $P makes one." }
$m1 = 'agentspace: -Host does not apply to ca.'
$m2 = 'agentspace: -All applies to status, down and sync alone.'
$m3 = 'agentspace: -Host and -All cannot be given together.'
$m4 = 'agentspace: provision needs -Host <project>.'
$m5 = 'agentspace: -InstanceType applies to deploy alone.'
$m6 = 'agentspace: -InstanceType needs -Host <project> of a host that is not the box; the box is sized through stack-parameters.json.'
$aliasOnly = 'agentspace: -Alias applies to up alone.'
$aliasForm = "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'."

Invoke-HostFile -FileName 'test-unmet-host-commands-line.ps1' -Body {
  param($fx)
  # A refusal: exit 1, the message and nothing else on stderr, nothing on stdout, and no AWS CLI, ssh or
  # scp call, whatever the registry holds.
  function Test-Usage([string]$Name, [AllowEmptyString()][string[]]$Arguments, [string]$Message, [scriptblock]$Setup = {}, [hashtable]$Environment = @{}) {
    Test-HostCase $Name $Arguments @{ Exit = 1; Stderr = "$Message`n"; Calls = @() } -Setup $Setup -Environment $Environment
  }
  # The registry of the cases that look a host up: the box and one more host, so a project the cases
  # name is found or is not, never read as missing.
  $twoHosts = { Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA)) }
  $invalidRegistry = { Set-HostsFile ([Text.Encoding]::UTF8.GetBytes('not json {')) }

  # ============ -Host's value (Command line) ============
  $c = 'operator.md#command-line: -Host'
  Test-Usage "${c}: as the last argument it has no value" @('status', '-Host') 'agentspace: -Host needs a value.'
  Test-Usage "${c}: a missing value names the parameter as the spec spells it: -hOsT" @('status', '-hOsT') 'agentspace: -Host needs a value.'
  Test-Usage "${c}: -Host given twice, the second in another case" @('status', '-Host', 'x', '-host', 'y') 'agentspace: -Host is given more than once.'
  Test-Usage "${c}: a repeated name is found before a missing value: status -Host x -Host" @('status', '-Host', 'x', '-Host') 'agentspace: -Host is given more than once.'
  Test-Usage "${c}: -All given twice" @('status', '-All', '-all') 'agentspace: -All is given more than once.'
  Test-Usage "${c}: -All given twice is found before a missing -Host value: status -All -All -Host" @('status', '-All', '-All', '-Host') 'agentspace: -All is given more than once.'
  $badHosts = @(
    @('bad!', 'a character that is not a letter, a digit or a hyphen'),
    @('', 'an empty value'),
    @('-x', 'a value that starts with a hyphen, which is its value whatever it looks like'),
    @('x-', 'a value that ends with a hyphen'),
    @('a_b', 'an underscore'),
    @('a b', 'a space inside'),
    @('a.b', 'a dot'),
    @(('a' * 33), '33 characters, one past the longest'),
    @('-', 'a lone hyphen'),
    @('--help2', 'a double-dash word that is not help')
  )
  foreach ($b in $badHosts) {
    Test-Usage "${c}: $($b[1]) is refused with the -Host message ('$($b[0])')" @('status', '-Host', $b[0]) (& $hostMsg $b[0])
  }
  Test-Usage "${c}: a control character in the value is written as ? in the message (a tab)" @('status', '-Host', "a${tab}b") (& $hostMsg 'a?b')
  Test-Usage "${c}: a control character in the value is written as ? in the message (a line feed, so the message stays one line)" @('status', '-Host', "a${lf}b") (& $hostMsg 'a?b')
  Test-Usage "${c}: the value keeps the case it was given in the message" @('status', '-Host', 'Bad!') (& $hostMsg 'Bad!')
  Test-Usage "${c}: every command checks -Host's form, deploy included" @('deploy', '-Host', 'bad!') (& $hostMsg 'bad!')
  Test-Usage "${c}: every command checks -Host's form, grow included" @('grow', '-Host', 'bad!') (& $hostMsg 'bad!')
  Test-Usage "${c}: every command checks -Host's form, provision included" @('provision', '-Host', 'bad!') (& $hostMsg 'bad!')
  Test-Usage "${c}: every command checks -Host's form, ca included, before the host checks" @('ca', '-Host', 'bad!') (& $hostMsg 'bad!')

  # A form the lease CLI accepts is accepted here and normalized: the message that follows names the
  # normalized project, which no registry holds.
  $n = 'operator.md#command-line: -Host is normalized as the lease CLI normalizes a project'
  foreach ($case in @(
      @('Agentspace', 'agentspace', 'upper case'),
      @('AGENTSPACE', 'agentspace', 'all capitals'),
      @("  Agentspace`t ", 'agentspace', 'surrounding whitespace, a tab among it'),
      @('a', 'a', 'one letter'),
      @('9', '9', 'one digit'),
      @('a-b-c', 'a-b-c', 'inner hyphens'),
      @('9lives', '9lives', 'a digit first'),
      @(('x' * 32), ('x' * 32), '32 characters, the longest'),
      @(('A' * 32), ('a' * 32), '32 capitals')
    )) {
    Test-Usage "${n}: $($case[2]) names the project '$($case[1].Substring(0, [Math]::Min(12, $case[1].Length)))' and finds no record" @('status', '-Host', $case[0]) (& $noHost $case[1])
  }
  Test-HostCase "${n}: -Host Agentspace finds the record of 'agentspace', and so acts on it (down)" @('down', '-Host', 'Agentspace') @{
    Exit = 0; Stderr = "Stopping $script:IdA.`n"; Calls = @("ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdA") } -Setup $twoHosts

  # ============ -InstanceType's value ============
  $t = 'operator.md#command-line: -InstanceType'
  Test-Usage "${t}: as the last argument it has no value" @('deploy', '-Host', 'agentspace', '-InstanceType') 'agentspace: -InstanceType needs a value.' -Setup $twoHosts
  Test-Usage "${t}: given twice" @('deploy', '-Host', 'agentspace', '-InstanceType', 'm9g.xlarge', '-instancetype', 'm9g.2xlarge') 'agentspace: -InstanceType is given more than once.' -Setup $twoHosts
  Test-Usage "${t}: a repeated name is found before a missing value" @('deploy', '-InstanceType', 'm9g.xlarge', '-InstanceType') 'agentspace: -InstanceType is given more than once.'
  foreach ($bad in @('m9g', 'M9g.xlarge', 'm9G.xlarge', 'm9g.', '.xlarge', 'm9g.x_large', 'm9g..xlarge', 'a.b.c', 'm9g.xlarge ', ' m9g.xlarge', '-m9g.xlarge', 'm9g.-xlarge', '')) {
    Test-Usage "${t}: '$bad' is not an instance type's form" @('deploy', '-Host', 'agentspace', '-InstanceType', $bad) (& $typeMsg $bad) -Setup $twoHosts
  }
  Test-Usage "${t}: a control character in the value is written as ? (a line feed)" @('deploy', '-Host', 'agentspace', '-InstanceType', "m9g.xlarge${lf}") (& $typeMsg 'm9g.xlarge?') -Setup $twoHosts
  Test-Usage "${t}: the argument after -InstanceType is its value whatever it looks like: -Quiet" @('deploy', '-Host', 'agentspace', '-InstanceType', '-Quiet') (& $typeMsg '-Quiet') -Setup $twoHosts
  # The form is accepted: the next check, which is the applies-to-deploy-alone check, speaks instead.
  foreach ($ok in @('m9g.2xlarge', 'm9g.xlarge', 'm7i.metal-24xl', 't3.micro', 'a.b', '9.9', 'c6g.12xlarge')) {
    Test-Usage "${t}: '$ok' is an instance type's form (the next check refuses it for status)" @('status', '-InstanceType', $ok) $m5
  }
  Test-Usage "${t}: the form check comes before the host checks: ca -Host x -InstanceType bad" @('ca', '-Host', 'x', '-InstanceType', 'bad') (& $typeMsg 'bad')

  # ============ The six host checks, in their order ============
  $o = 'operator.md#command-line: the host checks'
  Test-Usage "${o}: 1, -Host with ca" @('ca', '-Host', 'agentspace') $m1
  Test-Usage "${o}: 1 comes before 2 and 3: ca -Host x -All" @('ca', '-Host', 'x', '-All') $m1
  Test-Usage "${o}: 1 comes before 5: ca -Host x -InstanceType m9g.xlarge" @('ca', '-Host', 'x', '-InstanceType', 'm9g.xlarge') $m1
  Test-Usage "${o}: 1, -Host swimcoach with ca is refused too" @('ca', '-Host', 'swimcoach') $m1
  foreach ($cmd in 'ca', 'up', 'grow', 'deploy', 'bootstrap', 'provision') {
    Test-Usage "${o}: 2, -All with $cmd" @($cmd, '-All') $m2
  }
  Test-Usage "${o}: 2 comes before 3: up -Host x -All" @('up', '-Host', 'x', '-All') $m2
  Test-Usage "${o}: 2 comes before 3: provision -Host x -All" @('provision', '-Host', 'x', '-All') $m2
  Test-Usage "${o}: 2 comes before 4: provision -All" @('provision', '-All') $m2
  Test-Usage "${o}: 2 comes before 5: deploy -All -InstanceType m9g.xlarge" @('deploy', '-All', '-InstanceType', 'm9g.xlarge') $m2
  foreach ($cmd in 'status', 'down', 'sync') {
    Test-Usage "${o}: 3, -Host and -All together with $cmd" @($cmd, '-Host', 'agentspace', '-All') $m3 -Setup $twoHosts
    Test-Usage "${o}: 3, -All before -Host with $cmd" @($cmd, '-All', '-Host', 'agentspace') $m3 -Setup $twoHosts
  }
  Test-Usage "${o}: 3 comes before 5: status -Host x -All -InstanceType m9g.xlarge" @('status', '-Host', 'x', '-All', '-InstanceType', 'm9g.xlarge') $m3
  Test-Usage "${o}: 4, provision with no -Host" @('provision') $m4
  Test-Usage "${o}: 4, provision -Quiet with no -Host" @('provision', '-Quiet') $m4
  Test-Usage "${o}: 4 comes before 5: provision -InstanceType m9g.xlarge" @('provision', '-InstanceType', 'm9g.xlarge') $m4
  foreach ($cmd in 'up', 'down', 'status', 'grow', 'bootstrap', 'sync', 'provision', 'ca') {
    $line = @($cmd, '-InstanceType', 'm9g.xlarge')
    if ($cmd -eq 'provision') { $line = @($cmd, '-Host', 'agentspace', '-InstanceType', 'm9g.xlarge') }
    Test-Usage "${o}: 5, -InstanceType with $cmd" $line $m5 -Setup $twoHosts
  }
  Test-Usage "${o}: 5 applies to a host that exists too: up -Host agentspace -InstanceType m9g.xlarge" @('up', '-Host', 'agentspace', '-InstanceType', 'm9g.xlarge') $m5 -Setup $twoHosts
  Test-Usage "${o}: 5 for sync -All -InstanceType (2 and 3 pass first)" @('sync', '-All', '-InstanceType', 'm9g.xlarge') $m5 -Setup $twoHosts
  Test-Usage "${o}: 6, deploy -InstanceType with no -Host" @('deploy', '-InstanceType', 'm9g.xlarge') $m6
  Test-Usage "${o}: 6, deploy -Host swimcoach -InstanceType" @('deploy', '-Host', 'swimcoach', '-InstanceType', 'm9g.xlarge') $m6
  Test-Usage "${o}: 6, -Host swimcoach is the box in any case or whitespace" @('deploy', '-Host', ' SwimCoach ', '-InstanceType', 'm9g.xlarge') $m6
  Test-Usage "${o}: 6, -InstanceType before -Host" @('deploy', '-InstanceType', 'm9g.xlarge', '-Host', 'swimcoach') $m6
  Test-Usage "${o}: 6, -Quiet does not hide it" @('deploy', '-Quiet', '-InstanceType', 'm9g.xlarge') $m6

  # ---- Against -Alias's checks and the usage errors ----
  $a = 'operator.md#command-line: the order of the checks'
  Test-Usage "${a}: the -Alias applies-to-up check comes before the host checks (the spec's own example: ca -Alias x -Host y)" @('ca', '-Alias', 'x', '-Host', 'y') $aliasOnly
  Test-Usage "${a}: the -Alias applies-to-up check comes before -All's: status -Alias x -All" @('status', '-Alias', 'x', '-All') $aliasOnly
  Test-Usage "${a}: the -Alias form check comes before the host checks: up -Alias bad! -All" @('up', '-Alias', 'bad!', '-All') $aliasForm
  Test-Usage "${a}: the -Alias form check comes before the host checks: up -Host x -All -Alias bad!" @('up', '-Host', 'x', '-All', '-Alias', 'bad!') $aliasForm
  Test-Usage "${a}: a usage error comes before the host checks: ca -Host x -Foo" @('ca', '-Host', 'x', '-Foo') "agentspace: Unknown parameter '-Foo'."
  Test-Usage "${a}: the spec's example: status -Host bad! -GiB abc reports the -Host message" @('status', '-Host', 'bad!', '-GiB', 'abc') (& $hostMsg 'bad!')
  Test-Usage "${a}: the arguments are read from left to right: status -GiB abc -Host bad! reports the -GiB message" @('status', '-GiB', 'abc', '-Host', 'bad!') "agentspace: -GiB MUST be a whole number, not 'abc'."
  Test-Usage "${a}: a bad -Host comes before a later unknown parameter" @('status', '-Host', 'bad!', '-Foo') (& $hostMsg 'bad!')
  Test-Usage "${a}: an unknown parameter before a bad -Host is reported first" @('status', '-Foo', '-Host', 'bad!') "agentspace: Unknown parameter '-Foo'."
  Test-Usage "${a}: a second positional argument is found before the host checks: ca -Host x extra" @('ca', '-Host', 'x', 'extra') "agentspace: Unexpected argument 'extra'."
  Test-Usage "${a}: a missing command is found only once the whole line is read: -Host x" @('-Host', 'x') 'agentspace: No command given. The commands are deploy, up, sync, bootstrap, provision, ca, down, status and grow.'
  Test-Usage "${a}: a missing command is found only once the whole line is read: -All" @('-All') 'agentspace: No command given. The commands are deploy, up, sync, bootstrap, provision, ca, down, status and grow.'
  Test-Usage "${a}: an unknown command is reported, whatever host parameters follow: bogus -Host x -All" @('bogus', '-Host', 'x', '-All') "agentspace: Unknown command 'bogus'. The commands are deploy, up, sync, bootstrap, provision, ca, down, status and grow."
  Test-Usage "${a}: -Quiet does not hide a host check's failure: status -Host x -All -Quiet" @('status', '-Host', 'x', '-All', '-Quiet') $m3

  # ---- The checks read no file and start nothing: a registry that is not valid, a missing stack
  # parameters file and no public key do not speak before them ----
  $nf = 'operator.md#command-line: the host checks read no file'
  foreach ($case in @(
      @('ca -Host x', @('ca', '-Host', 'x'), $m1),
      @('status -Host x -All', @('status', '-Host', 'x', '-All'), $m3),
      @('provision with no -Host', @('provision'), $m4),
      @('grow -All', @('grow', '-All'), $m2),
      @('deploy -InstanceType m9g.xlarge', @('deploy', '-InstanceType', 'm9g.xlarge'), $m6),
      @('status -InstanceType m9g.xlarge', @('status', '-InstanceType', 'm9g.xlarge'), $m5)
    )) {
    Test-Usage "${nf}: $($case[0]) with a host registry that is not valid reports the usage error, not the registry" $case[1] $case[2] -Setup $invalidRegistry
  }

  # ============ The home and system folder checks, then the host is looked up ============
  $l = 'operator.md#command-line: the host is looked up after the home and system folder checks'
  $noSystem = "agentspace: SystemRoot is not set, so Windows' programs cannot be found."
  foreach ($cmd in 'deploy', 'up', 'sync', 'bootstrap', 'down', 'status', 'grow') {
    Test-Usage "${l}: $cmd -Host with no record and SystemRoot unset reports the system folder first" @($cmd, '-Host', 'nobody') $noSystem -Setup $twoHosts -Environment @{ SystemRoot = $null }
  }
  Test-Usage "${l}: status -All with a registry that is not valid and SystemRoot unset reports the system folder first" @('status', '-All') $noSystem -Setup $invalidRegistry -Environment @{ SystemRoot = $null }
  Test-Usage "${l}: provision -Host x with SystemRoot unset reports the system folder" @('provision', '-Host', 'x') $noSystem -Environment @{ SystemRoot = $null }

  # ============ A project with no record, refused by every command but provision ============
  # After the home and system folder checks and before any program starts or any AWS call is made:
  # exit 1 with the message, and nothing written: no registry change, no host folder, no alias, no
  # operator folder, no lock held in a way that leaves a file the case did not make.
  $r = 'operator.md#from-the-command-line'
  $noHostCases = @(
    @('deploy', @('deploy', '-Host', 'nobody')),
    @('deploy with -InstanceType', @('deploy', '-Host', 'nobody', '-InstanceType', 'm9g.2xlarge')),
    @('up', @('up', '-Host', 'nobody')),
    @('up with -Alias', @('up', '-Host', 'nobody', '-Alias', 'lease-1')),
    @('sync', @('sync', '-Host', 'nobody')),
    @('bootstrap', @('bootstrap', '-Host', 'nobody')),
    @('down', @('down', '-Host', 'nobody')),
    @('status', @('status', '-Host', 'nobody')),
    @('grow', @('grow', '-Host', 'nobody', '-GiB', '4')),
    @('status -Quiet', @('status', '-Host', 'nobody', '-Quiet')),
    @('down -Quiet', @('down', '-Quiet', '-Host', 'nobody'))
  )
  $registryBefore = $null
  foreach ($case in $noHostCases) {
    $args2 = [string[]]$case[1]
    Test-HostCase "${r}: $($case[0]) -Host nobody fails with Project 'nobody' has no host, before any program starts or any AWS call, and writes nothing" $args2 @{
      Exit = 1; Stderr = ((& $noHost 'nobody') + "`n"); Calls = @()
      Check = {
        param($run, $started)
        $p = [System.Collections.Generic.List[string]]::new()
        if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))))) { $p.Add('hosts.json was changed') }
        foreach ($m in (Get-NothingSignedProblems)) { $p.Add($m) }
        if (Test-Path -LiteralPath $fx.OperatorDir) { $p.Add('An operator folder was made') }
        $p
      } } -Setup $twoHosts
  }
  Test-HostCase "${r}: with no host registry at all, a project that is not the box has no record, and no registry is created by the refusal" @('status', '-Host', 'agentspace') @{
    Exit = 1; Stderr = ((& $noHost 'agentspace') + "`n"); Calls = @()
    Check = { param($run, $started) if (Test-Path -LiteralPath $fx.HostsFile) { 'A hosts.json was written by a refusal' } } } -Setup {
    Remove-HomePath $fx.HostsFile
  }
  Test-HostCase "${r}: the refusal names the project as normalized, whatever case it was given" @('down', '-Host', 'NoBody') @{
    Exit = 1; Stderr = ((& $noHost 'nobody') + "`n"); Calls = @() } -Setup $twoHosts
  Test-HostCase "${r}: the box's project has a record in a valid registry, so -Host swimcoach is never refused (down)" @('down', '-Host', 'swimcoach') @{
    Exit = 0; Stderr = "Stopping $script:IdBox.`n"; Calls = @("ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdBox") } -Setup $twoHosts
  Test-HostCase "${r}: a record is acted on whatever its state: a creating host's down" @('down', '-Host', 'agentspace') @{
    Exit = 0; Stderr = "Stopping $script:IdA.`n"; Calls = @("ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdA") } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -InstanceId $script:IdA -LastAttempt '2026-01-01T00:00:00Z'))
  }
  Test-HostCase "${r}: a record is acted on whatever its state: a failed host's down" @('down', '-Host', 'agentspace') @{
    Exit = 0; Stderr = "Stopping $script:IdA.`n"; Calls = @("ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdA") } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'failed' -InstanceId $script:IdA -LastAttempt '2026-01-01T00:00:00Z'))
  }

  # ============ -Host and -All before the command, and between the other parameters ============
  $f = 'operator.md#command-line: accepted'
  Test-HostCase "${f}: -Host before the command: -Host agentspace down" @('-Host', 'agentspace', 'down') @{
    Exit = 0; Stderr = "Stopping $script:IdA.`n"; Calls = @("ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdA") } -Setup $twoHosts
  Test-HostCase "${f}: -Host after another parameter: down -Quiet -Host agentspace stops the host and prints nothing"@('down', '-Quiet', '-Host', 'agentspace') @{
    Exit = 0; Stderr = ''; Calls = @("ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdA") } -Setup $twoHosts
  Test-HostCase "${f}: the parameter names are matched without regard to case: down -hOsT agentspace" @('down', '-hOsT', 'agentspace') @{
    Exit = 0; Stderr = "Stopping $script:IdA.`n"; Calls = @("ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdA") } -Setup $twoHosts
  Test-HostCase "${f}: -All before the command: -All down stops every host with an instance ID, in the registry's order" @('-All', 'down') @{
    Exit = 0; Calls = @("ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdBox", "ec2 stop-instances --region us-west-2 --output json --instance-ids $script:IdA") } -Setup $twoHosts

  # ============ Which help a line with -Host in it asks for (Command line, "Help is checked first") ============
  $hp = 'operator.md#command-line: help with -Host'
  # The help cases compare stdout through their own check: the first line of the block the spec names.
  # (test-unmet-usage-help.ps1 compares every block byte for byte.)
  foreach ($case in @(
      @('where a value is expected: status -Host --help names status', @('status', '-Host', '--help'), 'Usage: agentspace-operator status [-Host <project> | -All] [-Quiet]'),
      @('where a value is expected: -Host -h up names up', @('-Host', '-h', 'up'), 'Usage: agentspace-operator up [-Host <project>] [-Alias <alias>] [-Quiet]'),
      @('a value never names a command: -Host up -h prints the top level''s help', @('-Host', 'up', '-h'), 'Usage: agentspace-operator <command> [-Host <project> | -All] [-GiB <n>] [-Alias <alias>] [-InstanceType <type>] [-Quiet] [--help]'),
      @('a value never names a command: -InstanceType deploy -h prints the top level''s help', @('-InstanceType', 'deploy', '-h'), 'Usage: agentspace-operator <command> [-Host <project> | -All] [-GiB <n>] [-Alias <alias>] [-InstanceType <type>] [-Quiet] [--help]'),
      @('where a value is expected: deploy -Host x -InstanceType -h names deploy', @('deploy', '-Host', 'x', '-InstanceType', '-h'), 'Usage: agentspace-operator deploy [-Host <project>] [-InstanceType <type>] [-Quiet]'),
      @('help hides a bad -Host: sync -Host bad! --help', @('sync', '-Host', 'bad!', '--help'), 'Usage: agentspace-operator sync [-Host <project> | -All] [-Quiet]'),
      @('help hides the host checks: ca -Host x --help', @('ca', '-Host', 'x', '--help'), 'Usage: agentspace-operator ca [-Quiet]'),
      @('help hides -All with -Host: down -Host x -All -h', @('down', '-Host', 'x', '-All', '-h'), 'Usage: agentspace-operator down [-Host <project> | -All] [-Quiet]'),
      @('help hides a project with no record: grow -Host nobody -h', @('grow', '-Host', 'nobody', '-h'), 'Usage: agentspace-operator grow [-Host <project>] [-GiB <n>] [-Quiet]')
    )) {
    $first = $case[2]
    Test-HostCase "${hp}: $($case[0])" ([string[]]$case[1]) @{
      Exit = 0; Calls = @(); Stderr = ''; StdoutAny = $true
      Check = { param($run, $started) if (-not $run.Stdout.StartsWith($first + "`n")) { 'Stdout does not start with the first line of the help the spec names' } } } -Setup $twoHosts
  }
}
Exit-ConformanceRun
